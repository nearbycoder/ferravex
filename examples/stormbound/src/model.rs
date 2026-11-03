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
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Round => "Round",
            Self::Chain => "Chain",
            Self::Shell => "Shell",
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
    fn repair(&mut self, mut amount: u16) {
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
    enemy_setting: Setting,
    salvo_left: u8,
    checkpoint: Checkpoint,
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
            log: "Relief sails wait for your signal.",
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
    /// Campaign result and rank, suitable for a headless report or host ending.
    pub fn summary(&self) -> String {
        format!(
            "{} / 8 islands freed. Rank: {}. {} points; {} hits from {} shots; {} lifeboats remain.",
            self.wins,
            self.rank(),
            self.score,
            self.hits,
            self.shots,
            self.lifeboats
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
        self.ticks = self.ticks.saturating_add(1);
        self.impact_ticks = self.impact_ticks.saturating_sub(1);
        match self.phase {
            Phase::Briefing if input.primary => {
                self.phase = Phase::Aim;
                self.log = "Arrows aim and set power; Space fires. X shows the firing table.";
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
                self.log = "Lifeboat used: the island approach is restored. The failed sortie still counts in your accuracy.";
                Cue::Action
            }
            Phase::Setback if input.primary || (input.tertiary && self.lifeboats == 0) => {
                self.phase = Phase::Sunk;
                self.log = "Your flag is lowered. The islands will remember the harbors you freed.";
                Cue::Bad
            }
            _ => Cue::None,
        }
    }
    fn aim(&mut self, input: &Input) -> Cue {
        let before = self.setting;
        let mut cue = Cue::None;
        if let Some(ammo) = input.slot.and_then(Ammo::from_slot)
            && ammo != self.setting.ammo
        {
            self.setting.ammo = ammo;
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
            cue = Cue::Action;
        }
        if input.tertiary {
            self.phase = Phase::Setback;
            self.log =
                "Retreated to the lifeboats. Retry this island with C, or retire with Space.";
            return Cue::Bad;
        }
        if input.primary && self.ammo[self.setting.ammo.index()] > 0 {
            self.ammo[self.setting.ammo.index()] -= 1;
            self.shots = self.shots.saturating_add(1);
            self.last_setting = Some(self.setting);
            self.launch(true, self.setting);
            return Cue::Action;
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
        }
        self.phase = Phase::Flight;
        self.log = if player {
            "Shot away! Watch its flight through the wind."
        } else {
            "Enemy cannon fired!"
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
            self.log = if part == 3 && ship == 1 {
                "Rigging hit! A fallen enemy mast delays alternate volleys."
            } else if part == 3 {
                "Your mast is hit! If it falls, maximum cannon power drops to 76."
            } else if ship == 1 {
                "Direct hit! A hull section splinters."
            } else {
                "Incoming hit! Your hull takes damage."
            };
            let cue = if ship == 1 { Cue::Good } else { Cue::Bad };
            return self.end_flight(ball, cue);
        }
        if ball.ended() {
            self.impact = Some(Vec2::new(ball.pos.x, 0.));
            self.impact_ticks = 35;
            self.last_damage = 0;
            self.log = "Splash. No ship contact, no damage.";
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
            self.log = "Your hull is lost. C spends a lifeboat to retry; Space retires.";
            return Cue::Bad;
        }
        if self.ships[1].hull() == 0 {
            self.wins += 1;
            self.score += 800
                + self.ships[0].hull() as u32 * 2
                + self.ammo.iter().map(|&n| u32::from(n) * 15).sum::<u32>();
            self.credits += 85 + self.battle as u16 * 8;
            self.phase = if self.battle == 7 {
                Phase::Liberated
            } else {
                Phase::Dock
            };
            self.log = "Harbor liberated! The island offers salvage and a safe repair dock.";
            return if self.phase == Phase::Liberated {
                Cue::Win
            } else {
                Cue::Good
            };
        }
        if ball.from_player {
            if self.ships[1].hp[3] == 0 && self.round % 2 == 1 {
                self.log = "Enemy rigging fouls the gun crew. Their return volley is delayed.";
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
    }
    fn next_round(&mut self) {
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
        self.setting.power = self.setting.power.min(self.power_limit());
        if self.ammo.iter().all(|&n| n == 0) {
            self.phase = Phase::Setback;
            self.log = "The magazines are empty. C spends a lifeboat to retry; Space retires.";
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
                    self.log = "Shipwrights restored 80 hull and 25 mast integrity.";
                }
                2 => {
                    self.upgrades[0] += 1;
                    self.log = "Long guns installed: all ammunition deals 8 more direct damage.";
                }
                3 => {
                    self.upgrades[1] += 1;
                    self.log = "Hull plating installed: refreshed armor at each island.";
                }
                4 => {
                    self.upgrades[2] += 1;
                    self.log = "Magazine expanded: one extra shell at every island.";
                }
                _ => unreachable!(),
            }
            return Cue::Good;
        }
        if input.primary {
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
}
