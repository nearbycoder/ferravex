//! Small reusable helpers; games still decide which simulation event made a sound.
use super::*;
pub struct GameAudio {
    motor: Option<VoiceHandle>,
    last: Option<Vec3>,
    travel: f32,
    grounded: bool,
    seed: u32,
    acoustics: AcousticProfile,
}
impl Default for GameAudio {
    fn default() -> Self {
        Self::new()
    }
}
impl GameAudio {
    pub fn new() -> Self {
        preload_cues();
        Self {
            motor: None,
            last: None,
            travel: 0.,
            grounded: false,
            seed: 0x71da51,
            acoustics: AcousticProfile::default(),
        }
    }
    pub fn reset(&mut self) {
        if let Some(motor) = self.motor.take() {
            stop(motor, 128);
        }
        self.last = None;
        self.travel = 0.;
        self.grounded = false;
    }
    pub fn set_acoustics(&mut self, profile: AcousticProfile) -> bool {
        if !profile.valid() {
            return false;
        }
        self.acoustics = profile;
        true
    }
    pub fn cue(&mut self, cue: Cue, gain: f32) {
        let profile = if matches!(cue, Cue::Click) {
            AcousticProfile::default()
        } else {
            self.acoustics
        };
        play(
            cue,
            VoiceParams {
                gain: gain * profile.gain,
                low_pass_hz: profile.low_pass_hz,
                priority: if matches!(cue, Cue::Fire | Cue::Failure) {
                    5
                } else {
                    2
                },
                bus: if matches!(cue, Cue::Click) {
                    Bus::Ui
                } else {
                    Bus::Effects
                },
                ..Default::default()
            },
        );
    }
    pub fn at(&mut self, cue: Cue, position: Vec3, gain: f32) {
        play(
            cue,
            VoiceParams {
                position: position.to_array(),
                gain: gain * self.acoustics.gain,
                low_pass_hz: self.acoustics.low_pass_hz,
                spatial: Spatial::Attenuated {
                    min_distance: 1.,
                    max_distance: 65.,
                    rolloff: 0.04,
                },
                priority: 3,
                ..Default::default()
            },
        );
    }
    /// Local footfall cadence follows actual ground travel, avoiding sounds from
    /// pushing into a wall. Teleports, air motion and menus reset accumulated travel.
    pub fn walk(
        &mut self,
        position: Vec3,
        grounded: bool,
        active: bool,
        surface: Surface,
        stride: f32,
    ) {
        if !position.is_finite() || !stride.is_finite() || !(0.1..=1000.).contains(&stride) {
            return;
        }
        let previous = self.last.replace(position);
        let was_grounded = self.grounded;
        self.grounded = grounded;
        if !active || !grounded {
            self.travel = 0.;
            return;
        }
        let Some(previous) = previous else {
            return;
        };
        let delta = position - previous;
        if delta.length() > stride * 4. {
            self.travel = 0.;
            return;
        }
        if !was_grounded && delta.y < -0.04 {
            self.cue(Cue::Impact(surface), 0.55);
        }
        self.travel += vec3(delta.x, 0., delta.z).length();
        for _ in 0..2 {
            if self.travel < stride {
                break;
            }
            self.travel -= stride;
            self.seed ^= self.seed << 13;
            self.seed ^= self.seed >> 17;
            self.seed ^= self.seed << 5;
            let pitch = 0.94 + (self.seed % 1024) as f32 / 1024. * 0.12;
            play(
                Cue::Footstep(surface),
                VoiceParams {
                    gain: 0.55 * self.acoustics.gain,
                    pitch,
                    low_pass_hz: self.acoustics.low_pass_hz,
                    priority: 1,
                    ..Default::default()
                },
            );
        }
    }
    /// Local continuous motor/propeller loop. Speed changes pitch and loudness;
    /// stopping or pausing releases the voice instead of stacking fresh loops.
    pub fn motor(&mut self, speed: f32, max_speed: f32, active: bool) {
        if !active || !speed.is_finite() || !max_speed.is_finite() || max_speed <= 0. {
            if let Some(h) = self.motor.take() {
                stop(h, 128);
            }
            return;
        }
        let amount = (speed.abs() / max_speed).clamp(0., 1.);
        let params = VoiceParams {
            gain: (0.12 + amount * 0.28) * self.acoustics.gain,
            low_pass_hz: self.acoustics.low_pass_hz,
            pitch: 0.6 + amount * 2.2,
            looping: true,
            bus: Bus::Ambience,
            priority: 1,
            ..Default::default()
        };
        if let Some(h) = self.motor
            && set_params(h, params)
        {
            return;
        }
        self.motor = play(Cue::Engine, params);
    }
}
impl Drop for GameAudio {
    fn drop(&mut self) {
        if let Some(h) = self.motor.take() {
            stop(h, 128);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cadence_uses_ground_travel_and_motor_reuses_its_voice() {
        crate::edit(|s| *s = crate::State::default());
        super::super::reset();
        let mut sound = GameAudio::new();
        sound.walk(Vec3::ZERO, true, true, Surface::Stone, 1.5);
        sound.walk(vec3(0.5, 0., 0.), true, true, Surface::Stone, 1.5);
        assert_eq!(stats().active, 0);
        sound.walk(vec3(1.6, 0., 0.), true, true, Surface::Stone, 1.5);
        assert_eq!(stats().active, 1);
        sound.walk(vec3(100., 0., 0.), true, true, Surface::Stone, 1.5);
        assert_eq!(stats().active, 1, "teleport is not a footstep");
        sound.walk(vec3(101., 0., 0.), false, true, Surface::Stone, 1.5);
        assert_eq!(stats().active, 1);
        stop_all();
        sound.motor(10., 40., true);
        let voice = sound.motor.unwrap();
        sound.motor(20., 40., true);
        assert_eq!(sound.motor, Some(voice));
        assert_eq!(stats().active, 1);
        sound.motor(20., 40., false);
        assert!(sound.motor.is_none());
        render(&mut [[0.; 2]; 128], 48000);
        assert_eq!(stats().active, 0);
    }
}
