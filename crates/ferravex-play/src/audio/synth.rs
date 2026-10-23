//! Original compact procedural source bank. Generated once, never per frame.
//! [`Sound::with_release`] bakes a checked release into an existing source clip.
use super::{Clip, Sound};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Surface {
    Stone,
    Metal,
    Wood,
    Sand,
    Grass,
    Water,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Footstep(Surface),
    Impact(Surface),
    Fire,
    Reload,
    Jump,
    Pickup,
    Success,
    Failure,
    Click,
    Engine,
}
impl Cue {
    pub fn index(self) -> usize {
        match self {
            Self::Footstep(s) => s as usize,
            Self::Impact(s) => 6 + s as usize,
            Self::Fire => 12,
            Self::Reload => 13,
            Self::Jump => 14,
            Self::Pickup => 15,
            Self::Success => 16,
            Self::Failure => 17,
            Self::Click => 18,
            Self::Engine => 19,
        }
    }
}
pub struct SoundBank {
    sounds: Box<[Sound]>,
}
impl SoundBank {
    pub fn procedural(sample_rate: u32) -> Option<Self> {
        if !(8000..=96000).contains(&sample_rate) {
            return None;
        }
        let mut sounds = Vec::with_capacity(20);
        for impact in [false, true] {
            for surface in [
                Surface::Stone,
                Surface::Metal,
                Surface::Wood,
                Surface::Sand,
                Surface::Grass,
                Surface::Water,
            ] {
                let (body, noise, tone, seconds) = match surface {
                    Surface::Stone => (120., 0.7, 0.25, 0.09),
                    Surface::Metal => (670., 0.45, 0.48, 0.15),
                    Surface::Wood => (210., 0.55, 0.35, 0.1),
                    Surface::Sand => (95., 0.9, 0.09, 0.11),
                    Surface::Grass => (155., 0.85, 0.12, 0.085),
                    Surface::Water => (310., 0.8, 0.14, 0.16),
                };
                sounds.push(transient(
                    sample_rate,
                    body,
                    noise,
                    tone,
                    seconds * if impact { 1.4 } else { 1. },
                    (surface as u32 + 1) * 71237 + u32::from(impact),
                )?);
            }
        }
        sounds.push(transient(sample_rate, 95., 1., 0.3, 0.14, 0x91f23)?); // burst and body, not a sine beep
        sounds.push(reload(sample_rate)?);
        sounds.push(chirp(sample_rate, 210., 560., 0.16)?);
        sounds.push(chirp(sample_rate, 660., 1120., 0.12)?);
        sounds.push(chirp(sample_rate, 440., 880., 0.35)?);
        sounds.push(chirp(sample_rate, 170., 70., 0.32)?);
        sounds.push(transient(sample_rate, 900., 0.2, 0.35, 0.025, 0xa512)?);
        let frames = (sample_rate as f64 * 0.1).round() as usize;
        let samples = (0..frames)
            .map(|i| {
                let phase = i as f64 / frames as f64 * std::f64::consts::TAU * 8.;
                let value = (phase.sin() * 0.22
                    + (phase * 2.).sin() * 0.08
                    + (phase * 4.).sin() * 0.025) as f32;
                [value; 2]
            })
            .collect::<Vec<_>>();
        sounds.push(Sound::new(Clip {
            sample_rate,
            samples: samples.into(),
        })?);
        Some(Self {
            sounds: sounds.into_boxed_slice(),
        })
    }
    pub fn get(&self, cue: Cue) -> &Sound {
        &self.sounds[cue.index()]
    }
    pub fn decoded_bytes(&self) -> usize {
        self.sounds.iter().map(|s| s.clip().samples.len() * 8).sum()
    }
}
impl Sound {
    /// Returns a shortened copy with a quadratic release ending in exact silence.
    ///
    /// Frames before `start_frame` are copied unchanged. The release spans
    /// `release_frames` stereo frames: its first frame retains full source gain,
    /// and its last frame is exactly `[0.0; 2]`. Between them, gain follows the
    /// quadratic decay used by the bank's chirps, without restarting their phase
    /// or noise sequence. The returned clip contains `start_frame + release_frames`
    /// frames at the original sample rate. The source sound remains unchanged.
    ///
    /// Returns `None` if the release has fewer than two frames, its end overflows,
    /// or it extends past the source. Requiring both endpoints avoids a forced
    /// one-frame gain jump; very short releases can still be audible.
    ///
    /// This is a preparation/export operation: it allocates a new clip and takes
    /// linear work in its length, bounded by the already validated source length.
    /// Call it outside audio render callbacks and reuse the resulting [`Sound`].
    ///
    /// ```
    /// use ferravex_play::audio::{Cue, SoundBank};
    ///
    /// let bank = SoundBank::procedural(8_000).unwrap();
    /// let source = bank.get(Cue::Success);
    /// let short = source.with_release(400, 800).unwrap();
    /// assert_eq!(short.clip().sample_rate, 8_000);
    /// assert_eq!(short.clip().samples.len(), 1_200);
    /// assert_eq!(&short.clip().samples[..400], &source.clip().samples[..400]);
    /// assert_eq!(short.clip().samples.last(), Some(&[0.0; 2]));
    /// assert!(source.with_release(400, 1).is_none());
    /// ```
    pub fn with_release(&self, start_frame: usize, release_frames: usize) -> Option<Self> {
        if release_frames < 2 {
            return None;
        }
        let end = start_frame.checked_add(release_frames)?;
        let source = self.clip();
        let mut samples = source.samples.get(..end)?.to_vec();
        for (i, frame) in samples[start_frame..].iter_mut().enumerate() {
            if i == release_frames - 1 {
                // Assign rather than multiply to also canonicalize negative zero.
                *frame = [0.; 2];
            } else {
                let gain = quadratic_decay(i as f64 / (release_frames - 1) as f64) as f32;
                for sample in frame {
                    *sample *= gain;
                }
            }
        }
        Sound::new(Clip {
            sample_rate: source.sample_rate,
            samples: samples.into(),
        })
    }
}

fn quadratic_decay(t: f64) -> f64 {
    (1. - t) * (1. - t)
}

fn transient(
    rate: u32,
    hz: f32,
    noise: f32,
    tone: f32,
    seconds: f32,
    mut seed: u32,
) -> Option<Sound> {
    let frames = (rate as f32 * seconds).round() as usize;
    let mut low = 0.;
    let cutoff = (hz * 8.).min(rate as f32 * 0.4);
    let coefficient = 1. - (-std::f32::consts::TAU * cutoff / rate as f32).exp();
    let samples = (0..frames)
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let random = seed as f64 / u32::MAX as f64 * 2. - 1.;
            low += (random as f32 - low) * coefficient;
            let t = i as f32 / frames.max(1) as f32;
            let envelope = (1. - t) * (1. - t) * (1. - t) * (i as f32 / 16.).min(1.);
            let phase = i as f64 * f64::from(hz) * std::f64::consts::TAU / f64::from(rate);
            let v = (low * noise + phase.sin() as f32 * tone) * envelope * 0.48;
            [v; 2]
        })
        .collect::<Vec<_>>();
    Sound::new(Clip {
        sample_rate: rate,
        samples: samples.into(),
    })
}
fn chirp(rate: u32, from: f32, to: f32, seconds: f32) -> Option<Sound> {
    let frames = (rate as f32 * seconds).round() as usize;
    let samples = (0..frames)
        .map(|i| {
            let t = i as f64 / frames.max(1) as f64;
            let phase = (f64::from(from) * t + 0.5 * f64::from(to - from) * t * t)
                * f64::from(seconds)
                * std::f64::consts::TAU;
            let envelope = (quadratic_decay(t) * (i as f64 / 24.).min(1.)) as f32;
            let v = (phase.sin() + 0.18 * (phase * 2.).sin()) as f32 * envelope * 0.23;
            [v; 2]
        })
        .collect::<Vec<_>>();
    Sound::new(Clip {
        sample_rate: rate,
        samples: samples.into(),
    })
}
fn reload(rate: u32) -> Option<Sound> {
    let click = transient(rate, 1550., 0.75, 0.15, 0.05, 0x5151)?;
    let frames = (rate as f64 * 0.33) as usize;
    let mut samples = vec![[0.; 2]; frames];
    for offset in [
        0,
        (rate as f64 * 0.19) as usize,
        (rate as f64 * 0.27) as usize,
    ] {
        for (dst, src) in samples[offset..]
            .iter_mut()
            .zip(click.clip().samples.iter())
        {
            for c in 0..2 {
                dst[c] += src[c];
            }
        }
    }
    Sound::new(Clip {
        sample_rate: rate,
        samples: samples.into(),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bank_is_bounded_finite_deterministic_and_surfaces_are_distinct() {
        let a = SoundBank::procedural(24000).unwrap();
        let b = SoundBank::procedural(24000).unwrap();
        assert!(a.decoded_bytes() < 1024 * 1024);
        for i in 0..20 {
            assert_eq!(a.sounds[i].clip().samples, b.sounds[i].clip().samples);
            assert!(
                a.sounds[i]
                    .clip()
                    .samples
                    .iter()
                    .flatten()
                    .all(|v| v.is_finite() && v.abs() <= 1.)
            );
        }
        assert_ne!(
            a.get(Cue::Footstep(Surface::Stone)).clip().samples,
            a.get(Cue::Footstep(Surface::Metal)).clip().samples
        );
        let hum = a.get(Cue::Engine).clip().samples.as_ref();
        assert!((hum[0][0] - hum.last().unwrap()[0]).abs() < 0.03);
        assert!(SoundBank::procedural(1).is_none());
    }

    #[test]
    fn release_preserves_prefix_stereo_and_source_then_reaches_exact_silence() {
        let original = [
            [0.25, -0.5],
            [-0.25, 0.5],
            [0.5, -1.],
            [0.25, -0.5],
            [-0.25, 0.5],
            [0.5, -1.],
            [0.25, -0.5],
            [-0.25, 0.5],
        ];
        let source = Sound::new(Clip {
            sample_rate: 8000,
            samples: original.into(),
        })
        .unwrap();
        let released = source.with_release(2, 5).unwrap();
        let samples = &released.clip().samples;
        assert_eq!(released.clip().sample_rate, 8000);
        assert_eq!(samples.len(), 7);
        assert_eq!(&samples[..=2], &original[..=2]);
        for (frame, gain) in [(3, 0.5625), (4, 0.25), (5, 0.0625)] {
            assert_eq!(samples[frame], original[frame].map(|sample| sample * gain));
        }
        assert_eq!(samples[6].map(f32::to_bits), [0; 2]);
        assert_eq!(source.clip().samples.as_ref(), original);
        assert!(!std::sync::Arc::ptr_eq(
            &source.clip().samples,
            &released.clip().samples
        ));
    }

    #[test]
    fn release_handles_full_length_two_frame_and_already_silent_sources() {
        let source = Sound::new(Clip {
            sample_rate: 96000,
            samples: vec![[1., -1.]; 5].into(),
        })
        .unwrap();
        let full = source.with_release(0, 5).unwrap();
        assert_eq!(
            full.clip().samples.as_ref(),
            &[
                [1., -1.],
                [0.5625, -0.5625],
                [0.25, -0.25],
                [0.0625, -0.0625],
                [0., 0.],
            ]
        );
        let shortest = source.with_release(0, 2).unwrap();
        assert_eq!(shortest.clip().samples.as_ref(), &[[1., -1.], [0.; 2]]);
        let tail = source.with_release(3, 2).unwrap();
        assert_eq!(&tail.clip().samples[..4], &source.clip().samples[..4]);
        assert_eq!(tail.clip().samples[4].map(f32::to_bits), [0; 2]);

        let silent = Sound::new(Clip {
            sample_rate: 8000,
            samples: vec![[-0., 0.]; 8].into(),
        })
        .unwrap();
        let released = silent.with_release(1, 4).unwrap();
        assert!(
            released
                .clip()
                .samples
                .iter()
                .all(|frame| *frame == [0.; 2])
        );
        assert_eq!(released.clip().samples[4].map(f32::to_bits), [0; 2]);
    }

    #[test]
    fn release_rejects_invalid_and_overflowing_ranges_without_changing_source() {
        let source = Sound::new(Clip {
            sample_rate: 48000,
            samples: vec![[0.5, -0.5]; 8].into(),
        })
        .unwrap();
        let original = source.clip().samples.clone();
        for (start, frames) in [
            (0, 0),
            (0, 1),
            (0, 9),
            (7, 2),
            (8, 2),
            (usize::MAX, 2),
            (1, usize::MAX),
            (usize::MAX, usize::MAX),
            (0, super::super::MAX_CLIP_FRAMES + 1),
        ] {
            assert!(source.with_release(start, frames).is_none());
            assert!(std::sync::Arc::ptr_eq(&original, &source.clip().samples));
            assert_eq!(source.clip().samples.as_ref(), &[[0.5, -0.5]; 8]);
        }
        let one_frame = Sound::new(Clip {
            sample_rate: 8000,
            samples: vec![[1.; 2]].into(),
        })
        .unwrap();
        assert!(one_frame.with_release(0, 2).is_none());
    }

    #[test]
    fn released_bank_samples_remain_finite_and_bounded_at_supported_rate_limits() {
        for rate in [8000, 24000, 96000] {
            let bank = SoundBank::procedural(rate).unwrap();
            let mut released_bytes = 0;
            for source in &bank.sounds {
                let frames = source.clip().samples.len();
                let start = frames / 4;
                let released = source.with_release(start, frames / 2).unwrap();
                let samples = &released.clip().samples;
                assert_eq!(released.clip().sample_rate, rate);
                assert_eq!(samples.len(), start + frames / 2);
                assert_eq!(&samples[..=start], &source.clip().samples[..=start]);
                assert_eq!(samples.last().unwrap().map(f32::to_bits), [0; 2]);
                for (frame, original) in samples.iter().zip(source.clip().samples.iter()) {
                    for (sample, source_sample) in frame.iter().zip(original) {
                        assert!(sample.is_finite());
                        assert!(sample.abs() <= source_sample.abs());
                    }
                }
                released_bytes += samples.len() * std::mem::size_of::<[f32; 2]>();
                assert_eq!(
                    samples,
                    &source
                        .with_release(start, frames / 2)
                        .unwrap()
                        .clip()
                        .samples
                );
            }
            assert!(released_bytes < bank.decoded_bytes());
        }
    }
}
