//! Secrets, bonks, zones and whispered hints: presentation facts derived from
//! real rules, never feeding back into the objective graph.
use super::*;

fn playing() -> Expedition {
    let mut expedition = Expedition::new(false);
    expedition.start();
    expedition.advance(0., Input::default);
    expedition
}

fn use_input() -> Input {
    Input {
        interact: true,
        ..Default::default()
    }
}

#[test]
fn steve_and_the_note_are_optional_secrets_outside_the_objective_graph() {
    let mut expedition = playing();
    expedition.game.position = shy_position() + vec2(-0.3, -0.3);
    assert_eq!(expedition.game.target(), Some(Interactable::Shy));
    assert!(expedition.game.prompt("E").unwrap().contains("shy crystal"));
    let progress = expedition.advance(1. / 60., use_input);
    assert!(progress.shy);
    assert_eq!(progress.completed, 0);
    assert_eq!(progress.pickups, 0);
    assert!(expedition.game.shy_found);
    assert_eq!(expedition.game.objectives.last_event_serial(), 0);
    assert_eq!(expedition.game.count(), 0);
    // Steve can only be found once; the corner is then empty.
    assert_eq!(expedition.game.target(), None);

    expedition.game.position = note_position() + vec2(0.3, 0.3);
    assert_eq!(expedition.game.target(), Some(Interactable::Note));
    assert!(expedition.game.prompt("E").unwrap().contains("lost note"));
    let first = expedition.advance(1. / 60., use_input);
    assert_eq!(first.note, Some(0));
    assert!(expedition.game.notice.contains("Steve"));
    assert!(expedition.game.prompt("E").unwrap().contains("page 2"));
    expedition.advance(1. / 60., Input::default);
    let second = expedition.advance(1. / 60., use_input);
    assert_eq!(second.note, Some(1));
    assert_eq!(expedition.game.objectives.last_event_serial(), 0);

    expedition.restart();
    assert!(!expedition.game.shy_found);
    assert_eq!(expedition.game.note_reads, 0);
    assert_eq!(expedition.runs, 1);
}

#[test]
fn the_authored_pilot_never_detours_into_a_secret() {
    let mut expedition = Expedition::new(true);
    let mut zones = Vec::new();
    for _ in 0..7_200 {
        let progress = expedition.advance(1. / 60., Input::default);
        assert!(!progress.shy && progress.note.is_none());
        zones.extend(progress.zone);
        if progress.won {
            break;
        }
    }
    assert!(expedition.game.won);
    assert!(!expedition.game.shy_found);
    assert_eq!(expedition.game.note_reads, 0);
    // Every named area is discovered exactly once on the way to daylight.
    zones.sort_unstable();
    assert_eq!(zones, [1, 2, 3, 4, 5]);
    assert_eq!(expedition.game.zones_seen, 0b11_1111);
    assert_eq!(
        expedition.game.hints_used, 0,
        "a brisk run needs no whispers"
    );
}

#[test]
fn walking_into_the_sealed_gate_bonks_once_per_impact() {
    let mut expedition = playing();
    let north = Input {
        movement: Vec2::Y,
        ..Default::default()
    };
    let mut bonks = 0;
    for _ in 0..600 {
        bonks += expedition.advance(1. / 60., || north).bonks;
    }
    assert_eq!(
        bonks, 1,
        "pressing on into a wall is one bonk, not one per tick"
    );
    assert_eq!(expedition.game.bonks, 1);
    // Back off and charge again.
    for _ in 0..40 {
        expedition.advance(1. / 60., || Input {
            movement: -Vec2::Y,
            ..Default::default()
        });
    }
    for _ in 0..60 {
        bonks += expedition.advance(1. / 60., || north).bonks;
    }
    assert_eq!(bonks, 2);
    // Gliding along a corridor wall at a shallow angle is not a bonk.
    let mut game = Game::new();
    game.position = center(6, 8) + vec2(0.9, 0.);
    game.yaw = 0.3;
    for _ in 0..60 {
        game.step(
            Input {
                movement: Vec2::Y,
                ..Default::default()
            },
            1. / 60.,
        );
    }
    assert!(game.position.x > center(6, 8).x + 1.);
    assert!(
        game.position.y < center(6, 8).y - 3.,
        "slid north along the wall"
    );
    assert_eq!(game.bonks, 0);
}

#[test]
fn rejected_uses_are_reported_without_touching_objectives() {
    let mut expedition = playing();
    expedition.game.position = at(b'A');
    let progress = expedition.advance(1. / 60., use_input);
    assert_eq!(progress.rejected, Some(Interactable::Altar(0)));
    assert_eq!(progress.completed, 0);
    assert_eq!(expedition.game.rejected, 1);
    assert_eq!(expedition.game.objectives.last_event_serial(), 0);
}

#[test]
fn whispers_escalate_while_stuck_and_reset_on_progress() {
    let mut expedition = playing();
    let mut hints = Vec::new();
    for tick in 0..(90 * 60) {
        if expedition.advance(1. / 60., Input::default).hint {
            hints.push(tick);
        }
    }
    assert_eq!(hints.len(), 3);
    assert_eq!(expedition.game.hint_level, 3);
    assert_eq!(expedition.game.hints_used, 3);
    assert!(expedition.game.hint_text().contains("Try "));
    assert!(expedition.game.hint_target().is_some());
    // Progress resets the schedule, but the run remembers hints used.
    expedition.game.position = expedition.game.crystals[0];
    expedition.advance(1. / 60., use_input);
    assert_eq!(expedition.game.hint_level, 0);
    assert_eq!(expedition.game.hints_used, 3);
    assert!(expedition.game.hint_text().is_empty());
    expedition.restart();
    assert_eq!(expedition.game.hints_used, 0);
}

#[test]
fn resonance_rises_toward_the_nearest_missing_crystal_only() {
    let mut game = Game::new();
    game.position = game.crystals[0] + vec2(8., 0.);
    let far = game.resonance();
    game.position = game.crystals[0] + vec2(1., 0.);
    let near = game.resonance();
    assert!(near > far && near > 0.85);
    game.collected = [true; 3];
    game.position = shy_position();
    assert_eq!(game.resonance(), 0., "Steve does not hum. Steve is shy.");
    // Hints prefer a crystal in plain view over a nearer one behind rock.
    let mut game = Game::new();
    game.position = center(6, 9) + vec2(0., 1.2);
    let hidden = game.crystals[2];
    assert!(!game.visible(hidden));
    assert_eq!(game.nearest_crystal(), Some(hidden));
    let target = game.hint_target().unwrap();
    assert_ne!(target, hidden);
    assert!(game.visible(target));
    assert!(matches!(zone_at(target), 3 | 4), "a grotto crystal in view");
}

#[test]
fn zones_name_the_authored_areas() {
    assert_eq!(zone_at(at(b'S')), 0);
    assert_eq!(zone_at(center(7, 11)), 1);
    assert_eq!(zone_at(center(6, 8)), 2);
    assert_eq!(zone_at(center(2, 9)), 3);
    assert_eq!(zone_at(center(10, 9)), 4);
    assert_eq!(zone_at(at(b'A')), 5);
    assert_eq!(zone_at(shy_position()), 0);
    assert_eq!(zone_at(note_position()), 3);
}
