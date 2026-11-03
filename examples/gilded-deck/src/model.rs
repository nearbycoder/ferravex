//! Renderer-independent, deterministic deck-building rules.
//!
//! ```
//! use gilded_deck::Game;
//! use ferravex_arcade::{Input, Outcome};
//! let mut game = Game::new();
//! assert_eq!(game.encounter(), 1);
//! game.step(&Input { slot: Some(1), ..Input::default() }); // choose Gilt Needle
//! let health = game.health();
//! game.step(&Input::default()); // planning costs nothing
//! assert_eq!(game.health(), health);
//! assert_eq!(game.outcome(), Outcome::Playing);
//! ```
use crate::content::{Card, ENCOUNTERS, Encounter, Intent, Kind, RELIC_NAMES, REWARDS};
use ferravex_arcade::{Cue, Input, Outcome};
use ferravex_play::gameplay::loot::RewardRng;
pub(crate) const MAX_DECK: usize = 28;
const MAX_BATTLE: usize = 40;
pub(crate) const MAX_HAND: usize = 7;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Oath,
    Battle,
    Reward,
    Relic,
    Camp,
    Forge,
    Shop,
    Won,
    Lost,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_arcade::vec2;
    fn choice(slot: usize) -> Input {
        Input {
            slot: Some(slot),
            ..Input::default()
        }
    }
    fn battle() -> Game {
        let mut g = Game::new();
        g.step(&choice(1));
        g
    }
    fn hand(g: &mut Game, kinds: &[Kind]) {
        g.hand = kinds.iter().copied().map(Card::new).collect();
        g.selected = 0;
    }
    #[test]
    fn complete_run_uses_legal_controls_and_replays_exactly() {
        let mut game = Game::new();
        let mut replay = Game::new();
        let mut visited = [false; 9];
        let mut finish = None;
        for tick in 0..100_000 {
            visited[game.floor] = true;
            let command = game.pilot(tick);
            game.step(&command);
            replay.step(&command);
            assert_eq!(game, replay, "diverged at tick {tick}");
            assert!(
                game.hand.len() <= MAX_HAND
                    && game.deck.len() <= MAX_DECK
                    && game.battle_cards() <= MAX_BATTLE
            );
            if game.outcome() != Outcome::Playing {
                finish = Some(tick);
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Won, "{}", game.summary());
        assert!(visited.into_iter().all(|v| v));
        assert!(game.won_turns.into_iter().all(|t| (1..=40).contains(&t)));
        assert_eq!(game.relics.iter().filter(|&&r| r).count(), 3);
        assert!(game.deck.iter().any(|c| c.upgraded));
        assert!(game.score > 18_000);
        eprintln!(
            "Legal victory tick {}, {} HP, {} cards, {} coins, {} points; turns {:?}",
            finish.unwrap(),
            game.health(),
            game.deck.len(),
            game.coins,
            game.score,
            game.won_turns
        );
        let frozen = game.clone();
        for slot in 1..=5 {
            game.step(&Input {
                primary: true,
                secondary: true,
                tertiary: true,
                slot: Some(slot),
                click: true,
                pointer: vec2(630., 350.),
                ..Input::default()
            });
        }
        assert_eq!(game, frozen);
    }
    #[test]
    fn all_three_starter_relics_have_a_complete_legal_route() {
        for starter in 1..=3 {
            let mut game = Game::new();
            game.step(&choice(starter));
            for tick in 0..100_000 {
                let input = game.pilot(tick);
                game.step(&input);
                if game.outcome() != Outcome::Playing {
                    break;
                }
            }
            assert_eq!(
                game.outcome(),
                Outcome::Won,
                "starter {starter}: {}",
                game.summary()
            );
        }
    }
    #[test]
    fn rejected_cards_and_invalid_inputs_cannot_spend_or_reward() {
        let mut g = battle();
        hand(&mut g, &[Kind::Crush, Kind::Wound, Kind::Gambit]);
        g.energy = 1;
        g.hero.hp = 3;
        let before = (
            g.hero,
            g.enemy,
            g.energy,
            g.score,
            g.coins,
            g.hand.clone(),
            g.discard.clone(),
        );
        for index in [0, 1, 2, 100] {
            assert_eq!(g.play(index), Cue::None);
        }
        for input in [
            choice(0),
            choice(usize::MAX),
            Input {
                click: true,
                pointer: vec2(f32::NAN, f32::INFINITY),
                ..Input::default()
            },
        ] {
            assert_eq!(g.step(&input), Cue::None);
        }
        assert_eq!(
            before,
            (
                g.hero,
                g.enemy,
                g.energy,
                g.score,
                g.coins,
                g.hand.clone(),
                g.discard.clone()
            )
        );
    }
    #[test]
    fn held_buttons_do_not_repeat_card_play_or_selection() {
        let mut g = battle();
        let cards = g.hand.clone();
        let selected = g.selected;
        for _ in 0..120 {
            assert_eq!(
                g.step(&Input {
                    primary_down: true,
                    right: true,
                    ..Input::default()
                }),
                Cue::None
            );
        }
        assert_eq!(g.hand, cards);
        assert_eq!(g.selected, selected);
        assert_eq!(g.turn, 1);
        assert_eq!(g.hero.hp, 80);
    }
    #[test]
    fn reshuffle_preserves_cards_and_exhaust_is_not_redrawn() {
        let mut g = battle();
        g.draw.clear();
        g.hand.clear();
        g.discard = vec![Card::new(Kind::Strike), Card::new(Kind::Guard)];
        g.exhausted = vec![Card::new(Kind::Insight)];
        let mut replay = g.clone();
        g.draw_cards(20);
        replay.draw_cards(20);
        assert_eq!(g, replay);
        assert_eq!(g.hand.len(), 2);
        assert_eq!(g.exhausted.len(), 1);
        assert!(g.hand.iter().all(|c| c.kind != Kind::Insight));
        g.hand.clear();
        g.discard = (0..10).map(|_| Card::new(Kind::Guard)).collect();
        g.draw_cards(20);
        assert_eq!(g.hand.len(), 7);
        assert_eq!(g.draw.len(), 3);
    }
    #[test]
    fn poison_bypasses_armor_and_block_and_can_cancel_intent() {
        let mut g = battle();
        g.floor = 2;
        g.enemy.hp = 4;
        g.enemy.block = 80;
        g.enemy.poison = 4;
        g.hero.hp = 10;
        assert_eq!(g.end_turn(), Cue::Good);
        assert_eq!(g.phase, Phase::Reward);
        assert_eq!(g.enemy.hp, 0);
        assert_eq!(g.hero.hp, 14);
        assert_eq!(g.enemy.block, 80);
        assert_eq!(g.enemy.poison, 3);
    }
    #[test]
    fn multi_hit_applies_strength_weak_vulnerable_and_armor_per_hit() {
        let mut g = battle();
        g.floor = 2;
        g.hero.strength = 2;
        g.hero.weak = 1;
        g.enemy.vulnerable = 1;
        g.enemy.block = 2;
        g.enemy.hp = 30;
        hand(&mut g, &[Kind::Flurry]);
        g.play(0); // (3+2)*3/4*3/2=4; armor2 on each hit; first hit blocked.
        assert_eq!(g.enemy.hp, 26);
        assert_eq!(g.enemy.block, 0);
        assert_eq!(g.energy, 2);
    }
    #[test]
    fn riposte_weakens_the_next_intent_and_block_expires_after_it() {
        let mut g = battle();
        hand(&mut g, &[Kind::Riposte]);
        g.play(0);
        assert_eq!(g.enemy.weak, 1);
        assert_eq!(g.incoming(), 4);
        g.end_turn();
        assert_eq!(g.hero.hp, 80);
        assert_eq!(g.hero.block, 0);
        assert_eq!(g.enemy.weak, 0);
        assert_eq!(g.turn, 2);
    }
    #[test]
    fn cleanse_removes_all_negative_statuses_and_poison_timing_is_explicit() {
        let mut g = battle();
        g.floor = 3;
        g.turn = 3;
        g.hero.block = 100;
        g.end_turn();
        assert_eq!(g.hero.hp, 77);
        assert_eq!(g.hero.poison, 2);
        g.hero.weak = 2;
        g.hero.vulnerable = 1;
        hand(&mut g, &[Kind::Ward]);
        g.play(0);
        assert_eq!((g.hero.poison, g.hero.weak, g.hero.vulnerable), (0, 0, 0));
        assert_eq!(g.hero.block, 5);
    }
    #[test]
    fn energy_and_draw_cards_exhaust_and_healing_is_capped() {
        let mut g = battle();
        hand(&mut g, &[Kind::Spark, Kind::Insight, Kind::Purify]);
        g.energy = 0;
        g.play(0);
        assert_eq!(g.energy, 1);
        assert_eq!(g.exhausted.len(), 1);
        g.play(0);
        assert_eq!(g.hand.len(), 3);
        assert_eq!(g.exhausted.len(), 2);
        g.play(0);
        assert_eq!(g.hero.hp, 80);
        assert_eq!(g.exhausted.len(), 3);
        assert!(g.energy <= 10);
    }
    #[test]
    fn thorns_only_triggers_after_health_damage_and_death_has_priority() {
        let mut g = battle();
        g.hero.thorns = 10;
        g.hero.block = 6;
        g.enemy.hp = 20;
        g.end_turn();
        assert_eq!(g.enemy.hp, 20);
        g.turn = 1;
        g.hero.hp = 2;
        g.enemy.hp = 2;
        g.end_turn();
        assert_eq!(g.phase, Phase::Lost);
        assert_eq!(g.hero.hp, 0);
        assert_eq!(g.enemy.hp, 0);
        assert_eq!(g.score, 0);
        let frozen = g.clone();
        g.step(&choice(1));
        assert_eq!(g, frozen);
    }
    #[test]
    fn rewards_are_granted_once_and_skip_keeps_the_deck_small() {
        let mut g = battle();
        g.enemy.hp = 1;
        hand(&mut g, &[Kind::Strike]);
        g.play(0);
        let score = g.score;
        let coins = g.coins;
        let size = g.deck.len();
        g.step(&choice(1));
        assert_eq!(g.deck.len(), size + 1);
        assert_eq!(g.phase, Phase::Camp);
        for _ in 0..5 {
            g.step(&choice(1));
        }
        assert_eq!(g.deck.len(), size + 1);
        assert_eq!((g.score, g.coins), (score, coins));
        let mut skip = battle();
        skip.phase = Phase::Reward;
        let size = skip.deck.len();
        skip.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(skip.deck.len(), size);
        assert_eq!(skip.phase, Phase::Camp);
    }
    #[test]
    fn camp_shop_and_forge_spend_one_action_atomically() {
        let mut g = battle();
        g.phase = Phase::Camp;
        g.hero.hp = 30;
        g.step(&choice(1));
        assert_eq!(g.hero.hp, 48);
        assert!(g.camp_used);
        g.step(&choice(1));
        g.step(&choice(2));
        g.step(&choice(3));
        assert_eq!(g.hero.hp, 48);
        assert_eq!(g.phase, Phase::Camp);
        let mut shop = battle();
        shop.phase = Phase::Shop;
        shop.coins = 34;
        let size = shop.deck.len();
        assert_eq!(shop.step(&choice(1)), Cue::None);
        assert_eq!(shop.coins, 34);
        assert_eq!(shop.deck.len(), size);
        assert!(!shop.camp_used);
        shop.coins = 35;
        shop.step(&choice(1));
        assert_eq!(shop.coins, 0);
        assert_eq!(shop.deck.len(), size + 1);
        assert!(shop.camp_used);
        let mut forge = battle();
        forge.phase = Phase::Forge;
        forge.step(&Input {
            primary: true,
            ..Input::default()
        });
        assert!(forge.deck[0].upgraded && forge.camp_used);
        forge.step(&choice(4));
        assert!(forge.draw.iter().chain(&forge.hand).any(|c| c.upgraded));
    }
    #[test]
    fn relics_and_shop_trimming_cannot_be_farmed() {
        let mut g = battle();
        g.phase = Phase::Relic;
        g.step(&choice(1));
        assert_eq!(g.hero.max_hp, 96);
        let hp = g.hero.hp;
        g.phase = Phase::Relic;
        assert_eq!(g.step(&choice(1)), Cue::None);
        assert_eq!(g.hero.max_hp, 96);
        assert_eq!(g.hero.hp, hp);
        g.phase = Phase::Shop;
        g.coins = 20;
        let n = g.deck.len();
        g.step(&choice(3));
        assert_eq!(g.deck.len(), n - 1);
        assert_eq!(g.coins, 0);
        assert!(g.camp_used);
    }
    #[test]
    fn boss_enrages_once_and_turn_limit_prevents_stalling() {
        let mut g = battle();
        g.floor = 8;
        g.enemy.hp = 65;
        g.enemy.max_hp = 136;
        g.check_end();
        assert_eq!(g.enemy.strength, 3);
        g.check_end();
        assert_eq!(g.enemy.strength, 3);
        g.floor = 0;
        g.turn = 40;
        g.hero.block = 200;
        g.enemy.hp = 200;
        g.end_turn();
        assert_eq!(g.outcome(), Outcome::Lost);
    }
    #[test]
    fn pointer_hitboxes_and_deck_inspection_match_the_controls() {
        let mut g = battle();
        let (x, w) = g.card_bounds(1);
        g.step(&Input {
            click: true,
            pointer: vec2(x + w / 2., 430.),
            ..Input::default()
        });
        assert_eq!(g.selected, 1);
        hand(&mut g, &[Kind::Strike]);
        let hp = g.enemy.hp;
        g.step(&Input {
            click: true,
            pointer: vec2(650., 350.),
            ..Input::default()
        });
        assert!(g.enemy.hp < hp);
        let before = (g.hero, g.enemy, g.energy, g.hand.clone(), g.turn);
        g.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert!(g.deck_open);
        for _ in 0..12 {
            g.step(&Input {
                right_pressed: true,
                ..Input::default()
            });
            assert!(g.deck_page < g.deck.len().div_ceil(8));
        }
        g.step(&Input {
            primary: true,
            ..Input::default()
        });
        assert!(!g.deck_open);
        assert_eq!(before, (g.hero, g.enemy, g.energy, g.hand.clone(), g.turn));
    }
    #[test]
    fn wounds_obey_the_battle_bound_and_vanish_next_battle() {
        let mut g = battle();
        g.floor = 3;
        g.end_turn();
        assert!(
            g.draw
                .iter()
                .chain(&g.hand)
                .chain(&g.discard)
                .any(|c| c.kind == Kind::Wound)
        );
        g.draw.clear();
        g.hand.clear();
        g.exhausted.clear();
        g.discard = vec![Card::new(Kind::Guard); MAX_BATTLE];
        g.turn = 1;
        g.end_turn();
        assert_eq!(g.battle_cards(), MAX_BATTLE);
        g.start_battle();
        assert_eq!(g.battle_cards(), g.deck.len());
        assert!(g.draw.iter().chain(&g.hand).all(|c| c.kind != Kind::Wound));
    }
    #[test]
    fn gilded_effects_improve_and_mirror_edge_spends_all_block() {
        let mut plain = battle();
        hand(&mut plain, &[Kind::Strike]);
        let mut gilded = plain.clone();
        gilded.hand[0].upgraded = true;
        plain.play(0);
        gilded.play(0);
        assert_eq!(plain.enemy.hp - gilded.enemy.hp, 3);
        assert_eq!(plain.energy, gilded.energy);
        hand(&mut plain, &[Kind::Reversal]);
        plain.hero.block = 10;
        let hp = plain.enemy.hp;
        plain.play(0);
        assert_eq!(plain.hero.block, 0);
        assert_eq!(hp - plain.enemy.hp, 15);
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Fighter {
    pub hp: i16,
    pub max_hp: i16,
    pub block: i16,
    pub strength: i16,
    pub poison: u8,
    pub weak: u8,
    pub vulnerable: u8,
    pub thorns: i16,
}
/// A complete nine-encounter expedition. Mutate it only through [`Game::step`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    pub(crate) phase: Phase,
    pub(crate) floor: usize,
    pub(crate) hero: Fighter,
    pub(crate) enemy: Fighter,
    pub(crate) energy: u8,
    pub(crate) turn: u16,
    pub(crate) deck: Vec<Card>,
    pub(crate) draw: Vec<Card>,
    pub(crate) hand: Vec<Card>,
    pub(crate) discard: Vec<Card>,
    pub(crate) exhausted: Vec<Card>,
    pub(crate) relics: [bool; 6],
    pub(crate) selected: usize,
    pub(crate) coins: u16,
    pub(crate) score: u32,
    pub(crate) camp_used: bool,
    pub(crate) deck_open: bool,
    pub(crate) deck_page: usize,
    pub(crate) notice: String,
    pub(crate) previous: String,
    pub(crate) animation: u32,
    pub(crate) won_turns: [u16; 9],
    shuffle: u32,
    enraged: bool,
}
impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}
impl Game {
    /// Start with ten cards, 80 health and a choice of three relics.
    pub fn new() -> Self {
        let mut deck = Vec::with_capacity(MAX_DECK);
        for kind in [
            Kind::Strike,
            Kind::Guard,
            Kind::Strike,
            Kind::Guard,
            Kind::Riposte,
            Kind::Venom,
            Kind::Strike,
            Kind::Guard,
            Kind::Insight,
            Kind::Crush,
        ] {
            deck.push(Card::new(kind));
        }
        Self {
            phase: Phase::Oath,
            floor: 0,
            hero: Fighter {
                hp: 80,
                max_hp: 80,
                ..Fighter::default()
            },
            enemy: Fighter::default(),
            energy: 0,
            turn: 0,
            deck,
            draw: Vec::with_capacity(MAX_BATTLE),
            hand: Vec::with_capacity(MAX_HAND),
            discard: Vec::with_capacity(MAX_BATTLE),
            exhausted: Vec::with_capacity(MAX_BATTLE),
            relics: [false; 6],
            selected: 0,
            coins: 30,
            score: 0,
            camp_used: false,
            deck_open: false,
            deck_page: 0,
            notice: "Choose the heirloom you carried into the palace.".into(),
            previous: String::new(),
            animation: 0,
            won_turns: [0; 9],
            shuffle: 0,
            enraged: false,
        }
    }
    /// Current encounter, one through nine.
    pub fn encounter(&self) -> usize {
        self.floor + 1
    }
    /// Remaining player health; zero ends the run.
    pub fn health(&self) -> i16 {
        self.hero.hp
    }
    /// Score awarded only by won encounters and final survival.
    pub fn score(&self) -> u32 {
        self.score
    }
    /// Current terminal state; all terminal inputs are ignored.
    pub fn outcome(&self) -> Outcome {
        match self.phase {
            Phase::Won => Outcome::Won,
            Phase::Lost => Outcome::Lost,
            _ => Outcome::Playing,
        }
    }
    pub(crate) fn content(&self) -> &'static Encounter {
        &ENCOUNTERS[self.floor]
    }
    fn say(&mut self, text: impl Into<String>) {
        self.previous = std::mem::replace(&mut self.notice, text.into());
    }
    fn heal(&mut self, amount: i16) {
        self.hero.hp = (self.hero.hp + amount).min(self.hero.max_hp);
    }
    fn acquire_relic(&mut self, id: usize) {
        self.relics[id] = true;
        if id == 3 {
            self.hero.max_hp += 16;
            self.heal(16);
        }
        self.say(format!("{} joins your cause.", RELIC_NAMES[id]));
    }
    fn shuffle_draw(&mut self) {
        let mut rng = RewardRng::new(
            0x6769_6c64_6564_u64 + (self.floor as u64) * 101 + u64::from(self.shuffle) * 997,
        );
        for i in (1..self.draw.len()).rev() {
            let j = (rng.next_u64() % (i as u64 + 1)) as usize;
            self.draw.swap(i, j);
        }
        self.shuffle += 1;
    }
    fn draw_cards(&mut self, amount: usize) {
        for _ in 0..amount {
            if self.hand.len() >= MAX_HAND {
                break;
            }
            if self.draw.is_empty() {
                if self.discard.is_empty() {
                    break;
                }
                self.draw.append(&mut self.discard);
                self.shuffle_draw();
            }
            if let Some(card) = self.draw.pop() {
                self.hand.push(card);
            }
        }
    }
    fn start_battle(&mut self) {
        let hp = self.hero.hp;
        let max_hp = self.hero.max_hp;
        self.hero = Fighter {
            hp,
            max_hp,
            strength: i16::from(self.relics[0]),
            thorns: if self.relics[5] { 3 } else { 0 },
            ..Fighter::default()
        };
        self.enemy = Fighter {
            hp: self.content().hp,
            max_hp: self.content().hp,
            ..Fighter::default()
        };
        self.phase = Phase::Battle;
        self.turn = 1;
        self.shuffle = 0;
        self.enraged = false;
        self.draw.clear();
        self.hand.clear();
        self.discard.clear();
        self.exhausted.clear();
        self.draw.extend_from_slice(&self.deck);
        self.shuffle_draw();
        self.energy = if self.relics[4] { 4 } else { 3 };
        self.draw_cards(5 + usize::from(self.relics[1]));
        self.selected = 0;
        self.say(format!(
            "{}: read the intent, then play your hand.",
            self.content().name
        ));
    }
    pub(crate) fn intent(&self) -> Intent {
        self.content().intents
            [usize::from(self.turn.saturating_sub(1)) % self.content().intents.len()]
    }
    pub(crate) fn damage_value(base: i16, strength: i16, weak: u8, vulnerable: u8) -> i16 {
        let mut value = (base + strength).max(0);
        if weak > 0 {
            value = value * 3 / 4;
        }
        if vulnerable > 0 {
            value = value * 3 / 2;
        }
        value
    }
    pub(crate) fn incoming(&self) -> i16 {
        let i = self.intent();
        Self::damage_value(
            i.damage,
            self.enemy.strength,
            self.enemy.weak,
            self.hero.vulnerable,
        ) * i16::from(i.hits)
    }
    fn strike(&mut self, base: i16) {
        let damage = (Self::damage_value(
            base,
            self.hero.strength,
            self.hero.weak,
            self.enemy.vulnerable,
        ) - self.content().armor)
            .max(0);
        let absorbed = damage.min(self.enemy.block);
        self.enemy.block -= absorbed;
        self.enemy.hp = (self.enemy.hp - (damage - absorbed)).max(0);
    }
    fn victory(&mut self) -> Cue {
        self.won_turns[self.floor] = self.turn;
        self.coins = self.coins.saturating_add(22 + self.floor as u16 * 5);
        self.score += 1000
            + self.floor as u32 * 150
            + u32::from(600u16.saturating_sub(self.turn.saturating_mul(35)));
        self.heal(4 + if self.relics[2] { 6 } else { 0 });
        self.selected = 0;
        if self.floor == 8 {
            self.score += u32::try_from(self.hero.hp).unwrap_or(0) * 30;
            self.phase = Phase::Won;
            self.say("The Regent's contract tears. Every stolen name returns home.");
            Cue::Win
        } else {
            self.phase = Phase::Reward;
            self.say("A ledger is freed. Choose one card, or pass with X.");
            Cue::Good
        }
    }
    fn check_end(&mut self) -> Option<Cue> {
        if self.hero.hp <= 0 {
            self.hero.hp = 0;
            self.phase = Phase::Lost;
            self.say("Your name enters the ledger. R begins a new expedition.");
            return Some(Cue::Bad);
        }
        if self.enemy.hp <= 0 {
            return Some(self.victory());
        }
        if self.floor == 8 && !self.enraged && self.enemy.hp <= self.enemy.max_hp / 2 {
            self.enraged = true;
            self.enemy.strength += 3;
        }
        None
    }
    pub(crate) fn playable(&self, index: usize) -> bool {
        self.hand.get(index).is_some_and(|c| {
            c.kind != Kind::Wound
                && c.cost() <= self.energy
                && (c.kind != Kind::Gambit || self.hero.hp > c.amount(3, 2))
        })
    }
    fn play(&mut self, index: usize) -> Cue {
        if !self.playable(index) {
            self.say("Cannot play: check energy, HP and the selected card.");
            return Cue::None;
        }
        let card = self.hand.remove(index);
        self.energy -= card.cost();
        match card.kind {
            Kind::Strike => self.strike(card.amount(7, 10)),
            Kind::Guard => self.hero.block = (self.hero.block + card.amount(6, 9)).min(200),
            Kind::Riposte => {
                self.hero.block = (self.hero.block + card.amount(5, 7)).min(200);
                self.strike(card.amount(4, 6));
                self.enemy.weak = self.enemy.weak.saturating_add(1).min(9);
            }
            Kind::Venom => {
                self.enemy.poison = self
                    .enemy
                    .poison
                    .saturating_add(card.amount(4, 6) as u8)
                    .min(99)
            }
            Kind::Insight => self.draw_cards(card.amount(2, 3) as usize),
            Kind::Crush => {
                self.strike(card.amount(16, 21));
                self.enemy.vulnerable = self.enemy.vulnerable.saturating_add(2).min(9);
            }
            Kind::Siphon => {
                self.strike(card.amount(5, 8));
                self.heal(card.amount(3, 5));
            }
            Kind::Flurry => {
                for _ in 0..3 {
                    self.strike(card.amount(3, 4));
                }
            }
            Kind::Fortify => {
                self.hero.block = (self.hero.block + card.amount(14, 19)).min(200);
                self.hero.thorns = (self.hero.thorns + card.amount(2, 3)).min(30);
            }
            Kind::Spark => self.energy = (self.energy + card.amount(1, 2) as u8).min(10),
            Kind::Focus => self.hero.strength = (self.hero.strength + card.amount(2, 3)).min(30),
            Kind::Gambit => {
                self.hero.hp -= card.amount(3, 2);
                self.strike(card.amount(9, 13));
            }
            Kind::Ward => {
                self.hero.block = (self.hero.block + card.amount(5, 8)).min(200);
                self.hero.poison = 0;
                self.hero.weak = 0;
                self.hero.vulnerable = 0;
            }
            Kind::Reversal => {
                let damage = self.hero.block + card.amount(4, 8);
                self.hero.block = 0;
                self.strike(damage);
            }
            Kind::Purify => self.heal(card.amount(4, 7)),
            Kind::Finale => self.strike(card.amount(24, 31) + self.exhausted.len() as i16 * 3),
            Kind::Wound => unreachable!("unplayable card rejected"),
        }
        if card.exhausts() {
            self.exhausted.push(card);
        } else {
            self.discard.push(card);
        }
        self.selected = self.selected.min(self.hand.len().saturating_sub(1));
        self.say(format!(
            "Played {}{}. {} energy remains.",
            card.name(),
            if card.upgraded { "+" } else { "" },
            self.energy
        ));
        self.check_end().unwrap_or(Cue::Good)
    }
    fn end_turn(&mut self) -> Cue {
        self.enemy.hp = (self.enemy.hp - i16::from(self.enemy.poison)).max(0);
        self.enemy.poison = self.enemy.poison.saturating_sub(1);
        if let Some(cue) = self.check_end() {
            return cue;
        }
        self.discard.append(&mut self.hand);
        self.hero.weak = self.hero.weak.saturating_sub(1);
        let intent = self.intent();
        self.enemy.block = intent.block;
        let mut lost = 0;
        for _ in 0..intent.hits {
            let damage = Self::damage_value(
                intent.damage,
                self.enemy.strength,
                self.enemy.weak,
                self.hero.vulnerable,
            );
            let absorbed = damage.min(self.hero.block);
            self.hero.block -= absorbed;
            let hit = damage - absorbed;
            lost += hit;
            self.hero.hp = (self.hero.hp - hit).max(0);
            if hit > 0 {
                self.enemy.hp = (self.enemy.hp - self.hero.thorns).max(0);
            }
            if self.hero.hp == 0 || self.enemy.hp == 0 {
                break;
            }
        }
        self.enemy.weak = self.enemy.weak.saturating_sub(1);
        self.enemy.vulnerable = self.enemy.vulnerable.saturating_sub(1);
        self.hero.vulnerable = self.hero.vulnerable.saturating_sub(1);
        if let Some(cue) = self.check_end() {
            return cue;
        }
        self.enemy.strength = (self.enemy.strength + intent.strength).min(30);
        self.hero.weak = self.hero.weak.saturating_add(intent.weak).min(9);
        self.hero.vulnerable = self
            .hero
            .vulnerable
            .saturating_add(intent.vulnerable)
            .min(9);
        self.hero.poison = self.hero.poison.saturating_add(intent.poison).min(99);
        if intent.wound && self.battle_cards() < MAX_BATTLE {
            self.discard.push(Card::new(Kind::Wound));
        }
        let poison = i16::from(self.hero.poison);
        self.hero.hp = (self.hero.hp - poison).max(0);
        self.hero.poison = self.hero.poison.saturating_sub(1);
        if let Some(cue) = self.check_end() {
            return cue;
        }
        self.hero.block = 0;
        self.energy = if self.relics[4] { 4 } else { 3 };
        self.turn = self.turn.saturating_add(1);
        // A bounded fight cannot be stalled indefinitely for healing or score.
        if self.turn > 40 {
            self.hero.hp = 0;
            self.phase = Phase::Lost;
            self.say("The palace seals at turn 40. Your expedition ends.");
            return Cue::Bad;
        }
        self.draw_cards(5 + usize::from(self.relics[1]));
        self.selected = 0;
        self.say(format!(
            "Enemy dealt {} HP; poison dealt {}. New hand, turn {}.",
            lost, poison, self.turn
        ));
        if lost + poison > 0 {
            Cue::Bad
        } else {
            Cue::Action
        }
    }
    fn battle_cards(&self) -> usize {
        self.draw.len() + self.hand.len() + self.discard.len() + self.exhausted.len()
    }
    fn after_reward(&mut self) {
        self.selected = 0;
        self.camp_used = false;
        self.phase = if self.content().elite {
            Phase::Relic
        } else {
            Phase::Camp
        };
    }
    fn choose(&mut self, index: usize) -> Cue {
        match self.phase {
            Phase::Oath if index < 3 => {
                self.acquire_relic(index);
                self.start_battle();
                Cue::Good
            }
            Phase::Reward if index < 3 && self.deck.len() < MAX_DECK => {
                let card = Card::new(REWARDS[self.floor][index]);
                self.deck.push(card);
                self.say(format!("Added {} to your deck.", card.name()));
                self.after_reward();
                Cue::Good
            }
            Phase::Relic if index < 3 && !self.relics[index + 3] => {
                self.acquire_relic(index + 3);
                self.phase = Phase::Camp;
                self.selected = 0;
                Cue::Good
            }
            Phase::Camp => match index {
                0 if !self.camp_used && self.hero.hp < self.hero.max_hp => {
                    self.heal(18);
                    self.camp_used = true;
                    self.say("Restored 18 HP. Your camp action is spent.");
                    Cue::Good
                }
                1 if !self.camp_used => {
                    self.phase = Phase::Forge;
                    self.selected = 0;
                    Cue::Action
                }
                2 if !self.camp_used => {
                    self.phase = Phase::Shop;
                    self.selected = 0;
                    Cue::Action
                }
                3 => {
                    self.floor += 1;
                    self.start_battle();
                    Cue::Good
                }
                _ => Cue::None,
            },
            Phase::Forge if index < self.deck.len() && !self.deck[index].upgraded => {
                self.deck[index].upgraded = true;
                self.camp_used = true;
                self.phase = Phase::Camp;
                self.selected = 3;
                self.say("A card is gilded. Its improved effect lasts for the run.");
                Cue::Good
            }
            Phase::Shop if !self.camp_used => {
                let accepted = match index {
                    0 if self.coins >= 35 && self.deck.len() < MAX_DECK => {
                        self.coins -= 35;
                        self.deck.push(Card::new(self.shop_card()));
                        self.say("Bought the merchant's card.");
                        true
                    }
                    1 if self.coins >= 25 && self.hero.hp < self.hero.max_hp => {
                        self.coins -= 25;
                        self.heal(28);
                        self.say("Bought a tonic: healed 28 HP.");
                        true
                    }
                    2 if self.coins >= 20 && self.deck.len() > 6 => {
                        if let Some(at) = self
                            .deck
                            .iter()
                            .position(|c| c.kind == Kind::Strike && !c.upgraded)
                        {
                            self.deck.remove(at);
                            self.coins -= 20;
                            self.say("Removed one ungilded Gilt Strike.");
                            true
                        } else {
                            false
                        }
                    }
                    3 => {
                        self.phase = Phase::Camp;
                        self.selected = 0;
                        return Cue::Action;
                    }
                    _ => false,
                };
                if accepted {
                    self.camp_used = true;
                    self.phase = Phase::Camp;
                    self.selected = 3;
                    Cue::Good
                } else {
                    Cue::None
                }
            }
            _ => Cue::None,
        }
    }
    pub(crate) fn shop_card(&self) -> Kind {
        [
            Kind::Spark,
            Kind::Siphon,
            Kind::Fortify,
            Kind::Ward,
            Kind::Focus,
            Kind::Finale,
            Kind::Venom,
            Kind::Purify,
        ][self.floor.min(7)]
    }
    fn options(&self) -> usize {
        match self.phase {
            Phase::Oath | Phase::Reward | Phase::Relic => 3,
            Phase::Camp | Phase::Shop => 4,
            Phase::Forge => self.deck.len(),
            Phase::Battle => self.hand.len(),
            _ => 0,
        }
    }
    /// Execute one 60 Hz command. Planning has no real-time deadline; held keys do not repeat card plays.
    pub fn step(&mut self, input: &Input) -> Cue {
        if self.outcome() != Outcome::Playing {
            return Cue::None;
        }
        self.animation = self.animation.wrapping_add(1);
        if input.tertiary {
            self.deck_open = !self.deck_open;
            self.deck_page = 0;
            return Cue::Action;
        }
        if self.deck_open {
            let pages = self.deck.len().div_ceil(8).max(1);
            if input.primary || input.secondary {
                self.deck_open = false;
                return Cue::Action;
            }
            if input.right_pressed || input.down_pressed {
                self.deck_page = (self.deck_page + 1) % pages;
                return Cue::Action;
            }
            if input.left_pressed || input.up_pressed {
                self.deck_page = (self.deck_page + pages - 1) % pages;
                return Cue::Action;
            }
            return Cue::None;
        }
        if input.secondary {
            return match self.phase {
                Phase::Battle => self.end_turn(),
                Phase::Reward => {
                    self.say("Passed the card reward. A lean deck is a choice.");
                    self.after_reward();
                    Cue::Action
                }
                Phase::Forge | Phase::Shop => {
                    self.phase = Phase::Camp;
                    self.selected = 0;
                    Cue::Action
                }
                _ => Cue::None,
            };
        }
        if input.click {
            let p = input.pointer;
            if self.phase == Phase::Battle {
                if p.y >= 398. && p.y < 558. {
                    for i in 0..self.hand.len() {
                        let (x, w) = self.card_bounds(i);
                        if p.x >= x && p.x < x + w {
                            self.selected = i;
                            return Cue::Action;
                        }
                    }
                }
                if p.y >= 340. && p.y < 377. {
                    if p.x >= 620. && p.x < 766. {
                        return self.play(self.selected);
                    }
                    if p.x >= 778. && p.x < 928. {
                        return self.end_turn();
                    }
                }
            } else if self.phase == Phase::Forge {
                if p.x >= 40. && p.x < 600. && p.y >= 162. && p.y < 482. {
                    let id = ((p.y - 162.) / 40.) as usize + (self.selected / 8) * 8;
                    if id < self.deck.len() {
                        self.selected = id;
                        return Cue::Action;
                    }
                }
                if p.x >= 630. && p.x < 920. && p.y >= 472. && p.y < 522. {
                    return self.choose(self.selected);
                }
            } else if p.x >= 480.
                && p.x < 924.
                && p.y >= 190.
                && p.y < 190. + self.options() as f32 * 77.
            {
                return self.choose(((p.y - 190.) / 77.) as usize);
            }
        }
        let count = self.options();
        if count == 0 {
            return Cue::None;
        }
        if let Some(slot) = input.slot.filter(|s| *s >= 1 && *s <= 5)
            && slot <= count
        {
            self.selected = slot - 1;
            if self.phase != Phase::Battle && self.phase != Phase::Forge {
                return self.choose(self.selected);
            }
            return Cue::Action;
        }
        if input.left_pressed || input.up_pressed {
            self.selected = (self.selected + count - 1) % count;
            return Cue::Action;
        }
        if input.right_pressed || input.down_pressed {
            self.selected = (self.selected + 1) % count;
            return Cue::Action;
        }
        if input.primary {
            if self.phase == Phase::Battle {
                self.play(self.selected)
            } else {
                self.choose(self.selected)
            }
        } else {
            Cue::None
        }
    }
    pub(crate) fn card_bounds(&self, index: usize) -> (f32, f32) {
        let width = (896. / self.hand.len().max(1) as f32 - 8.).min(166.);
        let total = (width + 8.) * self.hand.len() as f32 - 8.;
        ((960. - total) / 2. + index as f32 * (width + 8.), width)
    }
    /// Generate read-only legal player commands for a reproducible full run.
    pub fn pilot(&self, tick: u64) -> Input {
        if !tick.is_multiple_of(24) {
            return Input::default();
        }
        let choice = |i: usize| Input {
            slot: Some(i + 1),
            ..Input::default()
        };
        match self.phase {
            Phase::Oath => choice(1),
            Phase::Battle => {
                let mut best = None;
                for (i, card) in self
                    .hand
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| self.playable(*i))
                {
                    let need = (self.incoming() - self.hero.block).max(0);
                    let value = match card.kind {
                        Kind::Spark => 90,
                        Kind::Focus => 80,
                        Kind::Insight => {
                            if self.hand.len() < 6 {
                                70
                            } else {
                                5
                            }
                        }
                        Kind::Purify => {
                            if self.hero.hp <= self.hero.max_hp - 7 {
                                85
                            } else {
                                0
                            }
                        }
                        Kind::Siphon => {
                            if self.hero.hp < self.hero.max_hp - 3 {
                                65
                            } else {
                                20
                            }
                        }
                        Kind::Ward => {
                            if self.hero.poison + self.hero.weak + self.hero.vulnerable > 0 {
                                88
                            } else if need > 0 {
                                42
                            } else {
                                2
                            }
                        }
                        Kind::Venom => {
                            if self.enemy.hp > 12 {
                                62
                            } else {
                                10
                            }
                        }
                        Kind::Crush => 58,
                        Kind::Finale => 56,
                        Kind::Flurry => {
                            if self.content().armor < 2 {
                                46
                            } else {
                                15
                            }
                        }
                        Kind::Fortify => {
                            if need > 5 {
                                63
                            } else {
                                12
                            }
                        }
                        Kind::Guard => {
                            if need > 0 {
                                50
                            } else {
                                1
                            }
                        }
                        Kind::Riposte => {
                            if need > 0 {
                                57
                            } else {
                                32
                            }
                        }
                        Kind::Reversal => {
                            if self.hero.block > need + 5 {
                                60
                            } else {
                                0
                            }
                        }
                        Kind::Gambit => {
                            if self.hero.hp > 12 {
                                29
                            } else {
                                0
                            }
                        }
                        Kind::Strike => 35,
                        Kind::Wound => 0,
                    };
                    if value > 0 && best.is_none_or(|(_, v)| value > v) {
                        best = Some((i, value));
                    }
                }
                if let Some((i, _)) = best {
                    if i == self.selected {
                        Input {
                            primary: true,
                            ..Input::default()
                        }
                    } else if i < 5 {
                        choice(i)
                    } else {
                        Input {
                            right_pressed: true,
                            ..Input::default()
                        }
                    }
                } else {
                    Input {
                        secondary: true,
                        ..Input::default()
                    }
                }
            }
            Phase::Reward => choice([0, 0, 0, 0, 0, 1, 0, 0][self.floor]),
            Phase::Relic => choice(if !self.relics[4] { 1 } else { 0 }),
            Phase::Camp => {
                if self.camp_used {
                    choice(3)
                } else if self.hero.hp < self.hero.max_hp - 18 {
                    choice(0)
                } else if self.floor.is_multiple_of(2) && self.coins >= 35 {
                    choice(2)
                } else {
                    choice(1)
                }
            }
            Phase::Shop => choice(0),
            Phase::Forge => {
                let at = self
                    .deck
                    .iter()
                    .position(|c| {
                        !c.upgraded && matches!(c.kind, Kind::Crush | Kind::Venom | Kind::Focus)
                    })
                    .or_else(|| self.deck.iter().position(|c| !c.upgraded));
                match at {
                    Some(i) if i == self.selected => Input {
                        primary: true,
                        ..Input::default()
                    },
                    Some(_) => Input {
                        right_pressed: true,
                        ..Input::default()
                    },
                    None => Input {
                        secondary: true,
                        ..Input::default()
                    },
                }
            }
            _ => Input::default(),
        }
    }
    /// Narrative ending and score summary for the shared results screen.
    pub fn summary(&self) -> String {
        match self.outcome() {
            Outcome::Won => format!(
                "The ledgers burn. Nine captors are defeated and the city remembers its names. {} cards, {} relics, {} HP and {} coins remain. {} points. Your choices decide the deck that reaches the throne.",
                self.deck.len(),
                self.relics.iter().filter(|&&r| r).count(),
                self.hero.hp,
                self.coins,
                self.score
            ),
            Outcome::Lost => format!(
                "The {} stopped your expedition at encounter {} of 9. {} points. Read each intent, keep a lean deck and return with a new relic. R begins a fresh run.",
                self.content().name,
                self.encounter(),
                self.score
            ),
            Outcome::Playing => format!(
                "Encounter {} of 9: {}. {} HP, {} coins.",
                self.encounter(),
                self.content().name,
                self.hero.hp,
                self.coins
            ),
        }
    }
}
