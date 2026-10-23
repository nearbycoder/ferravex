use ferravex_math::{Vec2, vec2};
use ferravex_play::ui::minimap::{
    MAX_MINIMAP_POINTS, MapConfig, Minimap, MinimapError, OffMapPolicy, ProjectionStatus,
    WorldYAxis,
};
use ferravex_play::ui::{Rect, Viewport};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect {
        min: [x, y],
        size: [w, h],
    }
}
fn config() -> MapConfig {
    MapConfig {
        world: rect(-10., -5., 20., 10.),
        map: rect(10.25, 20.75, 200., 200.),
        y_axis: WorldYAxis::Up,
        rotation_radians: 0.,
    }
}
fn near(a: Vec2, b: Vec2, tolerance: f32) {
    assert!(
        (a - b).length() <= tolerance,
        "{a:?} != {b:?}, tolerance {tolerance}"
    );
}
fn project(map: &Minimap, point: Vec2) -> Vec2 {
    map.project(point, OffMapPolicy::ClampToEdge)
        .unwrap()
        .position
}

#[test]
fn asymmetric_fractional_geometry_is_centered_uniformly_fitted_and_not_stretched() {
    let config = MapConfig {
        world: rect(-3.5, 2.25, 30., 10.),
        map: rect(7.25, 11.5, 120., 90.),
        ..config()
    };
    let map = Minimap::new(config).unwrap();
    near(project(&map, vec2(11.5, 7.25)), vec2(67.25, 56.5), 0.);
    near(project(&map, vec2(-3.5, 2.25)), vec2(7.25, 76.5), 0.);
    near(project(&map, vec2(26.5, 12.25)), vec2(127.25, 36.5), 0.);
    assert_eq!(
        map.footprint(),
        [
            vec2(7.25, 76.5),
            vec2(127.25, 76.5),
            vec2(127.25, 36.5),
            vec2(7.25, 36.5)
        ]
    );
    assert_eq!(map.pick(vec2(67.25, 15.), config.map), Ok(None));
    near(
        map.pick(vec2(67.25, 56.5), config.map).unwrap().unwrap(),
        vec2(11.5, 7.25),
        0.,
    );
}

#[test]
fn axes_and_clockwise_cardinals_have_explicit_screen_directions() {
    for y_axis in [WorldYAxis::Up, WorldYAxis::Down] {
        for (angle, right, down) in [
            (0., Vec2::X, Vec2::Y),
            (std::f32::consts::FRAC_PI_2, Vec2::Y, -Vec2::X),
            (std::f32::consts::PI, -Vec2::X, -Vec2::Y),
            (-std::f32::consts::FRAC_PI_2, -Vec2::Y, Vec2::X),
        ] {
            let c = MapConfig {
                world: rect(-5., -5., 10., 10.),
                rotation_radians: angle,
                y_axis,
                ..config()
            };
            let map = Minimap::new(c).unwrap();
            let center = project(&map, Vec2::ZERO);
            near(project(&map, Vec2::X) - center, right * 20., 0.00003);
            let sign = if y_axis == WorldYAxis::Up { -1. } else { 1. };
            near(
                project(&map, Vec2::Y) - center,
                down * (sign * 20.),
                0.00003,
            );
        }
    }
}

#[test]
fn rotated_footprint_is_fitted_and_corner_letterbox_is_not_pickable() {
    let c = MapConfig {
        rotation_radians: std::f32::consts::FRAC_PI_4,
        ..config()
    };
    let map = Minimap::new(c).unwrap();
    for p in map.footprint() {
        assert!(p.x >= c.map.min[0] && p.x <= c.map.min[0] + c.map.size[0]);
        assert!(p.y >= c.map.min[1] && p.y <= c.map.min[1] + c.map.size[1]);
    }
    assert_eq!(map.pick(vec2(11., 21.), c.map), Ok(None));
    // A corner which is not on the excluded panel maximum must pick despite
    // the last f32 rounding step of the displayed vertex.
    for (world, shown) in [
        vec2(-10., -5.),
        vec2(10., -5.),
        vec2(10., 5.),
        vec2(-10., 5.),
    ]
    .into_iter()
    .zip(map.footprint())
    {
        if c.map.contains(shown.to_array()) {
            near(map.pick(shown, c.map).unwrap().unwrap(), world, 0.00001);
        }
    }
}

#[test]
fn outside_cardinals_edges_and_corners_have_policy_independent_indicators() {
    let map = Minimap::new(config()).unwrap();
    for (point, edge, direction) in [
        (vec2(20., 0.), vec2(10., 0.), Vec2::X),
        (vec2(-20., 0.), vec2(-10., 0.), -Vec2::X),
        (vec2(0., 20.), vec2(0., 5.), -Vec2::Y),
        (vec2(0., -20.), vec2(0., -5.), Vec2::Y),
        (vec2(40., 20.), vec2(10., 5.), vec2(2., -1.).normalize()),
        (vec2(-40., -20.), vec2(-10., -5.), vec2(-2., 1.).normalize()),
        (vec2(20., 2.), vec2(10., 1.), vec2(10., -1.).normalize()),
    ] {
        let clamped = map.project(point, OffMapPolicy::ClampToEdge).unwrap();
        let outside = map.project(point, OffMapPolicy::Preserve).unwrap();
        assert_eq!(clamped.status, ProjectionStatus::Clamped);
        assert_eq!(outside.status, ProjectionStatus::Outside);
        assert_eq!(clamped.off_map, outside.off_map);
        assert_ne!(clamped.position, outside.position);
        let indicator = clamped.off_map.unwrap();
        assert_eq!(indicator.position, project(&map, edge));
        assert_eq!(indicator.position, clamped.position);
        near(indicator.direction, direction, 0.000001);
    }
}

#[test]
fn rotation_changes_off_map_direction_and_boundary_not_world_membership() {
    let c = MapConfig {
        rotation_radians: std::f32::consts::FRAC_PI_4,
        world: rect(-5., -5., 10., 10.),
        ..config()
    };
    let map = Minimap::new(c).unwrap();
    // Just outside the diamond still lies inside the panel's bounding box.
    let outside = map.project(vec2(6., 0.), OffMapPolicy::Preserve).unwrap();
    assert!(c.map.contains(outside.position.to_array()));
    assert_eq!(outside.status, ProjectionStatus::Outside);
    assert_eq!(map.pick(outside.position, c.map), Ok(None));
    let marker = outside.off_map.unwrap();
    near(marker.position, project(&map, vec2(5., 0.)), 0.00001);
    near(marker.direction, vec2(1., 1.).normalize(), 0.000001);
    assert_eq!(
        map.project(vec2(5., 0.), OffMapPolicy::ClampToEdge)
            .unwrap()
            .status,
        ProjectionStatus::Inside
    );
}

#[test]
fn world_projection_edges_are_closed_but_pointer_map_and_clip_are_half_open() {
    let c = MapConfig {
        world: rect(0., 0., 10., 10.),
        map: rect(0., 0., 100., 100.),
        y_axis: WorldYAxis::Down,
        ..config()
    };
    let map = Minimap::new(c).unwrap();
    for point in [Vec2::ZERO, vec2(10., 0.), vec2(10., 10.), vec2(0., 10.)] {
        let hit = map.project(point, OffMapPolicy::Preserve).unwrap();
        assert_eq!(hit.status, ProjectionStatus::Inside);
        assert_eq!(hit.off_map, None);
    }
    assert_eq!(map.pick(Vec2::ZERO, c.map), Ok(Some(Vec2::ZERO)));
    assert_eq!(map.pick(vec2(100., 50.), c.map), Ok(None));
    assert_eq!(map.pick(vec2(50., 100.), c.map), Ok(None));
    assert_eq!(map.pick(vec2(-f32::from_bits(1), 50.), c.map), Ok(None));
    let clip = rect(20., 30., 40., 50.);
    assert_eq!(map.pick(vec2(20., 30.), clip), Ok(Some(vec2(2., 3.))));
    assert_eq!(map.pick(vec2(60., 30.), clip), Ok(None));
    assert_eq!(map.pick(vec2(20., 80.), clip), Ok(None));
    assert_eq!(map.pick(vec2(50., 50.), rect(0., 0., 0., 100.)), Ok(None));
}

#[test]
fn round_trips_cover_rotation_axes_fractional_origins_and_viewport_safe_area() {
    let viewport = Viewport::fit([480., 360.], [400., 300.], [12., 20., 8., 10.]).unwrap();
    for angle in [0., 0.37, -1.9, std::f32::consts::PI, 5.4] {
        for y_axis in [WorldYAxis::Up, WorldYAxis::Down] {
            let c = MapConfig {
                rotation_radians: angle,
                y_axis,
                world: rect(-10.5, 4.25, 30.75, 10.5),
                ..config()
            };
            let map = Minimap::new(c).unwrap();
            for x in 1..10 {
                for y in 1..10 {
                    let point = vec2(
                        c.world.min[0] + c.world.size[0] * x as f32 / 10.,
                        c.world.min[1] + c.world.size[1] * y as f32 / 10.,
                    );
                    let shown = project(&map, point);
                    let host = std::array::from_fn(|i| {
                        viewport.pixels().min[i] + shown.to_array()[i] * viewport.scale()
                    });
                    let logical = Vec2::from_array(viewport.unproject(host).unwrap());
                    near(
                        map.pick(logical, viewport.bounds()).unwrap().unwrap(),
                        point,
                        0.00002,
                    );
                }
            }
        }
    }
}

#[test]
fn invalid_geometry_is_rejected_without_normalization_or_partial_reconfiguration() {
    let original = Minimap::new(config()).unwrap();
    let mut map = original;
    for world in [true, false] {
        for axis in 0..2 {
            for bad in [0., -1., f32::from_bits(1), f32::NAN, f32::INFINITY] {
                let mut c = config();
                let r = if world { &mut c.world } else { &mut c.map };
                r.size[axis] = bad;
                assert_eq!(map.reconfigure(c), Err(MinimapError::InvalidGeometry));
                assert_eq!(map, original);
            }
            for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                let mut c = config();
                let r = if world { &mut c.world } else { &mut c.map };
                r.min[axis] = bad;
                assert_eq!(map.reconfigure(c), Err(MinimapError::InvalidGeometry));
                assert_eq!(map, original);
            }
            for (origin, size) in [(f32::MAX, f32::MAX), (1e30, 1.)] {
                let mut c = config();
                let r = if world { &mut c.world } else { &mut c.map };
                r.min[axis] = origin;
                r.size[axis] = size;
                assert_eq!(map.reconfigure(c), Err(MinimapError::InvalidGeometry));
                assert_eq!(map, original);
            }
        }
    }
    for angle in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            map.reconfigure(MapConfig {
                rotation_radians: angle,
                ..config()
            }),
            Err(MinimapError::InvalidGeometry)
        );
        assert_eq!(map, original);
    }
    let collapsed = MapConfig {
        world: rect(0., 0., f32::MIN_POSITIVE, f32::MAX),
        ..config()
    };
    assert_eq!(
        map.reconfigure(collapsed),
        Err(MinimapError::Unrepresentable)
    );
    assert_eq!(map, original);
    // Adjacent rounded vertices are distinct, but every Y rounds to the same
    // large panel origin. That is not an invertible displayed footprint.
    let collinear = MapConfig {
        map: rect(0., 1e30, 100., 1e24),
        rotation_radians: std::f32::consts::FRAC_PI_4,
        ..config()
    };
    assert_eq!(
        map.reconfigure(collinear),
        Err(MinimapError::Unrepresentable)
    );
    assert_eq!(map, original);
    let changed = MapConfig {
        rotation_radians: 0.5,
        ..config()
    };
    map.reconfigure(changed).unwrap();
    assert_eq!(map.config(), changed);
    assert_ne!(map.footprint(), original.footprint());
    map.reconfigure(config()).unwrap();
    assert_eq!(map, original);
}

#[test]
fn point_pointer_and_clip_errors_are_explicit_and_queries_keep_no_history() {
    let map = Minimap::new(config()).unwrap();
    let valid = map.project(Vec2::ONE, OffMapPolicy::Preserve).unwrap();
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for axis in 0..2 {
            let mut point = Vec2::ZERO;
            point[axis] = bad;
            assert_eq!(
                map.project(point, OffMapPolicy::ClampToEdge),
                Err(MinimapError::InvalidPoint)
            );
            assert_eq!(
                map.pick(point, config().map),
                Err(MinimapError::InvalidGeometry)
            );
        }
    }
    assert_eq!(
        map.pick(Vec2::ZERO, rect(0., 0., -1., 2.)),
        Err(MinimapError::InvalidGeometry)
    );
    assert_eq!(
        map.project(Vec2::ONE, OffMapPolicy::Preserve).unwrap(),
        valid
    );
    assert_eq!(
        MinimapError::Capacity.to_string(),
        "minimap batch capacity exceeded"
    );
    let error: &dyn std::error::Error = &MinimapError::InvalidPoint;
    assert!(error.to_string().contains("finite"));
}

#[test]
fn extreme_world_points_are_bounded_and_preserve_overflow_is_reported() {
    let c = MapConfig {
        world: rect(-f32::MAX, -f32::MAX, f32::MAX, f32::MAX),
        ..config()
    };
    let map = Minimap::new(c).unwrap();
    for point in [Vec2::splat(f32::MAX), Vec2::splat(-f32::MAX), Vec2::ZERO] {
        let marker = map.project(point, OffMapPolicy::ClampToEdge).unwrap();
        assert!(marker.position.is_finite());
        if let Some(edge) = marker.off_map {
            near(edge.direction, vec2(1., -1.).normalize(), 0.000001);
        }
    }
    let tiny = Minimap::new(MapConfig {
        world: rect(0., 0., f32::MIN_POSITIVE, f32::MIN_POSITIVE),
        ..config()
    })
    .unwrap();
    assert_eq!(
        tiny.project(Vec2::splat(f32::MAX), OffMapPolicy::Preserve),
        Err(MinimapError::Unrepresentable)
    );
    let marker = tiny
        .project(Vec2::splat(f32::MAX), OffMapPolicy::ClampToEdge)
        .unwrap();
    assert!(marker.position.is_finite());
    assert_eq!(marker.status, ProjectionStatus::Clamped);
    let huge_angle = Minimap::new(MapConfig {
        rotation_radians: f32::MAX,
        ..config()
    })
    .unwrap();
    assert!(project(&huge_angle, Vec2::ONE).is_finite());
}

#[test]
fn batches_preflight_capacity_invalid_points_and_overflow_before_any_write() {
    let map = Minimap::new(config()).unwrap();
    let seed = map.project(Vec2::ONE, OffMapPolicy::Preserve).unwrap();
    let mut output = [seed; 4];
    assert_eq!(
        map.project_into(&[], OffMapPolicy::Preserve, &mut output),
        Ok(0)
    );
    assert_eq!(output, [seed; 4]);
    let points = [Vec2::ZERO, vec2(2., 3.), Vec2::NAN];
    assert_eq!(
        map.project_into(&points, OffMapPolicy::Preserve, &mut output),
        Err(MinimapError::InvalidPoint)
    );
    assert_eq!(output, [seed; 4]);
    assert_eq!(
        map.project_into(&points, OffMapPolicy::Preserve, &mut output[..2]),
        Err(MinimapError::Capacity)
    );
    assert_eq!(output, [seed; 4]);
    let tiny = Minimap::new(MapConfig {
        world: rect(0., 0., f32::MIN_POSITIVE, f32::MIN_POSITIVE),
        ..config()
    })
    .unwrap();
    assert_eq!(
        tiny.project_into(
            &[Vec2::ZERO, Vec2::splat(f32::MAX)],
            OffMapPolicy::Preserve,
            &mut output
        ),
        Err(MinimapError::Unrepresentable)
    );
    assert_eq!(output, [seed; 4]);
    assert_eq!(
        map.project_into(&points[..2], OffMapPolicy::Preserve, &mut output),
        Ok(2)
    );
    for i in 0..2 {
        assert_eq!(
            output[i],
            map.project(points[i], OffMapPolicy::Preserve).unwrap()
        );
    }
    assert_eq!(&output[2..], &[seed; 2]);
}

#[test]
fn maximum_batches_preserve_order_and_reject_one_extra_without_writes() {
    let map = Minimap::new(config()).unwrap();
    let seed = map.project(Vec2::ZERO, OffMapPolicy::Preserve).unwrap();
    let mut points = vec![Vec2::ZERO; MAX_MINIMAP_POINTS + 1];
    for (i, p) in points.iter_mut().enumerate() {
        *p = vec2(i as f32 / 50., i as f32 / 100.);
    }
    let mut output = vec![seed; points.len()];
    assert_eq!(
        map.project_into(&points, OffMapPolicy::ClampToEdge, &mut output),
        Err(MinimapError::Capacity)
    );
    assert!(output.iter().all(|p| *p == seed));
    assert_eq!(
        map.project_into(
            &points[..MAX_MINIMAP_POINTS],
            OffMapPolicy::ClampToEdge,
            &mut output
        ),
        Ok(MAX_MINIMAP_POINTS)
    );
    for i in 0..MAX_MINIMAP_POINTS {
        assert_eq!(
            output[i],
            map.project(points[i], OffMapPolicy::ClampToEdge).unwrap()
        );
    }
    assert_eq!(output[MAX_MINIMAP_POINTS], seed);
    let calls = allocations(|| {
        black_box(map.project_into(
            black_box(&points[..MAX_MINIMAP_POINTS]),
            OffMapPolicy::ClampToEdge,
            &mut output,
        ))
        .unwrap();
    });
    assert_eq!(calls, 0);
}

#[test]
fn omitted_unknown_rooms_do_not_generate_any_markers_or_change_bounds() {
    let map = Minimap::new(config()).unwrap();
    let supplied = [(7, Some(Vec2::ZERO)), (99, None), (8, Some(Vec2::ONE))];
    let known: Vec<_> = supplied
        .into_iter()
        .filter_map(|(id, p)| p.map(|p| (id, p)))
        .collect();
    let points: Vec<_> = known.iter().map(|(_, p)| *p).collect();
    let seed = map.project(Vec2::ZERO, OffMapPolicy::Preserve).unwrap();
    let mut output = [seed; 3];
    assert_eq!(
        map.project_into(&points, OffMapPolicy::Preserve, &mut output),
        Ok(2)
    );
    assert_eq!(
        known.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        vec![7, 8]
    );
    assert_eq!(map.config(), config());
    assert_eq!(output[2], seed);
}

struct Allocator;
thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
}

fn record_allocation() {
    let _ = RECORD.try_with(|record| {
        if record.get() {
            let _ = CALLS.try_with(|calls| calls.set(calls.get() + 1));
        }
    });
}

// SAFETY: All allocation contracts are forwarded unchanged to System; TLS only
// counts this test thread and tolerates teardown. Production play forbids unsafe.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

fn allocations(action: impl FnOnce()) -> usize {
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
fn allocation_counter_observes_a_real_heap_allocation() {
    assert!(
        allocations(|| {
            black_box(Box::new(283));
        }) > 0
    );
}

#[test]
fn warmed_projection_pick_reconfigure_and_atomic_batches_allocate_nothing() {
    let mut map = Minimap::new(config()).unwrap();
    let seed = map.project(Vec2::ZERO, OffMapPolicy::Preserve).unwrap();
    let points = [Vec2::ZERO, Vec2::ONE, vec2(40., 20.)];
    let mut output = [seed; 4];
    map.project_into(&points, OffMapPolicy::ClampToEdge, &mut output)
        .unwrap();
    map.pick(seed.position, config().map).unwrap();
    let calls = allocations(|| {
        for _ in 0..64 {
            black_box(Minimap::new(black_box(config()))).unwrap();
            map.reconfigure(black_box(config())).unwrap();
            assert!(
                map.reconfigure(MapConfig {
                    rotation_radians: f32::NAN,
                    ..config()
                })
                .is_err()
            );
            black_box(map.project_into(black_box(&points), OffMapPolicy::ClampToEdge, &mut output))
                .unwrap();
            assert!(
                map.project_into(
                    &[Vec2::ZERO, Vec2::NAN],
                    OffMapPolicy::Preserve,
                    &mut output
                )
                .is_err()
            );
            assert!(
                map.project_into(&points, OffMapPolicy::Preserve, &mut output[..1])
                    .is_err()
            );
            black_box(map.project(black_box(vec2(40., 20.)), OffMapPolicy::Preserve)).unwrap();
            assert!(map.project(Vec2::NAN, OffMapPolicy::ClampToEdge).is_err());
            black_box(map.pick(black_box(seed.position), config().map))
                .unwrap()
                .unwrap();
            black_box(map.pick(vec2(-1., -1.), config().map)).unwrap();
            black_box(map.footprint());
        }
    });
    assert_eq!(calls, 0);
}
