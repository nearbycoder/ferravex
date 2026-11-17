//! Read-only workshop art, the icon program editor, room results and the
//! effects that make accepted instructions felt where they happen.
use crate::layout::{self, BOARD, Button, CELL_GAP, COLUMNS, NEXT, PANEL, STATS_Y, TILE, center};
use crate::model::{CAPACITY, COLS, Event, Game, Op, Phase, ROWS, points};
use ferravex_arcade::shapes::{self, Cap, Fill, Join, Stroke};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;
use ferravex_play::navigation::Cell;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

const MINT: Color = Color::new(0.43, 0.86, 0.77, 1.);
const CREAM: Color = Color::new(0.98, 0.88, 0.65, 1.);
const COPPER: Color = Color::new(0.89, 0.55, 0.3, 1.);
const RED: Color = Color::new(0.97, 0.42, 0.4, 1.);
const BLUE: Color = Color::new(0.44, 0.67, 0.96, 1.);
const PLUM: Color = Color::new(0.24, 0.16, 0.27, 1.);
const VIOLET: Color = Color::new(0.78, 0.6, 0.95, 1.);
const GOLD: Color = Color::new(1., 0.8, 0.38, 1.);
const SOFT: Color = Color::new(0.72, 0.78, 0.80, 1.);
const DIM: Color = Color::new(0.24, 0.29, 0.34, 1.);
const SHADOW: Color = Color::new(0., 0., 0., 0.4);
const NUMBERS: [&str; 3] = ["1", "2", "3"];
/// Drawn half-size of a board tile, leaving a hairline of grout.
const HALF: f32 = TILE * 0.5 - 1.;

/// Flat text: every label here sits on a solid surface.
fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
/// Label centered on (`cx`, `cy`).
fn label_c(value: &str, cx: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color).with_align(TextAlign::Center);
    draw_text_styled(value, cx, s.centered_baseline(cy), s).width
}
/// Small key cap centered on (`cx`, `cy`).
fn mini_key(key: &str, cx: f32, cy: f32, a: f32) {
    let s = ink(10.5, with_alpha(CREAM, a)).with_align(TextAlign::Center);
    let w = (text_width(key, s) + 9.).max(15.);
    let x = cx - w * 0.5;
    rounded(x, cy - 6., w, 15., 3.5, Color::new(0., 0., 0., 0.45 * a));
    rounded_gradient(
        x,
        cy - 7.5,
        w,
        15.,
        3.5,
        Color::new(0.21, 0.25, 0.31, a),
        Color::new(0.14, 0.17, 0.22, a),
    );
    draw_text_styled(key, cx, s.centered_baseline(cy), s);
}
/// `2093` → `2,093`, matching the host's score.
fn grouped(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}
fn parcel_color(index: usize) -> Color {
    [COPPER, BLUE, MINT][index]
}
fn op_color(op: Op) -> Color {
    match op {
        Op::Forward => MINT,
        Op::Left | Op::Right => BLUE,
        Op::Grab | Op::Drop => COPPER,
        Op::Repeat2 | Op::Repeat3 => VIOLET,
    }
}
/// Filled arrowhead whose tip is at `tip`, pointing along `dir`.
fn arrow_head(tip: Vec2, dir: Vec2, length: f32, color: Color) {
    let n = vec2(-dir.y, dir.x) * length * 0.62;
    shapes::fill_polygon(
        &[tip, tip - dir * length + n, tip - dir * length - n],
        color,
    );
}
/// An instruction's pictogram in a `size` box centered on `c`.
fn glyph(op: Op, c: Vec2, size: f32, color: Color) {
    let h = size * 0.5;
    let p = |x: f32, y: f32| c + vec2(x, y) * h;
    let line = Stroke::new((size * 0.14).max(1.5), color)
        .cap(Cap::Round)
        .join(Join::Round);
    match op {
        Op::Forward => {
            shapes::polyline(&[p(0., 0.8), p(0., -0.24)], line);
            arrow_head(p(0., -0.88), vec2(0., -1.), h * 0.74, color);
        }
        Op::Left | Op::Right => {
            // Up the stem, then a quarter turn toward the new heading.
            let s = if op == Op::Left { -1. } else { 1. };
            let mut points = [Vec2::ZERO; 10];
            points[0] = p(-0.38 * s, 0.82);
            for (k, point) in points[1..].iter_mut().enumerate() {
                let a = PI + k as f32 / 8. * FRAC_PI_2;
                *point = p(s * (0.08 + 0.46 * a.cos()), 0.05 + 0.46 * a.sin());
            }
            shapes::polyline(&points, line);
            arrow_head(p(0.78 * s, -0.41), vec2(s, 0.), h * 0.72, color);
        }
        Op::Grab | Op::Drop => {
            // A parcel with an arrow lifting it or setting it down.
            let body = Rect::new(c.x - h * 0.58, c.y + h * 0.1, h * 1.16, h * 0.76);
            shapes::fill_rounded_rect(body, h * 0.14, color);
            shapes::fill_rounded_rect(
                Rect::new(c.x - h * 0.1, body.y, h * 0.2, body.h),
                0.,
                darken(color, 0.45),
            );
            if op == Op::Grab {
                shapes::polyline(&[p(0., -0.06), p(0., -0.5)], line);
                arrow_head(p(0., -0.94), vec2(0., -1.), h * 0.5, color);
            } else {
                shapes::polyline(&[p(0., -0.94), p(0., -0.5)], line);
                arrow_head(p(0., -0.06), vec2(0., 1.), h * 0.5, color);
            }
        }
        Op::Repeat2 | Op::Repeat3 => {
            let r = h * 0.68;
            let start = -FRAC_PI_2 + 0.55;
            let sweep = TAU - 1.15;
            shapes::stroke_arc(c, r, start, sweep, line);
            let end = start + sweep;
            let tangent = vec2(-end.sin(), end.cos());
            arrow_head(
                c + vec2(end.cos(), end.sin()) * r + tangent * h * 0.32,
                tangent,
                h * 0.5,
                color,
            );
            let digit = if op == Op::Repeat2 { "2" } else { "3" };
            label_c(digit, c.x + 0.5, c.y + 0.5, size * 0.52, color);
        }
    }
}
/// Play triangle and a bar: run exactly one instruction.
fn step_glyph(c: Vec2, size: f32, color: Color) {
    let h = size * 0.5;
    shapes::fill_polygon(
        &[
            c + vec2(-0.62, -0.64) * h,
            c + vec2(0.3, 0.) * h,
            c + vec2(-0.62, 0.64) * h,
        ],
        color,
    );
    shapes::fill_rounded_rect(
        Rect::new(c.x + 0.44 * h, c.y - 0.64 * h, 0.28 * h, 1.28 * h),
        0.1 * h,
        color,
    );
}
/// Five-pointed medal star.
fn star(c: Vec2, outer: f32, color: Color) {
    let inner = outer * 0.45;
    let point = |i: usize| {
        let a = -FRAC_PI_2 + i as f32 * PI / 5.;
        c + vec2(a.cos(), a.sin()) * if i.is_multiple_of(2) { outer } else { inner }
    };
    for i in 0..10 {
        shapes::fill_triangle(c, point(i), point(i + 1), color);
    }
}
/// A red "!" badge.
fn alert_badge(c: Vec2, radius: f32) {
    shapes::fill_circle(c + vec2(0., 1.5), radius, SHADOW);
    shapes::fill_circle(c, radius, RED);
    label_c("!", c.x + 0.5, c.y + 0.5, radius * 1.5, INK);
}
/// A tick inside a `size` box.
fn tick(c: Vec2, size: f32, color: Color) {
    let h = size * 0.5;
    shapes::polyline(
        &[
            c + vec2(-0.6, 0.05) * h,
            c + vec2(-0.18, 0.5) * h,
            c + vec2(0.62, -0.45) * h,
        ],
        Stroke::new(size * 0.18, color)
            .join(Join::Round)
            .cap(Cap::Round),
    );
}
/// A cross inside a `size` box.
fn cross(c: Vec2, size: f32, color: Color) {
    let h = size * 0.42;
    let s = Stroke::new(size * 0.18, color).cap(Cap::Round);
    shapes::polyline(&[c + vec2(-h, -h), c + vec2(h, h)], s);
    shapes::polyline(&[c + vec2(h, -h), c + vec2(-h, h)], s);
}
/// A numbered parcel crate centered on `pos` with half-size `r`.
fn parcel(pos: Vec2, index: usize, r: f32) {
    let color = parcel_color(index);
    rounded(
        pos.x - r + 2.,
        pos.y - r + 4.,
        r * 2.,
        r * 2.,
        4.,
        Color::new(0.01, 0.02, 0.03, 0.55),
    );
    rounded_gradient(
        pos.x - r,
        pos.y - r,
        r * 2.,
        r * 2.,
        4.,
        lighten(color, 0.18),
        darken(color, 0.12),
    );
    panel(
        pos.x - r + 3.,
        pos.y - r + 1.,
        r * 2. - 6.,
        1.5,
        lighten(color, 0.4),
    );
    panel(
        pos.x - r * 0.2,
        pos.y - r,
        r * 0.4,
        r * 2.,
        Color::new(0.97, 0.9, 0.68, 0.7),
    );
    let s = ink(r * 0.9, INK).with_align(TextAlign::Center);
    draw_text_styled(NUMBERS[index], pos.x + 0.5, s.centered_baseline(pos.y), s);
}
/// A numbered dock outline centered on `p` with half-size `r`.
fn dock(p: Vec2, index: usize, r: f32, done: bool, glow_amount: f32) {
    let color = parcel_color(index);
    if glow_amount > 0. {
        glow(p.x, p.y, r * 2.2, with_alpha(color, 0.3 * glow_amount));
    }
    rounded(
        p.x - r,
        p.y - r,
        r * 2.,
        r * 2.,
        r * 0.33,
        with_alpha(color, if done { 0.3 } else { 0.1 + 0.08 * glow_amount }),
    );
    outline(p.x - r, p.y - r, r * 2., r * 2., r * 0.33, 2., color);
    let k = r * 0.72;
    for (cx, cy) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
        panel(
            p.x + cx * k - 2.,
            p.y + cy * k - 2.,
            4.,
            4.,
            with_alpha(color, 0.7),
        );
    }
    if done {
        tick(p, r * 1.15, CREAM);
    } else {
        label_c(NUMBERS[index], p.x + 0.5, p.y, r * 0.86, color);
    }
}

impl Game {
    /// Robot position, facing direction and hop height for this frame: it
    /// glides to its cell, swings to its heading and lunges at a blocked tile.
    fn robot_pose(&self) -> (Vec2, Vec2, f32) {
        let age = f32::from(self.look.age);
        let from = center(self.look.from);
        let to = center(self.robot);
        let k = smooth(age / 9.);
        let mut pos = from + (to - from) * k;
        let hop = if self.look.from != self.robot {
            (k * PI).sin() * 4.
        } else {
            0.
        };
        let a0 = f32::from(self.look.from_facing) * FRAC_PI_2;
        let mut delta = f32::from(self.facing) * FRAC_PI_2 - a0;
        if delta > PI {
            delta -= TAU;
        } else if delta < -PI {
            delta += TAU;
        }
        let angle = a0 + delta * smooth(age / 8.);
        let dir = vec2(angle.sin(), -angle.cos());
        if self.phase == Phase::Fault
            && let Some(fault) = self.look.fault
            && fault.kind.blocked_move()
            && age < 20.
        {
            pos += dir * (age / 20. * PI).sin() * 10.;
        }
        (pos, dir, hop)
    }
    fn draw_floor(&self, cell: Cell, p: Vec2) {
        let shade = if (cell.x + cell.y).is_multiple_of(2) {
            0.015
        } else {
            0.
        };
        let r = Rect::new(p.x - HALF, p.y - HALF, HALF * 2., HALF * 2.);
        shapes::fill_rounded_rect(
            r,
            1.5,
            Fill::vertical(
                r,
                Color::new(0.18 + shade, 0.28 + shade, 0.29 + shade, 1.),
                Color::new(0.15 + shade, 0.24 + shade, 0.25 + shade, 1.),
            ),
        );
        panel(r.x, r.y, r.w, 1., Color::new(1., 1., 1., 0.07));
        panel(r.x, r.bottom() - 1., r.w, 1., Color::new(0., 0., 0., 0.18));
        panel(
            r.x + 5.,
            r.bottom() - 7.,
            2.,
            2.,
            Color::new(0.38, 0.5, 0.5, 0.7),
        );
    }
    fn draw_wall(&self, cell: Cell, p: Vec2) {
        let below = self.tile(Cell {
            x: cell.x,
            y: cell.y + 1,
        });
        if below != b'#' && cell.y + 1 < ROWS {
            panel(
                p.x - HALF,
                p.y + HALF + 1.,
                HALF * 2.,
                8.,
                Color::new(0., 0.01, 0.02, 0.4),
            );
        }
        let top = Rect::new(p.x - HALF, p.y - HALF, HALF * 2., HALF * 2. - 6.);
        shapes::fill_rounded_rect(
            top,
            0.,
            Fill::vertical(
                top,
                Color::new(0.17, 0.19, 0.24, 1.),
                Color::new(0.12, 0.135, 0.17, 1.),
            ),
        );
        panel(
            p.x - HALF,
            p.y + HALF - 6.,
            HALF * 2.,
            6.,
            Color::new(0.06, 0.065, 0.085, 1.),
        );
        panel(
            p.x - HALF,
            p.y - HALF,
            HALF * 2.,
            1.5,
            Color::new(1., 1., 1., 0.12),
        );
        let k = HALF - 7.;
        for (dx, dy) in [(-k, -k), (k - 3., -k), (-k, k - 9.), (k - 3., k - 9.)] {
            panel(p.x + dx, p.y + dy, 3., 3., Color::new(0.3, 0.33, 0.39, 1.));
        }
    }
    fn draw_pit(&self, p: Vec2, t: f32) {
        let r = Rect::new(p.x - HALF, p.y - HALF, HALF * 2., HALF * 2.);
        shapes::fill_rounded_rect(
            r,
            0.,
            Fill::vertical(
                r,
                Color::new(0.005, 0.01, 0.02, 1.),
                Color::new(0.03, 0.05, 0.07, 1.),
            ),
        );
        // A faint ember breathing far below.
        let ember = 0.05 + 0.05 * pulse(t + p.x * 0.013 + p.y * 0.007, 0.3);
        shapes::fill_circle(
            p + vec2(0., 6.),
            HALF * 0.8,
            Fill::radial(
                p + vec2(0., 6.),
                HALF * 0.8,
                with_alpha(COPPER, ember),
                with_alpha(COPPER, 0.),
            ),
        );
        panel(r.x, r.y, r.w, 6., Color::new(0.16, 0.1, 0.05, 1.));
        for i in 0..4 {
            let x = r.x + 2. + i as f32 * 13.;
            draw_line(x, r.y + 6., x + 6., r.y, 3., COPPER);
        }
    }
    fn draw_gate(&self, tile: u8, p: Vec2, t: f32) {
        let open = self.switches & (1 << (tile - b'A')) != 0;
        let r = HALF - 4.;
        if open {
            glow(p.x, p.y, 34., with_alpha(MINT, 0.12 + 0.06 * pulse(t, 0.8)));
            outline(
                p.x - r,
                p.y - r,
                r * 2.,
                r * 2.,
                6.,
                1.5,
                with_alpha(MINT, 0.6),
            );
            for dx in [-r + 4., r - 8.] {
                panel(
                    p.x + dx,
                    p.y - r + 2.,
                    4.,
                    r * 2. - 4.,
                    with_alpha(MINT, 0.25),
                );
            }
        } else {
            rounded(p.x - r, p.y - r, r * 2., r * 2., 6., darken(RED, 0.72));
            let hum = 0.8 + 0.2 * pulse(t, 0.9);
            for i in -1..=1 {
                let bx = p.x + i as f32 * 12. - 2.;
                panel(bx, p.y - r + 2., 5., r * 2. - 4., with_alpha(RED, hum));
                panel(bx, p.y - r + 2., 2., r * 2. - 4., lighten(RED, 0.3));
            }
            outline(p.x - r, p.y - r, r * 2., r * 2., 6., 2., RED);
        }
        rounded(
            p.x - 9.,
            p.y - 9.,
            18.,
            18.,
            4.,
            Color::new(0.04, 0.05, 0.07, 0.85),
        );
        let letter = [tile];
        let letter = std::str::from_utf8(&letter).unwrap_or("?");
        label_c(letter, p.x + 0.5, p.y, 16., if open { MINT } else { CREAM });
    }
    fn draw_switch(&self, tile: u8, p: Vec2, t: f32) {
        let active = self.switches & (1 << (tile - b'a')) != 0;
        if active {
            glow(p.x, p.y, 32., with_alpha(MINT, 0.25));
        } else {
            // Waiting to be stepped on: a slow beckoning ring.
            let k = pulse(t * 0.8 + p.x * 0.01, 0.6);
            shapes::stroke_circle(p, 19. + 5. * k, 1.5, with_alpha(CREAM, 0.3 * (1. - k)));
        }
        let rim = if active { MINT } else { CREAM };
        shapes::fill_circle(p + vec2(0., 2.), 19., Color::new(0., 0., 0., 0.4));
        shapes::fill_circle(p, 19., darken(rim, 0.45));
        shapes::fill_circle(
            p,
            15.,
            if active {
                Fill::radial(p - vec2(4., 4.), 20., lighten(MINT, 0.3), MINT)
            } else {
                Fill::radial(p - vec2(4., 4.), 20., lighten(PLUM, 0.2), PLUM)
            },
        );
        let letter = [tile];
        let letter = std::str::from_utf8(&letter).unwrap_or("?");
        label_c(
            letter,
            p.x + 0.5,
            p.y - 1.,
            15.,
            if active { INK } else { CREAM },
        );
    }
    fn draw_board(&self, t: f32) {
        let grid = Rect::new(BOARD.x, BOARD.y, COLS as f32 * TILE, ROWS as f32 * TILE);
        rounded(
            grid.x - 9.,
            grid.y - 7.,
            grid.w + 18.,
            grid.h + 18.,
            12.,
            Color::new(0., 0., 0., 0.3),
        );
        rounded_gradient(
            grid.x - 9.,
            grid.y - 9.,
            grid.w + 18.,
            grid.h + 18.,
            12.,
            Color::new(0.06, 0.09, 0.11, 1.),
            Color::new(0.045, 0.07, 0.085, 1.),
        );
        // Floors and pits first; walls cast onto the row below.
        for y in 0..ROWS {
            for x in 0..COLS {
                let cell = Cell { x, y };
                match self.tile(cell) {
                    b'#' => {}
                    b'~' => self.draw_pit(center(cell), t),
                    _ => self.draw_floor(cell, center(cell)),
                }
            }
        }
        for y in 0..ROWS {
            for x in 0..COLS {
                let cell = Cell { x, y };
                let p = center(cell);
                match self.tile(cell) {
                    b'#' => self.draw_wall(cell, p),
                    tile @ (b'A' | b'B') => self.draw_gate(tile, p, t),
                    tile @ (b'a' | b'b') => self.draw_switch(tile, p, t),
                    tile @ b'x'..=b'z' => {
                        let i = (tile - b'x') as usize;
                        let done = self.parcels[i].delivered;
                        // The carried parcel's dock calls to it.
                        let call = if done {
                            0.
                        } else if self.carrying == Some(i) {
                            0.6 + 0.4 * pulse(t, 1.4)
                        } else {
                            0.25 * pulse(t + i as f32 * 0.3, 0.5)
                        };
                        dock(p, i, HALF - 5., done, call);
                    }
                    _ => {}
                }
            }
        }
        let (pos, dir, hop) = self.robot_pose();
        self.draw_trail(pos);
        if self.show_hint && !matches!(self.phase, Phase::Solved | Phase::Complete) {
            self.draw_route(t);
        }
        for (i, p) in self.parcels.iter().enumerate() {
            if p.present && !p.delivered && self.carrying != Some(i) {
                parcel(center(p.pos), i, 15.);
            }
        }
        let fault = self.look.fault.filter(|_| self.phase == Phase::Fault);
        if let Some(fault) = fault
            && fault.at != self.robot
        {
            // The tile the robot could not enter.
            let c = center(fault.at);
            let beat = pulse(t, 1.6);
            rounded(
                c.x - HALF + 2.,
                c.y - HALF + 2.,
                HALF * 2. - 4.,
                HALF * 2. - 4.,
                7.,
                with_alpha(RED, 0.12 + 0.12 * beat),
            );
            outline(
                c.x - HALF + 2.,
                c.y - HALF + 2.,
                HALF * 2. - 4.,
                HALF * 2. - 4.,
                7.,
                2.5,
                with_alpha(RED, 0.6 + 0.4 * beat),
            );
            cross(c, 22., with_alpha(RED, 0.75 + 0.25 * beat));
        }
        self.draw_robot(pos, dir, hop, t);
        if let Some(fault) = fault
            && fault.at == self.robot
        {
            let beat = bob(t, 1.5, 2.);
            alert_badge(pos + vec2(19., -38. + beat), 9.);
        }
    }
    /// Fading dots along the cells this attempt has visited.
    fn draw_trail(&self, robot: Vec2) {
        let path = self.look.path();
        let n = path.len();
        for i in 1..n {
            let a = center(path[i - 1]);
            let b = if i + 1 == n { robot } else { center(path[i]) };
            let alpha = 0.1 + 0.4 * i as f32 / (n - 1) as f32;
            let span = b - a;
            let dots = (span.length() / 9.).floor() as usize;
            for k in 1..=dots {
                let q = a + span * (k as f32 / dots as f32);
                shapes::fill_circle(q + vec2(0., 10.), 2.2, with_alpha(MINT, alpha));
            }
        }
    }
    /// The hint: the reference program's route as marching chevrons.
    fn draw_route(&self, t: f32) {
        let sim = self.reference_route();
        let path = sim.look.path();
        let flow = (t * 1.2).fract();
        for pair in path.windows(2) {
            let a = center(pair[0]);
            let b = center(pair[1]);
            let dir = (b - a) / TILE;
            let n = vec2(-dir.y, dir.x);
            for k in 0..3 {
                let f = (k as f32 + flow) / 3.;
                let q = a + (b - a) * f;
                let fade = (f * PI).sin().max(0.2);
                shapes::polyline(
                    &[q - dir * 5. + n * 6., q + dir * 2., q - dir * 5. - n * 6.],
                    Stroke::new(3., with_alpha(GOLD, 0.85 * fade))
                        .cap(Cap::Round)
                        .join(Join::Round),
                );
            }
        }
        if let Some(&end) = path.last() {
            let c = center(end);
            let k = pulse(t, 1.);
            shapes::stroke_circle(c, 18. + 6. * k, 2., with_alpha(GOLD, 0.8 - 0.5 * k));
        }
    }
    fn draw_robot(&self, p: Vec2, dir: Vec2, hop: f32, t: f32) {
        let fault = self.phase == Phase::Fault;
        let active = matches!(self.phase, Phase::Running | Phase::Paused);
        let across = vec2(-dir.y, dir.x);
        // Heading arrow on the floor.
        // While planning, the heading arrow nudges forward: commands are
        // relative to where the robot faces.
        let nudge = if self.phase == Phase::Editing {
            2. * pulse(t, 0.9)
        } else {
            0.
        };
        let tip = p + dir * (37. + nudge);
        let base = p + dir * (27. + nudge);
        draw_triangle(
            tip + vec2(1., 2.),
            base + across * 6.5 + vec2(1., 2.),
            base - across * 6.5 + vec2(1., 2.),
            Color::new(0., 0., 0., 0.45),
        );
        draw_triangle(
            tip,
            base + across * 6.5,
            base - across * 6.5,
            if fault { RED } else { CREAM },
        );
        draw_ellipse(
            p.x + 1.,
            p.y + 19.,
            18. - hop,
            6.,
            0.,
            Color::new(0.01, 0.02, 0.03, 0.6),
        );
        let idle = if active || hop > 0. {
            0.
        } else {
            bob(t, 0.6, 1.2)
        };
        let p = p + vec2(0., idle - hop);
        for dx in [-10., 10.] {
            rounded(p.x + dx - 4., p.y + 11., 8., 9., 2.5, darken(COPPER, 0.15));
        }
        rounded_gradient(
            p.x - 14.,
            p.y - 6.,
            28.,
            21.,
            5.,
            lighten(MINT, 0.12),
            darken(MINT, 0.25),
        );
        panel(
            p.x - 6.,
            p.y + 2.,
            12.,
            3.,
            Color::new(0.06, 0.16, 0.16, 0.6),
        );
        rounded_gradient(
            p.x - 17.,
            p.y - 23.,
            34.,
            21.,
            6.,
            Color::new(1., 0.95, 0.8, 1.),
            Color::new(0.88, 0.76, 0.55, 1.),
        );
        rounded(p.x - 12., p.y - 18., 24., 9., 3., INK);
        let gaze = dir * 2.;
        let eye = if fault { RED } else { MINT };
        // An occasional blink.
        let blink = (t + 0.4) % 3.7 < 0.12;
        for ex in [-5.5, 5.5] {
            let h = if blink { 1. } else { 4. };
            draw_rectangle(
                p.x + ex - 2. + gaze.x,
                p.y - 13.5 - h * 0.5 + gaze.y * 0.5,
                4.,
                h,
                eye,
            );
        }
        draw_line(p.x, p.y - 23., p.x, p.y - 29., 2., COPPER);
        let bulb = vec2(p.x, p.y - 30.);
        let lit = if active {
            self.look.age < 9
        } else {
            pulse(t, 0.5) > 0.7
        };
        if lit {
            glow(bulb.x, bulb.y, 10., with_alpha(RED, 0.5));
        }
        draw_circle(
            bulb.x,
            bulb.y,
            3.,
            if lit { lighten(RED, 0.3) } else { RED },
        );
        if let Some(i) = self.carrying {
            // Held against the chest between both arms.
            parcel(p + vec2(0., 6.), i, 10.);
            for side in [-1., 1.] {
                rounded(
                    p.x + side * 12. - 2.5,
                    p.y + 1.,
                    5.,
                    10.,
                    2.5,
                    darken(COPPER, 0.1),
                );
            }
        }
    }

    fn draw_panel(&self, t: f32, mouse: Vec2) {
        surface(PANEL.x, PANEL.y, PANEL.w, PANEL.h);
        let editing = self.phase == Phase::Editing;
        let hover_tile = layout::palette_at(mouse).filter(|_| editing);
        for i in 0..Op::ALL.len() {
            self.draw_palette_tile(i, editing, hover_tile == Some(i), t);
        }
        let preview = hover_tile.filter(|&i| i < self.unlocked());
        self.draw_tape(editing, preview, t, mouse);
        let hover_button = Button::at(mouse);
        self.draw_stats(hover_button == Some(Button::Hint), t);
        for button in Button::ALL {
            self.draw_button(button, hover_button == Some(button), t);
        }
    }
    fn draw_palette_tile(&self, i: usize, editing: bool, hover: bool, t: f32) {
        let op = Op::ALL[i];
        let r = layout::palette(i);
        let cx = r.x + r.w * 0.5;
        rounded(r.x, r.y + 3., r.w, r.h, 9., Color::new(0., 0., 0., 0.3));
        if i >= self.unlocked() {
            rounded_gradient(
                r.x,
                r.y,
                r.w,
                r.h,
                9.,
                Color::new(0.11, 0.13, 0.16, 1.),
                Color::new(0.08, 0.095, 0.12, 1.),
            );
            outline(r.x, r.y, r.w, r.h, 9., 1., Color::new(1., 1., 1., 0.05));
            glyph(op, vec2(cx, r.y + 25.), 24., with_alpha(SOFT, 0.13));
            icon(
                Icon::Lock,
                cx,
                r.y + r.h - 14.,
                13.,
                with_alpha(MUTED, if hover { 0.9 } else { 0.55 }),
            );
            return;
        }
        let selected = i == self.palette;
        let a = if editing { 1. } else { 0.5 };
        let color = op_color(op);
        let lift = if !editing {
            0.
        } else if selected {
            3. + bob(t, 0.7, 1.)
        } else if hover {
            2.
        } else {
            0.
        };
        let y = r.y - lift;
        if selected && editing {
            glow(cx, y + r.h * 0.5, 44., with_alpha(color, 0.16));
        }
        let (top, bottom) = if selected {
            (darken(color, 0.3), darken(color, 0.56))
        } else {
            (darken(color, 0.66), darken(color, 0.77))
        };
        rounded_gradient(
            r.x,
            y,
            r.w,
            r.h,
            9.,
            with_alpha(top, a),
            with_alpha(bottom, a),
        );
        panel(
            r.x + 7.,
            y + 1.,
            r.w - 14.,
            1.,
            Color::new(1., 1., 1., 0.1 * a),
        );
        if selected {
            outline(r.x, y, r.w, r.h, 9., 2., with_alpha(lighten(color, 0.3), a));
        } else if hover {
            outline(r.x, y, r.w, r.h, 9., 1.5, with_alpha(color, 0.85));
        } else {
            outline(r.x, y, r.w, r.h, 9., 1., with_alpha(color, 0.2 * a));
        }
        let tone = if selected {
            lighten(color, 0.62)
        } else {
            lighten(color, 0.12)
        };
        glyph(op, vec2(cx, y + 25.), 26., with_alpha(tone, a));
        let ink = with_alpha(
            if selected { CREAM } else { SOFT },
            if selected { a } else { 0.75 * a },
        );
        let s = TextStyle::modern(10., ink).with_align(TextAlign::Center);
        let fit = (r.w - 10.) / text_width(op.label(), s);
        let s = if fit < 1. { s.with_size(10. * fit) } else { s };
        draw_text_styled(op.label(), cx, s.centered_baseline(y + r.h - 12.), s);
    }
    fn draw_tape(&self, editing: bool, preview: Option<usize>, t: f32, mouse: Vec2) {
        let budget = self.content().budget;
        let par = self.par().min(budget);
        let head = if editing { None } else { self.last_pc };
        let fault_pc = self
            .look
            .fault
            .filter(|_| self.phase == Phase::Fault)
            .and_then(|f| f.pc);
        let hover = layout::tape_at(mouse).filter(|&i| editing && i <= self.tape.len);
        let chosen = Op::ALL[preview.unwrap_or(self.palette)];
        // The three-star length: a gold rule under the par cells.
        for i in 0..par {
            let r = layout::tape(i);
            let row_end = (i + 1) % COLUMNS == 0 || i + 1 == par;
            let w = if row_end { r.w } else { r.w + CELL_GAP };
            panel(r.x, r.bottom() + 2., w, 2., with_alpha(GOLD, 0.5));
        }
        if par > 0 {
            let r = layout::tape(par - 1);
            star(vec2(r.right(), r.bottom() + 3.), 6.5, GOLD);
        }
        for i in 0..CAPACITY {
            let r = layout::tape(i);
            let c = r.center();
            if i >= budget {
                // Capacity this room does not grant.
                shapes::fill_circle(c, 2., Color::new(1., 1., 1., 0.06));
                continue;
            }
            let is_head = head == Some(i);
            let faulted = fault_pc == Some(i);
            let cursor = editing && i == self.tape.cursor;
            rounded(r.x, r.y, r.w, r.h, 7., Color::new(0.015, 0.025, 0.035, 0.8));
            outline(r.x, r.y, r.w, r.h, 7., 1., Color::new(1., 1., 1., 0.05));
            if i < self.tape.len {
                let op = self.tape.ops[i];
                let color = if faulted { RED } else { op_color(op) };
                let a = if head.is_some_and(|h| i < h) {
                    0.45
                } else {
                    1.
                };
                if is_head {
                    glow(c.x, c.y, 36., with_alpha(color, 0.3));
                }
                let (top, bottom) = if is_head || faulted {
                    (darken(color, 0.28), darken(color, 0.5))
                } else {
                    (darken(color, 0.62), darken(color, 0.74))
                };
                rounded_gradient(
                    r.x + 2.,
                    r.y + 2.,
                    r.w - 4.,
                    r.h - 4.,
                    6.,
                    with_alpha(top, a),
                    with_alpha(bottom, a),
                );
                let tone = if is_head || faulted {
                    lighten(color, 0.65)
                } else {
                    lighten(color, 0.15)
                };
                glyph(op, c, 22., with_alpha(tone, a));
                if is_head {
                    outline(
                        r.x - 1.,
                        r.y - 1.,
                        r.w + 2.,
                        r.h + 2.,
                        8.,
                        2.,
                        lighten(color, 0.35),
                    );
                    if matches!(op, Op::Repeat2 | Op::Repeat3) {
                        // Copies already played, as lit pips.
                        let total = if op == Op::Repeat2 { 2 } else { 3 };
                        let used = total - self.repeats.min(total);
                        for k in 0..total {
                            let x = c.x + (f32::from(k) - f32::from(total - 1) * 0.5) * 7.;
                            let lit = k < used;
                            shapes::fill_circle(
                                vec2(x, r.bottom() - 5.),
                                2.2,
                                if lit { CREAM } else { darken(VIOLET, 0.4) },
                            );
                        }
                    }
                }
            } else if cursor {
                // The append cell previews what Space would write.
                glyph(
                    chosen,
                    c,
                    22.,
                    with_alpha(op_color(chosen), 0.3 + 0.2 * pulse(t, 1.2)),
                );
            }
            if cursor {
                let beat = pulse(t, 1.3);
                outline(
                    r.x - 2.,
                    r.y - 2.,
                    r.w + 4.,
                    r.h + 4.,
                    9.,
                    2.,
                    with_alpha(MINT, 0.55 + 0.45 * beat),
                );
            } else if hover == Some(i) {
                outline(
                    r.x - 1.,
                    r.y - 1.,
                    r.w + 2.,
                    r.h + 2.,
                    8.,
                    1.5,
                    with_alpha(CREAM, 0.65),
                );
            }
            if faulted {
                let beat = pulse(t, 1.6);
                outline(
                    r.x - 2.,
                    r.y - 2.,
                    r.w + 4.,
                    r.h + 4.,
                    9.,
                    2.5,
                    with_alpha(RED, 0.6 + 0.4 * beat),
                );
                alert_badge(vec2(r.right() - 3., r.y + 3.), 7.5);
            }
        }
        if editing {
            let cursor = self.tape.cursor;
            if cursor < self.tape.len && chosen != self.tape.ops[cursor] {
                // Writing here would replace the instruction: show the newcomer.
                let r = layout::tape(cursor);
                let c = vec2(r.right() - 4., r.y + 4.);
                shapes::fill_circle(c, 10., Color::new(0.03, 0.04, 0.06, 0.95));
                shapes::stroke_circle(c, 10., 1.5, op_color(chosen));
                glyph(chosen, c, 12., lighten(op_color(chosen), 0.3));
            }
            if self.tape.len == 0 {
                let r = layout::tape(0);
                keycap("Space", "", r.right() + 12., r.y + 12., CREAM);
            }
        }
    }
    fn draw_stats(&self, hover_hint: bool, t: f32) {
        let y = STATS_Y;
        // The stars this program would earn if it solved now.
        let medal = self.medal(self.tape.len);
        let warn = hover_hint && !self.hinted && medal == 3;
        for i in 0..3_u8 {
            let c = vec2(556. + f32::from(i) * 24., y);
            star(c + vec2(1., 1.5), 10., SHADOW);
            let color = if i >= medal {
                DIM
            } else if warn && i == 2 {
                with_alpha(GOLD, 0.25 + 0.5 * pulse(t, 2.5))
            } else {
                GOLD
            };
            star(c, 10., color);
        }
        if self.hinted {
            icon(Icon::Bulb, 640., y, 15., with_alpha(GOLD, 0.6));
        }
        icon_value(Icon::Clock, &self.steps.to_string(), 726., y, 16., SOFT);
        let faults = if self.failures > 0 { RED } else { SOFT };
        icon_value(
            Icon::Alert,
            &self.failures.to_string(),
            852.,
            y,
            16.,
            faults,
        );
    }
    fn draw_button(&self, button: Button, hover: bool, t: f32) {
        let r = button.rect();
        let editing = self.phase == Phase::Editing;
        let running = self.phase == Phase::Running;
        let paused = self.phase == Phase::Paused;
        let fault = self.phase == Phase::Fault;
        let tape = self.tape.len > 0;
        let (enabled, active, call) = match button {
            Button::Run => (
                (editing && tape) || running || paused,
                running,
                (editing && tape) || paused,
            ),
            Button::Step => ((editing && tape) || running || paused, false, paused),
            Button::Stop => (!editing, false, fault),
            Button::Undo => (editing && self.can_undo(), false, false),
            Button::Clear => (editing && tape, false, false),
            Button::Hint => (true, self.show_hint, false),
        };
        let tone = match button {
            Button::Stop if fault => RED,
            Button::Hint => GOLD,
            _ => MINT,
        };
        let a = if enabled { 1. } else { 0.38 };
        let lift = if enabled && hover { 2. } else { 0. };
        let beat = if call { pulse(t, 1.1) } else { 0. };
        rounded(
            r.x,
            r.y + 3.,
            r.w,
            r.h,
            10.,
            Color::new(0., 0., 0., 0.4 * a),
        );
        let y = r.y - lift;
        let cx = r.x + r.w * 0.5;
        if call {
            glow(cx, y + r.h * 0.5, 50., with_alpha(tone, 0.2 * beat));
        }
        let (top, bottom) = if active {
            (darken(tone, 0.42), darken(tone, 0.62))
        } else {
            (
                Color::new(0.17, 0.22, 0.26, 1.),
                Color::new(0.11, 0.15, 0.18, 1.),
            )
        };
        rounded_gradient(
            r.x,
            y,
            r.w,
            r.h,
            10.,
            with_alpha(top, a),
            with_alpha(bottom, a),
        );
        if call {
            outline(
                r.x,
                y,
                r.w,
                r.h,
                10.,
                2.,
                with_alpha(tone, 0.35 + 0.55 * beat),
            );
        } else if hover && enabled {
            outline(r.x, y, r.w, r.h, 10., 1.5, with_alpha(tone, 0.75));
        } else {
            outline(r.x, y, r.w, r.h, 10., 1., Color::new(1., 1., 1., 0.07 * a));
        }
        let ink = with_alpha(
            if active || call {
                lighten(tone, 0.45)
            } else {
                PAPER
            },
            a,
        );
        let c = vec2(cx, y + 28.);
        match button {
            Button::Run if running => icon(Icon::Pause, c.x, c.y, 26., ink),
            Button::Run => icon(Icon::Play, c.x, c.y, 26., ink),
            Button::Step => step_glyph(c, 22., ink),
            Button::Stop => icon(Icon::Restart, c.x, c.y, 24., ink),
            Button::Undo => icon(Icon::Undo, c.x, c.y, 24., ink),
            Button::Clear => icon(Icon::Cross, c.x, c.y, 22., ink),
            Button::Hint => icon(Icon::Bulb, c.x, c.y, 24., ink),
        }
        if button == Button::Hint && self.hinted {
            shapes::fill_circle(vec2(r.right() - 9., y + 9.), 3.5, GOLD);
        }
        mini_key(button.key(), cx, y + r.h - 15., a);
    }

    fn star_spot(i: u8) -> Vec2 {
        vec2(PANEL.x + PANEL.w * 0.5 + (f32::from(i) - 1.) * 86., 226.)
    }
    fn draw_solved(&self, mouse: Vec2) {
        surface(PANEL.x, PANEL.y, PANEL.w, PANEL.h);
        let complete = self.phase == Phase::Complete;
        let earned = self.stars[self.level];
        let cx = PANEL.x + PANEL.w * 0.5;
        rounded(PANEL.x, PANEL.y, PANEL.w, 4., 2., MINT);
        let word = match earned {
            3 => "Perfect",
            2 => "Tidy",
            _ => "Solved",
        };
        let s = style(30., CREAM).with_align(TextAlign::Center);
        draw_text_styled(word, cx, s.centered_baseline(162.), s);
        // Stars land one after another.
        for i in 0..3_u8 {
            let c = Self::star_spot(i);
            star(c + vec2(2., 3.), 30., Color::new(0., 0., 0., 0.35));
            star(c, 30., Color::new(0.2, 0.24, 0.28, 1.));
            if i < earned {
                let start = 8 + u16::from(i) * 12;
                let k = if complete {
                    1.
                } else {
                    (self.solved_ticks.saturating_sub(start) as f32 / 10.).min(1.)
                };
                if k > 0. {
                    let pop = 1. + (k * PI).sin() * 0.25;
                    glow(c.x, c.y, 46. * k, with_alpha(GOLD, 0.3 * k));
                    star(c, 30. * k * pop, GOLD);
                    star(c - vec2(0., 2.), 15. * k * pop, lighten(GOLD, 0.6));
                }
            }
        }
        // Why: the three-star checks.
        let par = self.par();
        let checks = [
            (self.tape.len <= par, 0_u8),
            (self.failures == 0, 1),
            (!self.hinted, 2),
        ];
        let w = 118.;
        for (passed, k) in checks {
            let x = PANEL.x + 19. + f32::from(k) * (w + 11.);
            let r = Rect::new(x, 276., w, 58.);
            rounded(r.x, r.y, r.w, r.h, 9., Color::new(1., 1., 1., 0.04));
            outline(
                r.x,
                r.y,
                r.w,
                r.h,
                9.,
                1.,
                with_alpha(if passed { MINT } else { RED }, 0.3),
            );
            let cy = r.y + r.h * 0.5;
            match k {
                0 => {
                    // A scrap of tape: the program's length against par.
                    for (j, color) in [MINT, BLUE, COPPER].into_iter().enumerate() {
                        rounded(x + 11. + j as f32 * 9., cy - 9., 7., 18., 2., color);
                    }
                    let value = format!("{}/{}", self.tape.len, par);
                    label_c(&value, x + 64., cy, 18., CREAM);
                }
                1 => {
                    icon_value(
                        Icon::Alert,
                        &self.failures.to_string(),
                        x + 14.,
                        cy,
                        20.,
                        if self.failures > 0 { RED } else { SOFT },
                    );
                }
                _ => {
                    icon(
                        Icon::Bulb,
                        x + 24.,
                        cy,
                        22.,
                        if self.hinted { GOLD } else { SOFT },
                    );
                }
            }
            let mark = vec2(r.right() - 20., cy);
            if passed {
                tick(mark, 18., MINT);
            } else {
                cross(mark, 16., RED);
            }
        }
        // Steps run and the points this room banked.
        let y = 392.;
        icon_value(
            Icon::Clock,
            &self.steps.to_string(),
            PANEL.x + 24.,
            y,
            20.,
            SOFT,
        );
        let gained = format!("+{}", grouped(points(earned, self.steps)));
        let s = style(28., MINT).with_align(TextAlign::Right);
        let w = draw_text_styled(&gained, PANEL.right() - 22., s.centered_baseline(y), s).width;
        icon(Icon::Star, PANEL.right() - 40. - w, y, 22., MINT);
        divider(
            PANEL.x + 20.,
            438.,
            PANEL.w - 40.,
            Color::new(1., 1., 1., 0.1),
        );
        if complete {
            icon(Icon::Trophy, cx, NEXT.y + NEXT.h * 0.5, 36., GOLD);
            return;
        }
        // Next workbench.
        let hover = NEXT.contains(mouse);
        let lift = if hover { 2. } else { 0. };
        let beat = pulse(clock(), 1.);
        let r = Rect::new(NEXT.x, NEXT.y - lift, NEXT.w, NEXT.h);
        shapes::fill_rounded_rect(
            Rect::new(r.x, NEXT.y + 3., r.w, r.h),
            12.,
            Color::new(0., 0., 0., 0.4),
        );
        glow(
            r.center().x,
            r.center().y,
            140.,
            with_alpha(MINT, 0.08 + 0.08 * beat),
        );
        let base = Color::new(0.2, 0.42, 0.37, 1.);
        shapes::fill_rounded_rect(
            r,
            12.,
            Fill::vertical(r, lighten(base, if hover { 0.2 } else { 0.12 }), base),
        );
        shapes::stroke_rounded_rect(r, 12., 1.5, with_alpha(MINT, 0.4 + 0.4 * beat));
        let cy = r.y + r.h * 0.5;
        let kw = keycap("Space", "", cx - 66., cy - 10., CREAM);
        icon(Icon::Play, cx - 66. + kw + 22., cy, 22., CREAM);
        let s = style(20., CREAM);
        draw_text_styled("Next", cx - 66. + kw + 40., s.centered_baseline(cy), s);
    }
    /// Plain words for status readers: progress, the last action and, while
    /// the visual hint is open, its route advice.
    fn status(&self) -> String {
        let mut status = format!(
            "{} of {} parcels delivered, {} faults, {} of 36 stars. {}",
            self.delivered(),
            self.goals(),
            self.failures,
            self.total_stars(),
            self.notice
        );
        if self.show_hint {
            status.push(' ');
            status.push_str(self.content().hint);
        }
        status
    }
}

impl ArcadeGame for Game {
    fn new() -> Self {
        Game::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        let t = clock();
        let mouse = pointer();
        self.draw_board(t);
        if matches!(self.phase, Phase::Solved | Phase::Complete) {
            self.draw_solved(mouse);
        } else {
            self.draw_panel(t, mouse);
        }
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: self.content().name.into(),
            objective: self.content().lesson.into(),
            status: self.status(),
            score: self.score(),
        }
    }
    fn outcome(&self) -> Outcome {
        Game::outcome(self)
    }
    fn summary(&self) -> String {
        Game::summary(self)
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.room() as u32, 12))
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        for event in self.look.events() {
            match event {
                Event::Wrote { index, op } => {
                    let c = layout::tape(index).center();
                    fx.burst(c, op_color(op), 10);
                    fx.ring(c, op_color(op), 30.);
                }
                Event::Deleted { index } => {
                    fx.burst(layout::tape(index).center(), MUTED, 12);
                }
                Event::Undid => fx.ring(Button::Undo.rect().center(), MINT, 34.),
                Event::Cleared { len } => {
                    for i in 0..len {
                        fx.burst(layout::tape(i).center(), MUTED, 5);
                    }
                }
                Event::Cursor { index } => fx.ring(layout::tape(index).center(), MINT, 24.),
                Event::Palette { index } => {
                    fx.ring(
                        layout::palette(index).center(),
                        op_color(Op::ALL[index]),
                        28.,
                    );
                }
                Event::TapeFull { index } => {
                    let c = layout::tape(index).center();
                    fx.ring(c, RED, 32.);
                    fx.popup(c - vec2(0., 26.), "Full", RED);
                }
                Event::Locked { index } => {
                    let c = layout::palette(index).center();
                    fx.ring(c, RED, 30.);
                    fx.popup_icon(c - vec2(0., 30.), Icon::Lock, "", RED);
                }
                Event::EmptyRun => {
                    fx.ring(Button::Run.rect().center(), RED, 34.);
                    fx.ring(layout::tape(0).center(), RED, 30.);
                }
                Event::Started => {
                    fx.ring(center(self.look.path()[0]), MINT, 44.);
                    fx.ring(Button::Run.rect().center(), MINT, 36.);
                }
                Event::Paused { paused } => {
                    fx.ring(
                        Button::Run.rect().center(),
                        if paused { CREAM } else { MINT },
                        34.,
                    );
                }
                Event::Moved { from, .. } => {
                    fx.burst(
                        center(from) + vec2(0., 16.),
                        Color::new(0.5, 0.62, 0.6, 0.8),
                        6,
                    );
                }
                Event::Turned { at } => fx.ring(center(at), BLUE, 30.),
                Event::Repeat { index, times } => {
                    let c = layout::tape(index).center();
                    fx.ring(c, VIOLET, 32.);
                    fx.popup(
                        center(self.robot) - vec2(0., 44.),
                        if times == 2 { "x2" } else { "x3" },
                        VIOLET,
                    );
                }
                Event::Switch { at, gate } => {
                    fx.ring(center(at), MINT, 46.);
                    fx.burst(center(at), MINT, 16);
                    for y in 0..ROWS {
                        for x in 0..COLS {
                            let cell = Cell { x, y };
                            if self.tile(cell) == gate {
                                fx.ring(center(cell), MINT, 44.);
                                fx.sparkle(center(cell), MINT);
                                fx.popup_icon(center(cell) - vec2(0., 34.), Icon::Unlock, "", MINT);
                            }
                        }
                    }
                }
                Event::Grabbed { at, parcel } => {
                    fx.burst(center(at), parcel_color(parcel), 14);
                    fx.ring(center(at), parcel_color(parcel), 34.);
                }
                Event::Dropped {
                    at,
                    parcel,
                    delivered,
                } => {
                    let c = center(at);
                    if delivered {
                        fx.burst(c, parcel_color(parcel), 30);
                        fx.ring(c, parcel_color(parcel), 58.);
                        fx.sparkle(c, CREAM);
                        fx.popup_icon(c - vec2(0., 36.), Icon::Check, "", parcel_color(parcel));
                    } else {
                        fx.burst(c + vec2(0., 12.), Color::new(0.5, 0.62, 0.6, 0.8), 8);
                        fx.ring(c, parcel_color(parcel), 26.);
                    }
                }
                Event::Fault(fault) => {
                    let c = center(fault.at);
                    fx.ring(c, RED, 48.);
                    fx.burst(c, RED, 20);
                    fx.popup(c - vec2(0., 40.), fault.kind.word(), RED);
                    if let Some(pc) = fault.pc.filter(|&pc| pc < CAPACITY) {
                        fx.ring(layout::tape(pc).center(), RED, 34.);
                    }
                }
                Event::Solved { stars } => {
                    let board = center(Cell { x: 4, y: 3 });
                    let word = if stars == 3 { "Perfect!" } else { "Restored!" };
                    fx.callout(board - vec2(0., 20.), word, GOLD);
                    fx.sparkle(center(self.robot), GOLD);
                    fx.burst(center(self.robot), GOLD, 24);
                }
                Event::Stopped => fx.ring(center(self.robot), CREAM, 40.),
                Event::NextRoom => {
                    fx.ring(center(self.robot), MINT, 56.);
                    fx.sparkle(center(self.robot), MINT);
                }
                Event::Hint { open } => {
                    if open {
                        fx.sparkle(Button::Hint.rect().center(), GOLD);
                    }
                }
            }
        }
        // Each earned star sparkles as it lands on the solved card.
        if self.phase == Phase::Solved {
            for i in 0..self.stars[self.level] {
                if self.solved_ticks == 13 + u16::from(i) * 12 {
                    let c = Self::star_spot(i);
                    fx.sparkle(c, GOLD);
                    fx.ring(c, GOLD, 44.);
                }
            }
        }
    }
    fn draw_objective(&self, area: Rect) -> bool {
        let t = clock();
        let cy = area.y + area.h * 0.5;
        let mut x = area.x + 2.;
        for (i, p) in self.parcels.iter().enumerate() {
            if !p.present {
                continue;
            }
            let color = parcel_color(i);
            let r = Rect::new(x, cy - 14., 78., 28.);
            shapes::fill_rounded_rect(
                r,
                14.,
                with_alpha(color, if p.delivered { 0.2 } else { 0.07 }),
            );
            shapes::stroke_rounded_rect(
                r,
                14.,
                1.,
                with_alpha(color, if p.delivered { 0.7 } else { 0.3 }),
            );
            let lift = if self.carrying == Some(i) {
                3. + bob(t, 2., 1.5)
            } else {
                0.
            };
            parcel(vec2(x + 18., cy - lift), i, 9.);
            let ax = x + 34.;
            shapes::polyline(
                &[vec2(ax, cy), vec2(ax + 10., cy)],
                Stroke::new(2., with_alpha(color, 0.7)).cap(Cap::Round),
            );
            arrow_head(vec2(ax + 15., cy), vec2(1., 0.), 6., with_alpha(color, 0.7));
            dock(vec2(x + 61., cy), i, 9., p.delivered, 0.);
            x += r.w + 8.;
        }
        let s = style(16., Color::new(0.84, 0.88, 0.92, 1.));
        draw_text_styled(self.content().lesson, x + 6., s.centered_baseline(cy), s);
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        let cy = area.y + 30.;
        let col = area.w / 3.;
        let restored = format!("{}/12", self.restored());
        icon_value(Icon::Gear, &restored, area.x, cy, 30., MINT);
        let x = area.x + col;
        star(vec2(x + 15., cy), 16., GOLD);
        let s = style(30., GOLD);
        draw_text_styled(
            &format!("{}/36", self.total_stars()),
            x + 38.,
            s.centered_baseline(cy),
            s,
        );
        let perfect = self.stars.iter().filter(|&&s| s == 3).count();
        icon_value(
            Icon::Crown,
            &perfect.to_string(),
            area.x + col * 2.,
            cy,
            30.,
            CREAM,
        );
        // Every workbench's medal, left to right.
        let w = area.w / 12.;
        for (room, &earned) in self.stars.iter().enumerate() {
            let x = area.x + w * (room as f32 + 0.5);
            let y = area.y + 96.;
            rounded(
                x - w * 0.5 + 3.,
                y - 20.,
                w - 6.,
                40.,
                7.,
                Color::new(1., 1., 1., if earned > 0 { 0.06 } else { 0.03 }),
            );
            for k in 0..3_u8 {
                let c = vec2(
                    x + (f32::from(k) - 1.) * 11.,
                    y + if k == 1 { -5. } else { 4. },
                );
                star(c, 6., if k < earned { GOLD } else { DIM });
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::META;
    fn finished() -> Game {
        let mut game = Game::new();
        for tick in 0..40_000 {
            game.step(&game.pilot(tick));
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        game
    }
    #[test]
    fn progress_counts_workbenches_from_first_to_last() {
        let game = Game::new();
        assert_eq!(game.progress(), Some((1, 12)));
        let done = finished();
        assert_eq!(done.outcome(), Outcome::Won);
        assert_eq!(done.progress(), Some((12, 12)));
        assert_eq!(done.summary().matches('.').count(), 1);
    }
    #[test]
    fn header_copy_is_a_name_and_a_short_goal() {
        let mut game = Game::new();
        for level in 0..12 {
            game.level = level;
            let hud = game.hud();
            assert_eq!(hud.stage, game.content().name);
            assert!(
                hud.objective.split_whitespace().count() <= 5,
                "{}",
                hud.objective
            );
            assert!(hud.objective.is_ascii());
        }
        game.level = 0;
        game.show_hint = true;
        assert!(game.hud().status.contains(game.content().hint));
    }
    #[test]
    fn help_is_four_short_bullets_and_controls_name_every_button_key() {
        assert!(META.help.len() <= 4);
        for bullet in META.help {
            assert!(bullet.split_whitespace().count() < 15, "{bullet}");
        }
        for button in Button::ALL {
            assert!(
                META.controls
                    .split('|')
                    .any(|pair| pair.trim().starts_with(button.key())),
                "{button:?}"
            );
        }
    }
    #[test]
    fn deliveries_and_faults_produce_effects_where_they_happen() {
        // Run the first room's reference program; the delivery tick bursts
        // and the solve calls out.
        let mut game = Game::new();
        let mut delivered = None;
        for tick in 0..2_000 {
            let cue = game.step(&game.pilot(tick));
            let mut fx = Juice::new();
            game.effects(cue, &mut fx);
            if game.look.events().any(|e| {
                matches!(
                    e,
                    Event::Dropped {
                        delivered: true,
                        ..
                    }
                )
            }) {
                delivered = Some(fx.counts());
                break;
            }
        }
        let counts = delivered.expect("first parcel delivered");
        assert!(counts.sparks > 20);
        assert!(counts.popups >= 2, "check and callout");
        assert!(counts.rings >= 1);
        // A wall fault shows a popup and rings at the wall and the tape cell.
        let mut game = Game::new();
        for code in [0, 1, 0, 0, 0] {
            game.palette = code;
            game.step(&Input {
                primary: true,
                ..Input::default()
            });
        }
        let mut fx = Juice::new();
        let mut cue = Cue::None;
        for _ in 0..5 {
            cue = game.step(&Input {
                slot: Some(2),
                ..Input::default()
            });
            fx = Juice::new();
            game.effects(cue, &mut fx);
            if game.phase == Phase::Fault {
                break;
            }
        }
        assert_eq!(cue, Cue::Bad);
        let counts = fx.counts();
        assert_eq!(counts.popups, 1);
        assert_eq!(counts.rings, 2);
        assert!(counts.sparks >= 20);
    }
    #[test]
    fn writing_a_cell_pops_it_and_effects_never_change_the_game() {
        let mut game = Game::new();
        let cue = game.step(&Input {
            primary: true,
            ..Input::default()
        });
        let frozen = game.clone();
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        assert_eq!(game, frozen);
        assert!(fx.counts().sparks > 0 && fx.counts().rings == 1);
    }
}
