//! Presentation-only juice, created exclusively from accepted simulation events.
//!
//! Nothing here can change combat, timing or rewards. Reduced motion clears
//! and blocks shake, kick, floating numbers and moving particles; reduced
//! flash removes hit flashes, screen flashes and additive bursts. Both come
//! from the shared engine preferences through a `FeedbackController`.
#![allow(clippy::too_many_arguments)] // Drawing helpers take explicit geometry.
use crate::sfx::{Sfx, SoundKit, chime_pitch};
use crate::sim::content::*;
use crate::sim::{BossKind, Card, Event, Game};
use ferravex_play::feedback::{
    FeedbackConfig, FeedbackController, FeedbackEvent, FeedbackKind, FeedbackPolicy,
};
use ferravex_play::particles::{
    Emitter, EmitterDesc, EmitterTransform, ParticleDesc, ParticleMotion, ParticlePool,
};
use ferravex_render::preferences;
use ferravex_render::prelude::{Vec2, Vec3, vec2};

const NUMBERS: usize = 96;
const RINGS: usize = 48;
const GHOSTS: usize = 8;
const CALLOUTS: usize = 3;
const SEED: u64 = 0x676c_6f77;

#[derive(Clone, Copy, Debug, Default)]
pub struct Number {
    pub pos: Vec2,
    pub amount: f32,
    pub crit: bool,
    pub age: f32,
    pub uid: u32,
    pub pop: f32,
    /// 0 damage, 1 heal, 2 glimmer.
    pub style: u8,
    pub live: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Ring {
    pub pos: Vec2,
    pub r0: f32,
    pub r1: f32,
    pub age: f32,
    pub life: f32,
    pub color: Rgb,
    pub width: f32,
    pub live: bool,
}

impl Ring {
    pub fn t(&self) -> f32 {
        (self.age / self.life).clamp(0., 1.)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Say {
    Tide(u8),
    Elite { name: u8, kind: FoeKind },
    EliteDown,
    Swarm(u8),
    BossWarning(BossKind),
    BossPhase(BossKind, u8),
    BossDown(BossKind),
    Evolved(WeaponId),
    SecondWind,
    Charge,
    Lure,
}

#[derive(Clone, Copy, Debug)]
pub struct Callout {
    pub say: Say,
    pub age: f32,
    pub life: f32,
}

impl Callout {
    pub fn alpha(&self) -> f32 {
        let fade_in = (self.age / 0.12).min(1.);
        let fade_out = ((self.life - self.age) / 0.4).clamp(0., 1.);
        fade_in.min(fade_out)
    }
}

fn desc(
    start: [f32; 4],
    end: [f32; 4],
    speed: f32,
    size: f32,
    life: f32,
    drag: f32,
) -> EmitterDesc {
    EmitterDesc {
        particle: ParticleDesc {
            lifetime: life,
            start_color: start,
            end_color: end,
            start_size: size,
            end_size: size * 0.2,
            ..Default::default()
        },
        motion: ParticleMotion {
            drag,
            angular_velocity: 6.,
            ..Default::default()
        },
        velocity_jitter: [speed, speed, 0.],
        lifetime_jitter: 0.35,
        size_jitter: 0.45,
        rotation_jitter: std::f32::consts::PI,
        rate: 0.,
        ..Default::default()
    }
}

pub struct Fx {
    feedback: FeedbackController<64>,
    next_id: u64,
    pub sparks: ParticlePool,
    pub pixels: ParticlePool,
    spark: Emitter,
    chunk: Emitter,
    pub numbers: [Number; NUMBERS],
    pub rings: [Ring; RINGS],
    pub callouts: [Option<Callout>; CALLOUTS],
    pub ghosts: [(Vec2, f32); GHOSTS],
    ghost_next: usize,
    pub time: f32,
    trauma: f32,
    pub kick: Vec2,
    pub hurt: f32,
    pub flash: f32,
    pub level_glow: f32,
    pub chime_step: u32,
    chime_timer: f32,
    pub tide_from: usize,
    pub tide_to: usize,
    pub tide_t: f32,
    pub boss_intro: Option<(BossKind, f32)>,
    pub boss_flash: f32,
    pub camera: Vec2,
    camera_ready: bool,
    pub zoom_punch: f32,
    bursts_this_frame: u32,
    pub victory_glow: f32,
}

impl Fx {
    pub fn new() -> Self {
        Self {
            feedback: FeedbackController::new(FeedbackConfig::default(), SEED).unwrap(),
            next_id: 0,
            sparks: ParticlePool::new(1400),
            pixels: ParticlePool::new(700),
            spark: Emitter::new(desc([1.; 4], [1., 1., 1., 0.], 160., 4., 0.4, 4.), SEED).unwrap(),
            chunk: Emitter::new(desc([1.; 4], [1., 1., 1., 0.], 200., 6., 0.6, 3.), SEED ^ 7)
                .unwrap(),
            numbers: [Number::default(); NUMBERS],
            rings: [Ring::default(); RINGS],
            callouts: [None; CALLOUTS],
            ghosts: [(Vec2::ZERO, 1.); GHOSTS],
            ghost_next: 0,
            time: 0.,
            trauma: 0.,
            kick: Vec2::ZERO,
            hurt: 0.,
            flash: 0.,
            level_glow: 0.,
            chime_step: 0,
            chime_timer: 0.,
            tide_from: 0,
            tide_to: 0,
            tide_t: 1.,
            boss_intro: None,
            boss_flash: 0.,
            camera: Vec2::ZERO,
            camera_ready: false,
            zoom_punch: 0.,
            bursts_this_frame: 0,
            victory_glow: 0.,
        }
    }

    pub fn policy(&self) -> FeedbackPolicy {
        self.feedback.policy()
    }

    pub fn motion(&self) -> f32 {
        let p = self.policy();
        if p.reduced_motion() {
            0.
        } else {
            p.intensity()
        }
    }

    pub fn flashes(&self) -> f32 {
        let p = self.policy();
        if p.reduced_flash() { 0. } else { p.intensity() }
    }

    /// Call every frame (menus and zero-tick frames too) before events/drawing.
    pub fn sync(&mut self) {
        preferences::sync_feedback(&mut self.feedback);
        if self.motion() == 0. {
            self.trauma = 0.;
            self.kick = Vec2::ZERO;
            self.zoom_punch = 0.;
        }
        if self.motion() == 0. || self.flashes() == 0. {
            // Moving, additive particles honor either opt-out immediately.
            self.sparks.clear();
        }
        if self.flashes() == 0. {
            self.flash = 0.;
            self.boss_flash = 0.;
        }
    }

    pub fn reset(&mut self) {
        self.feedback.clear();
        self.next_id = 0;
        self.sparks.clear();
        self.pixels.clear();
        self.spark.reset(SEED);
        self.chunk.reset(SEED ^ 7);
        self.numbers = [Number::default(); NUMBERS];
        self.rings = [Ring::default(); RINGS];
        self.callouts = [None; CALLOUTS];
        self.ghosts = [(Vec2::ZERO, 1.); GHOSTS];
        self.trauma = 0.;
        self.kick = Vec2::ZERO;
        self.hurt = 0.;
        self.flash = 0.;
        self.level_glow = 0.;
        self.chime_step = 0;
        self.chime_timer = 0.;
        self.tide_from = 0;
        self.tide_to = 0;
        self.tide_t = 1.;
        self.boss_intro = None;
        self.boss_flash = 0.;
        self.camera_ready = false;
        self.zoom_punch = 0.;
        self.victory_glow = 0.;
    }

    /// Age cosmetics by presentation time (also while paused, for menus).
    pub fn age(&mut self, dt: f32, running: bool) {
        self.time += dt;
        let _ = self.feedback.advance(dt.clamp(0., 0.25));
        if running {
            self.sparks.update(dt).expect("bounded presentation delta");
            self.pixels.update(dt).expect("bounded presentation delta");
            for n in &mut self.numbers {
                if n.live {
                    n.age += dt;
                    n.pop = (n.pop - dt * 5.).max(0.);
                    if n.age > 0.75 {
                        n.live = false;
                    }
                }
            }
            for r in &mut self.rings {
                if r.live {
                    r.age += dt;
                    if r.age >= r.life {
                        r.live = false;
                    }
                }
            }
            for g in &mut self.ghosts {
                g.1 += dt;
            }
        }
        for c in self.callouts.iter_mut() {
            if let Some(call) = c {
                call.age += dt;
                if call.age > call.life {
                    *c = None;
                }
            }
        }
        if let Some((_, t)) = &mut self.boss_intro {
            *t += dt;
            if *t > 3.4 {
                self.boss_intro = None;
            }
        }
        self.trauma = (self.trauma - dt * 1.7).max(0.);
        self.kick *= (-14. * dt).exp();
        self.zoom_punch *= (-6. * dt).exp();
        self.hurt = (self.hurt - dt * 2.2).max(0.);
        self.flash = (self.flash - dt * 3.).max(0.);
        self.boss_flash = (self.boss_flash - dt * 1.5).max(0.);
        self.level_glow = (self.level_glow - dt * 1.4).max(0.);
        self.victory_glow = (self.victory_glow - dt * 0.4).max(0.);
        self.tide_t = (self.tide_t + dt / 4.).min(1.);
        self.chime_timer -= dt;
        if self.chime_timer <= 0. {
            self.chime_step = 0;
        }
        self.bursts_this_frame = 0;
    }

    /// Smoothed camera with a little look-ahead; snaps on new runs.
    pub fn follow(&mut self, target: Vec2, velocity: Vec2, dt: f32) {
        let lead = velocity * 0.12 * self.motion().min(1.);
        let goal = target + lead;
        if !self.camera_ready {
            self.camera = goal;
            self.camera_ready = true;
        }
        self.camera += (goal - self.camera) * (1. - (-7. * dt).exp());
    }

    pub fn shake_offset(&self) -> Vec2 {
        let m = self.motion();
        if m == 0. {
            return Vec2::ZERO;
        }
        let t = self.time;
        let k = self.trauma * self.trauma * 11. * m;
        vec2(
            (t * 61.).sin() * 0.6 + (t * 37.3).sin() * 0.4,
            (t * 53.).cos() * 0.6 + (t * 29.7).sin() * 0.4,
        ) * k
            + self.kick * m
    }

    pub fn add_trauma(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount * self.motion()).min(1.);
    }

    fn accept(&mut self, kind: FeedbackKind, pos: Vec2, intensity: f32) -> bool {
        self.next_id += 1;
        let event = FeedbackEvent::new(kind, Vec3::new(pos.x, pos.y, 0.), intensity.clamp(0., 4.));
        match event {
            Ok(event) => matches!(self.feedback.emit(self.next_id, event), Ok(Some(_))),
            Err(_) => false,
        }
    }

    pub fn ring(&mut self, pos: Vec2, r0: f32, r1: f32, life: f32, color: Rgb, width: f32) {
        let slot = self.rings.iter().position(|r| !r.live).unwrap_or_else(|| {
            (0..RINGS)
                .max_by(|&a, &b| self.rings[a].t().total_cmp(&self.rings[b].t()))
                .unwrap_or(0)
        });
        let motion = self.motion();
        self.rings[slot] = Ring {
            pos,
            r0: if motion == 0. { r1 } else { r0 },
            r1,
            age: 0.,
            life,
            color,
            width,
            live: true,
        };
    }

    fn number(&mut self, pos: Vec2, amount: f32, crit: bool, uid: u32, style: u8) {
        // Stack rapid hits on the same foe into one growing number.
        if uid != 0
            && let Some(n) = self
                .numbers
                .iter_mut()
                .find(|n| n.live && n.uid == uid && n.age < 0.45 && n.style == style)
        {
            n.amount += amount;
            n.crit |= crit;
            n.pop = 1.;
            n.age = n.age.min(0.15);
            return;
        }
        // Bound clutter: once the screen is busy, only crits and heals/glimmer pop up.
        let live = self.numbers.iter().filter(|n| n.live).count();
        if style == 0 && !crit && live > 28 && amount < 40. {
            return;
        }
        let slot = self
            .numbers
            .iter()
            .position(|n| !n.live)
            .unwrap_or_else(|| {
                (0..NUMBERS)
                    .max_by(|&a, &b| self.numbers[a].age.total_cmp(&self.numbers[b].age))
                    .unwrap_or(0)
            });
        let jitter = vec2(
            ((uid as f32) * 12.9898).sin(),
            ((uid as f32) * 78.233).sin(),
        ) * 6.;
        self.numbers[slot] = Number {
            pos: pos + jitter,
            amount,
            crit,
            age: 0.,
            uid,
            pop: 1.,
            style,
            live: true,
        };
    }

    fn burst(
        &mut self,
        chunk: bool,
        pos: Vec2,
        color: Rgb,
        count: u16,
        speed: f32,
        size: f32,
        life: f32,
    ) {
        if self.bursts_this_frame > 48 {
            return;
        }
        self.bursts_this_frame += 1;
        let scale = self
            .motion()
            .min(self.flashes().max(if chunk { 1. } else { 0. }));
        let budget = (f32::from(count) * scale) as u16;
        if budget == 0 {
            return;
        }
        let d = desc(
            [color[0], color[1], color[2], 1.],
            [color[0] * 0.6, color[1] * 0.6, color[2] * 0.6, 0.],
            speed,
            size,
            life,
            if chunk { 2.5 } else { 4. },
        );
        let (emitter, pool) = if chunk {
            (&mut self.chunk, &mut self.pixels)
        } else {
            (&mut self.spark, &mut self.sparks)
        };
        if emitter.set_config(d).is_ok() {
            let _ = emitter.burst_with_budget(
                u32::from(count),
                budget,
                EmitterTransform::at([pos.x, pos.y, 0.]),
                pool,
            );
        }
    }

    fn say(&mut self, say: Say, life: f32) {
        // Newest first; drop the oldest when full.
        for i in (1..CALLOUTS).rev() {
            self.callouts[i] = self.callouts[i - 1];
        }
        self.callouts[0] = Some(Callout { say, age: 0., life });
    }

    /// Route accepted events to visuals and sound. `quiet` mutes (attract mode).
    pub fn consume(&mut self, events: &[Event], game: &Game, sound: &mut SoundKit, quiet: bool) {
        let play = |sound: &mut SoundKit, s: Sfx, gain: f32, pitch: f32| {
            if !quiet {
                sound.play(s, gain, pitch);
            }
        };
        for &event in events {
            match event {
                Event::Hit {
                    pos,
                    amount,
                    crit,
                    weapon,
                    uid,
                } => {
                    self.number(pos, amount, crit, uid, 0);
                    let c = weapon.info().color;
                    self.burst(false, pos, c, if crit { 7 } else { 3 }, 140., 3.5, 0.28);
                    if crit {
                        play(sound, Sfx::Crit, 0.5, 1. + (uid % 5) as f32 * 0.04);
                    } else {
                        play(sound, Sfx::Hit, 0.35, 0.9 + (uid % 7) as f32 * 0.04);
                    }
                }
                Event::Kill {
                    pos, kind, elite, ..
                } => {
                    let info = kind.info();
                    let big = elite || kind.is_boss();
                    self.burst(
                        true,
                        pos,
                        info.color,
                        if big { 40 } else { 9 },
                        if big { 260. } else { 150. },
                        if big { 7. } else { 4.5 },
                        0.55,
                    );
                    self.burst(
                        false,
                        pos,
                        info.color,
                        if big { 30 } else { 6 },
                        220.,
                        4.,
                        0.4,
                    );
                    self.ring(
                        pos,
                        info.radius * 0.5,
                        info.radius * if big { 6. } else { 2.6 },
                        if big { 0.7 } else { 0.32 },
                        info.color,
                        if big { 6. } else { 3. },
                    );
                    if big {
                        self.add_trauma(0.45);
                        play(sound, Sfx::PopBig, 0.8, 1.);
                        if elite {
                            self.say(Say::EliteDown, 2.2);
                            if self.accept(FeedbackKind::Explosion, pos, 0.6) {
                                self.flash = self.flash.max(0.25 * self.flashes());
                            }
                        }
                    } else {
                        let pitch = match kind {
                            FoeKind::Mothling => 1.25,
                            FoeKind::Crabbo => 0.7,
                            FoeKind::Grumpfish => 0.85,
                            _ => 1.,
                        };
                        play(
                            sound,
                            Sfx::Pop,
                            0.32,
                            pitch * (0.92 + (self.time * 13.).fract() * 0.16),
                        );
                    }
                }
                Event::Hurt { pos, amount } => {
                    if self.accept(FeedbackKind::Hurt, pos, (amount / 20.).clamp(0.3, 1.5)) {
                        self.add_trauma(0.32 + amount / 80.);
                        self.hurt = 1.;
                        self.burst(true, pos, [1., 0.35, 0.35], 14, 180., 5., 0.5);
                        play(sound, Sfx::Hurt, 0.7, 1.);
                    }
                }
                Event::Dash { pos, dir } => {
                    if self.accept(FeedbackKind::Dash, pos, 1.) {
                        self.ghosts[self.ghost_next] = (pos, 0.);
                        self.ghost_next = (self.ghost_next + 1) % GHOSTS;
                        self.kick = -dir * 3.;
                        self.burst(false, pos, game.character().color, 10, 120., 4., 0.35);
                        play(sound, Sfx::Whoosh, 0.55, 1.1);
                    }
                }
                Event::Collect { pos, value } => {
                    self.chime_timer = 0.55;
                    let pitch = chime_pitch(self.chime_step / 2);
                    self.chime_step = (self.chime_step + 1).min(40);
                    play(
                        sound,
                        Sfx::Chime,
                        0.18 + (value as f32).sqrt() * 0.02,
                        pitch,
                    );
                    if value >= 10 {
                        self.burst(false, pos, [0.75, 0.6, 1.], 8, 90., 3.5, 0.4);
                    }
                }
                Event::Coin { pos, value } => {
                    self.number(pos, value as f32, false, 0, 2);
                    self.burst(false, pos, [1., 0.85, 0.4], 10, 110., 3.5, 0.45);
                    play(sound, Sfx::Coin, 0.45, 1.);
                }
                Event::Heal { pos, amount } => {
                    if amount >= 1. {
                        self.number(pos + vec2(0., -10.), amount, false, 0, 1);
                        play(sound, Sfx::Heal, 0.35, 1.);
                    }
                }
                Event::MagnetPulse { pos } => {
                    self.ring(pos, 20., 900., 0.9, [1., 0.6, 0.9], 10.);
                    play(sound, Sfx::Magnet, 0.6, 1.);
                }
                Event::ChestOpened { pos } => {
                    self.ring(pos, 10., 160., 0.6, [1., 0.85, 0.45], 8.);
                    self.burst(true, pos, [1., 0.85, 0.45], 30, 240., 6., 0.8);
                    play(sound, Sfx::Chest, 0.7, 1.);
                }
                Event::LevelUp { .. } => {
                    self.level_glow = 1.;
                    self.ring(game.player.pos, 10., 240., 0.7, [0.6, 1., 0.95], 8.);
                    if self.accept(FeedbackKind::Pickup, game.player.pos, 1.) {
                        play(sound, Sfx::LevelUp, 0.6, 1.);
                    }
                }
                Event::Fire { weapon, .. } => match weapon {
                    WeaponId::WispBolt => play(sound, Sfx::Pew, 0.22, 1.),
                    WeaponId::BubbleMortar => play(sound, Sfx::Bubble, 0.28, 1.),
                    WeaponId::TideLash => play(sound, Sfx::Lash, 0.4, 1.),
                    WeaponId::ChainSpark => play(sound, Sfx::Zap, 0.32, 1.),
                    WeaponId::ShellBoomerang => play(sound, Sfx::Whoosh, 0.18, 1.6),
                    WeaponId::StarSkipper => play(sound, Sfx::Shimmer, 0.25, 1.4),
                    WeaponId::HaloOrbit => play(sound, Sfx::Shimmer, 0.25, 0.9),
                    WeaponId::LanternAura => {}
                },
                Event::Burst {
                    pos,
                    radius,
                    evolved,
                } => {
                    let c = if evolved {
                        [0.5, 0.7, 1.]
                    } else {
                        [0.7, 0.85, 1.]
                    };
                    self.ring(pos, radius * 0.3, radius * 1.15, 0.38, c, 5.);
                    self.burst(false, pos, c, 14, radius * 3., 4., 0.45);
                    self.burst(true, pos, [0.85, 0.95, 1.], 10, radius * 2., 4., 0.6);
                    self.add_trauma(0.05);
                    play(sound, Sfx::Boom, 0.22, 1.6);
                }
                Event::TideShift { tide } => {
                    self.tide_from = self.tide_to;
                    self.tide_to = usize::from(tide).min(TIDES.len() - 1);
                    self.tide_t = 0.;
                    self.say(Say::Tide(tide), 3.8);
                    play(sound, Sfx::Shimmer, 0.6, 1.);
                }
                Event::EliteSpawn { name, kind } => {
                    self.say(Say::Elite { name, kind }, 3.6);
                    play(sound, Sfx::Warning, 0.4, 1.3);
                }
                Event::SwarmWarning { kind } => {
                    self.say(Say::Swarm(kind), 3.);
                    play(sound, Sfx::Warning, 0.45, 1.1);
                }
                Event::BossWarning { boss } => {
                    self.say(Say::BossWarning(boss), 4.);
                    play(sound, Sfx::Warning, 0.7, 0.8);
                }
                Event::BossSpawn { boss } => {
                    self.boss_intro = Some((boss, 0.));
                    self.add_trauma(0.6);
                    self.zoom_punch = 0.06;
                    play(
                        sound,
                        Sfx::Roar,
                        0.9,
                        if boss == BossKind::Gulpjaw { 0.8 } else { 1.1 },
                    );
                }
                Event::BossPhase { boss, phase } => {
                    self.say(Say::BossPhase(boss, phase), 3.);
                    self.add_trauma(0.4);
                    self.boss_flash = 0.6 * self.flashes();
                    play(sound, Sfx::Roar, 0.75, 1.2);
                }
                Event::BossCast { boss, pattern } => {
                    if boss == BossKind::Gulpjaw {
                        match pattern {
                            0 => {
                                self.say(Say::Lure, 2.);
                                play(sound, Sfx::Magnet, 0.5, 0.5);
                            }
                            10 => {
                                self.add_trauma(0.35);
                                play(sound, Sfx::Roar, 0.6, 1.5);
                            }
                            1 | 3 => {
                                self.say(Say::Charge, 1.6);
                                play(sound, Sfx::Warning, 0.4, 1.6);
                            }
                            _ => {}
                        }
                    } else if pattern == 1 {
                        play(sound, Sfx::Warning, 0.35, 1.8);
                    }
                }
                Event::BossDefeated { boss, pos } => {
                    self.say(Say::BossDown(boss), 4.5);
                    self.add_trauma(1.);
                    self.flash = 0.8 * self.flashes();
                    self.zoom_punch = 0.1;
                    for k in 0..4 {
                        self.ring(
                            pos,
                            20.,
                            160. + k as f32 * 140.,
                            0.8 + k as f32 * 0.25,
                            [1., 0.85, 0.6],
                            10.,
                        );
                    }
                    self.burst(true, pos, [1., 0.9, 0.6], 80, 420., 9., 1.2);
                    self.burst(false, pos, [1., 0.6, 0.9], 80, 380., 6., 1.);
                    self.accept(FeedbackKind::Explosion, pos, 1.5);
                    play(sound, Sfx::PopBig, 1., 0.6);
                    play(sound, Sfx::Boom, 1., 0.7);
                    if boss == BossKind::Gulpjaw {
                        self.victory_glow = 1.;
                    }
                }
                Event::Slam { pos, radius } => {
                    self.ring(pos, radius * 0.5, radius * 1.3, 0.45, [1., 0.45, 0.6], 7.);
                    self.burst(true, pos, [1., 0.5, 0.7], 18, 260., 6., 0.6);
                    self.add_trauma(0.25);
                    play(sound, Sfx::Boom, 0.6, 1.);
                }
                Event::Revive { pos } => {
                    self.say(Say::SecondWind, 3.);
                    self.ring(pos, 10., 300., 0.9, [1., 0.95, 0.7], 12.);
                    self.flash = 0.6 * self.flashes();
                    play(sound, Sfx::Evolve, 0.8, 1.2);
                }
                Event::Evolved { weapon } => {
                    self.say(Say::Evolved(weapon), 4.);
                    play(sound, Sfx::Evolve, 0.8, 1.);
                }
                Event::Picked { card } => {
                    play(
                        sound,
                        Sfx::Select,
                        0.6,
                        if matches!(card, Card::Weapon(_)) {
                            1.
                        } else {
                            1.12
                        },
                    );
                }
                Event::Victory => play(sound, Sfx::Victory, 0.9, 1.),
                Event::Defeat => play(sound, Sfx::Defeat, 0.9, 1.),
            }
        }
    }
}
