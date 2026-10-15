// Run the same behavioral contract against both independent Rapier engines.
#[cfg(any(feature = "dim2", feature = "dim3"))]
macro_rules! physics_tests {
    ($module:ident, $dimension:literal) => {
        mod $module {
            use ferravex_physics::$module::*;

            fn axis_y(value: f32) -> [f32; $dimension] {
                let mut vector = [0.0; $dimension];
                vector[1] = value;
                vector
            }

            #[test]
            fn falling_ball_settles_on_ground() {
                let mut world = PhysicsWorld::new(axis_y(-9.81));
                let ground = world.add_box(axis_y(-0.5), [0.5; $dimension], false);
                let ball = world.add_ball(axis_y(3.0), 0.5, true);

                for _ in 0..360 {
                    world.step(1.0 / 60.0);
                }

                assert!((world.bodies[ball].translation().y - 0.5).abs() < 0.02);
                assert!(world.bodies[ball].linvel().length() < 0.02);
                assert_eq!(world.bodies[ground].translation().y, -0.5);
                assert!(
                    world
                        .narrow_phase
                        .contact_pairs()
                        .any(|pair| pair.has_any_active_contact())
                );
            }

            #[test]
            fn impulse_changes_velocity_and_position() {
                let mut world = PhysicsWorld::new([0.0; $dimension]);
                let ball = world.add_ball([0.0; $dimension], 0.5, true);
                world.step(1.0 / 60.0);
                world.bodies[ball].apply_impulse(Vector::from_array(axis_y(2.0)), true);
                let velocity = world.bodies[ball].linvel().y;
                assert!(velocity > 0.0);

                world.step(1.0 / 60.0);
                assert!(world.bodies[ball].translation().y > 0.0);
                assert!((world.bodies[ball].linvel().y - velocity).abs() < 1.0e-5);
            }

            #[test]
            fn raycast_normalizes_direction_and_filters_bodies() {
                let mut world = PhysicsWorld::new([0.0; $dimension]);
                let target = world.add_box([0.0; $dimension], [1.0; $dimension], false);
                world.refresh_queries();

                let hit = world.raycast(axis_y(3.0), axis_y(-2.0), 10.0).unwrap();
                assert_eq!(hit.body, Some(target));
                assert!((hit.distance - 2.0).abs() < 1.0e-5);
                assert!((hit.point[1] - 1.0).abs() < 1.0e-5);
                assert!((hit.normal[1] - 1.0).abs() < 1.0e-5);
                assert!(world.raycast(axis_y(3.0), axis_y(-2.0), 1.0).is_none());
                assert!(
                    world
                        .raycast_with_filter(
                            axis_y(3.0),
                            axis_y(-2.0),
                            10.0,
                            QueryFilter::default().exclude_rigid_body(target),
                        )
                        .is_none()
                );

                let mut moved = [0.0; $dimension];
                moved[0] = 4.0;
                world.bodies[target].set_translation(Vector::from_array(moved), true);
                world.refresh_queries();
                assert!(world.raycast(axis_y(3.0), axis_y(-1.0), 10.0).is_none());
            }

            #[test]
            fn ccd_prevents_tunneling_through_thin_floor() {
                let mut world = PhysicsWorld::new([0.0; $dimension]);
                let mut floor_extents = [5.0; $dimension];
                floor_extents[1] = 0.01;
                world.add_box([0.0; $dimension], floor_extents, false);
                let projectile = world.add_ball(axis_y(1.0), 0.05, true);
                world.bodies[projectile].enable_ccd(true);
                world.bodies[projectile].set_linvel(Vector::from_array(axis_y(-150.0)), true);
                world.step(1.0 / 60.0);
                assert!(world.bodies[projectile].translation().y > 0.0);
            }

            #[test]
            fn removing_body_removes_its_colliders_and_query_hits() {
                let mut world = PhysicsWorld::new([0.0; $dimension]);
                let ball = world.add_ball([0.0; $dimension], 1.0, false);
                world.refresh_queries();
                assert!(world.raycast(axis_y(3.0), axis_y(-1.0), 10.0).is_some());
                assert!(world.remove_body(ball).is_some());
                world.refresh_queries();
                assert!(world.bodies.is_empty());
                assert!(world.colliders.is_empty());
                assert!(world.raycast(axis_y(3.0), axis_y(-1.0), 10.0).is_none());
            }

            #[test]
            fn invalid_ray_inputs_never_hit() {
                let mut world = PhysicsWorld::new([0.0; $dimension]);
                world.add_ball([0.0; $dimension], 1.0, false);
                world.refresh_queries();
                assert!(
                    world
                        .raycast(axis_y(3.0), [0.0; $dimension], 10.0)
                        .is_none()
                );
                assert!(
                    world
                        .raycast(axis_y(f32::NAN), axis_y(-1.0), 10.0)
                        .is_none()
                );
                assert!(
                    world
                        .raycast(axis_y(3.0), axis_y(f32::INFINITY), 10.0)
                        .is_none()
                );
                assert!(world.raycast(axis_y(3.0), axis_y(-1.0), -10.0).is_none());
            }

            #[test]
            #[should_panic(expected = "physics dt must be finite and positive")]
            fn invalid_timestep_is_rejected() {
                PhysicsWorld::default().step(f32::NAN);
            }
        }
    };
}

#[cfg(feature = "dim2")]
physics_tests!(d2, 2);

#[cfg(feature = "dim3")]
physics_tests!(d3, 3);
