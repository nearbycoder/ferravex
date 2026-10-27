//! Presentation-only juice for the 2D lab: impact sparks, hit flashes, spawn
//! pop-ins, pulse bursts, camera shake, a cinematic collapse beat, toasts and
//! world-anchored pops. Everything is created from accepted lab events (see
//! [`super::toys::FunEvents`]) and nothing here can change physics or timing.
//!
//! Sparks, flashes and pop-ins age with accepted simulation time, so pausing
//! freezes them in place for inspection. Shake, the collapse beat, pops and
//! toasts age with bounded render time so text never sticks while paused.
//!
//! Reduced motion clears and blocks particles, shake, pop-ins and the collapse
//! zoom/letterbox. Reduced flash clears and blocks hit flashes. Text remains,
//! but it no longer rises. Changing a preference never revives an old effect.
use super::quips;
use super::toys::{FunEvents, Goal, Scenario};
use super::{Lab, Shape};
use ferravex_play::feedback::FeedbackPolicy;
use ferravex_play::particles::{
    Emitter, EmitterDesc, EmitterTransform, ParticleDesc, ParticleMotion, ParticlePool,
};
use ferravex_render::audio::Surface;
use ferravex_render::prelude::*;
use ferravex_render::ui;

const SEED: u64 = 0x5048_5953_3244;
const POPS: usize = 6;
pub(super) const BEAT: f32 = 1.3;
pub(super) const GOLD: Color = Color::new(1.0, 0.82, 0.36, 1.0);
pub(super) const PINK: Color = Color::new(0.94, 0.47, 0.55, 1.0);
pub(super) const MINT: Color = Color::new(0.55, 0.93, 0.70, 1.0);

#[derive(Clone, Debug)]
pub(super) struct Pop {
    pub at: Vec2,
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
        let fade_out = ((self.life - self.age) / 0.3).clamp(0., 1.);
        fade_in.min(fade_out)
    }
}

pub(super) struct Juice {
    pub particles: ParticlePool,
    emitter: Emitter,
    flashes: Vec<f32>,
    pop_in: Vec<f32>,
    known: usize,
    pub pops: [Option<Pop>; POPS],
    pub toast: Option<Toast>,
    time: f32,
    trauma: f32,
    /// Seconds since the latest collapse beat began, if one is playing.
    beat: Option<(Vec2, f32)>,
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
            velocity: [0., speed * 0.35, 0.],
            acceleration: [0., gravity, 0.],
            lifetime: life,
            start_color: [color.r, color.g, color.b, color.a],
            end_color: [end.r, end.g, end.b, 0.],
            start_size: size,
            end_size: size * 0.25,
            ..Default::default()
        },
        motion: ParticleMotion {
            drag: 3.,
            angular_velocity: 8.,
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
            pop_in: Vec::new(),
            known: 0,
            pops: Default::default(),
            toast: None,
            time: 0.,
            trauma: 0.,
            beat: None,
            resets: 0,
            motion: 1.,
            flash_scale: 1.,
        }
    }

    /// Apply live preferences. Disabled channels are cleared, not hidden.
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
            self.pop_in.iter_mut().for_each(|t| *t = 0.);
        }
        if self.flash_scale == 0. {
            self.flashes.iter_mut().for_each(|f| *f = 0.);
        }
    }

    /// Bodies already introduced (and puffed) by presentation.
    pub fn known(&self) -> usize {
        self.known
    }

    /// A rebuilt scene drops every world-anchored effect; text may remain.
    pub fn rebuild(&mut self, bodies: usize) {
        self.particles.clear();
        self.flashes.clear();
        self.flashes.resize(bodies, 0.);
        self.pop_in.clear();
        self.pop_in.resize(bodies, 0.);
        self.known = bodies;
        self.pops = Default::default();
        self.trauma = 0.;
        self.beat = None;
    }

    /// Screen offset in pixels; zero under reduced motion.
    pub fn shake(&self) -> Vec2 {
        let amount = self.trauma * self.trauma * 9. * self.motion;
        vec2(
            (self.time * 47.).sin() * amount,
            (self.time * 39. + 1.1).cos() * amount * 0.7,
        )
    }

    /// Collapse beat envelope 0..=1 and its world focus.
    pub fn beat(&self) -> Option<(Vec2, f32)> {
        let (center, age) = self.beat?;
        let t = (age / BEAT).clamp(0., 1.);
        // Fast ease in, slow ease out: reads as a slow-motion hold without
        // changing the simulation's fixed tick.
        let envelope = if t < 0.18 {
            (t / 0.18).powf(0.6)
        } else {
            1. - ((t - 0.18) / 0.82).powi(2)
        };
        Some((center, envelope.clamp(0., 1.) * self.motion))
    }

    pub fn body_flash(&self, index: usize) -> f32 {
        self.flashes.get(index).copied().unwrap_or(0.) * self.flash_scale
    }

    /// Spawn pop-in scale for a body: an ease-out-back from 0.55 to 1.
    pub fn body_scale(&self, index: usize) -> f32 {
        let remaining = self.pop_in.get(index).copied().unwrap_or(0.);
        if remaining <= 0. || self.motion == 0. {
            return 1.;
        }
        let t = 1. - remaining / 0.2;
        let c = 2.4;
        let back = 1. + (c + 1.) * (t - 1.).powi(3) + c * (t - 1.).powi(2);
        (0.55 + 0.45 * back).max(0.2)
    }

    fn burst(&mut self, desc: EmitterDesc, at: Vec2, count: u32) {
        if self.motion == 0. || !at.is_finite() {
            return;
        }
        self.emitter.set_config(desc).expect("authored burst");
        let budget = ((count as f32) * self.motion).ceil().max(1.) as u16;
        let _ = self.emitter.burst_with_budget(
            count,
            budget,
            EmitterTransform::at([at.x, at.y, 0.]),
            &mut self.particles,
        );
    }

    fn kick(&mut self, trauma: f32) {
        if self.motion > 0. {
            self.trauma = (self.trauma + trauma).min(1.);
        }
    }

    pub fn pop(&mut self, at: Vec2, text: String, color: Color, size: f32) {
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
            life: 1.1,
            size,
        });
    }

    pub fn toast(&mut self, head: &str, sub: String, color: Color, life: f32, priority: Priority) {
        if self
            .toast
            .as_ref()
            .is_some_and(|t| t.priority > priority && t.age < t.life - 0.3)
        {
            return;
        }
        self.toast = Some(Toast {
            head: head.to_owned(),
            sub,
            color,
            age: 0.,
            life,
            priority,
        });
    }

    /// Route this frame's accepted lab events. `reset` marks a rebuilt scene.
    pub fn on_frame(&mut self, lab: &Lab, reset: bool) {
        let fun = &lab.fun;
        let events: &FunEvents = &fun.events;
        if reset {
            self.rebuild(lab.sandbox.bodies.len());
            self.resets += 1;
            let head = if fun.scenario == Scenario::Dominoes {
                "DOMINO RUN"
            } else {
                "WORKSHOP"
            };
            self.toast(
                head,
                quips::reset(self.resets).to_owned(),
                ui::INK,
                1.8,
                Priority::Info,
            );
            return;
        }
        // Newly spawned bodies pop in with a small puff.
        let count = lab.sandbox.bodies.len();
        self.flashes.resize(count, 0.);
        self.pop_in.resize(count, 0.);
        if count > self.known {
            for index in self.known..count {
                if self.motion > 0. {
                    self.pop_in[index] = 0.2;
                }
                let body = &lab.sandbox.bodies[index];
                let color = match body.shape {
                    Shape::Ball(_) => ui::CYAN,
                    _ => ui::AMBER,
                };
                self.burst(
                    burst_desc(color, ui::INK, 2.4, 0., 0.35, 0.12),
                    body.previous_position,
                    if events.chaos > 0 { 4 } else { 10 },
                );
            }
        }
        self.known = count;

        for spark in &events.sparks {
            let color = surface_color(spark.surface);
            let strength = (spark.strength / 10.).clamp(0.2, 1.);
            self.burst(
                burst_desc(GOLD, color, 2. + strength * 5., -9.8, 0.45, 0.09),
                spark.at,
                (4. + strength * 10.) as u32,
            );
            if spark.strength > 9. {
                self.kick(0.18);
            }
        }
        for &index in &events.hit {
            if let Some(flash) = self.flashes.get_mut(index)
                && self.flash_scale > 0.
            {
                *flash = 1.;
            }
        }
        if lab.pulsed {
            let center = lab.sandbox.pulse_center;
            self.burst(burst_desc(ui::CYAN, ui::INK, 7., 0., 0.5, 0.14), center, 26);
            self.kick(0.32);
            self.pop(
                center + vec2(0., 1.2),
                quips::pulse(fun.stats.pulses).to_owned(),
                ui::CYAN,
                18.,
            );
        }
        if events.chaos > 0 {
            self.toast(
                quips::chaos(fun.stats.chaos),
                format!("+{} SHAPES INBOUND", events.chaos),
                PINK,
                1.8,
                Priority::Info,
            );
        }
        if let Some((center, rumble)) = events.collapse {
            self.toast(
                quips::collapse(fun.stats.collapses),
                format!("COLLAPSE · RUMBLE {rumble}"),
                GOLD,
                2.,
                Priority::Alert,
            );
            self.kick(0.6);
            if self.motion > 0. {
                self.beat = Some((center, 0.));
            }
            self.burst(
                burst_desc(
                    Color::new(0.75, 0.8, 0.85, 0.7),
                    ui::MUTED,
                    3.5,
                    1.5,
                    0.9,
                    0.3,
                ),
                center,
                30,
            );
        }
        if let Some((at, top)) = events.tower {
            self.pop(
                at + vec2(0., 1.),
                format!("{} {:.1} M", quips::tower(top as u32), top),
                GOLD,
                20.,
            );
        }
        if events.returned > 0 {
            self.pop(
                vec2(0., 11.),
                quips::returned(fun.stats.returns).to_owned(),
                MINT,
                18.,
            );
        }
        if let Some(down) = events.dominoes_down
            && down == fun.domino_count()
            && down > 0
        {
            let seconds = fun.domino_seconds().unwrap_or(0.);
            self.toast(
                quips::domino(fun.stats.collapses + down as u32),
                format!("FULL RUN · {down}/{down} IN {seconds:.2} S"),
                MINT,
                2.4,
                Priority::Alert,
            );
        }
        for goal in &events.goals {
            let (head, sub) = quips::goal(*goal);
            self.toast(head, sub.to_owned(), GOLD, 3.2, Priority::Goal);
            let at = match goal {
                Goal::Liftoff => vec2(0., 11.),
                Goal::Skyscraper => vec2(-1.3, 7.5),
                Goal::Demolition => vec2(0., 4.),
                Goal::DominoRun => vec2(5., 2.),
            };
            for (color, dx) in [(GOLD, -3.), (PINK, 0.), (MINT, 3.)] {
                self.burst(
                    burst_desc(color, ui::INK, 6., -6., 1.4, 0.16),
                    at + vec2(dx, 0.),
                    24,
                );
            }
            self.kick(0.35);
        }
        if events.secret {
            let (head, sub) = quips::SECRET;
            self.toast(head, sub.to_owned(), MINT, 4., Priority::Goal);
            for dx in [-6., -2., 2., 6.] {
                self.burst(
                    burst_desc(MINT, GOLD, 6., -4., 1.6, 0.18),
                    vec2(dx, 12.),
                    20,
                );
            }
        }
    }

    /// Age world-anchored effects by accepted simulation time only.
    pub fn advance_sim(&mut self, dt: f32) {
        if dt <= 0. || !dt.is_finite() {
            return;
        }
        self.particles.update(dt).expect("fixed simulation step");
        for flash in &mut self.flashes {
            *flash = (*flash - dt * 7.).max(0.);
        }
        for t in &mut self.pop_in {
            *t = (*t - dt).max(0.);
        }
    }

    /// Age camera and text effects by bounded presentation time.
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
        if let Some(toast) = &mut self.toast {
            toast.age += dt;
            if toast.age >= toast.life {
                self.toast = None;
            }
        }
    }

    /// Rise for a pop in pixels; zero under reduced motion.
    pub fn pop_rise(&self, pop: &Pop) -> f32 {
        let t = (pop.age / pop.life).clamp(0., 1.);
        (1. - (1. - t).powi(3)) * 34. * self.motion.min(1.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LabInput, TICK_HZ, ToyInput};

    fn chaos_lab() -> Lab {
        let mut lab = Lab::new();
        lab.update_toys(
            1. / TICK_HZ,
            LabInput::default(),
            ToyInput {
                chaos: true,
                ..Default::default()
            },
        );
        lab
    }

    #[test]
    fn reduced_motion_and_flash_clear_their_channels_without_reviving() {
        let mut juice = Juice::new();
        let mut lab = chaos_lab();
        juice.rebuild(34);
        juice.on_frame(&lab, false);
        let mut sparked = false;
        for _ in 0..240 {
            lab.update_toys(1. / TICK_HZ, LabInput::default(), ToyInput::default());
            juice.on_frame(&lab, false);
            sparked |= !lab.fun.events.sparks.is_empty();
            if lab.fun.events.collapse.is_some() {
                break;
            }
        }
        assert!(sparked && !juice.particles.is_empty());
        assert!(juice.beat().is_some() && juice.shake() != Vec2::ZERO);
        assert!(juice.toast.is_some());
        assert!((0..lab.sandbox.bodies.len()).any(|i| juice.body_flash(i) > 0.));

        juice.apply_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        assert!(juice.particles.is_empty());
        assert_eq!(juice.shake(), Vec2::ZERO);
        assert!(juice.beat().is_none());
        assert!((0..lab.sandbox.bodies.len()).all(|i| juice.body_scale(i) == 1.));
        assert!(juice.toast.is_some(), "text stays readable");
        juice.apply_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        assert!((0..lab.sandbox.bodies.len()).all(|i| juice.body_flash(i) == 0.));
        juice.apply_policy(FeedbackPolicy::default());
        assert!(juice.particles.is_empty() && juice.beat().is_none());
        assert!((0..lab.sandbox.bodies.len()).all(|i| juice.body_flash(i) == 0.));
    }

    #[test]
    fn pausing_freezes_world_effects_while_text_keeps_aging() {
        let mut juice = Juice::new();
        let lab = chaos_lab();
        juice.rebuild(34);
        juice.on_frame(&lab, false);
        assert!(!juice.particles.is_empty(), "spawn puffs");
        let particles = juice.particles.len();
        juice.toast("A", String::new(), ui::INK, 1., Priority::Info);
        for _ in 0..30 {
            juice.advance_render(0.1);
            juice.advance_sim(0.);
        }
        assert_eq!(
            juice.particles.len(),
            particles,
            "paused sim time freezes sparks"
        );
        assert!(juice.toast.is_none(), "text uses render time");
        juice.advance_sim(1.);
        assert!(juice.particles.is_empty());
    }

    #[test]
    fn goal_toasts_outrank_routine_callouts_and_reset_clears_world_effects() {
        let mut juice = Juice::new();
        juice.toast("GOAL", String::new(), GOLD, 3., Priority::Goal);
        juice.toast("CHAOS", String::new(), PINK, 2., Priority::Info);
        assert_eq!(juice.toast.as_ref().unwrap().head, "GOAL");
        for _ in 0..28 {
            juice.advance_render(0.1);
        }
        juice.toast("CHAOS", String::new(), PINK, 2., Priority::Info);
        assert_eq!(juice.toast.as_ref().unwrap().head, "CHAOS");
        let lab = chaos_lab();
        juice.on_frame(&lab, false);
        juice.on_frame(&Lab::new(), true);
        assert!(juice.particles.is_empty() && juice.beat().is_none());
        assert!(juice.pops.iter().all(Option::is_none));
    }
}
