use ferravex_math::{Vec2, Vec3};
use ferravex_play::{
    attributes::Vitals,
    camera::{Blocker, Sweep},
    feedback::{FeedbackConfig, FeedbackController, FeedbackEvent},
    gameplay::{
        LoopError,
        projectile::{HitResult, ProjectileBudget, ProjectileState, TargetOutcome, WallOutcome},
    },
};

// Frozen pre-249 implementation: differential compatibility oracle, including
// arithmetic/validation order and private storage layout, independent of reports.
#[derive(Clone, Copy, Debug)]
struct Legacy<const N: usize> {
    targets: [Option<u64>; N],
    pierces: u8,
    bounces: u8,
    alive: bool,
}
impl<const N: usize> Legacy<N> {
    fn new(pierces: u8, bounces: u8) -> Self {
        assert!(N > 0 && N <= 256 && usize::from(pierces) < N);
        Self {
            targets: [None; N],
            pierces,
            bounces,
            alive: true,
        }
    }
    fn target(&mut self, id: u64) -> HitResult {
        if !self.alive || self.targets.contains(&Some(id)) {
            return HitResult::Ignored;
        }
        let Some(slot) = self.targets.iter_mut().find(|s| s.is_none()) else {
            self.alive = false;
            return HitResult::Stop;
        };
        *slot = Some(id);
        if self.pierces == 0 {
            self.alive = false;
            HitResult::Stop
        } else {
            self.pierces -= 1;
            HitResult::Continue
        }
    }
    fn wall(&mut self, velocity: Vec2, normal: Vec2) -> Result<Option<Vec2>, LoopError> {
        if !velocity.is_finite() || velocity.abs().max_element() > 1e6 || !normal.is_finite() {
            return Err(LoopError::Invalid);
        }
        let normal = normal.try_normalize().ok_or(LoopError::Invalid)?;
        if !self.alive {
            return Ok(None);
        }
        let dot = velocity.dot(normal);
        if dot >= 0. {
            return Ok(Some(velocity));
        }
        if self.bounces == 0 {
            self.alive = false;
            return Ok(None);
        }
        self.bounces -= 1;
        Ok(Some(velocity - normal * (2. * dot)))
    }
    fn state(&self) -> ProjectileState {
        ProjectileState {
            remaining_pierces: self.pierces,
            remaining_bounces: self.bounces,
            accepted_targets: self.targets.iter().filter(|s| s.is_some()).count(),
            alive: self.alive,
        }
    }
}

fn bits(result: Result<Option<Vec2>, LoopError>) -> Result<Option<[u32; 2]>, LoopError> {
    result.map(|v| v.map(|v| v.to_array().map(f32::to_bits)))
}

fn assert_state<const N: usize>(budget: &ProjectileBudget<N>, legacy: &Legacy<N>) {
    assert_eq!(budget.state(), legacy.state());
    assert_eq!(budget.alive(), legacy.alive);
    for id in [0, 1, 7, 8, u64::MAX, 0x1_0000_0007, 0x2_0000_0007] {
        assert_eq!(budget.has_hit(id), legacy.targets.contains(&Some(id)));
    }
}

fn target_trace<const N: usize>(pierces: u8) {
    let mut legacy = Legacy::<N>::new(pierces, 3);
    let mut old_api = ProjectileBudget::<N>::new(pierces, 3).unwrap();
    let mut detailed = old_api;
    for id in [0, 0, u64::MAX, u64::MAX, 0x1_0000_0007, 0x2_0000_0007]
        .into_iter()
        .chain(1..=257)
    {
        let expected = legacy.target(id);
        assert_eq!(old_api.target(id), expected);
        let report = detailed.target_report(id);
        assert_eq!(report.hit_result(), expected);
        assert_eq!(report.accepted(), expected != HitResult::Ignored);
        assert_eq!(report.state, legacy.state());
        assert_ne!(report.outcome, TargetOutcome::StorageExhausted);
        assert_state(&old_api, &legacy);
        assert_state(&detailed, &legacy);
    }
}

#[test]
fn legacy_target_sequence_and_exact_capacity_are_preserved() {
    target_trace::<1>(0);
    for pierces in 0..3 {
        target_trace::<3>(pierces);
    }
    for pierces in 0..=255 {
        target_trace::<256>(pierces);
    }
    assert!(ProjectileBudget::<0>::new(0, 0).is_err());
    assert!(ProjectileBudget::<257>::new(0, 0).is_err());
    assert!(ProjectileBudget::<3>::new(3, 0).is_err());
    assert!(ProjectileBudget::<256>::new(255, 255).is_ok());
    assert_eq!(
        std::mem::size_of::<ProjectileBudget<256>>(),
        std::mem::size_of::<Legacy<256>>()
    );
}

#[test]
fn same_id_is_lifetime_deduplicated_and_generations_remain_host_owned() {
    let mut budget = ProjectileBudget::<3>::new(2, 2).unwrap();
    let first = 0x1_0000_0007;
    let next_generation = 0x2_0000_0007;
    let first_report = budget.target_report(first);
    assert_eq!(first_report.outcome, TargetOutcome::Accepted);
    assert_eq!(first_report.state.remaining_pierces, 1);
    assert_eq!(first_report.state.remaining_bounces, 2);
    let duplicate = budget.target_report(first);
    assert_eq!(duplicate.outcome, TargetOutcome::DuplicateTarget);
    assert!(!duplicate.accepted());
    assert_eq!(duplicate.state, first_report.state);
    assert_eq!(
        budget.target_report(next_generation).outcome,
        TargetOutcome::Accepted
    );
    assert!(budget.has_hit(first) && budget.has_hit(next_generation));
    assert_eq!(budget.state().remaining_pierces, 0);
    assert!(budget.alive()); // Zero pierces permits the final target.
    let last = budget.target_report(u64::MAX);
    assert_eq!(last.outcome, TargetOutcome::PierceExhausted);
    assert!(last.accepted());
    assert_eq!(last.state.accepted_targets, 3);
    assert!(!last.state.alive);
    for id in [first, next_generation, u64::MAX, 0] {
        let inactive = budget.target_report(id);
        assert_eq!(inactive.outcome, TargetOutcome::Inactive);
        assert!(!inactive.accepted());
        assert_eq!(inactive.state, last.state);
    }
    assert!(!budget.has_hit(0));
}

#[test]
fn wall_differential_preserves_bits_validation_and_exact_budget_consumption() {
    let tiny = f32::from_bits(1);
    let normals = [
        Vec2::X,
        -Vec2::X,
        Vec2::Y,
        -Vec2::Y,
        Vec2::new(3., -4.),
        Vec2::new(-0., 1.),
        Vec2::splat(f32::MAX),
        Vec2::new(f32::MAX, -f32::MAX),
        Vec2::new(tiny, -tiny),
        Vec2::new(f32::MAX, tiny),
        Vec2::ZERO,
        Vec2::new(-0., 0.),
        Vec2::NAN,
        Vec2::INFINITY,
    ];
    let velocities = [
        Vec2::ZERO,
        Vec2::new(-0., 0.),
        Vec2::X,
        -Vec2::X,
        Vec2::Y,
        Vec2::new(5., -7.),
        Vec2::splat(1e6),
        Vec2::new(-1e6, 1e6),
        Vec2::splat(-tiny),
        Vec2::splat(f32::from_bits(1_000_000_f32.to_bits() + 1)),
        Vec2::NAN,
        Vec2::INFINITY,
    ];
    for bounces in [0, 1, 2, 255] {
        for inactive in [false, true] {
            for normal in normals {
                for velocity in velocities {
                    let mut legacy = Legacy::<1>::new(0, bounces);
                    let mut old_api = ProjectileBudget::<1>::new(0, bounces).unwrap();
                    let mut detailed = old_api;
                    if inactive {
                        legacy.target(7);
                        old_api.target(7);
                        detailed.target(7);
                    }
                    let before = legacy.state();
                    let expected = legacy.wall(velocity, normal);
                    assert_eq!(bits(old_api.wall(velocity, normal)), bits(expected));
                    let report = detailed.wall_report(velocity, normal);
                    assert_eq!(bits(report.map(|r| r.velocity)), bits(expected));
                    if let Ok(report) = report {
                        assert_eq!(report.state, legacy.state());
                        let dot = velocity.dot(normal.try_normalize().unwrap());
                        let outcome = if inactive {
                            WallOutcome::Inactive
                        } else if dot == 0. {
                            WallOutcome::Tangent
                        } else if dot > 0. {
                            WallOutcome::Separating
                        } else if bounces == 0 {
                            WallOutcome::BounceExhausted
                        } else {
                            WallOutcome::Bounced
                        };
                        assert_eq!(report.outcome, outcome);
                        assert_eq!(
                            report.state.remaining_bounces,
                            bounces - u8::from(outcome == WallOutcome::Bounced)
                        );
                    } else {
                        assert_eq!(legacy.state(), before);
                    }
                    assert_state(&old_api, &legacy);
                    assert_state(&detailed, &legacy);
                }
            }
        }
    }
}

#[test]
fn invalid_contacts_are_transactional_before_and_after_inactivation() {
    for inactive in [false, true] {
        let mut budget = ProjectileBudget::<3>::new(1, 1).unwrap();
        budget.target_report(7);
        if inactive {
            budget.target_report(8);
        }
        let before = budget.state();
        for (velocity, normal) in [
            (Vec2::X, Vec2::ZERO),
            (Vec2::ZERO, Vec2::ZERO),
            (Vec2::NAN, Vec2::X),
            (Vec2::X, Vec2::NAN),
            (Vec2::X, Vec2::INFINITY),
            (Vec2::splat(-1_000_001.), Vec2::X),
        ] {
            assert_eq!(
                budget.wall_report(velocity, normal),
                Err(LoopError::Invalid)
            );
            assert_eq!(budget.wall(velocity, normal), Err(LoopError::Invalid));
            assert_eq!(budget.state(), before);
            assert!(budget.has_hit(7));
            assert_eq!(budget.has_hit(8), inactive);
        }
        let valid = budget.wall_report(-Vec2::X, Vec2::X).unwrap();
        assert_eq!(
            valid.outcome,
            if inactive {
                WallOutcome::Inactive
            } else {
                WallOutcome::Bounced
            }
        );
        assert_eq!(valid.state.remaining_bounces, u8::from(inactive));
    }
}

#[test]
fn mixed_contacts_share_budgets_and_preserve_reflection_without_clamping() {
    let mut budget = ProjectileBudget::<3>::new(1, 2).unwrap();
    let mut legacy = Legacy::<3>::new(1, 2);
    assert_eq!(budget.target(7), legacy.target(7));
    for (velocity, normal) in [
        (Vec2::X, Vec2::X),                     // separating
        (Vec2::new(-0., 0.), Vec2::X),          // tangent / signed zero
        (Vec2::splat(-1e6), Vec2::new(2., 1.)), // reflected x exceeds input bound
        (-Vec2::X, Vec2::X),
        (Vec2::Y, Vec2::X),  // tangent with no bounces still survives
        (-Vec2::X, Vec2::X), // exhaustion
        (-Vec2::X, Vec2::X), // inactive
    ] {
        let report = budget.wall_report(velocity, normal).unwrap();
        assert_eq!(
            bits(Ok(report.velocity)),
            bits(legacy.wall(velocity, normal))
        );
        assert_state(&budget, &legacy);
        if let Some(reflected) = report.velocity.filter(|_| velocity == Vec2::splat(-1e6)) {
            assert!(reflected.x > 1e6); // Legacy bounds input, not output.
            let before = budget.state();
            assert_eq!(budget.wall(reflected, normal), Err(LoopError::Invalid));
            assert_eq!(budget.state(), before);
        }
        assert_eq!(budget.target_report(7).hit_result(), legacy.target(7));
    }
    assert!(!budget.alive());
    assert_eq!(budget.state().remaining_pierces, 0);
    assert_eq!(budget.state().remaining_bounces, 0);
    assert_eq!(budget.state().accepted_targets, 1);
    assert_eq!(budget.target(8), legacy.target(8));
}

#[test]
fn new_lifetime_is_explicit_and_copy_forks_instead_of_clearing_history() {
    let mut budget = ProjectileBudget::<2>::new(1, 1).unwrap();
    budget.target_report(7);
    let mut fork = budget;
    assert_eq!(
        fork.target_report(7).outcome,
        TargetOutcome::DuplicateTarget
    );
    assert_eq!(
        fork.target_report(8).outcome,
        TargetOutcome::PierceExhausted
    );
    assert!(budget.alive());
    assert!(!budget.has_hit(8));
    // Failed fresh construction leaves the old lifetime intact.
    let before = budget.state();
    if let Ok(fresh) = ProjectileBudget::<2>::new(2, 1) {
        budget = fresh;
    }
    assert_eq!(budget.state(), before);
    budget = ProjectileBudget::<2>::new(1, 1).unwrap();
    assert!(!budget.has_hit(7));
    assert_eq!(budget.target_report(7).outcome, TargetOutcome::Accepted);
    assert_eq!(budget.state().remaining_bounces, 1);
}

#[test]
fn real_geometry_admission_damage_and_feedback_remain_separate() {
    let sweep = Sweep::new(Vec3::ZERO, Vec3::X * 10., 0.25).unwrap();
    let target = [Blocker::Sphere {
        center: Vec3::X * 5.,
        radius: 0.75,
    }];
    let miss = [Blocker::Sphere {
        center: Vec3::Y * 5.,
        radius: 0.75,
    }];
    let mut budget = ProjectileBudget::<3>::new(2, 0).unwrap();
    let mut health = Vitals::new(5., 0., 0., [0.; 8]).unwrap();
    let mut feedback = FeedbackController::<8>::new(FeedbackConfig::default(), 7).unwrap();
    let mut cues = 0;
    for (event_id, id, geometry, damage) in [
        (1, 7, &miss, 5.),               // No geometry contact: no admission.
        (2, 7, &target, -1.),            // Invalid damage: admission is not refunded.
        (3, 7, &target, 5.),             // Duplicate cannot retry damage.
        (4, 0x1_0000_0007, &target, 5.), // Distinct identity, lethal damage.
        (5, 0x2_0000_0007, &target, 5.), // Final admission, game rejects dead target.
        (6, 9, &target, 5.),             // Inactive.
    ] {
        let Some(t) = sweep.probe(&geometry[..], 0.).unwrap().hit_fraction else {
            continue;
        };
        let report = budget.target_report(id);
        let accepted = report.accepted() && health.health() > 0.;
        let changed = if accepted {
            health
                .damage(damage, 0)
                .is_ok_and(|damage| damage.health > 0.)
        } else {
            false
        };
        let event = FeedbackEvent::hit(sweep.start.lerp(sweep.end, t), 1.).unwrap();
        cues += usize::from(
            feedback
                .emit_if_accepted(event_id, changed, event)
                .unwrap()
                .is_some(),
        );
    }
    assert_eq!((health.health(), cues), (0., 1));
    assert_eq!(budget.state().accepted_targets, 3);
    assert!(!budget.has_hit(9));

    // Probe actual initial touching geometry in each supplied direction. Even
    // separating/tangent queries observe the boundary; only an approaching
    // velocity can admit a reflection. The query never changes budget state.
    let wall = [Blocker::Plane {
        normal: -Vec3::X,
        offset: -5.,
    }];
    let mut bounce = ProjectileBudget::<1>::new(0, 1).unwrap();
    let mut bounce_cues = 0;
    for (index, velocity) in [-Vec2::X, Vec2::Y, Vec2::ZERO, Vec2::X, Vec2::X, Vec2::X]
        .into_iter()
        .enumerate()
    {
        let start = Vec3::X * 4.75;
        let probe = Sweep::new(start, start + velocity.extend(0.) * 10., 0.25).unwrap();
        assert_eq!(probe.probe(&wall[..], 0.).unwrap().hit_fraction, Some(0.));
        let report = bounce.wall_report(velocity, -Vec2::X).unwrap();
        let accepted_reflection = report.outcome == WallOutcome::Bounced;
        let event = FeedbackEvent::hit(Vec3::X * 4.75, 1.).unwrap();
        bounce_cues += usize::from(
            feedback
                .emit_if_accepted(20 + index as u64, accepted_reflection, event)
                .unwrap()
                .is_some(),
        );
    }
    assert_eq!(bounce_cues, 1);
}
