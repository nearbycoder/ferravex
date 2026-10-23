//! Presentation-only effects. Neither preferences nor render cadence own combat.
use crate::game::{Enemy, Game};
use ferravex_core::session::SessionState;
use ferravex_play::feedback::{
    FeedbackConfig, FeedbackController, FeedbackError, FeedbackEvent, FeedbackKind, FeedbackPolicy,
    FeedbackResponse, ImpactSample,
};
use ferravex_play::particles::{
    Emitter, EmitterDesc, EmitterTransform, ParticleDesc, ParticleMotion, ParticlePool,
};
use ferravex_render::{audio::GameAudio, preferences};

const SEED: u64 = 0x4e454f4e;

pub(crate) struct Presentation {
    feedback: FeedbackController<32>,
    next_id: u64,
    pub particles: ParticlePool,
    sparks: Emitter,
    pub muzzle: f32,
    pub hurt: f32,
    pub dash: f32,
}

impl Presentation {
    pub fn new() -> Self {
        Self {
            feedback: FeedbackController::new(FeedbackConfig::default(), SEED).unwrap(),
            next_id: 0,
            particles: ParticlePool::new(512),
            sparks: Emitter::new(
                EmitterDesc {
                    particle: ParticleDesc {
                        lifetime: 0.45,
                        start_color: [1., 0.49, 0.34, 1.],
                        end_color: [1., 0.2, 0.08, 0.],
                        start_size: 3.8,
                        end_size: 1.,
                        ..Default::default()
                    },
                    motion: ParticleMotion {
                        drag: 4.,
                        angular_velocity: 5.,
                        ..Default::default()
                    },
                    velocity_jitter: [110., 110., 0.],
                    lifetime_jitter: 0.25,
                    size_jitter: 0.3,
                    rotation_jitter: std::f32::consts::PI,
                    rate: 0.,
                    ..Default::default()
                },
                SEED,
            )
            .unwrap(),
            muzzle: 0.,
            hurt: 0.,
            dash: 0.,
        }
    }

    /// Called before accepted events and again before drawing, even on menus
    /// and zero-tick frames. Clear disabled effects, rather than hiding them
    /// until a preference is re-enabled. Enemy flash is cosmetic; hurt_time in
    /// Game is authoritative invulnerability and must never be cleared here.
    pub fn sync(&mut self, game: &mut Game) {
        preferences::sync_feedback(&mut self.feedback);
        if self.motion_scale() == 0. {
            self.dash = 0.;
        }
        if self.flash_scale() == 0. {
            self.muzzle = 0.;
            self.hurt = 0.;
            for mut enemy in &mut game.world.query::<&mut Enemy>() {
                enemy.flash = 0.;
            }
        }
        // These sparks are both moving and additive. Either independent opt-out
        // retires existing particles as well as preventing new emission.
        if self.motion_scale() == 0. || self.flash_scale() == 0. {
            self.particles.clear();
        }
    }

    fn policy(&self) -> FeedbackPolicy {
        self.feedback.policy()
    }

    pub fn motion_scale(&self) -> f32 {
        if self.policy().reduced_motion() {
            0.
        } else {
            self.policy().intensity()
        }
    }

    pub fn flash_scale(&self) -> f32 {
        if self.policy().reduced_flash() {
            0.
        } else {
            self.policy().intensity()
        }
    }

    pub fn reset(&mut self) {
        self.feedback.clear();
        self.next_id = 0;
        self.particles.clear();
        self.sparks.reset(SEED);
        self.muzzle = 0.;
        self.hurt = 0.;
        self.dash = 0.;
    }

    /// Age only cosmetics with presentation time, including while paused.
    pub fn age(&mut self, dt: f32) {
        self.particles
            .update(dt)
            .expect("bounded presentation delta");
        self.muzzle = (self.muzzle - dt).max(0.);
        self.hurt = (self.hurt - dt).max(0.);
        self.dash = (self.dash - dt).max(0.);
    }

    pub fn accept(
        &mut self,
        event: FeedbackEvent,
    ) -> Result<Option<FeedbackResponse>, FeedbackError> {
        self.next_id = self.next_id.checked_add(1).ok_or(FeedbackError::Capacity)?;
        let Some(response) = self.feedback.emit(self.next_id, event)? else {
            return Ok(None);
        };
        match event.kind {
            FeedbackKind::Fire if self.flash_scale() > 0. => self.muzzle = 0.055,
            FeedbackKind::Hurt if self.flash_scale() > 0. => self.hurt = 0.75,
            FeedbackKind::Dash if self.motion_scale() > 0. => self.dash = 0.18,
            // The combat event already distinguishes kills from ordinary hits.
            FeedbackKind::Hit if event.intensity == 1. => {
                let budget = if self.flash_scale() == 0. {
                    0
                } else {
                    (9. * self.motion_scale()) as u16
                };
                self.sparks
                    .burst_with_budget(
                        9,
                        budget,
                        EmitterTransform::at(event.position.to_array()),
                        &mut self.particles,
                    )
                    .expect("validated accepted hit position");
            }
            _ => {}
        }
        Ok(Some(response))
    }

    pub fn sample(&mut self, dt: f32) -> ImpactSample {
        let (_, impact) = self
            .feedback
            .advance_with_impact(dt)
            .expect("bounded presentation delta");
        impact
    }
}

pub(crate) fn sync_audio(sound: &mut GameAudio, state: SessionState) {
    if matches!(state, SessionState::Ready | SessionState::Paused) {
        sound.pause();
    } else {
        // Let accepted win/loss cues finish on terminal screens.
        sound.resume();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        game::{Game, Input, Position},
        run::Run,
    };
    use ferravex_render::{audio::Cue, prelude::Vec3};

    struct RestorePolicy(FeedbackPolicy);
    impl RestorePolicy {
        fn new() -> Self {
            let guard = Self(preferences::feedback_policy());
            preferences::set_feedback_policy(FeedbackPolicy::default());
            guard
        }
    }
    impl Drop for RestorePolicy {
        fn drop(&mut self) {
            preferences::set_feedback_policy(self.0);
        }
    }

    fn all_effects(presentation: &mut Presentation) {
        for kind in [
            FeedbackKind::Fire,
            FeedbackKind::Dash,
            FeedbackKind::Hurt,
            FeedbackKind::Hit,
        ] {
            assert!(
                presentation
                    .accept(FeedbackEvent {
                        kind,
                        intensity: 1.,
                        position: Vec3::ZERO
                    })
                    .unwrap()
                    .is_some()
            );
        }
    }

    #[test]
    fn live_preferences_retire_independent_channels_without_a_tick_or_resume() {
        let _guard = RestorePolicy::new();
        let mut game = Game::new();
        // Exercise the actual spawn-flash path before suspending all simulation.
        for _ in 0..96 {
            game.update(Input::default(), 1. / 120.);
        }
        assert_eq!(game.enemy_count(), 1);
        game.hurt_time = 0.75;
        let (time, player, health, shield) =
            (game.time, game.player, game.health, game.shield.value());
        for motion in [false, true] {
            for flash in [false, true] {
                preferences::set_feedback_policy(FeedbackPolicy::default());
                let mut p = Presentation::new();
                p.sync(&mut game);
                for mut enemy in &mut game.world.query::<&mut Enemy>() {
                    enemy.flash = 0.1;
                }
                all_effects(&mut p);
                assert_eq!(p.particles.len(), 9);
                preferences::set_feedback_policy(FeedbackPolicy::new(motion, flash, 1.).unwrap());
                p.sync(&mut game);
                let impact = p.sample(0.);
                assert_eq!(impact.emphasis > 0., !motion);
                assert_eq!(impact.flash > 0., !flash);
                assert_eq!(p.dash > 0., !motion);
                assert_eq!(p.hurt > 0., !flash);
                assert_eq!(p.muzzle > 0., !flash);
                assert_eq!(p.particles.len(), if motion || flash { 0 } else { 9 });
                assert!(
                    game.world
                        .query::<&Enemy>()
                        .iter()
                        .all(|enemy| (enemy.flash > 0.) != flash)
                );
                // New accepted events during suppression must not be queued.
                all_effects(&mut p);
                p.sync(&mut game);
                preferences::set_feedback_policy(FeedbackPolicy::default());
                p.sync(&mut game);
                let restored = p.sample(0.);
                assert_eq!(restored.emphasis > 0., !motion);
                assert_eq!(restored.flash > 0., !flash);
                assert_eq!(p.dash > 0., !motion);
                assert_eq!(p.hurt > 0., !flash);
                assert_eq!(p.muzzle > 0., !flash);
                assert_eq!(p.particles.is_empty(), motion || flash);
                assert_eq!(game.hurt_time, 0.75, "invulnerability is simulation state");
                assert_eq!(
                    (game.time, game.player, game.health, game.shield.value()),
                    (time, player, health, shield)
                );
            }
        }
    }

    #[test]
    fn zero_intensity_reset_and_paused_aging_do_not_resurrect_effects() {
        let _guard = RestorePolicy::new();
        let mut game = Game::new();
        let mut p = Presentation::new();
        p.sync(&mut game);
        all_effects(&mut p);
        preferences::set_feedback_policy(FeedbackPolicy::new(false, false, 0.).unwrap());
        p.sync(&mut game);
        assert_eq!(p.sample(0.), ImpactSample::default());
        all_effects(&mut p);
        preferences::set_feedback_policy(FeedbackPolicy::default());
        p.sync(&mut game);
        assert_eq!(
            (p.muzzle, p.hurt, p.dash, p.particles.len()),
            (0., 0., 0., 0)
        );
        assert_eq!(p.sample(0.), ImpactSample::default());
        all_effects(&mut p);
        for _ in 0..8 {
            p.age(0.25);
            p.sample(0.25);
        }
        assert_eq!(
            (p.muzzle, p.hurt, p.dash, p.particles.len()),
            (0., 0., 0., 0)
        );
        assert_eq!(p.sample(0.), ImpactSample::default());
        assert_eq!(game.time, 0.);
        all_effects(&mut p);
        p.reset();
        assert_eq!(
            (p.muzzle, p.hurt, p.dash, p.particles.len(), p.next_id),
            (0., 0., 0., 0, 0)
        );
        assert_eq!(p.sample(0.), ImpactSample::default());
    }

    #[test]
    fn accepted_sound_survives_visual_reduction_and_menus_use_audio_suspension() {
        let _guard = RestorePolicy::new();
        let mut game = Game::new();
        let mut p = Presentation::new();
        let event = FeedbackEvent::fire(Vec3::ZERO, 1.).unwrap();
        p.sync(&mut game);
        let full = p.accept(event).unwrap().unwrap();
        preferences::set_feedback_policy(FeedbackPolicy::new(true, true, 0.).unwrap());
        p.sync(&mut game);
        let reduced = p.accept(event).unwrap().unwrap();
        assert_eq!((full.cue, full.gain), (reduced.cue, reduced.gain));
        assert_eq!(p.muzzle, 0.);
        let mut sound = GameAudio::new();
        for state in [SessionState::Ready, SessionState::Paused] {
            sync_audio(&mut sound, state);
            assert!(sound.is_paused());
            assert_eq!(sound.cue_with_outcome(full.cue, full.gain), None);
            assert!(sound.cue_with_outcome(Cue::Click, 0.1).is_some());
        }
        sound.reset();
        assert!(sound.is_paused());
        for state in [SessionState::Playing, SessionState::Won, SessionState::Lost] {
            sync_audio(&mut sound, state);
            assert!(!sound.is_paused());
            assert!(sound.cue_with_outcome(full.cue, full.gain).is_some());
        }
    }

    #[test]
    fn rejected_requests_do_not_create_effects_and_particle_capacity_stays_bounded() {
        let _guard = RestorePolicy::new();
        let mut run = Run::new(true);
        let mut p = Presentation::new();
        p.sync(&mut run.game);
        let input = Input {
            fire: true,
            dash_pressed: true,
            ..Default::default()
        };
        run.advance(
            1. / 120.,
            |_| input,
            |event| {
                p.accept(event).unwrap();
            },
        );
        assert_eq!(p.next_id, 2);
        p.reset();
        run.advance(
            1. / 120.,
            |_| input,
            |event| {
                p.accept(event).unwrap();
            },
        );
        assert_eq!((p.next_id, p.muzzle, p.dash), (0, 0., 0.));
        for _ in 0..8 {
            for _ in 0..32 {
                p.accept(FeedbackEvent::hit(Vec3::ZERO, 1.).unwrap())
                    .unwrap();
            }
            p.sample(0.);
            assert!(p.particles.len() <= 512);
        }
        assert_eq!(p.particles.len(), 512);
        p.reset();
        assert!(p.particles.is_empty());
        all_effects(&mut p);
        assert_eq!(p.particles.len(), 9);
    }

    #[test]
    fn autoplay_combat_and_accepted_events_match_with_live_presentation_changes() {
        let _guard = RestorePolicy::new();
        let mut baseline = Run::new(true);
        let mut observed = Run::new(true);
        let mut p = Presentation::new();
        for frame in 0..20_000 {
            let dt = [0., 1. / 30., 1. / 144., 0.075][frame % 4];
            preferences::set_feedback_policy(
                FeedbackPolicy::new(frame % 3 == 0, frame % 5 == 0, 1.).unwrap(),
            );
            p.sync(&mut observed.game);
            p.age((dt as f32).min(0.25));
            let mut expected = Vec::new();
            let mut actual = Vec::new();
            baseline.advance(dt, Game::autoplay, |event| {
                expected.push((event.kind, event.position, event.intensity))
            });
            observed.advance(dt, Game::autoplay, |event| {
                actual.push((event.kind, event.position, event.intensity));
                p.accept(event).unwrap();
            });
            p.sync(&mut observed.game);
            p.sample((dt as f32).min(0.25));
            assert_eq!(expected, actual);
            assert_eq!(
                (
                    baseline.game.time,
                    baseline.game.player,
                    baseline.game.shots,
                    baseline.game.health
                ),
                (
                    observed.game.time,
                    observed.game.player,
                    observed.game.shots,
                    observed.game.health
                )
            );
            if !baseline.session.is_playing() {
                break;
            }
        }
        assert_eq!(observed.session.state(), SessionState::Won);
        assert_eq!(observed.game.kills, 41);
        assert_eq!(baseline.game.score.total(), observed.game.score.total());
        let enemies = observed
            .game
            .world
            .query::<(&Position, &Enemy)>()
            .iter()
            .count();
        assert_eq!(enemies, 0);
    }
}
