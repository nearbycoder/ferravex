//! Task 313: checked delta buffers and actual v5 envelope compatibility.
use ferravex_net::{
    MAX_PACKET_BYTES, ProtocolError, ServerExtension, ServerPacket, decode_server,
    delta::{Delta, DeltaError},
    encode_server,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountAlloc;
thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
fn count_allocation() {
    if TRACK.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
    }
}
// SAFETY: Every operation forwards the exact layout and pointer to System.
// Counting uses const-initialized thread-local Cells and never allocates.
unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: The caller provides GlobalAlloc's required valid layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: The caller provides GlobalAlloc's required valid layout.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        // SAFETY: The caller supplies a System allocation and valid new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The caller supplies the original System pointer and layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountAlloc = CountAlloc;

fn delta(bytes: &[u8]) -> Delta {
    Delta {
        base_tick: 7,
        tick: 8,
        bytes: bytes.to_vec(),
    }
}

#[test]
fn sparse_dense_zero_and_max_sizes_match_exact_legacy_wire() {
    let mut storage = Delta {
        base_tick: 0,
        tick: 0,
        bytes: Vec::with_capacity(MAX_PACKET_BYTES),
    };
    for len in [0, 1, 2, 3, 4, 10, MAX_PACKET_BYTES - 11] {
        let base = vec![7; len];
        for changed in [0, len / 3, (len / 3 + 1).min(len), len] {
            let mut next = base.clone();
            next[..changed].fill(9);
            let mut expected = (len as u16).to_le_bytes().to_vec();
            let sparse = changed * 3 < len;
            expected.push(u8::from(sparse));
            if sparse {
                for index in 0..changed {
                    expected.extend_from_slice(&(index as u16).to_le_bytes());
                    expected.push(9);
                }
            } else {
                expected.extend_from_slice(&next);
            }
            assert_eq!(Delta::encoded_len(&base, &next), Ok(expected.len()));
            let mut encoded = [0xA5; MAX_PACKET_BYTES];
            let n = Delta::encode_into(&base, &next, &mut encoded).unwrap();
            assert_eq!(&encoded[..n], expected);
            assert!(encoded[n..].iter().all(|&v| v == 0xA5));
            storage.between_into(u32::MAX, &base, 0, &next).unwrap();
            assert_eq!(storage.bytes, expected);
            assert_eq!(storage, Delta::between(u32::MAX, &base, 0, &next).unwrap());
            assert_eq!(storage.applied_len(u32::MAX, &base), Ok(len));
            let mut output = [0x5A; MAX_PACKET_BYTES];
            assert_eq!(storage.apply_into(u32::MAX, &base, &mut output), Ok(len));
            assert_eq!(&output[..len], next);
            assert!(output[len..].iter().all(|&v| v == 0x5A));
            assert_eq!(storage.apply(u32::MAX, &base).unwrap(), next);
        }
    }
}

#[test]
fn all_small_change_patterns_and_high_index_sparse_changes_roundtrip() {
    for len in 0..=8 {
        let base = vec![0; len];
        for mask in 0..1usize << len {
            let next: Vec<_> = (0..len).map(|i| u8::from(mask & (1 << i) != 0)).collect();
            let mut bytes = [0; 32];
            let n = Delta::encode_into(&base, &next, &mut bytes).unwrap();
            let d = delta(&bytes[..n]);
            let mut output = [0; 8];
            assert_eq!(d.apply_into(7, &base, &mut output), Ok(len));
            assert_eq!(&output[..len], next);
        }
    }
    let base = [0; MAX_PACKET_BYTES - 11];
    let mut next = base;
    next[MAX_PACKET_BYTES - 12] = 9;
    let d = Delta::between(7, &base, 8, &next).unwrap();
    assert_eq!(d.bytes, [0xA5, 4, 1, 0xA4, 4, 9]);
    let mut output = base;
    assert_eq!(d.apply_into(7, &base, &mut output), Ok(base.len()));
    assert_eq!(output, next);
}

#[test]
fn encode_rejections_preserve_slice_and_delta_including_ticks_and_capacity() {
    let mut output = [42; 3];
    assert_eq!(
        Delta::encode_into(&[0], &[], &mut output),
        Err(DeltaError::LengthMismatch { base: 1, next: 0 })
    );
    let too_big = [0; MAX_PACKET_BYTES - 10];
    assert_eq!(
        Delta::encoded_len(&too_big, &too_big),
        Err(DeltaError::SnapshotTooLarge {
            len: too_big.len(),
            max: MAX_PACKET_BYTES - 11
        })
    );
    assert_eq!(
        Delta::encode_into(&too_big, &too_big, &mut output),
        Err(DeltaError::SnapshotTooLarge {
            len: too_big.len(),
            max: MAX_PACKET_BYTES - 11
        })
    );
    assert_eq!(
        Delta::encode_into(&[0], &[1], &mut output),
        Err(DeltaError::Capacity {
            required: 4,
            available: 3
        })
    );
    assert_eq!(output, [42; 3]);

    let mut d = delta(&[0, 0, 0]);
    let before = d.clone();
    let capacity = d.bytes.capacity();
    let pointer = d.bytes.as_ptr();
    // Vec capacity, rather than length, is the exact available bound.
    let dense = vec![1; capacity + 1];
    let base = vec![0; dense.len()];
    assert_eq!(
        d.between_into(20, &base, 21, &dense),
        Err(DeltaError::Capacity {
            required: dense.len() + 3,
            available: capacity
        })
    );
    assert_eq!(
        d.between_into(20, &[0], 21, &[]),
        Err(DeltaError::LengthMismatch { base: 1, next: 0 })
    );
    assert_eq!(
        d.between_into(20, &too_big, 21, &too_big),
        Err(DeltaError::SnapshotTooLarge {
            len: too_big.len(),
            max: MAX_PACKET_BYTES - 11
        })
    );
    assert_eq!(d, before);
    assert_eq!(d.bytes.capacity(), capacity);
    assert_eq!(d.bytes.as_ptr(), pointer);
}

fn assert_rejected(d: &Delta, tick: u32, base: &[u8], error: DeltaError) {
    let mut output = [42; MAX_PACKET_BYTES + 1];
    assert_eq!(d.applied_len(tick, base), Err(error));
    assert_eq!(d.apply_into(tick, base, &mut output), Err(error));
    assert_eq!(output, [42; MAX_PACKET_BYTES + 1]);
    // Malformed data takes precedence over insufficient output capacity, too.
    assert_eq!(d.apply_into(tick, base, &mut []), Err(error));
    assert_eq!(d.apply(tick, base), Err(ProtocolError::Invalid));
}

#[test]
fn apply_diagnostics_cover_limits_ticks_headers_tags_and_body_lengths() {
    assert_rejected(
        &delta(&[]),
        6,
        &[],
        DeltaError::BaseTickMismatch {
            expected: 7,
            actual: 6,
        },
    );
    for len in 0..3 {
        assert_rejected(
            &delta(&[0; 3][..len]),
            7,
            &[],
            DeltaError::TruncatedHeader { len },
        );
    }
    let base = [0; MAX_PACKET_BYTES - 10];
    assert_rejected(
        &delta(&[0, 0, 0]),
        7,
        &base,
        DeltaError::SnapshotTooLarge {
            len: base.len(),
            max: MAX_PACKET_BYTES - 11,
        },
    );
    let oversized = delta(&[0; MAX_PACKET_BYTES - 7]);
    assert_rejected(
        &oversized,
        7,
        &[],
        DeltaError::EncodedTooLarge {
            len: MAX_PACKET_BYTES - 7,
            max: MAX_PACKET_BYTES - 8,
        },
    );
    // Exactly the old encoded bound is permitted, including noncanonical sparse data.
    let mut at_limit = vec![0; MAX_PACKET_BYTES - 8];
    at_limit[..3].copy_from_slice(&[(MAX_PACKET_BYTES - 11) as u8, 4, 0]);
    let d = delta(&at_limit);
    assert_eq!(
        d.applied_len(7, &[0; MAX_PACKET_BYTES - 11]),
        Ok(MAX_PACKET_BYTES - 11)
    );
    assert_rejected(
        &delta(&[1, 0, 0, 9]),
        7,
        &[],
        DeltaError::BaseLengthMismatch {
            expected: 1,
            actual: 0,
        },
    );
    for tag in [2, 127, 255] {
        assert_rejected(&delta(&[0, 0, tag]), 7, &[], DeltaError::InvalidTag { tag });
    }
    for body_len in [0, 1, 3] {
        let mut bytes = vec![2, 0, 0];
        bytes.resize(3 + body_len, 9);
        assert_rejected(
            &delta(&bytes),
            7,
            &[0; 2],
            DeltaError::DenseLengthMismatch {
                expected: 2,
                actual: body_len,
            },
        );
    }
    for body_len in [1, 2, 4, 5] {
        let mut bytes = vec![10, 0, 1];
        bytes.resize(3 + body_len, 0);
        assert_rejected(
            &delta(&bytes),
            7,
            &[0; 10],
            DeltaError::TruncatedEntry { body_len },
        );
    }
}

#[test]
fn late_corrupt_ranges_duplicates_and_descending_entries_never_mutate() {
    for index in [10u16, u16::MAX] {
        let [lo, hi] = index.to_le_bytes();
        let d = delta(&[10, 0, 1, 1, 0, 9, lo, hi, 8]);
        assert_rejected(
            &d,
            7,
            &[0; 10],
            DeltaError::EntryOutOfRange {
                entry: 1,
                index: index as usize,
                len: 10,
            },
        );
    }
    for index in [0, 5] {
        let d = delta(&[10, 0, 1, 5, 0, 9, index, 0, 8]);
        assert_rejected(
            &d,
            7,
            &[0; 10],
            DeltaError::EntryOrder {
                entry: 1,
                previous: 5,
                index: index as usize,
            },
        );
    }
    assert_rejected(
        &delta(&[0, 0, 1, 0, 0, 9]),
        7,
        &[],
        DeltaError::EntryOutOfRange {
            entry: 0,
            index: 0,
            len: 0,
        },
    );
}

#[test]
fn exact_capacity_success_short_capacity_failure_and_zero_output() {
    let base = [0; 10];
    let next = [1; 10];
    let required = Delta::encoded_len(&base, &next).unwrap();
    let mut bytes = vec![42; required];
    assert_eq!(Delta::encode_into(&base, &next, &mut bytes), Ok(required));
    let d = delta(&bytes);
    let mut output = [42; 9];
    assert_eq!(
        d.apply_into(7, &base, &mut output),
        Err(DeltaError::Capacity {
            required: 10,
            available: 9
        })
    );
    assert_eq!(output, [42; 9]);
    let mut output = [42; 10];
    assert_eq!(d.apply_into(7, &base, &mut output), Ok(10));
    assert_eq!(output, next);
    let mut short = vec![42; required - 1];
    assert_eq!(
        Delta::encode_into(&base, &next, &mut short),
        Err(DeltaError::Capacity {
            required,
            available: required - 1
        })
    );
    assert_eq!(short, vec![42; required - 1]);
    for tag in [0, 1] {
        assert_eq!(delta(&[0, 0, tag]).apply_into(7, &[], &mut []), Ok(0));
    }
    assert_eq!(Delta::encode_into(&[], &[], &mut [0; 3]), Ok(3));
    let mut empty = Delta {
        base_tick: 0,
        tick: 0,
        bytes: Vec::new(),
    };
    assert_eq!(
        empty.between_into(7, &[], 8, &[]),
        Err(DeltaError::Capacity {
            required: 3,
            available: 0
        })
    );
    assert_eq!(empty.base_tick, 0);
    assert!(empty.bytes.is_empty());
}

#[test]
fn legacy_noncanonical_sparse_values_and_arbitrary_ticks_are_preserved() {
    // Sparse can be larger than dense, include unchanged values, or have no entries.
    for bytes in [&[1, 0, 1, 0, 0, 7][..], &[1, 0, 1][..]] {
        let mut d = delta(bytes);
        d.tick = d.base_tick;
        assert_eq!(d.apply(7, &[7]).unwrap(), [7]);
        assert_eq!(d.apply_into(7, &[7], &mut [0]), Ok(1));
    }
    let mut d = Delta::between(7, &[7], 7, &[7]).unwrap();
    d.between_into(8, &[7], 7, &[7]).unwrap();
    assert_eq!((d.base_tick, d.tick), (8, 7));
}

#[test]
fn warmed_storage_reuses_allocation_across_dense_sparse_empty_and_failures() {
    let base = [0; MAX_PACKET_BYTES - 11];
    let dense = [1; MAX_PACKET_BYTES - 11];
    let mut sparse = base;
    sparse[base.len() - 1] = 9;
    let mut d = Delta {
        base_tick: 0,
        tick: 0,
        bytes: Vec::with_capacity(MAX_PACKET_BYTES),
    };
    let pointer = d.bytes.as_ptr();
    let capacity = d.bytes.capacity();
    let mut encoded = [0; MAX_PACKET_BYTES];
    let mut output = [0; MAX_PACKET_BYTES];
    // Warm both TLS cells before measuring only this test thread's operations.
    ALLOCATIONS.with(|n| n.set(0));
    TRACK.with(|enabled| enabled.set(true));
    for _ in 0..32 {
        for next in [&dense[..], &sparse[..], &base[..], &[][..]] {
            let base = &base[..next.len()];
            let n = Delta::encoded_len(base, next).unwrap();
            assert_eq!(Delta::encode_into(base, next, &mut encoded), Ok(n));
            d.between_into(7, base, 8, next).unwrap();
            assert_eq!(d.bytes, encoded[..n]);
            assert_eq!(d.applied_len(7, base), Ok(next.len()));
            assert_eq!(d.apply_into(7, base, &mut output), Ok(next.len()));
            assert_eq!(&output[..next.len()], next);
            assert!(d.apply_into(6, base, &mut output).is_err());
            assert!(d.between_into(9, base, 10, &[0; 2]).is_err());
        }
    }
    TRACK.with(|enabled| enabled.set(false));
    assert_eq!(
        ALLOCATIONS.with(Cell::get),
        0,
        "alloc/realloc/alloc_zeroed calls"
    );
    assert_eq!(d.bytes.as_ptr(), pointer);
    assert_eq!(d.bytes.capacity(), capacity);
    assert_eq!(d.bytes, [0, 0, 0]); // Empty reset leaves no old dense/sparse bytes visible.
}

#[test]
fn v5_envelope_bytes_and_corrupt_inner_decoder_handoff_are_unchanged() {
    let base = [0; 10];
    let mut next = base;
    next[9] = 9;
    let mut d = Delta {
        base_tick: 0,
        tick: 0,
        bytes: Vec::with_capacity(6),
    };
    d.between_into(7, &base, 8, &next).unwrap();
    let packet = ServerPacket::Extension(ServerExtension::SnapshotDelta {
        session: 2,
        delta: d,
    });
    let wire = encode_server(&packet).unwrap();
    assert_eq!(
        wire,
        [
            b'F', b'V', b'R', b'X', 5, 11, 2, 0, 0, 0, 0, 0, 0, 0, // session
            7, 0, 0, 0, 8, 0, 0, 0, // ticks
            6, 0, 10, 0, 1, 9, 0, 9, // inner length and sparse delta
        ]
    );
    assert_eq!(decode_server(&wire).unwrap(), packet);
    let mut corrupt = wire;
    corrupt[27] = 10; // Out-of-range sparse index: outer decoder retains the inner bytes.
    let ServerPacket::Extension(ServerExtension::SnapshotDelta { delta: d, .. }) =
        decode_server(&corrupt).unwrap()
    else {
        panic!("wrong packet kind")
    };
    assert_rejected(
        &d,
        7,
        &base,
        DeltaError::EntryOutOfRange {
            entry: 0,
            index: 10,
            len: 10,
        },
    );
}

#[test]
fn arbitrary_inner_bytes_are_bounded_and_never_partially_write_on_error() {
    let mut state = 0x12345678u32;
    for len in 0..=MAX_PACKET_BYTES + 1 {
        let mut bytes: Vec<_> = (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        if len >= 3 {
            bytes[..3].copy_from_slice(&[10, 0, (len % 3) as u8]);
        }
        let d = delta(&bytes);
        let mut output = [42; 12];
        if d.apply_into(7, &[0; 10], &mut output).is_err() {
            assert_eq!(output, [42; 12]);
        } else {
            assert_eq!(output[10..], [42; 2]);
            assert_eq!(d.apply(7, &[0; 10]).unwrap(), output[..10]);
        }
    }
}
