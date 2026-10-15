use super::*;
const DT: f32 = 1. / 120.;
fn capsule() -> CharacterShape {
    CharacterShape::Capsule {
        radius: 0.3,
        height: 1.8,
    }
}
fn floor() -> Obstacle {
    Obstacle {
        shape: ObstacleShape::Plane { normal: Vec3::Y },
        ..Obstacle::cuboid(1, Vec3::ZERO, Vec3::ONE)
    }
}
fn step(c: &mut Character, world: &[Obstacle], input: CharacterInput) -> CharacterStep {
    c.step(world, capsule(), Default::default(), input, DT)
        .unwrap()
}
#[test]
fn floor_wall_ceiling_and_long_sweeps() {
    let floor = floor();
    let wall = Obstacle::cuboid(2, Vec3::new(2., 2., 0.), Vec3::new(0.1, 2., 10.));
    let world = [floor, wall];
    let mut c = Character::new(Vec3::Y * 10.);
    for _ in 0..200 {
        step(&mut c, &world, Default::default());
    }
    assert!(c.grounded);
    assert!((c.position.y - 0.002).abs() < 1e-4);
    let m = move_and_slide(
        &world[..],
        capsule(),
        Default::default(),
        c.position,
        Vec3::new(100., 0., -3.),
        true,
    )
    .unwrap();
    assert!(m.position.x < 1.601);
    assert!(m.position.z < -2.9);
    let ceiling = Obstacle::cuboid(3, Vec3::Y * 2.2, Vec3::new(10., 0.2, 10.));
    let world = [floor, ceiling];
    let mut peak = 0f32;
    for i in 0..240 {
        step(
            &mut c,
            &world,
            CharacterInput {
                jump: i < 120,
                ..Default::default()
            },
        );
        peak = peak.max(c.position.y);
    }
    assert!(peak <= 0.201);
    assert!(c.grounded);
    assert!(c.velocity.y.abs() < 1e-4);
}
#[test]
fn box_platformer_preserves_sharp_corners_and_blocks_tunneling() {
    let shape = CharacterShape::Box2 {
        half: Vec2::new(0.3, 0.8),
    };
    let world = [
        floor(),
        Obstacle::cuboid(2, Vec3::new(2., 1., 0.), Vec3::new(0.1, 1., 1.)),
    ];
    let result = move_and_slide(
        &world[..],
        shape,
        Default::default(),
        Vec3::new(0., 0.002, 0.),
        Vec3::X * 100.,
        true,
    )
    .unwrap();
    assert!(result.position.x < 1.601);
    assert!(result.grounded);
    let result = move_and_slide(
        &world[..],
        shape,
        Default::default(),
        Vec3::new(0., 50., 0.),
        -Vec3::Y * 100.,
        false,
    )
    .unwrap();
    assert!((result.position.y - 0.002).abs() < 1e-3);
}
#[test]
fn stairs_require_ground_and_clear_headroom() {
    let mut world = [
        floor(),
        Obstacle::cuboid(2, Vec3::new(1., 0.1, 0.), Vec3::new(0.5, 0.1, 2.)),
        Obstacle::cuboid(3, Vec3::new(0., 10., 0.), Vec3::ONE),
    ];
    let mut c = Character::new(Vec3::ZERO);
    let mut stepped = false;
    for _ in 0..30 {
        stepped |= step(
            &mut c,
            &world,
            CharacterInput {
                direction: Vec3::X,
                ..Default::default()
            },
        )
        .motion
        .stepped;
    }
    assert!(stepped, "step not taken: {c:?}");
    assert!(c.position.x > 0.5);
    assert!(c.position.y > 0.19, "{c:?}");
    world[2] = Obstacle::cuboid(3, Vec3::new(1., 1.95, 0.), Vec3::new(3., 0.1, 2.));
    let mut blocked = Character::new(Vec3::ZERO);
    for _ in 0..80 {
        step(
            &mut blocked,
            &world,
            CharacterInput {
                direction: Vec3::X,
                ..Default::default()
            },
        );
    }
    assert!(blocked.position.x < 0.5);
    assert!(blocked.position.y < 0.1);
    let airborne = move_and_slide(
        &world[..2],
        capsule(),
        Default::default(),
        Vec3::new(0., 0.1, 0.),
        Vec3::X * 2.,
        false,
    )
    .unwrap();
    assert!(!airborne.stepped);
}
#[test]
fn walkable_ramps_climb_and_steep_slopes_do_not() {
    for (angle, walkable) in [(0.4, true), (1.2, false)] {
        let rotation = Quat::from_rotation_z(angle);
        let mut ramp = Obstacle::cuboid(2, Vec3::new(3., 0.4, 0.), Vec3::new(3., 0.2, 2.));
        ramp.rotation = rotation;
        let world = [floor(), ramp];
        let mut c = Character::new(Vec3::new(-1., 0., 0.));
        let mut grounded_on_ramp = 0;
        for _ in 0..120 {
            step(
                &mut c,
                &world,
                CharacterInput {
                    direction: Vec3::X,
                    ..Default::default()
                },
            );
            if c.position.y > 0.3 && c.grounded {
                grounded_on_ramp += 1;
            }
        }
        if walkable {
            assert!(
                grounded_on_ramp > 20,
                "lost slope support: {c:?}, ticks={grounded_on_ramp}"
            );
            assert!(c.position.x > 2. && c.position.y > 0.3, "ramp {c:?}");
        } else {
            assert!(c.position.y < 0.3, "climbed steep ramp: {c:?}");
        }
    }
}
#[test]
fn coyote_buffer_hold_release_and_snapshot_replay() {
    let ledge = Obstacle::cuboid(1, Vec3::new(-2., -0.2, 0.), Vec3::new(2., 0.2, 5.));
    let world = [ledge];
    let mut c = Character::new(Vec3::new(-0.1, 0.002, 0.));
    step(&mut c, &world, Default::default());
    for _ in 0..16 {
        step(
            &mut c,
            &world,
            CharacterInput {
                direction: Vec3::X,
                ..Default::default()
            },
        );
    }
    assert!(!c.grounded);
    assert!(c.jump.coyote > 0.);
    assert!(
        step(
            &mut c,
            &world,
            CharacterInput {
                jump: true,
                ..Default::default()
            }
        )
        .jumped
    );
    assert!(c.velocity.y > 8.);
    let floor = [floor()];
    let mut landing = Character::new(Vec3::Y * 0.08);
    landing.velocity.y = -2.;
    let mut jumps = 0;
    for _ in 0..240 {
        jumps += step(
            &mut landing,
            &floor,
            CharacterInput {
                jump: true,
                ..Default::default()
            },
        )
        .jumped as usize;
    }
    assert_eq!(jumps, 1, "held input must not re-jump on landing");
    assert!(landing.grounded);
    step(&mut landing, &floor, Default::default());
    assert!(
        step(
            &mut landing,
            &floor,
            CharacterInput {
                jump: true,
                ..Default::default()
            }
        )
        .jumped
    );
    let saved = landing;
    let mut replay = saved;
    for _ in 0..120 {
        let input = CharacterInput {
            direction: Vec3::Z,
            ..Default::default()
        };
        step(&mut landing, &floor, input);
        step(&mut replay, &floor, input);
    }
    assert_eq!(landing, replay);
}
#[test]
fn translating_rotating_and_removed_platforms() {
    let mut platform = Obstacle::cuboid(7, Vec3::new(0., -0.1, 0.), Vec3::new(4., 0.1, 4.));
    let mut c = Character::new(Vec3::new(1., 0.002, 0.));
    step(&mut c, &[platform], Default::default());
    let before = c.position;
    platform.position += Vec3::new(0.1, 0.05, 0.);
    platform.rotation = Quat::from_rotation_y(0.1);
    let result = step(&mut c, &[platform], Default::default());
    assert!(
        result.carried.x > 0.09 && result.carried.y > 0.049 && result.carried.z < -0.09,
        "{result:?}"
    );
    assert!(c.grounded);
    assert!(c.position.distance(before) > 0.1);
    let jumped = step(
        &mut c,
        &[platform],
        CharacterInput {
            jump: true,
            ..Default::default()
        },
    );
    assert!(jumped.jumped);
    assert!(c.support.is_none());
    let mut c = Character::new(Vec3::new(0., 0.052, 0.));
    step(&mut c, &[platform], Default::default());
    assert!(c.support.is_some());
    step(&mut c, &[], Default::default());
    assert!(c.support.is_none());
    assert!(!c.grounded);
}
#[test]
fn layers_sensors_overlap_recovery_and_failures_are_transactional() {
    let mut wall = Obstacle::cuboid(2, Vec3::new(2., 1., 0.), Vec3::new(0.1, 1., 2.));
    wall.sensor = true;
    let world = [floor(), wall];
    let mut c = Character::new(Vec3::new(0., -0.2, 0.));
    step(&mut c, &world, Default::default());
    assert!(c.position.y > 0.);
    let m = move_and_slide(
        &world[..],
        capsule(),
        Default::default(),
        c.position,
        Vec3::X * 4.,
        true,
    )
    .unwrap();
    assert!(m.position.x > 3.9);
    let saved = c;
    assert!(
        c.step(
            &world[..],
            capsule(),
            Default::default(),
            CharacterInput {
                direction: Vec3::splat(f32::NAN),
                jump: true
            },
            DT
        )
        .is_err()
    );
    assert_eq!(c, saved);
    let tiny = CharacterSettings {
        max_candidates: 1,
        ..Default::default()
    };
    assert_eq!(
        c.step(&world[..], capsule(), tiny, Default::default(), DT)
            .unwrap_err(),
        CharacterError::QueryBudget
    );
    assert_eq!(c, saved);
    let mut embedded = Character::new(Vec3::new(2., 1., 0.));
    wall.sensor = false;
    let saved = embedded;
    assert!(
        embedded
            .step(
                &[wall][..],
                capsule(),
                CharacterSettings {
                    max_recovery: 0.001,
                    ..Default::default()
                },
                Default::default(),
                DT
            )
            .is_err()
    );
    assert_eq!(embedded, saved);
}
#[cfg(feature = "dim3")]
#[test]
fn rigid_world_queries_use_current_poses_and_generational_support_ids() {
    use crate::d3::*;
    let mut world = PhysicsWorld::new([0., 0., 0.]);
    let (body, collider) = world.insert(
        RigidBodyBuilder::kinematic_position_based().translation(Vec3::new(0., -0.1, 0.)),
        ColliderBuilder::cuboid(4., 0.1, 4.),
    );
    let mut c = Character::new(Vec3::ZERO);
    c.step(
        &world.raw,
        capsule(),
        Default::default(),
        Default::default(),
        DT,
    )
    .unwrap();
    let id = c.support.unwrap().id;
    world.bodies[body].set_translation(Vec3::new(0.1, -0.1, 0.), true);
    let r = c
        .step(
            &world.raw,
            capsule(),
            Default::default(),
            Default::default(),
            DT,
        )
        .unwrap();
    assert!(r.carried.x > 0.099);
    world.remove_body(body);
    assert!(world.obstacle(id).is_none());
    let (_, new_collider) = world.insert(RigidBodyBuilder::fixed(), ColliderBuilder::ball(0.1));
    assert_ne!(collider, new_collider);
}
#[test]
fn capsule_shaft_and_all_supported_convex_obstacles_are_swept() {
    let shape = CharacterShape::Capsule {
        radius: 0.2,
        height: 3.,
    };
    let thin = Obstacle::cuboid(1, Vec3::new(2., 1.15, 0.), Vec3::new(0.01, 0.04, 1.));
    let m = move_and_slide(
        &[thin][..],
        shape,
        Default::default(),
        Vec3::ZERO,
        Vec3::X * 20.,
        false,
    )
    .unwrap();
    assert!(m.position.x < 1.791, "shaft missed thin geometry: {m:?}");
    for target in [
        ObstacleShape::Ball { radius: 0.3 },
        ObstacleShape::Capsule {
            a: Vec3::Y * -0.2,
            b: Vec3::Y * 0.2,
            radius: 0.3,
        },
        ObstacleShape::Cylinder {
            half_height: 0.3,
            radius: 0.3,
        },
    ] {
        let o = Obstacle {
            shape: target,
            ..Obstacle::cuboid(1, Vec3::new(2., 1., 0.), Vec3::ONE)
        };
        let m = move_and_slide(
            &[o][..],
            capsule(),
            Default::default(),
            Vec3::ZERO,
            Vec3::X * 20.,
            false,
        )
        .unwrap();
        assert!(m.position.x < 1.401, "missed convex obstacle: {target:?}");
    }
}
#[test]
fn planar_boxes_walk_slopes_and_stay_in_the_plane() {
    let shape = CharacterShape::Box2 {
        half: Vec2::new(0.3, 0.8),
    };
    let mut ramp = Obstacle::cuboid(2, Vec3::new(3., 0.4, 0.), Vec3::new(3., 0.2, 2.));
    ramp.rotation = Quat::from_rotation_z(0.4);
    let world = [floor(), ramp];
    let mut c = Character::new(Vec3::new(-1., 0., 0.));
    for _ in 0..100 {
        c.step(
            &world[..],
            shape,
            Default::default(),
            CharacterInput {
                direction: Vec3::X,
                ..Default::default()
            },
            DT,
        )
        .unwrap();
    }
    assert!(c.position.x > 2. && c.position.y > 0.3, "{c:?}");
    assert_eq!(c.position.z, 0.);
}
#[test]
fn jump_inherits_moving_platform_momentum() {
    let mut platform = Obstacle::cuboid(7, Vec3::new(0., -0.1, 0.), Vec3::new(4., 0.1, 4.));
    let mut c = Character::new(Vec3::new(0., 0.002, 0.));
    step(&mut c, &[platform], Default::default());
    platform.position += Vec3::new(0.1, 0.05, 0.);
    let r = step(
        &mut c,
        &[platform],
        CharacterInput {
            jump: true,
            ..Default::default()
        },
    );
    assert!(r.jumped && !c.grounded);
    assert!(c.velocity.x > 11.9 && c.velocity.y > 14.3, "{c:?}");
}

#[test]
fn layer_masks_and_malformed_slice_obstacles_are_not_silently_accepted() {
    let mut wall = Obstacle::cuboid(2, Vec3::new(2., 1., 0.), Vec3::new(0.1, 1., 2.));
    wall.membership = 2;
    let settings = CharacterSettings {
        membership_mask: 1,
        ..Default::default()
    };
    let m = move_and_slide(
        &[floor(), wall][..],
        capsule(),
        settings,
        Vec3::ZERO,
        Vec3::X * 4.,
        true,
    )
    .unwrap();
    assert!(m.position.x > 3.9);
    wall.position.x = f32::NAN;
    let mut c = Character::new(Vec3::ZERO);
    let saved = c;
    assert_eq!(
        c.step(
            &[wall][..],
            capsule(),
            Default::default(),
            Default::default(),
            DT
        )
        .unwrap_err(),
        CharacterError::InvalidObstacle
    );
    assert_eq!(c, saved);
}
