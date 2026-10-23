//! Bounded authority history and clock estimates; rendering and transport are independent.
//!
//! [`History::bracket_precise`] keeps tick identity as an integer. Its optional
//! freshness cutoff is caller data, independent of the requested presentation
//! time. Here a host supplies both times in the same nonwrapping session epoch:
//!
//! ```
//! use ferravex_net::history::{History, HistoryBracket, HistoryEndpointPolicy};
//! use ferravex_net::clock::{ClientClock, TickUnwrapper};
//!
//! #[derive(Debug, PartialEq)]
//! struct Pose { x: f64 }
//! let base = (1_u64 << 53) + 10;
//! let mut history = History::new(2);
//! assert!(history.push(base, Pose { x: 2.0 }));
//! assert!(history.push(base + 1, Pose { x: 6.0 }));
//! let now_tick = base + 2; // Supplied authority time, not a wall clock read.
//! let minimum_newest_tick = Some(now_tick.saturating_sub(2));
//! let query = |tick, fraction| history.bracket_precise(
//!     tick, fraction, HistoryEndpointPolicy::Reject, minimum_newest_tick);
//! assert!(matches!(query(base + 1, 0.0)?,
//!     HistoryBracket::Exact { sample: (tick, pose) }
//!         if tick == base + 1 && pose.x == 6.0));
//! let presented = match query(base, 0.25)? {
//!     HistoryBracket::Interpolated { before: (_, a), after: (_, b), alpha } => {
//!         Pose { x: a.x + (b.x - a.x) * alpha }
//!     }
//!     other => panic!("expected retained neighbors, got {other:?}"),
//! };
//! assert_eq!(presented, Pose { x: 3.0 });
//! assert!(history.push(base + 2, Pose { x: 10.0 })); // Evicts base.
//! assert!(matches!(history.bracket_precise(base, 0.5,
//!     HistoryEndpointPolicy::Reject, None)?, HistoryBracket::BeforeOldest { .. }));
//! assert!(matches!(history.bracket_precise(base, 0.5,
//!     HistoryEndpointPolicy::Clamp, None)?,
//!     HistoryBracket::Clamped { sample: (tick, _) } if tick == base + 1));
//! let later_now_tick = base + 6;
//! assert!(matches!(history.bracket_precise(base + 1, 0.0,
//!     HistoryEndpointPolicy::Clamp, Some(later_now_tick - 2))?,
//!     HistoryBracket::Stale { newest: (tick, _), .. } if tick == base + 2));
//!
//! // At reconnect, first exclude old-session traffic at the transport boundary.
//! // Clear retained poses and reset the separate clock/tick epochs together.
//! let mut clock = ClientClock::default();
//! let mut ticks = TickUnwrapper::default();
//! ticks.observe(u32::MAX);
//! history.clear();
//! clock.reset();
//! ticks.reset();
//! assert_eq!(ticks.observe(1), Some(1));
//! assert!(history.push(1, Pose { x: 20.0 })); // Lower new-session ticks now fit.
//! assert_eq!(history.len(), 1);
//! # Ok::<(), ferravex_net::history::HistoryQueryError>(())
//! ```
use std::collections::VecDeque;

/// Treatment of valid queries outside retained samples. Neither policy extrapolates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum HistoryEndpointPolicy {
    /// Report [`HistoryBracket::BeforeOldest`] or [`HistoryBracket::AfterNewest`].
    #[default]
    Reject,
    /// Explicitly select the nearest retained sample as [`HistoryBracket::Clamped`].
    Clamp,
}

/// Placement within actual retained samples; states are borrowed without cloning.
///
/// See the [module recipe](self) for exact selection, pose interpolation, eviction,
/// supplied-time staleness and reconnect. Only `Interpolated` supplies a weight;
/// neither a rounded endpoint weight nor a clamped result implies exact identity.
#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub enum HistoryBracket<'a, T> {
    /// No retained samples. No freshness claim can be made.
    Empty,
    /// The query's integer tick matches this sample and its normalized fraction is zero.
    Exact { sample: (u64, &'a T) },
    /// Consecutive retained samples strictly surround the query (including sparse gaps).
    Interpolated {
        before: (u64, &'a T),
        after: (u64, &'a T),
        /// Approximate weight in `[0, 1]`, computed from local integer differences.
        ///
        /// `f64` rounding may produce 0 or 1 for a strictly interior query over
        /// large gaps. Exact tick identity is determined before this calculation.
        alpha: f64,
    },
    /// Query is earlier than all retained samples; no state was selected.
    BeforeOldest { oldest: (u64, &'a T) },
    /// Query is later than all retained samples; no state was selected.
    AfterNewest { newest: (u64, &'a T) },
    /// Out-of-range query explicitly selected the nearest endpoint, without extrapolation.
    Clamped { sample: (u64, &'a T) },
    /// The newest retained tick is strictly below the caller's inclusive cutoff.
    ///
    /// This rejects the entire history before exact/range selection. It says
    /// nothing about elapsed wall time, the query's age, or why updates stopped.
    Stale {
        newest: (u64, &'a T),
        minimum_newest_tick: u64,
    },
}

/// Invalid input to [`History::bracket_precise`], validated even for empty history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HistoryQueryError {
    /// Fraction is nonfinite or outside `[0, 1]`.
    InvalidFraction,
    /// Normalizing `(u64::MAX, 1.0)` would overflow the integer tick.
    TickOverflow,
}

impl std::fmt::Display for HistoryQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidFraction => "history fraction must be finite and in [0, 1]",
            Self::TickOverflow => "history fraction carries beyond the maximum tick",
        })
    }
}

impl std::error::Error for HistoryQueryError {}

#[derive(Clone, Debug)]
pub struct History<T> {
    samples: VecDeque<(u64, T)>,
    capacity: usize,
}
impl<T> History<T> {
    pub fn new(capacity: usize) -> Self {
        assert!((1..=4096).contains(&capacity));
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }
    pub fn push(&mut self, tick: u64, state: T) -> bool {
        if self.samples.back().is_some_and(|(last, _)| tick <= *last) {
            return false;
        }
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
        }
        self.samples.push_back((tick, state));
        true
    }
    /// No extrapolation outside retained history: old/untrusted shot times are rejected.
    pub fn bracket(&self, tick: f64) -> Option<(&T, &T, f32)> {
        if !tick.is_finite()
            || tick < self.samples.front()?.0 as f64
            || tick > self.samples.back()?.0 as f64
        {
            return None;
        }
        for i in 0..self.samples.len() {
            let (a, left) = &self.samples[i];
            if tick == *a as f64 {
                return Some((left, left, 0.0));
            }
            if let Some((b, right)) = self.samples.get(i + 1)
                && tick < *b as f64
            {
                return Some((left, right, ((tick - *a as f64) / (*b - *a) as f64) as f32));
            }
        }
        None
    }
    /// Locate `tick + fraction` while comparing integer tick components exactly.
    ///
    /// `fraction` must be finite and in `[0, 1]`. Exactly 1 carries to the next
    /// integer tick; negative zero is zero. `(u64::MAX, 1)` is an overflow error,
    /// while `(u64::MAX, fraction < 1)` is valid: a positive fraction there is
    /// after every possible sample. Invalid inputs are never silently clamped.
    ///
    /// `minimum_newest_tick` is an optional inclusive freshness cutoff in the
    /// same nonwrapping authority/session tick domain as the stored samples. A
    /// host with supplied authority `now_tick` and an integer maximum age can
    /// pass `Some(now_tick.saturating_sub(max_age_ticks))`. `None` disables this
    /// check. The target tick is **not** assumed to be current server time. A
    /// cutoff cannot detect mismatched sessions or clock domains.
    ///
    /// Precedence is input errors, empty, stale, then exact/interpolated/range
    /// placement. Freshness equality passes. The endpoint policy only affects
    /// out-of-range placement; it cannot override stale or invalid input.
    ///
    /// Lookup takes `O(log(len))` time across wrapped ring storage, allocates
    /// nothing and leaves history unchanged. Strict insertion order makes each
    /// interpolation gap positive. Subtraction occurs in ordered `u64` values
    /// before conversion to `f64`, so short gaps at large ticks remain precise.
    /// Weights are approximate over wide gaps; use the outcome for identity,
    /// never `alpha == 0` or `alpha == 1`. See the runnable [module recipe](self).
    pub fn bracket_precise(
        &self,
        tick: u64,
        fraction: f64,
        endpoints: HistoryEndpointPolicy,
        minimum_newest_tick: Option<u64>,
    ) -> Result<HistoryBracket<'_, T>, HistoryQueryError> {
        if !(0.0..=1.0).contains(&fraction) {
            return Err(HistoryQueryError::InvalidFraction);
        }
        let (tick, fraction) = if fraction == 1.0 {
            (
                tick.checked_add(1).ok_or(HistoryQueryError::TickOverflow)?,
                0.0,
            )
        } else {
            (tick, fraction)
        };
        let Some((newest_tick, newest)) = self.samples.back() else {
            return Ok(HistoryBracket::Empty);
        };
        if let Some(minimum_newest_tick) = minimum_newest_tick
            && *newest_tick < minimum_newest_tick
        {
            return Ok(HistoryBracket::Stale {
                newest: (*newest_tick, newest),
                minimum_newest_tick,
            });
        }
        let after_index = match self
            .samples
            .binary_search_by_key(&tick, |(sample_tick, _)| *sample_tick)
        {
            Ok(index) if fraction == 0.0 => {
                let (tick, state) = &self.samples[index];
                return Ok(HistoryBracket::Exact {
                    sample: (*tick, state),
                });
            }
            Ok(index) => index + 1,
            Err(index) => index,
        };
        if after_index == 0 {
            let (tick, state) = &self.samples[0];
            let oldest = (*tick, state);
            return Ok(match endpoints {
                HistoryEndpointPolicy::Reject => HistoryBracket::BeforeOldest { oldest },
                HistoryEndpointPolicy::Clamp => HistoryBracket::Clamped { sample: oldest },
            });
        }
        let (before_tick, before) = &self.samples[after_index - 1];
        if after_index == self.samples.len() {
            let newest = (*before_tick, before);
            return Ok(match endpoints {
                HistoryEndpointPolicy::Reject => HistoryBracket::AfterNewest { newest },
                HistoryEndpointPolicy::Clamp => HistoryBracket::Clamped { sample: newest },
            });
        }
        let (after_tick, after) = &self.samples[after_index];
        let alpha = ((tick - before_tick) as f64 + fraction) / (after_tick - before_tick) as f64;
        Ok(HistoryBracket::Interpolated {
            before: (*before_tick, before),
            after: (*after_tick, after),
            alpha,
        })
    }

    /// Drop all retained states and forget insertion order, keeping allocated capacity.
    ///
    /// On return, a lower tick may start a new session. Clear this history at the
    /// same reconnect boundary as [`crate::clock::ClientClock::reset`] and
    /// [`crate::clock::TickUnwrapper::reset`], after excluding old-session traffic.
    /// Neither clearing nor tick unwrapping authenticates a session. Old samples
    /// cannot be recovered. A panic or side effect in `T::drop` is not rolled back;
    /// allocation-free reuse assumes the states' destructors do not allocate.
    /// The [module recipe](self) demonstrates lower-tick reuse after reconnect.
    pub fn clear(&mut self) {
        self.samples.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (u64, &T)> {
        self.samples.iter().map(|(t, v)| (*t, v))
    }
    pub fn first_tick(&self) -> Option<u64> {
        self.samples.front().map(|s| s.0)
    }
    pub fn len(&self) -> usize {
        self.samples.len()
    }
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}
#[derive(Default, Clone, Copy, Debug)]
pub struct ClockSync {
    pub offset_seconds: f64,
    pub rtt_seconds: f64,
    samples: u32,
}
impl ClockSync {
    /// NTP-style exchange: local send/receive and server receive/send timestamps.
    pub fn observe(
        &mut self,
        local_send: f64,
        server_receive: f64,
        server_send: f64,
        local_receive: f64,
    ) -> bool {
        if ![local_send, server_receive, server_send, local_receive]
            .iter()
            .all(|v| v.is_finite())
            || local_receive < local_send
            || server_send < server_receive
        {
            return false;
        }
        let rtt = (local_receive - local_send) - (server_send - server_receive);
        if !(0.0..=5.0).contains(&rtt) {
            return false;
        }
        let offset = ((server_receive - local_send) + (server_send - local_receive)) * 0.5;
        let weight = if self.samples == 0 { 1.0 } else { 0.125 };
        self.offset_seconds += (offset - self.offset_seconds) * weight;
        self.rtt_seconds += (rtt - self.rtt_seconds) * weight;
        self.samples = self.samples.saturating_add(1);
        true
    }
    pub fn server_time(&self, local: f64) -> f64 {
        local + self.offset_seconds
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn old_or_future_shots_are_rejected_and_history_interpolates() {
        let mut h = History::new(3);
        for i in 0..5 {
            assert!(h.push(i, i as f32));
        }
        assert_eq!(h.len(), 3);
        assert!(h.bracket(1.0).is_none());
        assert!(h.bracket(5.0).is_none());
        assert!(!h.push(3, 0.0));
        assert_eq!(h.bracket(2.5), Some((&2.0, &3.0, 0.5)));
        assert_eq!(h.bracket(4.0), Some((&4.0, &4.0, 0.0)));
    }
    #[test]
    fn clock_offset_accounts_for_server_processing() {
        let mut c = ClockSync::default();
        assert!(c.observe(1.0, 11.1, 11.2, 1.3));
        assert!((c.offset_seconds - 10.0).abs() < 1e-9);
        assert!((c.rtt_seconds - 0.2).abs() < 1e-9);
        assert!(!c.observe(f64::NAN, 0., 0., 0.));
    }
}
