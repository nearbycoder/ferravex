//! Presentation-only reactions to accepted FPS events.
//!
//! Nothing here feeds back into the match, the qualifier or replay bytes.
//! Timers advance on presentation time, and every animated amplitude is
//! scaled by the shared [`FeedbackPolicy`]: reduced motion removes wobble,
//! recoil, bell swing and falling animation (operators still lie down), and
//! reduced flashes removes the hit marker and countdown flashes.
use ferravex_engine::play::feedback::FeedbackPolicy;
use ferravex_graphics::{Quat, Vec3, vec3};

const SLOTS: usize = 8;
pub(crate) const WOBBLE_TIME: f32 = 0.5;
pub(crate) const BONK_TIME: f32 = 0.3;
pub(crate) const FALL_TIME: f32 = 0.35;
const RECOIL_TIME: f32 = 0.12;
const SWING_TIME: f32 = 1.6;
pub(crate) const POP_TIME: f32 = 0.9;
pub(crate) const RADIO_TIME: f32 = 3.6;
pub(crate) const BIG_TIME: f32 = 0.85;

/// What the last accepted hit did, for the marker's size and color.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Marker {
    #[default]
    Hit,
    Headshot,
    Elimination,
}

/// A rising score callout near the crosshair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pop {
    Points { points: u32, multiplier: u32 },
    Headshot { points: u32, multiplier: u32 },
    Eliminated,
    Bell,
    HeadshotStreak(u32),
}

/// An operator tipping over like a cardboard cutout after an elimination.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Fallen {
    pub position: Vec3,
    pub yaw: f32,
    /// Horizontal axis the cutout rotates around (away from the shooter).
    pub axis: Vec3,
    pub age: f32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Juice {
    wobble: [f32; SLOTS],
    wobble_axis: [Vec3; SLOTS],
    bonk: [f32; SLOTS],
    fallen: [Option<Fallen>; SLOTS],
    recoil: f32,
    swing: f32,
    pub marker: Marker,
    pops: [Option<(Pop, f32)>; 4],
    next_pop: usize,
    radio: Option<(&'static str, f32)>,
    big: Option<(&'static str, f32)>,
}

fn away_axis(from: Vec3, to: Vec3) -> Vec3 {
    let direction = vec3(to.x - from.x, 0., to.z - from.z).normalize_or_zero();
    let axis = Vec3::Y.cross(direction);
    if axis.length_squared() > 0.5 {
        axis
    } else {
        Vec3::X
    }
}

impl Juice {
    pub fn advance(&mut self, dt: f32, policy: FeedbackPolicy) {
        let dt = dt.max(0.);
        if policy.reduced_motion() {
            self.wobble = [0.; SLOTS];
            self.bonk = [0.; SLOTS];
            self.recoil = 0.;
            self.swing = 0.;
        }
        for t in self.wobble.iter_mut().chain(&mut self.bonk) {
            *t = (*t - dt).max(0.);
        }
        self.recoil = (self.recoil - dt).max(0.);
        self.swing = (self.swing - dt).max(0.);
        for fallen in self.fallen.iter_mut().flatten() {
            fallen.age = if policy.reduced_motion() {
                fallen.age.max(FALL_TIME)
            } else {
                fallen.age + dt
            };
        }
        for slot in &mut self.pops {
            if let Some((_, age)) = slot {
                *age += dt;
                if *age >= POP_TIME {
                    *slot = None;
                }
            }
        }
        for slot in [&mut self.radio, &mut self.big] {
            if let Some((_, time)) = slot {
                *time -= dt;
                if *time <= 0. {
                    *slot = None;
                }
            }
        }
    }
    pub fn fire(&mut self, policy: FeedbackPolicy) {
        if !policy.reduced_motion() {
            self.recoil = RECOIL_TIME;
        }
    }
    pub fn hit(
        &mut self,
        victim: u8,
        headshot: bool,
        from: Vec3,
        to: Vec3,
        policy: FeedbackPolicy,
    ) {
        let slot = usize::from(victim);
        if slot >= SLOTS || policy.reduced_motion() {
            return;
        }
        self.wobble[slot] = WOBBLE_TIME;
        self.wobble_axis[slot] = away_axis(from, to);
        if headshot {
            self.bonk[slot] = BONK_TIME;
        }
    }
    pub fn eliminate(
        &mut self,
        victim: u8,
        position: Vec3,
        yaw: f32,
        from: Vec3,
        policy: FeedbackPolicy,
    ) {
        if let Some(slot) = self.fallen.get_mut(usize::from(victim)) {
            *slot = Some(Fallen {
                position,
                yaw,
                axis: away_axis(from, position),
                // Reduced motion shows the cutout already lying down.
                age: if policy.reduced_motion() {
                    FALL_TIME
                } else {
                    0.
                },
            });
        }
    }
    pub fn respawned(&mut self, player: u8) {
        if let Some(slot) = self.fallen.get_mut(usize::from(player)) {
            *slot = None;
        }
        if let Some(slot) = self.wobble.get_mut(usize::from(player)) {
            *slot = 0.;
        }
    }
    pub fn ring(&mut self, policy: FeedbackPolicy) {
        if !policy.reduced_motion() {
            self.swing = SWING_TIME;
        }
    }
    pub fn pop(&mut self, pop: Pop) {
        self.pops[self.next_pop] = Some((pop, 0.));
        self.next_pop = (self.next_pop + 1) % self.pops.len();
    }
    pub fn radio(&mut self, line: &'static str) {
        self.radio = Some((line, RADIO_TIME));
    }
    pub fn clear_radio(&mut self) {
        self.radio = None;
    }
    pub fn big(&mut self, text: &'static str) {
        self.big = Some((text, BIG_TIME));
    }
    pub fn radio_line(&self) -> Option<(&'static str, f32)> {
        self.radio
    }
    pub fn big_text(&self) -> Option<(&'static str, f32)> {
        self.big
    }
    /// Live pops, oldest first, with their age in seconds.
    pub fn pops(&self) -> impl Iterator<Item = (Pop, f32)> + '_ {
        let start = self.next_pop;
        (0..self.pops.len()).filter_map(move |i| self.pops[(start + i) % self.pops.len()])
    }
    #[cfg(test)]
    pub fn fallen(&self, player: u8) -> Option<Fallen> {
        self.fallen.get(usize::from(player)).copied().flatten()
    }
    pub fn fallen_iter(&self) -> impl Iterator<Item = (u8, Fallen)> + '_ {
        self.fallen
            .iter()
            .enumerate()
            .filter_map(|(i, f)| f.map(|f| (i as u8, f)))
    }
    /// Rocking tilt for a live operator after a hit, pivoting at the feet.
    pub fn tilt(&self, player: u8, intensity: f32) -> Quat {
        let slot = usize::from(player);
        match self.wobble.get(slot) {
            Some(&t) if t > 0. => {
                let phase = WOBBLE_TIME - t;
                let angle = (phase * 30.).sin() * (t / WOBBLE_TIME) * 0.32 * intensity;
                Quat::from_axis_angle(self.wobble_axis[slot], angle)
            }
            _ => Quat::IDENTITY,
        }
    }
    /// Head scale multiplier: a headshot "bonks" the head bigger for a moment.
    pub fn head_scale(&self, player: u8, intensity: f32) -> f32 {
        match self.bonk.get(usize::from(player)) {
            Some(&t) if t > 0. => {
                let k = t / BONK_TIME;
                1. + (k * std::f32::consts::PI).sin() * 0.55 * intensity
            }
            _ => 1.,
        }
    }
    /// Fall rotation and how far the cutout has sunk into the stone.
    pub fn fall_pose(fallen: Fallen) -> (Quat, f32) {
        let k = (fallen.age / FALL_TIME).min(1.);
        // Ease-in like a toppling plank, with a small bounce on landing.
        let bounce = if fallen.age > FALL_TIME {
            let t = fallen.age - FALL_TIME;
            (t * 26.).sin() * (-t * 9.).exp() * 0.12
        } else {
            0.
        };
        let angle = k * k * std::f32::consts::FRAC_PI_2 - bounce;
        let sink = ((fallen.age - 2.2) / 0.6).clamp(0., 1.) * 0.5;
        (Quat::from_axis_angle(fallen.axis, angle), sink)
    }
    /// Weapon kick-back distance in meters.
    pub fn recoil(&self, intensity: f32) -> f32 {
        (self.recoil / RECOIL_TIME).clamp(0., 1.) * 0.07 * intensity
    }
    /// Bell swing angle in radians.
    pub fn bell_swing(&self, intensity: f32) -> f32 {
        let t = SWING_TIME - self.swing;
        if self.swing <= 0. {
            0.
        } else {
            (t * 9.).sin() * (self.swing / SWING_TIME) * 0.6 * intensity
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reduced_motion_removes_animation_but_keeps_lying_cutouts_and_text() {
        let calm = FeedbackPolicy::new(true, false, 1.).unwrap();
        let mut juice = Juice::default();
        juice.fire(FeedbackPolicy::default());
        juice.hit(
            2,
            true,
            Vec3::ZERO,
            vec3(0., 0., -5.),
            FeedbackPolicy::default(),
        );
        juice.ring(FeedbackPolicy::default());
        juice.eliminate(
            3,
            vec3(1., 0., 1.),
            0.,
            Vec3::ZERO,
            FeedbackPolicy::default(),
        );
        juice.pop(Pop::Eliminated);
        juice.radio("hello");
        juice.advance(0.05, FeedbackPolicy::default());
        assert!(juice.recoil(1.) > 0.);
        assert_ne!(juice.tilt(2, 1.), Quat::IDENTITY);
        assert!(juice.head_scale(2, 1.) >= 1.);
        juice.advance(0.01, calm);
        assert_eq!(juice.recoil(1.), 0.);
        assert_eq!(juice.tilt(2, 1.), Quat::IDENTITY);
        assert_eq!(juice.head_scale(2, 1.), 1.);
        assert_eq!(juice.bell_swing(1.), 0.);
        let fallen = juice.fallen(3).unwrap();
        assert!(
            fallen.age >= FALL_TIME,
            "cutouts lie down without animating"
        );
        assert_eq!(juice.pops().count(), 1);
        assert!(juice.radio_line().is_some());
        juice.hit(2, false, Vec3::ZERO, vec3(0., 0., -5.), calm);
        juice.fire(calm);
        assert_eq!(juice.tilt(2, 1.), Quat::IDENTITY);
        assert_eq!(juice.recoil(1.), 0.);
        juice.respawned(3);
        assert!(juice.fallen(3).is_none());
    }

    #[test]
    fn pops_rotate_through_four_slots_and_expire() {
        let mut juice = Juice::default();
        for points in 1..=6 {
            juice.pop(Pop::Points {
                points,
                multiplier: 1,
            });
        }
        let points: Vec<_> = juice
            .pops()
            .map(|(pop, _)| match pop {
                Pop::Points { points, .. } => points,
                _ => 0,
            })
            .collect();
        assert_eq!(points, [3, 4, 5, 6], "oldest first, bounded storage");
        juice.advance(POP_TIME, FeedbackPolicy::default());
        assert_eq!(juice.pops().count(), 0);
        juice.radio("x");
        juice.advance(RADIO_TIME + 0.01, FeedbackPolicy::default());
        assert!(juice.radio_line().is_none());
    }

    #[test]
    fn fallen_cutouts_tip_away_from_the_shooter_and_sink_before_respawn() {
        let mut juice = Juice::default();
        juice.eliminate(
            1,
            vec3(0., 0., -10.),
            0.,
            Vec3::ZERO,
            FeedbackPolicy::default(),
        );
        let start = Juice::fall_pose(juice.fallen(1).unwrap());
        assert_eq!(start.1, 0.);
        juice.advance(FALL_TIME, FeedbackPolicy::default());
        let (rotation, _) = Juice::fall_pose(juice.fallen(1).unwrap());
        let head = rotation * Vec3::Y;
        assert!(head.y.abs() < 0.05, "lying flat");
        assert!(head.z < -0.9, "fell away from a shooter at the origin");
        juice.advance(3., FeedbackPolicy::default());
        assert_eq!(Juice::fall_pose(juice.fallen(1).unwrap()).1, 0.5);
    }
}
