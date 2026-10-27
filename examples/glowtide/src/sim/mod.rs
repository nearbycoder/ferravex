//! Deterministic Glowtide rules. Fixed ticks, seeded randomness, bounded storage.
//!
//! The simulation never reads clocks, screens or devices. Hosts feed one
//! [`Input`] per executed tick and drain [`Event`]s afterwards; every event is
//! an *accepted* outcome (a hit that landed, a mote that was collected), so
//! presentation can never invent gameplay. All pools are allocated once in
//! [`Game::new`]; steady-state ticks do not allocate.
pub mod bot;
pub mod content;
mod foes;
pub mod grid;
mod upgrades;
mod weapons;

use content::*;
use ferravex_play::gameplay::loot::RewardRng;
use ferravex_play::gameplay::progression::Progression;
use ferravex_render::prelude::{Vec2, vec2};
pub use foes::{BossKind, Director};
pub use upgrades::{Card, ChestItem, ChestReveal, Offer};
pub use weapons::{WStats, aura_radius, lash_size, orbit_radius, weapon_stats};

/// Simulation rate. Sixty ticks keep 500 foes cheap while feeling crisp.
pub const TICK_HZ: f64 = 60.;
pub const DT: f32 = 1. / 60.;
pub const MAX_FOES: usize = 640;
pub const MAX_SHOTS: usize = 768;
pub const MAX_BULLETS: usize = 512;
pub const MAX_MOTES: usize = 700;
pub const MAX_PICKUPS: usize = 48;
pub const MAX_EVENTS: usize = 2048;
pub const MAX_ZAPS: usize = 128;
pub const MAX_TELEGRAPHS: usize = 48;
pub const MAX_ZONES: usize = 24;
pub const MAX_LASHES: usize = 24;
pub const PLAYER_RADIUS: f32 = 11.;
/// Distance foes are spawned from the player: just beyond every supported view.
pub const SPAWN_RADIUS: f32 = 720.;
/// Half extents of the rectangle star skippers ricochet inside.
pub const SKIP_BOUNDS: Vec2 = Vec2::new(520., 320.);
const DASH_SPEED: f32 = 560.;
const DASH_TIME: f32 = 0.17;
const DASH_INVULN: f32 = 0.32;
const DASH_COOLDOWN: f32 = 1.5;
const HURT_INVULN: f32 = 0.55;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Running,
    LevelUp,
    Chest,
    Won,
    Lost,
}

/// One tick of player intent. Hosts consume input edges only for executed ticks.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    pub movement: Vec2,
    /// Dash press edge.
    pub dash: bool,
    pub choice: Choice,
}

/// A level-up/treasure decision, delivered through the same tick input path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Choice {
    #[default]
    None,
    Pick(u8),
    Reroll,
    Banish(u8),
}

/// Permanent Shrine bonuses applied at run start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bonuses {
    pub might: f32,
    pub max_hp: f32,
    pub speed: f32,
    pub magnet: f32,
    pub rerolls: u8,
    pub banishes: u8,
    pub greed: f32,
    pub revives: u8,
}

impl Default for Bonuses {
    fn default() -> Self {
        Self {
            might: 1.,
            max_hp: 0.,
            speed: 1.,
            magnet: 1.,
            rerolls: 1,
            banishes: 1,
            greed: 1.,
            revives: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunConfig {
    pub character: u8,
    /// Five-minute "quick tide": the director clock runs twice as fast.
    pub quick: bool,
    pub seed: u64,
    pub bonuses: Bonuses,
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            character: 0,
            quick: false,
            seed: 0x6c75_6d69,
            bonuses: Bonuses::default(),
        }
    }
}

impl RunConfig {
    pub fn length(&self) -> f32 {
        if self.quick { 300. } else { 600. }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Player {
    pub pos: Vec2,
    pub prev: Vec2,
    pub vel: Vec2,
    pub facing: Vec2,
    pub hp: f32,
    pub max_hp: f32,
    pub dash_cd: f32,
    pub dash_time: f32,
    pub dash_dir: Vec2,
    pub invuln: f32,
    pub hurt: f32,
    pub revives: u8,
    /// External drift, e.g. Old Gulpjaw's lure.
    pub pull: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Foe {
    pub pos: Vec2,
    pub prev: Vec2,
    pub vel: Vec2,
    pub knock: Vec2,
    pub kind: FoeKind,
    pub hp: f32,
    pub max_hp: f32,
    pub radius: f32,
    pub speed: f32,
    pub damage: f32,
    pub uid: u32,
    /// Zero for ordinary foes, otherwise `ELITE_NAMES` index + 1.
    pub elite: u8,
    pub state: u8,
    pub timer: f32,
    pub aim: Vec2,
    pub hit_cd: [f32; WEAPONS],
    /// Cosmetic hit flash (seconds); presentation may ignore it under reduced flash.
    pub flash: f32,
    pub slow: f32,
    pub stun: f32,
    pub seed: f32,
    pub dead: bool,
    pub xp: u32,
    pub last_weapon: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotKind {
    Wisp,
    Lance,
    Bubble,
    Shell,
    Star,
    Stardust,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub pos: Vec2,
    pub prev: Vec2,
    pub vel: Vec2,
    pub origin: Vec2,
    pub target: Vec2,
    pub kind: ShotKind,
    pub weapon: WeaponId,
    pub damage: f32,
    pub radius: f32,
    pub life: f32,
    pub age: f32,
    pub pierce: u8,
    pub knock: f32,
    pub spin: f32,
    pub hits: [u32; 6],
    pub hit_n: u8,
    pub homing: u32,
    pub returning: bool,
    pub dead: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bullet {
    pub pos: Vec2,
    pub prev: Vec2,
    pub vel: Vec2,
    pub radius: f32,
    pub damage: f32,
    pub life: f32,
    pub age: f32,
    pub style: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    Circle {
        pos: Vec2,
        radius: f32,
    },
    Lane {
        from: Vec2,
        dir: Vec2,
        length: f32,
        width: f32,
    },
}

impl Shape {
    pub fn contains(&self, p: Vec2, pad: f32) -> bool {
        match *self {
            Shape::Circle { pos, radius } => (p - pos).length_squared() < (radius + pad).powi(2),
            Shape::Lane {
                from,
                dir,
                length,
                width,
            } => {
                let d = p - from;
                let along = d.dot(dir);
                let side = d.dot(vec2(-dir.y, dir.x)).abs();
                (0. ..=length).contains(&along) && side < width * 0.5 + pad
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Telegraph {
    pub shape: Shape,
    pub age: f32,
    pub delay: f32,
    pub damage: f32,
    /// 0 = slam, 1 = charge lane (warning only).
    pub style: u8,
}

impl Telegraph {
    pub fn progress(&self) -> f32 {
        (self.age / self.delay).clamp(0., 1.)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zap {
    pub a: Vec2,
    pub b: Vec2,
    pub age: f32,
    pub seed: u32,
    pub evolved: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lash {
    pub dir: Vec2,
    pub reach: f32,
    pub width: f32,
    pub age: f32,
    pub full: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Zone {
    pub pos: Vec2,
    pub radius: f32,
    pub life: f32,
    pub age: f32,
    pub damage: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mote {
    pub pos: Vec2,
    pub prev: Vec2,
    pub vel: Vec2,
    pub value: u32,
    pub pulled: bool,
    pub seed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickupKind {
    Coin(u32),
    Heal,
    Magnet,
    Chest(u8),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pickup {
    pub pos: Vec2,
    pub prev: Vec2,
    pub vel: Vec2,
    pub kind: PickupKind,
    pub pulled: bool,
    pub age: f32,
}

/// Accepted simulation outcomes for presentation, audio and analytics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Hit {
        pos: Vec2,
        amount: f32,
        crit: bool,
        weapon: WeaponId,
        uid: u32,
    },
    Kill {
        pos: Vec2,
        kind: FoeKind,
        elite: bool,
        weapon: WeaponId,
    },
    Hurt {
        pos: Vec2,
        amount: f32,
    },
    Dash {
        pos: Vec2,
        dir: Vec2,
    },
    Collect {
        pos: Vec2,
        value: u32,
    },
    Coin {
        pos: Vec2,
        value: u32,
    },
    Heal {
        pos: Vec2,
        amount: f32,
    },
    MagnetPulse {
        pos: Vec2,
    },
    ChestOpened {
        pos: Vec2,
    },
    LevelUp {
        level: u16,
    },
    Fire {
        weapon: WeaponId,
        pos: Vec2,
    },
    Burst {
        pos: Vec2,
        radius: f32,
        evolved: bool,
    },
    TideShift {
        tide: u8,
    },
    EliteSpawn {
        name: u8,
        kind: FoeKind,
    },
    SwarmWarning {
        kind: u8,
    },
    BossWarning {
        boss: BossKind,
    },
    BossSpawn {
        boss: BossKind,
    },
    BossPhase {
        boss: BossKind,
        phase: u8,
    },
    BossCast {
        boss: BossKind,
        pattern: u8,
    },
    BossDefeated {
        boss: BossKind,
        pos: Vec2,
    },
    Slam {
        pos: Vec2,
        radius: f32,
    },
    Revive {
        pos: Vec2,
    },
    Evolved {
        weapon: WeaponId,
    },
    Picked {
        card: Card,
    },
    Victory,
    Defeat,
}

/// Run-wide statistics for the summary screen and titles.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RunStats {
    pub damage: [f64; WEAPONS],
    pub kills_by: [u32; WEAPONS],
    pub kills: u32,
    pub elites: u32,
    pub coins: u64,
    pub dashes: u32,
    pub damage_taken: f32,
    pub healed: f32,
    pub motes: u32,
    pub chests: u32,
    pub brine_down: bool,
    pub gulpjaw_down: bool,
    /// Remaining fraction of Gulpjaw's health when the run ended, if met.
    pub gulpjaw_left: Option<f32>,
    pub dropped_events: u32,
}

/// Combined modifiers from character, Shrine and passives.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mods {
    pub might: f32,
    pub haste: f32,
    pub area: f32,
    pub amount: u32,
    pub crit: f32,
    pub crit_mult: f32,
    pub luck: f32,
    pub magnet: f32,
    pub move_speed: f32,
    pub armor: f32,
    pub regen: f32,
    pub max_hp: f32,
    pub dash_cd: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponSlot {
    pub id: WeaponId,
    pub level: u8,
    pub evolved: bool,
    pub cooldown: f32,
    pub burst: u8,
    pub burst_timer: f32,
    /// Remaining active time for duration weapons (orbit), or `f32::INFINITY`.
    pub active: f32,
    pub angle: f32,
    pub side: u8,
}

pub struct Game {
    pub config: RunConfig,
    rng: RewardRng,
    pub phase: Phase,
    pub time: f32,
    pub ticks: u64,
    pub player: Player,
    pub weapons: [Option<WeaponSlot>; WEAPON_SLOTS],
    pub passives: [Option<(PassiveId, u8)>; PASSIVE_SLOTS],
    pub mods: Mods,
    pub foes: Vec<Foe>,
    next_uid: u32,
    pub grid: grid::Grid,
    pub shots: Vec<Shot>,
    pub bullets: Vec<Bullet>,
    pub motes: Vec<Mote>,
    pub pickups: Vec<Pickup>,
    pub zaps: Vec<Zap>,
    pub lashes: Vec<Lash>,
    pub zones: Vec<Zone>,
    pub telegraphs: Vec<Telegraph>,
    pub director: Director,
    pub progression: Progression,
    pub pending_levels: u16,
    pub offer: Offer,
    pub rerolls: u8,
    pub banishes: u8,
    pub banished: u32,
    pub chest: ChestReveal,
    pub events: Vec<Event>,
    pub stats: RunStats,
    /// Ticks spent in the current modal (level-up/treasure), for the demo bot.
    pub menu_ticks: u32,
    scratch: Vec<u16>,
}

impl Game {
    pub fn new(config: RunConfig) -> Self {
        let character = &CHARACTERS[usize::from(config.character).min(CHARACTERS.len() - 1)];
        let mut game = Self {
            config,
            rng: RewardRng::new(config.seed),
            phase: Phase::Running,
            time: 0.,
            ticks: 0,
            player: Player {
                pos: Vec2::ZERO,
                prev: Vec2::ZERO,
                vel: Vec2::ZERO,
                facing: Vec2::X,
                hp: 100.,
                max_hp: 100.,
                dash_cd: 0.,
                dash_time: 0.,
                dash_dir: Vec2::X,
                invuln: 0.,
                hurt: 0.,
                revives: config.bonuses.revives,
                pull: Vec2::ZERO,
            },
            weapons: [None; WEAPON_SLOTS],
            passives: [None; PASSIVE_SLOTS],
            mods: upgrades::base_mods(),
            foes: Vec::with_capacity(MAX_FOES),
            next_uid: 1,
            grid: grid::Grid::new(MAX_FOES),
            shots: Vec::with_capacity(MAX_SHOTS),
            bullets: Vec::with_capacity(MAX_BULLETS),
            motes: Vec::with_capacity(MAX_MOTES),
            pickups: Vec::with_capacity(MAX_PICKUPS),
            zaps: Vec::with_capacity(MAX_ZAPS),
            lashes: Vec::with_capacity(MAX_LASHES),
            zones: Vec::with_capacity(MAX_ZONES),
            telegraphs: Vec::with_capacity(MAX_TELEGRAPHS),
            director: Director::default(),
            progression: Progression::new(5, 10, 200).expect("authored XP curve"),
            pending_levels: 0,
            offer: Offer::default(),
            rerolls: config.bonuses.rerolls,
            banishes: config.bonuses.banishes,
            banished: 0,
            chest: ChestReveal::default(),
            events: Vec::with_capacity(MAX_EVENTS),
            stats: RunStats::default(),
            menu_ticks: 0,
            scratch: Vec::with_capacity(MAX_FOES),
        };
        game.add_weapon(character.weapon);
        game.recompute_mods();
        game.player.hp = game.player.max_hp;
        game
    }

    /// Logical minutes on the full ten-minute tide (quick tides run 2× faster).
    pub fn minute(&self) -> f32 {
        self.time / 60. * (600. / self.config.length())
    }

    pub fn tide(&self) -> usize {
        let m = self.minute();
        TIDES.iter().rposition(|t| m >= t.start).unwrap_or(0)
    }

    pub fn level(&self) -> u16 {
        self.progression.level()
    }

    /// XP progress toward the next level, `0..=1`.
    pub fn xp_fraction(&self) -> f32 {
        match self.progression.next_cost() {
            Some(cost) => (self.progression.xp() as f64 / cost as f64).clamp(0., 1.) as f32,
            None => 1.,
        }
    }

    pub fn character(&self) -> &'static Character {
        &CHARACTERS[usize::from(self.config.character).min(CHARACTERS.len() - 1)]
    }

    pub fn is_over(&self) -> bool {
        matches!(self.phase, Phase::Won | Phase::Lost)
    }

    pub fn boss(&self) -> Option<&Foe> {
        self.foes.iter().find(|f| f.kind.is_boss() && !f.dead)
    }

    pub fn weapon_count(&self) -> usize {
        self.weapons.iter().flatten().count()
    }

    pub fn passive_level(&self, id: PassiveId) -> u8 {
        self.passives
            .iter()
            .flatten()
            .find(|(p, _)| *p == id)
            .map_or(0, |(_, l)| *l)
    }

    pub fn weapon(&self, id: WeaponId) -> Option<&WeaponSlot> {
        self.weapons.iter().flatten().find(|w| w.id == id)
    }

    /// Advance one fixed tick. The host stops a catch-up batch once
    /// [`Self::is_over`] becomes true.
    pub fn update(&mut self, input: Input) {
        if self.is_over() {
            return;
        }
        self.ticks += 1;
        match self.phase {
            Phase::LevelUp | Phase::Chest => {
                self.menu_ticks = self.menu_ticks.saturating_add(1);
                self.choose(input.choice);
                return;
            }
            Phase::Running => {}
            Phase::Won | Phase::Lost => return,
        }
        let dt = DT;
        self.time += dt;
        self.player.prev = self.player.pos;
        for f in &mut self.foes {
            f.prev = f.pos;
        }
        for s in &mut self.shots {
            s.prev = s.pos;
        }
        for b in &mut self.bullets {
            b.prev = b.pos;
        }
        for m in &mut self.motes {
            m.prev = m.pos;
        }
        for p in &mut self.pickups {
            p.prev = p.pos;
        }
        self.update_player(input, dt);
        self.direct(dt);
        let center = self.player.pos;
        self.grid.rebuild(center, self.foes.iter().map(|f| f.pos));
        self.update_foes(dt);
        self.update_bosses(dt);
        self.update_weapons(dt);
        self.update_shots(dt);
        self.update_bullets(dt);
        self.update_telegraphs(dt);
        self.update_zones(dt);
        self.update_pickups(dt);
        self.cleanup();
        self.check_outcome();
        if self.phase == Phase::Running && self.pending_levels > 0 {
            self.pending_levels -= 1;
            self.enter_level_up();
        }
    }

    pub(crate) fn rand(&mut self) -> f32 {
        (self.rng.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub(crate) fn rand_range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.rand()
    }

    pub(crate) fn rand_dir(&mut self) -> Vec2 {
        let a = self.rand() * std::f32::consts::TAU;
        vec2(a.cos(), a.sin())
    }

    pub(crate) fn rng(&mut self) -> &mut RewardRng {
        &mut self.rng
    }

    pub(crate) fn emit(&mut self, event: Event) {
        if self.events.len() < MAX_EVENTS {
            self.events.push(event);
        } else {
            self.stats.dropped_events += 1;
        }
    }

    /// Low-priority events keep headroom for kills, level-ups and boss beats.
    fn emit_minor(&mut self, event: Event) {
        if self.events.len() < MAX_EVENTS - 256 {
            self.events.push(event);
        } else {
            self.stats.dropped_events += 1;
        }
    }

    /// Move accepted events into the caller's (reused) buffer.
    pub fn drain_events(&mut self, out: &mut Vec<Event>) {
        let room = out.capacity().saturating_sub(out.len());
        let n = room.min(self.events.len());
        out.extend(self.events.drain(..n));
        self.events.clear();
    }

    fn update_player(&mut self, input: Input, dt: f32) {
        let mods = self.mods;
        let p = &mut self.player;
        p.invuln = (p.invuln - dt).max(0.);
        p.hurt = (p.hurt - dt).max(0.);
        p.dash_cd = (p.dash_cd - dt).max(0.);
        let mut movement = input.movement;
        if !movement.is_finite() {
            movement = Vec2::ZERO;
        }
        let len = movement.length();
        if len > 1. {
            movement /= len;
        }
        if movement.length_squared() > 0.01 {
            p.facing = movement.normalize();
        }
        let mut dashed = false;
        if input.dash && p.dash_cd <= 0. && p.dash_time <= 0. {
            p.dash_dir = if movement.length_squared() > 0.01 {
                movement.normalize()
            } else {
                p.facing
            };
            p.dash_time = DASH_TIME;
            p.dash_cd = DASH_COOLDOWN * mods.dash_cd;
            p.invuln = p.invuln.max(DASH_INVULN);
            dashed = true;
        }
        let speed = 150. * mods.move_speed;
        let target = if p.dash_time > 0. {
            p.dash_time -= dt;
            p.dash_dir * DASH_SPEED
        } else {
            movement * speed
        };
        // Snappy but not twitchy: quick acceleration, quicker stopping.
        let rate = if p.dash_time > 0. { 60. } else { 18. };
        p.vel += (target - p.vel) * (1. - (-rate * dt).exp());
        p.pos += (p.vel + p.pull) * dt;
        p.pull = Vec2::ZERO;
        if mods.regen > 0. && p.hp < p.max_hp && p.hp > 0. {
            let before = p.hp;
            p.hp = (p.hp + mods.regen * dt).min(p.max_hp);
            self.stats.healed += p.hp - before;
        }
        if dashed {
            self.stats.dashes += 1;
            let (pos, dir) = (self.player.pos, self.player.dash_dir);
            self.emit(Event::Dash { pos, dir });
        }
    }

    /// Apply contact or hazard damage, honoring dash/hurt invulnerability.
    pub(crate) fn hurt_player(&mut self, amount: f32) -> bool {
        if self.player.invuln > 0. || self.player.hp <= 0. {
            return false;
        }
        let amount = (amount - self.mods.armor).max(1.);
        self.player.hp -= amount;
        self.player.invuln = HURT_INVULN;
        self.player.hurt = 0.3;
        self.stats.damage_taken += amount;
        let pos = self.player.pos;
        self.emit(Event::Hurt { pos, amount });
        true
    }

    pub(crate) fn heal(&mut self, amount: f32) {
        let before = self.player.hp;
        self.player.hp = (self.player.hp + amount).min(self.player.max_hp);
        let healed = self.player.hp - before;
        if healed > 0.05 {
            self.stats.healed += healed;
            let pos = self.player.pos;
            self.emit_minor(Event::Heal {
                pos,
                amount: healed,
            });
        }
    }

    /// Damage foe `i` from `weapon`. Returns true if this hit defeated it.
    pub(crate) fn damage_foe(
        &mut self,
        i: usize,
        base: f32,
        weapon: WeaponId,
        push: Vec2,
        knock: f32,
    ) -> bool {
        if i >= self.foes.len() || self.foes[i].dead {
            return false;
        }
        let crit_roll = self.rand();
        let crit = crit_roll < self.mods.crit;
        let amount = base * self.mods.might * if crit { self.mods.crit_mult } else { 1. };
        let f = &mut self.foes[i];
        let dealt = amount.min(f.hp.max(0.));
        f.hp -= amount;
        f.flash = 0.11;
        f.last_weapon = weapon as u8;
        let sturdy = f.kind.info().sturdiness + if f.elite > 0 { 0.35 } else { 0. };
        f.knock += push.normalize_or_zero() * knock * (1. - sturdy.min(1.));
        let (pos, uid, killed) = (f.pos, f.uid, f.hp <= 0.);
        self.stats.damage[weapon as usize] += f64::from(dealt);
        self.emit_minor(Event::Hit {
            pos,
            amount,
            crit,
            weapon,
            uid,
        });
        if killed {
            self.kill(i, weapon);
        }
        killed
    }

    fn kill(&mut self, i: usize, weapon: WeaponId) {
        let f = self.foes[i];
        self.foes[i].dead = true;
        self.stats.kills += 1;
        self.stats.kills_by[weapon as usize] += 1;
        if f.elite > 0 {
            self.stats.elites += 1;
        }
        self.emit(Event::Kill {
            pos: f.pos,
            kind: f.kind,
            elite: f.elite > 0,
            weapon,
        });
        if f.kind.is_boss() {
            self.boss_defeated(f);
            return;
        }
        let xp_mult = if self.config.quick { 1.7 } else { 1. };
        let value = ((f.xp as f32) * xp_mult).round().max(1.) as u32;
        self.drop_mote(f.pos, value);
        if f.elite > 0 {
            self.drop_pickup(f.pos, PickupKind::Chest(1));
            for _ in 0..6 {
                let off = self.rand_dir() * self.rand_range(10., 40.);
                self.drop_mote(f.pos + off, value);
            }
        } else {
            let roll = self.rand();
            let luck = self.mods.luck;
            if roll < 0.012 * luck {
                let value = 1 + (self.rand() * 3.) as u32;
                self.drop_pickup(f.pos, PickupKind::Coin(value));
            } else if roll < 0.012 * luck + 0.0035 {
                self.drop_pickup(f.pos, PickupKind::Heal);
            } else if roll < 0.012 * luck + 0.0035 + 0.0007 {
                self.drop_pickup(f.pos, PickupKind::Magnet);
            }
        }
        if let Some(slot) = self.weapon(WeaponId::LanternAura)
            && slot.evolved
            && weapon == WeaponId::LanternAura
        {
            self.heal(0.4);
        }
    }

    pub(crate) fn drop_mote(&mut self, pos: Vec2, value: u32) {
        if self.motes.len() < MAX_MOTES {
            let seed = self.rand();
            self.motes.push(Mote {
                pos,
                prev: pos,
                vel: Vec2::ZERO,
                value,
                pulled: false,
                seed,
            });
        } else {
            // Bounded storage: fold the value into an existing mote, preferring
            // one that is not already flying to the player.
            let k = (self.rng.next_u64() % self.motes.len() as u64) as usize;
            let index = (0..self.motes.len())
                .map(|o| (k + o) % self.motes.len())
                .take(16)
                .find(|&j| !self.motes[j].pulled)
                .unwrap_or(k);
            self.motes[index].value = self.motes[index].value.saturating_add(value);
        }
    }

    pub(crate) fn drop_pickup(&mut self, pos: Vec2, kind: PickupKind) {
        if self.pickups.len() < MAX_PICKUPS {
            self.pickups.push(Pickup {
                pos,
                prev: pos,
                vel: Vec2::ZERO,
                kind,
                pulled: false,
                age: 0.,
            });
        } else if let PickupKind::Chest(_) = kind {
            // Treasure is never lost: replace the oldest non-chest pickup.
            if let Some(j) = self
                .pickups
                .iter()
                .position(|p| !matches!(p.kind, PickupKind::Chest(_)))
            {
                self.pickups[j] = Pickup {
                    pos,
                    prev: pos,
                    vel: Vec2::ZERO,
                    kind,
                    pulled: false,
                    age: 0.,
                };
            }
        }
    }

    fn update_pickups(&mut self, dt: f32) {
        let player = self.player.pos;
        let reach = 64. * self.mods.magnet;
        let reach2 = reach * reach;
        let mut gained = 0u64;
        let mut i = 0;
        while i < self.motes.len() {
            let m = &mut self.motes[i];
            let delta = player - m.pos;
            let d2 = delta.length_squared();
            if !m.pulled && d2 < reach2 {
                m.pulled = true;
                // A tiny hop away before the slurp reads as "caught".
                m.vel = -delta.normalize_or_zero() * 110.;
            }
            if m.pulled {
                let dir = delta.normalize_or_zero();
                m.vel += dir * 2300. * dt;
                let speed = m.vel.length();
                if speed > 1100. {
                    m.vel *= 1100. / speed;
                }
                m.pos += m.vel * dt;
                if (player - m.pos).length_squared() < 16. * 16. || d2 < 9. {
                    let (pos, value) = (m.pos, m.value);
                    self.motes.swap_remove(i);
                    gained += u64::from(value);
                    self.stats.motes += 1;
                    self.emit_minor(Event::Collect { pos, value });
                    continue;
                }
            }
            i += 1;
        }
        if gained > 0 {
            let report = self.progression.award_report(gained);
            self.pending_levels = self.pending_levels.saturating_add(report.promotions);
        }
        let mut i = 0;
        while i < self.pickups.len() {
            let p = &mut self.pickups[i];
            p.age += dt;
            let delta = player - p.pos;
            let d2 = delta.length_squared();
            let grab = match p.kind {
                PickupKind::Chest(_) => 24.,
                _ => reach * 0.8,
            };
            if !p.pulled && d2 < grab * grab {
                p.pulled = true;
                p.vel = -delta.normalize_or_zero() * 80.;
            }
            if p.pulled {
                p.vel += delta.normalize_or_zero() * 1800. * dt;
                p.vel = p.vel.clamp_length_max(900.);
                p.pos += p.vel * dt;
            }
            if p.pulled && (player - p.pos).length_squared() < 18. * 18. {
                let (pos, kind) = (p.pos, p.kind);
                self.pickups.swap_remove(i);
                self.collect(pos, kind);
                if self.phase != Phase::Running {
                    return;
                }
                continue;
            }
            i += 1;
        }
    }

    fn collect(&mut self, pos: Vec2, kind: PickupKind) {
        match kind {
            PickupKind::Coin(value) => {
                let value = ((value as f32) * self.config.bonuses.greed).round() as u32;
                self.stats.coins += u64::from(value);
                self.emit(Event::Coin { pos, value });
            }
            PickupKind::Heal => {
                let amount = 30. + self.player.max_hp * 0.1;
                self.heal(amount);
            }
            PickupKind::Magnet => {
                for m in &mut self.motes {
                    m.pulled = true;
                }
                self.emit(Event::MagnetPulse { pos });
            }
            PickupKind::Chest(tier) => {
                self.stats.chests += 1;
                self.emit(Event::ChestOpened { pos });
                self.open_chest(tier);
            }
        }
    }

    fn update_telegraphs(&mut self, dt: f32) {
        let mut i = 0;
        while i < self.telegraphs.len() {
            self.telegraphs[i].age += dt;
            let t = self.telegraphs[i];
            if t.age >= t.delay {
                if t.damage > 0. && t.shape.contains(self.player.pos, PLAYER_RADIUS * 0.6) {
                    self.hurt_player(t.damage);
                }
                if let Shape::Circle { pos, radius } = t.shape
                    && t.style == 0
                {
                    self.emit(Event::Slam { pos, radius });
                }
                self.telegraphs.swap_remove(i);
                continue;
            }
            i += 1;
        }
    }

    fn update_bullets(&mut self, dt: f32) {
        let player = self.player.pos;
        let mut i = 0;
        while i < self.bullets.len() {
            let b = &mut self.bullets[i];
            b.age += dt;
            b.pos += b.vel * dt;
            let r = b.radius + PLAYER_RADIUS * 0.7;
            if (b.pos - player).length_squared() < r * r {
                let damage = b.damage;
                if self.hurt_player(damage) || self.player.invuln > 0. {
                    // Bullets fizzle on contact, even while dashing through them.
                    if self.player.dash_time <= 0. {
                        self.bullets.swap_remove(i);
                        continue;
                    }
                }
            }
            if self.bullets[i].age >= self.bullets[i].life {
                self.bullets.swap_remove(i);
                continue;
            }
            i += 1;
        }
    }

    fn update_zones(&mut self, dt: f32) {
        for z in 0..self.zones.len() {
            self.zones[z].age += dt;
            let zone = self.zones[z];
            let mut scratch = std::mem::take(&mut self.scratch);
            scratch.clear();
            self.grid.visit(zone.pos, zone.radius + 30., |j| {
                if scratch.len() < scratch.capacity() {
                    scratch.push(j as u16);
                }
            });
            for &j in &scratch {
                let j = usize::from(j);
                if j >= self.foes.len() || self.foes[j].dead {
                    continue;
                }
                let f = &mut self.foes[j];
                let d = zone.pos - f.pos;
                let rr = zone.radius + f.radius;
                if d.length_squared() > rr * rr {
                    continue;
                }
                if !f.kind.is_boss() {
                    f.pos += d.normalize_or_zero() * 70. * dt;
                }
                if f.hit_cd[WeaponId::BubbleMortar as usize] <= 0. {
                    f.hit_cd[WeaponId::BubbleMortar as usize] = 0.3;
                    self.damage_foe(j, zone.damage, WeaponId::BubbleMortar, Vec2::ZERO, 0.);
                }
            }
            self.scratch = scratch;
        }
        self.zones.retain(|z| z.age < z.life);
    }

    fn cleanup(&mut self) {
        let mut i = 0;
        while i < self.foes.len() {
            if self.foes[i].dead {
                self.foes.swap_remove(i);
            } else {
                i += 1;
            }
        }
        self.shots.retain(|s| !s.dead && s.age < s.life);
        for z in &mut self.zaps {
            z.age += DT;
        }
        self.zaps.retain(|z| z.age < 0.22);
        for l in &mut self.lashes {
            l.age += DT;
        }
        self.lashes.retain(|l| l.age < 0.24);
    }

    fn check_outcome(&mut self) {
        if self.player.hp > 0. {
            return;
        }
        if self.player.revives > 0 {
            self.player.revives -= 1;
            self.player.hp = self.player.max_hp * 0.5;
            self.player.invuln = 2.;
            let pos = self.player.pos;
            // A revival shockwave clears breathing room without awarding kills.
            for f in &mut self.foes {
                let d = f.pos - pos;
                let d2 = d.length_squared();
                if d2 < 260. * 260. && !f.kind.is_boss() {
                    f.knock += d.normalize_or_zero() * 520.;
                }
            }
            self.bullets.clear();
            self.emit(Event::Revive { pos });
            return;
        }
        self.player.hp = 0.;
        self.phase = Phase::Lost;
        self.stats.gulpjaw_left = self
            .foes
            .iter()
            .find(|f| f.kind == FoeKind::Gulpjaw)
            .map(|f| (f.hp / f.max_hp).clamp(0., 1.));
        self.emit(Event::Defeat);
    }

    /// Total damage dealt this run.
    pub fn total_damage(&self) -> f64 {
        self.stats.damage.iter().sum()
    }

    /// End the run now as a loss (the pause menu's "give up").
    pub fn surrender(&mut self) {
        if self.is_over() {
            return;
        }
        self.player.hp = 0.;
        self.phase = Phase::Lost;
        self.stats.gulpjaw_left = self
            .foes
            .iter()
            .find(|f| f.kind == FoeKind::Gulpjaw)
            .map(|f| (f.hp / f.max_hp).clamp(0., 1.));
        self.emit(Event::Defeat);
    }

    #[doc(hidden)]
    pub fn debug_level_up(&mut self) {
        self.enter_level_up();
    }

    #[doc(hidden)]
    pub fn debug_chest(&mut self) {
        self.open_chest(2);
    }

    #[doc(hidden)]
    pub fn debug_end(&mut self, win: bool) {
        if win {
            self.phase = Phase::Won;
            self.stats.gulpjaw_down = true;
            self.stats.gulpjaw_left = Some(0.);
            self.emit(Event::Victory);
        } else {
            self.surrender();
        }
    }

    /// Debug/benchmark helper: populate a dense late-game horde around the player.
    pub fn stress(&mut self, count: usize) {
        let center = self.player.pos;
        for k in 0..count.min(MAX_FOES) {
            let kind = [
                FoeKind::Driftlet,
                FoeKind::Mothling,
                FoeKind::Grumpfish,
                FoeKind::Sneel,
                FoeKind::Urchin,
                FoeKind::Crabbo,
            ][k % 6];
            let a = k as f32 * 2.399_963;
            let r = 160. + (k as f32).sqrt() * 26.;
            let pos = center + vec2(a.cos(), a.sin()) * r;
            self.spawn_foe(kind, pos, 6.0, 0);
        }
        for k in 0..200 {
            let a = k as f32 * 0.7;
            let p = center + vec2(a.cos(), a.sin()) * (120. + k as f32 * 2.);
            self.drop_mote(p, 1);
        }
    }
}

#[cfg(test)]
mod tests;
