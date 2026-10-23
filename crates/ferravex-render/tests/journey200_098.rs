//! Allocation regression for the checked bitmap font measurement entry point.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
}

struct Allocator;

fn allocated(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = RECORD.try_with(|record| {
            if record.get() {
                let _ = CALLS.try_with(|calls| calls.set(calls.get() + 1));
            }
        });
    }
}

// SAFETY: Allocation and deallocation forward the identical contract to System.
// The thread-local counters do not allocate and ignore TLS destruction failures.
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
fn checked_font_measurements_allocate_nothing_including_first_use_and_errors() {
    let label = "a☃. 1\r\n\n".repeat(1024);
    CALLS.with(|calls| calls.set(0));
    RECORD.with(|record| record.set(true));
    let metrics = ferravex_render::get_default_font()
        .try_metrics(16, 0.75)
        .unwrap();
    for _ in 0..32 {
        black_box(metrics.measure_line(black_box(&label)).unwrap());
        black_box(metrics.measure_multiline(black_box(&label)).unwrap());
        black_box(metrics.measure_multiline(black_box("")).unwrap());
        black_box(
            ferravex_render::get_default_font()
                .try_metrics(0, 1.0)
                .unwrap_err(),
        );
        black_box(
            ferravex_render::get_default_font()
                .try_metrics(8, f32::NAN)
                .unwrap_err(),
        );
        let huge = ferravex_render::get_default_font()
            .try_metrics(1, f32::MAX)
            .unwrap();
        black_box(huge.measure_line("AA").unwrap_err());
        black_box(huge.measure_multiline("\n").unwrap_err());
    }
    RECORD.with(|record| record.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
}
