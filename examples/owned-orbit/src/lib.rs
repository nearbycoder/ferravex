//! Orbit Breaker: original gameplay, original CPU raster, and only first-party Ferravex crates.
pub mod game;
#[cfg(any(target_arch = "wasm32", test))]
mod interactive;
#[cfg(test)]
mod journey200_tests;
mod presentation;
mod raster;
mod readable;
use ferravex_core::session::{GameSession, SessionState};
use ferravex_engine::input::{ActionKind, ActionMap, Binding, ContextId, InputCode};
use game::{Game, HEIGHT, Input, WIDTH};
use raster::Canvas;
use std::sync::Mutex;

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
            self.pending_launch = false;
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
            if matches!(self.game.phase, game::Phase::Won | game::Phase::Lost) {
                self.session.finish(self.game.phase == game::Phase::Won);
                break;
            }
        }
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
        self.readable_status.refresh(
            &self.game,
            self.session.is_paused(),
            &self.presentation.hints,
            self.presentation.notice,
        );
        self.readable_status.as_str()
    }

    fn sync_presentation(&mut self) {
        ferravex_engine::render::preferences::sync_feedback(&mut self.presentation.feedback);
        self.game
            .apply_policy(ferravex_engine::render::preferences::feedback_policy());
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
}
