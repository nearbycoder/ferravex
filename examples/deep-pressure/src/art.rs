//! Shared palette, flat text, gradient triangles and the game's own glyphs.
//!
//! One idea, one look: air is the bubble glyph in aqua, the hull is a steel
//! shield, survivors are people (aqua aboard, gold once home), credits are
//! coins and recovery tugs are ships.
use crate::content::HazardKind;
use ferravex_arcade::shapes::{self, Cap, Fill, Stroke};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;
use ferravex_render::effects::{self, EffectBlend, EffectStyle, RibbonTriangle, RibbonVertex};
use std::f32::consts::{PI, TAU};

pub(crate) const AQUA: Color = Color::new(0.29, 0.91, 0.88, 1.);
pub(crate) const GOLD: Color = Color::new(1., 0.76, 0.27, 1.);
pub(crate) const CORAL: Color = Color::new(1., 0.42, 0.42, 1.);
pub(crate) const STEEL: Color = Color::new(0.62, 0.76, 1., 1.);
pub(crate) const VIOLET: Color = Color::new(0.82, 0.6, 0.98, 1.);
pub(crate) const MINT: Color = Color::new(0.52, 0.96, 0.58, 1.);
pub(crate) const AMBER: Color = Color::new(1., 0.64, 0.26, 1.);
pub(crate) const HAIRLINE: Color = Color::new(1., 1., 1., 0.09);
/// Hollow, not-yet-relevant marks.
pub(crate) const DIM: Color = Color::new(0.6, 0.67, 0.74, 0.35);

pub(crate) fn mix(a: Color, b: Color, t: f32) -> Color {
    shapes::mix(a, b, t)
}
pub(crate) fn hash(i: usize) -> usize {
    (i.wrapping_mul(2_654_435_761) >> 7) % 1009
}

type Vertex = (Vec2, Color);
/// Per-vertex coloured triangles in the ordinary stream (gradients and
/// lights without extra textures or shaders).
fn emit(blend: EffectBlend, triangles: impl IntoIterator<Item = [Vertex; 3]>) {
    effects::draw_triangles(
        triangles.into_iter().map(|t| RibbonTriangle {
            vertices: t.map(|(p, c)| RibbonVertex {
                position: [p.x, p.y, 0.],
                color: [c.r, c.g, c.b, c.a],
                uv: [0., 0.],
            }),
        }),
        EffectStyle {
            blend,
            depth_test: Some(false),
            depth_write: false,
        },
    );
}
/// Alpha-blended gradient triangles.
pub(crate) fn shade(triangles: impl IntoIterator<Item = [Vertex; 3]>) {
    emit(EffectBlend::Alpha, triangles);
}
/// Quad `a b / c d` (top pair, bottom pair) as two shaded triangles.
pub(crate) fn quad(a: Vertex, b: Vertex, c: Vertex, d: Vertex) -> [[Vertex; 3]; 2] {
    [[a, b, c], [c, b, d]]
}
/// Unit circle in 24 steps, computed once.
fn unit_ring() -> &'static [Vec2; 25] {
    static RING: std::sync::OnceLock<[Vec2; 25]> = std::sync::OnceLock::new();
    RING.get_or_init(|| {
        std::array::from_fn(|i| {
            let a = i as f32 * TAU / 24.;
            vec2(a.cos(), a.sin())
        })
    })
}
/// Cheap additive light: three bands approximating a quadratic falloff,
/// built from a cached unit ring so no trigonometry runs per frame.
pub(crate) fn light(center: Vec2, radius: f32, color: Color) {
    const BANDS: [(f32, f32); 4] = [(0., 1.), (0.35, 0.42), (0.7, 0.09), (1., 0.)];
    let ring = unit_ring();
    let at = |band: usize, i: usize| {
        let (r, a) = BANDS[band];
        (
            center + ring[i] * radius * r,
            with_alpha(color, color.a * a),
        )
    };
    emit(
        EffectBlend::Additive,
        (0..24).flat_map(move |i| {
            [
                [at(0, i), at(1, i), at(1, i + 1)],
                [at(1, i), at(2, i), at(1, i + 1)],
                [at(1, i + 1), at(2, i), at(2, i + 1)],
                [at(2, i), at(3, i), at(2, i + 1)],
                [at(2, i + 1), at(3, i), at(3, i + 1)],
            ]
        }),
    );
}

/// Flat text for card surfaces, where a drop shadow would be invisible and
/// would double the glyph geometry.
pub(crate) fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
pub(crate) fn say(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(size, color)).width
}
/// Text vertically centered on `cy`, starting at `x`; returns its width.
pub(crate) fn say_mid(value: &str, x: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color);
    draw_text_styled(value, x, s.centered_baseline(cy), s).width
}
/// Text vertically centered on `cy` and horizontally on `cx`.
pub(crate) fn say_mid_centered(value: &str, cx: f32, cy: f32, size: f32, color: Color) {
    let s = ink(size, color).with_align(TextAlign::Center);
    draw_text_styled(value, cx, s.centered_baseline(cy), s);
}
/// Flat text shrunk (never enlarged) to fit `width`.
pub(crate) fn say_fit(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) {
    let s = ink(size, color);
    let w = text_width(value, s);
    let s = if w > width && w > 0. {
        s.with_size(size * width / w)
    } else {
        s
    };
    draw_text_styled(value, x, y, s);
}
pub(crate) fn icon_chip_right(icon: Icon, label: &str, right: f32, y: f32, size: f32, c: Color) {
    icon_chip(
        icon,
        label,
        right - icon_chip_width(label, size),
        y,
        size,
        c,
    );
}
/// Key cap width as [`keycap`] draws it without an action label.
pub(crate) fn keycap_w(key: &str) -> f32 {
    (text_width(key, TextStyle::modern(13., PAPER)) + 12.).max(20.)
}
/// `seconds` as `m:ss`.
pub(crate) fn clock_label(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// Air: three bubbles of falling size rising apart from each other.
pub(crate) fn air_glyph(cx: f32, cy: f32, size: f32, color: Color) {
    let u = size * 0.5;
    let pen = (0.14 * u).max(1.2);
    let fill = with_alpha(color, color.a * 0.24);
    let shine = with_alpha(PAPER, color.a * 0.85);
    let big = vec2(cx - 0.3 * u, cy + 0.36 * u);
    shapes::fill_circle(big, 0.48 * u, fill);
    shapes::stroke_circle(big, 0.48 * u, pen, color);
    shapes::stroke_arc(
        big,
        0.26 * u,
        PI * 1.08,
        PI * 0.4,
        Stroke::new((0.11 * u).max(1.), shine).cap(Cap::Round),
    );
    let mid = vec2(cx + 0.5 * u, cy - 0.12 * u);
    shapes::fill_circle(mid, 0.27 * u, fill);
    shapes::stroke_circle(mid, 0.27 * u, pen, color);
    shapes::fill_circle(mid - vec2(0.09, 0.09) * u, 0.06 * u, shine);
    shapes::fill_circle(vec2(cx + 0.06 * u, cy - 0.7 * u), 0.16 * u, color);
}
/// A cross badge over the lower right of an icon box, for "missing" states.
pub(crate) fn cross_badge(cx: f32, cy: f32, size: f32) {
    let c = vec2(cx + size * 0.34, cy + size * 0.32);
    shapes::fill_circle(c, size * 0.24, CORAL);
    let d = size * 0.1;
    let pen = Stroke::new((size * 0.06).max(1.2), INK).cap(Cap::Round);
    shapes::polyline(&[c - vec2(d, d), c + vec2(d, d)], pen);
    shapes::polyline(&[c + vec2(-d, d), c + vec2(d, -d)], pen);
}
/// Medal tone: 3 gold, 2 silver, 1 bronze, otherwise unearned.
pub(crate) fn medal_metal(medal: u8) -> Color {
    match medal {
        3 => Color::new(1., 0.8, 0.3, 1.),
        2 => Color::new(0.8, 0.86, 0.92, 1.),
        1 => Color::new(0.85, 0.55, 0.32, 1.),
        _ => MUTED,
    }
}
/// A struck medal disc of radius `r`, with a ribbon when `ribbon` is set.
pub(crate) fn medal(c: Vec2, r: f32, medal: u8, ribbon: bool) {
    let metal = medal_metal(medal);
    if medal == 0 {
        shapes::stroke_circle(c, r * 0.85, 1., Color::new(1., 1., 1., 0.16));
        return;
    }
    if ribbon {
        let s = r / 11.;
        for side in [-1., 1.] {
            draw_triangle(
                c + vec2(side * 7., -14.) * s,
                c + vec2(side * 1., -14.) * s,
                c + vec2(side * 4., 0.) * s,
                darken(AQUA, 0.3),
            );
        }
    }
    shapes::fill_circle(
        c,
        r,
        Fill::radial(
            c - vec2(r * 0.27, r * 0.27),
            r * 1.27,
            lighten(metal, 0.45),
            darken(metal, 0.25),
        ),
    );
    shapes::stroke_circle(c, r * 0.68, 1., with_alpha(darken(metal, 0.4), 0.8));
}
/// Miniature of an authored hazard, matching its chart art, in a `size` box.
pub(crate) fn hazard_glyph(kind: HazardKind, c: Vec2, size: f32) {
    let s = size / 24.;
    match kind {
        HazardKind::Jelly => {
            for j in -1..=1 {
                let x = c.x + j as f32 * 3.5 * s;
                draw_line(
                    x,
                    c.y,
                    x + j as f32 * 1.5 * s,
                    c.y + 10. * s,
                    1.2 * s,
                    Color::new(0.8, 0.6, 1., 0.8),
                );
            }
            shapes::fill_sector(
                c,
                9. * s,
                PI,
                PI,
                Fill::radial(
                    c - vec2(2., 5.) * s,
                    10. * s,
                    Color::new(0.98, 0.86, 1., 1.),
                    Color::new(0.6, 0.34, 0.86, 1.),
                ),
            );
        }
        HazardKind::Mine => {
            for i in 0..8 {
                let a = i as f32 * TAU / 8. + 0.2;
                let tip = c + vec2(a.cos(), a.sin()) * 10. * s;
                draw_line(
                    c.x,
                    c.y,
                    tip.x,
                    tip.y,
                    1.6 * s,
                    Color::new(0.5, 0.42, 0.45, 1.),
                );
                shapes::fill_circle(tip, 1.4 * s, CORAL);
            }
            shapes::fill_circle(
                c,
                6.5 * s,
                Fill::radial(
                    c - vec2(2., 2.) * s,
                    8. * s,
                    Color::new(0.56, 0.48, 0.5, 1.),
                    Color::new(0.16, 0.11, 0.13, 1.),
                ),
            );
            shapes::fill_circle(c, 1.8 * s, CORAL);
        }
        HazardKind::Vent => {
            shapes::fill_polygon(
                &[
                    c + vec2(-4., 2.) * s,
                    c + vec2(4., 2.) * s,
                    c + vec2(10., 11.) * s,
                    c + vec2(-10., 11.) * s,
                ],
                Color::new(0.38, 0.24, 0.2, 1.),
            );
            for (i, dx) in [-3.5f32, 0., 3.5].into_iter().enumerate() {
                shapes::fill_circle(
                    c + vec2(dx, -3. - i as f32 * 3.) * s,
                    (2.6 + i as f32 * 0.6) * s,
                    Color::new(1., 0.6, 0.3, 0.75 - i as f32 * 0.15),
                );
            }
        }
    }
}
/// Dashed circle (`dashes` arcs) for approach ranges.
pub(crate) fn dashed_ring(c: Vec2, r: f32, width: f32, dashes: usize, spin: f32, color: Color) {
    let step = TAU / dashes as f32;
    for i in 0..dashes {
        shapes::stroke_arc(
            c,
            r,
            spin + i as f32 * step,
            step * 0.55,
            Stroke::new(width, color).cap(Cap::Round),
        );
    }
}
