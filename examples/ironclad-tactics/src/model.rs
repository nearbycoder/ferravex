//! Renderer-independent AP tactics, telegraphed enemy turns and campaign rules.
//!
//! ```
//! use ferravex_arcade::{Input, Outcome};
//! use ironclad_tactics::Game;
//! let mut game = Game::new();
//! assert_eq!(game.mission(), 1);
//! game.step(&Input { primary: true, ..Input::default() }); // Deploy.
//! assert_eq!(game.round(), 1);
//! game.step(&Input { tertiary: true, ..Input::default() }); // Enemy phase.
//! assert_eq!(game.round(), 2);
//! assert_eq!(game.outcome(), Outcome::Playing);
//! ```
use crate::content::{CELLS, COLS, MAX_ENEMIES, MISSIONS, Mission, Objective, ROWS};
use ferravex_arcade::{Cue, Input, Outcome};
use ferravex_math::Vec2;
use ferravex_play::navigation::{Cell, Connectivity, Grid};

pub(crate) const BOARD_X: f32 = 28.;
pub(crate) const BOARD_Y: f32 = 154.;
pub(crate) const TILE_SIZE: f32 = 50.;
pub(crate) const CARD_Y: f32 = 160.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Pos {
    pub x: i32,
    pub y: i32,
}
impl Pos {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
    pub fn index(self) -> usize {
        self.y as usize * COLS + self.x as usize
    }
    pub fn inside(self) -> bool {
        self.x >= 0 && self.y >= 0 && self.x < COLS as i32 && self.y < ROWS as i32
    }
    pub fn distance(self, other: Self) -> u16 {
        (self.x.abs_diff(other.x) + self.y.abs_diff(other.y)) as u16
    }
    pub fn center(self) -> Vec2 {
        Vec2::new(
            BOARD_X + (self.x as f32 + 0.5) * TILE_SIZE,
            BOARD_Y + (self.y as f32 + 0.5) * TILE_SIZE,
        )
    }
    fn cell(self) -> Cell {
        Cell {
            x: self.x as u32,
            y: self.y as u32,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Tile {
    #[default]
    Floor,
    Wall,
    Cover,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Role {
    Vanguard,
    Scout,
    Engineer,
}
impl Role {
    pub fn name(self) -> &'static str {
        match self {
            Self::Vanguard => "BRASS",
            Self::Scout => "KESTREL",
            Self::Engineer => "MICA",
        }
    }
    pub fn class(self) -> &'static str {
        match self {
            Self::Vanguard => "Vanguard",
            Self::Scout => "Scout",
            Self::Engineer => "Engineer",
        }
    }
    pub fn hull(self) -> u8 {
        match self {
            Self::Vanguard => 18,
            Self::Scout => 14,
            Self::Engineer => 15,
        }
    }
    pub fn ap(self) -> u8 {
        match self {
            Self::Scout => 5,
            _ => 4,
        }
    }
    pub fn damage(self) -> u8 {
        match self {
            Self::Vanguard => 4,
            _ => 3,
        }
    }
    pub fn range(self) -> u16 {
        match self {
            Self::Scout => 6,
            _ => 4,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Unit {
    pub role: Role,
    pub pos: Pos,
    pub hp: u8,
    pub ap: u8,
    pub braced: bool,
    pub focused: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Gunner,
    Bruiser,
    Sentry,
    Warden,
}
impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Gunner => "Patrol",
            Self::Bruiser => "Rammer",
            Self::Sentry => "Sentry",
            Self::Warden => "Warden",
        }
    }
    pub(crate) fn hull(self) -> u8 {
        match self {
            Self::Gunner => 6,
            Self::Bruiser => 8,
            Self::Sentry => 7,
            Self::Warden => 18,
        }
    }
    fn range(self) -> u16 {
        match self {
            Self::Gunner => 5,
            Self::Bruiser => 1,
            Self::Sentry => 6,
            Self::Warden => 5,
        }
    }
    pub(crate) fn damage(self) -> u8 {
        match self {
            Self::Bruiser | Self::Warden => 4,
            _ => 3,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Intent {
    #[default]
    Wait,
    Move(Pos),
    Fire(Pos),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Enemy {
    pub pos: Pos,
    pub hp: u8,
    pub kind: Kind,
    pub intent: Intent,
    pub arrives: u16,
    pub active: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Briefing,
    Battle,
    Debrief,
    Setback,
    Won,
    Lost,
}

/// A bounded six-mission campaign. Only [`Game::step`] commits player commands.
/// See the module example for headless deployment and an enemy turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub(crate) mission_index: usize,
    pub(crate) tiles: [Tile; CELLS],
    pub(crate) units: [Unit; 3],
    pub(crate) enemies: [Option<Enemy>; MAX_ENEMIES],
    pub(crate) objectives: [Option<Pos>; 2],
    pub(crate) extraction: Pos,
    pub(crate) completed: [bool; 2],
    pub(crate) carrier: Option<usize>,
    pub(crate) held: u8,
    pub(crate) round: u16,
    pub(crate) selected: usize,
    pub(crate) cursor: Pos,
    pub(crate) phase: Phase,
    pub(crate) upgrades: [u8; 3],
    pub(crate) kits: u8,
    pub(crate) retries: u8,
    pub(crate) score: u32,
    checkpoint: u32,
    pub(crate) medals: [u8; 6],
    pub(crate) notes: [String; 2],
    pub(crate) shot: Option<(Pos, Pos, bool)>,
    pub(crate) flash: u8,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Start at the first operation's briefing, with three mission retries.
    pub fn new() -> Self {
        let unit = Unit {
            role: Role::Vanguard,
            pos: Pos::default(),
            hp: 0,
            ap: 0,
            braced: false,
            focused: false,
        };
        let mut game = Self {
            mission_index: 0,
            tiles: [Tile::Floor; CELLS],
            units: [unit; 3],
            enemies: [None; MAX_ENEMIES],
            objectives: [None; 2],
            extraction: Pos::new(1, 1),
            completed: [false; 2],
            carrier: None,
            held: 0,
            round: 1,
            selected: 0,
            cursor: Pos::default(),
            phase: Phase::Briefing,
            upgrades: [0; 3],
            kits: 2,
            retries: 3,
            score: 0,
            checkpoint: 0,
            medals: [0; 6],
            notes: std::array::from_fn(|_| String::new()),
            shot: None,
            flash: 0,
        };
        game.load();
        game
    }
    /// Current mission, one-based, always in 1..=6.
    pub fn mission(&self) -> usize {
        self.mission_index + 1
    }
    /// Current friendly turn number. Idle ticks never advance it.
    pub fn round(&self) -> u16 {
        self.round
    }
    /// Committed campaign score; retry discards the failed mission's points.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// Campaign outcome; a recoverable mission setback is still Playing.
    pub fn outcome(&self) -> Outcome {
        match self.phase {
            Phase::Won => Outcome::Won,
            Phase::Lost => Outcome::Lost,
            _ => Outcome::Playing,
        }
    }
    pub(crate) fn content(&self) -> &'static Mission {
        &MISSIONS[self.mission_index]
    }
    pub(crate) fn max_hull(&self, role: Role) -> u8 {
        role.hull() + self.upgrades[0] * 3
    }
    pub(crate) fn max_ap(&self, role: Role) -> u8 {
        role.ap() + self.upgrades[2]
    }
    fn load(&mut self) {
        self.tiles.fill(Tile::Floor);
        self.enemies.fill(None);
        self.objectives = [None; 2];
        self.completed = [false; 2];
        self.carrier = None;
        self.held = 0;
        self.round = 1;
        self.kits = 2;
        self.selected = 0;
        self.phase = Phase::Briefing;
        self.shot = None;
        self.flash = 0;
        let mut count = 0;
        for (y, row) in self.content().map.iter().enumerate() {
            assert_eq!(row.len(), COLS, "authored tactical row width");
            for (x, byte) in row.bytes().enumerate() {
                let p = Pos::new(x as i32, y as i32);
                match byte {
                    b'#' => self.tiles[p.index()] = Tile::Wall,
                    b'c' => self.tiles[p.index()] = Tile::Cover,
                    b'1'..=b'3' => {
                        let i = (byte - b'1') as usize;
                        let role = [Role::Vanguard, Role::Scout, Role::Engineer][i];
                        self.units[i] = Unit {
                            role,
                            pos: p,
                            hp: self.max_hull(role),
                            ap: self.max_ap(role),
                            braced: false,
                            focused: false,
                        };
                    }
                    b'o' => self.objectives[0] = Some(p),
                    b'p' => self.objectives[1] = Some(p),
                    b'e' => self.extraction = p,
                    b'g' | b'b' | b't' | b'W' | b'r' | b's' => {
                        let kind = match byte {
                            b'b' => Kind::Bruiser,
                            b't' => Kind::Sentry,
                            b'W' => Kind::Warden,
                            _ => Kind::Gunner,
                        };
                        let arrives = match byte {
                            b'r' => 3,
                            b's' => 5,
                            _ => 1,
                        };
                        self.enemies[count] = Some(Enemy {
                            pos: p,
                            hp: kind.hull(),
                            kind,
                            intent: Intent::Wait,
                            arrives,
                            active: arrives == 1,
                        });
                        count += 1;
                    }
                    _ => {}
                }
            }
        }
        self.cursor = self.units[0].pos;
        self.notes = [self.content().tip.to_owned(), String::new()];
        self.plan_enemies();
    }
    fn note(&mut self, msg: String) {
        self.notes[1] = std::mem::replace(&mut self.notes[0], msg);
    }
    pub(crate) fn friendly(&self, p: Pos) -> Option<usize> {
        self.units.iter().position(|u| u.hp > 0 && u.pos == p)
    }
    pub(crate) fn hostile(&self, p: Pos) -> Option<usize> {
        self.enemies
            .iter()
            .position(|e| e.is_some_and(|e| e.active && e.hp > 0 && e.pos == p))
    }
    fn occupied(&self, p: Pos) -> bool {
        self.friendly(p).is_some() || self.hostile(p).is_some()
    }
    pub(crate) fn remaining(&self) -> usize {
        self.enemies.iter().flatten().filter(|e| e.hp > 0).count()
    }
    fn walkable(&self, p: Pos) -> bool {
        p.inside() && self.tiles[p.index()] != Tile::Wall
    }
    /// Route through the framework's deterministic four-neighbor navigation.
    fn path(&self, start: Pos, end: Pos, allow_end: bool) -> Option<Vec<Pos>> {
        if !self.walkable(start) || !self.walkable(end) {
            return None;
        }
        let mut grid = Grid::new(COLS as u32, ROWS as u32).expect("bounded authored board");
        for y in 0..ROWS {
            for x in 0..COLS {
                let p = Pos::new(x as i32, y as i32);
                let blocked = !self.walkable(p)
                    || (p != start && (!allow_end || p != end) && self.occupied(p));
                grid.set_blocked(p.cell(), blocked).expect("in-bounds tile");
            }
        }
        grid.path(start.cell(), end.cell(), Connectivity::Four)
            .ok()
            .map(|path| {
                path.into_iter()
                    .map(|p| Pos::new(p.x as i32, p.y as i32))
                    .collect()
            })
    }
    pub(crate) fn cursor_path(&self) -> Option<Vec<Pos>> {
        if self.occupied(self.cursor) && self.cursor != self.units[self.selected].pos {
            return None;
        }
        self.path(self.units[self.selected].pos, self.cursor, false)
    }
    /// Conservative grid visibility: solid walls and closed diagonal corners block.
    pub(crate) fn visible(&self, a: Pos, b: Pos) -> bool {
        if !a.inside() || !b.inside() {
            return false;
        }
        let (mut x, mut y) = (a.x, a.y);
        let (nx, ny) = ((b.x - a.x).abs(), (b.y - a.y).abs());
        let (sx, sy) = ((b.x - a.x).signum(), (b.y - a.y).signum());
        let (mut ix, mut iy) = (0, 0);
        while ix < nx || iy < ny {
            let decision = (1 + 2 * ix) * ny - (1 + 2 * iy) * nx;
            if decision == 0 {
                if !self.walkable(Pos::new(x + sx, y)) || !self.walkable(Pos::new(x, y + sy)) {
                    return false;
                }
                x += sx;
                y += sy;
                ix += 1;
                iy += 1;
            } else if decision < 0 {
                x += sx;
                ix += 1;
            } else {
                y += sy;
                iy += 1;
            }
            if !self.walkable(Pos::new(x, y)) {
                return false;
            }
        }
        true
    }
    pub(crate) fn shot_damage(&self, unit: usize, enemy: usize) -> u8 {
        let u = self.units[unit];
        let e = self.enemies[enemy].unwrap();
        let raw = u.role.damage() + self.upgrades[1] + if u.focused { 2 } else { 0 };
        raw.saturating_sub(u8::from(self.tiles[e.pos.index()] == Tile::Cover))
            .max(1)
    }
    fn can_shoot(&self, unit: usize, enemy: usize) -> bool {
        let u = self.units[unit];
        let Some(e) = self.enemies[enemy] else {
            return false;
        };
        u.hp > 0
            && u.ap >= 2
            && e.active
            && e.hp > 0
            && u.pos.distance(e.pos) <= u.role.range()
            && self.visible(u.pos, e.pos)
    }
    fn shot(&mut self, enemy: usize) -> Cue {
        let i = self.selected;
        if !self.can_shoot(i, enemy) {
            self.note("Need 2 AP, weapon range and clear sight.".into());
            return Cue::None;
        }
        let damage = self.shot_damage(i, enemy);
        self.units[i].ap -= 2;
        self.units[i].focused = false;
        let target = self.enemies[enemy].as_mut().unwrap();
        target.hp = target.hp.saturating_sub(damage);
        let (pos, kind, dead) = (target.pos, target.kind, target.hp == 0);
        if dead {
            self.score += 100;
        }
        self.shot = Some((self.units[i].pos, pos, false));
        self.flash = 12;
        self.note(format!(
            "{} hits {} for {}{}. ",
            self.units[i].role.name(),
            kind.name(),
            damage,
            if dead { "; disabled" } else { "" }
        ));
        self.check_success();
        if self.phase == Phase::Won {
            Cue::Win
        } else if dead {
            Cue::Good
        } else {
            Cue::Action
        }
    }
    fn move_unit(&mut self, to: Pos) -> Cue {
        let i = self.selected;
        let u = self.units[i];
        let Some(path) = self.path(u.pos, to, false) else {
            self.note("That route is blocked.".into());
            return Cue::None;
        };
        let steps = path.len() - 1;
        if steps == 0 || steps > usize::from(u.ap) {
            self.note("Choose a clear destination within remaining AP.".into());
            return Cue::None;
        }
        self.units[i].pos = to;
        self.units[i].ap -= steps as u8;
        self.units[i].braced = false;
        self.note(format!(
            "{} moves {} tile{}.",
            u.role.name(),
            steps,
            if steps == 1 { "" } else { "s" }
        ));
        self.check_success();
        Cue::Action
    }
    fn interact(&mut self, index: usize) -> Cue {
        let i = self.selected;
        let u = self.units[i];
        let target = self.objectives[index].unwrap();
        if u.pos.distance(target) > 1 {
            return self.move_unit(target);
        }
        if self.completed[index] {
            self.note("This objective is already secured.".into());
            return Cue::None;
        }
        if u.ap < 2 {
            self.note("Interacting needs 2 AP. End the turn to replenish.".into());
            return Cue::None;
        }
        self.units[i].ap -= 2;
        self.completed[index] = true;
        if self.content().objective == Objective::Rescue {
            self.carrier = Some(i);
            self.note(format!("Operator rescued. Escort {} to E.", u.role.name()));
        } else {
            self.note("Objective secured. Keep the squad alive.".into());
        }
        self.check_success();
        Cue::Good
    }
    fn context(&mut self) -> Cue {
        if let Some(enemy) = self.hostile(self.cursor) {
            return self.shot(enemy);
        }
        if let Some(friend) = self.friendly(self.cursor)
            && friend != self.selected
        {
            self.selected = friend;
            self.note(format!("{} selected.", self.units[friend].role.name()));
            return Cue::Action;
        }
        if let Some(index) = self.objectives.iter().position(|p| *p == Some(self.cursor))
            && !self.completed[index]
        {
            return self.interact(index);
        }
        self.move_unit(self.cursor)
    }
    fn special(&mut self) -> Cue {
        let i = self.selected;
        let u = self.units[i];
        match u.role {
            Role::Vanguard => {
                if u.ap == 0 || u.braced {
                    return Cue::None;
                }
                self.units[i].ap -= 1;
                self.units[i].braced = true;
                self.note("Braced: reduce incoming damage by 2 until next turn.".into());
            }
            Role::Scout => {
                if u.ap == 0 || u.focused {
                    return Cue::None;
                }
                self.units[i].ap -= 1;
                self.units[i].focused = true;
                self.note("Focused: next cannon shot deals +2 damage.".into());
            }
            Role::Engineer => {
                let target = self.friendly(self.cursor).unwrap_or(i);
                let friend = self.units[target];
                if u.ap < 2
                    || self.kits == 0
                    || u.pos.distance(friend.pos) > 2
                    || friend.hp == self.max_hull(friend.role)
                {
                    self.note(
                        "Patch needs 2 AP, one kit and a wounded ally within 2 tiles.".into(),
                    );
                    return Cue::None;
                }
                self.units[i].ap -= 2;
                self.kits -= 1;
                self.units[target].hp = (friend.hp + 6).min(self.max_hull(friend.role));
                self.note(format!(
                    "MICA patches {}. {} kits remain.",
                    friend.role.name(),
                    self.kits
                ));
            }
        }
        Cue::Good
    }
    fn plan_enemies(&mut self) {
        for index in 0..MAX_ENEMIES {
            let Some(e) = self.enemies[index] else {
                continue;
            };
            if e.hp == 0 || !e.active {
                continue;
            }
            let target = self
                .units
                .iter()
                .enumerate()
                .filter(|(_, u)| u.hp > 0)
                .min_by_key(|(i, u)| (e.pos.distance(u.pos), *i))
                .map(|(_, u)| u.pos);
            let intent = if let Some(to) = target {
                if e.pos.distance(to) <= e.kind.range() && self.visible(e.pos, to) {
                    Intent::Fire(to)
                } else if e.kind == Kind::Sentry {
                    Intent::Wait
                } else {
                    self.path(e.pos, to, true)
                        .and_then(|path| path.get(1).copied())
                        .map_or(Intent::Wait, Intent::Move)
                }
            } else {
                Intent::Wait
            };
            self.enemies[index].as_mut().unwrap().intent = intent;
        }
    }
    fn enemy_turn(&mut self) -> Cue {
        let mut hits = 0;
        for index in 0..MAX_ENEMIES {
            let Some(e) = self.enemies[index] else {
                continue;
            };
            if e.hp == 0 || !e.active {
                continue;
            }
            match e.intent {
                Intent::Wait => {}
                Intent::Move(to) => {
                    if self.walkable(to) && !self.occupied(to) {
                        self.enemies[index].as_mut().unwrap().pos = to;
                    }
                }
                Intent::Fire(to) => {
                    if let Some(i) = self.friendly(to)
                        && self.visible(e.pos, to)
                    {
                        let mitigation = u8::from(self.tiles[to.index()] == Tile::Cover)
                            + if self.units[i].braced { 2 } else { 0 };
                        let damage = e.kind.damage().saturating_sub(mitigation).max(1);
                        self.units[i].hp = self.units[i].hp.saturating_sub(damage);
                        hits += 1;
                        self.shot = Some((e.pos, to, true));
                        self.flash = 18;
                        if self.units[i].hp == 0 {
                            self.note(format!("{} is disabled.", self.units[i].role.name()));
                        }
                    }
                }
            }
        }
        if self.units.iter().all(|u| u.hp == 0)
            || self.carrier.is_some_and(|i| self.units[i].hp == 0)
        {
            self.fail("Squad disabled or the escorted operator was lost.");
            return Cue::Bad;
        }
        if matches!(
            self.content().objective,
            Objective::Capture | Objective::Hold
        ) && self.completed[0]
        {
            let objective = self.objectives[0].unwrap();
            let guarded = self
                .units
                .iter()
                .any(|u| u.hp > 0 && u.pos.distance(objective) <= 1);
            let contested = self
                .enemies
                .iter()
                .flatten()
                .any(|e| e.active && e.hp > 0 && e.pos.distance(objective) <= 1);
            self.held = if guarded && !contested {
                self.held + 1
            } else {
                0
            };
        }
        self.round += 1;
        self.check_success();
        if self.phase != Phase::Battle {
            return Cue::Win;
        }
        if self.round > self.content().limit {
            self.fail("The operation window closed before the objective was complete.");
            return Cue::Bad;
        }
        for i in 0..3 {
            self.units[i].ap = if self.units[i].hp > 0 {
                self.max_ap(self.units[i].role)
            } else {
                0
            };
            self.units[i].braced = false;
        }
        for i in 0..MAX_ENEMIES {
            if let Some(e) = self.enemies[i]
                && e.hp > 0
                && !e.active
                && e.arrives <= self.round
            {
                // An occupied entry delays neither the wave nor the objective.
                // Reserves take the nearest empty floor and telegraph before firing.
                let entry = (0..CELLS)
                    .map(|n| Pos::new((n % COLS) as i32, (n / COLS) as i32))
                    .filter(|p| self.walkable(*p) && !self.occupied(*p))
                    .min_by_key(|p| (p.distance(e.pos), p.index()));
                if let Some(pos) = entry {
                    let enemy = self.enemies[i].as_mut().unwrap();
                    enemy.pos = pos;
                    enemy.active = true;
                    self.note("Reserve patrol has arrived near a marked entry.".into());
                }
            }
        }
        if self.units[self.selected].hp == 0 {
            self.selected = self.units.iter().position(|u| u.hp > 0).unwrap();
            self.cursor = self.units[self.selected].pos;
        }
        self.plan_enemies();
        self.note(format!(
            "Turn {}. {} enemy hit{}. Fresh AP ready.",
            self.round,
            hits,
            if hits == 1 { "" } else { "s" }
        ));
        if hits > 0 { Cue::Bad } else { Cue::Action }
    }
    fn fail(&mut self, reason: &str) {
        self.phase = if self.retries > 0 {
            Phase::Setback
        } else {
            Phase::Lost
        };
        self.note(reason.to_owned());
    }
    fn check_success(&mut self) {
        if self.phase != Phase::Battle {
            return;
        }
        let clear = self.remaining() == 0;
        let won = match self.content().objective {
            Objective::Eliminate => clear,
            Objective::Rescue => self
                .carrier
                .is_some_and(|i| self.units[i].hp > 0 && self.units[i].pos == self.extraction),
            Objective::Capture => self.completed[0] && self.held >= 2,
            Objective::Hold => self.completed[0] && self.held >= 4 && self.round >= 7,
            Objective::Sabotage => clear && self.completed.iter().all(|c| *c),
            Objective::Liberate => clear && self.completed[0],
        };
        if won {
            let survivors = self.units.iter().filter(|u| u.hp > 0).count() as u32;
            let speed = usize::from(self.round) <= usize::from(self.content().limit) / 2;
            self.medals[self.mission_index] = if survivors == 3 && speed {
                3
            } else if survivors >= 2 {
                2
            } else {
                1
            };
            self.score += 1000
                + survivors * 150
                + u32::from(self.content().limit.saturating_sub(self.round)) * 25;
            self.phase = if self.mission_index == 5 {
                Phase::Won
            } else {
                Phase::Debrief
            };
            self.note(format!(
                "{} secured. {} machines return to the hangar.",
                self.content().name,
                survivors
            ));
        }
    }
    fn choose_upgrade(&mut self, upgrade: usize) -> Cue {
        if upgrade >= 3 || self.upgrades[upgrade] >= 2 {
            return Cue::None;
        }
        self.upgrades[upgrade] += 1;
        self.mission_index += 1;
        self.checkpoint = self.score;
        self.load();
        Cue::Good
    }
    fn retry(&mut self) -> Cue {
        if self.retries == 0 {
            return Cue::None;
        }
        self.retries -= 1;
        self.score = self.checkpoint;
        self.load();
        self.note(format!(
            "Operation reset. {} campaign retries remain.",
            self.retries
        ));
        Cue::Action
    }
    fn cell_at(pointer: Vec2) -> Option<Pos> {
        if !pointer.is_finite() || pointer.x < BOARD_X || pointer.y < BOARD_Y {
            return None;
        }
        let p = Pos::new(
            ((pointer.x - BOARD_X) / TILE_SIZE) as i32,
            ((pointer.y - BOARD_Y) / TILE_SIZE) as i32,
        );
        p.inside().then_some(p)
    }
    /// Apply one tick's input. AP and turns change only after accepted commands.
    pub fn step(&mut self, input: &Input) -> Cue {
        if self.outcome() != Outcome::Playing {
            return Cue::None;
        }
        self.flash = self.flash.saturating_sub(1);
        match self.phase {
            Phase::Briefing => {
                if input.primary || input.click {
                    self.phase = Phase::Battle;
                    self.note(self.content().tip.to_owned());
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Debrief => {
                if let Some(slot) = input.slot {
                    return self.choose_upgrade(slot.saturating_sub(1));
                }
                if input.click && input.pointer.y >= 355. && input.pointer.y <= 478. {
                    for i in 0..3 {
                        if input.pointer.x >= 46. + i as f32 * 174.
                            && input.pointer.x < 206. + i as f32 * 174.
                        {
                            return self.choose_upgrade(i);
                        }
                    }
                }
                return Cue::None;
            }
            Phase::Setback => {
                if input.secondary || input.primary {
                    return self.retry();
                }
                if input.tertiary {
                    self.phase = Phase::Lost;
                    return Cue::Bad;
                }
                return Cue::None;
            }
            Phase::Battle => {}
            Phase::Won | Phase::Lost => return Cue::None,
        }
        if let Some(slot) = input.slot {
            if (1..=3).contains(&slot) && self.units[slot - 1].hp > 0 {
                self.selected = slot - 1;
                self.cursor = self.units[self.selected].pos;
                return Cue::Action;
            }
            if slot == 4 {
                return self.enemy_turn();
            }
        }
        if input.tertiary {
            return self.enemy_turn();
        }
        if input.secondary {
            return self.special();
        }
        if input.click {
            if let Some(p) = Self::cell_at(input.pointer) {
                self.cursor = p;
                return self.context();
            }
            if input.pointer.x >= 600. && input.pointer.x <= 932. {
                for i in 0..3 {
                    if input.pointer.y >= CARD_Y + i as f32 * 55.
                        && input.pointer.y < CARD_Y + i as f32 * 55. + 50.
                        && self.units[i].hp > 0
                    {
                        self.selected = i;
                        self.cursor = self.units[i].pos;
                        return Cue::Action;
                    }
                }
                if (401. ..=435.).contains(&input.pointer.y) {
                    return self.context();
                }
                if (442. ..=476.).contains(&input.pointer.y) {
                    return self.special();
                }
                if (483. ..=517.).contains(&input.pointer.y) {
                    return self.enemy_turn();
                }
            }
        }
        if input.primary {
            return self.context();
        }
        let (dx, dy) = (
            i32::from(input.right_pressed) - i32::from(input.left_pressed),
            i32::from(input.down_pressed) - i32::from(input.up_pressed),
        );
        if dx != 0 || dy != 0 {
            self.cursor.x = (self.cursor.x + dx).clamp(0, COLS as i32 - 1);
            self.cursor.y = (self.cursor.y + dy).clamp(0, ROWS as i32 - 1);
        }
        Cue::None
    }
    fn click_command(p: Pos) -> Input {
        Input {
            click: true,
            pointer: p.center(),
            ..Input::default()
        }
    }
    fn select_or(&self, unit: usize, command: Input) -> Input {
        if self.selected == unit {
            command
        } else {
            Input {
                slot: Some(unit + 1),
                ..Input::default()
            }
        }
    }
    fn toward(&self, unit: usize, target: Pos, reserve: u8) -> Option<Input> {
        let u = self.units[unit];
        let path = match self.path(u.pos, target, true) {
            Some(path) => path,
            None => return self.clear_route(unit, target),
        };
        let budget = usize::from(u.ap.saturating_sub(reserve));
        let destination = path
            .iter()
            .copied()
            .skip(1)
            .take(budget)
            .take_while(|p| !self.occupied(*p))
            .filter(|p| {
                !(u.ap < 2
                    && u.pos.distance(*p) <= 1
                    && self
                        .objectives
                        .iter()
                        .enumerate()
                        .any(|(i, objective)| !self.completed[i] && *objective == Some(*p)))
            })
            .last()?;
        Some(self.select_or(unit, Self::click_command(destination)))
    }
    // Move a squadmate aside when it blocks the only corridor. This returns
    // normal selection/movement input, never a board mutation.
    fn clear_route(&self, unit: usize, target: Pos) -> Option<Input> {
        let mut grid = Grid::new(COLS as u32, ROWS as u32).ok()?;
        for y in 0..ROWS {
            for x in 0..COLS {
                let p = Pos::new(x as i32, y as i32);
                grid.set_blocked(
                    p.cell(),
                    !self.walkable(p) || (p != target && self.hostile(p).is_some()),
                )
                .ok()?;
            }
        }
        let path: Vec<_> = grid
            .path(
                self.units[unit].pos.cell(),
                target.cell(),
                Connectivity::Four,
            )
            .ok()?
            .into_iter()
            .map(|p| Pos::new(p.x as i32, p.y as i32))
            .collect();
        for p in path.iter().skip(1) {
            if let Some(friend) = self.friendly(*p) {
                if self.units[friend].ap == 0 {
                    continue;
                }
                for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                    let to = Pos::new(p.x + dx, p.y + dy);
                    if self.walkable(to)
                        && !self.occupied(to)
                        && !path.contains(&to)
                        && !self
                            .objectives
                            .iter()
                            .enumerate()
                            .any(|(i, objective)| !self.completed[i] && *objective == Some(to))
                    {
                        return Some(self.select_or(friend, Self::click_command(to)));
                    }
                }
            }
        }
        None
    }
    /// Deterministic legal controller for smoke runs and complete-campaign tests.
    /// It only returns selection, pointer, skill and end-turn commands.
    pub fn autoplay(&self, tick: u64) -> Input {
        if !tick.is_multiple_of(10) {
            return Input::default();
        }
        match self.phase {
            Phase::Briefing => {
                return Input {
                    primary: true,
                    ..Input::default()
                };
            }
            Phase::Debrief => {
                return Input {
                    slot: Some(if self.upgrades[1] < 2 {
                        2
                    } else if self.upgrades[0] < 2 {
                        1
                    } else {
                        3
                    }),
                    ..Input::default()
                };
            }
            Phase::Setback => {
                return Input {
                    secondary: true,
                    ..Input::default()
                };
            }
            Phase::Won | Phase::Lost => return Input::default(),
            Phase::Battle => {}
        }
        // The engineer patches an injured nearby teammate before committing AP.
        if self.units[2].hp > 0
            && self.units[2].ap >= 2
            && self.kits > 0
            && let Some(i) = (0..3)
                .filter(|&i| {
                    self.units[i].hp > 0 && self.units[2].pos.distance(self.units[i].pos) <= 2
                })
                .find(|&i| self.units[i].hp + 6 <= self.max_hull(self.units[i].role))
        {
            if self.selected != 2 {
                return Input {
                    slot: Some(3),
                    ..Input::default()
                };
            }
            if self.cursor != self.units[i].pos {
                let to = self.units[i].pos;
                return Input {
                    left_pressed: self.cursor.x > to.x,
                    right_pressed: self.cursor.x < to.x,
                    up_pressed: self.cursor.y > to.y,
                    down_pressed: self.cursor.y < to.y,
                    ..Input::default()
                };
            }
            return Input {
                secondary: true,
                ..Input::default()
            };
        }
        // Prefer a cannon shot that removes a threat before the enemy phase.
        let mut target = None;
        for unit in 0..3 {
            for enemy in 0..MAX_ENEMIES {
                if self.can_shoot(unit, enemy) {
                    let e = self.enemies[enemy].unwrap();
                    let kill = self.shot_damage(unit, enemy) >= e.hp;
                    let priority = (
                        u8::from(!kill),
                        e.hp,
                        self.units[unit].pos.distance(e.pos),
                        unit,
                        enemy,
                    );
                    if target.is_none_or(|best| priority < best) {
                        target = Some(priority);
                    }
                }
            }
        }
        if let Some((_, _, _, unit, enemy)) = target {
            return self.select_or(unit, Self::click_command(self.enemies[enemy].unwrap().pos));
        }
        // Advance toward active threats. Keep AP for a shot whenever a firing
        // position is reachable this turn; otherwise close the distance.
        for reserve in [2, 0] {
            for unit in 0..3 {
                if self.units[unit].hp == 0 || self.units[unit].ap <= reserve {
                    continue;
                }
                let mut paths: Vec<_> = self
                    .enemies
                    .iter()
                    .flatten()
                    .filter(|e| e.active && e.hp > 0)
                    .filter_map(|e| {
                        self.path(self.units[unit].pos, e.pos, true)
                            .map(|path| (path.len(), e.pos))
                    })
                    .collect();
                paths.sort_by_key(|(len, p)| (*len, p.index()));
                for (_, to) in paths {
                    if let Some(command) = self.toward(unit, to, reserve) {
                        return command;
                    }
                }
            }
        }
        // With the immediate patrol cleared, perform the mission objective.
        if !self.enemies.iter().flatten().any(|e| e.active && e.hp > 0) {
            let actor = self
                .carrier
                .or_else(|| self.units.iter().position(|u| u.hp > 0));
            if let Some(unit) = actor {
                if self.content().objective == Objective::Rescue && self.carrier.is_some() {
                    if let Some(command) = self.toward(unit, self.extraction, 0) {
                        return command;
                    }
                } else if let Some(index) = self
                    .completed
                    .iter()
                    .enumerate()
                    .find(|(i, done)| !**done && self.objectives[*i].is_some())
                    .map(|(i, _)| i)
                {
                    let to = self.objectives[index].unwrap();
                    let u = self.units[unit];
                    if u.pos.distance(to) <= 1 && u.ap >= 2 {
                        return self.select_or(unit, Self::click_command(to));
                    }
                    if let Some(command) = self.toward(unit, to, 0) {
                        return command;
                    }
                } else if matches!(
                    self.content().objective,
                    Objective::Capture | Objective::Hold
                ) {
                    let to = self.objectives[0].unwrap();
                    if self.units[unit].pos.distance(to) > 1
                        && let Some(command) = self.toward(unit, to, 0)
                    {
                        return command;
                    }
                }
            }
        }
        Input {
            tertiary: true,
            ..Input::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deploy() -> Game {
        let mut game = Game::new();
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        game
    }
    fn arena() -> Game {
        let mut game = deploy();
        game.tiles.fill(Tile::Floor);
        game.enemies.fill(None);
        // A future reserve keeps ordinary isolated mechanics from finishing OP 1.
        game.enemies[7] = Some(Enemy {
            pos: Pos::new(10, 7),
            hp: 1,
            kind: Kind::Sentry,
            intent: Intent::Wait,
            arrives: 20,
            active: false,
        });
        game.units[0].pos = Pos::new(2, 2);
        game.units[1].pos = Pos::new(1, 5);
        game.units[2].pos = Pos::new(2, 5);
        game.cursor = game.units[0].pos;
        game
    }
    fn foe(game: &mut Game, pos: Pos, hp: u8, intent: Intent) {
        game.enemies[0] = Some(Enemy {
            pos,
            hp,
            kind: Kind::Gunner,
            intent,
            arrives: 1,
            active: true,
        });
    }
    fn end(game: &mut Game) {
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
    }

    #[test]
    fn authored_campaign_has_six_distinct_valid_boards_and_objectives() {
        let mut game = Game::new();
        for (index, mission) in MISSIONS.iter().enumerate() {
            game.mission_index = index;
            game.load();
            assert!(mission.map.iter().all(|row| row.len() == COLS));
            assert!(game.units.iter().all(|u| game.walkable(u.pos)));
            assert!((4..=6).contains(&game.remaining()));
            for other in MISSIONS.iter().take(index) {
                assert_ne!(mission.map, other.map);
                assert_ne!(mission.objective, other.objective);
            }
        }
    }
    #[test]
    fn idle_held_and_invalid_orders_spend_no_ap_score_or_turns() {
        let mut game = deploy();
        let units = game.units;
        let score = game.score();
        for _ in 0..120 {
            game.step(&Input {
                right: true,
                primary_down: true,
                ..Input::default()
            });
            game.step(&Game::click_command(Pos::new(0, 0)));
            game.step(&Input {
                click: true,
                pointer: Vec2::new(f32::NAN, 40.),
                ..Input::default()
            });
            game.step(&Input {
                slot: Some(5),
                ..Input::default()
            });
        }
        assert_eq!(game.units, units);
        assert_eq!(game.round(), 1);
        assert_eq!(game.score(), score);
    }
    #[test]
    fn navigation_charges_route_length_and_rejects_over_budget_orders() {
        let mut game = arena();
        assert_eq!(game.step(&Game::click_command(Pos::new(4, 2))), Cue::Action);
        assert_eq!(game.units[0].ap, 2);
        assert_eq!(game.units[0].pos, Pos::new(4, 2));
        let before = game.units;
        assert_eq!(game.step(&Game::click_command(Pos::new(8, 2))), Cue::None);
        assert_eq!(game.units, before);
        end(&mut game);
        assert_eq!(game.units[0].ap, 4);
    }
    #[test]
    fn walls_corners_and_symmetric_line_of_sight() {
        let mut game = arena();
        game.tiles[Pos::new(3, 2).index()] = Tile::Wall;
        assert!(!game.visible(Pos::new(2, 2), Pos::new(4, 2)));
        assert!(!game.visible(Pos::new(2, 2), Pos::new(3, 3)));
        game.tiles[Pos::new(3, 2).index()] = Tile::Cover;
        assert!(game.visible(Pos::new(2, 2), Pos::new(4, 2)));
        let game = deploy();
        for a in 0..CELLS {
            for b in 0..CELLS {
                let a = Pos::new((a % COLS) as i32, (a / COLS) as i32);
                let b = Pos::new((b % COLS) as i32, (b / COLS) as i32);
                if game.walkable(a) && game.walkable(b) {
                    assert_eq!(game.visible(a, b), game.visible(b, a), "{a:?} {b:?}");
                }
            }
        }
    }
    #[test]
    fn shots_spend_two_ap_cover_reduces_damage_and_kills_score_once() {
        let mut game = arena();
        let to = Pos::new(4, 2);
        foe(&mut game, to, 6, Intent::Wait);
        game.tiles[to.index()] = Tile::Cover;
        assert_eq!(game.step(&Game::click_command(to)), Cue::Action);
        assert_eq!(game.enemies[0].unwrap().hp, 3);
        assert_eq!(game.units[0].ap, 2);
        assert_eq!(game.step(&Game::click_command(to)), Cue::Good);
        assert_eq!(game.score, 100);
        assert_eq!(game.units[0].ap, 0);
        for _ in 0..3 {
            game.step(&Game::click_command(to));
        }
        assert_eq!(game.score, 100);
        end(&mut game);
        foe(&mut game, Pos::new(10, 2), 6, Intent::Wait);
        let ap = game.units[0].ap;
        game.step(&Game::click_command(Pos::new(10, 2)));
        assert_eq!(game.units[0].ap, ap);
    }
    #[test]
    fn focus_is_consumed_once_and_repair_cannot_farm_resources() {
        let mut game = arena();
        game.selected = 1;
        game.units[1].pos = Pos::new(2, 2);
        game.units[0].pos = Pos::new(0, 0);
        foe(&mut game, Pos::new(4, 2), 12, Intent::Wait);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.units[1].ap, 4);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.units[1].ap, 4);
        game.step(&Game::click_command(Pos::new(4, 2)));
        assert_eq!(game.enemies[0].unwrap().hp, 7);
        assert!(!game.units[1].focused);
        game.step(&Game::click_command(Pos::new(4, 2)));
        assert_eq!(game.enemies[0].unwrap().hp, 4);
        game.selected = 2;
        game.cursor = game.units[2].pos;
        game.units[2].hp = 9;
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.units[2].hp, 15);
        assert_eq!(game.kits, 1);
        assert_eq!(game.units[2].ap, 2);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.kits, 1);
        assert_eq!(game.units[2].ap, 2);
    }
    #[test]
    fn enemy_fire_resolves_telegraphed_tile_and_brace_mitigates_hits() {
        let mut game = arena();
        let aim = game.units[0].pos;
        foe(&mut game, Pos::new(4, 2), 6, Intent::Fire(aim));
        game.step(&Game::click_command(Pos::new(2, 3)));
        end(&mut game);
        assert_eq!(game.units[0].hp, 18);
        game.enemies[0].as_mut().unwrap().intent = Intent::Fire(game.units[0].pos);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.units[0].ap, 3);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.units[0].ap, 3);
        end(&mut game);
        assert_eq!(game.units[0].hp, 17);
        assert!(!game.units[0].braced);
    }
    #[test]
    fn enemy_movement_is_one_tile_and_reserves_cannot_be_spawn_blocked() {
        let mut game = arena();
        foe(&mut game, Pos::new(8, 2), 6, Intent::Move(Pos::new(7, 2)));
        end(&mut game);
        assert_eq!(game.enemies[0].unwrap().pos, Pos::new(7, 2));
        game.enemies[1] = Some(Enemy {
            pos: game.units[0].pos,
            hp: 6,
            kind: Kind::Gunner,
            intent: Intent::Wait,
            arrives: 3,
            active: false,
        });
        end(&mut game);
        let reserve = game.enemies[1].unwrap();
        assert!(reserve.active);
        assert_ne!(reserve.pos, game.units[0].pos);
        assert_eq!(reserve.pos.distance(game.units[0].pos), 1);
    }
    #[test]
    fn lethal_enemy_phase_supports_retry_score_rollback_and_final_loss() {
        let mut game = arena();
        game.checkpoint = 500;
        game.score = 700;
        for i in 0..3 {
            game.units[i].hp = 1;
            game.enemies[i] = Some(Enemy {
                pos: Pos::new(6, i as i32 + 2),
                hp: 6,
                kind: Kind::Gunner,
                intent: Intent::Fire(game.units[i].pos),
                arrives: 1,
                active: true,
            });
        }
        end(&mut game);
        assert_eq!(game.phase, Phase::Setback);
        assert_eq!(game.outcome(), Outcome::Playing);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.phase, Phase::Briefing);
        assert_eq!(game.retries, 2);
        assert_eq!(game.score, 500);
        assert!(game.units.iter().all(|u| u.hp == game.max_hull(u.role)));
        game.phase = Phase::Setback;
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(game.outcome(), Outcome::Lost);
        let frozen = game.clone();
        for _ in 0..20 {
            game.step(&Input {
                primary: true,
                secondary: true,
                tertiary: true,
                slot: Some(1),
                ..Input::default()
            });
        }
        assert_eq!(game, frozen);
    }
    #[test]
    fn objective_cost_hold_contest_and_retry_deadline_are_enforced() {
        let mut game = arena();
        game.mission_index = 2;
        game.objectives[0] = Some(Pos::new(3, 2));
        game.step(&Game::click_command(Pos::new(3, 2)));
        assert!(game.completed[0]);
        assert_eq!(game.units[0].ap, 2);
        game.step(&Game::click_command(Pos::new(3, 2)));
        assert_eq!(game.units[0].ap, 1); // A normal move, no second interaction bonus.
        end(&mut game);
        assert_eq!(game.held, 1);
        foe(&mut game, Pos::new(4, 2), 6, Intent::Wait);
        end(&mut game);
        assert_eq!(game.held, 0);
        game.round = game.content().limit;
        end(&mut game);
        assert_eq!(game.phase, Phase::Setback);
    }
    #[test]
    fn upgrades_are_bounded_and_grant_permanent_stats() {
        let mut game = Game::new();
        game.phase = Phase::Debrief;
        game.upgrades[0] = 2;
        let before = game.clone();
        assert_eq!(
            game.step(&Input {
                slot: Some(1),
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(game, before);
        game.step(&Input {
            slot: Some(3),
            ..Input::default()
        });
        assert_eq!(game.mission(), 2);
        assert_eq!(game.upgrades, [2, 0, 1]);
        assert_eq!(game.units[0].hp, 24);
        assert_eq!(game.units[0].ap, 5);
    }
    #[test]
    fn legal_controller_targets_repairs_without_selecting_the_patient() {
        let mut game = arena();
        game.units[0].pos = Pos::new(2, 4);
        game.units[0].hp = 9;
        for tick in [0, 10, 20] {
            let input = game.autoplay(tick);
            game.step(&input);
        }
        assert_eq!(game.selected, 2);
        assert_eq!(game.units[0].hp, 15);
        assert_eq!(game.units[2].ap, 2);
        assert_eq!(game.kits, 1);
    }
    #[test]
    fn legal_controller_ends_turn_when_objective_blocks_a_one_ap_move() {
        let mut game = arena();
        game.mission_index = 5;
        game.objectives[0] = Some(Pos::new(3, 2));
        game.units[0].ap = 1;
        game.units[1].ap = 0;
        game.units[2].ap = 0;
        foe(&mut game, Pos::new(4, 2), 6, Intent::Wait);
        let command = game.autoplay(0);
        assert!(command.tertiary);
        game.step(&command);
        assert_eq!(game.round(), 2);
        assert_eq!(game.units[0].ap, 4);
    }
    #[test]
    fn complete_campaign_uses_only_legal_commands_and_terminal_state_is_frozen() {
        let mut game = Game::new();
        let mut replay = Game::new();
        let mut seen = [false; 6];
        let mut finished = None;
        for tick in 0..100_000 {
            seen[game.mission_index] = true;
            let input = game.autoplay(tick);
            game.step(&input);
            replay.step(&input);
            assert_eq!(game, replay, "diverged on tick {tick}");
            if game.outcome() != Outcome::Playing {
                finished = Some(tick);
                break;
            }
        }
        assert_eq!(
            game.outcome(),
            Outcome::Won,
            "op {} turn {} {:?}",
            game.mission(),
            game.round(),
            game.notes
        );
        assert!(seen.into_iter().all(|s| s));
        assert!(game.medals.into_iter().all(|m| m > 0));
        assert!(game.score() > 10_000);
        assert_eq!(game.retries, 3);
        eprintln!(
            "Legal campaign victory at tick {}, score {}, medals {:?}",
            finished.unwrap(),
            game.score,
            game.medals
        );
        let frozen = game.clone();
        for _ in 0..20 {
            game.step(&Input {
                primary: true,
                secondary: true,
                tertiary: true,
                slot: Some(1),
                click: true,
                pointer: Vec2::ZERO,
                ..Input::default()
            });
        }
        assert_eq!(game, frozen);
    }
}
