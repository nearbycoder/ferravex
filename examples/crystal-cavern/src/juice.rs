//! Presentation-only juice: crystal sparkle bursts, altar columns, seal dust,
//! camera rumble, FOV punch, edge glows, rising text pops, HUD icon bounces,
//! the resonance "hum" and the cave's callouts.
//!
//! Everything here is created from accepted [`Progress`] events and aged by
//! executed expedition time, so pausing or finishing freezes it and nothing here
//! can change movement, objectives, timing or input.
//!
//! Reduced motion clears and blocks particles, rumble, FOV punch, icon bounces
//! and rising pops. Reduced flash clears and blocks edge glows and the hum
//! meter's pulse. Callouts and pops are informational, so they remain readable.
use super::quips;
use super::{Game, Interactable, MINT, Progress, at, world};
use ferravex_play::feedback::FeedbackPolicy;
use ferravex_play::particles::{
    Emitter, EmitterDesc, EmitterTransform, ParticleDesc, ParticleMotion, ParticlePool, SpawnShape,
};
use ferravex_render::prelude::{Color, Vec2, Vec3, vec3};

const SEED: u64 = 0x4341_5645;
const POPS: usize = 4;
pub(super) const CALLOUT_FADE: f32 = 0.35;

pub(super) const GOLD: Color = Color::new(1.0, 0.84, 0.45, 1.0);
pub(super) const VIOLET: Color = Color::new(0.78, 0.6, 1.0, 1.0);
/// The cave's voice. Deliberately distinct from the warm notice/blocker copy.
pub(super) const ECHO: Color = Color::new(0.74, 0.78, 1.0, 1.0);
pub(super) const STONE: Color = Color::new(0.86, 0.84, 0.78, 1.0);

#[derive(Clone, Debug)]
pub(super) struct Pop {
    pub text: String,
    pub color: Color,
    pub age: f32,
    pub life: f32,
    pub size: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Priority {
    Zone,
    Echo,
    Hint,
    Event,
}

#[derive(Clone, Debug)]
pub(super) struct Callout {
    pub head: String,
    pub sub: String,
    pub color: Color,
    pub age: f32,
    pub life: f32,
    pub priority: Priority,
}

impl Callout {
    /// 0..=1 opacity with short fades, never a flash.
    pub fn alpha(&self) -> f32 {
        let fade_in = (self.age / 0.15).min(1.);
        let fade_out = ((self.life - self.age) / CALLOUT_FADE).clamp(0., 1.);
        fade_in.min(fade_out)
    }
}

pub(super) struct Juice {
    pub particles: ParticlePool,
    emitter: Emitter,
    pub pops: [Option<Pop>; POPS],
    pub callout: Option<Callout>,
    /// Accumulated executed presentation time; drives deterministic rumble.
    pub time: f32,
    trauma: f32,
    /// FOV punch, 0..=1.
    kick: f32,
    /// Edge glow level and tint.
    pub flash: f32,
    pub flash_color: Color,
    /// HUD icon bounce: crystals 0..=2, altars 3..=4, Steve 5.
    pub bounce: [f32; 6],
    /// Reticle head-shake after a rejected use.
    pub nope: f32,
    /// Hum meter pulse, 0..=1, and the phase toward the next ping.
    pub ping: f32,
    ping_phase: f32,
    mote_timer: f32,
    motes: u32,
    /// Render seconds on the results card (Won only).
    pub celebrate: f32,
    motion: f32,
    flash_scale: f32,
}

fn burst_desc(
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
            acceleration: [0., -gravity, 0.],
            lifetime: life,
            start_color: [start.r, start.g, start.b, start.a],
            end_color: [end.r, end.g, end.b, 0.],
            start_size: size,
            end_size: size * 0.15,
            ..Default::default()
        },
        motion: ParticleMotion {
            drag: 2.2,
            angular_velocity: 5.,
            ..Default::default()
        },
        shape: SpawnShape::Sphere { radius: 0.18 },
        velocity_jitter: jitter.to_array(),
        lifetime_jitter: 0.3,
        size_jitter: 0.4,
        rotation_jitter: std::f32::consts::PI,
        rate: 0.,
        ..Default::default()
    }
}

impl Juice {
    pub fn new() -> Self {
        Self {
            particles: ParticlePool::new(320),
            emitter: Emitter::new(
                burst_desc(MINT, MINT, Vec3::Y, Vec3::ONE, 0., 0.5, 0.1),
                SEED,
            )
            .expect("authored burst"),
            pops: Default::default(),
            callout: None,
            time: 0.,
            trauma: 0.,
            kick: 0.,
            flash: 0.,
            flash_color: MINT,
            bounce: [0.; 6],
            nope: 0.,
            ping: 0.,
            ping_phase: 0.,
            mote_timer: 0.,
            motes: 0,
            celebrate: 0.,
            motion: 1.,
            flash_scale: 1.,
        }
    }

    /// Retry discards every effect and reseeds bursts, so retries look identical.
    pub fn reset(&mut self) {
        let policy = (self.motion, self.flash_scale);
        *self = Self::new();
        (self.motion, self.flash_scale) = policy;
    }

    /// Apply live preferences. Disabled channels are cleared, not hidden, so
    /// re-enabling a preference cannot revive an old effect.
    pub fn apply_policy(&mut self, policy: FeedbackPolicy) {
        self.motion = if policy.reduced_motion() {
            0.
        } else {
            policy.intensity()
        };
        self.flash_scale = if policy.reduced_flash() {
            0.
        } else {
            policy.intensity()
        };
        if self.motion == 0. {
            self.particles.clear();
            self.trauma = 0.;
            self.kick = 0.;
            self.bounce = [0.; 6];
            self.nope = 0.;
        }
        if self.flash_scale == 0. {
            self.flash = 0.;
            self.ping = 0.;
        }
    }

    pub fn motion(&self) -> f32 {
        self.motion
    }

    /// Camera (yaw, pitch) rumble in radians; zero under reduced motion.
    pub fn shake(&self) -> (f32, f32) {
        let amount = self.trauma * self.trauma * 0.03 * self.motion;
        (
            (self.time * 47.).sin() * amount,
            (self.time * 39. + 1.1).cos() * amount * 0.8,
        )
    }

    /// Field-of-view change in degrees for the pickup punch.
    pub fn fov_punch(&self) -> f32 {
        -self.kick * self.kick * 4.5 * self.motion
    }

    /// Icon draw scale; 1 under reduced motion.
    pub fn icon_scale(&self, index: usize) -> f32 {
        let b = self.bounce[index] * self.motion;
        1. + 0.6 * b * (b * std::f32::consts::PI * 2.).cos().abs()
    }

    /// Horizontal reticle offset in pixels for the rejected-use head shake.
    pub fn nope_offset(&self) -> f32 {
        (self.nope * 30.).sin() * self.nope * 7. * self.motion
    }

    fn burst(&mut self, desc: EmitterDesc, position: Vec3, count: u32) {
        if self.motion == 0. {
            return;
        }
        self.emitter.set_config(desc).expect("authored burst");
        let budget = ((count as f32) * self.motion).ceil().clamp(1., 256.) as u16;
        self.emitter
            .burst_with_budget(
                count,
                budget,
                EmitterTransform::at(position.to_array()),
                &mut self.particles,
            )
            .expect("finite accepted event position");
    }

    pub fn pop(&mut self, text: impl Into<String>, color: Color, size: u16) {
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
            text: text.into(),
            color,
            age: 0.,
            life: 1.1,
            size,
        });
    }

    pub fn call(
        &mut self,
        head: impl Into<String>,
        sub: impl Into<String>,
        color: Color,
        life: f32,
        priority: Priority,
    ) {
        if self
            .callout
            .as_ref()
            .is_some_and(|c| c.priority > priority && c.age < c.life - CALLOUT_FADE)
        {
            return;
        }
        self.callout = Some(Callout {
            head: head.into(),
            sub: sub.into(),
            color,
            age: 0.,
            life,
            priority,
        });
    }

    /// The cave answers: a line plus its own fading echo.
    fn echo(&mut self, line: &str, priority: Priority) {
        self.call(line, quips::echo_tail(line), ECHO, 3.2, priority);
    }

    fn glow(&mut self, color: Color, level: f32) {
        if self.flash_scale > 0. {
            self.flash = (level * self.flash_scale).max(self.flash);
            self.flash_color = color;
        }
    }

    fn rumble(&mut self, trauma: f32) {
        if self.motion > 0. {
            self.trauma = (self.trauma + trauma).min(1.);
        }
    }

    /// Route this frame's accepted events. Call after the simulation update and
    /// before [`Self::advance`].
    pub fn on_progress(&mut self, progress: &Progress, game: &Game, runs: u32) {
        for i in 0..3 {
            if progress.completed & (1 << i) == 0 {
                continue;
            }
            let at = world(game.crystals[i], 0.95);
            self.burst(
                burst_desc(
                    Color::new(0.85, 1., 0.96, 1.),
                    MINT,
                    vec3(0., 2.4, 0.),
                    vec3(2.2, 1.8, 2.2),
                    3.5,
                    0.9,
                    0.05,
                ),
                at,
                48,
            );
            self.burst(
                burst_desc(
                    MINT,
                    GOLD,
                    vec3(0., 3.5, 0.),
                    vec3(0.6, 1.2, 0.6),
                    0.,
                    1.3,
                    0.045,
                ),
                at,
                18,
            );
            self.kick = 1.;
            self.bounce[i] = 1.;
            self.glow(MINT, 0.5);
            // One voice each: the pop counts, the echo jokes, the notice directs.
            let count = game.count();
            if count >= 3 {
                self.pop("ALL 3 CRYSTALS!", GOLD, 30);
            } else {
                self.pop(format!("+1 CRYSTAL  {count} / 3"), MINT, 26);
            }
            self.echo(quips::pickup(count, runs), Priority::Echo);
        }
        for altar in 0..2u8 {
            if progress.completed & (1 << (3 + altar)) == 0 {
                continue;
            }
            let base = at(if altar == 0 { b'A' } else { b'B' });
            self.burst(
                burst_desc(
                    GOLD,
                    MINT,
                    vec3(0., 4.5, 0.),
                    vec3(1.2, 1.6, 1.2),
                    0.5,
                    1.4,
                    0.08,
                ),
                world(base, 0.7),
                64,
            );
            self.bounce[3 + usize::from(altar)] = 1.;
            self.glow(GOLD, 0.45);
            let quip = quips::altar(altar);
            if altar == 1 {
                // The seal is the run's big beat: dust at the gate and a long rumble.
                self.burst(
                    burst_desc(
                        Color::new(0.75, 0.66, 0.55, 0.9),
                        Color::new(0.4, 0.34, 0.3, 0.6),
                        vec3(0., 0.6, 1.2),
                        vec3(2.6, 1.0, 1.4),
                        0.8,
                        1.6,
                        0.12,
                    ),
                    world(at(b'G'), 0.8),
                    70,
                );
                self.rumble(0.75);
                self.pop("SEAL OPEN!", GOLD, 30);
            } else {
                self.rumble(0.4);
                self.pop("ALTAR I AWAKE", GOLD, 26);
            }
            // The pop names the beat; the echo jokes; the notice says what's next.
            self.echo(quip, Priority::Event);
        }
        if progress.won {
            self.glow(Color::new(1., 0.95, 0.8, 1.), 0.7);
        }
        if progress.shy {
            let at = world(super::shy_position(), 0.45);
            self.burst(
                burst_desc(
                    VIOLET,
                    Color::new(1., 0.8, 0.95, 1.),
                    vec3(0., 2.6, 0.),
                    vec3(2.2, 2., 2.2),
                    3.,
                    1.1,
                    0.07,
                ),
                at,
                56,
            );
            self.kick = 1.;
            self.bounce[5] = 1.;
            self.glow(VIOLET, 0.55);
            self.pop("SECRET!", VIOLET, 30);
            self.call(
                "SECRET: STEVE, THE SHY CRYSTAL",
                "He's never been found before. He's thrilled.",
                VIOLET,
                4.,
                Priority::Event,
            );
        }
        if let Some(page) = progress.note {
            self.call(
                if page == 0 {
                    "SECRET: WENDELL'S LOST NOTE"
                } else {
                    "WENDELL'S NOTE"
                },
                format!("Page {} of 4. His hat is right there.", page % 4 + 1),
                STONE,
                4.,
                Priority::Event,
            );
            if page == 0 {
                self.pop("SECRET!", VIOLET, 30);
                self.glow(VIOLET, 0.35);
            }
        }
        if let Some(target) = progress.rejected {
            self.nope = 1.;
            self.echo(
                quips::locked(target == Interactable::Gate, game.rejected),
                Priority::Echo,
            );
        }
        if progress.bonks > 0 {
            self.rumble(0.3);
            self.pop("BONK!", STONE, 26);
            if game.bonks % 2 == 1 {
                self.echo(quips::bonk(game.bonks / 2), Priority::Echo);
            }
        }
        if let Some(zone) = progress.zone {
            let (name, sub) = quips::ZONES[zone];
            self.call(name, sub, STONE, 2.8, Priority::Zone);
        }
        if progress.hint {
            self.call(
                "THE ECHO WHISPERS",
                game.hint_text(),
                ECHO,
                7.,
                Priority::Hint,
            );
        }
    }

    /// Age effects by executed expedition time. Returns resonance pings due this
    /// frame (each a soft audio tick). Zero time changes nothing.
    pub fn advance(&mut self, dt: f32, game: &Game) -> u32 {
        if dt <= 0. || !dt.is_finite() {
            return 0;
        }
        let dt = dt.min(0.25);
        self.time += dt;
        let _ = self.particles.update(dt);
        for slot in &mut self.pops {
            if let Some(pop) = slot {
                pop.age += dt;
                if pop.age >= pop.life {
                    *slot = None;
                }
            }
        }
        if let Some(callout) = &mut self.callout {
            callout.age += dt;
            if callout.age >= callout.life {
                self.callout = None;
            }
        }
        self.trauma = (self.trauma - dt * 1.5).max(0.);
        self.kick = (self.kick - dt * 3.).max(0.);
        self.flash = (self.flash - dt * 1.8).max(0.);
        self.nope = (self.nope - dt * 2.5).max(0.);
        for bounce in &mut self.bounce {
            *bounce = (*bounce - dt * 2.2).max(0.);
        }
        self.ping = (self.ping - dt * 3.).max(0.);
        // The hum: a soft tick that quickens as the nearest crystal gets closer.
        let resonance = game.resonance();
        let mut pings = 0;
        if resonance > 0.2 {
            self.ping_phase += dt / (1.5 - 1.25 * resonance);
            while self.ping_phase >= 1. {
                self.ping_phase -= 1.;
                pings += 1;
                if self.flash_scale > 0. {
                    self.ping = 1.;
                }
            }
        } else {
            self.ping_phase = 0.;
        }
        // Slow rising motes around nearby crystals give the cave a living shimmer.
        self.mote_timer += dt;
        while self.mote_timer >= 0.09 {
            self.mote_timer -= 0.09;
            self.motes = self.motes.wrapping_add(1);
            let index = self.motes as usize % 4;
            let source = if index < 3 {
                (!game.collected[index]).then(|| game.crystals[index])
            } else {
                (!game.shy_found).then(super::shy_position)
            };
            let Some(source) = source else { continue };
            let near = source.distance(game.position) < 12.;
            if !near {
                continue;
            }
            let color = if index < 3 { MINT } else { VIOLET };
            self.burst(
                burst_desc(
                    color,
                    color,
                    vec3(0., 0.45, 0.),
                    vec3(0.5, 0.25, 0.5),
                    0.,
                    1.8,
                    0.06,
                ),
                world(source, if index < 3 { 0.7 } else { 0.25 }),
                1,
            );
        }
        pings
    }

    /// The results card intro uses render time while the run is frozen.
    pub fn advance_celebration(&mut self, dt: f32) {
        self.celebrate += dt.clamp(0., 0.1);
    }

    /// Callout/pop anchor for tests and HUD layout.
    pub fn pop_rise(&self, pop: &Pop) -> f32 {
        let t = pop.age / pop.life;
        (1. - (1. - t).powi(3)) * 70. * self.motion.min(1.)
    }
}

impl Default for Juice {
    fn default() -> Self {
        Self::new()
    }
}

/// Unit 2D direction helper for HUD arrows.
pub(super) fn heading(from: Vec2, to: Vec2, yaw: f32) -> f32 {
    let delta = to - from;
    let angle = delta.x.atan2(-delta.y);
    (angle - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

#[cfg(test)]
mod tests {
    use super::super::{Expedition, Input};
    use super::*;

    fn pickup_progress() -> (Game, Progress) {
        let mut expedition = Expedition::new(false);
        expedition.start();
        expedition.advance(0., Input::default);
        expedition.game.position = expedition.game.crystals[0];
        let progress = expedition.advance(1. / 60., || Input {
            interact: true,
            ..Default::default()
        });
        assert_eq!(progress.pickups, 1);
        (expedition.game, progress)
    }

    #[test]
    fn accepted_pickup_drives_burst_pop_glow_and_echo() {
        let (game, progress) = pickup_progress();
        let mut juice = Juice::new();
        juice.on_progress(&progress, &game, 0);
        assert!(!juice.particles.is_empty());
        assert!(juice.flash > 0. && juice.bounce[0] == 1.);
        assert!(juice.pops[0].as_ref().unwrap().text.contains("1 / 3"));
        let callout = juice.callout.as_ref().unwrap();
        assert_eq!(callout.priority, Priority::Echo);
        assert!(callout.sub.starts_with("..."));
        assert!(juice.fov_punch() < 0.);
        // A rejected frame with no events changes nothing.
        let mut idle = Juice::new();
        idle.on_progress(&Progress::default(), &game, 0);
        assert!(idle.particles.is_empty() && idle.callout.is_none() && idle.flash == 0.);
    }

    #[test]
    fn reduced_motion_and_flash_clear_their_channels_but_keep_text() {
        let (game, progress) = pickup_progress();
        let mut juice = Juice::new();
        juice.on_progress(&progress, &game, 0);
        juice.apply_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        assert!(juice.particles.is_empty());
        assert_eq!(juice.shake(), (0., 0.));
        assert_eq!(juice.fov_punch(), 0.);
        assert_eq!(juice.icon_scale(0), 1.);
        assert!(juice.flash > 0., "flash is an independent preference");
        assert!(juice.callout.is_some() && juice.pops[0].is_some());
        juice.on_progress(&progress, &game, 0);
        assert!(juice.particles.is_empty(), "no new motion while reduced");
        juice.apply_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        assert_eq!(juice.flash, 0.);
        juice.on_progress(&progress, &game, 0);
        assert_eq!(juice.flash, 0., "no new glow while reduced");
        juice.apply_policy(FeedbackPolicy::default());
        assert_eq!(juice.flash, 0., "nothing revives");
    }

    #[test]
    fn zero_time_freezes_effects_and_retry_resets_them() {
        let (game, progress) = pickup_progress();
        let mut juice = Juice::new();
        juice.on_progress(&progress, &game, 0);
        let before = (juice.time, juice.particles.len(), juice.flash);
        assert_eq!(juice.advance(0., &game), 0);
        assert_eq!((juice.time, juice.particles.len(), juice.flash), before);
        juice.advance(0.5, &game);
        assert!(juice.flash < before.2);
        juice.reset();
        assert!(juice.particles.is_empty() && juice.callout.is_none() && juice.time == 0.);
    }

    #[test]
    fn the_hum_quickens_near_a_crystal_and_stops_when_far_or_done() {
        let mut game = Game::new();
        let mut juice = Juice::new();
        game.position = game.crystals[0] + Vec2::new(1., 0.);
        let near: u32 = (0..120).map(|_| juice.advance(1. / 60., &game)).sum();
        let mut far_juice = Juice::new();
        game.position = game.crystals[0] + Vec2::new(7., 0.);
        let far: u32 = (0..120).map(|_| far_juice.advance(1. / 60., &game)).sum();
        assert!(near > far && far > 0, "near {near}, far {far}");
        game.collected = [true; 3];
        let mut done = Juice::new();
        assert_eq!(
            (0..120).map(|_| done.advance(1. / 60., &game)).sum::<u32>(),
            0
        );
    }

    #[test]
    fn events_never_stomp_a_live_milestone_with_a_zone_banner() {
        let mut juice = Juice::new();
        juice.call("ALTAR", "", GOLD, 3., Priority::Event);
        juice.call("ZONE", "", STONE, 3., Priority::Zone);
        assert_eq!(juice.callout.as_ref().unwrap().head, "ALTAR");
        let game = Game::new();
        for _ in 0..28 {
            juice.advance(0.1, &game);
        }
        juice.call("ZONE", "", STONE, 3., Priority::Zone);
        assert_eq!(juice.callout.as_ref().unwrap().head, "ZONE");
    }

    #[test]
    fn heading_is_relative_to_view_yaw() {
        let p = Vec2::ZERO;
        assert!(heading(p, Vec2::new(0., -5.), 0.).abs() < 1e-5);
        assert!((heading(p, Vec2::new(5., 0.), 0.) - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        assert!(heading(p, Vec2::new(0., 5.), 0.).abs() > 3.);
    }
}
