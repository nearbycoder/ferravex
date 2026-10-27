//! Safe checkpoints between waves; level identity/economy are checked before restore.
use crate::{
    game::{Game, Kind, Outcome, Tower},
    level::Level,
};
use ferravex_play::persistence::{Document, SaveData, SaveError, Value};
use ferravex_render::{
    prelude::*,
    storage::{LoadStatus, Records, Slot, StorageError, run_micros},
};
use std::sync::Arc;
#[derive(Clone)]
struct Snapshot {
    context: u64,
    money: u32,
    lives: u8,
    wave: u8,
    kills: u32,
    fired: u32,
    escaped: u32,
    selected: usize,
    time: f32,
    towers: Vec<Option<Tower>>,
}
#[derive(Clone, Default)]
struct Checkpoint(Option<Snapshot>);
impl Checkpoint {
    fn capture(g: &Game, context: u64) -> Option<Self> {
        if g.active || g.outcome != Outcome::Playing || !g.pests.is_empty() || !g.shots.is_empty() {
            return None;
        }
        let mut towers = g.towers.clone();
        for t in towers.iter_mut().flatten() {
            t.cooldown = t.cooldown.max(0.);
        }
        Some(Self(Some(Snapshot {
            context,
            money: g.money,
            lives: g.lives,
            wave: g.wave,
            kills: g.kills,
            fired: g.fired,
            escaped: g.escaped,
            selected: g.selected,
            time: g.time,
            towers,
        })))
    }
    fn restore(&self, level: Arc<Level>, context: u64) -> Result<Option<Game>, SaveError> {
        let Some(s) = &self.0 else { return Ok(None) };
        if s.context != context {
            return Ok(None);
        }
        if s.wave as usize >= level.waves.len()
            || s.lives == 0
            || s.lives > level.lives
            || s.towers.len() != level.plots.len()
            || s.selected >= s.towers.len()
        {
            return Err(SaveError::InvalidData);
        }
        let completed = &level.waves[..s.wave as usize];
        let spawned: u64 = completed.iter().map(|w| w.count as u64).sum();
        if s.kills as u64 + s.escaped as u64 != spawned
            || s.escaped != u32::from(level.lives - s.lives)
        {
            return Err(SaveError::InvalidData);
        }
        let earned = level.money as u64
            + s.kills as u64 * level.kill_reward as u64
            + completed.iter().map(|w| w.reward as u64).sum::<u64>();
        let spent: u64 = s
            .towers
            .iter()
            .flatten()
            .map(|t| {
                t.kind.price() as u64
                    + match t.level {
                        0 => 0,
                        1 => 65,
                        _ => 165,
                    }
            })
            .sum();
        if s.money as u64 + spent != earned {
            return Err(SaveError::InvalidData);
        }
        let mut g = Game::with_level(level);
        g.money = s.money;
        g.lives = s.lives;
        g.wave = s.wave;
        g.kills = s.kills;
        g.fired = s.fired;
        g.escaped = s.escaped;
        g.selected = s.selected;
        g.time = s.time;
        g.towers.clone_from(&s.towers);
        g.restore_spawn_sequence();
        Ok(Some(g))
    }
}
impl SaveData for Checkpoint {
    const SCHEMA: u64 = 0x46584f5243484431;
    const VERSION: u32 = 1;
    const MAX_BYTES: usize = 4096;
    fn write(&self, d: &mut Document) -> Result<(), SaveError> {
        d.set("present", Value::Bool(self.0.is_some()))?;
        let Some(s) = &self.0 else { return Ok(()) };
        for (key, v) in [
            ("context", s.context),
            ("money", s.money as u64),
            ("lives", s.lives as u64),
            ("wave", s.wave as u64),
            ("kills", s.kills as u64),
            ("fired", s.fired as u64),
            ("escaped", s.escaped as u64),
            ("selected", s.selected as u64),
        ] {
            d.set(key, Value::U64(v))?;
        }
        d.set("time", Value::F64(s.time as f64))?;
        let mut towers = vec![s.towers.len() as u8];
        for tower in &s.towers {
            if let Some(t) = tower {
                towers.push(if t.kind == Kind::Seed { 1 } else { 2 });
                towers.push(t.level);
                for v in [t.cooldown, t.aim.x, t.aim.y] {
                    towers.extend(v.to_le_bytes());
                }
            } else {
                towers.extend([0; 14]);
            }
        }
        d.set("towers", Value::Bytes(towers))
    }
    fn read(d: &Document) -> Result<Self, SaveError> {
        if !d.boolean("present")? {
            return Ok(Self::default());
        }
        let raw = d.bytes("towers")?;
        if raw.is_empty() || raw[0] == 0 || raw[0] > 8 || raw.len() != 1 + raw[0] as usize * 14 {
            return Err(SaveError::InvalidData);
        }
        let mut towers = Vec::with_capacity(raw[0] as usize);
        for r in raw[1..].as_chunks::<14>().0 {
            if r[0] == 0 {
                if r.iter().any(|&b| b != 0) {
                    return Err(SaveError::InvalidData);
                }
                towers.push(None);
                continue;
            }
            let f = |i| f32::from_le_bytes(r[i..i + 4].try_into().unwrap());
            let cooldown = f(2);
            let aim = vec2(f(6), f(10));
            if r[0] > 2
                || r[1] > 2
                || !cooldown.is_finite()
                || !(0. ..=1.).contains(&cooldown)
                || !aim.is_finite()
                || aim.length_squared() > 1.01
            {
                return Err(SaveError::InvalidData);
            }
            towers.push(Some(Tower {
                kind: if r[0] == 1 { Kind::Seed } else { Kind::Frost },
                level: r[1],
                cooldown,
                aim,
            }));
        }
        let u32at = |key| u32::try_from(d.u64(key)?).map_err(|_| SaveError::InvalidData);
        let lives = u8::try_from(d.u64("lives")?).map_err(|_| SaveError::InvalidData)?;
        let wave = u8::try_from(d.u64("wave")?).map_err(|_| SaveError::InvalidData)?;
        let time = d.f64("time")?;
        let selected = d.u64("selected")?;
        if lives == 0
            || wave > 16
            || selected >= towers.len() as u64
            || !(0. ..=604800.).contains(&time)
        {
            return Err(SaveError::InvalidData);
        }
        Ok(Self(Some(Snapshot {
            context: d.u64("context")?,
            money: u32at("money")?,
            lives,
            wave,
            kills: u32at("kills")?,
            fired: u32at("fired")?,
            escaped: u32at("escaped")?,
            selected: selected as usize,
            time: time as f32,
            towers,
        })))
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    money: u32,
    lives: u8,
    wave: u8,
    kills: u32,
    fired: u32,
    escaped: u32,
    selected: usize,
}
impl Stamp {
    fn new(g: &Game) -> Self {
        Self {
            money: g.money,
            lives: g.lives,
            wave: g.wave,
            kills: g.kills,
            fired: g.fired,
            escaped: g.escaped,
            selected: g.selected,
        }
    }
}
/// How a finished run compares with the stored personal records.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Best {
    /// Most pests stopped in any earlier run, if one was recorded.
    pub previous_score: Option<u64>,
    /// Fastest earlier winning harvest in microseconds of simulated time.
    pub previous_fastest_us: Option<u64>,
    pub new_score: bool,
    pub new_fastest: bool,
}
impl Best {
    /// Compare an attempt with the records that existed before it.
    pub fn compare(previous: Option<(u64, Option<u64>)>, score: u64, time: Option<u64>) -> Self {
        let previous_score = previous.map(|p| p.0);
        let previous_fastest_us = previous.and_then(|p| p.1);
        Self {
            previous_score,
            previous_fastest_us,
            new_score: previous_score.is_none_or(|best| score > best),
            new_fastest: time.is_some_and(|t| previous_fastest_us.is_none_or(|best| t < best)),
        }
    }
}
pub struct Progress {
    slot: Slot<Checkpoint>,
    records: Records,
    context: u64,
    last: Stamp,
    ended: bool,
    best: Option<Best>,
    error: Option<StorageError>,
}
impl Progress {
    pub fn new(g: &mut Game, context: u64) -> Self {
        let mut slot = Slot::open("orchard-guard", "checkpoint", Checkpoint::default()).unwrap();
        let restored = slot.validate_loaded(|c| c.restore(g.level.clone(), context));
        let error =
            restored
                .as_ref()
                .err()
                .copied()
                .or(if let LoadStatus::Rejected(e) = slot.status() {
                    Some(e)
                } else {
                    None
                });
        if let Ok(Some(restored)) = restored {
            *g = restored;
        }
        Self {
            slot,
            records: Records::new("orchard-guard").unwrap(),
            context,
            last: Stamp::new(g),
            ended: false,
            best: None,
            error,
        }
    }
    pub fn content_changed(&mut self, g: &Game, context: u64) {
        self.context = context;
        self.last = Stamp::new(g);
        self.ended = false;
        self.best = None;
    }
    pub fn restart(&mut self, g: &Game) {
        self.last = Stamp::new(g);
        self.ended = false;
        self.best = None;
        self.error = self.slot.reset(Checkpoint::default()).err();
    }
    pub fn error(&self) -> Option<StorageError> {
        self.error
    }
    /// The finished run's record comparison, available once it has ended.
    pub fn best(&self) -> Option<Best> {
        self.best
    }
    pub fn sync(&mut self, g: &Game) {
        if g.outcome != Outcome::Playing {
            if !self.ended {
                self.ended = true;
                let time = if g.outcome == Outcome::Won {
                    run_micros(g.time)
                } else {
                    None
                };
                let previous = self
                    .records
                    .get("harvest", self.context)
                    .map(|r| (r.score, r.fastest_us));
                self.best = Some(Best::compare(previous, g.kills as u64, time));
                let _ = self
                    .records
                    .attempt("harvest", self.context, g.kills as u64, time);
                if g.outcome == Outcome::Won {
                    self.error = self.slot.commit(Checkpoint::default()).err();
                }
            }
            return;
        }
        let stamp = Stamp::new(g);
        if g.active || stamp == self.last {
            return;
        }
        self.last = stamp;
        if self.slot.status() == LoadStatus::Transient {
            return;
        }
        if let Some(checkpoint) = Checkpoint::capture(g, self.context) {
            self.error = self.slot.commit(checkpoint).err();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn best_comparison_flags_first_and_improved_records_only() {
        let first = Best::compare(None, 0, None);
        assert!(first.new_score && !first.new_fastest);
        let better = Best::compare(Some((40, Some(200))), 41, Some(150));
        assert!(better.new_score && better.new_fastest);
        assert_eq!(better.previous_score, Some(40));
        let worse = Best::compare(Some((40, Some(200))), 40, Some(250));
        assert!(!worse.new_score && !worse.new_fastest);
        assert!(Best::compare(Some((64, None)), 64, Some(9)).new_fastest);
    }
    #[test]
    fn checkpoints_restore_valid_builds_and_reject_forged_economy_or_context() {
        let mut g = Game::new();
        assert!(g.build(0, Kind::Seed));
        let context = 42;
        let save = Checkpoint::capture(&g, context).unwrap();
        let bytes = save.encode(1).unwrap();
        let decoded = Checkpoint::decode(&bytes).unwrap().0;
        let restored = decoded.restore(g.level.clone(), context).unwrap().unwrap();
        assert_eq!(restored.money, g.money);
        assert!(restored.towers[0].is_some());
        assert_eq!(restored.wave, 0);
        assert!(
            decoded
                .restore(g.level.clone(), context + 1)
                .unwrap()
                .is_none()
        );
        let mut forged = decoded.clone();
        forged.0.as_mut().unwrap().money += 100;
        assert!(forged.restore(g.level.clone(), context).is_err());
        g.launch();
        assert!(Checkpoint::capture(&g, context).is_none());
    }
    #[test]
    fn actual_cleared_wave_resumes_and_can_finish_the_campaign() {
        let mut g = Game::new();
        g.autoplay();
        for _ in 0..7200 {
            g.step(1. / 60.);
            if !g.active && g.wave > 0 {
                break;
            }
        }
        assert_eq!(g.wave, 1);
        let save = Checkpoint::capture(&g, 7).expect("safe checkpoint after first wave");
        let mut restored = Checkpoint::decode(&save.encode(1).unwrap())
            .unwrap()
            .0
            .restore(g.level.clone(), 7)
            .unwrap()
            .unwrap();
        assert_eq!(restored.kills, g.kills);
        assert_eq!(restored.money, g.money);
        assert_eq!(
            restored.next_wave().unwrap().fast_count,
            g.next_wave().unwrap().fast_count
        );
        // Spawn identity determines fast-pest cadence. Restoring a checkpoint
        // must continue that cadence without extending the version-1 save.
        for _ in 0..240 {
            g.autoplay();
            restored.autoplay();
            g.step(1. / 60.);
            restored.step(1. / 60.);
            assert_eq!(restored.money, g.money);
            assert_eq!(restored.kills, g.kills);
            assert_eq!(restored.pests.len(), g.pests.len());
            for (actual, expected) in restored.pests.iter().zip(&g.pests) {
                assert_eq!((actual.id, actual.fast), (expected.id, expected.fast));
                assert_eq!((actual.hp, actual.pos), (expected.hp, expected.pos));
            }
        }
        for _ in 0..18000 {
            restored.autoplay();
            restored.step(1. / 60.);
            if restored.outcome != Outcome::Playing {
                break;
            }
        }
        assert_eq!(restored.outcome, Outcome::Won);
    }
}
