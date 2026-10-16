#![cfg(feature = "dim3")]
use ferravex_math::Vec3;
use ferravex_physics::d3::*;
#[test]
fn actual_constraint_load_breaks_once_and_wakes_released_chunk() {
    let mut w = PhysicsWorld::new([0.; 3]);
    let anchor = w.insert_body(RigidBodyBuilder::fixed()).unwrap();
    let (chunk, _) = w.insert(
        RigidBodyBuilder::dynamic().translation(Vec3::Y),
        ColliderBuilder::cuboid(0.5, 0.5, 0.5),
    );
    let joint = w
        .insert_joint(
            anchor,
            chunk,
            JointBuilder::fixed()
                .local_anchor1(Vec3::Y)
                .break_limits(50., 50.),
        )
        .unwrap();
    w.step(1. / 120.);
    assert!(w.joint_break_events().is_empty());
    w.bodies[chunk].apply_impulse(Vec3::X * 10., true);
    w.step(1. / 120.);
    assert_eq!(w.joint_break_events().len(), 1);
    let event = w.joint_break_events()[0];
    assert_eq!(event.joint, joint);
    assert!(event.load.force > 50.);
    assert!(w.joints.get(joint).is_none());
    w.step(1. / 120.);
    assert!(w.joint_break_events().is_empty());
    let replacement = w
        .insert_joint(anchor, chunk, JointBuilder::distance(1.))
        .unwrap();
    assert_ne!(replacement, joint);
    assert!(!w.remove_joint(joint));
}
