//! A small original arena shooter simulation: server-owned movement, collision,
//! weapons, and health. No rendering or platform services enter the simulation.
//! All movement uses the same 64 Hz step for offline play, authority, and replay.
use ferravex_math::Vec3;
use ferravex_physics::character::{
    Character, CharacterError, CharacterInput, CharacterSettings, CharacterShape, JumpState,
    Obstacle as CharacterObstacle, ObstacleShape, QueryBounds, Terrain,
};
pub mod checkpoint;
pub mod combat;
pub mod tactical;
pub use combat::{CombatRules, PracticeRules, WeaponSettings};
mod event_wire;
pub mod replay;
pub use event_wire::NetworkEvent;
mod rewind;
use ferravex_net::PlayerSnapshot;
pub use ferravex_net::{BUTTON_FIRE, BUTTON_JUMP, BUTTON_RELOAD, InputFrame, MAX_PLAYERS};
pub use rewind::{MAX_REWIND_TICKS, RewindError, RewindStep, ShotTime, ShotTrace};

pub const TICK_RATE: u32 = 64;
pub const FIXED_DT: f32 = 1.0 / TICK_RATE as f32;
pub const PLAYER_HEIGHT: f32 = 1.8;
pub const PLAYER_RADIUS: f32 = 0.3;
pub const EYE_HEIGHT: f32 = 1.62;
pub const WALK_SPEED: f32 = 7.0;
pub const MAGAZINE_SIZE: u8 = 30;
pub const RELOAD_TICKS: u16 = 128;
pub const RESPAWN_TICKS: u16 = 192;
pub const FIRE_INTERVAL_TICKS: u16 = 7;
pub const MAX_SHOT_DISTANCE: f32 = 100.0;

#[derive(Clone, Copy, Debug)]
pub struct Obstacle {
    pub center: Vec3,
    pub half_extents: Vec3,
}
impl Obstacle {
    pub fn min(self) -> Vec3 {
        self.center - self.half_extents
    }
    pub fn max(self) -> Vec3 {
        self.center + self.half_extents
    }
}
#[derive(Clone, Debug)]
pub struct Arena {
    pub solids: Vec<Obstacle>,
    pub spawns: Vec<Vec3>,
}
impl Default for Arena {
    fn default() -> Self {
        let data = [
            ([-23., 3., 0.], [1., 3., 28.]),
            ([23., 3., 0.], [1., 3., 28.]),
            ([0., 3., -27.], [24., 3., 1.]),
            ([0., 3., 27.], [24., 3., 1.]),
            ([-9., 1.25, 8.], [4., 1.25, 1.8]),
            ([8., 1.5, 8.], [3., 1.5, 3.]),
            ([-8., 1.5, -9.], [3.5, 1.5, 3.]),
            ([9., 1.25, -10.], [4., 1.25, 2.]),
            ([0., 1.25, -1.], [2., 1.25, 5.]),
            ([-17., 0.75, -2.], [1.5, 0.75, 2.]),
            ([17., 0.75, -2.], [1.5, 0.75, 2.]),
        ];
        let spawns = [
            [0., 0., 22.],
            [-16., 0., -20.],
            [16., 0., -20.],
            [-17., 0., 14.],
            [17., 0., 14.],
            [0., 0., -22.],
            [-17., 0., 22.],
            [17., 0., 22.],
            [-20., 0., -15.],
            [20., 0., -15.],
            [-5., 0., 22.],
            [5., 0., 22.],
            [-5., 0., -22.],
            [5., 0., -22.],
            [-20., 0., 5.],
            [20., 0., 5.],
        ];
        Self {
            solids: data
                .into_iter()
                .map(|(c, h)| Obstacle {
                    center: Vec3::from(c),
                    half_extents: Vec3::from(h),
                })
                .collect(),
            spawns: spawns.into_iter().map(Vec3::from).collect(),
        }
    }
}
impl Arena {
    pub fn spawn(&self, id: u8) -> Vec3 {
        self.spawns
            .get(id as usize % self.spawns.len().max(1))
            .copied()
            .unwrap_or(Vec3::ZERO)
    }
    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<f32> {
        self.solids
            .iter()
            .filter_map(|solid| ray_aabb(origin, direction, solid.min(), solid.max(), max_distance))
            .min_by(f32::total_cmp)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub id: u8,
    /// Feet position; rendering should offset the body center by half the height.
    pub position: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub health: u8,
    pub ammo: u8,
    pub grounded: bool,
    pub jump: JumpState,
    pub fire_cooldown: u16,
    pub reload_ticks: u16,
    pub respawn_ticks: u16,
}
impl Player {
    pub fn new(id: u8, position: Vec3) -> Self {
        Self {
            id,
            position,
            velocity: Vec3::ZERO,
            yaw: 0.,
            pitch: 0.,
            health: 100,
            ammo: MAGAZINE_SIZE,
            grounded: true,
            jump: JumpState::default(),
            fire_cooldown: 0,
            reload_ticks: 0,
            respawn_ticks: 0,
        }
    }
    pub fn eye_position(&self) -> Vec3 {
        self.position + Vec3::Y * EYE_HEIGHT
    }
    pub fn is_alive(&self) -> bool {
        self.health > 0
    }
    pub fn snapshot(&self) -> PlayerSnapshot {
        PlayerSnapshot {
            id: self.id,
            position: self.position.to_array(),
            velocity: self.velocity.to_array(),
            yaw: self.yaw,
            pitch: self.pitch,
            health: self.health,
            ammo: self.ammo,
            grounded: self.grounded,
            jump_coyote: self.jump.coyote,
            jump_buffer: self.jump.buffer,
            jump_held: self.jump.held,
            fire_cooldown: self.fire_cooldown,
            reload_ticks: self.reload_ticks,
            respawn_ticks: self.respawn_ticks,
        }
    }
    pub fn from_snapshot(p: &PlayerSnapshot) -> Self {
        Self {
            id: p.id,
            position: Vec3::from(p.position),
            velocity: Vec3::from(p.velocity),
            yaw: p.yaw,
            pitch: p.pitch,
            health: p.health,
            ammo: p.ammo,
            grounded: p.grounded,
            jump: JumpState {
                coyote: p.jump_coyote,
                buffer: p.jump_buffer,
                held: p.jump_held,
            },
            fire_cooldown: p.fire_cooldown,
            reload_ticks: p.reload_ticks,
            respawn_ticks: p.respawn_ticks,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GameEvent {
    Shot {
        player: u8,
        origin: Vec3,
        end: Vec3,
    },
    Hit {
        attacker: u8,
        victim: u8,
        damage: u8,
        headshot: bool,
    },
    Killed {
        attacker: u8,
        victim: u8,
    },
    Respawned {
        player: u8,
    },
}

pub struct Match {
    pub arena: Arena,
    pub players: Vec<Player>,
    pub tick: u32,
    rewind: rewind::RewindHistory,
}
impl Match {
    pub fn new(arena: Arena) -> Self {
        Self {
            arena,
            players: Vec::with_capacity(MAX_PLAYERS),
            tick: 0,
            rewind: rewind::RewindHistory::default(),
        }
    }
    pub fn spawn_player(&mut self, id: u8) -> bool {
        if id as usize >= MAX_PLAYERS
            || self.players.iter().any(|p| p.id == id)
            || self.players.len() >= MAX_PLAYERS
        {
            return false;
        }
        self.players.push(Player::new(id, self.arena.spawn(id)));
        self.rewind.new_life(id);
        true
    }
    pub(crate) fn reset_round_players(&mut self, states: &[(u8, Vec3, u8)]) {
        self.rewind.clear_frames();
        for &(id, position, ammo) in states {
            if let Some(player) = self.player_mut(id) {
                *player = Player::new(id, position);
                player.ammo = ammo.min(MAGAZINE_SIZE);
                self.rewind.new_life(id);
            }
        }
        self.rewind.capture(self.tick, &self.players);
    }
    /// Snapshot generations; absent slots are zero. Bounded interpolation never
    /// spans enough lives to confuse a low-32-bit generation wrap.
    pub fn life_epochs(&self) -> [u32; MAX_PLAYERS] {
        std::array::from_fn(|id| {
            if self.player(id as u8).is_some() {
                self.rewind.life(id as u8) as u32
            } else {
                0
            }
        })
    }
    pub fn remove_player(&mut self, id: u8) {
        self.players.retain(|p| p.id != id);
    }
    pub fn player(&self, id: u8) -> Option<&Player> {
        self.players.iter().find(|p| p.id == id)
    }
    pub fn player_mut(&mut self, id: u8) -> Option<&mut Player> {
        self.players.iter_mut().find(|p| p.id == id)
    }
    /// One authoritative tick. Duplicate player inputs are ignored after the first.
    /// Input axes are sanitized, diagonal movement is normalized, and dt is fixed.
    pub fn step(&mut self, inputs: &[(u8, InputFrame)]) -> Vec<GameEvent> {
        self.step_with_rewind(inputs, &[]).events
    }
    /// Advance one authoritative tick, applying validated historical target
    /// poses only to players with an explicit shot time. Other shots retain the
    /// existing current-world behavior. Invalid times suppress firing, never
    /// movement, and are returned to the caller. The first time per player wins.
    pub fn step_with_rewind(
        &mut self,
        inputs: &[(u8, InputFrame)],
        shot_times: &[ShotTime],
    ) -> RewindStep {
        self.step_with_rules(inputs, shot_times, &mut PracticeRules)
    }
    /// The same fixed-tick motor/rewind/shot path with host-owned combat policy.
    pub fn step_with_rules<R: CombatRules>(
        &mut self,
        inputs: &[(u8, InputFrame)],
        shot_times: &[ShotTime],
        rules: &mut R,
    ) -> RewindStep {
        self.rewind.capture(self.tick, &self.players);
        let mut rejected_shots = Vec::new();
        let mut events = Vec::new();
        let mut shots = [None; MAX_PLAYERS];
        let mut shot_count = 0;
        for player in &mut self.players {
            let mut input = inputs
                .iter()
                .find(|(id, _)| *id == player.id)
                .map(|(_, i)| *i)
                .unwrap_or(InputFrame {
                    yaw: player.yaw,
                    pitch: player.pitch,
                    ..Default::default()
                });
            input = rules.input(player, input).sanitized();
            if rules.frozen(player.id) {
                player.velocity = Vec3::ZERO;
                player.yaw = input.yaw;
                player.pitch = input.pitch;
                continue;
            }
            if !player.is_alive() && !rules.auto_respawn() {
                player.velocity = Vec3::ZERO;
                continue;
            }
            let shot_tick = shot_times
                .iter()
                .take(MAX_PLAYERS)
                .find(|time| time.player == player.id)
                .map(|time| time.tick);
            if input.buttons & BUTTON_FIRE != 0
                && let Some(tick) = shot_tick
                && let Err(error) = self.rewind.validate(self.tick, tick)
            {
                input.buttons &= !BUTTON_FIRE;
                rejected_shots.push((player.id, error));
            }
            let was_alive = player.is_alive();
            if step_player_with_settings(player, input, &self.arena, rules.weapon(player.id)) {
                shots[shot_count] = Some((
                    player.id,
                    player.eye_position(),
                    forward(player.yaw, player.pitch),
                    shot_tick,
                ));
                shot_count += 1;
            }
            if !was_alive && player.is_alive() {
                self.rewind.new_life(player.id);
                events.push(GameEvent::Respawned { player: player.id });
            }
        }
        // Shooter origin and static occlusion always use this tick's authority.
        for (attacker, origin, direction, shot_tick) in shots.into_iter().flatten() {
            let trace = self.trace_shot(attacker, origin, direction, shot_tick);
            events.push(GameEvent::Shot {
                player: attacker,
                origin,
                end: trace.end,
            });
            if let Some(victim) = trace.victim {
                let target = self
                    .player_mut(victim)
                    .expect("trace returned current player");
                let Some(damage) = rules.damage(attacker, target, trace.headshot) else {
                    continue;
                };
                target.health = target.health.saturating_sub(damage);
                events.push(GameEvent::Hit {
                    attacker,
                    victim,
                    damage,
                    headshot: trace.headshot,
                });
                if !target.is_alive() {
                    target.respawn_ticks = RESPAWN_TICKS;
                    target.velocity = Vec3::ZERO;
                    events.push(GameEvent::Killed { attacker, victim });
                }
            }
        }
        self.tick = self.tick.wrapping_add(1);
        self.rewind.capture(self.tick, &self.players);
        RewindStep {
            events,
            rejected_shots,
        }
    }
}
impl Default for Match {
    fn default() -> Self {
        Self::new(Arena::default())
    }
}

/// World-space unit look direction. Yaw 0 faces -Z and positive pitch looks up.
pub fn forward(yaw: f32, pitch: f32) -> Vec3 {
    let (sy, cy) = ferravex_math::deterministic::sin_cos(yaw);
    let (sp, cp) = ferravex_math::deterministic::sin_cos(pitch);
    Vec3::new(sy * cp, sp, -cy * cp)
}

/// Shared client/server step, including weapon timers but excluding damage to others.
/// Returns whether a legal shot was fired. Prediction never applies remote damage.
pub fn step_player(player: &mut Player, input: InputFrame, arena: &Arena) -> bool {
    step_player_with_settings(player, input, arena, WeaponSettings::default())
}
/// Invalid weapon settings leave the complete player state untouched.
pub fn step_player_with_settings(
    player: &mut Player,
    input: InputFrame,
    arena: &Arena,
    weapon: WeaponSettings,
) -> bool {
    if !weapon.valid() {
        return false;
    }
    let input = input.sanitized();
    if !player.is_alive() {
        player.respawn_ticks = player.respawn_ticks.saturating_sub(1);
        if player.respawn_ticks == 0 {
            *player = Player::new(player.id, arena.spawn(player.id));
        }
        return false;
    }
    player.yaw = input.yaw;
    player.pitch = input.pitch;
    if try_move_player(player, input, arena).is_err() {
        return false;
    }
    player.fire_cooldown = player.fire_cooldown.saturating_sub(1);
    if player.reload_ticks > 0 {
        player.reload_ticks -= 1;
        if player.reload_ticks == 0 {
            player.ammo = weapon.magazine;
        }
    }
    if input.buttons & BUTTON_RELOAD != 0
        && player.ammo < weapon.magazine
        && player.reload_ticks == 0
    {
        player.reload_ticks = weapon.reload_ticks;
    }
    if input.buttons & BUTTON_FIRE != 0
        && player.ammo > 0
        && player.reload_ticks == 0
        && player.fire_cooldown == 0
    {
        player.ammo -= 1;
        player.fire_cooldown = weapon.fire_ticks;
        return true;
    }
    false
}

/// Shared capsule motor. Static arena supports require no additional platform pose
/// state; jump timers and held state are included in PlayerSnapshot for reconciliation.
/// Invalid terrain/state or an exhausted query budget preserves the player's motion.
pub fn try_move_player(
    player: &mut Player,
    input: InputFrame,
    arena: &Arena,
) -> Result<(), CharacterError> {
    let input = input.sanitized();
    let (sin, cos) = ferravex_math::deterministic::sin_cos(input.yaw);
    let forward = Vec3::new(sin, 0., -cos);
    let right = Vec3::new(cos, 0., sin);
    let mut motor = Character::new(player.position);
    motor.velocity = player.velocity;
    motor.grounded = player.grounded;
    motor.jump = player.jump;
    motor.step(
        arena,
        CharacterShape::Capsule {
            radius: PLAYER_RADIUS,
            height: PLAYER_HEIGHT,
        },
        CharacterSettings {
            speed: WALK_SPEED,
            ..Default::default()
        },
        CharacterInput {
            direction: right * input.move_x + forward * input.move_z,
            jump: input.buttons & BUTTON_JUMP != 0,
        },
        FIXED_DT,
    )?;
    player.position = motor.position;
    player.velocity = motor.velocity;
    player.grounded = motor.grounded;
    player.jump = motor.jump;
    Ok(())
}
/// Compatibility entry point for callers that do not consume movement diagnostics.
pub fn move_player(player: &mut Player, input: InputFrame, arena: &Arena) {
    let _ = try_move_player(player, input, arena);
}
impl Arena {
    fn character_obstacle(&self, id: u64) -> Option<CharacterObstacle> {
        if id == 0 {
            return Some(CharacterObstacle {
                shape: ObstacleShape::Plane { normal: Vec3::Y },
                ..CharacterObstacle::cuboid(0, Vec3::ZERO, Vec3::ONE)
            });
        }
        let solid = self.solids.get((id - 1) as usize)?;
        Some(CharacterObstacle::cuboid(
            id,
            solid.center,
            solid.half_extents,
        ))
    }
}
impl Terrain for Arena {
    fn visit(&self, bounds: QueryBounds, visitor: &mut dyn FnMut(CharacterObstacle)) {
        visitor(self.character_obstacle(0).unwrap());
        for i in 0..self.solids.len() {
            let o = self.character_obstacle(i as u64 + 1).unwrap();
            if !o.valid() || bounds.intersects(o.bounds()) {
                visitor(o);
            }
        }
    }
    fn obstacle(&self, id: u64) -> Option<CharacterObstacle> {
        self.character_obstacle(id)
    }
}

/// Slab ray/AABB intersection, including axis-parallel rays and origins inside boxes.
pub fn ray_aabb(
    origin: Vec3,
    direction: Vec3,
    min: Vec3,
    max: Vec3,
    max_distance: f32,
) -> Option<f32> {
    if !origin.is_finite()
        || !direction.is_finite()
        || !max_distance.is_finite()
        || max_distance < 0.
    {
        return None;
    }
    let mut near = 0.0f32;
    let mut far = max_distance;
    for axis in 0..3 {
        if direction[axis].abs() < 1e-7 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - origin[axis]) / direction[axis];
            let b = (max[axis] - origin[axis]) / direction[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    Some(near)
}

#[cfg(target_arch = "wasm32")]
pub mod browser;
mod network;
pub use network::{GameClient, GameServer, TacticalOptions};

#[cfg(test)]
mod tests {
    use super::*;
    fn empty_arena() -> Arena {
        Arena {
            solids: vec![],
            spawns: vec![Vec3::ZERO],
        }
    }
    #[test]
    fn movement_is_fixed_normalized_and_finite() {
        let a = empty_arena();
        let mut straight = Player::new(0, Vec3::ZERO);
        let mut diagonal = straight.clone();
        for _ in 0..64 {
            move_player(
                &mut straight,
                InputFrame {
                    move_z: 1.,
                    ..Default::default()
                },
                &a,
            );
            move_player(
                &mut diagonal,
                InputFrame {
                    move_x: 99.,
                    move_z: 99.,
                    ..Default::default()
                },
                &a,
            );
        }
        assert!((straight.position.length() - diagonal.position.length()).abs() < 0.001);
        assert!(straight.position.length() <= WALK_SPEED);
        move_player(
            &mut diagonal,
            InputFrame {
                move_x: f32::NAN,
                yaw: f32::INFINITY,
                ..Default::default()
            },
            &a,
        );
        assert!(diagonal.position.is_finite());
    }
    #[test]
    fn walls_slide_and_ceiling_stops_jump() {
        let a = Arena {
            solids: vec![Obstacle {
                center: Vec3::new(2., 2., 0.),
                half_extents: Vec3::new(0.25, 2., 20.),
            }],
            spawns: vec![Vec3::ZERO],
        };
        let mut p = Player::new(0, Vec3::ZERO);
        for _ in 0..256 {
            move_player(
                &mut p,
                InputFrame {
                    move_x: 1.,
                    move_z: 1.,
                    ..Default::default()
                },
                &a,
            );
        }
        assert!(p.position.x <= 1.451);
        assert!(p.position.z < -10.);
        assert!(p.grounded);
        let ceiling = Arena {
            solids: vec![Obstacle {
                center: Vec3::new(0., 2.2, 0.),
                half_extents: Vec3::new(5., 0.2, 5.),
            }],
            spawns: vec![],
        };
        let mut p = Player::new(0, Vec3::ZERO);
        let mut peak = 0.0f32;
        for tick in 0..128 {
            move_player(
                &mut p,
                InputFrame {
                    buttons: if tick == 0 { BUTTON_JUMP } else { 0 },
                    ..Default::default()
                },
                &ceiling,
            );
            peak = peak.max(p.position.y);
        }
        assert!(peak <= 0.201);
        assert!(p.position.y.abs() < 0.003);
    }
    #[test]
    fn hitscan_is_occluded_and_server_controls_damage() {
        let mut game = Match::new(empty_arena());
        game.spawn_player(0);
        game.spawn_player(1);
        game.player_mut(1).unwrap().position = Vec3::new(0., 0., -5.);
        game.arena.solids.push(Obstacle {
            center: Vec3::new(0., 1., -2.),
            half_extents: Vec3::ONE,
        });
        game.step(&[(
            0,
            InputFrame {
                buttons: BUTTON_FIRE,
                ..Default::default()
            },
        )]);
        assert_eq!(game.player(1).unwrap().health, 100);
        game.arena.solids.clear();
        game.player_mut(0).unwrap().fire_cooldown = 0;
        let events = game.step(&[(
            0,
            InputFrame {
                buttons: BUTTON_FIRE,
                ..Default::default()
            },
        )]);
        assert_eq!(game.player(1).unwrap().health, 0);
        assert!(events.iter().any(|e| matches!(e, GameEvent::Killed { .. })));
        for _ in 0..RESPAWN_TICKS {
            game.step(&[]);
        }
        assert_eq!(game.player(1).unwrap().health, 100);
    }
    #[test]
    fn reload_and_fire_rate_are_authoritative() {
        let a = empty_arena();
        let mut p = Player::new(0, Vec3::ZERO);
        let mut shots = 0;
        for _ in 0..64 {
            shots += step_player(
                &mut p,
                InputFrame {
                    buttons: BUTTON_FIRE,
                    ..Default::default()
                },
                &a,
            ) as usize;
        }
        assert_eq!(shots, 10);
        assert_eq!(p.ammo, 20);
        step_player(
            &mut p,
            InputFrame {
                buttons: BUTTON_RELOAD | BUTTON_FIRE,
                ..Default::default()
            },
            &a,
        );
        assert_eq!(p.reload_ticks, RELOAD_TICKS);
        for _ in 0..RELOAD_TICKS {
            step_player(&mut p, InputFrame::default(), &a);
        }
        assert_eq!(p.ammo, MAGAZINE_SIZE);
    }
    #[test]
    fn replay_matches_uninterrupted_simulation() {
        let arena = Arena::default();
        let mut authority = Player::new(0, arena.spawn(0));
        let mut saved = None;
        let inputs: Vec<_> = (0..100)
            .map(|sequence| InputFrame {
                sequence,
                move_x: 0.3,
                move_z: 1.0,
                buttons: if sequence == 20 { BUTTON_JUMP } else { 0 },
                ..Default::default()
            })
            .collect();
        for (i, input) in inputs.iter().enumerate() {
            step_player(&mut authority, *input, &arena);
            if i == 30 {
                saved = Some(authority.snapshot());
            }
        }
        let mut replay = Player::from_snapshot(&saved.unwrap());
        for input in &inputs[31..] {
            step_player(&mut replay, *input, &arena);
        }
        assert_eq!(authority, replay);
    }
    #[test]
    fn buffered_jump_survives_wire_snapshot_and_prediction_replay() {
        let arena = empty_arena();
        let mut authority = Player::new(0, Vec3::Y * 0.2);
        authority.grounded = false;
        authority.velocity.y = -5.;
        let input = InputFrame {
            buttons: BUTTON_JUMP,
            ..Default::default()
        };
        try_move_player(&mut authority, input, &arena).unwrap();
        assert!(authority.jump.buffer > 0. && authority.jump.held);
        let bytes = ferravex_net::encode_server(&ferravex_net::ServerPacket::Snapshot(
            ferravex_net::Snapshot {
                session: 1,
                players: vec![authority.snapshot()],
                ..Default::default()
            },
        ))
        .unwrap();
        let ferravex_net::ServerPacket::Snapshot(snapshot) =
            ferravex_net::decode_server(&bytes).unwrap()
        else {
            panic!("snapshot expected")
        };
        let mut predicted = Player::from_snapshot(&snapshot.players[0]);
        let mut peak = 0f32;
        for _ in 0..128 {
            try_move_player(&mut authority, input, &arena).unwrap();
            try_move_player(&mut predicted, input, &arena).unwrap();
            assert_eq!(authority, predicted);
            peak = peak.max(authority.position.y);
        }
        assert!(peak > 1. && authority.grounded);
        assert_eq!(authority.jump.buffer, 0.);
    }
    #[test]
    fn default_spawns_are_clear_and_player_count_is_bounded() {
        let mut game = Match::default();
        for id in 0..MAX_PLAYERS as u8 {
            assert!(game.spawn_player(id));
            let p = game.player(id).unwrap();
            for solid in &game.arena.solids {
                let min = solid.min();
                let max = solid.max();
                assert!(
                    p.position.x + PLAYER_RADIUS <= min.x
                        || p.position.x - PLAYER_RADIUS >= max.x
                        || p.position.z + PLAYER_RADIUS <= min.z
                        || p.position.z - PLAYER_RADIUS >= max.z
                );
            }
        }
        assert!(!game.spawn_player(16));
        assert!(!game.spawn_player(0));
    }
}
