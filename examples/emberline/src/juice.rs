//! Presentation-only juice: particles, shake, squash/stretch, score pops and
//! toasts. Everything here is created from accepted simulation events and aged
//! by accepted simulation time, so pausing or finishing freezes it and nothing
//! here can change collision, timing, scoring or input.
//!
//! Reduced motion clears and blocks particles, rings, shake, squash and the
//! respawn pop-in. Reduced flash clears and blocks the fall vignette and lantern
//! flare. Text pops and toasts are informational, so they remain, but they no
//! longer rise or bounce under reduced motion.
use super::quips::{self, DeathCause};
use super::{Game, PLAYER, motion_scale};
use ferravex_play::feedback::FeedbackPolicy;
use ferravex_play::particles::{
    Emitter, EmitterDesc, EmitterTransform, ParticleDesc, ParticleMotion, ParticlePool,
};
use ferravex_render::prelude::{Color, Vec2, vec2};

const SEED: u64 = 0x454d_4a55;
const POPS: usize = 6;
const RINGS: usize = 6;
pub(super) const TOAST_FADE: f32 = 0.25;

#[derive(Clone, Debug)]
pub(super) struct Pop {
    pub at: Vec2,
    pub text: String,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Priority {
    Zone,
    Info,
    Alert,
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
        let fade_in = (self.age / 0.12).min(1.);
        let fade_out = ((self.life - self.age) / TOAST_FADE).clamp(0., 1.);
        fade_in.min(fade_out)
    }
}

pub(super) struct Juice {
    pub particles: ParticlePool,
    emitter: Emitter,
    pub pops: [Option<Pop>; POPS],
    pub rings: [Option<Ring>; RINGS],
    pub toast: Option<Toast>,
    /// Accumulated simulated presentation time; drives deterministic shake.
    pub time: f32,
    trauma: f32,
    /// Positive squashes (landing), negative stretches (takeoff).
    pub squash: f32,
    /// Seconds left in the respawn pop-in.
    pub spawn_in: f32,
    /// Red fall vignette level, 0..=1.
    pub flash: f32,
    /// Lantern flare per lantern, 0..=1.
    pub flare: [f32; 2],
    /// Presentation seconds since reaching the lift (render time, Won only).
    pub celebrate: f32,
    motion: f32,
    flash_scale: f32,
}

pub(super) const ORANGE: Color = super::ORANGE;
pub(super) const CREAM: Color = super::CREAM;
pub(super) const GREEN: Color = super::GREEN;
pub(super) const GOLD: Color = Color::new(1.0, 0.80, 0.36, 1.0);
pub(super) const RED: Color = Color::new(0.95, 0.38, 0.28, 1.0);
pub(super) const MOTH: Color = Color::new(0.86, 0.80, 1.0, 1.0);

fn burst_desc(
    start: Color,
    end: Color,
    velocity: [f32; 2],
    jitter: [f32; 2],
    gravity: f32,
    life: f32,
    size: f32,
) -> EmitterDesc {
    EmitterDesc {
        particle: ParticleDesc {
            velocity: [velocity[0], velocity[1], 0.],
            acceleration: [0., gravity, 0.],
            lifetime: life,
            start_color: [start.r, start.g, start.b, start.a],
            end_color: [end.r, end.g, end.b, 0.],
            start_size: size,
            end_size: size * 0.2,
            ..Default::default()
        },
        motion: ParticleMotion {
            drag: 2.5,
            angular_velocity: 6.,
            ..Default::default()
        },
        velocity_jitter: [jitter[0], jitter[1], 0.],
        lifetime_jitter: 0.3,
        size_jitter: 0.35,
        rotation_jitter: std::f32::consts::PI,
        rate: 0.,
        ..Default::default()
    }
}

impl Juice {
    pub fn new() -> Self {
        Self {
            particles: ParticlePool::new(384),
            emitter: Emitter::new(
                burst_desc(ORANGE, ORANGE, [0., 0.], [1., 1.], 0., 0.5, 4.),
                SEED,
            )
            .expect("authored burst"),
            pops: Default::default(),
            rings: [None; RINGS],
            toast: None,
            time: 0.,
            trauma: 0.,
            squash: 0.,
            spawn_in: 0.,
            flash: 0.,
            flare: [0.; 2],
            celebrate: 0.,
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
            self.squash = 0.;
            self.spawn_in = 0.;
        }
        if self.flash_scale == 0. {
            self.flash = 0.;
            self.flare = [0.; 2];
        }
    }

    pub fn motion(&self) -> f32 {
        self.motion
    }

    /// Screen offset in world units; zero under reduced motion.
    pub fn shake(&self) -> Vec2 {
        let amount = self.trauma * self.trauma * 11. * self.motion;
        vec2(
            (self.time * 53.).sin() * amount,
            (self.time * 41. + 1.3).cos() * amount * 0.7,
        )
    }

    /// Player draw scale around the feet: (horizontal, vertical).
    pub fn player_scale(&self) -> Vec2 {
        let s = self.squash * self.motion;
        let mut scale = if s >= 0. {
            vec2(1. + 0.30 * s, 1. - 0.26 * s)
        } else {
            vec2(1. + 0.18 * s, 1. - 0.24 * s)
        };
        if self.spawn_in > 0. {
            // Ease-out-back pop from a small seed back to full size.
            let t = 1. - self.spawn_in / 0.28;
            let c = 2.2;
            let pop = 1. + (c + 1.) * (t - 1.).powi(3) + c * (t - 1.).powi(2);
            scale *= (0.25 + 0.75 * pop).max(0.1);
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

    pub fn pop(&mut self, at: Vec2, text: String, color: Color, size: f32) {
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

    /// Route this accepted tick's events. Call once per accepted tick, after
    /// the simulation update and before [`Self::advance`].
    pub fn on_tick(&mut self, game: &Game) {
        let fb = game.feedback;
        let feet = game.position() + vec2(PLAYER.x * 0.5, PLAYER.y);
        if fb.respawned {
            let (head, color, desc) = match game.death_cause {
                DeathCause::Spikes => (
                    "OUCH!",
                    RED,
                    burst_desc(RED, ORANGE, [0., -190.], [300., 220.], 760., 0.7, 11.),
                ),
                DeathCause::Fissure => (
                    "WHOOPS!",
                    ORANGE,
                    burst_desc(ORANGE, CREAM, [0., -280.], [170., 100.], 260., 0.8, 10.),
                ),
            };
            let at = game.death_at + vec2(PLAYER.x * 0.5, PLAYER.y * 0.5);
            self.burst(desc, at.min(vec2(f32::MAX, 690.)), 34);
            self.kick(0.75);
            if self.flash_scale > 0. {
                self.flash = 0.55 * self.flash_scale;
            }
            self.spawn_in = if self.motion > 0. { 0.28 } else { 0. };
            self.squash = 0.;
            let back = if game.checkpoints.index() == 0 {
                "BACK TO THE TRAILHEAD".to_owned()
            } else {
                format!("BACK TO LANTERN {}", game.checkpoints.index())
            };
            self.toast(
                quips::fall(game.death_cause, game.deaths, game.deaths_here).to_owned(),
                format!("{head} {back}"),
                color,
                1.9,
                Priority::Alert,
            );
            return;
        }
        if fb.jumped {
            self.squash = -1.;
            if fb.air_jumped {
                self.burst(
                    burst_desc(GREEN, CREAM, [0., 140.], [160., 40.], 0., 0.35, 6.),
                    feet,
                    10,
                );
                self.ring(feet, 26., 0.3, GREEN);
            }
        }
        if fb.landed {
            self.squash = 0.4 + 0.6 * game.landing_strength;
            if game.landing_strength > 0.85 {
                self.kick(0.32);
            }
        }
        if fb.pickup_points > 0 {
            let at = fb.pickup_at;
            self.burst(
                burst_desc(GOLD, ORANGE, [0., -80.], [190., 190.], 320., 0.5, 8.),
                at,
                14,
            );
            self.ring(at, 30., 0.32, GOLD);
            self.pop(
                at - vec2(0., 14.),
                format!("+{}", fb.pickup_points),
                GOLD,
                19.,
            );
            if let Some(multiplier) = fb.chain_up {
                self.pop(
                    feet - vec2(0., 70.),
                    format!("CHAIN ×{multiplier}!"),
                    GREEN,
                    23.,
                );
            }
        }
        if fb.air_jump_unlocked {
            self.toast(
                "AIR JUMP UNLOCKED!".to_owned(),
                "PRESS JUMP AGAIN IN MID-AIR · ONCE PER LANDING".to_owned(),
                GREEN,
                2.8,
                Priority::Info,
            );
        }
        if fb.close_call {
            self.pop(
                feet - vec2(0., 64.),
                quips::close_call(game.close_calls).to_owned(),
                CREAM,
                19.,
            );
            self.burst(
                burst_desc(CREAM, ORANGE, [0., -60.], [140., 70.], 200., 0.4, 6.),
                feet,
                8,
            );
        }
        if fb.checkpoint {
            let index = game.checkpoints.index();
            let at = super::lantern_position(index);
            self.burst(
                burst_desc(GOLD, ORANGE, [0., -90.], [230., 200.], 140., 0.9, 9.),
                at,
                26,
            );
            self.ring(at, 70., 0.6, GOLD);
            if self.flash_scale > 0. {
                self.flare[index - 1] = self.flash_scale;
            }
            self.toast(
                format!("LANTERN {index} LIT"),
                quips::lantern(index).to_owned(),
                GOLD,
                2.4,
                Priority::Info,
            );
        }
        if fb.secret {
            self.burst(
                burst_desc(MOTH, GOLD, [0., -40.], [240., 240.], 60., 1.1, 9.),
                super::MOTH_AT,
                34,
            );
            self.ring(super::MOTH_AT, 90., 0.8, MOTH);
            self.toast(
                "SECRET: THE MOTH OF FORTUNE!".to_owned(),
                "+500 AND ETERNAL BRAGGING RIGHTS".to_owned(),
                MOTH,
                3.2,
                Priority::Alert,
            );
        }
        if let Some(split) = fb.split {
            self.toast(
                super::ZONES[split + 1].to_owned(),
                format!("SPLIT {:.2}S", game.trial.elapsed()),
                CREAM,
                1.6,
                Priority::Zone,
            );
        }
        if fb.won {
            for (color, dx) in [(ORANGE, -30.), (GOLD, 0.), (GREEN, 30.)] {
                self.burst(
                    burst_desc(color, CREAM, [0., -450.], [280., 160.], 560., 1.6, 10.),
                    vec2(super::LIFT_X + dx, 470.),
                    22,
                );
            }
            self.kick(0.35);
        }
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
        if let Some(toast) = &mut self.toast {
            toast.age += dt;
            if toast.age >= toast.life {
                self.toast = None;
            }
        }
        self.trauma = (self.trauma - dt * 2.4).max(0.);
        self.squash *= (-dt * 13.).exp();
        if self.squash.abs() < 0.01 {
            self.squash = 0.;
        }
        self.spawn_in = (self.spawn_in - dt).max(0.);
        self.flash = (self.flash - dt * 2.2).max(0.);
        for flare in &mut self.flare {
            *flare = (*flare - dt * 1.4).max(0.);
        }
    }

    /// The results card intro uses render time while the run is frozen. Particles
    /// from the finish keep settling; no simulation state changes.
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
    use crate::{Character, Input, feet};

    const DT: f32 = 1. / 120.;

    fn fall_into_spikes(game: &mut Game) {
        game.motor = Character::new(feet(vec2(360., 520.)));
        game.update(Input::default(), DT);
        assert!(game.feedback.respawned);
    }

    #[test]
    fn reduced_motion_blocks_and_clears_moving_effects_but_keeps_readable_text() {
        let mut juice = Juice::new();
        let mut game = Game::new(true);
        fall_into_spikes(&mut game);
        juice.on_tick(&game);
        assert!(!juice.particles.is_empty());
        assert!(juice.shake() != Vec2::ZERO || juice.trauma > 0.);
        assert!(juice.flash > 0. && juice.spawn_in > 0.);
        assert!(juice.toast.is_some());

        juice.apply_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        assert!(juice.particles.is_empty());
        assert_eq!(juice.shake(), Vec2::ZERO);
        assert_eq!(juice.player_scale(), vec2(1., 1.));
        assert!(juice.flash > 0., "flash is an independent preference");
        assert!(juice.toast.is_some(), "text remains readable");

        fall_into_spikes(&mut game);
        juice.on_tick(&game);
        assert!(juice.particles.is_empty(), "no new motion while reduced");
        juice.apply_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        assert_eq!(juice.flash, 0.);
        juice.apply_policy(FeedbackPolicy::default());
        assert!(
            juice.particles.is_empty() && juice.flash == 0.,
            "nothing revives"
        );
    }

    #[test]
    fn effects_age_with_simulation_time_and_restart_is_reproducible() {
        let run = || {
            let mut juice = Juice::new();
            let mut game = Game::new(true);
            for _ in 0..1200 {
                game.update(game.autoplay(), DT);
                juice.on_tick(&game);
                juice.advance(DT);
                if game.won() {
                    break;
                }
            }
            (
                juice.particles.len(),
                juice.time,
                juice.pops.iter().flatten().count(),
                juice.toast.as_ref().map(|t| t.head.clone()),
            )
        };
        let first = run();
        assert_eq!(first, run());
        assert!(first.0 > 0, "the finish throws confetti");

        let mut juice = Juice::new();
        let frozen = (juice.time, juice.particles.len());
        juice.advance(0.);
        assert_eq!((juice.time, juice.particles.len()), frozen);
        juice.toast("A".into(), "B".into(), CREAM, 1., Priority::Info);
        juice.reset();
        assert!(juice.toast.is_none() && juice.particles.is_empty());
    }

    #[test]
    fn higher_priority_toasts_are_not_stomped_by_zone_banners() {
        let mut juice = Juice::new();
        juice.toast("FALL".into(), String::new(), RED, 2., Priority::Alert);
        juice.toast("ZONE".into(), String::new(), CREAM, 2., Priority::Zone);
        assert_eq!(juice.toast.as_ref().unwrap().head, "FALL");
        juice.advance(1.9);
        juice.toast("ZONE".into(), String::new(), CREAM, 2., Priority::Zone);
        assert_eq!(juice.toast.as_ref().unwrap().head, "ZONE");
    }
}
