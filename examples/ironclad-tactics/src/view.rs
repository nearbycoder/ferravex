//! Presentation for Ironclad Tactics: a riveted deck board, tracked squad
//! machines, telegraphed enemy fire and a command sidebar.
//!
//! Everything here reads the model; nothing writes it. Pointer hit regions live
//! in the model, so every clickable surface below is drawn inside them.
use crate::content::{COLS, Objective, ROWS};
use crate::model::{
    BOARD_X, BOARD_Y, CARD_Y, Game, Intent, Kind, Phase, Pos, Role, TILE_SIZE, Tile,
};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;
use std::f32::consts::FRAC_1_SQRT_2;

const GOLD: Color = Color::new(0.95, 0.70, 0.29, 1.);
const TEAL: Color = Color::new(0.30, 0.79, 0.75, 1.);
const RED: Color = Color::new(0.98, 0.40, 0.34, 1.);
const BLUE: Color = Color::new(0.48, 0.65, 0.96, 1.);
const DARK: Color = Color::new(0.07, 0.10, 0.13, 1.);
const SOFT: Color = Color::new(0.80, 0.85, 0.89, 1.);
/// Command rows share the model's click bands.
const SIDE_X: f32 = 600.;
const SIDE_W: f32 = 332.;
const ROWS_Y: [f32; 3] = [401., 442., 483.];

fn role_color(role: Role) -> Color {
    match role {
        Role::Vanguard => GOLD,
        Role::Scout => TEAL,
        Role::Engineer => BLUE,
    }
}
/// Title-case display name for the squad's all-caps call signs.
fn call_sign(role: Role) -> &'static str {
    match role {
        Role::Vanguard => "Brass",
        Role::Scout => "Kestrel",
        Role::Engineer => "Mica",
    }
}

// --- Text: flat cards need no drop shadow, which halves glyph geometry. ---

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
/// Shrink a label to `width`, never enlarging it.
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
/// Small uppercase letter-spaced section header.
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    let s = TextStyle::modern(11., color).with_letter_spacing(1.5);
    draw_text_styled(value, x, y, s).width
}
/// Wrapped paragraph limited to `max_lines`; returns the next baseline.
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

// --- Small shape vocabulary. ---

/// Four L-shaped corner brackets around a square of half-size `r`.
fn brackets(c: Vec2, r: f32, arm: f32, t: f32, color: Color) {
    for (sx, sy) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
        let x = c.x + sx * r;
        let y = c.y + sy * r;
        let hx = if sx < 0. { x } else { x - arm };
        let vy = if sy < 0. { y } else { y - arm };
        panel(hx, if sy < 0. { y } else { y - t }, arm, t, color);
        panel(if sx < 0. { x } else { x - t }, vy, t, arm, color);
    }
}
/// Unit 12-gon (30 degree steps). Tokens are drawn from this table so the
/// per-frame board never evaluates trigonometry.
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
/// Unit octagon rotated 22.5 degrees (flat top).
const OCTAGON: [(f32, f32); 8] = [
    (0.923_879_5, 0.382_683_4),
    (0.382_683_4, 0.923_879_5),
    (-0.382_683_4, 0.923_879_5),
    (-0.923_879_5, 0.382_683_4),
    (-0.923_879_5, -0.382_683_4),
    (-0.382_683_4, -0.923_879_5),
    (0.382_683_4, -0.923_879_5),
    (0.923_879_5, -0.382_683_4),
];
/// Convex fan over `points` (unit outline, any length), scaled by `rx`/`ry`.
fn fan(x: f32, y: f32, rx: f32, ry: f32, points: &[(f32, f32)], step: usize, color: Color) {
    let n = points.len() / step;
    let at = |k: usize| {
        let (u, v) = points[(k % n) * step];
        vec2(x + u * rx, y + v * ry)
    };
    let center = vec2(x, y);
    for k in 0..n {
        draw_triangle(center, at(k), at(k + 1), color);
    }
}
/// Filled disc; a 12-gon reads as round at token sizes.
fn dot(x: f32, y: f32, r: f32, color: Color) {
    fan(x, y, r, r, &RING, 1, color);
}
/// Flattened disc for ground shadows.
fn oval(x: f32, y: f32, rx: f32, ry: f32, color: Color) {
    fan(x, y, rx, ry, &RING, 1, color);
}
/// Pointy-top hexagon.
fn hexagon(x: f32, y: f32, r: f32, color: Color) {
    fan(x, y, r, r, &RING[1..], 2, color);
}
fn octagon(x: f32, y: f32, r: f32, color: Color) {
    fan(x, y, r, r, &OCTAGON, 1, color);
}
/// Capsule of height `h` from flat fills and two end discs.
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
/// Hull bar on an opaque track (translucent ends would double-blend).
fn bar(x: f32, y: f32, w: f32, h: f32, frac: f32, color: Color) {
    pill(x, y, w, h, Color::new(0.03, 0.04, 0.055, 1.));
    let f = if frac.is_finite() {
        frac.clamp(0., 1.)
    } else {
        0.
    };
    if f > 0. {
        pill(x, y, (w * f).max(h), h, color);
    }
}
/// Model log lines use all-caps call signs; show them as names.
fn recase(note: &str) -> String {
    note.replace("BRASS", "Brass")
        .replace("KESTREL", "Kestrel")
        .replace("MICA", "Mica")
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
fn tile_rect(p: Pos) -> (f32, f32) {
    (
        BOARD_X + p.x as f32 * TILE_SIZE,
        BOARD_Y + p.y as f32 * TILE_SIZE,
    )
}
fn unit_dir(from: Vec2, to: Vec2, fallback: Vec2) -> Vec2 {
    let d = to - from;
    if d.length_squared() > 1. {
        d.normalize()
    } else {
        fallback
    }
}

impl Game {
    fn wall(&self, x: i32, y: i32) -> bool {
        let p = Pos::new(x, y);
        p.inside() && self.tiles[p.index()] == Tile::Wall
    }
    fn draw_header(&self) {
        let name = self.content().name;
        let w = label(name, 28., 139., 19., PAPER);
        let label_tag = match self.content().objective {
            Objective::Eliminate => "Eliminate",
            Objective::Rescue => "Rescue",
            Objective::Capture => "Capture",
            Objective::Hold => "Hold",
            Objective::Sabotage => "Sabotage",
            Objective::Liberate => "Liberate",
        };
        tag(label_tag, 28. + w + 12., 124., 12., GOLD);
        let limit = self.content().limit;
        let late = self.round + 3 > limit;
        let value = format!("{} / {}", self.round, limit);
        let vw = label_right(&value, 578., 139., 17., if late { RED } else { PAPER });
        label_right("Turn", 578. - vw - 8., 139., 13., MUTED);
    }
    fn draw_board(&self, path: Option<&[Pos]>) {
        plate(20., 146., 566., 416., Color::new(0.045, 0.06, 0.08, 1.));
        if self.phase != Phase::Battle {
            // The dossier card covers the whole deck.
            return;
        }
        // Deck plates: alternating tone, a top bevel and two bolts each.
        for y in 0..ROWS {
            for x in 0..COLS {
                let p = Pos::new(x as i32, y as i32);
                if self.tiles[p.index()] == Tile::Wall {
                    continue;
                }
                let (px, py) = tile_rect(p);
                let base = if (x + y) % 2 == 0 {
                    Color::new(0.115, 0.155, 0.18, 1.)
                } else {
                    Color::new(0.13, 0.17, 0.195, 1.)
                };
                panel(px + 1., py + 1., 48., 48., base);
                panel(px + 1., py + 1., 48., 1., Color::new(1., 1., 1., 0.05));
                panel(px + 1., py + 48., 48., 1., Color::new(0., 0., 0., 0.28));
                panel(px + 4., py + 4., 2., 2., Color::new(0.25, 0.3, 0.33, 1.));
                panel(px + 44., py + 44., 2., 2., Color::new(0.06, 0.08, 0.1, 1.));
            }
        }
        // Bulkheads join their neighbours into continuous steel.
        let steel = Color::new(0.165, 0.2, 0.225, 1.);
        let face = Color::new(0.25, 0.29, 0.32, 1.);
        for y in 0..ROWS as i32 {
            for x in 0..COLS as i32 {
                if !self.wall(x, y) {
                    continue;
                }
                let (px, py) = tile_rect(Pos::new(x, y));
                panel(px, py, 50., 50., steel);
                if !self.wall(x, y - 1) {
                    panel(px, py, 50., 5., face);
                    panel(px, py, 50., 1., Color::new(1., 1., 1., 0.14));
                }
                if !self.wall(x, y + 1) {
                    panel(px, py + 42., 50., 8., Color::new(0.095, 0.115, 0.13, 1.));
                    if y + 1 < ROWS as i32 {
                        panel(px, py + 50., 50., 7., Color::new(0., 0., 0., 0.22));
                    }
                }
                if !self.wall(x - 1, y) {
                    panel(px, py, 1., 50., Color::new(1., 1., 1., 0.08));
                } else {
                    panel(px, py + 6., 1., 36., Color::new(0., 0., 0., 0.3));
                }
                let rivet = Color::new(0.3, 0.35, 0.38, 1.);
                panel(px + 7., py + 12., 3., 3., rivet);
                panel(px + 40., py + 32., 3., 3., rivet);
            }
        }
        // Low cover: hazard-striped barricades with a ground shadow.
        for y in 0..ROWS {
            for x in 0..COLS {
                let p = Pos::new(x as i32, y as i32);
                if self.tiles[p.index()] != Tile::Cover {
                    continue;
                }
                let (px, py) = tile_rect(p);
                panel(px + 6., py + 41., 40., 4., Color::new(0., 0., 0., 0.35));
                slab(
                    px + 5.,
                    py + 27.,
                    40.,
                    15.,
                    3.,
                    Color::new(0.3, 0.33, 0.28, 1.),
                );
                slab(
                    px + 5.,
                    py + 27.,
                    40.,
                    8.,
                    3.,
                    Color::new(0.4, 0.43, 0.36, 1.),
                );
                for i in 0..4 {
                    let sx = px + 9. + i as f32 * 9.;
                    draw_line(sx, py + 39., sx + 6., py + 30., 3., with_alpha(GOLD, 0.85));
                }
                panel(px + 6., py + 28., 38., 1., Color::new(1., 1., 1., 0.18));
            }
        }
        self.draw_objectives();
        if let Some(path) = path {
            let affordable =
                path.len().saturating_sub(1) <= usize::from(self.units[self.selected].ap);
            let color = if affordable { TEAL } else { MUTED };
            for pair in path.windows(2) {
                let (a, b) = (pair[0].center(), pair[1].center());
                draw_line(a.x, a.y, b.x, b.y, 2., with_alpha(color, 0.35));
            }
            for p in path.iter().skip(1) {
                let c = p.center();
                dot(c.x, c.y, 3.5, color);
            }
        }
        self.draw_intents();
        self.draw_enemies();
        self.draw_units();
        if self.flash > 0
            && let Some((a, b, hostile)) = self.shot
        {
            let (a, b) = (a.center(), b.center());
            let k = f32::from(self.flash) / 18.;
            let color = if hostile { RED } else { PAPER };
            draw_line(a.x, a.y, b.x, b.y, 6., with_alpha(color, 0.25 * k));
            draw_line(a.x, a.y, b.x, b.y, 2., with_alpha(color, k.min(1.)));
            glow(
                b.x,
                b.y,
                22.,
                with_alpha(if hostile { RED } else { GOLD }, 0.8 * k),
            );
        }
        if self.phase == Phase::Battle {
            let c = self.cursor.center();
            panel(c.x - 23., c.y - 23., 46., 46., with_alpha(TEAL, 0.08));
            brackets(c, 24., 10., 2., TEAL);
        }
        self.draw_forecast_labels();
    }
    fn draw_objectives(&self) {
        for (i, p) in self.objectives.iter().enumerate() {
            let Some(p) = p else { continue };
            let c = p.center();
            let done = self.completed[i];
            let color = if done { TEAL } else { GOLD };
            draw_circle(c.x, c.y, 18., with_alpha(color, 0.16));
            draw_circle(c.x, c.y, 13., with_alpha(color, 0.14));
            draw_circle_lines(c.x, c.y, 18., 2., color);
            dot(c.x, c.y, 9., DARK);
            label_center(if i == 0 { "O" } else { "P" }, c.x, c.y + 5., 13., color);
        }
        if self.content().objective == Objective::Rescue {
            let c = self.extraction.center();
            panel(c.x - 21., c.y - 21., 42., 42., with_alpha(TEAL, 0.12));
            brackets(c, 21., 12., 3., TEAL);
            label_center("E", c.x, c.y + 6., 16., TEAL);
        }
    }
    fn draw_intents(&self) {
        for enemy in self.enemies.iter().flatten().filter(|e| e.hp > 0) {
            let c = enemy.pos.center();
            if !enemy.active {
                // Reserve entry marker.
                panel(c.x - 22., c.y - 22., 44., 44., with_alpha(RED, 0.07));
                brackets(c, 22., 8., 2., with_alpha(RED, 0.75));
                draw_circle_lines(c.x, c.y - 6., 6., 1.5, with_alpha(RED, 0.8));
                dot(c.x, c.y - 6., 2., RED);
                label_center(&format!("Turn {}", enemy.arrives), c.x, c.y + 14., 10., RED);
                continue;
            }
            match enemy.intent {
                Intent::Fire(to) => {
                    let d = to.center();
                    draw_line(c.x, c.y, d.x, d.y, 5., Color::new(0.98, 0.3, 0.26, 0.14));
                    draw_line(c.x, c.y, d.x, d.y, 1.5, Color::new(0.98, 0.4, 0.34, 0.75));
                    panel(d.x - 22., d.y - 22., 44., 44., with_alpha(RED, 0.16));
                    brackets(d, 22., 10., 2., RED);
                    draw_line(
                        d.x - 7.,
                        d.y - 7.,
                        d.x + 7.,
                        d.y + 7.,
                        2.,
                        with_alpha(RED, 0.7),
                    );
                    draw_line(
                        d.x + 7.,
                        d.y - 7.,
                        d.x - 7.,
                        d.y + 7.,
                        2.,
                        with_alpha(RED, 0.7),
                    );
                }
                Intent::Move(to) => {
                    let d = to.center();
                    let dir = unit_dir(c, d, Vec2::X);
                    let tip = d - dir * 6.;
                    let side = vec2(-dir.y, dir.x);
                    draw_line(
                        c.x,
                        c.y,
                        tip.x - dir.x * 6.,
                        tip.y - dir.y * 6.,
                        3.,
                        with_alpha(GOLD, 0.8),
                    );
                    draw_triangle(
                        tip,
                        tip - dir * 10. + side * 7.,
                        tip - dir * 10. - side * 7.,
                        GOLD,
                    );
                    dot(d.x, d.y, 4., with_alpha(GOLD, 0.45));
                }
                Intent::Wait => {}
            }
        }
    }
    /// Expected hit on each targeted tile, after cover and brace.
    fn draw_forecast_labels(&self) {
        for enemy in self.enemies.iter().flatten() {
            if enemy.hp == 0 || !enemy.active {
                continue;
            }
            let Intent::Fire(to) = enemy.intent else {
                continue;
            };
            let mut damage = enemy.kind.damage();
            if let Some(i) = self.friendly(to) {
                let cover = u8::from(self.tiles[to.index()] == Tile::Cover);
                let brace = if self.units[i].braced { 2 } else { 0 };
                damage = damage.saturating_sub(cover + brace).max(1);
            }
            let (px, py) = tile_rect(to);
            let text = format!("-{damage}");
            let w = text_width(&text, TextStyle::modern(12., PAPER)) + 8.;
            slab(
                px + 48. - w,
                py + 2.,
                w,
                15.,
                4.,
                Color::new(0.55, 0.12, 0.1, 0.95),
            );
            label_center(&text, px + 48. - w * 0.5, py + 14., 12., PAPER);
        }
    }
    fn draw_enemies(&self) {
        for enemy in self
            .enemies
            .iter()
            .flatten()
            .filter(|e| e.active && e.hp > 0)
        {
            let c = enemy.pos.center();
            let aim = match enemy.intent {
                Intent::Fire(to) | Intent::Move(to) => unit_dir(c, to.center(), -Vec2::X),
                Intent::Wait => -Vec2::X,
            };
            let hull = Color::new(0.33, 0.13, 0.14, 1.);
            let plate = Color::new(0.52, 0.2, 0.2, 1.);
            let big = enemy.kind == Kind::Warden;
            oval(
                c.x + 2.,
                c.y + 15.,
                if big { 21. } else { 17. },
                5.,
                Color::new(0., 0., 0., 0.4),
            );
            match enemy.kind {
                Kind::Gunner => {
                    hexagon(c.x, c.y, 16., RED);
                    hexagon(c.x, c.y, 14., hull);
                    hexagon(c.x, c.y - 1., 10., plate);
                }
                Kind::Bruiser => {
                    slab(c.x - 15., c.y - 13., 30., 27., 5., RED);
                    slab(c.x - 13., c.y - 11., 26., 23., 4., hull);
                    // Ram plate on the side it is heading for.
                    let side = vec2(-aim.y, aim.x);
                    let front = c + aim * 15.;
                    draw_triangle(
                        front + aim * 6.,
                        front + side * 12.,
                        front - side * 12.,
                        RED,
                    );
                    panel(c.x - 8., c.y - 4., 16., 3., Color::new(1., 0.75, 0.4, 0.9));
                }
                Kind::Sentry => {
                    for leg in [(-14., 14.), (14., 14.), (0., -15.)] {
                        draw_line(
                            c.x,
                            c.y,
                            c.x + leg.0,
                            c.y + leg.1,
                            4.,
                            Color::new(0.24, 0.12, 0.13, 1.),
                        );
                    }
                    dot(c.x, c.y, 12., RED);
                    dot(c.x, c.y, 10., hull);
                }
                Kind::Warden => {
                    octagon(c.x, c.y, 21., RED);
                    octagon(c.x, c.y, 19., hull);
                    octagon(c.x, c.y, 13., plate);
                    for (u, v) in [(1., 1.), (-1., 1.), (-1., -1.), (1., -1.)] {
                        dot(c.x + u * 12., c.y + v * 12., 2.5, GOLD);
                    }
                }
            }
            // Turret and barrel aimed at the announced target.
            let side = vec2(-aim.y, aim.x);
            let reach = if enemy.kind == Kind::Sentry { 20. } else { 16. };
            let twin = if big { 3.5 } else { 0. };
            for s in if big { [-1., 1.] } else { [0., 0.] } {
                let o = c + side * (s * twin);
                let tip = o + aim * reach;
                draw_line(o.x, o.y, tip.x, tip.y, 4., Color::new(0.16, 0.07, 0.08, 1.));
                draw_line(o.x, o.y, tip.x, tip.y, 2., Color::new(0.75, 0.32, 0.3, 1.));
            }
            dot(c.x, c.y, 6., Color::new(0.2, 0.08, 0.09, 1.));
            dot(c.x - 1., c.y - 1., 4., Color::new(1., 0.55, 0.45, 1.));
            // Hull strip with the exact value.
            let frac = f32::from(enemy.hp) / f32::from(enemy.kind.hull());
            panel(c.x - 17., c.y + 19., 26., 4., Color::new(0., 0., 0., 0.6));
            panel(c.x - 17., c.y + 19., 26. * frac.min(1.), 4., RED);
            label(
                &enemy.hp.to_string(),
                c.x + 12.,
                c.y + 24.,
                10.,
                Color::new(1., 0.72, 0.66, 1.),
            );
        }
    }
    fn draw_units(&self) {
        for (i, unit) in self.units.iter().enumerate() {
            let c = unit.pos.center();
            let color = role_color(unit.role);
            if unit.hp == 0 {
                oval(c.x + 2., c.y + 13., 16., 4., Color::new(0., 0., 0., 0.35));
                slab(
                    c.x - 14.,
                    c.y - 11.,
                    28.,
                    24.,
                    4.,
                    Color::new(0.2, 0.22, 0.24, 1.),
                );
                draw_line(
                    c.x - 8.,
                    c.y - 6.,
                    c.x + 8.,
                    c.y + 8.,
                    3.,
                    Color::new(0.08, 0.1, 0.12, 1.),
                );
                draw_line(
                    c.x + 8.,
                    c.y - 6.,
                    c.x - 8.,
                    c.y + 8.,
                    3.,
                    Color::new(0.08, 0.1, 0.12, 1.),
                );
                continue;
            }
            let selected = self.selected == i && self.phase == Phase::Battle;
            if selected {
                draw_circle(c.x, c.y, 24., with_alpha(color, 0.12));
                draw_circle(c.x, c.y, 18., with_alpha(color, 0.14));
                draw_circle_lines(c.x, c.y, 23., 1.5, with_alpha(lighten(color, 0.5), 0.9));
            }
            oval(c.x + 2., c.y + 16., 19., 5., Color::new(0., 0., 0., 0.42));
            // Treads.
            let tread = Color::new(0.1, 0.12, 0.14, 1.);
            panel(c.x - 19., c.y - 13., 8., 28., tread);
            panel(c.x + 11., c.y - 13., 8., 28., tread);
            panel(c.x - 18., c.y - 14., 6., 30., tread);
            panel(c.x + 12., c.y - 14., 6., 30., tread);
            for k in 0..4 {
                let ty = c.y - 10. + k as f32 * 7.;
                panel(c.x - 18., ty, 6., 2., Color::new(0.3, 0.33, 0.36, 1.));
                panel(c.x + 12., ty, 6., 2., Color::new(0.3, 0.33, 0.36, 1.));
            }
            // Hull with a lit top edge.
            slab(c.x - 12., c.y - 13., 24., 28., 5., darken(color, 0.3));
            slab(c.x - 12., c.y - 13., 24., 19., 5., lighten(color, 0.08));
            panel(c.x - 9., c.y - 12., 18., 1., Color::new(1., 1., 1., 0.45));
            // Turret faces the cursor while selected, otherwise down-range.
            let aim = if selected {
                unit_dir(c, self.cursor.center(), Vec2::X)
            } else {
                Vec2::X
            };
            let tip = c + aim * 20.;
            draw_line(c.x, c.y - 1., tip.x, tip.y - 1., 5., DARK);
            draw_line(c.x, c.y - 1., tip.x, tip.y - 1., 2.5, lighten(color, 0.35));
            dot(c.x, c.y - 1., 8., darken(color, 0.55));
            dot(c.x, c.y - 1., 6.5, lighten(color, 0.1));
            dot(c.x - 2., c.y - 3., 2., Color::new(1., 1., 1., 0.55));
            let b = c + vec2(-18., -18.);
            dot(b.x, b.y, 7.5, darken(color, 0.25));
            label_center(&(i + 1).to_string(), b.x, b.y + 4., 10., DARK);
            if unit.braced {
                draw_circle_lines(c.x, c.y, 21., 2.5, with_alpha(GOLD, 0.9));
            }
            if unit.focused {
                let p = c + vec2(15., -17.);
                dot(p.x, p.y, 5., DARK);
                dot(p.x, p.y, 3.8, TEAL);
                dot(p.x, p.y, 2.4, DARK);
                dot(p.x, p.y, 1.2, TEAL);
            }
            if self.carrier == Some(i) {
                let p = c + vec2(-16., -17.);
                dot(p.x, p.y, 6., PAPER);
                panel(p.x - 3.5, p.y - 1., 7., 2., RED);
                panel(p.x - 1., p.y - 3.5, 2., 7., RED);
            }
            let frac = f32::from(unit.hp) / f32::from(self.max_hull(unit.role));
            panel(c.x - 9., c.y + 20., 27., 3., Color::new(0., 0., 0., 0.6));
            panel(c.x - 9., c.y + 20., 27. * frac.min(1.), 3., color);
        }
    }
    fn draw_squad_header(&self) {
        eyebrow("SQUAD", SIDE_X, 140., MUTED);
        let mut right = SIDE_X + SIDE_W;
        let retries = match self.retries {
            1 => "1 retry".to_owned(),
            n => format!("{n} retries"),
        };
        right -= tag_width(&retries, 12.);
        tag(&retries, right, 125., 12., MUTED);
        let (hostiles, color) = match self.remaining() {
            0 => ("Area clear".to_owned(), TEAL),
            1 => ("1 hostile".to_owned(), RED),
            n => (format!("{n} hostiles"), RED),
        };
        right -= tag_width(&hostiles, 12.) + 8.;
        tag(&hostiles, right, 125., 12., color);
    }
    fn draw_squad(&self) {
        for (i, unit) in self.units.iter().enumerate() {
            let y = CARD_Y + i as f32 * 55.;
            let color = role_color(unit.role);
            let alive = unit.hp > 0;
            let selected = i == self.selected;
            let fill = if selected {
                Color::new(0.14, 0.19, 0.23, 1.)
            } else {
                Color::new(0.085, 0.115, 0.145, 1.)
            };
            if selected {
                framed(
                    Rect::new(SIDE_X, y, SIDE_W, 50.),
                    8.,
                    1.5,
                    darken(color, 0.25),
                    fill,
                );
            } else {
                slab(SIDE_X, y, SIDE_W, 50., 8., fill);
            }
            panel(
                SIDE_X + 8.,
                y + 10.,
                3.,
                30.,
                if alive { color } else { MUTED },
            );
            label(&(i + 1).to_string(), SIDE_X + 20., y + 21., 12., MUTED);
            let name_color = if alive { color } else { MUTED };
            let nw = label(call_sign(unit.role), SIDE_X + 34., y + 21., 16., name_color);
            label(
                unit.role.class(),
                SIDE_X + 34. + nw + 8.,
                y + 21.,
                12.,
                MUTED,
            );
            if !alive {
                label_right("Disabled", SIDE_X + SIDE_W - 14., y + 21., 12., RED);
            } else {
                // Action points as pips, right aligned.
                let max = self.max_ap(unit.role);
                for k in 0..max {
                    let px = SIDE_X + SIDE_W - 14. - (max - k) as f32 * 11. + 2.;
                    let on = k < unit.ap;
                    panel(
                        px,
                        y + 12.,
                        8.,
                        10.,
                        if on {
                            color
                        } else {
                            Color::new(1., 1., 1., 0.1)
                        },
                    );
                    if on {
                        panel(px, y + 12., 8., 1., Color::new(1., 1., 1., 0.35));
                    }
                }
                label_right(
                    "AP",
                    SIDE_X + SIDE_W - 18. - f32::from(max) * 11.,
                    y + 21.,
                    11.,
                    MUTED,
                );
            }
            let max_hull = self.max_hull(unit.role);
            let frac = f32::from(unit.hp) / f32::from(max_hull);
            let hull_color = if frac <= 0.34 { RED } else { color };
            bar(SIDE_X + 34., y + 32., 196., 6., frac, hull_color);
            let tag = if !alive {
                None
            } else if self.carrier == Some(i) {
                Some(("Operator", PAPER))
            } else if unit.braced {
                Some(("Braced", GOLD))
            } else if unit.focused {
                Some(("Focused", TEAL))
            } else {
                None
            };
            if let Some((tag, c)) = tag {
                label(tag, SIDE_X + 238., y + 40., 11., c);
            }
            label_right(
                &format!("{}/{}", unit.hp, max_hull),
                SIDE_X + SIDE_W - 14.,
                y + 40.,
                12.,
                SOFT,
            );
        }
    }
    fn draw_target(&self, path: Option<&[Pos]>) {
        let u = self.units[self.selected];
        eyebrow("TARGET", SIDE_X, 338., MUTED);
        let (line, line_color, verdict, verdict_color) =
            if let Some(index) = self.hostile(self.cursor) {
                let e = self.enemies[index].unwrap();
                let distance = u.pos.distance(e.pos);
                let (verdict, ok) = if distance > u.role.range() {
                    ("Out of range".to_owned(), false)
                } else if !self.visible(u.pos, e.pos) {
                    ("No line of sight".to_owned(), false)
                } else if u.ap < 2 {
                    ("Needs 2 AP".to_owned(), false)
                } else {
                    (
                        format!("Hits for {}", self.shot_damage(self.selected, index)),
                        true,
                    )
                };
                (
                    format!("{} · {} hull · {} tiles", e.kind.name(), e.hp, distance),
                    RED,
                    verdict,
                    if ok { GOLD } else { MUTED },
                )
            } else if let Some(i) = self.friendly(self.cursor) {
                let ally = self.units[i];
                (
                    format!("{} · {} hull", call_sign(ally.role), ally.hp),
                    role_color(ally.role),
                    if i == self.selected {
                        "Selected"
                    } else {
                        "Ally"
                    }
                    .to_owned(),
                    MUTED,
                )
            } else if self.objectives.contains(&Some(self.cursor)) {
                (
                    "Objective · interact from adjacent".to_owned(),
                    GOLD,
                    "2 AP".to_owned(),
                    MUTED,
                )
            } else if let Some(path) = path {
                let cost = path.len().saturating_sub(1);
                let ok = cost <= usize::from(u.ap);
                (
                    format!("Move · {} tile{}", cost, if cost == 1 { "" } else { "s" }),
                    if ok { TEAL } else { MUTED },
                    if ok {
                        format!("{cost} of {} AP", u.ap)
                    } else {
                        format!("Needs {cost} AP")
                    },
                    if ok { SOFT } else { RED },
                )
            } else {
                (
                    "Blocked route or wall".to_owned(),
                    MUTED,
                    String::new(),
                    MUTED,
                )
            };
        let vw = text_width(&verdict, TextStyle::modern(12., PAPER));
        label_fit(&line, SIDE_X, 359., SIDE_W - vw - 12., 15., line_color);
        label_right(&verdict, SIDE_X + SIDE_W, 359., 12., verdict_color);
        let mut stats = format!(
            "Range {} · Cannon {} · Kits {}",
            u.role.range(),
            u.role.damage() + self.upgrades[1],
            self.kits
        );
        if matches!(
            self.content().objective,
            Objective::Capture | Objective::Hold | Objective::Liberate
        ) {
            stats.push_str(&format!(" · Guard {}", self.held));
        }
        label_fit(&stats, SIDE_X, 381., SIDE_W, 12., MUTED);
    }
    fn draw_orders(&self, path: Option<&[Pos]>) {
        let u = self.units[self.selected];
        let (order, cost) = if self.hostile(self.cursor).is_some() {
            ("Fire cannon", "2 AP")
        } else if self
            .friendly(self.cursor)
            .is_some_and(|f| f != self.selected)
        {
            ("Select ally", "Free")
        } else if self.objectives.contains(&Some(self.cursor)) {
            ("Interact", "2 AP")
        } else if path.is_some_and(|p| p.len() > 1) {
            ("Move here", "1 AP / tile")
        } else {
            ("Execute order", "")
        };
        let (skill, skill_cost) = match u.role {
            Role::Vanguard => ("Brace", "1 AP · -2 per hit"),
            Role::Scout => ("Focus", "1 AP · +2 damage"),
            Role::Engineer => ("Repair ally", "2 AP · 1 kit"),
        };
        let rows = [
            ("Space", order, cost, TEAL),
            ("X", skill, skill_cost, role_color(u.role)),
            ("C", "End squad turn", "Enemies act", GOLD),
        ];
        for (i, (cap, action, detail, accent)) in rows.into_iter().enumerate() {
            let y = ROWS_Y[i];
            let end = i == 2;
            let fill = if end {
                Color::new(0.2, 0.16, 0.09, 1.)
            } else {
                Color::new(0.1, 0.135, 0.165, 1.)
            };
            slab(SIDE_X, y, SIDE_W, 34., 8., fill);
            panel(SIDE_X, y + 8., 3., 18., accent);
            key(
                cap,
                action,
                SIDE_X + 12.,
                y + 7.,
                if end { GOLD } else { PAPER },
            );
            if !detail.is_empty() {
                label_right(detail, SIDE_X + SIDE_W - 12., y + 22., 12., MUTED);
            }
        }
        // Latest log line, wrapped to the sidebar's foot.
        dot(SIDE_X + 3., 534., 2.5, GOLD);
        paragraph(
            &recase(&self.notes[0]),
            SIDE_X + 12.,
            538.,
            SIDE_W - 12.,
            12.,
            2,
            SOFT,
        );
    }
    fn overlay(&self) {
        if self.phase == Phase::Battle {
            return;
        }
        card(28., 154., 550., 400., Color::new(0.075, 0.1, 0.13, 1.));
        let x = 52.;
        let w = 502.;
        match self.phase {
            Phase::Briefing => {
                stripe(GOLD);
                eyebrow(
                    &format!("OPERATION {} OF 6  ·  BRIEFING", self.mission()),
                    x,
                    190.,
                    GOLD,
                );
                label_fit(self.content().name, x, 226., w, 28., PAPER);
                let y = paragraph(self.content().story, x, 254., w, 15., 2, SOFT);
                divider(x, y - 4., w, Color::new(1., 1., 1., 0.12));
                eyebrow("OBJECTIVE", x, y + 18., MUTED);
                let y = paragraph(self.content().order, x, y + 40., w, 16., 2, TEAL);
                let mut cx = x;
                for (tag, color) in [
                    (format!("{} turn deadline", self.content().limit), GOLD),
                    (format!("{} hostiles", self.remaining()), RED),
                    ("Full repair · 2 kits".to_owned(), BLUE),
                ] {
                    cx += chip(&tag, cx, y - 2., 12., color) + 8.;
                }
                rounded(x, y + 26., w, 70., 8., Color::new(1., 1., 1., 0.04));
                rounded(x, y + 38., 3., 46., 1.5, GOLD);
                eyebrow("FIELD TIP", x + 16., y + 50., GOLD);
                paragraph(self.content().tip, x + 16., y + 72., w - 32., 14., 2, SOFT);
                let mut kx = x;
                for (key, action) in [
                    ("1-3", "Select"),
                    ("Arrows", "Aim"),
                    ("Space", "Order"),
                    ("C", "End turn"),
                ] {
                    kx += keycap(key, action, kx, 466., SOFT) + 18.;
                }
                rounded_gradient(x, 504., w, 36., 9., lighten(GOLD, 0.08), darken(GOLD, 0.12));
                let s = TextStyle::modern(16., INK).with_align(TextAlign::Center);
                draw_text_styled("Deploy squad", x + w * 0.5, s.centered_baseline(522.), s);
                label_right(
                    "Space or click",
                    x + w - 14.,
                    527.,
                    12.,
                    with_alpha(INK, 0.7),
                );
            }
            Phase::Debrief => {
                stripe(TEAL);
                eyebrow("OPERATION SECURED", x, 190., TEAL);
                label_fit(
                    &format!("{} cleared", self.content().name),
                    x,
                    224.,
                    w,
                    26.,
                    PAPER,
                );
                let medals = self.medals[self.mission_index];
                let mut sx = x;
                for k in 0..3 {
                    let lit = k < medals;
                    sx += label(
                        "★",
                        sx,
                        258.,
                        20.,
                        if lit {
                            GOLD
                        } else {
                            Color::new(1., 1., 1., 0.15)
                        },
                    ) + 6.;
                }
                label(
                    &format!("Campaign score {}", self.score),
                    sx + 10.,
                    255.,
                    14.,
                    SOFT,
                );
                label(
                    "Repair the squad and choose one permanent upgrade.",
                    x,
                    296.,
                    14.,
                    SOFT,
                );
                eyebrow("CHOOSE 1, 2 OR 3  ·  TWO RANKS EACH", x, 330., MUTED);
                for i in 0..3 {
                    let cx = 46. + i as f32 * 174.;
                    let open = self.upgrades[i] < 2;
                    let color = if open { GOLD } else { MUTED };
                    rounded(cx, 355., 160., 123., 10., Color::new(0.12, 0.16, 0.195, 1.));
                    outline(
                        cx,
                        355.,
                        160.,
                        123.,
                        10.,
                        1.,
                        with_alpha(color, if open { 0.6 } else { 0.25 }),
                    );
                    keycap(&(i + 1).to_string(), "", cx + 12., 367., PAPER);
                    label(["Armor", "Cannon", "Drive"][i], cx + 42., 382., 15., color);
                    label(
                        ["+3 hull", "+1 damage", "+1 AP"][i],
                        cx + 14.,
                        424.,
                        22.,
                        if open { PAPER } else { MUTED },
                    );
                    for r in 0..2u8 {
                        rounded(
                            cx + 14. + f32::from(r) * 22.,
                            452.,
                            18.,
                            6.,
                            3.,
                            if r < self.upgrades[i] {
                                color
                            } else {
                                Color::new(1., 1., 1., 0.12)
                            },
                        );
                    }
                    label_right(
                        if open { "Rank up" } else { "Maxed" },
                        cx + 146.,
                        460.,
                        11.,
                        MUTED,
                    );
                }
                label(
                    "Next operation: fully repaired with two fresh kits.",
                    x,
                    520.,
                    13.,
                    MUTED,
                );
            }
            Phase::Setback => {
                stripe(RED);
                eyebrow("OPERATION LOST", x, 190., RED);
                label_fit(self.content().name, x, 226., w, 28., PAPER);
                let y = paragraph(&recase(&self.notes[0]), x, 262., w, 15., 3, SOFT);
                paragraph(
                    "A retry reloads this operation and discards its score. Upgrades you have earned stay.",
                    x,
                    y + 8.,
                    w,
                    13.,
                    3,
                    MUTED,
                );
                chip(
                    &match self.retries {
                        1 => "1 retry remains".to_owned(),
                        n => format!("{n} retries remain"),
                    },
                    x,
                    420.,
                    13.,
                    GOLD,
                );
                rounded_gradient(x, 470., w, 36., 9., lighten(TEAL, 0.08), darken(TEAL, 0.15));
                let s = TextStyle::modern(16., INK).with_align(TextAlign::Center);
                draw_text_styled("Retry operation", x + w * 0.5, s.centered_baseline(488.), s);
                label_right("Space or X", x + w - 14., 493., 12., with_alpha(INK, 0.7));
                keycap("C", "Concede the campaign", x, 522., MUTED);
            }
            Phase::Won => {
                stripe(GOLD);
                eyebrow("LIBERATION COMPLETE", x, 190., TEAL);
                label_fit("The coast is free", x, 228., w, 30., GOLD);
                paragraph(
                    "All six operations complete. Brass, Kestrel and Mica roll home through cheering streets.",
                    x,
                    264.,
                    w,
                    15.,
                    3,
                    SOFT,
                );
                let medals: u32 = self.medals.iter().map(|m| u32::from(*m)).sum();
                eyebrow("CAMPAIGN SCORE", x, 362., MUTED);
                label(&self.score.to_string(), x, 400., 30., GOLD);
                eyebrow("MEDALS", x + 260., 362., MUTED);
                label(&format!("★ {medals} / 18"), x + 260., 400., 24., PAPER);
                keycap("R", "Launch a new campaign", x, 500., MUTED);
            }
            Phase::Lost => {
                stripe(RED);
                eyebrow("CAMPAIGN OVER", x, 190., RED);
                label_fit("The coast still waits", x, 228., w, 30., PAPER);
                paragraph("No retries remain.", x, 264., w, 15., 2, SOFT);
                eyebrow("CAMPAIGN SCORE", x, 362., MUTED);
                label(&self.score.to_string(), x, 400., 30., GOLD);
                eyebrow("REACHED", x + 260., 362., MUTED);
                label(
                    &format!("Operation {}", self.mission()),
                    x + 260.,
                    400.,
                    22.,
                    PAPER,
                );
                keycap("R", "Rebuild and begin again", x, 500., MUTED);
            }
            Phase::Battle => {}
        }
    }
}
/// Short accent stripe on an overlay card's top edge, as on host menus.
fn stripe(color: Color) {
    rounded(52., 154., 64., 3., 1.5, color);
}
impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        // One route query per frame, shared by the board and the sidebar.
        let path = if self.phase == Phase::Battle {
            self.cursor_path()
        } else {
            None
        };
        let path = path.as_deref();
        self.draw_header();
        self.draw_board(path);
        self.draw_squad_header();
        self.draw_squad();
        self.draw_target(path);
        self.draw_orders(path);
        self.overlay();
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!("Operation {}/6", self.mission()),
            objective: self.content().order.to_owned(),
            status: format!(
                "Turn {} | {} retries | {} medals",
                self.round,
                self.retries,
                self.medals.iter().map(|m| u32::from(*m)).sum::<u32>()
            ),
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        self.outcome()
    }
    fn summary(&self) -> String {
        format!(
            "{} operations liberated. {} medals. {} campaign score.",
            self.medals.iter().filter(|m| **m > 0).count(),
            self.medals.iter().map(|m| u32::from(*m)).sum::<u32>(),
            self.score
        )
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.autoplay(tick)
    }
}
