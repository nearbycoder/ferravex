use super::*;
use ferravex_math::{Mat4, Vec2};
fn follow(position: Vec3) -> Input3 {
    Input3 {
        target: Target3::at(position),
        mode: Mode3::Follow {
            eye_offset: Vec3::Z * 10.,
            look_offset: Vec3::ZERO,
        },
        fov_y: 1.,
    }
}
fn impulse() -> ShakeImpulse {
    ShakeImpulse {
        translation: Vec3::splat(0.1),
        rotation: Vec3::splat(0.01),
        duration: 1.,
        frequency: 20.,
    }
}
fn obstruction() -> Settings3 {
    Settings3 {
        position_half_life: 0.,
        target_half_life: 0.,
        obstruction: Some(ObstructionSettings {
            radius: 0.25,
            padding: 0.02,
            release_half_life: 0.2,
            sweep_motion: true,
        }),
        ..Default::default()
    }
}
#[test]
fn exponential_follow_and_fov_are_independent_of_frame_partition() {
    let mut results = Vec::new();
    for steps in [30, 60, 120, 240] {
        let mut rig = Rig3::new(Settings3 {
            position_half_life: 0.5,
            target_half_life: 0.5,
            fov_half_life: 0.5,
            ..Default::default()
        })
        .unwrap();
        rig.update(0., follow(Vec3::ZERO), &NoObstructions).unwrap();
        for _ in 0..steps {
            rig.update(
                1. / steps as f32,
                Input3 {
                    fov_y: 2.,
                    ..follow(Vec3::X * 10.)
                },
                &NoObstructions,
            )
            .unwrap();
        }
        let pose = rig.pose().unwrap();
        assert!((pose.position.x - 7.5).abs() < 0.0002);
        assert!((pose.target.x - 7.5).abs() < 0.0002);
        assert!((pose.fov_y - 1.75).abs() < 0.0001);
        results.push(pose);
    }
    for pair in results.windows(2) {
        assert!(pair[0].position.distance(pair[1].position) < 0.0002);
    }
}
#[test]
fn first_person_matches_authoritative_minus_z_aim_without_lag() {
    let mut rig = Rig3::new(Settings3 {
        position_half_life: 0.1,
        target_half_life: 5.,
        ..Default::default()
    })
    .unwrap();
    for (yaw, pitch) in [
        (0., 0.),
        (std::f32::consts::PI - 0.001, 1.55),
        (-std::f32::consts::PI + 0.001, -1.55),
        (0.5, -0.3),
    ] {
        let frame = rig
            .update(
                1. / 60.,
                Input3 {
                    target: Target3::at(Vec3::new(2., 0., 1.)),
                    mode: Mode3::FirstPerson {
                        eye_offset: Vec3::Y * 1.6,
                        yaw,
                        pitch,
                    },
                    fov_y: 1.1,
                },
                &NoObstructions,
            )
            .unwrap();
        let f = (frame.pose.target - frame.pose.position).normalize();
        assert!(f.distance(direction(yaw, pitch)) < 1e-6);
        assert!(f.dot(frame.pose.up).abs() < 1e-5);
    }
}
#[test]
fn orbit_wrap_and_poles_make_finite_orthonormal_views() {
    let mut rig = Rig3::new(Settings3 {
        position_half_life: 0.,
        target_half_life: 0.,
        ..Default::default()
    })
    .unwrap();
    let mut last = None;
    for yaw in [std::f32::consts::PI - 0.001, -std::f32::consts::PI + 0.001] {
        let frame = rig
            .update(
                1. / 60.,
                Input3 {
                    target: Target3::at(Vec3::ZERO),
                    mode: Mode3::Orbit {
                        yaw,
                        pitch: 1.569,
                        distance: 8.,
                        pivot: Vec3::Y,
                    },
                    fov_y: 1.,
                },
                &NoObstructions,
            )
            .unwrap();
        assert!((frame.pose.position.distance(frame.pose.target) - 8.).abs() < 0.001);
        assert!(
            Mat4::look_at_rh(frame.pose.position, frame.pose.target, frame.pose.up).is_finite()
        );
        if let Some(p) = last {
            assert!(frame.pose.position.distance(p) < 0.01);
        }
        last = Some(frame.pose.position);
    }
}
#[test]
fn chase_uses_vehicle_heading_speed_and_prediction_without_roll() {
    let mut rig = Rig3::default();
    let rotation = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2) * Quat::from_rotation_z(0.6);
    let frame = rig
        .update(
            1. / 60.,
            Input3 {
                target: Target3 {
                    position: Vec3::ZERO,
                    rotation,
                    velocity: Vec3::X * 10.,
                },
                mode: Mode3::Chase {
                    distance: 5.,
                    height: 2.,
                    look_ahead: 3.,
                    look_height: 1.,
                    speed_distance: 0.2,
                    speed_height: 0.1,
                    prediction: 0.1,
                    level: true,
                },
                fov_y: 1.,
            },
            &NoObstructions,
        )
        .unwrap();
    assert!(frame.pose.position.distance(Vec3::new(-6., 3., 0.)) < 0.0001);
    assert!(frame.pose.target.distance(Vec3::new(4., 1., 0.)) < 0.0001);
}
#[test]
fn dead_zones_lookahead_viewport_bounds_and_cuts_have_distinct_behavior() {
    let mut rig = Follow2::new(Follow2Settings {
        dead_zone: Vec2::splat(5.),
        half_life: 0.,
        teleport_distance: Some(50.),
        ..Default::default()
    })
    .unwrap();
    assert!(rig.update(0., Vec2::ZERO, Vec2::ZERO).unwrap().cut);
    assert_eq!(
        rig.update(1. / 60., Vec2::splat(4.), Vec2::ZERO)
            .unwrap()
            .center,
        Vec2::ZERO
    );
    assert_eq!(
        rig.update(1. / 60., Vec2::new(10., -10.), Vec2::ZERO)
            .unwrap()
            .center,
        Vec2::new(5., -5.)
    );
    assert!(
        !rig.update(1. / 60., Vec2::new(10., -10.), Vec2::ZERO)
            .unwrap()
            .cut
    );
    let settings = Follow2Settings {
        limits: Some(Limits2 {
            min: Vec2::new(20., 0.),
            max: Vec2::new(100., 0.),
        }),
        look_ahead_time: 1.,
        max_look_ahead: 10.,
        ..rig.settings()
    };
    rig.set_settings(settings).unwrap();
    assert_eq!(
        rig.update(0., Vec2::new(10., -10.), Vec2::ZERO)
            .unwrap()
            .center,
        Vec2::new(20., 0.)
    );
    let frame = rig
        .update(1. / 60., Vec2::new(80., 0.), Vec2::X * 100.)
        .unwrap();
    assert!(frame.cut);
    assert_eq!(frame.center, Vec2::new(90., 0.));
    rig.cut();
    assert!(rig.update(0., Vec2::ZERO, Vec2::ZERO).unwrap().cut);
}
#[test]
fn teleports_and_mode_switches_cut_and_clear_old_shake() {
    let mut rig = Rig3::default();
    rig.update(0., follow(Vec3::ZERO), &NoObstructions).unwrap();
    rig.shake.push(impulse()).unwrap();
    let frame = rig
        .update(1. / 60., follow(Vec3::X * 100.), &NoObstructions)
        .unwrap();
    assert!(frame.cut);
    assert_eq!(frame.pose.position, Vec3::new(100., 0., 10.));
    assert_eq!(rig.shake.active(), 0);
    let frame = rig
        .update(
            1. / 60.,
            Input3 {
                mode: Mode3::FirstPerson {
                    eye_offset: Vec3::Y,
                    yaw: 0.,
                    pitch: 0.,
                },
                ..follow(Vec3::X * 100.)
            },
            &NoObstructions,
        )
        .unwrap();
    assert!(frame.cut);
    assert_eq!(frame.pose.position, Vec3::new(100., 1., 0.));
}
#[test]
fn sphere_boom_retracts_before_a_thin_wall_and_releases_smoothly() {
    let wall = [Blocker::Box {
        center: Vec3::Z * 6.,
        half: Vec3::new(2., 2., 0.02),
        rotation: Quat::IDENTITY,
    }];
    let mut rig = Rig3::new(obstruction()).unwrap();
    let frame = rig.update(1. / 60., follow(Vec3::ZERO), &wall[..]).unwrap();
    assert!(frame.obstructed);
    assert!((frame.pose.position.z - 5.71).abs() < 0.001);
    let frame = rig
        .update(0.2, follow(Vec3::ZERO), &NoObstructions)
        .unwrap();
    assert!((frame.pose.position.z - 7.855).abs() < 0.001);
    let closer = [Blocker::Box {
        center: Vec3::Z * 3.,
        half: Vec3::new(2., 2., 0.02),
        rotation: Quat::IDENTITY,
    }];
    let frame = rig
        .update(1. / 60., follow(Vec3::ZERO), &closer[..])
        .unwrap();
    assert!(frame.pose.position.z <= 2.711);
    assert!(frame.cut && frame.recovered);
    assert!(frame.queries <= 3);
}
#[test]
fn travel_sweep_handles_corners_and_queries_after_shake() {
    let wall = [Blocker::Box {
        center: Vec3::new(0., 0., 10.),
        half: Vec3::new(0.02, 3., 1.),
        rotation: Quat::IDENTITY,
    }];
    let mut rig = Rig3::new(obstruction()).unwrap();
    rig.update(0., follow(-Vec3::X * 2.), &wall[..]).unwrap();
    rig.shake.push(impulse()).unwrap();
    let frame = rig
        .update(1. / 60., follow(Vec3::X * 2.), &wall[..])
        .unwrap();
    assert!(frame.obstructed);
    assert!(frame.queries >= 2 && frame.queries <= 3);
    let p = frame.pose.position;
    assert!(
        p.x.abs() > 0.27 || p.z < 8.75 || p.z > 11.25,
        "frame {frame:?}"
    );
}
#[test]
fn starting_inside_a_blocker_is_reported_and_still_has_a_valid_view() {
    let wall = [Blocker::Sphere {
        center: Vec3::ZERO,
        radius: 1.,
    }];
    let mut rig = Rig3::new(obstruction()).unwrap();
    let frame = rig.update(1. / 60., follow(Vec3::ZERO), &wall[..]).unwrap();
    assert!(frame.blocked_pivot);
    assert!(frame.pose.position.is_finite());
    assert!(Mat4::look_at_rh(frame.pose.position, frame.pose.target, frame.pose.up).is_finite());
}
#[test]
fn invalid_inputs_queries_and_settings_leave_complete_state_unchanged() {
    let mut rig = Rig3::new(obstruction()).unwrap();
    rig.update(0., follow(Vec3::ZERO), &NoObstructions).unwrap();
    rig.shake.push(impulse()).unwrap();
    let before = rig;
    assert!(
        rig.update(-1., follow(Vec3::ZERO), &NoObstructions)
            .is_err()
    );
    assert_eq!(before, rig);
    assert!(
        rig.update(0.1, follow(Vec3::ZERO), &|_| Ok(Some(f32::NAN)))
            .is_err()
    );
    assert_eq!(before, rig);
    assert!(
        rig.update(0.1, follow(Vec3::ZERO), &|_| Ok(Some(2.)))
            .is_err()
    );
    assert_eq!(before, rig);
    assert!(
        rig.set_settings(Settings3 {
            position_half_life: f32::NAN,
            ..rig.settings()
        })
        .is_err()
    );
    assert_eq!(before, rig);
    let bad = Input3 {
        mode: Mode3::FirstPerson {
            eye_offset: Vec3::ZERO,
            yaw: 0.,
            pitch: 2.,
        },
        ..follow(Vec3::ZERO)
    };
    assert!(rig.update(0.1, bad, &NoObstructions).is_err());
    assert_eq!(before, rig);
    let mut planar = Follow2::default();
    planar.update(0., Vec2::ZERO, Vec2::ZERO).unwrap();
    let before = planar;
    assert!(planar.update(f32::NAN, Vec2::ZERO, Vec2::ZERO).is_err());
    assert_eq!(before, planar);
}
#[test]
fn shape_adapter_supports_rotated_boxes_spheres_planes_and_bounds_work() {
    let sweep = Sweep {
        start: Vec3::ZERO,
        end: Vec3::X * 10.,
        radius: 0.25,
    };
    let box_shape = [Blocker::Box {
        center: Vec3::X * 5.,
        half: Vec3::new(0.5, 1., 2.),
        rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
    }];
    assert!((box_shape[..].sweep(sweep).unwrap().unwrap() - 0.275).abs() < 0.001);
    let ball = [Blocker::Sphere {
        center: Vec3::X * 5.,
        radius: 1.,
    }];
    assert!((ball[..].sweep(sweep).unwrap().unwrap() - 0.375).abs() < 0.001);
    let plane = [Blocker::Plane {
        normal: -Vec3::X,
        offset: -5.,
    }];
    assert!((plane[..].sweep(sweep).unwrap().unwrap() - 0.475).abs() < 0.001);
    let malformed = [Blocker::Sphere {
        center: Vec3::ZERO,
        radius: f32::NAN,
    }];
    assert_eq!(
        malformed[..].sweep(sweep),
        Err(CameraError::InvalidObstacle)
    );
    let many = vec![ball[0]; MAX_BLOCKERS + 1];
    assert_eq!(many[..].sweep(sweep), Err(CameraError::QueryBudget));
}
#[test]
fn seeded_bounded_shake_is_partition_independent_and_never_drifts_base_pose() {
    let mut samples = Vec::new();
    for steps in [30, 60, 120] {
        let mut shake = CameraShake::new(42);
        shake.push(impulse()).unwrap();
        let mut sample = ShakeSample::default();
        for _ in 0..steps {
            sample = shake.advance(0.5 / steps as f32).unwrap();
        }
        samples.push(sample);
    }
    assert!(samples[0].translation.distance(samples[2].translation) < 1e-5);
    let mut rig = Rig3::default();
    rig.update(0., follow(Vec3::ZERO), &NoObstructions).unwrap();
    let original = rig.pose().unwrap();
    rig.shake.push(impulse()).unwrap();
    for _ in 0..150 {
        let frame = rig
            .update(1. / 120., follow(Vec3::ZERO), &NoObstructions)
            .unwrap();
        assert!(frame.pose.position.distance(original.position) < 0.174);
    }
    assert_eq!(rig.shake.active(), 0);
    assert_eq!(rig.pose().unwrap(), original);
    let mut shake = CameraShake::new(42);
    for _ in 0..8 {
        shake.push(impulse()).unwrap();
    }
    let before = shake;
    assert_eq!(shake.push(impulse()), Err(CameraError::ShakeCapacity));
    assert_eq!(before, shake);
    assert!(shake.advance(f32::INFINITY).is_err());
    assert_eq!(before, shake);
    let mut rig = Rig3::new(Settings3 {
        shake_strength: 0.,
        ..Default::default()
    })
    .unwrap();
    rig.update(0., follow(Vec3::ZERO), &NoObstructions).unwrap();
    rig.shake.push(impulse()).unwrap();
    assert_eq!(
        rig.update(0.1, follow(Vec3::ZERO), &NoObstructions)
            .unwrap()
            .pose,
        original
    );
}

#[test]
fn damped_orbit_turns_around_pivot_instead_of_crossing_through_it() {
    let mut rig = Rig3::new(Settings3 {
        position_half_life: 0.1,
        ..Default::default()
    })
    .unwrap();
    let input = |yaw| Input3 {
        target: Target3::at(Vec3::ZERO),
        mode: Mode3::Orbit {
            yaw,
            pitch: 0.,
            distance: 10.,
            pivot: Vec3::ZERO,
        },
        fov_y: 1.,
    };
    rig.update(0., input(0.), &NoObstructions).unwrap();
    for _ in 0..60 {
        let frame = rig.update(1. / 60., input(3.), &NoObstructions).unwrap();
        assert!((frame.pose.position.length() - 10.).abs() < 0.001);
    }
}

#[test]
fn explicit_cut_discards_old_feedback_but_keeps_a_new_impact_queued_after_it() {
    let mut rig = Rig3::default();
    rig.update(0., follow(Vec3::ZERO), &NoObstructions).unwrap();
    rig.shake.push(impulse()).unwrap();
    rig.cut();
    assert_eq!(rig.shake.active(), 0);
    rig.shake.push(impulse()).unwrap();
    let frame = rig
        .update(1. / 60., follow(Vec3::X * 100.), &NoObstructions)
        .unwrap();
    assert!(frame.cut);
    assert_eq!(rig.shake.active(), 1);
    assert!(frame.pose.position.distance(Vec3::new(100., 0., 10.)) > 0.0001);
}

#[test]
fn planar_snap_applies_composition_and_limits_then_resumes_authored_smoothing() {
    let settings = Follow2Settings {
        offset: Vec2::new(4., 2.),
        dead_zone: Vec2::splat(3.),
        half_life: 0.25,
        look_ahead_time: 0.5,
        max_look_ahead: 4.,
        limits: Some(Limits2 {
            min: Vec2::ZERO,
            max: Vec2::new(120., 80.),
        }),
        teleport_distance: None,
    };
    let mut rig = Follow2::new(settings).unwrap();
    assert_eq!(rig.center(), None);
    rig.update(0., Vec2::splat(10.), Vec2::ZERO).unwrap();
    let frame = rig.snap(Vec2::new(100., 20.), Vec2::X * 20.).unwrap();
    assert!(frame.cut);
    assert_eq!(frame.center, Vec2::new(108., 22.));
    assert_eq!(rig.center(), Some(frame.center));
    let frame = rig
        .update(0.25, Vec2::new(110., 20.), Vec2::X * 20.)
        .unwrap();
    assert!(!frame.cut);
    assert_eq!(frame.center, Vec2::new(111.5, 22.));
    let frame = rig.snap(Vec2::new(200., 20.), Vec2::X * 20.).unwrap();
    assert!(frame.cut);
    assert_eq!(frame.center, Vec2::new(120., 22.));
    rig.reset();
    assert_eq!(rig.center(), None);
    assert_eq!(rig.settings(), settings);
    let frame = rig.update(0., Vec2::splat(3.), Vec2::ZERO).unwrap();
    assert!(frame.cut);
    assert_eq!(frame.center, Vec2::new(7., 5.));
    assert!(!rig.update(0., Vec2::splat(3.), Vec2::ZERO).unwrap().cut);
}

#[test]
fn planar_dead_zone_smoothing_is_partition_independent_after_snap() {
    for steps in [1, 30, 60, 120] {
        let mut rig = Follow2::new(Follow2Settings {
            half_life: 0.25,
            dead_zone: Vec2::splat(2.),
            teleport_distance: None,
            ..Default::default()
        })
        .unwrap();
        rig.snap(Vec2::ZERO, Vec2::ZERO).unwrap();
        for _ in 0..steps {
            let frame = rig
                .update(0.25 / steps as f32, Vec2::X * 10., Vec2::ZERO)
                .unwrap();
            assert!(!frame.cut);
        }
        assert!(rig.center().unwrap().distance(Vec2::X * 4.) < 0.0001);
    }
}

#[test]
fn chase_snap_replaces_heading_speed_prediction_and_fov_then_smooths() {
    let settings = Settings3 {
        position_half_life: 0.25,
        target_half_life: 0.25,
        fov_half_life: 0.25,
        teleport_distance: None,
        ..Default::default()
    };
    let mut input = Input3 {
        target: Target3::at(Vec3::ZERO),
        mode: Mode3::Chase {
            distance: 5.,
            height: 2.,
            look_ahead: 3.,
            look_height: 1.,
            speed_distance: 0.2,
            speed_height: 0.1,
            prediction: 0.1,
            level: true,
        },
        fov_y: 1.,
    };
    let mut rig = Rig3::new(settings).unwrap();
    rig.update(0., input, &NoObstructions).unwrap();
    rig.shake.push(impulse()).unwrap();
    input.target = Target3 {
        position: Vec3::X * 100.,
        rotation: Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        velocity: Vec3::X * 10.,
    };
    input.fov_y = 2.;
    let frame = rig.snap(input, &NoObstructions).unwrap();
    assert!(frame.cut);
    assert!(frame.pose.position.distance(Vec3::new(94., 3., 0.)) < 0.0001);
    assert!(frame.pose.target.distance(Vec3::new(104., 1., 0.)) < 0.0001);
    assert_eq!(frame.pose.fov_y, 2.);
    assert_eq!(rig.shake.active(), 0);
    assert_eq!(rig.settings(), settings);
    assert_eq!(rig.pose(), Some(frame.pose));
    input.target.position.x += 8.;
    input.fov_y = 1.;
    for steps in [1, 30, 60, 120] {
        let mut smoothed = rig;
        for _ in 0..steps {
            let frame = smoothed
                .update(0.25 / steps as f32, input, &NoObstructions)
                .unwrap();
            assert!(!frame.cut);
        }
        let pose = smoothed.pose().unwrap();
        assert!(pose.position.distance(Vec3::new(98., 3., 0.)) < 0.0002);
        assert!(pose.target.distance(Vec3::new(108., 1., 0.)) < 0.0002);
        assert!((pose.fov_y - 1.5).abs() < 0.0001);
    }
}

#[test]
fn snap_checks_the_new_boom_without_sweeping_across_a_teleport() {
    let mut rig = Rig3::new(obstruction()).unwrap();
    rig.snap(follow(-Vec3::X * 100.), &NoObstructions).unwrap();
    let queries = std::cell::Cell::new(0);
    let frame = rig
        .snap(follow(Vec3::ZERO), &|sweep: Sweep| {
            queries.set(queries.get() + 1);
            assert_eq!(sweep.start, Vec3::ZERO);
            assert_eq!(sweep.end, Vec3::Z * 10.);
            Ok(Some(0.5))
        })
        .unwrap();
    assert!(frame.cut && frame.obstructed && !frame.recovered);
    assert_eq!(queries.get(), 1);
    assert_eq!(frame.queries, 1);
    assert!((frame.pose.position.z - 4.98).abs() < 0.0001);
    let frame = rig
        .update(0.2, follow(Vec3::ZERO), &NoObstructions)
        .unwrap();
    assert!(!frame.cut);
    assert!((frame.pose.position.z - 7.49).abs() < 0.0001);
    let frame = rig.snap(follow(Vec3::ZERO), &NoObstructions).unwrap();
    assert!(frame.cut);
    assert_eq!(frame.pose.position, Vec3::Z * 10.);
}

#[test]
fn reset_forgets_orbit_and_obstruction_history_without_replacing_tuning_or_shake_seed() {
    let settings = Settings3 {
        position_half_life: 0.25,
        target_half_life: 0.25,
        ..obstruction()
    };
    let mut rig = Rig3::new(settings).unwrap();
    rig.shake = CameraShake::new(42);
    let mut input = Input3 {
        mode: Mode3::Orbit {
            yaw: 0.5,
            pitch: 0.2,
            distance: 8.,
            pivot: Vec3::Y,
        },
        ..follow(Vec3::ZERO)
    };
    rig.snap(input, &|_| Ok(Some(0.25))).unwrap();
    rig.shake.push(impulse()).unwrap();
    let mut expected_shake = rig.shake;
    expected_shake.clear();
    rig.reset();
    assert_eq!(rig.pose(), None);
    assert_eq!(rig.settings(), settings);
    assert_eq!(rig.shake, expected_shake);
    input.target.position = Vec3::X * 100.;
    input.fov_y = 2.;
    input.mode = Mode3::Orbit {
        yaw: -2.,
        pitch: -0.1,
        distance: 12.,
        pivot: Vec3::Y,
    };
    let frame = rig.update(0., input, &NoObstructions).unwrap();
    let mut fresh = Rig3::new(settings).unwrap();
    assert_eq!(frame, fresh.update(0., input, &NoObstructions).unwrap());
    assert!(frame.cut);
    assert!(!rig.update(0., input, &NoObstructions).unwrap().cut);
    rig.shake.push(impulse()).unwrap();
    rig.reset();
    assert_eq!(rig.shake.active(), 0);
    rig.shake.push(impulse()).unwrap();
    let frame = rig.update(0.01, input, &NoObstructions).unwrap();
    assert!(frame.cut);
    assert_eq!(rig.shake.active(), 1);
}

#[test]
fn failed_snaps_preserve_pose_shake_and_pending_cuts() {
    let mut planar = Follow2::default();
    planar.snap(Vec2::X * 10., Vec2::ZERO).unwrap();
    for pending_cut in [false, true] {
        if pending_cut {
            planar.cut();
        }
        let before = planar;
        for (subject, velocity) in [
            (Vec2::splat(f32::NAN), Vec2::ZERO),
            (Vec2::ZERO, Vec2::splat(f32::INFINITY)),
        ] {
            assert_eq!(
                planar.snap(subject, velocity),
                Err(CameraError::InvalidInput)
            );
            assert_eq!(planar, before);
        }
    }
    let mut rig = Rig3::new(obstruction()).unwrap();
    rig.snap(follow(Vec3::ZERO), &NoObstructions).unwrap();
    for pending_cut in [false, true] {
        if pending_cut {
            rig.cut();
        }
        rig.shake.push(impulse()).unwrap();
        let before = rig;
        let bad = Input3 {
            fov_y: f32::NAN,
            ..follow(Vec3::X * 100.)
        };
        assert_eq!(
            rig.snap(bad, &NoObstructions),
            Err(CameraError::InvalidInput)
        );
        assert_eq!(rig, before);
        for result in [
            Ok(Some(f32::NAN)),
            Ok(Some(2.)),
            Err(CameraError::QueryBudget),
        ] {
            assert!(rig.snap(follow(Vec3::X * 100.), &|_| result).is_err());
            assert_eq!(rig, before);
        }
    }
    assert!(
        rig.update(0., follow(Vec3::ZERO), &NoObstructions)
            .unwrap()
            .cut
    );
    assert_eq!(rig.shake.active(), 1);
}

#[test]
fn oversized_and_invalid_dt_preserve_initialization_and_pending_discontinuities() {
    let mut planar = Follow2::default();
    let mut rig = Rig3::default();
    for initialized in [false, true] {
        if initialized {
            planar.snap(Vec2::ZERO, Vec2::ZERO).unwrap();
            rig.snap(follow(Vec3::ZERO), &NoObstructions).unwrap();
            planar.cut();
            rig.cut();
        }
        rig.shake.push(impulse()).unwrap();
        let planar_before = planar;
        let rig_before = rig;
        for dt in [
            -f32::EPSILON,
            f32::NEG_INFINITY,
            f32::NAN,
            f32::INFINITY,
            0.250001,
            10.,
            f32::MAX,
        ] {
            assert_eq!(
                planar.update(dt, Vec2::X * 5., Vec2::ZERO),
                Err(CameraError::InvalidInput)
            );
            assert_eq!(planar, planar_before);
            assert_eq!(
                rig.update(dt, follow(Vec3::X * 5.), &NoObstructions),
                Err(CameraError::InvalidInput)
            );
            assert_eq!(rig, rig_before);
        }
    }
    assert!(planar.update(0., Vec2::X * 5., Vec2::ZERO).unwrap().cut);
    assert!(
        rig.update(0., follow(Vec3::X * 5.), &NoObstructions)
            .unwrap()
            .cut
    );
}

#[test]
fn zero_time_freezes_damping_even_with_zero_half_life_but_explicit_snaps_move() {
    let mut planar = Follow2::new(Follow2Settings {
        half_life: 0.,
        ..Default::default()
    })
    .unwrap();
    let mut rig = Rig3::new(Settings3 {
        position_half_life: 0.,
        target_half_life: 0.,
        fov_half_life: 0.,
        ..Default::default()
    })
    .unwrap();
    let center = planar.update(0., Vec2::ZERO, Vec2::ZERO).unwrap().center;
    let pose = rig
        .update(0., follow(Vec3::ZERO), &NoObstructions)
        .unwrap()
        .pose;
    let moved = Input3 {
        fov_y: 2.,
        ..follow(Vec3::X * 5.)
    };
    assert_eq!(
        planar.update(0., Vec2::X * 5., Vec2::ZERO).unwrap().center,
        center
    );
    assert_eq!(rig.update(0., moved, &NoObstructions).unwrap().pose, pose);
    let planar_snap = planar.snap(Vec2::X * 5., Vec2::ZERO).unwrap();
    let rig_snap = rig.snap(moved, &NoObstructions).unwrap();
    assert!(planar_snap.cut && rig_snap.cut);
    assert_eq!(planar_snap.center, Vec2::X * 5.);
    assert_eq!(rig_snap.pose.position, Vec3::new(5., 0., 10.));
    assert_eq!(rig_snap.pose.fov_y, 2.);
    assert_eq!(
        planar
            .update(f32::EPSILON, Vec2::ZERO, Vec2::ZERO)
            .unwrap()
            .center,
        center
    );
    assert_eq!(
        rig.update(f32::EPSILON, follow(Vec3::ZERO), &NoObstructions)
            .unwrap()
            .pose,
        pose
    );
}
