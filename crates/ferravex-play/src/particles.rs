//! Bounded CPU particle simulation with no allocations after pool construction.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleError {
    Full,
    InvalidParticle,
    InvalidDelta,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleView {
    pub position: [f32; 3],
    pub color: [f32; 4],
    pub size: f32,
    pub normalized_age: f32,
}
struct Particle {
    desc: ParticleDesc,
    age: f32,
}
pub struct ParticlePool {
    particles: Vec<Particle>,
    limit: usize,
    dropped: u64,
}
impl ParticlePool {
    pub fn new(capacity: usize) -> Self {
        Self {
            particles: Vec::with_capacity(capacity),
            limit: capacity,
            dropped: 0,
        }
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
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
    pub fn clear(&mut self) {
        self.particles.clear();
    }
    pub fn spawn(&mut self, desc: ParticleDesc) -> Result<(), ParticleError> {
        if !desc
            .position
            .into_iter()
            .chain(desc.velocity)
            .chain(desc.acceleration)
            .chain(desc.start_color)
            .chain(desc.end_color)
            .chain([desc.lifetime, desc.start_size, desc.end_size])
            .all(f32::is_finite)
            || desc.lifetime <= 0.0
            || desc.start_size < 0.0
            || desc.end_size < 0.0
        {
            return Err(ParticleError::InvalidParticle);
        }
        if self.particles.len() == self.limit {
            self.dropped = self.dropped.saturating_add(1);
            return Err(ParticleError::Full);
        }
        self.particles.push(Particle { desc, age: 0.0 });
        Ok(())
    }
    pub fn update(&mut self, dt: f32) -> Result<(), ParticleError> {
        if !dt.is_finite() || dt < 0.0 {
            return Err(ParticleError::InvalidDelta);
        }
        let mut i = 0;
        while i < self.particles.len() {
            let particle = &mut self.particles[i];
            particle.age += dt;
            if particle.age >= particle.desc.lifetime {
                self.particles.swap_remove(i);
                continue;
            }
            for axis in 0..3 {
                // Analytic constant-acceleration integration makes partitioned dt agree.
                particle.desc.position[axis] += particle.desc.velocity[axis] * dt
                    + particle.desc.acceleration[axis] * (0.5 * dt * dt);
                particle.desc.velocity[axis] += particle.desc.acceleration[axis] * dt;
            }
            if !particle
                .desc
                .position
                .into_iter()
                .chain(particle.desc.velocity)
                .all(f32::is_finite)
            {
                self.particles.swap_remove(i);
                continue;
            }
            i += 1;
        }
        Ok(())
    }
    pub fn iter(&self) -> impl Iterator<Item = ParticleView> + '_ {
        self.particles.iter().map(|p| {
            let age = (p.age / p.desc.lifetime).clamp(0.0, 1.0);
            ParticleView {
                position: p.desc.position,
                color: std::array::from_fn(|i| {
                    p.desc.start_color[i] * (1.0 - age) + p.desc.end_color[i] * age
                }),
                size: p.desc.start_size * (1.0 - age) + p.desc.end_size * age,
                normalized_age: age,
            }
        })
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
