use ferravex_dynamics::*;
const DT: f32 = 1. / 120.;
fn axis<const D: usize>(index: usize, value: f32) -> [f32; D] {
    let mut p = [0.; D];
    p[index] = value;
    p
}
fn floor<const D: usize>(world: &mut PhysicsWorld<D>) -> BodyHandle {
    let mut half = [20.; D];
    half[1] = 0.5;
    world
        .insert(BodyDesc::fixed(Shape::cuboid(half), axis(1, -0.5)))
        .unwrap()
}
fn step<const D: usize>(world: &mut PhysicsWorld<D>, count: usize) {
    for _ in 0..count {
        world.step(DT).unwrap();
    }
}
fn resting_stack<const D: usize>() {
    let mut w = PhysicsWorld::<D>::new(axis(1, -9.81));
    floor(&mut w);
    let mut boxes = Vec::new();
    for i in 0..6 {
        boxes.push(
            w.insert(BodyDesc::dynamic(
                Shape::cuboid([0.5; D]),
                axis(1, 0.5 + i as f32),
                1.,
            ))
            .unwrap(),
        );
    }
    step(&mut w, 1200);
    for (i, h) in boxes.iter().enumerate() {
        let b = w.body(*h).unwrap();
        assert!(
            (b.position()[1] - (0.5 + i as f32)).abs() < 0.06,
            "body {i} at {:?}",
            b.position()
        );
        assert!(
            b.velocity()[1].abs() < 0.12,
            "body {i} velocity {:?}",
            b.velocity()
        );
    }
    assert_eq!(w.contacts().len(), 6);
}
#[test]
fn stacks_settle_in_2d_and_3d() {
    resting_stack::<2>();
    resting_stack::<3>();
}
fn ball_rest<const D: usize>() {
    let mut w = PhysicsWorld::<D>::new(axis(1, -9.81));
    floor(&mut w);
    let ball = w
        .insert(BodyDesc::dynamic(Shape::ball(0.4), axis(1, 4.), 2.))
        .unwrap();
    step(&mut w, 600);
    assert!((w.body(ball).unwrap().position()[1] - 0.4).abs() < 0.01);
    assert!(w.body(ball).unwrap().velocity()[1].abs() < 0.01);
}
#[test]
fn circles_and_spheres_land_on_boxes() {
    ball_rest::<2>();
    ball_rest::<3>();
}
#[test]
fn impulses_respect_mass_forces_are_one_step_and_statics_do_not_move() {
    let mut w = PhysicsWorld::<2>::new([0., 0.]);
    let ball = w
        .insert(BodyDesc::dynamic(Shape::ball(0.4), [0., 0.], 2.))
        .unwrap();
    let fixed = w
        .insert(BodyDesc::fixed(Shape::ball(0.4), [50., 0.]))
        .unwrap();
    w.body_mut(ball).unwrap().apply_impulse([4., 0.]).unwrap();
    w.body_mut(ball).unwrap().apply_force([0., 24.]).unwrap();
    w.body_mut(fixed).unwrap().apply_impulse([4., 0.]).unwrap();
    w.step(DT).unwrap();
    assert_eq!(w.body(ball).unwrap().velocity()[0], 2.);
    assert!((w.body(ball).unwrap().velocity()[1] - 0.1).abs() < 1e-6);
    w.step(DT).unwrap();
    assert_eq!(w.body(ball).unwrap().velocity()[0], 2.);
    assert!((w.body(ball).unwrap().velocity()[1] - 0.1).abs() < 1e-6);
    assert_eq!(w.body(fixed).unwrap().position(), [50., 0.]);
    assert!(w.body_mut(fixed).unwrap().set_velocity([1., 0.]).is_err());
}
#[test]
fn removing_and_reusing_slot_never_revives_old_handle() {
    let mut w = PhysicsWorld::<3>::new([0.; 3]);
    let old = w
        .insert(BodyDesc::dynamic(Shape::ball(1.), [0.; 3], 1.))
        .unwrap();
    assert!(w.remove(old).is_some());
    assert!(w.remove(old).is_none());
    let new = w
        .insert(BodyDesc::dynamic(Shape::ball(1.), [0.; 3], 1.))
        .unwrap();
    assert_eq!(old.index(), new.index());
    assert_ne!(old.generation(), new.generation());
    assert!(w.body(old).is_none());
    assert!(w.body_mut(old).is_none());
    assert_eq!(w.len(), 1);
}
#[test]
fn sensor_events_are_transitions_and_removal_ends_contact_once() {
    let mut w = PhysicsWorld::<2>::new([0.; 2]);
    let mut sensor = BodyDesc::fixed(Shape::cuboid([1.; 2]), [0.; 2]);
    sensor.sensor = true;
    let s = w.insert(sensor).unwrap();
    let ball = w
        .insert(BodyDesc::dynamic(Shape::ball(0.5), [0.; 2], 1.))
        .unwrap();
    w.step(DT).unwrap();
    assert_eq!(w.events().len(), 1);
    assert_eq!(w.events()[0].kind, ContactEventKind::Started);
    assert!(w.events()[0].contact.sensor);
    assert_eq!(w.body(ball).unwrap().position(), [0.; 2]);
    w.step(DT).unwrap();
    assert!(w.events().is_empty());
    w.body_mut(ball).unwrap().set_position([4., 0.]).unwrap();
    w.step(DT).unwrap();
    assert_eq!(w.events()[0].kind, ContactEventKind::Stopped);
    w.body_mut(ball).unwrap().set_position([0.; 2]).unwrap();
    w.step(DT).unwrap();
    w.remove(s).unwrap();
    w.step(DT).unwrap();
    assert_eq!(w.events().len(), 1);
    assert_eq!(w.events()[0].kind, ContactEventKind::Stopped);
    w.step(DT).unwrap();
    assert!(w.events().is_empty());
}
#[test]
fn collision_layers_apply_symmetrically_to_contacts_and_queries() {
    let mut w = PhysicsWorld::<2>::new([0.; 2]);
    let mut a = BodyDesc::dynamic(Shape::ball(1.), [0.; 2], 1.);
    a.layers = CollisionLayers::new(1, 1);
    let mut b = BodyDesc::dynamic(Shape::ball(1.), [0.5, 0.], 1.);
    b.layers = CollisionLayers::new(2, 2);
    let aa = w.insert(a).unwrap();
    let bb = w.insert(b).unwrap();
    w.step(DT).unwrap();
    assert!(w.contacts().is_empty());
    let filter = QueryFilter {
        layers: CollisionLayers::new(1, 1),
        ..QueryFilter::default()
    };
    assert_eq!(
        w.raycast([-4., 0.], [10., 0.], 10., filter).unwrap().body,
        aa
    );
    w.body_mut(bb)
        .unwrap()
        .set_layers(CollisionLayers::new(1, 1));
    w.step(DT).unwrap();
    assert_eq!(w.contacts().len(), 1);
}
#[test]
fn rays_are_normalized_nearest_filtered_and_follow_transform_changes() {
    let mut w = PhysicsWorld::<3>::new([0.; 3]);
    let box_id = w
        .insert(BodyDesc::fixed(Shape::cuboid([1.; 3]), [5., 0., 0.]))
        .unwrap();
    let ball = w
        .insert(BodyDesc::fixed(Shape::ball(0.5), [2., 0., 0.]))
        .unwrap();
    let hit = w
        .raycast([0.; 3], [100., 0., 0.], 20., QueryFilter::default())
        .unwrap();
    assert_eq!(hit.body, ball);
    assert!((hit.distance - 1.5).abs() < 1e-5);
    assert_eq!(hit.normal, [-1., 0., 0.]);
    let filtered = QueryFilter {
        exclude: Some(ball),
        ..QueryFilter::default()
    };
    assert!(
        (w.raycast([0.; 3], [1., 0., 0.], 20., filtered)
            .unwrap()
            .distance
            - 4.)
            .abs()
            < 1e-5
    );
    w.body_mut(box_id)
        .unwrap()
        .set_position([8., 0., 0.])
        .unwrap();
    assert!(
        (w.raycast([0.; 3], [1., 0., 0.], 20., filtered)
            .unwrap()
            .distance
            - 7.)
            .abs()
            < 1e-5
    );
    assert!(w.raycast([0.; 3], [0.; 3], 20., filtered).is_none());
    assert!(
        w.raycast([0.; 3], [1., 0., 0.], f32::NAN, filtered)
            .is_none()
    );
}
fn fast_ball<const D: usize>() {
    let mut w = PhysicsWorld::<D>::new([0.; D]);
    let mut half = [10.; D];
    half[0] = 0.025;
    w.insert(BodyDesc::fixed(Shape::cuboid(half), axis(0, 0.)))
        .unwrap();
    let mut desc = BodyDesc::dynamic(Shape::ball(0.1), axis(0, -5.), 1.);
    desc.velocity = axis(0, 1000.);
    desc.ccd = true;
    let ball = w.insert(desc).unwrap();
    let stats = w.step(1. / 60.).unwrap();
    assert!(stats.ccd_impacts > 0);
    let body = w.body(ball).unwrap();
    assert!(body.position()[0] <= -0.124, "{:?}", body.position());
    assert!(body.velocity()[0].abs() < 0.001);
}
#[test]
fn ccd_stops_fast_circle_and_sphere_at_thin_terrain() {
    fast_ball::<2>();
    fast_ball::<3>();
}
#[test]
fn ball_cast_does_not_invent_collisions_at_expanded_box_corners() {
    let mut w = PhysicsWorld::<2>::new([0.; 2]);
    w.insert(BodyDesc::fixed(Shape::cuboid([1.; 2]), [0.; 2]))
        .unwrap();
    // This segment lies inside the inflated AABB but outside the rounded corner.
    assert!(
        w.cast_ball([-1.45, 1.45], 0.5, [1., 0.], 0.1, QueryFilter::default())
            .is_none()
    );
    let hit = w
        .cast_ball([-3., 1.3], 0.5, [1., 0.], 10., QueryFilter::default())
        .unwrap();
    assert!((hit.distance - 1.6).abs() < 1e-5, "{hit:?}");
    assert!((hit.normal[0] + 0.8).abs() < 1e-5);
    assert!((hit.normal[1] - 0.6).abs() < 1e-5);
}
#[test]
fn ball_cast_hits_3d_edge_and_corner_with_correct_normals() {
    let mut w = PhysicsWorld::<3>::new([0.; 3]);
    w.insert(BodyDesc::fixed(Shape::cuboid([1.; 3]), [0.; 3]))
        .unwrap();
    let edge = w
        .cast_ball(
            [-3., 1.3, 0.],
            0.5,
            [1., 0., 0.],
            10.,
            QueryFilter::default(),
        )
        .unwrap();
    assert!((edge.distance - 1.6).abs() < 1e-5);
    let corner = w
        .cast_ball(
            [-3., 1.2, 1.2],
            0.5,
            [1., 0., 0.],
            10.,
            QueryFilter::default(),
        )
        .unwrap();
    assert!(
        (corner.distance - (2. - 0.17_f32.sqrt())).abs() < 1e-5,
        "{corner:?}"
    );
    assert!((corner.normal[1] - 0.4).abs() < 1e-5 && (corner.normal[2] - 0.4).abs() < 1e-5);
}
#[test]
fn casts_leave_touching_surface_when_moving_away() {
    let mut w = PhysicsWorld::<2>::new([0.; 2]);
    w.insert(BodyDesc::fixed(Shape::cuboid([1.; 2]), [0.; 2]))
        .unwrap();
    assert!(
        w.cast_ball([-1.5, 0.], 0.5, [-1., 0.], 3., QueryFilter::default())
            .is_none()
    );
    assert!(
        w.cast_ball([-1.5, 0.], 0.5, [0., 1.], 3., QueryFilter::default())
            .is_none()
    );
}
#[test]
fn restitution_bounces_and_friction_slows_horizontal_motion() {
    let mut w = PhysicsWorld::<2>::new([0., -9.81]);
    floor(&mut w);
    let mut desc = BodyDesc::dynamic(Shape::ball(0.5), [0., 0.6], 1.);
    desc.velocity = [4., -3.];
    desc.restitution = 0.8;
    desc.friction = 1.;
    let ball = w.insert(desc).unwrap();
    let mut bounced = false;
    for _ in 0..120 {
        w.step(DT).unwrap();
        bounced |= w.body(ball).unwrap().velocity()[1] > 2.;
    }
    assert!(bounced);
    assert!(w.body(ball).unwrap().velocity()[0].abs() < 0.5);
}
#[test]
fn kinematic_platform_pushes_dynamic_without_responding() {
    let mut w = PhysicsWorld::<2>::new([0.; 2]);
    let mut platform = BodyDesc::kinematic(Shape::cuboid([1.; 2]), [-2., 0.]);
    platform.velocity = [2., 0.];
    let k = w.insert(platform).unwrap();
    let b = w
        .insert(BodyDesc::dynamic(Shape::ball(0.5), [0.; 2], 1.))
        .unwrap();
    step(&mut w, 120);
    assert!((w.body(k).unwrap().position()[0]).abs() < 0.001);
    assert_eq!(w.body(k).unwrap().velocity(), [2., 0.]);
    assert!(w.body(b).unwrap().position()[0] > 1.49);
}
#[test]
fn pair_overflow_rolls_back_all_motion_and_preserves_force() {
    let mut config = WorldConfig::new([0.; 2]);
    config.max_pairs = 1;
    let mut w = PhysicsWorld::<2>::with_config(config).unwrap();
    let a = w
        .insert(BodyDesc::dynamic(Shape::ball(1.), [0.; 2], 1.))
        .unwrap();
    let b = w
        .insert(BodyDesc::dynamic(Shape::ball(1.), [0.; 2], 1.))
        .unwrap();
    let c = w
        .insert(BodyDesc::dynamic(Shape::ball(1.), [0.; 2], 1.))
        .unwrap();
    w.body_mut(a).unwrap().apply_force([120., 0.]).unwrap();
    assert_eq!(w.step(DT), Err(PhysicsError::PairCapacity));
    assert_eq!(w.body(a).unwrap().position(), [0.; 2]);
    assert_eq!(w.body(a).unwrap().velocity(), [0.; 2]);
    assert!(w.events().is_empty());
    w.remove(b);
    w.remove(c);
    w.step(DT).unwrap();
    assert_eq!(w.body(a).unwrap().velocity(), [1., 0.]);
}
#[test]
fn broadphase_has_explicit_work_budget_in_degenerate_scenes() {
    let mut config = WorldConfig::new([0.; 3]);
    config.max_pair_tests = 3;
    let mut w = PhysicsWorld::<3>::with_config(config).unwrap();
    for _ in 0..10 {
        w.insert(BodyDesc::dynamic(Shape::ball(1.), [0.; 3], 1.))
            .unwrap();
    }
    assert_eq!(w.step(DT), Err(PhysicsError::PairCapacity));
}
#[test]
fn separated_bodies_do_not_generate_quadratic_candidate_work() {
    let mut w = PhysicsWorld::<3>::new([0.; 3]);
    for i in 0..2000 {
        w.insert(BodyDesc::dynamic(
            Shape::ball(0.25),
            [i as f32 * 2., 0., 0.],
            1.,
        ))
        .unwrap();
    }
    let stats = w.step(DT).unwrap();
    assert_eq!(stats.candidate_pairs, 0);
    assert_eq!(stats.broadphase_tests, 0);
}
#[test]
fn validates_dimensions_shapes_timestep_capacity_and_nonfinite_state() {
    assert!(matches!(
        PhysicsWorld::<1>::with_config(WorldConfig::new([0.])),
        Err(PhysicsError::InvalidDimension)
    ));
    let mut config = WorldConfig::new([0.; 2]);
    config.max_bodies = 1;
    let mut w = PhysicsWorld::<2>::with_config(config).unwrap();
    assert_eq!(
        w.insert(BodyDesc::dynamic(Shape::ball(-1.), [0.; 2], 1.)),
        Err(PhysicsError::InvalidBody)
    );
    let b = w
        .insert(BodyDesc::dynamic(Shape::ball(1.), [0.; 2], 1.))
        .unwrap();
    assert_eq!(
        w.insert(BodyDesc::dynamic(Shape::ball(1.), [0.; 2], 1.)),
        Err(PhysicsError::BodyCapacity)
    );
    assert_eq!(w.step(f32::NAN), Err(PhysicsError::InvalidTimestep));
    assert_eq!(w.step(0.), Err(PhysicsError::InvalidTimestep));
    assert_eq!(
        w.body_mut(b).unwrap().set_position([f32::INFINITY, 0.]),
        Err(PhysicsError::InvalidBody)
    );
    assert_eq!(
        w.body_mut(b).unwrap().apply_impulse([f32::NAN, 0.]),
        Err(PhysicsError::InvalidBody)
    );
}

#[test]
fn character_slides_along_wall_and_snaps_to_ground() {
    let mut w = PhysicsWorld::<3>::new([0.; 3]);
    floor(&mut w);
    w.insert(BodyDesc::fixed(Shape::cuboid([0.2, 2., 5.]), [2., 2., 0.]))
        .unwrap();
    let settings = CharacterSettings {
        radius: 0.5,
        ..CharacterSettings::default()
    };
    let result = w
        .move_character(
            [0., 0.54, 0.],
            [3., -0.1, 1.],
            settings,
            QueryFilter::default(),
        )
        .unwrap();
    assert!(result.grounded && result.blocked);
    assert!(
        result.position[0] < 1.301 && result.position[0] > 1.28,
        "{result:?}"
    );
    assert!(result.position[2] > 0.98, "{result:?}");
    assert!((result.position[1] - 0.502).abs() < 0.005);
}
#[test]
fn character_climbs_small_step_but_not_tall_wall() {
    let mut w = PhysicsWorld::<2>::new([0.; 2]);
    floor(&mut w);
    w.insert(BodyDesc::fixed(Shape::cuboid([1., 0.125]), [2., 0.125]))
        .unwrap();
    let settings = CharacterSettings {
        radius: 0.5,
        step_height: 0.3,
        min_ground_normal: 0.95,
        ..CharacterSettings::default()
    };
    let result = w
        .move_character([0., 0.502], [2., -0.01], settings, QueryFilter::default())
        .unwrap();
    assert!(result.stepped && result.grounded, "{result:?}");
    assert!(
        result.position[0] > 1.99 && (result.position[1] - 0.752).abs() < 0.01,
        "{result:?}"
    );
    w.insert(BodyDesc::fixed(Shape::cuboid([0.2, 2.]), [4., 2.]))
        .unwrap();
    let blocked = w
        .move_character(
            result.position,
            [3., -0.01],
            settings,
            QueryFilter::default(),
        )
        .unwrap();
    assert!(
        blocked.position[0] < 3.301 && !blocked.stepped,
        "{blocked:?}"
    );
}
#[test]
fn elastic_sphere_collision_preserves_linear_momentum() {
    let mut w = PhysicsWorld::<3>::new([0.; 3]);
    let mut a = BodyDesc::dynamic(Shape::ball(0.5), [-0.6, 0., 0.], 1.);
    a.velocity = [2., 0., 0.];
    a.restitution = 1.;
    a.friction = 0.;
    let mut b = BodyDesc::dynamic(Shape::ball(0.5), [0.6, 0., 0.], 1.);
    b.velocity = [-2., 0., 0.];
    b.restitution = 1.;
    b.friction = 0.;
    let a = w.insert(a).unwrap();
    let b = w.insert(b).unwrap();
    w.step(0.1).unwrap();
    assert_eq!(w.body(a).unwrap().velocity(), [-2., 0., 0.]);
    assert_eq!(w.body(b).unwrap().velocity(), [2., 0., 0.]);
}
