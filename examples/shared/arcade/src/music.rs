//! Bounded host-owned note voices; rules only publish accepted MIDI events.
use ferravex_render::audio::{self, Bus, Clip, Sound, VoiceHandle, VoiceParams};

pub(crate) struct MusicAudio {
    // Lazily synthesize each pitch at most once. At most 128 * 11,520 stereo
    // frames are retained, regardless of campaign duration or note density.
    sounds: [Option<Sound>; 128],
    voices: [Option<VoiceHandle>; 32],
    next: usize,
}
impl Default for MusicAudio {
    fn default() -> Self {
        Self {
            sounds: std::array::from_fn(|_| None),
            voices: [None; 32],
            next: 0,
        }
    }
}
impl MusicAudio {
    pub fn play(&mut self, notes: [Option<u8>; 4]) {
        for note in notes.into_iter().flatten().filter(|n| *n < 128) {
            let sound = self.sounds[note as usize].get_or_insert_with(|| {
                let hz = 440. * 2_f32.powf((f32::from(note) - 69.) / 12.);
                Sound::new(Clip::tone(48_000, hz, 0.24).expect("valid MIDI tone"))
                    .expect("valid PCM")
            });
            if let Some(previous) = self.voices[self.next].take() {
                audio::stop(previous, 0);
            }
            self.voices[self.next] = audio::play_sound(
                sound,
                VoiceParams {
                    bus: Bus::Music,
                    gain: 0.55,
                    priority: 3,
                    ramp_frames: 64,
                    ..Default::default()
                },
            );
            self.next = (self.next + 1) % self.voices.len();
        }
    }
    pub fn clear(&mut self) {
        for voice in &mut self.voices {
            if let Some(handle) = voice.take() {
                audio::stop(handle, 0);
            }
        }
        self.next = 0;
    }
}
impl Drop for MusicAudio {
    fn drop(&mut self) {
        self.clear();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepted_notes_make_pcm_and_pause_discards_owned_voices() {
        audio::set_muted(false);
        let mut music = MusicAudio::default();
        music.play([Some(60), Some(64), Some(67), None]);
        let mut pcm = [[0.; 2]; 512];
        assert!(audio::render(&mut pcm, 48_000));
        assert!(pcm.iter().flatten().any(|v| v.abs() > 0.001));
        music.clear();
        assert!(audio::render(&mut pcm, 48_000));
        assert!(pcm.iter().flatten().all(|v| v.abs() < 0.000001));
    }
    #[test]
    fn invalid_notes_are_ignored_and_repeated_pitch_reuses_one_clip() {
        let mut music = MusicAudio::default();
        for _ in 0..40 {
            music.play([Some(60), Some(128), Some(255), None]);
        }
        assert_eq!(music.sounds.iter().filter(|s| s.is_some()).count(), 1);
        assert_eq!(music.voices.iter().flatten().count(), 32);
        music.clear();
        assert!(music.voices.iter().all(Option::is_none));
    }
}
