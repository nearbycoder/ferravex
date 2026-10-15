use super::{ParticleDesc, ParticleError, ParticleMotion, ParticlePool};
use ferravex_math::{Quat, Vec3};
pub const MAX_EMISSION_PER_UPDATE: u16 = 256;
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum SpawnShape {
    #[default]
    Point,
    Box {
        half: [f32; 3],
    },
    Sphere {
        radius: f32,
    },
    /// Local XY disc; rotate the emitter to choose another plane.
    Disc {
        radius: f32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DirectionCone {
    pub axis: [f32; 3],
    pub angle: f32,
    pub speed: [f32; 2],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmitterDesc {
    pub particle: ParticleDesc,
    pub motion: ParticleMotion,
    pub shape: SpawnShape,
    pub velocity_cone: Option<DirectionCone>,
    pub velocity_jitter: [f32; 3],
    /// Fractional symmetric jitter. Lifetime jitter is limited to 0..=0.95.
    pub lifetime_jitter: f32,
    pub size_jitter: f32,
    pub rotation_jitter: f32,
    pub inherit_velocity: f32,
    pub rate: f32,
    pub max_per_update: u16,
}
impl Default for EmitterDesc {
    fn default() -> Self {
        Self {
            particle: ParticleDesc::default(),
            motion: ParticleMotion::default(),
            shape: SpawnShape::Point,
            velocity_cone: None,
            velocity_jitter: [0.; 3],
            lifetime_jitter: 0.,
            size_jitter: 0.,
            rotation_jitter: 0.,
            inherit_velocity: 0.,
            rate: 10.,
            max_per_update: 64,
        }
    }
}
impl EmitterDesc {
    pub fn validate(&self) -> Result<(), ParticleError> {
        self.particle.validate()?;
        self.motion.validate()?;
        let mut valid = self.rate.is_finite()
            && (0.0..=65536.).contains(&self.rate)
            && self.max_per_update > 0
            && self.max_per_update <= MAX_EMISSION_PER_UPDATE;
        valid &= self
            .velocity_jitter
            .iter()
            .all(|v| v.is_finite() && *v >= 0.);
        valid &= self.lifetime_jitter.is_finite()
            && (0.0..=0.95).contains(&self.lifetime_jitter)
            && self.particle.lifetime * (1. + self.lifetime_jitter) <= super::MAX_EFFECT_LIFETIME;
        valid &= self.size_jitter.is_finite()
            && (0.0..=1.).contains(&self.size_jitter)
            && (self.particle.start_size * (1. + self.size_jitter)).is_finite()
            && (self.particle.end_size * (1. + self.size_jitter)).is_finite();
        valid &= self.rotation_jitter.is_finite()
            && (0.0..=std::f32::consts::TAU).contains(&self.rotation_jitter)
            && self.inherit_velocity.is_finite()
            && (0.0..=1.).contains(&self.inherit_velocity);
        valid &= match self.shape {
            SpawnShape::Point => true,
            SpawnShape::Box { half } => half.into_iter().all(|v| v.is_finite() && v >= 0.),
            SpawnShape::Sphere { radius } | SpawnShape::Disc { radius } => {
                radius.is_finite() && radius >= 0.
            }
        };
        if let Some(cone) = self.velocity_cone {
            valid &= Vec3::from_array(cone.axis).try_normalize().is_some()
                && cone.angle.is_finite()
                && (0.0..=std::f32::consts::PI).contains(&cone.angle)
                && cone
                    .speed
                    .into_iter()
                    .all(|v| v.is_finite() && (0.0..=100000.).contains(&v))
                && cone.speed[0] <= cone.speed[1];
        }
        if valid {
            Ok(())
        } else {
            Err(ParticleError::InvalidParticle)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EmitterTransform {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
}
impl Default for EmitterTransform {
    fn default() -> Self {
        Self {
            position: [0.; 3],
            rotation: [0., 0., 0., 1.],
        }
    }
}
impl EmitterTransform {
    pub fn at(position: [f32; 3]) -> Self {
        Self {
            position,
            ..Default::default()
        }
    }
    fn normalized(self) -> Result<Self, ParticleError> {
        let norm = self
            .rotation
            .iter()
            .map(|v| f64::from(*v).powi(2))
            .sum::<f64>()
            .sqrt();
        if !self
            .position
            .into_iter()
            .chain(self.rotation)
            .all(f32::is_finite)
            || norm < 1e-10
        {
            return Err(ParticleError::InvalidParticle);
        }
        Ok(Self {
            position: self.position,
            rotation: self.rotation.map(|v| (f64::from(v) / norm) as f32),
        })
    }
    fn interpolate(self, other: Self, t: f32) -> Self {
        Self {
            position: std::array::from_fn(|i| {
                (f64::from(self.position[i]) * (1. - f64::from(t))
                    + f64::from(other.position[i]) * f64::from(t)) as f32
            }),
            rotation: Quat::from_array(self.rotation)
                .slerp(Quat::from_array(other.rotation), t)
                .to_array(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmissionStats {
    pub requested: u64,
    pub spawned: u64,
    pub dropped_budget: u64,
    pub dropped_capacity: u64,
    pub expired: u64,
    pub invalid: u64,
}
impl EmissionStats {
    pub fn dropped(&self) -> u64 {
        self.dropped_budget.saturating_add(self.dropped_capacity)
    }
    fn add(&mut self, b: Self) {
        self.requested = self.requested.saturating_add(b.requested);
        self.spawned = self.spawned.saturating_add(b.spawned);
        self.dropped_budget = self.dropped_budget.saturating_add(b.dropped_budget);
        self.dropped_capacity = self.dropped_capacity.saturating_add(b.dropped_capacity);
        self.expired = self.expired.saturating_add(b.expired);
        self.invalid = self.invalid.saturating_add(b.invalid);
    }
}
struct Random(u64);
impl Random {
    fn new(seed: u64, sequence: u64) -> Self {
        Self(
            seed.wrapping_add(sequence.wrapping_mul(0x9e3779b97f4a7c15))
                .wrapping_add(0x4645525241564558),
        )
    }
    fn unit(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        if x == 0 {
            x = 0x4645525241564558;
        }
        self.0 = x;
        ((x.wrapping_mul(0x2545f4914f6cdd1d) >> 40) as u32) as f32 * (1. / 16777216.)
    }
    fn signed(&mut self) -> f32 {
        self.unit() * 2. - 1.
    }
    fn direction(&mut self) -> Vec3 {
        let z = self.signed();
        let angle = self.unit() * std::f32::consts::TAU;
        let radius = (1. - z * z).max(0.).sqrt();
        Vec3::new(radius * angle.cos(), radius * angle.sin(), z)
    }
}
/// Particle position/velocity and spawn shapes are emitter-local; acceleration
/// remains world-space so rotating a nozzle does not rotate gravity.
/// Counter-seeded emission: dropped work still advances the sequence without
/// iterating it, so pool capacity/quality cannot perturb a gameplay RNG or the
/// random values assigned to later requested particles. No per-frame allocation.
#[derive(Clone, Debug)]
pub struct Emitter {
    desc: EmitterDesc,
    seed: u64,
    sequence: u64,
    phase: f64,
    previous: Option<EmitterTransform>,
    total: EmissionStats,
}
impl Emitter {
    pub fn new(desc: EmitterDesc, seed: u64) -> Result<Self, ParticleError> {
        desc.validate()?;
        Ok(Self {
            desc,
            seed,
            sequence: 0,
            phase: 0.,
            previous: None,
            total: EmissionStats::default(),
        })
    }
    pub fn config(&self) -> EmitterDesc {
        self.desc
    }
    pub fn set_config(&mut self, desc: EmitterDesc) -> Result<(), ParticleError> {
        desc.validate()?;
        self.desc = desc;
        Ok(())
    }
    pub fn set_rate(&mut self, rate: f32) -> Result<(), ParticleError> {
        let mut desc = self.desc;
        desc.rate = rate;
        self.set_config(desc)
    }
    pub fn stats(&self) -> EmissionStats {
        self.total
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn reset(&mut self, seed: u64) {
        self.seed = seed;
        self.sequence = 0;
        self.phase = 0.;
        self.previous = None;
        self.total = EmissionStats::default();
    }
    pub fn teleport(&mut self, transform: EmitterTransform) -> Result<(), ParticleError> {
        let t = transform.normalized()?;
        self.previous = Some(t);
        self.phase = 0.;
        Ok(())
    }
    /// Update the existing pool first, then emit along previous->current motion.
    /// New particles catch up from their actual within-frame birth times. Large
    /// demand retains newest bounded samples; skipped demand never catches up later.
    pub fn update(
        &mut self,
        dt: f32,
        transform: EmitterTransform,
        pool: &mut ParticlePool,
    ) -> Result<EmissionStats, ParticleError> {
        if !dt.is_finite() || !(0.0..=60.).contains(&dt) {
            return Err(ParticleError::InvalidDelta);
        }
        let current = transform.normalized()?;
        let previous = self.previous.unwrap_or(current);
        let old_phase = self.phase;
        let demand = old_phase + f64::from(dt) * f64::from(self.desc.rate);
        let count = demand.floor() as u64;
        let velocity = if dt > 0. {
            std::array::from_fn(|i| {
                ((f64::from(current.position[i]) - f64::from(previous.position[i])) / f64::from(dt)
                    * f64::from(self.desc.inherit_velocity)) as f32
            })
        } else {
            [0.; 3]
        };
        let stats = self.emit(count, pool, |i| {
            let birth = if self.desc.rate > 0. {
                ((i + 1) as f64 - old_phase) / f64::from(self.desc.rate)
            } else {
                0.
            };
            let t = if dt > 0. {
                (birth / f64::from(dt)).clamp(0., 1.) as f32
            } else {
                1.
            };
            (
                previous.interpolate(current, t),
                (f64::from(dt) - birth).max(0.) as f32,
                velocity,
            )
        });
        self.phase = demand - count as f64;
        self.previous = Some(current);
        self.sequence = self.sequence.wrapping_add(count);
        self.total.add(stats);
        Ok(stats)
    }
    /// Burst samples share a birth time. Continuous rate phase is unchanged.
    pub fn burst(
        &mut self,
        count: u32,
        transform: EmitterTransform,
        pool: &mut ParticlePool,
    ) -> Result<EmissionStats, ParticleError> {
        let transform = transform.normalized()?;
        let stats = self.emit(u64::from(count), pool, |_| (transform, 0., [0.; 3]));
        self.sequence = self.sequence.wrapping_add(u64::from(count));
        self.total.add(stats);
        Ok(stats)
    }
    fn emit(
        &self,
        count: u64,
        pool: &mut ParticlePool,
        at: impl Fn(u64) -> (EmitterTransform, f32, [f32; 3]),
    ) -> EmissionStats {
        let budget = count.min(u64::from(self.desc.max_per_update));
        let selected = budget.min(pool.available() as u64);
        let mut stats = EmissionStats {
            requested: count,
            dropped_budget: count - budget,
            dropped_capacity: budget - selected,
            ..Default::default()
        };
        for i in count - selected..count {
            let (transform, age, inherited) = at(i);
            let mut rng = Random::new(self.seed, self.sequence.wrapping_add(i));
            let offset = match self.desc.shape {
                SpawnShape::Point => Vec3::ZERO,
                SpawnShape::Box { half } => Vec3::new(
                    half[0] * rng.signed(),
                    half[1] * rng.signed(),
                    half[2] * rng.signed(),
                ),
                SpawnShape::Sphere { radius } => rng.direction() * (radius * rng.unit().cbrt()),
                SpawnShape::Disc { radius } => {
                    let a = rng.unit() * std::f32::consts::TAU;
                    let r = radius * rng.unit().sqrt();
                    Vec3::new(a.cos() * r, a.sin() * r, 0.)
                }
            };
            let mut desc = self.desc.particle;
            let rotation = Quat::from_array(transform.rotation);
            desc.position = (Vec3::from_array(transform.position)
                + rotation * (Vec3::from_array(desc.position) + offset))
                .to_array();
            let velocity = if let Some(cone) = self.desc.velocity_cone {
                let z = 1. - rng.unit() * (1. - cone.angle.cos());
                let a = rng.unit() * std::f32::consts::TAU;
                let r = (1. - z * z).max(0.).sqrt();
                let direction = Quat::from_rotation_arc(Vec3::Z, Vec3::from_array(cone.axis))
                    * Vec3::new(r * a.cos(), r * a.sin(), z);
                direction * (cone.speed[0] + (cone.speed[1] - cone.speed[0]) * rng.unit())
            } else {
                Vec3::from_array(desc.velocity)
            };
            let jitter = Vec3::new(
                self.desc.velocity_jitter[0] * rng.signed(),
                self.desc.velocity_jitter[1] * rng.signed(),
                self.desc.velocity_jitter[2] * rng.signed(),
            );
            desc.velocity =
                (rotation * (velocity + jitter) + Vec3::from_array(inherited)).to_array();
            desc.lifetime *= 1. + self.desc.lifetime_jitter * rng.signed();
            let size = 1. + self.desc.size_jitter * rng.signed();
            desc.start_size *= size;
            desc.end_size *= size;
            let mut motion = self.desc.motion;
            motion.rotation += self.desc.rotation_jitter * rng.signed();
            match pool.spawn_aged(desc, motion, age) {
                Ok(()) => stats.spawned += 1,
                Err(ParticleError::Expired) => stats.expired += 1,
                Err(_) => stats.invalid += 1,
            }
        }
        stats
    }
}
