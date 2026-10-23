use super::*;

fn playing() -> Expedition {
    let mut expedition = Expedition::new(false);
    expedition.start();
    expedition.advance(0., Input::default);
    expedition
}

#[test]
fn focus_retains_near_ties_and_interacts_with_the_highlighted_crystal() {
    let mut game = Game::new();
    let p = game.crystals[0];
    game.crystals[1] = p + vec2(0.8, 0.);
    game.position = p + vec2(0.3, 0.);
    game.step(Input::default(), 1. / 60.);
    assert_eq!(game.focus.current(), Some(1));
    let snapshot = game.objectives.snapshot();
    for offset in [0.41, 0.38, 0.45, 0.42] {
        game.position = p + vec2(offset, 0.);
        game.step(Input::default(), 1. / 60.);
        assert_eq!(game.target(), Some(Interactable::Crystal(0)));
        assert_eq!(game.focus.current(), Some(1));
        assert_eq!(game.objectives.snapshot(), snapshot);
    }
    // The challenger is now slightly nearer, but the visible incumbent remains
    // the accepted interaction target, not a different nearest-only query.
    game.interact();
    assert_eq!(game.collected, [true, false, false]);
    assert_eq!(game.target(), Some(Interactable::Crystal(1)));
    assert!(game.prompt("Q").unwrap().starts_with("Q - Recover"));
}

#[test]
fn focus_switches_for_a_clear_advantage_and_forgets_obstructions_and_range() {
    let mut game = Game::new();
    let p = game.crystals[0];
    game.crystals[1] = p + vec2(0.8, 0.);
    game.position = p + vec2(0.3, 0.);
    game.step(Input::default(), 1. / 60.);
    game.position = p + vec2(0.65, 0.);
    game.step(Input::default(), 1. / 60.);
    assert_eq!(game.focus.current(), Some(2));
    let corner = center(5, 8);
    game.position = corner + vec2(-1.65, 1.);
    game.crystals[1] = corner + vec2(-1., 1.65);
    assert!(game.position.distance(game.crystals[1]) < 1.7);
    assert!(!game.visible(game.crystals[1]));
    game.step(Input::default(), 1. / 60.);
    assert_ne!(game.focus.current(), Some(2));
    assert_ne!(game.target(), Some(Interactable::Crystal(1)));
    game.position = at(b'S');
    game.step(Input::default(), 1. / 60.);
    assert_eq!(game.focus.current(), None);
}

#[test]
fn focus_and_objective_inspection_are_read_only_on_zero_tick_frames() {
    let mut expedition = playing();
    expedition.game.position = expedition.game.crystals[0];
    let snapshot = expedition.game.objectives.snapshot();
    for _ in 0..4 {
        let progress = expedition.advance(0.001, || panic!("no actual tick"));
        assert_eq!(progress.completed, 0);
        assert_eq!(expedition.game.target(), Some(Interactable::Crystal(0)));
        assert!(expedition.game.prompt("Q").is_some());
        assert_eq!(expedition.game.focus.current(), None);
        assert_eq!(expedition.game.objectives.snapshot(), snapshot);
    }
    expedition.advance(1. / 60., Input::default);
    assert_eq!(expedition.game.focus.current(), Some(1));
    expedition.restart();
    assert_eq!(expedition.game.focus.current(), None);
    assert!(
        expedition
            .game
            .notice
            .contains("All crystals and altars reset")
    );
}

#[test]
fn blockers_and_progress_follow_restored_objectives_without_mutation() {
    use ferravex_play::{objective::ObjectiveSnapshot, persistence::SaveData};
    let mut game = Game::new();
    game.position = game.crystals[0];
    game.interact();
    let bytes = game.objectives.snapshot().encode(1).unwrap();
    let mut restored = Game::new();
    restored
        .objectives
        .restore(ObjectiveSnapshot::decode(&bytes).unwrap().0)
        .unwrap();
    restored.sync_objectives();
    restored.position = at(b'A');
    assert_eq!(restored.crystal_progress(), (1, 3));
    assert!(restored.prompt("Q").unwrap().contains("1 / 3 recovered"));
    assert!(restored.objective_hint().contains("2 / 3 crystals remain"));
    let snapshot = restored.objectives.snapshot();
    restored.interact();
    assert_eq!(restored.objectives.snapshot(), snapshot);
    assert_eq!(restored.notice, restored.prompt("Q").unwrap());
    for i in 1..3 {
        restored.objective_event(i);
    }
    assert!(restored.target_ready(Interactable::Altar(0)));
    assert_eq!(restored.prompt("Q").unwrap(), "Q - Awaken altar I");
    restored.interact();
    restored.position = at(b'G') + vec2(0., 1.);
    assert!(restored.prompt("Q").unwrap().contains("1 / 2 awake"));
    assert!(!restored.open());
    assert_eq!(restored.completed_mask().count_ones(), 4);
}

#[test]
fn accepted_progress_reports_objectives_once_and_rejects_locked_altars() {
    let mut expedition = playing();
    expedition.game.position = at(b'A');
    let use_input = || Input {
        interact: true,
        ..Default::default()
    };
    let blocked = expedition.advance(0.1, use_input);
    assert_eq!(blocked.completed, 0);
    assert_eq!(blocked.altars, 0);
    assert_eq!(expedition.game.objectives.last_event_serial(), 0);
    expedition.game.position = expedition.game.crystals[0];
    let accepted = expedition.advance(1. / 60., use_input);
    assert_eq!(accepted.completed, 1);
    assert_eq!(accepted.pickups, 1);
    assert_eq!(expedition.advance(0.1, use_input).completed, 0);
    for i in 1..5 {
        expedition.game.objective_event(i);
    }
    expedition.game.position = at(b'X');
    let win = expedition.advance(0.1, Input::default);
    assert_eq!(win.completed, 1 << 5);
    assert!(win.won);
    assert_eq!(expedition.game.completed_mask().count_ones(), 6);
    assert_eq!(expedition.advance(10., Input::default).completed, 0);
}

#[test]
fn hints_follow_live_bindings_unbound_actions_and_reset() {
    let mut input = PlayerInput::transient("cavern-hints");
    for (name, key) in [
        ("interact", KeyCode::E),
        ("confirm", KeyCode::Enter),
        ("restart", KeyCode::R),
    ] {
        input
            .action(
                name,
                name,
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(key as u16))],
            )
            .unwrap();
    }
    let mut game = Game::new();
    game.position = game.crystals[0];
    assert_eq!(
        game.prompt(&action_hint(&input, "interact")).unwrap(),
        "E - Recover crystal"
    );
    input
        .rebind(
            "interact",
            vec![Binding::digital(InputCode::Key(KeyCode::Q as u16))],
        )
        .unwrap();
    input
        .rebind(
            "confirm",
            vec![Binding::digital(InputCode::Key(KeyCode::Space as u16))],
        )
        .unwrap();
    input.rebind("restart", Vec::new()).unwrap();
    assert_eq!(
        game.prompt(&action_hint(&input, "interact")).unwrap(),
        "Q - Recover crystal"
    );
    assert_eq!(action_hint(&input, "confirm"), "Space");
    assert_eq!(action_hint(&input, "restart"), "Unbound");
    input.reset_defaults();
    assert_eq!(action_hint(&input, "interact"), "E");
    assert_eq!(action_hint(&input, "confirm"), "Enter");
}

#[test]
fn oversized_binding_lists_show_controls_instead_of_a_partial_hint() {
    let mut input = PlayerInput::transient("cavern-long-hint");
    input
        .action(
            "interact",
            "Use",
            ActionKind::Button,
            vec![Binding::digital(InputCode::Key(KeyCode::Space as u16)); 64],
        )
        .unwrap();
    assert_eq!(action_hint(&input, "interact"), "See Controls");
}

#[test]
fn independent_presentation_preferences_apply_without_simulation_ticks() {
    for motion in [false, true] {
        for flash in [false, true] {
            let mut presentation = Presentation::default();
            presentation.advance(0.1, FeedbackPolicy::default());
            let before = (presentation.motion_time, presentation.glow_time);
            let policy = FeedbackPolicy::new(motion, flash, 1.).unwrap();
            // Ready, paused and Controls frames can have no gameplay time.
            presentation.advance(0., policy);
            assert_eq!(presentation.pulse, !flash);
            assert_eq!((presentation.motion_time, presentation.glow_time), before);
            presentation.advance(0.2, policy);
            assert!(
                (presentation.motion_time - before.0 - if motion { 0. } else { 0.2 }).abs() < 1e-6
            );
            assert!(
                (presentation.glow_time - before.1 - if flash { 0. } else { 0.2 }).abs() < 1e-6
            );
            let disabled = (presentation.motion_time, presentation.glow_time);
            presentation.advance(0., FeedbackPolicy::default());
            assert_eq!((presentation.motion_time, presentation.glow_time), disabled);
            presentation.advance(0.01, FeedbackPolicy::default());
            assert!((presentation.motion_time - disabled.0 - 0.01).abs() < 1e-6);
            assert!((presentation.glow_time - disabled.1 - 0.01).abs() < 1e-6);
        }
    }
}

#[test]
fn shared_policy_preserves_accepted_progress_and_retry_clears_phases() {
    use ferravex_render::preferences;
    let previous = preferences::feedback_policy();
    preferences::set_feedback_policy(FeedbackPolicy::new(true, true, 1.).unwrap());
    let mut expedition = playing();
    let mut presentation = Presentation::default();
    presentation.advance(0.1, previous);
    let motion = presentation.motion_time;
    expedition.game.position = expedition.game.crystals[0];
    let policy = preferences::feedback_policy();
    let accepted = expedition.advance(1. / 60., || Input {
        interact: true,
        ..Default::default()
    });
    presentation.advance(expedition.game.elapsed, policy);
    preferences::set_feedback_policy(previous);
    assert_eq!(accepted.pickups, 1);
    assert_eq!(accepted.completed, 1);
    assert_eq!(expedition.session.tick(), 1);
    assert_eq!(presentation.motion_time, motion);
    assert!(!presentation.pulse);
    expedition.restart();
    presentation = Presentation::default();
    presentation.advance(0., policy);
    assert_eq!((presentation.motion_time, presentation.glow_time), (0., 0.));
    assert!(!presentation.pulse);
    assert_eq!(expedition.game.completed_mask(), 0);
}
