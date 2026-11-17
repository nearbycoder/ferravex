//! Pure rules for fourteen days on a small village farm.
//!
//! There is no wall clock: only accepted actions consume resources, and a
//! confirmed sleep advances the calendar. The renderer supplies fixed ticks.
//!
//! ```
//! use ferravex_arcade::{Input, Outcome};
//! use hearthharvest::Game;
//! let mut farm = Game::new();
//! let energy = farm.energy();
//! farm.tick(&Input { primary: true, ..Input::default() }); // Plant turnip.
//! assert_eq!(farm.seeds(0), 4);
//! assert_eq!(farm.energy(), energy - 2);
//! farm.tick(&Input { primary: true, ..Input::default() }); // Water it.
//! assert_eq!(farm.outcome(), Outcome::Playing);
//! assert_eq!(farm.day(), 1);
//! ```
use crate::content::{CROPS, MORNINGS, ORDERS, TOOL_PRICES, Weather};
use ferravex_arcade::{Cue, Input, Outcome};
use ferravex_math::Vec2;
use ferravex_play::gameplay::inventory::{Inventory, Stack};

pub(crate) const COLS: usize = 4;
pub(crate) const PLOTS: usize = 12;
pub(crate) const GRID_X: f32 = 44.;
pub(crate) const GRID_Y: f32 = 242.;
pub(crate) const PITCH_X: f32 = 136.;
pub(crate) const PITCH_Y: f32 = 80.;
pub(crate) const PLOT_W: f32 = 124.;
pub(crate) const PLOT_H: f32 = 70.;
/// Most harvested crops of one kind the basket holds.
const PRODUCE_LIMIT: u32 = 99;

/// A pointer target in logical canvas units: `[x, y, w, h]`. The view draws
/// these areas and the rules hit-test the same ones, so they never drift.
pub(crate) type Area = [f32; 4];
pub(crate) fn inside(area: Area, p: Vec2) -> bool {
    p.x >= area[0] && p.x < area[0] + area[2] && p.y >= area[1] && p.y < area[1] + area[3]
}
/// Page tabs across the top of the play area.
pub(crate) fn tab_area(i: usize) -> Area {
    [22. + i as f32 * 120., 116., 120., 34.]
}
pub(crate) fn plot_area(i: usize) -> Area {
    [
        GRID_X + (i % COLS) as f32 * PITCH_X,
        GRID_Y + (i / COLS) as f32 * PITCH_Y,
        PLOT_W,
        PLOT_H,
    ]
}
/// Centre of a field plot, where its effects play.
pub(crate) fn plot_center(i: usize) -> Vec2 {
    let a = plot_area(i);
    Vec2::new(a[0] + a[2] * 0.5, a[1] + a[3] * 0.5)
}
/// Seed packets under the field.
pub(crate) fn seed_area(i: usize) -> Area {
    [44. + i as f32 * 166., 500., 156., 40.]
}
/// Stall, village and workshop rows.
pub(crate) fn row_area(i: usize) -> Area {
    [40., 196. + i as f32 * 98., 564., 88.]
}
/// A row's buttons: slot 0 buys at the stall; slot 1 sells at the stall,
/// buys a tool in the workshop and delivers the current village order.
pub(crate) fn button_area(i: usize, slot: usize) -> Area {
    let row = row_area(i);
    [if slot == 0 { 372. } else { 488. }, row[1] + 20., 108., 48.]
}
/// Workshop compost button (acts on the selected field plot).
pub(crate) const COMPOST: Area = [488., 496., 108., 48.];
/// Ledger button that performs the page's Space action.
pub(crate) const ACT: Area = [656., 504., 206., 48.];
/// Ledger button that opens the sleep card, like C.
pub(crate) const SLEEP: Area = [872., 504., 50., 48.];
/// Sleep card confirm and cancel buttons.
pub(crate) const CONFIRM: Area = [128., 452., 302., 44.];
pub(crate) const CANCEL: Area = [450., 452., 300., 44.];

pub(crate) fn tab_at(p: Vec2) -> Option<Page> {
    (0..5)
        .find(|&i| inside(tab_area(i), p))
        .and_then(|i| Page::from_slot(i + 1))
}
pub(crate) fn plot_at(p: Vec2) -> Option<usize> {
    (0..PLOTS).find(|&i| inside(plot_area(i), p))
}
pub(crate) fn seed_at(p: Vec2) -> Option<usize> {
    (0..3).find(|&i| inside(seed_area(i), p))
}
pub(crate) fn row_at(p: Vec2) -> Option<usize> {
    (0..3).find(|&i| inside(row_area(i), p))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plant {
    pub crop: usize,
    pub age: u8,
    pub nights: u8,
    pub watered: bool,
    pub stress: u8,
    pub quality: u8,
}
impl Plant {
    pub fn ready(self) -> bool {
        self.age >= self.nights && self.stress < 3
    }
    pub fn wilted(self) -> bool {
        self.stress >= 3
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Plot {
    pub soil: u8,
    pub plant: Option<Plant>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    Field,
    Stall,
    Orders,
    Tools,
    Almanac,
}
impl Page {
    pub fn index(self) -> usize {
        match self {
            Self::Field => 0,
            Self::Stall => 1,
            Self::Orders => 2,
            Self::Tools => 3,
            Self::Almanac => 4,
        }
    }
    pub fn from_slot(slot: usize) -> Option<Self> {
        match slot {
            1 => Some(Self::Field),
            2 => Some(Self::Stall),
            3 => Some(Self::Orders),
            4 => Some(Self::Tools),
            5 => Some(Self::Almanac),
            _ => None,
        }
    }
}
/// What Space does on a field plot, decided by the rules' own checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Work {
    Plant,
    Water,
    Harvest,
    Clear,
}
/// Why an action was refused; nothing is spent when it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    NoSeed,
    Tired,
    ShelfFull,
    Watered,
    Occupied,
    Rich,
    Broke,
    SoldOut,
    TinFull,
    NoProduce,
    Done,
    Locked,
    Short,
    Owned,
}
/// Something the latest tick accepted (or refused), with where it happened.
/// Presentation reads these to play effects; the rules never do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    Plant {
        plot: usize,
    },
    Water {
        plot: usize,
    },
    Harvest {
        plot: usize,
        crop: usize,
    },
    Clear {
        plot: usize,
    },
    Compost {
        plot: usize,
    },
    Buy {
        crop: usize,
    },
    Sell {
        crop: usize,
    },
    Deliver {
        order: usize,
    },
    Tool {
        tool: usize,
    },
    /// A watered crop grew overnight; `ripe` when it is now ready.
    Grow {
        plot: usize,
        ripe: bool,
    },
    /// Frost paused a watered crop.
    Frozen {
        plot: usize,
    },
    /// A dry crop gained stress without wilting.
    Thirst {
        plot: usize,
    },
    Wilt {
        plot: usize,
    },
    /// A new morning began.
    Dawn,
    /// The festival was held (the season is won).
    Festival,
    /// The season ended without the festival.
    Failed,
    /// An action was refused at `Spot`; nothing changed.
    Refuse(Refusal, Spot),
}
/// The thing that refused an action, so its feedback plays there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Spot {
    Plot(usize),
    Buy(usize),
    Sell(usize),
    Tool(usize),
    Compost,
    Order(usize),
}
/// Capacity of the per-tick event list (a night touches at most twelve plots).
pub(crate) const EVENT_CAP: usize = 16;
/// Bounded, allocation-free list of the latest tick's [`Event`]s, cleared at
/// the start of every playing tick.
#[derive(Clone, Copy)]
pub(crate) struct Events {
    list: [Event; EVENT_CAP],
    len: usize,
}
impl Events {
    const fn new() -> Self {
        Self {
            list: [Event::Dawn; EVENT_CAP],
            len: 0,
        }
    }
    fn clear(&mut self) {
        self.len = 0;
    }
    fn push(&mut self, event: Event) {
        if self.len < EVENT_CAP {
            self.list[self.len] = event;
            self.len += 1;
        }
    }
    pub(crate) fn as_slice(&self) -> &[Event] {
        &self.list[..self.len]
    }
}
impl std::fmt::Debug for Events {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}
/// What sleeping tonight would do, previewed with the same rules as sleep.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Night {
    pub grow: u8,
    pub ripen: u8,
    pub frozen: u8,
    pub dry: u8,
    pub wilt: u8,
}
/// A bounded farm campaign. All player mutations pass through [`Self::tick`].
#[derive(Clone, Debug)]
pub struct Game {
    pub(crate) plots: [Plot; PLOTS],
    pub(crate) day: u8,
    pub(crate) energy: u8,
    pub(crate) coins: u32,
    pub(crate) seeds: [u32; 3],
    pub(crate) stock: [u32; 3],
    pub(crate) produce: Inventory<3>,
    pub(crate) tools: [bool; 3],
    pub(crate) delivered: [bool; 3],
    pub(crate) page: Page,
    pub(crate) cursor: usize,
    pub(crate) row: usize,
    pub(crate) crop: usize,
    pub(crate) sleep_prompt: bool,
    pub(crate) events: Events,
    pub(crate) result: Outcome,
    pub(crate) score: u32,
    pub(crate) harvested: u32,
    pub(crate) wilted: u32,
    pub(crate) rank: &'static str,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Begin day one with twelve fertile plots, 35 coins and eleven seeds.
    pub fn new() -> Self {
        Self {
            plots: [Plot {
                soil: 2,
                plant: None,
            }; PLOTS],
            day: 1,
            energy: 18,
            coins: 35,
            seeds: [5, 3, 3],
            stock: [4, 3, 2],
            produce: Inventory::new().expect("three produce stacks"),
            tools: [false; 3],
            delivered: [false; 3],
            page: Page::Field,
            cursor: 0,
            row: 0,
            crop: 0,
            sleep_prompt: false,
            events: Events::new(),
            result: Outcome::Playing,
            score: 0,
            harvested: 0,
            wilted: 0,
            rank: "",
        }
    }
    /// Current calendar day, from one through fourteen.
    pub fn day(&self) -> u8 {
        self.day
    }
    /// Remaining energy. Invalid actions do not spend it.
    pub fn energy(&self) -> u8 {
        self.energy
    }
    /// Current coins, including accepted sales and delivered orders.
    pub fn coins(&self) -> u32 {
        self.coins
    }
    /// Seed count for 0 turnip, 1 bean or 2 pumpkin; unknown IDs return zero.
    pub fn seeds(&self, crop: usize) -> u32 {
        self.seeds.get(crop).copied().unwrap_or(0)
    }
    /// Stored harvested produce; crops in the ground are not counted.
    pub fn produce(&self, crop: usize) -> u32 {
        if crop < 3 {
            self.produce.count(crop as u32)
        } else {
            0
        }
    }
    /// Number of completed village orders.
    pub fn orders_completed(&self) -> usize {
        self.delivered.iter().filter(|&&v| v).count()
    }
    /// Read the authoritative campaign outcome without advancing anything.
    pub fn outcome(&self) -> Outcome {
        self.result
    }
    /// Score from accepted harvests, deliveries and the final festival.
    pub fn score(&self) -> u32 {
        self.score
    }
    pub(crate) fn weather(&self) -> Weather {
        MORNINGS[usize::from(self.day - 1)].weather
    }
    /// Tomorrow's authored weather, or `None` on the festival day.
    pub(crate) fn tomorrow(&self) -> Option<Weather> {
        MORNINGS.get(usize::from(self.day)).map(|m| m.weather)
    }
    pub(crate) fn max_energy(&self) -> u8 {
        18 + if self.tools[1] { 6 } else { 0 }
    }
    pub(crate) fn order(&self) -> Option<usize> {
        self.delivered.iter().position(|&v| !v)
    }
    pub(crate) fn needs(&self) -> [u32; 3] {
        let mut needs = [0; 3];
        for (i, order) in ORDERS.iter().enumerate() {
            if !self.delivered[i] {
                for (n, amount) in needs.iter_mut().zip(order.needs) {
                    *n += amount;
                }
            }
        }
        needs
    }
    pub(crate) fn growing(&self, crop: usize) -> u32 {
        self.plots
            .iter()
            .filter(|p| p.plant.is_some_and(|v| v.crop == crop && !v.wilted()))
            .count() as u32
    }
    pub(crate) fn dry(&self) -> usize {
        self.plots
            .iter()
            .filter(|p| {
                p.plant
                    .is_some_and(|v| !v.watered && !v.ready() && !v.wilted())
            })
            .count()
    }
    /// Crops ready to harvest.
    pub(crate) fn ripe(&self) -> usize {
        self.plots
            .iter()
            .filter(|p| p.plant.is_some_and(Plant::ready))
            .count()
    }
    /// True when sleeping now ends the season at an unserved deadline.
    pub(crate) fn deadline_tonight(&self) -> Option<usize> {
        self.order().filter(|&i| self.day >= ORDERS[i].deadline)
    }
    /// Preview tonight's growth, frost and thirst without sleeping.
    pub(crate) fn night(&self) -> Night {
        let weather = self.weather();
        let mut night = Night::default();
        for plant in self.plots.iter().filter_map(|p| p.plant) {
            if plant.ready() || plant.wilted() {
                continue;
            }
            if plant.watered {
                if weather != Weather::Frost || self.tools[2] {
                    night.grow += 1;
                    night.ripen += u8::from(plant.age + 1 >= plant.nights);
                } else {
                    night.frozen += 1;
                }
            } else {
                night.dry += 1;
                let stress = if weather == Weather::Heat { 2 } else { 1 };
                night.wilt += u8::from(plant.stress + stress >= 3);
            }
        }
        night
    }
    /// Latest tick's presentation events.
    pub(crate) fn events(&self) -> &[Event] {
        self.events.as_slice()
    }
    fn reject(&mut self, why: Refusal, at: Spot) -> Cue {
        self.events.push(Event::Refuse(why, at));
        Cue::None
    }
    /// What Space would do on plot `index`, or why it would be refused.
    pub(crate) fn field_check(&self, index: usize) -> Result<Work, Refusal> {
        match self.plots[index].plant {
            None if self.seeds[self.crop] == 0 => Err(Refusal::NoSeed),
            None if self.energy < 2 => Err(Refusal::Tired),
            None => Ok(Work::Plant),
            Some(plant) if plant.wilted() => {
                if self.energy == 0 {
                    Err(Refusal::Tired)
                } else {
                    Ok(Work::Clear)
                }
            }
            Some(plant) if plant.ready() => {
                if self.energy == 0 {
                    Err(Refusal::Tired)
                } else if self.produce(plant.crop) >= PRODUCE_LIMIT {
                    Err(Refusal::ShelfFull)
                } else {
                    Ok(Work::Harvest)
                }
            }
            Some(plant) if plant.watered => Err(Refusal::Watered),
            Some(_) if self.energy == 0 => Err(Refusal::Tired),
            Some(_) => Ok(Work::Water),
        }
    }
    /// Plots one watering reaches from `index`: the plot, and with the Brass
    /// Can its right and lower neighbors (never wrapping a row).
    pub(crate) fn reach(&self, index: usize) -> [Option<usize>; 3] {
        [
            Some(index),
            if self.tools[0] && index % COLS + 1 < COLS {
                Some(index + 1)
            } else {
                None
            },
            if self.tools[0] && index + COLS < PLOTS {
                Some(index + COLS)
            } else {
                None
            },
        ]
    }
    fn field_action(&mut self) -> Cue {
        let index = self.cursor;
        let plot = self.plots[index];
        match self.field_check(index) {
            Err(why) => self.reject(why, Spot::Plot(index)),
            Ok(Work::Plant) => {
                self.seeds[self.crop] -= 1;
                self.energy -= 2;
                self.plots[index].plant = Some(Plant {
                    crop: self.crop,
                    age: 0,
                    nights: CROPS[self.crop].nights + u8::from(plot.soil == 0),
                    watered: self.weather() == Weather::Rain,
                    stress: 0,
                    quality: plot.soil,
                });
                self.events.push(Event::Plant { plot: index });
                Cue::Action
            }
            Ok(Work::Clear) => {
                self.energy -= 1;
                self.plots[index].plant = None;
                self.events.push(Event::Clear { plot: index });
                Cue::Action
            }
            Ok(Work::Harvest) => {
                let plant = plot.plant.expect("checked ripe crop");
                if self
                    .produce
                    .add(Stack {
                        item: plant.crop as u32,
                        amount: 1,
                        limit: PRODUCE_LIMIT,
                    })
                    .is_err()
                {
                    return self.reject(Refusal::ShelfFull, Spot::Plot(index));
                }
                self.energy -= 1;
                self.plots[index].plant = None;
                self.plots[index].soil = self.plots[index].soil.saturating_sub(1);
                self.harvested += 1;
                self.score += 20 + u32::from(plant.quality) * 5;
                self.events.push(Event::Harvest {
                    plot: index,
                    crop: plant.crop,
                });
                Cue::Good
            }
            Ok(Work::Water) => {
                self.energy -= 1;
                for i in self.reach(index).into_iter().flatten() {
                    if let Some(p) = &mut self.plots[i].plant
                        && !p.watered
                        && !p.wilted()
                        && !p.ready()
                    {
                        p.watered = true;
                        self.events.push(Event::Water { plot: i });
                    }
                }
                Cue::Action
            }
        }
    }
    /// Why composting the selected plot would be refused, if it would be.
    pub(crate) fn compost_check(&self) -> Result<(), Refusal> {
        let plot = self.plots[self.cursor];
        if plot.plant.is_some() {
            Err(Refusal::Occupied)
        } else if plot.soil == 2 {
            Err(Refusal::Rich)
        } else if self.energy == 0 {
            Err(Refusal::Tired)
        } else if self.coins < 2 {
            Err(Refusal::Broke)
        } else {
            Ok(())
        }
    }
    fn compost(&mut self) -> Cue {
        if let Err(why) = self.compost_check() {
            return self.reject(why, Spot::Compost);
        }
        self.plots[self.cursor].soil = 2;
        self.coins -= 2;
        self.energy -= 1;
        self.events.push(Event::Compost { plot: self.cursor });
        Cue::Good
    }
    /// Why buying one `crop` seed would be refused, if it would be.
    pub(crate) fn seed_check(&self, crop: usize) -> Result<(), Refusal> {
        if self.coins < CROPS[crop].seed {
            Err(Refusal::Broke)
        } else if self.stock[crop] == 0 {
            Err(Refusal::SoldOut)
        } else if self.seeds[crop] >= 24 {
            Err(Refusal::TinFull)
        } else {
            Ok(())
        }
    }
    fn buy_seed(&mut self) -> Cue {
        let crop = self.row.min(2);
        if let Err(why) = self.seed_check(crop) {
            return self.reject(why, Spot::Buy(crop));
        }
        self.coins -= CROPS[crop].seed;
        self.stock[crop] -= 1;
        self.seeds[crop] += 1;
        self.events.push(Event::Buy { crop });
        Cue::Action
    }
    fn sell(&mut self) -> Cue {
        let crop = self.row.min(2);
        if self.produce.remove(crop as u32, 1).is_err() {
            return self.reject(Refusal::NoProduce, Spot::Sell(crop));
        }
        self.coins += CROPS[crop].sale;
        self.events.push(Event::Sell { crop });
        Cue::Good
    }
    /// The order Space would deliver, or why it would be refused.
    pub(crate) fn deliver_check(&self) -> Result<usize, Refusal> {
        let Some(index) = self.order() else {
            return Err(Refusal::Done);
        };
        let order = &ORDERS[index];
        if self.day < order.opens {
            Err(Refusal::Locked)
        } else if (0..3).any(|c| self.produce(c) < order.needs[c]) {
            Err(Refusal::Short)
        } else {
            Ok(index)
        }
    }
    fn deliver(&mut self) -> Cue {
        let index = match self.deliver_check() {
            Ok(index) => index,
            Err(why) => return self.reject(why, Spot::Order(self.order().unwrap_or(2))),
        };
        for (crop, &amount) in ORDERS[index].needs.iter().enumerate() {
            if amount > 0 {
                self.produce
                    .remove(crop as u32, amount)
                    .expect("preflighted produce");
            }
        }
        self.delivered[index] = true;
        self.coins += ORDERS[index].reward;
        self.score += 400;
        self.events.push(Event::Deliver { order: index });
        Cue::Good
    }
    /// Why buying tool `index` would be refused, if it would be.
    pub(crate) fn tool_check(&self, index: usize) -> Result<(), Refusal> {
        if self.tools[index] {
            Err(Refusal::Owned)
        } else if self.coins < TOOL_PRICES[index] {
            Err(Refusal::Broke)
        } else {
            Ok(())
        }
    }
    fn buy_tool(&mut self) -> Cue {
        let index = self.row.min(2);
        if let Err(why) = self.tool_check(index) {
            return self.reject(why, Spot::Tool(index));
        }
        self.coins -= TOOL_PRICES[index];
        self.tools[index] = true;
        if index == 1 {
            self.energy += 6;
        }
        self.events.push(Event::Tool { tool: index });
        Cue::Good
    }
    fn sleep(&mut self) -> Cue {
        self.sleep_prompt = false;
        if self.deadline_tonight().is_some() {
            self.result = Outcome::Lost;
            self.rank = "An Empty Table";
            self.events.push(Event::Failed);
            return Cue::Bad;
        }
        if self.day == 14 {
            if self.coins < 80 {
                self.result = Outcome::Lost;
                self.rank = "A Cold Oven";
                self.events.push(Event::Failed);
                return Cue::Bad;
            }
            self.result = Outcome::Won;
            self.rank = if self.tools.iter().all(|&v| v) && self.coins >= 120 && self.wilted == 0 {
                "Hearthkeeper"
            } else if self.coins >= 160 && self.harvested >= 24 {
                "Golden Harvest"
            } else {
                "Village Gardener"
            };
            self.score += self.coins * 4
                + self.tools.iter().filter(|&&v| v).count() as u32 * 150
                + 14u32.saturating_sub(self.wilted) * 10;
            self.events.push(Event::Festival);
            return Cue::Win;
        }
        let weather = self.weather();
        for (index, plot) in self.plots.iter_mut().enumerate() {
            if let Some(plant) = &mut plot.plant {
                if plant.ready() || plant.wilted() {
                    continue;
                }
                if plant.watered {
                    plant.stress = 0;
                    if weather != Weather::Frost || self.tools[2] {
                        plant.age += 1;
                        self.events.push(Event::Grow {
                            plot: index,
                            ripe: plant.ready(),
                        });
                    } else {
                        self.events.push(Event::Frozen { plot: index });
                    }
                } else {
                    plant.stress += if weather == Weather::Heat { 2 } else { 1 };
                    if plant.wilted() {
                        self.wilted += 1;
                        self.events.push(Event::Wilt { plot: index });
                    } else {
                        self.events.push(Event::Thirst { plot: index });
                    }
                }
            }
        }
        self.day += 1;
        self.energy = self.max_energy();
        self.stock = [4, 3, 2];
        let rain = self.weather() == Weather::Rain;
        for plot in &mut self.plots {
            if let Some(plant) = &mut plot.plant {
                plant.watered = rain;
            }
        }
        self.events.push(Event::Dawn);
        Cue::Good
    }
    /// Route a click through the same rule paths as the keys it mirrors.
    /// Returns `None` when the click hit nothing interactive.
    fn click(&mut self, point: Vec2) -> Option<Cue> {
        if !point.is_finite() {
            return None;
        }
        if let Some(page) = tab_at(point) {
            self.page = page;
            self.row = 0;
            return Some(Cue::Action);
        }
        match self.page {
            Page::Field => {
                if let Some(i) = plot_at(point) {
                    // A second click on the selected plot works it, like Space.
                    if self.cursor == i {
                        return Some(self.field_action());
                    }
                    self.cursor = i;
                    return Some(Cue::Action);
                }
                if let Some(crop) = seed_at(point) {
                    self.crop = crop;
                    return Some(Cue::Action);
                }
                return None;
            }
            Page::Stall => {
                for i in 0..3 {
                    if inside(button_area(i, 0), point) {
                        self.row = i;
                        return Some(self.buy_seed());
                    }
                    if inside(button_area(i, 1), point) {
                        self.row = i;
                        return Some(self.sell());
                    }
                }
            }
            Page::Tools => {
                if inside(COMPOST, point) {
                    return Some(self.compost());
                }
                for i in 0..3 {
                    if inside(button_area(i, 1), point) {
                        self.row = i;
                        return Some(self.buy_tool());
                    }
                }
            }
            Page::Orders => {
                if let Some(i) = self.order()
                    && inside(button_area(i, 1), point)
                {
                    return Some(self.deliver());
                }
            }
            Page::Almanac => {}
        }
        let i = row_at(point)?;
        self.row = i;
        Some(Cue::Action)
    }
    /// Execute one fixed update. Edges choose actions; held directions never repeat.
    /// Invalid actions may record a refusal for presentation but preserve money,
    /// crops and time.
    pub fn tick(&mut self, input: &Input) -> Cue {
        if self.result != Outcome::Playing {
            return Cue::None;
        }
        self.events.clear();
        if self.sleep_prompt {
            if input.primary {
                return self.sleep();
            }
            if input.secondary || input.tertiary {
                self.sleep_prompt = false;
                return Cue::Action;
            }
            if input.click {
                if inside(CONFIRM, input.pointer) {
                    return self.sleep();
                }
                if inside(CANCEL, input.pointer) {
                    self.sleep_prompt = false;
                    return Cue::Action;
                }
            }
            return Cue::None;
        }
        if input.tertiary || (input.click && inside(SLEEP, input.pointer)) {
            self.sleep_prompt = true;
            return Cue::Action;
        }
        if let Some(page) = input.slot.and_then(Page::from_slot) {
            self.page = page;
            self.row = 0;
            return Cue::Action;
        }
        if input.click && inside(ACT, input.pointer) {
            return self.tick(&Input {
                primary: true,
                ..Input::default()
            });
        }
        if input.click
            && let Some(cue) = self.click(input.pointer)
        {
            return cue;
        }
        let dx = i32::from(input.right_pressed) - i32::from(input.left_pressed);
        let dy = i32::from(input.down_pressed) - i32::from(input.up_pressed);
        if dx != 0 || dy != 0 {
            if self.page == Page::Field {
                let x = (self.cursor % COLS) as i32;
                let y = (self.cursor / COLS) as i32;
                self.cursor = ((y + dy).clamp(0, 2) * 4 + (x + dx).clamp(0, 3)) as usize;
            } else {
                self.row = (self.row as i32 + dy + dx).rem_euclid(3) as usize;
            }
            return Cue::Action;
        }
        if input.secondary {
            return match self.page {
                Page::Field => {
                    self.crop = (self.crop + 1) % 3;
                    Cue::Action
                }
                Page::Stall => self.sell(),
                Page::Tools => self.compost(),
                Page::Orders | Page::Almanac => Cue::None,
            };
        }
        if input.primary {
            return match self.page {
                Page::Field => self.field_action(),
                Page::Stall => self.buy_seed(),
                Page::Orders => self.deliver(),
                Page::Tools => self.buy_tool(),
                Page::Almanac => {
                    self.sleep_prompt = true;
                    Cue::Action
                }
            };
        }
        Cue::None
    }
    fn page_input(&self, page: Page) -> Option<Input> {
        if self.page != page {
            Some(Input {
                slot: Some(page.index() + 1),
                ..Input::default()
            })
        } else {
            None
        }
    }
    fn field_input(&self, index: usize) -> Input {
        self.page_input(Page::Field).unwrap_or_else(|| {
            if self.cursor != index {
                Input {
                    click: true,
                    pointer: plot_center(index),
                    ..Input::default()
                }
            } else {
                Input {
                    primary: true,
                    ..Input::default()
                }
            }
        })
    }
    fn row_input(&self, page: Page, row: usize, secondary: bool) -> Input {
        self.page_input(page).unwrap_or_else(|| {
            if self.row != row {
                Input {
                    down_pressed: true,
                    ..Input::default()
                }
            } else {
                Input {
                    primary: !secondary,
                    secondary,
                    ..Input::default()
                }
            }
        })
    }
    /// Choose ordinary commands for a repeatable successful season, without
    /// mutating the model or bypassing costs, weather, order windows or deadlines.
    pub fn autoplay(&self, tick: u64) -> Input {
        if self.result != Outcome::Playing || !tick.is_multiple_of(10) {
            return Input::default();
        }
        if self.sleep_prompt {
            return Input {
                primary: true,
                ..Input::default()
            };
        }
        if let Some(i) = self.order() {
            let order = &ORDERS[i];
            if self.day >= order.opens && (0..3).all(|c| self.produce(c) >= order.needs[c]) {
                return self.row_input(Page::Orders, 0, false);
            }
        }
        let needs = self.needs();
        // Sell only genuine surplus; never sell ingredients committed to future tables.
        if let Some(crop) = (0..3).find(|&c| self.produce(c) > needs[c]) {
            return self.row_input(Page::Stall, crop, true);
        }
        // Buy tools only with a reserve for seeds and the final oven contribution.
        let reserve = if self.orders_completed() == 3 { 80 } else { 12 };
        if let Some(tool) = (0..3).find(|&i| {
            !self.tools[i] && self.coins >= TOOL_PRICES[i] + reserve && (i < 2 || self.day >= 8)
        }) {
            return self.row_input(Page::Tools, tool, false);
        }
        if self.energy > 0 {
            if let Some(i) = self
                .plots
                .iter()
                .position(|p| p.plant.is_some_and(Plant::ready))
            {
                return self.field_input(i);
            }
            if let Some(i) = self
                .plots
                .iter()
                .position(|p| p.plant.is_some_and(|v| v.wilted()))
            {
                return self.field_input(i);
            }
            if let Some(i) = self
                .plots
                .iter()
                .position(|p| p.plant.is_some_and(|v| !v.watered && !v.ready()))
            {
                return self.field_input(i);
            }
        }
        if self.energy >= 3
            && self.day <= 11
            && let Some(index) = self.plots.iter().position(|p| p.plant.is_none())
        {
            let crop = (0..3)
                .find(|&c| self.produce(c) + self.growing(c) < needs[c])
                .or_else(|| (self.day <= 10).then_some(0));
            if let Some(crop) = crop {
                if self.seeds[crop] == 0 {
                    if self.stock[crop] > 0
                        && self.coins
                            >= CROPS[crop].seed + if self.orders_completed() == 3 { 80 } else { 0 }
                    {
                        return self.row_input(Page::Stall, crop, false);
                    }
                } else if self.crop != crop {
                    return self.page_input(Page::Field).unwrap_or(Input {
                        secondary: true,
                        ..Input::default()
                    });
                } else {
                    return self.field_input(index);
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
    fn primary(g: &mut Game) -> Cue {
        g.tick(&Input {
            primary: true,
            ..Input::default()
        })
    }
    fn sleep(g: &mut Game) -> Cue {
        g.tick(&Input {
            tertiary: true,
            ..Input::default()
        });
        primary(g)
    }
    fn put(g: &mut Game, crop: usize, amount: u32) {
        g.produce
            .add(Stack {
                item: crop as u32,
                amount,
                limit: 99,
            })
            .unwrap();
    }
    #[test]
    fn complete_fourteen_day_season_is_won_with_legal_inputs_only() {
        let mut g = Game::new();
        let mut seen = [false; 14];
        for tick in 0..100_000 {
            seen[usize::from(g.day - 1)] = true;
            let input = g.autoplay(tick);
            g.tick(&input);
            if g.result != Outcome::Playing {
                break;
            }
        }
        assert_eq!(
            g.result,
            Outcome::Won,
            "day {}, coins {}, energy {}, rank {}, basket {:?}, seeds {:?}",
            g.day,
            g.coins,
            g.energy,
            g.rank,
            [g.produce(0), g.produce(1), g.produce(2)],
            g.seeds
        );
        assert!(seen.iter().all(|&v| v));
        assert_eq!(g.orders_completed(), 3);
        assert!(g.coins >= 80);
        assert!(g.harvested >= 16);
        assert_eq!(g.day, 14);
    }
    #[test]
    fn planting_watering_and_repeated_invalid_actions_have_real_costs() {
        let mut g = Game::new();
        assert_eq!(primary(&mut g), Cue::Action);
        assert_eq!((g.energy, g.seeds[0]), (16, 4));
        assert_eq!(primary(&mut g), Cue::Action);
        assert_eq!(g.energy, 15);
        for _ in 0..100 {
            assert_eq!(primary(&mut g), Cue::None);
        }
        assert_eq!((g.energy, g.coins, g.score, g.seeds[0]), (15, 35, 0, 4));
        assert_eq!(g.plots[0].plant.unwrap().age, 0);
    }
    #[test]
    fn sleep_requires_confirmation_and_cancel_changes_no_resources() {
        let mut g = Game::new();
        primary(&mut g);
        g.tick(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert!(g.sleep_prompt);
        assert_eq!(g.day, 1);
        g.tick(&Input {
            secondary: true,
            ..Input::default()
        });
        assert!(!g.sleep_prompt);
        assert_eq!((g.day, g.energy, g.seeds[0]), (1, 16, 4));
        sleep(&mut g);
        assert_eq!((g.day, g.energy), (2, 18));
    }
    #[test]
    fn rain_waters_new_seeds_and_only_nights_advance_growth() {
        let mut g = Game::new();
        sleep(&mut g);
        assert_eq!(g.weather(), Weather::Rain);
        primary(&mut g);
        assert!(g.plots[0].plant.unwrap().watered);
        assert_eq!(g.energy, 16);
        for _ in 0..600 {
            g.tick(&Input::default());
        }
        assert_eq!(g.plots[0].plant.unwrap().age, 0);
        sleep(&mut g);
        assert_eq!(g.plots[0].plant.unwrap().age, 1);
    }
    #[test]
    fn heat_and_dry_nights_wilt_a_crop_without_giving_produce() {
        let mut g = Game::new();
        g.day = 4;
        g.delivered[0] = true;
        primary(&mut g);
        sleep(&mut g);
        assert_eq!(g.plots[0].plant.unwrap().stress, 2);
        sleep(&mut g);
        assert!(g.plots[0].plant.unwrap().wilted());
        assert_eq!((g.produce(0), g.wilted, g.score), (0, 1, 0));
        let energy = g.energy;
        primary(&mut g);
        assert!(g.plots[0].plant.is_none());
        assert_eq!(g.energy, energy - 1);
    }
    #[test]
    fn frost_needs_frames_but_does_not_punish_watered_plants() {
        for frames in [false, true] {
            let mut g = Game::new();
            g.day = 10;
            g.delivered = [true, true, false];
            g.tools[2] = frames;
            primary(&mut g);
            primary(&mut g);
            sleep(&mut g);
            let plant = g.plots[0].plant.unwrap();
            assert_eq!(plant.age, u8::from(frames));
            assert_eq!(plant.stress, 0);
        }
    }
    #[test]
    fn harvesting_is_once_only_and_exhausted_soil_requires_compost() {
        let mut g = Game::new();
        g.plots[0] = Plot {
            soil: 0,
            plant: Some(Plant {
                crop: 0,
                age: 2,
                nights: 2,
                watered: true,
                stress: 0,
                quality: 2,
            }),
        };
        assert_eq!(primary(&mut g), Cue::Good);
        assert_eq!((g.produce(0), g.harvested, g.score), (1, 1, 30));
        primary(&mut g);
        assert_eq!(g.plots[0].plant.unwrap().nights, 3);
        assert_eq!(g.produce(0), 1);
        g.plots[0].plant = None;
        g.page = Page::Tools;
        let coins = g.coins;
        let energy = g.energy;
        assert_eq!(
            g.tick(&Input {
                secondary: true,
                ..Input::default()
            }),
            Cue::Good
        );
        assert_eq!(
            (g.plots[0].soil, g.coins, g.energy),
            (2, coins - 2, energy - 1)
        );
        assert_eq!(
            g.tick(&Input {
                secondary: true,
                ..Input::default()
            }),
            Cue::None
        );
    }
    #[test]
    fn orders_preflight_every_ingredient_and_cannot_be_claimed_twice() {
        let mut g = Game::new();
        g.page = Page::Orders;
        put(&mut g, 0, 3);
        let coins = g.coins;
        assert_eq!(primary(&mut g), Cue::None);
        assert_eq!((g.produce(0), g.coins), (3, coins));
        put(&mut g, 0, 1);
        assert_eq!(primary(&mut g), Cue::Good);
        assert_eq!((g.produce(0), g.coins, g.score), (0, coins + 45, 400));
        put(&mut g, 0, 2);
        put(&mut g, 1, 3);
        assert_eq!(primary(&mut g), Cue::None);
        assert_eq!((g.produce(0), g.produce(1)), (2, 3));
        g.day = 6;
        assert_eq!(primary(&mut g), Cue::Good);
        assert_eq!(g.orders_completed(), 2);
        assert_eq!(g.score, 800);
    }
    #[test]
    fn seed_stall_has_finite_stock_and_does_not_sell_seeds_as_produce() {
        let mut g = Game::new();
        g.page = Page::Stall;
        assert_eq!(
            g.tick(&Input {
                secondary: true,
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(g.coins, 35);
        for _ in 0..4 {
            assert_eq!(primary(&mut g), Cue::Action);
        }
        assert_eq!((g.seeds[0], g.stock[0], g.coins), (9, 0, 19));
        assert_eq!(primary(&mut g), Cue::None);
        put(&mut g, 0, 1);
        g.tick(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(g.coins, 29);
        assert_eq!(g.produce(0), 0);
    }
    #[test]
    fn upgrades_are_one_time_and_brass_can_respects_grid_edges() {
        let mut g = Game::new();
        g.coins = 200;
        g.page = Page::Tools;
        primary(&mut g);
        let coins = g.coins;
        assert_eq!(primary(&mut g), Cue::None);
        assert_eq!(g.coins, coins);
        g.row = 1;
        let energy = g.energy;
        primary(&mut g);
        assert_eq!(g.energy, energy + 6);
        assert_eq!(g.max_energy(), 24);
        g.page = Page::Field;
        g.cursor = 3;
        for i in [3, 4, 7] {
            g.plots[i].plant = Some(Plant {
                crop: 0,
                age: 0,
                nights: 2,
                watered: false,
                stress: 0,
                quality: 2,
            });
        }
        primary(&mut g);
        assert!(g.plots[3].plant.unwrap().watered);
        assert!(g.plots[7].plant.unwrap().watered);
        assert!(!g.plots[4].plant.unwrap().watered);
    }
    #[test]
    fn missed_deadline_and_unfunded_oven_are_real_failure_paths() {
        let mut g = Game::new();
        for _ in 0..5 {
            sleep(&mut g);
        }
        assert_eq!(g.result, Outcome::Lost);
        assert_eq!(g.day, 5);
        let mut g = Game::new();
        g.day = 14;
        g.delivered = [true; 3];
        g.coins = 79;
        sleep(&mut g);
        assert_eq!(g.result, Outcome::Lost);
        let mut g = Game::new();
        g.day = 14;
        g.delivered = [true; 3];
        g.coins = 80;
        sleep(&mut g);
        assert_eq!(g.result, Outcome::Won);
    }
    #[test]
    fn terminal_state_ignores_all_commands_and_new_campaign_is_clean() {
        let mut g = Game::new();
        g.day = 14;
        g.delivered = [true; 3];
        g.coins = 80;
        sleep(&mut g);
        let before = format!("{g:?}");
        for _ in 0..50 {
            g.tick(&Input {
                primary: true,
                secondary: true,
                tertiary: true,
                click: true,
                slot: Some(2),
                right_pressed: true,
                ..Input::default()
            });
        }
        assert_eq!(format!("{g:?}"), before);
        let fresh = Game::new();
        assert_eq!(
            (
                fresh.day,
                fresh.coins,
                fresh.energy,
                fresh.orders_completed()
            ),
            (1, 35, 18, 0)
        );
    }
    #[test]
    fn held_direction_and_invalid_pointer_do_not_repeat_actions() {
        let mut g = Game::new();
        for _ in 0..100 {
            g.tick(&Input {
                right: true,
                primary_down: true,
                ..Input::default()
            });
        }
        assert_eq!((g.cursor, g.energy, g.seeds[0]), (0, 18, 5));
        g.tick(&Input {
            click: true,
            pointer: Vec2::new(f32::NAN, 0.),
            ..Input::default()
        });
        assert_eq!(g.cursor, 0);
        g.tick(&Input {
            right_pressed: true,
            ..Input::default()
        });
        assert_eq!(g.cursor, 1);
    }
    fn click(g: &mut Game, area: Area) -> Cue {
        g.tick(&Input {
            click: true,
            pointer: Vec2::new(area[0] + area[2] * 0.5, area[1] + area[3] * 0.5),
            ..Input::default()
        })
    }
    fn plant(crop: usize, age: u8, nights: u8, watered: bool, stress: u8) -> Option<Plant> {
        Some(Plant {
            crop,
            age,
            nights,
            watered,
            stress,
            quality: 2,
        })
    }
    #[test]
    fn a_second_click_on_the_selected_plot_works_it_like_space() {
        let mut g = Game::new();
        // Plot 0 starts selected: one click plants exactly as Space would.
        assert_eq!(click(&mut g, plot_area(0)), Cue::Action);
        assert_eq!((g.seeds[0], g.energy), (4, 16));
        assert_eq!(g.events(), &[Event::Plant { plot: 0 }]);
        // Another plot: the first click only selects it.
        assert_eq!(click(&mut g, plot_area(5)), Cue::Action);
        assert_eq!((g.cursor, g.seeds[0], g.energy), (5, 4, 16));
        assert!(g.events().is_empty());
        click(&mut g, plot_area(5));
        assert_eq!((g.seeds[0], g.energy), (3, 14));
        // A third click waters through the same rule path, and a fourth is refused.
        click(&mut g, plot_area(5));
        assert!(g.plots[5].plant.unwrap().watered);
        assert_eq!(click(&mut g, plot_area(5)), Cue::None);
        assert_eq!(
            g.events(),
            &[Event::Refuse(Refusal::Watered, Spot::Plot(5))]
        );
        assert_eq!(g.energy, 13);
        // Seed packets and tabs still just select.
        click(&mut g, seed_area(2));
        assert_eq!(g.crop, 2);
        click(&mut g, tab_area(1));
        assert_eq!(g.page, Page::Stall);
    }
    #[test]
    fn row_buttons_buy_sell_compost_deliver_and_sleep_through_key_rules() {
        let mut g = Game::new();
        g.page = Page::Stall;
        // Buy a bean seed from row 1 without first selecting the row.
        assert_eq!(click(&mut g, button_area(1, 0)), Cue::Action);
        assert_eq!((g.row, g.seeds[1], g.stock[1], g.coins), (1, 4, 2, 29));
        // Selling needs harvested produce, exactly like X.
        assert_eq!(click(&mut g, button_area(0, 1)), Cue::None);
        assert_eq!(
            g.events(),
            &[Event::Refuse(Refusal::NoProduce, Spot::Sell(0))]
        );
        put(&mut g, 0, 2);
        assert_eq!(click(&mut g, button_area(0, 1)), Cue::Good);
        assert_eq!((g.row, g.produce(0), g.coins), (0, 1, 39));
        // The workshop buys a tool from its button and composts the selected plot.
        g.page = Page::Tools;
        g.coins = 200;
        assert_eq!(click(&mut g, button_area(2, 1)), Cue::Good);
        assert!(g.tools[2]);
        assert_eq!(click(&mut g, button_area(2, 1)), Cue::None);
        assert_eq!(g.events(), &[Event::Refuse(Refusal::Owned, Spot::Tool(2))]);
        g.plots[3].soil = 0;
        g.cursor = 3;
        let (coins, energy) = (g.coins, g.energy);
        assert_eq!(click(&mut g, COMPOST), Cue::Good);
        assert_eq!(
            (g.plots[3].soil, g.coins, g.energy),
            (2, coins - 2, energy - 1)
        );
        assert_eq!(click(&mut g, COMPOST), Cue::None);
        // The village delivers the current order only through its own button.
        g.page = Page::Orders;
        put(&mut g, 0, 3);
        assert_eq!(click(&mut g, button_area(1, 1)), Cue::Action);
        assert!(!g.delivered[0]);
        assert_eq!(click(&mut g, button_area(0, 1)), Cue::Good);
        assert!(g.delivered[0]);
        // The ledger's moon opens the same confirmation card as C.
        assert_eq!(click(&mut g, SLEEP), Cue::Action);
        assert!(g.sleep_prompt);
        assert_eq!(g.day, 1);
        assert_eq!(click(&mut g, CONFIRM), Cue::Good);
        assert_eq!(g.day, 2);
    }
    #[test]
    fn night_preview_matches_the_sleep_that_follows() {
        for (day, frames) in [(3, false), (4, false), (10, false), (10, true)] {
            let mut g = Game::new();
            g.day = day;
            g.delivered = [day > 5, day > 9, false];
            g.tools[2] = frames;
            g.plots[0].plant = plant(0, 0, 2, true, 0);
            g.plots[1].plant = plant(1, 2, 3, true, 0);
            g.plots[2].plant = plant(2, 1, 4, false, 0);
            g.plots[3].plant = plant(0, 0, 2, false, 2);
            g.plots[4].plant = plant(0, 2, 2, false, 0);
            g.plots[5].plant = plant(1, 0, 3, false, 3);
            let night = g.night();
            sleep(&mut g);
            let count = |f: fn(&Event) -> bool| g.events().iter().filter(|e| f(e)).count() as u8;
            assert_eq!(night.grow, count(|e| matches!(e, Event::Grow { .. })));
            assert_eq!(
                night.ripen,
                count(|e| matches!(e, Event::Grow { ripe: true, .. }))
            );
            assert_eq!(night.frozen, count(|e| matches!(e, Event::Frozen { .. })));
            assert_eq!(night.wilt, count(|e| matches!(e, Event::Wilt { .. })));
            assert_eq!(
                night.dry,
                count(|e| matches!(e, Event::Thirst { .. } | Event::Wilt { .. }))
            );
            assert_eq!(g.events().last(), Some(&Event::Dawn));
        }
    }
    #[test]
    fn events_are_bounded_cleared_each_tick_and_never_steer_rules() {
        let mut g = Game::new();
        for i in 0..PLOTS {
            g.plots[i].plant = plant(i % 3, 0, 3, i % 2 == 0, 0);
        }
        sleep(&mut g);
        assert_eq!(g.events().len(), PLOTS + 1);
        assert!(g.events().len() <= EVENT_CAP);
        g.tick(&Input::default());
        assert!(g.events().is_empty());
        // Stale presentation events cannot change what a command does.
        let mut a = Game::new();
        let mut b = Game::new();
        for _ in 0..EVENT_CAP + 4 {
            b.events.push(Event::Sell { crop: 2 });
        }
        for input in [
            Input {
                primary: true,
                ..Input::default()
            },
            Input {
                right_pressed: true,
                ..Input::default()
            },
            Input {
                primary: true,
                ..Input::default()
            },
        ] {
            assert_eq!(a.tick(&input), b.tick(&input));
            assert_eq!(a.events(), b.events());
        }
        assert_eq!(
            (a.plots, a.energy, a.coins, a.seeds),
            (b.plots, b.energy, b.coins, b.seeds)
        );
    }
}
