//! Confirmed-event presentation routing. Never changes health, ammo or the clock.
//!
//! Apply game rules first, then route their accepted result. A visual preference
//! never changes the returned sound cue or gain. Hosts remain responsible for
//! their own animation/particle playback and should honor [`FeedbackPolicy`]
//! there too; [`Reaction`] is a semantic tag, not permission to animate.
//! [`FeedbackController::advance_with_impact`] adds a decaying hit-emphasis
//! sample alongside the existing frame, without pausing any simulation clock.
//!
//! ```
//! use ferravex_math::Vec3;
//! use ferravex_play::feedback::{
//!     FeedbackConfig, FeedbackController, FeedbackEvent, FeedbackPolicy,
//! };
//!
//! # fn main() -> Result<(), ferravex_play::feedback::FeedbackError> {
//! let mut feedback = FeedbackController::<32>::new(FeedbackConfig::default(), 7)?;
//! feedback.set_policy(FeedbackPolicy::new(true, true, 0.5)?);
//! let mut ammo = 1;
//! let accepted = ammo > 0;
//! if accepted {
//!     ammo -= 1; // Gameplay is independent of presentation preferences.
//! }
//! let response = feedback.emit_if_accepted(
//!     1, accepted, FeedbackEvent::fire(Vec3::ZERO, 1.)?,
//! )?;
//! assert_eq!(ammo, 0);
//! assert_eq!(response.unwrap().gain, 0.55); // Sound is unchanged.
//! assert_eq!(feedback.advance(1. / 60.)?.shake.translation, Vec3::ZERO);
//! feedback.clear(); // Clear transient effects and IDs when restarting a run.
//! # Ok(())
//! # }
//! ```

use crate::{
    audio::{Cue, Surface},
    camera::{CameraShake, ShakeImpulse, ShakeSample},
};
use ferravex_math::Vec3;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackError {
    Invalid,
    Capacity,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackKind {
    Fire,
    Hit,
    Hurt,
    Land,
    Dash,
    Pickup,
    Explosion,
}
#[derive(Clone, Copy, Debug)]
pub struct FeedbackEvent {
    pub kind: FeedbackKind,
    pub intensity: f32,
    pub position: Vec3,
}
impl FeedbackEvent {
    /// Creates an event with finite position and intensity in `0..=4`.
    /// Intensity scales the authored event, including its sound; use
    /// [`FeedbackPolicy`] to reduce visuals without changing sound.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::feedback::{FeedbackEvent, FeedbackKind};
    /// let event = FeedbackEvent::new(FeedbackKind::Hit, Vec3::ZERO, 0.5).unwrap();
    /// assert_eq!(event.intensity, 0.5);
    /// assert!(FeedbackEvent::hurt(Vec3::ZERO, f32::NAN).is_err());
    /// ```
    pub fn new(kind: FeedbackKind, position: Vec3, intensity: f32) -> Result<Self, FeedbackError> {
        let event = Self {
            kind,
            intensity,
            position,
        };
        if !event.valid() {
            return Err(FeedbackError::Invalid);
        }
        Ok(event)
    }

    /// Checked feedback for a fired shot. Call after ammo/cooldown acceptance.
    pub fn fire(position: Vec3, intensity: f32) -> Result<Self, FeedbackError> {
        Self::new(FeedbackKind::Fire, position, intensity)
    }

    /// Checked feedback for a confirmed hit, rather than an attempted attack.
    pub fn hit(position: Vec3, intensity: f32) -> Result<Self, FeedbackError> {
        Self::new(FeedbackKind::Hit, position, intensity)
    }

    /// Checked feedback for damage actually received.
    pub fn hurt(position: Vec3, intensity: f32) -> Result<Self, FeedbackError> {
        Self::new(FeedbackKind::Hurt, position, intensity)
    }

    /// Checked feedback for the transition from airborne to grounded.
    pub fn land(position: Vec3, intensity: f32) -> Result<Self, FeedbackError> {
        Self::new(FeedbackKind::Land, position, intensity)
    }

    /// Checked feedback for a dash accepted by the movement rules.
    pub fn dash(position: Vec3, intensity: f32) -> Result<Self, FeedbackError> {
        Self::new(FeedbackKind::Dash, position, intensity)
    }

    /// Checked feedback for an item actually collected.
    pub fn pickup(position: Vec3, intensity: f32) -> Result<Self, FeedbackError> {
        Self::new(FeedbackKind::Pickup, position, intensity)
    }

    /// Checked feedback for a confirmed explosion.
    pub fn explosion(position: Vec3, intensity: f32) -> Result<Self, FeedbackError> {
        Self::new(FeedbackKind::Explosion, position, intensity)
    }

    fn valid(self) -> bool {
        self.intensity.is_finite()
            && (0. ..=4.).contains(&self.intensity)
            && self.position.is_finite()
    }
}

/// Validated visual preferences, independent of authored sound and rumble.
///
/// Reduced motion disables camera shake, particle requests, hit emphasis and
/// presentation hit-stop. Reduced flash disables screen flashes. `intensity`
/// scales the remaining visuals in `0..=1`, after authored event limits are applied, so
/// even an intensity-four event cannot override a preference. The default
/// preserves the original feedback. Hosts can read the same preferences to
/// suppress their own animations; this controller cannot recall particles or
/// reactions a host has already played.
///
/// ```
/// use ferravex_play::feedback::FeedbackPolicy;
/// let policy = FeedbackPolicy::new(true, true, 0.5).unwrap();
/// assert!(policy.reduced_motion());
/// assert!(policy.reduced_flash());
/// assert_eq!(policy.intensity(), 0.5);
/// assert!(FeedbackPolicy::new(false, false, f32::INFINITY).is_err());
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FeedbackPolicy {
    reduced_motion: bool,
    reduced_flash: bool,
    intensity: f32,
}
impl Default for FeedbackPolicy {
    fn default() -> Self {
        Self {
            reduced_motion: false,
            reduced_flash: false,
            intensity: 1.,
        }
    }
}
impl FeedbackPolicy {
    /// Rejects non-finite intensity and values outside `0..=1`.
    pub fn new(
        reduced_motion: bool,
        reduced_flash: bool,
        intensity: f32,
    ) -> Result<Self, FeedbackError> {
        if !intensity.is_finite() || !(0. ..=1.).contains(&intensity) {
            return Err(FeedbackError::Invalid);
        }
        Ok(Self {
            reduced_motion,
            reduced_flash,
            intensity,
        })
    }

    pub fn reduced_motion(self) -> bool {
        self.reduced_motion
    }

    pub fn reduced_flash(self) -> bool {
        self.reduced_flash
    }

    pub fn intensity(self) -> f32 {
        self.intensity
    }

    fn motion_scale(self) -> f32 {
        if self.reduced_motion {
            0.
        } else {
            self.intensity
        }
    }

    fn flash_scale(self) -> f32 {
        if self.reduced_flash {
            0.
        } else {
            self.intensity
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FeedbackConfig {
    pub motion: f32,
    pub flash: f32,
    pub rumble: f32,
    pub translation_scale: f32,
    pub single_player_hitstop: bool,
    pub networked: bool,
}
impl Default for FeedbackConfig {
    fn default() -> Self {
        Self {
            motion: 1.,
            flash: 1.,
            rumble: 1.,
            translation_scale: 0.03,
            single_player_hitstop: false,
            networked: false,
        }
    }
}
impl FeedbackConfig {
    fn valid(self) -> bool {
        [self.motion, self.flash, self.rumble]
            .iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(v))
            && self.translation_scale.is_finite()
            && (0. ..=10.).contains(&self.translation_scale)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reaction {
    Recoil,
    Impact,
    Stagger,
    Land,
    Dash,
    Collect,
}
#[derive(Clone, Copy, Debug)]
pub struct FeedbackResponse {
    pub cue: Cue,
    pub gain: f32,
    /// Already adjusted for the visual policy; zero under reduced motion.
    pub particles: u8,
    /// Semantic event tag retained under all policies. Hosts should use
    /// [`FeedbackController::policy`] when choosing their animation treatment.
    pub reaction: Reaction,
    pub position: Vec3,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct FeedbackFrame {
    pub shake: ShakeSample,
    pub flash: f32,
    pub rumble: [f32; 2],
    /// Optional single-player presentation pause. Never apply this to a gameplay
    /// clock. Online sessions and reduced-motion policies always return zero.
    pub hitstop_seconds: f32,
}

/// Normalized presentation envelopes, sampled before a presentation-time advance.
///
/// Both channels combine accepted events by taking the maximum, never summing,
/// and stay in `0..=1`. Emphasis decays exponentially at 24 per second, flash at
/// 18 per second. After a positive advance, stored levels below `0.0001` become
/// exactly zero, cutting off the exponential tail. Policy scaling is applied
/// afterward. Zero-delta sampling does not decay or retire either channel.
///
/// These are visual weights, not time scales or simulation delays. Hosts choose
/// the visual treatment; the controller does not render it. See
/// [`FeedbackController::advance_with_impact`] for paired frame sampling.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImpactSample {
    /// Hit emphasis for a scale/pose pulse. Hit, Hurt and Explosion have authored
    /// peaks of `0.3`, `0.7` and `1.0`, scaled by event intensity and config motion,
    /// capped at one, then scaled by policy. Other events do not raise this level.
    /// Reduced motion clears it; reduced flash alone does not affect it.
    pub emphasis: f32,
    /// The same screen-flash level returned in [`FeedbackFrame::flash`]. Reduced
    /// flash clears it; reduced motion alone does not affect it. Hosts should
    /// render this channel once, even when using both frame and impact samples.
    pub flash: f32,
}

fn decay_impact(level: f32, rate: f32, dt: f32) -> f32 {
    let next = level * (-rate * dt).exp();
    if dt > 0. && next < 0.0001 { 0. } else { next }
}

/// Allocation-free feedback with `1..=128` recent IDs and at most `N` accepted
/// events between successful [`advance`](Self::advance) calls. Camera shake has
/// its own bounded eight-impulse pool; its overflow never suppresses sound.
/// Impact emphasis and flash use two scalar envelopes with no per-event storage.
pub struct FeedbackController<const N: usize> {
    config: FeedbackConfig,
    policy: FeedbackPolicy,
    seen: [u64; N],
    next: usize,
    pending: usize,
    shake: CameraShake,
    emphasis: f32,
    flash: f32,
    rumble: [f32; 2],
    hitstop: f32,
    pub dropped: u64,
}
impl<const N: usize> FeedbackController<N> {
    pub fn new(config: FeedbackConfig, seed: u64) -> Result<Self, FeedbackError> {
        if N == 0 || N > 128 {
            return Err(FeedbackError::Capacity);
        }
        if !config.valid() {
            return Err(FeedbackError::Invalid);
        }
        Ok(Self {
            config,
            policy: FeedbackPolicy::default(),
            seen: [0; N],
            next: 0,
            pending: 0,
            shake: CameraShake::new(seed),
            emphasis: 0.,
            flash: 0.,
            rumble: [0.; 2],
            hitstop: 0.,
            dropped: 0,
        })
    }

    /// Current visual preferences, also usable by host animation adapters.
    pub fn policy(&self) -> FeedbackPolicy {
        self.policy
    }

    /// Applies validated preferences to pending as well as future frame output.
    /// Disabled channels are cleared, so re-enabling them cannot replay an old
    /// effect. Fractional intensity changes rescale pending frame output on the
    /// next `advance`. Does not consume IDs or alter sound/rumble/configuration.
    ///
    /// ```
    /// use ferravex_play::feedback::{FeedbackController, FeedbackPolicy};
    /// let mut feedback = FeedbackController::<8>::new(Default::default(), 1).unwrap();
    /// feedback.set_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
    /// assert!(feedback.policy().reduced_motion());
    /// ```
    pub fn set_policy(&mut self, policy: FeedbackPolicy) {
        self.policy = policy;
        if policy.motion_scale() == 0. {
            self.shake.clear();
            self.emphasis = 0.;
            self.hitstop = 0.;
        }
        if policy.flash_scale() == 0. {
            self.flash = 0.;
        }
    }

    /// Replaces valid configuration, clamping active emphasis to `config.motion`
    /// and active flash to `config.flash`. Zero clears the respective envelope;
    /// increasing a limit does not restore a cleared or clamped level. Invalid
    /// configuration leaves all state unchanged.
    pub fn configure(&mut self, config: FeedbackConfig) -> Result<(), FeedbackError> {
        if !config.valid() {
            return Err(FeedbackError::Invalid);
        }
        self.config = config;
        if config.motion == 0. {
            self.shake.clear();
            self.hitstop = 0.;
        }
        if config.networked || !config.single_player_hitstop {
            self.hitstop = 0.;
        }
        self.emphasis = self.emphasis.min(config.motion);
        self.flash = self.flash.min(config.flash);
        for r in &mut self.rumble {
            *r = (*r).min(config.rumble);
        }
        Ok(())
    }
    /// Clears all transient output, pending capacity and remembered IDs for a
    /// restart. Preserves configuration, visual preferences, the shake seed
    /// sequence and the cumulative `dropped` diagnostic. Previously used IDs
    /// can be reused after this call.
    pub fn clear(&mut self) {
        self.seen.fill(0);
        self.next = 0;
        self.pending = 0;
        self.shake.clear();
        self.emphasis = 0.;
        self.flash = 0.;
        self.rumble = [0.; 2];
        self.hitstop = 0.;
    }

    /// Emits only when game rules accepted the action. Rejected actions return
    /// `Ok(None)` without validating/reserving the ID, consuming burst capacity
    /// or changing output. Accepted actions have the same validation, duplicate
    /// suppression and return value as [`emit`](Self::emit).
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::feedback::{FeedbackController, FeedbackEvent};
    /// let mut feedback = FeedbackController::<1>::new(Default::default(), 1).unwrap();
    /// let shot = FeedbackEvent::fire(Vec3::ZERO, 1.).unwrap();
    /// assert!(feedback.emit_if_accepted(7, false, shot).unwrap().is_none());
    /// assert!(feedback.emit_if_accepted(7, true, shot).unwrap().is_some());
    /// assert!(feedback.emit_if_accepted(7, true, shot).unwrap().is_none());
    /// ```
    pub fn emit_if_accepted(
        &mut self,
        id: u64,
        accepted: bool,
        event: FeedbackEvent,
    ) -> Result<Option<FeedbackResponse>, FeedbackError> {
        if !accepted {
            return Ok(None);
        }
        self.emit(id, event)
    }

    /// IDs must be nonzero and unique in this controller's current session. The
    /// bounded N-event dedup horizon supplements, rather than replaces, netcode ACKs.
    pub fn emit(
        &mut self,
        id: u64,
        event: FeedbackEvent,
    ) -> Result<Option<FeedbackResponse>, FeedbackError> {
        if id == 0 || !event.valid() {
            return Err(FeedbackError::Invalid);
        }
        if self.seen.contains(&id) {
            return Ok(None);
        }
        if self.pending == N {
            self.dropped = self.dropped.saturating_add(1);
            return Err(FeedbackError::Capacity);
        }
        let (cue, gain, particles, reaction, motion, flash, rumble, stop) = match event.kind {
            FeedbackKind::Fire => (Cue::Fire, 0.55, 2, Reaction::Recoil, 0.2, 0., 0.2, 0.),
            FeedbackKind::Hit => (
                Cue::Impact(Surface::Metal),
                0.4,
                8,
                Reaction::Impact,
                0.3,
                0.,
                0.3,
                0.02,
            ),
            FeedbackKind::Hurt => (
                Cue::Impact(Surface::Metal),
                0.4,
                6,
                Reaction::Stagger,
                0.7,
                0.7,
                0.7,
                0.035,
            ),
            FeedbackKind::Land => (
                Cue::Impact(Surface::Stone),
                0.4,
                4,
                Reaction::Land,
                0.3,
                0.,
                0.3,
                0.,
            ),
            FeedbackKind::Dash => (Cue::Jump, 0.3, 5, Reaction::Dash, 0.15, 0., 0.2, 0.),
            FeedbackKind::Pickup => (Cue::Pickup, 0.5, 6, Reaction::Collect, 0., 0., 0.15, 0.),
            FeedbackKind::Explosion => (
                Cue::Impact(Surface::Stone),
                0.8,
                24,
                Reaction::Impact,
                1.,
                0.5,
                1.,
                0.04,
            ),
        };
        self.seen[self.next] = id;
        self.next = (self.next + 1) % N;
        self.pending += 1;
        let intensity = event.intensity;
        if self.config.motion > 0.
            && self.policy.motion_scale() > 0.
            && intensity > 0.
            && motion > 0.
            && self
                .shake
                .push(ShakeImpulse {
                    translation: Vec3::splat(
                        (motion * intensity * self.config.translation_scale * self.config.motion)
                            .min(10.),
                    ),
                    rotation: Vec3::ZERO,
                    duration: 0.15,
                    frequency: 24.,
                })
                .is_err()
        {
            self.dropped = self.dropped.saturating_add(1);
        }
        if matches!(
            event.kind,
            FeedbackKind::Hit | FeedbackKind::Hurt | FeedbackKind::Explosion
        ) && self.policy.motion_scale() > 0.
        {
            self.emphasis = self
                .emphasis
                .max((motion * intensity * self.config.motion).min(1.));
        }
        if self.policy.flash_scale() > 0. {
            self.flash = self
                .flash
                .max((flash * intensity * self.config.flash).min(1.));
        }
        self.rumble[0] = self.rumble[0].max((rumble * intensity * self.config.rumble).min(1.));
        self.rumble[1] =
            self.rumble[1].max((rumble * 0.6 * intensity * self.config.rumble).min(1.));
        if self.config.single_player_hitstop
            && !self.config.networked
            && self.config.motion > 0.
            && self.policy.motion_scale() > 0.
        {
            self.hitstop = self.hitstop.max((stop * intensity).min(0.06));
        }
        Ok(Some(FeedbackResponse {
            cue,
            gain: (gain * intensity).min(1.),
            particles: ((particles as f32 * intensity).min(64.) * self.policy.motion_scale()) as u8,
            reaction,
            position: event.position,
        }))
    }

    /// Reads the current impact envelopes with live visual preferences applied.
    /// Does not advance time, release burst capacity, consume IDs or replay cues.
    /// Disabling a channel clears its stored level, so re-enabling cannot revive
    /// an old impact. Fractional policy changes rescale the current level.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::feedback::{FeedbackController, FeedbackEvent, FeedbackPolicy};
    /// let mut feedback = FeedbackController::<8>::new(Default::default(), 1).unwrap();
    /// feedback.emit(1, FeedbackEvent::hurt(Vec3::ZERO, 1.).unwrap()).unwrap();
    /// assert_eq!(feedback.impact().emphasis, 0.7);
    /// feedback.set_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
    /// assert_eq!(feedback.impact().emphasis, 0.);
    /// assert_eq!(feedback.impact().flash, 0.7);
    /// ```
    pub fn impact(&self) -> ImpactSample {
        ImpactSample {
            emphasis: self.emphasis * self.policy.motion_scale(),
            flash: self.flash * self.policy.flash_scale(),
        }
    }

    /// Advances presentation once and returns a frame with its matching impact
    /// sample. Both scalar channels are sampled **before** decay, preserving the
    /// existing flash timing and showing newly accepted impacts at their peak.
    /// A subsequent [`impact`](Self::impact) reads the decayed state.
    ///
    /// Use this in place of [`advance`](Self::advance), not in addition to it.
    /// It accepts finite presentation deltas in `0..=0.25`; an invalid delta
    /// leaves all state unchanged. Neither method advances or pauses gameplay,
    /// networking or replay. Emphasis works in networked mode even though
    /// presentation hit-stop is disabled there.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::feedback::{FeedbackConfig, FeedbackController, FeedbackEvent};
    /// let config = FeedbackConfig { networked: true, ..Default::default() };
    /// let mut feedback = FeedbackController::<8>::new(config, 1).unwrap();
    /// feedback.emit(1, FeedbackEvent::hurt(Vec3::ZERO, 1.).unwrap()).unwrap();
    /// let (frame, impact) = feedback.advance_with_impact(1. / 60.).unwrap();
    /// assert_eq!(impact.emphasis, 0.7);
    /// assert_eq!(impact.flash, frame.flash);
    /// assert_eq!(frame.hitstop_seconds, 0.);
    /// assert!(feedback.impact().emphasis < impact.emphasis);
    /// ```
    pub fn advance_with_impact(
        &mut self,
        dt: f32,
    ) -> Result<(FeedbackFrame, ImpactSample), FeedbackError> {
        let impact = self.impact();
        self.advance(dt).map(|frame| (frame, impact))
    }

    /// Samples feedback and advances its presentation envelopes by finite `dt`
    /// in `0..=0.25` seconds. Flash, rumble and hit-stop are sampled before decay;
    /// camera shake is sampled after its advance. Invalid time leaves all state
    /// unchanged. Also decays hit emphasis when using this existing entry point.
    pub fn advance(&mut self, dt: f32) -> Result<FeedbackFrame, FeedbackError> {
        if !dt.is_finite() || !(0. ..=0.25).contains(&dt) {
            return Err(FeedbackError::Invalid);
        }
        let mut shake = self.shake.advance(dt).map_err(|_| FeedbackError::Invalid)?;
        shake.translation *= self.policy.motion_scale();
        shake.rotation *= self.policy.motion_scale();
        let frame = FeedbackFrame {
            shake,
            flash: self.flash * self.policy.flash_scale(),
            rumble: self.rumble,
            hitstop_seconds: self.hitstop * self.policy.motion_scale(),
        };
        self.emphasis = decay_impact(self.emphasis, 24., dt);
        self.flash = decay_impact(self.flash, 18., dt);
        for rumble in &mut self.rumble {
            *rumble *= (-24. * dt).exp();
        }
        self.hitstop = (self.hitstop - dt).max(0.);
        self.pending = 0;
        Ok(frame)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn event() -> FeedbackEvent {
        FeedbackEvent {
            kind: FeedbackKind::Hurt,
            intensity: 1.,
            position: Vec3::ZERO,
        }
    }
    #[test]
    fn confirmed_events_dedup_bounded_feedback_and_decay() {
        let mut f = FeedbackController::<2>::new(FeedbackConfig::default(), 1).unwrap();
        assert!(f.emit(1, event()).unwrap().is_some());
        assert!(f.emit(1, event()).unwrap().is_none());
        f.emit(2, event()).unwrap();
        assert!(f.emit(3, event()).is_err());
        let first = f.advance(0.1).unwrap();
        assert!(first.flash > 0.5 && first.rumble[0] > 0.5);
        let mut last = first;
        for _ in 0..20 {
            last = f.advance(0.1).unwrap();
        }
        assert!(last.flash < 0.001 && last.rumble[0] < 0.001);
        assert_eq!(last.shake.translation, Vec3::ZERO);
    }
    #[test]
    fn reduced_motion_and_online_mode_never_pause_simulation() {
        let config = FeedbackConfig {
            single_player_hitstop: true,
            networked: true,
            ..Default::default()
        };
        let mut f = FeedbackController::<8>::new(config, 3).unwrap();
        f.emit(1, event()).unwrap();
        assert_eq!(f.advance(0.).unwrap().hitstop_seconds, 0.);
        f.configure(FeedbackConfig {
            motion: 0.,
            flash: 0.,
            rumble: 0.,
            ..config
        })
        .unwrap();
        f.emit(2, event()).unwrap();
        let frame = f.advance(0.).unwrap();
        assert_eq!(frame.shake.translation, Vec3::ZERO);
        assert_eq!(frame.flash, 0.);
        assert_eq!(frame.rumble, [0.; 2]);
    }
    #[test]
    fn invalid_config_and_event_are_transactional() {
        let mut f = FeedbackController::<2>::new(FeedbackConfig::default(), 1).unwrap();
        let mut e = event();
        e.intensity = f32::NAN;
        assert!(f.emit(1, e).is_err());
        assert!(f.emit(1, event()).unwrap().is_some());
        let old = f.advance(0.).unwrap();
        assert!(f.advance(f32::NAN).is_err());
        assert_eq!(f.advance(0.).unwrap().flash, old.flash);
    }

    #[test]
    fn policies_and_event_helpers_reject_invalid_inputs() {
        for intensity in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 1.01] {
            assert_eq!(
                FeedbackPolicy::new(false, false, intensity).unwrap_err(),
                FeedbackError::Invalid
            );
        }
        for intensity in [0., 0.5, 1.] {
            let policy = FeedbackPolicy::new(true, false, intensity).unwrap();
            assert!(policy.reduced_motion());
            assert!(!policy.reduced_flash());
            assert_eq!(policy.intensity(), intensity);
        }
        let constructors = [
            (
                FeedbackKind::Fire,
                FeedbackEvent::fire as fn(Vec3, f32) -> _,
            ),
            (FeedbackKind::Hit, FeedbackEvent::hit),
            (FeedbackKind::Hurt, FeedbackEvent::hurt),
            (FeedbackKind::Land, FeedbackEvent::land),
            (FeedbackKind::Dash, FeedbackEvent::dash),
            (FeedbackKind::Pickup, FeedbackEvent::pickup),
            (FeedbackKind::Explosion, FeedbackEvent::explosion),
        ];
        for (kind, make) in constructors {
            for intensity in [0., 0.5, 1., 4.] {
                let event = make(Vec3::ONE, intensity).unwrap();
                assert_eq!(event.kind, kind);
                assert_eq!(event.intensity, intensity);
                assert_eq!(event.position, Vec3::ONE);
            }
            for intensity in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 4.01] {
                assert_eq!(
                    make(Vec3::ZERO, intensity).unwrap_err(),
                    FeedbackError::Invalid
                );
            }
            for position in [
                Vec3::new(f32::NAN, 0., 0.),
                Vec3::new(0., f32::INFINITY, 0.),
                Vec3::new(0., 0., f32::NEG_INFINITY),
            ] {
                assert_eq!(make(position, 1.).unwrap_err(), FeedbackError::Invalid);
            }
        }
    }

    #[test]
    fn default_policy_preserves_authored_responses_and_shake() {
        let cases = [
            (
                FeedbackKind::Fire,
                Cue::Fire,
                0.55,
                2,
                Reaction::Recoil,
                0.2,
                0.,
                0.2,
                0.,
            ),
            (
                FeedbackKind::Hit,
                Cue::Impact(Surface::Metal),
                0.4,
                8,
                Reaction::Impact,
                0.3,
                0.,
                0.3,
                0.02,
            ),
            (
                FeedbackKind::Hurt,
                Cue::Impact(Surface::Metal),
                0.4,
                6,
                Reaction::Stagger,
                0.7,
                0.7,
                0.7,
                0.035,
            ),
            (
                FeedbackKind::Land,
                Cue::Impact(Surface::Stone),
                0.4,
                4,
                Reaction::Land,
                0.3,
                0.,
                0.3,
                0.,
            ),
            (
                FeedbackKind::Dash,
                Cue::Jump,
                0.3,
                5,
                Reaction::Dash,
                0.15,
                0.,
                0.2,
                0.,
            ),
            (
                FeedbackKind::Pickup,
                Cue::Pickup,
                0.5,
                6,
                Reaction::Collect,
                0.,
                0.,
                0.15,
                0.,
            ),
            (
                FeedbackKind::Explosion,
                Cue::Impact(Surface::Stone),
                0.8,
                24,
                Reaction::Impact,
                1.,
                0.5,
                1.,
                0.04,
            ),
        ];
        for (kind, cue, gain, particles, reaction, motion, flash, rumble, stop) in cases {
            let mut f = FeedbackController::<8>::new(
                FeedbackConfig {
                    single_player_hitstop: true,
                    ..Default::default()
                },
                7,
            )
            .unwrap();
            let response = f
                .emit(1, FeedbackEvent::new(kind, Vec3::ONE, 1.).unwrap())
                .unwrap()
                .unwrap();
            assert_eq!(response.cue, cue);
            assert_eq!(response.gain, gain);
            assert_eq!(response.particles, particles);
            assert_eq!(response.reaction, reaction);
            assert_eq!(response.position, Vec3::ONE);
            let frame = f.advance(0.).unwrap();
            let mut expected_shake = CameraShake::new(7);
            if motion > 0. {
                expected_shake
                    .push(ShakeImpulse {
                        translation: Vec3::splat(motion * 0.03),
                        rotation: Vec3::ZERO,
                        duration: 0.15,
                        frequency: 24.,
                    })
                    .unwrap();
            }
            assert_eq!(frame.shake, expected_shake.advance(0.).unwrap());
            assert_eq!(frame.flash, flash);
            assert_eq!(frame.rumble, [rumble, rumble * 0.6]);
            assert_eq!(frame.hitstop_seconds, stop);
        }
    }

    #[test]
    fn visual_policies_preserve_audio_rumble_and_semantic_responses_for_every_kind() {
        for kind in [
            FeedbackKind::Fire,
            FeedbackKind::Hit,
            FeedbackKind::Hurt,
            FeedbackKind::Land,
            FeedbackKind::Dash,
            FeedbackKind::Pickup,
            FeedbackKind::Explosion,
        ] {
            for intensity in [0., 0.5, 1., 4.] {
                for (motion, flash, scale) in [
                    (true, false, 1.),
                    (false, true, 1.),
                    (true, true, 1.),
                    (false, false, 0.),
                    (false, false, 0.5),
                ] {
                    let config = FeedbackConfig {
                        single_player_hitstop: true,
                        ..Default::default()
                    };
                    let mut full = FeedbackController::<8>::new(config, 7).unwrap();
                    let mut reduced = FeedbackController::<8>::new(config, 7).unwrap();
                    reduced.set_policy(FeedbackPolicy::new(motion, flash, scale).unwrap());
                    let event = FeedbackEvent::new(kind, Vec3::ONE, intensity).unwrap();
                    let a = full.emit(1, event).unwrap().unwrap();
                    let b = reduced.emit(1, event).unwrap().unwrap();
                    assert_eq!(
                        (a.cue, a.gain, a.reaction, a.position),
                        (b.cue, b.gain, b.reaction, b.position)
                    );
                    assert!(b.particles <= a.particles);
                    let a = full.advance(0.).unwrap();
                    let b_frame = reduced.advance(0.).unwrap();
                    assert_eq!(a.rumble, b_frame.rumble);
                    if motion || scale == 0. {
                        assert_eq!(b.particles, 0);
                        assert_eq!(b_frame.shake, ShakeSample::default());
                        assert_eq!(b_frame.hitstop_seconds, 0.);
                    } else {
                        assert_eq!(b_frame.shake.translation, a.shake.translation * scale);
                        assert_eq!(b_frame.hitstop_seconds, a.hitstop_seconds * scale);
                    }
                    assert_eq!(b_frame.flash, if flash { 0. } else { a.flash * scale });
                }
            }
        }
    }

    #[test]
    fn fractional_policy_scales_after_event_caps_and_updates_pending_output() {
        let config = FeedbackConfig {
            single_player_hitstop: true,
            ..Default::default()
        };
        let mut full = FeedbackController::<8>::new(config, 7).unwrap();
        let mut reduced = FeedbackController::<8>::new(config, 7).unwrap();
        reduced.set_policy(FeedbackPolicy::new(false, false, 0.5).unwrap());
        let event = FeedbackEvent::explosion(Vec3::ZERO, 4.).unwrap();
        let a = full.emit(1, event).unwrap().unwrap();
        let b = reduced.emit(1, event).unwrap().unwrap();
        assert_eq!((a.particles, b.particles), (64, 32));
        assert_eq!((a.gain, b.gain), (1., 1.));
        let a = full.advance(0.).unwrap();
        let b = reduced.advance(0.).unwrap();
        assert_eq!((a.flash, b.flash), (1., 0.5));
        assert_eq!((a.hitstop_seconds, b.hitstop_seconds), (0.06, 0.03));
        reduced.set_policy(FeedbackPolicy::new(false, false, 0.25).unwrap());
        let b = reduced.advance(0.).unwrap();
        assert_eq!(b.flash, 0.25);
        assert_eq!(b.shake.translation, a.shake.translation * 0.25);
        assert_eq!(b.hitstop_seconds, 0.015);
    }

    #[test]
    fn turning_visuals_off_clears_pending_effects_without_replay() {
        for (motion, flash, scale) in [(true, false, 1.), (false, true, 1.), (false, false, 0.)] {
            let config = FeedbackConfig {
                single_player_hitstop: true,
                ..Default::default()
            };
            let mut f = FeedbackController::<8>::new(config, 7).unwrap();
            f.emit(1, event()).unwrap();
            let before = f.advance(0.).unwrap();
            f.set_policy(FeedbackPolicy::new(motion, flash, scale).unwrap());
            f.emit(2, event()).unwrap();
            let disabled = f.advance(0.).unwrap();
            f.set_policy(FeedbackPolicy::default());
            let enabled = f.advance(0.).unwrap();
            if motion || scale == 0. {
                assert_eq!(disabled.shake, ShakeSample::default());
                assert_eq!(enabled.shake, ShakeSample::default());
                assert_eq!(enabled.hitstop_seconds, 0.);
            }
            if flash || scale == 0. {
                assert_eq!(disabled.flash, 0.);
                assert_eq!(enabled.flash, 0.);
            }
            assert_eq!(enabled.rumble, before.rumble);
            f.emit(3, event()).unwrap();
            let fresh = f.advance(0.).unwrap();
            assert!(fresh.shake.translation.length_squared() > 0.);
            assert!(fresh.flash > 0.);
        }
    }

    #[test]
    fn rejected_actions_do_not_reserve_ids_or_capacity() {
        let mut f = FeedbackController::<1>::new(FeedbackConfig::default(), 7).unwrap();
        let invalid = FeedbackEvent {
            intensity: f32::NAN,
            ..event()
        };
        assert!(f.emit_if_accepted(0, false, invalid).unwrap().is_none());
        assert!(f.emit_if_accepted(1, false, event()).unwrap().is_none());
        assert_eq!(f.pending, 0);
        assert_eq!(f.dropped, 0);
        let frame = f.advance(0.).unwrap();
        assert_eq!(frame.shake, ShakeSample::default());
        assert_eq!(frame.flash, 0.);
        assert_eq!(frame.rumble, [0.; 2]);
        assert!(f.emit_if_accepted(1, true, event()).unwrap().is_some());
        assert!(f.emit_if_accepted(1, true, event()).unwrap().is_none());
        assert_eq!(
            f.emit_if_accepted(2, true, event()).unwrap_err(),
            FeedbackError::Capacity
        );
        assert_eq!(f.dropped, 1);
    }

    #[test]
    fn duplicate_and_overflow_events_leave_feedback_unchanged_and_can_be_retried() {
        let mut f = FeedbackController::<2>::new(FeedbackConfig::default(), 7).unwrap();
        assert_eq!(f.emit(0, event()).unwrap_err(), FeedbackError::Invalid);
        f.emit(u64::MAX, event()).unwrap();
        let shake = f.shake;
        let intense = FeedbackEvent::explosion(Vec3::ZERO, 4.).unwrap();
        assert!(f.emit(u64::MAX, intense).unwrap().is_none());
        assert_eq!(f.shake, shake);
        assert_eq!(f.pending, 1);
        assert_eq!(f.flash, 0.7);
        f.emit(2, event()).unwrap();
        let shake = f.shake;
        assert!(f.emit(2, intense).unwrap().is_none());
        assert_eq!(f.emit(3, intense).unwrap_err(), FeedbackError::Capacity);
        assert_eq!(f.shake, shake);
        assert_eq!(f.flash, 0.7);
        assert_eq!(f.dropped, 1);
        f.advance(0.25).unwrap();
        assert!(f.emit(3, event()).unwrap().is_some());
        assert!(f.emit(2, event()).unwrap().is_none());
        // The first ID is now outside the documented N-event history.
        assert!(f.emit(u64::MAX, event()).unwrap().is_some());
    }

    #[test]
    fn restart_clear_preserves_preferences_but_releases_all_transient_state() {
        let config = FeedbackConfig {
            single_player_hitstop: true,
            ..Default::default()
        };
        let mut f = FeedbackController::<1>::new(config, 7).unwrap();
        let policy = FeedbackPolicy::new(false, false, 0.5).unwrap();
        f.set_policy(policy);
        f.emit(1, event()).unwrap();
        assert_eq!(f.emit(2, event()).unwrap_err(), FeedbackError::Capacity);
        f.clear();
        assert_eq!(f.policy(), policy);
        assert_eq!(f.dropped, 1);
        assert_eq!(f.pending, 0);
        let frame = f.advance(0.).unwrap();
        assert_eq!(frame.shake, ShakeSample::default());
        assert_eq!(frame.flash, 0.);
        assert_eq!(frame.rumble, [0.; 2]);
        assert_eq!(frame.hitstop_seconds, 0.);
        assert!(f.emit(1, event()).unwrap().is_some());
        assert!(f.advance(0.).unwrap().hitstop_seconds > 0.);
    }

    #[test]
    fn configuration_is_transactional_and_cannot_override_policy() {
        let mut f = FeedbackController::<8>::new(FeedbackConfig::default(), 7).unwrap();
        f.emit(1, event()).unwrap();
        let before = f.advance(0.).unwrap();
        assert_eq!(
            f.configure(FeedbackConfig {
                motion: f32::NAN,
                flash: 0.,
                ..Default::default()
            }),
            Err(FeedbackError::Invalid)
        );
        let after = f.advance(0.).unwrap();
        assert_eq!(after.shake, before.shake);
        assert_eq!(after.flash, before.flash);
        let policy = FeedbackPolicy::new(true, true, 1.).unwrap();
        f.set_policy(policy);
        f.configure(FeedbackConfig {
            single_player_hitstop: true,
            ..Default::default()
        })
        .unwrap();
        let response = f.emit(2, event()).unwrap().unwrap();
        let after = f.advance(0.).unwrap();
        assert_eq!(f.policy(), policy);
        assert_eq!(response.gain, 0.4);
        assert_eq!(response.particles, 0);
        assert_eq!(after.shake, ShakeSample::default());
        assert_eq!(after.flash, 0.);
        assert_eq!(after.hitstop_seconds, 0.);
    }

    #[test]
    fn silent_events_do_not_exhaust_shake_pool_and_visual_overflow_preserves_audio() {
        let mut f = FeedbackController::<32>::new(FeedbackConfig::default(), 7).unwrap();
        let silent = FeedbackEvent::hurt(Vec3::ZERO, 0.).unwrap();
        for id in 1..=8 {
            let response = f.emit(id, silent).unwrap().unwrap();
            assert_eq!(response.gain, 0.);
            assert_eq!(response.particles, 0);
        }
        assert_eq!(f.shake.active(), 0);
        for id in 9..=17 {
            assert_eq!(f.emit(id, event()).unwrap().unwrap().gain, 0.4);
        }
        assert_eq!(f.shake.active(), 8);
        assert_eq!(f.dropped, 1);
        assert!(f.emit(17, event()).unwrap().is_none());
        assert_eq!(f.dropped, 1);
    }

    #[test]
    fn capacities_are_bounded_and_invalid_time_does_not_release_pending_events() {
        assert_eq!(
            FeedbackController::<0>::new(Default::default(), 1).err(),
            Some(FeedbackError::Capacity)
        );
        assert_eq!(
            FeedbackController::<129>::new(Default::default(), 1).err(),
            Some(FeedbackError::Capacity)
        );
        let mut f = FeedbackController::<128>::new(Default::default(), 1).unwrap();
        f.set_policy(FeedbackPolicy::new(true, true, 1.).unwrap());
        for id in 1..=128 {
            assert!(f.emit(id, event()).unwrap().is_some());
        }
        assert!(f.advance(f32::NAN).is_err());
        assert_eq!(f.emit(129, event()).unwrap_err(), FeedbackError::Capacity);
        assert_eq!(f.seen.len(), 128);
        f.advance(0.).unwrap();
        assert!(f.emit(129, event()).unwrap().is_some());
    }

    #[test]
    fn impact_samples_pair_with_frames_decay_monotonically_and_settle_exactly() {
        let mut f = FeedbackController::<8>::new(Default::default(), 7).unwrap();
        assert_eq!(f.impact(), ImpactSample::default());
        let event = FeedbackEvent::explosion(Vec3::ZERO, 4.).unwrap();
        f.emit(1, event).unwrap();
        let peak = ImpactSample {
            emphasis: 1.,
            flash: 1.,
        };
        assert_eq!(f.impact(), peak);
        assert_eq!(f.advance_with_impact(0.).unwrap().1, peak);
        assert_eq!(f.impact(), peak);
        let mut previous = peak;
        for _ in 0..6 {
            let (frame, sample) = f.advance_with_impact(0.1).unwrap();
            assert_eq!(sample, previous);
            assert_eq!(frame.flash, sample.flash);
            let current = f.impact();
            assert!((0. ..=previous.emphasis).contains(&current.emphasis));
            assert!((0. ..=previous.flash).contains(&current.flash));
            previous = current;
        }
        assert_eq!(f.impact(), ImpactSample::default());
        assert_eq!(
            f.advance_with_impact(0.).unwrap().1,
            ImpactSample::default()
        );
        // Expiring the presentation envelope does not forget an accepted ID.
        assert!(f.emit(1, event).unwrap().is_none());
        assert_eq!(f.impact(), ImpactSample::default());
    }

    #[test]
    fn only_combat_events_raise_emphasis_and_authored_strength_is_capped() {
        for (kind, emphasis, flash) in [
            (FeedbackKind::Fire, 0., 0.),
            (FeedbackKind::Hit, 0.3, 0.),
            (FeedbackKind::Hurt, 0.7, 0.7),
            (FeedbackKind::Land, 0., 0.),
            (FeedbackKind::Dash, 0., 0.),
            (FeedbackKind::Pickup, 0., 0.),
            (FeedbackKind::Explosion, 1., 0.5),
        ] {
            for intensity in [0., 0.5, 1., 4.] {
                let config = FeedbackConfig {
                    motion: 0.5,
                    flash: 0.75,
                    ..Default::default()
                };
                let mut f = FeedbackController::<8>::new(config, 7).unwrap();
                f.emit(1, FeedbackEvent::new(kind, Vec3::ZERO, intensity).unwrap())
                    .unwrap();
                assert_eq!(
                    f.impact(),
                    ImpactSample {
                        emphasis: (emphasis * intensity * config.motion).min(1.),
                        flash: (flash * intensity * config.flash).min(1.),
                    }
                );
            }
        }
    }

    #[test]
    fn impact_policies_are_independent_and_preserve_sound_and_haptics_online() {
        let config = FeedbackConfig {
            single_player_hitstop: true,
            networked: true,
            ..Default::default()
        };
        for motion in [false, true] {
            for flash in [false, true] {
                for scale in [0., 0.25, 1.] {
                    let mut f = FeedbackController::<8>::new(config, 7).unwrap();
                    f.set_policy(FeedbackPolicy::new(motion, flash, scale).unwrap());
                    let event = FeedbackEvent::explosion(Vec3::ZERO, 4.).unwrap();
                    let response = f.emit(1, event).unwrap().unwrap();
                    assert_eq!(response.cue, Cue::Impact(Surface::Stone));
                    assert_eq!(response.gain, 1.);
                    for dt in [0., 0.125, 0.25] {
                        let (frame, impact) = f.advance_with_impact(dt).unwrap();
                        assert_eq!(impact.flash, frame.flash);
                        assert_eq!(frame.hitstop_seconds, 0.);
                        if motion {
                            assert_eq!(impact.emphasis, 0.);
                        }
                        if flash {
                            assert_eq!(impact.flash, 0.);
                        }
                        assert!((0. ..=scale).contains(&impact.emphasis));
                        assert!((0. ..=scale).contains(&impact.flash));
                    }
                    // Compare the same presentation schedule under full visuals.
                    let mut full = FeedbackController::<8>::new(config, 7).unwrap();
                    full.emit(1, event).unwrap();
                    for dt in [0., 0.125, 0.25] {
                        full.advance(dt).unwrap();
                    }
                    assert_eq!(f.rumble, full.rumble);
                    assert_eq!(
                        f.impact().emphasis,
                        if motion {
                            0.
                        } else {
                            full.impact().emphasis * scale
                        }
                    );
                    assert_eq!(
                        f.impact().flash,
                        if flash {
                            0.
                        } else {
                            full.impact().flash * scale
                        }
                    );
                }
            }
        }
    }

    #[test]
    fn impact_policy_changes_rescale_live_levels_and_disabled_channels_never_revive() {
        for (motion, flash, scale) in [
            (true, false, 1.),
            (false, true, 1.),
            (true, true, 1.),
            (false, false, 0.),
        ] {
            let mut f = FeedbackController::<8>::new(Default::default(), 7).unwrap();
            let event = FeedbackEvent::explosion(Vec3::ZERO, 4.).unwrap();
            f.emit(1, event).unwrap();
            f.set_policy(FeedbackPolicy::new(false, false, 0.5).unwrap());
            assert_eq!(
                f.impact(),
                ImpactSample {
                    emphasis: 0.5,
                    flash: 0.5
                }
            );
            f.set_policy(FeedbackPolicy::new(false, false, 0.25).unwrap());
            assert_eq!(
                f.impact(),
                ImpactSample {
                    emphasis: 0.25,
                    flash: 0.25
                }
            );
            f.set_policy(FeedbackPolicy::new(motion, flash, scale).unwrap());
            f.emit(2, event).unwrap();
            let disabled = f.impact();
            f.set_policy(FeedbackPolicy::default());
            let restored = f.impact();
            assert_eq!(disabled, restored);
            assert_eq!(
                restored.emphasis,
                if motion || scale == 0. { 0. } else { 1. }
            );
            assert_eq!(restored.flash, if flash || scale == 0. { 0. } else { 1. });
            f.emit(3, event).unwrap();
            assert_eq!(
                f.impact(),
                ImpactSample {
                    emphasis: 1.,
                    flash: 1.
                }
            );
        }
    }

    #[test]
    fn impact_sampling_rejections_duplicates_and_overflow_do_not_raise_or_refresh_levels() {
        let mut f = FeedbackController::<2>::new(Default::default(), 7).unwrap();
        let strong = FeedbackEvent::explosion(Vec3::ZERO, 4.).unwrap();
        f.emit(1, event()).unwrap();
        f.advance(0.125).unwrap();
        let before = f.impact();
        assert!(f.emit_if_accepted(2, false, strong).unwrap().is_none());
        assert!(f.emit(1, strong).unwrap().is_none());
        assert_eq!(f.impact(), before);
        // Accepted weak and zero-intensity impulses do not sum or refresh decay.
        f.emit(2, FeedbackEvent::hurt(Vec3::ZERO, 0.001).unwrap())
            .unwrap();
        f.emit(3, FeedbackEvent::hurt(Vec3::ZERO, 0.).unwrap())
            .unwrap();
        assert_eq!(f.impact(), before);
        assert_eq!(f.pending, 2);
        assert_eq!(f.emit(4, strong).unwrap_err(), FeedbackError::Capacity);
        assert_eq!(f.impact(), before);
        assert_eq!(f.dropped, 1);
        f.advance(0.125).unwrap();
        assert!(f.impact().emphasis < before.emphasis);
        assert!(f.emit(4, strong).unwrap().is_some());
        assert_eq!(
            f.impact(),
            ImpactSample {
                emphasis: 1.,
                flash: 1.
            }
        );
    }

    #[test]
    fn impact_errors_are_transactional_and_configuration_and_restart_clear_levels() {
        let mut f = FeedbackController::<1>::new(Default::default(), 7).unwrap();
        f.emit(1, event()).unwrap();
        let before = f.impact();
        for dt in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.01, 0.251] {
            assert_eq!(
                f.advance_with_impact(dt).unwrap_err(),
                FeedbackError::Invalid
            );
            assert_eq!(f.impact(), before);
            assert_eq!(f.pending, 1);
        }
        for invalid in [
            FeedbackEvent {
                intensity: f32::NAN,
                ..event()
            },
            FeedbackEvent {
                position: Vec3::splat(f32::INFINITY),
                ..event()
            },
        ] {
            assert_eq!(f.emit(2, invalid).unwrap_err(), FeedbackError::Invalid);
            assert_eq!(f.impact(), before);
        }
        assert_eq!(
            f.configure(FeedbackConfig {
                flash: f32::NAN,
                ..Default::default()
            }),
            Err(FeedbackError::Invalid)
        );
        assert_eq!(f.impact(), before);
        f.configure(FeedbackConfig {
            motion: 0.2,
            flash: 0.3,
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            f.impact(),
            ImpactSample {
                emphasis: 0.2,
                flash: 0.3
            }
        );
        f.configure(FeedbackConfig {
            motion: 0.,
            flash: 0.,
            ..Default::default()
        })
        .unwrap();
        f.configure(FeedbackConfig::default()).unwrap();
        assert_eq!(f.impact(), ImpactSample::default());
        assert!(f.emit(1, event()).unwrap().is_none());
        f.clear();
        assert!(f.emit(1, event()).unwrap().is_some());
        f.clear();
        assert_eq!(f.impact(), ImpactSample::default());
        assert_eq!(f.pending, 0);
    }

    #[test]
    fn existing_and_paired_advance_share_one_decay_and_preserve_frame_outputs() {
        let config = FeedbackConfig {
            single_player_hitstop: true,
            ..Default::default()
        };
        let mut original = FeedbackController::<8>::new(config, 7).unwrap();
        let mut paired = FeedbackController::<8>::new(config, 7).unwrap();
        original.emit(1, event()).unwrap();
        paired.emit(1, event()).unwrap();
        for dt in [0., 0.0625, 0.125, 0.25, 0.25] {
            let before = original.impact();
            let a = original.advance(dt).unwrap();
            let (b, impact) = paired.advance_with_impact(dt).unwrap();
            assert_eq!(before, impact);
            assert_eq!(a.shake, b.shake);
            assert_eq!(a.flash, b.flash);
            assert_eq!(a.rumble, b.rumble);
            assert_eq!(a.hitstop_seconds, b.hitstop_seconds);
            assert_eq!(original.impact(), paired.impact());
        }
    }

    #[test]
    fn impact_decay_is_consistent_across_frame_partitions_and_tiny_levels_do_not_linger() {
        let mut whole = FeedbackController::<8>::new(Default::default(), 7).unwrap();
        let mut split = FeedbackController::<8>::new(Default::default(), 7).unwrap();
        whole.emit(1, event()).unwrap();
        split.emit(1, event()).unwrap();
        whole.advance(0.125).unwrap();
        for _ in 0..4 {
            split.advance(0.03125).unwrap();
        }
        assert!((whole.impact().emphasis - split.impact().emphasis).abs() < 0.000001);
        assert!((whole.impact().flash - split.impact().flash).abs() < 0.000001);
        let mut tiny = FeedbackController::<8>::new(Default::default(), 7).unwrap();
        tiny.emit(1, FeedbackEvent::hurt(Vec3::ZERO, 0.00001).unwrap())
            .unwrap();
        let before = tiny.impact();
        assert!(before.emphasis > 0. && before.flash > 0.);
        tiny.advance(0.).unwrap();
        assert_eq!(tiny.impact(), before);
        tiny.advance(0.001).unwrap();
        assert_eq!(tiny.impact(), ImpactSample::default());
    }
}
