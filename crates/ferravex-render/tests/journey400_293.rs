use ferravex_render::input::{
    self, ActionKind, Binding, InputCode, PadState, PlayerInput,
    calibration::{
        AxisCalibration as Axis, CALIBRATION_AXES, CALIBRATION_STICKS,
        CalibratedGamepad as Adapter, CalibrationError as Error, CalibrationProfile as Profile,
        GAMEPAD_SLOTS, RadialStick as Stick,
    },
};

fn connected() -> PadState {
    PadState {
        connected: true,
        ..Default::default()
    }
}
fn range() -> Axis {
    Axis {
        min: -0.75,
        center: 0.125,
        max: 0.875,
    }
}
fn drift_profile() -> Profile {
    Profile::try_new(
        &[(0, range())],
        &[Stick {
            axes: [0, 1],
            deadzone: 0.125,
        }],
    )
    .unwrap()
}
fn near(actual: f32, expected: f32) {
    assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
}

#[test]
fn asymmetric_drifting_axes_use_each_half_range_and_keep_other_channels() {
    let p = Profile::try_new(&[(0, range()), (15, range())], &[]).unwrap();
    assert_eq!(p.axes()[0], Some(range()));
    assert_eq!(p.axes()[1], None);
    assert_eq!(p.axes()[15], Some(range()));
    assert_eq!(p.sticks(), &[None; 8]);
    let mut raw = connected();
    raw.axes[1] = -0.3;
    raw.axes[4] = 0.8; // trigger axis keeps the existing unipolar meaning
    raw.buttons = std::array::from_fn(|i| i as f32 / 31.);
    raw.epoch = 91;
    for (sample, expected) in [
        (-1., -1.),
        (-0.75, -1.),
        (-0.3125, -0.5),
        (0.125, 0.),
        (0.5, 0.5),
        (0.875, 1.),
        (1., 1.),
    ] {
        raw.axes[0] = sample;
        raw.axes[15] = sample;
        let out = p.apply(raw).unwrap();
        assert_eq!(out.axes[0], expected);
        assert_eq!(out.axes[15], expected);
        assert_eq!(out.axes[1], -0.3);
        assert_eq!(out.axes[4], 0.8);
        assert_eq!(out.buttons, raw.buttons);
        assert_eq!(out.epoch, 91);
        assert!(out.connected);
    }
}

#[test]
fn radial_boundaries_fullscale_direction_and_explicit_nonstandard_pairs() {
    let p = Profile::try_new(
        &[],
        &[
            Stick {
                axes: [8, 15],
                deadzone: 0.25,
            },
            Stick {
                axes: [3, 2],
                deadzone: 0.,
            },
        ],
    )
    .unwrap();
    assert_eq!(p.sticks()[0].unwrap().axes, [8, 15]);
    let mut raw = connected();
    for (x, expected) in [(0., 0.), (0.125, 0.), (0.25, 0.), (0.625, 0.5), (1., 1.)] {
        raw.axes[8] = x;
        let out = p.apply(raw).unwrap();
        assert_eq!(out.axes[8], expected);
        assert_eq!(out.axes[15], 0.);
    }
    raw.axes[8] = f32::from_bits(0.25_f32.to_bits() + 1);
    assert!(p.apply(raw).unwrap().axes[8] > 0.);
    raw.axes[8] = -1.;
    raw.axes[15] = 1.;
    raw.axes[3] = 0.6;
    raw.axes[2] = -0.8;
    raw.axes[0] = 1.; // unpaired axis does not participate
    let out = p.apply(raw).unwrap();
    near(out.axes[8], -std::f32::consts::FRAC_1_SQRT_2);
    near(out.axes[15], std::f32::consts::FRAC_1_SQRT_2);
    near(out.axes[3], 0.6);
    near(out.axes[2], -0.8);
    assert_eq!(out.axes[0], 1.);
    let corrected = drift_profile();
    raw.axes[0] = 0.125;
    raw.axes[1] = 0.;
    assert_eq!(corrected.apply(raw).unwrap().axes[0], 0.);
    raw.axes[0] = 0.125 + 0.75 * 0.125;
    assert_eq!(corrected.apply(raw).unwrap().axes[0], 0.);
}

#[test]
fn finite_extremes_subnormal_ranges_and_near_one_deadzones_remain_finite() {
    let tiny = f32::from_bits(1);
    let p = Profile::try_new(
        &[(
            0,
            Axis {
                min: -tiny,
                center: 0.,
                max: tiny,
            },
        )],
        &[Stick {
            axes: [0, 1],
            deadzone: f32::from_bits(1_f32.to_bits() - 1),
        }],
    )
    .unwrap();
    let mut raw = connected();
    for (value, expected) in [
        (-f32::MAX, -1.),
        (-tiny, -1.),
        (0., 0.),
        (tiny, 1.),
        (f32::MAX, 1.),
    ] {
        raw.axes[0] = value;
        let out = p.apply(raw).unwrap();
        assert_eq!(out.axes[0], expected);
        assert!(out.axes.iter().all(|v| v.is_finite() && v.abs() <= 1.));
    }
    raw.axes[0] = f32::MAX;
    raw.axes[1] = -f32::MAX;
    let out = p.apply(raw).unwrap();
    near(out.axes[0], std::f32::consts::FRAC_1_SQRT_2);
    near(out.axes[1], -std::f32::consts::FRAC_1_SQRT_2);
    let p = Profile::try_new(
        &[],
        &[Stick {
            axes: [0, 1],
            deadzone: 0.,
        }],
    )
    .unwrap();
    raw.axes[0] = tiny;
    raw.axes[1] = 0.;
    assert_eq!(p.apply(raw).unwrap().axes[0], tiny);
}

#[test]
fn bounds_duplicates_overlaps_capacity_and_bad_ranges_are_rejected() {
    let axes = std::array::from_fn::<_, 16, _>(|i| (i, range()));
    let pairs = std::array::from_fn::<_, 8, _>(|i| Stick {
        axes: [i * 2, i * 2 + 1],
        deadzone: 0.2,
    });
    assert!(Profile::try_new(&axes, &pairs).is_ok());
    assert_eq!(
        Profile::try_new(&[(0, range()); 17], &[]),
        Err(Error::TooManyAxes)
    );
    assert_eq!(
        Profile::try_new(&[], &[pairs[0]; 9]),
        Err(Error::TooManySticks)
    );
    for axis in [16, usize::MAX] {
        assert_eq!(
            Profile::try_new(&[(axis, range())], &[]),
            Err(Error::InvalidAxis { axis })
        );
        assert_eq!(
            Profile::try_new(
                &[],
                &[Stick {
                    axes: [0, axis],
                    deadzone: 0.
                }]
            ),
            Err(Error::InvalidAxis { axis })
        );
    }
    assert_eq!(
        Profile::try_new(&[(0, range()); 2], &[]),
        Err(Error::DuplicateAxis { axis: 0 })
    );
    for pair in [[0, 0], [1, 0]] {
        let sticks = if pair[0] == pair[1] {
            vec![Stick {
                axes: pair,
                deadzone: 0.,
            }]
        } else {
            vec![
                pairs[0],
                Stick {
                    axes: pair,
                    deadzone: 0.,
                },
            ]
        };
        assert!(matches!(
            Profile::try_new(&[], &sticks),
            Err(Error::OverlappingStick { .. })
        ));
    }
    for deadzone in [
        -0.1,
        1.,
        f32::MAX,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        assert_eq!(
            Profile::try_new(
                &[],
                &[Stick {
                    axes: [0, 1],
                    deadzone
                }]
            ),
            Err(Error::InvalidDeadzone { stick: 0 })
        );
    }
    for bad in [
        Axis {
            min: -1.1,
            ..range()
        },
        Axis {
            max: 1.1,
            ..range()
        },
        Axis {
            min: 0.125,
            ..range()
        },
        Axis {
            max: 0.125,
            ..range()
        },
        Axis {
            center: 2.,
            ..range()
        },
        Axis {
            min: 1.,
            max: -1.,
            ..range()
        },
    ] {
        assert_eq!(
            Profile::try_new(&[(3, bad)], &[]),
            Err(Error::InvalidRange { axis: 3 })
        );
    }
    for bad in [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::MAX,
        -f32::MAX,
    ] {
        for axis in [
            Axis {
                min: bad,
                ..range()
            },
            Axis {
                center: bad,
                ..range()
            },
            Axis {
                max: bad,
                ..range()
            },
        ] {
            assert_eq!(
                Profile::try_new(&[(0, axis)], &[]),
                Err(Error::InvalidRange { axis: 0 })
            );
        }
    }
    for slot in [4, usize::MAX] {
        assert!(matches!(Adapter::new(slot), Err(Error::InvalidSlot { slot: s }) if s == slot));
    }
    for slot in 0..4 {
        assert_eq!(Adapter::new(slot).unwrap().slot(), slot);
    }
    assert_eq!(connected().axes.len(), CALIBRATION_AXES);
    assert_eq!(CALIBRATION_STICKS * 2, CALIBRATION_AXES);
    assert_eq!(GAMEPAD_SLOTS, input::gamepads().len());
}

#[test]
fn bad_samples_are_atomic_and_disconnected_values_are_always_neutral() {
    let p = drift_profile();
    let raw = connected();
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for axis in 0..16 {
            let mut bad = raw;
            bad.axes[axis] = invalid;
            assert_eq!(p.apply(bad), Err(Error::NonFiniteAxis { axis }));
        }
        for button in 0..32 {
            let mut bad = raw;
            bad.buttons[button] = invalid;
            assert_eq!(p.apply(bad), Err(Error::NonFiniteButton { button }));
        }
    }
    let disconnected = PadState {
        connected: false,
        epoch: 8,
        axes: [f32::NAN; 16],
        buttons: [f32::INFINITY; 32],
    };
    assert_eq!(
        p.apply(disconnected).unwrap(),
        PadState {
            epoch: 8,
            ..Default::default()
        }
    );
    assert_eq!(raw, connected());
}

#[test]
fn defaults_match_direct_submission_and_preserve_button_bits() {
    let p = Profile::default();
    assert_eq!(p, Profile::try_new(&[], &[]).unwrap());
    let mut raw = connected();
    raw.axes = std::array::from_fn(|i| (i as f32 - 8.) / 8.);
    raw.axes[1] = -0.;
    raw.buttons = std::array::from_fn(|i| i as f32 / 31.);
    let out = p.apply(raw).unwrap();
    assert_eq!(out.axes.map(f32::to_bits), raw.axes.map(f32::to_bits));
    assert_eq!(out.buttons.map(f32::to_bits), raw.buttons.map(f32::to_bits));
    // Direct boundary clamps finite extremes, including buttons; default adapter
    // must reach exactly the same platform state and connection epoch.
    raw.axes[15] = f32::MAX;
    raw.buttons[31] = f32::MAX;
    let mut adapter = Adapter::new(0).unwrap();
    adapter.disconnect();
    input::gamepad_sample(0, raw);
    let expected = input::gamepads()[0];
    adapter.submit(raw).unwrap();
    assert_eq!(input::gamepads()[0], expected);
    adapter.select_profile(p).unwrap();
    assert_eq!(input::gamepads()[0], expected);
    adapter.reset();
    assert_eq!(input::gamepads()[0], expected);
    adapter.disconnect();
}

#[test]
fn profile_changes_reset_disconnect_and_epoch_replacement_do_not_leave_stuck_axes() {
    let mut adapter = Adapter::new(0).unwrap();
    adapter.disconnect();
    assert_eq!(
        adapter.select_profile(drift_profile()),
        Err(Error::Disconnected)
    );
    let mut raw = connected();
    raw.axes[0] = 0.125;
    adapter.submit(raw).unwrap();
    adapter.select_profile(drift_profile()).unwrap();
    assert_eq!(input::gamepads()[0].axes[0], 0.);
    assert_eq!(adapter.profile(), Some(&drift_profile()));
    // This replacement calibrates another axis and must undo the previous axis.
    let other = Profile::try_new(&[(15, range())], &[]).unwrap();
    adapter.select_profile(other).unwrap();
    assert_eq!(input::gamepads()[0].axes[0], 0.125);
    assert!(input::gamepads()[0].axes[15] < 0.);
    adapter.reset();
    assert!(adapter.profile().is_none());
    assert_eq!(input::gamepads()[0].axes, raw.axes);
    adapter.select_profile(drift_profile()).unwrap();
    let before = input::gamepads()[0];
    let mut invalid = raw;
    invalid.epoch = 99;
    invalid.axes[15] = f32::NAN;
    assert_eq!(
        adapter.submit(invalid),
        Err(Error::NonFiniteAxis { axis: 15 })
    );
    assert_eq!(adapter.last_raw(), &raw);
    assert_eq!(adapter.profile(), Some(&drift_profile()));
    assert_eq!(input::gamepads()[0], before);
    raw.epoch = 1;
    adapter.submit(raw).unwrap();
    assert!(adapter.profile().is_none());
    assert_eq!(input::gamepads()[0].axes, raw.axes);
    assert_eq!(input::gamepads()[0].epoch, before.epoch.wrapping_add(2));
    adapter.select_profile(drift_profile()).unwrap();
    adapter
        .submit(PadState {
            axes: [f32::NAN; 16],
            ..Default::default()
        })
        .unwrap();
    assert!(!input::gamepads()[0].connected);
    assert_eq!(input::gamepads()[0].axes, [0.; 16]);
    assert_eq!(input::gamepads()[0].buttons, [0.; 32]);
    assert!(adapter.profile().is_none());
    assert_eq!(adapter.last_raw(), &PadState::default());
    adapter.submit(raw).unwrap();
    assert_eq!(input::gamepads()[0].axes, raw.axes);
    adapter.disconnect();
    let epoch = input::gamepads()[0].epoch;
    adapter.disconnect();
    adapter.reset();
    assert_eq!(input::gamepads()[0].epoch, epoch);
}

fn player() -> PlayerInput {
    let mut p = PlayerInput::transient("journey400-293");
    for (name, kind, code) in [
        ("move_x", ActionKind::Axis, InputCode::PadAxis(0)),
        ("jump", ActionKind::Button, InputCode::PadButton(0)),
        ("fire", ActionKind::Button, InputCode::PadButton(7)),
        ("right", ActionKind::Button, InputCode::PadButton(15)),
    ] {
        p.action(name, name, kind, vec![Binding::digital(code)])
            .unwrap();
    }
    let mut settings = p.settings();
    settings.pad_deadzone = 0.;
    settings.pad_curve = 1.;
    p.set_settings(settings).unwrap();
    p
}
fn frame(p: &mut PlayerInput) {
    input::host_frame(0., 640, 480, || p.sample(true));
}

#[test]
fn actual_host_journal_player_recipe_keeps_taps_across_zero_tick_and_catchup_frames() {
    let mut session = ferravex_core::session::GameSession::try_new(64., 4, true).unwrap();
    let mut adapter = Adapter::new(0).unwrap();
    adapter.disconnect();
    let mut p = player();
    let mut raw = connected();
    raw.axes[0] = 0.125;
    adapter.submit(raw).unwrap();
    frame(&mut p);
    adapter.select_profile(drift_profile()).unwrap();
    raw.axes[0] = 0.875;
    raw.buttons[0] = 1.;
    raw.buttons[7] = 0.8; // actual trigger button mapping, not axis-derived
    raw.buttons[15] = 1.; // actual right hat mapping
    adapter.submit(raw).unwrap();
    raw.buttons[0] = 0.;
    adapter.submit(raw).unwrap();
    frame(&mut p); // zero simulation ticks
    assert_eq!(session.advance(0.).steps, 0);
    assert_eq!(p.value("move_x"), 1.);
    adapter.select_profile(Profile::default()).unwrap();
    adapter.reset(); // same held button values must not add presses
    frame(&mut p);
    let ticks = session.advance(2. / 64.);
    assert_eq!(ticks.steps, 2);
    for tick in 0..ticks.steps {
        let [jump, fire, right] = p.take_tick(["jump", "fire", "right"]);
        assert_eq!(jump.pressed, tick == 0);
        assert_eq!(jump.released, tick == 0);
        assert_eq!(jump.value, 0.);
        assert_eq!(fire.pressed, tick == 0);
        assert!(!fire.released && fire.value > 0.);
        assert_eq!(right.pressed, tick == 0);
        assert!(!right.released);
    }
    adapter.disconnect();
    frame(&mut p);
    assert_eq!(p.value("move_x"), 0.);
    assert_eq!(p.value("fire"), 0.);
    adapter.submit(raw).unwrap(); // reconnect while held -> existing suppression
    frame(&mut p);
    assert!(!p.take_tick(["fire"])[0].pressed);
    raw.buttons[7] = 0.;
    raw.axes[0] = 0.;
    adapter.submit(raw).unwrap();
    frame(&mut p);
    raw.buttons[7] = 1.;
    adapter.submit(raw).unwrap();
    frame(&mut p);
    assert!(p.take_tick(["fire"])[0].pressed);
    adapter.disconnect();
}

#[test]
fn replacement_epoch_uses_real_reconnection_suppression_and_forgets_selection() {
    let mut adapter = Adapter::new(0).unwrap();
    adapter.disconnect();
    let mut p = player();
    let mut raw = connected();
    adapter.submit(raw).unwrap();
    frame(&mut p);
    adapter.select_profile(drift_profile()).unwrap();
    raw.buttons[0] = 1.;
    adapter.submit(raw).unwrap();
    // Replacement before the frame consumes the old device's pending press.
    raw.epoch = 1;
    raw.axes[0] = 0.125;
    adapter.submit(raw).unwrap();
    frame(&mut p);
    assert!(adapter.profile().is_none());
    assert_eq!(input::gamepads()[0].axes[0], 0.125);
    assert!(!p.take_tick(["jump"])[0].pressed);
    raw.buttons[0] = 0.;
    adapter.submit(raw).unwrap();
    frame(&mut p);
    raw.buttons[0] = 1.;
    adapter.submit(raw).unwrap();
    frame(&mut p);
    assert!(p.take_tick(["jump"])[0].pressed);
    adapter.disconnect();
}

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};
thread_local! { static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) }; }
struct CountingAllocator;
fn allocated(p: *mut u8) {
    if !p.is_null() {
        let _ = ALLOCATIONS.try_with(|c| {
            if let Some(count) = c.get() {
                c.set(Some(count + 1));
            }
        });
    }
}
// SAFETY: Arguments and results are forwarded unchanged to System. Accounting
// is thread-local, nonallocating and tolerates TLS teardown.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        allocated(p);
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        allocated(p);
        p
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, layout, size) };
        allocated(p);
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn warmed_maximum_profile_math_selection_submission_reset_and_errors_allocate_zero() {
    let axes = std::array::from_fn::<_, 16, _>(|i| (i, range()));
    let sticks = std::array::from_fn::<_, 8, _>(|i| Stick {
        axes: [i * 2, i * 2 + 1],
        deadzone: 0.125,
    });
    let profile = Profile::try_new(&axes, &sticks).unwrap();
    let mut adapter = Adapter::new(0).unwrap();
    adapter.disconnect();
    let raw = connected();
    adapter.submit(raw).unwrap();
    adapter.select_profile(profile).unwrap();
    input::host_frame(0., 640, 480, || {});
    struct StopCounting;
    impl Drop for StopCounting {
        fn drop(&mut self) {
            ALLOCATIONS.with(|c| c.set(None));
        }
    }
    ALLOCATIONS.with(|c| c.set(Some(0)));
    let guard = StopCounting;
    for _ in 0..16 {
        black_box(Profile::try_new(&axes, &sticks).unwrap());
        black_box(profile.apply(raw).unwrap());
        adapter.submit(raw).unwrap();
        adapter.select_profile(profile).unwrap();
        adapter.reset();
        let mut invalid = raw;
        invalid.axes[15] = f32::INFINITY;
        assert!(black_box(adapter.submit(invalid)).is_err());
        assert!(black_box(Profile::try_new(&[(0, range()); 17], &[])).is_err());
        assert!(black_box(Adapter::new(4)).is_err());
        adapter.disconnect();
        input::host_frame(0., 640, 480, || {});
    }
    let count = ALLOCATIONS.with(|c| c.get().unwrap());
    drop(guard);
    assert_eq!(count, 0);
}
