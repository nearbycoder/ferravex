//! Poise accumulation and finite stagger windows. Hosts decide allowed interrupts.
use super::{LoopError, seconds};
#[derive(Clone, Copy, Debug)]
pub struct Stagger {
    threshold: f32,
    decay: f32,
    duration: f64,
    poise: f32,
    remaining: f64,
}
impl Stagger {
    pub fn new(threshold: f32, decay: f32, duration: f32) -> Result<Self, LoopError> {
        let duration = seconds(duration)?;
        if !threshold.is_finite()
            || !(0.01..=1e6).contains(&threshold)
            || !decay.is_finite()
            || !(0. ..=1e6).contains(&decay)
            || duration == 0.
        {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            threshold,
            decay,
            duration,
            poise: 0.,
            remaining: 0.,
        })
    }
    pub fn active(&self) -> bool {
        self.remaining > 0.
    }
    pub fn buildup(&self) -> f32 {
        self.poise / self.threshold
    }
    /// A stagger cannot be extended by repeated hits during its active window.
    pub fn hit(&mut self, amount: f32) -> Result<bool, LoopError> {
        if !amount.is_finite() || !(0. ..=1e6).contains(&amount) {
            return Err(LoopError::Invalid);
        }
        if self.active() {
            return Ok(false);
        }
        self.poise = (self.poise + amount).min(self.threshold);
        if self.poise >= self.threshold {
            self.poise = 0.;
            self.remaining = self.duration;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    pub fn advance(&mut self, dt: f32) -> Result<(), LoopError> {
        let dt = seconds(dt)?;
        let recovery = (dt - self.remaining).max(0.);
        self.remaining = (self.remaining - dt).max(0.);
        self.poise = (f64::from(self.poise) - f64::from(self.decay) * recovery).max(0.) as f32;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn concentrated_hits_stagger_but_spam_cannot_extend_lockout() {
        let mut s = Stagger::new(10., 2., 0.5).unwrap();
        assert!(!s.hit(6.).unwrap());
        s.advance(1.).unwrap();
        assert!((s.buildup() - 0.4).abs() < 1e-6);
        assert!(s.hit(6.).unwrap());
        s.advance(0.25).unwrap();
        assert!(!s.hit(100.).unwrap());
        s.advance(0.25).unwrap();
        assert!(!s.active());
        assert!(s.hit(10.).unwrap());
        assert!(s.hit(f32::NAN).is_err());
    }
}
