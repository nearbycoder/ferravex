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

#[test]
fn typed_lookup_and_copied_handles_share_edges_with_legacy_access() {
    let mut map = ActionMap::default();
    let jump = map
        .named_action_handle(
            "jump",
            ActionKind::Button,
            ContextId::GLOBAL,
            vec![binding(32, 1.)],
        )
        .unwrap();
    let id = map.action_id("jump").unwrap();
    assert_eq!(map.action_handle("jump"), Some(jump));
    assert_eq!(map.handle_from_id(id), Some(jump));
    assert_eq!(map.action_handle("missing"), None);
    assert_eq!(map.action_handle("Jump"), None);
    assert_eq!(map.handle_from_id(usize::MAX), None);
    assert_eq!(map.handle_from_id(id + 1), None);

    map.feed(InputCode::Key(32), 1.);
    map.feed(InputCode::Key(32), 0.);
    let tap = Action {
        value: 0.,
        pressed: true,
        released: true,
    };
    assert_eq!(map.peek_handle(jump), Ok(tap));
    assert_eq!(map.peek_handle(jump), Ok(tap));
    let copied = jump;
    assert_eq!(map.consume_tick_handle(copied), Ok(tap));
    assert_eq!(map.consume_tick_handle(jump), Ok(Action::default()));
    assert_eq!(map.consume_tick(id), Action::default());

    map.feed(InputCode::Key(32), 1.);
    assert!(map.consume_tick(id).pressed);
    assert_eq!(
        map.consume_tick_handle(jump),
        Ok(Action {
            value: 1.,
            pressed: false,
            released: false,
        })
    );
}

#[test]
fn foreign_and_stale_handles_cannot_read_rebind_or_consume_another_maps_action() {
    let mut original = ActionMap::default();
    let old = original
        .named_action_handle(
            "jump",
            ActionKind::Button,
            ContextId::GLOBAL,
            vec![binding(32, 1.)],
        )
        .unwrap();
    let mut other = ActionMap::default();
    let current = other
        .named_action_handle(
            "jump",
            ActionKind::Button,
            ContextId::GLOBAL,
            vec![binding(32, 1.)],
        )
        .unwrap();
    assert_eq!(original.action_id("jump"), other.action_id("jump"));
    assert_ne!(old, current);
    other.feed(InputCode::Key(32), 1.);
    let pending = other.peek_handle(current).unwrap();
    assert_eq!(other.peek_handle(old), Err(InputError::InvalidHandle));
    assert_eq!(other.bindings_handle(old), Err(InputError::InvalidHandle));
    assert_eq!(
        other.consume_tick_handle(old),
        Err(InputError::InvalidHandle)
    );
    assert_eq!(
        other.try_rebind_handle(old, vec![binding(82, 1.)]),
        Err(InputError::InvalidHandle)
    );
    assert_eq!(other.peek_handle(current), Ok(pending));
    assert_eq!(other.bindings_handle(current).unwrap(), &[binding(32, 1.)]);
    assert_eq!(
        original.peek_handle(current),
        Err(InputError::InvalidHandle)
    );

    // Replacement at the same variable/slot must not revive the old identity.
    original = other;
    assert_eq!(original.peek_handle(old), Err(InputError::InvalidHandle));
    assert_eq!(original.consume_tick_handle(current), Ok(pending));
    original = ActionMap::default();
    let replacement = original
        .named_action_handle("jump", ActionKind::Button, ContextId::GLOBAL, vec![])
        .unwrap();
    assert_ne!(replacement, old);
    assert_ne!(replacement, current);
    assert_eq!(original.peek_handle(old), Err(InputError::InvalidHandle));
    assert_eq!(
        original.peek_handle(current),
        Err(InputError::InvalidHandle)
    );
    assert_eq!(original.peek_handle(replacement), Ok(Action::default()));
}

#[test]
fn typed_handles_survive_map_moves_growth_and_registration_failures() {
    let mut map = ActionMap::default();
    let id = map.action(vec![binding(1, 1.)]);
    let first = map.handle_from_id(id).unwrap();
    let mut moved = Box::new(map);
    for id in 1..MAX_ACTIONS {
        let name = format!("action_{id}");
        let handle = moved
            .named_action_handle(&name, ActionKind::Axis, ContextId::GLOBAL, vec![])
            .unwrap();
        assert_eq!(moved.action_handle(&name), Some(handle));
        assert_eq!(moved.peek_handle(handle), Ok(Action::default()));
    }
    assert_eq!(
        moved.named_action_handle("overflow", ActionKind::Button, ContextId::GLOBAL, vec![]),
        Err(InputError::ActionCapacity)
    );
    assert_eq!(moved.action_handle("overflow"), None);
    assert_eq!(moved.handle_from_id(MAX_ACTIONS), None);
    moved.feed(InputCode::Key(1), 1.);
    assert!(moved.consume_tick_handle(first).unwrap().pressed);

    // Bounds are checked even for a corrupt internal handle; safe callers can
    // only construct handles through registration or a checked lookup.
    let invalid = ActionHandle {
        map_id: moved.map_id,
        index: usize::MAX,
    };
    assert_eq!(moved.peek_handle(invalid), Err(InputError::InvalidHandle));
    assert_eq!(
        moved.bindings_handle(invalid),
        Err(InputError::InvalidHandle)
    );
    assert_eq!(
        moved.consume_tick_handle(invalid),
        Err(InputError::InvalidHandle)
    );
    assert_eq!(
        moved.try_rebind_handle(invalid, vec![]),
        Err(InputError::InvalidHandle)
    );
}

#[test]
fn typed_rebinding_and_config_keep_identity_and_suppress_held_buttons() {
    let mut map = ActionMap::default();
    let jump = map
        .named_action_handle(
            "jump",
            ActionKind::Button,
            ContextId::GLOBAL,
            vec![binding(32, 1.)],
        )
        .unwrap();
    let saved = map.encode_config(ControlSettings::default()).unwrap();
    map.feed(InputCode::Key(32), 1.);
    let pending = map.peek_handle(jump).unwrap();
    assert_eq!(
        map.try_rebind_handle(jump, vec![binding(82, f32::NAN)]),
        Err(InputError::InvalidBinding)
    );
    assert_eq!(map.peek_handle(jump), Ok(pending));
    assert_eq!(map.bindings_handle(jump).unwrap(), &[binding(32, 1.)]);
    assert_eq!(
        map.named_action_handle("jump", ActionKind::Axis, ContextId::GLOBAL, vec![]),
        Err(InputError::DuplicateName)
    );
    assert_eq!(map.peek_handle(jump), Ok(pending));

    map.feed(InputCode::Key(82), 1.);
    map.try_rebind_handle(jump, vec![binding(82, 1.)]).unwrap();
    assert_eq!(map.action_handle("jump"), Some(jump));
    assert_eq!(map.consume_tick_handle(jump), Ok(Action::default()));
    map.feed(InputCode::Key(82), 0.);
    map.feed(InputCode::Key(82), 1.);
    assert!(map.consume_tick_handle(jump).unwrap().pressed);

    assert_eq!(map.apply_config(&saved).unwrap().matched, 1);
    assert_eq!(map.action_handle("jump"), Some(jump));
    assert_eq!(map.bindings_handle(jump).unwrap(), &[binding(32, 1.)]);
    assert_eq!(map.consume_tick_handle(jump), Ok(Action::default()));
    map.feed(InputCode::Key(32), 0.);
    map.feed(InputCode::Key(32), 1.);
    assert!(map.consume_tick_handle(jump).unwrap().pressed);
}

#[test]
fn typed_context_switches_preserve_validity_without_leaking_held_presses() {
    let mut map = ActionMap::default();
    let game = map.create_context("game").unwrap();
    let menu = map.create_context("menu").unwrap();
    let jump = map
        .named_action_handle("jump", ActionKind::Button, game, vec![binding(32, 1.)])
        .unwrap();
    let accept = map
        .named_action_handle("accept", ActionKind::Button, menu, vec![binding(32, 1.)])
        .unwrap();
    assert_eq!(map.action_handle("jump"), Some(jump));
    assert_eq!(map.peek_handle(jump), Ok(Action::default()));
    map.set_context(Some(game)).unwrap();
    map.feed(InputCode::Key(32), 1.);
    assert!(map.consume_tick_handle(jump).unwrap().pressed);
    map.push_context(menu, ContextMode::Exclusive).unwrap();
    assert_eq!(map.peek_handle(jump).unwrap().value, 0.);
    assert_eq!(map.consume_tick_handle(accept), Ok(Action::default()));
    map.pop_context();
    assert_eq!(map.consume_tick_handle(jump), Ok(Action::default()));
    map.feed(InputCode::Key(32), 0.);
    map.feed(InputCode::Key(32), 1.);
    assert!(map.consume_tick_handle(jump).unwrap().pressed);
    map.suppress_held();
    assert_eq!(map.consume_tick_handle(jump), Ok(Action::default()));
    map.lose_focus();
    map.feed(InputCode::Key(32), 1.);
    assert!(map.consume_tick_handle(jump).unwrap().pressed);
}

#[test]
fn typed_delta_consumption_is_per_action_and_focus_loss_discards_motion() {
    let mut map = ActionMap::default();
    let look = map
        .named_action_handle(
            "look",
            ActionKind::Delta,
            ContextId::GLOBAL,
            vec![Binding::digital(InputCode::MouseDelta(0))],
        )
        .unwrap();
    let pan = map
        .named_action_handle(
            "pan",
            ActionKind::Delta,
            ContextId::GLOBAL,
            vec![Binding::digital(InputCode::MouseDelta(0))],
        )
        .unwrap();
    map.feed_delta(InputCode::MouseDelta(0), 3.);
    map.feed_delta(InputCode::MouseDelta(0), 5.);
    assert_eq!(map.peek_handle(look).unwrap().value, 8.);
    assert_eq!(map.peek_handle(look).unwrap().value, 8.);
    let copied = look;
    assert_eq!(map.consume_tick_handle(copied).unwrap().value, 8.);
    assert_eq!(map.consume_tick_handle(look), Ok(Action::default()));
    assert_eq!(map.consume_tick_handle(pan).unwrap().value, 8.);
    map.feed_delta(InputCode::MouseDelta(0), 10.);
    map.lose_focus();
    assert_eq!(map.consume_tick_handle(look), Ok(Action::default()));
    assert_eq!(map.consume_tick_handle(pan), Ok(Action::default()));
}
