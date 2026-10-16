//! Seeded weighted choices with setup-time validation, fixed storage and no floats.
use super::LoopError;
#[derive(Clone, Copy, Debug)]
pub struct RewardTable<const N: usize> {
    weights: [u32; N],
    total: u32,
}
#[derive(Clone, Copy, Debug)]
pub struct RewardRng(u64);
impl RewardRng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }
    /// SplitMix64; gameplay variety only, never security or matchmaking secrets.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
}
impl<const N: usize> RewardTable<N> {
    pub fn new(weights: [u32; N]) -> Result<Self, LoopError> {
        if N == 0 || N > 256 {
            return Err(LoopError::Invalid);
        }
        let total = weights
            .iter()
            .try_fold(0_u32, |s, w| s.checked_add(*w))
            .ok_or(LoopError::Invalid)?;
        if total == 0 {
            return Err(LoopError::Invalid);
        }
        Ok(Self { weights, total })
    }
    /// Caller can supply a saved/replayed ticket. Zero weight is never selected.
    pub fn choose(&self, ticket: u32) -> usize {
        let mut ticket = ticket % self.total;
        for (i, w) in self.weights.iter().enumerate() {
            if ticket < *w {
                return i;
            }
            ticket -= w;
        }
        unreachable!("validated nonzero total")
    }
    /// Fixed work; multiply-high maps the full 64-bit sample to the weight range.
    pub fn roll(&self, rng: &mut RewardRng) -> usize {
        self.choose(((u128::from(rng.next_u64()) * u128::from(self.total)) >> 64) as u32)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tickets_respect_weights_and_seeds_reproduce_sequences() {
        let t = RewardTable::new([1, 0, 3]).unwrap();
        assert_eq!(
            (0..4).map(|n| t.choose(n)).collect::<Vec<_>>(),
            [0, 2, 2, 2]
        );
        let mut a = RewardRng::new(0);
        let mut b = a;
        for _ in 0..100 {
            assert_eq!(t.roll(&mut a), t.roll(&mut b));
        }
        assert!(RewardTable::new([0, 0]).is_err());
        assert!(RewardTable::new([u32::MAX, 1]).is_err());
    }
}
