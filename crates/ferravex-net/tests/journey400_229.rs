//! Task 229: retention authorization and real-codec composition, without sockets.
use ferravex_net::{
    ClientExtension, ClientPacket, MAX_PACKET_BYTES, PlayerSnapshot, ServerExtension, ServerPacket,
    Snapshot,
    baselines::{
        AckOutcome, ApplyError, BaselineError, BaselineLimits, FullReason, ReceiverBaselines,
        RetainOutcome, Selection, SenderBaselines,
    },
    decode_client, decode_server,
    delta::{Delta, DeltaError},
    encode_client, encode_server,
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
// SAFETY: Forward the exact allocations/layouts to System. Const-initialized
// thread-local Cells count only this test thread and do not allocate themselves.
unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: Layout is supplied under GlobalAlloc's caller contract.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: Layout is supplied under GlobalAlloc's caller contract.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        // SAFETY: Caller supplies the original allocation and valid new size.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Caller supplies the original allocation and layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountAlloc = CountAlloc;

fn limits(count: usize, bytes: usize) -> BaselineLimits {
    BaselineLimits {
        max_snapshots: count,
        max_bytes: bytes,
    }
}
fn scratch() -> Delta {
    Delta {
        base_tick: 90,
        tick: 91,
        bytes: Vec::with_capacity(MAX_PACKET_BYTES),
    }
}
fn snapshot(tick: u32) -> Snapshot {
    Snapshot {
        session: 7,
        tick,
        ..Snapshot::default()
    }
}
fn full(snapshot: &Snapshot) -> Vec<u8> {
    encode_server(&ServerPacket::Snapshot(snapshot.clone())).unwrap()
}

// Actual ACK codec; the application/driver gates the session before touching
// the store. No transport ACK or network delivery evidence is fabricated.
fn route_ack(sender: &mut SenderBaselines, wire: &[u8], session: u64) -> Option<AckOutcome> {
    match decode_client(wire).unwrap() {
        ClientPacket::Extension(ClientExtension::SnapshotAck {
            session: actual,
            tick,
        }) if actual == session => {
            if let Some(tick) = tick {
                Some(sender.acknowledge(tick))
            } else {
                sender.reset();
                None
            }
        }
        _ => None,
    }
}
fn installed_ack(
    sender: &mut SenderBaselines,
    receiver: &ReceiverBaselines,
    session: u64,
    tick: u32,
) -> AckOutcome {
    assert!(receiver.get(tick).is_some());
    let wire = encode_client(&ClientPacket::Extension(ClientExtension::SnapshotAck {
        session,
        tick: Some(tick),
    }))
    .unwrap();
    route_ack(sender, &wire, session).unwrap()
}
fn deliver_full(sender: &mut SenderBaselines, receiver: &mut ReceiverBaselines, value: &Snapshot) {
    let bytes = full(value);
    // This is an in-process delivery recipe, not socket or browser verification.
    let ServerPacket::Snapshot(decoded) = decode_server(&bytes).unwrap() else {
        panic!()
    };
    assert_eq!(decoded, *value);
    sender.record_sent(value.tick, &bytes).unwrap();
    receiver.install(decoded.tick, &bytes).unwrap();
    assert_eq!(
        installed_ack(sender, receiver, value.session, value.tick),
        AckOutcome::Accepted
    );
}
fn delta_wire(sender: &mut SenderBaselines, value: &Snapshot, delta: &mut Delta) -> Vec<u8> {
    let bytes = full(value);
    let Selection::Delta { packet_bytes, .. } =
        sender.select_into(value.tick, &bytes, delta, 24, MAX_PACKET_BYTES)
    else {
        panic!("delta should save wire bytes")
    };
    // Move the reusable allocation into/out of the owning outer packet enum.
    let packet = ServerPacket::Extension(ServerExtension::SnapshotDelta {
        session: value.session,
        delta: std::mem::replace(
            delta,
            Delta {
                base_tick: 0,
                tick: 0,
                bytes: Vec::new(),
            },
        ),
    });
    let wire = encode_server(&packet).unwrap();
    assert_eq!(wire.len(), packet_bytes);
    let ServerPacket::Extension(ServerExtension::SnapshotDelta {
        delta: returned, ..
    }) = packet
    else {
        panic!()
    };
    *delta = returned;
    sender.record_sent(value.tick, &bytes).unwrap();
    wire
}
fn deliver_delta(receiver: &mut ReceiverBaselines, wire: &[u8], expected: &Snapshot) {
    let ServerPacket::Extension(ServerExtension::SnapshotDelta { session, delta }) =
        decode_server(wire).unwrap()
    else {
        panic!()
    };
    assert_eq!(session, expected.session);
    let mut output = [0xA5; MAX_PACKET_BYTES];
    let n = receiver.apply_into(&delta, &mut output).unwrap();
    assert!(output[n..].iter().all(|&byte| byte == 0xA5));
    let ServerPacket::Snapshot(decoded) = decode_server(&output[..n]).unwrap() else {
        panic!()
    };
    assert_eq!((decoded.session, decoded.tick), (session, delta.tick));
    assert_eq!(decoded, *expected);
    // Scratch application has not installed or published anything yet.
    assert!(receiver.get(decoded.tick).is_none());
    receiver.install(decoded.tick, &output[..n]).unwrap();
}

#[test]
fn real_snapshot_delta_and_exact_installed_ack_select_the_next_baseline() {
    let mut sender = SenderBaselines::new(limits(4, 4 * MAX_PACKET_BYTES)).unwrap();
    let mut receiver = ReceiverBaselines::new(limits(4, 4 * MAX_PACKET_BYTES)).unwrap();
    let mut delta = scratch();
    deliver_full(&mut sender, &mut receiver, &snapshot(40));
    let wire = delta_wire(&mut sender, &snapshot(41), &mut delta);
    assert_eq!(sender.baseline().unwrap().0, 40); // Sending is not acknowledging.
    deliver_delta(&mut receiver, &wire, &snapshot(41));
    assert_eq!(sender.acknowledge(42), AckOutcome::Unknown); // Merely newer is no proof.
    assert_eq!(sender.baseline().unwrap().0, 40);
    assert_eq!(
        installed_ack(&mut sender, &receiver, 7, 41),
        AckOutcome::Accepted
    );
    assert_eq!(
        sender.baseline(),
        Some((41, full(&snapshot(41)).as_slice()))
    );
    let wire = delta_wire(&mut sender, &snapshot(42), &mut delta);
    assert_eq!(delta.base_tick, 41);
    deliver_delta(&mut receiver, &wire, &snapshot(42));
    assert_eq!(
        installed_ack(&mut sender, &receiver, 7, 42),
        AckOutcome::Accepted
    );
    assert_eq!(sender.acknowledge(40), AckOutcome::Stale);
    assert_eq!(sender.acknowledge(42), AckOutcome::AlreadyAcknowledged);
}

#[test]
fn receiver_lost_baseline_requests_real_full_recovery_then_delta_resumes() {
    let mut sender = SenderBaselines::new(limits(4, 4000)).unwrap();
    let mut receiver = ReceiverBaselines::new(limits(4, 4000)).unwrap();
    let mut delta = scratch();
    deliver_full(&mut sender, &mut receiver, &snapshot(10));
    let wire = delta_wire(&mut sender, &snapshot(11), &mut delta);
    receiver.reset(); // e.g. caller lost installed history.
    let ServerPacket::Extension(ServerExtension::SnapshotDelta { delta: decoded, .. }) =
        decode_server(&wire).unwrap()
    else {
        panic!()
    };
    let mut output = [55; MAX_PACKET_BYTES];
    assert_eq!(
        receiver.apply_into(&decoded, &mut output),
        Err(ApplyError::MissingBaseline { tick: 10 })
    );
    assert_eq!(output, [55; MAX_PACKET_BYTES]);
    assert!(receiver.is_empty());
    let request = encode_client(&ClientPacket::Extension(ClientExtension::SnapshotAck {
        session: 7,
        tick: None,
    }))
    .unwrap();
    assert_eq!(route_ack(&mut sender, &request, 7), None);
    assert_eq!(
        sender.select_into(12, &full(&snapshot(12)), &mut delta, 24, MAX_PACKET_BYTES),
        Selection::Full(FullReason::NoAcknowledgement)
    );
    deliver_full(&mut sender, &mut receiver, &snapshot(12));
    let wire = delta_wire(&mut sender, &snapshot(13), &mut delta);
    deliver_delta(&mut receiver, &wire, &snapshot(13));
    assert_eq!(
        installed_ack(&mut sender, &receiver, 7, 13),
        AckOutcome::Accepted
    );
}

#[test]
fn membership_length_change_uses_full_then_exact_new_length_delta() {
    let mut sender = SenderBaselines::new(limits(4, 4000)).unwrap();
    let mut receiver = ReceiverBaselines::new(limits(4, 4000)).unwrap();
    let mut delta = scratch();
    deliver_full(&mut sender, &mut receiver, &snapshot(20));
    let mut joined = snapshot(21);
    joined.players.push(PlayerSnapshot {
        id: 0,
        health: 100,
        ..PlayerSnapshot::default()
    });
    let bytes = full(&joined);
    assert_eq!(
        sender.select_into(21, &bytes, &mut delta, 24, MAX_PACKET_BYTES),
        Selection::Full(FullReason::LengthChanged {
            baseline: full(&snapshot(20)).len(),
            next: bytes.len(),
        })
    );
    deliver_full(&mut sender, &mut receiver, &joined);
    joined.tick = 22;
    joined.players[0].health = 90;
    let wire = delta_wire(&mut sender, &joined, &mut delta);
    deliver_delta(&mut receiver, &wire, &joined);
    assert_eq!(
        installed_ack(&mut sender, &receiver, 7, 22),
        AckOutcome::Accepted
    );
    let departed = snapshot(23);
    assert!(matches!(
        sender.select_into(23, &full(&departed), &mut delta, 24, MAX_PACKET_BYTES),
        Selection::Full(FullReason::LengthChanged { .. })
    ));
    deliver_full(&mut sender, &mut receiver, &departed);
}

#[test]
fn deterministic_count_and_byte_eviction_never_pin_acknowledged_bytes() {
    let mut sender = SenderBaselines::new(limits(3, 10)).unwrap();
    sender.record_sent(1, &[1; 3]).unwrap();
    sender.record_sent(2, &[2; 3]).unwrap();
    sender.record_sent(3, &[3; 3]).unwrap();
    assert_eq!(sender.acknowledge(2), AckOutcome::Accepted);
    assert_eq!(sender.record_sent(1, &[1; 3]), Ok(RetainOutcome::Duplicate));
    assert_eq!(sender.latest_tick(), Some(3));
    assert_eq!(
        sender.record_sent(4, &[4; 7]),
        Ok(RetainOutcome::Stored { evicted: 2 })
    );
    assert_eq!((sender.len(), sender.retained_bytes()), (2, 10));
    assert_eq!(sender.get(1), None);
    assert_eq!(sender.get(2), None);
    assert_eq!(sender.get(3), Some(&[3; 3][..]));
    assert_eq!(sender.get(4), Some(&[4; 7][..]));
    assert_eq!(sender.baseline(), None);
    assert_eq!(sender.acknowledge(2), AckOutcome::Unknown);
    assert_eq!(
        sender.select_into(5, &[5; 7], &mut scratch(), 0, 100),
        Selection::Full(FullReason::BaselineEvicted { tick: 2 })
    );
    assert_eq!(sender.acknowledge(4), AckOutcome::Accepted);
    assert_eq!(
        sender.record_sent(5, &[]),
        Ok(RetainOutcome::Stored { evicted: 0 })
    );
    assert_eq!(
        sender.record_sent(6, &[]),
        Ok(RetainOutcome::Stored { evicted: 1 })
    );
    assert_eq!(sender.get(3), None);
    assert_eq!((sender.len(), sender.retained_bytes()), (3, 7));
    // Count eviction is oldest-first even when removing an empty entry would
    // recover the same metadata capacity without discarding ACKed bytes.
    assert_eq!(
        sender.record_sent(7, &[]),
        Ok(RetainOutcome::Stored { evicted: 1 })
    );
    assert_eq!(sender.baseline(), None);
    assert_eq!(sender.retained_bytes(), 0);
}

#[test]
fn receiver_eviction_does_not_apply_against_a_merely_newer_snapshot() {
    let mut receiver = ReceiverBaselines::new(limits(1, 20)).unwrap();
    receiver.install(1, &[1; 10]).unwrap();
    receiver.install(2, &[2; 10]).unwrap();
    let delta = Delta::between(1, &[1; 10], 3, &[3; 10]).unwrap();
    let mut output = [5; 20];
    assert_eq!(
        receiver.apply_into(&delta, &mut output),
        Err(ApplyError::MissingBaseline { tick: 1 })
    );
    assert_eq!(output, [5; 20]);
    assert_eq!(
        (
            receiver.len(),
            receiver.retained_bytes(),
            receiver.latest_tick()
        ),
        (1, 10, Some(2))
    );
}

#[test]
fn capacity_conflicts_and_tick_errors_preserve_bytes_and_authorization() {
    assert!(matches!(
        SenderBaselines::new(limits(0, 100)),
        Err(BaselineError::ZeroSnapshotCapacity)
    ));
    assert!(matches!(
        ReceiverBaselines::new(limits(0, 0)),
        Err(BaselineError::ZeroSnapshotCapacity)
    ));
    assert!(matches!(
        SenderBaselines::new(limits(usize::MAX, 0)),
        Err(BaselineError::AllocationFailed)
    ));
    assert!(matches!(
        ReceiverBaselines::new(limits(1, usize::MAX)),
        Err(BaselineError::AllocationFailed)
    ));
    let mut sender = SenderBaselines::new(limits(2, 10)).unwrap();
    let mut receiver = ReceiverBaselines::new(limits(2, 10)).unwrap();
    sender.record_sent(7, &[7; 5]).unwrap();
    receiver.install(7, &[7; 5]).unwrap();
    assert_eq!(sender.acknowledge(7), AckOutcome::Accepted);
    for (tick, bytes, expected) in [
        (
            8,
            &[0; 11][..],
            BaselineError::SnapshotTooLarge {
                required: 11,
                max: 10,
            },
        ),
        (7, &[9; 5][..], BaselineError::ConflictingTick { tick: 7 }),
        (
            6,
            &[0; 5][..],
            BaselineError::TickNotNewer { tick: 6, latest: 7 },
        ),
        (
            7 + (1 << 31),
            &[0; 5][..],
            BaselineError::TickNotNewer {
                tick: 7 + (1 << 31),
                latest: 7,
            },
        ),
    ] {
        assert_eq!(sender.record_sent(tick, bytes), Err(expected));
        assert_eq!(receiver.install(tick, bytes), Err(expected));
        assert_eq!(sender.baseline(), Some((7, &[7; 5][..])));
        assert_eq!(receiver.get(7), Some(&[7; 5][..]));
        assert_eq!((sender.len(), sender.retained_bytes()), (1, 5));
        assert_eq!((receiver.len(), receiver.retained_bytes()), (1, 5));
        if tick != 7 {
            assert_eq!(sender.acknowledge(tick), AckOutcome::Unknown);
        }
    }
    // Rejection did not consume tick 8 or evict the retained bytes.
    assert_eq!(
        sender.record_sent(8, &[8; 5]),
        Ok(RetainOutcome::Stored { evicted: 0 })
    );
    assert_eq!(sender.acknowledge(8), AckOutcome::Accepted);
}

#[test]
fn tick_wrap_old_ack_and_retained_half_range_window_are_explicit() {
    let mut sender = SenderBaselines::new(limits(3, 30)).unwrap();
    let mut receiver = ReceiverBaselines::new(limits(3, 30)).unwrap();
    for tick in [u32::MAX - 1, u32::MAX, 0] {
        sender.record_sent(tick, &[0; 10]).unwrap();
        receiver.install(tick, &[0; 10]).unwrap();
    }
    assert_eq!(sender.acknowledge(u32::MAX), AckOutcome::Accepted);
    let mut delta = scratch();
    assert!(matches!(
        sender.select_into(1, &[0; 10], &mut delta, 0, 100),
        Selection::Delta {
            base_tick: u32::MAX,
            ..
        }
    ));
    let mut output = [5; 10];
    assert_eq!(receiver.apply_into(&delta, &mut output), Ok(10));
    assert_eq!(output, [0; 10]);
    assert_eq!(sender.acknowledge(0), AckOutcome::Accepted);
    assert_eq!(sender.acknowledge(u32::MAX - 1), AckOutcome::Stale);
    sender.record_sent(1, &[0; 10]).unwrap();
    assert_eq!(sender.acknowledge(u32::MAX - 1), AckOutcome::Unknown);
    assert_eq!(
        sender.record_sent(u32::MAX - 1, &[0; 10]),
        Err(BaselineError::TickNotNewer {
            tick: u32::MAX - 1,
            latest: 1
        })
    );
    sender.reset();
    sender.record_sent(0, &[0; 10]).unwrap();
    sender.record_sent((1 << 31) - 1, &[0; 10]).unwrap();
    assert_eq!(
        sender.record_sent(1 << 31, &[0; 10]),
        Err(BaselineError::TickWindow {
            tick: 1 << 31,
            oldest: 0
        })
    );
    assert_eq!(sender.len(), 2);
    assert!(sender.get(0).is_some());
}

#[test]
fn ack_generations_survive_more_than_a_full_tick_revolution() {
    let mut sender = SenderBaselines::new(limits(1, 1)).unwrap();
    sender.record_sent(0, &[0]).unwrap();
    assert_eq!(sender.acknowledge(0), AckOutcome::Accepted);
    // Each step is less than half-range; bounded eviction prevents retained
    // aliases. This is local ACK ordering, not identification of ancient ACKs:
    // the consumer must expire packets or rotate the external session epoch.
    for tick in [0x7fff_ffff, 0xffff_fffe, 0, 1] {
        sender.record_sent(tick, &[1]).unwrap();
    }
    assert_eq!(sender.baseline(), None);
    assert_eq!(sender.acknowledge(1), AckOutcome::Accepted);
    assert_eq!(sender.baseline(), Some((1, &[1][..])));
}

#[test]
fn varied_lengths_and_empty_entries_match_oldest_first_reference_retention() {
    use std::collections::VecDeque;
    for count_limit in 1..=5 {
        for byte_limit in [0, 1, 3, 8, 31] {
            let mut sender = SenderBaselines::new(limits(count_limit, byte_limit)).unwrap();
            let mut receiver = ReceiverBaselines::new(limits(count_limit, byte_limit)).unwrap();
            let mut reference: VecDeque<(u32, Vec<u8>)> = VecDeque::new();
            let mut reference_bytes = 0;
            let mut seed = 229u32;
            for tick in 0..256 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let bytes = vec![(seed >> 24) as u8; (seed as usize >> 16) % 35];
                if bytes.len() > byte_limit {
                    let error = BaselineError::SnapshotTooLarge {
                        required: bytes.len(),
                        max: byte_limit,
                    };
                    assert_eq!(sender.record_sent(tick, &bytes), Err(error));
                    assert_eq!(receiver.install(tick, &bytes), Err(error));
                    assert_eq!(sender.acknowledge(tick), AckOutcome::Unknown);
                } else {
                    let mut evicted = 0;
                    while reference.len() >= count_limit
                        || reference_bytes + bytes.len() > byte_limit
                    {
                        reference_bytes -= reference.pop_front().unwrap().1.len();
                        evicted += 1;
                    }
                    let expected = RetainOutcome::Stored { evicted };
                    assert_eq!(sender.record_sent(tick, &bytes), Ok(expected));
                    assert_eq!(receiver.install(tick, &bytes), Ok(expected));
                    reference_bytes += bytes.len();
                    reference.push_back((tick, bytes));
                    assert_eq!(sender.acknowledge(tick), AckOutcome::Accepted);
                }
                assert_eq!(
                    (sender.len(), sender.retained_bytes()),
                    (reference.len(), reference_bytes)
                );
                assert_eq!(
                    (receiver.len(), receiver.retained_bytes()),
                    (reference.len(), reference_bytes)
                );
                for &(retained_tick, ref bytes) in &reference {
                    assert_eq!(sender.get(retained_tick), Some(bytes.as_slice()));
                    assert_eq!(receiver.get(retained_tick), Some(bytes.as_slice()));
                }
                assert_eq!(sender.latest_tick(), reference.back().map(|entry| entry.0));
            }
        }
    }
}

#[test]
fn all_full_selection_reasons_preserve_caller_delta_and_store() {
    let mut sender = SenderBaselines::new(limits(2, 3000)).unwrap();
    let mut delta = scratch();
    delta.bytes.extend_from_slice(&[1, 2, 3]);
    let before = delta.clone();
    assert_eq!(
        sender.select_into(1, &[0; 100], &mut delta, 24, 1200),
        Selection::Full(FullReason::NoAcknowledgement)
    );
    sender.record_sent(1, &[0; 100]).unwrap();
    sender.acknowledge(1);
    for (tick, full, overhead, limit, expected) in [
        (
            1,
            &[0; 100][..],
            24,
            1200,
            FullReason::TickNotNewer {
                tick: 1,
                baseline: 1,
            },
        ),
        (
            1 + (1 << 31),
            &[0; 100][..],
            24,
            1200,
            FullReason::TickNotNewer {
                tick: 1 + (1 << 31),
                baseline: 1,
            },
        ),
        (
            2,
            &[0; 99][..],
            24,
            1200,
            FullReason::LengthChanged {
                baseline: 100,
                next: 99,
            },
        ),
        (2, &[1; 100][..], 24, 1200, FullReason::NotSmaller),
        (2, &[0; 100][..], 24, 26, FullReason::PacketLimit),
        (
            2,
            &[0; 100][..],
            usize::MAX,
            usize::MAX,
            FullReason::PacketLimit,
        ),
        (2, &[0; 100][..], 97, 1200, FullReason::NotSmaller), // Equal size.
    ] {
        assert_eq!(
            sender.select_into(tick, full, &mut delta, overhead, limit),
            Selection::Full(expected)
        );
        assert_eq!(delta, before);
        assert_eq!(sender.baseline(), Some((1, &[0; 100][..])));
    }
    let mut short = Delta {
        base_tick: 11,
        tick: 12,
        bytes: vec![],
    };
    assert_eq!(
        sender.select_into(2, &[0; 100], &mut short, 24, 1200),
        Selection::Full(FullReason::Delta(DeltaError::Capacity {
            required: 3,
            available: 0
        }))
    );
    assert_eq!(
        (
            short.base_tick,
            short.tick,
            short.bytes.len(),
            short.bytes.capacity()
        ),
        (11, 12, 0, 0)
    );
    // Exact packet limit succeeds; selection does not retain or ACK the new tick.
    assert_eq!(
        sender.select_into(2, &[0; 100], &mut delta, 24, 27),
        Selection::Delta {
            base_tick: 1,
            packet_bytes: 27
        }
    );
    assert_eq!(sender.acknowledge(2), AckOutcome::Unknown);
    assert_eq!(sender.len(), 1);
    sender.reset();
    let oversized = [0; MAX_PACKET_BYTES - 10];
    sender.record_sent(3, &oversized).unwrap();
    sender.acknowledge(3);
    let before = delta.clone();
    assert_eq!(
        sender.select_into(4, &oversized, &mut delta, 0, 3000),
        Selection::Full(FullReason::Delta(DeltaError::SnapshotTooLarge {
            len: oversized.len(),
            max: MAX_PACKET_BYTES - 11,
        }))
    );
    assert_eq!(delta, before);
}

#[test]
fn invalid_delta_lengths_ticks_ranges_and_capacity_never_install_or_write() {
    let mut receiver = ReceiverBaselines::new(limits(3, 100)).unwrap();
    receiver.install(7, &[0; 10]).unwrap();
    for (delta, expected) in [
        (
            Delta {
                base_tick: 6,
                tick: 8,
                bytes: vec![],
            },
            ApplyError::MissingBaseline { tick: 6 },
        ),
        (
            Delta {
                base_tick: 7,
                tick: 7,
                bytes: vec![],
            },
            ApplyError::TickNotNewer {
                tick: 7,
                previous: 7,
            },
        ),
        (
            Delta {
                base_tick: 7,
                tick: 7 + (1 << 31),
                bytes: vec![],
            },
            ApplyError::TickNotNewer {
                tick: 7 + (1 << 31),
                previous: 7,
            },
        ),
        (
            Delta {
                base_tick: 7,
                tick: 8,
                bytes: vec![9, 0, 1],
            },
            ApplyError::Delta(DeltaError::BaseLengthMismatch {
                expected: 9,
                actual: 10,
            }),
        ),
        // Late corrupt entry after a valid one must not partially write.
        (
            Delta {
                base_tick: 7,
                tick: 8,
                bytes: vec![10, 0, 1, 0, 0, 9, 10, 0, 8],
            },
            ApplyError::Delta(DeltaError::EntryOutOfRange {
                entry: 1,
                index: 10,
                len: 10,
            }),
        ),
        (
            Delta {
                base_tick: 7,
                tick: 8,
                bytes: vec![10, 0, 4],
            },
            ApplyError::Delta(DeltaError::InvalidTag { tag: 4 }),
        ),
    ] {
        let mut output = [42; 20];
        assert_eq!(receiver.apply_into(&delta, &mut output), Err(expected));
        assert_eq!(output, [42; 20]);
        assert_eq!(receiver.get(7), Some(&[0; 10][..]));
        assert_eq!(receiver.latest_tick(), Some(7));
        assert_eq!(receiver.len(), 1);
    }
    let delta = Delta::between(7, &[0; 10], 8, &[1; 10]).unwrap();
    let mut short = [42; 9];
    assert_eq!(
        receiver.apply_into(&delta, &mut short),
        Err(ApplyError::Delta(DeltaError::Capacity {
            required: 10,
            available: 9
        }))
    );
    assert_eq!(short, [42; 9]);
    let mut output = [42; 12];
    assert_eq!(receiver.apply_into(&delta, &mut output), Ok(10));
    assert_eq!(output, [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 42, 42]);
    assert_eq!(receiver.latest_tick(), Some(7));
    assert_eq!(receiver.get(8), None);
}

#[test]
fn session_reconnect_requires_external_epoch_gate_and_explicit_resets() {
    let mut sender = SenderBaselines::new(limits(2, 2400)).unwrap();
    let mut receiver = ReceiverBaselines::new(limits(2, 2400)).unwrap();
    deliver_full(&mut sender, &mut receiver, &snapshot(5));
    let old_ack = encode_client(&ClientPacket::Extension(ClientExtension::SnapshotAck {
        session: 7,
        tick: Some(5),
    }))
    .unwrap();
    sender.reset();
    receiver.reset();
    assert!(sender.is_empty() && receiver.is_empty());
    assert_eq!((sender.latest_tick(), receiver.latest_tick()), (None, None));
    assert_eq!((sender.retained_bytes(), receiver.retained_bytes()), (0, 0));
    assert_eq!(sender.baseline(), None);
    assert_eq!(sender.acknowledge(5), AckOutcome::Unknown);
    let new = Snapshot {
        session: 8,
        ..snapshot(5)
    };
    let bytes = full(&new);
    sender.record_sent(5, &bytes).unwrap();
    // A reused tick alone cannot authenticate an old ACK. The real outer
    // session field is checked by the consumer before calling acknowledge.
    assert_eq!(route_ack(&mut sender, &old_ack, 8), None);
    assert_eq!(sender.baseline(), None);
    assert_eq!(decode_server(&bytes).unwrap(), ServerPacket::Snapshot(new));
    receiver.install(5, &bytes).unwrap();
    assert_eq!(
        installed_ack(&mut sender, &receiver, 8, 5),
        AckOutcome::Accepted
    );
    assert_eq!(sender.baseline(), Some((5, bytes.as_slice())));
}

#[test]
fn zero_byte_budget_and_empty_snapshots_still_obey_count_and_reset() {
    let mut sender = SenderBaselines::new(limits(2, 0)).unwrap();
    let mut receiver = ReceiverBaselines::new(limits(2, 0)).unwrap();
    for tick in 0..3 {
        let expected = RetainOutcome::Stored {
            evicted: usize::from(tick == 2),
        };
        assert_eq!(sender.record_sent(tick, &[]), Ok(expected));
        assert_eq!(receiver.install(tick, &[]), Ok(expected));
    }
    assert_eq!(sender.get(0), None);
    assert_eq!(receiver.get(0), None);
    assert_eq!(sender.acknowledge(1), AckOutcome::Accepted);
    assert_eq!(sender.baseline(), Some((1, &[][..])));
    assert_eq!(
        receiver.install(3, &[0]),
        Err(BaselineError::SnapshotTooLarge {
            required: 1,
            max: 0
        })
    );
    assert_eq!((sender.len(), receiver.len()), (2, 2));
    sender.reset();
    receiver.reset();
    assert_eq!(
        sender.record_sent(0, &[]),
        Ok(RetainOutcome::Stored { evicted: 0 })
    );
    assert_eq!(
        receiver.install(0, &[]),
        Ok(RetainOutcome::Stored { evicted: 0 })
    );
}

#[test]
fn warmed_retention_eviction_ack_selection_apply_errors_and_reset_allocate_zero() {
    let mut sender = SenderBaselines::new(limits(3, 48)).unwrap();
    let mut receiver = ReceiverBaselines::new(limits(3, 48)).unwrap();
    let mut delta = scratch();
    let delta_pointer = delta.bytes.as_ptr();
    let delta_capacity = delta.bytes.capacity();
    let mut output = [99; 32];
    ALLOCATIONS.with(|count| count.set(0));
    TRACK.with(|track| track.set(true));
    for _ in 0..8 {
        for tick in 0..64 {
            let mut bytes = [0; 16];
            bytes[0] = tick as u8;
            if tick != 0 {
                assert!(matches!(
                    sender.select_into(tick, &bytes, &mut delta, 0, 100),
                    Selection::Delta { .. }
                ));
                assert_eq!(receiver.apply_into(&delta, &mut output), Ok(16));
                assert_eq!(&output[..16], bytes);
            }
            sender.record_sent(tick, &bytes).unwrap();
            receiver.install(tick, &bytes).unwrap();
            assert_eq!(sender.acknowledge(tick), AckOutcome::Accepted);
            assert_eq!(sender.acknowledge(tick + 1), AckOutcome::Unknown);
            assert!(sender.record_sent(tick + 1, &[0; 49]).is_err());
            assert!(receiver.install(tick + 1, &[0; 49]).is_err());
            assert_eq!(
                sender.record_sent(tick, &bytes),
                Ok(RetainOutcome::Duplicate)
            );
            assert_eq!(receiver.install(tick, &bytes), Ok(RetainOutcome::Duplicate));
            assert!(sender.len() <= 3 && sender.retained_bytes() <= 48);
        }
        sender.reset();
        receiver.reset();
    }
    TRACK.with(|track| track.set(false));
    assert_eq!(ALLOCATIONS.with(Cell::get), 0);
    assert_eq!(delta.bytes.as_ptr(), delta_pointer);
    assert_eq!(delta.bytes.capacity(), delta_capacity);
    assert_eq!(output[16..], [99; 16]);
}
