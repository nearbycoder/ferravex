use super::{Clip, frame_limit};
/// A clip whose sample and metadata validation is complete. Keep a bank owner
/// alive during real-time playback if the last Arc must not free in render.
#[derive(Clone, Debug)]
pub struct Sound(Clip);
impl Sound {
    /// Decode a validated PCM16 WAV once; playback reuses the owned sample Arc.
    pub fn wav(bytes: &[u8]) -> Option<Self> {
        let clip = Clip::wav(bytes)?;
        if clip.samples.is_empty() {
            None
        } else {
            Some(Self(clip))
        }
    }

    pub fn new(clip: Clip) -> Option<Self> {
        if clip.samples.is_empty()
            || !(8000..=96000).contains(&clip.sample_rate)
            || clip.samples.len() > frame_limit(clip.sample_rate)
            || clip
                .samples
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > 1.)
        {
            return None;
        }
        Some(Self(clip))
    }
    pub fn clip(&self) -> &Clip {
        &self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Bus {
    Effects,
    Ambience,
    Music,
    Ui,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Spatial {
    /// Preserve stereo, without positional attenuation or panning.
    None,
    /// Inverse-square falloff beyond min_distance; silence at max_distance.
    Attenuated {
        min_distance: f32,
        max_distance: f32,
        rolloff: f32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceParams {
    pub position: [f32; 3],
    pub gain: f32,
    pub pitch: f32,
    pub looping: bool,
    pub spatial: Spatial,
    pub bus: Bus,
    pub priority: u8,
    /// Initial/changed channel gain smoothing. Zero gives sample-exact legacy playback.
    pub ramp_frames: u16,
}
impl Default for VoiceParams {
    fn default() -> Self {
        Self {
            position: [0.; 3],
            gain: 1.,
            pitch: 1.,
            looping: false,
            spatial: Spatial::None,
            bus: Bus::Effects,
            priority: 0,
            ramp_frames: 128,
        }
    }
}
impl VoiceParams {
    fn valid(self) -> bool {
        self.position.iter().all(|v| v.is_finite())
            && self.gain.is_finite()
            && (0. ..=4.).contains(&self.gain)
            && self.pitch.is_finite()
            && (0.25..=4.).contains(&self.pitch)
            && match self.spatial {
                Spatial::None => true,
                Spatial::Attenuated {
                    min_distance,
                    max_distance,
                    rolloff,
                } => {
                    min_distance.is_finite()
                        && max_distance.is_finite()
                        && rolloff.is_finite()
                        && min_distance >= 0.
                        && max_distance > min_distance
                        && (0. ..=16.).contains(&rolloff)
                }
            }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceHandle {
    slot: u8,
    generation: u64,
}
struct Voice {
    sound: Sound,
    cursor: f64,
    params: VoiceParams,
    gains: [f32; 2],
    targets: [f32; 2],
    ramp_left: u16,
    release: Option<u32>,
    age: u64,
}
struct Slot {
    generation: u64,
    voice: Option<Voice>,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct MixStats {
    pub rendered_frames: u64,
    pub dropped: u64,
    pub stolen: u64,
    pub active: usize,
}
pub struct Mixer {
    sample_rate: u32,
    voices: Vec<Slot>,
    limit: usize,
    age: u64,
    pub listener: [f32; 3],
    pub right: [f32; 3],
    master: f32,
    buses: [f32; 4],
    stats: MixStats,
}
impl Mixer {
    pub fn new(sample_rate: u32, limit: usize) -> Option<Self> {
        if !(8000..=96000).contains(&sample_rate) || !(1..=128).contains(&limit) {
            return None;
        }
        Some(Self {
            sample_rate,
            voices: (0..limit)
                .map(|_| Slot {
                    generation: 1,
                    voice: None,
                })
                .collect(),
            limit,
            age: 0,
            listener: [0.; 3],
            right: [1., 0., 0.],
            master: 1.,
            buses: [1.; 4],
            stats: MixStats::default(),
        })
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    /// Does not restart voices. Fractional source positions survive a device-rate change.
    pub fn set_sample_rate(&mut self, rate: u32) -> bool {
        if !(8000..=96000).contains(&rate) {
            return false;
        }
        self.sample_rate = rate;
        true
    }
    pub fn master_gain(&self) -> f32 {
        self.master
    }
    pub fn set_master_gain(&mut self, gain: f32) -> bool {
        if !gain.is_finite() || !(0. ..=1.).contains(&gain) {
            return false;
        }
        self.master = gain;
        true
    }
    pub fn bus_gain(&self, bus: Bus) -> f32 {
        self.buses[bus as usize]
    }
    pub fn set_bus_gain(&mut self, bus: Bus, gain: f32) -> bool {
        if !gain.is_finite() || !(0. ..=1.).contains(&gain) {
            return false;
        }
        self.buses[bus as usize] = gain;
        true
    }
    pub fn stats(&self) -> MixStats {
        MixStats {
            active: self.active_voices(),
            ..self.stats
        }
    }
    /// Compatibility entry point. Use a validated Sound for repeated playback.
    pub fn play(&mut self, clip: Clip, position: [f32; 3], gain: f32, looping: bool) -> bool {
        let Some(sound) = Sound::new(clip) else {
            return false;
        };
        self.start(
            &sound,
            VoiceParams {
                position,
                gain,
                looping,
                spatial: Spatial::Attenuated {
                    min_distance: 0.,
                    max_distance: f32::MAX,
                    rolloff: 0.05,
                },
                ramp_frames: 0,
                ..Default::default()
            },
        )
        .is_some()
    }
    /// Bounded insertion; a higher-priority sound can replace the oldest voice
    /// of the lowest lower priority. Equal-priority overload drops the new voice.
    pub fn start(&mut self, sound: &Sound, params: VoiceParams) -> Option<VoiceHandle> {
        if !params.valid() {
            return None;
        }
        let index = self
            .voices
            .iter()
            .position(|v| v.voice.is_none())
            .or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| {
                        s.voice
                            .as_ref()
                            .filter(|v| v.params.priority < params.priority)
                            .map(|v| (i, v.params.priority, v.age))
                    })
                    .min_by_key(|&(_, p, a)| (p, a))
                    .map(|(i, _, _)| i)
            });
        let Some(index) = index else {
            self.stats.dropped = self.stats.dropped.saturating_add(1);
            return None;
        };
        let slot = &mut self.voices[index];
        if slot.voice.is_some() {
            self.stats.stolen = self.stats.stolen.saturating_add(1);
            slot.generation = slot.generation.wrapping_add(1).max(1);
        }
        self.age = self.age.wrapping_add(1);
        slot.voice = Some(Voice {
            sound: sound.clone(),
            cursor: 0.,
            params,
            gains: [0.; 2],
            targets: [0.; 2],
            ramp_left: 0,
            release: None,
            age: self.age,
        });
        Some(VoiceHandle {
            slot: index as u8,
            generation: slot.generation,
        })
    }
    pub fn params(&self, handle: VoiceHandle) -> Option<VoiceParams> {
        self.voices
            .get(handle.slot as usize)
            .filter(|s| s.generation == handle.generation)?
            .voice
            .as_ref()
            .map(|v| v.params)
    }
    pub fn set_params(&mut self, handle: VoiceHandle, params: VoiceParams) -> bool {
        if !params.valid() {
            return false;
        }
        let Some(slot) = self
            .voices
            .get_mut(handle.slot as usize)
            .filter(|s| s.generation == handle.generation)
        else {
            return false;
        };
        let Some(v) = slot.voice.as_mut() else {
            return false;
        };
        v.params = params;
        true
    }
    pub fn stop(&mut self, handle: VoiceHandle, fade_frames: u32) -> bool {
        let Some(slot) = self
            .voices
            .get_mut(handle.slot as usize)
            .filter(|s| s.generation == handle.generation)
        else {
            return false;
        };
        let Some(v) = slot.voice.as_mut() else {
            return false;
        };
        if fade_frames == 0 {
            retire(slot);
        } else {
            v.release = Some(fade_frames.min(96000));
            v.targets = [0.; 2];
            v.ramp_left = fade_frames.min(u16::MAX as u32) as u16;
        }
        true
    }
    pub fn stop_all(&mut self) {
        for slot in &mut self.voices {
            if slot.voice.is_some() {
                retire(slot);
            }
        }
    }
    /// Drop transient/releasing voices and prime continuous loops for a quiet
    /// resume. Used at output focus/mute boundaries, with no allocation.
    pub fn suspend(&mut self) {
        for slot in &mut self.voices {
            if let Some(voice) = slot.voice.as_mut() {
                if !voice.params.looping || voice.release.is_some() {
                    retire(slot);
                } else {
                    voice.gains = [0.; 2];
                    voice.targets = [0.; 2];
                    voice.ramp_left = 0;
                }
            }
        }
    }
    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|s| s.voice.is_some()).count()
    }
    /// Bounded work, no allocations. Finished voices release their Sound Arc.
    pub fn render(&mut self, out: &mut [[f32; 2]]) {
        out.fill([0.; 2]);
        let right = self.right.map(f64::from);
        let right_len = right.iter().map(|v| v * v).sum::<f64>().sqrt();
        let listener_valid = self.listener.iter().all(|v| v.is_finite());
        for slot in &mut self.voices {
            let Some(voice) = slot.voice.as_mut() else {
                continue;
            };
            let delta = std::array::from_fn::<_, 3, _>(|i| {
                f64::from(voice.params.position[i]) - f64::from(self.listener[i])
            });
            let dist = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
            let (attenuation, pan) = match voice.params.spatial {
                Spatial::None => (1., 0.),
                Spatial::Attenuated {
                    min_distance,
                    max_distance,
                    rolloff,
                } => {
                    let d = (dist - f64::from(min_distance)).max(0.);
                    let gain = if dist >= f64::from(max_distance) {
                        0.
                    } else {
                        1. / (1. + d * d * f64::from(rolloff))
                    };
                    let pan = if dist > 1e-5 && right_len > 1e-5 && right_len.is_finite() {
                        (delta.iter().zip(right).map(|(a, b)| a * b).sum::<f64>()
                            / (dist * right_len))
                            .clamp(-1., 1.)
                    } else {
                        0.
                    };
                    (gain, pan)
                }
            };
            let gain = if listener_valid {
                voice.params.gain
                    * attenuation as f32
                    * self.buses[voice.params.bus as usize]
                    * self.master
            } else {
                0.
            };
            let target = if voice.release.is_some() {
                [0.; 2]
            } else if voice.params.spatial == Spatial::None {
                [gain; 2]
            } else {
                [
                    ((1. - pan) * 0.5).sqrt() as f32 * gain,
                    ((1. + pan) * 0.5).sqrt() as f32 * gain,
                ]
            };
            if voice.release.is_none() && voice.params.ramp_frames == 0 {
                voice.gains = target;
            } else if voice.release.is_none() && voice.targets != target {
                voice.ramp_left = voice.params.ramp_frames;
            }
            voice.targets = target;
            let step = f64::from(voice.sound.0.sample_rate) / f64::from(self.sample_rate)
                * f64::from(voice.params.pitch);
            let len = voice.sound.0.samples.len();
            for sample in out.iter_mut() {
                if voice.cursor >= len as f64 {
                    if voice.params.looping {
                        voice.cursor %= len as f64;
                    } else {
                        break;
                    }
                }
                if voice.release == Some(0) {
                    break;
                }
                if voice.ramp_left > 0 {
                    for (gain, target) in voice.gains.iter_mut().zip(target) {
                        *gain += (target - *gain) / f32::from(voice.ramp_left);
                    }
                    voice.ramp_left -= 1;
                }
                let index = voice.cursor as usize;
                let next = if index + 1 < len {
                    index + 1
                } else if voice.params.looping {
                    0
                } else {
                    index
                };
                let fraction = (voice.cursor - index as f64) as f32;
                let source = std::array::from_fn::<_, 2, _>(|c| {
                    let a = voice.sound.0.samples[index][c];
                    let b = voice.sound.0.samples[next][c];
                    a + (b - a) * fraction
                });
                let mono = (source[0] + source[1]) * 0.5;
                let tail = if !voice.params.looping && voice.params.ramp_frames > 0 {
                    (((len as f64 - voice.cursor) / step) / f64::from(voice.params.ramp_frames))
                        .clamp(0., 1.) as f32
                } else {
                    1.
                };
                for (c, sample) in sample.iter_mut().enumerate() {
                    let value = if voice.params.spatial == Spatial::None {
                        source[c]
                    } else {
                        mono
                    };
                    *sample += value * voice.gains[c] * tail;
                }
                voice.cursor += step;
                if let Some(left) = voice.release.as_mut() {
                    *left = left.saturating_sub(1);
                }
            }
            if voice.release == Some(0) || (!voice.params.looping && voice.cursor >= len as f64) {
                retire(slot);
            }
        }
        for frame in out.iter_mut() {
            for value in frame {
                *value = if value.is_finite() {
                    value.clamp(-1., 1.)
                } else {
                    0.
                };
            }
        }
        self.stats.rendered_frames = self.stats.rendered_frames.saturating_add(out.len() as u64);
    }
    /// Advance inaudible/suspended hosts in bounded O(voices), without generating PCM.
    pub fn skip(&mut self, frames: u32) {
        for slot in &mut self.voices {
            if let Some(v) = slot.voice.as_mut() {
                v.gains = [0.; 2];
                v.targets = [0.; 2];
                v.ramp_left = 0;
                v.cursor += f64::from(frames) * f64::from(v.sound.0.sample_rate)
                    / f64::from(self.sample_rate)
                    * f64::from(v.params.pitch);
                if v.params.looping {
                    v.cursor %= v.sound.0.samples.len() as f64;
                }
                if let Some(left) = v.release.as_mut() {
                    *left = left.saturating_sub(frames);
                }
                if v.release == Some(0)
                    || (!v.params.looping && v.cursor >= v.sound.0.samples.len() as f64)
                {
                    retire(slot);
                }
            }
        }
    }
    pub fn voice_limit(&self) -> usize {
        self.limit
    }
}
fn retire(slot: &mut Slot) {
    slot.voice = None;
    slot.generation = slot.generation.wrapping_add(1).max(1);
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sound() -> Sound {
        Sound::new(Clip {
            sample_rate: 8000,
            samples: vec![[0.5; 2]; 800].into(),
        })
        .unwrap()
    }
    #[test]
    fn handles_stops_priority_and_validation_are_transactional() {
        let sound = sound();
        let mut m = Mixer::new(8000, 1).unwrap();
        let h = m.start(&sound, VoiceParams::default()).unwrap();
        assert!(m.start(&sound, VoiceParams::default()).is_none());
        let h2 = m
            .start(
                &sound,
                VoiceParams {
                    priority: 1,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_ne!(h, h2);
        assert!(m.params(h).is_none());
        assert!(!m.stop(h, 0));
        let before = m.params(h2).unwrap();
        assert!(!m.set_params(
            h2,
            VoiceParams {
                pitch: f32::NAN,
                ..before
            }
        ));
        assert_eq!(m.params(h2), Some(before));
        assert!(m.stop(h2, 16));
        m.render(&mut [[0.; 2]; 16]);
        assert_eq!(m.active_voices(), 0);
        assert_eq!(m.stats().stolen, 1);
        assert_eq!(m.stats().dropped, 1);
    }
    #[test]
    fn buses_pitch_spatial_and_mute_have_finite_output() {
        let sound = sound();
        let mut m = Mixer::new(8000, 2).unwrap();
        let h = m
            .start(
                &sound,
                VoiceParams {
                    pitch: 4.,
                    ramp_frames: 0,
                    ..Default::default()
                },
            )
            .unwrap();
        let mut out = [[0.; 2]; 200];
        m.render(&mut out);
        assert_eq!(m.active_voices(), 0);
        assert_eq!(out[0], [0.5; 2]);
        assert!(m.params(h).is_none());
        m.set_bus_gain(Bus::Effects, 0.2);
        m.start(
            &sound,
            VoiceParams {
                ramp_frames: 0,
                ..Default::default()
            },
        );
        m.render(&mut out[..1]);
        assert_eq!(out[0], [0.1; 2]);
        m.set_master_gain(0.);
        m.render(&mut out[..1]);
        assert_eq!(out[0], [0.; 2]);
        assert!(!m.set_master_gain(f32::INFINITY));
        m.skip(8000);
        assert_eq!(m.active_voices(), 0);
    }
    #[test]
    fn suspension_discards_one_shots_and_resumes_loops_from_silence() {
        let sound = sound();
        let mut m = Mixer::new(8000, 2).unwrap();
        let shot = m.start(&sound, VoiceParams::default()).unwrap();
        let motor = m
            .start(
                &sound,
                VoiceParams {
                    looping: true,
                    ..Default::default()
                },
            )
            .unwrap();
        m.render(&mut [[0.; 2]; 128]);
        m.suspend();
        assert!(m.params(shot).is_none());
        assert!(m.params(motor).is_some());
        let mut out = [[0.; 2]; 1];
        m.render(&mut out);
        assert!(out[0][0] > 0. && out[0][0] < 0.01);
    }
    #[test]
    fn positional_stereo_is_downmixed_and_explicit_stop_fades_even_without_gain_smoothing() {
        let sound = Sound::new(Clip {
            sample_rate: 8000,
            samples: vec![[1., 0.]; 800].into(),
        })
        .unwrap();
        let mut m = Mixer::new(8000, 1).unwrap();
        let h = m
            .start(
                &sound,
                VoiceParams {
                    ramp_frames: 0,
                    position: [2., 0., 0.],
                    spatial: Spatial::Attenuated {
                        min_distance: 3.,
                        max_distance: 10.,
                        rolloff: 1.,
                    },
                    ..Default::default()
                },
            )
            .unwrap();
        let mut out = [[0.; 2]; 16];
        m.render(&mut out[..1]);
        assert_eq!(out[0], [0., 0.5]);
        m.stop(h, 16);
        m.render(&mut out);
        assert!(out[0][1] > 0.4);
        assert_eq!(out[15], [0.; 2]);
        assert_eq!(m.active_voices(), 0);
    }
    #[test]
    fn ramp_partition_and_device_rate_change_preserve_source_time() {
        let sound = sound();
        let mut a = Mixer::new(8000, 1).unwrap();
        let mut b = Mixer::new(8000, 1).unwrap();
        for m in [&mut a, &mut b] {
            m.start(
                &sound,
                VoiceParams {
                    looping: true,
                    ..Default::default()
                },
            );
        }
        let mut whole = [[0.; 2]; 256];
        let mut pieces = whole;
        a.render(&mut whole);
        for part in pieces.chunks_mut(13) {
            b.render(part);
        }
        assert_eq!(whole, pieces);
        a.set_sample_rate(16000);
        a.render(&mut whole);
        assert!(
            whole
                .iter()
                .flatten()
                .all(|v| v.is_finite() && v.abs() <= 0.5)
        );
    }
}
