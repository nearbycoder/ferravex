#![cfg(feature = "dim3")]
use ferravex_math::Vec3;
use ferravex_physics::{PhysicsMaterial, d3::*};
fn run(resistance: f32) -> f32 {
    let mut w = PhysicsWorld::new([0., -9.81, 0.]);
    w.insert(
        RigidBodyBuilder::fixed().translation(Vec3::new(0., -0.5, 0.)),
        ColliderBuilder::cuboid(30., 0.5, 30.).material(PhysicsMaterial {
            rolling_resistance: resistance,
            ..Default::default()
        }),
    );
    let (b, _) = w.insert(
        RigidBodyBuilder::dynamic()
            .translation(Vec3::Y * 0.5)
            .linvel(Vec3::X * 4.),
        ColliderBuilder::ball(0.5).material(PhysicsMaterial {
            rolling_resistance: resistance,
            ..Default::default()
        }),
    );
    for _ in 0..240 {
        w.step(1. / 120.);
    }
    w.bodies[b].linvel().length()
}
#[test]
fn rolling_resistance_dissipates_contact_motion() {
    let free = run(0.);
    let drag = run(0.05);
    assert!(free > 2.);
    assert!(drag < free * 0.7, "{drag} vs {free}");
}
#[test]
fn invalid_material_edit_leaves_existing_material() {
    let mut w = PhysicsWorld::new([0.; 3]);
    let (_, c) = w.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
    let old = w.colliders[c].material();
    assert!(
        w.colliders[c]
            .set_material(PhysicsMaterial {
                sliding_friction: f32::NAN,
                ..old
            })
            .is_err()
    );
    assert_eq!(w.colliders[c].material(), old);
}
fn pushed(static_friction: f32) -> f32 {
    let mut w = PhysicsWorld::new([0., -9.81, 0.]);
    let m = PhysicsMaterial {
        static_friction,
        sliding_friction: 0.15,
        ..Default::default()
    };
    w.insert(
        RigidBodyBuilder::fixed().translation(Vec3::new(0., -0.5, 0.)),
        ColliderBuilder::cuboid(30., 0.5, 30.).material(m),
    );
    let (b, _) = w.insert(
        RigidBodyBuilder::dynamic().translation(Vec3::Y * 0.5),
        ColliderBuilder::cuboid(0.5, 0.5, 0.5).material(m),
    );
    for _ in 0..240 {
        w.bodies[b].apply_impulse(Vec3::X * (2.5 / 120.), true);
        w.step(1. / 120.);
    }
    w.bodies[b].translation().x
}
#[test]
fn static_grip_holds_a_load_that_sliding_grip_cannot() {
    let held = pushed(1.);
    let slide = pushed(0.15);
    assert!(held.abs() < 0.05, "{held}");
    assert!(slide > 1., "{slide}");
}
