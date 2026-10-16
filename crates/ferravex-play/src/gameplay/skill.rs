//! Precision-return rewards gated on confirmed gameplay progress between returns.
//! Repeated empty bounces cannot farm points. Host collision supplies -1..1 offset.
use super::LoopError;
#[derive(Clone, Copy, Debug)]
pub struct PrecisionReturns {
    tolerance: f32,
    base: u32,
    cap: u32,
    armed: bool,
    streak: u32,
    best: u32,
}
impl PrecisionReturns {
    pub fn new(tolerance: f32, base: u32, cap: u32) -> Result<Self, LoopError> {
        if !tolerance.is_finite()
            || !(0. ..=1.).contains(&tolerance)
            || base == 0
            || base > 1_000_000
            || !(1..=100).contains(&cap)
        {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            tolerance,
            base,
            cap,
            armed: false,
            streak: 0,
            best: 0,
        })
    }
    pub fn progress(&mut self) {
        self.armed = true;
    }
    pub fn streak(&self) -> u32 {
        self.streak
    }
    pub fn best(&self) -> u32 {
        self.best
    }
    pub fn reset(&mut self) {
        self.armed = false;
        self.streak = 0;
    }
    /// Consumes the eligibility gate on any accepted return, including a miss.
    /// Invalid offsets preserve the gate so malformed host input loses no reward.
    pub fn returned(&mut self, offset: f32) -> Result<u32, LoopError> {
        if !offset.is_finite() || !(-1. ..=1.).contains(&offset) {
            return Err(LoopError::Invalid);
        }
        let accepted = self.armed && offset.abs() <= self.tolerance;
        self.armed = false;
        if accepted {
            self.streak = self.streak.saturating_add(1);
            self.best = self.best.max(self.streak);
            Ok(self.base * self.streak.min(self.cap))
        } else {
            self.streak = 0;
            Ok(0)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_offsets_preserve_progress_and_reset_clears_only_current_streak() {
        let mut p = PrecisionReturns::new(0.2, 50, 3).unwrap();
        p.progress();
        for offset in [f32::NAN, f32::INFINITY, -1.01, 1.01] {
            assert!(p.returned(offset).is_err());
        }
        assert_eq!(p.returned(-0.2).unwrap(), 50);
        p.progress();
        p.reset();
        assert_eq!(p.returned(0.).unwrap(), 0);
        assert_eq!(p.streak(), 0);
        assert_eq!(p.best(), 1);
        p.progress();
        assert_eq!(p.returned(0.2).unwrap(), 50);
    }

    #[test]
    fn empty_returns_cannot_farm_and_progressive_precision_caps_the_reward() {
        let mut p = PrecisionReturns::new(0.2, 50, 3).unwrap();
        assert_eq!(p.returned(0.).unwrap(), 0);
        for expected in [50, 100, 150, 150] {
            p.progress();
            assert_eq!(p.returned(0.1).unwrap(), expected);
        }
        assert_eq!(p.best(), 4);
        p.progress();
        assert!(p.returned(f32::NAN).is_err());
        assert_eq!(p.returned(0.8).unwrap(), 0);
        assert_eq!(p.streak(), 0);
        p.progress();
        assert_eq!(p.returned(0.).unwrap(), 50);
        assert_eq!(p.returned(0.).unwrap(), 0);
    }
}
