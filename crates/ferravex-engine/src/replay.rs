pub use ferravex_play::replay::{
    EventInput, EventIter, Header, Limits, Playback, PlaybackError, Recorder, Recording,
    ReplayError, ReplayWorld, derive_seed, ghost,
};
// Original bounded binary input replay. Checksums detect corruption, not malicious tampering.
// Replay stores inputs and hashes; it does not capture arbitrary game state or restore sessions.
use ferravex_net::InputFrame;
const MAGIC: &[u8; 8] = b"FXRP0001";
pub const MAX_REPLAY_FRAMES: usize = 1_000_000;
const ROW_BYTES: usize = 37;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub tick: u64,
    pub input: InputFrame,
    pub state_hash: u64,
}
#[derive(Default)]
pub struct Replay {
    frames: Vec<Frame>,
}
impl Replay {
    pub fn record(&mut self, frame: Frame) -> bool {
        if self.frames.len() >= MAX_REPLAY_FRAMES
            || !frame.input.is_valid()
            || self.frames.last().is_some_and(|f| f.tick >= frame.tick)
        {
            return false;
        }
        // Growth stays within the logical frame cap instead of doubling past it.
        if self.frames.len() == self.frames.capacity() {
            let growth = self
                .frames
                .len()
                .max(1024)
                .min(MAX_REPLAY_FRAMES - self.frames.len());
            self.frames.reserve_exact(growth);
        }
        self.frames.push(frame);
        true
    }
    /// Exact tick lookup in the original FXRP0001 input stream.
    pub fn frame_at(&self, tick: u64) -> Option<&Frame> {
        self.frames
            .binary_search_by_key(&tick, |f| f.tick)
            .ok()
            .map(|i| &self.frames[i])
    }
    /// Most recent input at or before a tick; gaps do not invent new frames.
    pub fn frame_before_or_at(&self, tick: u64) -> Option<&Frame> {
        let end = self.frames.partition_point(|f| f.tick <= tick);
        end.checked_sub(1).map(|i| &self.frames[i])
    }
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(20 + self.frames.len() * ROW_BYTES);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(self.frames.len() as u32).to_le_bytes());
        for frame in &self.frames {
            out.extend(frame.tick.to_le_bytes());
            out.extend(frame.input.sequence.to_le_bytes());
            for value in [
                frame.input.move_x,
                frame.input.move_z,
                frame.input.yaw,
                frame.input.pitch,
            ] {
                out.extend(value.to_le_bytes());
            }
            out.push(frame.input.buttons);
            out.extend(frame.state_hash.to_le_bytes());
        }
        let hash = checksum(&out);
        out.extend(hash.to_le_bytes());
        out
    }
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 20 || &data[..8] != MAGIC {
            return None;
        }
        let count = u32::from_le_bytes(data[8..12].try_into().ok()?) as usize;
        let expected = count.checked_mul(ROW_BYTES)?.checked_add(20)?;
        if count > MAX_REPLAY_FRAMES || data.len() != expected {
            return None;
        }
        let end = data.len() - 8;
        if checksum(&data[..end]) != u64::from_le_bytes(data[end..].try_into().ok()?) {
            return None;
        }
        let rows = &data[12..end];
        let mut previous = None;
        // Validate every row before reserving output for a hostile count/header.
        for row in rows.as_chunks::<ROW_BYTES>().0 {
            let frame = decode_row(row)?;
            if previous.is_some_and(|tick| tick >= frame.tick) {
                return None;
            }
            previous = Some(frame.tick);
        }
        let mut frames = Vec::with_capacity(count);
        for row in rows.as_chunks::<ROW_BYTES>().0 {
            frames.push(decode_row(row)?);
        }
        Some(Self { frames })
    }
}
fn decode_row(row: &[u8]) -> Option<Frame> {
    let u32at = |i| u32::from_le_bytes(row[i..i + 4].try_into().unwrap());
    let f32at = |i| f32::from_bits(u32at(i));
    let frame = Frame {
        tick: u64::from_le_bytes(row[..8].try_into().ok()?),
        input: InputFrame {
            sequence: u32at(8),
            move_x: f32at(12),
            move_z: f32at(16),
            yaw: f32at(20),
            pitch: f32at(24),
            buttons: row[28],
        },
        state_hash: u64::from_le_bytes(row[29..37].try_into().ok()?),
    };
    frame.input.is_valid().then_some(frame)
}
pub fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn seal(bytes: &mut [u8]) {
        let end = bytes.len() - 8;
        let hash = checksum(&bytes[..end]);
        bytes[end..].copy_from_slice(&hash.to_le_bytes());
    }
    fn recording() -> Replay {
        let mut replay = Replay::default();
        for tick in 0..3 {
            assert!(replay.record(Frame {
                tick,
                input: InputFrame::default(),
                state_hash: tick * 7
            }));
        }
        replay
    }
    #[test]
    fn replay_roundtrip_rejects_corruption_and_nonmonotonic_ticks() {
        let mut r = recording();
        assert!(!r.record(r.frames[0]));
        let bytes = r.encode();
        assert_eq!(Replay::decode(&bytes).unwrap().frames(), r.frames());
        for len in 0..bytes.len() {
            assert!(Replay::decode(&bytes[..len]).is_none());
        }
        let mut bad = bytes;
        bad[24] ^= 1;
        assert!(Replay::decode(&bad).is_none());
    }
    #[test]
    fn checked_decoder_rejects_checksum_valid_invalid_inputs() {
        let original = recording().encode();
        for (offset, value) in [
            (24, f32::NAN.to_le_bytes()),
            (24, 2.0_f32.to_le_bytes()),
            (32, 10.0_f32.to_le_bytes()),
        ] {
            let mut bytes = original.clone();
            bytes[offset..offset + 4].copy_from_slice(&value);
            seal(&mut bytes);
            assert!(Replay::decode(&bytes).is_none());
        }
        let mut buttons = original.clone();
        buttons[40] = 255;
        seal(&mut buttons);
        assert!(Replay::decode(&buttons).is_none());
        let mut duplicate = original;
        duplicate[49..57].copy_from_slice(&0_u64.to_le_bytes());
        seal(&mut duplicate);
        assert!(Replay::decode(&duplicate).is_none());
    }
    #[test]
    fn rejects_forged_counts_and_trailing_bytes() {
        let original = recording().encode();
        let mut count = original.clone();
        count[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        seal(&mut count);
        assert!(Replay::decode(&count).is_none());
        let mut trailing = original;
        trailing.push(0);
        assert!(Replay::decode(&trailing).is_none());
        assert_eq!(
            Replay::decode(&Replay::default().encode())
                .unwrap()
                .frames()
                .len(),
            0
        );
    }
}

#[cfg(test)]
mod seek_tests {
    use super::*;
    #[test]
    fn sparse_ticks_and_u64_boundaries() {
        let mut r = Replay::default();
        assert!(r.frame_before_or_at(0).is_none());
        for tick in [2, 10, u64::MAX] {
            assert!(r.record(Frame {
                tick,
                input: InputFrame::default(),
                state_hash: tick
            }));
        }
        assert!(r.frame_at(3).is_none());
        assert!(r.frame_before_or_at(1).is_none());
        assert_eq!(r.frame_before_or_at(9).unwrap().tick, 2);
        assert_eq!(r.frame_at(10).unwrap().tick, 10);
        assert_eq!(r.frame_before_or_at(u64::MAX).unwrap().tick, u64::MAX);
    }
}
