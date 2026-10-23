//! Allocation regression through the registered production spatial module.
use ferravex_engine::spatial::{SpatialError, SpatialGrid, SpatialMatch, SpatialQueryReport};
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

fn record_allocation() {
    let _ = RECORD.try_with(|record| {
        if record.get() {
            let _ = CALLS.try_with(|calls| calls.set(calls.get() + 1));
        }
    });
}

// SAFETY: Forward the original allocation contract unchanged to System.
// Thread-local counters do not allocate and tolerate TLS teardown.
unsafe impl GlobalAlloc for Allocator {
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
fn full_capacity_queries_errors_and_output_reuse_allocate_nothing() {
    let mut grid = SpatialGrid::new(2., 128).unwrap();
    for i in 0..128 {
        grid.insert(i, [(i % 64) as f64 * 0.5 - 16., (i / 64) as f64])
            .unwrap();
    }
    assert_eq!(grid.insert(128, [0.; 2]), Err(SpatialError::Capacity));
    let sentinel = SpatialMatch {
        id: 999,
        point: [99.; 2],
        distance_squared: -1.,
    };
    let mut out = [sentinel; 7];
    let mut ids = [999; 3];

    // Measure first and repeated queries after allocating the grid. No query
    // warm-up, scratch buffer, Vec growth or allocating consumer is hidden here.
    let allocations = allocation_count(|| {
        for _ in 0..32 {
            let report = grid
                .query_radius_into(black_box([0.; 2]), 20., &mut out)
                .unwrap();
            assert_eq!(
                report,
                SpatialQueryReport {
                    written: 7,
                    total: 128
                }
            );
            assert!(report.is_truncated());
            assert_eq!(
                (out[0].id, out[0].point, out[0].distance_squared),
                (32, [0.; 2], 0.)
            );

            out.fill(sentinel);
            let report = grid.query_nearest_into([0.; 2], 20., 3, &mut out).unwrap();
            assert_eq!(
                report,
                SpatialQueryReport {
                    written: 3,
                    total: 128
                }
            );
            assert_eq!((out[0].id, out[1].id, out[2].id), (32, 31, 33));
            assert_eq!(out[3..], [sentinel; 4]);
            let before = out;

            for k in [0, 1, usize::MAX] {
                let report = grid.query_nearest_into([0.; 2], 20., k, &mut []).unwrap();
                assert_eq!(
                    report,
                    SpatialQueryReport {
                        written: 0,
                        total: 128
                    }
                );
                assert!(report.is_truncated());
                assert_eq!(
                    grid.query_nearest_into([0.; 2], 100., k, &mut out),
                    Err(SpatialError::QueryTooLarge)
                );
                assert_eq!(
                    grid.query_nearest_into([0.; 2], 1e155, k, &mut out),
                    Err(SpatialError::Invalid)
                );
                assert_eq!(out, before);
            }
            assert_eq!(
                grid.query_radius_into([f64::NAN, 0.], 1., &mut out),
                Err(SpatialError::Invalid)
            );
            let report = grid.query_nearest_into([0.; 2], 20., 0, &mut out).unwrap();
            assert_eq!(
                report,
                SpatialQueryReport {
                    written: 0,
                    total: 128
                }
            );
            assert_eq!(out, before);
            let report = grid.query_into([-16., 0.], [16., 1.], &mut ids).unwrap();
            assert_eq!(
                report,
                SpatialQueryReport {
                    written: 3,
                    total: 128
                }
            );
            assert_eq!(ids, [0, 1, 2]);
        }
    });
    assert_eq!(allocations, 0);

    // Mutations/setup may allocate; exclude them from query measurements.
    for id in 0..128 {
        assert!(grid.remove(id));
    }
    assert_eq!(
        allocation_count(|| {
            let before = out;
            assert_eq!(
                grid.query_radius_into([0.; 2], 20., &mut out).unwrap(),
                SpatialQueryReport::default()
            );
            assert_eq!(out, before);
        }),
        0
    );

    grid.insert(32, [3., 4.]).unwrap();
    assert_eq!(
        allocation_count(|| {
            let report = grid.query_radius_into([0.; 2], 5., &mut out).unwrap();
            assert_eq!(
                report,
                SpatialQueryReport {
                    written: 1,
                    total: 1
                }
            );
            assert!(!report.is_truncated());
            assert_eq!(
                out[0],
                SpatialMatch {
                    id: 32,
                    point: [3., 4.],
                    distance_squared: 25.
                }
            );
        }),
        0
    );
}
