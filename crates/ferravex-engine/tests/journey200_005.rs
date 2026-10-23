//! Allocation checks for the registered scheduler's public draining APIs.
use ferravex_engine::scheduler::TickScheduler;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct CountingAllocator;

fn count_allocation(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = RECORD.try_with(|record| {
            if record.get() {
                let _ = ALLOCATIONS.try_with(|count| count.set(count.get().saturating_add(1)));
            }
        });
    }
}

// SAFETY: Each operation forwards the identical allocation contract to System.
// The counters use allocation-free TLS and tolerate thread-local teardown.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        count_allocation(pointer);
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        count_allocation(pointer);
        pointer
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        count_allocation(pointer);
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocations_during(run: impl FnOnce()) -> usize {
    struct StopRecording;
    impl Drop for StopRecording {
        fn drop(&mut self) {
            RECORD.with(|record| record.set(false));
        }
    }

    ALLOCATIONS.with(|count| count.set(0));
    RECORD.with(|record| record.set(true));
    let stop = StopRecording;
    run();
    drop(stop);
    ALLOCATIONS.with(Cell::get)
}

#[test]
fn allocation_counter_observes_buffer_growth() {
    let mut buffer = Vec::new();
    let calls = allocations_during(|| {
        buffer.push(std::hint::black_box(1_u64));
        buffer.reserve(std::hint::black_box(1024));
        std::hint::black_box(&buffer);
    });
    assert!(calls >= 2);
}

#[test]
fn iterator_draining_allocates_nothing_across_tree_rebalancing() {
    let mut tasks = TickScheduler::new(1025);
    for value in 0..1024 {
        tasks.schedule(value / 4, value).unwrap();
    }
    tasks.schedule(u64::MAX, 1024).unwrap();

    let calls = allocations_during(|| {
        assert_eq!(tasks.drain_ready_iter(0, 0).next(), None);
        drop(tasks.drain_ready_iter(u64::MAX, usize::MAX));
        assert_eq!(tasks.drain_ready_iter(0, 2).sum::<u64>(), 1);
        assert_eq!(tasks.len(), 1023);
        for expected in 2..1024 {
            assert_eq!(tasks.drain_ready_iter(255, 7).next(), Some(expected));
        }
        assert_eq!(tasks.drain_ready_iter(255, usize::MAX).next(), None);
        assert_eq!(tasks.next_tick(), Some(u64::MAX));
        assert_eq!(tasks.drain_ready_iter(u64::MAX, 1).next(), Some(1024));
        assert!(tasks.is_empty());
    });
    assert_eq!(calls, 0);
}

#[test]
fn caller_buffer_is_reused_across_batches_without_allocations() {
    let mut tasks = TickScheduler::new(1024);
    let mut ready = Vec::with_capacity(17);
    let capacity = ready.capacity();
    let pointer = ready.as_ptr();

    // Scheduling allocates tree nodes; only draining is measured here.
    for _ in 0..3 {
        for value in 0..1024 {
            tasks.schedule(value / 4, value).unwrap();
        }
        let calls = allocations_during(|| {
            let mut empty = Vec::new();
            assert_eq!(tasks.drain_ready_into(255, usize::MAX, &mut empty), 0);
            let mut expected = 0;
            while !tasks.is_empty() {
                ready.clear();
                ready.push(u64::MAX);
                assert_eq!(tasks.drain_ready_into(255, 0, &mut ready), 0);
                let appended = tasks.drain_ready_into(255, usize::MAX, &mut ready);
                assert_eq!(ready[0], u64::MAX);
                assert_eq!(
                    appended,
                    (1024 - expected).min((capacity - 1) as u64) as usize
                );
                assert_eq!(tasks.drain_ready_into(255, usize::MAX, &mut ready), 0);
                for &value in &ready[1..] {
                    assert_eq!(value, expected);
                    expected += 1;
                }
                assert_eq!(ready.as_ptr(), pointer);
                assert_eq!(ready.capacity(), capacity);
            }
            assert_eq!(expected, 1024);
        });
        assert_eq!(calls, 0);
    }
}
