use ferravex_math::{Mat4, Quat, Vec3, projection::NdcDepth};
use ferravex_play::camera::{
    Frame3, Input3, Mode3, NoObstructions, Pose3, Rig3, Target3,
    transition::{Perspective3, Transition3, TransitionError, ViewHandedness},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

fn lens() -> Perspective3 {
    Perspective3 {
        aspect: 16. / 9.,
        near: 0.1,
        far: 1000.,
        handedness: ViewHandedness::Right,
        depth: NdcDepth::NegativeOneToOne,
    }
}
fn pose(position: Vec3, rotation: Quat, distance: f32, fov_y: f32) -> Pose3 {
    Pose3 {
        position,
        target: position + rotation * Vec3::NEG_Z * distance,
        up: rotation * Vec3::Y,
        fov_y,
    }
}
fn at(x: f32) -> Pose3 {
    pose(Vec3::X * x, Quat::IDENTITY, 1., 1.)
}
fn close(a: Vec3, b: Vec3) {
    assert!(a.distance(b) < 3e-5, "{a:?} != {b:?}");
}
fn forward(p: Pose3) -> Vec3 {
    (p.target - p.position).normalize()
}
fn started() -> Transition3 {
    let mut view = Transition3::new();
    view.cut(at(0.), lens()).unwrap();
    view.retarget(at(8.), lens(), 1.).unwrap();
    view.advance(0.25).unwrap();
    view
}

#[test]
fn empty_initialization_zero_time_and_exact_endpoints() {
    let mut view = Transition3::new();
    assert_eq!(view, Transition3::default());
    assert_eq!((view.pose(), view.projection()), (None, None));
    assert_eq!(view.advance(0.25), Ok(None));
    assert!(!view.is_blending());
    view.hold();
    let initial = Pose3 {
        up: Vec3::new(0., 20., 4.),
        ..at(0.)
    };
    view.retarget(initial, lens(), 100.).unwrap();
    assert_eq!(view.pose(), Some(initial));
    assert!(!view.is_held());
    assert_eq!(view.generation(), 1);
    let goal = Pose3 {
        up: Vec3::new(0., 2., 5.),
        ..at(8.)
    };
    view.retarget(goal, lens(), 0.5).unwrap();
    assert_eq!(view.advance(-0.), Ok(Some(initial)));
    let middle = view.advance(0.25).unwrap().unwrap();
    close(middle.position, Vec3::X * 4.);
    close(middle.up, Vec3::Y);
    assert_eq!(view.advance(0.25), Ok(Some(goal)));
    assert!(!view.is_blending());
    assert_eq!(view.advance(0.25), Ok(Some(goal)));
    assert_eq!(view.generation(), 1);
}

#[test]
fn shortest_arc_wrap_and_distance_fov_use_independent_interpolation() {
    let mut view = Transition3::new();
    let degrees = |v: f32| v.to_radians();
    let a = pose(Vec3::ZERO, Quat::from_rotation_y(degrees(170.)), 2., 0.5);
    let b = pose(
        Vec3::new(4., 2., -2.),
        Quat::from_rotation_y(degrees(-170.)),
        6.,
        2.5,
    );
    view.cut(a, lens()).unwrap();
    view.retarget(b, lens(), 0.5).unwrap();
    let p = view.advance(0.25).unwrap().unwrap();
    close(p.position, Vec3::new(2., 1., -1.));
    close(forward(p), Vec3::Z);
    close(p.up, Vec3::Y);
    assert!((p.position.distance(p.target) - 4.).abs() < 1e-5);
    assert_eq!(p.fov_y, 1.5);
    assert_eq!(view.advance(0.25), Ok(Some(b)));
}

#[test]
fn exact_half_turn_ties_are_stable_for_yaw_and_roll() {
    for rotation in [
        Quat::from_xyzw(0., 1., 0., 0.),
        Quat::from_xyzw(0., 0., 1., 0.),
    ] {
        let mut view = Transition3::new();
        view.cut(at(0.), lens()).unwrap();
        let end = pose(Vec3::ZERO, rotation, 1., 1.);
        view.retarget(end, lens(), 0.5).unwrap();
        let p = view.advance(0.25).unwrap().unwrap();
        let expected = Quat::IDENTITY
            .try_rotate_towards(rotation, std::f32::consts::FRAC_PI_2)
            .unwrap();
        close(forward(p), expected * Vec3::NEG_Z);
        close(p.up, expected * Vec3::Y);
        assert_eq!(view.advance(0.25), Ok(Some(end)));
        // Antipodal quaternion input constructs the same public pose.
        view.cut(at(0.), lens()).unwrap();
        view.retarget(pose(Vec3::ZERO, -rotation, 1., 1.), lens(), 0.5)
            .unwrap();
        assert_eq!(view.advance(0.25), Ok(Some(p)));
    }
}

#[test]
fn arbitrary_roll_and_pitch_match_existing_quaternion_helpers() {
    let a = Quat::from_rotation_x(-0.8) * Quat::from_rotation_y(1.2);
    let b = Quat::from_rotation_z(2.1) * Quat::from_rotation_x(1.1);
    let mut view = Transition3::new();
    view.cut(pose(Vec3::ZERO, a, 3., 1.), lens()).unwrap();
    view.retarget(pose(Vec3::ZERO, b, 3., 1.), lens(), 1.)
        .unwrap();
    let p = view.advance(0.25).unwrap().unwrap();
    let expected = a
        .try_rotate_towards(b, a.try_angle_between(b).unwrap() * 0.25)
        .unwrap();
    close(forward(p), expected * Vec3::NEG_Z);
    close(p.up, expected * Vec3::Y);
    assert!(forward(p).dot(p.up).abs() < 1e-6);
}

#[test]
fn retarget_starts_at_displayed_pose_including_held_retarget() {
    let mut view = started();
    let displayed = view.pose().unwrap();
    assert_eq!(displayed.position.x, 2.);
    view.retarget(at(-6.), lens(), 0.5).unwrap();
    assert_eq!(view.pose(), Some(displayed));
    assert_eq!(view.advance(0.), Ok(Some(displayed)));
    assert_eq!(view.advance(0.25).unwrap().unwrap().position.x, -2.);
    assert!(view.hold());
    assert!(!view.hold());
    let frozen = view.pose();
    view.retarget(at(10.), lens(), 0.5).unwrap();
    assert!(view.is_held());
    assert_eq!(view.advance(0.25).unwrap(), frozen);
    assert!(view.resume());
    assert!(!view.resume());
    assert_eq!(view.advance(0.).unwrap(), frozen);
    assert_eq!(view.advance(0.25).unwrap().unwrap().position.x, 4.);
    assert_eq!(view.advance(0.25), Ok(Some(at(10.))));
    assert_eq!(view.generation(), 1);
}

#[test]
fn hold_discards_time_and_resume_does_not_rewind_or_catch_up() {
    let mut view = started();
    view.hold();
    let frozen = view;
    for _ in 0..1000 {
        assert_eq!(view.advance(0.25).unwrap(), frozen.pose());
    }
    assert_eq!(view, frozen);
    view.resume();
    assert_eq!(view.advance(0.).unwrap(), frozen.pose());
    assert_eq!(view.advance(0.25).unwrap().unwrap().position.x, 4.);
    assert_eq!(view.advance(0.25).unwrap().unwrap().position.x, 6.);
    assert_eq!(view.advance(0.25), Ok(Some(at(8.))));
}

#[test]
fn zero_duration_and_explicit_cut_override_hold_and_allow_lens_changes() {
    let mut view = started();
    view.hold();
    let changed = Perspective3 {
        aspect: 2.,
        ..lens()
    };
    view.retarget(at(40.), changed, -0.).unwrap();
    assert!(!view.is_held());
    assert!(!view.is_blending());
    assert_eq!(view.pose(), Some(at(40.)));
    assert_eq!(view.projection(), Some(changed));
    assert_eq!(view.generation(), 2);
    view.hold();
    view.cut(at(-40.), lens()).unwrap();
    assert_eq!(view.advance(0.25), Ok(Some(at(-40.))));
    assert!(!view.is_held());
    assert_eq!(view.generation(), 3);
}

#[test]
fn projection_changes_are_rejected_atomically_unless_cut() {
    let base = started();
    for changed in [
        Perspective3 {
            aspect: 2.,
            ..lens()
        },
        Perspective3 {
            near: 0.2,
            ..lens()
        },
        Perspective3 {
            far: 999.,
            ..lens()
        },
        Perspective3 {
            handedness: ViewHandedness::Left,
            ..lens()
        },
        Perspective3 {
            depth: NdcDepth::ZeroToOne,
            ..lens()
        },
    ] {
        let mut view = base;
        view.hold();
        let before = view;
        assert_eq!(
            view.retarget(at(0.), changed, 1.),
            Err(TransitionError::IncompatibleProjection)
        );
        assert_eq!(view, before);
        view.cut(at(0.), changed).unwrap();
        assert_eq!(view.projection(), Some(changed));
    }
    for handedness in [ViewHandedness::Right, ViewHandedness::Left] {
        for depth in [
            NdcDepth::NegativeOneToOne,
            NdcDepth::ZeroToOne,
            NdcDepth::ReverseNegativeOneToOne,
            NdcDepth::ReverseZeroToOne,
        ] {
            let lens = Perspective3 {
                handedness,
                depth,
                ..lens()
            };
            let mut view = Transition3::new();
            view.cut(at(0.), lens).unwrap();
            view.retarget(at(4.), lens, 0.5).unwrap();
            let p = view.advance(0.25).unwrap().unwrap();
            let matrix = match handedness {
                ViewHandedness::Right => Mat4::try_look_at_rh(p.position, p.target, p.up),
                ViewHandedness::Left => Mat4::try_look_at_lh(p.position, p.target, p.up),
            }
            .unwrap();
            let target = matrix.transform_point3(p.target);
            assert!(match handedness {
                ViewHandedness::Right => target.z < 0.,
                ViewHandedness::Left => target.z > 0.,
            });
            assert_eq!(view.projection(), Some(lens));
        }
    }
}

#[test]
fn invalid_lenses_are_rejected_in_all_states() {
    for invalid in [
        Perspective3 {
            aspect: 0.,
            ..lens()
        },
        Perspective3 {
            aspect: -1.,
            ..lens()
        },
        Perspective3 {
            aspect: f32::INFINITY,
            ..lens()
        },
        Perspective3 {
            aspect: f32::NAN,
            ..lens()
        },
        Perspective3 { near: 0., ..lens() },
        Perspective3 {
            near: -1.,
            ..lens()
        },
        Perspective3 {
            near: f32::NAN,
            ..lens()
        },
        Perspective3 { far: 0.1, ..lens() },
        Perspective3 { far: 0., ..lens() },
        Perspective3 {
            far: f32::INFINITY,
            ..lens()
        },
    ] {
        for mut view in [Transition3::new(), started()] {
            view.hold();
            let before = view;
            assert_eq!(
                view.cut(at(0.), invalid),
                Err(TransitionError::InvalidProjection)
            );
            assert_eq!(view, before);
            assert_eq!(
                view.retarget(at(0.), invalid, 0.),
                Err(TransitionError::InvalidProjection)
            );
            assert_eq!(view, before);
        }
    }
}

#[test]
fn invalid_poses_reject_before_mutation_even_for_cuts_and_held_state() {
    for invalid in [
        Pose3 {
            position: Vec3::NAN,
            ..at(0.)
        },
        Pose3 {
            target: Vec3::INFINITY,
            ..at(0.)
        },
        Pose3 {
            position: Vec3::X * 1_000_001.,
            target: Vec3::X * 1_000_001. - Vec3::Z,
            ..at(0.)
        },
        Pose3 {
            target: Vec3::ZERO,
            ..at(0.)
        },
        Pose3 {
            target: Vec3::NEG_Z * 0.0009,
            ..at(0.)
        },
        Pose3 {
            target: Vec3::NEG_Z * 1_000_001.,
            ..at(0.)
        },
        Pose3 {
            up: Vec3::ZERO,
            ..at(0.)
        },
        Pose3 {
            up: Vec3::NAN,
            ..at(0.)
        },
        Pose3 {
            up: Vec3::Z,
            ..at(0.)
        },
        Pose3 {
            up: Vec3::new(0., 0.00001, 1.),
            ..at(0.)
        },
        Pose3 {
            fov_y: 0.,
            ..at(0.)
        },
        Pose3 {
            fov_y: 3.1001,
            ..at(0.)
        },
        Pose3 {
            fov_y: f32::NAN,
            ..at(0.)
        },
    ] {
        for mut view in [Transition3::new(), started()] {
            view.hold();
            let before = view;
            assert_eq!(view.cut(invalid, lens()), Err(TransitionError::InvalidPose));
            assert_eq!(view, before);
            assert_eq!(
                view.retarget(invalid, lens(), 1.),
                Err(TransitionError::InvalidPose)
            );
            assert_eq!(view, before);
            assert_eq!(
                view.retarget_frame(&frame(invalid, true), lens(), 0.),
                Err(TransitionError::InvalidPose)
            );
            assert_eq!(view, before);
        }
    }
}

#[test]
fn up_scale_skew_and_pose_bounds_are_explicit() {
    for up in [
        Vec3::Y * f32::MAX,
        Vec3::Y * f32::from_bits(1),
        Vec3::new(0., 1., -1000.),
    ] {
        let mut view = Transition3::new();
        let a = Pose3 { up, ..at(0.) };
        view.cut(a, lens()).unwrap();
        view.retarget(at(1.), lens(), 0.5).unwrap();
        assert_eq!(view.pose(), Some(a));
        close(view.advance(0.25).unwrap().unwrap().up, Vec3::Y);
    }
    let mut view = Transition3::new();
    for distance in [0.001, 1e6] {
        for fov_y in [0.05, 3.1] {
            let p = pose(Vec3::ZERO, Quat::IDENTITY, distance, fov_y);
            view.cut(p, lens()).unwrap();
            assert_eq!(view.advance(0.), Ok(Some(p)));
        }
    }
}

#[test]
fn invalid_duration_and_delta_reject_even_when_idle_empty_held_or_cut() {
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 3600.1] {
        for mut view in [Transition3::new(), started()] {
            view.hold();
            let before = view;
            assert_eq!(
                view.retarget(at(8.), lens(), invalid),
                Err(TransitionError::InvalidDuration)
            );
            assert_eq!(view, before);
            assert_eq!(
                view.retarget_frame(&frame(at(100.), true), lens(), invalid),
                Err(TransitionError::InvalidDuration)
            );
            assert_eq!(view, before);
        }
    }
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 0.250001] {
        for mut view in [Transition3::new(), started()] {
            for held in [false, true] {
                if held {
                    view.hold();
                }
                let before = view;
                assert_eq!(view.advance(invalid), Err(TransitionError::InvalidDelta));
                assert_eq!(view, before);
            }
        }
    }
    let mut view = Transition3::new();
    view.retarget(at(0.), lens(), 3600.).unwrap();
    view.retarget(at(8.), lens(), f32::from_bits(1)).unwrap();
    assert_eq!(view.advance(0.25), Ok(Some(at(8.))));
}

fn frame(pose: Pose3, cut: bool) -> Frame3 {
    Frame3 {
        pose,
        cut,
        obstructed: true,
        blocked_pivot: true,
        recovered: true,
        queries: 19,
    }
}

#[test]
fn frame_diagnostics_are_preserved_and_never_treated_as_cut_or_path_observations() {
    let f = frame(at(8.), false);
    let original = f;
    let mut view = Transition3::new();
    view.cut(at(0.), lens()).unwrap();
    view.retarget_frame(&f, lens(), 0.5).unwrap();
    assert_eq!(view.advance(0.25).unwrap().unwrap().position.x, 4.);
    assert_eq!(f, original);
    assert_eq!(view.generation(), 1);
    view.hold();
    let cut = frame(at(100.), true);
    view.retarget_frame(&cut, lens(), 1.).unwrap();
    assert_eq!(view.advance(0.), Ok(Some(cut.pose)));
    assert!(!view.is_held());
    assert!(!view.is_blending());
    assert_eq!(view.generation(), 2);
}

#[test]
fn actual_rig_accepted_teleport_clears_old_blend_and_reset_forgets_everything() {
    let mut view = started();
    view.hold();
    let mut rig = Rig3::default();
    let frame = rig
        .snap(
            Input3 {
                target: Target3::at(Vec3::X * 100.),
                mode: Mode3::Follow {
                    eye_offset: Vec3::Z * 5.,
                    look_offset: Vec3::ZERO,
                },
                fov_y: 1.,
            },
            &NoObstructions,
        )
        .unwrap();
    assert!(frame.cut);
    view.retarget_frame(&frame, lens(), 2.).unwrap();
    for dt in [0., 0.25] {
        assert_eq!(view.advance(dt), Ok(Some(frame.pose)));
    }
    view.reset();
    assert_eq!(view.generation(), 3);
    assert_eq!(view.pose(), None);
    assert_eq!(view.projection(), None);
    assert!(!view.is_held());
    assert!(!view.is_blending());
    assert_eq!(view.advance(0.25), Ok(None));
    view.retarget(at(-5.), lens(), 1.).unwrap();
    assert_eq!(view.advance(0.), Ok(Some(at(-5.))));
    assert_eq!(view.generation(), 4);
}

#[test]
fn partitioned_time_does_not_change_stationary_segment_and_overshoot_is_discarded() {
    let mut a = Transition3::new();
    a.cut(at(0.), lens()).unwrap();
    a.retarget(at(8.), lens(), 0.5).unwrap();
    let mut b = a;
    a.advance(0.25).unwrap();
    for _ in 0..16 {
        b.advance(1. / 64.).unwrap();
    }
    assert_eq!(a.pose(), b.pose());
    a.advance(0.25).unwrap();
    b.advance(0.25).unwrap();
    assert_eq!(a, b);
    a.retarget(at(0.), lens(), 0.125).unwrap();
    assert_eq!(a.advance(0.25), Ok(Some(at(0.))));
    a.retarget(at(8.), lens(), 0.5).unwrap();
    assert_eq!(a.advance(0.), Ok(Some(at(0.))));
    assert_eq!(a.advance(0.25).unwrap().unwrap().position.x, 4.);
}

#[test]
fn rounded_target_collapse_is_atomic_and_explicit_cut_can_complete() {
    // Both endpoints have valid 0.001-long views at x=0. After half a roll,
    // the short target points along Y at y=1e6, below one f32 coordinate ULP.
    let eye = Vec3::new(0., 1e6, 0.);
    let a = Pose3 {
        position: eye,
        target: eye + Vec3::X * 0.001,
        up: Vec3::Z,
        fov_y: 1.,
    };
    let b = Pose3 {
        position: eye,
        target: eye - Vec3::X * 0.001,
        up: Vec3::Z,
        fov_y: 1.,
    };
    let mut view = Transition3::new();
    view.cut(a, lens()).unwrap();
    view.retarget(b, lens(), 0.5).unwrap();
    let before = view;
    assert_eq!(view.advance(0.25), Err(TransitionError::NumericalLimit));
    assert_eq!(view, before);
    view.cut(b, lens()).unwrap();
    assert_eq!(view.advance(0.), Ok(Some(b)));
}

// SAFETY: Forward allocation contracts unchanged to System. TLS counts only
// this test thread, tolerating teardown; production code contains no unsafe.
struct Allocator;
thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
}
fn count() {
    let _ = RECORD.try_with(|r| {
        if r.get() {
            let _ = CALLS.try_with(|c| c.set(c.get() + 1));
        }
    });
}
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
fn allocations(action: impl FnOnce()) -> usize {
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            RECORD.set(false);
        }
    }
    CALLS.set(0);
    RECORD.set(true);
    let stop = Stop;
    action();
    drop(stop);
    CALLS.get()
}
#[test]
fn allocation_instrument_has_positive_control() {
    assert!(
        allocations(|| {
            black_box(Box::new(287));
        }) > 0
    );
}
#[test]
fn construction_blend_retarget_cut_hold_reset_and_errors_have_bounded_inline_storage() {
    assert_eq!(
        allocations(|| {
            let mut view = Transition3::new();
            for _ in 0..512 {
                view.cut(black_box(at(0.)), black_box(lens())).unwrap();
                view.retarget(black_box(at(8.)), lens(), 1.).unwrap();
                black_box(view.advance(0.25)).unwrap();
                view.hold();
                view.retarget(at(-4.), lens(), 0.5).unwrap();
                black_box(view.advance(0.25)).unwrap();
                view.resume();
                black_box(view.advance(0.25)).unwrap();
                assert_eq!(view.advance(f32::NAN), Err(TransitionError::InvalidDelta));
                view.retarget_frame(&frame(at(100.), true), lens(), 1.)
                    .unwrap();
                view.reset();
                black_box(view.pose());
            }
        }),
        0
    );
}

#[test]
fn rotational_retarget_begins_at_visible_orientation_and_distance() {
    let mut view = Transition3::new();
    view.cut(pose(Vec3::ZERO, Quat::IDENTITY, 2., 1.), lens())
        .unwrap();
    view.retarget(
        pose(Vec3::ZERO, Quat::from_rotation_y(1.2), 6., 1.),
        lens(),
        1.,
    )
    .unwrap();
    let visible = view.advance(0.25).unwrap().unwrap();
    let visible_rotation = Quat::from_rotation_y(0.3);
    close(forward(visible), visible_rotation * Vec3::NEG_Z);
    assert!((visible.position.distance(visible.target) - 3.).abs() < 1e-5);
    let next_rotation = Quat::from_rotation_z(-0.8);
    let next = pose(Vec3::ZERO, next_rotation, 7., 2.);
    view.retarget(next, lens(), 0.5).unwrap();
    assert_eq!(view.advance(0.), Ok(Some(visible)));
    let half = view.advance(0.25).unwrap().unwrap();
    let expected = visible_rotation
        .try_rotate_towards(
            next_rotation,
            visible_rotation.try_angle_between(next_rotation).unwrap() * 0.5,
        )
        .unwrap();
    close(forward(half), expected * Vec3::NEG_Z);
    close(half.up, expected * Vec3::Y);
    assert!((half.position.distance(half.target) - 5.).abs() < 1e-5);
    assert_eq!(half.fov_y, 1.5);
    assert_eq!(view.advance(0.25), Ok(Some(next)));
}
