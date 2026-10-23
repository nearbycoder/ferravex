//! Presentation-only transforms. Ghosts never enter collision or game authority.
//!
//! [`GhostTrack::sample_checked`] distinguishes missing history from invalid
//! queries and exposes exact keys, interpolation and holds before teleports.
//! Queries never advance replay time or mutate the recorded samples.
use super::ReplayError;
use ferravex_math::{Quat, Vec3};
pub const MAX_SAMPLES: usize = 120_000;
/// Recorded pose at an integer simulation tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub tick: u64,
    pub position: Vec3,
    pub rotation: Quat,
    /// This key starts a discontinuity: hold the preceding key until this tick.
    /// Motion from this key to the next is continuous unless that next key also
    /// has `teleport` set. The recorder must mark teleports explicitly.
    pub teleport: bool,
}

/// Placement and presentation pose of a checked ghost query.
///
/// Exact keys retain their stored teleport flag. Synthesized poses clear it;
/// [`Held`](Self::Held) explicitly identifies a discontinuous interval instead.
/// These states describe the requested time, not an event stream: repeating an
/// exact query returns the same key, including its teleport flag.
/// See [`GhostTrack::sample_checked`] for a complete example.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SampleState {
    /// No recorded poses, including a zero-capacity track.
    Empty,
    /// Earlier than the first recorded tick; no pose is synthesized.
    BeforeStart {
        /// First recorded pose, available for an explicit caller-owned hold.
        first: Sample,
    },
    /// Later than the last recorded tick; no pose is synthesized.
    AfterEnd {
        /// Last recorded pose, available for an explicit caller-owned hold.
        last: Sample,
    },
    /// Exactly a recorded tick with zero alpha; the stored pose is unchanged.
    Exact(Sample),
    /// Continuous motion between two distinct recorded ticks.
    Interpolated {
        /// Interpolated pose, with the query's integer tick and no teleport flag.
        sample: Sample,
        /// Recorded tick preceding the query.
        from_tick: u64,
        /// Recorded tick following the query.
        to_tick: u64,
    },
    /// Holding the preceding pose until a marked teleport; neither position nor
    /// rotation is blended across the discontinuity.
    Held {
        /// Held pose, with the query's integer tick and no teleport flag.
        sample: Sample,
        /// Tick of the recorded pose being held.
        from_tick: u64,
        /// Tick at which the new pose becomes visible.
        teleport_tick: u64,
    },
}

impl SampleState {
    /// Return an in-range presentation pose without clamping or extrapolation.
    ///
    /// ```
    /// use ferravex_play::replay::ghost::{GhostTrack, Sample};
    /// use ferravex_math::{Quat, Vec3};
    ///
    /// let mut track = GhostTrack::new(7, 60, 1)?;
    /// track.push(Sample {
    ///     tick: 10, position: Vec3::ZERO, rotation: Quat::IDENTITY, teleport: true,
    /// })?;
    /// assert!(track.sample_checked(10, 0.0)?.sample().unwrap().teleport);
    /// assert!(track.sample_checked(10, 0.5)?.sample().is_none());
    /// # Ok::<(), ferravex_play::replay::ReplayError>(())
    /// ```
    pub fn sample(self) -> Option<Sample> {
        match self {
            Self::Exact(sample) | Self::Interpolated { sample, .. } | Self::Held { sample, .. } => {
                Some(sample)
            }
            Self::Empty | Self::BeforeStart { .. } | Self::AfterEnd { .. } => None,
        }
    }
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
    /// Append a pose at a strictly newer tick, normalizing its rotation.
    ///
    /// Duplicate, decreasing and wrapped ticks return [`ReplayError::TickOrder`]
    /// while space remains; a full track returns [`ReplayError::Capacity`] first.
    /// Rejections leave all recorded poses unchanged, including teleport flags.
    /// A teleport must use its own newer tick, never replace an existing key.
    ///
    /// ```
    /// use ferravex_play::replay::{ReplayError, ghost::{GhostTrack, Sample}};
    /// use ferravex_math::{Quat, Vec3};
    ///
    /// let mut track = GhostTrack::new(7, 60, 2)?;
    /// let pose = Sample {
    ///     tick: 10, position: Vec3::ZERO, rotation: Quat::IDENTITY, teleport: false,
    /// };
    /// track.push(pose)?;
    /// assert_eq!(track.push(Sample { teleport: true, ..pose }), Err(ReplayError::TickOrder));
    /// assert_eq!(track.samples(), &[pose]);
    /// # Ok::<(), ReplayError>(())
    /// ```
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
    ///
    /// This compatibility API retains its original synthesized teleport flags.
    /// Use [`Self::sample_checked`] for exact stored keys and explicit states.
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
        Some(interpolate(a, b, tick, alpha))
    }

    /// Sample `tick + alpha` with explicit range and discontinuity states.
    ///
    /// Alpha is a fractional simulation tick in `[0, 1)`, not a fraction of the
    /// gap between recorded keys. Invalid alpha (including NaN, infinity or `1`)
    /// returns [`ReplayError::InvalidData`], even for an empty track. No input is
    /// clamped. A positive alpha at the final tick is [`AfterEnd`](SampleState::AfterEnd),
    /// including at `u64::MAX`; ticks never wrap or convert wholesale to floats.
    ///
    /// Exact queries return stored keys unchanged, including teleport flags and
    /// quaternion signs. Between keys, an upcoming teleport holds both position
    /// and rotation until its exact tick. Interpolation resumes from that key
    /// unless the next key is also marked. Only exact keys retain teleport flags;
    /// a hold is reported as [`Held`](SampleState::Held), not a repeated teleport.
    ///
    /// Duplicate/decreasing keys are rejected by [`Self::push`], so every
    /// interpolation interval has positive length. Queries allocate nothing,
    /// take `O(log(samples.len()))` time and may repeat or seek in either direction.
    /// Interpolation uses relative `f64` tick differences, then `f32` poses;
    /// rounding can produce an endpoint pose for a strictly interior query.
    ///
    /// ```
    /// use ferravex_play::replay::{ReplayError, ghost::{GhostTrack, Sample, SampleState}};
    /// use ferravex_math::{Quat, Vec3};
    ///
    /// let mut track = GhostTrack::new(7, 60, 3)?;
    /// for (tick, x, teleport) in [(10, 0.0, false), (20, 100.0, true), (30, 110.0, false)] {
    ///     track.push(Sample {
    ///         tick, position: Vec3::X * x, rotation: Quat::IDENTITY, teleport,
    ///     })?;
    /// }
    /// assert!(matches!(track.sample_checked(9, 0.5)?, SampleState::BeforeStart { .. }));
    /// assert!(matches!(track.sample_checked(19, 0.5)?, SampleState::Held {
    ///     from_tick: 10, teleport_tick: 20, ..
    /// }));
    /// assert_eq!(track.sample_checked(19, 0.5)?.sample().unwrap().position.x, 0.0);
    /// assert_eq!(track.sample_checked(20, 0.0)?, SampleState::Exact(track.samples()[1]));
    /// let moving = track.sample_checked(20, 0.5)?;
    /// assert!(matches!(moving, SampleState::Interpolated { from_tick: 20, to_tick: 30, .. }));
    /// assert_eq!(moving.sample().unwrap().position.x, 100.5);
    /// assert!(matches!(track.sample_checked(30, 0.5)?, SampleState::AfterEnd { .. }));
    /// assert_eq!(track.sample_checked(20, f32::NAN), Err(ReplayError::InvalidData));
    /// # Ok::<(), ReplayError>(())
    /// ```
    pub fn sample_checked(&self, tick: u64, alpha: f32) -> Result<SampleState, ReplayError> {
        if !alpha.is_finite() || !(0.0..1.0).contains(&alpha) {
            return Err(ReplayError::InvalidData);
        }
        let Some(&first) = self.samples.first() else {
            return Ok(SampleState::Empty);
        };
        if tick < first.tick {
            return Ok(SampleState::BeforeStart { first });
        }
        let last = self.samples[self.samples.len() - 1];
        if tick > last.tick || (tick == last.tick && alpha > 0.) {
            return Ok(SampleState::AfterEnd { last });
        }
        let i = self.samples.partition_point(|s| s.tick <= tick) - 1;
        let a = self.samples[i];
        if tick == a.tick && alpha == 0. {
            return Ok(SampleState::Exact(a));
        }
        // The final key was handled above; an in-range nonexact query has a successor.
        let b = self.samples[i + 1];
        if b.teleport {
            return Ok(SampleState::Held {
                sample: Sample {
                    tick,
                    teleport: false,
                    ..a
                },
                from_tick: a.tick,
                teleport_tick: b.tick,
            });
        }
        Ok(SampleState::Interpolated {
            sample: interpolate(a, b, tick, alpha),
            from_tick: a.tick,
            to_tick: b.tick,
        })
    }
}

fn interpolate(a: Sample, b: Sample, tick: u64, alpha: f32) -> Sample {
    let t = ((tick - a.tick) as f64 + f64::from(alpha)) / (b.tick - a.tick) as f64;
    Sample {
        tick,
        position: a.position.lerp(b.position, t as f32),
        rotation: a.rotation.slerp(b.rotation, t as f32),
        teleport: false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn pose(tick: u64, x: f32, teleport: bool) -> Sample {
        Sample {
            tick,
            position: Vec3::X * x,
            rotation: Quat::IDENTITY,
            teleport,
        }
    }

    #[test]
    fn checked_empty_and_invalid_fractions_are_distinct() {
        let mut empty = GhostTrack::new(4, 64, 0).unwrap();
        let mut populated = GhostTrack::new(4, 64, 1).unwrap();
        populated.push(pose(10, 1., false)).unwrap();
        assert_eq!(empty.push(pose(10, 1., false)), Err(ReplayError::Capacity));
        for track in [&empty, &populated] {
            for tick in [0, 10, u64::MAX] {
                for alpha in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 1., 2.] {
                    assert_eq!(
                        track.sample_checked(tick, alpha),
                        Err(ReplayError::InvalidData)
                    );
                    assert_eq!(track.sample(tick, alpha), None);
                }
            }
        }
        for tick in [0, 10, u64::MAX] {
            for alpha in [0., -0., 0.5] {
                assert_eq!(empty.sample_checked(tick, alpha), Ok(SampleState::Empty));
            }
        }
        assert_eq!(SampleState::Empty.sample(), None);
        assert!(empty.samples().is_empty());
    }

    #[test]
    fn checked_single_key_has_inclusive_exact_and_exclusive_fractional_end() {
        for tick in [0, 10, u64::MAX] {
            let mut track = GhostTrack::new(4, 64, 1).unwrap();
            track.push(pose(tick, 7., true)).unwrap();
            let key = track.samples()[0];
            for alpha in [0., -0.] {
                let state = track.sample_checked(tick, alpha).unwrap();
                assert_eq!(state, SampleState::Exact(key));
                assert_eq!(state.sample(), Some(key));
            }
            for alpha in [
                f32::from_bits(1),
                0.5,
                f32::from_bits(1.0_f32.to_bits() - 1),
            ] {
                let state = track.sample_checked(tick, alpha).unwrap();
                assert_eq!(state, SampleState::AfterEnd { last: key });
                assert_eq!(state.sample(), None);
            }
            if let Some(before) = tick.checked_sub(1) {
                let state = track.sample_checked(before, 0.999).unwrap();
                assert_eq!(state, SampleState::BeforeStart { first: key });
                assert_eq!(state.sample(), None);
            }
            if let Some(after) = tick.checked_add(1) {
                assert_eq!(
                    track.sample_checked(after, 0.),
                    Ok(SampleState::AfterEnd { last: key })
                );
            }
        }
    }

    #[test]
    fn checked_exact_keys_preserve_quaternion_sign_and_teleport_flag() {
        let mut track = GhostTrack::new(4, 64, 3).unwrap();
        for tick in [10, 20, 30] {
            track
                .push(Sample {
                    rotation: if tick == 20 {
                        -Quat::IDENTITY
                    } else {
                        Quat::IDENTITY
                    },
                    ..pose(tick, tick as f32, tick != 30)
                })
                .unwrap();
        }
        for &key in track.samples() {
            assert_eq!(
                track.sample_checked(key.tick, 0.),
                Ok(SampleState::Exact(key))
            );
        }
        // The additive API does not change the legacy interior-key flag behavior.
        assert!(!track.sample(20, 0.).unwrap().teleport);
        assert!(
            track
                .sample_checked(20, 0.)
                .unwrap()
                .sample()
                .unwrap()
                .teleport
        );
        assert!(track.sample(15, 0.).unwrap().teleport);
        assert!(
            !track
                .sample_checked(15, 0.)
                .unwrap()
                .sample()
                .unwrap()
                .teleport
        );
        assert_eq!(
            track.sample_checked(9, 0.5),
            Ok(SampleState::BeforeStart {
                first: track.samples()[0]
            })
        );
        assert_eq!(
            track.sample_checked(30, 0.5),
            Ok(SampleState::AfterEnd {
                last: track.samples()[2]
            })
        );
    }

    #[test]
    fn checked_teleports_hold_both_components_then_resume_continuous_motion() {
        let mut track = GhostTrack::new(4, 64, 4).unwrap();
        let turn = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        for key in [
            pose(10, 0., false),
            Sample {
                rotation: turn,
                ..pose(20, 100., true)
            },
            pose(21, -100., true),
            pose(25, -92., false),
        ] {
            track.push(key).unwrap();
        }
        let original = track.samples().to_vec();
        // Seek in both directions and repeat a query: sampling has no cursor.
        for (tick, alpha, from, to) in [
            (20, 0.5, 1, 2),
            (19, 0.999, 0, 1),
            (10, 0.5, 0, 1),
            (15, 0., 0, 1),
            (20, 0.5, 1, 2),
        ] {
            let expected = Sample {
                tick,
                teleport: false,
                ..original[from]
            };
            let state = track.sample_checked(tick, alpha).unwrap();
            assert_eq!(
                state,
                SampleState::Held {
                    sample: expected,
                    from_tick: original[from].tick,
                    teleport_tick: original[to].tick,
                }
            );
            assert_eq!(state.sample(), Some(expected));
        }
        for index in [1, 2] {
            assert_eq!(
                track.sample_checked(original[index].tick, 0.),
                Ok(SampleState::Exact(original[index]))
            );
        }
        assert_eq!(
            track.sample_checked(21, 0.5),
            Ok(SampleState::Interpolated {
                sample: pose(21, -99., false),
                from_tick: 21,
                to_tick: 25,
            })
        );
        assert_eq!(track.samples(), original);
    }

    #[test]
    fn checked_sparse_interpolation_uses_tick_gap_and_shortest_rotation() {
        let mut track = GhostTrack::new(4, 64, 3).unwrap();
        let start = u64::MAX - 20;
        track.push(pose(start, 0., false)).unwrap();
        track
            .push(Sample {
                rotation: -Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                ..pose(start + 10, 10., false)
            })
            .unwrap();
        track.push(pose(u64::MAX, 50., true)).unwrap();
        let state = track.sample_checked(start + 5, 0.5).unwrap();
        assert!(
            matches!(state, SampleState::Interpolated { from_tick, to_tick, .. }
            if from_tick == start && to_tick == start + 10)
        );
        let sample = state.sample().unwrap();
        assert_eq!(sample.position.x, 5.5);
        assert_eq!(sample.tick, start + 5);
        assert!(!sample.teleport);
        assert!((sample.rotation.length_squared() - 1.).abs() < 1e-6);
        let expected = Quat::from_rotation_y(std::f32::consts::FRAC_PI_2 * 0.55) * Vec3::X;
        assert!((sample.rotation * Vec3::X - expected).length() < 1e-6);
        assert_eq!(track.sample(start + 5, 0.5), Some(sample));
        assert!(matches!(
            track.sample_checked(u64::MAX - 1, 0.999).unwrap(),
            SampleState::Held {
                teleport_tick: u64::MAX,
                ..
            }
        ));
        assert_eq!(
            track.sample_checked(u64::MAX, 0.),
            Ok(SampleState::Exact(track.samples()[2]))
        );
    }

    #[test]
    fn checked_full_tick_span_never_wraps_or_extrapolates() {
        let mut track = GhostTrack::new(4, 64, 2).unwrap();
        track.push(pose(0, -1e6, false)).unwrap();
        track.push(pose(u64::MAX, 1e6, false)).unwrap();
        for tick in [0, 1, u64::MAX / 2, u64::MAX - 1] {
            let state = track.sample_checked(tick, 0.5).unwrap();
            assert!(matches!(
                state,
                SampleState::Interpolated {
                    from_tick: 0,
                    to_tick: u64::MAX,
                    ..
                }
            ));
            let sample = state.sample().unwrap();
            assert!(sample.position.is_finite());
            assert!(sample.position.x.abs() <= 1e6);
            assert!(sample.rotation.is_finite());
        }
        assert_eq!(
            track.sample_checked(u64::MAX, 0.5),
            Ok(SampleState::AfterEnd {
                last: track.samples()[1]
            })
        );
    }

    #[test]
    fn rejected_duplicate_decreasing_and_wrapped_ticks_preserve_history() {
        let mut track = GhostTrack::new(4, 64, 3).unwrap();
        track.push(pose(10, 0., false)).unwrap();
        track.push(pose(20, 10., false)).unwrap();
        let original = track.samples().to_vec();
        let before = track.sample_checked(15, 0.5).unwrap();
        for tick in [20, 19, 10, 0] {
            assert_eq!(
                track.push(pose(tick, 99., true)),
                Err(ReplayError::TickOrder)
            );
            assert_eq!(track.samples(), original);
            assert_eq!(track.sample_checked(15, 0.5), Ok(before));
        }
        track.push(pose(30, 20., false)).unwrap();
        // Full-track error precedence stays compatible, even for a duplicate.
        for tick in [30, 31] {
            assert_eq!(
                track.push(pose(tick, 99., true)),
                Err(ReplayError::Capacity)
            );
        }
        assert_eq!(track.samples().len(), 3);
        let mut wrapping = GhostTrack::new(4, 64, 2).unwrap();
        wrapping.push(pose(u64::MAX, 0., false)).unwrap();
        assert_eq!(
            wrapping.push(pose(0, 99., true)),
            Err(ReplayError::TickOrder)
        );
        assert_eq!(wrapping.samples(), &[pose(u64::MAX, 0., false)]);
    }

    #[test]
    fn rejected_invalid_poses_do_not_create_discontinuities() {
        let mut track = GhostTrack::new(4, 64, 2).unwrap();
        track.push(pose(10, 0., false)).unwrap();
        let valid = pose(20, 10., true);
        for invalid in [
            Sample {
                position: Vec3::NAN,
                ..valid
            },
            Sample {
                position: Vec3::X * 1_000_001.,
                ..valid
            },
            Sample {
                rotation: Quat::NAN,
                ..valid
            },
            Sample {
                rotation: Quat::from_xyzw(0., 0., 0., 0.),
                ..valid
            },
        ] {
            assert_eq!(track.push(invalid), Err(ReplayError::InvalidData));
            assert_eq!(track.samples(), &[pose(10, 0., false)]);
        }
        track
            .push(Sample {
                rotation: Quat::IDENTITY * 2.,
                ..valid
            })
            .unwrap();
        assert_eq!(track.samples()[1], valid);
        assert!(matches!(
            track.sample_checked(15, 0.).unwrap(),
            SampleState::Held {
                teleport_tick: 20,
                ..
            }
        ));
    }

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
