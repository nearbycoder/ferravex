//! Read-only nautical chart and trading ledger presentation.
//!
//! Layout rectangles here mirror the model's pointer hit regions: tabs at
//! `650 + 57 * i` (y 118..149), list rows per page, islands within 29 units
//! of their port, and the two action buttons at y 478 and 514 (x 658..926).
//! The ledger speaks in icons, pips, meters and previews; words are kept to
//! names, numbers and one-word verbs next to their key caps.
use crate::content::{CHARTERS, FORECASTS, GOODS, PORTS};
use crate::model::{
    DAY_TICKS, DEADLINE, DEBT, Event, Game, Loss, REFITS, ROUTE_SPLIT, Refusal, SAIL_TILE_W,
    SAIL_TILE_Y, SAIL_TILES, SUPPLY_CAP, forecast_shift,
};
use ferravex_arcade::shapes::{self, Cap, Fill, Stroke};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;
use std::f32::consts::{PI, TAU};

// ---------------------------------------------------------------------------
// Stack labels: per-frame numbers never allocate.

/// A short label formatted on the stack.
struct Small {
    buf: [u8; 32],
    len: usize,
}

impl Small {
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl std::fmt::Write for Small {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let end = self.len + s.len();
        let slot = self.buf.get_mut(self.len..end).ok_or(std::fmt::Error)?;
        slot.copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

/// `format!` into a [`Small`] stack label.
macro_rules! small {
    ($($arg:tt)*) => {{
        let mut label = Small { buf: [0; 32], len: 0 };
        let _ = std::fmt::Write::write_fmt(&mut label, format_args!($($arg)*));
        label
    }};
}

// ---------------------------------------------------------------------------
// Palette and layout

const DEEP: Color = Color::new(0.025, 0.105, 0.14, 1.);
const FOAM: Color = Color::new(0.62, 0.84, 0.82, 1.);
const SAND: Color = Color::new(0.87, 0.80, 0.62, 1.);
const PARCHMENT: Color = Color::new(0.93, 0.88, 0.72, 1.);
const GOLD: Color = Color::new(0.95, 0.73, 0.35, 1.);
const DARK: Color = Color::new(0.14, 0.21, 0.22, 1.);
const LEAF: Color = Color::new(0.25, 0.42, 0.30, 1.);
const RUST: Color = Color::new(0.71, 0.33, 0.22, 1.);
const SOFT: Color = Color::new(0.42, 0.46, 0.40, 1.);
const TEAL_TEXT: Color = Color::new(0.58, 0.74, 0.72, 1.);
/// Bright warning red and success green for dark backgrounds.
const DANGER: Color = Color::new(0.96, 0.4, 0.33, 1.);
const MINT: Color = Color::new(0.56, 0.87, 0.62, 1.);
/// Translucent plates laid over the sea.
const NIGHT: Color = Color::new(0.015, 0.075, 0.1, 0.86);
/// Coin icons on parchment: a deeper gold that stays legible on paper.
const COIN: Color = Color::new(0.83, 0.57, 0.17, 1.);
const GRAIN: Color = Color::new(0.93, 0.74, 0.32, 1.);
const CLOTH: Color = Color::new(0.47, 0.57, 0.93, 1.);
const SPICE: Color = Color::new(0.89, 0.41, 0.24, 1.);

/// Sidebar content column (matches the model's row and button hit regions).
const LEFT: f32 = 662.;
const RIGHT: f32 = 916.;
const ROW_X: f32 = 658.;
const ROW_W: f32 = 268.;
const PRIMARY_Y: f32 = 478.;
const SECONDARY_Y: f32 = 514.;
const BUTTON_H: f32 = 31.;
/// Title row centre line shared by every ledger page.
const TITLE_Y: f32 = 174.;

/// Ship-status band across the top of the chart: purse, hull, food, hold.
const BAND_Y: f32 = 135.;
const COIN_X: f32 = 44.;
const HULL_X: f32 = 112.;
const FOOD_X: f32 = 236.;
const HOLD_X: f32 = 376.;

/// Where market coin popups rise: beside the prices, clear of the purse.
const SALE_X: f32 = 806.;
const TAB_ICONS: [Icon; 5] = [Icon::Map, Icon::Bag, Icon::Scroll, Icon::Wrench, Icon::Book];
const DIGITS: [&str; 5] = ["1", "2", "3", "4", "5"];
const FORECAST_ICONS: [Icon; 6] = [
    Icon::Sun,
    Icon::Music,
    Icon::Rain,
    Icon::Sprout,
    Icon::Snowflake,
    Icon::Leaf,
];
const REFIT_ICONS: [Icon; 4] = [Icon::Wind, Icon::Crate, Icon::Shield, Icon::Key];
const REFIT_NAMES: [&str; 4] = [
    "Swift canvas",
    "Cargo expansion",
    "Brass plating",
    "Settle debt",
];

fn food_pip(i: u32) -> Vec2 {
    vec2(FOOD_X + 16. + i as f32 * 8., BAND_Y)
}
fn hold_slot(i: u32) -> Vec2 {
    vec2(HOLD_X + 17. + i as f32 * 9., BAND_Y)
}
fn hull_meter() -> Rect {
    Rect::new(HULL_X + 13., BAND_Y - 3.5, 58., 7.)
}
fn tab_rect(i: usize) -> Rect {
    Rect::new(650. + i as f32 * 57., 118., 55., 31.)
}
fn chart_row_y(i: usize) -> f32 {
    194. + i as f32 * 30.
}
fn market_row_y(i: usize) -> f32 {
    206. + i as f32 * 40.
}
fn shipyard_row_y(i: usize) -> f32 {
    199. + i as f32 * 60.
}
fn seal_at(i: usize) -> Vec2 {
    vec2(682. + i as f32 * 44.5, 404.)
}
fn button_center(primary: bool) -> Vec2 {
    let y = if primary { PRIMARY_Y } else { SECONDARY_Y };
    vec2(ROW_X + ROW_W * 0.5, y + BUTTON_H * 0.5)
}
fn good_color(good: usize) -> Color {
    match good {
        0 => GRAIN,
        1 => CLOTH,
        _ => SPICE,
    }
}
fn inside(r: Rect, p: Vec2) -> bool {
    p.x >= r.x && p.x < r.x + r.w && p.y >= r.y && p.y < r.y + r.h
}
/// The ledger row under the pointer, mirroring the model's click rows.
fn row_hover(top: f32, height: f32, count: usize) -> Option<usize> {
    let p = pointer();
    if !(ROW_X..ROW_X + ROW_W).contains(&p.x) || p.y < top {
        return None;
    }
    let i = ((p.y - top) / height) as usize;
    (i < count).then_some(i)
}

// ---------------------------------------------------------------------------
// Typography and small widgets

/// Ink on parchment: crisp, without the drop shadow light text gets on the sea.
fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
/// Text vertically centred on `cy`; returns its width.
fn say(value: &str, x: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color);
    draw_text_styled(value, x, s.centered_baseline(cy), s).width
}
fn say_right(value: &str, right: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color).with_align(TextAlign::Right);
    draw_text_styled(value, right, s.centered_baseline(cy), s).width
}
fn say_center(value: &str, cx: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color).with_align(TextAlign::Center);
    draw_text_styled(value, cx, s.centered_baseline(cy), s).width
}
/// Light text with a shadow for the sea and the header.
fn shout(value: &str, x: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = style(size, color).with_shadow(Color::new(0., 0., 0., 0.5 * color.a));
    draw_text_styled(value, x, s.centered_baseline(cy), s).width
}
/// Coins are always gold, whatever the value's color.
fn glyph_color(glyph: Icon, color: Color) -> Color {
    if glyph == Icon::Coin {
        with_alpha(COIN, color.a)
    } else {
        color
    }
}
/// Icon then value, left-aligned at `x`; returns the width.
fn stat(glyph: Icon, value: &str, x: f32, cy: f32, size: f32, color: Color) -> f32 {
    icon(glyph, x + size * 0.5, cy, size, glyph_color(glyph, color));
    if value.is_empty() {
        return size;
    }
    size * 1.2 + say(value, x + size * 1.2, cy, size * 0.95, color)
}
/// Value ending at `right` with its icon before it; returns the width.
fn stat_right(glyph: Icon, value: &str, right: f32, cy: f32, size: f32, color: Color) -> f32 {
    let w = say_right(value, right, cy, size, color);
    icon(
        glyph,
        right - w - size * 0.65,
        cy,
        size * 0.9,
        glyph_color(glyph, color),
    );
    w + size * 1.15
}

fn row(y: f32, height: f32, selected: bool, hover: bool) {
    let (top, h) = (y + 1., height - 3.);
    if selected {
        rounded(ROW_X, top, ROW_W, h, 8., lighten(PARCHMENT, 0.45));
        outline(ROW_X, top, ROW_W, h, 8., 1., with_alpha(RUST, 0.55));
        shapes::fill_capsule(
            vec2(ROW_X + 4.5, top + 7.),
            vec2(ROW_X + 4.5, top + h - 7.),
            1.5,
            RUST,
        );
    } else if hover {
        rounded(ROW_X, top - 1., ROW_W, h, 8., lighten(PARCHMENT, 0.28));
        outline(ROW_X, top - 1., ROW_W, h, 8., 1., with_alpha(DARK, 0.2));
    } else {
        rounded(ROW_X, top, ROW_W, h, 8., darken(PARCHMENT, 0.045));
    }
}

/// A meter for dark backgrounds: a faint light track keeps the empty part
/// visible, then the shared rounded fill.
fn gauge(x: f32, y: f32, w: f32, h: f32, fraction: f32, color: Color) {
    rounded(x, y, w, h, h * 0.5, with_alpha(PAPER, 0.13));
    if fraction > 0. {
        meter(x, y, w, h, fraction, color);
    }
}

/// Lightweight parchment progress bar: recessed track and a rounded fill.
fn bar(x: f32, y: f32, w: f32, h: f32, fraction: f32, color: Color) {
    let f = if fraction.is_finite() {
        fraction.clamp(0., 1.)
    } else {
        0.
    };
    let r = h * 0.5;
    rounded(x, y, w, h, r, with_alpha(DARK, 0.16));
    if f > 0. {
        rounded(x, y, (w * f).max(h), h, r, color);
    }
}

#[derive(Clone, Copy)]
struct Button<'a> {
    key: &'a str,
    glyph: Option<Icon>,
    action: &'a str,
    primary: bool,
    enabled: bool,
}

/// Painted action button inside the model's click rectangle, with an
/// optional right-hand preview of what pressing it changes.
fn button(y: f32, b: Button, preview: Option<(Icon, &str, Color)>) {
    let h = BUTTON_H;
    let hover = b.enabled && inside(Rect::new(ROW_X, y, ROW_W, h), pointer());
    let a = if b.enabled { 1. } else { 0.42 };
    let lift = if hover { 1. } else { 0. };
    let y = y - lift;
    if b.primary {
        let top = lighten(DARK, if hover { 0.2 } else { 0.1 });
        rounded_gradient(
            ROW_X,
            y,
            ROW_W,
            h,
            9.,
            with_alpha(top, a),
            with_alpha(darken(DARK, 0.1), a),
        );
    } else {
        rounded(
            ROW_X,
            y,
            ROW_W,
            h,
            9.,
            with_alpha(DARK, if hover { 0.18 } else { 0.11 } * a),
        );
    }
    if hover {
        outline(ROW_X, y, ROW_W, h, 9., 1.5, with_alpha(GOLD, 0.8));
    }
    let cy = y + h * 0.5;
    let mut x = ROW_X + 8.;
    if !b.key.is_empty() {
        x += keycap(b.key, "", x, cy - 10., with_alpha(MUTED, a)) + 9.;
    }
    let ink = with_alpha(if b.primary { PARCHMENT } else { DARK }, a);
    if let Some(glyph) = b.glyph {
        icon(glyph, x + 8., cy, 16., glyph_color(glyph, ink));
        x += 22.;
    }
    say(b.action, x, cy, 14., ink);
    if let Some((glyph, value, color)) = preview {
        stat_right(
            glyph,
            value,
            ROW_X + ROW_W - 12.,
            cy,
            14.,
            with_alpha(color, a),
        );
    }
}

/// Icon and name heading a ledger page; `room` keeps it clear of the right
/// column. Proper names only.
fn title_row(glyph: Icon, name: &str, color: Color, room: f32) {
    icon(glyph, 671., TITLE_Y, 18., color);
    let s = ink(18., color);
    let w = text_width(name, s);
    let s = if w > room {
        s.with_size(18. * room / w)
    } else {
        s
    };
    draw_text_styled(name, 686., s.centered_baseline(TITLE_Y), s);
}

/// The purse at the right of a title row.
fn purse(game: &Game) {
    stat_right(
        Icon::Coin,
        small!("{}", game.coins).as_str(),
        RIGHT + 6.,
        TITLE_Y,
        16.,
        darken(GOLD, 0.25),
    );
}

/// Bespoke trade-good pictograms (grain sack, cloth bolt, spice jar) in a
/// `size` box centred on `c`, so goods read by shape as well as color.
fn good_glyph(good: usize, c: Vec2, size: f32, alpha: f32) {
    let s = size;
    let tone = good_color(good);
    let base = with_alpha(tone, alpha);
    let dark = with_alpha(darken(tone, 0.45), alpha);
    let light = with_alpha(lighten(tone, 0.42), alpha);
    let shade = Color::new(0., 0., 0., 0.25 * alpha);
    match good {
        0 => {
            // Sack: round belly, gathered neck, cord and a wheat tuft.
            shapes::fill_circle(c + vec2(0.6, s * 0.15 + 1.), s * 0.33, shade);
            shapes::fill_circle(c + vec2(0., s * 0.15), s * 0.33, base);
            shapes::fill_rounded_rect(
                Rect::new(c.x - s * 0.17, c.y - s * 0.2, s * 0.34, s * 0.28),
                s * 0.08,
                base,
            );
            for k in [-1., 0., 1.] {
                let root = c + vec2(k * s * 0.07, -s * 0.2);
                shapes::fill_capsule(root, root + vec2(k * s * 0.11, -s * 0.2), s * 0.05, light);
            }
            shapes::fill_capsule(
                c + vec2(-s * 0.19, -s * 0.07),
                c + vec2(s * 0.19, -s * 0.07),
                s * 0.055,
                dark,
            );
            shapes::fill_circle(
                c + vec2(-s * 0.12, s * 0.1),
                s * 0.07,
                with_alpha(light, 0.7 * alpha),
            );
        }
        1 => {
            // Bolt of cloth: a rolled body, a loose tail and a spiral end.
            let body = Rect::new(c.x - s * 0.4, c.y - s * 0.2, s * 0.62, s * 0.4);
            shapes::fill_rounded_rect(
                Rect::new(body.x + 0.6, body.y + 1., body.w, body.h),
                s * 0.06,
                shade,
            );
            shapes::fill_polygon(
                &[
                    c + vec2(-s * 0.36, s * 0.12),
                    c + vec2(-s * 0.08, s * 0.12),
                    c + vec2(-s * 0.16, s * 0.42),
                    c + vec2(-s * 0.42, s * 0.36),
                ],
                dark,
            );
            shapes::fill_rounded_rect(body, s * 0.06, base);
            for k in [0.3, 0.62] {
                let x = body.x + body.w * k;
                shapes::fill_rounded_rect(
                    Rect::new(x, body.y, (s * 0.05).max(1.), body.h),
                    0.,
                    with_alpha(light, 0.7 * alpha),
                );
            }
            let end = c + vec2(s * 0.22, 0.);
            shapes::fill_circle(end, s * 0.21, light);
            shapes::stroke_circle(end, s * 0.11, (s * 0.05).max(1.), dark);
            shapes::fill_circle(end, s * 0.04, dark);
        }
        _ => {
            // Spice jar: body, label band, lid and knob.
            let body = Rect::new(c.x - s * 0.26, c.y - s * 0.13, s * 0.52, s * 0.52);
            shapes::fill_rounded_rect(
                Rect::new(body.x + 0.6, body.y + 1., body.w, body.h),
                s * 0.13,
                shade,
            );
            shapes::fill_rounded_rect(body, s * 0.13, base);
            shapes::fill_rounded_rect(
                Rect::new(body.x, c.y + s * 0.03, body.w, s * 0.13),
                0.,
                light,
            );
            shapes::fill_rounded_rect(
                Rect::new(c.x - s * 0.31, c.y - s * 0.3, s * 0.62, s * 0.17),
                s * 0.05,
                dark,
            );
            shapes::fill_circle(c + vec2(0., -s * 0.35), s * 0.07, dark);
        }
    }
}

/// A small sailing boat with full or reefed canvas.
fn sail_glyph(c: Vec2, s: f32, reefed: bool, color: Color) {
    shapes::fill_polygon(
        &[
            c + vec2(-s * 0.44, s * 0.2),
            c + vec2(s * 0.44, s * 0.2),
            c + vec2(s * 0.3, s * 0.4),
            c + vec2(-s * 0.3, s * 0.4),
        ],
        color,
    );
    let top = c + vec2(-s * 0.04, -s * 0.46);
    let foot = c + vec2(-s * 0.04, s * 0.2);
    shapes::fill_capsule(top, foot, (s * 0.035).max(0.8), color);
    if reefed {
        // A scrap of storm canvas and the furled sail lashed to the boom.
        shapes::fill_triangle(
            top + vec2(s * 0.05, s * 0.05),
            top + vec2(s * 0.05, s * 0.32),
            top + vec2(s * 0.27, s * 0.32),
            color,
        );
        shapes::fill_capsule(
            c + vec2(0.02 * s, s * 0.1),
            c + vec2(s * 0.38, s * 0.1),
            s * 0.065,
            color,
        );
    } else {
        shapes::fill_triangle(
            top + vec2(s * 0.05, s * 0.02),
            foot + vec2(s * 0.05, -s * 0.08),
            c + vec2(s * 0.42, s * 0.12),
            color,
        );
        shapes::fill_triangle(
            top + vec2(-s * 0.05, s * 0.12),
            foot + vec2(-s * 0.05, -s * 0.08),
            c + vec2(-s * 0.32, s * 0.12),
            with_alpha(color, color.a * 0.7),
        );
    }
}

/// A tiny island for ledger rows, tying each name to the chart: shallow
/// water, a beach and a green crown (grey until the port is visited).
fn islet(c: Vec2, visited: bool) {
    let water = Color::new(0.16, 0.38, 0.4, 1.);
    shapes::fill_sector(c + vec2(0., 3.), 7.5, PI, PI, darken(SAND, 0.08));
    shapes::fill_circle(
        c + vec2(-1., -0.5),
        4.,
        if visited {
            LEAF
        } else {
            with_alpha(SOFT, 0.75)
        },
    );
    shapes::fill_capsule(c + vec2(-8.5, 4.5), c + vec2(8.5, 4.5), 1.6, water);
}

/// A small check or cross badge that marks a met or missing requirement.
fn tick_mark(c: Vec2, met: bool) {
    icon_badge(
        if met { Icon::Check } else { Icon::Cross },
        c.x,
        c.y,
        13.,
        if met { LEAF } else { RUST },
        PARCHMENT,
    );
}

/// Wax seal for one charter: filled and checked once delivered.
fn seal(c: Vec2, complete: bool, active: bool, t: f32) {
    shapes::fill_circle(c + vec2(0., 2.), 16., Color::new(0.3, 0.2, 0.1, 0.18));
    if complete {
        shapes::fill_circle(
            c,
            15.,
            Fill::radial(
                c - vec2(4., 5.),
                21.,
                lighten(LEAF, 0.25),
                darken(LEAF, 0.15),
            ),
        );
        shapes::stroke_circle(c, 11., 1., with_alpha(PARCHMENT, 0.4));
        let tick = [c + vec2(-5., 0.), c + vec2(-1.5, 4.), c + vec2(5.5, -4.)];
        shapes::polyline(&tick, Stroke::new(2.2, PARCHMENT).cap(Cap::Round));
    } else if active {
        let k = pulse(t, 0.9);
        glow(c.x, c.y, 20. + 4. * k, with_alpha(RUST, 0.16 + 0.12 * k));
        shapes::fill_circle(c, 15., lighten(PARCHMENT, 0.5));
        shapes::stroke_circle(c, 15., 2., RUST);
        icon(Icon::Scroll, c.x, c.y, 15., RUST);
    } else {
        shapes::fill_circle(c, 15., darken(PARCHMENT, 0.06));
        shapes::stroke_circle(c, 15., 1., with_alpha(DARK, 0.25));
        icon(Icon::Lock, c.x, c.y, 12., with_alpha(SOFT, 0.7));
    }
}

fn seals(game: &Game, t: f32) {
    let a = seal_at(0);
    let b = seal_at(5);
    line(a, b, 1.5, with_alpha(RUST, 0.35));
    for i in 0..CHARTERS.len() {
        seal(seal_at(i), i < game.charter, i == game.charter, t);
    }
}

// ---------------------------------------------------------------------------
// Chart art

fn line(a: Vec2, b: Vec2, width: f32, color: Color) {
    draw_line(a.x, a.y, b.x, b.y, width, color);
}

fn route_point(from: usize, to: usize, sheltered: bool, t: f32) -> Vec2 {
    let a = Game::position(from);
    let b = Game::position(to);
    let d = (b - a).normalize_or_zero();
    let side = vec2(-d.y, d.x);
    a.lerp(b, t) + side * if sheltered { (t * PI).sin() * 34. } else { 0. }
}

/// Dashed course line whose dashes march toward the destination.
fn course(from: usize, to: usize, sheltered: bool, color: Color, march: f32) {
    let dashes = 28;
    for i in 0..dashes {
        let s = (i as f32 + march.fract()) / dashes as f32;
        let e = s + 0.45 / dashes as f32;
        if e <= 1. {
            line(
                route_point(from, to, sheltered, s),
                route_point(from, to, sheltered, e),
                2.,
                color,
            );
        }
    }
}

fn distance_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0., 1.);
    p.distance(a + ab * t)
}

/// Whether today's storm rings lie across the open-sea course to `to`.
fn crosses_storm(game: &Game, to: usize) -> bool {
    let a = Game::position(game.harbor);
    let b = Game::position(to);
    game.storms()
        .iter()
        .any(|(c, r)| distance_to_segment(*c, a, b) < *r)
}

/// How close the ship is to a storm on an open passage: 1 inside a ring,
/// fading to 0 thirty units outside it.
fn storm_threat(game: &Game) -> f32 {
    if game.passage.is_none_or(|p| p.sheltered) {
        return 0.;
    }
    let pos = game.boat_position();
    game.storms()
        .iter()
        .map(|(c, r)| (1. - (pos.distance(*c) - r) / 30.).clamp(0., 1.))
        .fold(0., f32::max)
}

/// The island under the pointer while in port (same 29-unit test as the model).
fn hovered_port(game: &Game) -> Option<usize> {
    let p = pointer();
    if game.passage.is_some() || !(p.x < 625. && p.y > 155. && p.y < 550.) {
        return None;
    }
    (0..PORTS.len()).find(|&port| p.distance(Game::position(port)) < 29.)
}

/// Travel preview chip: days, and a shield (sheltered) or squall (storms on
/// the open course). Red when the pantry cannot cover the passage.
fn route_chip(game: &Game, to: usize, at: Vec2, alpha: f32) {
    let days = game.voyage_days(to);
    let short = days > game.provisions;
    let safety = if game.sheltered {
        Some((Icon::Shield, MINT))
    } else if crosses_storm(game, to) {
        Some((Icon::Rain, DANGER))
    } else {
        None
    };
    let label = small!("{days}");
    let tint = if short { DANGER } else { GOLD };
    let s = style(
        14.,
        with_alpha(if short { DANGER } else { PARCHMENT }, alpha),
    );
    let tw = text_width(label.as_str(), s);
    let w = 30. + tw + if safety.is_some() { 22. } else { 8. } + if short { 18. } else { 0. };
    let h = 22.;
    let (x, y) = (at.x - w * 0.5, at.y - h * 0.5);
    rounded(x, y, w, h, 11., with_alpha(NIGHT, NIGHT.a * alpha));
    outline(x, y, w, h, 11., 1., with_alpha(tint, 0.6 * alpha));
    icon(Icon::Hourglass, x + 14., at.y, 13., with_alpha(tint, alpha));
    draw_text_styled(label.as_str(), x + 25., s.centered_baseline(at.y), s);
    let mut right = x + w - 4.;
    if let Some((glyph, color)) = safety {
        icon(glyph, right - 10., at.y, 14., with_alpha(color, alpha));
        right -= 20.;
    }
    if short {
        icon(Icon::Pot, right - 8., at.y, 13., with_alpha(DANGER, alpha));
    }
}

fn island(port: usize, chosen: bool, destination: bool, hover: bool, t: f32) {
    let p = Game::position(port);
    if hover {
        glow(p.x, p.y, 58., with_alpha(FOAM, 0.22));
    }
    // Soft two-step shadow, a shallow-water shelf with a foam line, then land.
    draw_ellipse(
        p.x + 4.,
        p.y + 8.,
        43.,
        26.,
        -17.,
        Color::new(0.0, 0.05, 0.07, 0.42),
    );
    let shelf = 0.5 + 0.5 * (t * 1.3 + port as f32).sin();
    draw_ellipse(p.x, p.y, 41., 29., -17., Color::new(0.16, 0.38, 0.40, 1.));
    draw_ellipse(
        p.x,
        p.y - 1.,
        35. + shelf,
        23. + shelf * 0.7,
        -17.,
        with_alpha(FOAM, 0.45 + 0.15 * shelf),
    );
    draw_ellipse(p.x, p.y - 2., 33., 21., -17., SAND);
    draw_ellipse(p.x - 1., p.y - 4., 26., 15.5, -17., LEAF);
    draw_ellipse(p.x - 5., p.y - 8., 16., 8., -17., lighten(LEAF, 0.14));
    // Pier.
    panel(
        p.x - 3.,
        p.y + 7.,
        6.,
        18.,
        Color::new(0.47, 0.31, 0.22, 1.),
    );
    panel(
        p.x - 3.,
        p.y + 7.,
        2.,
        18.,
        Color::new(0.58, 0.40, 0.28, 1.),
    );
    // Houses with shaded walls and pitched roofs.
    let roof = if port.is_multiple_of(2) { RUST } else { DARK };
    for house in 0..3 {
        let x = p.x - 17. + house as f32 * 12.;
        let y = p.y - 3. - (house % 2) as f32 * 5.;
        panel(x + 1., y + 1., 9., 9., Color::new(0.1, 0.16, 0.1, 0.35));
        panel(x, y, 9., 9., PARCHMENT);
        panel(x + 6., y, 3., 9., darken(PARCHMENT, 0.12));
        draw_triangle(
            vec2(x - 2., y),
            vec2(x + 4.5, y - 7.),
            vec2(x + 11., y),
            roof,
        );
        draw_triangle(
            vec2(x + 4.5, y - 7.),
            vec2(x + 11., y),
            vec2(x + 6.5, y),
            darken(roof, 0.2),
        );
        panel(x + 3., y + 4., 3., 5., DARK);
    }
    if port == 1 || port == 5 {
        // Lighthouse with a breathing lamp halo.
        panel(p.x + 18., p.y - 27., 6., 24., PARCHMENT);
        panel(p.x + 18., p.y - 19., 6., 4., RUST);
        panel(p.x + 17., p.y - 27., 8., 5., GOLD);
        draw_triangle(
            vec2(p.x + 15., p.y - 27.),
            vec2(p.x + 21., p.y - 33.),
            vec2(p.x + 27., p.y - 27.),
            RUST,
        );
        let lamp = pulse(t + port as f32 * 0.3, 0.5);
        glow(
            p.x + 21.,
            p.y - 25.,
            9. + 5. * lamp,
            Color::new(1., 0.85, 0.45, 0.22 + 0.2 * lamp),
        );
    }
    if chosen {
        let k = pulse(t, 0.7);
        shapes::stroke_circle(p, 42. + k * 1.5, 2., GOLD);
        shapes::stroke_circle(p, 47. + k * 2., 1., with_alpha(GOLD, 0.2 + 0.2 * k));
    } else if hover {
        shapes::stroke_circle(p, 42., 1.5, with_alpha(FOAM, 0.75));
    }
    if destination {
        // Charter pennant, bobbing with the swell.
        let tip = vec2(p.x, p.y - 35. + bob(t, 0.6, 2.5));
        shapes::fill_capsule(tip, tip + vec2(0., -16.), 1., DARK);
        draw_triangle(
            tip + vec2(0., -16.),
            tip + vec2(13., -12. + bob(t + 0.3, 1.1, 1.)),
            tip + vec2(0., -8.),
            GOLD,
        );
    }
}

/// Name plate; drawn after the ship so names stay legible over the wake.
fn island_label(port: usize, visited: bool, destination: bool) {
    let p = Game::position(port);
    let name = PORTS[port].name;
    let size = 15.;
    let width = measure(name, size) + if destination { 14. } else { 0. };
    let center = if p.y > 450. {
        p + vec2(width * 0.5 + 44., 0.)
    } else {
        p + vec2(0., 42.)
    };
    let (x, y, w, h) = (
        center.x - width * 0.5 - 8.,
        center.y - 10.,
        width + 16.,
        20.,
    );
    rounded(x, y, w, h, 10., Color::new(0.02, 0.1, 0.13, 0.84));
    if destination {
        outline(x, y, w, h, 10., 1., with_alpha(GOLD, 0.75));
        icon(Icon::Scroll, x + 13., center.y, 12., GOLD);
    } else if !visited {
        outline(x, y, w, h, 10., 1., with_alpha(FOAM, 0.18));
    }
    let s = TextStyle::modern(size, if visited { PARCHMENT } else { TEAL_TEXT });
    let left = center.x - width * 0.5 + if destination { 14. } else { 0. };
    draw_text_styled(name, left, s.centered_baseline(center.y), s);
}

fn boat(position: Vec2, heading: Vec2, reefed: bool, t: f32) {
    let forward = if heading.length_squared() < 0.01 {
        vec2(0.3, -1.).normalize()
    } else {
        heading.normalize()
    };
    let side = vec2(-forward.y, forward.x);
    let position = position + side * bob(t, 0.9, 0.8);
    // Fading wake on both quarters.
    for k in 0..3 {
        let a = 12. + k as f32 * 6.;
        let b = a + 6.;
        let alpha = 0.55 - k as f32 * 0.17;
        for s in [-1., 1.] {
            line(
                position - forward * a + side * s * (3. + k as f32 * 2.),
                position - forward * b + side * s * (5. + k as f32 * 2.),
                1.6,
                with_alpha(FOAM, alpha),
            );
        }
    }
    // Shadow, hull, deck, mast, sails and pennant.
    let drop = vec2(2.5, 3.);
    draw_triangle(
        position + forward * 19. + drop,
        position - forward * 12. - side * 9. + drop,
        position - forward * 12. + side * 9. + drop,
        Color::new(0., 0.04, 0.06, 0.4),
    );
    draw_triangle(
        position + forward * 19.,
        position - forward * 12. - side * 9.,
        position - forward * 12. + side * 9.,
        DARK,
    );
    draw_triangle(
        position + forward * 15.,
        position - forward * 8. - side * 6.,
        position - forward * 8. + side * 6.,
        RUST,
    );
    draw_triangle(
        position + forward * 15.,
        position - forward * 8. + side * 6.,
        position - forward * 6.,
        darken(RUST, 0.2),
    );
    line(position - forward * 10., position + forward * 11., 2., GOLD);
    let reach = if reefed { 7. } else { 14. + bob(t, 0.45, 0.8) };
    draw_triangle(
        position + forward * 10.,
        position + side * reach - forward * 5.,
        position - forward * 7.,
        PARCHMENT,
    );
    draw_triangle(
        position + forward * 9.,
        position - side * (reach - 3.) - forward * 2.,
        position - forward * 5.,
        SAND,
    );
    let flutter = bob(t, 2.3, 1.5);
    draw_triangle(
        position + forward * 13.,
        position + forward * 18. + side * (7. + flutter),
        position + forward * 9. + side * 6.,
        GOLD,
    );
}

fn compass(game: &Game) {
    let c = vec2(584., 152.);
    shapes::fill_circle(c, 22., Color::new(0.02, 0.1, 0.13, 0.55));
    draw_circle_lines(c.x, c.y, 22., 1., with_alpha(SAND, 0.55));
    draw_circle_lines(c.x, c.y, 17., 1., with_alpha(SAND, 0.18));
    // Four-point rose, light and dark halves.
    for k in 0..4 {
        let d = vec2((k as f32 * PI * 0.5).cos(), (k as f32 * PI * 0.5).sin());
        let s = vec2(-d.y, d.x);
        draw_triangle(c + d * 19., c + s * 4., c, with_alpha(SAND, 0.75));
        draw_triangle(c + d * 19., c - s * 4., c, with_alpha(SAND, 0.4));
    }
    // This week's wind, with a gentle gusting wobble.
    let gust = 1. + 0.06 * (clock() * 2.1).sin();
    let wind = game.wind().normalize() * 17. * gust;
    let tail = c - wind * 0.7;
    let head = c + wind;
    shapes::fill_capsule(tail, c + wind * 0.55, 1.4, GOLD);
    let side = vec2(-wind.y, wind.x).normalize() * 5.;
    draw_triangle(head, c + wind * 0.45 + side, c + wind * 0.45 - side, GOLD);
    say_center("N", c.x, c.y - 29., 10., SAND);
    icon(Icon::Wind, c.x, c.y + 33., 14., with_alpha(SAND, 0.85));
}

fn storm(center: Vec2, radius: f32, t: f32, danger: f32) {
    draw_circle(center.x, center.y, radius, Color::new(0.14, 0.26, 0.3, 0.5));
    draw_circle(
        center.x,
        center.y,
        radius * 0.72,
        Color::new(0.2, 0.31, 0.35, 0.45),
    );
    let spin = t * 0.5;
    for k in 0..3 {
        let start = spin + k as f32 * TAU / 3.;
        shapes::stroke_arc(
            center,
            radius - 7.,
            start,
            1.3,
            Stroke::new(2., Color::new(0.62, 0.74, 0.74, 0.42)).cap(Cap::Round),
        );
        shapes::stroke_arc(
            center,
            radius * 0.45,
            -spin * 1.4 + k as f32 * TAU / 3.,
            0.9,
            Stroke::new(1.5, Color::new(0.62, 0.74, 0.74, 0.3)).cap(Cap::Round),
        );
    }
    draw_circle_lines(
        center.x,
        center.y,
        radius,
        1.,
        Color::new(0.45, 0.58, 0.6, 0.55),
    );
    if danger > 0. {
        let k = pulse(t, 2.);
        shapes::stroke_circle(
            center,
            radius,
            2. + 2. * k,
            with_alpha(DANGER, danger * (0.5 + 0.4 * k)),
        );
    }
    // A squall cloud at the eye; it flickers when the ship is caught.
    let flash = if danger >= 1. {
        0.25 * pulse(t, 3.)
    } else {
        0.
    };
    icon(
        Icon::Rain,
        center.x,
        center.y + bob(t, 0.4, 1.5),
        22.,
        Color::new(0.82 + flash, 0.9, 0.88, 0.85),
    );
}

fn draw_chart(game: &Game) {
    let t = clock();
    rounded_gradient(
        16.,
        112.,
        612.,
        456.,
        10.,
        Color::new(0.07, 0.22, 0.26, 1.),
        DEEP,
    );
    // Neatline and graticule.
    let neat = with_alpha(SAND, 0.16);
    draw_rectangle_lines(24., 120., 596., 440., 1., neat);
    let grid = Color::new(0.12, 0.3, 0.33, 0.45);
    for x in 0..13 {
        let x = 38. + x as f32 * 46.;
        draw_line(x, 162., x, 545., 1., grid);
    }
    for y in 0..9 {
        let y = 189. + y as f32 * 46.;
        draw_line(30., y, 615., y, 1., grid);
    }
    // Swell marks drift; they hurry along while underway.
    let speed = if game.passage.is_some() { 1.6 } else { 0.8 };
    for i in 0..48 {
        let phase = t * speed + i as f32 * 1.7;
        let drift = phase.sin() * 3.;
        let x = 43. + ((i * 137) % 548) as f32 + drift;
        let y = 168. + ((i * 79) % 372) as f32;
        let c = Color::new(
            0.2,
            0.42,
            0.45,
            0.35 + 0.25 * (0.5 + 0.5 * (phase * 0.7).cos()),
        );
        line(vec2(x - 5., y), vec2(x, y + 2.), 1., c);
        line(vec2(x, y + 2.), vec2(x + 5., y), 1., c);
    }
    compass(game);
    let threat = storm_threat(game);
    let pos = game.boat_position();
    for (center, radius) in game.storms() {
        let danger = if threat > 0. && pos.distance(center) < radius + 30. {
            threat
        } else {
            0.
        };
        storm(center, radius, t, danger);
    }
    let hover = hovered_port(game);
    let (from, to, sheltered) = game
        .passage
        .map_or((game.harbor, game.selected_port, game.sheltered), |p| {
            (p.from, p.to, p.sheltered)
        });
    if from != to {
        let march = t * if game.passage.is_some() { 1.2 } else { 0.5 };
        course(from, to, sheltered, with_alpha(GOLD, 0.85), march);
    }
    if let Some(h) = hover.filter(|&h| h != game.harbor && h != game.selected_port) {
        course(
            game.harbor,
            h,
            game.sheltered,
            with_alpha(FOAM, 0.5),
            t * 0.5,
        );
    }
    let destination = CHARTERS.get(game.charter).map(|c| c.destination);
    for port in 0..PORTS.len() {
        island(
            port,
            port == game.selected_port && game.passage.is_none(),
            Some(port) == destination,
            hover == Some(port),
            t,
        );
    }
    let heading = game.passage.map_or(vec2(0.3, -1.), |p| {
        Game::position(p.to) - Game::position(p.from)
    });
    if threat > 0. {
        let k = pulse(t, 2.2);
        glow(pos.x, pos.y, 26. + 6. * k, with_alpha(DANGER, 0.3 * threat));
    }
    boat(pos, heading, game.passage.is_some_and(|p| p.reefed), t);
    for port in 0..PORTS.len() {
        island_label(port, game.visited[port], Some(port) == destination);
    }
    // Travel previews: the chosen course, and the island under the pointer.
    if game.passage.is_none() {
        if game.selected_port != game.harbor {
            let mid = route_point(game.harbor, game.selected_port, game.sheltered, 0.5);
            route_chip(game, game.selected_port, mid, 1.);
        }
        if let Some(h) = hover.filter(|&h| h != game.harbor && h != game.selected_port) {
            let mid = route_point(game.harbor, h, game.sheltered, 0.5);
            route_chip(game, h, mid, 0.85);
        }
    }
    ship_band(game, t);
}

/// Food the current plan will eat: the passage underway, or the selected
/// course while planning on the chart.
fn food_needed(game: &Game) -> u32 {
    match game.passage {
        Some(p) => p.total.saturating_sub(p.traveled).div_ceil(DAY_TICKS * 100),
        None if game.page == 0 => game.voyage_days(game.selected_port),
        None => 0,
    }
}

fn hull_color(hull: u32) -> Color {
    if hull < 35 {
        DANGER
    } else if hull < 65 {
        GOLD
    } else {
        MINT
    }
}

/// The ship's state at a glance: purse, hull, pantry pips and hold slots.
fn ship_band(game: &Game, t: f32) {
    let cap = game.capacity();
    let right = hold_slot(cap - 1).x + 13.;
    rounded(28., 121., right - 28., 28., 14., NIGHT);
    outline(28., 121., right - 28., 28., 14., 1., with_alpha(SAND, 0.16));
    for x in [HULL_X - 15., FOOD_X - 17., HOLD_X - 17.] {
        line(
            vec2(x, BAND_Y - 8.),
            vec2(x, BAND_Y + 8.),
            1.,
            with_alpha(SAND, 0.16),
        );
    }
    // Purse.
    icon(Icon::Coin, COIN_X, BAND_Y, 16., GOLD);
    shout(
        small!("{}", game.coins).as_str(),
        COIN_X + 12.,
        BAND_Y,
        15.,
        PARCHMENT,
    );
    // Hull.
    let tint = hull_color(game.hull);
    let danger = game.hull < 35;
    let k = if danger { pulse(t, 1.6) } else { 0. };
    icon(Icon::Ship, HULL_X, BAND_Y, 17., lighten(tint, 0.2 * k));
    let m = hull_meter();
    gauge(m.x, m.y, m.w, m.h, game.hull as f32 / 100., tint);
    say(
        small!("{}", game.hull).as_str(),
        m.x + m.w + 5.,
        BAND_Y,
        12.,
        with_alpha(PARCHMENT, 0.85),
    );
    // Pantry: one pip per day of food, with the planned passage previewed.
    let need = food_needed(game);
    let food = game.provisions;
    let short = need > food;
    let blink = pulse(t, 1.4);
    icon(
        Icon::Pot,
        FOOD_X,
        BAND_Y,
        16.,
        if short || food == 0 {
            lighten(DANGER, 0.2 * blink)
        } else {
            SAND
        },
    );
    for i in 0..SUPPLY_CAP {
        let at = food_pip(i);
        if i < food {
            let eaten = i >= food.saturating_sub(need);
            let color = if eaten {
                with_alpha(GOLD, 0.35 + 0.35 * blink)
            } else {
                GOLD
            };
            shapes::fill_circle(at, 3., color);
        } else if i < need {
            shapes::stroke_circle(at, 2.8, 1.4, with_alpha(DANGER, 0.6 + 0.4 * blink));
        } else {
            shapes::stroke_circle(at, 2.6, 1., with_alpha(SAND, 0.28));
        }
    }
    say(
        small!("{food}").as_str(),
        food_pip(SUPPLY_CAP - 1).x + 8.,
        BAND_Y,
        12.,
        with_alpha(if short { DANGER } else { PARCHMENT }, 0.85),
    );
    // Hold: one slot per crate, colored by its good.
    icon(Icon::Crate, HOLD_X, BAND_Y, 16., SAND);
    let mut slot = 0;
    for good in 0..GOODS.len() {
        for _ in 0..game.cargo(good) {
            let at = hold_slot(slot);
            rounded(at.x - 3.5, at.y - 7., 7., 14., 2., good_color(good));
            rounded(
                at.x - 3.5,
                at.y - 7.,
                7.,
                3.,
                1.5,
                lighten(good_color(good), 0.35),
            );
            slot += 1;
        }
    }
    for i in slot..cap {
        let at = hold_slot(i);
        outline(
            at.x - 3.5,
            at.y - 7.,
            7.,
            14.,
            2.,
            1.,
            with_alpha(SAND, 0.3),
        );
    }
}

// ---------------------------------------------------------------------------
// Ledger

/// The tab that holds the next useful action (presentation only; the rules
/// never consult it): buy charter goods, sail, deliver or settle.
fn next_step(game: &Game) -> Option<usize> {
    if game.passage.is_some() {
        return None;
    }
    Some(match CHARTERS.get(game.charter) {
        Some(job) => {
            let loaded = game.cargo(job.good) >= job.amount;
            if loaded && game.harbor == job.destination {
                2
            } else if loaded {
                0
            } else {
                1
            }
        }
        None if game.harbor != 0 => 0,
        None if game.coins >= DEBT => 3,
        None => 1,
    })
}

fn sidebar_frame(game: &Game, t: f32) {
    card(642., 112., 302., 456., PARCHMENT);
    // Segmented icon tabs over the model's tab hit regions.
    rounded(648., 117., 290., 33., 10., darken(PARCHMENT, 0.1));
    let underway = game.passage.is_some();
    let next = next_step(game);
    for (i, glyph) in TAB_ICONS.iter().enumerate() {
        let r = tab_rect(i);
        let active = game.page == i && !underway;
        let hover = !underway && !active && inside(r, pointer());
        if active {
            rounded(r.x + 1., 119., 54., 29., 8., DARK);
        } else if hover {
            rounded(r.x + 1., 119., 54., 29., 8., with_alpha(DARK, 0.13));
        }
        let ink = if active {
            PARCHMENT
        } else if underway {
            with_alpha(DARK, 0.28)
        } else {
            DARK
        };
        let lift = if hover { 1.5 } else { 0. };
        let digit = TextStyle::modern(10., with_alpha(if active { GOLD } else { SOFT }, ink.a));
        draw_text_styled(DIGITS[i], r.x + 10., digit.centered_baseline(133.5), digit);
        icon(*glyph, r.x + 32., 133.5 - lift, 19., ink);
        if next == Some(i) && !active {
            let k = pulse(t, 1.2);
            glow(
                r.x + 47.,
                124.,
                8. + 4. * k,
                with_alpha(RUST, 0.25 + 0.25 * k),
            );
            shapes::fill_circle(vec2(r.x + 47., 124.), 3.5, RUST);
        }
    }
}

fn chart_page(game: &Game) {
    title_row(Icon::Anchor, PORTS[game.harbor].name, DARK, 150.);
    wind_badge(game, vec2(RIGHT - 6., TITLE_Y));
    let destination = CHARTERS.get(game.charter).map(|c| c.destination);
    let hover = row_hover(194., 30., PORTS.len());
    for (i, port) in PORTS.iter().enumerate() {
        let y = chart_row_y(i);
        row(y, 30., game.selected_port == i, hover == Some(i));
        let mid = y + 14.5;
        islet(vec2(675., mid), game.visited[i]);
        say(
            port.name,
            690.,
            mid,
            15.,
            if game.visited[i] { DARK } else { SOFT },
        );
        let mut right = RIGHT + 2.;
        if i == game.harbor {
            icon(Icon::Anchor, right - 8., mid, 16., LEAF);
            right -= 22.;
        } else {
            let days = game.voyage_days(i);
            let color = if days > game.provisions { RUST } else { SOFT };
            right -= stat_right(
                Icon::Hourglass,
                small!("{days}").as_str(),
                right,
                mid,
                14.,
                color,
            ) + 8.;
        }
        if Some(i) == destination {
            icon(Icon::Scroll, right - 8., mid, 15., RUST);
        }
    }
    voyage_preview(game);
    let can_sail = game.selected_port != game.harbor && game.provisions > 0;
    let days = game.voyage_days(game.selected_port);
    button(
        PRIMARY_Y,
        Button {
            key: "Space",
            glyph: Some(Icon::Ship),
            action: "Set sail",
            primary: true,
            enabled: can_sail,
        },
        can_sail.then_some((Icon::Hourglass, small!("{days}").as_str(), GOLD)),
    );
    route_toggle(game);
}

/// This week's wind as an arrow in a small dial.
fn wind_badge(game: &Game, c: Vec2) {
    shapes::fill_circle(c, 12., with_alpha(DARK, 0.1));
    let w = game.wind().normalize() * 8.;
    shapes::fill_capsule(c - w * 0.8, c + w * 0.3, 1.3, DARK);
    let side = vec2(-w.y, w.x).normalize() * 3.5;
    draw_triangle(c + w, c + w * 0.2 + side, c + w * 0.2 - side, DARK);
    icon(Icon::Wind, c.x - 24., c.y, 14., SOFT);
}

/// What Set sail will cost: days, food and the danger of the chosen sea.
fn voyage_preview(game: &Game) {
    let top = 388.;
    rounded(ROW_X, top, ROW_W, 80., 10., darken(PARCHMENT, 0.05));
    let to = game.selected_port;
    let cy = top + 40.;
    if to == game.harbor {
        // Nothing chosen yet: point at the chart.
        let k = bob(clock(), 0.8, 3.);
        icon(Icon::ArrowLeft, 760. + k, cy, 22., SOFT);
        icon(Icon::Hand, 800., cy, 26., SOFT);
        icon(Icon::Map, 838., cy, 22., SOFT);
        return;
    }
    let days = game.voyage_days(to);
    let short = days > game.provisions;
    let cell = ROW_W / 3.;
    let centers = [ROW_X + cell * 0.5, ROW_X + cell * 1.5, ROW_X + cell * 2.5];
    for x in [ROW_X + cell, ROW_X + cell * 2.] {
        line(
            vec2(x, top + 16.),
            vec2(x, top + 64.),
            1.,
            with_alpha(DARK, 0.12),
        );
    }
    // Days at sea.
    icon(Icon::Hourglass, centers[0], cy - 13., 20., DARK);
    say_center(small!("{days}").as_str(), centers[0], cy + 14., 18., DARK);
    // Food eaten, red when the pantry runs out first.
    let food = if short { RUST } else { DARK };
    icon(Icon::Pot, centers[1], cy - 13., 20., food);
    say_center(small!("-{days}").as_str(), centers[1], cy + 14., 18., food);
    if short {
        icon_badge(
            Icon::Alert,
            centers[1] + 16.,
            cy - 22.,
            15.,
            RUST,
            PARCHMENT,
        );
    }
    // Sea state.
    let (glyph, tint) = if game.sheltered {
        (Icon::Shield, LEAF)
    } else if crosses_storm(game, to) {
        (Icon::Rain, RUST)
    } else {
        (Icon::Wave, DARK)
    };
    icon(glyph, centers[2], cy - 13., 20., tint);
    if game.sheltered {
        icon(Icon::Check, centers[2], cy + 14., 16., LEAF);
    } else {
        icon(Icon::Alert, centers[2], cy + 14., 16., tint);
    }
}

/// The X button as a two-sided switch on the model's split: sheltered
/// (shield) or open sea (wave). Only the other side reacts to a click.
fn route_toggle(game: &Game) {
    let y = SECONDARY_Y;
    rounded(ROW_X, y, ROW_W, BUTTON_H, 9., with_alpha(DARK, 0.11));
    let cy = y + BUTTON_H * 0.5;
    keycap("X", "", ROW_X + 8., cy - 10., MUTED);
    let seg_w = ROW_X + ROW_W - 4. - ROUTE_SPLIT;
    let p = pointer();
    for (k, (glyph, name, tint)) in [
        (Icon::Shield, "Sheltered", LEAF),
        (Icon::Wave, "Open sea", RUST),
    ]
    .into_iter()
    .enumerate()
    {
        let on = (k == 0) == game.sheltered;
        let sx = ROUTE_SPLIT - seg_w + k as f32 * seg_w;
        let hit = if k == 0 {
            Rect::new(ROW_X, y, ROUTE_SPLIT - ROW_X, BUTTON_H)
        } else {
            Rect::new(ROUTE_SPLIT, y, ROW_X + ROW_W - ROUTE_SPLIT, BUTTON_H)
        };
        let hover = !on && inside(hit, p);
        if on {
            rounded(sx + 1., y + 3., seg_w - 2., BUTTON_H - 6., 7., tint);
        } else if hover {
            rounded(
                sx + 1.,
                y + 3.,
                seg_w - 2.,
                BUTTON_H - 6.,
                7.,
                with_alpha(tint, 0.2),
            );
            outline(
                sx + 1.,
                y + 3.,
                seg_w - 2.,
                BUTTON_H - 6.,
                7.,
                1.5,
                with_alpha(tint, 0.8),
            );
        }
        let ink = if on {
            PARCHMENT
        } else {
            with_alpha(DARK, if hover { 0.9 } else { 0.55 })
        };
        icon(glyph, sx + 15., cy, 15., ink);
        say(name, sx + 27., cy, 13., ink);
    }
}

fn market_page(game: &Game) {
    title_row(Icon::Anchor, PORTS[game.harbor].name, DARK, 150.);
    purse(game);
    let hover = row_hover(206., 40., 5);
    for good in 0..GOODS.len() {
        good_row(game, good, hover == Some(good));
    }
    food_row(game, hover == Some(3));
    repair_row(game, hover == Some(4));
    best_sale(game);
    // Buttons preview the exact coin change of the selected row.
    let (enabled, glyph, verb, price) = match game.row {
        good @ 0..=2 => {
            let price = game.buy_price(good);
            (
                game.stock[game.harbor][good] > 0
                    && game.cargo_total() < game.capacity()
                    && game.coins >= price,
                Icon::Bag,
                "Buy",
                price,
            )
        }
        3 => {
            let amount = pantry_offer(game);
            (amount > 0, Icon::Pot, "Buy", amount * 2)
        }
        _ => {
            let (amount, price) = repair_offer(game);
            (
                amount > 0 && game.coins >= price,
                Icon::Wrench,
                "Repair",
                price,
            )
        }
    };
    button(
        PRIMARY_Y,
        Button {
            key: "Space",
            glyph: Some(glyph),
            action: verb,
            primary: true,
            enabled,
        },
        enabled.then_some((Icon::Coin, small!("-{price}").as_str(), GOLD)),
    );
    let sellable =
        game.row <= 2 && game.cargo(game.row) > 0 && game.stock[game.harbor][game.row] < 16;
    let sale = small!("+{}", game.sell_price(game.row.min(2)));
    button(
        SECONDARY_Y,
        Button {
            key: "X",
            glyph: Some(Icon::Coin),
            action: "Sell",
            primary: false,
            enabled: sellable,
        },
        sellable.then_some((Icon::Coin, sale.as_str(), LEAF)),
    );
}

/// Days of food the provisions row would load now.
fn pantry_offer(game: &Game) -> u32 {
    (SUPPLY_CAP - game.provisions).min(6).min(game.coins / 2)
}

/// Hull the repair row would restore, and its price.
fn repair_offer(game: &Game) -> (u32, u32) {
    let amount = (100 - game.hull.min(100)).min(30);
    (amount, amount.div_ceil(2))
}

fn good_row(game: &Game, good: usize, hover: bool) {
    let y = market_row_y(good);
    row(y, 40., game.row == good, hover);
    let top = y + 13.;
    let bottom = y + 29.;
    let lift = if hover { 1. } else { 0. };
    good_glyph(good, vec2(679., y + 19. - lift), 24., 1.);
    let mut x = 697. + say(GOODS[good], 697., top, 15., DARK) + 12.;
    // This week's news lifts the price: a twinkling star.
    if forecast_shift(game.week(), good) > 0 {
        let k = pulse(clock() + good as f32, 1.);
        icon(Icon::Star, x, top - 1., 10. + 2. * k, GOLD);
        x += 16.;
    }
    // The charter's cargo: crates still missing, or a check once loaded.
    if let Some(job) = CHARTERS.get(game.charter).filter(|job| job.good == good) {
        let missing = job.amount.saturating_sub(game.cargo(good));
        if missing > 0 {
            stat(
                Icon::Scroll,
                small!("{missing}").as_str(),
                x - 6.,
                top,
                12.,
                RUST,
            );
        } else {
            icon(Icon::Scroll, x, top, 12., LEAF);
            icon(Icon::Check, x + 12., top, 11., LEAF);
        }
    }
    // In the hold, and left in the market stall.
    let held = game.cargo(good);
    let stock = game.stock[game.harbor][good];
    let mut x = 697.;
    x += stat(
        Icon::Crate,
        small!("{held}").as_str(),
        x,
        bottom,
        11.,
        if held > 0 { DARK } else { SOFT },
    ) + 10.;
    stat(
        Icon::Bag,
        small!("{stock}").as_str(),
        x,
        bottom,
        11.,
        if stock == 0 { RUST } else { SOFT },
    );
    // Pay to buy, receive to sell; the arrow shows the price against this
    // port's usual rate (news and scarcity move it).
    let buy = game.buy_price(good);
    let w = stat_right(
        Icon::Coin,
        small!("-{buy}").as_str(),
        RIGHT,
        top,
        15.,
        if stock == 0 { SOFT } else { DARK },
    );
    let usual = PORTS[game.harbor].prices[good];
    if buy != usual {
        let (arrow, tint) = if buy > usual {
            (Icon::ArrowUp, RUST)
        } else {
            (Icon::ArrowDown, LEAF)
        };
        icon(arrow, RIGHT - w - 9., top, 12., tint);
    }
    stat_right(
        Icon::Coin,
        small!("+{}", game.sell_price(good)).as_str(),
        RIGHT,
        bottom,
        12.,
        LEAF,
    );
}

fn food_row(game: &Game, hover: bool) {
    let y = market_row_y(3);
    row(y, 40., game.row == 3, hover);
    let top = y + 13.;
    let bottom = y + 29.;
    icon(Icon::Pot, 679., y + 19., 20., DARK);
    say("Food", 697., top, 15., DARK);
    // Pantry pips; the purchase shows as pulsing hollow pips.
    let amount = pantry_offer(game);
    let k = pulse(clock(), 1.1);
    for i in 0..SUPPLY_CAP {
        let at = vec2(701. + i as f32 * 8., bottom);
        if i < game.provisions {
            shapes::fill_circle(at, 2.8, LEAF);
        } else if i < game.provisions + amount {
            shapes::stroke_circle(at, 2.8, 1.3, with_alpha(darken(GOLD, 0.2), 0.5 + 0.5 * k));
        } else {
            shapes::stroke_circle(at, 2.4, 1., with_alpha(DARK, 0.2));
        }
    }
    if game.provisions >= SUPPLY_CAP {
        icon(Icon::Check, RIGHT - 8., top, 15., LEAF);
    } else if amount > 0 {
        stat_right(
            Icon::Coin,
            small!("-{}", amount * 2).as_str(),
            RIGHT,
            top,
            15.,
            DARK,
        );
        stat_right(
            Icon::Pot,
            small!("+{amount}").as_str(),
            RIGHT,
            bottom,
            12.,
            LEAF,
        );
    }
}

fn repair_row(game: &Game, hover: bool) {
    let y = market_row_y(4);
    row(y, 40., game.row == 4, hover);
    let top = y + 13.;
    let bottom = y + 29.;
    icon(Icon::Wrench, 679., y + 19., 18., DARK);
    say("Repair", 697., top, 15., DARK);
    // Hull meter with the repair previewed in pulsing gold.
    let (amount, price) = repair_offer(game);
    let (x, w) = (698., 100.);
    bar(
        x,
        bottom - 2.5,
        w,
        5.,
        game.hull as f32 / 100.,
        if game.hull < 35 { RUST } else { LEAF },
    );
    if amount > 0 {
        let k = pulse(clock(), 1.1);
        rounded(
            x + w * game.hull as f32 / 100.,
            bottom - 2.5,
            w * amount as f32 / 100.,
            5.,
            2.5,
            with_alpha(darken(GOLD, 0.15), 0.45 + 0.4 * k),
        );
        stat_right(
            Icon::Coin,
            small!("-{price}").as_str(),
            RIGHT,
            top,
            15.,
            if game.coins >= price { DARK } else { RUST },
        );
        stat_right(
            Icon::Ship,
            small!("+{amount}").as_str(),
            RIGHT,
            bottom,
            12.,
            LEAF,
        );
    } else {
        icon(Icon::Check, RIGHT - 8., top, 15., LEAF);
    }
}

/// Where the selected good sells best right now: a destination preview
/// instead of "buy low, sell high" advice.
fn best_sale(game: &Game) {
    let good = game.row;
    if good > 2 {
        return;
    }
    let Some((port, price)) = (0..PORTS.len())
        .filter(|&p| p != game.harbor)
        .map(|p| (p, game.price_at(p, good).saturating_sub(7).max(1)))
        .max_by_key(|&(_, price)| price)
    else {
        return;
    };
    let top = 414.;
    rounded(ROW_X, top, ROW_W, 50., 10., darken(PARCHMENT, 0.05));
    let cy = top + 25.;
    good_glyph(good, vec2(679., cy), 20., 1.);
    icon(Icon::ArrowRight, 699., cy, 13., SOFT);
    islet(vec2(718., cy), game.visited[port]);
    say(PORTS[port].name, 732., cy, 14., DARK);
    stat_right(
        Icon::Coin,
        small!("+{price}").as_str(),
        RIGHT,
        cy,
        16.,
        LEAF,
    );
}

fn charter_page(game: &Game, t: f32) {
    let Some(job) = CHARTERS.get(game.charter) else {
        homecoming_page(game, t);
        return;
    };
    title_row(Icon::Scroll, job.name, RUST, RIGHT - 686.);
    let aboard = game.cargo(job.good);
    let loaded = aboard >= job.amount;
    let here = game.harbor == job.destination;
    // Manifest: one slot per crate the charter needs.
    let n = job.amount as usize;
    let (size, gap) = (46., 8.);
    let x0 = ROW_X + (ROW_W - (n as f32 * size + (n - 1) as f32 * gap)) * 0.5;
    for i in 0..n {
        let x = x0 + i as f32 * (size + gap);
        let y = 198.;
        let c = vec2(x + size * 0.5, y + size * 0.5);
        if (i as u32) < aboard {
            rounded(x, y, size, size, 9., lighten(good_color(job.good), 0.62));
            outline(
                x,
                y,
                size,
                size,
                9.,
                1.5,
                darken(good_color(job.good), 0.15),
            );
            good_glyph(job.good, c, 32., 1.);
        } else {
            rounded(x, y, size, size, 9., darken(PARCHMENT, 0.06));
            outline(x, y, size, size, 9., 1., with_alpha(DARK, 0.25));
            good_glyph(job.good, c, 28., 0.22);
        }
    }
    // Where it goes and what it pays.
    let cy = 272.;
    icon(Icon::Pin, 671., cy, 17., RUST);
    say(PORTS[job.destination].name, 686., cy, 16., DARK);
    if here {
        icon(
            Icon::Anchor,
            686. + measure(PORTS[job.destination].name, 16.) + 14.,
            cy,
            14.,
            LEAF,
        );
    }
    stat_right(
        Icon::Coin,
        small!("+{}", job.reward).as_str(),
        RIGHT + 4.,
        cy,
        17.,
        LEAF,
    );
    // The next step, as one word on the tab's own icon.
    let (glyph, word, tint) = if loaded && here {
        (Icon::Check, "Ready", LEAF)
    } else if loaded {
        (Icon::Map, "Sail", DARK)
    } else {
        (Icon::Bag, "Buy", RUST)
    };
    let k = pulse(t, 1.2);
    let w = measure(word, 15.) + 15. * 1.9 + 4.;
    let x = ROW_X + (ROW_W - w) * 0.5;
    if loaded && here {
        glow(
            x + w * 0.5,
            321.,
            w * 0.6,
            with_alpha(LEAF, 0.12 + 0.12 * k),
        );
    }
    icon_chip(glyph, word, x, 310., 15., tint);
    seals(game, t);
    button(
        PRIMARY_Y,
        Button {
            key: "Space",
            glyph: Some(Icon::Scroll),
            action: "Deliver",
            primary: true,
            enabled: loaded && here,
        },
        (loaded && here).then_some((Icon::Coin, small!("+{}", job.reward).as_str(), GOLD)),
    );
    button(
        SECONDARY_Y,
        Button {
            key: "X",
            glyph: Some(Icon::Bag),
            action: "Market",
            primary: false,
            enabled: true,
        },
        None,
    );
}

/// After the sixth seal: the treasury against the lien, and what is missing.
fn homecoming_page(game: &Game, t: f32) {
    title_row(Icon::Key, "Homecoming", RUST, RIGHT - 686.);
    let paid = game.coins >= DEBT;
    let cy = 212.;
    icon(Icon::Coin, 672., cy, 20., darken(GOLD, 0.2));
    let w = say(small!("{}", game.coins).as_str(), 688., cy, 22., DARK);
    say(small!("/ {DEBT}").as_str(), 694. + w, cy + 2., 14., SOFT);
    bar(
        LEFT,
        232.,
        RIGHT + 8. - LEFT,
        9.,
        game.coins as f32 / DEBT as f32,
        if paid { LEAF } else { GOLD },
    );
    // Requirements: six seals, docked at Haven, crowns in hand.
    let checks = [
        (Icon::Scroll, "6/6", true),
        (Icon::Pin, PORTS[0].name, game.harbor == 0),
        (Icon::Coin, "900", paid),
    ];
    let mut x = LEFT;
    let mut ready = true;
    for (glyph, name, met) in checks {
        let tint = if met { LEAF } else { RUST };
        let w = icon_chip(glyph, name, x, 258., 13., tint);
        tick_mark(vec2(x + w + 3., 258.), met);
        x += w + 22.;
        ready &= met;
    }
    // Saltwind under the lien: the lock opens once all three are met.
    let c = vec2(ROW_X + ROW_W * 0.5, 330.);
    let k = pulse(t, 0.8);
    if ready {
        glow(c.x, c.y, 46. + 6. * k, with_alpha(GOLD, 0.25 + 0.15 * k));
    }
    sail_glyph(c + vec2(0., bob(t, 0.5, 1.5)), 58., false, DARK);
    icon_badge(
        if ready { Icon::Unlock } else { Icon::Lock },
        c.x + 30.,
        c.y + 14.,
        26.,
        if ready { LEAF } else { RUST },
        PARCHMENT,
    );
    seals(game, t);
    button(
        PRIMARY_Y,
        Button {
            key: "Space",
            glyph: Some(Icon::Wrench),
            action: "Shipyard",
            primary: true,
            enabled: true,
        },
        None,
    );
    button(
        SECONDARY_Y,
        Button {
            key: "X",
            glyph: Some(Icon::Bag),
            action: "Market",
            primary: false,
            enabled: true,
        },
        None,
    );
}

/// Whether the selected shipyard row can be bought now (or the lien paid).
fn refit_ready(game: &Game, row: usize) -> bool {
    match row {
        0..=2 => !game.upgrades[row] && game.coins >= REFITS[row],
        _ => game.harbor == 0 && game.charter == CHARTERS.len() && game.coins >= DEBT,
    }
}

fn shipyard_page(game: &Game) {
    title_row(Icon::Anchor, PORTS[game.harbor].name, DARK, 150.);
    purse(game);
    let hover = row_hover(199., 60., 4);
    for i in 0..4 {
        let y = shipyard_row_y(i);
        row(y, 60., game.row == i, hover == Some(i));
        let installed = i < 3 && game.upgrades[i];
        let lift = if hover == Some(i) { 1. } else { 0. };
        icon_badge(
            REFIT_ICONS[i],
            682.,
            y + 29. - lift,
            32.,
            if installed { LEAF } else { DARK },
            PARCHMENT,
        );
        say(REFIT_NAMES[i], 706., y + 21., 15., DARK);
        // What it changes, as icon deltas.
        let cy = y + 41.;
        match i {
            0 => {
                stat(Icon::Hourglass, "-1", 706., cy, 12., SOFT);
            }
            1 => {
                let x = 706. + stat(Icon::Crate, "8", 706., cy, 12., SOFT);
                icon(Icon::ArrowRight, x + 9., cy, 10., SOFT);
                say("12", x + 18., cy, 12., SOFT);
            }
            2 => {
                stat(Icon::Rain, "-3", 706., cy, 12., SOFT);
            }
            _ => {
                let mut x = 706.;
                for (glyph, met) in [
                    (Icon::Scroll, game.charter == CHARTERS.len()),
                    (Icon::Pin, game.harbor == 0),
                    (Icon::Coin, game.coins >= DEBT),
                ] {
                    icon(glyph, x + 6., cy, 12., if met { LEAF } else { RUST });
                    tick_mark(vec2(x + 17., cy + 3.), met);
                    x += 32.;
                }
            }
        }
        let mid = y + 29.;
        if installed {
            icon(Icon::Check, RIGHT - 8., mid, 18., LEAF);
        } else {
            let price = if i < 3 { REFITS[i] } else { DEBT };
            let afford = game.coins >= price;
            stat_right(
                Icon::Coin,
                small!("{price}").as_str(),
                RIGHT,
                mid,
                15.,
                if afford { DARK } else { RUST },
            );
        }
    }
    let row = game.row.min(3);
    let ready = refit_ready(game, row);
    let price = if row < 3 { REFITS[row] } else { DEBT };
    button(
        PRIMARY_Y,
        Button {
            key: "Space",
            glyph: Some(REFIT_ICONS[row]),
            action: if row == 3 { "Settle" } else { "Buy" },
            primary: true,
            enabled: ready,
        },
        ready.then_some((Icon::Coin, small!("-{price}").as_str(), GOLD)),
    );
    button(
        SECONDARY_Y,
        Button {
            key: "X",
            glyph: Some(Icon::Map),
            action: "Chart",
            primary: false,
            enabled: true,
        },
        None,
    );
}

fn log_page(game: &Game, t: f32) {
    title_row(
        Icon::Calendar,
        small!("Day {}", game.day.min(DEADLINE)).as_str(),
        DARK,
        150.,
    );
    let left = DEADLINE.saturating_sub(game.day);
    stat_right(
        Icon::Hourglass,
        small!("{left}").as_str(),
        RIGHT + 4.,
        TITLE_Y,
        16.,
        if left < 7 { RUST } else { DARK },
    );
    // The season: six weeks of seven days, each week led by its forecast.
    let week = game.week();
    for (w, glyph) in FORECAST_ICONS.iter().enumerate() {
        let cy = 206. + w as f32 * 26.;
        let tint = if w == week {
            RUST
        } else if w < week {
            with_alpha(SOFT, 0.6)
        } else {
            with_alpha(DARK, 0.55)
        };
        icon(*glyph, 673., cy, 15., tint);
        for d in 0..7 {
            let day = (w * 7 + d + 1) as u32;
            let x = 692. + d as f32 * 32.;
            let r = Rect::new(x, cy - 10., 28., 20.);
            if day < game.day {
                rounded(r.x, r.y, r.w, r.h, 5., with_alpha(DARK, 0.3));
            } else if day == game.day {
                let k = pulse(t, 1.);
                glow(
                    r.x + r.w * 0.5,
                    cy,
                    20. + 4. * k,
                    with_alpha(GOLD, 0.3 + 0.2 * k),
                );
                rounded(r.x, r.y, r.w, r.h, 5., GOLD);
                outline(r.x, r.y, r.w, r.h, 5., 1.5, darken(GOLD, 0.35));
            } else {
                outline(r.x, r.y, r.w, r.h, 5., 1., with_alpha(DARK, 0.22));
            }
            if day == DEADLINE {
                icon(Icon::Flag, r.x + r.w * 0.5, cy, 13., RUST);
            }
        }
    }
    // This week's forecast: its icon, name and price effects.
    let top = 364.;
    rounded(ROW_X, top, ROW_W, 100., 10., darken(PARCHMENT, 0.05));
    icon_badge(FORECAST_ICONS[week], 688., top + 32., 38., DARK, GOLD);
    say(FORECASTS[week], 716., top + 26., 18., DARK);
    let mut x = 716.;
    for good in 0..GOODS.len() {
        let shift = forecast_shift(week, good);
        if shift == 0 {
            continue;
        }
        good_glyph(good, vec2(x + 9., top + 50.), 18., 1.);
        let (arrow, tint) = if shift > 0 {
            (Icon::ArrowUp, RUST)
        } else {
            (Icon::ArrowDown, LEAF)
        };
        icon(arrow, x + 25., top + 50., 12., tint);
        x += 34. + say(small!("{shift:+}").as_str(), x + 33., top + 50., 13., tint) + 12.;
    }
    // Wind and the islands seen so far.
    wind_badge(game, vec2(RIGHT - 6., top + 78.));
    let mut x = LEFT + 4.;
    for (port, &seen) in game.visited.iter().enumerate() {
        islet(vec2(x + 6., top + 78.), seen);
        if port == game.harbor {
            shapes::stroke_circle(vec2(x + 6., top + 78.), 11., 1.2, with_alpha(RUST, 0.7));
        }
        x += 24.;
    }
    button(
        PRIMARY_Y,
        Button {
            key: "Space",
            glyph: Some(Icon::Hourglass),
            action: "Wait",
            primary: true,
            enabled: true,
        },
        Some((Icon::Calendar, "+1", GOLD)),
    );
    button(
        SECONDARY_Y,
        Button {
            key: "X",
            glyph: Some(Icon::Map),
            action: "Chart",
            primary: false,
            enabled: true,
        },
        None,
    );
}

fn sea_page(game: &Game, t: f32) {
    let Some(passage) = game.passage else {
        return;
    };
    title_row(Icon::Pin, PORTS[passage.to].name, RUST, 150.);
    // Passage progress is stored in hundredths of a tick.
    let unit = DAY_TICKS * 100;
    let remaining = passage
        .total
        .saturating_sub(passage.traveled)
        .div_ceil(unit);
    stat_right(
        Icon::Hourglass,
        small!("{remaining}").as_str(),
        RIGHT + 4.,
        TITLE_Y,
        16.,
        DARK,
    );
    // Voyage track: one pip per day from the home anchor to the pin.
    let (a, b, cy) = (690., 896., 214.);
    icon(Icon::Anchor, 672., cy, 15., SOFT);
    icon(Icon::Pin, 912., cy, 16., RUST);
    let progress = passage.traveled as f32 / passage.total as f32;
    let days = passage.total / unit;
    rounded(a, cy - 2., b - a, 4., 2., with_alpha(DARK, 0.14));
    rounded(a, cy - 2., (b - a) * progress, 4., 2., LEAF);
    for d in 0..=days {
        let x = a + (b - a) * d as f32 / days.max(1) as f32;
        let done = (d as f32) / (days.max(1) as f32) <= progress;
        shapes::fill_circle(
            vec2(x, cy),
            3.5,
            if done { LEAF } else { darken(PARCHMENT, 0.2) },
        );
    }
    sail_glyph(
        vec2(a + (b - a) * progress, cy - 14. + bob(t, 0.9, 1.2)),
        22.,
        passage.reefed,
        DARK,
    );
    // Sail plan: full canvas or reefed, with speed and squall damage.
    let armor = if game.upgrades[2] { 3 } else { 0 };
    let tiles = [
        (false, 1., 11_u32.saturating_sub(armor)),
        (true, 0.7, 3_u32.saturating_sub(armor)),
    ];
    for (k, (reefed, speed, hit)) in tiles.into_iter().enumerate() {
        let (x, y) = (SAIL_TILES[k], SAIL_TILE_Y[0]);
        let (w, h) = (SAIL_TILE_W, SAIL_TILE_Y[1] - SAIL_TILE_Y[0]);
        let on = passage.reefed == reefed;
        let hover = !on && inside(Rect::new(x, y, w, h), pointer());
        let y = y - if hover { 1. } else { 0. };
        if on {
            rounded(x, y, w, h, 10., DARK);
        } else {
            rounded(
                x,
                y,
                w,
                h,
                10.,
                if hover {
                    lighten(PARCHMENT, 0.3)
                } else {
                    darken(PARCHMENT, 0.06)
                },
            );
            outline(
                x,
                y,
                w,
                h,
                10.,
                if hover { 1.5 } else { 1. },
                if hover {
                    with_alpha(GOLD, 0.9)
                } else {
                    with_alpha(DARK, 0.15)
                },
            );
        }
        let ink = if on {
            PARCHMENT
        } else {
            with_alpha(DARK, 0.55)
        };
        sail_glyph(vec2(x + 34., y + 34.), 40., reefed, ink);
        // Speed.
        icon(Icon::Wind, x + 72., y + 22., 14., ink);
        bar(
            x + 82.,
            y + 19.,
            38.,
            6.,
            speed,
            if on { GOLD } else { with_alpha(DARK, 0.4) },
        );
        // Damage per squall hit (none on a sheltered course).
        if passage.sheltered {
            icon(
                Icon::Shield,
                x + 72.,
                y + 46.,
                14.,
                if on { MINT } else { ink },
            );
        } else {
            stat(
                Icon::Rain,
                small!("-{hit}").as_str(),
                x + 65.,
                y + 46.,
                14.,
                if on { lighten(RUST, 0.45) } else { ink },
            );
        }
        let s = if reefed { "Reefed" } else { "Full sail" };
        say_center(s, x + w * 0.5, y + 76., 13., ink);
    }
    // Next sunrise: a day dial, the food it eats and the sea it brings.
    let top = 344.;
    rounded(ROW_X, top, ROW_W, 84., 10., darken(PARCHMENT, 0.05));
    let cell = ROW_W / 3.;
    let centers = [ROW_X + cell * 0.5, ROW_X + cell * 1.5, ROW_X + cell * 2.5];
    for x in [ROW_X + cell, ROW_X + cell * 2.] {
        line(
            vec2(x, top + 16.),
            vec2(x, top + 68.),
            1.,
            with_alpha(DARK, 0.12),
        );
    }
    let cy = top + 42.;
    let k = pulse(t, 1.5);
    ring_meter(
        centers[0],
        cy,
        21.,
        4.,
        passage.day_ticks as f32 / DAY_TICKS as f32,
        darken(GOLD, 0.15),
    );
    icon(Icon::Sun, centers[0], cy, 20., darken(GOLD, 0.2));
    if game.provisions > 0 {
        icon(Icon::Pot, centers[1], cy - 13., 20., DARK);
        say_center("-1", centers[1], cy + 14., 18., DARK);
    } else {
        let starving = with_alpha(RUST, 0.6 + 0.4 * k);
        icon(Icon::Ship, centers[1], cy - 13., 20., starving);
        say_center("-22", centers[1], cy + 14., 18., starving);
    }
    let threat = storm_threat(game);
    let sea = centers[2];
    if passage.sheltered {
        icon(Icon::Shield, sea, cy - 13., 22., LEAF);
        icon(Icon::Check, sea, cy + 14., 16., LEAF);
    } else if threat >= 1. {
        glow(sea, cy, 34. + 8. * k, with_alpha(RUST, 0.18 + 0.15 * k));
        icon(Icon::Rain, sea, cy - 12., 26. + 2. * k, RUST);
        say_center("Squall", sea, cy + 15., 14., RUST);
    } else if threat > 0. {
        icon(Icon::Rain, sea, cy - 13., 22., darken(GOLD, 0.25));
        icon(Icon::Alert, sea, cy + 14., 16., darken(GOLD, 0.25));
    } else {
        icon(Icon::Wave, sea, cy - 13., 22., SOFT);
        icon(Icon::Check, sea, cy + 14., 16., SOFT);
    }
    button(
        PRIMARY_Y,
        Button {
            key: "Space",
            glyph: None,
            action: if passage.reefed { "Unfurl" } else { "Reef" },
            primary: true,
            enabled: true,
        },
        None,
    );
    // The reef glyph sits where the button's icon would be.
    let target = !passage.reefed;
    let r = Rect::new(ROW_X, PRIMARY_Y, ROW_W, BUTTON_H);
    let hovering = inside(r, pointer());
    sail_glyph(
        vec2(
            ROW_X + 8. + measure("Space", 13.) + 12. + 9. + 8. + 50.,
            PRIMARY_Y + BUTTON_H * 0.5 - if hovering { 1. } else { 0. },
        ),
        18.,
        target,
        PARCHMENT,
    );
    if threat >= 1. && !passage.reefed {
        outline(
            ROW_X - 2.,
            PRIMARY_Y - 2.,
            ROW_W + 4.,
            BUTTON_H + 4.,
            11.,
            2.,
            with_alpha(RUST, 0.4 + 0.5 * k),
        );
    }
    // Steering is held, not clicked: show the keys and their effect.
    let y = SECONDARY_Y;
    rounded(ROW_X, y, ROW_W, BUTTON_H, 9., with_alpha(DARK, 0.11));
    let cy = y + BUTTON_H * 0.5;
    let mut x = ROW_X + 8.;
    x += keycap("Up", "", x, cy - 10., MUTED) + 4.;
    x += keycap("Down", "", x, cy - 10., MUTED) + 10.;
    let nudge = bob(t, 0.5, 2.);
    icon(Icon::Ship, x + 9., cy + nudge, 15., DARK);
    say("Steer", x + 24., cy, 14., DARK);
}

// ---------------------------------------------------------------------------
// Header, results and effects

/// The objective row: the charter's cargo, port and pay, or the lien; then
/// the season's calendar with this week's forecast.
fn objective(game: &Game, area: Rect) {
    let cy = area.y + area.h * 0.5;
    let t = clock();
    let mut x = area.x + 4.;
    match CHARTERS.get(game.charter) {
        Some(job) => {
            let aboard = game.cargo(job.good).min(job.amount);
            let loaded = aboard >= job.amount;
            good_glyph(job.good, vec2(x + 11., cy), 24., 1.);
            x += 28.;
            x += shout(
                small!("{aboard}/{}", job.amount).as_str(),
                x,
                cy,
                17.,
                if loaded { MINT } else { PAPER },
            ) + 12.;
            icon(Icon::ArrowRight, x + 7., cy, 14., MUTED);
            x += 22.;
            icon(Icon::Pin, x + 9., cy, 18., GOLD);
            x += 22.;
            x += shout(PORTS[job.destination].name, x, cy, 17., PAPER) + 18.;
            icon(Icon::Coin, x + 9., cy, 17., GOLD);
            x += 22.;
            x += shout(small!("{}", job.reward).as_str(), x, cy, 17., GOLD) + 12.;
            if loaded && game.harbor == job.destination && game.passage.is_none() {
                let k = pulse(t, 1.2);
                glow(x + 10., cy, 16. + 4. * k, with_alpha(MINT, 0.2 + 0.2 * k));
                icon(Icon::Check, x + 10., cy, 18., MINT);
            }
        }
        None if game.result == Outcome::Won => {
            icon(Icon::Key, x + 10., cy, 18., GOLD);
            icon(Icon::Check, x + 34., cy, 18., MINT);
            icon(Icon::Pin, x + 60., cy, 18., GOLD);
            shout(PORTS[0].name, x + 72., cy, 17., PAPER);
        }
        None => {
            icon(Icon::Key, x + 10., cy, 18., GOLD);
            x += 26.;
            icon(Icon::Coin, x + 9., cy, 17., GOLD);
            x += 22.;
            let paid = game.coins >= DEBT;
            x += shout(
                small!("{}/{DEBT}", game.coins).as_str(),
                x,
                cy,
                17.,
                if paid { MINT } else { PAPER },
            ) + 12.;
            icon(Icon::ArrowRight, x + 7., cy, 14., MUTED);
            x += 22.;
            icon(Icon::Pin, x + 9., cy, 18., GOLD);
            x += 22.;
            shout(PORTS[0].name, x, cy, 17., PAPER);
        }
    }
    // Calendar: the season as a meter split into weeks, days left at the
    // end. It stops short of the score column, where gains rise.
    let right = area.x + area.w - 112.;
    let left = DEADLINE.saturating_sub(game.day);
    let late = left < 7;
    let tint = if late { DANGER } else { GOLD };
    shout(small!("{left}").as_str(), right - 26., cy, 17., tint);
    icon(Icon::Hourglass, right - 36., cy, 16., tint);
    let (mx, mw) = (right - 200., 150.);
    gauge(
        mx,
        cy - 4.,
        mw,
        8.,
        game.day.min(DEADLINE) as f32 / DEADLINE as f32,
        tint,
    );
    // Week boundaries: the forecast changes at each notch.
    for k in 1..6 {
        let x = mx + mw * k as f32 / 6.;
        line(
            vec2(x, cy - 4.),
            vec2(x, cy + 4.),
            2.,
            Color::new(0.04, 0.05, 0.08, 1.),
        );
    }
    icon(Icon::Calendar, mx - 14., cy, 17., MUTED);
    let week = game.week();
    icon_badge(
        FORECAST_ICONS[week],
        mx - 44.,
        cy,
        26.,
        Color::new(0.12, 0.16, 0.2, 1.),
        GOLD,
    );
}

/// The ending's facts as icons: rank or cause, then charters, day, crowns, hull.
fn result_card(game: &Game, area: Rect) {
    let (glyph, title, tint) = match (game.result, game.loss) {
        (Outcome::Won, _) => (Icon::Crown, game.rank(), GOLD),
        (_, Some(Loss::Sank)) => (Icon::Wave, "Sunk", DANGER),
        (_, Some(Loss::Starved)) => (Icon::Pot, "Starved", DANGER),
        (_, Some(Loss::Season)) => (Icon::Calendar, "Out of time", DANGER),
        _ => (Icon::Coin, "Disbanded", DANGER),
    };
    let x = area.x;
    let cy = area.y + 26.;
    icons::draw_styled(
        glyph,
        vec2(x + 16., cy),
        30.,
        IconStyle::new(tint).shadow(Color::new(0., 0., 0., 0.45)),
    );
    shout(title, x + 42., cy, 26., tint);
    let stats = [
        (Icon::Scroll, small!("{}/{}", game.charter, CHARTERS.len())),
        (Icon::Calendar, small!("{}", game.day.min(DEADLINE))),
        (Icon::Coin, small!("{}", game.coins)),
        (Icon::Ship, small!("{}%", game.hull)),
    ];
    let cell = area.w / stats.len() as f32;
    let cy = area.y + 98.;
    for (i, (glyph, value)) in stats.iter().enumerate() {
        let cx = x + cell * i as f32;
        icon_value(*glyph, value.as_str(), cx, cy, 26., PAPER);
    }
}

/// The game-specific half of the feedback: effects where each accepted (or
/// refused) event happened, from the event list of the tick that just ran.
fn feedback(game: &Game, fx: &mut Juice) {
    for event in game.events.iter() {
        match event {
            Event::Chose(port) => fx.ring(Game::position(port), with_alpha(GOLD, 0.8), 40.),
            Event::Bought { good, price } => {
                let row = market_row_y(good) + 13.;
                fx.popup_icon(
                    vec2(SALE_X, row),
                    Icon::Coin,
                    small!("-{price}").as_str(),
                    GOLD,
                );
                let slot = hold_slot(crates_through(game, good).saturating_sub(1));
                fx.burst(slot, good_color(good), 12);
                fx.ring(slot, good_color(good), 14.);
                fx.ring(vec2(679., market_row_y(good) + 19.), good_color(good), 22.);
            }
            Event::Sold { good, price } => {
                let row = market_row_y(good) + 29.;
                fx.popup_icon(
                    vec2(SALE_X, row),
                    Icon::Coin,
                    small!("+{price}").as_str(),
                    GOLD,
                );
                fx.sparkle(vec2(COIN_X + 14., BAND_Y), GOLD);
                fx.burst(hold_slot(crates_through(game, good)), good_color(good), 8);
            }
            Event::Provisioned { days, price } => {
                let pip = food_pip(game.provisions.saturating_sub(1));
                fx.popup_icon(
                    pip - vec2(0., 10.),
                    Icon::Pot,
                    small!("+{days}").as_str(),
                    MINT,
                );
                fx.sparkle(food_pip(game.provisions.saturating_sub(days / 2 + 1)), MINT);
                let row = market_row_y(3) + 13.;
                fx.popup_icon(
                    vec2(SALE_X, row),
                    Icon::Coin,
                    small!("-{price}").as_str(),
                    GOLD,
                );
            }
            Event::Repaired { hull, price } => {
                let m = hull_meter();
                fx.popup_icon(
                    vec2(m.x + m.w * 0.5, BAND_Y - 10.),
                    Icon::Wrench,
                    small!("+{hull}").as_str(),
                    MINT,
                );
                fx.sparkle(game.boat_position(), SAND);
                let row = market_row_y(4) + 13.;
                fx.popup_icon(
                    vec2(SALE_X, row),
                    Icon::Coin,
                    small!("-{price}").as_str(),
                    GOLD,
                );
            }
            Event::Delivered { port, reward } => {
                let at = Game::position(port);
                fx.burst(at, GOLD, 48);
                fx.ring(at, GOLD, 72.);
                fx.ring(at, FOAM, 46.);
                fx.popup_icon(beside(at), Icon::Coin, small!("+{reward}").as_str(), GOLD);
                fx.callout(
                    vec2(at.x.clamp(150., 490.), (at.y + 84.).clamp(260., 470.)),
                    "Sealed!",
                    GOLD,
                );
                let seal = seal_at(game.charter.saturating_sub(1));
                fx.sparkle(seal, GOLD);
                fx.ring(seal, LEAF, 26.);
            }
            Event::Refitted(upgrade) => {
                let at = vec2(682., shipyard_row_y(upgrade) + 29.);
                fx.ring(at, GOLD, 34.);
                fx.sparkle(at, GOLD);
                fx.popup_icon(at - vec2(0., 22.), REFIT_ICONS[upgrade], "", GOLD);
                fx.sparkle(game.boat_position(), GOLD);
            }
            Event::Settled => {
                let haven = Game::position(0);
                fx.burst(haven, GOLD, 90);
                fx.ring(haven, GOLD, 96.);
                fx.ring(haven, FOAM, 60.);
                fx.callout(haven + vec2(150., -80.), "Saltwind is yours", GOLD);
            }
            Event::Route { sheltered } => {
                let half = (ROW_X + ROW_W - 4. - ROUTE_SPLIT) * 0.5;
                let x = ROUTE_SPLIT + if sheltered { -half } else { half };
                let at = vec2(x, button_center(false).y);
                fx.ring(at, if sheltered { LEAF } else { RUST }, 30.);
            }
            Event::Departed { from } => {
                let at = Game::position(from) + vec2(0., 20.);
                fx.ring(at, FOAM, 38.);
                fx.burst(at, FOAM, 14);
            }
            Event::Reefed(reefed) => {
                let at = game.boat_position();
                fx.ring(at, SAND, if reefed { 22. } else { 34. });
                let tile = SAIL_TILES[usize::from(reefed)] + SAIL_TILE_W * 0.5;
                fx.ring(vec2(tile, 286.), GOLD, 40.);
            }
            Event::Ate => fx.ring(food_pip(game.provisions), GOLD, 9.),
            Event::Starved { damage } => {
                fx.popup_icon(
                    game.boat_position() - vec2(0., 26.),
                    Icon::Ship,
                    small!("-{damage}").as_str(),
                    DANGER,
                );
                fx.ring(food_pip(0), DANGER, 14.);
                fx.shake(0.3);
                fx.vignette(Color::new(0.85, 0.2, 0.15, 0.28));
            }
            Event::Squall { damage, at } => {
                fx.burst(at, FOAM, 30);
                fx.ring(at, DANGER, 36.);
                fx.popup_icon(
                    at - vec2(0., 28.),
                    Icon::Ship,
                    small!("-{damage}").as_str(),
                    DANGER,
                );
                let m = hull_meter();
                fx.ring(
                    vec2(m.x + m.w * game.hull as f32 / 100., BAND_Y),
                    DANGER,
                    12.,
                );
            }
            Event::Docked { port, first } => {
                let at = Game::position(port);
                fx.ring(at, FOAM, 54.);
                if first {
                    fx.sparkle(at, GOLD);
                    fx.popup_icon(beside(at) - vec2(0., 20.), Icon::Flag, "New", GOLD);
                }
            }
            Event::Week(week) => {
                fx.toast(Some(FORECAST_ICONS[week]), FORECASTS[week], GOLD);
                let tab = tab_rect(1);
                fx.sparkle(vec2(tab.x + 32., 133.), GOLD);
            }
            Event::Waited => fx.popup_icon(
                button_center(true) - vec2(0., 18.),
                Icon::Calendar,
                "+1",
                SAND,
            ),
            Event::Refused(why) => refusal(game, why, fx),
            Event::Lost(Loss::Sank | Loss::Starved) => {
                fx.burst(game.boat_position(), FOAM, 60);
                fx.ring(game.boat_position(), DANGER, 60.);
            }
            Event::Lost(_) => {}
        }
    }
}

/// A popup spot by an island that stays clear of the ship band: above it,
/// or beside it for the northern islands.
fn beside(at: Vec2) -> Vec2 {
    if at.y < 260. {
        at + vec2(50., -4.)
    } else {
        at - vec2(0., 44.)
    }
}

/// Crates of goods up to and including `good` (hold slots fill grain,
/// cloth, then spice).
fn crates_through(game: &Game, good: usize) -> u32 {
    (0..=good.min(2)).map(|g| game.cargo(g)).sum()
}

/// A short shake, a red ring on the thing that refused and one or two
/// words (or the missing resource) rising from it.
fn refusal(game: &Game, why: Refusal, fx: &mut Juice) {
    let at = match game.page {
        1 => vec2(860., market_row_y(game.row.min(4)) + 19.),
        3 => vec2(860., shipyard_row_y(game.row.min(3)) + 29.),
        _ => button_center(true),
    };
    let need;
    let unsealed;
    let (glyph, word): (Icon, &str) = match why {
        Refusal::SoldOut => (Icon::Bag, "Sold out"),
        Refusal::HoldFull => {
            fx.ring(hold_slot(game.capacity() - 1), DANGER, 16.);
            (Icon::Crate, "Full")
        }
        Refusal::Crowns(short) => {
            need = small!("-{short}");
            fx.ring(vec2(COIN_X, BAND_Y), DANGER, 16.);
            (Icon::Coin, need.as_str())
        }
        Refusal::PantryFull => (Icon::Pot, "Full"),
        Refusal::HullSound => (Icon::Ship, "100"),
        Refusal::NotCargo => (Icon::Cross, "No sale"),
        Refusal::NoneAboard => (Icon::Crate, "0"),
        Refusal::WarehouseFull => (Icon::Bag, "Full"),
        Refusal::AllSealed => (Icon::Check, "Done"),
        Refusal::WrongPort(port) => {
            fx.ring(Game::position(port), GOLD, 50.);
            (Icon::Pin, PORTS[port].name)
        }
        Refusal::Missing(count) => {
            need = small!("-{count}");
            (Icon::Crate, need.as_str())
        }
        Refusal::Installed => (Icon::Check, ""),
        Refusal::NotHaven => {
            fx.ring(Game::position(0), GOLD, 50.);
            (Icon::Pin, PORTS[0].name)
        }
        Refusal::Unsealed => {
            unsealed = small!("{}/{}", game.charter, CHARTERS.len());
            (Icon::Scroll, unsealed.as_str())
        }
        Refusal::SamePort => {
            fx.ring(Game::position(game.harbor), DANGER, 46.);
            (Icon::Anchor, "")
        }
        Refusal::NoFood => {
            fx.ring(food_pip(0), DANGER, 16.);
            (Icon::Pot, "0")
        }
    };
    fx.shake(0.12);
    fx.ring(at, DANGER, 24.);
    fx.popup_icon(at - vec2(0., 16.), glyph, word, DANGER);
}

impl ArcadeGame for Game {
    fn new() -> Self {
        Game::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.tick(input)
    }
    fn draw(&self) {
        let t = clock();
        draw_chart(self);
        sidebar_frame(self, t);
        if self.passage.is_some() {
            sea_page(self, t);
        } else {
            match self.page {
                0 => chart_page(self),
                1 => market_page(self),
                2 => charter_page(self, t),
                3 => shipyard_page(self),
                _ => log_page(self, t),
            }
        }
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: CHARTERS
                .get(self.charter)
                .map_or("Homecoming", |c| c.name)
                .into(),
            objective: CHARTERS.get(self.charter).map_or_else(
                || format!("Pay {DEBT} crowns at Haven"),
                |c| {
                    format!(
                        "Deliver {} {} to {}",
                        c.amount, GOODS[c.good], PORTS[c.destination].name
                    )
                },
            ),
            status: format!(
                "{} crowns | Hull {}% | Food {}d | Cargo {}/{}",
                self.coins,
                self.hull,
                self.provisions,
                self.cargo_total(),
                self.capacity()
            ),
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        self.result()
    }
    fn summary(&self) -> String {
        match self.result {
            Outcome::Won => format!(
                "{}: home on day {} with {} crowns and {}% hull.",
                self.rank(),
                self.day,
                self.coins,
                self.hull
            ),
            Outcome::Lost => self.loss.map_or("The voyage ended.", Loss::text).into(),
            Outcome::Playing => "Trade carefully and bring Saltwind home.".into(),
        }
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
    /// Six charters, then the homecoming settlement at Haven.
    fn progress(&self) -> Option<(u32, u32)> {
        let total = CHARTERS.len() as u32 + 1;
        Some(((self.charter as u32 + 1).min(total), total))
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        feedback(self, fx);
    }
    fn draw_objective(&self, area: Rect) -> bool {
        objective(self, area);
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        result_card(self, area);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(game: &mut Game, input: Input) -> Cue {
        game.update(&input)
    }
    fn slot(n: usize) -> Input {
        Input {
            slot: Some(n),
            ..Input::default()
        }
    }
    fn primary() -> Input {
        Input {
            primary: true,
            ..Input::default()
        }
    }

    #[test]
    fn progress_counts_six_charters_then_the_homecoming() {
        let mut game = Game::new();
        assert_eq!(game.progress(), Some((1, 7)));
        assert_eq!(game.hud().stage, "Bread for the Beacon");
        for tick in 0..40_000 {
            let input = game.bot_input(tick);
            game.update(&input);
            if game.charter == CHARTERS.len() && game.outcome() == Outcome::Playing {
                assert_eq!(game.progress(), Some((7, 7)));
                assert_eq!(game.hud().stage, "Homecoming");
            }
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Won);
        assert_eq!(game.progress(), Some((7, 7)));
        assert!(game.summary().ends_with("% hull."));
        assert_eq!(game.summary().matches(". ").count(), 0, "one sentence");
    }

    #[test]
    fn hud_objective_stays_short() {
        let game = Game::new();
        let hud = game.hud();
        assert_eq!(hud.objective, "Deliver 3 Grain to Brinewatch");
        assert!(hud.objective.split_whitespace().count() <= 5);
    }

    #[test]
    fn buying_a_crate_pops_its_price_and_fills_a_hold_slot() {
        let mut game = Game::new();
        press(&mut game, slot(2));
        let cue = press(&mut game, primary());
        assert_eq!(cue, Cue::Action);
        assert_eq!(
            game.events.iter().next(),
            Some(Event::Bought { good: 0, price: 14 })
        );
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        let counts = fx.counts();
        assert_eq!(counts.popups, 1);
        assert!(counts.sparks >= 12 && counts.rings >= 2);
        // The next tick clears the list: effects never repeat.
        press(&mut game, Input::default());
        assert_eq!(game.events.iter().count(), 0);
        let mut quiet = Juice::new();
        game.effects(Cue::None, &mut quiet);
        assert!(quiet.is_idle());
    }

    #[test]
    fn refusals_shake_and_ring_without_changing_state() {
        let mut game = Game::new();
        press(&mut game, slot(3));
        let before = (game.coins, game.cargo_total(), game.charter);
        assert_eq!(press(&mut game, primary()), Cue::None);
        assert_eq!((game.coins, game.cargo_total(), game.charter), before);
        assert_eq!(
            game.events.iter().next(),
            Some(Event::Refused(Refusal::WrongPort(1)))
        );
        let mut fx = Juice::new();
        game.effects(Cue::None, &mut fx);
        assert!(fx.counts().rings >= 2 && fx.counts().popups == 1);
        fx.update(1. / 60.);
        assert_ne!(fx.shake_offset(), Vec2::ZERO);
    }

    #[test]
    fn delivery_celebrates_at_the_port_with_its_reward() {
        let mut game = Game::new();
        for tick in 0..10_000 {
            let input = game.bot_input(tick);
            let cue = game.update(&input);
            if let Some(Event::Delivered { port, reward }) = game.events.iter().next() {
                assert_eq!((port, reward), (1, 170));
                assert_eq!(cue, Cue::Good);
                let mut fx = Juice::new();
                game.effects(cue, &mut fx);
                let counts = fx.counts();
                assert!(counts.sparks >= 48, "{counts:?}");
                assert!(counts.rings >= 3 && counts.popups >= 2, "{counts:?}");
                return;
            }
        }
        panic!("the pilot never delivered a charter");
    }

    #[test]
    fn squall_hits_are_felt_where_the_ship_is() {
        let mut game = Game::new();
        press(&mut game, slot(1));
        while game.selected_port != 3 {
            press(
                &mut game,
                Input {
                    down_pressed: true,
                    ..Input::default()
                },
            );
        }
        press(
            &mut game,
            Input {
                secondary: true,
                ..Input::default()
            },
        );
        assert!(!game.sheltered);
        press(&mut game, primary());
        for _ in 0..5_000 {
            let cue = press(&mut game, Input::default());
            if let Some(Event::Squall { damage, at }) = game
                .events
                .iter()
                .find(|e| matches!(e, Event::Squall { .. }))
            {
                assert!(damage > 0 && at.x.is_finite());
                // Docking on the same tick may turn the cue Good; the hit
                // is still shown where the ship is.
                let mut fx = Juice::new();
                game.effects(cue, &mut fx);
                assert!(fx.counts().sparks >= 30 && fx.counts().popups >= 1);
                return;
            }
        }
        panic!("no squall struck the open-sea passage");
    }

    #[test]
    fn stack_labels_format_without_allocating() {
        assert_eq!(small!("-{}", 17).as_str(), "-17");
        assert_eq!(small!("{:+}", -4).as_str(), "-4");
        // Overflow keeps the label valid instead of panicking.
        let long = small!("{}", "x".repeat(64));
        assert!(long.as_str().len() <= 32);
    }
}
