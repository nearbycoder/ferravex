//! Display-free microbenchmark for the immediate 2D drawing path.
//!
//! `cargo run --release -p ferravex-render --example shapes_bench --offline --locked`
//!
//! Each workload records 32 calls per host frame for enough frames to take
//! about 50 ms, then reports the median microseconds per call over seven
//! repetitions, after subtracting an empty frame.
//! Wall-clock timings are local observations for comparing builds on one
//! machine, not portable performance guarantees.
use ferravex_render::{
    self as render, Color, Rect, input, post,
    shapes::{self, Cap, Join, Shadow, Stroke},
    text::{self, TextStyle},
    ui::kit::{self, KitTheme},
    vec2,
};
use std::time::Instant;

const PER_FRAME: usize = 32;

fn time_frames(frames: usize, draw: &dyn Fn(usize)) -> f64 {
    let start = Instant::now();
    for frame in 0..frames {
        input::host_frame(1. / 60., 1280, 800, || {
            for i in 0..PER_FRAME {
                draw(frame * PER_FRAME + i);
            }
        });
    }
    start.elapsed().as_secs_f64() * 1e6
}

fn main() {
    let theme = KitTheme::default();
    let white = Color::new(1., 1., 1., 1.);
    let gold = Color::from_hex(0xf7b553);
    let path = [
        vec2(10., 80.),
        vec2(60., 20.),
        vec2(110., 70.),
        vec2(160., 30.),
    ];
    let jitter = |i: usize| (i % 7) as f32 * 3.;
    type Workload<'a> = (&'a str, Box<dyn Fn(usize) + 'a>);
    let workloads: Vec<Workload> = vec![
        ("empty", Box::new(|_| {})),
        (
            "draw_rectangle",
            Box::new(|i| render::draw_rectangle(10. + jitter(i), 10., 120., 40., gold)),
        ),
        (
            "fill_rounded_rect 120x40 r8",
            Box::new(|i| {
                shapes::fill_rounded_rect(Rect::new(10. + jitter(i), 10., 120., 40.), 8., gold)
            }),
        ),
        (
            "fill_rounded_rect vertical gradient",
            Box::new(|i| {
                let r = Rect::new(10. + jitter(i), 10., 120., 40.);
                shapes::fill_rounded_rect(r, 8., shapes::Fill::vertical(r, gold, white))
            }),
        ),
        (
            "stroke_rounded_rect 120x40 r8",
            Box::new(|i| {
                shapes::stroke_rounded_rect(
                    Rect::new(10. + jitter(i), 10., 120., 40.),
                    8.,
                    1.,
                    white,
                )
            }),
        ),
        (
            "drop_shadow 300x200 r16 blur24",
            Box::new(|i| {
                shapes::drop_shadow(
                    Rect::new(10. + jitter(i), 10., 300., 200.),
                    16.,
                    Shadow::soft(Color::new(0., 0., 0., 0.5)).blur(24.),
                )
            }),
        ),
        (
            "kit::card 300x200",
            Box::new(|i| {
                kit::card(Rect::new(10. + jitter(i), 10., 300., 200.), None, &theme);
            }),
        ),
        (
            "kit::keycap E",
            Box::new(|i| {
                kit::keycap(10. + jitter(i), 10., "E", &theme);
            }),
        ),
        (
            "kit::meter",
            Box::new(|i| {
                kit::meter(
                    Rect::new(10. + jitter(i), 10., 180., 12.),
                    0.4,
                    Some(0.65),
                    gold,
                    &theme,
                )
            }),
        ),
        (
            "fill_circle r12",
            Box::new(|i| shapes::fill_circle(vec2(40. + jitter(i), 40.), 12., gold)),
        ),
        (
            "fill_circle r120",
            Box::new(|i| shapes::fill_circle(vec2(200. + jitter(i), 200.), 120., gold)),
        ),
        (
            "stroke_circle r40 w3",
            Box::new(|i| shapes::stroke_circle(vec2(80. + jitter(i), 80.), 40., 3., white)),
        ),
        (
            "fill_capsule",
            Box::new(|i| {
                shapes::fill_capsule(vec2(20. + jitter(i), 20.), vec2(120., 40.), 6., gold)
            }),
        ),
        (
            "stroke_arc r18 0.7 turn round cap",
            Box::new(|i| {
                shapes::stroke_arc(
                    vec2(40. + jitter(i), 40.),
                    18.,
                    -1.5,
                    4.4,
                    Stroke::new(5., gold).cap(Cap::Round),
                )
            }),
        ),
        (
            "polyline 4 pts round",
            Box::new(|i| {
                let _ = i;
                shapes::polyline(
                    &path,
                    Stroke::new(4., gold).join(Join::Round).cap(Cap::Round),
                )
            }),
        ),
        (
            "glow_disc r24",
            Box::new(|i| shapes::glow_disc(vec2(64. + jitter(i), 64.), 24., gold)),
        ),
        (
            "glow_disc r80",
            Box::new(|i| shapes::glow_disc(vec2(64. + jitter(i), 64.), 80., gold)),
        ),
        (
            "glow_ring r30 w12",
            Box::new(|i| shapes::glow_ring(vec2(64. + jitter(i), 64.), 30., 12., gold)),
        ),
        (
            "post::glow_circle r40",
            Box::new(|i| post::glow_circle(64. + jitter(i), 64., 40., gold, 1.)),
        ),
        (
            "draw_circle r20",
            Box::new(|i| render::draw_circle(64. + jitter(i), 64., 20., gold)),
        ),
        (
            "draw_ellipse 30x12",
            Box::new(|i| render::draw_ellipse(64. + jitter(i), 64., 30., 12., 15., gold)),
        ),
        (
            "draw_text 16 chars size 16",
            Box::new(|i| {
                render::draw_text("SCORE 0012345 OK", 10. + jitter(i), 40., 16., white);
            }),
        ),
        (
            "draw_text_styled modern 16 chars",
            Box::new(|i| {
                text::draw_text_styled(
                    "Score 0012345 ok",
                    10. + jitter(i),
                    40.,
                    TextStyle::modern(16., white),
                );
            }),
        ),
    ];
    let mut empty = 0.;
    println!("{:40} {:>10}", "workload", "us/call");
    for (name, draw) in &workloads {
        // Calibrate frames so a repetition takes roughly 50 ms.
        let probe = time_frames(20, draw.as_ref()) / 20.;
        let frames = ((50_000. / probe.max(1.)) as usize).clamp(20, 20_000);
        let mut samples: Vec<f64> = (0..7)
            .map(|_| time_frames(frames, draw.as_ref()) / frames as f64)
            .collect();
        samples.sort_by(f64::total_cmp);
        let per_frame = samples[3];
        if *name == "empty" {
            empty = per_frame;
            println!("{name:40} {:>10.3} (frame overhead)", per_frame);
            continue;
        }
        println!("{name:40} {:>10.3}", (per_frame - empty) / PER_FRAME as f64);
    }
}
