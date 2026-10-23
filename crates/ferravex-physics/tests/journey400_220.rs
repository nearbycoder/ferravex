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
// Count only this test thread, so concurrently running tests cannot interfere.
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

macro_rules! overlap_tests {
    ($module:ident, $dim:literal, $quarter_turn:expr) => {
        mod $module {
            use ferravex_math::{Quat, Vec3};
            use ferravex_physics::$module::{
                ColliderBuilder, OverlapHit, PhysicsWorld, QueryError, QueryOptions, QueryReport,
                RigidBodyBuilder, SensorPolicy, Vector,
            };

            fn point(x: f32, y: f32) -> [f32; $dim] {
                let mut p = [0.; $dim];
                p[0] = x;
                p[1] = y;
                p
            }
            fn close(actual: f32, expected: f32) {
                assert!(
                    (actual - expected).abs() < 0.00001,
                    "{actual} != {expected}"
                );
            }
            fn witnesses(hit: OverlapHit, center: [f32; $dim], radius: f32) {
                let n = Vector::from_array(hit.normal);
                close(n.length(), 1.);
                let target = Vector::from_array(hit.collider_point);
                let probe = Vector::from_array(hit.probe_point);
                close((probe - Vector::from_array(center)).length(), radius);
                close((probe - target - n * hit.separation).length(), 0.);
            }

            #[test]
            fn outside_touch_inside_and_center_have_signed_surface_witnesses() {
                for is_box in [false, true] {
                    let mut world = PhysicsWorld::default();
                    let body = if is_box {
                        world.add_box(point(0., 0.), [1.; $dim], false)
                    } else {
                        world.add_ball(point(0., 0.), 1., false)
                    };
                    for radius in [0., 0.5] {
                        for (x, separation) in [
                            (2., 1. - radius),
                            (1. + radius, 0.),
                            (0.25, -0.75 - radius),
                            (0., -1. - radius),
                        ] {
                            let center = point(x, 0.);
                            let hit = world
                                .overlap_sphere_with_options(center, radius, QueryOptions::new(2.))
                                .unwrap();
                            assert_eq!(hit.body, Some(body));
                            close(hit.separation, separation);
                            close(hit.penetration_depth(), (-separation).max(0.));
                            witnesses(hit, center, radius);
                            // The returned target witness lies on the target boundary.
                            let boundary = world
                                .overlap_point_with_options(
                                    hit.collider_point,
                                    QueryOptions::new(0.001),
                                )
                                .unwrap();
                            close(boundary.separation, 0.);
                            if separation < 0. {
                                let moved = (Vector::from_array(center)
                                    + Vector::from_array(hit.normal) * hit.penetration_depth())
                                .to_array();
                                close(
                                    world
                                        .overlap_sphere_with_options(
                                            moved,
                                            radius,
                                            QueryOptions::new(0.001),
                                        )
                                        .unwrap()
                                        .separation,
                                    0.,
                                );
                            }
                            let overlap = world.overlap_sphere_with_options(
                                center,
                                radius,
                                QueryOptions::new(0.),
                            );
                            assert_eq!(overlap.is_some(), separation <= 0.);
                        }
                    }
                    let corner = point(2., 2.);
                    let hit = world
                        .overlap_point_with_options(corner, QueryOptions::new(3.))
                        .unwrap();
                    witnesses(hit, corner, 0.);
                    close(
                        hit.separation,
                        if is_box {
                            2_f32.sqrt()
                        } else {
                            8_f32.sqrt() - 1.
                        },
                    );
                }
            }

            #[test]
            fn all_capacities_keep_smallest_signed_separations_and_later_ties() {
                let mut world = PhysicsWorld::default();
                let far = world.add_ball(point(4., 0.), 1., false);
                let early = world.add_ball(point(0., 0.), 1., false);
                let touching = world.add_ball(point(1., 0.), 1., false);
                let deep = world.add_ball(point(0., 0.), 2., false);
                let late = world.add_ball(point(0., 0.), 1., false);
                let options = QueryOptions::new(3.);
                let mut all = [None; 7];
                assert_eq!(
                    world
                        .overlap_point_into(point(0., 0.), options, &mut all)
                        .unwrap(),
                    QueryReport {
                        written: 5,
                        total: 5
                    }
                );
                for (hit, body) in all.iter().zip([deep, late, early, touching, far]) {
                    assert_eq!(hit.unwrap().body, Some(body));
                }
                assert_eq!(
                    world.overlap_point_with_options(point(0., 0.), options),
                    all[0]
                );
                for capacity in 0..=7 {
                    let mut output = [all[0]; 7];
                    let report = world
                        .overlap_sphere_into(point(0., 0.), 0., options, &mut output[..capacity])
                        .unwrap();
                    assert_eq!((report.written, report.total), (capacity.min(5), 5));
                    assert_eq!(report.is_truncated(), capacity < 5);
                    assert_eq!(&output[..capacity], &all[..capacity]);
                }
                // A reused earlier slot remains earlier in iteration, regardless of generation.
                assert!(world.remove_body(early));
                let reused = world.add_ball(point(0., 0.), 1., false);
                assert_ne!(reused, early);
                let report = world
                    .overlap_point_into(point(0., 0.), options, &mut all)
                    .unwrap();
                assert_eq!(report.total, 5);
                assert_eq!(all[1].unwrap().body, Some(late));
                assert_eq!(all[2].unwrap().body, Some(reused));
            }

            #[test]
            fn empty_world_misses_and_reused_buffers_clear_successfully() {
                let mut world = PhysicsWorld::default();
                let body = world.add_ball(point(0., 0.), 1., false);
                let seed = world.overlap_point_with_options(point(0., 0.), QueryOptions::new(0.));
                assert!(seed.is_some());
                for removed in [false, true] {
                    if removed {
                        assert!(world.remove_body(body));
                    }
                    let mut output = [seed; 3];
                    let report = world
                        .overlap_point_into(point(10., 0.), QueryOptions::new(0.), &mut output)
                        .unwrap();
                    assert_eq!(report, QueryReport::default());
                    assert_eq!(output, [None; 3]);
                    assert!(
                        world
                            .overlap_point_with_options(point(10., 0.), QueryOptions::new(0.))
                            .is_none()
                    );
                }
            }

            #[test]
            fn invalid_input_is_atomic_even_for_empty_buffers_and_filtered_worlds() {
                let mut world = PhysicsWorld::default();
                world.add_ball(point(0., 0.), 1., false);
                let seed = world.overlap_point_with_options(point(0., 0.), QueryOptions::new(0.));
                let cases = [
                    (
                        point(f32::NAN, 0.),
                        f32::NAN,
                        f32::NAN,
                        QueryError::NonFiniteOrigin,
                    ),
                    (
                        point(0., f32::INFINITY),
                        0.,
                        0.,
                        QueryError::NonFiniteOrigin,
                    ),
                    (point(0., 0.), f32::NAN, -1., QueryError::InvalidMaxDistance),
                    (
                        point(0., 0.),
                        0.,
                        f32::INFINITY,
                        QueryError::InvalidMaxDistance,
                    ),
                    (point(0., 0.), 0., f32::NAN, QueryError::InvalidMaxDistance),
                    (point(0., 0.), f32::NAN, 0., QueryError::InvalidRadius),
                    (point(0., 0.), f32::INFINITY, 0., QueryError::InvalidRadius),
                    (point(0., 0.), -1., 0., QueryError::InvalidRadius),
                    (point(0., 0.), 1000.1, 0., QueryError::InvalidRadius),
                ];
                for empty_world in [false, true] {
                    if empty_world {
                        world = PhysicsWorld::default();
                    }
                    for (center, radius, distance, expected) in cases {
                        let options = QueryOptions::new(distance).layers(0);
                        for capacity in [0, 3] {
                            let mut output = [seed; 3];
                            assert_eq!(
                                world.overlap_sphere_into(
                                    center,
                                    radius,
                                    options,
                                    &mut output[..capacity]
                                ),
                                Err(expected)
                            );
                            assert_eq!(output, [seed; 3]);
                        }
                        assert!(
                            world
                                .overlap_sphere_with_options(center, radius, options)
                                .is_none()
                        );
                    }
                }
                // Inclusive upper radius bound and signed zero are valid.
                assert!(
                    world
                        .overlap_sphere_into(point(0., 0.), 1000., QueryOptions::new(-0.), &mut [])
                        .is_ok()
                );
            }

            #[test]
            fn filters_compound_children_sensors_and_stale_ignored_handles() {
                let mut world = PhysicsWorld::default();
                let compound = world
                    .insert_compound(
                        RigidBodyBuilder::fixed(),
                        [
                            ColliderBuilder::ball(1.).collision_layers(0b10, 0),
                            ColliderBuilder::ball(1.)
                                .translation(Vector::from_array(point(1., 0.)))
                                .collision_layers(0b10, 0),
                        ],
                    )
                    .unwrap();
                let (sensor, child) = world.insert(
                    RigidBodyBuilder::fixed(),
                    ColliderBuilder::ball(1.)
                        .sensor(true)
                        .collision_layers(0b01, 0),
                );
                let options = QueryOptions::new(0.);
                let mut output = [None; 4];
                for (policy, expected) in [
                    (SensorPolicy::Exclude, 2),
                    (SensorPolicy::Include, 3),
                    (SensorPolicy::Only, 1),
                ] {
                    let report = world
                        .overlap_point_into(point(0., 0.), options.sensors(policy), &mut output)
                        .unwrap();
                    assert_eq!(report.total, expected);
                }
                for (options, expected) in [
                    (options.layers(0b10), 2),
                    (options.layers(0b10).ignore_body(compound), 0),
                    (
                        options.ignore_body(compound).sensors(SensorPolicy::Include),
                        1,
                    ),
                    (options.sensors(SensorPolicy::Only).ignore_body(sensor), 0),
                    (options.layers(0), 0),
                ] {
                    assert_eq!(
                        world
                            .overlap_point_into(point(0., 0.), options, &mut output)
                            .unwrap()
                            .total,
                        expected
                    );
                }
                assert!(world.remove_body(compound));
                let replacement = world.add_ball(point(0., 0.), 1., false);
                assert_ne!(replacement, compound);
                assert_eq!(
                    world
                        .overlap_point_with_options(point(0., 0.), options.ignore_body(compound))
                        .unwrap()
                        .body,
                    Some(replacement)
                );
                world.colliders[child].set_sensor(false);
                world.colliders[child].set_collision_layers(0b10, 0);
                assert_eq!(
                    world
                        .overlap_point_into(point(0., 0.), options.layers(0b10), &mut output)
                        .unwrap()
                        .total,
                    2
                );
                world.remove_collider(child).unwrap();
                assert_eq!(
                    world
                        .overlap_point_into(point(0., 0.), options, &mut output)
                        .unwrap()
                        .total,
                    1
                );
            }

            #[test]
            fn body_and_child_pose_edits_are_visible_without_step_or_refresh() {
                let mut world = PhysicsWorld::default();
                let (body, child) = world.insert(
                    RigidBodyBuilder::fixed(),
                    ColliderBuilder::ball(0.5).translation(Vector::from_array(point(2., 0.))),
                );
                let options = QueryOptions::new(20.);
                let center = point(0., 0.);
                close(
                    world
                        .overlap_point_with_options(center, options)
                        .unwrap()
                        .separation,
                    1.5,
                );
                world.bodies[body].set_translation(Vector::from_array(point(0., 3.)), false);
                world.bodies[body].set_rotation($quarter_turn, false);
                let hit = world.overlap_point_with_options(center, options).unwrap();
                close(hit.separation, 4.5);
                close(hit.normal[1], -1.);
                world
                    .set_collider_pose(child, Vec3::Y * 3., Quat::IDENTITY)
                    .unwrap();
                let hit = world.overlap_point_with_options(center, options).unwrap();
                close(hit.separation, 18_f32.sqrt() - 0.5);
                witnesses(hit, center, 0.);

                let mut half = [0.25; $dim];
                half[0] = 2.;
                let box_body = world.add_box(point(0., 0.), half, false);
                let box_child = world.bodies[box_body].colliders()[0];
                let center = point(0., 1.);
                let options = options.ignore_body(body);
                close(
                    world
                        .overlap_point_with_options(center, options)
                        .unwrap()
                        .separation,
                    0.75,
                );
                world
                    .set_collider_pose(
                        box_child,
                        Vec3::ZERO,
                        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                    )
                    .unwrap();
                close(
                    world
                        .overlap_point_with_options(center, options)
                        .unwrap()
                        .separation,
                    -0.25,
                );
                let hit = world.overlap_point_with_options(center, options).unwrap();
                witnesses(hit, center, 0.);
            }

            #[test]
            fn queries_allocate_nothing_and_do_not_change_bodies_or_collider_state() {
                let mut world = PhysicsWorld::default();
                let body = world.add_ball(point(0., 0.), 1., true);
                world.bodies[body].set_linvel(Vector::from_array(point(1., 2.)), false);
                world.bodies[body].add_force(Vector::from_array(point(3., 4.)), false);
                let before = format!("{:?}{:?}", world.bodies, world.colliders);
                let sleep = world.sleep_stats();
                let mut output = [None; 2];
                super::ALLOCATIONS.with(|count| count.set(Some(0)));
                let result = world.overlap_sphere_into(
                    point(0., 0.),
                    0.5,
                    QueryOptions::new(0.),
                    &mut output,
                );
                let nearest =
                    world.overlap_point_with_options(point(0., 0.), QueryOptions::new(1.));
                let invalid = world.overlap_point_into(
                    point(f32::NAN, 0.),
                    QueryOptions::new(0.),
                    &mut output,
                );
                let count = super::ALLOCATIONS.with(|count| count.replace(None).unwrap());
                assert_eq!(count, 0);
                assert_eq!(result.unwrap().total, 1);
                assert!(nearest.is_some());
                assert_eq!(invalid, Err(QueryError::NonFiniteOrigin));
                assert_eq!(before, format!("{:?}{:?}", world.bodies, world.colliders));
                assert_eq!(sleep, world.sleep_stats());
            }

            #[test]
            fn unrepresentable_geometry_is_omitted_without_false_hits_or_panics() {
                let mut world = PhysicsWorld::default();
                world.add_ball(point(-f32::MAX, 0.), 1., false);
                let mut output = [None; 2];
                let report = world
                    .overlap_sphere_into(
                        point(f32::MAX, 0.),
                        0.5,
                        QueryOptions::new(f32::MAX),
                        &mut output,
                    )
                    .unwrap();
                assert_eq!(report, QueryReport::default());
                assert_eq!(output, [None; 2]);
            }

            #[test]
            fn reading_proximity_preserves_contact_events_and_sleep_requests() {
                let mut world = PhysicsWorld::default();
                let body = world.add_ball(point(0., 0.), 1., true);
                world.add_ball(point(1.5, 0.), 1., false);
                let _ = world.step(1. / 60.);
                assert!(!world.contact_events().is_empty());
                let events = world.contact_events().to_vec();
                let contacts = format!("{:?}", world.contacts().collect::<Vec<_>>());
                let sleep = world.sleep_state(body);
                let state = format!("{:?}{:?}", world.bodies, world.colliders);
                let mut output = [None; 4];
                let report = world
                    .overlap_point_into(point(0., 0.), QueryOptions::new(3.), &mut output)
                    .unwrap();
                assert_eq!(report.total, 2);
                assert_eq!(events, world.contact_events());
                assert_eq!(
                    contacts,
                    format!("{:?}", world.contacts().collect::<Vec<_>>())
                );
                assert_eq!(sleep, world.sleep_state(body));
                assert_eq!(state, format!("{:?}{:?}", world.bodies, world.colliders));
            }

            #[test]
            fn positive_gaps_are_not_expanded_by_the_solver_margin() {
                let mut world = PhysicsWorld::default();
                world.add_box(point(0., 0.), [1.; $dim], false);
                let center = point(1.0000001, 1.0000001);
                assert!(
                    world
                        .overlap_point_with_options(center, QueryOptions::new(0.))
                        .is_none()
                );
                let hit = world
                    .overlap_point_with_options(center, QueryOptions::new(0.001))
                    .unwrap();
                assert!(hit.separation > 0.);
                close(hit.normal[0], 0.5_f32.sqrt());
                close(hit.normal[1], 0.5_f32.sqrt());
                witnesses(hit, center, 0.);
                assert!(
                    world
                        .overlap_point_with_options(point(1.0011, 0.), QueryOptions::new(0.001))
                        .is_none()
                );
            }
        }
    };
}

#[cfg(feature = "dim2")]
overlap_tests!(
    d2,
    2,
    ferravex_physics::Rotation2::from_angle(std::f32::consts::FRAC_PI_2)
);
#[cfg(feature = "dim3")]
overlap_tests!(
    d3,
    3,
    ferravex_math::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
);

#[cfg(feature = "dim2")]
#[test]
fn capsule_centerline_point_segment_short_segment_and_rotated_witnesses() {
    use ferravex_math::Vec2;
    use ferravex_physics::d2::{ColliderBuilder, PhysicsWorld, QueryOptions, RigidBodyBuilder};
    for (a, b, center, radius) in [
        (Vec2::Y * -2., Vec2::Y * 2., [0., 0.], 0.5),
        (Vec2::ZERO, Vec2::ZERO, [0., 0.], 0.5),
        (Vec2::ZERO, Vec2::Y * 0.0000001, [0., 0.0000001], 0.00000001),
    ] {
        let mut world = PhysicsWorld::default();
        world.insert(
            RigidBodyBuilder::fixed(),
            ColliderBuilder::capsule_from_endpoints(a, b, radius),
        );
        let hit = world
            .overlap_point_with_options(center, QueryOptions::new(0.))
            .unwrap();
        assert!((hit.separation + radius).abs() < radius * 0.00001);
        assert!(hit.normal[0].abs() > 0.999 || a == b);
        let boundary = world
            .overlap_point_with_options(hit.collider_point, QueryOptions::new(0.00001))
            .unwrap();
        assert!(boundary.separation.abs() < radius * 0.00001);
    }
    let mut world = PhysicsWorld::default();
    world.insert(
        RigidBodyBuilder::fixed().rotation(std::f32::consts::FRAC_PI_2),
        ColliderBuilder::capsule_from_endpoints(-Vec2::Y, Vec2::Y, 0.5),
    );
    let hit = world
        .overlap_sphere_with_options([0.; 2], 0.25, QueryOptions::new(0.))
        .unwrap();
    assert!((hit.separation + 0.75).abs() < 0.00001);
    assert!((hit.normal[1] + 1.).abs() < 0.00001);
}

#[cfg(feature = "dim3")]
#[test]
fn cylinder_side_caps_rim_center_and_rotated_witnesses() {
    use ferravex_math::Vec3;
    use ferravex_physics::d3::{ColliderBuilder, PhysicsWorld, QueryOptions, RigidBodyBuilder};
    let mut world = PhysicsWorld::default();
    let (body, _) = world.insert(RigidBodyBuilder::fixed(), ColliderBuilder::cylinder(2., 1.));
    for (center, radius, distance) in [
        ([0.; 3], 0.5, -1.5),
        ([2., 0., 0.], 0.5, 0.5),
        ([0., 3., 0.], 0.5, 0.5),
        ([2., 3., 0.], 0., 2_f32.sqrt()),
        ([0., 1.5, 0.], 0., -0.5),
        ([1., 2., 0.], 0., 0.),
    ] {
        let hit = world
            .overlap_sphere_with_options(center, radius, QueryOptions::new(2.))
            .unwrap();
        assert!((hit.separation - distance).abs() < 0.00001);
        assert!((Vec3::from_array(hit.normal).length() - 1.).abs() < 0.00001);
        let target = Vec3::from_array(hit.collider_point);
        let probe = Vec3::from_array(hit.probe_point);
        assert!(
            (probe - target - Vec3::from_array(hit.normal) * hit.separation).length() < 0.00001
        );
        let boundary = world
            .overlap_point_with_options(hit.collider_point, QueryOptions::new(0.00001))
            .unwrap();
        assert!(boundary.separation.abs() < 0.00001);
    }
    world.bodies[body].set_rotation(
        ferravex_math::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        false,
    );
    let hit = world
        .overlap_point_with_options([3., 0., 0.], QueryOptions::new(2.))
        .unwrap();
    assert!((hit.separation - 1.).abs() < 0.00001);
    assert!((hit.normal[0] - 1.).abs() < 0.00001);
}
