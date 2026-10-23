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
    /// Initial/changed voice and spatial gain smoothing. Bus/master controls
    /// have independent timing. Zero gives sample-exact legacy playback.
    pub ramp_frames: u16,
    /// One-pole source filter; 24 kHz bypass preserves legacy sample output.
    pub low_pass_hz: f32,
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
            low_pass_hz: 24000.,
        }
    }
}
impl VoiceParams {
    fn valid(self) -> bool {
        self.low_pass_hz.is_finite()
            && (80. ..=24000.).contains(&self.low_pass_hz)
            && self.position.iter().all(|v| v.is_finite())
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
/// Opt-in reservation using the existing [`VoiceParams::priority`] scale.
///
/// Priorities at or above `critical_priority` are critical; lower priorities
/// are cosmetic. Cosmetic voices may occupy at most `voice_limit -
/// reserved_critical` slots, including voices fading out. Critical voices can
/// use every slot. Within either class, higher priorities still steal the
/// oldest voice of the lowest strictly lower priority; equal priorities drop.
///
/// The default policy reserves one slot for priorities 128..=255. A new
/// [`Mixer`] has **no policy** until [`Mixer::set_voice_policy`] enables one.
/// See that method for a compiling configuration example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoicePolicy {
    /// First critical priority, in 1..=255.
    pub critical_priority: u8,
    /// Number of slots cosmetic voices cannot occupy, in 0..=voice_limit.
    pub reserved_critical: usize,
}
impl Default for VoicePolicy {
    fn default() -> Self {
        Self {
            critical_priority: 128,
            reserved_critical: 1,
        }
    }
}
/// Why [`Mixer::start_with_outcome`] did not admit a voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceRejection {
    /// Invalid parameters; no voice, generation or statistic changed.
    InvalidParams,
    /// The cosmetic budget is full and no lower-priority cosmetic can be stolen.
    ReservedForCritical,
    /// All slots are occupied and none has a strictly lower priority.
    NoLowerPriority,
}
/// Explicit admission result, including the invalidated handle after a steal.
/// See [`Mixer::start_with_outcome`] for a compiling example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum VoiceAdmission {
    Started(VoiceHandle),
    Stolen {
        handle: VoiceHandle,
        replaced: VoiceHandle,
    },
    Rejected(VoiceRejection),
}
impl VoiceAdmission {
    /// The new handle on success. See [`Mixer::start_with_outcome`].
    pub fn handle(self) -> Option<VoiceHandle> {
        match self {
            Self::Started(handle) | Self::Stolen { handle, .. } => Some(handle),
            Self::Rejected(_) => None,
        }
    }
    /// The stale handle invalidated by a steal, otherwise `None`.
    /// See [`Mixer::start_with_outcome`].
    pub fn replaced(self) -> Option<VoiceHandle> {
        match self {
            Self::Stolen { replaced, .. } => Some(replaced),
            _ => None,
        }
    }
    /// The rejection reason, otherwise `None`. See [`Mixer::start_with_outcome`].
    pub fn rejection(self) -> Option<VoiceRejection> {
        match self {
            Self::Rejected(reason) => Some(reason),
            _ => None,
        }
    }
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
    filtered: [f32; 2],
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
/// A sample-timed linear envelope. Evaluating from its original endpoints keeps
/// rendering invariant to callback partitioning, without a per-sample buffer.
#[derive(Clone, Copy)]
struct Gain {
    start: f32,
    target: f32,
    frames: u64,
    elapsed: u64,
}
impl Gain {
    const fn new(value: f32) -> Self {
        Self {
            start: value,
            target: value,
            frames: 0,
            elapsed: 0,
        }
    }
    fn value_after(self, frames: u64) -> f32 {
        let elapsed = self.elapsed.saturating_add(frames);
        if elapsed >= self.frames {
            self.target
        } else {
            let fraction = elapsed as f64 / self.frames as f64;
            (f64::from(self.start) + (f64::from(self.target) - f64::from(self.start)) * fraction)
                as f32
        }
    }
    fn fading(self) -> bool {
        self.elapsed < self.frames
    }
    fn set(&mut self, value: f32, seconds: f64, rate: u32) -> bool {
        let frames = (seconds * f64::from(rate)).ceil();
        if !value.is_finite()
            || !(0. ..=1.).contains(&value)
            || !seconds.is_finite()
            || seconds < 0.
            || frames >= u64::MAX as f64
        {
            return false;
        }
        let current = self.value_after(0);
        *self = Self {
            start: current,
            target: value,
            frames: if current == value { 0 } else { frames as u64 },
            elapsed: 0,
        };
        true
    }
    fn cancel(&mut self) -> bool {
        let was_fading = self.fading();
        *self = Self::new(self.value_after(0));
        was_fading
    }
    fn advance(&mut self, frames: u64) {
        self.elapsed = self.elapsed.saturating_add(frames).min(self.frames);
    }
    fn resample(&mut self, old_rate: u32, new_rate: u32) -> bool {
        if !self.fading() || old_rate == new_rate {
            return true;
        }
        let remaining = u128::from(self.frames - self.elapsed);
        let frames = (remaining * u128::from(new_rate)).div_ceil(u128::from(old_rate));
        let Ok(frames) = u64::try_from(frames) else {
            return false;
        };
        self.start = self.value_after(0);
        self.frames = frames;
        self.elapsed = 0;
        true
    }
}
pub struct Mixer {
    sample_rate: u32,
    voices: Vec<Slot>,
    limit: usize,
    voice_policy: Option<VoicePolicy>,
    age: u64,
    pub listener: [f32; 3],
    pub right: [f32; 3],
    master: Gain,
    buses: [Gain; 4],
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
            voice_policy: None,
            age: 0,
            listener: [0.; 3],
            right: [1., 0., 0.],
            master: Gain::new(1.),
            buses: [Gain::new(1.); 4],
            stats: MixStats::default(),
        })
    }
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    /// Does not restart voices. Fractional source positions survive a device-rate change.
    /// Active gain fades keep their current value and remaining duration, rounded
    /// up to the next frame at the new rate. Returns false without changes for an
    /// unsupported rate or if a fade's remaining frame count would overflow u64.
    pub fn set_sample_rate(&mut self, rate: u32) -> bool {
        if !(8000..=96000).contains(&rate) {
            return false;
        }
        let mut master = self.master;
        let mut buses = self.buses;
        if !master.resample(self.sample_rate, rate)
            || buses
                .iter_mut()
                .any(|gain| !gain.resample(self.sample_rate, rate))
        {
            return false;
        }
        self.master = master;
        self.buses = buses;
        self.sample_rate = rate;
        true
    }
    /// Current master gain, including any rendered or skipped portion of a fade.
    pub fn master_gain(&self) -> f32 {
        self.master.value_after(0)
    }
    /// Set a finite gain in 0..=1 immediately, cancelling its fade. Invalid
    /// input leaves both the current gain and any active fade unchanged.
    pub fn set_master_gain(&mut self, gain: f32) -> bool {
        self.master.set(gain, 0., self.sample_rate)
    }
    /// Current bus gain, including any rendered or skipped portion of a fade.
    pub fn bus_gain(&self, bus: Bus) -> f32 {
        self.buses[bus as usize].value_after(0)
    }
    /// Set a finite gain in 0..=1 immediately, cancelling this bus's fade.
    /// Invalid input leaves the current gain and fade unchanged.
    pub fn set_bus_gain(&mut self, bus: Bus, gain: f32) -> bool {
        self.buses[bus as usize].set(gain, 0., self.sample_rate)
    }
    /// Fade linearly from the current master gain to `gain` over `seconds`.
    ///
    /// Retargeting starts from the current value without a jump. Each rendered
    /// or skipped output frame advances the fade once, even without voices.
    /// Duration rounds up to whole output frames; the last frame reaches the
    /// target exactly. Zero seconds applies immediately. Gain must be finite
    /// in 0..=1; duration must be finite, nonnegative and fit in a u64 frame
    /// count. Invalid input returns false without changing an existing fade.
    ///
    /// Fading to zero mutes exactly, independently of voice gain smoothing.
    /// Muted voices keep advancing; fade back to a saved gain to unmute.
    /// [`Self::suspend`] does not advance or cancel fades; [`Self::skip`] does
    /// advance them when a host stops rendering PCM.
    ///
    /// ```
    /// use ferravex_play::audio::Mixer;
    /// let mut mixer = Mixer::new(48_000, 16).unwrap();
    /// let resume_gain = mixer.master_gain();
    /// assert!(mixer.fade_master_gain(0.0, 0.01));
    /// mixer.render(&mut [[0.0; 2]; 480]);
    /// assert_eq!(mixer.master_gain(), 0.0);
    /// assert!(!mixer.is_master_fading());
    /// assert!(mixer.fade_master_gain(resume_gain, 0.01));
    /// ```
    pub fn fade_master_gain(&mut self, gain: f32, seconds: f64) -> bool {
        self.master.set(gain, seconds, self.sample_rate)
    }
    /// Fade one bus independently of the master and other buses. Validation,
    /// timing, retargeting and mute behavior match [`Self::fade_master_gain`].
    ///
    /// ```
    /// use ferravex_play::audio::{Bus, Mixer};
    /// let mut mixer = Mixer::new(48_000, 16).unwrap();
    /// assert!(mixer.fade_bus_gain(Bus::Music, 0.2, 0.25));
    /// mixer.skip(6_000);
    /// assert_eq!(mixer.bus_gain(Bus::Music), 0.6);
    /// // A new fade starts at 0.6, including when it reverses direction.
    /// assert!(mixer.fade_bus_gain(Bus::Music, 1.0, 0.25));
    /// assert_eq!(mixer.bus_gain(Bus::Effects), 1.0);
    /// ```
    pub fn fade_bus_gain(&mut self, bus: Bus, gain: f32, seconds: f64) -> bool {
        self.buses[bus as usize].set(gain, seconds, self.sample_rate)
    }
    /// Freeze the master at its current gain; return whether a fade was active.
    ///
    /// ```
    /// use ferravex_play::audio::Mixer;
    /// let mut mixer = Mixer::new(8_000, 1).unwrap();
    /// mixer.fade_master_gain(0.0, 1.0);
    /// mixer.skip(4_000);
    /// assert!(mixer.cancel_master_fade());
    /// mixer.skip(8_000);
    /// assert_eq!(mixer.master_gain(), 0.5);
    /// assert!(!mixer.is_master_fading());
    /// ```
    pub fn cancel_master_fade(&mut self) -> bool {
        self.master.cancel()
    }
    /// Freeze this bus at its current gain; return whether a fade was active.
    ///
    /// ```
    /// use ferravex_play::audio::{Bus, Mixer};
    /// let mut mixer = Mixer::new(8_000, 1).unwrap();
    /// mixer.fade_bus_gain(Bus::Music, 0.0, 1.0);
    /// mixer.skip(4_000);
    /// assert!(mixer.cancel_bus_fade(Bus::Music));
    /// assert!(!mixer.is_bus_fading(Bus::Music));
    /// assert!(!mixer.cancel_bus_fade(Bus::Music));
    /// ```
    pub fn cancel_bus_fade(&mut self, bus: Bus) -> bool {
        self.buses[bus as usize].cancel()
    }
    /// Whether the master fade still has output frames to advance.
    /// See [`Self::fade_master_gain`] for an example.
    pub fn is_master_fading(&self) -> bool {
        self.master.fading()
    }
    /// Whether this bus's fade still has output frames to advance.
    /// See [`Self::cancel_bus_fade`] for an example.
    pub fn is_bus_fading(&self, bus: Bus) -> bool {
        self.buses[bus as usize].fading()
    }
    pub fn stats(&self) -> MixStats {
        MixStats {
            active: self.active_voices(),
            ..self.stats
        }
    }
    /// Current opt-in policy; `None` preserves legacy priority-only admission.
    /// See [`Self::set_voice_policy`] for configuration and validation.
    pub fn voice_policy(&self) -> Option<VoicePolicy> {
        self.voice_policy
    }
    /// Enable, replace or disable critical-slot reservations without allocating.
    ///
    /// Returns false without changes if the critical threshold is zero, the
    /// reserve exceeds the voice limit, or existing cosmetic voices (including
    /// releases) exceed the proposed cosmetic budget. Stop those voices first
    /// or wait for them to finish. `None` disables the policy immediately.
    /// Successful changes preserve voices, handles, statistics and all fades.
    ///
    /// ```
    /// use ferravex_play::audio::Mixer;
    /// let mut mixer = Mixer::new(48_000, 16).unwrap();
    /// assert_eq!(mixer.voice_policy(), None);
    /// assert!(mixer.set_voice_policy(Some(Default::default())));
    /// let mut policy = mixer.voice_policy().unwrap();
    /// policy.critical_priority = 200;
    /// policy.reserved_critical = 4; // At most 12 cosmetic voices.
    /// assert!(mixer.set_voice_policy(Some(policy)));
    /// policy.reserved_critical = 17;
    /// assert!(!mixer.set_voice_policy(Some(policy)));
    /// assert_eq!(mixer.voice_policy().unwrap().reserved_critical, 4);
    /// assert!(mixer.set_voice_policy(None));
    /// ```
    pub fn set_voice_policy(&mut self, policy: Option<VoicePolicy>) -> bool {
        if let Some(policy) = policy
            && (policy.critical_priority == 0
                || policy.reserved_critical > self.limit
                || self.cosmetic_voices(policy) > self.limit - policy.reserved_critical)
        {
            return false;
        }
        self.voice_policy = policy;
        true
    }
    fn cosmetic_voices(&self, policy: VoicePolicy) -> usize {
        self.voices
            .iter()
            .filter(|slot| {
                slot.voice
                    .as_ref()
                    .is_some_and(|voice| voice.params.priority < policy.critical_priority)
            })
            .count()
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
    /// Honors any opt-in [`Self::set_voice_policy`] reservation. Use
    /// [`Self::start_with_outcome`] to distinguish rejection from stealing.
    pub fn start(&mut self, sound: &Sound, params: VoiceParams) -> Option<VoiceHandle> {
        self.start_with_outcome(sound, params).handle()
    }
    /// Admit a voice with an explicit result, scanning at most 128 fixed slots.
    ///
    /// A cosmetic cue at its budget can replace a lower-priority cosmetic even
    /// when reserved slots are empty. Critical cues first use any empty slot,
    /// then steal the oldest voice of the lowest strictly lower priority.
    /// Critical cues can still be rejected when all voices have equal or higher
    /// priority. A disabled policy uses the original priority-only behavior.
    ///
    /// Stealing immediately retires the replaced voice, including its release
    /// fade, and invalidates its handle. The new voice uses its own gain ramp;
    /// no overlapping tail or extra voice is allocated. Other voices and
    /// bus/master fades are unchanged. Rejections increment `stats().dropped`
    /// except for invalid parameters; steals increment `stats().stolen`.
    /// Keep a bank owner alive if the last sample Arc must not free on stealing.
    ///
    /// ```
    /// use ferravex_play::audio::{Clip, Mixer, Sound, VoiceParams};
    /// let sound = Sound::new(Clip::tone(8_000, 440.0, 0.1).unwrap()).unwrap();
    /// let mut mixer = Mixer::new(8_000, 2).unwrap();
    /// assert!(mixer.set_voice_policy(Some(Default::default())));
    /// let cosmetic = mixer.start_with_outcome(&sound, VoiceParams::default());
    /// let first = cosmetic.handle().unwrap();
    /// assert_eq!(cosmetic.replaced(), None);
    /// let denied = mixer.start_with_outcome(&sound, VoiceParams::default());
    /// assert!(denied.rejection().is_some()); // One slot is reserved.
    /// let critical = VoiceParams { priority: 128, ..Default::default() };
    /// assert!(mixer.start_with_outcome(&sound, critical).handle().is_some());
    /// let replacement = mixer.start_with_outcome(&sound, critical);
    /// assert_eq!(replacement.replaced(), Some(first));
    /// assert!(mixer.params(first).is_none());
    /// assert_eq!(mixer.stats().stolen, 1);
    /// ```
    pub fn start_with_outcome(&mut self, sound: &Sound, params: VoiceParams) -> VoiceAdmission {
        if !params.valid() {
            return VoiceAdmission::Rejected(VoiceRejection::InvalidParams);
        }
        let cosmetic_full = self.voice_policy.is_some_and(|policy| {
            policy.reserved_critical > 0
                && params.priority < policy.critical_priority
                && self.cosmetic_voices(policy) >= self.limit - policy.reserved_critical
        });
        let index = self
            .voices
            .iter()
            .position(|v| !cosmetic_full && v.voice.is_none())
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
            return VoiceAdmission::Rejected(if cosmetic_full {
                VoiceRejection::ReservedForCritical
            } else {
                VoiceRejection::NoLowerPriority
            });
        };
        let slot = &mut self.voices[index];
        let replaced = slot.voice.as_ref().map(|_| VoiceHandle {
            slot: index as u8,
            generation: slot.generation,
        });
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
            filtered: [0.; 2],
        });
        let handle = VoiceHandle {
            slot: index as u8,
            generation: slot.generation,
        };
        match replaced {
            Some(replaced) => VoiceAdmission::Stolen { handle, replaced },
            None => VoiceAdmission::Started(handle),
        }
    }
    pub fn params(&self, handle: VoiceHandle) -> Option<VoiceParams> {
        self.voices
            .get(handle.slot as usize)
            .filter(|s| s.generation == handle.generation)?
            .voice
            .as_ref()
            .map(|v| v.params)
    }
    /// Change a live voice's parameters. Invalid input or a stale handle leaves
    /// it unchanged. With a voice policy, demoting a critical voice to cosmetic
    /// also fails if the cosmetic budget is full; other parameter changes keep
    /// their existing behavior, including during release fades.
    pub fn set_params(&mut self, handle: VoiceHandle, params: VoiceParams) -> bool {
        if !params.valid() {
            return false;
        }
        if let Some(policy) = self.voice_policy
            && params.priority < policy.critical_priority
            && self
                .params(handle)
                .is_some_and(|old| old.priority >= policy.critical_priority)
            && self.cosmetic_voices(policy) >= self.limit - policy.reserved_critical
        {
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
                voice.params.gain * attenuation as f32
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
            let filter = if voice.params.low_pass_hz >= 24000. {
                1.
            } else {
                1. - (-std::f32::consts::TAU * voice.params.low_pass_hz / self.sample_rate as f32)
                    .exp()
            };
            let bus = self.buses[voice.params.bus as usize];
            for (frame, sample) in out.iter_mut().enumerate() {
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
                let mut source = std::array::from_fn::<_, 2, _>(|c| {
                    let a = voice.sound.0.samples[index][c];
                    let b = voice.sound.0.samples[next][c];
                    a + (b - a) * fraction
                });
                if filter == 1. {
                    voice.filtered = source;
                } else {
                    for (value, state) in source.iter_mut().zip(&mut voice.filtered) {
                        *state += (*value - *state) * filter;
                        *value = *state;
                    }
                }
                let mono = (source[0] + source[1]) * 0.5;
                let tail = if !voice.params.looping && voice.params.ramp_frames > 0 {
                    (((len as f64 - voice.cursor) / step) / f64::from(voice.params.ramp_frames))
                        .clamp(0., 1.) as f32
                } else {
                    1.
                };
                let bus_gain = bus.value_after(frame as u64 + 1);
                for (c, sample) in sample.iter_mut().enumerate() {
                    let value = if voice.params.spatial == Spatial::None {
                        source[c]
                    } else {
                        mono
                    };
                    *sample += value * voice.gains[c] * tail * bus_gain;
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
        for (index, frame) in out.iter_mut().enumerate() {
            let master_gain = self.master.value_after(index as u64 + 1);
            for value in frame {
                *value *= master_gain;
                *value = if value.is_finite() {
                    value.clamp(-1., 1.)
                } else {
                    0.
                };
            }
        }
        self.advance_gains(out.len() as u64);
        self.stats.rendered_frames = self.stats.rendered_frames.saturating_add(out.len() as u64);
    }
    /// Advance inaudible/suspended hosts in bounded O(voices), without generating PCM.
    /// Gain fades advance by the same output time, including when no voices exist.
    pub fn skip(&mut self, frames: u32) {
        self.advance_gains(u64::from(frames));
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
    fn advance_gains(&mut self, frames: u64) {
        self.master.advance(frames);
        for bus in &mut self.buses {
            bus.advance(frames);
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
    fn loop_voice(mixer: &mut Mixer, bus: Bus, ramp_frames: u16) -> VoiceHandle {
        mixer
            .start(
                &sound(),
                VoiceParams {
                    bus,
                    looping: true,
                    ramp_frames,
                    ..Default::default()
                },
            )
            .unwrap()
    }
    #[test]
    fn critical_reservations_apply_to_all_playback_entry_points() {
        let sound = sound();
        let mut m = Mixer::new(8000, 2).unwrap();
        assert!(m.set_voice_policy(Some(VoicePolicy::default())));
        let cosmetic = m.start(&sound, VoiceParams::default()).unwrap();
        assert!(!m.play(sound.clip().clone(), [0.; 3], 1., false));
        assert_eq!(
            m.start_with_outcome(&sound, VoiceParams::default()),
            VoiceAdmission::Rejected(VoiceRejection::ReservedForCritical)
        );
        assert_eq!(m.active_voices(), 1);
        let critical = VoiceParams {
            priority: 128,
            ..Default::default()
        };
        let first = m.start_with_outcome(&sound, critical);
        assert!(matches!(first, VoiceAdmission::Started(_)));
        let second = m.start_with_outcome(&sound, critical);
        assert_eq!(second.replaced(), Some(cosmetic));
        assert!(second.rejection().is_none());
        assert_ne!(second.handle(), Some(cosmetic));
        assert!(!m.stop(cosmetic, 0));
        assert!(!m.set_params(cosmetic, critical));
        assert_eq!(
            m.start_with_outcome(&sound, critical),
            VoiceAdmission::Rejected(VoiceRejection::NoLowerPriority)
        );
        assert_eq!(m.stats().stolen, 1);
        assert_eq!(m.stats().dropped, 3);
        assert_eq!(m.stats().active, 2);
    }
    #[test]
    fn cosmetic_replacement_leaves_reserved_slots_empty() {
        let sound = sound();
        let mut m = Mixer::new(8000, 2).unwrap();
        m.set_voice_policy(Some(VoicePolicy::default()));
        let first = m.start(&sound, VoiceParams::default()).unwrap();
        let params = VoiceParams {
            priority: 127,
            ..Default::default()
        };
        let replacement = m.start_with_outcome(&sound, params);
        assert_eq!(replacement.replaced(), Some(first));
        assert_eq!(m.active_voices(), 1);
        assert_eq!(
            m.start_with_outcome(&sound, params).rejection(),
            Some(VoiceRejection::ReservedForCritical)
        );
        assert!(m.params(replacement.handle().unwrap()).is_some());
        assert!(
            m.start(
                &sound,
                VoiceParams {
                    priority: 128,
                    ..params
                }
            )
            .is_some()
        );
        assert_eq!(m.active_voices(), 2);
    }
    #[test]
    fn policy_configuration_and_invalid_admission_are_transactional() {
        let sound = sound();
        let mut m = Mixer::new(8000, 2).unwrap();
        let first = loop_voice(&mut m, Bus::Music, 16);
        let second = m.start(&sound, VoiceParams::default()).unwrap();
        assert!(m.fade_master_gain(0., 0.001));
        assert!(m.fade_bus_gain(Bus::Music, 0., 0.001));
        let before = m.stats();
        for policy in [
            VoicePolicy::default(), // Existing cosmetic voices exceed the budget.
            VoicePolicy {
                critical_priority: 0,
                reserved_critical: 0,
            },
            VoicePolicy {
                critical_priority: 255,
                reserved_critical: usize::MAX,
            },
        ] {
            assert!(!m.set_voice_policy(Some(policy)));
            assert_eq!(m.voice_policy(), None);
        }
        assert_eq!(
            m.start_with_outcome(
                &sound,
                VoiceParams {
                    gain: f32::NAN,
                    ..Default::default()
                }
            ),
            VoiceAdmission::Rejected(VoiceRejection::InvalidParams)
        );
        assert_eq!(m.stats(), before);
        assert!(m.params(first).is_some());
        assert!(m.params(second).is_some());
        assert_eq!(m.master_gain(), 1.);
        assert_eq!(m.bus_gain(Bus::Music), 1.);
        assert!(m.stop(second, 0));
        assert!(m.set_voice_policy(Some(VoicePolicy::default())));
        assert!(!m.set_voice_policy(Some(VoicePolicy {
            reserved_critical: 2,
            ..Default::default()
        })));
        assert_eq!(m.voice_policy(), Some(VoicePolicy::default()));
        m.render(&mut [[0.; 2]; 8]);
        assert_eq!(m.master_gain(), 0.);
        assert_eq!(m.bus_gain(Bus::Music), 0.);
        assert!(m.params(first).is_some());
    }
    #[test]
    fn priority_changes_cannot_exceed_the_cosmetic_budget() {
        let sound = sound();
        let mut m = Mixer::new(8000, 2).unwrap();
        m.set_voice_policy(Some(VoicePolicy::default()));
        let cosmetic = VoiceParams::default();
        let critical = VoiceParams {
            priority: 128,
            ..cosmetic
        };
        let first = m.start(&sound, cosmetic).unwrap();
        let second = m.start(&sound, critical).unwrap();
        let stats = m.stats();
        assert!(!m.set_params(second, cosmetic));
        assert_eq!(m.params(second), Some(critical));
        assert_eq!(m.stats(), stats);
        assert!(m.set_params(
            second,
            VoiceParams {
                gain: 0.2,
                ..critical
            }
        ));
        assert!(m.set_params(first, critical)); // Promoting frees cosmetic capacity.
        assert!(m.set_params(second, cosmetic));
        assert!(!m.set_params(first, cosmetic));
        assert!(m.stop(second, 0));
        assert!(m.set_params(first, cosmetic));
    }
    #[test]
    fn critical_stealing_selects_lowest_priority_then_oldest_voice() {
        let sound = sound();
        let mut m = Mixer::new(8000, 4).unwrap();
        m.set_voice_policy(Some(VoicePolicy::default()));
        let params = VoiceParams {
            priority: 10,
            ..Default::default()
        };
        let reused = m.start(&sound, params).unwrap();
        let oldest = m.start(&sound, params).unwrap();
        let younger = m.start(&sound, params).unwrap();
        m.stop(reused, 0);
        let newest = m.start(&sound, params).unwrap();
        let critical = VoiceParams {
            priority: 128,
            ..params
        };
        m.start(&sound, critical).unwrap();
        assert_eq!(
            m.start_with_outcome(&sound, critical).replaced(),
            Some(oldest)
        );
        assert!(m.params(newest).is_some());
        assert!(m.set_params(
            newest,
            VoiceParams {
                priority: 1,
                ..params
            }
        ));
        assert_eq!(
            m.start_with_outcome(&sound, critical).replaced(),
            Some(newest)
        );
        assert!(m.params(younger).is_some());
    }
    #[test]
    fn releasing_and_retired_voices_update_reservations_without_extra_storage() {
        let sound = sound();
        let mut m = Mixer::new(8000, 2).unwrap();
        m.set_voice_policy(Some(VoicePolicy::default()));
        let params = VoiceParams {
            looping: true,
            ramp_frames: 0,
            ..Default::default()
        };
        let first = m.start(&sound, params).unwrap();
        m.render(&mut [[0.; 2]; 1]);
        m.stop(first, 8);
        assert!(m.start(&sound, params).is_none());
        let mut out = [[0.; 2]; 8];
        m.render(&mut out[..7]);
        assert!(m.start(&sound, params).is_none());
        m.render(&mut out[7..]);
        assert_eq!(out[0], [0.4375; 2]);
        assert_eq!(out[7], [0.; 2]);
        let next = m.start(&sound, params).unwrap();
        m.stop(next, 8);
        m.skip(8);
        assert!(m.start(&sound, VoiceParams::default()).is_some());
        m.suspend();
        assert!(m.start(&sound, params).is_some());
        m.stop_all();
        assert_eq!(m.active_voices(), 0);
        assert!(m.start(&sound, VoiceParams::default()).is_some());
        m.render(&mut [[0.; 2]; 800]);
        assert!(m.start(&sound, params).is_some());
    }
    #[test]
    fn policy_admission_preserves_fades_and_block_partitioning() {
        let sound = sound();
        let mut legacy = Mixer::new(8000, 2).unwrap();
        let mut policy = Mixer::new(8000, 2).unwrap();
        policy.set_voice_policy(Some(VoicePolicy::default()));
        let params = VoiceParams {
            priority: 128,
            looping: true,
            bus: Bus::Music,
            ramp_frames: 8,
            ..Default::default()
        };
        for m in [&mut legacy, &mut policy] {
            m.fade_master_gain(0.5, 0.002);
            m.fade_bus_gain(Bus::Music, 0., 0.002);
            let old = m
                .start(
                    &sound,
                    VoiceParams {
                        priority: 0,
                        ..params
                    },
                )
                .unwrap();
            m.start(&sound, params).unwrap();
            m.render(&mut [[0.; 2]; 4]);
            m.stop(old, 8);
            let replacement = m.start_with_outcome(&sound, params);
            assert_eq!(replacement.replaced(), Some(old));
            assert!(m.set_sample_rate(16000));
        }
        let mut whole = [[0.; 2]; 32];
        let mut pieces = whole;
        legacy.render(&mut whole);
        for part in pieces.chunks_mut(3) {
            policy.render(part);
        }
        assert_eq!(whole, pieces);
        assert_eq!(whole[24..], [[0.; 2]; 8]);
        assert_eq!(legacy.stats(), policy.stats());
        assert_eq!(policy.master_gain(), 0.5);
        assert_eq!(policy.bus_gain(Bus::Music), 0.);
    }
    #[test]
    fn disabled_and_zero_reservations_keep_legacy_admission() {
        let sound = sound();
        for enabled in [false, true] {
            let mut legacy = Mixer::new(8000, 3).unwrap();
            let mut m = Mixer::new(8000, 3).unwrap();
            m.set_voice_policy(Some(VoicePolicy {
                reserved_critical: 0,
                ..Default::default()
            }));
            if !enabled {
                assert!(m.set_voice_policy(None));
            }
            for priority in [0, 0, 1, 0, 1, 2, 127, 128, 255, 255, 255, 255] {
                let params = VoiceParams {
                    priority,
                    ..Default::default()
                };
                assert_eq!(
                    m.start_with_outcome(&sound, params),
                    legacy.start_with_outcome(&sound, params)
                );
            }
            assert_eq!(m.stats(), legacy.stats());
        }
    }
    #[test]
    fn all_slots_can_be_reserved_at_both_capacity_bounds() {
        let sound = sound();
        for limit in [1, 128] {
            let mut m = Mixer::new(8000, limit).unwrap();
            assert!(m.set_voice_policy(Some(VoicePolicy {
                critical_priority: 255,
                reserved_critical: limit
            })));
            let cosmetic = VoiceParams {
                priority: 254,
                ..Default::default()
            };
            assert_eq!(
                m.start_with_outcome(&sound, cosmetic).rejection(),
                Some(VoiceRejection::ReservedForCritical)
            );
            let critical = VoiceParams {
                priority: 255,
                ..cosmetic
            };
            for _ in 0..limit {
                assert!(m.start(&sound, critical).is_some());
            }
            assert_eq!(
                m.start_with_outcome(&sound, critical).rejection(),
                Some(VoiceRejection::NoLowerPriority)
            );
            assert_eq!(m.active_voices(), limit);
            assert_eq!(m.voices.len(), limit);
        }
    }
    #[test]
    fn bus_fades_retarget_from_current_gain_and_cancel_without_jumps() {
        let mut m = Mixer::new(8000, 1).unwrap();
        loop_voice(&mut m, Bus::Music, 0);
        assert!(m.fade_bus_gain(Bus::Music, 0., 0.001));
        let mut out = [[0.; 2]; 3];
        m.render(&mut out);
        assert_eq!(out, [[0.4375; 2], [0.375; 2], [0.3125; 2]]);
        assert_eq!(m.bus_gain(Bus::Music), 0.625);
        assert!(m.fade_bus_gain(Bus::Music, 1., 0.0005));
        assert_eq!(m.bus_gain(Bus::Music), 0.625);
        m.render(&mut out[..2]);
        assert_eq!(out[..2], [[0.359375; 2], [0.40625; 2]]);
        assert!(m.cancel_bus_fade(Bus::Music));
        assert!(!m.is_bus_fading(Bus::Music));
        assert!(!m.cancel_bus_fade(Bus::Music));
        m.render(&mut out);
        assert_eq!(out, [[0.40625; 2]; 3]);
        assert_eq!(m.bus_gain(Bus::Effects), 1.);
        assert_eq!(m.master_gain(), 1.);
    }
    #[test]
    fn master_fades_retarget_cancel_and_immediate_controls_override() {
        let mut m = Mixer::new(8000, 1).unwrap();
        loop_voice(&mut m, Bus::Music, 0);
        assert!(m.fade_master_gain(0., 0.001));
        let mut out = [[0.; 2]; 4];
        m.render(&mut out);
        assert_eq!(out[3], [0.25; 2]);
        assert!(m.fade_master_gain(1., 0.0005));
        assert_eq!(m.master_gain(), 0.5);
        m.render(&mut out[..2]);
        assert_eq!(out[..2], [[0.3125; 2], [0.375; 2]]);
        assert!(m.cancel_master_fade());
        assert!(!m.cancel_master_fade());
        m.render(&mut out);
        assert_eq!(out, [[0.375; 2]; 4]);

        assert!(m.fade_master_gain(0., 1.));
        assert!(m.fade_bus_gain(Bus::Music, 0., 1.));
        assert!(m.set_master_gain(0.5));
        assert!(m.set_bus_gain(Bus::Music, 0.5));
        assert!(!m.is_master_fading());
        assert!(!m.is_bus_fading(Bus::Music));
        m.render(&mut out);
        assert_eq!(out, [[0.125; 2]; 4]);
    }
    #[test]
    fn fades_validate_transactionally_and_accept_zero_or_subframe_duration() {
        let mut m = Mixer::new(8000, 1).unwrap();
        assert!(m.fade_master_gain(0., 0.001));
        assert!(m.fade_bus_gain(Bus::Ambience, 0., 0.001));
        m.skip(2);
        for gain in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.1, 1.1] {
            assert!(!m.fade_master_gain(gain, 0.001));
            assert!(!m.fade_bus_gain(Bus::Ambience, gain, 0.001));
            assert!(!m.set_master_gain(gain));
            assert!(!m.set_bus_gain(Bus::Ambience, gain));
        }
        for seconds in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, f64::MAX] {
            assert!(!m.fade_master_gain(1., seconds));
            assert!(!m.fade_bus_gain(Bus::Ambience, 1., seconds));
        }
        assert_eq!(m.master_gain(), 0.75);
        assert_eq!(m.bus_gain(Bus::Ambience), 0.75);
        assert!(m.is_master_fading());
        assert!(m.is_bus_fading(Bus::Ambience));
        m.skip(6);
        assert_eq!(m.master_gain(), 0.);
        assert_eq!(m.bus_gain(Bus::Ambience), 0.);
        assert!(!m.is_master_fading());
        assert!(!m.is_bus_fading(Bus::Ambience));

        assert!(m.fade_master_gain(0.5, 0.));
        assert!(m.fade_bus_gain(Bus::Ambience, 0.25, 0.));
        assert_eq!(m.master_gain(), 0.5);
        assert_eq!(m.bus_gain(Bus::Ambience), 0.25);
        assert!(!m.is_master_fading());
        assert!(!m.is_bus_fading(Bus::Ambience));
        assert!(m.fade_master_gain(1., f64::MIN_POSITIVE));
        m.render(&mut []);
        assert_eq!(m.master_gain(), 0.5);
        assert!(m.is_master_fading());
        m.skip(1);
        assert_eq!(m.master_gain(), 1.);
        assert!(!m.is_master_fading());
    }
    #[test]
    fn simultaneous_fades_share_one_clock_across_voices_and_callback_sizes() {
        let mut a = Mixer::new(8000, 3).unwrap();
        let mut b = Mixer::new(8000, 3).unwrap();
        for m in [&mut a, &mut b] {
            for bus in [Bus::Music, Bus::Music, Bus::Effects] {
                let handle = loop_voice(m, bus, 0);
                let mut params = m.params(handle).unwrap();
                params.gain = 0.25;
                assert!(m.set_params(handle, params));
            }
            assert!(m.fade_bus_gain(Bus::Music, 0., 0.008));
            assert!(m.fade_master_gain(0.5, 0.008));
        }
        let mut whole = [[0.; 2]; 100];
        let mut pieces = whole;
        a.render(&mut whole);
        for part in pieces.chunks_mut(13) {
            b.render(part);
        }
        assert_eq!(whole, pieces);
        for (i, frame) in whole.iter().enumerate() {
            let progress = ((i + 1) as f32 / 64.).min(1.);
            let expected = (0.25 * (1. - progress) + 0.125) * (1. - progress * 0.5);
            assert_eq!(*frame, [expected; 2]);
        }
        assert_eq!(a.bus_gain(Bus::Music), 0.);
        assert_eq!(a.master_gain(), 0.5);
    }
    #[test]
    fn fade_seconds_and_remaining_duration_survive_sample_rate_changes() {
        for rate in [8000, 16000, 44100, 48000, 96000] {
            let mut m = Mixer::new(rate, 1).unwrap();
            loop_voice(&mut m, Bus::Ui, 0);
            assert!(m.fade_master_gain(0., 0.02));
            assert!(m.fade_bus_gain(Bus::Ui, 0., 0.02));
            let half = rate / 100;
            let mut out = vec![[0.; 2]; half as usize];
            m.render(&mut out);
            assert_eq!(m.master_gain(), 0.5);
            assert_eq!(m.bus_gain(Bus::Ui), 0.5);
            assert_eq!(*out.last().unwrap(), [0.125; 2]);
            assert!(m.set_sample_rate(48000));
            assert_eq!(m.master_gain(), 0.5);
            assert_eq!(m.bus_gain(Bus::Ui), 0.5);
            m.render(&mut [[0.; 2]; 479]);
            assert!(m.is_master_fading());
            assert!(m.is_bus_fading(Bus::Ui));
            let mut last = [[1.; 2]; 1];
            m.render(&mut last);
            assert_eq!(last, [[0.; 2]]);
            assert_eq!(m.master_gain(), 0.);
            assert_eq!(m.bus_gain(Bus::Ui), 0.);
            assert!(!m.is_master_fading());
            assert!(!m.is_bus_fading(Bus::Ui));
        }
    }
    #[test]
    fn rate_changes_round_up_remaining_frames_and_reject_overflow_transactionally() {
        let mut m = Mixer::new(8000, 1).unwrap();
        assert!(m.fade_master_gain(0., 0.001));
        assert!(m.fade_bus_gain(Bus::Effects, 0., 0.001));
        m.skip(1);
        assert!(!m.set_sample_rate(0));
        assert_eq!(m.sample_rate(), 8000);
        assert!(m.set_sample_rate(12000));
        assert_eq!(m.master_gain(), 0.875);
        assert_eq!(m.bus_gain(Bus::Effects), 0.875);
        m.skip(10);
        assert!(m.is_master_fading());
        m.skip(1);
        assert_eq!(m.master_gain(), 0.);
        assert_eq!(m.bus_gain(Bus::Effects), 0.);

        // A representable duration at 8 kHz need not fit at 96 kHz. Even
        // an earlier successful master rescale must be rolled back on error.
        assert!(m.set_sample_rate(8000));
        assert!(m.fade_master_gain(1., 1.));
        assert!(m.fade_bus_gain(Bus::Music, 0., 1e15));
        assert!(!m.set_sample_rate(96000));
        assert_eq!(m.sample_rate(), 8000);
        m.skip(4000);
        assert_eq!(m.master_gain(), 0.5);
    }
    #[test]
    fn empty_and_suspended_mixing_and_skip_advance_the_same_fade_time() {
        let mut rendered = Mixer::new(8000, 1).unwrap();
        let mut skipped = Mixer::new(8000, 1).unwrap();
        for m in [&mut rendered, &mut skipped] {
            assert!(m.fade_master_gain(0., 0.001));
            assert!(m.fade_bus_gain(Bus::Music, 0., 0.001));
        }
        rendered.render(&mut [[0.; 2]; 3]);
        skipped.skip(3);
        assert_eq!(rendered.master_gain(), skipped.master_gain());
        assert_eq!(rendered.bus_gain(Bus::Music), skipped.bus_gain(Bus::Music));
        skipped.suspend();
        assert_eq!(skipped.master_gain(), 0.625);
        assert!(skipped.is_master_fading());
        rendered.render(&mut [[0.; 2]; 5]);
        skipped.skip(u32::MAX);
        assert_eq!(rendered.master_gain(), 0.);
        assert_eq!(skipped.master_gain(), 0.);
        assert_eq!(skipped.bus_gain(Bus::Music), 0.);
        assert!(!skipped.is_master_fading());
        assert!(!skipped.is_bus_fading(Bus::Music));
    }
    #[test]
    fn fades_mute_exactly_despite_voice_smoothing_and_keep_playback_time() {
        for master in [true, false] {
            let mut m = Mixer::new(8000, 2).unwrap();
            let handle = loop_voice(&mut m, Bus::Music, 128);
            let mut warmup = [[0.; 2]; 128];
            m.render(&mut warmup);
            assert_eq!(warmup[127], [0.5; 2]);
            if master {
                assert!(m.fade_master_gain(0., 0.001));
            } else {
                assert!(m.fade_bus_gain(Bus::Music, 0., 0.001));
            }
            let mut out = [[0.; 2]; 16];
            m.render(&mut out);
            for (i, frame) in out[..8].iter().enumerate() {
                assert_eq!(*frame, [0.5 * (1. - (i + 1) as f32 / 8.); 2]);
            }
            assert_eq!(out[8..], [[0.; 2]; 8]);
            let one_shot = m.start(&sound(), VoiceParams::default()).unwrap();
            // Music remains silent; the one-shot on Effects is also muted by
            // the master in that case, and still reaches its natural end.
            let mut silent = [[0.; 2]; 800];
            m.render(&mut silent);
            if master {
                assert_eq!(silent, [[0.; 2]; 800]);
            }
            assert!(m.params(one_shot).is_none());
            assert!(m.params(handle).is_some());
            assert_eq!(
                m.voices[handle.slot as usize]
                    .voice
                    .as_ref()
                    .unwrap()
                    .cursor,
                144.
            );
            if master {
                assert!(m.fade_master_gain(1., 0.001));
            } else {
                assert!(m.fade_bus_gain(Bus::Music, 1., 0.001));
            }
            m.render(&mut out[..8]);
            for (i, frame) in out[..8].iter().enumerate() {
                assert_eq!(*frame, [0.5 * (i + 1) as f32 / 8.; 2]);
            }
        }
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
