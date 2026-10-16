//! Reusable platform mechanisms; hosts provide collision-confirmed contact edges.
use super::{LoopError, seconds};
#[derive(Clone, Copy, Debug)]
pub struct LaunchPad {
    speed: f32,
    delay: f64,
    remaining: f64,
    contact: bool,
}
impl LaunchPad {
    pub fn new(speed: f32, delay: f32) -> Result<Self, LoopError> {
        let delay = seconds(delay)?;
        if !speed.is_finite() || !(0.01..=1e5).contains(&speed) {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            speed,
            delay,
            remaining: 0.,
            contact: false,
        })
    }
    /// Returns an authored upward speed on a fresh accepted landing. Continuous
    /// contact cannot retrigger, even after cooldown. Hosts apply the velocity.
    pub fn step(&mut self, dt: f32, contact: bool) -> Result<Option<f32>, LoopError> {
        self.remaining = (self.remaining - seconds(dt)?).max(0.);
        let fire = contact && !self.contact && self.remaining == 0.;
        self.contact = contact;
        if fire {
            self.remaining = self.delay;
            Ok(Some(self.speed))
        } else {
            Ok(None)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn launch_requires_a_new_landing_and_respects_cooldown() {
        let mut p = LaunchPad::new(8., 0.5).unwrap();
        assert_eq!(p.step(0., true).unwrap(), Some(8.));
        assert_eq!(p.step(1., true).unwrap(), None);
        p.step(0., false).unwrap();
        assert_eq!(p.step(0., true).unwrap(), Some(8.));
        p.step(0., false).unwrap();
        assert_eq!(p.step(0.1, true).unwrap(), None);
        assert!(p.step(f32::NAN, false).is_err());
    }
}
