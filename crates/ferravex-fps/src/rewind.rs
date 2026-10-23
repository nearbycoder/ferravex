//! Bounded server-authoritative pose rewind. The arena is always queried now.
use super::*;
mod checkpoint;
use std::collections::VecDeque;

/// 250 ms at 64 Hz. Client claims cannot extend the authority's rewind window.
pub const MAX_REWIND_TICKS: u32 = 16;

/// Controller continuity on an accepted authority relocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelocationMotion {
    /// Zero velocity, clear grounded and reset all jump timers/held state.
    /// Clearance does not establish support; the next actual motor tick does.
    Reset,
    /// Retain velocity, grounded, coyote/buffer timers and jump-held state exactly.
    /// This deliberately permits the previous controller's jump continuity at
    /// the new pose; it does not promise future swept clearance.
    Preserve,
}
/// Dynamic occupancy is separate from exact static Arena capsule clearance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelocationOccupancy {
    /// Check only Arena terrain. Other players may share the destination.
    StaticOnly,
    /// Reject open axis-aligned player volume overlap with every other living
    /// actor. Touching is allowed; dead/absent actors and self are ignored.
    /// This is conservative AABB occupancy, not inter-capsule narrowphase.
    ForbidLivingAabb,
}
/// Per-call authority options. Defaults reset motion, forbid living occupancy
/// and allow ground plus all 256 bounded Arena solids as clearance candidates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelocationOptions {
    pub motion: RelocationMotion,
    pub occupancy: RelocationOccupancy,
    /// Complete static-query budget in 0..=1_000_000, including implicit ground.
    pub max_candidates: usize,
}
impl Default for RelocationOptions {
    fn default() -> Self {
        Self {
            motion: RelocationMotion::Reset,
            occupancy: RelocationOccupancy::ForbidLivingAabb,
            max_candidates: arena_authoring::MAX_SOLIDS + 1,
        }
    }
}
/// Accepted feet/aim pose and discontinuity identity for presentation handoff.
/// Tick is the current authority tick (relocation does not execute a tick).
/// Life is the full checked u64 generation, shared with rewind and StepTransition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Relocation {
    pub player: u8,
    pub position: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub tick: u32,
    pub life: u64,
}
/// Every rejection leaves all players, tick, rewind and caller output untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelocationError {
    InvalidPlayerId,
    UnknownPlayer,
    PlayerDead,
    InvalidDestination,
    InvalidOptions,
    InvalidState(crate::checkpoint::CheckpointError),
    Arena(arena_authoring::ArenaAuthoringError),
    Clearance(ferravex_physics::character::ClearanceError),
    Occupied {
        player: u8,
    },
    /// No wrapping/aliasing or global history clear: start a new host session.
    GenerationExhausted,
}
impl std::fmt::Display for RelocationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "authority relocation: {self:?}")
    }
}
impl std::error::Error for RelocationError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShotTime {
    pub player: u8,
    pub tick: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RewindError {
    UnknownPlayer,
    ShooterDead,
    InvalidAim,
    FutureTick,
    TooOld,
    HistoryUnavailable,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotTrace {
    pub origin: Vec3,
    pub end: Vec3,
    pub victim: Option<u8>,
    pub headshot: bool,
}
#[derive(Debug, Default)]
pub struct RewindStep {
    pub events: Vec<GameEvent>,
    /// Invalid shot times suppress firing while movement and weapon timers advance.
    pub rejected_shots: Vec<(u8, RewindError)>,
}
/// Fixed conservative event capacity for a checked authoritative tick.
/// Each of at most MAX_PLAYERS actors either respawns (one event), fires (at
/// most Shot + Hit + Killed), or does neither. Respawning cannot fire that tick,
/// so R + 3*S <= 3*MAX_PLAYERS even when respawns are subsequently killed.
pub const MAX_STEP_EVENTS: usize = 3 * MAX_PLAYERS;
/// At most one invalid shot time is reported per unique roster actor per tick.
pub const MAX_STEP_REJECTIONS: usize = MAX_PLAYERS;

/// Legacy event/rejection lengths, or capacities from StepOutput::capacities.
/// The additive inline sidecar is counted by StepOutput::transition_count, with
/// fixed capacity StepOutput::transition_capacity; this struct stays compatible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepCounts {
    pub events: usize,
    pub rejected_shots: usize,
}
impl StepCounts {
    pub(crate) fn of(output: &RewindStep) -> Self {
        Self {
            events: output.events.len(),
            rejected_shots: output.rejected_shots.len(),
        }
    }
}

/// Preflight failures preserve the entire Match, previous output and capacities,
/// and invoke no CombatRules hooks. Callback panics/side effects and allocation
/// failure are outside rollback. Invalid *shot times* instead remain accepted
/// tick results in StepOutput::rejected_shots, exactly as in RewindStep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepError {
    OutputCapacity {
        required: StepCounts,
        provided: StepCounts,
    },
    /// Existing checkpoint bounds on arena, unique roster and rewind history.
    InvalidState(crate::checkpoint::CheckpointError),
    /// Only explicit safe-spawn stepping can produce this error.
    Spawn(crate::spawn::Error),
}
impl std::fmt::Display for StepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "authority step: {self:?}")
    }
}
impl std::error::Error for StepError {}

/// Caller-owned reusable authoritative tick output. Default/new reserve the
/// fixed worst case: MAX_STEP_EVENTS and MAX_STEP_REJECTIONS, even for an empty
/// roster. Checked stepping never grows these buffers or truncates events.
/// A separate inline sidecar retains at most MAX_STEP_TRANSITIONS records, one
/// aggregate per actor. Its capacity cannot be undersized and needs no allocation.
/// See PlayerStepResult for confirmed versus speculative feedback ownership.
/// Construction may allocate; clear and accepted warmed stepping retain capacity.
/// CombatRules implementations retain responsibility for their own allocations.
///
/// The checked path accepts checkpoint-valid state (including a unique bounded
/// roster and valid rewind history); legacy allocating wrappers still accept
/// their original raw state and preserve their original behavior. Input lookup,
/// first-wins duplicates, sanitization and invalid-shot suppression are unchanged
/// on the legacy spawn path. Safe-spawn calls retain their stricter preflight.
///
/// This actual authority/replay consumer records complete accepted events and
/// replays the resulting recording twice. Keep one output per authority and
/// deliver its borrowed slices before the next accepted tick overwrites them.
/// ```
/// use ferravex_fps::{Arena, Match, InputFrame, StepOutput, BUTTON_FIRE, replay};
/// use ferravex_play::replay::{Limits, Playback};
/// use std::sync::Arc;
/// let mut game = Match::new(Arena::default());
/// assert!(game.spawn_player(0));
/// assert!(game.spawn_player(1));
/// let mut output = StepOutput::new();
/// let capacities = output.capacities();
/// let mut capture = replay::Capture::new(&game, 19, Limits::default()).unwrap();
/// for sequence in 0..64 {
///     let commands = [(0, InputFrame { sequence, buttons: BUTTON_FIRE,
///         ..InputFrame::default() })];
///     let counts = game.step_with_rewind_into(&commands, &[], &mut output).unwrap();
///     assert_eq!(counts.events, output.events().len());
///     assert!(output.rejected_shots().is_empty());
///     capture.push(&game, &commands, &[], output.events()).unwrap();
/// }
/// let recording = Arc::new(capture.finish());
/// let mut playback = Playback::new(recording);
/// let mut world = replay::World::new(Match::new(game.arena.clone()));
/// for _ in 0..2 {
///     playback.reset();
///     playback.start(&mut world).unwrap();
///     while !playback.finished() { playback.step(&mut world).unwrap(); }
///     assert_eq!(game.state_hash(), world.game.state_hash());
/// }
/// output.clear(); // End-of-session/reset: Match lifecycle remains host-owned.
/// assert!(output.events().is_empty());
/// assert_eq!(output.capacities(), capacities);
/// ```
#[derive(Debug)]
pub struct StepOutput {
    pub(crate) inner: RewindStep,
    pub(crate) transitions: combat::TransitionBuffer,
}
impl Default for StepOutput {
    fn default() -> Self {
        Self::new()
    }
}
impl StepOutput {
    /// Prepare full tick capacity once; see the runnable type example.
    pub fn new() -> Self {
        Self::with_capacity(MAX_STEP_EVENTS, MAX_STEP_REJECTIONS)
    }
    /// Explicit storage preparation. Insufficient actual capacity rejects every
    /// checked step before mutation, even if that tick would produce no events.
    pub fn with_capacity(events: usize, rejected_shots: usize) -> Self {
        Self {
            transitions: combat::TransitionBuffer::new(),
            inner: RewindStep {
                events: Vec::with_capacity(events),
                rejected_shots: Vec::with_capacity(rejected_shots),
            },
        }
    }
    /// Complete accepted events, in the existing kernel's order.
    pub fn events(&self) -> &[GameEvent] {
        &self.inner.events
    }
    /// Invalid shot times, in roster order (ID order for explicit safe spawn).
    pub fn rejected_shots(&self) -> &[(u8, RewindError)] {
        &self.inner.rejected_shots
    }
    /// Nonempty accepted outcomes in roster order (ID order for safe spawn).
    /// The tick and full life generation come from the authority kernel.
    /// Consume before the next accepted step or clear. Rejected preflight keeps
    /// the previous slice, just like events/rejections; do not dispatch it again.
    pub fn transitions(&self) -> &[StepTransition] {
        self.transitions.as_slice()
    }
    /// Number of aggregate records in the last accepted output.
    pub fn transition_count(&self) -> usize {
        self.transitions().len()
    }
    /// Fixed inline record capacity, unaffected by with_capacity or clear.
    pub fn transition_capacity(&self) -> usize {
        MAX_STEP_TRANSITIONS
    }
    /// Actual retained allocation capacities, independent of previous lengths.
    pub fn capacities(&self) -> StepCounts {
        StepCounts {
            events: self.inner.events.capacity(),
            rejected_shots: self.inner.rejected_shots.capacity(),
        }
    }
    /// Discard all previous results while retaining allocations and inline storage.
    pub fn clear(&mut self) {
        self.transitions.clear();
        self.inner.events.clear();
        self.inner.rejected_shots.clear();
    }
}
#[derive(Clone, Copy)]
struct Pose {
    position: Vec3,
    alive: bool,
    life: u64,
}
struct Frame {
    tick: u32,
    players: [Option<Pose>; MAX_PLAYERS],
}
pub(crate) struct RewindHistory {
    frames: VecDeque<Frame>,
    lives: [u64; MAX_PLAYERS],
}
impl Default for RewindHistory {
    fn default() -> Self {
        Self {
            frames: VecDeque::with_capacity(MAX_REWIND_TICKS as usize + 1),
            lives: [0; MAX_PLAYERS],
        }
    }
}
impl RewindHistory {
    pub(crate) fn clear_frames(&mut self) {
        self.frames.clear();
    }
    pub(crate) fn life(&self, id: u8) -> u64 {
        self.lives.get(id as usize).copied().unwrap_or(0)
    }
    pub(crate) fn new_life(&mut self, id: u8) {
        if let Some(life) = self.lives.get_mut(id as usize) {
            // Exhaustion is unreachable at game timescales; clear history rather
            // than ever aliasing an earlier generation if it does happen.
            if *life == u64::MAX {
                self.frames.clear();
                *life = 0;
            }
            *life += 1;
        }
    }
    // Caller preflights state and checked generation before any mutation. The
    // same-tick frame may already exist: replace ONLY this actor's pose, leaving
    // every other historical pose and life intact. A missing current frame is
    // captured normally, with the same bounded ring and no extra allocation.
    fn relocate(&mut self, tick: u32, players: &[Player], id: u8, life: u64) {
        self.lives[id as usize] = life;
        self.capture(tick, players);
        let player = players.iter().find(|p| p.id == id).expect("checked actor");
        self.frames
            .back_mut()
            .expect("captured current frame")
            .players[id as usize] = Some(Pose {
            position: player.position,
            alive: player.is_alive(),
            life,
        });
    }
    pub(crate) fn capture(&mut self, tick: u32, players: &[Player]) {
        if self.frames.back().is_some_and(|f| f.tick == tick) {
            return;
        }
        if self.frames.len() == MAX_REWIND_TICKS as usize + 1 {
            self.frames.pop_front();
        }
        let mut poses = [None; MAX_PLAYERS];
        for player in players {
            if let Some(pose) = poses.get_mut(player.id as usize) {
                *pose = Some(Pose {
                    position: player.position,
                    alive: player.is_alive(),
                    life: self.lives[player.id as usize],
                });
            }
        }
        self.frames.push_back(Frame {
            tick,
            players: poses,
        });
    }
    pub(crate) fn validate(&self, now: u32, tick: u32) -> Result<(), RewindError> {
        if ferravex_net::sequence_newer(tick, now) || now.wrapping_sub(tick) == 1 << 31 {
            return Err(RewindError::FutureTick);
        }
        if now.wrapping_sub(tick) > MAX_REWIND_TICKS {
            return Err(RewindError::TooOld);
        }
        if !self.frames.iter().any(|f| f.tick == tick) {
            return Err(RewindError::HistoryUnavailable);
        }
        Ok(())
    }
    fn pose(&self, tick: u32, player: &Player) -> Option<Vec3> {
        let frame = self.frames.iter().find(|f| f.tick == tick)?;
        let pose = frame.players.get(player.id as usize)?.as_ref()?;
        (pose.alive && pose.life == self.lives[player.id as usize]).then_some(pose.position)
    }
}
impl Match {
    /// Relocate an existing **living** actor at an authority-owned destination.
    /// This is neither a client input nor a respawn: health, ammo, aim, weapon
    /// and respawn timers are retained. Dead actors reject; existing respawn
    /// APIs own life reset. Motion policy controls velocity/ground/jump only.
    ///
    /// Full checkpoint state and every Arena geometry/spawn value are validated
    /// before querying the actual FPS capsule against Arena terrain/all layers.
    /// Destination must be finite and within the existing +/-4096 checkpoint
    /// domain on every axis. No recovery, snap-to-ground or destination clamping
    /// occurs. Incomplete clearance rejects even if a blocker was seen first.
    /// Dynamic occupancy follows the explicit option; self is always ignored.
    /// No tick/input/event/output is consumed or produced, including StepOutput's
    /// retained sidecar. Failures do not authorize any presentation reset.
    ///
    /// Acceptance increments the full life/discontinuity generation exactly once
    /// (even at the same position), invalidating this actor's pre-boundary poses.
    /// It refreshes only this actor in an existing current-tick frame, so repeated
    /// same-tick calls use the latest accepted pose and preserve other actors'
    /// history. Ordinary current shots use the new live pose too. Generation
    /// exhaustion rejects atomically; no history is globally cleared.
    ///
    /// Existing v5 snapshots carry only `life_epochs()`'s low32 generation.
    /// Receivers compare equality, not ordering: an adjacent low32 wrap differs,
    /// but 2^32 transitions between samples can alias. Same-tick network snapshots
    /// are ignored by existing clients; publish on a later actual server tick.
    /// A restored older checkpoint also restores its generations: reset host
    /// presentation/confirmation state at that explicit timeline boundary.
    ///
    /// External relocations are **not recorded** by existing replay::Capture or
    /// spawn::Capture input codecs. End the prior recording before relocation;
    /// create a new Capture from the accepted checkpoint afterwards. Restore or
    /// replay that segment from its checkpoint, never expect old input to replay
    /// an unrecorded teleport. No event/packet/checkpoint schema changes here.
    ///
    /// Runnable authority acceptance and accepted-only camera handoff. An engine
    /// host can supply `|p| { pair.snap(p); }` using its InterpolationPair and
    /// cut/reset its camera at the same boundary (FPS has no engine dependency).
    ///
    /// ```
    /// use ferravex_fps::{Arena, Match, RelocationOptions, RelocationError};
    /// use ferravex_math::Vec3;
    /// fn move_and_cut(game: &mut Match, destination: Vec3,
    ///                 mut snap_camera: impl FnMut(Vec3)) -> Result<(), RelocationError> {
    ///     let accepted = game.relocate_player(0, destination, RelocationOptions::default())?;
    ///     assert_eq!(accepted.tick, game.tick); // No synthetic simulation tick.
    ///     snap_camera(accepted.position); // Only after acceptance.
    ///     Ok(())
    /// }
    /// let mut game = Match::new(Arena { solids: vec![], spawns: vec![Vec3::ZERO] });
    /// assert!(game.spawn_player(0));
    /// let mut camera = Vec3::ZERO;
    /// let mut cuts = 0;
    /// move_and_cut(&mut game, Vec3::X * 4., |p| { camera = p; cuts += 1; }).unwrap();
    /// assert_eq!((camera, cuts), (Vec3::X * 4., 1));
    /// assert!(move_and_cut(&mut game, Vec3::splat(f32::NAN), |_| cuts += 1).is_err());
    /// assert_eq!(cuts, 1);
    /// let mut checkpoint = Vec::new();
    /// game.checkpoint_into(&mut checkpoint).unwrap();
    /// let restored = Match::from_checkpoint(&checkpoint).unwrap();
    /// assert_eq!(restored.state_hash(), game.state_hash());
    /// // Start a fresh replay Capture here, from the accepted state.
    /// ```
    pub fn relocate_player(
        &mut self,
        id: u8,
        destination: Vec3,
        options: RelocationOptions,
    ) -> Result<Relocation, RelocationError> {
        if id as usize >= MAX_PLAYERS {
            return Err(RelocationError::InvalidPlayerId);
        }
        if options.max_candidates > 1_000_000 {
            return Err(RelocationError::InvalidOptions);
        }
        if !crate::checkpoint::bounded(destination, 4096.) {
            return Err(RelocationError::InvalidDestination);
        }
        arena_authoring::validate_arena(
            &self.arena,
            arena_authoring::AuthoringOptions {
                empty_spawns: arena_authoring::EmptySpawnPolicy::LegacyOrigin,
                ..Default::default()
            },
        )
        .map_err(RelocationError::Arena)?;
        self.state_hash().map_err(RelocationError::InvalidState)?;
        let index = self
            .players
            .iter()
            .position(|p| p.id == id)
            .ok_or(RelocationError::UnknownPlayer)?;
        if !self.players[index].is_alive() {
            return Err(RelocationError::PlayerDead);
        }
        let life = self
            .rewind
            .life(id)
            .checked_add(1)
            .ok_or(RelocationError::GenerationExhausted)?;
        ferravex_physics::character::check_clearance(
            &self.arena,
            CharacterShape::Capsule {
                radius: PLAYER_RADIUS,
                height: PLAYER_HEIGHT,
            },
            destination,
            u32::MAX,
            options.max_candidates,
        )
        .map_err(RelocationError::Clearance)?;
        if options.occupancy == RelocationOccupancy::ForbidLivingAabb {
            // Ascending ID makes the reported blocker independent of roster order.
            if let Some(player) = self
                .players
                .iter()
                .filter(|p| {
                    let delta = destination - p.position;
                    p.id != id
                        && p.is_alive()
                        && delta.x.abs() < 2. * PLAYER_RADIUS
                        && delta.z.abs() < 2. * PLAYER_RADIUS
                        && delta.y.abs() < PLAYER_HEIGHT
                })
                .min_by_key(|p| p.id)
            {
                return Err(RelocationError::Occupied { player: player.id });
            }
        }
        let player = &mut self.players[index];
        player.position = destination;
        if options.motion == RelocationMotion::Reset {
            player.velocity = Vec3::ZERO;
            player.grounded = false;
            player.jump = JumpState::default();
        }
        let accepted = Relocation {
            player: id,
            position: destination,
            yaw: player.yaw,
            pitch: player.pitch,
            tick: self.tick,
            life,
        };
        self.rewind.relocate(self.tick, &self.players, id, life);
        Ok(accepted)
    }

    /// Read-only authority query using the shooter's current eye and historical
    /// target poses. Current arena solids occlude old poses. Removed players,
    /// dead targets and previous lives of a respawned/reused slot cannot be hit.
    /// This function alone never spends ammunition or applies damage.
    pub fn rewind_hit_query(
        &self,
        attacker: u8,
        shot_tick: u32,
        yaw: f32,
        pitch: f32,
    ) -> Result<ShotTrace, RewindError> {
        let player = self.player(attacker).ok_or(RewindError::UnknownPlayer)?;
        if !player.is_alive() {
            return Err(RewindError::ShooterDead);
        }
        if !(InputFrame {
            yaw,
            pitch,
            ..InputFrame::default()
        })
        .is_valid()
        {
            return Err(RewindError::InvalidAim);
        }
        self.rewind.validate(self.tick, shot_tick)?;
        Ok(self.trace_shot(
            attacker,
            player.eye_position(),
            forward(yaw, pitch),
            Some(shot_tick),
        ))
    }
    pub(crate) fn trace_shot(
        &self,
        attacker: u8,
        origin: Vec3,
        direction: Vec3,
        shot_tick: Option<u32>,
    ) -> ShotTrace {
        let mut distance = self
            .arena
            .raycast(origin, direction, MAX_SHOT_DISTANCE)
            .unwrap_or(MAX_SHOT_DISTANCE);
        let mut victim = None;
        let mut headshot = false;
        for target in &self.players {
            if target.id == attacker || !target.is_alive() {
                continue;
            }
            let position = if let Some(tick) = shot_tick {
                let Some(position) = self.rewind.pose(tick, target) else {
                    continue;
                };
                position
            } else {
                target.position
            };
            let min = position - Vec3::new(PLAYER_RADIUS, 0., PLAYER_RADIUS);
            let max = position + Vec3::new(PLAYER_RADIUS, PLAYER_HEIGHT, PLAYER_RADIUS);
            if let Some(hit) = ray_aabb(origin, direction, min, max, distance)
                && hit < distance
            {
                distance = hit;
                victim = Some(target.id);
                headshot = (origin + direction * hit).y - position.y > 1.35;
            }
        }
        ShotTrace {
            origin,
            end: origin + direction * distance,
            victim,
            headshot,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn journey400_240_preserves_every_other_pose_even_in_existing_current_frame() {
        let mut game = game();
        game.step(&[]);
        game.step(&[]);
        let before: Vec<_> = game
            .rewind
            .frames
            .iter()
            .map(|f| (f.tick, f.players[0].map(|p| (p.position, p.alive, p.life))))
            .collect();
        let life = game.rewind.life(0);
        // Raw current edits do not rewrite another actor's retained current pose.
        // The relocation operation must preserve it, rather than recapture all.
        game.player_mut(0).unwrap().position.x = 1.;
        for x in [2., 3., 4.] {
            game.relocate_player(1, Vec3::new(x, 0., -5.), RelocationOptions::default())
                .unwrap();
            let after: Vec<_> = game
                .rewind
                .frames
                .iter()
                .map(|f| (f.tick, f.players[0].map(|p| (p.position, p.alive, p.life))))
                .collect();
            assert_eq!(after, before);
            assert_eq!(game.rewind.life(0), life);
            assert_eq!(
                game.rewind.pose(game.tick, game.player(1).unwrap()),
                Some(Vec3::new(x, 0., -5.))
            );
        }
    }
    #[test]
    fn journey400_240_full_generation_exhaustion_rejects_without_global_history_clear() {
        let mut game = game();
        game.step(&[]);
        game.rewind.lives[1] = u64::MAX - 1;
        let accepted = game
            .relocate_player(1, Vec3::new(0., 0., -3.), RelocationOptions::default())
            .unwrap();
        assert_eq!(accepted.life, u64::MAX);
        let mut bytes = Vec::new();
        game.checkpoint_into(&mut bytes).unwrap();
        let mut restored = Match::from_checkpoint(&bytes).unwrap();
        assert_eq!(
            restored
                .rewind_hit_query(0, restored.tick, 0., 0.)
                .unwrap()
                .victim,
            Some(1)
        );
        let before = restored.state_hash().unwrap();
        let frames = restored.rewind.frames.len();
        for _ in 0..3 {
            assert_eq!(
                restored.relocate_player(1, Vec3::X, RelocationOptions::default()),
                Err(RelocationError::GenerationExhausted)
            );
            assert_eq!(restored.state_hash().unwrap(), before);
            assert_eq!(restored.rewind.frames.len(), frames);
        }
    }
    #[test]
    fn journey400_240_low32_wrap_keeps_full_checkpoint_generation_and_existing_v5_codec() {
        use ferravex_net::{ServerPacket, Snapshot, decode_server, encode_server};
        let mut game = game();
        game.rewind.lives[1] = u64::from(u32::MAX);
        game.tick = u32::MAX;
        game.step(&[]); // Tick wraps, retaining both frames.
        let old = game.tick;
        game.step(&[]);
        let old_epochs = game.life_epochs();
        let accepted = game
            .relocate_player(1, Vec3::new(0., 0., -3.), RelocationOptions::default())
            .unwrap();
        assert_eq!(accepted.life, 1 << 32);
        assert_eq!(game.life_epochs()[1], 0);
        assert_ne!(old_epochs[1], game.life_epochs()[1]); // Equality-based interpolation cuts.
        assert_eq!(game.rewind_hit_query(0, old, 0., 0.).unwrap().victim, None);
        let packet = ServerPacket::Snapshot(Snapshot {
            session: 7,
            tick: game.tick,
            epochs: game.life_epochs(),
            players: game.players.iter().map(Player::snapshot).collect(),
            ..Default::default()
        });
        assert_eq!(
            decode_server(&encode_server(&packet).unwrap()).unwrap(),
            packet
        );
        let mut bytes = Vec::new();
        game.checkpoint_into(&mut bytes).unwrap();
        let mut restored = Match::from_checkpoint(&bytes).unwrap();
        assert_eq!(restored.rewind.life(1), accepted.life);
        assert_eq!(
            restored
                .rewind_hit_query(0, restored.tick, 0., 0.)
                .unwrap()
                .victim,
            Some(1)
        );
        let next = restored
            .relocate_player(1, Vec3::new(0., 0., -4.), RelocationOptions::default())
            .unwrap();
        assert_eq!(next.life, accepted.life + 1);
        assert_eq!(restored.life_epochs()[1], 1);
    }
    #[test]
    fn capacity_rejection_preserves_existing_short_output_and_allocations() {
        let mut game = Match::default();
        assert!(game.spawn_player(0));
        let mut output = StepOutput::with_capacity(1, 1);
        let event = GameEvent::Respawned { player: 0 };
        let rejection = (0, RewindError::FutureTick);
        output.inner.events.push(event);
        output.inner.rejected_shots.push(rejection);
        let capacities = output.capacities();
        let events_ptr = output.events().as_ptr();
        let rejected_ptr = output.rejected_shots().as_ptr();
        let before = game.state_hash().unwrap();
        assert!(matches!(
            game.step_into(&[], &mut output),
            Err(StepError::OutputCapacity { .. })
        ));
        assert_eq!(game.state_hash().unwrap(), before);
        assert_eq!(output.events(), [event]);
        assert_eq!(output.rejected_shots(), [rejection]);
        assert_eq!(output.capacities(), capacities);
        assert_eq!(output.events().as_ptr(), events_ptr);
        assert_eq!(output.rejected_shots().as_ptr(), rejected_ptr);
    }
    #[test]
    fn journey400_239_short_output_rejection_preserves_nonempty_inline_sidecar() {
        for policy in [
            None,
            Some(spawn::Policy::new(spawn::Config::default()).unwrap()),
        ] {
            let mut game = Match::default();
            assert!(game.spawn_player(0));
            let mut output = StepOutput::new();
            game.step_into(
                &[(
                    0,
                    InputFrame {
                        buttons: BUTTON_FIRE,
                        ..Default::default()
                    },
                )],
                &mut output,
            )
            .unwrap();
            let record = output.transitions()[0];
            // Simulate storage being replaced by an undersized caller buffer.
            output.inner.events = vec![GameEvent::Respawned { player: 0 }];
            let before = game.state_hash();
            let capacities = output.capacities();
            let pointer = output.transitions().as_ptr();
            assert!(matches!(
                game.step_with_spawn_policy_into(&[], &[], policy.as_ref(), &mut output),
                Err(StepError::OutputCapacity { .. })
            ));
            assert_eq!(game.state_hash(), before);
            assert_eq!(output.transitions(), [record]);
            assert_eq!(output.transition_count(), 1);
            assert_eq!(output.transition_capacity(), MAX_STEP_TRANSITIONS);
            assert_eq!(output.transitions().as_ptr(), pointer);
            assert_eq!(output.capacities(), capacities);
            assert_eq!(output.events(), [GameEvent::Respawned { player: 0 }]);
        }
    }
    fn game() -> Match {
        let mut game = Match::new(Arena {
            solids: vec![],
            spawns: vec![Vec3::ZERO, Vec3::new(0., 0., -5.)],
        });
        game.spawn_player(0);
        game.spawn_player(1);
        game
    }
    fn move_victim(game: &mut Match) -> u32 {
        game.step(&[]);
        let old_tick = game.tick;
        for _ in 0..10 {
            game.step(&[(
                1,
                InputFrame {
                    move_x: 1.,
                    ..InputFrame::default()
                },
            )]);
        }
        old_tick
    }
    #[test]
    fn rewind_hits_moving_victim_and_applies_authoritative_damage() {
        let mut game = game();
        let old_tick = move_victim(&mut game);
        assert!(game.player(1).unwrap().position.x > PLAYER_RADIUS);
        assert_eq!(
            game.rewind_hit_query(0, game.tick, 0., 0.).unwrap().victim,
            None
        );
        assert_eq!(
            game.rewind_hit_query(0, old_tick, 0., 0.).unwrap().victim,
            Some(1)
        );
        let result = game.step_with_rewind(
            &[(
                0,
                InputFrame {
                    buttons: BUTTON_FIRE,
                    ..InputFrame::default()
                },
            )],
            &[ShotTime {
                player: 0,
                tick: old_tick,
            }],
        );
        assert!(result.rejected_shots.is_empty());
        assert!(result.events.iter().any(|e| matches!(
            e,
            GameEvent::Killed {
                attacker: 0,
                victim: 1
            }
        )));
        assert_eq!(game.player(1).unwrap().health, 0);
        assert_eq!(game.player(0).unwrap().ammo, MAGAZINE_SIZE - 1);
    }
    #[test]
    fn future_and_expired_times_cannot_fire_but_movement_still_advances() {
        let mut game = game();
        for _ in 0..MAX_REWIND_TICKS + 3 {
            game.step(&[]);
        }
        assert_eq!(
            game.rewind_hit_query(0, 0, 0., 0.),
            Err(RewindError::TooOld)
        );
        assert_eq!(
            game.rewind_hit_query(0, game.tick + 1, 0., 0.),
            Err(RewindError::FutureTick)
        );
        let future = game.tick + 1;
        let result = game.step_with_rewind(
            &[(
                0,
                InputFrame {
                    move_x: 1.,
                    buttons: BUTTON_FIRE,
                    ..InputFrame::default()
                },
            )],
            &[ShotTime {
                player: 0,
                tick: future,
            }],
        );
        assert_eq!(result.rejected_shots, vec![(0, RewindError::FutureTick)]);
        assert!(result.events.is_empty());
        assert_eq!(game.player(0).unwrap().ammo, MAGAZINE_SIZE);
        assert!(game.player(0).unwrap().position.x > 0.);
        assert_eq!(game.rewind.frames.len(), MAX_REWIND_TICKS as usize + 1);
    }
    #[test]
    fn current_world_occludes_historical_target_pose() {
        let mut game = game();
        let old_tick = move_victim(&mut game);
        game.arena.solids.push(Obstacle {
            center: Vec3::new(0., 1., -2.),
            half_extents: Vec3::new(0.5, 2., 0.2),
        });
        let trace = game.rewind_hit_query(0, old_tick, 0., 0.).unwrap();
        assert_eq!(trace.victim, None);
        assert!((trace.end.z + 1.8).abs() < 0.001);
        let result = game.step_with_rewind(
            &[(
                0,
                InputFrame {
                    buttons: BUTTON_FIRE,
                    ..InputFrame::default()
                },
            )],
            &[ShotTime {
                player: 0,
                tick: old_tick,
            }],
        );
        assert!(
            result
                .events
                .iter()
                .all(|e| !matches!(e, GameEvent::Hit { .. }))
        );
        assert_eq!(game.player(1).unwrap().health, 100);
    }
    #[test]
    fn rewind_never_hits_old_lives_or_reused_player_slots() {
        let mut game = game();
        game.step(&[]);
        let old = game.tick;
        game.player_mut(1).unwrap().health = 0;
        game.player_mut(1).unwrap().respawn_ticks = 1;
        game.step(&[]);
        assert_eq!(game.rewind_hit_query(0, old, 0., 0.).unwrap().victim, None);
        let before_reuse = game.tick;
        game.remove_player(1);
        game.spawn_player(1);
        game.step(&[]);
        // Recorded tick poses stay immutable across removal and slot reuse.
        assert_eq!(game.rewind_hit_query(0, old, 0., 0.).unwrap().victim, None);
        assert_eq!(
            game.rewind_hit_query(0, before_reuse, 0., 0.)
                .unwrap()
                .victim,
            None
        );
    }
    #[test]
    fn wraparound_tick_window_remains_bounded_and_queries_do_not_mutate() {
        let mut game = Match::new(Arena {
            solids: vec![],
            spawns: vec![Vec3::ZERO, Vec3::new(0., 0., -5.)],
        });
        game.tick = u32::MAX - 3;
        game.spawn_player(0);
        game.spawn_player(1);
        let before_wrap = game.tick;
        for _ in 0..6 {
            game.step(&[]);
        }
        let players = game.players.clone();
        assert_eq!(
            game.rewind_hit_query(0, before_wrap, 0., 0.)
                .unwrap()
                .victim,
            Some(1)
        );
        assert_eq!(
            game.rewind_hit_query(0, game.tick + 1, 0., 0.),
            Err(RewindError::FutureTick)
        );
        assert_eq!(
            game.rewind_hit_query(0, before_wrap, f32::NAN, 0.),
            Err(RewindError::InvalidAim)
        );
        assert_eq!(game.players, players);
    }
}
