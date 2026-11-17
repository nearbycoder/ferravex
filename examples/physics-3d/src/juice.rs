//! Presentation-only juice for the yard: impact sparks, crate hit flashes,
//! spawn and bowl puffs, floor shockwave rings, pulse bursts, camera shake, a
//! cinematic collapse beat, headline toasts and world-anchored pops. Created
//! only from accepted lab events; nothing here can change physics, timing or
//! input.
//!
//! Sparks and flashes age with accepted simulation time (pausing freezes them
//! for inspection). Shake, the beat, rings, pops and toasts age with bounded
//! render time. Reduced motion clears and blocks particles, shake, rings and
//! the collapse push-in/letterbox; reduced flash clears and blocks hit flashes
//! and rings. Text stays, without rising. A preference change never revives an
//! old effect.
use super::look::{self, Glyph, Icon};
use super::quips;
use super::toys::Goal;
use super::{Lab, LabFrame};
use ferravex_play::feedback::FeedbackPolicy;
use ferravex_play::particles::{
    Emitter, EmitterDesc, EmitterTransform, ParticleDesc, ParticleMotion, ParticlePool,
};
use ferravex_render::audio::Surface;
use ferravex_render::icons::Icon as Pict;
use ferravex_render::prelude::*;
use ferravex_render::ui;

const SEED: u64 = 0x5048_5953_3344;
const POPS: usize = 6;
const RINGS: usize = 12;
pub(super) const BEAT: f32 = 1.4;
pub(super) const GOLD: Color = Color::new(1.0, 0.82, 0.36, 1.0);
pub(super) const PINK: Color = Color::new(0.94, 0.47, 0.55, 1.0);
pub(super) const MINT: Color = Color::new(0.55, 0.93, 0.70, 1.0);
/// Floor point under the authored crate stack, where strikes ring out.
const STACK_BASE: Vec3 = Vec3::new(-0.6, 0.05, 0.);

#[derive(Clone, Debug)]
pub(super) struct Pop {
    pub at: Vec3,
    pub text: String,
    pub color: Color,
    pub age: f32,
    pub life: f32,
    pub size: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Priority {
    Info,
    Alert,
    Goal,
}

/// An expanding horizontal shockwave in the yard, in meters.
#[derive(Clone, Copy, Debug)]
pub(super) struct Ring {
    pub at: Vec3,
    pub radius: f32,
    pub color: Color,
    pub age: f32,
    pub life: f32,
}

impl Ring {
    /// Current radius (meters) and opacity, easing out as it fades.
    pub fn look(&self) -> (f32, f32) {
        let t = (self.age / self.life).clamp(0., 1.);
        let grow = 1. - (1. - t).powi(3);
        (self.radius * (0.2 + 0.8 * grow), (1. - t) * (1. - t))
    }
}

/// A headline callout: an optional glyph, at most three words and an
/// optional icon stat (never an explanatory subtitle).
#[derive(Clone, Debug)]
pub(super) struct Toast {
    pub head: String,
    pub glyph: Option<Glyph>,
    pub value: Option<(Glyph, String)>,
    pub color: Color,
    pub age: f32,
    pub life: f32,
    pub priority: Priority,
}

impl Toast {
    pub fn alpha(&self) -> f32 {
        let fade_in = (self.age / 0.12).min(1.);
        let fade_out = ((self.life - self.age) / 0.3).clamp(0., 1.);
        fade_in.min(fade_out)
    }
}

pub(super) struct Juice {
    pub particles: ParticlePool,
    emitter: Emitter,
    flashes: Vec<f32>,
    known: usize,
    pub pops: [Option<Pop>; POPS],
    pub rings: [Option<Ring>; RINGS],
    pub toast: Option<Toast>,
    time: f32,
    trauma: f32,
    beat: Option<(Vec3, f32)>,
    resets: u32,
    motion: f32,
    flash_scale: f32,
}

fn burst_desc(
    color: Color,
    end: Color,
    speed: f32,
    gravity: f32,
    life: f32,
    size: f32,
) -> EmitterDesc {
    EmitterDesc {
        particle: ParticleDesc {
            velocity: [0., speed * 0.4, 0.],
            acceleration: [0., gravity, 0.],
            lifetime: life,
            start_color: [color.r, color.g, color.b, color.a],
            end_color: [end.r, end.g, end.b, 0.],
            start_size: size,
            end_size: size * 0.25,
            ..Default::default()
        },
        motion: ParticleMotion {
            drag: 2.5,
            angular_velocity: 7.,
            ..Default::default()
        },
        velocity_jitter: [speed, speed, speed],
        lifetime_jitter: 0.35,
        size_jitter: 0.4,
        rotation_jitter: std::f32::consts::PI,
        rate: 0.,
        ..Default::default()
    }
}

fn surface_color(surface: Surface) -> Color {
    match surface {
        Surface::Wood => ui::AMBER,
        Surface::Metal => ui::INK,
        _ => ui::CYAN,
    }
}

pub(super) fn motion_scale(policy: FeedbackPolicy) -> f32 {
    if policy.reduced_motion() {
        0.
    } else {
        policy.intensity()
    }
}

impl Juice {
    pub fn new() -> Self {
        Self {
            particles: ParticlePool::new(480),
            emitter: Emitter::new(burst_desc(ui::AMBER, ui::AMBER, 1., 0., 0.4, 0.1), SEED)
                .expect("authored burst"),
            flashes: Vec::new(),
            known: 0,
            pops: Default::default(),
            rings: [None; RINGS],
            toast: None,
            time: 0.,
            trauma: 0.,
            beat: None,
            resets: 0,
            motion: 1.,
            flash_scale: 1.,
        }
    }

    pub fn apply_policy(&mut self, policy: FeedbackPolicy) {
        self.motion = motion_scale(policy);
        self.flash_scale = if policy.reduced_flash() {
            0.
        } else {
            policy.intensity()
        };
        if self.motion == 0. {
            self.particles.clear();
            self.trauma = 0.;
            self.beat = None;
        }
        if self.flash_scale == 0. {
            self.flashes.iter_mut().for_each(|f| *f = 0.);
        }
        if !self.rings_enabled() {
            self.rings = [None; RINGS];
        }
    }

    /// Rings move and glow: either reduced preference suppresses them.
    fn rings_enabled(&self) -> bool {
        self.motion > 0. && self.flash_scale > 0.
    }

    /// A floor shockwave growing to `radius` meters; the oldest ring yields.
    pub fn ring(&mut self, at: Vec3, radius: f32, color: Color, life: f32) {
        if !self.rings_enabled() || !at.is_finite() {
            return;
        }
        let slot = self
            .rings
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                self.rings
                    .iter()
                    .enumerate()
                    .max_by(|a, b| {
                        let age = |r: &Option<Ring>| r.map_or(0., |r| r.age / r.life);
                        age(a.1).total_cmp(&age(b.1))
                    })
                    .map_or(0, |(i, _)| i)
            });
        self.rings[slot] = Some(Ring {
            at,
            radius,
            color,
            age: 0.,
            life,
        });
    }

    pub fn known(&self) -> usize {
        self.known
    }

    pub fn rebuild(&mut self, bodies: usize) {
        self.particles.clear();
        self.flashes.clear();
        self.flashes.resize(bodies, 0.);
        self.known = bodies;
        self.pops = Default::default();
        self.rings = [None; RINGS];
        self.trauma = 0.;
        self.beat = None;
    }

    /// Collapse beat envelope 0..=1 and its world focus.
    pub fn beat(&self) -> Option<(Vec3, f32)> {
        let (center, age) = self.beat?;
        let t = (age / BEAT).clamp(0., 1.);
        let envelope = if t < 0.16 {
            (t / 0.16).powf(0.6)
        } else {
            1. - ((t - 0.16) / 0.84).powi(2)
        };
        Some((center, envelope.clamp(0., 1.) * self.motion))
    }

    /// Apply shake and the collapse push-in to a camera pose.
    pub fn camera(&self, eye: Vec3, target: Vec3) -> (Vec3, Vec3) {
        let mut eye = eye;
        let mut target = target;
        if let Some((center, beat)) = self.beat() {
            let focus = target.lerp(center, 0.3 * beat);
            eye = focus + (eye - target) * (1. - 0.16 * beat);
            target = focus;
        }
        let amount = self.trauma * self.trauma * 0.22 * self.motion;
        let offset = vec3(
            (self.time * 47.).sin(),
            (self.time * 39. + 1.1).cos() * 0.7,
            (self.time * 43. + 2.3).sin() * 0.5,
        ) * amount;
        (eye + offset, target + offset)
    }

    pub fn body_flash(&self, index: usize) -> f32 {
        self.flashes.get(index).copied().unwrap_or(0.) * self.flash_scale
    }

    fn burst(&mut self, desc: EmitterDesc, at: Vec3, count: u32) {
        if self.motion == 0. || !at.is_finite() {
            return;
        }
        self.emitter.set_config(desc).expect("authored burst");
        let budget = ((count as f32) * self.motion).ceil().max(1.) as u16;
        let _ = self.emitter.burst_with_budget(
            count,
            budget,
            EmitterTransform::at(at.to_array()),
            &mut self.particles,
        );
    }

    fn kick(&mut self, trauma: f32) {
        if self.motion > 0. {
            self.trauma = (self.trauma + trauma).min(1.);
        }
    }

    pub fn pop(&mut self, at: Vec3, text: String, color: Color, size: f32) {
        let slot = self
            .pops
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                self.pops
                    .iter()
                    .enumerate()
                    .max_by(|a, b| {
                        let age = |p: &Option<Pop>| p.as_ref().map_or(0., |p| p.age);
                        age(a.1).total_cmp(&age(b.1))
                    })
                    .map_or(0, |(i, _)| i)
            });
        self.pops[slot] = Some(Pop {
            at,
            text,
            color,
            age: 0.,
            life: 1.2,
            size,
        });
    }

    pub fn toast(
        &mut self,
        head: &str,
        glyph: Option<Glyph>,
        value: Option<(Glyph, String)>,
        color: Color,
        life: f32,
        priority: Priority,
    ) {
        if self
            .toast
            .as_ref()
            .is_some_and(|t| t.priority > priority && t.age < t.life - 0.3)
        {
            return;
        }
        self.toast = Some(Toast {
            head: head.to_owned(),
            glyph,
            value,
            color,
            age: 0.,
            life,
            priority,
        });
    }

    pub fn on_frame(&mut self, lab: &Lab, frame: &LabFrame) {
        let fun = &lab.fun;
        let events = &fun.events;
        if frame.reset {
            self.rebuild(lab.sandbox.bodies.len());
            self.toast = None;
            self.resets += 1;
            self.toast(
                quips::reset(self.resets),
                Some(Glyph::Tool(Icon::Reset)),
                None,
                ui::INK,
                1.6,
                Priority::Info,
            );
            return;
        }
        let count = lab.sandbox.bodies.len();
        self.flashes.resize(count, 0.);
        if count > self.known {
            for body in &lab.sandbox.bodies[self.known..] {
                let color = if body.heavy {
                    PINK
                } else if body.ball {
                    ui::CYAN
                } else {
                    ui::AMBER
                };
                self.burst(
                    burst_desc(color, ui::INK, 1.6, 0., 0.4, 0.14),
                    body.previous_position,
                    if events.chaos > 0 { 3 } else { 10 },
                );
                // A ring marks a deliberate drop or bowl; CHAOS stays a hail.
                if events.chaos == 0 {
                    self.ring(body.previous_position, 1.6, color, 0.5);
                }
            }
        }
        self.known = count;
        let mut impact_rings = 0;
        for spark in &events.sparks {
            let strength = (spark.strength / 10.).clamp(0.2, 1.);
            // Hard hits also thump a small ring where they land.
            if spark.strength > 6. && impact_rings < 3 {
                impact_rings += 1;
                self.ring(spark.at, 0.6 + strength * 0.8, GOLD, 0.35);
            }
            self.burst(
                burst_desc(
                    GOLD,
                    surface_color(spark.surface),
                    2. + strength * 4.,
                    -9.8,
                    0.5,
                    0.09,
                ),
                spark.at,
                (4. + strength * 10.) as u32,
            );
            if spark.strength > 9. {
                self.kick(0.16);
            }
        }
        for &index in &events.hit {
            if let Some(flash) = self.flashes.get_mut(index)
                && self.flash_scale > 0.
            {
                *flash = 1.;
            }
        }
        if frame.pulse {
            let center = lab.sandbox.pulse_center;
            self.burst(burst_desc(ui::CYAN, ui::INK, 6., 0., 0.6, 0.18), center, 30);
            self.kick(0.35);
            self.pop(
                center + vec3(0., 2.5, 0.),
                quips::pulse(fun.stats.pulses).to_owned(),
                ui::CYAN,
                18.,
            );
        }
        if let Some(at) = events.bowled {
            self.pop(
                at + vec3(0., 1.4, 0.),
                quips::bowl(fun.stats.bowls).to_owned(),
                PINK,
                18.,
            );
        }
        if let Some(knocked) = events.bowl_result {
            let (head, tier) = quips::bowl_call(knocked);
            let color = [ui::MUTED, ui::INK, ui::CYAN, GOLD][tier as usize];
            self.toast(
                head,
                Some(Glyph::Tool(Icon::Bowl)),
                Some((Glyph::Pict(Pict::Crate), knocked.to_string())),
                color,
                2.2,
                Priority::Alert,
            );
            if tier == 3 {
                self.kick(0.3);
                self.ring(STACK_BASE, 3.2, GOLD, 0.7);
            }
        }
        if events.chaos > 0 {
            self.toast(
                quips::chaos(fun.stats.chaos),
                None,
                Some((Glyph::Tool(Icon::Chaos), format!("+{}", events.chaos))),
                PINK,
                1.6,
                Priority::Info,
            );
        }
        if let Some((center, _rumble)) = events.collapse {
            self.toast(
                quips::collapse(fun.stats.collapses),
                Some(Glyph::Pict(Pict::Bomb)),
                None,
                GOLD,
                1.8,
                Priority::Info,
            );
            self.ring(
                vec3(center.x, 0.05, center.z),
                3.6,
                Color::new(0.8, 0.84, 0.9, 0.8),
                0.8,
            );
            self.kick(0.55);
            if self.motion > 0. {
                self.beat = Some((center, 0.));
            }
            self.burst(
                burst_desc(
                    Color::new(0.75, 0.78, 0.8, 0.6),
                    ui::MUTED,
                    2.5,
                    0.8,
                    1.1,
                    0.45,
                ),
                center,
                30,
            );
        }
        if events.returned > 0 {
            self.pop(
                vec3(0., 10., 0.),
                quips::returned(fun.stats.returns).to_owned(),
                MINT,
                18.,
            );
            self.ring(vec3(0., 10., 0.), 1.6, MINT, 0.5);
        }
        for goal in &events.goals {
            self.toast(
                quips::goal(*goal),
                Some(look::goal_glyph(*goal)),
                None,
                GOLD,
                3.,
                Priority::Goal,
            );
            let at = match goal {
                Goal::Airtime => vec3(0., 7., 0.),
                _ => vec3(-0.6, 3., 0.),
            };
            self.ring(vec3(at.x, 0.05, at.z), 5., GOLD, 0.9);
            for (color, dx) in [(GOLD, -2.5), (PINK, 0.), (MINT, 2.5)] {
                self.burst(
                    burst_desc(color, ui::INK, 5., -6., 1.5, 0.2),
                    at + vec3(dx, 0., 0.),
                    24,
                );
            }
            self.kick(0.35);
        }
        if events.secret {
            self.toast(
                quips::SECRET,
                Some(Glyph::Pict(Pict::Trophy)),
                None,
                MINT,
                3.6,
                Priority::Goal,
            );
        }
    }

    pub fn advance_sim(&mut self, dt: f32) {
        if dt <= 0. || !dt.is_finite() {
            return;
        }
        self.particles.update(dt).expect("fixed simulation step");
        for flash in &mut self.flashes {
            *flash = (*flash - dt * 6.).max(0.);
        }
    }

    pub fn advance_render(&mut self, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.1)
        } else {
            0.
        };
        self.time += dt;
        self.trauma = (self.trauma - dt * 2.2).max(0.);
        if let Some((_, age)) = &mut self.beat {
            *age += dt;
            if *age >= BEAT {
                self.beat = None;
            }
        }
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
    }

    pub fn pop_rise(&self, pop: &Pop) -> f32 {
        let t = (pop.age / pop.life).clamp(0., 1.);
        (1. - (1. - t).powi(3)) * 36. * self.motion.min(1.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Commands;

    #[test]
    fn reduced_preferences_clear_moving_and_flashing_effects_without_reviving() {
        let mut juice = Juice::new();
        let mut lab = Lab::new();
        juice.rebuild(lab.sandbox.bodies.len());
        let frame = lab.advance(
            0.,
            Commands {
                chaos: true,
                ..Default::default()
            },
        );
        juice.on_frame(&lab, &frame);
        let mut collapsed = false;
        for _ in 0..360 {
            let frame = lab.advance(1. / 120., Commands::default());
            juice.on_frame(&lab, &frame);
            if lab.fun.events.collapse.is_some() {
                collapsed = true;
                break;
            }
        }
        assert!(collapsed && !juice.particles.is_empty());
        assert!(juice.beat().is_some());
        assert!(juice.rings.iter().any(Option::is_some), "collapse ring");
        let (eye, target) = juice.camera(vec3(10., 10., 10.), Vec3::ZERO);
        assert_ne!((eye, target), (vec3(10., 10., 10.), Vec3::ZERO));
        assert!((0..lab.sandbox.bodies.len()).any(|i| juice.body_flash(i) > 0.));
        juice.apply_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        assert!(juice.particles.is_empty() && juice.beat().is_none());
        assert!(juice.rings.iter().all(Option::is_none));
        juice.ring(Vec3::ZERO, 1., GOLD, 1.);
        assert!(
            juice.rings.iter().all(Option::is_none),
            "blocked while reduced"
        );
        assert_eq!(
            juice.camera(vec3(10., 10., 10.), Vec3::ZERO),
            (vec3(10., 10., 10.), Vec3::ZERO)
        );
        assert!(juice.toast.is_some(), "text stays readable");
        juice.apply_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        assert!((0..lab.sandbox.bodies.len()).all(|i| juice.body_flash(i) == 0.));
        juice.apply_policy(FeedbackPolicy::default());
        assert!(juice.particles.is_empty() && juice.beat().is_none());
        assert!(juice.rings.iter().all(Option::is_none), "no revival");
    }

    #[test]
    fn bowl_calls_toast_once_and_reset_clears_world_effects() {
        let mut juice = Juice::new();
        let mut lab = Lab::new();
        juice.rebuild(lab.sandbox.bodies.len());
        for _ in 0..360 {
            lab.advance(1. / 120., Commands::default());
        }
        // Goal celebrations outrank bowl calls; keep this test on the call.
        lab.fun.goals = [true; 4];
        let frame = lab.advance(
            0.,
            Commands {
                bowl: true,
                ..Default::default()
            },
        );
        juice.on_frame(&lab, &frame);
        assert!(juice.pops.iter().flatten().count() == 1);
        assert!(
            juice.rings.iter().any(Option::is_some),
            "the bowl rolls out of a ring"
        );
        let mut calls = 0;
        for _ in 0..400 {
            let frame = lab.advance(1. / 120., Commands::default());
            juice.on_frame(&lab, &frame);
            if let Some(knocked) = lab.fun.events.bowl_result {
                calls += 1;
                let toast = juice.toast.as_ref().unwrap();
                let (_, count) = toast.value.as_ref().unwrap();
                assert_eq!(count, &knocked.to_string(), "an icon stat, not a sentence");
                assert!(toast.head.split_whitespace().count() <= 3);
            }
        }
        assert_eq!(calls, 1);
        let frame = lab.advance(
            0.,
            Commands {
                reset: true,
                ..Default::default()
            },
        );
        juice.on_frame(&lab, &frame);
        assert!(juice.particles.is_empty() && juice.pops.iter().all(Option::is_none));
        assert!(juice.rings.iter().all(Option::is_none));
        let toast = juice.toast.as_ref().unwrap();
        assert_eq!(toast.glyph, Some(Glyph::Tool(Icon::Reset)));
        assert!(toast.head.split_whitespace().count() <= 3);
    }
}
