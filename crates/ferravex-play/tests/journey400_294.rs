//! Public, supplied-time gesture regressions. No host clock or browser is used.
use ferravex_math::{Rect, Vec2};
use ferravex_play::input::{
    Action, ActionKind, ActionMap, Binding, ContextId, InputCode, InputError, touch::*,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

fn event(id: u64, x: f32, y: f32, phase: Phase) -> Event {
    Event {
        id,
        position: Vec2::new(x, y),
        phase,
    }
}
fn button(channel: u8) -> Control {
    Control {
        region: Region::Rect(Rect::new(0., 0., 100., 100.)),
        output: Output::Button(channel),
    }
}
fn setup(profile: GestureProfile) -> (TouchSurface, ActionMap) {
    let mut map = ActionMap::default();
    let mut surface = TouchSurface::default();
    surface.set_layout(&[button(0)], &mut map).unwrap();
    surface
        .set_gesture_profile(Some(profile), &mut map)
        .unwrap();
    (surface, map)
}
fn send(
    surface: &mut TouchSurface,
    map: &mut ActionMap,
    id: u64,
    x: f32,
    phase: Phase,
    time: f64,
) -> Option<GestureObservation> {
    surface
        .handle_timed(event(id, x, 20., phase), time, map)
        .unwrap()
}
fn action(map: &mut ActionMap, code: InputCode, kind: ActionKind) -> usize {
    map.named_action(
        "sample",
        kind,
        ContextId::GLOBAL,
        vec![Binding::digital(code)],
    )
    .unwrap()
}

#[test]
fn tap_inclusive_distance_time_and_immediate_edges_are_separate() {
    let (mut surface, mut map) = setup(GestureProfile::default());
    let jump = action(&mut map, InputCode::Touch(0), ActionKind::Button);
    assert_eq!(
        send(&mut surface, &mut map, 1, 20., Phase::Start, 10.),
        None
    );
    assert!(map.peek(jump).pressed);
    let observed = send(&mut surface, &mut map, 1, 30., Phase::End, 10.25).unwrap();
    assert_eq!(
        observed,
        GestureObservation {
            id: 1,
            control: 0,
            kind: GestureKind::Tap,
            phase: Phase::End,
            origin: Vec2::splat(20.),
            position: Vec2::new(30., 20.),
            elapsed_seconds: 0.25,
        }
    );
    assert_eq!(
        map.peek(jump),
        Action {
            value: 0.,
            pressed: true,
            released: true
        }
    );
    assert_eq!(
        send(&mut surface, &mut map, 1, 30., Phase::End, 10.25),
        None
    );
    let first = map.consume_tick(jump);
    assert!(first.pressed && first.released);
    assert_eq!(map.consume_tick(jump), Action::default());
}

#[test]
fn drag_requires_distance_and_supplied_duration_then_latches_excursion() {
    let profile = GestureProfile {
        drag_min_seconds: 0.25,
        ..GestureProfile::default()
    };
    let (mut surface, mut map) = setup(profile);
    send(&mut surface, &mut map, 1, 20., Phase::Start, 2.);
    // Distance alone cannot satisfy a supplied duration; calls do not advance time.
    for _ in 0..20 {
        assert_eq!(send(&mut surface, &mut map, 1, 32., Phase::Move, 2.), None);
    }
    assert_eq!(
        send(&mut surface, &mut map, 1, 20., Phase::Move, 2.125),
        None
    );
    let drag = send(&mut surface, &mut map, 1, 20., Phase::Move, 2.25).unwrap();
    assert_eq!(
        (drag.kind, drag.phase, drag.elapsed_seconds),
        (GestureKind::Drag, Phase::Move, 0.25)
    );
    assert_eq!(
        send(&mut surface, &mut map, 1, 20., Phase::End, 3.)
            .unwrap()
            .kind,
        GestureKind::Drag
    );
    send(&mut surface, &mut map, 2, 20., Phase::Start, 4.);
    // Just below the distance is not a drag, even after enough time.
    assert_eq!(
        send(&mut surface, &mut map, 2, 31.5, Phase::Move, 4.25),
        None
    );
    assert_eq!(
        send(&mut surface, &mut map, 2, 32., Phase::Move, 4.25)
            .unwrap()
            .kind,
        GestureKind::Drag
    );
}

#[test]
fn end_without_move_classifies_swipe_then_drag_and_uses_final_net_distance() {
    let (mut surface, mut map) = setup(GestureProfile::default());
    for (start, end, seconds, expected) in [
        (20., 68., 0.5, Some(GestureKind::Swipe)),
        (20., 68., 0.5001, Some(GestureKind::Drag)),
        (20., 67.5, 0.5, Some(GestureKind::Drag)),
        (20., 32., 0.5, Some(GestureKind::Drag)),
        (20., 31., 0.125, None), // Authored gap between tap and drag.
        (20., 20., 0.2501, None),
        (20., 30.5, 0.125, None),
    ] {
        surface.clear(&mut map);
        send(&mut surface, &mut map, 1, start, Phase::Start, 0.);
        assert_eq!(
            send(&mut surface, &mut map, 1, end, Phase::End, seconds).map(|o| o.kind),
            expected
        );
    }
    surface.clear(&mut map);
    send(&mut surface, &mut map, 1, 20., Phase::Start, 0.);
    assert_eq!(
        send(&mut surface, &mut map, 1, 80., Phase::Move, 0.125)
            .unwrap()
            .kind,
        GestureKind::Drag
    );
    assert_eq!(
        send(&mut surface, &mut map, 1, 20., Phase::End, 0.25)
            .unwrap()
            .kind,
        GestureKind::Drag
    );
    send(&mut surface, &mut map, 1, 20., Phase::Start, 1.);
    send(&mut surface, &mut map, 1, 32., Phase::Move, 1.125);
    assert_eq!(
        send(&mut surface, &mut map, 1, 68., Phase::End, 1.5)
            .unwrap()
            .kind,
        GestureKind::Swipe
    );
}

#[test]
fn diagonal_logical_distance_and_stick_origin_are_independent_of_look_scale() {
    let (mut surface, mut map) = setup(GestureProfile::default());
    surface
        .set_layout(
            &[Control {
                output: Output::Look { x: 0, y: 1 },
                ..button(0)
            }],
            &mut map,
        )
        .unwrap();
    let pan = action(&mut map, InputCode::TouchDelta(0), ActionKind::Delta);
    surface
        .handle_timed_scaled(
            event(1, 20., 20., Phase::Start),
            0.,
            &mut map,
            Vec2::splat(100.),
        )
        .unwrap();
    assert_eq!(
        surface
            .handle_timed_scaled(
                event(1, 26., 28., Phase::Move),
                0.125,
                &mut map,
                Vec2::splat(100.)
            )
            .unwrap(),
        None
    );
    assert_eq!(map.peek(pan).value, 600.);
    let tap = surface
        .handle_timed_scaled(event(1, 26., 28., Phase::End), 0.25, &mut map, Vec2::ZERO)
        .unwrap()
        .unwrap();
    assert_eq!(tap.kind, GestureKind::Tap); // 6/8/10 triangle; not scaled to 1000 px.
    assert_eq!(map.peek(pan).value, 600.); // End never adds legacy look motion.
    surface
        .set_layout(
            &[Control {
                output: Output::Stick {
                    x: 0,
                    y: 1,
                    radius: 50.,
                    floating: false,
                },
                ..button(0)
            }],
            &mut map,
        )
        .unwrap();
    send(&mut surface, &mut map, 1, 20., Phase::Start, 0.);
    assert_eq!(surface.stick(0, 1).unwrap().0, Vec2::splat(50.));
    let tap = send(&mut surface, &mut map, 1, 20., Phase::End, 0.125).unwrap();
    assert_eq!(tap.origin, Vec2::splat(20.));
    assert_eq!(tap.kind, GestureKind::Tap);
}

#[test]
fn every_invalid_profile_field_and_late_field_rejects_atomically() {
    let base = GestureProfile::default();
    let mut invalid = Vec::new();
    for field in 0..3 {
        for value in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            -0.001,
            1_000_001.,
        ] {
            let mut p = base;
            match field {
                0 => p.tap_max_distance = value,
                1 => p.drag_min_distance = value,
                _ => p.swipe_min_distance = value,
            }
            invalid.push(p);
        }
    }
    for field in 0..3 {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.001, 3600.001] {
            let mut p = base;
            match field {
                0 => p.tap_max_seconds = value,
                1 => p.drag_min_seconds = value,
                _ => p.swipe_max_seconds = value,
            }
            invalid.push(p);
        }
    }
    invalid.extend([
        GestureProfile {
            tap_max_distance: 12.,
            ..base
        },
        GestureProfile {
            tap_max_distance: 13.,
            ..base
        },
        GestureProfile {
            drag_min_distance: 49.,
            ..base
        },
        GestureProfile {
            swipe_min_distance: 11.,
            ..base
        },
        GestureProfile {
            tap_max_distance: 1.,
            tap_max_seconds: 0.5,
            drag_min_distance: 2.,
            drag_min_seconds: 1.,
            swipe_min_distance: 3.,
            swipe_max_seconds: f64::NAN,
        },
    ]);
    for profile in invalid {
        let (mut surface, mut map) = setup(base);
        let jump = action(&mut map, InputCode::Touch(0), ActionKind::Button);
        send(&mut surface, &mut map, 1, 20., Phase::Start, 10.);
        let before = map.peek(jump);
        assert_eq!(profile.validate(), Err(GestureError::InvalidProfile));
        assert_eq!(
            surface.set_gesture_profile(Some(profile), &mut map),
            Err(GestureError::InvalidProfile)
        );
        assert_eq!(surface.gesture_profile(), Some(base));
        assert_eq!(surface.active(), 1);
        assert_eq!(map.peek(jump), before);
        assert_eq!(
            send(&mut surface, &mut map, 1, 30., Phase::End, 10.25)
                .unwrap()
                .kind,
            GestureKind::Tap
        );
    }
    for p in [
        GestureProfile {
            tap_max_distance: 0.,
            drag_min_distance: 0.001,
            swipe_min_distance: 0.001,
            tap_max_seconds: 0.,
            drag_min_seconds: 0.,
            swipe_max_seconds: 0.,
        },
        GestureProfile {
            tap_max_distance: 999_999.,
            drag_min_distance: 1e6,
            swipe_min_distance: 1e6,
            tap_max_seconds: 3600.,
            drag_min_seconds: 3600.,
            swipe_max_seconds: 3600.,
        },
    ] {
        p.validate().unwrap();
    }
}

#[test]
fn invalid_times_positions_scales_and_backwards_events_preserve_all_state() {
    for (event, time, scale, expected) in [
        (
            event(1, 90., 20., Phase::End),
            f64::NAN,
            Vec2::ONE,
            GestureError::InvalidTime,
        ),
        (
            event(1, 90., 20., Phase::Cancel),
            f64::INFINITY,
            Vec2::ONE,
            GestureError::InvalidTime,
        ),
        (
            event(1, 90., 20., Phase::Start),
            f64::NEG_INFINITY,
            Vec2::ONE,
            GestureError::InvalidTime,
        ),
        (
            event(1, 90., 20., Phase::Move),
            -0.001,
            Vec2::ONE,
            GestureError::InvalidTime,
        ),
        (
            event(1, 90., 20., Phase::Move),
            9.,
            Vec2::ONE,
            GestureError::TimeWentBackwards,
        ),
        (
            event(1, f32::NAN, 20., Phase::End),
            1000.,
            Vec2::ONE,
            GestureError::Input(InputError::InvalidTouch),
        ),
        (
            event(1, f32::INFINITY, 20., Phase::Cancel),
            1000.,
            Vec2::ONE,
            GestureError::Input(InputError::InvalidTouch),
        ),
        (
            event(1, -1_000_001., 20., Phase::Start),
            1000.,
            Vec2::ONE,
            GestureError::Input(InputError::InvalidTouch),
        ),
        (
            event(1, 90., 20., Phase::Move),
            1000.,
            Vec2::splat(f32::NAN),
            GestureError::Input(InputError::InvalidTouch),
        ),
        (
            event(1, 90., 20., Phase::Move),
            1000.,
            Vec2::splat(f32::INFINITY),
            GestureError::Input(InputError::InvalidTouch),
        ),
        (
            event(1, 90., 20., Phase::Move),
            1000.,
            Vec2::new(0., -100.001),
            GestureError::Input(InputError::InvalidTouch),
        ),
    ] {
        let (mut surface, mut map) = setup(GestureProfile::default());
        let jump = action(&mut map, InputCode::Touch(0), ActionKind::Button);
        send(&mut surface, &mut map, 1, 20., Phase::Start, 10.);
        let before = map.peek(jump);
        assert_eq!(
            surface.handle_timed_scaled(event, time, &mut map, scale),
            Err(expected)
        );
        assert_eq!(surface.active(), 1);
        assert_eq!(map.peek(jump), before);
        // Rejection did not advance watermark, replace origin or record excursion.
        assert_eq!(
            send(&mut surface, &mut map, 1, 20., Phase::End, 10.125)
                .unwrap()
                .kind,
            GestureKind::Tap
        );
    }
}

#[test]
fn uncaptured_events_enforce_one_global_monotonic_domain_but_never_gesture() {
    let (mut surface, mut map) = setup(GestureProfile::default());
    for phase in [Phase::Move, Phase::End, Phase::Cancel] {
        assert_eq!(send(&mut surface, &mut map, 99, 20., phase, 1.), None);
    }
    assert_eq!(
        send(&mut surface, &mut map, 1, 200., Phase::Start, 2.),
        None
    );
    assert_eq!(surface.active(), 0);
    assert_eq!(
        surface.handle_timed(event(1, 20., 20., Phase::Start), 1., &mut map),
        Err(GestureError::TimeWentBackwards)
    );
    send(&mut surface, &mut map, 1, 20., Phase::Start, 2.);
    // Global watermark spans pointers; a new pointer cannot provide older time.
    assert_eq!(
        surface.handle_timed(event(2, 20., 20., Phase::Start), 1.5, &mut map),
        Err(GestureError::TimeWentBackwards)
    );
    assert_eq!(
        send(&mut surface, &mut map, 1, 20., Phase::End, 2.)
            .unwrap()
            .elapsed_seconds,
        0.
    );
    surface.clear(&mut map);
    // Nonnegative f64 domain has no arbitrary session-length cap.
    send(&mut surface, &mut map, 1, 20., Phase::Start, f64::MAX);
    assert_eq!(
        send(&mut surface, &mut map, 1, 20., Phase::End, f64::MAX)
            .unwrap()
            .kind,
        GestureKind::Tap
    );
}

#[test]
fn cancellation_and_resets_preserve_other_input_edges_and_never_tap_or_swipe() {
    let (mut surface, mut map) = setup(GestureProfile::default());
    let jump = action(&mut map, InputCode::Touch(0), ActionKind::Button);
    let key = map
        .try_action(vec![Binding::digital(InputCode::Key(32))])
        .unwrap();
    let pan = map
        .named_action(
            "pan",
            ActionKind::Delta,
            ContextId::GLOBAL,
            vec![Binding::digital(InputCode::MouseDelta(0))],
        )
        .unwrap();
    for reset in 0..4 {
        surface.clear(&mut map);
        map.consume_tick(jump);
        map.feed(InputCode::Key(32), 1.);
        map.feed(InputCode::Key(32), 0.);
        map.feed_delta(InputCode::MouseDelta(0), 7.);
        send(&mut surface, &mut map, 1, 20., Phase::Start, 10.);
        send(&mut surface, &mut map, 2, 20., Phase::Start, 10.);
        let untouched = [map.peek(key), map.peek(pan)];
        match reset {
            0 => {
                assert_eq!(
                    send(&mut surface, &mut map, 1, 80., Phase::Cancel, 10.125),
                    None
                );
                assert_eq!(map.peek(jump).value, 1.); // Other capture still owns button.
                assert_eq!(
                    send(&mut surface, &mut map, 2, 20., Phase::Cancel, 10.125),
                    None
                );
            }
            1 => surface.clear(&mut map), // Timestamp-free focus loss.
            2 => surface
                .set_layout(
                    &[Control {
                        region: Region::Rect(Rect::new(0., 0., 200., 200.)),
                        ..button(0)
                    }],
                    &mut map,
                )
                .unwrap(),
            _ => surface
                .set_gesture_profile(
                    Some(GestureProfile {
                        tap_max_seconds: 0.5,
                        ..GestureProfile::default()
                    }),
                    &mut map,
                )
                .unwrap(),
        }
        assert_eq!(surface.active(), 0);
        assert_eq!([map.peek(key), map.peek(pan)], untouched);
        assert_eq!(
            map.peek(jump),
            Action {
                value: 0.,
                pressed: true,
                released: true
            }
        );
        // Cancel keeps its watermark; explicit reset forgets it.
        let time = if reset == 0 { 10.125 } else { 0. };
        for id in [1, 2] {
            assert_eq!(
                send(&mut surface, &mut map, id, 80., Phase::End, time),
                None
            );
        }
        assert!(map.consume_tick(key).pressed);
        assert_eq!(map.consume_tick(pan).value, 7.);
    }
}

#[test]
fn identical_profile_layout_and_invalid_layout_preserve_captures_but_disable_releases() {
    let profile = GestureProfile::default();
    let (mut surface, mut map) = setup(profile);
    send(&mut surface, &mut map, 1, 20., Phase::Start, 10.);
    surface
        .set_gesture_profile(Some(profile), &mut map)
        .unwrap();
    surface.set_layout(&[button(0)], &mut map).unwrap();
    let mut bad = [button(0); MAX_TOUCH_CONTROLS];
    bad[MAX_TOUCH_CONTROLS - 1].output = Output::Button(64);
    assert_eq!(
        surface.set_layout(&bad, &mut map),
        Err(InputError::InvalidTouch)
    );
    assert_eq!(
        surface.set_layout(&[button(0); MAX_TOUCH_CONTROLS + 1], &mut map),
        Err(InputError::TouchCapacity)
    );
    assert_eq!(
        surface.handle_timed(event(99, 20., 20., Phase::Move), 9., &mut map),
        Err(GestureError::TimeWentBackwards)
    );
    assert_eq!(
        send(&mut surface, &mut map, 1, 20., Phase::End, 10.125)
            .unwrap()
            .kind,
        GestureKind::Tap
    );
    send(&mut surface, &mut map, 1, 20., Phase::Start, 11.);
    surface.set_gesture_profile(None, &mut map).unwrap();
    assert_eq!(surface.active(), 0);
    assert_eq!(map.physical_value(InputCode::Touch(0)), 0.);
    assert_eq!(
        surface.handle_timed(event(1, f32::NAN, 20., Phase::End), f64::NAN, &mut map),
        Err(GestureError::Disabled)
    );
    surface
        .set_gesture_profile(Some(profile), &mut map)
        .unwrap();
    assert_eq!(send(&mut surface, &mut map, 1, 20., Phase::End, 0.), None);
}

#[test]
fn capacity_layout_priority_duplicate_starts_and_multi_pointer_delivery() {
    let (mut surface, mut map) = setup(GestureProfile::default());
    let layout: [Control; MAX_TOUCH_CONTROLS] = std::array::from_fn(|i| Control {
        region: Region::Rect(Rect::new(i as f32 * 100., 0., 100., 100.)),
        output: Output::Button(i as u8),
    });
    surface.set_layout(&layout, &mut map).unwrap();
    let actions: [usize; MAX_TOUCHES] = std::array::from_fn(|i| {
        map.try_action(vec![Binding::digital(InputCode::Touch(i as u8))])
            .unwrap()
    });
    for id in 0..MAX_TOUCHES as u64 {
        send(
            &mut surface,
            &mut map,
            id,
            id as f32 * 100. + 20.,
            Phase::Start,
            10.,
        );
    }
    assert_eq!(surface.active(), MAX_TOUCHES);
    assert_eq!(
        surface.handle_timed(event(100, 20., 20., Phase::Start), 1000., &mut map),
        Err(GestureError::Input(InputError::TouchCapacity))
    );
    assert_eq!(
        send(&mut surface, &mut map, 100, -200., Phase::Start, 10.),
        None
    );
    // Duplicate Start replaces in place while full, resetting origin/time/control.
    send(&mut surface, &mut map, 0, 120., Phase::Start, 10.125);
    assert_eq!(surface.active(), MAX_TOUCHES);
    assert_eq!(map.physical_value(InputCode::Touch(0)), 0.);
    let restarted = send(&mut surface, &mut map, 0, 120., Phase::End, 10.25).unwrap();
    assert_eq!((restarted.control, restarted.elapsed_seconds), (1, 0.125));
    assert_eq!(map.physical_value(InputCode::Touch(1)), 1.); // Pointer 1 still holds it.
    for id in 1..MAX_TOUCHES as u64 {
        let observed = send(
            &mut surface,
            &mut map,
            id,
            id as f32 * 100. + 20.,
            Phase::End,
            10.25,
        )
        .unwrap();
        assert_eq!(
            (observed.id, observed.control, observed.kind),
            (id, id as usize, GestureKind::Tap)
        );
        assert_eq!(map.physical_value(InputCode::Touch(id as u8)), 0.);
    }
    assert_eq!(surface.active(), 0);
    for id in actions {
        assert_eq!(
            map.consume_tick(id),
            Action {
                value: 0.,
                pressed: true,
                released: true
            }
        );
        assert_eq!(map.consume_tick(id), Action::default());
    }
    surface
        .set_layout(&[button(3), button(4)], &mut map)
        .unwrap();
    send(&mut surface, &mut map, 1, 20., Phase::Start, 0.);
    assert_eq!(map.physical_value(InputCode::Touch(3)), 1.);
    assert_eq!(map.physical_value(InputCode::Touch(4)), 0.);
    assert_eq!(
        send(&mut surface, &mut map, 1, 20., Phase::End, 0.125)
            .unwrap()
            .control,
        0
    );
    send(&mut surface, &mut map, 1, 20., Phase::Start, 1.);
    send(&mut surface, &mut map, 1, -200., Phase::Start, 1.125);
    assert_eq!(surface.active(), 0);
    assert_eq!(send(&mut surface, &mut map, 1, 20., Phase::End, 1.25), None);
}

#[test]
fn untimed_events_drop_only_affected_observation_and_cannot_revive_missing_start() {
    let (mut surface, mut map) = setup(GestureProfile::default());
    for id in [1, 2] {
        send(&mut surface, &mut map, id, 20., Phase::Start, 0.);
    }
    surface
        .handle(event(1, 21., 20., Phase::Move), &mut map)
        .unwrap();
    assert_eq!(
        send(&mut surface, &mut map, 1, 21., Phase::End, 0.125),
        None
    );
    assert_eq!(
        send(&mut surface, &mut map, 2, 20., Phase::End, 0.125)
            .unwrap()
            .kind,
        GestureKind::Tap
    );
    surface
        .handle(event(1, 20., 20., Phase::Start), &mut map)
        .unwrap();
    assert_eq!(
        send(&mut surface, &mut map, 1, 80., Phase::Move, 0.25),
        None
    );
    assert_eq!(send(&mut surface, &mut map, 1, 80., Phase::End, 0.5), None);
    send(&mut surface, &mut map, 1, 20., Phase::Start, 1.);
    // Invalid untimed events cannot discard a valid observation.
    assert!(
        surface
            .handle(event(1, f32::NAN, 20., Phase::Move), &mut map)
            .is_err()
    );
    assert_eq!(
        send(&mut surface, &mut map, 1, 20., Phase::End, 1.125)
            .unwrap()
            .kind,
        GestureKind::Tap
    );
}

#[test]
fn opt_in_and_disabled_paths_preserve_all_legacy_channel_snapshots() {
    let layout = [
        button(0),
        Control {
            region: Region::Rect(Rect::new(100., 0., 100., 100.)),
            output: Output::Stick {
                x: 1,
                y: 2,
                radius: 20.,
                floating: true,
            },
        },
        Control {
            region: Region::Rect(Rect::new(200., 0., 100., 100.)),
            output: Output::Look { x: 0, y: 1 },
        },
    ];
    let mut maps: [ActionMap; 3] = std::array::from_fn(|_| ActionMap::default());
    let mut surfaces: [TouchSurface; 3] = std::array::from_fn(|_| TouchSurface::default());
    for (i, (surface, map)) in surfaces.iter_mut().zip(&mut maps).enumerate() {
        surface.set_layout(&layout, map).unwrap();
        for (n, (code, kind)) in [
            (InputCode::Touch(0), ActionKind::Button),
            (InputCode::Touch(1), ActionKind::Axis),
            (InputCode::Touch(2), ActionKind::Axis),
            (InputCode::TouchDelta(0), ActionKind::Delta),
            (InputCode::TouchDelta(1), ActionKind::Delta),
        ]
        .into_iter()
        .enumerate()
        {
            map.named_action(
                &format!("action{n}"),
                kind,
                ContextId::GLOBAL,
                vec![Binding::digital(code)],
            )
            .unwrap();
        }
        if i > 0 {
            surface
                .set_gesture_profile(Some(GestureProfile::default()), map)
                .unwrap();
        }
    }
    for (n, event) in [
        event(1, 20., 20., Phase::Start),
        event(2, 120., 20., Phase::Start),
        event(3, 220., 20., Phase::Start),
        event(2, 160., 20., Phase::Move),
        event(3, 232., 12., Phase::Move),
        event(1, 90., 90., Phase::End),
        event(3, 299., 99., Phase::End),
        event(2, 120., 20., Phase::Start),
        event(2, 130., 40., Phase::Move),
        event(2, -200., 20., Phase::Cancel),
        event(4, -200., 20., Phase::Start),
    ]
    .into_iter()
    .enumerate()
    {
        for i in 0..3 {
            if i == 2 {
                surfaces[i]
                    .handle_timed_scaled(event, n as f64 / 64., &mut maps[i], Vec2::new(-2., 0.5))
                    .unwrap();
            } else {
                surfaces[i]
                    .handle_scaled(event, &mut maps[i], Vec2::new(-2., 0.5))
                    .unwrap();
            }
        }
        for id in 0..5 {
            assert_eq!(maps[0].peek(id), maps[1].peek(id));
            assert_eq!(maps[0].peek(id), maps[2].peek(id));
        }
        assert_eq!(surfaces[0].active(), surfaces[2].active());
    }
    assert_eq!(maps[0].peek(3).value, -24.);
    assert_eq!(maps[0].peek(4).value, -4.);
    for map in &mut maps {
        let tap = map.consume_tick(0);
        assert!(tap.pressed && tap.released);
        assert_eq!(map.consume_tick(0), Action::default());
    }
}

#[test]
fn extreme_valid_profiles_coordinates_and_cancelled_drag_do_not_leak_outcomes() {
    let zero_time = GestureProfile {
        tap_max_distance: 0.,
        tap_max_seconds: 0.,
        drag_min_distance: 1.,
        drag_min_seconds: 3600.,
        swipe_min_distance: 1.,
        swipe_max_seconds: 0.,
    };
    let (mut surface, mut map) = setup(zero_time);
    send(&mut surface, &mut map, 1, 20., Phase::Start, 0.);
    assert_eq!(
        send(&mut surface, &mut map, 1, 20., Phase::End, 0.)
            .unwrap()
            .kind,
        GestureKind::Tap
    );
    send(&mut surface, &mut map, 1, 20., Phase::Start, 0.);
    // Swipe's own time and net-distance thresholds do not require a prior drag.
    assert_eq!(
        send(&mut surface, &mut map, 1, 21., Phase::End, 0.)
            .unwrap()
            .kind,
        GestureKind::Swipe
    );
    send(&mut surface, &mut map, 1, 20., Phase::Start, 0.);
    assert_eq!(
        send(&mut surface, &mut map, 1, 21., Phase::Move, 3599.),
        None
    );
    assert_eq!(
        send(&mut surface, &mut map, 1, 21., Phase::Move, 3600.)
            .unwrap()
            .kind,
        GestureKind::Drag
    );
    assert_eq!(
        send(&mut surface, &mut map, 1, 1e6, Phase::Cancel, 3600.),
        None
    );
    assert_eq!(
        send(&mut surface, &mut map, 1, 21., Phase::End, 3600.),
        None
    );
    surface
        .set_gesture_profile(Some(GestureProfile::default()), &mut map)
        .unwrap();
    surface
        .set_layout(
            &[Control {
                region: Region::Circle {
                    center: Vec2::splat(-1e6),
                    radius: 1.,
                },
                output: Output::Button(0),
            }],
            &mut map,
        )
        .unwrap();
    surface
        .handle_timed(event(1, -1e6, -1e6, Phase::Start), 0., &mut map)
        .unwrap();
    let swipe = surface
        .handle_timed(event(1, 1e6, 1e6, Phase::End), 0.5, &mut map)
        .unwrap()
        .unwrap();
    assert_eq!(swipe.kind, GestureKind::Swipe);
    assert_eq!(swipe.position, Vec2::splat(1e6));
}

#[test]
fn finite_subnormal_distance_thresholds_keep_their_order() {
    let tiny = f32::from_bits(1);
    let profile = GestureProfile {
        tap_max_distance: 0.,
        drag_min_distance: tiny * 2.,
        swipe_min_distance: tiny * 4.,
        ..GestureProfile::default()
    };
    let (mut surface, mut map) = setup(profile);
    surface
        .handle_timed(event(1, 0., 0., Phase::Start), 0., &mut map)
        .unwrap();
    assert_eq!(
        surface
            .handle_timed(event(1, tiny, 0., Phase::End), 0., &mut map)
            .unwrap(),
        None
    );
    surface
        .handle_timed(event(1, 0., 0., Phase::Start), 0., &mut map)
        .unwrap();
    assert_eq!(
        surface
            .handle_timed(event(1, tiny * 2., 0., Phase::End), 0., &mut map)
            .unwrap()
            .unwrap()
            .kind,
        GestureKind::Drag
    );
    surface
        .handle_timed(event(1, 0., 0., Phase::Start), 0., &mut map)
        .unwrap();
    assert_eq!(
        surface
            .handle_timed(event(1, tiny * 4., 0., Phase::End), 0., &mut map)
            .unwrap()
            .unwrap()
            .kind,
        GestureKind::Swipe
    );
}

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
}
struct Allocator;
fn allocated() {
    let _ = RECORD.try_with(|on| {
        if on.get() {
            let _ = CALLS.try_with(|n| n.set(n.get() + 1));
        }
    });
}
// SAFETY: All allocation contracts pass through unchanged to System. Counters
// are nonallocating thread locals and tolerate TLS teardown.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        allocated();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        allocated();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        allocated();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

#[test]
fn warmed_full_capacity_observation_reset_errors_and_ticks_do_not_allocate() {
    let profile = GestureProfile::default();
    let (mut surface, mut map) = setup(profile);
    let jump = action(&mut map, InputCode::Touch(0), ActionKind::Button);
    let layout = [button(0); MAX_TOUCH_CONTROLS];
    surface.set_layout(&layout, &mut map).unwrap();
    // Warm every exercised path before recording.
    for measured in [false, true] {
        CALLS.with(|n| n.set(0));
        RECORD.with(|on| on.set(measured));
        for _ in 0..1000 {
            surface.clear(&mut map);
            surface.set_layout(&layout, &mut map).unwrap();
            surface
                .set_gesture_profile(Some(profile), &mut map)
                .unwrap();
            for id in 0..MAX_TOUCHES as u64 {
                send(&mut surface, &mut map, id, 20., Phase::Start, 0.);
            }
            black_box(
                surface
                    .handle_timed(event(99, 20., 20., Phase::Start), 1000., &mut map)
                    .unwrap_err(),
            );
            black_box(
                surface
                    .set_gesture_profile(
                        Some(GestureProfile {
                            swipe_max_seconds: f64::NAN,
                            ..profile
                        }),
                        &mut map,
                    )
                    .unwrap_err(),
            );
            for id in 0..MAX_TOUCHES as u64 {
                black_box(send(&mut surface, &mut map, id, 32., Phase::Move, 0.125).unwrap());
            }
            for id in 0..MAX_TOUCHES as u64 {
                black_box(send(&mut surface, &mut map, id, 68., Phase::End, 0.25).unwrap());
            }
            black_box(map.consume_tick(jump));
            surface.set_gesture_profile(None, &mut map).unwrap();
            surface
                .set_gesture_profile(Some(profile), &mut map)
                .unwrap();
        }
        RECORD.with(|on| on.set(false));
        assert_eq!(CALLS.with(Cell::get), 0);
    }
}
