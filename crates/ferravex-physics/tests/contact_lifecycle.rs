use ferravex_physics::{ContactEvent, SolverSettings};

const DT: f32 = 1. / 120.;

// The lifecycle contract is shared by both dimensional frontends.
macro_rules! lifecycle_tests {
    ($module:ident, $dimension:ident) => {
        mod $module {
            use super::*;
            use ferravex_physics::$dimension::{
                ColliderBuilder, ColliderHandle, PhysicsWorld, RigidBodyBuilder, RigidBodyHandle,
                Vector,
            };

            fn overlap(
                sensor: bool,
                distance: f32,
            ) -> (
                PhysicsWorld,
                RigidBodyHandle,
                ColliderHandle,
                RigidBodyHandle,
                ColliderHandle,
            ) {
                let mut world = PhysicsWorld::default();
                world
                    .set_solver_settings(SolverSettings {
                        sleep_delay: 0.05,
                        ..Default::default()
                    })
                    .unwrap();
                let (a, ca) = world.insert(
                    RigidBodyBuilder::dynamic(),
                    ColliderBuilder::ball(1.).sensor(sensor),
                );
                let (b, cb) = world.insert(
                    RigidBodyBuilder::dynamic().translation(Vector::X * distance),
                    ColliderBuilder::ball(1.),
                );
                (world, a, ca, b, cb)
            }

            fn settle(world: &mut PhysicsWorld) {
                for _ in 0..20 {
                    world.step(DT);
                }
            }

            #[test]
            fn removal_hides_stale_contacts_but_reports_stop_on_next_step() {
                let (mut world, a, ca, b, cb) = overlap(true, 1.);
                world.step(DT);
                assert_eq!(world.contact_events(), &[ContactEvent::Started(ca, cb)]);
                assert!(world.contact_pair(cb, ca).is_some());
                assert!(world.remove_body(b));
                assert!(!world.remove_body(b));
                assert!(world.contact_pair(ca, cb).is_none());
                assert!(world.contact_pair(cb, ca).is_none());
                assert_eq!(world.contacts().count(), 0);
                assert_eq!(world.contact_events(), &[ContactEvent::Started(ca, cb)]);
                assert!(world.remove_body(a));

                let stats = world.step(DT);
                assert!(world.bodies.is_empty());
                assert_eq!(stats.substeps, 1);
                assert_eq!(stats.broadphase_pairs, 0);
                assert_eq!(stats.contact_points, 0);
                assert_eq!(world.contact_events(), &[ContactEvent::Stopped(ca, cb)]);
                world.step(DT);
                assert!(world.contact_events().is_empty());
            }

            #[test]
            fn reused_slots_start_new_generations_and_stop_old_ones_once() {
                let (mut world, _, ca, mut body, mut collider) = overlap(true, 1.);
                world.step(DT);
                for _ in 0..12 {
                    assert!(world.remove_body(body));
                    let (fresh_body, fresh_collider) = world.insert(
                        RigidBodyBuilder::dynamic().translation(Vector::X),
                        ColliderBuilder::ball(1.),
                    );
                    assert_ne!(fresh_body, body);
                    assert_ne!(fresh_collider, collider);
                    assert!(!world.remove_body(body));
                    assert!(world.bodies.get(body).is_none());
                    assert!(world.colliders.get(collider).is_none());
                    assert!(world.contact_pair(ca, collider).is_none());
                    assert!(world.contact_pair(ca, fresh_collider).is_none());
                    world.step(DT);
                    assert_eq!(
                        world.contact_events(),
                        &[
                            ContactEvent::Started(ca, fresh_collider),
                            ContactEvent::Stopped(ca, collider),
                        ]
                    );
                    world.step(DT);
                    assert!(world.contact_events().is_empty());
                    body = fresh_body;
                    collider = fresh_collider;
                }
            }

            #[test]
            fn compound_child_removal_keeps_unaffected_contact_and_stops_each_pair_once() {
                let mut world = PhysicsWorld::default();
                let compound = world
                    .insert_compound(
                        RigidBodyBuilder::fixed(),
                        [
                            ColliderBuilder::ball(1.).translation(-Vector::X),
                            ColliderBuilder::ball(1.).translation(Vector::X),
                        ],
                    )
                    .unwrap();
                let children = world.bodies[compound].colliders().to_vec();
                let (_, sensor) = world.insert(
                    RigidBodyBuilder::fixed(),
                    ColliderBuilder::ball(3.).sensor(true),
                );
                world.step(DT);
                assert_eq!(world.contacts().count(), 2);
                world.remove_collider(children[0]).unwrap();
                assert!(world.remove_collider(children[0]).is_err());
                assert!(world.contact_pair(children[0], sensor).is_none());
                assert!(world.contact_pair(children[1], sensor).is_some());
                assert_eq!(world.contacts().count(), 1);
                world.step(DT);
                assert_eq!(
                    world.contact_events(),
                    &[ContactEvent::Stopped(children[0], sensor)]
                );
                assert!(world.remove_body(compound));
                world.step(DT);
                assert_eq!(
                    world.contact_events(),
                    &[ContactEvent::Stopped(children[1], sensor)]
                );
                world.step(DT);
                assert!(world.contact_events().is_empty());
            }

            #[test]
            fn events_are_canonical_sorted_starts_then_sorted_stops() {
                let mut world = PhysicsWorld::default();
                let mut bodies = Vec::new();
                let mut colliders = Vec::new();
                for i in 0..4 {
                    let (body, collider) = world.insert(
                        RigidBodyBuilder::fixed().translation(Vector::X * (3 - i) as f32 * 0.1),
                        ColliderBuilder::ball(1.)
                            .sensor(true)
                            .collision_layers(1, 0),
                    );
                    bodies.push(body);
                    colliders.push(collider);
                }
                world.step(DT);
                assert!(world.contact_events().is_empty());
                for &c in &colliders[..2] {
                    world.colliders[c].set_collision_layers(1, 1);
                }
                world.step(DT);
                assert_eq!(
                    world.contact_events(),
                    &[ContactEvent::Started(colliders[0], colliders[1])]
                );
                world.colliders[colliders[0]].set_collision_layers(1, 0);
                for &c in &colliders[2..] {
                    world.colliders[c].set_collision_layers(1, 1);
                }
                world.step(DT);
                assert_eq!(
                    world.contact_events(),
                    &[
                        ContactEvent::Started(colliders[1], colliders[2]),
                        ContactEvent::Started(colliders[1], colliders[3]),
                        ContactEvent::Started(colliders[2], colliders[3]),
                        ContactEvent::Stopped(colliders[0], colliders[1]),
                    ]
                );
                for &body in bodies.iter().rev() {
                    assert!(world.remove_body(body));
                }
                world.step(DT);
                assert_eq!(
                    world.contact_events(),
                    &[
                        ContactEvent::Stopped(colliders[1], colliders[2]),
                        ContactEvent::Stopped(colliders[1], colliders[3]),
                        ContactEvent::Stopped(colliders[2], colliders[3]),
                    ]
                );
            }

            #[test]
            fn invalid_steps_preserve_events_and_pending_removal() {
                let (mut world, a, ca, b, cb) = overlap(true, 1.);
                world.step(DT);
                let position = world.bodies[a].translation();
                for dt in [0., -DT, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.101] {
                    assert!(
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| world.step(dt)))
                            .is_err()
                    );
                    assert_eq!(world.contact_events(), &[ContactEvent::Started(ca, cb)]);
                    assert_eq!(world.contacts().count(), 1);
                    assert_eq!(world.bodies[a].translation(), position);
                    assert_eq!(world.last_step.substeps, 1);
                }
                world.gravity.x = f32::NAN;
                assert!(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| world.step(DT)))
                        .is_err()
                );
                assert_eq!(world.contact_events(), &[ContactEvent::Started(ca, cb)]);
                world.gravity.x = 0.;
                assert!(world.remove_body(b));
                assert!(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| world.step(0.)))
                        .is_err()
                );
                world.step(DT);
                assert_eq!(world.contact_events(), &[ContactEvent::Stopped(ca, cb)]);
                world.step(DT);
                assert!(world.contact_events().is_empty());
            }

            #[test]
            fn sensor_changes_keep_overlap_while_layers_end_and_restart_it() {
                let (mut world, _, ca, _, cb) = overlap(true, 2.);
                world.step(DT);
                for sensor in [false, true, false, true] {
                    world.colliders[ca].set_sensor(sensor);
                    world.step(DT);
                    let pair = world.contact_pair(ca, cb).unwrap();
                    assert_eq!(pair.sensor, sensor);
                    assert_eq!(pair.has_any_active_contact(), !sensor);
                    assert!(world.contact_events().is_empty());
                }
                world.colliders[ca].set_collision_layers(1, 0);
                world.step(DT);
                assert_eq!(world.contact_events(), &[ContactEvent::Stopped(ca, cb)]);
                assert!(world.contact_pair(ca, cb).is_none());
                world.colliders[ca].set_sensor(false);
                world.step(DT);
                assert!(world.contact_events().is_empty());
                world.colliders[ca].set_collision_layers(1, u32::MAX);
                world.step(DT);
                assert_eq!(world.contact_events(), &[ContactEvent::Started(ca, cb)]);
            }

            #[test]
            fn non_dynamic_pair_ends_when_its_last_sensor_is_disabled() {
                let mut world = PhysicsWorld::default();
                let (_, ca) = world.insert(
                    RigidBodyBuilder::fixed(),
                    ColliderBuilder::ball(1.).sensor(true),
                );
                let (_, cb) = world.insert(
                    RigidBodyBuilder::kinematic_position_based(),
                    ColliderBuilder::ball(1.),
                );
                world.step(DT);
                assert_eq!(world.contact_events(), &[ContactEvent::Started(ca, cb)]);
                world.colliders[ca].set_sensor(false);
                world.step(DT);
                assert_eq!(world.contact_events(), &[ContactEvent::Stopped(ca, cb)]);
                world.colliders[cb].set_sensor(true);
                world.step(DT);
                assert_eq!(world.contact_events(), &[ContactEvent::Started(ca, cb)]);
            }

            #[test]
            fn sleeping_and_waking_do_not_repeat_contact_events() {
                let (mut world, a, ca, b, cb) = overlap(false, 2.);
                world.step(DT);
                assert_eq!(world.contact_events(), &[ContactEvent::Started(ca, cb)]);
                for _ in 0..20 {
                    world.step(DT);
                    assert!(world.contact_events().is_empty());
                    assert_eq!(world.contacts().count(), 1);
                }
                assert!(world.bodies[a].is_sleeping());
                assert!(world.bodies[b].is_sleeping());
                world.bodies[a].wake_up(true);
                world.step(DT);
                assert!(!world.bodies[a].is_sleeping());
                assert!(!world.bodies[b].is_sleeping());
                assert!(world.contact_events().is_empty());
                world.colliders[ca].set_collision_layers(1, 0);
                world.step(DT);
                assert_eq!(world.contact_events(), &[ContactEvent::Stopped(ca, cb)]);
            }

            #[test]
            fn sleeping_sensor_overlap_wakes_when_it_becomes_solid() {
                let (mut world, a, ca, b, cb) = overlap(true, 0.5);
                settle(&mut world);
                assert!(world.bodies[a].is_sleeping());
                assert!(world.bodies[b].is_sleeping());
                world.colliders[ca].set_sensor(false);
                world.step(DT);
                assert!(!world.bodies[a].is_sleeping());
                assert!(!world.bodies[b].is_sleeping());
                assert!(world.bodies[b].translation().x - world.bodies[a].translation().x > 0.5);
                assert!(!world.contact_pair(ca, cb).unwrap().sensor);
                assert!(world.contact_events().is_empty());
            }

            #[test]
            fn enabling_layers_wakes_new_contacts_between_sleeping_bodies() {
                let (mut world, a, ca, b, cb) = overlap(false, 0.5);
                world.colliders[ca].set_collision_layers(1, 0);
                settle(&mut world);
                assert!(world.bodies[a].is_sleeping());
                assert!(world.bodies[b].is_sleeping());
                assert_eq!(world.contacts().count(), 0);
                world.colliders[ca].set_collision_layers(1, u32::MAX);
                world.step(DT);
                assert!(!world.bodies[a].is_sleeping());
                assert!(!world.bodies[b].is_sleeping());
                assert!(world.bodies[b].translation().x - world.bodies[a].translation().x > 0.5);
                assert_eq!(world.contact_events(), &[ContactEvent::Started(ca, cb)]);
            }

            #[test]
            fn moved_fixed_body_wakes_new_sleeping_contact() {
                let mut world = PhysicsWorld::default();
                world
                    .set_solver_settings(SolverSettings {
                        sleep_delay: 0.05,
                        ..Default::default()
                    })
                    .unwrap();
                let (fixed, ca) = world.insert(
                    RigidBodyBuilder::fixed().translation(Vector::X * 5.),
                    ColliderBuilder::ball(1.),
                );
                let (body, cb) =
                    world.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
                settle(&mut world);
                assert!(world.bodies[body].is_sleeping());
                world.bodies[fixed].set_translation(Vector::X, false);
                world.step(DT);
                assert!(!world.bodies[body].is_sleeping());
                assert!(world.bodies[body].translation().x < 0.);
                assert_eq!(world.contact_events(), &[ContactEvent::Started(ca, cb)]);
            }

            #[test]
            fn transient_substep_overlap_emits_one_start_and_one_later_stop() {
                let mut world = PhysicsWorld::default();
                let (_, sensor) = world.insert(
                    RigidBodyBuilder::fixed(),
                    ColliderBuilder::ball(0.2).sensor(true),
                );
                let (body, collider) = world.insert(
                    RigidBodyBuilder::dynamic()
                        .translation(-Vector::X)
                        .linvel(Vector::X * 240.)
                        .ccd_enabled(true),
                    ColliderBuilder::ball(0.1),
                );
                let stats = world.step(DT);
                assert_eq!(stats.substeps, 8);
                assert!(world.bodies[body].translation().x > 0.3);
                assert_eq!(world.contacts().count(), 1);
                assert_eq!(
                    world.contact_events(),
                    &[ContactEvent::Started(sensor, collider)]
                );
                world.step(DT);
                assert_eq!(
                    world.contact_events(),
                    &[ContactEvent::Stopped(sensor, collider)]
                );
                world.step(DT);
                assert!(world.contact_events().is_empty());
            }
        }
    };
}

#[cfg(feature = "dim2")]
lifecycle_tests!(planar, d2);
#[cfg(feature = "dim3")]
lifecycle_tests!(spatial, d3);
