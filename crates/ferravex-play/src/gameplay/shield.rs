//! Shield absorption and delayed regeneration independent of health/damage types.
use super::{LoopError, seconds};
#[derive(Clone, Copy, Debug)]
pub struct Shield {
    maximum: f32,
    value: f32,
    rate: f32,
    delay: f64,
    remaining: f64,
}
impl Shield {
    pub fn new(maximum: f32, rate: f32, delay: f32) -> Result<Self, LoopError> {
        let delay = seconds(delay)?;
        if !maximum.is_finite()
            || !(0. ..=1e6).contains(&maximum)
            || !rate.is_finite()
            || !(0. ..=1e6).contains(&rate)
        {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            maximum,
            value: maximum,
            rate,
            delay,
            remaining: 0.,
        })
    }
    pub fn value(&self) -> f32 {
        self.value
    }
    /// Returns unabsorbed damage for the host's health rules. Zero damage does
    /// not reset recovery; nonzero confirmed hits reset even an empty shield.
    pub fn absorb(&mut self, damage: f32) -> Result<f32, LoopError> {
        if !damage.is_finite() || !(0. ..=1e6).contains(&damage) {
            return Err(LoopError::Invalid);
        }
        if damage == 0. {
            return Ok(0.);
        }
        self.remaining = self.delay;
        let absorbed = damage.min(self.value);
        self.value -= absorbed;
        Ok(damage - absorbed)
    }
    pub fn advance(&mut self, dt: f32) -> Result<(), LoopError> {
        let dt = seconds(dt)?;
        let recovery = (dt - self.remaining).max(0.);
        self.remaining = (self.remaining - dt).max(0.);
        self.value = (f64::from(self.value) + f64::from(self.rate) * recovery)
            .min(f64::from(self.maximum)) as f32;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn damage_conserves_and_recovery_only_uses_time_after_delay() {
        let mut s = Shield::new(20., 10., 2.).unwrap();
        assert_eq!(s.absorb(30.).unwrap(), 10.);
        let mut partitioned = s;
        s.advance(3.).unwrap();
        for _ in 0..6 {
            partitioned.advance(0.5).unwrap();
        }
        assert_eq!(s.value(), 10.);
        assert_eq!(s.value(), partitioned.value());
        s.absorb(0.).unwrap();
        s.advance(1.).unwrap();
        assert_eq!(s.value(), 20.);
        assert!(s.absorb(f32::NAN).is_err());
        assert_eq!(s.value(), 20.);
        s.absorb(20.).unwrap();
        s.advance(1.).unwrap();
        s.absorb(1.).unwrap();
        s.advance(1.).unwrap();
        assert_eq!(s.value(), 0.);
    }
}
