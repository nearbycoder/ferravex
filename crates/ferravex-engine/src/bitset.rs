//! Dense dirty/visibility masks with checked indices and ascending iteration.
//!
//! The caller chooses an immutable bit bound with [`BitSet::new`](crate::bitset::BitSet::new). All binary
//! operations require exactly equal [`BitSet::bit_len`](crate::bitset::BitSet::bit_len) values, even when the
//! backing word counts match. A mismatch returns `false` from in-place operations
//! or `None` from lazy operations, without changing either input. Equal empty
//! bounds are valid. No operation resizes a set or exposes unused trailing bits.
//!
//! Construction and cloning allocate storage. Updates and iterators allocate
//! nothing: algebra works a word at a time and iteration visits only set bits
//! within each word. Lazy algebra borrows both sets instead of building a third
//! mask. Collecting an iterator into a `Vec` is a caller-owned allocation.
//!
//! ```
//! use ferravex_engine::bitset::BitSet;
//!
//! let mut dirty = BitSet::new(65);
//! dirty.set(0, true);
//! dirty.set(64, true);
//! let mut visible = BitSet::new(65);
//! visible.set(64, true);
//!
//! // Process matching indices directly, without a temporary mask or buffer.
//! let mut updates = 0;
//! for index in dirty.intersection(&visible).expect("matching bit bounds") {
//!     assert_eq!(index, 64);
//!     updates += 1;
//! }
//! assert_eq!(updates, 1);
//! assert!(dirty.difference_with(&visible));
//! assert!(dirty.iter().eq([0]));
//! ```

/// A fixed-bound dense set of indices in `0..self.bit_len()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitSet {
    words: Vec<u64>,
    bits: usize,
}
impl BitSet {
    /// Creates an empty set with `bits` addressable indices, allocating its storage.
    pub fn new(bits: usize) -> Self {
        Self {
            words: vec![0; bits.div_ceil(64)],
            bits,
        }
    }

    /// Returns the exclusive index bound, not the number of set bits ([`Self::count`]).
    ///
    /// ```
    /// use ferravex_engine::bitset::BitSet;
    /// let set = BitSet::new(65);
    /// assert_eq!(set.bit_len(), 65);
    /// assert_eq!(set.count(), 0);
    /// assert_eq!(BitSet::new(0).bit_len(), 0);
    /// ```
    pub fn bit_len(&self) -> usize {
        self.bits
    }

    /// Sets or clears an index. Returns `false` without changing the set if out of bounds.
    /// A valid index returns `true` even when its value is unchanged.
    pub fn set(&mut self, index: usize, value: bool) -> bool {
        if index >= self.bits {
            return false;
        }
        let mask = 1_u64 << (index % 64);
        if value {
            self.words[index / 64] |= mask;
        } else {
            self.words[index / 64] &= !mask;
        }
        true
    }

    /// Tests membership, returning `false` for an out-of-bounds index.
    pub fn contains(&self, index: usize) -> bool {
        index < self.bits && self.words[index / 64] & (1_u64 << (index % 64)) != 0
    }

    /// Counts set bits, excluding unused trailing bits.
    pub fn count(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Clears every bit while retaining the bound and allocated storage.
    pub fn clear(&mut self) {
        self.words.fill(0);
    }

    /// Adds every bit in `other` without allocating.
    ///
    /// Returns `true` for equal bit bounds, including an unchanged result, or
    /// `false` for unequal bounds without modifying `self`.
    ///
    /// ```
    /// use ferravex_engine::bitset::BitSet;
    /// let mut a = BitSet::new(65);
    /// let mut b = BitSet::new(65);
    /// a.set(0, true);
    /// b.set(64, true);
    /// assert!(a.union_with(&b));
    /// assert!(a.iter().eq([0, 64]));
    /// assert!(!a.union_with(&BitSet::new(66)));
    /// assert!(a.iter().eq([0, 64]));
    /// ```
    pub fn union_with(&mut self, other: &Self) -> bool {
        if self.bits != other.bits {
            return false;
        }
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a |= *b;
        }
        true
    }

    /// Keeps only bits also in `other`, without allocating.
    ///
    /// Returns `true` for equal bit bounds, including an unchanged result, or
    /// `false` for unequal bounds without modifying `self`.
    ///
    /// ```
    /// use ferravex_engine::bitset::BitSet;
    /// let mut a = BitSet::new(65);
    /// let mut b = BitSet::new(65);
    /// a.set(0, true);
    /// a.set(64, true);
    /// b.set(64, true);
    /// assert!(a.intersection_with(&b));
    /// assert!(a.iter().eq([64]));
    /// assert!(!a.intersection_with(&BitSet::new(64)));
    /// assert!(a.iter().eq([64]));
    /// ```
    pub fn intersection_with(&mut self, other: &Self) -> bool {
        if self.bits != other.bits {
            return false;
        }
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            *a &= *b;
        }
        true
    }

    /// Removes every bit in `other` from `self`, without allocating.
    ///
    /// Returns `true` for equal bit bounds, including an unchanged result, or
    /// `false` for unequal bounds without modifying `self`. Operand order matters.
    ///
    /// ```
    /// use ferravex_engine::bitset::BitSet;
    /// let mut a = BitSet::new(65);
    /// let mut b = BitSet::new(65);
    /// a.set(0, true);
    /// a.set(64, true);
    /// b.set(64, true);
    /// assert!(a.difference_with(&b));
    /// assert!(a.iter().eq([0]));
    /// assert!(!a.difference_with(&BitSet::new(66)));
    /// assert!(a.iter().eq([0]));
    /// ```
    pub fn difference_with(&mut self, other: &Self) -> bool {
        if self.bits != other.bits {
            return false;
        }
        for (a, b) in self.words.iter_mut().zip(&other.words) {
            // AND with the original word keeps unused trailing bits clear.
            *a &= !*b;
        }
        true
    }

    /// Lazily yields indices present in either set, once each in ascending order.
    ///
    /// Returns `None` if bit bounds differ. Borrows both sets without modifying
    /// or allocating storage; partially consuming the iterator has no side effects.
    ///
    /// ```
    /// use ferravex_engine::bitset::BitSet;
    /// let mut a = BitSet::new(65);
    /// let mut b = BitSet::new(65);
    /// a.set(0, true);
    /// a.set(64, true);
    /// b.set(64, true);
    /// assert!(a.union(&b).unwrap().eq([0, 64]));
    /// assert!(a.union(&BitSet::new(66)).is_none());
    /// ```
    pub fn union<'a>(&'a self, other: &'a Self) -> Option<impl Iterator<Item = usize> + 'a> {
        (self.bits == other.bits)
            .then(|| set_bits(self.words.iter().zip(&other.words).map(|(a, b)| *a | *b)))
    }

    /// Lazily yields indices present in both sets in ascending order.
    ///
    /// Returns `None` if bit bounds differ. Borrows both sets without modifying
    /// or allocating storage; partially consuming the iterator has no side effects.
    ///
    /// ```
    /// use ferravex_engine::bitset::BitSet;
    /// let mut a = BitSet::new(65);
    /// let mut b = BitSet::new(65);
    /// a.set(0, true);
    /// a.set(64, true);
    /// b.set(64, true);
    /// assert!(a.intersection(&b).unwrap().eq([64]));
    /// assert!(a.intersection(&BitSet::new(66)).is_none());
    /// ```
    pub fn intersection<'a>(&'a self, other: &'a Self) -> Option<impl Iterator<Item = usize> + 'a> {
        (self.bits == other.bits)
            .then(|| set_bits(self.words.iter().zip(&other.words).map(|(a, b)| *a & *b)))
    }

    /// Lazily yields indices in `self` but absent from `other` in ascending order.
    ///
    /// Returns `None` if bit bounds differ. Borrows both sets without modifying
    /// or allocating storage; partially consuming the iterator has no side effects.
    /// Operand order matters.
    ///
    /// ```
    /// use ferravex_engine::bitset::BitSet;
    /// let mut a = BitSet::new(65);
    /// let mut b = BitSet::new(65);
    /// a.set(0, true);
    /// a.set(64, true);
    /// b.set(64, true);
    /// assert!(a.difference(&b).unwrap().eq([0]));
    /// assert!(b.difference(&a).unwrap().next().is_none());
    /// assert!(a.difference(&BitSet::new(66)).is_none());
    /// ```
    pub fn difference<'a>(&'a self, other: &'a Self) -> Option<impl Iterator<Item = usize> + 'a> {
        (self.bits == other.bits)
            .then(|| set_bits(self.words.iter().zip(&other.words).map(|(a, b)| *a & !*b)))
    }

    /// Yields set indices in ascending order without allocating or changing the set.
    /// Empty words are skipped, and unused trailing bits are never yielded.
    ///
    /// ```
    /// use ferravex_engine::bitset::BitSet;
    /// let mut set = BitSet::new(65);
    /// set.set(64, true);
    /// set.set(0, true);
    /// assert!(set.iter().eq([0, 64]));
    /// assert_eq!(set.count(), 2);
    /// ```
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        set_bits(self.words.iter().copied())
    }
}

fn set_bits(words: impl Iterator<Item = u64>) -> impl Iterator<Item = usize> {
    words.enumerate().flat_map(|(word, mut remaining)| {
        std::iter::from_fn(move || {
            if remaining == 0 {
                return None;
            }
            let bit = remaining.trailing_zeros() as usize;
            remaining &= remaining - 1;
            Some(word * 64 + bit)
        })
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn word_edges_set_algebra_and_no_padding_bits() {
        let mut a = BitSet::new(65);
        for i in [0, 63, 64] {
            assert!(a.set(i, true));
        }
        assert!(!a.set(65, true));
        assert!(!a.contains(usize::MAX));
        assert_eq!(a.iter().collect::<Vec<_>>(), [0, 63, 64]);
        assert_eq!(a.count(), 3);
        let mut b = BitSet::new(65);
        b.set(64, true);
        assert!(a.intersection_with(&b));
        assert_eq!(a.iter().collect::<Vec<_>>(), [64]);
        b.set(1, true);
        a.union_with(&b);
        assert_eq!(a.iter().collect::<Vec<_>>(), [1, 64]);
        let before = a.clone();
        assert!(!a.union_with(&BitSet::new(64)));
        assert_eq!(a, before);
        a.clear();
        assert_eq!(a.count(), 0);
        assert_eq!(BitSet::new(0).iter().count(), 0);
    }

    #[test]
    fn algebra_matches_membership_oracle_for_every_trailing_length() {
        // Covers empty sets, full words and every partial-word size, including
        // more than one complete word before the tail.
        for bits in 0..=193 {
            for pattern in 0..4 {
                let mut a = BitSet::new(bits);
                let mut b = BitSet::new(bits);
                let mut union = Vec::new();
                let mut intersection = Vec::new();
                let mut difference = Vec::new();
                let mut reverse_difference = Vec::new();
                for i in 0..bits {
                    let left = match pattern {
                        0 => false,
                        1 => true,
                        _ => i % 3 == 0,
                    };
                    let right = match pattern {
                        0 => true,
                        1 => false,
                        2 => i % 5 == 0,
                        _ => i % 3 != 0,
                    };
                    assert!(a.set(i, left));
                    assert!(b.set(i, right));
                    if left || right {
                        union.push(i);
                    }
                    if left && right {
                        intersection.push(i);
                    }
                    if left && !right {
                        difference.push(i);
                    }
                    if right && !left {
                        reverse_difference.push(i);
                    }
                }

                assert_eq!(a.bit_len(), bits);
                assert!(a.union(&b).unwrap().eq(union.iter().copied()));
                assert!(b.union(&a).unwrap().eq(union.iter().copied()));
                assert!(a.intersection(&b).unwrap().eq(intersection.iter().copied()));
                assert!(b.intersection(&a).unwrap().eq(intersection.iter().copied()));
                assert!(a.difference(&b).unwrap().eq(difference.iter().copied()));
                assert!(b.difference(&a).unwrap().eq(reverse_difference));

                let operations: [fn(&mut BitSet, &BitSet) -> bool; 3] = [
                    BitSet::union_with,
                    BitSet::intersection_with,
                    BitSet::difference_with,
                ];
                for (operation, expected) in operations.into_iter().zip([
                    union.as_slice(),
                    intersection.as_slice(),
                    difference.as_slice(),
                ]) {
                    let mut actual = a.clone();
                    let storage = actual.words.as_ptr();
                    let capacity = actual.words.capacity();
                    assert!(operation(&mut actual, &b));
                    assert!(actual.iter().eq(expected.iter().copied()));
                    assert_eq!(actual.count(), expected.len());
                    assert_eq!(actual.bit_len(), bits);
                    assert_eq!(actual.words.as_ptr(), storage);
                    assert_eq!(actual.words.capacity(), capacity);
                    assert!(!actual.contains(bits));
                    assert!(!actual.contains(usize::MAX));
                    if bits % 64 != 0 {
                        assert_eq!(actual.words.last().unwrap() >> (bits % 64), 0);
                    }
                    // Every operation is idempotent when applied again with b.
                    assert!(operation(&mut actual, &b));
                    assert!(actual.iter().eq(expected.iter().copied()));
                }
            }
        }
    }

    #[test]
    fn unequal_bit_bounds_are_rejected_atomically_even_with_equal_word_counts() {
        for (left, right) in [
            (0, 1),
            (1, 0),
            (1, 63),
            (63, 1),
            (63, 64),
            (64, 63),
            (64, 65),
            (65, 64),
            (65, 127),
            (127, 65),
        ] {
            let mut a = BitSet::new(left);
            let mut b = BitSet::new(right);
            for i in 0..left {
                a.set(i, i % 3 == 0);
            }
            for i in 0..right {
                b.set(i, true);
            }
            let before_a = a.clone();
            let before_b = b.clone();
            assert!(!a.union_with(&b));
            assert_eq!(a, before_a);
            assert!(!a.intersection_with(&b));
            assert_eq!(a, before_a);
            assert!(!a.difference_with(&b));
            assert_eq!(a, before_a);
            assert!(a.union(&b).is_none());
            assert!(a.intersection(&b).is_none());
            assert!(a.difference(&b).is_none());
            assert_eq!(b, before_b);

            a.clear();
            b.clear();
            assert!(a.union(&b).is_none());
            assert!(a.intersection(&b).is_none());
            assert!(a.difference(&b).is_none());
        }
    }

    #[test]
    fn identical_and_empty_operands_obey_set_laws() {
        for bits in [0, 1, 63, 64, 65, 127, 128, 129] {
            let mut a = BitSet::new(bits);
            for i in (0..bits).step_by(7) {
                a.set(i, true);
            }
            let empty = BitSet::new(bits);
            assert!(a.union(&a).unwrap().eq(a.iter()));
            assert!(a.intersection(&a).unwrap().eq(a.iter()));
            assert!(a.difference(&a).unwrap().next().is_none());
            assert!(a.union(&empty).unwrap().eq(a.iter()));
            assert!(a.intersection(&empty).unwrap().next().is_none());
            assert!(a.difference(&empty).unwrap().eq(a.iter()));
            assert!(empty.difference(&a).unwrap().next().is_none());

            let mut same = a.clone();
            assert!(same.union_with(&a));
            assert_eq!(same, a);
            assert!(same.intersection_with(&a));
            assert_eq!(same, a);
            assert!(same.difference_with(&a));
            assert_eq!(same, empty);
        }
    }

    #[test]
    fn sparse_iteration_skips_zero_words_and_can_stop_early_without_mutation() {
        let mut a = BitSet::new(1025);
        let mut b = BitSet::new(1025);
        for i in [63, 192, 1024] {
            a.set(i, true);
        }
        for i in [64, 1024] {
            b.set(i, true);
        }
        let before_a = a.clone();
        let before_b = b.clone();
        assert_eq!(a.union(&b).unwrap().next(), Some(63));
        assert_eq!(a.intersection(&b).unwrap().next(), Some(1024));
        assert_eq!(a.difference(&b).unwrap().next(), Some(63));
        assert_eq!(a, before_a);
        assert_eq!(b, before_b);
        assert!(a.iter().eq([63, 192, 1024]));
        assert!(a.union(&b).unwrap().eq([63, 64, 192, 1024]));
        assert!(a.intersection(&b).unwrap().eq([1024]));
        assert!(a.difference(&b).unwrap().eq([63, 192]));

        let mut iter = a.union(&b).unwrap();
        assert_eq!(iter.next(), Some(63));
        assert_eq!(iter.nth(1), Some(192));
        assert_eq!(iter.next(), Some(1024));
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
        assert_eq!(iter.next(), None);
    }
}
