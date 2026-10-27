//! The optimized immediate path must keep post/glow frame state and modern HUD
//! drawing allocation-free when the world and HUD share one frame.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
}
struct Allocator;
fn allocated(pointer: *mut u8) {
    if !pointer.is_null() {
        let _ = RECORD.try_with(|record| {
            if record.get() {
                let _ = CALLS.try_with(|calls| calls.set(calls.get() + 1));
            }
        });
    }
}
// SAFETY: Each operation preserves System's allocation/deallocation contract;
// the thread-local accounting is nonallocating and tolerates TLS destruction.
unsafe impl GlobalAlloc for Allocator {
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
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;

#[test]
fn optimized_world_glow_and_modern_hud_preserve_boundaries_without_allocating() {
    use ferravex_render::{
        Camera2D, Color, Rect, WHITE, draw_circle, draw_circle_lines, draw_ellipse, draw_line,
        draw_line_3d, draw_poly, draw_rectangle, draw_rectangle_lines, draw_triangle,
        input::host_frame,
        post::{self, PostFx, PostTier},
        set_camera, set_default_camera, shapes,
        text::{TextStyle, draw_text_styled},
        ui::kit::{self, KitTheme},
        vec2, vec3,
    };
    let theme = KitTheme::default();
    post::set_post_fx(PostFx::neon().auto_hud(false));
    let draw = || {
        host_frame(1. / 60., 1280, 800, || {
            assert!(
                !post::post_fx_status().hud_started,
                "each frame resets its boundary"
            );
            set_camera(&Camera2D::default());
            draw_rectangle(0., 0., 1280., 800., Color::new(0.02, 0.03, 0.07, 1.));
            post::with_glow(|| {
                draw_circle(200., 180., 20., WHITE);
                draw_poly(240., 180., 200, 20., 13., WHITE);
                draw_ellipse(280., 180., 20., 10., 0.2, WHITE);
                draw_line(100., 100., 300., 200., 4., WHITE);
                draw_rectangle_lines(100., 100., 200., 100., 2., WHITE);
                draw_circle_lines(220., 180., 30., 2., WHITE);
                draw_triangle(vec2(10., 20.), vec2(30., 40.), vec2(50., 10.), WHITE);
                draw_line_3d(vec3(0., 0., 0.), vec3(1., 2., 3.), WHITE);
                post::with_glow(|| draw_rectangle(200., 200., 40., 20., WHITE));
            });
            shapes::glow_disc(vec2(200., 180.), 32., Color::new(0.3, 0.9, 1., 0.6));
            set_default_camera();
            assert!(
                !post::post_fx_status().hud_started,
                "auto_hud was explicitly disabled"
            );
            assert!(post::post_fx_hud_begin());
            assert!(
                !post::post_fx_hud_begin(),
                "only one boundary is emitted per frame"
            );
            kit::card(Rect::new(20., 20., 260., 100.), Some("Ready"), &theme);
            draw_text_styled("Score 12,480 ★", 36., 90., TextStyle::modern(16., WHITE));
            let status = post::post_fx_status();
            assert_eq!(status.tier, PostTier::Medium);
            assert!(status.hud_started);
        });
    };
    for _ in 0..4 {
        draw();
    }
    CALLS.with(|calls| calls.set(0));
    RECORD.with(|record| record.set(true));
    for _ in 0..180 {
        draw();
    }
    RECORD.with(|record| record.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
    post::set_post_fx(PostFx::off());
}
