//! Presentation-only juice: score pops, callouts, camera kick, paddle squash,
//! shockwave rings and the victory confetti. Everything is created from
//! accepted simulation events and aged by fixed simulation ticks, so pause and
//! terminal screens freeze it and nothing here can change collisions, timing,
//! scoring or input.
//!
//! Reduced motion clears and blocks shake, squash, rings and confetti, and
//! keeps pops and callouts still. Reduced flash clears and blocks the drain
//! vignette and bonus glow. Text stays because it is information.
use crate::{game::Pickup, presentation::Spark, quips};
use ferravex_engine::play::feedback::FeedbackPolicy;
use std::fmt::{self, Write};

const POPS: usize = 8;
const RINGS: usize = 6;

pub(crate) const PAPER: [u8; 4] = [225, 234, 218, 255];
pub(crate) const CYAN: [u8; 4] = [99, 215, 196, 255];
pub(crate) const GOLD: [u8; 4] = [216, 173, 109, 255];
pub(crate) const HOT: [u8; 4] = [255, 196, 112, 255];
pub(crate) const VIOLET: [u8; 4] = [196, 180, 255, 255];
pub(crate) const LIME: [u8; 4] = [158, 234, 140, 255];

/// A short, fixed-capacity ASCII label: no allocation per pop or callout.
#[derive(Clone, Copy)]
pub(crate) struct Label {
    bytes: [u8; 40],
    len: u8,
}
impl Label {
    pub fn new(args: fmt::Arguments) -> Self {
        let mut label = Self {
            bytes: [0; 40],
            len: 0,
        };
        let _ = label.write_fmt(args);
        label
    }
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}
impl Write for Label {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes().filter(u8::is_ascii) {
            let Some(slot) = self.bytes.get_mut(self.len as usize) else {
                return Err(fmt::Error);
            };
            *slot = byte;
            self.len += 1;
        }
        Ok(())
    }
}
impl fmt::Debug for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Pop {
    pub at: [f32; 2],
    pub text: Label,
    pub color: [u8; 4],
    pub scale: i32,
    pub age: f32,
    pub life: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Ring {
    pub at: [f32; 2],
    pub radius: f32,
    pub color: [u8; 4],
    pub age: f32,
    pub life: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Priority {
    Volley,
    Pickup,
    Rescue,
    Bonus,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Callout {
    pub head: Label,
    pub sub: Label,
    pub color: [u8; 4],
    pub priority: Priority,
    pub age: f32,
    pub life: f32,
}
impl Callout {
    /// 0..=1 opacity with short fades, never a flash.
    pub fn alpha(&self) -> f32 {
        let fade_in = (self.age / 0.1).min(1.);
        let fade_out = ((self.life - self.age) / 0.3).clamp(0., 1.);
        fade_in.min(fade_out)
    }
}

pub(crate) struct Juice {
    pub pops: [Option<Pop>; POPS],
    next_pop: usize,
    pub rings: [Option<Ring>; RINGS],
    pub callout: Option<Callout>,
    /// Accumulated simulated presentation time; drives deterministic shake.
    pub time: f32,
    trauma: f32,
    /// 1 right after a paddle contact, decaying to 0.
    pub squash: f32,
    /// Red drain vignette, 0..=1.
    pub vignette: f32,
    /// Bonus glow for satellites and flawless sectors, 0..=1.
    pub glow: f32,
    /// Presentation seconds spent on the victory card (render time, Won only).
    pub celebrate: f32,
    /// Pitch for this frame's panel chime, rising with the volley.
    pub chime: Option<f32>,
    /// Seconds of deliberate steering; the steering hint fades once learned.
    pub steer_learned: f32,
    /// Whether a full-charge serve has been seen; the charge tip then retires.
    pub charge_learned: bool,
    saves: u32,
    backups: u32,
    last_panels: u32,
    motion: f32,
    flash: f32,
}

impl Default for Juice {
    fn default() -> Self {
        Self {
            pops: [None; POPS],
            next_pop: 0,
            rings: [None; RINGS],
            callout: None,
            time: 0.,
            trauma: 0.,
            squash: 0.,
            vignette: 0.,
            glow: 0.,
            celebrate: 0.,
            chime: None,
            steer_learned: 0.,
            charge_learned: false,
            saves: 0,
            backups: 0,
            last_panels: 0,
            motion: 1.,
            flash: 1.,
        }
    }
}

impl Juice {
    /// Restart discards every effect but keeps what the player has learned
    /// and the live accessibility policy.
    pub fn reset(&mut self) {
        let keep = (
            self.steer_learned,
            self.charge_learned,
            self.motion,
            self.flash,
        );
        *self = Self::default();
        (
            self.steer_learned,
            self.charge_learned,
            self.motion,
            self.flash,
        ) = keep;
    }

    /// Apply live preferences. Disabled channels are cleared, not hidden, so
    /// re-enabling a preference cannot revive an old effect.
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
            self.trauma = 0.;
            self.squash = 0.;
            self.rings = [None; RINGS];
            self.celebrate = 0.;
        }
        if self.flash == 0. {
            self.vignette = 0.;
            self.glow = 0.;
        }
    }

    pub fn motion(&self) -> f32 {
        self.motion
    }

    pub fn flash(&self) -> f32 {
        self.flash
    }

    /// Whole-pixel camera kick for the playfield; zero under reduced motion.
    pub fn shake(&self) -> [i32; 2] {
        let amount = self.trauma * self.trauma * 7. * self.motion;
        [
            ((self.time * 61.).sin() * amount).round() as i32,
            ((self.time * 47. + 1.3).cos() * amount * 0.8).round() as i32,
        ]
    }

    /// Paddle draw scale (horizontal, vertical); identity under reduced motion.
    pub fn paddle_scale(&self) -> (f32, f32) {
        let s = self.squash * self.squash * self.motion;
        (1. + 0.12 * s, 1. - 0.3 * s)
    }

    /// Age effects by one fixed simulation tick.
    pub fn tick(&mut self, dt: f32) {
        self.time += dt;
        self.trauma = (self.trauma - dt * 1.8).max(0.);
        self.squash = (self.squash - dt / 0.18).max(0.);
        self.vignette = (self.vignette - dt / 0.6).max(0.);
        self.glow = (self.glow - dt / 0.5).max(0.);
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
        if let Some(callout) = &mut self.callout {
            callout.age += dt;
            if callout.age >= callout.life {
                self.callout = None;
            }
        }
    }

    /// Victory confetti runs on presentation time, since simulation has ended.
    pub fn celebrate(&mut self, dt: f32) {
        if self.motion > 0. && dt.is_finite() {
            self.celebrate = (self.celebrate + dt.clamp(0., 0.25)).min(30.);
        }
    }

    /// Track deliberate steering on actual ticks so the hint can retire.
    pub fn steered(&mut self, axis: f32, dt: f32) {
        if axis.abs() > 0.3 {
            self.steer_learned = (self.steer_learned + dt).min(10.);
        }
    }

    pub fn steering_learned(&self) -> bool {
        self.steer_learned >= 0.75
    }

    fn kick(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount * self.motion).min(1.);
    }

    fn pop(&mut self, at: [f32; 2], text: Label, color: [u8; 4], scale: i32) {
        self.pops[self.next_pop] = Some(Pop {
            at,
            text,
            color,
            scale,
            age: 0.,
            life: 0.75,
        });
        self.next_pop = (self.next_pop + 1) % POPS;
    }

    fn ring(&mut self, at: [f32; 2], radius: f32, color: [u8; 4]) {
        if self.motion == 0. {
            return;
        }
        let slot = self
            .rings
            .iter()
            .position(Option::is_none)
            .unwrap_or_else(|| {
                // Replace the oldest ring.
                let mut oldest = 0;
                for (i, ring) in self.rings.iter().enumerate() {
                    if ring.is_some_and(|r| self.rings[oldest].is_some_and(|o| r.age > o.age)) {
                        oldest = i;
                    }
                }
                oldest
            });
        self.rings[slot] = Some(Ring {
            at,
            radius,
            color,
            age: 0.,
            life: 0.32,
        });
    }

    fn callout(&mut self, head: Label, sub: Label, color: [u8; 4], priority: Priority) {
        // A busy volley cannot bury a rarer callout that is still fresh.
        if let Some(current) = &self.callout
            && current.priority > priority
            && current.age < current.life * 0.6
        {
            return;
        }
        self.callout = Some(Callout {
            head,
            sub,
            color,
            priority,
            age: 0.,
            life: 1.5,
        });
    }

    pub fn saved(&mut self) {
        self.saves += 1;
        self.kick(0.25);
        self.callout(
            Label::new(format_args!("BALL SAVED")),
            Label::new(format_args!("{}", quips::save(self.saves))),
            CYAN,
            Priority::Rescue,
        );
    }

    pub fn accept(&mut self, spark: Spark) {
        match spark {
            Spark::Panel {
                at,
                points,
                destroyed,
                volley,
                ..
            } => {
                let color = if volley >= 5 {
                    HOT
                } else if volley >= 3 {
                    GOLD
                } else {
                    PAPER
                };
                let scale = if points >= 100 { 2 } else { 1 };
                self.pop(at, Label::new(format_args!("+{points}")), color, scale);
                self.kick(if destroyed { 0.12 } else { 0.05 });
                if destroyed && volley >= 5 {
                    self.ring(at, 46., HOT);
                }
                self.chime = Some(1. + 0.045 * volley.saturating_sub(1).min(14) as f32);
            }
            Spark::Paddle { at, bonus } => {
                self.squash = if self.motion > 0. { 1. } else { 0. };
                self.kick(0.06);
                if bonus > 0 {
                    self.pop(
                        [at[0], at[1] - 18.],
                        Label::new(format_args!("PRECISE +{bonus}")),
                        CYAN,
                        1,
                    );
                    self.ring(at, 30., CYAN);
                }
            }
            Spark::Volley(hits) => {
                self.kick(0.18);
                self.callout(
                    Label::new(format_args!("{}", quips::volley(hits).unwrap_or("VOLLEY!"))),
                    Label::new(format_args!("{hits} HITS IN ONE VOLLEY")),
                    if hits >= 8 { HOT } else { GOLD },
                    Priority::Volley,
                );
            }
            Spark::Pickup { kind, at } => {
                self.ring(at, 38., CYAN);
                let (head, sub, color) = match kind {
                    Pickup::Wide => ("WIDE PADDLE +50", "NOW WITH 55% MORE PADDLE", CYAN),
                    Pickup::Split => ("MULTI-BALL +50", "MORE BALLS, ZERO EXTRA RISK", VIOLET),
                    Pickup::Sticky => ("STICKY PADDLE +50", "NEXT 3 RETURNS STICK. LAUNCH!", LIME),
                };
                self.callout(
                    Label::new(format_args!("{head}")),
                    Label::new(format_args!("{sub}")),
                    color,
                    Priority::Pickup,
                );
            }
            Spark::Satellite { at, points } => {
                self.kick(0.5);
                self.glow = self.flash;
                self.ring(at, 60., VIOLET);
                self.ring(at, 34., PAPER);
                self.pop(at, Label::new(format_args!("+{points}")), VIOLET, 2);
                self.callout(
                    Label::new(format_args!("SATELLITE BONK! +{points}")),
                    Label::new(format_args!("SORRY, MISSION CONTROL")),
                    VIOLET,
                    Priority::Bonus,
                );
            }
            Spark::Drain { .. } => {
                self.kick(0.7);
                self.vignette = self.flash;
                self.callout = None;
            }
            Spark::LastPanel { at } => {
                self.last_panels += 1;
                self.ring(at, 52., GOLD);
                self.callout(
                    Label::new(format_args!("LAST ONE!")),
                    Label::new(format_args!("{}", quips::last_panel(self.last_panels))),
                    GOLD,
                    Priority::Bonus,
                );
            }
            Spark::Flawless(_) => {
                self.glow = self.flash;
                self.kick(0.35);
            }
            Spark::Backup { at } => {
                self.backups += 1;
                self.ring(at, 30., VIOLET);
                self.callout(
                    Label::new(format_args!("BACKUP BALL ONLINE")),
                    Label::new(format_args!("MULTI-BALL TO THE RESCUE")),
                    VIOLET,
                    Priority::Rescue,
                );
            }
            Spark::Caught => {
                self.squash = if self.motion > 0. { 0.7 } else { 0. };
            }
            Spark::Release { at } => {
                self.squash = if self.motion > 0. { 0.5 } else { 0. };
                self.ring(at, 24., LIME);
            }
            Spark::Fizzle => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_bounded_ascii_without_allocation() {
        let label = Label::new(format_args!("+{}", 1200));
        assert_eq!(label.as_str(), "+1200");
        let long = Label::new(format_args!("{}", "X".repeat(80)));
        assert_eq!(long.as_str().len(), 40);
        assert_eq!(Label::new(format_args!("é+1")).as_str(), "+1");
    }

    #[test]
    fn reduced_motion_blocks_kick_squash_rings_and_confetti_but_keeps_text() {
        let mut juice = Juice::default();
        juice.apply_policy(FeedbackPolicy::new(true, false, 1.).unwrap());
        juice.accept(Spark::Paddle {
            at: [320., 620.],
            bonus: 100,
        });
        juice.accept(Spark::Volley(5));
        juice.tick(0.01);
        juice.celebrate(1.);
        assert_eq!(juice.shake(), [0, 0]);
        assert_eq!(juice.paddle_scale(), (1., 1.));
        assert!(juice.rings.iter().all(Option::is_none));
        assert_eq!(juice.celebrate, 0.);
        assert!(juice.pops.iter().any(Option::is_some));
        assert!(juice.callout.is_some());
    }

    #[test]
    fn reduced_flash_clears_glow_and_vignette_and_reenabling_cannot_revive_them() {
        let mut juice = Juice::default();
        juice.accept(Spark::Drain { at: [100., 653.] });
        juice.accept(Spark::Flawless(1000));
        assert!(juice.vignette > 0. && juice.glow > 0.);
        juice.apply_policy(FeedbackPolicy::new(false, true, 1.).unwrap());
        assert_eq!((juice.vignette, juice.glow), (0., 0.));
        juice.apply_policy(FeedbackPolicy::default());
        assert_eq!((juice.vignette, juice.glow), (0., 0.));
    }

    #[test]
    fn rare_callouts_survive_a_busy_volley_and_effects_expire_on_ticks() {
        let mut juice = Juice::default();
        juice.accept(Spark::Satellite {
            at: [300., 121.],
            points: 1000,
        });
        juice.accept(Spark::Volley(3));
        let head = juice.callout.map(|c| c.head);
        assert_eq!(head.unwrap().as_str(), "SATELLITE BONK! +1000");
        for _ in 0..240 {
            juice.tick(1. / 120.);
        }
        assert!(juice.callout.is_none());
        assert!(juice.pops.iter().all(Option::is_none));
        assert!(juice.rings.iter().all(Option::is_none));
        assert_eq!(juice.shake(), [0, 0]);
    }

    #[test]
    fn panel_chime_rises_with_the_volley_and_reset_keeps_learned_hints() {
        let mut juice = Juice::default();
        let mut pitches = Vec::new();
        for volley in 1..=4 {
            juice.accept(Spark::Panel {
                at: [80., 165.],
                points: 100,
                destroyed: true,
                row: 0,
                volley,
            });
            pitches.push(juice.chime.unwrap());
        }
        assert!(pitches.windows(2).all(|w| w[1] > w[0]));
        juice.steered(1., 1.);
        juice.charge_learned = true;
        juice.reset();
        assert!(juice.steering_learned() && juice.charge_learned);
        assert!(juice.pops.iter().all(Option::is_none));
    }
}
