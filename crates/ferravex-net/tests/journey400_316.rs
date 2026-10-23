//! Task 316: observable admission with unchanged authority ordering/coalescing.
use ferravex_net::{
    BUTTON_FIRE, BUTTON_JUMP, BUTTON_RELOAD, ClientExtension, ClientPacket, InputAdmissionError,
    InputFrame, InputQueue, InputQueueMetrics, MAX_PITCH, MAX_QUEUED_INPUTS, TimedInput,
    decode_client, encode_client,
};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

fn input(sequence: u32) -> InputFrame {
    InputFrame {
        sequence,
        ..InputFrame::default()
    }
}
fn timed(sequence: u32, buttons: u8, yaw: f32, pitch: f32, shot_tick: Option<u32>) -> TimedInput {
    TimedInput {
        input: InputFrame {
            buttons,
            yaw,
            pitch,
            ..input(sequence)
        },
        shot_tick,
    }
}

#[test]
fn checked_and_bool_methods_count_once_and_inspection_is_read_only() {
    let mut queue = InputQueue::default();
    assert_eq!(queue.push_checked(input(1)), Ok(()));
    assert!(queue.push(input(2)));
    assert_eq!(queue.push_timed_checked(timed(3, 0, 0., 0., None)), Ok(()));
    assert!(queue.push_timed(timed(4, 0, 0., 0., Some(0))));
    assert!(!queue.push(input(4)));
    assert_eq!(
        queue.push_checked(input(4)),
        Err(InputAdmissionError::DuplicateNewest)
    );
    assert!(!queue.push_timed(timed(3, 0, 0., 0., None)));
    assert_eq!(
        queue.push_timed_checked(timed(3, 0, 0., 0., None)),
        Err(InputAdmissionError::OutOfOrder)
    );
    let expected = InputQueueMetrics {
        accepted_inputs: 4,
        rejected_duplicate_newest: 2,
        rejected_out_of_order: 2,
        ..InputQueueMetrics::default()
    };
    for _ in 0..10 {
        assert_eq!(queue.metrics(), expected);
        assert_eq!(queue.len(), 4);
        assert!(!queue.is_empty());
    }
    for sequence in 1..=4 {
        assert_eq!(queue.pop(), Some(input(sequence)));
    }
    assert_eq!(queue.metrics().returned_inputs, 4);
    let after = queue.metrics();
    assert_eq!(queue.pop(), None);
    assert_eq!(queue.pop_latest(), None);
    assert_eq!(queue.pop_latest_timed(), None);
    assert_eq!(queue.metrics(), after);
}

#[test]
fn invalid_precedes_full_and_full_precedes_duplicate_or_order_without_eviction() {
    let mut queue = InputQueue::default();
    for sequence in 0..MAX_QUEUED_INPUTS as u32 {
        queue.push_checked(input(sequence)).unwrap();
    }
    let newest = MAX_QUEUED_INPUTS as u32 - 1;
    assert_eq!(queue.len(), MAX_QUEUED_INPUTS);
    assert_eq!(
        queue.push_checked(InputFrame {
            move_x: f32::NAN,
            ..input(newest)
        }),
        Err(InputAdmissionError::InvalidInput)
    );
    for sequence in [newest, newest - 1, newest.wrapping_add(1 << 31), 1000] {
        assert_eq!(
            queue.push_checked(input(sequence)),
            Err(InputAdmissionError::Full)
        );
    }
    assert!(!queue.push(input(1000)));
    assert!(!queue.push_timed(timed(1000, 0, 0., 0., None)));
    assert_eq!(
        queue.metrics(),
        InputQueueMetrics {
            accepted_inputs: MAX_QUEUED_INPUTS as u64,
            rejected_invalid: 1,
            rejected_full: 6,
            ..InputQueueMetrics::default()
        }
    );
    // No queued value was evicted, and the rejected future sequence never
    // advanced the watermark: a lower (but still newer) value can now enter.
    assert_eq!(queue.pop(), Some(input(0)));
    queue.push_checked(input(100)).unwrap();
    assert_eq!(queue.len(), MAX_QUEUED_INPUTS);
    for sequence in 1..=newest {
        assert_eq!(queue.pop(), Some(input(sequence)));
    }
    assert_eq!(queue.pop(), Some(input(100)));
    queue.push_checked(input(1000)).unwrap(); // The originally full input is retryable.
    assert_eq!(queue.pop(), Some(input(1000)));
    assert_eq!(queue.metrics().coalesced_inputs, 0);
    assert_eq!(queue.metrics().discarded_inputs, 0);
}

#[test]
fn duplicate_newest_is_distinct_from_older_pending_consumed_and_half_range() {
    let mut queue = InputQueue::default();
    queue.push_checked(input(u32::MAX - 1)).unwrap();
    queue.push_checked(input(u32::MAX)).unwrap();
    queue.push_checked(input(0)).unwrap();
    for sequence in [u32::MAX - 1, u32::MAX, 1 << 31] {
        assert_eq!(
            queue.push_checked(input(sequence)),
            Err(InputAdmissionError::OutOfOrder)
        );
    }
    assert_eq!(
        queue.push_checked(input(0)),
        Err(InputAdmissionError::DuplicateNewest)
    );
    assert_eq!(queue.pop_latest(), Some(input(0)));
    assert_eq!(
        queue.push_checked(input(0)),
        Err(InputAdmissionError::DuplicateNewest)
    );
    assert_eq!(
        queue.push_checked(input(u32::MAX)),
        Err(InputAdmissionError::OutOfOrder)
    );
    queue.clear_pending();
    assert_eq!(
        queue.push_checked(input(1 << 31)),
        Err(InputAdmissionError::OutOfOrder)
    );
    assert_eq!(queue.push_checked(input(1)), Ok(()));
    assert_eq!(queue.pop(), Some(input(1)));
    assert_eq!(
        queue.metrics(),
        InputQueueMetrics {
            accepted_inputs: 4,
            rejected_duplicate_newest: 2,
            rejected_out_of_order: 5,
            returned_inputs: 2,
            coalesced_inputs: 2,
            ..InputQueueMetrics::default()
        }
    );
}

#[test]
fn invalid_controls_never_change_pending_values_or_accepted_watermark() {
    let mut queue = InputQueue::default();
    let fire = timed(10, BUTTON_FIRE, 1., -0.25, Some(u32::MAX));
    queue.push_timed_checked(fire).unwrap();
    let bad = [
        InputFrame {
            move_x: f32::NAN,
            ..input(500)
        },
        InputFrame {
            move_x: 1.01,
            ..input(500)
        },
        InputFrame {
            move_z: f32::NEG_INFINITY,
            ..input(500)
        },
        InputFrame {
            move_z: -1.01,
            ..input(500)
        },
        InputFrame {
            yaw: f32::INFINITY,
            ..input(500)
        },
        InputFrame {
            yaw: 4.,
            ..input(500)
        },
        InputFrame {
            pitch: f32::NAN,
            ..input(500)
        },
        InputFrame {
            pitch: MAX_PITCH + 0.01,
            ..input(500)
        },
        InputFrame {
            buttons: 8,
            ..input(500)
        },
    ];
    for frame in bad {
        assert_eq!(
            queue.push_timed_checked(TimedInput {
                input: frame,
                shot_tick: Some(0)
            }),
            Err(InputAdmissionError::InvalidInput)
        );
        assert_eq!(queue.len(), 1);
    }
    assert_eq!(queue.metrics().rejected_invalid, bad.len() as u64);
    // A rejected sequence did not advance ordering or overwrite the old fire.
    queue
        .push_checked(InputFrame {
            move_x: 1.,
            move_z: -1.,
            ..input(11)
        })
        .unwrap();
    let returned = queue.pop_latest_timed().unwrap();
    assert_eq!(
        returned,
        TimedInput {
            input: InputFrame {
                sequence: 11,
                move_x: 1.,
                move_z: -1.,
                ..fire.input
            },
            shot_tick: fire.shot_tick,
        }
    );
    assert_eq!(queue.metrics().accepted_inputs, 2);
    assert_eq!(queue.metrics().returned_inputs, 1);
    assert_eq!(queue.metrics().coalesced_inputs, 1);
}

#[test]
fn coalescing_preserves_first_recovered_fire_or_newest_fire_aim_and_shot_time() {
    for newest_fires in [false, true] {
        let mut queue = InputQueue::default();
        queue
            .push_timed_checked(timed(20, BUTTON_JUMP, 0., 0., None))
            .unwrap();
        queue
            .push_timed_checked(timed(21, BUTTON_FIRE, 0.5, -0.25, Some(101)))
            .unwrap();
        queue
            .push_timed_checked(timed(22, BUTTON_FIRE | BUTTON_RELOAD, 1., 0.5, Some(102)))
            .unwrap();
        let mut latest = timed(
            23,
            if newest_fires { BUTTON_FIRE } else { 0 },
            2.,
            1.,
            Some(103),
        );
        latest.input.move_x = -1.;
        latest.input.move_z = 1.;
        queue.push_timed_checked(latest).unwrap();
        let returned = queue.pop_latest_timed().unwrap();
        assert_eq!(
            (
                returned.input.sequence,
                returned.input.move_x,
                returned.input.move_z
            ),
            (23, -1., 1.)
        );
        assert_eq!(
            returned.input.buttons,
            BUTTON_FIRE | BUTTON_JUMP | BUTTON_RELOAD
        );
        let expected = if newest_fires {
            (2., 1., Some(103))
        } else {
            (0.5, -0.25, Some(101))
        };
        assert_eq!(
            (returned.input.yaw, returned.input.pitch, returned.shot_tick),
            expected
        );
        assert_eq!(queue.metrics().returned_inputs, 1);
        assert_eq!(queue.metrics().coalesced_inputs, 3);
        assert_eq!(queue.metrics().discarded_inputs, 0);
        assert!(queue.is_empty());
    }
    let mut queue = InputQueue::default();
    queue
        .push_timed_checked(timed(1, BUTTON_JUMP, 1., -1., Some(0)))
        .unwrap();
    let latest = timed(2, BUTTON_RELOAD, 2., 1., Some(u32::MAX));
    queue.push_timed_checked(latest).unwrap();
    assert_eq!(
        queue.pop_latest_timed(),
        Some(TimedInput {
            input: InputFrame {
                buttons: BUTTON_JUMP | BUTTON_RELOAD,
                ..latest.input
            },
            ..latest
        })
    );
}

#[test]
fn metrics_reset_pending_clear_and_new_session_have_separate_scopes() {
    let mut queue = InputQueue::default();
    queue.push_checked(input(40)).unwrap();
    queue.push_checked(input(41)).unwrap();
    let old = queue.reset_metrics();
    assert_eq!(old.accepted_inputs, 2);
    assert_eq!(queue.metrics(), InputQueueMetrics::default());
    assert_eq!(queue.len(), 2);
    assert_eq!(
        queue.push_checked(input(41)),
        Err(InputAdmissionError::DuplicateNewest)
    );
    assert_eq!(queue.pop(), Some(input(40)));
    queue.clear_pending();
    assert!(queue.is_empty());
    assert_eq!(
        queue.metrics(),
        InputQueueMetrics {
            rejected_duplicate_newest: 1,
            returned_inputs: 1,
            discarded_inputs: 1,
            ..InputQueueMetrics::default()
        }
    );
    queue.clear_pending(); // Empty discard is a no-op.
    assert_eq!(queue.metrics().discarded_inputs, 1);
    assert_eq!(
        queue.push_checked(input(41)),
        Err(InputAdmissionError::DuplicateNewest)
    );
    assert_eq!(
        queue.push_checked(input(40)),
        Err(InputAdmissionError::OutOfOrder)
    );
    queue.push_checked(input(42)).unwrap();
    queue.push_checked(input(43)).unwrap();
    queue.reset_session();
    assert!(queue.is_empty());
    assert_eq!(queue.metrics().discarded_inputs, 3);
    assert_eq!(queue.metrics().rejected_duplicate_newest, 2);
    assert_eq!(queue.metrics().rejected_out_of_order, 1);
    let metrics = queue.metrics();
    queue.reset_session();
    assert_eq!(queue.metrics(), metrics);
    queue.push_checked(input(0)).unwrap(); // Watermark reset only on new session.
    assert_eq!(queue.pop_latest(), Some(input(0)));
    assert_eq!(queue.metrics().accepted_inputs, 3);
    let previous = queue.reset_metrics();
    assert_eq!(previous.discarded_inputs, 3);
    assert_eq!(queue.metrics(), InputQueueMetrics::default());
    assert_eq!(
        queue.push_checked(input(0)),
        Err(InputAdmissionError::DuplicateNewest)
    );
}

#[test]
fn timed_admission_matches_protocol_boundary_and_exact_existing_v5_bytes() {
    let packet = ClientPacket::Extension(ClientExtension::Inputs {
        session: 9,
        inputs: vec![timed(7, 0, 0., 0., Some(u32::MAX))],
    });
    let fixture = [
        b'F', b'V', b'R', b'X', 5, 7, 9, 0, 0, 0, 0, 0, 0, 0, 1, // envelope
        7, 0, 0, 0, // sequence
        0, 0, 0, 0, 0, 0, 0, 0, // movement
        0, 0, 0, 0, 0, 0, 0, 0, // aim
        0, 1, 255, 255, 255, 255, // buttons and present shot_tick (without fire)
    ];
    assert_eq!(encode_client(&packet).unwrap(), fixture);
    assert_eq!(decode_client(&fixture).unwrap(), packet);
    let mut queue = InputQueue::default();
    for (index, shot_tick) in [None, Some(0), Some(1 << 31), Some(u32::MAX)]
        .into_iter()
        .enumerate()
    {
        for buttons in [0, BUTTON_FIRE] {
            let sample = timed(
                (index * 2 + usize::from(buttons != 0)) as u32,
                buttons,
                0.,
                0.,
                shot_tick,
            );
            let packet = ClientPacket::Extension(ClientExtension::Inputs {
                session: 9,
                inputs: vec![sample],
            });
            assert_eq!(
                decode_client(&encode_client(&packet).unwrap()).unwrap(),
                packet
            );
            queue.push_timed_checked(sample).unwrap();
            assert_eq!(queue.pop_latest_timed(), Some(sample));
        }
    }
    assert_eq!(queue.metrics().accepted_inputs, 8);
    assert_eq!(queue.metrics().returned_inputs, 8);
    assert_eq!(queue.metrics().coalesced_inputs, 0);
}

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
// SAFETY: The allocator forwards all original pointer/layout arguments to System.
// Counting uses const-initialized thread-local Cells and never allocates.
unsafe impl GlobalAlloc for CountAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: The caller supplies GlobalAlloc's required valid layout.
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_allocation();
        // SAFETY: The caller supplies GlobalAlloc's required valid layout.
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

#[test]
fn warmed_queue_reuses_storage_for_admission_rejection_removal_and_resets() {
    let mut queue = InputQueue::default();
    for sequence in 0..MAX_QUEUED_INPUTS as u32 {
        queue.push_checked(input(sequence)).unwrap();
    }
    queue.reset_session();
    queue.reset_metrics();
    ALLOCATIONS.with(|n| n.set(0));
    TRACK.with(|enabled| enabled.set(true));
    for _ in 0..32 {
        queue.reset_session();
        for sequence in 0..MAX_QUEUED_INPUTS as u32 {
            queue
                .push_timed_checked(timed(sequence, BUTTON_FIRE, 1., 0.5, Some(sequence)))
                .unwrap();
        }
        assert_eq!(
            queue.push_checked(input(100)),
            Err(InputAdmissionError::Full)
        );
        assert_eq!(
            queue.push_checked(InputFrame {
                move_z: f32::NAN,
                ..input(100)
            }),
            Err(InputAdmissionError::InvalidInput)
        );
        assert_eq!(queue.pop().unwrap().sequence, 0);
        queue.push_checked(input(100)).unwrap();
        assert_eq!(queue.pop_latest_timed().unwrap().shot_tick, Some(1));
        assert_eq!(
            queue.push_checked(input(100)),
            Err(InputAdmissionError::DuplicateNewest)
        );
        assert_eq!(
            queue.push_checked(input(1)),
            Err(InputAdmissionError::OutOfOrder)
        );
        for sequence in 101..101 + MAX_QUEUED_INPUTS as u32 {
            assert!(queue.push(input(sequence)));
        }
        queue.clear_pending();
        assert!(queue.pop().is_none());
        assert!(queue.pop_latest().is_none());
        assert!(queue.pop_latest_timed().is_none());
        assert!(queue.metrics().accepted_inputs > 0);
        queue.reset_metrics();
    }
    TRACK.with(|enabled| enabled.set(false));
    assert_eq!(
        ALLOCATIONS.with(Cell::get),
        0,
        "alloc/realloc/alloc_zeroed calls on warmed test thread"
    );
}
