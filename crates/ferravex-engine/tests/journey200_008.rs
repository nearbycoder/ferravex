use ferravex_engine::bitset::BitSet;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct CountingAllocator;

fn record_allocation() {
    let _ = RECORD.try_with(|enabled| {
        if enabled.get() {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
    });
}

// SAFETY: Every allocation and deallocation forwards the same arguments and
// return value to System. The counters are allocation-free thread locals and
// ignore teardown-time TLS access failures.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(pointer, layout, size) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

#[test]
fn bitset_updates_and_lazy_algebra_allocate_nothing_after_construction() {
    let mut a = BitSet::new(1025);
    let mut b = BitSet::new(1025);
    let mut scratch = BitSet::new(1025);
    let mismatch = BitSet::new(1026);
    let empty = BitSet::new(0);
    for index in [0, 63, 64, 127, 1024] {
        a.set(index, true);
    }
    for index in [1, 63, 128, 1024] {
        b.set(index, true);
    }

    ALLOCATIONS.with(|count| count.set(0));
    RECORD.with(|enabled| enabled.set(true));
    for _ in 0..128 {
        let a = black_box(&a);
        let b = black_box(&b);
        assert_eq!(a.bit_len(), 1025);
        assert!(a.iter().eq([0, 63, 64, 127, 1024]));
        assert!(a.union(b).unwrap().eq([0, 1, 63, 64, 127, 128, 1024]));
        assert!(a.intersection(b).unwrap().eq([63, 1024]));
        assert!(a.difference(b).unwrap().eq([0, 64, 127]));
        assert!(a.union(&mismatch).is_none());
        assert!(a.intersection(&mismatch).is_none());
        assert!(a.difference(&mismatch).is_none());
        assert!(empty.union(&empty).unwrap().next().is_none());
        assert!(empty.intersection(&empty).unwrap().next().is_none());
        assert!(empty.difference(&empty).unwrap().next().is_none());

        scratch.clear();
        assert!(scratch.union_with(a));
        assert!(scratch.union_with(b));
        assert_eq!(scratch.count(), 7);
        assert!(scratch.intersection_with(a));
        assert!(scratch.difference_with(b));
        assert!(scratch.iter().eq([0, 64, 127]));
        assert!(scratch.set(1024, true));
        assert!(scratch.contains(1024));
        assert!(scratch.set(1024, false));
        assert!(!scratch.contains(1024));
        assert!(!scratch.set(1025, true));
        assert!(!scratch.union_with(&mismatch));
        assert!(!scratch.intersection_with(&mismatch));
        assert!(!scratch.difference_with(&mismatch));
        assert!(scratch.iter().eq([0, 64, 127]));
    }
    RECORD.with(|enabled| enabled.set(false));
    assert_eq!(ALLOCATIONS.with(Cell::get), 0);
}
