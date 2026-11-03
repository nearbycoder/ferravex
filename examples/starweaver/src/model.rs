//! Pure, bounded rules for a complete stellar salvage expedition.
use crate::content::*;
use ferravex_arcade::{Cue, Input, Outcome};

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
    pub(crate) ticks: u64,
    pub(crate) notice: String,
    pub(crate) logs: [String; 4],
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
            ticks: 0,
            notice: "Recover three relics, forge their mechanisms, and return to the gate.".into(),
            logs: std::array::from_fn(|_| String::new()),
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
    fn say(&mut self, message: impl Into<String>) {
        self.notice = message.into();
    }
    fn record(&mut self, message: impl Into<String>) {
        let text = message.into();
        self.notice = text.clone();
        self.logs.rotate_left(1);
        self.logs[3] = text;
    }
    fn spend_watch(&mut self) -> bool {
        self.watch = self.watch.saturating_add(1);
        if self.watch >= DEADLINE {
            self.result = Outcome::Lost;
            self.record("The star collapses before the ark can cross. The expedition is over.");
            false
        } else {
            true
        }
    }
    fn damage(&mut self, raw: u16) {
        let amount = if self.upgrades[0] {
            raw.div_ceil(2)
        } else {
            raw
        };
        self.hull = self.hull.saturating_sub(amount);
        if self.hull == 0 {
            self.result = Outcome::Lost;
            self.record("Hull breach. Your last transmission gives the ark a map, but no escape.");
        }
    }
    fn jump(&mut self) -> Cue {
        if !Self::adjacent(self.sector, self.selected) {
            self.say("No charted lane to that star. Select a connected sector.");
            return Cue::None;
        }
        if self.fuel == 0 {
            self.say("No fuel. Condense a crystal or use the one-use distress tow on Ship [4].");
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
        self.record(format!(
            "Lane engaged: {}. One fuel and one watch spent.",
            CHART[self.selected].name
        ));
        Cue::Action
    }
    fn survey(&mut self) -> Cue {
        if self.row == 0 {
            if self.sites[self.sector].scanned {
                self.say("This sector is already mapped. Its deposits do not regenerate.");
                return Cue::None;
            }
            self.sites[self.sector].scanned = true;
            self.points += 75;
            self.record(format!(
                "Survey complete: {}. The local caches are now marked.",
                CHART[self.sector].name
            ));
            return if self.spend_watch() {
                Cue::Good
            } else {
                Cue::Bad
            };
        }
        if !self.sites[self.sector].scanned {
            self.say("Scan this sector first: select Survey sector and press Space.");
            return Cue::None;
        }
        if self.row == 4 {
            return self.explore();
        }
        if !(1..=3).contains(&self.row) {
            return Cue::None;
        }
        let kind = self.row - 1;
        let quantity = if self.upgrades[2] { 3 } else { 2 };
        let amount = quantity
            .min(self.sites[self.sector].stock[kind])
            .min(self.hold_max() - self.hold_used());
        if amount == 0 {
            self.say(if self.hold_used() == self.hold_max() {
                "Cargo full. Forge, sell, or X-jettison a selected resource."
            } else {
                "That deposit is exhausted. The chart shows other sources."
            });
            return Cue::None;
        }
        self.sites[self.sector].stock[kind] -= amount;
        self.cargo[kind] += amount;
        self.record(format!(
            "Recovered {amount} {}. Radiation costs {} hull before shielding.",
            RESOURCE_NAMES[kind], CHART[self.sector].extraction_damage
        ));
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
    fn explore(&mut self) -> Cue {
        if self.sites[self.sector].claimed {
            self.say("This landmark has already been recovered. There is nothing left to claim.");
            return Cue::None;
        }
        match self.sector {
            0 => {
                self.say("The gate awaits three mechanisms. Open Mission [5] when they are ready.");
                Cue::None
            }
            1 => {
                if self.hold_max() - self.hold_used() < 2 {
                    self.say("The freight cache needs two free cargo spaces.");
                    return Cue::None;
                }
                self.cargo[0] += 2;
                self.sites[1].claimed = true;
                self.points += 100;
                self.record("A buried freight capsule yields two alloy and a fragment of the old colony flag.");
                if self.spend_watch() {
                    Cue::Good
                } else {
                    Cue::Bad
                }
            }
            2 => {
                if self.fuel == self.fuel_max() {
                    self.say("Your fuel tank is full. Leave this one-use collector for later.");
                    return Cue::None;
                }
                self.fuel = (self.fuel + 4).min(self.fuel_max());
                self.sites[2].claimed = true;
                self.points += 100;
                self.record("The comet collector transfers its final four fuel units. The collector falls dark.");
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
                self.record(
                    "Navigator's key recovered. The Beacon spindle recipe is now unlocked.",
                );
                if self.spend_watch() {
                    Cue::Good
                } else {
                    Cue::Bad
                }
            }
            4 => {
                self.encounter = Some(Encounter::Choir);
                self.choice = 0;
                Cue::Action
            }
            5 => {
                if self.hold_max() - self.hold_used() < 2 {
                    self.say("The archive's seed crystals need two free cargo spaces.");
                    return Cue::None;
                }
                self.cargo[1] += 2;
                self.credits += 15;
                self.sites[5].claimed = true;
                self.points += 150;
                self.record(
                    "The archive entrusts two seed crystals and 15 credits to the ark's new world.",
                );
                if self.spend_watch() {
                    Cue::Good
                } else {
                    Cue::Bad
                }
            }
            6 => {
                self.encounter = Some(Encounter::Privateers);
                self.choice = 0;
                Cue::Action
            }
            7 => {
                self.encounter = Some(Encounter::Sentinel);
                self.choice = 0;
                Cue::Action
            }
            8 => {
                self.crew = true;
                self.sites[8].claimed = true;
                self.credits += 25;
                self.points += 350;
                self.hull = (self.hull + 10).min(100);
                self.record("Nine engineers rescued. They patch ten hull and pledge 25 credits to the gate.");
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
        match (encounter, self.choice) {
            (Encounter::Choir, 0) => {
                if self.fuel == 0 {
                    self.say(
                        "The probe needs one fuel. Choose a different approach or return later.",
                    );
                    return Cue::None;
                }
                self.fuel -= 1;
                self.relics[1] = true;
                self.record("A powered probe lifts the Singing lens without cracking your hull.");
            }
            (Encounter::Choir, 1) => {
                self.damage(18);
                self.relics[1] = true;
                if self.hold_used() < self.hold_max() {
                    self.cargo[1] += 1;
                }
                self.record("You dive into the fracture: Singing lens and a crystal recovered, 18 hull at risk.");
            }
            (Encounter::Choir, 2) => {
                self.encounter = None;
                self.record("The Choir falls quiet. You can return to this decision later.");
                return Cue::Action;
            }
            (Encounter::Sentinel, 0) => {
                if self.cargo[2] < 2 {
                    self.say("The override requires two circuits. No resources were spent.");
                    return Cue::None;
                }
                self.cargo[2] -= 2;
                self.relics[2] = true;
                self.record("Two circuits persuade the sentinel to release its Heart core.");
            }
            (Encounter::Sentinel, 1) => {
                if self.credits < 30 {
                    self.say("The sentinel's toll is 30 credits. Choose another approach.");
                    return Cue::None;
                }
                self.credits -= 30;
                self.relics[2] = true;
                self.record("The ancient toll is paid. The Sentinel core is yours.");
            }
            (Encounter::Sentinel, 2) => {
                self.damage(34);
                self.relics[2] = true;
                self.record(
                    "The vault yields to force. You recover its core through a storm of shrapnel.",
                );
            }
            (Encounter::Privateers, 0) => {
                if self.cargo[0] == 0 || self.hold_max() - self.hold_used() < 2 {
                    self.say("The trade needs one alloy and two free cargo spaces for a net gain of two units.");
                    return Cue::None;
                }
                self.cargo[0] -= 1;
                self.cargo[2] += 3;
                self.record("A fair trade: one alloy for three privateer circuits.");
            }
            (Encounter::Privateers, 1) => {
                if self.fuel < 2 {
                    self.say("Escaping the privateers needs two fuel.");
                    return Cue::None;
                }
                self.fuel -= 2;
                self.record("You burn two fuel and leave the privateers behind.");
            }
            (Encounter::Privateers, 2) => {
                self.damage(22);
                self.credits += 40;
                self.record(
                    "You take the captain's purse: 40 credits, but the guns scar your hull.",
                );
            }
            _ => return Cue::None,
        }
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
        let done = if self.row < 3 {
            self.components[self.row]
        } else {
            self.upgrades[self.row - 3]
        };
        if done {
            self.say("That mechanism is already fitted. It cannot be crafted twice.");
            return Cue::None;
        }
        if self.row < 3 && !self.relics[self.row] {
            self.say(format!(
                "Missing relic: {}. Recover it from {}.",
                RELIC_NAMES[self.row], CHART[RELIC_SECTORS[self.row]].name
            ));
            return Cue::None;
        }
        if (0..3).any(|i| self.cargo[i] < recipe.cost[i]) {
            self.say("Not enough materials. The selected recipe lists every required cargo unit.");
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
        self.record(format!(
            "{} fabricated and fitted. Its materials have been consumed.",
            recipe.name
        ));
        if self.spend_watch() {
            Cue::Good
        } else {
            Cue::Bad
        }
    }
    fn service(&mut self) -> Cue {
        match self.row {
            0 => {
                if self.cargo[1] == 0 || self.fuel == self.fuel_max() {
                    self.say("Condensing needs one crystal and room in the fuel tank.");
                    return Cue::None;
                }
                self.cargo[1] -= 1;
                self.fuel = (self.fuel + 3).min(self.fuel_max());
                self.record("One crystal condensed into up to three fuel units.");
            }
            1 => {
                if self.cargo[0] == 0 || self.cargo[2] == 0 || self.hull == 100 {
                    self.say("A field patch needs one alloy, one circuit and damaged hull.");
                    return Cue::None;
                }
                self.cargo[0] -= 1;
                self.cargo[2] -= 1;
                self.hull = (self.hull + 25).min(100);
                self.record("One alloy and one circuit seal 25 hull damage.");
            }
            2 => {
                if !CHART[self.sector].station || self.credits < 15 || self.fuel == self.fuel_max()
                {
                    self.say("Fuel service: dock at Loom or Verdant, bring 15 credits, and leave tank space.");
                    return Cue::None;
                }
                self.credits -= 15;
                self.fuel = (self.fuel + 5).min(self.fuel_max());
                self.record("Station fuel purchased: up to five units for 15 credits.");
            }
            3 => {
                if !CHART[self.sector].station || self.credits < 20 || self.hull == 100 {
                    self.say("Dry dock: Loom or Verdant, 20 credits, and damaged hull required.");
                    return Cue::None;
                }
                self.credits -= 20;
                self.hull = (self.hull + 45).min(100);
                self.record("The dry dock repairs up to 45 hull for 20 credits.");
            }
            4 => {
                if !CHART[self.sector].station || self.cargo[self.sell_kind] == 0 {
                    self.say(
                        "Sell cargo only at Loom or Verdant. X changes the selected resource.",
                    );
                    return Cue::None;
                }
                self.cargo[self.sell_kind] -= 1;
                let price = [8, 12, 16][self.sell_kind];
                self.credits += price;
                self.record(format!(
                    "Sold one {} for {price} credits.",
                    RESOURCE_NAMES[self.sell_kind]
                ));
            }
            5 => {
                if !self.tow {
                    self.say(
                        "The one-use tow has been spent. Refuel from crystals, or R to begin anew.",
                    );
                    return Cue::None;
                }
                if self.sector == 0 {
                    self.say("Already at Loom Dock. The distress charge is preserved.");
                    return Cue::None;
                }
                self.tow = false;
                self.sector = 0;
                self.selected = 1;
                self.fuel = 5;
                self.damage(15);
                for _ in 0..5 {
                    if self.result == Outcome::Playing {
                        self.spend_watch();
                    }
                }
                self.record(
                    "Distress tow: Loom Dock, five fuel, fifteen hull damage and six watches lost.",
                );
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
        if self.sector != 0 {
            self.say("The ark and gate are at Loom Dock. Bring the mechanisms home.");
            return Cue::None;
        }
        if self.built() != 3 {
            self.say("All three gate mechanisms must be fabricated before the ark can cross.");
            return Cue::None;
        }
        if !self.spend_watch() {
            return Cue::Bad;
        }
        self.points += 1200
            + u32::from(DEADLINE - self.watch) * 25
            + u32::from(self.hull) * 3
            + self.credits.min(500);
        self.result = Outcome::Won;
        self.record(if self.crew {
            "The gate blooms. The ark and all nine engineers cross into a new dawn."
        } else {
            "The gate blooms. The ark crosses, carrying the unanswered beacon in its log."
        });
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
        self.ticks = self.ticks.wrapping_add(1);
        if let Some(mut flight) = self.transit {
            flight.remaining = flight.remaining.saturating_sub(1);
            if flight.remaining == 0 {
                self.sector = flight.to;
                self.transit = None;
                if !self.sites[self.sector].visited {
                    self.sites[self.sector].visited = true;
                    self.damage(CHART[self.sector].arrival_damage);
                }
                if self.result == Outcome::Playing {
                    self.record(format!(
                        "Arrived at {}. Survey [2] reveals finite deposits and landmarks.",
                        CHART[self.sector].name
                    ));
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
            if input.click {
                for i in 0..3 {
                    if (130. ..830.).contains(&input.pointer.x)
                        && (330. + i as f32 * 62. ..378. + i as f32 * 62.)
                            .contains(&input.pointer.y)
                    {
                        self.choice = i;
                        act = true;
                    }
                }
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
            && (116. ..145.).contains(&input.pointer.y)
            && (24. ..936.).contains(&input.pointer.x)
        {
            self.switch_page(((input.pointer.x - 24.) / 184.) as usize);
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
                self.record(format!(
                    "Jettisoned one {}. No credits or score awarded.",
                    RESOURCE_NAMES[kind]
                ));
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
                for (i, site) in CHART.iter().enumerate() {
                    let dx = input.pointer.x - site.x;
                    let dy = input.pointer.y - site.y;
                    if dx * dx + dy * dy < 29. * 29. {
                        self.selected = i;
                    }
                }
            } else if self.page < 4 && (640. ..922.).contains(&input.pointer.x) {
                let top = if self.page == 1 { 282. } else { 230. };
                for i in 0..self.rows() {
                    let y = top + i as f32 * 34.;
                    if (y..y + 30.).contains(&input.pointer.y) {
                        self.row = i;
                        act = true;
                    }
                }
            }
            if (640. ..922.).contains(&input.pointer.x) && (526. ..556.).contains(&input.pointer.y)
            {
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
        assert_eq!(game.result, Outcome::Won, "{}", game.notice);
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
}
