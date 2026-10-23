use ferravex_core::FrameStats;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

thread_local! {
    static TRACK_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
    static ALLOCATION_CALLS: Cell<usize> = const { Cell::new(0) };
}

struct CountingAllocator;

fn allocated(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = TRACK_ALLOCATIONS.try_with(|tracking| {
            if tracking.get() {
                let _ = ALLOCATION_CALLS.try_with(|count| count.set(count.get() + 1));
            }
        });
    }
}

// SAFETY: Each operation forwards the unchanged allocation contract to System.
// The counters use allocation-free TLS and tolerate thread teardown.
unsafe impl GlobalAlloc for CountingAllocator {
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
        unsafe { System.dealloc(pointer, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn assert_empty<const N: usize>(stats: &FrameStats<N>) {
    assert_eq!(stats.sample_count(), 0);
    assert_eq!(stats.capacity(), N);
    assert_eq!(stats.mean_ms(), 0.0);
    assert_eq!(stats.max_ms(), 0.0);
    assert_eq!(stats.jitter_ms(), 0.0);
    for percentile in [0.0, 50.0, 95.0, 99.0, 100.0] {
        assert_eq!(stats.percentile_ms(percentile), None);
    }
}

#[test]
fn empty_and_zero_capacity_windows_are_safe() {
    assert_empty(&FrameStats::<4>::default());
    let mut disabled = FrameStats::<0>::default();
    assert_empty(&disabled);
    for seconds in [0.0, 0.016, -1.0, f32::NAN, f32::INFINITY, f32::MAX] {
        assert!(!disabled.try_record(seconds));
        disabled.record(seconds);
        assert_empty(&disabled);
    }
    disabled.clear();
    assert_empty(&disabled);
}

#[test]
fn warmup_uses_only_accepted_samples_and_zero_is_a_sample() {
    let mut stats = FrameStats::<120>::default();
    stats.record(0.010);
    assert_eq!(stats.sample_count(), 1);
    assert_eq!(stats.mean_ms(), 10.0);
    assert_eq!(stats.max_ms(), 10.0);
    assert_eq!(stats.percentile_ms(0.0), Some(10.0));
    assert_eq!(stats.percentile_ms(99.0), Some(10.0));
    assert_eq!(stats.jitter_ms(), 0.0);

    assert!(stats.try_record(0.0));
    assert_eq!(stats.sample_count(), 2);
    assert_eq!(stats.mean_ms(), 5.0);
    assert_eq!(stats.percentile_ms(0.0), Some(0.0));
    assert_eq!(stats.percentile_ms(50.0), Some(0.0));
    assert_eq!(stats.percentile_ms(99.0), Some(10.0));
    assert_eq!(stats.jitter_ms(), 5.0);

    assert!(stats.try_record(-0.0));
    assert_eq!(stats.sample_count(), 3);
}

#[test]
fn singleton_window_replaces_samples_and_has_no_jitter() {
    let mut stats = FrameStats::<1>::default();
    for seconds in [0.008, 0.016, 0.0, 0.032] {
        stats.record(seconds);
        let milliseconds = seconds * 1000.0;
        assert_eq!(stats.sample_count(), 1);
        assert_eq!(stats.mean_ms(), milliseconds);
        assert_eq!(stats.max_ms(), milliseconds);
        assert_eq!(stats.jitter_ms(), 0.0);
        for percentile in [0.0, 0.01, 50.0, 95.0, 99.0, 100.0] {
            assert_eq!(stats.percentile_ms(percentile), Some(milliseconds));
        }
    }
}

#[test]
fn percentiles_use_documented_nearest_rank_and_validate_percentages() {
    let mut stats = FrameStats::<8>::default();
    for seconds in [0.040, 0.010, 0.030, 0.020] {
        stats.record(seconds);
    }
    for (percentile, milliseconds) in [
        (0.0, 10.0),
        (f32::from_bits(1), 10.0),
        (25.0, 10.0),
        (25.01, 20.0),
        (50.0, 20.0),
        (75.0, 30.0),
        (95.0, 40.0),
        (99.0, 40.0),
        (100.0, 40.0),
    ] {
        assert_eq!(stats.percentile_ms(percentile), Some(milliseconds));
    }
    for invalid in [
        -f32::from_bits(1),
        -1.0,
        100.001,
        f32::MAX,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        assert_eq!(stats.percentile_ms(invalid), None);
    }
    assert_eq!(stats.sample_count(), 4);
    assert_eq!(stats.mean_ms(), 25.0);
}

#[test]
fn p95_and_p99_select_distinct_ranks_in_a_full_window() {
    let mut stats = FrameStats::<100>::default();
    for frame in (1..=100).rev() {
        stats.record(frame as f32 / 1000.0);
    }
    assert_eq!(stats.sample_count(), 100);
    assert_eq!(stats.percentile_ms(95.0), Some(95.0));
    assert_eq!(stats.percentile_ms(99.0), Some(99.0));
    assert_eq!(stats.percentile_ms(100.0), Some(100.0));
}

#[test]
fn invalid_samples_do_not_fill_or_evict_the_window() {
    let mut stats = FrameStats::<2>::default();
    for retained in 0..=2 {
        for invalid in [
            -1.0,
            -f32::from_bits(1),
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
        ] {
            assert!(!stats.try_record(invalid));
            stats.record(invalid);
            assert_eq!(stats.sample_count(), retained);
        }
        if retained < 2 {
            stats.record((retained + 1) as f32 / 1000.0);
        }
    }
    assert_eq!(stats.mean_ms(), 1.5);
    assert_eq!(stats.jitter_ms(), 0.5);
    stats.record(0.003);
    assert_eq!(stats.mean_ms(), 2.5);
    assert_eq!(stats.percentile_ms(0.0), Some(2.0));
    assert_eq!(stats.max_ms(), 3.0);
}

#[test]
fn percentile_queries_preserve_oldest_sample_eviction() {
    let mut stats = FrameStats::<3>::default();
    for seconds in [0.003, 0.001, 0.004] {
        stats.record(seconds);
    }
    assert_eq!(stats.percentile_ms(50.0), Some(3.0));
    assert_eq!(stats.percentile_ms(95.0), Some(4.0));
    stats.record(0.002); // Evicts 3, even after querying sorted percentiles.
    assert_eq!(stats.sample_count(), 3);
    assert_eq!(stats.mean_ms(), 7.0 / 3.0);
    assert_eq!(stats.percentile_ms(50.0), Some(2.0));
    assert!((stats.jitter_ms() - (14.0_f32 / 9.0).sqrt()).abs() < 0.000001);
    for seconds in [0.005, 0.006, 0.007, 0.008, 0.009] {
        stats.record(seconds);
        black_box(stats.percentile_ms(50.0));
    }
    assert_eq!(stats.sample_count(), 3);
    assert_eq!(stats.mean_ms(), 8.0);
    assert_eq!(stats.percentile_ms(0.0), Some(7.0));
    assert_eq!(stats.percentile_ms(50.0), Some(8.0));
    assert_eq!(stats.max_ms(), 9.0);
}

#[test]
fn jitter_is_population_spread_in_milliseconds() {
    let mut stats = FrameStats::<8>::default();
    for seconds in [0.010, 0.010, 0.030, 0.030] {
        stats.record(seconds);
    }
    assert_eq!(stats.mean_ms(), 20.0);
    assert_eq!(stats.jitter_ms(), 10.0); // Population divisor is 4, not 3.
    stats.clear();
    for seconds in [0.030, 0.010, 0.030, 0.010] {
        stats.record(seconds);
    }
    assert_eq!(stats.jitter_ms(), 10.0); // Independent of adjacent differences.
}

#[test]
fn clear_discards_stale_slots_and_restarts_warmup_and_eviction() {
    let mut stats = FrameStats::<3>::default();
    for seconds in [0.010, 0.020, 0.030, 0.040] {
        stats.record(seconds);
    }
    stats.clear();
    assert_empty(&stats);
    stats.clear();
    stats.record(0.001);
    assert_eq!(stats.sample_count(), 1);
    assert_eq!(stats.mean_ms(), 1.0);
    assert_eq!(stats.max_ms(), 1.0);
    assert_eq!(stats.percentile_ms(95.0), Some(1.0));
    assert_eq!(stats.jitter_ms(), 0.0);
    for seconds in [0.002, 0.003, 0.004] {
        stats.record(seconds);
    }
    assert_eq!(stats.sample_count(), 3);
    assert_eq!(stats.mean_ms(), 3.0);
    assert_eq!(stats.percentile_ms(0.0), Some(2.0));
    assert_eq!(stats.max_ms(), 4.0);
}

#[test]
fn large_accepted_samples_do_not_overflow_mean_or_jitter() {
    let large_seconds = f32::MAX / 2048.0;
    let large_ms = large_seconds * 1000.0;
    assert!(large_ms.is_finite());
    let mut stats = FrameStats::<4>::default();
    for _ in 0..4 {
        assert!(stats.try_record(large_seconds));
    }
    assert_eq!(stats.mean_ms(), large_ms);
    assert_eq!(stats.max_ms(), large_ms);
    assert_eq!(stats.percentile_ms(99.0), Some(large_ms));
    assert_eq!(stats.jitter_ms(), 0.0);
    stats.clear();
    stats.record(0.0);
    stats.record(large_seconds);
    assert_eq!(stats.mean_ms(), large_ms / 2.0);
    assert_eq!(stats.jitter_ms(), large_ms / 2.0);
}

#[test]
fn jitter_retains_small_spread_and_subnormal_samples() {
    let mut stats = FrameStats::<2>::default();
    let seconds = 1_048_576.0_f32;
    let next_seconds = f32::from_bits(seconds.to_bits() + 1);
    stats.record(seconds);
    stats.record(next_seconds);
    let difference_ms = next_seconds * 1000.0 - seconds * 1000.0;
    assert_eq!(stats.jitter_ms(), difference_ms / 2.0);

    stats.clear();
    let smallest = f32::from_bits(1);
    assert!(stats.try_record(smallest));
    assert!(stats.try_record(0.0));
    assert_eq!(stats.max_ms(), smallest * 1000.0);
    assert_eq!(stats.mean_ms(), smallest * 500.0);
    assert_eq!(stats.jitter_ms(), smallest * 500.0);
}

#[test]
fn construction_recording_queries_and_clear_do_not_allocate() {
    ALLOCATION_CALLS.with(|count| count.set(0));
    TRACK_ALLOCATIONS.with(|tracking| tracking.set(true));
    let mut stats = black_box(FrameStats::<120>::default());
    let mut disabled = black_box(FrameStats::<0>::default());
    for frame in 0..1200 {
        let seconds = black_box((frame % 37) as f32 / 1000.0);
        stats.record(seconds);
        black_box(stats.try_record(seconds));
        stats.record(black_box(f32::NAN));
        stats.record(black_box(f32::MAX));
        disabled.record(seconds);
        black_box(stats.sample_count());
        black_box(stats.capacity());
        black_box(stats.mean_ms());
        black_box(stats.max_ms());
        black_box(stats.percentile_ms(black_box(95.0)));
        black_box(stats.jitter_ms());
        if frame % 400 == 399 {
            stats.clear();
        }
    }
    black_box(stats);
    black_box(disabled);
    TRACK_ALLOCATIONS.with(|tracking| tracking.set(false));
    assert_eq!(ALLOCATION_CALLS.with(Cell::get), 0);
}
