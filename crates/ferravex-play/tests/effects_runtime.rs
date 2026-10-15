use ferravex_play::particles::*;
fn near(a: f32, b: f32) {
    assert!((a - b).abs() < 0.0001, "{a} != {b}");
}
fn sample(x: f32) -> TrailSample {
    TrailSample {
        position: [x, 0., 0.],
        color: [1.; 4],
        width: 0.2,
    }
}
#[test]
fn analytic_drag_acceleration_rotation_and_frame_partition_agree() {
    let desc = ParticleDesc {
        velocity: [10., 2., 0.],
        acceleration: [0., -3., 0.],
        lifetime: 10.,
        ..Default::default()
    };
    let motion = ParticleMotion {
        drag: 2.,
        rotation: 0.3,
        angular_velocity: 2.,
    };
    let mut a = ParticlePool::new(1);
    let mut b = ParticlePool::new(1);
    a.spawn_with(desc, motion).unwrap();
    b.spawn_with(desc, motion).unwrap();
    a.update(1.).unwrap();
    for _ in 0..100 {
        b.update(0.01).unwrap();
    }
    let (a, b) = (a.iter().next().unwrap(), b.iter().next().unwrap());
    for i in 0..3 {
        near(a.position[i], b.position[i]);
        near(a.velocity[i], b.velocity[i]);
    }
    near(a.position[0], 5. * (1. - (-2_f32).exp()));
    near(a.velocity[0], 10. * (-2_f32).exp());
    near(a.rotation, 2.3);
    near(a.color[3], 0.9);
}
#[test]
fn tiny_drag_is_continuous_and_invalid_updates_do_not_mutate() {
    let mut pool = ParticlePool::new(2);
    let desc = ParticleDesc {
        velocity: [1., 2., 3.],
        acceleration: [0., -1., 0.],
        lifetime: 2.,
        ..Default::default()
    };
    pool.spawn(desc).unwrap();
    pool.spawn_with(
        desc,
        ParticleMotion {
            drag: 1e-8,
            ..Default::default()
        },
    )
    .unwrap();
    pool.update(0.5).unwrap();
    let v = pool.iter().collect::<Vec<_>>();
    for i in 0..3 {
        near(v[0].position[i], v[1].position[i]);
    }
    assert!(pool.update(f32::NAN).is_err());
    assert_eq!(pool.iter().collect::<Vec<_>>(), v);
    assert!(ParticlePool::try_new(MAX_PARTICLES + 1).is_err());
    assert!(
        pool.spawn_aged(desc, ParticleMotion::default(), 2.)
            .is_err()
    );
}
fn emitter(rate: f32, budget: u16, seed: u64) -> Emitter {
    Emitter::new(
        EmitterDesc {
            particle: ParticleDesc {
                velocity: [2., 0., 0.],
                lifetime: 3.,
                ..Default::default()
            },
            rate,
            max_per_update: budget,
            velocity_jitter: [1., 2., 1.],
            ..Default::default()
        },
        seed,
    )
    .unwrap()
}
#[test]
fn moving_emitters_place_births_and_ages_consistently_across_frame_partitions() {
    let mut a = emitter(4., 64, 7);
    let mut b = emitter(4., 64, 7);
    let mut pa = ParticlePool::new(16);
    let mut pb = ParticlePool::new(16);
    a.teleport(EmitterTransform::default()).unwrap();
    b.teleport(EmitterTransform::default()).unwrap();
    pa.update(1.).unwrap();
    a.update(1., EmitterTransform::at([10., 0., 0.]), &mut pa)
        .unwrap();
    for i in 1..=10 {
        pb.update(0.1).unwrap();
        b.update(0.1, EmitterTransform::at([i as f32, 0., 0.]), &mut pb)
            .unwrap();
    }
    assert_eq!(pa.len(), 4);
    assert_eq!(pb.len(), 4);
    assert_eq!(a.sequence(), b.sequence());
    for (a, b) in pa.iter().zip(pb.iter()) {
        for i in 0..3 {
            near(a.position[i], b.position[i]);
            near(a.velocity[i], b.velocity[i]);
        }
        near(a.normalized_age, b.normalized_age);
    }
}
#[test]
fn saturation_is_bounded_counted_and_does_not_change_later_counter_seeded_samples() {
    let mut a = emitter(10., 8, 9);
    let mut b = a.clone();
    let mut pa = ParticlePool::new(2);
    let mut pb = ParticlePool::new(8);
    let sa = a.burst(100, EmitterTransform::default(), &mut pa).unwrap();
    let sb = b.burst(100, EmitterTransform::default(), &mut pb).unwrap();
    assert_eq!(sa.spawned, 2);
    assert_eq!(sa.dropped_budget, 92);
    assert_eq!(sa.dropped_capacity, 6);
    assert_eq!(sb.spawned, 8);
    assert_eq!(
        pa.iter().collect::<Vec<_>>(),
        pb.iter().skip(6).collect::<Vec<_>>()
    );
    pa.clear();
    pb.clear();
    a.burst(1, EmitterTransform::default(), &mut pa).unwrap();
    b.burst(1, EmitterTransform::default(), &mut pb).unwrap();
    assert_eq!(pa.iter().next(), pb.iter().next());
    let mut empty = ParticlePool::new(0);
    let s = a
        .burst(u32::MAX, EmitterTransform::default(), &mut empty)
        .unwrap();
    assert_eq!(s.requested, u32::MAX as u64);
    assert_eq!(s.spawned, 0);
    assert_eq!(s.dropped(), s.requested);
}
#[test]
fn shape_cone_transforms_and_expired_births_are_finite() {
    let mut e = Emitter::new(
        EmitterDesc {
            particle: ParticleDesc {
                lifetime: 0.1,
                ..Default::default()
            },
            shape: SpawnShape::Sphere { radius: 2. },
            velocity_cone: Some(DirectionCone {
                axis: [0., 1., 0.],
                angle: 0.,
                speed: [3., 3.],
            }),
            rate: 1.,
            ..Default::default()
        },
        11,
    )
    .unwrap();
    let mut pool = ParticlePool::new(32);
    e.burst(16, EmitterTransform::at([10., 0., 0.]), &mut pool)
        .unwrap();
    for p in pool.iter() {
        let d = p
            .position
            .iter()
            .enumerate()
            .map(|(i, v)| (*v - if i == 0 { 10. } else { 0. }).powi(2))
            .sum::<f32>()
            .sqrt();
        assert!(d <= 2.0001);
        near(p.velocity[1], 3.);
    }
    pool.clear();
    let stats = e
        .update(2.5, EmitterTransform::default(), &mut pool)
        .unwrap();
    assert_eq!(stats.requested, 2);
    assert_eq!(stats.expired, 2);
    assert_eq!(stats.spawned, 0);
    let sequence = e.sequence();
    assert!(
        e.update(-1., EmitterTransform::default(), &mut pool)
            .is_err()
    );
    assert_eq!(e.sequence(), sequence);
    assert!(e.set_rate(f32::INFINITY).is_err());
}
#[test]
fn trails_resample_motion_fade_stop_and_break_on_teleport() {
    let config = TrailConfig {
        capacity: 8,
        spacing: 1.,
        lifetime: 2.,
        teleport_distance: 10.,
        ..Default::default()
    };
    let mut trail = Trail::new(config).unwrap();
    trail.update(0., Some(sample(0.))).unwrap();
    trail.update(1., Some(sample(3.))).unwrap();
    assert!(trail.len() <= 8);
    let points = trail.iter().collect::<Vec<_>>();
    assert_eq!(points[0].position, [0.; 3]);
    near(points[0].color[3], 0.5);
    assert!(points.iter().any(|p| p.position[0] == 2.));
    let before = trail.iter().collect::<Vec<_>>();
    assert!(trail.update(f32::NAN, None).is_err());
    assert_eq!(trail.iter().collect::<Vec<_>>(), before);
    trail.update(0.1, Some(sample(100.))).unwrap();
    assert_eq!(trail.stats().teleports, 1);
    let triangles = RibbonTriangles::new(
        &trail,
        RibbonFacing::Plane {
            normal: [0., 0., 1.],
        },
        2.,
    )
    .unwrap()
    .collect::<Vec<_>>();
    assert!(triangles.iter().all(|t| {
        let p = t.vertices.map(|v| v.position[0]);
        p.into_iter().fold(f32::NEG_INFINITY, f32::max)
            - p.into_iter().fold(f32::INFINITY, f32::min)
            < 10.
    }));
    trail.update(3., None).unwrap();
    assert!(trail.is_empty());
}
#[test]
fn trail_ring_and_sampling_work_stay_bounded() {
    let mut trail = Trail::new(TrailConfig {
        capacity: 4,
        spacing: 0.01,
        teleport_distance: 100.,
        max_per_update: 4,
        ..Default::default()
    })
    .unwrap();
    trail.update(0., Some(sample(0.))).unwrap();
    let stats = trail.update(0.1, Some(sample(50.))).unwrap();
    assert!(stats.dropped_budget >= 4995);
    assert!(trail.len() <= trail.capacity());
    for i in 51..1000 {
        trail.update(0.01, Some(sample(i as f32))).unwrap();
        assert!(trail.len() <= 4);
    }
    assert!(trail.stats().overwritten > 0);
    trail.clear();
    assert!(trail.is_empty());
    assert!(
        Trail::new(TrailConfig {
            spacing: 0.,
            ..Default::default()
        })
        .is_err()
    );
}
#[test]
fn ribbon_miters_degeneracy_breaks_and_camera_alignment_are_bounded() {
    let p = |x, y, break_before| RibbonPoint {
        position: [x, y, 0.],
        color: [1.; 4],
        width: 1.,
        break_before,
    };
    let points = [
        p(0., 0., true),
        p(1., 0., false),
        p(1., 0., false),
        p(0., 0.001, false),
        p(100., 0., true),
        p(101., 0., false),
    ];
    for facing in [
        RibbonFacing::Plane {
            normal: [0., 0., 1.],
        },
        RibbonFacing::Camera {
            eye: [-5., 0., 0.],
            up: [0., 1., 0.],
        },
    ] {
        let triangles = RibbonTriangles::new(&points, facing, 2.)
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(triangles.len(), 6);
        for v in triangles.iter().flat_map(|t| t.vertices) {
            assert!(v.position.into_iter().chain(v.uv).all(f32::is_finite));
            assert!(v.position[1].abs() < 2.);
        }
    }
    let mut path = RibbonPath::new(6).unwrap();
    path.set(&points).unwrap();
    let original = path.points().to_vec();
    let mut bad = points;
    bad[1].width = f32::NAN;
    assert!(path.set(&bad).is_err());
    assert_eq!(path.points(), original);
    assert!(RibbonTriangles::new(&path, RibbonFacing::Plane { normal: [0.; 3] }, 1.).is_err());
}
#[test]
fn beams_expire_fade_and_replace_the_oldest_with_stable_ties() {
    let mut beams = BeamPool::new(2, OverflowPolicy::ReplaceOldest);
    let desc = |x| BeamDesc {
        start: [x, 0., 0.],
        lifetime: 1.,
        ..Default::default()
    };
    beams.spawn(desc(1.)).unwrap();
    beams.update(0.25).unwrap();
    beams.spawn(desc(2.)).unwrap();
    beams.spawn(desc(3.)).unwrap();
    assert_eq!(beams.replaced(), 1);
    assert!(!beams.iter().any(|p| p.start[0] == 1.));
    beams.update(0.5).unwrap();
    for b in beams.iter() {
        near(b.start_color[3], 0.5);
    }
    beams.update(0.5).unwrap();
    assert!(beams.is_empty());
    let mut newest = BeamPool::new(1, OverflowPolicy::DropNewest);
    newest.spawn(desc(1.)).unwrap();
    assert_eq!(newest.spawn(desc(2.)), Err(BeamError::Full));
    assert_eq!(newest.dropped(), 1);
    assert!(newest.update(-1.).is_err());
    assert!(BeamPool::try_new(MAX_BEAMS + 1, OverflowPolicy::DropNewest).is_err());
}
#[test]
fn built_in_templates_validate_and_produce_bounded_original_samples() {
    for desc in [
        presets::sparks([1., 0.6, 0.1, 1.]),
        presets::dust([0.7, 0.6, 0.4, 0.5]),
        presets::exhaust([0.7, 0.8, 1., 0.4]),
    ] {
        let mut e = Emitter::new(desc, 42).unwrap();
        let mut p = ParticlePool::new(128);
        assert_eq!(
            e.burst(16, EmitterTransform::default(), &mut p)
                .unwrap()
                .spawned,
            16
        );
        p.update(0.1).unwrap();
        assert!(!p.is_empty());
    }
}
#[test]
fn rotating_a_nozzle_does_not_rotate_world_gravity() {
    let mut emitter = Emitter::new(
        EmitterDesc {
            particle: ParticleDesc {
                velocity: [1., 0., 0.],
                acceleration: [0., -2., 0.],
                lifetime: 2.,
                ..Default::default()
            },
            rate: 0.,
            ..Default::default()
        },
        0,
    )
    .unwrap();
    let mut pool = ParticlePool::new(1);
    emitter
        .burst(
            1,
            EmitterTransform {
                rotation: ferravex_math::Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)
                    .to_array(),
                ..Default::default()
            },
            &mut pool,
        )
        .unwrap();
    pool.update(1.).unwrap();
    let p = pool.iter().next().unwrap();
    near(p.position[0], 0.);
    near(p.position[1], 0.);
    near(p.velocity[1], -1.);
}
