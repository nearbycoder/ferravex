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
use crate::content::{CROPS, MORNINGS, ORDERS, TOOL_NAMES, TOOL_PRICES, Weather};
use ferravex_arcade::{Cue, Input, Outcome};
use ferravex_math::Vec2;
use ferravex_play::gameplay::inventory::{Inventory, Stack};

pub(crate) const COLS: usize = 4;
pub(crate) const PLOTS: usize = 12;
pub(crate) const GRID_X: f32 = 44.;
pub(crate) const GRID_Y: f32 = 242.;
pub(crate) const PITCH_X: f32 = 136.;
pub(crate) const PITCH_Y: f32 = 80.;
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
    pub(crate) notice: String,
    pub(crate) result: Outcome,
    pub(crate) score: u32,
    pub(crate) harvested: u32,
    pub(crate) wilted: u32,
    pub(crate) last_growth: u8,
    pub(crate) last_ready: u8,
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
            notice: "Space plants a turnip; Space again waters it. Plant four for Soup Night."
                .into(),
            result: Outcome::Playing,
            score: 0,
            harvested: 0,
            wilted: 0,
            last_growth: 0,
            last_ready: 0,
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
    fn reject(&mut self, message: impl Into<String>) -> Cue {
        self.notice = message.into();
        Cue::None
    }
    fn field_action(&mut self) -> Cue {
        let index = self.cursor;
        let plot = self.plots[index];
        match plot.plant {
            None => {
                if self.seeds[self.crop] == 0 {
                    return self.reject("No seeds of that kind. Visit the seed stall [2].");
                }
                if self.energy < 2 {
                    return self.reject("Planting needs two energy. C opens the sleep card.");
                }
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
                self.notice = format!(
                    "Planted {}. Water it before bed; {} growing nights.",
                    CROPS[self.crop].name,
                    CROPS[self.crop].nights + u8::from(plot.soil == 0)
                );
                Cue::Action
            }
            Some(plant) if plant.wilted() => {
                if self.energy == 0 {
                    return self.reject("Clearing a wilted crop needs one energy.");
                }
                self.energy -= 1;
                self.plots[index].plant = None;
                self.notice = "Wilted crop cleared. The seed is gone; try a fresh planting.".into();
                Cue::Action
            }
            Some(plant) if plant.ready() => {
                if self.energy == 0 {
                    return self
                        .reject("Harvesting needs one energy. Ready crops can wait safely.");
                }
                if self
                    .produce
                    .add(Stack {
                        item: plant.crop as u32,
                        amount: 1,
                        limit: 99,
                    })
                    .is_err()
                {
                    return self.reject("The produce shelf is full. Sell or deliver crops first.");
                }
                self.energy -= 1;
                self.plots[index].plant = None;
                self.plots[index].soil = self.plots[index].soil.saturating_sub(1);
                self.harvested += 1;
                self.score += 20 + u32::from(plant.quality) * 5;
                self.notice = format!(
                    "Harvested {}. Saved in your basket; village orders use stored produce.",
                    CROPS[plant.crop].name
                );
                Cue::Good
            }
            Some(plant) => {
                if plant.watered {
                    return self.reject("Already watered. Growth happens when you sleep.");
                }
                if self.energy == 0 {
                    return self.reject("Watering needs one energy. Rain tomorrow may help.");
                }
                self.energy -= 1;
                let indices = [
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
                ];
                let mut count = 0;
                for i in indices.into_iter().flatten() {
                    if let Some(p) = &mut self.plots[i].plant
                        && !p.watered
                        && !p.wilted()
                        && !p.ready()
                    {
                        p.watered = true;
                        count += 1;
                    }
                }
                self.notice = format!("Watered {count} plot(s). Wet soil grows tonight.");
                Cue::Action
            }
        }
    }
    fn compost(&mut self) -> Cue {
        let plot = &mut self.plots[self.cursor];
        if plot.plant.is_some() {
            return self.reject("Compost only an empty plot. Harvest or clear it first.");
        }
        if plot.soil == 2 {
            return self.reject("This soil is already rich.");
        }
        if self.energy == 0 || self.coins < 2 {
            return self.reject("Compost costs two coins and one energy.");
        }
        plot.soil = 2;
        self.coins -= 2;
        self.energy -= 1;
        self.notice = "Fresh compost restores two harvests of rich soil.".into();
        Cue::Good
    }
    fn buy_seed(&mut self) -> Cue {
        let crop = self.row.min(2);
        if self.coins < CROPS[crop].seed {
            return self.reject("Not enough coins. Sell surplus produce or complete an order.");
        }
        if self.stock[crop] == 0 {
            return self.reject("That seed is sold out today. Stock returns each morning.");
        }
        if self.seeds[crop] >= 24 {
            return self.reject("Your seed tin holds at most 24 of each kind.");
        }
        self.coins -= CROPS[crop].seed;
        self.stock[crop] -= 1;
        self.seeds[crop] += 1;
        self.notice = format!(
            "Bought one {} seed for {} coins.",
            CROPS[crop].name, CROPS[crop].seed
        );
        Cue::Action
    }
    fn sell(&mut self) -> Cue {
        let crop = self.row.min(2);
        if self.produce.remove(crop as u32, 1).is_err() {
            return self.reject("No harvested crop to sell. Seeds are not produce.");
        }
        self.coins += CROPS[crop].sale;
        self.notice = format!(
            "Sold one {} for {} coins. Keep ingredients needed by the village!",
            CROPS[crop].name, CROPS[crop].sale
        );
        Cue::Good
    }
    fn deliver(&mut self) -> Cue {
        let Some(index) = self.order() else {
            return self.reject("Every village order is complete. Save 80 coins for the oven.");
        };
        let order = &ORDERS[index];
        if self.day < order.opens {
            return self.reject(format!(
                "{} opens on day {}. Store its crops until then.",
                order.name, order.opens
            ));
        }
        if (0..3).any(|c| self.produce(c) < order.needs[c]) {
            return self.reject("The basket is short. Only harvested crops count for an order.");
        }
        for (crop, &amount) in order.needs.iter().enumerate() {
            if amount > 0 {
                self.produce
                    .remove(crop as u32, amount)
                    .expect("preflighted produce");
            }
        }
        self.delivered[index] = true;
        self.coins += order.reward;
        self.score += 400;
        self.notice = format!(
            "{} delivered! +{} coins. The village lights another lantern.",
            order.name, order.reward
        );
        Cue::Good
    }
    fn buy_tool(&mut self) -> Cue {
        let index = self.row.min(2);
        if self.tools[index] {
            return self.reject("That tool is already yours.");
        }
        if self.coins < TOOL_PRICES[index] {
            return self.reject("Not enough coins for this tool. Village orders pay well.");
        }
        self.coins -= TOOL_PRICES[index];
        self.tools[index] = true;
        if index == 1 {
            self.energy += 6;
        }
        self.notice = format!(
            "{} is yours. Its improvement is permanent for this season.",
            TOOL_NAMES[index]
        );
        Cue::Good
    }
    fn sleep(&mut self) -> Cue {
        self.sleep_prompt = false;
        if let Some(i) = self.order()
            && self.day >= ORDERS[i].deadline
        {
            self.result = Outcome::Lost;
            self.rank = "An Empty Table";
            self.notice = format!(
                "{} could not be served by day {}. A new season is a fresh start.",
                ORDERS[i].name, ORDERS[i].deadline
            );
            return Cue::Bad;
        }
        if self.day == 14 {
            if self.coins < 80 {
                self.result = Outcome::Lost;
                self.rank = "A Cold Oven";
                self.notice="The feast is ready, but the village oven needs 80 coins. Sell more surplus next season.".into();
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
            self.notice =
                "The oven glows. Three tables become one, and the village finally feels like home."
                    .into();
            return Cue::Win;
        }
        let weather = self.weather();
        self.last_growth = 0;
        self.last_ready = 0;
        for plot in &mut self.plots {
            if let Some(plant) = &mut plot.plant {
                if plant.ready() || plant.wilted() {
                    continue;
                }
                if plant.watered {
                    plant.stress = 0;
                    if weather != Weather::Frost || self.tools[2] {
                        plant.age += 1;
                        self.last_growth += 1;
                        if plant.ready() {
                            self.last_ready += 1;
                        }
                    }
                } else {
                    plant.stress += if weather == Weather::Heat { 2 } else { 1 };
                    if plant.wilted() {
                        self.wilted += 1;
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
        self.notice = format!(
            "Day {}: {}. {} crops grew; {} are newly ready.",
            self.day,
            self.weather().name(),
            self.last_growth,
            self.last_ready
        );
        Cue::Good
    }
    fn select_pointer(&mut self, point: Vec2) -> bool {
        if !point.is_finite() {
            return false;
        }
        if (116. ..150.).contains(&point.y) && (22. ..622.).contains(&point.x) {
            let slot = ((point.x - 22.) / 120.) as usize + 1;
            if let Some(page) = Page::from_slot(slot) {
                self.page = page;
                self.row = 0;
                return true;
            }
        }
        if self.page == Page::Field {
            for i in 0..PLOTS {
                let x = GRID_X + (i % COLS) as f32 * PITCH_X;
                let y = GRID_Y + (i / COLS) as f32 * PITCH_Y;
                if point.x >= x && point.x < x + 124. && point.y >= y && point.y < y + 70. {
                    self.cursor = i;
                    return true;
                }
            }
            if (494. ..524.).contains(&point.y) && (44. ..602.).contains(&point.x) {
                self.crop = (((point.x - 44.) / 186.) as usize).min(2);
                return true;
            }
        } else if (44. ..602.).contains(&point.x) && (205. ..445.).contains(&point.y) {
            self.row = (((point.y - 205.) / 80.) as usize).min(2);
            return true;
        }
        false
    }
    /// Execute one fixed update. Edges choose actions; held directions never repeat.
    /// Invalid actions may update explanatory copy but preserve money, crops and time.
    pub fn tick(&mut self, input: &Input) -> Cue {
        if self.result != Outcome::Playing {
            return Cue::None;
        }
        if self.sleep_prompt {
            if input.primary {
                return self.sleep();
            }
            if input.secondary || input.tertiary {
                self.sleep_prompt = false;
                self.notice = "Back to the farm. Nothing changed while you planned.".into();
                return Cue::Action;
            }
            if input.click {
                if input.pointer.x >= 128.
                    && input.pointer.x < 430.
                    && input.pointer.y >= 452.
                    && input.pointer.y < 496.
                {
                    return self.sleep();
                }
                if input.pointer.x >= 450.
                    && input.pointer.x < 750.
                    && input.pointer.y >= 452.
                    && input.pointer.y < 496.
                {
                    self.sleep_prompt = false;
                    return Cue::Action;
                }
            }
            return Cue::None;
        }
        if input.tertiary {
            self.sleep_prompt = true;
            return Cue::Action;
        }
        if let Some(page) = input.slot.and_then(Page::from_slot) {
            self.page = page;
            self.row = 0;
            return Cue::Action;
        }
        if input.click
            && input.pointer.x >= 658.
            && input.pointer.x < 922.
            && input.pointer.y >= 518.
            && input.pointer.y < 552.
        {
            return self.tick(&Input {
                primary: true,
                ..Input::default()
            });
        }
        if input.click && self.select_pointer(input.pointer) {
            return Cue::Action;
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
                    self.notice = format!(
                        "Selected {} seeds. Space works on the selected plot.",
                        CROPS[self.crop].name
                    );
                    Cue::Action
                }
                Page::Stall => self.sell(),
                Page::Tools => self.compost(),
                Page::Orders => self.reject("Keep crops in your basket until the order opens."),
                Page::Almanac => {
                    self.reject("Read today's letter and tomorrow's forecast; C opens sleep.")
                }
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
                    pointer: Vec2::new(
                        GRID_X + (index % COLS) as f32 * PITCH_X + 62.,
                        GRID_Y + (index / COLS) as f32 * PITCH_Y + 35.,
                    ),
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
            "day {}, coins {}, energy {}, notice {}, basket {:?}, seeds {:?}",
            g.day,
            g.coins,
            g.energy,
            g.notice,
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
}
