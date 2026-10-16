//! Host-driven simulation time with independent pause, scale, and stall clamping.
//! No wall-clock reads. Feed `scaled_delta` to a fixed clock; UI may use unscaled_delta.
use std::time::Duration;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimeStep {
    pub unscaled_delta: Duration,
    pub scaled_delta: Duration,
    pub total: Duration,
}
pub struct SimulationTime {
    scale: f64,
    paused: bool,
    max_delta: Duration,
    total: Duration,
}
impl Default for SimulationTime {
    fn default() -> Self {
        Self::new(Duration::from_millis(250)).unwrap()
    }
}
impl SimulationTime {
    /// Clamp each supplied frame to max_delta (nonzero, at most 60 seconds).
    pub fn new(max_delta: Duration) -> Option<Self> {
        if max_delta.is_zero() || max_delta > Duration::from_secs(60) {
            return None;
        }
        Some(Self {
            scale: 1.,
            paused: false,
            max_delta,
            total: Duration::ZERO,
        })
    }
    /// Supported scale range 0..=16. Invalid changes preserve the prior setting.
    pub fn set_scale(&mut self, scale: f64) -> bool {
        if !scale.is_finite() || !(0.0..=16.0).contains(&scale) {
            return false;
        }
        self.scale = scale;
        true
    }
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }
    pub fn advance(&mut self, elapsed: Duration) -> TimeStep {
        let unscaled_delta = elapsed.min(self.max_delta);
        let scaled_delta = if self.paused {
            Duration::ZERO
        } else {
            unscaled_delta.mul_f64(self.scale)
        };
        self.total = self.total.saturating_add(scaled_delta);
        TimeStep {
            unscaled_delta,
            scaled_delta,
            total: self.total,
        }
    }
    /// Manual paused simulation step. Does not apply time scale or advance a wall clock.
    pub fn step_paused(&mut self, delta: Duration) -> bool {
        if !self.paused || delta > self.max_delta {
            return false;
        }
        self.total = self.total.saturating_add(delta);
        true
    }
    pub fn total(&self) -> Duration {
        self.total
    }
    pub fn reset(&mut self) {
        self.total = Duration::ZERO;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scale_pause_stalls_and_manual_debug_step() {
        let mut t = SimulationTime::default();
        assert!(t.set_scale(2.));
        assert!(!t.set_scale(f64::NAN));
        assert_eq!(
            t.advance(Duration::from_secs(1)).scaled_delta,
            Duration::from_millis(500)
        );
        t.set_paused(true);
        let frame = t.advance(Duration::from_millis(20));
        assert_eq!(frame.unscaled_delta, Duration::from_millis(20));
        assert_eq!(frame.scaled_delta, Duration::ZERO);
        assert!(t.step_paused(Duration::from_millis(10)));
        assert_eq!(t.total(), Duration::from_millis(510));
        assert!(!t.step_paused(Duration::from_secs(1)));
        t.set_paused(false);
        assert!(!t.step_paused(Duration::ZERO));
        t.set_scale(0.);
        assert_eq!(t.advance(Duration::MAX).scaled_delta, Duration::ZERO);
        t.reset();
        assert_eq!(t.total(), Duration::ZERO);
        assert!(SimulationTime::new(Duration::ZERO).is_none());
    }
}
