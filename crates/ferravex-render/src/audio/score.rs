use super::*;
/// Two phase-aligned original stems. Starts/restarts both together after host
/// suspension and crossfades gains on musical bars without restarting cursors.
pub struct AdaptiveScore {
    sounds: [Sound; 2],
    voices: [Option<VoiceHandle>; 2],
    clock: MusicConductor,
    bpm: u16,
}
impl AdaptiveScore {
    pub fn new(bpm: u16, seed: u32) -> Option<Self> {
        Some(Self {
            sounds: music_stems(SAMPLE_RATE, bpm, seed)?,
            voices: [None; 2],
            clock: MusicConductor::new(bpm)?,
            bpm,
        })
    }
    pub fn update(&mut self, dt: f32, intensity: f32) -> bool {
        if !dt.is_finite()
            || !(0. ..=0.25).contains(&dt)
            || !intensity.is_finite()
            || !(0. ..=1.).contains(&intensity)
        {
            return false;
        }
        if !self.voices.iter().all(|v| v.is_some_and(playing)) {
            for v in &mut self.voices {
                if let Some(h) = v.take() {
                    stop(h, 0);
                }
            }
            self.clock = MusicConductor::new(self.bpm).unwrap();
            for (sound, voice) in self.sounds.iter().zip(&mut self.voices) {
                *voice = play_sound(
                    sound,
                    VoiceParams {
                        bus: Bus::Music,
                        gain: 0.,
                        looping: true,
                        priority: 1,
                        ..Default::default()
                    },
                );
            }
        }
        let frame = self.clock.update(dt, intensity).unwrap();
        for (voice, gain) in self.voices.iter().zip(frame.gains) {
            if let Some(h) = voice {
                set_params(
                    *h,
                    VoiceParams {
                        bus: Bus::Music,
                        gain,
                        looping: true,
                        priority: 1,
                        ramp_frames: 1024,
                        ..Default::default()
                    },
                );
            }
        }
        true
    }
}
impl Drop for AdaptiveScore {
    fn drop(&mut self) {
        for h in self.voices.iter().flatten() {
            stop(*h, 128);
        }
    }
}
