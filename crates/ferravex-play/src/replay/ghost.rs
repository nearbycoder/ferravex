//! Presentation-only transforms. Ghosts never enter collision or game authority.
use super::ReplayError;
use ferravex_math::{Quat, Vec3};
pub const MAX_SAMPLES: usize = 120_000;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub tick: u64,
    pub position: Vec3,
    pub rotation: Quat,
    pub teleport: bool,
}
#[derive(Clone, Debug)]
pub struct GhostTrack {
    context: u64,
    tick_hz: u16,
    samples: Vec<Sample>,
    capacity: usize,
}
impl GhostTrack {
    pub fn new(context: u64, tick_hz: u16, capacity: usize) -> Result<Self, ReplayError> {
        if !(1..=1000).contains(&tick_hz) || capacity > MAX_SAMPLES {
            return Err(ReplayError::InvalidLimits);
        }
        Ok(Self {
            context,
            tick_hz,
            samples: Vec::with_capacity(capacity),
            capacity,
        })
    }
    pub fn context(&self) -> u64 {
        self.context
    }
    pub fn tick_hz(&self) -> u16 {
        self.tick_hz
    }
    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }
    pub fn push(&mut self, mut sample: Sample) -> Result<(), ReplayError> {
        if self.samples.len() == self.capacity {
            return Err(ReplayError::Capacity);
        }
        if self.samples.last().is_some_and(|s| s.tick >= sample.tick) {
            return Err(ReplayError::TickOrder);
        }
        if !sample.position.is_finite()
            || sample.position.abs().max_element() > 1e6
            || !sample.rotation.is_finite()
            || sample.rotation.length_squared() < 1e-8
        {
            return Err(ReplayError::InvalidData);
        }
        sample.rotation = sample.rotation.normalize();
        self.samples.push(sample);
        Ok(())
    }
    /// Integer ticks retain precision even after long sessions. Alpha is the
    /// fractional simulation tick, not a fraction between sparse samples.
    pub fn sample(&self, tick: u64, alpha: f32) -> Option<Sample> {
        if !alpha.is_finite() || !(0.0..1.0).contains(&alpha) {
            return None;
        }
        let first = self.samples.first()?;
        let last = self.samples.last()?;
        if tick < first.tick || tick > last.tick || (tick == last.tick && alpha > 0.) {
            return None;
        }
        let i = self.samples.partition_point(|s| s.tick <= tick) - 1;
        let a = self.samples[i];
        let Some(b) = self.samples.get(i + 1).copied() else {
            return Some(a);
        };
        if b.teleport {
            return Some(Sample { tick, ..a });
        }
        let t = ((tick - a.tick) as f64 + f64::from(alpha)) / (b.tick - a.tick) as f64;
        Some(Sample {
            tick,
            position: a.position.lerp(b.position, t as f32),
            rotation: a.rotation.slerp(b.rotation, t as f32),
            teleport: false,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_interpolation_and_discontinuities() {
        let mut g = GhostTrack::new(4, 64, 3).unwrap();
        for (tick, x, teleport) in [
            (u64::MAX - 20, 0., false),
            (u64::MAX - 10, 10., false),
            (u64::MAX, 50., true),
        ] {
            g.push(Sample {
                tick,
                position: Vec3::X * x,
                rotation: Quat::IDENTITY,
                teleport,
            })
            .unwrap();
        }
        assert_eq!(g.sample(u64::MAX - 15, 0.5).unwrap().position.x, 5.5);
        assert_eq!(g.sample(u64::MAX - 5, 0.).unwrap().position.x, 10.);
        assert!(g.sample(u64::MAX, 0.5).is_none());
        assert!(g.sample(u64::MAX - 21, 0.).is_none());
    }
}
