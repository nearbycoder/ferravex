use ferravex_render::{
    Vec2,
    input::{PlayerInput, host_frame, touch, touch_event},
    ui::{ButtonStyle, Ui, UiInteraction, UiRect, Viewport},
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
}
struct Allocator;
fn allocated(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = RECORD.try_with(|on| {
            if on.get() {
                let _ = CALLS.try_with(|calls| calls.set(calls.get() + 1));
            }
        });
    }
}
// SAFETY: Forward exactly the caller's allocation contract to System. The
// counters are thread-local and nonallocating, and ignore TLS teardown errors.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        allocated(pointer);
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        allocated(pointer);
        pointer
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        allocated(pointer);
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

#[test]
fn public_result_and_cancelled_tail_allocate_nothing_after_warmup() {
    let mut ui = Ui::new();
    let mut player = PlayerInput::transient("journey400-279-allocation");
    let viewport = Viewport::responsive([320., 240.], [0.; 4]).unwrap();
    let reserved = ui.reserved_bytes();
    let mut run = |cycle: usize| {
        let id = cycle as u64;
        touch_event(touch::Event {
            id,
            phase: touch::Phase::Start,
            position: Vec2::new(40., 40.),
        });
        host_frame(0., 320, 240, || {
            player.sample(true);
            ui.begin(viewport);
            ui.button(
                10,
                UiRect {
                    min: [20.; 2],
                    size: [100., 44.],
                },
                "BUY",
                cycle.is_multiple_of(2),
                ButtonStyle::default(),
            )
            .unwrap();
            ui.finish_player(&mut player, true);
            assert_eq!(
                ui.interaction(),
                UiInteraction {
                    pointer_consumed: true,
                    pointer_capture: Some(10),
                    ..Default::default()
                }
            );
            ui.cancel();
            ui.paint();
        });
        touch_event(touch::Event {
            id,
            phase: if cycle.is_multiple_of(3) {
                touch::Phase::Cancel
            } else {
                touch::Phase::End
            },
            position: Vec2::new(240., 180.),
        });
        host_frame(0., 320, 240, || {
            player.sample(true);
            ui.begin(viewport); // Removed button; cancelled tail still owned.
            ui.finish_player(&mut player, true);
            assert_eq!(
                ui.interaction(),
                UiInteraction {
                    pointer_consumed: true,
                    ..Default::default()
                }
            );
            ui.finish_player(&mut player, true);
            assert!(ui.interaction().pointer_consumed);
            ui.paint();
        });
        host_frame(0., 320, 240, || {
            player.sample(true);
            ui.begin(viewport);
            ui.finish_player(&mut player, true);
            assert_eq!(ui.interaction(), UiInteraction::default());
        });
    };
    for cycle in 0..6 {
        run(cycle);
    }
    CALLS.with(|calls| calls.set(0));
    RECORD.with(|on| on.set(true));
    for cycle in 6..506 {
        run(cycle);
    }
    RECORD.with(|on| on.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
    assert_eq!(ui.reserved_bytes(), reserved);
}
