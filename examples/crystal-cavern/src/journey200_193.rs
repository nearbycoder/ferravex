use super::*;

#[test]
fn readable_hint_budget_keeps_long_single_keys_and_redirects_binding_lists() {
    let mut input = PlayerInput::transient("crystal-193-hints");
    input
        .action(
            "interact",
            "Use",
            ActionKind::Button,
            vec![Binding::digital(InputCode::Key(KeyCode::Backspace as u16))],
        )
        .unwrap();
    assert_eq!(action_hint(&input, "interact"), "Backspace");
    input
        .rebind(
            "interact",
            vec![
                Binding::digital(InputCode::Key(KeyCode::Backspace as u16)),
                Binding::digital(InputCode::Key(KeyCode::E as u16)),
            ],
        )
        .unwrap();
    let mut complete = [0; 96];
    assert_eq!(
        input.format_hint("interact", &mut complete).unwrap(),
        "Backspace / E"
    );
    assert_eq!(action_hint(&input, "interact"), "See Controls");
    assert_eq!(input.bindings("interact").unwrap().len(), 2);
    input.rebind("interact", vec![]).unwrap();
    assert_eq!(action_hint(&input, "interact"), "Unbound");
    input.reset_defaults();
    assert_eq!(action_hint(&input, "interact"), "Backspace");
}
