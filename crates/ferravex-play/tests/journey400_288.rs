use ferravex_math::{
    Quat, Vec3,
    curves::{CubicBezier3, TessellationOptions, TessellationStatus},
    projection::NdcDepth,
};
use ferravex_play::{
    camera::{
        Pose3,
        rail::*,
        transition::{Perspective3, TransitionError, ViewHandedness},
    },
    navigation::{NavigationError, Route3},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

fn lens() -> Perspective3 {
    Perspective3 {
        aspect: 1.,
        near: 0.1,
        far: 100.,
        handedness: ViewHandedness::Right,
        depth: NdcDepth::ZeroToOne,
    }
}
fn key(x: f32) -> RailKey3 {
    RailKey3 {
        pose: Pose3 {
            position: Vec3::X * x,
            target: Vec3::X * x - Vec3::Z * 4.,
            ..Pose3::default()
        },
        projection: lens(),
    }
}
fn curve(a: f32, b: f32) -> CubicBezier3 {
    CubicBezier3::new([
        Vec3::X * a,
        Vec3::X * (a * 2. / 3. + b / 3.),
        Vec3::X * (a / 3. + b * 2. / 3.),
        Vec3::X * b,
    ])
    .unwrap()
}
fn moving(a: f32, b: f32, seconds: f64) -> RailSegment3<'static> {
    RailSegment3::new(RailPath3::Cubic(curve(a, b)), key(a), key(b), seconds).unwrap()
}
fn close(a: Vec3, b: Vec3) {
    assert!(a.distance(b) < 3e-5, "{a:?} != {b:?}");
}

#[test]
fn start_end_overshoot_repeated_time_and_backward_seek_are_explicit() {
    let segments = [moving(0., 8., 2.)];
    let rail = Rail3::new(&segments, RailMode::Once).unwrap();
    assert_eq!(rail.duration(), 2.);
    let start = rail.sample_at(-0., false).unwrap();
    assert!(start.cut && !start.finished && !start.looped);
    assert_eq!(start.pose, key(0.).pose);
    let middle = rail.sample_at(0.5, false).unwrap();
    close(middle.pose.position, Vec3::X * 2.);
    assert!(!middle.cut && !middle.finished);
    let end = rail.sample_at(2., false).unwrap();
    assert_eq!(end.pose, key(8.).pose);
    assert!(end.finished && !end.cut);
    assert_eq!(rail.sample_at(f64::MAX, false).unwrap(), end);
    assert_eq!(rail.sample_at(0.5, false).unwrap(), middle);
    let mut player = RailPlayer3::new(rail);
    assert_eq!(player.displayed(), None);
    assert!(player.sample_at(0.5, false).unwrap().cut);
    assert_eq!(player.generation(), 1);
    assert!(!player.sample_at(0.5, false).unwrap().cut);
    assert_eq!(player.generation(), 1);
    player.sample_at(1., false).unwrap();
    assert!(player.sample_at(0.5, false).unwrap().cut);
    assert_eq!(player.generation(), 2);
}

#[test]
fn exact_boundaries_are_right_continuous_and_skipped_cuts_are_not_lost() {
    let mut b = key(20.);
    b.projection.handedness = ViewHandedness::Left;
    b.projection.depth = NdcDepth::NegativeOneToOne;
    let mut c = b;
    c.pose.position.x = 24.;
    c.pose.target.x = 24.;
    let segments = [
        moving(0., 4., 1.),
        RailSegment3::new(RailPath3::Cut, key(4.), b, 0.).unwrap(),
        RailSegment3::new(RailPath3::Cubic(curve(20., 24.)), b, c, 1.).unwrap(),
    ];
    let rail = Rail3::new(&segments, RailMode::Once).unwrap();
    assert!(
        rail.sample_at(1_f64.next_down(), false)
            .unwrap()
            .pose
            .position
            .x
            <= 4.
    );
    let boundary = rail.sample_at(1., false).unwrap();
    assert_eq!(boundary.segment, 2);
    assert_eq!(boundary.pose, b.pose);
    assert_eq!(boundary.projection, b.projection);
    assert!(boundary.cut && !boundary.finished);
    assert!(!rail.sample_at(1_f64.next_up(), false).unwrap().cut);
    let mut player = RailPlayer3::new(rail);
    player.sample_at(0.5, false).unwrap();
    let skipped = player.sample_at(1.5, false).unwrap();
    assert!(skipped.cut);
    close(skipped.pose.position, Vec3::X * 22.);
    assert_eq!(player.generation(), 2);
    assert!(!player.sample_at(1.5, false).unwrap().cut);
}

#[test]
fn ordinary_boundary_does_not_cut_and_cut_chains_select_last_key() {
    let segments = [moving(0., 4., 1.), moving(4., 8., 1.)];
    let rail = Rail3::new(&segments, RailMode::Once).unwrap();
    assert_eq!(rail.sample_at(1., false).unwrap().pose, key(4.).pose);
    assert!(!rail.sample_at(1., false).unwrap().cut);
    let cuts = [
        RailSegment3::new(RailPath3::Cut, key(0.), key(4.), 0.).unwrap(),
        RailSegment3::new(RailPath3::Cut, key(4.), key(8.), 0.).unwrap(),
    ];
    let rail = Rail3::new(&cuts, RailMode::Once).unwrap();
    for t in [0., 1., f64::MAX] {
        let sample = rail.sample_at(t, false).unwrap();
        assert_eq!(sample.pose, key(8.).pose);
        assert!(sample.cut && sample.finished);
        assert_eq!(sample.segment, 1);
    }
    assert!(matches!(
        Rail3::new(&cuts, RailMode::Loop),
        Err(RailError::InvalidTimeline)
    ));
}

#[test]
fn loop_queries_have_absolute_cycles_and_bounded_large_seeks() {
    let segments = [moving(0., 8., 2.)];
    let rail = Rail3::new(&segments, RailMode::Loop).unwrap();
    for cycle in [1_u64, 3, 1_000_000, (1_u64 << 52) - 1] {
        let sample = rail.sample_at(cycle as f64 * 2., false).unwrap();
        assert_eq!(sample.cycle, cycle);
        assert_eq!(sample.pose, key(0.).pose);
        assert!(sample.cut && sample.looped && !sample.finished);
    }
    let sample = rail.sample_at(7., false).unwrap();
    assert_eq!(sample.cycle, 3);
    assert!(!sample.looped && !sample.cut);
    close(sample.pose.position, Vec3::X * 4.);
    let mut player = RailPlayer3::new(rail);
    player.sample_at(0.25, false).unwrap();
    let sample = player.sample_at(1000000.5, false).unwrap();
    assert!(sample.cut && sample.looped);
    assert_eq!(sample.cycle, 500000);
    assert!(!player.sample_at(1000000.75, false).unwrap().looped);
    assert!(player.sample_at(0.5, false).unwrap().cut);
    assert_eq!(
        rail.sample_at(2_f64.powi(54), false),
        Err(RailError::NumericalLimit)
    );
}

#[test]
fn nonbinary_loop_duration_keeps_cycle_and_remainder_consistent() {
    let segments = [moving(0., 8., 0.1)];
    let rail = Rail3::new(&segments, RailMode::Loop).unwrap();
    let near_end = rail.sample_at(1., false).unwrap();
    assert_eq!(near_end.cycle, 9);
    assert!(!near_end.looped && !near_end.cut);
    assert!(near_end.pose.position.x > 7.99);
    let next = rail.sample_at(1_f64.next_up(), false).unwrap();
    assert_eq!(next.cycle, 10);
    assert!(next.pose.position.x < 0.01);
    let boundary = rail.sample_at(0.1, false).unwrap();
    assert_eq!(boundary.cycle, 1);
    assert!(boundary.looped && boundary.cut);
}

#[test]
fn authored_shortest_arc_roll_pitch_distance_fov_and_nonunit_up_survive() {
    let authored = |position: Vec3, rotation: Quat, distance, fov_y| RailKey3 {
        pose: Pose3 {
            position,
            target: position + rotation * Vec3::NEG_Z * distance,
            up: rotation * Vec3::Y * 3.,
            fov_y,
        },
        projection: lens(),
    };
    let a = authored(
        Vec3::ZERO,
        Quat::from_rotation_y(170_f32.to_radians()),
        2.,
        0.5,
    );
    let b = authored(
        Vec3::X * 8.,
        Quat::from_rotation_y(-170_f32.to_radians()),
        6.,
        2.5,
    );
    let segments = [RailSegment3::new(RailPath3::Cubic(curve(0., 8.)), a, b, 1.).unwrap()];
    let rail = Rail3::new(&segments, RailMode::Once).unwrap();
    assert_eq!(rail.sample_at(0., false).unwrap().pose, a.pose);
    assert_eq!(rail.sample_at(1., false).unwrap().pose, b.pose);
    let p = rail.sample_at(0.5, false).unwrap().pose;
    close((p.target - p.position).normalize(), Vec3::Z);
    close(p.up, Vec3::Y);
    assert!((p.target.distance(p.position) - 4.).abs() < 1e-5);
    assert_eq!(p.fov_y, 1.5);
    let rotation = Quat::from_rotation_z(1.2) * Quat::from_rotation_x(0.7);
    let a = authored(Vec3::ZERO, rotation, 3., 1.);
    let b = authored(Vec3::X * 8., rotation, 3., 1.);
    let segments = [RailSegment3::new(RailPath3::Cubic(curve(0., 8.)), a, b, 1.).unwrap()];
    let p = Rail3::new(&segments, RailMode::Once)
        .unwrap()
        .sample_at(0.5, false)
        .unwrap()
        .pose;
    close((p.target - p.position).normalize(), rotation * Vec3::NEG_Z);
    close(p.up, rotation * Vec3::Y);
}

#[test]
fn shared_half_turn_ties_and_all_projection_conventions_are_preserved() {
    use ferravex_play::camera::transition::Transition3;
    for depth in [
        NdcDepth::ZeroToOne,
        NdcDepth::NegativeOneToOne,
        NdcDepth::ReverseZeroToOne,
        NdcDepth::ReverseNegativeOneToOne,
    ] {
        for handedness in [ViewHandedness::Right, ViewHandedness::Left] {
            for rotation in [
                Quat::from_xyzw(0., 1., 0., 0.),
                Quat::from_xyzw(0., 0., 1., 0.),
            ] {
                let mut a = key(0.);
                a.projection.depth = depth;
                a.projection.handedness = handedness;
                let b = RailKey3 {
                    pose: Pose3 {
                        target: rotation * Vec3::NEG_Z * 4.,
                        up: rotation * Vec3::Y,
                        ..a.pose
                    },
                    ..a
                };
                let segments =
                    [RailSegment3::new(RailPath3::Cubic(curve(0., 0.)), a, b, 2.).unwrap()];
                let rail = Rail3::new(&segments, RailMode::Once).unwrap();
                let mut expected = Transition3::new();
                expected.cut(a.pose, a.projection).unwrap();
                expected.retarget(b.pose, b.projection, 0.25).unwrap();
                let expected = expected.advance(0.125).unwrap().unwrap();
                let observed = rail.sample_at(1., false).unwrap();
                assert_eq!(observed.pose, expected);
                assert_eq!(observed.projection, a.projection);
                let expected = Quat::IDENTITY
                    .try_rotate_towards(rotation, std::f32::consts::FRAC_PI_2)
                    .unwrap();
                close(observed.pose.up, expected * Vec3::Y);
                close(
                    (observed.pose.target - observed.pose.position).normalize(),
                    expected * Vec3::NEG_Z,
                );
            }
        }
    }
}

#[test]
fn finite_subnormal_duration_loops_and_overflowing_loop_counts_are_checked() {
    let duration = f64::from_bits(1);
    let segments = [moving(0., 0., duration)];
    let rail = Rail3::new(&segments, RailMode::Loop).unwrap();
    let sample = rail.sample_at(duration, false).unwrap();
    assert_eq!(sample.cycle, 1);
    assert!(sample.looped && sample.cut);
    assert_eq!(rail.sample_at(1., false), Err(RailError::NumericalLimit));
    assert_eq!(rail.sample_at(1., true).unwrap().pose, key(0.).pose);
}

#[test]
fn cubic_parameter_is_not_route_arc_distance_and_orientation_is_not_tangent() {
    let cubic = CubicBezier3::new([Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, Vec3::X * 8.]).unwrap();
    let direct = [RailSegment3::new(RailPath3::Cubic(cubic), key(0.), key(8.), 1.).unwrap()];
    let direct = Rail3::new(&direct, RailMode::Once)
        .unwrap()
        .sample_at(0.5, false)
        .unwrap();
    close(direct.pose.position, Vec3::X);
    let mut route = Route3::new(3).unwrap();
    route.set(&[Vec3::ZERO, Vec3::X, Vec3::X * 8.]).unwrap();
    let segments = [RailSegment3::new(RailPath3::Route(&route), key(0.), key(8.), 1.).unwrap()];
    let arc = Rail3::new(&segments, RailMode::Once)
        .unwrap()
        .sample_at(0.5, false)
        .unwrap();
    close(arc.pose.position, Vec3::X * 4.);
    close(
        (arc.pose.target - arc.pose.position).normalize(),
        Vec3::NEG_Z,
    );
}

#[test]
fn closed_repeated_and_stationary_routes_reuse_existing_sampling() {
    let mut route = Route3::new(4).unwrap();
    route
        .set_closed(&[Vec3::ZERO, Vec3::ZERO, Vec3::X * 4.])
        .unwrap();
    let segments = [RailSegment3::new(RailPath3::Route(&route), key(0.), key(0.), 2.).unwrap()];
    let rail = Rail3::new(&segments, RailMode::Once).unwrap();
    close(
        rail.sample_at(1., false).unwrap().pose.position,
        Vec3::X * 4.,
    );
    assert_eq!(rail.sample_at(2., false).unwrap().pose, key(0.).pose);
    for closed in [false, true] {
        if closed {
            route.set_closed(&[Vec3::ZERO]).unwrap();
        } else {
            route.set(&[Vec3::ZERO; 2]).unwrap();
        }
        let segments = [RailSegment3::new(RailPath3::Route(&route), key(0.), key(0.), 2.).unwrap()];
        assert_eq!(
            Rail3::new(&segments, RailMode::Once)
                .unwrap()
                .sample_at(1., false)
                .unwrap()
                .pose,
            key(0.).pose
        );
    }
}

#[test]
fn hold_discards_supplied_time_resume_rebases_reset_and_reduced_motion_cuts() {
    let segments = [moving(0., 8., 2.)];
    for mode in [RailMode::Once, RailMode::Loop] {
        let mut player = RailPlayer3::new(Rail3::new(&segments, mode).unwrap());
        assert!(player.hold());
        assert!(!player.hold());
        let visible = player.sample_at(0.5, false).unwrap();
        assert!(player.is_held());
        assert_eq!(player.sample_at(100., false).unwrap(), visible);
        assert_eq!(
            player.sample_at(f64::NAN, false),
            Err(RailError::InvalidTime)
        );
        assert_eq!(player.displayed(), Some(visible));
        assert!(player.resume());
        assert!(!player.resume());
        assert_eq!(player.sample_at(0.5, false).unwrap().pose, visible.pose);
        player.hold();
        let cut = player.sample_at(0.75, true).unwrap();
        assert!(cut.cut && cut.finished && !cut.looped);
        assert_eq!(cut.pose, key(8.).pose);
        assert!(!player.is_held());
        let generation = player.generation();
        player.sample_at(0.75, true).unwrap();
        assert_eq!(player.generation(), generation);
        assert!(player.sample_at(0.75, false).unwrap().cut);
        player.hold();
        player.reset();
        assert_eq!(player.displayed(), None);
        assert!(!player.is_held());
        assert!(player.sample_at(0., false).unwrap().cut);
        assert_eq!(player.displayed().unwrap().pose, key(0.).pose);
    }
}

#[test]
fn batches_preserve_destination_on_late_error_capacity_and_numeric_failure() {
    let segments = [moving(0., 8., 2.)];
    let rail = Rail3::new(&segments, RailMode::Once).unwrap();
    let sentinel = rail.sample_at(0., false).unwrap();
    let mut output = [sentinel; 4];
    assert_eq!(
        rail.sample_into(&[0.5, f64::NAN], false, &mut output),
        Err(RailError::InvalidTime)
    );
    assert_eq!(output, [sentinel; 4]);
    assert_eq!(
        rail.sample_into(&[0.; 5], false, &mut output),
        Err(RailError::Capacity)
    );
    assert_eq!(
        rail.sample_into(&[0.; MAX_RAIL_SAMPLES + 1], false, &mut output),
        Err(RailError::Capacity)
    );
    assert_eq!(output, [sentinel; 4]);
    assert_eq!(rail.sample_into(&[], false, &mut []), Ok(0));
    assert_eq!(
        rail.sample_into(&[0.5, 2., 0.25], false, &mut output),
        Ok(3)
    );
    assert_eq!(output[3], sentinel);
    assert_eq!(output[0], rail.sample_at(0.5, false).unwrap());
    let mut max = [sentinel; MAX_RAIL_SAMPLES];
    assert_eq!(
        rail.sample_into(&[1.; MAX_RAIL_SAMPLES], false, &mut max),
        Ok(MAX_RAIL_SAMPLES)
    );

    let mut tiny = key(0.);
    tiny.pose.target = Vec3::X * 0.01;
    let bow = CubicBezier3::new([Vec3::ZERO, Vec3::X * 1e6, Vec3::X * 1e6, Vec3::ZERO]).unwrap();
    let segments = [RailSegment3::new(RailPath3::Cubic(bow), tiny, tiny, 1.).unwrap()];
    let rail = Rail3::new(&segments, RailMode::Once).unwrap();
    assert_eq!(rail.sample_at(0.5, false), Err(RailError::NumericalLimit));
    let before = output;
    assert_eq!(
        rail.sample_into(&[0., 0.5, 1.], false, &mut output),
        Err(RailError::NumericalLimit)
    );
    assert_eq!(output, before);
    let mut player = RailPlayer3::new(rail);
    let visible = player.sample_at(0., false).unwrap();
    let generation = player.generation();
    assert_eq!(player.sample_at(0.5, false), Err(RailError::NumericalLimit));
    assert_eq!(player.displayed(), Some(visible));
    assert_eq!(player.generation(), generation);
    assert_eq!(player.sample_at(1., false).unwrap().pose, tiny.pose);
}

#[test]
fn invalid_authoring_projection_path_timeline_and_capacity_are_rejected() {
    for t in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1., 3600.00001] {
        assert!(matches!(
            RailSegment3::new(RailPath3::Cubic(curve(0., 8.)), key(0.), key(8.), t),
            Err(RailError::InvalidTime)
        ));
    }
    for (path, t) in [(RailPath3::Cut, 1.), (RailPath3::Cubic(curve(0., 8.)), 0.)] {
        assert!(matches!(
            RailSegment3::new(path, key(0.), key(8.), t),
            Err(RailError::InvalidCut)
        ));
    }
    let mut bad = key(8.);
    bad.pose.target = bad.pose.position;
    assert!(matches!(
        RailSegment3::new(RailPath3::Cut, key(0.), bad, 0.),
        Err(RailError::Transition(TransitionError::InvalidPose))
    ));
    bad = key(8.);
    bad.projection.near = f32::NAN;
    assert!(matches!(
        RailSegment3::new(RailPath3::Cut, key(0.), bad, 0.),
        Err(RailError::Transition(TransitionError::InvalidProjection))
    ));
    for change in 0..6 {
        let mut b = key(8.);
        match change {
            0 => b.projection.aspect = 2.,
            1 => b.projection.near = 0.2,
            2 => b.projection.far = 200.,
            3 => b.projection.handedness = ViewHandedness::Left,
            4 => b.projection.depth = NdcDepth::NegativeOneToOne,
            _ => b.projection.depth = NdcDepth::ReverseZeroToOne,
        }
        assert!(matches!(
            RailSegment3::new(RailPath3::Cubic(curve(0., 8.)), key(0.), b, 1.),
            Err(RailError::Transition(
                TransitionError::IncompatibleProjection
            ))
        ));
        assert!(RailSegment3::new(RailPath3::Cut, key(0.), b, 0.).is_ok());
    }
    let route = Route3::new(2).unwrap();
    assert!(matches!(
        RailSegment3::new(RailPath3::Route(&route), key(0.), key(8.), 1.),
        Err(RailError::InvalidPath)
    ));
    assert!(matches!(
        RailSegment3::new(RailPath3::Cubic(curve(0., 8.)), key(1.), key(8.), 1.),
        Err(RailError::InvalidPath)
    ));
    let outside =
        CubicBezier3::new([Vec3::ZERO, Vec3::X * 1000001., Vec3::ZERO, Vec3::X * 8.]).unwrap();
    assert!(matches!(
        RailSegment3::new(RailPath3::Cubic(outside), key(0.), key(8.), 1.),
        Err(RailError::InvalidPath)
    ));
    assert!(matches!(
        Rail3::new(&[], RailMode::Once),
        Err(RailError::Capacity)
    ));
    let cut = RailSegment3::new(RailPath3::Cut, key(0.), key(0.), 0.).unwrap();
    let max = [cut; MAX_RAIL_SEGMENTS];
    assert!(Rail3::new(&max, RailMode::Once).is_ok());
    let over = [cut; MAX_RAIL_SEGMENTS + 1];
    assert!(matches!(
        Rail3::new(&over, RailMode::Once),
        Err(RailError::Capacity)
    ));
    let disconnected = [moving(0., 4., 1.), moving(5., 8., 1.)];
    assert!(matches!(
        Rail3::new(&disconnected, RailMode::Once),
        Err(RailError::Disconnected)
    ));
    let lost = [moving(0., 0., 3600.), moving(0., 0., f64::from_bits(1))];
    assert!(matches!(
        Rail3::new(&lost, RailMode::Once),
        Err(RailError::InvalidTimeline)
    ));
    let segments = [moving(0., 8., 1.)];
    let rail = Rail3::new(&segments, RailMode::Once).unwrap();
    for t in [
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        -f64::MIN_POSITIVE,
    ] {
        for reduced in [false, true] {
            assert_eq!(rail.sample_at(t, reduced), Err(RailError::InvalidTime));
        }
    }
}

fn options() -> TessellationOptions {
    TessellationOptions {
        tolerance: 0.001,
        max_depth: 16,
        max_work: 1023,
    }
}
#[test]
fn curve_route_adapter_never_accepts_a_partial_prefix_and_retains_last_good() {
    let bow = CubicBezier3::new([
        Vec3::ZERO,
        Vec3::Y * 4.,
        Vec3::X * 4. + Vec3::Y * 4.,
        Vec3::X * 4.,
    ])
    .unwrap();
    let mut route = Route3::new(128).unwrap();
    route.set_closed(&[Vec3::X * 20., Vec3::X * 24.]).unwrap();
    let before = route.points().to_vec();
    let length = route.length();
    let mut scratch = [Vec3::ONE; 128];
    for opts in [
        TessellationOptions {
            max_work: 1,
            ..options()
        },
        TessellationOptions {
            max_depth: 0,
            ..options()
        },
        TessellationOptions {
            max_work: 0,
            ..options()
        },
    ] {
        assert!(matches!(
            tessellate_route_into(bow, &mut route, &mut scratch, opts),
            Err(RailError::IncompleteCurve(_))
        ));
        assert_eq!(route.points(), before);
        assert_eq!(route.length(), length);
        assert!(route.is_closed());
    }
    for n in 0..2 {
        assert!(matches!(
            tessellate_route_into(bow, &mut route, &mut scratch[..n], options()),
            Err(RailError::IncompleteCurve(TessellationStatus::OutputFull))
        ));
        assert_eq!(route.points(), before);
    }
    assert!(matches!(
        tessellate_route_into(
            bow,
            &mut route,
            &mut scratch,
            TessellationOptions {
                tolerance: f64::NAN,
                ..options()
            }
        ),
        Err(RailError::Curve(_))
    ));
    assert_eq!(route.points(), before);
    assert_eq!(
        tessellate_route_into(
            bow,
            &mut route,
            &mut scratch,
            TessellationOptions {
                max_work: MAX_RAIL_TESSELLATION_WORK + 1,
                ..options()
            }
        ),
        Err(RailError::Capacity)
    );
    let mut too_large = [Vec3::ZERO; 4097];
    assert_eq!(
        tessellate_route_into(bow, &mut route, &mut too_large, options()),
        Err(RailError::Capacity)
    );
    assert_eq!(route.points(), before);
    let mut small = Route3::new(1).unwrap();
    small.set(&[Vec3::Y]).unwrap();
    assert_eq!(
        tessellate_route_into(bow, &mut small, &mut scratch, options()),
        Err(RailError::Route(NavigationError::Capacity))
    );
    assert_eq!(small.points(), [Vec3::Y]);
    let result = tessellate_route_into(bow, &mut route, &mut scratch, options()).unwrap();
    assert_eq!(result.status, TessellationStatus::Complete);
    assert_eq!(result.covered_until, 1.);
    assert_eq!(route.points(), &scratch[..result.written]);
    assert!(!route.is_closed());
    assert_eq!(*route.points().last().unwrap(), Vec3::X * 4.);
    let segments = [RailSegment3::new(RailPath3::Route(&route), key(0.), key(4.), 1.).unwrap()];
    assert!(
        Rail3::new(&segments, RailMode::Once)
            .unwrap()
            .sample_at(0.5, false)
            .is_ok()
    );
}

struct CountingAllocator;
thread_local! { static COUNT: Cell<Option<usize>> = const { Cell::new(None) }; }
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        COUNT.with(|n| {
            if let Some(count) = n.get() {
                n.set(Some(count + 1));
            }
        });
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        COUNT.with(|n| {
            if let Some(count) = n.get() {
                n.set(Some(count + 1));
            }
        });
        unsafe { System.realloc(pointer, layout, size) }
    }
}
fn measured(f: impl FnOnce()) -> usize {
    COUNT.with(|n| n.set(Some(0)));
    f();
    COUNT.with(|n| n.replace(None).unwrap())
}
#[test]
fn warmed_sampling_authoring_batch_failure_and_lifecycle_allocate_zero() {
    let cubic = curve(0., 8.);
    let mut route = Route3::new(128).unwrap();
    let mut scratch = [Vec3::ZERO; 128];
    tessellate_route_into(cubic, &mut route, &mut scratch, options()).unwrap();
    let segments = [RailSegment3::new(RailPath3::Route(&route), key(0.), key(8.), 1.).unwrap()];
    let rail = Rail3::new(&segments, RailMode::Loop).unwrap();
    let mut output = [rail.sample_at(0., false).unwrap(); 4];
    let mut player = RailPlayer3::new(rail);
    player.sample_at(0.25, false).unwrap();
    assert!(
        measured(|| {
            black_box(Box::new(black_box(7_u64)));
        }) > 0
    );
    let allocations = measured(|| {
        for _ in 0..512 {
            let prepared =
                RailSegment3::new(RailPath3::Cubic(cubic), key(0.), key(8.), 1.).unwrap();
            black_box(prepared);
            black_box(
                rail.sample_into(&[0., 0.5, 8., 100000.25], false, &mut output)
                    .unwrap(),
            );
            black_box(
                rail.sample_into(&[0., f64::NAN], false, &mut output)
                    .unwrap_err(),
            );
            player.reset();
            player.sample_at(0., false).unwrap();
            player.hold();
            player.sample_at(100., false).unwrap();
            player.resume();
            player.sample_at(0.5, false).unwrap();
            player.sample_at(32., false).unwrap();
            player.sample_at(0.25, false).unwrap();
            player.sample_at(0.25, true).unwrap();
            black_box(player.sample_at(f64::NAN, false).unwrap_err());
        }
    });
    assert_eq!(allocations, 0);
    let allocations = measured(|| {
        for _ in 0..128 {
            black_box(tessellate_route_into(cubic, &mut route, &mut scratch, options()).unwrap());
            black_box(
                tessellate_route_into(cubic, &mut route, &mut scratch[..1], options()).unwrap_err(),
            );
        }
    });
    assert_eq!(allocations, 0);
}
