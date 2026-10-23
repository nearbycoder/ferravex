use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAllocator;
thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
fn record_allocation() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

macro_rules! radial_tests {
    ($module:ident, $dim:literal, $angular:expr) => {
        mod $module {
            use ferravex_math::Vec3;
            use ferravex_physics::{SolverSettings, radial::*, $module::*};

            fn point(x: f32, y: f32) -> [f32; $dim] {
                let mut result = [0.; $dim];
                result[0] = x;
                result[1] = y;
                result
            }
            fn request() -> RadialImpulseRequest {
                RadialImpulseRequest {
                    center: Vec3::ZERO,
                    radius: 4.,
                    model: ImpulseModel::PerBody(8.),
                    falloff: Falloff::Constant,
                    zero_distance: ZeroDistance::Axis(Vec3::Y),
                    sleeping: SleepingPolicy::Wake,
                    sensors: RadialSensors::Exclude,
                    layers: u32::MAX,
                    ignored_body: None,
                    max_bodies: 8,
                    max_colliders: 16,
                }
            }
            fn close(a: f32, b: f32) {
                assert!((a - b).abs() < 0.00001, "{a} != {b}");
            }
            fn snapshot(world: &PhysicsWorld) -> String {
                // BodySet Debug includes velocity, force, torque, wake flags,
                // revision/targets/quiet time; contacts/events are also preserved.
                format!(
                    "{:?} {:?} {:?} {:?} {:?}",
                    world.bodies,
                    world.colliders,
                    world.sleep_stats(),
                    world.contact_events(),
                    world.last_step
                )
            }
            fn seed(world: &mut PhysicsWorld) -> RadialImpulse {
                let mut scratch = [None; 16];
                let mut out = [None; 8];
                let _ = apply_radial_impulse(world, request(), &mut scratch, &mut out).unwrap();
                out[0].unwrap()
            }

            #[test]
            fn single_body_report_matches_actual_existing_impulse_method() {
                let mut world = PhysicsWorld::default();
                let body = world.add_ball(point(2., 0.), 0.5, true);
                world.bodies[body].add_force(Vector::from_array(point(1., 2.)), false);
                world.bodies[body].set_angvel($angular, false);
                world.bodies[body].add_torque($angular, false);
                let mut expected = world.bodies[body].clone();
                expected.apply_impulse(Vector::from_array(point(8., 0.)), true);
                let mut scratch = [None; 16];
                let mut out = [None; 8];
                let report =
                    apply_radial_impulse(&mut world, request(), &mut scratch, &mut out).unwrap();
                assert_eq!(
                    report,
                    RadialImpulseReport {
                        colliders: 1,
                        candidates: 1,
                        applied: 1
                    }
                );
                assert_eq!(world.bodies[body].linvel(), expected.linvel());
                assert_eq!(world.bodies[body].angvel(), expected.angvel());
                assert_eq!(format!("{:?}", world.bodies[body]), format!("{expected:?}"));
                let accepted = out[0].unwrap();
                assert_eq!(accepted.body, body);
                assert_eq!(accepted.impulse, Vec3::X * 8.);
                close(accepted.separation, -2.5);
                assert!(out[1..].iter().all(Option::is_none));
                world.step(1. / 120.);
                assert!(world.bodies[body].translation().x > 2.);
            }

            #[test]
            fn compounds_selected_by_child_surface_not_body_origin_and_applied_once() {
                let mut world = PhysicsWorld::default();
                // Body origin is outside the probe; its near child surface is inside.
                let (body, child) = world.insert(
                    RigidBodyBuilder::dynamic().translation(Vector::from_array(point(10., 0.))),
                    ColliderBuilder::ball(0.5).translation(point(-8., 0.).into()),
                );
                let second = world
                    .attach_collider(
                        body,
                        ColliderBuilder::ball(0.5).translation(point(-8., 0.).into()),
                    )
                    .unwrap();
                let mut scratch = [None; 16];
                let mut out = [None; 8];
                let mut req = request();
                req.falloff = Falloff::LinearSurface;
                let report = apply_radial_impulse(&mut world, req, &mut scratch, &mut out).unwrap();
                assert_eq!((report.candidates, report.applied), (2, 1));
                let effect = out[0].unwrap();
                assert_eq!(effect.collider, child.min(second));
                close(effect.impulse.x, 5.); // surface at 1.5, weight 1 - 1.5/4
                close(
                    world.bodies[body].linvel().x,
                    5. / world.bodies[body].mass(),
                );
                assert_eq!(world.bodies[body].angvel(), Angular::default());
            }

            #[test]
            fn near_sensor_child_can_select_body_whose_com_is_outside_blast() {
                let mut world = PhysicsWorld::default();
                let (body, _) = world.insert(
                    RigidBodyBuilder::dynamic().translation(point(10., 0.).into()),
                    ColliderBuilder::ball(0.5),
                );
                let near = world
                    .attach_collider(
                        body,
                        ColliderBuilder::ball(0.5)
                            .translation(point(-8., 0.).into())
                            .sensor(true)
                            .density(0.)
                            .collision_layers(2, 0),
                    )
                    .unwrap();
                assert!(world.bodies[body].center_of_mass().x > request().radius);
                let mut req = request();
                req.sensors = RadialSensors::Only;
                req.layers = 2;
                req.falloff = Falloff::LinearSurface;
                let mut scratch = [None; 16];
                let mut out = [None; 8];
                let report = apply_radial_impulse(&mut world, req, &mut scratch, &mut out).unwrap();
                assert_eq!(report.applied, 1);
                assert_eq!(out[0].unwrap().collider, near);
                close(out[0].unwrap().impulse.x, 5.);
                close(
                    world.bodies[body].linvel().x,
                    5. / world.bodies[body].mass(),
                );
            }

            #[test]
            fn body_order_is_handle_order_not_proximity_and_reused_slots_are_generational() {
                let mut world = PhysicsWorld::default();
                let old = world.add_ball(point(3., 0.), 0.5, true);
                let late = world.add_ball(point(1., 0.), 0.5, true);
                assert!(world.remove_body(old));
                let reused = world.add_ball(point(2., 0.), 0.5, true);
                let mut req = request();
                req.ignored_body = Some(old); // A stale generation excludes nothing.
                let mut scratch = [None; 16];
                let mut out = [None; 8];
                let report = apply_radial_impulse(&mut world, req, &mut scratch, &mut out).unwrap();
                assert_eq!(report.applied, 2);
                assert_eq!(out[0].unwrap().body, reused.min(late));
                assert_eq!(out[1].unwrap().body, reused.max(late));
                let first = out;
                let report = apply_radial_impulse(&mut world, req, &mut scratch, &mut out).unwrap();
                assert_eq!(report.applied, 2);
                assert_eq!(out, first);
            }

            #[test]
            fn total_budget_weights_unique_bodies_not_compound_children() {
                let mut world = PhysicsWorld::default();
                let (a, _) = world.insert(
                    RigidBodyBuilder::dynamic().translation(point(1.5, 0.).into()),
                    ColliderBuilder::ball(0.5),
                );
                world
                    .attach_collider(a, ColliderBuilder::ball(0.5))
                    .unwrap();
                let b = world.add_ball(point(3.5, 0.), 0.5, true);
                let mut req = request();
                req.model = ImpulseModel::Total(12.);
                req.falloff = Falloff::LinearSurface;
                let mut scratch = [None; 16];
                let mut out = [None; 8];
                let report = apply_radial_impulse(&mut world, req, &mut scratch, &mut out).unwrap();
                assert_eq!((report.candidates, report.applied), (3, 2));
                assert_eq!(out[0].unwrap().body, a);
                assert_eq!(out[1].unwrap().body, b);
                close(out[0].unwrap().impulse.x, 9.);
                close(out[1].unwrap().impulse.x, 3.);
            }

            #[test]
            fn coincident_axis_skip_touching_and_zero_radius_are_explicit() {
                let mut world = PhysicsWorld::default();
                let centered = world.add_ball(point(0., 0.), 1., true);
                let touching = world.add_ball(point(5., 0.), 1., true);
                let mut req = request();
                req.zero_distance = ZeroDistance::Axis(Vec3::new(0., f32::from_bits(1), 0.));
                req.falloff = Falloff::LinearSurface;
                let mut scratch = [None; 16];
                let mut out = [None; 8];
                let report = apply_radial_impulse(&mut world, req, &mut scratch, &mut out).unwrap();
                assert_eq!((report.candidates, report.applied), (2, 1));
                assert_eq!(out[0].unwrap().body, centered);
                assert_eq!(out[0].unwrap().impulse, Vec3::Y * 8.);
                assert_eq!(world.bodies[touching].linvel(), Vector::ZERO);
                req.radius = 0.;
                let report = apply_radial_impulse(&mut world, req, &mut scratch, &mut out).unwrap();
                assert_eq!((report.candidates, report.applied), (1, 1));
                req.zero_distance = ZeroDistance::Skip;
                req.max_bodies = 0;
                let before = snapshot(&world);
                let report = apply_radial_impulse(&mut world, req, &mut scratch, &mut out).unwrap();
                assert_eq!(report.applied, 0);
                assert_eq!(snapshot(&world), before);
                assert!(out.iter().all(Option::is_none));
            }

            #[test]
            fn filters_use_membership_sensors_ignore_and_exclude_fixed_kinematic() {
                let mut world = PhysicsWorld::default();
                let (solid, _) = world.insert(
                    RigidBodyBuilder::dynamic().translation(point(2., 0.).into()),
                    ColliderBuilder::ball(0.5).collision_layers(2, 0),
                );
                let (sensor, sensor_child) = world.insert(
                    RigidBodyBuilder::dynamic().translation(point(2., 0.).into()),
                    ColliderBuilder::ball(0.5)
                        .sensor(true)
                        .collision_layers(2, 0),
                );
                let fixed = world.add_ball(point(2., 0.), 1., false);
                let (kinematic, _) = world.insert(
                    RigidBodyBuilder::kinematic_position_based().translation(point(2., 0.).into()),
                    ColliderBuilder::ball(0.5),
                );
                let mut req = request();
                req.layers = 2;
                let mut scratch = [None; 16];
                let mut out = [None; 8];
                assert_eq!(
                    apply_radial_impulse(&mut world, req, &mut scratch, &mut out)
                        .unwrap()
                        .applied,
                    1
                );
                assert_eq!(out[0].unwrap().body, solid);
                req.sensors = RadialSensors::Only;
                assert_eq!(
                    apply_radial_impulse(&mut world, req, &mut scratch, &mut out)
                        .unwrap()
                        .applied,
                    1
                );
                assert_eq!(out[0].unwrap().body, sensor);
                assert_eq!(out[0].unwrap().collider, sensor_child);
                req.sensors = RadialSensors::Include;
                req.ignored_body = Some(solid);
                assert_eq!(
                    apply_radial_impulse(&mut world, req, &mut scratch, &mut out)
                        .unwrap()
                        .applied,
                    1
                );
                assert_eq!(out[0].unwrap().body, sensor);
                assert_eq!(world.bodies[fixed].linvel(), Vector::ZERO);
                assert_eq!(world.bodies[kinematic].linvel(), Vector::ZERO);
                req.layers = 0;
                assert_eq!(
                    apply_radial_impulse(&mut world, req, &mut [], &mut out)
                        .unwrap()
                        .applied,
                    0
                );
                assert!(out.iter().all(Option::is_none));
            }

            #[test]
            fn capacities_invalid_requests_and_truncation_are_atomic_including_output() {
                let mut world = PhysicsWorld::default();
                let a = world.add_ball(point(1., 0.), 0.5, true);
                let b = world.add_ball(point(2., 0.), 0.5, true);
                let effect = seed(&mut world);
                world.bodies[a].add_force(Vector::from_array(point(1., 2.)), false);
                world.bodies[b].add_force(Vector::from_array(point(-3., 2.)), false);
                let before = snapshot(&world);
                let mut scratch = [None; 16];
                let mut out = [Some(effect); 8];
                let output_before = out;
                let mut req = request();
                let cases = [
                    (
                        RadialImpulseRequest {
                            center: Vec3::new(f32::NAN, 0., 0.),
                            ..req
                        },
                        RadialImpulseError::InvalidCenter,
                    ),
                    (
                        RadialImpulseRequest {
                            radius: f32::INFINITY,
                            ..req
                        },
                        RadialImpulseError::InvalidRadius,
                    ),
                    (
                        RadialImpulseRequest { radius: -1., ..req },
                        RadialImpulseError::InvalidRadius,
                    ),
                    (
                        RadialImpulseRequest {
                            radius: 1001.,
                            ..req
                        },
                        RadialImpulseError::InvalidRadius,
                    ),
                    (
                        RadialImpulseRequest {
                            model: ImpulseModel::Total(-1.),
                            ..req
                        },
                        RadialImpulseError::InvalidMagnitude,
                    ),
                    (
                        RadialImpulseRequest {
                            model: ImpulseModel::PerBody(f32::NAN),
                            ..req
                        },
                        RadialImpulseError::InvalidMagnitude,
                    ),
                    (
                        RadialImpulseRequest {
                            zero_distance: ZeroDistance::Axis(Vec3::ZERO),
                            ..req
                        },
                        RadialImpulseError::InvalidZeroAxis,
                    ),
                    (
                        RadialImpulseRequest {
                            zero_distance: ZeroDistance::Axis(Vec3::splat(f32::INFINITY)),
                            ..req
                        },
                        RadialImpulseError::InvalidZeroAxis,
                    ),
                    (
                        RadialImpulseRequest {
                            max_bodies: 9,
                            ..req
                        },
                        RadialImpulseError::OutputCapacity { required: 9 },
                    ),
                    (
                        RadialImpulseRequest {
                            max_colliders: 1,
                            ..req
                        },
                        RadialImpulseError::WorkLimit { required: 2 },
                    ),
                    (
                        RadialImpulseRequest {
                            max_bodies: 1,
                            ..req
                        },
                        RadialImpulseError::BodyLimit { required: 2 },
                    ),
                ];
                for (request, error) in cases {
                    assert_eq!(
                        apply_radial_impulse(&mut world, request, &mut scratch, &mut out),
                        Err(error)
                    );
                    assert_eq!(snapshot(&world), before);
                    assert_eq!(out, output_before);
                }
                for cap in 0..2 {
                    assert_eq!(
                        apply_radial_impulse(&mut world, req, &mut scratch[..cap], &mut out),
                        Err(RadialImpulseError::CandidateCapacity { required: 2 })
                    );
                    assert_eq!(snapshot(&world), before);
                    assert_eq!(out, output_before);
                }
                req.max_bodies = 2;
                let report =
                    apply_radial_impulse(&mut world, req, &mut scratch[..2], &mut out[..2])
                        .unwrap();
                assert_eq!(report.applied, 2); // Exactly-full is complete.
                assert_eq!(out[2..], output_before[2..]);
            }

            #[test]
            fn sleeping_skip_preserve_and_wake_have_distinct_semantics() {
                for policy in [
                    SleepingPolicy::Skip,
                    SleepingPolicy::Preserve,
                    SleepingPolicy::Wake,
                ] {
                    let mut world = PhysicsWorld::default();
                    let settings = world.solver_settings();
                    world
                        .set_solver_settings(SolverSettings {
                            sleep_delay: 0.05,
                            ..settings
                        })
                        .unwrap();
                    let body = world.add_ball(point(1., 0.), 0.5, true);
                    for _ in 0..10 {
                        world.step(1. / 60.);
                    }
                    assert!(world.bodies[body].is_sleeping());
                    let sleep = world.sleep_state(body).unwrap();
                    let mut req = request();
                    req.sleeping = policy;
                    let mut scratch = [None; 16];
                    let mut out = [None; 8];
                    let report =
                        apply_radial_impulse(&mut world, req, &mut scratch, &mut out).unwrap();
                    assert_eq!(report.applied, usize::from(policy != SleepingPolicy::Skip));
                    if policy == SleepingPolicy::Wake {
                        assert!(!world.bodies[body].is_sleeping());
                        assert!(world.sleep_state(body).unwrap().wake_requested);
                    } else {
                        assert_eq!(world.sleep_state(body).unwrap(), sleep);
                    }
                    if policy != SleepingPolicy::Skip {
                        assert!(world.bodies[body].linvel().x > 0.);
                    } else {
                        assert_eq!(world.bodies[body].linvel(), Vector::ZERO);
                    }
                }
            }

            #[test]
            fn pending_mass_rejects_without_refreshing_then_explicit_refresh_allows_retry() {
                let mut world = PhysicsWorld::default();
                let (a, child) = world.insert(
                    RigidBodyBuilder::dynamic().translation(point(1., 0.).into()),
                    ColliderBuilder::ball(0.5),
                );
                let effect = seed(&mut world);
                world.colliders[child].set_density(2.);
                let before = snapshot(&world);
                let mut scratch = [None; 16];
                let mut out = [Some(effect); 8];
                assert_eq!(
                    apply_radial_impulse(&mut world, request(), &mut scratch, &mut out),
                    Err(RadialImpulseError::PendingMass { body: a })
                );
                assert_eq!(snapshot(&world), before);
                assert_eq!(out, [Some(effect); 8]);
                world.refresh_mass_properties().unwrap();
                assert_eq!(
                    apply_radial_impulse(&mut world, request(), &mut scratch, &mut out)
                        .unwrap()
                        .applied,
                    1
                );
            }

            #[test]
            fn numeric_extremes_reject_all_before_any_partial_effect() {
                let mut world = PhysicsWorld::default();
                let (a, _) = world.insert(
                    RigidBodyBuilder::dynamic().translation(point(1., 0.).into()),
                    ColliderBuilder::ball(0.5).density(1000.),
                );
                let b = world.add_ball(point(2., 0.), 0.1, true);
                let effect = seed(&mut world);
                let mut req = request();
                req.model = ImpulseModel::PerBody(f32::MAX);
                let mut scratch = [None; 16];
                let mut out = [Some(effect); 8];
                let before = snapshot(&world);
                let result = apply_radial_impulse(&mut world, req, &mut scratch, &mut out);
                assert!(matches!(
                    result,
                    Err(RadialImpulseError::NonFiniteEffect { .. })
                ));
                assert_eq!(snapshot(&world), before);
                assert_eq!(out, [Some(effect); 8]);
                world.bodies[b].set_translation(Vector::from_array(point(f32::MAX, 0.)), false);
                let before = snapshot(&world);
                assert!(matches!(
                    apply_radial_impulse(
                        &mut world,
                        RadialImpulseRequest {
                            center: Vec3::new(-f32::MAX, 0., 0.),
                            ..request()
                        },
                        &mut scratch,
                        &mut out
                    ),
                    Err(RadialImpulseError::IncompleteGeometry { .. })
                ));
                assert_eq!(snapshot(&world), before);
                assert_eq!(out, [Some(effect); 8]);
                assert_eq!(
                    world.bodies[a].linvel().x,
                    effect.impulse.x / world.bodies[a].mass()
                );
            }

            #[test]
            fn huge_finite_total_and_axis_have_finite_effects_when_mass_allows() {
                let mut world = PhysicsWorld::default();
                for y in [-1., 1.] {
                    world.insert(
                        RigidBodyBuilder::dynamic().translation(point(0., y).into()),
                        ColliderBuilder::ball(1.).density(1000.),
                    );
                }
                let mut req = request();
                req.model = ImpulseModel::Total(f32::MAX);
                req.zero_distance = ZeroDistance::Axis(Vec3::new(f32::MAX, f32::MAX, 0.));
                let mut scratch = [None; 16];
                let mut out = [None; 8];
                let report = apply_radial_impulse(&mut world, req, &mut scratch, &mut out).unwrap();
                assert_eq!(report.applied, 2);
                for effect in out.iter().flatten() {
                    assert!(effect.impulse.is_finite());
                    assert_eq!(effect.impulse.y.abs(), f32::MAX * 0.5);
                    assert!(world.bodies[effect.body].linvel().is_finite());
                }
            }

            #[test]
            fn zero_magnitude_empty_miss_underflow_and_reset_leave_no_stale_outputs() {
                let mut world = PhysicsWorld::default();
                let body = world.add_ball(point(1., 1.), 0.5, true);
                let effect = seed(&mut world);
                let mut scratch = [None; 16];
                let mut out = [Some(effect); 8];
                for magnitude in [0., f32::from_bits(1)] {
                    let mut req = request();
                    req.model = ImpulseModel::Total(magnitude);
                    // Tiny falloff makes each component round to zero.
                    if magnitude > 0. {
                        req.radius = 1.;
                        req.falloff = Falloff::LinearSurface;
                        req.model = ImpulseModel::PerBody(magnitude);
                    }
                    let before = snapshot(&world);
                    let report =
                        apply_radial_impulse(&mut world, req, &mut scratch, &mut out).unwrap();
                    assert_eq!(report.applied, 0);
                    assert_eq!(snapshot(&world), before);
                    assert!(out.iter().all(Option::is_none));
                }
                assert!(world.remove_body(body));
                let report =
                    apply_radial_impulse(&mut world, request(), &mut scratch, &mut out).unwrap();
                assert_eq!(report, RadialImpulseReport::default());
                let replacement = world.add_ball(point(1., 0.), 0.5, true);
                assert_ne!(body, replacement);
                assert_eq!(
                    apply_radial_impulse(&mut world, request(), &mut scratch, &mut out)
                        .unwrap()
                        .applied,
                    1
                );
                assert_eq!(out[0].unwrap().body, replacement);
                assert!(out[1..].iter().all(Option::is_none));
            }

            #[test]
            fn rejected_sleeping_blast_preserves_quiet_wake_and_pending_force() {
                let mut world = PhysicsWorld::default();
                let settings = world.solver_settings();
                world
                    .set_solver_settings(SolverSettings {
                        sleep_delay: 0.05,
                        ..settings
                    })
                    .unwrap();
                let a = world.add_ball(point(1., 0.), 0.2, true);
                let b = world.add_ball(point(2., 0.), 0.2, true);
                for _ in 0..10 {
                    world.step(1. / 60.);
                }
                assert!(world.bodies[a].is_sleeping() && world.bodies[b].is_sleeping());
                world.bodies[a].add_force(Vector::from_array(point(1., 0.)), false);
                world.bodies[b].add_force(Vector::from_array(point(2., 0.)), false);
                let before = snapshot(&world);
                let effect = RadialImpulse {
                    body: a,
                    collider: world.bodies[a].colliders()[0],
                    separation: -1.,
                    impulse: Vec3::X,
                };
                let mut scratch = [None; 16];
                let mut out = [Some(effect); 8];
                let mut req = request();
                req.max_bodies = 1;
                assert_eq!(
                    apply_radial_impulse(&mut world, req, &mut scratch, &mut out),
                    Err(RadialImpulseError::BodyLimit { required: 2 })
                );
                assert_eq!(snapshot(&world), before);
                assert_eq!(out, [Some(effect); 8]);
                req.max_bodies = 8;
                assert_eq!(
                    apply_radial_impulse(&mut world, req, &mut scratch[..1], &mut out),
                    Err(RadialImpulseError::CandidateCapacity { required: 2 })
                );
                assert_eq!(snapshot(&world), before);
                assert_eq!(out, [Some(effect); 8]);
            }

            #[test]
            fn total_budget_excludes_zero_axis_skips_and_uses_huge_axis_at_center() {
                let mut world = PhysicsWorld::default();
                let (centered, _) = world.insert(
                    RigidBodyBuilder::dynamic(),
                    ColliderBuilder::ball(0.2).density(1000.),
                );
                let other = world.add_ball(point(2., 0.), 0.2, true);
                let mut req = request();
                req.model = ImpulseModel::Total(6.);
                req.zero_distance = ZeroDistance::Skip;
                req.max_bodies = 1;
                let mut scratch = [None; 16];
                let mut out = [None; 8];
                let report = apply_radial_impulse(&mut world, req, &mut scratch, &mut out).unwrap();
                assert_eq!(report.applied, 1);
                assert_eq!(out[0].unwrap().body, other);
                assert_eq!(out[0].unwrap().impulse, Vec3::X * 6.);
                assert_eq!(world.bodies[centered].linvel(), Vector::ZERO);
                req.ignored_body = Some(other);
                req.zero_distance = ZeroDistance::Axis(Vec3::new(f32::MAX, f32::MAX, 0.));
                req.model = ImpulseModel::PerBody(f32::MAX);
                let report = apply_radial_impulse(&mut world, req, &mut scratch, &mut out).unwrap();
                assert_eq!(report.applied, 1);
                let impulse = out[0].unwrap().impulse;
                assert!(impulse.is_finite() && impulse.x > f32::MAX * 0.7);
                assert_eq!(impulse.x, impulse.y);
                assert!(world.bodies[centered].linvel().is_finite());
            }

            #[test]
            fn overflowed_velocity_addition_rejects_and_filtered_extreme_is_not_a_candidate() {
                let mut world = PhysicsWorld::default();
                let (body, child) = world.insert(
                    RigidBodyBuilder::dynamic()
                        .translation(point(1., 0.).into())
                        .linvel(point(f32::MAX, 0.).into()),
                    ColliderBuilder::ball(1.),
                );
                let mut req = request();
                req.model = ImpulseModel::PerBody(f32::MAX);
                let mut scratch = [None; 16];
                let mut out = [None; 8];
                let before = snapshot(&world);
                assert_eq!(
                    apply_radial_impulse(&mut world, req, &mut scratch, &mut out),
                    Err(RadialImpulseError::NonFiniteEffect { body })
                );
                assert_eq!(snapshot(&world), before);
                world.colliders[child].set_collision_layers(0, u32::MAX);
                world.bodies[body].set_translation(Vector::from_array(point(f32::MAX, 0.)), false);
                req.center = Vec3::new(-f32::MAX, 0., 0.);
                assert_eq!(
                    apply_radial_impulse(&mut world, req, &mut scratch, &mut out)
                        .unwrap()
                        .candidates,
                    0
                );
            }

            #[test]
            fn accepted_truncated_and_invalid_paths_allocate_nothing() {
                let mut world = PhysicsWorld::default();
                for i in 0..64 {
                    world.add_ball(point(1. + (i % 3) as f32, 0.), 0.5, true);
                }
                let mut scratch = [None; 64];
                let mut out = [None; 64];
                let req = RadialImpulseRequest {
                    max_bodies: 64,
                    max_colliders: 64,
                    ..request()
                };
                super::ALLOCATIONS.with(|n| n.set(Some(0)));
                let accepted = apply_radial_impulse(&mut world, req, &mut scratch, &mut out);
                let truncated = apply_radial_impulse(&mut world, req, &mut scratch[..1], &mut out);
                let invalid = apply_radial_impulse(
                    &mut world,
                    RadialImpulseRequest {
                        radius: -1.,
                        ..request()
                    },
                    &mut scratch,
                    &mut out,
                );
                let allocations = super::ALLOCATIONS.with(|n| n.replace(None).unwrap());
                assert!(accepted.is_ok() && truncated.is_err() && invalid.is_err());
                assert_eq!(allocations, 0);
            }
        }
    };
}
#[cfg(feature = "dim2")]
radial_tests!(d2, 2, 0.25);
#[cfg(feature = "dim3")]
radial_tests!(d3, 3, Vec3::new(0.25, 0.5, 0.75));

#[cfg(feature = "dim2")]
#[test]
fn planar_center_and_zero_axis_reject_z_in_empty_world() {
    use ferravex_math::Vec3;
    use ferravex_physics::{d2::*, radial::*};
    let mut world = PhysicsWorld::default();
    let mut req = RadialImpulseRequest {
        center: Vec3::Z,
        radius: 1.,
        model: ImpulseModel::PerBody(0.),
        falloff: Falloff::Constant,
        zero_distance: ZeroDistance::Skip,
        sleeping: SleepingPolicy::Wake,
        sensors: RadialSensors::Exclude,
        layers: 0,
        ignored_body: None,
        max_bodies: 0,
        max_colliders: 0,
    };
    assert_eq!(
        apply_radial_impulse(&mut world, req, &mut [], &mut []),
        Err(RadialImpulseError::InvalidCenter)
    );
    req.center = Vec3::ZERO;
    req.zero_distance = ZeroDistance::Axis(Vec3::Z);
    assert_eq!(
        apply_radial_impulse(&mut world, req, &mut [], &mut []),
        Err(RadialImpulseError::InvalidZeroAxis)
    );
}
