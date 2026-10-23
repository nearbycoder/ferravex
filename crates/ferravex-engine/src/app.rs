//! A renderer-independent, host-driven application lifecycle.
//!
//! [`App`](crate::app::App) owns a [`Game`](crate::app::Game) and a [`GameSession`](crate::session::GameSession). The host injects elapsed seconds
//! into [`App::frame`](crate::app::App::frame); no window, executor, renderer, or wall clock is required.
//! Existing async renderer games can continue using `ferravex_render::game!`.
//! Hosts needing timing and execution details can use [`App::frame_report`](crate::app::App::frame_report)
//! instead, or inspect [`App::last_frame_report`](crate::app::App::last_frame_report) after either method, even on error.
//!
//! ```
//! use ferravex_core::session::GameSession;
//! use ferravex_engine::app::{App, AppControl, Game};
//! use std::convert::Infallible;
//!
//! #[derive(Default)]
//! struct Counter(u32);
//! impl Game for Counter {
//!     type Error = Infallible;
//!     fn fixed_update(&mut self, _dt: f32) -> Result<AppControl, Self::Error> {
//!         self.0 += 1;
//!         Ok(AppControl::Continue)
//!     }
//! }
//! let mut app = App::new(Counter::default(), GameSession::new(64.0, 8, true));
//! let report = app.frame_report(2.5 / 64.0)?;
//! assert_eq!(report.executed_updates, 2);
//! assert_eq!(report.alpha, 0.5);
//! assert_eq!(app.game().0, 2);
//! app.session_mut().set_paused(true);
//! app.frame(10.0)?; // still renders, with no simulation catch-up
//! assert_eq!(app.game().0, 2);
//! app.stop()?; // shutdown once; later frame/stop calls do not invoke hooks
//! # Ok::<(), ferravex_engine::app::AppError<Infallible>>(())
//! ```
//!
//! # Hook and error guarantees
//!
//! - Construction invokes no hooks. The first [`App::startup`](crate::app::App::startup) or [`App::frame`](crate::app::App::frame)
//!   attempts startup exactly once, even if the session is Ready or paused.
//! - Each running frame performs the session's bounded fixed updates, followed
//!   by one render, including frames with zero updates. Ready, paused, and terminal
//!   sessions keep rendering. Starting the application does **not** start a Ready
//!   session: the host controls that separately with [`App::session_mut`](crate::app::App::session_mut).
//! - A hook returning [`AppControl::Stop`](crate::app::AppControl::Stop) skips remaining updates/rendering and
//!   attempts shutdown immediately. [`App::stop`](crate::app::App::stop) does the same. Once startup was
//!   attempted, shutdown is attempted exactly once, including after startup fails.
//!   Stopping/dropping an application that never started invokes neither hook.
//! - A hook error stops the application, attempts shutdown, and returns an
//!   [`AppError`](crate::app::AppError) retaining both errors if cleanup also fails. Nothing is retried;
//!   later calls return Stop (or `Ok(())` for `stop`). Mutations are not rolled back.
//! - Dropping a running application attempts shutdown as a fallback and discards
//!   its error. Call `stop()` explicitly to observe cleanup errors. Panics are not
//!   caught; hooks should report recoverable failures through `Result`.
//!
//! The session clock schedules its whole frame before update hooks run. Its tick
//! therefore includes scheduled updates skipped by an early stop/error. The
//! [`AppFrameReport`](crate::app::AppFrameReport) distinguishes scheduled updates from executed hooks. A stopped
//! app cannot resume; create a new app for another lifecycle. Session restart/reset
//! between frames affects simulation timing only and never repeats startup.

/// Scheduled update context passed to [`Game::fixed_update_context`].
pub use ferravex_core::TickContext;
use ferravex_core::session::GameSession;
use std::fmt;

/// A hook's decision about whether the application should continue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppControl {
    /// Continue to the next hook or host frame.
    Continue,
    /// Stop now, skip subsequent frame hooks, and attempt shutdown.
    Stop,
}

/// Application lifecycle state, independent of the simulation's session state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppState {
    /// Startup has not been attempted.
    Ready,
    /// Startup was attempted and the app has not stopped.
    Running,
    /// No more hooks will run, including after a shutdown failure.
    Stopped,
}

/// Execution and timing details for one completed host frame call.
///
/// Returned by [`App::frame_report`] and retained by [`App::last_frame_report`],
/// including when a frame returns an error. Counts describe hooks that actually
/// ran, not successful simulation commits: a failing hook may mutate game state.
/// See [`App::frame_report`] for an executable example.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AppFrameReport {
    /// Number of fixed updates budgeted by the session for this frame.
    ///
    /// The session tick includes this whole budget, even when stop/error skips
    /// some hooks. Zero if startup stops/fails or the app was already stopped.
    pub scheduled_updates: u32,
    /// Number of fixed-update hooks that ran, including one returning Stop or Err.
    ///
    /// Never greater than `scheduled_updates`; an error does not roll back work.
    pub executed_updates: u32,
    /// The session's fixed timestep in seconds, even when no updates run.
    pub dt: f32,
    /// The session's interpolation factor in `0.0..=1.0` for this frame.
    ///
    /// This is passed to render if it runs. It remains available when an update
    /// stops/fails before rendering, and is zero if the session is inactive or
    /// never advanced because startup stopped/failed or the app was stopped.
    pub alpha: f32,
    /// Whole-step time discarded by the clock's catch-up budget, in seconds.
    ///
    /// This is the session's `FrameStep::dropped_seconds` unchanged. It excludes
    /// scheduled updates skipped by stop/error, inactive-session time, invalid
    /// elapsed time and time above the core clock's elapsed-time cap.
    pub dropped_seconds: f64,
    /// Whether the render hook ran, including one returning Stop or Err.
    ///
    /// This records invocation, not successful presentation by a host renderer.
    pub rendered: bool,
    /// Whether the app can accept another frame after this call.
    ///
    /// Stop also covers errors and calls on an already stopped app. This is an
    /// application decision, independent of the session's pause or terminal state.
    pub control: AppControl,
}

/// The hook that produced an [`AppError`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppHook {
    Startup,
    FixedUpdate,
    Render,
    Shutdown,
}

impl fmt::Display for AppHook {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Startup => "startup",
            Self::FixedUpdate => "fixed update",
            Self::Render => "render",
            Self::Shutdown => "shutdown",
        })
    }
}

/// A hook failure and, when applicable, a separate cleanup failure.
///
/// The primary error is never replaced by a shutdown error. A failure from an
/// explicit stop has `hook == AppHook::Shutdown` and no `shutdown_error`.
#[derive(Debug, PartialEq, Eq)]
pub struct AppError<E> {
    /// Hook that failed first.
    pub hook: AppHook,
    /// Original error returned by that hook.
    pub error: E,
    /// Additional error from automatic shutdown after the primary failure.
    pub shutdown_error: Option<E>,
}

impl<E: fmt::Display> fmt::Display for AppError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} hook failed: {}", self.hook, self.error)?;
        if let Some(error) = &self.shutdown_error {
            write!(f, "; shutdown also failed: {error}")?;
        }
        Ok(())
    }
}

impl<E: std::error::Error + 'static> std::error::Error for AppError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Game hooks called synchronously by [`App`]. Only fixed update is required.
///
/// Keep current and previous simulation poses in the game, shift the current
/// pose to previous before each update, and interpolate them in `render(alpha)`.
/// The host may sample input through [`App::game_mut`] before each frame and
/// control start/pause/retry through [`App::session_mut`]. See the module example
/// for a headless implementation and the `app_lifecycle` example for interpolation.
pub trait Game {
    /// Application-defined error; `Infallible` is useful for headless simulations.
    type Error;

    /// Initialize once. Shutdown will run even if initialization returns an error,
    /// so cleanup must handle partially initialized resources.
    fn startup(&mut self) -> Result<AppControl, Self::Error> {
        Ok(AppControl::Continue)
    }

    /// Advance simulation by the session's constant timestep in seconds.
    ///
    /// The default [`Self::fixed_update_context`] calls this once per executed
    /// tick. Existing implementations need no changes.
    fn fixed_update(&mut self, dt: f32) -> Result<AppControl, Self::Error>;

    /// Advance one scheduled tick with its absolute ID and index within the batch.
    ///
    /// [`App`] invokes this hook instead of directly invoking `fixed_update`.
    /// The default forwards to [`Self::fixed_update`] with `context.dt`. An
    /// override owns the update and may call the old hook itself; the runner
    /// never calls both. Stop/error handling and [`AppHook::FixedUpdate`] are
    /// unchanged. Reports count invocations, including Stop/Err, not commits.
    ///
    /// IDs begin at 1, wrap through 0, and are reused after session reset/restart.
    /// They count issued work, including manual or skipped updates, and are not
    /// an executed-update total. See [`TickContext`] and [`ferravex_core::TickBatch`].
    ///
    /// ```
    /// use ferravex_engine::app::{App, AppControl, Game, TickContext};
    /// use ferravex_engine::session::GameSession;
    /// use std::convert::Infallible;
    /// #[derive(Default)]
    /// struct Counter { position: f32, last_tick: u64 }
    /// impl Game for Counter {
    ///     type Error = Infallible;
    ///     fn fixed_update(&mut self, dt: f32) -> Result<AppControl, Self::Error> {
    ///         self.position += 2.0 * dt;
    ///         Ok(AppControl::Continue)
    ///     }
    ///     fn fixed_update_context(&mut self, tick: TickContext) -> Result<AppControl, Self::Error> {
    ///         self.last_tick = tick.tick;
    ///         self.fixed_update(tick.dt)
    ///     }
    /// }
    /// let mut app = App::new(Counter::default(), GameSession::new(64.0, 4, true));
    /// let report = app.frame_report(2.0 / 64.0)?;
    /// assert_eq!(report.executed_updates, 2);
    /// assert_eq!(app.game().last_tick, 2);
    /// assert_eq!(app.game().position, 4.0 / 64.0);
    /// # Ok::<(), ferravex_engine::app::AppError<Infallible>>(())
    /// ```
    fn fixed_update_context(&mut self, context: TickContext) -> Result<AppControl, Self::Error> {
        self.fixed_update(context.dt)
    }

    /// Present once after updates, with the session's interpolation factor.
    ///
    /// `alpha` is the fractional remainder in `0.0..=1.0`; inactive sessions
    /// supply zero. This hook is also called when no fixed updates are due.
    fn render(&mut self, _alpha: f32) -> Result<AppControl, Self::Error> {
        Ok(AppControl::Continue)
    }

    /// Release application resources once after startup was attempted.
    fn shutdown(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// A single application lifecycle driven by host-supplied elapsed time.
///
/// No per-frame allocation or platform work is performed by the runner itself.
/// Pause and terminal game states belong to its [`GameSession`]; application stop
/// is permanent and also releases resources through [`Game::shutdown`].
pub struct App<G: Game> {
    game: G,
    session: GameSession,
    state: AppState,
    last_frame_report: Option<AppFrameReport>,
}

impl<G: Game> App<G> {
    /// Take ownership of a game and its configured session without invoking hooks.
    pub fn new(game: G, session: GameSession) -> Self {
        Self {
            game,
            session,
            state: AppState::Ready,
            last_frame_report: None,
        }
    }

    /// Inspect application data, including after shutdown.
    pub fn game(&self) -> &G {
        &self.game
    }

    /// Update host input or application data between frames.
    pub fn game_mut(&mut self) -> &mut G {
        &mut self.game
    }

    /// Inspect simulation state and its clock.
    pub fn session(&self) -> &GameSession {
        &self.session
    }

    /// Control start, pause, finish, or restart between frames.
    ///
    /// Let `App::frame` advance the clock; manually advancing it consumes time
    /// without invoking game updates. Session changes never restart a stopped app.
    pub fn session_mut(&mut self) -> &mut GameSession {
        &mut self.session
    }

    /// Inspect application state independently of the session's game state.
    pub fn state(&self) -> AppState {
        self.state
    }

    /// Attempt startup once, without advancing time or rendering.
    ///
    /// Repeated calls return Continue while running and Stop after termination.
    pub fn startup(&mut self) -> Result<AppControl, AppError<G::Error>> {
        match self.state {
            AppState::Running => Ok(AppControl::Continue),
            AppState::Stopped => Ok(AppControl::Stop),
            AppState::Ready => {
                self.state = AppState::Running;
                let result = self.game.startup();
                self.handle(AppHook::Startup, result)
            }
        }
    }

    /// Run startup if needed, bounded fixed updates, and one interpolated render.
    ///
    /// The duration is in seconds and is passed directly to `GameSession::advance`.
    /// Negative/non-finite elapsed time follows the core clock's sanitization
    /// policy. Stop or error short-circuits the frame, including during catch-up.
    /// Use [`Self::frame_report`] for execution counts and timing, or inspect
    /// [`Self::last_frame_report`] after this call, including on error.
    pub fn frame(&mut self, elapsed_seconds: f64) -> Result<AppControl, AppError<G::Error>> {
        self.frame_report(elapsed_seconds)
            .map(|report| report.control)
    }

    /// Run a frame with the same hooks and errors as [`Self::frame`], returning details.
    ///
    /// Reports distinguish scheduled ticks from executed updates if a hook
    /// stops early. On error, the partial report is available through
    /// [`Self::last_frame_report`], with `control == AppControl::Stop`, after
    /// exactly-once cleanup has been attempted. Primary and cleanup errors use
    /// the unchanged [`AppError`] contract. No hooks or elapsed time are retried.
    ///
    /// ```
    /// use ferravex_engine::app::{App, AppControl, Game};
    /// use ferravex_core::session::GameSession;
    /// # use std::convert::Infallible;
    /// # struct Demo;
    /// # impl Game for Demo {
    /// #     type Error = Infallible;
    /// #     fn fixed_update(&mut self, _: f32) -> Result<AppControl, Infallible> {
    /// #         Ok(AppControl::Continue)
    /// #     }
    /// # }
    /// let mut app = App::new(Demo, GameSession::new(64.0, 4, true));
    /// let report = app.frame_report(6.5 / 64.0)?;
    /// assert_eq!(report.scheduled_updates, 4);
    /// assert_eq!(report.executed_updates, 4);
    /// assert_eq!(report.dt, 1.0 / 64.0);
    /// assert_eq!(report.alpha, 0.5);
    /// assert_eq!(report.dropped_seconds, 2.0 / 64.0);
    /// assert!(report.rendered);
    /// assert_eq!(report.control, AppControl::Continue);
    /// app.stop()?;
    /// assert_eq!(app.frame_report(1.0)?.control, AppControl::Stop);
    /// # Ok::<(), ferravex_engine::app::AppError<Infallible>>(())
    /// ```
    pub fn frame_report(
        &mut self,
        elapsed_seconds: f64,
    ) -> Result<AppFrameReport, AppError<G::Error>> {
        let mut report = AppFrameReport {
            scheduled_updates: 0,
            executed_updates: 0,
            dt: self.session.dt(),
            alpha: 0.0,
            dropped_seconds: 0.0,
            rendered: false,
            control: AppControl::Stop,
        };
        let result = self.run_frame(elapsed_seconds, &mut report);
        report.control = result.as_ref().copied().unwrap_or(AppControl::Stop);
        self.last_frame_report = Some(report);
        result.map(|_| report)
    }

    /// Snapshot of the most recent completed [`Self::frame`] or [`Self::frame_report`].
    ///
    /// Returns `None` before the first frame call. Both successful and failed
    /// frames replace this snapshot; a hook panic does not. Direct calls to
    /// startup/stop and session changes leave it unchanged, so its control is
    /// historical; use [`Self::state`] to inspect the current application state.
    ///
    /// ```
    /// use ferravex_engine::app::{App, AppControl, AppHook, Game};
    /// use ferravex_core::session::GameSession;
    /// struct FailingGame;
    /// impl Game for FailingGame {
    ///     type Error = &'static str;
    ///     fn fixed_update(&mut self, _: f32) -> Result<AppControl, Self::Error> {
    ///         Err("update failed")
    ///     }
    /// }
    /// let mut app = App::new(FailingGame, GameSession::new(64.0, 4, true));
    /// assert_eq!(app.last_frame_report(), None);
    /// let error = app.frame(3.5 / 64.0).unwrap_err();
    /// assert_eq!(error.hook, AppHook::FixedUpdate);
    /// let report = app.last_frame_report().unwrap();
    /// assert_eq!(report.scheduled_updates, 3);
    /// assert_eq!(report.executed_updates, 1); // Includes the failing hook.
    /// assert_eq!(report.alpha, 0.5);
    /// assert!(!report.rendered);
    /// assert_eq!(report.control, AppControl::Stop);
    /// ```
    pub fn last_frame_report(&self) -> Option<AppFrameReport> {
        self.last_frame_report
    }

    fn run_frame(
        &mut self,
        elapsed_seconds: f64,
        report: &mut AppFrameReport,
    ) -> Result<AppControl, AppError<G::Error>> {
        if self.startup()? == AppControl::Stop {
            return Ok(AppControl::Stop);
        }
        let batch = self.session.advance_batch(elapsed_seconds);
        let frame = batch.frame();
        report.scheduled_updates = frame.steps;
        report.dt = frame.dt;
        report.alpha = frame.alpha;
        report.dropped_seconds = frame.dropped_seconds;
        for context in batch.ticks() {
            let result = self.game.fixed_update_context(context);
            report.executed_updates += 1;
            if self.handle(AppHook::FixedUpdate, result)? == AppControl::Stop {
                return Ok(AppControl::Stop);
            }
        }
        let result = self.game.render(frame.alpha);
        report.rendered = true;
        self.handle(AppHook::Render, result)
    }

    /// Permanently stop and attempt shutdown once if startup was attempted.
    ///
    /// Safe to repeat, including after any hook error. The state becomes Stopped
    /// before cleanup, so even a failed shutdown is never retried by `Drop`.
    pub fn stop(&mut self) -> Result<(), AppError<G::Error>> {
        self.shutdown().map_err(|error| AppError {
            hook: AppHook::Shutdown,
            error,
            shutdown_error: None,
        })
    }

    fn shutdown(&mut self) -> Result<(), G::Error> {
        let previous = std::mem::replace(&mut self.state, AppState::Stopped);
        if previous == AppState::Running {
            self.game.shutdown()
        } else {
            Ok(())
        }
    }

    fn handle(
        &mut self,
        hook: AppHook,
        result: Result<AppControl, G::Error>,
    ) -> Result<AppControl, AppError<G::Error>> {
        match result {
            Ok(AppControl::Continue) => Ok(AppControl::Continue),
            Ok(AppControl::Stop) => {
                self.stop()?;
                Ok(AppControl::Stop)
            }
            Err(error) => Err(AppError {
                hook,
                error,
                shutdown_error: self.shutdown().err(),
            }),
        }
    }
}

impl<G: Game> Drop for App<G> {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_core::session::SessionState;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Default)]
    struct ContextProbe {
        ticks: Vec<TickContext>,
        terminate_at: Option<u64>,
        fail: bool,
        fail_shutdown: bool,
        renders: usize,
        startups: usize,
        shutdowns: usize,
    }

    impl Game for ContextProbe {
        type Error = &'static str;

        fn startup(&mut self) -> Result<AppControl, Self::Error> {
            self.startups += 1;
            Ok(AppControl::Continue)
        }

        fn fixed_update(&mut self, _: f32) -> Result<AppControl, Self::Error> {
            panic!("an overridden contextual hook must not also invoke the legacy hook");
        }

        fn fixed_update_context(
            &mut self,
            context: TickContext,
        ) -> Result<AppControl, Self::Error> {
            self.ticks.push(context);
            if self.terminate_at == Some(context.tick) {
                if self.fail {
                    Err("context update")
                } else {
                    Ok(AppControl::Stop)
                }
            } else {
                Ok(AppControl::Continue)
            }
        }

        fn render(&mut self, _: f32) -> Result<AppControl, Self::Error> {
            self.renders += 1;
            Ok(AppControl::Continue)
        }

        fn shutdown(&mut self) -> Result<(), Self::Error> {
            self.shutdowns += 1;
            if self.fail_shutdown {
                Err("context cleanup")
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn context_hooks_receive_individual_ids_with_zero_one_and_catchup_frames() {
        let mut app = App::new(ContextProbe::default(), GameSession::new(64.0, 4, false));
        assert_eq!(app.frame_report(100.0).unwrap().executed_updates, 0);
        app.session_mut().start();
        assert_eq!(app.frame_report(0.5 / 64.0).unwrap().executed_updates, 0);
        assert!(app.game().ticks.is_empty());
        assert_eq!(app.frame_report(0.5 / 64.0).unwrap().executed_updates, 1);
        let report = app.frame_report(6.5 / 64.0).unwrap();
        assert_eq!((report.scheduled_updates, report.executed_updates), (4, 4));
        assert_eq!(report.dropped_seconds, 2.0 / 64.0);
        assert_eq!(report.alpha, 0.5);
        app.session_mut().set_paused(true);
        assert_eq!(app.frame_report(100.0).unwrap().executed_updates, 0);
        app.session_mut().set_paused(false);
        assert_eq!(app.frame_report(0.5 / 64.0).unwrap().executed_updates, 1);
        assert_eq!(
            app.game()
                .ticks
                .iter()
                .map(|c| (c.tick, c.batch_index))
                .collect::<Vec<_>>(),
            [(1, 0), (2, 0), (3, 1), (4, 2), (5, 3), (6, 0)]
        );
        assert!(app.game().ticks.iter().all(|c| c.dt == 1.0 / 64.0));
        assert_eq!(app.game().renders, 6);
        app.session_mut().restart();
        app.frame(1.0 / 64.0).unwrap();
        assert_eq!(app.game().ticks.last().unwrap().tick, 1);
        app.session_mut().reset();
        app.frame(1.0).unwrap();
        app.session_mut().start();
        app.frame(1.0 / 64.0).unwrap();
        assert_eq!(app.game().ticks.last().unwrap().tick, 1);
        assert_eq!(app.game().startups, 1);
        app.stop().unwrap();
        assert_eq!(app.game().shutdowns, 1);
    }

    #[test]
    fn contextual_stops_and_errors_count_only_invoked_hooks_and_preserve_cleanup() {
        for fail in [false, true] {
            for fail_shutdown in [false, true] {
                let game = ContextProbe {
                    terminate_at: Some(3),
                    fail,
                    fail_shutdown,
                    ..ContextProbe::default()
                };
                let mut app = App::new(game, GameSession::new(64.0, 4, true));
                let result = app.frame_report(6.5 / 64.0);
                if fail {
                    assert_eq!(
                        result.unwrap_err(),
                        AppError {
                            hook: AppHook::FixedUpdate,
                            error: "context update",
                            shutdown_error: fail_shutdown.then_some("context cleanup"),
                        }
                    );
                } else if fail_shutdown {
                    assert_eq!(
                        result.unwrap_err(),
                        AppError {
                            hook: AppHook::Shutdown,
                            error: "context cleanup",
                            shutdown_error: None,
                        }
                    );
                } else {
                    assert_eq!(result.unwrap().control, AppControl::Stop);
                }
                let report = app.last_frame_report().unwrap();
                assert_eq!((report.scheduled_updates, report.executed_updates), (4, 3));
                assert_eq!(report.control, AppControl::Stop);
                assert!(!report.rendered);
                assert_eq!(app.session().tick(), 4);
                assert_eq!(
                    app.game().ticks.iter().map(|c| c.tick).collect::<Vec<_>>(),
                    [1, 2, 3]
                );
                assert_eq!(app.game().renders, 0);
                assert_eq!(app.game().shutdowns, 1);
                assert_eq!(app.frame_report(1.0).unwrap().executed_updates, 0);
                app.stop().unwrap();
                assert_eq!(app.game().ticks.len(), 3);
                assert_eq!(app.game().shutdowns, 1);
            }
        }
    }

    #[test]
    fn context_ids_include_manually_scheduled_work_without_counting_it_as_app_execution() {
        let mut session = GameSession::new(64.0, 4, true);
        session.advance_batch(2.5 / 64.0); // Issued but never executed by this app.
        session.set_paused(true);
        let manual = session.step_paused_batch().unwrap();
        assert_eq!(manual.ticks().next().unwrap().tick, 3);
        session.set_paused(false);
        let mut app = App::new(ContextProbe::default(), session);
        let report = app.frame_report(0.5 / 64.0).unwrap();
        assert_eq!((report.scheduled_updates, report.executed_updates), (1, 1));
        assert_eq!(
            app.game().ticks,
            [TickContext {
                tick: 4,
                batch_index: 0,
                dt: 1.0 / 64.0
            }]
        );
    }

    #[derive(Clone, Debug, PartialEq)]
    enum Event {
        Startup,
        Update(f32),
        Render(f32),
        Shutdown,
    }

    struct Probe {
        events: Rc<RefCell<Vec<Event>>>,
        fail: Option<AppHook>,
        stop: Option<AppHook>,
        fail_shutdown: bool,
        updates_before_control: u32,
    }

    impl Probe {
        fn result(&self, hook: AppHook) -> Result<AppControl, &'static str> {
            if self.fail == Some(hook) {
                Err("primary")
            } else if self.stop == Some(hook) {
                Ok(AppControl::Stop)
            } else {
                Ok(AppControl::Continue)
            }
        }
    }

    impl Game for Probe {
        type Error = &'static str;

        fn startup(&mut self) -> Result<AppControl, Self::Error> {
            self.events.borrow_mut().push(Event::Startup);
            self.result(AppHook::Startup)
        }

        fn fixed_update(&mut self, dt: f32) -> Result<AppControl, Self::Error> {
            self.events.borrow_mut().push(Event::Update(dt));
            if self.updates_before_control > 0 {
                self.updates_before_control -= 1;
                Ok(AppControl::Continue)
            } else {
                self.result(AppHook::FixedUpdate)
            }
        }

        fn render(&mut self, alpha: f32) -> Result<AppControl, Self::Error> {
            self.events.borrow_mut().push(Event::Render(alpha));
            self.result(AppHook::Render)
        }

        fn shutdown(&mut self) -> Result<(), Self::Error> {
            self.events.borrow_mut().push(Event::Shutdown);
            if self.fail_shutdown {
                Err("cleanup")
            } else {
                Ok(())
            }
        }
    }

    fn app(autoplay: bool) -> App<Probe> {
        App::new(
            Probe {
                events: Rc::new(RefCell::new(Vec::new())),
                fail: None,
                stop: None,
                fail_shutdown: false,
                updates_before_control: 0,
            },
            GameSession::new(64.0, 4, autoplay),
        )
    }

    #[test]
    fn ordered_hooks_constant_steps_and_fractional_render() {
        let mut app = app(true);
        let events = app.game().events.clone();
        assert!(events.borrow().is_empty());
        assert_eq!(app.state(), AppState::Ready);
        assert_eq!(app.startup().unwrap(), AppControl::Continue);
        app.startup().unwrap();
        assert_eq!(app.state(), AppState::Running);
        app.frame(2.5 / 64.0).unwrap();
        app.frame(0.25 / 64.0).unwrap();
        app.frame(0.25 / 64.0).unwrap();
        app.stop().unwrap();
        app.stop().unwrap();
        assert_eq!(app.frame(1.0).unwrap(), AppControl::Stop);
        assert_eq!(app.startup().unwrap(), AppControl::Stop);
        assert_eq!(app.session().tick(), 3);
        assert_eq!(app.state(), AppState::Stopped);
        drop(app);
        assert_eq!(
            *events.borrow(),
            [
                Event::Startup,
                Event::Update(1.0 / 64.0),
                Event::Update(1.0 / 64.0),
                Event::Render(0.5),
                Event::Render(0.75),
                Event::Update(1.0 / 64.0),
                Event::Render(0.0),
                Event::Shutdown,
            ]
        );
    }

    #[test]
    fn inactive_sessions_render_without_catchup_and_restart_does_not_repeat_startup() {
        let mut app = app(false);
        app.frame(100.0).unwrap();
        assert_eq!(app.session().state(), SessionState::Ready);
        assert_eq!(app.session().tick(), 0);
        assert!(app.session_mut().start());
        app.frame(1.0 / 64.0).unwrap();
        assert!(app.session_mut().set_paused(true));
        app.frame(100.0).unwrap();
        assert_eq!(app.session().tick(), 1);
        assert!(app.session_mut().set_paused(false));
        app.frame(1.0 / 64.0).unwrap();
        assert_eq!(app.session().tick(), 2);
        for won in [true, false] {
            assert!(app.session_mut().finish(won));
            app.frame(100.0).unwrap();
            app.session_mut().restart();
            assert_eq!(app.session().tick(), 0);
            app.frame(1.0 / 64.0).unwrap();
            assert_eq!(app.session().tick(), 1);
        }
        app.session_mut().reset();
        app.frame(100.0).unwrap();
        assert_eq!(app.session().tick(), 0);
        app.stop().unwrap();
        let events = app.game().events.borrow();
        assert_eq!(events.iter().filter(|e| **e == Event::Startup).count(), 1);
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e, Event::Update(_)))
                .count(),
            4
        );
        assert_eq!(
            events.iter().filter(|e| **e == Event::Render(0.0)).count(),
            9
        );
        assert_eq!(events.iter().filter(|e| **e == Event::Shutdown).count(), 1);
    }

    #[test]
    fn stop_before_start_never_initializes_or_cleans_up() {
        let mut app = app(true);
        let events = app.game().events.clone();
        app.stop().unwrap();
        app.session_mut().restart();
        assert_eq!(app.startup().unwrap(), AppControl::Stop);
        assert_eq!(app.frame(1.0).unwrap(), AppControl::Stop);
        drop(app);
        assert!(events.borrow().is_empty());
    }

    #[test]
    fn hook_stop_short_circuits_remaining_frame_and_shuts_down_once() {
        for hook in [AppHook::Startup, AppHook::FixedUpdate, AppHook::Render] {
            let mut app = app(true);
            let events = app.game().events.clone();
            app.game_mut().stop = Some(hook);
            assert_eq!(app.frame(4.0 / 64.0).unwrap(), AppControl::Stop);
            app.stop().unwrap();
            assert_eq!(app.frame(1.0).unwrap(), AppControl::Stop);
            drop(app);
            let expected = match hook {
                AppHook::Startup => vec![Event::Startup, Event::Shutdown],
                AppHook::FixedUpdate => {
                    vec![Event::Startup, Event::Update(1.0 / 64.0), Event::Shutdown]
                }
                AppHook::Render => vec![
                    Event::Startup,
                    Event::Update(1.0 / 64.0),
                    Event::Update(1.0 / 64.0),
                    Event::Update(1.0 / 64.0),
                    Event::Update(1.0 / 64.0),
                    Event::Render(0.0),
                    Event::Shutdown,
                ],
                AppHook::Shutdown => unreachable!(),
            };
            assert_eq!(*events.borrow(), expected);
        }
    }

    #[test]
    fn each_hook_error_stops_without_retry_and_preserves_cleanup_failure() {
        for hook in [AppHook::Startup, AppHook::FixedUpdate, AppHook::Render] {
            for fail_shutdown in [false, true] {
                let mut app = app(true);
                let events = app.game().events.clone();
                app.game_mut().fail = Some(hook);
                app.game_mut().fail_shutdown = fail_shutdown;
                let error = app.frame(4.0 / 64.0).unwrap_err();
                assert_eq!(error.hook, hook);
                assert_eq!(error.error, "primary");
                assert_eq!(error.shutdown_error, fail_shutdown.then_some("cleanup"));
                assert!(
                    error
                        .to_string()
                        .contains(&format!("{hook} hook failed: primary"))
                );
                assert_eq!(app.state(), AppState::Stopped);
                assert_eq!(app.frame(1.0).unwrap(), AppControl::Stop);
                app.stop().unwrap();
                drop(app);
                let events = events.borrow();
                assert_eq!(events.first(), Some(&Event::Startup));
                assert_eq!(events.last(), Some(&Event::Shutdown));
                assert_eq!(events.iter().filter(|e| **e == Event::Shutdown).count(), 1);
                assert_eq!(
                    events
                        .iter()
                        .filter(|e| matches!(e, Event::Update(_)))
                        .count(),
                    match hook {
                        AppHook::Startup => 0,
                        AppHook::FixedUpdate => 1,
                        AppHook::Render => 4,
                        AppHook::Shutdown => unreachable!(),
                    }
                );
                assert_eq!(
                    events
                        .iter()
                        .filter(|e| matches!(e, Event::Render(_)))
                        .count(),
                    usize::from(hook == AppHook::Render)
                );
            }
        }
    }

    #[test]
    fn explicit_and_hook_requested_stop_report_shutdown_error_once() {
        for hook in [
            None,
            Some(AppHook::Startup),
            Some(AppHook::FixedUpdate),
            Some(AppHook::Render),
        ] {
            let mut app = app(true);
            let events = app.game().events.clone();
            app.game_mut().stop = hook;
            app.game_mut().fail_shutdown = true;
            let error = if hook.is_some() {
                app.frame(1.0 / 64.0).unwrap_err()
            } else {
                app.startup().unwrap();
                app.stop().unwrap_err()
            };
            assert_eq!(
                error,
                AppError {
                    hook: AppHook::Shutdown,
                    error: "cleanup",
                    shutdown_error: None,
                }
            );
            app.stop().unwrap();
            drop(app);
            assert_eq!(
                events
                    .borrow()
                    .iter()
                    .filter(|e| **e == Event::Shutdown)
                    .count(),
                1
            );
        }
    }

    #[test]
    fn drop_cleans_up_started_apps_only_and_handles_fallible_cleanup() {
        for started in [false, true] {
            let mut app = app(true);
            let events = app.game().events.clone();
            app.game_mut().fail_shutdown = true;
            if started {
                app.startup().unwrap();
            }
            drop(app);
            let expected = if started {
                vec![Event::Startup, Event::Shutdown]
            } else {
                vec![]
            };
            assert_eq!(*events.borrow(), expected);
        }
    }

    #[test]
    fn bounded_catchup_retains_interpolation_and_invalid_elapsed_is_ignored() {
        let mut app = app(true);
        app.frame(10.5 / 64.0).unwrap();
        assert_eq!(app.session().tick(), 4);
        assert_eq!(app.game().events.borrow().last(), Some(&Event::Render(0.5)));
        for elapsed in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
            app.frame(elapsed).unwrap();
            assert_eq!(app.session().tick(), 4);
            assert_eq!(app.game().events.borrow().last(), Some(&Event::Render(0.5)));
        }
        app.frame(0.5 / 64.0).unwrap();
        assert_eq!(app.session().tick(), 5);
        assert_eq!(app.game().events.borrow().last(), Some(&Event::Render(0.0)));
    }

    #[test]
    fn different_host_frame_rates_produce_the_same_simulation() {
        for fps in [30, 60, 144] {
            let mut app = app(true);
            for _ in 0..fps {
                app.frame(1.0 / f64::from(fps)).unwrap();
            }
            assert_eq!(app.session().tick(), 64);
            let events = app.game().events.borrow();
            let updates: Vec<_> = events
                .iter()
                .filter_map(|e| match e {
                    Event::Update(dt) => Some(*dt),
                    _ => None,
                })
                .collect();
            assert_eq!(updates, vec![1.0 / 64.0; 64]);
        }
    }

    fn empty_report(control: AppControl, rendered: bool) -> AppFrameReport {
        AppFrameReport {
            scheduled_updates: 0,
            executed_updates: 0,
            dt: 1.0 / 64.0,
            alpha: 0.0,
            dropped_seconds: 0.0,
            rendered,
            control,
        }
    }

    #[test]
    fn reports_count_actual_updates_and_refresh_on_zero_tick_and_legacy_frames() {
        let mut app = app(true);
        assert_eq!(app.last_frame_report(), None);
        app.startup().unwrap();
        assert_eq!(app.last_frame_report(), None);
        let report = app.frame_report(2.5 / 64.0).unwrap();
        assert_eq!(
            report,
            AppFrameReport {
                scheduled_updates: 2,
                executed_updates: 2,
                alpha: 0.5,
                ..empty_report(AppControl::Continue, true)
            }
        );
        assert_eq!(app.last_frame_report(), Some(report));
        assert_eq!(app.game().events.borrow().last(), Some(&Event::Render(0.5)));

        let report = app.frame_report(0.25 / 64.0).unwrap();
        assert_eq!(
            report,
            AppFrameReport {
                alpha: 0.75,
                ..empty_report(AppControl::Continue, true)
            }
        );
        assert_eq!(app.last_frame_report(), Some(report));
        assert_eq!(
            app.game().events.borrow().last(),
            Some(&Event::Render(0.75))
        );

        assert_eq!(app.frame(0.25 / 64.0).unwrap(), AppControl::Continue);
        assert_eq!(
            app.last_frame_report(),
            Some(AppFrameReport {
                scheduled_updates: 1,
                executed_updates: 1,
                ..empty_report(AppControl::Continue, true)
            })
        );
        assert_eq!(app.session().tick(), 3);
        assert_eq!(app.game().events.borrow().last(), Some(&Event::Render(0.0)));
    }

    #[test]
    fn reports_expose_budget_drops_but_not_invalid_or_capped_away_elapsed() {
        let mut app = app(true);
        assert_eq!(
            app.frame_report(10.5 / 64.0).unwrap(),
            AppFrameReport {
                scheduled_updates: 4,
                executed_updates: 4,
                alpha: 0.5,
                dropped_seconds: 6.0 / 64.0,
                ..empty_report(AppControl::Continue, true)
            }
        );
        for elapsed in [0.0, -0.0, -1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                app.frame_report(elapsed).unwrap(),
                AppFrameReport {
                    alpha: 0.5,
                    ..empty_report(AppControl::Continue, true)
                }
            );
            assert_eq!(app.session().tick(), 4);
        }
        assert_eq!(app.frame_report(0.5 / 64.0).unwrap().executed_updates, 1);
    }

    #[test]
    fn reports_preserve_the_core_elapsed_cap_accounting() {
        for elapsed in [3600.0, 7200.0, f64::MAX] {
            let mut app = app(true);
            assert_eq!(
                app.frame_report(elapsed).unwrap(),
                AppFrameReport {
                    scheduled_updates: 4,
                    executed_updates: 4,
                    dropped_seconds: 3600.0 - 4.0 / 64.0,
                    ..empty_report(AppControl::Continue, true)
                }
            );
        }
    }

    #[test]
    fn inactive_reports_render_without_timing_or_application_stop() {
        for state in [
            SessionState::Ready,
            SessionState::Paused,
            SessionState::Won,
            SessionState::Lost,
        ] {
            let mut app = app(false);
            if state != SessionState::Ready {
                app.session_mut().start();
                assert_eq!(app.frame_report(0.5 / 64.0).unwrap().alpha, 0.5);
                if state == SessionState::Paused {
                    app.session_mut().set_paused(true);
                } else {
                    app.session_mut().finish(state == SessionState::Won);
                }
            }
            assert_eq!(
                app.frame_report(100.0).unwrap(),
                empty_report(AppControl::Continue, true)
            );
            assert_eq!(app.session().state(), state);
            assert_eq!(app.session().tick(), 0);
            assert_eq!(app.state(), AppState::Running);
            if state == SessionState::Paused {
                app.session_mut().set_paused(false);
                let resumed = app.frame_report(0.5 / 64.0).unwrap();
                assert_eq!(resumed.executed_updates, 1);
                assert_eq!(resumed.alpha, 0.0);
                assert_eq!(resumed.dropped_seconds, 0.0);
            }
            app.session_mut().restart();
            assert_eq!(
                app.frame_report(0.5 / 64.0).unwrap(),
                AppFrameReport {
                    alpha: 0.5,
                    ..empty_report(AppControl::Continue, true)
                }
            );
            assert_eq!(
                app.game()
                    .events
                    .borrow()
                    .iter()
                    .filter(|event| **event == Event::Startup)
                    .count(),
                1
            );
        }
    }

    fn assert_terminated_report(app: &App<Probe>, hook: AppHook) {
        let executed_updates = match hook {
            AppHook::Startup => 0,
            AppHook::FixedUpdate => 3,
            AppHook::Render => 4,
            AppHook::Shutdown => unreachable!(),
        };
        let advanced = hook != AppHook::Startup;
        assert_eq!(
            app.last_frame_report(),
            Some(AppFrameReport {
                scheduled_updates: if advanced { 4 } else { 0 },
                executed_updates,
                alpha: if advanced { 0.5 } else { 0.0 },
                dropped_seconds: if advanced { 6.0 / 64.0 } else { 0.0 },
                ..empty_report(AppControl::Stop, hook == AppHook::Render)
            })
        );
        assert_eq!(app.session().tick(), if advanced { 4 } else { 0 });
        assert_eq!(app.state(), AppState::Stopped);
        let mut expected = vec![Event::Startup];
        expected.extend(std::iter::repeat_n(
            Event::Update(1.0 / 64.0),
            executed_updates as usize,
        ));
        if hook == AppHook::Render {
            expected.push(Event::Render(0.5));
        }
        expected.push(Event::Shutdown);
        assert_eq!(*app.game().events.borrow(), expected);
    }

    #[test]
    fn stop_reports_count_the_stopping_hook_and_preserve_cleanup_failure_order() {
        for hook in [AppHook::Startup, AppHook::FixedUpdate, AppHook::Render] {
            for fail_shutdown in [false, true] {
                let mut app = app(true);
                let events = app.game().events.clone();
                app.game_mut().stop = Some(hook);
                app.game_mut().updates_before_control = 2;
                app.game_mut().fail_shutdown = fail_shutdown;
                let result = app.frame_report(10.5 / 64.0);
                if fail_shutdown {
                    assert_eq!(
                        result.unwrap_err(),
                        AppError {
                            hook: AppHook::Shutdown,
                            error: "cleanup",
                            shutdown_error: None,
                        }
                    );
                } else {
                    assert_eq!(app.last_frame_report(), Some(result.unwrap()));
                }
                assert_terminated_report(&app, hook);
                let event_count = events.borrow().len();
                app.stop().unwrap();
                assert_eq!(app.startup().unwrap(), AppControl::Stop);
                assert_eq!(
                    app.frame_report(100.0).unwrap(),
                    empty_report(AppControl::Stop, false)
                );
                assert_eq!(app.frame(1.0).unwrap(), AppControl::Stop);
                drop(app);
                assert_eq!(events.borrow().len(), event_count);
            }
        }
    }

    #[test]
    fn failed_frames_retain_partial_reports_and_both_errors_without_retry() {
        for legacy in [false, true] {
            for hook in [AppHook::Startup, AppHook::FixedUpdate, AppHook::Render] {
                for fail_shutdown in [false, true] {
                    let mut app = app(true);
                    let events = app.game().events.clone();
                    app.game_mut().fail = Some(hook);
                    app.game_mut().updates_before_control = 2;
                    app.game_mut().fail_shutdown = fail_shutdown;
                    let error = if legacy {
                        app.frame(10.5 / 64.0).unwrap_err()
                    } else {
                        app.frame_report(10.5 / 64.0).unwrap_err()
                    };
                    assert_eq!(
                        error,
                        AppError {
                            hook,
                            error: "primary",
                            shutdown_error: fail_shutdown.then_some("cleanup"),
                        }
                    );
                    assert_terminated_report(&app, hook);
                    let event_count = events.borrow().len();
                    app.stop().unwrap();
                    assert_eq!(
                        app.frame_report(100.0).unwrap(),
                        empty_report(AppControl::Stop, false)
                    );
                    drop(app);
                    assert_eq!(events.borrow().len(), event_count);
                }
            }
        }
    }

    #[test]
    fn last_report_is_a_snapshot_and_failed_frames_replace_previous_success() {
        let mut app = app(true);
        let successful = app.frame_report(0.5 / 64.0).unwrap();
        app.session_mut().set_paused(true);
        app.startup().unwrap();
        assert_eq!(app.last_frame_report(), Some(successful));
        app.session_mut().set_paused(false);
        app.game_mut().fail = Some(AppHook::FixedUpdate);
        app.frame_report(2.0 / 64.0).unwrap_err();
        let failed = app.last_frame_report().unwrap();
        assert_eq!(failed.control, AppControl::Stop);
        assert_eq!(failed.executed_updates, 1);
        assert_eq!(failed.scheduled_updates, 2);
        assert!(!failed.rendered);
        app.stop().unwrap();
        app.session_mut().restart();
        assert_eq!(app.last_frame_report(), Some(failed));
        assert_eq!(
            app.frame_report(100.0).unwrap(),
            empty_report(AppControl::Stop, false)
        );
        assert_eq!(app.session().tick(), 0);
    }

    #[test]
    fn report_after_stop_before_start_does_no_work_and_ignores_elapsed() {
        let mut app = app(true);
        let events = app.game().events.clone();
        app.stop().unwrap();
        assert_eq!(app.last_frame_report(), None);
        let report = app.frame_report(100.0).unwrap();
        assert_eq!(report, empty_report(AppControl::Stop, false));
        assert_eq!(app.last_frame_report(), Some(report));
        assert_eq!(app.session().tick(), 0);
        drop(app);
        assert!(events.borrow().is_empty());
    }

    #[test]
    fn errors_expose_the_primary_source() {
        use std::error::Error;
        let error = AppError {
            hook: AppHook::FixedUpdate,
            error: std::io::Error::other("simulation failed"),
            shutdown_error: Some(std::io::Error::other("cleanup failed")),
        };
        assert_eq!(error.source().unwrap().to_string(), "simulation failed");
        assert_eq!(
            error.to_string(),
            "fixed update hook failed: simulation failed; shutdown also failed: cleanup failed"
        );
    }
}
