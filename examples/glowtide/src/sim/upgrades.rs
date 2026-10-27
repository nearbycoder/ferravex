//! Level-up offers, rerolls, banishes, treasure and evolutions.
use super::content::*;
use super::{Event, Game, Mods, Phase, WeaponSlot};
use ferravex_play::gameplay::loot::RewardTable;

/// One upgrade card. Fallback cards appear when every pool item is maxed/banished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Card {
    Weapon(WeaponId),
    Passive(PassiveId),
    Snack,
    Pebble,
}

impl Card {
    fn pool_index(self) -> Option<usize> {
        match self {
            Card::Weapon(w) => Some(w as usize),
            Card::Passive(p) => Some(WEAPONS + p as usize),
            _ => None,
        }
    }
    fn from_pool(index: usize) -> Card {
        if index < WEAPONS {
            Card::Weapon(WeaponId::ALL[index])
        } else {
            Card::Passive(PassiveId::ALL[index - WEAPONS])
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Card::Weapon(w) => w.info().name,
            Card::Passive(p) => p.info().name,
            Card::Snack => "Seaweed Snack",
            Card::Pebble => "Shiny Pebble",
        }
    }
    pub fn flavor(self) -> &'static str {
        match self {
            Card::Weapon(w) => w.info().flavor,
            Card::Passive(p) => p.info().flavor,
            Card::Snack => "Crunchy. Salty. Restorative.",
            Card::Pebble => "It's shiny. That's the whole pitch.",
        }
    }
    pub fn color(self) -> Rgb {
        match self {
            Card::Weapon(w) => w.info().color,
            Card::Passive(p) => p.info().color,
            Card::Snack => [0.5, 1.0, 0.55],
            Card::Pebble => [1.0, 0.85, 0.45],
        }
    }
}

const POOL: usize = WEAPONS + PASSIVES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Offer {
    pub cards: [Card; 4],
    pub count: u8,
    /// Increments whenever the visible cards change, for UI animation.
    pub serial: u32,
}

impl Default for Offer {
    fn default() -> Self {
        Self {
            cards: [Card::Snack; 4],
            count: 0,
            serial: 0,
        }
    }
}

impl Offer {
    pub fn cards(&self) -> &[Card] {
        &self.cards[..usize::from(self.count)]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChestItem {
    Evolved(WeaponId),
    Level(Card, u8),
    Snack,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChestReveal {
    pub items: [ChestItem; 3],
    pub count: u8,
    pub glimmer: u32,
    pub serial: u32,
}

impl Default for ChestReveal {
    fn default() -> Self {
        Self {
            items: [ChestItem::Snack; 3],
            count: 0,
            glimmer: 0,
            serial: 0,
        }
    }
}

impl ChestReveal {
    pub fn items(&self) -> &[ChestItem] {
        &self.items[..usize::from(self.count)]
    }
}

pub(crate) fn base_mods() -> Mods {
    Mods {
        might: 1.,
        haste: 1.,
        area: 1.,
        amount: 0,
        crit: 0.05,
        crit_mult: 1.75,
        luck: 1.,
        magnet: 1.,
        move_speed: 1.,
        armor: 0.,
        regen: 0.,
        max_hp: 100.,
        dash_cd: 1.,
    }
}

impl Game {
    pub(crate) fn add_weapon(&mut self, id: WeaponId) -> bool {
        if self.weapon(id).is_some() {
            return false;
        }
        let Some(slot) = self.weapons.iter_mut().find(|s| s.is_none()) else {
            return false;
        };
        *slot = Some(WeaponSlot {
            id,
            level: 1,
            evolved: false,
            cooldown: 0.4,
            burst: 0,
            burst_timer: 0.,
            active: 0.,
            angle: 0.,
            side: 0,
        });
        true
    }

    /// Recompute all modifiers from character, Shrine and passive ranks.
    pub(crate) fn recompute_mods(&mut self) {
        let c = self.character();
        let b = self.config.bonuses;
        let mut m = base_mods();
        m.might = c.might * b.might;
        m.area = c.area;
        m.move_speed = c.speed * b.speed;
        m.magnet = b.magnet;
        m.max_hp = c.max_hp + b.max_hp;
        if self.config.character == 3 {
            m.armor += 1.;
        }
        for (id, level) in self.passives.iter().flatten() {
            let l = f32::from(*level);
            match id {
                PassiveId::MoonPearl => m.haste *= 1. - 0.08 * l,
                PassiveId::TidalSpiral => m.area *= 1. + 0.1 * l,
                PassiveId::PressureValve => m.might *= 1. + 0.1 * l,
                PassiveId::StingingCells => {
                    m.crit += 0.06 * l;
                    m.crit_mult += 0.15 * l;
                }
                PassiveId::EchoShell => m.amount += u32::from(level.div_ceil(2)),
                PassiveId::KelpTea => {
                    m.regen += 0.3 * l;
                    m.max_hp += 8. * l;
                }
                PassiveId::SwiftFins => {
                    m.move_speed *= 1. + 0.08 * l;
                    m.dash_cd *= 1. - 0.08 * l;
                }
                PassiveId::LuckyBarnacle => {
                    m.luck += 0.1 * l;
                    m.crit += 0.01 * l;
                }
                PassiveId::MagnetAnemone => m.magnet *= 1. + 0.3 * l,
                PassiveId::ThickShell => {
                    m.armor += l;
                    m.max_hp += 15. * l;
                }
            }
        }
        let grow = m.max_hp - self.mods.max_hp;
        self.mods = m;
        self.player.max_hp = m.max_hp;
        if grow > 0. && self.ticks > 0 {
            self.player.hp += grow;
        }
        self.player.hp = self.player.hp.min(self.player.max_hp);
    }

    fn card_level(&self, card: Card) -> u8 {
        match card {
            Card::Weapon(w) => self.weapon(w).map_or(0, |s| s.level),
            Card::Passive(p) => self.passive_level(p),
            _ => 0,
        }
    }

    /// Level this card would reach if picked.
    pub fn card_next_level(&self, card: Card) -> u8 {
        self.card_level(card) + 1
    }

    fn pool_weights(&self, owned_only: bool) -> [u32; POOL] {
        let weapon_room = self.weapon_count() < WEAPON_SLOTS;
        let passive_room = self.passives.iter().any(|p| p.is_none());
        let mut weights = [0; POOL];
        for (i, weight) in weights.iter_mut().enumerate() {
            if self.banished & (1 << i) != 0 {
                continue;
            }
            let card = Card::from_pool(i);
            let level = self.card_level(card);
            *weight = match card {
                Card::Weapon(_) => {
                    if level >= WEAPON_MAX {
                        0
                    } else if level > 0 {
                        12
                    } else if weapon_room && !owned_only {
                        8
                    } else {
                        0
                    }
                }
                Card::Passive(p) => {
                    let pairs = self
                        .weapons
                        .iter()
                        .flatten()
                        .any(|s| s.id.info().pair == p && !s.evolved);
                    if level >= PASSIVE_MAX {
                        0
                    } else if level > 0 {
                        8 + if pairs { 3 } else { 0 }
                    } else if passive_room && !owned_only {
                        6 + if pairs { 4 } else { 0 }
                    } else {
                        0
                    }
                }
                _ => 0,
            };
        }
        weights
    }

    pub(crate) fn roll_offer(&mut self) {
        let weights = self.pool_weights(false);
        let luck_roll = self.rand();
        let want = if luck_roll < (self.mods.luck - 1.) * 0.6 + 0.08 {
            4
        } else {
            3
        };
        let mut picks = [0usize; 4];
        let mut written = 0;
        if weights.iter().any(|w| *w > 0)
            && let Ok(table) = RewardTable::new(weights)
            && let Ok(report) = table.roll_distinct_into(self.rng(), want, &mut picks, 64)
        {
            written = report.written;
        }
        let mut cards = [Card::Snack; 4];
        for (slot, &pick) in cards.iter_mut().zip(&picks[..written]) {
            *slot = Card::from_pool(pick);
        }
        let mut count = written;
        if count == 0 {
            cards[0] = Card::Snack;
            cards[1] = Card::Pebble;
            count = 2;
        }
        self.offer = Offer {
            cards,
            count: count as u8,
            serial: self.offer.serial.wrapping_add(1),
        };
    }

    pub(crate) fn enter_level_up(&mut self) {
        self.phase = Phase::LevelUp;
        self.menu_ticks = 0;
        self.roll_offer();
        let level = self.level();
        self.emit(Event::LevelUp { level });
    }

    /// Apply a decision while a modal is open. Invalid/unaffordable choices are ignored.
    pub(crate) fn choose(&mut self, choice: Choice) {
        match (self.phase, choice) {
            (Phase::LevelUp, Choice::Pick(i)) if i < self.offer.count => {
                let card = self.offer.cards[usize::from(i)];
                self.apply_card(card);
                self.emit(Event::Picked { card });
                self.resume_after_modal();
            }
            (Phase::LevelUp, Choice::Reroll) if self.rerolls > 0 => {
                self.rerolls -= 1;
                self.roll_offer();
            }
            (Phase::LevelUp, Choice::Banish(i)) if i < self.offer.count && self.banishes > 0 => {
                if let Some(index) = self.offer.cards[usize::from(i)].pool_index() {
                    self.banishes -= 1;
                    self.banished |= 1 << index;
                    self.roll_offer();
                }
            }
            (Phase::Chest, Choice::Pick(_)) => self.resume_after_modal(),
            _ => {}
        }
    }

    fn resume_after_modal(&mut self) {
        if self.pending_levels > 0 {
            self.pending_levels -= 1;
            self.enter_level_up();
        } else {
            self.phase = Phase::Running;
            self.menu_ticks = 0;
        }
    }

    pub(crate) fn apply_card(&mut self, card: Card) {
        match card {
            Card::Weapon(w) => {
                if let Some(slot) = self.weapons.iter_mut().flatten().find(|s| s.id == w) {
                    slot.level = (slot.level + 1).min(WEAPON_MAX);
                } else {
                    self.add_weapon(w);
                }
            }
            Card::Passive(p) => {
                if let Some(entry) = self.passives.iter_mut().flatten().find(|(id, _)| *id == p) {
                    entry.1 = (entry.1 + 1).min(PASSIVE_MAX);
                } else if let Some(slot) = self.passives.iter_mut().find(|s| s.is_none()) {
                    *slot = Some((p, 1));
                }
                self.recompute_mods();
            }
            Card::Snack => {
                let amount = self.player.max_hp * 0.3;
                self.heal(amount);
            }
            Card::Pebble => {
                self.stats.coins += 25;
            }
        }
    }

    /// Weapons at max level whose paired passive is owned and not yet evolved.
    pub fn evolution_ready(&self) -> Option<WeaponId> {
        self.weapons.iter().flatten().find_map(|s| {
            (!s.evolved && s.level >= WEAPON_MAX && self.passive_level(s.id.info().pair) > 0)
                .then_some(s.id)
        })
    }

    pub(crate) fn open_chest(&mut self, tier: u8) {
        let mut reveal = ChestReveal {
            serial: self.chest.serial.wrapping_add(1),
            ..ChestReveal::default()
        };
        let mut slots = if tier >= 2 { 3 } else { 1 };
        if tier < 2 && self.rand() < 0.15 * self.mods.luck {
            slots += 1;
        }
        while usize::from(reveal.count) < slots {
            let item = if let Some(w) = self.evolution_ready() {
                if let Some(slot) = self.weapons.iter_mut().flatten().find(|s| s.id == w) {
                    slot.evolved = true;
                    slot.active = 0.;
                }
                self.emit(Event::Evolved { weapon: w });
                ChestItem::Evolved(w)
            } else {
                let weights = self.pool_weights(true);
                let mut pick = [0usize; 1];
                let rolled = weights.iter().any(|w| *w > 0)
                    && RewardTable::new(weights)
                        .ok()
                        .and_then(|t| t.roll_distinct_into(self.rng(), 1, &mut pick, 32).ok())
                        .is_some_and(|r| r.written == 1);
                if rolled {
                    let card = Card::from_pool(pick[0]);
                    self.apply_card(card);
                    ChestItem::Level(card, self.card_level(card))
                } else {
                    let amount = self.player.max_hp * 0.3;
                    self.heal(amount);
                    ChestItem::Snack
                }
            };
            reveal.items[usize::from(reveal.count)] = item;
            reveal.count += 1;
        }
        let glimmer = (self.rand_range(8., 18.)
            * f32::from(tier.max(1))
            * self.mods.luck
            * self.config.bonuses.greed) as u32;
        reveal.glimmer = glimmer;
        self.stats.coins += u64::from(glimmer);
        self.chest = reveal;
        self.phase = Phase::Chest;
        self.menu_ticks = 0;
    }
}

use super::Choice;
