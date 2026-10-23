//! Checked integer-weight sampling for engine content and procedural variants.
//!
//! Weights are `u64`: negative, fractional and non-finite values are outside
//! this API and should be rejected by content parsers rather than cast. Individual
//! zero weights disable an entry without changing the other entries' indices.
//! An empty slice, more than 4096 entries, an all-zero slice, or a total above
//! `u64::MAX` is invalid.
//!
//! [`WeightedTable`](crate::weighted::WeightedTable) is already a prepared distribution: build it once for
//! repeated allocation-free draws. [`try_sample`](crate::weighted::try_sample) validates and selects directly
//! from a slice without allocating, useful for occasional or changing weights.
//! Both use the same [`Random::below`](crate::random::Random::below) rejection sampler and preserve the original
//! table's ticket mapping and stream consumption, including single-entry tables.
//! Invalid weights consume no random draws. Neither API normalizes weights;
//! scaling even equivalent ratios can change the seeded sequence.
//!
//! ```
//! use ferravex_engine::{random::Random, weighted::WeightedTable};
//!
//! let loot = ["disabled", "common", "rare"];
//! let table = WeightedTable::try_new(&[0, 3, 1])?;
//! let mut random = Random::new(42);
//! for _ in 0..4 {
//!     let item = loot[table.sample(&mut random)];
//!     assert_ne!(item, "disabled");
//! }
//! assert_eq!(table.total(), 4);
//! assert_eq!(table.ticket(3), Some(2)); // Replay-supplied ticket.
//! assert_eq!(table.ticket(4), None); // Exclusive upper bound, never wrapped.
//! # Ok::<(), ferravex_engine::weighted::WeightValidationError>(())
//! ```

use crate::random::Random;
use std::fmt;

/// Compatibility error categories returned by [`WeightedTable::new`].
/// Use [`WeightedTable::try_new`] for entry-specific diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightError {
    /// No entries were supplied.
    Empty,
    /// More than [`WeightedTable::MAX_ENTRIES`] entries were supplied.
    Capacity,
    /// Every entry has zero weight.
    ZeroTotal,
    /// Adding an entry's weight would exceed `u64::MAX`.
    Overflow,
}

impl fmt::Display for WeightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("weights are empty; provide at least one positive weight"),
            Self::Capacity => write!(
                f,
                "too many weights; provide at most {} entries (including zero weights)",
                WeightedTable::MAX_ENTRIES
            ),
            Self::ZeroTotal => {
                f.write_str("all weights are zero; set at least one weight above zero")
            }
            Self::Overflow => f.write_str("weight total exceeds u64::MAX; reduce the weights"),
        }
    }
}

impl std::error::Error for WeightError {}

/// Detailed validation failure, without consuming randomness or allocating.
///
/// Length is checked first; otherwise the first overflowing entry is reported.
/// Zero weights are valid unless every entry is zero. Convert into [`WeightError`]
/// when only the original compatibility category is needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightValidationError {
    /// No entries were supplied.
    Empty,
    /// The entry count exceeds the table's bound, including disabled entries.
    Capacity {
        /// Number of supplied entries.
        len: usize,
        /// Maximum accepted number of entries.
        max: usize,
    },
    /// Every entry has zero weight; at least one positive weight is required.
    ZeroTotal,
    /// This entry would make the total exceed `u64::MAX`.
    Overflow {
        /// Zero-based index into the original weights slice.
        index: usize,
        /// Sum of weights strictly before this entry.
        partial_total: u64,
        /// Weight of the entry that cannot be added.
        weight: u64,
    },
}

impl From<WeightValidationError> for WeightError {
    fn from(error: WeightValidationError) -> Self {
        match error {
            WeightValidationError::Empty => Self::Empty,
            WeightValidationError::Capacity { .. } => Self::Capacity,
            WeightValidationError::ZeroTotal => Self::ZeroTotal,
            WeightValidationError::Overflow { .. } => Self::Overflow,
        }
    }
}

impl fmt::Display for WeightValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => WeightError::Empty.fmt(f),
            Self::Capacity { len, max } => write!(
                f,
                "received {len} weights; provide at most {max} entries (including zero weights)"
            ),
            Self::ZeroTotal => WeightError::ZeroTotal.fmt(f),
            Self::Overflow {
                index,
                partial_total,
                weight,
            } => write!(
                f,
                "weight at index {index} ({weight}) added to the preceding total ({partial_total}) \
                 exceeds u64::MAX; reduce the weights"
            ),
        }
    }
}

impl std::error::Error for WeightValidationError {}

/// A validated, owned cumulative distribution for repeated sampling.
///
/// Preparation uses `O(n)` time and stores one `u64` per input entry, including
/// zero weights. [`Self::ticket`] and [`Self::sample`] search in `O(log n)` time
/// without allocations; random rejection sampling may require multiple draws.
/// Changing the original slice cannot change a prepared table.
#[derive(Debug)]
pub struct WeightedTable {
    cumulative: Vec<u64>,
    total: u64,
}

impl WeightedTable {
    /// Maximum number of entries, including those with zero weight.
    pub const MAX_ENTRIES: usize = 4096;

    /// Prepare a distribution using the original error categories.
    ///
    /// This retains the original validation and seeded sampling behavior.
    /// Prefer [`Self::try_new`] when reporting invalid authored weights.
    ///
    /// ```
    /// use ferravex_engine::weighted::{WeightError, WeightedTable};
    /// assert_eq!(WeightedTable::new(&[0, 0]).unwrap_err(), WeightError::ZeroTotal);
    /// assert_eq!(WeightedTable::new(&[0, 2, 1])?.ticket(2), Some(2));
    /// # Ok::<(), WeightError>(())
    /// ```
    pub fn new(weights: &[u64]) -> Result<Self, WeightError> {
        Self::try_new(weights).map_err(WeightError::from)
    }

    /// Check the entry count and sum without allocating; return the nonzero total.
    ///
    /// # Errors
    ///
    /// Returns [`WeightValidationError::Empty`] or
    /// [`WeightValidationError::Capacity`] before examining entries. Otherwise
    /// reports the first [`WeightValidationError::Overflow`], or
    /// [`WeightValidationError::ZeroTotal`] if all entries are zero.
    ///
    /// ```
    /// use ferravex_engine::weighted::{WeightValidationError, WeightedTable};
    /// assert_eq!(WeightedTable::validate(&[0, 2, 3]), Ok(5));
    /// assert_eq!(WeightedTable::validate(&[u64::MAX, 0, 1]), Err(
    ///     WeightValidationError::Overflow {
    ///         index: 2, partial_total: u64::MAX, weight: 1,
    ///     }
    /// ));
    /// ```
    pub fn validate(weights: &[u64]) -> Result<u64, WeightValidationError> {
        if weights.is_empty() {
            return Err(WeightValidationError::Empty);
        }
        if weights.len() > Self::MAX_ENTRIES {
            return Err(WeightValidationError::Capacity {
                len: weights.len(),
                max: Self::MAX_ENTRIES,
            });
        }
        let mut total = 0_u64;
        for (index, &weight) in weights.iter().enumerate() {
            total = total
                .checked_add(weight)
                .ok_or(WeightValidationError::Overflow {
                    index,
                    partial_total: total,
                    weight,
                })?;
        }
        if total == 0 {
            return Err(WeightValidationError::ZeroTotal);
        }
        Ok(total)
    }

    /// Prepare a reusable distribution with detailed validation errors.
    ///
    /// Performs [`Self::validate`] before allocating cumulative storage. The
    /// table owns its weights' cumulative sums, so the source may be dropped or
    /// changed after this call. Use [`try_sample`] for allocation-free selection
    /// when a reusable table is unnecessary.
    ///
    /// ```
    /// use ferravex_engine::weighted::WeightedTable;
    /// let mut weights = [0, 2, 1];
    /// let table = WeightedTable::try_new(&weights)?;
    /// weights[1] = 0;
    /// assert_eq!(table.ticket(0), Some(1)); // Prepared weights are retained.
    /// # Ok::<(), ferravex_engine::weighted::WeightValidationError>(())
    /// ```
    pub fn try_new(weights: &[u64]) -> Result<Self, WeightValidationError> {
        let total = Self::validate(weights)?;
        let mut cumulative = Vec::with_capacity(weights.len());
        let mut end = 0;
        for &weight in weights {
            // Validation established that every prefix fits in u64.
            end += weight;
            cumulative.push(end);
        }
        Ok(Self { cumulative, total })
    }

    /// Resolve a ticket in `[0, total)` to its original entry index.
    ///
    /// Zero-weight entries are never returned. A ticket at or above the total
    /// returns `None`, without wrapping or clamping. Useful when a replay host
    /// supplies draws; no randomness is consumed.
    pub fn ticket(&self, ticket: u64) -> Option<usize> {
        (ticket < self.total).then(|| self.cumulative.partition_point(|&end| end <= ticket))
    }

    /// Sample an original entry index using the existing rejection sampler.
    ///
    /// The prepared table is always valid, so selection cannot fail. No
    /// allocation or revalidation is needed. Even a single selectable entry
    /// consumes randomness, preserving the original stream position.
    pub fn sample(&self, random: &mut Random) -> usize {
        self.ticket(random.below(self.total).unwrap()).unwrap()
    }

    /// Sum of the validated weights, always in `1..=u64::MAX`.
    pub fn total(&self) -> u64 {
        self.total
    }
}

/// Validate and sample directly from weights without allocating a table.
///
/// Returns the original index, never a zero-weight entry. Validation completes
/// before any random draw: every [`WeightValidationError`] leaves `random`
/// unchanged, even when overflow occurs late in the slice. Successful selection
/// uses exactly the same stream consumption and ticket mapping as
/// [`WeightedTable::sample`]. Work is `O(n)` per call, plus random rejection
/// sampling; prepare a [`WeightedTable`] for repeated draws with unchanged weights.
///
/// ```
/// use ferravex_engine::{random::Random, weighted::{try_sample, WeightValidationError}};
/// let mut random = Random::new(42);
/// let mut expected = random.clone();
/// assert_eq!(try_sample(&[0, 0], &mut random), Err(WeightValidationError::ZeroTotal));
/// assert_eq!(random.next_u64(), expected.next_u64());
/// assert_eq!(try_sample(&[0, 9, 0], &mut random)?, 1);
/// # Ok::<(), WeightValidationError>(())
/// ```
pub fn try_sample(weights: &[u64], random: &mut Random) -> Result<usize, WeightValidationError> {
    let total = WeightedTable::validate(weights)?;
    let mut ticket = random
        .below(total)
        .expect("validated positive weight total");
    for (index, &weight) in weights.iter().enumerate() {
        if ticket < weight {
            return Ok(index);
        }
        ticket -= weight;
    }
    unreachable!("validated weights contain every ticket below the total")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_weight_boundaries_and_rejected_overflow() {
        let table = WeightedTable::new(&[0, 2, 0, 3, 0]).unwrap();
        assert_eq!(table.total(), 5);
        assert_eq!(
            (0..5).map(|t| table.ticket(t).unwrap()).collect::<Vec<_>>(),
            [1, 1, 3, 3, 3]
        );
        assert_eq!(table.ticket(5), None);
        assert!(matches!(WeightedTable::new(&[]), Err(WeightError::Empty)));
        assert!(matches!(
            WeightedTable::new(&[0]),
            Err(WeightError::ZeroTotal)
        ));
        assert!(matches!(
            WeightedTable::new(&[u64::MAX, 1]),
            Err(WeightError::Overflow)
        ));
        let table = WeightedTable::new(&[u64::MAX]).unwrap();
        assert_eq!(table.sample(&mut Random::new(1)), 0);
    }

    #[test]
    fn detailed_errors_keep_the_legacy_categories_and_report_the_first_bad_entry() {
        let cases = [
            (vec![], WeightValidationError::Empty, WeightError::Empty),
            (
                vec![0, 0, 0],
                WeightValidationError::ZeroTotal,
                WeightError::ZeroTotal,
            ),
            (
                vec![u64::MAX; WeightedTable::MAX_ENTRIES + 1],
                WeightValidationError::Capacity {
                    len: WeightedTable::MAX_ENTRIES + 1,
                    max: WeightedTable::MAX_ENTRIES,
                },
                WeightError::Capacity,
            ),
            (
                vec![u64::MAX, 1, 1],
                WeightValidationError::Overflow {
                    index: 1,
                    partial_total: u64::MAX,
                    weight: 1,
                },
                WeightError::Overflow,
            ),
            (
                vec![2, 0, u64::MAX - 2, 7, 9],
                WeightValidationError::Overflow {
                    index: 3,
                    partial_total: u64::MAX,
                    weight: 7,
                },
                WeightError::Overflow,
            ),
        ];
        for (weights, detailed, category) in cases {
            assert_eq!(WeightedTable::validate(&weights), Err(detailed));
            assert_eq!(WeightedTable::try_new(&weights).unwrap_err(), detailed);
            assert_eq!(WeightedTable::new(&weights).unwrap_err(), category);
            assert_eq!(WeightError::from(detailed), category);
        }
    }

    #[test]
    fn diagnostics_are_actionable_standard_errors() {
        fn message(error: &dyn std::error::Error) -> String {
            error.to_string()
        }
        assert!(message(&WeightError::Empty).contains("at least one positive weight"));
        assert!(message(&WeightError::Capacity).contains("4096"));
        assert!(message(&WeightError::ZeroTotal).contains("above zero"));
        assert!(message(&WeightError::Overflow).contains("reduce the weights"));
        assert_eq!(
            message(&WeightValidationError::Capacity {
                len: 4097,
                max: 4096,
            }),
            "received 4097 weights; provide at most 4096 entries (including zero weights)"
        );
        assert_eq!(
            message(&WeightValidationError::Overflow {
                index: 3,
                partial_total: u64::MAX,
                weight: 7,
            }),
            "weight at index 3 (7) added to the preceding total (18446744073709551615) \
             exceeds u64::MAX; reduce the weights"
        );
    }

    #[test]
    fn maximum_entry_count_and_total_are_accepted_without_compacting_zero_weights() {
        assert_eq!(WeightedTable::MAX_ENTRIES, 4096);
        let mut weights = vec![0; WeightedTable::MAX_ENTRIES];
        assert_eq!(
            WeightedTable::validate(&weights),
            Err(WeightValidationError::ZeroTotal)
        );
        let last = weights.len() - 1;
        weights[last] = u64::MAX;
        assert_eq!(WeightedTable::validate(&weights), Ok(u64::MAX));
        let table = WeightedTable::try_new(&weights).unwrap();
        assert_eq!(table.total(), u64::MAX);
        assert_eq!(table.ticket(0), Some(last));
        assert_eq!(table.ticket(u64::MAX - 1), Some(last));
        assert_eq!(table.ticket(u64::MAX), None);
        assert_eq!(table.sample(&mut Random::new(0)), last);
        assert_eq!(try_sample(&weights, &mut Random::new(0)), Ok(last));

        let table = WeightedTable::try_new(&[0, u64::MAX - 1, 0, 1, 0]).unwrap();
        assert_eq!(table.ticket(u64::MAX - 2), Some(1));
        assert_eq!(table.ticket(u64::MAX - 1), Some(3));
        assert_eq!(table.ticket(u64::MAX), None);
    }

    #[test]
    fn rejected_selection_never_advances_the_stream() {
        for weights in [
            vec![],
            vec![0; WeightedTable::MAX_ENTRIES + 1],
            vec![0; WeightedTable::MAX_ENTRIES],
            vec![u64::MAX, 1],
            vec![1, 0, u64::MAX],
        ] {
            let mut random = Random::new(42);
            let mut expected = random.clone();
            assert_eq!(
                try_sample(&weights, &mut random),
                Err(WeightedTable::validate(&weights).unwrap_err())
            );
            // A later valid operation must behave as if the failed draw never happened.
            assert_eq!(
                try_sample(&[1, 2, 3], &mut random),
                try_sample(&[1, 2, 3], &mut expected)
            );
            assert_eq!(random.next_u64(), expected.next_u64());
        }
    }

    #[test]
    fn seeded_selection_and_subsequent_draw_match_the_original_stream() {
        let weights = [0, 2, 0, 3, 2, 0];
        let legacy = WeightedTable::new(&weights).unwrap();
        let prepared = WeightedTable::try_new(&weights).unwrap();
        let mut legacy_random = Random::new(42);
        let mut prepared_random = legacy_random.clone();
        let mut direct_random = legacy_random.clone();
        for expected in [4, 4, 1, 3, 4, 3, 3, 4, 4, 4, 4, 4, 1, 3, 1, 4] {
            assert_eq!(legacy.sample(&mut legacy_random), expected);
            assert_eq!(prepared.sample(&mut prepared_random), expected);
            assert_eq!(try_sample(&weights, &mut direct_random), Ok(expected));
        }
        for random in [&mut legacy_random, &mut prepared_random, &mut direct_random] {
            assert_eq!(random.next_u64(), 0x1a83d752f35eba75);
        }
    }

    #[test]
    fn rejection_sampling_and_single_choice_preserve_draw_consumption() {
        // This total rejects nearly half the raw draws. It must not be replaced
        // with modulo, multiply-high mapping, or a different prepared sampler.
        let weights = [0, 1 << 62, 0, (1 << 62) + 1, 0];
        let prepared = WeightedTable::try_new(&weights).unwrap();
        let mut random = Random::new(0);
        let mut direct_random = random.clone();
        let mut reference = random.clone();
        for _ in 0..16 {
            let ticket = reference.below((1 << 63) + 1).unwrap();
            let expected = if ticket < 1 << 62 { 1 } else { 3 };
            assert_eq!(prepared.sample(&mut random), expected);
            assert_eq!(try_sample(&weights, &mut direct_random), Ok(expected));
        }
        // Sixteen accepted tickets consume 26 raw draws for this seed and bound.
        assert_eq!(reference.next_u64(), 0x69b82ebc92233300);
        assert_eq!(random.next_u64(), 0x69b82ebc92233300);
        assert_eq!(direct_random.next_u64(), 0x69b82ebc92233300);

        for weights in [&[1][..], &[0, 1, 0], &[0, u64::MAX, 0]] {
            let table = WeightedTable::try_new(weights).unwrap();
            let only_index = weights.iter().position(|&weight| weight > 0).unwrap();
            let mut random = Random::new(42);
            let mut direct_random = random.clone();
            for _ in 0..16 {
                assert_eq!(table.sample(&mut random), only_index);
                assert_eq!(try_sample(weights, &mut direct_random), Ok(only_index));
            }
            assert_eq!(random.next_u64(), 0x1a83d752f35eba75);
            assert_eq!(direct_random.next_u64(), 0x1a83d752f35eba75);
        }
    }

    #[test]
    fn exhaustive_small_distributions_preserve_every_ticket_and_selection() {
        for encoding in 1..256 {
            // Every nonzero four-entry distribution with weights in 0..=3.
            let weights: [u64; 4] = std::array::from_fn(|i| (encoding >> (i * 2)) & 3);
            let total: u64 = weights.iter().sum();
            assert_eq!(WeightedTable::validate(&weights), Ok(total));
            let prepared = WeightedTable::try_new(&weights).unwrap();
            let mut ticket = 0;
            for (index, &weight) in weights.iter().enumerate() {
                for _ in 0..weight {
                    assert_eq!(prepared.ticket(ticket), Some(index));
                    ticket += 1;
                }
            }
            assert_eq!(prepared.ticket(total), None);
            assert_eq!(prepared.ticket(u64::MAX), None);

            let mut random = Random::new(encoding);
            let mut direct_random = random.clone();
            for _ in 0..32 {
                assert_eq!(
                    try_sample(&weights, &mut direct_random),
                    Ok(prepared.sample(&mut random))
                );
            }
            assert_eq!(random.next_u64(), direct_random.next_u64());
        }
    }

    #[test]
    fn prepared_distribution_retains_its_weights_after_source_changes() {
        let mut weights = [0, 2, 0, 3, 0];
        let table = WeightedTable::try_new(&weights).unwrap();
        weights.fill(0);
        assert!(WeightedTable::validate(&weights).is_err());
        assert_eq!(table.total(), 5);
        assert_eq!(table.ticket(0), Some(1));
        assert_eq!(table.ticket(4), Some(3));
        let mut random = Random::new(42);
        for _ in 0..32 {
            assert!(matches!(table.sample(&mut random), 1 | 3));
        }
    }
}
