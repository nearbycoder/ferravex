//! Presentation-only juice: gate sparks, rising callouts, lap banners, the
//! KDUNE radio toast, screen-edge flashes, FOV kicks and speed lines.
//!
//! Everything here is created from accepted [`RaceEvent`]s and aged by accepted
//! simulation time, so pausing or finishing freezes it and nothing here can
//! change steering, gates, boost or timing.
//!
//! Reduced motion clears and blocks sparks, FOV kicks, speed lines, camera
//! kicks and countdown punch; text callouts stay but no longer rise. Reduced
//! flash clears and blocks the edge flashes. Toasts and banners are
//! informational, so they remain under both preferences.
use super::quips;
use super::race::{GATES, HOVER_HEIGHT, LAPS, MULTIPLIER_CAP, Race, RaceEvent, world};
use ferravex_play::camera::{Rig3, ShakeImpulse};
use ferravex_play::feedback::FeedbackPolicy;
use ferravex_play::particles::{Emitter, EmitterTransform, ParticlePool, presets};
use ferravex_render::audio::{Cue, Surface};
use ferravex_render::prelude::*;

const SPARK_SEED: u64 = 0x5350_4b53;
pub(super) const GOLD: Color = Color::new(1.0, 0.80, 0.36, 1.0);
pub(super) const MINT: Color = Color::new(0.55, 0.93, 0.80, 1.0);
pub(super) const ROSE: Color = Color::new(1.0, 0.55, 0.45, 1.0);
const PALE: Color = super::PALE;
const ORANGE: Color = super::ORANGE;
const TEAL: Color = super::TEAL;

/// A rising callout near the craft, in screen space.
#[derive(Clone, Debug)]
pub(super) struct Pop {
    pub head: String,
    pub sub: String,
    pub color: Color,
    pub age: f32,
    pub life: f32,
    pub size: f32,
}

impl Pop {
    pub fn alpha(&self) -> f32 {
        ((self.life - self.age) / 0.3).clamp(0., 1.)
    }
    /// 0..=1 pop-in for the squash-and-settle scale.
    pub fn grow(&self) -> f32 {
        (self.age / 0.12).min(1.)
    }
}

/// A centered lap / final-lap / mirage banner.
#[derive(Clone, Debug)]
pub(super) struct Banner {
    pub head: String,
    pub sub: String,
    pub color: Color,
    pub sub_color: Color,
    pub age: f32,
    pub life: f32,
}

impl Banner {
    pub fn alpha(&self) -> f32 {
        (self.age / 0.15)
            .min(1.)
            .min(((self.life - self.age) / 0.35).clamp(0., 1.))
    }
}

/// One KDUNE radio line.
#[derive(Clone, Debug)]
pub(super) struct Toast {
    pub line: String,
    pub age: f32,
    pub life: f32,
    pub priority: u8,
}

impl Toast {
    pub fn alpha(&self) -> f32 {
        (self.age / 0.12)
            .min(1.)
            .min(((self.life - self.age) / 0.3).clamp(0., 1.))
    }
}

/// Onboarding progress, learned from accepted race state; hints fade once learned.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Learned {
    pub drive: bool,
    pub boost: bool,
    pub thread: bool,
}

pub(super) struct Juice {
    pub sparks: ParticlePool,
    emitter: Emitter,
    /// One callout at a time: a newer one replaces the old, so they never overlap.
    pub pop: Option<Pop>,
    pub banner: Option<Banner>,
    pub toast: Option<Toast>,
    /// Edge flash level and tint.
    pub flash: f32,
    pub flash_color: Color,
    /// Extra vertical FOV in degrees; decays quickly.
    pub fov_kick: f32,
    /// Accumulated accepted simulation time; drives speed lines.
    pub time: f32,
    /// Seconds since the finish, for the results card entrance.
    pub finish_age: f32,
    pub learned: Learned,
    /// Seconds each hint has been learned, for its fade-out.
    pub hint_fade: f32,
    /// A veteran (any stored finish) sees no onboarding hints.
    pub veteran: bool,
    /// Finishes recorded before this race; rotates the start/launch lines.
    pub attempt: u32,
    clipped: u32,
    said_streak: u32,
    lost: u32,
    sands: u32,
    motion: f32,
    flash_scale: f32,
}

impl Juice {
    pub fn new() -> Self {
        Self {
            sparks: ParticlePool::new(192),
            emitter: Emitter::new(presets::sparks([1.; 4]), SPARK_SEED).expect("authored sparks"),
            pop: None,
            banner: None,
            toast: None,
            flash: 0.,
            flash_color: TEAL,
            fov_kick: 0.,
            time: 0.,
            finish_age: 0.,
            learned: Learned::default(),
            hint_fade: 0.,
            veteran: false,
            attempt: 0,
            clipped: 0,
            said_streak: 0,
            lost: 0,
            sands: 0,
            motion: 1.,
            flash_scale: 1.,
        }
    }

    /// Restart discards every effect and reseeds bursts, so retries look identical.
    /// Preferences, veteran status and learned onboarding survive.
    pub fn reset(&mut self) {
        let keep = (
            self.motion,
            self.flash_scale,
            self.veteran,
            self.attempt,
            self.learned,
        );
        *self = Self::new();
        (
            self.motion,
            self.flash_scale,
            self.veteran,
            self.attempt,
            self.learned,
        ) = keep;
        if self.learned.drive && self.learned.boost && self.learned.thread {
            self.hint_fade = 1.;
        }
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
            self.sparks.clear();
            self.emitter.reset(SPARK_SEED);
            self.fov_kick = 0.;
        }
        if self.flash_scale == 0. {
            self.flash = 0.;
        }
    }

    pub fn motion(&self) -> f32 {
        self.motion
    }

    /// Age effects by accepted simulation time only (zero while paused).
    pub fn advance(&mut self, dt: f32, race: &Race) {
        if dt <= 0. {
            return;
        }
        self.time = (self.time + dt) % 1000.;
        self.sparks.update(dt).expect("finite spark update");
        if let Some(pop) = &mut self.pop {
            pop.age += dt;
            if pop.age >= pop.life {
                self.pop = None;
            }
        }
        if let Some(banner) = &mut self.banner {
            banner.age += dt;
            if banner.age >= banner.life {
                self.banner = None;
            }
        }
        if let Some(toast) = &mut self.toast {
            toast.age += dt;
            if toast.age >= toast.life {
                self.toast = None;
            }
        }
        self.flash = (self.flash - dt * 2.6).max(0.);
        self.fov_kick *= (-dt * 5.).exp();
        if race.finished {
            self.finish_age += dt;
        }
        if !self.learned.drive && race.countdown == 0. && race.speed() > 6. {
            self.learned.drive = true;
        }
        if self.learned.drive && self.learned.boost && self.learned.thread {
            self.hint_fade = (self.hint_fade + dt * 2.).min(1.);
        }
    }

    fn pop(&mut self, head: impl Into<String>, sub: impl Into<String>, color: Color, size: f32) {
        let pop = Pop {
            head: head.into(),
            sub: sub.into(),
            color,
            age: 0.,
            life: 1.1,
            size,
        };
        self.pop = Some(pop);
    }

    /// A radio line replaces the current one unless that one is more important
    /// and still fresh, so warnings are not instantly buried by chatter.
    pub fn say(&mut self, line: impl Into<String>, priority: u8) {
        if let Some(toast) = &self.toast
            && toast.priority > priority
            && toast.age < 1.2
        {
            return;
        }
        self.toast = Some(Toast {
            line: line.into(),
            age: 0.,
            life: 3.2,
            priority,
        });
    }

    fn flash(&mut self, color: Color, amount: f32) {
        if self.flash_scale > 0. {
            self.flash = (self.flash.max(amount * self.flash_scale)).min(1.);
            self.flash_color = color;
        }
    }

    fn kick(&mut self, camera: &mut Rig3, degrees: f32, shake: f32) {
        if self.motion == 0. {
            return;
        }
        self.fov_kick = (self.fov_kick + degrees * self.motion).min(10.);
        if shake > 0. {
            // Capacity failures only drop this extra kick; impacts own their own layer.
            let _ = camera.shake.push(ShakeImpulse {
                translation: vec3(0.02, 0.012, 0.) * shake * self.motion,
                rotation: vec3(0.004, 0.003, 0.012) * shake * self.motion,
                duration: 0.18,
                frequency: 22.,
            });
        }
    }

    fn burst(&mut self, at: Vec3, color: Color, count: u32) {
        if self.motion == 0. {
            return;
        }
        let mut desc = presets::sparks([color.r, color.g, color.b, 1.]);
        desc.particle.start_size = 0.32;
        desc.particle.end_size = 0.04;
        desc.particle.lifetime = 0.65;
        desc.particle.acceleration = [0., -7., 0.];
        if let Some(cone) = &mut desc.velocity_cone {
            cone.speed = [4., 11.];
        }
        if self.emitter.set_config(desc).is_ok() {
            let _ = self.emitter.burst(
                (count as f32 * self.motion.max(0.5)) as u32,
                EmitterTransform::at(at.to_array()),
                &mut self.sparks,
            );
        }
    }

    /// React to one accepted event. Returns the audio cue to play, if any.
    pub fn on_event(
        &mut self,
        event: RaceEvent,
        race: &Race,
        camera: &mut Rig3,
    ) -> Option<(Cue, f32)> {
        match event {
            RaceEvent::Go { rocket, eager } => {
                if rocket {
                    self.pop("ROCKET START!", "PERFECT LAUNCH", GOLD, 30.);
                    self.say(quips::launch(true, self.attempt), 2);
                    self.flash(GOLD, 0.45);
                    self.kick(camera, 6., 1.);
                    Some((Cue::Success, 0.4))
                } else {
                    if eager {
                        self.say(quips::launch(false, self.attempt), 2);
                    }
                    self.pop("GO!", "", MINT, 34.);
                    Some((Cue::Pickup, 0.45))
                }
            }
            RaceEvent::Gate {
                gate,
                threaded,
                streak,
                refill,
            } => {
                let at = course_gate(gate);
                if threaded {
                    self.learned.thread = true;
                    let multiplier = streak.min(MULTIPLIER_CAP);
                    let sub = if multiplier > 1 {
                        format!("+{:.0}% BOOST  x{multiplier}", refill * 100.)
                    } else {
                        format!("+{:.0}% BOOST", refill * 100.)
                    };
                    let color = if streak >= MULTIPLIER_CAP { GOLD } else { MINT };
                    self.pop(quips::streak_callout(streak), sub, color, 24.);
                    self.burst(
                        at,
                        if streak >= 3 { GOLD } else { MINT },
                        18 + 3 * multiplier,
                    );
                    self.flash(color, 0.18 + 0.04 * multiplier as f32);
                    self.kick(camera, 1.2 + 0.4 * multiplier as f32, 0.);
                    if quips::streak_milestone(streak) {
                        self.say(quips::streak(self.said_streak), 1);
                        self.said_streak += 1;
                    }
                    Some((Cue::Pickup, 0.28 + 0.04 * multiplier as f32))
                } else {
                    self.clipped += 1;
                    let sub = if !self.veteran && self.clipped <= 3 {
                        "THREAD THE CENTER FOR BOOST"
                    } else {
                        ""
                    };
                    self.pop("CLIPPED", sub, PALE, 18.);
                    self.burst(at, PALE, 8);
                    Some((Cue::Click, 0.3))
                }
            }
            RaceEvent::StreakLost { streak } => {
                if streak >= 3 {
                    self.say(quips::streak_lost(self.lost), 0);
                    self.lost += 1;
                }
                None
            }
            RaceEvent::Lap { lap, time, perfect } => {
                let previous = (lap >= 2).then(|| race.lap_times[lap as usize - 2]);
                let delta = previous.map(|p| time - p);
                let split = super::time_label(time);
                let (sub, sub_color) = match delta {
                    Some(d) => (
                        format!("{split}   {}", signed(d)),
                        if d <= 0. { MINT } else { ROSE },
                    ),
                    None => (format!("{split}   FIRST LAP"), PALE),
                };
                let head = if perfect {
                    "MIRAGE LAP!".to_owned()
                } else if lap + 1 == LAPS {
                    "FINAL LAP!".to_owned()
                } else if lap == LAPS {
                    "FINISH!".to_owned()
                } else {
                    format!("LAP {lap} DONE")
                };
                // The lap banner subsumes the gate callout from the same tick.
                self.pop = None;
                self.banner = Some(Banner {
                    head,
                    sub,
                    color: if perfect { GOLD } else { super::WHITE },
                    sub_color,
                    age: 0.,
                    life: if lap == LAPS { 1.0 } else { 2.4 },
                });
                if lap < LAPS {
                    self.say(quips::lap(lap, LAPS, perfect, delta), 2);
                }
                if perfect {
                    self.flash(GOLD, 0.7);
                    self.kick(camera, 5., 0.6);
                    self.burst(world(race.position, HOVER_HEIGHT + 1.), GOLD, 48);
                    Some((Cue::Success, 0.5))
                } else {
                    Some((Cue::Pickup, 0.5))
                }
            }
            RaceEvent::Finished => {
                self.finish_age = 0.;
                self.flash(GOLD, 0.55);
                self.burst(world(race.position, HOVER_HEIGHT + 1.), GOLD, 56);
                Some((Cue::Success, 0.65))
            }
            RaceEvent::Scrape { at, refill } => {
                self.pop(
                    "SCRAPE!",
                    format!("+{:.0}% BOOST", refill * 100.),
                    ORANGE,
                    22.,
                );
                self.burst(world(at, 1.0), ORANGE, 14);
                self.kick(camera, 0.8, 0.5);
                self.say(quips::scrape(race.scrapes), 1);
                Some((Cue::Click, 0.45))
            }
            RaceEvent::Impact { at, speed } => {
                let hard = (speed / 20.).clamp(0.2, 1.);
                self.burst(world(at, 0.9), super::STONE, (6. + 14. * hard) as u32);
                self.flash(ROSE, 0.2 + 0.25 * hard);
                self.say(quips::impact(race.impacts), 1);
                // Camera shake for impacts stays in Presentation::impact.
                Some((Cue::Impact(Surface::Metal), 0.5 + 0.3 * hard))
            }
            RaceEvent::BoostIgnited => {
                self.learned.boost = true;
                self.kick(camera, 3.5, 0.4);
                None
            }
            RaceEvent::Recovered => {
                self.sparks.clear();
                self.fov_kick = 0.;
                self.say(quips::recovery(race.recovery_count), 3);
                Some((Cue::Failure, 0.45))
            }
        }
    }

    /// Entering loose sand is a state edge rather than an event; the host calls
    /// this from accepted ticks only.
    pub fn sand(&mut self) {
        self.say(quips::sand(self.sands), 1);
        self.sands += 1;
    }

    /// Speed-line strength 0..=1 from speed in m/s; zero under reduced motion.
    pub fn speed_lines(&self, speed: f32, boosting: bool) -> f32 {
        let base = ((speed - 30.) / 12.).clamp(0., 1.);
        (base * if boosting { 1. } else { 0.6 }) * self.motion
    }
}

/// Gate world position at hover height; the course is fixed, so this is cheap.
fn course_gate(gate: usize) -> Vec3 {
    let t = gate as f32 / GATES as f32 * std::f32::consts::TAU;
    world(super::race::path(t), HOVER_HEIGHT + 1.2)
}

pub(super) fn signed(delta: f32) -> String {
    if delta <= 0. {
        format!("-{:.2}", -delta)
    } else {
        format!("+{delta:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::race::Course;

    fn threaded(gate: usize, streak: u32) -> RaceEvent {
        RaceEvent::Gate {
            gate,
            threaded: true,
            streak,
            refill: crate::race::thread_refill(streak),
        }
    }

    #[test]
    fn reduced_motion_blocks_sparks_and_kicks_but_keeps_informative_text() {
        let race = Race::new();
        let mut camera = Rig3::default();
        let mut juice = Juice::new();
        juice.on_event(threaded(3, 4), &race, &mut camera);
        assert!(!juice.sparks.is_empty());
        assert!(juice.fov_kick > 0. && juice.flash > 0.);
        juice.apply_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        assert!(juice.sparks.is_empty());
        assert_eq!(juice.fov_kick, 0.);
        juice.on_event(threaded(4, 5), &race, &mut camera);
        juice.on_event(RaceEvent::BoostIgnited, &race, &mut camera);
        assert!(juice.sparks.is_empty());
        assert_eq!(juice.fov_kick, 0.);
        assert_eq!(camera.shake.active(), 0);
        assert!(
            juice
                .pop
                .as_ref()
                .is_some_and(|p| p.head == "MAX MULTIPLIER!")
        );
        // Reduced flash clears the running flash and blocks new ones.
        juice.apply_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        assert_eq!(juice.flash, 0.);
        juice.on_event(threaded(5, 6), &race, &mut camera);
        assert_eq!(juice.flash, 0.);
        assert!(!juice.sparks.is_empty(), "motion is allowed again");
    }

    #[test]
    fn effects_freeze_without_accepted_time_and_reset_keeps_learning() {
        let race = Race::new();
        let mut camera = Rig3::default();
        let mut juice = Juice::new();
        juice.veteran = true;
        juice.on_event(
            RaceEvent::Scrape {
                at: Vec2::ZERO,
                refill: 0.06,
            },
            &race,
            &mut camera,
        );
        juice.on_event(threaded(1, 1), &race, &mut camera);
        let pop = juice.pop.as_ref().unwrap().age;
        let toast = juice.toast.clone().unwrap();
        juice.advance(0., &race);
        assert_eq!(juice.pop.as_ref().unwrap().age, pop);
        assert_eq!(juice.toast.as_ref().unwrap().age, toast.age);
        for _ in 0..600 {
            juice.advance(1. / 120., &race);
        }
        assert!(juice.pop.is_none());
        assert!(juice.toast.is_none() && juice.sparks.is_empty());
        assert!(juice.learned.thread);
        juice.reset();
        assert!(juice.veteran && juice.learned.thread);
    }

    #[test]
    fn warnings_are_not_buried_by_chatter_and_lap_banners_report_deltas() {
        let course = Course::new();
        let mut race = Race::new();
        let mut camera = Rig3::default();
        let mut juice = Juice::new();
        race.recovery_count = 1;
        juice.on_event(RaceEvent::Recovered, &race, &mut camera);
        let warning = juice.toast.clone().unwrap().line;
        juice.on_event(threaded(1, 3), &race, &mut camera);
        assert_eq!(juice.toast.as_ref().unwrap().line, warning);
        for _ in 0..200 {
            juice.advance(1. / 120., &race);
        }
        juice.on_event(threaded(2, 5), &race, &mut camera);
        assert_ne!(juice.toast.as_ref().unwrap().line, warning);

        race.lap_times = [10.5, 10.0, 0.];
        juice.on_event(
            RaceEvent::Lap {
                lap: 2,
                time: 10.0,
                perfect: false,
            },
            &race,
            &mut camera,
        );
        let banner = juice.banner.clone().unwrap();
        assert_eq!(banner.head, "FINAL LAP!");
        assert!(banner.sub.ends_with("-0.50"), "{}", banner.sub);
        assert_eq!(banner.sub_color, MINT);
        let _ = course;
        assert_eq!(signed(0.25), "+0.25");
    }
}
