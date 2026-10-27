//! Presentation-only effects. Neither preferences nor render cadence own combat.
use crate::game::{Enemy, Foe, Game, Note, Style, Supply};
use crate::hud::{Line, line};
use crate::juice::{self, GOLD, Juice, MAGENTA, Priority, VIOLET};
use crate::quips;
use crate::{AMBER, CORAL, CYAN, PAPER};
use ferravex_core::session::SessionState;
use ferravex_play::feedback::{
    FeedbackConfig, FeedbackController, FeedbackError, FeedbackEvent, FeedbackKind, FeedbackPolicy,
    FeedbackResponse, ImpactSample,
};
use ferravex_play::particles::{
    Emitter, EmitterDesc, EmitterTransform, ParticleDesc, ParticleMotion, ParticlePool,
};
use ferravex_render::audio::{Cue, Surface};
use ferravex_render::prelude::{Color, Vec2, vec2};
use ferravex_render::{audio::GameAudio, preferences};

const SEED: u64 = 0x4e454f4e;

pub(crate) struct Presentation {
    feedback: FeedbackController<32>,
    next_id: u64,
    pub particles: ParticlePool,
    sparks: Emitter,
    chunks: Emitter,
    pub muzzle: f32,
    pub hurt: f32,
    pub dash: f32,
    pub juice: Juice,
    facing: Vec2,
    focus: bool,
}

fn chunk_desc(start: Color, end: Color, speed: f32, size: f32, life: f32) -> EmitterDesc {
    EmitterDesc {
        particle: ParticleDesc {
            lifetime: life,
            start_color: [start.r, start.g, start.b, 1.],
            end_color: [end.r, end.g, end.b, 0.],
            start_size: size,
            end_size: size * 0.25,
            ..Default::default()
        },
        motion: ParticleMotion {
            drag: 3.2,
            angular_velocity: 9.,
            ..Default::default()
        },
        velocity_jitter: [speed, speed, 0.],
        lifetime_jitter: 0.35,
        size_jitter: 0.4,
        rotation_jitter: std::f32::consts::PI,
        rate: 0.,
        ..Default::default()
    }
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
            chunks: Emitter::new(chunk_desc(CORAL, AMBER, 240., 7., 0.55), SEED ^ 0x55).unwrap(),
            muzzle: 0.,
            hurt: 0.,
            dash: 0.,
            juice: Juice::new(),
            facing: Vec2::X,
            focus: false,
        }
    }

    /// Called before accepted events and again before drawing, even on menus
    /// and zero-tick frames. Clear disabled effects, rather than hiding them
    /// until a preference is re-enabled. Enemy flash is cosmetic; hurt_time in
    /// Game is authoritative invulnerability and must never be cleared here.
    pub fn sync(&mut self, game: &mut Game) {
        preferences::sync_feedback(&mut self.feedback);
        self.juice.apply_motion(self.motion_scale());
        self.facing = game.facing;
        self.focus = game.final_drone();
        if self.juice.stretch > 0. {
            self.juice.stretch_dir = game.dash_direction();
        }
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
        self.chunks.reset(SEED ^ 0x55);
        self.muzzle = 0.;
        self.hurt = 0.;
        self.dash = 0.;
        self.juice.reset();
        self.focus = false;
    }

    /// Age only cosmetics with presentation time, including while paused.
    pub fn age(&mut self, dt: f32) {
        self.particles
            .update(dt)
            .expect("bounded presentation delta");
        self.muzzle = (self.muzzle - dt).max(0.);
        self.hurt = (self.hurt - dt).max(0.);
        self.dash = (self.dash - dt).max(0.);
        let before = self.juice.celebrate;
        self.juice.age(dt, self.focus);
        // Victory fireworks pop along the arena sides, beside the results card.
        if let (Some(before), Some(after)) = (before, self.juice.celebrate) {
            for (i, at) in [0.3_f32, 0.6, 0.9, 1.2, 1.5].into_iter().enumerate() {
                if before < at && after >= at {
                    let spot = [
                        vec2(200., 250.),
                        vec2(1080., 600.),
                        vec2(1080., 250.),
                        vec2(200., 600.),
                        vec2(640., 660.),
                    ][i];
                    let color = [GOLD, CYAN, MAGENTA, AMBER, VIOLET][i];
                    self.juice.ring(spot, 10., 110., 0.6, color);
                    self.chunk_burst(spot, chunk_desc(color, PAPER, 300., 8., 0.9), 24);
                }
            }
        }
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
            FeedbackKind::Fire => self.juice.recoil(self.facing),
            FeedbackKind::Hurt => self.juice.trauma(0.25 + 0.3 * event.intensity),
            FeedbackKind::Dash => self.juice.dash(self.facing),
            _ => {}
        }
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

    fn chunk_burst(&mut self, at: Vec2, desc: EmitterDesc, count: u16) {
        // Additive and moving: either opt-out suppresses them with no backlog.
        let budget = if self.flash_scale() == 0. {
            0
        } else {
            (f32::from(count) * self.motion_scale()) as u16
        };
        self.chunks.set_config(desc).expect("authored chunk burst");
        self.chunks
            .burst_with_budget(
                u32::from(count),
                budget,
                EmitterTransform::at([at.x, at.y, 0.]),
                &mut self.particles,
            )
            .expect("validated accepted note position");
    }

    /// Route one accepted simulation note to juice. Returns an extra audio cue
    /// for moments the base feedback events do not already voice.
    pub fn note(&mut self, note: Note, style: &Style) -> Option<(Cue, f32)> {
        let j = &mut self.juice;
        match note {
            Note::Kill {
                at,
                foe,
                points,
                multiplier,
                multi,
                drive_by,
            } => {
                let tier = juice::tier_color(multiplier);
                let (color, size, ring, trauma, count) = match foe {
                    Foe::Drone => (CORAL, 18., 46., 0.1, 14),
                    Foe::Brute => (MAGENTA, 22., 70., 0.3, 24),
                    Foe::Overseer => (GOLD, 30., 150., 0.85, 60),
                };
                j.ring(at, 8., ring, 0.32, color);
                j.ring(at, 4., ring * 0.55, 0.22, PAPER);
                j.trauma(trauma);
                j.pop(at, line!("+{points}"), tier, size);
                if foe == Foe::Overseer {
                    j.ring(at, 20., 260., 0.7, VIOLET);
                    j.callout(
                        Line::from("OVERSEER DOWN!"),
                        Line::from("+250 style · the hive is leaderless"),
                        GOLD,
                        Priority::Epic,
                    );
                } else if let Some(line) = quips::multi_kill(multi) {
                    j.callout(
                        Line::from(line),
                        line!("{multi} in a blink · +{} style", 25 * (multi - 1)),
                        tier,
                        Priority::Combo,
                    );
                } else if drive_by {
                    j.callout(
                        Line::from(quips::drive_by(style.drive_bys)),
                        Line::from("+50 style"),
                        CYAN,
                        Priority::Minor,
                    );
                }
                let desc = match foe {
                    Foe::Drone => chunk_desc(CORAL, AMBER, 230., 7., 0.5),
                    Foe::Brute => chunk_desc(MAGENTA, CORAL, 280., 9., 0.65),
                    Foe::Overseer => chunk_desc(GOLD, VIOLET, 420., 12., 1.0),
                };
                self.chunk_burst(at, desc, count);
                Some((
                    Cue::Impact(Surface::Metal),
                    match foe {
                        Foe::Drone => 0.35,
                        Foe::Brute => 0.55,
                        Foe::Overseer => 0.9,
                    },
                ))
            }
            Note::MultiplierUp(tier) => {
                j.callout(
                    line!("×{tier} {}", quips::multiplier(tier)),
                    Line::from("Keep the chain alive"),
                    juice::tier_color(tier),
                    Priority::Combo,
                );
                Some((Cue::Success, 0.18 + 0.04 * tier as f32))
            }
            Note::ChainBroken(chain) => {
                j.callout(
                    Line::from(quips::chain_broken(style.hits_taken)),
                    line!("{chain}-kill chain lost"),
                    CORAL,
                    Priority::Minor,
                );
                None
            }
            Note::CloseShave(at) => {
                j.ring(at, 12., 40., 0.25, AMBER);
                j.callout(
                    Line::from(quips::close_shave(style.close_shaves)),
                    Line::from("+20 style"),
                    AMBER,
                    Priority::Minor,
                );
                Some((Cue::Jump, 0.25))
            }
            Note::LevelUp { level, damage } => {
                j.callout(
                    line!("WEAPON LV {level}"),
                    line!("Shots now hit for {damage:.0}"),
                    CYAN,
                    Priority::Major,
                );
                Some((Cue::Reload, 0.4))
            }
            Note::EliteInbound(at) => {
                j.ring(at, 10., 120., 0.6, GOLD);
                j.trauma(0.3);
                j.callout(
                    Line::from("OVERSEER INBOUND"),
                    Line::from("Big, armored, grumpy · focus fire to stagger"),
                    GOLD,
                    Priority::Epic,
                );
                Some((Cue::Impact(Surface::Metal), 0.8))
            }
            Note::LastDrone => {
                j.callout(
                    Line::from("ONE LEFT!"),
                    Line::from("Make it count"),
                    PAPER,
                    Priority::Major,
                );
                None
            }
            Note::WaveClear {
                untouched, health, ..
            } => {
                if health <= crate::game::CLUTCH_HEALTH {
                    j.callout(
                        line!("SURVIVED ON {health} HP"),
                        Line::from("+150 style · breathe"),
                        CORAL,
                        Priority::Epic,
                    );
                } else if untouched {
                    j.callout(
                        Line::from("UNTOUCHED!"),
                        Line::from("Not a scratch · +200 style"),
                        GOLD,
                        Priority::Epic,
                    );
                }
                None
            }
            Note::Beacon { index, lit } => {
                let at = crate::game::BEACONS[index];
                j.ring(at, 6., 60., 0.5, CYAN);
                if lit == 4 {
                    j.callout(
                        Line::from("DISCO MODE UNLOCKED"),
                        Line::from("All four lamps lit · the sector is now a party"),
                        MAGENTA,
                        Priority::Epic,
                    );
                    for beacon in crate::game::BEACONS {
                        self.chunk_burst(beacon, chunk_desc(MAGENTA, CYAN, 260., 8., 0.9), 16);
                    }
                    Some((Cue::Success, 0.6))
                } else {
                    j.callout(
                        line!("LAMP {lit}/4 LIT"),
                        Line::from("Something is warming up…"),
                        CYAN,
                        Priority::Major,
                    );
                    Some((Cue::Pickup, 0.4))
                }
            }
            Note::Supply { at, kind } => {
                let (text, color) = match kind {
                    Supply::Repair => ("+22 HP", Color::from_rgba(130, 246, 160, 255)),
                    Supply::Battery => ("Shield full", Color::from_rgba(140, 200, 255, 255)),
                };
                j.ring(at, 10., 44., 0.3, color);
                j.pop(at - vec2(0., 18.), Line::from(text), color, 16.);
                None
            }
        }
    }

    /// Start the victory celebration once; it ages with presentation time.
    pub fn celebrate(&mut self) {
        if self.juice.celebrate.is_none() {
            self.juice.celebrate = Some(0.);
            for (i, at) in crate::game::BEACONS.into_iter().enumerate() {
                let color = [CYAN, GOLD, MAGENTA, AMBER][i];
                self.chunk_burst(at, chunk_desc(color, PAPER, 320., 9., 1.1), 20);
            }
        }
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

    #[test]
    fn notes_respect_reduced_motion_and_flash_but_keep_cues_and_text() {
        let _guard = RestorePolicy::new();
        let mut game = Game::new();
        let kill = Note::Kill {
            at: Vec2::new(300., 300.),
            foe: Foe::Overseer,
            points: 500,
            multiplier: 5,
            multi: 1,
            drive_by: false,
        };
        let style = Style::default();
        let mut full = Presentation::new();
        full.sync(&mut game);
        let cue = full.note(kill, &style);
        assert!(full.particles.len() >= 60, "chunky death pop");
        assert!(full.juice.rings.iter().flatten().count() >= 2);
        assert!(full.juice.pops.iter().flatten().any(|p| p.text == "+500"));
        assert_eq!(full.juice.callout.as_ref().unwrap().head, "OVERSEER DOWN!");
        full.age(0.01);
        assert!(full.juice.shake().length() > 0.);
        for (motion, flash) in [(true, false), (false, true), (true, true)] {
            preferences::set_feedback_policy(FeedbackPolicy::new(motion, flash, 1.).unwrap());
            let mut p = Presentation::new();
            p.sync(&mut game);
            assert_eq!(
                p.note(kill, &style),
                cue,
                "audio ignores visual preferences"
            );
            assert!(p.particles.is_empty());
            assert_eq!(p.juice.rings.iter().flatten().count() == 0, motion);
            assert!(p.juice.pops.iter().flatten().any(|p| p.text == "+500"));
            assert!(p.juice.callout.is_some());
            p.age(0.01);
            if motion {
                assert_eq!(p.juice.shake(), Vec2::ZERO);
            }
        }
        preferences::set_feedback_policy(FeedbackPolicy::default());
        full.celebrate();
        full.age(2.);
        full.reset();
        assert!(full.juice.callout.is_none() && full.juice.celebrate.is_none());
        assert!(full.particles.is_empty());
    }
}
