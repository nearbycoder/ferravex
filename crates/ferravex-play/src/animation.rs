//! Scalar/vector tracks, shortest-arc quaternion interpolation, and timed sprite clips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationError {
    Empty,
    InvalidKey,
    UnorderedKeys,
    InvalidTime,
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
        if keys.is_empty() {
            return Err(AnimationError::Empty);
        }
        if keys
            .iter()
            .any(|key| !key.time.is_finite() || key.time < 0.0 || !key.value.valid())
        {
            return Err(AnimationError::InvalidKey);
        }
        if keys.windows(2).any(|pair| pair[0].time >= pair[1].time) {
            return Err(AnimationError::UnorderedKeys);
        }
        Ok(Self {
            keys,
            interpolation,
        })
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
        if frames.is_empty() {
            return Err(AnimationError::Empty);
        }
        let mut duration = 0.0;
        let mut ends = Vec::with_capacity(frames.len());
        for frame in &frames {
            if !frame.duration.is_finite() || frame.duration <= 0.0 {
                return Err(AnimationError::InvalidKey);
            }
            duration += frame.duration;
            if !duration.is_finite() || ends.last().is_some_and(|previous| *previous >= duration) {
                return Err(AnimationError::InvalidKey);
            }
            ends.push(duration);
        }
        Ok(Self {
            frames,
            ends,
            duration,
        })
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
}
