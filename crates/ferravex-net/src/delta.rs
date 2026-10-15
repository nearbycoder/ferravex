//! Versioned byte snapshot differences. Callers acknowledge exact baselines before using a delta.
use crate::{MAX_PACKET_BYTES, ProtocolError};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delta {
    pub base_tick: u32,
    pub tick: u32,
    pub bytes: Vec<u8>,
}
impl Delta {
    pub fn between(
        base_tick: u32,
        base: &[u8],
        tick: u32,
        next: &[u8],
    ) -> Result<Self, ProtocolError> {
        if base.len() != next.len() || next.len() > MAX_PACKET_BYTES - 11 {
            return Err(ProtocolError::Invalid);
        }
        let mut changes = Vec::new();
        for (i, (&a, &b)) in base.iter().zip(next).enumerate() {
            if a != b {
                changes.extend_from_slice(&(i as u16).to_le_bytes());
                changes.push(b);
            }
        }
        // Full snapshot fallback avoids larger packets for dense changes.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(next.len() as u16).to_le_bytes());
        if changes.len() < next.len() {
            bytes.push(1);
            bytes.extend(changes)
        } else {
            bytes.push(0);
            bytes.extend(next)
        }
        Ok(Self {
            base_tick,
            tick,
            bytes,
        })
    }
    pub fn apply(&self, base_tick: u32, base: &[u8]) -> Result<Vec<u8>, ProtocolError> {
        if self.base_tick != base_tick
            || self.bytes.len() < 3
            || base.len() > MAX_PACKET_BYTES - 11
            || self.bytes.len() > MAX_PACKET_BYTES - 8
        {
            return Err(ProtocolError::Invalid);
        }
        let len = u16::from_le_bytes([self.bytes[0], self.bytes[1]]) as usize;
        if len != base.len() {
            return Err(ProtocolError::Invalid);
        }
        let body = &self.bytes[3..];
        match self.bytes[2] {
            0 if body.len() == len => Ok(body.to_vec()),
            1 if body.len().is_multiple_of(3) => {
                let mut result = base.to_vec();
                let mut previous = None;
                for entry in body.as_chunks::<3>().0 {
                    let i = u16::from_le_bytes([entry[0], entry[1]]) as usize;
                    if i >= len || previous.is_some_and(|p| i <= p) {
                        return Err(ProtocolError::Invalid);
                    }
                    previous = Some(i);
                    result[i] = entry[2];
                }
                Ok(result)
            }
            _ => Err(ProtocolError::Invalid),
        }
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
