//! Presentation-only juice: tire smoke, grass spray, sparks, splashes, confetti,
//! rising score pops, banner toasts, a follow camera and camera shake.
//!
//! Effects are created from accepted race ticks and [`crate::style::Event`]s
//! and aged by accepted simulation time, so pause freezes them and nothing here
//! can change the car, gates, timers or records. The results celebration ages
//! by render time only while the race is frozen at the finish.
//!
//! Reduced motion clears and blocks particles, shake, squash, zoom punches and
//! the countdown fly-in; the camera still follows the car without lookahead
//! sway. Reduced flash clears and blocks the gold new-best edge glow. Text pops
//! and toasts are information, so they remain but no longer rise or bounce.
use crate::quips;
use crate::sim::{Race, Track};
use crate::style::{Event, Rating, Style};
use ferravex_play::feedback::FeedbackPolicy;
use ferravex_play::particles::{
    Emitter, EmitterDesc, EmitterTransform, ParticleDesc, ParticleMotion, ParticlePool,
};
use ferravex_render::prelude::*;

const SEED: u64 = 0x5241_4c4c;
const POPS: usize = 8;
pub const TOAST_FADE: f32 = 0.25;
/// World area shown by the overview camera (the whole circuit).
pub const WORLD_CENTER: Vec2 = vec2(550., 325.);
pub const WORLD_SIZE: Vec2 = vec2(1140., 650.);
/// Land drawn around the circuit; the follow camera stays inside it.
pub const LAND: Rect = Rect::new(-100., -100., 1300., 850.);

pub const CREAM: Color = Color::new(0.96, 0.92, 0.81, 1.);
pub const GOLD: Color = Color::new(1.0, 0.80, 0.33, 1.);
pub const GREEN: Color = Color::new(0.55, 0.92, 0.50, 1.);
pub const RED: Color = Color::new(1.0, 0.45, 0.36, 1.);
pub const AMBER: Color = Color::new(1.0, 0.84, 0.49, 1.);
pub const WATER: Color = Color::new(0.62, 0.86, 0.95, 1.);
pub const MUTED: Color = Color::new(0.78, 0.80, 0.70, 1.);

#[derive(Clone, Debug)]
pub struct Pop {
    pub at: Vec2,
    pub text: String,
    pub color: Color,
    pub age: f32,
    pub life: f32,
    pub size: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Info,
    Lap,
    Alert,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub head: String,
    pub sub: String,
    pub color: Color,
    pub sub_color: Color,
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

pub struct Juice {
    pub particles: ParticlePool,
    emitter: Emitter,
    pub pops: [Option<Pop>; POPS],
    pub toast: Option<Toast>,
    /// Accumulated accepted simulation time; drives deterministic wobble.
    pub time: f32,
    trauma: f32,
    /// Positive squashes the car after a fence bump.
    pub squash: f32,
    /// Gold edge glow after a new best lap, 0..=1.
    pub flash: f32,
    /// Zoom punch on laps and the finish, 0..=1.
    pub punch: f32,
    /// Seconds since GO, for the big GO! stamp.
    pub since_go: Option<f32>,
    /// Render seconds since the finish (results intro).
    pub celebrate: f32,
    /// Where the race finished, for the celebration's confetti volleys.
    finish_at: Option<Vec2>,
    /// Smoothed camera focus in world units.
    pub focus: Vec2,
    /// Latest split (or lap) delta versus the best lap, for the HUD.
    pub last_split: Option<f32>,
    motion: f32,
    flash_scale: f32,
    tick: u32,
}

fn desc(
    start: Color,
    end: Color,
    velocity: Vec2,
    jitter: Vec2,
    life: f32,
    size: (f32, f32),
    drag: f32,
) -> EmitterDesc {
    EmitterDesc {
        particle: ParticleDesc {
            velocity: [velocity.x, velocity.y, 0.],
            lifetime: life,
            start_color: [start.r, start.g, start.b, start.a],
            end_color: [end.r, end.g, end.b, 0.],
            start_size: size.0,
            end_size: size.1,
            ..Default::default()
        },
        motion: ParticleMotion {
            drag,
            angular_velocity: 5.,
            ..Default::default()
        },
        velocity_jitter: [jitter.x, jitter.y, 0.],
        lifetime_jitter: 0.3,
        size_jitter: 0.35,
        rotation_jitter: std::f32::consts::PI,
        rate: 0.,
        ..Default::default()
    }
}

/// Rear wheel positions (left, right) in world units.
pub fn rear_wheels(race: &Race) -> [Vec2; 2] {
    let pose = race.car.pose;
    let rotation = Vec2::from_angle(pose.angle);
    [-1., 1.].map(|side| pose.position + rotation.rotate(vec2(-8., side * 6.)))
}

impl Default for Juice {
    fn default() -> Self {
        Self::new(&Track::new())
    }
}

impl Juice {
    pub fn new(track: &Track) -> Self {
        Self {
            particles: ParticlePool::new(512),
            emitter: Emitter::new(
                desc(CREAM, CREAM, Vec2::ZERO, Vec2::ONE, 0.5, (4., 1.), 2.),
                SEED,
            )
            .expect("authored burst"),
            pops: Default::default(),
            toast: None,
            time: 0.,
            trauma: 0.,
            squash: 0.,
            flash: 0.,
            punch: 0.,
            since_go: None,
            celebrate: 0.,
            finish_at: None,
            focus: track.points[0],
            last_split: None,
            motion: 1.,
            flash_scale: 1.,
            tick: 0,
        }
    }

    /// Restart discards every effect and reseeds bursts, so retries look identical.
    pub fn reset(&mut self, track: &Track) {
        let policy = (self.motion, self.flash_scale);
        *self = Self::new(track);
        (self.motion, self.flash_scale) = policy;
    }

    /// Apply live preferences on every frame. Disabled channels are cleared,
    /// not hidden, so re-enabling a preference cannot revive an old effect.
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
            self.squash = 0.;
            self.punch = 0.;
        }
        if self.flash_scale == 0. {
            self.flash = 0.;
        }
    }

    pub fn motion(&self) -> f32 {
        self.motion
    }

    /// Camera shake in world units; zero under reduced motion.
    pub fn shake(&self) -> Vec2 {
        let amount = self.trauma * self.trauma * 9. * self.motion;
        vec2(
            (self.time * 57.).sin() * amount,
            (self.time * 43. + 1.3).cos() * amount * 0.8,
        )
    }

    /// Car draw scale (length, width): squashed along its axis after a bump.
    pub fn car_scale(&self) -> Vec2 {
        let s = self.squash * self.motion;
        vec2(1. - 0.22 * s, 1. + 0.18 * s)
    }

    fn burst(&mut self, desc: EmitterDesc, at: Vec2, count: u32) {
        if self.motion == 0. || !at.is_finite() {
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

    pub fn toast(
        &mut self,
        head: String,
        sub: String,
        colors: (Color, Color),
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
            color: colors.0,
            sub_color: colors.1,
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

    /// The countdown reached GO. Text only: no particles before the car moves.
    pub fn on_go(&mut self) {
        self.since_go = Some(0.);
    }

    /// Route one accepted vehicle tick. Call after [`Style::observe`].
    pub fn on_tick(&mut self, race: &Race, style: &Style, track: &Track) {
        self.tick = self.tick.wrapping_add(1);
        let car = &race.car;
        let speed = car.velocity.length();
        let back = -car.velocity.normalize_or_zero();
        // Continuous emission: tire smoke while drifting, grass spray off-road.
        if self.tick.is_multiple_of(3) && self.motion > 0. {
            for wheel in rear_wheels(race) {
                if car.offroad && speed > 40. {
                    let spray = back * speed * 0.35;
                    let (start, end) = if self.tick.is_multiple_of(2) {
                        (
                            Color::new(0.45, 0.58, 0.27, 0.9),
                            Color::new(0.35, 0.47, 0.22, 0.),
                        )
                    } else {
                        (
                            Color::new(0.62, 0.52, 0.34, 0.8),
                            Color::new(0.55, 0.47, 0.32, 0.),
                        )
                    };
                    self.burst(
                        desc(start, end, spray, vec2(55., 55.), 0.45, (4.5, 1.5), 3.),
                        wheel,
                        2,
                    );
                } else if style.drifting() || car.sliding && speed > 60. {
                    self.burst(
                        desc(
                            Color::new(0.90, 0.90, 0.86, 0.55),
                            Color::new(0.85, 0.86, 0.82, 0.),
                            back * 18.,
                            vec2(14., 14.),
                            0.7,
                            (6., 15.),
                            1.5,
                        ),
                        wheel,
                        1,
                    );
                }
            }
        }
        let at = car.pose.position;
        for event in style.events.iter() {
            match event {
                Event::DriftBanked {
                    points,
                    multiplier,
                    rating,
                } => {
                    let text = if multiplier > 1 {
                        format!("+{points} {} x{multiplier}", rating.label())
                    } else {
                        format!("+{points} {}", rating.label())
                    };
                    let size = if rating >= Rating::PureStyle {
                        22.
                    } else {
                        18.
                    };
                    self.pop(at + vec2(0., -26.), text, GOLD, size);
                    if rating >= Rating::Sideways {
                        self.burst(
                            desc(GOLD, AMBER, Vec2::ZERO, vec2(130., 130.), 0.5, (5., 1.), 3.),
                            at,
                            12,
                        );
                    }
                }
                Event::DriftBotched { lost } => {
                    self.pop(at + vec2(0., -26.), format!("-{lost} BOTCHED"), RED, 18.);
                    self.toast(
                        "DRIFT BOTCHED".to_owned(),
                        quips::botched(style.stats.botched).to_owned(),
                        (RED, CREAM),
                        1.6,
                        Priority::Info,
                    );
                }
                Event::Split { gate: _, delta } => {
                    self.last_split = Some(delta);
                    let (text, color) = split_label(delta);
                    self.pop(at + vec2(0., 22.), text, color, 17.);
                }
                Event::Lap { number, clean } => {
                    let new_best = race
                        .last_comparison
                        .is_some_and(|c| c.comparison.is_new_best());
                    self.last_split = race
                        .last_comparison
                        .and_then(|c| c.delta)
                        .map(|d| d as f32 / 1_000_000.);
                    if new_best {
                        if self.flash_scale > 0. && number > 1 {
                            self.flash = self.flash_scale;
                        }
                        self.burst(
                            desc(GOLD, CREAM, Vec2::ZERO, vec2(220., 220.), 0.9, (7., 2.), 2.),
                            at,
                            30,
                        );
                    }
                    if self.motion > 0. {
                        self.punch = 1.;
                    }
                    if clean {
                        self.pop(
                            at + vec2(0., -48.),
                            format!("CLEAN LAP +{}", crate::style::CLEAN_LAP_BONUS),
                            GREEN,
                            18.,
                        );
                    }
                }
                Event::PerfectLine => {
                    self.pop(
                        at + vec2(0., -70.),
                        format!("PERFECT LINE +{}", crate::style::PERFECT_BONUS),
                        GOLD,
                        22.,
                    );
                }
                Event::Bump { strength } => {
                    self.kick(0.35 + 0.5 * strength);
                    self.squash = strength;
                    self.burst(
                        desc(
                            Color::new(1., 0.93, 0.55, 1.),
                            Color::new(1., 0.55, 0.2, 1.),
                            -car.velocity * 0.2,
                            vec2(160., 160.),
                            0.35,
                            (4., 1.),
                            4.,
                        ),
                        at,
                        16,
                    );
                    self.pop(
                        at + vec2(0., -30.),
                        quips::bump(style.stats.bumps).to_owned(),
                        AMBER,
                        18.,
                    );
                }
                Event::OffTrack => {}
                Event::WrongWay => {}
                Event::Splash { first } => {
                    self.kick(0.45);
                    self.burst(
                        desc(
                            WATER,
                            CREAM,
                            Vec2::ZERO,
                            vec2(200., 200.),
                            0.8,
                            (7., 2.),
                            2.5,
                        ),
                        at,
                        36,
                    );
                    self.toast(
                        if first {
                            format!("SPLASH! +{}", crate::style::SPLASH_BONUS)
                        } else {
                            "SPLOOSH!".to_owned()
                        },
                        quips::splash(style.stats.splashes).to_owned(),
                        (WATER, CREAM),
                        2.6,
                        Priority::Alert,
                    );
                }
                Event::Donut => {
                    self.pop(
                        at + vec2(0., -34.),
                        format!("DONUT! +{}", crate::style::DONUT_BONUS),
                        GOLD,
                        22.,
                    );
                    self.burst(
                        desc(
                            Color::new(0.92, 0.92, 0.88, 0.7),
                            CREAM,
                            Vec2::ZERO,
                            vec2(120., 120.),
                            0.9,
                            (8., 18.),
                            2.,
                        ),
                        at,
                        24,
                    );
                }
                Event::Finished => {
                    for (color, offset) in [
                        (GOLD, vec2(-24., 0.)),
                        (RED, vec2(0., -18.)),
                        (GREEN, vec2(24., 0.)),
                        (WATER, vec2(0., 18.)),
                    ] {
                        self.burst(
                            desc(
                                color,
                                CREAM,
                                Vec2::ZERO,
                                vec2(320., 320.),
                                1.5,
                                (7., 3.),
                                1.6,
                            ),
                            at + offset,
                            26,
                        );
                    }
                    self.kick(0.3);
                    self.finish_at = Some(at);
                }
            }
        }
        let _ = track;
    }

    /// Lap/off-track banners that depend on the session's lap notice.
    /// Lap banner; the second-to-last lap announces the final lap. A clean-lap
    /// quip rides along on ordinary laps (the final-lap line is long enough).
    pub fn lap_toast(
        &mut self,
        number: u32,
        time: f32,
        best: bool,
        comparison: String,
        quip: Option<&str>,
        final_lap_next: bool,
    ) {
        let sub_color = if comparison.starts_with('+') {
            RED
        } else if comparison.starts_with("NEW BEST") || comparison.starts_with("FIRST BEST") {
            GREEN
        } else {
            CREAM
        };
        let lap = format!("LAP {number}  {}", crate::hud::clock(time));
        let (head, sub, color) = if final_lap_next {
            (
                "FINAL LAP!".to_owned(),
                format!("{lap}  ·  {comparison}"),
                AMBER,
            )
        } else if let Some(quip) = quip {
            (
                lap,
                format!("{comparison}  ·  {quip}"),
                if best { GOLD } else { CREAM },
            )
        } else {
            (lap, comparison, if best { GOLD } else { CREAM })
        };
        self.toast(head, sub, (color, sub_color), 2.8, Priority::Lap);
    }

    /// Follow the car once per accepted tick (including countdown ticks).
    pub fn follow(&mut self, race: &Race, dt: f32) {
        let lookahead = if self.motion > 0. { 0.42 } else { 0.25 };
        let target = race.car.pose.position + race.car.velocity * lookahead;
        let blend = 1. - (-5. * dt).exp();
        self.focus += (target - self.focus) * blend;
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
        if let Some(toast) = &mut self.toast {
            toast.age += dt;
            if toast.age >= toast.life {
                self.toast = None;
            }
        }
        if let Some(go) = &mut self.since_go {
            *go += dt;
            if *go > 1.2 {
                self.since_go = None;
            }
        }
        self.trauma = (self.trauma - dt * 2.2).max(0.);
        self.squash *= (-dt * 10.).exp();
        if self.squash < 0.01 {
            self.squash = 0.;
        }
        self.punch = (self.punch - dt * 2.5).max(0.);
        self.flash = (self.flash - dt * 1.6).max(0.);
    }

    /// The results intro ages by render time while the race is frozen.
    pub fn advance_celebration(&mut self, dt: f32) {
        let dt = dt.clamp(0., 0.1);
        let before = self.celebrate;
        self.celebrate += dt;
        // A few extra confetti volleys while the podium card is up.
        if let Some(at) = self.finish_at
            && self.celebrate < 4.
            && (before / 0.7).floor() != (self.celebrate / 0.7).floor()
        {
            let volley = (self.celebrate / 0.7) as usize;
            let color = [GOLD, RED, GREEN, WATER][volley % 4];
            let offset = vec2(
                [-40., 30., -10., 45., -35.][volley % 5],
                [-25., 20., 35., -30., 5.][volley % 5],
            );
            self.burst(
                desc(
                    color,
                    CREAM,
                    Vec2::ZERO,
                    vec2(260., 260.),
                    1.3,
                    (6., 2.),
                    1.8,
                ),
                at + offset,
                18,
            );
        }
        self.advance(dt);
    }

    /// World rectangle to show. `mix` blends the overview (0) into the
    /// follow camera (1); the follow view never leaves the drawn land.
    /// `framing` is (screen-pixel shift, ease 0..=1) for the finish framing.
    pub fn view(
        &self,
        width: f32,
        height: f32,
        top: f32,
        mix: f32,
        framing: (Vec2, f32),
    ) -> (Vec2, f32) {
        let usable = (height - top).max(120.);
        let overview = (width / WORLD_SIZE.x).min(usable / WORLD_SIZE.y);
        // Landscape screens show ~800 world units across; portrait screens
        // narrow that to ~430 so the car stays readable on a phone. Short
        // landscape screens still show at least 280 units of height.
        let aspect = ((width / usable - 0.6) / 0.7).clamp(0., 1.);
        let visible = 430. + 370. * aspect;
        let follow = (width / visible).min(usable / 280.);
        let punch = 1. + 0.04 * self.punch * self.motion;
        let scale = (overview + (follow - overview) * mix) * punch;
        let half = vec2(width, height) / (2. * scale);
        let (shift, ease) = framing;
        let mut center = self.focus + self.shake() + shift * ease / scale;
        for axis in 0..2 {
            let (lo, hi) = (
                LAND.point()[axis] + half[axis],
                LAND.point()[axis] + LAND.size()[axis] - half[axis],
            );
            center[axis] = if lo <= hi {
                center[axis].clamp(lo, hi)
            } else {
                // The whole land already fits; only the finish framing moves it.
                let middle = LAND.point()[axis] + LAND.size()[axis] * 0.5;
                let target = self.focus[axis] + shift[axis] / scale;
                middle + (target - middle) * ease
            };
        }
        // The HUD bar covers the top; nudge the overview so the circuit sits below it.
        let overview_center = WORLD_CENTER - vec2(0., top * 0.5 / scale);
        (overview_center + (center - overview_center) * mix, scale)
    }
}

/// Split label and colour: green when faster than the best lap at this gate.
pub fn split_label(delta: f32) -> (String, Color) {
    if delta.abs() < 0.005 {
        ("0.00".to_owned(), MUTED)
    } else if delta < 0. {
        (format!("-{:.2}", delta.abs()), GREEN)
    } else {
        (format!("+{delta:.2}"), RED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::RaceSession;
    use crate::sim::Controls;
    use crate::style::Before;

    const DT: f32 = 1. / 120.;

    fn run(juice: &mut Juice, ticks: usize) -> (Race, Style, usize) {
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut session = RaceSession::new();
        let mut style = Style::new();
        let mut most = 0;
        for _ in 0..ticks {
            let frame = session.advance(DT as f64, true);
            for _ in 0..frame.steps {
                let mut controls = if session.countdown() == 0 {
                    race.autopilot(&track)
                } else {
                    Controls::default()
                };
                // A scripted handbrake slide on the first straight.
                if (240..330).contains(&race.ticks) {
                    controls.steer = 1.;
                    controls.handbrake = true;
                }
                let before = Before::new(&race, session.off_track());
                if session.step(&mut race, &track, controls, frame.dt) {
                    style.observe(
                        before,
                        &race,
                        &track,
                        controls,
                        session.off_track(),
                        frame.dt,
                    );
                    juice.on_tick(&race, &style, &track);
                }
                juice.follow(&race, frame.dt);
                juice.advance(frame.dt);
                most = most.max(juice.particles.len());
            }
        }
        (race, style, most)
    }

    #[test]
    fn reduced_motion_blocks_and_clears_moving_effects_but_keeps_text() {
        let previous = ferravex_render::preferences::feedback_policy();
        let track = Track::new();
        let mut juice = Juice::new(&track);
        let (race, _, most) = run(&mut juice, 360 + 1400);
        assert!(most > 0, "drifting smokes the tires");
        juice.burst(
            desc(GOLD, GOLD, Vec2::ZERO, Vec2::ONE, 1., (4., 1.), 1.),
            vec2(100., 100.),
            20,
        );
        juice.kick(0.8);
        juice.pop(race.car.pose.position, "+10".into(), GOLD, 18.);
        assert_ne!(juice.shake(), Vec2::ZERO);

        juice.apply_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        assert!(juice.particles.is_empty());
        assert_eq!(juice.shake(), Vec2::ZERO);
        assert_eq!(juice.car_scale(), vec2(1., 1.));
        assert!(
            juice.pops.iter().flatten().count() > 0,
            "text stays readable"
        );
        juice.burst(
            desc(GOLD, GOLD, Vec2::ZERO, Vec2::ONE, 1., (4., 1.), 1.),
            vec2(100., 100.),
            20,
        );
        assert!(juice.particles.is_empty(), "no new motion while reduced");

        juice.flash = 1.;
        juice.apply_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        assert_eq!(juice.flash, 0.);
        juice.apply_policy(FeedbackPolicy::default());
        assert!(
            juice.particles.is_empty() && juice.flash == 0.,
            "nothing revives"
        );
        ferravex_render::preferences::set_feedback_policy(previous);
    }

    #[test]
    fn effects_are_reproducible_and_freeze_without_ticks() {
        let sample = || {
            let track = Track::new();
            let mut juice = Juice::new(&track);
            run(&mut juice, 360 + 900);
            (
                juice.particles.len(),
                juice.time,
                juice.focus,
                juice
                    .pops
                    .iter()
                    .flatten()
                    .map(|p| p.text.clone())
                    .collect::<Vec<_>>(),
            )
        };
        let first = sample();
        assert_eq!(first, sample());
        let track = Track::new();
        let mut juice = Juice::new(&track);
        run(&mut juice, 500);
        let frozen = (juice.time, juice.particles.len(), juice.focus);
        juice.advance(0.);
        assert_eq!((juice.time, juice.particles.len(), juice.focus), frozen);
        juice.toast("A".into(), "B".into(), (CREAM, CREAM), 1., Priority::Info);
        juice.reset(&track);
        assert!(juice.toast.is_none() && juice.particles.is_empty());
        assert_eq!(juice.focus, track.points[0]);
    }

    #[test]
    fn higher_priority_banners_are_not_stomped() {
        let mut juice = Juice::new(&Track::new());
        juice.toast(
            "SPLASH".into(),
            String::new(),
            (WATER, CREAM),
            2.,
            Priority::Alert,
        );
        juice.toast(
            "BOTCH".into(),
            String::new(),
            (RED, CREAM),
            2.,
            Priority::Info,
        );
        assert_eq!(juice.toast.as_ref().unwrap().head, "SPLASH");
        juice.advance(1.9);
        juice.toast(
            "BOTCH".into(),
            String::new(),
            (RED, CREAM),
            2.,
            Priority::Info,
        );
        assert_eq!(juice.toast.as_ref().unwrap().head, "BOTCH");
    }

    #[test]
    fn follow_view_stays_on_the_land_and_zooms_in_from_the_overview() {
        let track = Track::new();
        let mut juice = Juice::new(&track);
        for (w, h) in [
            (1280., 800.),
            (1600., 1000.),
            (390., 800.),
            (320., 568.),
            (844., 390.),
        ] {
            let (overview_center, overview) = juice.view(w, h, 80., 0., (Vec2::ZERO, 0.));
            let (_, follow) = juice.view(w, h, 80., 1., (Vec2::ZERO, 0.));
            assert!(follow > overview * 1.3, "{w}x{h}: follow zooms in");
            assert!((overview_center.x - WORLD_CENTER.x).abs() < 0.01);
            for focus in [vec2(-500., -500.), vec2(5000., 5000.), track.points[40]] {
                juice.focus = focus;
                let (center, scale) = juice.view(w, h, 80., 1., (Vec2::ZERO, 0.));
                let half = vec2(w, h) / (2. * scale);
                for axis in 0..2 {
                    if half[axis] * 2. <= LAND.size()[axis] {
                        assert!(center[axis] - half[axis] >= LAND.point()[axis] - 0.01);
                        assert!(
                            center[axis] + half[axis]
                                <= LAND.point()[axis] + LAND.size()[axis] + 0.01
                        );
                    }
                }
            }
        }
        assert_eq!(split_label(-0.424), ("-0.42".to_owned(), GREEN));
        assert_eq!(split_label(0.31), ("+0.31".to_owned(), RED));
        assert_eq!(split_label(0.), ("0.00".to_owned(), MUTED));
        assert_eq!(split_label(-0.001), ("0.00".to_owned(), MUTED));
    }
}
