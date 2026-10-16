//! Best-lap ghost persisted through the shared typed save envelope/platform store.
use crate::sim::{Pose, Race, Track};
use ferravex_play::persistence::{
    Document, MAX_RUN_US, SaveData, SaveError, Value, checksum, run_micros,
};
use ferravex_render::prelude::*;
#[derive(Default)]
pub struct BestLap {
    pub context: u64,
    pub time_us: u64,
    pub poses: Vec<Pose>,
}
impl BestLap {
    pub fn from_race(race: &Race, context: u64) -> Option<Self> {
        Some(Self {
            context,
            time_us: run_micros(race.best_time?)?,
            poses: race.best_ghost.clone(),
        })
    }
    pub fn restore(&self, race: &mut Race, context: u64) {
        if self.context == context && self.time_us > 0 {
            race.best_time = Some(self.time_us as f32 / 1_000_000.);
            race.best_ghost.clone_from(&self.poses);
            race.rebuild_ghost();
        }
    }
}
impl SaveData for BestLap {
    const SCHEMA: u64 = 0x465852414c4c5931;
    const VERSION: u32 = 1;
    const MAX_BYTES: usize = 80 * 1024;
    fn write(&self, d: &mut Document) -> Result<(), SaveError> {
        if !valid(self.context, self.time_us, &self.poses) {
            return Err(SaveError::InvalidData);
        }
        d.set("context", Value::U64(self.context))?;
        d.set("time-us", Value::U64(self.time_us))?;
        d.set("hz", Value::U64(20))?;
        let mut bytes = Vec::with_capacity(self.poses.len() * 12);
        for pose in &self.poses {
            for v in [pose.position.x, pose.position.y, pose.angle] {
                bytes.extend(v.to_le_bytes());
            }
        }
        d.set("poses", Value::Bytes(bytes))
    }
    fn read(d: &Document) -> Result<Self, SaveError> {
        let context = d.u64("context")?;
        let time_us = d.u64("time-us")?;
        let raw = d.bytes("poses")?;
        if d.u64("hz")? != 20 || raw.len() % 12 != 0 || raw.len() > 6000 * 12 {
            return Err(SaveError::InvalidData);
        }
        // Validate numerical content before allocating decoded poses.
        for row in raw.as_chunks::<4>().0 {
            let v = f32::from_le_bytes(*row);
            if !v.is_finite() || v.abs() > 1e6 {
                return Err(SaveError::InvalidData);
            }
        }
        let poses: Vec<Pose> = raw
            .as_chunks::<12>()
            .0
            .iter()
            .map(|r| {
                let at = |i| f32::from_le_bytes(r[i..i + 4].try_into().unwrap());
                Pose {
                    position: vec2(at(0), at(4)),
                    angle: at(8),
                }
            })
            .collect();
        if !valid(context, time_us, &poses) {
            return Err(SaveError::InvalidData);
        }
        Ok(Self {
            context,
            time_us,
            poses,
        })
    }
}
fn valid(context: u64, time_us: u64, poses: &[Pose]) -> bool {
    if time_us == 0 {
        return context == 0 && poses.is_empty();
    }
    time_us <= MAX_RUN_US
        && !poses.is_empty()
        && poses.len() <= 6000
        && poses.len() as u64 <= time_us.div_ceil(50_000) + 1
        && poses.iter().all(|p| {
            [p.position.x, p.position.y, p.angle]
                .into_iter()
                .all(|v| v.is_finite() && v.abs() <= 1e6)
        })
}
pub fn context(track: &Track) -> u64 {
    let mut bytes = b"rally-rules-1-120hz-20hz-ghost".to_vec();
    for point in &track.points {
        bytes.extend(point.x.to_le_bytes());
        bytes.extend(point.y.to_le_bytes());
    }
    checksum(&bytes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actual_lap_ghost_roundtrips_and_only_restores_matching_track() {
        let track = Track::new();
        let mut race = Race::new(&track);
        let ctx = context(&track);
        for _ in 0..18000 {
            race.step(&track, race.autopilot(&track), 1. / 120.);
            if race.laps > 0 {
                break;
            }
        }
        let best = BestLap::from_race(&race, ctx).expect("autopilot must finish a lap");
        let count = best.poses.len();
        assert!(count > 10);
        let loaded = BestLap::decode(&best.encode(3).unwrap()).unwrap().0;
        let mut restored = Race::new(&track);
        loaded.restore(&mut restored, ctx ^ 1);
        assert!(restored.best_time.is_none());
        loaded.restore(&mut restored, ctx);
        assert_eq!(restored.best_ghost.len(), count);
        assert!((restored.best_time.unwrap() - race.best_time.unwrap()).abs() < 0.0001);
        let mut invalid = best;
        invalid.poses[0].angle = f32::NAN;
        assert!(invalid.encode(4).is_err());
    }
}
