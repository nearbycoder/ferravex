//! Orbit Breaker: original gameplay, original CPU raster, and only first-party Ferravex crates.
pub mod game;
#[cfg(any(target_arch = "wasm32", test))]
mod interactive;
#[cfg(test)]
mod journey200_tests;
mod juice;
mod presentation;
mod quips;
mod raster;
mod readable;
use ferravex_core::session::{GameSession, SessionState};
use ferravex_engine::input::{ActionKind, ActionMap, Binding, ContextId, InputCode};
use game::{Game, HEIGHT, Input, WIDTH};
use raster::Canvas;
use std::sync::Mutex;

/// A finished run awaiting the host's record keeping. See [`Orbit::take_finished_run`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FinishedRun {
    pub won: bool,
    pub score: u32,
    /// Simulated run time in seconds.
    pub seconds: f32,
    /// Whether this score beat the personal best supplied by the host.
    pub new_best: bool,
}

pub struct Orbit {
    pub game: Game,
    pub canvas: Canvas,
    session: GameSession,
    actions: ActionMap,
    axis: usize,
    launch: usize,
    restart: usize,
    pending_launch: bool,
    presentation: presentation::Presentation,
    readable_status: readable::ReadableStatus,
    unrecorded: Option<FinishedRun>,
}
impl Default for Orbit {
    fn default() -> Self {
        Self::new()
    }
}
impl Orbit {
    pub fn new() -> Self {
        let mut actions = ActionMap::default();
        let axis = actions.action(vec![Binding {
            code: InputCode::PadAxis(0),
            scale: 1.0,
            deadzone: 0.02,
        }]);
        let launch = actions
            .named_action(
                "launch",
                ActionKind::Button,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .expect("valid launch action");
        let restart = actions
            .named_action(
                "restart",
                ActionKind::Button,
                ContextId::GLOBAL,
                vec![Binding::digital(InputCode::Key(82))],
            )
            .expect("valid restart action");
        let mut app = Self {
            game: Game::new(),
            canvas: Canvas::new(),
            // The simulation's Ready phase still steers and charges the serve.
            // Session Playing means the local simulation is allowed to advance.
            session: GameSession::new(120.0, 12, true),
            actions,
            axis,
            launch,
            restart,
            pending_launch: false,
            presentation: presentation::Presentation::default(),
            readable_status: readable::ReadableStatus::default(),
            unrecorded: None,
        };
        app.render();
        app
    }
    /// Advance the legacy host with held buttons (bit 0 launch, bit 1 restart).
    /// Press/release edges are latched even when elapsed time produces no tick.
    pub fn advance(&mut self, dt: f64, axis: f32, buttons: u32) {
        self.actions.feed(InputCode::PadAxis(0), axis);
        self.actions
            .feed(InputCode::Key(32), f32::from(buttons & 1 != 0));
        self.actions
            .feed(InputCode::Key(82), f32::from(buttons & 2 != 0));
        let input = Input {
            axis: self.actions.consume_tick(self.axis).value,
            launch: self.actions.consume_tick(self.launch).pressed,
            restart: self.actions.consume_tick(self.restart).pressed,
        };
        self.advance_input(dt, input);
    }

    /// Advance with an axis level and **press edges**, sampled once per host frame.
    /// Launch waits for the next fixed tick; catch-up ticks never repeat the edge.
    /// Restart and terminal retry apply immediately, discard that frame's elapsed
    /// time, and leave a fresh ball ready for a separate launch press.
    ///
    /// ```
    /// use owned_orbit::{Orbit, game::{Input, Phase}};
    /// let mut orbit = Orbit::new();
    /// orbit.advance_input(0.0, Input { launch: true, ..Input::default() });
    /// orbit.advance_input(1.0 / 120.0, Input::default());
    /// assert_eq!(orbit.game.phase, Phase::Playing);
    /// ```
    pub fn advance_input(&mut self, dt: f64, input: Input) {
        self.presentation.begin_frame(dt);
        self.sync_presentation();
        if input.restart {
            self.restart();
            return;
        }
        if self.session.is_paused() {
            self.pending_launch = false;
            return;
        }
        if matches!(self.game.phase, game::Phase::Won | game::Phase::Lost) {
            self.session.finish(self.game.phase == game::Phase::Won);
            self.finish();
            self.pending_launch = false;
            if self.game.phase == game::Phase::Won {
                self.presentation.juice.celebrate(dt as f32);
            }
            if input.launch {
                self.restart();
            }
            return;
        }
        self.pending_launch |= input.launch;
        let step = self.session.advance(dt);
        for _ in 0..step.steps {
            self.game.tick(
                Input {
                    axis: input.axis,
                    launch: std::mem::take(&mut self.pending_launch),
                    restart: false,
                },
                step.dt,
            );
            self.presentation.accept_tick(self.game.events.drain());
            let juice = &mut self.presentation.juice;
            juice.tick(step.dt);
            if matches!(self.game.phase, game::Phase::Ready | game::Phase::Playing) {
                juice.steered(input.axis, step.dt);
            }
            juice.charge_learned |= self.game.armor_piercing();
            if matches!(self.game.phase, game::Phase::Won | game::Phase::Lost) {
                self.session.finish(self.game.phase == game::Phase::Won);
                self.finish();
                break;
            }
        }
    }

    /// Build the results card once, from the accepted final state.
    fn finish(&mut self) {
        if self.presentation.results.is_some() {
            return;
        }
        let g = &self.game;
        let summary = quips::RunSummary {
            won: g.phase == game::Phase::Won,
            score: g.score,
            sector: g.sector,
            clears: g.clears,
            lives_lost: g.lives_lost,
            saves: g.saves,
            satellites: g.satellites,
        };
        let previous_best = self.presentation.best;
        let new_best = g.score > 0 && previous_best.is_none_or(|best| g.score > best);
        if new_best {
            self.presentation.best = Some(g.score);
        }
        self.presentation.runs = self.presentation.runs.saturating_add(1);
        self.presentation.results = Some(presentation::Results {
            summary,
            rank: quips::rank(summary),
            previous_best,
            new_best,
        });
        self.unrecorded = Some(FinishedRun {
            won: summary.won,
            score: g.score,
            seconds: g.time,
            new_best,
        });
    }

    /// Supply the stored personal best so the results card can compare.
    /// Storage stays with the host; the simulation never reads it.
    ///
    /// ```
    /// let mut orbit = owned_orbit::Orbit::new();
    /// orbit.set_personal_best(Some(9000));
    /// assert_eq!(orbit.personal_best(), Some(9000));
    /// ```
    pub fn set_personal_best(&mut self, best: Option<u32>) {
        self.presentation.best = best;
    }

    /// The best score known to this session, including a just-finished run.
    pub fn personal_best(&self) -> Option<u32> {
        self.presentation.best
    }

    /// The run that just finished, returned once so a host can record it.
    ///
    /// ```
    /// use owned_orbit::{Orbit, game::Phase};
    /// let mut orbit = Orbit::new();
    /// orbit.game.phase = Phase::Lost;
    /// orbit.advance(0.0, 0.0, 0);
    /// let run = orbit.take_finished_run().unwrap();
    /// assert!(!run.won && run.score == 0 && !run.new_best);
    /// assert_eq!(orbit.take_finished_run(), None);
    /// ```
    pub fn take_finished_run(&mut self) -> Option<FinishedRun> {
        self.unrecorded.take()
    }

    /// The host lifecycle; `Playing` also permits pre-serve charging and sector transitions.
    ///
    /// ```
    /// use ferravex_core::session::SessionState;
    /// assert_eq!(owned_orbit::Orbit::new().session_state(), SessionState::Playing);
    /// ```
    pub fn session_state(&self) -> SessionState {
        self.session.state()
    }

    /// Pause/resume without accumulating inactive time or retaining queued launch presses.
    /// Terminal sessions cannot be paused. Returns whether the lifecycle changed.
    ///
    /// ```
    /// let mut orbit = owned_orbit::Orbit::new();
    /// assert!(orbit.set_paused(true));
    /// orbit.advance(20.0, 1.0, 1);
    /// assert_eq!(orbit.game.time, 0.0);
    /// assert!(orbit.set_paused(false));
    /// ```
    pub fn set_paused(&mut self, paused: bool) -> bool {
        let changed = self.session.set_paused(paused);
        if changed {
            if paused {
                self.presentation.pauses = self.presentation.pauses.wrapping_add(1);
            }
            self.pending_launch = false;
            self.actions.suppress_held();
        }
        changed
    }

    /// Reset the run, clock and buffered controls, then allow steering and charging.
    /// A restart never launches the ball and also resumes a paused session.
    ///
    /// ```
    /// let mut orbit = owned_orbit::Orbit::new();
    /// orbit.game.score = 500;
    /// orbit.set_paused(true);
    /// orbit.restart();
    /// assert_eq!(orbit.game.score, 0);
    /// assert_eq!(orbit.game.phase, owned_orbit::game::Phase::Ready);
    /// ```
    pub fn restart(&mut self) {
        self.game.restart();
        self.presentation.clear();
        self.presentation.restarts = self.presentation.restarts.wrapping_add(1);
        self.unrecorded = None;
        self.sync_presentation();
        self.session.restart();
        self.pending_launch = false;
        self.actions.suppress_held();
    }
    pub fn render(&mut self) {
        self.sync_presentation();
        self.canvas
            .render_session(&self.game, self.session.is_paused(), &self.presentation);
    }

    /// Refresh bounded UTF-8 status from actual gameplay and the current hints.
    /// This human-readable v1 presentation is not a save/replay format. It does
    /// not advance gameplay, sample input or consume feedback. At most 2048
    /// UTF-8 bytes are returned; overlong copy produces a complete error hint.
    /// Native hosts use the authored default hints; the interactive adapter
    /// refreshes them from its existing `PlayerInput` before each host frame.
    ///
    /// ```
    /// let mut orbit = owned_orbit::Orbit::new();
    /// assert!(orbit.status_text_v1().contains("Panels: 35 remaining"));
    /// assert!(orbit.status_text_v1().contains("Best precision: 0. Ball saves: 0."));
    /// ```
    pub fn status_text_v1(&mut self) -> &str {
        self.readable_status
            .refresh(&self.game, self.session.is_paused(), &self.presentation);
        self.readable_status.as_str()
    }

    fn sync_presentation(&mut self) {
        ferravex_engine::render::preferences::sync_feedback(&mut self.presentation.feedback);
        let policy = ferravex_engine::render::preferences::feedback_policy();
        self.game.apply_policy(policy);
        self.presentation.juice.apply_policy(policy);
    }
}
static APP: Mutex<Option<Orbit>> = Mutex::new(None);
#[unsafe(no_mangle)]
pub extern "C" fn init() {
    *APP.lock().expect("app lock") = Some(Orbit::new());
}
/// buttons: bit0 launch/retry, bit1 reset. Input edges survive frames with no simulation tick.
#[unsafe(no_mangle)]
pub extern "C" fn step(dt: f64, axis: f32, buttons: u32) {
    if let Some(app) = APP.lock().expect("app lock").as_mut() {
        app.advance(dt, axis, buttons);
        app.render();
    }
}
#[unsafe(no_mangle)]
pub extern "C" fn pixels() -> *const u8 {
    APP.lock()
        .expect("app lock")
        .as_ref()
        .map_or(std::ptr::null(), |app| {
            app.canvas.frame.pixels().as_ptr().cast()
        })
}
#[unsafe(no_mangle)]
pub extern "C" fn width() -> u32 {
    WIDTH
}
#[unsafe(no_mangle)]
pub extern "C" fn height() -> u32 {
    HEIGHT
}
#[unsafe(no_mangle)]
pub extern "C" fn status() -> u32 {
    APP.lock()
        .expect("app lock")
        .as_ref()
        .map_or(0, |app| app.game.phase as u32)
}
#[unsafe(no_mangle)]
pub extern "C" fn score() -> u32 {
    APP.lock()
        .expect("app lock")
        .as_ref()
        .map_or(0, |app| app.game.score)
}
#[unsafe(no_mangle)]
pub extern "C" fn lives() -> u32 {
    APP.lock()
        .expect("app lock")
        .as_ref()
        .map_or(3, |app| app.game.lives)
}
#[unsafe(no_mangle)]
pub extern "C" fn sector() -> u32 {
    APP.lock()
        .expect("app lock")
        .as_ref()
        .map_or(1, |app| app.game.sector)
}
#[unsafe(no_mangle)]
pub extern "C" fn paddle() -> f32 {
    APP.lock()
        .expect("app lock")
        .as_ref()
        .map_or(320.0, |app| app.game.paddle_x)
}

/// Pointer to a refreshed human-readable v1 UTF-8 status, or null before init.
/// Read `orbit_status_text_v1_len` immediately afterward, then copy the bytes
/// before any other mutating game call. No terminator is included. Memory stays
/// module-owned; the maximum length is 2048. Legacy exports are unchanged.
///
/// ```
/// owned_orbit::init();
/// let ptr = owned_orbit::orbit_status_text_v1();
/// let len = owned_orbit::orbit_status_text_v1_len() as usize;
/// assert!(!ptr.is_null() && len <= 2048);
/// // Copy immediately, before another mutating game call invalidates the view.
/// let copy = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
/// assert!(std::str::from_utf8(&copy).unwrap().contains("Ready."));
/// ```
#[unsafe(no_mangle)]
pub extern "C" fn orbit_status_text_v1() -> *const u8 {
    APP.lock()
        .expect("app lock")
        .as_mut()
        .map_or(std::ptr::null(), |app| app.status_text_v1().as_ptr())
}

/// Byte length of the last `orbit_status_text_v1` snapshot, zero before init.
#[unsafe(no_mangle)]
pub extern "C" fn orbit_status_text_v1_len() -> u32 {
    APP.lock()
        .expect("app lock")
        .as_ref()
        .map_or(0, |app| app.readable_status.as_str().len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_engine::{
        play::{audio::Cue, feedback::FeedbackPolicy},
        render::preferences,
    };
    const DT: f64 = 1.0 / 120.0;

    struct RestorePolicy(FeedbackPolicy);
    impl RestorePolicy {
        fn new() -> Self {
            let previous = preferences::feedback_policy();
            preferences::set_feedback_policy(FeedbackPolicy::default());
            Self(previous)
        }
    }
    impl Drop for RestorePolicy {
        fn drop(&mut self) {
            preferences::set_feedback_policy(self.0);
        }
    }

    #[test]
    fn live_policy_clears_existing_effects_on_paused_zero_tick_and_render_only_frames() {
        let _restore = RestorePolicy::new();
        for (motion, flash) in [(true, false), (false, true), (true, true)] {
            preferences::set_feedback_policy(FeedbackPolicy::default());
            let mut app = Orbit::new();
            app.game.phase = game::Phase::Playing;
            app.game.blocks[28].health = 2;
            app.game.ball = [80., 380.];
            app.game.velocity = [0., -12000.];
            app.advance_input(2. * DT, Input::default());
            assert!(!app.game.particles.is_empty());
            assert!(!app.game.trail.is_empty());
            assert!(app.game.blocks.iter().any(|b| b.flash > 0.));
            assert!(app.presentation.feedback.impact().emphasis > 0.);
            let state = (app.game.time, app.game.ball, app.game.score, app.game.lives);
            app.set_paused(true);
            preferences::set_feedback_policy(FeedbackPolicy::new(motion, flash, 1.).unwrap());
            app.advance_input(0., Input::default());
            app.render();
            assert_eq!(app.game.particles.is_empty(), motion);
            assert_eq!(app.game.trail.is_empty(), motion);
            assert_eq!(app.presentation.feedback.impact().emphasis == 0., motion);
            assert_eq!(app.game.blocks.iter().all(|b| b.flash == 0.), flash);
            preferences::set_feedback_policy(FeedbackPolicy::default());
            app.render();
            assert_eq!(app.game.particles.is_empty(), motion);
            assert_eq!(app.game.blocks.iter().all(|b| b.flash == 0.), flash);
            assert_eq!(
                (app.game.time, app.game.ball, app.game.score, app.game.lives),
                state
            );
            preferences::set_feedback_policy(FeedbackPolicy::new(true, true, 1.).unwrap());
            app.render(); // no advance call: render itself must consume live policy
            assert_eq!(app.game.particles.len(), 0);
            assert!(app.game.blocks.iter().all(|b| b.flash == 0.));
        }
    }

    #[test]
    fn muted_visuals_preserve_accepted_pickup_save_audio_and_reset_drops_pending_cues() {
        let _restore = RestorePolicy::new();
        for reduced in [false, true] {
            preferences::set_feedback_policy(FeedbackPolicy::new(reduced, reduced, 1.).unwrap());
            let mut app = Orbit::new();
            app.advance_input(
                0.,
                Input {
                    launch: true,
                    ..Default::default()
                },
            );
            assert_eq!(app.presentation.take_cues().count(), 0);
            app.advance_input(DT, Input::default());
            assert_eq!(
                app.presentation.take_cues().collect::<Vec<_>>(),
                [(Cue::Jump, 0.45)]
            );
            app.game.ball = [100., 676.];
            app.game.velocity = [0., 325.];
            app.game.drops.push(game::Drop {
                position: [app.game.paddle_x, game::PADDLE_Y - 2.],
                kind: game::Pickup::Wide,
            });
            app.advance_input(6. * DT, Input::default());
            assert_eq!((app.game.saves, app.game.lives), (1, 3));
            assert_eq!(app.presentation.notice, Some(("WIDE PADDLE", 50)));
            assert_eq!(
                app.presentation.take_cues().collect::<Vec<_>>(),
                [(Cue::Pickup, 0.55)]
            );
            assert_eq!(app.game.particles.is_empty(), reduced);
            app.advance_input(0., Input::default());
            assert_eq!(app.presentation.take_cues().count(), 0);
            app.game.ball = [100., 676.];
            app.game.velocity = [0., 325.];
            app.advance_input(DT, Input::default());
            assert_eq!(app.game.lives, 2);
            app.restart();
            assert_eq!(app.presentation.take_cues().count(), 0);
            assert_eq!(app.presentation.notice, None);
            assert_eq!(app.presentation.feedback.impact().emphasis, 0.);
            assert_eq!(app.game.policy.reduced_motion(), reduced);
        }
    }

    #[test]
    fn ready_session_keeps_steering_and_charging_at_each_host_frame_rate() {
        for fps in [30, 60, 144] {
            let mut app = Orbit::new();
            for _ in 0..fps * 2 {
                app.advance(1.0 / f64::from(fps), 1., 0);
            }
            assert_eq!(app.session.tick(), 240);
            assert_eq!(app.session_state(), SessionState::Playing);
            assert_eq!(app.game.phase, game::Phase::Ready);
            assert_eq!(app.game.serve_charge.power(), 1.);
            assert!(app.game.paddle_x > 500.);
        }
    }

    #[test]
    fn sampled_edge_survives_sub_tick_frames_and_launches_once_during_catchup() {
        let mut app = Orbit::new();
        app.advance_input(
            DT / 4.,
            Input {
                launch: true,
                ..Input::default()
            },
        );
        app.advance_input(DT / 4., Input::default());
        assert_eq!(app.game.phase, game::Phase::Ready);
        app.advance_input(DT * 5.5, Input::default());
        assert_eq!(app.game.phase, game::Phase::Playing);
        assert!((app.game.ball_save - (2.5 - 5. / 120.)).abs() < 0.0001);
        assert_eq!(app.session.tick(), 6);
    }

    #[test]
    fn catchup_does_not_reuse_launch_after_a_lost_life() {
        let mut app = Orbit::new();
        app.game.phase = game::Phase::Playing;
        app.game.ball = [100., 676.];
        app.game.velocity = [0., 325.];
        // This press happens during play and is consumed before the drain.
        app.advance(6. * DT, 0., 1);
        assert_eq!(app.game.lives, 2);
        assert_eq!(app.game.phase, game::Phase::Ready);
        assert_eq!(app.game.feedback(), Some(game::Feedback::LifeLost));
        app.advance(6. * DT, 0., 1);
        assert_eq!(app.game.phase, game::Phase::Ready);
        app.advance(0., 0., 0);
        app.advance(DT, 0., 1);
        assert_eq!(app.game.phase, game::Phase::Playing);
    }

    #[test]
    fn pause_discards_launch_edges_and_never_advances_gameplay_time() {
        let mut app = Orbit::new();
        app.advance(DT / 2., 0., 1);
        assert!(app.set_paused(true));
        let ball = app.game.ball;
        for _ in 0..4 {
            app.advance(10., 1., 1);
        }
        assert_eq!((app.game.ball, app.game.time), (ball, 0.));
        assert!(app.set_paused(false));
        app.advance(DT / 2., 0., 1);
        assert_eq!(app.session.tick(), 1);
        assert_eq!(app.game.phase, game::Phase::Ready);
        app.advance(0., 0., 0);
        app.advance(DT, 0., 1);
        assert_eq!(app.game.phase, game::Phase::Playing);
    }

    #[test]
    fn quick_restart_resumes_and_clears_fractional_time_and_pending_launch() {
        let mut app = Orbit::new();
        app.game.score = 1000;
        app.advance(DT * 0.75, 0., 1);
        app.set_paused(true);
        app.advance(0., 0., 3);
        app.advance(0., 0., 0);
        assert_eq!(app.session_state(), SessionState::Playing);
        assert_eq!((app.game.score, app.game.lives, app.game.sector), (0, 3, 1));
        assert_eq!(app.game.feedback(), Some(game::Feedback::Restarted));
        app.advance(DT / 2., 0., 0);
        assert_eq!(app.game.time, 0.);
        app.advance(DT / 2., 0., 0);
        assert_eq!(app.session.tick(), 1);
        assert_eq!(app.game.phase, game::Phase::Ready);
    }

    #[test]
    fn retry_and_held_restart_do_not_launch_or_repeatedly_reset_during_catchup() {
        for button in [1, 2] {
            let mut app = Orbit::new();
            app.game.phase = game::Phase::Lost;
            app.game.lives = 0;
            app.advance(1., 0., button);
            assert_eq!(app.game.phase, game::Phase::Ready);
            assert_eq!(app.game.time, 0.);
            app.advance(6. * DT, 0., button);
            assert_eq!(app.session.tick(), 6);
            assert_eq!(app.game.phase, game::Phase::Ready);
            assert!(app.game.serve_charge.power() > 0.);
            app.advance(0., 0., 0);
            app.advance(DT, 0., 1);
            assert_eq!(app.game.phase, game::Phase::Playing);
        }
    }

    #[test]
    fn terminal_result_stops_remaining_catchup_and_retry_starts_a_fresh_clock() {
        for won in [false, true] {
            let mut app = Orbit::new();
            app.game.phase = game::Phase::Playing;
            if won {
                app.game.sector = 2;
                for block in &mut app.game.blocks {
                    block.health = 0;
                }
            } else {
                app.game.lives = 1;
                app.game.ball = [100., 676.];
                app.game.velocity = [0., 325.];
            }
            app.advance(12. * DT, 0., 0);
            assert_eq!(
                app.session_state(),
                if won {
                    SessionState::Won
                } else {
                    SessionState::Lost
                }
            );
            assert_eq!(app.game.time, DT as f32);
            let result = (app.game.time, app.game.ball, app.game.score, app.game.lives);
            app.advance(30., 1., 0);
            assert_eq!(
                (app.game.time, app.game.ball, app.game.score, app.game.lives),
                result
            );
            assert!(!app.set_paused(true));
            app.advance(0., 0., 1);
            app.advance(0., 0., 0);
            assert_eq!(app.session.tick(), 0);
            assert_eq!(app.game.phase, game::Phase::Ready);
            assert_eq!(app.game.remaining(), 35);
        }
    }

    #[test]
    fn sector_countdown_freezes_on_pause_and_does_not_buffer_a_transition_launch() {
        let mut app = Orbit::new();
        app.game.phase = game::Phase::Playing;
        for block in &mut app.game.blocks {
            block.health = 0;
        }
        app.advance(DT, 0., 0);
        assert_eq!(app.game.phase, game::Phase::Transition);
        let remaining = app.game.transition_remaining();
        let time = app.game.time;
        app.set_paused(true);
        app.advance(20., 1., 0);
        assert_eq!(
            (app.game.transition_remaining(), app.game.time),
            (remaining, time)
        );
        app.set_paused(false);
        for _ in 0..16 {
            app.advance(12. * DT, 0., 1);
        }
        assert_eq!((app.game.sector, app.game.phase), (2, game::Phase::Ready));
        assert!(app.game.serve_charge.power() > 0.);
        app.advance(0., 0., 0);
        app.advance(DT, 0., 1);
        assert_eq!(app.game.phase, game::Phase::Playing);
    }

    #[test]
    fn session_autoplay_still_wins_both_sectors() {
        let mut app = Orbit::new();
        for _ in 0..120 * 600 {
            let input = app.game.autoplay();
            app.advance_input(DT, input);
            if matches!(app.session_state(), SessionState::Won | SessionState::Lost) {
                break;
            }
        }
        assert_eq!(app.session_state(), SessionState::Won);
        assert_eq!(app.game.clears, 70);
        assert!(app.game.ccd_impacts > 70);
    }

    #[test]
    fn quick_input_tap_survives_zero_tick_and_frame_is_rgba() {
        let mut app = Orbit::new();
        app.advance(0.0, 0.0, 1);
        app.advance(0.0, 0.0, 0);
        app.advance(1.0 / 120.0, 0.0, 0);
        assert_eq!(app.game.phase, game::Phase::Playing);
        app.render();
        assert_eq!(app.canvas.frame.pixels().len(), (WIDTH * HEIGHT) as usize);
        assert!(app.canvas.frame.pixels().iter().all(|p| p[3] == 255));
        assert!(
            app.canvas
                .frame
                .pixels()
                .iter()
                .filter(|p| p[0] > 100)
                .count()
                > 5000
        );
    }

    #[test]
    fn results_compare_with_the_supplied_best_once_and_restart_keeps_it() {
        for (best, new_best) in [(None, true), (Some(100), true), (Some(99_999), false)] {
            let mut app = Orbit::new();
            app.set_personal_best(best);
            app.game.phase = game::Phase::Playing;
            app.game.score = 4321;
            app.game.lives = 1;
            app.game.ball = [100., 676.];
            app.game.velocity = [0., 325.];
            app.advance(DT, 0., 0);
            assert_eq!(app.session_state(), SessionState::Lost);
            let results = app.presentation.results.expect("results card");
            assert_eq!((results.previous_best, results.new_best), (best, new_best));
            assert_eq!(results.rank.title, "SPACE TOURIST");
            let run = app.take_finished_run().expect("one record");
            assert_eq!((run.won, run.score, run.new_best), (false, 4321, new_best));
            app.advance(1., 0., 0);
            assert_eq!(
                app.take_finished_run(),
                None,
                "terminal frames never re-record"
            );
            let expected = if new_best { Some(4321) } else { best };
            assert_eq!(app.personal_best(), expected);
            app.restart();
            assert!(app.presentation.results.is_none());
            assert_eq!(app.personal_best(), expected);
            assert_eq!(app.take_finished_run(), None);
        }
    }

    #[test]
    fn juice_freezes_with_pause_and_presentation_detail_never_adds_cues() {
        let _restore = RestorePolicy::new();
        let mut app = Orbit::new();
        app.game.phase = game::Phase::Playing;
        app.game.ball = [80., 380.];
        app.game.velocity = [0., -12000.];
        app.advance_input(DT, Input::default());
        assert!(app.presentation.juice.pops.iter().any(Option::is_some));
        assert!(app.presentation.juice.chime.is_some());
        let cues: Vec<_> = app.presentation.take_cues().collect();
        assert!(cues.len() <= 2, "{cues:?}");
        let time = app.presentation.juice.time;
        app.set_paused(true);
        app.advance_input(5., Input::default());
        app.render();
        assert_eq!(app.presentation.juice.time, time);
        assert!(app.presentation.juice.pops.iter().any(Option::is_some));
        app.set_paused(false);
        for _ in 0..240 {
            app.advance_input(DT, Input::default());
        }
        assert!(app.presentation.juice.pops.iter().all(Option::is_none));
    }

    #[test]
    fn steering_hint_retires_after_real_steering_and_survives_restart() {
        let mut app = Orbit::new();
        assert!(!app.presentation.juice.steering_learned());
        for _ in 0..120 {
            app.advance_input(
                DT,
                Input {
                    axis: 1.,
                    ..Input::default()
                },
            );
        }
        assert!(app.presentation.juice.steering_learned());
        app.restart();
        assert!(app.presentation.juice.steering_learned());
    }

    #[test]
    fn every_screen_renders_opaque_and_reduced_motion_ignores_camera_kick() {
        let _restore = RestorePolicy::new();
        let mut app = Orbit::new();
        app.set_personal_best(Some(5000));
        app.game.phase = game::Phase::Playing;
        app.game.extra_balls.push(game::Ball {
            position: [200., 400.],
            velocity: [0., -300.],
        });
        app.game.sticky = 2;
        app.game.time = 15.;
        app.render();
        app.presentation
            .juice
            .accept(presentation::Spark::Drain { at: [100., 653.] });
        let kicked = app.presentation.juice.shake();
        assert_ne!(kicked, [0, 0]);
        preferences::set_feedback_policy(FeedbackPolicy::new(true, true, 1.).unwrap());
        app.render();
        assert_eq!(app.presentation.juice.shake(), [0, 0]);
        for phase in [game::Phase::Transition, game::Phase::Won, game::Phase::Lost] {
            app.game.phase = phase;
            app.advance(0., 0., 0);
            app.render();
            assert!(app.canvas.frame.pixels().iter().all(|p| p[3] == 255));
        }
        assert!(app.presentation.results.is_some());
        app.set_paused(false);
        app.restart();
        app.set_paused(true);
        app.render();
        assert!(app.canvas.frame.pixels().iter().all(|p| p[3] == 255));
    }
}
