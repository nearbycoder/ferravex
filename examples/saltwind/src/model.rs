//! Renderer-independent rules for a bounded merchant campaign.
//!
//! Each call advances one 60 Hz tick. Port planning consumes no calendar time;
//! a day at sea is 180 ticks. Menus and real-time clocks belong to the host.
//!
//! ```
//! use ferravex_arcade::{Input, Outcome};
//! use saltwind::Game;
//! let mut game = Game::new();
//! game.tick(&Input { slot: Some(2), ..Input::default() });
//! let before = game.coins();
//! game.tick(&Input { primary: true, ..Input::default() });
//! assert!(game.coins() < before);
//! assert_eq!(game.cargo(0), 1);
//! assert_eq!(game.result(), Outcome::Playing);
//! ```

use crate::content::{CHARTERS, GOODS, NEWS, PORTS};
use ferravex_arcade::{Cue, Input, Outcome};
use ferravex_math::Vec2;
use ferravex_play::gameplay::inventory::{Inventory, Stack};

pub(crate) const DAY_TICKS: u32 = 180;
pub(crate) const DEADLINE: u32 = 42;
pub(crate) const DEBT: u32 = 900;
pub(crate) const SUPPLY_CAP: u32 = 12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Passage {
    pub from: usize,
    pub to: usize,
    pub total: u32,
    pub traveled: u32,
    pub day_ticks: u32,
    pub offset: f32,
    pub sheltered: bool,
    pub reefed: bool,
}

/// A six-charter voyage with finite stock, calendar, cargo and ship resources.
///
/// Use [`Self::tick`] with player inputs; accessors do not advance the rules.
#[derive(Clone, Debug)]
pub struct Game {
    pub(crate) coins: u32,
    pub(crate) hull: u32,
    pub(crate) provisions: u32,
    pub(crate) day: u32,
    pub(crate) harbor: usize,
    pub(crate) selected_port: usize,
    pub(crate) page: usize,
    pub(crate) row: usize,
    pub(crate) charter: usize,
    pub(crate) inventory: Inventory<3>,
    pub(crate) stock: [[u32; 3]; 6],
    pub(crate) upgrades: [bool; 3],
    pub(crate) visited: [bool; 6],
    pub(crate) sheltered: bool,
    pub(crate) passage: Option<Passage>,
    pub(crate) score: u32,
    pub(crate) notice: String,
    pub(crate) notice_ticks: u32,
    pub(crate) result: Outcome,
    pub(crate) loss: &'static str,
    pub(crate) total_trades: u32,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    /// Begin at Haven with 180 crowns, eight cargo slots and twelve days of food.
    pub fn new() -> Self {
        Self {
            coins: 180,
            hull: 100,
            provisions: 12,
            day: 1,
            harbor: 0,
            selected_port: 1,
            page: 2,
            row: 0,
            charter: 0,
            inventory: Inventory::new().expect("three fixed cargo stacks"),
            stock: [[8; 3]; 6],
            upgrades: [false; 3],
            visited: [true, false, false, false, false, false],
            sheltered: true,
            passage: None,
            score: 0,
            notice: "Your first charter: buy 3 grain at Market [2], then sail to Brinewatch on Chart [1].".into(),
            notice_ticks: 600,
            result: Outcome::Playing,
            loss: "",
            total_trades: 0,
        }
    }

    /// Current money; no market or calendar state changes.
    pub fn coins(&self) -> u32 {
        self.coins
    }

    /// Count a cargo good (0 grain, 1 cloth, 2 spice); unknown IDs return zero.
    pub fn cargo(&self, good: usize) -> u32 {
        if good < 3 {
            self.inventory.count(good as u32)
        } else {
            0
        }
    }

    /// Whether the debt was settled, the ship was lost, or play continues.
    pub fn result(&self) -> Outcome {
        self.result
    }

    pub(crate) fn cargo_total(&self) -> u32 {
        self.inventory.stacks().map(|s| s.amount).sum()
    }

    pub(crate) fn capacity(&self) -> u32 {
        if self.upgrades[1] { 12 } else { 8 }
    }

    pub(crate) fn week(&self) -> usize {
        ((self.day - 1) / 7).min(5) as usize
    }

    pub(crate) fn news(&self) -> &'static str {
        NEWS[self.week()]
    }

    fn price_at(&self, port: usize, good: usize) -> u32 {
        let weather = match (self.week(), good) {
            (1, 1) | (2, 2) | (4, 0) | (5, 1) => 5,
            (3, 0) => -4,
            _ => 0,
        };
        let scarcity = 8 - self.stock[port][good] as i32;
        (PORTS[port].prices[good] as i32 + weather + scarcity).max(5) as u32
    }

    pub(crate) fn buy_price(&self, good: usize) -> u32 {
        self.price_at(self.harbor, good)
    }

    pub(crate) fn sell_price(&self, good: usize) -> u32 {
        self.buy_price(good).saturating_sub(7).max(1)
    }

    pub(crate) fn wind(&self) -> Vec2 {
        // An authored six-week forecast, independent of wall time and RNG.
        const WINDS: [[f32; 2]; 6] = [
            [0.8, -0.4],
            [0.7, 0.5],
            [-0.5, 0.8],
            [-0.8, -0.2],
            [0.3, -0.8],
            [0.8, 0.4],
        ];
        Vec2::new(WINDS[self.week()][0], WINDS[self.week()][1])
    }

    pub(crate) fn position(port: usize) -> Vec2 {
        Vec2::new(PORTS[port].position[0], PORTS[port].position[1])
    }

    pub(crate) fn voyage_days(&self, to: usize) -> u32 {
        if to == self.harbor || to >= PORTS.len() {
            return 0;
        }
        let direction = Self::position(to) - Self::position(self.harbor);
        let base = (direction.length() / 70.).ceil() as i32;
        let facing = direction.normalize_or_zero().dot(self.wind());
        let wind_days = if facing < -0.35 {
            1
        } else if facing > 0.65 {
            -1
        } else {
            0
        };
        (base + wind_days + i32::from(self.sheltered) - i32::from(self.upgrades[0])).max(2) as u32
    }

    pub(crate) fn boat_position(&self) -> Vec2 {
        if let Some(p) = self.passage {
            let from = Self::position(p.from);
            let to = Self::position(p.to);
            let direction = (to - from).normalize_or_zero();
            let side = Vec2::new(-direction.y, direction.x);
            let t = p.traveled as f32 / p.total as f32;
            let arc = if p.sheltered {
                (t * std::f32::consts::PI).sin() * 34.
            } else {
                0.
            };
            from.lerp(to, t) + side * (arc + p.offset * (t * std::f32::consts::PI).sin())
        } else {
            Self::position(self.harbor) + Vec2::new(0., 20.)
        }
    }

    pub(crate) fn storms(&self) -> [(Vec2, f32); 2] {
        let shift = (self.day % 7) as f32 * 7.;
        [
            (Vec2::new(285. + shift, 315.), 48.),
            (Vec2::new(340. - shift * 0.5, 409.), 38.),
        ]
    }

    fn tell(&mut self, text: impl Into<String>) {
        self.notice = text.into();
        self.notice_ticks = 360;
    }

    fn lose(&mut self, reason: &'static str) -> Cue {
        self.result = Outcome::Lost;
        self.loss = reason;
        self.tell(reason);
        Cue::Bad
    }

    fn advance_day(&mut self, at_sea: bool) -> Cue {
        self.day += 1;
        if self.day > DEADLINE {
            return self.lose(
                "The season closed. The banker repossessed Saltwind before you settled the debt.",
            );
        }
        if at_sea {
            if self.provisions > 0 {
                self.provisions -= 1;
            } else {
                self.hull = self.hull.saturating_sub(22);
                self.tell(
                    "No food! Exhausted sailors leave the ship taking on water: -22 hull per day.",
                );
                if self.hull == 0 {
                    return self.lose("Your hungry crew could no longer keep the ship afloat.");
                }
            }
        }
        if (self.day - 1).is_multiple_of(7) {
            for stock in &mut self.stock {
                for amount in stock {
                    *amount = (*amount + 2).min(12);
                }
            }
        }
        Cue::None
    }

    fn buy(&mut self) -> Cue {
        match self.row {
            good @ 0..=2 => {
                let price = self.buy_price(good);
                if self.stock[self.harbor][good] == 0 {
                    self.tell("That market stall is empty. Weekly caravans restore two crates.");
                    return Cue::None;
                }
                if self.cargo_total() >= self.capacity() {
                    self.tell("The hold is full. Sell cargo or fit a larger hold at the shipyard.");
                    return Cue::None;
                }
                if self.coins < price {
                    self.tell("Not enough crowns for this crate.");
                    return Cue::None;
                }
                if self
                    .inventory
                    .add(Stack {
                        item: good as u32,
                        amount: 1,
                        limit: 12,
                    })
                    .is_err()
                {
                    return Cue::None;
                }
                self.coins -= price;
                self.stock[self.harbor][good] -= 1;
                self.total_trades += 1;
                self.tell(format!(
                    "Bought 1 {} for {price} crowns. Cargo {}/{}.",
                    GOODS[good],
                    self.cargo_total(),
                    self.capacity()
                ));
            }
            3 => {
                if self.provisions == SUPPLY_CAP {
                    self.tell("The pantry is full: twelve days of provisions.");
                    return Cue::None;
                }
                let amount = (SUPPLY_CAP - self.provisions).min(6).min(self.coins / 2);
                let price = amount * 2;
                if amount == 0 {
                    self.tell("Food costs 2 crowns per day; sell a crate to provision the crew.");
                    return Cue::None;
                }
                self.coins -= price;
                self.provisions += amount;
                self.tell(format!("Loaded {amount} days of food for {price} crowns."));
            }
            4 => {
                if self.hull == 100 {
                    self.tell("The hull is already sound.");
                    return Cue::None;
                }
                let amount = (100 - self.hull).min(30);
                let price = amount.div_ceil(2);
                if self.coins < price {
                    self.tell("Repairs cost 1 crown per 2 hull. Sell cargo first.");
                    return Cue::None;
                }
                self.coins -= price;
                self.hull += amount;
                self.tell(format!(
                    "The shipwright restored {amount} hull for {price} crowns."
                ));
            }
            _ => return Cue::None,
        }
        Cue::Action
    }

    fn sell(&mut self) -> Cue {
        if self.row > 2 {
            self.tell("Food and repair work cannot be resold.");
            return Cue::None;
        }
        let good = self.row;
        if self.cargo(good) == 0 {
            self.tell("You have no crates of this good.");
            return Cue::None;
        }
        if self.stock[self.harbor][good] >= 16 {
            self.tell("The warehouse is full; sail to another market.");
            return Cue::None;
        }
        let price = self.sell_price(good);
        if self.inventory.remove(good as u32, 1).is_err() {
            return Cue::None;
        }
        self.coins += price;
        self.stock[self.harbor][good] += 1;
        self.total_trades += 1;
        self.tell(format!(
            "Sold 1 {} for {price} crowns. Local buy/sell spread prevents free profit.",
            GOODS[good]
        ));
        Cue::Action
    }

    fn deliver(&mut self) -> Cue {
        let Some(contract) = CHARTERS.get(self.charter).copied() else {
            self.tell("All six charters are sealed. Settle 900 crowns at Haven's shipyard [4].");
            return Cue::None;
        };
        if self.harbor != contract.destination {
            self.tell(format!(
                "This charter must be delivered at {}.",
                PORTS[contract.destination].name
            ));
            return Cue::None;
        }
        if self.cargo(contract.good) < contract.amount {
            self.tell(format!(
                "Need {} {} crates in the hold to seal this charter.",
                contract.amount, GOODS[contract.good]
            ));
            return Cue::None;
        }
        if self
            .inventory
            .remove(contract.good as u32, contract.amount)
            .is_err()
        {
            return Cue::None;
        }
        self.coins += contract.reward;
        self.charter += 1;
        self.score += 300 + contract.reward;
        self.selected_port = CHARTERS.get(self.charter).map_or(0, |c| c.destination);
        self.tell(contract.arrival);
        Cue::Good
    }

    fn shipyard(&mut self) -> Cue {
        if self.row == 3 {
            if self.harbor != 0 {
                self.tell("The banker keeps her office at Haven. Sail home to settle.");
                return Cue::None;
            }
            if self.charter < CHARTERS.len() {
                self.tell("Your charter bond requires all six deliveries before the ship can be released.");
                return Cue::None;
            }
            if self.coins < DEBT {
                self.tell(format!(
                    "You still need {} crowns. Trade surplus cargo before the season ends.",
                    DEBT - self.coins
                ));
                return Cue::None;
            }
            self.coins -= DEBT;
            self.score += self.coins * 2 + (DEADLINE - self.day) * 20 + self.hull * 5;
            self.result = Outcome::Won;
            self.tell(
                "The lien is torn in two. Saltwind is yours, and every island knows your name.",
            );
            return Cue::Win;
        }
        if self.row >= 3 {
            return Cue::None;
        }
        let cost = [85, 70, 70][self.row];
        if self.upgrades[self.row] {
            self.tell("That upgrade is already installed.");
            return Cue::None;
        }
        if self.coins < cost {
            self.tell(format!(
                "This refit needs {cost} crowns. Keep enough aside for cargo and debt."
            ));
            return Cue::None;
        }
        self.coins -= cost;
        self.upgrades[self.row] = true;
        self.tell(
            [
                "New sails fitted. Every passage is one day shorter (minimum two).",
                "Expanded hold fitted: twelve crates of cargo.",
                "Brass plating fitted: storm damage reduced by three.",
            ][self.row],
        );
        Cue::Good
    }

    fn depart(&mut self) -> Cue {
        if self.selected_port >= PORTS.len() || self.selected_port == self.harbor {
            self.tell("Choose another island on the chart.");
            return Cue::None;
        }
        if self.provisions == 0 {
            self.tell("Provision the crew at Market [2] before leaving port.");
            return Cue::None;
        }
        let days = self.voyage_days(self.selected_port);
        self.passage = Some(Passage {
            from: self.harbor,
            to: self.selected_port,
            total: days * DAY_TICKS * 100,
            traveled: 0,
            day_ticks: 0,
            offset: 0.,
            sheltered: self.sheltered,
            reefed: false,
        });
        self.tell(format!(
            "Underway to {}. Estimated {days} days. Up/down steer; Space reefs sails in storms.",
            PORTS[self.selected_port].name
        ));
        Cue::Action
    }

    fn sail(&mut self, input: &Input) -> Cue {
        let mut passage = self.passage.expect("underway");
        if input.primary
            || (input.click
                && (658.0..926.0).contains(&input.pointer.x)
                && (478.0..509.0).contains(&input.pointer.y))
        {
            passage.reefed = !passage.reefed;
        }
        let steer = i32::from(input.down) - i32::from(input.up);
        passage.offset = (passage.offset + steer as f32 * 0.6).clamp(-55., 55.);
        passage.traveled =
            (passage.traveled + if passage.reefed { 70 } else { 100 }).min(passage.total);
        passage.day_ticks += 1;
        self.passage = Some(passage);
        let mut cue = Cue::None;
        if passage.day_ticks >= DAY_TICKS {
            passage.day_ticks = 0;
            cue = self.advance_day(true);
            if self.result != Outcome::Playing {
                return cue;
            }
            let pos = self.boat_position();
            let in_storm = self
                .storms()
                .iter()
                .any(|(center, radius)| pos.distance(*center) < *radius);
            let damage: u32 = if !passage.sheltered && in_storm {
                if passage.reefed { 3 } else { 11 }
            } else if !passage.sheltered && self.day.is_multiple_of(6) {
                4
            } else {
                0
            };
            let damage = damage.saturating_sub(if self.upgrades[2] { 3 } else { 0 });
            if damage > 0 {
                self.hull = self.hull.saturating_sub(damage);
                self.tell(format!("A squall tears at the ship: -{damage} hull. Reef sails or steer around storm rings."));
                cue = Cue::Bad;
                if self.hull == 0 {
                    return self
                        .lose("Saltwind sank in the outer sea. Her charters never reached home.");
                }
            }
        }
        self.passage = Some(passage);
        if passage.traveled >= passage.total {
            // A reefed final partial day still consumes calendar time and food.
            if passage.day_ticks > 0 {
                cue = self.advance_day(true);
                if self.result != Outcome::Playing {
                    return cue;
                }
            }
            self.harbor = passage.to;
            self.passage = None;
            if !self.visited[self.harbor] {
                self.visited[self.harbor] = true;
                self.score += 75;
            }
            self.page = 2;
            self.row = 0;
            self.tell(format!(
                "Docked at {}. Open Charters [3] to deliver; Market [2] to resupply.",
                PORTS[self.harbor].name
            ));
            return Cue::Good;
        }
        cue
    }

    /// Advance one fixed tick, accepting only available trades and legal commands.
    /// Terminal states are immutable. Input fields are consumed only by the caller's tick.
    pub fn tick(&mut self, input: &Input) -> Cue {
        if self.result != Outcome::Playing {
            return Cue::None;
        }
        self.notice_ticks = self.notice_ticks.saturating_sub(1);
        if self.passage.is_some() {
            return self.sail(input);
        }
        let mut primary = input.primary;
        let mut secondary = input.secondary;
        if let Some(slot @ 1..=5) = input.slot {
            self.page = slot - 1;
            self.row = 0;
        }
        if input.tertiary {
            self.page = 0;
            self.row = 0;
        }
        if input.click {
            let p = input.pointer;
            if (650.0..935.0).contains(&p.x) && (118.0..149.0).contains(&p.y) {
                self.page = (((p.x - 650.) / 57.) as usize).min(4);
                self.row = 0;
            } else if p.x < 625. && p.y > 155. && p.y < 550. {
                if let Some(port) =
                    (0..PORTS.len()).find(|&port| p.distance(Self::position(port)) < 29.)
                {
                    self.selected_port = port;
                    self.page = 0;
                }
            } else if (658.0..926.0).contains(&p.x) {
                if (478.0..509.0).contains(&p.y) {
                    primary = true;
                } else if (514.0..545.0).contains(&p.y) {
                    secondary = true;
                } else {
                    let (top, height, count) = match self.page {
                        0 => (194., 30., 6),
                        1 => (206., 40., 5),
                        3 => (199., 60., 4),
                        _ => (0., 0., 0),
                    };
                    if p.y >= top && p.y < top + height * count as f32 {
                        self.row = ((p.y - top) / height) as usize;
                        if self.page == 0 {
                            self.selected_port = self.row;
                        }
                    }
                }
            }
        }
        let count = match self.page {
            0 => 6,
            1 => 5,
            3 => 4,
            _ => 1,
        };
        let direction = i32::from(input.down_pressed || input.right_pressed)
            - i32::from(input.up_pressed || input.left_pressed);
        if direction != 0 {
            if self.page == 0 {
                self.selected_port = (self.selected_port as i32 + direction).rem_euclid(6) as usize;
            } else {
                self.row = (self.row as i32 + direction).rem_euclid(count) as usize;
            }
        }
        let cue = match self.page {
            0 if primary => self.depart(),
            0 if secondary => {
                self.sheltered = !self.sheltered;
                self.tell(if self.sheltered {
                    "Sheltered route: one extra day, protected from squalls."
                } else {
                    "Open sea: quicker, but steer clear of storm rings. Space reefs sails."
                });
                Cue::Action
            }
            1 if primary => self.buy(),
            1 if secondary => self.sell(),
            2 if primary && self.charter == CHARTERS.len() => {
                self.page = 3;
                self.row = 3;
                Cue::Action
            }
            2 if primary => self.deliver(),
            2 if secondary => {
                self.page = 1;
                self.row = 0;
                Cue::Action
            }
            3 | 4 if secondary => {
                self.page = 0;
                self.row = 0;
                Cue::Action
            }
            3 if primary => self.shipyard(),
            4 if primary => {
                self.tell("A day passes in harbor. The debt deadline does not wait.");
                self.advance_day(false)
            }
            _ => Cue::None,
        };
        if self.passage.is_none()
            && self.coins < 2
            && self.provisions == 0
            && self.cargo_total() == 0
            && self.result == Outcome::Playing
        {
            return self.lose("With no food, cargo or coin, the crew disbanded. Restart to plan a solvent voyage.");
        }
        cue
    }

    pub(crate) fn rank(&self) -> &'static str {
        if self.day <= 30 && self.hull >= 85 && self.coins >= 150 {
            "Admiral of the Isles"
        } else if self.day <= 36 && self.hull >= 65 {
            "Master Mariner"
        } else {
            "Free Captain"
        }
    }

    /// A deterministic pilot that issues ordinary menu and sailing inputs.
    /// No state edits, hidden inventory grants or hazard exemptions are used.
    pub fn pilot(&self, tick: u64) -> Input {
        let mut input = Input::default();
        if self.result != Outcome::Playing || self.passage.is_some() || !tick.is_multiple_of(8) {
            return input;
        }
        let choose_page = |wanted: usize| -> Input {
            Input {
                slot: Some(wanted + 1),
                ..Input::default()
            }
        };
        let choose_row = |wanted: usize| -> Input {
            Input {
                down_pressed: self.row != wanted,
                primary: self.row == wanted,
                ..Input::default()
            }
        };
        if let Some(charter) = CHARTERS.get(self.charter) {
            if self.harbor == charter.destination && self.cargo(charter.good) >= charter.amount {
                if self.page != 2 {
                    return choose_page(2);
                }
                input.primary = true;
            } else if self.cargo(charter.good) < charter.amount {
                if self.page != 1 {
                    return choose_page(1);
                }
                return choose_row(charter.good);
            } else if self.provisions < self.voyage_days(charter.destination) {
                if self.page != 1 {
                    return choose_page(1);
                }
                return choose_row(3);
            } else {
                if self.page != 0 {
                    return choose_page(0);
                }
                if self.selected_port != charter.destination {
                    input.down_pressed = true;
                } else {
                    input.primary = true;
                }
            }
        } else {
            if self.page != 3 {
                return choose_page(3);
            }
            return choose_row(3);
        }
        input
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(game: &mut Game, slot: usize) {
        game.tick(&Input {
            slot: Some(slot),
            ..Input::default()
        });
    }
    fn press(game: &mut Game) -> Cue {
        game.tick(&Input {
            primary: true,
            ..Input::default()
        })
    }
    fn down(game: &mut Game) {
        game.tick(&Input {
            down_pressed: true,
            ..Input::default()
        });
    }

    #[test]
    fn trade_spread_consumes_money_and_never_awards_points() {
        let mut game = Game::new();
        key(&mut game, 2);
        let initial = game.coins;
        for _ in 0..15 {
            press(&mut game);
            game.tick(&Input {
                secondary: true,
                ..Input::default()
            });
        }
        assert!(game.coins < initial);
        assert_eq!(game.score, 0);
        assert_eq!(game.cargo_total(), 0);
        assert_eq!(game.stock[0][0], 8);
    }

    #[test]
    fn cargo_capacity_and_rejected_actions_are_atomic() {
        let mut game = Game::new();
        key(&mut game, 2);
        for _ in 0..8 {
            press(&mut game);
        }
        assert_eq!(game.cargo_total(), 8);
        down(&mut game);
        let before = (game.coins, game.stock, game.cargo_total(), game.score);
        for _ in 0..20 {
            assert_eq!(press(&mut game), Cue::None);
        }
        assert_eq!(
            (game.coins, game.stock, game.cargo_total(), game.score),
            before
        );
        assert_eq!(game.day, 1);
    }

    #[test]
    fn wrong_port_and_duplicate_delivery_do_not_mint_rewards() {
        let mut game = Game::new();
        key(&mut game, 2);
        for _ in 0..3 {
            press(&mut game);
        }
        key(&mut game, 3);
        let before = (game.coins, game.cargo_total(), game.charter);
        assert_eq!(press(&mut game), Cue::None);
        assert_eq!((game.coins, game.cargo_total(), game.charter), before);
        key(&mut game, 1);
        press(&mut game);
        while game.passage.is_some() {
            game.tick(&Input::default());
        }
        assert_eq!(press(&mut game), Cue::Good);
        let earned = game.coins;
        assert_eq!(press(&mut game), Cue::None);
        assert_eq!(game.coins, earned);
        assert_eq!(game.charter, 1);
    }

    #[test]
    fn passages_consume_forecast_days_and_food() {
        let mut game = Game::new();
        key(&mut game, 1);
        let days = game.voyage_days(1);
        press(&mut game);
        for _ in 0..days * DAY_TICKS {
            game.tick(&Input::default());
        }
        assert!(game.passage.is_none());
        assert_eq!(game.harbor, 1);
        assert_eq!(game.day, 1 + days);
        assert_eq!(game.provisions, 12 - days);
        assert_eq!(game.hull, 100);
    }

    #[test]
    fn reefing_costs_time_and_food_without_advancing_in_port() {
        let mut normal = Game::new();
        key(&mut normal, 1);
        press(&mut normal);
        let mut reefed = normal.clone();
        reefed.tick(&Input {
            primary: true,
            ..Input::default()
        });
        while normal.passage.is_some() {
            normal.tick(&Input::default());
        }
        while reefed.passage.is_some() {
            reefed.tick(&Input::default());
        }
        assert!(reefed.day > normal.day);
        assert!(reefed.provisions < normal.provisions);
        let day = reefed.day;
        for _ in 0..500 {
            reefed.tick(&Input::default());
        }
        assert_eq!(reefed.day, day);
    }

    #[test]
    fn calendar_failure_is_terminal_and_restart_restores_all_resources() {
        let mut game = Game::new();
        key(&mut game, 5);
        for _ in 0..DEADLINE {
            press(&mut game);
        }
        assert_eq!(game.result(), Outcome::Lost);
        let before = (
            game.day,
            game.coins,
            game.hull,
            game.score,
            game.notice.clone(),
        );
        for _ in 0..200 {
            game.tick(&Input {
                primary: true,
                slot: Some(2),
                ..Input::default()
            });
        }
        assert_eq!(
            (
                game.day,
                game.coins,
                game.hull,
                game.score,
                game.notice.clone()
            ),
            before
        );
        let reset = Game::new();
        assert_eq!(
            (
                reset.day,
                reset.coins,
                reset.hull,
                reset.provisions,
                reset.charter
            ),
            (1, 180, 100, 12, 0)
        );
        assert_eq!(reset.result(), Outcome::Playing);
    }

    #[test]
    fn complete_six_charter_voyage_and_debt_settlement_use_legal_inputs() {
        let mut game = Game::new();
        for tick in 0..40_000 {
            let input = game.pilot(tick);
            game.tick(&input);
            if game.result() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(
            game.result(),
            Outcome::Won,
            "day={} crowns={} charter={} food={} notice={}",
            game.day,
            game.coins,
            game.charter,
            game.provisions,
            game.notice
        );
        assert_eq!(game.charter, CHARTERS.len());
        assert!(game.visited.iter().all(|&value| value));
        assert_eq!(game.harbor, 0);
        assert!(game.day <= DEADLINE);
        assert!(game.score > 4000);
        let final_state = (game.coins, game.score, game.day, game.charter);
        for _ in 0..20 {
            press(&mut game);
        }
        assert_eq!(
            (game.coins, game.score, game.day, game.charter),
            final_state
        );
    }

    #[test]
    fn pilot_replay_is_reproducible_and_inventory_remains_bounded() {
        let mut a = Game::new();
        let mut b = Game::new();
        for tick in 0..10_000 {
            let input = a.pilot(tick);
            a.tick(&input);
            b.tick(&input);
            assert_eq!(
                (a.coins, a.day, a.hull, a.charter, a.cargo_total()),
                (b.coins, b.day, b.hull, b.charter, b.cargo_total())
            );
            assert!(a.cargo_total() <= a.capacity());
        }
    }

    #[test]
    fn starving_on_a_legal_underprovisioned_passage_loses_the_ship() {
        let mut game = Game::new();
        key(&mut game, 1);
        while game.selected_port != 4 {
            down(&mut game);
        }
        press(&mut game);
        press(&mut game); // Reefing stretches a six-day passage to nine days.
        while game.passage.is_some() && game.result() == Outcome::Playing {
            game.tick(&Input::default());
        }
        assert_eq!(game.harbor, 4);
        assert!(game.provisions > 0 && game.provisions < 6);
        key(&mut game, 1);
        while game.selected_port != 0 {
            down(&mut game);
        }
        press(&mut game);
        press(&mut game);
        for _ in 0..4_000 {
            game.tick(&Input::default());
            if game.result() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(game.result(), Outcome::Lost);
        assert_eq!(game.hull, 0);
        assert_eq!(game.provisions, 0);
        assert!(game.loss.contains("hungry"));
    }

    #[test]
    fn click_targets_share_the_displayed_port_and_market_rows() {
        let mut game = Game::new();
        game.tick(&Input {
            click: true,
            pointer: Game::position(2),
            ..Input::default()
        });
        assert_eq!(game.page, 0);
        assert_eq!(game.selected_port, 2);
        game.tick(&Input {
            click: true,
            pointer: Vec2::new(730., 130.),
            ..Input::default()
        });
        assert_eq!(game.page, 1);
        game.tick(&Input {
            click: true,
            pointer: Vec2::new(700., 298.),
            ..Input::default()
        });
        assert_eq!(game.row, 2);
        let crowns = game.coins;
        game.tick(&Input {
            click: true,
            pointer: Vec2::new(700., 491.),
            ..Input::default()
        });
        assert_eq!(game.cargo(2), 1);
        assert!(game.coins < crowns);
    }

    #[test]
    fn open_sea_squalls_damage_hull_and_armor_reduces_accepted_hits() {
        let mut plain = Game::new();
        let mut armored = Game::new();
        key(&mut armored, 4);
        down(&mut armored);
        down(&mut armored);
        press(&mut armored);
        for game in [&mut plain, &mut armored] {
            key(game, 1);
            while game.selected_port != 3 {
                down(game);
            }
            game.tick(&Input {
                secondary: true,
                ..Input::default()
            });
            press(game);
            while game.passage.is_some() && game.result() == Outcome::Playing {
                game.tick(&Input::default());
            }
            assert_eq!(game.harbor, 3);
        }
        assert!(plain.hull < 100);
        assert!(armored.hull > plain.hull);
        assert_eq!(armored.day, plain.day);
    }

    #[test]
    fn mouse_can_reef_the_sails_using_the_painted_action_button() {
        let mut game = Game::new();
        key(&mut game, 1);
        press(&mut game);
        game.tick(&Input {
            click: true,
            pointer: Vec2::new(720., 493.),
            ..Input::default()
        });
        assert!(game.passage.unwrap().reefed);
        assert_eq!(game.passage.unwrap().traveled, 70);
    }

    #[test]
    fn upgrades_spend_once_and_reduce_time_or_expand_capacity() {
        let mut game = Game::new();
        let initial_days = game.voyage_days(1);
        key(&mut game, 4);
        assert_eq!(press(&mut game), Cue::Good);
        assert_eq!(game.coins, 95);
        assert!(game.voyage_days(1) < initial_days);
        assert_eq!(press(&mut game), Cue::None);
        assert_eq!(game.coins, 95);
        down(&mut game);
        assert_eq!(press(&mut game), Cue::Good);
        assert_eq!(game.capacity(), 12);
        assert_eq!(game.coins, 25);
    }
}
