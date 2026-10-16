#![cfg(all(feature = "dim2", feature = "dim3"))]
use ferravex_math::{Vec2, Vec3};
use ferravex_physics::{d2, d3};
#[test]
fn fast_box_cannot_tunnel_through_thin_rotated_wall() {
    let mut w = d3::PhysicsWorld::new([0.; 3]);
    w.insert(
        d3::RigidBodyBuilder::fixed().rotation(Vec3::Z * 0.35),
        d3::ColliderBuilder::cuboid(0.005, 1000., 1000.),
    );
    let (b, _) = w.insert(
        d3::RigidBodyBuilder::dynamic()
            .translation(Vec3::new(-5., 0., 0.))
            .linvel(Vec3::X * 3000.)
            .ccd_enabled(true),
        d3::ColliderBuilder::cuboid(0.2, 0.3, 0.25),
    );
    let stats = w.step(1. / 60.);
    assert!(stats.ccd_impacts > 0);
    assert!(
        w.bodies[b]
            .translation()
            .dot(Vec3::new(0.35_f32.cos(), 0.35_f32.sin(), 0.))
            < 0.3,
        "{:?}",
        w.bodies[b].translation()
    );
    assert!(w.bodies[b].linvel().is_finite());
}
#[test]
fn planar_box_sweep_accounts_for_moving_kinematic_target() {
    let mut w = d2::PhysicsWorld::new([0.; 2]);
    let (wall, _) = w.insert(
        d2::RigidBodyBuilder::kinematic_position_based(),
        d2::ColliderBuilder::cuboid(0.005, 10.),
    );
    w.bodies[wall].set_next_kinematic_translation(Vec2::new(-1., 0.));
    let (b, _) = w.insert(
        d2::RigidBodyBuilder::dynamic()
            .translation(Vec2::new(-5., 0.))
            .linvel(Vec2::X * 2000.)
            .ccd_enabled(true),
        d2::ColliderBuilder::cuboid(0.2, 0.2),
    );
    assert!(w.step(1. / 60.).ccd_impacts > 0);
    assert!(w.bodies[b].translation().x < 0.);
}
#[test]
fn sweep_respects_sensor_and_collision_masks() {
    let mut w = d3::PhysicsWorld::new([0.; 3]);
    w.insert(
        d3::RigidBodyBuilder::fixed(),
        d3::ColliderBuilder::cuboid(0.005, 8., 8.).sensor(true),
    );
    let (b, _) = w.insert(
        d3::RigidBodyBuilder::dynamic()
            .translation(Vec3::new(-5., 0., 0.))
            .linvel(Vec3::X * 1000.)
            .ccd_enabled(true),
        d3::ColliderBuilder::cuboid(0.2, 0.2, 0.2),
    );
    w.step(1. / 60.);
    assert!(w.bodies[b].translation().x > 5.);
}
