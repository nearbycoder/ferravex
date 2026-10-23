//! Reusable enemy tactics. Feed only validated memory from sight/hearing; outputs
//! are intentions for ordinary movement, navigation and weapons, never damage.
//! Inspect [`EnemyController::status`] for reaction windup, recovery and target
//! loss without advancing simulation. Only the host's accepted weapon events
//! should produce combat feedback; an attack intention is not an accepted hit.
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
    /// A request for the ordinary weapon system on this tick, never a hit event.
    pub attack: bool,
    pub reload: bool,
    /// Host should invalidate its route after genuine movement stalls.
    pub replan: bool,
}
/// Phase of the existing reaction/cooldown timers, not a separate action queue.
/// Recovery takes precedence when both timers are running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnemyAttackPhase {
    /// No recent sight reaction is pending and no recovery remains.
    Inactive,
    /// Reacting to recent sight; see [`EnemyStatus::windup_remaining`].
    Windup,
    /// Reaction is complete. Current sight, range, aim and a positive tick are
    /// still required before an attack request can be emitted.
    Ready,
    /// Waiting after an attack request; see [`EnemyStatus::recovery_remaining`].
    Recovery,
}
/// Constant-size snapshot of the controller's latest successfully applied input.
/// Reading this snapshot never advances time or consumes a transition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EnemyStatus {
    /// Selected memory identity, including hearing or remembered sight.
    pub target: Option<u64>,
    /// Only a visible, age-zero sight track counts as current sight.
    pub target_visible: bool,
    /// Previous identity whose selection or current sight was lost on the last
    /// successful tick, or removed by [`EnemyController::cancel_target`].
    /// A remembered target may still be selected. Cleared on the next successful
    /// tick (including a zero delta); invalid inputs leave this unchanged.
    pub lost_target: Option<u64>,
    pub phase: EnemyAttackPhase,
    /// Seconds left in the current reaction, or zero when no reaction is pending.
    /// Recent sight can maintain reaction between sensing slots for 0.25 seconds;
    /// hearing, stale sight, reload and retreat cancel it.
    pub windup_remaining: f32,
    /// Seconds until another request is allowed, even if the target was lost.
    pub recovery_remaining: f32,
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
    target_visible: bool,
    lost_target: Option<u64>,
    attack_pending: bool,
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
            target_visible: false,
            lost_target: None,
            attack_pending: false,
        })
    }
    /// Clear target, reaction, recovery, movement history and inspection status.
    /// Restores the original identity's strafe phase, as for a fresh controller.
    /// Death applies this same reset before returning a dead intention.
    ///
    /// ```
    /// use ferravex_play::ai::{EnemyConfig, EnemyController};
    /// let mut enemy = EnemyController::new(EnemyConfig::ranged(), 7).unwrap();
    /// let initial = enemy.status();
    /// enemy.reset();
    /// assert_eq!(enemy.status(), initial);
    /// ```
    pub fn reset(&mut self) {
        *self = Self::new(self.config, self.identity).unwrap();
    }
    /// Inspect reaction windup and recovery without producing attack requests.
    /// The timers may overlap after a target change. A zero windup does not imply
    /// that firing is possible: inspect the phase and the returned tick intention.
    ///
    /// ```
    /// use ferravex_play::ai::{EnemyAttackPhase, EnemyConfig, EnemyController, EnemyStatus};
    /// let enemy = EnemyController::new(EnemyConfig::ranged(), 7).unwrap();
    /// let status: EnemyStatus = enemy.status();
    /// assert_eq!(status.phase, EnemyAttackPhase::Inactive);
    /// assert_eq!(status.target, None);
    /// assert_eq!(status.windup_remaining, 0.0);
    /// assert_eq!(status.recovery_remaining, 0.0);
    /// assert!(!status.target_visible);
    /// ```
    pub fn status(&self) -> EnemyStatus {
        let windup_remaining = if self.attack_pending {
            (self.config.reaction_seconds - self.reaction).max(0.)
        } else {
            0.
        };
        let phase = if self.cooldown > 0. {
            EnemyAttackPhase::Recovery
        } else if !self.attack_pending {
            EnemyAttackPhase::Inactive
        } else if windup_remaining > 0. {
            EnemyAttackPhase::Windup
        } else {
            EnemyAttackPhase::Ready
        };
        EnemyStatus {
            target: self.target,
            target_visible: self.target_visible,
            lost_target: self.lost_target,
            phase,
            windup_remaining,
            recovery_remaining: self.cooldown,
        }
    }
    /// Forget the selected target and pending reaction, returning its identity.
    /// Recovery is preserved, so cancellation cannot bypass attack cadence.
    /// Movement/stall history is cleared; the deterministic strafe phase remains.
    /// The host must also remove invalid tracks from its memory/input, or a later
    /// tick can select that identity again and begin a new reaction.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::ai::*;
    /// let mut memory = Memory::new(MemorySettings::default()).unwrap();
    /// memory.observe(Candidate { id: 9, position: Vec3::Z * 10.0, groups: 1 }, Sense::Sight).unwrap();
    /// let mut enemy = EnemyController::new(EnemyConfig::ranged(), 7).unwrap();
    /// enemy.tick(EnemyInput {
    ///     position: Vec3::ZERO, forward: Vec3::Z, up: Vec3::Y,
    ///     patrol_goal: Vec3::ZERO, target: memory.best(1).copied(),
    ///     health_fraction: 1.0, weapon_ready: true, alive: true,
    /// }, 0.1).unwrap();
    /// assert_eq!(enemy.cancel_target(), Some(9));
    /// assert_eq!(enemy.status().lost_target, Some(9));
    /// assert_eq!(enemy.status().phase, EnemyAttackPhase::Inactive);
    /// assert_eq!(enemy.status().target, None);
    /// memory.forget(9);
    /// ```
    pub fn cancel_target(&mut self) -> Option<u64> {
        let target = self.target.take();
        self.lost_target = target;
        self.target_visible = false;
        self.attack_pending = false;
        self.reaction = 0.;
        self.previous = None;
        self.stalled = 0.;
        self.moving = false;
        target
    }
    /// Apply a fixed simulation delta in `0..=1` seconds. Zero observes input
    /// transitions but advances no timers and never requests attack or replanning.
    /// Invalid input returns [`AiError::InvalidInput`] without changing state.
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
        if dt > 0. {
            self.phase = (self.phase + dt * 2.).rem_euclid(std::f32::consts::TAU);
        }
        let target_id = input.target.map(|t| t.id);
        let visible = input
            .target
            .is_some_and(|t| t.visible && t.sense == Sense::Sight && t.age == 0.);
        self.lost_target = self
            .target
            .filter(|id| Some(*id) != target_id || self.target_visible && !visible);
        self.target_visible = visible;
        if self.target != target_id {
            self.reaction = 0.;
            self.target = target_id;
        }
        let retreat =
            self.config.retreat_health > 0. && input.health_fraction <= self.config.retreat_health;
        // Sparse sensing may retain awareness; firing still needs current sight.
        self.attack_pending = input.weapon_ready
            && !retreat
            && input
                .target
                .is_some_and(|t| t.sense == Sense::Sight && t.age <= 0.25);
        if self.attack_pending {
            self.reaction = (self.reaction + dt).min(self.config.reaction_seconds);
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
            if visible {
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
                if retreat {
                    intent.state = EnemyState::Retreat;
                    intent.goal = input.position - toward * self.config.preferred_range.max(2.);
                    intent.movement = -toward * 0.8;
                }
                intent.attack = dt > 0.
                    && intent.state == EnemyState::Engage
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
                if visible {
                    intent.goal = input.position - toward * self.config.preferred_range.max(2.);
                    intent.movement = -toward * 0.6;
                }
            }
        } else if !input.weapon_ready {
            intent.reload = true;
            intent.state = EnemyState::Reload;
        }
        if !visible {
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
        if dt > 0. {
            if self.moving
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
        }
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
    fn inspection_config() -> EnemyConfig {
        EnemyConfig {
            reaction_seconds: 0.25,
            attack_interval: 0.5,
            ..EnemyConfig::ranged()
        }
    }
    fn assert_same_intent(a: EnemyIntent, b: EnemyIntent) {
        assert_eq!(
            (
                a.state, a.goal, a.movement, a.aim, a.attack, a.reload, a.replan
            ),
            (
                b.state, b.goal, b.movement, b.aim, b.attack, b.reload, b.replan
            )
        );
    }
    #[test]
    fn inspection_reports_windup_ready_and_recovery_without_advancing() {
        let mut memory = Memory::new(MemorySettings::default()).unwrap();
        let track = seen(&mut memory, Vec3::Z * 100., Sense::Sight);
        let mut brain = EnemyController::new(inspection_config(), 5).unwrap();
        assert_eq!(brain.status().phase, EnemyAttackPhase::Inactive);
        assert!(!brain.tick(input(Some(track)), 0.125).unwrap().attack);
        let status = brain.status();
        assert_eq!(status.target, Some(1));
        assert!(status.target_visible);
        assert_eq!(status.lost_target, None);
        assert_eq!(status.phase, EnemyAttackPhase::Windup);
        assert_eq!(status.windup_remaining, 0.125);
        assert_eq!(status.recovery_remaining, 0.);
        for _ in 0..10 {
            assert_eq!(brain.status(), status);
        }
        // A finished reaction does not bypass the range check.
        assert!(!brain.tick(input(Some(track)), 0.125).unwrap().attack);
        assert_eq!(brain.status().phase, EnemyAttackPhase::Ready);
        assert_eq!(brain.status().windup_remaining, 0.);
        let track = seen(&mut memory, Vec3::Z * 10., Sense::Sight);
        assert!(brain.tick(input(Some(track)), 0.125).unwrap().attack);
        assert_eq!(brain.status().phase, EnemyAttackPhase::Recovery);
        assert_eq!(brain.status().recovery_remaining, 0.5);
        assert!(!brain.tick(input(Some(track)), 0.125).unwrap().attack);
        assert_eq!(brain.status().recovery_remaining, 0.375);
    }
    #[test]
    fn target_changes_and_loss_restart_reaction_without_erasing_recovery() {
        let mut memory = Memory::new(MemorySettings::default()).unwrap();
        let mut track = seen(&mut memory, Vec3::Z * 10., Sense::Sight);
        let mut brain = EnemyController::new(inspection_config(), 5).unwrap();
        assert!(!brain.tick(input(Some(track)), 0.125).unwrap().attack);
        track.id = 2;
        assert!(!brain.tick(input(Some(track)), 0.125).unwrap().attack);
        assert_eq!(brain.status().lost_target, Some(1));
        assert_eq!(brain.status().windup_remaining, 0.125);
        assert!(brain.tick(input(Some(track)), 0.125).unwrap().attack);
        assert_eq!(brain.status().lost_target, None);
        track.id = 3;
        assert!(!brain.tick(input(Some(track)), 0.125).unwrap().attack);
        assert_eq!(brain.status().lost_target, Some(2));
        assert_eq!(brain.status().phase, EnemyAttackPhase::Recovery);
        assert_eq!(brain.status().windup_remaining, 0.125);
        assert_eq!(brain.status().recovery_remaining, 0.375);
        assert!(!brain.tick(input(None), 0.125).unwrap().attack);
        assert_eq!(brain.status().target, None);
        assert_eq!(brain.status().lost_target, Some(3));
        assert_eq!(brain.status().windup_remaining, 0.);
        assert_eq!(brain.status().recovery_remaining, 0.25);
        assert!(!brain.tick(input(None), 0.125).unwrap().attack);
        assert_eq!(brain.status().lost_target, None);
        assert!(!brain.tick(input(Some(track)), 0.125).unwrap().attack);
        assert_eq!(brain.status().phase, EnemyAttackPhase::Windup);
        assert!(brain.tick(input(Some(track)), 0.125).unwrap().attack);
    }
    #[test]
    fn zero_reaction_never_authorizes_hearing_hidden_or_stale_visible_tracks() {
        let mut memory = Memory::new(MemorySettings::default()).unwrap();
        let fresh = seen(&mut memory, Vec3::Z * 10., Sense::Sight);
        let config = EnemyConfig {
            reaction_seconds: 0.,
            ..inspection_config()
        };
        for (sense, visible, age) in [
            (Sense::Hearing, true, 0.),
            (Sense::Hearing, false, 0.),
            (Sense::Sight, false, 0.),
            (Sense::Sight, true, 0.125),
            (Sense::Sight, true, 0.5),
        ] {
            let mut brain = EnemyController::new(config, 5).unwrap();
            assert!(brain.tick(input(Some(fresh)), 0.125).unwrap().attack);
            let mut track = fresh;
            track.sense = sense;
            track.visible = visible;
            track.age = age;
            // The recovery expires, but fabricated visibility cannot permit fire.
            for tick in 0..3 {
                let intent = brain.tick(input(Some(track)), 0.5).unwrap();
                assert!(!intent.attack);
                assert_eq!(intent.state, EnemyState::Investigate);
                assert!(!brain.status().target_visible);
                assert_eq!(brain.status().lost_target, (tick == 0).then_some(1));
                assert_eq!(brain.status().recovery_remaining, 0.);
            }
        }
    }
    #[test]
    fn sparse_sight_preserves_recent_reaction_but_stale_sight_cancels_it() {
        let mut memory = Memory::new(MemorySettings::default()).unwrap();
        let fresh = seen(&mut memory, Vec3::Z * 10., Sense::Sight);
        let mut brain = EnemyController::new(inspection_config(), 5).unwrap();
        assert!(!brain.tick(input(Some(fresh)), 0.125).unwrap().attack);
        memory.advance(0.125).unwrap();
        let hidden = *memory.best(1).unwrap();
        assert!(!brain.tick(input(Some(hidden)), 0.125).unwrap().attack);
        assert_eq!(brain.status().lost_target, Some(1));
        assert_eq!(brain.status().phase, EnemyAttackPhase::Ready);
        assert!(brain.tick(input(Some(fresh)), 0.125).unwrap().attack);
        assert_eq!(brain.status().lost_target, None);
        memory.advance(0.375).unwrap();
        let stale = *memory.best(1).unwrap();
        assert!(!brain.tick(input(Some(stale)), 0.5).unwrap().attack);
        assert_eq!(brain.status().phase, EnemyAttackPhase::Inactive);
        assert!(!brain.tick(input(Some(fresh)), 0.125).unwrap().attack);
        assert_eq!(brain.status().windup_remaining, 0.125);
        assert!(brain.tick(input(Some(fresh)), 0.125).unwrap().attack);
    }
    #[test]
    fn reload_retreat_and_hearing_cancel_pending_windup() {
        let mut memory = Memory::new(MemorySettings::default()).unwrap();
        let track = seen(&mut memory, Vec3::Z * 10., Sense::Sight);
        let ready = input(Some(track));
        let mut reload = ready;
        reload.weapon_ready = false;
        let mut retreat = ready;
        retreat.health_fraction = 0.1;
        let mut hearing = ready;
        hearing.target.as_mut().unwrap().sense = Sense::Hearing;
        hearing.target.as_mut().unwrap().visible = false;
        for canceled in [reload, retreat, hearing] {
            let mut brain = EnemyController::new(inspection_config(), 5).unwrap();
            assert!(!brain.tick(ready, 0.125).unwrap().attack);
            assert!(!brain.tick(canceled, 0.5).unwrap().attack);
            assert_eq!(brain.status().phase, EnemyAttackPhase::Inactive);
            assert_eq!(brain.status().windup_remaining, 0.);
            assert!(!brain.tick(ready, 0.125).unwrap().attack);
            assert_eq!(brain.status().windup_remaining, 0.125);
            assert!(brain.tick(ready, 0.125).unwrap().attack);
        }
    }
    #[test]
    fn explicit_cancellation_preserves_cadence_and_clears_stall_history() {
        let mut memory = Memory::new(MemorySettings::default()).unwrap();
        let ready = input(Some(seen(&mut memory, Vec3::Z * 10., Sense::Sight)));
        let mut brain = EnemyController::new(inspection_config(), 5).unwrap();
        assert!(!brain.tick(ready, 0.125).unwrap().attack);
        assert_eq!(brain.cancel_target(), Some(1));
        assert_eq!(brain.status().phase, EnemyAttackPhase::Inactive);
        assert_eq!(brain.status().target, None);
        assert_eq!(brain.status().lost_target, Some(1));
        assert!(!brain.tick(ready, 0.125).unwrap().attack);
        assert!(brain.tick(ready, 0.125).unwrap().attack);
        let phase = brain.phase;
        assert_eq!(brain.cancel_target(), Some(1));
        assert_eq!(brain.status().phase, EnemyAttackPhase::Recovery);
        assert_eq!(brain.status().windup_remaining, 0.);
        assert_eq!(brain.status().recovery_remaining, 0.5);
        assert_eq!(brain.phase, phase);
        assert_eq!(brain.previous, None);
        assert!(!brain.moving);
        assert_eq!(brain.stalled, 0.);
        assert_eq!(brain.cancel_target(), None);
        assert_eq!(brain.status().lost_target, None);
        for _ in 0..3 {
            assert!(!brain.tick(ready, 0.125).unwrap().attack);
        }
        assert!(brain.tick(ready, 0.125).unwrap().attack);
    }
    #[test]
    fn zero_delta_never_fires_or_advances_recovery_or_stall_time() {
        let mut memory = Memory::new(MemorySettings::default()).unwrap();
        let ready = input(Some(seen(&mut memory, Vec3::Z * 10., Sense::Sight)));
        let mut brain = EnemyController::new(
            EnemyConfig {
                reaction_seconds: 0.,
                ..inspection_config()
            },
            5,
        )
        .unwrap();
        for _ in 0..10 {
            assert!(!brain.tick(ready, 0.).unwrap().attack);
            assert_eq!(brain.status().phase, EnemyAttackPhase::Ready);
            assert_eq!(brain.status().recovery_remaining, 0.);
        }
        assert!(brain.tick(ready, 0.125).unwrap().attack);
        let status = brain.status();
        for _ in 0..10 {
            assert!(!brain.tick(ready, 0.).unwrap().attack);
            assert_eq!(brain.status(), status);
        }
        // Zero deltas can still cancel invalidated targets without advancing time.
        assert!(!brain.tick(input(None), 0.).unwrap().attack);
        assert_eq!(brain.status().lost_target, Some(1));
        assert_eq!(brain.status().recovery_remaining, 0.5);
        brain.reset();
        assert!(!brain.tick(input(None), 0.25).unwrap().replan);
        assert!(!brain.tick(input(None), 0.5).unwrap().replan);
        assert!(!brain.tick(input(None), 0.).unwrap().replan);
        assert!(brain.tick(input(None), 0.25).unwrap().replan);
    }
    #[test]
    fn invalid_inputs_preserve_status_and_future_intentions() {
        let mut memory = Memory::new(MemorySettings::default()).unwrap();
        let ready = input(Some(seen(&mut memory, Vec3::Z * 10., Sense::Sight)));
        let mut brain = EnemyController::new(inspection_config(), 5).unwrap();
        let mut reference = EnemyController::new(inspection_config(), 5).unwrap();
        for i in [ready, input(None)] {
            assert_same_intent(
                brain.tick(i, 0.125).unwrap(),
                reference.tick(i, 0.125).unwrap(),
            );
        }
        let status = brain.status();
        for dt in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
            assert!(matches!(brain.tick(ready, dt), Err(AiError::InvalidInput)));
            assert_eq!(brain.status(), status);
        }
        let mut invalid_age = ready;
        invalid_age.target.as_mut().unwrap().age = f64::NAN;
        let mut invalid_position = ready;
        invalid_position.target.as_mut().unwrap().position.x = f32::INFINITY;
        let mut invalid_health = ready;
        invalid_health.health_fraction = -1.;
        for i in [invalid_age, invalid_position, invalid_health] {
            assert!(matches!(brain.tick(i, 0.125), Err(AiError::InvalidInput)));
            assert_eq!(brain.status(), status);
        }
        for _ in 0..12 {
            assert_same_intent(
                brain.tick(ready, 0.125).unwrap(),
                reference.tick(ready, 0.125).unwrap(),
            );
            assert_eq!(brain.status(), reference.status());
        }
    }
    #[test]
    fn reset_and_death_reproduce_fresh_controller_intentions() {
        let mut memory = Memory::new(MemorySettings::default()).unwrap();
        let ready = input(Some(seen(&mut memory, Vec3::Z * 10., Sense::Sight)));
        let mut brain = EnemyController::new(inspection_config(), 987).unwrap();
        for death in [false, true] {
            for _ in 0..10 {
                brain.tick(ready, 0.125).unwrap();
            }
            if death {
                let mut dead = ready;
                dead.alive = false;
                let intent = brain.tick(dead, 0.125).unwrap();
                assert_eq!(intent.state, EnemyState::Dead);
                assert!(!intent.attack && !intent.reload && !intent.replan);
                assert_eq!(intent.movement, Vec3::ZERO);
            } else {
                brain.reset();
            }
            let mut reference = EnemyController::new(inspection_config(), 987).unwrap();
            assert_eq!(brain.status(), reference.status());
            for tick in 0..32 {
                let i = if tick % 7 == 0 { input(None) } else { ready };
                assert_same_intent(
                    brain.tick(i, 0.125).unwrap(),
                    reference.tick(i, 0.125).unwrap(),
                );
                assert_eq!(brain.status(), reference.status());
            }
        }
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
