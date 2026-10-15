//! Bounded particles, seeded emitters, ribbons/trails and short-lived beams.
//! Simulation and geometry use first-party math; no GPU/device dependency.
mod beam;
mod emitter;
pub mod presets;
mod ribbon;
mod trail;
pub use beam::*;
pub use emitter::*;
pub use ribbon::*;
pub use trail::*;
pub const MAX_PARTICLES: usize = 65536;
pub const MAX_EFFECT_LIFETIME: f32 = 3600.;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleDesc {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub acceleration: [f32; 3],
    pub lifetime: f32,
    pub start_color: [f32; 4],
    pub end_color: [f32; 4],
    pub start_size: f32,
    pub end_size: f32,
}
impl Default for ParticleDesc {
    fn default() -> Self {
        Self {
            position: [0.0; 3],
            velocity: [0.0; 3],
            acceleration: [0.0; 3],
            lifetime: 1.0,
            start_color: [1.0; 4],
            end_color: [1.0, 1.0, 1.0, 0.0],
            start_size: 1.0,
            end_size: 0.0,
        }
    }
}

impl ParticleDesc {
    pub fn validate(&self) -> Result<(), ParticleError> {
        if !self
            .position
            .into_iter()
            .chain(self.velocity)
            .chain(self.acceleration)
            .chain(self.start_color)
            .chain(self.end_color)
            .chain([self.lifetime, self.start_size, self.end_size])
            .all(f32::is_finite)
            || !(0.0..=MAX_EFFECT_LIFETIME).contains(&self.lifetime)
            || self.lifetime == 0.
            || self.start_size < 0.
            || self.end_size < 0.
        {
            Err(ParticleError::InvalidParticle)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParticleMotion {
    /// Linear drag coefficient in inverse seconds; exact constant-force solution.
    pub drag: f32,
    pub rotation: f32,
    pub angular_velocity: f32,
}
impl ParticleMotion {
    pub fn validate(&self) -> Result<(), ParticleError> {
        if !self.drag.is_finite()
            || !(0.0..=128.).contains(&self.drag)
            || !self.rotation.is_finite()
            || !self.angular_velocity.is_finite()
            || self.angular_velocity.abs() > 1024.
        {
            Err(ParticleError::InvalidParticle)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleError {
    Full,
    InvalidParticle,
    InvalidDelta,
    InvalidCapacity,
    Expired,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleView {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub color: [f32; 4],
    pub size: f32,
    pub rotation: f32,
    pub normalized_age: f32,
}
struct Particle {
    desc: ParticleDesc,
    motion: ParticleMotion,
    age: f64,
    view: ParticleView,
}
impl Particle {
    fn sample(desc: ParticleDesc, motion: ParticleMotion, age: f64) -> Option<ParticleView> {
        let k = f64::from(motion.drag);
        let x = k * age;
        let (s, u) = if x < 0.0001 {
            (
                age * (1. - x * 0.5 + x * x / 6. - x * x * x / 24.),
                age * age * (0.5 - x / 6. + x * x / 24. - x * x * x / 120.),
            )
        } else {
            let s = -(-x).exp_m1() / k;
            (s, (age - s) / k)
        };
        let decay = if k == 0. { 1. } else { (-x).exp() };
        let position = std::array::from_fn(|i| {
            (f64::from(desc.position[i])
                + f64::from(desc.velocity[i]) * s
                + f64::from(desc.acceleration[i]) * u) as f32
        });
        let velocity = std::array::from_fn(|i| {
            (f64::from(desc.velocity[i]) * decay + f64::from(desc.acceleration[i]) * s) as f32
        });
        if !position.into_iter().chain(velocity).all(f32::is_finite) {
            return None;
        }
        let t = (age / f64::from(desc.lifetime)).clamp(0., 1.);
        Some(ParticleView {
            position,
            velocity,
            color: std::array::from_fn(|i| {
                (f64::from(desc.start_color[i]) * (1. - t) + f64::from(desc.end_color[i]) * t)
                    as f32
            }),
            size: (f64::from(desc.start_size) * (1. - t) + f64::from(desc.end_size) * t) as f32,
            rotation: ((f64::from(motion.rotation) + f64::from(motion.angular_velocity) * age)
                % f64::from(std::f32::consts::TAU)) as f32,
            normalized_age: t as f32,
        })
    }
}
pub struct ParticlePool {
    particles: Vec<Particle>,
    limit: usize,
    dropped: u64,
    discarded: u64,
}
impl ParticlePool {
    /// Trusted convenience constructor. Use try_new for untrusted capacities.
    pub fn new(capacity: usize) -> Self {
        Self::try_new(capacity).expect("particle capacity <= 65536")
    }
    pub fn try_new(capacity: usize) -> Result<Self, ParticleError> {
        if capacity > MAX_PARTICLES {
            return Err(ParticleError::InvalidCapacity);
        }
        Ok(Self {
            particles: Vec::with_capacity(capacity),
            limit: capacity,
            dropped: 0,
            discarded: 0,
        })
    }
    pub fn len(&self) -> usize {
        self.particles.len()
    }
    pub fn is_empty(&self) -> bool {
        self.particles.is_empty()
    }
    pub fn capacity(&self) -> usize {
        self.limit
    }
    pub fn available(&self) -> usize {
        self.limit - self.particles.len()
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    pub fn discarded(&self) -> u64 {
        self.discarded
    }
    pub fn clear(&mut self) {
        self.particles.clear();
    }
    pub fn reset_stats(&mut self) {
        self.dropped = 0;
        self.discarded = 0;
    }
    pub fn spawn(&mut self, desc: ParticleDesc) -> Result<(), ParticleError> {
        self.spawn_with(desc, ParticleMotion::default())
    }
    pub fn spawn_with(
        &mut self,
        desc: ParticleDesc,
        motion: ParticleMotion,
    ) -> Result<(), ParticleError> {
        self.spawn_aged(desc, motion, 0.)
    }
    /// Birth-time catch-up used by emitters after the existing pool was updated.
    /// Invalid or already expired samples do not consume a slot.
    pub fn spawn_aged(
        &mut self,
        desc: ParticleDesc,
        motion: ParticleMotion,
        age: f32,
    ) -> Result<(), ParticleError> {
        desc.validate()?;
        motion.validate()?;
        if !age.is_finite() || age < 0. {
            return Err(ParticleError::InvalidDelta);
        }
        if age >= desc.lifetime {
            return Err(ParticleError::Expired);
        }
        let view =
            Particle::sample(desc, motion, f64::from(age)).ok_or(ParticleError::InvalidParticle)?;
        if self.available() == 0 {
            self.dropped = self.dropped.saturating_add(1);
            return Err(ParticleError::Full);
        }
        self.particles.push(Particle {
            desc,
            motion,
            age: f64::from(age),
            view,
        });
        Ok(())
    }
    pub fn update(&mut self, dt: f32) -> Result<(), ParticleError> {
        if !dt.is_finite() || dt < 0. {
            return Err(ParticleError::InvalidDelta);
        }
        let mut i = 0;
        while i < self.particles.len() {
            let p = &mut self.particles[i];
            p.age += f64::from(dt);
            if p.age >= f64::from(p.desc.lifetime) {
                self.particles.swap_remove(i);
                continue;
            }
            if let Some(view) = Particle::sample(p.desc, p.motion, p.age) {
                p.view = view;
                i += 1;
            } else {
                self.discarded = self.discarded.saturating_add(1);
                self.particles.swap_remove(i);
            }
        }
        Ok(())
    }
    pub fn iter(&self) -> impl ExactSizeIterator<Item = ParticleView> + '_ {
        self.particles.iter().map(|p| p.view)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capacity_is_bounded_and_expired_slots_are_reusable() {
        let mut pool = ParticlePool::new(2);
        pool.spawn(ParticleDesc::default()).unwrap();
        pool.spawn(ParticleDesc::default()).unwrap();
        assert_eq!(
            pool.spawn(ParticleDesc::default()),
            Err(ParticleError::Full)
        );
        assert_eq!(pool.dropped(), 1);
        pool.update(1.0).unwrap();
        assert!(pool.is_empty());
        pool.spawn(ParticleDesc::default()).unwrap();
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.capacity(), 2);
    }
    #[test]
    fn constant_acceleration_matches_frame_partition_and_color_fades() {
        let desc = ParticleDesc {
            velocity: [2.0, 0.0, 0.0],
            acceleration: [0.0, -4.0, 0.0],
            lifetime: 2.0,
            ..ParticleDesc::default()
        };
        let mut a = ParticlePool::new(1);
        let mut b = ParticlePool::new(1);
        a.spawn(desc).unwrap();
        b.spawn(desc).unwrap();
        a.update(1.0).unwrap();
        for _ in 0..10 {
            b.update(0.1).unwrap();
        }
        let av = a.iter().next().unwrap();
        let bv = b.iter().next().unwrap();
        assert_eq!(av.position, [2.0, -2.0, 0.0]);
        for i in 0..3 {
            assert!((av.position[i] - bv.position[i]).abs() < 1e-5);
        }
        assert_eq!(av.color[3], 0.5);
    }
    #[test]
    fn invalid_values_do_not_poison_pool() {
        let mut pool = ParticlePool::new(1);
        assert_eq!(
            pool.spawn(ParticleDesc {
                lifetime: f32::NAN,
                ..ParticleDesc::default()
            }),
            Err(ParticleError::InvalidParticle)
        );
        pool.spawn(ParticleDesc::default()).unwrap();
        assert_eq!(pool.update(-1.0), Err(ParticleError::InvalidDelta));
        assert_eq!(pool.iter().next().unwrap().normalized_age, 0.0);
    }
}
