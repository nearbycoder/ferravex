use super::*;
use crate::checkpoint::{CheckpointError, Reader, Writer, bounded};
impl RewindHistory {
    pub(crate) fn write_checkpoint(
        &self,
        w: &mut Writer<'_>,
        now: u32,
    ) -> Result<(), CheckpointError> {
        for life in self.lives {
            w.u64(life);
        }
        w.u8(self.frames.len() as u8);
        let mut prev = None;
        for f in &self.frames {
            let age = now.wrapping_sub(f.tick);
            if age > MAX_REWIND_TICKS || prev.is_some_and(|p| p <= age) {
                return Err(CheckpointError::Invalid);
            }
            prev = Some(age);
            w.u32(f.tick);
            let mask = f
                .players
                .iter()
                .enumerate()
                .fold(0u16, |m, (i, p)| m | ((p.is_some() as u16) << i));
            w.u16(mask);
            for (id, p) in f.players.iter().enumerate() {
                if let Some(p) = p {
                    if !bounded(p.position, 4096.) || p.life > self.lives[id] {
                        return Err(CheckpointError::Invalid);
                    }
                    w.vec3(p.position);
                    w.u8(p.alive as u8);
                    w.u64(p.life);
                }
            }
        }
        Ok(())
    }
    pub(crate) fn read_checkpoint(r: &mut Reader<'_>, now: u32) -> Result<Self, CheckpointError> {
        let mut out = Self::default();
        for v in &mut out.lives {
            *v = r.u64()?;
        }
        let count = r.u8()? as usize;
        if count > MAX_REWIND_TICKS as usize + 1 {
            return Err(CheckpointError::Invalid);
        }
        let mut prev = None;
        for _ in 0..count {
            let tick = r.u32()?;
            let age = now.wrapping_sub(tick);
            if age > MAX_REWIND_TICKS || prev.is_some_and(|p| p <= age) {
                return Err(CheckpointError::Invalid);
            }
            prev = Some(age);
            let mask = r.u16()?;
            let mut players = [None; MAX_PLAYERS];
            for (id, p) in players.iter_mut().enumerate() {
                if mask & (1 << id) != 0 {
                    let pose = Pose {
                        position: r.vec3()?,
                        alive: r.bool()?,
                        life: r.u64()?,
                    };
                    if !bounded(pose.position, 4096.) || pose.life > out.lives[id] {
                        return Err(CheckpointError::Invalid);
                    }
                    *p = Some(pose);
                }
            }
            out.frames.push_back(Frame { tick, players });
        }
        Ok(out)
    }
}
