//! Per-projectile target deduplication, piercing and ricochet budgets.
//!
//! This helper admits **caller-confirmed** contacts; it performs no collision
//! query, target lookup, damage or feedback. A `u64` is an opaque host-provided
//! generation identity (for ECS entities, use the existing `Entity::to_bits()`).
//! The host must reject stale identities and supply a different ID for a new
//! generation. Equal IDs are admitted at most once over this budget's lifetime;
//! distinct generations can both be admitted. Even rejected game-owned damage
//! consumes an admitted contact: there is no retry or refund here.
//!
//! Create a fresh budget with [`ProjectileBudget::new`] for a new projectile or
//! run. There is no in-place reset. Copying a budget forks its admission history;
//! it does not coordinate two authoritative projectiles or provide a rollback
//! ledger. Operations allocate nothing and inspect at most `N <= 256` slots.
//!
//! This runnable recipe uses the existing first-party sphere sweep in the XY
//! plane, then budget admission, then actual health acceptance and feedback.
//! The second generation is immune, so its nominal admission emits no feedback.
//! This is a single-contact recipe, not a multi-contact traversal/solver.
//!
//! ```rust
//! use ferravex_math::Vec3;
//! use ferravex_play::{
//!     attributes::Vitals,
//!     camera::{Blocker, Sweep},
//!     feedback::{FeedbackConfig, FeedbackController, FeedbackEvent},
//!     gameplay::projectile::{ProjectileBudget, TargetOutcome},
//! };
//!
//! fn main() {
//!     let mut budget = ProjectileBudget::<3>::new(2, 1).unwrap();
//!     let mut health = Vitals::new(20., 0., 0., [0.; 8]).unwrap();
//!     let mut immune = Vitals::new(20., 0., 0., [1.; 8]).unwrap();
//!     let mut feedback = FeedbackController::<4>::new(FeedbackConfig::default(), 7).unwrap();
//!     let sweep = Sweep::new(Vec3::ZERO, Vec3::X * 10., 0.25).unwrap();
//!     let target = [Blocker::Sphere { center: Vec3::X * 5., radius: 0.75 }];
//!     let miss = [Blocker::Sphere { center: Vec3::Y * 5., radius: 0.75 }];
//!     assert!(sweep.probe(&miss[..], 0.).unwrap().hit_fraction.is_none());
//!     assert_eq!(budget.state().accepted_targets, 0);
//!     let mut cues = 0;
//!     // IDs are supplied by the host, not inferred from geometric witnesses.
//!     for (event_id, id) in [(1, 0x1_0000_0007), (2, 0x1_0000_0007), (3, 0x2_0000_0007)] {
//!         let contact = sweep.probe(&target[..], 0.).unwrap();
//!         if let Some(fraction) = contact.hit_fraction {
//!             assert!((fraction - 0.4).abs() < 1e-6);
//!             let report = budget.target_report(id);
//!             let mut accepted_damage = false;
//!             if report.accepted() {
//!                 let vitals = if event_id == 3 { &mut immune } else { &mut health };
//!                 let damage = vitals.damage(5., 0).unwrap();
//!                 accepted_damage = damage.health > 0. || damage.shield > 0.;
//!             } else {
//!                 assert_eq!(report.outcome, TargetOutcome::DuplicateTarget);
//!             }
//!             let event = FeedbackEvent::hit(sweep.start.lerp(sweep.end, fraction), 1.).unwrap();
//!             cues += usize::from(feedback.emit_if_accepted(event_id, accepted_damage, event)
//!                 .unwrap().is_some());
//!         }
//!     }
//!     assert_eq!((health.health(), immune.health(), cues), (15., 20., 1));
//!     assert_eq!(budget.state().accepted_targets, 2);
//!     assert!(budget.has_hit(0x2_0000_0007));
//! }
//! ```
use super::LoopError;
use ferravex_math::Vec2;
/// Legacy admission result. `Stop` normally includes an accepted final hit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitResult {
    Ignored,
    Continue,
    Stop,
}
/// Read-only state **after** a reported contact. See the module's runnable recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectileState {
    /// Additional target contacts that may continue travel. Zero still permits
    /// one final target admission if alive; duplicates never spend this count.
    pub remaining_pierces: u8,
    /// Remaining reflections. Zero stops only on an approaching wall contact.
    pub remaining_bounces: u8,
    /// Number of lifetime target identities retained, including the final hit.
    pub accepted_targets: usize,
    pub alive: bool,
}

/// Budget policy only; acceptance does not prove game-owned damage or validity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetOutcome {
    /// Checked before duplicate identity, including a repeat of the final hit.
    Inactive,
    DuplicateTarget,
    /// Admitted and consumed one pierce; travel can continue.
    Accepted,
    /// Admitted the final target with no pierce remaining; projectile stopped.
    PierceExhausted,
    /// Not admitted; projectile stopped. Unreachable through the public API:
    /// `pierces < N` guarantees room for every admitted target plus the final hit.
    /// Retained only to describe the legacy defensive storage guard honestly.
    StorageExhausted,
}

/// Target admission and resulting budgets. See the module's runnable recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetReport {
    pub outcome: TargetOutcome,
    pub state: ProjectileState,
}
impl TargetReport {
    /// Whether this contact was nominally admitted, including the final hit.
    /// Apply actual target/damage rules before emitting feedback.
    pub fn accepted(self) -> bool {
        matches!(
            self.outcome,
            TargetOutcome::Accepted | TargetOutcome::PierceExhausted
        )
    }

    /// Exact legacy mapping, including the defensive storage stop.
    pub fn hit_result(self) -> HitResult {
        match self.outcome {
            TargetOutcome::Inactive | TargetOutcome::DuplicateTarget => HitResult::Ignored,
            TargetOutcome::Accepted => HitResult::Continue,
            TargetOutcome::PierceExhausted | TargetOutcome::StorageExhausted => HitResult::Stop,
        }
    }
}

/// Wall budget policy after vector validation, not a collision/normal query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallOutcome {
    Inactive,
    /// Positive dot product; unchanged velocity and no bounce consumed.
    Separating,
    /// Exactly zero dot product (including stationary velocity and signed zero).
    /// Unchanged velocity and no bounce consumed; no tolerance is applied.
    Tangent,
    /// Approaching with no bounces; stopped without reflection.
    BounceExhausted,
    /// One bounce consumed using the legacy perfect-reflection arithmetic.
    Bounced,
}

/// Wall outcome and resulting budgets. `velocity` is exactly the legacy wall
/// return value: `None` only for inactive or exhausted bounce. Separating and
/// tangent outcomes are not accepted impacts and must not trigger hit feedback.
/// A bounced report is nominal budget admission; the host owns applying the
/// response and accepting any game-owned impact/damage before feedback.
/// See [`ProjectileBudget::wall_report`] for a runnable example.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallReport {
    pub outcome: WallOutcome,
    pub velocity: Option<Vec2>,
    pub state: ProjectileState,
}

/// Bounded, allocation-free policy for one projectile lifetime. See module recipe.
#[derive(Clone, Copy, Debug)]
pub struct ProjectileBudget<const N: usize> {
    targets: [Option<u64>; N],
    pierces: u8,
    bounces: u8,
    alive: bool,
}
impl<const N: usize> ProjectileBudget<N> {
    /// Requires `1 <= N <= 256` and `pierces < N`. No change to authored defaults.
    pub fn new(pierces: u8, bounces: u8) -> Result<Self, LoopError> {
        if N == 0 || N > 256 || usize::from(pierces) >= N {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            targets: [None; N],
            pierces,
            bounces,
            alive: true,
        })
    }
    pub fn alive(&self) -> bool {
        self.alive
    }

    /// Observe budgets without granting contacts or changing lifetime history.
    pub fn state(&self) -> ProjectileState {
        ProjectileState {
            remaining_pierces: self.pierces,
            remaining_bounces: self.bounces,
            accepted_targets: self.targets.iter().filter(|slot| slot.is_some()).count(),
            alive: self.alive,
        }
    }

    /// Whether this exact host generation identity has been admitted, even if
    /// now inactive. This does not establish that the target is still valid.
    pub fn has_hit(&self, id: u64) -> bool {
        self.targets.contains(&Some(id))
    }

    /// Supply a unique generational target ID after confirmed collision. Legacy
    /// result; use [`Self::target_report`] to distinguish admission/stop reasons.
    pub fn target(&mut self, id: u64) -> HitResult {
        self.target_report(id).hit_result()
    }

    /// Admit one caller-confirmed target with the legacy ordering and budgets.
    /// An inactive projectile wins over a duplicate ID. The final accepted hit
    /// stops travel without decrementing the already-zero pierce count.
    /// See the module recipe for geometry, damage and feedback composition.
    pub fn target_report(&mut self, id: u64) -> TargetReport {
        let outcome = if !self.alive {
            TargetOutcome::Inactive
        } else if self.has_hit(id) {
            TargetOutcome::DuplicateTarget
        } else if let Some(slot) = self.targets.iter_mut().find(|s| s.is_none()) {
            *slot = Some(id);
            if self.pierces == 0 {
                self.alive = false;
                TargetOutcome::PierceExhausted
            } else {
                self.pierces -= 1;
                TargetOutcome::Accepted
            }
        } else {
            self.alive = false;
            TargetOutcome::StorageExhausted
        };
        TargetReport {
            outcome,
            state: self.state(),
        }
    }
    /// Separating contacts spend no bounce; invalid vectors leave state intact.
    pub fn wall(&mut self, velocity: Vec2, normal: Vec2) -> Result<Option<Vec2>, LoopError> {
        Ok(self.wall_report(velocity, normal)?.velocity)
    }

    /// Validate vectors, then admit a caller-confirmed wall contact. Velocity
    /// must be finite with each component in `[-1e6, 1e6]`; normal must be finite
    /// and nonzero. Normalization uses the existing `Vec2::try_normalize`, even
    /// for huge finite or subnormal normals. Invalid input returns
    /// `LoopError::Invalid` before any mutation, **including when inactive**.
    /// There is no epsilon, restitution change, post-reflection speed clamp or
    /// extra numerical rejection. `dot >= 0` preserves the supplied velocity.
    ///
    /// ```rust
    /// use ferravex_math::Vec2;
    /// use ferravex_play::gameplay::{LoopError, projectile::{ProjectileBudget, WallOutcome}};
    /// fn main() {
    ///     let mut budget = ProjectileBudget::<1>::new(0, 1).unwrap();
    ///     let tangent = budget.wall_report(Vec2::Y, Vec2::X).unwrap();
    ///     assert_eq!(tangent.outcome, WallOutcome::Tangent);
    ///     assert_eq!(tangent.state.remaining_bounces, 1);
    ///     let bounce = budget.wall_report(-Vec2::X, Vec2::X * f32::MAX).unwrap();
    ///     assert_eq!((bounce.outcome, bounce.velocity), (WallOutcome::Bounced, Some(Vec2::X)));
    ///     let stop = budget.wall_report(-Vec2::X, Vec2::X).unwrap();
    ///     assert_eq!(stop.outcome, WallOutcome::BounceExhausted);
    ///     assert!(!stop.state.alive);
    ///     assert_eq!(budget.wall_report(Vec2::X, Vec2::ZERO), Err(LoopError::Invalid));
    /// }
    /// ```
    pub fn wall_report(&mut self, velocity: Vec2, normal: Vec2) -> Result<WallReport, LoopError> {
        if !velocity.is_finite() || velocity.abs().max_element() > 1e6 || !normal.is_finite() {
            return Err(LoopError::Invalid);
        }
        let normal = normal.try_normalize().ok_or(LoopError::Invalid)?;
        if !self.alive {
            return Ok(WallReport {
                outcome: WallOutcome::Inactive,
                velocity: None,
                state: self.state(),
            });
        }
        let dot = velocity.dot(normal);
        let (outcome, velocity) = if dot >= 0. {
            (
                if dot == 0. {
                    WallOutcome::Tangent
                } else {
                    WallOutcome::Separating
                },
                Some(velocity),
            )
        } else if self.bounces == 0 {
            self.alive = false;
            (WallOutcome::BounceExhausted, None)
        } else {
            self.bounces -= 1;
            (WallOutcome::Bounced, Some(velocity - normal * (2. * dot)))
        };
        Ok(WallReport {
            outcome,
            velocity,
            state: self.state(),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_targets_and_separating_contacts_cannot_spend_twice() {
        let mut p = ProjectileBudget::<3>::new(1, 1).unwrap();
        assert_eq!(p.target(8), HitResult::Continue);
        assert_eq!(p.target(8), HitResult::Ignored);
        assert_eq!(p.wall(Vec2::X, Vec2::X).unwrap(), Some(Vec2::X));
        assert!(p.wall(-Vec2::X, Vec2::ZERO).is_err());
        assert_eq!(p.wall(-Vec2::X, Vec2::X).unwrap(), Some(Vec2::X));
        assert_eq!(p.target(9), HitResult::Stop);
        assert!(!p.alive());
        assert_eq!(p.target(10), HitResult::Ignored);
        let mut p = ProjectileBudget::<1>::new(0, 0).unwrap();
        assert_eq!(p.wall(-Vec2::X, Vec2::X).unwrap(), None);
    }
}
