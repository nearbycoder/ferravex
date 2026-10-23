use ferravex_math::Vec3;
use ferravex_play::navigation::{MAX_ROUTE_POINTS, NavigationError, PathCursor, Route3};

fn set(route: &mut Route3, points: &[Vec3], closed: bool) -> Result<(), NavigationError> {
    if closed {
        route.set_closed(points)
    } else {
        route.set(points)
    }
}

fn square() -> [Vec3; 4] {
    [
        Vec3::ZERO,
        Vec3::X * 4.,
        Vec3::new(4., 4., 0.),
        Vec3::Y * 4.,
    ]
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() <= 1e-12 * b.abs().max(1.), "{a} != {b}");
}

#[test]
fn projects_in_three_dimensions_and_clamps_to_endpoints() {
    let mut route = Route3::new(2).unwrap();
    route.set(&[Vec3::ZERO, Vec3::new(0., 6., 8.)]).unwrap();
    let hit = route.project(Vec3::new(2., 3., 4.)).unwrap().unwrap();
    assert_eq!(hit.position, Vec3::new(0., 3., 4.));
    assert_eq!(hit.direction, Vec3::new(0., 0.6, 0.8));
    assert_eq!(hit.segment, 0);
    assert_eq!(hit.segment_fraction, 0.5);
    assert_eq!(hit.distance, 5.);
    assert_eq!(hit.progress, 0.5);
    assert_eq!(hit.distance_to_route, 2.);
    let before = route.project(Vec3::new(0., -3., -4.)).unwrap().unwrap();
    assert_eq!(before.position, Vec3::ZERO);
    assert_eq!(before.distance, 0.);
    assert_eq!(before.distance_to_route, 5.);
    let after = route.project(Vec3::new(0., 12., 16.)).unwrap().unwrap();
    assert_eq!(after.position, Vec3::new(0., 6., 8.));
    assert_eq!(after.segment_fraction, 1.);
    assert_eq!(after.distance, 10.);
    assert_eq!(after.progress, 1.);
    assert_eq!(after.distance_to_route, 10.);
}

#[test]
fn progress_is_arc_length_and_duplicate_vertices_are_skipped() {
    let mut route = Route3::new(6).unwrap();
    route
        .set(&[
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::X * 2.,
            Vec3::X * 2.,
            Vec3::new(2., 6., 0.),
            Vec3::new(2., 6., 0.),
        ])
        .unwrap();
    let sample = route.sample_progress(0.5).unwrap().unwrap();
    assert_eq!(sample.position, Vec3::new(2., 2., 0.));
    assert_eq!(sample.segment, 3);
    assert_eq!(sample.distance, 4.);
    let hit = route.project(Vec3::new(3., 2., 0.)).unwrap().unwrap();
    assert_eq!(hit.position, sample.position);
    assert_eq!(hit.segment, 3);
    close(hit.segment_fraction, 1. / 3.);
    assert_eq!(hit.progress, 0.5);
    // The incoming segment wins a geometric tie, while distance sampling
    // retains its existing outgoing-segment convention at a corner.
    let corner = route.project(Vec3::X * 2.).unwrap().unwrap();
    assert_eq!(corner.segment, 1);
    assert_eq!(corner.direction, Vec3::X);
    assert_eq!(route.sample(2.).unwrap().unwrap().segment, 3);
    let end = route.sample_progress(1.).unwrap().unwrap();
    assert_eq!(end.segment, 5);
    assert_eq!(end.direction, Vec3::ZERO);
    assert!(end.complete);
}

#[test]
fn projection_ties_choose_first_segment_at_intersections() {
    let mut route = Route3::new(4).unwrap();
    route
        .set(&[
            Vec3::new(-1., -1., 0.),
            Vec3::new(1., 1., 0.),
            Vec3::new(-1., 1., 0.),
            Vec3::new(1., -1., 0.),
        ])
        .unwrap();
    let hit = route.project(Vec3::Z).unwrap().unwrap();
    assert_eq!(hit.position, Vec3::ZERO);
    assert_eq!(hit.segment, 0);
    assert_eq!(hit.segment_fraction, 0.5);
    close(hit.distance, 2_f64.sqrt());
    assert_eq!(hit.distance_to_route, 1.);
}

#[test]
fn closed_route_includes_closing_segment_and_wraps_at_seam() {
    let mut route = Route3::new(4).unwrap();
    route.set_closed(&square()).unwrap();
    assert_eq!(route.points(), square());
    assert_eq!(route.length(), 16.);
    let sample = route.sample(14.).unwrap().unwrap();
    assert_eq!(sample.position, Vec3::Y * 2.);
    assert_eq!(sample.direction, -Vec3::Y);
    assert_eq!(sample.segment, 3);
    assert!(!sample.complete);
    let hit = route.project(Vec3::new(-3., 2., 0.)).unwrap().unwrap();
    assert_eq!(hit.position, sample.position);
    assert_eq!(hit.distance, 14.);
    assert_eq!(hit.segment, 3);
    assert_eq!(hit.progress, 0.875);
    assert_eq!(hit.distance_to_route, 3.);
    for travel in [0., 16., 48.] {
        let seam = route.sample(travel).unwrap().unwrap();
        assert_eq!(seam.position, Vec3::ZERO);
        assert_eq!(seam.distance, 0.);
        assert_eq!(seam.segment, 0);
        assert!(!seam.complete);
    }
    assert_eq!(route.sample_progress(1.).unwrap().unwrap().distance, 0.);
    let seam = route.project(-Vec3::X - Vec3::Y).unwrap().unwrap();
    assert_eq!(seam.distance, 0.);
    assert_eq!(seam.segment, 0);
    let wrapped = route.sample(50.).unwrap().unwrap();
    assert_eq!(wrapped.distance, 2.);
    assert_eq!(wrapped.position, Vec3::X * 2.);
}

#[test]
fn closed_routes_accept_explicit_duplicate_seams() {
    let mut route = Route3::new(7).unwrap();
    route
        .set_closed(&[
            Vec3::ZERO,
            Vec3::ZERO,
            Vec3::X * 4.,
            Vec3::new(4., 4., 0.),
            Vec3::Y * 4.,
            Vec3::ZERO,
            Vec3::ZERO,
        ])
        .unwrap();
    assert_eq!(route.length(), 16.);
    assert_eq!(route.sample(16.).unwrap().unwrap().segment, 1);
    let hit = route.project(Vec3::Y * 2.).unwrap().unwrap();
    assert_eq!(hit.segment, 4);
    assert_eq!(hit.distance, 14.);
    assert_eq!(route.project(Vec3::ZERO).unwrap().unwrap().distance, 0.);
}

#[test]
fn empty_singleton_and_coincident_routes_have_defined_progress() {
    let mut route = Route3::new(3).unwrap();
    for closed in [false, true] {
        set(&mut route, &[], closed).unwrap();
        assert!(route.sample_progress(0.5).unwrap().is_none());
        assert_eq!(route.project(Vec3::ZERO), Ok(None));
        for count in 1..=3 {
            set(&mut route, &[Vec3::X; 3][..count], closed).unwrap();
            let hit = route.project(Vec3::ZERO).unwrap().unwrap();
            assert_eq!(hit.position, Vec3::X);
            assert_eq!(hit.direction, Vec3::ZERO);
            assert_eq!(hit.distance, 0.);
            assert_eq!(hit.progress, 0.);
            assert_eq!(hit.segment_fraction, 0.);
            assert_eq!(hit.distance_to_route, 1.);
            let sample = route.sample(f64::MAX).unwrap().unwrap();
            assert_eq!(sample.position, Vec3::X);
            assert_eq!(sample.distance, 0.);
            assert_eq!(sample.complete, !closed);
        }
    }
    // A two-point loop traverses the segment in both directions.
    route.set_closed(&[Vec3::ZERO, Vec3::X * 2.]).unwrap();
    assert_eq!(route.length(), 4.);
    assert_eq!(route.sample(3.).unwrap().unwrap().direction, -Vec3::X);
    assert_eq!(route.project(Vec3::X).unwrap().unwrap().segment, 0);
}

#[test]
fn closed_cursor_wraps_and_recovery_can_jump_in_either_direction() {
    let mut route = Route3::new(4).unwrap();
    route.set_closed(&square()).unwrap();
    let mut cursor = PathCursor::default();
    cursor.seek(&route, 31.).unwrap();
    assert_eq!(cursor.distance(), 15.);
    let sample = cursor.advance(&route, 3., 1.).unwrap().unwrap();
    assert_eq!(sample.position, Vec3::X * 2.);
    assert_eq!(cursor.distance(), 2.);
    assert!(!sample.complete);
    cursor.recover(&route, Vec3::Y * 2.).unwrap();
    assert_eq!(cursor.distance(), 14.);
    cursor.recover(&route, Vec3::X).unwrap();
    assert_eq!(cursor.distance(), 1.);
    cursor.recover(&route, Vec3::ZERO).unwrap();
    assert_eq!(cursor.distance(), 0.);
    assert_eq!(
        cursor.advance(&route, 34., 1.).unwrap().unwrap().distance,
        2.
    );
    // Replacement geometry is consulted rather than stale progress history.
    route.set(&[Vec3::X * 10., Vec3::X * 20.]).unwrap();
    let hit = cursor
        .recover(&route, Vec3::new(18., 2., 0.))
        .unwrap()
        .unwrap();
    assert_eq!(hit.distance, 8.);
    cursor.recover(&route, Vec3::X * 12.).unwrap();
    assert_eq!(cursor.distance(), 2.);
    let sample = cursor.advance(&route, 1., 1.).unwrap().unwrap();
    assert_eq!(sample.position, Vec3::X * 13.);
}

#[test]
fn huge_closed_advances_preserve_phase_and_invalid_updates_are_atomic() {
    let mut route = Route3::new(4).unwrap();
    route.set_closed(&square()).unwrap();
    let mut cursor = PathCursor::default();
    cursor.seek(&route, 2.).unwrap();
    // This speed is an exact multiple of the route length; adding it to
    // two directly in f64 would lose the cursor's current offset.
    let sample = cursor.advance(&route, 2_f32.powi(60), 1.).unwrap().unwrap();
    assert_eq!(sample.distance, 2.);
    assert_eq!(
        cursor
            .advance(&route, f32::MAX, 0.)
            .unwrap()
            .unwrap()
            .distance,
        2.
    );
    for (speed, dt) in [
        (f32::NAN, 1.),
        (1., f32::INFINITY),
        (-1., 1.),
        (1., -1.),
        (1., 61.),
    ] {
        assert!(matches!(
            cursor.advance(&route, speed, dt),
            Err(NavigationError::InvalidInput)
        ));
        assert_eq!(cursor.distance(), 2.);
    }
    for invalid in [f64::NAN, f64::INFINITY, -1.] {
        assert_eq!(
            cursor.seek(&route, invalid),
            Err(NavigationError::InvalidInput)
        );
        assert_eq!(cursor.distance(), 2.);
        assert!(matches!(
            route.sample(invalid),
            Err(NavigationError::InvalidInput)
        ));
    }
}

#[test]
fn rejected_or_empty_recovery_preserves_cursor() {
    let mut route = Route3::new(2).unwrap();
    route.set(&[Vec3::ZERO, Vec3::X * 10.]).unwrap();
    let mut cursor = PathCursor::default();
    cursor.seek(&route, 7.).unwrap();
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            cursor.recover(&route, Vec3::new(0., invalid, 0.)),
            Err(NavigationError::InvalidInput)
        );
        assert_eq!(cursor.distance(), 7.);
    }
    route.clear();
    assert_eq!(cursor.recover(&route, Vec3::ZERO), Ok(None));
    assert_eq!(cursor.distance(), 7.);
    cursor.reset();
    assert_eq!(cursor.distance(), 0.);
}

#[test]
fn checked_inputs_do_not_mutate_valid_topology_or_geometry() {
    let mut route = Route3::new(4).unwrap();
    route.set_closed(&square()).unwrap();
    assert_eq!(route.set(&[Vec3::ZERO; 5]), Err(NavigationError::Capacity));
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            route.set(&[Vec3::splat(invalid)]),
            Err(NavigationError::InvalidInput)
        );
        assert_eq!(
            route.set_closed(&[Vec3::splat(invalid)]),
            Err(NavigationError::InvalidInput)
        );
        assert_eq!(
            route.project(Vec3::splat(invalid)),
            Err(NavigationError::InvalidInput)
        );
    }
    assert!(route.is_closed());
    assert_eq!(route.length(), 16.);
    assert_eq!(route.points(), square());
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.1] {
        assert!(matches!(
            route.sample_progress(invalid),
            Err(NavigationError::InvalidInput)
        ));
    }
    route.set(&square()).unwrap();
    assert!(!route.is_closed());
    assert_eq!(route.length(), 12.);
    assert!(route.sample(16.).unwrap().unwrap().complete);
    route.clear();
    assert!(!route.is_closed());
    assert!(matches!(
        route.sample_progress(f64::NAN),
        Err(NavigationError::InvalidInput)
    ));
    assert_eq!(
        route.project(Vec3::splat(f32::NAN)),
        Err(NavigationError::InvalidInput)
    );
}

#[test]
fn projection_handles_extreme_and_tiny_finite_coordinates() {
    let mut route = Route3::new(2).unwrap();
    route
        .set_closed(&[Vec3::splat(-f32::MAX), Vec3::splat(f32::MAX)])
        .unwrap();
    let hit = route.project(Vec3::ZERO).unwrap().unwrap();
    assert_eq!(hit.position, Vec3::ZERO);
    assert!(hit.direction.is_finite());
    assert!(hit.distance.is_finite());
    assert_eq!(hit.distance_to_route, 0.);
    close(hit.progress, 0.25);
    assert!(
        route
            .sample(f64::MAX)
            .unwrap()
            .unwrap()
            .position
            .is_finite()
    );
    route
        .set(&[Vec3::ZERO, Vec3::X * f32::MIN_POSITIVE])
        .unwrap();
    let tiny = Vec3::X * (f32::MIN_POSITIVE * 0.5);
    let hit = route.project(tiny).unwrap().unwrap();
    assert_eq!(hit.position, tiny);
    assert_eq!(hit.progress, 0.5);
    assert_eq!(hit.direction, Vec3::X);
    assert_eq!(route.sample_progress(0.5).unwrap().unwrap().position, tiny);
}

#[test]
fn round_trip_samples_and_projections_on_open_and_closed_routes() {
    let mut route = Route3::new(4).unwrap();
    for closed in [false, true] {
        set(&mut route, &square(), closed).unwrap();
        for step in 0..=128 {
            let fraction = f64::from(step) / 128.;
            let sample = route.sample_progress(fraction).unwrap().unwrap();
            let hit = route.project(sample.position).unwrap().unwrap();
            assert_eq!(hit.position, sample.position);
            close(hit.distance, sample.distance);
            close(hit.distance_to_route, 0.);
        }
    }
}

#[test]
fn replacements_reuse_bounded_storage_including_closure() {
    let mut route = Route3::new(MAX_ROUTE_POINTS).unwrap();
    let points: Vec<_> = (0..MAX_ROUTE_POINTS).map(|i| Vec3::X * i as f32).collect();
    let pointer = route.points().as_ptr();
    let mut cursor = PathCursor::default();
    allocation::record(|| {
        for closed in [true, false, true] {
            set(&mut route, &points, closed).unwrap();
            assert_eq!(route.points().len(), MAX_ROUTE_POINTS);
            assert_eq!(route.points().as_ptr(), pointer);
            let hit = route.project(Vec3::new(100.5, 1., 0.)).unwrap().unwrap();
            assert_eq!(hit.segment, 100);
            assert_eq!(hit.distance, 100.5);
            route.sample_progress(0.5).unwrap();
            cursor.recover(&route, Vec3::X * 200.).unwrap();
            cursor.seek(&route, 10.).unwrap();
            cursor.advance(&route, 5., 0.25).unwrap();
            route.clear();
        }
    });
}

mod allocation {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::cell::Cell;

    thread_local! {
        static ACTIVE: Cell<bool> = const { Cell::new(false) };
        static COUNT: Cell<usize> = const { Cell::new(0) };
    }
    struct Counter;
    #[global_allocator]
    static ALLOCATOR: Counter = Counter;

    fn count(pointer: *mut u8) {
        if !pointer.is_null() {
            let _ = ACTIVE.try_with(|active| {
                if active.get() {
                    let _ = COUNT.try_with(|count| count.set(count.get() + 1));
                }
            });
        }
    }
    // SAFETY: All operations forward their unchanged arguments to System and
    // return its allocation. Thread-local counters do not allocate.
    unsafe impl GlobalAlloc for Counter {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let pointer = unsafe { System.alloc(layout) };
            count(pointer);
            pointer
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let pointer = unsafe { System.alloc_zeroed(layout) };
            count(pointer);
            pointer
        }
        unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            let pointer = unsafe { System.realloc(pointer, layout, size) };
            count(pointer);
            pointer
        }
        unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
            unsafe { System.dealloc(pointer, layout) };
        }
    }
    pub fn record(run: impl FnOnce()) {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                ACTIVE.with(|active| active.set(false));
            }
        }
        COUNT.with(|count| count.set(0));
        ACTIVE.with(|active| active.set(true));
        let reset = Reset;
        run();
        drop(reset);
        assert_eq!(COUNT.with(Cell::get), 0, "route operations allocated");
    }
}
