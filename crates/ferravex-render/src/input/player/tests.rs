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
