use super::{CameraError, valid_dt};
use ferravex_math::Vec3;

/// Bounded local-space motion with a quadratic decay envelope.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShakeImpulse {
    /// Maximum local right/up/forward displacement.
    pub translation: Vec3,
    /// Maximum rotation about local right/up/forward, radians.
    pub rotation: Vec3,
    /// Lifetime in seconds, finite and in `0.001..=10`.
    pub duration: f32,
    /// Noise samples per second, finite and in `0.1..=100`.
    pub frequency: f32,
}
impl ShakeImpulse {
    fn valid(self) -> bool {
        self.translation.is_finite()
            && self.translation.min_element() >= 0.
            && self.translation.max_element() <= 10.
            && self.rotation.is_finite()
            && self.rotation.min_element() >= 0.
            && self.rotation.max_element() <= 0.5
            && self.duration.is_finite()
            && (0.001..=10.).contains(&self.duration)
            && self.frequency.is_finite()
            && (0.1..=100.).contains(&self.frequency)
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ShakeSample {
    pub translation: Vec3,
    pub rotation: Vec3,
}

/// An owned snapshot of an active impulse; changing it does not change the shake.
///
/// ```
/// use ferravex_math::Vec3;
/// use ferravex_play::camera::{CameraShake, ShakeImpulse};
/// let mut shake = CameraShake::new(42);
/// shake.push(ShakeImpulse {
///     translation: Vec3::X, rotation: Vec3::ZERO, duration: 1., frequency: 20.,
/// })?;
/// shake.advance(0.25)?;
/// let envelope = shake.envelopes().next().unwrap();
/// assert_eq!(envelope.elapsed, 0.25);
/// assert_eq!(envelope.remaining, 0.75);
/// assert_eq!(envelope.strength, 0.5625);
/// # Ok::<(), ferravex_play::camera::CameraError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShakeEnvelope {
    /// The accepted, unscaled impulse.
    pub impulse: ShakeImpulse,
    /// Accumulated accepted delta time in seconds.
    pub elapsed: f64,
    /// Seconds until expiry; always positive for an active impulse.
    pub remaining: f64,
    /// Quadratic amplitude multiplier, `(1 - elapsed / duration)^2`, in `0..=1`.
    pub strength: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Layer {
    impulse: ShakeImpulse,
    time: f64,
    seed: u64,
}
impl Layer {
    fn envelope(self) -> ShakeEnvelope {
        let duration = f64::from(self.impulse.duration);
        ShakeEnvelope {
            impulse: self.impulse,
            elapsed: self.time,
            remaining: duration - self.time,
            strength: (1. - self.time / duration).powi(2) as f32,
        }
    }
}

/// Allocation-free seeded shake, driven only by explicitly supplied delta time.
///
/// Identical seeds, accepted non-silent impulses and delta-time sequences yield
/// identical samples on the same target. Use fixed deltas for replay; different
/// floating-point partitions of elapsed time can differ by rounding. Sampling
/// does not advance time. Shake affects presentation, never simulation timing.
///
/// At most [`Self::CAPACITY`] impulses are summed in slot order. Each impulse's
/// translation is limited to 10 per axis and rotation to 0.5 radians per axis,
/// bounding the aggregate by 80 and 4 respectively before caller-side scaling.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraShake {
    layers: [Option<Layer>; Self::CAPACITY],
    seed: u64,
    sequence: u64,
}
impl CameraShake {
    /// Maximum simultaneous non-silent impulses. Pushing beyond this fails
    /// with [`CameraError::ShakeCapacity`] without replacing an existing layer.
    pub const CAPACITY: usize = 8;

    pub const fn new(seed: u64) -> Self {
        Self {
            layers: [None; Self::CAPACITY],
            seed,
            sequence: 0,
        }
    }
    pub fn active(&self) -> usize {
        self.layers.iter().filter(|v| v.is_some()).count()
    }
    /// Cancel all active impulses, preserving the seed and next noise sequence.
    /// Use [`Self::reset`] to reproduce the original sequence on a restart.
    pub fn clear(&mut self) {
        self.layers.fill(None);
    }

    /// Cancel every impulse and rewind the noise sequence, retaining the seed.
    /// The result equals a fresh [`Self::new`] with the original seed.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::camera::{CameraShake, ShakeImpulse};
    /// let mut shake = CameraShake::new(42);
    /// let impulse = ShakeImpulse {
    ///     translation: Vec3::X, rotation: Vec3::ZERO, duration: 1., frequency: 20.,
    /// };
    /// shake.push(impulse)?;
    /// let first = shake.advance(0.125)?;
    /// shake.reset();
    /// shake.push(impulse)?;
    /// assert_eq!(shake.advance(0.125)?, first);
    /// # Ok::<(), ferravex_play::camera::CameraError>(())
    /// ```
    pub fn reset(&mut self) {
        self.clear();
        self.sequence = 0;
    }

    /// Inspect active impulses without advancing time or allocating.
    ///
    /// Iteration follows storage slot order, not insertion order: expired or
    /// cancelled slots are reused. Snapshots are not stable impulse handles.
    /// See [`ShakeEnvelope`] for an inspection example.
    pub fn envelopes(&self) -> impl Iterator<Item = ShakeEnvelope> + '_ {
        self.layers.iter().flatten().map(|layer| layer.envelope())
    }

    /// Cancel matching active impulses, returning the number removed.
    ///
    /// The predicate runs once per active impulse in slot order. Remaining
    /// impulses keep their elapsed times and seeds; cancellation does not rewind
    /// the noise sequence. Use [`Self::clear`] to cancel everything.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::camera::{CameraShake, ShakeImpulse};
    /// let mut shake = CameraShake::new(42);
    /// shake.push(ShakeImpulse {
    ///     translation: Vec3::X, rotation: Vec3::ZERO, duration: 1., frequency: 20.,
    /// })?;
    /// assert_eq!(shake.cancel_where(|state| state.impulse.rotation == Vec3::ZERO), 1);
    /// assert_eq!(shake.active(), 0);
    /// # Ok::<(), ferravex_play::camera::CameraError>(())
    /// ```
    pub fn cancel_where(&mut self, mut predicate: impl FnMut(ShakeEnvelope) -> bool) -> usize {
        let mut cancelled = 0;
        for slot in &mut self.layers {
            if slot.is_some_and(|layer| predicate(layer.envelope())) {
                *slot = None;
                cancelled += 1;
            }
        }
        cancelled
    }

    /// Add an impulse with a fresh noise seed.
    ///
    /// Invalid inputs and capacity failures leave all state unchanged. A valid
    /// impulse with zero translation and rotation succeeds without using a slot
    /// or advancing the seed sequence, even when all slots are occupied.
    pub fn push(&mut self, impulse: ShakeImpulse) -> Result<(), CameraError> {
        if !impulse.valid() {
            return Err(CameraError::InvalidInput);
        }
        if impulse.translation == Vec3::ZERO && impulse.rotation == Vec3::ZERO {
            return Ok(());
        }
        let slot = self
            .layers
            .iter_mut()
            .find(|v| v.is_none())
            .ok_or(CameraError::ShakeCapacity)?;
        *slot = Some(Layer {
            impulse,
            time: 0.,
            seed: mix(self.seed.wrapping_add(self.sequence)),
        });
        self.sequence = self.sequence.wrapping_add(1);
        Ok(())
    }
    /// Advance by a finite delta in `0..=0.25` seconds, then sample active motion.
    /// Invalid deltas leave all state unchanged. Zero delta only samples; an
    /// impulse expires at its duration and contributes exactly zero thereafter.
    pub fn advance(&mut self, dt: f32) -> Result<ShakeSample, CameraError> {
        if !valid_dt(dt) {
            return Err(CameraError::InvalidInput);
        }
        for slot in &mut self.layers {
            let Some(layer) = slot else {
                continue;
            };
            layer.time += f64::from(dt);
            if layer.time >= f64::from(layer.impulse.duration) {
                *slot = None;
            }
        }
        Ok(self.sample())
    }

    /// Sample the current aggregate without advancing time or changing state.
    /// An empty shake yields exactly zero translation and rotation.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::camera::{CameraShake, ShakeImpulse, ShakeSample};
    /// let mut shake = CameraShake::new(42);
    /// shake.push(ShakeImpulse {
    ///     translation: Vec3::X, rotation: Vec3::ZERO, duration: 1., frequency: 20.,
    /// })?;
    /// let current = shake.sample();
    /// assert_eq!(shake.advance(0.)?, current);
    /// shake.clear();
    /// assert_eq!(shake.sample(), ShakeSample::default());
    /// assert_eq!(CameraShake::CAPACITY, 8);
    /// # Ok::<(), ferravex_play::camera::CameraError>(())
    /// ```
    pub fn sample(&self) -> ShakeSample {
        let mut result = ShakeSample::default();
        for layer in self.layers.iter().flatten() {
            let envelope = layer.envelope().strength;
            let sample = |axis| {
                noise(
                    layer.seed.wrapping_add(axis),
                    layer.time * layer.impulse.frequency as f64,
                ) * envelope
            };
            result.translation +=
                layer.impulse.translation * Vec3::new(sample(0), sample(1), sample(2));
            result.rotation += layer.impulse.rotation * Vec3::new(sample(3), sample(4), sample(5));
        }
        result
    }
}
fn mix(mut n: u64) -> u64 {
    n ^= n >> 30;
    n = n.wrapping_mul(0xbf58476d1ce4e5b9);
    n ^= n >> 27;
    n = n.wrapping_mul(0x94d049bb133111eb);
    n ^ (n >> 31)
}
fn noise(seed: u64, time: f64) -> f32 {
    let index = time.floor() as u64;
    let fraction = (time - index as f64) as f32;
    let t = fraction * fraction * (3. - 2. * fraction);
    let value = |i| ((mix(seed.wrapping_add(i)) >> 40) as f32 / (1_u32 << 24) as f32) * 2. - 1.;
    let a = value(index);
    a + (value(index + 1) - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse() -> ShakeImpulse {
        ShakeImpulse {
            translation: Vec3::new(1., 2., 3.),
            rotation: Vec3::new(0.1, 0.2, 0.3),
            duration: 1.,
            frequency: 20.,
        }
    }

    #[test]
    fn inspection_tracks_quadratic_decay_and_exact_expiry() {
        let mut shake = CameraShake::new(42);
        shake.push(impulse()).unwrap();
        for (elapsed, strength) in [(0., 1.), (0.25, 0.5625), (0.5, 0.25), (0.75, 0.0625)] {
            let before = shake;
            let states: Vec<_> = shake.envelopes().collect();
            assert_eq!(states.len(), 1);
            assert_eq!(states[0].impulse, impulse());
            assert_eq!(states[0].elapsed, elapsed);
            assert_eq!(states[0].remaining, 1. - elapsed);
            assert_eq!(states[0].strength, strength);
            assert_eq!(shake, before);
            assert_eq!(shake.advance(0.25).unwrap(), shake.sample());
        }
        assert_eq!(shake.active(), 0);
        assert_eq!(shake.envelopes().count(), 0);
        assert_eq!(shake.sample(), ShakeSample::default());
        assert_eq!(shake.advance(0.25).unwrap(), ShakeSample::default());
    }

    #[test]
    fn samples_and_zero_delta_do_not_advance_or_change_state() {
        let mut shake = CameraShake::new(42);
        shake.push(impulse()).unwrap();
        shake.advance(0.125).unwrap();
        let before = shake;
        let current = shake.sample();
        assert_ne!(current, ShakeSample::default());
        for _ in 0..4 {
            assert_eq!(shake.sample(), current);
            assert_eq!(shake.advance(0.).unwrap(), current);
            assert_eq!(shake, before);
        }
        let mut snapshot = shake.envelopes().next().unwrap();
        snapshot.impulse.translation = Vec3::ZERO;
        assert_ne!(snapshot, shake.envelopes().next().unwrap());
        assert_eq!(shake, before);
    }

    #[test]
    fn selective_cancellation_preserves_other_envelopes_and_the_seed_sequence() {
        let first = ShakeImpulse {
            duration: 0.25,
            ..impulse()
        };
        let second = impulse();
        let mut shake = CameraShake::new(42);
        shake.push(first).unwrap();
        shake.push(second).unwrap();
        shake.advance(0.125).unwrap();
        let mut surviving = CameraShake::new(42);
        surviving.push(first).unwrap();
        surviving.clear();
        surviving.push(second).unwrap();
        surviving.advance(0.125).unwrap();

        let before = shake;
        let mut calls = 0;
        assert_eq!(
            shake.cancel_where(|_| {
                calls += 1;
                false
            }),
            0
        );
        assert_eq!(calls, 2);
        assert_eq!(shake, before);
        assert_eq!(shake.cancel_where(|state| state.remaining < 0.25), 1);
        assert_eq!(shake.active(), 1);
        assert_eq!(shake.envelopes().next(), surviving.envelopes().next());
        assert_eq!(shake.sample(), surviving.sample());
        assert_eq!(shake.advance(0.125), surviving.advance(0.125));

        assert_eq!(shake.cancel_where(|_| true), 1);
        surviving.clear();
        assert_eq!(shake, surviving);
        assert_eq!(shake.sample(), ShakeSample::default());
        assert_eq!(shake.cancel_where(|_| panic!("no active impulse")), 0);
        shake.push(first).unwrap();
        surviving.push(first).unwrap();
        assert_eq!(shake.advance(0.125), surviving.advance(0.125));
    }

    #[test]
    fn clear_preserves_sequence_while_reset_replays_the_original_samples() {
        let mut original = CameraShake::new(u64::MAX);
        original.push(impulse()).unwrap();
        let mut cleared = original;
        let mut reset = original;
        cleared.advance(0.25).unwrap();
        cleared.clear();
        assert_eq!(cleared.sample(), ShakeSample::default());
        cleared.push(impulse()).unwrap();
        assert_ne!(cleared.sample(), original.sample());

        reset.advance(0.25).unwrap();
        reset.reset();
        assert_eq!(reset, CameraShake::new(u64::MAX));
        reset.reset();
        reset.push(impulse()).unwrap();
        assert_eq!(reset, original);
        for _ in 0..64 {
            assert_eq!(reset.advance(1. / 64.), original.advance(1. / 64.));
        }
        assert_eq!(reset.active(), 0);
        reset.reset();
        assert_eq!(reset, CameraShake::new(u64::MAX));
    }

    #[test]
    fn silent_impulses_never_use_capacity_or_shift_future_motion() {
        let silent = ShakeImpulse {
            translation: Vec3::new(0., -0., 0.),
            rotation: Vec3::ZERO,
            ..impulse()
        };
        let mut shake = CameraShake::new(42);
        let before = shake;
        for _ in 0..CameraShake::CAPACITY + 1 {
            shake.push(silent).unwrap();
        }
        assert_eq!(shake, before);
        assert_eq!(shake.advance(0.25).unwrap(), ShakeSample::default());
        for _ in 0..CameraShake::CAPACITY {
            shake.push(impulse()).unwrap();
        }
        let before = shake;
        shake.push(silent).unwrap();
        assert_eq!(shake, before);
        assert_eq!(shake.push(impulse()), Err(CameraError::ShakeCapacity));
        assert_eq!(shake, before);
        assert_eq!(
            shake.push(ShakeImpulse {
                duration: 0.,
                ..silent
            }),
            Err(CameraError::InvalidInput)
        );
        assert_eq!(shake, before);
    }

    #[test]
    fn invalid_inputs_and_deltas_are_transactional() {
        let mut shake = CameraShake::new(42);
        shake.push(impulse()).unwrap();
        shake.advance(0.125).unwrap();
        let before = shake;
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 11.] {
            let invalid = [
                ShakeImpulse {
                    translation: Vec3::new(value, 0., 0.),
                    ..impulse()
                },
                ShakeImpulse {
                    rotation: Vec3::new(0., value, 0.),
                    ..impulse()
                },
                ShakeImpulse {
                    duration: value,
                    ..impulse()
                },
            ];
            for bad in invalid {
                assert_eq!(shake.push(bad), Err(CameraError::InvalidInput));
                assert_eq!(shake, before);
            }
        }
        for frequency in [f32::NAN, f32::INFINITY, -1., 0., 0.099, 100.001] {
            assert_eq!(
                shake.push(ShakeImpulse {
                    frequency,
                    ..impulse()
                }),
                Err(CameraError::InvalidInput)
            );
            assert_eq!(shake, before);
        }
        for dt in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.001, 0.250001] {
            assert_eq!(shake.advance(dt), Err(CameraError::InvalidInput));
            assert_eq!(shake, before);
        }
    }

    #[test]
    fn maximum_aggregation_stays_finite_bounded_and_expires() {
        let maximum = ShakeImpulse {
            translation: Vec3::splat(10.),
            rotation: Vec3::splat(0.5),
            duration: 10.,
            frequency: 100.,
        };
        for seed in [0, 1, 42, u64::MAX] {
            let mut shake = CameraShake::new(seed);
            for _ in 0..CameraShake::CAPACITY {
                shake.push(maximum).unwrap();
            }
            for _ in 0..40 {
                let envelope = shake.envelopes().next().unwrap().strength;
                let sample = shake.sample();
                assert!(sample.translation.is_finite());
                assert!(sample.rotation.is_finite());
                assert!(sample.translation.abs().max_element() <= 80. * envelope + 1e-5);
                assert!(sample.rotation.abs().max_element() <= 4. * envelope + 1e-6);
                assert_eq!(shake.active(), CameraShake::CAPACITY);
                shake.advance(0.25).unwrap();
            }
            assert_eq!(shake.active(), 0);
            assert_eq!(shake.sample(), ShakeSample::default());
        }
    }

    #[test]
    fn cancellation_and_expiry_free_slots_without_compacting_survivors() {
        let short = ShakeImpulse {
            duration: 0.125,
            ..impulse()
        };
        let long = impulse();
        let mut shake = CameraShake::new(42);
        shake.push(short).unwrap();
        for _ in 1..CameraShake::CAPACITY {
            shake.push(long).unwrap();
        }
        shake.advance(0.125).unwrap();
        assert_eq!(shake.active(), CameraShake::CAPACITY - 1);
        shake.push(short).unwrap();
        {
            let mut states = shake.envelopes();
            assert_eq!(states.next().unwrap().elapsed, 0.);
            assert!(states.all(|state| state.elapsed == 0.125));
        }
        assert_eq!(shake.cancel_where(|state| state.impulse == short), 1);
        shake.push(short).unwrap();
        assert_eq!(shake.active(), CameraShake::CAPACITY);
        assert_eq!(shake.envelopes().next().unwrap().elapsed, 0.);
    }

    #[test]
    fn exact_time_partitions_and_extra_observations_leave_decay_deterministic() {
        let mut reference = CameraShake::new(42);
        reference.push(impulse()).unwrap();
        reference.advance(0.25).unwrap();
        reference.advance(0.25).unwrap();
        for steps in [4, 8, 32, 64] {
            let mut shake = CameraShake::new(42);
            shake.push(impulse()).unwrap();
            for _ in 0..steps {
                shake.advance(0.5 / steps as f32).unwrap();
                let before = shake;
                let _ = shake.sample();
                let _ = shake.envelopes().next();
                assert_eq!(shake, before);
            }
            assert_eq!(shake, reference);
            assert_eq!(shake.sample(), reference.sample());
        }
    }
}
