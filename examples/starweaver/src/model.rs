//! Pure, bounded rules for a complete stellar salvage expedition.
use crate::content::*;
use crate::layout::{button_hit, choice_at, row_at, star_at, tab_at};
use ferravex_arcade::{Cue, Input, Outcome};

/// Why a command was refused. Nothing is spent when one is recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// No charted lane joins the current and selected sectors.
    NoLane,
    /// At least this much fuel is needed.
    Fuel(u16),
    /// At least this many credits are needed.
    Credits(u16),
    /// At least this many units of a cargo kind are needed.
    Cargo(usize, u16),
    /// The recipe's materials are not all aboard.
    Materials,
    /// The hold lacks the free space this needs.
    Space,
    /// The recipe's relic has not been recovered.
    Relic(usize),
    /// The sector must be surveyed first.
    Unsurveyed,
    /// The deposit is exhausted.
    Depleted,
    /// Already surveyed, claimed, built or spent.
    Done,
    /// The fuel tank is already full.
    TankFull,
    /// The hull is already intact.
    HullFull,
    /// Only Loom Dock and Verdant Archive offer this service.
    Station,
    /// The gate stands at Loom Dock.
    Home,
    /// The ship is already at Loom Dock.
    Docked,
    /// The gate needs all three mechanisms.
    Mechanisms,
    /// Loom's landmark is the gate itself, opened from the Mission panel.
    Gate,
}

/// What one tick accepted (or refused), for presentation only: the view turns
/// these into effects where they happened. Rules never read them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// A lane jump left `from` for `to`, spending one fuel and one watch.
    Launch { from: usize, to: usize },
    /// The ship reached `sector`; `first` marks the first visit.
    Arrive { sector: usize, first: bool },
    /// Hull lost (after shielding) at the ship's sector.
    Damage { sector: usize, amount: u16 },
    /// The first survey of `sector`.
    Survey { sector: usize },
    /// `amount` units of cargo `kind` extracted at `sector`.
    Extract {
        sector: usize,
        kind: usize,
        amount: u16,
    },
    /// A landmark claimed directly; `gain` is its clamped variable reward
    /// (fuel at Glass Comet, hull at Far Beacon).
    Landmark { sector: usize, gain: u16 },
    /// An encounter opened at `sector`.
    Encounter { sector: usize },
    /// An encounter approach was accepted; `bonus` marks the Choir dive's
    /// extra crystal.
    Resolve {
        sector: usize,
        encounter: Encounter,
        choice: usize,
        bonus: bool,
    },
    /// The crew withdrew from the Choir at no cost.
    Withdraw { sector: usize },
    /// A recipe was fabricated.
    Forge { recipe: usize },
    /// Ship service `row` was accepted; `gain` is the fuel, hull or credits added.
    Service { row: usize, kind: usize, gain: u16 },
    /// The one-use distress tow hauled the ship home from `from`.
    Tow { from: usize },
    /// One unit of cargo `kind` was jettisoned.
    Jettison { kind: usize },
    /// The gate opened; the expedition is won.
    Gate,
    /// The star collapsed at the deadline.
    Collapse,
    /// The hull failed.
    Breach,
    /// A command was refused at the current focus; nothing was spent.
    Refuse(Refusal),
}

/// The bounded event list of the most recent tick.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Feed {
    slots: [Option<Event>; 8],
}
impl Feed {
    fn clear(&mut self) {
        self.slots = [None; 8];
    }
    fn push(&mut self, event: Event) {
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(event);
        }
    }
    /// Events in the order they were accepted.
    pub(crate) fn iter(&self) -> impl Iterator<Item = Event> + '_ {
        self.slots.iter().map_while(|slot| *slot)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Encounter {
    Choir,
    Sentinel,
    Privateers,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Transit {
    pub from: usize,
    pub to: usize,
    pub remaining: u16,
    pub total: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Site {
    pub scanned: bool,
    pub visited: bool,
    pub claimed: bool,
    pub stock: [u16; 3],
}

/// One expedition, without rendering, wall clocks, storage or random I/O.
///
/// ```
/// use starweaver::Game;
/// use ferravex_arcade::{Input, Outcome};
/// let mut expedition = Game::new();
/// assert_eq!(expedition.sector(), 0);
/// assert_eq!(expedition.outcome(), Outcome::Playing);
/// expedition.step(&Input { slot: Some(2), ..Input::default() });
/// expedition.step(&Input { primary: true, ..Input::default() });
/// assert_eq!(expedition.watch(), 1); // First survey is an accepted action.
/// assert!(expedition.score() > 0);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub(crate) sector: usize,
    pub(crate) selected: usize,
    pub(crate) page: usize,
    pub(crate) row: usize,
    pub(crate) sell_kind: usize,
    pub(crate) sites: [Site; SECTORS],
    pub(crate) cargo: [u16; 3],
    pub(crate) fuel: u16,
    pub(crate) hull: u16,
    pub(crate) credits: u32,
    pub(crate) watch: u16,
    pub(crate) relics: [bool; 3],
    pub(crate) components: [bool; 3],
    pub(crate) upgrades: [bool; 3],
    pub(crate) crew: bool,
    pub(crate) tow: bool,
    pub(crate) transit: Option<Transit>,
    pub(crate) encounter: Option<Encounter>,
    pub(crate) choice: usize,
    pub(crate) points: u32,
    pub(crate) result: Outcome,
    /// What the most recent tick did; presentation only.
    pub(crate) events: Feed,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Start a fresh expedition with finite stores and an intact survey ship.
    pub fn new() -> Self {
        let mut sites = std::array::from_fn(|i| Site {
            scanned: false,
            visited: false,
            claimed: false,
            stock: CHART[i].supplies,
        });
        sites[0].visited = true;
        Self {
            sector: 0,
            selected: 1,
            page: 0,
            row: 0,
            sell_kind: 0,
            sites,
            cargo: [1, 0, 1],
            fuel: 8,
            hull: 100,
            credits: 60,
            watch: 0,
            relics: [false; 3],
            components: [false; 3],
            upgrades: [false; 3],
            crew: false,
            tow: true,
            transit: None,
            encounter: None,
            choice: 0,
            points: 0,
            result: Outcome::Playing,
            events: Feed::default(),
        }
    }
    /// Current chart index; names and artwork are authored presentation data.
    pub fn sector(&self) -> usize {
        self.sector
    }
    /// Accepted expedition watches spent, excluding idle planning time.
    pub fn watch(&self) -> u16 {
        self.watch
    }
    /// Earned score, including the one-shot ending bonus when successful.
    pub fn score(&self) -> u32 {
        self.points
    }
    /// Current terminal or active outcome.
    pub fn outcome(&self) -> Outcome {
        self.result
    }
    pub(crate) fn hold_used(&self) -> u16 {
        self.cargo.iter().sum()
    }
    pub(crate) fn hold_max(&self) -> u16 {
        if self.upgrades[1] { 18 } else { 12 }
    }
    pub(crate) fn fuel_max(&self) -> u16 {
        10
    }
    pub(crate) fn scanned(&self) -> usize {
        self.sites.iter().filter(|s| s.scanned).count()
    }
    pub(crate) fn built(&self) -> usize {
        self.components.iter().filter(|v| **v).count()
    }
    pub(crate) fn adjacent(a: usize, b: usize) -> bool {
        LINKS
            .iter()
            .any(|&(x, y)| (x == a && y == b) || (x == b && y == a))
    }
    /// Record a refusal; the caller returns without spending anything.
    fn refuse(&mut self, reason: Refusal) {
        self.events.push(Event::Refuse(reason));
    }
    fn emit(&mut self, event: Event) {
        self.events.push(event);
    }
    fn spend_watch(&mut self) -> bool {
        self.watch = self.watch.saturating_add(1);
        if self.watch >= DEADLINE {
            self.result = Outcome::Lost;
            self.emit(Event::Collapse);
            false
        } else {
            true
        }
    }
    fn damage(&mut self, raw: u16) {
        let amount = self.shielded(raw);
        let lost = amount.min(self.hull);
        self.hull = self.hull.saturating_sub(amount);
        if lost > 0 {
            self.emit(Event::Damage {
                sector: self.sector,
                amount: lost,
            });
        }
        if self.hull == 0 {
            self.result = Outcome::Lost;
            self.emit(Event::Breach);
        }
    }
    /// Hull lost to `raw` damage once a Radiant Shield halves it, rounding up.
    pub(crate) fn shielded(&self, raw: u16) -> u16 {
        if self.upgrades[0] {
            raw.div_ceil(2)
        } else {
            raw
        }
    }
    pub(crate) fn hold_free(&self) -> u16 {
        self.hold_max() - self.hold_used()
    }
    /// Units one extraction of cargo `kind` would recover here.
    pub(crate) fn extraction(&self, kind: usize) -> u16 {
        let quantity = if self.upgrades[2] { 3 } else { 2 };
        quantity
            .min(self.sites[self.sector].stock[kind])
            .min(self.hold_free())
    }
    /// Why a jump to sector `to` would be refused, if it would.
    pub(crate) fn jump_refusal(&self, to: usize) -> Option<Refusal> {
        if !Self::adjacent(self.sector, to) {
            Some(Refusal::NoLane)
        } else if self.fuel == 0 {
            Some(Refusal::Fuel(1))
        } else {
            None
        }
    }
    /// Why Survey row `row` would be refused here, if it would.
    pub(crate) fn work_refusal(&self, row: usize) -> Option<Refusal> {
        let site = self.sites[self.sector];
        match row {
            0 => site.scanned.then_some(Refusal::Done),
            _ if !site.scanned => Some(Refusal::Unsurveyed),
            1..=3 => (self.extraction(row - 1) == 0).then_some(if self.hold_free() == 0 {
                Refusal::Space
            } else {
                Refusal::Depleted
            }),
            4 => self.landmark_refusal(),
            _ => None,
        }
    }
    fn landmark_refusal(&self) -> Option<Refusal> {
        if self.sites[self.sector].claimed {
            return Some(Refusal::Done);
        }
        match self.sector {
            0 => Some(Refusal::Gate),
            1 | 5 if self.hold_free() < 2 => Some(Refusal::Space),
            2 if self.fuel == self.fuel_max() => Some(Refusal::TankFull),
            _ => None,
        }
    }
    /// Why encounter approach `choice` would be refused, if it would.
    pub(crate) fn approach_refusal(&self, encounter: Encounter, choice: usize) -> Option<Refusal> {
        match (encounter, choice) {
            (Encounter::Choir, 0) if self.fuel == 0 => Some(Refusal::Fuel(1)),
            (Encounter::Sentinel, 0) if self.cargo[2] < 2 => Some(Refusal::Cargo(2, 2)),
            (Encounter::Sentinel, 1) if self.credits < 30 => Some(Refusal::Credits(30)),
            (Encounter::Privateers, 0) if self.cargo[0] == 0 => Some(Refusal::Cargo(0, 1)),
            (Encounter::Privateers, 0) if self.hold_free() < 2 => Some(Refusal::Space),
            (Encounter::Privateers, 1) if self.fuel < 2 => Some(Refusal::Fuel(2)),
            _ => None,
        }
    }
    /// Why recipe `row` would be refused, if it would.
    pub(crate) fn forge_refusal(&self, row: usize) -> Option<Refusal> {
        let Some(recipe) = RECIPES.get(row) else {
            return Some(Refusal::Done);
        };
        let done = if row < 3 {
            self.components[row]
        } else {
            self.upgrades[row - 3]
        };
        if done {
            Some(Refusal::Done)
        } else if row < 3 && !self.relics[row] {
            Some(Refusal::Relic(row))
        } else if (0..3).any(|i| self.cargo[i] < recipe.cost[i]) {
            Some(Refusal::Materials)
        } else {
            None
        }
    }
    /// Why ship service `row` would be refused here, if it would.
    pub(crate) fn service_refusal(&self, row: usize) -> Option<Refusal> {
        let station = CHART[self.sector].station;
        match row {
            0 if self.cargo[1] == 0 => Some(Refusal::Cargo(1, 1)),
            0 if self.fuel == self.fuel_max() => Some(Refusal::TankFull),
            1 if self.cargo[0] == 0 => Some(Refusal::Cargo(0, 1)),
            1 if self.cargo[2] == 0 => Some(Refusal::Cargo(2, 1)),
            1 if self.hull == 100 => Some(Refusal::HullFull),
            2..=4 if !station => Some(Refusal::Station),
            2 if self.credits < 15 => Some(Refusal::Credits(15)),
            2 if self.fuel == self.fuel_max() => Some(Refusal::TankFull),
            3 if self.credits < 20 => Some(Refusal::Credits(20)),
            3 if self.hull == 100 => Some(Refusal::HullFull),
            4 if self.cargo[self.sell_kind] == 0 => Some(Refusal::Cargo(self.sell_kind, 1)),
            5 if !self.tow => Some(Refusal::Done),
            5 if self.sector == 0 => Some(Refusal::Docked),
            _ => None,
        }
    }
    /// The ending bonus for crossing on `watch` with the current hull and credits.
    pub(crate) fn escape_bonus(&self, watch: u16) -> u32 {
        1200 + u32::from(DEADLINE.saturating_sub(watch)) * 25
            + u32::from(self.hull) * 3
            + self.credits.min(500)
    }
    /// Why opening the gate would be refused, if it would.
    pub(crate) fn gate_refusal(&self) -> Option<Refusal> {
        if self.sector != 0 {
            Some(Refusal::Home)
        } else if self.built() != 3 {
            Some(Refusal::Mechanisms)
        } else {
            None
        }
    }
    fn jump(&mut self) -> Cue {
        if let Some(reason) = self.jump_refusal(self.selected) {
            self.refuse(reason);
            return Cue::None;
        }
        self.fuel -= 1;
        if !self.spend_watch() {
            return Cue::Bad;
        }
        self.transit = Some(Transit {
            from: self.sector,
            to: self.selected,
            remaining: 120,
            total: 120,
        });
        self.emit(Event::Launch {
            from: self.sector,
            to: self.selected,
        });
        Cue::Action
    }
    fn survey(&mut self) -> Cue {
        if let Some(reason) = self.work_refusal(self.row) {
            self.refuse(reason);
            return Cue::None;
        }
        match self.row {
            0 => {
                self.sites[self.sector].scanned = true;
                self.points += 75;
                self.emit(Event::Survey {
                    sector: self.sector,
                });
                if self.spend_watch() {
                    Cue::Good
                } else {
                    Cue::Bad
                }
            }
            1..=3 => {
                let kind = self.row - 1;
                let amount = self.extraction(kind);
                self.sites[self.sector].stock[kind] -= amount;
                self.cargo[kind] += amount;
                self.emit(Event::Extract {
                    sector: self.sector,
                    kind,
                    amount,
                });
                self.damage(CHART[self.sector].extraction_damage);
                if self.result == Outcome::Playing {
                    self.spend_watch();
                }
                if self.result == Outcome::Lost {
                    Cue::Bad
                } else {
                    Cue::Good
                }
            }
            4 => self.explore(),
            _ => Cue::None,
        }
    }
    /// Claim the surveyed, unclaimed landmark here (refusals already checked).
    fn explore(&mut self) -> Cue {
        match self.sector {
            1 => {
                self.cargo[0] += 2;
                self.sites[1].claimed = true;
                self.points += 100;
                self.emit(Event::Landmark { sector: 1, gain: 2 });
                if self.spend_watch() {
                    Cue::Good
                } else {
                    Cue::Bad
                }
            }
            2 => {
                let before = self.fuel;
                self.fuel = (self.fuel + 4).min(self.fuel_max());
                self.sites[2].claimed = true;
                self.points += 100;
                self.emit(Event::Landmark {
                    sector: 2,
                    gain: self.fuel - before,
                });
                if self.spend_watch() {
                    Cue::Good
                } else {
                    Cue::Bad
                }
            }
            3 => {
                self.relics[0] = true;
                self.sites[3].claimed = true;
                self.points += 250;
                self.credits += 20;
                self.emit(Event::Landmark {
                    sector: 3,
                    gain: 20,
                });
                if self.spend_watch() {
                    Cue::Good
                } else {
                    Cue::Bad
                }
            }
            4 => {
                self.encounter = Some(Encounter::Choir);
                self.choice = 0;
                self.emit(Event::Encounter { sector: 4 });
                Cue::Action
            }
            5 => {
                self.cargo[1] += 2;
                self.credits += 15;
                self.sites[5].claimed = true;
                self.points += 150;
                self.emit(Event::Landmark { sector: 5, gain: 2 });
                if self.spend_watch() {
                    Cue::Good
                } else {
                    Cue::Bad
                }
            }
            6 => {
                self.encounter = Some(Encounter::Privateers);
                self.choice = 0;
                self.emit(Event::Encounter { sector: 6 });
                Cue::Action
            }
            7 => {
                self.encounter = Some(Encounter::Sentinel);
                self.choice = 0;
                self.emit(Event::Encounter { sector: 7 });
                Cue::Action
            }
            8 => {
                self.crew = true;
                self.sites[8].claimed = true;
                self.credits += 25;
                self.points += 350;
                let before = self.hull;
                self.hull = (self.hull + 10).min(100);
                self.emit(Event::Landmark {
                    sector: 8,
                    gain: self.hull - before,
                });
                if self.spend_watch() {
                    Cue::Good
                } else {
                    Cue::Bad
                }
            }
            _ => Cue::None,
        }
    }
    fn resolve(&mut self) -> Cue {
        let Some(encounter) = self.encounter else {
            return Cue::None;
        };
        if let Some(reason) = self.approach_refusal(encounter, self.choice) {
            self.refuse(reason);
            return Cue::None;
        }
        let mut bonus = false;
        match (encounter, self.choice) {
            (Encounter::Choir, 0) => {
                self.fuel -= 1;
                self.relics[1] = true;
            }
            (Encounter::Choir, 1) => {
                self.damage(18);
                self.relics[1] = true;
                if self.hold_used() < self.hold_max() {
                    self.cargo[1] += 1;
                    bonus = true;
                }
            }
            (Encounter::Choir, 2) => {
                self.encounter = None;
                self.emit(Event::Withdraw {
                    sector: self.sector,
                });
                return Cue::Action;
            }
            (Encounter::Sentinel, 0) => {
                self.cargo[2] -= 2;
                self.relics[2] = true;
            }
            (Encounter::Sentinel, 1) => {
                self.credits -= 30;
                self.relics[2] = true;
            }
            (Encounter::Sentinel, 2) => {
                self.damage(34);
                self.relics[2] = true;
            }
            (Encounter::Privateers, 0) => {
                self.cargo[0] -= 1;
                self.cargo[2] += 3;
            }
            (Encounter::Privateers, 1) => {
                self.fuel -= 2;
            }
            (Encounter::Privateers, 2) => {
                self.damage(22);
                self.credits += 40;
            }
            _ => return Cue::None,
        }
        self.emit(Event::Resolve {
            sector: self.sector,
            encounter,
            choice: self.choice,
            bonus,
        });
        self.sites[self.sector].claimed = true;
        self.encounter = None;
        self.points += 250;
        if self.result == Outcome::Playing {
            self.spend_watch();
        }
        if self.result == Outcome::Lost {
            Cue::Bad
        } else {
            Cue::Good
        }
    }
    fn forge(&mut self) -> Cue {
        let Some(recipe) = RECIPES.get(self.row) else {
            return Cue::None;
        };
        if let Some(reason) = self.forge_refusal(self.row) {
            self.refuse(reason);
            return Cue::None;
        }
        for i in 0..3 {
            self.cargo[i] -= recipe.cost[i];
        }
        if self.row < 3 {
            self.components[self.row] = true;
            self.points += 500;
        } else {
            self.upgrades[self.row - 3] = true;
            self.points += 100;
        }
        self.emit(Event::Forge { recipe: self.row });
        if self.spend_watch() {
            Cue::Good
        } else {
            Cue::Bad
        }
    }
    fn service(&mut self) -> Cue {
        if let Some(reason) = self.service_refusal(self.row) {
            self.refuse(reason);
            return Cue::None;
        }
        match self.row {
            0 => {
                let before = self.fuel;
                self.cargo[1] -= 1;
                self.fuel = (self.fuel + 3).min(self.fuel_max());
                self.emit(Event::Service {
                    row: 0,
                    kind: 1,
                    gain: self.fuel - before,
                });
            }
            1 => {
                let before = self.hull;
                self.cargo[0] -= 1;
                self.cargo[2] -= 1;
                self.hull = (self.hull + 25).min(100);
                self.emit(Event::Service {
                    row: 1,
                    kind: 0,
                    gain: self.hull - before,
                });
            }
            2 => {
                let before = self.fuel;
                self.credits -= 15;
                self.fuel = (self.fuel + 5).min(self.fuel_max());
                self.emit(Event::Service {
                    row: 2,
                    kind: 0,
                    gain: self.fuel - before,
                });
            }
            3 => {
                let before = self.hull;
                self.credits -= 20;
                self.hull = (self.hull + 45).min(100);
                self.emit(Event::Service {
                    row: 3,
                    kind: 0,
                    gain: self.hull - before,
                });
            }
            4 => {
                self.cargo[self.sell_kind] -= 1;
                let price = SELL_PRICE[self.sell_kind];
                self.credits += u32::from(price);
                self.emit(Event::Service {
                    row: 4,
                    kind: self.sell_kind,
                    gain: price,
                });
            }
            5 => {
                let from = self.sector;
                self.tow = false;
                self.sector = 0;
                self.selected = 1;
                self.fuel = 5;
                self.emit(Event::Tow { from });
                self.damage(15);
                for _ in 0..5 {
                    if self.result == Outcome::Playing {
                        self.spend_watch();
                    }
                }
            }
            _ => return Cue::None,
        }
        if self.result == Outcome::Playing {
            self.spend_watch();
        }
        if self.result == Outcome::Lost {
            Cue::Bad
        } else {
            Cue::Good
        }
    }
    fn open_gate(&mut self) -> Cue {
        if let Some(reason) = self.gate_refusal() {
            self.refuse(reason);
            return Cue::None;
        }
        if !self.spend_watch() {
            return Cue::Bad;
        }
        self.points += self.escape_bonus(self.watch);
        self.result = Outcome::Won;
        self.emit(Event::Gate);
        Cue::Win
    }
    fn rows(&self) -> usize {
        match self.page {
            0 => SECTORS,
            1 => 5,
            2 | 3 => 6,
            _ => 1,
        }
    }
    fn switch_page(&mut self, page: usize) {
        if page != self.page {
            self.page = page;
            self.row = 0;
        }
    }
    /// Execute one 60 Hz tick. Only accepted operations spend watches or stores.
    /// Idle planning is free; terminal state is immutable until the host restarts.
    pub fn step(&mut self, input: &Input) -> Cue {
        if self.result != Outcome::Playing {
            return Cue::None;
        }
        self.events.clear();
        if let Some(mut flight) = self.transit {
            flight.remaining = flight.remaining.saturating_sub(1);
            if flight.remaining == 0 {
                self.sector = flight.to;
                self.transit = None;
                let first = !self.sites[self.sector].visited;
                self.emit(Event::Arrive {
                    sector: self.sector,
                    first,
                });
                if first {
                    self.sites[self.sector].visited = true;
                    self.damage(CHART[self.sector].arrival_damage);
                }
                return if self.result == Outcome::Lost {
                    Cue::Bad
                } else {
                    Cue::Good
                };
            }
            self.transit = Some(flight);
            return Cue::None;
        }
        if self.encounter.is_some() {
            if input.up_pressed || input.left_pressed {
                self.choice = (self.choice + 2) % 3;
            }
            if input.down_pressed || input.right_pressed {
                self.choice = (self.choice + 1) % 3;
            }
            let mut act = input.primary;
            if input.click
                && let Some(i) = choice_at(input.pointer)
            {
                self.choice = i;
                act = true;
            }
            return if act { self.resolve() } else { Cue::None };
        }
        if let Some(slot) = input.slot
            && (1..=5).contains(&slot)
        {
            self.switch_page(slot - 1);
            return Cue::Action;
        }
        if input.click
            && let Some(page) = tab_at(input.pointer)
        {
            self.switch_page(page);
            return Cue::Action;
        }
        if self.page == 0 {
            if input.left_pressed || input.up_pressed {
                self.selected = (self.selected + SECTORS - 1) % SECTORS;
            }
            if input.right_pressed || input.down_pressed {
                self.selected = (self.selected + 1) % SECTORS;
            }
        } else {
            let rows = self.rows();
            if input.left_pressed || input.up_pressed {
                self.row = (self.row + rows - 1) % rows;
            }
            if input.right_pressed || input.down_pressed {
                self.row = (self.row + 1) % rows;
            }
        }
        if input.secondary && self.page == 1 && (1..=3).contains(&self.row) {
            let kind = self.row - 1;
            if self.cargo[kind] > 0 {
                self.cargo[kind] -= 1;
                self.emit(Event::Jettison { kind });
                return Cue::Action;
            }
        }
        if input.secondary && self.page == 3 {
            self.sell_kind = (self.sell_kind + 1) % 3;
            return Cue::Action;
        }
        let mut act = input.primary;
        if input.click {
            if self.page == 0 {
                if let Some(i) = star_at(input.pointer) {
                    // Clicking the already selected star confirms the jump,
                    // exactly as the confirm button and Space do.
                    act |= i == self.selected;
                    self.selected = i;
                }
            } else if self.page < 4
                && let Some(i) = row_at(self.page, self.rows(), input.pointer)
            {
                self.row = i;
                act = true;
            }
            if button_hit(input.pointer) {
                act = true;
            }
        }
        if !act {
            return Cue::None;
        }
        match self.page {
            0 => self.jump(),
            1 => self.survey(),
            2 => self.forge(),
            3 => self.service(),
            4 => self.open_gate(),
            _ => Cue::None,
        }
    }
    fn distances(from: usize) -> [u8; SECTORS] {
        let mut distances = [u8::MAX; SECTORS];
        distances[from] = 0;
        for _ in 0..SECTORS {
            for &(a, b) in &LINKS {
                if distances[a] != u8::MAX {
                    distances[b] = distances[b].min(distances[a] + 1);
                }
                if distances[b] != u8::MAX {
                    distances[a] = distances[a].min(distances[b] + 1);
                }
            }
        }
        distances
    }
    fn command(&self, page: usize, row: usize) -> Input {
        if self.page != page {
            Input {
                slot: Some(page + 1),
                ..Input::default()
            }
        } else if self.row != row {
            Input {
                down_pressed: true,
                ..Input::default()
            }
        } else {
            Input {
                primary: true,
                ..Input::default()
            }
        }
    }
    fn navigate(&self, target: usize) -> Input {
        if self.fuel == 0 {
            if self.cargo[1] > 0 {
                return self.command(3, 0);
            }
            if self.sites[self.sector].stock[1] > 0 {
                return self.command(
                    1,
                    if self.sites[self.sector].scanned {
                        2
                    } else {
                        0
                    },
                );
            }
            if CHART[self.sector].station && self.credits >= 15 {
                return self.command(3, 2);
            }
            return self.command(3, 5);
        }
        let distances = Self::distances(target);
        let next = (0..SECTORS)
            .filter(|&i| Self::adjacent(self.sector, i))
            .min_by_key(|&i| (distances[i], i))
            .unwrap_or(self.sector);
        if self.page != 0 {
            Input {
                slot: Some(1),
                ..Input::default()
            }
        } else if self.selected != next {
            Input {
                right_pressed: true,
                ..Input::default()
            }
        } else {
            Input {
                primary: true,
                ..Input::default()
            }
        }
    }
    /// Read-only legal expedition pilot. It uses only the same menus, fuel,
    /// hazards, discoveries, resource conversions and recipes as a human.
    pub fn pilot(&self, tick: u64) -> Input {
        if self.result != Outcome::Playing || self.transit.is_some() || !tick.is_multiple_of(8) {
            return Input::default();
        }
        if let Some(event) = self.encounter {
            let choice = match event {
                Encounter::Choir => usize::from(self.fuel < 2),
                Encounter::Sentinel => {
                    if self.credits >= 30 {
                        1
                    } else if self.cargo[2] >= 2 {
                        0
                    } else {
                        2
                    }
                }
                Encounter::Privateers => {
                    if self.cargo[0] > 0 && self.hold_max() - self.hold_used() >= 2 {
                        0
                    } else if self.fuel >= 2 {
                        1
                    } else {
                        2
                    }
                }
            };
            return if self.choice != choice {
                Input {
                    down_pressed: true,
                    ..Input::default()
                }
            } else {
                Input {
                    primary: true,
                    ..Input::default()
                }
            };
        }
        if self.hull < 25 && self.cargo[0] > 0 && self.cargo[2] > 0 {
            return self.command(3, 1);
        }
        if self.fuel <= 1 && self.cargo[1] > 0 {
            return self.command(3, 0);
        }
        if let Some(recipe) = (0..3).find(|&i| !self.components[i]) {
            if !self.relics[recipe] {
                let target = RELIC_SECTORS[recipe];
                if self.sector != target {
                    return self.navigate(target);
                }
                return self.command(1, if self.sites[target].scanned { 4 } else { 0 });
            }
            if let Some(kind) = (0..3).find(|&i| self.cargo[i] < RECIPES[recipe].cost[i]) {
                if self.hold_used() == self.hold_max()
                    && let Some(extra) = (0..3).find(|&i| self.cargo[i] > RECIPES[recipe].cost[i])
                {
                    if self.page != 1 || self.row != extra + 1 {
                        return self.command(1, extra + 1);
                    }
                    return Input {
                        secondary: true,
                        ..Input::default()
                    };
                }
                let distance = Self::distances(self.sector);
                if let Some(target) = (0..SECTORS)
                    .filter(|&i| self.sites[i].stock[kind] > 0)
                    .min_by_key(|&i| (distance[i], CHART[i].extraction_damage, i))
                {
                    if self.sector != target {
                        return self.navigate(target);
                    }
                    return self.command(
                        1,
                        if self.sites[target].scanned {
                            kind + 1
                        } else {
                            0
                        },
                    );
                }
            }
            return self.command(2, recipe);
        }
        if !self.crew && self.watch < 42 {
            if self.sector != 8 {
                return self.navigate(8);
            }
            return self.command(1, if self.sites[8].scanned { 4 } else { 0 });
        }
        if self.sector != 0 {
            return self.navigate(0);
        }
        self.command(4, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_arcade::Vec2;

    fn accept(game: &mut Game, page: usize, row: usize) -> Cue {
        game.step(&Input {
            slot: Some(page + 1),
            ..Input::default()
        });
        while game.row != row {
            game.step(&Input {
                down_pressed: true,
                ..Input::default()
            });
        }
        game.step(&Input {
            primary: true,
            ..Input::default()
        })
    }
    fn stores(game: &Game) -> ([u16; 3], u16, u16, u32, u16, u32) {
        (
            game.cargo,
            game.fuel,
            game.hull,
            game.credits,
            game.watch,
            game.points,
        )
    }

    #[test]
    fn complete_legal_expedition_rescues_engineers_and_opens_gate() {
        let mut game = Game::new();
        let mut final_tick = None;
        for tick in 0..20_000 {
            let input = game.pilot(tick);
            game.step(&input);
            assert!(game.hold_used() <= game.hold_max());
            assert!(game.fuel <= game.fuel_max());
            assert!(game.hull <= 100);
            if game.outcome() != Outcome::Playing {
                final_tick = Some(tick);
                break;
            }
        }
        assert_eq!(
            game.result,
            Outcome::Won,
            "{:?}",
            (game.watch, game.hull, game.sector)
        );
        assert!(game.components.iter().all(|&built| built));
        assert!(game.relics.iter().all(|&found| found));
        assert!(game.crew);
        assert!(game.sites.iter().filter(|site| site.visited).count() >= 8);
        assert_eq!(game.sector, 0);
        assert_eq!(
            (game.watch, game.hull, game.score(), final_tick),
            (37, 64, 4817, Some(2424))
        );
        let frozen = game.clone();
        for _ in 0..60 {
            game.step(&Input {
                primary: true,
                slot: Some(3),
                ..Input::default()
            });
        }
        assert_eq!(game, frozen, "A won campaign cannot award score twice");
    }

    #[test]
    fn two_legal_pilots_are_tick_for_tick_identical() {
        let mut a = Game::new();
        let mut b = Game::new();
        for tick in 0..2500 {
            let input = a.pilot(tick);
            a.step(&input);
            b.step(&input);
            assert_eq!(a, b);
        }
        assert_eq!(a.result, Outcome::Won);
    }

    #[test]
    fn scan_and_finite_extraction_cannot_farm_score_or_stores() {
        let mut game = Game::new();
        game.sector = 1;
        assert_eq!(accept(&mut game, 1, 0), Cue::Good);
        let before = stores(&game);
        assert_eq!(accept(&mut game, 1, 0), Cue::None);
        assert_eq!(stores(&game), before);
        for _ in 0..4 {
            assert_eq!(accept(&mut game, 1, 1), Cue::Good);
        }
        assert_eq!(game.sites[1].stock[0], 0);
        assert_eq!(game.cargo[0], 9);
        let before = stores(&game);
        assert_eq!(accept(&mut game, 1, 1), Cue::None);
        assert_eq!(stores(&game), before);
        assert_eq!(game.points, 75);
    }

    #[test]
    fn recipe_rejections_are_atomic_and_success_consumes_exact_cost_once() {
        let mut game = Game::new();
        game.cargo = [3, 2, 1];
        let before = stores(&game);
        assert_eq!(accept(&mut game, 2, 0), Cue::None, "A relic is mandatory");
        assert_eq!(stores(&game), before);
        game.relics[0] = true;
        game.cargo[1] = 1;
        let before = stores(&game);
        assert_eq!(accept(&mut game, 2, 0), Cue::None);
        assert_eq!(stores(&game), before);
        game.cargo[1] = 2;
        assert_eq!(accept(&mut game, 2, 0), Cue::Good);
        assert_eq!(game.cargo, [0, 0, 0]);
        assert_eq!((game.watch, game.points), (1, 500));
        assert!(game.components[0]);
        let before = stores(&game);
        assert_eq!(accept(&mut game, 2, 0), Cue::None);
        assert_eq!(stores(&game), before);
    }

    #[test]
    fn invalid_lanes_and_empty_tank_never_spend_watches() {
        let mut game = Game::new();
        game.selected = 8;
        let before = stores(&game);
        assert_eq!(accept(&mut game, 0, 0), Cue::None);
        assert_eq!(stores(&game), before);
        game.selected = 1;
        game.fuel = 0;
        let before = stores(&game);
        assert_eq!(accept(&mut game, 0, 0), Cue::None);
        assert_eq!(stores(&game), before);
        assert!(game.transit.is_none());
    }

    #[test]
    fn cargo_capacity_clamps_extraction_without_destroying_deposit() {
        let mut game = Game::new();
        game.sector = 1;
        game.sites[1].scanned = true;
        game.cargo = [9, 1, 1];
        assert_eq!(accept(&mut game, 1, 1), Cue::Good);
        assert_eq!(game.hold_used(), 12);
        assert_eq!(game.sites[1].stock[0], 7);
        let before = stores(&game);
        assert_eq!(accept(&mut game, 1, 1), Cue::None);
        assert_eq!(stores(&game), before);
        game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.cargo[0], 9);
        assert_eq!(game.credits, before.3);
        assert_eq!(game.points, before.5);
    }

    #[test]
    fn permanent_upgrades_consume_recipe_and_change_actual_rules() {
        let mut game = Game::new();
        game.cargo = [6, 2, 2];
        for row in 3..6 {
            assert_eq!(accept(&mut game, 2, row), Cue::Good);
        }
        assert_eq!(game.cargo, [0, 0, 0]);
        assert_eq!(game.hold_max(), 18);
        assert_eq!(game.watch, 3);
        game.damage(15);
        assert_eq!(game.hull, 92, "Shield rounds odd damage up");
        game.sector = 1;
        game.sites[1].scanned = true;
        assert_eq!(accept(&mut game, 1, 1), Cue::Good);
        assert_eq!(game.cargo[0], 3);
        assert_eq!(game.sites[1].stock[0], 5);
    }

    #[test]
    fn encounters_reject_unaffordable_choice_and_landmarks_are_unique() {
        let mut game = Game::new();
        game.sector = 7;
        game.sites[7].scanned = true;
        assert_eq!(accept(&mut game, 1, 4), Cue::Action);
        let before = stores(&game);
        assert_eq!(
            game.resolve(),
            Cue::None,
            "One circuit cannot pay a two-circuit override"
        );
        assert_eq!(stores(&game), before);
        assert!(!game.relics[2]);
        game.choice = 1;
        assert_eq!(game.resolve(), Cue::Good);
        assert_eq!(game.credits, 30);
        assert!(game.relics[2]);
        let before = stores(&game);
        assert_eq!(accept(&mut game, 1, 4), Cue::None);
        assert_eq!(stores(&game), before);
    }

    #[test]
    fn risky_vault_entry_can_kill_and_restart_restores_every_store() {
        let mut game = Game::new();
        game.sector = 7;
        game.hull = 34;
        game.encounter = Some(Encounter::Sentinel);
        game.choice = 2;
        assert_eq!(
            game.step(&Input {
                primary: true,
                ..Input::default()
            }),
            Cue::Bad
        );
        assert_eq!(game.result, Outcome::Lost);
        assert_eq!(game.hull, 0);
        let frozen = game.clone();
        game.step(&Input {
            primary: true,
            secondary: true,
            slot: Some(4),
            ..Input::default()
        });
        assert_eq!(game, frozen);
        let restart = Game::new();
        assert_eq!(restart, Game::default());
        assert_eq!(restart.result, Outcome::Playing);
        assert!(restart.tow);
        assert_eq!(
            (restart.watch, restart.hull, restart.fuel, restart.credits),
            (0, 100, 8, 60)
        );
        assert!(!restart.relics.iter().any(|&found| found));
    }

    #[test]
    fn collapse_deadline_is_strict_but_idle_planning_is_free() {
        let mut game = Game::new();
        for _ in 0..1000 {
            game.step(&Input {
                down: true,
                primary_down: true,
                ..Input::default()
            });
        }
        assert_eq!(game.watch, 0);
        assert_eq!(game.selected, 1);
        assert_eq!(game.fuel, 8);
        game.watch = DEADLINE - 1;
        assert_eq!(accept(&mut game, 1, 0), Cue::Bad);
        assert_eq!(game.watch, DEADLINE);
        assert_eq!(game.result, Outcome::Lost);
        let frozen = game.clone();
        game.step(&Input::default());
        assert_eq!(game, frozen);
    }

    #[test]
    fn flight_ignores_navigation_edges_and_only_first_entry_applies_hazard() {
        let mut game = Game::new();
        game.sector = 1;
        game.selected = 3;
        assert_eq!(
            game.step(&Input {
                primary: true,
                ..Input::default()
            }),
            Cue::Action
        );
        assert_eq!((game.fuel, game.watch), (7, 1));
        for _ in 0..120 {
            game.step(&Input {
                primary: true,
                right_pressed: true,
                slot: Some(5),
                ..Input::default()
            });
        }
        assert_eq!((game.sector, game.hull, game.page), (3, 94, 0));
        game.sector = 1;
        game.selected = 3;
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        for _ in 0..120 {
            game.step(&Input::default());
        }
        assert_eq!(game.hull, 94);
    }

    #[test]
    fn services_charge_only_accepted_work_and_cannot_create_trade_loops() {
        let mut game = Game::new();
        game.fuel = 10;
        game.cargo[1] = 1;
        let before = stores(&game);
        assert_eq!(accept(&mut game, 3, 0), Cue::None);
        assert_eq!(accept(&mut game, 3, 2), Cue::None);
        assert_eq!(accept(&mut game, 3, 3), Cue::None);
        assert_eq!(stores(&game), before);
        game.fuel = 8;
        assert_eq!(accept(&mut game, 3, 0), Cue::Good);
        assert_eq!((game.fuel, game.cargo[1], game.watch), (10, 0, 1));
        game.sell_kind = 0;
        assert_eq!(accept(&mut game, 3, 4), Cue::Good);
        assert_eq!((game.cargo[0], game.credits), (0, 68));
        let before = stores(&game);
        assert_eq!(accept(&mut game, 3, 4), Cue::None);
        assert_eq!(stores(&game), before);
        game.sector = 1;
        game.fuel = 1;
        let before = stores(&game);
        assert_eq!(accept(&mut game, 3, 2), Cue::None);
        assert_eq!(stores(&game), before);
    }

    #[test]
    fn one_use_distress_tow_has_explicit_time_hull_and_fuel_costs() {
        let mut game = Game::new();
        game.sector = 7;
        game.fuel = 0;
        assert_eq!(accept(&mut game, 3, 5), Cue::Good);
        assert_eq!(
            (game.sector, game.fuel, game.hull, game.watch),
            (0, 5, 85, 6)
        );
        assert!(!game.tow);
        game.sector = 7;
        let before = stores(&game);
        assert_eq!(accept(&mut game, 3, 5), Cue::None);
        assert_eq!(stores(&game), before);
    }

    #[test]
    fn mouse_hit_regions_select_stars_and_accept_work_without_hidden_actions() {
        let mut game = Game::new();
        game.step(&Input {
            click: true,
            pointer: Vec2::new(CHART[2].x, CHART[2].y),
            ..Input::default()
        });
        assert_eq!(game.selected, 2);
        assert_eq!(game.watch, 0);
        game.step(&Input {
            click: true,
            pointer: Vec2::new(220., 130.),
            ..Input::default()
        });
        assert_eq!(game.page, 1);
        assert_eq!(
            game.step(&Input {
                click: true,
                pointer: Vec2::new(700., 292.),
                ..Input::default()
            }),
            Cue::Good
        );
        assert!(game.sites[0].scanned);
        assert_eq!(game.watch, 1);
        game.step(&Input {
            slot: Some(5),
            ..Input::default()
        });
        assert_eq!(
            game.step(&Input {
                click: true,
                pointer: Vec2::new(700., 241.),
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(game.watch, 1);
    }

    #[test]
    fn gate_needs_all_components_at_loom_and_awards_ending_once() {
        let mut game = Game::new();
        let before = stores(&game);
        assert_eq!(accept(&mut game, 4, 0), Cue::None);
        assert_eq!(stores(&game), before);
        game.components = [true; 3];
        game.sector = 1;
        assert_eq!(accept(&mut game, 4, 0), Cue::None);
        assert_eq!(stores(&game), before);
        game.sector = 0;
        assert_eq!(accept(&mut game, 4, 0), Cue::Win);
        let frozen = game.clone();
        assert_eq!(
            game.step(&Input {
                primary: true,
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(game, frozen);
    }

    #[test]
    fn clicking_the_selected_star_again_jumps_like_the_confirm_button() {
        let click = |game: &mut Game, i: usize| {
            game.step(&Input {
                click: true,
                pointer: Vec2::new(CHART[i].x, CHART[i].y),
                ..Input::default()
            })
        };
        let mut game = Game::new();
        assert_eq!(
            click(&mut game, 2),
            Cue::None,
            "the first click only selects"
        );
        assert_eq!((game.selected, game.watch, game.fuel), (2, 0, 8));
        assert!(game.transit.is_none());
        assert_eq!(click(&mut game, 2), Cue::Action, "the second click jumps");
        assert_eq!((game.watch, game.fuel), (1, 7));
        assert_eq!(game.transit.map(|flight| flight.to), Some(2));
        assert_eq!(
            game.events.iter().collect::<Vec<_>>(),
            vec![Event::Launch { from: 0, to: 2 }]
        );
        // An unconnected star refuses exactly like Space would, spending nothing.
        let mut game = Game::new();
        click(&mut game, 8);
        let before = stores(&game);
        assert_eq!(click(&mut game, 8), Cue::None);
        assert_eq!(stores(&game), before);
        assert_eq!(
            game.events.iter().collect::<Vec<_>>(),
            vec![Event::Refuse(Refusal::NoLane)]
        );
    }

    #[test]
    fn events_record_one_tick_and_refusals_spend_nothing() {
        let mut game = Game::new();
        accept(&mut game, 1, 1);
        assert_eq!(
            game.events.iter().collect::<Vec<_>>(),
            vec![Event::Refuse(Refusal::Unsurveyed)]
        );
        assert_eq!(game.watch, 0);
        game.step(&Input::default());
        assert_eq!(game.events.iter().count(), 0, "the feed is per tick");
        assert_eq!(accept(&mut game, 1, 0), Cue::Good);
        assert_eq!(
            game.events.iter().collect::<Vec<_>>(),
            vec![Event::Survey { sector: 0 }]
        );
        // A hazardous first arrival records the arrival and the hull it cost.
        let mut game = Game::new();
        game.sector = 1;
        game.selected = 3;
        accept(&mut game, 0, 0);
        for _ in 0..120 {
            game.step(&Input::default());
        }
        assert_eq!(
            game.events.iter().collect::<Vec<_>>(),
            vec![
                Event::Arrive {
                    sector: 3,
                    first: true
                },
                Event::Damage {
                    sector: 3,
                    amount: 6
                }
            ]
        );
    }

    #[test]
    fn previews_match_what_the_rules_accept() {
        let mut game = Game::new();
        let mut checked = 0;
        for tick in 0..3000 {
            if tick % 24 == 0
                && game.result == Outcome::Playing
                && game.transit.is_none()
                && game.encounter.is_none()
            {
                for (page, rows) in [(0, SECTORS), (1, 5), (2, 6), (3, 6), (4, 1)] {
                    for row in 0..rows {
                        let mut probe = game.clone();
                        probe.page = page;
                        probe.row = row;
                        probe.selected = row;
                        let refusal = match page {
                            0 => probe.jump_refusal(row),
                            1 => probe.work_refusal(row),
                            2 => probe.forge_refusal(row),
                            3 => probe.service_refusal(row),
                            _ => probe.gate_refusal(),
                        };
                        let cue = probe.step(&Input {
                            primary: true,
                            ..Input::default()
                        });
                        assert_eq!(
                            refusal.is_none(),
                            cue != Cue::None,
                            "page {page} row {row} at tick {tick}"
                        );
                        if let Some(reason) = refusal {
                            assert_eq!(
                                probe.events.iter().collect::<Vec<_>>(),
                                vec![Event::Refuse(reason)]
                            );
                        }
                        checked += 1;
                    }
                }
                for (sector, encounter) in [
                    (4, Encounter::Choir),
                    (7, Encounter::Sentinel),
                    (6, Encounter::Privateers),
                ] {
                    for choice in 0..3 {
                        let mut probe = game.clone();
                        probe.sector = sector;
                        probe.encounter = Some(encounter);
                        probe.choice = choice;
                        let refusal = probe.approach_refusal(encounter, choice);
                        let cue = probe.step(&Input {
                            primary: true,
                            ..Input::default()
                        });
                        assert_eq!(
                            refusal.is_none(),
                            cue != Cue::None,
                            "{encounter:?} {choice}"
                        );
                        checked += 1;
                    }
                }
            }
            let input = game.pilot(tick);
            game.step(&input);
            if game.result != Outcome::Playing {
                break;
            }
        }
        assert_eq!(game.result, Outcome::Won);
        assert!(checked > 1500, "{checked}");
    }
}
