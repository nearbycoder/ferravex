use super::*;
fn binding(code: u16, scale: f32) -> Binding {
    Binding {
        code: InputCode::Key(code),
        scale,
        deadzone: 0.0,
    }
}
#[test]
fn quick_tap_survives_no_simulation_and_edges_fire_once() {
    let mut map = ActionMap::default();
    let jump = map.action(vec![binding(32, 1.0)]);
    map.feed(InputCode::Key(32), 1.0);
    map.feed(InputCode::Key(32), 0.0);
    assert_eq!(
        map.consume_tick(jump),
        Action {
            value: 0.0,
            pressed: true,
            released: true
        }
    );
    assert_eq!(map.consume_tick(jump), Action::default());
    map.feed(InputCode::Key(32), 1.0);
    map.lose_focus();
    assert_eq!(map.peek(jump).value, 0.0);
}
#[test]
fn finite_extreme_scales_and_nonfinite_inputs_cannot_poison_actions() {
    let mut map = ActionMap::default();
    let id = map.action(vec![
        binding(1, f32::MAX),
        binding(1, f32::MAX),
        binding(1, -f32::MAX),
        binding(1, -f32::MAX),
    ]);
    map.feed(InputCode::Key(1), 1.0);
    assert_eq!(map.peek(id).value, 0.0);
    assert!(map.rebind(id, vec![binding(1, 1.0)]));
    assert_eq!(map.peek(id).value, 1.0);
    map.feed(InputCode::Key(1), f32::NAN);
    assert_eq!(map.peek(id).value, 0.0);
}
#[test]
fn released_codes_free_capacity_and_registration_observes_held_input() {
    let mut map = ActionMap::default();
    for code in 0..1100 {
        map.feed(InputCode::Key(code), 1.0);
        map.feed(InputCode::Key(code), 0.0);
    }
    map.feed(InputCode::Key(1200), 1.0);
    let id = map.action(vec![binding(1200, 1.0)]);
    assert!(map.peek(id).pressed);
    assert_eq!(map.peek(id).value, 1.0);
    assert_eq!(map.values.len(), 1);
}
#[test]
fn rebind_and_handle_limits_are_transactional() {
    let mut map = ActionMap::default();
    let id = map.action(vec![binding(1, 1.0)]);
    assert!(!map.rebind(id, vec![binding(2, 1.0); MAX_BINDINGS + 1]));
    map.feed(InputCode::Key(1), 1.0);
    assert_eq!(map.peek(id).value, 1.0);
    assert!(
        map.try_action(vec![binding(2, 1.0); MAX_BINDINGS + 1])
            .is_none()
    );
    assert!(!map.rebind(999, vec![]));
    assert_eq!(map.consume_tick(999), Action::default());
}

#[test]
fn contexts_release_gameplay_and_never_retrigger_a_held_button_on_resume() {
    let mut map = ActionMap::default();
    let game = map.create_context("gameplay").unwrap();
    let menu = map.create_context("menu").unwrap();
    let jump = map
        .named_action("jump", ActionKind::Button, game, vec![binding(32, 1.)])
        .unwrap();
    let accept = map
        .named_action("accept", ActionKind::Button, menu, vec![binding(32, 1.)])
        .unwrap();
    let global = map
        .named_action(
            "pause",
            ActionKind::Button,
            ContextId::GLOBAL,
            vec![binding(27, 1.)],
        )
        .unwrap();
    map.push_context(game, ContextMode::Exclusive).unwrap();
    map.feed(InputCode::Key(32), 1.);
    assert!(map.consume_tick(jump).pressed);
    map.push_context(menu, ContextMode::Exclusive).unwrap();
    assert_eq!(map.peek(jump).value, 0.);
    assert_eq!(map.peek(accept).value, 0.);
    map.feed(InputCode::Key(27), 1.);
    assert!(map.consume_tick(global).pressed);
    map.pop_context();
    assert_eq!(map.peek(jump).value, 0.);
    assert!(!map.consume_tick(jump).pressed);
    map.feed(InputCode::Key(32), 0.);
    map.feed(InputCode::Key(32), 1.);
    assert!(map.consume_tick(jump).pressed);
}
#[test]
fn passthrough_contexts_axes_and_idempotent_activation_preserve_edges() {
    let mut map = ActionMap::default();
    let game = map.create_context("gameplay").unwrap();
    let overlay = map.create_context("overlay").unwrap();
    let axis = map
        .named_action("move_x", ActionKind::Axis, game, vec![binding(65, 1.)])
        .unwrap();
    map.set_context(Some(game)).unwrap();
    map.feed(InputCode::Key(65), 1.);
    map.set_context(Some(game)).unwrap();
    assert!(map.consume_tick(axis).pressed);
    map.push_context(overlay, ContextMode::Passthrough).unwrap();
    assert_eq!(map.peek(axis).value, 1.);
    map.clear_contexts();
    assert_eq!(map.peek(axis).value, 0.);
    map.set_context(Some(game)).unwrap();
    assert_eq!(map.peek(axis).value, 1.);
}
#[test]
fn relative_motion_accumulates_without_ticks_and_consumption_is_per_action() {
    let mut map = ActionMap::default();
    let a = map
        .named_action(
            "look_x",
            ActionKind::Delta,
            ContextId::GLOBAL,
            vec![Binding::digital(InputCode::MouseDelta(0))],
        )
        .unwrap();
    let b = map
        .named_action(
            "pan_x",
            ActionKind::Delta,
            ContextId::GLOBAL,
            vec![Binding::digital(InputCode::MouseDelta(0))],
        )
        .unwrap();
    map.feed_delta(InputCode::MouseDelta(0), 3.);
    map.feed_delta(InputCode::MouseDelta(0), 5.);
    assert_eq!(map.consume_tick(a).value, 8.);
    assert_eq!(map.consume_tick(a).value, 0.);
    assert_eq!(map.consume_tick(b).value, 8.);
    map.feed_delta(InputCode::MouseDelta(0), 10.);
    map.lose_focus();
    assert_eq!(map.consume_tick(a), Action::default());
}
#[test]
fn binding_validation_and_button_rebinding_are_transactional() {
    let mut map = ActionMap::default();
    let jump = map
        .named_action(
            "jump",
            ActionKind::Button,
            ContextId::GLOBAL,
            vec![binding(32, 1.)],
        )
        .unwrap();
    map.feed(InputCode::Key(82), 1.);
    assert!(map.rebind(jump, vec![binding(82, 1.)]));
    assert_eq!(map.peek(jump).value, 0.);
    map.feed(InputCode::Key(82), 0.);
    map.feed(InputCode::Key(82), 1.);
    assert!(map.consume_tick(jump).pressed);
    assert!(map.try_rebind(jump, vec![binding(1, f32::NAN)]).is_err());
    assert_eq!(map.bindings(jump).unwrap()[0].code, InputCode::Key(82));
    assert_eq!(
        map.named_action("jump", ActionKind::Button, ContextId::GLOBAL, vec![]),
        Err(InputError::DuplicateName)
    );
    assert_eq!(
        map.named_action("bad/name", ActionKind::Axis, ContextId::GLOBAL, vec![]),
        Err(InputError::InvalidName)
    );
}
