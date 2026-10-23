//! Allocation checks for scoped jobs through the registered production module.
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
fn allocation_counter_observes_allocations() {
    assert!(
        allocations_during(|| {
            std::hint::black_box(Box::new(std::hint::black_box(42_u64)));
        }) > 0
    );
}

#[test]
fn fixed_scope_storage_reuses_without_allocating_and_rejects_foreign_handles() {
    let mut q = TickScheduler::<()>::with_scope_capacity(2, 2);
    let mut other = TickScheduler::<()>::with_scope_capacity(2, 2);
    let foreign = other.create_scope().unwrap();
    let calls = allocations_during(|| {
        for _ in 0..1024 {
            let first = q.create_scope().unwrap();
            let copy = first.clone();
            let second = q.create_scope().unwrap();
            assert!(q.create_scope().is_none());
            assert!(q.is_scope_active(&copy));
            assert!(!q.is_scope_active(&foreign));
            assert_eq!(q.cancel_scope(&foreign), None);
            assert_eq!(q.cancel_scope_with(&first, |_, _| unreachable!()), Some(0));
            assert_eq!(q.cancel_scope(&copy), None);
            assert_eq!(q.cancel_scope(&second), Some(0));
        }
    });
    assert_eq!(calls, 0);
}

#[test]
fn cancellation_callback_and_discard_do_not_allocate_across_tree_rebalancing() {
    let mut q = TickScheduler::with_scope_capacity(1025, 2);
    let a = q.create_scope().unwrap();
    let b = q.create_scope().unwrap();
    for value in 0..1024_u64 {
        let owner = if value % 2 == 0 { &a } else { &b };
        q.schedule_scoped(owner, value / 4, value).unwrap();
    }
    q.schedule(u64::MAX, 1024).unwrap();
    let mut out = Vec::with_capacity(512);
    let pointer = out.as_ptr();
    let calls = allocations_during(|| {
        assert_eq!(
            q.cancel_scope_with(&a, |id, payload| out.push((id, payload))),
            Some(512)
        );
        assert_eq!(q.cancel_scope(&b), Some(512));
        assert_eq!(q.len(), 1);
        assert_eq!(q.next_tick(), Some(u64::MAX));
        assert_eq!(q.drain_ready_iter(u64::MAX, 1).next(), Some(1024));
    });
    assert_eq!(calls, 0);
    assert_eq!(out.as_ptr(), pointer);
    assert!(
        out.iter()
            .map(|(_, payload)| *payload)
            .eq((0..1024).step_by(2))
    );
}

#[test]
fn scoped_ready_iteration_and_preallocated_output_preserve_allocation_contract() {
    let mut q = TickScheduler::with_scope_capacity(1024, 1);
    let scope = q.create_scope().unwrap();
    for value in 0..1024_u64 {
        q.schedule_scoped(&scope, value / 4, value).unwrap();
    }
    let mut out = Vec::with_capacity(17);
    let calls = allocations_during(|| {
        assert_eq!(q.drain_ready_iter(255, 512).sum::<u64>(), (0..512).sum());
        let mut expected = 512;
        while !q.is_empty() {
            out.clear();
            q.drain_ready_into(255, 17, &mut out);
            for value in &out {
                assert_eq!(*value, expected);
                expected += 1;
            }
        }
        assert_eq!(expected, 1024);
        assert!(q.is_scope_active(&scope));
        assert_eq!(q.cancel_scope(&scope), Some(0));
    });
    assert_eq!(calls, 0);
}

#[test]
fn original_constructor_and_zero_scope_capacity_remain_allocation_free() {
    let calls = allocations_during(|| {
        let mut legacy = TickScheduler::<()>::new(8);
        let mut zero = TickScheduler::<()>::with_scope_capacity(8, 0);
        assert!(legacy.create_scope().is_none());
        assert!(zero.create_scope().is_none());
        std::hint::black_box((legacy, zero));
    });
    assert_eq!(calls, 0);
}
