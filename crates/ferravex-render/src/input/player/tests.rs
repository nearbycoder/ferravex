use super::*;
fn player() -> PlayerInput {
    crate::edit(|s| *s = crate::State::default());
    let mut p = PlayerInput::transient("input-test");
    p.action(
        "jump",
        "Jump",
        ActionKind::Button,
        vec![
            Binding::digital(InputCode::Key(32)),
            Binding::digital(InputCode::PadButton(0)),
            Binding::digital(InputCode::Touch(3)),
        ],
    )
    .unwrap();
    p.action(
        "move_x",
        "Move",
        ActionKind::Axis,
        vec![
            Binding::digital(InputCode::Key(100)),
            Binding::digital(InputCode::PadAxis(0)),
            Binding::digital(InputCode::Touch(0)),
        ],
    )
    .unwrap();
    p.action(
        "look_x",
        "Look",
        ActionKind::Delta,
        vec![
            Binding::digital(InputCode::MouseDelta(0)),
            Binding::digital(InputCode::TouchDelta(0)),
        ],
    )
    .unwrap();
    p.action(
        "look_rate_x",
        "Stick look",
        ActionKind::Axis,
        vec![Binding::digital(InputCode::PadAxis(2))],
    )
    .unwrap();
    p.global_action(
        "confirm",
        "Play",
        ActionKind::Button,
        vec![Binding::digital(InputCode::Key(0xff0d))],
    )
    .unwrap();
    p
}
fn next() {
    crate::edit(|s| {
        s.frame += 1;
        s.input.finish_frame();
        s.pressed.clear();
        s.clicks.clear();
    });
}

#[test]
fn custom_pause_screen_keeps_controls_editor_without_a_second_banner() {
    let mut p = player();
    p.set_paused(true);
    p.draw_controls();
    assert!(p.paused());
    assert!(crate::read(|s| s.vertices.is_empty()));
    p.draw_settings();
    assert!(crate::read(|s| !s.vertices.is_empty()));
    crate::edit(|s| s.vertices.clear());
    p.open = true;
    p.draw_controls();
    assert!(p.controls_open());
    assert!(p.paused());
    assert!(crate::read(|s| !s.vertices.is_empty()));
}
#[test]
fn default_pause_badge_stays_compact_without_a_second_instruction_row() {
    for (width, height) in [(320., 480.), (844., 390.), (1280., 800.)] {
        let mut input = player();
        crate::edit(|s| {
            s.w = width as u32;
            s.h = height as u32;
        });
        input.set_paused(true);
        input.draw_settings();
        assert!(input.paused());
        crate::read(|s| {
            assert!(
                s.vertices.len() < 1500,
                "only a small badge and its title are needed"
            );
            let background = &s.vertices[..6];
            let min_x = background
                .iter()
                .map(|v| v[0])
                .fold(f32::INFINITY, f32::min);
            let max_x = background
                .iter()
                .map(|v| v[0])
                .fold(f32::NEG_INFINITY, f32::max);
            let min_y = background
                .iter()
                .map(|v| v[1])
                .fold(f32::INFINITY, f32::min);
            let max_y = background
                .iter()
                .map(|v| v[1])
                .fold(f32::NEG_INFINITY, f32::max);
            assert!((max_x - min_x) * width * 0.5 <= 160.1);
            assert!((max_y - min_y) * height * 0.5 <= 40.1);
        });
    }
}

fn pad_connected(connected: bool) {
    crate::edit(|s| s.input.pad_connected(0, connected));
}
fn pad_button(button: usize, value: f32) {
    crate::edit(|s| s.input.pad_button(0, button, value));
}
fn pad_axis(axis: usize, value: f32) {
    crate::edit(|s| s.input.pad_axis(0, axis, value));
}
#[test]
fn taps_wait_for_a_tick_while_global_menu_edges_expire() {
    let mut p = player();
    p.sample(true);
    next();
    crate::key(32, true);
    crate::key(32, false);
    crate::key(0xff0d, true);
    crate::key(0xff0d, false);
    p.sample(true);
    next();
    p.sample(true); // no simulation tick in the previous render frame
    assert!(!p.pressed("confirm"));
    let tap = p.take("jump");
    assert!(tap.pressed && tap.released && tap.value == 0.);
    assert!(!p.take("jump").pressed);
    crate::key(32, true);
    next();
    p.sample(true); // journal is cleared by next()
    assert!(!p.pressed("jump"));
    crate::key(32, false);
    next();
    crate::key(32, true);
    p.sample(true);
    assert!(p.pressed("jump"));
    assert_eq!(p.take("jump").value, 1.);
}
#[test]
fn relative_mouse_accumulates_once_and_drag_does_not_gate_touch() {
    let mut p = player();
    p.set_mouse_look_button(Some(1)).unwrap();
    crate::edit(|s| {
        s.w = 1000;
        s.h = 500;
    });
    crate::pointer(20., 20., 10., 0.);
    p.sample(true);
    assert_eq!(p.look(0.1), Vec2::ZERO);
    next();
    crate::button(1, true);
    crate::pointer(40., 20., 20., 0.);
    p.sample(true);
    p.sample(true);
    assert!((p.look(0.1).x - 0.038).abs() < 1e-6);
    assert_eq!(p.look(0.1), Vec2::ZERO);
    p.set_touch_layout(&[touch::Control {
        region: touch::Region::Rect(Rect::new(0., 0., 100., 100.)),
        output: touch::Output::Look { x: 0, y: 1 },
    }])
    .unwrap();
    next();
    crate::button(1, false);
    super::super::touch_event(touch::Event {
        id: 5,
        position: vec2(10., 10.),
        phase: touch::Phase::Start,
    });
    super::super::touch_event(touch::Event {
        id: 5,
        position: vec2(20., 10.),
        phase: touch::Phase::Move,
    });
    p.sample(true);
    assert!((p.look(0.1).x - 0.04).abs() < 1e-6);
}
#[test]
fn menus_and_focus_clear_edges_without_firing_on_resume() {
    let mut p = player();
    crate::key(32, true);
    p.sample(true);
    assert!(p.pressed("jump"));
    p.set_controls_open(true);
    assert_eq!(p.simulation_dt(0.016), 0.);
    p.set_controls_open(false);
    p.sample(true);
    assert!(!p.held("jump"));
    next();
    crate::key(32, false);
    crate::key(32, true);
    p.sample(true);
    assert!(p.pressed("jump"));
    crate::focus(false);
    next();
    p.sample(true);
    assert!(!p.held("jump"));
    assert_eq!(p.look(0.1), Vec2::ZERO);
    next();
    crate::focus(true);
    p.sample(true);
    assert!(p.gameplay_active());
    assert!(!p.pressed("jump"));
    next();
    crate::key(32, true);
    p.sample(false);
    assert!(!p.held("jump"));
    p.sample(true);
    assert!(!p.pressed("jump"));
}
#[test]
fn disconnected_events_and_hotplug_holds_require_neutral() {
    let mut p = player();
    pad_connected(true);
    pad_button(0, 1.);
    pad_axis(0, 0.8);
    p.sample(true);
    assert!(!p.held("jump"));
    assert_eq!(p.movement().x, 0.);
    next();
    pad_button(0, 0.);
    pad_axis(0, 0.);
    p.sample(true);
    next();
    pad_button(0, 1.);
    pad_axis(0, 1.);
    p.sample(true);
    assert!(p.pressed("jump"));
    assert_eq!(p.movement().x, 1.);
    next();
    pad_button(0, 0.);
    pad_button(0, 1.);
    pad_connected(false);
    p.sample(true);
    assert!(!p.held("jump"));
    assert_eq!(p.movement(), Vec2::ZERO);
    next();
    p.sample(true);
    assert!(!p.held("jump"));
    next();
    pad_connected(true);
    pad_button(0, 1.);
    p.sample(true);
    assert!(!p.pressed("jump"));
    next();
    pad_button(0, 0.);
    p.sample(true);
    next();
    pad_button(0, 1.);
    pad_button(0, 0.);
    p.sample(true);
    assert!(p.pressed("jump"));
}
#[test]
fn stick_look_is_frame_rate_independent_and_slot_changes_are_neutral() {
    let mut p = player();
    pad_connected(true);
    p.sample(true);
    next();
    pad_axis(2, 1.);
    p.sample(true);
    let x = p.look(1. / 60.).x;
    assert!((p.look(1. / 30.).x - x * 2.).abs() < 1e-6);
    next();
    let mut settings = p.settings();
    settings.gamepad = 1;
    p.set_settings(settings).unwrap();
    p.sample(true);
    assert_eq!(p.look(0.1), Vec2::ZERO);
}
#[test]
fn captures_preserve_other_devices_and_reject_reserved_imports_atomically() {
    let mut p = player();
    p.set_controls_open(true);
    p.begin_capture("jump", CaptureDevice::KeyboardMouse)
        .unwrap();
    crate::key(106, true);
    p.sample(true);
    let bindings = p.bindings("jump").unwrap();
    assert!(bindings.iter().any(|b| b.code == InputCode::Key(106)));
    assert!(bindings.iter().any(|b| b.code == InputCode::PadButton(0)));
    assert!(bindings.iter().any(|b| b.code == InputCode::Touch(3)));
    assert!(!bindings.iter().any(|b| b.code == InputCode::Key(32)));
    assert_eq!(p.status(), ControlsStatus::Transient);
    let before = p.export_config().unwrap();
    assert_eq!(
        p.rebind("jump", vec![Binding::digital(InputCode::Key(0xffbf))]),
        Err(InputError::InvalidBinding)
    );
    let id = p.map.action_id("jump").unwrap();
    p.map
        .try_rebind(id, vec![Binding::digital(InputCode::PadButton(16))])
        .unwrap();
    let invalid = p.export_config().unwrap();
    p.import_config(&before).unwrap();
    assert_eq!(p.import_config(&invalid), Err(InputError::InvalidBinding));
    assert_eq!(p.export_config().unwrap(), before);
    let mut corrupt = before.clone();
    corrupt[10] ^= 1;
    assert!(p.import_config(&corrupt).is_err());
    assert_eq!(p.export_config().unwrap(), before);
    p.reset_defaults();
    assert!(
        p.bindings("jump")
            .unwrap()
            .iter()
            .any(|b| b.code == InputCode::Key(32))
    );
}
#[test]
fn overflow_discards_entire_packet_and_touch_mouse_does_not_fire_real_mouse() {
    let mut p = player();
    p.action(
        "fire",
        "Fire",
        ActionKind::Button,
        vec![Binding::digital(InputCode::Mouse(1))],
    )
    .unwrap();
    crate::button_source(1, true, true);
    p.sample(true);
    assert!(!p.held("fire"));
    next();
    crate::button(1, true);
    p.sample(true);
    assert!(
        p.pressed("fire"),
        "a touch-emulated legacy mouse hold must not mask a real mouse press"
    );
    next();
    crate::button(1, false);
    p.sample(true);
    assert!(!p.held("fire"));
    next();
    crate::button_source(1, false, true);
    crate::key(32, true);
    for _ in 0..MAX_EVENTS {
        crate::pointer(10., 10., 1., 0.);
    }
    p.sample(true);
    assert!(!p.pressed("jump"));
    assert!(!p.held("jump"));
    assert_eq!(p.look(0.1), Vec2::ZERO);
    next();
    p.sample(true);
    assert!(p.gameplay_active());
    assert!(!p.pressed("jump"));
}
#[test]
fn edited_touch_capture_matches_visual_regions_and_edits_release_holds() {
    let mut p = player();
    crate::edit(|s| {
        s.w = 320;
        s.h = 200;
    });
    let control = touch::Control {
        region: touch::Region::Circle {
            center: vec2(40., 140.),
            radius: 24.,
        },
        output: touch::Output::Button(3),
    };
    p.set_touch_settings(touch::TouchLayoutSettings {
        mirrored: true,
        ..Default::default()
    })
    .unwrap();
    p.set_touch_layout(&[control]).unwrap();
    let center = match p.surface.controls()[0].region {
        touch::Region::Circle { center, .. } => center,
        _ => unreachable!(),
    };
    assert_eq!(center, vec2(280., 140.));
    super::super::touch_event(touch::Event {
        id: 19,
        position: center,
        phase: touch::Phase::Start,
    });
    p.sample(true);
    assert!(p.held("jump"));
    p.set_touch_settings(touch::TouchLayoutSettings::default())
        .unwrap();
    assert!(!p.held("jump"));
    let old = p.touch_settings();
    assert!(
        p.set_touch_settings(touch::TouchLayoutSettings {
            scale: f32::NAN,
            ..old
        })
        .is_err()
    );
    assert_eq!(p.touch_settings(), old);
}

#[test]
fn tick_snapshots_preserve_quick_taps_across_zero_tick_frames() {
    let mut p = player();
    p.sample(true);
    next();
    crate::key(32, true);
    crate::key(32, false);
    crate::key(100, true);
    crate::pointer(20., 20., 10., 0.);
    p.sample(true);
    p.sample(true);
    let pending_look = p.value("look_x");
    assert!(pending_look > 0.);
    next();
    p.sample(true); // No fixed tick ran in the previous render frame.
    let tick = p.take_tick(["jump", "move_x", "look_x"]);
    assert_eq!(
        tick[0],
        Action {
            value: 0.,
            pressed: true,
            released: true,
        }
    );
    assert_eq!(tick[1].value, 1.);
    assert_eq!(tick[2].value, pending_look);
    let animation = tick;
    let physics = tick;
    assert_eq!(animation, physics); // Consumers share the same immutable tick.
    for _ in 0..4 {
        p.sample(true); // Even accidental re-sampling must not replay input.
        let [jump, movement, look] = p.take_tick(["jump", "move_x", "look_x"]);
        assert_eq!(jump, Action::default());
        assert_eq!(look, Action::default());
        assert_eq!(movement.value, 1.);
        assert!(!movement.pressed && !movement.released);
    }
}

#[test]
fn tick_snapshot_duplicates_share_edges_and_leave_other_actions_pending() {
    let mut p = player();
    crate::key(32, true);
    crate::key(100, true);
    crate::pointer(20., 20., 10., 0.);
    p.sample(true);
    assert_eq!(p.take_tick([]), []);
    let [jump, missing, same_jump, look, same_look] =
        p.take_tick(["jump", "missing", "jump", "look_x", "look_x"]);
    assert!(jump.pressed && look.pressed);
    assert_eq!(jump, same_jump);
    assert_eq!(look, same_look);
    assert_eq!(missing, Action::default());
    assert!(p.take("move_x").pressed, "unnamed actions remain pending");
    let [jump, look] = p.take_tick(["jump", "look_x"]);
    assert_eq!(jump.value, 1.);
    assert!(!jump.pressed);
    assert_eq!(look, Action::default());
    next();
    crate::key(32, false);
    p.sample(true);
    assert!(p.take_tick(["jump"])[0].released);
    assert_eq!(p.take_tick(["jump"])[0], Action::default());
}

#[test]
fn explicit_pause_immediately_stops_gameplay_and_discards_pending_input() {
    let mut p = player();
    crate::key(32, true);
    crate::key(100, true);
    crate::pointer(20., 20., 10., 0.);
    p.sample_play();
    p.set_paused(true);
    assert!(p.paused());
    assert!(!p.gameplay_active());
    assert_eq!(p.simulation_dt(0.016), 0.);
    assert_eq!(p.take_tick(["jump", "look_x"]), [Action::default(); 2]);
    next();
    crate::key(0xff0d, true);
    p.sample_play();
    assert!(p.pressed("confirm"), "global menu input remains usable");
    p.set_paused(false);
    assert!(p.gameplay_active());
    p.sample_play();
    assert_eq!(p.take_tick(["jump", "look_x"]), [Action::default(); 2]);
    assert_eq!(p.movement().x, 1., "held axes can resume at their level");
    next();
    crate::key(32, false);
    crate::key(32, true);
    p.sample_play();
    assert!(p.take_tick(["jump"])[0].pressed);
    p.set_paused(true);
    p.sample(true);
    assert!(p.gameplay_active(), "explicit sampling owns its enablement");
}

#[test]
fn pause_action_is_once_per_frame_and_requires_release_to_resume() {
    let mut p = player();
    p.global_action(
        "pause",
        "Pause",
        ActionKind::Button,
        vec![Binding::digital(InputCode::Key(112))],
    )
    .unwrap();
    p.sample_play();
    next();
    crate::key(32, true);
    crate::key(112, true);
    p.sample_play();
    assert!(p.paused());
    assert_eq!(p.take_tick(["jump"])[0], Action::default());
    p.sample_play();
    assert!(p.paused());
    next();
    crate::key(112, true); // Host key repeat.
    p.sample_play();
    assert!(p.paused());
    next();
    crate::key(112, false);
    crate::key(112, true);
    p.sample_play();
    p.sample_play();
    assert!(!p.paused());
    assert_eq!(p.take_tick(["jump"])[0], Action::default());
}

#[test]
fn disabled_sampling_cancels_touch_gestures_before_and_during_pause() {
    let mut p = player();
    p.set_touch_layout(&[touch::Control {
        region: touch::Region::Rect(Rect::new(0., 0., 100., 100.)),
        output: touch::Output::Look { x: 0, y: 1 },
    }])
    .unwrap();
    let touch = |phase, x| {
        super::super::touch_event(touch::Event {
            id: 1,
            position: vec2(x, 10.),
            phase,
        });
    };
    touch(touch::Phase::Start, 10.);
    touch(touch::Phase::Move, 20.);
    p.sample(true);
    assert!(p.touch_active());
    assert!(p.value("look_x") > 0.);
    p.sample(false);
    assert!(!p.touch_active());
    p.sample(true);
    assert_eq!(p.look(0.1), Vec2::ZERO);
    next();
    touch(touch::Phase::Move, 30.);
    p.sample(true);
    assert_eq!(p.look(0.1), Vec2::ZERO);
    next();
    touch(touch::Phase::Start, 10.);
    p.sample(false);
    p.sample(true); // A gesture begun while paused must also be cancelled.
    next();
    touch(touch::Phase::Move, 20.);
    p.sample(true);
    assert_eq!(p.look(0.1), Vec2::ZERO);
    next();
    touch(touch::Phase::End, 20.);
    touch(touch::Phase::Start, 10.);
    touch(touch::Phase::Move, 20.);
    p.sample(true);
    assert!(p.look(0.1).x > 0.);
}

#[test]
fn repeated_sampling_observes_blur_refocus_and_external_modal_changes() {
    let mut p = player();
    pad_connected(true);
    p.sample(true);
    next();
    crate::key(32, true);
    crate::key(100, true);
    pad_button(0, 1.);
    p.sample(true);
    assert!(p.held("jump"));
    crate::focus(false);
    p.sample(true); // No new render frame between sampling and focus loss.
    assert!(!p.gameplay_active());
    assert_eq!(p.take_tick(["jump", "move_x"]), [Action::default(); 2]);
    crate::focus(true);
    p.sample(true);
    assert!(p.gameplay_active());
    assert_eq!(p.take_tick(["jump", "move_x"]), [Action::default(); 2]);
    next();
    p.sample(true);
    assert!(!p.held("jump"), "held pad must neutralize after refocus");
    next();
    pad_button(0, 0.);
    pad_button(0, 1.);
    p.sample(true);
    assert!(p.pressed("jump"));
    crate::edit(|s| s.details_open = true);
    p.sample(true);
    assert!(p.controls_open());
    assert!(!p.gameplay_active());
    crate::edit(|s| s.details_open = false);
    p.sample(true);
    assert!(!p.controls_open());
    assert!(p.gameplay_active());
    assert!(!p.pressed("jump"));
}

#[test]
fn reserved_keyboard_repeats_do_not_retoggle_controls_or_cancel_capture() {
    let mut p = player();
    let key = |code, value| {
        crate::edit(|s| {
            assert!(
                s.input
                    .push(PlatformEvent::Value(InputCode::Key(code), value))
            );
        });
    };
    key(0xffbf, 1.);
    key(0xffbf, 1.);
    p.sample(true);
    assert!(p.controls_open());
    next();
    key(0xffbf, 1.);
    p.sample(true);
    assert!(p.controls_open());
    p.begin_capture("jump", CaptureDevice::KeyboardMouse)
        .unwrap();
    next();
    key(0xff1b, 1.);
    key(0xff1b, 1.);
    p.sample(true);
    assert!(p.capture.is_none());
    assert!(
        p.controls_open(),
        "repeat must not close after cancelling capture"
    );
    next();
    key(0xff1b, 0.);
    key(0xff1b, 1.);
    p.sample(true);
    assert!(!p.controls_open());
    next();
    key(0xffbf, 0.);
    key(0xffbf, 1.);
    p.sample(true);
    assert!(p.controls_open(), "a fresh press still toggles controls");
}

#[test]
fn reserved_pad_button_uses_threshold_edges_within_one_packet() {
    let mut p = player();
    pad_connected(true);
    p.sample(true);
    next();
    pad_button(16, 0.6);
    pad_button(16, 0.9);
    p.sample(true);
    assert!(
        p.controls_open(),
        "one press despite changing analog pressure"
    );
    next();
    pad_button(16, 0.7);
    p.sample(true);
    assert!(p.controls_open());
    next();
    pad_button(16, 0.);
    pad_button(16, 1.);
    pad_button(16, 0.);
    pad_button(16, 1.);
    p.sample(true);
    assert!(p.controls_open(), "two fresh presses toggle twice");
}

#[test]
fn hints_format_every_binding_family_and_signed_scales() {
    let mut p = player();
    p.action(
        "mixed",
        "Mixed",
        ActionKind::Axis,
        vec![
            Binding {
                code: InputCode::Key(97),
                scale: -1.,
                deadzone: 0.,
            },
            Binding::digital(InputCode::Key(100)),
            Binding::digital(InputCode::Mouse(5)),
            Binding::digital(InputCode::PadButton(31)),
            Binding {
                code: InputCode::PadAxis(15),
                scale: -0.5,
                deadzone: 0.25,
            },
            Binding::digital(InputCode::Touch(63)),
            Binding {
                code: InputCode::Key(32),
                scale: -0.,
                deadzone: 0.,
            },
        ],
    )
    .unwrap();
    p.action(
        "relative",
        "Relative",
        ActionKind::Delta,
        vec![
            Binding::digital(InputCode::MouseDelta(0)),
            Binding {
                code: InputCode::MouseDelta(1),
                scale: -1.,
                deadzone: 0.,
            },
            Binding::digital(InputCode::Wheel(0)),
            Binding::digital(InputCode::Wheel(1)),
            Binding::digital(InputCode::TouchDelta(15)),
        ],
    )
    .unwrap();
    let mut text = [0; 256];
    assert_eq!(
        p.format_hint_for("mixed", None, &mut text).unwrap(),
        "A x-1 / D / Mouse 5 / Pad 31 / Pad axis 15 x-0.5 / Touch 63"
    );
    assert_eq!(
        p.format_hint("mixed", &mut text).unwrap(),
        "A x-1 / D / Mouse 5"
    );
    assert_eq!(
        p.format_hint_for("mixed", Some(HintDevice::Gamepad), &mut text)
            .unwrap(),
        "Pad 31 / Pad axis 15 x-0.5"
    );
    assert_eq!(
        p.format_hint_for("mixed", Some(HintDevice::Touch), &mut text)
            .unwrap(),
        "Touch 63"
    );
    assert_eq!(
        p.format_hint_for("relative", None, &mut text).unwrap(),
        "Mouse X / Mouse Y x-1 / Wheel 0 / Wheel 1 / Touch look 15"
    );
    assert_eq!(
        p.format_hint_for("relative", Some(HintDevice::Gamepad), &mut text)
            .unwrap(),
        "Unbound"
    );
    p.rebind("jump", vec![]).unwrap();
    assert_eq!(
        p.format_hint_for("jump", None, &mut text).unwrap(),
        "Unbound"
    );
    p.rebind(
        "jump",
        vec![Binding {
            code: InputCode::Key(32),
            scale: 0.,
            deadzone: 0.,
        }],
    )
    .unwrap();
    assert_eq!(p.format_hint("jump", &mut text).unwrap(), "Unbound");
}

#[test]
fn hints_capacity_errors_are_exact_and_atomic_including_maximum_bindings() {
    let mut p = player();
    let expected = "Space / Pad 0 / Touch 3";
    for capacity in 0..expected.len() {
        let mut text = vec![b'?'; capacity];
        assert_eq!(
            p.format_hint_for("jump", None, &mut text),
            Err(HintError::BufferTooSmall {
                required: expected.len()
            })
        );
        assert!(text.iter().all(|&b| b == b'?'));
    }
    let mut text = [b'?'; 128];
    assert_eq!(
        p.format_hint_for("jump", None, &mut text[..expected.len()])
            .unwrap(),
        expected
    );
    assert!(text[expected.len()..].iter().all(|&b| b == b'?'));
    let before = text;
    assert_eq!(
        p.format_hint("missing", &mut text),
        Err(HintError::Input(InputError::InvalidHandle))
    );
    assert_eq!(text, before);
    assert_eq!(
        p.format_hint("missing", &mut []),
        Err(HintError::Input(InputError::InvalidHandle))
    );

    let mut bindings = vec![Binding::digital(InputCode::Key(65534)); MAX_BINDINGS];
    bindings[0].scale = f32::MAX;
    bindings[1].scale = f32::from_bits(1);
    bindings[2].scale = -f32::MAX;
    p.rebind("jump", bindings.clone()).unwrap();
    let Err(HintError::BufferTooSmall { required }) = p.format_hint("jump", &mut []) else {
        panic!("empty storage must report exact required capacity");
    };
    let mut full = vec![b'?'; required + 1];
    let formatted = p.format_hint("jump", &mut full).unwrap();
    assert_eq!(formatted.len(), required);
    assert_eq!(formatted.split(" / ").count(), MAX_BINDINGS);
    assert_eq!(formatted.matches("Key 65534").count(), MAX_BINDINGS);
    assert_eq!(formatted.matches(" x").count(), 3);
    assert!(formatted.is_ascii());
    assert_eq!(full[required], b'?');
    bindings.push(Binding::digital(InputCode::Key(32)));
    assert_eq!(p.rebind("jump", bindings), Err(InputError::BindingCapacity));
    assert_eq!(p.format_hint("jump", &mut full).unwrap().len(), required);
}

#[test]
fn hints_distinguish_modifier_sides_and_name_common_nonprinting_keys() {
    let mut p = player();
    p.rebind(
        "jump",
        [
            0xffe1, 0xffe2, 0xffe3, 0xffe4, 0xffe9, 0xffea, 0xff09, 0xff08, 0xffff, 0xffbe, 0xffc9,
            0xff1b, 0xff0d,
        ]
        .map(|code| Binding::digital(InputCode::Key(code)))
        .to_vec(),
    )
    .unwrap();
    let mut text = [0; 256];
    assert_eq!(
        p.format_hint("jump", &mut text).unwrap(),
        "Left Shift / Right Shift / Left Ctrl / Right Ctrl / Left Alt / Right Alt / Tab / Backspace / Delete / F1 / F12 / Esc / Enter"
    );
}

#[test]
fn hints_follow_rebind_import_capture_and_reset_without_cached_defaults() {
    let mut p = player();
    let mut text = [0; 128];
    let original = p.export_config().unwrap();
    p.rebind(
        "jump",
        vec![
            Binding::digital(InputCode::Key(106)),
            Binding::digital(InputCode::Mouse(2)),
        ],
    )
    .unwrap();
    assert_eq!(p.format_hint("jump", &mut text).unwrap(), "J / Mouse 2");
    let changed = p.export_config().unwrap();
    p.import_config(&original).unwrap();
    assert_eq!(p.format_hint("jump", &mut text).unwrap(), "Space");
    p.import_config(&changed).unwrap();
    assert_eq!(p.format_hint("jump", &mut text).unwrap(), "J / Mouse 2");
    let mut invalid = changed.clone();
    invalid[10] ^= 1;
    assert!(p.import_config(&invalid).is_err());
    assert_eq!(p.format_hint("jump", &mut text).unwrap(), "J / Mouse 2");
    p.reset_defaults();
    assert_eq!(
        p.format_hint_for("jump", None, &mut text).unwrap(),
        "Space / Pad 0 / Touch 3"
    );
    p.set_controls_open(true);
    p.begin_capture("jump", CaptureDevice::KeyboardMouse)
        .unwrap();
    crate::key(107, true);
    p.sample(true);
    assert_eq!(
        p.format_hint_for("jump", None, &mut text).unwrap(),
        "Pad 0 / Touch 3 / K"
    );
    assert_eq!(p.format_hint("jump", &mut text).unwrap(), "K");
    assert_eq!(p.take_tick(["jump"]), [Action::default()]);
    p.set_controls_open(false);
    p.sample(true);
    assert_eq!(p.take_tick(["jump"]), [Action::default()]);
}

fn hint_touch(phase: touch::Phase) {
    super::super::touch_event(touch::Event {
        id: 92,
        position: vec2(10., 10.),
        phase,
    });
}

#[test]
fn current_hints_follow_ordered_activity_not_releases_repeats_or_synthetic_mouse() {
    let mut p = player();
    pad_connected(true);
    p.sample(true);
    let mut text = [0; 128];
    assert_eq!(p.current_device(), HintDevice::KeyboardMouse);
    next();
    pad_button(0, 1.);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::Gamepad);
    assert_eq!(p.format_hint("jump", &mut text).unwrap(), "Pad 0");
    assert_eq!(p.format_hint("confirm", &mut text).unwrap(), "Unbound");
    next();
    crate::key(32, true);
    hint_touch(touch::Phase::Start);
    crate::button_source(1, true, true);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::Touch);
    assert_eq!(p.format_hint("jump", &mut text).unwrap(), "Touch 3");
    next();
    pad_button(0, 0.);
    crate::edit(|s| assert!(s.input.push(PlatformEvent::Value(InputCode::Key(32), 1.))));
    crate::key(32, false);
    hint_touch(touch::Phase::End);
    crate::pointer(50., 50., 0., 0.);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::Touch);
    next();
    crate::button(1, true);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::KeyboardMouse);
    next();
    hint_touch(touch::Phase::Start);
    crate::wheel(-1.);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::KeyboardMouse);
    next();
    hint_touch(touch::Phase::Start);
    p.sample(true);
    next();
    p.set_mouse_look_button(Some(2)).unwrap();
    crate::pointer(60., 50., 10., 0.);
    p.sample(true);
    assert_eq!(
        p.current_device(),
        HintDevice::KeyboardMouse,
        "pointer activity counts without a look drag"
    );
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::KeyboardMouse);
    next();
    hint_touch(touch::Phase::Move);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::Touch);
    next();
    crate::wheel(1.);
    hint_touch(touch::Phase::Cancel);
    p.sample(true);
    assert_eq!(
        p.current_device(),
        HintDevice::KeyboardMouse,
        "touch cancellation does not steal hints"
    );
}

#[test]
fn current_hints_ignore_unselected_pads_drift_and_suppressed_reconnections() {
    let mut p = player();
    pad_connected(true);
    pad_button(0, 1.);
    pad_axis(0, 0.8);
    crate::edit(|s| s.input.pad_connected(1, true));
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::KeyboardMouse);
    next();
    crate::edit(|s| {
        s.input.pad_button(1, 0, 1.);
        s.input.pad_axis(1, 0, 1.);
    });
    pad_axis(0, 0.9);
    pad_button(0, 0.9);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::KeyboardMouse);
    next();
    pad_button(0, 0.);
    pad_axis(0, 0.1);
    p.sample(true);
    next();
    pad_axis(0, 0.24);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::KeyboardMouse);
    next();
    pad_axis(0, 0.7);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::Gamepad);
    next();
    crate::key(100, true);
    pad_axis(0, 0.71);
    p.sample(true);
    assert_eq!(
        p.current_device(),
        HintDevice::KeyboardMouse,
        "held axis jitter cannot steal hints"
    );
    next();
    pad_axis(0, -0.7);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::Gamepad);
    next();
    hint_touch(touch::Phase::Start);
    pad_button(0, 1.);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::Gamepad);
    next();
    pad_connected(false);
    p.sample(true);
    assert_eq!(
        p.current_device(),
        HintDevice::Touch,
        "restore last non-pad family"
    );
    next();
    pad_connected(true);
    p.sample(true);
    next();
    pad_button(0, 1.);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::Gamepad);
    p.set_settings(ControlSettings {
        gamepad: 1,
        ..p.settings()
    })
    .unwrap();
    next();
    p.sample(true);
    assert_eq!(
        p.current_device(),
        HintDevice::Touch,
        "new slot's held buttons stay suppressed"
    );
}

#[test]
fn current_hints_respect_configured_axis_threshold_and_global_menu_activity() {
    let mut p = player();
    p.set_settings(ControlSettings {
        pad_deadzone: 0.8,
        ..p.settings()
    })
    .unwrap();
    pad_connected(true);
    p.sample(false);
    next();
    pad_axis(0, 0.7);
    p.sample(false);
    assert_eq!(p.current_device(), HintDevice::KeyboardMouse);
    next();
    pad_axis(0, 0.9);
    p.sample(false);
    assert_eq!(p.current_device(), HintDevice::Gamepad);
    assert!(!p.gameplay_active());
    assert_eq!(p.take_tick(["move_x"]), [Action::default()]);
    next();
    crate::key(0xffbf, true);
    p.sample(false);
    assert!(p.controls_open());
    assert_eq!(p.current_device(), HintDevice::KeyboardMouse);
    next();
    pad_button(16, 1.);
    p.sample(false);
    assert!(!p.controls_open());
    assert_eq!(p.current_device(), HintDevice::Gamepad);
}

#[test]
fn hint_reads_preserve_zero_tick_taps_motion_focus_and_overflow_suppression() {
    let mut p = player();
    pad_connected(true);
    p.sample(true);
    next();
    crate::key(32, true);
    crate::key(32, false);
    crate::pointer(20., 20., 10., 0.);
    p.sample(true);
    let pending_look = p.value("look_x");
    let mut text = [0; 128];
    for _ in 0..3 {
        assert_eq!(p.format_hint("jump", &mut text).unwrap(), "Space");
        p.format_hint_for("look_x", None, &mut text).unwrap();
        p.current_device();
        next();
        p.sample(true);
    }
    let [jump, look] = p.take_tick(["jump", "look_x"]);
    assert!(jump.pressed && jump.released);
    assert_eq!(look.value, pending_look);
    assert_eq!(p.take_tick(["jump", "look_x"]), [Action::default(); 2]);
    next();
    pad_button(0, 1.);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::Gamepad);
    crate::focus(false);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::Gamepad); // Packet is not replayed on same-frame sample.
    assert_eq!(p.format_hint("jump", &mut text).unwrap(), "Pad 0");
    assert_eq!(p.take_tick(["jump"]), [Action::default()]);
    crate::focus(true);
    p.sample(true);
    next();
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::KeyboardMouse);
    assert_eq!(p.take_tick(["jump"]), [Action::default()]);
    next();
    pad_button(0, 0.);
    pad_button(0, 1.);
    p.sample(true);
    assert_eq!(p.current_device(), HintDevice::Gamepad);
    assert!(p.take_tick(["jump"])[0].pressed);
    next();
    hint_touch(touch::Phase::Start);
    for _ in 0..MAX_EVENTS {
        crate::pointer(10., 10., 1., 0.);
    }
    p.sample(true);
    assert_eq!(
        p.current_device(),
        HintDevice::KeyboardMouse,
        "overflow discards touch activity and rearms pad suppression"
    );
    assert_eq!(p.take_tick(["jump", "look_x"]), [Action::default(); 2]);
}
