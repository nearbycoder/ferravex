//! Allocation regression for checked selected mutable pool access.
use ferravex_engine::pool::{Pool, PoolAccessError};
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
fn selected_access_and_returned_arrays_allocate_nothing() {
    let mut pool = Pool::new(8);
    let handles = std::array::from_fn::<_, 8, _>(|_| pool.insert(0_u32).unwrap());
    // Setup is outside the measured region. Slot storage is full, but no
    // removal has warmed the free-index vector yet.
    assert_eq!(
        allocation_count(|| {
            for _ in 0..32 {
                let (first, second) = pool.get2_mut(handles[0], handles[1]).unwrap();
                *first += 1;
                *second += 2;
                let mut reversed = handles;
                reversed.reverse();
                for value in pool.get_many_mut(reversed).unwrap() {
                    *value += 1;
                }
                assert_eq!(pool.get_many_mut([]).unwrap().len(), 0);
                assert_eq!(
                    pool.get2_mut(handles[2], handles[2]),
                    Err(PoolAccessError::DuplicateHandle {
                        first: 0,
                        second: 1
                    })
                );
            }
        }),
        0
    );
    assert_eq!(pool.get(handles[0]), Some(&64));
    assert_eq!(pool.get(handles[1]), Some(&96));
    for &handle in &handles[2..] {
        assert_eq!(pool.get(handle), Some(&32));
    }
    assert!(pool.is_full());

    // Warm reusable storage and produce stale handles outside measurement.
    // Each region below measures returned-reference access and errors only,
    // with live integer payloads and no allocation in the consumer.
    for _ in 0..4 {
        pool.clear();
        let current = std::array::from_fn::<_, 8, _>(|_| pool.insert(10).unwrap());
        assert_eq!(
            allocation_count(|| {
                let (last, first) = pool.get2_mut(current[7], current[0]).unwrap();
                *last -= 3;
                *first += 3;
                for value in pool.get_many_mut(current).unwrap() {
                    *value += 1;
                }
                assert_eq!(
                    pool.get_many_mut([current[0], handles[0]]),
                    Err(PoolAccessError::InvalidHandle { position: 1 })
                );
                assert_eq!(
                    pool.get_many_mut([current[0], current[0], handles[1]]),
                    Err(PoolAccessError::InvalidHandle { position: 2 })
                );
                assert_eq!(
                    pool.get_many_mut([current[1], current[0], current[1]]),
                    Err(PoolAccessError::DuplicateHandle {
                        first: 0,
                        second: 2
                    })
                );
            }),
            0
        );
        assert_eq!(pool.get(current[0]), Some(&14));
        assert_eq!(pool.get(current[7]), Some(&8));
        assert_eq!(pool.len(), 8);
        assert_eq!(pool.available(), 0);
    }
}
