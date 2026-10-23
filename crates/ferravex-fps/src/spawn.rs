//! Stateless, opt-in authority spawn selection over authored Vec3 feet positions.
//!
//! All other present, living players are hostile for distance/exposure scoring;
//! dead and absent slots are ignored and the spawning ID is excluded. Distance
//! is explicitly horizontal XZ (even across floors). Optional occupancy rejects
//! overlapping player bounding volumes, including vertical overlap. Static
//! clearance uses the actual 3D capsule narrowphase, not the XZ projection.
//! Higher nearest-player horizontal distance squared minus visible-player count
//! times the exposure penalty wins; no other living players gives distance zero.
//! Ties always keep the lowest authored source index, independent of seed/roster.
//!
//! A policy tick assesses the living roster at tick entry, then reserves accepted
//! respawns in ascending player-ID order. It does not predict subsequent movement
//! into those volumes. LOS is full 3D eye-to-eye, uses Arena raycasts and implicit
//! ground-plane crossings, and ignores facing and player-body occlusion. All
//! candidates and possible reservation LOS pairs are preflighted before a tick;
//! incomplete queries are errors. No candidate means defer, with no fallback.
//!
//! Legacy Match stepping and replay remain unchanged. Use this separate replay
//! adapter for policy runs: its schema and immutable policy context distinguish
//! them from version-1 practice recordings. Checkpoints and event payloads reuse
//! existing formats. Captures require the caller to push exactly the commands and
//! accepted events of one policy step; playback verifies the resulting state.
//!
//! This recipe executes real movement ticks, records, plays and restarts them:
//! ```
//! use ferravex_fps::{Arena, Match, InputFrame, spawn::{Policy, Config, Capture, World}};
//! use ferravex_play::replay::{Limits, Playback};
//! use std::sync::Arc;
//! let policy = Policy::new(Config::default()).unwrap();
//! let mut game = Match::new(Arena::default());
//! assert!(game.spawn_player_with_policy(0, &policy).unwrap().is_some());
//! assert!(game.spawn_player_with_policy(1, &policy).unwrap().is_some());
//! let mut capture = Capture::new(&game, policy, 19, Limits::default()).unwrap();
//! for sequence in 0..64 {
//!     let commands = [(0, InputFrame { sequence, move_x: 1., ..InputFrame::default() })];
//!     let step = game.step_with_spawn_policy(&commands, &[], Some(&policy)).unwrap();
//!     capture.push(&game, &commands, &[], &step.events).unwrap();
//! }
//! assert_eq!(game.tick, 64);
//! let tape = Arc::new(capture.finish());
//! let mut world = World::new(Match::new(game.arena.clone()), policy).unwrap();
//! let mut playback = Playback::new(tape);
//! for _ in 0..2 {
//!     playback.reset();
//!     playback.start(&mut world).unwrap();
//!     while !playback.finished() { playback.step(&mut world).unwrap(); }
//!     assert_eq!(game.state_hash(), world.game.state_hash());
//! }
//! ```
use crate::{
    Arena, CombatRules, EYE_HEIGHT, GameEvent, InputFrame, MAX_PLAYERS, Match, PLAYER_HEIGHT,
    PLAYER_RADIUS, Player, PracticeRules, RewindStep, ShotTime, StepCounts, StepError, StepOutput,
    TICK_RATE, arena_authoring, checkpoint, replay,
};
use ferravex_math::{Vec2, Vec3};
use ferravex_physics::character::{CharacterShape, ClearanceError, check_clearance};
use ferravex_play::{
    gameplay::spawn::{SpawnAssessment, SpawnSelector},
    replay::{EventIter, Header, Limits, Recording, ReplayWorld},
};

/// This schema's version 1 fixes all scoring, hostility and tick-entry semantics.
pub const SCHEMA: u64 = 0x4658535041574e31;
/// Bounded complete LOS preflight: 16 spawns times (16 players + 16 spawns).
pub const MAX_LOS_TESTS: usize = MAX_PLAYERS * MAX_PLAYERS * 2;

/// World-unit settings; defaults only apply when explicitly opting into Policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Config {
    /// Finite 0..=8192; strictly smaller horizontal separations are excluded.
    pub minimum_horizontal_distance: f32,
    /// Reject overlap of open player AABBs (radius .3, height 1.8), on all axes.
    pub forbid_occupied: bool,
    /// None skips LOS. Some(finite 0..=1e8) subtracts this per visible player.
    pub exposure_penalty: Option<f32>,
    /// Actual character candidate budget per authored spawn, 0..=1_000_000.
    pub clearance_candidates: usize,
    /// Complete eye-ray count budget, 0..=MAX_LOS_TESTS; unused when LOS is None.
    pub max_los_tests: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            minimum_horizontal_distance: 3.,
            forbid_occupied: true,
            exposure_penalty: None,
            clearance_candidates: arena_authoring::MAX_SOLIDS + 1,
            max_los_tests: MAX_LOS_TESTS,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidConfig,
    InvalidPlayerId,
    PlayerUnavailable,
    PlayerCapacity,
    InvalidCommands,
    Arena(arena_authoring::ArenaAuthoringError),
    Checkpoint(checkpoint::CheckpointError),
    Clearance { index: usize, error: ClearanceError },
    LosBudget { required: usize, provided: usize },
    Replay(replay::Error),
    ContextChanged,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "authority spawn: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<checkpoint::CheckpointError> for Error {
    fn from(error: checkpoint::CheckpointError) -> Self {
        Self::Checkpoint(error)
    }
}
impl From<replay::Error> for Error {
    fn from(error: replay::Error) -> Self {
        Self::Replay(error)
    }
}
/// Eligible authored source identity and its tick-entry assessment; selection
/// alone does not admit an actor. Match join/respawn performs the acceptance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selection {
    pub index: usize,
    pub position: Vec3,
    pub score: f32,
    pub exposed_players: usize,
}
/// Validated immutable configuration, with no RNG/cursor/reset state. Recreating
/// it with the same Config gives the same selection on the same authority state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Policy {
    config: Config,
}
impl Policy {
    /// Validate all settings without clamping. See the module's runnable recipe.
    pub fn new(config: Config) -> Result<Self, Error> {
        if !config.minimum_horizontal_distance.is_finite()
            || !(0. ..=8192.).contains(&config.minimum_horizontal_distance)
            || config
                .exposure_penalty
                .is_some_and(|v| !v.is_finite() || !(0. ..=1e8).contains(&v))
            || config.clearance_candidates > 1_000_000
            || config.max_los_tests > MAX_LOS_TESTS
        {
            return Err(Error::InvalidConfig);
        }
        Ok(Self { config })
    }
    pub fn config(self) -> Config {
        self.config
    }
    /// Read-only bounded selection. Empty/all blocked return Ok(None). Every
    /// arena/player/candidate and query budget is validated even if an earlier
    /// candidate wins. Allocation-free; the result is not a reservation.
    ///
    /// ```
    /// use ferravex_fps::{Match, spawn::{Policy, Config}};
    /// let game = Match::default();
    /// let policy = Policy::new(Config::default()).unwrap();
    /// let before = game.state_hash().unwrap();
    /// let first = policy.select(&game, 0).unwrap().unwrap();
    /// assert_eq!(first.index, 0); // No living references: source-index tie.
    /// assert_eq!(policy.select(&game, 0).unwrap(), Some(first));
    /// assert_eq!(game.state_hash().unwrap(), before);
    /// assert_eq!(policy.context_hash(&game).unwrap(),
    ///     Policy::new(policy.config()).unwrap().context_hash(&game).unwrap());
    /// ```
    pub fn select(&self, game: &Match, actor: u8) -> Result<Option<Selection>, Error> {
        if actor as usize >= MAX_PLAYERS {
            return Err(Error::InvalidPlayerId);
        }
        Ok(Prepared::new(game, *self)?.choose(actor))
    }
    /// Context includes schema semantics, every validated config field (including
    /// query budgets), and the existing arena/game context. Not a state hash.
    pub fn context_hash(&self, game: &Match) -> Result<u64, Error> {
        validate_game(game)?;
        let mut bytes = [0u8; 42];
        bytes[..8].copy_from_slice(&SCHEMA.to_le_bytes());
        bytes[8..16].copy_from_slice(&game.context_hash()?.to_le_bytes());
        bytes[16..20].copy_from_slice(
            &self
                .config
                .minimum_horizontal_distance
                .to_bits()
                .to_le_bytes(),
        );
        bytes[20] = u8::from(self.config.forbid_occupied);
        bytes[21] = u8::from(self.config.exposure_penalty.is_some());
        bytes[22..26].copy_from_slice(
            &self
                .config
                .exposure_penalty
                .unwrap_or(0.)
                .to_bits()
                .to_le_bytes(),
        );
        bytes[26..34].copy_from_slice(&(self.config.clearance_candidates as u64).to_le_bytes());
        bytes[34..42].copy_from_slice(&(self.config.max_los_tests as u64).to_le_bytes());
        Ok(checkpoint::hash(&bytes))
    }
}
fn validate_game(game: &Match) -> Result<(), Error> {
    arena_authoring::validate_arena(
        &game.arena,
        arena_authoring::AuthoringOptions {
            empty_spawns: arena_authoring::EmptySpawnPolicy::LegacyOrigin,
            ..Default::default()
        },
    )
    .map_err(Error::Arena)?;
    if game.players.len() > MAX_PLAYERS {
        return Err(Error::PlayerCapacity);
    }
    // Reuse the existing full player/duplicate-ID/rewind validation, without
    // serializing into an allocating scratch buffer or changing checkpoint code.
    game.state_hash()?;
    Ok(())
}

/// All fallible geometry queries finish before Match mutates. Only fixed arrays;
/// references start as living players, then include accepted respawn reservations.
pub(crate) struct Prepared {
    policy: Policy,
    positions: [Vec3; MAX_PLAYERS],
    xz: [Vec2; MAX_PLAYERS],
    len: usize,
    clear: [bool; MAX_PLAYERS],
    living: [Option<Vec3>; MAX_PLAYERS],
    reserved: [Option<usize>; MAX_PLAYERS],
    visible_live: [[bool; MAX_PLAYERS]; MAX_PLAYERS],
    visible_spawn: [[bool; MAX_PLAYERS]; MAX_PLAYERS],
}
impl Prepared {
    pub(crate) fn new(game: &Match, policy: Policy) -> Result<Self, Error> {
        validate_game(game)?;
        let mut out = Self {
            policy,
            positions: [Vec3::ZERO; MAX_PLAYERS],
            xz: [Vec2::ZERO; MAX_PLAYERS],
            len: game.arena.spawns.len(),
            clear: [false; MAX_PLAYERS],
            living: [None; MAX_PLAYERS],
            reserved: [None; MAX_PLAYERS],
            visible_live: [[false; MAX_PLAYERS]; MAX_PLAYERS],
            visible_spawn: [[false; MAX_PLAYERS]; MAX_PLAYERS],
        };
        for p in &game.players {
            if p.is_alive() {
                out.living[p.id as usize] = Some(p.position);
            }
        }
        let live_count = out.living.iter().flatten().count();
        let required = out.len * (live_count + out.len);
        if policy.config.exposure_penalty.is_some() && required > policy.config.max_los_tests {
            return Err(Error::LosBudget {
                required,
                provided: policy.config.max_los_tests,
            });
        }
        for (i, &position) in game.arena.spawns.iter().enumerate() {
            out.positions[i] = position;
            out.xz[i] = Vec2::new(position.x, position.z);
            out.clear[i] = match check_clearance(
                &game.arena,
                CharacterShape::Capsule {
                    radius: PLAYER_RADIUS,
                    height: PLAYER_HEIGHT,
                },
                position,
                u32::MAX,
                policy.config.clearance_candidates,
            ) {
                Ok(_) => true,
                Err(ClearanceError::Blocked { .. }) => false,
                Err(error) => return Err(Error::Clearance { index: i, error }),
            };
            if policy.config.exposure_penalty.is_some() {
                for (id, target) in out.living.iter().enumerate() {
                    if let Some(target) = target {
                        out.visible_live[i][id] = visible(&game.arena, position, *target);
                    }
                }
                for (j, &target) in game.arena.spawns.iter().enumerate() {
                    out.visible_spawn[i][j] = visible(&game.arena, position, target);
                }
            }
        }
        Ok(out)
    }
    fn metrics(&self, index: usize, actor: u8) -> Option<(f32, usize)> {
        if !self.clear[index] {
            return None;
        }
        let position = self.positions[index];
        let mut nearest: Option<f32> = None;
        let mut exposure = 0;
        let minimum = self.policy.config.minimum_horizontal_distance;
        for (id, target) in self.living.iter().enumerate() {
            let Some(target) = target.filter(|_| id != actor as usize) else {
                continue;
            };
            let delta = position - target;
            let distance = self.xz[index].distance_squared(Vec2::new(target.x, target.z));
            if distance < minimum * minimum
                || (self.policy.config.forbid_occupied
                    && delta.x.abs() < 2. * PLAYER_RADIUS
                    && delta.z.abs() < 2. * PLAYER_RADIUS
                    && delta.y.abs() < PLAYER_HEIGHT)
            {
                return None;
            }
            nearest = Some(nearest.map_or(distance, |n| n.min(distance)));
            let seen = self.reserved[id].map_or(self.visible_live[index][id], |j| {
                self.visible_spawn[index][j]
            });
            exposure += usize::from(seen);
        }
        Some((
            nearest.unwrap_or(0.)
                - self.policy.config.exposure_penalty.unwrap_or(0.) * exposure as f32,
            exposure,
        ))
    }
    pub(crate) fn choose(&self, actor: u8) -> Option<Selection> {
        if self.len == 0 {
            return None;
        }
        // J250 handles stable source identity/ranking. A fresh nonrotating selector
        // uses only validated XZ coordinates and finite bounded scores; full Vec3
        // geometry has already been checked independently above.
        let selector = SpawnSelector::new(0.).expect("constant valid minimum");
        let mut output = [SpawnAssessment::EMPTY; MAX_PLAYERS];
        let index = selector
            .assess_into::<std::convert::Infallible>(
                &self.xz[..self.len],
                Vec2::ZERO,
                &mut output,
                |_| Ok(true),
                |i, _| Ok(self.metrics(i, actor).map(|m| m.0)),
                |_| Ok(true),
            )
            .expect("preflighted bounded geometry and scores")?;
        let (score, exposed_players) = self
            .metrics(index, actor)
            .expect("selected eligible candidate");
        Some(Selection {
            index,
            position: self.positions[index],
            score,
            exposed_players,
        })
    }
    pub(crate) fn respawn(&mut self, player: &mut Player) {
        player.respawn_ticks = player.respawn_ticks.saturating_sub(1);
        if player.respawn_ticks == 0
            && let Some(selected) = self.choose(player.id)
        {
            *player = Player::new(player.id, selected.position);
            self.living[player.id as usize] = Some(selected.position);
            self.reserved[player.id as usize] = Some(selected.index);
        }
    }
}
fn visible(arena: &Arena, from: Vec3, to: Vec3) -> bool {
    let from = from + Vec3::Y * EYE_HEIGHT;
    let to = to + Vec3::Y * EYE_HEIGHT;
    // The concrete Terrain's implicit ground is a plane at Y=0.
    if from.y * to.y <= 0. {
        return false;
    }
    let delta = to - from;
    let distance = delta.length();
    distance == 0. || arena.raycast(from, delta / distance, distance).is_none()
}
impl Match {
    /// Authority-only initial join, with no ID-modulo/origin fallback. Failure or
    /// deferral preserves players, tick and life generations. See module recipe.
    pub fn spawn_player_with_policy(
        &mut self,
        id: u8,
        policy: &Policy,
    ) -> Result<Option<Selection>, Error> {
        if id as usize >= MAX_PLAYERS {
            return Err(Error::InvalidPlayerId);
        }
        if self.player(id).is_some() {
            return Err(Error::PlayerUnavailable);
        }
        if self.players.len() >= MAX_PLAYERS {
            return Err(Error::PlayerCapacity);
        }
        let selection = policy.select(self, id)?;
        if let Some(selection) = selection {
            self.players.push(Player::new(id, selection.position));
            self.rewind.new_life(id);
        }
        Ok(selection)
    }
    /// Opt-in actual respawn stepping through the shared motor/combat/rewind
    /// kernel. None calls the exact legacy path. Invalid policy-world queries
    /// preserve the complete Match and do not invoke mutable CombatRules hooks.
    pub fn step_with_spawn_policy(
        &mut self,
        inputs: &[(u8, InputFrame)],
        shot_times: &[ShotTime],
        policy: Option<&Policy>,
    ) -> Result<RewindStep, Error> {
        self.step_with_rules_and_spawn_policy(inputs, shot_times, &mut PracticeRules, policy)
    }
    /// The same per-call policy with host-owned combat rules. Frozen, disabled
    /// auto-respawn and invalid weapon settings retain their existing gates.
    /// Inputs retain legacy sanitization; duplicate IDs/count overflow are errors
    /// on this checked path. Shot-time rejection remains in the shared kernel.
    pub fn step_with_rules_and_spawn_policy<R: CombatRules>(
        &mut self,
        inputs: &[(u8, InputFrame)],
        shot_times: &[ShotTime],
        rules: &mut R,
        policy: Option<&Policy>,
    ) -> Result<RewindStep, Error> {
        let Some(policy) = policy else {
            return Ok(self.step_with_rules(inputs, shot_times, rules));
        };
        let mut prepared = self.prepare_spawn_step(inputs, shot_times, policy)?;
        let mut output = RewindStep::default();
        self.step_kernel(
            inputs,
            shot_times,
            rules,
            Some(&mut prepared),
            &mut output,
            None,
        );
        Ok(output)
    }
    /// Checked retained-output equivalent of step_with_spawn_policy. None keeps
    /// legacy spawn behavior; Some retains policy validation, ID ordering,
    /// deferred respawns and accepted-only life changes. See StepOutput's bounds.
    ///
    /// ```
    /// use ferravex_fps::{Match, StepOutput, spawn::{Policy, Config}};
    /// let mut game = Match::default();
    /// let policy = Policy::new(Config::default()).unwrap();
    /// assert!(game.spawn_player_with_policy(0, &policy).unwrap().is_some());
    /// let mut output = StepOutput::new();
    /// let counts = game.step_with_spawn_policy_into(&[], &[], Some(&policy),
    ///     &mut output).unwrap();
    /// assert_eq!(game.tick, 1);
    /// assert_eq!(counts.events, output.events().len());
    /// ```
    pub fn step_with_spawn_policy_into(
        &mut self,
        inputs: &[(u8, InputFrame)],
        shot_times: &[ShotTime],
        policy: Option<&Policy>,
        output: &mut StepOutput,
    ) -> Result<StepCounts, StepError> {
        self.step_with_rules_and_spawn_policy_into(
            inputs,
            shot_times,
            &mut PracticeRules,
            policy,
            output,
        )
    }
    /// Retained-output equivalent of step_with_rules_and_spawn_policy. Capacity,
    /// complete state and policy preflight finish before any output, capture,
    /// rule callback or tick mutation. Policy errors use StepError::Spawn.
    pub fn step_with_rules_and_spawn_policy_into<R: CombatRules>(
        &mut self,
        inputs: &[(u8, InputFrame)],
        shot_times: &[ShotTime],
        rules: &mut R,
        policy: Option<&Policy>,
        output: &mut StepOutput,
    ) -> Result<StepCounts, StepError> {
        let Some(policy) = policy else {
            return self.step_with_rules_into(inputs, shot_times, rules, output);
        };
        self.preflight_step_output(output)?;
        let mut prepared = self
            .prepare_spawn_step(inputs, shot_times, policy)
            .map_err(StepError::Spawn)?;
        Ok(self.step_kernel(
            inputs,
            shot_times,
            rules,
            Some(&mut prepared),
            &mut output.inner,
            Some(&mut output.transitions),
        ))
    }
    // Shared fallible command/policy preparation for both output wrappers.
    fn prepare_spawn_step(
        &self,
        inputs: &[(u8, InputFrame)],
        shot_times: &[ShotTime],
        policy: &Policy,
    ) -> Result<Prepared, Error> {
        if inputs.len() > MAX_PLAYERS || shot_times.len() > MAX_PLAYERS {
            return Err(Error::InvalidCommands);
        }
        let mut ids = 0u16;
        for &(id, _) in inputs {
            if id as usize >= MAX_PLAYERS || ids & (1 << id) != 0 {
                return Err(Error::InvalidCommands);
            }
            ids |= 1 << id;
        }
        ids = 0;
        for shot in shot_times {
            if shot.player as usize >= MAX_PLAYERS || ids & (1 << shot.player) != 0 {
                return Err(Error::InvalidCommands);
            }
            ids |= 1 << shot.player;
        }
        Prepared::new(self, *policy)
    }
}

/// Policy recording with immutable context; never use legacy replay::Capture
/// for these ticks. The shared recorder reserves its scratch storage up front.
pub struct Capture {
    inner: replay::Capture,
    policy: Policy,
    context: u64,
}
impl Capture {
    pub fn new(game: &Match, policy: Policy, seed: u64, limits: Limits) -> Result<Self, Error> {
        Prepared::new(game, policy)?;
        let context = policy.context_hash(game)?;
        let inner = replay::Capture::with_header(
            game,
            Header {
                schema: SCHEMA,
                version: 1,
                context,
                tick_hz: TICK_RATE as u16,
                seed,
            },
            limits,
        )?;
        Ok(Self {
            inner,
            policy,
            context,
        })
    }
    pub fn tick_count(&self) -> usize {
        self.inner.tick_count()
    }
    /// Push exactly one accepted policy tick. Changed arena/context is rejected
    /// before appending; a reported failure preserves the recorded prefix.
    pub fn push(
        &mut self,
        game: &Match,
        commands: &[(u8, InputFrame)],
        shots: &[ShotTime],
        events: &[GameEvent],
    ) -> Result<(), Error> {
        if self.policy.context_hash(game)? != self.context {
            return Err(Error::ContextChanged);
        }
        Ok(self.inner.push(game, commands, shots, events)?)
    }
    pub fn finish(self) -> Recording {
        self.inner.finish()
    }
}
/// Replay authority with immutable policy configuration. External edits to game
/// are still subject to context validation and per-tick preflight. PracticeRules
/// is the replay combat policy; arbitrary custom CombatRules are not captured.
pub struct World {
    pub game: Match,
    pub events: Vec<GameEvent>,
    policy: Policy,
}
impl World {
    pub fn new(game: Match, policy: Policy) -> Result<Self, Error> {
        Prepared::new(&game, policy)?;
        Ok(Self {
            game,
            events: Vec::with_capacity(64),
            policy,
        })
    }
    pub fn policy(&self) -> Policy {
        self.policy
    }
}
impl ReplayWorld for World {
    type Error = Error;
    const SCHEMA: u64 = SCHEMA;
    const VERSION: u32 = 1;
    const TICK_HZ: u16 = TICK_RATE as u16;
    fn context(&self) -> Result<u64, Error> {
        self.policy.context_hash(&self.game)
    }
    fn restore(&mut self, state: &[u8]) -> Result<(), Error> {
        let game = Match::from_checkpoint(state)?;
        if self.policy.context_hash(&game)? != self.policy.context_hash(&self.game)? {
            return Err(Error::ContextChanged);
        }
        Prepared::new(&game, self.policy)?;
        self.game = game;
        self.events.clear();
        Ok(())
    }
    fn state_hash(&self) -> Result<u64, Error> {
        Ok(self.game.state_hash()?)
    }
    fn step(&mut self, input: &[u8], expected: EventIter<'_>) -> Result<(), Error> {
        let input = replay::decode_inputs(input)?;
        let wanted = replay::decode_events(expected.clone(), self.game.tick.wrapping_add(1))?;
        let step = self.game.step_with_spawn_policy(
            &input.commands[..input.command_count],
            &input.shots[..input.shot_count],
            Some(&self.policy),
        )?;
        self.events.clear();
        self.events.extend(step.events);
        Ok(replay::compare_events(
            &self.events,
            &wanted,
            expected.len(),
        )?)
    }
}
