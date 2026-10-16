//! Bound unlucky reward droughts without reroll loops or hidden probability changes.
use super::LoopError;
#[derive(Clone, Copy, Debug)]
pub struct PityCounter {
    limit: u16,
    misses: u16,
}
impl PityCounter {
    /// At least one reward in each `limit` accepted attempts, including the first.
    pub fn new(limit: u16) -> Result<Self, LoopError> {
        if limit == 0 {
            return Err(LoopError::Invalid);
        }
        Ok(Self { limit, misses: 0 })
    }
    /// Call only after the host confirms an eligible event. Successful natural
    /// rolls reset the same counter. The returned reward must be applied once.
    pub fn attempt(&mut self, natural_success: bool) -> bool {
        if natural_success || self.misses >= self.limit - 1 {
            self.misses = 0;
            true
        } else {
            self.misses += 1;
            false
        }
    }
    pub fn misses(&self) -> u16 {
        self.misses
    }
    pub fn reset(&mut self) {
        self.misses = 0;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drought_is_bounded_and_natural_rewards_reset_it() {
        let mut p = PityCounter::new(3).unwrap();
        assert!(!p.attempt(false));
        assert!(!p.attempt(false));
        assert!(p.attempt(false));
        assert_eq!(p.misses(), 0);
        assert!(!p.attempt(false));
        assert!(p.attempt(true));
        assert!(!p.attempt(false));
        assert!(PityCounter::new(1).unwrap().attempt(false));
        assert!(PityCounter::new(0).is_err());
    }
}
