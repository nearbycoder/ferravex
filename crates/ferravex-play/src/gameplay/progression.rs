//! Integer experience curves. At most 1023 promotions per award; no lost overflow XP.
use super::LoopError;
#[derive(Clone, Copy, Debug)]
pub struct Progression {
    base: u32,
    growth: u32,
    maximum: u16,
    level: u16,
    xp: u64,
}
impl Progression {
    pub fn new(base: u32, growth: u32, maximum: u16) -> Result<Self, LoopError> {
        if base == 0 || !(1..=1024).contains(&maximum) {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            base,
            growth,
            maximum,
            level: 1,
            xp: 0,
        })
    }
    pub fn level(&self) -> u16 {
        self.level
    }
    pub fn xp(&self) -> u64 {
        self.xp
    }
    pub fn next_cost(&self) -> Option<u64> {
        (self.level < self.maximum)
            .then(|| u64::from(self.base) + u64::from(self.growth) * u64::from(self.level - 1))
    }
    /// Returns earned promotions; hosts apply rewards once from this result.
    /// Excess at the level cap is retained for display, never re-awarded.
    pub fn award(&mut self, amount: u64) -> u16 {
        self.xp = self.xp.saturating_add(amount);
        let before = self.level;
        while let Some(cost) = self.next_cost() {
            if self.xp < cost {
                break;
            }
            self.xp -= cost;
            self.level += 1;
        }
        self.level - before
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn large_and_partitioned_awards_have_identical_levels_and_remainders() {
        let mut a = Progression::new(10, 5, 8).unwrap();
        let mut b = a;
        assert_eq!(a.award(85), 4);
        for _ in 0..85 {
            b.award(1);
        }
        assert_eq!((a.level(), a.xp()), (b.level(), b.xp()));
        assert_eq!((a.level(), a.xp(), a.next_cost()), (5, 15, Some(30)));
        a.award(u64::MAX);
        assert_eq!(a.level(), 8);
        assert_eq!(a.next_cost(), None);
        assert_eq!(a.award(u64::MAX), 0);
        assert!(Progression::new(0, 1, 2).is_err());
    }
}
