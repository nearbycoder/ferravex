//! Presentation for Wildfire Watch: a painted district map, crew tokens, the
//! fire forecast and an incident-command sidebar.
//!
//! Everything here reads the model. Pointer hit regions live in the model, so
//! every clickable surface below is drawn inside them.
use crate::content::{BASE, CELLS, COLS, MISSIONS, ROWS};
use crate::model::{Game, Phase, Terrain, Tile};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;
use std::f32::consts::FRAC_1_SQRT_2;

const AMBER: Color = Color::new(1., 0.64, 0.32, 1.);
const GREEN: Color = Color::new(0.38, 0.73, 0.48, 1.);
const BLUE: Color = Color::new(0.35, 0.72, 0.92, 1.);
const RED: Color = Color::new(1., 0.34, 0.25, 1.);
const SOFT: Color = Color::new(0.8, 0.85, 0.88, 1.);
const CREW: [Color; 3] = [AMBER, GREEN, BLUE];
const NAMES: [&str; 3] = ["Engine", "Ranger", "Rescue"];
/// Map geometry shared with the model's pointer mapping.
const MAP_X: f32 = 28.;
const MAP_Y: f32 = 162.;
const TW: f32 = 50.;
const TH: f32 = 46.;
const SIDE_X: f32 = 610.;
const SIDE_W: f32 = 326.;

// --- Text: flat surfaces need no drop shadow, which halves glyph geometry. ---

fn label(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, TextStyle::modern(size, color)).width
}
fn label_right(value: &str, right: f32, y: f32, size: f32, color: Color) -> f32 {
    let s = TextStyle::modern(size, color).with_align(TextAlign::Right);
    draw_text_styled(value, right, y, s).width
}
fn label_center(value: &str, cx: f32, y: f32, size: f32, color: Color) {
    let s = TextStyle::modern(size, color).with_align(TextAlign::Center);
    draw_text_styled(value, cx, y, s);
}
fn label_fit(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) -> f32 {
    let s = TextStyle::modern(size, color);
    let w = text_width(value, s);
    let s = if w > width && w > 0. {
        s.with_size(size * width / w)
    } else {
        s
    };
    draw_text_styled(value, x, y, s).width
}
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    let s = TextStyle::modern(11., color).with_letter_spacing(1.5);
    draw_text_styled(value, x, y, s).width
}
fn paragraph(
    value: &str,
    x: f32,
    y: f32,
    width: f32,
    size: f32,
    max_lines: usize,
    color: Color,
) -> f32 {
    let s = TextStyle::modern(size, color);
    let mut baseline = y;
    for line in wrap_text(value, width, s).take(max_lines) {
        draw_text_styled(line, x, baseline, s);
        baseline += size * 1.4;
    }
    baseline
}
/// "MOVE CREW" -> "Move crew".
fn sentence(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for (i, c) in value.chars().enumerate() {
        out.push(if i == 0 { c } else { c.to_ascii_lowercase() });
    }
    out
}
fn wind_name(w: (i8, i8)) -> &'static str {
    match w {
        (-1, 0) => "West",
        (1, 0) => "East",
        (0, -1) => "North",
        _ => "South",
    }
}

// --- Small shape vocabulary. ---

/// Unit 12-gon (30 degree steps). Map details are drawn from this table so
/// the per-frame map never evaluates trigonometry.
const RING: [(f32, f32); 12] = [
    (1., 0.),
    (0.866_025_4, 0.5),
    (0.5, 0.866_025_4),
    (0., 1.),
    (-0.5, 0.866_025_4),
    (-0.866_025_4, 0.5),
    (-1., 0.),
    (-0.866_025_4, -0.5),
    (-0.5, -0.866_025_4),
    (0., -1.),
    (0.5, -0.866_025_4),
    (0.866_025_4, -0.5),
];
/// Convex fan over every `step`-th point of [`RING`], scaled by `rx`/`ry`.
fn fan(x: f32, y: f32, rx: f32, ry: f32, offset: usize, step: usize, color: Color) {
    let n = RING.len() / step;
    let at = |k: usize| {
        let (u, v) = RING[((k % n) * step + offset) % RING.len()];
        vec2(x + u * rx, y + v * ry)
    };
    let center = vec2(x, y);
    for k in 0..n {
        draw_triangle(center, at(k), at(k + 1), color);
    }
}
/// Filled disc; a 12-gon reads as round at token sizes.
fn dot(x: f32, y: f32, r: f32, color: Color) {
    fan(x, y, r, r, 0, 1, color);
}
/// Capsule of height `h` from a flat fill and two end discs.
fn pill(x: f32, y: f32, w: f32, h: f32, color: Color) {
    let r = h * 0.5;
    if w <= h {
        dot(x + w * 0.5, y + r, w.min(h) * 0.5, color);
        return;
    }
    panel(x + r, y, w - h, h, color);
    dot(x + r, y + r, r, color);
    dot(x + w - r, y + r, r, color);
}
fn brackets(x: f32, y: f32, w: f32, h: f32, arm: f32, t: f32, color: Color) {
    panel(x, y, arm, t, color);
    panel(x, y, t, arm, color);
    panel(x + w - arm, y, arm, t, color);
    panel(x + w - t, y, t, arm, color);
    panel(x, y + h - t, arm, t, color);
    panel(x, y + h - arm, t, arm, color);
    panel(x + w - arm, y + h - t, arm, t, color);
    panel(x + w - t, y + h - arm, t, arm, color);
}
// --- Trig-free surfaces for every-frame interface. The anti-aliased host
// widgets evaluate sin/cos per outline point on each call; these build the
// same shapes from flat fills and a constant quarter arc instead. ---

/// Unit quarter arc in 15 degree steps, from +x to +y.
const QUARTER: [(f32, f32); 7] = [
    (1., 0.),
    (0.965_925_8, 0.258_819),
    (0.866_025_4, 0.5),
    (FRAC_1_SQRT_2, FRAC_1_SQRT_2),
    (0.5, 0.866_025_4),
    (0.258_819, 0.965_925_8),
    (0., 1.),
];
/// Flat rounded rectangle: three fills and four corner fans that never
/// overlap, so translucent colours blend exactly once.
fn slab(x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    let r = r.min(w * 0.5).min(h * 0.5).max(0.);
    if r < 0.5 {
        panel(x, y, w, h, color);
        return;
    }
    panel(x + r, y, w - 2. * r, h, color);
    if h > 2. * r {
        panel(x, y + r, r, h - 2. * r, color);
        panel(x + w - r, y + r, r, h - 2. * r, color);
    }
    for (cx, cy, sx, sy) in [
        (x + w - r, y + h - r, 1., 1.),
        (x + r, y + h - r, -1., 1.),
        (x + r, y + r, -1., -1.),
        (x + w - r, y + r, 1., -1.),
    ] {
        let c = vec2(cx, cy);
        for k in 0..6 {
            let (a, b) = (QUARTER[k], QUARTER[k + 1]);
            draw_triangle(
                c,
                c + vec2(a.0 * sx, a.1 * sy) * r,
                c + vec2(b.0 * sx, b.1 * sy) * r,
                color,
            );
        }
    }
}
/// Opaque slab with a `border`-coloured rim of `t` units.
fn framed(rect: Rect, r: f32, t: f32, border: Color, fill: Color) {
    let Rect { x, y, w, h } = rect;
    slab(x, y, w, h, r, border);
    slab(x + t, y + t, w - 2. * t, h - 2. * t, (r - t).max(0.), fill);
}
/// Raised plate: two soft shadow layers, a rim and a lit top edge.
fn plate(x: f32, y: f32, w: f32, h: f32, fill: Color) {
    slab(
        x - 4.,
        y + 2.,
        w + 8.,
        h + 10.,
        14.,
        Color::new(0., 0., 0., 0.12),
    );
    slab(
        x - 1.,
        y + 2.,
        w + 2.,
        h + 4.,
        11.,
        Color::new(0., 0., 0., 0.22),
    );
    framed(Rect::new(x, y, w, h), 10., 1., lighten(fill, 0.07), fill);
    panel(x + 10., y + 1., w - 20., 1., Color::new(1., 1., 1., 0.05));
}
/// Background the play area's tags sit on, for opaque tint mixing.
const GROUND: Color = Color::new(0.045, 0.06, 0.085, 1.);
fn tint(color: Color, amount: f32) -> Color {
    let m = |a: f32, b: f32| a + (b - a) * amount;
    Color::new(
        m(GROUND.r, color.r),
        m(GROUND.g, color.g),
        m(GROUND.b, color.b),
        1.,
    )
}
/// Width of a [`tag`] at `size`.
fn tag_width(value: &str, size: f32) -> f32 {
    text_width(value, TextStyle::modern(size, PAPER)) + size * 1.3
}
/// Pill tag in the host chip's language; returns its width.
fn tag(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    let s = TextStyle::modern(size, color);
    let w = text_width(value, s) + size * 1.3;
    let h = size * 1.45;
    framed(
        Rect::new(x, y, w, h),
        h * 0.5,
        1.,
        tint(color, 0.38),
        tint(color, 0.14),
    );
    draw_text_styled(value, x + size * 0.65, s.centered_baseline(y + h * 0.5), s);
    w
}
/// Key cap with an action label, matching the host footer; returns its width.
fn key(cap: &str, action: &str, x: f32, y: f32, ink: Color) -> f32 {
    let s = TextStyle::modern(13., PAPER);
    let h = 20.;
    let w = (text_width(cap, s) + 12.).max(h);
    slab(x, y + 2., w, h, 4., Color::new(0., 0., 0., 0.5));
    framed(
        Rect::new(x, y, w, h),
        4.,
        1.,
        Color::new(0.24, 0.28, 0.34, 1.),
        Color::new(0.165, 0.2, 0.255, 1.),
    );
    draw_text_styled(
        cap,
        x + w * 0.5,
        s.centered_baseline(y + h * 0.5),
        s.with_align(TextAlign::Center),
    );
    if action.is_empty() {
        return w;
    }
    let a = TextStyle::modern(13., ink);
    w + 7. + draw_text_styled(action, x + w + 7., a.centered_baseline(y + h * 0.5), a).width
}
/// Wind arrow inside a small dial.
fn wind_arrow(cx: f32, cy: f32, r: f32, w: (i8, i8), color: Color) {
    let d = vec2(f32::from(w.0), f32::from(w.1));
    let side = vec2(-d.y, d.x);
    let tail = vec2(cx, cy) - d * r * 0.7;
    let tip = vec2(cx, cy) + d * r * 0.75;
    let neck = tip - d * r * 0.55;
    draw_line(tail.x, tail.y, neck.x, neck.y, 2.5, color);
    draw_triangle(tip, neck + side * r * 0.45, neck - side * r * 0.45, color);
}
/// Stable per-tile variation for procedural detail.
fn hash(at: usize) -> u32 {
    (at as u32).wrapping_mul(2_654_435_761).rotate_right(13) ^ 0x9e37
}
fn origin(at: usize) -> (f32, f32) {
    (
        MAP_X + (at % COLS) as f32 * TW,
        MAP_Y + (at / COLS) as f32 * TH,
    )
}
fn flammable(tile: Tile) -> bool {
    matches!(
        tile.terrain,
        Terrain::Grass | Terrain::Forest | Terrain::Brush
    )
}

fn terrain_base(tile: Tile) -> Color {
    match tile.terrain {
        Terrain::Water => Color::new(0.07, 0.25, 0.34, 1.),
        Terrain::Rock => Color::new(0.27, 0.27, 0.26, 1.),
        Terrain::Road => Color::new(0.24, 0.22, 0.2, 1.),
        Terrain::Base => Color::new(0.33, 0.33, 0.32, 1.),
        _ if tile.fuel == 0 && flammable(tile) => Color::new(0.16, 0.14, 0.13, 1.),
        Terrain::Forest => Color::new(0.12, 0.23, 0.16, 1.),
        Terrain::Brush => Color::new(0.21, 0.3, 0.18, 1.),
        _ => Color::new(0.24, 0.33, 0.2, 1.),
    }
}

fn pine(x: f32, y: f32, s: f32, dark: Color, light: Color) {
    // Ground shadow, trunk, then two tiers.
    draw_triangle(
        vec2(x - 9. * s, y + 3.),
        vec2(x + 13. * s, y + 3.),
        vec2(x + 6. * s, y - 4. * s),
        Color::new(0., 0., 0., 0.22),
    );
    panel(
        x - 1.5 * s,
        y - 3. * s,
        3. * s,
        6. * s,
        Color::new(0.38, 0.27, 0.18, 1.),
    );
    draw_triangle(
        vec2(x, y - 13. * s),
        vec2(x - 10. * s, y),
        vec2(x + 10. * s, y),
        dark,
    );
    draw_triangle(
        vec2(x, y - 21. * s),
        vec2(x - 7.5 * s, y - 8. * s),
        vec2(x + 7.5 * s, y - 8. * s),
        light,
    );
    draw_triangle(
        vec2(x, y - 21. * s),
        vec2(x - 7.5 * s, y - 8. * s),
        vec2(x, y - 8. * s),
        lighten(light, 0.12),
    );
}

fn draw_tile(game: &Game, at: usize) {
    let tile = game.tiles[at];
    let (px, py) = origin(at);
    let h = hash(at);
    panel(px, py, TW, TH, terrain_base(tile));
    let burned = tile.fuel == 0 && flammable(tile);
    match tile.terrain {
        _ if burned => {
            for k in 0..3u32 {
                let sx = px + 8. + ((h >> (k * 4)) % 30) as f32;
                let sy = py + 10. + ((h >> (k * 5 + 2)) % 26) as f32;
                panel(sx, sy, 3., 6., Color::new(0.07, 0.06, 0.06, 1.));
                panel(sx + 4., sy + 4., 2., 2., Color::new(0.4, 0.38, 0.36, 0.7));
            }
        }
        Terrain::Grass => {
            for k in 0..2u32 {
                let sx = px + 6. + ((h >> (k * 4)) % 36) as f32;
                let sy = py + 8. + ((h >> (k * 5 + 1)) % 30) as f32;
                let tuft = Color::new(0.31, 0.42, 0.24, 1.);
                panel(sx, sy, 2., 4., tuft);
                panel(sx + 3., sy + 1., 2., 3., tuft);
            }
        }
        Terrain::Brush => {
            let dark = Color::new(0.24, 0.45, 0.27, 1.);
            for (k, (bx, by, r)) in [(15., 28., 7.5), (31., 22., 9.), (34., 36., 6.)]
                .into_iter()
                .enumerate()
            {
                if k == 2 && h.is_multiple_of(2) {
                    continue;
                }
                dot(
                    px + bx + 1.5,
                    py + by + 2.5,
                    r,
                    Color::new(0., 0., 0., 0.25),
                );
                dot(px + bx, py + by, r, dark);
                dot(px + bx - r * 0.3, py + by - r * 0.35, r * 0.5, GREEN);
            }
        }
        Terrain::Forest => {
            let dark = Color::new(0.17, 0.4, 0.26, 1.);
            let light = Color::new(0.33, 0.62, 0.4, 1.);
            let shift = (h % 6) as f32;
            pine(px + 15. + shift, py + 25., 1., dark, light);
            pine(px + 34. - shift * 0.5, py + 40., 1.05, dark, light);
            if h.is_multiple_of(3) {
                pine(px + 37., py + 18., 0.75, dark, light);
            }
        }
        Terrain::Water => {
            panel(
                px,
                py + TH * 0.5,
                TW,
                TH * 0.5,
                Color::new(0.05, 0.2, 0.29, 1.),
            );
            let wave = Color::new(0.4, 0.72, 0.88, 0.45);
            let o = (h % 8) as f32;
            draw_line(px + 6. + o, py + 15., px + 20. + o, py + 15., 2., wave);
            draw_line(
                px + 22. - o * 0.5,
                py + 31.,
                px + 40. - o * 0.5,
                py + 31.,
                2.,
                wave,
            );
            // Foam where water meets land.
            let foam = Color::new(0.75, 0.9, 0.95, 0.5);
            let x = at % COLS;
            let y = at / COLS;
            let land = |n: usize| game.tiles[n].terrain != Terrain::Water;
            if y > 0 && land(at - COLS) {
                panel(px, py, TW, 2., foam);
            }
            if y + 1 < ROWS && land(at + COLS) {
                panel(px, py + TH - 2., TW, 2., foam);
            }
            if x > 0 && land(at - 1) {
                panel(px, py, 2., TH, foam);
            }
            if x + 1 < COLS && land(at + 1) {
                panel(px + TW - 2., py, 2., TH, foam);
            }
        }
        Terrain::Road => {
            let x = at % COLS;
            let y = at / COLS;
            let road = |n: usize| matches!(game.tiles[n].terrain, Terrain::Road | Terrain::Base);
            let horizontal = (x > 0 && road(at - 1)) || (x + 1 < COLS && road(at + 1));
            let vertical = (y > 0 && road(at - COLS)) || (y + 1 < ROWS && road(at + COLS));
            let paint = Color::new(0.85, 0.75, 0.45, 0.55);
            if horizontal {
                panel(px + 8., py + TH * 0.5 - 1., 12., 2., paint);
                panel(px + 30., py + TH * 0.5 - 1., 12., 2., paint);
            }
            if vertical && !horizontal {
                panel(px + TW * 0.5 - 1., py + 6., 2., 12., paint);
                panel(px + TW * 0.5 - 1., py + 28., 2., 12., paint);
            }
            let kerb = Color::new(0., 0., 0., 0.25);
            if !(y > 0 && road(at - COLS)) {
                panel(px, py, TW, 2., kerb);
            }
            if !(y + 1 < ROWS && road(at + COLS)) {
                panel(px, py + TH - 2., TW, 2., kerb);
            }
        }
        Terrain::Rock => {
            let face = Color::new(0.47, 0.46, 0.44, 1.);
            let shade = Color::new(0.33, 0.32, 0.31, 1.);
            fan(px + 18., py + 28., 12., 10., 0, 2, shade);
            fan(px + 17., py + 25., 10., 8.5, 0, 2, face);
            fan(px + 35., py + 33., 8., 7., 1, 2, shade);
            fan(px + 34., py + 31., 6.5, 5.5, 1, 2, lighten(face, 0.08));
        }
        Terrain::Town | Terrain::Base => draw_building(game, at, px, py),
    }
}

fn draw_building(game: &Game, at: usize, px: f32, py: f32) {
    let tile = game.tiles[at];
    let base = at == BASE;
    if base {
        // Concrete apron with a helipad ring.
        panel(
            px + 3.,
            py + 3.,
            TW - 6.,
            TH - 6.,
            Color::new(0.4, 0.4, 0.39, 1.),
        );
    }
    // Ground shadow.
    panel(px + 12., py + 38., 30., 4., Color::new(0., 0., 0., 0.3));
    let (wall, roof) = if tile.ruined {
        (
            Color::new(0.26, 0.23, 0.21, 1.),
            Color::new(0.2, 0.18, 0.17, 1.),
        )
    } else if base {
        (
            Color::new(0.9, 0.9, 0.86, 1.),
            Color::new(0.82, 0.24, 0.2, 1.),
        )
    } else {
        (
            Color::new(0.82, 0.76, 0.6, 1.),
            Color::new(0.78, 0.4, 0.22, 1.),
        )
    };
    panel(px + 12., py + 21., 28., 19., wall);
    draw_triangle(
        vec2(px + 8., py + 22.),
        vec2(px + 26., py + 7.),
        vec2(px + 44., py + 22.),
        roof,
    );
    draw_triangle(
        vec2(px + 26., py + 7.),
        vec2(px + 44., py + 22.),
        vec2(px + 26., py + 22.),
        darken(roof, 0.18),
    );
    if tile.ruined {
        panel(
            px + 16.,
            py + 26.,
            20.,
            2.,
            Color::new(0.08, 0.07, 0.07, 1.),
        );
        return;
    }
    if base {
        panel(
            px + 15.,
            py + 28.,
            22.,
            12.,
            Color::new(0.24, 0.27, 0.3, 1.),
        );
        panel(px + 15., py + 28., 22., 2., Color::new(0.82, 0.24, 0.2, 1.));
    } else {
        panel(px + 24., py + 30., 6., 10., Color::new(0.2, 0.15, 0.12, 1.));
        panel(px + 15., py + 27., 6., 5., Color::new(1., 0.84, 0.5, 1.));
        panel(px + 33., py + 27., 5., 5., Color::new(1., 0.84, 0.5, 1.));
    }
}

fn map(game: &Game) {
    // Header: current wind, the next shift, forecast state and the deadline.
    let wind = game.wind(false);
    let next = game.wind(true);
    dot(40., 136., 12., Color::new(0.1, 0.13, 0.16, 1.));
    wind_arrow(40., 136., 10., wind, AMBER);
    let lw = label("Wind", 60., 142., 13., MUTED);
    let w = label(wind_name(wind), 60. + lw + 7., 142., 17., PAPER);
    let nx = 60. + lw + 7. + w + 14.;
    let tw = label("then", nx, 142., 12., MUTED);
    wind_arrow(nx + tw + 12., 137., 7., next, SOFT);
    label(wind_name(next), nx + tw + 23., 142., 13., SOFT);
    let limit = MISSIONS[game.index].limit;
    let late = game.turn + 3 > limit;
    let value = format!("{} / {}", game.turn, limit);
    let vw = label_right(&value, 578., 142., 17., if late { RED } else { PAPER });
    let tw = label_right("Turn", 578. - vw - 8., 142., 13., MUTED);
    let (state, color) = if game.show_forecast {
        ("Forecast on", RED)
    } else {
        ("Forecast off", MUTED)
    };
    let cw = tag_width(state, 12.);
    tag(state, 578. - vw - tw - 24. - cw, 127., 12., color);

    // Plate under the map.
    plate(22., 156., 562., 380., Color::new(0.05, 0.065, 0.08, 1.));
    if game.phase != Phase::Active {
        // The briefing card covers the map.
        return;
    }
    for at in 0..CELLS {
        draw_tile(game, at);
    }
    // Hairline grid keeps targeting legible over painted terrain.
    let grid = Color::new(0., 0., 0., 0.22);
    for x in 1..COLS {
        panel(MAP_X + x as f32 * TW, MAP_Y, 1., TH * ROWS as f32, grid);
    }
    for y in 1..ROWS {
        panel(MAP_X, MAP_Y + y as f32 * TH, TW * COLS as f32, 1., grid);
    }
    // Weather and fire state on top of terrain.
    let forecast = game.forecast();
    for (at, (&tile, &warn)) in game.tiles.iter().zip(forecast.iter()).enumerate() {
        let (px, py) = origin(at);
        if tile.wet > 0 {
            panel(px, py, TW, TH, Color::new(0.3, 0.65, 0.95, 0.16));
            for (dx, dy) in [(7., 8.), (40., 12.), (10., 36.)] {
                draw_triangle(
                    vec2(px + dx, py + dy - 3.),
                    vec2(px + dx - 2., py + dy + 1.),
                    vec2(px + dx + 2., py + dy + 1.),
                    BLUE,
                );
                dot(px + dx, py + dy + 1.5, 2.2, BLUE);
            }
        }
        if game.show_forecast && warn {
            panel(
                px + 1.,
                py + 1.,
                TW - 2.,
                TH - 2.,
                Color::new(1., 0.3, 0.2, 0.14),
            );
            draw_rectangle_lines(px + 2., py + 2., TW - 4., TH - 4., 2., with_alpha(RED, 0.9));
            dot(px + TW - 9., py + 9., 6., Color::new(0.55, 0.1, 0.07, 1.));
            label_center("!", px + TW - 9., py + 13., 10., PAPER);
        }
        if tile.fire > 0 {
            draw_fire(px, py, tile.fire, hash(at) ^ u32::from(game.turn));
        }
    }
    // Crew tokens; several crews may share a tile.
    for (id, crew) in game.crews.iter().enumerate() {
        let (px, py) = origin(crew.tile);
        let x = px + 15. + id as f32 * 10.;
        let y = py + 31.;
        let selected = id == game.selected;
        dot(x + 1.5, y + 3., 10., Color::new(0., 0., 0., 0.4));
        if selected {
            dot(x, y, 13., PAPER);
        }
        dot(x, y, 10., Color::new(0.05, 0.07, 0.09, 1.));
        dot(x, y, 8.5, CREW[id]);
        dot(x - 2.5, y - 3., 3., with_alpha(lighten(CREW[id], 0.5), 0.8));
        label_center(&(id + 1).to_string(), x, y + 4., 10., INK);
        if crew.cargo > 0 {
            dot(x + 7., y + 8., 5., PAPER);
            label_center(&crew.cargo.to_string(), x + 7., y + 11.5, 8., INK);
        }
    }
    // Resident counts and the base label sit above tokens.
    for at in 0..CELLS {
        let tile = game.tiles[at];
        let (px, py) = origin(at);
        if at == BASE {
            pill(px + 3., py + 3., 28., 12., Color::new(0.82, 0.24, 0.2, 1.));
            label_center("Base", px + 17., py + 13., 9., PAPER);
        } else if tile.terrain == Terrain::Town && tile.people > 0 {
            let text = tile.people.to_string();
            let w = text_width(&text, TextStyle::modern(11., INK)) + 20.;
            pill(px + 3., py + 3., w, 15., Color::new(0.95, 0.93, 0.86, 1.));
            dot(px + 10., py + 7.5, 2.2, Color::new(0.75, 0.3, 0.15, 1.));
            panel(px + 7., py + 10., 6., 5., Color::new(0.75, 0.3, 0.15, 1.));
            label(&text, px + 16., py + 15., 11., INK);
        }
    }
    // Targeting cursor.
    let (cx, cy) = origin(game.cursor);
    panel(cx, cy, TW, TH, Color::new(1., 0.85, 0.4, 0.1));
    brackets(
        cx - 1.,
        cy - 1.,
        TW + 2.,
        TH + 2.,
        11.,
        2.5,
        Color::new(1., 0.82, 0.36, 1.),
    );
    // Tile readout and the latest event.
    let tile = game.tiles[game.cursor];
    let name = if game.cursor == BASE {
        "Base · safe refuge".to_owned()
    } else if let Some(v) = MISSIONS[game.index]
        .villages
        .iter()
        .find(|v| v.y * COLS + v.x == game.cursor)
    {
        if tile.ruined {
            format!("{} · ruined", v.name)
        } else {
            format!("{} · {} waiting", v.name, tile.people)
        }
    } else {
        match tile.terrain {
            _ if tile.fuel == 0 && flammable(tile) => "Burned ground",
            Terrain::Grass => "Grassland",
            Terrain::Forest => "Forest · 2 AP to cross",
            Terrain::Brush => "Brush",
            Terrain::Road => "Road · firebreak",
            Terrain::Water => "Water · refill",
            Terrain::Rock => "Rock",
            _ => "Field",
        }
        .to_owned()
    };
    let nw = label(&name, 28., 552., 14., PAPER);
    let mut x = 28. + nw + 18.;
    for (text, value, color) in [("Fuel", tile.fuel, GREEN), ("Heat", tile.fire, RED)] {
        x += label(text, x, 552., 12., MUTED) + 5.;
        x += label(
            &value.to_string(),
            x,
            552.,
            14.,
            if value > 0 { color } else { SOFT },
        ) + 14.;
    }
    label_fit(&game.note, 28., 567., 550., 12., with_alpha(AMBER, 0.9));
}

/// Layered flame tongues; hotter tiles burn taller.
fn draw_fire(px: f32, py: f32, heat: u8, h: u32) {
    let k = 0.7 + 0.15 * f32::from(heat.min(3));
    panel(
        px,
        py,
        TW,
        TH,
        Color::new(1., 0.35, 0.1, 0.22 + 0.06 * f32::from(heat)),
    );
    let base = py + TH - 5.;
    let tongues = [(13., 10., 26.), (26., 13., 34.), (38., 9., 24.)];
    for (i, (x, half, height)) in tongues.into_iter().enumerate() {
        let lean = ((h >> (i * 3)) % 5) as f32 - 2.;
        let x = px + x;
        let top = base - height * k;
        draw_triangle(
            vec2(x - half, base),
            vec2(x + half, base),
            vec2(x + lean, top),
            RED,
        );
        draw_triangle(
            vec2(x - half * 0.65, base),
            vec2(x + half * 0.65, base),
            vec2(x + lean * 0.6, top + height * k * 0.3),
            AMBER,
        );
        draw_triangle(
            vec2(x - half * 0.3, base),
            vec2(x + half * 0.3, base),
            vec2(x + lean * 0.3, top + height * k * 0.6),
            Color::new(1., 0.93, 0.7, 1.),
        );
    }
    panel(px + 3., base, TW - 6., 2., Color::new(1., 0.8, 0.4, 0.8));
    // Heat badge.
    dot(px + 9., py + 9., 7., Color::new(0.25, 0.05, 0.03, 0.95));
    label_center(
        &heat.to_string(),
        px + 9.,
        py + 13.,
        10.,
        Color::new(1., 0.86, 0.6, 1.),
    );
}

fn sidebar(game: &Game) {
    for (id, crew) in game.crews.iter().enumerate() {
        let y = 122. + id as f32 * 66.;
        let color = CREW[id];
        let selected = id == game.selected;
        let fill = if selected {
            Color::new(0.14, 0.18, 0.2, 1.)
        } else {
            Color::new(0.08, 0.105, 0.13, 1.)
        };
        if selected {
            framed(
                Rect::new(SIDE_X, y, SIDE_W, 60.),
                9.,
                1.5,
                darken(color, 0.25),
                fill,
            );
        } else {
            slab(SIDE_X, y, SIDE_W, 60., 9., fill);
        }
        panel(SIDE_X + 8., y + 12., 3., 36., color);
        label(&(id + 1).to_string(), SIDE_X + 20., y + 23., 12., MUTED);
        let nw = label(NAMES[id], SIDE_X + 34., y + 23., 16., color);
        let role = ["Range 2 hose", "Cuts breaks fast", "Carries 6"][id];
        label(role, SIDE_X + 34. + nw + 8., y + 23., 11., MUTED);
        // Action points as cells, right aligned.
        let max = game.max_ap();
        for k in 0..max {
            let x = SIDE_X + SIDE_W - 14. - f32::from(max - k) * 11. + 2.;
            let on = k < crew.ap;
            panel(
                x,
                y + 13.,
                8.,
                10.,
                if on {
                    color
                } else {
                    Color::new(1., 1., 1., 0.1)
                },
            );
            if on {
                panel(x, y + 13., 8., 1., Color::new(1., 1., 1., 0.35));
            }
        }
        label_right(
            "AP",
            SIDE_X + SIDE_W - 18. - f32::from(max) * 11.,
            y + 22.,
            11.,
            MUTED,
        );
        // Water tank and passengers.
        let water_max = game.water_max(id);
        let frac = f32::from(crew.water) / f32::from(water_max.max(1));
        draw_triangle(
            vec2(SIDE_X + 38., y + 34.),
            vec2(SIDE_X + 34.5, y + 40.),
            vec2(SIDE_X + 41.5, y + 40.),
            BLUE,
        );
        dot(SIDE_X + 38., y + 41., 3.6, BLUE);
        bar(
            SIDE_X + 50.,
            y + 37.,
            120.,
            6.,
            frac,
            if crew.water == 0 { RED } else { BLUE },
        );
        label(
            &format!("{}/{}", crew.water, water_max),
            SIDE_X + 178.,
            y + 44.,
            12.,
            SOFT,
        );
        let cap = Game::capacity(id);
        let people = format!("{} / {} aboard", crew.cargo, cap);
        label_right(
            &people,
            SIDE_X + SIDE_W - 14.,
            y + 44.,
            12.,
            if crew.cargo > 0 { PAPER } else { MUTED },
        );
    }
    // Incident status.
    let quota = game.quota();
    let cols = [SIDE_X, SIDE_X + 120., SIDE_X + 222.];
    eyebrow("HOME", cols[0], 336., MUTED);
    eyebrow("FIRES", cols[1], 336., MUTED);
    eyebrow("CREDITS", cols[2], 336., MUTED);
    let hw = label(&game.landed.to_string(), cols[0], 362., 22., BLUE);
    label(&format!("/ {quota}"), cols[0] + hw + 5., 362., 14., MUTED);
    let fires = game.fire_count();
    label(
        &fires.to_string(),
        cols[1],
        362.,
        22.,
        if fires > 0 { RED } else { GREEN },
    );
    label(&game.cash.to_string(), cols[2], 362., 22., AMBER);
    bar(
        SIDE_X,
        376.,
        SIDE_W,
        6.,
        f32::from(game.landed) / f32::from(quota.max(1)),
        BLUE,
    );
    label(
        &format!(
            "{} residents still in danger",
            game.quota().saturating_sub(game.landed)
        ),
        SIDE_X,
        399.,
        11.,
        MUTED,
    );
    // Primary order.
    framed(
        Rect::new(SIDE_X, 409., SIDE_W, 42.),
        10.,
        1.,
        Color::new(0.25, 0.3, 0.3, 1.),
        Color::new(0.17, 0.215, 0.215, 1.),
    );
    panel(
        SIDE_X + 10.,
        410.,
        SIDE_W - 20.,
        1.,
        Color::new(1., 1., 1., 0.07),
    );
    key(
        "Space",
        &sentence(game.action_name()),
        SIDE_X + 12.,
        420.,
        PAPER,
    );
    label_right("or click map", SIDE_X + SIDE_W - 14., 435., 12., MUTED);
    // Secondary orders.
    key("X", "Cut firebreak", SIDE_X + 4., 462., GREEN);
    label_right(
        "Ranger 1 AP · others 2",
        SIDE_X + SIDE_W - 14.,
        476.,
        11.,
        MUTED,
    );
    key("4", "Air drop at cursor", SIDE_X + 4., 487., BLUE);
    let drops = match game.drops {
        1 => "1 sortie · 12 cr".to_owned(),
        n => format!("{n} sorties · 12 cr"),
    };
    label_right(&drops, SIDE_X + SIDE_W - 14., 501., 11., MUTED);
    // End turn.
    framed(
        Rect::new(SIDE_X, 518., SIDE_W, 42.),
        10.,
        1.,
        Color::new(0.38, 0.23, 0.12, 1.),
        Color::new(0.26, 0.155, 0.085, 1.),
    );
    panel(
        SIDE_X + 10.,
        519.,
        SIDE_W - 20.,
        1.,
        Color::new(1., 0.8, 0.6, 0.08),
    );
    key("C", "End turn", SIDE_X + 12., 529., AMBER);
    label_right(
        "Fire spreads",
        SIDE_X + SIDE_W - 14.,
        544.,
        12.,
        with_alpha(AMBER, 0.75),
    );
}

/// Progress bar on an opaque track (translucent ends would double-blend).
fn bar(x: f32, y: f32, w: f32, h: f32, frac: f32, color: Color) {
    pill(x, y, w, h, Color::new(0.03, 0.04, 0.05, 1.));
    let f = if frac.is_finite() {
        frac.clamp(0., 1.)
    } else {
        0.
    };
    if f > 0. {
        pill(x, y, (w * f).max(h), h, color);
    }
}
/// Full-width call to action inside the model's click band (y 470..520).
fn action_button(label_text: &str, hint: &str, color: Color) {
    let (x, y, w, h) = (54., 474., 499., 40.);
    rounded_gradient(x, y, w, h, 10., lighten(color, 0.08), darken(color, 0.14));
    let s = TextStyle::modern(17., INK).with_align(TextAlign::Center);
    draw_text_styled(label_text, x + w * 0.5, s.centered_baseline(y + h * 0.5), s);
    label_right(hint, x + w - 14., y + 25., 12., with_alpha(INK, 0.7));
}

fn overlay(game: &Game) {
    if game.phase == Phase::Active {
        return;
    }
    card(39., 168., 529., 360., Color::new(0.07, 0.09, 0.11, 1.));
    let x = 62.;
    let w = 483.;
    let mission = &MISSIONS[game.index];
    let stripe = |c: Color| rounded(62., 168., 64., 3., 1.5, c);
    match game.phase {
        Phase::Briefing => {
            stripe(AMBER);
            eyebrow(
                &format!("INCIDENT {} OF 6  ·  BRIEFING", game.mission()),
                x,
                200.,
                AMBER,
            );
            label_fit(mission.name, x, 234., w, 28., PAPER);
            let y = paragraph(mission.story, x, 262., w, 15., 2, SOFT);
            let mut cx = x;
            for (tag, color) in [
                (format!("{} residents", game.quota()), BLUE),
                (format!("{} turns", mission.limit), AMBER),
                (format!("{} fires", mission.fires.len()), RED),
                (format!("Wind {}", wind_name(game.wind(false))), SOFT),
            ] {
                cx += chip(&tag, cx, y, 12., color) + 8.;
            }
            let y = y + 32.;
            let lines = wrap_text(mission.tip, w - 32., TextStyle::modern(14., SOFT))
                .take(2)
                .count() as f32;
            let h = 44. + lines * 19.6;
            rounded(x, y, w, h, 8., Color::new(1., 1., 1., 0.04));
            rounded(x, y + 12., 3., h - 24., 1.5, AMBER);
            eyebrow("FIELD TIP", x + 16., y + 24., AMBER);
            paragraph(mission.tip, x + 16., y + 46., w - 32., 14., 2, SOFT);
            let mut kx = x;
            for (key, action) in [
                ("1-3", "Crew"),
                ("Space", "Act"),
                ("X", "Firebreak"),
                ("C", "End turn"),
            ] {
                kx += keycap(key, action, kx, 436., SOFT) + 16.;
            }
            action_button("Deploy crews", "Space or click", AMBER);
        }
        Phase::Debrief => {
            stripe(GREEN);
            eyebrow("DISTRICT SECURED", x, 200., GREEN);
            label_fit(&format!("{} is safe", mission.name), x, 234., w, 26., PAPER);
            let medals = game.medals[game.index];
            let mut sx = x;
            for k in 0..3 {
                let lit = k < medals;
                sx += label(
                    "★",
                    sx,
                    268.,
                    20.,
                    if lit {
                        AMBER
                    } else {
                        Color::new(1., 1., 1., 0.15)
                    },
                ) + 6.;
            }
            label(
                &format!("{} credits banked", game.cash),
                sx + 10.,
                265.,
                14.,
                SOFT,
            );
            label(
                "Buy equipment, or save credits for air drops.",
                x,
                300.,
                13.,
                MUTED,
            );
            for (id, (name, detail)) in [
                ("Tanks", "+2 water"),
                ("Training", "+1 AP"),
                ("Air base", "+1 drop"),
            ]
            .into_iter()
            .enumerate()
            {
                let cx = 54. + id as f32 * 166.;
                let level = game.upgrades[id];
                let cost = game.upgrade_cost(id);
                let open = level < 2;
                let afford = open && game.cash >= cost;
                let color = CREW[id];
                rounded(cx, 328., 155., 113., 10., Color::new(0.11, 0.15, 0.17, 1.));
                outline(
                    cx,
                    328.,
                    155.,
                    113.,
                    10.,
                    1.,
                    with_alpha(color, if afford { 0.6 } else { 0.18 }),
                );
                keycap(&(id + 1).to_string(), "", cx + 12., 340., PAPER);
                label(name, cx + 42., 355., 15., if open { color } else { MUTED });
                label(
                    detail,
                    cx + 14.,
                    392.,
                    20.,
                    if open { PAPER } else { MUTED },
                );
                for r in 0..2u8 {
                    rounded(
                        cx + 14. + f32::from(r) * 22.,
                        410.,
                        18.,
                        6.,
                        3.,
                        if r < level {
                            color
                        } else {
                            Color::new(1., 1., 1., 0.12)
                        },
                    );
                }
                let (price, pc) = if !open {
                    ("Maxed".to_owned(), MUTED)
                } else {
                    (format!("{cost} cr"), if afford { AMBER } else { RED })
                };
                label_right(&price, cx + 141., 431., 12., pc);
            }
            action_button("Next incident", "Space", GREEN);
        }
        Phase::Setback | Phase::Ended => {
            let won = game.outcome() == Outcome::Won;
            let (eye, title, color) = if won {
                ("CAREER COMPLETE", "Dawn has come", AMBER)
            } else if game.phase == Phase::Ended {
                ("CAREER OVER", "The watch is over", RED)
            } else {
                ("INCIDENT SETBACK", "Operation setback", RED)
            };
            stripe(color);
            eyebrow(eye, x, 200., color);
            label_fit(title, x, 236., w, 28., PAPER);
            let y = paragraph(&game.note, x, 272., w, 16., 4, SOFT);
            if game.phase == Phase::Setback {
                paragraph(
                    "A retry restores this incident's map, residents and credits, so rewards cannot be farmed.",
                    x,
                    y + 8.,
                    w,
                    13.,
                    3,
                    MUTED,
                );
                chip(
                    &match game.retries {
                        1 => "1 retry remains".to_owned(),
                        n => format!("{n} retries remain"),
                    },
                    x,
                    436.,
                    13.,
                    BLUE,
                );
                action_button("Retry incident", "Space or click", BLUE);
            } else {
                keycap("R", "Start a new career", x, 490., MUTED);
            }
        }
        Phase::Active => {}
    }
}
impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        map(self);
        sidebar(self);
        overlay(self);
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!("Incident {}/6", self.mission()),
            objective: format!(
                "{}: evacuate all residents and contain the fire.",
                MISSIONS[self.index].name
            ),
            status: format!(
                "Turn {}/{} | {} rescued | {} retries",
                self.turn,
                MISSIONS[self.index].limit,
                self.rescued(),
                self.retries
            ),
            score: self.score(),
        }
    }
    fn outcome(&self) -> Outcome {
        self.outcome()
    }
    fn summary(&self) -> String {
        self.summary()
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
}
