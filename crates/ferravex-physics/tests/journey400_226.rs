use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAllocator;
thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
    static FAIL_AFTER: Cell<Option<usize>> = const { Cell::new(None) };
}
fn allocation_fails() -> bool {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(n) = count.get() {
            count.set(Some(n + 1));
        }
    });
    FAIL_AFTER
        .try_with(|remaining| match remaining.get() {
            Some(0) => true,
            Some(n) => {
                remaining.set(Some(n - 1));
                false
            }
            None => false,
        })
        .unwrap_or(false)
}
// SAFETY: Forward System's unchanged allocation contract; thread-local counters
// do not allocate. A requested null result exercises fallible Vec reservation.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if allocation_fails() {
            std::ptr::null_mut()
        } else {
            unsafe { System.alloc(layout) }
        }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if allocation_fails() {
            std::ptr::null_mut()
        } else {
            unsafe { System.alloc_zeroed(layout) }
        }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if allocation_fails() {
            std::ptr::null_mut()
        } else {
            unsafe { System.realloc(ptr, layout, size) }
        }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

macro_rules! impact_tests {
    ($dimension:ident, $box:expr, $coords:expr) => {
        mod $dimension {
            use super::{ALLOCATIONS, FAIL_AFTER};
            use ferravex_physics::$dimension::*;
            use ferravex_physics::{
                ContactEvent, ImpactRecordingError, ImpactSettings, MAX_IMPACT_RECORDS,
                SolverSettings,
            };
            const DT: f32 = 1. / 60.;

            fn enable(w: &mut PhysicsWorld, capacity: usize) {
                let mut settings = w.impact_settings().unwrap_or_default();
                settings.capacity = capacity;
                w.set_impact_settings(Some(settings)).unwrap();
            }
            fn corridor(
                capacity: Option<usize>,
            ) -> (PhysicsWorld, RigidBodyHandle, [ColliderHandle; 3]) {
                let mut w = PhysicsWorld::default();
                if let Some(capacity) = capacity {
                    enable(&mut w, capacity);
                }
                let (_, left) = w.insert(
                    RigidBodyBuilder::fixed().translation(-Vector::X),
                    $box(0.05, 5.).friction(0.).restitution(1.),
                );
                let (_, right) = w.insert(
                    RigidBodyBuilder::fixed().translation(Vector::X),
                    $box(0.05, 5.).friction(0.).restitution(1.),
                );
                let (ball, collider) = w.insert(
                    RigidBodyBuilder::dynamic()
                        .linvel(Vector::X * 1600.)
                        .ccd_enabled(true),
                    ColliderBuilder::ball(0.1).friction(0.).restitution(1.),
                );
                (w, ball, [left, right, collider])
            }
            fn approaching() -> (PhysicsWorld, RigidBodyHandle, [ColliderHandle; 2]) {
                let mut w = PhysicsWorld::default();
                enable(&mut w, 8);
                let (_, fixed) = w.insert(RigidBodyBuilder::fixed(), ColliderBuilder::ball(0.5));
                let (body, collider) = w.insert(
                    RigidBodyBuilder::dynamic()
                        .translation(-Vector::X * 1.1)
                        .linvel(Vector::X * 10.),
                    ColliderBuilder::ball(0.5),
                );
                (w, body, [fixed, collider])
            }

            #[test]
            fn real_ccd_transient_and_repeated_substeps_retain_pair_peaks_not_sums() {
                let (mut w, body, [left, right, ball]) = corridor(Some(8));
                let expected = w.bodies[body].mass() * 3200.;
                let stats = w.step(DT);
                assert_eq!(stats.substeps, 8);
                assert_eq!(stats.ccd_impacts, 8);
                assert_eq!(w.impacts().len(), 2);
                assert_eq!(w.impact_dropped(), 0);
                for (record, wall) in w.impacts().iter().zip([left, right]) {
                    assert_eq!((record.collider1, record.collider2), (wall, ball));
                    assert!(
                        (record.peak_normal_impulse - expected).abs() < expected * 1e-4,
                        "{record:?}, expected {expected}"
                    );
                    assert!((record.approach_speed - 1600.).abs() < 0.1);
                    assert!(record.point.is_finite() && record.normal.is_finite());
                    assert!((record.normal.length() - 1.).abs() < 1e-5);
                }
                // The first wall hit is transient: absent from the final cache,
                // yet both pairs have only one record after four hits apiece.
                assert_eq!(w.contact_manifolds().count(), 1);
                let cached = w.contact_manifolds().next().unwrap();
                assert_eq!((cached.collider1, cached.collider2), (left, ball));
                assert!(w.impacts().iter().any(|r| r.collider1 == right));
                assert_eq!(w.contacts().count(), 2);
                assert_eq!(
                    w.contact_events(),
                    &[
                        ContactEvent::Started(left, ball),
                        ContactEvent::Started(right, ball),
                    ]
                );
            }

            #[test]
            fn cacheless_solving_records_impacts_and_rejects_resting_support() {
                let (mut w, _, _) = corridor(Some(8));
                w.set_solver_settings(SolverSettings {
                    max_cached_manifolds: 0,
                    warm_start: false,
                    ..Default::default()
                })
                .unwrap();
                let stats = w.step(DT);
                assert!(stats.cache_dropped > 0);
                assert_eq!(w.contact_manifolds().count(), 0);
                assert_eq!(w.impacts().len(), 2);
                let mut rest = PhysicsWorld::new($coords(0., -1000.));
                enable(&mut rest, 8);
                rest.set_solver_settings(SolverSettings {
                    max_cached_manifolds: 0,
                    warm_start: false,
                    allow_sleep: false,
                    ..Default::default()
                })
                .unwrap();
                rest.insert(
                    RigidBodyBuilder::fixed().translation(-Vector::Y),
                    $box(5., 0.5),
                );
                rest.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(0.5));
                for _ in 0..30 {
                    assert!(rest.step(DT).contact_points > 0);
                    assert!(rest.impacts().is_empty());
                }
            }

            #[test]
            fn zero_one_and_full_buffers_have_explicit_pair_substep_rejections() {
                for (capacity, records, drops) in [(0, 0, 8), (1, 1, 4), (2, 2, 0)] {
                    let (mut w, _, [_, right, _]) = corridor(Some(capacity));
                    w.step(DT);
                    assert_eq!(w.impacts().len(), records);
                    assert_eq!(w.impact_dropped(), drops);
                    if capacity == 1 {
                        assert_eq!(w.impacts()[0].collider1, right);
                    }
                    w.clear_impacts();
                    assert!(w.impacts().is_empty());
                    assert_eq!(w.impact_dropped(), 0);
                }
            }

            #[test]
            fn recording_leaves_exact_physics_contacts_stats_and_sleep_unchanged() {
                let (mut off, a, _) = corridor(None);
                let (mut on, b, _) = corridor(Some(1));
                for _ in 0..64 {
                    let before = format!("{:?}", off.step(DT));
                    let after = format!("{:?}", on.step(DT));
                    assert_eq!(before, after);
                    assert_eq!(format!("{:?}", off.bodies), format!("{:?}", on.bodies));
                    assert_eq!(
                        format!("{:?}", off.colliders),
                        format!("{:?}", on.colliders)
                    );
                    assert_eq!(off.bodies[a].translation(), on.bodies[b].translation());
                    assert_eq!(off.contact_events(), on.contact_events());
                    assert_eq!(
                        format!("{:?}", off.contacts().collect::<Vec<_>>()),
                        format!("{:?}", on.contacts().collect::<Vec<_>>())
                    );
                    assert_eq!(off.sleep_stats(), on.sleep_stats());
                    assert!(off.impacts().is_empty());
                }
            }

            #[test]
            fn gravity_support_and_warm_start_replay_are_not_new_impacts() {
                for warm_start in [true, false] {
                    for gravity in [9.81, 1000.] {
                        let mut w = PhysicsWorld::new($coords(0., -gravity));
                        enable(&mut w, 8);
                        let mut options = w.impact_settings().unwrap();
                        options.min_approach_speed = 0.01;
                        w.set_impact_settings(Some(options)).unwrap();
                        w.set_solver_settings(SolverSettings {
                            warm_start,
                            allow_sleep: false,
                            ..Default::default()
                        })
                        .unwrap();
                        w.insert(
                            RigidBodyBuilder::fixed().translation(-Vector::Y),
                            $box(5., 0.5),
                        );
                        let (supported, _) =
                            w.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(0.5));
                        let mut warm_points = 0;
                        for _ in 0..120 {
                            warm_points += w.step(DT).warm_started_points;
                            assert!(w.impacts().is_empty(), "{warm_start}: {:?}", w.impacts());
                            assert_eq!(w.impact_dropped(), 0);
                            assert!(
                                w.contact_manifolds()
                                    .flat_map(|m| m.points)
                                    .any(|p| p.normal_impulse > 0.)
                            );
                        }
                        // Existing solver refuses warm start when the pre-warm
                        // approach is below -1; high gravity takes that cold path.
                        assert_eq!(warm_points > 0, warm_start && gravity < 10.);
                        // Real new approach on the same still-contacting pair is
                        // accepted; a prior loaded support point is not blacklisted.
                        w.bodies[supported].set_linvel(-Vector::Y * 20., true);
                        w.step(DT);
                        assert_eq!(w.impacts().len(), 1);
                        assert!(w.impacts()[0].approach_speed > 19.);
                        assert!(w.impacts()[0].peak_normal_impulse > 0.);
                        w.step(DT);
                        assert!(w.impacts().is_empty());
                    }
                }
            }

            #[test]
            fn strict_approach_and_impulse_thresholds_and_invalid_values() {
                let (mut reference, _, _) = approaching();
                reference.step(DT);
                let impulse = reference.impacts()[0].peak_normal_impulse;
                for (speed, minimum, accepted) in [
                    (10., 0., false),
                    (9.99, 0., true),
                    (1., impulse, false),
                    (1., impulse * 0.999, true),
                    (1., impulse * 1.001, false),
                ] {
                    let (mut w, _, _) = approaching();
                    let mut settings = w.impact_settings().unwrap();
                    settings.min_approach_speed = speed;
                    settings.min_normal_impulse = minimum;
                    w.set_impact_settings(Some(settings)).unwrap();
                    w.step(DT);
                    assert_eq!(!w.impacts().is_empty(), accepted);
                }
                let settings = reference.impact_settings().unwrap();
                let snapshot = reference.impacts().to_vec();
                for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.] {
                    let mut bad = settings;
                    bad.min_normal_impulse = value;
                    assert_eq!(
                        reference.set_impact_settings(Some(bad)),
                        Err(ImpactRecordingError::InvalidSettings)
                    );
                    bad = settings;
                    bad.min_approach_speed = value;
                    assert_eq!(
                        reference.set_impact_settings(Some(bad)),
                        Err(ImpactRecordingError::InvalidSettings)
                    );
                }
                let mut bad = settings;
                bad.min_approach_speed = 0.;
                assert_eq!(
                    reference.set_impact_settings(Some(bad)),
                    Err(ImpactRecordingError::InvalidSettings)
                );
                bad = settings;
                bad.capacity = 65537;
                assert_eq!(
                    reference.set_impact_settings(Some(bad)),
                    Err(ImpactRecordingError::InvalidSettings)
                );
                bad.capacity = usize::MAX;
                assert_eq!(
                    reference.set_impact_settings(Some(bad)),
                    Err(ImpactRecordingError::InvalidSettings)
                );
                assert_eq!(reference.impact_settings(), Some(settings));
                assert_eq!(reference.impacts(), snapshot);
            }

            #[test]
            fn sensor_separation_and_in_substep_acceleration_from_rest_are_omitted() {
                let (mut w, body, [_, ball]) = approaching();
                w.colliders[ball].set_sensor(true);
                w.step(DT);
                assert!(w.impacts().is_empty());
                assert_eq!(w.contacts().count(), 1);
                w.colliders[ball].set_sensor(false);
                w.bodies[body].set_translation(-Vector::X * 0.99, true);
                w.bodies[body].set_linvel(-Vector::X * 10., true);
                w.step(DT);
                assert!(w.impacts().is_empty());
                let (mut w, body, _) = approaching();
                w.bodies[body].set_linvel(Vector::ZERO, true);
                w.gravity = ferravex_math::Vec3::X * 600.;
                assert!(w.step(DT).contact_points > 0);
                assert!(w.impacts().is_empty());
            }

            #[test]
            fn snapshot_generations_teleport_removal_empty_step_invalid_step_and_cancel() {
                let (mut w, body, [fixed, old]) = approaching();
                w.step(DT);
                let snapshot = w.impacts().to_vec();
                let settings = w.impact_settings();
                w.set_impact_settings(settings).unwrap();
                assert_eq!(w.impacts(), snapshot);
                for dt in [0., -DT, 0.101, f32::NAN, f32::INFINITY] {
                    assert!(
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.step(dt)))
                            .is_err()
                    );
                    assert_eq!(w.impacts(), snapshot);
                }
                w.gravity.x = f32::NAN;
                assert!(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.step(DT))).is_err()
                );
                w.gravity.x = 0.;
                let rotation = *w.bodies[body].rotation();
                w.try_teleport_body(
                    body,
                    RigidBodyPose {
                        translation: -Vector::X * 1.1,
                        rotation,
                    },
                    TeleportOptions::new(VelocityPolicy::Clear),
                )
                .unwrap();
                assert_eq!(w.impacts(), snapshot);
                assert!(w.remove_body(body));
                assert_eq!(w.impacts()[0].collider2, old);
                let (replacement, fresh) = w.insert(
                    RigidBodyBuilder::dynamic()
                        .translation(-Vector::X * 1.1)
                        .linvel(Vector::X * 10.),
                    ColliderBuilder::ball(0.5),
                );
                assert_ne!(old, fresh);
                w.step(DT);
                assert_eq!(w.impacts().len(), 1);
                assert_eq!(
                    (w.impacts()[0].collider1, w.impacts()[0].collider2),
                    (fixed, fresh)
                );
                w.set_impact_settings(None).unwrap();
                assert!(w.impacts().is_empty());
                assert_eq!(w.impact_dropped(), 0);
                assert!(w.impact_settings().is_none());
                enable(&mut w, 8);
                let mut changed = w.impact_settings().unwrap();
                changed.capacity = 4;
                w.set_impact_settings(Some(changed)).unwrap();
                assert!(w.impacts().is_empty());
                w.remove_body(replacement);
                let fixed_body = w.colliders[fixed].parent().unwrap();
                w.remove_body(fixed_body);
                w.step(DT);
                assert!(w.impacts().is_empty());
            }

            #[test]
            fn reservation_failures_preserve_settings_snapshot_and_allow_retry() {
                // Exercise failure of the record reserve and then motion reserve.
                for successful in [0, 1] {
                    let mut w = PhysicsWorld::default();
                    let settings = ImpactSettings::default();
                    FAIL_AFTER.with(|n| n.set(Some(successful)));
                    let result = w.set_impact_settings(Some(settings));
                    FAIL_AFTER.with(|n| n.set(None));
                    assert_eq!(result, Err(ImpactRecordingError::AllocationFailed));
                    assert!(w.impact_settings().is_none());
                    assert!(w.impacts().is_empty());
                    assert_eq!(w.impact_dropped(), 0);
                    w.set_impact_settings(Some(settings)).unwrap();
                    assert_eq!(w.impact_settings(), Some(settings));
                }
                let (mut w, _, _) = approaching();
                w.step(DT);
                let old = w.impact_settings();
                let snapshot = w.impacts().to_vec();
                let mut bigger = old.unwrap();
                bigger.capacity = MAX_IMPACT_RECORDS;
                FAIL_AFTER.with(|n| n.set(Some(0)));
                let result = w.set_impact_settings(Some(bigger));
                FAIL_AFTER.with(|n| n.set(None));
                assert_eq!(result, Err(ImpactRecordingError::AllocationFailed));
                assert_eq!(w.impact_settings(), old);
                assert_eq!(w.impacts(), snapshot);
                w.set_impact_settings(Some(bigger)).unwrap();
                assert!(w.impacts().is_empty());
            }

            #[test]
            fn warmed_recording_overflow_clear_and_reconfigure_reuse_allocations() {
                let (mut w, _, _) = corridor(Some(8));
                for _ in 0..16 {
                    w.step(DT);
                }
                ALLOCATIONS.with(|n| n.set(Some(0)));
                for _ in 0..128 {
                    enable(&mut w, 0);
                    w.step(DT);
                    assert_eq!(w.impact_dropped(), 8);
                    enable(&mut w, 1);
                    w.step(DT);
                    assert_eq!(w.impact_dropped(), 4);
                    enable(&mut w, 8);
                    w.step(DT);
                    assert_eq!(w.impacts().len(), 2);
                    w.clear_impacts();
                    w.set_impact_settings(None).unwrap();
                    enable(&mut w, 8);
                }
                let allocations = ALLOCATIONS.with(|n| n.replace(None).unwrap());
                assert_eq!(allocations, 0);
            }
        }
    };
}
#[cfg(feature = "dim2")]
impact_tests!(d2, ColliderBuilder::cuboid, |x, y| [x, y]);
#[cfg(feature = "dim3")]
impact_tests!(d3, |x, y| ColliderBuilder::cuboid(x, y, 5.), |x, y| [
    x, y, 0.
]);
