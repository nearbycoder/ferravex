//! Timed chains reward continued accepted actions. Hosts own event deduplication.
use super::{LoopError, seconds};
#[derive(Clone, Copy, Debug)]
pub struct ScoreChain {
    window: f64,
    remaining: f64,
    chain: u32,
    per_tier: u32,
    maximum: u32,
    total: u64,
    best: u32,
}
impl ScoreChain {
    pub fn new(window: f32, per_tier: u32, maximum: u32) -> Result<Self, LoopError> {
        let window = seconds(window)?;
        if window == 0. || per_tier == 0 || maximum == 0 || maximum > 100 {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            window,
            remaining: 0.,
            chain: 0,
            per_tier,
            maximum,
            total: 0,
            best: 0,
        })
    }
    pub fn advance(&mut self, dt: f32) -> Result<(), LoopError> {
        self.remaining = (self.remaining - seconds(dt)?).max(0.);
        if self.remaining <= 1e-9 {
            self.break_chain();
        }
        Ok(())
    }
    /// Zero-point events cannot refresh a chain or farm multipliers.
    pub fn award(&mut self, base: u32) -> u64 {
        if base == 0 {
            return 0;
        }
        self.chain = self.chain.saturating_add(1);
        self.best = self.best.max(self.chain);
        self.remaining = self.window;
        let points = u64::from(base) * u64::from(self.multiplier());
        self.total = self.total.saturating_add(points);
        points
    }
    pub fn multiplier(&self) -> u32 {
        (1 + self.chain.saturating_sub(1) / self.per_tier).min(self.maximum)
    }
    pub fn chain(&self) -> u32 {
        self.chain
    }
    pub fn best(&self) -> u32 {
        self.best
    }
    pub fn total(&self) -> u64 {
        self.total
    }
    pub fn break_chain(&mut self) {
        self.chain = 0;
        self.remaining = 0.;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions_build_capped_multipliers_and_timeout_preserves_total() {
        let mut c = ScoreChain::new(2., 2, 3).unwrap();
        let awards: Vec<_> = (0..7).map(|_| c.award(100)).collect();
        assert_eq!(awards, [100, 100, 200, 200, 300, 300, 300]);
        assert_eq!(c.total(), 1500);
        c.advance(1.).unwrap();
        c.award(0);
        c.advance(1.).unwrap();
        assert_eq!((c.chain(), c.best(), c.multiplier()), (0, 7, 1));
        assert_eq!(c.award(100), 100);
        assert!(c.advance(-1.).is_err());
        assert_eq!(c.chain(), 1);
    }
}
