//! Small reusable helpers; games still decide which simulation event made a sound.
use super::*;
use std::rc::Weak;

const CADENCE_SEED: u32 = 0x71da51;
// Mixer accepts at most 128 voices. Keep ownership bounded even as old handles
// expire or are stolen; no allocation is needed to accept or discard a cue.
const TRACKED_VOICES: usize = 128;

/// Local gameplay audio with explicit suspension, independent of simulation time.
///
/// Pause/reset discard only this instance's gameplay voices. Non-positional
/// [`Cue::Click`] voices use the UI bus and remain independent of this lifecycle,
/// as do other instances, music and the user's mute/bus/master settings. There
/// is no pending-cue queue. Admission describes the mixer, not device audibility.
/// Runtime replacement discards stale voice ownership and cadence history.
/// Controllers belong to their creating thread, like the runtime mixer:
///
/// ```compile_fail
/// let audio = ferravex_render::audio::GameAudio::new();
/// std::thread::spawn(move || drop(audio));
/// ```
///
/// ```
/// use ferravex_render::audio::{Cue, GameAudio};
/// let mut audio = GameAudio::new();
/// audio.pause();
/// assert!(audio.is_paused());
/// assert_eq!(audio.cue_with_outcome(Cue::Jump, 0.5), None);
/// audio.reset(); // Reset during a menu stays paused.
/// assert!(audio.is_paused());
/// audio.resume();
/// assert!(!audio.is_paused());
/// assert!(audio.cue_with_outcome(Cue::Jump, 0.5).unwrap().handle().is_some());
/// ```
pub struct GameAudio {
    runtime: Weak<()>,
    motor: Option<VoiceHandle>,
    voices: [Option<VoiceHandle>; TRACKED_VOICES],
    paused: bool,
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
    /// Preload the shared bank and start unpaused. See [`Self`].
    pub fn new() -> Self {
        preload_cues();
        Self {
            runtime: AUDIO.with(|cell| Rc::downgrade(&cell.borrow().as_ref().unwrap().identity)),
            motor: None,
            voices: [None; TRACKED_VOICES],
            paused: false,
            last: None,
            travel: 0.,
            grounded: false,
            seed: CADENCE_SEED,
            acoustics: AcousticProfile::default(),
        }
    }
    /// Immediately discard this instance's gameplay voices and restart its
    /// footfall history/pitch sequence, retaining acoustics and pause state.
    /// UI clicks and shared mixer settings are unchanged. See [`Self`].
    pub fn reset(&mut self) {
        self.clear_gameplay();
        self.seed = CADENCE_SEED;
    }
    /// Immediately discard gameplay voices (including releasing motors) and
    /// footfall history, then suppress new gameplay cues until [`Self::resume`].
    /// Repeated calls are harmless; UI clicks remain usable. See [`Self`].
    pub fn pause(&mut self) {
        if !self.paused {
            self.clear_gameplay();
            self.paused = true;
        }
    }
    /// Accept fresh gameplay cues again. Does not replay suppressed events or
    /// restart the motor: the next active [`Self::motor`] call supplies its state.
    /// The first valid walk sample establishes a new origin. See [`Self`].
    pub fn resume(&mut self) {
        self.paused = false;
    }
    /// Whether gameplay cues are locally suspended, regardless of device or
    /// user mute state. See [`Self`].
    pub fn is_paused(&self) -> bool {
        self.paused
    }
    fn bind_runtime(&mut self, runtime: Weak<()>) {
        if !Weak::ptr_eq(&self.runtime, &runtime) {
            self.runtime = runtime;
            self.voices.fill(None);
            self.motor = None;
            self.last = None;
            self.travel = 0.;
            self.grounded = false;
            self.seed = CADENCE_SEED;
        }
    }
    fn sync_runtime(&mut self) {
        // A weak identity keeps its allocation reserved while a stale owner
        // exists, preventing address reuse from aliasing a replacement mixer.
        // try_with also permits controllers to drop after audio TLS teardown.
        let runtime = AUDIO
            .try_with(|cell| {
                cell.borrow()
                    .as_ref()
                    .map(|audio| Rc::downgrade(&audio.identity))
            })
            .ok()
            .flatten()
            .unwrap_or_default();
        self.bind_runtime(runtime);
    }
    fn clear_gameplay(&mut self) {
        self.sync_runtime();
        for voice in &mut self.voices {
            if let Some(handle) = voice.take() {
                stop(handle, 0);
            }
        }
        self.motor = None;
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
    /// Play a cue, discarding its admission result for compatibility.
    /// See [`Self::cue_with_outcome`] for validation and suspension semantics.
    pub fn cue(&mut self, cue: Cue, gain: f32) {
        let _ = self.cue_with_outcome(cue, gain);
    }
    /// Submit a non-positional cue using the existing mixer admission policy.
    ///
    /// `None` means local pause suppressed the cue without touching the mixer.
    /// Otherwise the result distinguishes a start, a steal and a rejection.
    /// The mixer validates the resulting parameters, including the gain after
    /// acoustics (finite, in `0..=4`). Clicks bypass acoustics and local pause.
    /// Muted, zero-gain and device-unavailable playback can still be admitted;
    /// the host continues to render/skip these voices normally.
    ///
    /// ```
    /// use ferravex_render::audio::{Cue, GameAudio, VoiceAdmission, VoiceRejection};
    /// let mut audio = GameAudio::new();
    /// assert_eq!(audio.cue_with_outcome(Cue::Fire, f32::NAN),
    ///     Some(VoiceAdmission::Rejected(VoiceRejection::InvalidParams)));
    /// let admission = audio.cue_with_outcome(Cue::Fire, 0.5).unwrap();
    /// assert!(admission.handle().is_some());
    /// audio.pause();
    /// assert_eq!(audio.cue_with_outcome(Cue::Fire, 0.5), None);
    /// assert!(audio.cue_with_outcome(Cue::Click, 0.2).unwrap().handle().is_some());
    /// ```
    #[must_use]
    pub fn cue_with_outcome(&mut self, cue: Cue, gain: f32) -> Option<VoiceAdmission> {
        let profile = if matches!(cue, Cue::Click) {
            AcousticProfile::default()
        } else {
            self.acoustics
        };
        self.start(
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
        )
    }
    /// Play a positional gameplay cue, discarding its admission result.
    /// See [`Self::at_with_outcome`].
    pub fn at(&mut self, cue: Cue, position: Vec3, gain: f32) {
        let _ = self.at_with_outcome(cue, position, gain);
    }
    /// Submit a positional gameplay cue. All positional cues, including Click,
    /// retain the original effects-bus/acoustics behavior and obey local pause.
    /// Invalid positions or resulting gains are rejected without a voice change.
    /// Outcomes have the same meaning as [`Self::cue_with_outcome`].
    ///
    /// ```
    /// use ferravex_render::{audio::{Cue, GameAudio, VoiceAdmission, VoiceRejection},
    ///     prelude::Vec3};
    /// let mut audio = GameAudio::new();
    /// assert_eq!(audio.at_with_outcome(Cue::Fire, Vec3::splat(f32::NAN), 0.5),
    ///     Some(VoiceAdmission::Rejected(VoiceRejection::InvalidParams)));
    /// assert!(audio.at_with_outcome(Cue::Fire, Vec3::ZERO, 0.5)
    ///     .unwrap().handle().is_some());
    /// audio.pause();
    /// assert_eq!(audio.at_with_outcome(Cue::Click, Vec3::ZERO, 0.2), None);
    /// ```
    #[must_use]
    pub fn at_with_outcome(
        &mut self,
        cue: Cue,
        position: Vec3,
        gain: f32,
    ) -> Option<VoiceAdmission> {
        self.start(
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
        )
    }
    fn start(&mut self, cue: Cue, params: VoiceParams) -> Option<VoiceAdmission> {
        if self.paused && params.bus != Bus::Ui {
            return None;
        }
        initialize();
        Some(AUDIO.with(|cell| {
            let mut state = cell.borrow_mut();
            let audio = state.as_mut().unwrap();
            self.bind_runtime(Rc::downgrade(&audio.identity));
            let Audio { mixer, bank, .. } = audio;
            let bank = bank.get_or_insert_with(|| SoundBank::procedural(24000).unwrap());
            let admission = mixer.start_with_outcome(bank.get(cue), params);
            if params.bus != Bus::Ui
                && let Some(handle) = admission.handle()
            {
                // A successful admission must leave a free/stale entry: the
                // mixer cannot have more live handles than this fixed array.
                let slot = self
                    .voices
                    .iter_mut()
                    .find(|voice| voice.is_none_or(|voice| mixer.params(voice).is_none()))
                    .expect("admitted voice fits the mixer's 128-voice bound");
                *slot = Some(handle);
            }
            admission
        }))
    }
    /// Local footfall cadence follows actual ground travel, avoiding sounds from
    /// pushing into a wall. Teleports, air motion and menus reset accumulated travel.
    /// Local pause ignores samples; resume establishes a fresh origin.
    pub fn walk(
        &mut self,
        position: Vec3,
        grounded: bool,
        active: bool,
        surface: Surface,
        stride: f32,
    ) {
        self.sync_runtime();
        if self.paused
            || !position.is_finite()
            || !stride.is_finite()
            || !(0.1..=1000.).contains(&stride)
        {
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
            let _ = self.start(
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
    /// inactive/invalid input releases the voice over 128 output frames. Explicit
    /// [`Self::pause`] discards it immediately, even without an output callback.
    pub fn motor(&mut self, speed: f32, max_speed: f32, active: bool) {
        self.sync_runtime();
        if self.paused || !active || !speed.is_finite() || !max_speed.is_finite() || max_speed <= 0.
        {
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
        self.motor = self
            .start(Cue::Engine, params)
            .and_then(VoiceAdmission::handle);
    }
}
impl Drop for GameAudio {
    fn drop(&mut self) {
        self.sync_runtime();
        if let Some(h) = self.motor.take() {
            stop(h, 128);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> GameAudio {
        crate::edit(|s| *s = crate::State::default());
        super::super::reset();
        set_muted(false);
        set_master_gain(0.7);
        GameAudio::new()
    }

    fn params(handle: VoiceHandle) -> VoiceParams {
        AUDIO.with(|cell| {
            cell.borrow()
                .as_ref()
                .unwrap()
                .mixer
                .params(handle)
                .unwrap()
        })
    }

    fn mixer_limit(limit: usize) {
        AUDIO.with(|cell| {
            cell.borrow_mut().as_mut().unwrap().mixer = Mixer::new(SAMPLE_RATE, limit).unwrap();
        });
    }

    #[test]
    fn old_controller_pause_and_reset_cannot_stop_replacement_runtime_voices() {
        for pause in [false, true] {
            let mut old = fresh();
            let old_voice = old
                .cue_with_outcome(Cue::Jump, 0.5)
                .unwrap()
                .handle()
                .unwrap();
            crate::start(Default::default(), async {});
            let mut new = GameAudio::new();
            let new_voice = new
                .cue_with_outcome(Cue::Fire, 0.5)
                .unwrap()
                .handle()
                .unwrap();
            assert_eq!(
                old_voice, new_voice,
                "raw handles are intentionally mixer-local"
            );
            if pause {
                old.pause();
            } else {
                old.reset();
            }
            assert!(playing(new_voice));
            assert_eq!(stats().active, 1);
        }
    }

    #[test]
    fn motor_updates_release_and_drop_ignore_replaced_runtime_handles() {
        for active in [false, true] {
            let mut old = fresh();
            old.motor(10., 40., true);
            let old_voice = old.motor.unwrap();
            super::super::reset();
            let mut new = GameAudio::new();
            new.motor(20., 40., true);
            let new_voice = new.motor.unwrap();
            let new_params = params(new_voice);
            assert_eq!(old_voice, new_voice);
            old.motor(30., 40., active);
            assert_eq!(params(new_voice), new_params);
            drop(old);
            render(&mut [[0.; 2]; 128], SAMPLE_RATE);
            assert!(playing(new_voice));
            assert_eq!(params(new_voice), new_params);
        }
        let mut old = fresh();
        old.motor(10., 40., true);
        super::super::reset();
        let mut new = GameAudio::new();
        new.motor(20., 40., true);
        let new_voice = new.motor.unwrap();
        drop(old);
        render(&mut [[0.; 2]; 128], SAMPLE_RATE);
        assert!(
            playing(new_voice),
            "drop without another method must also rebase ownership"
        );
    }

    #[test]
    fn replacement_reuses_tracking_and_resets_walk_history() {
        let mut old = fresh();
        mixer_limit(TRACKED_VOICES);
        for _ in 0..TRACKED_VOICES {
            assert!(
                old.cue_with_outcome(Cue::Jump, 0.5)
                    .unwrap()
                    .handle()
                    .is_some()
            );
        }
        old.walk(Vec3::ZERO, true, true, Surface::Stone, 1.5);
        old.walk(vec3(1.4, 0., 0.), true, true, Surface::Stone, 1.5);
        super::super::reset();
        let mut new = GameAudio::new();
        let new_voice = new
            .cue_with_outcome(Cue::Fire, 0.5)
            .unwrap()
            .handle()
            .unwrap();
        old.walk(vec3(1.6, 0., 0.), true, true, Surface::Stone, 1.5);
        assert_eq!(
            stats().active,
            1,
            "replacement establishes a new walking origin"
        );
        let admitted = old
            .cue_with_outcome(Cue::Jump, 0.5)
            .unwrap()
            .handle()
            .unwrap();
        assert_ne!(admitted, new_voice);
        old.reset();
        assert!(!playing(admitted));
        assert!(playing(new_voice));
    }

    #[test]
    fn controller_drop_after_audio_thread_local_teardown_is_harmless() {
        std::thread::spawn(|| {
            thread_local! { static OWNER: RefCell<Option<GameAudio>> = const { RefCell::new(None) }; }
            OWNER.with(|owner| {
                let mut audio = GameAudio::new();
                audio.motor(10., 40., true);
                *owner.borrow_mut() = Some(audio);
            });
        }).join().unwrap();
    }

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

    #[test]
    fn pause_discards_owned_gameplay_but_preserves_ui_and_other_owners() {
        let mut sound = fresh();
        let mut other = GameAudio::new();
        let other_cue = other
            .cue_with_outcome(Cue::Fire, 0.5)
            .unwrap()
            .handle()
            .unwrap();
        let music = play(
            Cue::Engine,
            VoiceParams {
                looping: true,
                bus: Bus::Music,
                ..Default::default()
            },
        )
        .unwrap();
        let click = sound
            .cue_with_outcome(Cue::Click, 0.2)
            .unwrap()
            .handle()
            .unwrap();
        let cue = sound
            .cue_with_outcome(Cue::Jump, 0.5)
            .unwrap()
            .handle()
            .unwrap();
        let positional = sound
            .at_with_outcome(Cue::Click, Vec3::ZERO, 0.2)
            .unwrap()
            .handle()
            .unwrap();
        sound.walk(Vec3::ZERO, true, true, Surface::Stone, 1.);
        sound.walk(vec3(1., 0., 0.), true, true, Surface::Stone, 1.);
        sound.motor(10., 40., true);
        let motor = sound.motor.unwrap();
        assert_eq!(stats().active, 7);
        set_master_gain(0.3);
        set_bus_gain(Bus::Effects, 0.4);
        let buses = bus_gains();

        sound.pause();
        sound.pause();
        assert!(sound.is_paused());
        assert!(!playing(cue));
        assert!(!playing(positional));
        assert!(!playing(motor));
        assert!(playing(click) && playing(other_cue) && playing(music));
        assert_eq!(
            stats().active,
            3,
            "owned footsteps must also stop immediately"
        );
        assert_eq!(master_gain(), 0.3);
        assert_eq!(bus_gains(), buses);
        assert!(!muted());

        let before = stats();
        assert_eq!(sound.cue_with_outcome(Cue::Fire, 0.5), None);
        assert_eq!(sound.at_with_outcome(Cue::Click, Vec3::ZERO, 0.2), None);
        assert_eq!(sound.cue_with_outcome(Cue::Jump, f32::NAN), None);
        sound.cue(Cue::Jump, 0.5);
        sound.at(Cue::Fire, Vec3::ZERO, 0.5);
        sound.walk(vec3(2., 0., 0.), true, true, Surface::Stone, 1.);
        sound.motor(20., 40., true);
        assert_eq!(stats(), before);
        let menu = sound
            .cue_with_outcome(Cue::Click, 0.2)
            .unwrap()
            .handle()
            .unwrap();
        sound.reset();
        assert!(playing(menu) && playing(click) && playing(other_cue) && playing(music));
        assert!(sound.is_paused());
        assert_eq!(master_gain(), 0.3);
        assert_eq!(bus_gains(), buses);
    }

    #[test]
    fn resume_discards_paused_travel_and_waits_for_fresh_motor_input() {
        let mut sound = fresh();
        sound.walk(Vec3::ZERO, true, true, Surface::Stone, 1.);
        sound.walk(vec3(0.75, 0., 0.), true, true, Surface::Stone, 1.);
        sound.motor(10., 40., true);
        sound.pause();
        sound.walk(vec3(2., 1., 0.), false, true, Surface::Stone, 1.);
        sound.motor(40., 40., true);
        sound.resume();
        sound.resume();
        assert_eq!(
            stats().active,
            0,
            "resume has no queued events or cached motor"
        );
        sound.walk(vec3(2., 0., 0.), true, true, Surface::Stone, 1.);
        sound.walk(vec3(2.5, 0., 0.), true, true, Surface::Stone, 1.);
        assert_eq!(
            stats().active,
            0,
            "no landing or partial stride from before pause"
        );
        sound.resume(); // An already resumed controller keeps its current stride.
        sound.walk(vec3(3., 0., 0.), true, true, Surface::Stone, 1.);
        assert_eq!(stats().active, 1);
        sound.motor(20., 40., true);
        let motor = sound.motor.unwrap();
        sound.motor(30., 40., true);
        assert_eq!(sound.motor, Some(motor));
        assert_eq!(stats().active, 2);
    }

    #[test]
    fn reset_clears_release_tails_and_repeats_cadence_without_changing_acoustics() {
        let mut sound = fresh();
        let profile = AcousticProfile {
            gain: 0.4,
            low_pass_hz: 1200.,
        };
        assert!(sound.set_acoustics(profile));
        sound.walk(Vec3::ZERO, true, true, Surface::Stone, 1.);
        sound.walk(vec3(1., 0., 0.), true, true, Surface::Stone, 1.);
        let first = sound.voices.iter().flatten().copied().next().unwrap();
        let first_params = params(first);
        sound.walk(vec3(2., 0., 0.), true, true, Surface::Stone, 1.);
        sound.motor(10., 40., true);
        sound.motor(10., 40., false);
        assert!(sound.motor.is_none());
        assert_eq!(stats().active, 3);
        sound.reset();
        sound.reset();
        assert_eq!(stats().active, 0, "reset does not need an output callback");
        assert!(!sound.is_paused());
        assert_eq!(sound.acoustics, profile);
        sound.walk(vec3(10., 0., 0.), true, true, Surface::Stone, 1.);
        assert_eq!(stats().active, 0);
        sound.walk(vec3(11., 0., 0.), true, true, Surface::Stone, 1.);
        let repeated = sound.voices.iter().flatten().copied().next().unwrap();
        assert_eq!(params(repeated), first_params);
        sound.motor(20., 40., true);
        sound.motor(20., 40., false);
        sound.pause();
        assert_eq!(
            stats().active,
            0,
            "pause also clears detached release tails"
        );
        sound.reset();
        assert!(sound.is_paused());
        sound.resume();
        assert_eq!(stats().active, 0);
    }

    #[test]
    fn outcomes_preserve_mixer_reservation_stealing_and_rejection_reasons() {
        let mut sound = fresh();
        mixer_limit(2);
        AUDIO.with(|cell| {
            assert!(
                cell.borrow_mut()
                    .as_mut()
                    .unwrap()
                    .mixer
                    .set_voice_policy(Some(VoicePolicy {
                        critical_priority: 5,
                        reserved_critical: 1
                    },))
            );
        });
        let cosmetic = sound
            .cue_with_outcome(Cue::Jump, 0.5)
            .unwrap()
            .handle()
            .unwrap();
        assert_eq!(
            sound.cue_with_outcome(Cue::Jump, 0.5),
            Some(VoiceAdmission::Rejected(
                VoiceRejection::ReservedForCritical
            ))
        );
        let critical = sound
            .cue_with_outcome(Cue::Fire, 0.5)
            .unwrap()
            .handle()
            .unwrap();
        let replacement = sound.cue_with_outcome(Cue::Failure, 0.5).unwrap();
        assert_eq!(
            replacement,
            VoiceAdmission::Stolen {
                handle: replacement.handle().unwrap(),
                replaced: cosmetic,
            }
        );
        assert!(!playing(cosmetic));
        assert!(playing(critical));
        assert_eq!(
            sound.cue_with_outcome(Cue::Fire, 0.5),
            Some(VoiceAdmission::Rejected(VoiceRejection::NoLowerPriority))
        );
        assert_eq!(stats().dropped, 2);
        assert_eq!(stats().stolen, 1);
        sound.pause();
        assert_eq!(stats().active, 0);
    }

    #[test]
    fn invalid_cues_leave_existing_voices_and_admission_counters_unchanged() {
        let mut sound = fresh();
        let handle = sound
            .cue_with_outcome(Cue::Jump, 0.5)
            .unwrap()
            .handle()
            .unwrap();
        let before = stats();
        let rejected = Some(VoiceAdmission::Rejected(VoiceRejection::InvalidParams));
        for gain in [f32::NAN, f32::INFINITY, -1., 4.1] {
            assert_eq!(sound.cue_with_outcome(Cue::Fire, gain), rejected);
            assert_eq!(sound.at_with_outcome(Cue::Fire, Vec3::ZERO, gain), rejected);
        }
        for position in [Vec3::splat(f32::NAN), vec3(0., f32::INFINITY, 0.)] {
            assert_eq!(sound.at_with_outcome(Cue::Fire, position, 0.5), rejected);
        }
        assert_eq!(stats(), before);
        assert!(playing(handle));
    }

    #[test]
    fn compatibility_wrappers_keep_gain_bus_spatial_and_priority_settings() {
        let mut sound = fresh();
        sound.set_acoustics(AcousticProfile {
            gain: 0.5,
            low_pass_hz: 1200.,
        });
        sound.cue(Cue::Fire, 0.6);
        let fire = sound.voices.iter().flatten().copied().next().unwrap();
        assert_eq!(
            params(fire),
            VoiceParams {
                gain: 0.3,
                low_pass_hz: 1200.,
                priority: 5,
                ..Default::default()
            }
        );
        sound.at(Cue::Click, vec3(1., 2., 3.), 0.4);
        let spatial = sound.voices.iter().flatten().copied().nth(1).unwrap();
        assert_eq!(
            params(spatial),
            VoiceParams {
                position: [1., 2., 3.],
                gain: 0.2,
                low_pass_hz: 1200.,
                priority: 3,
                spatial: Spatial::Attenuated {
                    min_distance: 1.,
                    max_distance: 65.,
                    rolloff: 0.04
                },
                ..Default::default()
            }
        );
        let click = sound
            .cue_with_outcome(Cue::Click, 0.4)
            .unwrap()
            .handle()
            .unwrap();
        assert_eq!(
            params(click),
            VoiceParams {
                gain: 0.4,
                priority: 2,
                bus: Bus::Ui,
                ..Default::default()
            }
        );
    }

    #[test]
    fn stale_owned_handles_do_not_stop_other_owners_replacement_voices() {
        let mut sound = fresh();
        mixer_limit(1);
        sound.motor(10., 40., true);
        let stale = sound.motor.unwrap();
        let replacement = play(
            Cue::Fire,
            VoiceParams {
                priority: 5,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!playing(stale));
        sound.pause();
        sound.reset();
        assert!(playing(replacement));
        assert_eq!(stats().active, 1);
        sound.resume();
        sound.motor(10., 40., true);
        assert!(sound.motor.is_none());
        stop(replacement, 0);
        sound.motor(10., 40., true);
        assert!(sound.motor.is_some_and(playing));
    }

    #[test]
    fn tracking_reuses_finished_slots_and_handles_the_full_mixer_bound() {
        let mut sound = fresh();
        mixer_limit(TRACKED_VOICES);
        for _ in 0..TRACKED_VOICES * 2 {
            let voice = sound
                .cue_with_outcome(Cue::Jump, 0.5)
                .unwrap()
                .handle()
                .unwrap();
            assert!(stop(voice, 0));
        }
        for _ in 0..TRACKED_VOICES {
            assert!(
                sound
                    .cue_with_outcome(Cue::Jump, 0.5)
                    .unwrap()
                    .handle()
                    .is_some()
            );
        }
        assert_eq!(stats().active, TRACKED_VOICES);
        assert!(
            sound
                .cue_with_outcome(Cue::Fire, 0.5)
                .unwrap()
                .replaced()
                .is_some()
        );
        sound.reset();
        assert_eq!(stats().active, 0);
    }

    #[test]
    fn muted_or_absent_output_does_not_turn_admission_into_an_audibility_claim() {
        let mut sound = fresh();
        assert!(!output_stats().ready, "test never opens a device");
        let absent = sound
            .cue_with_outcome(Cue::Jump, 0.5)
            .unwrap()
            .handle()
            .unwrap();
        super::super::end_frame(0.1);
        super::super::end_frame(0.1);
        assert!(
            !playing(absent),
            "absent output still skips elapsed samples"
        );
        set_muted(true);
        let muted = sound
            .cue_with_outcome(Cue::Fire, 0.5)
            .unwrap()
            .handle()
            .unwrap();
        let mut output = [[1.; 2]; 256];
        assert!(render(&mut output, SAMPLE_RATE));
        assert!(output.iter().flatten().all(|sample| *sample == 0.));
        assert!(playing(muted));
        sound.pause();
        sound.reset();
        sound.resume();
        assert!(super::super::muted());
        assert_eq!(stats().active, 0);
        set_muted(false);
        let zero_gain = sound
            .cue_with_outcome(Cue::Jump, 0.)
            .unwrap()
            .handle()
            .unwrap();
        assert!(playing(zero_gain));
        assert!(render(&mut output, SAMPLE_RATE));
        assert!(output.iter().flatten().all(|sample| *sample == 0.));
    }
}
