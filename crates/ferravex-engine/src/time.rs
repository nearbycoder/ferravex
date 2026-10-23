//! Supplied simulation time, pause/scale control, and drift-free duration timers.
//!
//! No wall-clock reads. Feed [`TimeStep::scaled_delta`] to a fixed clock and
//! advance gameplay timers only with the duration of updates actually executed.
//! Do not also feed them the render-frame delta. For systems outside a fixed
//! update loop, choose `scaled_delta` for simulation or `unscaled_delta` for UI:
//!
//! ```
//! use ferravex_engine::time::{SimulationTime, SimulationTimer};
//! use std::time::Duration;
//!
//! let mut time = SimulationTime::default();
//! let mut timer = SimulationTimer::repeating(Duration::from_millis(100), 2)?;
//! assert!(time.set_scale(0.5));
//! let step = time.advance(Duration::from_millis(250));
//! assert_eq!(timer.advance(step.scaled_delta)?.expirations, 1);
//! assert_eq!(timer.elapsed(), Duration::from_millis(25));
//!
//! time.set_paused(true);
//! let step = time.advance(Duration::from_millis(250));
//! assert_eq!(timer.advance(step.scaled_delta)?.expirations, 0);
//! assert_eq!(timer.elapsed(), Duration::from_millis(25));
//! # Ok::<(), ferravex_engine::time::TimerError>(())
//! ```
//!
//! Timers apply no additional scale or stall clamp. They preserve the integer
//! nanoseconds supplied by the caller; upstream float conversion/scaling may
//! round. Their optional local pause discards incoming time without accumulating
//! catch-up debt. Keep authoritative network/replay timers advancing under local
//! menus; pause only timers whose owning simulation is actually paused.

use std::time::Duration;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeStep {
    pub unscaled_delta: Duration,
    pub scaled_delta: Duration,
    pub total: Duration,
}
pub struct SimulationTime {
    scale: f64,
    paused: bool,
    max_delta: Duration,
    total: Duration,
}
impl Default for SimulationTime {
    fn default() -> Self {
        Self::new(Duration::from_millis(250)).unwrap()
    }
}
impl SimulationTime {
    /// Clamp each supplied frame to max_delta (nonzero, at most 60 seconds).
    pub fn new(max_delta: Duration) -> Option<Self> {
        if max_delta.is_zero() || max_delta > Duration::from_secs(60) {
            return None;
        }
        Some(Self {
            scale: 1.,
            paused: false,
            max_delta,
            total: Duration::ZERO,
        })
    }
    /// Supported scale range 0..=16. Invalid changes preserve the prior setting.
    pub fn set_scale(&mut self, scale: f64) -> bool {
        if !scale.is_finite() || !(0.0..=16.0).contains(&scale) {
            return false;
        }
        self.scale = scale;
        true
    }
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }
    pub fn advance(&mut self, elapsed: Duration) -> TimeStep {
        let unscaled_delta = elapsed.min(self.max_delta);
        let scaled_delta = if self.paused {
            Duration::ZERO
        } else {
            unscaled_delta.mul_f64(self.scale)
        };
        self.total = self.total.saturating_add(scaled_delta);
        TimeStep {
            unscaled_delta,
            scaled_delta,
            total: self.total,
        }
    }
    /// Manual paused simulation step. Does not apply time scale or advance a wall clock.
    pub fn step_paused(&mut self, delta: Duration) -> bool {
        if !self.paused || delta > self.max_delta {
            return false;
        }
        self.total = self.total.saturating_add(delta);
        true
    }
    pub fn total(&self) -> Duration {
        self.total
    }
    pub fn reset(&mut self) {
        self.total = Duration::ZERO;
    }
}

/// Whether a [`SimulationTimer`] expires once or at every interval boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerMode {
    /// Expires once, then stays finished until reset.
    Once,
    /// Preserves fractional progress after every batch of interval boundaries.
    Repeating,
}

/// A rejected timer configuration or advance. Errors never change timer state.
///
/// ```
/// use ferravex_engine::time::{SimulationTimer, TimerError};
/// use std::time::Duration;
///
/// assert_eq!(SimulationTimer::once(Duration::ZERO).unwrap_err(), TimerError::ZeroInterval);
/// assert_eq!(
///     SimulationTimer::repeating(Duration::from_secs(1), 0).unwrap_err(),
///     TimerError::ZeroCatchUpLimit,
/// );
/// let mut timer = SimulationTimer::repeating(Duration::from_secs(1), 4)?;
/// timer.advance(Duration::from_nanos(1))?;
/// assert_eq!(timer.advance(Duration::MAX), Err(TimerError::ElapsedOverflow));
/// assert_eq!(timer.elapsed(), Duration::from_nanos(1));
/// # Ok::<(), TimerError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerError {
    /// An interval must contain at least one nanosecond.
    ZeroInterval,
    /// A repeating timer must permit at least one reported expiration per call.
    ZeroCatchUpLimit,
    /// Adding a delta to the repeating timer's remainder exceeded [`Duration::MAX`].
    ElapsedOverflow,
}

impl std::fmt::Display for TimerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ZeroInterval => "timer interval must be nonzero",
            Self::ZeroCatchUpLimit => "timer catch-up limit must be nonzero",
            Self::ElapsedOverflow => "timer elapsed time exceeds Duration::MAX",
        })
    }
}

impl std::error::Error for TimerError {}

/// Expirations produced by one successful [`SimulationTimer::advance`].
///
/// A report is a count, not an event queue: process each returned report once.
/// `expirations + skipped` (in `u128`) is the exact number of interval boundaries
/// crossed by a repeating timer. A one-shot timer reports at most one expiration
/// over its lifetime until reset and never reports skipped expirations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimerReport {
    /// Expirations to handle, at most [`SimulationTimer::max_expirations`].
    pub expirations: u32,
    /// Additional repeating expirations discarded by this call's catch-up limit.
    /// They are not deferred to later calls. `u128` covers even nanosecond intervals
    /// advanced by [`Duration::MAX`] without truncating the count.
    pub skipped: u128,
}

/// A reusable, allocation-free timer driven only by supplied elapsed durations.
///
/// Repeating timers retain `elapsed % interval`, including when the catch-up
/// limit skips expirations. They never reset the remainder to zero merely
/// because they expired. Each advance performs constant work independent of the
/// number of elapsed intervals; callers can bound their own work by iterating
/// only over [`TimerReport::expirations`]. No overdue work is queued internally.
///
/// One-shot timers clamp elapsed time at the interval and discard overshoot.
/// Both modes start unpaused with zero elapsed time. Use [`Self::reset`] to reuse
/// a timer and [`Self::set_paused`] to pause it without losing partial progress.
/// Configuration is immutable; replace the timer to change its interval or mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimulationTimer {
    interval: Duration,
    elapsed: Duration,
    mode: TimerMode,
    max_expirations: u32,
    paused: bool,
}

impl SimulationTimer {
    /// Create a one-shot countdown. Zero duration returns [`TimerError::ZeroInterval`].
    ///
    /// Expiration is an edge in the report; [`Self::is_finished`] remains true
    /// on subsequent calls. Overshoot cannot trigger another expiration.
    ///
    /// ```
    /// use ferravex_engine::time::{SimulationTimer, TimerMode};
    /// use std::time::Duration;
    ///
    /// let mut countdown = SimulationTimer::once(Duration::from_secs(3))?;
    /// assert_eq!(countdown.mode(), TimerMode::Once);
    /// assert_eq!(countdown.advance(Duration::from_secs(2))?.expirations, 0);
    /// assert_eq!(countdown.remaining(), Duration::from_secs(1));
    /// assert_eq!(countdown.advance(Duration::from_secs(2))?.expirations, 1);
    /// assert!(countdown.is_finished());
    /// assert_eq!(countdown.elapsed(), countdown.interval());
    /// assert_eq!(countdown.advance(Duration::MAX)?.expirations, 0);
    /// countdown.reset();
    /// assert!(!countdown.is_finished());
    /// # Ok::<(), ferravex_engine::time::TimerError>(())
    /// ```
    pub fn once(interval: Duration) -> Result<Self, TimerError> {
        Self::new(interval, TimerMode::Once, 1)
    }

    /// Create a repeating timer with a nonzero interval and per-call report limit.
    ///
    /// Zero interval returns [`TimerError::ZeroInterval`] before checking the
    /// limit; a zero limit returns [`TimerError::ZeroCatchUpLimit`]. All nonzero
    /// [`Duration`] intervals and `u32` limits are supported.
    ///
    /// ```
    /// use ferravex_engine::time::{SimulationTimer, TimerMode, TimerReport};
    /// use std::time::Duration;
    ///
    /// let mut pulse = SimulationTimer::repeating(Duration::from_millis(100), 2)?;
    /// assert_eq!(pulse.mode(), TimerMode::Repeating);
    /// assert_eq!(pulse.max_expirations(), 2);
    /// assert_eq!(pulse.advance(Duration::from_millis(550))?, TimerReport {
    ///     expirations: 2, skipped: 3,
    /// });
    /// assert_eq!(pulse.elapsed(), Duration::from_millis(50));
    /// assert_eq!(pulse.remaining(), Duration::from_millis(50));
    /// assert_eq!(pulse.advance(Duration::ZERO)?, TimerReport::default());
    /// assert_eq!(pulse.advance(Duration::from_millis(50))?.expirations, 1);
    /// assert!(!pulse.is_finished());
    /// # Ok::<(), ferravex_engine::time::TimerError>(())
    /// ```
    pub fn repeating(interval: Duration, max_expirations: u32) -> Result<Self, TimerError> {
        Self::new(interval, TimerMode::Repeating, max_expirations)
    }

    fn new(interval: Duration, mode: TimerMode, max_expirations: u32) -> Result<Self, TimerError> {
        if interval.is_zero() {
            return Err(TimerError::ZeroInterval);
        }
        if max_expirations == 0 {
            return Err(TimerError::ZeroCatchUpLimit);
        }
        Ok(Self {
            interval,
            elapsed: Duration::ZERO,
            mode,
            max_expirations,
            paused: false,
        })
    }

    /// Consume elapsed simulation time and report expirations for this call only.
    ///
    /// Supply the duration of an executed simulation update, or explicitly choose
    /// a [`TimeStep`] delta for a frame-driven system. No scale is applied here.
    /// Paused or finished timers ignore the delta and return an empty report.
    /// A zero delta never expires a timer or drains previously skipped work.
    ///
    /// A repeating timer returns [`TimerError::ElapsedOverflow`] if its remainder
    /// plus `delta` exceeds [`Duration::MAX`], leaving all state unchanged. One-shot
    /// timers compare the delta to their remaining duration before addition, so
    /// even [`Duration::MAX`] can finish them without overflowing.
    pub fn advance(&mut self, delta: Duration) -> Result<TimerReport, TimerError> {
        if self.paused || self.is_finished() {
            return Ok(TimerReport::default());
        }
        if self.mode == TimerMode::Once && delta >= self.remaining() {
            self.elapsed = self.interval;
            return Ok(TimerReport {
                expirations: 1,
                skipped: 0,
            });
        }
        let elapsed = self
            .elapsed
            .checked_add(delta)
            .ok_or(TimerError::ElapsedOverflow)?;
        if self.mode == TimerMode::Once {
            self.elapsed = elapsed;
            return Ok(TimerReport::default());
        }

        let due = elapsed.as_nanos() / self.interval.as_nanos();
        let remainder = elapsed.as_nanos() % self.interval.as_nanos();
        let expirations = due.min(u128::from(self.max_expirations)) as u32;
        // remainder < interval <= Duration::MAX, so seconds fit u64. The
        // subsecond part is strictly below 1_000_000_000 and therefore fits u32.
        self.elapsed = Duration::new(
            (remainder / 1_000_000_000) as u64,
            (remainder % 1_000_000_000) as u32,
        );
        Ok(TimerReport {
            expirations,
            skipped: due - u128::from(expirations),
        })
    }

    /// The nonzero duration of each interval.
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Whether this timer expires once or repeatedly.
    pub fn mode(&self) -> TimerMode {
        self.mode
    }

    /// Maximum reported expirations per advance; always one for a one-shot timer.
    pub fn max_expirations(&self) -> u32 {
        self.max_expirations
    }

    /// Progress in the current interval; equal to the interval for a finished
    /// one-shot timer and strictly less than the interval for a repeating timer.
    pub fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Time until the next expiration, or zero for a finished one-shot timer.
    /// Immediately after a repeating expiration this is the full interval.
    pub fn remaining(&self) -> Duration {
        self.interval - self.elapsed
    }

    /// Whether this one-shot timer has expired. Always false for repeating timers.
    pub fn is_finished(&self) -> bool {
        self.mode == TimerMode::Once && self.elapsed == self.interval
    }

    /// Whether local pause currently discards supplied deltas.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Pause or resume without changing elapsed time or expiration state.
    /// Paused deltas are discarded, not saved for catch-up on resume.
    ///
    /// ```
    /// use ferravex_engine::time::SimulationTimer;
    /// use std::time::Duration;
    ///
    /// let mut timer = SimulationTimer::once(Duration::from_secs(2))?;
    /// timer.advance(Duration::from_secs(1))?;
    /// timer.set_paused(true);
    /// assert!(timer.is_paused());
    /// assert_eq!(timer.advance(Duration::MAX)?.expirations, 0);
    /// assert_eq!(timer.remaining(), Duration::from_secs(1));
    /// timer.reset(); // Clears progress, but deliberately preserves pause.
    /// assert!(timer.is_paused());
    /// timer.set_paused(false);
    /// assert_eq!(timer.advance(Duration::from_secs(2))?.expirations, 1);
    /// # Ok::<(), ferravex_engine::time::TimerError>(())
    /// ```
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Clear elapsed time and rearm a one-shot timer, preserving configuration
    /// and pause. Reset this alongside its owning game state on restart.
    pub fn reset(&mut self) {
        self.elapsed = Duration::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scale_pause_stalls_and_manual_debug_step() {
        let mut t = SimulationTime::default();
        assert!(t.set_scale(2.));
        assert!(!t.set_scale(f64::NAN));
        assert_eq!(
            t.advance(Duration::from_secs(1)).scaled_delta,
            Duration::from_millis(500)
        );
        t.set_paused(true);
        let frame = t.advance(Duration::from_millis(20));
        assert_eq!(frame.unscaled_delta, Duration::from_millis(20));
        assert_eq!(frame.scaled_delta, Duration::ZERO);
        assert!(t.step_paused(Duration::from_millis(10)));
        assert_eq!(t.total(), Duration::from_millis(510));
        assert!(!t.step_paused(Duration::from_secs(1)));
        t.set_paused(false);
        assert!(!t.step_paused(Duration::ZERO));
        t.set_scale(0.);
        assert_eq!(t.advance(Duration::MAX).scaled_delta, Duration::ZERO);
        t.reset();
        assert_eq!(t.total(), Duration::ZERO);
        assert!(SimulationTime::new(Duration::ZERO).is_none());
    }

    #[test]
    fn timer_configuration_rejects_zero_and_accepts_full_nonzero_range() {
        assert_eq!(
            SimulationTimer::once(Duration::ZERO),
            Err(TimerError::ZeroInterval)
        );
        assert_eq!(
            SimulationTimer::repeating(Duration::ZERO, 0),
            Err(TimerError::ZeroInterval)
        );
        assert_eq!(
            SimulationTimer::repeating(Duration::ZERO, 1),
            Err(TimerError::ZeroInterval)
        );
        assert_eq!(
            SimulationTimer::repeating(Duration::from_nanos(1), 0),
            Err(TimerError::ZeroCatchUpLimit)
        );
        for interval in [Duration::from_nanos(1), Duration::MAX] {
            let once = SimulationTimer::once(interval).unwrap();
            assert_eq!(once.mode(), TimerMode::Once);
            assert_eq!(once.max_expirations(), 1);
            assert_eq!(once.elapsed(), Duration::ZERO);
            assert_eq!(once.remaining(), interval);
            assert!(!once.is_finished());
            assert!(!once.is_paused());
            for limit in [1, u32::MAX] {
                let repeating = SimulationTimer::repeating(interval, limit).unwrap();
                assert_eq!(repeating.mode(), TimerMode::Repeating);
                assert_eq!(repeating.interval(), interval);
                assert_eq!(repeating.max_expirations(), limit);
                assert_eq!(repeating.remaining(), interval);
            }
        }
    }

    #[test]
    fn one_shot_expires_at_boundary_once_until_reset() {
        let interval = Duration::from_millis(100);
        let mut timer = SimulationTimer::once(interval).unwrap();
        assert_eq!(timer.advance(Duration::ZERO), Ok(TimerReport::default()));
        assert_eq!(
            timer.advance(Duration::from_millis(99)),
            Ok(TimerReport::default())
        );
        assert!(!timer.is_finished());
        assert_eq!(timer.remaining(), Duration::from_millis(1));
        assert_eq!(
            timer.advance(Duration::from_millis(1)),
            Ok(TimerReport {
                expirations: 1,
                skipped: 0,
            })
        );
        assert!(timer.is_finished());
        assert_eq!(timer.elapsed(), interval);
        assert_eq!(timer.remaining(), Duration::ZERO);
        let finished = timer.clone();
        for delta in [Duration::ZERO, interval, Duration::MAX] {
            assert_eq!(timer.advance(delta), Ok(TimerReport::default()));
            assert_eq!(timer, finished);
        }
        timer.reset();
        assert!(!timer.is_finished());
        assert_eq!(timer.elapsed(), Duration::ZERO);
        assert_eq!(timer.remaining(), interval);
        let report = timer.advance(Duration::from_millis(250)).unwrap();
        assert_eq!(report.expirations, 1);
        assert_eq!(report.skipped, 0);
        assert_eq!(timer.elapsed(), interval);
    }

    #[test]
    fn one_shot_large_overshoot_finishes_without_overflow() {
        for interval in [Duration::from_nanos(2), Duration::MAX] {
            let mut timer = SimulationTimer::once(interval).unwrap();
            timer.advance(Duration::from_nanos(1)).unwrap();
            assert_eq!(
                timer.advance(Duration::MAX),
                Ok(TimerReport {
                    expirations: 1,
                    skipped: 0,
                })
            );
            assert!(timer.is_finished());
            assert_eq!(timer.elapsed(), interval);
        }
    }

    #[test]
    fn repeating_catch_up_skips_whole_intervals_but_preserves_remainder() {
        let mut timer = SimulationTimer::repeating(Duration::from_millis(100), 2).unwrap();
        assert_eq!(
            timer.advance(Duration::from_millis(40)),
            Ok(TimerReport::default())
        );
        assert_eq!(
            timer.advance(Duration::from_millis(515)),
            Ok(TimerReport {
                expirations: 2,
                skipped: 3,
            })
        );
        assert_eq!(timer.elapsed(), Duration::from_millis(55));
        assert_eq!(timer.remaining(), Duration::from_millis(45));
        assert_eq!(timer.advance(Duration::ZERO), Ok(TimerReport::default()));
        assert_eq!(
            timer.advance(Duration::from_millis(44)),
            Ok(TimerReport::default())
        );
        assert_eq!(
            timer.advance(Duration::from_millis(1)).unwrap().expirations,
            1
        );
        assert_eq!(timer.elapsed(), Duration::ZERO);
        assert_eq!(timer.remaining(), timer.interval());
        assert!(!timer.is_finished());
        assert_eq!(
            timer.advance(Duration::from_millis(500)),
            Ok(TimerReport {
                expirations: 2,
                skipped: 3,
            })
        );
        assert_eq!(timer.elapsed(), Duration::ZERO);
    }

    #[test]
    fn repeating_does_not_drift_across_thousands_of_fixed_updates() {
        let mut timer = SimulationTimer::repeating(Duration::from_millis(100), 1).unwrap();
        let tick = Duration::from_micros(15_625); // Exactly 1/64 second.
        let mut expirations = 0;
        for _ in 0..6401 {
            let report = timer.advance(tick).unwrap();
            expirations += report.expirations;
            assert_eq!(report.skipped, 0);
        }
        assert_eq!(expirations, 1000);
        assert_eq!(timer.elapsed(), tick);
        assert_eq!(timer.remaining(), Duration::from_micros(84_375));
    }

    #[test]
    fn repeating_phase_and_total_boundaries_are_independent_of_delta_partition() {
        let deltas = [0, 1, 17, 250, 999, 1_000_000_001, 11];
        for nanos in [1, 2, 3, 7, 16, 125, 999_999_937, 1_000_000_001] {
            for limit in [1, 3, u32::MAX] {
                let interval = Duration::from_nanos(nanos);
                let mut split = SimulationTimer::repeating(interval, limit).unwrap();
                let mut whole = split.clone();
                let mut total = Duration::ZERO;
                let mut boundaries = 0_u128;
                for delta in deltas.map(Duration::from_nanos) {
                    total += delta;
                    let report = split.advance(delta).unwrap();
                    assert!(report.expirations <= limit);
                    boundaries += u128::from(report.expirations) + report.skipped;
                    assert!(split.elapsed() < interval);
                    assert_eq!(split.elapsed() + split.remaining(), interval);
                    assert_eq!(
                        boundaries * interval.as_nanos() + split.elapsed().as_nanos(),
                        total.as_nanos()
                    );
                }
                let report = whole.advance(total).unwrap();
                assert_eq!(u128::from(report.expirations) + report.skipped, boundaries);
                assert_eq!(whole.elapsed(), split.elapsed());
                assert_eq!(whole.remaining(), split.remaining());
            }
        }
    }

    #[test]
    fn nanosecond_intervals_report_huge_skipped_counts_exactly() {
        for limit in [1, u32::MAX] {
            let mut timer = SimulationTimer::repeating(Duration::from_nanos(1), limit).unwrap();
            let report = timer.advance(Duration::MAX).unwrap();
            assert_eq!(report.expirations, limit);
            assert_eq!(report.skipped, Duration::MAX.as_nanos() - u128::from(limit));
            assert!(report.skipped > u128::from(u64::MAX));
            assert_eq!(timer.elapsed(), Duration::ZERO);
            assert_eq!(timer.advance(Duration::ZERO), Ok(TimerReport::default()));
        }
    }

    #[test]
    fn largest_interval_retains_large_residual_and_expires_at_exact_boundary() {
        let mut timer = SimulationTimer::repeating(Duration::MAX, 2).unwrap();
        let remainder = Duration::MAX - Duration::from_nanos(1);
        assert_eq!(timer.advance(remainder), Ok(TimerReport::default()));
        assert_eq!(timer.elapsed(), remainder);
        assert_eq!(timer.remaining(), Duration::from_nanos(1));
        assert_eq!(
            timer.advance(Duration::from_nanos(1)).unwrap().expirations,
            1
        );
        assert_eq!(timer.elapsed(), Duration::ZERO);
        assert_eq!(timer.advance(Duration::MAX).unwrap().expirations, 1);
        assert_eq!(timer.remaining(), Duration::MAX);
    }

    #[test]
    fn repeating_overflow_is_transactional_and_timer_remains_usable() {
        for interval in [Duration::from_secs(1), Duration::MAX] {
            let mut timer = SimulationTimer::repeating(interval, 4).unwrap();
            timer.advance(Duration::from_nanos(1)).unwrap();
            let before = timer.clone();
            assert_eq!(
                timer.advance(Duration::MAX),
                Err(TimerError::ElapsedOverflow)
            );
            assert_eq!(timer, before);
            let report = timer.advance(interval - Duration::from_nanos(1)).unwrap();
            assert_eq!(report.expirations, 1);
            assert_eq!(report.skipped, 0);
            assert_eq!(timer.elapsed(), Duration::ZERO);
        }
    }

    #[test]
    fn timer_pause_discards_time_and_reset_preserves_pause_and_configuration() {
        let interval = Duration::from_millis(100);
        for mut timer in [
            SimulationTimer::once(interval).unwrap(),
            SimulationTimer::repeating(interval, 3).unwrap(),
        ] {
            timer.advance(Duration::from_millis(40)).unwrap();
            timer.set_paused(true);
            let paused = timer.clone();
            assert_eq!(timer.advance(Duration::MAX), Ok(TimerReport::default()));
            assert_eq!(timer, paused);
            timer.set_paused(false);
            assert_eq!(timer.advance(Duration::ZERO), Ok(TimerReport::default()));
            assert_eq!(
                timer
                    .advance(Duration::from_millis(60))
                    .unwrap()
                    .expirations,
                1
            );
            timer.set_paused(true);
            timer.reset();
            assert!(timer.is_paused());
            assert!(!timer.is_finished());
            assert_eq!(timer.interval(), paused.interval());
            assert_eq!(timer.mode(), paused.mode());
            assert_eq!(timer.max_expirations(), paused.max_expirations());
            assert_eq!(timer.elapsed(), Duration::ZERO);
            assert_eq!(timer.remaining(), interval);
            assert_eq!(timer.advance(interval), Ok(TimerReport::default()));
            timer.set_paused(false);
            assert_eq!(timer.advance(interval).unwrap().expirations, 1);
        }
    }

    #[test]
    fn supplied_scaled_and_unscaled_deltas_have_explicit_pause_and_scale_behavior() {
        let mut time = SimulationTime::new(Duration::from_secs(1)).unwrap();
        let interval = Duration::from_millis(100);
        let mut simulation = SimulationTimer::repeating(interval, 10).unwrap();
        let mut ui = simulation.clone();
        assert!(time.set_scale(0.5));
        let step = time.advance(interval);
        assert_eq!(
            simulation.advance(step.scaled_delta).unwrap().expirations,
            0
        );
        assert_eq!(simulation.elapsed(), Duration::from_millis(50));
        assert_eq!(ui.advance(step.unscaled_delta).unwrap().expirations, 1);

        time.set_paused(true);
        let step = time.advance(Duration::from_secs(60)); // Clamped before scaling.
        assert_eq!(
            simulation.advance(step.scaled_delta).unwrap().expirations,
            0
        );
        assert_eq!(simulation.elapsed(), Duration::from_millis(50));
        assert_eq!(ui.advance(step.unscaled_delta).unwrap().expirations, 10);

        // Manual paused stepping changes total; explicitly feed the executed
        // duration once. Calling advance while paused would still yield zero.
        let manual_step = Duration::from_millis(50);
        assert!(time.step_paused(manual_step));
        assert_eq!(simulation.advance(manual_step).unwrap().expirations, 1);

        time.set_paused(false);
        assert!(time.set_scale(0.0));
        let step = time.advance(interval);
        assert_eq!(
            simulation.advance(step.scaled_delta).unwrap().expirations,
            0
        );
        assert_eq!(ui.advance(step.unscaled_delta).unwrap().expirations, 1);
        assert!(time.set_scale(2.0));
        let step = time.advance(interval);
        assert_eq!(
            simulation.advance(step.scaled_delta).unwrap().expirations,
            2
        );
        assert_eq!(ui.advance(step.unscaled_delta).unwrap().expirations, 1);
        assert_eq!(simulation.elapsed(), Duration::ZERO);
    }
}
