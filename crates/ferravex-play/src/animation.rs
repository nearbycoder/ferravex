//! Tracks, bounded playback/events, pose blending and reusable animation controllers.
mod contact;
pub use contact::*;
mod graph;
mod playback;
mod pose;
pub mod retarget;
pub use graph::*;
pub use playback::*;
pub use pose::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationError {
    Empty,
    InvalidKey,
    UnorderedKeys,
    InvalidTime,
    InvalidClip,
    InvalidPose,
    InvalidWeight,
    JointCount,
    Capacity,
}
/// An authoring error with the zero-based position of the offending key/frame.
///
/// `index` is `None` only for empty input. For unordered keys it identifies the
/// second key of the first non-increasing adjacent pair. Invalid keys take
/// precedence over ordering errors, just as in [`Track::new`].
///
/// ```
/// use ferravex_play::animation::{AnimationError, Keyframe, ScalarTrack};
/// let keys = [Keyframe { time: 1.0, value: 2.0 }; 2];
/// let diagnostic = ScalarTrack::validate_keys(&keys).unwrap_err();
/// assert_eq!(diagnostic.error, AnimationError::UnorderedKeys);
/// assert_eq!(diagnostic.index, Some(1));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnimationDiagnostic {
    /// The error category returned by the existing constructor.
    pub error: AnimationError,
    /// Offending key/frame index, or `None` for empty input.
    pub index: Option<usize>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopMode {
    Clamp,
    Repeat,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interpolation {
    Step,
    Linear,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keyframe<T> {
    pub time: f32,
    pub value: T,
}
mod sealed {
    pub trait Sample {}
}
pub trait Sample: sealed::Sample + Copy {
    fn valid(self) -> bool;
    fn blend(a: Self, b: Self, amount: f32) -> Self;
}
impl sealed::Sample for f32 {}
impl Sample for f32 {
    fn valid(self) -> bool {
        self.is_finite()
    }
    fn blend(a: Self, b: Self, amount: f32) -> Self {
        lerp(a, b, amount)
    }
}
impl sealed::Sample for [f32; 3] {}
impl Sample for [f32; 3] {
    fn valid(self) -> bool {
        self.into_iter().all(f32::is_finite)
    }
    fn blend(a: Self, b: Self, amount: f32) -> Self {
        [
            lerp(a[0], b[0], amount),
            lerp(a[1], b[1], amount),
            lerp(a[2], b[2], amount),
        ]
    }
}
impl sealed::Sample for [f32; 4] {}
impl Sample for [f32; 4] {
    fn valid(self) -> bool {
        self.into_iter().all(f32::is_finite)
            && self.into_iter().map(|v| f64::from(v).powi(2)).sum::<f64>() > 1e-20
    }
    fn blend(a: Self, b: Self, amount: f32) -> Self {
        slerp(a, b, amount)
    }
}

#[derive(Clone, Debug)]
pub struct Track<T: Sample> {
    keys: Vec<Keyframe<T>>,
    interpolation: Interpolation,
}
impl<T: Sample> Track<T> {
    pub fn new(
        keys: Vec<Keyframe<T>>,
        interpolation: Interpolation,
    ) -> Result<Self, AnimationError> {
        Self::validate_keys(&keys).map_err(|diagnostic| diagnostic.error)?;
        Ok(Self {
            keys,
            interpolation,
        })
    }
    /// Check borrowed authoring data without allocating, sorting or consuming it.
    ///
    /// Keys must have finite, nonnegative times and valid values. Scalar/vector
    /// values must be finite; quaternion squared length (computed in `f64`)
    /// must also exceed `1e-20`, as required by [`Sample::valid`]. Times must be strictly increasing:
    /// duplicates and descending pairs return `UnorderedKeys` at the latter key.
    /// The first invalid key is reported before checking ordering, preserving
    /// [`Self::new`]'s error precedence. A single key at time zero is valid.
    /// Work is O(keys.len()) with constant scratch space.
    ///
    /// ```
    /// use ferravex_play::animation::{Interpolation, Keyframe, ScalarTrack};
    /// let keys = [Keyframe { time: 0.0, value: 3.0 }];
    /// ScalarTrack::validate_keys(&keys).unwrap();
    /// let track = ScalarTrack::new(keys.to_vec(), Interpolation::Step).unwrap();
    /// assert_eq!(track.keys(), &keys);
    /// assert_eq!(track.interpolation(), Interpolation::Step);
    /// ```
    pub fn validate_keys(keys: &[Keyframe<T>]) -> Result<(), AnimationDiagnostic> {
        if keys.is_empty() {
            return Err(AnimationDiagnostic {
                error: AnimationError::Empty,
                index: None,
            });
        }
        if let Some(index) = keys
            .iter()
            .position(|key| !key.time.is_finite() || key.time < 0.0 || !key.value.valid())
        {
            return Err(AnimationDiagnostic {
                error: AnimationError::InvalidKey,
                index: Some(index),
            });
        }
        if let Some(index) = keys
            .windows(2)
            .position(|pair| pair[0].time >= pair[1].time)
        {
            return Err(AnimationDiagnostic {
                error: AnimationError::UnorderedKeys,
                index: Some(index + 1),
            });
        }
        Ok(())
    }
    /// Borrow the original keys in authoring order, without normalizing values.
    /// See [`Self::validate_keys`] for an inspection example.
    pub fn keys(&self) -> &[Keyframe<T>] {
        &self.keys
    }
    /// Inspect the interpolation selected at construction.
    /// See [`Self::validate_keys`] for an example.
    pub fn interpolation(&self) -> Interpolation {
        self.interpolation
    }
    pub fn duration(&self) -> f32 {
        self.keys.last().expect("nonempty track").time
    }
    pub fn sample(&self, time: f32, mode: LoopMode) -> Result<T, AnimationError> {
        let time = playback_time(time, self.duration(), mode)?;
        let upper = self.keys.partition_point(|key| key.time <= time);
        if upper == 0 {
            return Ok(T::blend(self.keys[0].value, self.keys[0].value, 0.0));
        }
        let a = &self.keys[upper - 1];
        if upper == self.keys.len() || self.interpolation == Interpolation::Step {
            return Ok(T::blend(a.value, a.value, 0.0));
        }
        let b = &self.keys[upper];
        Ok(T::blend(
            a.value,
            b.value,
            (time - a.time) / (b.time - a.time),
        ))
    }
    /// Sample arbitrary times into a reusable output prefix without allocating.
    ///
    /// Returns the number written (`times.len()`); any remaining output elements
    /// are untouched. Empty input succeeds without writes. A short output returns
    /// `Capacity`, before checking for nonfinite times (`InvalidTime`). Either
    /// error leaves the entire output unchanged. Negative and unsorted times are
    /// supported and each result is identical to [`Self::sample`].
    /// Work is O(times.len() * log(keys.len())) with constant scratch space.
    ///
    /// ```
    /// use ferravex_play::animation::{Interpolation, Keyframe, LoopMode, ScalarTrack};
    /// let track = ScalarTrack::new(vec![
    ///     Keyframe { time: 0.0, value: 0.0 },
    ///     Keyframe { time: 1.0, value: 4.0 },
    /// ], Interpolation::Linear).unwrap();
    /// let mut out = [-1.0; 4];
    /// let count = track.sample_into(&[0.5, 1.0, -0.25], LoopMode::Repeat, &mut out).unwrap();
    /// assert_eq!(&out[..count], &[2.0, 0.0, 3.0]);
    /// assert_eq!(out[3], -1.0);
    /// assert!(track.sample_into(&[0.0, f32::NAN], LoopMode::Clamp, &mut out).is_err());
    /// assert_eq!(out, [2.0, 0.0, 3.0, -1.0]);
    /// ```
    pub fn sample_into(
        &self,
        times: &[f32],
        mode: LoopMode,
        out: &mut [T],
    ) -> Result<usize, AnimationError> {
        validate_sample_output(times, out.len())?;
        for (&time, value) in times.iter().zip(out) {
            *value = self.sample(time, mode)?;
        }
        Ok(times.len())
    }
}
pub type ScalarTrack = Track<f32>;
pub type Vec3Track = Track<[f32; 3]>;
/// Quaternion order is [x, y, z, w]. Samples are normalized, including exact keys.
pub type QuatTrack = Track<[f32; 4]>;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteFrame {
    pub index: u32,
    pub duration: f32,
}
#[derive(Clone, Debug)]
pub struct SpriteClip {
    frames: Vec<SpriteFrame>,
    ends: Vec<f32>,
    duration: f32,
}
impl SpriteClip {
    pub fn new(frames: Vec<SpriteFrame>) -> Result<Self, AnimationError> {
        let duration = Self::validate_frames(&frames).map_err(|diagnostic| diagnostic.error)?;
        let mut end = 0.0;
        let ends = frames
            .iter()
            .map(|frame| {
                end += frame.duration;
                end
            })
            .collect();
        Ok(Self {
            frames,
            ends,
            duration,
        })
    }
    /// Check borrowed frames and return their accumulated duration, without
    /// allocating or consuming the authoring data.
    ///
    /// Reports the first frame with a nonfinite/nonpositive duration or whose
    /// addition overflows or fails to advance the accumulated `f32` end time.
    /// All such cases have category `InvalidKey`; empty input has category
    /// `Empty` and no index. Frame indices may repeat and need not be ordered.
    /// Work is O(frames.len()) with constant scratch space.
    ///
    /// ```
    /// use ferravex_play::animation::{SpriteClip, SpriteFrame};
    /// let frames = [
    ///     SpriteFrame { index: 7, duration: 0.25 },
    ///     SpriteFrame { index: 2, duration: 0.75 },
    /// ];
    /// assert_eq!(SpriteClip::validate_frames(&frames), Ok(1.0));
    /// let clip = SpriteClip::new(frames.to_vec()).unwrap();
    /// assert_eq!(clip.frames(), &frames);
    /// ```
    pub fn validate_frames(frames: &[SpriteFrame]) -> Result<f32, AnimationDiagnostic> {
        if frames.is_empty() {
            return Err(AnimationDiagnostic {
                error: AnimationError::Empty,
                index: None,
            });
        }
        let mut duration = 0.0_f32;
        for (index, frame) in frames.iter().enumerate() {
            if !frame.duration.is_finite() || frame.duration <= 0.0 {
                return Err(AnimationDiagnostic {
                    error: AnimationError::InvalidKey,
                    index: Some(index),
                });
            }
            let end = duration + frame.duration;
            if !end.is_finite() || end <= duration {
                return Err(AnimationDiagnostic {
                    error: AnimationError::InvalidKey,
                    index: Some(index),
                });
            }
            duration = end;
        }
        Ok(duration)
    }
    /// Borrow the original frame indices and durations in playback order.
    /// See [`Self::validate_frames`] for an inspection example.
    pub fn frames(&self) -> &[SpriteFrame] {
        &self.frames
    }
    pub fn duration(&self) -> f32 {
        self.duration
    }
    pub fn frame(&self, time: f32, mode: LoopMode) -> Result<u32, AnimationError> {
        let time = playback_time(time, self.duration, mode)?;
        let index = self
            .ends
            .partition_point(|end| *end <= time)
            .min(self.frames.len() - 1);
        Ok(self.frames[index].index)
    }
    /// Sample sprite indices into a reusable output prefix without allocating.
    ///
    /// Returns `times.len()`, leaving the output tail untouched. Empty input
    /// succeeds. A short output returns `Capacity` before nonfinite times are
    /// checked (`InvalidTime`); either error leaves output unchanged. Times may
    /// be negative or unsorted. Each result is identical to [`Self::frame`],
    /// including exact frame/loop boundaries. Work is
    /// O(times.len() * log(frames.len())) with constant scratch space.
    ///
    /// ```
    /// use ferravex_play::animation::{LoopMode, SpriteClip, SpriteFrame};
    /// let clip = SpriteClip::new(vec![
    ///     SpriteFrame { index: 4, duration: 0.25 },
    ///     SpriteFrame { index: 7, duration: 0.75 },
    /// ]).unwrap();
    /// let mut out = [99; 4];
    /// assert_eq!(clip.sample_into(&[0.0, 0.25, 1.0], LoopMode::Repeat, &mut out), Ok(3));
    /// assert_eq!(out, [4, 7, 4, 99]);
    /// ```
    pub fn sample_into(
        &self,
        times: &[f32],
        mode: LoopMode,
        out: &mut [u32],
    ) -> Result<usize, AnimationError> {
        validate_sample_output(times, out.len())?;
        for (&time, value) in times.iter().zip(out) {
            *value = self.frame(time, mode)?;
        }
        Ok(times.len())
    }
}
fn validate_sample_output(times: &[f32], capacity: usize) -> Result<(), AnimationError> {
    if times.len() > capacity {
        return Err(AnimationError::Capacity);
    }
    if times.iter().any(|time| !time.is_finite()) {
        return Err(AnimationError::InvalidTime);
    }
    Ok(())
}
fn playback_time(time: f32, duration: f32, mode: LoopMode) -> Result<f32, AnimationError> {
    if !time.is_finite() {
        return Err(AnimationError::InvalidTime);
    }
    Ok(if mode == LoopMode::Repeat && duration > 0.0 {
        time.rem_euclid(duration)
    } else {
        time.clamp(0.0, duration)
    })
}
fn lerp(a: f32, b: f32, amount: f32) -> f32 {
    (f64::from(a) * (1.0 - f64::from(amount)) + f64::from(b) * f64::from(amount)) as f32
}
fn normalize(q: [f32; 4]) -> [f32; 4] {
    let length = q
        .into_iter()
        .map(|v| f64::from(v).powi(2))
        .sum::<f64>()
        .sqrt();
    q.map(|v| (f64::from(v) / length) as f32)
}
fn slerp(a: [f32; 4], b: [f32; 4], amount: f32) -> [f32; 4] {
    let a = normalize(a);
    let mut b = normalize(b);
    let mut dot = a.iter().zip(b).map(|(a, b)| a * b).sum::<f32>();
    if dot < 0.0 {
        b = b.map(|v| -v);
        dot = -dot;
    }
    dot = dot.clamp(-1.0, 1.0);
    let (wa, wb) = if dot > 0.9995 {
        (1.0 - amount, amount)
    } else {
        let angle = dot.acos();
        (
            ((1.0 - amount) * angle).sin() / angle.sin(),
            (amount * angle).sin() / angle.sin(),
        )
    };
    normalize(std::array::from_fn(|i| a[i] * wa + b[i] * wb))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vector_keys_clamp_interpolate_and_repeat() {
        let track = Vec3Track::new(
            vec![
                Keyframe {
                    time: 0.0,
                    value: [0.0; 3],
                },
                Keyframe {
                    time: 2.0,
                    value: [2.0, 4.0, 6.0],
                },
            ],
            Interpolation::Linear,
        )
        .unwrap();
        assert_eq!(track.sample(1.0, LoopMode::Clamp).unwrap(), [1.0, 2.0, 3.0]);
        assert_eq!(track.sample(-1.0, LoopMode::Clamp).unwrap(), [0.0; 3]);
        assert_eq!(
            track.sample(10.0, LoopMode::Clamp).unwrap(),
            [2.0, 4.0, 6.0]
        );
        assert_eq!(
            track.sample(3.0, LoopMode::Repeat).unwrap(),
            [1.0, 2.0, 3.0]
        );
        assert_eq!(
            track.sample(f32::NAN, LoopMode::Clamp),
            Err(AnimationError::InvalidTime)
        );
    }
    #[test]
    fn quaternion_uses_shortest_arc_and_normalizes_keys() {
        let track = QuatTrack::new(
            vec![
                Keyframe {
                    time: 0.0,
                    value: [0.0, 0.0, 0.0, 2.0],
                },
                Keyframe {
                    time: 1.0,
                    value: [0.0, 0.0, 0.0, -3.0],
                },
            ],
            Interpolation::Linear,
        )
        .unwrap();
        for time in [0.0, 0.5, 1.0] {
            assert!((track.sample(time, LoopMode::Clamp).unwrap()[3].abs() - 1.0).abs() < 1e-6);
        }
        let half_turn = QuatTrack::new(
            vec![
                Keyframe {
                    time: 0.0,
                    value: [0.0, 0.0, 0.0, 1.0],
                },
                Keyframe {
                    time: 1.0,
                    value: [0.0, 0.0, 1.0, 0.0],
                },
            ],
            Interpolation::Linear,
        )
        .unwrap();
        let q = half_turn.sample(0.5, LoopMode::Clamp).unwrap();
        assert!((q[2] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
        assert!((q[3] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-5);
    }
    #[test]
    fn sprite_frames_use_individual_durations_and_exact_boundaries() {
        let clip = SpriteClip::new(vec![
            SpriteFrame {
                index: 4,
                duration: 0.25,
            },
            SpriteFrame {
                index: 7,
                duration: 0.75,
            },
        ])
        .unwrap();
        assert_eq!(clip.frame(0.24, LoopMode::Clamp).unwrap(), 4);
        assert_eq!(clip.frame(0.25, LoopMode::Clamp).unwrap(), 7);
        assert_eq!(clip.frame(1.0, LoopMode::Clamp).unwrap(), 7);
        assert_eq!(clip.frame(1.0, LoopMode::Repeat).unwrap(), 4);
    }
    #[test]
    fn malformed_keys_and_zero_quaternions_are_rejected() {
        assert!(matches!(
            QuatTrack::new(
                vec![Keyframe {
                    time: 0.0,
                    value: [0.0; 4]
                }],
                Interpolation::Linear
            ),
            Err(AnimationError::InvalidKey)
        ));
        assert!(matches!(
            ScalarTrack::new(
                vec![
                    Keyframe {
                        time: 1.0,
                        value: 1.0
                    },
                    Keyframe {
                        time: 1.0,
                        value: 2.0
                    }
                ],
                Interpolation::Linear
            ),
            Err(AnimationError::UnorderedKeys)
        ));
        assert!(matches!(
            SpriteClip::new(vec![SpriteFrame {
                index: 0,
                duration: 0.0
            }]),
            Err(AnimationError::InvalidKey)
        ));
    }

    #[test]
    fn authoring_diagnostics_locate_empty_duplicate_and_descending_keys() {
        assert_eq!(
            ScalarTrack::validate_keys(&[]),
            Err(AnimationDiagnostic {
                error: AnimationError::Empty,
                index: None,
            })
        );
        assert!(matches!(
            ScalarTrack::new(vec![], Interpolation::Step),
            Err(AnimationError::Empty)
        ));
        for times in [[0.0, 1.0, 1.0, 0.5], [0.0, 2.0, 1.0, 0.5]] {
            let keys = times.map(|time| Keyframe { time, value: 4.0 });
            let original = keys;
            assert_eq!(
                ScalarTrack::validate_keys(&keys),
                Err(AnimationDiagnostic {
                    error: AnimationError::UnorderedKeys,
                    index: Some(2),
                })
            );
            assert_eq!(keys, original);
            assert!(matches!(
                ScalarTrack::new(keys.to_vec(), Interpolation::Linear),
                Err(AnimationError::UnorderedKeys)
            ));
        }
        let zeros = [0.0, -0.0].map(|time| Keyframe { time, value: 4.0 });
        assert_eq!(
            ScalarTrack::validate_keys(&zeros).unwrap_err().index,
            Some(1)
        );
        assert!(ScalarTrack::validate_keys(&zeros[..1]).is_ok());
    }

    #[test]
    fn invalid_key_diagnostics_preserve_constructor_error_precedence() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.5] {
            let keys = [1.0, 0.0, invalid].map(|time| Keyframe { time, value: 4.0 });
            assert_eq!(
                ScalarTrack::validate_keys(&keys),
                Err(AnimationDiagnostic {
                    error: AnimationError::InvalidKey,
                    index: Some(2),
                })
            );
            assert!(matches!(
                ScalarTrack::new(keys.to_vec(), Interpolation::Linear),
                Err(AnimationError::InvalidKey)
            ));
        }
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let keys = [
                Keyframe {
                    time: 0.0,
                    value: 2.0,
                },
                Keyframe {
                    time: 1.0,
                    value: invalid,
                },
                Keyframe {
                    time: 2.0,
                    value: invalid,
                },
            ];
            assert_eq!(
                ScalarTrack::validate_keys(&keys),
                Err(AnimationDiagnostic {
                    error: AnimationError::InvalidKey,
                    index: Some(1),
                })
            );
            let vectors = keys.map(|key| Keyframe {
                time: key.time,
                value: [key.value; 3],
            });
            assert_eq!(
                Vec3Track::validate_keys(&vectors),
                ScalarTrack::validate_keys(&keys)
            );
        }
        for value in [[0.0; 4], [1e-12; 4], [f32::NAN; 4], [f32::INFINITY; 4]] {
            assert_eq!(
                QuatTrack::validate_keys(&[Keyframe { time: 0.0, value }]),
                Err(AnimationDiagnostic {
                    error: AnimationError::InvalidKey,
                    index: Some(0),
                })
            );
        }
    }

    #[test]
    fn key_inspection_preserves_original_quaternions_and_interpolation() {
        let keys = [Keyframe {
            time: 0.0,
            value: [0.0, 0.0, 0.0, 2.0],
        }];
        for interpolation in [Interpolation::Step, Interpolation::Linear] {
            let track = QuatTrack::new(keys.to_vec(), interpolation).unwrap();
            assert_eq!(track.keys(), keys);
            assert_eq!(track.interpolation(), interpolation);
            assert_eq!(track.duration(), 0.0);
            assert_eq!(
                track.sample(0.0, LoopMode::Repeat).unwrap(),
                [0.0, 0.0, 0.0, 1.0]
            );
        }
    }

    #[test]
    fn sprite_authoring_diagnostics_locate_invalid_and_unrepresentable_ends() {
        assert_eq!(
            SpriteClip::validate_frames(&[]),
            Err(AnimationDiagnostic {
                error: AnimationError::Empty,
                index: None
            })
        );
        assert!(matches!(
            SpriteClip::new(vec![]),
            Err(AnimationError::Empty)
        ));
        for durations in [
            [0.25, 0.0],
            [0.25, -1.0],
            [0.25, f32::NAN],
            [0.25, f32::INFINITY],
            [0.25, f32::NEG_INFINITY],
            [f32::MAX, f32::MAX],
            [1.0, f32::MIN_POSITIVE],
        ] {
            let frames = durations.map(|duration| SpriteFrame { index: 3, duration });
            assert_eq!(
                SpriteClip::validate_frames(&frames),
                Err(AnimationDiagnostic {
                    error: AnimationError::InvalidKey,
                    index: Some(1),
                })
            );
            assert!(matches!(
                SpriteClip::new(frames.to_vec()),
                Err(AnimationError::InvalidKey)
            ));
        }
        let frames = [
            SpriteFrame {
                index: 7,
                duration: 0.25,
            },
            SpriteFrame {
                index: 2,
                duration: 0.5,
            },
            SpriteFrame {
                index: 2,
                duration: 0.25,
            },
        ];
        let clip = SpriteClip::new(frames.to_vec()).unwrap();
        assert_eq!(SpriteClip::validate_frames(&frames), Ok(clip.duration()));
        assert_eq!(clip.frames(), frames);
        let subnormal = [SpriteFrame {
            index: u32::MAX,
            duration: f32::from_bits(1),
        }];
        assert_eq!(
            SpriteClip::validate_frames(&subnormal),
            Ok(f32::from_bits(1))
        );
        assert!(SpriteClip::new(subnormal.to_vec()).is_ok());
    }

    fn check_batch_matches_samples<T: Sample + PartialEq + std::fmt::Debug>(
        keys: Vec<Keyframe<T>>,
    ) {
        let sentinel = keys[0].value;
        for interpolation in [Interpolation::Step, Interpolation::Linear] {
            let track = Track::new(keys.clone(), interpolation).unwrap();
            let times = [f32::MAX, 0.25, -1.0, 0.0, 1.0, 0.5, 0.75, -f32::MAX, 2.0];
            let mut output = [sentinel; 11];
            for mode in [LoopMode::Clamp, LoopMode::Repeat] {
                assert_eq!(
                    track.sample_into(&times, mode, &mut output),
                    Ok(times.len())
                );
                for (&time, &value) in times.iter().zip(&output) {
                    assert_eq!(value, track.sample(time, mode).unwrap());
                    assert!(value.valid());
                }
                assert_eq!(&output[times.len()..], &[sentinel; 2]);
                let previous = output;
                assert_eq!(track.sample_into(&[], mode, &mut output), Ok(0));
                assert_eq!(output, previous);
                assert_eq!(track.sample_into(&[], mode, &mut []), Ok(0));
                assert_eq!(
                    track.sample_into(&times, mode, &mut output[..1]),
                    Err(AnimationError::Capacity)
                );
                assert_eq!(output, previous);
                assert_eq!(
                    track.sample_into(&[f32::NAN], mode, &mut []),
                    Err(AnimationError::Capacity)
                );
                for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                    for invalid_index in [0, 1, 2] {
                        let mut times = [0.0, 0.5, 1.0];
                        times[invalid_index] = invalid;
                        assert_eq!(
                            track.sample_into(&times, mode, &mut output),
                            Err(AnimationError::InvalidTime)
                        );
                        assert_eq!(output, previous);
                    }
                }
                assert_eq!(track.sample_into(&[0.5], mode, &mut output), Ok(1));
                assert_eq!(output[0], track.sample(0.5, mode).unwrap());
                assert_eq!(output[1..], previous[1..]);
            }
        }
    }

    #[test]
    fn batch_tracks_preserve_sampling_and_reuse_output_for_all_value_types() {
        check_batch_matches_samples(vec![
            Keyframe {
                time: 0.25,
                value: -f32::MAX,
            },
            Keyframe {
                time: 0.5,
                value: 0.0,
            },
            Keyframe {
                time: 1.0,
                value: f32::MAX,
            },
        ]);
        check_batch_matches_samples(vec![
            Keyframe {
                time: 0.0,
                value: [0.0; 3],
            },
            Keyframe {
                time: 1.0,
                value: [2.0, 4.0, 6.0],
            },
        ]);
        check_batch_matches_samples(vec![
            Keyframe {
                time: 0.0,
                value: [0.0, 0.0, 0.0, 2.0],
            },
            Keyframe {
                time: 1.0,
                value: [0.0, 0.0, -3.0, 0.0],
            },
        ]);
        check_batch_matches_samples(vec![Keyframe {
            time: 0.0,
            value: 7.0,
        }]);
        check_batch_matches_samples(vec![Keyframe {
            time: 0.5,
            value: [0.0, 0.0, 0.0, 2.0],
        }]);
    }

    #[test]
    fn batch_sprite_frames_preserve_boundaries_and_output_on_errors() {
        let clip = SpriteClip::new(vec![
            SpriteFrame {
                index: 4,
                duration: 0.25,
            },
            SpriteFrame {
                index: 7,
                duration: 0.75,
            },
        ])
        .unwrap();
        let times = [1.0, 0.0, 0.25, -0.25, f32::MAX, -f32::MAX];
        let mut output = [99; 8];
        for mode in [LoopMode::Repeat, LoopMode::Clamp] {
            assert_eq!(clip.sample_into(&times, mode, &mut output), Ok(times.len()));
            for (&time, &frame) in times.iter().zip(&output) {
                assert_eq!(frame, clip.frame(time, mode).unwrap());
            }
            assert_eq!(
                &output[..3],
                if mode == LoopMode::Repeat {
                    &[4, 4, 7]
                } else {
                    &[7, 4, 7]
                }
            );
            assert_eq!(&output[times.len()..], &[99; 2]);
            let previous = output;
            assert_eq!(clip.sample_into(&[], mode, &mut output), Ok(0));
            assert_eq!(output, previous);
            assert_eq!(clip.sample_into(&[], mode, &mut []), Ok(0));
            assert_eq!(
                clip.sample_into(&times, mode, &mut output[..1]),
                Err(AnimationError::Capacity)
            );
            assert_eq!(output, previous);
            assert_eq!(
                clip.sample_into(&[f32::NAN], mode, &mut []),
                Err(AnimationError::Capacity)
            );
            for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                for invalid_index in [0, 1, 2] {
                    let mut times = [0.0, 0.25, 1.0];
                    times[invalid_index] = invalid;
                    assert_eq!(
                        clip.sample_into(&times, mode, &mut output),
                        Err(AnimationError::InvalidTime)
                    );
                    assert_eq!(output, previous);
                }
            }
            assert_eq!(clip.sample_into(&[0.25], mode, &mut output), Ok(1));
            assert_eq!(output[0], 7);
            assert_eq!(output[1..], previous[1..]);
        }
    }
}
