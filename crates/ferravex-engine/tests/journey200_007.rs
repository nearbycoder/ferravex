//! Allocation regression for pool inspection, iteration, and warmed reuse.
use ferravex_engine::pool::Pool;
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
        let _ = RECORD.try_with(|record| {
            if record.get() {
                let _ = CALLS.try_with(|calls| calls.set(calls.get() + 1));
            }
        });
    }
}

// SAFETY: Each method forwards the original allocator contract to System.
// The thread-local counters do not allocate and tolerate TLS teardown.
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

fn allocation_count(action: impl FnOnce()) -> usize {
    struct StopRecording;
    impl Drop for StopRecording {
        fn drop(&mut self) {
            RECORD.set(false);
        }
    }

    CALLS.set(0);
    RECORD.set(true);
    let recording = StopRecording;
    action();
    drop(recording);
    CALLS.get()
}

#[test]
fn inspection_iteration_and_warmed_reuse_do_not_allocate() {
    assert_eq!(
        allocation_count(|| {
            let pool = Pool::<u32>::new(usize::MAX);
            assert_eq!(pool.capacity(), usize::MAX);
            assert_eq!(pool.available(), usize::MAX);
            assert!(!pool.is_full());
        }),
        0
    );

    let mut pool = Pool::new(64);
    for value in 0..64 {
        pool.insert(value).unwrap();
    }
    // Even before warming the free-index storage, iteration and retaining all
    // entries must not allocate a temporary collection.
    assert_eq!(
        allocation_count(|| {
            for (_, value) in pool.iter_mut() {
                *value += 1;
            }
            pool.retain(|_, value| *value > 0);
            assert_eq!(pool.iter().count(), 64);
        }),
        0
    );

    // The first removal pass may grow the free-index vector, as before.
    pool.clear();
    assert_eq!(
        allocation_count(|| {
            for _ in 0..8 {
                for value in 0..64 {
                    pool.insert(value).unwrap();
                }
                assert!(pool.is_full());
                for (_, value) in pool.iter_mut() {
                    *value += 1;
                }
                pool.retain(|_, value| *value % 2 == 0);
                assert_eq!(pool.len(), 32);
                assert_eq!(pool.available(), 32);
                for value in 0..32 {
                    let handle = pool.insert(value).unwrap();
                    assert_eq!(pool.remove(handle), Some(value));
                }
                pool.clear();
                assert_eq!(pool.available(), 64);
                assert_eq!(pool.capacity(), 64);
            }
        }),
        0
    );
}
