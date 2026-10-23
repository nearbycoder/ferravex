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

/// Invalid input to an [`AcousticTransition`]. Errors leave its state unchanged.
///
/// ```
/// use ferravex_play::audio::{AcousticTransition, AcousticTransitionError};
/// let mut transition = AcousticTransition::default();
/// assert_eq!(transition.advance(f64::NAN), Err(AcousticTransitionError::InvalidDelta));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcousticTransitionError {
    /// Gain must be finite in 0..=1 and cutoff finite in 80..=24000 Hz.
    InvalidProfile,
    /// Transition duration must be finite and nonnegative, in seconds.
    InvalidDuration,
    /// Elapsed time must be finite and nonnegative, in seconds.
    InvalidDelta,
}
impl std::fmt::Display for AcousticTransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidProfile => "acoustic gain must be in 0..=1 and cutoff in 80..=24000 Hz",
            Self::InvalidDuration => "acoustic transition duration must be finite and nonnegative",
            Self::InvalidDelta => "acoustic elapsed time must be finite and nonnegative",
        })
    }
}
impl std::error::Error for AcousticTransitionError {}

/// An allocation-free, opt-in transition between room/occlusion profiles.
///
/// Gain and cutoff interpolate linearly over seconds, reaching the exact target
/// at completion. Time is supplied by the caller; there is no sample-rate,
/// renderer or wall-clock dependency. For an unchanged target, splitting elapsed
/// time into smaller updates gives the same profile within floating-point error.
/// Advance once per chosen presentation interval, then share [`Self::current`]
/// across voices. This does not advance or pause gameplay, network or replay time.
///
/// [`AcousticScene::sample`] remains instantaneous and retains its authored
/// defaults. Apply the result to the original voice parameters each update;
/// repeatedly applying it to already attenuated parameters compounds the gain.
/// Mixer bus/master fades and user preferences remain independent.
///
/// ```
/// use ferravex_math::Vec3;
/// use ferravex_play::audio::{AcousticScene, AcousticTransition, VoiceParams};
/// let scene = AcousticScene::<0>::new([]).unwrap();
/// let mut transition = AcousticTransition::default();
/// // A real host query supplies the occlusion flag.
/// let occluded = scene.sample(Vec3::ZERO, true).unwrap();
/// transition.transition_to(occluded, 0.2)?;
/// let halfway = transition.advance(0.1)?;
/// assert!(halfway.gain > occluded.gain && halfway.gain < 1.0);
/// let authored = VoiceParams { gain: 0.5, ..Default::default() };
/// let voice = halfway.apply(authored);
/// assert!(voice.gain < authored.gain);
/// assert_eq!(transition.advance(0.1)?, occluded);
/// assert!(!transition.is_transitioning());
/// # Ok::<(), ferravex_play::audio::AcousticTransitionError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AcousticTransition {
    start: AcousticProfile,
    target: AcousticProfile,
    duration_seconds: f64,
    elapsed_seconds: f64,
}
impl Default for AcousticTransition {
    fn default() -> Self {
        let profile = AcousticProfile::default();
        Self {
            start: profile,
            target: profile,
            duration_seconds: 0.,
            elapsed_seconds: 0.,
        }
    }
}
impl AcousticTransition {
    /// Start settled at a checked profile, without an initial fade.
    ///
    /// ```
    /// use ferravex_play::audio::{AcousticProfile, AcousticTransition};
    /// let profile = AcousticProfile { gain: 0.8, low_pass_hz: 2500.0 };
    /// let transition = AcousticTransition::new(profile)?;
    /// assert_eq!(transition.current(), profile);
    /// assert_eq!(transition.target(), profile);
    /// assert!(!transition.is_transitioning());
    /// # Ok::<(), ferravex_play::audio::AcousticTransitionError>(())
    /// ```
    pub fn new(profile: AcousticProfile) -> Result<Self, AcousticTransitionError> {
        if !profile.valid() {
            return Err(AcousticTransitionError::InvalidProfile);
        }
        Ok(Self {
            start: profile,
            target: profile,
            duration_seconds: 0.,
            elapsed_seconds: 0.,
        })
    }

    /// Current finite profile, without advancing time. See [`Self::new`].
    pub fn current(&self) -> AcousticProfile {
        if !self.is_transitioning() {
            return self.target;
        }
        let fraction = self.elapsed_seconds / self.duration_seconds;
        let blend = |start: f32, target: f32| {
            (f64::from(start) + (f64::from(target) - f64::from(start)) * fraction) as f32
        };
        AcousticProfile {
            gain: blend(self.start.gain, self.target.gain),
            low_pass_hz: blend(self.start.low_pass_hz, self.target.low_pass_hz),
        }
    }

    /// Last accepted destination profile. See [`Self::new`].
    pub fn target(&self) -> AcousticProfile {
        self.target
    }

    /// Whether transition time remains. See [`AcousticTransition`].
    pub fn is_transitioning(&self) -> bool {
        self.elapsed_seconds < self.duration_seconds
    }

    /// Retarget from the current profile without a value jump.
    ///
    /// Duration must be finite and nonnegative. Zero snaps immediately, even to
    /// an existing target. Resubmitting the same target with a positive duration
    /// preserves the existing timeline (the new duration is ignored), so sampling
    /// an unchanged scene each tick cannot indefinitely postpone completion.
    /// A destination equal to the current profile settles immediately. All inputs
    /// are checked before mutation, including when the destination is unchanged.
    ///
    /// ```
    /// use ferravex_play::audio::{AcousticProfile, AcousticTransition};
    /// let mut transition = AcousticTransition::default();
    /// transition.transition_to(AcousticProfile { gain: 0.0, low_pass_hz: 80.0 }, 1.0)?;
    /// let current = transition.advance(0.5)?;
    /// transition.transition_to(AcousticProfile::default(), 0.25)?;
    /// assert_eq!(transition.current(), current);
    /// assert_eq!(transition.advance(0.25)?, AcousticProfile::default());
    /// # Ok::<(), ferravex_play::audio::AcousticTransitionError>(())
    /// ```
    pub fn transition_to(
        &mut self,
        target: AcousticProfile,
        duration_seconds: f64,
    ) -> Result<(), AcousticTransitionError> {
        if !target.valid() {
            return Err(AcousticTransitionError::InvalidProfile);
        }
        if !duration_seconds.is_finite() || duration_seconds < 0. {
            return Err(AcousticTransitionError::InvalidDuration);
        }
        if duration_seconds == 0. {
            return self.reset(target);
        }
        if target == self.target {
            return Ok(());
        }
        let start = self.current();
        if start == target {
            return self.reset(target);
        }
        *self = Self {
            start,
            target,
            duration_seconds,
            elapsed_seconds: 0.,
        };
        Ok(())
    }

    /// Advance by finite, nonnegative seconds, returning the current profile.
    ///
    /// Zero freezes the transition. Time past completion is discarded and the
    /// target is returned exactly; it does not carry into later transitions.
    /// Elapsed time is accumulated in f64 rather than rounding each update's
    /// profile, preserving small steps at audio callback/sample rates. See
    /// [`AcousticTransition`] for an example.
    pub fn advance(
        &mut self,
        elapsed_seconds: f64,
    ) -> Result<AcousticProfile, AcousticTransitionError> {
        if !elapsed_seconds.is_finite() || elapsed_seconds < 0. {
            return Err(AcousticTransitionError::InvalidDelta);
        }
        // Compare before adding so even f64::MAX deltas cannot overflow state.
        if elapsed_seconds >= self.duration_seconds - self.elapsed_seconds {
            self.elapsed_seconds = self.duration_seconds;
        } else {
            self.elapsed_seconds += elapsed_seconds;
        }
        Ok(self.current())
    }

    /// Immediately settle at a checked profile, clearing all transition history.
    /// Use on restart/teleport or to restore [`AcousticProfile::default`].
    ///
    /// ```
    /// use ferravex_play::audio::{AcousticProfile, AcousticTransition};
    /// let mut transition = AcousticTransition::default();
    /// transition.transition_to(AcousticProfile { gain: 0.2, low_pass_hz: 900.0 }, 1.0)?;
    /// transition.advance(0.1)?;
    /// transition.reset(AcousticProfile::default())?;
    /// assert!(!transition.is_transitioning());
    /// assert_eq!(transition.advance(10.0)?, AcousticProfile::default());
    /// # Ok::<(), ferravex_play::audio::AcousticTransitionError>(())
    /// ```
    pub fn reset(&mut self, profile: AcousticProfile) -> Result<(), AcousticTransitionError> {
        *self = Self::new(profile)?;
        Ok(())
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

    fn muffled() -> AcousticProfile {
        AcousticProfile {
            gain: 0.,
            low_pass_hz: 80.,
        }
    }

    fn assert_profile_near(actual: AcousticProfile, expected: AcousticProfile) {
        assert!((actual.gain - expected.gain).abs() < 1e-6);
        assert!((actual.low_pass_hz - expected.low_pass_hz).abs() < 0.002);
        assert!(actual.valid());
    }

    #[test]
    fn transition_interpolates_both_controls_and_settles_exactly() {
        let initial = AcousticProfile::default();
        let mut transition = AcousticTransition::default();
        assert_eq!(transition.current(), initial);
        assert_eq!(transition.target(), initial);
        assert!(!transition.is_transitioning());
        transition.transition_to(muffled(), 2.).unwrap();
        assert!(transition.is_transitioning());
        assert_eq!(transition.current(), initial);
        assert_eq!(transition.target(), muffled());
        assert_eq!(transition.advance(0.).unwrap(), initial);
        assert_eq!(
            transition.advance(0.5).unwrap(),
            AcousticProfile {
                gain: 0.75,
                low_pass_hz: 18020.,
            }
        );
        assert_eq!(
            transition.advance(0.5).unwrap(),
            AcousticProfile {
                gain: 0.5,
                low_pass_hz: 12040.,
            }
        );
        assert_eq!(transition.advance(1.).unwrap(), muffled());
        assert!(!transition.is_transitioning());
        let settled = transition;
        assert_eq!(transition.advance(f64::MAX).unwrap(), muffled());
        assert_eq!(transition, settled);
    }

    #[test]
    fn retargeting_is_continuous_and_repeated_targets_do_not_restart() {
        let mut transition = AcousticTransition::default();
        transition.transition_to(muffled(), 1.).unwrap();
        let partial = transition.advance(0.25).unwrap();
        let before = transition;
        transition.transition_to(muffled(), 10.).unwrap();
        assert_eq!(
            transition, before,
            "same target keeps the original timeline"
        );
        transition
            .transition_to(AcousticProfile::default(), 0.5)
            .unwrap();
        assert_eq!(transition.current(), partial);
        assert!(transition.advance(0.25).unwrap().gain > partial.gain);
        assert_eq!(
            transition.advance(0.25).unwrap(),
            AcousticProfile::default()
        );
        assert!(!transition.is_transitioning());
        for _ in 0..4 {
            transition.transition_to(muffled(), 1.).unwrap();
            transition.advance(0.25).unwrap();
        }
        assert_eq!(transition.current(), muffled());
        assert!(!transition.is_transitioning());
    }

    #[test]
    fn reset_immediate_targets_and_zero_time_have_explicit_semantics() {
        let mut transition = AcousticTransition::default();
        transition.transition_to(muffled(), 1.).unwrap();
        let current = transition.advance(0.25).unwrap();
        let before = transition;
        assert_eq!(transition.advance(0.).unwrap(), current);
        assert_eq!(transition, before);

        transition.transition_to(current, 5.).unwrap();
        assert_eq!(transition, AcousticTransition::new(current).unwrap());
        transition.transition_to(muffled(), 1.).unwrap();
        transition.advance(0.25).unwrap();
        transition.transition_to(muffled(), 0.).unwrap();
        assert_eq!(transition, AcousticTransition::new(muffled()).unwrap());
        transition
            .transition_to(AcousticProfile::default(), 1.)
            .unwrap();
        transition.advance(0.25).unwrap();
        transition.reset(current).unwrap();
        assert_eq!(transition, AcousticTransition::new(current).unwrap());
        assert_eq!(transition.advance(100.).unwrap(), current);

        transition.reset(AcousticProfile::default()).unwrap();
        assert_eq!(transition, AcousticTransition::default());
        transition.transition_to(muffled(), 1.).unwrap();
        assert_eq!(transition.advance(100.).unwrap(), muffled());
        transition
            .transition_to(AcousticProfile::default(), 1.)
            .unwrap();
        assert_eq!(transition.advance(0.).unwrap(), muffled());
        assert_eq!(transition.advance(0.5).unwrap().gain, 0.5);
    }

    #[test]
    fn invalid_profiles_are_transactional_for_construction_retarget_and_reset() {
        let mut transition = AcousticTransition::default();
        transition.transition_to(muffled(), 1.).unwrap();
        transition.advance(0.25).unwrap();
        let before = transition;
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 1.1] {
            let profile = AcousticProfile {
                gain: invalid,
                ..AcousticProfile::default()
            };
            assert_eq!(
                AcousticTransition::new(profile),
                Err(AcousticTransitionError::InvalidProfile)
            );
            assert_eq!(
                transition.transition_to(profile, 1.),
                Err(AcousticTransitionError::InvalidProfile)
            );
            assert_eq!(
                transition.reset(profile),
                Err(AcousticTransitionError::InvalidProfile)
            );
            assert_eq!(transition, before);
        }
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 79., 24001.] {
            let profile = AcousticProfile {
                low_pass_hz: invalid,
                ..AcousticProfile::default()
            };
            assert_eq!(
                AcousticTransition::new(profile),
                Err(AcousticTransitionError::InvalidProfile)
            );
            assert_eq!(
                transition.transition_to(profile, 0.),
                Err(AcousticTransitionError::InvalidProfile)
            );
            assert_eq!(
                transition.reset(profile),
                Err(AcousticTransitionError::InvalidProfile)
            );
            assert_eq!(transition, before);
        }
        assert_eq!(transition.advance(0.75).unwrap(), muffled());
    }

    #[test]
    fn invalid_times_are_rejected_even_for_unchanged_or_settled_targets() {
        let mut active = AcousticTransition::default();
        active.transition_to(muffled(), 1.).unwrap();
        active.advance(0.25).unwrap();
        for mut transition in [active, AcousticTransition::default()] {
            let before = transition;
            for invalid in [
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                -1.,
                -f64::MIN_POSITIVE,
            ] {
                for target in [transition.target(), transition.current()] {
                    assert_eq!(
                        transition.transition_to(target, invalid),
                        Err(AcousticTransitionError::InvalidDuration)
                    );
                    assert_eq!(transition, before);
                }
                assert_eq!(
                    transition.advance(invalid),
                    Err(AcousticTransitionError::InvalidDelta)
                );
                assert_eq!(transition, before);
            }
        }
    }

    #[test]
    fn elapsed_seconds_are_independent_of_sample_rates_and_update_partitioning() {
        let mut reference = AcousticTransition::default();
        reference.transition_to(muffled(), 2.).unwrap();
        let expected = reference.advance(1.).unwrap();
        for rate in [8000, 44100, 48000, 96000] {
            for block in [1, 127, 512] {
                let mut transition = AcousticTransition::default();
                transition.transition_to(muffled(), 2.).unwrap();
                let mut remaining = rate;
                while remaining > 0 {
                    let frames = remaining.min(block);
                    transition
                        .advance(f64::from(frames) / f64::from(rate))
                        .unwrap();
                    remaining -= frames;
                }
                assert_profile_near(transition.current(), expected);
                // A device-rate switch needs no transition reconfiguration.
                for _ in 0..480 {
                    transition.advance(50. / 48000.).unwrap();
                }
                assert_profile_near(
                    transition.current(),
                    AcousticProfile {
                        gain: 0.25,
                        low_pass_hz: 6060.,
                    },
                );
                assert_eq!(transition.advance(1.).unwrap(), muffled());
                assert!(!transition.is_transitioning());
            }
        }
    }

    #[test]
    fn extreme_finite_times_and_repeated_reversals_keep_profiles_bounded() {
        for duration in [f64::from_bits(1), f64::MIN_POSITIVE, 1., f64::MAX] {
            let mut transition = AcousticTransition::default();
            transition.transition_to(muffled(), duration).unwrap();
            assert!(transition.advance(duration * 0.25).unwrap().valid());
            assert_eq!(transition.advance(duration).unwrap(), muffled());
            assert!(!transition.is_transitioning());
            assert!(transition.duration_seconds.is_finite());
            assert!(transition.elapsed_seconds.is_finite());
        }
        let mut transition = AcousticTransition::default();
        for i in 0..1000 {
            let previous = transition.current();
            let target = if i % 2 == 0 {
                muffled()
            } else {
                AcousticProfile::default()
            };
            transition.transition_to(target, 0.25).unwrap();
            assert_eq!(transition.current(), previous);
            let current = transition.advance(1. / 60.).unwrap();
            assert!(current.valid());
            assert!(
                (previous.gain.min(target.gain)..=previous.gain.max(target.gain))
                    .contains(&current.gain)
            );
            assert!(
                (previous.low_pass_hz.min(target.low_pass_hz)
                    ..=previous.low_pass_hz.max(target.low_pass_hz))
                    .contains(&current.low_pass_hz)
            );
        }
    }

    #[test]
    fn sampled_occlusion_enters_and_clears_gradually_without_changing_authored_values() {
        let authored = AcousticProfile {
            gain: 0.8,
            low_pass_hz: 2500.,
        };
        let scene = AcousticScene::new([AcousticZone {
            min: Vec3::splat(-10.),
            max: Vec3::splat(10.),
            feather: 2.,
            profile: authored,
        }])
        .unwrap();
        let clear = scene.sample(Vec3::ZERO, false).unwrap();
        assert_eq!(clear, authored);
        let occluded = scene.sample(Vec3::ZERO, true).unwrap();
        assert_eq!(occluded.gain, authored.gain * 0.45);
        assert_eq!(occluded.low_pass_hz, 900.);
        let mut transition = AcousticTransition::new(clear).unwrap();
        transition.transition_to(occluded, 0.5).unwrap();
        let current = transition.advance(0.25).unwrap();
        assert!(current.gain > occluded.gain && current.gain < clear.gain);
        assert_eq!(current.low_pass_hz, 1700.);
        let before = transition;
        assert!(scene.sample(Vec3::splat(f32::NAN), false).is_none());
        assert_eq!(transition, before);
        assert_eq!(transition.advance(0.25).unwrap(), occluded);
        transition.transition_to(clear, 0.5).unwrap();
        assert_profile_near(transition.advance(0.25).unwrap(), current);
        assert_eq!(transition.advance(0.25).unwrap(), clear);

        // Existing authored voice controls are composed, never overwritten.
        let voice = VoiceParams {
            gain: 0.5,
            low_pass_hz: 500.,
            bus: super::super::Bus::Ambience,
            ramp_frames: 37,
            looping: true,
            priority: 3,
            pitch: 1.2,
            ..Default::default()
        };
        assert_eq!(
            current.apply(voice),
            VoiceParams {
                gain: current.gain * voice.gain,
                ..voice
            }
        );
        assert_eq!(AcousticProfile::default().apply(voice), voice);
    }

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
