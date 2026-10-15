#![cfg(feature = "dim3")]
use ferravex_math::{Quat, Vec3};
use ferravex_physics::d3::{ColliderBuilder, PhysicsWorld, RigidBodyBuilder};
#[test]
fn camera_volume_hits_thin_rotated_parts_that_a_center_ray_misses() {
    let mut w = PhysicsWorld::default();
    let rotation = Quat::from_rotation_y(0.4);
    let wall = w
        .insert_compound(
            RigidBodyBuilder::fixed(),
            [
                ColliderBuilder::cuboid(0.02, 1., 1.)
                    .translation(Vec3::new(3., 0., 0.))
                    .rotation(rotation),
                ColliderBuilder::cuboid(0.1, 1., 1.).translation(Vec3::new(8., 0., 0.)),
            ],
        )
        .unwrap();
    let c = w.bodies[wall].colliders()[0];
    let origin = Vec3::new(0., 1.2, 0.);
    let direction = Vec3::X;
    assert!(
        w.raycast(origin.to_array(), direction.to_array(), 6.)
            .is_none()
    );
    let hit = w
        .spherecast(
            origin.to_array(),
            direction.to_array(),
            6.,
            0.3,
            u32::MAX,
            None,
        )
        .unwrap();
    assert_eq!(hit.collider, c);
    assert!((2. ..3.1).contains(&hit.distance));
    assert!((hit.point[0] - hit.distance).abs() < 1e-5);
    w.set_collider_pose(c, Vec3::new(4., 0., 0.), rotation)
        .unwrap();
    let moved = w
        .spherecast(
            origin.to_array(),
            direction.to_array(),
            6.,
            0.3,
            u32::MAX,
            None,
        )
        .unwrap();
    assert!((moved.distance - hit.distance - 1.).abs() < 0.001);
    assert!(
        w.spherecast(
            origin.to_array(),
            direction.to_array(),
            6.,
            0.3,
            u32::MAX,
            Some(wall)
        )
        .is_none()
    );
}
#[test]
fn zero_radius_matches_rays_and_invalid_sweeps_do_not_mutate() {
    let mut w = PhysicsWorld::default();
    let b = w.add_ball([3., 0., 0.], 0.5, false);
    let ray = w.raycast([0.; 3], [1., 0., 0.], 10.).unwrap();
    let swept = w
        .spherecast([0.; 3], [1., 0., 0.], 10., 0., u32::MAX, None)
        .unwrap();
    assert_eq!(ray.distance, swept.distance);
    assert_eq!(ray.collider, swept.collider);
    assert!(
        w.spherecast([0.; 3], [1., 0., 0.], 10., f32::NAN, u32::MAX, None)
            .is_none()
    );
    assert!(
        w.spherecast([0.; 3], [0.; 3], 10., 0.2, u32::MAX, None)
            .is_none()
    );
    let overlap = w
        .spherecast([3., 0., 0.], [1., 0., 0.], 0., 0.2, u32::MAX, None)
        .unwrap();
    assert_eq!(overlap.distance, 0.);
    assert_eq!(w.bodies[b].translation(), Vec3::X * 3.);
}
