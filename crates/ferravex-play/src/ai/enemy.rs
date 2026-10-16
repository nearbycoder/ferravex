//! Reusable enemy tactics. Feed only validated memory from sight/hearing; outputs
//! are intentions for ordinary movement, navigation and weapons, never damage.
use super::{AiError, Sense, Track};
use ferravex_math::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnemyState {
    Patrol,
    Investigate,
    Search,
    Engage,
    Retreat,
    Reload,
    Dead,
}
#[derive(Clone, Copy, Debug)]
pub struct EnemyConfig {
    pub attack_range: f32,
    pub preferred_range: f32,
    pub reaction_seconds: f32,
    pub attack_interval: f32,
    pub turn_radians_per_second: f32,
    pub aim_tolerance: f32,
    pub retreat_health: f32,
    pub strafe: f32,
    pub search_radius: f32,
}
impl EnemyConfig {
    pub fn ranged() -> Self {
        Self {
            attack_range: 50.,
            preferred_range: 12.,
            reaction_seconds: 0.35,
            attack_interval: 0.65,
            turn_radians_per_second: 4.,
            aim_tolerance: 0.07,
            retreat_health: 0.25,
            strafe: 0.35,
            search_radius: 3.,
        }
    }
    pub fn melee() -> Self {
        Self {
            attack_range: 1.5,
            preferred_range: 0.,
            reaction_seconds: 0.2,
            attack_interval: 0.8,
            retreat_health: 0.,
            strafe: 0.,
            ..Self::ranged()
        }
    }
    fn valid(self) -> bool {
        [
            self.attack_range,
            self.preferred_range,
            self.reaction_seconds,
            self.attack_interval,
            self.turn_radians_per_second,
            self.aim_tolerance,
            self.retreat_health,
            self.strafe,
            self.search_radius,
        ]
        .iter()
        .all(|x| x.is_finite())
            && self.attack_range > 0.
            && (0. ..=self.attack_range).contains(&self.preferred_range)
            && (0. ..=60.).contains(&self.reaction_seconds)
            && self.attack_interval > 0.
            && self.turn_radians_per_second > 0.
            && (0. ..=1.).contains(&self.aim_tolerance)
            && (0. ..=1.).contains(&self.retreat_health)
            && (0. ..=1.).contains(&self.strafe)
            && self.search_radius >= 0.
    }
}
#[derive(Clone, Copy, Debug)]
pub struct EnemyInput {
    pub position: Vec3,
    pub forward: Vec3,
    /// Plane normal: Y for ground-based 3D, Z for a top-down 2D game.
    pub up: Vec3,
    pub patrol_goal: Vec3,
    /// Last known target position only. A hearing track never permits attack.
    pub target: Option<Track>,
    pub health_fraction: f32,
    pub weapon_ready: bool,
    pub alive: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct EnemyIntent {
    pub state: EnemyState,
    /// World goal for the host's collision-aware path planner.
    pub goal: Vec3,
    /// Unitless world movement for direct, visible engagement only.
    pub movement: Vec3,
    pub aim: Vec3,
    pub attack: bool,
    pub reload: bool,
    /// Host should invalidate its route after genuine movement stalls.
    pub replan: bool,
}
/// Constant-space controller. Identity supplies a deterministic strafe phase;
/// no random global state, heap allocation, renderer, transport or damage access.
pub struct EnemyController {
    config: EnemyConfig,
    identity: u64,
    target: Option<u64>,
    reaction: f32,
    cooldown: f32,
    phase: f32,
    previous: Option<Vec3>,
    stalled: f32,
    moving: bool,
}
impl EnemyController {
    pub fn new(config: EnemyConfig, identity: u64) -> Result<Self, AiError> {
        if !config.valid() {
            return Err(AiError::InvalidConfiguration);
        }
        Ok(Self {
            config,
            identity,
            target: None,
            reaction: 0.,
            cooldown: 0.,
            phase: (identity % 1024) as f32 * 1.618,
            previous: None,
            stalled: 0.,
            moving: false,
        })
    }
    pub fn reset(&mut self) {
        *self = Self::new(self.config, self.identity).unwrap();
    }
    pub fn tick(&mut self, input: EnemyInput, dt: f32) -> Result<EnemyIntent, AiError> {
        if !dt.is_finite()
            || !(0. ..=1.).contains(&dt)
            || !input.position.is_finite()
            || !input.forward.is_finite()
            || input.forward.length_squared() < 1e-8
            || !input.up.is_finite()
            || input.up.length_squared() < 1e-8
            || !input.patrol_goal.is_finite()
            || !input.health_fraction.is_finite()
            || !(0. ..=1.).contains(&input.health_fraction)
            || input
                .target
                .is_some_and(|t| !t.position.is_finite() || !t.age.is_finite() || t.age < 0.)
        {
            return Err(AiError::InvalidInput);
        }
        let up = input.up.normalize();
        let plane = |v: Vec3| v - up * v.dot(up);
        self.cooldown = (self.cooldown - dt).max(0.);
        self.phase = (self.phase + dt * 2.).rem_euclid(std::f32::consts::TAU);
        let target_id = input.target.map(|t| t.id);
        if self.target != target_id {
            self.reaction = 0.;
            self.target = target_id;
        }
        // Sparse sensing may retain awareness; firing still needs current sight.
        if input
            .target
            .is_some_and(|t| t.sense == Sense::Sight && t.age <= 0.25)
        {
            self.reaction += dt;
        } else {
            self.reaction = 0.;
        }
        let mut intent = EnemyIntent {
            state: EnemyState::Patrol,
            goal: input.patrol_goal,
            movement: Vec3::ZERO,
            aim: input.forward.normalize(),
            attack: false,
            reload: false,
            replan: false,
        };
        if !input.alive {
            self.reset();
            intent.state = EnemyState::Dead;
            intent.goal = input.position;
            return Ok(intent);
        }
        if let Some(track) = input.target {
            let delta = track.position - input.position;
            let flat = plane(delta);
            let toward = flat.normalize_or_zero();
            intent.goal = input.position + flat;
            intent.state = EnemyState::Investigate;
            if track.visible {
                intent.state = EnemyState::Engage;
                intent.aim =
                    turn_towards(intent.aim, delta, self.config.turn_radians_per_second * dt);
                let distance = flat.length();
                let advance = if distance > self.config.preferred_range * 1.2 {
                    0.7
                } else if distance < self.config.preferred_range * 0.7 {
                    -0.4
                } else {
                    0.
                };
                intent.movement =
                    toward * advance + toward.cross(up) * self.phase.sin() * self.config.strafe;
                if self.config.retreat_health > 0.
                    && input.health_fraction <= self.config.retreat_health
                {
                    intent.state = EnemyState::Retreat;
                    intent.goal = input.position - toward * self.config.preferred_range.max(2.);
                    intent.movement = -toward * 0.8;
                }
                intent.attack = intent.state == EnemyState::Engage
                    && input.weapon_ready
                    && delta.length() <= self.config.attack_range
                    && self.reaction >= self.config.reaction_seconds
                    && self.cooldown == 0.
                    && intent.aim.dot(delta.normalize_or_zero()) >= self.config.aim_tolerance.cos();
            } else if flat.length() < self.config.search_radius.max(0.5) {
                intent.state = EnemyState::Search;
                let axis = plane(Vec3::X).try_normalize().unwrap_or(Vec3::Z);
                intent.goal += (axis * self.phase.cos() + axis.cross(up) * self.phase.sin())
                    * self.config.search_radius;
            }
            if !input.weapon_ready {
                intent.state = EnemyState::Reload;
                intent.reload = true;
                intent.attack = false;
                // Back away while the ordinary weapon system handles reload.
                if track.visible {
                    intent.goal = input.position - toward * self.config.preferred_range.max(2.);
                    intent.movement = -toward * 0.6;
                }
            }
        } else if !input.weapon_ready {
            intent.reload = true;
            intent.state = EnemyState::Reload;
        }
        if !input.target.is_some_and(|t| t.visible) {
            intent.aim = turn_towards(
                intent.aim,
                plane(intent.goal - input.position),
                self.config.turn_radians_per_second * dt,
            );
        }
        if intent.attack {
            self.cooldown = self.config.attack_interval;
        }
        // Deliberate stationary aiming is not a stall. Ignore vertical jitter.
        if dt > 0.
            && self.moving
            && self
                .previous
                .is_some_and(|p| plane(input.position - p).length() < dt * 0.05)
        {
            self.stalled += dt;
        } else {
            self.stalled = 0.;
        }
        if self.stalled >= 0.75 {
            intent.replan = true;
            self.stalled = 0.;
        }
        self.moving = intent.movement.length_squared() > 0.01
            || plane(intent.goal - input.position).length_squared() > 1.;
        self.previous = Some(input.position);
        Ok(intent)
    }
}
/// Angular speed limit, including a deterministic antiparallel fallback.
pub fn turn_towards(from: Vec3, to: Vec3, max_angle: f32) -> Vec3 {
    let from = from.normalize_or_zero();
    let to = to.normalize_or_zero();
    if to == Vec3::ZERO || max_angle <= 0. {
        return from;
    }
    let cosine = from.dot(to).clamp(-1., 1.);
    let angle = cosine.acos();
    if angle <= max_angle {
        return to;
    }
    let tangent = (to - from * cosine).try_normalize().unwrap_or_else(|| {
        from.cross(if from.x.abs() < 0.9 { Vec3::X } else { Vec3::Y })
            .normalize_or_zero()
    });
    (from * max_angle.cos() + tangent * max_angle.sin()).normalize_or_zero()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{Candidate, Memory, MemorySettings};
    fn input(target: Option<Track>) -> EnemyInput {
        EnemyInput {
            position: Vec3::ZERO,
            forward: Vec3::Z,
            up: Vec3::Y,
            patrol_goal: Vec3::X * 10.,
            target,
            health_fraction: 1.,
            weapon_ready: true,
            alive: true,
        }
    }
    fn seen(memory: &mut Memory, position: Vec3, sense: Sense) -> Track {
        memory
            .observe(
                Candidate {
                    id: 1,
                    position,
                    groups: 1,
                },
                sense,
            )
            .unwrap();
        *memory.best(1).unwrap()
    }
    #[test]
    fn reacts_then_fires_only_with_fresh_sight_and_real_aim() {
        let mut memory = Memory::new(MemorySettings::default()).unwrap();
        let mut brain = EnemyController::new(EnemyConfig::ranged(), 1).unwrap();
        for frame in 0..100 {
            let mut i = input(Some(seen(&mut memory, Vec3::Z * 10., Sense::Sight)));
            // A target behind the actor cannot be hit by a snapped aim.
            i.forward = -Vec3::Z;
            assert!(!brain.tick(i, 1. / 60.).unwrap().attack);
            if frame > 30 {
                break;
            }
        }
        brain.reset();
        let mut shots = 0;
        for frame in 0..120 {
            let intent = brain
                .tick(
                    input(Some(seen(&mut memory, Vec3::Z * 10., Sense::Sight))),
                    1. / 60.,
                )
                .unwrap();
            if frame < 20 {
                assert!(!intent.attack);
            }
            shots += usize::from(intent.attack);
        }
        assert!((2..=3).contains(&shots));
        memory.advance(1. / 60.).unwrap();
        let hidden = *memory.best(1).unwrap();
        let hidden_position = hidden.position;
        for _ in 0..120 {
            let intent = brain.tick(input(Some(hidden)), 1. / 60.).unwrap();
            assert!(!intent.attack);
            assert_eq!(intent.goal, hidden_position);
        }
    }
    #[test]
    fn heard_target_investigates_searches_expires_and_never_fires() {
        let mut memory = Memory::new(MemorySettings::default()).unwrap();
        let mut brain = EnemyController::new(EnemyConfig::ranged(), 2).unwrap();
        let heard = seen(&mut memory, Vec3::Z * 10., Sense::Hearing);
        for _ in 0..100 {
            let intent = brain.tick(input(Some(heard)), 0.02).unwrap();
            assert_eq!(intent.state, EnemyState::Investigate);
            assert!(!intent.attack);
        }
        let mut i = input(Some(heard));
        i.position = heard.position;
        assert_eq!(brain.tick(i, 0.02).unwrap().state, EnemyState::Search);
        memory.advance(4.).unwrap();
        assert_eq!(
            brain
                .tick(input(memory.best(1).copied()), 0.02)
                .unwrap()
                .state,
            EnemyState::Patrol
        );
    }
    #[test]
    fn reload_retreat_and_death_override_attacks_and_reset_reaction() {
        let mut memory = Memory::new(MemorySettings::default()).unwrap();
        let mut brain = EnemyController::new(EnemyConfig::ranged(), 3).unwrap();
        let mut i = input(Some(seen(&mut memory, Vec3::Z * 10., Sense::Sight)));
        i.health_fraction = 0.2;
        let retreat = brain.tick(i, 0.5).unwrap();
        assert_eq!(retreat.state, EnemyState::Retreat);
        assert!(retreat.movement.z < 0. && retreat.goal.z < 0. && !retreat.attack);
        i.weapon_ready = false;
        let reload = brain.tick(i, 0.5).unwrap();
        assert!(reload.reload && !reload.attack);
        i.alive = false;
        assert_eq!(brain.tick(i, 0.5).unwrap().state, EnemyState::Dead);
        i.alive = true;
        i.health_fraction = 1.;
        i.weapon_ready = true;
        assert!(!brain.tick(i, 0.01).unwrap().attack);
    }
    #[test]
    fn plane_and_stall_work_for_2d_and_invalid_inputs_do_not_mutate() {
        let mut config = EnemyConfig::melee();
        config.attack_range = 30.;
        let mut brain = EnemyController::new(config, 4).unwrap();
        let mut i = input(None);
        i.up = Vec3::Z;
        i.forward = Vec3::Y;
        let mut replans = 0;
        for _ in 0..100 {
            let intent = brain.tick(i, 0.02).unwrap();
            replans += usize::from(intent.replan);
            assert_eq!(intent.aim.z, 0.);
        }
        assert_eq!(replans, 2);
        assert!(brain.tick(i, f32::NAN).is_err());
        config.turn_radians_per_second = -1.;
        assert!(EnemyController::new(config, 1).is_err());
        let angle = turn_towards(Vec3::Z, -Vec3::Z, 0.1).dot(Vec3::Z).acos();
        assert!((angle - 0.1).abs() < 1e-5);
    }
}
