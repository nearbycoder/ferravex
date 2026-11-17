//! Fixed-tick naval artillery rules, independent of the renderer and platform IO.
//!
//! ```
//! use stormbound::{Game, Phase};
//! use ferravex_arcade::{Input, Outcome};
//! let mut game = Game::new();
//! assert_eq!(game.phase(), Phase::Briefing);
//! game.step(&Input { primary: true, ..Input::default() });
//! assert_eq!(game.phase(), Phase::Aim);
//! let ammunition = game.ammunition();
//! game.step(&Input { primary: true, ..Input::default() });
//! assert_eq!(game.phase(), Phase::Flight);
//! assert_eq!(game.ammunition()[0], ammunition[0] - 1);
//! assert_eq!(game.outcome(), Outcome::Playing);
//! ```
use crate::content::{BATTLES, Battle, Tactic};
use crate::layout;
use ferravex_arcade::{Cue, Input, Outcome, TICK_DT};
use ferravex_math::{Quat, Vec2, Vec3};
use ferravex_play::camera::{Blocker, Sweep};

const GRAVITY: f32 = 160.;
const TRACE_CAP: usize = 148;
const FLIGHT_LIMIT: u16 = 420;
const PLAYER_X: f32 = 100.;

/// Rules phases. A setback is recoverable until both lifeboats are spent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Briefing,
    Aim,
    Flight,
    EnemyReady,
    Dock,
    Setback,
    Liberated,
    Sunk,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ammo {
    Round,
    Chain,
    Shell,
}
impl Ammo {
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Round => 0,
            Self::Chain => 1,
            Self::Shell => 2,
        }
    }
    fn from_slot(slot: usize) -> Option<Self> {
        match slot {
            1 => Some(Self::Round),
            2 => Some(Self::Chain),
            3 => Some(Self::Shell),
            _ => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ship {
    pub x: f32,
    pub hp: [u16; 4],
    pub max: [u16; 4],
    pub armor: [u16; 3],
}
impl Ship {
    fn player() -> Self {
        Self {
            x: PLAYER_X,
            hp: [60, 75, 60, 50],
            max: [60, 75, 60, 50],
            armor: [0; 3],
        }
    }
    fn enemy(index: usize) -> Self {
        let n = index as u16;
        let hp = [45 + n * 8, 60 + n * 11, 45 + n * 8, 40 + n];
        Self {
            x: BATTLES[index].x,
            hp,
            max: hp,
            armor: [BATTLES[index].armor; 3],
        }
    }
    pub(crate) fn hull(self) -> u16 {
        self.hp[..3].iter().sum()
    }
    fn hull_max(self) -> u16 {
        self.max[..3].iter().sum()
    }
    pub(crate) fn section_center(self, part: usize) -> Vec2 {
        if part == 3 {
            Vec2::new(self.x, 75.)
        } else {
            Vec2::new(self.x + (part as f32 - 1.) * 30., 24.)
        }
    }
    fn blocker(self, part: usize) -> Blocker {
        let p = self.section_center(part);
        Blocker::Box {
            center: Vec3::new(p.x, p.y, 0.),
            half: if part == 3 {
                Vec3::new(5., 35., 5.)
            } else {
                Vec3::new(15., 14., 8.)
            },
            rotation: Quat::IDENTITY,
        }
    }
    pub(crate) fn repair(&mut self, mut amount: u16) {
        // Mend the weakest section first so a wrecked section can sail again.
        while amount > 0 && self.hull() < self.hull_max() {
            let part = (0..3)
                .filter(|&p| self.hp[p] < self.max[p])
                .min_by_key(|&p| self.hp[p])
                .unwrap_or(0);
            self.hp[part] += 1;
            amount -= 1;
        }
        self.hp[3] = (self.hp[3] + 25).min(self.max[3]);
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Setting {
    pub angle: u8,
    pub power: u8,
    pub ammo: Ammo,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ball {
    pub pos: Vec2,
    velocity: Vec2,
    pub from_player: bool,
    pub ammo: Ammo,
    age: u16,
}
impl Ball {
    fn launch(ship: Ship, from_player: bool, setting: Setting) -> Self {
        let direction = if from_player { 1. } else { -1. };
        let radians = f32::from(setting.angle).to_radians();
        let speed = 140. + 3. * f32::from(setting.power);
        Self {
            pos: Vec2::new(ship.x + direction * 50., 40.),
            velocity: Vec2::new(direction * speed * radians.cos(), speed * radians.sin()),
            from_player,
            ammo: setting.ammo,
            age: 0,
        }
    }
    fn advance(&mut self, wind: f32) -> Vec2 {
        let previous = self.pos;
        self.velocity += Vec2::new(wind, -GRAVITY) * TICK_DT;
        self.pos += self.velocity * TICK_DT;
        self.age += 1;
        previous
    }
    fn ended(self) -> bool {
        self.pos.y <= 0. || self.pos.x < -40. || self.pos.x > 840. || self.age >= FLIGHT_LIMIT
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Trace {
    pub points: [Vec2; TRACE_CAP],
    pub len: usize,
}
impl Trace {
    fn new() -> Self {
        Self {
            points: [Vec2::ZERO; TRACE_CAP],
            len: 0,
        }
    }
    fn push(&mut self, p: Vec2) {
        if self.len < TRACE_CAP {
            self.points[self.len] = p;
            self.len += 1;
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Forecast {
    pub trace: Trace,
    pub contact: Option<(usize, usize)>,
    pub end: Vec2,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Checkpoint {
    ship: Ship,
    score: u32,
    credits: u16,
}
/// Why a sortie ended in a recoverable setback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cause {
    Retreat,
    Sunk,
    Empty,
}
/// One thing the latest tick accepted, with its world position where it has
/// one. Presentation reads these to place effects; rules never read them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Event {
    /// The briefing closed and the battle began.
    Engaged,
    /// A cannon fired from its muzzle.
    Fired { player: bool, at: Vec2 },
    /// A ball struck `side`'s section `part` (3 is the mast). Integrity of
    /// every section before and after, including shell splash, and the
    /// struck hull section's armor before and after.
    Hit {
        side: usize,
        part: usize,
        at: Vec2,
        before: [u16; 4],
        after: [u16; 4],
        armor: [u16; 2],
    },
    /// A ball fell into the sea or left the chart.
    Splash { at: Vec2 },
    /// The selected ammunition changed.
    Loaded(Ammo),
    /// The navigator's firing table was shown (`true`) or hidden.
    Table(bool),
    /// Fire was refused: the selected magazine is empty.
    Empty(Ammo),
    /// The enemy began laying its next volley.
    EnemyAims,
    /// The enemy's fallen rigging cancelled its volley.
    Fouled,
    /// The wind changed between exchanges.
    Gust(f32),
    /// The enemy ship sank and paid `salvage`.
    Sank { salvage: u16 },
    /// A dock purchase in `slot` (1-4) spent `cost`.
    Bought { slot: usize, cost: u16 },
    /// A dock purchase was refused: `short` salvage was missing, or zero
    /// when the item is already full or capped.
    Refused { slot: usize, short: u16 },
    /// The ship sailed for the next island.
    Sailed,
    /// A setback began.
    Setback(Cause),
    /// A lifeboat restored the island approach.
    Rescued,
    /// The captain retired and the voyage ended.
    Retired,
}
const EVENTS: usize = 8;
/// The bounded event list of the latest tick; cleared when the next executed
/// tick starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Events {
    list: [Option<Event>; EVENTS],
}
impl Events {
    const fn new() -> Self {
        Self {
            list: [None; EVENTS],
        }
    }
    fn push(&mut self, event: Event) {
        if let Some(slot) = self.list.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(event);
        }
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = Event> + '_ {
        self.list.iter().map_while(|event| *event)
    }
}
/// A read-only forecast of what the current aim would do on contact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Preview {
    pub side: usize,
    pub part: usize,
    pub before: [u16; 4],
    pub after: [u16; 4],
}

/// An eight-battle single-player campaign. Storage and flight duration are bounded.
#[derive(Clone, Debug, PartialEq)]
pub struct Game {
    pub(crate) phase: Phase,
    pub(crate) battle: usize,
    pub(crate) ships: [Ship; 2],
    pub(crate) setting: Setting,
    pub(crate) ammo: [u8; 3],
    enemy_ammo: [u8; 3],
    pub(crate) wind: f32,
    pub(crate) ball: Option<Ball>,
    pub(crate) trace: Trace,
    pub(crate) history: Trace,
    pub(crate) forecast: Forecast,
    pub(crate) last_setting: Option<Setting>,
    pub(crate) table: Setting,
    pub(crate) show_table: bool,
    pub(crate) round: u16,
    pub(crate) timer: u16,
    pub(crate) ticks: u64,
    pub(crate) score: u32,
    pub(crate) credits: u16,
    pub(crate) upgrades: [u8; 3],
    pub(crate) lifeboats: u8,
    pub(crate) shots: u16,
    pub(crate) hits: u16,
    pub(crate) wins: u8,
    pub(crate) log: &'static str,
    pub(crate) impact: Option<Vec2>,
    pub(crate) impact_ticks: u16,
    pub(crate) last_damage: u16,
    pub(crate) enemy_setting: Setting,
    salvo_left: u8,
    checkpoint: Checkpoint,
    pub(crate) events: Events,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Begin at the first island briefing with a sound ship and two lifeboats.
    pub fn new() -> Self {
        let ship = Ship::player();
        let setting = Setting {
            angle: 40,
            power: 29,
            ammo: Ammo::Round,
        };
        let mut game = Self {
            phase: Phase::Briefing,
            battle: 0,
            ships: [ship, Ship::enemy(0)],
            setting,
            ammo: [8, 2, 2],
            enemy_ammo: [12, 3, 3],
            wind: 0.,
            ball: None,
            trace: Trace::new(),
            history: Trace::new(),
            forecast: Forecast {
                trace: Trace::new(),
                contact: None,
                end: Vec2::ZERO,
            },
            last_setting: None,
            table: setting,
            show_table: false,
            round: 1,
            timer: 0,
            ticks: 0,
            score: 0,
            credits: 40,
            upgrades: [0; 3],
            lifeboats: 2,
            shots: 0,
            hits: 0,
            wins: 0,
            log: BATTLES[0].advice,
            impact: None,
            impact_ticks: 0,
            last_damage: 0,
            enemy_setting: setting,
            salvo_left: 0,
            checkpoint: Checkpoint {
                ship,
                score: 0,
                credits: 40,
            },
            events: Events::new(),
        };
        game.refresh_aim();
        game
    }
    /// Current rules phase, including recoverable setbacks and terminal endings.
    pub fn phase(&self) -> Phase {
        self.phase
    }
    /// One-based authored battle number (1..=8).
    pub fn battle_number(&self) -> usize {
        self.battle + 1
    }
    /// Remaining roundshot, chainshot and explosive shells, respectively.
    pub fn ammunition(&self) -> [u8; 3] {
        self.ammo
    }
    /// Player hull integrity, excluding the independently destructible mast.
    pub fn hull(&self) -> u16 {
        self.ships[0].hull()
    }
    /// Awarded score. A battle awards salvage and points only once.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// Won/lost is terminal. A local setback remains Playing until retry/abandon.
    pub fn outcome(&self) -> Outcome {
        match self.phase {
            Phase::Liberated => Outcome::Won,
            Phase::Sunk => Outcome::Lost,
            _ => Outcome::Playing,
        }
    }
    /// One-sentence campaign result and rank, for headless reports and status
    /// readers.
    pub fn summary(&self) -> String {
        format!(
            "{}: {} of 8 islands freed, {} of {} shots hit, {} points.",
            self.rank(),
            self.wins,
            self.hits,
            self.shots,
            self.score
        )
    }
    pub(crate) fn rank(&self) -> &'static str {
        if self.wins == 8
            && self.lifeboats == 2
            && self.hits.saturating_mul(4) >= self.shots.saturating_mul(3)
        {
            "Storm Admiral"
        } else if self.wins == 8 {
            "Island Liberator"
        } else if self.wins >= 4 {
            "Fleet Captain"
        } else {
            "Free Sailor"
        }
    }
    pub(crate) fn battle_info(&self) -> &'static Battle {
        &BATTLES[self.battle]
    }
    pub(crate) fn power_limit(&self) -> u8 {
        if self.ships[0].hp[3] == 0 { 76 } else { 90 }
    }

    /// Execute one fixed 1/60-second tick. Invalid commands spend no resources.
    /// Terminal states are immutable until a new `Game` is constructed.
    pub fn step(&mut self, input: &Input) -> Cue {
        if self.outcome() != Outcome::Playing {
            return Cue::None;
        }
        self.events = Events::new();
        let input = &self.route_click(input);
        self.ticks = self.ticks.saturating_add(1);
        self.impact_ticks = self.impact_ticks.saturating_sub(1);
        match self.phase {
            Phase::Briefing if input.primary => {
                self.phase = Phase::Aim;
                self.log = "Your guns are ready.";
                self.events.push(Event::Engaged);
                Cue::Action
            }
            Phase::Aim => self.aim(input),
            Phase::Flight => self.flight(),
            Phase::EnemyReady => {
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    self.launch(false, self.enemy_setting);
                    Cue::Action
                } else {
                    Cue::None
                }
            }
            Phase::Dock => self.dock(input),
            Phase::Setback if input.tertiary && self.lifeboats > 0 => {
                self.lifeboats -= 1;
                self.ships[0] = self.checkpoint.ship;
                self.score = self.checkpoint.score;
                self.credits = self.checkpoint.credits;
                self.prepare_battle();
                self.phase = Phase::Aim;
                self.log = "Lifeboat used; the island approach is restored.";
                self.events.push(Event::Rescued);
                Cue::Action
            }
            Phase::Setback if input.primary || (input.tertiary && self.lifeboats == 0) => {
                self.phase = Phase::Sunk;
                self.log = "Voyage ended.";
                self.events.push(Event::Retired);
                Cue::Bad
            }
            _ => Cue::None,
        }
    }
    /// Route a pointer click on a drawn control to the key action it mirrors,
    /// so mouse and touch players take exactly the keyboard's rule paths.
    fn route_click(&self, input: &Input) -> Input {
        let mut routed = *input;
        if !input.click {
            return routed;
        }
        let p = input.pointer;
        match self.phase {
            Phase::Briefing => routed.primary |= layout::START.contains(p),
            Phase::Aim => {
                if let Some(i) = layout::AMMO.iter().position(|r| r.contains(p)) {
                    routed.slot = Some(i + 1);
                }
                routed.primary |= layout::FIRE.contains(p);
                routed.secondary |= layout::TABLE.contains(p);
                routed.left_pressed |= layout::ANGLE_DOWN.contains(p);
                routed.right_pressed |= layout::ANGLE_UP.contains(p);
                routed.down_pressed |= layout::POWER_DOWN.contains(p);
                routed.up_pressed |= layout::POWER_UP.contains(p);
            }
            Phase::Dock => {
                if let Some(i) = layout::SHOP.iter().position(|r| r.contains(p)) {
                    routed.slot = Some(i + 1);
                }
                routed.primary |= layout::SAIL.contains(p);
            }
            Phase::Setback => {
                routed.tertiary |= self.lifeboats > 0 && layout::RETRY.contains(p);
                routed.primary |= layout::RETIRE.contains(p);
            }
            _ => {}
        }
        routed
    }
    /// Read-only forecast of the current aim's contact when fired with
    /// `ammo`: the struck side and section, and every section's integrity
    /// before and after. It uses the live damage rules on a copy.
    pub(crate) fn preview(&self, ammo: Ammo) -> Option<Preview> {
        let (side, part) = self.forecast.contact?;
        let mut ship = self.ships[side];
        apply_damage(
            &mut ship,
            part,
            ammo,
            self.upgrades[0],
            false,
            self.battle,
            self.forecast.end,
        );
        Some(Preview {
            side,
            part,
            before: self.ships[side].hp,
            after: ship.hp,
        })
    }
    fn aim(&mut self, input: &Input) -> Cue {
        let before = self.setting;
        let mut cue = Cue::None;
        if let Some(ammo) = input.slot.and_then(Ammo::from_slot)
            && ammo != self.setting.ammo
        {
            self.setting.ammo = ammo;
            self.events.push(Event::Loaded(ammo));
            cue = Cue::Action;
        }
        let slow = self.ticks.is_multiple_of(3);
        let left = input.left_pressed || (input.left && slow);
        let right = input.right_pressed || (input.right && slow);
        let up = input.up_pressed || (input.up && slow);
        let down = input.down_pressed || (input.down && slow);
        if left != right {
            self.setting.angle = if right {
                self.setting.angle.saturating_add(1).min(70)
            } else {
                self.setting.angle.saturating_sub(1).max(15)
            };
        }
        if up != down {
            self.setting.power = if up {
                self.setting.power.saturating_add(1).min(self.power_limit())
            } else {
                self.setting.power.saturating_sub(1).max(25)
            };
        }
        if self.setting != before {
            self.refresh_forecast();
        }
        if input.secondary {
            self.show_table = !self.show_table;
            self.events.push(Event::Table(self.show_table));
            cue = Cue::Action;
        }
        if input.tertiary {
            self.phase = Phase::Setback;
            self.log = "Retreated to the lifeboats.";
            self.events.push(Event::Setback(Cause::Retreat));
            return Cue::Bad;
        }
        if input.primary && self.ammo[self.setting.ammo.index()] > 0 {
            self.ammo[self.setting.ammo.index()] -= 1;
            self.shots = self.shots.saturating_add(1);
            self.last_setting = Some(self.setting);
            self.launch(true, self.setting);
            return Cue::Action;
        }
        if input.primary {
            // Refused: nothing is spent; presentation shows the empty rack.
            self.events.push(Event::Empty(self.setting.ammo));
        }
        cue
    }
    fn launch(&mut self, player: bool, setting: Setting) {
        self.ball = Some(Ball::launch(
            self.ships[usize::from(!player)],
            player,
            setting,
        ));
        self.trace = Trace::new();
        if let Some(ball) = self.ball {
            self.trace.push(ball.pos);
            self.events.push(Event::Fired {
                player,
                at: ball.pos,
            });
        }
        self.phase = Phase::Flight;
        self.log = if player {
            "Shot away."
        } else {
            "Enemy cannon fired."
        };
    }
    fn flight(&mut self) -> Cue {
        let Some(mut ball) = self.ball else {
            return Cue::None;
        };
        let previous = ball.advance(self.wind);
        if ball.age.is_multiple_of(3) {
            self.trace.push(ball.pos);
        }
        if let Some((ship, part, fraction)) = contact(previous, ball.pos, self.ships, ball.age) {
            ball.pos = previous.lerp(ball.pos, fraction);
            self.trace.push(ball.pos);
            let before = self.ships[ship];
            let damage = apply_damage(
                &mut self.ships[ship],
                part,
                ball.ammo,
                if ball.from_player {
                    self.upgrades[0]
                } else {
                    0
                },
                !ball.from_player,
                self.battle,
                ball.pos,
            );
            self.impact = Some(ball.pos);
            self.impact_ticks = 50;
            self.last_damage = damage;
            if ball.from_player && ship == 1 && damage > 0 {
                self.hits = self.hits.saturating_add(1);
            }
            let armor = |s: Ship| if part < 3 { s.armor[part] } else { 0 };
            self.events.push(Event::Hit {
                side: ship,
                part,
                at: ball.pos,
                before: before.hp,
                after: self.ships[ship].hp,
                armor: [armor(before), armor(self.ships[ship])],
            });
            self.log = match (ship, part == 3) {
                (1, true) => "Enemy rigging hit.",
                (1, false) => "Enemy hull hit.",
                (_, true) => "Your mast is hit.",
                _ => "Your hull is hit.",
            };
            let cue = if ship == 1 { Cue::Good } else { Cue::Bad };
            return self.end_flight(ball, cue);
        }
        if ball.ended() {
            self.impact = Some(Vec2::new(ball.pos.x, 0.));
            self.impact_ticks = 35;
            self.last_damage = 0;
            self.log = "Splash: a miss.";
            self.events.push(Event::Splash {
                at: Vec2::new(ball.pos.x, 0.),
            });
            return self.end_flight(ball, Cue::Action);
        }
        self.ball = Some(ball);
        Cue::None
    }
    fn end_flight(&mut self, ball: Ball, cue: Cue) -> Cue {
        self.ball = None;
        if ball.from_player {
            self.history = self.trace;
        }
        if self.ships[0].hull() == 0 {
            self.phase = Phase::Setback;
            self.log = "Your hull is lost.";
            self.events.push(Event::Setback(Cause::Sunk));
            return Cue::Bad;
        }
        if self.ships[1].hull() == 0 {
            self.wins += 1;
            self.score += 800
                + self.ships[0].hull() as u32 * 2
                + self.ammo.iter().map(|&n| u32::from(n) * 15).sum::<u32>();
            let salvage = 85 + self.battle as u16 * 8;
            self.credits += salvage;
            self.events.push(Event::Sank { salvage });
            self.phase = if self.battle == 7 {
                Phase::Liberated
            } else {
                Phase::Dock
            };
            self.log = "Harbor liberated.";
            return if self.phase == Phase::Liberated {
                Cue::Win
            } else {
                Cue::Good
            };
        }
        if ball.from_player {
            if self.ships[1].hp[3] == 0 && self.round % 2 == 1 {
                self.log = "Enemy rigging fouled; their volley is skipped.";
                self.events.push(Event::Fouled);
                self.next_round();
            } else {
                self.salvo_left = if self.battle_info().tactic == Tactic::Broadside
                    && self.round.is_multiple_of(3)
                {
                    2
                } else {
                    1
                };
                self.queue_enemy();
            }
        } else if self.salvo_left > 0 {
            self.queue_enemy();
        } else {
            self.next_round();
        }
        cue
    }
    fn queue_enemy(&mut self) {
        if self.enemy_ammo.iter().all(|&n| n == 0) {
            self.next_round();
            return;
        }
        let tactic = self.battle_info().tactic;
        let desired = match tactic {
            Tactic::Rigger if self.ships[0].hp[3] > 0 => Ammo::Chain,
            Tactic::Bomber | Tactic::Admiral => Ammo::Shell,
            _ => Ammo::Round,
        };
        let ammo = if self.enemy_ammo[desired.index()] > 0 {
            desired
        } else if self.enemy_ammo[0] > 0 {
            Ammo::Round
        } else if self.enemy_ammo[2] > 0 {
            Ammo::Shell
        } else {
            Ammo::Chain
        };
        let preferred = if ammo == Ammo::Chain && self.ships[0].hp[3] > 0 {
            Some(3)
        } else if tactic == Tactic::Admiral {
            (0..3)
                .filter(|&p| self.ships[0].hp[p] > 0)
                .min_by_key(|&p| self.ships[0].hp[p])
        } else {
            None
        };
        let high = matches!(tactic, Tactic::Lobber | Tactic::Ironwall);
        let mut setting = solve(self.ships, false, self.wind, ammo, 90, preferred, high);
        // These are actual aiming errors, never damage rolls or manufactured hits.
        let pattern = [-1_i16, 1, -1, 0];
        let error = match tactic {
            Tactic::Apprentice => 8,
            Tactic::Gunner => 3,
            Tactic::Lobber => 1,
            Tactic::Rigger => 2,
            _ => 2,
        };
        let torn = if self.ships[1].hp[3] == 0 { 6 } else { 0 };
        let offset = pattern[(usize::from(self.round) - 1) % pattern.len()] * (error + torn);
        setting.power = (i16::from(setting.power) + offset).clamp(25, 90) as u8;
        self.enemy_setting = setting;
        self.enemy_ammo[ammo.index()] -= 1;
        self.salvo_left = self.salvo_left.saturating_sub(1);
        self.phase = Phase::EnemyReady;
        self.timer = 65;
        self.events.push(Event::EnemyAims);
    }
    fn next_round(&mut self) {
        let calm = self.wind;
        self.round = self.round.saturating_add(1);
        self.wind = if self.battle_info().tactic == Tactic::Bomber {
            self.battle_info().wind
                * if self.round.is_multiple_of(2) {
                    -1.
                } else {
                    1.
                }
        } else {
            self.battle_info().wind
        };
        if self.wind != calm {
            self.events.push(Event::Gust(self.wind));
        }
        self.setting.power = self.setting.power.min(self.power_limit());
        if self.ammo.iter().all(|&n| n == 0) {
            self.phase = Phase::Setback;
            self.log = "The magazines are empty.";
            self.events.push(Event::Setback(Cause::Empty));
        } else {
            self.phase = Phase::Aim;
            self.refresh_aim();
        }
    }
    pub(crate) fn cost(&self, slot: usize) -> Option<u16> {
        match slot {
            1 if self.ships[0].hp != self.ships[0].max => Some(30),
            2 if self.upgrades[0] < 2 => Some(65 + u16::from(self.upgrades[0]) * 30),
            3 if self.upgrades[1] < 2 => Some(55 + u16::from(self.upgrades[1]) * 25),
            4 if self.upgrades[2] < 2 => Some(45 + u16::from(self.upgrades[2]) * 20),
            _ => None,
        }
    }
    fn dock(&mut self, input: &Input) -> Cue {
        if let Some(slot) = input.slot
            && let Some(cost) = self.cost(slot)
            && self.credits >= cost
        {
            self.credits -= cost;
            match slot {
                1 => {
                    self.ships[0].repair(80);
                    self.log = "Shipwrights mended the hull and mast.";
                }
                2 => {
                    self.upgrades[0] += 1;
                    self.log = "Long guns installed.";
                }
                3 => {
                    self.upgrades[1] += 1;
                    self.log = "Hull plating installed.";
                }
                4 => {
                    self.upgrades[2] += 1;
                    self.log = "Magazine expanded.";
                }
                _ => unreachable!(),
            }
            self.events.push(Event::Bought { slot, cost });
            return Cue::Good;
        }
        if let Some(slot) = input.slot.filter(|slot| (1..=4).contains(slot)) {
            // Refused: full, capped or unaffordable purchases spend nothing.
            let short = self
                .cost(slot)
                .map_or(0, |cost| cost.saturating_sub(self.credits));
            self.events.push(Event::Refused { slot, short });
        }
        if input.primary {
            self.events.push(Event::Sailed);
            self.battle += 1;
            self.ships[0].armor = [u16::from(self.upgrades[1]) * 5; 3];
            self.checkpoint = Checkpoint {
                ship: self.ships[0],
                score: self.score,
                credits: self.credits,
            };
            self.prepare_battle();
            self.phase = Phase::Briefing;
            return Cue::Action;
        }
        Cue::None
    }
    fn prepare_battle(&mut self) {
        self.ships[1] = Ship::enemy(self.battle);
        self.ammo = [8, 2, 2 + self.upgrades[2]];
        self.enemy_ammo = [12, 3, 3];
        self.wind = self.battle_info().wind;
        self.round = 1;
        self.ball = None;
        self.history = Trace::new();
        self.trace = Trace::new();
        self.last_setting = None;
        self.impact = None;
        self.impact_ticks = 0;
        self.timer = 0;
        self.salvo_left = 0;
        self.setting.power = self.setting.power.min(self.power_limit());
        self.log = self.battle_info().advice;
        self.refresh_aim();
    }
    fn refresh_forecast(&mut self) {
        self.forecast = forecast(self.ships, true, self.wind, self.setting);
    }
    fn refresh_aim(&mut self) {
        let ammo =
            if self.battle >= 2 && self.round == 1 && self.ships[1].hp[3] > 0 && self.ammo[1] > 0 {
                Ammo::Chain
            } else if self.ammo[2] > 0 {
                Ammo::Shell
            } else if self.ammo[0] > 0 {
                Ammo::Round
            } else {
                Ammo::Chain
            };
        self.table = solve(
            self.ships,
            true,
            self.wind,
            ammo,
            self.power_limit(),
            if ammo == Ammo::Chain && self.ships[1].hp[3] > 0 {
                Some(3)
            } else {
                None
            },
            false,
        );
        self.refresh_forecast();
    }
    /// Produce legal ordinary controls. This read-only navigator adjusts aim and
    /// power one degree/unit at a time, then fires; it never writes game state.
    pub fn pilot(&self, _tick: u64) -> Input {
        let mut input = Input::default();
        match self.phase {
            Phase::Briefing => input.primary = true,
            Phase::Aim => {
                if self.setting.ammo != self.table.ammo {
                    input.slot = Some(self.table.ammo.index() + 1);
                } else if self.setting.angle != self.table.angle {
                    input.left_pressed = self.setting.angle > self.table.angle;
                    input.right_pressed = !input.left_pressed;
                } else if self.setting.power != self.table.power {
                    input.down_pressed = self.setting.power > self.table.power;
                    input.up_pressed = !input.down_pressed;
                } else {
                    input.primary = true;
                }
            }
            Phase::Dock => {
                let repair = self.hull() < 155 || self.ships[0].hp[3] < 25;
                let options = if repair { [1, 2, 3, 4] } else { [2, 3, 4, 1] };
                input.slot = options
                    .into_iter()
                    .find(|&slot| self.cost(slot).is_some_and(|cost| cost <= self.credits));
                input.primary = input.slot.is_none();
            }
            Phase::Setback => {
                input.tertiary = self.lifeboats > 0;
                input.primary = self.lifeboats == 0;
            }
            _ => {}
        }
        input
    }
}

// Swept first-party box queries prevent fast projectiles tunnelling through a mast.
// Every solid section on BOTH ships is queried; a returning shot can hit its owner.
fn contact(from: Vec2, to: Vec2, ships: [Ship; 2], age: u16) -> Option<(usize, usize, f32)> {
    if age == 0 {
        return None;
    }
    let sweep = Sweep::new(Vec3::new(from.x, from.y, 0.), Vec3::new(to.x, to.y, 0.), 2.).ok()?;
    let mut nearest: Option<(usize, usize, f32)> = None;
    for (side, ship) in ships.iter().enumerate() {
        if from.x.min(to.x) > ship.x + 49.
            || from.x.max(to.x) < ship.x - 49.
            || from.y.min(to.y) > 113.
        {
            continue;
        }
        for part in 0..4 {
            if ship.hp[part] == 0 {
                continue;
            }
            let blockers = [ship.blocker(part)];
            if let Ok(hit) = sweep.probe(&blockers[..], 0.)
                && let Some(fraction) = hit.hit_fraction
                && nearest.is_none_or(|(_, _, t)| fraction < t)
            {
                nearest = Some((side, part, fraction));
            }
        }
    }
    nearest
}
fn forecast(ships: [Ship; 2], player: bool, wind: f32, setting: Setting) -> Forecast {
    let mut ball = Ball::launch(ships[usize::from(!player)], player, setting);
    let mut trace = Trace::new();
    trace.push(ball.pos);
    let mut hit = None;
    for _ in 0..FLIGHT_LIMIT {
        let from = ball.advance(wind);
        if ball.age.is_multiple_of(3) {
            trace.push(ball.pos);
        }
        if let Some((side, part, fraction)) = contact(from, ball.pos, ships, ball.age) {
            ball.pos = from.lerp(ball.pos, fraction);
            hit = Some((side, part));
            break;
        }
        if ball.ended() {
            break;
        }
    }
    trace.push(ball.pos);
    Forecast {
        trace,
        contact: hit,
        end: ball.pos,
    }
}
fn solve(
    ships: [Ship; 2],
    player: bool,
    wind: f32,
    ammo: Ammo,
    max_power: u8,
    preferred: Option<usize>,
    high: bool,
) -> Setting {
    let source = ships[usize::from(!player)];
    let target = ships[usize::from(player)];
    let mut best = Setting {
        angle: 40,
        power: 50,
        ammo,
    };
    let mut best_value = f32::NEG_INFINITY;
    // Discrete control-space search: the result is reachable using ordinary keys.
    // Angle stride 3 plus every integer powder charge keeps this inexpensive.
    let (low_angle, high_angle) = if high { (55, 70) } else { (25, 58) };
    for angle in (low_angle..=high_angle).step_by(3) {
        for power in 25..=max_power {
            let setting = Setting { angle, power, ammo };
            let predicted = forecast(ships, player, wind, setting);
            let value = if let Some((side, part)) = predicted.contact {
                if side != usize::from(player) {
                    -10000.
                } else {
                    let priority = if preferred == Some(part) {
                        700.
                    } else if part == 3 {
                        if ammo == Ammo::Chain { 400. } else { 60. }
                    } else {
                        300.
                    };
                    let splash = if ammo == Ammo::Shell && part < 3 {
                        (0..3)
                            .filter(|&p| {
                                target.hp[p] > 0
                                    && target.section_center(p).distance(predicted.end) <= 57.
                            })
                            .count() as f32
                            * 60.
                    } else {
                        0.
                    };
                    priority + splash + f32::from(target.hp[part].min(40))
                        - f32::from(angle) * 0.12
                        - f32::from(power) * 0.02
                }
            } else {
                -predicted.end.distance(Vec2::new(target.x, 24.))
                    - (source.x - target.x).abs() * 0.001
            };
            if value > best_value {
                best_value = value;
                best = setting;
            }
        }
    }
    best
}
fn apply_damage(
    ship: &mut Ship,
    part: usize,
    ammo: Ammo,
    gun: u8,
    enemy: bool,
    battle: usize,
    hit: Vec2,
) -> u16 {
    let before: u16 = ship.hp.iter().sum();
    let bonus = u16::from(gun) * 8;
    let base = if enemy { 19 + battle as u16 } else { 30 };
    let direct = match ammo {
        Ammo::Round => {
            if part == 3 {
                18 + bonus
            } else {
                base + bonus
            }
        }
        Ammo::Chain => {
            if part == 3 {
                52 + bonus
            } else {
                10 + bonus
            }
        }
        Ammo::Shell => {
            if part == 3 {
                24 + bonus
            } else {
                base + 12 + bonus
            }
        }
    };
    let armor = if part < 3 && ammo != Ammo::Shell {
        ship.armor[part]
    } else {
        0
    };
    ship.hp[part] = ship.hp[part].saturating_sub(direct.saturating_sub(armor));
    if part < 3 {
        ship.armor[part] = ship.armor[part].saturating_sub(3);
    }
    if ammo == Ammo::Shell {
        for other in 0..3 {
            if other != part && ship.section_center(other).distance(hit) <= 57. {
                ship.hp[other] = ship.hp[other].saturating_sub(14 + bonus / 2);
            }
        }
    }
    before - ship.hp.iter().sum::<u16>()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command(primary: bool, slot: Option<usize>) -> Input {
        Input {
            primary,
            slot,
            ..Input::default()
        }
    }
    fn play_until(game: &mut Game, phase: Phase, limit: u64) {
        for tick in 0..limit {
            if game.phase == phase {
                return;
            }
            game.step(&game.pilot(tick));
        }
        panic!(
            "never reached {phase:?}: {} {:?}",
            game.summary(),
            game.phase
        );
    }
    #[test]
    fn gravity_and_wind_change_the_entire_ballistic_flight() {
        let setting = Setting {
            angle: 40,
            power: 43,
            ammo: Ammo::Round,
        };
        let mut calm = Ball::launch(Ship::player(), true, setting);
        let mut east = calm;
        let start_vy = calm.velocity.y;
        for _ in 0..90 {
            calm.advance(0.);
            east.advance(12.);
        }
        assert!(east.pos.x > calm.pos.x + 10.);
        assert_eq!(east.pos.y, calm.pos.y);
        assert!(calm.velocity.y < start_vy);
        assert!(calm.velocity.y < 0.);
    }
    #[test]
    fn swept_contacts_are_geometric_and_destroyed_sections_are_holes() {
        let mut ships = [Ship::player(), Ship::enemy(0)];
        // Fast traversal catches the thin mast even if neither endpoint overlaps.
        let hit = contact(Vec2::new(460., 75.), Vec2::new(500., 75.), ships, 1);
        assert_eq!(hit.map(|h| (h.0, h.1)), Some((1, 3)));
        assert!(contact(Vec2::new(460., 140.), Vec2::new(500., 140.), ships, 1).is_none());
        ships[1].hp[3] = 0;
        assert!(contact(Vec2::new(460., 75.), Vec2::new(500., 75.), ships, 1).is_none());
        // Friendly fire is geometry too: the source ship is not exempt.
        assert_eq!(
            contact(Vec2::new(120., 24.), Vec2::new(95., 24.), ships, 8).map(|h| h.0),
            Some(0)
        );
    }
    #[test]
    fn ammunition_changes_damage_armor_and_splash() {
        let mut armored = Ship::enemy(3);
        let hit = armored.section_center(1);
        let before = armored.hp;
        apply_damage(&mut armored, 1, Ammo::Round, 0, false, 3, hit);
        assert_eq!(armored.hp[1], before[1].saturating_sub(20));
        assert_eq!(armored.armor[1], 7);
        let mut shell = Ship::enemy(3);
        apply_damage(&mut shell, 1, Ammo::Shell, 0, false, 3, hit);
        assert_eq!(shell.hp[1], before[1].saturating_sub(42));
        assert!(shell.hp[0] < before[0] && shell.hp[2] < before[2]);
        let mut chain = Ship::enemy(7);
        let mast = chain.section_center(3);
        apply_damage(&mut chain, 3, Ammo::Chain, 0, false, 7, mast);
        assert_eq!(chain.hp[3], 0);
        assert_eq!(chain.hull(), Ship::enemy(7).hull());
    }
    #[test]
    fn forecast_and_live_shot_share_the_exact_collision_result() {
        let mut game = Game::new();
        game.step(&command(true, None));
        // Reach the predicted settings entirely through ordinary controls.
        while game.phase == Phase::Aim {
            game.step(&game.pilot(0));
        }
        let predicted = game.forecast;
        let before = game.ships[1].hp;
        for _ in 0..FLIGHT_LIMIT {
            if game.phase != Phase::Flight {
                break;
            }
            game.step(&Input::default());
        }
        assert_eq!(predicted.contact.map(|v| v.0), Some(1));
        assert_eq!(game.impact, Some(predicted.end));
        assert!(
            game.ships[1]
                .hp
                .iter()
                .zip(before)
                .any(|(&after, old)| after < old)
        );
        assert_eq!(game.hits, 1);
    }
    #[test]
    fn fire_edges_spend_once_and_invalid_inputs_cannot_farm_resources() {
        let mut game = Game::new();
        game.step(&command(true, None));
        let ammo = game.ammo;
        let score = game.score;
        let credits = game.credits;
        game.step(&Input {
            slot: Some(usize::MAX),
            pointer: Vec2::new(f32::NAN, f32::INFINITY),
            click: true,
            ..Input::default()
        });
        assert_eq!(game.ammo, ammo);
        assert_eq!(game.setting.ammo, Ammo::Round);
        game.step(&command(true, None));
        assert_eq!(game.ammo[0], ammo[0] - 1);
        for _ in 0..30 {
            game.step(&command(true, Some(3)));
        }
        assert_eq!(game.ammo, [ammo[0] - 1, ammo[1], ammo[2]]);
        assert_eq!((game.score, game.credits), (score, credits));
    }
    #[test]
    fn dock_upgrade_and_repair_purchases_are_paid_and_bounded() {
        let mut game = Game::new();
        play_until(&mut game, Phase::Dock, 10_000);
        let funds = game.credits;
        let cost = game.cost(2).unwrap();
        game.step(&command(false, Some(2)));
        assert_eq!(game.upgrades[0], 1);
        assert_eq!(game.credits, funds - cost);
        let funded = game.credits;
        assert!(funded < game.cost(2).unwrap());
        assert_eq!(game.step(&command(false, Some(2))), Cue::None);
        assert_eq!(game.credits, funded);
        let before = game.hull();
        if game.cost(1).is_some() && game.credits >= 30 {
            game.step(&command(false, Some(1)));
            assert_eq!(game.hull(), (before + 80).min(195));
            assert_eq!(game.credits, funded - 30);
        }
        let paid = game.upgrades;
        game.step(&command(true, None));
        assert_eq!(game.battle_number(), 2);
        assert_eq!(game.upgrades, paid);
        assert_eq!(game.ammo, [8, 2, 2 + paid[2]]);
    }
    #[test]
    fn retreat_retry_consumes_lifeboats_without_salvage_or_score_farming() {
        let mut game = Game::new();
        game.step(&command(true, None));
        let initial = game.checkpoint;
        for left in [1, 0] {
            game.step(&Input {
                tertiary: true,
                ..Input::default()
            });
            assert_eq!(game.phase, Phase::Setback);
            game.step(&Input {
                tertiary: true,
                ..Input::default()
            });
            assert_eq!(game.phase, Phase::Aim);
            assert_eq!(game.lifeboats, left);
            assert_eq!(game.ships[0], initial.ship);
            assert_eq!((game.score, game.credits), (initial.score, initial.credits));
        }
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(game.outcome(), Outcome::Lost);
        let frozen = game.clone();
        for _ in 0..10 {
            assert_eq!(game.step(&command(true, Some(1))), Cue::None);
        }
        assert_eq!(game, frozen);
    }
    #[test]
    fn empty_magazines_and_sunk_hull_are_real_setbacks() {
        let mut game = Game::new();
        game.ammo = [0; 3];
        game.next_round();
        assert_eq!(game.phase, Phase::Setback);
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(game.ammo, [8, 2, 2]);
        game.ships[0].hp[..3].fill(0);
        let ball = Ball::launch(game.ships[1], false, game.setting);
        game.end_flight(ball, Cue::Bad);
        assert_eq!(game.phase, Phase::Setback);
        assert_eq!(game.hull(), 0);
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(game.hull(), 195);
    }
    #[test]
    fn disabled_mast_disrupts_enemy_volley_and_limits_player_power() {
        let mut game = Game::new();
        game.ships[1].hp[3] = 0;
        let ball = Ball::launch(game.ships[0], true, game.setting);
        game.end_flight(ball, Cue::Good);
        assert_eq!(game.phase, Phase::Aim);
        assert_eq!(game.round, 2);
        assert!(game.events.iter().any(|e| e == Event::Fouled));
        game.ships[0].hp[3] = 0;
        game.setting.power = 90;
        game.next_round();
        assert_eq!(game.setting.power, 76);
        assert_eq!(game.power_limit(), 76);
    }
    #[test]
    fn tutorial_setting_hits_a_real_enemy_section() {
        let game = Game::new();
        assert_eq!(game.forecast.contact.map(|v| v.0), Some(1));
    }
    #[test]
    fn authored_gusts_reverse_and_broadside_launches_two_physical_balls() {
        let mut game = Game::new();
        game.battle = 5;
        game.prepare_battle();
        assert_eq!(game.wind, 13.);
        game.next_round();
        assert_eq!(game.wind, -13.);
        assert!(game.events.iter().any(|e| e == Event::Gust(-13.)));
        game.battle = 6;
        game.prepare_battle();
        game.round = 3;
        let ball = Ball::launch(game.ships[0], true, game.setting);
        game.end_flight(ball, Cue::None);
        assert_eq!(game.phase, Phase::EnemyReady);
        assert_eq!(game.salvo_left, 1);
        assert_eq!(game.enemy_ammo[0], 11);
        let mut launches = 0;
        let mut last_phase = game.phase;
        for _ in 0..1000 {
            game.step(&Input::default());
            if last_phase == Phase::EnemyReady && game.phase == Phase::Flight {
                launches += 1;
                assert!(game.ball.is_some_and(|b| !b.from_player));
            }
            if game.phase == Phase::Aim {
                break;
            }
            last_phase = game.phase;
        }
        assert_eq!(launches, 2);
        assert_eq!(game.enemy_ammo[0], 10);
        assert_eq!(game.round, 4);
    }
    #[test]
    fn full_eight_battle_campaign_is_won_only_with_ordinary_inputs() {
        let mut game = Game::new();
        let mut visited = [false; 8];
        let mut took_damage = false;
        let mut used_ammo = [false; 3];
        for tick in 0..100_000 {
            visited[game.battle] = true;
            took_damage |= game.hull() < 195;
            if let Some(ball) = game.ball
                && ball.from_player
            {
                used_ammo[ball.ammo.index()] = true;
            }
            let input = game.pilot(tick);
            game.step(&input);
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(
            game.outcome(),
            Outcome::Won,
            "{} phase {:?}, hull {}, enemy {:?}, ammo {:?}",
            game.summary(),
            game.phase,
            game.hull(),
            game.ships[1].hp,
            game.ammo
        );
        assert!(visited.into_iter().all(|v| v));
        assert_eq!(game.wins, 8);
        assert!(took_damage, "enemy shots must cause real combat damage");
        assert!(
            used_ammo.into_iter().all(|used| used),
            "campaign should exercise every ammunition type"
        );
        assert!(game.score > 8_000 && game.shots >= 16 && game.hits > 0);
        let frozen = game.clone();
        for _ in 0..10 {
            assert_eq!(game.step(&command(true, Some(1))), Cue::None);
        }
        assert_eq!(game, frozen);
    }
    fn click(at: ferravex_math::Rect) -> Input {
        Input {
            click: true,
            pointer: at.center(),
            ..Input::default()
        }
    }
    fn key(press: fn(&mut Input)) -> Input {
        let mut input = Input::default();
        press(&mut input);
        input
    }
    #[test]
    fn clicks_take_exactly_the_rule_paths_of_their_keys() {
        // Aiming: every gun deck control mirrors its key.
        let (mut keys, mut mouse) = (Game::new(), Game::new());
        for (input, rect) in [
            (key(|i| i.primary = true), layout::START),
            (key(|i| i.slot = Some(3)), layout::AMMO[2]),
            (key(|i| i.right_pressed = true), layout::ANGLE_UP),
            (key(|i| i.left_pressed = true), layout::ANGLE_DOWN),
            (key(|i| i.up_pressed = true), layout::POWER_UP),
            (key(|i| i.down_pressed = true), layout::POWER_DOWN),
            (key(|i| i.secondary = true), layout::TABLE),
            (key(|i| i.primary = true), layout::FIRE),
        ] {
            assert_eq!(keys.step(&input), mouse.step(&click(rect)));
            assert_eq!(keys, mouse);
        }
        assert_eq!(mouse.phase, Phase::Flight);
        assert_eq!((mouse.ammo[2], mouse.show_table), (1, true));
        // Controls are inert outside their phase, and so is open water.
        let (mut idle, mut clicked) = (mouse.clone(), mouse.clone());
        idle.step(&Input::default());
        clicked.step(&click(layout::AMMO[0]));
        assert_eq!(idle, clicked);
        clicked.step(&click(layout::SCENE));
        idle.step(&Input::default());
        assert_eq!(idle, clicked);
        // The dock's cards and sail button.
        let mut docked = Game::new();
        play_until(&mut docked, Phase::Dock, 10_000);
        let (mut keys, mut mouse) = (docked.clone(), docked);
        for (input, rect) in [
            (key(|i| i.slot = Some(2)), layout::SHOP[1]),
            (key(|i| i.slot = Some(1)), layout::SHOP[0]),
            (key(|i| i.slot = Some(4)), layout::SHOP[3]),
            (key(|i| i.primary = true), layout::SAIL),
        ] {
            assert_eq!(keys.step(&input), mouse.step(&click(rect)));
            assert_eq!(keys, mouse);
        }
        assert_eq!((mouse.phase, mouse.upgrades[0]), (Phase::Briefing, 1));
        // Setbacks: retry spends a lifeboat, retire ends the voyage.
        let retreat = key(|i| i.tertiary = true);
        for game in [&mut keys, &mut mouse] {
            game.step(&command(true, None));
            game.step(&retreat);
        }
        assert_eq!(keys.step(&retreat), mouse.step(&click(layout::RETRY)));
        assert_eq!(keys, mouse);
        assert_eq!((mouse.phase, mouse.lifeboats), (Phase::Aim, 1));
        for game in [&mut keys, &mut mouse] {
            game.step(&retreat);
        }
        assert_eq!(
            keys.step(&command(true, None)),
            mouse.step(&click(layout::RETIRE))
        );
        assert_eq!(keys, mouse);
        assert_eq!(mouse.outcome(), Outcome::Lost);
        // Without lifeboats the retry button is inert rather than a retire.
        let mut stranded = Game::new();
        stranded.lifeboats = 0;
        stranded.step(&command(true, None));
        stranded.step(&retreat);
        stranded.step(&click(layout::RETRY));
        assert_eq!(stranded.phase, Phase::Setback);
    }
    #[test]
    fn events_and_damage_preview_match_the_live_shot() {
        let mut game = Game::new();
        game.step(&command(true, None));
        assert_eq!(game.events.iter().collect::<Vec<_>>(), [Event::Engaged]);
        let mut preview = None;
        while game.phase == Phase::Aim {
            preview = game.preview(game.setting.ammo);
            game.step(&game.pilot(0));
        }
        let muzzle = game.events.iter().find_map(|e| match e {
            Event::Fired { player: true, at } => Some(at),
            _ => None,
        });
        assert_eq!(muzzle, Some(Vec2::new(PLAYER_X + 50., 40.)));
        let preview = preview.expect("the navigator's aim strikes a ship");
        let hit = loop {
            assert_eq!(game.phase, Phase::Flight);
            game.step(&Input::default());
            if let Some(hit) = game.events.iter().find(|e| matches!(e, Event::Hit { .. })) {
                break hit;
            }
        };
        let Event::Hit {
            side,
            part,
            at,
            before,
            after,
            ..
        } = hit
        else {
            unreachable!()
        };
        assert_eq!((side, part, before, after), {
            let p = preview;
            (p.side, p.part, p.before, p.after)
        });
        assert_eq!(Some(at), game.impact);
        assert!(after.iter().sum::<u16>() < before.iter().sum::<u16>());
        // Each executed tick starts from a clean list.
        game.step(&Input::default());
        assert!(!game.events.iter().any(|e| matches!(e, Event::Hit { .. })));
    }
    #[test]
    fn refusals_are_recorded_and_spend_nothing() {
        let mut game = Game::new();
        game.step(&command(true, None));
        game.ammo[0] = 0;
        let ammo = game.ammo;
        game.step(&command(true, None));
        assert_eq!(
            game.events.iter().collect::<Vec<_>>(),
            [Event::Empty(Ammo::Round)]
        );
        assert_eq!((game.phase, game.ammo, game.shots), (Phase::Aim, ammo, 0));
        let mut game = Game::new();
        play_until(&mut game, Phase::Dock, 10_000);
        assert!(game.events.iter().any(|e| e == Event::Sank { salvage: 85 }));
        game.credits = 10;
        let upgrades = game.upgrades;
        game.step(&command(false, Some(2)));
        assert_eq!(
            game.events.iter().collect::<Vec<_>>(),
            [Event::Refused { slot: 2, short: 55 }]
        );
        game.upgrades[1] = 2;
        game.step(&command(false, Some(3)));
        assert_eq!(
            game.events.iter().collect::<Vec<_>>(),
            [Event::Refused { slot: 3, short: 0 }]
        );
        assert_eq!((game.credits, game.upgrades[0]), (10, upgrades[0]));
        // Keys that are not purchases are not refusals either.
        game.step(&command(false, Some(5)));
        assert_eq!(game.events.iter().count(), 0);
    }
}
