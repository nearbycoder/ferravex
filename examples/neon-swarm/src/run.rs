//! Testable local host: Game owns combat rules; GameSession owns lifecycle/time.
use crate::game::{Game, Input, Phase};
use ferravex_core::session::{GameSession, SessionState};
use ferravex_play::feedback::FeedbackEvent;
use ferravex_play::input::Action;

pub(crate) struct Run {
    pub game: Game,
    pub session: GameSession,
}

impl Run {
    pub fn new(autoplay: bool) -> Self {
        Self {
            game: Game::new(),
            session: GameSession::new(120., 12, autoplay),
        }
    }

    pub fn restart(&mut self) {
        self.game = Game::new();
        self.session.restart();
    }

    /// Start, explicitly resume, or retry through the same overlay action.
    pub fn activate(&mut self) -> bool {
        match self.session.state() {
            SessionState::Ready => self.session.start(),
            SessionState::Paused => self.session.set_paused(false),
            SessionState::Won | SessionState::Lost => {
                self.restart();
                true
            }
            SessionState::Playing => false,
        }
    }

    /// Focus loss and settings suspend a run until an explicit resume. Queued
    /// dash taps do not survive the interruption, while cooldowns remain frozen.
    pub fn pause(&mut self) -> bool {
        if !self.session.set_paused(true) {
            return false;
        }
        self.game.clear_input_buffer();
        true
    }

    pub fn advance(
        &mut self,
        elapsed: f64,
        mut input: impl FnMut(&Game) -> Input,
        mut feedback: impl FnMut(FeedbackEvent),
    ) {
        let frame = self.session.advance(elapsed);
        for _ in 0..frame.steps {
            let input = input(&self.game);
            self.game
                .update_with_feedback(input, frame.dt, &mut feedback);
            if self.game.phase != Phase::Playing {
                self.session.finish(self.game.phase == Phase::Won);
                break;
            }
        }
    }
}

/// Called only on a simulation tick, after PlayerInput::take consumed each edge.
/// Preserve held fire, but keep a dash edge separate from its held value: two
/// quick taps on consecutive ticks must not look like one continuous hold.
pub(crate) fn combat_input(mut input: Input, fire: Action, dash: Action, auto_fire: bool) -> Input {
    input.fire = fire.pressed || fire.value > 0.5 || auto_fire;
    input.dash = dash.value > 0.5;
    input.dash_pressed = dash.pressed;
    input
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_play::feedback::FeedbackKind;
    use ferravex_play::input::{ActionKind, ActionMap, Binding, ContextId, InputCode};

    #[test]
    fn ready_pause_refocus_and_retry_do_not_accumulate_time_or_consume_inputs() {
        let mut run = Run::new(false);
        run.advance(30., |_| panic!("ready must not consume input"), |_| {});
        assert_eq!(run.game.time, 0.);
        assert!(run.activate());
        assert!(!run.activate());
        run.advance(1. / 60., |_| Input::default(), |_| {});
        let (time, tick) = (run.game.time, run.session.tick());
        assert!(run.pause());
        assert!(!run.pause());
        run.advance(30., |_| panic!("paused must not consume input"), |_| {});
        assert_eq!((run.game.time, run.session.tick()), (time, tick));
        // Focus returning alone does not activate the run.
        run.advance(1. / 60., |_| panic!("explicit resume required"), |_| {});
        assert!(run.activate());
        run.advance(1. / 120., |_| Input::default(), |_| {});
        assert_eq!(run.session.tick(), tick + 1);
        assert!((run.game.time - time - 1. / 120.).abs() < 1e-6);
        run.restart();
        assert_eq!(run.session.state(), SessionState::Playing);
        assert_eq!(run.session.tick(), 0);
        assert_eq!(run.game.time, 0.);
        assert_eq!(run.game.world.len(), 0);
        assert_eq!(run.game.health, 100);
        assert_eq!(run.game.shield.value(), 20.);
        assert_eq!(run.game.magazine.loaded(), 12);
        assert_eq!(run.game.score.total(), 0);
        assert_eq!(run.game.progression.level(), 1);
        assert!(run.game.hit_marker.is_none());
    }

    #[test]
    fn consumed_quick_tap_survives_zero_step_frame_and_catchup_never_repeats_it() {
        let mut input = ActionMap::default();
        let dash = input
            .named_action(
                "boost",
                ActionKind::Button,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .unwrap();
        let fire = input
            .named_action(
                "fire",
                ActionKind::Button,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::Mouse(1))],
            )
            .unwrap();
        let mut run = Run::new(true);
        input.feed(InputCode::Key(32), 1.);
        input.feed(InputCode::Key(32), 0.);
        input.feed(InputCode::Mouse(1), 1.);
        input.feed(InputCode::Mouse(1), 0.);
        let mut dashes = 0;
        for elapsed in [0., 0.1, 0.1] {
            run.advance(
                elapsed,
                |_| {
                    combat_input(
                        Input::default(),
                        input.consume_tick(fire),
                        input.consume_tick(dash),
                        false,
                    )
                },
                |event| dashes += usize::from(matches!(event.kind, FeedbackKind::Dash)),
            );
        }
        assert_eq!(dashes, 1);
        assert_eq!(run.game.shots, 1);
        assert_eq!(run.session.tick(), 24);
        assert!(run.game.dash_cooldown > 1.4);
    }

    #[test]
    fn pause_discards_a_buffered_dash_without_resetting_its_charge() {
        let mut run = Run::new(true);
        let tap = Input {
            dash_pressed: true,
            ..Input::default()
        };
        run.advance(1. / 120., |_| tap, |_| {});
        for _ in 0..190 {
            run.advance(1. / 120., |_| Input::default(), |_| {});
        }
        assert!(run.game.dash_cooldown > 0. && run.game.dash_cooldown < 0.15);
        run.advance(1. / 120., |_| tap, |_| panic!("dash is still charging"));
        let cooldown = run.game.dash_cooldown;
        assert!(run.pause());
        run.advance(30., |_| panic!("paused"), |_| panic!("paused"));
        assert_eq!(run.game.dash_cooldown, cooldown);
        assert!(run.activate());
        for _ in 0..30 {
            run.advance(
                1. / 120.,
                |_| Input::default(),
                |event| {
                    assert_ne!(
                        event.kind,
                        FeedbackKind::Dash,
                        "paused tap must be discarded"
                    );
                },
            );
        }
        assert_eq!(run.game.dash_cooldown, 0.);
        let mut dashes = 0;
        run.advance(
            1. / 120.,
            |_| tap,
            |event| {
                dashes += usize::from(event.kind == FeedbackKind::Dash);
            },
        );
        assert_eq!(dashes, 1, "a new tap works after resuming");
    }

    #[test]
    fn terminal_frame_stops_catchup_and_retry_works_for_both_outcomes() {
        let mut run = Run::new(true);
        run.game.health = 0;
        let mut inputs = 0;
        run.advance(
            0.1,
            |_| {
                inputs += 1;
                Input::default()
            },
            |_| {},
        );
        assert_eq!(inputs, 1, "stop inside the frame that ends the run");
        assert_eq!(run.session.state(), SessionState::Lost);
        assert_eq!(run.game.time, run.session.dt());
        run.advance(30., |_| panic!("terminal must not consume input"), |_| {});
        assert!(!run.pause());
        assert!(run.activate());
        assert_eq!(run.game.phase, Phase::Playing);
        assert_eq!(run.game.health, 100);
        assert_eq!(run.session.tick(), 0);
        // Victory is exercised through ordinary gameplay below.
        play_to_win(&mut run, &[1. / 60.]);
        assert!(run.activate());
        assert_eq!(run.session.tick(), 0);
        assert_eq!(run.game.kills, 0);
        assert_eq!(run.game.wave, 1);
    }

    fn play_to_win(run: &mut Run, cadence: &[f64]) {
        for frame in 0..20_000 {
            run.advance(cadence[frame % cadence.len()], Game::autoplay, |_| {});
            if !run.session.is_playing() {
                break;
            }
        }
        assert_eq!(run.session.state(), SessionState::Won);
        assert_eq!(run.game.phase, Phase::Won);
        assert_eq!(run.game.kills, 41);
    }

    #[test]
    fn autoplay_wins_deterministically_across_render_cadences_and_pause() {
        let mut regular = Run::new(true);
        play_to_win(&mut regular, &[1. / 60.]);
        let mut irregular = Run::new(true);
        irregular.advance(0.1, Game::autoplay, |_| {});
        irregular.pause();
        irregular.advance(60., |_| panic!("paused"), |_| {});
        irregular.activate();
        play_to_win(&mut irregular, &[0.001, 1. / 30., 1. / 144., 0.075]);
        assert_eq!(regular.game.time, irregular.game.time);
        assert_eq!(regular.game.health, irregular.game.health);
        assert_eq!(regular.game.player, irregular.game.player);
        assert_eq!(regular.game.shots, irregular.game.shots);
        assert_eq!(regular.game.score.total(), irregular.game.score.total());
    }

    #[test]
    fn accepted_notes_and_style_match_across_render_cadences() {
        let collect = |cadence: &[f64]| {
            let mut run = Run::new(true);
            let mut notes = Vec::new();
            let mut all = Vec::new();
            for frame in 0..20_000 {
                run.advance(cadence[frame % cadence.len()], Game::autoplay, |_| {});
                run.game.take_notes(&mut notes);
                all.extend(notes.drain(..).map(|n| format!("{n:?}")));
                if !run.session.is_playing() {
                    break;
                }
            }
            assert_eq!(run.session.state(), SessionState::Won);
            (all, run.game.style)
        };
        let (regular, style) = collect(&[1. / 60.]);
        let (irregular, other) = collect(&[0.001, 1. / 30., 1. / 144., 0.075]);
        assert_eq!(regular, irregular);
        assert_eq!(style, other);
        assert!(style.overseer_down, "autoplay defeats the Overseer");
        assert_eq!(regular.iter().filter(|n| n.starts_with("Kill")).count(), 41);
        assert_eq!(
            regular
                .iter()
                .filter(|n| n.starts_with("EliteInbound"))
                .count(),
            1
        );
        assert_eq!(
            regular
                .iter()
                .filter(|n| n.starts_with("WaveClear"))
                .count(),
            3
        );
    }
}
