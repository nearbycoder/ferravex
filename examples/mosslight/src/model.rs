//! Pure turn-based rules for the six-floor Mosslight expedition.
//!
//! One input edge commits at most one dungeon turn. Idle fixed ticks do not
//! move enemies or consume supplies. No renderer, clock or platform I/O is used.
//!
//! ```
//! use ferravex_arcade::{Input, Outcome};
//! use mosslight::Game;
//! let mut game = Game::new();
//! assert_eq!(game.floor(), 1);
//! game.step(&Input { right_pressed: true, ..Input::default() });
//! assert_eq!(game.turns(), 1);
//! assert_eq!(game.outcome(), Outcome::Playing);
//! ```

use crate::content::{CELLS, COLS, FLOORS, ROWS};
use ferravex_arcade::{Cue, Input, Outcome};

pub(crate) const ENEMIES: usize = 8;
const DIRS: [(i32, i32); 4] = [(0, -1), (-1, 0), (1, 0), (0, 1)];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Pos {
    pub x: i32,
    pub y: i32,
}
impl Pos {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
    pub fn index(self) -> usize {
        self.y as usize * COLS + self.x as usize
    }
    pub fn inside(self) -> bool {
        self.x >= 0 && self.y >= 0 && self.x < COLS as i32 && self.y < ROWS as i32
    }
    pub fn offset(self, dx: i32, dy: i32) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }
    pub fn distance(self, other: Self) -> i32 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tile {
    #[default]
    Floor,
    Wall,
    Seal,
    Seed,
    Stairs,
    Amber,
    Flask,
    Well,
    EmptyWell,
    Thorns,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Species {
    Sprout,
    Knight,
    Spore,
    King,
}
impl Species {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sprout => "Rootling",
            Self::Knight => "Bramble knight",
            Self::Spore => "Spore bulb",
            Self::King => "Root King",
        }
    }
    pub fn damage(self) -> u8 {
        match self {
            Self::Sprout => 3,
            Self::Knight => 5,
            Self::Spore => 4,
            Self::King => 7,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Plan {
    Rest,
    Move(Pos),
    Strike(Pos),
    Beam(i32, i32),
    Cross,
    Ring,
}
impl Plan {
    fn is_attack(self) -> bool {
        matches!(
            self,
            Self::Strike(_) | Self::Beam(_, _) | Self::Cross | Self::Ring
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Enemy {
    pub pos: Pos,
    pub species: Species,
    pub health: u8,
    pub max_health: u8,
    pub plan: Plan,
    pub rounds: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Explore,
    Camp,
    Won,
    Lost,
}

/// An expedition with bounded board, enemy and message storage.
///
/// Construct a fresh run with [`Self::new`], commit fixed inputs with
/// [`Self::step`], then inspect [`Self::outcome`]. Drawing is a separate adapter.
#[derive(Clone, Debug)]
pub struct Game {
    pub(crate) board: [Tile; CELLS],
    pub(crate) enemies: [Option<Enemy>; ENEMIES],
    pub(crate) hero: Pos,
    pub(crate) depth: usize,
    pub(crate) phase: Phase,
    pub(crate) health: u8,
    pub(crate) max_health: u8,
    pub(crate) attack: u8,
    pub(crate) flasks: u8,
    pub(crate) charge: u8,
    pub(crate) max_charge: u8,
    pub(crate) seed: bool,
    pub(crate) turns: u32,
    pub(crate) score: u32,
    pub(crate) kills: u16,
    pub(crate) treasure: u16,
    pub(crate) choices: [u8; 3],
    pub(crate) choice: usize,
    pub(crate) messages: [String; 3],
    pub(crate) ticks: u64,
    pub(crate) flash: u8,
    pub(crate) inspected: Option<Pos>,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Begin at the Lantern Stair with full health, two flasks and three bursts.
    /// See the [module example](self).
    pub fn new() -> Self {
        let mut game = Self {
            board: [Tile::Floor; CELLS],
            enemies: [None; ENEMIES],
            hero: Pos::default(),
            depth: 0,
            phase: Phase::Explore,
            health: 26,
            max_health: 26,
            attack: 3,
            flasks: 2,
            charge: 3,
            max_charge: 3,
            seed: false,
            turns: 0,
            score: 0,
            kills: 0,
            treasure: 0,
            choices: [0; 3],
            choice: 0,
            messages: std::array::from_fn(|_| String::new()),
            ticks: 0,
            flash: 0,
            inspected: None,
        };
        game.load_floor();
        game.log("Take your time. Only actions advance the dungeon.");
        game
    }

    /// One-based current floor, always in `1..=6`.
    pub fn floor(&self) -> usize {
        self.depth + 1
    }
    /// Number of accepted dungeon actions; invalid moves and idle ticks are free.
    pub fn turns(&self) -> u32 {
        self.turns
    }
    /// Current hit points. Zero is a terminal defeat.
    pub fn health(&self) -> u8 {
        self.health
    }
    /// Monotonic score from unique treasures, defeated enemies and cleared floors.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// The campaign result; camp choices still count as active play.
    pub fn outcome(&self) -> Outcome {
        match self.phase {
            Phase::Won => Outcome::Won,
            Phase::Lost => Outcome::Lost,
            _ => Outcome::Playing,
        }
    }
    pub(crate) fn log(&mut self, message: &str) {
        self.messages.rotate_left(1);
        self.messages[2] = message.to_owned();
    }
    fn load_floor(&mut self) {
        self.enemies.fill(None);
        self.seed = false;
        self.phase = Phase::Explore;
        self.inspected = None;
        let mut next_enemy = 0;
        for (y, row) in FLOORS[self.depth].map.iter().enumerate() {
            for (x, symbol) in row.bytes().enumerate() {
                let pos = Pos::new(x as i32, y as i32);
                let tile = match symbol {
                    b'#' => Tile::Wall,
                    b'D' => Tile::Seal,
                    b'K' => Tile::Seed,
                    b'>' => Tile::Stairs,
                    b'$' => Tile::Amber,
                    b'!' => Tile::Flask,
                    b'O' => Tile::Well,
                    b'^' => Tile::Thorns,
                    _ => Tile::Floor,
                };
                self.board[pos.index()] = tile;
                if symbol == b'@' {
                    self.hero = pos;
                }
                let species = match symbol {
                    b's' => Some(Species::Sprout),
                    b'g' => Some(Species::Knight),
                    b'f' => Some(Species::Spore),
                    b'B' => Some(Species::King),
                    _ => None,
                };
                if let Some(species) = species {
                    let health = match species {
                        Species::Sprout => 5 + self.depth as u8,
                        Species::Knight => 10 + self.depth as u8,
                        Species::Spore => 6 + self.depth as u8,
                        Species::King => 36,
                    };
                    assert!(next_enemy < ENEMIES, "authored enemy budget");
                    self.enemies[next_enemy] = Some(Enemy {
                        pos,
                        species,
                        health,
                        max_health: health,
                        plan: Plan::Rest,
                        rounds: 0,
                    });
                    next_enemy += 1;
                }
            }
        }
        self.plan_enemies();
        self.log(FLOORS[self.depth].name);
    }
    pub(crate) fn tile(&self, pos: Pos) -> Tile {
        if pos.inside() {
            self.board[pos.index()]
        } else {
            Tile::Wall
        }
    }
    fn walkable(&self, pos: Pos) -> bool {
        self.tile(pos) != Tile::Wall && (self.tile(pos) != Tile::Seal || self.seed)
    }
    pub(crate) fn enemy_at(&self, pos: Pos) -> Option<usize> {
        self.enemies
            .iter()
            .position(|enemy| enemy.is_some_and(|enemy| enemy.pos == pos))
    }
    pub(crate) fn king_alive(&self) -> bool {
        self.enemies
            .iter()
            .flatten()
            .any(|enemy| enemy.species == Species::King)
    }
    fn damage_enemy(&mut self, index: usize, amount: u8) -> bool {
        let Some(enemy) = self.enemies[index].as_mut() else {
            return false;
        };
        enemy.health = enemy.health.saturating_sub(amount);
        let name = enemy.species.name();
        let king = enemy.species == Species::King;
        if enemy.health == 0 {
            self.enemies[index] = None;
            self.score += if king { 700 } else { 80 };
            self.kills += 1;
            self.log(&format!("{name} falls. Its attack is cancelled."));
            if king {
                self.log("The crown goes dark. Take the seed to the stairs!");
            }
            true
        } else {
            self.log(&format!("You strike {name} for {amount}."));
            false
        }
    }
    fn pickup(&mut self) -> Cue {
        let cell = self.hero.index();
        let cue = match self.board[cell] {
            Tile::Seed => {
                self.seed = true;
                self.score += 100;
                self.log("Seed sigil found. Gold seals now open.");
                Cue::Good
            }
            Tile::Amber => {
                self.treasure += 1;
                self.score += 140;
                self.log("A jar of daylight. +140 amber score.");
                Cue::Good
            }
            Tile::Flask => {
                if self.flasks >= 5 {
                    self.log("Flask belt full. Leave this one for later.");
                    return Cue::Action;
                }
                self.flasks += 1;
                self.log("One healing flask found. X drinks it.");
                Cue::Good
            }
            Tile::Well => {
                self.health = self.health.saturating_add(8).min(self.max_health);
                self.charge = self.charge.saturating_add(1).min(self.max_charge);
                self.flasks = self.flasks.saturating_add(1).min(5);
                self.board[cell] = Tile::EmptyWell;
                self.log("Moonwell: +8 health, one flask, one lantern charge.");
                return Cue::Good;
            }
            Tile::Thorns => {
                self.health = self.health.saturating_sub(2);
                self.flash = 18;
                self.log("Thorns cut for 2. Stone paths are safer.");
                return Cue::Bad;
            }
            _ => return Cue::Action,
        };
        self.board[cell] = Tile::Floor;
        cue
    }
    fn move_or_strike(&mut self, dx: i32, dy: i32) -> Cue {
        let dest = self.hero.offset(dx, dy);
        if !self.walkable(dest) {
            self.log(if self.tile(dest) == Tile::Seal {
                "The gold seal needs this floor's seed sigil."
            } else {
                "Stone blocks the way. No turn spent."
            });
            return Cue::None;
        }
        let cue = if let Some(index) = self.enemy_at(dest) {
            if self.damage_enemy(index, self.attack) {
                Cue::Good
            } else {
                Cue::Action
            }
        } else {
            self.hero = dest;
            self.pickup()
        };
        self.finish_turn(false, cue)
    }
    fn descend(&mut self) -> Cue {
        if !self.seed {
            self.log("Find the leaf-shaped seed sigil before descending.");
            return Cue::None;
        }
        if self.king_alive() {
            self.log("The Root King's crown still binds the exit.");
            return Cue::None;
        }
        self.score += 350;
        if self.depth == FLOORS.len() - 1 {
            self.score += 2000 + u32::from(self.health) * 20;
            self.score += 600_u32.saturating_sub(self.turns);
            self.phase = Phase::Won;
            self.log("Dawn returns. The village remembers your lantern.");
            Cue::Win
        } else {
            self.phase = Phase::Camp;
            self.choice = 0;
            self.log("A safe root hollow. Choose one relic before descending.");
            Cue::Good
        }
    }
    fn camp(&mut self, input: &Input) -> Cue {
        if input.left_pressed || input.up_pressed {
            self.choice = (self.choice + 2) % 3;
        }
        if input.right_pressed || input.down_pressed {
            self.choice = (self.choice + 1) % 3;
        }
        let clicked = if input.click && (278. ..428.).contains(&input.pointer.y) {
            (0..3).find(|i| {
                let x = 44. + *i as f32 * 181.;
                (x..x + 171.).contains(&input.pointer.x)
            })
        } else {
            None
        };
        let choice = input
            .slot
            .filter(|slot| (1..=3).contains(slot))
            .map(|slot| slot - 1)
            .or(clicked)
            .or(if input.primary {
                Some(self.choice)
            } else {
                None
            });
        let Some(choice) = choice else {
            return Cue::None;
        };
        self.choices[choice] += 1;
        match choice {
            0 => self.attack += 1,
            1 => {
                self.max_health += 5;
                self.health = self.health.saturating_add(5);
            }
            _ => {
                self.max_charge += 1;
                self.charge = self.charge.saturating_add(2).min(self.max_charge);
            }
        }
        self.health = self.health.saturating_add(7).min(self.max_health);
        self.flasks = self.flasks.saturating_add(1).min(5);
        self.charge = self.charge.saturating_add(1).min(self.max_charge);
        self.depth += 1;
        self.load_floor();
        self.log("Camp restored 7 health, one flask and one charge.");
        Cue::Good
    }

    /// Execute one fixed tick. Accepted directional edges move or melee; Space
    /// guards/descends, X heals and C bursts. Idle ticks only animate presentation.
    /// Finished runs reject all further actions. See the [module example](self).
    pub fn step(&mut self, input: &Input) -> Cue {
        if self.outcome() != Outcome::Playing {
            return Cue::None;
        }
        self.ticks = self.ticks.wrapping_add(1);
        self.flash = self.flash.saturating_sub(1);
        if self.phase == Phase::Camp {
            return self.camp(input);
        }
        if input.secondary {
            if self.flasks == 0 || self.health == self.max_health {
                self.log("No flask needed or none remaining. No turn spent.");
                return Cue::None;
            }
            self.flasks -= 1;
            self.health = self.health.saturating_add(12).min(self.max_health);
            self.log("You drink a flask: up to 12 health restored.");
            return self.finish_turn(false, Cue::Good);
        }
        if input.tertiary {
            let targets: [bool; ENEMIES] = std::array::from_fn(|i| {
                self.enemies[i].is_some_and(|e| self.hero.distance(e.pos) <= 2)
            });
            if self.charge == 0 || !targets.iter().any(|t| *t) {
                self.log("Burst needs charge and a foe within two tiles.");
                return Cue::None;
            }
            self.charge -= 1;
            self.log("Lantern burst! Every nearby root burns.");
            for (index, hit) in targets.iter().enumerate() {
                if *hit {
                    self.damage_enemy(index, self.attack + 3);
                }
            }
            self.flash = 12;
            return self.finish_turn(false, Cue::Good);
        }
        if input.primary {
            if self.tile(self.hero) == Tile::Stairs {
                return self.descend();
            }
            self.log("You brace the lantern. Incoming damage reduced by 3.");
            return self.finish_turn(true, Cue::Action);
        }
        let direction = if input.up_pressed {
            Some((0, -1))
        } else if input.left_pressed {
            Some((-1, 0))
        } else if input.right_pressed {
            Some((1, 0))
        } else if input.down_pressed {
            Some((0, 1))
        } else {
            None
        };
        if let Some((dx, dy)) = direction {
            return self.move_or_strike(dx, dy);
        }
        if input.click {
            let x = ((input.pointer.x - 26.) / 38.).floor() as i32;
            let y = ((input.pointer.y - 128.) / 38.).floor() as i32;
            let cell = Pos::new(x, y);
            if cell.inside() {
                self.inspected = Some(cell);
                if self.hero.distance(cell) == 1 {
                    return self.move_or_strike(cell.x - self.hero.x, cell.y - self.hero.y);
                }
            }
        }
        Cue::None
    }

    fn finish_turn(&mut self, guarding: bool, cue: Cue) -> Cue {
        self.turns = self.turns.saturating_add(1);
        if self.health == 0 {
            self.phase = Phase::Lost;
            self.log("Your lantern goes out among the roots.");
            return Cue::Bad;
        }
        let mut damage = 0;
        for index in 0..ENEMIES {
            let Some(enemy) = self.enemies[index] else {
                continue;
            };
            if self.threatens(enemy, self.hero) {
                let hit = enemy
                    .species
                    .damage()
                    .saturating_sub(if guarding { 3 } else { 0 });
                damage += hit;
            }
            if let Plan::Move(dest) = enemy.plan
                && dest != self.hero
                && self.walkable(dest)
                && self.enemy_at(dest).is_none()
            {
                self.enemies[index].as_mut().unwrap().pos = dest;
            }
        }
        self.health = self.health.saturating_sub(damage);
        if damage > 0 {
            self.flash = 22;
            self.log(&format!(
                "Telegraphed attacks hit for {damage}. Watch the red tiles."
            ));
        }
        if self.health == 0 {
            self.phase = Phase::Lost;
            self.log("Your lantern goes out among the roots.");
            return Cue::Bad;
        }
        self.plan_enemies();
        if damage > 0 { Cue::Bad } else { cue }
    }
    pub(crate) fn threatens(&self, enemy: Enemy, pos: Pos) -> bool {
        match enemy.plan {
            Plan::Strike(target) => pos == target,
            Plan::Beam(dx, dy) => self.ray_hits(enemy.pos, pos, dx, dy, 6),
            Plan::Cross => DIRS
                .iter()
                .any(|&(dx, dy)| self.ray_hits(enemy.pos, pos, dx, dy, 4)),
            Plan::Ring => {
                (1..=2).contains(&enemy.pos.distance(pos)) && self.tile(pos) != Tile::Wall
            }
            _ => false,
        }
    }
    fn ray_hits(&self, origin: Pos, target: Pos, dx: i32, dy: i32, range: i32) -> bool {
        for step in 1..=range {
            let pos = origin.offset(dx * step, dy * step);
            if self.tile(pos) == Tile::Wall || self.tile(pos) == Tile::Seal {
                break;
            }
            if pos == target {
                return true;
            }
        }
        false
    }
    pub(crate) fn danger(&self, pos: Pos) -> u8 {
        self.enemies
            .iter()
            .flatten()
            .filter(|enemy| self.threatens(**enemy, pos))
            .map(|enemy| enemy.species.damage())
            .sum()
    }
    fn plan_enemies(&mut self) {
        for index in 0..ENEMIES {
            let Some(enemy) = self.enemies[index] else {
                continue;
            };
            let mut round = enemy.rounds;
            let plan = if enemy.plan.is_attack() {
                Plan::Rest
            } else {
                match enemy.species {
                    Species::King => {
                        round = round.wrapping_add(1);
                        if round % 2 == 1 {
                            Plan::Cross
                        } else {
                            Plan::Ring
                        }
                    }
                    Species::Spore => {
                        let dx = (self.hero.x - enemy.pos.x).signum();
                        let dy = (self.hero.y - enemy.pos.y).signum();
                        if (dx == 0 || dy == 0) && self.ray_hits(enemy.pos, self.hero, dx, dy, 6) {
                            Plan::Beam(dx, dy)
                        } else {
                            Plan::Rest
                        }
                    }
                    Species::Sprout | Species::Knight => {
                        let range = if enemy.species == Species::Knight {
                            2
                        } else {
                            1
                        };
                        if enemy.pos.distance(self.hero) <= range {
                            Plan::Strike(self.hero)
                        } else if enemy.pos.distance(self.hero) <= 7 {
                            DIRS.iter()
                                .map(|&(dx, dy)| enemy.pos.offset(dx, dy))
                                .filter(|pos| {
                                    self.walkable(*pos)
                                        && self.enemy_at(*pos).is_none()
                                        && *pos != self.hero
                                })
                                .min_by_key(|pos| pos.distance(self.hero))
                                .filter(|pos| {
                                    pos.distance(self.hero) < enemy.pos.distance(self.hero)
                                })
                                .map_or(Plan::Rest, Plan::Move)
                        } else {
                            Plan::Rest
                        }
                    }
                }
            };
            let target = self.enemies[index].as_mut().unwrap();
            target.plan = plan;
            target.rounds = round;
        }
    }
    fn goal(&self) -> Pos {
        if self.seed && self.king_alive() {
            return self
                .enemies
                .iter()
                .flatten()
                .find(|enemy| enemy.species == Species::King)
                .unwrap()
                .pos;
        }
        let wanted = if self.seed { Tile::Stairs } else { Tile::Seed };
        let index = self
            .board
            .iter()
            .position(|tile| *tile == wanted)
            .unwrap_or(self.hero.index());
        Pos::new((index % COLS) as i32, (index / COLS) as i32)
    }
    fn distances(&self, goal: Pos) -> [u16; CELLS] {
        let mut distance = [u16::MAX; CELLS];
        let mut queue = [Pos::default(); CELLS];
        distance[goal.index()] = 0;
        queue[0] = goal;
        let mut read = 0;
        let mut write = 1;
        while read < write {
            let current = queue[read];
            read += 1;
            for (dx, dy) in DIRS {
                let next = current.offset(dx, dy);
                if self.walkable(next) && distance[next.index()] == u16::MAX {
                    distance[next.index()] = distance[current.index()] + 1;
                    queue[write] = next;
                    write += 1;
                }
            }
        }
        distance
    }

    /// Read-only legal-input driver for reproducible smoke runs. It navigates the
    /// authored board, heals, fights and chooses relics through [`Self::step`];
    /// it never modifies the expedition or bypasses combat.
    pub fn autoplay(&self, tick: u64) -> Input {
        if !tick.is_multiple_of(9) || self.outcome() != Outcome::Playing {
            return Input::default();
        }
        if self.phase == Phase::Camp {
            return Input {
                slot: Some(1),
                ..Input::default()
            };
        }
        if self.health <= self.max_health.saturating_sub(10) && self.flasks > 0 {
            return Input {
                secondary: true,
                ..Input::default()
            };
        }
        if self.tile(self.hero) == Tile::Stairs && self.seed && !self.king_alive() {
            return Input {
                primary: true,
                ..Input::default()
            };
        }
        let goal = self.goal();
        let distance = self.distances(goal);
        let base = distance[self.hero.index()];
        let mut best = Input {
            primary: true,
            ..Input::default()
        };
        let mut best_value = i32::MIN;
        let options = [
            Input {
                up_pressed: true,
                ..Input::default()
            },
            Input {
                left_pressed: true,
                ..Input::default()
            },
            Input {
                right_pressed: true,
                ..Input::default()
            },
            Input {
                down_pressed: true,
                ..Input::default()
            },
            Input {
                primary: true,
                ..Input::default()
            },
            Input {
                tertiary: true,
                ..Input::default()
            },
        ];
        for input in options {
            let mut next = self.clone();
            let before_enemies: u16 = self
                .enemies
                .iter()
                .flatten()
                .map(|e| u16::from(e.health))
                .sum();
            let before_turns = next.turns;
            next.step(&input);
            if next.turns == before_turns || next.phase == Phase::Lost {
                continue;
            }
            let after_enemies: u16 = next
                .enemies
                .iter()
                .flatten()
                .map(|e| u16::from(e.health))
                .sum();
            let progress = i32::from(base) - i32::from(distance[next.hero.index()]);
            let hurt = i32::from(self.health) - i32::from(next.health);
            let dealt = i32::from(before_enemies) - i32::from(after_enemies);
            let useful_kill = i32::from(next.kills - self.kills);
            let value = progress * 24 - hurt * 7 + dealt * 9 + useful_kill * 10
                - if input.tertiary { 14 } else { 0 }
                - if input.primary { 9 } else { 0 }
                + if !self.seed && next.seed { 100 } else { 0 };
            if value > best_value {
                best_value = value;
                best = input;
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_floors_have_reachable_seed_stairs_and_bounded_enemies() {
        for (depth, floor) in FLOORS.iter().enumerate() {
            let mut game = Game::new();
            game.depth = depth;
            game.load_floor();
            assert!(floor.map.iter().all(|row| row.len() == COLS));
            let seed = game.goal();
            assert_ne!(
                game.distances(seed)[game.hero.index()],
                u16::MAX,
                "floor {} seed",
                depth + 1
            );
            game.seed = true;
            let exit = game
                .board
                .iter()
                .position(|tile| *tile == Tile::Stairs)
                .unwrap();
            let exit = Pos::new((exit % COLS) as i32, (exit / COLS) as i32);
            assert_ne!(
                game.distances(exit)[seed.index()],
                u16::MAX,
                "floor {} exit",
                depth + 1
            );
            assert_eq!(
                game.enemies
                    .iter()
                    .flatten()
                    .filter(|e| e.species == Species::King)
                    .count(),
                usize::from(depth == 5)
            );
        }
    }

    #[test]
    fn full_expedition_is_won_only_through_legal_inputs() {
        let mut game = Game::new();
        let mut visited = [false; 6];
        for tick in 0..54_000 {
            visited[game.depth] = true;
            let input = game.autoplay(tick);
            game.step(&input);
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(
            game.outcome(),
            Outcome::Won,
            "floor={} pos={:?} hp={} turns={} messages={:?} enemies={:?}",
            game.floor(),
            game.hero,
            game.health,
            game.turns,
            game.messages,
            game.enemies
        );
        assert!(visited.iter().all(|visited| *visited));
        assert!(!game.king_alive());
        assert_eq!(game.choices.iter().sum::<u8>(), 5);
        assert!(game.score >= 4700);
    }

    #[test]
    fn idle_and_invalid_actions_cannot_advance_or_farm_rewards() {
        let mut game = Game::new();
        let enemies = game.enemies;
        for _ in 0..100 {
            game.step(&Input::default());
            game.step(&Input {
                up_pressed: true,
                ..Input::default()
            });
            game.step(&Input {
                secondary: true,
                ..Input::default()
            });
            game.step(&Input {
                tertiary: true,
                ..Input::default()
            });
        }
        assert_eq!(game.turns, 0);
        assert_eq!(game.score, 0);
        assert_eq!(game.flasks, 2);
        assert_eq!(game.charge, 3);
        assert_eq!(game.enemies, enemies);
    }

    #[test]
    fn quick_direction_edge_only_moves_once_without_repetition() {
        let mut game = Game::new();
        game.step(&Input {
            right: true,
            right_pressed: true,
            ..Input::default()
        });
        let position = game.hero;
        for _ in 0..120 {
            game.step(&Input {
                right: true,
                ..Input::default()
            });
        }
        assert_eq!(game.hero, position);
        assert_eq!(game.turns, 1);
    }

    #[test]
    fn telegraph_is_fixed_and_can_be_dodged() {
        let mut game = Game::new();
        game.enemies.fill(None);
        game.enemies[0] = Some(Enemy {
            pos: Pos::new(2, 1),
            species: Species::Knight,
            health: 10,
            max_health: 10,
            plan: Plan::Strike(game.hero),
            rounds: 0,
        });
        let hp = game.health;
        game.step(&Input {
            down_pressed: true,
            ..Input::default()
        });
        assert_eq!(game.health, hp);
        assert_eq!(game.enemies[0].unwrap().plan, Plan::Rest);
    }

    #[test]
    fn guard_mitigates_each_attack_and_killing_cancels_telegraph() {
        let mut game = Game::new();
        game.enemies.fill(None);
        let enemy = Enemy {
            pos: Pos::new(2, 1),
            species: Species::Knight,
            health: 3,
            max_health: 10,
            plan: Plan::Strike(game.hero),
            rounds: 0,
        };
        game.enemies[0] = Some(enemy);
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(game.health, 24);
        game.enemies[0] = Some(enemy);
        game.step(&Input {
            right_pressed: true,
            ..Input::default()
        });
        assert_eq!(game.health, 24);
        assert!(game.enemies[0].is_none());
        assert_eq!(game.kills, 1);
    }

    #[test]
    fn flask_and_well_are_consumed_once_and_health_is_capped() {
        let mut game = Game::new();
        game.enemies.fill(None);
        game.health = 10;
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!((game.health, game.flasks), (22, 1));
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!((game.health, game.flasks), (26, 0));
        game.board[Pos::new(2, 1).index()] = Tile::Well;
        game.step(&Input {
            right_pressed: true,
            ..Input::default()
        });
        assert_eq!(game.flasks, 1);
        game.step(&Input {
            left_pressed: true,
            ..Input::default()
        });
        game.step(&Input {
            right_pressed: true,
            ..Input::default()
        });
        assert_eq!(game.flasks, 1);
        assert_eq!(game.board[Pos::new(2, 1).index()], Tile::EmptyWell);
    }

    #[test]
    fn enemy_damage_causes_loss_and_terminal_state_is_immutable() {
        let mut game = Game::new();
        game.enemies.fill(None);
        game.health = 1;
        game.enemies[0] = Some(Enemy {
            pos: Pos::new(2, 1),
            species: Species::King,
            health: 36,
            max_health: 36,
            plan: Plan::Strike(game.hero),
            rounds: 0,
        });
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(game.outcome(), Outcome::Lost);
        let snapshot = format!("{game:?}");
        for _ in 0..20 {
            game.step(&Input {
                secondary: true,
                primary: true,
                right_pressed: true,
                ..Input::default()
            });
        }
        assert_eq!(format!("{game:?}"), snapshot);
        let retry = Game::new();
        assert_eq!(retry.outcome(), Outcome::Playing);
        assert_eq!(
            (retry.turns, retry.depth, retry.flasks, retry.charge),
            (0, 0, 2, 3)
        );
    }

    #[test]
    fn exits_require_seed_and_final_boss_defeat() {
        let mut game = Game::new();
        game.board[game.hero.index()] = Tile::Stairs;
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(game.phase, Phase::Explore);
        assert_eq!(game.score, 0);
        game.seed = true;
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(game.phase, Phase::Camp);
        assert_eq!(game.score, 350);
        game.step(&Input {
            slot: Some(2),
            ..Input::default()
        });
        assert_eq!(game.depth, 1);
        assert_eq!(game.max_health, 31);
        game.depth = 5;
        game.load_floor();
        game.seed = true;
        game.board[game.hero.index()] = Tile::Stairs;
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(game.outcome(), Outcome::Playing);
    }

    #[test]
    fn amber_is_unique_and_burst_consumes_one_charge_for_multiple_targets() {
        let mut game = Game::new();
        game.enemies.fill(None);
        game.board[Pos::new(2, 1).index()] = Tile::Amber;
        for direction in [true, false, true, false] {
            game.step(&Input {
                right_pressed: direction,
                left_pressed: !direction,
                ..Input::default()
            });
        }
        assert_eq!((game.treasure, game.score), (1, 140));
        for (index, pos) in [Pos::new(2, 1), Pos::new(1, 2)].into_iter().enumerate() {
            game.enemies[index] = Some(Enemy {
                pos,
                species: Species::Sprout,
                health: 5,
                max_health: 5,
                plan: Plan::Strike(game.hero),
                rounds: 0,
            });
        }
        let health = game.health;
        let charge = game.charge;
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(game.charge, charge - 1);
        assert_eq!(game.kills, 2);
        assert_eq!(game.health, health);
        assert_eq!(game.score, 300);
    }

    #[test]
    fn idle_fixed_ticks_do_not_change_an_identical_action_sequence() {
        let mut immediate = Game::new();
        let mut idle = Game::new();
        for tick in (0..600).step_by(9) {
            let input = immediate.autoplay(tick);
            immediate.step(&input);
            for _ in 0..17 {
                idle.step(&Input::default());
            }
            idle.step(&input);
        }
        assert_eq!(immediate.hero, idle.hero);
        assert_eq!(immediate.enemies, idle.enemies);
        assert_eq!(immediate.board, idle.board);
        assert_eq!(immediate.health, idle.health);
        assert_eq!(immediate.score, idle.score);
        assert_eq!(immediate.turns, idle.turns);
        assert_eq!(immediate.depth, idle.depth);
    }

    #[test]
    fn victory_rejects_every_later_action_and_does_not_duplicate_bonus() {
        let mut game = Game::new();
        for tick in 0..54_000 {
            game.step(&game.autoplay(tick));
            if game.outcome() == Outcome::Won {
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Won);
        let before = format!("{game:?}");
        for slot in 1..=5 {
            game.step(&Input {
                primary: true,
                secondary: true,
                tertiary: true,
                slot: Some(slot),
                ..Input::default()
            });
        }
        assert_eq!(format!("{game:?}"), before);
    }

    #[test]
    fn beam_stops_at_walls_and_burst_rejects_empty_targets() {
        let mut game = Game::new();
        game.enemies.fill(None);
        let enemy = Enemy {
            pos: Pos::new(1, 1),
            species: Species::Spore,
            health: 6,
            max_health: 6,
            plan: Plan::Beam(1, 0),
            rounds: 0,
        };
        assert!(game.threatens(enemy, Pos::new(4, 1)));
        assert!(!game.threatens(enemy, Pos::new(6, 1)));
        let charge = game.charge;
        assert_eq!(
            game.step(&Input {
                tertiary: true,
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!((game.charge, game.turns), (charge, 0));
    }
}
