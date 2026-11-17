//! Pure fixed-tick stealth rules and six authored heists.
//!
//! ```
//! use velvet_heist::{Game, Phase};
//! use ferravex_arcade::{Input, Outcome};
//! let mut game = Game::new();
//! assert_eq!(game.phase(), Phase::Briefing);
//! game.step(&Input { primary: true, ..Input::default() });
//! game.step(&Input { secondary: true, ..Input::default() });
//! assert!(game.is_crouching());
//! assert_eq!(game.outcome(), Outcome::Playing);
//! ```
use crate::content::{HEISTS, Heist};
use ferravex_arcade::{Cue, Input, Outcome};
use ferravex_play::navigation::{Cell, Connectivity, Grid};

pub(crate) const COLS: usize = 16;
pub(crate) const ROWS: usize = 10;
pub(crate) const CELLS: usize = COLS * ROWS;
pub(crate) const TILE: f32 = 39.;
pub(crate) const MAP_X: f32 = 24.;
pub(crate) const MAP_Y: f32 = 156.;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Pos {
    pub x: u8,
    pub y: u8,
}
impl Pos {
    pub const fn new(x: u8, y: u8) -> Self {
        Self { x, y }
    }
    pub fn index(self) -> usize {
        usize::from(self.y) * COLS + usize::from(self.x)
    }
    pub fn distance(self, other: Self) -> u16 {
        u16::from(self.x.abs_diff(other.x)) + u16::from(self.y.abs_diff(other.y))
    }
    fn cell(self) -> Cell {
        Cell {
            x: u32::from(self.x),
            y: u32::from(self.y),
        }
    }
    pub fn shifted(self, dx: i8, dy: i8) -> Option<Self> {
        let x = i16::from(self.x) + i16::from(dx);
        let y = i16::from(self.y) + i16::from(dy);
        ((0..COLS as i16).contains(&x) && (0..ROWS as i16).contains(&y)).then_some(Self {
            x: x as u8,
            y: y as u8,
        })
    }
}
/// A heist's rules phase. Shared title/help/pause are outside this model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Briefing,
    Infiltrating,
    Debrief,
    Setback,
    Escaped,
    Caught,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Guard {
    pub pos: Pos,
    pub facing: (i8, i8),
    pub route: usize,
    pub wait: u16,
    pub search: u16,
    pub target: Pos,
}
impl Default for Guard {
    fn default() -> Self {
        Self {
            pos: Pos::default(),
            facing: (1, 0),
            route: 1,
            wait: 0,
            search: 0,
            target: Pos::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Work {
    pub target: Pos,
    pub remaining: u16,
    pub total: u16,
    pub kind: u8,
}
/// A logical-canvas pointer zone. The sidebar buttons mirror X, C and Space
/// so a pointer player reaches every action through the same rule path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Zone {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Zone {
    pub fn contains(self, x: f32, y: f32) -> bool {
        x.is_finite()
            && y.is_finite()
            && (self.x..self.x + self.w).contains(&x)
            && (self.y..self.y + self.h).contains(&y)
    }
}
/// Sidebar button that toggles crouching, like X.
pub(crate) const CROUCH_ZONE: Zone = Zone {
    x: 682.,
    y: 246.,
    w: 250.,
    h: 52.,
};
/// Sidebar button that throws a coin, like C.
pub(crate) const COIN_ZONE: Zone = Zone {
    x: 682.,
    y: 306.,
    w: 250.,
    h: 52.,
};
/// Sidebar button that interacts, like Space.
pub(crate) const WORK_ZONE: Zone = Zone {
    x: 682.,
    y: 366.,
    w: 250.,
    h: 66.,
};
/// Why a heist attempt failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cause {
    Guard,
    Alarm,
    Time,
}
/// What a refused command was missing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    NoCoins,
    BadAim,
    NeedPass,
    NeedRecords,
    Nothing,
}
/// One accepted occurrence of the latest tick, recorded only for
/// presentation effects. Rules never read these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// The briefing was accepted and the clock started.
    Start,
    /// A heist (re)loaded at its entry stair.
    Load,
    Crouch(bool),
    /// Guard index that started investigating a sound.
    Heard(u8),
    /// Guard index that newly saw the thief.
    Spotted(u8),
    /// A coin landed on this tile.
    Throw(Pos),
    Refused(Pos, Refusal),
    WorkStart(Pos, u8),
    WorkCancel(Pos),
    WorkDone(Pos, u8),
    /// The thief crossed a live beam.
    Beam(Pos),
    /// The heist's first alarm cost the clean medals.
    MedalLost,
    /// A final-stretch countdown second (seconds left).
    Second(u32),
    /// Extraction with this many medals.
    Escape(u8),
    Caught(Cause),
}
const EVENT_CAP: usize = 16;
/// Bounded per-tick event list; cleared at the start of every live update.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Events {
    list: [Event; EVENT_CAP],
    len: u8,
}
impl Default for Events {
    fn default() -> Self {
        Self {
            list: [Event::Start; EVENT_CAP],
            len: 0,
        }
    }
}
impl PartialEq for Events {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}
impl Eq for Events {}
impl Events {
    fn push(&mut self, event: Event) {
        if let Some(slot) = self.list.get_mut(usize::from(self.len)) {
            *slot = event;
            self.len += 1;
        }
    }
    fn clear(&mut self) {
        self.len = 0;
    }
    pub fn as_slice(&self) -> &[Event] {
        &self.list[..usize::from(self.len)]
    }
}
/// Complete campaign state. Only [`Game::step`] mutates gameplay during play.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub(crate) phase: Phase,
    pub(crate) level: usize,
    pub(crate) tiles: [u8; CELLS],
    pub(crate) player: Pos,
    pub(crate) facing: (i8, i8),
    pub(crate) crouched: bool,
    pub(crate) move_wait: u16,
    pub(crate) work: Option<Work>,
    pub(crate) guards: [Guard; 4],
    pub(crate) guard_count: usize,
    pub(crate) key: bool,
    pub(crate) disabled: bool,
    pub(crate) coins: u8,
    pub(crate) noise: Option<(Pos, u16)>,
    pub(crate) aim: Option<Pos>,
    pub(crate) alarm: u16,
    pub(crate) peak_alarm: u16,
    pub(crate) seen: bool,
    pub(crate) quiet: u16,
    pub(crate) stolen: u8,
    pub(crate) needed: u8,
    pub(crate) extras: u8,
    pub(crate) extra_total: u8,
    pub(crate) elapsed: u32,
    pub(crate) score: u32,
    pub(crate) opening_score: u32,
    pub(crate) retries: u8,
    pub(crate) medals: [u8; 6],
    pub(crate) total_loot: u8,
    pub(crate) note: &'static str,
    /// Cause of the latest failure, for the capture card.
    pub(crate) cause: Cause,
    /// Presentation record of the latest tick; never read by rules.
    pub(crate) events: Events,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Begin a new career at the Velvet Foyer briefing.
    pub fn new() -> Self {
        let mut g = Self {
            phase: Phase::Briefing,
            level: 0,
            tiles: [b'#'; CELLS],
            player: Pos::default(),
            facing: (1, 0),
            crouched: false,
            move_wait: 0,
            work: None,
            guards: [Guard::default(); 4],
            guard_count: 0,
            key: false,
            disabled: false,
            coins: 3,
            noise: None,
            aim: None,
            alarm: 0,
            peak_alarm: 0,
            seen: false,
            quiet: 0,
            stolen: 0,
            needed: 0,
            extras: 0,
            extra_total: 0,
            elapsed: 0,
            score: 0,
            opening_score: 0,
            retries: 3,
            medals: [0; 6],
            total_loot: 0,
            note: "Keep low in the violet corridors. Read the guard cones.",
            cause: Cause::Guard,
            events: Events::default(),
        };
        g.load();
        g
    }
    pub(crate) fn content(&self) -> &'static Heist {
        &HEISTS[self.level]
    }
    fn load(&mut self) {
        self.tiles = [b'#'; CELLS];
        self.needed = 0;
        self.extra_total = 0;
        for (y, row) in self.content().rows.iter().enumerate() {
            for (x, t) in row.bytes().enumerate() {
                self.tiles[y * COLS + x] = t;
                if t == b'E' {
                    self.player = Pos::new(x as u8, y as u8);
                }
                self.needed += u8::from(t == b'L');
                self.extra_total += u8::from(t == b'$');
            }
        }
        self.guards = [Guard::default(); 4];
        self.guard_count = self.content().patrols.len();
        for i in 0..self.guard_count {
            let p = self.content().patrols[i];
            let a = p.points[0];
            let b = p.points[1];
            self.guards[i] = Guard {
                pos: Pos::new(a.0, a.1),
                facing: (
                    (b.0 as i8 - a.0 as i8).signum(),
                    (b.1 as i8 - a.1 as i8).signum(),
                ),
                route: 1,
                wait: p.period,
                search: 0,
                target: Pos::new(a.0, a.1),
            };
        }
        self.phase = Phase::Briefing;
        self.facing = (1, 0);
        self.crouched = false;
        self.move_wait = 0;
        self.work = None;
        self.key = false;
        self.disabled = !self.tiles.contains(&b'!');
        self.coins = 3;
        self.noise = None;
        self.aim = None;
        self.alarm = 0;
        self.peak_alarm = 0;
        self.seen = false;
        self.quiet = 0;
        self.stolen = 0;
        self.extras = 0;
        self.elapsed = 0;
        self.opening_score = self.score;
        self.note = "Space enters the heist. X crouches; C throws a coin.";
    }
    /// One-based heist number, including a completed final heist.
    pub fn heist(&self) -> usize {
        self.level + 1
    }
    /// Current local rules phase.
    pub fn phase(&self) -> Phase {
        self.phase
    }
    /// True while moving quietly and using shadows for concealment.
    pub fn is_crouching(&self) -> bool {
        self.crouched
    }
    /// Current career points. A local retry rolls back the failed heist's points.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// Required records successfully extracted across completed heists.
    pub fn records_saved(&self) -> u8 {
        self.total_loot
    }
    /// Terminal campaign state; further commands cannot change it.
    pub fn outcome(&self) -> Outcome {
        match self.phase {
            Phase::Escaped => Outcome::Won,
            Phase::Caught => Outcome::Lost,
            _ => Outcome::Playing,
        }
    }
    pub(crate) fn tile(&self, p: Pos) -> u8 {
        self.tiles[p.index()]
    }
    pub(crate) fn solid(&self, p: Pos) -> bool {
        matches!(self.tile(p), b'#' | b'=')
    }
    pub(crate) fn shadow(&self, p: Pos) -> bool {
        matches!(self.tile(p), b':' | b'H' | b'E') || p.x == 1 || p.x == 14 || p.y == 1 || p.y == 8
    }
    pub(crate) fn los(&self, a: Pos, b: Pos) -> bool {
        let (mut x, mut y) = (i16::from(a.x), i16::from(a.y));
        let (tx, ty) = (i16::from(b.x), i16::from(b.y));
        let dx = (tx - x).abs();
        let dy = -(ty - y).abs();
        let sx = if x < tx { 1 } else { -1 };
        let sy = if y < ty { 1 } else { -1 };
        let mut error = dx + dy;
        for _ in 0..COLS + ROWS {
            if x == tx && y == ty {
                return true;
            }
            let twice = error * 2;
            if twice >= dy {
                error += dy;
                x += sx;
            }
            if twice <= dx {
                error += dx;
                y += sy;
            }
            let p = Pos::new(x as u8, y as u8);
            if self.solid(p) {
                return false;
            }
        }
        false
    }
    pub(crate) fn cone(&self, guard: Guard, p: Pos, crouch: bool) -> bool {
        let distance = guard.pos.distance(p);
        if distance == 0 {
            return true;
        }
        if distance > if crouch { 4 } else { 6 } {
            return false;
        }
        if crouch && self.shadow(p) && distance > 1 {
            return false;
        }
        let dx = i16::from(p.x) - i16::from(guard.pos.x);
        let dy = i16::from(p.y) - i16::from(guard.pos.y);
        let forward = dx * i16::from(guard.facing.0) + dy * i16::from(guard.facing.1);
        let side = dx * i16::from(guard.facing.1) - dy * i16::from(guard.facing.0);
        forward > 0 && side.abs() <= forward && self.los(guard.pos, p)
    }
    fn grid(&self, careful: bool) -> Grid {
        let mut grid = Grid::new(COLS as u32, ROWS as u32).expect("small fixed board");
        for y in 0..ROWS {
            for x in 0..COLS {
                let p = Pos::new(x as u8, y as u8);
                if self.solid(p) || (careful && !self.disabled && self.tile(p) == b'!') {
                    grid.set_blocked(p.cell(), true).expect("board cell");
                } else if careful {
                    let danger = self.guards[..self.guard_count]
                        .iter()
                        .any(|g| self.cone(*g, p, true) || g.pos.distance(p) <= 1);
                    grid.set_cost(
                        p.cell(),
                        if danger {
                            160
                        } else if self.shadow(p) {
                            1
                        } else {
                            7
                        },
                    )
                    .expect("positive cell cost");
                }
            }
        }
        if careful {
            for g in &self.guards[..self.guard_count] {
                if g.pos != self.player {
                    grid.set_blocked(g.pos.cell(), true).expect("guard cell");
                }
            }
        }
        grid
    }
    fn fail(&mut self, cause: Cause) -> Cue {
        self.phase = if self.retries > 0 {
            Phase::Setback
        } else {
            Phase::Caught
        };
        self.work = None;
        self.cause = cause;
        self.note = match cause {
            Cause::Guard => "A guard caught you. Use a recovery favor to retry this heist.",
            Cause::Alarm => "The alarm reached lockdown. This heist must be retried.",
            Cause::Time => "The midnight shutters closed. Time ran out.",
        };
        self.events.push(Event::Caught(cause));
        Cue::Bad
    }
    /// Send guards within `radius` to investigate `p`. Footsteps report only
    /// newly alerted guards; a thrown coin reports every guard it attracts.
    fn hear(&mut self, p: Pos, radius: u16, report_all: bool) {
        for i in 0..self.guard_count {
            let g = &mut self.guards[i];
            if g.pos.distance(p) <= radius {
                let fresh = g.search == 0;
                g.search = 180;
                g.target = p;
                g.wait = g.wait.min(18);
                if report_all || fresh {
                    self.events.push(Event::Heard(i as u8));
                }
            }
        }
    }
    /// Where C would throw now: the marked tile, or up to three clear tiles ahead.
    pub(crate) fn throw_target(&self) -> Pos {
        if let Some(p) = self.aim {
            return p;
        }
        let mut p = self.player;
        for _ in 0..3 {
            if let Some(q) = p.shifted(self.facing.0, self.facing.1) {
                if self.solid(q) {
                    break;
                }
                p = q;
            }
        }
        p
    }
    /// True when a coin may land on `target`: clear, visible and within five steps.
    pub(crate) fn throwable(&self, target: Pos) -> bool {
        target != self.player
            && target.distance(self.player) <= 5
            && !self.solid(target)
            && self.los(self.player, target)
    }
    fn throw_coin(&mut self) -> Cue {
        if self.coins == 0 {
            self.note = "No coins remain. Shadows and timing are still free.";
            self.events
                .push(Event::Refused(self.player, Refusal::NoCoins));
            return Cue::None;
        }
        let target = self.throw_target();
        if !self.throwable(target) {
            self.note = "Aim at a clear tile within five steps.";
            self.events.push(Event::Refused(target, Refusal::BadAim));
            return Cue::None;
        }
        self.coins -= 1;
        self.noise = Some((target, 180));
        self.events.push(Event::Throw(target));
        self.hear(target, 6, true);
        self.note = "A coin rings out. Nearby guards investigate its landing.";
        Cue::Action
    }
    /// The current tile and its four neighbours, a faced door first.
    fn candidates(&self) -> [Pos; 5] {
        let mut candidates = [self.player; 5];
        for (i, (dx, dy)) in [(0, -1), (1, 0), (0, 1), (-1, 0)].into_iter().enumerate() {
            candidates[i + 1] = self.player.shifted(dx, dy).unwrap_or(self.player);
        }
        if let Some(ahead) = self.player.shifted(self.facing.0, self.facing.1)
            && self.tile(ahead) == b'='
        {
            let i = candidates.iter().position(|p| *p == ahead).unwrap_or(0);
            candidates.swap(0, i);
        }
        candidates
    }
    /// The tile and object a Space press would act on now, if any. A faced
    /// door takes priority, then the current tile and the adjacent tiles in
    /// north, east, south, west order. Read-only; [`Self::interact`] uses it.
    pub(crate) fn interaction(&self) -> Option<(Pos, u8)> {
        self.candidates().into_iter().find_map(|target| {
            let kind = self.tile(target);
            let ready = match kind {
                b'K' | b'L' | b'$' => true,
                b'=' => self.key,
                b'T' => !self.disabled,
                b'E' => self.stolen == self.needed,
                _ => false,
            };
            ready.then_some((target, kind))
        })
    }
    fn interact(&mut self) -> Cue {
        if self.work.is_some() {
            return Cue::None;
        }
        let Some((target, kind)) = self.interaction() else {
            let near = self.candidates();
            let refusal = if let Some(door) = near.iter().find(|p| self.tile(**p) == b'=') {
                (*door, Refusal::NeedPass)
            } else if let Some(exit) = near.iter().find(|p| self.tile(**p) == b'E') {
                (*exit, Refusal::NeedRecords)
            } else {
                (self.player, Refusal::Nothing)
            };
            self.events.push(Event::Refused(refusal.0, refusal.1));
            self.note = if !self.key {
                "Find the brass K pass before opening a security door."
            } else {
                "Stand beside a record, console or door. Return to E with all records."
            };
            return Cue::None;
        };
        let total = match kind {
            b'K' => 12,
            b'=' => 30,
            b'T' => 90,
            b'L' => 45,
            b'$' => 30,
            _ => {
                self.finish_heist();
                return if self.phase == Phase::Escaped {
                    Cue::Win
                } else {
                    Cue::Good
                };
            }
        };
        self.work = Some(Work {
            target,
            remaining: total,
            total,
            kind,
        });
        self.events.push(Event::WorkStart(target, kind));
        self.note = match kind {
            b'K' => "Taking the brass pass.",
            b'=' => "Quietly opening the security door.",
            b'T' => "Rewiring the console. Guards keep moving.",
            b'L' => "Lifting a required record from its display.",
            _ => "Packing an optional artwork.",
        };
        Cue::Action
    }
    /// Medals an extraction would earn right now: three when clean with both
    /// artworks, two when clean, otherwise one.
    pub(crate) fn forecast_medals(&self) -> u8 {
        let clean = self.peak_alarm == 0;
        if clean && self.extras == self.extra_total {
            3
        } else if clean {
            2
        } else {
            1
        }
    }
    /// Points an extraction would add right now.
    pub(crate) fn extraction_points(&self) -> u32 {
        1000 + u32::from(self.forecast_medals()) * 200 + self.seconds_left() * 3
    }
    fn finish_heist(&mut self) {
        let stars = self.forecast_medals();
        self.medals[self.level] = stars;
        self.total_loot += self.stolen;
        self.score += self.extraction_points();
        self.phase = if self.level == 5 {
            Phase::Escaped
        } else {
            Phase::Debrief
        };
        self.events.push(Event::Escape(stars));
        self.note = "The records are safe. The city remembers another stolen story.";
    }
    pub(crate) fn seconds_left(&self) -> u32 {
        self.content().seconds.saturating_sub(self.elapsed / 60)
    }
    fn advance_guards(&mut self) {
        for g in &mut self.guards[..self.guard_count] {
            g.search = g.search.saturating_sub(1);
            g.wait = g.wait.saturating_sub(1);
        }
        if self.guards[..self.guard_count].iter().all(|g| g.wait > 0) {
            return;
        }
        let grid = self.grid(false);
        for i in 0..self.guard_count {
            let p = self.content().patrols[i];
            let g = &mut self.guards[i];
            if g.wait > 0 {
                continue;
            }
            let target = if g.search > 0 {
                g.target
            } else {
                let t = p.points[g.route];
                Pos::new(t.0, t.1)
            };
            if target == g.pos {
                if g.search > 0 {
                    g.facing = (-g.facing.1, g.facing.0);
                } else {
                    g.route = (g.route + 1) % p.points.len();
                }
            } else if let Ok(path) = grid.path(g.pos.cell(), target.cell(), Connectivity::Four)
                && let Some(next) = path.get(1)
            {
                let next = Pos::new(next.x as u8, next.y as u8);
                g.facing = (next.x as i8 - g.pos.x as i8, next.y as i8 - g.pos.y as i8);
                g.pos = next;
            }
            g.wait = if g.search > 0 { 18 } else { p.period };
        }
    }
    /// Apply one 60 Hz command. Invalid interactions never award points or consume coins.
    pub fn step(&mut self, input: &Input) -> Cue {
        if self.outcome() != Outcome::Playing {
            return Cue::None;
        }
        self.events.clear();
        match self.phase {
            Phase::Briefing => {
                if input.primary || input.click {
                    self.phase = Phase::Infiltrating;
                    self.note = "X crouches. Violet shadow corridors conceal a crouching thief.";
                    self.events.push(Event::Start);
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Debrief => {
                if input.primary || input.click {
                    self.level += 1;
                    self.load();
                    self.events.push(Event::Load);
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Setback => {
                if input.primary || input.click {
                    self.retries -= 1;
                    self.score = self.opening_score;
                    self.load();
                    self.events.push(Event::Load);
                    return Cue::Action;
                }
                return Cue::None;
            }
            _ => {}
        }
        // Sidebar buttons are pointer twins of X, C and Space.
        let tap = |zone: Zone| input.click && zone.contains(input.pointer.x, input.pointer.y);
        let crouch = input.secondary || tap(CROUCH_ZONE);
        let throw = input.tertiary || tap(COIN_ZONE);
        let work = input.primary || tap(WORK_ZONE);
        let clean = self.peak_alarm == 0;
        let was_seen = self.seen;
        self.elapsed += 1;
        if self.elapsed.is_multiple_of(60) && (1..=10).contains(&self.seconds_left()) {
            self.events.push(Event::Second(self.seconds_left()));
        }
        self.move_wait = self.move_wait.saturating_sub(1);
        if let Some((p, t)) = self.noise {
            self.noise = if t > 1 { Some((p, t - 1)) } else { None };
        }
        let mut cue = Cue::None;
        if crouch {
            self.crouched = !self.crouched;
            self.note = if self.crouched {
                "Crouched: quiet steps and concealment in violet shadows."
            } else {
                "Standing: faster steps, longer guard sight and nearby footstep noise."
            };
            self.events.push(Event::Crouch(self.crouched));
            cue = Cue::Action;
        }
        if input.click
            && let Some(p) = Self::pointer(input.pointer.x, input.pointer.y)
        {
            self.aim = Some(p);
            self.note = "Coin target marked. Press C to throw if the path is clear.";
        }
        let direction = if input.left || input.left_pressed {
            Some((-1, 0))
        } else if input.right || input.right_pressed {
            Some((1, 0))
        } else if input.up || input.up_pressed {
            Some((0, -1))
        } else if input.down || input.down_pressed {
            Some((0, 1))
        } else {
            None
        };
        if let Some((dx, dy)) = direction {
            self.facing = (dx, dy);
            self.aim = None;
            if self.move_wait == 0
                && let Some(p) = self.player.shifted(dx, dy)
                && !self.solid(p)
            {
                self.player = p;
                if let Some(cancelled) = self.work.take() {
                    self.events.push(Event::WorkCancel(cancelled.target));
                }
                self.move_wait = if self.crouched { 18 } else { 9 };
                if !self.crouched {
                    self.hear(p, 2, false);
                }
                if self.tile(p) == b'!' && !self.disabled {
                    self.alarm = (self.alarm + 450).min(1000);
                    self.note = "Security beam! The alarm rises sharply.";
                    self.events.push(Event::Beam(p));
                    cue = Cue::Bad;
                }
            }
        }
        if throw {
            let c = self.throw_coin();
            if c != Cue::None {
                cue = c;
            }
        }
        if work {
            let c = self.interact();
            if c != Cue::None {
                cue = c;
            }
            if self.phase != Phase::Infiltrating {
                return cue;
            }
        }
        if let Some(mut work) = self.work {
            work.remaining -= 1;
            if work.remaining == 0 {
                self.tiles[work.target.index()] = b':';
                self.work = None;
                self.events.push(Event::WorkDone(work.target, work.kind));
                match work.kind {
                    b'K' => {
                        self.key = true;
                        self.note = "Brass pass acquired. It opens every security door here.";
                    }
                    b'=' => self.note = "Door open. Guards can use the passage too.",
                    b'T' => {
                        self.disabled = true;
                        self.note = "Security beams disabled for the rest of this heist.";
                    }
                    b'L' => {
                        self.stolen += 1;
                        self.score += 250;
                        self.note =
                            "Required record secured. Return to E when every record is packed.";
                    }
                    b'$' => {
                        self.extras += 1;
                        self.score += 175;
                        self.note =
                            "Optional artwork secured. Clean extraction earns the best medal.";
                    }
                    _ => {}
                }
                cue = Cue::Good;
            } else {
                self.work = Some(work);
            }
        }
        self.advance_guards();
        if self.guards[..self.guard_count]
            .iter()
            .any(|g| g.pos == self.player)
        {
            return self.fail(Cause::Guard);
        }
        self.seen = false;
        for i in 0..self.guard_count {
            if self.cone(self.guards[i], self.player, self.crouched) {
                self.seen = true;
                if !was_seen || self.guards[i].search == 0 {
                    self.events.push(Event::Spotted(i as u8));
                }
                self.guards[i].search = 180;
                self.guards[i].target = self.player;
            }
        }
        if self.seen {
            self.alarm = (self.alarm + if self.crouched { 7 } else { 18 }).min(1000);
            self.quiet = 0;
        } else {
            self.quiet = self.quiet.saturating_add(1);
            if self.quiet > 60 {
                self.alarm = self.alarm.saturating_sub(2);
            }
        }
        self.peak_alarm = self.peak_alarm.max(self.alarm);
        if clean && self.peak_alarm > 0 {
            self.events.push(Event::MedalLost);
        }
        if self.alarm >= 1000 {
            return self.fail(Cause::Alarm);
        }
        if self.elapsed >= self.content().seconds * 60 {
            return self.fail(Cause::Time);
        }
        cue
    }
    pub(crate) fn pointer(x: f32, y: f32) -> Option<Pos> {
        if !x.is_finite()
            || !y.is_finite()
            || !(MAP_X..MAP_X + COLS as f32 * TILE).contains(&x)
            || !(MAP_Y..MAP_Y + ROWS as f32 * TILE).contains(&y)
        {
            return None;
        }
        Some(Pos::new(
            ((x - MAP_X) / TILE) as u8,
            ((y - MAP_Y) / TILE) as u8,
        ))
    }
    /// Read-only demonstration commands. It uses ordinary crouch, movement and work actions.
    pub fn pilot(&self, tick: u64) -> Input {
        match self.phase {
            Phase::Briefing | Phase::Debrief | Phase::Setback => {
                return Input {
                    primary: tick.is_multiple_of(30),
                    ..Input::default()
                };
            }
            Phase::Infiltrating => {}
            _ => return Input::default(),
        }
        if !self.crouched {
            return Input {
                secondary: true,
                ..Input::default()
            };
        }
        if self.work.is_some() || self.move_wait > 0 {
            return Input::default();
        }
        let priorities: &[u8] = if !self.key {
            b"K"
        } else if !self.disabled {
            b"T"
        } else if self.tiles.contains(&b'=') {
            b"="
        } else if self.stolen < self.needed {
            b"L"
        } else {
            b"E"
        };
        let grid = self.grid(true);
        let mut best: Option<(u32, Vec<Cell>, Pos)> = None;
        for (index, t) in self.tiles.iter().enumerate() {
            if !priorities.contains(t) {
                continue;
            }
            let target = Pos::new((index % COLS) as u8, (index / COLS) as u8);
            if self.player.distance(target) <= 1 {
                return Input {
                    primary: true,
                    ..Input::default()
                };
            }
            for (dx, dy) in [(0, 0), (0, -1), (1, 0), (0, 1), (-1, 0)] {
                let Some(goal) = target.shifted(dx, dy) else {
                    continue;
                };
                let Ok(path) = grid.path(self.player.cell(), goal.cell(), Connectivity::Four)
                else {
                    continue;
                };
                let cost = path
                    .iter()
                    .map(|p| {
                        let p = Pos::new(p.x as u8, p.y as u8);
                        if self.guards[..self.guard_count]
                            .iter()
                            .any(|g| self.cone(*g, p, true) || g.pos.distance(p) <= 1)
                        {
                            160
                        } else if self.shadow(p) {
                            1
                        } else {
                            7
                        }
                    })
                    .sum();
                if best.as_ref().is_none_or(|(old, _, _)| cost < *old) {
                    best = Some((cost, path, target));
                }
            }
        }
        let Some((_, path, target)) = best else {
            return Input::default();
        };
        if path.len() < 2 || self.player.distance(target) <= 1 {
            return Input {
                primary: true,
                ..Input::default()
            };
        }
        let next = path[1];
        // Waiting is a normal player choice: do not walk directly into a patrol.
        if self.guards[..self.guard_count]
            .iter()
            .any(|g| g.pos.distance(Pos::new(next.x as u8, next.y as u8)) <= 1)
        {
            return Input::default();
        }
        Input {
            left: next.x < u32::from(self.player.x),
            right: next.x > u32::from(self.player.x),
            up: next.y < u32::from(self.player.y),
            down: next.y > u32::from(self.player.y),
            ..Input::default()
        }
    }
    /// One-sentence career result, published for status and screen readers.
    pub fn summary(&self) -> String {
        let medals = self.medals.iter().sum::<u8>();
        match self.phase {
            Phase::Escaped => format!(
                "All six heists cleared with {} records, {medals} of 18 medals and {} points.",
                self.total_loot, self.score
            ),
            Phase::Caught => format!(
                "Caught on heist {} of 6 with {} records safe, {medals} medals and {} points.",
                self.heist(),
                self.total_loot,
                self.score
            ),
            _ => format!(
                "{} of 6 heists cleared with {} records, {medals} medals and {} points.",
                self.cleared(),
                self.total_loot,
                self.score
            ),
        }
    }
    /// Heists extracted so far.
    pub(crate) fn cleared(&self) -> usize {
        self.level + usize::from(matches!(self.phase, Phase::Debrief | Phase::Escaped))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn primary() -> Input {
        Input {
            primary: true,
            ..Input::default()
        }
    }
    fn arena() -> Game {
        let mut g = Game::new();
        g.phase = Phase::Infiltrating;
        g.player = Pos::new(2, 2);
        g.guard_count = 0;
        g.disabled = true;
        for y in 0..ROWS {
            for x in 0..COLS {
                g.tiles[y * COLS + x] = if x == 0 || y == 0 || x == COLS - 1 || y == ROWS - 1 {
                    b'#'
                } else {
                    b'.'
                };
            }
        }
        g
    }
    fn tick(g: &mut Game, n: usize) {
        for _ in 0..n {
            g.step(&Input::default());
        }
    }
    fn sentry(g: &mut Game, p: Pos, facing: (i8, i8)) {
        g.guard_count = 1;
        g.guards[0] = Guard {
            pos: p,
            facing,
            wait: 1000,
            ..Guard::default()
        };
    }
    #[test]
    fn six_complete_heists_use_only_ordinary_commands_and_replay_exactly() {
        let mut g = Game::new();
        let mut replay = Game::new();
        let mut seen = [false; 6];
        let mut ended = None;
        for t in 0..120_000 {
            seen[g.level] = true;
            let input = g.pilot(t);
            g.step(&input);
            replay.step(&input);
            assert_eq!(g, replay, "same-host replay diverged at {t}");
            assert!(g.guard_count <= 4 && g.alarm <= 1000 && g.coins <= 3 && g.stolen <= g.needed);
            if g.outcome() != Outcome::Playing {
                ended = Some(t);
                break;
            }
        }
        assert_eq!(g.outcome(), Outcome::Won, "{} {:?}", g.summary(), g.player);
        assert!(seen.into_iter().all(|v| v));
        assert_eq!(g.records_saved(), 15);
        assert_eq!(g.retries, 3);
        assert!(g.medals.into_iter().all(|s| s >= 1));
        assert!(g.score() > 12_000);
        eprintln!(
            "Legal six-heist victory at tick {}, {} points, {:?} medals",
            ended.unwrap(),
            g.score(),
            g.medals
        );
        let frozen = g.clone();
        for _ in 0..20 {
            g.step(&Input {
                primary: true,
                secondary: true,
                tertiary: true,
                left: true,
                click: true,
                slot: Some(1),
                ..Input::default()
            });
        }
        assert_eq!(g, frozen);
    }
    #[test]
    fn authored_maps_have_unique_layouts_valid_patrols_and_reachable_objects() {
        assert_eq!(
            HEISTS
                .iter()
                .map(|h| h
                    .rows
                    .iter()
                    .map(|r| r.bytes().filter(|&b| b == b'L').count())
                    .sum::<usize>())
                .sum::<usize>(),
            15
        );
        for (level, h) in HEISTS.iter().enumerate() {
            assert!(h.rows.iter().all(|r| r.len() == COLS));
            assert!((2..=4).contains(&h.patrols.len()));
            assert_eq!(
                h.rows
                    .iter()
                    .flat_map(|r| r.bytes())
                    .filter(|&b| b == b'E')
                    .count(),
                1
            );
            assert_eq!(
                h.rows
                    .iter()
                    .flat_map(|r| r.bytes())
                    .filter(|&b| b == b'K')
                    .count(),
                1
            );
            assert_eq!(
                h.rows
                    .iter()
                    .flat_map(|r| r.bytes())
                    .filter(|&b| b == b'$')
                    .count(),
                2
            );
            for earlier in &HEISTS[..level] {
                assert_ne!(h.rows, earlier.rows);
            }
            let mut g = Game::new();
            g.level = level;
            g.load();
            g.disabled = true;
            for t in &mut g.tiles {
                if *t == b'=' {
                    *t = b':';
                }
            }
            let grid = g.grid(false);
            for patrol in h.patrols {
                for &(x, y) in patrol.points {
                    assert!(grid.walkable(Cell {
                        x: u32::from(x),
                        y: u32::from(y)
                    }));
                }
            }
            for (index, &t) in g.tiles.iter().enumerate() {
                if matches!(t, b'L' | b'K' | b'T' | b'$') {
                    assert!(
                        grid.path(
                            g.player.cell(),
                            Cell {
                                x: (index % COLS) as u32,
                                y: (index / COLS) as u32
                            },
                            Connectivity::Four
                        )
                        .is_ok(),
                        "{level}: {index}"
                    );
                }
            }
        }
    }
    #[test]
    fn briefing_is_inert_and_start_is_deliberate() {
        let mut g = Game::new();
        let before = g.clone();
        for _ in 0..120 {
            assert_eq!(
                g.step(&Input {
                    right: true,
                    secondary: true,
                    tertiary: true,
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!(g, before);
        assert_eq!(g.step(&primary()), Cue::Action);
        assert_eq!(g.phase(), Phase::Infiltrating);
        assert_eq!(g.elapsed, 0);
    }
    #[test]
    fn movement_respects_cooldown_walls_and_quiet_speed() {
        let mut walking = arena();
        let mut quiet = walking.clone();
        quiet.crouched = true;
        for _ in 0..18 {
            walking.step(&Input {
                right: true,
                ..Input::default()
            });
            quiet.step(&Input {
                right: true,
                ..Input::default()
            });
        }
        assert_eq!(walking.player, Pos::new(4, 2));
        assert_eq!(quiet.player, Pos::new(3, 2));
        walking.tiles[Pos::new(5, 2).index()] = b'#';
        let p = walking.player;
        for _ in 0..40 {
            walking.step(&Input {
                right: true,
                ..Input::default()
            });
        }
        assert_eq!(walking.player, p);
    }
    #[test]
    fn sight_uses_facing_range_occlusion_and_shadow_concealment() {
        let mut g = arena();
        let guard = Guard {
            pos: Pos::new(4, 4),
            facing: (1, 0),
            ..Guard::default()
        };
        assert!(g.cone(guard, Pos::new(7, 4), false));
        assert!(!g.cone(guard, Pos::new(3, 4), false));
        assert!(!g.cone(guard, Pos::new(5, 7), false));
        assert!(!g.cone(guard, Pos::new(11, 4), false));
        for wall in [b'#', b'='] {
            g.tiles[Pos::new(5, 4).index()] = wall;
            assert!(!g.cone(guard, Pos::new(7, 4), false));
        }
        g.tiles[Pos::new(5, 4).index()] = b'.';
        g.tiles[Pos::new(7, 4).index()] = b'H';
        assert!(g.cone(guard, Pos::new(7, 4), false));
        assert!(!g.cone(guard, Pos::new(7, 4), true));
        g.tiles[Pos::new(5, 4).index()] = b':';
        assert!(g.cone(guard, Pos::new(5, 4), true));
    }
    #[test]
    fn work_requires_real_ticks_and_cannot_award_a_record_twice() {
        let mut g = arena();
        let item = Pos::new(3, 2);
        g.tiles[item.index()] = b'L';
        assert_eq!(g.step(&primary()), Cue::Action);
        assert_eq!(g.stolen, 0);
        assert_eq!(g.work.unwrap().remaining, 44);
        for _ in 0..43 {
            g.step(&primary());
        }
        assert_eq!(g.stolen, 0);
        assert_eq!(g.step(&primary()), Cue::Good);
        assert_eq!(g.stolen, 1);
        assert_eq!(g.score, 250);
        assert_eq!(g.tile(item), b':');
        for _ in 0..80 {
            g.step(&primary());
        }
        assert_eq!(g.stolen, 1);
        assert_eq!(g.score, 250);
    }
    #[test]
    fn moving_cancels_unfinished_work_without_partial_loot() {
        let mut g = arena();
        g.tiles[Pos::new(3, 2).index()] = b'L';
        g.step(&primary());
        tick(&mut g, 20);
        g.step(&Input {
            down: true,
            ..Input::default()
        });
        assert!(g.work.is_none());
        tick(&mut g, 60);
        assert_eq!(g.stolen, 0);
        assert_eq!(g.score, 0);
    }
    #[test]
    fn pass_is_required_and_opening_door_changes_navigation_and_sight() {
        let mut g = arena();
        let door = Pos::new(3, 2);
        g.tiles[door.index()] = b'=';
        g.facing = (1, 0);
        assert_eq!(g.step(&primary()), Cue::None);
        assert!(g.work.is_none());
        assert!(g.solid(door));
        g.tiles[g.player.index()] = b'K';
        g.step(&primary());
        tick(&mut g, 11);
        assert!(g.key);
        g.step(&primary());
        tick(&mut g, 29);
        assert!(!g.solid(door));
        assert!(g.los(Pos::new(2, 2), Pos::new(5, 2)));
        assert!(g.key);
    }
    #[test]
    fn terminal_work_disables_beams_and_crossing_live_beams_raises_alarm() {
        let mut g = arena();
        g.disabled = false;
        g.tiles[Pos::new(2, 3).index()] = b'T';
        g.tiles[Pos::new(3, 2).index()] = b'!';
        g.step(&Input {
            right: true,
            ..Input::default()
        });
        assert_eq!(g.alarm, 450);
        assert_eq!(g.peak_alarm, 450);
        let mut safe = arena();
        safe.disabled = false;
        safe.tiles[Pos::new(2, 3).index()] = b'T';
        safe.tiles[Pos::new(3, 2).index()] = b'!';
        safe.step(&primary());
        tick(&mut safe, 88);
        assert!(!safe.disabled);
        safe.step(&Input::default());
        assert!(safe.disabled);
        safe.step(&Input {
            right: true,
            ..Input::default()
        });
        assert_eq!(safe.alarm, 0);
        assert_eq!(safe.player, Pos::new(3, 2));
    }
    #[test]
    fn quiet_steps_do_not_send_the_footstep_investigation() {
        let mut loud = arena();
        sentry(&mut loud, Pos::new(4, 2), (0, -1));
        let mut quiet = loud.clone();
        loud.step(&Input {
            right: true,
            ..Input::default()
        });
        quiet.step(&Input {
            right: true,
            secondary: true,
            ..Input::default()
        });
        assert!(loud.guards[0].search > 0);
        assert_eq!(loud.guards[0].target, Pos::new(3, 2));
        assert_eq!(quiet.guards[0].search, 0);
    }
    #[test]
    fn coins_have_range_occlusion_capacity_and_real_investigation() {
        let mut g = arena();
        sentry(&mut g, Pos::new(6, 2), (0, -1));
        g.facing = (1, 0);
        assert_eq!(
            g.step(&Input {
                tertiary: true,
                ..Input::default()
            }),
            Cue::Action
        );
        assert_eq!(g.coins, 2);
        assert_eq!(g.guards[0].target, Pos::new(5, 2));
        assert!(g.guards[0].search > 0);
        g.aim = Some(Pos::new(14, 2));
        g.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(g.coins, 2);
        g.aim = Some(Pos::new(4, 2));
        g.tiles[Pos::new(3, 2).index()] = b'#';
        g.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(g.coins, 2);
        g.guard_count = 0;
        g.tiles[Pos::new(3, 2).index()] = b'.';
        for _ in 0..5 {
            g.step(&Input {
                tertiary: true,
                ..Input::default()
            });
        }
        assert_eq!(g.coins, 0);
        assert_eq!(g.score, 0);
    }
    #[test]
    fn suspicion_decays_only_after_losing_sight_and_search_expires() {
        let mut g = arena();
        sentry(&mut g, Pos::new(4, 2), (-1, 0));
        tick(&mut g, 20);
        assert_eq!(g.alarm, 360);
        assert!(g.seen);
        g.player = Pos::new(2, 7);
        tick(&mut g, 60);
        assert_eq!(g.alarm, 360);
        tick(&mut g, 1);
        assert_eq!(g.alarm, 358);
        tick(&mut g, 180);
        assert_eq!(g.alarm, 0);
        assert_eq!(g.guards[0].search, 0);
        assert_eq!(g.peak_alarm, 360);
    }
    #[test]
    fn guard_patrols_move_through_real_paths_and_touch_is_capture() {
        let mut g = arena();
        sentry(&mut g, Pos::new(3, 3), (1, 0));
        g.guards[0].wait = 1;
        g.guards[0].route = 1;
        g.step(&Input::default());
        assert_eq!(g.guards[0].pos, Pos::new(4, 3));
        let mut blocked = arena();
        sentry(&mut blocked, Pos::new(3, 3), (1, 0));
        blocked.guards[0].wait = 1;
        blocked.guards[0].route = 1;
        blocked.tiles[Pos::new(4, 3).index()] = b'#';
        blocked.step(&Input::default());
        assert_ne!(blocked.guards[0].pos, Pos::new(4, 3));
        assert_eq!(blocked.guards[0].pos.distance(Pos::new(3, 3)), 1);
        blocked.guards[0].pos = blocked.player;
        blocked.step(&Input::default());
        assert_eq!(blocked.phase(), Phase::Setback);
    }
    #[test]
    fn retries_restore_attempt_score_and_four_failures_end_the_career() {
        let mut g = Game::new();
        g.score = 2000;
        g.opening_score = 2000;
        g.total_loot = 4;
        for failure in 0..4 {
            g.phase = Phase::Infiltrating;
            g.score = 2250;
            g.stolen = 1;
            g.elapsed = g.content().seconds * 60 - 1;
            g.step(&Input::default());
            if failure < 3 {
                assert_eq!(g.phase(), Phase::Setback);
                g.step(&primary());
                assert_eq!(g.score, 2000);
                assert_eq!(g.stolen, 0);
                assert_eq!(g.total_loot, 4);
                assert_eq!(g.retries, 2 - failure);
            } else {
                assert_eq!(g.outcome(), Outcome::Lost);
            }
        }
        let frozen = g.clone();
        g.step(&Input {
            primary: true,
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(g, frozen);
    }
    #[test]
    fn extraction_requires_every_record_and_rewards_clean_optional_loot() {
        let mut g = arena();
        g.tiles[g.player.index()] = b'E';
        g.needed = 2;
        assert_eq!(g.step(&primary()), Cue::None);
        assert_eq!(g.phase(), Phase::Infiltrating);
        g.stolen = 2;
        g.extras = 2;
        g.extra_total = 2;
        assert_eq!(g.step(&primary()), Cue::Good);
        assert_eq!(g.phase(), Phase::Debrief);
        assert_eq!(g.total_loot, 2);
        assert_eq!(g.medals[0], 3);
        let bank = g.score;
        for _ in 0..20 {
            g.step(&Input::default());
        }
        assert_eq!(g.score, bank);
        g.step(&primary());
        assert_eq!(g.heist(), 2);
        assert_eq!(g.phase(), Phase::Briefing);
        assert_eq!(g.coins, 3);
    }
    #[test]
    fn pointer_bounds_and_unrelated_slots_do_not_spend_resources() {
        assert_eq!(Game::pointer(MAP_X, MAP_Y), Some(Pos::new(0, 0)));
        assert_eq!(Game::pointer(MAP_X + TILE * COLS as f32, MAP_Y), None);
        assert_eq!(Game::pointer(MAP_X - 0.01, MAP_Y), None);
        assert_eq!(Game::pointer(f32::NAN, MAP_Y), None);
        assert_eq!(Game::pointer(MAP_X, f32::INFINITY), None);
        let mut g = arena();
        for slot in 1..=5 {
            g.step(&Input {
                slot: Some(slot),
                ..Input::default()
            });
        }
        assert_eq!((g.coins, g.score, g.stolen), (3, 0, 0));
    }
    fn tap(zone: Zone) -> Input {
        Input {
            click: true,
            pointer: ferravex_arcade::vec2(zone.x + zone.w * 0.5, zone.y + zone.h * 0.5),
            ..Input::default()
        }
    }
    #[test]
    fn sidebar_buttons_mirror_crouch_coin_and_work_keys() {
        let mut keys = arena();
        sentry(&mut keys, Pos::new(6, 2), (0, -1));
        keys.facing = (1, 0);
        keys.tiles[Pos::new(2, 3).index()] = b'L';
        let mut taps = keys.clone();
        let pairs = [
            (
                Input {
                    secondary: true,
                    ..Input::default()
                },
                tap(CROUCH_ZONE),
            ),
            (
                Input {
                    tertiary: true,
                    ..Input::default()
                },
                tap(COIN_ZONE),
            ),
            (primary(), tap(WORK_ZONE)),
        ];
        for (key, click) in pairs {
            assert_eq!(keys.step(&key), taps.step(&click));
            assert_eq!(keys, taps);
        }
        assert!(taps.crouched);
        assert_eq!(taps.coins, 2);
        assert_eq!(taps.guards[0].target, Pos::new(5, 2));
        assert_eq!(taps.work.map(|w| w.target), Some(Pos::new(2, 3)));
        // Clicks elsewhere only mark coin targets; they never act.
        let mut idle = arena();
        let before = idle.clone();
        for pointer in [(670., 240.), (940., 440.), (660., 300.)] {
            idle.step(&Input {
                click: true,
                pointer: ferravex_arcade::vec2(pointer.0, pointer.1),
                ..Input::default()
            });
        }
        assert_eq!(
            (idle.crouched, idle.coins, idle.work),
            (before.crouched, before.coins, None)
        );
    }
    #[test]
    fn interaction_preview_is_exactly_what_space_accepts() {
        // A faced door outranks an object underfoot once the pass is held.
        let mut g = arena();
        g.facing = (1, 0);
        g.tiles[Pos::new(3, 2).index()] = b'=';
        g.tiles[g.player.index()] = b'L';
        assert_eq!(g.interaction(), Some((g.player, b'L')));
        g.key = true;
        assert_eq!(g.interaction(), Some((Pos::new(3, 2), b'=')));
        // A rewired console and a premature exit offer nothing.
        let mut idle = arena();
        idle.tiles[Pos::new(2, 3).index()] = b'T';
        idle.tiles[Pos::new(1, 2).index()] = b'E';
        idle.needed = 1;
        assert_eq!(idle.interaction(), None);
        assert_eq!(idle.step(&primary()), Cue::None);
        assert_eq!(
            idle.events.as_slice(),
            &[Event::Refused(Pos::new(1, 2), Refusal::NeedRecords)]
        );
        // Over a whole legal career, every accepted press matches the preview.
        let mut g = Game::new();
        let mut checked = 0;
        for t in 0..120_000 {
            let input = g.pilot(t);
            let live = g.phase == Phase::Infiltrating && g.work.is_none() && input.primary;
            let preview = g.interaction();
            let heist = g.level;
            g.step(&input);
            if live {
                checked += 1;
                match preview {
                    Some((_, b'E')) => assert!(g.level > heist || g.phase != Phase::Infiltrating),
                    Some((target, kind)) => {
                        assert!(
                            g.work.is_some_and(|w| w.target == target && w.kind == kind)
                                || g.events.as_slice().contains(&Event::WorkDone(target, kind))
                        )
                    }
                    None => assert!(g.work.is_none()),
                }
            }
            if g.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(g.outcome(), Outcome::Won);
        assert!(checked > 30, "{checked}");
    }
    #[test]
    fn events_record_one_tick_of_accepted_play() {
        let mut g = arena();
        sentry(&mut g, Pos::new(5, 2), (-1, 0));
        g.step(&Input::default());
        assert!(g.seen);
        assert_eq!(g.events.as_slice(), &[Event::Spotted(0), Event::MedalLost]);
        // Still watched: no repeated alert, and the list is cleared each tick.
        g.step(&Input::default());
        assert!(g.events.as_slice().is_empty());
        // A beam crossing and the first alarm are both reported.
        let mut beam = arena();
        beam.disabled = false;
        beam.tiles[Pos::new(3, 2).index()] = b'!';
        beam.step(&Input {
            right: true,
            ..Input::default()
        });
        assert_eq!(
            beam.events.as_slice(),
            &[Event::Beam(Pos::new(3, 2)), Event::MedalLost]
        );
        // Capture names its cause.
        let mut late = arena();
        late.elapsed = late.content().seconds * 60 - 1;
        assert_eq!(late.step(&Input::default()), Cue::Bad);
        assert_eq!(late.cause, Cause::Time);
        assert_eq!(late.events.as_slice(), &[Event::Caught(Cause::Time)]);
        // A fresh career and its inert briefing carry no events.
        let mut fresh = Game::new();
        assert!(fresh.events.as_slice().is_empty());
        fresh.step(&Input::default());
        assert!(fresh.events.as_slice().is_empty());
    }
}
