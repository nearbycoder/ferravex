//! Task 311: borrowed sends, explicit acceptance and exact payload budgets.
use ferravex_net::reliable::{
    EVENT_BYTES, EVENT_LIMIT, Error, EventPacket, EventReceiver, EventSender, SendReport,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};

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
// SAFETY: All operations forward their exact arguments to System. Const TLS
// cells count only this thread's allocations and do not allocate themselves.
unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: GlobalAlloc's caller supplies a valid layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: GlobalAlloc's caller supplies a valid layout.
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_allocation();
        // SAFETY: Caller supplies an existing System allocation and valid size.
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: Caller supplies the allocation's original pointer and layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountAlloc = CountAlloc;

fn report(sent: usize, payload_bytes: usize, remaining_due: usize) -> SendReport {
    SendReport {
        sent,
        payload_bytes,
        remaining_due,
    }
}

#[test]
fn packet_and_payload_budgets_preserve_every_unsent_tail() {
    let mut sender = EventSender::default();
    for bytes in [&b"ab"[..], &b"cde"[..], &b"f"[..]] {
        sender.queue(bytes).unwrap();
    }
    assert_eq!(
        sender.send_due(10, 0, 99, |_| unreachable!()),
        report(0, 0, 3)
    );
    assert_eq!(
        sender.send_due(10, 3, 0, |_| unreachable!()),
        report(0, 0, 3)
    );
    let first = sender.send_due(10, 3, 4, |p| {
        assert_eq!(p.sequence, 0);
        assert_eq!(p.payload, b"ab");
        true
    });
    assert_eq!(first, report(1, 2, 2));
    // The smaller last packet cannot jump ahead of the over-budget due head.
    assert_eq!(
        sender.send_due(10, 3, 2, |_| unreachable!()),
        report(0, 0, 2)
    );
    assert_eq!(
        sender.send_due(10, 1, 4, |p| p.sequence == 1),
        report(1, 3, 1)
    );
    assert_eq!(
        sender.send_due(10, 3, 1, |p| p.sequence == 2),
        report(1, 1, 0)
    );
    assert_eq!(sender.pending(), 3); // All still await their individual ACKs.
    assert_eq!(
        sender.send_due(109, 3, 99, |_| unreachable!()),
        report(0, 0, 0)
    );
    assert_eq!(sender.send_due(110, 3, 6, |_| true), report(3, 6, 0));
}

#[test]
fn empty_payload_and_oversized_due_head_have_explicit_budget_behavior() {
    let mut sender = EventSender::default();
    sender.queue(&[]).unwrap();
    sender.queue(&[7; EVENT_BYTES]).unwrap();
    sender.queue(&[]).unwrap();
    assert_eq!(
        sender.send_due(0, 0, 0, |_| unreachable!()),
        report(0, 0, 3)
    );
    assert_eq!(
        sender.send_due(0, 3, 0, |p| p.sequence == 0),
        report(1, 0, 2)
    );
    assert_eq!(
        sender.send_due(0, usize::MAX, EVENT_BYTES - 1, |_| unreachable!()),
        report(0, 0, 2)
    );
    // Exact payload size fits: no hidden sequence/length/envelope charge.
    assert_eq!(
        sender.send_due(0, 1, EVENT_BYTES, |p| p.sequence == 1),
        report(1, EVENT_BYTES, 1)
    );
    assert_eq!(
        sender.send_due(0, 1, 0, |p| p.sequence == 2),
        report(1, 0, 0)
    );
    assert_eq!(
        sender.send_due(100, 3, EVENT_BYTES, |_| true),
        report(3, EVENT_BYTES, 0)
    );
}

#[test]
fn refusal_is_not_acceptance_and_tail_is_immediately_visible_to_legacy_due() {
    let mut sender = EventSender::default();
    for n in 0..4 {
        sender.queue(&[n]).unwrap();
    }
    let mut visited = Vec::new();
    assert_eq!(
        sender.send_due(50, 4, 4, |p| {
            visited.push(p.sequence);
            p.sequence < 1
        }),
        report(1, 1, 3)
    );
    assert_eq!(visited, [0, 1]);
    assert_eq!(sender.pending(), 4);
    let tail = sender.due(50, 4);
    assert_eq!(
        tail.iter().map(|p| p.sequence).collect::<Vec<_>>(),
        [1, 2, 3]
    );
    assert_eq!(
        sender.send_due(149, 4, 4, |_| unreachable!()),
        report(0, 0, 0)
    );
    assert_eq!(sender.send_due(150, 4, 4, |_| false), report(0, 0, 4));
    assert_eq!(sender.due(150, 4).len(), 4);
}

#[test]
fn callback_panic_keeps_accepted_prefix_and_preserves_unsent_tail() {
    let mut sender = EventSender::default();
    for n in 0..4 {
        sender.queue(&[n]).unwrap();
    }
    let failure = catch_unwind(AssertUnwindSafe(|| {
        sender.send_due(700, 4, 4, |p| {
            assert_ne!(p.sequence, 1, "injected callback panic");
            true
        })
    }));
    assert!(failure.is_err());
    assert_eq!(sender.pending(), 4);
    assert_eq!(
        sender.send_due(700, 0, 0, |_| unreachable!()),
        report(0, 0, 3)
    );
    let mut tail = Vec::new();
    assert_eq!(
        sender.send_due(700, 4, 4, |p| {
            tail.push(p.sequence);
            true
        }),
        report(3, 3, 0)
    );
    assert_eq!(tail, [1, 2, 3]);
    assert_eq!(
        sender.send_due(799, 4, 4, |_| unreachable!()),
        report(0, 0, 0)
    );
    assert_eq!(sender.due(800, 4).len(), 4);
}

#[test]
fn panic_on_retry_does_not_advance_its_previous_timestamp() {
    let mut sender = EventSender::default();
    sender.queue(b"x").unwrap();
    assert_eq!(sender.send_due(0, 1, 1, |_| true), report(1, 1, 0));
    assert!(
        catch_unwind(AssertUnwindSafe(|| {
            sender.send_due(100, 1, 1, |_| panic!("retry refusal by panic"))
        }))
        .is_err()
    );
    assert_eq!(sender.send_due(100, 1, 1, |_| true), report(1, 1, 0));
    assert_eq!(
        sender.send_due(199, 1, 1, |_| unreachable!()),
        report(0, 0, 0)
    );
    assert_eq!(sender.send_due(200, 1, 1, |_| true), report(1, 1, 0));
}

#[test]
fn full_window_sending_does_not_free_capacity_and_ack_is_individual() {
    let mut sender = EventSender::default();
    for i in 0..EVENT_LIMIT {
        assert_eq!(sender.queue(&[i as u8; EVENT_BYTES]), Ok(i as u64));
    }
    assert_eq!(sender.queue(&[]), Err(Error::Full));
    assert_eq!(sender.queue(&[0; EVENT_BYTES + 1]), Err(Error::Oversized));
    assert_eq!(
        sender.send_due(0, usize::MAX, usize::MAX, |_| false),
        report(0, 0, EVENT_LIMIT)
    );
    assert_eq!(
        sender.send_due(0, usize::MAX, usize::MAX, |_| true),
        report(EVENT_LIMIT, EVENT_LIMIT * EVENT_BYTES, 0)
    );
    assert_eq!(sender.queue(&[]), Err(Error::Full));
    assert!(!sender.acknowledge(EVENT_LIMIT as u64));
    assert!(sender.acknowledge((EVENT_LIMIT - 1) as u64));
    assert!(!sender.acknowledge((EVENT_LIMIT - 1) as u64));
    assert_eq!(sender.pending(), EVENT_LIMIT - 1);
    assert_eq!(sender.queue(&[]), Ok(EVENT_LIMIT as u64));
    assert_eq!(
        sender.send_due(1, 1, 0, |p| p.sequence == EVENT_LIMIT as u64),
        report(1, 0, 0)
    );
    assert_eq!(
        sender.send_due(100, 0, 0, |_| unreachable!()),
        report(0, 0, EVENT_LIMIT - 1)
    );
}

#[test]
fn old_and_borrowed_paths_match_bytes_order_ack_and_retry_cadence() {
    let (mut old, mut borrowed) = (EventSender::default(), EventSender::default());
    for payload in [&[][..], &b"one"[..], &[9; EVENT_BYTES][..], &b"tail"[..]] {
        assert_eq!(old.queue(payload), borrowed.queue(payload));
    }
    for (now, budget) in [
        (0, 0),
        (0, 2),
        (0, 8),
        (99, 8),
        (100, 1),
        (90, 8),
        (100, 8),
        (199, 2),
        (200, 8),
        (u64::MAX, usize::MAX),
    ] {
        let expected = old.due(now, budget);
        let mut actual = Vec::new();
        let result = borrowed.send_due(now, budget, usize::MAX, |p| {
            actual.push(p.clone());
            true
        });
        assert_eq!(actual, expected);
        assert_eq!(result.sent, actual.len());
        assert_eq!(
            result.payload_bytes,
            actual.iter().map(|p| p.payload.len()).sum::<usize>()
        );
        for sequence in [2, 999] {
            assert_eq!(old.acknowledge(sequence), borrowed.acknowledge(sequence));
        }
        assert_eq!(old.pending(), borrowed.pending());
    }
    assert!(old.due(u64::MAX, 8).is_empty());
    assert_eq!(
        borrowed.send_due(u64::MAX, 8, usize::MAX, |_| unreachable!()),
        report(0, 0, 0)
    );
}

#[test]
fn monotonic_boundary_and_backwards_time_match_saturating_legacy_behavior() {
    let mut sender = EventSender::default();
    sender.queue(b"x").unwrap();
    assert_eq!(sender.send_due(500, 1, 1, |_| true), report(1, 1, 0));
    for now in [0, 400, 500, 599] {
        assert_eq!(
            sender.send_due(now, 1, 1, |_| unreachable!()),
            report(0, 0, 0)
        );
    }
    assert_eq!(sender.send_due(600, 1, 1, |_| true), report(1, 1, 0));
    assert_eq!(
        sender.send_due(u64::MAX - 100, 1, 1, |_| true),
        report(1, 1, 0)
    );
    assert_eq!(
        sender.send_due(u64::MAX - 1, 1, 1, |_| unreachable!()),
        report(0, 0, 0)
    );
    assert_eq!(sender.send_due(u64::MAX, 1, 1, |_| true), report(1, 1, 0));
    assert_eq!(
        sender.send_due(u64::MAX, 1, 1, |_| unreachable!()),
        report(0, 0, 0)
    );
}

#[test]
fn actual_receiver_recipe_handles_refusal_loss_retry_and_individual_acks() {
    let (mut sender, mut receiver) = (EventSender::default(), EventReceiver::default());
    for payload in [&b"lost first"[..], &b"refused"[..], &b"last"[..]] {
        sender.queue(payload).unwrap();
    }
    let first = sender.send_due(1_000, 3, 100, |p| p.sequence == 0);
    assert_eq!(first, report(1, 10, 2)); // Zero accepted locally, lost in transit.
    let mut ack = [0; 2];
    let mut ack_len = 0;
    let second = sender.send_due(1_001, 3, 100, |p| {
        assert!(receiver.receive(p).unwrap().is_empty());
        ack[ack_len] = p.sequence;
        ack_len += 1;
        true
    });
    assert_eq!(second, report(2, 11, 0));
    for &sequence in &ack[..ack_len] {
        assert!(sender.acknowledge(sequence));
    }
    assert_eq!(sender.pending(), 1);
    assert_eq!(receiver.next_sequence(), 0);
    assert_eq!(receiver.buffered(), 2);
    assert_eq!(
        sender.send_due(1_099, 3, 100, |_| unreachable!()),
        report(0, 0, 0)
    );
    let mut delivered = Vec::new();
    let mut zero = None;
    assert_eq!(
        sender.send_due(1_100, 3, 100, |p| {
            delivered = receiver.receive(p).unwrap();
            assert!(receiver.receive(p).unwrap().is_empty()); // Lost ACK duplicates are harmless.
            zero = Some(p.sequence);
            true
        }),
        report(1, 10, 0)
    );
    assert_eq!(
        delivered
            .iter()
            .map(|p| p.payload.as_slice())
            .collect::<Vec<_>>(),
        [
            b"lost first".as_slice(),
            b"refused".as_slice(),
            b"last".as_slice()
        ]
    );
    assert_eq!(receiver.next_sequence(), 3);
    assert!(sender.acknowledge(zero.unwrap()));
    assert_eq!(sender.pending(), 0);
}

#[test]
fn default_replacement_clears_pending_timestamps_for_a_new_session() {
    let mut sender = EventSender::default();
    sender.queue(b"old").unwrap();
    sender.send_due(9_000, 1, 3, |_| true);
    sender = EventSender::default();
    assert_eq!(
        sender.send_due(0, 1, 99, |_| unreachable!()),
        report(0, 0, 0)
    );
    assert_eq!(sender.queue(b"new"), Ok(0));
    assert_eq!(sender.send_due(0, 1, 3, |_| true), report(1, 3, 0));
    assert_eq!(
        sender.due(100, 1),
        [EventPacket {
            sequence: 0,
            payload: b"new".to_vec()
        }]
    );
}

struct TrackingGuard;
impl Drop for TrackingGuard {
    fn drop(&mut self) {
        TRACK.with(|enabled| enabled.set(false));
    }
}

#[test]
fn warmed_full_window_borrows_original_storage_without_any_allocations() {
    let mut sender = EventSender::default();
    for i in 0..EVENT_LIMIT {
        sender
            .queue(if i % 2 == 0 { &[8; EVENT_BYTES] } else { &[] })
            .unwrap();
    }
    let mut pointers = [std::ptr::null(); EVENT_LIMIT];
    let mut packets = [std::ptr::null(); EVENT_LIMIT];
    // Warm the path and capture packet/payload addresses for subsequent retries.
    sender.send_due(0, EVENT_LIMIT, usize::MAX, |p| {
        pointers[p.sequence as usize] = p.payload.as_ptr();
        packets[p.sequence as usize] = p as *const EventPacket;
        true
    });
    ALLOCATIONS.with(|n| n.set(0));
    TRACK.with(|enabled| enabled.set(true));
    let guard = TrackingGuard;
    for now in (100..=3_200).step_by(100) {
        assert_eq!(
            sender.send_due(now, 0, 0, |_| unreachable!()).remaining_due,
            EVENT_LIMIT
        );
        assert_eq!(
            sender
                .send_due(now, EVENT_LIMIT, EVENT_BYTES - 1, |_| unreachable!())
                .sent,
            0
        );
        assert_eq!(
            sender
                .send_due(now, EVENT_LIMIT, usize::MAX, |_| false)
                .sent,
            0
        );
        let first = sender.send_due(now, EVENT_LIMIT / 2, usize::MAX, |p| {
            assert_eq!(p.payload.as_ptr(), pointers[p.sequence as usize]);
            assert_eq!(p as *const EventPacket, packets[p.sequence as usize]);
            true
        });
        assert_eq!(
            first,
            report(
                EVENT_LIMIT / 2,
                EVENT_LIMIT / 4 * EVENT_BYTES,
                EVENT_LIMIT / 2
            )
        );
        let second = sender.send_due(now, EVENT_LIMIT, usize::MAX, |p| {
            assert_eq!(p.payload.as_ptr(), pointers[p.sequence as usize]);
            assert_eq!(p as *const EventPacket, packets[p.sequence as usize]);
            true
        });
        assert_eq!(
            second,
            report(EVENT_LIMIT / 2, EVENT_LIMIT / 4 * EVENT_BYTES, 0)
        );
    }
    drop(guard);
    assert_eq!(
        ALLOCATIONS.with(Cell::get),
        0,
        "alloc/realloc/alloc_zeroed calls"
    );
    assert_eq!(sender.pending(), EVENT_LIMIT);
}
