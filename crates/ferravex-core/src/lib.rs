//! Small, platform-independent building blocks. Simulation time never comes from a wall clock.
//!
//! ```
//! use ferravex_core::{FixedClock, World};
//! let mut world = World::new();
//! world.spawn((42_u32,));
//! let mut clock = FixedClock::new(64.0, 8);
//! let frame = clock.advance(1.0 / 60.0);
//! for _ in 0..frame.steps { /* update systems with frame.dt */ }
//! ```
pub use ferravex_ecs as ecs;
pub use ferravex_ecs::{Entity, World};
pub mod collision;
pub mod telemetry;

/// A fixed simulation step budget with a fractional render interpolation factor.
#[derive(Debug, Clone, Copy)]
pub struct FrameStep {
    pub steps: u32,
    pub dt: f32,
    pub alpha: f32,
    /// Whole steps intentionally discarded to prevent a spiral of death.
    pub dropped_seconds: f64,
}

/// Frame-rate independent simulation with bounded catch-up work.
///
/// Supply elapsed seconds from your platform. Non-finite/negative durations are ignored.
/// Over-budget whole steps are discarded; fractional remainder is retained for interpolation.
#[derive(Debug, Clone)]
pub struct FixedClock {
    step: f64,
    accumulator: f64,
    max_steps: u32,
    tick: u64,
}

impl FixedClock {
    pub fn new(hz: f64, max_steps: u32) -> Self {
        assert!(
            hz.is_finite() && (1.0..=1000.0).contains(&hz),
            "tick rate must be in 1..=1000 Hz"
        );
        assert!(max_steps > 0, "catch-up budget must be nonzero");
        Self {
            step: 1.0 / hz,
            accumulator: 0.0,
            max_steps,
            tick: 0,
        }
    }

    pub fn advance(&mut self, frame_seconds: f64) -> FrameStep {
        // Clamp absurd durations before addition to keep all arithmetic finite.
        let elapsed = if frame_seconds.is_finite() {
            frame_seconds.clamp(0.0, 3600.0)
        } else {
            0.0
        };
        self.accumulator += elapsed;
        let available = ((self.accumulator + self.step * 1e-9) / self.step).floor() as u64;
        let steps = available.min(self.max_steps as u64) as u32;
        self.accumulator = (self.accumulator - available as f64 * self.step).max(0.0);
        self.tick = self.tick.wrapping_add(steps as u64);
        FrameStep {
            steps,
            dt: self.step as f32,
            alpha: (self.accumulator / self.step).clamp(0.0, 1.0) as f32,
            dropped_seconds: (available - steps as u64) as f64 * self.step,
        }
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn dt(&self) -> f32 {
        self.step as f32
    }
    pub fn reset(&mut self) {
        self.accumulator = 0.0;
        self.tick = 0;
    }
}

/// Allocation-free rolling frame-time statistics. Zero samples are valid.
pub struct FrameStats<const N: usize = 120> {
    samples: [f32; N],
    cursor: usize,
    len: usize,
}
impl<const N: usize> Default for FrameStats<N> {
    fn default() -> Self {
        assert!(N > 0);
        Self {
            samples: [0.0; N],
            cursor: 0,
            len: 0,
        }
    }
}
impl<const N: usize> FrameStats<N> {
    pub fn record(&mut self, seconds: f32) {
        if !seconds.is_finite() || seconds < 0.0 {
            return;
        }
        self.samples[self.cursor] = seconds * 1000.0;
        self.cursor = (self.cursor + 1) % N;
        self.len = (self.len + 1).min(N);
    }
    pub fn mean_ms(&self) -> f32 {
        if self.len == 0 {
            return 0.0;
        }
        self.samples[..self.len].iter().sum::<f32>() / self.len as f32
    }
    pub fn max_ms(&self) -> f32 {
        self.samples[..self.len].iter().copied().fold(0.0, f32::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_rate_independent() {
        for fps in [30, 60, 144, 240] {
            let mut clock = FixedClock::new(64.0, 8);
            for _ in 0..fps * 10 {
                clock.advance(1.0 / fps as f64);
            }
            assert_eq!(clock.tick(), 640);
        }
    }
    #[test]
    fn catchup_is_bounded_and_fraction_retained() {
        let mut clock = FixedClock::new(64.0, 4);
        let f = clock.advance(10.5 / 64.0);
        assert_eq!(f.steps, 4);
        assert!((f.alpha - 0.5).abs() < 0.0001);
        assert!((f.dropped_seconds - 6.0 / 64.0).abs() < 0.0001);
        assert_eq!(clock.advance(0.5 / 64.0).steps, 1);
    }
    #[test]
    fn invalid_durations_do_not_poison_clock() {
        let mut c = FixedClock::new(64.0, 8);
        for dt in [f64::NAN, f64::INFINITY, -5.0] {
            assert_eq!(c.advance(dt).steps, 0);
        }
        assert_eq!(c.advance(1.0 / 64.0).steps, 1);
    }
    #[test]
    fn stats_wrap_and_ignore_nan() {
        let mut s = FrameStats::<2>::default();
        for dt in [0.001, 0.003, 0.005, f32::NAN] {
            s.record(dt);
        }
        assert!((s.mean_ms() - 4.0).abs() < 0.001);
        assert_eq!(s.max_ms(), 5.0);
    }
}
