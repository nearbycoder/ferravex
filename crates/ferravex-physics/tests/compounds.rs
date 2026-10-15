#![cfg(all(feature = "dim2", feature = "dim3"))]
use ferravex_math::{Quat, Vec2, Vec3};
use ferravex_physics::{
    PhysicsError,
    character::{CharacterSettings, CharacterShape, move_and_slide},
    d2, d3,
};
const DT: f32 = 1. / 120.;
#[test]
fn separated_parts_keep_their_gap_in_rays_contacts_and_character_queries() {
    let mut w = d3::PhysicsWorld::new([0.; 3]);
    let b = w
        .insert_compound(
            d3::RigidBodyBuilder::fixed(),
            [-2., 2.].map(|x| d3::ColliderBuilder::ball(0.5).translation(Vec3::new(x, 1., 0.))),
        )
        .unwrap();
    assert!(w.raycast([0., 5., 0.], [0., -1., 0.], 10.).is_none());
    let hit = w.raycast([2., 5., 0.], [0., -1., 0.], 10.).unwrap();
    assert_eq!(hit.body, Some(b));
    assert_eq!(hit.collider, w.bodies[b].colliders()[1]);
    assert!((hit.distance - 3.5).abs() < 1e-4);
    let m = move_and_slide(
        &w,
        CharacterShape::Capsule {
            radius: 0.3,
            height: 1.8,
        },
        CharacterSettings {
            ground_snap: 0.,
            step_height: 0.,
            ..Default::default()
        },
        Vec3::new(0., 0., 3.),
        Vec3::Z * -6.,
        false,
    )
    .unwrap();
    assert!(m.position.z < -2.99);
    let m = move_and_slide(
        &w,
        CharacterShape::Capsule {
            radius: 0.3,
            height: 1.8,
        },
        Default::default(),
        Vec3::ZERO,
        Vec3::X * 4.,
        false,
    )
    .unwrap();
    assert!(m.position.x < 1.21);
}
#[test]
fn compound_mass_center_and_parallel_axis_moments_are_analytic() {
    let mut w = d3::PhysicsWorld::new([0.; 3]);
    let b = w
        .insert_compound(
            d3::RigidBodyBuilder::dynamic(),
            [-1., 1.].map(|x| d3::ColliderBuilder::ball(0.5).translation(Vec3::X * x)),
        )
        .unwrap();
    let mass = std::f32::consts::PI / 3.;
    assert!((w.bodies[b].mass() - mass).abs() < 1e-5);
    assert_eq!(w.bodies[b].local_center_of_mass(), Vec3::ZERO);
    w.bodies[b].apply_torque_impulse(Vec3::X, true);
    assert!((w.bodies[b].angvel().x - 1. / (mass * 0.1)).abs() < 1e-4);
    w.bodies[b].set_angvel(Vec3::ZERO, true);
    w.bodies[b].apply_torque_impulse(Vec3::Y, true);
    assert!((w.bodies[b].angvel().y - 1. / (mass * 1.1)).abs() < 1e-5);
}
#[test]
fn rotated_child_uses_full_tensor_including_off_diagonal_terms() {
    let mut w = d3::PhysicsWorld::new([0.; 3]);
    let q = Quat::from_rotation_z(0.6);
    let half = Vec3::new(2., 0.5, 0.25);
    let b = w
        .insert_compound(
            d3::RigidBodyBuilder::dynamic(),
            [d3::ColliderBuilder::cuboid(half.x, half.y, half.z).rotation(q)],
        )
        .unwrap();
    let mass = w.bodies[b].mass();
    let moment = Vec3::new(
        half.y * half.y + half.z * half.z,
        half.x * half.x + half.z * half.z,
        half.x * half.x + half.y * half.y,
    ) * mass
        / 3.;
    let expected = q * ((q.conjugate() * Vec3::X) / moment);
    w.bodies[b].apply_torque_impulse(Vec3::X, true);
    assert!(w.bodies[b].angvel().distance(expected) < 1e-5);
    assert!(w.bodies[b].angvel().y.abs() > 0.1);
}
#[test]
fn asymmetric_body_spins_about_its_mass_center_without_moving_it() {
    let mut w = d3::PhysicsWorld::new([0.; 3]);
    let b = w
        .insert_compound(
            d3::RigidBodyBuilder::dynamic()
                .translation(Vec3::X * 10.)
                .angvel(Vec3::Z * 2.),
            [
                d3::ColliderBuilder::ball(0.5).translation(Vec3::X * -1.),
                d3::ColliderBuilder::ball(0.5)
                    .translation(Vec3::X)
                    .density(3.),
            ],
        )
        .unwrap();
    let center = w.bodies[b].center_of_mass();
    assert!((center.x - 10.5).abs() < 1e-5);
    for _ in 0..120 {
        w.step(DT);
        assert!(w.bodies[b].center_of_mass().distance(center) < 1e-4);
    }
    assert!(w.bodies[b].translation().distance(Vec3::X * 10.) > 0.4);
}
#[test]
fn all_ball_children_and_all_target_children_participate_in_ccd() {
    let mut w = d3::PhysicsWorld::new([0.; 3]);
    w.add_box([0.; 3], [0.01, 3., 3.], false);
    let b = w
        .insert_compound(
            d3::RigidBodyBuilder::dynamic()
                .translation(Vec3::X * -5.)
                .linvel(Vec3::X * 1000.)
                .ccd_enabled(true),
            [
                d3::ColliderBuilder::cuboid(0.1, 0.1, 0.1).translation(Vec3::X * -1.),
                d3::ColliderBuilder::ball(0.2).translation(Vec3::X),
            ],
        )
        .unwrap();
    assert!(w.step(1. / 60.).ccd_impacts > 0);
    assert!(w.bodies[b].translation().x < -1.20);
    let mut w = d3::PhysicsWorld::new([0.; 3]);
    w.insert_compound(
        d3::RigidBodyBuilder::fixed(),
        [
            d3::ColliderBuilder::cuboid(1., 1., 1.).translation(Vec3::Y * 10.),
            d3::ColliderBuilder::cuboid(0.01, 3., 3.).translation(Vec3::X),
        ],
    )
    .unwrap();
    let ball = w
        .insert(
            d3::RigidBodyBuilder::dynamic()
                .translation(Vec3::X * -5.)
                .linvel(Vec3::X * 1000.)
                .ccd_enabled(true),
            d3::ColliderBuilder::ball(0.2),
        )
        .0;
    assert!(w.step(1. / 60.).ccd_impacts > 0);
    assert!(w.bodies[ball].translation().x < 0.80);
}
#[test]
fn lifecycle_edits_preserve_origin_motion_and_invalidate_removed_children() {
    let mut w = d3::PhysicsWorld::new([0.; 3]);
    let (body, first) = w.insert(
        d3::RigidBodyBuilder::dynamic()
            .translation(Vec3::Y * 3.)
            .angvel(Vec3::Z),
        d3::ColliderBuilder::ball(0.5),
    );
    let origin = w.bodies[body].translation();
    let velocity = w.bodies[body].velocity_at_point(origin);
    let mass = w.bodies[body].mass();
    let child = w
        .attach_collider(
            body,
            d3::ColliderBuilder::ball(0.5).translation(Vec3::X * 2.),
        )
        .unwrap();
    assert!((w.bodies[body].mass() - mass * 2.).abs() < 1e-5);
    assert_eq!(w.bodies[body].translation(), origin);
    assert!(w.bodies[body].velocity_at_point(origin).distance(velocity) < 1e-5);
    w.set_collider_pose(child, Vec3::X * 4., Quat::IDENTITY)
        .unwrap();
    assert!((w.bodies[body].local_center_of_mass().x - 2.).abs() < 1e-5);
    w.remove_collider(first).unwrap();
    assert!(w.colliders.get(first).is_none());
    assert_eq!(w.bodies[body].colliders(), &[child]);
    assert_eq!(w.remove_collider(child), Err(PhysicsError::InvalidMass));
    assert!(w.colliders.get(child).is_some());
    assert!(w.remove_body(body));
    assert!(w.colliders.get(child).is_none());
}
#[test]
fn local_edits_and_density_validation_are_transactional() {
    let mut w = d2::PhysicsWorld::new([0.; 2]);
    let (b, c) = w.insert(
        d2::RigidBodyBuilder::dynamic(),
        d2::ColliderBuilder::ball(0.5),
    );
    let mass = w.bodies[b].mass();
    assert_eq!(
        w.set_collider_pose(c, Vec3::Z, Quat::IDENTITY),
        Err(PhysicsError::InvalidCollider)
    );
    assert_eq!(w.colliders[c].local_translation(), Vec3::ZERO);
    w.colliders[c].set_density(0.);
    assert_eq!(w.refresh_mass_properties(), Err(PhysicsError::InvalidMass));
    assert_eq!(w.bodies[b].mass(), mass);
    w.colliders[c].set_density(2.);
    w.refresh_mass_properties().unwrap();
    assert!((w.bodies[b].mass() - mass * 2.).abs() < 1e-5);
    let count = w.bodies.len();
    assert_eq!(
        w.insert_compound(
            d2::RigidBodyBuilder::dynamic(),
            std::iter::empty::<d2::ColliderBuilder>()
        ),
        Err(PhysicsError::InvalidMass)
    );
    assert_eq!(w.bodies.len(), count);
}
#[test]
fn overlapping_children_never_generate_self_contacts() {
    let mut w = d2::PhysicsWorld::new([0.; 2]);
    w.insert_compound(
        d2::RigidBodyBuilder::dynamic(),
        [
            d2::ColliderBuilder::cuboid(1., 1.),
            d2::ColliderBuilder::ball(1.).translation(Vec2::X * 0.2),
        ],
    )
    .unwrap();
    w.step(DT);
    assert_eq!(w.contacts().count(), 0);
    assert_eq!(w.last_step.contact_points, 0);
}
#[test]
fn child_local_rotation_is_used_by_contact_geometry() {
    let mut w = d3::PhysicsWorld::new([0., -9.81, 0.]);
    let b = w
        .insert_compound(
            d3::RigidBodyBuilder::fixed(),
            [d3::ColliderBuilder::cuboid(3., 0.1, 3.)
                .translation(Vec3::Y * 0.5)
                .rotation(Quat::from_rotation_z(0.3))],
        )
        .unwrap();
    let hit = w.raycast([0., 3., 0.], [0., -1., 0.], 10.).unwrap();
    assert_eq!(hit.body, Some(b));
    assert!(hit.normal[0] < -0.25);
    let ball = w.add_ball([0., 2., 0.], 0.2, true);
    for _ in 0..60 {
        w.step(DT);
    }
    assert!(w.bodies[ball].translation().y > 0.4);
}
#[test]
fn dynamic_capsule_uses_its_offset_center_and_stadium_inertia() {
    let mut w = d2::PhysicsWorld::new([0.; 2]);
    let r = 0.25;
    let length = 2.;
    let density = 2.;
    let (b, _) = w.insert(
        d2::RigidBodyBuilder::dynamic(),
        d2::ColliderBuilder::capsule_from_endpoints(Vec2::ZERO, Vec2::X * length, r)
            .density(density),
    );
    let mr = 2. * r * length * density;
    let ms = std::f32::consts::PI * r * r * density;
    let moment = mr * (length * length + 4. * r * r) / 12.
        + ms * (0.5 * r * r + length * length / 4. + 4. * length * r / (3. * std::f32::consts::PI));
    assert!(w.bodies[b].local_center_of_mass().distance(Vec2::X) < 1e-5);
    assert!((w.bodies[b].mass() - mr - ms).abs() < 1e-5);
    w.bodies[b].apply_torque_impulse(1., true);
    assert!((w.bodies[b].angvel() - 1. / moment).abs() < 1e-5);
}
