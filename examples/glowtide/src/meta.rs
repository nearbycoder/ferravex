//! Meta progression: Glimmer, Shrine upgrades, character unlocks and titles.
//!
//! Saved through the engine's versioned [`SaveData`] documents in a `Slot`, so
//! corrupt or future files are protected rather than overwritten. Autoplay and
//! smoke runs use the platform's transient store and never touch real saves.
use crate::sim::content::*;
use crate::sim::{Bonuses, Game, Phase};
use ferravex_play::persistence::{Document, SaveData, SaveError, Value};

pub const OWNER: &str = "glowtide";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    pub glimmer: u64,
    pub lifetime: u64,
    /// Bit `i` set = character `i` unlocked. Lumi (bit 0) is always unlocked.
    pub unlocked: u8,
    pub shrine: [u8; SHRINE_ITEMS],
    pub character: u8,
    pub quick: bool,
    pub runs: u32,
    pub wins: u32,
}

impl Default for Meta {
    fn default() -> Self {
        Self {
            glimmer: 0,
            lifetime: 0,
            unlocked: 1,
            shrine: [0; SHRINE_ITEMS],
            character: 0,
            quick: false,
            runs: 0,
            wins: 0,
        }
    }
}

const SHRINE_KEYS: [&str; SHRINE_ITEMS] = [
    "shrine.might",
    "shrine.vitality",
    "shrine.swiftness",
    "shrine.reach",
    "shrine.reroll",
    "shrine.banish",
    "shrine.greed",
    "shrine.second_wind",
];

impl SaveData for Meta {
    const SCHEMA: u64 = 0x474c_4f57_5449_4445; // "GLOWTIDE"
    const VERSION: u32 = 1;
    const MAX_BYTES: usize = 4096;

    fn write(&self, d: &mut Document) -> Result<(), SaveError> {
        d.set("glimmer", Value::U64(self.glimmer))?;
        d.set("lifetime", Value::U64(self.lifetime))?;
        d.set("unlocked", Value::U64(u64::from(self.unlocked)))?;
        d.set("character", Value::U64(u64::from(self.character)))?;
        d.set("quick", Value::Bool(self.quick))?;
        d.set("runs", Value::U64(u64::from(self.runs)))?;
        d.set("wins", Value::U64(u64::from(self.wins)))?;
        for (key, rank) in SHRINE_KEYS.iter().zip(self.shrine) {
            d.set(key, Value::U64(u64::from(rank)))?;
        }
        Ok(())
    }

    fn read(d: &Document) -> Result<Self, SaveError> {
        let small = |key: &str, max: u64| -> Result<u64, SaveError> {
            let v = d.u64(key)?;
            if v > max {
                Err(SaveError::InvalidData)
            } else {
                Ok(v)
            }
        };
        let mut shrine = [0; SHRINE_ITEMS];
        for (i, key) in SHRINE_KEYS.iter().enumerate() {
            shrine[i] = small(key, u64::from(SHRINE[i].max))? as u8;
        }
        let character = small("character", CHARACTERS.len() as u64 - 1)? as u8;
        let unlocked = (small("unlocked", 0xff)? as u8) | 1;
        let meta = Self {
            glimmer: d.u64("glimmer")?,
            lifetime: d.u64("lifetime")?,
            unlocked,
            shrine,
            character: if unlocked & (1 << character) != 0 {
                character
            } else {
                0
            },
            quick: d.boolean("quick")?,
            runs: small("runs", u64::from(u32::MAX))? as u32,
            wins: small("wins", u64::from(u32::MAX))? as u32,
        };
        Ok(meta)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purchase {
    Bought,
    Maxed,
    TooPoor,
}

impl Meta {
    pub fn is_unlocked(&self, character: usize) -> bool {
        character < CHARACTERS.len() && self.unlocked & (1 << character) != 0
    }

    pub fn rank(&self, item: ShrineId) -> u8 {
        self.shrine[item as usize]
    }

    pub fn next_cost(&self, item: usize) -> Option<u64> {
        let rank = self.shrine[item];
        (rank < SHRINE[item].max).then(|| SHRINE[item].cost(rank))
    }

    pub fn buy(&mut self, item: usize) -> Purchase {
        let Some(cost) = self.next_cost(item) else {
            return Purchase::Maxed;
        };
        if self.glimmer < cost {
            return Purchase::TooPoor;
        }
        self.glimmer -= cost;
        self.shrine[item] += 1;
        Purchase::Bought
    }

    pub fn unlock(&mut self, character: usize) -> Purchase {
        if self.is_unlocked(character) {
            return Purchase::Maxed;
        }
        let cost = CHARACTERS[character].cost;
        if self.glimmer < cost {
            return Purchase::TooPoor;
        }
        self.glimmer -= cost;
        self.unlocked |= 1 << character;
        Purchase::Bought
    }

    /// Run bonuses granted by Shrine ranks.
    pub fn bonuses(&self) -> Bonuses {
        let r = |id: ShrineId| f32::from(self.rank(id));
        Bonuses {
            might: 1. + 0.05 * r(ShrineId::Might),
            max_hp: 10. * r(ShrineId::Vitality),
            speed: 1. + 0.04 * r(ShrineId::Swiftness),
            magnet: 1. + 0.15 * r(ShrineId::Reach),
            rerolls: 1 + self.rank(ShrineId::Reroll),
            banishes: 1 + self.rank(ShrineId::Banish),
            greed: 1. + 0.1 * r(ShrineId::Greed),
            revives: self.rank(ShrineId::SecondWind),
        }
    }

    pub fn award(&mut self, glimmer: u64, won: bool) {
        self.glimmer = self.glimmer.saturating_add(glimmer);
        self.lifetime = self.lifetime.saturating_add(glimmer);
        self.runs = self.runs.saturating_add(1);
        if won {
            self.wins = self.wins.saturating_add(1);
        }
    }
}

/// Glimmer earned by a finished run: pickups plus a performance purse.
pub fn run_glimmer(game: &Game) -> u64 {
    let s = &game.stats;
    let minutes = game.minute().min(10.);
    let mut purse = f64::from(s.kills) / 45. + f64::from(minutes) * 5. + f64::from(s.elites) * 4.;
    if s.brine_down {
        purse += 25.;
    }
    if game.phase == Phase::Won {
        purse += 60.;
    }
    let purse = (purse * f64::from(game.config.bonuses.greed)).round() as u64;
    s.coins + purse
}

/// A fun title for the run summary; first matching rule wins.
pub fn title(game: &Game) -> (&'static str, &'static str) {
    let s = &game.stats;
    let total = game.total_damage().max(1.);
    let share = |w: WeaponId| s.damage[w as usize] / total;
    let pick = |i: usize| TITLES[i];
    if game.phase == Phase::Won {
        return pick(0);
    }
    if s.gulpjaw_left.is_some_and(|left| left <= 0.5) {
        return pick(1);
    }
    if share(WeaponId::BubbleMortar) > 0.4 {
        return pick(2);
    }
    if share(WeaponId::HaloOrbit) > 0.4 {
        return pick(3);
    }
    if share(WeaponId::ChainSpark) > 0.4 {
        return pick(4);
    }
    if s.dashes > 60 {
        return pick(5);
    }
    if s.motes > 2500 {
        return pick(6);
    }
    if s.damage_taken < 40. && game.minute() > 3. {
        return pick(7);
    }
    if game.minute() < 2. {
        return pick(8);
    }
    if s.brine_down {
        return pick(9);
    }
    if game.minute() > 5. {
        return pick(10);
    }
    pick(11)
}

/// Personal-best record identities, per tide length.
pub fn record_id(quick: bool, metric: &str) -> &'static str {
    match (quick, metric) {
        (false, "survive") => "full-survive",
        (false, "kills") => "full-kills",
        (false, "level") => "full-level",
        (true, "survive") => "quick-survive",
        (true, "kills") => "quick-kills",
        _ => "quick-level",
    }
}

/// Bumped when rules change enough that old bests are not comparable.
pub const RECORD_RULES: u64 = 1;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::RunConfig;

    #[test]
    fn save_round_trips_and_rejects_out_of_range_ranks() {
        let mut meta = Meta {
            glimmer: 1234,
            lifetime: 5000,
            unlocked: 0b101,
            character: 2,
            quick: true,
            runs: 7,
            wins: 2,
            ..Meta::default()
        };
        meta.shrine[ShrineId::Might as usize] = 3;
        meta.shrine[ShrineId::SecondWind as usize] = 1;
        let bytes = meta.encode(4).unwrap();
        let (loaded, generation) = Meta::decode(&bytes).unwrap();
        assert_eq!(generation, 4);
        assert_eq!(loaded, meta);

        let mut d = Document::new(Meta::SCHEMA, Meta::VERSION, 1).unwrap();
        meta.write(&mut d).unwrap();
        d.set("shrine.might", Value::U64(99)).unwrap();
        assert_eq!(Meta::read(&d), Err(SaveError::InvalidData));
        // A locked selected character falls back to Lumi rather than failing.
        let mut d = Document::new(Meta::SCHEMA, Meta::VERSION, 1).unwrap();
        Meta {
            unlocked: 1,
            character: 3,
            ..Meta::default()
        }
        .write(&mut d)
        .unwrap();
        assert_eq!(Meta::read(&d).unwrap().character, 0);
    }

    #[test]
    fn shrine_and_unlocks_spend_glimmer_and_grant_bonuses() {
        let mut meta = Meta {
            glimmer: 100,
            ..Meta::default()
        };
        assert_eq!(meta.unlock(1), Purchase::TooPoor);
        assert_eq!(meta.buy(ShrineId::Might as usize), Purchase::Bought);
        assert_eq!(meta.glimmer, 60);
        assert_eq!(meta.next_cost(ShrineId::Might as usize), Some(100));
        assert_eq!(meta.buy(ShrineId::Might as usize), Purchase::TooPoor);
        meta.glimmer = 10_000;
        for _ in 0..10 {
            meta.buy(ShrineId::Might as usize);
        }
        assert_eq!(meta.rank(ShrineId::Might), 5);
        assert_eq!(meta.buy(ShrineId::Might as usize), Purchase::Maxed);
        assert_eq!(meta.unlock(1), Purchase::Bought);
        assert!(meta.is_unlocked(1));
        assert_eq!(meta.unlock(1), Purchase::Maxed);
        meta.buy(ShrineId::SecondWind as usize);
        let bonuses = meta.bonuses();
        assert!((bonuses.might - 1.25).abs() < 1e-6);
        assert_eq!(bonuses.revives, 1);
        // Bonuses reach the run.
        let game = Game::new(RunConfig {
            bonuses,
            ..RunConfig::default()
        });
        assert_eq!(game.player.revives, 1);
        assert!((game.mods.might - 1.25).abs() < 1e-6);
    }

    #[test]
    fn titles_and_glimmer_reflect_the_run() {
        let mut game = Game::new(RunConfig::default());
        assert_eq!(title(&game).0, "Snack-Sized Hero");
        game.stats.kills = 450;
        game.stats.coins = 7;
        game.time = 120.;
        assert_eq!(run_glimmer(&game), 7 + 20);
        game.stats.damage[WeaponId::BubbleMortar as usize] = 900.;
        game.stats.damage[WeaponId::WispBolt as usize] = 100.;
        assert_eq!(title(&game).0, "Professional Bubble Enthusiast");
    }
}
