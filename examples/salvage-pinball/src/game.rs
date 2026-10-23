use crate::{Controls, Table};
use ferravex_core::session::{GameSession, SessionState};

/// Confirmed simulation events for one rendered frame, suitable for shared
/// `GameAudio` cues. Zero-tick, paused and terminal frames return no events.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PinballFeedback {
    pub points: u32,
    pub hits: u32,
    pub launches: u32,
    pub drains: u32,
    pub saves: u32,
    pub flipper_presses: [bool; 2],
}

/// Host edges and final levels are distinct: a release/repress can end held,
/// and a complete tap can end released. The buffer coalesces either until a tick.
#[derive(Default)]
pub(crate) struct ControlSample {
    pub held: Controls,
    pub pressed: Controls,
    pub released: Controls,
}

impl ControlSample {
    // Compatibility for callers that supply only held levels. The live adapter
    // supplies journal edges directly instead of deriving them a second time.
    fn from_held(held: Controls, previous: Controls) -> Self {
        Self {
            held,
            pressed: Controls {
                left: held.left && !previous.left,
                right: held.right && !previous.right,
                launch: held.launch && !previous.launch,
            },
            released: Controls {
                left: !held.left,
                right: !held.right,
                launch: !held.launch,
            },
        }
    }
}

#[derive(Default)]
struct BufferedControls {
    held: Controls,
    pending: Controls,
    blocked: Controls,
}

impl BufferedControls {
    fn sample(&mut self, sample: ControlSample) {
        for (raw, pressed, released, held, pending, blocked) in [
            (
                sample.held.left,
                sample.pressed.left,
                sample.released.left,
                &mut self.held.left,
                &mut self.pending.left,
                &mut self.blocked.left,
            ),
            (
                sample.held.right,
                sample.pressed.right,
                sample.released.right,
                &mut self.held.right,
                &mut self.pending.right,
                &mut self.blocked.right,
            ),
            (
                sample.held.launch,
                sample.pressed.launch,
                sample.released.launch,
                &mut self.held.launch,
                &mut self.pending.launch,
                &mut self.blocked.launch,
            ),
        ] {
            // A final neutral level or an explicit release/repress rearms a
            // blocked control. A release from just one merged source must not
            // reactivate another source that is still held without a new press.
            *blocked &= raw && !(released && pressed);
            *pending |= pressed && !*blocked;
            *held = raw && !*blocked;
        }
    }

    fn take(&mut self) -> Controls {
        let pending = std::mem::take(&mut self.pending);
        Controls {
            left: self.held.left || pending.left,
            right: self.held.right || pending.right,
            launch: pending.launch,
        }
    }

    fn clear(&mut self) {
        self.blocked.left |= self.held.left;
        self.blocked.right |= self.held.right;
        self.blocked.launch |= self.held.launch;
        self.held = Controls::default();
        self.pending = Controls::default();
    }
}

/// A three-ball game with a 120 Hz [`GameSession`] and bounded catch-up.
/// The table owns pinball rules; the session owns start, pause and game-over.
///
/// ```
/// use salvage_pinball::{Controls, PinballGame};
/// use ferravex_core::session::SessionState;
///
/// let mut game = PinballGame::new(false);
/// assert_eq!(game.session().state(), SessionState::Ready);
/// let held_launch = Controls { launch: true, ..Controls::default() };
/// game.advance(0.0, held_launch); // start and retain the edge until a tick
/// let events = game.advance(1.0 / 60.0, held_launch);
/// assert_eq!(events.launches, 1);
/// assert!(game.set_paused(true));
/// let frozen = game.table().time;
/// game.advance(3.0, Controls::default());
/// assert_eq!(game.table().time, frozen);
/// game.restart(); // fresh three-ball game, cleared clock and queued input
/// assert_eq!(game.table().lives, 3);
/// ```
pub struct PinballGame {
    table: Table,
    session: GameSession,
    input: BufferedControls,
    autoplay: bool,
}

impl PinballGame {
    /// Autoplay starts immediately and uses [`Table::autoplay`] each fixed tick.
    pub fn new(autoplay: bool) -> Self {
        Self {
            table: Table::new(),
            session: GameSession::new(120., 16, autoplay),
            input: BufferedControls::default(),
            autoplay,
        }
    }

    /// The current authoritative table, including score and ball-save status.
    pub fn table(&self) -> &Table {
        &self.table
    }

    /// Read-only lifecycle and fixed-clock state.
    pub fn session(&self) -> &GameSession {
        &self.session
    }

    /// Enter play without launching; a launch press can also start from Ready.
    pub fn start(&mut self) -> bool {
        self.session.start()
    }

    /// Pause or resume. Queued presses are discarded and held controls must be
    /// released before they can act again; inactive elapsed time never accrues.
    pub fn set_paused(&mut self, paused: bool) -> bool {
        let changed = self.session.set_paused(paused);
        if changed {
            self.input.clear();
            self.table.flipper_pressed = [false; 2];
        }
        changed
    }

    /// Start a fresh three-ball game, clearing score, timers, clock and feedback.
    /// Controls held on the last frame remain suppressed until released.
    pub fn restart(&mut self) {
        self.table = Table::new();
        self.input.clear();
        self.session.restart();
    }

    /// Return to the ready screen with a fresh table and no pending input.
    pub fn reset(&mut self) {
        self.restart();
        self.session.reset();
    }

    /// Sample held input once per render frame and simulate available fixed
    /// ticks. Quick taps survive zero-tick frames. Each launch rising edge is
    /// consumed once, even if a ball drains during a multi-tick catch-up frame.
    /// A launch in mid-play is consumed without serving a later ball.
    /// Autoplay ignores supplied controls. Retry is explicit via [`Self::restart`].
    pub fn advance(&mut self, elapsed_seconds: f64, controls: Controls) -> PinballFeedback {
        self.advance_sample(
            elapsed_seconds,
            ControlSample::from_held(controls, self.input.held),
        )
    }

    /// Transfer the live adapter's edges into bounded storage, then consume
    /// pending actions only on executed ticks. This is private to the demo;
    /// callers of the public held-controls API retain its existing semantics.
    pub(crate) fn advance_sample(
        &mut self,
        elapsed_seconds: f64,
        sample: ControlSample,
    ) -> PinballFeedback {
        self.input.sample(if self.autoplay {
            ControlSample::default()
        } else {
            sample
        });
        if self.session.state() == SessionState::Ready && self.input.pending.launch {
            self.session.start();
        }
        if !self.session.is_playing() {
            self.input.clear();
            return PinballFeedback::default();
        }
        let frame = self.session.advance(elapsed_seconds);
        let before = (
            self.table.score,
            self.table.hits,
            self.table.launches,
            self.table.drains,
            self.table.saves,
        );
        let mut feedback = PinballFeedback::default();
        for _ in 0..frame.steps {
            let controls = if self.autoplay {
                self.table.autoplay()
            } else {
                self.input.take()
            };
            for (i, pressed) in [controls.left, controls.right].into_iter().enumerate() {
                feedback.flipper_presses[i] |= pressed && !self.table.flipper_pressed[i];
            }
            self.table.step(frame.dt, controls);
            if self.table.over() {
                self.session.finish(false);
                self.input.clear();
                self.table.flipper_pressed = [false; 2];
                break;
            }
        }
        feedback.points = self.table.score - before.0;
        feedback.hits = self.table.hits - before.1;
        feedback.launches = self.table.launches - before.2;
        feedback.drains = self.table.drains - before.3;
        feedback.saves = self.table.saves - before.4;
        feedback
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_physics::d2::Vector;

    const DT: f64 = 1. / 120.;
    const LAUNCH: Controls = Controls {
        left: false,
        right: false,
        launch: true,
    };

    fn force_drain(game: &mut PinballGame) {
        game.table.world.bodies[game.table.ball].set_translation(Vector::new(4.9, 17.2), true);
    }

    #[test]
    fn ready_and_paused_time_never_catches_up_and_resume_discards_taps() {
        let mut game = PinballGame::new(false);
        game.advance(30., Controls::default());
        assert_eq!(game.session.state(), SessionState::Ready);
        assert_eq!(game.table.time, 0.);
        assert!(game.start());
        assert!(game.set_paused(true));
        game.advance(30., LAUNCH);
        assert_eq!(game.session.tick(), 0);
        assert!(game.set_paused(false));
        game.advance(DT, LAUNCH);
        assert_eq!(game.session.tick(), 1);
        assert_eq!(game.table.launches, 0, "held resume input must not launch");
        game.advance(0., Controls::default());
        game.advance(DT, LAUNCH);
        assert_eq!(game.table.launches, 1);
    }

    #[test]
    fn quick_taps_survive_zero_ticks_and_flippers_release_on_next_tick() {
        let mut game = PinballGame::new(false);
        let tap = Controls {
            left: true,
            right: true,
            launch: true,
        };
        assert_eq!(game.advance(0., tap), PinballFeedback::default());
        assert_eq!(
            game.advance(0., Controls::default()),
            PinballFeedback::default()
        );
        let events = game.advance(DT, Controls::default());
        assert_eq!(events.launches, 1);
        assert_eq!(events.flipper_presses, [true; 2]);
        assert_eq!(game.table.flipper_pressed, [true; 2]);
        assert_eq!(
            game.advance(DT, Controls::default()),
            PinballFeedback::default()
        );
        assert_eq!(game.table.flipper_pressed, [false; 2]);
    }

    #[test]
    fn held_launch_cannot_serve_again_after_drain_during_catch_up() {
        let mut game = PinballGame::new(false);
        // The first tick launches and immediately saves this out-of-bounds ball.
        // The remaining catch-up ticks must leave the replacement in its lane.
        force_drain(&mut game);
        let events = game.advance(8. * DT, LAUNCH);
        assert_eq!((events.launches, events.saves, events.drains), (1, 1, 1));
        assert!(game.table.waiting);
        game.advance(8. * DT, LAUNCH);
        assert_eq!(game.table.launches, 1);
        game.advance(0., Controls::default());
        game.advance(DT, LAUNCH);
        assert_eq!(game.table.launches, 2);
    }

    #[test]
    fn final_drain_finishes_once_and_retry_resets_all_game_state() {
        let mut game = PinballGame::new(false);
        game.start();
        game.table.score = 500;
        for lives in [2, 1, 0] {
            force_drain(&mut game);
            let events = game.advance(8. * DT, Controls::default());
            assert_eq!(events.drains, 1);
            assert_eq!(game.table.lives, lives);
        }
        assert_eq!(game.session.state(), SessionState::Lost);
        let time = game.table.time;
        assert_eq!(game.advance(10., LAUNCH), PinballFeedback::default());
        assert_eq!(game.table.time, time);
        assert!(!game.start());
        assert!(!game.set_paused(true));
        game.restart();
        assert_eq!(game.session.state(), SessionState::Playing);
        assert_eq!(game.session.tick(), 0);
        assert_eq!(
            (game.table.score, game.table.lives, game.table.drains),
            (0, 3, 0)
        );
        assert_eq!(game.table.bumper_notice, [0.; 3]);
        assert_eq!(game.table.drain_notice, 0.);
        assert_eq!(game.table.ball_save, 0.);
        assert_eq!(game.table.save_notice, 0.);
        assert!(game.table.waiting);
        game.advance(DT, LAUNCH);
        assert_eq!(
            game.table.launches, 0,
            "retry must discard held terminal input"
        );
        game.advance(0., Controls::default());
        game.advance(DT, LAUNCH);
        assert_eq!(game.table.launches, 1);
        assert!(game.table.ball_save > 4.9);
        game.reset();
        assert_eq!(game.session.state(), SessionState::Ready);
        assert_eq!(game.session.tick(), 0);
        assert_eq!(game.table.time, 0.);
    }

    #[test]
    fn contact_feedback_is_emitted_once_and_cleared_by_restart() {
        let mut game = PinballGame::new(false);
        game.start();
        game.table.waiting = false;
        game.table.world.bodies[game.table.ball].set_translation(Vector::new(2.6, 3.32), true);
        game.table.world.bodies[game.table.ball].set_linvel(Vector::new(0., 2.), true);
        let events = game.advance(DT, Controls::default());
        assert_eq!((events.points, events.hits), (100, 1));
        assert!(game.table.bumper_notice[0] > 0.7);
        let events = game.advance(8. * DT, Controls::default());
        assert_eq!((events.points, events.hits), (0, 0));
        game.restart();
        assert_eq!(game.table.bumper_notice, [0.; 3]);
        assert_eq!(
            game.advance(DT, Controls::default()),
            PinballFeedback::default()
        );
    }

    #[test]
    fn pause_preserves_live_ball_save_and_restart_discards_fractional_time_and_taps() {
        let mut game = PinballGame::new(false);
        game.advance(DT, LAUNCH);
        let position = game.table.position();
        let save = game.table.ball_save;
        game.set_paused(true);
        game.advance(60., Controls::default());
        assert_eq!(game.table.position(), position);
        assert_eq!(game.table.ball_save, save);
        game.set_paused(false);
        force_drain(&mut game);
        let saved = game.advance(DT, Controls::default());
        assert_eq!((saved.saves, game.table.lives), (1, 3));
        game.advance(
            0.75 * DT,
            Controls {
                left: true,
                ..LAUNCH
            },
        );
        game.restart();
        let events = game.advance(0.5 * DT, Controls::default());
        assert_eq!(events, PinballFeedback::default());
        assert_eq!(
            game.session.tick(),
            0,
            "old fractional time must not survive retry"
        );
        game.advance(0.5 * DT, Controls::default());
        assert_eq!(
            game.table.launches, 0,
            "buffered launch must not survive retry"
        );
        assert_eq!(game.table.flipper_pressed, [false; 2]);
    }

    #[test]
    fn autoplay_session_still_plays_and_scores_at_different_render_rates() {
        let mut slow = PinballGame::new(true);
        let mut fast = PinballGame::new(true);
        for _ in 0..1800 {
            slow.advance(1. / 30., Controls::default());
            for _ in 0..4 {
                fast.advance(DT, Controls::default());
            }
        }
        assert!(slow.table.launches > 0);
        assert!(slow.table.hits >= 2);
        assert_eq!(
            (slow.table.score, slow.table.lives, slow.table.drains),
            (fast.table.score, fast.table.lives, fast.table.drains)
        );
        assert_eq!(slow.table.position(), fast.table.position());
    }

    #[test]
    fn ball_recovery_totals_survive_a_save_and_bank_only_on_an_unsaved_drain() {
        let mut game = PinballGame::new(false);
        game.advance(DT, LAUNCH);
        game.table.world.bodies[game.table.ball].set_translation(Vector::new(2.6, 3.32), true);
        game.table.world.bodies[game.table.ball].set_linvel(Vector::new(0., 2.), true);
        let hit = game.advance(DT, Controls::default());
        assert_eq!((hit.points, game.table.ball_points()), (100, 100));
        force_drain(&mut game);
        let saved = game.advance(DT, Controls::default());
        assert_eq!((saved.saves, game.table.lives), (1, 3));
        assert_eq!(game.table.ball_points(), 100);
        assert_eq!(game.table.last_ball_points(), None);

        game.advance(DT, LAUNCH);
        force_drain(&mut game);
        let drained = game.advance(8. * DT, Controls::default());
        assert_eq!((drained.drains, drained.saves, drained.points), (1, 0, 0));
        assert_eq!((game.table.score, game.table.ball_points()), (100, 0));
        assert_eq!(game.table.last_ball_points(), Some(100));
        // The next empty life replaces the previous summary without awarding
        // an extra bonus, even through a terminal catch-up frame.
        for _ in 0..2 {
            force_drain(&mut game);
            game.advance(8. * DT, Controls::default());
        }
        assert_eq!(game.session.state(), SessionState::Lost);
        assert_eq!(game.table.last_ball_points(), Some(0));
        assert_eq!(game.table.score, 100);
        game.restart();
        assert_eq!(game.table.ball_points(), 0);
        assert_eq!(game.table.last_ball_points(), None);
        assert_eq!(game.table.bumper_hits, [0; 3]);
    }
}
