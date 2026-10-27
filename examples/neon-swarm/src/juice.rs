//! Presentation-only juice: camera shake/kick, the final-drone camera tighten,
//! dash stretch, expanding rings, rising score pops and one announcer slot.
//!
//! Everything here is created from accepted simulation notes or feedback
//! events and aged by presentation time. Nothing here can change collision,
//! timing, scoring or input. Reduced motion clears and blocks shake, kick,
//! zoom, stretch and rings, and keeps pops/callouts still. Reduced flash only
//! affects the separate additive particles and flashes owned by Presentation.
use crate::hud::Line;
use crate::{AMBER, CORAL, CYAN};
use ferravex_render::prelude::{Color, Vec2, vec2};

const POPS: usize = 10;
const RINGS: usize = 16;
pub(crate) const GOLD: Color = Color::from_rgba(255, 214, 102, 255);
pub(crate) const MAGENTA: Color = Color::from_rgba(236, 104, 214, 255);
pub(crate) const VIOLET: Color = Color::from_rgba(168, 128, 255, 255);

#[derive(Clone, Debug)]
pub(crate) struct Pop {
    pub at: Vec2,
    pub text: Line,
    pub color: Color,
    pub age: f32,
    pub life: f32,
    pub size: f32,
}

impl Pop {
    pub fn alpha(&self) -> f32 {
        ((self.life - self.age) / 0.25).clamp(0., 1.)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Ring {
    pub at: Vec2,
    pub age: f32,
    pub life: f32,
    pub from: f32,
    pub to: f32,
    pub width: f32,
    pub color: Color,
}

impl Ring {
    pub fn progress(&self) -> f32 {
        (self.age / self.life).clamp(0., 1.)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Priority {
    Minor,
    Combo,
    Major,
    Epic,
}

#[derive(Clone, Debug)]
pub(crate) struct Callout {
    pub head: Line,
    pub sub: Line,
    pub color: Color,
    pub age: f32,
    pub life: f32,
    pub priority: Priority,
}

impl Callout {
    /// Short fades, never a flash.
    pub fn alpha(&self) -> f32 {
        let fade_in = (self.age / 0.08).min(1.);
        let fade_out = ((self.life - self.age) / 0.3).clamp(0., 1.);
        fade_in.min(fade_out)
    }

    /// A brief overshoot on arrival; 1.0 under reduced motion.
    pub fn scale(&self, motion: f32) -> f32 {
        let t = (self.age / 0.18).min(1.);
        1. + (1. - t) * 0.35 * motion
    }
}

pub(crate) struct Juice {
    pub pops: [Option<Pop>; POPS],
    pub rings: [Option<Ring>; RINGS],
    pub callout: Option<Callout>,
    /// Presentation seconds; drives deterministic shake and ambient motion.
    pub time: f32,
    trauma: f32,
    kick: Vec2,
    /// 0..=1 eased progress toward the final-drone close-up.
    pub focus: f32,
    /// Dash stretch remaining and its direction.
    pub stretch: f32,
    pub stretch_dir: Vec2,
    /// Seconds since the run ended in victory, for the celebration.
    pub celebrate: Option<f32>,
    motion: f32,
}

fn oldest(ages: impl Iterator<Item = Option<f32>>) -> usize {
    ages.enumerate()
        .max_by(|a, b| a.1.unwrap_or(0.).total_cmp(&b.1.unwrap_or(0.)))
        .map_or(0, |(i, _)| i)
}

impl Juice {
    pub fn new() -> Self {
        Self {
            pops: Default::default(),
            rings: [None; RINGS],
            callout: None,
            time: 0.,
            trauma: 0.,
            kick: Vec2::ZERO,
            focus: 0.,
            stretch: 0.,
            stretch_dir: Vec2::X,
            celebrate: None,
            motion: 1.,
        }
    }

    pub fn reset(&mut self) {
        let motion = self.motion;
        *self = Self::new();
        self.motion = motion;
    }

    /// Apply the live motion scale. A disabled channel is cleared, not hidden.
    pub fn apply_motion(&mut self, motion: f32) {
        self.motion = motion;
        if motion == 0. {
            self.rings = [None; RINGS];
            self.trauma = 0.;
            self.kick = Vec2::ZERO;
            self.focus = 0.;
            self.stretch = 0.;
        }
    }

    pub fn motion(&self) -> f32 {
        self.motion
    }

    pub fn age(&mut self, dt: f32, focus_target: bool) {
        self.time += dt;
        self.trauma = (self.trauma - dt * 1.9).max(0.);
        self.kick *= (-26. * dt).exp();
        self.stretch = (self.stretch - dt).max(0.);
        let target = if focus_target && self.motion > 0. {
            1.
        } else {
            0.
        };
        // Slow in, quicker out, so the last drone gets a deliberate close-up.
        let rate = if target > self.focus { 0.9 } else { 2.5 };
        self.focus += (target - self.focus).clamp(-rate * dt, rate * dt);
        if let Some(t) = &mut self.celebrate {
            *t += dt;
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
        if let Some(callout) = &mut self.callout {
            callout.age += dt;
            if callout.age >= callout.life {
                self.callout = None;
            }
        }
    }

    /// World-space camera offset: deterministic shake plus recoil kick.
    pub fn shake(&self) -> Vec2 {
        let amount = self.trauma * self.trauma * 13. * self.motion;
        vec2(
            (self.time * 57.).sin() * amount,
            (self.time * 43. + 1.7).cos() * amount * 0.8,
        ) + self.kick * self.motion
    }

    /// Camera zoom factor for the final-drone close-up.
    pub fn lead(&self) -> Vec2 {
        -self.kick * 0.6 * self.motion
    }

    pub fn zoom(&self) -> f32 {
        let t = self.focus * self.focus * (3. - 2. * self.focus);
        1. + 0.08 * t * self.motion
    }

    pub fn trauma(&mut self, amount: f32) {
        if self.motion > 0. {
            self.trauma = (self.trauma + amount).min(1.);
        }
    }

    pub fn recoil(&mut self, facing: Vec2) {
        if self.motion > 0. {
            self.kick = -facing * 3.;
        }
    }

    pub fn dash(&mut self, direction: Vec2) {
        if self.motion > 0. {
            self.stretch = 0.2;
            self.stretch_dir = direction;
        }
    }

    /// Player draw scale (along the dash, across it).
    pub fn player_scale(&self) -> Vec2 {
        let s = (self.stretch / 0.2) * self.motion;
        vec2(1. + 0.45 * s, 1. - 0.25 * s)
    }

    pub fn ring(&mut self, at: Vec2, from: f32, to: f32, life: f32, color: Color) {
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
            from,
            to,
            width: if to > 80. { 4. } else { 2.5 },
            color,
        });
    }

    pub fn pop(&mut self, at: Vec2, text: Line, color: Color, size: f32) {
        // Stack simultaneous pops instead of printing them on top of each other.
        let crowd = self
            .pops
            .iter()
            .flatten()
            .filter(|p| p.age < 0.4 && p.at.distance(at) < 40.)
            .count();
        let at = at - vec2(0., 26. * crowd as f32);
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
            life: 0.85,
            size,
        });
    }

    /// One announcer slot: higher priority wins; equal or lower priority
    /// replaces a callout only once it has been readable for a moment.
    pub fn callout(&mut self, head: Line, sub: Line, color: Color, priority: Priority) {
        if self
            .callout
            .as_ref()
            .is_some_and(|c| c.priority > priority && c.age < 0.7)
        {
            return;
        }
        let life = match priority {
            Priority::Minor => 0.9,
            Priority::Combo => 1.2,
            Priority::Major => 1.6,
            Priority::Epic => 2.2,
        };
        self.callout = Some(Callout {
            head,
            sub,
            color,
            age: 0.,
            life,
            priority,
        });
    }

    /// How far a pop has risen; still under reduced motion.
    pub fn rise(&self, pop: &Pop) -> f32 {
        let t = pop.age / pop.life;
        (1. - (1. - t) * (1. - t)) * 34. * self.motion
    }
}

/// Multiplier tier color, shared by the arena border, chain HUD and pops.
pub(crate) fn tier_color(multiplier: u32) -> Color {
    match multiplier {
        0 | 1 => CYAN,
        2 => Color::from_rgba(130, 246, 160, 255),
        3 => AMBER,
        4 => CORAL,
        _ => MAGENTA,
    }
}

pub(crate) fn fade(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0., 1.))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_motion_clears_moving_effects_and_keeps_information() {
        let mut juice = Juice::new();
        juice.trauma(0.8);
        juice.recoil(Vec2::X);
        juice.dash(Vec2::Y);
        juice.ring(Vec2::ZERO, 10., 60., 0.4, CYAN);
        juice.pop(Vec2::ZERO, Line::from("+100"), CYAN, 18.);
        juice.callout(Line::from("ON FIRE"), Line::new(), AMBER, Priority::Combo);
        juice.age(0.01, true);
        assert!(juice.shake().length() > 0.);
        juice.age(0.5, true);
        assert!(juice.zoom() > 1.);
        assert!(juice.player_scale().x == 1., "dash stretch already expired");
        juice.dash(Vec2::Y);
        juice.apply_motion(0.);
        assert_eq!(juice.shake(), Vec2::ZERO);
        assert_eq!(juice.zoom(), 1.);
        assert_eq!(juice.player_scale(), vec2(1., 1.));
        assert!(juice.rings.iter().all(Option::is_none));
        assert!(
            juice.pops.iter().any(Option::is_some),
            "score pops stay readable"
        );
        assert!(juice.callout.is_some(), "callouts stay readable");
        juice.trauma(1.);
        juice.ring(Vec2::ZERO, 10., 60., 0.4, CYAN);
        juice.age(0.1, true);
        assert_eq!(juice.shake(), Vec2::ZERO);
        assert_eq!(juice.zoom(), 1., "no close-up under reduced motion");
        let pop = juice.pops.iter().flatten().next().unwrap();
        assert_eq!(juice.rise(pop), 0.);
        juice.apply_motion(1.);
        assert_eq!(
            juice.shake(),
            Vec2::ZERO,
            "re-enabling does not revive shake"
        );
    }

    #[test]
    fn callouts_respect_priority_and_storage_stays_bounded() {
        let mut juice = Juice::new();
        juice.callout(Line::from("OVERSEER"), Line::new(), GOLD, Priority::Epic);
        juice.callout(Line::from("NOT TODAY!"), Line::new(), CYAN, Priority::Minor);
        assert_eq!(juice.callout.as_ref().unwrap().head, "OVERSEER");
        juice.age(0.8, false);
        juice.callout(Line::from("NOT TODAY!"), Line::new(), CYAN, Priority::Minor);
        assert_eq!(juice.callout.as_ref().unwrap().head, "NOT TODAY!");
        for n in 0..100 {
            juice.pop(Vec2::ZERO, crate::hud::line!("+{n}"), CYAN, 18.);
            juice.ring(Vec2::ZERO, 1., 2., 1., CYAN);
            juice.age(0.01, false);
        }
        assert_eq!(juice.pops.iter().flatten().count(), POPS);
        assert_eq!(juice.rings.iter().flatten().count(), RINGS);
        juice.age(5., false);
        assert!(juice.pops.iter().all(Option::is_none) && juice.callout.is_none());
        juice.celebrate = Some(0.);
        juice.reset();
        assert!(juice.celebrate.is_none());
    }
}
