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
    pub(crate) notice: &'static str,
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
            notice: "Survey with X. Colored traces reveal possible specimens.",
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
    fn load(&mut self) {
        self.cells = [Cell::default(); CELLS];
        for (i, c) in self.cells.iter_mut().enumerate() {
            c.depth = 2 + ((i * 7 + self.index * 3) % 4) as u8 + self.index as u8;
            c.hard = SITES[self.index].rock;
        }
        for (n, f) in self.content().finds.iter().enumerate() {
            let c = &mut self.cells[f.y * COLS + f.x];
            c.find = Some(n);
            c.depth = f.depth;
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
        self.notice = "X surveys this tile and its neighbors. Look for pale fossil traces.";
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
        let hard = self.cells[at].hard;
        match self.tool {
            0 => (1, 1, 3 - u16::from(self.upgrades[0])),
            1 => (2u8.saturating_sub(hard / 2).max(1), 2, 2),
            _ => ((4 + 2 * self.upgrades[1]).saturating_sub(hard).max(1), 3, 1),
        }
    }
    fn pay(&mut self, kits: u16, time: u16) -> bool {
        if self.kits < kits {
            self.notice = "Not enough supplies. Camp sells twelve units for ten coins.";
            return false;
        }
        if self.daylight < time {
            self.notice = "Not enough daylight. Rest at camp or recover this permit.";
            return false;
        }
        self.kits -= kits;
        self.daylight -= time;
        true
    }
    fn survey(&mut self) -> Cue {
        let x = self.cursor % COLS;
        let y = self.cursor / COLS;
        let near = |i: usize| (i % COLS).abs_diff(x) + (i / COLS).abs_diff(y) <= 1;
        if !(0..CELLS).any(|i| near(i) && !self.cells[i].known) {
            return Cue::None;
        }
        if !self.pay(1, 2) {
            return Cue::None;
        }
        for (i, c) in self.cells.iter_mut().enumerate() {
            if near(i) {
                c.known = true;
            }
        }
        self.notice = "Survey complete. Traces mark fossils; numbered tiles show remaining depth.";
        Cue::Action
    }
    fn dig(&mut self) -> Cue {
        let c = self.cells[self.cursor];
        if c.taken || (c.depth == 0 && c.find.is_none()) {
            return Cue::None;
        }
        if c.depth == 0 {
            let Some(n) = c.find else { return Cue::None };
            if !self.pay(1, 2) {
                return Cue::None;
            }
            self.cells[self.cursor].taken = true;
            if c.quality == 0 {
                self.notice =
                    "This specimen was destroyed. Its context remains in the field notes.";
                return Cue::Bad;
            }
            self.samples[n] = Some(Sample {
                kind: self.content().finds[n].kind,
                quality: c.quality,
                identified: false,
            });
            self.sample = n;
            self.notice = "Specimen jacketed. Open the Lab to read its traits and identify it.";
            return Cue::Good;
        }
        let (power, kits, time) = self.tool_stats(self.cursor);
        if !self.pay(kits, time) {
            return Cue::None;
        }
        let cell = &mut self.cells[self.cursor];
        cell.depth = cell.depth.saturating_sub(power);
        self.strikes += 1;
        if let Some(n) = c.find
            && power >= c.depth
            && self.tool > 0
        {
            let base = u16::from(self.content().finds[n].fragile)
                + u16::from(power - c.depth) * 10
                + if self.tool == 2 { 20 } else { 0 };
            let damage = base.saturating_sub(u16::from(self.upgrades[0]) * 8);
            self.cells[self.cursor].quality = c.quality.saturating_sub(damage.min(100) as u8);
        }
        if self.cells[self.cursor].depth == 0 {
            self.cells[self.cursor].known = true;
            self.notice = if c.find.is_some() {
                "Fossil exposed. Space jackets it; rough tools may have reduced its condition."
            } else {
                "Bedrock reached. This tile contains no specimen."
            };
        } else {
            self.notice = "Overburden removed. Stop rough tools before they reach the fossil."
        }
        if c.find.is_some() && self.cells[self.cursor].quality < c.quality {
            Cue::Bad
        } else {
            Cue::Action
        }
    }
    fn research(&mut self) -> Cue {
        let Some(s) = self.samples[self.sample] else {
            return Cue::None;
        };
        if s.identified {
            return Cue::None;
        }
        if !self.pay(1, 2) {
            return Cue::None;
        }
        if self.choice != s.kind {
            self.coins = self.coins.saturating_sub(6);
            self.renown = self.renown.saturating_sub(1);
            self.notice =
                "Identification rejected: compare both traits with the Atlas. Six coins lost.";
            return Cue::Bad;
        }
        self.samples[self.sample].as_mut().unwrap().identified = true;
        self.museum[self.index * 4 + self.sample] = Record {
            quality: s.quality,
            identified: true,
        };
        let grant = SPECIES[s.kind].value * u32::from(s.quality) / 100;
        self.coins += grant;
        self.renown += if s.quality >= 80 { 3 } else { 1 };
        self.score += grant * 12 + u32::from(s.quality) * 2;
        self.notice = "Identification accepted. Museum funding and research renown awarded once.";
        Cue::Good
    }
    fn recover(&mut self) -> Cue {
        if self.retries == 0 {
            self.phase = Phase::Lost;
            self.notice = "No recovery grants remain. The expedition closes.";
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
        self.notice =
            "Permit recovered. This site's earnings, purchases and specimens were rolled back.";
        Cue::Action
    }
    pub(crate) fn upgrade_cost(&self, which: usize) -> u32 {
        (if which == 0 { 35 } else { 40 }) + u32::from(self.upgrades[which]) * 35
    }
    fn camp(&mut self) -> Cue {
        match self.row {
            0 => {
                if self.day >= 3 || self.daylight == self.content().daylight || self.coins < 12 {
                    return Cue::None;
                }
                self.coins -= 12;
                self.day += 1;
                self.daylight = self.content().daylight;
                self.notice = "Camp rested. A new field day begins; three days cover one permit.";
                Cue::Good
            }
            1 => {
                if self.packs == 0 || self.coins < 10 || self.kits > 108 {
                    return Cue::None;
                }
                self.coins -= 10;
                self.kits += 12;
                self.packs -= 1;
                self.notice = "Twelve supply units purchased. This camp has a finite stock.";
                Cue::Good
            }
            2 | 3 => {
                let u = self.row - 2;
                let cost = self.upgrade_cost(u);
                if self.upgrades[u] >= 2
                    || self.coins < cost
                    || self.renown < 3 + u16::from(self.upgrades[u]) * 3
                {
                    return Cue::None;
                }
                self.coins -= cost;
                self.upgrades[u] += 1;
                self.notice =
                    "Field equipment upgraded. Improved tools persist after successful permits.";
                Cue::Good
            }
            4 => {
                if !self.ready() {
                    self.notice =
                        "Need the signature part and at least one other identified specimen.";
                    return Cue::None;
                }
                let n = self
                    .samples
                    .iter()
                    .flatten()
                    .filter(|s| s.identified)
                    .count();
                let quality = self
                    .samples
                    .iter()
                    .flatten()
                    .filter(|s| s.identified)
                    .map(|s| u16::from(s.quality))
                    .sum::<u16>()
                    / n as u16;
                self.medals[self.index] = if n == 4 && quality >= 90 {
                    3
                } else if n >= 3 && quality >= 60 {
                    2
                } else {
                    1
                };
                self.score +=
                    1000 + u32::from(self.daylight) * 5 + u32::from(self.medals[self.index]) * 200;
                self.coins += 30;
                if self.index == 5 {
                    self.phase = Phase::Complete;
                    self.score += 3000 + u32::from(self.renown) * 20;
                    self.notice = "The Dawn Giant stands again. The basin museum opens its doors.";
                    Cue::Win
                } else {
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
            self.notice =
                "This permit cannot meet its museum obligations. Space uses a recovery grant.";
            Cue::Bad
        } else {
            Cue::None
        }
    }
    /// Execute one fixed tick. Only fresh command edges cause field work or spending.
    pub fn step(&mut self, input: &Input) -> Cue {
        if self.outcome() != Outcome::Playing {
            return Cue::None;
        }
        match self.phase {
            Phase::Briefing => {
                if input.primary || input.click {
                    self.phase = Phase::Field;
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Setback => {
                return if input.primary {
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
                        return Cue::Action;
                    }
                }
                if input.tertiary {
                    self.tool = (self.tool + 1) % 3;
                    return Cue::Action;
                }
                if input.secondary {
                    cue = self.survey()
                } else if input.primary {
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
                        self.choice = (self.choice + 1) % 6;
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
            Page::Museum => {
                if input.left_pressed {
                    self.choice = (self.choice + 5) % 6;
                    cue = Cue::Action
                }
                if input.right_pressed {
                    self.choice = (self.choice + 1) % 6;
                    cue = Cue::Action
                }
            }
            Page::Atlas => {
                if input.left_pressed || input.up_pressed {
                    self.choice = (self.choice + 5) % 6;
                    cue = Cue::Action
                }
                if input.right_pressed || input.down_pressed {
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
    /// Text for the final career report; safe to call during an unfinished permit.
    pub fn summary(&self) -> String {
        format!(
            "{} of 6 sites completed. {} museum specimens, {} Giant parts. {} medals and {} points. {} coins remain. {}",
            self.medals.iter().filter(|&&m| m > 0).count(),
            self.identified(),
            self.museum_pieces(),
            self.medals.iter().map(|&m| u32::from(m)).sum::<u32>(),
            self.score,
            self.coins,
            if self.outcome() == Outcome::Won {
                "The Dawn Giant stands in the new basin museum."
            } else {
                "Careful fieldwork preserves the story in every stone."
            }
        )
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
}
