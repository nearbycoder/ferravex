//! Presentation for Wildfire Watch: a living district map that previews every
//! order, icon-first crew and command cards, and effects where each accepted
//! event happens.
//!
//! Everything here reads the model. Clickable surfaces are drawn inside the
//! rectangles of [`crate::layout`], which the model's pointer mapping shares.
use crate::content::{BASE, CELLS, COLS, MISSIONS, ROWS};
use crate::layout::{
    ACT, CARD_H, CARD_STEP, CARD_Y, CUT, DEPLOY, DROP, END, FORECAST, MAP_X, MAP_Y, PROCEED, RETRY,
    SIDE_W, SIDE_X, TH, TW, WALLET_Y, card_at, center, hit, origin, proceed_at, shop_at, shop_card,
    tile_at,
};
use crate::model::{Action, DROP_COST, Event, Game, Lack, Phase, Setback, Spot, Terrain, Tile};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;
use std::f32::consts::FRAC_1_SQRT_2;

const AMBER: Color = Color::new(1., 0.64, 0.32, 1.);
const GREEN: Color = Color::new(0.38, 0.73, 0.48, 1.);
const BLUE: Color = Color::new(0.35, 0.72, 0.92, 1.);
const RED: Color = Color::new(1., 0.34, 0.25, 1.);
const SOFT: Color = Color::new(0.8, 0.85, 0.88, 1.);
/// Action points.
const GOLD: Color = Color::new(1., 0.84, 0.38, 1.);
const DIRT: Color = Color::new(0.66, 0.48, 0.3, 1.);
const STEAM: Color = Color::new(0.88, 0.94, 0.98, 1.);
const SKY: Color = Color::new(0.6, 0.8, 1., 1.);
const CREW: [Color; 3] = [AMBER, GREEN, BLUE];
const NAMES: [&str; 3] = ["Engine", "Ranger", "Rescue"];
/// Engine sprays, Ranger cuts breaks, Rescue carries residents.
const ROLE: [Icon; 3] = [Icon::Drop, Icon::Pickaxe, Icon::People];
/// Small signed numbers for chips and popups without per-frame formatting.
const PLUS: [&str; 13] = [
    "+0", "+1", "+2", "+3", "+4", "+5", "+6", "+7", "+8", "+9", "+10", "+11", "+12",
];
const MINUS: [&str; 4] = ["0", "-1", "-2", "-3"];

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
fn plus(n: u8) -> &'static str {
    PLUS[usize::from(n).min(PLUS.len() - 1)]
}
/// `n` without its sign, for small counts (0..=12).
fn num(n: u8) -> &'static str {
    &plus(n)[1..]
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
/// A water droplet: a point over a disc.
fn droplet(x: f32, y: f32, r: f32, color: Color) {
    draw_triangle(
        vec2(x, y - r * 1.9),
        vec2(x - r * 0.9, y - r * 0.2),
        vec2(x + r * 0.9, y - r * 0.2),
        color,
    );
    dot(x, y, r, color);
}
/// A small red cross over a refused resource.
fn cross(x: f32, y: f32, r: f32, color: Color) {
    draw_line(x - r, y - r, x + r, y + r, 2.5, color);
    draw_line(x - r, y + r, x + r, y - r, 2.5, color);
}
/// Row of `total` pips, the first `lit` filled; returns its width.
fn pip_row(x: f32, cy: f32, lit: u8, total: u8, r: f32, color: Color) -> f32 {
    let gap = r * 2. + 3.;
    for k in 0..total {
        let at = vec2(x + r + f32::from(k) * gap, cy);
        if k < lit {
            shapes::fill_circle(at, r, color);
        } else {
            shapes::stroke_circle(at, r - 0.6, 1.2, with_alpha(color, 0.38));
        }
    }
    pip_width(total, r)
}
/// Width of a [`pip_row`].
fn pip_width(total: u8, r: f32) -> f32 {
    if total == 0 {
        0.
    } else {
        f32::from(total - 1) * (r * 2. + 3.) + r * 2.
    }
}
/// Segmented gauge: one cell per unit, the first `lit` filled.
fn cells(x: f32, y: f32, w: f32, h: f32, lit: u8, total: u8, color: Color) {
    if total == 0 {
        return;
    }
    let gap = 2.;
    let n = f32::from(total);
    let cell = ((w - gap * (n - 1.)) / n).max(1.);
    for k in 0..total {
        let cx = x + f32::from(k) * (cell + gap);
        if k < lit {
            panel(cx, y, cell, h, color);
            panel(cx, y, cell, 1., Color::new(1., 1., 1., 0.3));
        } else {
            panel(cx, y, cell, h, Color::new(1., 1., 1., 0.08));
        }
    }
}

// --- Trig-free surfaces for every-frame interface. ---

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
/// Background the play area's surfaces sit on, for opaque tint mixing.
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
/// Key cap matching the host footer; returns its width.
fn key(cap: &str, x: f32, y: f32) -> f32 {
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
    w
}
/// Wind arrow in a small dial; `flow` animates streaks along the wind.
fn wind_dial(cx: f32, cy: f32, r: f32, w: (i8, i8), color: Color, t: f32, flow: bool) {
    let d = vec2(f32::from(w.0), f32::from(w.1));
    let side = vec2(-d.y, d.x);
    let c = vec2(cx, cy);
    dot(cx, cy, r, Color::new(0.09, 0.115, 0.15, 1.));
    shapes::stroke_circle(c, r, 1., with_alpha(color, 0.3 * color.a));
    if flow {
        for k in 0..3 {
            let phase = (t * 0.8 + k as f32 / 3.).fract();
            let lane = (k as f32 - 1.) * r * 0.5;
            let head = c + side * lane + d * (phase * 2. - 1.) * r * 0.85;
            let tail = head - d * r * 0.3;
            let a = (1. - (phase * 2. - 1.).abs()) * 0.55 * color.a;
            draw_line(tail.x, tail.y, head.x, head.y, 1.2, with_alpha(color, a));
        }
    }
    let nudge = if flow { (t * 3.).sin() * r * 0.07 } else { 0. };
    let tip = c + d * (r * 0.72 + nudge);
    let tail = c - d * (r * 0.62 - nudge);
    let neck = tip - d * r * 0.5;
    draw_line(tail.x, tail.y, neck.x, neck.y, (r * 0.17).max(2.), color);
    draw_triangle(tip, neck + side * r * 0.42, neck - side * r * 0.42, color);
}
/// Stable per-tile variation for procedural detail.
fn hash(at: usize) -> u32 {
    (at as u32).wrapping_mul(2_654_435_761).rotate_right(13) ^ 0x9e37
}
fn flammable(tile: Tile) -> bool {
    matches!(
        tile.terrain,
        Terrain::Grass | Terrain::Forest | Terrain::Brush
    )
}

// --- Frame state: what the pointer is over, decided once per frame. ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hover {
    None,
    Tile(usize),
    Card(usize),
    Act,
    Cut,
    Drop,
    End,
    Forecast,
    Proceed,
    Shop(usize),
}
struct Frame {
    /// Presentation seconds for idle motion; frozen under reduced motion.
    t: f32,
    /// Hold red alarms steady (reduced motion or reduced flashing).
    calm: bool,
    hover: Hover,
    /// Hovered map tile, else the keyboard cursor.
    focus: usize,
    live: bool,
    forecast: [bool; CELLS],
}
impl Frame {
    fn new(game: &Game) -> Self {
        let p = pointer();
        let live = game.phase == Phase::Active;
        let hover = match game.phase {
            Phase::Active => {
                if let Some(at) = tile_at(p) {
                    Hover::Tile(at)
                } else if let Some(id) = card_at(p) {
                    Hover::Card(id)
                } else if hit(ACT, p) {
                    Hover::Act
                } else if hit(CUT, p) {
                    Hover::Cut
                } else if hit(DROP, p) {
                    Hover::Drop
                } else if hit(END, p) {
                    Hover::End
                } else if hit(FORECAST, p) {
                    Hover::Forecast
                } else {
                    Hover::None
                }
            }
            Phase::Debrief => match shop_at(p) {
                Some(id) => Hover::Shop(id),
                None if proceed_at(p) => Hover::Proceed,
                None => Hover::None,
            },
            Phase::Briefing if hit(DEPLOY, p) => Hover::Proceed,
            Phase::Setback if hit(RETRY, p) => Hover::Proceed,
            _ => Hover::None,
        };
        let focus = match hover {
            Hover::Tile(at) => at,
            _ => game.cursor,
        };
        Self {
            t: idle_clock(),
            calm: {
                let policy = feedback_policy();
                policy.reduced_motion() || policy.reduced_flash()
            },
            hover,
            focus,
            live,
            forecast: game.forecast(),
        }
    }
}

impl Frame {
    /// A red alarm's beat (0..1): pulsing, or steady when the player asked
    /// for calm feedback.
    fn alarm(&self, hz: f32) -> f32 {
        if self.calm { 0.6 } else { pulse(self.t, hz) }
    }
}
/// The host clock for idle motion, held still for players who reduce motion
/// (every state stays visible; only the movement stops).
fn idle_clock() -> f32 {
    if feedback_policy().reduced_motion() {
        0.
    } else {
        clock()
    }
}

// --- The painted district. ---

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

fn draw_tile(game: &Game, at: usize, t: f32) {
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
            // Ripples drift along the river; each tile keeps its own phase.
            let wave = Color::new(0.4, 0.72, 0.88, 0.45);
            let phase = (h % 8) as f32 + t * 1.3 + (at % COLS) as f32 * 0.8;
            let o = 4. + phase.sin() * 4.;
            let o2 = 4. + (phase * 0.8 + 2.).sin() * 4.;
            draw_line(px + 4. + o, py + 15., px + 18. + o, py + 15., 2., wave);
            draw_line(px + 22. - o2, py + 31., px + 40. - o2, py + 31., 2., wave);
            let glint = 0.5 + 0.5 * (t * 2.1 + h as f32).sin();
            panel(
                px + 30. + o * 0.5,
                py + 21.,
                4.,
                1.5,
                Color::new(0.85, 0.95, 1., 0.35 * glint),
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
        Terrain::Town | Terrain::Base => draw_building(game, at, px, py, t),
    }
}

fn draw_building(game: &Game, at: usize, px: f32, py: f32, t: f32) {
    let tile = game.tiles[at];
    let base = at == BASE;
    if base {
        // Concrete apron.
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
        // A thin wisp of smoke rises from the ruin.
        for k in 0..3 {
            let life = (t * 0.35 + k as f32 / 3.).fract();
            let r = 2.5 + life * 4.;
            dot(
                px + 30. + (t * 1.7 + k as f32).sin() * 3. + life * 6.,
                py + 12. - life * 16.,
                r,
                Color::new(0.5, 0.5, 0.52, 0.32 * (1. - life)),
            );
        }
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
        // Lit windows flicker softly while anyone is still inside.
        let lit = if tile.people > 0 {
            0.85 + 0.15 * (t * 2.3 + at as f32).sin()
        } else {
            0.35
        };
        let lamp = Color::new(1., 0.84, 0.5, lit);
        panel(px + 24., py + 30., 6., 10., Color::new(0.2, 0.15, 0.12, 1.));
        panel(px + 15., py + 27., 6., 5., lamp);
        panel(px + 33., py + 27., 5., 5., lamp);
    }
}

/// Layered flame tongues that flicker and lean with the wind; hotter tiles
/// burn taller. Embers drift downwind.
fn draw_fire(px: f32, py: f32, heat: u8, seed: u32, t: f32, wind: Vec2) {
    let k = 0.7 + 0.15 * f32::from(heat.min(3));
    let phase = (seed % 628) as f32 * 0.01;
    glow(
        px + TW * 0.5,
        py + TH * 0.6,
        26. + 5. * f32::from(heat),
        Color::new(1., 0.42, 0.12, 0.17 + 0.05 * (t * 5.3 + phase).sin()),
    );
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
        let p = phase + i as f32 * 1.7;
        let flick = 1. + 0.12 * (t * 7.3 + p).sin() + 0.07 * (t * 12.1 + p * 1.9).sin();
        let lean = ((seed >> (i * 3)) % 5) as f32 - 2. + wind.x * 3. + (t * 4.1 + p).sin() * 1.6;
        let x = px + x;
        let h = height * k * flick;
        let top = base - h;
        draw_triangle(
            vec2(x - half, base),
            vec2(x + half, base),
            vec2(x + lean, top),
            RED,
        );
        draw_triangle(
            vec2(x - half * 0.65, base),
            vec2(x + half * 0.65, base),
            vec2(x + lean * 0.6, top + h * 0.3),
            AMBER,
        );
        draw_triangle(
            vec2(x - half * 0.3, base),
            vec2(x + half * 0.3, base),
            vec2(x + lean * 0.3, top + h * 0.6),
            Color::new(1., 0.93, 0.7, 1.),
        );
    }
    panel(px + 3., base, TW - 6., 2., Color::new(1., 0.8, 0.4, 0.8));
    for i in 0..3 {
        let life = (t * 0.55 + i as f32 * 0.33 + phase * 0.1).fract();
        let drift = wind * life * 20. + vec2((t * 3. + i as f32 * 2.).sin() * 2., -life * 26.);
        let at = vec2(px + 14. + i as f32 * 11., base - 16.) + drift;
        dot(
            at.x,
            at.y,
            1.7 * (1. - life * 0.5),
            Color::new(1., 0.78, 0.38, 1. - life),
        );
    }
    heat_pips(px, py, heat, 0);
}
/// Heat as three pips in the tile corner; `cooled` trailing pips show what a
/// spray would remove.
fn heat_pips(px: f32, py: f32, heat: u8, cooled: u8) {
    pill(px + 3., py + 3., 27., 10., Color::new(0.2, 0.04, 0.02, 0.9));
    for k in 0..3u8 {
        let x = px + 9. + f32::from(k) * 7.5;
        let at = vec2(x, py + 8.);
        if k < heat.saturating_sub(cooled) {
            shapes::fill_circle(at, 2.6, Color::new(1., 0.86, 0.5, 1.));
        } else if k < heat {
            shapes::fill_circle(at, 2.6, Color::new(0.3, 0.62, 1., 1.));
        } else {
            shapes::fill_circle(at, 1.6, Color::new(1., 1., 1., 0.18));
        }
    }
}
/// A translucent flame where the forecast says fire arrives next.
fn ghost_flame(px: f32, py: f32, seed: u32, t: f32, boost: f32) {
    let phase = (seed % 628) as f32 * 0.01;
    let beat = pulse(t + phase * 0.2, 0.9);
    let a = ((0.3 + 0.2 * beat) * boost).min(0.9);
    panel(
        px + 1.,
        py + 1.,
        TW - 2.,
        TH - 2.,
        Color::new(1., 0.3, 0.2, 0.1 * boost),
    );
    glow(
        px + TW * 0.5,
        py + TH * 0.65,
        22.,
        Color::new(1., 0.4, 0.15, 0.12 * boost * (0.6 + 0.4 * beat)),
    );
    // Outlined "hologram" tongues read as fire-to-come over any terrain.
    let base = py + TH - 6.;
    let edge = Color::new(1., 0.55, 0.25, (a * 1.6).min(0.95));
    for (i, (x, half, height)) in [(17., 8., 22.), (33., 9., 27.)].into_iter().enumerate() {
        let flick = 1. + 0.12 * (t * 5. + phase + i as f32).sin();
        let lean = (t * 3.3 + phase + i as f32 * 2.).sin() * 1.5;
        let top = vec2(px + x + lean, base - height * flick);
        let (l, r) = (vec2(px + x - half, base), vec2(px + x + half, base));
        draw_triangle(l, r, top, Color::new(1., 0.5, 0.2, a * 0.45));
        draw_triangle(
            vec2(px + x - half * 0.45, base),
            vec2(px + x + half * 0.45, base),
            vec2(px + x + lean * 0.5, top.y + height * 0.45),
            Color::new(1., 0.9, 0.6, a * 0.8),
        );
        draw_line(l.x, l.y, top.x, top.y, 1.6, edge);
        draw_line(r.x, r.y, top.x, top.y, 1.6, edge);
    }
    draw_rectangle_lines(
        px + 2.,
        py + 2.,
        TW - 4.,
        TH - 4.,
        2.,
        with_alpha(RED, ((0.45 + 0.4 * beat) * boost).min(1.)),
    );
}
/// Wet ground: a blue sheen and one droplet per remaining protected spread.
fn wet_tile(px: f32, py: f32, wet: u8, seed: u32, t: f32) {
    panel(px, py, TW, TH, Color::new(0.3, 0.65, 0.95, 0.16));
    let spots = [(8., 12.), (42., 15.), (10., 39.)];
    for (k, &(dx, dy)) in spots.iter().take(usize::from(wet.min(3))).enumerate() {
        let b = bob(t + k as f32 * 0.4 + (seed % 7) as f32 * 0.3, 0.6, 1.2);
        droplet(px + dx, py + dy + b, 2.3, BLUE);
    }
}

fn token_at(game: &Game, id: usize) -> Vec2 {
    let (px, py) = origin(game.crews[id].tile);
    vec2(px + 15. + id as f32 * 10., py + 31.)
}
fn token(game: &Game, id: usize, f: &Frame) {
    let crew = game.crews[id];
    let selected = id == game.selected && f.live;
    let mut at = token_at(game, id);
    if selected {
        at.y += bob(f.t, 1.1, 1.6) - 1.;
    }
    let spent = f.live && crew.ap == 0;
    let color = if spent {
        shapes::mix(CREW[id], Color::new(0.4, 0.42, 0.45, 1.), 0.6)
    } else {
        CREW[id]
    };
    dot(at.x + 1.5, at.y + 3., 10., Color::new(0., 0., 0., 0.4));
    if selected {
        glow(at.x, at.y, 22., with_alpha(CREW[id], 0.3));
        shapes::stroke_circle(at, 12.5 + pulse(f.t, 1.1) * 1.5, 2., with_alpha(PAPER, 0.9));
    }
    dot(at.x, at.y, 10., Color::new(0.05, 0.07, 0.09, 1.));
    dot(at.x, at.y, 8.5, color);
    icon(ROLE[id], at.x, at.y, 11., INK);
    if crew.cargo > 0 {
        dot(at.x + 8., at.y + 8., 5.5, PAPER);
        label_center(num(crew.cargo), at.x + 8., at.y + 11.5, 8., INK);
    }
}

fn map(game: &Game, f: &Frame) {
    plate(22., 156., 562., 380., Color::new(0.05, 0.065, 0.08, 1.));
    for at in 0..CELLS {
        draw_tile(game, at, f.t);
    }
    // Hairline grid keeps targeting legible over painted terrain.
    let grid = Color::new(0., 0., 0., 0.22);
    for x in 1..COLS {
        panel(MAP_X + x as f32 * TW, MAP_Y, 1., TH * ROWS as f32, grid);
    }
    for y in 1..ROWS {
        panel(MAP_X, MAP_Y + y as f32 * TH, TW * COLS as f32, 1., grid);
    }
    // Weather and fire state on top of terrain. Hovering End turn brightens
    // the coming spread.
    let wind = vec2(f32::from(game.wind(false).0), f32::from(game.wind(false).1));
    let boost = if f.hover == Hover::End { 1.6 } else { 1. };
    for (at, (&tile, &warn)) in game.tiles.iter().zip(f.forecast.iter()).enumerate() {
        let (px, py) = origin(at);
        if tile.wet > 0 {
            wet_tile(px, py, tile.wet, hash(at), f.t);
        }
        if game.show_forecast && warn {
            ghost_flame(px, py, hash(at), f.t, boost);
        }
        if tile.fire > 0 {
            draw_fire(px, py, tile.fire, hash(at), f.t, wind);
        }
    }
    if f.live {
        reach(game, f);
        targets(game, f);
    }
    badges(game, f);
    for id in 0..3 {
        token(game, id, f);
    }
    if f.live {
        preview(game, f);
    }
    if let Some(reason) = game.setback {
        let spot = match reason {
            Setback::Village(at) => Some(center(at)),
            Setback::Trapped(id) => Some(token_at(game, id)),
            Setback::Nightfall => None,
        };
        if let Some(c) = spot {
            let p = f.alarm(1.3);
            glow(c.x, c.y, 40. + 8. * p, with_alpha(RED, 0.3));
            shapes::stroke_circle(c, 24. + 4. * p, 3., with_alpha(RED, 0.9));
        }
    }
}

/// The selected crew's movement range: a light wash and a crew-colored rim.
fn reach(game: &Game, f: &Frame) {
    let id = game.selected;
    let crew = game.crews[id];
    if crew.ap == 0 {
        return;
    }
    let cost = game.reach(id);
    let inside = |at: usize| cost[at] <= crew.ap;
    let color = CREW[id];
    let wash = with_alpha(color, 0.07 + 0.03 * pulse(f.t, 0.5));
    let rim = with_alpha(color, 0.75);
    for at in 0..CELLS {
        if !inside(at) {
            continue;
        }
        let (px, py) = origin(at);
        panel(px, py, TW, TH, wash);
        let (x, y) = (at % COLS, at / COLS);
        if x == 0 || !inside(at - 1) {
            panel(px, py, 2., TH, rim);
        }
        if x + 1 == COLS || !inside(at + 1) {
            panel(px + TW - 2., py, 2., TH, rim);
        }
        if y == 0 || !inside(at - COLS) {
            panel(px, py, TW, 2., rim);
        }
        if y + 1 == ROWS || !inside(at + COLS) {
            panel(px, py + TH - 2., TW, 2., rim);
        }
    }
}
/// Pulsing rings on everything the selected crew can work on right now.
fn targets(game: &Game, f: &Frame) {
    let id = game.selected;
    let crew = game.crews[id];
    let thirsty = crew.water < game.water_max(id);
    let beat = pulse(f.t, 1.3);
    for at in 0..CELLS {
        if Game::distance(crew.tile, at) > Game::range(id) {
            continue;
        }
        let tile = game.tiles[at];
        let source =
            matches!(tile.terrain, Terrain::Water | Terrain::Base) && (thirsty || crew.cargo > 0);
        if !(tile.fire > 0 || tile.people > 0 || source) {
            continue;
        }
        let color = match game.action(id, at) {
            Some(Action::Hose | Action::Refill | Action::Unload) => SKY,
            Some(Action::Load) => PAPER,
            _ => continue,
        };
        let c = center(at);
        shapes::stroke_circle(
            c,
            18. + 3. * beat,
            2.,
            with_alpha(color, 0.45 + 0.45 * beat),
        );
    }
}
/// Resident counts, the base marker and alarms on threatened villages.
fn badges(game: &Game, f: &Frame) {
    for at in 0..CELLS {
        let tile = game.tiles[at];
        let (px, py) = origin(at);
        if at == BASE {
            icon_badge(
                Icon::Home,
                px + 11.,
                py + 11.,
                17.,
                Color::new(0.82, 0.24, 0.2, 1.),
                PAPER,
            );
            continue;
        }
        if tile.terrain != Terrain::Town || tile.people == 0 {
            continue;
        }
        let threatened = f.forecast[at] || Game::adjacent(at).any(|n| game.tiles[n].fire > 0);
        if threatened {
            let p = f.alarm(1.6);
            glow(
                px + TW * 0.5,
                py + TH * 0.5,
                34. + 6. * p,
                with_alpha(RED, 0.18 + 0.2 * p),
            );
            shapes::stroke_circle(
                vec2(px + TW * 0.5, py + TH * 0.5),
                22. + 2. * p,
                2.,
                with_alpha(RED, 0.5 + 0.4 * p),
            );
        }
        let n = num(tile.people);
        let w = text_width(n, TextStyle::modern(11., INK)) + 24.;
        let lift = if threatened { bob(f.t, 1.6, 1.) } else { 0. };
        pill(
            px + 2.,
            py + 2. + lift,
            w,
            16.,
            if threatened {
                Color::new(1., 0.86, 0.8, 1.)
            } else {
                Color::new(0.95, 0.93, 0.86, 1.)
            },
        );
        icon(
            Icon::People,
            px + 11.,
            py + 10. + lift,
            12.,
            if threatened {
                Color::new(0.75, 0.15, 0.1, 1.)
            } else {
                Color::new(0.45, 0.3, 0.2, 1.)
            },
        );
        label(n, px + 19., py + 14.5 + lift, 11., INK);
    }
}

/// What the focused tile would do for the selected crew: a path, a spray
/// arc, the heat a spray removes and a chip with the cost and the gain.
fn preview(game: &Game, f: &Frame) {
    let id = game.selected;
    let color = CREW[id];
    if let Hover::Tile(at) = f.hover {
        let (px, py) = origin(at);
        draw_rectangle_lines(px, py, TW, TH, 2., Color::new(1., 1., 1., 0.55));
    }
    match f.hover {
        Hover::Drop if game.drop_lack(game.cursor).is_none() => {
            let beat = pulse(f.t, 1.5);
            for at in Game::footprint(game.cursor) {
                let (px, py) = origin(at);
                panel(px, py, TW, TH, with_alpha(SKY, 0.16 + 0.12 * beat));
                draw_rectangle_lines(px + 1., py + 1., TW - 2., TH - 2., 2., with_alpha(SKY, 0.8));
            }
        }
        Hover::Cut if game.cut_lack(id, game.cursor).is_none() => {
            let (px, py) = origin(game.cursor);
            panel(
                px,
                py,
                TW,
                TH,
                with_alpha(DIRT, 0.25 + 0.15 * pulse(f.t, 1.5)),
            );
        }
        _ => {}
    }
    let focus = f.focus;
    let c = center(focus);
    let from = token_at(game, id);
    match game.action(id, focus) {
        Some(Action::Move(path, cost)) => {
            route_dots(&path, from, color, f.t);
            dot(c.x, c.y + 8., 8.5, with_alpha(color, 0.45));
            shapes::stroke_circle(vec2(c.x, c.y + 8.), 10., 1.5, with_alpha(PAPER, 0.6));
            cost_chip(c, None, cost);
        }
        Some(Action::Hose) => {
            spray_arc(from, c, f.t);
            let tile = game.tiles[focus];
            let (px, py) = origin(focus);
            heat_pips(px, py, tile.fire, Game::power(id).min(tile.fire));
            if tile.fire <= Game::power(id) {
                icon(Icon::Cloud, c.x, c.y - 2. + bob(f.t, 1.2, 2.), 22., STEAM);
            }
            cost_chip(c, Some((Icon::Drop, MINUS[1], BLUE)), 2);
        }
        Some(Action::Load) => {
            let n = game.boarding(id, focus);
            cost_chip(c, Some((Icon::Person, plus(n), PAPER)), 2);
        }
        Some(Action::Unload) => {
            let n = game.crews[id].cargo;
            cost_chip(c, Some((Icon::Home, plus(n), BLUE)), 1);
        }
        Some(Action::Refill) => {
            let n = game.water_max(id) - game.crews[id].water;
            cost_chip(c, Some((Icon::Drop, plus(n), BLUE)), 1);
        }
        None => {
            if let Some(lack) = game.lack(id, focus) {
                lack_chip(c, lack);
            }
        }
    }
    // Keyboard cursor: breathing gold corners.
    let (cx, cy) = origin(game.cursor);
    let breathe = 1.5 * pulse(f.t, 0.8);
    panel(cx, cy, TW, TH, Color::new(1., 0.85, 0.4, 0.08));
    brackets(
        cx - 1. - breathe,
        cy - 1. - breathe,
        TW + 2. + breathe * 2.,
        TH + 2. + breathe * 2.,
        11.,
        2.5,
        Color::new(1., 0.82, 0.36, 1.),
    );
}
/// Dots flowing along a movement path from the crew token.
fn route_dots(path: &[usize], from: Vec2, color: Color, t: f32) {
    let mut prev = from;
    let mut travelled = 0.;
    let step = 8.;
    let shift = (t * 14.) % step;
    for (k, &at) in path.iter().enumerate().skip(1) {
        let next = center(at)
            + if k + 1 == path.len() {
                vec2(0., 8.)
            } else {
                Vec2::ZERO
            };
        let span = (next - prev).length();
        let mut d = (step - (travelled % step)) % step + shift;
        while d < span {
            let p = prev + (next - prev) * (d / span);
            dot(p.x, p.y, 2.4, Color::new(0., 0., 0., 0.35));
            dot(p.x, p.y, 1.8, color);
            d += step;
        }
        travelled += span;
        prev = next;
    }
}
/// A dashed water arc from the crew to the fire.
fn spray_arc(from: Vec2, to: Vec2, t: f32) {
    let mid = (from + to) * 0.5 - vec2(0., 22. + (to - from).length() * 0.15);
    let n = 14;
    let shift = (t * 2.) % 1.;
    for k in 0..n {
        let u = (k as f32 + shift) / n as f32;
        let a = from * (1. - u) * (1. - u) + mid * 2. * u * (1. - u) + to * u * u;
        dot(a.x, a.y, 2.2, with_alpha(SKY, 0.4 + 0.5 * u));
    }
}
/// Cost of a preview: an optional resource change and the action points.
fn cost_chip(c: Vec2, gain: Option<(Icon, &str, Color)>, ap: u8) {
    let s = TextStyle::modern(13., PAPER);
    let mut w = 10. + 14. + 4. + f32::from(ap) * 8. + 8.;
    if let Some((_, text, _)) = gain {
        w += 16. + text_width(text, s) + 10.;
    }
    let y = if c.y < MAP_Y + TH {
        c.y + TH * 0.5 + 4.
    } else {
        c.y - TH * 0.5 - 24.
    };
    let x = (c.x - w * 0.5).clamp(MAP_X, MAP_X + TW * COLS as f32 - w);
    framed(
        Rect::new(x, y, w, 20.),
        10.,
        1.,
        Color::new(0.4, 0.45, 0.52, 1.),
        Color::new(0.06, 0.08, 0.11, 0.94),
    );
    let mut cx = x + 9.;
    if let Some((glyph, text, tone)) = gain {
        icon(glyph, cx + 7., y + 10., 14., tone);
        cx += 16.;
        cx += draw_text_styled(text, cx, s.centered_baseline(y + 10.), s.with_color(tone)).width
            + 10.;
    }
    icon(Icon::Bolt, cx + 7., y + 10., 13., GOLD);
    cx += 16.;
    for _ in 0..ap {
        dot(cx + 3., y + 10., 3., GOLD);
        cx += 8.;
    }
}
fn lack_icon(lack: Lack) -> Icon {
    match lack {
        Lack::Ap(_) => Icon::Bolt,
        Lack::Water => Icon::Drop,
        Lack::Seats => Icon::Person,
        Lack::Range => Icon::Target,
        Lack::Credits(_) => Icon::Coin,
        Lack::Sorties => Icon::Rain,
        Lack::NoFire => Icon::Flame,
        Lack::Uncuttable => Icon::Pickaxe,
        Lack::Maxed => Icon::Check,
    }
}
/// A refused preview: the missing resource, crossed out.
fn lack_chip(c: Vec2, lack: Lack) {
    let y = if c.y < MAP_Y + TH {
        c.y + TH * 0.5 + 4.
    } else {
        c.y - TH * 0.5 - 24.
    };
    let x = (c.x - 17.).clamp(MAP_X, MAP_X + TW * COLS as f32 - 34.);
    framed(
        Rect::new(x, y, 34., 20.),
        10.,
        1.,
        RED,
        Color::new(0.16, 0.05, 0.05, 0.94),
    );
    icon(lack_icon(lack), x + 17., y + 10., 14., SOFT);
    cross(x + 17., y + 10., 6., RED);
}

/// The focused tile's properties as icons and pips, under the map.
fn inspector(game: &Game, f: &Frame) {
    // Measure first so the backing hugs whatever the tile has to show.
    let w = inspector_row(game, f, false);
    slab(
        22.,
        538.,
        (w - 26.).max(28.),
        28.,
        14.,
        Color::new(1., 1., 1., 0.04),
    );
    inspector_row(game, f, true);
}
/// Lay out (and with `draw`, paint) the focused tile's icons; returns the
/// right edge.
fn inspector_row(game: &Game, f: &Frame, draw: bool) -> f32 {
    let at = f.focus;
    let tile = game.tiles[at];
    let cy = 552.;
    let mut x = 34.;
    let glyph = match tile.terrain {
        Terrain::Base => Some(Icon::Home),
        Terrain::Town => Some(Icon::People),
        Terrain::Water => Some(Icon::Wave),
        Terrain::Rock => Some(Icon::Mountain),
        Terrain::Road => None,
        Terrain::Forest => Some(Icon::Tree),
        Terrain::Brush => Some(Icon::Leaf),
        Terrain::Grass => Some(Icon::Sprout),
    };
    let mark = |glyph: Icon, x: f32, size: f32, color: Color| {
        if draw {
            icon(glyph, x, cy, size, color);
        }
    };
    let pips = |x: f32, lit: u8, total: u8, color: Color| {
        if draw {
            pip_row(x, cy, lit, total, 3.2, color)
        } else {
            pip_width(total, 3.2)
        }
    };
    if let Some(glyph) = glyph {
        mark(glyph, x + 9., 18., SOFT);
        x += 28.;
    }
    let village = MISSIONS[game.index]
        .villages
        .iter()
        .find(|v| v.y * COLS + v.x == at);
    if let Some(v) = village {
        let tone = if tile.ruined { MUTED } else { PAPER };
        x += if draw {
            label(v.name, x, cy + 5., 15., tone)
        } else {
            text_width(v.name, TextStyle::modern(15., tone))
        } + 14.;
        if tile.people > 0 {
            mark(Icon::People, x + 7., 14., SOFT);
            x += 17.;
            x += if draw {
                label(num(tile.people), x, cy + 5., 14., SOFT)
            } else {
                text_width(num(tile.people), TextStyle::modern(14., SOFT))
            } + 14.;
        }
    }
    if tile.fuel > 0 {
        mark(Icon::Leaf, x + 7., 14., GREEN);
        x += 17.;
        x += pips(x, tile.fuel, 4, GREEN) + 14.;
    } else if tile.fire == 0 {
        // Roads, water, rock, base, breaks and ash never burn.
        mark(Icon::Shield, x + 8., 16., SKY);
        x += 26.;
    }
    if tile.fire > 0 {
        mark(Icon::Flame, x + 7., 14., RED);
        x += 17.;
        x += pips(x, tile.fire, 3, RED) + 14.;
    }
    if tile.wet > 0 {
        mark(Icon::Drop, x + 7., 14., BLUE);
        x += 17.;
        x += pips(x, tile.wet, 3, BLUE) + 14.;
    }
    if game.show_forecast && f.forecast[at] {
        mark(Icon::Alert, x + 8., 16., RED);
        x += 26.;
    }
    x
}

// --- Above the map: wind, the forecast switch and the deadline. ---

fn strip(game: &Game, f: &Frame) {
    if game.phase == Phase::Briefing {
        return;
    }
    let cy = 134.;
    icon(Icon::Wind, 38., cy, 18., MUTED);
    wind_dial(70., cy, 14., game.wind(false), AMBER, f.t, true);
    // "Then": a small chevron toward the next shift.
    draw_line(90., cy - 4., 95., cy, 2., MUTED);
    draw_line(95., cy, 90., cy + 4., 2., MUTED);
    wind_dial(
        112.,
        cy,
        10.,
        game.wind(true),
        with_alpha(SOFT, 0.55),
        f.t,
        false,
    );
    // Forecast switch (5).
    let r = FORECAST;
    let on = game.show_forecast;
    let hover = f.hover == Hover::Forecast;
    let tone = if on { RED } else { MUTED };
    framed(
        r,
        r.h * 0.5,
        1.,
        tint(tone, if hover { 0.8 } else { 0.45 }),
        tint(tone, if hover { 0.3 } else { 0.16 }),
    );
    key("5", r.x + 5., r.y + 4.);
    icon(
        if on { Icon::Eye } else { Icon::EyeOff },
        r.x + r.w - 18.,
        r.y + r.h * 0.5,
        18.,
        if on { lighten(RED, 0.35) } else { MUTED },
    );
    // Deadline: hourglass, turns left (this one included), a cell per turn.
    let limit = MISSIONS[game.index].limit;
    let left = (limit + 1).saturating_sub(game.turn);
    let urgent = left <= 3;
    let tone = if urgent { RED } else { AMBER };
    let beat = if urgent && f.live { f.alarm(1.4) } else { 0. };
    icon(Icon::Hourglass, 300., cy, 18., lighten(tone, 0.3 * beat));
    label(
        &left.to_string(),
        312.,
        cy + 6.,
        17.,
        if urgent { RED } else { PAPER },
    );
    let (sx, right) = (342., 552.);
    let used = game.turn.saturating_sub(1).min(limit);
    let gap = 2.;
    let n = f32::from(limit);
    let cell = (right - sx - gap * (n - 1.)) / n;
    for k in 0..limit {
        let x = sx + f32::from(k) * (cell + gap);
        let color = if k < used {
            Color::new(1., 1., 1., 0.08)
        } else if k == used {
            lighten(tone, 0.25 + 0.3 * pulse(f.t, 1.2))
        } else {
            with_alpha(tone, 0.7)
        };
        panel(x, cy - 5., cell, 10., color);
    }
    icon(Icon::Moon, 566., cy, 16., if urgent { RED } else { MUTED });
}

// --- Command column. ---

/// Shared button surface; `glow_amount` (0..1) calls attention to it.
fn surface(r: Rect, tone: Color, ready: bool, hover: bool, glow_amount: f32) -> Rect {
    let lift = if hover && ready { 2. } else { 0. };
    let r = Rect::new(r.x, r.y - lift, r.w, r.h);
    if glow_amount > 0. {
        glow(
            r.x + r.w * 0.5,
            r.y + r.h * 0.5,
            r.w * 0.45,
            with_alpha(tone, 0.16 * glow_amount),
        );
    }
    slab(r.x, r.y + 3., r.w, r.h, 10., Color::new(0., 0., 0., 0.35));
    let (rim, fill) = match (ready, hover) {
        (true, true) => (tint(tone, 0.85), tint(tone, 0.32)),
        (true, false) => (tint(tone, 0.5 + 0.3 * glow_amount), tint(tone, 0.2)),
        (false, _) => (tint(tone, 0.16), tint(tone, 0.06)),
    };
    framed(r, 10., 1., rim, fill);
    panel(
        r.x + 10.,
        r.y + 1.,
        r.w - 20.,
        1.,
        Color::new(1., 1., 1., if ready { 0.08 } else { 0.03 }),
    );
    r
}
/// Action points drawn right-aligned at `right`; returns the left edge.
fn ap_cost(right: f32, cy: f32, ap: u8, color: Color) -> f32 {
    let mut x = right;
    for _ in 0..ap {
        dot(x - 3.5, cy, 3.5, color);
        x -= 10.;
    }
    icon(Icon::Bolt, x - 6., cy, 15., color);
    x - 14.
}
/// An icon and a short value right-aligned at `right`; returns the left edge.
fn gain(right: f32, cy: f32, glyph: Icon, text: &str, color: Color) -> f32 {
    let w = label_right(text, right, cy + 5., 15., color);
    icon(glyph, right - w - 10., cy, 16., color);
    right - w - 20.
}

fn crew_card(game: &Game, id: usize, f: &Frame) {
    let y = CARD_Y + id as f32 * CARD_STEP;
    let crew = game.crews[id];
    let color = CREW[id];
    let selected = id == game.selected;
    let hover = f.hover == Hover::Card(id);
    let spent = f.live && crew.ap == 0;
    let r = Rect::new(SIDE_X, y, SIDE_W, CARD_H);
    if selected && f.live {
        glow(
            r.x + r.w * 0.5,
            r.y + r.h * 0.5,
            r.w * 0.42,
            with_alpha(color, 0.1 + 0.06 * pulse(f.t, 0.7)),
        );
    }
    let fill = match (selected, hover) {
        (true, _) => Color::new(0.14, 0.18, 0.2, 1.),
        (false, true) => Color::new(0.11, 0.14, 0.17, 1.),
        _ => Color::new(0.08, 0.105, 0.13, 1.),
    };
    if selected {
        framed(r, 10., 2., darken(color, 0.15), fill);
    } else if hover {
        framed(r, 10., 1., tint(color, 0.45), fill);
    } else {
        slab(r.x, r.y, r.w, r.h, 10., fill);
    }
    let ink = if spent { 0.45 } else { 1. };
    key(num(id as u8 + 1), SIDE_X + 10., y + 20.);
    icon_badge(
        ROLE[id],
        SIDE_X + 56.,
        y + 30.,
        38.,
        with_alpha(color, ink),
        INK,
    );
    label(
        NAMES[id],
        SIDE_X + 84.,
        y + 26.,
        16.,
        with_alpha(if selected { PAPER } else { SOFT }, ink),
    );
    // Action points: a bolt and one pip per point this turn.
    let max = game.max_ap();
    let right = SIDE_X + SIDE_W - 14.;
    let w = pip_row(
        right - (f32::from(max) * 11. - 3.),
        y + 20.,
        crew.ap,
        max,
        4.,
        GOLD,
    );
    icon(
        Icon::Bolt,
        right - w - 12.,
        y + 20.,
        15.,
        with_alpha(GOLD, ink),
    );
    // Water tank: a cell per unit.
    let water_max = game.water_max(id);
    let tank = if crew.water == 0 { RED } else { BLUE };
    icon(Icon::Drop, SIDE_X + 91., y + 44., 14., tank);
    cells(
        SIDE_X + 102.,
        y + 40.,
        104.,
        8.,
        crew.water,
        water_max,
        tank,
    );
    // Seats: one pip per passenger place, filled while carrying.
    let cap = Game::capacity(id);
    let seat_w = pip_row(
        right - (f32::from(cap) * 10. - 3.),
        y + 44.,
        crew.cargo,
        cap,
        3.5,
        PAPER,
    );
    icon(
        Icon::Person,
        right - seat_w - 11.,
        y + 44.,
        14.,
        if crew.cargo > 0 { PAPER } else { MUTED },
    );
}

fn sidebar(game: &Game, f: &Frame) {
    for id in 0..3 {
        crew_card(game, id, f);
    }
    // Wallet: credits and air sorties.
    let cy = WALLET_Y;
    let w = icon_value(
        Icon::Coin,
        &game.cash.to_string(),
        SIDE_X + 4.,
        cy,
        18.,
        AMBER,
    );
    let total = game.drops + game.used_drops;
    icon(Icon::Rain, SIDE_X + w + 34., cy, 18., SKY);
    pip_row(SIDE_X + w + 48., cy, game.drops, total, 4., SKY);
    act_button(game, f);
    cut_button(game, f);
    drop_button(game, f);
    end_button(game, f);
}

fn act_button(game: &Game, f: &Frame) {
    let id = game.selected;
    let plan = if f.live {
        game.action(id, game.cursor)
    } else {
        None
    };
    let lack = if f.live && plan.is_none() {
        game.lack(id, game.cursor)
    } else {
        None
    };
    let ready = plan.is_some();
    let r = surface(ACT, CREW[id], ready, f.hover == Hover::Act, 0.);
    let cy = r.y + r.h * 0.5;
    let kw = key("Space", r.x + 12., cy - 10.);
    let (glyph, word) = match plan {
        Some(Action::Move(..)) => (Icon::Pin, "Move"),
        Some(Action::Hose) => (Icon::Drop, "Spray"),
        Some(Action::Load) => (Icon::Person, "Load"),
        Some(Action::Unload) => (Icon::Home, "Unload"),
        Some(Action::Refill) => (Icon::Wave, "Refill"),
        None => (Icon::Target, ""),
    };
    let ink = if ready { PAPER } else { MUTED };
    // With nothing to do here, the target glyph breathes: pick a tile.
    let idle = if f.live && !ready && lack.is_none() {
        0.25 * pulse(f.t, 0.7)
    } else {
        0.
    };
    icon(
        glyph,
        r.x + 12. + kw + 20.,
        cy,
        20. + 2. * idle,
        if ready {
            lighten(CREW[id], 0.25)
        } else {
            lighten(MUTED, idle)
        },
    );
    label(word, r.x + 12. + kw + 36., cy + 6., 17., ink);
    let right = r.x + r.w - 16.;
    match plan {
        Some(Action::Move(_, cost)) => {
            ap_cost(right, cy, cost, GOLD);
        }
        Some(Action::Hose) => {
            let x = ap_cost(right, cy, 2, GOLD);
            gain(x - 6., cy, Icon::Drop, MINUS[1], BLUE);
        }
        Some(Action::Load) => {
            let x = ap_cost(right, cy, 2, GOLD);
            gain(
                x - 6.,
                cy,
                Icon::Person,
                plus(game.boarding(id, game.cursor)),
                PAPER,
            );
        }
        Some(Action::Unload) => {
            let x = ap_cost(right, cy, 1, GOLD);
            gain(x - 6., cy, Icon::Home, plus(game.crews[id].cargo), BLUE);
        }
        Some(Action::Refill) => {
            let x = ap_cost(right, cy, 1, GOLD);
            let n = game.water_max(id) - game.crews[id].water;
            gain(x - 6., cy, Icon::Drop, plus(n), BLUE);
        }
        None => {
            if let Some(lack) = lack {
                icon(lack_icon(lack), right - 10., cy, 18., SOFT);
                cross(right - 10., cy, 7., RED);
            }
        }
    }
}

fn cut_button(game: &Game, f: &Frame) {
    let id = game.selected;
    let lack = game.cut_lack(id, game.cursor);
    let ready = f.live && lack.is_none();
    let r = surface(CUT, DIRT, ready, f.hover == Hover::Cut, 0.);
    let cy = r.y + r.h * 0.5;
    let kw = key("X", r.x + 10., cy - 10.);
    let ink = if ready { PAPER } else { MUTED };
    icon(
        Icon::Pickaxe,
        r.x + 10. + kw + 16.,
        cy,
        18.,
        if ready { lighten(DIRT, 0.3) } else { MUTED },
    );
    label("Break", r.x + 10. + kw + 29., cy + 5., 14., ink);
    let cost = Game::cut_cost(id);
    let short = f.live && matches!(lack, Some(Lack::Ap(_)));
    ap_cost(
        r.x + r.w - 12.,
        cy,
        cost,
        if short {
            RED
        } else if ready {
            GOLD
        } else {
            with_alpha(GOLD, 0.45)
        },
    );
}

fn drop_button(game: &Game, f: &Frame) {
    let lack = game.drop_lack(game.cursor);
    let ready = f.live && lack.is_none();
    let r = surface(DROP, SKY, ready, f.hover == Hover::Drop, 0.);
    let cy = r.y + r.h * 0.5;
    let kw = key("4", r.x + 10., cy - 10.);
    let ink = if ready { PAPER } else { MUTED };
    icon(
        Icon::Rain,
        r.x + 10. + kw + 16.,
        cy,
        18.,
        if ready { SKY } else { MUTED },
    );
    label("Drop", r.x + 10. + kw + 29., cy + 5., 14., ink);
    let broke = f.live && matches!(lack, Some(Lack::Credits(_) | Lack::Sorties));
    let tone = if broke {
        RED
    } else if ready {
        AMBER
    } else {
        with_alpha(AMBER, 0.45)
    };
    let w = label_right(&DROP_COST.to_string(), r.x + r.w - 12., cy + 5., 15., tone);
    icon(Icon::Coin, r.x + r.w - 12. - w - 10., cy, 15., tone);
}

fn end_button(game: &Game, f: &Frame) {
    let idle = f.live && game.crews.iter().all(|c| c.ap == 0);
    let beat = if idle { pulse(f.t, 0.9) } else { 0. };
    let r = surface(END, AMBER, f.live, f.hover == Hover::End, beat);
    let cy = r.y + r.h * 0.5;
    let kw = key("C", r.x + 12., cy - 10.);
    let ink = if f.live { PAPER } else { MUTED };
    icon(
        Icon::Hourglass,
        r.x + 12. + kw + 20.,
        cy,
        20.,
        if f.live { AMBER } else { MUTED },
    );
    label("End turn", r.x + 12. + kw + 36., cy + 6., 17., ink);
    if game.show_forecast && f.live {
        // The coming spread: how many tiles ignite, or a check when none.
        let n = f.forecast.iter().filter(|&&w| w).count();
        let right = r.x + r.w - 16.;
        if n == 0 {
            icon(Icon::Check, right - 8., cy, 16., GREEN);
            icon(Icon::Flame, right - 27., cy, 16., MUTED);
        } else {
            gain(right, cy, Icon::Flame, &format!("+{n}"), lighten(RED, 0.15));
        }
    }
}

// --- Between rounds: briefing, debrief and setback. ---

/// The call to action, drawn inside its click rectangle.
fn cta(word: &str, glyph: Icon, color: Color, f: &Frame, band: Rect, h: f32) {
    let hover = f.hover == Hover::Proceed;
    let lift = if hover { 2. } else { 0. };
    let pad = (band.h - h) * 0.5;
    let (x, y, w) = (band.x + 4., band.y + pad - lift, band.w - 8.);
    glow(
        x + w * 0.5,
        y + h * 0.5,
        w * 0.4,
        with_alpha(color, 0.12 + 0.1 * pulse(f.t, 0.8)),
    );
    let top = if hover {
        lighten(color, 0.18)
    } else {
        lighten(color, 0.08)
    };
    rounded_gradient(x, y, w, h, 10., top, darken(color, 0.14));
    let s = TextStyle::modern(18., INK);
    let tw = text_width(word, s);
    let kw = text_width("Space", TextStyle::modern(13., PAPER)) + 12.;
    let total = kw + 14. + 22. + 8. + tw;
    let mut cx = x + w * 0.5 - total * 0.5;
    key("Space", cx, y + h * 0.5 - 10.);
    cx += kw + 14.;
    icon(glyph, cx + 11., y + h * 0.5, 22., INK);
    cx += 30.;
    draw_text_styled(word, cx, s.centered_baseline(y + h * 0.5), s);
}

/// The incident at a glance in the strip above the map; the whole district
/// stays visible with its first forecast.
fn briefing(game: &Game, f: &Frame) {
    let mission = &MISSIONS[game.index];
    let cy = 134.;
    let w = label(mission.name, 30., cy + 8., 22., PAPER);
    let mut x = 30. + w + 22.;
    x += icon_value(Icon::People, &game.quota().to_string(), x, cy, 17., BLUE) + 20.;
    x += icon_value(Icon::Flame, &game.fire_count().to_string(), x, cy, 17., RED) + 20.;
    x += icon_value(
        Icon::Hourglass,
        &mission.limit.to_string(),
        x,
        cy,
        17.,
        AMBER,
    ) + 20.;
    icon(Icon::Wind, x + 9., cy, 17., MUTED);
    wind_dial(x + 34., cy, 13., game.wind(false), AMBER, f.t, true);
    cta("Deploy", Icon::Play, AMBER, f, DEPLOY, 30.);
}

fn debrief(game: &Game, f: &Frame) {
    card(39., 168., 529., 360., Color::new(0.07, 0.09, 0.11, 1.));
    let mission = &MISSIONS[game.index];
    icon_badge(Icon::Check, 76., 210., 34., GREEN, INK);
    label(mission.name, 102., 219., 24., PAPER);
    // The medal this incident earned.
    let medals = game.medals[game.index];
    for k in 0..3u8 {
        let lit = k < medals;
        let at = vec2(470. + f32::from(k) * 34., 208.);
        if lit {
            glow(at.x, at.y, 20., with_alpha(AMBER, 0.2));
        }
        icon(
            Icon::Star,
            at.x,
            at.y + if lit {
                bob(f.t + f32::from(k) * 0.2, 0.7, 1.5)
            } else {
                0.
            },
            28.,
            if lit {
                AMBER
            } else {
                Color::new(1., 1., 1., 0.13)
            },
        );
    }
    // Credits to spend.
    icon_value(Icon::Coin, &game.cash.to_string(), 62., 276., 22., AMBER);
    let glyphs = [Icon::Drop, Icon::Bolt, Icon::Rain];
    let tones = [BLUE, GOLD, SKY];
    let gains = ["+2", "+1", "+1"];
    for id in 0..3 {
        let hover = f.hover == Hover::Shop(id);
        let level = game.upgrades[id];
        let cost = game.upgrade_cost(id);
        let open = level < 2;
        let afford = open && game.cash >= cost;
        let tone = tones[id];
        let base = shop_card(id);
        let lift = if hover && afford { 3. } else { 0. };
        let r = Rect::new(base.x, base.y - lift, base.w, base.h);
        if hover && afford {
            glow(
                r.x + r.w * 0.5,
                r.y + r.h * 0.5,
                80.,
                with_alpha(tone, 0.14),
            );
        }
        slab(r.x, r.y + 3., r.w, r.h, 10., Color::new(0., 0., 0., 0.3));
        let rim = if afford {
            tint(tone, if hover { 0.9 } else { 0.55 })
        } else {
            tint(tone, 0.18)
        };
        framed(r, 10., 1., rim, Color::new(0.11, 0.15, 0.17, 1.));
        key(num(id as u8 + 1), r.x + 10., r.y + 10.);
        for k in 0..2u8 {
            let at = vec2(r.x + r.w - 30. + f32::from(k) * 12., r.y + 20.);
            if k < level {
                shapes::fill_circle(at, 4., tone);
            } else {
                shapes::stroke_circle(at, 3.5, 1.3, with_alpha(tone, 0.5));
            }
        }
        let ink = if open { tone } else { MUTED };
        icon(glyphs[id], r.x + r.w * 0.5 - 18., r.y + 56., 34., ink);
        label(
            gains[id],
            r.x + r.w * 0.5 + 4.,
            r.y + 66.,
            24.,
            if open { PAPER } else { MUTED },
        );
        let cy = r.y + r.h - 18.;
        if open {
            let tone = if afford { AMBER } else { RED };
            let text = cost.to_string();
            let w = text_width(&text, TextStyle::modern(15., tone)) + 22.;
            icon_value(Icon::Coin, &text, r.x + r.w * 0.5 - w * 0.5, cy, 15., tone);
        } else {
            icon(Icon::Check, r.x + r.w * 0.5, cy, 18., GREEN);
        }
    }
    cta("Next", Icon::ArrowRight, GREEN, f, PROCEED, 40.);
}
/// A compact bar under the failure: what went wrong, the hearts left and
/// the retry button. The map above keeps the failure ringed in red.
fn setback(game: &Game, f: &Frame) {
    let r = Rect::new(44., 458., 520., 72.);
    card(r.x, r.y, r.w, r.h, Color::new(0.09, 0.06, 0.07, 1.));
    rounded(r.x + 20., r.y, 64., 3., 1.5, RED);
    let (glyph, word, burning) = match game.setback {
        Some(Setback::Village(_)) => (Icon::Home, "Village lost", true),
        Some(Setback::Trapped(_)) => (Icon::Person, "Crew trapped", true),
        Some(Setback::Nightfall) | None => (Icon::Moon, "Nightfall", false),
    };
    let beat = f.alarm(1.2);
    let c = vec2(r.x + 40., r.y + r.h * 0.5);
    glow(c.x, c.y, 30., with_alpha(RED, 0.16 + 0.12 * beat));
    icon(glyph, c.x, c.y, 34., RED);
    if burning {
        icon(Icon::Flame, c.x + 15., c.y + 13., 15., AMBER);
    }
    label(word, r.x + 72., r.y + 33., 21., PAPER);
    // Hearts left; the one a retry spends pulses.
    for k in 0..3u8 {
        let at = vec2(r.x + 82. + f32::from(k) * 24., r.y + 52.);
        let spending = k + 1 == game.retries;
        let color = if spending {
            with_alpha(RED, 0.5 + 0.5 * beat)
        } else if k < game.retries {
            RED
        } else {
            Color::new(1., 1., 1., 0.14)
        };
        icon(
            Icon::Heart,
            at.x,
            at.y,
            if spending { 17. + 2. * beat } else { 17. },
            color,
        );
    }
    cta("Retry", Icon::Restart, BLUE, f, RETRY, 40.);
}

fn overlay(game: &Game, f: &Frame) {
    match game.phase {
        Phase::Briefing => briefing(game, f),
        Phase::Debrief => debrief(game, f),
        Phase::Setback => setback(game, f),
        Phase::Active | Phase::Ended => {}
    }
}

// --- Header objective and results card. ---

/// Residents as people: home, aboard, waiting (red when their village is
/// threatened). Returns `(home, aboard, waiting, threatened)`.
fn residents(game: &Game) -> (u8, u8, u8, u8) {
    let quota = game.quota();
    match game.phase {
        Phase::Debrief => return (quota, 0, 0, 0),
        Phase::Ended if game.outcome() == Outcome::Won => return (quota, 0, 0, 0),
        _ => {}
    }
    let forecast = game.forecast();
    let aboard: u8 = game.crews.iter().map(|c| c.cargo).sum();
    let mut waiting = 0;
    let mut threatened = 0;
    for (at, tile) in game.tiles.iter().enumerate() {
        if tile.people > 0 {
            waiting += tile.people;
            if forecast[at] || Game::adjacent(at).any(|n| game.tiles[n].fire > 0) {
                threatened += tile.people;
            }
        }
    }
    (game.landed, aboard, waiting, threatened)
}

fn objective(game: &Game, area: Rect) -> bool {
    let t = idle_clock();
    let calm = {
        let policy = feedback_policy();
        policy.reduced_motion() || policy.reduced_flash()
    };
    let cy = area.y + area.h * 0.5;
    let mut x = area.x + 2.;
    let (home, aboard, waiting, threatened) = residents(game);
    let quota = game.quota();
    icon(Icon::Home, x + 10., cy, 20., BLUE);
    x += 26.;
    let safe_waiting = waiting.saturating_sub(threatened);
    for k in 0..quota {
        let color = if k < home {
            BLUE
        } else if k < home + aboard {
            PAPER
        } else if k < home + aboard + safe_waiting {
            Color::new(1., 1., 1., 0.28)
        } else {
            with_alpha(
                RED,
                if calm {
                    0.85
                } else {
                    0.6 + 0.4 * pulse(t, 1.6)
                },
            )
        };
        icon(Icon::Person, x + 6. + f32::from(k) * 14., cy, 14., color);
    }
    x += f32::from(quota) * 14. + 8.;
    let s = TextStyle::modern(16., PAPER);
    let count = format!("{home}/{quota}");
    x += draw_text_styled(&count, x, s.centered_baseline(cy), s).width + 26.;
    // Fires still burning; a check once contained.
    let fires = game.fire_count();
    if fires == 0 {
        icon(Icon::Flame, x + 9., cy, 18., MUTED);
        icon(Icon::Check, x + 26., cy, 16., GREEN);
        x += 44.;
    } else {
        x += icon_value(Icon::Flame, &fires.to_string(), x, cy, 18., RED) + 24.;
    }
    // Medal on track (or earned), then career retries.
    let stars = match game.phase {
        Phase::Debrief | Phase::Ended => game.medals[game.index],
        _ => game.stars(),
    };
    for k in 0..3u8 {
        icon(
            Icon::Star,
            x + 9. + f32::from(k) * 19.,
            cy,
            17.,
            if k < stars {
                AMBER
            } else {
                Color::new(1., 1., 1., 0.14)
            },
        );
    }
    x += 3. * 19. + 22.;
    for k in 0..3u8 {
        icon(
            Icon::Heart,
            x + 9. + f32::from(k) * 19.,
            cy,
            16.,
            if k < game.retries {
                with_alpha(RED, 0.9)
            } else {
                Color::new(1., 1., 1., 0.14)
            },
        );
    }
    true
}

fn result(game: &Game, area: Rect) -> bool {
    let cleared = game.medals.iter().filter(|&&m| m > 0).count();
    let medals: u32 = game.medals.iter().map(|&m| u32::from(m)).sum();
    let stats = [
        (Icon::Home, game.rescued().to_string(), BLUE),
        (Icon::Flag, format!("{cleared}/6"), GREEN),
        (Icon::Star, format!("{medals}/18"), AMBER),
        (Icon::Heart, game.retries.to_string(), RED),
    ];
    // Even gaps between measured stats keep wide values apart.
    let size = 28.;
    let widths = [0, 1, 2, 3].map(|k| size * 1.25 + measure(&stats[k].1, size));
    let gap = ((area.w - widths.iter().sum::<f32>()) / 3.).clamp(24., 90.);
    let mut x = area.x;
    for (k, (glyph, value, color)) in stats.iter().enumerate() {
        icon_value(*glyph, value, x, area.y + 34., size, *color);
        x += widths[k] + gap;
    }
    // Every incident's medal, left to right.
    let cy = area.y + 104.;
    for (i, &m) in game.medals.iter().enumerate() {
        let x = area.x + i as f32 * 100.;
        for k in 0..3u8 {
            icon(
                Icon::Star,
                x + 10. + f32::from(k) * 21.,
                cy,
                19.,
                if k < m {
                    AMBER
                } else {
                    Color::new(1., 1., 1., 0.12)
                },
            );
        }
    }
    true
}

// --- Effects for accepted events. ---

fn spot_at(spot: Spot) -> Vec2 {
    match spot {
        Spot::Tile(at) => center(at),
        Spot::Upgrade(id) => {
            let r = shop_card(id);
            vec2(r.x + r.w * 0.5, r.y + r.h * 0.5)
        }
    }
}
fn lack_word(lack: Lack) -> String {
    match lack {
        Lack::Ap(n) => format!("Need {n}"),
        Lack::Water => "Empty".into(),
        Lack::Seats => "Full".into(),
        Lack::Range => "Too far".into(),
        Lack::Credits(n) => format!("Need {n}"),
        Lack::Sorties => "None left".into(),
        Lack::NoFire => "No fire".into(),
        Lack::Uncuttable => "Can't cut".into(),
        Lack::Maxed => "Max".into(),
    }
}
/// Where credit changes pop up: right of the wallet's value.
const WALLET: Vec2 = Vec2::new(SIDE_X + 110., WALLET_Y + 4.);

fn effects(game: &Game, fx: &mut Juice) {
    let rise = vec2(0., 18.);
    for event in game.feed.iter() {
        match event {
            Event::Deployed => {
                fx.ring(center(BASE), AMBER, 70.);
                fx.sparkle(center(BASE), AMBER);
            }
            Event::Retried => fx.ring(center(BASE), BLUE, 70.),
            Event::Advanced => {}
            Event::Selected(id) => fx.ring(token_at(game, id), CREW[id], 26.),
            Event::Forecast(on) => {
                let c = vec2(FORECAST.x + FORECAST.w * 0.5, FORECAST.y + FORECAST.h * 0.5);
                fx.ring(c, if on { RED } else { MUTED }, 30.);
            }
            Event::Moved { crew, from, to } => {
                fx.burst(center(from) + vec2(0., 12.), DIRT, 8);
                let (px, py) = origin(to);
                fx.ring(
                    vec2(px + 15. + crew as f32 * 10., py + 31.),
                    CREW[crew],
                    24.,
                );
            }
            Event::Sprayed {
                at, removed, out, ..
            } => {
                let c = center(at);
                fx.burst(c, STEAM, if out { 28 } else { 14 });
                fx.ring(c, SKY, if out { 40. } else { 28. });
                if out {
                    fx.sparkle(c, STEAM);
                    fx.popup_icon(c - rise, Icon::Cloud, "Out", STEAM);
                } else {
                    let text = MINUS[usize::from(removed.min(3))];
                    fx.popup_icon(c - rise, Icon::Flame, text, SKY);
                }
            }
            Event::Loaded { at, people, .. } => {
                let c = center(at);
                fx.sparkle(c, PAPER);
                fx.ring(c, PAPER, 30.);
                fx.popup_icon(c - rise, Icon::Person, plus(people), PAPER);
            }
            Event::Delivered { people, .. } => {
                let c = center(BASE);
                fx.burst(c, BLUE, 20);
                fx.sparkle(c, SKY);
                fx.ring(c, BLUE, 48.);
                fx.popup_icon(c - rise, Icon::Home, plus(people), SKY);
            }
            Event::Refilled { crew, at, water } => {
                fx.burst(center(at), BLUE, 12);
                fx.popup_icon(token_at(game, crew) - rise, Icon::Drop, plus(water), SKY);
            }
            Event::Cut { at, .. } => {
                let c = center(at);
                fx.burst(c, DIRT, 22);
                fx.burst(c, Color::new(0.4, 0.62, 0.3, 1.), 8);
                fx.ring(c, DIRT, 30.);
                fx.shake(0.12);
            }
            Event::Dropped(at) => {
                for p in Game::footprint(at) {
                    fx.burst(center(p), SKY, 12);
                }
                fx.burst(center(at), STEAM, 18);
                fx.ring(center(at), SKY, 96.);
                fx.shake(0.25);
                fx.flash(Color::new(0.55, 0.78, 1., 0.1));
                fx.popup_icon(center(at) - rise, Icon::Rain, "", SKY);
                fx.popup_icon(WALLET, Icon::Coin, &format!("-{DROP_COST}"), AMBER);
            }
            Event::Spread { lit, out } => {
                for at in 0..CELLS {
                    if lit & (1 << at) != 0 {
                        fx.burst(center(at), AMBER, 10);
                        fx.ring(center(at), RED, 26.);
                    }
                    if out & (1 << at) != 0 {
                        fx.burst(center(at), Color::new(0.5, 0.5, 0.52, 0.8), 6);
                    }
                }
                if lit != 0 {
                    fx.shake(0.1);
                }
            }
            Event::Retreated(_) => {
                let c = center(BASE);
                fx.ring(c, RED, 42.);
                fx.popup_icon(c - rise, Icon::Alert, "Retreat", RED);
            }
            Event::Ruined(at) => {
                let c = center(at);
                fx.burst(c, Color::new(0.45, 0.12, 0.08, 1.), 22);
                fx.ring(c, RED, 46.);
                fx.shake(0.3);
                fx.popup_icon(c - rise, Icon::Home, "Lost", RED);
            }
            Event::Cleared { stars, credits } => {
                let c = vec2(MAP_X + TW * COLS as f32 * 0.5, 300.);
                fx.confetti(c, 60);
                fx.callout(c + vec2(0., 18.), "Secured!", GREEN);
                for k in 0..stars {
                    fx.popup_icon(
                        c + vec2(-34. + f32::from(k) * 34., 10.),
                        Icon::Star,
                        "",
                        AMBER,
                    );
                }
                fx.popup_icon(WALLET, Icon::Coin, &format!("+{credits}"), AMBER);
            }
            Event::Failed(reason) => {
                let c = match reason {
                    Setback::Village(at) => center(at),
                    Setback::Trapped(id) => token_at(game, id),
                    Setback::Nightfall => vec2(450., 134.),
                };
                fx.ring(c, RED, 64.);
                fx.burst(c, RED, 24);
            }
            Event::Bought { id, cost } => {
                let r = shop_card(id);
                let c = vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
                fx.sparkle(c, GOLD);
                fx.ring(c, GOLD, 64.);
                fx.popup_icon(c - vec2(0., 34.), Icon::Coin, &format!("-{cost}"), AMBER);
            }
            Event::Refused { spot, lack } => {
                let c = spot_at(spot);
                fx.ring(c, RED, 28.);
                fx.shake(0.08);
                fx.popup_icon(c - rise, lack_icon(lack), &lack_word(lack), RED);
            }
        }
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
        let f = Frame::new(self);
        strip(self, &f);
        map(self, &f);
        if f.live {
            inspector(self, &f);
        }
        sidebar(self, &f);
        overlay(self, &f);
    }
    fn hud(&self) -> Hud {
        let mission = &MISSIONS[self.index];
        let (objective, status) = match self.phase {
            // Screen readers still hear the authored briefing and field tip.
            Phase::Briefing => (
                "Deploy the crews",
                format!("{} {}", mission.story, mission.tip),
            ),
            Phase::Debrief => ("Upgrade, then next incident", self.summary()),
            Phase::Setback => ("Retry the incident", self.summary()),
            _ => (
                "Evacuate everyone, put out fires",
                format!(
                    "Turn {}/{} | {} rescued | {} retries",
                    self.turn,
                    mission.limit,
                    self.rescued(),
                    self.retries
                ),
            ),
        };
        Hud {
            stage: mission.name.to_owned(),
            objective: objective.to_owned(),
            status,
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
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.mission() as u32, MISSIONS.len() as u32))
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        effects(self, fx);
    }
    fn draw_objective(&self, area: Rect) -> bool {
        objective(self, area)
    }
    fn draw_result(&self, area: Rect) -> bool {
        result(self, area)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::BASE;

    fn active() -> Game {
        let mut game = Game::new();
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(game.phase, Phase::Active);
        game
    }
    fn fx_after(game: &mut Game, input: Input) -> Juice {
        let cue = game.step(&input);
        let mut fx = Juice::new();
        ArcadeGame::effects(game, cue, &mut fx);
        fx
    }

    #[test]
    fn progress_pips_run_from_the_first_to_the_sixth_incident() {
        let mut game = Game::new();
        assert_eq!(game.progress(), Some((1, 6)));
        for tick in 0..30_000 {
            let input = game.pilot(tick);
            game.step(&input);
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Won);
        assert_eq!(game.progress(), Some((6, 6)));
    }

    #[test]
    fn spraying_a_fire_puffs_steam_where_it_burned() {
        let mut game = active();
        let fire = BASE - 2 * COLS;
        game.tiles[fire].fire = 3;
        game.tiles[fire].fuel = 4;
        game.cursor = fire;
        let fx = fx_after(
            &mut game,
            Input {
                primary: true,
                ..Input::default()
            },
        );
        assert_eq!(game.tiles[fire].fire, 0);
        let counts = fx.counts();
        assert!(counts.sparks >= 20, "{counts:?}");
        assert!(counts.popups >= 1 && counts.rings >= 1, "{counts:?}");
        // Idle ticks request nothing.
        let fx = fx_after(&mut game, Input::default());
        assert!(fx.is_idle());
    }

    #[test]
    fn a_refused_order_rings_red_and_names_the_missing_resource() {
        let mut game = active();
        let fire = BASE - 2 * COLS;
        game.tiles[fire].fire = 2;
        game.tiles[fire].fuel = 4;
        game.cursor = fire;
        game.crews[0].water = 0;
        let fx = fx_after(
            &mut game,
            Input {
                primary: true,
                ..Input::default()
            },
        );
        assert_eq!(game.tiles[fire].fire, 2);
        let counts = fx.counts();
        assert_eq!((counts.popups, counts.rings), (1, 1), "{counts:?}");
        assert_eq!(lack_word(Lack::Water), "Empty");
        assert_eq!(lack_word(Lack::Ap(3)), "Need 3");
    }

    #[test]
    fn ending_a_turn_throws_embers_on_every_new_ignition() {
        let mut game = active();
        let ignitions = game.forecast().iter().filter(|&&w| w).count();
        assert!(ignitions > 0);
        let fx = fx_after(
            &mut game,
            Input {
                tertiary: true,
                ..Input::default()
            },
        );
        let counts = fx.counts();
        assert!(counts.rings >= ignitions, "{counts:?}");
        assert!(counts.sparks >= ignitions * 10, "{counts:?}");
    }

    #[test]
    fn clearing_an_incident_celebrates_and_pays_out_visibly() {
        let mut game = active();
        for tile in game.tiles.iter_mut() {
            tile.fire = 0;
            tile.people = 0;
        }
        game.landed = game.quota() - 2;
        game.crews[0].cargo = 2;
        game.cursor = BASE;
        let fx = fx_after(
            &mut game,
            Input {
                primary: true,
                ..Input::default()
            },
        );
        assert_eq!(game.phase, Phase::Debrief);
        let counts = fx.counts();
        // Delivery popup, callout, medal stars and the credit payout.
        assert!(counts.popups >= 5, "{counts:?}");
        assert!(counts.sparks >= 60, "{counts:?}");
    }

    #[test]
    fn the_header_names_the_incident_and_keeps_the_objective_short() {
        let game = Game::new();
        let hud = game.hud();
        assert_eq!(hud.stage, "Pine Creek");
        assert!(hud.objective.split_whitespace().count() <= 5);
        let hud = active().hud();
        assert!(hud.objective.split_whitespace().count() <= 5);
        assert!(hud.status.starts_with("Turn 1/18"));
        // The authored briefing and tip stay available to status readers.
        assert!(game.hud().status.contains(MISSIONS[0].tip));
        let summary = game.summary();
        assert_eq!(summary.matches(['.', ';']).count(), 2, "{summary}");
        assert!(summary.ends_with('.'));
    }

    #[test]
    fn help_is_four_short_bullets() {
        assert!(crate::META.help.len() <= 4);
        for line in crate::META.help {
            assert!(line.split_whitespace().count() < 15, "{line}");
            assert!(line.is_ascii());
        }
    }
}
