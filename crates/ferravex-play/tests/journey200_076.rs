use ferravex_play::particles::{
    MAX_RIBBON_POINTS, MAX_RIBBON_WIDTH, RibbonError, RibbonFacing, RibbonPath, RibbonPoint,
    RibbonSource, RibbonTriangle, RibbonTriangles,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}
struct CountingAllocator;

fn record_allocation(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = RECORD.try_with(|record| {
            if record.get() {
                let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            }
        });
    }
}

// SAFETY: Allocation/deallocation forward the original arguments to System.
// The thread-local counters do not allocate and tolerate TLS destruction.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        record_allocation(pointer);
        pointer
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        record_allocation(pointer);
        pointer
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, layout, size) };
        record_allocation(pointer);
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

const XY: RibbonFacing = RibbonFacing::Plane {
    normal: [0., 0., 1.],
};

fn point(x: f32, y: f32) -> RibbonPoint {
    RibbonPoint {
        position: [x, y, 0.],
        color: [1.; 4],
        width: 2.,
        break_before: false,
    }
}

fn triangles(points: &[RibbonPoint]) -> Vec<RibbonTriangle> {
    RibbonTriangles::new(points, XY, 2.).unwrap().collect()
}

fn assert_finite(triangles: &[RibbonTriangle]) {
    assert!(triangles.iter().flat_map(|t| t.vertices).all(|v| {
        v.position
            .into_iter()
            .chain(v.color)
            .chain(v.uv)
            .all(f32::is_finite)
    }));
}

fn assert_position(actual: [f32; 3], expected: [f32; 3]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() <= 1e-6, "{actual} != {expected}");
    }
}

#[test]
fn checked_widths_preserve_path_on_error_and_accept_boundaries() {
    let points = [point(0., 0.), point(2., 0.)];
    let mut path = RibbonPath::new(points.len()).unwrap();
    path.set(&points).unwrap();
    for bad in [
        -1.,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        MAX_RIBBON_WIDTH + 1.,
    ] {
        assert_eq!(path.set_width(bad), Err(RibbonError::InvalidPoint));
        assert_eq!(path.set_taper(bad, 1.), Err(RibbonError::InvalidPoint));
        assert_eq!(path.set_taper(1., bad), Err(RibbonError::InvalidPoint));
        assert_eq!(path.points(), points);
    }
    for width in [0., f32::from_bits(1), 0.5, MAX_RIBBON_WIDTH] {
        path.set_width(width).unwrap();
        assert!(path.points().iter().all(|p| p.width == width && p.valid()));
        for (actual, expected) in path.points().iter().zip(points) {
            assert_eq!(actual.position, expected.position);
            assert_eq!(actual.color, expected.color);
            assert_eq!(actual.break_before, expected.break_before);
        }
    }
}

#[test]
fn taper_uses_arc_length_and_restarts_at_breaks() {
    let points = [
        point(0., 0.),
        point(1., 0.),
        point(1., 0.),
        point(1., 3.),
        RibbonPoint {
            break_before: true,
            ..point(10., 0.)
        },
        point(10., 6.),
        RibbonPoint {
            break_before: true,
            ..point(20., 0.)
        },
        RibbonPoint {
            break_before: true,
            ..point(30., 0.)
        },
        point(30., 0.),
    ];
    let mut path = RibbonPath::new(points.len()).unwrap();
    path.set(&points).unwrap();
    path.set_taper(2., 0.).unwrap();
    assert_eq!(
        path.points().iter().map(|p| p.width).collect::<Vec<_>>(),
        [2., 1.5, 1.5, 0., 2., 0., 2., 2., 2.]
    );
    path.set_taper(0., 4.).unwrap();
    assert_eq!(
        path.points().iter().map(|p| p.width).collect::<Vec<_>>(),
        [0., 1., 1., 4., 0., 4., 0., 0., 0.]
    );
    for (actual, expected) in path.points().iter().zip(points) {
        assert_eq!(actual.position, expected.position);
        assert_eq!(actual.color, expected.color);
        assert_eq!(actual.break_before, expected.break_before);
    }
}

#[test]
fn empty_paths_still_validate_widths() {
    let mut path = RibbonPath::new(0).unwrap();
    assert_eq!(path.set_width(f32::NAN), Err(RibbonError::InvalidPoint));
    assert_eq!(path.set_taper(1., -1.), Err(RibbonError::InvalidPoint));
    path.set_width(0.).unwrap();
    path.set_taper(MAX_RIBBON_WIDTH, 0.).unwrap();
    assert!(path.points().is_empty());
    assert_eq!(RibbonTriangles::new(&path, XY, 2.).unwrap().count(), 0);
}

#[test]
fn taper_handles_extreme_finite_distances() {
    let points = [point(-f32::MAX, 0.), point(0., 0.), point(f32::MAX, 0.)];
    let mut path = RibbonPath::new(3).unwrap();
    path.set(&points).unwrap();
    path.set_taper(MAX_RIBBON_WIDTH, 0.).unwrap();
    assert_eq!(
        path.points().iter().map(|p| p.width).collect::<Vec<_>>(),
        [MAX_RIBBON_WIDTH, MAX_RIBBON_WIDTH * 0.5, 0.]
    );
    let output = RibbonTriangles::new(&path, XY, 2.)
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(output.len(), 4);
    assert_finite(&output);
    assert_eq!(output[3].vertices[2].position, points[2].position);
}

#[test]
fn coincident_corner_samples_preserve_join_geometry_and_uvs() {
    let points = [point(0., 0.), point(1., 0.), point(1., 1.)];
    let duplicated = [
        points[0], points[0], points[1], points[1], points[1], points[2], points[2],
    ];
    let expected = triangles(&points);
    assert_eq!(triangles(&duplicated), expected);
    assert_eq!(expected[0].vertices[2], expected[2].vertices[0]);
    assert_eq!(expected[1].vertices[2], expected[2].vertices[1]);
    assert_position(expected[0].vertices[2].position, [2., -1., 0.]);
    assert_position(expected[1].vertices[2].position, [0., 1., 0.]);
    assert_eq!(expected[3].vertices[2].uv, [2., 1.]);
}

#[test]
fn duplicate_samples_keep_authored_widths_and_colors() {
    let mut points = [point(0., 0.), point(1., 0.), point(1., 0.), point(1., 1.)];
    points[2].width = 1.;
    points[2].color = [0.2, 0.4, 0.6, 0.8];
    let output = triangles(&points);
    assert_eq!(output.len(), 4);
    assert_position(output[0].vertices[2].position, [2., -1., 0.]);
    assert_position(output[2].vertices[0].position, [1.5, -0.5, 0.]);
    assert_eq!(output[2].vertices[0].color, points[2].color);
    assert_eq!(output[0].vertices[2].uv, output[2].vertices[0].uv);
}

#[test]
fn reversals_and_near_reversals_use_segment_perpendicular_butt_ends() {
    for y in [0., 0.001, -0.001] {
        let output = triangles(&[point(0., 0.), point(1., 0.), point(0., y)]);
        assert_eq!(output.len(), 4);
        assert_finite(&output);
        assert_eq!(output[0].vertices[2].position, [1., -1., 0.]);
        assert_eq!(output[1].vertices[2].position, [1., 1., 0.]);
        let outgoing = output[2].vertices[0].position;
        let offset = [outgoing[0] - 1., outgoing[1]];
        assert!((-offset[0] + y * offset[1]).abs() < 1e-6);
        assert!((offset[0].hypot(offset[1]) - 1.).abs() < 1e-6);
        assert!(outgoing[1] > 0.999);
    }
}

#[test]
fn corner_miters_are_bounded_and_shared() {
    for facing in [
        XY,
        RibbonFacing::Camera {
            eye: [3., 4., 5.],
            up: [0., 1., 0.],
        },
    ] {
        for limit in [1., 2., 4.] {
            for direction in [
                [0., 1., 0.],
                [-1., 0.02, 0.],
                [-1., -0.02, 0.],
                [0., 0., 1.],
            ] {
                let points = [
                    point(-1., 0.),
                    point(0., 0.),
                    RibbonPoint {
                        position: direction,
                        ..point(0., 0.)
                    },
                ];
                let output = RibbonTriangles::new(&points, facing, limit)
                    .unwrap()
                    .collect::<Vec<_>>();
                assert_eq!(output.len(), 4);
                assert_finite(&output);
                assert_eq!(output[0].vertices[2], output[2].vertices[0]);
                assert_eq!(output[1].vertices[2], output[2].vertices[1]);
                let offset = output[0].vertices[2].position;
                let length = offset.into_iter().map(|x| x * x).sum::<f32>().sqrt();
                assert!(length <= limit + 1e-5);
            }
        }
    }
}

#[test]
fn breaks_through_duplicate_runs_stop_miters_and_reset_uvs() {
    let points = [
        point(0., 0.),
        point(1., 0.),
        point(1., 0.),
        RibbonPoint {
            break_before: true,
            ..point(1., 0.)
        },
        point(1., 0.),
        point(1., 1.),
    ];
    let output = triangles(&points);
    assert_eq!(output.len(), 4);
    assert_eq!(output[0].vertices[2].position, [1., -1., 0.]);
    assert_eq!(output[2].vertices[0].position, [2., 0., 0.]);
    assert_eq!(output[2].vertices[0].uv, [0., 0.]);
    assert_eq!(output[3].vertices[2].uv, [1., 1.]);
}

struct SparseSource {
    points: [Option<RibbonPoint>; 7],
}
impl RibbonSource for SparseSource {
    fn point_count(&self) -> usize {
        self.points.len()
    }
    fn point(&self, index: usize) -> Option<RibbonPoint> {
        // A source need not support out-of-range neighbor probes.
        self.points[index]
    }
}

#[test]
fn missing_or_invalid_points_end_runs_without_bridges() {
    for gap in [
        None,
        Some(RibbonPoint {
            width: f32::NAN,
            ..point(1., 0.)
        }),
        Some(RibbonPoint {
            position: [f32::INFINITY; 3],
            ..point(1., 0.)
        }),
    ] {
        let source = SparseSource {
            points: [
                Some(point(0., 0.)),
                Some(point(1., 0.)),
                Some(point(1., 0.)),
                gap,
                Some(point(1., 0.)),
                Some(point(1., 0.)),
                Some(point(1., 1.)),
            ],
        };
        let output = RibbonTriangles::new(&source, XY, 2.)
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(output.len(), 4);
        assert_finite(&output);
        assert_eq!(output[0].vertices[2].position, [1., -1., 0.]);
        assert_eq!(output[2].vertices[0].position, [2., 0., 0.]);
        assert_eq!(output[2].vertices[0].uv, [0., 0.]);
        assert_eq!(output[3].vertices[2].uv, [1., 1.]);
    }
}

#[test]
fn huge_coordinates_and_uv_distances_stay_finite_and_continue() {
    let points = [
        point(-f32::MAX, 0.),
        point(f32::MAX, 0.),
        point(0., 0.),
        point(1., 0.),
        RibbonPoint {
            break_before: true,
            ..point(10., 0.)
        },
        point(11., 0.),
    ];
    for facing in [
        XY,
        RibbonFacing::Camera {
            eye: [f32::MAX; 3],
            up: [0., 1., 0.],
        },
    ] {
        let output = RibbonTriangles::new(&points, facing, 4.)
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(output.len(), 8);
        assert_finite(&output);
        assert_eq!(output[0].vertices[0].uv[0], 0.);
        assert_eq!(output[0].vertices[2].uv[0], f32::MAX);
        assert_eq!(output[5].vertices[2].uv[0], f32::MAX);
        assert_eq!(output[6].vertices[0].uv[0], 0.);
        assert_eq!(output[7].vertices[2].uv[0], 1.);
    }
}

#[test]
fn view_alignment_eye_at_endpoint_tiny_segments_and_zero_widths_are_finite() {
    for facing in [
        RibbonFacing::Plane {
            normal: [1., 0., 0.],
        },
        RibbonFacing::Camera {
            eye: [0.; 3],
            up: [1., 0., 0.],
        },
        RibbonFacing::Camera {
            eye: [2., 0., 0.],
            up: [0., 1., 0.],
        },
    ] {
        let points = [
            point(0., 0.),
            point(f32::from_bits(1), 0.),
            point(1., 0.),
            RibbonPoint {
                width: 0.,
                ..point(2., 0.)
            },
        ];
        let output = RibbonTriangles::new(&points, facing, 2.)
            .unwrap()
            .collect::<Vec<_>>();
        assert_eq!(output.len(), 6);
        assert_finite(&output);
        assert_eq!(output[0].vertices[2].uv[0], f32::from_bits(1));
        assert_eq!(output[5].vertices[0].position, points[3].position);
        assert_eq!(output[5].vertices[2].position, points[3].position);
    }
}

#[test]
fn parallel_diagonal_camera_up_uses_a_perpendicular_fallback() {
    let points = [
        point(0., 0.),
        RibbonPoint {
            position: [1.; 3],
            ..point(0., 0.)
        },
    ];
    let facing = RibbonFacing::Camera {
        eye: [0.; 3],
        up: [1.; 3],
    };
    let output = RibbonTriangles::new(&points, facing, 2.)
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(output.len(), 2);
    assert_finite(&output);
    for (vertex, center) in [
        (output[0].vertices[0], [0.; 3]),
        (output[0].vertices[2], [1.; 3]),
    ] {
        let offset: [f32; 3] = std::array::from_fn(|i| vertex.position[i] - center[i]);
        assert!(offset.into_iter().sum::<f32>().abs() < 1e-6);
        assert!((offset.into_iter().map(|x| x * x).sum::<f32>() - 1.).abs() < 1e-6);
    }
}

#[test]
fn near_view_alignment_does_not_amplify_small_eye_perturbations() {
    let points = [point(0., 0.), point(1., 0.)];
    let make = |y| {
        RibbonTriangles::new(
            &points,
            RibbonFacing::Camera {
                eye: [-1., y, 0.],
                up: [0., 1., 0.],
            },
            2.,
        )
        .unwrap()
        .collect::<Vec<_>>()
    };
    assert_eq!(make(0.), make(1e-6));
    assert_eq!(make(0.), make(-1e-6));
}

#[test]
fn degenerate_and_capacity_limited_sources_terminate() {
    let points = vec![point(0., 0.); MAX_RIBBON_POINTS];
    let mut iterator = RibbonTriangles::new(points.as_slice(), XY, 2.).unwrap();
    assert_eq!(iterator.next(), None);
    assert_eq!(iterator.next(), None);
    let oversized = vec![point(0., 0.); MAX_RIBBON_POINTS + 1];
    assert!(matches!(
        RibbonTriangles::new(oversized.as_slice(), XY, 2.),
        Err(RibbonError::InvalidCapacity)
    ));
    assert!(matches!(
        RibbonPath::new(MAX_RIBBON_POINTS + 1),
        Err(RibbonError::InvalidCapacity)
    ));
}

#[test]
fn width_taper_and_triangle_updates_reuse_storage_without_allocating() {
    let mut points = [point(1., 0.); MAX_RIBBON_POINTS];
    points[0] = point(0., 0.);
    points[MAX_RIBBON_POINTS - 1] = point(1., 1.);
    let mut path = RibbonPath::new(MAX_RIBBON_POINTS).unwrap();
    path.set(&points).unwrap();
    let storage = path.points().as_ptr();
    ALLOCATIONS.with(|count| count.set(0));
    RECORD.with(|record| record.set(true));
    let mut count = 0;
    for _ in 0..32 {
        path.set_width(2.).unwrap();
        path.set_taper(1., 0.).unwrap();
        count += RibbonTriangles::new(&path, XY, 2.).unwrap().count();
        path.clear();
        path.set(&points).unwrap();
    }
    RECORD.with(|record| record.set(false));
    assert_eq!(ALLOCATIONS.with(Cell::get), 0);
    assert_eq!(path.points().as_ptr(), storage);
    assert_eq!(count, 32 * 4);
}
