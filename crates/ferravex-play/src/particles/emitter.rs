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
    /// Maximum samples attempted by each update or burst call. Separate calls
    /// do not share a budget; use [`Emitter::burst_with_budget`] to lower it.
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
/// Read-only burst admission at the pool's current occupancy.
///
/// `requested == admitted + dropped_budget + dropped_capacity`. Admission
/// bounds the work attempted, not successful spawns: generated particles can
/// still fail validation. The actual outcome is returned as [`EmissionStats`].
/// This value reserves no slots and remains applicable only while the emitter
/// configuration and pool occupancy are unchanged. See [`Emitter::burst_admission`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BurstAdmission {
    /// Total demand, including particles that will be skipped.
    pub requested: u64,
    /// Maximum samples attempted, bounded by both budgets and available slots.
    pub admitted: u64,
    /// Demand beyond the smaller of the caller and configured budgets.
    pub dropped_budget: u64,
    /// Budgeted demand beyond the pool's currently available slots.
    pub dropped_capacity: u64,
}
impl BurstAdmission {
    fn new(count: u64, budget: u16, available: usize) -> Self {
        let budget = count.min(u64::from(budget));
        let admitted = budget.min(available as u64);
        Self {
            requested: count,
            admitted,
            dropped_budget: count - budget,
            dropped_capacity: budget - admitted,
        }
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
        let stats = self.emit(count, self.desc.max_per_update, pool, |i| {
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
    /// Uses the configured per-call maximum; see [`Self::burst_with_budget`]
    /// for a caller-supplied lower budget and detailed admission semantics.
    pub fn burst(
        &mut self,
        count: u32,
        transform: EmitterTransform,
        pool: &mut ParticlePool,
    ) -> Result<EmissionStats, ParticleError> {
        self.burst_with_budget(count, self.desc.max_per_update, transform, pool)
    }
    /// Preview a burst's work and capacity limits without allocating, consuming
    /// sequence numbers, changing statistics or reserving pool slots.
    ///
    /// `budget` is a per-call limit, capped by [`EmitterDesc::max_per_update`].
    /// Zero suppresses all samples. Budget drops are counted before capacity
    /// drops, so every requested particle belongs to exactly one category.
    /// This query does not validate a transform or sample any particles.
    ///
    /// ```
    /// use ferravex_play::particles::{BurstAdmission, Emitter, EmitterDesc, ParticlePool};
    /// let emitter = Emitter::new(EmitterDesc::default(), 42)?;
    /// let pool = ParticlePool::new(3);
    /// let plan: BurstAdmission = emitter.burst_admission(10, 4, &pool);
    /// assert_eq!((plan.admitted, plan.dropped_budget, plan.dropped_capacity), (3, 6, 1));
    /// assert_eq!(emitter.sequence(), 0);
    /// assert!(pool.is_empty());
    /// # Ok::<(), ferravex_play::particles::ParticleError>(())
    /// ```
    pub fn burst_admission(&self, count: u32, budget: u16, pool: &ParticlePool) -> BurstAdmission {
        BurstAdmission::new(
            u64::from(count),
            budget.min(self.desc.max_per_update),
            pool.available(),
        )
    }
    /// Emit an immediate burst with an additional per-call work limit.
    ///
    /// At most `min(count, budget, config.max_per_update, pool.available())`
    /// samples are attempted. Excess demand is dropped without iteration or
    /// deferred catch-up; zero budget is valid. Budgets above the configured
    /// maximum are clamped. Multiple calls have independent budgets: callers
    /// enforcing a shared frame budget must pass its remaining allowance.
    ///
    /// As with [`Self::burst`], the newest admitted sequence numbers are sampled
    /// and **all** requested particles advance the wrapping seeded sequence,
    /// including dropped or invalid samples. The result accounts for each
    /// request in `spawned`, `dropped_budget`, `dropped_capacity`, `expired` or
    /// `invalid`. Admitted samples that fail validation are not replaced.
    ///
    /// A non-finite or degenerate transform returns [`ParticleError::InvalidParticle`]
    /// without mutating the emitter or pool, even with zero count or budget.
    /// Bursts never age existing particles, change the continuous rate phase or
    /// motion history, or allocate storage. New particles have age zero and no
    /// inherited emitter velocity. Use [`Self::burst_admission`] to inspect the
    /// limits beforehand; it is a snapshot, not a reservation.
    ///
    /// ```
    /// use ferravex_play::particles::{Emitter, EmitterDesc, EmitterTransform, ParticlePool};
    /// let mut emitter = Emitter::new(EmitterDesc::default(), 42)?;
    /// let mut pool = ParticlePool::new(2);
    /// let result = emitter.burst_with_budget(8, 3, EmitterTransform::default(), &mut pool)?;
    /// assert_eq!((result.spawned, result.dropped_budget, result.dropped_capacity), (2, 5, 1));
    /// assert_eq!(emitter.sequence(), 8);
    /// let muted = emitter.burst_with_budget(5, 0, EmitterTransform::default(), &mut pool)?;
    /// assert_eq!(muted.dropped_budget, 5);
    /// assert_eq!(emitter.sequence(), 13);
    /// # Ok::<(), ferravex_play::particles::ParticleError>(())
    /// ```
    pub fn burst_with_budget(
        &mut self,
        count: u32,
        budget: u16,
        transform: EmitterTransform,
        pool: &mut ParticlePool,
    ) -> Result<EmissionStats, ParticleError> {
        let transform = transform.normalized()?;
        let stats = self.emit(
            u64::from(count),
            budget.min(self.desc.max_per_update),
            pool,
            |_| (transform, 0., [0.; 3]),
        );
        self.sequence = self.sequence.wrapping_add(u64::from(count));
        self.total.add(stats);
        Ok(stats)
    }
    fn emit(
        &self,
        count: u64,
        budget: u16,
        pool: &mut ParticlePool,
        at: impl Fn(u64) -> (EmitterTransform, f32, [f32; 3]),
    ) -> EmissionStats {
        let admission = BurstAdmission::new(count, budget, pool.available());
        let mut stats = EmissionStats {
            requested: count,
            dropped_budget: admission.dropped_budget,
            dropped_capacity: admission.dropped_capacity,
            ..Default::default()
        };
        for i in count - admission.admitted..count {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_emitter() -> Emitter {
        Emitter::new(
            EmitterDesc {
                shape: SpawnShape::Sphere { radius: 2. },
                velocity_cone: Some(DirectionCone {
                    axis: [0., 1., 0.],
                    angle: 0.5,
                    speed: [1., 3.],
                }),
                velocity_jitter: [0.5; 3],
                lifetime_jitter: 0.25,
                size_jitter: 0.5,
                rotation_jitter: 1.,
                max_per_update: 4,
                ..Default::default()
            },
            42,
        )
        .unwrap()
    }

    fn assert_accounted(stats: EmissionStats) {
        assert_eq!(
            stats.requested,
            stats.spawned
                + stats.dropped_budget
                + stats.dropped_capacity
                + stats.expired
                + stats.invalid
        );
    }

    #[test]
    fn admission_matches_zero_partial_full_and_clamped_bursts() {
        for (count, budget, capacity, occupied, expected) in [
            (0, 4, 2, 0, (0, 0, 0)),
            (7, 0, 2, 0, (0, 7, 0)),
            (7, 3, 0, 0, (0, 4, 3)),
            (7, 3, 2, 2, (0, 4, 3)),
            (7, 3, 2, 1, (1, 4, 2)),
            (7, 3, 8, 0, (3, 4, 0)),
            (7, u16::MAX, 8, 0, (4, 3, 0)),
            (2, u16::MAX, 8, 0, (2, 0, 0)),
            (u32::MAX, 2, 1, 0, (1, u64::from(u32::MAX) - 2, 1)),
        ] {
            let mut emitter = seeded_emitter();
            let mut pool = ParticlePool::new(capacity);
            for _ in 0..occupied {
                pool.spawn(ParticleDesc::default()).unwrap();
            }
            let before = pool.iter().collect::<Vec<_>>();
            let plan = emitter.burst_admission(count, budget, &pool);
            assert_eq!(plan.requested, u64::from(count));
            assert_eq!(
                (plan.admitted, plan.dropped_budget, plan.dropped_capacity),
                expected
            );
            assert_eq!(
                plan.requested,
                plan.admitted + plan.dropped_budget + plan.dropped_capacity
            );
            assert_eq!(emitter.sequence(), 0);
            assert_eq!(emitter.stats(), EmissionStats::default());
            assert_eq!(pool.iter().collect::<Vec<_>>(), before);
            let result = emitter
                .burst_with_budget(count, budget, EmitterTransform::default(), &mut pool)
                .unwrap();
            assert_accounted(result);
            assert_eq!(
                (
                    result.spawned,
                    result.dropped_budget,
                    result.dropped_capacity
                ),
                expected
            );
            assert_eq!(pool.len(), occupied + plan.admitted as usize);
            assert_eq!(emitter.sequence(), u64::from(count));
            assert_eq!(emitter.stats(), result);
            assert_eq!(pool.dropped(), 0);
        }
    }

    #[test]
    fn admission_is_a_snapshot_and_burst_rechecks_capacity() {
        let mut emitter = seeded_emitter();
        let mut pool = ParticlePool::new(3);
        assert_eq!(emitter.burst_admission(4, 4, &pool).admitted, 3);
        pool.spawn(ParticleDesc::default()).unwrap();
        let result = emitter
            .burst_with_budget(4, 4, EmitterTransform::default(), &mut pool)
            .unwrap();
        assert_eq!(result.spawned, 2);
        assert_eq!(result.dropped_capacity, 2);
        assert_accounted(result);
        assert_eq!(pool.len(), 3);
    }

    #[test]
    fn budget_drops_keep_newest_seeded_samples_and_later_sequence_after_wrap() {
        let mut full = seeded_emitter();
        full.sequence = u64::MAX - 2;
        let mut partial = full.clone();
        let mut muted = full.clone();
        let mut full_pool = ParticlePool::new(4);
        let mut partial_pool = ParticlePool::new(4);
        let mut muted_pool = ParticlePool::new(4);
        for count in [12, u32::MAX] {
            full_pool.clear();
            partial_pool.clear();
            full.burst(count, EmitterTransform::default(), &mut full_pool)
                .unwrap();
            partial
                .burst_with_budget(count, 2, EmitterTransform::default(), &mut partial_pool)
                .unwrap();
            let result = muted
                .burst_with_budget(count, 0, EmitterTransform::default(), &mut muted_pool)
                .unwrap();
            assert_eq!(result.dropped_budget, u64::from(count));
            assert!(muted_pool.is_empty());
            assert_eq!(full.sequence(), partial.sequence());
            assert_eq!(full.sequence(), muted.sequence());
            assert_eq!(
                partial_pool.iter().collect::<Vec<_>>(),
                full_pool.iter().skip(2).collect::<Vec<_>>()
            );
        }
        for (emitter, pool) in [
            (&mut full, &mut full_pool),
            (&mut partial, &mut partial_pool),
            (&mut muted, &mut muted_pool),
        ] {
            pool.clear();
            emitter
                .burst_with_budget(1, 1, EmitterTransform::default(), pool)
                .unwrap();
            assert_accounted(emitter.stats());
        }
        assert_eq!(full_pool.iter().next(), partial_pool.iter().next());
        assert_eq!(full_pool.iter().next(), muted_pool.iter().next());
        partial.reset(42);
        partial_pool.clear();
        full = seeded_emitter();
        full_pool.clear();
        partial
            .burst_with_budget(4, 4, EmitterTransform::default(), &mut partial_pool)
            .unwrap();
        full.burst(4, EmitterTransform::default(), &mut full_pool)
            .unwrap();
        assert_eq!(partial.stats(), full.stats());
        assert_eq!(
            full_pool.iter().collect::<Vec<_>>(),
            partial_pool.iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn invalid_transforms_are_atomic_even_for_zero_count_or_budget() {
        let mut emitter = seeded_emitter();
        let mut pool = ParticlePool::new(4);
        emitter
            .update(0.125, EmitterTransform::at([2., 0., 0.]), &mut pool)
            .unwrap();
        let before = emitter.clone();
        let particles = pool.iter().collect::<Vec<_>>();
        for transform in [
            EmitterTransform::at([f32::NAN, 0., 0.]),
            EmitterTransform {
                rotation: [0.; 4],
                ..Default::default()
            },
            EmitterTransform {
                rotation: [0., 0., 0., f32::INFINITY],
                ..Default::default()
            },
        ] {
            for (count, budget) in [(0, 4), (4, 0), (4, 4)] {
                assert_eq!(
                    emitter.burst_with_budget(count, budget, transform, &mut pool),
                    Err(ParticleError::InvalidParticle)
                );
                assert_eq!(emitter.config(), before.config());
                assert_eq!(emitter.sequence(), before.sequence());
                assert_eq!(emitter.stats(), before.stats());
                assert_eq!(emitter.phase, before.phase);
                assert_eq!(emitter.previous, before.previous);
                assert_eq!(pool.iter().collect::<Vec<_>>(), particles);
            }
        }
    }

    #[test]
    fn admitted_samples_can_be_invalid_without_refilling_or_losing_accounting() {
        let mut emitter = Emitter::new(
            EmitterDesc {
                particle: ParticleDesc {
                    position: [f32::MAX, 0., 0.],
                    ..Default::default()
                },
                ..Default::default()
            },
            42,
        )
        .unwrap();
        let mut pool = ParticlePool::new(1);
        assert_eq!(emitter.burst_admission(5, 2, &pool).admitted, 1);
        let result = emitter
            .burst_with_budget(5, 2, EmitterTransform::at([f32::MAX, 0., 0.]), &mut pool)
            .unwrap();
        assert_eq!(result.invalid, 1);
        assert_eq!(result.spawned, 0);
        assert_eq!(result.dropped_budget, 3);
        assert_eq!(result.dropped_capacity, 1);
        assert_accounted(result);
        assert!(pool.is_empty());
        assert_eq!(emitter.sequence(), 5);
    }

    #[test]
    fn bursts_preserve_continuous_birth_timing_and_motion_history() {
        let mut emitter = Emitter::new(
            EmitterDesc {
                rate: 4.,
                inherit_velocity: 1.,
                ..Default::default()
            },
            42,
        )
        .unwrap();
        emitter.teleport(EmitterTransform::default()).unwrap();
        let mut reference = emitter.clone();
        let mut pool = ParticlePool::new(4);
        let mut reference_pool = ParticlePool::new(4);
        for (e, p) in [
            (&mut emitter, &mut pool),
            (&mut reference, &mut reference_pool),
        ] {
            assert_eq!(
                e.update(0.125, EmitterTransform::at([1., 0., 0.]), p)
                    .unwrap()
                    .requested,
                0
            );
        }
        let mut burst_pool = ParticlePool::new(3);
        burst_pool.spawn(ParticleDesc::default()).unwrap();
        burst_pool.update(0.25).unwrap();
        let existing = burst_pool.iter().next().unwrap();
        emitter
            .burst_with_budget(5, 2, EmitterTransform::at([1000., 0., 0.]), &mut burst_pool)
            .unwrap();
        assert_eq!(burst_pool.iter().next(), Some(existing));
        for particle in burst_pool.iter().skip(1) {
            assert_eq!(particle.position, [1000., 0., 0.]);
            assert_eq!(particle.velocity, [0.; 3]);
            assert_eq!(particle.normalized_age, 0.);
        }
        for (dt, x) in [(0.25, 3.), (0.125, 4.)] {
            pool.update(dt).unwrap();
            reference_pool.update(dt).unwrap();
            assert_eq!(
                emitter
                    .update(dt, EmitterTransform::at([x, 0., 0.]), &mut pool)
                    .unwrap(),
                reference
                    .update(dt, EmitterTransform::at([x, 0., 0.]), &mut reference_pool)
                    .unwrap()
            );
            assert_eq!(
                pool.iter().collect::<Vec<_>>(),
                reference_pool.iter().collect::<Vec<_>>()
            );
        }
        assert_eq!(pool.len(), 2);
        assert_eq!(emitter.sequence(), reference.sequence() + 5);
    }

    #[test]
    fn repeated_budgeted_bursts_reuse_pool_storage_and_never_queue_drops() {
        let mut emitter = seeded_emitter();
        let mut pool = ParticlePool::new(3);
        emitter
            .burst(3, EmitterTransform::default(), &mut pool)
            .unwrap();
        let storage = pool.particles.as_ptr();
        let capacity = pool.particles.capacity();
        for i in 0..256 {
            pool.clear();
            let budget = i % 5;
            let plan = emitter.burst_admission(u32::MAX, budget, &pool);
            let result = emitter
                .burst_with_budget(u32::MAX, budget, EmitterTransform::default(), &mut pool)
                .unwrap();
            assert_eq!(result.spawned, plan.admitted);
            assert_accounted(result);
            assert_eq!(pool.particles.as_ptr(), storage);
            assert_eq!(pool.particles.capacity(), capacity);
            pool.clear();
            assert_eq!(
                emitter
                    .update(0., EmitterTransform::default(), &mut pool)
                    .unwrap(),
                EmissionStats::default()
            );
            assert!(pool.is_empty());
        }
    }
}
