//! Public atomic typed snapshot behavior and scoped native allocation evidence.
use ferravex_play::input::{
    Action, ActionHandle, ActionKind, ActionMap, ActionSnapshotError, Binding, ContextId,
    ContextMode, ControlSettings, InputCode, MAX_ACTIONS,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

const SENTINEL: Action = Action {
    value: -0.75,
    pressed: false,
    released: true,
};
const TAP: Action = Action {
    value: 0.,
    pressed: true,
    released: true,
};
fn register(map: &mut ActionMap, name: &str, kind: ActionKind, code: InputCode) -> ActionHandle {
    map.named_action_handle(name, kind, ContextId::GLOBAL, vec![Binding::digital(code)])
        .unwrap()
}
fn tap(map: &mut ActionMap, code: InputCode) {
    map.feed(code, 1.);
    map.feed(code, 0.);
}

#[test]
fn duplicate_snapshots_capture_both_edges_held_levels_and_motion_before_clearing() {
    let mut map = ActionMap::default();
    let jump = register(&mut map, "jump", ActionKind::Button, InputCode::Key(32));
    let axis = register(&mut map, "axis", ActionKind::Axis, InputCode::PadAxis(0));
    let pan = register(&mut map, "pan", ActionKind::Delta, InputCode::MouseDelta(0));
    tap(&mut map, InputCode::Key(32));
    map.feed(InputCode::PadAxis(0), -0.5);
    map.feed_delta(InputCode::MouseDelta(0), 3.);
    map.feed_delta(InputCode::MouseDelta(0), 5.);
    let handles = [pan, jump, axis, jump, pan, axis];
    let before = map.peek_handles(handles).unwrap();
    assert_eq!(before[0].value, 8.);
    assert_eq!(before[1], TAP);
    assert_eq!(before[2].value, -0.5);
    assert!(before[0].pressed && before[2].pressed);
    assert_eq!(before[0], before[4]);
    assert_eq!(before[1], before[3]);
    assert_eq!(before[2], before[5]);
    assert_eq!(map.peek_handles(handles).unwrap(), before);
    assert_eq!(map.consume_tick_handles(handles).unwrap(), before);
    let after = map.consume_tick_handles(handles).unwrap();
    assert_eq!(after[0], Action::default());
    assert_eq!(after[1], Action::default());
    assert_eq!(
        after[2],
        Action {
            value: -0.5,
            ..Action::default()
        }
    );
    assert_eq!(after[0], after[4]);
    assert_eq!(after[1], after[3]);
    assert_eq!(after[2], after[5]);

    map.feed(InputCode::Key(32), 1.);
    let first = map.consume_tick_handles([jump, jump]).unwrap();
    assert_eq!(first[0], first[1]);
    assert_eq!(first[0].value, 1.);
    assert!(first[0].pressed);
    assert_eq!(
        map.consume_tick_handles([jump]).unwrap()[0],
        Action {
            value: 1.,
            ..Action::default()
        }
    );
    // Motion that cancels before a tick still has both transition flags.
    map.feed_delta(InputCode::MouseDelta(0), 5.);
    map.feed_delta(InputCode::MouseDelta(0), -5.);
    assert_eq!(map.peek_handles([pan, pan]).unwrap(), [TAP; 2]);
    assert_eq!(map.consume_tick_handles([pan, pan]).unwrap(), [TAP; 2]);
    assert_eq!(map.peek_handle(pan).unwrap(), Action::default());
}

#[test]
fn consuming_a_group_preserves_unnamed_and_same_source_actions_and_legacy_sharing() {
    let mut map = ActionMap::default();
    let jump = register(&mut map, "jump", ActionKind::Button, InputCode::Key(32));
    let legacy = map.action(vec![Binding::digital(InputCode::Key(32))]);
    let unnamed = map.handle_from_id(legacy).unwrap();
    let pan = register(&mut map, "pan", ActionKind::Delta, InputCode::Wheel(0));
    let look = register(&mut map, "look", ActionKind::Delta, InputCode::Wheel(0));
    tap(&mut map, InputCode::Key(32));
    map.feed_delta(InputCode::Wheel(0), -7.);
    map.consume_tick_handles([jump, pan]).unwrap();
    assert_eq!(map.peek_handle(unnamed).unwrap(), TAP);
    assert_eq!(map.peek_handle(look).unwrap().value, -7.);
    assert_eq!(map.consume_tick(legacy), TAP);
    assert_eq!(
        map.consume_tick_handles([unnamed]).unwrap(),
        [Action::default()]
    );
    assert_eq!(map.consume_tick_handle(look).unwrap().value, -7.);
    assert_eq!(
        map.consume_tick_handles([look]).unwrap(),
        [Action::default()]
    );
    tap(&mut map, InputCode::Key(32));
    assert_eq!(
        map.consume_tick_handles([unnamed, unnamed]).unwrap(),
        [TAP; 2]
    );
    assert_eq!(map.consume_tick(legacy), Action::default());
}

#[test]
fn every_position_rejects_foreign_or_stale_handles_without_output_or_input_mutation() {
    let mut map = ActionMap::default();
    let a = register(&mut map, "a", ActionKind::Button, InputCode::Key(1));
    let b = register(&mut map, "b", ActionKind::Delta, InputCode::MouseDelta(0));
    let mut other = ActionMap::default();
    let foreign = register(&mut other, "a", ActionKind::Button, InputCode::Key(1));
    let stale = {
        let mut old = ActionMap::default();
        register(&mut old, "a", ActionKind::Button, InputCode::Key(1))
    };
    tap(&mut map, InputCode::Key(1));
    map.feed_delta(InputCode::MouseDelta(0), 11.);
    other.feed(InputCode::Key(1), 1.);
    let before = map.peek_handles([a, b]).unwrap();
    let other_before = other.peek_handle(foreign).unwrap();
    for invalid in [foreign, stale] {
        for index in 0..4 {
            let mut handles = [a, b, a, b];
            handles[index] = invalid;
            let err = ActionSnapshotError::InvalidHandle { index };
            let mut output = [SENTINEL; 6];
            assert_eq!(map.peek_handles_into(&handles, &mut output), Err(err));
            assert_eq!(output, [SENTINEL; 6]);
            assert_eq!(
                map.consume_tick_handles_into(&handles, &mut output),
                Err(err)
            );
            assert_eq!(output, [SENTINEL; 6]);
            assert_eq!(map.peek_handles(handles), Err(err));
            assert_eq!(map.consume_tick_handles(handles), Err(err));
            assert_eq!(map.peek_handles([a, b]).unwrap(), before);
            assert_eq!(other.peek_handle(foreign).unwrap(), other_before);
        }
    }
    assert_eq!(map.consume_tick_handles([a, b]).unwrap(), before);
}

#[test]
fn output_capacity_rejects_atomically_and_success_preserves_the_entire_tail() {
    let mut map = ActionMap::default();
    let a = register(&mut map, "a", ActionKind::Button, InputCode::Key(1));
    tap(&mut map, InputCode::Key(1));
    for available in 0..3 {
        let mut output = [SENTINEL; 5];
        let error = ActionSnapshotError::OutputTooSmall {
            required: 3,
            available,
        };
        assert_eq!(
            map.peek_handles_into(&[a; 3], &mut output[..available]),
            Err(error)
        );
        assert_eq!(
            map.consume_tick_handles_into(&[a; 3], &mut output[..available]),
            Err(error)
        );
        assert_eq!(output, [SENTINEL; 5]);
        assert_eq!(map.peek_handle(a).unwrap(), TAP);
    }
    let mut output = [SENTINEL; 5];
    map.peek_handles_into(&[a; 3], &mut output).unwrap();
    assert_eq!(output, [TAP, TAP, TAP, SENTINEL, SENTINEL]);
    assert_eq!(map.peek_handle(a).unwrap(), TAP);
    output.fill(SENTINEL);
    map.consume_tick_handles_into(&[a; 3], &mut output).unwrap();
    assert_eq!(output, [TAP, TAP, TAP, SENTINEL, SENTINEL]);
    assert_eq!(map.peek_handle(a).unwrap(), Action::default());
}

#[test]
fn empty_groups_full_capacity_and_one_over_are_bounded_including_duplicates() {
    let mut map = ActionMap::default();
    assert_eq!(map.peek_handles([]).unwrap(), []);
    assert_eq!(map.consume_tick_handles([]).unwrap(), []);
    let handles: [_; MAX_ACTIONS] = std::array::from_fn(|i| {
        let id = map.action(vec![Binding::digital(InputCode::Key((i + 1) as u16))]);
        map.handle_from_id(id).unwrap()
    });
    for i in 1..=MAX_ACTIONS {
        tap(&mut map, InputCode::Key(i as u16));
    }
    let mut output = [SENTINEL; MAX_ACTIONS + 2];
    map.peek_handles_into(&[], &mut output).unwrap();
    map.consume_tick_handles_into(&[], &mut output).unwrap();
    assert_eq!(output, [SENTINEL; MAX_ACTIONS + 2]);
    assert_eq!(map.consume_tick_handles([]).unwrap(), []);
    assert_eq!(map.peek_handles(handles).unwrap(), [TAP; MAX_ACTIONS]);
    let mut late_invalid = handles;
    late_invalid[MAX_ACTIONS - 1] = {
        let mut other = ActionMap::default();
        let id = other.action(vec![]);
        other.handle_from_id(id).unwrap()
    };
    let error = ActionSnapshotError::InvalidHandle {
        index: MAX_ACTIONS - 1,
    };
    assert_eq!(
        map.peek_handles_into(&late_invalid, &mut output),
        Err(error)
    );
    assert_eq!(
        map.consume_tick_handles_into(&late_invalid, &mut output),
        Err(error)
    );
    assert_eq!(output, [SENTINEL; MAX_ACTIONS + 2]);
    assert_eq!(map.peek_handles(handles).unwrap(), [TAP; MAX_ACTIONS]);
    let oversized = [handles[0]; MAX_ACTIONS + 1];
    let err = ActionSnapshotError::TooManyHandles {
        count: MAX_ACTIONS + 1,
    };
    assert_eq!(map.peek_handles_into(&oversized, &mut output), Err(err));
    assert_eq!(
        map.consume_tick_handles_into(&oversized, &mut output),
        Err(err)
    );
    assert_eq!(map.peek_handles(oversized), Err(err));
    assert_eq!(map.consume_tick_handles(oversized), Err(err));
    assert_eq!(output, [SENTINEL; MAX_ACTIONS + 2]);
    assert_eq!(map.peek_handles(handles).unwrap(), [TAP; MAX_ACTIONS]);
    map.consume_tick_handles_into(&handles, &mut output)
        .unwrap();
    assert_eq!(&output[..MAX_ACTIONS], &[TAP; MAX_ACTIONS]);
    assert_eq!(&output[MAX_ACTIONS..], &[SENTINEL; 2]);
    assert_eq!(
        map.consume_tick_handles(handles).unwrap(),
        [Action::default(); MAX_ACTIONS]
    );
    tap(&mut map, InputCode::Key(1));
    assert_eq!(
        map.consume_tick_handles([handles[0]; MAX_ACTIONS]).unwrap(),
        [TAP; MAX_ACTIONS]
    );
}

#[test]
fn error_precedence_is_count_then_output_then_first_invalid_handle() {
    let mut map = ActionMap::default();
    let a = map.action(vec![]);
    let a = map.handle_from_id(a).unwrap();
    let mut other = ActionMap::default();
    let id = other.action(vec![]);
    let foreign = other.handle_from_id(id).unwrap();
    assert_eq!(
        map.consume_tick_handles_into(&[foreign; MAX_ACTIONS + 1], &mut []),
        Err(ActionSnapshotError::TooManyHandles {
            count: MAX_ACTIONS + 1
        })
    );
    assert_eq!(
        map.consume_tick_handles_into(&[a, foreign], &mut []),
        Err(ActionSnapshotError::OutputTooSmall {
            required: 2,
            available: 0
        })
    );
    assert_eq!(
        map.consume_tick_handles([a, foreign, foreign]),
        Err(ActionSnapshotError::InvalidHandle { index: 1 })
    );
}

#[test]
fn zero_tick_frames_retain_edges_and_only_executed_catch_up_updates_consume() {
    let mut map = ActionMap::default();
    let a = register(&mut map, "a", ActionKind::Button, InputCode::Key(1));
    let b = register(&mut map, "b", ActionKind::Delta, InputCode::TouchDelta(0));
    // Two host frames with no update. Previewing is safe on each frame.
    map.feed(InputCode::Key(1), 1.);
    map.feed_delta(InputCode::TouchDelta(0), 4.);
    assert!(map.peek_handles([a, b]).unwrap()[0].pressed);
    map.feed(InputCode::Key(1), 0.);
    map.feed_delta(InputCode::TouchDelta(0), 6.);
    assert_eq!(map.peek_handles([a, b]).unwrap()[0], TAP);
    // Three issued steps, but only two updates execute before gameplay stops.
    let mut executed = 0;
    for _ in 0..3 {
        if executed == 2 {
            break;
        }
        let [button, delta] = map.consume_tick_handles([a, b]).unwrap();
        if executed == 0 {
            assert_eq!(button, TAP);
            assert_eq!(delta.value, 10.);
        } else {
            assert_eq!([button, delta], [Action::default(); 2]);
        }
        executed += 1;
    }
    assert_eq!(executed, 2);
    tap(&mut map, InputCode::Key(1));
    // A scheduled but skipped update must not consume this newly queued tap.
    assert_eq!(map.peek_handles([a]).unwrap(), [TAP]);
    assert_eq!(map.consume_tick_handles([a]).unwrap(), [TAP]);
}

#[test]
fn context_transitions_passthrough_and_held_suppression_match_single_reads() {
    let mut map = ActionMap::default();
    let game = map.create_context("game").unwrap();
    let menu = map.create_context("menu").unwrap();
    let button = map
        .named_action_handle(
            "button",
            ActionKind::Button,
            game,
            vec![Binding::digital(InputCode::Key(1))],
        )
        .unwrap();
    let delta = map
        .named_action_handle(
            "delta",
            ActionKind::Delta,
            game,
            vec![Binding::digital(InputCode::MouseDelta(0))],
        )
        .unwrap();
    let global = register(&mut map, "global", ActionKind::Button, InputCode::Key(2));
    map.set_context(Some(game)).unwrap();
    map.feed(InputCode::Key(1), 1.);
    map.feed_delta(InputCode::MouseDelta(0), 9.);
    map.set_context(Some(game)).unwrap(); // Idempotent activation retains edges.
    map.push_context(menu, ContextMode::Passthrough).unwrap();
    let expected = [
        map.peek_handle(button).unwrap(),
        map.peek_handle(delta).unwrap(),
    ];
    assert!(expected[0].pressed);
    assert_eq!(expected[1].value, 9.);
    assert_eq!(map.consume_tick_handles([button, delta]).unwrap(), expected);
    map.pop_context();
    map.push_context(menu, ContextMode::Exclusive).unwrap();
    tap(&mut map, InputCode::Key(2));
    // Preserve the existing release-on-deactivation policy exactly.
    let expected = [
        map.peek_handle(button).unwrap(),
        map.peek_handle(delta).unwrap(),
    ];
    assert_eq!(map.consume_tick_handles([button, delta]).unwrap(), expected);
    assert_eq!(expected[0].value, 0.);
    assert_eq!(expected[1], Action::default());
    assert_eq!(map.peek_handle(global).unwrap(), TAP);
    map.pop_context();
    assert_eq!(
        map.consume_tick_handles([button, delta, button]).unwrap(),
        [Action::default(); 3]
    );
    map.feed(InputCode::Key(1), 0.);
    map.feed(InputCode::Key(1), 1.);
    assert!(map.consume_tick_handles([button]).unwrap()[0].pressed);
    map.feed_delta(InputCode::MouseDelta(0), 5.);
    map.suppress_held();
    assert_eq!(
        map.consume_tick_handles([button, delta]).unwrap(),
        [Action::default(); 2]
    );
    map.feed(InputCode::Key(1), 0.);
    map.feed(InputCode::Key(1), 1.);
    assert!(map.consume_tick_handles([button]).unwrap()[0].pressed);
}

#[test]
fn focus_rebinding_config_and_map_reset_keep_the_existing_identity_and_reset_policy() {
    let mut map = ActionMap::default();
    let a = register(&mut map, "a", ActionKind::Button, InputCode::Key(1));
    let b = register(&mut map, "b", ActionKind::Delta, InputCode::MouseDelta(0));
    let saved = map.encode_config(ControlSettings::default()).unwrap();
    map.feed(InputCode::Key(1), 1.);
    map.feed_delta(InputCode::MouseDelta(0), 9.);
    map.lose_focus();
    let released = Action {
        released: true,
        ..Action::default()
    };
    assert_eq!(
        map.consume_tick_handles([a, b, a]).unwrap(),
        [released, Action::default(), released]
    );
    assert_eq!(
        map.consume_tick_handles([a, b]).unwrap(),
        [Action::default(); 2]
    );
    map.feed(InputCode::Key(1), 1.);
    map.try_rebind_handle(a, vec![Binding::digital(InputCode::Key(1))])
        .unwrap();
    assert_eq!(map.consume_tick_handles([a]).unwrap(), [Action::default()]);
    map.feed(InputCode::Key(1), 0.);
    tap(&mut map, InputCode::Key(1));
    assert_eq!(map.consume_tick_handles([a]).unwrap(), [TAP]);
    map.feed(InputCode::Key(1), 1.);
    map.apply_config(&saved).unwrap();
    assert_eq!(
        map.encode_config(ControlSettings::default()).unwrap(),
        saved
    );
    assert_eq!(
        map.consume_tick_handles([a, b]).unwrap(),
        [Action::default(); 2]
    );
    map.feed(InputCode::Key(1), 0.);
    tap(&mut map, InputCode::Key(1));
    assert_eq!(map.consume_tick_handles([a]).unwrap(), [TAP]);
    let mut moved = Box::new(map);
    assert_eq!(moved.peek_handles([a, b]).unwrap(), [Action::default(); 2]);
    *moved = ActionMap::default();
    assert_eq!(
        moved.consume_tick_handles([a, b]),
        Err(ActionSnapshotError::InvalidHandle { index: 0 })
    );
}

struct CountingAllocator;
thread_local! { static ALLOCS: Cell<Option<usize>> = const { Cell::new(None) }; }
fn count_alloc() {
    let _ = ALLOCS.try_with(|cell| {
        if let Some(count) = cell.get() {
            cell.set(Some(count + 1));
        }
    });
}
// SAFETY: Allocation/deallocation contracts are forwarded unchanged to System.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count_alloc();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count_alloc();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count_alloc();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn warmed_valid_and_rejected_tick_reads_allocate_nothing_at_maximum_capacity() {
    let mut map = ActionMap::default();
    let handles: [_; MAX_ACTIONS] = std::array::from_fn(|i| {
        register(
            &mut map,
            &format!("action_{i}"),
            if i % 2 == 0 {
                ActionKind::Button
            } else {
                ActionKind::Delta
            },
            if i % 2 == 0 {
                InputCode::Key(1)
            } else {
                InputCode::MouseDelta(0)
            },
        )
    });
    let mut output = [SENTINEL; MAX_ACTIONS + 1];
    map.feed(InputCode::Key(1), 1.);
    map.feed_delta(InputCode::MouseDelta(0), 1.);
    map.consume_tick_handles_into(&handles, &mut output)
        .unwrap();
    ALLOCS.with(|cell| cell.set(Some(0)));
    for _ in 0..1000 {
        tap(&mut map, InputCode::Key(1));
        map.feed_delta(InputCode::MouseDelta(0), black_box(1.));
        black_box(map.peek_handles(black_box(handles)).unwrap());
        map.peek_handles_into(black_box(&handles), black_box(&mut output))
            .unwrap();
        black_box(map.consume_tick_handles(black_box(handles)).unwrap());
        map.consume_tick_handles_into(black_box(&handles), black_box(&mut output))
            .unwrap();
        black_box(map.consume_tick_handles([handles[0]; MAX_ACTIONS]).unwrap());
        assert!(map.consume_tick_handles_into(&handles, &mut []).is_err());
        black_box(map.consume_tick_handles([]).unwrap());
    }
    let allocations = ALLOCS.with(|cell| cell.replace(None).unwrap());
    assert_eq!(allocations, 0);
    assert_eq!(output[MAX_ACTIONS], SENTINEL);
}
