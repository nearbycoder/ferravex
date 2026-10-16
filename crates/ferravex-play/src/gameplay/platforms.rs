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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformPhase {
    Solid,
    Warning,
    Absent,
}
/// A timed platform that disappears after contact, then regrows. Hosts exclude
/// absent surfaces from collision and use phase/progress to telegraph the change.
#[derive(Clone, Copy, Debug)]
pub struct CrumblePlatform {
    warning: f64,
    regrow: f64,
    remaining: f64,
    phase: PlatformPhase,
}
impl CrumblePlatform {
    pub fn new(warning: f32, regrow: f32) -> Result<Self, LoopError> {
        let warning = seconds(warning)?;
        let regrow = seconds(regrow)?;
        if warning == 0. || regrow == 0. {
            return Err(LoopError::Invalid);
        }
        Ok(Self {
            warning,
            regrow,
            remaining: 0.,
            phase: PlatformPhase::Solid,
        })
    }
    pub fn phase(&self) -> PlatformPhase {
        self.phase
    }
    pub fn solid(&self) -> bool {
        self.phase != PlatformPhase::Absent
    }
    pub fn remaining(&self) -> f32 {
        self.remaining as f32
    }
    /// At most two transitions per step; regrowth never rearms on stale contact
    /// in the same update. Invalid time leaves the platform unchanged.
    pub fn step(&mut self, dt: f32, contact: bool) -> Result<(), LoopError> {
        let mut dt = seconds(dt)?;
        if self.phase == PlatformPhase::Solid && contact {
            self.phase = PlatformPhase::Warning;
            self.remaining = self.warning;
        }
        if self.phase == PlatformPhase::Warning {
            let used = dt.min(self.remaining);
            self.remaining -= used;
            dt -= used;
            if self.remaining <= 1e-7 {
                self.phase = PlatformPhase::Absent;
                self.remaining = self.regrow;
            }
        }
        if self.phase == PlatformPhase::Absent {
            self.remaining = (self.remaining - dt).max(0.);
            if self.remaining <= 1e-7 {
                self.phase = PlatformPhase::Solid;
                self.remaining = 0.;
            }
        }
        Ok(())
    }
    pub fn reset(&mut self) {
        self.phase = PlatformPhase::Solid;
        self.remaining = 0.;
    }
}
#[cfg(test)]
mod crumble_tests {
    use super::*;
    #[test]
    fn contact_telegraphs_collapse_and_absence_regrows_without_frame_dependence() {
        let mut a = CrumblePlatform::new(0.5, 2.).unwrap();
        let mut b = a;
        a.step(1., true).unwrap();
        b.step(0., true).unwrap();
        for _ in 0..4 {
            b.step(0.25, false).unwrap();
        }
        assert_eq!(a.phase(), PlatformPhase::Absent);
        assert_eq!(a.phase(), b.phase());
        assert_eq!(a.remaining(), b.remaining());
        a.step(1.5, false).unwrap();
        assert!(a.solid());
        assert_eq!(a.phase(), PlatformPhase::Solid);
        a.step(0.1, true).unwrap();
        assert!(a.step(f32::NAN, false).is_err());
        assert_eq!(a.phase(), PlatformPhase::Warning);
        a.reset();
        assert_eq!(a.phase(), PlatformPhase::Solid);
    }
}
