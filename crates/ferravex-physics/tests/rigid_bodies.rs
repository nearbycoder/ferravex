#![cfg(all(feature = "dim2", feature = "dim3"))]
use ferravex_math::{Quat, Vec2, Vec3};
use ferravex_physics::{ContactEvent, d2, d3};
const DT: f32 = 1. / 120.;

#[test]
fn off_center_impulse_uses_mass_and_planar_moment() {
    let mut world = d2::PhysicsWorld::new([0., 0.]);
    let (h, _) = world.insert(
        d2::RigidBodyBuilder::dynamic(),
        d2::ColliderBuilder::cuboid(1., 0.5).density(3.),
    );
    let mass = world.bodies[h].mass();
    assert!((mass - 6.).abs() < 1e-5);
    world.bodies[h].apply_impulse_at_point(Vec2::new(0., 6.), Vec2::X, true);
    assert!((world.bodies[h].linvel().y - 1.).abs() < 1e-5);
    assert!((world.bodies[h].angvel() - 2.4).abs() < 1e-5);
    world.step(DT);
    assert!(world.bodies[h].rotation().angle() > 0.);
}

#[test]
fn anisotropic_inertia_rotates_with_the_body() {
    let mut world = d3::PhysicsWorld::new([0.; 3]);
    let (a, _) = world.insert(
        d3::RigidBodyBuilder::dynamic(),
        d3::ColliderBuilder::cuboid(2., 1., 0.5),
    );
    let (b, _) = world.insert(
        d3::RigidBodyBuilder::dynamic().translation(Vec3::X * 10.),
        d3::ColliderBuilder::cuboid(2., 1., 0.5),
    );
    world.bodies[b].set_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2), true);
    world.bodies[a].apply_torque_impulse(Vec3::X, true);
    world.bodies[b].apply_torque_impulse(Vec3::Y, true);
    assert!((world.bodies[a].angvel().x - world.bodies[b].angvel().y).abs() < 1e-5);
    assert!(world.bodies[b].angvel().x.abs() < 1e-5);
}

#[test]
fn friction_creates_rolling_without_explicit_torque() {
    let mut world = d3::PhysicsWorld::new([0., -9.81, 0.]);
    world.add_box([0., -0.5, 0.], [20., 0.5, 20.], false);
    let (h, _) = world.insert(
        d3::RigidBodyBuilder::dynamic()
            .translation(Vec3::Y * 0.5)
            .linvel(Vec3::X * 5.),
        d3::ColliderBuilder::ball(0.5).friction(0.9),
    );
    for _ in 0..180 {
        world.step(DT);
    }
    let b = &world.bodies[h];
    assert!(b.angvel().z < -4.);
    assert!(b.linvel().x > 2.);
    assert!((b.linvel().x + b.angvel().z * 0.5).abs() < 0.08);
    assert!((b.translation().y - 0.5).abs() < 0.01);
}

#[test]
fn restitution_returns_energy_without_amplifying_it() {
    let mut world = d2::PhysicsWorld::new([0., 0.]);
    world.insert(
        d2::RigidBodyBuilder::fixed(),
        d2::ColliderBuilder::cuboid(0.05, 5.).restitution(0.8),
    );
    let (h, _) = world.insert(
        d2::RigidBodyBuilder::dynamic()
            .translation(Vec2::new(-1., 0.))
            .linvel(Vec2::new(10., 0.))
            .ccd_enabled(true),
        d2::ColliderBuilder::ball(0.2).restitution(0.8),
    );
    for _ in 0..20 {
        world.step(DT);
    }
    assert!((world.bodies[h].linvel().x + 8.).abs() < 0.02);
}

#[test]
fn swept_ball_stops_at_thin_rotated_box_at_high_speed() {
    let mut world = d3::PhysicsWorld::new([0.; 3]);
    world.insert(
        d3::RigidBodyBuilder::fixed().rotation(Vec3::Z * 0.35),
        d3::ColliderBuilder::cuboid(0.005, 8., 8.),
    );
    let (h, _) = world.insert(
        d3::RigidBodyBuilder::dynamic()
            .translation(Vec3::new(-6., 0., 0.))
            .linvel(Vec3::X * 1600.)
            .ccd_enabled(true),
        d3::ColliderBuilder::ball(0.1).friction(0.),
    );
    let stats = world.step(1. / 60.);
    assert!(stats.ccd_impacts > 0);
    let local = Quat::from_rotation_z(-0.35) * world.bodies[h].translation();
    assert!(local.x < -0.09, "{local:?}");
    assert!(world.bodies[h].linvel().is_finite());
}

#[test]
fn swept_ball_cannot_cross_thin_capsule_rail() {
    let mut world = d2::PhysicsWorld::new([0.; 2]);
    world.insert(
        d2::RigidBodyBuilder::fixed(),
        d2::ColliderBuilder::capsule_from_endpoints(Vec2::new(0., -3.), Vec2::new(0., 3.), 0.01),
    );
    let (h, _) = world.insert(
        d2::RigidBodyBuilder::dynamic()
            .translation(Vec2::new(-5., 0.))
            .linvel(Vec2::new(1000., 0.))
            .ccd_enabled(true),
        d2::ColliderBuilder::ball(0.1),
    );
    world.step(1. / 60.);
    assert!(world.bodies[h].translation().x < -0.1);
}

#[test]
fn sensors_layers_and_generation_handles_have_real_lifetimes() {
    let mut world = d2::PhysicsWorld::new([0.; 2]);
    let (_, sensor) = world.insert(
        d2::RigidBodyBuilder::fixed(),
        d2::ColliderBuilder::ball(2.)
            .sensor(true)
            .collision_layers(1, 2),
    );
    let (body, collider) = world.insert(
        d2::RigidBodyBuilder::dynamic(),
        d2::ColliderBuilder::ball(0.5).collision_layers(2, 1),
    );
    world.step(DT);
    assert!(world.contact_pair(sensor, collider).unwrap().sensor);
    assert_eq!(world.contact_events().len(), 1);
    assert!(matches!(
        world.contact_events()[0],
        ContactEvent::Started(..)
    ));
    assert!(world.remove_body(body));
    assert!(!world.remove_body(body));
    assert!(world.bodies.get(body).is_none());
    assert!(world.colliders.get(collider).is_none());
    let fresh = world.add_ball([0., 0.], 0.5, true);
    assert_ne!(fresh, body);
    world.step(DT);
    assert!(
        world
            .contact_events()
            .iter()
            .any(|e| matches!(e, ContactEvent::Stopped(..)))
    );
    world.remove_body(fresh);
    let (excluded, _) = world.insert(
        d2::RigidBodyBuilder::dynamic(),
        d2::ColliderBuilder::ball(0.1).collision_layers(4, 4),
    );
    world.step(DT);
    assert_eq!(world.bodies[excluded].translation(), Vec2::ZERO);
}

#[test]
fn oriented_box_raycast_and_cylinder_contact_use_true_geometry() {
    let mut world = d3::PhysicsWorld::new([0., -9.81, 0.]);
    let (box_h, _) = world.insert(
        d3::RigidBodyBuilder::fixed().rotation(Vec3::Z * 0.5),
        d3::ColliderBuilder::cuboid(1., 0.1, 1.),
    );
    let hit = world.raycast([0., 3., 0.], [0., -1., 0.], 10.).unwrap();
    assert_eq!(hit.body, Some(box_h));
    assert!(hit.normal[0] < -0.4);
    world.remove_body(box_h);
    world.insert(
        d3::RigidBodyBuilder::fixed(),
        d3::ColliderBuilder::cylinder(0.5, 1.),
    );
    let ball = world.add_ball([0., 2., 0.], 0.2, true);
    for _ in 0..240 {
        world.step(DT);
    }
    assert!((world.bodies[ball].translation().y - 0.7).abs() < 0.01);
    let miss = world.raycast([0.95, 3., 0.95], [0., -1., 0.], 10.);
    assert!(miss.is_none(), "cylinder must not collide like its AABB");
}

#[test]
fn stacked_boxes_settle_sleep_and_wake_from_projectile() {
    let mut world = d3::PhysicsWorld::new([0., -9.81, 0.]);
    world.add_box([0., -0.5, 0.], [8., 0.5, 8.], false);
    let boxes: Vec<_> = (0..4)
        .map(|i| world.add_box([0., 0.5 + i as f32 * 1.002, 0.], [0.5; 3], true))
        .collect();
    for _ in 0..1200 {
        world.step(DT);
    }
    for (i, &h) in boxes.iter().enumerate() {
        let b = &world.bodies[h];
        assert!((b.translation().y - (0.5 + i as f32)).abs() < 0.04);
        assert!(((*b.rotation()) * Vec3::Y).y > 0.99);
    }
    assert!(boxes.iter().any(|&h| world.bodies[h].is_sleeping()));
    world.insert(
        d3::RigidBodyBuilder::dynamic()
            .translation(Vec3::new(-4., 2.8, 0.3))
            .linvel(Vec3::X * 30.)
            .ccd_enabled(true),
        d3::ColliderBuilder::ball(0.3).density(15.),
    );
    let mut spun = false;
    for _ in 0..360 {
        world.step(DT);
        spun |= world.bodies[boxes[3]].angvel().length() > 1.;
    }
    assert!(world.bodies[boxes[3]].translation().x.abs() > 1.);
    assert!(spun, "projectile contact must transfer angular momentum");
}

#[test]
fn rotated_2d_box_lands_on_its_edges() {
    let mut world = d2::PhysicsWorld::new([0., -9.81]);
    world.add_box([0., -0.5], [10., 0.5], false);
    let (h, _) = world.insert(
        d2::RigidBodyBuilder::dynamic()
            .translation(Vec2::new(0., 3.))
            .rotation(0.6),
        d2::ColliderBuilder::cuboid(0.8, 0.4),
    );
    let mut spun = false;
    for _ in 0..600 {
        world.step(DT);
        spun |= world.bodies[h].angvel().abs() > 0.5;
    }
    assert!(spun);
    assert!((world.bodies[h].translation().y - 0.4).abs() < 0.03);
}

#[test]
fn relative_sweep_catches_fast_kinematic_wall_against_stationary_ball() {
    let mut world = d2::PhysicsWorld::new([0.; 2]);
    let (wall, _) = world.insert(
        d2::RigidBodyBuilder::kinematic_position_based().translation(Vec2::new(-5., 0.)),
        d2::ColliderBuilder::cuboid(0.01, 2.),
    );
    let (ball, _) = world.insert(
        d2::RigidBodyBuilder::dynamic().ccd_enabled(true),
        d2::ColliderBuilder::ball(0.1),
    );
    world.bodies[wall].set_next_kinematic_translation(Vec2::new(5., 0.));
    assert!(world.step(1. / 60.).ccd_impacts > 0);
    assert!(world.bodies[ball].translation().x > 5.09);
    assert!(world.bodies[ball].linvel().x > 500.);
}

#[test]
fn accumulated_force_is_integrated_once_and_then_cleared() {
    let mut world = d3::PhysicsWorld::new([0.; 3]);
    let ball = world.add_ball([0.; 3], 0.5, true);
    let mass = world.bodies[ball].mass();
    world.bodies[ball].add_force(Vec3::X * mass * 120., true);
    world.step(DT);
    assert!((world.bodies[ball].linvel().x - 1.).abs() < 0.0001);
    world.step(DT);
    assert!((world.bodies[ball].linvel().x - 1.).abs() < 0.0001);
}
