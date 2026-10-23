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
// SAFETY: The unchanged allocation contract is forwarded to System. Counters
// are thread-local, do not allocate, and ignore destruction-time TLS failures.
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

macro_rules! teleport_tests {
    ($module:ident, $dim:literal, $rotation:expr, $angular:expr) => {
        mod $module {
            use ferravex_physics::$module::{
                ColliderBuilder, JointBuilder, PhysicsWorld, QueryOptions, RigidBodyBuilder,
                RigidBodyPose, TeleportError, TeleportOptions, Vector, VelocityPolicy,
            };
            use ferravex_physics::{ContactEvent, PhysicsError, SolverSettings};
            const DT: f32 = 1. / 60.;
            fn point(x: f32, y: f32) -> [f32; $dim] {
                let mut p = [0.; $dim];
                p[0] = x;
                p[1] = y;
                p
            }
            fn pose(x: f32, y: f32) -> RigidBodyPose {
                RigidBodyPose {
                    translation: Vector::from_array(point(x, y)),
                    rotation: $rotation(0.),
                }
            }
            fn reset() -> TeleportOptions {
                TeleportOptions::new(VelocityPolicy::Clear)
            }
            fn close(a: Vector, b: Vector) {
                assert!((a - b).length() < 0.0001, "{a:?} != {b:?}");
            }
            fn settle(world: &mut PhysicsWorld) {
                for _ in 0..120 {
                    world.step(DT);
                }
            }

            #[test]
            fn dynamic_velocity_force_and_target_policies_are_independent() {
                for velocity in [VelocityPolicy::Clear, VelocityPolicy::Preserve] {
                    for clear_forces in [false, true] {
                        for clear_kinematic_targets in [false, true] {
                            let mut world = PhysicsWorld::default();
                            let (body, _) = world.insert(
                                RigidBodyBuilder::dynamic()
                                    .linvel(Vector::X * 2.)
                                    .angvel($angular(0.3)),
                                ColliderBuilder::ball(1.),
                            );
                            let mass = world.bodies[body].mass();
                            let b = &mut world.bodies[body];
                            b.add_force(Vector::Y * (mass * 3.), true);
                            b.add_torque($angular(mass), true);
                            b.set_next_kinematic_translation(Vector::X * 100.);
                            b.set_next_kinematic_rotation($rotation(1.));
                            let destination = RigidBodyPose {
                                rotation: $rotation(0.7),
                                ..pose(4., 5.)
                            };
                            let committed = world
                                .try_teleport_body(
                                    body,
                                    destination,
                                    TeleportOptions {
                                        velocity,
                                        clear_forces,
                                        clear_kinematic_targets,
                                    },
                                )
                                .unwrap();
                            close(committed.translation, destination.translation);
                            close(
                                *world.bodies[body].rotation() * Vector::X,
                                destination.rotation * Vector::X,
                            );
                            let kept = velocity == VelocityPolicy::Preserve;
                            assert_eq!(
                                world.bodies[body].linvel(),
                                Vector::X * if kept { 2. } else { 0. }
                            );
                            assert_eq!(
                                world.bodies[body].angvel(),
                                $angular(if kept { 0.3 } else { 0. })
                            );
                            assert!(!world.bodies[body].is_sleeping());
                            world.step(DT);
                            let expected = Vector::X * if kept { 2. } else { 0. }
                                + Vector::Y * if clear_forces { 0. } else { 3. * DT };
                            close(world.bodies[body].linvel(), expected);
                            close(
                                world.bodies[body].translation(),
                                destination.translation + expected * DT,
                            );
                            if clear_forces {
                                assert_eq!(
                                    world.bodies[body].angvel(),
                                    $angular(if kept { 0.3 } else { 0. })
                                );
                            } else {
                                assert_ne!(
                                    world.bodies[body].angvel(),
                                    $angular(if kept { 0.3 } else { 0. })
                                );
                            }
                            // Force/torque is consumed once, not resurrected by reset.
                            let angular_after = world.bodies[body].angvel();
                            world.step(DT);
                            close(world.bodies[body].linvel(), expected);
                            assert_eq!(world.bodies[body].angvel(), angular_after);
                        }
                    }
                }
            }

            #[test]
            fn kinematic_targets_are_cleared_or_honored_on_the_next_step() {
                for velocity in [VelocityPolicy::Clear, VelocityPolicy::Preserve] {
                    for clear_targets in [false, true] {
                        let mut world = PhysicsWorld::default();
                        let (body, child) = world.insert(
                            RigidBodyBuilder::kinematic_position_based()
                                .linvel(Vector::X * 2.)
                                .angvel($angular(0.3)),
                            ColliderBuilder::ball(0.5),
                        );
                        world.bodies[body].set_next_kinematic_translation(Vector::X * 5.);
                        world.bodies[body].set_next_kinematic_rotation($rotation(0.8));
                        let committed = world
                            .try_teleport_body(
                                body,
                                pose(3., 0.),
                                TeleportOptions {
                                    clear_kinematic_targets: clear_targets,
                                    ..TeleportOptions::new(velocity)
                                },
                            )
                            .unwrap();
                        assert_eq!(
                            world.bodies[body].linvel(),
                            Vector::X
                                * if velocity == VelocityPolicy::Preserve {
                                    2.
                                } else {
                                    0.
                                }
                        );
                        assert_eq!(
                            world.bodies[body].angvel(),
                            $angular(if velocity == VelocityPolicy::Preserve {
                                0.3
                            } else {
                                0.
                            })
                        );
                        world.step(DT);
                        assert_eq!(
                            world.bodies[body].translation(),
                            if clear_targets {
                                committed.translation
                            } else {
                                Vector::X * 5.
                            }
                        );
                        close(
                            *world.bodies[body].rotation() * Vector::X,
                            if clear_targets {
                                Vector::X
                            } else {
                                $rotation(0.8) * Vector::X
                            },
                        );
                        if clear_targets {
                            assert_eq!(world.bodies[body].linvel(), Vector::ZERO);
                            assert_eq!(world.bodies[body].angvel(), $angular(0.));
                        } else {
                            close(world.bodies[body].linvel(), Vector::X * (2. / DT));
                            assert_ne!(world.bodies[body].angvel(), $angular(0.));
                        }
                        let final_position = world.bodies[body].translation();
                        world.step(DT);
                        assert_eq!(world.bodies[body].translation(), final_position);
                        assert_eq!(world.bodies[body].linvel(), Vector::ZERO);
                        assert_eq!(world.bodies[body].colliders(), &[child]);
                    }
                }
            }

            #[test]
            fn compound_origin_mass_identity_and_velocity_are_preserved() {
                let mut world = PhysicsWorld::default();
                let body = world
                    .insert_compound(
                        RigidBodyBuilder::dynamic()
                            .linvel(Vector::X * 2.)
                            .angvel($angular(0.5))
                            .ccd_enabled(true)
                            .linear_damping(0.1)
                            .angular_damping(0.2)
                            .gravity_scale(0.3),
                        [
                            ColliderBuilder::ball(1.).translation(Vector::X),
                            ColliderBuilder::ball(0.5).translation(Vector::X * 3.),
                        ],
                    )
                    .unwrap();
                let children = world.bodies[body].colliders().to_vec();
                let collider_states: Vec<_> = children
                    .iter()
                    .map(|&c| format!("{:?}", world.colliders[c]))
                    .collect();
                let local_com = world.bodies[body].local_center_of_mass();
                assert!(local_com.x > 1.);
                let mass = world.bodies[body].mass();
                let destination = RigidBodyPose {
                    rotation: $rotation(0.7),
                    ..pose(4., 5.)
                };
                let committed = world
                    .try_teleport_body(
                        body,
                        destination,
                        TeleportOptions::new(VelocityPolicy::Preserve),
                    )
                    .unwrap();
                assert_eq!(committed.translation, destination.translation);
                assert_eq!(world.bodies[body].local_center_of_mass(), local_com);
                close(
                    world.bodies[body].center_of_mass(),
                    destination.translation + destination.rotation * local_com,
                );
                assert_eq!(world.bodies[body].mass(), mass);
                assert_eq!(world.bodies[body].linvel(), Vector::X * 2.);
                assert_eq!(world.bodies[body].angvel(), $angular(0.5));
                assert_eq!(world.bodies[body].colliders(), children.as_slice());
                assert!(world.bodies[body].is_ccd_enabled());
                for (i, &child) in children.iter().enumerate() {
                    assert_eq!(format!("{:?}", world.colliders[child]), collider_states[i]);
                    let p = destination.translation
                        + destination.rotation * Vector::X * if i == 0 { 1. } else { 3. };
                    let hit = world
                        .overlap_point_with_options(p.to_array(), QueryOptions::new(0.))
                        .unwrap();
                    assert_eq!(hit.collider, child);
                    assert_eq!(hit.body, Some(body));
                }
            }

            #[test]
            fn invalid_translations_preserve_entire_body_and_live_snapshots() {
                let mut world = PhysicsWorld::default();
                let (body, child) =
                    world.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(0.5));
                let (_, sensor) = world.insert(
                    RigidBodyBuilder::fixed(),
                    ColliderBuilder::ball(1.).sensor(true),
                );
                world.step(DT);
                world.bodies[body].add_force(Vector::X, true);
                world.bodies[body].add_torque($angular(1.), true);
                world.bodies[body].set_next_kinematic_translation(Vector::X * 5.);
                world.bodies[body].set_next_kinematic_rotation($rotation(1.));
                let before = format!("{:?}", world.bodies[body]);
                let events = world.contact_events().to_vec();
                let sleep = world.sleep_state(body);
                for axis in 0..$dim {
                    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                        let mut p = point(4., 5.);
                        p[axis] = invalid;
                        assert_eq!(
                            world.try_teleport_body(
                                body,
                                RigidBodyPose {
                                    translation: Vector::from_array(p),
                                    rotation: $rotation(0.7),
                                },
                                reset()
                            ),
                            Err(TeleportError::Translation)
                        );
                        assert_eq!(format!("{:?}", world.bodies[body]), before);
                        assert_eq!(world.contact_events(), events);
                        assert_eq!(world.sleep_state(body), sleep);
                        assert!(world.contact_pair(child, sensor).is_some());
                    }
                }
                // Failed reset did not clear the pending force or torque.
                world.step(DT);
                assert!(world.bodies[body].linvel().x > 0.);
                assert_ne!(world.bodies[body].angvel(), $angular(0.));
            }

            #[test]
            fn stale_handle_is_checked_first_and_cannot_edit_reused_slot() {
                let mut world = PhysicsWorld::default();
                let body = world.add_ball(point(0., 0.), 1., true);
                world.remove_body(body);
                let replacement = world.add_ball(point(2., 3.), 1., true);
                let before = format!("{:?}", world.bodies[replacement]);
                assert_ne!(body, replacement);
                assert_eq!(
                    world.try_teleport_body(
                        body,
                        RigidBodyPose {
                            translation: Vector::from_array([f32::NAN; $dim]),
                            ..pose(0., 0.)
                        },
                        reset()
                    ),
                    Err(TeleportError::InvalidHandle)
                );
                assert_eq!(format!("{:?}", world.bodies[replacement]), before);
            }

            #[test]
            fn same_pose_reset_cold_starts_only_affected_contact_features() {
                let mut world = PhysicsWorld::new(point(0., -10.));
                let floor = world.add_box(point(0., -0.5), [1.; $dim], false);
                let body = world.add_ball(point(0., 1.5), 1., true);
                let other_floor = world.add_box(point(20., -0.5), [1.; $dim], false);
                let other = world.add_ball(point(20., 1.5), 1., true);
                settle(&mut world);
                assert!(world.bodies[body].is_sleeping());
                let child = world.bodies[body].colliders()[0];
                let other_child = world.bodies[other].colliders()[0];
                let ids = |world: &PhysicsWorld, c| {
                    world
                        .contact_manifolds()
                        .filter(|m| m.collider1 == c || m.collider2 == c)
                        .flat_map(|m| m.points.iter().map(|p| p.id))
                        .collect::<Vec<_>>()
                };
                let old_ids = ids(&world, child);
                let other_ids = ids(&world, other_child);
                assert!(!old_ids.is_empty() && !other_ids.is_empty());
                let destination = RigidBodyPose {
                    translation: world.bodies[body].translation(),
                    rotation: *world.bodies[body].rotation(),
                };
                let committed = world.try_teleport_body(body, destination, reset()).unwrap();
                assert_eq!(committed, destination);
                assert!(!world.bodies[body].is_sleeping());
                assert_eq!(ids(&world, child), old_ids); // Last-step snapshot retained.
                world.step(DT);
                let fresh_ids = ids(&world, child);
                assert!(!fresh_ids.is_empty());
                assert!(fresh_ids.iter().all(|id| !old_ids.contains(id)));
                assert_eq!(ids(&world, other_child), other_ids);
                assert!(world.bodies[other].is_sleeping());
                assert!(world.contact_events().is_empty());
                assert!(
                    world.bodies.get(floor).is_some() && world.bodies.get(other_floor).is_some()
                );
            }

            #[test]
            fn moved_support_wakes_previous_contacts_and_reports_stop_once() {
                for kinematic in [false, true] {
                    let mut world = PhysicsWorld::new(point(0., -10.));
                    let (support, ca) = world.insert(
                        if kinematic {
                            RigidBodyBuilder::kinematic_position_based()
                        } else {
                            RigidBodyBuilder::fixed()
                        },
                        ColliderBuilder::ball(1.),
                    );
                    let (body, cb) = world.insert(
                        RigidBodyBuilder::dynamic().translation(Vector::Y * 2.),
                        ColliderBuilder::ball(1.),
                    );
                    let upper = world.add_ball(point(0., 4.), 1., true);
                    let isolated = world.add_ball(point(20., 0.), 1., true);
                    world.bodies[isolated].set_gravity_scale(0., false);
                    settle(&mut world);
                    assert!(world.bodies[body].is_sleeping());
                    assert!(world.bodies[upper].is_sleeping());
                    assert!(world.bodies[isolated].is_sleeping());
                    world
                        .try_teleport_body(support, pose(10., 0.), reset())
                        .unwrap();
                    assert!(world.bodies[body].is_sleeping()); // Deferred island wake.
                    assert!(world.contact_pair(ca, cb).is_some());
                    let y = world.bodies[body].translation().y;
                    world.step(DT);
                    assert!(!world.bodies[body].is_sleeping());
                    assert!(!world.bodies[upper].is_sleeping());
                    assert!(world.bodies[body].translation().y < y);
                    assert!(world.bodies[isolated].is_sleeping());
                    assert_eq!(world.contact_events(), &[ContactEvent::Stopped(ca, cb)]);
                    world.step(DT);
                    assert!(world.contact_events().is_empty());
                }
            }

            #[test]
            fn moved_obstacle_wakes_new_solid_contact_but_not_sensors() {
                for sensor in [false, true] {
                    let mut world = PhysicsWorld::default();
                    let (obstacle, ca) = world.insert(
                        RigidBodyBuilder::fixed().translation(Vector::X * 10.),
                        ColliderBuilder::ball(1.).sensor(sensor),
                    );
                    let (body, cb) =
                        world.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
                    settle(&mut world);
                    assert!(world.bodies[body].is_sleeping());
                    world
                        .try_teleport_body(obstacle, pose(1., 0.), reset())
                        .unwrap();
                    world.step(DT);
                    assert_eq!(world.bodies[body].is_sleeping(), sensor);
                    assert_eq!(world.contact_events(), &[ContactEvent::Started(ca, cb)]);
                    if !sensor {
                        assert!(world.bodies[body].translation().x < 0.);
                    }
                }
            }

            #[test]
            fn fixed_and_dynamic_joint_edits_wake_enabled_chain_and_retain_handles() {
                for move_fixed in [false, true] {
                    let mut world = PhysicsWorld::default();
                    let anchor = world.insert_body(RigidBodyBuilder::fixed()).unwrap();
                    let a = world.add_ball(point(1., 0.), 0.2, true);
                    let b = world.add_ball(point(2., 0.), 0.2, true);
                    let first = world
                        .insert_joint(anchor, a, JointBuilder::distance(1.))
                        .unwrap();
                    let second = world
                        .insert_joint(a, b, JointBuilder::distance(1.))
                        .unwrap();
                    let isolated = world.add_ball(point(10., 0.), 0.2, true);
                    settle(&mut world);
                    assert!(world.bodies[a].is_sleeping() && world.bodies[b].is_sleeping());
                    let desc = world.joints[first].description();
                    let edited = if move_fixed { anchor } else { a };
                    world
                        .try_teleport_body(
                            edited,
                            pose(if move_fixed { 0.25 } else { 1.25 }, 0.),
                            reset(),
                        )
                        .unwrap();
                    assert!(world.bodies[b].is_sleeping());
                    assert_eq!(world.joints[first].description(), desc);
                    assert_eq!(world.joints[first].bodies(), [anchor, a]);
                    assert_eq!(world.joints[second].bodies(), [a, b]);
                    world.step(DT);
                    assert!(!world.bodies[a].is_sleeping() && !world.bodies[b].is_sleeping());
                    assert!(world.bodies[isolated].is_sleeping());
                    assert!(
                        world.joints.get(first).is_some() && world.joints.get(second).is_some()
                    );
                    assert!(world.last_step.joint_rows > 0);
                    assert!(
                        world.bodies[a].translation().is_finite()
                            && world.bodies[b].translation().is_finite()
                    );
                }
            }

            #[test]
            fn reset_succeeds_at_body_and_child_capacity() {
                let mut world = PhysicsWorld::default();
                let body = world
                    .insert_compound(
                        RigidBodyBuilder::dynamic(),
                        (0..ferravex_physics::MAX_CHILDREN).map(|_| ColliderBuilder::ball(0.1)),
                    )
                    .unwrap();
                let children = world.bodies[body].colliders().to_vec();
                for _ in 1..4096 {
                    world.insert_body(RigidBodyBuilder::fixed()).unwrap();
                }
                assert_eq!(
                    world.insert_body(RigidBodyBuilder::fixed()),
                    Err(PhysicsError::BodyCapacity)
                );
                world
                    .try_teleport_body(body, pose(4., 5.), reset())
                    .unwrap();
                assert_eq!(world.bodies.len(), 4096);
                assert_eq!(world.colliders.len(), ferravex_physics::MAX_CHILDREN);
                assert_eq!(world.bodies[body].colliders(), children);
            }

            #[test]
            fn repeated_valid_and_invalid_teleports_do_not_allocate() {
                let mut world = PhysicsWorld::default();
                let body = world.add_ball(point(0., 0.), 1., true);
                let stale = world.add_ball(point(3., 0.), 1., true);
                world.remove_body(stale);
                let bad = RigidBodyPose {
                    translation: Vector::from_array([f32::NAN; $dim]),
                    ..pose(0., 0.)
                };
                super::ALLOCATIONS.with(|count| count.set(Some(0)));
                let mut successes = 0;
                let mut failures = 0;
                for i in 0..256 {
                    successes += usize::from(
                        world
                            .try_teleport_body(body, pose(i as f32, 0.), reset())
                            .is_ok(),
                    );
                    failures += usize::from(
                        world.try_teleport_body(body, bad, reset())
                            == Err(TeleportError::Translation),
                    );
                    failures += usize::from(
                        world.try_teleport_body(stale, pose(0., 0.), reset())
                            == Err(TeleportError::InvalidHandle),
                    );
                }
                let allocations = super::ALLOCATIONS.with(|count| count.replace(None).unwrap());
                assert_eq!((successes, failures), (256, 512));
                assert_eq!(allocations, 0);
            }

            #[test]
            fn fixed_body_stays_at_committed_pose_and_settings_are_retained() {
                let mut world = PhysicsWorld::new(point(0., -10.));
                world
                    .set_solver_settings(SolverSettings {
                        allow_sleep: false,
                        ..Default::default()
                    })
                    .unwrap();
                let (body, _) = world.insert(
                    RigidBodyBuilder::fixed()
                        .linvel(Vector::X)
                        .angvel($angular(1.)),
                    ColliderBuilder::ball(1.),
                );
                let committed = world
                    .try_teleport_body(
                        body,
                        RigidBodyPose {
                            rotation: $rotation(0.7),
                            ..pose(4., 5.)
                        },
                        reset(),
                    )
                    .unwrap();
                assert_eq!(world.bodies[body].linvel(), Vector::ZERO);
                assert_eq!(world.bodies[body].angvel(), $angular(0.));
                world.step(DT);
                assert_eq!(world.bodies[body].translation(), committed.translation);
                assert_eq!(*world.bodies[body].rotation(), committed.rotation);
                assert!(!world.sleep_state(body).unwrap().dynamic);
                assert!(!world.solver_settings().allow_sleep);
            }
        }
    };
}
#[cfg(feature = "dim2")]
teleport_tests!(d2, 2, ferravex_physics::Rotation2::from_angle, |v| v);
#[cfg(feature = "dim3")]
teleport_tests!(
    d3,
    3,
    ferravex_math::Quat::from_rotation_z,
    |v| ferravex_math::Vec3::Z * v
);

#[cfg(feature = "dim3")]
#[test]
fn spatial_invalid_rotation_rejects_every_change_and_all_finite_scales_normalize() {
    use ferravex_math::{Quat, Vec3};
    use ferravex_physics::d3::*;
    let mut world = PhysicsWorld::default();
    let body = world.add_ball([0.; 3], 1., true);
    world.bodies[body].add_force(Vec3::X, true);
    world.bodies[body].set_next_kinematic_translation(Vec3::Y);
    let before = format!("{:?}", world.bodies[body]);
    let options = TeleportOptions::new(VelocityPolicy::Clear);
    let destination = RigidBodyPose {
        translation: Vec3::X * 4.,
        rotation: Quat::IDENTITY,
    };
    for axis in 0..4 {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut q = [0., 0., 0., 1.];
            q[axis] = invalid;
            assert_eq!(
                world.try_teleport_body(
                    body,
                    RigidBodyPose {
                        rotation: Quat::from_array(q),
                        ..destination
                    },
                    options
                ),
                Err(TeleportError::Rotation)
            );
            assert_eq!(format!("{:?}", world.bodies[body]), before);
        }
    }
    assert_eq!(
        world.try_teleport_body(
            body,
            RigidBodyPose {
                rotation: Quat::from_array([0.; 4]),
                ..destination
            },
            options
        ),
        Err(TeleportError::Rotation)
    );
    assert_eq!(format!("{:?}", world.bodies[body]), before);
    let both_bad = RigidBodyPose {
        translation: Vec3::NAN,
        rotation: Quat::NAN,
    };
    assert_eq!(
        world.try_teleport_body(body, both_bad, options),
        Err(TeleportError::Translation)
    );
    for scale in [2., -2., f32::MAX, f32::from_bits(1)] {
        let result = world
            .try_teleport_body(
                body,
                RigidBodyPose {
                    rotation: Quat::from_xyzw(scale, scale, scale, scale),
                    ..destination
                },
                options,
            )
            .unwrap();
        assert!(result.rotation.is_normalized());
        assert_eq!(result.rotation, *world.bodies[body].rotation());
        assert!((result.rotation * Vec3::X - Vec3::Y).length() < 0.00001);
    }
}
