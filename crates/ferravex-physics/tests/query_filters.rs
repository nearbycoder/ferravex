// Exercise the same public contract in each independently selectable dimension.
macro_rules! query_tests {
    ($module:ident, $dim:literal, $quarter_turn:expr) => {
        mod $module {
            use ferravex_math::{Quat, Vec3};
            use ferravex_physics::$module::{
                ColliderBuilder, PhysicsWorld, QueryOptions, RayHit, RigidBodyBuilder,
                SensorPolicy, Vector,
            };

            fn point(x: f32, y: f32) -> [f32; $dim] {
                let mut p = [0.; $dim];
                p[0] = x;
                p[1] = y;
                p
            }

            fn assert_close(actual: f32, expected: f32) {
                assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
            }

            fn assert_same_hit(actual: Option<RayHit>, expected: Option<RayHit>) {
                match (actual, expected) {
                    (Some(a), Some(b)) => {
                        assert_eq!(a.body, b.body);
                        assert_eq!(a.collider, b.collider);
                        assert_eq!(a.distance, b.distance);
                        assert_eq!(a.point, b.point);
                        assert_eq!(a.normal, b.normal);
                    }
                    (None, None) => {}
                    (a, b) => panic!("different hits: {a:?}, {b:?}"),
                }
            }

            #[test]
            fn layers_ignore_every_child_and_use_membership_not_response_filter() {
                let mut world = PhysicsWorld::default();
                world.insert(
                    RigidBodyBuilder::fixed().translation(Vector::from_array(point(1., 0.))),
                    ColliderBuilder::ball(0.25).sensor(true),
                );
                let player = world
                    .insert_compound(
                        RigidBodyBuilder::fixed(),
                        [2., 3.].map(|x| {
                            ColliderBuilder::ball(0.25)
                                .translation(Vector::from_array(point(x, 0.)))
                                .collision_layers(0b100, u32::MAX)
                        }),
                    )
                    .unwrap();
                world.insert(
                    RigidBodyBuilder::fixed().translation(Vector::from_array(point(4., 0.))),
                    ColliderBuilder::ball(0.5).collision_layers(0b001, u32::MAX),
                );
                let (target, collider) = world.insert(
                    RigidBodyBuilder::fixed().translation(Vector::from_array(point(6., 0.))),
                    ColliderBuilder::ball(0.5).collision_layers(0b100, 0),
                );
                let options = QueryOptions::new(10.).layers(0b100).ignore_body(player);
                let origin = point(0., 0.);
                let direction = point(1., 0.);
                let ray = world
                    .raycast_with_options(origin, direction, options)
                    .unwrap();
                assert_eq!(ray.body, Some(target));
                assert_eq!(ray.collider, collider);
                assert_close(ray.distance, 5.5);
                let sphere = world
                    .spherecast_with_options(origin, direction, 0.25, options)
                    .unwrap();
                assert_eq!(sphere.body, Some(target));
                assert_close(sphere.distance, 5.25);
                assert_same_hit(
                    Some(sphere),
                    world.spherecast(origin, direction, 10., 0.25, 0b100, Some(player)),
                );
                assert!(
                    world
                        .raycast_with_options(origin, direction, options.layers(0))
                        .is_none()
                );
                assert!(
                    world
                        .spherecast_with_options(origin, direction, 0.25, options.layers(0))
                        .is_none()
                );
                assert_eq!(
                    world
                        .raycast_with_options(origin, direction, QueryOptions::new(10.))
                        .unwrap()
                        .body,
                    Some(player),
                );
            }

            #[test]
            fn sensor_policies_compose_with_layers_and_ignored_bodies() {
                let mut world = PhysicsWorld::default();
                let (far_sensor, _) = world.insert(
                    RigidBodyBuilder::fixed().translation(Vector::from_array(point(6., 0.))),
                    ColliderBuilder::ball(0.5)
                        .sensor(true)
                        .collision_layers(0b01, 0),
                );
                let solid = world.add_ball(point(4., 0.), 0.5, false);
                let (near_sensor, near_collider) = world.insert(
                    RigidBodyBuilder::fixed().translation(Vector::from_array(point(2., 0.))),
                    ColliderBuilder::ball(0.5)
                        .sensor(true)
                        .collision_layers(0b10, 0),
                );
                let origin = point(0., 0.);
                let direction = point(1., 0.);
                for radius in [0., 0.25] {
                    for (policy, expected) in [
                        (SensorPolicy::Exclude, solid),
                        (SensorPolicy::Include, near_sensor),
                        (SensorPolicy::Only, near_sensor),
                    ] {
                        let options = QueryOptions::new(10.).sensors(policy);
                        assert_eq!(
                            world
                                .spherecast_with_options(origin, direction, radius, options)
                                .unwrap()
                                .body,
                            Some(expected),
                        );
                        assert_eq!(
                            world
                                .raycast_with_options(origin, direction, options)
                                .unwrap()
                                .body,
                            Some(expected),
                        );
                    }
                    let options = QueryOptions::new(10.).sensors(SensorPolicy::Only);
                    for filtered in [options.ignore_body(near_sensor), options.layers(0b01)] {
                        assert_eq!(
                            world
                                .spherecast_with_options(origin, direction, radius, filtered)
                                .unwrap()
                                .body,
                            Some(far_sensor),
                        );
                    }
                    assert!(
                        world
                            .spherecast_with_options(
                                origin,
                                direction,
                                radius,
                                options.layers(0b10).ignore_body(near_sensor),
                            )
                            .is_none()
                    );
                }
                // Live sensor edits are visible without stepping or refreshing.
                world.colliders[near_collider].set_sensor(false);
                let options = QueryOptions::new(10.);
                assert_eq!(
                    world
                        .raycast_with_options(origin, direction, options)
                        .unwrap()
                        .body,
                    Some(near_sensor)
                );
                assert_eq!(
                    world
                        .raycast_with_options(
                            origin,
                            direction,
                            options.sensors(SensorPolicy::Only)
                        )
                        .unwrap()
                        .body,
                    Some(far_sensor)
                );
            }

            #[test]
            fn current_body_child_pose_and_layer_edits_need_no_refresh() {
                let mut world = PhysicsWorld::default();
                let (body, child) = world.insert(
                    RigidBodyBuilder::fixed().translation(Vector::from_array(point(4., 0.))),
                    ColliderBuilder::ball(0.5).translation(Vector::from_array(point(0., 2.))),
                );
                let options = QueryOptions::new(20.).layers(0b10);
                let origin = point(0., 0.);
                let direction = point(1., 0.);
                assert!(
                    world
                        .raycast_with_options(origin, direction, options)
                        .is_none()
                );
                world.bodies[body].set_rotation($quarter_turn, false);
                assert_close(
                    world
                        .raycast_with_options(origin, direction, options)
                        .unwrap()
                        .distance,
                    5.5,
                );
                world.bodies[body].set_translation(Vector::from_array(point(8., 0.)), false);
                assert_close(
                    world
                        .spherecast_with_options(origin, direction, 0.25, options)
                        .unwrap()
                        .distance,
                    9.25,
                );
                world
                    .set_collider_pose(child, Vec3::new(0., 3., 0.), Quat::IDENTITY)
                    .unwrap();
                assert_close(
                    world
                        .raycast_with_options(origin, direction, options)
                        .unwrap()
                        .distance,
                    10.5,
                );
                assert_close(
                    world
                        .spherecast_with_options(origin, direction, 0.25, options)
                        .unwrap()
                        .distance,
                    10.25,
                );
                world.colliders[child].set_collision_layers(0b01, u32::MAX);
                assert!(
                    world
                        .raycast_with_options(origin, direction, options)
                        .is_none()
                );
                world.colliders[child].set_collision_layers(0b10, 0);
                assert_eq!(
                    world
                        .raycast_with_options(origin, direction, options)
                        .unwrap()
                        .collider,
                    child
                );
            }

            #[test]
            fn rotating_a_child_changes_query_geometry_immediately() {
                let mut world = PhysicsWorld::default();
                let mut half = [0.1; $dim];
                half[0] = 2.;
                let body = world.add_box(point(4., 0.), half, false);
                let child = world.bodies[body].colliders()[0];
                let options = QueryOptions::new(10.);
                let origin = point(0., 0.);
                let direction = point(1., 0.);
                assert_close(
                    world
                        .raycast_with_options(origin, direction, options)
                        .unwrap()
                        .distance,
                    2.,
                );
                world
                    .set_collider_pose(
                        child,
                        Vec3::ZERO,
                        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
                    )
                    .unwrap();
                assert_close(
                    world
                        .raycast_with_options(origin, direction, options)
                        .unwrap()
                        .distance,
                    3.9,
                );
                assert_close(
                    world
                        .spherecast_with_options(origin, direction, 0.25, options)
                        .unwrap()
                        .distance,
                    3.65,
                );
            }

            #[test]
            fn volume_radius_distance_bounds_and_normalized_directions_are_respected() {
                let mut world = PhysicsWorld::default();
                let target = world.add_ball(point(3., 1.), 0.5, false);
                let origin = point(0., 0.);
                let options = QueryOptions::new(10.);
                // The center ray misses, but the swept volume hits at x = 2.25.
                for speed in [0.01, 1., 30., f32::MAX, f32::from_bits(1)] {
                    let direction = point(speed, 0.);
                    assert!(
                        world
                            .raycast_with_options(origin, direction, options)
                            .is_none()
                    );
                    let hit = world
                        .spherecast_with_options(origin, direction, 0.75, options)
                        .unwrap();
                    assert_eq!(hit.body, Some(target));
                    assert_close(hit.distance, 2.25);
                    assert_close(hit.point[0], hit.distance);
                    assert_close(hit.normal[0], -0.6);
                    assert_close(hit.normal[1], -0.8);
                    assert!(
                        world
                            .spherecast_with_options(
                                origin,
                                direction,
                                0.75,
                                QueryOptions::new(2.2)
                            )
                            .is_none()
                    );
                }
                world.bodies[target].set_translation(Vector::from_array(point(3., 0.)), false);
                let hit = world
                    .raycast_with_options(origin, point(1., 0.), QueryOptions::new(2.5))
                    .unwrap();
                assert_close(hit.distance, 2.5);
                assert!(
                    world
                        .raycast_with_options(origin, point(1., 0.), QueryOptions::new(2.4))
                        .is_none()
                );
            }

            #[test]
            fn invalid_numeric_inputs_are_rejected_without_mutation() {
                let mut world = PhysicsWorld::default();
                let body = world.add_ball(point(3., 0.), 0.5, false);
                let origin = point(0., 0.);
                let direction = point(1., 0.);
                let before = world.raycast(origin, direction, 10.);
                for policy in [
                    SensorPolicy::Exclude,
                    SensorPolicy::Include,
                    SensorPolicy::Only,
                ] {
                    let options = QueryOptions::new(10.).sensors(policy);
                    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                        for axis in 0..$dim {
                            let mut bad_origin = origin;
                            bad_origin[axis] = bad;
                            let mut bad_direction = direction;
                            bad_direction[axis] = bad;
                            assert!(
                                world
                                    .raycast_with_options(bad_origin, direction, options)
                                    .is_none()
                            );
                            assert!(
                                world
                                    .raycast_with_options(origin, bad_direction, options)
                                    .is_none()
                            );
                            assert!(
                                world
                                    .spherecast_with_options(bad_origin, direction, 0.25, options)
                                    .is_none()
                            );
                            assert!(
                                world
                                    .spherecast_with_options(origin, bad_direction, 0.25, options)
                                    .is_none()
                            );
                        }
                    }
                    for distance in [-1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                        let invalid = QueryOptions {
                            max_distance: distance,
                            ..options
                        };
                        assert!(
                            world
                                .raycast_with_options(origin, direction, invalid)
                                .is_none()
                        );
                        assert!(
                            world
                                .spherecast_with_options(origin, direction, 0.25, invalid)
                                .is_none()
                        );
                    }
                    for radius in [-1., 1000.1, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                        assert!(
                            world
                                .spherecast_with_options(origin, direction, radius, options)
                                .is_none()
                        );
                    }
                    assert!(
                        world
                            .raycast_with_options(point(3., 0.), [0.; $dim], options)
                            .is_none()
                    );
                    assert!(
                        world
                            .spherecast_with_options(point(3., 0.), [0.; $dim], 0.25, options)
                            .is_none()
                    );
                    // Finite inputs whose endpoint overflows are invalid as well.
                    let overflow = QueryOptions {
                        max_distance: f32::MAX,
                        ..options
                    };
                    assert!(
                        world
                            .raycast_with_options(point(f32::MAX, 0.), direction, overflow)
                            .is_none()
                    );
                    assert!(
                        world
                            .spherecast_with_options(point(f32::MAX, 0.), direction, 0.25, overflow)
                            .is_none()
                    );
                }
                assert_same_hit(world.raycast(origin, direction, 10.), before);
                assert_eq!(
                    world.bodies[body].translation(),
                    Vector::from_array(point(3., 0.))
                );
                assert_eq!(world.bodies.len(), 1);
                assert_eq!(world.colliders.len(), 1);
                assert!(world.contact_events().is_empty());
            }

            #[test]
            fn overlap_zero_radius_and_ties_preserve_legacy_results() {
                let mut world = PhysicsWorld::default();
                world.add_ball(point(8., 0.), 0.5, false);
                world.add_ball(point(3., 0.), 0.5, false);
                let later = world.add_ball(point(3., 0.), 0.5, false);
                let origin = point(0., 0.);
                let direction = point(1., 0.);
                let options = QueryOptions::new(10.);
                let ray = world.raycast_with_options(origin, direction, options);
                assert_eq!(ray.unwrap().body, Some(later));
                assert_same_hit(ray, world.raycast(origin, direction, 10.));
                assert_same_hit(
                    ray,
                    world.spherecast_with_options(origin, direction, 0., options),
                );
                assert_same_hit(
                    world.spherecast_with_options(origin, direction, 0.25, options),
                    world.spherecast(origin, direction, 10., 0.25, u32::MAX, None),
                );
                let overlap = QueryOptions::new(0.);
                for radius in [0., 0.25, 1000.] {
                    let hit = world
                        .spherecast_with_options(point(3., 0.), direction, radius, overlap)
                        .unwrap();
                    assert_eq!(hit.distance, 0.);
                    assert_eq!(hit.point, point(3., 0.));
                    assert!(hit.normal.iter().all(|v| v.is_finite()));
                }
                assert_eq!(
                    world
                        .raycast_with_options(point(3., 0.), direction, overlap)
                        .unwrap()
                        .distance,
                    0.
                );
            }

            #[test]
            fn removed_ignored_handle_does_not_hide_reused_slot_or_attached_children() {
                let mut world = PhysicsWorld::default();
                let removed = world.add_ball(point(3., 0.), 0.5, false);
                let options = QueryOptions::new(10.).ignore_body(removed);
                let origin = point(0., 0.);
                let direction = point(1., 0.);
                assert!(
                    world
                        .raycast_with_options(origin, direction, options)
                        .is_none()
                );
                assert!(world.remove_body(removed));
                let replacement = world.add_ball(point(6., 0.), 0.5, false);
                assert_ne!(replacement, removed);
                assert_eq!(
                    world
                        .raycast_with_options(origin, direction, options)
                        .unwrap()
                        .body,
                    Some(replacement)
                );
                let child = world
                    .attach_collider(
                        replacement,
                        ColliderBuilder::ball(0.5).translation(Vector::from_array(point(-3., 0.))),
                    )
                    .unwrap();
                assert_eq!(
                    world
                        .raycast_with_options(origin, direction, options)
                        .unwrap()
                        .collider,
                    child
                );
                assert!(
                    world
                        .spherecast_with_options(
                            origin,
                            direction,
                            0.25,
                            options.ignore_body(replacement)
                        )
                        .is_none()
                );
                world.remove_collider(child).unwrap();
                assert_close(
                    world
                        .raycast_with_options(origin, direction, options)
                        .unwrap()
                        .distance,
                    5.5,
                );
            }
        }
    };
}

#[cfg(feature = "dim2")]
query_tests!(
    d2,
    2,
    ferravex_physics::Rotation2::from_angle(-std::f32::consts::FRAC_PI_2)
);
#[cfg(feature = "dim3")]
query_tests!(d3, 3, Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2));
