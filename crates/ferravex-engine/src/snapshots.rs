//! Bounded monotonic tick snapshots. Hosts own state and interpolation semantics.
//! Failed inserts preserve history; rolling eviction removes only the oldest state.
//!
//! Use [`SnapshotRing::bracket_checked`](crate::snapshots::SnapshotRing::bracket_checked) for presentation queries with an explicit
//! outcome when the requested time is outside retained history. It borrows states
//! without allocating, interpolating game data, or silently extrapolating.
//!
//! ```
//! use ferravex_engine::snapshots::{SnapshotBracket, SnapshotQueryError, SnapshotRing};
//!
//! let mut history = SnapshotRing::new(8);
//! history.push(10, 0.0_f64).unwrap();
//! history.push(14, 8.0).unwrap();
//! // Query tick 11 + 0.5 without converting the whole tick to floating point.
//! let position = match history.bracket_checked(11, 0.5)? {
//!     SnapshotBracket::Between { before: (_, a), after: (_, b), alpha } => {
//!         Some(a + (b - a) * alpha)
//!     }
//!     SnapshotBracket::Exact { snapshot: (_, state) } => Some(*state),
//!     SnapshotBracket::Empty
//!     | SnapshotBracket::BeforeOldest { .. }
//!     | SnapshotBracket::AfterNewest { .. } => None,
//! };
//! assert_eq!(position, Some(3.0));
//! # Ok::<(), SnapshotQueryError>(())
//! ```
use std::collections::VecDeque;

/// Optional tick and borrowed state, used by [`SnapshotRing::bracket`].
pub type SnapshotRef<'a, T> = Option<(u64, &'a T)>;

/// Checked placement of a presentation time within retained snapshot history.
///
/// Only [`Between`](Self::Between) supplies an interpolation factor. Out-of-range
/// results expose the nearest endpoint so the host can explicitly choose to hold
/// it, wait for more snapshots, or apply its own extrapolation policy. See the
/// [module example](self) for a host that only renders in-range queries.
#[derive(Debug, PartialEq)]
pub enum SnapshotBracket<'a, T> {
    /// No snapshots are retained, including when the ring has zero capacity.
    Empty,
    /// The query precedes the oldest retained tick.
    BeforeOldest {
        /// Earliest available tick and state.
        oldest: (u64, &'a T),
    },
    /// The query equals a retained tick; no division or interpolation is needed.
    Exact {
        /// Matching tick and state.
        snapshot: (u64, &'a T),
    },
    /// Two distinct, consecutive retained snapshots surround the query.
    Between {
        /// Snapshot strictly before the query.
        before: (u64, &'a T),
        /// Snapshot strictly after the query.
        after: (u64, &'a T),
        /// Fraction from `before` to `after`, in `[0, 1]`.
        ///
        /// Uses the actual tick gap, including missing ticks. Floating-point
        /// rounding can produce an endpoint factor for a strictly interior query.
        alpha: f64,
    },
    /// The query follows the newest retained tick.
    AfterNewest {
        /// Latest available tick and state.
        newest: (u64, &'a T),
    },
}

/// Invalid input to [`SnapshotRing::bracket_checked`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotQueryError {
    /// The fractional tick is nonfinite or outside the inclusive range `[0, 1]`.
    InvalidFraction,
    /// A fraction of exactly `1` would advance `u64::MAX` to an unrepresentable tick.
    TickOverflow,
}

impl std::fmt::Display for SnapshotQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidFraction => "snapshot fraction must be finite and in [0, 1]",
            Self::TickOverflow => "snapshot fraction carries beyond the maximum tick",
        })
    }
}

impl std::error::Error for SnapshotQueryError {}

/// Bounded history of strictly increasing, nonwrapping `u64` ticks.
///
/// Duplicate or decreasing ticks are rejected; rollback hosts can explicitly
/// discard future snapshots with [`truncate_after`](Self::truncate_after).
pub struct SnapshotRing<T> {
    states: VecDeque<(u64, T)>,
    capacity: usize,
}
impl<T> SnapshotRing<T> {
    /// Create an empty history. A zero-capacity ring rejects every insertion.
    pub fn new(capacity: usize) -> Self {
        Self {
            states: VecDeque::new(),
            capacity,
        }
    }
    /// Append a strictly newer tick, returning the oldest snapshot if evicted.
    ///
    /// Zero capacity, duplicate ticks and decreasing ticks return `Err(state)`
    /// without changing history or evicting anything. Ticks do not wrap at
    /// `u64::MAX`; start a new history for a new tick epoch.
    pub fn push(&mut self, tick: u64, state: T) -> Result<Option<(u64, T)>, T> {
        if self.capacity == 0 || self.states.back().is_some_and(|(last, _)| *last >= tick) {
            return Err(state);
        }
        let old = if self.states.len() == self.capacity {
            self.states.pop_front()
        } else {
            None
        };
        self.states.push_back((tick, state));
        Ok(old)
    }
    pub fn get(&self, tick: u64) -> Option<&T> {
        self.states.iter().find(|(t, _)| *t == tick).map(|(_, s)| s)
    }
    /// Nearest samples on either side (inclusive); exact matches appear in both slots.
    ///
    /// See [`bracket_checked`](Self::bracket_checked) for fractional queries,
    /// explicit boundary outcomes and a checked interpolation factor.
    pub fn bracket(&self, tick: u64) -> (SnapshotRef<'_, T>, SnapshotRef<'_, T>) {
        let before = self
            .states
            .iter()
            .rev()
            .find(|(t, _)| *t <= tick)
            .map(|(t, s)| (*t, s));
        let after = self
            .states
            .iter()
            .find(|(t, _)| *t >= tick)
            .map(|(t, s)| (*t, s));
        (before, after)
    }

    /// Locate `tick + fraction` without extrapolation or state allocation.
    ///
    /// `fraction` must be finite and in `[0, 1]`; exactly `1` is normalized to the
    /// next whole tick. Integer ordering is preserved across the full `u64` tick
    /// range: only relative tick differences are converted to `f64` for the
    /// interpolation factor. A positive fraction at the newest tick returns
    /// [`AfterNewest`](SnapshotBracket::AfterNewest), even at `u64::MAX` when the
    /// fraction is less than `1`.
    ///
    /// Exact queries return a single snapshot, so duplicates never create a
    /// zero-length interpolation interval. Inserts already reject duplicate and
    /// decreasing ticks. Lookup uses logical deque indices, works across wrapped
    /// storage, takes `O(log(len))` time and does not mutate history.
    ///
    /// # Errors
    ///
    /// Invalid fractions return [`SnapshotQueryError::InvalidFraction`]. A query
    /// of `(u64::MAX, 1.0)` returns [`SnapshotQueryError::TickOverflow`]. Inputs are
    /// validated even when the ring is empty. No invalid input is clamped.
    ///
    /// ```
    /// use ferravex_engine::snapshots::{SnapshotBracket, SnapshotQueryError, SnapshotRing};
    ///
    /// let mut history = SnapshotRing::new(2);
    /// history.push(10, "previous").unwrap();
    /// history.push(12, "current").unwrap();
    /// assert!(matches!(history.bracket_checked(9, 0.5)?,
    ///     SnapshotBracket::BeforeOldest { oldest: (10, _) }));
    /// assert!(matches!(history.bracket_checked(11, 1.0)?,
    ///     SnapshotBracket::Exact { snapshot: (12, _) }));
    /// assert!(matches!(history.bracket_checked(12, 0.5)?,
    ///     SnapshotBracket::AfterNewest { newest: (12, _) }));
    /// assert_eq!(history.bracket_checked(11, f64::NAN),
    ///     Err(SnapshotQueryError::InvalidFraction));
    /// # Ok::<(), SnapshotQueryError>(())
    /// ```
    pub fn bracket_checked(
        &self,
        tick: u64,
        fraction: f64,
    ) -> Result<SnapshotBracket<'_, T>, SnapshotQueryError> {
        if !(0.0..=1.0).contains(&fraction) {
            return Err(SnapshotQueryError::InvalidFraction);
        }
        let (tick, fraction) = if fraction == 1.0 {
            (
                tick.checked_add(1)
                    .ok_or(SnapshotQueryError::TickOverflow)?,
                0.0,
            )
        } else {
            (tick, fraction)
        };
        if self.states.is_empty() {
            return Ok(SnapshotBracket::Empty);
        }

        let after_index = match self
            .states
            .binary_search_by_key(&tick, |(sample_tick, _)| *sample_tick)
        {
            Ok(index) if fraction == 0.0 => {
                let (tick, state) = &self.states[index];
                return Ok(SnapshotBracket::Exact {
                    snapshot: (*tick, state),
                });
            }
            Ok(index) => index + 1,
            Err(index) => index,
        };
        if after_index == 0 {
            let (tick, state) = &self.states[0];
            return Ok(SnapshotBracket::BeforeOldest {
                oldest: (*tick, state),
            });
        }
        let (before_tick, before) = &self.states[after_index - 1];
        if after_index == self.states.len() {
            return Ok(SnapshotBracket::AfterNewest {
                newest: (*before_tick, before),
            });
        }
        let (after_tick, after) = &self.states[after_index];
        // Strict insertion order guarantees a positive denominator. Subtract
        // before converting, preserving short intervals at large absolute ticks.
        let alpha = ((tick - before_tick) as f64 + fraction) / (after_tick - before_tick) as f64;
        Ok(SnapshotBracket::Between {
            before: (*before_tick, before),
            after: (*after_tick, after),
            alpha,
        })
    }

    /// Discard the future so a rollback host can append a new timeline.
    pub fn truncate_after(&mut self, tick: u64) {
        while self.states.back().is_some_and(|(t, _)| *t > tick) {
            self.states.pop_back();
        }
    }
    pub fn len(&self) -> usize {
        self.states.len()
    }
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rolling_brackets_and_rollback_branch() {
        let mut h = SnapshotRing::new(2);
        h.push(10, "a").unwrap();
        h.push(20, "b").unwrap();
        assert_eq!(h.push(19, "bad"), Err("bad"));
        assert_eq!(h.bracket(15), (Some((10, &"a")), Some((20, &"b"))));
        assert_eq!(h.bracket(20), (Some((20, &"b")), Some((20, &"b"))));
        assert_eq!(h.bracket(0), (None, Some((10, &"a"))));
        assert_eq!(h.push(30, "c"), Ok(Some((10, "a"))));
        assert!(h.get(10).is_none());
        h.truncate_after(20);
        h.push(25, "branch").unwrap();
        assert_eq!(h.get(25), Some(&"branch"));
        assert_eq!(h.get(30), None);
        assert_eq!(SnapshotRing::new(0).push(0, 7), Err(7));
    }

    #[test]
    fn checked_empty_and_single_snapshot_boundaries() {
        for capacity in [0, 1, 4] {
            let mut history = SnapshotRing::new(capacity);
            assert_eq!(history.bracket_checked(0, 0.0), Ok(SnapshotBracket::Empty));
            if capacity == 0 {
                assert_eq!(history.push(10, "only"), Err("only"));
                continue;
            }
            history.push(10, "only").unwrap();
            assert_eq!(
                history.bracket_checked(9, 0.75),
                Ok(SnapshotBracket::BeforeOldest {
                    oldest: (10, &"only"),
                })
            );
            for (tick, fraction) in [(10, 0.0), (10, -0.0), (9, 1.0)] {
                assert_eq!(
                    history.bracket_checked(tick, fraction),
                    Ok(SnapshotBracket::Exact {
                        snapshot: (10, &"only"),
                    })
                );
            }
            for (tick, fraction) in [(10, 0.25), (10, 1.0), (11, 0.0)] {
                assert_eq!(
                    history.bracket_checked(tick, fraction),
                    Ok(SnapshotBracket::AfterNewest {
                        newest: (10, &"only"),
                    })
                );
            }
        }
    }

    #[test]
    fn checked_sparse_ticks_and_fractional_exact_tick() {
        let mut history = SnapshotRing::new(3);
        for tick in [10, 14, 20] {
            history.push(tick, tick).unwrap();
        }
        for (tick, fraction, before, after, alpha) in [
            (10, 0.5, 10, 14, 0.125),
            (11, 0.5, 10, 14, 0.375),
            (11, 1.0, 10, 14, 0.5),
            (14, 0.75, 14, 20, 0.125),
            (17, 0.0, 14, 20, 0.5),
        ] {
            assert_eq!(
                history.bracket_checked(tick, fraction),
                Ok(SnapshotBracket::Between {
                    before: (before, &before),
                    after: (after, &after),
                    alpha,
                })
            );
        }
        for tick in [10, 14, 20] {
            assert_eq!(
                history.bracket_checked(tick, 0.0),
                Ok(SnapshotBracket::Exact {
                    snapshot: (tick, &tick),
                })
            );
        }
    }

    #[test]
    fn invalid_query_inputs_are_errors_even_without_history() {
        let mut history = SnapshotRing::new(2);
        for populated in [false, true] {
            if populated {
                history.push(0, "first").unwrap();
                history.push(u64::MAX, "last").unwrap();
            }
            for fraction in [
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                -f64::EPSILON,
                1.0 + f64::EPSILON,
                f64::MAX,
            ] {
                assert_eq!(
                    history.bracket_checked(0, fraction),
                    Err(SnapshotQueryError::InvalidFraction)
                );
            }
            assert_eq!(
                history.bracket_checked(u64::MAX, 1.0),
                Err(SnapshotQueryError::TickOverflow)
            );
        }
        assert_eq!(history.len(), 2);
        assert_eq!(history.get(0), Some(&"first"));
        assert_eq!(history.get(u64::MAX), Some(&"last"));
    }

    #[test]
    fn large_absolute_ticks_preserve_fractional_precision_and_order() {
        for base in [1_u64 << 53, u64::MAX - 4] {
            let mut history = SnapshotRing::new(2);
            history.push(base, "a").unwrap();
            history.push(base + 4, "b").unwrap();
            assert_eq!(
                history.bracket_checked(base + 2, 0.25),
                Ok(SnapshotBracket::Between {
                    before: (base, &"a"),
                    after: (base + 4, &"b"),
                    alpha: 0.5625,
                })
            );
            assert!(matches!(
                history.bracket_checked(base - 1, 0.5),
                Ok(SnapshotBracket::BeforeOldest { .. })
            ));
            assert!(matches!(
                history.bracket_checked(base + 3, 1.0),
                Ok(SnapshotBracket::Exact { snapshot: (tick, _) }) if tick == base + 4
            ));
            assert!(matches!(
                history.bracket_checked(base + 4, 0.5),
                Ok(SnapshotBracket::AfterNewest { .. })
            ));
        }
    }

    #[test]
    fn full_u64_interval_produces_finite_bounded_factors() {
        let mut history = SnapshotRing::new(2);
        history.push(0, ()).unwrap();
        history.push(u64::MAX, ()).unwrap();
        for (tick, fraction) in [
            (0, f64::MIN_POSITIVE),
            (0, 0.5),
            (1, 0.0),
            (1_u64 << 63, 0.5),
            (u64::MAX - 1, 0.75),
        ] {
            let SnapshotBracket::Between { alpha, .. } =
                history.bracket_checked(tick, fraction).unwrap()
            else {
                panic!("interior query must retain its classification");
            };
            assert!(alpha.is_finite());
            assert!((0.0..=1.0).contains(&alpha));
        }
    }

    #[test]
    fn rejected_duplicates_decreasing_and_wrapped_ticks_preserve_full_history() {
        let mut history = SnapshotRing::new(2);
        history.push(10, String::from("first")).unwrap();
        history.push(20, String::from("last")).unwrap();
        for rejected_tick in [20, 10, 19, 0] {
            let rejected_state = String::from("returned intact");
            let allocation = rejected_state.as_ptr();
            let returned = history.push(rejected_tick, rejected_state).unwrap_err();
            assert_eq!(returned, "returned intact");
            assert_eq!(returned.as_ptr(), allocation);
            assert_eq!(history.len(), 2);
            assert_eq!(history.get(10).map(String::as_str), Some("first"));
            assert_eq!(history.get(20).map(String::as_str), Some("last"));
            assert!(matches!(
                history.bracket_checked(15, 0.0),
                Ok(SnapshotBracket::Between { alpha: 0.5, .. })
            ));
        }
        history.push(u64::MAX, String::from("maximum")).unwrap();
        for rejected_tick in [u64::MAX, 0] {
            assert!(history.push(rejected_tick, String::new()).is_err());
        }
        assert_eq!(history.get(20).map(String::as_str), Some("last"));
        assert_eq!(history.get(u64::MAX).map(String::as_str), Some("maximum"));
    }

    #[test]
    fn checked_queries_cross_wrapped_deque_storage_and_evicted_boundaries() {
        let mut history = SnapshotRing::new(3);
        let mut queried_wrapped_storage = false;
        for tick in (0..320).step_by(10) {
            let evicted = history.push(tick, tick).unwrap();
            assert_eq!(evicted, tick.checked_sub(30).map(|old| (old, old)));
            if tick < 20 {
                continue;
            }
            queried_wrapped_storage |= !history.states.as_slices().1.is_empty();
            for earlier in [tick - 20, tick - 10] {
                assert_eq!(
                    history.bracket_checked(earlier + 5, 0.5),
                    Ok(SnapshotBracket::Between {
                        before: (earlier, &earlier),
                        after: (earlier + 10, &(earlier + 10)),
                        alpha: 0.55,
                    })
                );
                assert_eq!(
                    history.bracket_checked(earlier, 0.0),
                    Ok(SnapshotBracket::Exact {
                        snapshot: (earlier, &earlier),
                    })
                );
            }
            assert!(matches!(
                history.bracket_checked(tick, 0.5),
                Ok(SnapshotBracket::AfterNewest { newest: (t, _) }) if t == tick
            ));
            if tick >= 30 {
                assert!(matches!(
                    history.bracket_checked(tick - 30, 0.0),
                    Ok(SnapshotBracket::BeforeOldest { oldest: (t, _) }) if t == tick - 20
                ));
            }
            assert_eq!(history.len(), 3);
        }
        assert!(queried_wrapped_storage);
    }

    #[test]
    fn rollback_rebranches_checked_queries_and_can_remove_every_snapshot() {
        let mut history = SnapshotRing::new(3);
        for tick in [10, 20, 30, 40] {
            history.push(tick, tick).unwrap();
        }
        history.truncate_after(30);
        assert_eq!(
            history.bracket_checked(35, 0.0),
            Ok(SnapshotBracket::AfterNewest { newest: (30, &30) })
        );
        history.push(38, 38).unwrap();
        assert_eq!(
            history.bracket_checked(35, 0.0),
            Ok(SnapshotBracket::Between {
                before: (30, &30),
                after: (38, &38),
                alpha: 0.625,
            })
        );
        history.truncate_after(19);
        assert_eq!(history.bracket_checked(35, 0.0), Ok(SnapshotBracket::Empty));
        history.push(0, 0).unwrap();
        assert_eq!(
            history.bracket_checked(0, 0.0),
            Ok(SnapshotBracket::Exact { snapshot: (0, &0) })
        );
    }

    #[test]
    fn checked_results_borrow_non_clone_states() {
        struct State;
        let mut history = SnapshotRing::new(2);
        assert!(history.push(0, State).is_ok());
        assert!(history.push(2, State).is_ok());
        let SnapshotBracket::Between { before, after, .. } =
            history.bracket_checked(1, 0.0).unwrap()
        else {
            panic!("expected surrounding snapshots");
        };
        assert!(std::ptr::eq(before.1, history.get(0).unwrap()));
        assert!(std::ptr::eq(after.1, history.get(2).unwrap()));
    }
}
