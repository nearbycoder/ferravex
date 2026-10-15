use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {static RECORD:Cell<bool>=const {Cell::new(false)};static CALLS:Cell<usize>=const {Cell::new(0)};}
struct Allocator;
fn allocated(p: *mut u8) {
    if !p.is_null() {
        let _ = RECORD.try_with(|on| {
            if on.get() {
                let _ = CALLS.try_with(|n| n.set(n.get() + 1));
            }
        });
    }
}
// SAFETY: Every operation forwards the identical allocation contract to System;
// thread-local counters do not allocate and ignore destruction-time TLS failures.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        allocated(p);
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        allocated(p);
        p
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, l, n) };
        allocated(p);
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
#[test]
fn feeds_contexts_motion_and_touch_reuse_bounded_storage() {
    use ferravex_math::{Rect, Vec2};
    use ferravex_play::input::touch::*;
    use ferravex_play::input::*;
    let mut map = ActionMap::default();
    let game = map.create_context("gameplay").unwrap();
    let menu = map.create_context("menu").unwrap();
    let movement = map
        .named_action(
            "move_x",
            ActionKind::Axis,
            game,
            vec![Binding::digital(InputCode::PadAxis(0))],
        )
        .unwrap();
    let fire = map
        .named_action(
            "fire",
            ActionKind::Button,
            game,
            vec![
                Binding::digital(InputCode::Key(32)),
                Binding::digital(InputCode::Touch(0)),
            ],
        )
        .unwrap();
    let look = map
        .named_action(
            "look_x",
            ActionKind::Delta,
            game,
            vec![
                Binding::digital(InputCode::MouseDelta(0)),
                Binding::digital(InputCode::TouchDelta(0)),
            ],
        )
        .unwrap();
    let mut touch = TouchSurface::default();
    let layout = [Control {
        region: Region::Rect(Rect::new(0., 0., 100., 100.)),
        output: Output::Button(0),
    }];
    touch.set_layout(&layout, &mut map).unwrap();
    CALLS.with(|n| n.set(0));
    RECORD.with(|r| r.set(true));
    for i in 0..1000 {
        map.set_context(Some(game)).unwrap();
        map.feed(InputCode::PadAxis(0), (i as f32 * 0.01).sin());
        map.feed(InputCode::Key(32), 1.);
        map.feed(InputCode::Key(32), 0.);
        map.feed_delta(InputCode::MouseDelta(0), 10.);
        touch
            .handle(
                Event {
                    id: 1,
                    position: Vec2::splat(30.),
                    phase: Phase::Start,
                },
                &mut map,
            )
            .unwrap();
        touch
            .handle(
                Event {
                    id: 1,
                    position: Vec2::splat(30.),
                    phase: Phase::End,
                },
                &mut map,
            )
            .unwrap();
        map.consume_tick(movement);
        map.consume_tick(fire);
        map.consume_tick(look);
        map.push_context(menu, ContextMode::Exclusive).unwrap();
        map.pop_context();
        touch.set_layout(&layout, &mut map).unwrap();
    }
    RECORD.with(|r| r.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
}
