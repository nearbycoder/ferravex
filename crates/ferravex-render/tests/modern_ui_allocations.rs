//! Styled text, anti-aliased shapes, glows, kit widgets and tweens allocate
//! nothing once the frame's vertex stream has grown to its working size.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {
    static RECORD: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
}
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
fn modern_text_shapes_kit_and_tweens_allocate_nothing_after_warmup() {
    use ferravex_play::tween::{
        Ease, Entrance, Motion, RollCounter, ScreenTransition, Spring, TransitionKind, Tween,
    };
    use ferravex_render::{
        Color, Rect,
        input::host_frame,
        shapes::{self, Cap, Fill, Join, Shadow, Stroke},
        text::{TextAlign, TextStyle, draw_text_styled, draw_text_wrapped, wrap_text},
        ui::kit::{self, ButtonKind, ButtonMotion, ButtonState, KitTheme, Tone},
        vec2,
    };
    let theme = KitTheme::default();
    let mut button = ButtonMotion::default();
    let mut tween = Tween::new(0., 1., 0.5, Ease::BackOut);
    let mut spring = Spring::underdamped(0., 4., 0.4);
    let mut score = RollCounter::new(0);
    let mut transition = ScreenTransition::new(TransitionKind::Iris, 0.4);
    let path = [
        vec2(10., 10.),
        vec2(60., 40.),
        vec2(110., 10.),
        vec2(160., 50.),
    ];
    let mut draw = |frame: usize| {
        host_frame(1. / 60., 1280, 720, || {
            let dt = 1. / 60.;
            let t = tween.advance(dt, Motion::Full);
            spring.set_target((frame % 2) as f32);
            spring.advance(dt, Motion::Full);
            score.set_target(frame as i64 * 10);
            score.advance(dt, Motion::Full);
            if frame.is_multiple_of(30) {
                transition.cover();
            }
            transition.advance(dt, Motion::Full);
            let style = TextStyle::modern(16., Color::new(1., 1., 1., 1.))
                .with_shadow(Color::new(0., 0., 0., 0.5))
                .with_outline(Color::new(0., 0., 0., 1.));
            draw_text_styled("Score 12,480 ★ — Ünïcödé", 20., 40., style);
            draw_text_styled("Right", 1260., 80., style.with_align(TextAlign::Right));
            draw_text_wrapped(
                "Wrapped copy keeps words together and splits long ones.",
                20.,
                120.,
                200.,
                style,
            );
            assert!(wrap_text("a b c d e f g", 10., style).count() > 1);
            let card = Rect::new(300., 40., 300., 200.);
            shapes::drop_shadow(card, 12., Shadow::soft(Color::new(0., 0., 0., 0.5)));
            shapes::fill_rounded_rect(
                card,
                12.,
                Fill::vertical(
                    card,
                    Color::new(0.2, 0.2, 0.3, 1.),
                    Color::new(0.1, 0.1, 0.2, 1.),
                ),
            );
            shapes::stroke_rounded_rect(card, 12., 1., Color::new(1., 1., 1., 0.1));
            shapes::fill_circle(vec2(400., 300.), 20. + t, Color::new(1., 0.5, 0.2, 1.));
            shapes::stroke_circle(vec2(460., 300.), 20., 3., Color::new(1., 1., 1., 1.));
            shapes::stroke_arc(
                vec2(520., 300.),
                20.,
                0.,
                4.,
                Stroke::new(4., Color::new(1., 1., 1., 1.)).cap(Cap::Round),
            );
            shapes::fill_capsule(
                vec2(560., 300.),
                vec2(640., 320.),
                6.,
                Color::new(0., 1., 1., 1.),
            );
            shapes::polyline(
                &path,
                Stroke::new(3., Color::new(1., 0.4, 0.6, 1.))
                    .join(Join::Round)
                    .cap(Cap::Round),
            );
            shapes::glow_disc(vec2(700., 300.), 40., Color::new(1., 0.6, 0.2, 0.8));
            shapes::glow_ring(vec2(800., 300.), 30., 10., Color::new(0.4, 0.8, 1., 0.8));
            let inner = kit::card(Rect::new(40., 400., 360., 240.), Some("Paused"), &theme);
            button.update(
                ButtonState {
                    hovered: frame.is_multiple_of(3),
                    ..ButtonState::default()
                },
                dt,
                Motion::Full,
            );
            kit::button(
                Rect::new(inner.x, inner.y, 140., 36.),
                "Resume",
                ButtonKind::Primary,
                &button,
                &theme,
            );
            kit::meter(
                Rect::new(inner.x, inner.y + 50., 200., 12.),
                spring.value,
                Some(0.9),
                theme.danger,
                &theme,
            );
            kit::key_prompt(inner.x, inner.y + 80., &["Shift", "Space"], "Dash", &theme);
            let look = Entrance::fade_up(10.).sample(frame as f32 * dt, 0, Motion::Full);
            kit::toast(640., 20., Some('★'), "Saved", Tone::Success, look, &theme);
            kit::draw_screen_transition(
                transition.kind(Motion::Full),
                transition.coverage(),
                Color::new(0., 0., 0., 1.),
                vec2(640., 360.),
            );
        })
    };
    for frame in 0..4 {
        draw(frame);
    }
    CALLS.with(|n| n.set(0));
    RECORD.with(|on| on.set(true));
    for frame in 4..304 {
        draw(frame);
    }
    RECORD.with(|on| on.set(false));
    assert_eq!(CALLS.with(Cell::get), 0);
}
