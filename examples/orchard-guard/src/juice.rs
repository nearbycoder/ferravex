//! Presentation-only juice: particles, rings, text pops, squash/stretch,
//! screen shake, toasts and ambient orchard life. Everything reacts to the
//! accepted [`Feed`] events recorded by the simulation and ages with
//! presentation time while the orchard is not paused, so nothing here can
//! change combat, timing, the economy or input.
//!
//! Reduced motion clears and blocks particles, rings, shake, squash, tree sway
//! and ambient critters. Reduced flash clears and blocks the gate and heart
//! flashes. Text pops and toasts are informational, so they remain, but they
//! stop rising or bouncing under reduced motion.
use crate::game::{Feed, Game, Kind, MAX_PESTS, ROYAL_ID};
use crate::quips;
use ferravex_play::feedback::FeedbackPolicy;
use ferravex_play::particles::{
    Emitter, EmitterDesc, EmitterTransform, ParticleDesc, ParticleMotion, ParticlePool,
};
use ferravex_render::prelude::{Color, Vec2, vec2};

const SEED: u64 = 0x4f52_4348;
const POPS: usize = 10;
const RINGS: usize = 10;
const GHOSTS: usize = 12;
pub(super) const GHOST_LIFE: f32 = 0.75;
const TOAST_FADE: f32 = 0.3;
/// At most this many young pops stack over one spot.
const MAX_STACK: usize = 3;

/// The text a pop displays: its word, its coin reward, or both.
pub(super) fn pop_label(text: &str, value: Option<u32>) -> String {
    match (text.is_empty(), value) {
        (_, None) => text.to_owned(),
        (true, Some(v)) => format!("+{v}"),
        (false, Some(v)) => format!("{text} +{v}"),
    }
}

/// Bitmap width of a pop label plus its coin icon.
fn pop_width(text: &str, value: Option<u32>, size: f32) -> f32 {
    let chars = pop_label(text, value).chars().count() as f32;
    (chars * 6. - 1.).max(0.) * size / 8. + if value.is_some() { size } else { 0. }
}

pub(super) const GOLD: Color = Color::new(0.97, 0.76, 0.27, 1.);
pub(super) const APPLE: Color = Color::new(0.85, 0.33, 0.24, 1.);
pub(super) const LEAF: Color = Color::new(0.45, 0.62, 0.30, 1.);
pub(super) const ICE: Color = Color::new(0.76, 0.94, 0.95, 1.);
pub(super) const CREAM: Color = super::CREAM;
pub(super) const BEETLE: Color = Color::new(0.65, 0.37, 0.30, 1.);
pub(super) const ZOOMIE: Color = Color::new(0.60, 0.45, 0.32, 1.);

#[derive(Clone, Debug)]
pub(super) struct Pop {
    pub at: Vec2,
    pub text: &'static str,
    pub value: Option<u32>,
    pub color: Color,
    pub age: f32,
    pub life: f32,
    pub size: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Ring {
    pub at: Vec2,
    pub age: f32,
    pub life: f32,
    pub radius: f32,
    pub color: Color,
}

/// A stopped pest flipped onto its back, legs waving, before it fades.
#[derive(Clone, Copy, Debug)]
pub(super) struct Ghost {
    pub at: Vec2,
    pub fast: bool,
    pub royal: bool,
    pub age: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Priority {
    Info,
    Alert,
    Milestone,
}

#[derive(Clone, Debug)]
pub(super) struct Toast {
    pub head: String,
    pub sub: String,
    pub color: Color,
    pub age: f32,
    pub life: f32,
    pub priority: Priority,
}

impl Toast {
    /// 0..=1 opacity with short fades, never a flash.
    pub fn alpha(&self) -> f32 {
        let fade_in = (self.age / 0.15).min(1.);
        let fade_out = ((self.life - self.age) / TOAST_FADE).clamp(0., 1.);
        fade_in.min(fade_out)
    }
}

pub(super) fn motion_scale(policy: FeedbackPolicy) -> f32 {
    if policy.reduced_motion() {
        0.
    } else {
        policy.intensity()
    }
}

fn burst_desc(
    start: Color,
    end: Color,
    up: f32,
    spread: f32,
    gravity: f32,
    life: f32,
    size: f32,
) -> EmitterDesc {
    EmitterDesc {
        particle: ParticleDesc {
            velocity: [0., -up, 0.],
            acceleration: [0., gravity, 0.],
            lifetime: life,
            start_color: [start.r, start.g, start.b, start.a],
            end_color: [end.r, end.g, end.b, 0.],
            start_size: size,
            end_size: size * 0.25,
            ..Default::default()
        },
        motion: ParticleMotion {
            drag: 2.2,
            angular_velocity: 7.,
            ..Default::default()
        },
        velocity_jitter: [spread, spread, 0.],
        lifetime_jitter: 0.3,
        size_jitter: 0.35,
        rotation_jitter: std::f32::consts::PI,
        rate: 0.,
        ..Default::default()
    }
}

pub(super) struct Juice {
    pub particles: ParticlePool,
    emitter: Emitter,
    pub pops: [Option<Pop>; POPS],
    pub rings: [Option<Ring>; RINGS],
    pub ghosts: [Option<Ghost>; GHOSTS],
    pub toast: Option<Toast>,
    /// Per-plot recoil (fire) 0..=1.
    pub recoil: [f32; 8],
    /// Per-plot grow-in after planting or upgrading, seconds remaining.
    pub grow: [f32; 8],
    /// Heart icon wobble after a leak, 0..=1.
    pub hurt: f32,
    /// Coin icon bump after earning, 0..=1.
    pub coin: f32,
    /// Red gate glow after a leak, 0..=1 (reduced flash clears it).
    pub gate: f32,
    /// Presentation clock for ambient motion; frozen under reduced motion.
    pub clock: f32,
    /// Seconds since the run finished; drives the harvest festival.
    pub celebrate: f32,
    trauma: f32,
    stops: u32,
    leaks: u32,
    motion: f32,
    flash_scale: f32,
}

impl Juice {
    pub fn new() -> Self {
        Self {
            particles: ParticlePool::new(320),
            emitter: Emitter::new(burst_desc(GOLD, GOLD, 0., 1., 0., 0.5, 4.), SEED)
                .expect("authored burst"),
            pops: Default::default(),
            rings: [None; RINGS],
            ghosts: [None; GHOSTS],
            toast: None,
            recoil: [0.; 8],
            grow: [0.; 8],
            hurt: 0.,
            coin: 0.,
            gate: 0.,
            clock: 0.,
            celebrate: 0.,
            trauma: 0.,
            stops: 0,
            leaks: 0,
            motion: 1.,
            flash_scale: 1.,
        }
    }

    /// Restart and content reload discard every effect and reseed bursts.
    pub fn reset(&mut self) {
        let policy = (self.motion, self.flash_scale);
        *self = Self::new();
        (self.motion, self.flash_scale) = policy;
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
            self.trauma = 0.;
            self.recoil = [0.; 8];
            self.grow = [0.; 8];
            self.clock = 0.;
        }
        if self.flash_scale == 0. {
            self.gate = 0.;
        }
    }

    pub fn motion(&self) -> f32 {
        self.motion
    }

    /// World-space camera offset; zero under reduced motion.
    pub fn shake(&self) -> Vec2 {
        let amount = self.trauma * self.trauma * 9. * self.motion;
        vec2(
            (self.clock * 57.).sin() * amount,
            (self.clock * 43. + 1.1).cos() * amount * 0.7,
        )
    }

    /// Tower draw scale: recoil squash plus an ease-out-back grow-in.
    pub fn tower_scale(&self, plot: usize) -> Vec2 {
        let r = self.recoil.get(plot).copied().unwrap_or(0.) * self.motion;
        let mut scale = vec2(1. + 0.12 * r, 1. - 0.12 * r);
        let left = self.grow.get(plot).copied().unwrap_or(0.);
        if left > 0. {
            let t = 1. - left / 0.35;
            let c = 2.4;
            let pop = 1. + (c + 1.) * (t - 1.).powi(3) + c * (t - 1.).powi(2);
            scale *= (0.3 + 0.7 * pop).max(0.1);
        }
        scale
    }

    fn burst(&mut self, desc: EmitterDesc, at: Vec2, count: u32) {
        if self.motion == 0. {
            return;
        }
        self.emitter.set_config(desc).expect("authored burst");
        let budget = ((count as f32) * self.motion).ceil() as u16;
        self.emitter
            .burst_with_budget(
                count,
                budget,
                EmitterTransform::at([at.x, at.y, 0.]),
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

    /// Queue a world pop. Pops that would overlap a young neighbour stack
    /// upward (by measured width, not slot timing); if a spot is already
    /// holding `MAX_STACK` pops the new one is dropped, so one busy bend can
    /// never print an unreadable pile. Fully deterministic.
    fn pop(&mut self, at: Vec2, text: &'static str, value: Option<u32>, color: Color, size: f32) {
        let width = pop_width(text, value, size);
        let mut at = at;
        let mut placed = false;
        for _ in 0..MAX_STACK {
            let crowded = self.pops.iter().flatten().any(|p| {
                let y = p.at.y - self.rise(p);
                p.age < 0.6
                    && (p.at.x - at.x).abs()
                        < (width + pop_width(p.text, p.value, p.size)) * 0.5 + 8.
                    && (y - at.y).abs() < (size.max(p.size)) + 6.
            });
            if !crowded {
                placed = true;
                break;
            }
            at.y -= size + 8.;
        }
        if !placed {
            return;
        }
        let slot = self
            .pops
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| oldest(self.pops.iter().map(|p| p.as_ref().map(|p| p.age))));
        self.pops[slot] = Some(Pop {
            at,
            text,
            value,
            color,
            age: 0.,
            life: 0.9,
            size,
        });
    }

    pub fn toast(
        &mut self,
        head: String,
        sub: String,
        color: Color,
        life: f32,
        priority: Priority,
    ) {
        if self
            .toast
            .as_ref()
            .is_some_and(|t| t.priority > priority && t.age < t.life - TOAST_FADE)
        {
            return;
        }
        self.toast = Some(Toast {
            head,
            sub,
            color,
            age: 0.,
            life,
            priority,
        });
    }

    fn kick(&mut self, trauma: f32) {
        if self.motion > 0. {
            self.trauma = (self.trauma + trauma).min(1.);
        }
    }

    /// Route one accepted simulation event. Call for every drained [`Feed`].
    pub fn on_event(&mut self, game: &Game, event: Feed) {
        match event {
            Feed::Fired { plot, lucky } => {
                if let Some(slot) = self.recoil.get_mut(plot) {
                    *slot = 1.;
                }
                if lucky && let Some(&at) = game.level.plots.get(plot) {
                    self.ring(at, 30., 0.25, GOLD);
                }
            }
            Feed::Hit { at, kind, lucky } => {
                let (start, end) = match kind {
                    Kind::Seed => (GOLD, LEAF),
                    Kind::Frost => (ICE, CREAM),
                };
                self.burst(
                    burst_desc(start, end, 30., 120., 160., 0.3, 4.),
                    at,
                    if lucky { 14 } else { 5 },
                );
                if lucky {
                    self.pop(
                        at - vec2(0., 30.),
                        if kind == Kind::Seed {
                            "LUCKY BONK!"
                        } else {
                            "BRRRR!"
                        },
                        None,
                        if kind == Kind::Seed { GOLD } else { ICE },
                        22.,
                    );
                    self.ring(at, 26., 0.3, if kind == Kind::Seed { GOLD } else { ICE });
                    self.kick(0.18);
                }
            }
            Feed::Stopped {
                at,
                id,
                fast,
                lucky,
            } => {
                self.stops += 1;
                let royal = id == ROYAL_ID;
                let color = if fast { ZOOMIE } else { BEETLE };
                self.burst(burst_desc(color, LEAF, 90., 170., 420., 0.55, 7.), at, 14);
                self.burst(burst_desc(GOLD, GOLD, 120., 60., 500., 0.5, 5.), at, 5);
                self.coin = 1.;
                let slot = self
                    .ghosts
                    .iter()
                    .position(Option::is_none)
                    .unwrap_or_else(|| oldest(self.ghosts.iter().map(|g| g.map(|g| g.age))));
                self.ghosts[slot] = Some(Ghost {
                    at,
                    fast,
                    royal,
                    age: 0.,
                });
                // One merged pop per stopped pest: the word (when shown) and
                // its coin reward, so the two can never print over each other.
                let word = if !royal && !lucky && (fast || self.stops % 3 == 1) {
                    quips::stopped(self.stops, fast)
                } else {
                    ""
                };
                self.pop(
                    at - vec2(0., 24.),
                    word,
                    Some(game.level.kill_reward),
                    if word.is_empty() { GOLD } else { CREAM },
                    if word.is_empty() { 18. } else { 19. },
                );
                if royal {
                    self.burst(burst_desc(GOLD, APPLE, 260., 260., 300., 1.2, 9.), at, 40);
                    self.ring(at, 80., 0.7, GOLD);
                    self.kick(0.45);
                    self.toast(
                        "SIR CRUNCHALOT DETHRONED!".into(),
                        "THE CROWN ROLLS INTO A FLOWERBED. LONG LIVE THE ORCHARD.".into(),
                        GOLD,
                        3.2,
                        Priority::Milestone,
                    );
                }
            }
            Feed::Escaped { .. } => {
                self.leaks += 1;
                self.hurt = 1.;
                if self.flash_scale > 0. {
                    self.gate = self.flash_scale;
                }
                let gate = *game.level.path.last().unwrap_or(&vec2(1150., 330.));
                let gate = vec2(gate.x.min(1150.), gate.y);
                self.burst(
                    burst_desc(APPLE, APPLE, 160., 140., 420., 0.7, 8.),
                    gate,
                    16,
                );
                self.kick(0.5);
                self.toast(
                    "-1 HEART".into(),
                    quips::leak(self.leaks).into(),
                    APPLE,
                    1.8,
                    Priority::Alert,
                );
            }
            Feed::Planted { plot, kind } => {
                if let Some(&at) = game.level.plots.get(plot) {
                    self.grow[plot.min(7)] = if self.motion > 0. { 0.35 } else { 0. };
                    let color = if kind == Kind::Seed { LEAF } else { ICE };
                    self.burst(
                        burst_desc(
                            Color::new(0.55, 0.43, 0.28, 1.),
                            color,
                            60.,
                            130.,
                            260.,
                            0.5,
                            6.,
                        ),
                        at,
                        16,
                    );
                    self.ring(at, 40., 0.35, CREAM);
                    self.pop(at - vec2(0., 46.), "PLANTED!", None, CREAM, 18.);
                }
            }
            Feed::Grown { plot, level } => {
                if let Some(&at) = game.level.plots.get(plot) {
                    self.grow[plot.min(7)] = if self.motion > 0. { 0.35 } else { 0. };
                    self.burst(burst_desc(GOLD, LEAF, 140., 150., 260., 0.8, 7.), at, 22);
                    self.ring(at, 52., 0.5, GOLD);
                    self.pop(
                        at - vec2(0., 48.),
                        if level >= 2 {
                            "MAX LEVEL!"
                        } else {
                            "LEVEL UP!"
                        },
                        None,
                        GOLD,
                        20.,
                    );
                }
            }
            Feed::Launched { wave } => {
                self.toast(
                    format!("WAVE {wave} INCOMING!"),
                    quips::incoming(wave).into(),
                    CREAM,
                    2.2,
                    Priority::Info,
                );
            }
            Feed::Cleared {
                wave,
                perfect,
                reward,
            } => {
                self.coin = 1.;
                let finale = usize::from(wave) == game.level.waves.len();
                if !finale {
                    let head = if perfect {
                        format!("PERFECT WAVE {wave}! +{reward} COINS")
                    } else {
                        format!("WAVE {wave} CLEARED! +{reward} COINS")
                    };
                    self.toast(
                        head,
                        quips::cleared(wave, perfect).into(),
                        if perfect { GOLD } else { CREAM },
                        2.8,
                        Priority::Milestone,
                    );
                }
                for (i, color) in [APPLE, GOLD, LEAF, CREAM].into_iter().enumerate() {
                    let at = vec2(300. + i as f32 * 200., 380.);
                    self.burst(
                        burst_desc(color, color, 260., 220., 380., 1.1, 8.),
                        at,
                        if perfect { 18 } else { 10 },
                    );
                }
            }
        }
    }

    /// Age effects. Callers pass zero while paused so effects freeze.
    pub fn advance(&mut self, dt: f32, finished: bool) {
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.25)
        } else {
            0.
        };
        if self.motion > 0. {
            self.clock += dt * self.motion;
        }
        if finished {
            let before = self.celebrate;
            self.celebrate += dt;
            // A gentle festival of fruit while the results card is up.
            if (before / 0.9).floor() != (self.celebrate / 0.9).floor() {
                let n = (self.celebrate / 0.9) as u32;
                let side = ((n * 367) % 200) as f32;
                let x = if n.is_multiple_of(2) {
                    60. + side
                } else {
                    940. + side
                };
                let color = [APPLE, GOLD, LEAF, ICE][n as usize % 4];
                self.burst(
                    burst_desc(color, CREAM, 300., 200., 360., 1.2, 8.),
                    vec2(x, 600.),
                    16,
                );
            }
        }
        self.particles
            .update(dt)
            .expect("bounded presentation step");
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
        for slot in &mut self.ghosts {
            if let Some(ghost) = slot {
                ghost.age += dt;
                if ghost.age >= GHOST_LIFE {
                    *slot = None;
                }
            }
        }
        if let Some(toast) = &mut self.toast {
            toast.age += dt;
            if toast.age >= toast.life {
                self.toast = None;
            }
        }
        for r in &mut self.recoil {
            *r = (*r - dt * 7.).max(0.);
        }
        for g in &mut self.grow {
            *g = (*g - dt).max(0.);
        }
        self.hurt = (self.hurt - dt * 2.2).max(0.);
        self.coin = (self.coin - dt * 3.).max(0.);
        self.gate = (self.gate - dt * 2.).max(0.);
        self.trauma = (self.trauma - dt * 2.4).max(0.);
    }

    /// Pop rise in world units; zero under reduced motion.
    pub fn rise(&self, pop: &Pop) -> f32 {
        let t = pop.age / pop.life;
        (1. - (1. - t).powi(2)) * 36. * self.motion
    }

    /// Flash scale from preferences: zero under reduced flash.
    pub fn flash_amount(&self) -> f32 {
        self.flash_scale
    }

    /// Ambient sway for tree `variant`; zero under reduced motion.
    pub fn sway(&self, variant: usize) -> f32 {
        (self.clock * 1.3 + variant as f32 * 0.9).sin() * 1.8 * self.motion
    }
}

/// Index of the oldest occupied slot; slots are all occupied when called.
fn oldest(ages: impl Iterator<Item = Option<f32>>) -> usize {
    ages.enumerate()
        .max_by(|a, b| a.1.unwrap_or(0.).total_cmp(&b.1.unwrap_or(0.)))
        .map_or(0, |(i, _)| i)
}

/// Pests on screen never exceed the simulation cap, so ghosts stay bounded too.
const _: () = assert!(GHOSTS <= MAX_PESTS);

#[cfg(test)]
mod tests {
    use super::*;

    fn stopped_event() -> Feed {
        Feed::Stopped {
            at: vec2(300., 300.),
            id: 3,
            fast: false,
            lucky: false,
        }
    }

    #[test]
    fn reduced_motion_clears_moving_effects_but_keeps_readable_text() {
        let game = Game::new();
        let mut juice = Juice::new();
        juice.apply_policy(FeedbackPolicy::default());
        juice.on_event(&game, stopped_event());
        juice.on_event(&game, Feed::Escaped { id: 4 });
        assert!(!juice.particles.is_empty());
        assert!(juice.shake().length() > 0. || juice.trauma > 0.);
        assert!(juice.gate > 0.);
        juice.apply_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        assert!(juice.particles.is_empty() && juice.trauma == 0.);
        assert_eq!(juice.shake(), Vec2::ZERO);
        assert!(juice.pops.iter().flatten().count() > 0, "text pops stay");
        assert!(juice.toast.is_some(), "toasts stay");
        juice.on_event(&game, stopped_event());
        assert!(juice.particles.is_empty(), "no new motion while reduced");
        let pop = juice.pops.iter().flatten().next().unwrap().clone();
        assert_eq!(juice.rise(&pop), 0.);
        assert_eq!(juice.sway(3), 0.);
        juice.apply_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        assert_eq!(juice.gate, 0., "reduced flash clears the gate glow");
        juice.on_event(&game, Feed::Escaped { id: 5 });
        assert_eq!(juice.gate, 0.);
    }

    #[test]
    fn effects_freeze_at_zero_dt_expire_and_restart_cleanly() {
        let game = Game::new();
        let mut juice = Juice::new();
        juice.on_event(
            &game,
            Feed::Planted {
                plot: 0,
                kind: Kind::Seed,
            },
        );
        juice.on_event(&game, stopped_event());
        let frozen = (juice.particles.len(), juice.grow[0]);
        juice.advance(0., false);
        assert_eq!((juice.particles.len(), juice.grow[0]), frozen);
        for _ in 0..240 {
            juice.advance(1. / 60., false);
        }
        assert!(juice.particles.is_empty());
        assert!(juice.pops.iter().all(Option::is_none));
        assert!(juice.ghosts.iter().all(Option::is_none));
        assert_eq!(juice.tower_scale(0), vec2(1., 1.));
        juice.on_event(&game, Feed::Escaped { id: 9 });
        juice.reset();
        assert!(juice.toast.is_none() && juice.hurt == 0. && juice.stops == 0);
    }

    #[test]
    fn milestones_are_not_stomped_by_wave_info_and_slots_stay_bounded() {
        let game = Game::new();
        let mut juice = Juice::new();
        juice.on_event(
            &game,
            Feed::Cleared {
                wave: 1,
                perfect: true,
                reward: 45,
            },
        );
        juice.on_event(&game, Feed::Launched { wave: 2 });
        assert!(
            juice
                .toast
                .as_ref()
                .unwrap()
                .head
                .starts_with("PERFECT WAVE 1")
        );
        for id in 0..100 {
            juice.on_event(
                &game,
                Feed::Stopped {
                    at: vec2(id as f32, 200.),
                    id,
                    fast: id % 4 == 0,
                    lucky: false,
                },
            );
        }
        assert_eq!(juice.ghosts.iter().flatten().count(), GHOSTS);
        assert!(juice.particles.len() <= juice.particles.capacity());
    }

    #[test]
    fn simultaneous_pops_stack_without_overlap_and_cap_per_spot() {
        let game = Game::new();
        for policy in [
            FeedbackPolicy::default(),
            FeedbackPolicy::new(true, false, 1.).unwrap(),
        ] {
            let mut juice = Juice::new();
            juice.apply_policy(policy);
            for id in 1..=8 {
                juice.on_event(
                    &game,
                    Feed::Stopped {
                        at: vec2(300., 290.),
                        id,
                        fast: id % 2 == 0,
                        lucky: false,
                    },
                );
                juice.on_event(
                    &game,
                    Feed::Hit {
                        at: vec2(304., 292.),
                        kind: Kind::Seed,
                        lucky: true,
                    },
                );
            }
            let pops: Vec<_> = juice.pops.iter().flatten().collect();
            assert!(pops.len() <= 2 * MAX_STACK, "busy spot is capped");
            for (i, a) in pops.iter().enumerate() {
                for b in &pops[i + 1..] {
                    let wa = pop_width(a.text, a.value, a.size);
                    let wb = pop_width(b.text, b.value, b.size);
                    let apart_x = (a.at.x - b.at.x).abs() >= (wa + wb) * 0.5;
                    let apart_y = (a.at.y - b.at.y).abs() >= a.size.max(b.size);
                    assert!(apart_x || apart_y, "{} / {}", a.text, b.text);
                }
            }
            assert!(
                pops.iter().any(|p| !p.text.is_empty() && p.value.is_some()),
                "word and coin share one pop"
            );
        }
    }
}
