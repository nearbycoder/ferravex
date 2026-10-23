use ferravex_play::ui::{HUD_REGION_CAPACITY, HudEdge, HudLayoutError, HudRegions, Rect};
use ferravex_render::{Camera, Touch, TouchPhase, Vec2, canvas::Canvas2D, vec2};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
};

fn rect(min: [f32; 2], size: [f32; 2]) -> Rect {
    Rect { min, size }
}

fn same_fit(a: Canvas2D, b: Canvas2D) {
    assert_eq!(a.size, b.size);
    assert_eq!(a.screen, b.screen);
    assert_eq!(a.scale, b.scale);
    assert_eq!(a.offset, b.offset);
}

fn close(a: Vec2, b: Vec2) {
    assert!(a.distance(b) < 0.002, "{a:?} != {b:?}");
}

#[test]
fn checked_full_screen_preserves_legacy_framing_and_camera() {
    for screen in [vec2(1280., 800.), vec2(390., 844.), vec2(2560., 720.)] {
        let size = vec2(1280., 800.);
        let legacy = Canvas2D::fit(size, screen);
        let checked = Canvas2D::try_fit(size, screen).unwrap().unwrap();
        same_fit(legacy, checked);
        for canvas in [legacy, checked] {
            let camera = canvas.camera();
            assert_eq!(camera.offset, Vec2::ZERO);
            assert_eq!(camera.target, size * 0.5);
            assert_eq!(camera.zoom, Vec2::splat(2. * canvas.scale) / screen);
            assert_eq!(camera.rotation, 0.);
        }
    }
    // Existing struct literals and the positive subpixel legacy fit remain valid.
    let legacy = Canvas2D::fit(Vec2::ONE, Vec2::splat(0.5));
    let literal = Canvas2D {
        size: Vec2::ONE,
        screen: Vec2::splat(0.5),
        scale: 0.5,
        offset: Vec2::ZERO,
    };
    same_fit(legacy, literal);
}

#[test]
fn invalid_world_screen_and_content_return_errors_before_suspension() {
    for bad in [0., -1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for size in [vec2(bad, 100.), vec2(100., bad)] {
            for screen in [vec2(800., 600.), Vec2::ZERO] {
                assert!(matches!(
                    Canvas2D::try_fit(size, screen),
                    Err(HudLayoutError::InvalidLayout)
                ));
            }
        }
    }
    for bad in [-1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        for screen in [vec2(bad, 600.), vec2(800., bad)] {
            assert!(Canvas2D::try_fit(Vec2::ONE, screen).is_err());
        }
    }
    for content in [
        rect([-1., 0.], [100.; 2]),
        rect([0., -1.], [100.; 2]),
        rect([f32::NAN, 0.], [100.; 2]),
        rect([0., f32::INFINITY], [100.; 2]),
        rect([0.; 2], [-1., 0.]),
        rect([0.; 2], [0., f32::NAN]),
        rect([0.; 2], [100., f32::INFINITY]),
        rect([750., 0.], [100.; 2]),
        rect([0., 550.], [100.; 2]),
        rect([801., 0.], [0.; 2]),
        rect([f32::MAX; 2], [f32::MAX; 2]),
    ] {
        assert!(matches!(
            Canvas2D::try_fit_in(Vec2::ONE, vec2(800., 600.), content),
            Err(HudLayoutError::InvalidLayout)
        ));
    }
}

#[test]
fn subpixel_and_zero_content_suspend_then_resize_without_stale_offsets() {
    let world = vec2(400., 200.);
    for screen in [Vec2::ZERO, vec2(0., 600.), vec2(800., 0.5)] {
        assert!(Canvas2D::try_fit(world, screen).unwrap().is_none());
    }
    let screen = vec2(800., 600.);
    for content in [
        rect([800., 600.], [0.; 2]),
        rect([10., 100.], [100., 0.999]),
        rect([10., 100.], [0.5, 100.]),
    ] {
        assert!(
            Canvas2D::try_fit_in(world, screen, content)
                .unwrap()
                .is_none()
        );
    }
    assert!(Canvas2D::try_fit(world, Vec2::ONE).unwrap().is_some());
    let mut hud = HudRegions::new(screen.to_array(), [10., 20., 30., 40.], [0.; 2]).unwrap();
    hud.reserve("header", HudEdge::Top, 100.).unwrap();
    let old = Canvas2D::from_fit(hud.fit_canvas(world.to_array()).unwrap().unwrap());
    hud.clear();
    let reset = Canvas2D::from_fit(hud.fit_canvas(world.to_array()).unwrap().unwrap());
    assert_ne!(old.offset, reset.offset);
    let fresh = HudRegions::new(screen.to_array(), [10., 20., 30., 40.], [0.; 2]).unwrap();
    same_fit(
        reset,
        Canvas2D::from_fit(fresh.fit_canvas(world.to_array()).unwrap().unwrap()),
    );
    hud = HudRegions::new([0.; 2], [0.; 4], [0.; 2]).unwrap();
    assert!(hud.fit_canvas(world.to_array()).unwrap().is_none());
    hud = HudRegions::new([390., 844.], [8., 24., 12., 16.], [0.; 2]).unwrap();
    let resized = Canvas2D::from_fit(hud.fit_canvas(world.to_array()).unwrap().unwrap());
    same_fit(
        resized,
        Canvas2D::try_fit_in(world, vec2(390., 844.), hud.content())
            .unwrap()
            .unwrap(),
    );
    assert_eq!(resized.size, old.size);
}

#[test]
fn unrepresentable_scale_zoom_extent_and_screen_offsets_are_rejected() {
    for (world, screen, content) in [
        (
            Vec2::splat(f32::MIN_POSITIVE),
            vec2(800., 600.),
            rect([0.; 2], [800., 600.]),
        ),
        (
            Vec2::ONE,
            Vec2::splat(f32::MAX),
            rect([0.; 2], [f32::MAX; 2]),
        ),
        (
            vec2(f32::MAX, f32::MIN_POSITIVE),
            Vec2::ONE,
            rect([0.; 2], [1.; 2]),
        ),
        (Vec2::ONE, Vec2::splat(1.0e30), rect([1.0e25; 2], [10.; 2])),
    ] {
        assert!(matches!(
            Canvas2D::try_fit_in(world, screen, content),
            Err(HudLayoutError::InvalidLayout)
        ));
    }
}

#[test]
fn reserved_regions_camera_and_synthetic_dpr_touch_round_trip() {
    let world = vec2(1280., 720.);
    for screen in [vec2(640., 320.), vec2(390., 844.), vec2(2560., 720.)] {
        let mut hud =
            HudRegions::new(screen.to_array(), [8., 24., 12., 16.], [160., 100.]).unwrap();
        hud.reserve("header", HudEdge::Top, 76.).unwrap();
        hud.reserve("actions", HudEdge::Bottom, 44.).unwrap();
        hud.reserve("sidebar", HudEdge::Left, 30.).unwrap();
        let fit = hud.fit_canvas(world.to_array()).unwrap().unwrap();
        let canvas = Canvas2D::from_fit(fit);
        same_fit(
            canvas,
            Canvas2D::try_fit_in(world, screen, hud.content())
                .unwrap()
                .unwrap(),
        );
        close(canvas.camera().offset, Vec2::from(fit.camera_offset()));
        let matrix = canvas.camera().matrix(screen.x, screen.y);
        for point in [Vec2::ZERO, vec2(640., 360.), vec2(1200., 700.), world] {
            let logical = canvas.project(point);
            close(canvas.unproject(logical), point);
            let ndc = matrix.project_point3(point.extend(0.));
            for dpr in [1., 1.25, 2., 3.] {
                let framebuffer =
                    vec2((ndc.x + 1.) * screen.x * 0.5, (1. - ndc.y) * screen.y * 0.5) * dpr;
                close(framebuffer / dpr, logical);
                // Synthetic touch data uses the host's logical pixel convention.
                for phase in [
                    TouchPhase::Started,
                    TouchPhase::Moved,
                    TouchPhase::Ended,
                    TouchPhase::Cancelled,
                ] {
                    let touch = Touch {
                        id: 7,
                        position: logical * dpr / dpr,
                        phase,
                    };
                    close(canvas.unproject(touch.position), point);
                    if point == vec2(640., 360.) || point == vec2(1200., 700.) {
                        close(canvas.pointer_half_open(touch.position).unwrap(), point);
                        close(
                            Vec2::from(
                                fit.viewport().unproject(touch.position.to_array()).unwrap(),
                            ),
                            point,
                        );
                    }
                }
            }
        }
        for region in hud.regions() {
            let center = Vec2::from(region.rect.min) + Vec2::from(region.rect.size) * 0.5;
            assert!(region.rect.contains(center.to_array()));
            assert!(canvas.pointer_half_open(center).is_none());
        }
        assert!(canvas.pointer_half_open(vec2(0., 0.)).is_none());
    }
}

#[test]
fn half_open_world_edges_match_hud_and_preserve_legacy_inclusive_pointer() {
    let mut hud = HudRegions::new([800., 600.], [0.; 4], [0.; 2]).unwrap();
    let footer = hud.reserve("footer", HudEdge::Bottom, 100.).unwrap();
    let right = hud.reserve("right", HudEdge::Right, 100.).unwrap();
    let fit = hud.fit_canvas([700., 500.]).unwrap().unwrap();
    let canvas = Canvas2D::from_fit(fit);
    assert_eq!(canvas.pointer_half_open(Vec2::ZERO), Some(Vec2::ZERO));
    for edge in [vec2(350., 500.), vec2(700., 250.), canvas.size] {
        let logical = canvas.project(edge);
        assert!(footer.contains(logical.to_array()) || right.contains(logical.to_array()));
        assert_eq!(canvas.pointer_half_open(logical), None);
        assert_eq!(fit.viewport().unproject(logical.to_array()), None);
        assert_eq!(canvas.pointer(logical), Some(edge));
    }
    for point in [
        vec2(700_f32.next_down(), 250.),
        vec2(350., 500_f32.next_down()),
    ] {
        assert_eq!(canvas.pointer_half_open(point), Some(point));
    }
    for point in [
        vec2(-1., 0.),
        vec2(0., -1.),
        vec2(f32::NAN, 0.),
        vec2(0., f32::INFINITY),
    ] {
        assert_eq!(canvas.pointer_half_open(point), None);
    }
}

#[test]
fn malformed_public_literals_cannot_produce_half_open_hits() {
    let good = Canvas2D::fit(Vec2::splat(100.), Vec2::splat(800.));
    for canvas in [
        Canvas2D { scale: 0., ..good },
        Canvas2D { scale: -1., ..good },
        Canvas2D {
            scale: f32::NAN,
            ..good
        },
        Canvas2D {
            scale: f32::INFINITY,
            ..good
        },
        Canvas2D {
            size: Vec2::ZERO,
            ..good
        },
        Canvas2D {
            size: Vec2::splat(-1.),
            scale: -1.,
            ..good
        },
        Canvas2D {
            size: Vec2::splat(f32::INFINITY),
            ..good
        },
        Canvas2D {
            screen: Vec2::ZERO,
            ..good
        },
        Canvas2D {
            offset: Vec2::splat(f32::NAN),
            ..good
        },
    ] {
        assert!(canvas.pointer_half_open(Vec2::ZERO).is_none());
    }
}

#[test]
fn fractional_reservations_fit_without_crossing_shared_edges() {
    let edges = [HudEdge::Top, HudEdge::Right, HudEdge::Bottom, HudEdge::Left];
    for screen in [[390.25, 844.125], [640.3, 320.6], [2560.5, 720.25]] {
        for sequence in 0..256 {
            let mut hud = HudRegions::new(screen, [1.1, 2.2, 3.3, 4.4], [0.; 2]).unwrap();
            for (index, name) in ["a", "b", "c", "d"].into_iter().enumerate() {
                hud.reserve(name, edges[(sequence >> (index * 2)) & 3], 11.7)
                    .unwrap();
            }
            for world in [vec2(1280., 720.), vec2(333.3, 777.7)] {
                let fit = hud.fit_canvas(world.to_array()).unwrap().unwrap();
                let canvas = Canvas2D::try_fit_in(world, Vec2::from(screen), hud.content())
                    .unwrap()
                    .unwrap();
                same_fit(canvas, Canvas2D::from_fit(fit));
                for axis in 0..2 {
                    assert!(canvas.offset.to_array()[axis] >= hud.content().min[axis]);
                    assert!(
                        canvas.project(world).to_array()[axis]
                            <= hud.content().min[axis] + hud.content().size[axis]
                    );
                }
                let center = canvas.project(world * 0.5);
                close(canvas.pointer_half_open(center).unwrap(), world * 0.5);
                assert_eq!(canvas.pointer_half_open(canvas.project(world)), None);
            }
        }
    }
}

#[test]
fn full_hud_capacity_and_failed_reservation_leave_fitted_canvas_unchanged() {
    let mut hud = HudRegions::new([800., 600.], [0.; 4], [100.; 2]).unwrap();
    for name in ["a", "b", "c", "d", "e", "f", "g", "h"] {
        hud.reserve(name, HudEdge::Top, 10.).unwrap();
    }
    assert_eq!(hud.regions().len(), HUD_REGION_CAPACITY);
    let before = Canvas2D::from_fit(hud.fit_canvas([400., 200.]).unwrap().unwrap());
    assert_eq!(
        hud.reserve("overflow", HudEdge::Left, 10.),
        Err(HudLayoutError::Capacity)
    );
    same_fit(
        before,
        Canvas2D::from_fit(hud.fit_canvas([400., 200.]).unwrap().unwrap()),
    );
    hud.clear();
    let reset = Canvas2D::from_fit(hud.fit_canvas([400., 200.]).unwrap().unwrap());
    same_fit(
        reset,
        Canvas2D::try_fit(vec2(400., 200.), vec2(800., 600.))
            .unwrap()
            .unwrap(),
    );
}

thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
struct CountingAllocator;
fn allocated(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = ALLOCATIONS.try_with(|counter| {
            if let Some(count) = counter.get() {
                counter.set(Some(count + 1));
            }
        });
    }
}
// SAFETY: System receives the original allocator arguments unchanged. The
// thread-local counter allocates nothing and tolerates thread teardown.
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

#[test]
fn checked_fit_adapter_camera_and_pointer_paths_allocate_nothing() {
    struct StopCounting;
    impl Drop for StopCounting {
        fn drop(&mut self) {
            ALLOCATIONS.with(|counter| counter.set(None));
        }
    }
    ALLOCATIONS.with(|counter| counter.set(Some(0)));
    let guard = StopCounting;
    for _ in 0..64 {
        let screen = black_box(vec2(800., 600.));
        let world = black_box(vec2(400., 200.));
        let mut hud = HudRegions::new(screen.to_array(), [10., 20., 30., 40.], [0.; 2]).unwrap();
        hud.reserve("header", HudEdge::Top, 60.).unwrap();
        let fit = hud.fit_canvas(world.to_array()).unwrap().unwrap();
        let canvas = black_box(Canvas2D::from_fit(fit));
        black_box(canvas.camera());
        black_box(canvas.unproject(canvas.project(world * 0.5)));
        black_box(canvas.pointer_half_open(canvas.project(world * 0.5)));
        black_box(Canvas2D::try_fit_in(world, screen, hud.content()).unwrap());
        black_box(Canvas2D::try_fit(world, screen).unwrap());
        assert!(Canvas2D::try_fit(world, Vec2::ZERO).unwrap().is_none());
        assert!(Canvas2D::try_fit(Vec2::ZERO, screen).is_err());
        hud.clear();
    }
    let count = ALLOCATIONS.with(|counter| counter.get().unwrap());
    drop(guard);
    assert_eq!(count, 0);
}
