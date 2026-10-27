//! Saves: lifetime progress and hats, the in-progress to-do list, and the
//! best-run ghost.
//!
//! All three are versioned [`SaveData`] documents in engine `Slot`s, so
//! corrupt or future files are protected rather than overwritten. The to-do
//! list is the engine's own [`ObjectiveSnapshot`], restored into a fresh
//! [`ObjectiveGraph`](ferravex_play::objective::ObjectiveGraph) whose rules fingerprint must match. Autoplay and smoke
//! runs use transient storage and never touch real saves.
//!
//! ```
//! use ferravex_play::persistence::SaveData;
//! use picnic_panic::meta::{Ghost, Meta};
//! let mut meta = Meta::default();
//! meta.record_clear(13 * 60 * 1000, 3, 2, 0b11, true);
//! let bytes = meta.encode(1).unwrap();
//! let (back, _) = Meta::decode(&bytes).unwrap();
//! assert_eq!(back.best_ms, 13 * 60 * 1000);
//! assert!(back.has_hat(picnic_panic::models::Hat::PaperCrown));
//! let mut ghost = Ghost::default();
//! ghost.push(ferravex_render::prelude::Vec3::new(1.5, 0.25, -3.), 0.7);
//! let (back, _) = Ghost::decode(&ghost.encode(1).unwrap()).unwrap();
//! assert_eq!(back.len(), 1);
//! ```
use crate::models::Hat;
use crate::sim::pranks::{PRANKS, SLOTS, Todo};
use ferravex_play::objective::ObjectiveSnapshot;
use ferravex_play::persistence::{Document, SaveData, SaveError, Value};
use ferravex_play::replay::ghost::{GhostTrack, Sample};
use ferravex_render::prelude::{Quat, Vec3};

pub const OWNER: &str = "picnic-panic";
pub const RECORD: &str = "clear";
pub const RECORD_RULES: u64 = 0x7069_636e_0002;
pub const FAST_MS: u64 = 15 * 60 * 1000;
pub const STAR_MS: u64 = 18 * 60 * 1000;
/// Ghost samples are kept every `GHOST_STEP` ticks (6 Hz).
pub const GHOST_STEP: u64 = 10;
pub const GHOST_MAX: usize = 13_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meta {
    pub runs: u32,
    pub clears: u32,
    pub best_ms: u64,
    pub best_stars: u8,
    pub hats: u8,
    pub hat: u8,
    pub bonuses: u8,
    pub pranks: u8,
    pub scruffs: u32,
    pub ghost: bool,
    pub cones: bool,
    /// Elapsed milliseconds of the saved in-progress day (0 = none).
    pub day_ms: u64,
}

impl Default for Meta {
    fn default() -> Self {
        Self {
            runs: 0,
            clears: 0,
            best_ms: 0,
            best_stars: 0,
            hats: 1,
            hat: 0,
            bonuses: 0,
            pranks: 0,
            scruffs: 0,
            ghost: true,
            cones: true,
            day_ms: 0,
        }
    }
}

impl SaveData for Meta {
    const SCHEMA: u64 = 0x5049_434e_4943_0001; // "PICNIC"
    const VERSION: u32 = 1;
    const MAX_BYTES: usize = 2048;

    fn write(&self, d: &mut Document) -> Result<(), SaveError> {
        for (k, v) in [
            ("runs", u64::from(self.runs)),
            ("clears", u64::from(self.clears)),
            ("best_ms", self.best_ms),
            ("best_stars", u64::from(self.best_stars)),
            ("hats", u64::from(self.hats)),
            ("hat", u64::from(self.hat)),
            ("bonuses", u64::from(self.bonuses)),
            ("pranks", u64::from(self.pranks)),
            ("scruffs", u64::from(self.scruffs)),
            ("day_ms", self.day_ms),
        ] {
            d.set(k, Value::U64(v))?;
        }
        d.set("ghost", Value::Bool(self.ghost))?;
        d.set("cones", Value::Bool(self.cones))?;
        Ok(())
    }

    fn read(d: &Document) -> Result<Self, SaveError> {
        let small = |k: &str, max: u64| -> Result<u64, SaveError> {
            let v = d.u64(k)?;
            if v > max {
                Err(SaveError::InvalidData)
            } else {
                Ok(v)
            }
        };
        let hats = (small("hats", 0xff)? as u8) | 1;
        let hat = small("hat", Hat::ALL.len() as u64 - 1)? as u8;
        Ok(Self {
            runs: small("runs", u64::from(u32::MAX))? as u32,
            clears: small("clears", u64::from(u32::MAX))? as u32,
            best_ms: small("best_ms", 24 * 3600 * 1000)?,
            best_stars: small("best_stars", 3)? as u8,
            hats,
            hat: if hats & (1 << hat) != 0 { hat } else { 0 },
            bonuses: small("bonuses", 0x3f)? as u8,
            pranks: small("pranks", 0x7f)? as u8,
            scruffs: small("scruffs", u64::from(u32::MAX))? as u32,
            ghost: d.boolean("ghost")?,
            cones: d.boolean("cones")?,
            day_ms: small("day_ms", 24 * 3600 * 1000)?,
        })
    }
}

/// Stars for a clear: one for finishing, one for speed, one for stealth.
pub fn stars(ms: u64, scruffs: u32) -> u8 {
    1 + u8::from(ms <= STAR_MS) + u8::from(scruffs <= 2)
}

impl Meta {
    pub fn has_hat(&self, hat: Hat) -> bool {
        self.hats & (1 << hat.index()) != 0
    }

    fn unlock(&mut self, hat: Hat) -> bool {
        let bit = 1 << hat.index();
        let new = self.hats & bit == 0;
        self.hats |= bit;
        new
    }

    /// Note crossed-off pranks and found bonuses; returns a newly unlocked hat.
    pub fn note_progress(&mut self, pranks: u8, bonuses: u8, all_shinies: bool) -> Option<Hat> {
        self.pranks |= pranks & 0x7f;
        self.bonuses |= bonuses & 0x3f;
        let mut new = None;
        if self.pranks != 0 && self.unlock(Hat::PieTin) {
            new = Some(Hat::PieTin);
        }
        if self.bonuses.count_ones() >= 3 && self.unlock(Hat::FlowerCrown) {
            new = Some(Hat::FlowerCrown);
        }
        if all_shinies && self.unlock(Hat::CapCrown) {
            new = Some(Hat::CapCrown);
        }
        new
    }

    /// Record a finished day. Returns (new best?, newly unlocked hat).
    pub fn record_clear(
        &mut self,
        ms: u64,
        scruffs: u32,
        bonuses_this_run: u32,
        bonuses: u8,
        all_shinies: bool,
    ) -> (bool, Option<Hat>) {
        let _ = bonuses_this_run;
        self.clears = self.clears.saturating_add(1);
        self.day_ms = 0;
        let best = self.best_ms == 0 || ms < self.best_ms;
        if best {
            self.best_ms = ms;
        }
        self.best_stars = self.best_stars.max(stars(ms, scruffs));
        let mut new = self.note_progress(0x7f, bonuses, all_shinies);
        if self.unlock(Hat::TeaCosy) {
            new = Some(Hat::TeaCosy);
        }
        if ms <= FAST_MS && self.unlock(Hat::PaperCrown) {
            new = Some(Hat::PaperCrown);
        }
        (best, new)
    }

    pub fn selected_hat(&self) -> Hat {
        Hat::ALL[usize::from(self.hat).min(Hat::ALL.len() - 1)]
    }
}

/// Saved to-do progress: the engine's objective snapshot.
pub type DaySave = ObjectiveSnapshot<SLOTS>;

/// Validate a saved snapshot against the current rules; returns the mask of
/// crossed-off pranks, or `None` for an empty, foreign or invalid list.
pub fn restore_mask(saved: &DaySave) -> Option<u8> {
    let mut todo = Todo::new();
    todo.graph.restore(saved.clone()).ok()?;
    let mask = todo.pranks_done();
    (mask != 0 && !todo.done(crate::sim::pranks::Prank::Escape))
        .then_some(mask & ((1 << PRANKS) - 1))
}

/// Best-run ghost: quantized positions every [`GHOST_STEP`] ticks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ghost {
    data: Vec<u8>,
}

impl Ghost {
    pub fn len(&self) -> usize {
        self.data.len() / 7
    }
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    pub fn clear(&mut self) {
        self.data.clear();
    }
    pub fn push(&mut self, p: Vec3, yaw: f32) -> bool {
        if self.len() >= GHOST_MAX || !p.is_finite() {
            return false;
        }
        if self.data.capacity() == 0 {
            self.data.reserve_exact(GHOST_MAX * 7);
        }
        let q = |v: f32| ((v * 100.).round().clamp(-32767., 32767.) as i16).to_le_bytes();
        self.data.extend_from_slice(&q(p.x));
        self.data.extend_from_slice(&q(p.y));
        self.data.extend_from_slice(&q(p.z));
        let turn = yaw.rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
        self.data.push((turn * 255.).round() as u8);
        true
    }
    pub fn sample(&self, i: usize) -> Option<(Vec3, f32)> {
        let b = self.data.get(i * 7..i * 7 + 7)?;
        let v = |k: usize| f32::from(i16::from_le_bytes([b[k], b[k + 1]])) / 100.;
        Some((
            Vec3::new(v(0), v(2), v(4)),
            f32::from(b[6]) / 255. * std::f32::consts::TAU,
        ))
    }
    /// Build the engine ghost track (interpolating ticks and rotations).
    pub fn track(&self) -> Option<GhostTrack> {
        let mut track = GhostTrack::new(RECORD_RULES, 60, self.len().max(1)).ok()?;
        for i in 0..self.len() {
            let (p, yaw) = self.sample(i)?;
            track
                .push(Sample {
                    tick: i as u64 * GHOST_STEP,
                    position: p,
                    rotation: Quat::from_rotation_y(-yaw),
                    teleport: i > 0 && self.sample(i - 1).is_some_and(|(q, _)| q.distance(p) > 4.),
                })
                .ok()?;
        }
        Some(track)
    }
}

impl SaveData for Ghost {
    const SCHEMA: u64 = 0x5049_434e_4748_0001;
    const VERSION: u32 = 1;
    const MAX_BYTES: usize = GHOST_MAX * 7 + 512;

    fn write(&self, d: &mut Document) -> Result<(), SaveError> {
        d.set("samples", Value::Bytes(self.data.clone()))
    }

    fn read(d: &Document) -> Result<Self, SaveError> {
        let data = d.bytes("samples")?;
        if !data.len().is_multiple_of(7) || data.len() > GHOST_MAX * 7 {
            return Err(SaveError::InvalidData);
        }
        Ok(Self {
            data: data.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_round_trips_and_rejects_nonsense() {
        let mut meta = Meta::default();
        assert_eq!(
            meta.note_progress(0b1, 0b111, false),
            Some(Hat::FlowerCrown)
        );
        assert!(meta.has_hat(Hat::PieTin));
        let (best, hat) = meta.record_clear(20 * 60 * 1000, 9, 0, 0, false);
        assert!(best);
        assert_eq!(hat, Some(Hat::TeaCosy));
        assert_eq!(meta.best_stars, 1);
        let (back, _) = Meta::decode(&meta.encode(3).unwrap()).unwrap();
        assert_eq!(back, meta);
        let mut d = Document::new(Meta::SCHEMA, Meta::VERSION, 1).unwrap();
        meta.write(&mut d).unwrap();
        d.set("hat", Value::U64(99)).unwrap();
        assert!(Meta::read(&d).is_err());
        assert_eq!(stars(10 * 60 * 1000, 0), 3);
    }

    #[test]
    fn day_snapshot_restores_only_matching_rules() {
        let mut game = crate::sim::Game::new(crate::sim::RunConfig::default());
        crate::sim::pranks::complete(&mut game, crate::sim::pranks::Prank::Bell, Vec3::ZERO);
        crate::sim::pranks::complete(&mut game, crate::sim::pranks::Prank::Pie, Vec3::ZERO);
        let saved = game.todo.snapshot();
        let bytes = saved.encode(1).unwrap();
        let (loaded, _) = DaySave::decode(&bytes).unwrap();
        let mask = restore_mask(&loaded).unwrap();
        assert_eq!(mask, 0b10001);
        // A continued day starts with those lines already crossed off.
        let resumed = crate::sim::Game::new(crate::sim::RunConfig {
            seed: 1,
            completed: mask,
        });
        assert_eq!(resumed.completed(), mask);
        assert!(restore_mask(&Todo::new().snapshot()).is_none());
    }

    #[test]
    fn ghost_quantizes_and_builds_an_engine_track() {
        let mut g = Ghost::default();
        for i in 0..20 {
            assert!(g.push(Vec3::new(i as f32 * 0.5, 0., -1.), 0.3));
        }
        let (back, _) = Ghost::decode(&g.encode(1).unwrap()).unwrap();
        assert_eq!(back, g);
        let track = back.track().unwrap();
        let mid = track.sample(GHOST_STEP * 3 + 5, 0.).unwrap();
        assert!((mid.position.x - 1.75).abs() < 0.02);
    }
}
