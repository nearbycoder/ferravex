use super::*;
const MAGIC: &[u8; 8] = b"FXRT0001";
fn checksum(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
fn u16at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes(b[i..i + 2].try_into().unwrap())
}
fn u32at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}
fn u64at(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().unwrap())
}
fn span(b: &[u8], i: usize) -> Span {
    Span {
        start: u32at(b, i),
        len: u32at(b, i + 4),
    }
}
fn tick(b: &[u8]) -> Tick {
    Tick {
        tick: u64at(b, 0),
        hash: u64at(b, 8),
        input: span(b, 16),
        event_start: u32at(b, 24),
        event_count: u16at(b, 28),
    }
}
fn event(b: &[u8]) -> Event {
    Event {
        kind: u16at(b, 0),
        data: span(b, 4),
    }
}
fn checkpoint(b: &[u8]) -> Checkpoint {
    Checkpoint {
        tick: u64at(b, 0),
        hash: u64at(b, 8),
        state: span(b, 16),
    }
}
fn put_span(out: &mut Vec<u8>, s: Span) {
    out.extend(s.start.to_le_bytes());
    out.extend(s.len.to_le_bytes());
}
impl Recording {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            64 + self.ticks.len() * 32
                + self.events.len() * 12
                + self.checkpoints.len() * 24
                + self.arena.len(),
        );
        out.extend(MAGIC);
        out.extend(self.header.schema.to_le_bytes());
        out.extend(self.header.version.to_le_bytes());
        out.extend(self.header.context.to_le_bytes());
        out.extend(self.header.tick_hz.to_le_bytes());
        out.extend([0; 2]);
        out.extend(self.header.seed.to_le_bytes());
        for n in [
            self.ticks.len(),
            self.events.len(),
            self.checkpoints.len(),
            self.arena.len(),
        ] {
            out.extend((n as u32).to_le_bytes());
        }
        for t in &self.ticks {
            out.extend(t.tick.to_le_bytes());
            out.extend(t.hash.to_le_bytes());
            put_span(&mut out, t.input);
            out.extend(t.event_start.to_le_bytes());
            out.extend(t.event_count.to_le_bytes());
            out.extend([0; 2]);
        }
        for e in &self.events {
            out.extend(e.kind.to_le_bytes());
            out.extend([0; 2]);
            put_span(&mut out, e.data);
        }
        for c in &self.checkpoints {
            out.extend(c.tick.to_le_bytes());
            out.extend(c.hash.to_le_bytes());
            put_span(&mut out, c.state);
        }
        out.extend(&self.arena);
        out.extend(checksum(&out).to_le_bytes());
        out
    }
    /// Validates all tables, canonical packing and checksum before allocating.
    pub fn decode(b: &[u8]) -> Result<Self, ReplayError> {
        let invalid = ReplayError::InvalidData;
        if b.len() < 64 || b.len() > MAX_FILE_BYTES {
            return Err(invalid);
        }
        if &b[..8] != MAGIC {
            return Err(ReplayError::UnsupportedVersion);
        }
        let header = Header {
            schema: u64at(b, 8),
            version: u32at(b, 16),
            context: u64at(b, 20),
            tick_hz: u16at(b, 28),
            seed: u64at(b, 32),
        };
        if !header.valid() || u16at(b, 30) != 0 {
            return Err(invalid);
        }
        let n = u32at(b, 40) as usize;
        let e = u32at(b, 44) as usize;
        let c = u32at(b, 48) as usize;
        let a = u32at(b, 52) as usize;
        if n > MAX_TICKS || e > MAX_EVENTS || !(1..=MAX_CHECKPOINTS).contains(&c) {
            return Err(invalid);
        }
        let expected = 64usize
            .checked_add(n * 32)
            .and_then(|v| v.checked_add(e * 12))
            .and_then(|v| v.checked_add(c * 24))
            .and_then(|v| v.checked_add(a));
        if expected != Some(b.len()) {
            return Err(invalid);
        }
        if checksum(&b[..b.len() - 8]) != u64at(b, b.len() - 8) {
            return Err(ReplayError::Checksum);
        }
        let ticks = &b[56..56 + n * 32];
        let events = &b[56 + n * 32..56 + n * 32 + e * 12];
        let cps = &b[56 + n * 32 + e * 12..56 + n * 32 + e * 12 + c * 24];
        let arena = &b[56 + n * 32 + e * 12 + c * 24..b.len() - 8];
        let mut cursor = 0usize;
        let mut consume = |s: Span, limit: usize, nonempty: bool| -> Result<(), ReplayError> {
            if s.start as usize != cursor || s.len as usize > limit || (nonempty && s.len == 0) {
                return Err(invalid);
            }
            cursor = cursor.checked_add(s.len as usize).ok_or(invalid)?;
            if cursor > a {
                return Err(invalid);
            }
            Ok(())
        };
        let initial = checkpoint(&cps[..24]);
        consume(initial.state, MAX_STATE_BYTES, true)?;
        let mut ci = 1;
        let mut ei = 0;
        for (i, row) in ticks.chunks_exact(32).enumerate() {
            let t = tick(row);
            if initial.tick.checked_add(i as u64 + 1) != Some(t.tick)
                || t.event_start as usize != ei
                || t.event_count > 64
                || u16at(row, 30) != 0
            {
                return Err(invalid);
            }
            consume(t.input, MAX_INPUT_BYTES, false)?;
            for _ in 0..t.event_count {
                if ei >= e {
                    return Err(invalid);
                }
                let row = &events[ei * 12..ei * 12 + 12];
                if u16at(row, 2) != 0 {
                    return Err(invalid);
                }
                consume(event(row).data, MAX_EVENT_BYTES, false)?;
                ei += 1;
            }
            if ci < c {
                let cp = checkpoint(&cps[ci * 24..ci * 24 + 24]);
                if cp.tick < t.tick {
                    return Err(invalid);
                }
                if cp.tick == t.tick {
                    if cp.hash != t.hash {
                        return Err(invalid);
                    }
                    consume(cp.state, MAX_STATE_BYTES, true)?;
                    ci += 1;
                }
            }
        }
        if ci != c || ei != e || cursor != a {
            return Err(invalid);
        }
        Ok(Self {
            header,
            ticks: ticks.chunks_exact(32).map(tick).collect(),
            events: events.chunks_exact(12).map(event).collect(),
            checkpoints: cps.chunks_exact(24).map(checkpoint).collect(),
            arena: arena.to_vec(),
        })
    }
}
