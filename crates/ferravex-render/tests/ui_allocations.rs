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
fn shared_hud_layout_formatting_clipping_and_paint_allocate_nothing_after_warmup() {
    use ferravex_render::{
        input::host_frame,
        ui::{ButtonStyle, TextStyle, Ui, UiRect, Viewport},
    };
    let mut ui = Ui::new();
    let viewport = Viewport::fit([1280., 720.], [960., 540.], [12., 8., 12., 8.]).unwrap();
    let reserved = ui.reserved_bytes();
    let mut draw = |frame: usize| {
        host_frame(1. / 60., 1280, 720, || {
            ui.begin(viewport);
            ui.push_clip(UiRect {
                min: [20.; 2],
                size: [920., 500.],
            })
            .unwrap();
            for row in 0..16 {
                ui.text_fmt(
                    UiRect {
                        min: [24., 24. + row as f32 * 24.],
                        size: [400., 22.],
                    },
                    TextStyle {
                        shadow: true,
                        ..Default::default()
                    },
                    format_args!("PLAYER {row:02} / HP {:03}", frame % 101),
                )
                .unwrap();
            }
            for id in 0..6 {
                ui.button(
                    id,
                    UiRect {
                        min: [500., 24. + id as f32 * 52.],
                        size: [180., 44.],
                    },
                    "RESUME",
                    id != 2,
                    ButtonStyle::default(),
                )
                .unwrap();
            }
            ui.finish(true);
            ui.paint();
        })
    };
    for frame in 0..4 {
        draw(frame);
    }
    CALLS.with(|n| n.set(0));
    RECORD.with(|on| on.set(true));
    for frame in 0..1000 {
        draw(frame);
    }
    RECORD.with(|on| on.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
    assert_eq!(ui.reserved_bytes(), reserved);
}
