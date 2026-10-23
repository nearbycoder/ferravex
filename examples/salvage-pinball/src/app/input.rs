//! The live PlayerInput-to-game boundary. Tests drive this same adapter through
//! the renderer's bounded host journal, without running a window or drawing.

use crate::{Controls, PinballFeedback, PinballGame, game::ControlSample};
use ferravex_render::{input::PlayerInput, prelude::*};

pub(super) fn advance(
    game: &mut PinballGame,
    player: &mut PlayerInput,
    elapsed_seconds: f64,
    pointer: Vec2,
) -> PinballFeedback {
    let split = screen_width() * 0.5;
    let lower = screen_height() * 0.5;
    let (_, _, reset) = super::table_layout(screen_width(), screen_height());
    let left = player.take("flipper_left");
    let right = player.take("flipper_right");
    let launch = player.take("launch");
    let pointer_action = player.take("table_pointer");
    let combine = |left: bool, right: bool, launch: bool, pointer_action: bool| {
        let pointer_action = pointer_action && !reset.contains(pointer);
        Controls {
            left: left || pointer_action && pointer.x < split && pointer.y > lower,
            right: right || pointer_action && pointer.x >= split && pointer.y > lower,
            launch: launch || pointer_action,
        }
    };
    // Taking actions here transfers their edges into the game's bounded buffer;
    // zero-tick frames retain them there. Never OR an edge into its final level.
    let sample = ControlSample {
        held: combine(
            left.value > 0.5,
            right.value > 0.5,
            launch.value > 0.5,
            pointer_action.value > 0.5,
        ),
        pressed: combine(
            left.pressed,
            right.pressed,
            launch.pressed,
            pointer_action.pressed,
        ),
        released: combine(
            left.released,
            right.released,
            launch.released,
            pointer_action.released,
        ),
    };
    game.advance_sample(elapsed_seconds, sample)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_render::input::{self, ActionKind, Binding, InputCode, PadState};

    const TICK: f64 = 1. / 120.;
    const POINTER: Vec2 = vec2(400., 300.); // Above flipper regions, outside Reset.

    fn player() -> PlayerInput {
        let mut player = PlayerInput::transient("pinball-edge-repair");
        for (name, button) in [
            ("launch", 0),        // The production preset's default controller binding.
            ("table_pointer", 1), // Remap the mouse action to an injectable host button.
            ("flipper_left", 4),
            ("flipper_right", 5),
        ] {
            player
                .action(
                    name,
                    name,
                    ActionKind::Button,
                    vec![Binding::digital(InputCode::PadButton(button))],
                )
                .unwrap();
        }
        player
    }

    fn feed(buttons: &[usize]) {
        let mut pad = PadState {
            connected: true,
            ..Default::default()
        };
        for &button in buttons {
            pad.buttons[button] = 1.;
        }
        assert!(input::gamepad_sample(0, pad));
    }

    fn frame(game: &mut PinballGame, player: &mut PlayerInput, dt: f64) -> PinballFeedback {
        frame_at(game, player, dt, POINTER)
    }

    fn frame_at(
        game: &mut PinballGame,
        player: &mut PlayerInput,
        dt: f64,
        pointer: Vec2,
    ) -> PinballFeedback {
        input::host_frame(dt as f32, 800, 1000, || {
            player.sample_play();
            game.set_paused(!player.gameplay_active());
            advance(game, player, dt, pointer)
        })
    }

    fn wait_for_drain(game: &mut PinballGame, player: &mut PlayerInput) {
        let lives = game.table().lives;
        for _ in 0..6000 {
            let events = frame(game, player, TICK);
            assert_eq!(events.launches, 0);
            if events.drains > 0 {
                assert_eq!((events.drains, events.saves), (1, 0));
                break;
            }
        }
        assert_eq!(game.table().lives, lives - 1, "bounded natural drain");
    }

    #[test]
    fn journal_repress_launches_naturally_waiting_replacement() {
        for button in [0, 1] {
            let mut game = PinballGame::new(false);
            let mut player = player();
            feed(&[]);
            assert_eq!(
                frame(&mut game, &mut player, 0.),
                PinballFeedback::default()
            );
            feed(&[button]);
            assert_eq!(frame(&mut game, &mut player, TICK).launches, 1);
            wait_for_drain(&mut game, &mut player);
            assert_eq!(game.table().lives, 2);
            assert!(game.table().waiting);
            assert!(game.table().score > 0, "real contact preceded the drain");
            assert_eq!(frame(&mut game, &mut player, 16. * TICK).launches, 0);
            assert!(game.table().waiting, "a held control cannot serve again");

            // No host frame separates these events. The production adapter
            // must retain the journal's release/repress despite a final held value.
            feed(&[]);
            feed(&[button]);
            let events = frame(&mut game, &mut player, 16. * TICK);
            assert_eq!(events.launches, 1, "action on host button {button}");
            assert_eq!(game.table().launches, 2);
            assert!(!game.table().waiting);
            assert_eq!(frame(&mut game, &mut player, 16. * TICK).launches, 0);
        }
    }

    #[test]
    fn journal_taps_coalesce_across_zero_ticks_and_release_flippers_in_catch_up() {
        for (buttons, pointer, flippers) in [
            (&[0, 4, 5][..], POINTER, [true; 2]),
            (&[1][..], vec2(200., 750.), [true, false]),
            (&[1][..], vec2(600., 750.), [false, true]),
        ] {
            let mut game = PinballGame::new(false);
            let mut player = player();
            feed(&[]);
            frame(&mut game, &mut player, 0.);
            let position = game.table().position();
            for _ in 0..4 {
                // Complete taps within each journal sample end with value zero.
                feed(buttons);
                feed(&[]);
                assert_eq!(
                    frame_at(&mut game, &mut player, 0.2 * TICK, pointer),
                    PinballFeedback::default()
                );
                assert_eq!(game.session().tick(), 0);
                assert_eq!(game.table().position(), position);
                assert_eq!(game.table().flipper_pressed, [false; 2]);
            }
            let events = frame_at(&mut game, &mut player, 8.2 * TICK, pointer);
            assert_eq!(game.session().tick(), 9);
            assert_eq!(events.launches, 1);
            assert_eq!(events.flipper_presses, flippers);
            let mut reference = PinballGame::new(false);
            assert_eq!(
                reference.advance(
                    TICK,
                    Controls {
                        left: flippers[0],
                        right: flippers[1],
                        launch: true,
                    }
                ),
                events
            );
            for _ in 0..8 {
                reference.advance(TICK, Controls::default());
            }
            assert_eq!(game.table().position(), reference.table().position());
            assert_eq!(game.table().angles, reference.table().angles);
            assert_eq!(game.table().trail, reference.table().trail);
            assert_eq!(game.table().ball_save, reference.table().ball_save);
            assert_eq!(game.table().flipper_pressed, [false; 2]);
            assert_eq!(
                frame(&mut game, &mut player, TICK),
                PinballFeedback::default()
            );
            assert_eq!(game.table().launches, 1);
        }
    }

    #[test]
    fn journal_repress_is_consumed_before_a_natural_drain_in_the_same_batch() {
        for button in [0, 1] {
            let mut reference = PinballGame::new(false);
            let mut player = player();
            feed(&[]);
            frame(&mut reference, &mut player, 0.);
            feed(&[button]);
            assert_eq!(frame(&mut reference, &mut player, TICK).launches, 1);
            wait_for_drain(&mut reference, &mut player);
            let drain_tick = reference.session().tick();
            assert!(drain_tick > 4);

            // Replay the same actual controls up to four ticks before the
            // observed drain. The next catch-up batch crosses that drain.
            let mut game = PinballGame::new(false);
            feed(&[]);
            frame(&mut game, &mut player, 0.);
            feed(&[button]);
            assert_eq!(frame(&mut game, &mut player, TICK).launches, 1);
            for _ in 1..drain_tick - 4 {
                assert_eq!(frame(&mut game, &mut player, TICK).launches, 0);
            }
            assert_eq!(game.table().drains, 0);
            assert!(!game.table().waiting);
            feed(&[]);
            feed(&[button]);
            let before = game.session().tick();
            assert_eq!(
                frame(&mut game, &mut player, 0.),
                PinballFeedback::default()
            );
            assert_eq!(game.session().tick(), before);
            let events = frame(&mut game, &mut player, 16. * TICK);
            assert_eq!((events.launches, events.drains, events.saves), (0, 1, 0));
            assert!(game.table().waiting);
            assert_eq!(game.table().lives, 2);
            assert_eq!(frame(&mut game, &mut player, 16. * TICK).launches, 0);
            assert!(
                game.table().waiting,
                "rejected input cannot become a later serve"
            );

            // Release/repress across a zero-tick host sample must survive until
            // a real tick, and several such presses coalesce to one serve.
            for _ in 0..3 {
                feed(&[]);
                feed(&[button]);
                assert_eq!(
                    frame(&mut game, &mut player, 0.),
                    PinballFeedback::default()
                );
                assert!(game.table().waiting);
            }
            assert_eq!(frame(&mut game, &mut player, 16. * TICK).launches, 1);
            assert_eq!(game.table().launches, 2);
            assert_eq!(frame(&mut game, &mut player, 16. * TICK).launches, 0);
        }
    }

    #[test]
    fn journal_pause_and_restart_discard_edges_and_suppress_held_controls() {
        for buttons in [&[0, 4, 5][..], &[1][..]] {
            let mut game = PinballGame::new(false);
            let mut player = player();
            feed(&[]);
            frame(&mut game, &mut player, 0.);
            feed(buttons);
            assert_eq!(
                frame(&mut game, &mut player, 0.25 * TICK),
                PinballFeedback::default()
            );
            player.set_paused(true);
            assert_eq!(
                frame(&mut game, &mut player, 60.),
                PinballFeedback::default()
            );
            feed(&[]);
            feed(buttons); // A tap while paused cannot leak into resume.
            assert_eq!(
                frame(&mut game, &mut player, 60.),
                PinballFeedback::default()
            );
            assert_eq!(game.session().tick(), 0);
            player.set_paused(false);
            assert_eq!(
                frame(&mut game, &mut player, 16. * TICK),
                PinballFeedback::default()
            );
            assert!(game.table().waiting);
            assert_eq!(game.table().flipper_pressed, [false; 2]);
            feed(&[]);
            feed(buttons);
            assert_eq!(
                frame(&mut game, &mut player, 0.),
                PinballFeedback::default()
            );

            // These are the production retry operations: clear both the game
            // buffer and physical held input, including the just-queued tap.
            game.restart();
            player.set_paused(true);
            player.set_paused(false);
            assert_eq!(
                frame(&mut game, &mut player, 16. * TICK),
                PinballFeedback::default()
            );
            assert!(game.table().waiting);
            assert_eq!(game.table().launches, 0);
            assert_eq!(game.table().flipper_pressed, [false; 2]);
            feed(&[]);
            feed(buttons);
            let events = frame(&mut game, &mut player, TICK);
            assert_eq!(events.launches, 1);
            assert_eq!(events.flipper_presses, [buttons.len() == 3; 2]);
        }
    }

    #[test]
    fn journal_terminal_retry_requires_release_and_a_new_press() {
        use ferravex_core::session::SessionState;

        let mut game = PinballGame::new(false);
        let mut player = player();
        feed(&[]);
        frame(&mut game, &mut player, 0.);
        for lives in [2, 1, 0] {
            feed(&[]);
            feed(&[0]);
            assert_eq!(frame(&mut game, &mut player, TICK).launches, 1);
            wait_for_drain(&mut game, &mut player);
            assert_eq!(game.table().lives, lives);
        }
        assert_eq!(game.session().state(), SessionState::Lost);
        let tick = game.session().tick();
        let time = game.table().time;
        feed(&[]);
        feed(&[0]);
        assert_eq!(
            frame(&mut game, &mut player, 60.),
            PinballFeedback::default()
        );
        assert_eq!(game.session().tick(), tick);
        assert_eq!(game.table().time, time);
        game.restart();
        player.set_paused(true);
        player.set_paused(false);
        assert_eq!(
            frame(&mut game, &mut player, 16. * TICK),
            PinballFeedback::default()
        );
        assert_eq!(game.table().lives, 3);
        assert_eq!(game.table().score, 0);
        assert_eq!(game.table().launches, 0);
        assert!(game.table().waiting);
        feed(&[]);
        feed(&[0]);
        assert_eq!(frame(&mut game, &mut player, TICK).launches, 1);
    }
}
