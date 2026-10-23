//! Small, platform-independent building blocks. Simulation time never comes from a wall clock.
//!
//! ```
//! use ferravex_core::{FixedClock, World};
//! let mut world = World::new();
//! world.spawn((42_u32,));
//! let mut clock = FixedClock::new(64.0, 8);
//! let frame = clock.advance(1.0 / 60.0);
//! for _ in 0..frame.steps { /* update systems with frame.dt */ }
//! ```
pub use ferravex_ecs as ecs;
pub use ferravex_ecs::{Entity, World};
pub mod collision;
/// Shared lifecycle and fixed-timestep coordination for local games.
pub mod session;
pub mod telemetry;

/// A fixed simulation step budget with a fractional render interpolation factor.
#[derive(Debug, Clone, Copy)]
pub struct FrameStep {
    pub steps: u32,
    pub dt: f32,
    pub alpha: f32,
    /// Whole steps intentionally discarded to prevent a spiral of death.
    pub dropped_seconds: f64,
}

impl FrameStep {
    /// Iterate over this frame's scheduled ticks, yielding the fixed `dt` for each.
    ///
    /// This iterator allocates nothing and excludes discarded catch-up time. It owns
    /// a copy of the budget; iterating does not advance or otherwise mutate a clock.
    /// Use [`Iterator::enumerate`] when an input edge should apply only on the first tick.
    ///
    /// ```
    /// use ferravex_core::FixedClock;
    ///
    /// let mut clock = FixedClock::try_new(64.0, 8)?;
    /// let frame = clock.advance(3.0 / 64.0);
    /// let mut position = 0.0;
    /// let mut jumps = 0;
    /// for (index, dt) in frame.ticks().enumerate() {
    ///     position += 2.0 * dt;
    ///     if index == 0 { jumps += 1; } // Consume a pending jump edge once.
    /// }
    /// assert_eq!(position, 6.0 / 64.0);
    /// assert_eq!(jumps, 1);
    /// # Ok::<(), ferravex_core::FixedClockError>(())
    /// ```
    pub fn ticks(
        self,
    ) -> impl ExactSizeIterator<Item = f32> + DoubleEndedIterator + std::iter::FusedIterator {
        std::iter::repeat_n(self.dt, self.steps as usize)
    }
}

/// Context for one scheduled fixed update, produced by [`TickBatch::ticks`].
///
/// `tick` is the absolute scheduled count at this update, not an execution count.
/// IDs begin at 1 after construction/reset and wrap from `u64::MAX` to 0. Reset
/// reuses IDs; they are not globally unique or an execution/commit acknowledgment.
/// See [`FixedClock::advance_batch`] for a runnable consumer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickContext {
    /// Scheduled ID, including earlier issued work even if it was never executed.
    pub tick: u64,
    /// Zero-based position in the original batch, independent of iteration order.
    pub batch_index: u32,
    /// Fixed simulation delta in seconds, identical to the batch's [`FrameStep::dt`].
    pub dt: f32,
}

/// An allocation-free snapshot of a frame budget and its scheduled tick IDs.
///
/// Created by [`FixedClock::advance_batch`], [`session::GameSession::advance_batch`]
/// or [`session::GameSession::step_paused_batch`]. Scheduling already advanced the
/// clock by the whole budget. Iterating, skipping, copying or dropping this value
/// never advances or rolls back that clock and does not record actual execution.
/// Track executed updates in your runner; an iterator yield alone is not an update.
///
/// A snapshot keeps its original IDs even after the clock advances or resets.
/// Discard pending batches when resetting your simulation, since reset reuses IDs.
/// See [`Self::ticks`] for an example of abandoning a batch early.
#[derive(Debug, Clone, Copy)]
pub struct TickBatch {
    frame: FrameStep,
    tick_before: u64,
}

impl TickBatch {
    /// The unchanged timing budget, including zero-work and paused-step alpha.
    ///
    /// ```
    /// use ferravex_core::FixedClock;
    /// let batch = FixedClock::new(64.0, 2).advance_batch(3.5 / 64.0);
    /// assert_eq!(batch.frame().steps, 2);
    /// assert_eq!(batch.frame().alpha, 0.5);
    /// assert_eq!(batch.frame().dropped_seconds, 1.0 / 64.0);
    /// ```
    pub fn frame(self) -> FrameStep {
        self.frame
    }

    /// Iterate scheduled contexts without allocation or borrowing the clock.
    ///
    /// Each ID is `tick_before.wrapping_add(batch_index + 1)`. Discarded catch-up
    /// time consumes no IDs. Empty budgets yield nothing. Every call produces the
    /// same contexts, so executing a batch twice would repeat work with the same IDs.
    /// Reverse/skip iteration preserves indices from the original forward order.
    /// `nth`/`nth_back` skip in constant time, even for a maximum-sized budget.
    ///
    /// ```
    /// use ferravex_core::FixedClock;
    /// let mut clock = FixedClock::new(64.0, 8);
    /// let batch = clock.advance_batch(3.0 / 64.0);
    /// let mut executed = 0;
    /// for context in batch.ticks().take(1) {
    ///     assert_eq!(context.tick, 1);
    ///     assert_eq!(context.batch_index, 0);
    ///     assert_eq!(context.dt, 1.0 / 64.0);
    ///     executed += 1; // Invoke the simulation here, then stop this batch.
    /// }
    /// assert_eq!(executed, 1);
    /// assert_eq!(clock.tick(), 3); // Includes the two unexecuted scheduled IDs.
    /// assert_eq!(clock.advance_batch(1.0 / 64.0).ticks().next().unwrap().tick, 4);
    /// ```
    pub fn ticks(
        self,
    ) -> impl ExactSizeIterator<Item = TickContext> + DoubleEndedIterator + std::iter::FusedIterator
    {
        TickContexts {
            indices: 0..self.frame.steps as usize,
            tick_before: self.tick_before,
            dt: self.frame.dt,
        }
    }
}

// Map's default skipping can visit every skipped element. Keep range arithmetic
// here so inspecting a large budget with nth/nth_back never executes that work.
struct TickContexts {
    indices: std::ops::Range<usize>,
    tick_before: u64,
    dt: f32,
}

impl TickContexts {
    fn context(&self, index: usize) -> TickContext {
        TickContext {
            tick: self.tick_before.wrapping_add(index as u64 + 1),
            batch_index: index as u32,
            dt: self.dt,
        }
    }
}

impl Iterator for TickContexts {
    type Item = TickContext;

    fn next(&mut self) -> Option<Self::Item> {
        self.indices.next().map(|index| self.context(index))
    }

    fn nth(&mut self, n: usize) -> Option<Self::Item> {
        self.indices.nth(n).map(|index| self.context(index))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.indices.size_hint()
    }

    fn count(self) -> usize {
        self.indices.len()
    }

    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }
}

impl DoubleEndedIterator for TickContexts {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.indices.next_back().map(|index| self.context(index))
    }

    fn nth_back(&mut self, n: usize) -> Option<Self::Item> {
        self.indices.nth_back(n).map(|index| self.context(index))
    }
}

impl ExactSizeIterator for TickContexts {}
impl std::iter::FusedIterator for TickContexts {}

/// Invalid configuration supplied to [`FixedClock::try_new`].
///
/// ```
/// use ferravex_core::{FixedClock, FixedClockError};
///
/// let error = FixedClock::try_new(60.0, 0).unwrap_err();
/// assert_eq!(error, FixedClockError::ZeroCatchUpBudget);
/// assert!(error.to_string().contains("at least 1"));
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FixedClockError {
    /// The rate must be finite and within the inclusive range `1..=1000` Hz.
    InvalidTickRate { hz: f64 },
    /// At least one simulation step must be permitted per frame.
    ZeroCatchUpBudget,
}

impl std::fmt::Display for FixedClockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTickRate { hz } => write!(
                f,
                "tick rate must be in 1..=1000 Hz and finite; got {hz} Hz"
            ),
            Self::ZeroCatchUpBudget => write!(
                f,
                "catch-up budget must be nonzero; set max_steps to at least 1"
            ),
        }
    }
}

impl std::error::Error for FixedClockError {}

/// Frame-rate independent simulation with bounded catch-up work.
///
/// Supply elapsed seconds from your platform. Non-finite/negative durations are ignored.
/// Over-budget whole steps are discarded; fractional remainder is retained for interpolation.
#[derive(Debug, Clone)]
pub struct FixedClock {
    step: f64,
    accumulator: f64,
    max_steps: u32,
    tick: u64,
}

impl FixedClock {
    /// Create a clock with the given tick rate and maximum steps per frame.
    ///
    /// # Panics
    ///
    /// Panics unless `hz` is finite and in `1..=1000`, and `max_steps` is nonzero.
    /// Use [`Self::try_new`] to report invalid user configuration without panicking.
    pub fn new(hz: f64, max_steps: u32) -> Self {
        Self::try_new(hz, max_steps).unwrap_or_else(|error| panic!("{error}"))
    }

    /// Create a clock, returning an actionable error for invalid configuration.
    ///
    /// `hz` must be finite and in `1..=1000`; `max_steps` must be at least one.
    /// The tick rate is checked first if both values are invalid. Valid inputs have
    /// exactly the same timing behavior as [`Self::new`].
    ///
    /// ```
    /// use ferravex_core::FixedClock;
    ///
    /// let mut clock = FixedClock::try_new(60.0, 8)?;
    /// assert_eq!(clock.advance(1.0 / 30.0).ticks().count(), 2);
    /// # Ok::<(), ferravex_core::FixedClockError>(())
    /// ```
    pub fn try_new(hz: f64, max_steps: u32) -> Result<Self, FixedClockError> {
        if !hz.is_finite() || !(1.0..=1000.0).contains(&hz) {
            return Err(FixedClockError::InvalidTickRate { hz });
        }
        if max_steps == 0 {
            return Err(FixedClockError::ZeroCatchUpBudget);
        }
        Ok(Self {
            step: 1.0 / hz,
            accumulator: 0.0,
            max_steps,
            tick: 0,
        })
    }

    /// Schedule fixed ticks from elapsed platform time, preserving fractional time.
    ///
    /// Negative and non-finite durations are ignored. Finite durations are capped
    /// at one hour before accumulation; time above that cap is ignored and is not
    /// included in [`FrameStep::dropped_seconds`]. Within the cap, whole ticks beyond
    /// the catch-up budget are discarded and reported in `dropped_seconds`.
    pub fn advance(&mut self, frame_seconds: f64) -> FrameStep {
        // Clamp absurd durations before addition to keep all arithmetic finite.
        let elapsed = if frame_seconds.is_finite() {
            frame_seconds.clamp(0.0, 3600.0)
        } else {
            0.0
        };
        self.accumulator += elapsed;
        let available = ((self.accumulator + self.step * 1e-9) / self.step).floor() as u64;
        let steps = available.min(self.max_steps as u64) as u32;
        self.accumulator = (self.accumulator - available as f64 * self.step).max(0.0);
        self.tick = self.tick.wrapping_add(steps as u64);
        FrameStep {
            steps,
            dt: self.step as f32,
            alpha: (self.accumulator / self.step).clamp(0.0, 1.0) as f32,
            dropped_seconds: (available - steps as u64) as f64 * self.step,
        }
    }

    /// Schedule the same work as [`Self::advance`], retaining per-update tick IDs.
    ///
    /// IDs start at 1, wrap through 0 and restart at 1 after [`Self::reset`]. They
    /// count scheduled work, including any batch tail the caller does not execute.
    /// No extra steps or time are consumed by constructing/iterating the batch.
    ///
    /// ```
    /// use ferravex_core::{FixedClock, TickContext};
    /// let mut clock = FixedClock::new(64.0, 4);
    /// assert_eq!(clock.advance_batch(0.5 / 64.0).ticks().len(), 0);
    /// let batch = clock.advance_batch(2.0 / 64.0);
    /// let mut position = 0.0;
    /// for TickContext { tick, batch_index, dt } in batch.ticks() {
    ///     assert_eq!(tick, u64::from(batch_index) + 1);
    ///     position += 2.0 * dt;
    /// }
    /// assert_eq!(position, 4.0 / 64.0);
    /// assert_eq!(batch.frame().alpha, 0.5);
    /// assert_eq!(clock.tick(), 2);
    /// ```
    pub fn advance_batch(&mut self, frame_seconds: f64) -> TickBatch {
        let tick_before = self.tick;
        let frame = self.advance(frame_seconds);
        TickBatch { frame, tick_before }
    }

    /// Number of scheduled ticks since construction or reset, wrapping at `u64::MAX`.
    /// Discarded catch-up ticks are not counted.
    /// The whole batch is counted before execution; use [`Self::advance_batch`]
    /// for individual IDs instead of reading this batch-end count inside a loop.
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn dt(&self) -> f32 {
        self.step as f32
    }
    pub fn reset(&mut self) {
        self.accumulator = 0.0;
        self.tick = 0;
    }
}

/// Allocation-free statistics over the most recent `N` accepted frame times.
///
/// Record elapsed **seconds**; diagnostics return **milliseconds**. During warm-up,
/// only recorded samples contribute. Zero durations are valid, and `N == 0`
/// creates a disabled, always-empty window. Invalid samples never evict history.
/// Recording takes constant time; queries scan at most `N` samples. Percentile
/// queries also copy the fixed array onto the stack and sort the populated part.
///
/// ```
/// use ferravex_core::FrameStats;
///
/// let mut stats = FrameStats::<120>::default();
/// stats.record(0.010);
/// stats.record(0.030);
/// assert_eq!(stats.sample_count(), 2);
/// assert_eq!(stats.capacity(), 120);
/// assert_eq!(stats.mean_ms(), 20.0);
/// assert_eq!(stats.max_ms(), 30.0);
/// assert_eq!(stats.percentile_ms(95.0), Some(30.0));
/// assert_eq!(stats.jitter_ms(), 10.0);
/// ```
pub struct FrameStats<const N: usize = 120> {
    samples: [f32; N],
    cursor: usize,
    len: usize,
}
impl<const N: usize> Default for FrameStats<N> {
    fn default() -> Self {
        Self {
            samples: [0.0; N],
            cursor: 0,
            len: 0,
        }
    }
}
impl<const N: usize> FrameStats<N> {
    /// Record elapsed seconds, silently ignoring invalid or disabled samples.
    ///
    /// See [`Self::try_record`] to learn whether a sample was accepted. Neither
    /// method allocates, including during warm-up and when the window wraps.
    pub fn record(&mut self, seconds: f32) {
        self.try_record(seconds);
    }

    /// Record elapsed seconds, returning whether the sample was accepted.
    ///
    /// Returns `false` without changing the window if `N == 0`, `seconds` is
    /// negative or non-finite, or converting to `f32` milliseconds overflows.
    /// Zero (including negative zero) is accepted. When full, the oldest accepted
    /// sample is replaced. This operation takes constant time and never allocates.
    ///
    /// ```
    /// use ferravex_core::FrameStats;
    ///
    /// let mut stats = FrameStats::<2>::default();
    /// assert!(stats.try_record(0.016));
    /// assert!(!stats.try_record(f32::NAN));
    /// assert!(!stats.try_record(f32::MAX)); // Milliseconds would overflow.
    /// assert_eq!(stats.sample_count(), 1);
    /// assert!(!FrameStats::<0>::default().try_record(0.016));
    /// ```
    pub fn try_record(&mut self, seconds: f32) -> bool {
        let milliseconds = seconds * 1000.0;
        if N == 0 || seconds < 0.0 || !milliseconds.is_finite() {
            return false;
        }
        self.samples[self.cursor] = milliseconds;
        self.cursor = (self.cursor + 1) % N;
        self.len = (self.len + 1).min(N);
        true
    }

    /// Number of accepted samples currently retained, at most [`Self::capacity`].
    ///
    /// This is a window count, not a lifetime counter. Compare it with capacity
    /// to determine whether a nonzero window has completed warm-up.
    ///
    /// ```
    /// use ferravex_core::FrameStats;
    /// let mut stats = FrameStats::<2>::default();
    /// assert_eq!(stats.sample_count(), 0);
    /// for _ in 0..3 { stats.record(0.016); }
    /// assert_eq!(stats.sample_count(), 2);
    /// ```
    pub const fn sample_count(&self) -> usize {
        self.len
    }

    /// Maximum retained sample count; zero means recording is disabled.
    ///
    /// ```
    /// use ferravex_core::FrameStats;
    /// assert_eq!(FrameStats::<90>::default().capacity(), 90);
    /// assert_eq!(FrameStats::<0>::default().capacity(), 0);
    /// ```
    pub const fn capacity(&self) -> usize {
        N
    }

    /// Forget all samples and restart warm-up without allocating.
    ///
    /// ```
    /// use ferravex_core::FrameStats;
    /// let mut stats = FrameStats::<120>::default();
    /// stats.record(0.016);
    /// stats.clear();
    /// assert_eq!(stats.sample_count(), 0);
    /// assert_eq!(stats.mean_ms(), 0.0);
    /// assert_eq!(stats.percentile_ms(95.0), None);
    /// ```
    pub fn clear(&mut self) {
        self.cursor = 0;
        self.len = 0;
    }

    /// Arithmetic mean in milliseconds, or zero when empty.
    ///
    /// Accumulates in `f64` so summing large accepted samples cannot overflow.
    pub fn mean_ms(&self) -> f32 {
        self.mean_ms_f64() as f32
    }

    fn mean_ms_f64(&self) -> f64 {
        if self.len == 0 {
            return 0.0;
        }
        self.samples[..self.len]
            .iter()
            .map(|&sample| f64::from(sample))
            .sum::<f64>()
            / self.len as f64
    }

    /// Largest retained frame time in milliseconds, or zero when empty.
    pub fn max_ms(&self) -> f32 {
        self.samples[..self.len].iter().copied().fold(0.0, f32::max)
    }

    /// Nearest-rank percentile in milliseconds, using a percentage in `0..=100`.
    ///
    /// Returns `None` for an empty window or a non-finite/out-of-range percentage.
    /// For sorted samples, selects rank `ceil(percentile * sample_count / 100)`
    /// (one-based), with percentile zero selecting the minimum. Percentile 100
    /// selects the maximum. Small windows can therefore share several percentiles;
    /// values are never interpolated and unused warm-up slots do not contribute.
    ///
    /// Does not change recording order or allocate on the heap. A query uses
    /// `N * size_of::<f32>()` bytes of stack scratch space and sorts the `m`
    /// retained samples in `O(m log m)` time. Choose `N` to fit your stack budget.
    ///
    /// ```
    /// use ferravex_core::FrameStats;
    /// let mut stats = FrameStats::<120>::default();
    /// assert_eq!(stats.percentile_ms(99.0), None);
    /// for seconds in [0.040, 0.010, 0.030, 0.020] { stats.record(seconds); }
    /// assert_eq!(stats.percentile_ms(0.0), Some(10.0));
    /// assert_eq!(stats.percentile_ms(50.0), Some(20.0));
    /// assert_eq!(stats.percentile_ms(95.0), Some(40.0));
    /// assert_eq!(stats.percentile_ms(101.0), None);
    /// ```
    pub fn percentile_ms(&self, percentile: f32) -> Option<f32> {
        if self.len == 0 || !(0.0..=100.0).contains(&percentile) {
            return None;
        }
        let mut sorted = self.samples;
        sorted[..self.len].sort_unstable_by(f32::total_cmp);
        let rank = (f64::from(percentile) * self.len as f64 / 100.0).ceil() as usize;
        let index = rank.saturating_sub(1).min(self.len - 1);
        Some(sorted[index])
    }

    /// Population standard deviation of retained frame times, in milliseconds.
    ///
    /// Jitter is `sqrt(sum((sample - mean)^2) / sample_count)`, measuring spread
    /// around the mean, independent of sample order. It is not a difference between
    /// consecutive frames. Empty, single-sample and constant windows return zero.
    /// Uses two passes with `f64` arithmetic to avoid overflow and cancellation
    /// from subtracting squared means. Never allocates.
    ///
    /// ```
    /// use ferravex_core::FrameStats;
    /// let mut stats = FrameStats::<120>::default();
    /// assert_eq!(stats.jitter_ms(), 0.0);
    /// stats.record(0.010);
    /// assert_eq!(stats.jitter_ms(), 0.0);
    /// stats.record(0.030);
    /// assert_eq!(stats.jitter_ms(), 10.0);
    /// ```
    pub fn jitter_ms(&self) -> f32 {
        if self.len < 2 {
            return 0.0;
        }
        let mean = self.mean_ms_f64();
        let squared_deviations = self.samples[..self.len]
            .iter()
            .map(|&sample| {
                let deviation = f64::from(sample) - mean;
                deviation * deviation
            })
            .sum::<f64>();
        (squared_deviations / self.len as f64).sqrt() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contextual_batches_preserve_legacy_timing_and_issued_ids() {
        for hz in [1.0, 59.94, 64.0, 1000.0] {
            for max_steps in [1, 4, u32::MAX] {
                let mut clock = FixedClock::new(hz, max_steps);
                let mut legacy = clock.clone();
                for elapsed in [
                    0.0,
                    0.5 / hz,
                    0.5 / hz,
                    3.0 / hz,
                    10.25 / hz,
                    f64::NAN,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    -1.0,
                    f64::MAX,
                ] {
                    let before = clock.tick();
                    let expected = legacy.advance(elapsed);
                    let batch = clock.advance_batch(elapsed);
                    let frame = batch.frame();
                    assert_eq!(frame.steps, expected.steps);
                    assert_eq!(frame.dt, expected.dt);
                    assert_eq!(frame.alpha, expected.alpha);
                    assert_eq!(frame.dropped_seconds, expected.dropped_seconds);
                    assert_eq!(clock.tick(), legacy.tick());
                    let mut ticks = batch.ticks();
                    assert_eq!(ticks.len(), frame.steps as usize);
                    if frame.steps == 0 {
                        assert_eq!(ticks.next(), None);
                        assert_eq!(ticks.next_back(), None);
                    } else {
                        assert_eq!(
                            ticks.next(),
                            Some(TickContext {
                                tick: before.wrapping_add(1),
                                batch_index: 0,
                                dt: frame.dt,
                            })
                        );
                        if frame.steps > 1 {
                            assert_eq!(
                                ticks.next_back(),
                                Some(TickContext {
                                    tick: clock.tick(),
                                    batch_index: frame.steps - 1,
                                    dt: frame.dt,
                                })
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn contextual_batches_wrap_skip_and_retain_snapshot_ids_after_reset() {
        let mut clock = FixedClock::new(64.0, 4);
        clock.tick = u64::MAX - 1;
        let batch = clock.advance_batch(10.5 / 64.0);
        assert_eq!(batch.frame().dropped_seconds, 6.0 / 64.0);
        assert_eq!(clock.tick(), 2);
        let expected = [u64::MAX, 0, 1, 2];
        for ((index, id), context) in expected.into_iter().enumerate().zip(batch.ticks()) {
            assert_eq!(context.tick, id);
            assert_eq!(context.batch_index, index as u32);
            assert_eq!(context.dt, 1.0 / 64.0);
        }
        let mut ticks = batch.ticks();
        assert_eq!(ticks.size_hint(), (4, Some(4)));
        assert_eq!(ticks.next_back().unwrap().batch_index, 3);
        assert_eq!(ticks.nth(1).unwrap().tick, 0);
        assert_eq!(ticks.len(), 1);
        assert_eq!(ticks.next().unwrap().tick, 1);
        assert_eq!(ticks.next(), None);
        assert_eq!(ticks.next_back(), None);
        assert_eq!(ticks.next(), None);
        assert_eq!(
            clock.advance_batch(0.5 / 64.0).ticks().next().unwrap().tick,
            3
        );
        clock.reset();
        assert_eq!(clock.advance_batch(0.5 / 64.0).ticks().len(), 0);
        assert_eq!(
            clock.advance_batch(0.5 / 64.0).ticks().next().unwrap().tick,
            1
        );
        assert_eq!(
            batch
                .ticks()
                .map(|context| context.tick)
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn contextual_iterator_supports_full_u32_budget_without_materializing_it() {
        let frame = FrameStep {
            steps: u32::MAX,
            dt: 0.125,
            alpha: 0.0,
            dropped_seconds: 0.0,
        };
        let batch = TickBatch {
            frame,
            tick_before: u64::MAX,
        };
        let mut ticks = batch.ticks();
        assert_eq!(ticks.len(), u32::MAX as usize);
        let last = ticks.nth((u32::MAX - 1) as usize).unwrap();
        assert_eq!(last.batch_index, u32::MAX - 1);
        assert_eq!(last.tick, u64::from(u32::MAX - 1));
        assert_eq!(ticks.next(), None);
        assert_eq!(ticks.next_back(), None);
        let mut reverse = batch.ticks();
        assert_eq!(reverse.nth_back((u32::MAX - 1) as usize).unwrap().tick, 0);
        assert_eq!(reverse.len(), 0);
        assert_eq!(reverse.next(), None);
        let mut skipped = batch.ticks();
        assert_eq!(skipped.nth(usize::MAX), None);
        assert_eq!(skipped.next_back(), None);
    }

    #[test]
    fn abandoned_batch_tails_and_legacy_calls_still_consume_scheduled_ids() {
        let mut clock = FixedClock::new(64.0, 4);
        assert_eq!(clock.advance(2.0 / 64.0).steps, 2);
        let batch = clock.advance_batch(4.0 / 64.0);
        let mut executed = 0;
        for context in batch.ticks() {
            executed += 1;
            assert_eq!(context.tick, 3);
            if context.batch_index == 0 {
                break;
            }
        }
        assert_eq!(executed, 1);
        assert_eq!(clock.tick(), 6);
        let unused = clock.advance_batch(2.0 / 64.0);
        assert_eq!(unused.frame().steps, 2);
        assert_eq!(
            clock.advance_batch(1.0 / 64.0).ticks().next().unwrap().tick,
            9
        );
    }

    #[test]
    fn checked_clock_rejects_invalid_configuration_with_actionable_errors() {
        for hz in [
            0.0,
            -0.0,
            -60.0,
            1.0 - f64::EPSILON,
            1000.001,
            f64::MAX,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            let error = FixedClock::try_new(hz, 8).unwrap_err();
            match error {
                FixedClockError::InvalidTickRate { hz: actual } => {
                    assert_eq!(actual.to_bits(), hz.to_bits());
                }
                _ => panic!("expected an invalid tick rate"),
            }
            let message = error.to_string();
            assert!(message.contains("1..=1000 Hz"));
            assert!(message.contains("finite"));
            assert!(message.contains(&hz.to_string()));
            let source: &dyn std::error::Error = &error;
            assert!(source.source().is_none());
        }

        let error = FixedClock::try_new(60.0, 0).unwrap_err();
        assert_eq!(error, FixedClockError::ZeroCatchUpBudget);
        assert!(error.to_string().contains("max_steps to at least 1"));
        assert!(matches!(
            FixedClock::try_new(0.0, 0),
            Err(FixedClockError::InvalidTickRate { .. })
        ));
    }

    #[test]
    fn checked_clock_accepts_rate_and_budget_boundaries() {
        for hz in [1.0, 59.94, 1000.0] {
            for max_steps in [1, u32::MAX] {
                let mut clock = FixedClock::try_new(hz, max_steps).unwrap();
                assert_eq!(clock.tick(), 0);
                assert_eq!(clock.dt(), (1.0 / hz) as f32);
                let frame = clock.advance(1.0 / hz);
                assert_eq!(frame.steps, 1);
                assert_eq!(frame.alpha, 0.0);
                assert_eq!(frame.dropped_seconds, 0.0);
                assert_eq!(clock.tick(), 1);
            }
        }
    }

    #[test]
    #[should_panic(expected = "tick rate must be in 1..=1000 Hz")]
    fn original_constructor_still_panics_on_invalid_rate() {
        FixedClock::new(f64::NAN, 8);
    }

    #[test]
    #[should_panic(expected = "catch-up budget must be nonzero")]
    fn original_constructor_still_panics_on_zero_budget() {
        FixedClock::new(60.0, 0);
    }

    #[test]
    fn frame_rate_independent() {
        for hz in [30, 60, 64, 144] {
            for fps in [30, 60, 144, 240] {
                let mut clock = FixedClock::try_new(hz as f64, 8).unwrap();
                let mut simulated_seconds = 0.0_f64;
                for _ in 0..fps * 10 {
                    let frame = clock.advance(1.0 / fps as f64);
                    assert_eq!(frame.dropped_seconds, 0.0);
                    assert!((0.0..=1.0).contains(&frame.alpha));
                    for dt in frame.ticks() {
                        simulated_seconds += dt as f64;
                    }
                }
                assert_eq!(clock.tick(), hz * 10, "{hz} Hz at {fps} fps");
                assert!((simulated_seconds - 10.0).abs() < 0.000001);
            }
        }
    }

    #[test]
    fn catchup_is_bounded_and_fraction_retained() {
        let mut clock = FixedClock::new(64.0, 4);
        let f = clock.advance(10.5 / 64.0);
        assert_eq!(f.steps, 4);
        assert_eq!(f.ticks().count(), 4);
        assert!((f.alpha - 0.5).abs() < 0.0001);
        assert!((f.dropped_seconds - 6.0 / 64.0).abs() < 0.0001);
        assert_eq!(clock.advance(0.5 / 64.0).steps, 1);
        assert_eq!(clock.tick(), 5);
        let idle = clock.advance(0.0);
        assert_eq!(idle.steps, 0);
        assert_eq!(idle.alpha, 0.0);
        assert_eq!(idle.dropped_seconds, 0.0);
    }

    #[test]
    fn invalid_durations_do_not_poison_clock() {
        let mut c = FixedClock::new(64.0, 8);
        assert_eq!(c.advance(0.25 / 64.0).steps, 0);
        for dt in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -5.0] {
            let frame = c.advance(dt);
            assert_eq!(frame.steps, 0);
            assert_eq!(frame.alpha, 0.25);
            assert_eq!(frame.dropped_seconds, 0.0);
            assert_eq!(c.tick(), 0);
        }
        assert_eq!(c.advance(0.75 / 64.0).steps, 1);
    }

    #[test]
    fn ticks_preserve_struct_literals_and_iterator_bounds() {
        let frame = FrameStep {
            steps: 3,
            dt: 0.125,
            alpha: 0.75,
            dropped_seconds: 8.0,
        };
        let mut ticks = frame.ticks();
        assert_eq!(ticks.size_hint(), (3, Some(3)));
        assert_eq!(ticks.next(), Some(0.125));
        assert_eq!(ticks.len(), 2);
        assert_eq!(ticks.next_back(), Some(0.125));
        assert_eq!(ticks.next(), Some(0.125));
        assert_eq!(ticks.len(), 0);
        assert_eq!(ticks.next(), None);
        assert_eq!(ticks.next_back(), None);
        assert_eq!(ticks.next(), None);
        assert_eq!(frame.ticks().sum::<f32>(), 0.375);

        let mut empty = FrameStep { steps: 0, ..frame }.ticks();
        assert_eq!(empty.len(), 0);
        assert_eq!(empty.next(), None);
        assert_eq!(empty.next_back(), None);

        let mut largest = FrameStep {
            steps: u32::MAX,
            ..frame
        }
        .ticks();
        assert_eq!(largest.len(), u32::MAX as usize);
        assert_eq!(largest.nth((u32::MAX - 1) as usize), Some(frame.dt));
        assert_eq!(largest.next(), None);
    }

    #[test]
    fn finite_duration_cap_prevents_overflow_and_retains_fraction() {
        for max_steps in [1, u32::MAX] {
            let mut clock = FixedClock::try_new(1000.0, max_steps).unwrap();
            clock.advance(0.00025);
            let frame = clock.advance(f64::MAX);
            let expected_steps = max_steps.min(3_600_000);
            assert_eq!(frame.steps, expected_steps);
            assert_eq!(frame.ticks().len(), expected_steps as usize);
            assert_eq!(clock.tick(), expected_steps as u64);
            assert!(frame.dt.is_finite());
            assert!((frame.alpha - 0.25).abs() < 0.00001);
            assert!(
                (frame.dropped_seconds - (3_600_000 - expected_steps) as f64 / 1000.0).abs()
                    < 0.000001
            );
            assert_eq!(clock.advance(0.0).steps, 0);
            assert_eq!(clock.advance(0.00075).steps, 1);
        }
    }

    #[test]
    fn tick_counter_wraps_only_scheduled_ticks() {
        let mut clock = FixedClock::new(64.0, 4);
        clock.tick = u64::MAX - 1;
        let frame = clock.advance(10.5 / 64.0);
        assert_eq!(frame.steps, 4);
        assert_eq!(frame.dropped_seconds, 6.0 / 64.0);
        assert_eq!(clock.tick(), 2);
        assert_eq!(clock.advance(0.5 / 64.0).steps, 1);
        assert_eq!(clock.tick(), 3);
    }

    #[test]
    fn reset_clears_tick_and_fraction_without_changing_configuration() {
        let mut clock = FixedClock::try_new(64.0, 2).unwrap();
        assert_eq!(clock.advance(2.75 / 64.0).steps, 2);
        clock.reset();
        assert_eq!(clock.tick(), 0);
        assert_eq!(clock.dt(), 1.0 / 64.0);
        let frame = clock.advance(0.25 / 64.0);
        assert_eq!(frame.steps, 0);
        assert_eq!(frame.alpha, 0.25);
        assert_eq!(frame.dropped_seconds, 0.0);
        assert_eq!(clock.advance(4.0 / 64.0).steps, 2);
    }

    #[test]
    fn stats_wrap_and_ignore_nan() {
        let mut s = FrameStats::<2>::default();
        for dt in [0.001, 0.003, 0.005, f32::NAN] {
            s.record(dt);
        }
        assert!((s.mean_ms() - 4.0).abs() < 0.001);
        assert_eq!(s.max_ms(), 5.0);
    }
}
