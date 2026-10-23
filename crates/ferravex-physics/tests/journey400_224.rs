use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAllocator;
thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
fn allocation() {
    let _ = ALLOCATIONS.try_with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}
// SAFETY: System's allocation contract is forwarded unchanged. Thread-local
// counters do not allocate, and tolerate thread-local destruction.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

macro_rules! assembly_tests {
    ($dim:ident, $n:literal, $angular:expr) => {
        mod $dim {
            use ferravex_physics::assembly::{
                Assembly, AssemblyError, AssemblyHandles, LocalBodyId, LocalJointId,
                MAX_ASSEMBLY_BODIES,
            };
            use ferravex_physics::$dim::{
                ColliderBuilder, JointBuilder, PhysicsWorld, RigidBody, RigidBodyBuilder, Vector,
            };
            use ferravex_physics::{
                Collider, JointDesc, JointKind, JointMotor, MAX_CHILDREN, MAX_COLLIDERS,
                MAX_JOINTS, PhysicsError,
            };
            fn v(x: f32, y: f32) -> Vector {
                let mut a = [0.; $n];
                a[0] = x;
                a[1] = y;
                a.into()
            }
            fn mechanism() -> Assembly<RigidBody> {
                let mut plan = Assembly::new();
                let a = plan
                    .add_body(RigidBodyBuilder::fixed(), [] as [Collider; 0])
                    .unwrap();
                let b = plan
                    .add_body(
                        RigidBodyBuilder::dynamic(),
                        [
                            ColliderBuilder::ball(0.2).translation(v(0.5, 0.)),
                            ColliderBuilder::ball(0.3).translation(v(-0.5, 0.)),
                        ],
                    )
                    .unwrap();
                plan.add_joint(a, b, JointBuilder::hinge()).unwrap();
                plan
            }
            fn snapshot(w: &PhysicsWorld) -> String {
                format!(
                    "{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
                    w.bodies,
                    w.colliders,
                    w.joints,
                    w.contact_events(),
                    w.joint_break_events(),
                    w.last_step,
                    w.sleep_stats()
                )
            }
            fn reject(w: &mut PhysicsWorld, plan: Assembly<RigidBody>, error: AssemblyError) {
                let before = snapshot(w);
                assert_eq!(w.validate_assembly(&plan), Err(error));
                assert_eq!(w.insert_assembly(plan), Err(error));
                assert_eq!(snapshot(w), before);
            }
            #[test]
            fn late_invalid_joint_and_all_bad_local_references_are_atomic() {
                let mut w = PhysicsWorld::default();
                let (live, collider) =
                    w.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
                let (hole, hole_collider) =
                    w.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
                w.step(1. / 120.);
                w.remove_body(hole);
                for (a, b, desc, cause) in [
                    (
                        0,
                        1,
                        JointBuilder::distance(-1.).build(),
                        PhysicsError::InvalidJoint,
                    ),
                    (
                        1,
                        1,
                        JointBuilder::hinge().build(),
                        PhysicsError::InvalidJoint,
                    ),
                    (
                        2,
                        1,
                        JointBuilder::hinge().build(),
                        PhysicsError::InvalidHandle,
                    ),
                    (
                        0,
                        usize::MAX,
                        JointBuilder::hinge().build(),
                        PhysicsError::InvalidHandle,
                    ),
                    (
                        usize::MAX,
                        usize::MAX,
                        JointBuilder::hinge().build(),
                        PhysicsError::InvalidHandle,
                    ),
                ] {
                    let mut plan = mechanism();
                    let joint = plan
                        .add_joint(LocalBodyId::from_index(a), LocalBodyId::from_index(b), desc)
                        .unwrap();
                    reject(
                        &mut w,
                        plan,
                        AssemblyError::Joint {
                            joint,
                            error: cause,
                        },
                    );
                    assert!(w.bodies.get(live).is_some());
                    assert!(w.colliders.get(collider).is_some());
                }
                let (replacement, replacement_collider) =
                    w.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
                // Failure must not burn generations in holes, even temporarily.
                let mut control = PhysicsWorld::default();
                control.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
                let (removed, _) =
                    control.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
                control.remove_body(removed);
                let expected =
                    control.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
                assert_eq!((replacement, replacement_collider), expected);
                assert_ne!(replacement, hole);
                assert_ne!(replacement_collider, hole_collider);
            }
            #[test]
            fn late_zero_underflow_and_overflow_mass_fail_before_any_insertion() {
                let mut w = PhysicsWorld::default();
                for children in [
                    Vec::<Collider>::new(),
                    vec![ColliderBuilder::ball(1.).density(0.).into()],
                    vec![ColliderBuilder::try_ball(f32::MIN_POSITIVE).unwrap().into()],
                    vec![ColliderBuilder::try_ball(f32::MAX).unwrap().into()],
                ] {
                    let mut plan = mechanism();
                    let bad = plan
                        .add_body(RigidBodyBuilder::dynamic(), children)
                        .unwrap();
                    reject(
                        &mut w,
                        plan,
                        AssemblyError::Body {
                            body: bad,
                            error: PhysicsError::InvalidMass,
                        },
                    );
                }
            }
            #[test]
            fn no_dynamic_endpoint_is_rejected_including_disabled_joints() {
                for kind in [
                    RigidBodyBuilder::fixed(),
                    RigidBodyBuilder::kinematic_position_based(),
                ] {
                    let mut w = PhysicsWorld::default();
                    let mut plan = Assembly::new();
                    let a = plan
                        .add_body(RigidBodyBuilder::fixed(), [] as [Collider; 0])
                        .unwrap();
                    let b = plan.add_body(kind, [] as [Collider; 0]).unwrap();
                    let mut desc = JointDesc::new(JointKind::Fixed);
                    desc.enabled = false;
                    let joint = plan.add_joint(a, b, desc).unwrap();
                    reject(
                        &mut w,
                        plan,
                        AssemblyError::Joint {
                            joint,
                            error: PhysicsError::InvalidJoint,
                        },
                    );
                }
            }
            #[test]
            fn deterministic_maps_match_manual_insertion_and_reset_generations() {
                let mut w = PhysicsWorld::default();
                let mut control = PhysicsWorld::default();
                let plan = mechanism();
                let body_ids = [LocalBodyId::from_index(0), LocalBodyId::from_index(1)];
                let joint_id = LocalJointId::from_index(0);
                assert_eq!(body_ids[1].index(), 1);
                assert_eq!(joint_id.index(), 0);
                let a = control.insert_body(RigidBodyBuilder::fixed()).unwrap();
                let b = control
                    .insert_compound(
                        RigidBodyBuilder::dynamic(),
                        [
                            ColliderBuilder::ball(0.2).translation(v(0.5, 0.)),
                            ColliderBuilder::ball(0.3).translation(v(-0.5, 0.)),
                        ],
                    )
                    .unwrap();
                let joint = control.insert_joint(a, b, JointBuilder::hinge()).unwrap();
                let handles = w.insert_assembly(plan.clone()).unwrap();
                assert_eq!(handles.bodies(), &[a, b]);
                assert_eq!(handles.joints(), &[joint]);
                assert_eq!(handles.colliders(body_ids[0]), Some(&[][..]));
                assert_eq!(
                    handles.colliders(body_ids[1]),
                    Some(control.bodies[b].colliders())
                );
                assert_eq!(
                    handles.collider(body_ids[1], 1),
                    Some(control.bodies[b].colliders()[1])
                );
                assert_eq!(handles.body(LocalBodyId::from_index(2)), None);
                assert_eq!(handles.collider(body_ids[0], 0), None);
                assert_eq!(handles.collider(body_ids[1], usize::MAX), None);
                assert_eq!(handles.colliders(LocalBodyId::from_index(usize::MAX)), None);
                assert_eq!(handles.joint(LocalJointId::from_index(1)), None);
                assert_eq!(snapshot(&w), snapshot(&control));
                for _ in 0..60 {
                    w.step(1. / 120.);
                    control.step(1. / 120.);
                }
                assert_eq!(snapshot(&w), snapshot(&control));
                for &id in handles.bodies() {
                    assert!(w.remove_body(id));
                }
                assert_eq!(
                    (w.bodies.len(), w.colliders.len(), w.joints.len()),
                    (0, 0, 0)
                );
                let reset = w.insert_assembly(plan).unwrap();
                assert_ne!(reset.body(body_ids[0]), handles.body(body_ids[0]));
                assert_ne!(reset.joint(joint_id), handles.joint(joint_id));
                for &id in handles.bodies() {
                    assert!(w.bodies.get(id).is_none());
                }
                assert_eq!(reset.colliders(body_ids[1]).unwrap().len(), 2);
            }
            #[test]
            fn forward_reference_reference_capture_and_settings_use_existing_joint_semantics() {
                let mut w = PhysicsWorld::default();
                let mut plan = Assembly::new();
                let mut desc = JointBuilder::hinge()
                    .contacts_enabled(true)
                    .limits(-0.4, 0.4)
                    .motor(JointMotor::velocity(1., 2.))
                    .break_limits(10., 20.)
                    .build();
                desc.enabled = false;
                let joint = plan
                    .add_joint(LocalBodyId::from_index(0), LocalBodyId::from_index(1), desc)
                    .unwrap();
                let a = plan
                    .add_body(
                        RigidBodyBuilder::kinematic_position_based().rotation($angular(0.3)),
                        [] as [Collider; 0],
                    )
                    .unwrap();
                let b = plan
                    .add_body(
                        RigidBodyBuilder::dynamic().rotation($angular(-0.8)),
                        [ColliderBuilder::ball(1.).sensor(true)],
                    )
                    .unwrap();
                let maps = w.insert_assembly(plan).unwrap();
                let stored = w.joints[maps.joint(joint).unwrap()].description();
                assert!(stored.reference_rotation.is_some());
                desc.reference_rotation = stored.reference_rotation;
                assert_eq!(stored, desc);
                assert!(w.joint_angle(maps.joint(joint).unwrap()).unwrap().abs() < 1e-6);
                assert_eq!(
                    w.joints[maps.joint(joint).unwrap()].bodies(),
                    [maps.body(a).unwrap(), maps.body(b).unwrap()]
                );
                assert!(w.bodies[maps.body(b).unwrap()].mass() > 0.);
            }
            #[test]
            fn body_capacity_is_checked_as_a_batch_and_recovers_after_removal() {
                let mut w = PhysicsWorld::default();
                let mut last = None;
                for _ in 0..MAX_ASSEMBLY_BODIES - 1 {
                    last = Some(w.insert_body(RigidBodyBuilder::fixed()).unwrap());
                }
                reject(
                    &mut w,
                    mechanism(),
                    AssemblyError::Capacity(PhysicsError::BodyCapacity),
                );
                assert_eq!(
                    w.insert_assembly(Assembly::new()).unwrap(),
                    AssemblyHandles::default()
                );
                assert!(w.remove_body(last.unwrap()));
                let handles = w.insert_assembly(mechanism()).unwrap();
                assert_eq!(handles.bodies().len(), 2);
                assert_eq!(w.bodies.len(), MAX_ASSEMBLY_BODIES);
            }
            #[test]
            fn collider_capacity_is_checked_as_a_batch_and_recovers_after_removal() {
                let mut w = PhysicsWorld::default();
                let mut last = None;
                for _ in 0..MAX_COLLIDERS / MAX_CHILDREN {
                    last = Some(
                        w.insert_compound(
                            RigidBodyBuilder::fixed(),
                            (0..MAX_CHILDREN).map(|_| ColliderBuilder::ball(1.)),
                        )
                        .unwrap(),
                    );
                }
                // Leave one slot free: each child fits alone, the assembly does not.
                let last = last.unwrap();
                let child = w.bodies[last].colliders()[0];
                w.remove_collider(child).unwrap();
                reject(
                    &mut w,
                    mechanism(),
                    AssemblyError::Capacity(PhysicsError::ColliderCapacity),
                );
                let child = w.bodies[last].colliders()[0];
                w.remove_collider(child).unwrap();
                let maps = w.insert_assembly(mechanism()).unwrap();
                assert_eq!(maps.colliders(LocalBodyId::from_index(1)).unwrap().len(), 2);
                assert_eq!(w.colliders.len(), MAX_COLLIDERS);
            }
            #[test]
            fn joint_capacity_is_checked_as_a_batch_and_recovers_after_removal() {
                let mut w = PhysicsWorld::default();
                let a = w.insert_body(RigidBodyBuilder::fixed()).unwrap();
                let (b, _) = w.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
                let mut last = None;
                for _ in 0..MAX_JOINTS - 1 {
                    last = Some(w.insert_joint(a, b, JointBuilder::hinge()).unwrap());
                }
                let mut plan = mechanism();
                plan.add_joint(
                    LocalBodyId::from_index(0),
                    LocalBodyId::from_index(1),
                    JointBuilder::distance(0.),
                )
                .unwrap();
                reject(
                    &mut w,
                    plan.clone(),
                    AssemblyError::Capacity(PhysicsError::JointCapacity),
                );
                w.remove_joint(last.unwrap());
                let maps = w.insert_assembly(plan).unwrap();
                assert_eq!(maps.joints().len(), 2);
                assert_eq!(w.joints.len(), MAX_JOINTS);
            }
            #[test]
            fn staging_bounds_leave_plan_unchanged_and_stop_infinite_children() {
                let mut plan = Assembly::<RigidBody>::new();
                assert_eq!(
                    plan.add_body(
                        RigidBodyBuilder::fixed(),
                        std::iter::repeat_with(|| ColliderBuilder::ball(1.))
                    ),
                    Err(PhysicsError::ChildCapacity)
                );
                assert_eq!(
                    (plan.body_count(), plan.collider_count(), plan.joint_count()),
                    (0, 0, 0)
                );
                let first = plan
                    .add_body(RigidBodyBuilder::fixed(), [] as [Collider; 0])
                    .unwrap();
                assert_eq!(first, LocalBodyId::from_index(0));
                for _ in 1..MAX_ASSEMBLY_BODIES {
                    plan.add_body(RigidBodyBuilder::fixed(), [] as [Collider; 0])
                        .unwrap();
                }
                assert_eq!(
                    plan.add_body(RigidBodyBuilder::fixed(), [] as [Collider; 0]),
                    Err(PhysicsError::BodyCapacity)
                );
                assert_eq!(plan.body_count(), MAX_ASSEMBLY_BODIES);
                for _ in 0..MAX_JOINTS {
                    plan.add_joint(first, first, JointBuilder::hinge()).unwrap();
                }
                assert_eq!(
                    plan.add_joint(first, first, JointBuilder::hinge()),
                    Err(PhysicsError::JointCapacity)
                );
                assert_eq!(plan.joint_count(), MAX_JOINTS);
                let mut plan = Assembly::<RigidBody>::new();
                for _ in 0..MAX_COLLIDERS / MAX_CHILDREN {
                    plan.add_body(
                        RigidBodyBuilder::fixed(),
                        (0..MAX_CHILDREN).map(|_| ColliderBuilder::ball(1.)),
                    )
                    .unwrap();
                }
                let before = (plan.body_count(), plan.collider_count());
                assert_eq!(
                    plan.add_body(RigidBodyBuilder::fixed(), [ColliderBuilder::ball(1.)]),
                    Err(PhysicsError::ColliderCapacity)
                );
                assert_eq!((plan.body_count(), plan.collider_count()), before);
                // Empty anchors remain legal at collider capacity.
                plan.add_body(RigidBodyBuilder::fixed(), [] as [Collider; 0])
                    .unwrap();
            }
            #[test]
            fn joint_contact_filters_apply_on_the_first_tick() {
                for (enabled, contacts_enabled) in [(true, false), (true, true), (false, false)] {
                    let mut w = PhysicsWorld::default();
                    let mut plan = Assembly::new();
                    let a = plan
                        .add_body(RigidBodyBuilder::dynamic(), [ColliderBuilder::ball(1.)])
                        .unwrap();
                    let b = plan
                        .add_body(RigidBodyBuilder::dynamic(), [ColliderBuilder::ball(1.)])
                        .unwrap();
                    let mut desc = JointBuilder::hinge()
                        .contacts_enabled(contacts_enabled)
                        .build();
                    desc.enabled = enabled;
                    plan.add_joint(a, b, desc).unwrap();
                    let maps = w.insert_assembly(plan).unwrap();
                    w.step(1. / 120.);
                    assert_eq!(
                        w.contact_pair(maps.collider(a, 0).unwrap(), maps.collider(b, 0).unwrap())
                            .is_some(),
                        !enabled || contacts_enabled
                    );
                }
            }
            #[test]
            fn validation_is_a_snapshot_and_insertion_rechecks_capacity() {
                let mut w = PhysicsWorld::default();
                for _ in 0..MAX_ASSEMBLY_BODIES - 2 {
                    w.insert_body(RigidBodyBuilder::fixed()).unwrap();
                }
                let plan = mechanism();
                assert_eq!(w.validate_assembly(&plan), Ok(()));
                w.insert_body(RigidBodyBuilder::fixed()).unwrap();
                reject(
                    &mut w,
                    plan,
                    AssemblyError::Capacity(PhysicsError::BodyCapacity),
                );
                w.insert_body(RigidBodyBuilder::fixed()).unwrap();
                // An empty plan still works with every body slot occupied.
                assert_eq!(
                    w.insert_assembly(Assembly::new()).unwrap(),
                    AssemblyHandles::default()
                );
            }
            #[test]
            fn validation_and_rejection_allocate_nothing_after_staging() {
                let mut w = PhysicsWorld::default();
                let good = mechanism();
                let mut bad = mechanism();
                bad.add_joint(
                    LocalBodyId::from_index(0),
                    LocalBodyId::from_index(usize::MAX),
                    JointBuilder::hinge(),
                )
                .unwrap();
                super::ALLOCATIONS.with(|n| n.set(Some(0)));
                for _ in 0..256 {
                    assert_eq!(w.validate_assembly(&good), Ok(()));
                }
                assert!(w.insert_assembly(bad).is_err());
                let count = super::ALLOCATIONS.with(|n| n.replace(None).unwrap());
                assert_eq!(count, 0);
                assert!(w.bodies.is_empty() && w.colliders.is_empty() && w.joints.is_empty());
            }
        }
    };
}
#[cfg(feature = "dim2")]
assembly_tests!(d2, 2, |a| a);
#[cfg(feature = "dim3")]
assembly_tests!(d3, 3, |a| ferravex_math::Vec3::Z * a);

#[cfg(all(feature = "dim2", feature = "dim3"))]
#[test]
fn cross_dimension_fixed_collider_geometry_is_rejected() {
    use ferravex_physics::{
        PhysicsError,
        assembly::{Assembly, AssemblyError},
        d2, d3,
    };
    let mut w2 = d2::PhysicsWorld::default();
    for body in [
        d2::RigidBodyBuilder::fixed(),
        d2::RigidBodyBuilder::kinematic_position_based(),
        d2::RigidBodyBuilder::dynamic(),
    ] {
        let mut plan = Assembly::new();
        let bad = plan
            .add_body(body, [d3::ColliderBuilder::try_cylinder(1., 1.).unwrap()])
            .unwrap();
        assert_eq!(
            w2.insert_assembly(plan),
            Err(AssemblyError::Body {
                body: bad,
                error: PhysicsError::InvalidCollider
            })
        );
    }
    assert!(w2.bodies.is_empty() && w2.colliders.is_empty() && w2.joints.is_empty());
}
