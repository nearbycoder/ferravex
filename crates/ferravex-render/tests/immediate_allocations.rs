//! Performance guards for the immediate path: once vertex/batch storage has
//! grown, a HUD-heavy 2D frame plus a resident-cube scene allocates nothing,
//! and repeated `draw_cube` calls keep reusing their resident meshes.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {static RECORD:Cell<bool>=const {Cell::new(false)};static CALLS:Cell<usize>=const {Cell::new(0)};}
struct Allocator;
fn allocated(p: *mut u8) {
    if !p.is_null() {
        let _ = RECORD.try_with(|on| {
            if on.get() {
                let _ = CALLS.try_with(|n| n.set(n.get() + 1));
            }
        });
    }
}
// SAFETY: Every operation forwards the identical allocation contract to System;
// thread-local counters do not allocate and ignore destruction-time TLS failures.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        allocated(p);
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        allocated(p);
        p
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        let p = unsafe { System.realloc(p, l, n) };
        allocated(p);
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

#[test]
fn steady_immediate_hud_and_cube_frames_allocate_nothing() {
    use ferravex_render::{
        Camera3D, Color, WHITE, draw_circle, draw_circle_lines, draw_cube, draw_ellipse, draw_line,
        draw_line_3d, draw_poly, draw_rectangle, draw_rectangle_lines, draw_text, draw_triangle,
        input::host_frame, retained::telemetry_snapshot, set_camera, set_default_camera, vec2,
        vec3,
    };
    let draw = |_frame: usize| {
        host_frame(1. / 60., 1280, 720, || {
            set_camera(&Camera3D::default());
            for i in 0..24 {
                let f = i as f32;
                let color = Color::new(0.2 + (i % 4) as f32 * 0.2, 0.5, 0.7, 1.);
                draw_cube(
                    vec3(f - 12., 0., -4.),
                    vec3(1., 0.5 + (i % 3) as f32, 1.),
                    None,
                    color,
                );
            }
            draw_line_3d(vec3(0., 0., 0.), vec3(1., 2., 3.), WHITE);
            set_default_camera();
            for i in 0..40 {
                let f = i as f32 * 3.;
                draw_circle(f, 40., 6., WHITE);
                draw_poly(f, 80., 6, 5., f, WHITE);
                draw_rectangle(f, 120., 8., 4., WHITE);
                draw_line(f, 0., f + 9., 9., 2., WHITE);
            }
            draw_rectangle_lines(4., 4., 300., 200., 2., WHITE);
            draw_circle_lines(200., 200., 30., 2., WHITE);
            draw_ellipse(300., 200., 30., 10., 15., WHITE);
            draw_triangle(vec2(0., 0.), vec2(9., 0.), vec2(0., 9.), WHITE);
            for row in 0..12 {
                draw_text(
                    "SCORE 000123 / LAP 3",
                    10.,
                    300. + row as f32 * 14.,
                    12.,
                    WHITE,
                );
            }
            telemetry_snapshot().live_meshes
        })
    };
    let mut warm = 0;
    for frame in 0..4 {
        warm = draw(frame);
    }
    CALLS.with(|n| n.set(0));
    RECORD.with(|on| on.set(true));
    for frame in 0..200 {
        assert_eq!(draw(frame), warm, "draw_cube must reuse resident meshes");
    }
    RECORD.with(|on| on.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
}

#[test]
fn changing_cube_styles_keeps_only_the_bounded_resident_cache() {
    use ferravex_render::{
        Camera3D, Color, draw_cube, input::host_frame, retained::telemetry_snapshot, set_camera,
        vec3,
    };
    for style in 0..256 {
        host_frame(1. / 60., 1280, 720, || {
            set_camera(&Camera3D::default());
            draw_cube(
                vec3(0., 0., 0.),
                vec3(1., 1., 1.),
                None,
                Color::new(style as f32 / 256., 0.5, 0.7, 1.),
            );
            let live = telemetry_snapshot().live_meshes;
            assert!(live <= 64, "evicted cuboids must be released: {live}");
        });
    }
}
