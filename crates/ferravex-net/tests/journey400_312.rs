//! Task 312: bounded receipt, ordered destination ownership and backpressure.
use ferravex_net::reliable::{
    DeliveryReport, EVENT_BYTES, EVENT_LIMIT, Error, EventPacket, EventReceiver, EventSender,
    Receipt,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn packet(sequence: u64, payload: &[u8]) -> EventPacket {
    EventPacket {
        sequence,
        payload: payload.to_vec(),
    }
}
fn report(
    delivered: usize,
    payload_bytes: usize,
    remaining_ready: usize,
    buffered: usize,
) -> DeliveryReport {
    DeliveryReport {
        delivered,
        payload_bytes,
        remaining_ready,
        buffered,
    }
}

#[test]
fn lost_first_retry_receipt_ack_and_destination_pressure_are_independent() {
    let (mut sender, mut receiver) = (EventSender::default(), EventReceiver::default());
    for bytes in [b"first".as_slice(), b"second", b"third"] {
        sender.queue(bytes).unwrap();
    }
    assert_eq!(sender.send_due(1_000, 1, 5, |_| true).sent, 1); // Lost zero.
    let mut acks = [None; 3];
    sender.send_due(1_001, 3, 99, |p| {
        assert_eq!(receiver.admit(p, EVENT_LIMIT), Ok(Receipt::Retained));
        acks[p.sequence as usize] = Some(p.sequence);
        true
    });
    for ack in acks.iter_mut().filter_map(Option::take) {
        assert!(sender.acknowledge(ack));
    }
    assert_eq!(sender.pending(), 1);
    assert!(!sender.acknowledge(999));
    assert_eq!(receiver.drain_into(&mut [], usize::MAX), report(0, 0, 0, 2));
    assert_eq!(sender.send_due(1_099, 3, 99, |_| unreachable!()).sent, 0);
    sender.send_due(1_100, 1, 5, |p| {
        assert_eq!(receiver.admit(p, 2), Err(Error::Full)); // Never ACK refusal.
        assert_eq!(receiver.admit(p, 3), Ok(Receipt::Retained));
        acks[0] = Some(p.sequence);
        true
    });
    assert!(sender.acknowledge(acks[0].take().unwrap()));
    assert_eq!(sender.pending(), 0); // Receiver still owns every undelivered byte.
    let mut slot = [None];
    assert_eq!(receiver.drain_into(&mut slot, 5), report(1, 5, 2, 2));
    assert_eq!(receiver.drain_into(&mut slot, 99), report(0, 0, 2, 2));
    assert_eq!(slot[0].take().unwrap(), packet(0, b"first"));
    assert_eq!(receiver.drain_into(&mut slot, 6), report(1, 6, 1, 1));
    assert_eq!(slot[0].take().unwrap(), packet(1, b"second"));
    assert_eq!(receiver.drain_into(&mut slot, 5), report(1, 5, 0, 0));
    assert_eq!(slot[0].take().unwrap(), packet(2, b"third"));
}

#[test]
fn capacity_rejections_and_future_retries_never_advance_admission_state() {
    let mut receiver = EventReceiver::default();
    assert_eq!(receiver.admit(&packet(0, b"a"), 0), Err(Error::Full));
    assert_eq!((receiver.next_sequence(), receiver.buffered()), (0, 0));
    assert_eq!(receiver.admit(&packet(1, b"b"), 1), Ok(Receipt::Retained));
    assert_eq!(receiver.admit(&packet(0, b"a"), 1), Err(Error::Full));
    assert_eq!(
        receiver.admit(&packet(1, b"conflict"), 0),
        Ok(Receipt::Duplicate)
    );
    for _ in 0..3 {
        assert_eq!(
            receiver.admit(&packet(EVENT_LIMIT as u64, b"future"), 0),
            Err(Error::OutsideWindow)
        );
        assert_eq!((receiver.next_sequence(), receiver.buffered()), (0, 1));
    }
    assert_eq!(receiver.admit(&packet(0, b"a"), 2), Ok(Receipt::Retained));
    assert_eq!(
        receiver.drain_ready(1, 1, |s, b| s == 0 && b == b"a"),
        report(1, 1, 1, 1)
    );
    assert_eq!(
        receiver.admit(&packet(EVENT_LIMIT as u64, b"future"), 2),
        Ok(Receipt::Retained)
    );
    // Admission window tracks delivery only, even after repeated future attempts.
    assert_eq!(receiver.next_sequence(), 1);
    assert_eq!(
        receiver.drain_ready(9, 1, |s, b| s == 1 && b == b"b"),
        report(1, 1, 0, 1)
    );
}

#[test]
fn full_forward_window_retains_lost_head_and_never_exceeds_bounds() {
    let mut receiver = EventReceiver::default();
    for sequence in (1..EVENT_LIMIT as u64).rev() {
        assert_eq!(
            receiver.admit(&packet(sequence, &[7; EVENT_BYTES]), usize::MAX),
            Ok(Receipt::Retained)
        );
    }
    assert_eq!(
        receiver.drain_ready(usize::MAX, usize::MAX, |_, _| unreachable!()),
        report(0, 0, 0, EVENT_LIMIT - 1)
    );
    assert_eq!(
        receiver.admit(&packet(EVENT_LIMIT as u64, &[]), usize::MAX),
        Err(Error::OutsideWindow)
    );
    assert_eq!(
        receiver.admit(&packet(0, &[]), usize::MAX),
        Ok(Receipt::Retained)
    );
    assert_eq!(receiver.buffered(), EVENT_LIMIT);
    assert_eq!(
        receiver.admit(&packet(128, b"other"), 0),
        Ok(Receipt::Duplicate)
    );
    let mut expected = 0;
    assert_eq!(
        receiver.drain_ready(usize::MAX, usize::MAX, |s, b| {
            assert_eq!(s, expected);
            if s == 0 {
                assert!(b.is_empty());
            } else {
                assert_eq!(b, [7; EVENT_BYTES]);
            }
            expected += 1;
            true
        }),
        report(EVENT_LIMIT, (EVENT_LIMIT - 1) * EVENT_BYTES, 0, 0)
    );
    assert_eq!(receiver.next_sequence(), EVENT_LIMIT as u64);
}

#[test]
fn output_prefix_stops_at_occupied_slots_and_preserves_exact_tail() {
    let mut receiver = EventReceiver::default();
    for s in 0..4 {
        receiver.admit(&packet(s, &[s as u8]), 4).unwrap();
    }
    let mut output = [
        None,
        Some(packet(500, b"occupied")),
        None,
        Some(packet(501, b"tail")),
    ];
    assert_eq!(receiver.drain_into(&mut output, 99), report(1, 1, 3, 3));
    assert_eq!(
        output,
        [
            Some(packet(0, &[0])),
            Some(packet(500, b"occupied")),
            None,
            Some(packet(501, b"tail"))
        ]
    );
    assert_eq!(receiver.drain_into(&mut output, 99), report(0, 0, 3, 3));
    output[0] = None;
    output[1] = None;
    assert_eq!(receiver.drain_into(&mut output, 2), report(2, 2, 1, 1));
    assert_eq!(output[0], Some(packet(1, &[1])));
    assert_eq!(output[1], Some(packet(2, &[2])));
    assert!(output[2].is_none());
    assert_eq!(output[3], Some(packet(501, b"tail")));
    assert_eq!(receiver.next_sequence(), 3);
    assert!(receiver.contains_buffered(3));
}

#[test]
fn empty_and_exact_byte_budgets_never_skip_a_large_head() {
    let mut receiver = EventReceiver::default();
    for (s, b) in [(0, &[][..]), (1, &[8; EVENT_BYTES][..]), (2, &[][..])] {
        receiver.admit(&packet(s, b), EVENT_LIMIT).unwrap();
    }
    let mut output = [None, None, None];
    assert_eq!(
        receiver.drain_into(&mut output[..0], usize::MAX),
        report(0, 0, 3, 3)
    );
    assert_eq!(receiver.drain_into(&mut output, 0), report(1, 0, 2, 2));
    output[0] = None;
    assert_eq!(
        receiver.drain_into(&mut output, EVENT_BYTES - 1),
        report(0, 0, 2, 2)
    );
    assert_eq!(
        receiver.drain_ready(0, usize::MAX, |_, _| unreachable!()),
        report(0, 0, 2, 2)
    );
    assert_eq!(
        receiver.drain_ready(3, EVENT_BYTES - 1, |_, _| unreachable!()),
        report(0, 0, 2, 2)
    );
    assert_eq!(
        receiver.drain_into(&mut output[..1], EVENT_BYTES),
        report(1, EVENT_BYTES, 1, 1)
    );
    assert_eq!(output[0].as_ref().unwrap().payload, [8; EVENT_BYTES]);
    assert_eq!(
        receiver.drain_ready(1, 0, |s, b| s == 2 && b.is_empty()),
        report(1, 0, 0, 0)
    );
}

#[test]
fn callback_refusal_keeps_head_and_tail_until_explicit_acceptance() {
    let mut receiver = EventReceiver::default();
    for s in 0..4 {
        receiver.admit(&packet(s, &[s as u8]), 4).unwrap();
    }
    let mut visited = Vec::new();
    assert_eq!(
        receiver.drain_ready(4, 4, |s, _| {
            visited.push(s);
            s == 0
        }),
        report(1, 1, 3, 3)
    );
    assert_eq!(visited, [0, 1]);
    assert_eq!(receiver.next_sequence(), 1);
    assert_eq!(
        receiver.admit(&packet(1, b"changed"), 0),
        Ok(Receipt::Duplicate)
    );
    let mut output = [None, None];
    assert_eq!(receiver.drain_into(&mut output, 2), report(2, 2, 1, 1));
    assert_eq!(output, [Some(packet(1, &[1])), Some(packet(2, &[2]))]);
    assert_eq!(
        receiver.drain_ready(99, 1, |s, _| s == 3),
        report(1, 1, 0, 0)
    );
}

#[test]
fn native_unwind_commits_prior_acceptances_only() {
    let mut receiver = EventReceiver::default();
    for s in 0..4 {
        receiver.admit(&packet(s, &[s as u8]), 4).unwrap();
    }
    let mut external_effects = Vec::new();
    let panic = catch_unwind(AssertUnwindSafe(|| {
        receiver.drain_ready(4, 4, |s, _| {
            external_effects.push(s);
            assert_ne!(s, 1, "injected consumer panic");
            true
        })
    }));
    assert!(panic.is_err());
    assert_eq!(external_effects, [0, 1]); // Side effects do not roll back.
    assert_eq!((receiver.next_sequence(), receiver.buffered()), (1, 3));
    assert!(!receiver.contains_buffered(0));
    assert!(receiver.contains_buffered(1));
    assert_eq!(
        receiver.drain_ready(4, 4, |s, _| {
            external_effects.push(s);
            true
        }),
        report(3, 3, 0, 0)
    );
    assert_eq!(external_effects, [0, 1, 1, 2, 3]);
}

#[test]
fn conflicting_duplicates_are_first_wins_before_and_after_delivery() {
    let mut receiver = EventReceiver::default();
    let original = packet(0, b"original");
    assert_eq!(receiver.admit(&original, 1), Ok(Receipt::Retained));
    assert_eq!(receiver.admit(&original, 0), Ok(Receipt::Duplicate));
    assert_eq!(
        receiver.admit(&packet(0, b"different"), 0),
        Ok(Receipt::Duplicate)
    );
    let mut out = [None];
    receiver.drain_into(&mut out, 99);
    assert_eq!(out[0], Some(original));
    assert_eq!(
        receiver.admit(&packet(0, b"different again"), 0),
        Ok(Receipt::Duplicate)
    );
    assert_eq!(
        receiver.drain_ready(4, 99, |_, _| unreachable!()),
        report(0, 0, 0, 0)
    );
    assert_eq!(
        receiver.admit(&packet(0, &[1; EVENT_BYTES + 1]), 0),
        Err(Error::Oversized)
    );
}

#[test]
fn malformed_errors_have_documented_precedence_and_never_drain() {
    let mut receiver = EventReceiver::default();
    receiver.admit(&packet(0, b"retained"), 1).unwrap();
    for sequence in [0, 1, 256, u64::MAX] {
        let oversized = packet(sequence, &[0; EVENT_BYTES + 1]);
        assert_eq!(receiver.admit(&oversized, 0), Err(Error::Oversized));
        assert_eq!(receiver.receive(&oversized), Err(Error::Oversized));
        assert_eq!((receiver.next_sequence(), receiver.buffered()), (0, 1));
    }
    assert_eq!(
        receiver.admit(&packet(u64::MAX, &[]), 0),
        Err(Error::OutsideWindow)
    );
    assert_eq!(
        receiver.receive(&packet(256, &[])),
        Err(Error::OutsideWindow)
    );
    assert!(receiver.contains_buffered(0));
    assert_eq!(receiver.admit(&packet(1, &[]), 0), Err(Error::Full));
}

#[test]
fn legacy_receive_mixing_uses_the_same_queue_and_once_only_cursor() {
    let mut receiver = EventReceiver::default();
    for s in [2, 1, 0] {
        receiver.admit(&packet(s, &[s as u8]), 3).unwrap();
    }
    assert_eq!(
        receiver.drain_ready(1, 1, |s, _| s == 0),
        report(1, 1, 2, 2)
    );
    // A delivered duplicate admits successfully, then legacy receive drains ready work.
    assert_eq!(
        receiver.receive(&packet(0, b"ignored")),
        Ok(vec![packet(1, &[1]), packet(2, &[2])])
    );
    assert_eq!(receiver.receive(&packet(2, b"ignored")), Ok(vec![]));
    assert_eq!(receiver.receive(&packet(4, b"four")), Ok(vec![]));
    receiver.admit(&packet(3, b"three"), 2).unwrap();
    let mut output = [None];
    assert_eq!(receiver.drain_into(&mut output, 5), report(1, 5, 1, 1));
    assert_eq!(
        receiver.receive(&packet(4, b"conflict")),
        Ok(vec![packet(4, b"four")])
    );
    assert_eq!(receiver.next_sequence(), 5);
    assert_eq!(output[0], Some(packet(3, b"three")));
}

#[test]
fn default_replacement_discards_old_receipts_at_external_session_boundary() {
    let mut receiver = EventReceiver::default();
    receiver.admit(&packet(0, b"old"), 1).unwrap();
    receiver = EventReceiver::default(); // Caller must reject stale session packets.
    assert_eq!((receiver.next_sequence(), receiver.buffered()), (0, 0));
    assert_eq!(receiver.drain_into(&mut [], 99), report(0, 0, 0, 0));
    receiver.admit(&packet(0, b"new"), 1).unwrap();
    assert_eq!(
        receiver.receive(&packet(0, b"old duplicate")),
        Ok(vec![packet(0, b"new")])
    );
    receiver = EventReceiver::default();
    assert_eq!(receiver.next_sequence(), 0);
    assert_eq!(
        receiver.receive(&packet(0, b"next session")),
        Ok(vec![packet(0, b"next session")])
    );
}

struct CountAlloc;
thread_local! {
    static TRACK: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
fn count_allocation() {
    if TRACK.try_with(Cell::get).unwrap_or(false) {
        let _ = ALLOCATIONS.try_with(|n| n.set(n.get() + 1));
    }
}
// SAFETY: Each operation forwards the unchanged arguments to System; const TLS
// counters measure this test thread only and do not allocate.
unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: Caller supplies the valid allocation layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: Caller supplies the valid allocation layout.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        // SAFETY: Caller supplies an existing System allocation and valid size.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Caller supplies this allocation's original pointer/layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountAlloc = CountAlloc;
struct TrackingGuard;
impl Drop for TrackingGuard {
    fn drop(&mut self) {
        TRACK.with(|t| t.set(false));
    }
}
fn track() -> TrackingGuard {
    ALLOCATIONS.with(|n| n.set(0));
    TRACK.with(|t| t.set(true));
    TrackingGuard
}

#[test]
fn warmed_receipts_and_drains_allocate_nothing_but_initial_admission_does() {
    let packets: Vec<_> = (0..EVENT_LIMIT as u64)
        .map(|s| packet(s, &[8; EVENT_BYTES]))
        .collect();
    let mut receiver = EventReceiver::default();
    let guard = track();
    for p in &packets {
        receiver.admit(p, EVENT_LIMIT).unwrap();
    }
    drop(guard);
    assert!(ALLOCATIONS.with(Cell::get) >= EVENT_LIMIT); // Owned payload copies are real costs.
    let mut pointers = [std::ptr::null(); EVENT_LIMIT];
    receiver.drain_ready(1, EVENT_BYTES, |s, b| {
        pointers[s as usize] = b.as_ptr();
        false
    });
    // Capture remaining storage pointers without consuming (one head per receiver
    // cannot expose its tail); output pointer check below proves the head moves.
    let mut slots: [Option<EventPacket>; EVENT_LIMIT] = std::array::from_fn(|_| None);
    let guard = track();
    for _ in 0..32 {
        for p in &packets {
            assert_eq!(receiver.admit(p, 0), Ok(Receipt::Duplicate));
        }
        assert_eq!(
            receiver.drain_into(&mut [], usize::MAX),
            report(0, 0, EVENT_LIMIT, EVENT_LIMIT)
        );
        assert_eq!(
            receiver.drain_into(&mut slots, EVENT_BYTES - 1).delivered,
            0
        );
        assert_eq!(
            receiver
                .drain_ready(0, usize::MAX, |_, _| unreachable!())
                .delivered,
            0
        );
        assert_eq!(
            receiver
                .drain_ready(9, EVENT_BYTES - 1, |_, _| unreachable!())
                .delivered,
            0
        );
        assert_eq!(
            receiver.drain_ready(9, usize::MAX, |_, _| false).delivered,
            0
        );
    }
    assert_eq!(
        receiver.drain_into(&mut slots[..128], 128 * EVENT_BYTES),
        report(128, 128 * EVENT_BYTES, 128, 128)
    );
    assert_eq!(slots[0].as_ref().unwrap().payload.as_ptr(), pointers[0]);
    assert_eq!(
        receiver.drain_ready(usize::MAX, usize::MAX, |_, b| b == [8; EVENT_BYTES]),
        report(128, 128 * EVENT_BYTES, 0, 0)
    );
    drop(guard);
    assert_eq!(
        ALLOCATIONS.with(Cell::get),
        0,
        "alloc/alloc_zeroed/realloc calls after admission"
    );
}
