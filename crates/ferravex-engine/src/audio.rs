//! Original PCM16 WAV decoder and bounded stereo spatial mixer.
//! The mixer writes PCM; a platform audio device supplies its output callback.
use std::sync::Arc;
/// Maximum decoded interleaved-stereo storage: 64 MiB, also bounded to 10 minutes.
pub const MAX_CLIP_FRAMES: usize = 8 * 1024 * 1024;
const MAX_WAV_BYTES: usize = 64 * 1024 * 1024;
#[derive(Clone, Debug)]
pub struct Clip {
    pub sample_rate: u32,
    pub samples: Arc<[[f32; 2]]>,
}
impl Clip {
    /// Uncompressed PCM16 mono/stereo, 8–96 kHz. Rejects malformed chunk boundaries.
    pub fn wav(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 12
            || bytes.len() > MAX_WAV_BYTES
            || &bytes[..4] != b"RIFF"
            || &bytes[8..12] != b"WAVE"
        {
            return None;
        }
        let declared = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
        let end = usize::try_from(declared).ok()?.checked_add(8)?;
        if end != bytes.len() {
            return None;
        }
        let mut at = 12_usize;
        let mut format = None;
        let mut pcm = None;
        while at < end {
            let header_end = at.checked_add(8)?;
            let header = bytes.get(at..header_end)?;
            let len = u32::from_le_bytes(header[4..8].try_into().ok()?) as usize;
            at = header_end;
            let chunk_end = at.checked_add(len)?;
            let chunk = bytes.get(at..chunk_end)?;
            match &header[..4] {
                b"fmt " if format.is_none() && len >= 16 => {
                    let u16at = |i| u16::from_le_bytes(chunk[i..i + 2].try_into().unwrap());
                    let u32at = |i| u32::from_le_bytes(chunk[i..i + 4].try_into().unwrap());
                    let channels = u16at(2);
                    let rate = u32at(4);
                    let stride = u16at(12);
                    if u16at(0) != 1
                        || !(1..=2).contains(&channels)
                        || !(8000..=96000).contains(&rate)
                        || u16at(14) != 16
                        || stride != channels * 2
                        || u32at(8) != rate * u32::from(stride)
                    {
                        return None;
                    }
                    format = Some((usize::from(channels), rate));
                }
                b"data" if pcm.is_none() => pcm = Some(chunk),
                b"fmt " | b"data" => return None,
                _ => {}
            }
            at = chunk_end.checked_add(len % 2)?;
            if at > end {
                return None;
            }
        }
        let (channels, sample_rate) = format?;
        let pcm = pcm?;
        let frames = pcm.len() / (channels * 2);
        if !pcm.len().is_multiple_of(channels * 2) || frames > frame_limit(sample_rate) {
            return None;
        }
        let samples: Vec<_> = pcm
            .chunks_exact(channels * 2)
            .map(|frame| {
                let left = f32::from(i16::from_le_bytes([frame[0], frame[1]])) / 32768.0;
                let right = if channels == 1 {
                    left
                } else {
                    f32::from(i16::from_le_bytes([frame[2], frame[3]])) / 32768.0
                };
                [left, right]
            })
            .collect();
        Some(Self {
            sample_rate,
            samples: samples.into(),
        })
    }
    pub fn tone(sample_rate: u32, hz: f32, seconds: f32) -> Option<Self> {
        if !(8000..=96000).contains(&sample_rate)
            || !hz.is_finite()
            || hz <= 0.0
            || hz > sample_rate as f32 * 0.5
            || !seconds.is_finite()
            || !(0.0..=10.0).contains(&seconds)
        {
            return None;
        }
        let n = (sample_rate as f32 * seconds) as usize;
        let samples: Vec<_> = (0..n)
            .map(|i| {
                let envelope = 1.0 - i as f32 / n.max(1) as f32;
                // Wide phase intermediates prevent loss of precision on long, high-frequency tones.
                let phase =
                    i as f64 * f64::from(hz) * std::f64::consts::TAU / f64::from(sample_rate);
                let v = phase.sin() as f32 * envelope * 0.3;
                [v, v]
            })
            .collect();
        Some(Self {
            sample_rate,
            samples: samples.into(),
        })
    }
}
fn frame_limit(rate: u32) -> usize {
    (rate as usize * 600).min(MAX_CLIP_FRAMES)
}
struct Voice {
    clip: Clip,
    cursor: f64,
    position: [f32; 3],
    gain: f32,
    looping: bool,
}
pub struct Mixer {
    sample_rate: u32,
    voices: Vec<Voice>,
    limit: usize,
    pub listener: [f32; 3],
    pub right: [f32; 3],
}
impl Mixer {
    pub fn new(sample_rate: u32, limit: usize) -> Option<Self> {
        if !(8000..=96000).contains(&sample_rate) || !(1..=128).contains(&limit) {
            return None;
        }
        Some(Self {
            sample_rate,
            voices: Vec::with_capacity(limit),
            limit,
            listener: [0.0; 3],
            right: [1.0, 0.0, 0.0],
        })
    }
    /// Validates manually constructed clips outside the audio callback.
    /// Samples must be finite and within [-1,1]; playback never grows the voice allocation.
    pub fn play(&mut self, clip: Clip, position: [f32; 3], gain: f32, looping: bool) -> bool {
        if self.voices.len() >= self.limit
            || clip.samples.is_empty()
            || !(8000..=96000).contains(&clip.sample_rate)
            || clip.samples.len() > frame_limit(clip.sample_rate)
            || !position.iter().all(|v| v.is_finite())
            || !gain.is_finite()
            || !(0.0..=4.0).contains(&gain)
            || clip
                .samples
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > 1.0)
        {
            return false;
        }
        self.voices.push(Voice {
            clip,
            cursor: 0.0,
            position,
            gain,
            looping,
        });
        true
    }
    pub fn active_voices(&self) -> usize {
        self.voices.len()
    }
    /// No allocations; finished voices may release their clip Arc. Invalid listener poses mute
    /// playback while advancing voices. An invalid right vector falls back to centered panning.
    pub fn render(&mut self, out: &mut [[f32; 2]]) {
        out.fill([0.0; 2]);
        let right = self.right.map(f64::from);
        let right_len = right.iter().map(|v| v * v).sum::<f64>().sqrt();
        let listener_valid = self.listener.iter().all(|v| v.is_finite());
        for voice in &mut self.voices {
            let delta = std::array::from_fn::<_, 3, _>(|i| {
                f64::from(voice.position[i]) - f64::from(self.listener[i])
            });
            let distance_squared = delta.iter().map(|v| v * v).sum::<f64>();
            let distance = distance_squared.sqrt();
            let pan = if distance > 1.0e-5 && right_len > 1.0e-5 && right_len.is_finite() {
                (delta.iter().zip(right).map(|(a, b)| a * b).sum::<f64>() / (distance * right_len))
                    .clamp(-1.0, 1.0)
            } else {
                0.0
            };
            let gain = if listener_valid {
                f64::from(voice.gain) / (1.0 + distance_squared * 0.05)
            } else {
                0.0
            };
            let gains = [
                (((1.0 - pan) * 0.5).sqrt() * gain) as f32,
                (((1.0 + pan) * 0.5).sqrt() * gain) as f32,
            ];
            let step = f64::from(voice.clip.sample_rate) / f64::from(self.sample_rate);
            for sample in out.iter_mut() {
                let len = voice.clip.samples.len();
                if voice.cursor >= len as f64 {
                    if voice.looping {
                        voice.cursor %= len as f64;
                    } else {
                        break;
                    }
                }
                let index = voice.cursor as usize;
                let next = if index + 1 < len {
                    index + 1
                } else if voice.looping {
                    0
                } else {
                    index
                };
                let fraction = (voice.cursor - index as f64) as f32;
                for channel in 0..2 {
                    let a = voice.clip.samples[index][channel];
                    let b = voice.clip.samples[next][channel];
                    sample[channel] += (a + (b - a) * fraction) * gains[channel];
                }
                voice.cursor += step;
            }
        }
        self.voices
            .retain(|v| v.looping || v.cursor < v.clip.samples.len() as f64);
        for sample in out {
            for v in sample {
                *v = if v.is_finite() {
                    v.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
            }
        }
    }
}
pub fn encode_wav(sample_rate: u32, samples: &[[f32; 2]]) -> Option<Vec<u8>> {
    if !(8000..=96000).contains(&sample_rate) || samples.len() > frame_limit(sample_rate) {
        return None;
    }
    let len = u32::try_from(samples.len().checked_mul(4)?).ok()?;
    let mut out = Vec::with_capacity(len as usize + 44);
    out.extend(b"RIFF");
    out.extend((36 + len).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16_u32.to_le_bytes());
    out.extend(1_u16.to_le_bytes());
    out.extend(2_u16.to_le_bytes());
    out.extend(sample_rate.to_le_bytes());
    out.extend((sample_rate * 4).to_le_bytes());
    out.extend(4_u16.to_le_bytes());
    out.extend(16_u16.to_le_bytes());
    out.extend(b"data");
    out.extend(len.to_le_bytes());
    for frame in samples {
        for sample in frame {
            let v = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            let scale = if v < 0.0 { 32768.0 } else { 32767.0 };
            out.extend(((v * scale).round() as i16).to_le_bytes());
        }
    }
    Some(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spatial_pan_voice_limits_and_resampling() {
        let clip = Clip {
            sample_rate: 8000,
            samples: vec![[0.5; 2]; 8].into(),
        };
        let mut m = Mixer::new(16000, 1).unwrap();
        let capacity = m.voices.capacity();
        assert!(m.play(clip.clone(), [2.0, 0.0, 0.0], 1.0, false));
        assert!(!m.play(clip, [0.0; 3], 1.0, true));
        let mut out = [[0.0; 2]; 32];
        m.render(&mut out);
        assert!(out[0][1] > 0.3);
        assert_eq!(out[0][0], 0.0);
        assert_eq!(out[16], [0.0; 2]);
        assert_eq!(m.active_voices(), 0);
        assert_eq!(m.voices.capacity(), capacity);
    }
    #[test]
    fn wav_roundtrip_and_truncation() {
        let samples = [[0.25, -0.5]; 100];
        let bytes = encode_wav(48000, &samples).unwrap();
        let clip = Clip::wav(&bytes).unwrap();
        assert_eq!(clip.sample_rate, 48000);
        assert!((clip.samples[0][0] - 0.25).abs() < 0.0001);
        for i in 0..bytes.len() {
            assert!(Clip::wav(&bytes[..i]).is_none());
        }
    }
    #[test]
    fn invalid_clip_and_listener_do_not_poison_other_voices() {
        let mut m = Mixer::new(8000, 3).unwrap();
        assert!(!m.play(
            Clip {
                sample_rate: 8000,
                samples: vec![[f32::NAN; 2]].into()
            },
            [0.0; 3],
            1.0,
            false
        ));
        let clip = Clip {
            sample_rate: 8000,
            samples: vec![[0.5; 2]; 4].into(),
        };
        assert!(m.play(clip.clone(), [0.0; 3], 1.0, false));
        m.listener = [f32::NAN; 3];
        let mut out = [[1.0; 2]; 8];
        m.render(&mut out);
        assert_eq!(out, [[0.0; 2]; 8]);
        assert_eq!(m.active_voices(), 0);
        m.listener = [-f32::MAX; 3];
        m.right = [f32::MAX; 3];
        assert!(m.play(clip, [f32::MAX; 3], 1.0, false));
        m.render(&mut out);
        assert!(out.iter().flatten().all(|v| v.is_finite()));
    }
    #[test]
    fn looping_and_block_partition_preserve_sample_time() {
        let clip = Clip {
            sample_rate: 8000,
            samples: vec![[0.2; 2], [0.6; 2]].into(),
        };
        let mut a = Mixer::new(12000, 1).unwrap();
        let mut b = Mixer::new(12000, 1).unwrap();
        a.play(clip.clone(), [0.0; 3], 1.0, true);
        b.play(clip, [0.0; 3], 1.0, true);
        let mut whole = [[0.0; 2]; 30];
        a.render(&mut whole);
        let mut pieces = [[0.0; 2]; 30];
        for part in pieces.chunks_mut(3) {
            b.render(part);
        }
        assert_eq!(whole, pieces);
    }
    #[test]
    fn mono_pcm_is_duplicated_and_chunks_can_arrive_out_of_order() {
        let mut bytes = encode_wav(8000, &[[1.0, -1.0]]).unwrap();
        bytes[22..24].copy_from_slice(&1_u16.to_le_bytes());
        bytes[28..32].copy_from_slice(&16000_u32.to_le_bytes());
        bytes[32..34].copy_from_slice(&2_u16.to_le_bytes());
        let mut reordered = bytes[..12].to_vec();
        reordered.extend_from_slice(&bytes[36..]);
        reordered.extend_from_slice(&bytes[12..36]);
        let clip = Clip::wav(&reordered).unwrap();
        assert_eq!(clip.samples.len(), 2);
        assert_eq!(clip.samples[0][0], clip.samples[0][1]);
        assert_eq!(clip.samples[1], [-1.0; 2]);
    }
    #[test]
    fn wav_rejects_huge_lengths_duplicate_chunks_and_missing_padding() {
        let bytes = encode_wav(8000, &[[1.0, -1.0]]).unwrap();
        let decoded = Clip::wav(&bytes).unwrap();
        assert_eq!(decoded.samples[0][1], -1.0);
        let mut huge = bytes.clone();
        huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Clip::wav(&huge).is_none());
        let mut duplicate = bytes.clone();
        duplicate.extend_from_slice(&bytes[36..]);
        let declared = (duplicate.len() - 8) as u32;
        duplicate[4..8].copy_from_slice(&declared.to_le_bytes());
        assert!(Clip::wav(&duplicate).is_none());
        let mut odd = bytes;
        odd.extend(b"JUNK");
        odd.extend(1_u32.to_le_bytes());
        odd.push(0);
        let declared = (odd.len() - 8) as u32;
        odd[4..8].copy_from_slice(&declared.to_le_bytes());
        assert!(Clip::wav(&odd).is_none());
        odd.push(0);
        let declared = (odd.len() - 8) as u32;
        odd[4..8].copy_from_slice(&declared.to_le_bytes());
        assert!(Clip::wav(&odd).is_some());
    }
}
