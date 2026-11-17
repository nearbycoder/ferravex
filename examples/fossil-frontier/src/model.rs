//! Pure excavation, identification and expedition rules.
//!
//!     use fossil_frontier::{Game, Phase};
//!     use ferravex_arcade::{Input, Outcome};
//!     let mut game = Game::new();
//!     assert_eq!(game.phase(), Phase::Briefing);
//!     game.step(&Input { primary:true, ..Input::default() });
//!     assert_eq!(game.phase(), Phase::Field);
//!     let supplies = game.supplies();
//!     game.step(&Input { secondary:true, ..Input::default() }); // Survey.
//!     assert_eq!(game.supplies(), supplies - 1);
//!     assert_eq!(game.outcome(), Outcome::Playing);
use crate::content::{SITES, SPECIES, Site};
use ferravex_arcade::{Cue, Input, Outcome};
use ferravex_math::Vec2;
pub(crate) const COLS: usize = 8;
pub(crate) const ROWS: usize = 6;
pub(crate) const CELLS: usize = COLS * ROWS;
pub(crate) const GRID_X: f32 = 28.;
pub(crate) const GRID_Y: f32 = 180.;
pub(crate) const PITCH_X: f32 = 68.;
pub(crate) const PITCH_Y: f32 = 60.;
/// Dig-page action buttons as `[x, y, w, h]`, shared by drawing and clicks.
pub(crate) const DIG_BUTTON: [f32; 4] = [610., 306., 306., 34.];
pub(crate) const SURVEY_BUTTON: [f32; 4] = [610., 346., 306., 34.];
/// The Lab's species chooser: six equal chips across this strip.
pub(crate) const SPECIES_BAR: [f32; 4] = [332., 425., 586., 40.];
/// The setback dialog's Recover button.
pub(crate) const RECOVER_BUTTON: [f32; 4] = [182., 426., 596., 44.];
/// Atlas reference card `i` as `[x, y, w, h]`.
pub(crate) fn atlas_card(i: usize) -> [f32; 4] {
    [
        38. + (i % 3) as f32 * 304.,
        186. + (i / 3) as f32 * 178.,
        290.,
        168.,
    ]
}
/// Museum placard `i` as `[x, y, w, h]`.
pub(crate) fn placard(i: usize) -> [f32; 4] {
    [38. + i as f32 * 150., 448., 142., 78.]
}
/// Half-open hit test; non-finite points are never inside.
pub(crate) fn inside(p: Vec2, r: [f32; 4]) -> bool {
    p.x >= r[0] && p.x < r[0] + r[2] && p.y >= r[1] && p.y < r[1] + r[3]
}
/// Where an accepted or refused action happened, for presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Spot {
    Tile(usize),
    Lab,
    Camp(usize),
}
/// Why an action was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lack {
    /// Nothing to do there (cleared rock, a known cross, no sample).
    Nothing,
    Supplies(u16),
    Daylight(u16),
    Coins(u32),
    Renown(u16),
    /// No supply packs left at this camp.
    Stock,
    /// Supply storage is full.
    Storage,
    /// The upgrade track is complete.
    Maxed,
    /// The permit's last day is already under way.
    Days,
    /// Daylight is still full; resting would buy nothing.
    Morning,
    /// The signature part and another identification are still missing.
    Ready,
}
/// What one executed tick accepted or refused, recorded for presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// The permit briefing closed and fieldwork began.
    Began,
    Tool(usize),
    /// A survey centred on `at` revealed the cells in the `revealed` bit mask.
    Survey {
        at: usize,
        revealed: u64,
    },
    /// A stroke removed `removed` layers and cost the fossil `loss` condition.
    Cut {
        at: usize,
        removed: u8,
        loss: u8,
        exposed: bool,
        empty: bool,
    },
    /// An exposed specimen was jacketed into tray `slot`.
    Jacket {
        at: usize,
        slot: usize,
    },
    /// A destroyed specimen was lifted; nothing reached the tray.
    Ruined {
        at: usize,
    },
    Refused {
        spot: Spot,
        lack: Lack,
    },
    Identified {
        slot: usize,
        grant: u32,
        renown: u16,
    },
    /// Research rejected; the coins and renown actually lost.
    Rejected {
        slot: usize,
        coins: u32,
        renown: u16,
    },
    Rested {
        light: u16,
    },
    Bought,
    Upgraded(usize),
    /// Permit `site` (zero-based) closed with `medals`.
    Advanced {
        site: usize,
        medals: u8,
    },
    /// The final permit closed and the museum opened.
    Opened {
        medals: u8,
    },
    Recovered,
    Setback,
    Closed,
}
/// The events of the latest executed tick. [`Game::step`] clears it first and
/// the rules never read it, so it takes no part in equality: snapshots and
/// replays compare expedition state only.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Feed([Option<Event>; 4]);
impl PartialEq for Feed {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for Feed {}
impl Feed {
    fn clear(&mut self) {
        self.0 = [None; 4];
    }
    fn push(&mut self, event: Event) {
        if let Some(slot) = self.0.iter_mut().find(|e| e.is_none()) {
            *slot = Some(event);
        }
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = Event> + '_ {
        self.0.iter().flatten().copied()
    }
}
/// Expedition phases; the shared host separately owns pause, help and restart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Briefing,
    Field,
    Setback,
    Complete,
    Lost,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    Dig,
    Lab,
    Camp,
    Museum,
    Atlas,
}
impl Page {
    pub fn index(self) -> usize {
        match self {
            Self::Dig => 0,
            Self::Lab => 1,
            Self::Camp => 2,
            Self::Museum => 3,
            Self::Atlas => 4,
        }
    }
    fn slot(n: usize) -> Option<Self> {
        Some(match n {
            1 => Self::Dig,
            2 => Self::Lab,
            3 => Self::Camp,
            4 => Self::Museum,
            5 => Self::Atlas,
            _ => return None,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cell {
    pub depth: u8,
    pub hard: u8,
    pub known: bool,
    pub find: Option<usize>,
    pub quality: u8,
    pub taken: bool,
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            depth: 0,
            hard: 0,
            known: false,
            find: None,
            quality: 100,
            taken: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Sample {
    pub kind: usize,
    pub quality: u8,
    pub identified: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Record {
    pub quality: u8,
    pub identified: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Start {
    coins: u32,
    score: u32,
    renown: u16,
    upgrades: [u8; 2],
    supplies: u16,
}
/// A six-permit career with fixed-capacity field, sample tray and museum records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub(crate) phase: Phase,
    pub(crate) index: usize,
    pub(crate) page: Page,
    pub(crate) cells: [Cell; CELLS],
    pub(crate) samples: [Option<Sample>; 4],
    pub(crate) museum: [Record; 24],
    pub(crate) cursor: usize,
    pub(crate) tool: usize,
    pub(crate) sample: usize,
    pub(crate) choice: usize,
    pub(crate) row: usize,
    pub(crate) coins: u32,
    pub(crate) kits: u16,
    pub(crate) daylight: u16,
    pub(crate) day: u8,
    pub(crate) packs: u8,
    pub(crate) upgrades: [u8; 2],
    pub(crate) renown: u16,
    pub(crate) retries: u8,
    pub(crate) medals: [u8; 6],
    pub(crate) score: u32,
    pub(crate) strikes: u16,
    pub(crate) feed: Feed,
    start: Start,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Open the first permit with 65 coins, 30 supply units and three recovery grants.
    pub fn new() -> Self {
        let mut g = Self {
            phase: Phase::Briefing,
            index: 0,
            page: Page::Dig,
            cells: [Cell::default(); CELLS],
            samples: [None; 4],
            museum: [Record::default(); 24],
            cursor: 0,
            tool: 0,
            sample: 0,
            choice: 0,
            row: 0,
            coins: 65,
            kits: 30,
            daylight: 54,
            day: 1,
            packs: 5,
            upgrades: [0; 2],
            renown: 0,
            retries: 3,
            medals: [0; 6],
            score: 0,
            strikes: 0,
            feed: Feed::default(),
            start: Start {
                coins: 65,
                score: 0,
                renown: 0,
                upgrades: [0; 2],
                supplies: 30,
            },
        };
        g.load();
        g
    }
    /// Current expedition phase.
    pub fn phase(&self) -> Phase {
        self.phase
    }
    /// One-based dig site, including the completed sixth permit.
    pub fn site(&self) -> usize {
        self.index + 1
    }
    /// Available expedition money. Purchases and grant recovery are atomic.
    pub fn coins(&self) -> u32 {
        self.coins
    }
    /// Remaining supply units for tools, surveys, extraction and research.
    pub fn supplies(&self) -> u16 {
        self.kits
    }
    /// Career score; recovering a permit rolls its earnings back.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// Total correctly identified specimens in the museum.
    pub fn identified(&self) -> usize {
        self.museum.iter().filter(|r| r.identified).count()
    }
    /// Number of identified signature skeleton parts.
    pub fn museum_pieces(&self) -> usize {
        (0..6).filter(|&i| self.museum[i * 4].identified).count()
    }
    /// Read the authoritative terminal result without advancing the model.
    pub fn outcome(&self) -> Outcome {
        match self.phase {
            Phase::Complete => Outcome::Won,
            Phase::Lost => Outcome::Lost,
            _ => Outcome::Playing,
        }
    }
    pub(crate) fn content(&self) -> &'static Site {
        &SITES[self.index]
    }
    /// The depth cell `i` had when this permit opened.
    pub(crate) fn initial_depth(&self, i: usize) -> u8 {
        self.content()
            .finds
            .iter()
            .find(|f| f.y * COLS + f.x == i)
            .map_or(
                2 + ((i * 7 + self.index * 3) % 4) as u8 + self.index as u8,
                |f| f.depth,
            )
    }
    fn load(&mut self) {
        let depths: [u8; CELLS] = core::array::from_fn(|i| self.initial_depth(i));
        self.cells = [Cell::default(); CELLS];
        for (c, depth) in self.cells.iter_mut().zip(depths) {
            c.depth = depth;
            c.hard = SITES[self.index].rock;
        }
        for (n, f) in self.content().finds.iter().enumerate() {
            self.cells[f.y * COLS + f.x].find = Some(n);
        }
        self.samples = [None; 4];
        self.cursor = 0;
        self.tool = 0;
        self.sample = 0;
        self.choice = 0;
        self.row = 0;
        self.page = Page::Dig;
        self.day = 1;
        self.daylight = self.content().daylight;
        self.packs = 5;
        self.phase = Phase::Briefing;
        self.start = Start {
            coins: self.coins,
            score: self.score,
            renown: self.renown,
            upgrades: self.upgrades,
            supplies: self.kits,
        };
    }
    pub(crate) fn ready(&self) -> bool {
        self.samples[0].is_some_and(|s| s.identified)
            && self
                .samples
                .iter()
                .flatten()
                .filter(|s| s.identified)
                .count()
                >= 2
    }
    pub(crate) fn tool_stats(&self, at: usize) -> (u8, u16, u16) {
        self.tool_power(self.tool, at)
    }
    /// Layers removed, supplies and daylight for one stroke of `tool` at `at`.
    pub(crate) fn tool_power(&self, tool: usize, at: usize) -> (u8, u16, u16) {
        let hard = self.cells[at].hard;
        match tool {
            0 => (1, 1, 3 - u16::from(self.upgrades[0])),
            1 => (2u8.saturating_sub(hard / 2).max(1), 2, 2),
            _ => ((4 + 2 * self.upgrades[1]).saturating_sub(hard).max(1), 3, 1),
        }
    }
    /// Condition one stroke of `tool` would cost the fossil at `at`: zero for
    /// the brush, for cuts that stop above the fossil and for empty rock.
    pub(crate) fn cut_loss(&self, at: usize, tool: usize) -> u8 {
        let c = self.cells[at];
        let (power, _, _) = self.tool_power(tool, at);
        match c.find {
            Some(n) if power >= c.depth && tool > 0 => {
                let base = u16::from(self.content().finds[n].fragile)
                    + u16::from(power - c.depth) * 10
                    + if tool == 2 { 20 } else { 0 };
                base.saturating_sub(u16::from(self.upgrades[0]) * 8)
                    .min(100) as u8
            }
            _ => 0,
        }
    }
    /// Medals the tray's identified specimens would earn this permit.
    pub(crate) fn permit_medals(&self) -> u8 {
        let (n, total) = self
            .samples
            .iter()
            .flatten()
            .filter(|s| s.identified)
            .fold((0_u16, 0_u16), |(n, t), s| {
                (n + 1, t + u16::from(s.quality))
            });
        if n == 0 {
            return 0;
        }
        let quality = total / n;
        if n == 4 && quality >= 90 {
            3
        } else if n >= 3 && quality >= 60 {
            2
        } else {
            1
        }
    }
    /// Score for closing this permit with `medals`.
    pub(crate) fn permit_score(&self, medals: u8) -> u32 {
        1000 + u32::from(self.daylight) * 5 + u32::from(medals) * 200
    }
    fn refuse(&mut self, spot: Spot, lack: Lack) -> Cue {
        self.feed.push(Event::Refused { spot, lack });
        Cue::None
    }
    fn pay(&mut self, kits: u16, time: u16, spot: Spot) -> bool {
        let lack = if self.kits < kits {
            Lack::Supplies(kits)
        } else if self.daylight < time {
            Lack::Daylight(time)
        } else {
            self.kits -= kits;
            self.daylight -= time;
            return true;
        };
        self.refuse(spot, lack);
        false
    }
    /// Whether cell `i` lies in the survey cross centred on cell `at`.
    pub(crate) fn in_cross(at: usize, i: usize) -> bool {
        (i % COLS).abs_diff(at % COLS) + (i / COLS).abs_diff(at / COLS) <= 1
    }
    fn survey(&mut self) -> Cue {
        let at = self.cursor;
        let spot = Spot::Tile(at);
        if !(0..CELLS).any(|i| Self::in_cross(at, i) && !self.cells[i].known) {
            return self.refuse(spot, Lack::Nothing);
        }
        if !self.pay(1, 2, spot) {
            return Cue::None;
        }
        let mut revealed = 0_u64;
        for (i, c) in self.cells.iter_mut().enumerate() {
            if Self::in_cross(at, i) {
                if !c.known {
                    revealed |= 1 << i;
                }
                c.known = true;
            }
        }
        self.feed.push(Event::Survey { at, revealed });
        Cue::Action
    }
    fn dig(&mut self) -> Cue {
        let at = self.cursor;
        let spot = Spot::Tile(at);
        let c = self.cells[at];
        if c.taken || (c.depth == 0 && c.find.is_none()) {
            return self.refuse(spot, Lack::Nothing);
        }
        if c.depth == 0 {
            let Some(n) = c.find else { return Cue::None };
            if !self.pay(1, 2, spot) {
                return Cue::None;
            }
            self.cells[at].taken = true;
            if c.quality == 0 {
                self.feed.push(Event::Ruined { at });
                return Cue::Bad;
            }
            self.samples[n] = Some(Sample {
                kind: self.content().finds[n].kind,
                quality: c.quality,
                identified: false,
            });
            self.sample = n;
            self.feed.push(Event::Jacket { at, slot: n });
            return Cue::Good;
        }
        let (power, kits, time) = self.tool_stats(at);
        let loss = self.cut_loss(at, self.tool);
        if !self.pay(kits, time, spot) {
            return Cue::None;
        }
        let cell = &mut self.cells[at];
        cell.depth = cell.depth.saturating_sub(power);
        self.strikes += 1;
        if loss > 0 {
            self.cells[at].quality = c.quality.saturating_sub(loss);
        }
        let cell = self.cells[at];
        if cell.depth == 0 {
            self.cells[at].known = true;
        }
        self.feed.push(Event::Cut {
            at,
            removed: c.depth - cell.depth,
            loss: c.quality - cell.quality,
            exposed: cell.depth == 0 && c.find.is_some(),
            empty: cell.depth == 0 && c.find.is_none(),
        });
        if c.find.is_some() && cell.quality < c.quality {
            Cue::Bad
        } else {
            Cue::Action
        }
    }
    fn research(&mut self) -> Cue {
        let slot = self.sample;
        let Some(s) = self.samples[slot].filter(|s| !s.identified) else {
            return self.refuse(Spot::Lab, Lack::Nothing);
        };
        if !self.pay(1, 2, Spot::Lab) {
            return Cue::None;
        }
        if self.choice != s.kind {
            let (coins, renown) = (self.coins.min(6), self.renown.min(1));
            self.coins -= coins;
            self.renown -= renown;
            self.feed.push(Event::Rejected {
                slot,
                coins,
                renown,
            });
            return Cue::Bad;
        }
        self.samples[slot].as_mut().unwrap().identified = true;
        self.museum[self.index * 4 + slot] = Record {
            quality: s.quality,
            identified: true,
        };
        let grant = SPECIES[s.kind].value * u32::from(s.quality) / 100;
        let renown = if s.quality >= 80 { 3 } else { 1 };
        self.coins += grant;
        self.renown += renown;
        self.score += grant * 12 + u32::from(s.quality) * 2;
        self.feed.push(Event::Identified {
            slot,
            grant,
            renown,
        });
        Cue::Good
    }
    fn recover(&mut self) -> Cue {
        if self.retries == 0 {
            self.phase = Phase::Lost;
            self.feed.push(Event::Closed);
            return Cue::Bad;
        }
        self.retries -= 1;
        self.coins = self.start.coins;
        self.score = self.start.score;
        self.renown = self.start.renown;
        self.upgrades = self.start.upgrades;
        self.kits = self.start.supplies;
        for r in &mut self.museum[self.index * 4..self.index * 4 + 4] {
            *r = Record::default();
        }
        self.load();
        self.feed.push(Event::Recovered);
        Cue::Action
    }
    pub(crate) fn upgrade_cost(&self, which: usize) -> u32 {
        (if which == 0 { 35 } else { 40 }) + u32::from(self.upgrades[which]) * 35
    }
    /// Why camp action `row` would be refused right now, if it would be.
    pub(crate) fn camp_lack(&self, row: usize) -> Option<Lack> {
        match row {
            0 if self.day >= 3 => Some(Lack::Days),
            0 if self.daylight == self.content().daylight => Some(Lack::Morning),
            0 if self.coins < 12 => Some(Lack::Coins(12)),
            1 if self.packs == 0 => Some(Lack::Stock),
            1 if self.coins < 10 => Some(Lack::Coins(10)),
            1 if self.kits > 108 => Some(Lack::Storage),
            2 | 3 => {
                let u = row - 2;
                let cost = self.upgrade_cost(u);
                let need = 3 + u16::from(self.upgrades[u]) * 3;
                if self.upgrades[u] >= 2 {
                    Some(Lack::Maxed)
                } else if self.coins < cost {
                    Some(Lack::Coins(cost))
                } else if self.renown < need {
                    Some(Lack::Renown(need))
                } else {
                    None
                }
            }
            4 if !self.ready() => Some(Lack::Ready),
            _ => None,
        }
    }
    fn camp(&mut self) -> Cue {
        if let Some(lack) = self.camp_lack(self.row) {
            return self.refuse(Spot::Camp(self.row), lack);
        }
        match self.row {
            0 => {
                self.coins -= 12;
                self.day += 1;
                let light = self.content().daylight - self.daylight;
                self.daylight = self.content().daylight;
                self.feed.push(Event::Rested { light });
                Cue::Good
            }
            1 => {
                self.coins -= 10;
                self.kits += 12;
                self.packs -= 1;
                self.feed.push(Event::Bought);
                Cue::Good
            }
            2 | 3 => {
                let u = self.row - 2;
                self.coins -= self.upgrade_cost(u);
                self.upgrades[u] += 1;
                self.feed.push(Event::Upgraded(u));
                Cue::Good
            }
            4 => {
                let medals = self.permit_medals();
                self.medals[self.index] = medals;
                self.score += self.permit_score(medals);
                self.coins += 30;
                if self.index == 5 {
                    self.phase = Phase::Complete;
                    self.score += 3000 + u32::from(self.renown) * 20;
                    self.feed.push(Event::Opened { medals });
                    Cue::Win
                } else {
                    self.feed.push(Event::Advanced {
                        site: self.index,
                        medals,
                    });
                    self.index += 1;
                    self.kits = (self.kits + 16).min(120);
                    self.load();
                    Cue::Good
                }
            }
            5 => self.recover(),
            _ => Cue::None,
        }
    }
    fn assess(&mut self) -> Cue {
        if self.phase != Phase::Field || self.ready() {
            return Cue::None;
        }
        let signature = &self.content().finds[0];
        let c = self.cells[signature.y * COLS + signature.x];
        let hopeless = (c.quality == 0)
            || (self.kits == 0 && self.coins < 10)
            || (self.daylight == 0 && (self.day >= 3 || self.coins < 12));
        if hopeless {
            self.phase = Phase::Setback;
            self.feed.push(Event::Setback);
            Cue::Bad
        } else {
            Cue::None
        }
    }
    /// Execute one fixed tick. Only fresh command edges cause field work or spending.
    pub fn step(&mut self, input: &Input) -> Cue {
        self.feed.clear();
        if self.outcome() != Outcome::Playing {
            return Cue::None;
        }
        match self.phase {
            Phase::Briefing => {
                if input.primary || input.click {
                    self.phase = Phase::Field;
                    self.feed.push(Event::Began);
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Setback => {
                // Its button spends the grant exactly as Space does.
                let button = input.click && inside(input.pointer, RECOVER_BUTTON);
                return if input.primary || button {
                    self.recover()
                } else {
                    Cue::None
                };
            }
            _ => {}
        }
        if let Some(page) = input.slot.and_then(Page::slot)
            && self.page != page
        {
            self.page = page;
            return Cue::Action;
        }
        if input.click
            && (112. ..=146.).contains(&input.pointer.y)
            && (20. ..940.).contains(&input.pointer.x)
        {
            let page =
                Page::slot(((input.pointer.x - 20.) / 184.) as usize + 1).unwrap_or(self.page);
            if page == self.page {
                return Cue::None;
            }
            self.page = page;
            return Cue::Action;
        }
        let mut cue = Cue::None;
        match self.page {
            Page::Dig => {
                let x = self.cursor % COLS;
                let y = self.cursor / COLS;
                let nx = if input.left_pressed {
                    x.saturating_sub(1)
                } else if input.right_pressed {
                    (x + 1).min(COLS - 1)
                } else {
                    x
                };
                let ny = if input.up_pressed {
                    y.saturating_sub(1)
                } else if input.down_pressed {
                    (y + 1).min(ROWS - 1)
                } else {
                    y
                };
                if nx != x || ny != y {
                    self.cursor = ny * COLS + nx;
                    cue = Cue::Action
                }
                // The panel's Dig and Survey buttons take the same rule path
                // as Space and X.
                let mut button = None;
                if input.click {
                    if let Some(at) = Self::tile_at(input.pointer) {
                        if self.cursor == at {
                            return Cue::None;
                        }
                        self.cursor = at;
                        return Cue::Action;
                    }
                    if (614. ..=918.).contains(&input.pointer.x)
                        && (390. ..525.).contains(&input.pointer.y)
                    {
                        let tool = ((input.pointer.y - 390.) / 45.) as usize;
                        if self.tool == tool {
                            return Cue::None;
                        }
                        self.tool = tool;
                        self.feed.push(Event::Tool(tool));
                        return Cue::Action;
                    }
                    button = [DIG_BUTTON, SURVEY_BUTTON]
                        .iter()
                        .position(|r| inside(input.pointer, *r));
                }
                if input.tertiary {
                    self.tool = (self.tool + 1) % 3;
                    self.feed.push(Event::Tool(self.tool));
                    return Cue::Action;
                }
                if input.secondary || button == Some(1) {
                    cue = self.survey()
                } else if input.primary || button == Some(0) {
                    cue = self.dig()
                }
            }
            Page::Lab => {
                if input.up_pressed && self.sample > 0 {
                    self.sample = self.sample.saturating_sub(1);
                    cue = Cue::Action
                }
                if input.down_pressed && self.sample < 3 {
                    self.sample = (self.sample + 1).min(3);
                    cue = Cue::Action
                }
                if input.left_pressed {
                    self.choice = (self.choice + 5) % 6;
                    cue = Cue::Action
                }
                if input.right_pressed {
                    self.choice = (self.choice + 1) % 6;
                    cue = Cue::Action
                }
                if input.click {
                    if (30. ..=310.).contains(&input.pointer.x)
                        && (190. ..=470.).contains(&input.pointer.y)
                    {
                        let sample = (((input.pointer.y - 190.) / 70.) as usize).min(3);
                        if self.sample == sample {
                            return Cue::None;
                        }
                        self.sample = sample;
                        return Cue::Action;
                    }
                    if (332. ..=918.).contains(&input.pointer.x)
                        && (425. ..=465.).contains(&input.pointer.y)
                    {
                        // Each chip names one species, as Left/Right would.
                        let [x, _, w, _] = SPECIES_BAR;
                        let pick = (((input.pointer.x - x) / (w / 6.)) as usize).min(5);
                        if self.choice == pick {
                            return Cue::None;
                        }
                        self.choice = pick;
                        return Cue::Action;
                    }
                    if (332. ..=918.).contains(&input.pointer.x)
                        && (480. ..=522.).contains(&input.pointer.y)
                    {
                        cue = self.research()
                    }
                } else if input.primary {
                    cue = self.research()
                }
            }
            Page::Camp => {
                if input.up_pressed && self.row > 0 {
                    self.row = self.row.saturating_sub(1);
                    cue = Cue::Action
                }
                if input.down_pressed && self.row < 5 {
                    self.row = (self.row + 1).min(5);
                    cue = Cue::Action
                }
                if input.click
                    && (50. ..=566.).contains(&input.pointer.x)
                    && (194. ..=530.).contains(&input.pointer.y)
                {
                    self.row = (((input.pointer.y - 194.) / 56.) as usize).min(5);
                    cue = self.camp()
                } else if input.primary {
                    cue = self.camp()
                }
            }
            Page::Museum | Page::Atlas => {
                // Clicking a placard or reference card highlights it, as the
                // arrow keys do.
                let atlas = self.page == Page::Atlas;
                let card = if atlas { atlas_card } else { placard };
                if input.click
                    && let Some(i) = (0..6).find(|&i| inside(input.pointer, card(i)))
                {
                    if self.choice == i {
                        return Cue::None;
                    }
                    self.choice = i;
                    return Cue::Action;
                }
                if input.left_pressed || (atlas && input.up_pressed) {
                    self.choice = (self.choice + 5) % 6;
                    cue = Cue::Action
                }
                if input.right_pressed || (atlas && input.down_pressed) {
                    self.choice = (self.choice + 1) % 6;
                    cue = Cue::Action
                }
            }
        }
        let failed = self.assess();
        if failed == Cue::Bad { failed } else { cue }
    }
    pub(crate) fn tile_at(p: Vec2) -> Option<usize> {
        if !p.is_finite()
            || !(GRID_X..GRID_X + PITCH_X * COLS as f32).contains(&p.x)
            || !(GRID_Y..GRID_Y + PITCH_Y * ROWS as f32).contains(&p.y)
        {
            return None;
        }
        Some(((p.y - GRID_Y) / PITCH_Y) as usize * COLS + ((p.x - GRID_X) / PITCH_X) as usize)
    }
    fn tab(page: Page) -> Input {
        Input {
            slot: Some(page.index() + 1),
            ..Input::default()
        }
    }
    fn target(at: usize) -> Input {
        Input {
            click: true,
            pointer: Vec2::new(
                GRID_X + (at % COLS) as f32 * PITCH_X + 20.,
                GRID_Y + (at / COLS) as f32 * PITCH_Y + 20.,
            ),
            ..Input::default()
        }
    }
    fn camp_command(&self, row: usize) -> Input {
        if self.page != Page::Camp {
            return Self::tab(Page::Camp);
        }
        if self.row < row {
            Input {
                down_pressed: true,
                ..Input::default()
            }
        } else if self.row > row {
            Input {
                up_pressed: true,
                ..Input::default()
            }
        } else {
            Input {
                primary: true,
                ..Input::default()
            }
        }
    }
    /// Read-only legal survey/excavation/research controls for repeatable demonstrations.
    pub fn pilot(&self, tick: u64) -> Input {
        if !tick.is_multiple_of(12) {
            return Input::default();
        }
        if self.phase != Phase::Field {
            return Input {
                primary: true,
                ..Input::default()
            };
        }
        let complete = self
            .samples
            .iter()
            .flatten()
            .filter(|s| s.identified)
            .count()
            == 4;
        if complete {
            for u in 0..2 {
                if self.upgrades[u] < 2
                    && self.coins >= self.upgrade_cost(u) + 25
                    && self.renown >= 3 + u16::from(self.upgrades[u]) * 3
                {
                    return self.camp_command(u + 2);
                }
            }
            return self.camp_command(4);
        }
        if self.daylight < 4 {
            return self.camp_command(0);
        }
        if self.kits < 6 {
            return self.camp_command(1);
        }
        if let Some(n) = self
            .samples
            .iter()
            .position(|s| s.is_some_and(|s| !s.identified))
        {
            if self.page != Page::Lab {
                return Self::tab(Page::Lab);
            }
            if self.sample < n {
                return Input {
                    down_pressed: true,
                    ..Input::default()
                };
            }
            if self.sample > n {
                return Input {
                    up_pressed: true,
                    ..Input::default()
                };
            }
            if self.choice != self.samples[n].unwrap().kind {
                return Input {
                    right_pressed: true,
                    ..Input::default()
                };
            }
            return Input {
                primary: true,
                ..Input::default()
            };
        }
        if self.page != Page::Dig {
            return Self::tab(Page::Dig);
        }
        if let Some(at) = self
            .cells
            .iter()
            .position(|c| c.known && c.find.is_some() && !c.taken)
        {
            if self.cursor != at {
                return Self::target(at);
            }
            let c = self.cells[at];
            let drill = (4 + 2 * self.upgrades[1]).saturating_sub(c.hard).max(1);
            let chisel = 2u8.saturating_sub(c.hard / 2).max(1);
            let tool = if c.depth > drill {
                2
            } else if c.depth > chisel && c.depth > 2 {
                1
            } else {
                0
            };
            if self.tool != tool {
                return Input {
                    tertiary: true,
                    ..Input::default()
                };
            }
            return Input {
                primary: true,
                ..Input::default()
            };
        }
        if let Some(at) = self.cells.iter().position(|c| !c.known) {
            if self.cursor != at {
                Self::target(at)
            } else {
                Input {
                    secondary: true,
                    ..Input::default()
                }
            }
        } else {
            self.camp_command(4)
        }
    }
    /// One-sentence career report; safe to call during an unfinished permit.
    pub fn summary(&self) -> String {
        let sites = self.medals.iter().filter(|&&m| m > 0).count();
        let medals = self.medals.iter().map(|&m| u32::from(m)).sum::<u32>();
        let found = self.identified();
        if self.outcome() == Outcome::Won {
            format!(
                "The museum opened with {found} specimens, {medals} medals and {} points.",
                self.score
            )
        } else {
            format!(
                "Completed {sites} of 6 sites with {found} specimens, {medals} medals and {} points.",
                self.score
            )
        }
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
    fn active() -> Game {
        let mut g = Game::new();
        assert_eq!(g.step(&primary()), Cue::Action);
        g
    }
    fn at_find(g: &mut Game, n: usize) {
        let f = g.content().finds[n];
        g.step(&Game::target(f.y * COLS + f.x));
    }
    fn excavate(g: &mut Game, n: usize) {
        at_find(g, n);
        g.tool = 0;
        while g.cells[g.cursor].depth > 0 {
            assert_eq!(g.step(&primary()), Cue::Action);
        }
        assert_eq!(g.step(&primary()), Cue::Good);
    }
    fn identify(g: &mut Game, n: usize) {
        g.step(&Game::tab(Page::Lab));
        g.sample = n;
        g.choice = g.samples[n].unwrap().kind;
        assert_eq!(g.step(&primary()), Cue::Good);
    }
    #[test]
    fn legal_pilot_completes_all_six_permits_and_replays_identically() {
        let mut g = Game::new();
        let mut replay = Game::new();
        let mut visits = [false; 6];
        let mut won_at = None;
        for tick in 0..20_000 {
            visits[g.site() - 1] = true;
            let input = g.pilot(tick);
            assert_eq!(g.step(&input), replay.step(&input));
            assert_eq!(g, replay, "replay diverged at {tick}");
            assert!(g.kits <= 120);
            assert!(g.day <= 3);
            assert!(g.daylight <= g.content().daylight);
            if g.outcome() != Outcome::Playing {
                won_at = Some(tick);
                break;
            }
        }
        assert_eq!(g.outcome(), Outcome::Won, "{g:?}");
        assert_eq!(g.identified(), 24);
        assert_eq!(g.museum_pieces(), 6);
        assert_eq!(g.medals, [3; 6]);
        assert_eq!(g.retries, 3);
        assert!(visits.into_iter().all(|v| v));
        assert!(g.museum.iter().all(|r| r.identified && r.quality == 100));
        assert_eq!(won_at, Some(7752));
    }
    #[test]
    fn surveys_reveal_cross_and_repeat_costs_nothing() {
        let mut g = active();
        let before = (g.kits, g.daylight);
        assert_eq!(
            g.step(&Input {
                secondary: true,
                ..Input::default()
            }),
            Cue::Action
        );
        assert_eq!((g.kits, g.daylight), (before.0 - 1, before.1 - 2));
        let known: Vec<_> = g
            .cells
            .iter()
            .enumerate()
            .filter(|(_, c)| c.known)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(known, [0, 1, 8]);
        let snapshot = g.clone();
        for _ in 0..100 {
            assert_eq!(
                g.step(&Input {
                    secondary: true,
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!(g, snapshot);
    }
    #[test]
    fn brush_preserves_fossils_and_extraction_is_separate_and_once() {
        let mut g = active();
        at_find(&mut g, 0);
        let (kits, time) = (g.kits, g.daylight);
        let depth = g.cells[g.cursor].depth;
        for _ in 0..depth {
            assert_eq!(g.step(&primary()), Cue::Action);
        }
        assert!(g.samples[0].is_none());
        assert_eq!(g.cells[g.cursor].quality, 100);
        assert_eq!(
            (g.kits, g.daylight),
            (kits - u16::from(depth), time - u16::from(depth) * 3)
        );
        assert_eq!(g.step(&primary()), Cue::Good);
        assert_eq!(g.samples[0].unwrap().quality, 100);
        assert_eq!(
            (g.kits, g.daylight),
            (kits - u16::from(depth) - 1, time - u16::from(depth) * 3 - 2)
        );
        let snapshot = g.clone();
        for _ in 0..30 {
            assert_eq!(g.step(&primary()), Cue::None);
        }
        assert_eq!(g, snapshot);
    }
    #[test]
    fn rough_tools_have_distinct_costs_and_damage_when_touching_bone() {
        let mut chisel = active();
        at_find(&mut chisel, 0);
        chisel.tool = 1;
        assert_eq!(chisel.step(&primary()), Cue::Action);
        assert_eq!(
            (
                chisel.kits,
                chisel.daylight,
                chisel.cells[chisel.cursor].depth
            ),
            (28, 52, 1)
        );
        assert_eq!(chisel.cells[chisel.cursor].quality, 100);
        assert_eq!(chisel.step(&primary()), Cue::Bad);
        assert_eq!(chisel.cells[chisel.cursor].quality, 68);
        let mut drill = active();
        at_find(&mut drill, 0);
        drill.tool = 2;
        assert_eq!(drill.step(&primary()), Cue::Bad);
        assert_eq!(
            (drill.kits, drill.daylight, drill.cells[drill.cursor].depth),
            (27, 53, 0)
        );
        assert_eq!(drill.cells[drill.cursor].quality, 48);
    }
    #[test]
    fn hardness_and_upgrades_change_tools_without_changing_safe_brush_depth() {
        let mut g = active();
        g.cells[g.cursor].hard = 3;
        g.tool = 1;
        assert_eq!(g.tool_stats(g.cursor), (1, 2, 2));
        g.tool = 2;
        assert_eq!(g.tool_stats(g.cursor), (1, 3, 1));
        g.upgrades[1] = 2;
        assert_eq!(g.tool_stats(g.cursor), (5, 3, 1));
        g.tool = 0;
        assert_eq!(g.tool_stats(g.cursor), (1, 1, 3));
        g.upgrades[0] = 2;
        assert_eq!(g.tool_stats(g.cursor), (1, 1, 1));
        let mut protected = active();
        at_find(&mut protected, 0);
        protected.tool = 2;
        protected.upgrades[0] = 2;
        protected.step(&primary());
        assert_eq!(protected.cells[protected.cursor].quality, 64);
    }
    #[test]
    fn insufficient_resources_never_half_apply_a_dig() {
        let mut g = active();
        at_find(&mut g, 0);
        g.tool = 2;
        g.kits = 2;
        let cell = g.cells[g.cursor];
        let time = g.daylight;
        assert_eq!(g.step(&primary()), Cue::None);
        assert_eq!(g.kits, 2);
        assert_eq!(g.daylight, time);
        assert_eq!(g.cells[g.cursor], cell);
        g.kits = 30;
        g.daylight = 1;
        g.tool = 0;
        assert_eq!(g.step(&primary()), Cue::None);
        assert_eq!(g.kits, 30);
        assert_eq!(g.daylight, 1);
        assert_eq!(g.cells[g.cursor], cell);
    }
    #[test]
    fn identification_requires_evidence_and_pays_only_once() {
        let mut g = active();
        excavate(&mut g, 0);
        g.step(&Game::tab(Page::Lab));
        g.choice = 0;
        let (coins, kits, daylight) = (g.coins, g.kits, g.daylight);
        assert_eq!(g.step(&primary()), Cue::Bad);
        assert_eq!(
            (g.coins, g.kits, g.daylight),
            (coins - 6, kits - 1, daylight - 2)
        );
        assert_eq!(g.identified(), 0);
        assert!(!g.samples[0].unwrap().identified);
        g.choice = 5;
        assert_eq!(g.step(&primary()), Cue::Good);
        assert_eq!(g.coins, coins - 6 + 65);
        assert_eq!(g.renown, 3);
        assert_eq!(g.identified(), 1);
        assert_eq!(g.museum_pieces(), 1);
        let snapshot = g.clone();
        for _ in 0..50 {
            assert_eq!(g.step(&primary()), Cue::None);
        }
        assert_eq!(g, snapshot);
    }
    #[test]
    fn damaged_research_earns_lower_funding_score_and_renown() {
        let mut g = active();
        at_find(&mut g, 0);
        g.tool = 2;
        assert_eq!(g.step(&primary()), Cue::Bad);
        assert_eq!(g.step(&primary()), Cue::Good);
        identify(&mut g, 0);
        assert_eq!(g.museum[0].quality, 48);
        assert_eq!(g.coins, 65 + 31);
        assert_eq!(g.renown, 1);
        assert_eq!(g.score, 31 * 12 + 96);
    }
    #[test]
    fn held_inputs_and_invalid_navigation_are_inert() {
        let mut g = active();
        let snapshot = g.clone();
        let input = Input {
            left: true,
            right: true,
            up: true,
            down: true,
            primary_down: true,
            slot: Some(9),
            ..Input::default()
        };
        for _ in 0..300 {
            assert_eq!(g.step(&input), Cue::None);
        }
        assert_eq!(g, snapshot);
        assert_eq!(
            g.step(&Input {
                up_pressed: true,
                left_pressed: true,
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(g.step(&Game::target(0)), Cue::None);
        g.step(&Game::tab(Page::Lab));
        assert_eq!(
            g.step(&Input {
                up_pressed: true,
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(g.step(&primary()), Cue::None);
        g.step(&Game::tab(Page::Camp));
        assert_eq!(
            g.step(&Input {
                up_pressed: true,
                ..Input::default()
            }),
            Cue::None
        );
    }
    #[test]
    fn pointer_edges_never_escape_the_bounded_field() {
        assert_eq!(Game::tile_at(Vec2::new(GRID_X, GRID_Y)), Some(0));
        assert_eq!(Game::tile_at(Vec2::new(571.9, 539.9)), Some(CELLS - 1));
        for p in [
            Vec2::new(572., 200.),
            Vec2::new(30., 540.),
            Vec2::new(0., 0.),
            Vec2::new(f32::NAN, 200.),
            Vec2::new(100., f32::INFINITY),
        ] {
            assert_eq!(Game::tile_at(p), None);
        }
        let mut g = active();
        let snapshot = g.clone();
        for p in [
            Vec2::new(950., 400.),
            Vec2::new(f32::NAN, 200.),
            Vec2::new(100., 570.),
        ] {
            assert_eq!(
                g.step(&Input {
                    click: true,
                    pointer: p,
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!(g, snapshot);
    }
    #[test]
    fn camp_stock_storage_and_day_limits_are_enforced() {
        let mut g = active();
        g.coins = 200;
        g.step(&Game::tab(Page::Camp));
        assert_eq!(g.step(&primary()), Cue::None); // Cannot buy an untouched morning.
        g.daylight -= 1;
        assert_eq!(g.step(&primary()), Cue::Good);
        assert_eq!(g.day, 2);
        g.daylight -= 1;
        assert_eq!(g.step(&primary()), Cue::Good);
        assert_eq!(g.day, 3);
        g.daylight -= 1;
        let coins = g.coins;
        assert_eq!(g.step(&primary()), Cue::None);
        assert_eq!(g.coins, coins);
        g.row = 1;
        for _ in 0..5 {
            assert_eq!(g.step(&primary()), Cue::Good);
        }
        assert_eq!(g.packs, 0);
        let snapshot = g.clone();
        assert_eq!(g.step(&primary()), Cue::None);
        assert_eq!(g, snapshot);
        g.packs = 1;
        g.kits = 109;
        let coins = g.coins;
        assert_eq!(g.step(&primary()), Cue::None);
        assert_eq!(g.coins, coins);
        assert_eq!(g.kits, 109);
    }
    #[test]
    fn upgrades_require_funds_and_renown_and_are_capped() {
        let mut g = active();
        g.step(&Game::tab(Page::Camp));
        g.row = 2;
        assert_eq!(g.step(&primary()), Cue::None);
        assert_eq!(g.coins, 65);
        g.renown = 3;
        assert_eq!(g.step(&primary()), Cue::Good);
        assert_eq!(g.upgrades, [1, 0]);
        assert_eq!(g.coins, 30);
        g.coins = 150;
        assert_eq!(g.step(&primary()), Cue::None); // Renown gate for level two.
        g.renown = 6;
        assert_eq!(g.step(&primary()), Cue::Good);
        assert_eq!(g.upgrades, [2, 0]);
        assert_eq!(g.coins, 80);
        let snapshot = g.clone();
        assert_eq!(g.step(&primary()), Cue::None);
        assert_eq!(g, snapshot);
    }
    #[test]
    fn a_signature_and_another_identification_gate_the_next_site() {
        let mut g = active();
        excavate(&mut g, 0);
        identify(&mut g, 0);
        g.step(&Game::tab(Page::Camp));
        g.row = 4;
        assert_eq!(g.step(&primary()), Cue::None);
        assert_eq!(g.site(), 1);
        g.step(&Game::tab(Page::Dig));
        excavate(&mut g, 1);
        identify(&mut g, 1);
        g.step(&Game::tab(Page::Camp));
        g.row = 4;
        let (coins, kits) = (g.coins, g.kits);
        assert_eq!(g.step(&primary()), Cue::Good);
        assert_eq!(g.site(), 2);
        assert_eq!(g.phase, Phase::Briefing);
        assert_eq!(g.medals[0], 1);
        assert_eq!(g.coins, coins + 30);
        assert_eq!(g.kits, kits + 16);
        assert_eq!(g.identified(), 2);
        assert!(g.samples.iter().all(Option::is_none));
    }
    #[test]
    fn recovery_rolls_back_local_earnings_tools_and_finds_preserving_prior_sites() {
        let mut g = Game::new();
        for tick in 0..10_000 {
            g.step(&g.pilot(tick));
            if g.site() == 2 {
                break;
            }
        }
        assert_eq!(g.site(), 2);
        let saved = g.clone();
        for tick in 0..10_000 {
            g.step(&g.pilot(tick));
            if g.museum[4..8].iter().any(|r| r.identified) {
                break;
            }
        }
        assert!(g.identified() > saved.identified());
        g.step(&Game::tab(Page::Camp));
        assert_eq!(
            g.step(&Input {
                click: true,
                pointer: Vec2::new(70., 500.),
                ..Input::default()
            }),
            Cue::Action
        );
        assert_eq!(g.phase, Phase::Briefing);
        assert_eq!(g.retries, saved.retries - 1);
        assert_eq!(g.coins, saved.coins);
        assert_eq!(g.kits, saved.kits);
        assert_eq!(g.score, saved.score);
        assert_eq!(g.renown, saved.renown);
        assert_eq!(g.upgrades, saved.upgrades);
        assert_eq!(g.museum, saved.museum);
        assert_eq!(g.medals, saved.medals);
        assert_eq!(g.cells, saved.cells);
    }
    #[test]
    fn three_legal_recoveries_then_a_fourth_request_ends_career() {
        let mut g = Game::new();
        for remaining in (0..=2).rev() {
            g.step(&primary());
            g.step(&Game::tab(Page::Camp));
            assert_eq!(
                g.step(&Input {
                    click: true,
                    pointer: Vec2::new(70., 500.),
                    ..Input::default()
                }),
                Cue::Action
            );
            assert_eq!(g.retries, remaining);
            assert_eq!(g.coins, 65);
            assert_eq!(g.identified(), 0);
        }
        g.step(&primary());
        g.step(&Game::tab(Page::Camp));
        assert_eq!(
            g.step(&Input {
                click: true,
                pointer: Vec2::new(70., 500.),
                ..Input::default()
            }),
            Cue::Bad
        );
        assert_eq!(g.outcome(), Outcome::Lost);
    }
    #[test]
    fn insolvency_and_destroyed_signature_raise_recoverable_setbacks() {
        let mut broke = active();
        broke.kits = 1;
        broke.coins = 0;
        assert_eq!(
            broke.step(&Input {
                secondary: true,
                ..Input::default()
            }),
            Cue::Bad
        );
        assert_eq!(broke.phase, Phase::Setback);
        assert_eq!(broke.step(&primary()), Cue::Action);
        assert_eq!(broke.coins, 65);
        assert_eq!(broke.phase, Phase::Briefing);
        let mut g = active();
        at_find(&mut g, 0);
        g.cells[g.cursor].depth = 1;
        g.tool = 2;
        g.upgrades[1] = 2;
        assert_eq!(g.step(&primary()), Cue::Bad);
        assert_eq!(g.cells[g.cursor].quality, 0);
        assert_eq!(g.phase, Phase::Setback);
    }
    #[test]
    fn terminal_states_freeze_every_control_and_new_game_restarts() {
        let mut lost = active();
        lost.retries = 0;
        lost.step(&Game::tab(Page::Camp));
        lost.row = 5;
        lost.step(&primary());
        let mut won = Game::new();
        for tick in 0..20_000 {
            won.step(&won.pilot(tick));
            if won.outcome() == Outcome::Won {
                break;
            }
        }
        assert_eq!(won.outcome(), Outcome::Won);
        for mut g in [lost, won] {
            let snapshot = g.clone();
            let input = Input {
                primary: true,
                secondary: true,
                tertiary: true,
                up_pressed: true,
                down_pressed: true,
                left_pressed: true,
                right_pressed: true,
                click: true,
                pointer: Vec2::new(70., 500.),
                slot: Some(3),
                ..Input::default()
            };
            for _ in 0..60 {
                assert_eq!(g.step(&input), Cue::None);
            }
            assert_eq!(g, snapshot);
        }
        let restarted = Game::new();
        assert_eq!(restarted.outcome(), Outcome::Playing);
        assert_eq!(restarted.museum_pieces(), 0);
        assert_eq!(restarted.retries, 3);
    }
    #[test]
    fn authored_sites_have_unique_valid_specimens_and_distinct_geology() {
        assert_eq!(SITES.len(), 6);
        assert_eq!(SPECIES.len(), 6);
        for (i, site) in SITES.iter().enumerate() {
            assert_eq!(site.finds[0].kind, 5);
            assert!((46..=54).contains(&site.daylight));
            for (n, f) in site.finds.iter().enumerate() {
                assert!(f.x < COLS && f.y < ROWS && f.kind < SPECIES.len());
                assert!((2..=9).contains(&f.depth));
                assert!(
                    site.finds[..n]
                        .iter()
                        .all(|other| (other.x, other.y) != (f.x, f.y))
                );
            }
            assert!(SITES[..i].iter().all(|other| other.name != site.name
                && other.piece != site.piece
                && other.strata != site.strata));
        }
    }
    fn click_on(r: [f32; 4]) -> Input {
        Input {
            click: true,
            pointer: Vec2::new(r[0] + r[2] * 0.5, r[1] + r[3] * 0.5),
            ..Input::default()
        }
    }
    #[test]
    fn panel_buttons_take_the_same_rule_path_as_their_keys() {
        let survey = Input {
            secondary: true,
            ..Input::default()
        };
        let (mut keys, mut mouse) = (active(), active());
        assert_eq!(keys.step(&survey), mouse.step(&click_on(SURVEY_BUTTON)));
        assert_eq!(keys, mouse);
        for _ in 0..3 {
            assert_eq!(keys.step(&primary()), mouse.step(&click_on(DIG_BUTTON)));
            assert_eq!(keys, mouse);
        }
        // A surveyed cross refuses the button exactly as it refuses X.
        let snapshot = mouse.clone();
        assert_eq!(mouse.step(&click_on(SURVEY_BUTTON)), Cue::None);
        assert_eq!(mouse, snapshot);
        // The buttons exist only on the Dig page.
        mouse.step(&Game::tab(Page::Lab));
        let snapshot = mouse.clone();
        assert_eq!(mouse.step(&click_on(DIG_BUTTON)), Cue::None);
        assert_eq!(mouse, snapshot);
        // A setback's Recover button spends a grant like Space does.
        let mut broke = active();
        broke.kits = 1;
        broke.coins = 0;
        assert_eq!(broke.step(&survey), Cue::Bad);
        assert_eq!(broke.phase, Phase::Setback);
        assert_eq!(broke.step(&click_on(DIG_BUTTON)), Cue::None);
        assert_eq!(broke.step(&click_on(RECOVER_BUTTON)), Cue::Action);
        assert_eq!((broke.retries, broke.coins), (2, 65));
    }
    #[test]
    fn species_atlas_and_museum_clicks_choose_directly() {
        let mut g = active();
        g.step(&Game::tab(Page::Lab));
        let [x, _, w, _] = SPECIES_BAR;
        let chip = |i: usize| Input {
            click: true,
            pointer: Vec2::new(x + (i as f32 + 0.5) * w / 6., 445.),
            ..Input::default()
        };
        assert_eq!(g.step(&chip(3)), Cue::Action);
        assert_eq!(g.choice, 3);
        assert_eq!(g.step(&chip(3)), Cue::None);
        assert_eq!(g.step(&chip(5)), Cue::Action);
        assert_eq!(g.choice, 5);
        type Card = fn(usize) -> [f32; 4];
        for (page, card) in [(Page::Atlas, atlas_card as Card), (Page::Museum, placard)] {
            g.step(&Game::tab(page));
            for i in [2, 0, 4] {
                assert_eq!(g.step(&click_on(card(i))), Cue::Action);
                assert_eq!(g.choice, i);
                assert_eq!(g.step(&click_on(card(i))), Cue::None);
            }
        }
        g.step(&Input {
            right_pressed: true,
            ..Input::default()
        });
        assert_eq!(g.choice, 5, "arrow keys still browse");
    }
    #[test]
    fn feed_reports_only_the_latest_tick() {
        let mut g = active();
        let survey = Input {
            secondary: true,
            ..Input::default()
        };
        let events = |g: &Game| g.feed.iter().collect::<Vec<_>>();
        g.step(&survey);
        let revealed = 1 << 0 | 1 << 1 | 1 << 8;
        assert_eq!(events(&g), [Event::Survey { at: 0, revealed }]);
        g.step(&survey);
        let lack = Lack::Nothing;
        assert_eq!(
            events(&g),
            [Event::Refused {
                spot: Spot::Tile(0),
                lack
            }]
        );
        g.step(&Input::default());
        assert!(events(&g).is_empty());
        // A rough cut reports the layers it removed and the condition it cost.
        at_find(&mut g, 0);
        g.tool = 1;
        g.step(&primary());
        g.step(&primary());
        let cut = Event::Cut {
            at: 18,
            removed: 1,
            loss: 32,
            exposed: true,
            empty: false,
        };
        assert_eq!(events(&g), [cut]);
        assert_eq!(g.step(&primary()), Cue::Good);
        assert_eq!(events(&g), [Event::Jacket { at: 18, slot: 0 }]);
        // Camp refusals name the reason the rules gave.
        let mut camp = active();
        camp.step(&Game::tab(Page::Camp));
        for (row, lack) in [(0, Lack::Morning), (2, Lack::Renown(3)), (4, Lack::Ready)] {
            camp.row = row;
            let snapshot = camp.clone();
            assert_eq!(camp.step(&primary()), Cue::None);
            assert_eq!(camp, snapshot);
            let spot = Spot::Camp(row);
            assert_eq!(events(&camp), [Event::Refused { spot, lack }]);
        }
    }
    #[test]
    fn previews_match_what_the_rules_then_do() {
        for tool in 0..3 {
            let mut g = active();
            at_find(&mut g, 0);
            g.tool = tool;
            assert_eq!(g.initial_depth(g.cursor), 3);
            while g.cells[g.cursor].depth > 0 {
                let before = g.cells[g.cursor];
                let loss = g.cut_loss(g.cursor, tool);
                let (power, _, _) = g.tool_power(tool, g.cursor);
                g.step(&primary());
                let after = g.cells[g.cursor];
                assert_eq!(after.quality, before.quality.saturating_sub(loss));
                assert_eq!(after.depth, before.depth.saturating_sub(power));
            }
        }
        let mut g = active();
        g.step(&Game::tab(Page::Camp));
        g.daylight -= 1;
        for coins in [0, 11, 12, 40, 200] {
            for renown in [0, 3, 6] {
                for row in 0..6 {
                    let mut probe = g.clone();
                    (probe.coins, probe.renown, probe.row) = (coins, renown, row);
                    let lack = probe.camp_lack(row);
                    let cue = probe.step(&primary());
                    assert_eq!(lack.is_none(), cue != Cue::None, "{row} {coins} {renown}");
                }
            }
        }
        let mut g = active();
        excavate(&mut g, 0);
        identify(&mut g, 0);
        g.step(&Game::tab(Page::Dig));
        excavate(&mut g, 1);
        identify(&mut g, 1);
        let medals = g.permit_medals();
        let score = g.score + g.permit_score(medals);
        g.step(&Game::tab(Page::Camp));
        g.row = 4;
        assert_eq!(g.step(&primary()), Cue::Good);
        assert_eq!((g.medals[0], g.score), (medals, score));
    }
}
