//! Presentation-only juice: particle bursts, the speed trail, squash and
//! pop-in, world-anchored callouts, banners, camera kicks and the finish
//! confetti. Everything is created from accepted simulation events and aged by
//! accepted simulation time, so pausing, the course picker and Controls freeze
//! it, and nothing here can change physics, timing, medals or records.
//!
//! Reduced motion clears and blocks particles, the trail, squash, pop-in,
//! camera kicks and confetti; callouts stay readable but no longer rise or
//! bounce. Reduced flash clears and blocks the finish/moonstone edge glow.
use crate::{
    CREAM, GOLD, INK, TEAL,
    courses::Course,
    quips::{self, Fall},
    sim::Run,
};
use ferravex_engine::camera::ShakeImpulse;
use ferravex_engine::feedback::FeedbackPolicy;
use ferravex_engine::play::particles::{
    ParticleDesc, ParticleMotion, ParticlePool, Trail, TrailConfig, TrailSample,
};
use ferravex_render::prelude::*;

pub(crate) const VIOLET: Color = Color::new(0.72, 0.56, 0.98, 1.);
pub(crate) const CORAL: Color = Color::new(0.93, 0.45, 0.32, 1.);
pub(crate) const SILVER: Color = Color::new(0.80, 0.84, 0.86, 1.);
pub(crate) const BRONZE: Color = Color::new(0.82, 0.55, 0.34, 1.);
pub(crate) const GREEN: Color = Color::new(0.55, 0.86, 0.62, 1.);
const DUST: Color = Color::new(0.86, 0.74, 0.58, 1.);
const POPS: usize = 6;
/// Render seconds between reaching the arch and the results card.
pub(crate) const CARD_DELAY: f32 = 0.9;
const CONFETTI: [Color; 5] = [GOLD, TEAL, CREAM, CORAL, VIOLET];

pub(crate) fn medal_color(medal: &str) -> Color {
    match medal {
        "GOLD" => GOLD,
        "SILVER" => SILVER,
        _ => BRONZE,
    }
}

/// A short world-anchored callout, drawn in screen space after projection.
#[derive(Clone, Debug)]
pub(crate) struct Pop {
    pub at: Vec3,
    pub text: String,
    pub color: Color,
    pub age: f32,
    pub life: f32,
    pub size: f32,
}

/// An informational banner shown when no countdown/checkpoint/fall notice is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Banner {
    pub head: String,
    pub sub: String,
    pub color: Color,
    pub age: f32,
    pub life: f32,
}

impl Banner {
    pub fn alpha(&self) -> f32 {
        ((self.life - self.age) / 0.25).clamp(0., 1.) * (self.age / 0.1).clamp(0., 1.)
    }
}

/// Counters captured before an accepted tick, compared after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Before {
    pub gems: usize,
    pub falls: u32,
    pub checkpoint: usize,
    pub finished: bool,
    pub pace: &'static str,
}

impl Before {
    pub fn of(run: &Run, course: &Course) -> Self {
        Self {
            gems: run.gems(),
            falls: run.falls,
            checkpoint: run.checkpoint,
            finished: run.finished,
            pace: course.pace(run.elapsed, run.falls).0,
        }
    }
}

pub(crate) struct Juice {
    pub particles: ParticlePool,
    pub trail: Trail,
    pub pops: [Option<Pop>; POPS],
    pub banner: Option<Banner>,
    /// Lines chosen at accepted events and shown by the sim-owned notice.
    pub fall_line: &'static str,
    pub checkpoint_line: &'static str,
    pub split: Option<String>,
    /// Falls since the latest checkpoint, for the stubborn-section quips.
    pub falls_here: u32,
    pub airs: u32,
    /// Render seconds since the finish; `None` while the run is live.
    pub celebrate: Option<f32>,
    /// Accepted simulation seconds; drives sparkle cadence and bobbing.
    pub time: f32,
    squash: f32,
    pop_in: f32,
    sparkle: f32,
    glow: f32,
    seed: u32,
    shake: Option<ShakeImpulse>,
    motion: f32,
    flash: f32,
}

fn color4(c: Color, a: f32) -> [f32; 4] {
    [c.r, c.g, c.b, a]
}

impl Juice {
    pub fn new() -> Self {
        Self {
            particles: ParticlePool::new(640),
            trail: Trail::new(TrailConfig {
                capacity: 96,
                lifetime: 0.4,
                spacing: 0.12,
                teleport_distance: 3.,
                max_per_update: 32,
                fade_width: true,
            })
            .expect("authored trail"),
            pops: Default::default(),
            banner: None,
            fall_line: "",
            checkpoint_line: "",
            split: None,
            falls_here: 0,
            airs: 0,
            celebrate: None,
            time: 0.,
            squash: 0.,
            pop_in: 0.,
            sparkle: 0.,
            glow: 0.,
            seed: 0x4d52_4a55,
            shake: None,
            motion: 1.,
            flash: 1.,
        }
    }

    /// Retry, course changes and reloads discard every effect and reseed, so a
    /// retried run looks identical. Live preferences survive.
    pub fn reset(&mut self) {
        let policy = (self.motion, self.flash);
        *self = Self::new();
        (self.motion, self.flash) = policy;
    }

    /// Apply live preferences every frame. Disabled channels are cleared, not
    /// hidden, so re-enabling a preference cannot revive an old effect.
    pub fn apply_policy(&mut self, policy: FeedbackPolicy) {
        self.motion = if policy.reduced_motion() {
            0.
        } else {
            policy.intensity()
        };
        self.flash = if policy.reduced_flash() {
            0.
        } else {
            policy.intensity()
        };
        if self.motion == 0. {
            self.particles.clear();
            self.trail.clear();
            self.squash = 0.;
            self.pop_in = 0.;
            self.shake = None;
        }
        if self.flash == 0. {
            self.glow = 0.;
        }
    }

    pub fn motion(&self) -> f32 {
        self.motion
    }

    /// Edge glow strength, 0..=1; always zero under reduced flash.
    pub fn glow(&self) -> f32 {
        self.glow * self.flash
    }

    /// A camera kick queued by the latest accepted event, if motion allows.
    pub fn take_shake(&mut self) -> Option<ShakeImpulse> {
        self.shake.take().filter(|_| self.motion > 0.)
    }

    fn kick(&mut self, amount: f32, duration: f32) {
        if self.motion == 0. {
            return;
        }
        let amount = (amount * self.motion).min(1.);
        self.shake = Some(ShakeImpulse {
            translation: vec3(0.22, 0.16, 0.08) * amount,
            rotation: vec3(0.012, 0.01, 0.018) * amount,
            duration,
            frequency: 22.,
        });
    }

    fn next(&mut self) -> f32 {
        // Deterministic presentation jitter in -1..=1; never touches gameplay.
        self.seed = self
            .seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1 << 23) as f32 - 1.
    }

    #[allow(clippy::too_many_arguments)]
    fn burst(
        &mut self,
        at: Vec3,
        count: u32,
        speed: f32,
        up: f32,
        gravity: f32,
        life: f32,
        size: f32,
        colors: &[Color],
    ) {
        if self.motion == 0. {
            return;
        }
        let count = ((count as f32) * self.motion).ceil() as u32;
        for i in 0..count {
            let dir = vec3(self.next(), self.next() * 0.5 + 0.5, self.next()).normalize_or_zero();
            let speed = speed * (0.6 + 0.4 * self.next().abs());
            let color = colors[i as usize % colors.len()];
            let (life, spin, turn) = (
                life * (0.75 + 0.25 * self.next().abs()),
                self.next() * 3.,
                8. * self.next(),
            );
            let _ = self.particles.spawn_with(
                ParticleDesc {
                    position: at.to_array(),
                    velocity: (dir * speed + Vec3::Y * up).to_array(),
                    acceleration: [0., gravity, 0.],
                    lifetime: life,
                    start_color: color4(color, 1.),
                    end_color: color4(color, 0.),
                    start_size: size,
                    end_size: size * 0.2,
                },
                ParticleMotion {
                    drag: 1.6,
                    rotation: spin,
                    angular_velocity: turn,
                },
            );
        }
    }

    /// Flat outward ring in the XZ plane: checkpoint pings and landing dust.
    fn ring(&mut self, at: Vec3, count: u32, radius: f32, speed: f32, life: f32, color: Color) {
        if self.motion == 0. {
            return;
        }
        for i in 0..count {
            let angle = i as f32 / count as f32 * std::f32::consts::TAU;
            let dir = vec3(angle.cos(), 0., angle.sin());
            let _ = self.particles.spawn_with(
                ParticleDesc {
                    position: (at + dir * radius).to_array(),
                    velocity: (dir * speed + Vec3::Y * 0.6).to_array(),
                    acceleration: [0., -1., 0.],
                    lifetime: life,
                    start_color: color4(color, 0.95),
                    end_color: color4(color, 0.),
                    start_size: 0.2,
                    end_size: 0.05,
                },
                ParticleMotion {
                    drag: 2.2,
                    rotation: angle,
                    angular_velocity: 4.,
                },
            );
        }
    }

    pub fn pop(&mut self, at: Vec3, text: String, color: Color, size: f32) {
        // Retire older nearby callouts and stack over fresh ones, so two
        // never print on top of each other.
        for slot in &mut self.pops {
            if slot
                .as_ref()
                .is_some_and(|p| p.age >= 0.35 && p.at.distance(at) < 1.6)
            {
                *slot = None;
            }
        }
        let lift = self
            .pops
            .iter()
            .flatten()
            .filter(|p| p.at.distance(at) < 1.6)
            .count() as f32;
        let at = at + Vec3::Y * (0.75 * lift);
        let slot = self
            .pops
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                // Replace the oldest callout; the array never grows.
                (0..POPS)
                    .max_by(|&a, &b| {
                        let age = |i: usize| self.pops[i].as_ref().map_or(0., |p| p.age);
                        age(a).total_cmp(&age(b))
                    })
                    .unwrap_or(0)
            });
        self.pops[slot] = Some(Pop {
            at,
            text,
            color,
            age: 0.,
            life: 1.0,
            size,
        });
    }

    pub fn banner(&mut self, head: impl Into<String>, sub: impl Into<String>, color: Color) {
        self.banner = Some(Banner {
            head: head.into(),
            sub: sub.into(),
            color,
            age: 0.,
            life: 2.4,
        });
    }

    /// Route one accepted tick's changes. Call after the tick, never on frames
    /// without ticks. `split` names the checkpoint split label when one is new.
    pub fn on_tick(
        &mut self,
        run: &Run,
        course: &Course,
        before: Before,
        split: Option<String>,
        dt: f32,
    ) {
        let events = run.events;
        let p = run.position();
        let r = run.level.tuning.radius;
        if let Some(index) = events.gem {
            let at = run.level.gems[index];
            self.burst(at, 24, 3.4, 1.8, -7., 0.65, 0.28, &[GOLD, CREAM]);
            let (gems, total) = (run.gems(), run.level.gems.len());
            if gems == total {
                self.pop(at, "ALL CRYSTALS!".into(), GOLD, 26.);
                self.banner("ALL CRYSTALS!", "THE FINISH ARCH IS OPEN. GO GO GO!", GOLD);
            } else {
                self.pop(at, format!("{gems}/{total}"), GOLD, 26.);
            }
        }
        if run.checkpoint > before.checkpoint {
            let marker = run.level.checkpoints[run.checkpoint - 1].marker;
            self.ring(marker + Vec3::Y * 0.1, 28, 1.25, 3.4, 0.7, TEAL);
            self.ring(marker + Vec3::Y * 0.2, 16, 0.6, 2.2, 0.55, GOLD);
            self.checkpoint_line = quips::checkpoint(run.checkpoint);
            self.split = split;
            self.falls_here = 0;
        }
        if run.falls > before.falls {
            self.falls_here += 1;
            let kind = if !events.fell {
                Fall::Manual
            } else if run
                .bonked_at
                .is_some_and(|at| run.elapsed - at < 2.5 && at <= run.elapsed)
            {
                Fall::Swept
            } else if run.level.platforms.iter().any(|p| p.friction < 0.2) {
                Fall::Ice
            } else {
                Fall::Void
            };
            self.fall_line = quips::fall(kind, run.falls, self.falls_here);
            self.trail.clear();
            self.squash = 0.;
            self.pop_in = if self.motion > 0. { 0.3 } else { 0. };
            self.ring(p - Vec3::Y * (r * 0.9), 20, 0.5, 2.6, 0.5, CREAM);
            self.kick(0.35, 0.25);
        }
        if let Some((at, change)) = events.bonk {
            let count = run.stats.bonks;
            self.burst(at, 16, 4.5, 0.8, -9., 0.4, 0.14, &[CORAL, CREAM, GOLD]);
            self.pop(at + Vec3::Y * 0.4, quips::bonk(count).into(), CORAL, 28.);
            self.squash = (self.squash + 0.5).min(1.);
            self.kick((0.45 + change * 0.05).min(0.9), 0.3);
        }
        if let Some(air) = events.landed {
            self.airs += 1;
            self.ring(p - Vec3::Y * (r * 0.9), 14, 0.4, 2.4, 0.45, DUST);
            self.squash = (air * 1.4).clamp(0.35, 1.);
            if air >= 0.45 {
                self.pop(
                    p + Vec3::Y * 0.6,
                    format!("{} {air:.1}s", quips::air(self.airs)),
                    CREAM,
                    20.,
                );
                self.kick(0.25, 0.2);
            }
        }
        if events.clutch {
            self.burst(p, 10, 2.2, 1.8, -4., 0.5, 0.14, &[TEAL, CREAM]);
            self.pop(
                p + Vec3::Y * 0.5,
                quips::clutch(run.stats.clutches).into(),
                GREEN,
                22.,
            );
        }
        if events.secret
            && let Some(at) = run.secret
        {
            self.burst(at, 34, 4., 2.4, -6., 0.9, 0.22, &[VIOLET, CREAM, GOLD]);
            self.pop(at, "MOONSTONE!".into(), VIOLET, 24.);
            self.banner(
                "SECRET MOONSTONE!",
                "SHINY! IT'LL BE ON YOUR RESULTS CARD",
                VIOLET,
            );
            self.glow = 0.7;
        }
        let pace = course.pace(run.elapsed, run.falls);
        if !run.finished && pace.0 != before.pace && run.falls == before.falls {
            // A fall shows its own quip; announce time-based slips only.
            self.pace_banner(pace);
        } else if !run.finished && pace.0 != before.pace {
            self.banner = None;
        }
        if run.finished && !before.finished {
            let arch = run.level.finish + Vec3::Y * 2.6;
            self.burst(arch, 90, 6., 3.5, -7., 2., 0.22, &CONFETTI);
            self.burst(run.level.finish, 30, 3., 2., -6., 1.2, 0.18, &[GOLD, CREAM]);
            self.kick(0.4, 0.35);
            self.celebrate = Some(0.);
            self.glow = 0.6;
            self.banner = None;
        }
        // Speed sparkle and trail follow accepted ticks, not render frames.
        let v = run.physics.bodies[run.marble].linvel();
        let speed = vec2(v.x, v.z).length();
        if self.motion > 0. {
            let t = ((speed - 3.2) / 3.).clamp(0., 1.);
            let sample = (t > 0.).then(|| TrailSample {
                position: p.to_array(),
                color: [
                    CREAM.r + (GOLD.r - CREAM.r) * t,
                    CREAM.g + (GOLD.g - CREAM.g) * t,
                    CREAM.b + (GOLD.b - CREAM.b) * t,
                    0.35 + 0.45 * t,
                ],
                width: r * (0.8 + 0.8 * t),
            });
            let _ = self.trail.update(dt, sample);
            if speed > 5.5 {
                self.sparkle -= dt;
                if self.sparkle <= 0. {
                    self.sparkle = 0.05;
                    let jitter = vec3(self.next(), self.next(), self.next()) * r * 0.6;
                    self.burst(p + jitter, 1, 0.6, 0.6, -2., 0.45, 0.12, &[GOLD]);
                }
            }
        }
    }

    fn pace_banner(&mut self, pace: (&'static str, Option<f32>)) {
        match pace {
            ("SILVER", Some(deadline)) => self.banner(
                "GOLD SLIPPED AWAY",
                format!("SILVER STILL ON: FINISH UNDER {deadline:.1}s"),
                SILVER,
            ),
            _ => self.banner("BRONZE IT IS", "NO PRESSURE NOW. ENJOY THE VIEW!", BRONZE),
        }
    }

    /// Age effects by accepted simulation time. `notice_visible` holds the
    /// banner while a countdown/checkpoint/fall notice owns the same space.
    pub fn advance(&mut self, dt: f32, notice_visible: bool) {
        if dt <= 0. {
            return;
        }
        self.time += dt;
        let _ = self.particles.update(dt);
        self.squash = (self.squash - dt * 5.).max(0.);
        self.pop_in = (self.pop_in - dt).max(0.);
        self.glow = (self.glow - dt * 1.2).max(0.);
        for slot in &mut self.pops {
            if let Some(pop) = slot {
                pop.age += dt;
                if pop.age >= pop.life {
                    *slot = None;
                }
            }
        }
        if !notice_visible && let Some(banner) = &mut self.banner {
            banner.age += dt;
            if banner.age >= banner.life {
                self.banner = None;
            }
        }
    }

    /// Celebration time runs on render frames once the run is won, so the
    /// confetti and card delay work while the session itself has stopped.
    pub fn advance_celebration(&mut self, dt: f32) {
        if let Some(t) = &mut self.celebrate {
            *t += dt.clamp(0., 0.1);
            let _ = self.particles.update(dt.clamp(0., 0.1));
            self.glow = (self.glow - dt * 1.2).max(0.);
        }
    }

    /// Whether the results card may appear yet.
    pub fn card_ready(&self) -> bool {
        self.celebrate.is_none_or(|t| t >= CARD_DELAY)
    }

    /// World-axis marble scale and the downward offset that keeps it grounded.
    pub fn marble_scale(&self) -> Vec3 {
        let s = self.squash * self.motion;
        let mut scale = vec3(1. + 0.16 * s, 1. - 0.2 * s, 1. + 0.16 * s);
        if self.pop_in > 0. && self.motion > 0. {
            // Ease-out-back from a small seed to full size.
            let t = 1. - self.pop_in / 0.3;
            let c = 2.;
            let pop = 1. + (c + 1.) * (t - 1.).powi(3) + c * (t - 1.).powi(2);
            scale *= (0.2 + 0.8 * pop).max(0.1);
        }
        scale
    }

    /// Analytic, allocation-free screen confetti for the first seconds of a win.
    pub fn draw_confetti(&self, w: f32, h: f32, exclude: Option<Rect>) {
        let Some(t) = self.celebrate else {
            return;
        };
        if self.motion == 0. || t > 3.2 {
            return;
        }
        let fade = (1. - (t - 2.4) / 0.8).clamp(0., 1.);
        for i in 0..56 {
            let f = i as f32;
            let x0 = (f * 0.618_034).fract() * w;
            let speed = 150. + (i % 7) as f32 * 32.;
            let delay = (i % 5) as f32 * 0.07;
            let age = (t - delay).max(0.);
            let y = -16. + age * speed + age * age * 30.;
            if y > h + 16. || age == 0. {
                continue;
            }
            let x = x0 + (age * 3. + f).sin() * 22.;
            if exclude.is_some_and(|r| r.contains(vec2(x, y))) {
                continue;
            }
            let color = CONFETTI[i % CONFETTI.len()];
            draw_poly(
                x,
                y,
                4,
                4. + (i % 3) as f32 * 1.5,
                age * 240. + f * 17.,
                Color::new(color.r, color.g, color.b, 0.95 * fade),
            );
        }
    }

    /// Callouts rise and fade; under reduced motion they stay put and fade.
    pub fn draw_pops(&self, project: impl Fn(Vec3) -> Option<Vec2>) {
        for pop in self.pops.iter().flatten() {
            let Some(at) = project(pop.at) else {
                continue;
            };
            let t = pop.age / pop.life;
            let rise = 46. * t * self.motion.min(1.);
            let scale = if self.motion > 0. {
                1. + 0.35 * (1. - (pop.age / 0.12).min(1.))
            } else {
                1.
            };
            let alpha = ((1. - t) / 0.35).clamp(0., 1.);
            let size = (pop.size * scale).round().max(10.);
            let width = measure_text(&pop.text, None, size as u16, 1.).width;
            let (x, y) = (at.x - width * 0.5, at.y - 24. - rise);
            // A soft drop shadow keeps gold and coral readable over warm stone.
            let shadow = Color::new(INK.r, INK.g, INK.b, 0.75 * alpha);
            let px = (size / 8.).max(1.);
            draw_text(&pop.text, x + px * 0.75, y + px * 0.75, size, shadow);
            draw_text(
                &pop.text,
                x,
                y,
                size,
                Color::new(pop.color.r, pop.color.g, pop.color.b, alpha),
            );
        }
    }
}

/// Project a world point to logical screen pixels; `None` behind the camera.
pub(crate) fn project(view_proj: Mat4, p: Vec3, w: f32, h: f32) -> Option<Vec2> {
    let clip = view_proj * p.extend(1.);
    if clip.w <= 0.05 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    Some(vec2((ndc.x + 1.) * 0.5 * w, (1. - ndc.y) * 0.5 * h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{courses::COURSES, sim::Controls};

    fn tick(juice: &mut Juice, run: &mut Run, controls: Controls) {
        let before = Before::of(run, &COURSES[0]);
        run.step(controls, 1. / 120.);
        juice.on_tick(run, &COURSES[0], before, None, 1. / 120.);
        juice.advance(1. / 120., false);
    }

    #[test]
    fn reduced_motion_blocks_and_clears_motion_effects_but_keeps_callouts() {
        let mut run = Run::new();
        let mut juice = Juice::new();
        juice.apply_policy(FeedbackPolicy::default());
        let gem = run.level.gems[0];
        run.physics.bodies[run.marble].set_translation(
            ferravex_physics::d3::Vector::from_array(gem.to_array()),
            true,
        );
        tick(&mut juice, &mut run, Controls::default());
        assert_eq!(run.gems(), 1);
        assert!(!juice.particles.is_empty(), "accepted pickup bursts");
        assert!(juice.pops.iter().flatten().any(|p| p.text == "1/6"));
        juice.kick(1., 0.2);
        juice.squash = 1.;
        juice.apply_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        assert!(juice.particles.is_empty());
        assert!(juice.take_shake().is_none());
        assert_eq!(juice.marble_scale(), Vec3::ONE);
        assert!(
            juice.pops.iter().flatten().count() == 1,
            "information stays"
        );
        juice.burst(gem, 10, 1., 1., -1., 1., 0.1, &[GOLD]);
        assert!(juice.particles.is_empty(), "no new bursts while reduced");
        juice.apply_policy(FeedbackPolicy::default());
        assert!(juice.particles.is_empty(), "re-enabling revives nothing");
    }

    #[test]
    fn nearby_callouts_retire_or_stack_instead_of_overlapping() {
        let mut juice = Juice::new();
        juice.pop(Vec3::ZERO, "2/8".into(), GOLD, 26.);
        juice.advance(0.5, false);
        juice.pop(vec3(0.3, 0., 0.), "BONK!".into(), CORAL, 28.);
        let live: Vec<_> = juice
            .pops
            .iter()
            .flatten()
            .map(|p| p.text.as_str())
            .collect();
        assert_eq!(live, ["BONK!"], "the older callout retires");
        juice.pop(Vec3::ZERO, "CLUTCH!".into(), GREEN, 22.);
        let heights: Vec<_> = juice.pops.iter().flatten().map(|p| p.at.y).collect();
        assert_eq!(heights.len(), 2);
        assert!(
            (heights[0] - heights[1]).abs() >= 0.75,
            "fresh callouts stack"
        );
    }

    #[test]
    fn reduced_flash_zeroes_glow_independently_of_motion() {
        let mut juice = Juice::new();
        juice.glow = 0.7;
        juice.apply_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        assert_eq!(juice.glow(), 0.);
        juice.apply_policy(FeedbackPolicy::default());
        assert_eq!(juice.glow(), 0., "cleared, not hidden");
        assert!(juice.motion() > 0.);
    }

    #[test]
    fn falls_pick_non_repeating_quips_and_effects_freeze_without_ticks() {
        let mut run = Run::new();
        let mut juice = Juice::new();
        let mut lines = Vec::new();
        for _ in 0..4 {
            run.physics.bodies[run.marble]
                .set_translation(ferravex_physics::d3::Vector::new(40., -20., 0.), true);
            tick(&mut juice, &mut run, Controls::default());
            lines.push(juice.fall_line);
        }
        assert_eq!(run.falls, 4);
        assert!(lines.windows(2).all(|w| w[0] != w[1]), "{lines:?}");
        assert_eq!(juice.falls_here, 4);
        let frozen = (juice.time, juice.particles.len(), juice.marble_scale());
        juice.advance(0., false);
        assert_eq!(
            (juice.time, juice.particles.len(), juice.marble_scale()),
            frozen
        );
    }

    #[test]
    fn pace_banner_announces_a_time_slip_but_yields_to_fall_quips() {
        let mut run = Run::new();
        let mut juice = Juice::new();
        let course = &COURSES[0];
        run.elapsed = course.gold_seconds - 0.001;
        tick(&mut juice, &mut run, Controls::default());
        assert_eq!(juice.banner.as_ref().unwrap().head, "GOLD SLIPPED AWAY");
        let mut fresh = Run::new();
        let mut juice = Juice::new();
        fresh.physics.bodies[fresh.marble]
            .set_translation(ferravex_physics::d3::Vector::new(40., -20., 0.), true);
        tick(&mut juice, &mut fresh, Controls::default());
        assert!(juice.banner.is_none(), "the fall notice explains the slip");
    }

    #[test]
    fn finish_starts_confetti_and_delays_the_card_on_render_time() {
        let mut run = Run::new();
        let mut juice = Juice::new();
        run.collected.fill(true);
        run.physics.bodies[run.marble].set_translation(
            ferravex_physics::d3::Vector::from_array(run.level.finish.to_array()),
            true,
        );
        tick(&mut juice, &mut run, Controls::default());
        assert!(run.finished);
        assert_eq!(juice.celebrate, Some(0.));
        assert!(!juice.card_ready());
        for _ in 0..60 {
            juice.advance_celebration(1. / 60.);
        }
        assert!(juice.card_ready());
        juice.reset();
        assert!(juice.card_ready() && juice.celebrate.is_none());
    }

    #[test]
    fn projection_maps_the_view_target_to_screen_centre() {
        let camera = Camera3D {
            position: vec3(0., 5., 10.),
            target: Vec3::ZERO,
            ..Default::default()
        };
        let m = ferravex_render::Camera::matrix(&camera, 800., 600.);
        let centre = project(m, Vec3::ZERO, 800., 600.).unwrap();
        assert!(centre.abs_diff_eq(vec2(400., 300.), 0.01));
        let above = project(m, vec3(0., 1., 0.), 800., 600.).unwrap();
        assert!(above.y < 300., "world up is screen up");
        assert!(project(m, vec3(0., 5., 20.), 800., 600.).is_none());
    }
}
