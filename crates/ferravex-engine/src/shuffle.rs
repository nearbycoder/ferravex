//! Reproducible shuffled index cycles for nonrepeating engine variants.
//!
//! Each cycle is a permutation of `0..count`. [`ShuffleBag::new`] retains the
//! original seeded sequence, including possible repeats across cycle boundaries.
//! Opt into [`BoundaryPolicy::AvoidImmediateRepeat`] to exclude those repeats
//! when there are at least two items. Each allowed permutation is equally likely;
//! a two-item bag necessarily alternates forever after its random first choice.
//! An empty bag always returns `None`; a singleton always returns `Some(0)`.
//!
//! Cycles are shuffled lazily on their first draw. Inspection never consumes
//! randomness. [`ShuffleBag::reset`] restores the original seed and index order,
//! discards any unfinished cycle, and clears progress and boundary history.
//!
//! ```
//! use ferravex_engine::shuffle::{BoundaryPolicy, ShuffleBag};
//!
//! let mut bag = ShuffleBag::with_policy(3, 42, BoundaryPolicy::AvoidImmediateRepeat).unwrap();
//! assert_eq!(bag.boundary_policy(), BoundaryPolicy::AvoidImmediateRepeat);
//! assert_eq!(bag.len(), 3); // Items per cycle, not an iterator length.
//! assert!(!bag.is_empty());
//! assert_eq!(bag.completed_cycles(), 0);
//! assert_eq!(bag.cycle_position(), 0);
//! assert_eq!(bag.remaining_in_cycle(), 3);
//! assert_eq!(bag.last_drawn(), None);
//! let first = bag.by_ref().take(3).collect::<Vec<_>>();
//! assert_eq!(bag.completed_cycles(), 1);
//! assert_eq!(bag.cycle_position(), 3); // Retained until the next draw.
//! assert_eq!(bag.remaining_in_cycle(), 0);
//! assert_eq!(bag.last_drawn(), first.last().copied());
//! assert_ne!(bag.next(), first.last().copied());
//! assert_eq!(bag.cycle_position(), 1);
//! bag.reset();
//! assert_eq!(bag.last_drawn(), None);
//! assert_eq!(bag.by_ref().take(3).collect::<Vec<_>>(), first);
//! ```

use crate::random::Random;

/// Whether a new cycle may begin with the previous cycle's final index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BoundaryPolicy {
    /// Allow boundary repeats, preserving the original shuffle-bag sequence.
    #[default]
    AllowRepeat,
    /// Exclude boundary repeats when the bag has at least two items.
    /// Singleton repeats are unavoidable and remain permitted.
    AvoidImmediateRepeat,
}

/// An endless sequence of shuffled index cycles, or an empty iterator for zero items.
///
/// Stores at most one million indices. Construction and reset take `O(count)`
/// time; drawing takes `O(1)` except for the `O(count)` shuffle at each cycle's
/// first draw. Draws, inspection and reset do not allocate. Bounded-integer
/// random sampling may retry internally to avoid modulo bias.
#[derive(Debug)]
pub struct ShuffleBag {
    items: Vec<usize>,
    cursor: usize,
    random: Random,
    seed: u64,
    policy: BoundaryPolicy,
    completed_cycles: u64,
    last_drawn: Option<usize>,
}

impl ShuffleBag {
    /// Construct a bag with [`BoundaryPolicy::AllowRepeat`].
    ///
    /// Counts above one million are rejected. Empty bags produce no values.
    /// The original seeded output and random draw consumption are preserved.
    ///
    /// ```
    /// use ferravex_engine::shuffle::ShuffleBag;
    /// let bag = ShuffleBag::new(5, 42).unwrap();
    /// assert_eq!(bag.take(5).collect::<Vec<_>>(), [1, 2, 0, 4, 3]);
    /// assert!(ShuffleBag::new(1_000_001, 42).is_none());
    /// ```
    pub fn new(count: usize, seed: u64) -> Option<Self> {
        Self::with_policy(count, seed, BoundaryPolicy::AllowRepeat)
    }

    /// Construct a bag with an explicit cycle boundary policy.
    ///
    /// Counts above one million are rejected. The first cycle matches
    /// [`Self::new`]. Avoiding repeats changes subsequent seeded output and may
    /// consume an extra random sample on a boundary collision. It does not retry
    /// entire shuffles, discard indices, or favor any allowed permutation.
    ///
    /// ```
    /// use ferravex_engine::shuffle::{BoundaryPolicy, ShuffleBag};
    /// let policy = BoundaryPolicy::AvoidImmediateRepeat;
    /// assert_eq!(ShuffleBag::with_policy(0, 42, policy).unwrap().next(), None);
    /// assert_eq!(ShuffleBag::with_policy(1, 42, policy).unwrap().take(3)
    ///     .collect::<Vec<_>>(), [0, 0, 0]);
    /// let draws = ShuffleBag::with_policy(2, 42, policy).unwrap().take(20)
    ///     .collect::<Vec<_>>();
    /// assert!(draws.windows(2).all(|pair| pair[0] != pair[1]));
    /// ```
    pub fn with_policy(count: usize, seed: u64, policy: BoundaryPolicy) -> Option<Self> {
        if count > 1_000_000 {
            return None;
        }
        Some(Self {
            items: (0..count).collect(),
            cursor: count,
            random: Random::new(seed),
            seed,
            policy,
            completed_cycles: 0,
            last_drawn: None,
        })
    }

    /// Number of distinct indices in each cycle, not the iterator's remaining length.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether this bag will always return `None`.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The policy selected at construction; reset preserves it.
    pub fn boundary_policy(&self) -> BoundaryPolicy {
        self.policy
    }

    /// Fully consumed cycles since construction or reset, saturating at `u64::MAX`.
    ///
    /// Increments on a cycle's final draw. Empty bags never complete a cycle.
    pub fn completed_cycles(&self) -> u64 {
        self.completed_cycles
    }

    /// Draws consumed in the current cycle, or zero before the first draw or after reset.
    ///
    /// Retains [`Self::len`] after the final draw until the next draw starts a
    /// new cycle. Always zero for an empty bag.
    pub fn cycle_position(&self) -> usize {
        if self.last_drawn.is_none() {
            0
        } else {
            self.cursor
        }
    }

    /// Draws left in the current cycle: `len() - cycle_position()`.
    ///
    /// Before the first draw or after reset, returns the full cycle length.
    /// After a cycle's final draw, returns zero until the next draw starts one.
    pub fn remaining_in_cycle(&self) -> usize {
        self.len() - self.cycle_position()
    }

    /// Most recently returned index, or `None` before the first draw or after reset.
    pub fn last_drawn(&self) -> Option<usize> {
        self.last_drawn
    }

    /// Restart the original seeded sequence with the same size and policy.
    ///
    /// Resets progress and boundary history, including an unfinished cycle.
    /// The first value after reset may equal the value just before reset.
    /// Reuses the existing index storage without allocating or drawing randomness.
    ///
    /// ```
    /// use ferravex_engine::shuffle::{BoundaryPolicy, ShuffleBag};
    /// let mut bag = ShuffleBag::with_policy(4, 7, BoundaryPolicy::AvoidImmediateRepeat).unwrap();
    /// let original = bag.by_ref().take(19).collect::<Vec<_>>();
    /// bag.reset();
    /// assert_eq!(bag.completed_cycles(), 0);
    /// assert_eq!(bag.cycle_position(), 0);
    /// assert_eq!(bag.remaining_in_cycle(), 4);
    /// assert_eq!(bag.by_ref().take(19).collect::<Vec<_>>(), original);
    /// ```
    pub fn reset(&mut self) {
        for (index, value) in self.items.iter_mut().enumerate() {
            *value = index;
        }
        self.cursor = self.items.len();
        self.random = Random::new(self.seed);
        self.completed_cycles = 0;
        self.last_drawn = None;
    }
}

// A uniform shuffle is corrected only on a collision. Each valid permutation
// has its unchanged preimage plus one colliding preimage repaired with chance
// 1/(n - 1), so all valid permutations have the same probability. A fixed swap
// would bias the ordering. The chooser returns an index in 0..upper and is called
// at most once; separating it lets tests exhaust every equally likely choice.
fn avoid_boundary_repeat(
    items: &mut [usize],
    previous: Option<usize>,
    choose: impl FnOnce(usize) -> usize,
) {
    if items.len() > 1 && previous == Some(items[0]) {
        let index = 1 + choose(items.len() - 1);
        items.swap(0, index);
    }
}

impl Iterator for ShuffleBag {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        if self.items.is_empty() {
            return None;
        }
        if self.cursor == self.items.len() {
            for i in (1..self.items.len()).rev() {
                let j = self.random.below((i + 1) as u64).unwrap() as usize;
                self.items.swap(i, j);
            }
            if self.policy == BoundaryPolicy::AvoidImmediateRepeat {
                avoid_boundary_repeat(&mut self.items, self.last_drawn, |upper| {
                    self.random.below(upper as u64).unwrap() as usize
                });
            }
            self.cursor = 0;
        }
        let value = self.items[self.cursor];
        self.cursor += 1;
        self.last_drawn = Some(value);
        if self.cursor == self.items.len() {
            self.completed_cycles = self.completed_cycles.saturating_add(1);
        }
        Some(value)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cycle_is_permutation_and_seed_is_reproducible() {
        let mut a = ShuffleBag::new(12, 42).unwrap();
        let mut b = ShuffleBag::new(12, 42).unwrap();
        for _ in 0..10 {
            let cycle: Vec<_> = a.by_ref().take(12).collect();
            assert_eq!(cycle, b.by_ref().take(12).collect::<Vec<_>>());
            let mut sorted = cycle;
            sorted.sort_unstable();
            assert_eq!(sorted, (0..12).collect::<Vec<_>>());
        }
        assert_eq!(ShuffleBag::new(0, 0).unwrap().next(), None);
        assert_eq!(
            ShuffleBag::new(1, 0).unwrap().take(3).collect::<Vec<_>>(),
            [0, 0, 0]
        );
        assert!(ShuffleBag::new(1_000_001, 0).is_none());
    }

    #[test]
    fn default_stream_keeps_original_golden_cycles_and_boundary_repeats() {
        assert_eq!(BoundaryPolicy::default(), BoundaryPolicy::AllowRepeat);
        for (count, seed, expected) in [
            (
                5,
                42,
                vec![1, 2, 0, 4, 3, 4, 3, 2, 0, 1, 3, 1, 0, 2, 4, 1, 3, 0, 4, 2],
            ),
            (2, 0, vec![0, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0]),
            (
                3,
                u64::MAX,
                vec![0, 1, 2, 2, 0, 1, 1, 0, 2, 2, 1, 0, 1, 0, 2, 1, 2, 0],
            ),
        ] {
            let default = ShuffleBag::new(count, seed).unwrap();
            assert_eq!(default.boundary_policy(), BoundaryPolicy::AllowRepeat);
            assert_eq!(default.take(expected.len()).collect::<Vec<_>>(), expected);
            let explicit =
                ShuffleBag::with_policy(count, seed, BoundaryPolicy::AllowRepeat).unwrap();
            assert_eq!(explicit.take(expected.len()).collect::<Vec<_>>(), expected);
        }
    }

    #[test]
    fn avoiding_repeats_keeps_permutations_and_seed_reproducibility() {
        for count in [2, 3, 7, 16] {
            for seed in [0, 1, 42, u64::MAX] {
                let mut bag =
                    ShuffleBag::with_policy(count, seed, BoundaryPolicy::AvoidImmediateRepeat)
                        .unwrap();
                let mut replay =
                    ShuffleBag::with_policy(count, seed, BoundaryPolicy::AvoidImmediateRepeat)
                        .unwrap();
                let initial = ShuffleBag::new(count, seed)
                    .unwrap()
                    .take(count)
                    .collect::<Vec<_>>();
                let mut previous = None;
                for cycle_index in 0..100 {
                    let cycle = bag.by_ref().take(count).collect::<Vec<_>>();
                    assert_eq!(cycle, replay.by_ref().take(count).collect::<Vec<_>>());
                    if cycle_index == 0 {
                        assert_eq!(cycle, initial);
                    }
                    assert_ne!(Some(cycle[0]), previous);
                    previous = cycle.last().copied();
                    let mut sorted = cycle;
                    sorted.sort_unstable();
                    assert_eq!(sorted, (0..count).collect::<Vec<_>>());
                    assert_eq!(bag.completed_cycles(), cycle_index + 1);
                }
            }
        }
    }

    #[test]
    fn two_item_policy_alternates_from_either_initial_index() {
        let mut starts = [false; 2];
        for seed in 0..32 {
            let values = ShuffleBag::with_policy(2, seed, BoundaryPolicy::AvoidImmediateRepeat)
                .unwrap()
                .take(200)
                .collect::<Vec<_>>();
            starts[values[0]] = true;
            assert!(values.windows(2).all(|pair| pair[0] != pair[1]));
        }
        assert_eq!(starts, [true, true]);
    }

    #[test]
    fn inspection_tracks_lazy_cycle_transitions_without_changing_draws() {
        for policy in [
            BoundaryPolicy::AllowRepeat,
            BoundaryPolicy::AvoidImmediateRepeat,
        ] {
            let mut bag = ShuffleBag::with_policy(3, 42, policy).unwrap();
            let mut uninspected = ShuffleBag::with_policy(3, 42, policy).unwrap();
            assert_eq!(bag.cycle_position(), 0);
            assert_eq!(bag.remaining_in_cycle(), 3);
            assert_eq!(bag.completed_cycles(), 0);
            assert_eq!(bag.last_drawn(), None);
            for draw_index in 0..30 {
                let value = bag.next();
                assert_eq!(value, uninspected.next());
                assert_eq!(bag.len(), 3);
                assert!(!bag.is_empty());
                assert_eq!(bag.boundary_policy(), policy);
                assert_eq!(bag.cycle_position(), draw_index % 3 + 1);
                assert_eq!(bag.remaining_in_cycle(), 2 - draw_index % 3);
                assert_eq!(bag.completed_cycles(), (draw_index as u64 + 1) / 3);
                assert_eq!(bag.last_drawn(), value);
            }
        }
    }

    #[test]
    fn reset_restarts_every_policy_from_partial_or_completed_cycles_in_place() {
        for policy in [
            BoundaryPolicy::AllowRepeat,
            BoundaryPolicy::AvoidImmediateRepeat,
        ] {
            for count in [0, 1, 2, 7] {
                for seed in [0, 42, u64::MAX] {
                    let mut bag = ShuffleBag::with_policy(count, seed, policy).unwrap();
                    let storage = bag.items.as_ptr();
                    let capacity = bag.items.capacity();
                    for draws in [0, 1, count, count + 1, count * 7 + 3] {
                        for _ in 0..draws {
                            bag.next();
                        }
                        bag.reset();
                        assert_eq!(bag.items.as_ptr(), storage);
                        assert_eq!(bag.items.capacity(), capacity);
                        assert_eq!(bag.len(), count);
                        assert_eq!(bag.is_empty(), count == 0);
                        assert_eq!(bag.boundary_policy(), policy);
                        assert_eq!(bag.completed_cycles(), 0);
                        assert_eq!(bag.cycle_position(), 0);
                        assert_eq!(bag.remaining_in_cycle(), count);
                        assert_eq!(bag.last_drawn(), None);
                        let fresh = ShuffleBag::with_policy(count, seed, policy).unwrap();
                        assert_eq!(
                            bag.by_ref().take(100).collect::<Vec<_>>(),
                            fresh.take(100).collect::<Vec<_>>()
                        );
                        assert_eq!(bag.items.as_ptr(), storage);
                        assert_eq!(bag.items.capacity(), capacity);
                        bag.reset();
                    }
                }
            }
        }
    }

    #[test]
    fn reset_forgets_previous_boundary_value() {
        let mut bag = ShuffleBag::with_policy(2, 42, BoundaryPolicy::AvoidImmediateRepeat).unwrap();
        let first = bag.next();
        bag.next();
        assert_eq!(bag.next(), first);
        bag.reset();
        assert_eq!(bag.next(), first);
    }

    #[test]
    fn empty_and_singleton_bags_do_not_draw_randomness_or_stall() {
        for policy in [
            BoundaryPolicy::AllowRepeat,
            BoundaryPolicy::AvoidImmediateRepeat,
        ] {
            for count in [0, 1] {
                let mut bag = ShuffleBag::with_policy(count, 42, policy).unwrap();
                for cycles in 1..=100 {
                    let value = bag.next();
                    assert_eq!(value, if count == 0 { None } else { Some(0) });
                    assert_eq!(bag.last_drawn(), value);
                    assert_eq!(bag.cycle_position(), count);
                    assert_eq!(bag.remaining_in_cycle(), 0);
                    assert_eq!(bag.completed_cycles(), if count == 0 { 0 } else { cycles });
                }
                assert_eq!(bag.random.next_u64(), Random::new(42).next_u64());
            }
        }
    }

    #[test]
    fn completed_cycle_inspection_saturates_without_stopping_draws() {
        let mut bag = ShuffleBag::new(1, 0).unwrap();
        bag.completed_cycles = u64::MAX - 1;
        for _ in 0..3 {
            assert_eq!(bag.next(), Some(0));
            assert_eq!(bag.completed_cycles(), u64::MAX);
        }
        bag.reset();
        assert_eq!(bag.completed_cycles(), 0);
        assert_eq!(bag.next(), Some(0));
        assert_eq!(bag.completed_cycles(), 1);
    }

    #[test]
    fn both_policies_keep_the_existing_capacity_bound() {
        for policy in [
            BoundaryPolicy::AllowRepeat,
            BoundaryPolicy::AvoidImmediateRepeat,
        ] {
            assert!(ShuffleBag::with_policy(1_000_001, 42, policy).is_none());
            assert!(ShuffleBag::with_policy(usize::MAX, 42, policy).is_none());
            assert_eq!(
                ShuffleBag::with_policy(1_000_000, 42, policy)
                    .unwrap()
                    .len(),
                1_000_000
            );
        }
    }

    #[test]
    fn boundary_correction_samples_only_on_a_possible_collision() {
        for (mut items, previous) in [
            (vec![], None),
            (vec![], Some(0)),
            (vec![0], Some(0)),
            (vec![0, 1], None),
            (vec![1, 0], Some(0)),
        ] {
            let original = items.clone();
            avoid_boundary_repeat(&mut items, previous, |_| panic!("unneeded sample"));
            assert_eq!(items, original);
        }
    }

    #[test]
    fn boundary_correction_is_uniform_over_all_allowed_small_permutations() {
        fn permutations(items: &mut [usize], start: usize, output: &mut Vec<Vec<usize>>) {
            if start == items.len() {
                output.push(items.to_vec());
                return;
            }
            for index in start..items.len() {
                items.swap(start, index);
                permutations(items, start + 1, output);
                items.swap(start, index);
            }
        }

        for count in 2..=5 {
            let mut all = Vec::new();
            permutations(&mut (0..count).collect::<Vec<_>>(), 0, &mut all);
            for previous in 0..count {
                let mut outcomes = std::collections::BTreeMap::new();
                for permutation in &all {
                    // Enumerate each possible repair choice even for noncolliding
                    // inputs, weighting every input shuffle equally without a
                    // statistical approximation or reliance on particular seeds.
                    for choice in 0..count - 1 {
                        let mut corrected = permutation.clone();
                        avoid_boundary_repeat(&mut corrected, Some(previous), |upper| {
                            assert_eq!(upper, count - 1);
                            choice
                        });
                        assert_ne!(corrected[0], previous);
                        *outcomes.entry(corrected).or_insert(0) += 1;
                    }
                }
                assert_eq!(outcomes.len(), all.len() * (count - 1) / count);
                assert!(outcomes.values().all(|&ways| ways == count));
            }
        }
    }
}
