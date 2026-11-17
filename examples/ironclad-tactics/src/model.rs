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
use crate::fx::{Event, Glide, Refusal, Show};
use ferravex_arcade::{Cue, Input, Outcome};
use ferravex_math::{Rect, Vec2};
use ferravex_play::navigation::{Cell, Connectivity, Grid};

pub(crate) const BOARD_X: f32 = 28.;
pub(crate) const BOARD_Y: f32 = 140.;
pub(crate) const TILE_SIZE: f32 = 50.;
/// Command sidebar column, shared by drawing and pointer hit-testing.
pub(crate) const SIDE_X: f32 = 600.;
pub(crate) const SIDE_W: f32 = 332.;
pub(crate) const CARD_Y: f32 = 132.;
pub(crate) const CARD_H: f32 = 82.;
pub(crate) const CARD_STEP: f32 = 90.;
pub(crate) const ORDER_Y: f32 = 404.;
pub(crate) const ORDER_H: f32 = 44.;
pub(crate) const ORDER_STEP: f32 = 50.;
/// Engineer's patient buttons inside the skill row, one per machine.
pub(crate) const PATCH_X: f32 = SIDE_X + 206.;
pub(crate) const PATCH_STEP: f32 = 40.;
pub(crate) const PATCH_W: f32 = 36.;
/// Debrief upgrade cards.
pub(crate) const UPGRADE_X: f32 = 52.;
pub(crate) const UPGRADE_STEP: f32 = 172.;
pub(crate) const UPGRADE_W: f32 = 158.;
pub(crate) const UPGRADE_Y: f32 = 344.;
pub(crate) const UPGRADE_H: f32 = 172.;
/// Setback buttons, in the sidebar where orders usually are.
pub(crate) const RETRY: Rect = Rect::new(SIDE_X, 430., SIDE_W, 60.);
pub(crate) const CONCEDE: Rect = Rect::new(SIDE_X, 500., SIDE_W, 48.);
/// Guarded enemy phases a capture needs, and a hold needs.
pub(crate) const CAPTURE_PHASES: u8 = 2;
pub(crate) const HOLD_PHASES: u8 = 4;
/// A hold also lasts until this friendly turn.
pub(crate) const HOLD_UNTIL: u16 = 7;

pub(crate) fn card_area(i: usize) -> Rect {
    Rect::new(SIDE_X, CARD_Y + i as f32 * CARD_STEP, SIDE_W, CARD_H)
}
pub(crate) fn order_area(i: usize) -> Rect {
    Rect::new(SIDE_X, ORDER_Y + i as f32 * ORDER_STEP, SIDE_W, ORDER_H)
}
pub(crate) fn patch_area(i: usize) -> Rect {
    let row = order_area(1);
    Rect::new(
        PATCH_X + i as f32 * PATCH_STEP,
        row.y + (row.h - PATCH_W) * 0.5,
        PATCH_W,
        PATCH_W,
    )
}
pub(crate) fn upgrade_area(i: usize) -> Rect {
    Rect::new(
        UPGRADE_X + i as f32 * UPGRADE_STEP,
        UPGRADE_Y,
        UPGRADE_W,
        UPGRADE_H,
    )
}

/// Every clickable surface. Input and hover drawing share this hit-test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hit {
    Cell(Pos),
    Card(usize),
    Order,
    Skill,
    Patch(usize),
    EndTurn,
    Upgrade(usize),
    Retry,
    Concede,
}
/// What the contextual order would do at a cell, without doing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Preview {
    Fire {
        enemy: usize,
        damage: u8,
        lethal: bool,
        refusal: Option<Refusal>,
    },
    Select(usize),
    Interact {
        index: usize,
        refusal: Option<Refusal>,
    },
    Move {
        path: Vec<Pos>,
        refusal: Option<Refusal>,
    },
    Stay,
    Blocked,
}
/// What the selected machine's skill would do, without doing it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SkillPreview {
    pub patient: usize,
    pub amount: u8,
    pub refusal: Option<Refusal>,
}
/// Why an operation was lost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    SquadDown,
    OperatorLost,
    OutOfTime,
}

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
    pub(crate) fn range(self) -> u16 {
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
    pub(crate) checkpoint: u32,
    pub(crate) medals: [u8; 6],
    pub(crate) notes: [String; 2],
    /// Presentation feedback only; rules never read it.
    pub(crate) show: Show,
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
            show: Show::default(),
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
        self.show.reset();
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
    /// Route lengths from the selected machine within its remaining AP, by
    /// cell (`u8::MAX` when out of reach). Mirrors [`Self::path`]'s blocking.
    pub(crate) fn reach(&self) -> [u8; CELLS] {
        let mut cost = [u8::MAX; CELLS];
        let u = self.units[self.selected];
        if u.hp == 0 || !self.walkable(u.pos) {
            return cost;
        }
        let mut queue = [Pos::default(); CELLS];
        let (mut head, mut tail) = (0, 1);
        queue[0] = u.pos;
        cost[u.pos.index()] = 0;
        while head < tail {
            let p = queue[head];
            head += 1;
            let next = cost[p.index()] + 1;
            if next > u.ap {
                continue;
            }
            for (dx, dy) in [(0, -1), (1, 0), (0, 1), (-1, 0)] {
                let q = Pos::new(p.x + dx, p.y + dy);
                if self.walkable(q) && !self.occupied(q) && cost[q.index()] == u8::MAX {
                    cost[q.index()] = next;
                    queue[tail] = q;
                    tail += 1;
                }
            }
        }
        cost
    }
    /// Why the selected machine cannot shoot `enemy`, if it cannot.
    fn shot_refusal(&self, enemy: usize) -> Option<Refusal> {
        let u = self.units[self.selected];
        let e = self.enemies[enemy]?;
        if u.pos.distance(e.pos) > u.role.range() {
            Some(Refusal::Range)
        } else if !self.visible(u.pos, e.pos) {
            Some(Refusal::Sight)
        } else if !self.can_shoot(self.selected, enemy) {
            Some(Refusal::Ap)
        } else {
            None
        }
    }
    /// Why moving the selected machine along `path` would be refused.
    fn move_refusal(&self, path: &[Pos]) -> Option<Refusal> {
        let steps = path.len().saturating_sub(1);
        (steps == 0 || steps > usize::from(self.units[self.selected].ap)).then_some(Refusal::Ap)
    }
    /// What the contextual order (Space, or a click) would do at `at`. This
    /// mirrors `context` exactly and changes nothing.
    pub(crate) fn preview(&self, at: Pos) -> Preview {
        let u = self.units[self.selected];
        if let Some(enemy) = self.hostile(at) {
            let damage = self.shot_damage(self.selected, enemy);
            return Preview::Fire {
                enemy,
                damage,
                lethal: damage >= self.enemies[enemy].map_or(0, |e| e.hp),
                refusal: self.shot_refusal(enemy),
            };
        }
        if let Some(friend) = self.friendly(at)
            && friend != self.selected
        {
            return Preview::Select(friend);
        }
        let objective = self.objectives.iter().position(|p| *p == Some(at));
        let to = match objective {
            Some(index) if !self.completed[index] => {
                if u.pos.distance(at) <= 1 {
                    return Preview::Interact {
                        index,
                        refusal: (u.ap < 2).then_some(Refusal::Ap),
                    };
                }
                at
            }
            _ => at,
        };
        if to == u.pos {
            return Preview::Stay;
        }
        match self.path(u.pos, to, false) {
            Some(path) => Preview::Move {
                refusal: self.move_refusal(&path),
                path,
            },
            None => Preview::Blocked,
        }
    }
    /// What the selected machine's skill would do; for the engineer, on
    /// `patient` (the cursor's ally when `None`, as the X key does).
    pub(crate) fn skill_preview(&self, patient: Option<usize>) -> SkillPreview {
        let i = self.selected;
        let u = self.units[i];
        let ready = |active: bool| {
            if active {
                Some(Refusal::Ready)
            } else if u.ap == 0 {
                Some(Refusal::Ap)
            } else {
                None
            }
        };
        match u.role {
            Role::Vanguard => SkillPreview {
                patient: i,
                amount: 2,
                refusal: ready(u.braced),
            },
            Role::Scout => SkillPreview {
                patient: i,
                amount: 2,
                refusal: ready(u.focused),
            },
            Role::Engineer => {
                let target = patient
                    .filter(|&p| self.units[p].hp > 0)
                    .or_else(|| self.friendly(self.cursor))
                    .unwrap_or(i);
                let friend = self.units[target];
                let max = self.max_hull(friend.role);
                let refusal = if u.ap < 2 {
                    Some(Refusal::Ap)
                } else if self.kits == 0 {
                    Some(Refusal::Kit)
                } else if u.pos.distance(friend.pos) > 2 {
                    Some(Refusal::Range)
                } else if friend.hp == max {
                    Some(Refusal::Full)
                } else {
                    None
                };
                SkillPreview {
                    patient: target,
                    amount: max.saturating_sub(friend.hp).min(6),
                    refusal,
                }
            }
        }
    }
    /// Hull each machine would lose to the announced enemy fire, capped at
    /// its remaining hull (a disabled machine absorbs no further shots).
    pub(crate) fn incoming(&self) -> [u8; 3] {
        let mut damage = [0u8; 3];
        for e in self.enemies.iter().flatten() {
            if e.hp == 0 || !e.active {
                continue;
            }
            let Intent::Fire(to) = e.intent else {
                continue;
            };
            if let Some(i) = self.friendly(to)
                && self.visible(e.pos, to)
            {
                let mitigation = u8::from(self.tiles[to.index()] == Tile::Cover)
                    + if self.units[i].braced { 2 } else { 0 };
                let hit = e.kind.damage().saturating_sub(mitigation).max(1);
                damage[i] = damage[i].saturating_add(hit).min(self.units[i].hp);
            }
        }
        damage
    }
    /// Guarded enemy phases the objective still needs, as `(held, needed)`.
    pub(crate) fn guard(&self) -> Option<(u8, u8)> {
        match self.content().objective {
            Objective::Capture => Some((self.held, CAPTURE_PHASES)),
            Objective::Hold => Some((self.held, HOLD_PHASES)),
            _ => None,
        }
    }
    /// Whether an ally guards the activated objective and no enemy contests it.
    pub(crate) fn guard_state(&self) -> (bool, bool) {
        let Some(objective) = self.objectives[0] else {
            return (false, false);
        };
        let guarded = self
            .units
            .iter()
            .any(|u| u.hp > 0 && u.pos.distance(objective) <= 1);
        let contested = self
            .enemies
            .iter()
            .flatten()
            .any(|e| e.active && e.hp > 0 && e.pos.distance(objective) <= 1);
        (guarded, contested)
    }
    /// Why the operation was lost (meaningful during a setback or loss).
    pub(crate) fn failure(&self) -> Failure {
        if self.units.iter().all(|u| u.hp == 0) {
            Failure::SquadDown
        } else if self.carrier.is_some_and(|i| self.units[i].hp == 0) {
            Failure::OperatorLost
        } else {
            Failure::OutOfTime
        }
    }
    /// The clickable surface under `p` in the current phase.
    pub(crate) fn hit(&self, p: Vec2) -> Option<Hit> {
        if !p.is_finite() {
            return None;
        }
        match self.phase {
            Phase::Battle => {
                if let Some(cell) = Self::cell_at(p) {
                    return Some(Hit::Cell(cell));
                }
                if let Some(i) = (0..3).find(|&i| card_area(i).contains(p)) {
                    return Some(Hit::Card(i));
                }
                if order_area(0).contains(p) {
                    return Some(Hit::Order);
                }
                if order_area(1).contains(p) {
                    if self.units[self.selected].role == Role::Engineer
                        && let Some(i) = (0..3).find(|&i| patch_area(i).contains(p))
                    {
                        return Some(Hit::Patch(i));
                    }
                    return Some(Hit::Skill);
                }
                order_area(2).contains(p).then_some(Hit::EndTurn)
            }
            Phase::Debrief => (0..3)
                .find(|&i| upgrade_area(i).contains(p))
                .map(Hit::Upgrade),
            Phase::Setback => {
                if RETRY.contains(p) {
                    Some(Hit::Retry)
                } else {
                    CONCEDE.contains(p).then_some(Hit::Concede)
                }
            }
            Phase::Briefing | Phase::Won | Phase::Lost => None,
        }
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
            if let (Some(why), Some(e)) = (self.shot_refusal(enemy), self.enemies[enemy]) {
                self.show.push(Event::Refused { at: e.pos, why });
            }
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
        let from = self.units[i].pos;
        self.show.tracer(from, pos);
        self.show.enemy_hurt[enemy] = crate::fx::HURT_TICKS;
        self.show.push(Event::Fired {
            from,
            to: pos,
            damage,
            disabled: dead,
        });
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
            self.show.push(Event::Refused {
                at: to,
                why: Refusal::Route,
            });
            self.note("That route is blocked.".into());
            return Cue::None;
        };
        let steps = path.len() - 1;
        if steps == 0 || steps > usize::from(u.ap) {
            if steps > 0 {
                self.show.push(Event::Refused {
                    at: to,
                    why: Refusal::Ap,
                });
            }
            self.note("Choose a clear destination within remaining AP.".into());
            return Cue::None;
        }
        self.units[i].pos = to;
        self.units[i].ap -= steps as u8;
        self.units[i].braced = false;
        self.show.glides[i] = Glide::along(&path);
        self.show.push(Event::Moved { unit: i as u8, to });
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
            self.show.push(Event::Refused {
                at: target,
                why: Refusal::Ap,
            });
            self.note("Interacting needs 2 AP. End the turn to replenish.".into());
            return Cue::None;
        }
        self.units[i].ap -= 2;
        self.completed[index] = true;
        self.show.push(Event::Interacted {
            index: index as u8,
            at: target,
        });
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
            self.show.push(Event::Selected(friend as u8));
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
        let refuse = |game: &mut Self, at: Pos, why: Option<Refusal>| {
            if let Some(why) = why {
                game.show.push(Event::Refused { at, why });
            }
        };
        match u.role {
            Role::Vanguard => {
                if u.ap == 0 || u.braced {
                    refuse(self, u.pos, self.skill_preview(None).refusal);
                    return Cue::None;
                }
                self.units[i].ap -= 1;
                self.units[i].braced = true;
                self.show.push(Event::Braced(i as u8));
                self.note("Braced: reduce incoming damage by 2 until next turn.".into());
            }
            Role::Scout => {
                if u.ap == 0 || u.focused {
                    refuse(self, u.pos, self.skill_preview(None).refusal);
                    return Cue::None;
                }
                self.units[i].ap -= 1;
                self.units[i].focused = true;
                self.show.push(Event::Focused(i as u8));
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
                    refuse(self, friend.pos, self.skill_preview(None).refusal);
                    self.note(
                        "Patch needs 2 AP, one kit and a wounded ally within 2 tiles.".into(),
                    );
                    return Cue::None;
                }
                self.units[i].ap -= 2;
                self.kits -= 1;
                self.units[target].hp = (friend.hp + 6).min(self.max_hull(friend.role));
                self.show.push(Event::Repaired {
                    patient: target as u8,
                    amount: self.units[target].hp - friend.hp,
                });
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
        let mut volley = [None; MAX_ENEMIES];
        for (index, tracer) in volley.iter_mut().enumerate() {
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
                        self.show.enemy_glides[index] = Glide::along(&[e.pos, to]);
                        self.show.push(Event::EnemyMoved { to });
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
                        *tracer = Some((e.pos, to, true));
                        self.show.hurt[i] = crate::fx::HURT_TICKS;
                        self.show.push(Event::Hit {
                            at: to,
                            damage,
                            braced: self.units[i].braced,
                            disabled: self.units[i].hp == 0,
                        });
                        if self.units[i].hp == 0 {
                            self.note(format!("{} is disabled.", self.units[i].role.name()));
                        }
                    } else {
                        // The machine moved off the marked tile: a dodged shot.
                        *tracer = Some((e.pos, to, true));
                        if !self.units.iter().any(|u| u.pos == to) {
                            self.show.push(Event::Missed { at: to });
                        }
                    }
                }
            }
        }
        self.show.volley(volley);
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
            let (guarded, contested) = self.guard_state();
            let before = self.held;
            self.held = if guarded && !contested {
                self.held + 1
            } else {
                0
            };
            if self.held > 0 {
                let needed = self.guard().map_or(0, |(_, needed)| needed);
                self.show.push(Event::Guarded {
                    at: objective,
                    held: self.held.min(needed),
                    needed,
                });
            } else if before > 0 {
                self.show.push(Event::Contested { at: objective });
            }
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
                    self.show.push(Event::Reinforced { at: pos });
                    self.note("Reserve patrol has arrived near a marked entry.".into());
                }
            }
        }
        if self.units[self.selected].hp == 0 {
            self.selected = self.units.iter().position(|u| u.hp > 0).unwrap();
            self.cursor = self.units[self.selected].pos;
        }
        self.plan_enemies();
        self.show.push(Event::Turn(self.round));
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
            Objective::Capture => self.completed[0] && self.held >= CAPTURE_PHASES,
            Objective::Hold => {
                self.completed[0] && self.held >= HOLD_PHASES && self.round >= HOLD_UNTIL
            }
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
            self.show.push(Event::Secured);
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
        self.show.push(Event::Upgraded(upgrade as u8));
        self.load();
        Cue::Good
    }
    fn retry(&mut self) -> Cue {
        if self.retries == 0 {
            return Cue::None;
        }
        self.retries -= 1;
        self.score = self.checkpoint;
        self.show.push(Event::Retried);
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
    /// Select a living machine and put the cursor on it.
    fn select(&mut self, unit: usize) -> Cue {
        self.selected = unit;
        self.cursor = self.units[unit].pos;
        self.show.push(Event::Selected(unit as u8));
        Cue::Action
    }
    /// Apply one tick's input. AP and turns change only after accepted commands.
    pub fn step(&mut self, input: &Input) -> Cue {
        if self.outcome() != Outcome::Playing {
            return Cue::None;
        }
        self.show.tick();
        if matches!(self.phase, Phase::Debrief | Phase::Setback)
            && self.show.phase_age == crate::fx::REVEAL_TICKS
        {
            self.show.push(Event::Revealed);
        }
        let phase = self.phase;
        let cue = self.command(input);
        if self.phase != phase {
            self.show.phase_age = 0;
        }
        cue
    }
    fn command(&mut self, input: &Input) -> Cue {
        let hit = if input.click {
            self.hit(input.pointer)
        } else {
            None
        };
        match self.phase {
            Phase::Briefing => {
                if input.primary || input.click {
                    self.phase = Phase::Battle;
                    self.show.push(Event::Deployed);
                    self.note(self.content().tip.to_owned());
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Debrief => {
                if let Some(slot) = input.slot {
                    return self.choose_upgrade(slot.saturating_sub(1));
                }
                if let Some(Hit::Upgrade(i)) = hit {
                    return self.choose_upgrade(i);
                }
                return Cue::None;
            }
            Phase::Setback => {
                // The on-screen buttons mirror Space/X (retry) and C (concede).
                if input.secondary || input.primary || hit == Some(Hit::Retry) {
                    return self.retry();
                }
                if input.tertiary || hit == Some(Hit::Concede) {
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
                return self.select(slot - 1);
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
        match hit {
            Some(Hit::Cell(p)) => {
                self.cursor = p;
                return self.context();
            }
            Some(Hit::Card(i)) if self.units[i].hp > 0 => return self.select(i),
            Some(Hit::Order) => return self.context(),
            Some(Hit::Skill) => return self.special(),
            // Aiming the cursor at a patient and pressing X, in one click.
            Some(Hit::Patch(i)) if self.units[i].hp > 0 => {
                self.cursor = self.units[i].pos;
                return self.special();
            }
            Some(Hit::EndTurn) => return self.enemy_turn(),
            _ => {}
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
    fn click(game: &mut Game, at: Vec2) -> Cue {
        game.step(&Input {
            click: true,
            pointer: at,
            ..Input::default()
        })
    }
    #[test]
    fn sidebar_clicks_mirror_selection_order_skill_and_end_turn_keys() {
        let mut game = arena();
        // A squad card selects like its number key.
        assert_eq!(click(&mut game, card_area(1).center()), Cue::Action);
        assert_eq!(game.selected, 1);
        assert_eq!(game.cursor, game.units[1].pos);
        // The skill row is X: Kestrel focuses for 1 AP.
        assert_eq!(click(&mut game, order_area(1).center()), Cue::Good);
        assert!(game.units[1].focused);
        assert_eq!(game.units[1].ap, 4);
        // The order row is Space at the cursor: here, a one-tile move.
        game.step(&Input {
            down_pressed: true,
            ..Input::default()
        });
        assert_eq!(click(&mut game, order_area(0).center()), Cue::Action);
        assert_eq!(game.units[1].pos, Pos::new(1, 6));
        assert_eq!(game.units[1].ap, 3);
        // The end-turn row is C.
        click(&mut game, order_area(2).center());
        assert_eq!(game.round(), 2);
        // A disabled machine's card selects nothing.
        game.units[0].hp = 0;
        let selected = game.selected;
        assert_eq!(click(&mut game, card_area(0).center()), Cue::None);
        assert_eq!(game.selected, selected);
    }
    #[test]
    fn engineer_patient_buttons_aim_and_repair_like_cursor_and_x() {
        let wounded = || {
            let mut game = arena();
            game.units[0].pos = Pos::new(2, 4);
            game.units[0].hp = 9;
            game.selected = 2;
            game.cursor = game.units[2].pos;
            game
        };
        // Keyboard: aim the cursor at Brass, then X.
        let mut keys = wounded();
        keys.step(&Input {
            up_pressed: true,
            ..Input::default()
        });
        assert_eq!(
            keys.step(&Input {
                secondary: true,
                ..Input::default()
            }),
            Cue::Good
        );
        // Pointer: one click on Brass's patient button does the same.
        let mut game = wounded();
        let preview = game.skill_preview(Some(0));
        assert_eq!(
            (preview.patient, preview.amount, preview.refusal),
            (0, 6, None)
        );
        assert_eq!(game.hit(patch_area(0).center()), Some(Hit::Patch(0)));
        assert_eq!(click(&mut game, patch_area(0).center()), Cue::Good);
        assert_eq!(game.units, keys.units);
        assert_eq!((game.kits, game.selected, game.cursor), (1, 2, keys.cursor));
        // A refused patient spends nothing: Kestrel is out of reach.
        game.units[1].hp = 5;
        game.units[1].pos = Pos::new(8, 5);
        game.units[2].ap = 2;
        assert_eq!(game.skill_preview(Some(1)).refusal, Some(Refusal::Range));
        let before = (game.units, game.kits);
        assert_eq!(click(&mut game, patch_area(1).center()), Cue::None);
        assert_eq!((game.units, game.kits), before);
        // The buttons exist only while the engineer is selected.
        game.selected = 0;
        assert_eq!(game.hit(patch_area(0).center()), Some(Hit::Skill));
    }
    #[test]
    fn setback_buttons_mirror_retry_and_concede_keys() {
        let mut game = arena();
        game.phase = Phase::Setback;
        assert_eq!(click(&mut game, SIDE_X_POINT), Cue::None);
        assert_eq!(game.phase, Phase::Setback);
        assert_eq!(click(&mut game, RETRY.center()), Cue::Action);
        assert_eq!((game.phase, game.retries), (Phase::Briefing, 2));
        game.phase = Phase::Setback;
        assert_eq!(click(&mut game, CONCEDE.center()), Cue::Bad);
        assert_eq!(game.outcome(), Outcome::Lost);
    }
    const SIDE_X_POINT: Vec2 = Vec2::new(SIDE_X + 10., CARD_Y + 10.);
    #[test]
    fn previews_predict_exactly_what_the_order_then_does() {
        let mut game = arena();
        foe(&mut game, Pos::new(4, 2), 4, Intent::Wait);
        let Preview::Fire {
            damage,
            lethal,
            refusal,
            ..
        } = game.preview(Pos::new(4, 2))
        else {
            panic!("an enemy in range previews a shot");
        };
        assert_eq!((damage, lethal, refusal), (4, true, None));
        game.step(&Game::click_command(Pos::new(4, 2)));
        assert_eq!(game.enemies[0].unwrap().hp, 0);
        // An affordable route costs exactly its previewed length.
        let Preview::Move {
            path,
            refusal: None,
        } = game.preview(Pos::new(2, 3))
        else {
            panic!("a neighbouring tile previews a move");
        };
        let reach = game.reach();
        assert_eq!(usize::from(reach[Pos::new(2, 3).index()]), path.len() - 1);
        let ap = game.units[0].ap;
        game.step(&Game::click_command(Pos::new(2, 3)));
        assert_eq!(usize::from(ap - game.units[0].ap), path.len() - 1);
        // An unaffordable route is refused in the preview and by the rules.
        assert!(matches!(
            game.preview(Pos::new(9, 6)),
            Preview::Move {
                refusal: Some(Refusal::Ap),
                ..
            }
        ));
        assert_eq!(game.reach()[Pos::new(9, 6).index()], u8::MAX);
        let before = game.units;
        game.step(&Game::click_command(Pos::new(9, 6)));
        assert_eq!(game.units, before);
        assert_eq!(game.preview(game.units[1].pos), Preview::Select(1));
        // Announced fire costs exactly the previewed hull.
        let aim = game.units[0].pos;
        foe(&mut game, Pos::new(6, 3), 6, Intent::Fire(aim));
        let incoming = game.incoming();
        let hp = game.units[0].hp;
        end(&mut game);
        assert_eq!(hp - game.units[0].hp, incoming[0]);
        assert_eq!(incoming[0], 3);
    }
    #[test]
    fn presentation_events_record_accepted_and_refused_orders_for_one_tick() {
        let mut game = arena();
        foe(&mut game, Pos::new(4, 2), 6, Intent::Wait);
        game.step(&Game::click_command(Pos::new(4, 2)));
        assert!(game.show.events.iter().any(|e| matches!(
            e,
            Event::Fired {
                damage: 4,
                disabled: false,
                ..
            }
        )));
        // The next tick starts clean.
        game.step(&Input::default());
        assert_eq!(game.show.events.iter().count(), 0);
        // A refusal is recorded where it happened and spends nothing.
        game.step(&Game::click_command(Pos::new(10, 7)));
        assert!(game.show.events.iter().any(|e| e
            == Event::Refused {
                at: Pos::new(10, 7),
                why: Refusal::Ap,
            }));
        // The enemy phase reports hits and the new turn.
        game.enemies[0].as_mut().unwrap().intent = Intent::Fire(game.units[0].pos);
        end(&mut game);
        let events: Vec<_> = game.show.events.iter().collect();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Hit { damage: 3, .. }))
        );
        assert!(events.contains(&Event::Turn(2)));
    }
}
