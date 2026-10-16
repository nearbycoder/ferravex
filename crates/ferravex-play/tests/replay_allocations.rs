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
fn record_and_ghost_reuse_reserved_storage() {
    use ferravex_math::{Quat, Vec3};
    use ferravex_play::replay::ghost::*;
    use ferravex_play::replay::*;
    let mut r = Recorder::new(
        Header {
            schema: 1,
            version: 1,
            context: 1,
            tick_hz: 64,
            seed: 7,
        },
        Limits {
            ticks: 1000,
            events: 1000,
            checkpoints: 11,
            bytes: 16384,
            checkpoint_interval: 100,
            ..Limits::default()
        },
        0,
        &[1; 8],
        0,
    )
    .unwrap();
    let mut g = GhostTrack::new(1, 64, 1000).unwrap();
    RECORD.with(|v| v.set(true));
    for tick in 1..=1000 {
        let cp = r.needs_checkpoint(tick).then_some(&[1u8; 8][..]);
        r.push(
            tick,
            &[1; 4],
            &[EventInput {
                kind: 1,
                payload: &[2; 4],
            }],
            cp,
            tick,
        )
        .unwrap();
        g.push(Sample {
            tick,
            position: Vec3::X * tick as f32,
            rotation: Quat::IDENTITY,
            teleport: false,
        })
        .unwrap();
        let _ = g.sample(tick, 0.);
    }
    RECORD.with(|v| v.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
    assert_eq!(r.tick_count(), 1000);
}
