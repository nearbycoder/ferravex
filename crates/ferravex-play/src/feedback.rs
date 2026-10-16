//! Confirmed-event presentation routing. Never changes health, ammo or the clock.
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
#[derive(Clone, Copy, Debug)]
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
#[derive(Clone, Copy, Debug)]
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
    pub particles: u8,
    pub reaction: Reaction,
    pub position: Vec3,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct FeedbackFrame {
    pub shake: ShakeSample,
    pub flash: f32,
    pub rumble: [f32; 2],
    /// Optional single-player presentation pause. Online sessions always return zero.
    pub hitstop_seconds: f32,
}
pub struct FeedbackController<const N: usize> {
    config: FeedbackConfig,
    seen: [u64; N],
    next: usize,
    pending: usize,
    shake: CameraShake,
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
            seen: [0; N],
            next: 0,
            pending: 0,
            shake: CameraShake::new(seed),
            flash: 0.,
            rumble: [0.; 2],
            hitstop: 0.,
            dropped: 0,
        })
    }
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
        self.flash = self.flash.min(config.flash);
        for r in &mut self.rumble {
            *r = (*r).min(config.rumble);
        }
        Ok(())
    }
    pub fn clear(&mut self) {
        self.seen.fill(0);
        self.next = 0;
        self.pending = 0;
        self.shake.clear();
        self.flash = 0.;
        self.rumble = [0.; 2];
        self.hitstop = 0.;
    }
    /// IDs must be nonzero and unique in this controller's current session. The
    /// bounded N-event dedup horizon supplements, rather than replaces, netcode ACKs.
    pub fn emit(
        &mut self,
        id: u64,
        event: FeedbackEvent,
    ) -> Result<Option<FeedbackResponse>, FeedbackError> {
        if id == 0
            || !event.intensity.is_finite()
            || !(0. ..=4.).contains(&event.intensity)
            || !event.position.is_finite()
        {
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
        self.flash = self
            .flash
            .max((flash * intensity * self.config.flash).min(1.));
        self.rumble[0] = self.rumble[0].max((rumble * intensity * self.config.rumble).min(1.));
        self.rumble[1] =
            self.rumble[1].max((rumble * 0.6 * intensity * self.config.rumble).min(1.));
        if self.config.single_player_hitstop && !self.config.networked && self.config.motion > 0. {
            self.hitstop = self.hitstop.max((stop * intensity).min(0.06));
        }
        Ok(Some(FeedbackResponse {
            cue,
            gain: (gain * intensity).min(1.),
            particles: (particles as f32 * intensity).min(64.) as u8,
            reaction,
            position: event.position,
        }))
    }
    pub fn advance(&mut self, dt: f32) -> Result<FeedbackFrame, FeedbackError> {
        if !dt.is_finite() || !(0. ..=0.25).contains(&dt) {
            return Err(FeedbackError::Invalid);
        }
        let frame = FeedbackFrame {
            shake: self.shake.advance(dt).map_err(|_| FeedbackError::Invalid)?,
            flash: self.flash,
            rumble: self.rumble,
            hitstop_seconds: self.hitstop,
        };
        self.flash *= (-18. * dt).exp();
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
}
