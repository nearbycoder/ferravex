//! Presentation-only juice: debris chunks, dust, muzzle flash, camera kick and
//! shake, score pops, the callout banner, Foreman Gus and the lunch bell.
//!
//! Effects are created only from accepted [`RangeEvent`]s and aged only by
//! accepted fixed-tick time, so pausing, Controls or a result freezes them and
//! nothing here can change collisions, timing, scoring or input. A fixed seed
//! makes every retry look identical.
//!
//! Reduced motion clears and blocks debris, dust, rings, camera kick/shake,
//! barrel squash, Gus's hops and the bell swing. Reduced flash clears and blocks
//! the muzzle flash and impact flashes. Score pops, the banner and Gus's speech
//! are information, so they stay, but no longer rise, pop or bounce.
use super::quips::{Banter, Mood};
use super::score::{Callout, RangeEvent};
use ferravex_render::prelude::{Quat, Vec2, Vec3, vec2, vec3};

const CHUNKS: usize = 56;
const PUFFS: usize = 72;
const POPS: usize = 8;
const RINGS: usize = 6;
const TRAIL: usize = 14;
const SEED: u64 = 0x5349_4547_4521;
pub(super) const BUBBLE_LIFE: f32 = 2.8;
pub(super) const BANNER_LIFE: f32 = 1.9;
/// Render seconds the final collapse and banner get before the results card.
pub(super) const CARD_DELAY: f32 = 0.9;

/// Which palette a debris chunk uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Material {
    Rust,
    Sand,
    Soot,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Chunk {
    pub position: Vec3,
    velocity: Vec3,
    pub rotation: Quat,
    spin: Vec3,
    pub size: f32,
    pub material: Material,
    pub age: f32,
    pub life: f32,
}

impl Chunk {
    /// Shrinks away over the final quarter of its life.
    pub fn scale(&self) -> f32 {
        self.size * ((self.life - self.age) / (self.life * 0.25)).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PuffKind {
    Dust,
    Smoke,
    /// Bright additive-looking burst; blocked by reduced flash.
    Flash,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Puff {
    pub position: Vec3,
    velocity: Vec3,
    pub radius: f32,
    grow: f32,
    pub age: f32,
    pub life: f32,
    pub kind: PuffKind,
    pub spin: f32,
}

impl Puff {
    pub fn alpha(&self) -> f32 {
        let t = self.age / self.life;
        (1.0 - t).clamp(0.0, 1.0) * (self.age / 0.06).min(1.0)
    }

    pub fn current_radius(&self) -> f32 {
        self.radius + self.grow * self.age
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PopText {
    Points { points: u32, chain: u32 },
    Word(&'static str),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Pop {
    pub at: Vec3,
    pub text: PopText,
    pub age: f32,
    pub life: f32,
}

impl Pop {
    pub fn alpha(&self) -> f32 {
        ((self.life - self.age) / 0.35).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Ring {
    pub at: Vec3,
    pub age: f32,
    pub life: f32,
    pub radius: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Banner {
    pub callout: Callout,
    pub points: u32,
    pub age: f32,
}

impl Banner {
    pub fn alpha(&self) -> f32 {
        let fade_in = (self.age / 0.08).min(1.0);
        let fade_out = ((BANNER_LIFE - self.age) / 0.3).clamp(0.0, 1.0);
        fade_in.min(fade_out)
    }

    /// Ease-out-back pop from 0.6x to 1x, settling in 0.25 s.
    pub fn scale(&self, motion: f32) -> f32 {
        let t = (self.age / 0.25).min(1.0) - 1.0;
        let back = 1.0 + 2.7 * t.powi(3) + 1.7 * t.powi(2);
        1.0 + (0.6 + 0.4 * back - 1.0) * motion
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Bubble {
    pub text: &'static str,
    pub mood: Mood,
    pub age: f32,
}

impl Bubble {
    pub fn alpha(&self) -> f32 {
        let fade_in = (self.age / 0.1).min(1.0);
        let fade_out = ((BUBBLE_LIFE - self.age) / 0.3).clamp(0.0, 1.0);
        fade_in.min(fade_out)
    }
}

fn mood_priority(mood: Mood) -> u8 {
    match mood {
        Mood::Bell | Mood::Wonder => 5,
        Mood::Domino => 4,
        Mood::Knocked => 3,
        Mood::Wobble | Mood::Miss | Mood::MissStreak => 2,
        Mood::Start | Mood::Retry | Mood::LastShell | Mood::Settling => 1,
    }
}

pub(super) struct Juice {
    pub chunks: [Option<Chunk>; CHUNKS],
    pub puffs: [Option<Puff>; PUFFS],
    pub pops: [Option<Pop>; POPS],
    pub rings: [Option<Ring>; RINGS],
    pub banner: Option<Banner>,
    pub bubble: Option<Bubble>,
    pub trail: [Vec3; TRAIL],
    pub trail_len: usize,
    /// Rolling score shown on the HUD, eased toward the real total.
    pub shown_points: f32,
    pub time: f32,
    trauma: f32,
    kick: f32,
    /// Muzzle flash level, 0..=1.
    pub flash: f32,
    /// Gus: hop height and alarm (wobble) levels, 0..=1.
    pub hop: f32,
    pub alarm: f32,
    pub bell_swing: f32,
    pub squash: f32,
    /// Render seconds since a result froze the range (presentation only).
    pub result_age: f32,
    settling_said: bool,
    rng: u64,
    motion: f32,
    flash_scale: f32,
}

impl Juice {
    pub fn new() -> Self {
        Self {
            chunks: [None; CHUNKS],
            puffs: [None; PUFFS],
            pops: [None; POPS],
            rings: [None; RINGS],
            banner: None,
            bubble: None,
            trail: [Vec3::ZERO; TRAIL],
            trail_len: 0,
            shown_points: 0.0,
            time: 0.0,
            trauma: 0.0,
            kick: 0.0,
            flash: 0.0,
            hop: 0.0,
            alarm: 0.0,
            bell_swing: 0.0,
            squash: 0.0,
            result_age: 0.0,
            settling_said: false,
            rng: SEED,
            motion: 1.0,
            flash_scale: 1.0,
        }
    }

    /// Forget every effect and reseed, keeping the live preferences.
    pub fn reset(&mut self) {
        let policy = (self.motion, self.flash_scale);
        *self = Self::new();
        (self.motion, self.flash_scale) = policy;
    }

    /// Apply live preferences. Disabled channels are cleared, not hidden, so
    /// re-enabling a preference cannot revive an old effect.
    pub fn apply_policy(&mut self, reduced_motion: bool, reduced_flash: bool, intensity: f32) {
        self.motion = if reduced_motion { 0.0 } else { intensity };
        self.flash_scale = if reduced_flash { 0.0 } else { intensity };
        if self.motion == 0.0 {
            self.chunks = [None; CHUNKS];
            self.rings = [None; RINGS];
            self.trauma = 0.0;
            self.kick = 0.0;
            self.hop = 0.0;
            self.alarm = 0.0;
            self.bell_swing = 0.0;
            self.squash = 0.0;
            self.trail_len = 0;
            for puff in &mut self.puffs {
                if puff.is_some_and(|p| p.kind != PuffKind::Flash) {
                    *puff = None;
                }
            }
        }
        if self.flash_scale == 0.0 {
            self.flash = 0.0;
            for puff in &mut self.puffs {
                if puff.is_some_and(|p| p.kind == PuffKind::Flash) {
                    *puff = None;
                }
            }
        }
    }

    pub fn motion(&self) -> f32 {
        self.motion
    }

    /// Screen-space camera offset in pixels: a recoil kick plus decaying
    /// shake. Zero under reduced motion.
    pub fn camera_offset(&self) -> Vec2 {
        let shake = self.trauma * self.trauma * 9.0;
        vec2(
            (self.time * 47.0).sin() * shake,
            (self.time * 39.0 + 1.1).cos() * shake * 0.8 + self.kick * self.kick * 7.0,
        ) * self.motion
    }

    fn random(&mut self) -> f32 {
        // xorshift64*: deterministic, allocation-free, reseeded on rebuild.
        self.rng ^= self.rng >> 12;
        self.rng ^= self.rng << 25;
        self.rng ^= self.rng >> 27;
        let bits = self.rng.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40;
        bits as f32 / (1u64 << 24) as f32
    }

    fn spread(&mut self, amount: f32) -> f32 {
        (self.random() * 2.0 - 1.0) * amount
    }

    fn chunks(&mut self, at: Vec3, count: usize, material: Material, power: f32) {
        if self.motion == 0.0 {
            return;
        }
        let count = ((count as f32) * self.motion).ceil() as usize;
        for _ in 0..count {
            let velocity = vec3(
                self.spread(3.2),
                2.5 + self.random() * 3.5,
                self.spread(3.2),
            ) * power;
            let spin = vec3(self.spread(9.0), self.spread(9.0), self.spread(9.0));
            let size = 0.22 + self.random() * 0.26;
            let life = 1.3 + self.random() * 0.9;
            let offset = vec3(self.spread(0.4), self.spread(0.4), self.spread(0.4));
            let chunk = Chunk {
                position: at + offset,
                velocity,
                rotation: Quat::from_axis_angle(Vec3::Y, self.random() * 6.0),
                spin,
                size,
                material,
                age: 0.0,
                life,
            };
            let slot = free_slot(&mut self.chunks, |c| c.age / c.life);
            self.chunks[slot] = Some(chunk);
        }
    }

    fn puffs(&mut self, at: Vec3, count: usize, kind: PuffKind, size: f32, rise: f32) {
        let scale = match kind {
            PuffKind::Flash => self.flash_scale,
            _ => self.motion,
        };
        if scale == 0.0 {
            return;
        }
        let count = ((count as f32) * scale).ceil() as usize;
        for _ in 0..count {
            let (life, grow) = match kind {
                PuffKind::Dust => (0.9 + self.random() * 0.7, 0.9),
                PuffKind::Smoke => (0.7 + self.random() * 0.5, 1.2),
                PuffKind::Flash => (0.14, 2.6),
            };
            let puff = Puff {
                position: at + vec3(self.spread(0.35), self.random() * 0.3, self.spread(0.35)),
                velocity: vec3(
                    self.spread(1.6),
                    rise + self.random() * 0.8,
                    self.spread(1.6),
                ),
                radius: size * (0.7 + self.random() * 0.6),
                grow,
                age: 0.0,
                life,
                kind,
                spin: self.random() * 6.0,
            };
            let slot = free_slot(&mut self.puffs, |p| p.age / p.life);
            self.puffs[slot] = Some(puff);
        }
    }

    fn pop(&mut self, at: Vec3, text: PopText, life: f32) {
        // Stack fresh pops near the same spot instead of overprinting them.
        let crowd = self
            .pops
            .iter()
            .flatten()
            .filter(|pop| pop.age < 0.7 && pop.at.distance(at) < 3.5)
            .count();
        let at = at + Vec3::Y * 0.6 * crowd as f32;
        let slot = free_slot(&mut self.pops, |p| p.age / p.life);
        self.pops[slot] = Some(Pop {
            at,
            text,
            age: 0.0,
            life,
        });
    }

    fn ring(&mut self, at: Vec3, radius: f32) {
        if self.motion == 0.0 {
            return;
        }
        let slot = free_slot(&mut self.rings, |r| r.age / r.life);
        self.rings[slot] = Some(Ring {
            at,
            age: 0.0,
            life: 0.45,
            radius,
        });
    }

    fn shake(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount * self.motion).min(1.0);
    }

    /// Gus speaks unless he is already making a more important point.
    pub fn say(&mut self, banter: &mut Banter, mood: Mood) {
        let interrupt = self.bubble.is_none_or(|bubble| {
            mood_priority(mood) >= mood_priority(bubble.mood) || bubble.age > 1.2
        });
        if interrupt {
            self.bubble = Some(Bubble {
                text: banter.say(mood),
                mood,
                age: 0.0,
            });
        }
    }

    fn banner(&mut self, callout: Callout, points: u32) {
        let replace = self.banner.is_none_or(|banner| {
            callout.priority() >= banner.callout.priority() || banner.age > BANNER_LIFE * 0.6
        });
        if replace {
            self.banner = Some(Banner {
                callout,
                points,
                age: 0.0,
            });
        }
    }

    /// React to one accepted simulation event.
    pub fn event(&mut self, event: RangeEvent, banter: &mut Banter) {
        match event {
            RangeEvent::Fired {
                muzzle,
                direction,
                last,
            } => {
                self.kick = self.motion.min(1.0);
                self.squash = self.motion.min(1.0);
                self.flash = self.flash_scale;
                self.trail_len = 0;
                self.puffs(muzzle + direction * 0.4, 3, PuffKind::Flash, 0.5, 0.0);
                self.puffs(muzzle + direction * 0.6, 9, PuffKind::Smoke, 0.24, 0.9);
                self.shake(0.18);
                if last {
                    self.say(banter, Mood::LastShell);
                }
            }
            RangeEvent::Impact { at, strength } => {
                let power = (strength / 18.0).clamp(0.35, 1.0);
                self.puffs(at, (4.0 + 6.0 * power) as usize, PuffKind::Dust, 0.4, 0.6);
                self.puffs(at, 1, PuffKind::Flash, 0.6 * power, 0.0);
                self.chunks(at, 3, Material::Soot, 0.6 + power * 0.4);
                self.shake(0.3 * power);
            }
            RangeEvent::Knocked {
                at,
                target,
                points,
                chain,
                ..
            } => {
                let material = if target {
                    Material::Rust
                } else {
                    Material::Sand
                };
                self.chunks(at, if target { 9 } else { 4 }, material, 1.0);
                self.puffs(at, if target { 6 } else { 4 }, PuffKind::Dust, 0.45, 0.4);
                if target {
                    // The wobble is resolved; its word gives way to the points.
                    for pop in &mut self.pops {
                        if pop.is_some_and(|p| matches!(p.text, PopText::Word(_))) {
                            *pop = None;
                        }
                    }
                    // Support points join the total quietly; only targets pop.
                    self.pop(at + Vec3::Y * 0.8, PopText::Points { points, chain }, 1.3);
                    self.ring(at, 1.4);
                    self.shake(0.4);
                    self.hop = self.motion.min(1.0);
                    self.alarm = 0.0;
                    self.say(banter, Mood::Knocked);
                } else {
                    self.shake(0.12);
                }
            }
            RangeEvent::Wobble { at, .. } => {
                self.pop(at + Vec3::Y * 1.1, PopText::Word("WOBBLE..."), 1.0);
                self.alarm = self.motion.min(1.0);
                self.say(banter, Mood::Wobble);
            }
            RangeEvent::Callout {
                callout,
                points,
                at,
            } => {
                self.banner(callout, points);
                match callout {
                    Callout::Domino => {
                        self.shake(0.35);
                        self.say(banter, Mood::Domino);
                    }
                    Callout::OneShotWonder => {
                        self.shake(0.5);
                        self.say(banter, Mood::Wonder);
                    }
                    Callout::Bell => {
                        self.bell_swing = self.motion.min(1.0);
                        self.ring(at, 1.2);
                        self.puffs(at, 2, PuffKind::Flash, 0.7, 0.0);
                        self.say(banter, Mood::Bell);
                    }
                    Callout::Overkill => self.shake(0.25),
                    _ => {}
                }
            }
            RangeEvent::Miss { streak } => {
                let mood = if streak >= 2 && streak % 2 == 0 {
                    Mood::MissStreak
                } else {
                    Mood::Miss
                };
                self.say(banter, mood);
            }
        }
    }

    /// Advance the results-card reveal by render time. The range itself is
    /// frozen, so nothing else ages here.
    pub fn result_time(&mut self, dt: f32) {
        self.result_age = (self.result_age + dt).min(CARD_DELAY + 1.0);
        // Let the final callout fade in and hold until the card replaces it.
        if let Some(banner) = &mut self.banner {
            banner.age = (banner.age + dt).min(BANNER_LIFE * 0.5);
        }
    }

    /// 0..=1 reveal of the results card after the final beat.
    pub fn card_reveal(&self) -> f32 {
        ((self.result_age - CARD_DELAY) / 0.2).clamp(0.0, 1.0)
    }

    /// The last shell is past its miss window and the verdict is pending.
    pub fn settling(&mut self, banter: &mut Banter) {
        if !std::mem::replace(&mut self.settling_said, true) {
            self.say(banter, Mood::Settling);
        }
    }

    /// Record the newest flying ball for its smoke trail.
    pub fn sample_trail(&mut self, ball: Option<(Vec3, f32)>) {
        if self.motion == 0.0 {
            self.trail_len = 0;
            return;
        }
        match ball {
            Some((position, speed)) if speed > 6.0 => {
                self.trail.copy_within(0..TRAIL - 1, 1);
                self.trail[0] = position;
                self.trail_len = (self.trail_len + 1).min(TRAIL);
            }
            _ => self.trail_len = self.trail_len.saturating_sub(1),
        }
    }

    /// Age everything by one accepted fixed tick.
    pub fn step(&mut self, dt: f32, points: u32) {
        self.time += dt;
        self.trauma = (self.trauma - dt * 1.6).max(0.0);
        self.kick = (self.kick - dt * 5.0).max(0.0);
        self.flash = (self.flash - dt * 9.0).max(0.0);
        self.hop = (self.hop - dt * 2.2).max(0.0);
        self.alarm = (self.alarm - dt * 0.5).max(0.0);
        self.bell_swing = (self.bell_swing - dt * 0.35).max(0.0);
        self.squash = (self.squash - dt * 4.0).max(0.0);
        let target = points as f32;
        self.shown_points = if self.motion == 0.0 {
            target
        } else {
            let step = ((target - self.shown_points) * dt * 7.0).max(dt * 240.0);
            (self.shown_points + step).min(target)
        };
        for slot in &mut self.chunks {
            if let Some(chunk) = slot {
                chunk.age += dt;
                chunk.velocity.y -= 9.81 * dt;
                chunk.position += chunk.velocity * dt;
                let floor = chunk.size * 0.5;
                if chunk.position.y < floor {
                    chunk.position.y = floor;
                    chunk.velocity = vec3(
                        chunk.velocity.x * 0.55,
                        -chunk.velocity.y * 0.3,
                        chunk.velocity.z * 0.55,
                    );
                    chunk.spin *= 0.5;
                }
                let spin = chunk.spin * dt;
                if let Some(axis) = spin.try_normalize() {
                    chunk.rotation =
                        (Quat::from_axis_angle(axis, spin.length()) * chunk.rotation).normalize();
                }
                if chunk.age >= chunk.life {
                    *slot = None;
                }
            }
        }
        for slot in &mut self.puffs {
            if let Some(puff) = slot {
                puff.age += dt;
                puff.position += puff.velocity * dt;
                puff.velocity *= 1.0 - (dt * 2.5).min(1.0);
                if puff.age >= puff.life {
                    *slot = None;
                }
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
        if let Some(banner) = &mut self.banner {
            banner.age += dt;
            if banner.age >= BANNER_LIFE {
                self.banner = None;
            }
        }
        if let Some(bubble) = &mut self.bubble {
            bubble.age += dt;
            if bubble.age >= BUBBLE_LIFE {
                self.bubble = None;
            }
        }
    }

    /// World-space height a pop has risen; static under reduced motion.
    pub fn pop_rise(&self, pop: &Pop) -> f32 {
        (pop.age * 1.6).min(1.4) * self.motion.min(1.0)
    }

    /// Gus's jump height in metres.
    pub fn hop_height(&self) -> f32 {
        let t = 1.0 - self.hop;
        if self.hop > 0.0 {
            (t * std::f32::consts::PI * 2.0).sin().abs() * 0.45 * self.hop
        } else {
            0.0
        }
    }

    /// Gus's nervous lean while a tower wobbles, in radians.
    pub fn fret(&self) -> f32 {
        (self.time * 18.0).sin() * 0.12 * self.alarm
    }

    pub fn bell_angle(&self) -> f32 {
        (self.time * 7.0).sin() * 0.6 * self.bell_swing * self.bell_swing
    }

    /// Cannon carriage scale: brief squash on an accepted shot.
    pub fn carriage_scale(&self) -> Vec3 {
        let s = self.squash * self.squash;
        vec3(1.0 + 0.12 * s, 1.0 - 0.18 * s, 1.0 + 0.12 * s)
    }
}

/// The first empty slot, or the oldest by normalized age.
fn free_slot<T: Copy>(slots: &mut [Option<T>], progress: impl Fn(&T) -> f32) -> usize {
    if let Some(index) = slots.iter().position(Option::is_none) {
        return index;
    }
    let mut oldest = 0;
    let mut age = f32::NEG_INFINITY;
    for (index, slot) in slots.iter().enumerate() {
        if let Some(value) = slot
            && progress(value) > age
        {
            age = progress(value);
            oldest = index;
        }
    }
    oldest
}

#[cfg(test)]
mod tests {
    use super::*;

    fn knocked(target: bool) -> RangeEvent {
        RangeEvent::Knocked {
            at: vec3(0.0, 3.0, -4.0),
            target,
            column: 1,
            points: 100,
            chain: 1,
        }
    }

    fn live<T>(slots: &[Option<T>]) -> usize {
        slots.iter().flatten().count()
    }

    #[test]
    fn reduced_motion_clears_and_blocks_motion_but_keeps_information() {
        let mut banter = Banter::default();
        let mut juice = Juice::new();
        juice.event(knocked(true), &mut banter);
        juice.step(1.0 / 120.0, 100);
        assert!(live(&juice.chunks) > 0 && live(&juice.puffs) > 0);
        assert!(juice.camera_offset().length() > 0.0);
        juice.apply_policy(true, false, 1.0);
        assert_eq!(live(&juice.chunks), 0);
        assert_eq!(live(&juice.rings), 0);
        assert_eq!(juice.camera_offset(), Vec2::ZERO);
        assert_eq!(juice.hop_height(), 0.0);
        assert_eq!(live(&juice.pops), 1, "score pops are information");
        assert!(juice.bubble.is_some(), "Gus still talks");
        juice.event(knocked(true), &mut banter);
        juice.step(1.0 / 120.0, 200);
        assert_eq!(live(&juice.chunks), 0);
        assert_eq!(juice.shown_points, 200.0, "score snaps instead of rolling");
        let pop = juice.pops.iter().flatten().next().unwrap();
        assert_eq!(juice.pop_rise(pop), 0.0);
        // Restoring motion does not revive old debris.
        juice.apply_policy(false, false, 1.0);
        assert_eq!(live(&juice.chunks), 0);
    }

    #[test]
    fn reduced_flash_blocks_muzzle_and_impact_flashes_only() {
        let mut banter = Banter::default();
        let mut juice = Juice::new();
        juice.apply_policy(false, true, 1.0);
        juice.event(
            RangeEvent::Fired {
                muzzle: Vec3::ZERO,
                direction: Vec3::Z,
                last: false,
            },
            &mut banter,
        );
        juice.event(
            RangeEvent::Impact {
                at: Vec3::ZERO,
                strength: 20.0,
            },
            &mut banter,
        );
        assert_eq!(juice.flash, 0.0);
        assert!(
            juice
                .puffs
                .iter()
                .flatten()
                .all(|p| p.kind != PuffKind::Flash)
        );
        assert!(live(&juice.puffs) > 0, "smoke and dust remain");
        assert!(juice.kick > 0.0, "motion is independent of flash");
    }

    #[test]
    fn effects_are_bounded_deterministic_and_age_only_with_ticks() {
        let mut banter = Banter::default();
        let mut a = Juice::new();
        let mut b = Juice::new();
        for _ in 0..40 {
            a.event(knocked(true), &mut banter);
            b.event(knocked(true), &mut Banter::default());
        }
        assert_eq!(live(&a.chunks), CHUNKS);
        assert!(live(&a.puffs) <= PUFFS && live(&a.pops) == POPS);
        let positions = |j: &Juice| {
            j.chunks
                .iter()
                .flatten()
                .map(|c| c.position)
                .collect::<Vec<_>>()
        };
        assert_eq!(positions(&a), positions(&b));
        let before = positions(&a);
        // No tick, no ageing: presentation frames alone never move effects.
        assert_eq!(positions(&a), before);
        for _ in 0..400 {
            a.step(1.0 / 120.0, 0);
        }
        assert_eq!(live(&a.chunks) + live(&a.puffs) + live(&a.pops), 0);
        assert!(a.banner.is_none() && a.bubble.is_none());
        a.reset();
        a.event(knocked(true), &mut banter);
        let mut fresh = Juice::new();
        fresh.event(knocked(true), &mut banter);
        assert_eq!(positions(&a), positions(&fresh), "retries look identical");
    }

    #[test]
    fn bigger_callouts_and_moods_win_the_single_slots() {
        let mut banter = Banter::default();
        let mut juice = Juice::new();
        for callout in [Callout::Domino, Callout::Chain(2)] {
            juice.event(
                RangeEvent::Callout {
                    callout,
                    points: 0,
                    at: Vec3::ZERO,
                },
                &mut banter,
            );
        }
        assert_eq!(juice.banner.unwrap().callout, Callout::Domino);
        assert_eq!(juice.bubble.unwrap().mood, Mood::Domino);
        juice.event(RangeEvent::Miss { streak: 1 }, &mut banter);
        assert_eq!(juice.bubble.unwrap().mood, Mood::Domino, "no interruption");
        for _ in 0..200 {
            juice.step(1.0 / 120.0, 0);
        }
        juice.event(RangeEvent::Miss { streak: 1 }, &mut banter);
        assert_eq!(juice.bubble.unwrap().mood, Mood::Miss);
    }
}
