//! Hold interactions bound to a specific actor/target pair with explicit interruption.
use super::{LoopError, seconds};
#[derive(Clone, Copy, Debug)]
pub struct Interaction {
    duration: f64,
    elapsed: f64,
    pair: Option<(u64, u64)>,
    completed: bool,
}
impl Interaction {
    pub fn new(duration: f32) -> Result<Self, LoopError> {
        let duration = seconds(duration)?;
        if duration == 0. {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            duration,
            elapsed: 0.,
            pair: None,
            completed: false,
        })
    }
    pub fn progress(&self) -> f32 {
        (self.elapsed / self.duration) as f32
    }
    pub fn cancel(&mut self) {
        self.elapsed = 0.;
        self.pair = None;
        self.completed = false;
    }
    /// None interrupts. Supply Some only while the button, range, permissions and
    /// line of sight are valid. Target switches restart; completion latches until
    /// interruption/switch, so holding cannot repeatedly grant a reward.
    pub fn step(&mut self, dt: f32, pair: Option<(u64, u64)>) -> Result<bool, LoopError> {
        let dt = seconds(dt)?;
        if pair != self.pair {
            self.cancel();
            self.pair = pair;
        }
        if pair.is_none() || self.completed {
            return Ok(false);
        }
        self.elapsed = (self.elapsed + dt).min(self.duration);
        if self.elapsed + 1e-7 >= self.duration {
            self.elapsed = self.duration;
            self.completed = true;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interruptions_and_target_switches_require_a_full_new_hold() {
        let mut i = Interaction::new(1.).unwrap();
        assert!(!i.step(0.5, Some((1, 2))).unwrap());
        assert!(!i.step(0.5, Some((1, 3))).unwrap());
        assert!(!i.step(0., None).unwrap());
        assert_eq!(i.progress(), 0.);
        assert!(i.step(1., Some((1, 3))).unwrap());
        assert!(!i.step(2., Some((1, 3))).unwrap());
        assert!(i.step(f32::NAN, None).is_err());
        assert_eq!(i.progress(), 1.);
    }
}
