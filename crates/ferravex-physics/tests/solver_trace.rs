//! Golden traces pinning solver optimizations to the exact pre-optimization
//! results: a CCD-heavy 3D/2D scene (cached per-substep sweep targets, including
//! bodies corrected earlier in the same substep) and a contact/joint scene.
use ferravex_math::{Vec2, Vec3};
use ferravex_physics::{d2, d3};

fn mix(hash: &mut u64, value: f32) {
    *hash ^= u64::from(value.to_bits());
    *hash = hash.wrapping_mul(0x0100_0000_01b3);
}

fn trace_3d() -> (u64, usize) {
    let mut world = d3::PhysicsWorld::new([0., -9.81, 0.]);
    world.add_box([0., -0.5, 0.], [12., 0.5, 12.], false);
    world.add_box([4., 2., 0.], [0.05, 3., 6.], false);
    let mut bodies = Vec::new();
    for i in 0..36 {
        let f = i as f32;
        let position = Vec3::new(
            -6. + (f * 0.37) % 8.,
            0.6 + (i % 6) as f32 * 0.9,
            -3. + (i % 7) as f32,
        );
        let velocity = Vec3::new(
            140. + f * 3.,
            -20. + (i % 5) as f32 * 9.,
            (i % 3) as f32 * 30. - 30.,
        );
        let builder = d3::RigidBodyBuilder::dynamic()
            .translation(position)
            .linvel(velocity)
            .ccd_enabled(true);
        let collider = if i % 2 == 0 {
            d3::ColliderBuilder::ball(0.2 + (i % 3) as f32 * 0.05)
        } else {
            d3::ColliderBuilder::cuboid(0.15, 0.2, 0.25)
        };
        bodies.push(world.insert(builder, collider).0);
    }
    let mut hash = 0xcbf2_9ce4_8422_2325;
    let mut impacts = 0;
    for _ in 0..90 {
        impacts += world.step(1. / 60.).ccd_impacts;
        for &body in &bodies {
            let b = &world.bodies[body];
            for v in b.translation().to_array() {
                mix(&mut hash, v);
            }
            for v in b.linvel().to_array() {
                mix(&mut hash, v);
            }
        }
    }
    (hash, impacts)
}

fn trace_2d() -> (u64, usize) {
    let mut world = d2::PhysicsWorld::new([0., -9.81]);
    world.add_box([0., -0.5], [12., 0.5], false);
    world.add_box([3., 2.], [0.05, 3.], false);
    let mut bodies = Vec::new();
    for i in 0..30 {
        let f = i as f32;
        let builder = d2::RigidBodyBuilder::dynamic()
            .translation(Vec2::new(-6. + (f * 0.41) % 7., 0.5 + (i % 5) as f32 * 0.8))
            .linvel(Vec2::new(150. + f * 4., (i % 4) as f32 * 10. - 15.))
            .ccd_enabled(true);
        let collider = if i % 3 == 0 {
            d2::ColliderBuilder::cuboid(0.2, 0.15)
        } else {
            d2::ColliderBuilder::ball(0.18)
        };
        bodies.push(world.insert(builder, collider).0);
    }
    let mut hash = 0xcbf2_9ce4_8422_2325;
    let mut impacts = 0;
    for _ in 0..90 {
        impacts += world.step(1. / 60.).ccd_impacts;
        for &body in &bodies {
            let b = &world.bodies[body];
            for v in b.translation().to_array() {
                mix(&mut hash, v);
            }
        }
    }
    (hash, impacts)
}

#[test]
fn ccd_scene_matches_recorded_trace() {
    let three = trace_3d();
    let two = trace_2d();
    assert!(
        three.1 > 20 && two.1 > 20,
        "scene must exercise CCD corrections"
    );
    assert_eq!(three, (15_030_636_209_541_905_975, 2476));
    assert_eq!(two, (3_342_039_139_537_784_618, 1566));
}

/// Rotating box stacks, rolling balls, friction and joints: pins the contact
/// solver (cached lever arms) and joint velocity rows bit-for-bit.
fn trace_solver() -> u64 {
    use ferravex_physics::{JointMotor, PhysicsMaterial};
    let mut world = d3::PhysicsWorld::new([0., -9.81, 0.]);
    let ground = PhysicsMaterial {
        rolling_resistance: 0.02,
        ..Default::default()
    };
    world.insert(
        d3::RigidBodyBuilder::fixed().translation(Vec3::new(0., -0.5, 0.)),
        d3::ColliderBuilder::cuboid(20., 0.5, 20.).material(ground),
    );
    let mut bodies = Vec::new();
    for stack in 0..4 {
        for level in 0..5 {
            let position = Vec3::new(stack as f32 * 2.5 - 4., 0.5 + level as f32 * 1.02, 0.);
            let (body, _) = world.insert(
                d3::RigidBodyBuilder::dynamic()
                    .translation(position)
                    .angvel(Vec3::new(0., 0.3 * stack as f32, 0.1)),
                d3::ColliderBuilder::cuboid(0.5, 0.5, 0.5),
            );
            bodies.push(body);
        }
    }
    for i in 0..6 {
        let (body, _) = world.insert(
            d3::RigidBodyBuilder::dynamic()
                .translation(Vec3::new(i as f32 - 3., 0.4, 3.))
                .linvel(Vec3::new(1.5, 0., -2. + i as f32 * 0.4)),
            d3::ColliderBuilder::ball(0.4).material(ground),
        );
        bodies.push(body);
    }
    let anchor = world.add_box([0., 6., -3.], [0.1, 0.1, 0.1], false);
    let arm = world.add_box([1., 6., -3.], [0.8, 0.1, 0.2], true);
    world
        .insert_joint(
            anchor,
            arm,
            d3::JointBuilder::hinge().motor(JointMotor::velocity(3., 30.)),
        )
        .unwrap();
    let bob = world.add_box([0., 3.5, -3.], [0.3, 0.3, 0.3], true);
    world
        .insert_joint(arm, bob, d3::JointBuilder::distance(2.))
        .unwrap();
    bodies.extend([arm, bob]);
    let mut hash = 0xcbf2_9ce4_8422_2325;
    for _ in 0..240 {
        world.step(1. / 120.);
        for &body in &bodies {
            let b = &world.bodies[body];
            for v in b.translation().to_array() {
                mix(&mut hash, v);
            }
            for v in b.angvel().to_array() {
                mix(&mut hash, v);
            }
        }
    }
    hash
}

#[test]
fn contact_and_joint_scene_matches_recorded_trace() {
    assert_eq!(trace_solver(), 13_903_498_620_214_788_908);
}
