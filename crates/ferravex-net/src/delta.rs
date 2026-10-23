//! Versioned byte snapshot differences. Callers acknowledge exact baselines before using a delta.
//!
//! The inner bytes remain `[length: u16 LE, tag: u8, body...]`: tag 0 stores
//! the full snapshot, tag 1 stores ordered `[index: u16 LE, value: u8]` entries.
//! Construction chooses sparse only when its body is strictly smaller. The
//! checked buffer methods allocate nothing, validate before writing, and leave
//! unused destination tails untouched. Safe Rust requires disjoint input and
//! output borrows; these methods do not support overlapping/in-place application.
//! No method authenticates a baseline's contents, session, ACK or tick ordering.
//!
//! This runnable acknowledged-base recipe uses the actual v5 snapshot, ACK and
//! delta packet codecs. It models one already-installed baseline, not a retention
//! or ACK-authorization policy. Outer packet codecs still allocate and enforce
//! their own tighter size and tick-order limits; fall back to a full snapshot if
//! delta construction or packet encoding fails in a real sender.
//!
//! ```
//! use ferravex_net::{
//!     ClientExtension, ClientPacket, ServerExtension, ServerPacket, Snapshot,
//!     MAX_PACKET_BYTES, decode_client, decode_server, encode_client, encode_server,
//!     delta::Delta,
//! };
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let baseline = Snapshot { session: 7, tick: 40, ..Snapshot::default() };
//!     let base = encode_server(&ServerPacket::Snapshot(baseline.clone()))?;
//!     // The receiver validates and installs the full snapshot before ACKing it.
//!     assert_eq!(decode_server(&base)?, ServerPacket::Snapshot(baseline.clone()));
//!     let ack = encode_client(&ClientPacket::Extension(ClientExtension::SnapshotAck {
//!         session: baseline.session, tick: Some(baseline.tick),
//!     }))?;
//!     let ClientPacket::Extension(ClientExtension::SnapshotAck {
//!         session: 7, tick: Some(40),
//!     }) = decode_client(&ack)? else { panic!("unexpected ACK") };
//!     // The sender selects these exact retained bytes for that acknowledged tick.
//!     let mut next = baseline;
//!     next.tick = 41;
//!     let full = encode_server(&ServerPacket::Snapshot(next.clone()))?;
//!     let mut delta = Delta { base_tick: 0, tick: 0,
//!         bytes: Vec::with_capacity(MAX_PACKET_BYTES) };
//!     delta.between_into(40, &base, 41, &full)?;
//!     let packet = encode_server(&ServerPacket::Extension(
//!         ServerExtension::SnapshotDelta { session: 7, delta },
//!     ))?;
//!     let ServerPacket::Extension(ServerExtension::SnapshotDelta {
//!         session: 7, delta,
//!     }) = decode_server(&packet)? else { panic!("unexpected delta") };
//!     let mut output = [0; MAX_PACKET_BYTES];
//!     let len = delta.applied_len(40, &base)?;
//!     assert_eq!(delta.apply_into(40, &base, &mut output)?, len);
//!     let ServerPacket::Snapshot(recovered) = decode_server(&output[..len])?
//!         else { panic!("unexpected reconstructed packet") };
//!     assert_eq!(recovered.session, 7);
//!     assert_eq!(recovered.tick, delta.tick);
//!     assert_eq!(recovered, next); // Publish/ACK only after these checks.
//!     Ok(())
//! }
//! ```
use crate::{MAX_PACKET_BYTES, ProtocolError};

const MAX_SNAPSHOT_BYTES: usize = MAX_PACKET_BYTES - 11;
const MAX_DELTA_BYTES: usize = MAX_PACKET_BYTES - 8;

/// Exact rejection from the checked delta APIs. All indices are zero based and
/// lengths/capacities are bytes. Legacy methods map every rejection to
/// [`ProtocolError::Invalid`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeltaError {
    LengthMismatch {
        base: usize,
        next: usize,
    },
    SnapshotTooLarge {
        len: usize,
        max: usize,
    },
    Capacity {
        required: usize,
        available: usize,
    },
    /// Expected is the delta's base tick; actual is the supplied base tick.
    BaseTickMismatch {
        expected: u32,
        actual: u32,
    },
    TruncatedHeader {
        len: usize,
    },
    EncodedTooLarge {
        len: usize,
        max: usize,
    },
    /// Expected is the encoded snapshot length; actual is the supplied base length.
    BaseLengthMismatch {
        expected: usize,
        actual: usize,
    },
    InvalidTag {
        tag: u8,
    },
    DenseLengthMismatch {
        expected: usize,
        actual: usize,
    },
    /// Sparse body length is not a multiple of the three-byte entry size.
    TruncatedEntry {
        body_len: usize,
    },
    EntryOutOfRange {
        entry: usize,
        index: usize,
        len: usize,
    },
    EntryOrder {
        entry: usize,
        previous: usize,
        index: usize,
    },
}
impl std::fmt::Display for DeltaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid Ferravex delta: {self:?}")
    }
}
impl std::error::Error for DeltaError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delta {
    pub base_tick: u32,
    pub tick: u32,
    pub bytes: Vec<u8>,
}
impl Delta {
    /// Plan the exact inner wire length without allocating. Equal lengths and
    /// the historical snapshot limit (`MAX_PACKET_BYTES - 11`) are checked before
    /// counting changes, so all size arithmetic is bounded. This does not plan
    /// the outer `ServerExtension::SnapshotDelta` envelope (24 more bytes).
    ///
    /// ```
    /// use ferravex_net::delta::{Delta, DeltaError};
    /// assert_eq!(Delta::encoded_len(&[0; 10], &[1; 10]), Ok(13));
    /// assert_eq!(Delta::encoded_len(&[0; 10], &[0; 10]), Ok(3));
    /// assert_eq!(Delta::encoded_len(&[0], &[]),
    ///     Err(DeltaError::LengthMismatch { base: 1, next: 0 }));
    /// ```
    pub fn encoded_len(base: &[u8], next: &[u8]) -> Result<usize, DeltaError> {
        if base.len() != next.len() {
            return Err(DeltaError::LengthMismatch {
                base: base.len(),
                next: next.len(),
            });
        }
        if next.len() > MAX_SNAPSHOT_BYTES {
            return Err(DeltaError::SnapshotTooLarge {
                len: next.len(),
                max: MAX_SNAPSHOT_BYTES,
            });
        }
        let changes = base.iter().zip(next).filter(|(a, b)| a != b).count();
        Ok(3 + (changes * 3).min(next.len()))
    }

    /// Encode just [`Delta::bytes`] into a caller slice, returning its written
    /// prefix length. Snapshot validation precedes capacity validation. Every
    /// error preserves the entire destination; success preserves the tail.
    /// Uses the slice's length, not any backing Vec's capacity.
    ///
    /// ```
    /// use ferravex_net::delta::Delta;
    /// let mut output = [99; 20];
    /// let n = Delta::encode_into(&[0; 10], &[1; 10], &mut output).unwrap();
    /// assert_eq!(&output[..n], Delta::between(1, &[0; 10], 2, &[1; 10]).unwrap().bytes);
    /// assert_eq!(&output[n..], &[99; 7]);
    /// ```
    pub fn encode_into(
        base: &[u8],
        next: &[u8],
        destination: &mut [u8],
    ) -> Result<usize, DeltaError> {
        let required = Self::encoded_len(base, next)?;
        check_capacity(required, destination.len())?;
        encode_validated(base, next, &mut destination[..required]);
        Ok(required)
    }

    /// Replace this delta using its existing Vec capacity, with no allocation.
    /// On error, ticks, bytes, length and capacity are unchanged. On success,
    /// bytes are replaced (never appended); unused Vec capacity is not scrubbed.
    /// Reserve outside the hot path. This deliberately allows any tick pair,
    /// matching [`Self::between`]; the outer wire codec checks serial ordering.
    ///
    /// ```
    /// use ferravex_net::delta::Delta;
    /// let mut delta = Delta { base_tick: 0, tick: 0, bytes: Vec::with_capacity(20) };
    /// delta.between_into(7, &[0; 10], 8, &[1; 10]).unwrap();
    /// delta.between_into(8, &[1; 10], 9, &[1; 10]).unwrap();
    /// assert_eq!(delta.bytes, [10, 0, 1]); // Dense storage was replaced by sparse.
    /// assert_eq!(delta.apply(8, &[1; 10]).unwrap(), [1; 10]);
    /// ```
    pub fn between_into(
        &mut self,
        base_tick: u32,
        base: &[u8],
        tick: u32,
        next: &[u8],
    ) -> Result<(), DeltaError> {
        let required = Self::encoded_len(base, next)?;
        check_capacity(required, self.bytes.capacity())?;
        self.bytes.resize(required, 0);
        encode_validated(base, next, &mut self.bytes);
        self.base_tick = base_tick;
        self.tick = tick;
        Ok(())
    }

    pub fn between(
        base_tick: u32,
        base: &[u8],
        tick: u32,
        next: &[u8],
    ) -> Result<Self, ProtocolError> {
        let required = Self::encoded_len(base, next).map_err(|_| ProtocolError::Invalid)?;
        let mut bytes = vec![0; required];
        encode_validated(base, next, &mut bytes);
        Ok(Self {
            base_tick,
            tick,
            bytes,
        })
    }
    /// Validate the entire delta and return the reconstructed snapshot length,
    /// without allocation or writes. Checks run in this order: base tick,
    /// header availability, base limit, encoded limit, encoded/base length,
    /// tag/body shape, then each sparse range and order. No canonical sparse
    /// density or value-change requirement is imposed, preserving legacy apply.
    /// See the module's runnable snapshot recipe and [`Self::apply_into`].
    pub fn applied_len(&self, base_tick: u32, base: &[u8]) -> Result<usize, DeltaError> {
        if self.base_tick != base_tick {
            return Err(DeltaError::BaseTickMismatch {
                expected: self.base_tick,
                actual: base_tick,
            });
        }
        if self.bytes.len() < 3 {
            return Err(DeltaError::TruncatedHeader {
                len: self.bytes.len(),
            });
        }
        if base.len() > MAX_SNAPSHOT_BYTES {
            return Err(DeltaError::SnapshotTooLarge {
                len: base.len(),
                max: MAX_SNAPSHOT_BYTES,
            });
        }
        if self.bytes.len() > MAX_DELTA_BYTES {
            return Err(DeltaError::EncodedTooLarge {
                len: self.bytes.len(),
                max: MAX_DELTA_BYTES,
            });
        }
        let len = u16::from_le_bytes([self.bytes[0], self.bytes[1]]) as usize;
        if len != base.len() {
            return Err(DeltaError::BaseLengthMismatch {
                expected: len,
                actual: base.len(),
            });
        }
        let body = &self.bytes[3..];
        match self.bytes[2] {
            0 if body.len() != len => {
                return Err(DeltaError::DenseLengthMismatch {
                    expected: len,
                    actual: body.len(),
                });
            }
            0 => {}
            1 => {
                if !body.len().is_multiple_of(3) {
                    return Err(DeltaError::TruncatedEntry {
                        body_len: body.len(),
                    });
                }
                let mut previous = None;
                for (entry, bytes) in body.as_chunks::<3>().0.iter().enumerate() {
                    let index = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
                    if index >= len {
                        return Err(DeltaError::EntryOutOfRange { entry, index, len });
                    }
                    if let Some(previous) = previous
                        && index <= previous
                    {
                        return Err(DeltaError::EntryOrder {
                            entry,
                            previous,
                            index,
                        });
                    }
                    previous = Some(index);
                }
            }
            tag => return Err(DeltaError::InvalidTag { tag }),
        }
        Ok(len)
    }

    /// Apply into a caller slice without allocating. Complete validation via
    /// [`Self::applied_len`] precedes destination capacity checking and all writes.
    /// Every error preserves the entire destination, even a late corrupt entry.
    /// Success replaces only the returned prefix; the tail is untouched. Input
    /// and destination must be disjoint. Matching ticks alone cannot verify the
    /// baseline contents: select the exact retained bytes at the session layer.
    ///
    /// ```
    /// use ferravex_net::delta::{Delta, DeltaError};
    /// let delta = Delta::between(4, &[0; 10], 5, &[1; 10]).unwrap();
    /// let mut output = [99; 12];
    /// assert_eq!(delta.apply_into(3, &[0; 10], &mut output),
    ///     Err(DeltaError::BaseTickMismatch { expected: 4, actual: 3 }));
    /// assert_eq!(output, [99; 12]);
    /// assert_eq!(delta.apply_into(4, &[0; 10], &mut output), Ok(10));
    /// assert_eq!(output, [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 99, 99]);
    /// ```
    pub fn apply_into(
        &self,
        base_tick: u32,
        base: &[u8],
        destination: &mut [u8],
    ) -> Result<usize, DeltaError> {
        let required = self.applied_len(base_tick, base)?;
        check_capacity(required, destination.len())?;
        self.apply_validated(base, &mut destination[..required]);
        Ok(required)
    }

    pub fn apply(&self, base_tick: u32, base: &[u8]) -> Result<Vec<u8>, ProtocolError> {
        let required = self
            .applied_len(base_tick, base)
            .map_err(|_| ProtocolError::Invalid)?;
        let mut result = vec![0; required];
        self.apply_validated(base, &mut result);
        Ok(result)
    }

    fn apply_validated(&self, base: &[u8], destination: &mut [u8]) {
        let body = &self.bytes[3..];
        if self.bytes[2] == 0 {
            destination.copy_from_slice(body);
        } else {
            destination.copy_from_slice(base);
            for entry in body.as_chunks::<3>().0 {
                let index = u16::from_le_bytes([entry[0], entry[1]]) as usize;
                destination[index] = entry[2];
            }
        }
    }
}

fn check_capacity(required: usize, available: usize) -> Result<(), DeltaError> {
    if required > available {
        Err(DeltaError::Capacity {
            required,
            available,
        })
    } else {
        Ok(())
    }
}

// Only called after size planning. The exact output length also selects the
// original strictly-smaller sparse representation, including dense empty data.
fn encode_validated(base: &[u8], next: &[u8], destination: &mut [u8]) {
    let sparse = destination.len() - 3 < next.len();
    destination[..2].copy_from_slice(&(next.len() as u16).to_le_bytes());
    destination[2] = u8::from(sparse);
    if sparse {
        let mut offset = 3;
        for (index, (&a, &b)) in base.iter().zip(next).enumerate() {
            if a != b {
                destination[offset..offset + 2].copy_from_slice(&(index as u16).to_le_bytes());
                destination[offset + 2] = b;
                offset += 3;
            }
        }
    } else {
        destination[3..].copy_from_slice(next);
    }
}
/// Centimeter precision for a fixed world range; invalid or out-of-range values fail explicitly.
pub fn quantize_position(value: f32) -> Option<i32> {
    if !value.is_finite() || value.abs() > 32768.0 {
        return None;
    }
    Some((value * 100.0).round() as i32)
}
pub fn dequantize_position(value: i32) -> Option<f32> {
    (-3_276_800..=3_276_800)
        .contains(&value)
        .then_some(value as f32 / 100.0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn acknowledged_baseline_is_required_and_sparse_data_shrinks() {
        let base = vec![0; 1000];
        let mut next = base.clone();
        next[20] = 4;
        next[999] = 9;
        let delta = Delta::between(1, &base, 2, &next).unwrap();
        assert!(delta.bytes.len() < 20);
        assert_eq!(delta.apply(1, &base).unwrap(), next);
        assert!(delta.apply(0, &base).is_err());
        let dense = Delta::between(1, &base, 2, &vec![4; 1000]).unwrap();
        assert_eq!(dense.bytes.len(), 1003);
        assert_eq!(dense.apply(1, &base).unwrap(), vec![4; 1000]);
    }
    #[test]
    fn representation_boundaries_preserve_wire_bytes_and_packet_limit() {
        for len in [0, 1, 2, 3, 4, 10, MAX_PACKET_BYTES - 11] {
            let base = vec![7; len];
            for changed in [0, len / 3, (len / 3 + 1).min(len), len] {
                let mut next = base.clone();
                next[..changed].fill(9);
                let delta = Delta::between(3, &base, 4, &next).unwrap();
                assert_eq!(delta.apply(3, &base).unwrap(), next);
                let sparse = changed * 3 < len;
                assert_eq!(delta.bytes[2], u8::from(sparse));
                let mut expected = (len as u16).to_le_bytes().to_vec();
                expected.push(u8::from(sparse));
                if sparse {
                    for i in 0..changed {
                        expected.extend_from_slice(&(i as u16).to_le_bytes());
                        expected.push(9);
                    }
                } else {
                    expected.extend_from_slice(&next);
                }
                assert_eq!(delta.bytes, expected);
                assert!(delta.bytes.len() <= len + 3);
            }
        }
        assert!(
            Delta::between(
                0,
                &[0; MAX_PACKET_BYTES - 10],
                1,
                &[0; MAX_PACKET_BYTES - 10]
            )
            .is_err()
        );
    }
    #[test]
    fn malformed_deltas_never_write_out_of_bounds() {
        for bytes in [
            vec![1, 0, 1, 2, 0, 9],
            vec![1, 0, 1, 0, 0, 1, 0, 0, 2],
            vec![1, 0, 2],
            vec![],
        ] {
            assert!(
                Delta {
                    base_tick: 0,
                    tick: 1,
                    bytes
                }
                .apply(0, &[0])
                .is_err()
            );
        }
        assert!(quantize_position(f32::NAN).is_none());
        assert!(
            (dequantize_position(quantize_position(12.345).unwrap()).unwrap() - 12.345).abs()
                <= 0.0051
        );
    }
}
