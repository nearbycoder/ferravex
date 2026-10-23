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
// SAFETY: Forward System's allocation contract unchanged. TLS counters do not
// allocate and tolerate TLS destruction; parallel test threads are independent.
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

macro_rules! authoring_tests {
    ($module:ident, $dim:literal, $angular:expr, $rotation:expr, $quat:expr) => {
        mod $module {
            use ferravex_math::{Quat, Vec3};
            use ferravex_physics::$module::{
                ColliderBuilder, JointBuilder, JointPoseMode, PhysicsWorld, RigidBodyBuilder,
                RigidBodyPose, TeleportOptions, Vector, VelocityPolicy, WorldJointFrame,
            };
            use ferravex_physics::{
                JointDesc, JointKind, JointMotor, MAX_JOINTS, PhysicsError, RigidBodyHandle,
            };
            const DT: f32 = 1. / 120.;
            fn v(x: f32, y: f32) -> Vector {
                let mut values = [0.; $dim];
                values[0] = x;
                values[1] = y;
                Vector::from_array(values)
            }
            fn to3(v: Vector) -> Vec3 {
                let mut values = [0.; 3];
                values[..$dim].copy_from_slice(&v.to_array());
                Vec3::from_array(values)
            }
            fn q(world: &PhysicsWorld, id: RigidBodyHandle) -> Quat {
                $quat(*world.bodies[id].rotation())
            }
            fn close(a: Vec3, b: Vec3) {
                assert!(a.distance(b) < 0.0001, "{a:?} != {b:?}");
            }
            fn pair() -> (PhysicsWorld, RigidBodyHandle, RigidBodyHandle) {
                let mut world = PhysicsWorld::default();
                let a = world
                    .insert_body(
                        RigidBodyBuilder::fixed()
                            .translation(v(2., 3.))
                            .rotation($angular(0.4)),
                    )
                    .unwrap();
                let (b, _) = world.insert(
                    RigidBodyBuilder::dynamic()
                        .translation(v(4., 5.))
                        .rotation($angular(-0.8)),
                    ColliderBuilder::ball(0.3).translation(v(0.35, -0.2)),
                );
                (world, a, b)
            }
            fn frame() -> WorldJointFrame {
                WorldJointFrame::at_pivot(v(3., 4.))
            }
            fn local_to_world(world: &PhysicsWorld, id: RigidBodyHandle, anchor: Vec3) -> Vec3 {
                to3(world.bodies[id].translation()) + q(world, id) * anchor
            }

            #[test]
            fn rotated_translated_pairings_hold_pose_on_the_immediate_next_tick() {
                for kinds in [[0, 1], [1, 0], [1, 1], [2, 1], [1, 2]] {
                    for hinge in [false, true] {
                        let mut world = PhysicsWorld::default();
                        let ids = kinds.map(|kind| {
                            let builder = match kind {
                                0 => RigidBodyBuilder::fixed(),
                                1 => RigidBodyBuilder::dynamic(),
                                _ => RigidBodyBuilder::kinematic_position_based(),
                            };
                            let index = world.bodies.len() as f32;
                            world
                                .insert(
                                    builder
                                        .translation(v(2. + index * 2., 3. + index))
                                        .rotation($angular(0.4 - index * 1.2)),
                                    ColliderBuilder::ball(0.3).translation(v(0.35, -0.2)),
                                )
                                .0
                        });
                        let [a, b] = ids;
                        let positions = ids.map(|id| to3(world.bodies[id].translation()));
                        let rotations = ids.map(|id| q(&world, id));
                        let desc = world
                            .joint_desc_from_world(
                                a,
                                b,
                                if hinge {
                                    JointBuilder::hinge()
                                } else {
                                    JointBuilder::fixed()
                                },
                                frame(),
                                JointPoseMode::Preserve,
                            )
                            .unwrap();
                        close(
                            local_to_world(&world, a, desc.local_anchor1),
                            to3(frame().anchors[0]),
                        );
                        close(
                            local_to_world(&world, b, desc.local_anchor2),
                            to3(frame().anchors[1]),
                        );
                        assert!(
                            desc.reference_rotation
                                .unwrap()
                                .dot(rotations[0].conjugate() * rotations[1])
                                .abs()
                                > 0.99999
                        );
                        if let JointKind::Hinge { axis1, axis2 } = desc.kind {
                            close(rotations[0] * axis1, frame().axis);
                            close(rotations[1] * axis2, frame().axis);
                        }
                        let joint = world.insert_joint(a, b, desc).unwrap();
                        if hinge {
                            assert!(world.joint_angle(joint).unwrap().abs() < 0.00001);
                        }
                        world.step(DT);
                        for (i, id) in ids.into_iter().enumerate() {
                            close(to3(world.bodies[id].translation()), positions[i]);
                            assert!(q(&world, id).dot(rotations[i]).abs() > 0.99999);
                            assert!(world.bodies[id].linvel().length() < 0.0001);
                        }
                        let anchors = world.joint_anchors(joint).unwrap();
                        close(anchors[0], anchors[1]);
                    }
                }
            }

            #[test]
            fn world_frames_see_teleports_and_shape_edits_before_any_step() {
                let (mut world, a, b) = pair();
                let child = world.bodies[b].colliders()[0];
                let stale_snapshot = world
                    .joint_desc_from_world(
                        a,
                        b,
                        JointBuilder::fixed(),
                        frame(),
                        JointPoseMode::Preserve,
                    )
                    .unwrap();
                for (id, translation, angle) in [(a, v(-4., 6.), 0.9), (b, v(2., -3.), -1.4)] {
                    world
                        .try_teleport_body(
                            id,
                            RigidBodyPose {
                                translation,
                                rotation: $rotation(angle),
                            },
                            TeleportOptions::new(VelocityPolicy::Clear),
                        )
                        .unwrap();
                }
                world
                    .replace_collider_shape(child, ColliderBuilder::ball(0.7))
                    .unwrap();
                let mass = world.bodies[b].mass();
                let positions = [a, b].map(|id| to3(world.bodies[id].translation()));
                let rotations = [a, b].map(|id| q(&world, id));
                let joint = world
                    .insert_joint_from_world(
                        a,
                        b,
                        JointBuilder::fixed(),
                        frame(),
                        JointPoseMode::Preserve,
                    )
                    .unwrap();
                let desc = world.joints[joint].description();
                assert_ne!(desc.local_anchor1, stale_snapshot.local_anchor1);
                assert_ne!(desc.local_anchor2, stale_snapshot.local_anchor2);
                for anchor in world.joint_anchors(joint).unwrap() {
                    close(anchor, to3(frame().anchors[0]));
                }
                world.step(DT);
                for (i, id) in [a, b].into_iter().enumerate() {
                    close(to3(world.bodies[id].translation()), positions[i]);
                    assert!(q(&world, id).dot(rotations[i]).abs() > 0.99999);
                }
                assert_eq!(world.bodies[b].colliders(), &[child]);
                assert_eq!(world.bodies[b].mass(), mass);
                assert_eq!(world.joints[joint].description(), desc);
            }

            #[test]
            fn separated_world_anchors_capture_rest_lengths_without_prestress() {
                for spring in [false, true] {
                    let (mut world, a, b) = pair();
                    let anchors = WorldJointFrame::at_anchors([v(2., 3.), v(4., 5.)]);
                    let mut builder = if spring {
                        JointBuilder::spring(0.1, 20., 2.)
                    } else {
                        JointBuilder::distance(0.1)
                    };
                    builder = builder.reference_rotation($rotation(1.2));
                    let desc = world
                        .joint_desc_from_world(
                            a,
                            b,
                            builder,
                            anchors.with_axis(Vec3::NAN),
                            JointPoseMode::Preserve,
                        )
                        .unwrap();
                    let expected = to3(anchors.anchors[0]).distance(to3(anchors.anchors[1]));
                    assert_eq!(
                        desc.kind,
                        if spring {
                            JointKind::Spring {
                                length: expected,
                                stiffness: 20.,
                                damping: 2.,
                            }
                        } else {
                            JointKind::Distance { length: expected }
                        }
                    );
                    let before = world.bodies[b].translation();
                    let rotation = q(&world, b);
                    world.insert_joint(a, b, desc).unwrap();
                    world.step(DT);
                    close(to3(world.bodies[b].translation()), to3(before));
                    assert!(q(&world, b).dot(rotation).abs() > 0.99999);
                    assert!(world.bodies[b].linvel().length() < 0.0001);
                }
            }

            #[test]
            fn authored_mode_preserves_reference_rest_length_and_all_controls() {
                let (mut world, a, b) = pair();
                let builder = JointBuilder::hinge()
                    .reference_rotation($rotation(0.3))
                    .limits(-0.5, 0.6)
                    .motor(JointMotor::velocity(2., 5.))
                    .break_limits(100., 200.)
                    .contacts_enabled(true);
                let original = builder.build();
                for mode in [JointPoseMode::Preserve, JointPoseMode::Authored] {
                    let desc = world
                        .joint_desc_from_world(a, b, builder, frame(), mode)
                        .unwrap();
                    assert_eq!(desc.limits, original.limits);
                    assert_eq!(desc.motor, original.motor);
                    assert_eq!(desc.break_limits, original.break_limits);
                    assert_eq!(desc.contacts_enabled, original.contacts_enabled);
                    assert_eq!(desc.enabled, original.enabled);
                    if mode == JointPoseMode::Authored {
                        assert_eq!(desc.reference_rotation, original.reference_rotation);
                    }
                }
                for builder in [
                    JointBuilder::distance(0.2),
                    JointBuilder::spring(0.2, 10., 3.),
                ] {
                    let desc = world
                        .joint_desc_from_world(
                            a,
                            b,
                            builder,
                            WorldJointFrame::at_anchors([v(2., 3.), v(4., 5.)]),
                            JointPoseMode::Authored,
                        )
                        .unwrap();
                    assert_eq!(desc.kind, builder.build().kind);
                    assert!(
                        desc.reference_rotation
                            .unwrap()
                            .dot(q(&world, a).conjugate() * q(&world, b))
                            .abs()
                            > 0.99999
                    );
                }
                // Capture geometric zero but retain an authored motor: the
                // next tick still executes that control through the real solver.
                let joint = world
                    .insert_joint_from_world(
                        a,
                        b,
                        JointBuilder::hinge().motor(JointMotor::velocity(1., 100.)),
                        frame(),
                        JointPoseMode::Preserve,
                    )
                    .unwrap();
                assert!(world.joint_angle(joint).unwrap().abs() < 0.00001);
                world.step(DT);
                assert!(world.bodies[b].angvel().abs_or_length() > 0.01);
            }

            #[test]
            fn invalid_inputs_are_atomic_and_use_live_generational_handles() {
                let (mut world, a, b) = pair();
                for _ in 0..120 {
                    world.step(DT);
                }
                assert!(world.bodies[b].is_sleeping());
                let before = format!("{:?}", world.bodies[b]);
                let sleep = world.sleep_state(b);
                let events = world.contact_events().to_vec();
                let invalid_builders = [
                    JointBuilder::hinge().limits(1., -1.),
                    JointBuilder::hinge().motor(JointMotor::velocity(f32::NAN, 1.)),
                    JointBuilder::fixed().limits(0., 1.),
                    JointBuilder::spring(0., 0., 1.),
                    JointBuilder::distance(-1.),
                ];
                for builder in invalid_builders {
                    assert_eq!(
                        world.insert_joint_from_world(
                            a,
                            b,
                            builder,
                            frame(),
                            JointPoseMode::Authored
                        ),
                        Err(PhysicsError::InvalidJoint)
                    );
                }
                for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
                    for axis in 0..$dim {
                        let mut pivot = v(3., 4.).to_array();
                        pivot[axis] = bad;
                        assert_eq!(
                            world.insert_joint_from_world(
                                a,
                                b,
                                JointBuilder::fixed(),
                                WorldJointFrame::at_pivot(Vector::from_array(pivot)),
                                JointPoseMode::Preserve
                            ),
                            Err(PhysicsError::InvalidJoint)
                        );
                    }
                }
                assert_eq!(
                    world.insert_joint_from_world(
                        a,
                        a,
                        JointBuilder::fixed(),
                        frame(),
                        JointPoseMode::Preserve
                    ),
                    Err(PhysicsError::InvalidJoint)
                );
                assert_eq!(format!("{:?}", world.bodies[b]), before);
                assert_eq!(world.sleep_state(b), sleep);
                assert_eq!(world.contact_events(), events);
                assert!(world.joints.is_empty());
                assert!(world.remove_body(b));
                let replacement = world
                    .insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(0.3))
                    .0;
                assert_ne!(replacement, b);
                for ids in [[a, b], [b, a]] {
                    assert_eq!(
                        world.insert_joint_from_world(
                            ids[0],
                            ids[1],
                            JointBuilder::fixed(),
                            WorldJointFrame::at_pivot(Vector::NAN),
                            JointPoseMode::Preserve
                        ),
                        Err(PhysicsError::InvalidHandle)
                    );
                }
                let fixed = world.insert_body(RigidBodyBuilder::fixed()).unwrap();
                let kinematic = world
                    .insert_body(RigidBodyBuilder::kinematic_position_based())
                    .unwrap();
                for ids in [[a, fixed], [a, kinematic], [kinematic, fixed]] {
                    assert_eq!(
                        world.joint_desc_from_world(
                            ids[0],
                            ids[1],
                            JointBuilder::fixed(),
                            frame(),
                            JointPoseMode::Preserve
                        ),
                        Err(PhysicsError::InvalidJoint)
                    );
                }
            }

            #[test]
            fn separated_pivots_reject_preservation_but_allow_authored_correction() {
                let (mut world, a, b) = pair();
                let anchors = WorldJointFrame::at_anchors([v(2., 3.), v(4., 5.)]);
                for builder in [JointBuilder::fixed(), JointBuilder::hinge()] {
                    assert_eq!(
                        world.insert_joint_from_world(
                            a,
                            b,
                            builder,
                            anchors,
                            JointPoseMode::Preserve
                        ),
                        Err(PhysicsError::InvalidJoint)
                    );
                    let desc = world
                        .joint_desc_from_world(a, b, builder, anchors, JointPoseMode::Authored)
                        .unwrap();
                    assert_ne!(
                        local_to_world(&world, a, desc.local_anchor1),
                        local_to_world(&world, b, desc.local_anchor2)
                    );
                }
                assert!(world.joints.is_empty());
                let before = world.bodies[b].translation();
                world
                    .insert_joint_from_world(
                        a,
                        b,
                        JointBuilder::fixed(),
                        anchors,
                        JointPoseMode::Authored,
                    )
                    .unwrap();
                world.step(DT);
                assert!(world.bodies[b].translation().distance(before) > 0.01);
            }

            #[test]
            fn hinge_axis_normalization_checks_zero_nonfinite_and_extreme_scales() {
                let (mut world, a, b) = pair();
                for axis in [
                    Vec3::ZERO,
                    Vec3::NAN,
                    Vec3::INFINITY,
                    Vec3::new(1., f32::NEG_INFINITY, 0.),
                ] {
                    assert_eq!(
                        world.insert_joint_from_world(
                            a,
                            b,
                            JointBuilder::hinge(),
                            frame().with_axis(axis),
                            JointPoseMode::Preserve
                        ),
                        Err(PhysicsError::InvalidJoint)
                    );
                }
                for scale in [f32::from_bits(1), 1e-30, 1., f32::MAX] {
                    let desc = world
                        .joint_desc_from_world(
                            a,
                            b,
                            JointBuilder::hinge(),
                            frame().with_axis(frame().axis * scale),
                            JointPoseMode::Preserve,
                        )
                        .unwrap();
                    let JointKind::Hinge { axis1, axis2 } = desc.kind else {
                        panic!("hinge")
                    };
                    assert!(axis1.is_normalized() && axis2.is_normalized());
                    close(q(&world, a) * axis1, frame().axis);
                    close(q(&world, b) * axis2, frame().axis);
                }
                if $dim == 2 {
                    for axis in [Vec3::X, Vec3::Y, -Vec3::Z, Vec3::new(1., 0., 1.)] {
                        assert_eq!(
                            world.joint_desc_from_world(
                                a,
                                b,
                                JointBuilder::hinge(),
                                frame().with_axis(axis),
                                JointPoseMode::Preserve
                            ),
                            Err(PhysicsError::InvalidJoint)
                        );
                    }
                }
                assert!(world.joints.is_empty());
            }

            #[test]
            fn helper_replaces_only_frame_and_selected_rest_fields_then_validates() {
                let (world, a, b) = pair();
                let mut desc = JointDesc::new(JointKind::Hinge {
                    axis1: Vec3::NAN,
                    axis2: Vec3::ZERO,
                });
                desc.local_anchor1 = Vec3::NAN;
                desc.local_anchor2 = Vec3::INFINITY;
                desc.reference_rotation = Some(Quat::NAN);
                let valid = world
                    .joint_desc_from_world(a, b, desc, frame(), JointPoseMode::Preserve)
                    .unwrap();
                valid.validate($dim == 2).unwrap();
                assert_eq!(
                    world.joint_desc_from_world(a, b, desc, frame(), JointPoseMode::Authored),
                    Err(PhysicsError::InvalidJoint)
                );
                for bad in [-1., f32::NAN, f32::INFINITY, 1_000_001.] {
                    let result = world
                        .joint_desc_from_world(
                            a,
                            b,
                            JointBuilder::distance(bad),
                            frame(),
                            JointPoseMode::Preserve,
                        )
                        .unwrap();
                    assert_eq!(result.kind, JointKind::Distance { length: 0. });
                }
                assert_eq!(
                    world.joint_desc_from_world(
                        a,
                        b,
                        JointBuilder::distance(0.),
                        WorldJointFrame::at_anchors([v(-600_000., 0.), v(600_000., 0.)]),
                        JointPoseMode::Preserve
                    ),
                    Err(PhysicsError::InvalidJoint)
                );
                assert_eq!(
                    world.joint_desc_from_world(
                        a,
                        b,
                        JointBuilder::fixed(),
                        WorldJointFrame::at_pivot(v(2_000_000., 0.)),
                        JointPoseMode::Preserve
                    ),
                    Err(PhysicsError::InvalidJoint)
                );
            }

            #[test]
            fn full_joint_capacity_is_checked_only_by_insertion_and_recovers_after_remove() {
                let (mut world, a, b) = pair();
                for _ in 0..MAX_JOINTS {
                    world
                        .insert_joint_from_world(
                            a,
                            b,
                            JointBuilder::fixed(),
                            frame(),
                            JointPoseMode::Preserve,
                        )
                        .unwrap();
                }
                for _ in 0..120 {
                    world.step(DT);
                }
                assert!(world.bodies[b].is_sleeping());
                let sleep = world.sleep_state(b);
                let before = format!("{:?}", world.bodies[b]);
                let first = world.joints.iter().next().unwrap().0;
                let desc = world
                    .joint_desc_from_world(
                        a,
                        b,
                        JointBuilder::fixed(),
                        frame(),
                        JointPoseMode::Preserve,
                    )
                    .unwrap();
                assert_eq!(
                    world.insert_joint_from_world(a, b, desc, frame(), JointPoseMode::Preserve),
                    Err(PhysicsError::JointCapacity)
                );
                assert_eq!(world.sleep_state(b), sleep);
                assert_eq!(format!("{:?}", world.bodies[b]), before);
                assert_eq!(world.joints.len(), MAX_JOINTS);
                assert!(world.remove_joint(first));
                let next = world
                    .insert_joint_from_world(a, b, desc, frame(), JointPoseMode::Preserve)
                    .unwrap();
                assert_ne!(first, next);
                assert!(world.joints.get(first).is_none());
                assert_eq!(world.joints.len(), MAX_JOINTS);
                assert!(!world.bodies[b].is_sleeping());
                assert!(world.remove_body(b));
                assert!(world.joints.is_empty());
            }

            #[test]
            fn sampled_frames_and_failures_allocate_nothing_or_change_motion() {
                let (mut world, a, b) = pair();
                for _ in 0..120 {
                    world.step(DT);
                }
                let before = format!("{:?}", world.bodies[b]);
                let events = world.contact_events().to_vec();
                let step = format!("{:?}", world.last_step);
                let stale = world.insert_body(RigidBodyBuilder::fixed()).unwrap();
                world.remove_body(stale);
                super::ALLOCATIONS.with(|count| count.set(Some(0)));
                for _ in 0..256 {
                    std::hint::black_box(
                        world
                            .joint_desc_from_world(
                                a,
                                b,
                                JointBuilder::hinge(),
                                frame(),
                                JointPoseMode::Preserve,
                            )
                            .unwrap(),
                    );
                    assert_eq!(
                        world.insert_joint_from_world(
                            a,
                            b,
                            JointBuilder::hinge(),
                            frame().with_axis(Vec3::ZERO),
                            JointPoseMode::Preserve
                        ),
                        Err(PhysicsError::InvalidJoint)
                    );
                    assert_eq!(
                        world.insert_joint_from_world(
                            a,
                            stale,
                            JointBuilder::fixed(),
                            frame(),
                            JointPoseMode::Preserve
                        ),
                        Err(PhysicsError::InvalidHandle)
                    );
                }
                let count = super::ALLOCATIONS.with(|count| count.replace(None).unwrap());
                assert_eq!(count, 0);
                assert_eq!(format!("{:?}", world.bodies[b]), before);
                assert_eq!(world.contact_events(), events);
                assert_eq!(format!("{:?}", world.last_step), step);
                assert!(world.joints.is_empty());
            }

            // Let dimension-generic motor assertions inspect scalar/Vec3 speed.
            trait AngularMagnitude {
                fn abs_or_length(self) -> f32;
            }
            impl AngularMagnitude for f32 {
                fn abs_or_length(self) -> f32 {
                    self.abs()
                }
            }
            impl AngularMagnitude for Vec3 {
                fn abs_or_length(self) -> f32 {
                    self.length()
                }
            }
        }
    };
}

#[cfg(feature = "dim2")]
authoring_tests!(
    d2,
    2,
    |a| a,
    ferravex_physics::Rotation2::from_angle,
    |r: ferravex_physics::Rotation2| Quat::from_rotation_z(r.angle())
);
#[cfg(feature = "dim3")]
authoring_tests!(
    d3,
    3,
    |a| Vec3::new(a * 0.3, a * -0.6, a),
    |a| Quat::from_scaled_axis(Vec3::new(a * 0.3, a * -0.6, a)),
    |r| r
);

#[cfg(feature = "dim3")]
#[test]
fn arbitrary_spatial_axis_is_local_to_each_rotated_parent_and_rotates_freely() {
    use ferravex_math::Vec3;
    use ferravex_physics::{JointKind, JointMotor, d3::*};
    let mut world = PhysicsWorld::default();
    let a = world
        .insert_body(
            RigidBodyBuilder::fixed()
                .translation(Vec3::new(1., 2., 3.))
                .rotation(Vec3::new(0.4, -0.5, 0.8)),
        )
        .unwrap();
    let (b, _) = world.insert(
        RigidBodyBuilder::dynamic()
            .translation(Vec3::new(-2., 3., 5.))
            .rotation(Vec3::new(-0.8, 0.2, 1.2)),
        ColliderBuilder::ball(0.2),
    );
    let axis = Vec3::new(-2., 3., 4.).normalize();
    let pivot = world.bodies[b].translation();
    let joint = world
        .insert_joint_from_world(
            a,
            b,
            JointBuilder::hinge().motor(JointMotor::velocity(1., 1000.)),
            WorldJointFrame::at_pivot(pivot).with_axis(axis * f32::MAX),
            JointPoseMode::Preserve,
        )
        .unwrap();
    let JointKind::Hinge { axis1, axis2 } = world.joints[joint].description().kind else {
        panic!("hinge")
    };
    assert!((*world.bodies[a].rotation() * axis1).distance(axis) < 0.0001);
    assert!((*world.bodies[b].rotation() * axis2).distance(axis) < 0.0001);
    let initial = *world.bodies[b].rotation();
    world.step(1. / 120.);
    assert!(world.bodies[b].angvel().dot(axis) > 0.01);
    for _ in 0..60 {
        world.step(1. / 120.);
    }
    assert!(
        (*world.bodies[b].rotation() * axis2).distance(axis) < 0.002,
        "axis drift {:?}",
        *world.bodies[b].rotation() * axis2 - axis
    );
    assert!(initial.angle_between(*world.bodies[b].rotation()) > 0.1);
    assert!(world.joint_angle(joint).unwrap().abs() > 0.1);
    let anchors = world.joint_anchors(joint).unwrap();
    assert!(anchors[0].distance(anchors[1]) < 0.002);
}
