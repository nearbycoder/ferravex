//! Shared start, pause, retry and fixed-timestep timing for local game sessions.
//!
//! Game-specific rules still decide when a session has been won or lost. A local
//! menu or overlay must not pause an authoritative remote match's clock.
//!
//! ```
//! use ferravex_core::session::{GameSession, SessionState};
//!
//! let mut session = GameSession::new(60.0, 8, false);
//! assert!(session.start());
//! let frame = session.advance(1.0 / 30.0);
//! let mut simulated_seconds = 0.0;
//! for _ in 0..frame.steps {
//!     simulated_seconds += frame.dt; // Update game rules with this fixed dt.
//! }
//! assert_eq!(session.tick(), 2);
//! assert!(simulated_seconds > 0.0);
//! assert!(session.set_paused(true));
//! assert_eq!(session.advance(10.0).steps, 0);
//! assert!(session.toggle_pause());
//! assert!(session.finish(true));
//! assert_eq!(session.state(), SessionState::Won);
//! session.restart();
//! assert_eq!(session.tick(), 0);
//! ```

use crate::{FixedClock, FixedClockError, FrameStep, TickBatch};

/// Validated, reusable settings for a [`GameSession`].
///
/// Fields cannot be changed after validation. Use [`Self::try_new`] to check
/// settings from a host or inspector, then [`GameSession::from_config`] to
/// create independent sessions with the same settings.
///
/// ```
/// use ferravex_core::session::{GameSession, SessionConfig, SessionState};
///
/// let config = SessionConfig::try_new(64.0, 4, false)?;
/// assert_eq!(config.hz(), 64.0);
/// assert_eq!(config.max_steps(), 4);
/// assert!(!config.autoplay());
/// let first = GameSession::from_config(config);
/// let second = GameSession::from_config(config);
/// assert_eq!(first.state(), SessionState::Ready);
/// assert_eq!(second.tick(), 0);
/// # Ok::<(), ferravex_core::FixedClockError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SessionConfig {
    hz: f64,
    max_steps: u32,
    autoplay: bool,
}

impl SessionConfig {
    /// Validates settings using [`FixedClock::try_new`].
    ///
    /// `hz` must be finite and in `1..=1000`, and `max_steps` must be nonzero.
    /// The tick rate is checked first if both values are invalid. `autoplay`
    /// selects `Playing` instead of `Ready` for each newly created session.
    /// See the [`SessionConfig`] example for constructing and reusing settings.
    pub fn try_new(hz: f64, max_steps: u32, autoplay: bool) -> Result<Self, FixedClockError> {
        FixedClock::try_new(hz, max_steps)?;
        Ok(Self {
            hz,
            max_steps,
            autoplay,
        })
    }

    /// Returns the validated fixed tick rate in Hz; see [`SessionConfig`].
    pub fn hz(&self) -> f64 {
        self.hz
    }

    /// Returns the nonzero per-frame catch-up limit; see [`SessionConfig`].
    pub fn max_steps(&self) -> u32 {
        self.max_steps
    }

    /// Whether a newly created session starts in `Playing`; see [`SessionConfig`].
    pub fn autoplay(&self) -> bool {
        self.autoplay
    }
}

/// Host lifecycle state, independent of a game's simulation-specific phases.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    /// Waiting for an explicit start.
    Ready,
    /// Accepting elapsed time and producing simulation steps.
    Playing,
    /// Suspended; elapsed time is ignored, but an explicit inspector step is allowed.
    /// Fractional time is retained, and ticks change only via [`GameSession::step_paused`].
    Paused,
    /// Finished successfully; only reset or restart can leave this state.
    Won,
    /// Finished unsuccessfully; only reset or restart can leave this state.
    Lost,
}

/// A rejected request to issue an inspector tick with [`GameSession::step_paused`].
///
/// ```
/// use ferravex_core::session::{GameSession, SessionState, SessionStepError};
///
/// let mut session = GameSession::new(60.0, 8, false);
/// let error = session.step_paused().unwrap_err();
/// assert_eq!(error, SessionStepError::NotPaused { state: SessionState::Ready });
/// assert_eq!(session.tick(), 0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionStepError {
    /// The session must be `Paused`; the state and clock were left unchanged.
    NotPaused {
        /// The state in which stepping was requested.
        state: SessionState,
    },
}

impl std::fmt::Display for SessionStepError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotPaused { state } => write!(
                f,
                "single stepping requires a Paused session; current state is {state:?}"
            ),
        }
    }
}

impl std::error::Error for SessionStepError {}

/// A local game lifecycle with an owned, deterministic [`FixedClock`].
///
/// Only [`Playing`](SessionState::Playing) consumes elapsed time. Pausing and
/// finishing preserve clock state; restarting and resetting clear both ticks
/// and fractional time. Lifecycle transitions do not reset game-specific data:
/// callers must reset their simulation alongside [`restart`](Self::restart).
/// Inspectors can explicitly issue one tick while paused using
/// [`step_paused`](Self::step_paused), without resuming or consuming elapsed time.
#[derive(Clone, Debug)]
pub struct GameSession {
    state: SessionState,
    clock: FixedClock,
}

impl GameSession {
    /// Creates a session in `Ready`, or `Playing` when `autoplay` is true.
    ///
    /// # Panics
    ///
    /// Like [`FixedClock::new`], panics if `hz` is not finite or outside
    /// `1..=1000`, or if `max_steps` is zero.
    /// Use [`Self::try_new`] to report invalid settings without panicking.
    pub fn new(hz: f64, max_steps: u32, autoplay: bool) -> Self {
        Self {
            state: if autoplay {
                SessionState::Playing
            } else {
                SessionState::Ready
            },
            clock: FixedClock::new(hz, max_steps),
        }
    }

    /// Creates a session with checked settings and the same behavior as [`Self::new`].
    ///
    /// Returns [`FixedClockError`] if `hz` is not finite or outside `1..=1000`,
    /// or if `max_steps` is zero. The tick rate is checked first if both are
    /// invalid. For reusable validated settings, see [`SessionConfig`].
    ///
    /// ```
    /// use ferravex_core::{FixedClockError, session::GameSession};
    ///
    /// assert!(matches!(
    ///     GameSession::try_new(60.0, 0, false),
    ///     Err(FixedClockError::ZeroCatchUpBudget)
    /// ));
    /// let mut session = GameSession::try_new(64.0, 4, true)?;
    /// assert_eq!(session.advance(2.0 / 64.0).steps, 2);
    /// # Ok::<(), FixedClockError>(())
    /// ```
    pub fn try_new(hz: f64, max_steps: u32, autoplay: bool) -> Result<Self, FixedClockError> {
        let config = SessionConfig::try_new(hz, max_steps, autoplay)?;
        Ok(Self::from_config(config))
    }

    /// Creates a fresh session from reusable, already validated settings.
    ///
    /// Starts at tick zero with no fractional time. `config.autoplay()` selects
    /// `Playing` instead of `Ready`. See the [`SessionConfig`] example.
    pub fn from_config(config: SessionConfig) -> Self {
        Self::new(config.hz, config.max_steps, config.autoplay)
    }

    /// Returns the current host lifecycle state.
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Whether this session has started, including paused or finished sessions.
    pub fn is_started(&self) -> bool {
        self.state != SessionState::Ready
    }

    /// Whether elapsed time currently advances the simulation clock.
    pub fn is_playing(&self) -> bool {
        self.state == SessionState::Playing
    }

    /// Whether this session is temporarily paused.
    pub fn is_paused(&self) -> bool {
        self.state == SessionState::Paused
    }

    /// Starts a ready session. Returns true only for `Ready -> Playing`.
    ///
    /// To resume use [`set_paused(false)`](Self::set_paused); to retry a finished
    /// game use [`restart`](Self::restart).
    pub fn start(&mut self) -> bool {
        if self.state != SessionState::Ready {
            return false;
        }
        self.state = SessionState::Playing;
        true
    }

    /// Clears ticks and fractional time and enters `Playing` from any state.
    pub fn restart(&mut self) {
        self.clock.reset();
        self.state = SessionState::Playing;
    }

    /// Clears ticks and fractional time and enters `Ready` from any state.
    pub fn reset(&mut self) {
        self.clock.reset();
        self.state = SessionState::Ready;
    }

    /// Pauses or resumes an active session, returning true only when changed.
    ///
    /// Ready and finished sessions are unchanged. Repeated requests for the
    /// current pause state are harmless and return false.
    pub fn set_paused(&mut self, paused: bool) -> bool {
        match (self.state, paused) {
            (SessionState::Playing, true) => self.state = SessionState::Paused,
            (SessionState::Paused, false) => self.state = SessionState::Playing,
            _ => return false,
        }
        true
    }

    /// Toggles `Playing` and `Paused`, returning true only when changed.
    ///
    /// Ready and finished sessions are unchanged.
    pub fn toggle_pause(&mut self) -> bool {
        self.set_paused(!self.is_paused())
    }

    /// Finishes a playing or paused session as `Won` or `Lost`.
    ///
    /// Returns true only when changed. A ready session cannot finish, and an
    /// existing result cannot be overwritten without resetting or restarting.
    pub fn finish(&mut self, won: bool) -> bool {
        if !matches!(self.state, SessionState::Playing | SessionState::Paused) {
            return false;
        }
        self.state = if won {
            SessionState::Won
        } else {
            SessionState::Lost
        };
        true
    }

    /// Produces bounded fixed steps from the elapsed seconds of one host frame.
    ///
    /// While inactive, returns zero steps, alpha and dropped time with the
    /// normal fixed `dt`, and leaves the clock completely unchanged. Time
    /// submitted while inactive is discarded, so resuming does not catch up
    /// on that time. Fractional time from earlier playing frames is retained.
    ///
    /// Callers must measure elapsed time between host frames, including while
    /// inactive. If the host itself stops delivering frames, rebase its elapsed
    /// time measurement on resume instead of submitting the whole suspension.
    /// In `Playing`, invalid durations and catch-up use [`FixedClock::advance`]
    /// semantics. The tick counter includes all steps returned by this call.
    pub fn advance(&mut self, elapsed_seconds: f64) -> FrameStep {
        if self.is_playing() {
            self.clock.advance(elapsed_seconds)
        } else {
            FrameStep {
                steps: 0,
                dt: self.clock.dt(),
                alpha: 0.0,
                dropped_seconds: 0.0,
            }
        }
    }

    /// Schedule a frame with per-update IDs, using [`Self::advance`] semantics.
    ///
    /// Inactive sessions return an empty batch without consuming time or IDs.
    /// The clock counts the entire issued batch, even if the caller stops early.
    /// Consume input and count execution only when actually invoking an update.
    /// [`TickBatch`] documents wrapping, copying and reset semantics.
    ///
    /// ```
    /// use ferravex_core::session::GameSession;
    /// let mut session = GameSession::new(64.0, 4, true);
    /// let batch = session.advance_batch(3.0 / 64.0);
    /// let mut executed = 0;
    /// for context in batch.ticks() {
    ///     if !session.is_playing() { break; }
    ///     assert_eq!(context.tick, 1);
    ///     executed += 1; // Invoke game rules; this update wins the game.
    ///     session.finish(true);
    /// }
    /// assert_eq!(executed, 1);
    /// assert_eq!(session.tick(), 3);
    /// assert_eq!(session.advance_batch(60.0).ticks().len(), 0);
    /// session.restart();
    /// assert_eq!(session.advance_batch(1.0 / 64.0).ticks().next().unwrap().tick, 1);
    /// ```
    pub fn advance_batch(&mut self, elapsed_seconds: f64) -> TickBatch {
        let tick_before = self.tick();
        let frame = self.advance(elapsed_seconds);
        TickBatch { frame, tick_before }
    }

    /// Issues exactly one fixed simulation tick while remaining `Paused`.
    ///
    /// Returns [`SessionStepError::NotPaused`] in every other state, leaving
    /// both lifecycle and clock unchanged. An accepted step increments
    /// [`tick`](Self::tick) once (wrapping at `u64::MAX`), preserves all
    /// fractional time from earlier playing frames, and consumes no elapsed
    /// time. Repeated calls each issue one tick, independent of the per-frame
    /// catch-up limit. Ordinary paused [`advance`](Self::advance) calls still
    /// discard their elapsed time and issue no ticks.
    ///
    /// The returned frame has `steps = 1`, the normal fixed `dt`, no dropped
    /// time, and `alpha = 1.0` so an inspector can display the current pose after
    /// the update. This presentation alpha does not change the clock's stored
    /// fractional time or the alpha returned by later `advance` calls.
    ///
    /// Like `advance`, this method only schedules work: the tick is counted
    /// before the caller executes the update. Consume gameplay input edges and
    /// emit accepted game feedback inside that update. Game rules can still
    /// [`finish`](Self::finish) a session after the step.
    ///
    /// ```
    /// use ferravex_core::session::{GameSession, SessionState};
    ///
    /// let mut session = GameSession::new(64.0, 4, true);
    /// session.advance(0.5 / 64.0); // Retain half a normal tick for resume.
    /// assert!(session.set_paused(true));
    /// let frame = session.step_paused()?;
    /// let mut simulated_seconds = 0.0;
    /// for dt in frame.ticks() {
    ///     simulated_seconds += dt; // Execute the inspector's fixed update.
    /// }
    /// assert_eq!(simulated_seconds, session.dt());
    /// assert_eq!(frame.alpha, 1.0); // Present the current simulation pose.
    /// assert_eq!(session.tick(), 1);
    /// assert_eq!(session.state(), SessionState::Paused);
    /// assert_eq!(session.advance(30.0).steps, 0);
    /// assert!(session.set_paused(false));
    /// assert_eq!(session.advance(0.5 / 64.0).steps, 1);
    /// assert_eq!(session.tick(), 2);
    /// # Ok::<(), ferravex_core::session::SessionStepError>(())
    /// ```
    pub fn step_paused(&mut self) -> Result<FrameStep, SessionStepError> {
        if !self.is_paused() {
            return Err(SessionStepError::NotPaused { state: self.state });
        }
        // Adding dt to the accumulator would round/consume its remainder and
        // could issue multiple steps near a tick boundary. Only count the
        // explicitly requested tick, retaining the exact accumulator bits.
        self.clock.tick = self.clock.tick.wrapping_add(1);
        Ok(FrameStep {
            steps: 1,
            dt: self.clock.dt(),
            alpha: 1.0,
            dropped_seconds: 0.0,
        })
    }

    /// Issue one paused inspector update with its scheduled ID and index zero.
    ///
    /// Uses [`Self::step_paused`] unchanged: rejects non-Paused states without
    /// mutation, preserves fractional time and returns presentation alpha 1.
    /// The caller must execute the tick; this does not invoke application hooks.
    ///
    /// ```
    /// use ferravex_core::session::GameSession;
    /// let mut session = GameSession::new(64.0, 4, true);
    /// session.advance_batch(1.5 / 64.0);
    /// session.set_paused(true);
    /// let batch = session.step_paused_batch()?;
    /// let context = batch.ticks().next().unwrap();
    /// assert_eq!((context.tick, context.batch_index), (2, 0));
    /// assert_eq!(context.dt, session.dt()); // Execute the inspector update here.
    /// assert_eq!(batch.frame().alpha, 1.0);
    /// assert!(session.is_paused());
    /// session.set_paused(false);
    /// assert_eq!(session.advance_batch(0.5 / 64.0).ticks().next().unwrap().tick, 3);
    /// # Ok::<(), ferravex_core::session::SessionStepError>(())
    /// ```
    pub fn step_paused_batch(&mut self) -> Result<TickBatch, SessionStepError> {
        let tick_before = self.tick();
        let frame = self.step_paused()?;
        Ok(TickBatch { frame, tick_before })
    }

    /// Returns the number of fixed steps issued since creation or the last reset/restart.
    ///
    /// Counts both playing [`advance`](Self::advance) batches and accepted
    /// [`step_paused`](Self::step_paused) calls, wrapping at `u64::MAX` like
    /// [`FixedClock::tick`]. Dropped catch-up ticks and rejected requests do not
    /// count. This is scheduled work, not a count of updates the caller actually
    /// executed: the whole returned batch is already counted when `advance`
    /// returns, even if game rules finish before the caller executes every step.
    pub fn tick(&self) -> u64 {
        self.clock.tick()
    }

    /// Returns the fixed simulation step duration in seconds in every state.
    pub fn dt(&self) -> f32 {
        self.clock.dt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contextual_inactive_and_rejected_steps_preserve_state_and_clock() {
        for state in [
            SessionState::Ready,
            SessionState::Playing,
            SessionState::Won,
            SessionState::Lost,
        ] {
            let mut session = GameSession::new(64.0, 4, false);
            if state != SessionState::Ready {
                session.start();
                session.advance_batch(2.5 / 64.0);
                if state != SessionState::Playing {
                    session.finish(state == SessionState::Won);
                }
            }
            let before = session.tick();
            let fraction = session.clock.accumulator.to_bits();
            assert_eq!(
                session.step_paused_batch().unwrap_err(),
                SessionStepError::NotPaused { state }
            );
            assert_eq!(session.state(), state);
            assert_eq!(session.tick(), before);
            assert_eq!(session.clock.accumulator.to_bits(), fraction);
            if state != SessionState::Playing {
                let batch = session.advance_batch(100.0);
                assert_eq!(batch.ticks().len(), 0);
                assert_eq!(batch.frame().dt, session.dt());
                assert_eq!(batch.frame().alpha, 0.0);
                assert_eq!(batch.frame().dropped_seconds, 0.0);
                assert_eq!(session.tick(), before);
                assert_eq!(session.clock.accumulator.to_bits(), fraction);
            }
        }
    }

    #[test]
    fn contextual_paused_steps_wrap_and_resume_with_exact_retained_fraction() {
        for hz in [1.0, 60.0, 64.0, 1000.0] {
            let mut session = GameSession::new(hz, 1, true);
            session.advance_batch(0.375 / hz);
            session.set_paused(true);
            session.clock.tick = u64::MAX;
            let fraction = session.clock.accumulator.to_bits();
            for tick in 0..3 {
                let batch = session.step_paused_batch().unwrap();
                let contexts: Vec<_> = batch.ticks().collect();
                assert_eq!(
                    contexts,
                    [crate::TickContext {
                        tick,
                        batch_index: 0,
                        dt: session.dt()
                    }]
                );
                assert_eq!(batch.frame().steps, 1);
                assert_eq!(batch.frame().alpha, 1.0);
                assert_eq!(batch.frame().dropped_seconds, 0.0);
                assert_eq!(session.tick(), tick);
                assert_eq!(session.state(), SessionState::Paused);
                assert_eq!(session.advance_batch(100.0).ticks().len(), 0);
                assert_eq!(session.clock.accumulator.to_bits(), fraction);
            }
            // Mixing the old and new manual APIs uses the same ID sequence.
            session.step_paused().unwrap();
            session.set_paused(false);
            assert_eq!(
                session
                    .advance_batch(0.625 / hz)
                    .ticks()
                    .next()
                    .unwrap()
                    .tick,
                4
            );
        }
    }

    #[test]
    fn contextual_early_finish_reset_and_restart_preserve_scheduled_accounting() {
        let mut session = GameSession::new(64.0, 4, true);
        let batch = session.advance_batch(4.5 / 64.0);
        let mut executed = 0;
        for context in batch.ticks() {
            if !session.is_playing() {
                break;
            }
            assert_eq!(context.tick, 1);
            executed += 1;
            session.finish(true);
        }
        assert_eq!(executed, 1);
        assert_eq!(session.tick(), 4);
        assert_eq!(session.advance_batch(100.0).ticks().len(), 0);
        for restart in [false, true] {
            if restart {
                session.restart();
            } else {
                session.reset();
                assert_eq!(session.advance_batch(1.0).ticks().len(), 0);
                session.start();
            }
            assert_eq!(session.tick(), 0);
            assert_eq!(session.advance_batch(0.5 / 64.0).ticks().len(), 0);
            assert_eq!(
                session
                    .advance_batch(0.5 / 64.0)
                    .ticks()
                    .next()
                    .unwrap()
                    .tick,
                1
            );
            session.set_paused(true);
            assert_eq!(
                session
                    .step_paused_batch()
                    .unwrap()
                    .ticks()
                    .next()
                    .unwrap()
                    .tick,
                2
            );
        }
    }

    #[test]
    fn paused_step_wraps_tick_and_preserves_exact_fraction() {
        let mut session = GameSession::new(60.0, 8, true);
        session.advance(0.375 / 60.0);
        session.set_paused(true);
        session.clock.tick = u64::MAX;
        let accumulator = session.clock.accumulator.to_bits();

        let frame = session.step_paused().unwrap();
        assert_eq!(frame.steps, 1);
        assert_eq!(session.tick(), 0);
        assert_eq!(session.clock.accumulator.to_bits(), accumulator);
        assert_eq!(session.state(), SessionState::Paused);

        session.step_paused().unwrap();
        assert_eq!(session.tick(), 1);
        assert_eq!(session.clock.accumulator.to_bits(), accumulator);
        session.set_paused(false);
        assert_eq!(session.advance(0.625 / 60.0).steps, 1);
        assert_eq!(session.tick(), 2);
    }
}
