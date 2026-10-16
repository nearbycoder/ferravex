//! Complete authority checkpoints, including lag-compensation life generations.
use super::*;
pub const MAX_CHECKPOINT_BYTES: usize = 32 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckpointError {
    Invalid,
    Checksum,
    Capacity,
}
pub(crate) fn hash(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
pub(crate) struct Writer<'a>(pub &'a mut dyn FnMut(&[u8]));
impl Writer<'_> {
    pub fn u8(&mut self, v: u8) {
        (self.0)(&[v]);
    }
    pub fn u16(&mut self, v: u16) {
        (self.0)(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        (self.0)(&v.to_le_bytes());
    }
    pub fn u64(&mut self, v: u64) {
        (self.0)(&v.to_le_bytes());
    }
    pub fn f32(&mut self, v: f32) {
        self.u32(v.to_bits());
    }
    pub fn vec3(&mut self, v: Vec3) {
        for x in v.to_array() {
            self.f32(x);
        }
    }
}
pub(crate) struct Reader<'a> {
    pub b: &'a [u8],
    pub i: usize,
}
impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], CheckpointError> {
        let end = self.i.checked_add(N).ok_or(CheckpointError::Invalid)?;
        let b = self.b.get(self.i..end).ok_or(CheckpointError::Invalid)?;
        self.i = end;
        Ok(b.try_into().unwrap())
    }
    pub fn u8(&mut self) -> Result<u8, CheckpointError> {
        Ok(self.take::<1>()?[0])
    }
    pub fn bool(&mut self) -> Result<bool, CheckpointError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CheckpointError::Invalid),
        }
    }
    pub fn u16(&mut self) -> Result<u16, CheckpointError> {
        Ok(u16::from_le_bytes(self.take()?))
    }
    pub fn u32(&mut self) -> Result<u32, CheckpointError> {
        Ok(u32::from_le_bytes(self.take()?))
    }
    pub fn u64(&mut self) -> Result<u64, CheckpointError> {
        Ok(u64::from_le_bytes(self.take()?))
    }
    pub fn f32(&mut self) -> Result<f32, CheckpointError> {
        let v = f32::from_bits(self.u32()?);
        if !v.is_finite() {
            return Err(CheckpointError::Invalid);
        }
        Ok(v)
    }
    pub fn vec3(&mut self) -> Result<Vec3, CheckpointError> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
}
pub(crate) fn bounded(v: Vec3, max: f32) -> bool {
    v.is_finite() && v.abs().max_element() <= max
}
fn arena_valid(a: &Arena) -> bool {
    a.solids.len() <= 256
        && a.spawns.len() <= MAX_PLAYERS
        && a.solids.iter().all(|s| {
            bounded(s.center, 8192.)
                && bounded(s.half_extents, 8192.)
                && s.half_extents.min_element() > 0.
        })
        && a.spawns.iter().all(|p| bounded(*p, 4096.))
}
fn player_valid(p: &Player) -> bool {
    (p.id as usize) < MAX_PLAYERS
        && bounded(p.position, 4096.)
        && bounded(p.velocity, 256.)
        && p.yaw.is_finite()
        && p.yaw.abs() <= std::f32::consts::PI
        && p.pitch.is_finite()
        && p.pitch.abs() <= ferravex_net::MAX_PITCH
        && p.health <= 100
        && p.ammo <= MAGAZINE_SIZE
        && p.jump.coyote.is_finite()
        && (0.0..=0.5).contains(&p.jump.coyote)
        && p.jump.buffer.is_finite()
        && (0.0..=0.5).contains(&p.jump.buffer)
        && p.fire_cooldown <= FIRE_INTERVAL_TICKS
        && p.reload_ticks <= RELOAD_TICKS
        && p.respawn_ticks <= RESPAWN_TICKS
}
impl Match {
    fn write_arena(&self, w: &mut Writer<'_>) -> Result<(), CheckpointError> {
        if !arena_valid(&self.arena) {
            return Err(CheckpointError::Invalid);
        }
        w.u16(self.arena.solids.len() as u16);
        w.u8(self.arena.spawns.len() as u8);
        for s in &self.arena.solids {
            w.vec3(s.center);
            w.vec3(s.half_extents);
        }
        for p in &self.arena.spawns {
            w.vec3(*p);
        }
        Ok(())
    }
    fn write_checkpoint(&self, w: &mut Writer<'_>) -> Result<(), CheckpointError> {
        (w.0)(b"FXMC0001");
        w.u32(self.tick);
        self.write_arena(w)?;
        if self.players.len() > MAX_PLAYERS {
            return Err(CheckpointError::Capacity);
        }
        w.u8(self.players.len() as u8);
        let mut ids = 0u16;
        for p in &self.players {
            if !player_valid(p) || ids & (1 << p.id) != 0 {
                return Err(CheckpointError::Invalid);
            }
            ids |= 1 << p.id;
            w.u8(p.id);
            w.vec3(p.position);
            w.vec3(p.velocity);
            w.f32(p.yaw);
            w.f32(p.pitch);
            w.u8(p.health);
            w.u8(p.ammo);
            w.u8(p.grounded as u8);
            w.f32(p.jump.coyote);
            w.f32(p.jump.buffer);
            w.u8(p.jump.held as u8);
            w.u16(p.fire_cooldown);
            w.u16(p.reload_ticks);
            w.u16(p.respawn_ticks);
        }
        self.rewind.write_checkpoint(w, self.tick)
    }
    /// No allocation; hashes all simulation state, including arena and rewind.
    pub fn state_hash(&self) -> Result<u64, CheckpointError> {
        let mut h = 0xcbf29ce484222325u64;
        self.write_checkpoint(&mut Writer(&mut |b| {
            for v in b {
                h = (h ^ u64::from(*v)).wrapping_mul(0x100000001b3);
            }
        }))?;
        Ok(h)
    }
    pub fn context_hash(&self) -> Result<u64, CheckpointError> {
        let mut h = 0xcbf29ce484222325u64;
        let mut emit = |b: &[u8]| {
            for v in b {
                h = (h ^ u64::from(*v)).wrapping_mul(0x100000001b3);
            }
        };
        emit(b"strike-rules-1-64hz");
        self.write_arena(&mut Writer(&mut emit))?;
        Ok(h)
    }
    /// Reuses caller storage. On rejection the output is empty.
    pub fn checkpoint_into(&self, out: &mut Vec<u8>) -> Result<(), CheckpointError> {
        out.clear();
        if let Err(e) = self.write_checkpoint(&mut Writer(&mut |b| out.extend_from_slice(b))) {
            out.clear();
            return Err(e);
        }
        let h = hash(out);
        out.extend(h.to_le_bytes());
        Ok(())
    }
    pub fn from_checkpoint(b: &[u8]) -> Result<Self, CheckpointError> {
        let invalid = CheckpointError::Invalid;
        if b.len() < 40 || b.len() > MAX_CHECKPOINT_BYTES || &b[..8] != b"FXMC0001" {
            return Err(invalid);
        }
        let end = b.len() - 8;
        if hash(&b[..end]) != u64::from_le_bytes(b[end..].try_into().unwrap()) {
            return Err(CheckpointError::Checksum);
        }
        let mut r = Reader { b: &b[..end], i: 8 };
        let tick = r.u32()?;
        let solids = r.u16()? as usize;
        let spawns = r.u8()? as usize;
        if solids > 256 || spawns > MAX_PLAYERS {
            return Err(invalid);
        }
        let mut arena = Arena {
            solids: Vec::with_capacity(solids),
            spawns: Vec::with_capacity(spawns),
        };
        for _ in 0..solids {
            arena.solids.push(Obstacle {
                center: r.vec3()?,
                half_extents: r.vec3()?,
            });
        }
        for _ in 0..spawns {
            arena.spawns.push(r.vec3()?);
        }
        if !arena_valid(&arena) {
            return Err(invalid);
        }
        let count = r.u8()? as usize;
        if count > MAX_PLAYERS {
            return Err(invalid);
        }
        let mut game = Self::new(arena);
        game.tick = tick;
        let mut ids = 0u16;
        for _ in 0..count {
            let p = Player {
                id: r.u8()?,
                position: r.vec3()?,
                velocity: r.vec3()?,
                yaw: r.f32()?,
                pitch: r.f32()?,
                health: r.u8()?,
                ammo: r.u8()?,
                grounded: r.bool()?,
                jump: JumpState {
                    coyote: r.f32()?,
                    buffer: r.f32()?,
                    held: r.bool()?,
                },
                fire_cooldown: r.u16()?,
                reload_ticks: r.u16()?,
                respawn_ticks: r.u16()?,
            };
            if !player_valid(&p) || ids & (1 << p.id) != 0 {
                return Err(invalid);
            }
            ids |= 1 << p.id;
            game.players.push(p);
        }
        game.rewind = rewind::RewindHistory::read_checkpoint(&mut r, tick)?;
        if r.i != r.b.len() {
            return Err(invalid);
        }
        Ok(game)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoint_preserves_complete_simulation_across_tick_wrap() {
        let mut g = Match::new(Arena::default());
        for id in 0..4 {
            g.spawn_player(id);
        }
        g.tick = u32::MAX - 30;
        let mut bytes = Vec::with_capacity(MAX_CHECKPOINT_BYTES);
        for i in 0..80 {
            let commands = [(
                0,
                InputFrame {
                    sequence: i,
                    move_z: 0.7,
                    buttons: if i % 20 < 10 {
                        BUTTON_FIRE | BUTTON_JUMP
                    } else {
                        BUTTON_RELOAD
                    },
                    ..Default::default()
                },
            )];
            g.step(&commands);
            g.checkpoint_into(&mut bytes).unwrap();
            let mut restored = Match::from_checkpoint(&bytes).unwrap();
            assert_eq!(g.state_hash(), restored.state_hash());
            assert_eq!(g.step(&commands), restored.step(&commands));
            assert_eq!(g.state_hash(), restored.state_hash());
        }
        for n in 0..bytes.len() {
            assert!(Match::from_checkpoint(&bytes[..n]).is_err());
        }
        bytes[15] ^= 1;
        assert!(matches!(
            Match::from_checkpoint(&bytes),
            Err(CheckpointError::Checksum)
        ));
    }
}
