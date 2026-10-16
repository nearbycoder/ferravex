//! Bounded filter-based room acoustics and bar-quantized adaptive music. These
//! controls are shared by the same Rust mixer on native and WebAssembly hosts.
use super::{Clip, Sound, VoiceParams};
use ferravex_math::Vec3;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcousticProfile {
    pub gain: f32,
    pub low_pass_hz: f32,
}
impl Default for AcousticProfile {
    fn default() -> Self {
        Self {
            gain: 1.,
            low_pass_hz: 24000.,
        }
    }
}
impl AcousticProfile {
    pub fn valid(self) -> bool {
        self.gain.is_finite()
            && (0. ..=1.).contains(&self.gain)
            && self.low_pass_hz.is_finite()
            && (80. ..=24000.).contains(&self.low_pass_hz)
    }
    pub fn apply(self, mut voice: VoiceParams) -> VoiceParams {
        voice.gain *= self.gain;
        voice.low_pass_hz = voice.low_pass_hz.min(self.low_pass_hz);
        voice
    }
}
#[derive(Clone, Copy, Debug)]
pub struct AcousticZone {
    pub min: Vec3,
    pub max: Vec3,
    pub feather: f32,
    pub profile: AcousticProfile,
}
pub struct AcousticScene<const N: usize> {
    zones: [AcousticZone; N],
}
impl<const N: usize> AcousticScene<N> {
    pub fn new(zones: [AcousticZone; N]) -> Option<Self> {
        if N > 32
            || zones.iter().any(|z| {
                !z.min.is_finite()
                    || !z.max.is_finite()
                    || (z.max - z.min).min_element() <= 0.
                    || !z.feather.is_finite()
                    || !(0. ..=1000.).contains(&z.feather)
                    || !z.profile.valid()
            })
        {
            None
        } else {
            Some(Self { zones })
        }
    }
    /// Occlusion must come from a real host query. Zones alone cannot detect walls.
    pub fn sample(&self, listener: Vec3, occluded: bool) -> Option<AcousticProfile> {
        if !listener.is_finite() {
            return None;
        }
        let mut result = AcousticProfile::default();
        for z in self.zones {
            let margin = (listener - z.min).min(z.max - listener).min_element();
            if margin >= 0. {
                let weight = if z.feather == 0. {
                    1.
                } else {
                    (margin / z.feather).clamp(0., 1.)
                };
                result.gain = result.gain.min(1. + (z.profile.gain - 1.) * weight);
                result.low_pass_hz = result
                    .low_pass_hz
                    .min(24000. + (z.profile.low_pass_hz - 24000.) * weight);
            }
        }
        if occluded {
            result.gain *= 0.45;
            result.low_pass_hz = result.low_pass_hz.min(900.);
        }
        Some(result)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct MusicFrame {
    pub beat: f32,
    pub bar_changed: bool,
    pub gains: [f32; 2],
}
#[derive(Clone, Copy, Debug)]
pub struct MusicConductor {
    bpm: f64,
    beat: f64,
    intensity: f32,
    layer: f32,
}
impl MusicConductor {
    pub fn new(bpm: u16) -> Option<Self> {
        if !(40..=240).contains(&bpm) {
            None
        } else {
            Some(Self {
                bpm: f64::from(bpm),
                beat: 0.,
                intensity: 0.,
                layer: 0.,
            })
        }
    }
    pub fn update(&mut self, dt: f32, requested: f32) -> Option<MusicFrame> {
        if !dt.is_finite()
            || !(0. ..=0.25).contains(&dt)
            || !requested.is_finite()
            || !(0. ..=1.).contains(&requested)
        {
            return None;
        }
        let next = self.beat + f64::from(dt) * self.bpm / 60.;
        let bar_changed = next >= 4.;
        self.beat = next % 4.;
        if bar_changed {
            self.intensity = requested;
        }
        self.layer += (self.intensity - self.layer) * (1. - (-dt * 5.).exp());
        Some(MusicFrame {
            beat: self.beat as f32,
            bar_changed,
            gains: [0.22, 0.2 * self.layer],
        })
    }
}
/// Original short synchronized stems, synthesized at setup; no asset download.
pub fn music_stems(sample_rate: u32, bpm: u16, mut seed: u32) -> Option<[Sound; 2]> {
    if !(8000..=96000).contains(&sample_rate) || !(40..=240).contains(&bpm) {
        return None;
    }
    seed = seed.max(1);
    let beat = (f64::from(sample_rate) * 60. / f64::from(bpm)).round() as usize;
    let frames = beat * 4;
    let mut bed = Vec::with_capacity(frames);
    let mut pulse = Vec::with_capacity(frames);
    for i in 0..frames {
        let phase = (i % beat) as f32 / beat as f32;
        let seconds = (i % beat) as f32 / sample_rate as f32;
        let hz = [110., 130.8128, 146.8324, 98.][i / beat];
        let env = (std::f32::consts::PI * phase).sin().powi(2);
        let bass = (std::f32::consts::TAU * hz * seconds).sin() * env * 0.2;
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let noise = (seed as f64 / u32::MAX as f64 * 2. - 1.) as f32;
        let tap = noise * phase.min(0.01) * 100. * (-phase * 28.).exp() * 0.12;
        bed.push([bass; 2]);
        pulse.push([tap; 2]);
    }
    Some([
        Sound::new(Clip {
            sample_rate,
            samples: bed.into(),
        })?,
        Sound::new(Clip {
            sample_rate,
            samples: pulse.into(),
        })?,
    ])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zones_feather_and_occlusion_is_explicit() {
        let s = AcousticScene::new([AcousticZone {
            min: Vec3::splat(-10.),
            max: Vec3::splat(10.),
            feather: 2.,
            profile: AcousticProfile {
                gain: 0.8,
                low_pass_hz: 2500.,
            },
        }])
        .unwrap();
        assert_eq!(
            s.sample(Vec3::X * 11., false),
            Some(AcousticProfile::default())
        );
        assert_eq!(s.sample(Vec3::ZERO, true).unwrap().low_pass_hz, 900.);
        assert!(s.sample(Vec3::X * 9., false).unwrap().low_pass_hz > 2500.);
    }
    #[test]
    fn score_waits_for_bar_and_stems_share_length() {
        let mut c = MusicConductor::new(120).unwrap();
        for _ in 0..7 {
            assert_eq!(c.update(0.25, 1.).unwrap().gains[1], 0.);
        }
        let f = c.update(0.25, 1.).unwrap();
        assert!(f.bar_changed && f.gains[1] > 0.);
        let stems = music_stems(8000, 120, 9).unwrap();
        assert_eq!(stems[0].clip().samples.len(), stems[1].clip().samples.len());
        assert!(c.update(f32::NAN, 0.).is_none());
    }
}
