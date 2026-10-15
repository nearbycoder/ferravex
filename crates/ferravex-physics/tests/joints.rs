#![cfg(all(feature = "dim2", feature = "dim3"))]
use ferravex_math::{Vec2, Vec3};
use ferravex_physics::{JointDesc, JointKind, JointMotor, PhysicsError, SolverSettings, d2, d3};
const DT: f32 = 1. / 120.;
fn advance(w: &mut d3::PhysicsWorld, ticks: usize) {
    for _ in 0..ticks {
        w.step(DT);
    }
}
fn anchor(w: &mut d3::PhysicsWorld, p: Vec3) -> ferravex_physics::RigidBodyHandle {
    w.insert_body(d3::RigidBodyBuilder::fixed().translation(p))
        .unwrap()
}
#[test]
fn distance_pendulum_retains_length_under_gravity_and_off_center_load() {
    let mut w = d3::PhysicsWorld::new([0., -9.81, 0.]);
    let a = anchor(&mut w, Vec3::new(0., 5., 0.));
    let b = w
        .insert_compound(
            d3::RigidBodyBuilder::dynamic()
                .translation(Vec3::new(2., 5., 0.))
                .linvel(Vec3::new(0., 0., 3.)),
            [
                d3::ColliderBuilder::ball(0.3).translation(Vec3::new(0.4, 0., 0.)),
                d3::ColliderBuilder::ball(0.2).translation(Vec3::new(-0.4, 0., 0.)),
            ],
        )
        .unwrap();
    let j = w
        .insert_joint(a, b, d3::JointBuilder::distance(2.))
        .unwrap();
    for _ in 0..2400 {
        w.step(DT);
        let p = w.joint_anchors(j).unwrap();
        assert!((p[0].distance(p[1]) - 2.).abs() < 0.003);
    }
    assert!(w.bodies[b].translation().is_finite());
}
#[test]
fn fixed_joint_holds_relative_pose_on_an_asymmetric_compound() {
    let mut w = d3::PhysicsWorld::new([0., -9.81, 0.]);
    let a = anchor(&mut w, Vec3::new(0., 4., 0.));
    let b = w
        .insert_compound(
            d3::RigidBodyBuilder::dynamic().translation(Vec3::new(1., 4., 0.)),
            [
                d3::ColliderBuilder::cuboid(0.6, 0.2, 0.2).translation(Vec3::X),
                d3::ColliderBuilder::ball(0.4).translation(-Vec3::X),
            ],
        )
        .unwrap();
    let j = w
        .insert_joint(a, b, d3::JointBuilder::fixed().local_anchor1(Vec3::X))
        .unwrap();
    w.bodies[b].apply_impulse_at_point(Vec3::new(0., 0., 4.), Vec3::new(1., 5., 0.), true);
    advance(&mut w, 600);
    let p = w.joint_anchors(j).unwrap();
    assert!(p[0].distance(p[1]) < 0.001);
    assert!((w.bodies[b].rotation().dot(ferravex_math::Quat::IDENTITY)).abs() > 0.9999);
    assert!(w.bodies[b].is_sleeping());
}
#[test]
fn spring_settles_to_hooke_law_extension_instead_of_a_rigid_length() {
    let mut w = d3::PhysicsWorld::new([0., -10., 0.]);
    let a = anchor(&mut w, Vec3::new(0., 5., 0.));
    let b = w.add_ball([0., 3., 0.], 0.5, true);
    let mass = w.bodies[b].mass();
    let j = w
        .insert_joint(a, b, d3::JointBuilder::spring(2., 20., 5.))
        .unwrap();
    advance(&mut w, 2400);
    let p = w.joint_anchors(j).unwrap();
    let extension = p[0].distance(p[1]) - 2.;
    assert!(
        (extension - mass * 10. / 20.).abs() < 0.015,
        "extension {extension}, expected {}, velocity {:?}, sleeping {}",
        mass * 10. / 20.,
        w.bodies[b].linvel(),
        w.bodies[b].is_sleeping()
    );
    assert!(w.bodies[b].is_sleeping());
}
#[test]
fn hinge_motor_has_a_torque_budget_and_recovers_after_an_impact() {
    let mut w = d2::PhysicsWorld::default();
    let a = w.insert_body(d2::RigidBodyBuilder::fixed()).unwrap();
    let (b, _) = w.insert(
        d2::RigidBodyBuilder::dynamic().ccd_enabled(true),
        d2::ColliderBuilder::cuboid(1., 0.2).translation(Vec2::X),
    );
    let j = w
        .insert_joint(
            a,
            b,
            d2::JointBuilder::hinge()
                .limits(-0.6, 0.8)
                .motor(JointMotor::velocity(30., 0.01)),
        )
        .unwrap();
    w.step(DT);
    assert!(
        w.bodies[b].angvel().abs() < 0.01,
        "motor exceeded torque cap"
    );
    w.joints[j]
        .set_motor(Some(JointMotor::position(0.5, 20., 8., 80.)))
        .unwrap();
    for _ in 0..600 {
        w.step(DT);
        let angle = w.joint_angle(j).unwrap();
        assert!((-0.605..=0.805).contains(&angle));
    }
    assert!((w.joint_angle(j).unwrap() - 0.5).abs() < 0.005);
    w.bodies[b].apply_torque_impulse(-3., true);
    for _ in 0..600 {
        w.step(DT);
    }
    assert!((w.joint_angle(j).unwrap() - 0.5).abs() < 0.005);
    let p = w.joint_anchors(j).unwrap();
    assert!(p[0].distance(p[1]) < 0.001);
}
#[test]
fn spatial_hinge_aligns_axes_and_leaves_twist_free() {
    let mut w = d3::PhysicsWorld::default();
    let a = anchor(&mut w, Vec3::ZERO);
    let b = w.add_box([0.; 3], [0.5, 1., 0.2], true);
    let j = w
        .insert_joint(
            a,
            b,
            d3::JointBuilder::hinge().motor(JointMotor::velocity(2., 20.)),
        )
        .unwrap();
    w.bodies[b].apply_torque_impulse(Vec3::new(2., 0., 3.), true);
    advance(&mut w, 300);
    let axis = *w.bodies[b].rotation() * Vec3::Y;
    assert!(axis.distance(Vec3::Y) < 0.002, "axis {axis:?}");
    assert!((w.bodies[b].angvel().y - 2.).abs() < 0.001);
    assert!(w.joint_angle(j).unwrap().abs() > 0.3);
}
#[test]
fn connected_sleeping_island_wakes_as_a_unit_and_detaches_safely() {
    let mut w = d3::PhysicsWorld::new([0., -10., 0.]);
    let a = anchor(&mut w, Vec3::new(0., 5., 0.));
    let b = w.add_ball([0., 4., 0.], 0.2, true);
    let c = w.add_ball([0., 3., 0.], 0.2, true);
    let ab = w
        .insert_joint(a, b, d3::JointBuilder::distance(1.))
        .unwrap();
    let bc = w
        .insert_joint(b, c, d3::JointBuilder::distance(1.))
        .unwrap();
    advance(&mut w, 1000);
    assert!(w.bodies[b].is_sleeping() && w.bodies[c].is_sleeping());
    w.bodies[c].apply_impulse(Vec3::X, true);
    w.step(DT);
    assert!(!w.bodies[b].is_sleeping() && !w.bodies[c].is_sleeping());
    assert_eq!(w.last_step.islands, 1);
    assert!(w.remove_body(b));
    assert!(w.joints.get(ab).is_none() && w.joints.get(bc).is_none());
    let replacement = w.add_ball([0., 4., 0.], 0.2, true);
    let next = w
        .insert_joint(a, replacement, d3::JointBuilder::distance(1.))
        .unwrap();
    assert_ne!(next, ab);
    assert!(!w.remove_joint(ab));
    assert!(!w.bodies[c].is_sleeping());
    w.bodies[c].set_linvel(Vec3::ZERO, true);
    let y = w.bodies[c].translation().y;
    advance(&mut w, 60);
    assert!(
        w.bodies[c].translation().y < y - 0.5,
        "before {y} after {:?} velocity {:?} sleeping {} joints {}",
        w.bodies[c].translation(),
        w.bodies[c].linvel(),
        w.bodies[c].is_sleeping(),
        w.joints.len()
    );
}
#[test]
fn moving_kinematic_anchor_wakes_attached_bodies() {
    let mut w = d3::PhysicsWorld::default();
    let a = w
        .insert_body(d3::RigidBodyBuilder::kinematic_position_based())
        .unwrap();
    let b = w.add_ball([1., 0., 0.], 0.2, true);
    w.insert_joint(a, b, d3::JointBuilder::fixed().local_anchor1(Vec3::X))
        .unwrap();
    advance(&mut w, 300);
    assert!(w.bodies[b].is_sleeping());
    w.bodies[a].set_next_kinematic_translation(Vec3::new(0., 1., 0.));
    w.step(DT);
    assert!(!w.bodies[b].is_sleeping());
    assert!(w.bodies[b].linvel().y > 1.);
    advance(&mut w, 100);
    assert!((w.bodies[b].translation() - Vec3::new(1., 1., 0.)).length() < 0.002);
}
#[test]
fn linked_pair_collision_filter_covers_ccd_and_runtime_reenable() {
    let mut w = d3::PhysicsWorld::default();
    let (a, ca) = w.insert(d3::RigidBodyBuilder::fixed(), d3::ColliderBuilder::ball(1.));
    let (b, cb) = w.insert(
        d3::RigidBodyBuilder::dynamic()
            .translation(Vec3::new(-3., 0., 0.))
            .linvel(Vec3::new(100., 0., 0.))
            .ccd_enabled(true),
        d3::ColliderBuilder::ball(0.2),
    );
    let j = w
        .insert_joint(a, b, d3::JointBuilder::spring(3., 0.001, 0.))
        .unwrap();
    for _ in 0..6 {
        w.step(DT);
        assert!(w.contact_pair(ca, cb).is_none());
        assert_eq!(w.last_step.ccd_impacts, 0);
    }
    w.bodies[b].set_translation(Vec3::new(0.8, 0., 0.), true);
    w.bodies[b].set_linvel(Vec3::ZERO, true);
    w.joints[j].set_contacts_enabled(true);
    w.step(DT);
    assert!(w.contact_pair(ca, cb).is_some());
    w.joints[j].set_enabled(false);
    w.step(DT);
    assert_eq!(w.last_step.joint_rows, 0);
}
#[test]
fn invalid_joint_edits_leave_the_live_connection_unchanged() {
    let mut w = d3::PhysicsWorld::default();
    let a = anchor(&mut w, Vec3::ZERO);
    let b = w.add_ball([1., 0., 0.], 0.2, true);
    assert_eq!(
        w.insert_joint(a, a, d3::JointBuilder::fixed()),
        Err(PhysicsError::InvalidJoint)
    );
    assert_eq!(
        w.insert_joint(a, b, d3::JointBuilder::spring(1., -1., 2.)),
        Err(PhysicsError::InvalidJoint)
    );
    let desc = JointDesc {
        local_anchor1: Vec3::splat(f32::NAN),
        ..JointDesc::new(JointKind::Fixed)
    };
    assert_eq!(w.insert_joint(a, b, desc), Err(PhysicsError::InvalidJoint));
    let j = w.insert_joint(a, b, d3::JointBuilder::hinge()).unwrap();
    let before = w.joints[j].description();
    assert!(w.joints[j].set_limits(Some([1., -1.])).is_err());
    assert!(
        w.joints[j]
            .set_motor(Some(JointMotor::velocity(f32::NAN, 1.)))
            .is_err()
    );
    assert_eq!(before, w.joints[j].description());
    w.remove_body(b);
    assert_eq!(
        w.insert_joint(a, b, d3::JointBuilder::fixed()),
        Err(PhysicsError::InvalidHandle)
    );
    let mut planar = d2::PhysicsWorld::default();
    let x = planar.insert_body(d2::RigidBodyBuilder::fixed()).unwrap();
    let y = planar.add_ball([1., 0.], 0.2, true);
    assert_eq!(
        planar.insert_joint(
            x,
            y,
            JointDesc::new(JointKind::Hinge {
                axis1: Vec3::X,
                axis2: Vec3::X
            })
        ),
        Err(PhysicsError::InvalidJoint)
    );
}
#[test]
fn mass_edits_and_timestep_changes_do_not_reuse_incompatible_impulses() {
    for warm in [false, true] {
        let mut w = d3::PhysicsWorld::new([0., -10., 0.]);
        w.set_solver_settings(SolverSettings {
            warm_start: warm,
            allow_sleep: false,
            ..Default::default()
        })
        .unwrap();
        let a = anchor(&mut w, Vec3::new(0., 4., 0.));
        let (b, c) = w.insert(
            d3::RigidBodyBuilder::dynamic().translation(Vec3::new(0., 2., 0.)),
            d3::ColliderBuilder::ball(0.3),
        );
        let j = w
            .insert_joint(a, b, d3::JointBuilder::distance(2.))
            .unwrap();
        advance(&mut w, 100);
        w.colliders[c].set_density(10.);
        w.set_collider_pose(c, Vec3::new(0.2, 0., 0.), ferravex_math::Quat::IDENTITY)
            .unwrap();
        for i in 0..300 {
            w.step(if i % 2 == 0 { 1. / 240. } else { 1. / 30. });
            let p = w.joint_anchors(j).unwrap();
            assert!((p[0].distance(p[1]) - 2.).abs() < 0.005);
            assert!(w.bodies[b].linvel().length() < 2.);
        }
    }
}

#[test]
fn hinge_limits_stop_an_unpowered_free_rotor_on_both_sides() {
    for direction in [-1., 1.] {
        let mut w = d2::PhysicsWorld::default();
        let a = w.insert_body(d2::RigidBodyBuilder::fixed()).unwrap();
        let b = w.add_box([0.; 2], [0.5, 0.2], true);
        let j = w
            .insert_joint(a, b, d2::JointBuilder::hinge().limits(-0.4, 0.6))
            .unwrap();
        w.bodies[b].set_angvel(direction * 8., true);
        for _ in 0..120 {
            w.step(DT);
            assert!((-0.401..=0.601).contains(&w.joint_angle(j).unwrap()));
        }
        assert!(w.bodies[b].angvel().abs() < 0.001);
    }
}
#[test]
fn joint_and_compound_capacity_failures_do_not_mutate_world() {
    use ferravex_physics::{MAX_CHILDREN, MAX_JOINTS};
    let mut w = d3::PhysicsWorld::default();
    let a = anchor(&mut w, Vec3::ZERO);
    let b = w.add_ball([1., 0., 0.], 0.2, true);
    for _ in 0..MAX_JOINTS {
        w.insert_joint(a, b, d3::JointBuilder::distance(1.))
            .unwrap();
    }
    assert_eq!(
        w.insert_joint(a, b, d3::JointBuilder::distance(1.)),
        Err(PhysicsError::JointCapacity)
    );
    assert_eq!(w.joints.len(), MAX_JOINTS);
    let before = (w.bodies.len(), w.colliders.len());
    assert_eq!(
        w.insert_compound(
            d3::RigidBodyBuilder::dynamic(),
            (0..=MAX_CHILDREN).map(|_| d3::ColliderBuilder::ball(0.1))
        ),
        Err(PhysicsError::ChildCapacity)
    );
    assert_eq!((w.bodies.len(), w.colliders.len()), before);
}
