//! Reproducible shuffled index cycles for nonrepeating engine variants.
//! Each cycle is a permutation; consecutive cycles may share a boundary value.
use crate::random::Random;
pub struct ShuffleBag {
    items: Vec<usize>,
    cursor: usize,
    random: Random,
}
impl ShuffleBag {
    /// Counts above one million are rejected. Empty bags produce no values.
    pub fn new(count: usize, seed: u64) -> Option<Self> {
        if count > 1_000_000 {
            return None;
        }
        Some(Self {
            items: (0..count).collect(),
            cursor: count,
            random: Random::new(seed),
        })
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
            self.cursor = 0;
        }
        let value = self.items[self.cursor];
        self.cursor += 1;
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
}
