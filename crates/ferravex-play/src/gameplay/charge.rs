//! Hold/release actions with minimum commitment, capped power and recovery time.
use super::{LoopError, seconds};
#[derive(Clone, Copy, Debug)]
pub struct ChargeAction {
    minimum: f64,
    maximum: f64,
    recovery: f64,
    held: f64,
    cooldown: f64,
    charging: bool,
}
impl ChargeAction {
    pub fn new(minimum: f32, maximum: f32, recovery: f32) -> Result<Self, LoopError> {
        let minimum = seconds(minimum)?;
        let maximum = seconds(maximum)?;
        let recovery = seconds(recovery)?;
        if maximum == 0. || minimum > maximum {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            minimum,
            maximum,
            recovery,
            held: 0.,
            cooldown: 0.,
            charging: false,
        })
    }
    pub fn power(&self) -> f32 {
        (self.held / self.maximum) as f32
    }
    pub fn charging(&self) -> bool {
        self.charging
    }
    pub fn cancel(&mut self) {
        self.held = 0.;
        self.charging = false;
    }
    /// A release emits at most one normalized power value. Continued holds cap
    /// at full charge. Hosts commit resource costs only for returned releases.
    pub fn step(&mut self, dt: f32, held: bool) -> Result<Option<f32>, LoopError> {
        let dt = seconds(dt)?;
        let active_dt = (dt - self.cooldown).max(0.);
        self.cooldown = (self.cooldown - dt).max(0.);
        if held && self.cooldown == 0. {
            self.charging = true;
            self.held = (self.held + active_dt).min(self.maximum);
            return Ok(None);
        }
        if !held && self.charging {
            let output = (self.held + 1e-7 >= self.minimum).then(|| self.power());
            self.cancel();
            if output.is_some() {
                self.cooldown = self.recovery;
            }
            return Ok(output);
        }
        Ok(None)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn short_taps_cancel_full_charge_caps_and_release_is_single_use() {
        let mut c = ChargeAction::new(0.2, 1., 0.5).unwrap();
        c.step(0.1, true).unwrap();
        assert_eq!(c.step(0., false).unwrap(), None);
        c.step(2., true).unwrap();
        assert_eq!(c.power(), 1.);
        assert_eq!(c.step(0., false).unwrap(), Some(1.));
        assert_eq!(c.step(0., false).unwrap(), None);
        c.step(0.25, true).unwrap();
        assert!(!c.charging());
        c.step(0.5, true).unwrap();
        assert!((c.power() - 0.25).abs() < 1e-6);
        assert!(c.step(f32::NAN, false).is_err());
        assert!(c.charging());
        c.cancel();
        assert_eq!(c.power(), 0.);
    }
}
