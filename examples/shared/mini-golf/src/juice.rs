//! Presentation-only juice for both views: particles, rings, rising callouts,
//! ball squash/stretch, a rolling trail, the cup drop, screen kick and flash.
//!
//! Effects are created only from accepted rules events and live in course
//! coordinates (`x`, height, `z` = course `y`, in metres), so the 2D and 3D
//! views draw the same moments through their own projection. While the ball
//! rolls they age with accepted fixed ticks; after a hole ends the rules are
//! frozen, so the celebration ages with clamped render time instead. Nothing
//! here can change collision, scoring or input.
//!
//! Reduced motion clears and blocks particles, rings, trail, shake, squash and
//! the bouncing headline. Reduced flash clears and blocks the screen flash.
//! Text callouts are information, so they stay, but no longer rise or bounce.
use crate::card::{HoleOutcome, ScoreName, Style, Tone};
use crate::quips;
use ferravex_play::feedback::FeedbackPolicy;
use ferravex_play::particles::{
    Emitter, EmitterDesc, EmitterTransform, ParticleDesc, ParticleMotion, ParticlePool,
};
use ferravex_render::prelude::{Color, Vec2, Vec3, vec2, vec3};

const SEED: u64 = 0x434f_5050;
const POPS: usize = 6;
const RINGS: usize = 6;
pub const TRAIL: usize = 12;
const GRAVITY: f32 = -9.;

pub const PAPER: Color = Color::new(246. / 255., 239. / 255., 218. / 255., 1.);
pub const GOLD: Color = Color::new(232. / 255., 193. / 255., 109. / 255., 1.);
pub const COPPER: Color = Color::new(199. / 255., 111. / 255., 65. / 255., 1.);
/// Lighter copper that keeps 4.5:1 contrast on the dark panel.
pub const CLAY: Color = Color::new(240. / 255., 156. / 255., 112. / 255., 1.);
pub const MINT: Color = Color::new(150. / 255., 222. / 255., 160. / 255., 1.);
pub const WATER: Color = Color::new(120. / 255., 196. / 255., 222. / 255., 1.);
pub const STONE: Color = Color::new(225. / 255., 214. / 255., 184. / 255., 1.);
pub const GRASS: Color = Color::new(160. / 255., 205. / 255., 150. / 255., 1.);

/// Color for a result tone, shared by callouts, the card and the HUD.
pub fn tone_color(tone: Tone) -> Color {
    match tone {
        Tone::Ace => GOLD,
        Tone::Under => MINT,
        Tone::Even => PAPER,
        Tone::Over => CLAY,
        Tone::Rough => CLAY,
    }
}

#[derive(Clone, Debug)]
pub struct Pop {
    pub at: Vec3,
    pub text: String,
    pub color: Color,
    pub age: f32,
    pub life: f32,
    pub size: f32,
    /// Screen rows above the anchor, so stacked callouts never overlap.
    pub row: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct Ring {
    pub at: Vec2,
    pub age: f32,
    pub life: f32,
    pub radius: f32,
    pub color: Color,
}

/// A full-width headline for rare moments: aces, splashes, the hidden trick.
#[derive(Clone, Debug)]
pub struct Banner {
    pub head: String,
    pub sub: String,
    pub color: Color,
    pub age: f32,
    pub life: f32,
    /// A finished hole's score name: it hands its headline to the sheet.
    pub score: bool,
}

/// Render seconds a score headline celebrates alone before the sheet takes it.
pub const HANDOFF: f32 = 0.9;
/// Cross-fade length of that hand-off.
pub const HANDOFF_FADE: f32 = 0.3;

impl Banner {
    /// 0..=1 opacity with short fades, never a flash.
    pub fn alpha(&self) -> f32 {
        let fade_in = (self.age / 0.12).min(1.);
        let fade_out = ((self.life - self.age) / 0.3).clamp(0., 1.);
        fade_in.min(fade_out)
    }
}

pub struct Juice {
    pub particles: ParticlePool,
    emitter: Emitter,
    pub pops: [Option<Pop>; POPS],
    pub rings: [Option<Ring>; RINGS],
    pub banner: Option<Banner>,
    /// Recent rolling positions with their ages, newest last.
    pub trail: [(Vec2, f32); TRAIL],
    pub trail_len: usize,
    trail_tick: u8,
    /// Accumulated presentation time; drives deterministic shake.
    pub time: f32,
    trauma: f32,
    /// Positive squashes against a wall; negative stretches along a putt.
    pub squash: f32,
    pub squash_dir: Vec2,
    /// Seconds since the ball dropped, while the cup animation runs.
    pub sink: Option<f32>,
    /// Flag wave amplitude after a drop, 0..=1.
    pub flag: f32,
    /// Screen tint after an ace, 0..=1.
    pub flash: f32,
    /// Render seconds since the current hole ended.
    pub celebrate: f32,
    /// Whether the current celebration is a hole in one.
    pub ace: bool,
    motion: f32,
    flash_scale: f32,
}

fn burst(
    start: Color,
    end: Color,
    velocity: Vec3,
    jitter: Vec3,
    gravity: f32,
    life: f32,
    size: f32,
) -> EmitterDesc {
    EmitterDesc {
        particle: ParticleDesc {
            velocity: velocity.to_array(),
            acceleration: [0., gravity, 0.],
            lifetime: life,
            start_color: [start.r, start.g, start.b, start.a],
            end_color: [end.r, end.g, end.b, 0.],
            start_size: size,
            end_size: size * 0.3,
            ..Default::default()
        },
        motion: ParticleMotion {
            drag: 1.6,
            angular_velocity: 8.,
            ..Default::default()
        },
        velocity_jitter: jitter.to_array(),
        lifetime_jitter: 0.3,
        size_jitter: 0.35,
        rotation_jitter: std::f32::consts::PI,
        rate: 0.,
        ..Default::default()
    }
}

/// Motion scale for a policy: zero when reduced motion is requested.
pub fn motion_scale(policy: FeedbackPolicy) -> f32 {
    if policy.reduced_motion() {
        0.
    } else {
        policy.intensity()
    }
}

impl Default for Juice {
    fn default() -> Self {
        Self::new()
    }
}

impl Juice {
    pub fn new() -> Self {
        Self {
            particles: ParticlePool::new(320),
            emitter: Emitter::new(burst(GOLD, GOLD, Vec3::ZERO, Vec3::ONE, 0., 0.5, 0.1), SEED)
                .expect("authored burst"),
            pops: Default::default(),
            rings: [None; RINGS],
            banner: None,
            trail: [(Vec2::ZERO, f32::INFINITY); TRAIL],
            trail_len: 0,
            trail_tick: 0,
            time: 0.,
            trauma: 0.,
            squash: 0.,
            squash_dir: Vec2::X,
            sink: None,
            flag: 0.,
            flash: 0.,
            celebrate: 0.,
            ace: false,
            motion: 1.,
            flash_scale: 1.,
        }
    }

    /// Restart discards every effect and reseeds bursts, so retries look identical.
    pub fn reset(&mut self) {
        let policy = (self.motion, self.flash_scale);
        *self = Self::new();
        (self.motion, self.flash_scale) = policy;
    }

    /// A new hole keeps policy and the seeded stream but drops old effects.
    pub fn new_hole(&mut self) {
        self.particles.clear();
        self.pops = Default::default();
        self.rings = [None; RINGS];
        self.banner = None;
        self.trail_len = 0;
        self.sink = None;
        self.flag = 0.;
        self.flash = 0.;
        self.celebrate = 0.;
        self.ace = false;
        self.squash = 0.;
        self.trauma = 0.;
    }

    /// Apply live preferences. Disabled channels are cleared, not hidden, so
    /// re-enabling a preference cannot revive an old effect.
    pub fn apply_policy(&mut self, policy: FeedbackPolicy) {
        self.motion = motion_scale(policy);
        self.flash_scale = if policy.reduced_flash() {
            0.
        } else {
            policy.intensity()
        };
        if self.motion == 0. {
            self.particles.clear();
            self.rings = [None; RINGS];
            self.trail_len = 0;
            self.trauma = 0.;
            self.squash = 0.;
            self.flag = 0.;
        }
        if self.flash_scale == 0. {
            self.flash = 0.;
        }
    }

    pub fn motion(&self) -> f32 {
        self.motion
    }

    /// Screen offset in pixels; zero under reduced motion.
    pub fn shake(&self) -> Vec2 {
        let amount = self.trauma * self.trauma * 12. * self.motion;
        vec2(
            (self.time * 53.).sin() * amount,
            (self.time * 41. + 1.3).cos() * amount * 0.7,
        )
    }

    /// Ball scale (along `squash_dir`, across it); identity under reduced motion.
    pub fn ball_scale(&self) -> Vec2 {
        let s = self.squash * self.motion;
        if s >= 0. {
            vec2(1. - 0.32 * s, 1. + 0.22 * s)
        } else {
            vec2(1. - 0.36 * s, 1. + 0.2 * s)
        }
    }

    /// Ball draw scale while dropping into the cup (1 = full, 0 = gone).
    pub fn sink_scale(&self) -> f32 {
        match self.sink {
            None => 1.,
            Some(_) if self.motion == 0. => 0.,
            Some(t) => (1. - t / 0.28).clamp(0., 1.),
        }
    }

    /// Bouncing headline scale for a fresh ace banner.
    pub fn banner_scale(&self) -> f32 {
        let Some(b) = &self.banner else { return 1. };
        if self.motion == 0. {
            return 1.;
        }
        let t = (b.age / 0.35).min(1.);
        let c = 2.4;
        let back = 1. + (c + 1.) * (t - 1.).powi(3) + c * (t - 1.).powi(2);
        (0.6 + 0.4 * back).max(0.2)
    }

    /// 0 while a score headline owns the moment, rising to 1 once the
    /// between-hole sheet shows it instead. Always 1 without a score banner.
    pub fn handoff(&self) -> f32 {
        match &self.banner {
            Some(b) if b.score => ((self.celebrate - HANDOFF) / HANDOFF_FADE).clamp(0., 1.),
            _ => 1.,
        }
    }

    pub fn flash_level(&self) -> f32 {
        self.flash * self.flash_scale.min(1.)
    }

    fn emit(&mut self, desc: EmitterDesc, at: Vec3, count: u32) {
        if self.motion == 0. {
            return;
        }
        self.emitter.set_config(desc).expect("authored burst");
        let budget = ((count as f32) * self.motion).ceil().max(1.) as u16;
        self.emitter
            .burst_with_budget(
                count,
                budget,
                EmitterTransform::at(at.to_array()),
                &mut self.particles,
            )
            .expect("finite accepted event position");
    }

    fn ring(&mut self, at: Vec2, radius: f32, life: f32, color: Color) {
        if self.motion == 0. {
            return;
        }
        let slot = self
            .rings
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| oldest(self.rings.iter().map(|r| r.map(|r| r.age))));
        self.rings[slot] = Some(Ring {
            at,
            age: 0.,
            life,
            radius,
            color,
        });
    }

    pub fn pop(&mut self, at: Vec3, text: impl Into<String>, color: Color, size: f32, life: f32) {
        self.pop_slot(at, text.into(), color, size, life);
    }

    /// Store a callout in a free (or the oldest) slot and return that slot.
    fn pop_slot(&mut self, at: Vec3, text: String, color: Color, size: f32, life: f32) -> usize {
        let slot = self
            .pops
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| oldest(self.pops.iter().map(|p| p.as_ref().map(|p| p.age))));
        self.pops[slot] = Some(Pop {
            at,
            text,
            color,
            age: 0.,
            life,
            size,
            row: 0,
        });
        slot
    }

    /// A callout stacked `row` text lines above an anchor shared with others.
    fn pop_row(&mut self, at: Vec3, text: String, color: Color, size: f32, row: u8) {
        let slot = self.pop_slot(at, text, color, size, 1.8);
        if let Some(pop) = &mut self.pops[slot] {
            pop.row = row;
        }
    }

    pub fn banner(
        &mut self,
        head: impl Into<String>,
        sub: impl Into<String>,
        color: Color,
        life: f32,
    ) {
        self.banner = Some(Banner {
            head: head.into(),
            sub: sub.into(),
            color,
            age: 0.,
            life,
            score: false,
        });
    }

    fn kick(&mut self, trauma: f32) {
        if self.motion > 0. {
            self.trauma = (self.trauma + trauma).min(1.);
        }
    }

    /// An accepted putt: stretch along the line and kick up a little turf.
    pub fn putt(&mut self, ball: Vec2, dir: Vec2, power: f32) {
        if self.motion > 0. {
            self.squash = -(0.4 + 0.6 * power);
            self.squash_dir = dir.normalize_or(Vec2::X);
        }
        let back = -dir.normalize_or(Vec2::X) * (0.6 + power);
        self.emit(
            burst(
                GRASS,
                PAPER,
                vec3(back.x, 1.4, back.y),
                vec3(1.1, 0.8, 1.1),
                GRAVITY,
                0.45,
                0.07,
            ),
            vec3(ball.x, 0.05, ball.y),
            4 + (power * 8.) as u32,
        );
        if power > 0.85 {
            self.kick(0.18);
            self.pop(vec3(ball.x, 0.4, ball.y), "FULL SEND!", CLAY, 15., 0.8);
        }
    }

    /// An accepted wall or rail contact this tick.
    pub fn bank(&mut self, ball: Vec2, velocity: Vec2, impact_cms: u16, moving: bool) {
        let strength = (f32::from(impact_cms) / 700.).clamp(0., 1.);
        if strength < 0.08 {
            return;
        }
        if self.motion > 0. {
            self.squash = 0.35 + 0.65 * strength;
            self.squash_dir = velocity.normalize_or(Vec2::X);
        }
        let (a, b) = if moving {
            (GOLD, COPPER)
        } else {
            (STONE, PAPER)
        };
        self.emit(
            burst(
                a,
                b,
                vec3(0., 1.2, 0.),
                vec3(2.2, 1.0, 2.2),
                GRAVITY,
                0.35,
                0.05,
            ),
            vec3(ball.x, 0.15, ball.y),
            3 + (strength * 9.) as u32,
        );
        if strength > 0.4 {
            self.kick(0.08 + 0.22 * strength);
        }
        if moving && strength > 0.3 {
            self.pop(vec3(ball.x, 0.5, ball.y), "CLANG!", GOLD, 15., 0.7);
        }
    }

    pub fn sand(&mut self, ball: Vec2, count: u32) {
        self.emit(
            burst(
                GOLD,
                STONE,
                vec3(0., 1.6, 0.),
                vec3(1.4, 0.6, 1.4),
                GRAVITY,
                0.5,
                0.06,
            ),
            vec3(ball.x, 0.05, ball.y),
            10,
        );
        self.pop(
            vec3(ball.x, 0.5, ball.y),
            quips::sand(count),
            GOLD,
            14.,
            0.9,
        );
    }

    pub fn water(&mut self, ball: Vec2, count: u32) {
        self.emit(
            burst(
                WATER,
                PAPER,
                vec3(0., 3.4, 0.),
                vec3(1.6, 1.4, 1.6),
                GRAVITY,
                0.7,
                0.09,
            ),
            vec3(ball.x, 0.05, ball.y),
            26,
        );
        self.ring(ball, 0.9, 0.6, WATER);
        self.ring(ball, 0.5, 0.45, PAPER);
        self.kick(0.32);
        self.pop(vec3(ball.x, 0.5, ball.y), "SPLASH! +1", WATER, 22., 1.6);
        self.pop(
            vec3(ball.x, 1.05, ball.y),
            quips::water(count),
            PAPER,
            14.,
            1.6,
        );
    }

    pub fn lip_out(&mut self, cup: Vec2, count: u32) {
        self.ring(cup, 0.6, 0.4, CLAY);
        self.kick(0.16);
        self.pop(
            vec3(cup.x, 0.6, cup.y),
            quips::lip_out(count),
            CLAY,
            17.,
            1.1,
        );
    }

    pub fn near_miss(&mut self, ball: Vec2, count: u32) {
        self.pop(
            vec3(ball.x, 0.5, ball.y),
            quips::near(count),
            PAPER,
            15.,
            1.0,
        );
    }

    pub fn returned(&mut self, ball: Vec2, count: u32) {
        self.ring(ball, 0.5, 0.35, PAPER);
        self.pop(
            vec3(ball.x, 0.5, ball.y),
            quips::returned(count),
            PAPER,
            14.,
            1.1,
        );
    }

    /// Sample the rolling ball every other accepted tick for the trail.
    pub fn roll(&mut self, ball: Vec2) {
        if self.motion == 0. {
            return;
        }
        self.trail_tick = self.trail_tick.wrapping_add(1);
        if !self.trail_tick.is_multiple_of(2) {
            return;
        }
        if self.trail_len == TRAIL {
            self.trail.copy_within(1.., 0);
            self.trail_len -= 1;
        }
        self.trail[self.trail_len] = (ball, 0.);
        self.trail_len += 1;
    }

    /// The hole is over: the cup drop, confetti by tone and the callouts.
    pub fn hole_done(&mut self, cup: Vec2, outcome: &HoleOutcome) {
        let tone = outcome.name.tone();
        self.celebrate = 0.;
        self.ace = outcome.name == ScoreName::Ace;
        // Every score name gets the headline first; the sheet takes it after.
        self.banner(
            outcome.name.headline(),
            quips::hole(outcome),
            tone_color(tone),
            HANDOFF + HANDOFF_FADE + 0.3,
        );
        if let Some(b) = &mut self.banner {
            b.score = true;
        }
        if outcome.name == ScoreName::PickedUp {
            return;
        }
        self.sink = Some(0.);
        if self.motion > 0. {
            self.flag = 1.;
        }
        self.ring(cup, 0.7, 0.45, PAPER);
        let base = vec3(cup.x, 0.1, cup.y);
        match tone {
            Tone::Ace => {
                for (color, dx) in [(GOLD, -0.35), (CLAY, 0.), (MINT, 0.35), (PAPER, 0.)] {
                    self.emit(
                        burst(
                            color,
                            color,
                            vec3(dx * 3., 6.8, 0.),
                            vec3(2.8, 1.8, 2.8),
                            GRAVITY,
                            1.7,
                            0.16,
                        ),
                        base,
                        30,
                    );
                }
                self.ring(cup, 1.6, 0.8, GOLD);
                self.ring(cup, 2.6, 1.1, PAPER);
                self.kick(0.75);
                if self.flash_scale > 0. {
                    self.flash = 1.;
                }
            }
            Tone::Under => {
                for color in [MINT, GOLD] {
                    self.emit(
                        burst(
                            color,
                            color,
                            vec3(0., 5., 0.),
                            vec3(2.2, 1.3, 2.2),
                            GRAVITY,
                            1.2,
                            0.13,
                        ),
                        base,
                        20,
                    );
                }
                self.ring(cup, 1.3, 0.6, MINT);
                self.kick(0.35);
            }
            Tone::Even => {
                self.emit(
                    burst(
                        PAPER,
                        GOLD,
                        vec3(0., 3.2, 0.),
                        vec3(1.6, 1., 1.6),
                        GRAVITY,
                        0.9,
                        0.08,
                    ),
                    base,
                    14,
                );
                self.kick(0.15);
            }
            Tone::Over | Tone::Rough => {
                self.emit(
                    burst(
                        STONE,
                        PAPER,
                        vec3(0., 2., 0.),
                        vec3(1., 0.6, 1.),
                        GRAVITY,
                        0.7,
                        0.06,
                    ),
                    base,
                    8,
                );
            }
        }
        let above = vec3(cup.x, 1.45, cup.y);
        let mut row = 0;
        if let Some(line) = quips::style(outcome.style, outcome.count) {
            let banks = match outcome.style {
                Style::Bank(n) | Style::Trick(n) => n,
                Style::Straight => 0,
            };
            let text = if banks > 1 {
                format!("{line} ×{banks}")
            } else {
                line.to_owned()
            };
            self.pop_row(above, text, GOLD, 16., row);
            row += 1;
        }
        if outcome.streak >= 2 {
            self.pop_row(above, quips::streak(outcome.streak), MINT, 15., row);
            row += 1;
        }
        if outcome.assist_found {
            self.pop_row(above, "SECRET: COPPER ASSIST!".into(), GOLD, 18., row);
        }
    }

    /// A great round ends with fireworks over the final cup.
    pub fn finale(&mut self, cup: Vec2) {
        for (color, dx) in [(GOLD, -1.), (MINT, 0.), (CLAY, 1.)] {
            self.emit(
                burst(
                    color,
                    color,
                    vec3(dx, 7.5, 0.),
                    vec3(3.2, 2., 3.2),
                    GRAVITY,
                    2.2,
                    0.16,
                ),
                vec3(cup.x, 0.2, cup.y),
                26,
            );
        }
        self.ring(cup, 3., 1.2, GOLD);
    }

    /// Age effects by accepted simulation time only.
    pub fn advance(&mut self, dt: f32) {
        self.time += dt;
        self.particles.update(dt).expect("fixed simulation step");
        for slot in &mut self.pops {
            if let Some(pop) = slot {
                pop.age += dt;
                if pop.age >= pop.life {
                    *slot = None;
                }
            }
        }
        for slot in &mut self.rings {
            if let Some(ring) = slot {
                ring.age += dt;
                if ring.age >= ring.life {
                    *slot = None;
                }
            }
        }
        if let Some(b) = &mut self.banner {
            b.age += dt;
            if b.age >= b.life {
                self.banner = None;
            }
        }
        for (_, age) in &mut self.trail[..self.trail_len] {
            *age += dt;
        }
        while self.trail_len > 0 && self.trail[0].1 > 0.4 {
            self.trail.copy_within(1.., 0);
            self.trail_len -= 1;
        }
        self.trauma = (self.trauma - dt * 2.2).max(0.);
        self.squash *= (-dt * 12.).exp();
        if self.squash.abs() < 0.01 {
            self.squash = 0.;
        }
        if let Some(t) = &mut self.sink {
            *t += dt;
        }
        self.flag = (self.flag - dt * 0.8).max(0.);
        self.flash = (self.flash - dt * 2.5).max(0.);
    }

    /// The finished-hole celebration uses render time while the rules are
    /// frozen. No simulation state changes.
    pub fn advance_celebration(&mut self, dt: f32) {
        let dt = dt.clamp(0., 0.1);
        self.celebrate += dt;
        self.advance(dt);
    }
}

fn oldest(ages: impl Iterator<Item = Option<f32>>) -> usize {
    ages.enumerate()
        .max_by(|a, b| a.1.unwrap_or(0.).total_cmp(&b.1.unwrap_or(0.)))
        .map_or(0, |(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::RoundStats;
    use crate::game::{DT, Phase, Round};

    fn ace(stats: &mut RoundStats) -> HoleOutcome {
        stats.finish_hole(0, 1, 2, false, 2, false)
    }

    #[test]
    fn reduced_motion_blocks_and_clears_motion_but_keeps_readable_text() {
        let mut juice = Juice::new();
        let mut stats = RoundStats::default();
        juice.hole_done(vec2(14.5, 6.), &ace(&mut stats));
        assert!(!juice.particles.is_empty());
        assert!(juice.trauma > 0. && juice.flash > 0.);
        assert!(juice.banner.is_some());

        juice.apply_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        assert!(juice.particles.is_empty());
        assert_eq!(juice.shake(), Vec2::ZERO);
        assert_eq!(juice.ball_scale(), vec2(1., 1.));
        assert_eq!(juice.banner_scale(), 1.);
        assert_eq!(juice.sink_scale(), 0., "the ball is simply in the cup");
        assert!(juice.flash > 0., "flash is an independent preference");
        assert!(juice.banner.is_some(), "text remains readable");

        juice.putt(vec2(3., 6.), Vec2::X, 1.);
        juice.roll(vec2(3., 6.));
        juice.roll(vec2(3.1, 6.));
        juice.water(vec2(9., 2.), 1);
        assert!(
            juice.particles.is_empty() && juice.trail_len == 0,
            "no new motion"
        );
        assert!(juice.rings.iter().all(Option::is_none));
        juice.apply_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        assert_eq!(juice.flash_level(), 0.);
        juice.apply_policy(FeedbackPolicy::default());
        assert!(
            juice.particles.is_empty() && juice.flash == 0.,
            "nothing revives"
        );
    }

    #[test]
    fn effects_follow_accepted_ticks_and_restart_is_reproducible() {
        let run = || {
            let mut juice = Juice::new();
            let mut round = Round::new();
            let mut stats = RoundStats::default();
            let (dir, power) = round.suggested_shot();
            assert!(round.shoot(dir, power));
            juice.putt(round.ball, dir, power);
            for _ in 0..2400 {
                let e = round.tick();
                if round.phase == Phase::Rolling {
                    juice.roll(round.ball);
                }
                if e.bank {
                    juice.bank(round.ball, round.velocity, e.impact_cms, e.moving_hit);
                }
                if e.holed {
                    let outcome = stats.finish_hole(0, round.strokes, 2, false, 0, false);
                    juice.hole_done(round.course.cup, &outcome);
                    break;
                }
                juice.advance(DT);
            }
            for _ in 0..30 {
                juice.advance_celebration(1. / 60.);
            }
            (
                juice.particles.len(),
                juice.time,
                juice.pops.iter().flatten().count(),
                juice.banner.as_ref().map(|b| b.head.clone()),
                juice.sink,
            )
        };
        let first = run();
        assert_eq!(first, run());
        assert!(first.0 > 0, "the ace throws confetti");
        assert_eq!(first.3.as_deref(), Some("HOLE IN ONE!"));
        assert!(first.4.is_some_and(|t| t > 0.28), "the ball has dropped");

        let mut juice = Juice::new();
        let frozen = (juice.time, juice.particles.len());
        juice.advance(0.);
        assert_eq!((juice.time, juice.particles.len()), frozen);
        juice.banner("A", "B", PAPER, 1.);
        juice.reset();
        assert!(juice.banner.is_none() && juice.particles.is_empty());
    }

    #[test]
    fn every_score_headline_celebrates_first_then_hands_off_to_the_sheet() {
        let mut stats = RoundStats::default();
        for (strokes, picked) in [(1, false), (3, false), (6, false), (12, true)] {
            let mut juice = Juice::new();
            let outcome = stats.finish_hole(2, strokes, 3, picked, 0, false);
            juice.hole_done(vec2(14.5, 6.), &outcome);
            let banner = juice.banner.as_ref().expect("score headline");
            assert!(banner.score);
            assert_eq!(banner.head, outcome.name.headline());
            assert_eq!(banner.sub, quips::hole(&outcome));
            assert_eq!(juice.handoff(), 0., "the headline owns the moment");
            assert!(
                juice.pops.iter().flatten().all(|p| p.text != banner.head),
                "no duplicate headline callout"
            );
            for _ in 0..6 {
                juice.advance_celebration(0.1);
            }
            assert_eq!(juice.handoff(), 0.);
            for _ in 0..8 {
                juice.advance_celebration(0.1);
            }
            assert_eq!(juice.handoff(), 1., "the sheet now shows it");
            for _ in 0..6 {
                juice.advance_celebration(0.1);
            }
            assert!(juice.banner.is_none());
            assert_eq!(juice.handoff(), 1.);
        }
        let mut juice = Juice::new();
        juice.banner("A", "B", PAPER, 2.);
        assert_eq!(juice.handoff(), 1., "other banners never hide the sheet");
    }

    #[test]
    fn bounded_storage_recycles_oldest_callouts_and_trail() {
        let mut juice = Juice::new();
        for i in 0..20 {
            juice.pop(Vec3::ZERO, format!("{i}"), PAPER, 12., 2.);
            juice.advance(0.01);
        }
        assert_eq!(juice.pops.iter().flatten().count(), POPS);
        assert!(juice.pops.iter().flatten().any(|p| p.text == "19"));
        for i in 0..100 {
            juice.roll(vec2(i as f32, 0.));
        }
        assert_eq!(juice.trail_len, TRAIL);
        assert_eq!(juice.trail[TRAIL - 1].0, vec2(99., 0.));
        juice.advance(1.);
        assert_eq!(juice.trail_len, 0, "old trail points fade out");
    }
}
