//! Renderer-independent restaurant rules at exactly sixty updates per second.
//!
//! ```
//! use ferravex_arcade::{Input, Outcome};
//! use pantry_panic::Game;
//! let mut kitchen = Game::new();
//! assert_eq!(kitchen.shift(), 1);
//! kitchen.step(&Input { primary: true, ..Input::default() });
//! for tick in 0..600 {
//!     let command = kitchen.pilot(tick);
//!     kitchen.step(&command);
//! }
//! assert_eq!(kitchen.outcome(), Outcome::Playing);
//! ```
use crate::content::*;
use ferravex_arcade::{Cue, Input, Outcome, Vec2};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Briefing,
    Service,
    Debrief,
    Setback,
    Won,
    Lost,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Carry {
    Empty,
    Raw(usize),
    Chopped(usize),
    Mixed(usize),
    Cooked(usize),
    Plated(usize),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pot {
    pub recipe: usize,
    pub ticks: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Order {
    pub recipe: usize,
    pub patience: u32,
    pub number: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ledger {
    cash: u32,
    score: u32,
    served: u32,
}
/// A bounded seven-shift campaign. Mutations enter through [`Game::step`].
/// No renderer, storage, wall clock, random source or autoplay privilege is used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub(crate) phase: Phase,
    pub(crate) shift_index: usize,
    pub(crate) position: Cell,
    pub(crate) previous: Cell,
    pub(crate) move_wait: u32,
    pub(crate) destination: Option<Cell>,
    pub(crate) carry: Carry,
    pub(crate) chopped_ticks: u32,
    pub(crate) mixing: u8,
    pub(crate) burners: [Option<Pot>; 2],
    pub(crate) orders: [Option<Order>; 3],
    pub(crate) next_order: usize,
    pub(crate) ticks: u32,
    pub(crate) served: u8,
    pub(crate) missed: u8,
    pub(crate) waste: u8,
    pub(crate) cash: u32,
    pub(crate) score: u32,
    pub(crate) total_served: u32,
    pub(crate) upgrades: [u8; 3],
    pub(crate) medals: [u8; 7],
    pub(crate) retries: u8,
    pub(crate) book: usize,
    pub(crate) note: &'static str,
    saved: Ledger,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Begin a new restaurant career with 48 coins, empty stations and three retries.
    pub fn new() -> Self {
        Self {
            phase: Phase::Briefing,
            shift_index: 0,
            position: Cell::new(5, 3),
            previous: Cell::new(5, 3),
            move_wait: 0,
            destination: None,
            carry: Carry::Empty,
            chopped_ticks: 0,
            mixing: 0,
            burners: [None; 2],
            orders: [None; 3],
            next_order: 0,
            ticks: 0,
            served: 0,
            missed: 0,
            waste: 0,
            cash: 48,
            score: 0,
            total_served: 0,
            upgrades: [0; 3],
            medals: [0; 7],
            retries: 3,
            book: 0,
            note: "Fresh produce costs 2 coins. A complete dish pays much more.",
            saved: Ledger {
                cash: 48,
                score: 0,
                served: 0,
            },
        }
    }
    /// One-based current service number, from one through seven.
    pub fn shift(&self) -> usize {
        self.shift_index + 1
    }
    /// Accepted score; retry rolls back the failed service's provisional points.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// Dishes actually served, excluding rolled-back failed shifts.
    pub fn served_total(&self) -> u32 {
        self.total_served
    }
    /// A setback remains playable while a retry is available.
    pub fn outcome(&self) -> Outcome {
        match self.phase {
            Phase::Won => Outcome::Won,
            Phase::Lost => Outcome::Lost,
            _ => Outcome::Playing,
        }
    }
    pub(crate) fn content(&self) -> &'static Shift {
        &SHIFTS[self.shift_index]
    }
    pub(crate) fn walk_ticks(&self) -> u32 {
        [10, 8, 6][self.upgrades[1] as usize]
    }
    pub(crate) fn chop_ticks(&self) -> u32 {
        [90, 60, 42][self.upgrades[0] as usize]
    }
    pub(crate) fn burn_grace(&self) -> u32 {
        300 + u32::from(self.upgrades[2]) * 180
    }
    pub(crate) fn deadline(&self) -> u32 {
        (self.content().menu.len() as u32 - 1) * self.content().interval
            + self.content().patience
            + 600
    }
    pub(crate) fn station(&self) -> Option<Station> {
        STATIONS.iter().find(|s| s.2 == self.position).map(|s| s.0)
    }
    pub(crate) fn pot_ready(&self, pot: Pot) -> bool {
        pot.ticks >= RECIPES[pot.recipe].cook && !self.pot_burnt(pot)
    }
    pub(crate) fn pot_burnt(&self, pot: Pot) -> bool {
        pot.ticks >= RECIPES[pot.recipe].cook + self.burn_grace()
    }
    fn snapshot(&self) -> Ledger {
        Ledger {
            cash: self.cash,
            score: self.score,
            served: self.total_served,
        }
    }
    fn reset_kitchen(&mut self) {
        self.position = Cell::new(5, 3);
        self.previous = self.position;
        self.move_wait = 0;
        self.destination = None;
        self.carry = Carry::Empty;
        self.chopped_ticks = 0;
        self.mixing = 0;
        self.burners = [None; 2];
        self.orders = [None; 3];
        self.next_order = 0;
        self.ticks = 0;
        self.served = 0;
        self.missed = 0;
        self.waste = 0;
        self.book = self.content().menu[0];
    }
    fn launch(&mut self) -> Cue {
        self.saved = self.snapshot();
        self.reset_kitchen();
        self.phase = Phase::Service;
        self.note = "Collect, chop, mix, cook if needed, plate, then serve.";
        self.spawn_order();
        Cue::Action
    }
    fn spawn_order(&mut self) {
        if self.next_order >= self.content().menu.len()
            || self.ticks < self.next_order as u32 * self.content().interval
        {
            return;
        }
        if let Some(slot) = self.orders.iter().position(Option::is_none) {
            self.orders[slot] = Some(Order {
                recipe: self.content().menu[self.next_order],
                patience: self.content().patience,
                number: self.next_order + 1,
            });
            self.next_order += 1;
        }
    }
    fn fail(&mut self, note: &'static str) -> Cue {
        self.note = note;
        self.phase = if self.retries > 0 {
            Phase::Setback
        } else {
            Phase::Lost
        };
        Cue::Bad
    }
    fn complete_shift(&mut self) -> Cue {
        let all = usize::from(self.served) == self.content().menu.len();
        self.medals[self.shift_index] = if all && self.waste == 0 {
            3
        } else if all {
            2
        } else {
            1
        };
        self.score += 1000 + u32::from(self.medals[self.shift_index]) * 250;
        self.cash += 20;
        self.note = "Service complete! The neighborhood leaves the lights on.";
        self.phase = if self.shift_index == 6 {
            Phase::Won
        } else {
            Phase::Debrief
        };
        if self.phase == Phase::Won {
            Cue::Win
        } else {
            Cue::Good
        }
    }
    fn buy(&mut self, kind: usize) -> Cue {
        if kind >= 3 || self.upgrades[kind] >= 2 {
            return Cue::None;
        }
        let cost = upgrade_cost(kind, self.upgrades[kind]);
        if self.cash < cost {
            self.note = "Keep serving to earn this upgrade.";
            return Cue::None;
        }
        self.cash -= cost;
        self.upgrades[kind] += 1;
        self.note = "Upgrade installed for the next service.";
        Cue::Good
    }
    /// Execute one 1/60-second update. Edges perform at most one interaction.
    /// Held primary chops only at the board; menus and terminal outcomes do not age food.
    pub fn step(&mut self, input: &Input) -> Cue {
        if self.outcome() != Outcome::Playing {
            return Cue::None;
        }
        match self.phase {
            Phase::Briefing => {
                if input.primary || (input.click && menu_start(input.pointer)) {
                    return self.launch();
                }
                return Cue::None;
            }
            Phase::Setback => {
                if input.primary || (input.click && menu_start(input.pointer)) {
                    self.cash = self.saved.cash;
                    self.score = self.saved.score;
                    self.total_served = self.saved.served;
                    self.retries -= 1;
                    return self.launch();
                }
                return Cue::None;
            }
            Phase::Debrief => {
                if let Some(choice) = input.slot.filter(|n| *n >= 1 && *n <= 3) {
                    return self.buy(choice - 1);
                }
                if input.click {
                    for i in 0..3 {
                        if input.pointer.x >= 86. + i as f32 * 270.
                            && input.pointer.x < 338. + i as f32 * 270.
                            && input.pointer.y >= 346.
                            && input.pointer.y <= 442.
                        {
                            return self.buy(i);
                        }
                    }
                }
                if input.primary || (input.click && menu_start(input.pointer)) {
                    self.shift_index += 1;
                    self.phase = Phase::Briefing;
                    self.reset_kitchen();
                    self.note = "Read the menu, then open the doors.";
                    return Cue::Action;
                }
                return Cue::None;
            }
            Phase::Service => {}
            Phase::Won | Phase::Lost => return Cue::None,
        }
        self.ticks += 1;
        let mut cue = Cue::None;
        for pot in self.burners.iter_mut().flatten() {
            pot.ticks = pot
                .ticks
                .saturating_add(1)
                .min(RECIPES[pot.recipe].cook + 1000);
        }
        for i in 0..2 {
            if self.burners[i].is_some_and(|p| p.ticks == RECIPES[p.recipe].cook) {
                cue = Cue::Good;
                self.note = "Ding! A pot is ready. Pick it up before it burns.";
            }
            if self.burners[i]
                .is_some_and(|p| p.ticks == RECIPES[p.recipe].cook + self.burn_grace())
            {
                self.waste = self.waste.saturating_add(1);
                cue = Cue::Bad;
                self.note = "A pot burned. Stand by it and press X to clear it.";
            }
        }
        for order in &mut self.orders {
            if let Some(o) = order {
                o.patience = o.patience.saturating_sub(1);
                if o.patience == 0 {
                    *order = None;
                    self.missed += 1;
                    cue = Cue::Bad;
                    self.note = "A guest left hungry. The quota is still on the board.";
                }
            }
        }
        self.spawn_order();
        let remaining = self.content().menu.len() - usize::from(self.missed);
        if remaining < usize::from(self.content().quota) {
            return self.fail("Too many guests left. Try the service again.");
        }
        if self.next_order == self.content().menu.len() && self.orders.iter().all(Option::is_none) {
            return self.complete_shift();
        }
        if self.ticks >= self.deadline() {
            return self.fail("The kitchen closed before the quota was met.");
        }
        if input.tertiary {
            self.book = (self.book + 1) % RECIPES.len();
            self.note = RECIPES[self.book].note;
        }
        if input.click {
            if input.pointer.x >= 654. && input.pointer.y >= 124. && input.pointer.y < 180. {
                self.book = (self.book + 1) % RECIPES.len();
            }
            for (i, order) in self.orders.iter().enumerate() {
                if input.pointer.x >= 28. + i as f32 * 204.
                    && input.pointer.x < 224. + i as f32 * 204.
                    && input.pointer.y >= 124.
                    && input.pointer.y < 200.
                    && let Some(order) = order
                {
                    self.book = order.recipe;
                }
            }
            let cell = Cell::new(
                ((input.pointer.x - GRID_X) / TILE).floor() as i32,
                ((input.pointer.y - GRID_Y) / TILE).floor() as i32,
            );
            if walkable(cell) {
                self.destination = Some(cell);
            } else if let Some(station) = STATIONS.iter().find(|s| s.1 == cell) {
                self.destination = Some(station.2);
            }
        }
        self.move_wait = self.move_wait.saturating_sub(1);
        let manual = input.left
            || input.right
            || input.up
            || input.down
            || input.left_pressed
            || input.right_pressed
            || input.up_pressed
            || input.down_pressed;
        if manual {
            self.destination = None;
        }
        if self.move_wait == 0 {
            let delta = if input.left || input.left_pressed {
                (-1, 0)
            } else if input.right || input.right_pressed {
                (1, 0)
            } else if input.up || input.up_pressed {
                (0, -1)
            } else if input.down || input.down_pressed {
                (0, 1)
            } else if let Some(target) = self.destination {
                (
                    (target.x - self.position.x).signum(),
                    if target.x == self.position.x {
                        (target.y - self.position.y).signum()
                    } else {
                        0
                    },
                )
            } else {
                (0, 0)
            };
            let next = Cell::new(self.position.x + delta.0, self.position.y + delta.1);
            if delta != (0, 0) && walkable(next) {
                self.previous = self.position;
                self.position = next;
                self.move_wait = self.walk_ticks();
                self.chopped_ticks = 0;
            }
            if self.destination == Some(self.position) {
                self.destination = None;
            }
        }
        if self.move_wait > 0 {
            return cue;
        }
        if input.secondary {
            let accepted = self.discard();
            if accepted != Cue::None {
                return accepted;
            }
        }
        if self.station() == Some(Station::Chop)
            && (input.primary_down || input.primary)
            && let Carry::Raw(i) = self.carry
        {
            self.chopped_ticks += 1;
            if self.chopped_ticks >= self.chop_ticks() {
                self.carry = Carry::Chopped(i);
                self.chopped_ticks = 0;
                self.note = "Prep complete. Take the ingredient to Mix.";
                return Cue::Good;
            }
            return cue;
        }
        if input.primary {
            let accepted = self.interact();
            if accepted != Cue::None {
                cue = accepted;
            }
        }
        if self.next_order == self.content().menu.len() && self.orders.iter().all(Option::is_none) {
            return if self.served >= self.content().quota {
                self.complete_shift()
            } else {
                self.fail("The last guests left before you reached quota.")
            };
        }
        cue
    }
    fn interact(&mut self) -> Cue {
        let Some(station) = self.station() else {
            self.note = "Stand on a station's marked floor tile to use it.";
            return Cue::None;
        };
        match (station, self.carry) {
            (Station::Pantry(i), Carry::Empty) if self.cash >= 2 => {
                self.cash -= 2;
                self.carry = Carry::Raw(i);
                self.note = "Ingredient collected. Hold Space at Chop to prepare it.";
                Cue::Action
            }
            (Station::Mix, Carry::Chopped(i)) if self.mixing & (1 << i) == 0 => {
                self.mixing |= 1 << i;
                self.carry = Carry::Empty;
                self.note = "Added to Mix. Empty hands + Space lifts a complete recipe.";
                Cue::Action
            }
            (Station::Mix, Carry::Empty) => {
                if let Some(recipe) = recipe_for(self.mixing) {
                    self.carry = Carry::Mixed(recipe);
                    self.mixing = 0;
                    self.note = if RECIPES[recipe].cook == 0 {
                        "Salad needs no cooking. Take it directly to Plate."
                    } else {
                        "Put the mixed bowl on either empty burner."
                    };
                    Cue::Good
                } else {
                    self.note = "The bowl needs a valid recipe. C opens the next recipe.";
                    Cue::None
                }
            }
            (Station::Burner(i), Carry::Mixed(recipe))
                if RECIPES[recipe].cook > 0 && self.burners[i].is_none() =>
            {
                self.burners[i] = Some(Pot { recipe, ticks: 0 });
                self.carry = Carry::Empty;
                self.note = "Cooking! Prep another dish, but watch the timer.";
                Cue::Action
            }
            (Station::Burner(i), Carry::Empty) => {
                if let Some(pot) = self.burners[i].filter(|p| self.pot_ready(*p)) {
                    self.carry = Carry::Cooked(pot.recipe);
                    self.burners[i] = None;
                    self.note = "Cooked just right. Take it to Plate.";
                    Cue::Good
                } else {
                    Cue::None
                }
            }
            (Station::Plate, Carry::Cooked(recipe)) => {
                self.carry = Carry::Plated(recipe);
                self.note = "Ready to serve! Take the plate to the Serve window.";
                Cue::Action
            }
            (Station::Plate, Carry::Mixed(recipe)) if RECIPES[recipe].cook == 0 => {
                self.carry = Carry::Plated(recipe);
                self.note = "Fresh salad plated. Deliver it at Serve.";
                Cue::Action
            }
            (Station::Serve, Carry::Plated(recipe)) => {
                let order = self
                    .orders
                    .iter()
                    .enumerate()
                    .filter_map(|(i, o)| o.filter(|o| o.recipe == recipe).map(|o| (i, o)))
                    .min_by_key(|(_, o)| o.number);
                if let Some((i, order)) = order {
                    let tip = 2 + order.patience * 8 / self.content().patience;
                    self.cash += RECIPES[recipe].value + tip;
                    self.score += 400 + tip * 25;
                    self.orders[i] = None;
                    self.carry = Carry::Empty;
                    self.served += 1;
                    self.total_served += 1;
                    self.note = "Served! Fresh ingredients and quick service earn better tips.";
                    Cue::Good
                } else {
                    self.note = "No guest ordered that dish. Keep it for a later guest or bin it.";
                    Cue::None
                }
            }
            (Station::Bin, _) => self.discard(),
            _ => Cue::None,
        }
    }
    fn discard(&mut self) -> Cue {
        if self.carry != Carry::Empty {
            self.carry = Carry::Empty;
            self.chopped_ticks = 0;
            self.waste = self.waste.saturating_add(1);
            self.note = "Discarded. Ingredients are not refunded.";
            return Cue::Bad;
        }
        match self.station() {
            Some(Station::Mix) if self.mixing != 0 => {
                self.mixing = 0;
                self.waste = self.waste.saturating_add(1);
                self.note = "Mixing bowl cleared. Start a fresh recipe.";
                Cue::Bad
            }
            Some(Station::Burner(i)) if self.burners[i].is_some_and(|p| self.pot_burnt(p)) => {
                self.burners[i] = None;
                self.note = "Burner cleaned. The next pot gets a fresh start.";
                Cue::Action
            }
            _ => Cue::None,
        }
    }
    /// Read-only legal controller for captures and regression playthroughs.
    /// It purchases ordinary upgrades, walks and cooks using the same controls as a player.
    pub fn pilot(&self, _tick: u64) -> Input {
        let action = || Input {
            primary: true,
            ..Input::default()
        };
        match self.phase {
            Phase::Briefing | Phase::Setback => return action(),
            Phase::Debrief => {
                for kind in [0, 1, 2] {
                    if self.upgrades[kind] < 2
                        && self.cash >= upgrade_cost(kind, self.upgrades[kind]) + 18
                    {
                        return Input {
                            slot: Some(kind + 1),
                            ..Input::default()
                        };
                    }
                }
                return action();
            }
            Phase::Service => {}
            _ => return Input::default(),
        }
        if self.move_wait > 0 {
            return Input::default();
        }
        let go = |s: Station, interaction: Input| {
            let p = access(s);
            if self.position == p {
                interaction
            } else {
                Input {
                    left: self.position.x > p.x,
                    right: self.position.x < p.x,
                    up: self.position.x == p.x && self.position.y > p.y,
                    down: self.position.x == p.x && self.position.y < p.y,
                    ..Input::default()
                }
            }
        };
        match self.carry {
            Carry::Raw(_) => go(
                Station::Chop,
                Input {
                    primary_down: true,
                    ..Input::default()
                },
            ),
            Carry::Chopped(_) => go(Station::Mix, action()),
            Carry::Mixed(recipe) if RECIPES[recipe].cook == 0 => go(Station::Plate, action()),
            Carry::Mixed(_) => {
                if let Some(i) = self.burners.iter().position(Option::is_none) {
                    go(Station::Burner(i), action())
                } else {
                    Input::default()
                }
            }
            Carry::Cooked(_) => go(Station::Plate, action()),
            Carry::Plated(_) => go(Station::Serve, action()),
            Carry::Empty => {
                if let Some(i) = self.burners.iter().position(Option::is_some) {
                    return go(Station::Burner(i), action());
                }
                let Some(order) = self.orders.iter().flatten().min_by_key(|o| o.number) else {
                    return Input::default();
                };
                let recipe = RECIPES[order.recipe];
                if self.mixing == recipe.mask {
                    return go(Station::Mix, action());
                }
                let missing = recipe.mask & !self.mixing;
                let ingredient = missing.trailing_zeros() as usize;
                if ingredient < 4 {
                    go(Station::Pantry(ingredient), action())
                } else {
                    Input::default()
                }
            }
        }
    }
    /// Completed-career or setback text for the shared host's paginated ending.
    pub fn summary(&self) -> String {
        if self.phase == Phase::Won {
            let stars: u32 = self.medals.iter().map(|s| u32::from(*s)).sum();
            let rank = if stars >= 20 {
                "Neighborhood Legend"
            } else if stars >= 15 {
                "Beloved Bistro"
            } else {
                "Open for Business"
            };
            format!(
                "The lanterns stay lit. You served {} of 42 guests across all seven services and earned {stars}/21 stars. Your grandmother's kitchen has become the neighborhood's table. Rank: {rank}. Cash left: {}. Score: {}. Try a new career for cleaner service and bigger tips.",
                self.total_served, self.cash, self.score
            )
        } else {
            format!(
                "The kitchen went quiet during {}. {} You served {} guests this career. Score: {}. A fresh career brings three service retries; improve your route and keep an eye on the cooking timers.",
                self.content().name,
                self.note,
                self.total_served,
                self.score
            )
        }
    }
}
fn walkable(cell: Cell) -> bool {
    (1..=9).contains(&cell.x) && (1..=4).contains(&cell.y)
}
fn menu_start(p: Vec2) -> bool {
    p.x >= 290. && p.x <= 670. && p.y >= 484. && p.y <= 536.
}

#[cfg(test)]
mod tests {
    use super::*;
    fn active() -> Game {
        let mut game = Game::new();
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        game
    }
    fn at(game: &mut Game, station: Station) {
        game.position = access(station);
        game.previous = game.position;
        game.move_wait = 0;
    }
    fn press(game: &mut Game) -> Cue {
        game.step(&Input {
            primary: true,
            ..Input::default()
        })
    }
    #[test]
    fn complete_career_uses_only_legal_inputs_and_replays_exactly() {
        let mut game = Game::new();
        let mut replay = Game::new();
        let mut seen = [false; 7];
        let mut finish = None;
        for tick in 0..180_000 {
            seen[game.shift_index] = true;
            let input = game.pilot(tick);
            game.step(&input);
            replay.step(&input);
            assert_eq!(game, replay, "tick {tick}");
            if game.outcome() != Outcome::Playing {
                finish = Some(tick);
                break;
            }
        }
        assert_eq!(
            game.outcome(),
            Outcome::Won,
            "shift {} ticks {} served {} missed {} carry {:?} mixing {} orders {:?} burners {:?}",
            game.shift(),
            game.ticks,
            game.served,
            game.missed,
            game.carry,
            game.mixing,
            game.orders,
            game.burners
        );
        assert!(seen.into_iter().all(|v| v));
        assert_eq!(game.total_served, 42);
        assert_eq!(game.medals, [3; 7]);
        assert_eq!(game.retries, 3);
        assert_eq!(game.upgrades, [2; 3]);
        eprintln!(
            "Legal seven-service victory at tick {}, score {}, cash {}, served {}",
            finish.unwrap(),
            game.score,
            game.cash,
            game.total_served
        );
        let frozen = game.clone();
        for _ in 0..30 {
            game.step(&Input {
                primary: true,
                secondary: true,
                tertiary: true,
                left: true,
                slot: Some(1),
                click: true,
                pointer: Vec2::ZERO,
                ..Input::default()
            });
        }
        assert_eq!(game, frozen);
    }
    #[test]
    fn pantry_charges_once_and_cannot_overwrite_carried_food() {
        let mut game = active();
        at(&mut game, Station::Pantry(0));
        assert_eq!(press(&mut game), Cue::Action);
        assert_eq!(game.cash, 46);
        assert_eq!(game.carry, Carry::Raw(0));
        for _ in 0..20 {
            assert_eq!(press(&mut game), Cue::None);
        }
        assert_eq!(game.cash, 46);
        assert_eq!(game.carry, Carry::Raw(0));
        game.carry = Carry::Empty;
        game.cash = 1;
        assert_eq!(press(&mut game), Cue::None);
        assert_eq!(game.carry, Carry::Empty);
        assert_eq!(game.cash, 1);
    }
    #[test]
    fn chopping_needs_held_action_and_movement_cancels_partial_work() {
        let mut game = active();
        at(&mut game, Station::Chop);
        game.carry = Carry::Raw(2);
        for _ in 0..30 {
            game.step(&Input::default());
        }
        assert_eq!(game.chopped_ticks, 0);
        for _ in 0..89 {
            game.step(&Input {
                primary_down: true,
                ..Input::default()
            });
        }
        assert_eq!(game.carry, Carry::Raw(2));
        game.step(&Input {
            right: true,
            ..Input::default()
        });
        assert_eq!(game.chopped_ticks, 0);
        at(&mut game, Station::Chop);
        for _ in 0..90 {
            game.step(&Input {
                primary_down: true,
                ..Input::default()
            });
        }
        assert_eq!(game.carry, Carry::Chopped(2));
        game.step(&Input {
            primary_down: true,
            ..Input::default()
        });
        assert_eq!(game.carry, Carry::Chopped(2));
    }
    #[test]
    fn mixture_rejects_raw_and_duplicate_ingredients_and_lifts_exact_recipe() {
        let mut game = active();
        at(&mut game, Station::Mix);
        game.carry = Carry::Raw(0);
        assert_eq!(press(&mut game), Cue::None);
        assert_eq!(game.mixing, 0);
        game.carry = Carry::Chopped(0);
        press(&mut game);
        assert_eq!(game.mixing, 1);
        assert_eq!(game.carry, Carry::Empty);
        game.carry = Carry::Chopped(0);
        assert_eq!(press(&mut game), Cue::None);
        assert_eq!(game.carry, Carry::Chopped(0));
        game.carry = Carry::Empty;
        assert_eq!(press(&mut game), Cue::None);
        assert_eq!(game.mixing, 1);
        game.carry = Carry::Chopped(1);
        press(&mut game);
        assert_eq!(game.mixing, 3);
        press(&mut game);
        assert_eq!(game.carry, Carry::Mixed(0));
        assert_eq!(game.mixing, 0);
        let score = game.score;
        press(&mut game);
        assert_eq!(game.score, score);
        assert_eq!(game.carry, Carry::Mixed(0));
    }
    #[test]
    fn burners_require_full_cook_time_and_burn_once_after_grace() {
        let mut game = active();
        at(&mut game, Station::Burner(0));
        game.carry = Carry::Mixed(1);
        press(&mut game);
        assert_eq!(game.carry, Carry::Empty);
        assert_eq!(game.burners[0].unwrap().ticks, 0);
        for _ in 0..178 {
            game.step(&Input::default());
        }
        assert_eq!(press(&mut game), Cue::None);
        assert_eq!(game.carry, Carry::Empty);
        press(&mut game);
        assert_eq!(game.carry, Carry::Cooked(1));
        assert!(game.burners[0].is_none());
        game.carry = Carry::Mixed(1);
        press(&mut game);
        for _ in 0..600 {
            game.step(&Input::default());
        }
        assert!(game.pot_burnt(game.burners[0].unwrap()));
        assert_eq!(game.waste, 1);
        assert_eq!(press(&mut game), Cue::None);
        assert_eq!(game.carry, Carry::Empty);
        assert_eq!(
            game.step(&Input {
                secondary: true,
                ..Input::default()
            }),
            Cue::Action
        );
        assert!(game.burners[0].is_none());
        assert_eq!(
            game.step(&Input {
                secondary: true,
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(game.waste, 1);
    }
    #[test]
    fn second_burner_runs_independently_and_cannot_be_overwritten() {
        let mut game = active();
        at(&mut game, Station::Burner(0));
        game.carry = Carry::Mixed(2);
        press(&mut game);
        game.carry = Carry::Mixed(4);
        assert_eq!(press(&mut game), Cue::None);
        assert_eq!(game.burners[0].unwrap().recipe, 2);
        at(&mut game, Station::Burner(1));
        press(&mut game);
        for _ in 0..240 {
            game.step(&Input::default());
        }
        assert!(game.pot_ready(game.burners[0].unwrap()));
        assert!(!game.pot_ready(game.burners[1].unwrap()));
    }
    #[test]
    fn plating_and_serving_validate_recipe_and_never_reward_twice() {
        let mut game = active();
        game.carry = Carry::Mixed(1);
        at(&mut game, Station::Plate);
        assert_eq!(press(&mut game), Cue::None);
        assert_eq!(game.carry, Carry::Mixed(1));
        game.carry = Carry::Mixed(0);
        press(&mut game);
        assert_eq!(game.carry, Carry::Plated(0));
        at(&mut game, Station::Serve);
        game.carry = Carry::Plated(1);
        assert_eq!(press(&mut game), Cue::None);
        assert_eq!(game.served, 0);
        game.carry = Carry::Plated(0);
        assert_eq!(press(&mut game), Cue::Good);
        assert_eq!(game.served, 1);
        assert_eq!(game.total_served, 1);
        let receipt = (game.cash, game.score);
        for _ in 0..20 {
            press(&mut game);
        }
        assert_eq!((game.cash, game.score), receipt);
    }
    #[test]
    fn patience_loses_guests_and_retries_restore_only_the_failed_service() {
        let mut game = active();
        let initial = game.saved;
        at(&mut game, Station::Serve);
        game.carry = Carry::Plated(0);
        press(&mut game);
        assert!(game.cash > initial.cash);
        assert!(game.score > 0);
        for _ in 0..15_000 {
            game.step(&Input::default());
            if game.phase == Phase::Setback {
                break;
            }
        }
        assert_eq!(game.phase, Phase::Setback);
        let ticks = game.ticks;
        for _ in 0..10 {
            game.step(&Input::default());
        }
        assert_eq!(game.ticks, ticks);
        press(&mut game);
        assert_eq!(game.phase, Phase::Service);
        assert_eq!(game.retries, 2);
        assert_eq!(game.cash, initial.cash);
        assert_eq!(game.score, initial.score);
        assert_eq!(game.total_served, initial.served);
        assert_eq!(game.missed, 0);
    }
    #[test]
    fn four_failed_services_end_the_career() {
        let mut game = active();
        for attempt in 0..4 {
            for _ in 0..15_000 {
                game.step(&Input::default());
                if game.phase != Phase::Service {
                    break;
                }
            }
            if attempt < 3 {
                assert_eq!(game.phase, Phase::Setback);
                press(&mut game);
            } else {
                assert_eq!(game.outcome(), Outcome::Lost);
            }
        }
        let frozen = game.clone();
        press(&mut game);
        assert_eq!(game, frozen);
    }
    #[test]
    fn upgrade_costs_caps_and_effects_are_real_and_menu_time_is_frozen() {
        let mut game = Game::new();
        game.phase = Phase::Debrief;
        game.cash = 300;
        for _ in 0..2 {
            game.step(&Input {
                slot: Some(1),
                ..Input::default()
            });
        }
        assert_eq!(game.upgrades, [2, 0, 0]);
        assert_eq!(game.cash, 180);
        assert_eq!(game.chop_ticks(), 42);
        let same = game.clone();
        assert_eq!(
            game.step(&Input {
                slot: Some(1),
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(game, same);
        game.step(&Input {
            slot: Some(2),
            ..Input::default()
        });
        assert_eq!(game.walk_ticks(), 8);
        game.step(&Input {
            slot: Some(3),
            ..Input::default()
        });
        assert_eq!(game.burn_grace(), 480);
        assert_eq!(game.ticks, 0);
        game.cash = 0;
        let rank = game.upgrades;
        game.step(&Input {
            slot: Some(2),
            ..Input::default()
        });
        assert_eq!(game.upgrades, rank);
    }
    #[test]
    fn authored_menus_have_unique_recipes_and_bounded_kitchen_capacity() {
        assert_eq!(SHIFTS.iter().map(|s| s.menu.len()).sum::<usize>(), 42);
        for (i, r) in RECIPES.iter().enumerate() {
            assert!(r.mask.count_ones() >= 2);
            assert_eq!(recipe_for(r.mask), Some(i));
        }
        for shift in SHIFTS {
            assert!(shift.menu.len() <= 8);
            assert!(shift.menu.iter().all(|r| *r < RECIPES.len()));
            assert_eq!(usize::from(shift.quota) + 1, shift.menu.len());
        }
        let mut game = active();
        for _ in 0..5000 {
            game.step(&Input::default());
            assert!(game.orders.iter().flatten().count() <= 3);
        }
    }
    #[test]
    fn pointer_walk_uses_the_same_station_access_cells() {
        let mut game = active();
        let pantry = STATIONS[0].1;
        game.step(&Input {
            click: true,
            pointer: Vec2::new(
                GRID_X + (pantry.x as f32 + 0.5) * TILE,
                GRID_Y + (pantry.y as f32 + 0.5) * TILE,
            ),
            ..Input::default()
        });
        for _ in 0..100 {
            game.step(&Input::default());
        }
        assert_eq!(game.position, access(Station::Pantry(0)));
        assert_eq!(game.station(), Some(Station::Pantry(0)));
        press(&mut game);
        assert_eq!(game.carry, Carry::Raw(0));
    }
    #[test]
    fn discard_spends_food_without_refunding_or_farming_score() {
        let mut game = active();
        at(&mut game, Station::Pantry(0));
        press(&mut game);
        let cash = game.cash;
        assert_eq!(
            game.step(&Input {
                secondary: true,
                ..Input::default()
            }),
            Cue::Bad
        );
        assert_eq!(game.cash, cash);
        assert_eq!(game.score, 0);
        assert_eq!(game.waste, 1);
        assert_eq!(
            game.step(&Input {
                secondary: true,
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(game.waste, 1);
    }

    #[test]
    fn last_guest_timeout_completes_a_met_quota_even_while_cook_moves() {
        let mut game = active();
        game.next_order = 4;
        game.served = 3;
        game.total_served = 3;
        game.orders = [
            Some(Order {
                recipe: 0,
                patience: 1,
                number: 4,
            }),
            None,
            None,
        ];
        game.move_wait = 8;
        assert_eq!(
            game.step(&Input {
                right: true,
                ..Input::default()
            }),
            Cue::Good
        );
        assert_eq!(game.phase, Phase::Debrief);
        assert_eq!(game.medals[0], 1);
        assert_eq!(game.missed, 1);
    }

    #[test]
    fn pointer_upgrade_and_launch_rectangles_match_the_drawn_cards() {
        let mut game = Game::new();
        game.step(&Input {
            click: true,
            pointer: Vec2::new(470., 510.),
            ..Input::default()
        });
        assert_eq!(game.phase, Phase::Service);
        game.phase = Phase::Debrief;
        game.cash = 200;
        game.step(&Input {
            click: true,
            pointer: Vec2::new(400., 380.),
            ..Input::default()
        });
        assert_eq!(game.upgrades, [0, 1, 0]);
        assert_eq!(game.cash, 150);
        game.step(&Input {
            click: true,
            pointer: Vec2::new(470., 510.),
            ..Input::default()
        });
        assert_eq!(game.phase, Phase::Briefing);
        assert_eq!(game.shift(), 2);
    }

    #[test]
    fn retry_preserves_prior_service_progress_and_purchased_equipment() {
        let mut game = Game::new();
        for tick in 0..10_000 {
            let command = game.pilot(tick);
            game.step(&command);
            if game.shift() == 2 && game.phase == Phase::Service {
                break;
            }
        }
        assert_eq!(game.shift(), 2);
        let retained = (game.saved, game.upgrades, game.medals);
        for _ in 0..20_000 {
            game.step(&Input::default());
            if game.phase == Phase::Setback {
                break;
            }
        }
        press(&mut game);
        assert_eq!((game.snapshot(), game.upgrades, game.medals), retained);
        assert_eq!(game.total_served, 4);
        assert_eq!(game.shift(), 2);
        assert_eq!(game.retries, 2);
    }
}
