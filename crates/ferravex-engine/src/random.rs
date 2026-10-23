//! Reproducible SplitMix64 streams for engine variation, never cryptography.
//! Forks depend on the original seed and stream ID, independent of parent draws.
//!
//! Give each subsystem a stable name and keep its child stream between updates:
//!
//! ```
//! use ferravex_engine::random::Random;
//!
//! let root = Random::new(42);
//! let mut loot = root.fork_named("loot");
//! let mut enemy = root.fork_tagged("enemies", 7); // Stable entity ID.
//! let coins = loot.try_range_u64(2..6).unwrap();
//! let offset = enemy.try_range_i64(-3..4).unwrap();
//! let speed = enemy.try_range_f32(1.0..2.0).unwrap();
//! assert!((2..6).contains(&coins));
//! assert!((-3..4).contains(&offset));
//! assert!((1.0..2.0).contains(&speed));
//! ```
//!
//! Integer range sampling is allocation-free. The half-open methods exclude the
//! upper bound; [`Random::try_range_u64_inclusive`](crate::random::Random::try_range_u64_inclusive) and
//! [`Random::try_range_i64_inclusive`](crate::random::Random::try_range_i64_inclusive) also support the maximum integer and the
//! entire 64-bit domain. Invalid ranges consume no draws. Valid singletons and
//! full-width ranges consume exactly one draw; other widths use the existing
//! rejection sampler, whose draw count has no fixed cap.
//!
//! # Seed and version contract
//!
//! [`Random::VERSION`](crate::random::Random::VERSION) is 1. The original seed, numeric [`Random::fork`](crate::random::Random::fork),
//! SplitMix64 draw sequence, rejection sampling and 24-bit unit float mapping
//! are unchanged. Named forks hash the exact UTF-8 bytes with 64-bit FNV-1a
//! (offset `0xcbf29ce484222325`, prime `0x100000001b3`, wrapping arithmetic),
//! then pass that hash to the original numeric fork. Tagged forks apply a
//! numeric fork to the named child. No platform hasher, byte order, process
//! state, allocation or parent draw count participates in seed derivation.
//!
//! Names are case-sensitive and are not Unicode-normalized; an empty name is
//! valid. Names are hashed to 64 bits, so distinct names can collide. Renaming
//! a subsystem or changing its stable tag intentionally changes its sequence.
//! Repeating a fork restarts that child; it does not resume previous draws.
//! Hierarchical names use successive forks, not separator parsing:
//! `root.fork_named("world").fork_named("loot")` differs from
//! `root.fork_named("world/loot")`. Persist the version, seed and exact fork
//! path when recording how to recreate a stream. No save format is introduced;
//! [`Clone::clone`] provides an in-memory checkpoint of a stream's current draws.

use std::ops::{Range, RangeInclusive};

/// A seeded random stream with reproducible, independently derived children.
#[derive(Clone, Debug)]
pub struct Random {
    seed: u64,
    state: u64,
}
fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}
impl Random {
    /// Version of the seed, fork and sampling algorithms documented by this module.
    /// Any incompatible change requires a new version; version 1 retains the
    /// original numeric streams.
    pub const VERSION: u32 = 1;

    /// Start a stream from any 64-bit seed, including zero.
    pub fn new(seed: u64) -> Self {
        Self { seed, state: seed }
    }
    /// Derive a child from this stream's original seed and a stable numeric ID.
    /// Neither reads nor advances the current draw position.
    pub fn fork(&self, stream: u64) -> Self {
        Self::new(mix(self.seed ^ mix(stream.wrapping_add(0x9e3779b97f4a7c15))))
    }
    /// Derive a child from a stable, case-sensitive UTF-8 name without consuming
    /// any draws. See the module's version contract for the fixed hash algorithm.
    ///
    /// ```
    /// use ferravex_engine::random::Random;
    /// let mut root = Random::new(17);
    /// let mut expected = root.fork_named("loot");
    /// root.next_u64();
    /// assert_eq!(expected.next_u64(), root.fork_named("loot").next_u64());
    /// ```
    pub fn fork_named(&self, name: &str) -> Self {
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in name.bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        self.fork(hash)
    }

    /// Derive an independently replayable instance within a named subsystem.
    /// Equivalent to `self.fork_named(name).fork(tag)`; use stable IDs rather
    /// than order-dependent counters if instance creation order can change.
    ///
    /// ```
    /// use ferravex_engine::random::Random;
    /// let root = Random::new(17);
    /// let mut enemy = root.fork_tagged("enemies", 42);
    /// let mut replay = root.fork_named("enemies").fork(42);
    /// assert_eq!(enemy.next_u64(), replay.next_u64());
    /// ```
    pub fn fork_tagged(&self, name: &str, tag: u64) -> Self {
        self.fork_named(name).fork(tag)
    }

    /// Advance the stream and return the next SplitMix64 output.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        mix(self.state)
    }
    /// Uniform integer in [0, upper). Rejection avoids modulo bias. Zero returns None.
    pub fn below(&mut self, upper: u64) -> Option<u64> {
        if upper == 0 {
            return None;
        }
        let threshold = upper.wrapping_neg() % upper;
        loop {
            let n = self.next_u64();
            if n >= threshold {
                return Some(n % upper);
            }
        }
    }

    /// Uniform integer in the half-open range, using rejection to avoid bias.
    /// Empty or reversed ranges return `None` without consuming a draw.
    ///
    /// ```
    /// use ferravex_engine::random::Random;
    /// let mut rng = Random::new(5);
    /// assert_eq!(rng.try_range_u64(9..10), Some(9));
    /// assert_eq!(rng.try_range_u64(9..9), None);
    /// ```
    pub fn try_range_u64(&mut self, range: Range<u64>) -> Option<u64> {
        let width = range.end.checked_sub(range.start)?;
        self.below(width).map(|offset| range.start + offset)
    }

    /// Uniform signed integer in the half-open range, including ranges that
    /// cross zero or whose width exceeds `i64::MAX`. Empty or reversed ranges
    /// return `None` without consuming a draw.
    ///
    /// ```
    /// use ferravex_engine::random::Random;
    /// let mut rng = Random::new(5);
    /// let value = rng.try_range_i64(i64::MIN..i64::MAX).unwrap();
    /// assert!((i64::MIN..i64::MAX).contains(&value));
    /// ```
    pub fn try_range_i64(&mut self, range: Range<i64>) -> Option<i64> {
        if range.start >= range.end {
            return None;
        }
        // Unsigned wrapping arithmetic represents the full signed range width.
        let width = (range.end as u64).wrapping_sub(range.start as u64);
        self.below(width)
            .map(|offset| (range.start as u64).wrapping_add(offset) as i64)
    }

    /// Uniform integer including both bounds, using rejection to avoid bias.
    /// Reversed or exhausted ranges return `None` without consuming a draw.
    /// A partially iterated range samples only its remaining values.
    ///
    /// A singleton consumes exactly one draw. The full-width `0..=u64::MAX`
    /// returns one unmodified [`Self::next_u64`] draw; its `2^64` values are not
    /// treated as a zero-width range. Every other range uses [`Self::below`] for
    /// its width, preserving the output and draw count of the equivalent
    /// half-open [`Self::try_range_u64`] range when that range is representable.
    ///
    /// ```
    /// use ferravex_engine::random::Random;
    /// let mut rng = Random::new(5);
    /// assert_eq!(rng.try_range_u64_inclusive(u64::MAX..=u64::MAX), Some(u64::MAX));
    /// let mut exhausted = 1..=1;
    /// assert_eq!(exhausted.next(), Some(1));
    /// let mut expected = rng.clone();
    /// assert_eq!(rng.try_range_u64_inclusive(exhausted), None);
    /// assert_eq!(rng.try_range_u64_inclusive(0..=u64::MAX), Some(expected.next_u64()));
    /// assert_eq!(rng.next_u64(), expected.next_u64());
    /// ```
    pub fn try_range_u64_inclusive(&mut self, range: RangeInclusive<u64>) -> Option<u64> {
        if range.is_empty() {
            return None;
        }
        let (start, end) = range.into_inner();
        match (end - start).checked_add(1) {
            Some(width) => self.below(width).map(|offset| start + offset),
            // Only the entire u64 domain has a width that cannot fit in u64.
            None => Some(self.next_u64()),
        }
    }

    /// Uniform signed integer including both bounds, using rejection to avoid
    /// bias even when crossing zero or spanning more than `i64::MAX` values.
    /// Reversed or exhausted ranges return `None` without consuming a draw.
    /// A partially iterated range samples only its remaining values.
    ///
    /// A singleton consumes exactly one draw. The full-width
    /// `i64::MIN..=i64::MAX` adds one raw [`Self::next_u64`] draw to `i64::MIN`
    /// as an unsigned offset (equivalently, flip the draw's high bit and cast to
    /// `i64`). Other widths retain the output and draw count of the equivalent
    /// half-open [`Self::try_range_i64`] range when it is representable.
    ///
    /// ```
    /// use ferravex_engine::random::Random;
    /// let mut rng = Random::new(5);
    /// assert_eq!(rng.try_range_i64_inclusive(i64::MAX..=i64::MAX), Some(i64::MAX));
    /// let offset = rng.try_range_i64_inclusive(-3..=3).unwrap();
    /// assert!((-3..=3).contains(&offset));
    /// let mut expected = rng.clone();
    /// assert_eq!(
    ///     rng.try_range_i64_inclusive(i64::MIN..=i64::MAX),
    ///     Some((expected.next_u64() ^ (1 << 63)) as i64),
    /// );
    /// assert_eq!(rng.next_u64(), expected.next_u64());
    /// ```
    pub fn try_range_i64_inclusive(&mut self, range: RangeInclusive<i64>) -> Option<i64> {
        if range.is_empty() {
            return None;
        }
        let (start, end) = range.into_inner();
        // Unsigned wrapping subtraction represents even the full signed span.
        let last_offset = (end as u64).wrapping_sub(start as u64);
        self.try_range_u64_inclusive(0..=last_offset)
            .map(|offset| (start as u64).wrapping_add(offset) as i64)
    }

    /// Scale one 24-bit unit sample into a finite, half-open float range.
    /// Empty, reversed or non-finite bounds return `None` without consuming a
    /// draw. Uses `f64` intermediates to support even `-f32::MAX..f32::MAX`, and
    /// clamps rounding at the exclusive upper bound to its preceding float.
    /// This is a scaled unit distribution, not uniform over representable floats.
    ///
    /// ```
    /// use ferravex_engine::random::Random;
    /// let mut rng = Random::new(5);
    /// let speed = rng.try_range_f32(0.5..2.0).unwrap();
    /// assert!((0.5..2.0).contains(&speed));
    /// assert_eq!(rng.try_range_f32(0.0..f32::INFINITY), None);
    /// ```
    pub fn try_range_f32(&mut self, range: Range<f32>) -> Option<f32> {
        if !range.start.is_finite() || !range.end.is_finite() || range.start >= range.end {
            return None;
        }
        let start = f64::from(range.start);
        let width = f64::from(range.end) - start;
        let value = (start + width * f64::from(self.unit_f32())) as f32;
        Some(value.max(range.start).min(range.end.next_down()))
    }

    /// Exactly 24 random mantissa bits in [0,1); independent of platform libm.
    pub fn unit_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / 16777216.0
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn draws(mut random: Random) -> [u64; 4] {
        std::array::from_fn(|_| random.next_u64())
    }

    #[test]
    fn version_one_numeric_forks_and_wrapping_seeds_remain_compatible() {
        assert_eq!(Random::VERSION, 1);
        assert_eq!(
            draws(Random::new(u64::MAX)),
            [
                0xe4d971771b652c20,
                0xe99ff867dbf682c9,
                0x382ff84cb27281e9,
                0x6d1db36ccba982d2,
            ]
        );
        assert_eq!(
            draws(Random::new(0).fork(7)),
            [
                0xf33dc6bd55ffa86b,
                0xe1332a7db412c5a9,
                0xe6af094f768935b3,
                0x0fdf2d08f5c29727,
            ]
        );
        let mut random = Random::new(42);
        for expected in [5, 5, 0, 2, 6, 4, 2, 6] {
            assert_eq!(random.below(7), Some(expected));
        }
        let mut random = Random::new(42);
        for expected in [
            0x3f3dd732, 0x3e23bf8c, 0x3e8ea4ce, 0x3eb039c2, 0x3d1bc580, 0x3f5e4431, 0x3e5fa59c,
            0x3f4cf635,
        ] {
            assert_eq!(random.unit_f32().to_bits(), expected);
        }
    }

    #[test]
    fn named_and_tagged_streams_have_fixed_utf8_vectors() {
        let root = Random::new(42);
        for (name, first) in [
            ("", 0xe0935f776843f237),
            ("loot", 0xf9e0d6ebb0a83ede),
            ("敵", 0x99765e69080f4113),
            ("é", 0x53ab402895a9d08d),
            ("e\u{301}", 0x70a35478dc8af1a8),
        ] {
            assert_eq!(root.fork_named(name).next_u64(), first, "{name:?}");
        }
        assert_eq!(
            draws(root.fork_tagged("enemies", 7)),
            [
                0xb8f0a5c94abdb803,
                0x19db673b5f53317c,
                0x2b6918f11a814234,
                0xff3ff53ee0a79222,
            ]
        );
        assert_eq!(
            root.fork_named("world").fork_named("loot").next_u64(),
            0xba521209acc673b5
        );
        assert_ne!(
            draws(root.fork_named("world").fork_named("loot")),
            draws(root.fork_named("world/loot"))
        );
        assert_ne!(
            draws(root.fork_named("loot")),
            draws(root.fork_named("Loot"))
        );
        for tag in [0, 7, u64::MAX] {
            assert_eq!(
                draws(root.fork_tagged("enemies", tag)),
                draws(root.fork_named("enemies").fork(tag))
            );
            assert_ne!(
                draws(root.fork_tagged("enemies", tag)),
                draws(root.fork_tagged("enemies", tag.wrapping_add(1)))
            );
        }
    }

    #[test]
    fn unrelated_subsystems_and_creation_order_do_not_change_loot_or_enemy_draws() {
        let mut root = Random::new(42);
        let original = root.clone();
        let mut expected_loot = original.fork_named("loot");
        let mut expected_enemy = original.fork_tagged("enemies", 7);
        let mut particles = root.fork_named("particles");
        let mut other_enemy = root.fork_tagged("enemies", 8);
        let mut enemy = root.fork_tagged("enemies", 7);
        let mut loot = root.fork_named("loot");
        assert_eq!(draws(root.clone()), draws(original.clone()));
        for tick in 0..128 {
            root.next_u64();
            for _ in 0..tick {
                particles.next_u64();
                other_enemy.next_u64();
            }
            assert_eq!(loot.next_u64(), expected_loot.next_u64());
            assert_eq!(enemy.next_u64(), expected_enemy.next_u64());
        }
        assert_eq!(
            draws(root.fork_named("loot")),
            draws(original.fork_named("loot"))
        );
        assert_eq!(
            draws(root.fork_tagged("enemies", 7)),
            draws(original.fork_tagged("enemies", 7))
        );
    }

    #[test]
    fn invalid_ranges_never_consume_draws() {
        let mut random = Random::new(42);
        let expected = random.clone();
        assert_eq!(random.below(0), None);
        for (start, end) in [(0, 0), (5, 4), (u64::MAX, 0), (u64::MAX, u64::MAX)] {
            assert_eq!(random.try_range_u64(start..end), None);
        }
        for (start, end) in [(0, 0), (5, -4), (i64::MIN, i64::MIN), (i64::MAX, i64::MIN)] {
            assert_eq!(random.try_range_i64(start..end), None);
        }
        for (start, end) in [
            (0.0, 0.0),
            (-0.0, 0.0),
            (1.0, -1.0),
            (f32::NAN, 1.0),
            (0.0, f32::NAN),
            (f32::NEG_INFINITY, 0.0),
            (0.0, f32::INFINITY),
            (f32::INFINITY, f32::INFINITY),
        ] {
            assert_eq!(random.try_range_f32(start..end), None);
        }
        assert_eq!(draws(random), draws(expected));
    }

    #[test]
    fn integer_ranges_handle_signed_and_unsigned_boundaries() {
        let mut random = Random::new(42);
        for (start, end) in [(0, 1), (1, 7), (0, u64::MAX), (u64::MAX - 1, u64::MAX)] {
            let mut seen = std::collections::BTreeSet::new();
            for _ in 0..1024 {
                let value = random.try_range_u64(start..end).unwrap();
                assert!((start..end).contains(&value));
                seen.insert(value);
            }
            if end - start == 6 {
                assert_eq!(seen, (start..end).collect());
            }
        }
        for (start, end) in [
            (i64::MIN, i64::MIN + 1),
            (i64::MAX - 1, i64::MAX),
            (i64::MIN, i64::MAX),
            (i64::MIN, 0),
            (-8, -2),
            (-3, 4),
        ] {
            let mut seen = std::collections::BTreeSet::new();
            for _ in 0..1024 {
                let value = random.try_range_i64(start..end).unwrap();
                assert!((start..end).contains(&value));
                seen.insert(value);
            }
            if (end as i128) - (start as i128) <= 7 {
                assert_eq!(seen, (start..end).collect());
            }
        }
    }

    #[test]
    fn float_ranges_remain_finite_and_exclude_upper_bound_after_rounding() {
        let mut random = Random::new(42);
        let tiny = f32::from_bits(1);
        for (start, end) in [
            (-f32::MAX, f32::MAX),
            (-f32::MAX, -f32::MAX / 2.0),
            (f32::MAX / 2.0, f32::MAX),
            (-3.0, 4.0),
            (1.0, 1.0_f32.next_up()),
            (-1.0, (-1.0_f32).next_up()),
            (0.0, tiny),
            (-tiny, 0.0),
        ] {
            for _ in 0..1024 {
                let value = random.try_range_f32(start..end).unwrap();
                assert!(value.is_finite());
                assert!(
                    (start..end).contains(&value),
                    "{value} outside {start}..{end}"
                );
            }
        }
    }

    #[test]
    fn checked_ranges_use_the_existing_sampling_sequence() {
        let mut random = Random::new(42);
        let mut reference = random.clone();
        for _ in 0..128 {
            // A width above 2^63 exercises rejection as well as direct draws.
            let width = (1 << 63) + 1;
            assert_eq!(
                random.try_range_u64(4..4 + width),
                reference.below(width).map(|v| v + 4)
            );
            assert_eq!(
                random.try_range_i64(-3..4),
                reference.below(7).map(|v| v as i64 - 3)
            );
            assert_eq!(random.try_range_f32(0.0..1.0), Some(reference.unit_f32()));
        }
        assert_eq!(draws(random), draws(reference));
    }

    #[test]
    fn invalid_inclusive_ranges_never_consume_draws() {
        let mut random = Random::new(42);
        let expected = random.clone();
        for (start, end) in [(1, 0), (u64::MAX, 0), (u64::MAX, u64::MAX - 1)] {
            assert_eq!(random.try_range_u64_inclusive(start..=end), None);
        }
        for (start, end) in [(1, 0), (0, -1), (i64::MAX, i64::MIN)] {
            assert_eq!(random.try_range_i64_inclusive(start..=end), None);
        }
        for value in [0, 1, u64::MAX] {
            let mut exhausted = value..=value;
            assert_eq!(exhausted.next(), Some(value));
            assert_eq!(random.try_range_u64_inclusive(exhausted), None);
            let mut exhausted = value..=value;
            assert_eq!(exhausted.next_back(), Some(value));
            assert_eq!(random.try_range_u64_inclusive(exhausted), None);
        }
        for value in [i64::MIN, -1, 0, i64::MAX] {
            let mut exhausted = value..=value;
            assert_eq!(exhausted.next(), Some(value));
            assert_eq!(random.try_range_i64_inclusive(exhausted), None);
            let mut exhausted = value..=value;
            assert_eq!(exhausted.next_back(), Some(value));
            assert_eq!(random.try_range_i64_inclusive(exhausted), None);
        }
        assert_eq!(draws(random), draws(expected));
    }

    #[test]
    fn inclusive_ranges_sample_only_the_unconsumed_iterator_values() {
        let mut unsigned = u64::MAX - 3..=u64::MAX;
        assert_eq!(unsigned.next(), Some(u64::MAX - 3));
        assert_eq!(unsigned.next_back(), Some(u64::MAX));
        let mut signed = -2..=2;
        assert_eq!(signed.next(), Some(-2));
        assert_eq!(signed.next_back(), Some(2));
        let mut random = Random::new(42);
        let mut expected = random.clone();
        for _ in 0..32 {
            assert_eq!(
                random.try_range_u64_inclusive(unsigned.clone()),
                expected.try_range_u64(u64::MAX - 2..u64::MAX)
            );
            assert_eq!(
                random.try_range_i64_inclusive(signed.clone()),
                expected.try_range_i64(-1..2)
            );
        }
        assert_eq!(draws(random), draws(expected));
    }

    #[test]
    fn inclusive_singletons_consume_exactly_one_draw_including_at_type_limits() {
        let mut random = Random::new(42);
        let mut expected = random.clone();
        for value in [0, 1, 42, u64::MAX] {
            assert_eq!(random.try_range_u64_inclusive(value..=value), Some(value));
            expected.next_u64();
            assert_eq!(draws(random.clone()), draws(expected.clone()));
        }
        for value in [i64::MIN, -1, 0, 42, i64::MAX] {
            assert_eq!(random.try_range_i64_inclusive(value..=value), Some(value));
            expected.next_u64();
            assert_eq!(draws(random.clone()), draws(expected.clone()));
        }
    }

    #[test]
    fn inclusive_full_width_keeps_every_raw_bit_and_consumes_one_draw() {
        // Fixed seeds yielding raw 0 and MAX exercise both full-domain endpoints.
        for (seed, raw) in [(0x61c8864680b583eb, 0), (0x31628af67b2131ab, u64::MAX)] {
            assert_eq!(Random::new(seed).next_u64(), raw);
            assert_eq!(
                Random::new(seed).try_range_u64_inclusive(0..=u64::MAX),
                Some(raw)
            );
            assert_eq!(
                Random::new(seed).try_range_i64_inclusive(i64::MIN..=i64::MAX),
                Some((i128::from(i64::MIN) + i128::from(raw)) as i64)
            );
        }
        for seed in [0, 42, u64::MAX, 0x61c8864680b583eb, 0x31628af67b2131ab] {
            let mut unsigned = Random::new(seed);
            let mut signed = unsigned.clone();
            let mut expected = unsigned.clone();
            for _ in 0..128 {
                let raw = expected.next_u64();
                assert_eq!(unsigned.try_range_u64_inclusive(0..=u64::MAX), Some(raw));
                assert_eq!(
                    signed.try_range_i64_inclusive(i64::MIN..=i64::MAX),
                    Some((i128::from(i64::MIN) + i128::from(raw)) as i64)
                );
            }
            assert_eq!(draws(unsigned), draws(expected.clone()));
            assert_eq!(draws(signed), draws(expected));
        }
    }

    #[test]
    fn inclusive_upper_endpoints_are_reachable_without_overflow() {
        let seed = 0x8ea405cd52dd0fa0;
        assert_eq!(Random::new(seed).next_u64(), 2);
        assert_eq!(
            Random::new(seed).try_range_u64_inclusive(u64::MAX - 2..=u64::MAX),
            Some(u64::MAX)
        );
        assert_eq!(
            Random::new(seed).try_range_i64_inclusive(i64::MAX - 2..=i64::MAX),
            Some(i64::MAX)
        );
        assert_eq!(
            Random::new(seed).try_range_i64_inclusive(i64::MIN..=i64::MIN + 2),
            Some(i64::MIN + 2)
        );
        // Width u64::MAX is representable and distinct from a full-width draw.
        let seed = 0x4f10d6ffa9921217;
        assert_eq!(Random::new(seed).next_u64(), u64::MAX - 1);
        assert_eq!(
            Random::new(seed).try_range_u64_inclusive(1..=u64::MAX),
            Some(u64::MAX)
        );
        assert_eq!(
            Random::new(seed).try_range_i64_inclusive(i64::MIN + 1..=i64::MAX),
            Some(i64::MAX)
        );
        assert_eq!(
            Random::new(seed).try_range_i64_inclusive(i64::MIN..=i64::MAX - 1),
            Some(i64::MAX - 1)
        );
    }

    #[test]
    fn inclusive_ranges_preserve_equivalent_half_open_outputs_and_draw_counts() {
        let mut random = Random::new(42);
        let mut expected = random.clone();
        for start in 0..=16 {
            for end in start..=16 {
                for _ in 0..8 {
                    assert_eq!(
                        random.try_range_u64_inclusive(start..=end),
                        expected.try_range_u64(start..end + 1)
                    );
                    assert_eq!(
                        random.try_range_i64_inclusive(start as i64 - 8..=end as i64 - 8),
                        expected.try_range_i64(start as i64 - 8..end as i64 - 7)
                    );
                }
            }
        }
        for (start, end) in [
            (0, u64::MAX - 1),
            (0, 1 << 63),
            (u64::MAX - 2, u64::MAX - 1),
        ] {
            for _ in 0..32 {
                assert_eq!(
                    random.try_range_u64_inclusive(start..=end),
                    expected.try_range_u64(start..end + 1)
                );
            }
        }
        for (start, end) in [
            (i64::MIN, i64::MAX - 1),
            (i64::MIN, 0),
            (i64::MIN, i64::MIN + 2),
            (-3, i64::MAX - 1),
        ] {
            for _ in 0..32 {
                assert_eq!(
                    random.try_range_i64_inclusive(start..=end),
                    expected.try_range_i64(start..end + 1)
                );
            }
        }
        assert_eq!(draws(random), draws(expected));
    }

    #[test]
    fn inclusive_rejection_boundary_and_subsequent_draw_have_fixed_vectors() {
        // Width 2^63 + 1 rejects raw values below 2^63 - 1. These independently
        // calculated seeds exercise zero, multiple rejections, threshold - 1,
        // the accepted threshold itself, and MAX. Following draws pin consumption.
        for (seed, first, offset, following) in [
            (
                0x61c8864680b583eb,
                0,
                0x6220a8397b1dcdae,
                0x6e789e6aa1b965f4,
            ),
            (
                0x8ea405cd52dd0fa0,
                2,
                0x69d6e4d011291b60,
                0x4dea669499093108,
            ),
            (
                0xea81f8e8c877763a,
                (1 << 63) - 2,
                0x46f2592e01b0c366,
                0x73e255b70d84e748,
            ),
            (
                0x0d4f88b4067761bd,
                (1 << 63) - 1,
                (1 << 63) - 1,
                0xd7d3d3a9b5882e9d,
            ),
            (
                0x31628af67b2131ab,
                u64::MAX,
                (1 << 63) - 2,
                0xc0986a9c933f53d1,
            ),
        ] {
            assert_eq!(Random::new(seed).next_u64(), first);
            let mut unsigned = Random::new(seed);
            let mut signed = unsigned.clone();
            assert_eq!(
                unsigned.try_range_u64_inclusive(4..=(1 << 63) + 4),
                Some(offset + 4)
            );
            assert_eq!(
                signed.try_range_i64_inclusive(i64::MIN..=0),
                Some((i128::from(i64::MIN) + i128::from(offset)) as i64)
            );
            assert_eq!(unsigned.next_u64(), following);
            assert_eq!(signed.next_u64(), following);
        }
    }

    #[test]
    fn known_vector_and_independent_streams() {
        let mut r = Random::new(0);
        assert_eq!(r.next_u64(), 0xe220a8397b1dcdaf);
        assert_eq!(r.next_u64(), 0x6e789e6aa1b965f4);
        let mut child = r.fork(7);
        for _ in 0..100 {
            r.next_u64();
        }
        assert_eq!(child.next_u64(), r.fork(7).next_u64());
        assert_ne!(r.fork(7).next_u64(), r.fork(8).next_u64());
        assert_eq!(r.below(0), None);
        assert_eq!(r.below(1), Some(0));
        for _ in 0..10000 {
            assert!(r.below(7).unwrap() < 7);
            let f = r.unit_f32();
            assert!((0.0..1.0).contains(&f));
        }
    }
}
