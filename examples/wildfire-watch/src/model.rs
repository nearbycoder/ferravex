//! Pure, integer, turn-based incident command. Time advances only on an accepted
//! end-turn command; render cadence and idle ticks never spread fire.
//!
//! ```
//! use wildfire_watch::Game;
//! use ferravex_arcade::{Input, Outcome};
//! let mut game = Game::new();
//! assert_eq!(game.mission(), 1);
//! game.step(&Input { primary: true, ..Input::default() });
//! for _ in 0..120 { game.step(&Input::default()); }
//! assert_eq!(game.outcome(), Outcome::Playing);
//! assert_eq!(game.rescued(), 0);
//! ```
use crate::content::{BASE, CELLS, COLS, MISSIONS, ROWS};
use crate::layout::{self, ACT, CUT, DEPLOY, DROP, END, FORECAST, RETRY};
use ferravex_arcade::{Cue, Input, Outcome};
use ferravex_play::navigation::{Cell as NavCell, Connectivity, Grid};

/// Credits charged for one air drop.
pub(crate) const DROP_COST: u32 = 12;
/// Most presentation events one step can record.
const FEED: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Terrain {
    Grass,
    Forest,
    Brush,
    Road,
    Water,
    Rock,
    Town,
    Base,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tile {
    pub terrain: Terrain,
    pub fuel: u8,
    pub fire: u8,
    pub wet: u8,
    pub people: u8,
    pub ruined: bool,
}
impl Tile {
    fn from_char(c: u8) -> Self {
        let (terrain, fuel) = match c {
            b'f' => (Terrain::Forest, 4),
            b'b' => (Terrain::Brush, 2),
            b'=' => (Terrain::Road, 0),
            b'~' => (Terrain::Water, 0),
            b'#' => (Terrain::Rock, 0),
            _ => (Terrain::Grass, 2),
        };
        Self {
            terrain,
            fuel,
            fire: 0,
            wet: 0,
            people: 0,
            ruined: false,
        }
    }
    pub(crate) fn walkable(self) -> bool {
        self.fire == 0 && !matches!(self.terrain, Terrain::Water | Terrain::Rock)
    }
    pub(crate) fn cost(self) -> u8 {
        if self.terrain == Terrain::Forest && self.fuel > 0 {
            2
        } else {
            1
        }
    }
    fn resistance(self) -> u8 {
        match self.terrain {
            Terrain::Forest | Terrain::Town => 4,
            Terrain::Brush => 2,
            _ => 3,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Crew {
    pub tile: usize,
    pub ap: u8,
    pub water: u8,
    pub cargo: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Briefing,
    Active,
    Debrief,
    Setback,
    Ended,
}
/// The context action a crew would take at a tile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Walk the path (crew tile first) for the given action points.
    Move(Vec<usize>, u8),
    Hose,
    Load,
    Unload,
    Refill,
}
/// Why an incident attempt failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Setback {
    /// The occupied village on this tile caught fire.
    Village(usize),
    /// This crew was caught by new fire with residents aboard.
    Trapped(usize),
    /// The turn deadline passed.
    Nightfall,
}
/// What a refused command lacked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lack {
    /// Action points; the value is the cost that could not be paid.
    Ap(u8),
    Water,
    Seats,
    /// The target is beyond the crew's reach for this command.
    Range,
    /// Credits; the value is the price that could not be paid.
    Credits(u32),
    Sorties,
    /// An air drop footprint holds no fire.
    NoFire,
    /// Base, villages, burning and fuelless tiles cannot become breaks.
    Uncuttable,
    /// The upgrade is already at its highest level.
    Maxed,
}
/// Where a presentation event happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Spot {
    Tile(usize),
    Upgrade(usize),
}
/// One accepted (or refused) command or spread consequence, for effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    Deployed,
    Retried,
    Advanced,
    Selected(usize),
    Forecast(bool),
    Moved {
        crew: usize,
        from: usize,
        to: usize,
    },
    Sprayed {
        crew: usize,
        at: usize,
        removed: u8,
        out: bool,
    },
    Loaded {
        crew: usize,
        at: usize,
        people: u8,
    },
    Delivered {
        crew: usize,
        people: u8,
    },
    Refilled {
        crew: usize,
        at: usize,
        water: u8,
    },
    Cut {
        crew: usize,
        at: usize,
    },
    Dropped(usize),
    /// Bitmasks of tiles that ignited and burned out in one spread.
    Spread {
        lit: u128,
        out: u128,
    },
    Retreated(usize),
    Ruined(usize),
    Cleared {
        stars: u8,
        credits: u32,
    },
    Failed(Setback),
    Bought {
        id: usize,
        cost: u32,
    },
    Refused {
        spot: Spot,
        lack: Lack,
    },
}
/// Presentation-only record of what the latest [`Game::step`] accepted or
/// refused, and where. Every step clears it first and the rules never read
/// it, so equality ignores it: two games with the same rule state are equal
/// whatever effects they last requested.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Feed([Option<Event>; FEED]);
impl Feed {
    fn clear(&mut self) {
        self.0 = [None; FEED];
    }
    fn push(&mut self, event: Event) {
        if let Some(slot) = self.0.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(event);
        }
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = Event> + '_ {
        self.0.iter().map_while(|event| *event)
    }
}
impl PartialEq for Feed {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for Feed {}

/// A complete six-incident career. Every gameplay mutation uses [`Game::step`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub(crate) tiles: [Tile; CELLS],
    pub(crate) crews: [Crew; 3],
    pub(crate) phase: Phase,
    pub(crate) index: usize,
    pub(crate) selected: usize,
    pub(crate) cursor: usize,
    pub(crate) turn: u8,
    pub(crate) cash: u32,
    pub(crate) upgrades: [u8; 3],
    pub(crate) drops: u8,
    pub(crate) used_drops: u8,
    pub(crate) retries: u8,
    pub(crate) medals: [u8; 6],
    pub(crate) landed: u8,
    pub(crate) total_rescued: u32,
    pub(crate) points: u32,
    pub(crate) show_forecast: bool,
    pub(crate) retreats: u8,
    /// Why the latest attempt failed, until the incident reloads.
    pub(crate) setback: Option<Setback>,
    pub(crate) feed: Feed,
    start_cash: u32,
    won: bool,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Start a fresh career at Pine Creek's briefing.
    pub fn new() -> Self {
        let mut g = Self {
            tiles: [Tile::from_char(b'.'); CELLS],
            crews: [Crew {
                tile: BASE,
                ap: 4,
                water: 4,
                cargo: 0,
            }; 3],
            phase: Phase::Briefing,
            index: 0,
            selected: 0,
            cursor: BASE,
            turn: 1,
            cash: 50,
            upgrades: [0; 3],
            drops: 2,
            used_drops: 0,
            retries: 3,
            medals: [0; 6],
            landed: 0,
            total_rescued: 0,
            points: 0,
            show_forecast: true,
            retreats: 0,
            setback: None,
            feed: Feed::default(),
            start_cash: 50,
            won: false,
        };
        g.load();
        g
    }
    fn load(&mut self) {
        let content = &MISSIONS[self.index];
        for (y, row) in content.rows.iter().enumerate() {
            assert_eq!(row.len(), COLS, "authored row width");
            for (x, c) in row.bytes().enumerate() {
                self.tiles[y * COLS + x] = Tile::from_char(c);
            }
        }
        self.tiles[BASE] = Tile {
            terrain: Terrain::Base,
            fuel: 0,
            ..Tile::from_char(b'=')
        };
        for v in content.villages {
            self.tiles[v.y * COLS + v.x] = Tile {
                terrain: Terrain::Town,
                fuel: 4,
                people: v.people,
                ..Tile::from_char(b'f')
            };
        }
        for &(x, y) in content.fires {
            self.tiles[y * COLS + x].fuel = 4;
            self.tiles[y * COLS + x].fire = 2;
        }
        for id in 0..3 {
            self.crews[id] = Crew {
                tile: BASE,
                ap: self.max_ap(),
                water: self.water_max(id),
                cargo: 0,
            };
        }
        self.phase = Phase::Briefing;
        self.selected = 0;
        self.cursor = BASE;
        self.turn = 1;
        self.landed = 0;
        self.drops = 2 + self.upgrades[2];
        self.used_drops = 0;
        self.retreats = 0;
        self.start_cash = self.cash;
        self.setback = None;
    }
    /// Current incident number, one through six.
    pub fn mission(&self) -> usize {
        self.index + 1
    }
    /// Banked score. A failed attempt never awards points.
    pub fn score(&self) -> u32 {
        self.points
    }
    /// People delivered to base in this and earlier cleared incidents.
    pub fn rescued(&self) -> u32 {
        self.total_rescued + u32::from(self.landed)
    }
    /// Terminal career result; a recoverable setback is still `Playing`.
    pub fn outcome(&self) -> Outcome {
        if self.phase != Phase::Ended {
            Outcome::Playing
        } else if self.won {
            Outcome::Won
        } else {
            Outcome::Lost
        }
    }
    pub(crate) fn max_ap(&self) -> u8 {
        4 + self.upgrades[1]
    }
    pub(crate) fn water_max(&self, id: usize) -> u8 {
        (if id == 0 { 6 } else { 3 }) + self.upgrades[0] * 2
    }
    pub(crate) fn capacity(id: usize) -> u8 {
        if id == 2 { 6 } else { 2 }
    }
    pub(crate) fn range(id: usize) -> usize {
        if id == 0 { 2 } else { 1 }
    }
    /// Heat one hose action removes.
    pub(crate) fn power(id: usize) -> u8 {
        if id == 0 { 3 } else { 2 }
    }
    /// Action points one firebreak costs.
    pub(crate) fn cut_cost(id: usize) -> u8 {
        if id == 1 { 1 } else { 2 }
    }
    /// The air-drop cross: the center and its orthogonal neighbors.
    pub(crate) fn footprint(at: usize) -> impl Iterator<Item = usize> {
        std::iter::once(at).chain(Self::adjacent(at))
    }
    /// Residents a load at `at` would board now.
    pub(crate) fn boarding(&self, id: usize, at: usize) -> u8 {
        (Self::capacity(id) - self.crews[id].cargo).min(self.tiles[at].people)
    }
    /// Medal the incident would earn if it were cleared now.
    pub(crate) fn stars(&self) -> u8 {
        let ruined = self.tiles.iter().any(|t| t.ruined);
        if !ruined && self.retreats == 0 && self.used_drops <= 1 {
            3
        } else if self.retreats <= 1 && !ruined {
            2
        } else {
            1
        }
    }
    pub(crate) fn wind(&self, next: bool) -> (i8, i8) {
        MISSIONS[self.index].wind[(usize::from(self.turn - 1) + usize::from(next)) % 4]
    }
    pub(crate) fn quota(&self) -> u8 {
        MISSIONS[self.index].villages.iter().map(|v| v.people).sum()
    }
    pub(crate) fn fire_count(&self) -> usize {
        self.tiles.iter().filter(|t| t.fire > 0).count()
    }
    pub(crate) fn distance(a: usize, b: usize) -> usize {
        (a % COLS).abs_diff(b % COLS) + (a / COLS).abs_diff(b / COLS)
    }
    pub(crate) fn adjacent(at: usize) -> impl Iterator<Item = usize> {
        let x = at % COLS;
        let y = at / COLS;
        [
            x.checked_sub(1).map(|x| y * COLS + x),
            (x + 1 < COLS).then_some(at + 1),
            y.checked_sub(1).map(|y| y * COLS + x),
            (y + 1 < ROWS).then_some(at + COLS),
        ]
        .into_iter()
        .flatten()
    }
    pub(crate) fn forecast(&self) -> [bool; CELLS] {
        let mut result = [false; CELLS];
        let wind = self.wind(false);
        for (at, tile) in self.tiles.iter().enumerate() {
            if tile.fuel == 0 || tile.fire > 0 || tile.wet > 0 {
                continue;
            }
            let mut heat = 0u8;
            for from in Self::adjacent(at) {
                if self.tiles[from].fire == 0 {
                    continue;
                }
                let delta = (
                    (at % COLS) as i8 - (from % COLS) as i8,
                    (at / COLS) as i8 - (from / COLS) as i8,
                );
                heat =
                    heat.saturating_add(self.tiles[from].fire + if delta == wind { 2 } else { 0 });
            }
            result[at] = heat >= tile.resistance();
        }
        result
    }
    fn route(&self, start: usize, end: usize) -> Option<Vec<usize>> {
        let mut grid = Grid::new(COLS as u32, ROWS as u32).ok()?;
        for (at, tile) in self.tiles.iter().enumerate() {
            let p = NavCell {
                x: (at % COLS) as u32,
                y: (at / COLS) as u32,
            };
            if !tile.walkable() {
                grid.set_blocked(p, true).ok()?;
            } else {
                grid.set_cost(p, u16::from(tile.cost())).ok()?;
            }
        }
        let p = |at: usize| NavCell {
            x: (at % COLS) as u32,
            y: (at / COLS) as u32,
        };
        grid.path(p(start), p(end), Connectivity::Four)
            .ok()
            .map(|path| {
                path.into_iter()
                    .map(|p| p.y as usize * COLS + p.x as usize)
                    .collect()
            })
    }
    pub(crate) fn action(&self, id: usize, target: usize) -> Option<Action> {
        let crew = self.crews[id];
        let tile = self.tiles[target];
        let distance = Self::distance(crew.tile, target);
        if tile.fire > 0 && distance <= Self::range(id) {
            return (crew.ap >= 2 && crew.water > 0).then_some(Action::Hose);
        }
        if tile.people > 0 && distance <= 1 {
            return (crew.ap >= 2 && crew.cargo < Self::capacity(id)).then_some(Action::Load);
        }
        if target == BASE && distance <= 1 && crew.cargo > 0 {
            return (crew.ap > 0).then_some(Action::Unload);
        }
        if matches!(tile.terrain, Terrain::Base | Terrain::Water)
            && distance <= 1
            && crew.water < self.water_max(id)
        {
            return (crew.ap > 0).then_some(Action::Refill);
        }
        if crew.tile == target || !tile.walkable() {
            return None;
        }
        let path = self.route(crew.tile, target)?;
        let cost: u8 = path.iter().skip(1).map(|&at| self.tiles[at].cost()).sum();
        (cost <= crew.ap).then_some(Action::Move(path, cost))
    }
    /// Why [`Self::action`] refuses a target that offers a context action the
    /// crew cannot afford or reach this turn. `None` when the action is
    /// accepted or the tile offers nothing (rock, the crew's own tile, an
    /// unreachable pocket).
    pub(crate) fn lack(&self, id: usize, target: usize) -> Option<Lack> {
        let crew = self.crews[id];
        let tile = self.tiles[target];
        let distance = Self::distance(crew.tile, target);
        if tile.fire > 0 {
            return if distance > Self::range(id) {
                Some(Lack::Range)
            } else if crew.water == 0 {
                Some(Lack::Water)
            } else {
                (crew.ap < 2).then_some(Lack::Ap(2))
            };
        }
        if tile.people > 0 && distance <= 1 {
            return if crew.cargo >= Self::capacity(id) {
                Some(Lack::Seats)
            } else {
                (crew.ap < 2).then_some(Lack::Ap(2))
            };
        }
        let deliver = target == BASE && distance <= 1 && crew.cargo > 0;
        let refill = matches!(tile.terrain, Terrain::Base | Terrain::Water)
            && distance <= 1
            && crew.water < self.water_max(id);
        if deliver || refill {
            return (crew.ap == 0).then_some(Lack::Ap(1));
        }
        if crew.tile == target || !tile.walkable() {
            return None;
        }
        let path = self.route(crew.tile, target)?;
        let cost: u8 = path.iter().skip(1).map(|&at| self.tiles[at].cost()).sum();
        (cost > crew.ap).then_some(Lack::Ap(cost))
    }
    /// Least action points crew `id` needs to stand on each tile, using the
    /// same terrain costs and blockers as movement; `u8::MAX` is unreachable.
    pub(crate) fn reach(&self, id: usize) -> [u8; CELLS] {
        let mut cost = [u8::MAX; CELLS];
        let mut done = [false; CELLS];
        cost[self.crews[id].tile] = 0;
        while let Some(at) = (0..CELLS)
            .filter(|&at| !done[at] && cost[at] != u8::MAX)
            .min_by_key(|&at| cost[at])
        {
            done[at] = true;
            for next in Self::adjacent(at) {
                let tile = self.tiles[next];
                if tile.walkable() {
                    let through = cost[at].saturating_add(tile.cost());
                    if through < cost[next] {
                        cost[next] = through;
                    }
                }
            }
        }
        cost
    }
    /// Why a firebreak at `at` is refused, if it is.
    pub(crate) fn cut_lack(&self, id: usize, at: usize) -> Option<Lack> {
        let tile = self.tiles[at];
        let cost = Self::cut_cost(id);
        if tile.fuel == 0 || tile.fire > 0 || matches!(tile.terrain, Terrain::Town | Terrain::Base)
        {
            Some(Lack::Uncuttable)
        } else if Self::distance(self.crews[id].tile, at) > 1 {
            Some(Lack::Range)
        } else {
            (self.crews[id].ap < cost).then_some(Lack::Ap(cost))
        }
    }
    /// Why an air drop centered on `at` is refused, if it is.
    pub(crate) fn drop_lack(&self, at: usize) -> Option<Lack> {
        if self.drops == 0 {
            Some(Lack::Sorties)
        } else if self.cash < DROP_COST {
            Some(Lack::Credits(DROP_COST))
        } else if !Self::footprint(at).any(|p| self.tiles[p].fire > 0) {
            Some(Lack::NoFire)
        } else {
            None
        }
    }
    fn act(&mut self) -> Cue {
        self.order(true)
    }
    /// The context action at the cursor. A map click also aims (air drops,
    /// inspection), so only an explicit order reports a target out of reach.
    fn order(&mut self, explicit: bool) -> Cue {
        let id = self.selected;
        let at = self.cursor;
        let Some(action) = self.action(id, at) else {
            if let Some(lack) = self
                .lack(id, at)
                .filter(|&lack| explicit || lack != Lack::Range)
            {
                self.feed.push(Event::Refused {
                    spot: Spot::Tile(at),
                    lack,
                });
            }
            return Cue::None;
        };
        match action {
            Action::Move(path, cost) => {
                let from = self.crews[id].tile;
                let to = *path.last().unwrap();
                self.crews[id].tile = to;
                self.crews[id].ap -= cost;
                self.feed.push(Event::Moved { crew: id, from, to });
            }
            Action::Hose => {
                self.crews[id].ap -= 2;
                self.crews[id].water -= 1;
                let before = self.tiles[at].fire;
                self.tiles[at].fire = before.saturating_sub(Self::power(id));
                let out = self.tiles[at].fire == 0;
                if out {
                    self.tiles[at].wet = 3;
                }
                self.feed.push(Event::Sprayed {
                    crew: id,
                    at,
                    removed: before - self.tiles[at].fire,
                    out,
                });
            }
            Action::Load => {
                let n = self.boarding(id, at);
                self.crews[id].cargo += n;
                self.tiles[at].people -= n;
                self.crews[id].ap -= 2;
                self.feed.push(Event::Loaded {
                    crew: id,
                    at,
                    people: n,
                });
            }
            Action::Unload => {
                let n = self.crews[id].cargo;
                self.landed += n;
                self.crews[id].cargo = 0;
                self.crews[id].ap -= 1;
                self.feed.push(Event::Delivered {
                    crew: id,
                    people: n,
                });
            }
            Action::Refill => {
                let water = self.water_max(id) - self.crews[id].water;
                self.crews[id].water = self.water_max(id);
                self.crews[id].ap -= 1;
                self.feed.push(Event::Refilled {
                    crew: id,
                    at,
                    water,
                });
            }
        }
        if self.check_clear() {
            Cue::Win
        } else {
            Cue::Good
        }
    }
    fn cut(&mut self) -> Cue {
        let at = self.cursor;
        let id = self.selected;
        if let Some(lack) = self.cut_lack(id, at) {
            self.feed.push(Event::Refused {
                spot: Spot::Tile(at),
                lack,
            });
            return Cue::None;
        }
        self.crews[id].ap -= Self::cut_cost(id);
        self.tiles[at].fuel = 0;
        self.feed.push(Event::Cut { crew: id, at });
        Cue::Good
    }
    fn drop_water(&mut self) -> Cue {
        if let Some(lack) = self.drop_lack(self.cursor) {
            self.feed.push(Event::Refused {
                spot: Spot::Tile(self.cursor),
                lack,
            });
            return Cue::None;
        }
        for at in Self::footprint(self.cursor) {
            self.tiles[at].fire = self.tiles[at].fire.saturating_sub(3);
            self.tiles[at].wet = 3;
        }
        self.cash -= DROP_COST;
        self.drops -= 1;
        self.used_drops += 1;
        self.feed.push(Event::Dropped(self.cursor));
        if self.check_clear() {
            Cue::Win
        } else {
            Cue::Good
        }
    }
    fn fail(&mut self, reason: Setback) -> Cue {
        self.phase = if self.retries == 0 {
            Phase::Ended
        } else {
            Phase::Setback
        };
        self.setback = Some(reason);
        self.feed.push(Event::Failed(reason));
        Cue::Bad
    }
    fn weather(&mut self) -> Cue {
        let ignition = self.forecast();
        let (mut lit, mut out) = (0u128, 0u128);
        for (at, ignite) in ignition.into_iter().enumerate() {
            let tile = &mut self.tiles[at];
            if tile.fire > 0 {
                tile.fuel = tile.fuel.saturating_sub(1);
                tile.fire = if tile.fuel == 0 {
                    out |= 1 << at;
                    0
                } else {
                    (tile.fire + 1).min(3)
                };
            }
            if ignite {
                tile.fire = 1;
                lit |= 1 << at;
            }
            tile.wet = tile.wet.saturating_sub(1);
            if tile.fire > 0 && tile.terrain == Terrain::Town {
                let fresh = !tile.ruined;
                tile.ruined = true;
                if tile.people > 0 {
                    self.feed.push(Event::Spread { lit, out });
                    return self.fail(Setback::Village(at));
                }
                if fresh {
                    self.feed.push(Event::Ruined(at));
                }
            }
        }
        self.feed.push(Event::Spread { lit, out });
        for id in 0..3 {
            if self.tiles[self.crews[id].tile].fire > 0 {
                if self.crews[id].cargo > 0 {
                    return self.fail(Setback::Trapped(id));
                }
                self.crews[id].tile = BASE;
                self.crews[id].water = 0;
                self.retreats += 1;
                self.feed.push(Event::Retreated(id));
            }
            self.crews[id].ap = self.max_ap();
        }
        self.turn += 1;
        if self.check_clear() {
            return Cue::Win;
        }
        if self.turn > MISSIONS[self.index].limit {
            return self.fail(Setback::Nightfall);
        }
        Cue::Action
    }
    fn check_clear(&mut self) -> bool {
        if self.phase != Phase::Active || self.landed != self.quota() || self.fire_count() != 0 {
            return false;
        }
        let stars = self.stars();
        self.medals[self.index] = stars;
        let saved = self
            .tiles
            .iter()
            .filter(|t| t.terrain == Terrain::Forest && t.fuel > 0)
            .count() as u32;
        self.points += 1000
            + u32::from(self.quota()) * 100
            + saved * 5
            + u32::from(MISSIONS[self.index].limit.saturating_sub(self.turn)) * 20
            + u32::from(stars) * 200;
        let credits = 25 + u32::from(self.quota()) * 3;
        self.cash += credits;
        self.total_rescued += u32::from(self.landed);
        self.landed = 0;
        self.phase = if self.index == 5 {
            self.won = true;
            Phase::Ended
        } else {
            Phase::Debrief
        };
        self.feed.push(Event::Cleared { stars, credits });
        true
    }
    pub(crate) fn upgrade_cost(&self, id: usize) -> u32 {
        [30, 35, 40][id] + u32::from(self.upgrades[id]) * 15
    }
    fn buy(&mut self, id: usize) -> Cue {
        let cost = self.upgrade_cost(id);
        if self.upgrades[id] >= 2 || self.cash < cost {
            let lack = if self.upgrades[id] >= 2 {
                Lack::Maxed
            } else {
                Lack::Credits(cost)
            };
            self.feed.push(Event::Refused {
                spot: Spot::Upgrade(id),
                lack,
            });
            return Cue::None;
        }
        self.cash -= cost;
        self.upgrades[id] += 1;
        self.feed.push(Event::Bought { id, cost });
        Cue::Good
    }
    /// Execute one supplied input tick. Idle ticks and held arrows are inert;
    /// one fresh direction edge moves the targeting cursor by one tile.
    pub fn step(&mut self, input: &Input) -> Cue {
        self.feed.clear();
        if self.phase == Phase::Ended {
            return Cue::None;
        }
        let pointer = input.pointer;
        match self.phase {
            Phase::Briefing => {
                // The drawn deploy button mirrors Space.
                if input.primary || (input.click && layout::hit(DEPLOY, pointer)) {
                    self.phase = Phase::Active;
                    self.feed.push(Event::Deployed);
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Setback => {
                // The drawn retry button mirrors Space.
                if input.primary || (input.click && layout::hit(RETRY, pointer)) {
                    self.retries -= 1;
                    self.cash = self.start_cash;
                    self.load();
                    self.feed.push(Event::Retried);
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Debrief => {
                if let Some(slot @ 1..=3) = input.slot {
                    return self.buy(slot - 1);
                }
                if input.click
                    && let Some(id) = layout::shop_at(pointer)
                {
                    return self.buy(id);
                }
                // The drawn "next" button mirrors Space.
                if input.primary || (input.click && layout::proceed_at(pointer)) {
                    self.index += 1;
                    self.load();
                    self.feed.push(Event::Advanced);
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Active => {}
            Phase::Ended => unreachable!(),
        }
        if let Some(slot) = input.slot {
            match slot {
                1..=3 => return self.select(slot - 1),
                4 => return self.drop_water(),
                5 => return self.toggle_forecast(),
                _ => return Cue::None,
            }
        }
        if input.tertiary {
            return self.weather();
        }
        if input.secondary {
            return self.cut();
        }
        if input.primary {
            return self.act();
        }
        let dx = i32::from(input.right_pressed) - i32::from(input.left_pressed);
        let dy = i32::from(input.down_pressed) - i32::from(input.up_pressed);
        if dx != 0 || dy != 0 {
            let x = (self.cursor % COLS) as i32 + dx;
            let y = (self.cursor / COLS) as i32 + dy;
            if (0..COLS as i32).contains(&x) && (0..ROWS as i32).contains(&y) {
                self.cursor = y as usize * COLS + x as usize;
                return Cue::Action;
            }
        }
        if input.click {
            if let Some(at) = layout::tile_at(pointer) {
                self.cursor = at;
                let cue = self.order(false);
                return if cue == Cue::None { Cue::Action } else { cue };
            }
            if let Some(id) = layout::card_at(pointer) {
                return self.select(id);
            }
            if layout::hit(ACT, pointer) {
                return self.act();
            }
            if layout::hit(CUT, pointer) {
                return self.cut();
            }
            if layout::hit(DROP, pointer) {
                return self.drop_water();
            }
            if layout::hit(END, pointer) {
                return self.weather();
            }
            // The drawn forecast switch mirrors key 5.
            if layout::hit(FORECAST, pointer) {
                return self.toggle_forecast();
            }
        }
        Cue::None
    }
    fn select(&mut self, id: usize) -> Cue {
        self.selected = id;
        self.cursor = self.crews[id].tile;
        self.feed.push(Event::Selected(id));
        Cue::Action
    }
    fn toggle_forecast(&mut self) -> Cue {
        self.show_forecast = !self.show_forecast;
        self.feed.push(Event::Forecast(self.show_forecast));
        Cue::Action
    }
    fn command(target: usize) -> Input {
        Input {
            click: true,
            pointer: layout::center(target),
            ..Input::default()
        }
    }
    fn approach(&self, id: usize, target: usize, range: usize) -> Option<usize> {
        let mut best: Option<(u32, Vec<usize>)> = None;
        for end in 0..CELLS {
            if Self::distance(end, target) > range || !self.tiles[end].walkable() {
                continue;
            }
            let Some(path) = self.route(self.crews[id].tile, end) else {
                continue;
            };
            let cost: u32 = path
                .iter()
                .skip(1)
                .map(|&at| u32::from(self.tiles[at].cost()))
                .sum();
            if best.as_ref().is_none_or(|b| cost < b.0) {
                best = Some((cost, path));
            }
        }
        let (_, path) = best?;
        let mut spent = 0;
        let mut destination = self.crews[id].tile;
        for at in path.into_iter().skip(1) {
            spent += self.tiles[at].cost();
            if spent > self.crews[id].ap {
                break;
            }
            destination = at;
        }
        (destination != self.crews[id].tile && self.action(id, destination).is_some())
            .then_some(destination)
    }
    fn crew_plan(&self, id: usize) -> Option<usize> {
        let crew = self.crews[id];
        if crew.ap == 0 {
            return None;
        }
        if crew.cargo > 0 {
            if matches!(self.action(id, BASE), Some(Action::Unload)) {
                return Some(BASE);
            }
            return self.approach(id, BASE, 1);
        }
        let residents: Vec<_> = (0..CELLS).filter(|&at| self.tiles[at].people > 0).collect();
        if id == 2 || self.fire_count() == 0 {
            let mut targets = residents.clone();
            targets.sort_by_key(|&at| Self::distance(at, crew.tile));
            for target in targets {
                if matches!(self.action(id, target), Some(Action::Load)) {
                    return Some(target);
                }
                if let Some(destination) = self.approach(id, target, 1) {
                    return Some(destination);
                }
            }
        }
        if crew.water == 0 && self.fire_count() > 0 {
            let mut sources: Vec<_> = (0..CELLS)
                .filter(|&at| matches!(self.tiles[at].terrain, Terrain::Water | Terrain::Base))
                .collect();
            sources.sort_by_key(|&at| Self::distance(at, crew.tile));
            for at in sources {
                if matches!(self.action(id, at), Some(Action::Refill)) {
                    return Some(at);
                }
                if let Some(destination) = self.approach(id, at, 1) {
                    return Some(destination);
                }
            }
        }
        if crew.water > 0 {
            let mut fires: Vec<_> = (0..CELLS).filter(|&at| self.tiles[at].fire > 0).collect();
            fires.sort_by_key(|&at| {
                (
                    Self::distance(crew.tile, at),
                    residents
                        .iter()
                        .map(|&v| Self::distance(at, v))
                        .min()
                        .unwrap_or(20),
                )
            });
            for at in fires {
                if matches!(self.action(id, at), Some(Action::Hose)) {
                    return Some(at);
                }
                if crew.ap >= 1
                    && let Some(destination) = self.approach(id, at, Self::range(id))
                {
                    return Some(destination);
                }
            }
        }
        None
    }
    /// Generate ordinary controls for a reproducible demonstration. This planner
    /// is read-only and has the same costs, hazards and retry rules as a person.
    pub fn pilot(&self, tick: u64) -> Input {
        if !tick.is_multiple_of(10) {
            return Input::default();
        }
        match self.phase {
            Phase::Briefing | Phase::Setback => {
                return Input {
                    primary: true,
                    ..Input::default()
                };
            }
            Phase::Debrief => {
                for id in [1, 0, 2] {
                    if self.upgrades[id] < 2 && self.cash >= self.upgrade_cost(id) + 24 {
                        return Input {
                            slot: Some(id + 1),
                            ..Input::default()
                        };
                    }
                }
                return Input {
                    primary: true,
                    ..Input::default()
                };
            }
            Phase::Ended => return Input::default(),
            Phase::Active => {}
        }
        if self.drops > 0 && self.cash >= 12 {
            let forecast = self.forecast();
            let mut best = None;
            for at in 0..CELLS {
                let count = std::iter::once(at)
                    .chain(Self::adjacent(at))
                    .filter(|&p| self.tiles[p].fire > 0)
                    .count();
                let threat = Self::adjacent(at).any(|p| self.tiles[p].people > 0 && forecast[p]);
                if (count >= 3 || (count > 0 && threat)) && best.is_none_or(|(_, n)| count > n) {
                    best = Some((at, count));
                }
            }
            if let Some((at, _)) = best {
                if self.cursor != at {
                    return Self::command(at);
                }
                return Input {
                    slot: Some(4),
                    ..Input::default()
                };
            }
        }
        for id in [2, 0, 1] {
            if let Some(target) = self.crew_plan(id) {
                if self.selected != id {
                    return Input {
                        slot: Some(id + 1),
                        ..Input::default()
                    };
                }
                return Self::command(target);
            }
        }
        Input {
            tertiary: true,
            ..Input::default()
        }
    }
    /// One plain sentence on the career so far, for status readers.
    pub fn summary(&self) -> String {
        let medals: u32 = self.medals.iter().map(|&m| u32::from(m)).sum();
        if self.won {
            format!(
                "Dawn: all {} residents home with {} medals and {} points.",
                self.total_rescued, medals, self.points
            )
        } else {
            let reason = match self.setback {
                Some(Setback::Village(_)) => "an occupied village burned",
                Some(Setback::Trapped(_)) => "a loaded crew was trapped",
                Some(Setback::Nightfall) => "nightfall came first",
                None => "the watch continues",
            };
            format!(
                "{} of 6 incidents cleared and {} residents home for {} points; {}.",
                self.medals.iter().filter(|&&m| m > 0).count(),
                self.rescued(),
                self.points,
                reason
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_arcade::vec2;
    fn fixture() -> Game {
        let mut game = Game::new();
        game.phase = Phase::Active;
        game.tiles = [Tile::from_char(b'='); CELLS];
        game.tiles[BASE].terrain = Terrain::Base;
        game
    }
    fn primary() -> Input {
        Input {
            primary: true,
            ..Input::default()
        }
    }
    fn end_turn() -> Input {
        Input {
            tertiary: true,
            ..Input::default()
        }
    }
    fn town(game: &mut Game, at: usize, people: u8) {
        game.tiles[at] = Tile {
            terrain: Terrain::Town,
            people,
            ..Tile::from_char(b'f')
        };
    }
    #[test]
    fn legal_career_evacuates_all_fifty_nine_through_six_incidents() {
        let mut game = Game::new();
        let mut replay = Game::new();
        let mut seen = [false; 6];
        let mut finish = None;
        for tick in 0..30_000 {
            seen[game.index] = true;
            let input = game.pilot(tick);
            game.step(&input);
            replay.step(&input);
            assert_eq!(game, replay, "replay tick {tick}");
            if game.outcome() != Outcome::Playing {
                finish = Some(tick);
                break;
            }
        }
        assert_eq!(
            game.outcome(),
            Outcome::Won,
            "incident {} turn {} crews {:?} setback {:?}",
            game.mission(),
            game.turn,
            game.crews,
            game.setback
        );
        assert_eq!(game.rescued(), 59);
        assert!(seen.into_iter().all(|v| v));
        assert!(game.medals.into_iter().all(|m| m > 0));
        assert_eq!(game.retries, 3);
        eprintln!(
            "Legal victory tick {}, score {}, medals {:?}, upgrades {:?}",
            finish.unwrap(),
            game.score(),
            game.medals,
            game.upgrades
        );
        let frozen = game.clone();
        for slot in 1..=6 {
            game.step(&Input {
                primary: true,
                secondary: true,
                tertiary: true,
                click: true,
                slot: Some(slot),
                ..Input::default()
            });
        }
        assert_eq!(game, frozen);
    }
    #[test]
    fn authored_maps_have_unique_reachable_villages_and_finite_content() {
        assert_eq!(
            MISSIONS
                .iter()
                .flat_map(|m| m.villages)
                .map(|v| u32::from(v.people))
                .sum::<u32>(),
            59
        );
        for (index, mission) in MISSIONS.iter().enumerate() {
            let mut game = Game::new();
            game.index = index;
            game.load();
            let mut seen = [false; CELLS];
            for village in mission.villages {
                assert!(village.x < COLS && village.y < ROWS);
                let at = village.y * COLS + village.x;
                assert!(!seen[at]);
                seen[at] = true;
                assert!(
                    std::iter::once(at)
                        .chain(Game::adjacent(at))
                        .any(|target| game.route(BASE, target).is_some()),
                    "{} inaccessible",
                    village.name
                );
            }
            assert!(game.fire_count() >= 2);
            assert_eq!(game.tiles[BASE].fuel, 0);
            assert!(game.drops <= 4);
        }
    }
    #[test]
    fn idle_ticks_held_directions_and_invalid_slots_do_not_advance_weather() {
        let mut game = Game::new();
        let before = game.clone();
        game.step(&Input::default());
        assert_eq!(game, before);
        game.step(&primary());
        let before = game.clone();
        for _ in 0..200 {
            game.step(&Input {
                left: true,
                right: true,
                primary_down: true,
                ..Input::default()
            });
        }
        game.step(&Input {
            slot: Some(99),
            ..Input::default()
        });
        assert_eq!(game, before);
    }
    #[test]
    fn forecast_is_simultaneous_and_wind_drives_first_ignition() {
        let mut game = fixture();
        let source = 3 * COLS + 5;
        let west = source - 1;
        let east = source + 1;
        for at in [source, west, west - 1, east] {
            game.tiles[at] = Tile::from_char(b'f');
        }
        game.tiles[source].fire = 2;
        let forecast = game.forecast();
        assert!(forecast[west]);
        assert!(!forecast[east]);
        assert!(!forecast[west - 1]);
        let previous = game.tiles;
        game.step(&end_turn());
        for at in 0..CELLS {
            if previous[at].fire == 0 {
                assert_eq!(game.tiles[at].fire > 0, forecast[at], "cell {at}");
            }
        }
        assert_eq!(game.tiles[source].fire, 3);
        assert_eq!(game.tiles[source].fuel, 3);
        assert_eq!(game.tiles[west].fire, 1);
        assert_eq!(game.tiles[west - 1].fire, 0);
        assert_eq!(game.turn, 2);
    }
    #[test]
    fn wet_ground_blocks_three_spreads_and_nonfuel_terrain_stays_safe() {
        let mut game = fixture();
        let source = 3 * COLS + 5;
        let west = source - 1;
        game.tiles[source] = Tile {
            fire: 3,
            fuel: 4,
            ..Tile::from_char(b'f')
        };
        game.tiles[west] = Tile {
            wet: 3,
            ..Tile::from_char(b'f')
        };
        for expected in [2, 1, 0] {
            assert!(!game.forecast()[west]);
            game.weather();
            assert_eq!(game.tiles[west].wet, expected);
            assert_eq!(game.tiles[west].fire, 0);
        }
        for at in Game::adjacent(source) {
            if at != west {
                assert_eq!(game.tiles[at].fire, 0);
            }
        }
        assert!(game.forecast()[west]);
    }
    #[test]
    fn hose_has_role_range_ap_water_and_wetness_costs() {
        let mut game = fixture();
        let target = BASE - 2 * COLS;
        game.tiles[target] = Tile {
            fire: 3,
            ..Tile::from_char(b'f')
        };
        game.cursor = target;
        game.selected = 1;
        let unchanged = game.clone();
        assert_eq!(game.step(&primary()), Cue::None);
        assert_eq!(game, unchanged);
        game.selected = 0;
        assert_eq!(game.step(&primary()), Cue::Good);
        assert_eq!((game.tiles[target].fire, game.tiles[target].wet), (0, 3));
        assert_eq!((game.crews[0].ap, game.crews[0].water), (2, 5));
        game.tiles[target].fire = 2;
        game.crews[0].water = 0;
        let unchanged = game.clone();
        assert_eq!(game.step(&primary()), Cue::None);
        assert_eq!(game, unchanged);
    }
    #[test]
    fn refill_requires_adjacency_and_only_pays_for_a_nonfull_tank() {
        let mut game = fixture();
        let water = BASE + 1;
        game.tiles[water] = Tile::from_char(b'~');
        game.cursor = water;
        game.crews[0].water = 1;
        game.step(&primary());
        assert_eq!((game.crews[0].water, game.crews[0].ap), (6, 3));
        let unchanged = game.clone();
        assert_eq!(game.step(&primary()), Cue::None);
        assert_eq!(game, unchanged);
        game.cursor = water - 2 * COLS;
        game.tiles[game.cursor] = Tile::from_char(b'~');
        game.crews[0].water = 0;
        let unchanged = game.clone();
        game.step(&primary());
        assert_eq!(game, unchanged);
    }
    #[test]
    fn ranger_break_is_permanent_and_repeated_or_burning_cuts_are_rejected() {
        let mut game = fixture();
        game.selected = 1;
        let target = BASE + 1;
        game.tiles[target] = Tile::from_char(b'f');
        game.cursor = target;
        assert_eq!(
            game.step(&Input {
                secondary: true,
                ..Input::default()
            }),
            Cue::Good
        );
        assert_eq!((game.crews[1].ap, game.tiles[target].fuel), (3, 0));
        let unchanged = game.clone();
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game, unchanged);
        game.tiles[target] = Tile {
            fire: 1,
            ..Tile::from_char(b'f')
        };
        let unchanged = game.clone();
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game, unchanged);
        town(&mut game, target, 3);
        let unchanged = game.clone();
        game.cut();
        assert_eq!(game, unchanged);
    }
    #[test]
    fn movement_uses_weighted_existing_navigation_and_never_overspends_ap() {
        let mut game = fixture();
        game.tiles[BASE - COLS] = Tile::from_char(b'f');
        game.cursor = BASE - 2 * COLS;
        game.step(&primary());
        assert_eq!((game.crews[0].tile, game.crews[0].ap), (BASE - 2 * COLS, 1));
        game.cursor = BASE + 4;
        let unchanged = game.clone();
        game.step(&primary());
        assert_eq!(game, unchanged);
        game.tiles[game.cursor].fire = 2;
        game.crews[0].ap = 6;
        let unchanged = game.clone();
        game.step(&primary());
        assert_eq!(game, unchanged);
    }
    #[test]
    fn loading_is_capacity_bounded_and_only_base_delivery_counts_as_rescue() {
        let mut game = fixture();
        town(&mut game, BASE + 1, 4);
        game.cursor = BASE + 1;
        game.step(&primary());
        assert_eq!(
            (
                game.crews[0].cargo,
                game.tiles[BASE + 1].people,
                game.crews[0].ap
            ),
            (2, 2, 2)
        );
        assert_eq!(game.rescued(), 0);
        let unchanged = game.clone();
        game.step(&primary());
        assert_eq!(game, unchanged);
        game.cursor = BASE;
        game.step(&primary());
        assert_eq!(
            (game.rescued(), game.crews[0].cargo, game.crews[0].ap),
            (2, 0, 1)
        );
        assert_eq!(game.score(), 0);
        game.crews[0].water = 6;
        let unchanged = game.clone();
        game.step(&primary());
        assert_eq!(game, unchanged);
    }
    #[test]
    fn air_drop_uses_a_cross_and_rejects_empty_or_unaffordable_requests() {
        let mut game = fixture();
        game.cursor = 3 * COLS + 5;
        for at in std::iter::once(game.cursor).chain(Game::adjacent(game.cursor)) {
            game.tiles[at] = Tile {
                fire: 3,
                ..Tile::from_char(b'f')
            };
        }
        let diagonal = game.cursor + COLS + 1;
        game.tiles[diagonal] = Tile {
            fire: 3,
            ..Tile::from_char(b'f')
        };
        game.step(&Input {
            slot: Some(4),
            ..Input::default()
        });
        assert_eq!((game.cash, game.drops, game.used_drops), (38, 1, 1));
        assert_eq!(game.tiles[diagonal].fire, 3);
        for at in std::iter::once(game.cursor).chain(Game::adjacent(game.cursor)) {
            assert_eq!((game.tiles[at].fire, game.tiles[at].wet), (0, 3));
        }
        let unchanged = game.clone();
        game.step(&Input {
            slot: Some(4),
            ..Input::default()
        });
        assert_eq!(game, unchanged);
        game.cursor = diagonal;
        game.cash = 11;
        let unchanged = game.clone();
        game.drop_water();
        assert_eq!(game, unchanged);
    }
    #[test]
    fn occupied_villages_and_loaded_crews_fail_but_empty_crews_retreat() {
        let source = 3 * COLS + 5;
        let west = source - 1;
        let mut game = fixture();
        game.tiles[source] = Tile {
            fire: 2,
            ..Tile::from_char(b'f')
        };
        town(&mut game, west, 3);
        game.weather();
        assert_eq!(game.phase, Phase::Setback);
        let mut empty = fixture();
        empty.tiles[source] = Tile {
            fire: 2,
            ..Tile::from_char(b'f')
        };
        town(&mut empty, west, 0);
        empty.crews[0].tile = west;
        empty.weather();
        assert_eq!(empty.phase, Phase::Active);
        assert!(empty.tiles[west].ruined);
        assert_eq!(
            (empty.crews[0].tile, empty.crews[0].water, empty.retreats),
            (BASE, 0, 1)
        );
        let mut loaded = fixture();
        loaded.tiles[source] = Tile {
            fire: 2,
            ..Tile::from_char(b'f')
        };
        loaded.tiles[west] = Tile::from_char(b'f');
        loaded.crews[0].tile = west;
        loaded.crews[0].cargo = 1;
        loaded.weather();
        assert_eq!(loaded.phase, Phase::Setback);
    }
    #[test]
    fn retry_restores_starting_resources_without_farming_rewards() {
        let mut game = Game::new();
        game.step(&primary());
        game.cash -= 24;
        game.landed = 3;
        game.turn = MISSIONS[0].limit;
        game.weather();
        assert_eq!(game.phase, Phase::Setback);
        game.step(&primary());
        assert_eq!(game.phase, Phase::Briefing);
        assert_eq!(
            (
                game.cash,
                game.retries,
                game.landed,
                game.score(),
                game.rescued()
            ),
            (50, 2, 0, 0, 0)
        );
        assert_eq!(game.fire_count(), 2);
    }
    #[test]
    fn four_real_failed_attempts_end_the_career_and_terminal_input_is_inert() {
        let mut game = Game::new();
        for attempt in 0..4 {
            assert_eq!(game.phase, Phase::Briefing);
            game.step(&primary());
            for _ in 0..30 {
                if game.phase == Phase::Active {
                    game.step(&end_turn());
                }
            }
            if attempt < 3 {
                assert_eq!(game.phase, Phase::Setback);
                game.step(&primary());
            }
        }
        assert_eq!(game.outcome(), Outcome::Lost);
        let frozen = game.clone();
        for _ in 0..100 {
            game.step(&primary());
            game.step(&end_turn());
        }
        assert_eq!(game, frozen);
    }
    #[test]
    fn upgrades_have_atomic_costs_and_apply_to_the_next_dispatch() {
        let mut game = fixture();
        game.phase = Phase::Debrief;
        game.cash = 200;
        for _ in 0..2 {
            game.step(&Input {
                slot: Some(2),
                ..Input::default()
            });
        }
        assert_eq!((game.upgrades[1], game.cash), (2, 115));
        let unchanged = game.clone();
        game.step(&Input {
            slot: Some(2),
            ..Input::default()
        });
        assert_eq!(game, unchanged);
        game.step(&primary());
        assert_eq!(game.mission(), 2);
        assert_eq!(game.crews[0].ap, 6);
        assert_eq!(game.start_cash, 115);
    }
    #[test]
    fn pointer_bounds_and_sidebar_actions_match_the_rendered_geometry() {
        let mut game = fixture();
        for point in [
            vec2(578., 200.),
            vec2(28., 530.),
            vec2(f32::NAN, 200.),
            vec2(-10., 180.),
        ] {
            let old = game.clone();
            game.step(&Input {
                click: true,
                pointer: point,
                ..Input::default()
            });
            assert_eq!(game, old);
        }
        game.step(&Input {
            click: true,
            pointer: vec2(650., 270.),
            ..Input::default()
        });
        assert_eq!(game.selected, 2);
        game.step(&Game::command(BASE + 1));
        assert_eq!((game.cursor, game.crews[2].tile), (BASE + 1, BASE + 1));
        game.step(&Input {
            click: true,
            pointer: vec2(END.x + 40., END.y + END.h * 0.5),
            ..Input::default()
        });
        assert_eq!(game.turn, 2);
    }
    fn click(rect: ferravex_arcade::Rect) -> Input {
        Input {
            click: true,
            pointer: vec2(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5),
            ..Input::default()
        }
    }
    #[test]
    fn drawn_deploy_retry_next_and_forecast_buttons_mirror_their_keys() {
        use crate::layout::{PROCEED, shop_card};
        // Deploy: a click beside the button changes nothing; on it, it deploys.
        let mut game = Game::new();
        let before = game.clone();
        game.step(&Input {
            click: true,
            pointer: vec2(DEPLOY.x - 4., DEPLOY.y + 4.),
            ..Input::default()
        });
        assert_eq!(game, before);
        let mut keyed = Game::new();
        assert_eq!(game.step(&click(DEPLOY)), keyed.step(&primary()));
        assert_eq!(game, keyed);
        assert_eq!(game.phase, Phase::Active);
        // Forecast switch: the same toggle as key 5, both ways.
        assert_eq!(game.step(&click(FORECAST)), Cue::Action);
        assert!(!game.show_forecast);
        keyed.step(&Input {
            slot: Some(5),
            ..Input::default()
        });
        assert_eq!(game, keyed);
        game.step(&click(FORECAST));
        assert!(game.show_forecast);
        // Retry: the drawn button spends a retry exactly like Space.
        let mut failed = Game::new();
        failed.step(&primary());
        failed.turn = MISSIONS[0].limit;
        failed.weather();
        assert_eq!(failed.phase, Phase::Setback);
        let mut keyed = failed.clone();
        let unchanged = failed.clone();
        failed.step(&click(PROCEED));
        assert_eq!(failed, unchanged, "only the retry button retries");
        failed.step(&click(RETRY));
        keyed.step(&primary());
        assert_eq!(failed, keyed);
        assert_eq!((failed.phase, failed.retries), (Phase::Briefing, 2));
        // Next: the debrief button advances exactly like Space; cards still buy.
        let mut debrief = fixture();
        debrief.phase = Phase::Debrief;
        debrief.cash = 100;
        let mut keyed = debrief.clone();
        debrief.step(&click(shop_card(0)));
        assert_eq!((debrief.upgrades[0], debrief.cash), (1, 70));
        keyed.step(&Input {
            slot: Some(1),
            ..Input::default()
        });
        debrief.step(&click(PROCEED));
        keyed.step(&primary());
        assert_eq!(debrief, keyed);
        assert_eq!((debrief.mission(), debrief.phase), (2, Phase::Briefing));
    }
    #[test]
    fn the_feed_records_each_step_and_never_affects_equality() {
        let mut game = fixture();
        let fire = BASE - 2 * COLS;
        game.tiles[fire] = Tile {
            fire: 2,
            fuel: 4,
            ..Tile::from_char(b'f')
        };
        game.cursor = fire;
        let quiet = game.clone();
        assert_eq!(game.step(&primary()), Cue::Good);
        let events: Vec<_> = game.feed.iter().collect();
        assert_eq!(
            events,
            [Event::Sprayed {
                crew: 0,
                at: fire,
                removed: 2,
                out: true
            }]
        );
        assert_ne!(game, quiet, "the spray changed rule state");
        // An idle step clears the record without touching rule state.
        let sprayed = game.clone();
        game.step(&Input::default());
        assert_eq!(game.feed.iter().count(), 0);
        assert_eq!(game, sprayed);
        // A refusal is recorded with what was missing, and changes nothing.
        game.tiles[fire].fire = 1;
        game.crews[0].water = 0;
        let unchanged = game.clone();
        assert_eq!(game.step(&primary()), Cue::None);
        assert_eq!(game, unchanged);
        assert_eq!(
            game.feed.iter().collect::<Vec<_>>(),
            [Event::Refused {
                spot: Spot::Tile(fire),
                lack: Lack::Water
            }]
        );
        // Clicking a distant fire only aims; Space there reports the range.
        let far = 2 * COLS + 9;
        game.tiles[far] = Tile {
            fire: 2,
            fuel: 4,
            ..Tile::from_char(b'f')
        };
        assert_eq!(game.step(&Game::command(far)), Cue::Action);
        assert_eq!(game.cursor, far);
        assert_eq!(game.feed.iter().count(), 0);
        game.step(&primary());
        assert_eq!(
            game.feed.iter().collect::<Vec<_>>(),
            [Event::Refused {
                spot: Spot::Tile(far),
                lack: Lack::Range
            }]
        );
        // A spread reports exactly the forecast ignitions.
        let forecast = game.forecast();
        game.step(&end_turn());
        let lit = game
            .feed
            .iter()
            .find_map(|e| match e {
                Event::Spread { lit, .. } => Some(lit),
                _ => None,
            })
            .expect("spread event");
        for (at, &warned) in forecast.iter().enumerate() {
            assert_eq!(lit & (1 << at) != 0, warned, "cell {at}");
        }
    }
    #[test]
    fn reach_and_refusals_agree_with_the_rules_across_a_whole_career() {
        let mut game = Game::new();
        let mut checked = 0;
        for tick in 0..30_000u64 {
            if game.outcome() != Outcome::Playing {
                break;
            }
            if game.phase == Phase::Active && tick % 97 == 0 {
                for id in 0..3 {
                    let reach = game.reach(id);
                    let crew = game.crews[id];
                    for (at, &least) in reach.iter().enumerate() {
                        let action = game.action(id, at);
                        let lack = game.lack(id, at);
                        assert!(
                            !(action.is_some() && lack.is_some()),
                            "tick {tick} cell {at}"
                        );
                        if let Some(Action::Move(path, cost)) = &action {
                            assert_eq!(least, *cost, "tick {tick} crew {id} cell {at}");
                            assert_eq!(path.last(), Some(&at));
                        }
                        let tile = game.tiles[at];
                        let plain = tile.people == 0
                            && !matches!(tile.terrain, Terrain::Base | Terrain::Water);
                        if plain && at != crew.tile && tile.walkable() {
                            let affordable = least <= crew.ap;
                            assert_eq!(
                                matches!(action, Some(Action::Move(..))),
                                affordable,
                                "tick {tick} crew {id} cell {at}"
                            );
                            if least != u8::MAX && !affordable {
                                assert_eq!(lack, Some(Lack::Ap(least)));
                            }
                        }
                        if tile.fire > 0 && Game::distance(crew.tile, at) > Game::range(id) {
                            assert_eq!(lack, Some(Lack::Range));
                        }
                    }
                }
                checked += 1;
            }
            let input = game.pilot(tick);
            game.step(&input);
        }
        assert_eq!(game.outcome(), Outcome::Won);
        assert!(checked > 20, "{checked} sampled states");
    }
    #[test]
    fn planner_does_not_treat_an_unaffordable_load_as_a_one_ap_move() {
        let mut game = fixture();
        town(&mut game, BASE + 1, 2);
        town(&mut game, BASE + 2, 2);
        game.crews[2].ap = 1;
        assert!(game.approach(2, BASE + 2, 1).is_none());
    }
}
