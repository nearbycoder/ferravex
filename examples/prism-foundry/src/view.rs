//! Read-only brass-and-glass workbench presentation.
//!
//! Every animated detail (beam flow, lock pulses, cursor breathing) is derived
//! from the model's own tick counter, so drawing never reads a clock.
use crate::model::{BOARD_X, BOARD_Y, COLS, Game, Kind, Phase, ROWS, TILE};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;

const BRASS: Color = Color::new(0.91, 0.73, 0.39, 1.);
const GLASS: Color = Color::new(0.35, 0.83, 0.85, 1.);
const DARK: Color = Color::new(0.055, 0.08, 0.11, 1.);
const WARN: Color = Color::new(1., 0.46, 0.42, 1.);
const SOFT: Color = Color::new(0.8, 0.85, 0.9, 1.);
/// Workbench card and its inner content column.
const CARD_X: f32 = 618.;
const CARD_W: f32 = 318.;
const X0: f32 = 636.;
const X1: f32 = 918.;

fn light(mask: u8) -> Color {
    match mask {
        1 => Color::new(1., 0.33, 0.35, 1.),
        2 => Color::new(0.35, 0.96, 0.52, 1.),
        3 => Color::new(1., 0.88, 0.35, 1.),
        4 => Color::new(0.35, 0.64, 1., 1.),
        5 => Color::new(0.94, 0.45, 0.98, 1.),
        6 => Color::new(0.35, 0.96, 0.98, 1.),
        7 => Color::new(0.95, 0.97, 1., 1.),
        _ => MUTED,
    }
}
fn color_name(mask: u8) -> &'static str {
    match mask {
        1 => "R",
        2 => "G",
        3 => "Y",
        4 => "B",
        5 => "M",
        6 => "C",
        7 => "W",
        _ => "-",
    }
}
fn color_word(mask: u8) -> &'static str {
    match mask {
        1 => "Red",
        2 => "Green",
        3 => "Yellow",
        4 => "Blue",
        5 => "Magenta",
        6 => "Cyan",
        7 => "White",
        _ => "None",
    }
}
/// Shadowless text for flat dark surfaces: crisp, and half the geometry of
/// the shadowed host style.
fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
fn say(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(size, color)).width
}
/// Draw shrunk to `width` if needed, never enlarged.
fn say_fit(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) {
    let w = text_width(value, ink(size, color));
    let size = if w > width { size * width / w } else { size };
    say(value, x, y, size, color);
}
/// Wrapped paragraph; returns the next baseline.
fn paragraph(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) -> f32 {
    let mut baseline = y;
    for line in wrap_text(value, width, ink(size, color)) {
        say(line, x, baseline, size, color);
        baseline += size * 1.35;
    }
    baseline
}
/// Small letter-spaced section label.
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(11., color).with_letter_spacing(1.5)).width
}
fn centered(value: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text_styled(value, x, y, ink(size, color).with_align(TextAlign::Center));
}
/// Chip whose right edge sits on `right`.
fn chip_right(value: &str, right: f32, y: f32, size: f32, color: Color) {
    let w = text_width(value, style(size, color)) + size * 1.3;
    chip(value, right - w, y, size, color);
}
/// `12480` -> `12,480`.
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
/// Gentle 0..1 breathing driven by model ticks.
fn breathe(age: u32, period: f32) -> f32 {
    0.5 + 0.5 * (age as f32 / period * std::f32::consts::TAU).sin()
}
fn diamond(x: f32, y: f32, radius: f32, color: Color) {
    let (n, e, s, w) = (
        vec2(x, y - radius),
        vec2(x + radius, y),
        vec2(x, y + radius),
        vec2(x - radius, y),
    );
    draw_triangle(n, e, s, color);
    draw_triangle(n, s, w, color);
}
/// Cut-gem diamond: lit upper facets, shaded lower facets.
fn gem(x: f32, y: f32, radius: f32, color: Color) {
    let c = vec2(x, y);
    let (n, e, s, w) = (
        vec2(x, y - radius),
        vec2(x + radius, y),
        vec2(x, y + radius),
        vec2(x - radius, y),
    );
    draw_triangle(c, w, n, lighten(color, 0.3));
    draw_triangle(c, n, e, lighten(color, 0.12));
    draw_triangle(c, e, s, darken(color, 0.12));
    draw_triangle(c, s, w, darken(color, 0.28));
}
/// Contact shadow under a round device.
/// Two flat rings fake a soft falloff at a fraction of an anti-aliased fill's cost.
fn shadow(x: f32, y: f32, r: f32) {
    draw_circle(x + 1.5, y + 4.5, r + 1.5, Color::new(0., 0., 0., 0.16));
    draw_circle(x + 1.5, y + 4., r - 0.5, Color::new(0., 0., 0., 0.26));
}
/// Polished housing: lit top, shaded bottom.
fn housing(x: f32, y: f32, r: f32, top: Color, bottom: Color) {
    let rect = Rect::new(x - r, y - r, r * 2., r * 2.);
    shapes::fill_circle(vec2(x, y), r, Fill::vertical(rect, top, bottom));
}

impl Game {
    fn board(&self) {
        let age = self.age;
        let lit_total = self.locks();
        // Chamber title.
        let title = self.content().name;
        let tw = draw_text_styled(title, BOARD_X, 136., style(20., lighten(BRASS, 0.25))).width;
        let (lit, total) = lit_total;
        if lit == total {
            chip("All locks lit", BOARD_X + tw + 12., 122., 12., GLASS);
        }
        // Coordinates.
        for col in 0..COLS {
            centered(
                &char::from(b'A' + col as u8).to_string(),
                BOARD_X + (col as f32 + 0.5) * TILE,
                153.,
                10.,
                with_alpha(MUTED, 0.75),
            );
        }
        for row in 0..ROWS {
            centered(
                &(row + 1).to_string(),
                19.,
                BOARD_Y + (row as f32 + 0.5) * TILE + 4.,
                10.,
                with_alpha(MUTED, 0.75),
            );
        }
        // Workbench plate.
        let (w, h) = (COLS as f32 * TILE, ROWS as f32 * TILE);
        card(BOARD_X - 3., BOARD_Y - 3., w + 6., h + 6., DARK);
        // Cells are batched: one strip per row in the base shade, the
        // alternate checker squares, row bevels, then the column grooves.
        let groove = Color::new(0.05, 0.072, 0.098, 1.);
        let shade = |v: f32| Color::new(v, v + 0.025, v + 0.045, 1.);
        for row in 0..ROWS {
            let y = BOARD_Y + row as f32 * TILE;
            panel(BOARD_X + 1., y + 1., w - 2., TILE - 2., shade(0.095));
            for col in (1 - row % 2..COLS).step_by(2) {
                let x = BOARD_X + col as f32 * TILE;
                panel(x + 1., y + 1., TILE - 2., TILE - 2., shade(0.108));
            }
            panel(
                BOARD_X + 1.,
                y + 1.,
                w - 2.,
                1.,
                Color::new(1., 1., 1., 0.035),
            );
            panel(
                BOARD_X + 1.,
                y + TILE - 2.,
                w - 2.,
                1.,
                Color::new(0., 0., 0., 0.22),
            );
        }
        for col in 1..COLS {
            let x = BOARD_X + col as f32 * TILE;
            panel(x - 1., BOARD_Y, 2., h, groove);
        }
        // Engraved register studs at interior intersections.
        for row in 1..ROWS {
            for col in 1..COLS {
                let x = BOARD_X + col as f32 * TILE;
                let y = BOARD_Y + row as f32 * TILE;
                panel(x - 1., y - 1., 2., 2., Color::new(0.42, 0.47, 0.52, 0.45));
            }
        }
        self.beams();
        self.halos();
        for (i, p) in self.content().pieces.iter().enumerate() {
            let x = BOARD_X + (f32::from(p.x) + 0.5) * TILE;
            let y = BOARD_Y + (f32::from(p.y) + 0.5) * TILE;
            self.piece(i, p.kind, x, y);
        }
        // Cursor.
        let x = BOARD_X + f32::from(self.cursor.0) * TILE;
        let y = BOARD_Y + f32::from(self.cursor.1) * TILE;
        let pulse = if self.phase == Phase::Editing {
            breathe(age, 70.)
        } else {
            0.
        };
        rounded(
            x + 2.,
            y + 2.,
            TILE - 4.,
            TILE - 4.,
            8.,
            with_alpha(BRASS, 0.07),
        );
        outline(
            x + 2.,
            y + 2.,
            TILE - 4.,
            TILE - 4.,
            8.,
            2.,
            with_alpha(BRASS, 0.7 + 0.3 * pulse),
        );
        // Notice line.
        let notice_color = if self.notice.starts_with("Seal rejected") {
            WARN
        } else if self.phase != Phase::Editing || lit == total {
            GLASS
        } else {
            MUTED
        };
        draw_circle(BOARD_X + 4., 546., 3.5, notice_color);
        say_fit(
            self.notice,
            BOARD_X + 14.,
            551.,
            w - 14.,
            14.,
            if notice_color == MUTED {
                SOFT
            } else {
                notice_color
            },
        );
    }
    /// Luminous beams: wide halo, colored body, white-hot core and a photon
    /// pulse flowing outward along every lit half-edge.
    fn beams(&self) {
        let flow = (self.age % 48) as f32 / 48.;
        for row in 0..ROWS {
            for col in 0..COLS {
                let x = BOARD_X + (col as f32 + 0.5) * TILE;
                let y = BOARD_Y + (row as f32 + 0.5) * TILE;
                for (dir, mask) in self.trace.edges[row * COLS + col].iter().enumerate() {
                    if *mask == 0 {
                        continue;
                    }
                    let (dx, dy) = [(1., 0.), (0., 1.), (-1., 0.), (0., -1.)][dir];
                    let border = (dir == 0 && col == COLS - 1)
                        || (dir == 1 && row == ROWS - 1)
                        || (dir == 2 && col == 0)
                        || (dir == 3 && row == 0);
                    let length = if border { TILE / 2. - 2. } else { TILE };
                    let c = light(*mask);
                    let (ex, ey) = (x + dx * length, y + dy * length);
                    draw_line(x, y, ex, ey, 14., with_alpha(c, 0.06));
                    draw_line(x, y, ex, ey, 7., with_alpha(c, 0.16));
                    draw_line(x, y, ex, ey, 3., with_alpha(c, 0.9));
                    draw_line(x, y, ex, ey, 1.2, lighten(c, 0.65));
                    // A soft photon pulse flows away from every source.
                    let t = flow * TILE;
                    if t < length {
                        let end = (t + 14.).min(length);
                        draw_line(
                            x + dx * t,
                            y + dy * t,
                            x + dx * end,
                            y + dy * end,
                            5.,
                            with_alpha(lighten(c, 0.5), 0.35),
                        );
                    }
                }
            }
        }
    }
    /// Additive light around sources and satisfied locks, drawn in one run so
    /// the glows share a single blend batch beneath the device housings.
    fn halos(&self) {
        for (i, p) in self.content().pieces.iter().enumerate() {
            let x = BOARD_X + (f32::from(p.x) + 0.5) * TILE;
            let y = BOARD_Y + (f32::from(p.y) + 0.5) * TILE;
            match p.kind {
                Kind::Emitter { color, .. } => glow(
                    x,
                    y,
                    44.,
                    with_alpha(light(color), 0.34 + 0.12 * breathe(self.age, 90.)),
                ),
                Kind::Receiver(target) if self.trace.received[i] == target => glow(
                    x,
                    y,
                    50.,
                    with_alpha(light(target), 0.42 + 0.18 * breathe(self.age, 80.)),
                ),
                _ => {}
            }
        }
    }
    fn piece(&self, i: usize, kind: Kind, x: f32, y: f32) {
        match kind {
            Kind::Wall => {
                panel(x - 20., y - 17., 44., 42., Color::new(0., 0., 0., 0.3));
                rounded_gradient(
                    x - 22.,
                    y - 22.,
                    44.,
                    44.,
                    7.,
                    Color::new(0.44, 0.41, 0.38, 1.),
                    Color::new(0.25, 0.23, 0.22, 1.),
                );
                panel(x - 16., y - 21., 32., 1., Color::new(1., 1., 1., 0.16));
                let seam = Color::new(0.14, 0.13, 0.13, 0.85);
                draw_line(x - 21., y - 1., x + 21., y - 1., 2., seam);
                draw_line(x - 5., y - 21., x - 5., y - 1., 2., seam);
                draw_line(x + 9., y - 1., x + 9., y + 21., 2., seam);
                // Chipped face and a hairline crack.
                draw_line(x - 15., y + 7., x - 9., y + 13., 1., seam);
                draw_line(x - 9., y + 13., x - 3., y + 12., 1., seam);
                panel(x + 12., y - 15., 5., 2., Color::new(1., 1., 1., 0.08));
            }
            Kind::Emitter { direction, color } => {
                let c = light(color);
                let d = [vec2(1., 0.), vec2(0., 1.), vec2(-1., 0.), vec2(0., -1.)]
                    [usize::from(direction)];
                let at = vec2(x, y);
                shadow(x, y, 20.);
                // Barrel toward the beam.
                shapes::fill_capsule(at, at + d * 22., 7., darken(BRASS, 0.35));
                housing(x, y, 20., lighten(BRASS, 0.25), darken(BRASS, 0.35));
                draw_circle(x, y, 15., DARK);
                draw_circle(x, y, 11., with_alpha(c, 0.22));
                let normal = vec2(-d.y, d.x);
                draw_triangle(
                    at + d * 11.,
                    at - d * 7. + normal * 8.,
                    at - d * 7. - normal * 8.,
                    c,
                );
                draw_circle(at.x + d.x, at.y + d.y, 3., lighten(c, 0.7));
            }
            Kind::Mirror | Kind::Splitter => {
                let splitter = kind == Kind::Splitter;
                shadow(x, y, 20.);
                let rim = if splitter { BRASS } else { GLASS };
                housing(x, y, 20., lighten(rim, 0.15), darken(rim, 0.3));
                // Flat face (cheaper than a second anti-aliased fill), lit from above.
                draw_circle(x, y, 17.5, Color::new(0.08, 0.115, 0.15, 1.));
                draw_circle(x, y - 2.5, 14., Color::new(1., 1., 1., 0.03));
                if splitter {
                    diamond(x, y, 12., Color::new(0.23, 0.36, 0.37, 0.8));
                    draw_line(x - 13., y, x + 13., y, 1., with_alpha(BRASS, 0.6));
                    draw_line(x, y - 13., x, y + 13., 1., with_alpha(BRASS, 0.6));
                }
                let slope = if self.dials[i] == 0 { -1. } else { 1. };
                let a = vec2(x - 13., y - 13. * slope);
                let b = vec2(x + 13., y + 13. * slope);
                let silver = if splitter {
                    with_alpha(lighten(GLASS, 0.35), 0.85)
                } else {
                    lighten(GLASS, 0.45)
                };
                shapes::fill_capsule(a, b, 3., silver);
                // Specular glint along the upper face of the mirror.
                let lift = vec2(slope, -1.) * 0.9;
                draw_line(
                    a.x + 4. + lift.x,
                    a.y + 4. * slope + lift.y,
                    b.x - 9. + lift.x,
                    b.y - 9. * slope + lift.y,
                    1.,
                    Color::new(1., 1., 1., 0.75),
                );
                draw_circle(x, y, 3.5, BRASS);
                draw_circle(x - 0.8, y - 0.8, 1.4, lighten(BRASS, 0.6));
            }
            Kind::Filter => {
                let c = light(1 << self.dials[i]);
                diamond(x + 1.5, y + 4., 23., Color::new(0., 0., 0., 0.38));
                gem(x, y, 23., BRASS);
                gem(x, y, 18., c);
                centered(color_name(1 << self.dials[i]), x, y + 5., 14., INK);
            }
            Kind::Receiver(target) => {
                let got = self.trace.received[i];
                let matched = target == got;
                let tc = light(target);
                shadow(x, y, 22.);
                if matched {
                    housing(x, y, 22., lighten(tc, 0.35), darken(tc, 0.25));
                    draw_circle(x, y, 17., shapes::mix(DARK, tc, 0.3));
                } else {
                    housing(x, y, 22., lighten(BRASS, 0.25), darken(BRASS, 0.35));
                    draw_circle(x, y, 17., DARK);
                    draw_circle_lines(x, y, 14.5, 1., with_alpha(tc, 0.4));
                }
                centered(
                    color_name(target),
                    x,
                    y + 4.,
                    17.,
                    if matched { PAPER } else { tc },
                );
                for n in 0..3 {
                    let on = got & (1 << n) != 0;
                    draw_circle(
                        x - 6. + n as f32 * 6.,
                        y + 11.,
                        2.,
                        if on {
                            light(1 << n)
                        } else {
                            Color::new(0.25, 0.28, 0.32, 1.)
                        },
                    );
                }
            }
        }
    }
    fn projected_stars(&self) -> u8 {
        let moves = self.rotations();
        let par = self.par();
        if moves <= par && !self.hinted && self.resets == 0 && self.mistakes == 0 {
            3
        } else if moves <= par * 2 && self.resets <= 2 {
            2
        } else {
            1
        }
    }
    fn stars(value: u8, x: f32, y: f32, size: f32) -> f32 {
        let mut at = x;
        for n in 0..3 {
            let on = n < value;
            at += draw_text_styled(
                "★",
                at,
                y,
                ink(
                    size,
                    if on {
                        BRASS
                    } else {
                        Color::new(0.3, 0.33, 0.37, 1.)
                    },
                ),
            )
            .width
                + size * 0.2;
        }
        at - x
    }
    fn workbench(&self) {
        surface(CARD_X, 120., CARD_W, 446.);
        rounded(CARD_X + 18., 120., 56., 3., 1.5, BRASS);
        let (lit, total) = self.locks();
        if self.phase == Phase::Cleared || self.phase == Phase::Finished {
            self.sealed_panel();
        } else {
            self.editing_panel(lit, total);
        }
        // Seal / continue button (shares the model's click region).
        let ready = lit == total || self.phase != Phase::Editing;
        let (bx, by, bw, bh) = (630., 510., 294., 42.);
        if ready {
            shapes::drop_shadow(
                Rect::new(bx, by, bw, bh),
                11.,
                shapes::Shadow::soft(with_alpha(GLASS, 0.22 + 0.12 * breathe(self.age, 80.)))
                    .blur(18.),
            );
            rounded_gradient(
                bx,
                by,
                bw,
                bh,
                11.,
                lighten(GLASS, 0.2),
                darken(GLASS, 0.12),
            );
            outline(bx, by, bw, bh, 11., 1., Color::new(1., 1., 1., 0.3));
        } else {
            rounded_gradient(
                bx,
                by,
                bw,
                bh,
                11.,
                Color::new(0.16, 0.15, 0.13, 1.),
                Color::new(0.11, 0.11, 0.1, 1.),
            );
            outline(bx, by, bw, bh, 11., 1., with_alpha(BRASS, 0.35));
        }
        let (key, label) = if self.phase == Phase::Editing {
            ("5", "Seal chamber")
        } else if self.phase == Phase::Finished {
            ("", "Foundry restored")
        } else {
            ("Space", "Next chamber")
        };
        let ink = if ready { INK } else { BRASS };
        say(label, bx + 16., by + 27., 17., ink);
        if !key.is_empty() {
            let kw = text_width(key, style(13., PAPER)) + 12.;
            keycap(key, "", bx + bw - 14. - kw.max(20.), by + 11., PAPER);
        }
        // Campaign progress: one pill per chamber.
        for (i, stars) in self.stars.iter().enumerate() {
            let x = 632. + i as f32 * 24.;
            let color = match *stars {
                3 => BRASS,
                2 => with_alpha(BRASS, 0.72),
                1 => with_alpha(BRASS, 0.45),
                _ if i == self.level => with_alpha(GLASS, 0.75),
                _ => Color::new(1., 1., 1., 0.1),
            };
            panel(x, 557., 19., 4., color);
        }
    }
    fn editing_panel(&self, lit: usize, total: usize) {
        eyebrow("TARGET LOCKS", X0, 148., MUTED);
        chip_right(
            &format!("{lit} of {total} lit"),
            X1,
            135.,
            12.,
            if lit == total { GLASS } else { BRASS },
        );
        let mut row = 0;
        for (i, p) in self.content().pieces.iter().enumerate() {
            let Kind::Receiver(target) = p.kind else {
                continue;
            };
            let actual = self.trace.received[i];
            let ok = actual == target;
            let y = 160. + row as f32 * 32.;
            rounded(
                X0 - 6.,
                y,
                X1 - X0 + 12.,
                27.,
                7.,
                if ok {
                    with_alpha(GLASS, 0.1)
                } else {
                    Color::new(1., 1., 1., 0.035)
                },
            );
            let base = y + 18.;
            let mid = y + 13.5;
            say(
                &format!("{}{}", char::from(b'A' + p.x), p.y + 1),
                X0 + 4.,
                base,
                13.,
                BRASS,
            );
            say("Need", X0 + 40., base, 12., MUTED);
            draw_circle(X0 + 82., mid, 4.5, light(target));
            say(color_word(target), X0 + 92., base, 13., PAPER);
            say("Have", X0 + 156., base, 12., MUTED);
            if actual == 0 {
                draw_circle(X0 + 198., mid, 4., Color::new(0.25, 0.28, 0.32, 1.));
            } else {
                draw_circle(X0 + 198., mid, 4.5, light(actual));
            }
            say(
                color_word(actual),
                X0 + 208.,
                base,
                13.,
                if actual == 0 { MUTED } else { PAPER },
            );
            if ok {
                draw_text_styled("✓", X1, base, ink(14., GLASS).with_align(TextAlign::Right));
            }
            row += 1;
        }
        // Turn statistics flow directly under the lock list.
        let top = 160. + row as f32 * 32. + 22.;
        let cols = [
            ("TURNS", self.rotations().to_string()),
            ("PAR", self.par().to_string()),
            ("RESETS", self.resets.to_string()),
        ];
        for (n, (name, value)) in cols.iter().enumerate() {
            let x = X0 + [0., 82., 140.][n];
            eyebrow(name, x, top, MUTED);
            let over = n == 0 && self.rotations() > self.par();
            say(
                value,
                x,
                top + 27.,
                22.,
                if over { lighten(BRASS, 0.2) } else { PAPER },
            );
        }
        eyebrow("PACE", X0 + 228., top, MUTED);
        Self::stars(self.projected_stars(), X0 + 228., top + 24., 14.);
        divider(X0, top + 46., X1 - X0, Color::new(1., 1., 1., 0.12));
        let top = top + 70.;
        // Device under the cursor.
        let (name, detail) = self
            .selected()
            .map(|i| match self.content().pieces[i].kind {
                Kind::Mirror => ("Mirror", "Turns between / and \\ to bend light"),
                Kind::Splitter => ("Splitter", "Passes straight and reflects a branch"),
                Kind::Filter => ("Filter", "Passes one channel: R, then G, then B"),
                Kind::Receiver(_) => ("Lock", "Needs its exact color, nothing extra"),
                Kind::Emitter { .. } => ("Source", "Emits a continuous beam"),
                Kind::Wall => ("Stone", "Absorbs all light"),
            })
            .unwrap_or(("Empty cell", "Beams cross here freely"));
        let w = eyebrow("SELECTED", X0, top, MUTED);
        eyebrow(
            &format!("{}{}", char::from(b'A' + self.cursor.0), self.cursor.1 + 1),
            X0 + w + 8.,
            top,
            BRASS,
        );
        say(name, X0, top + 25., 18., PAPER);
        say_fit(detail, X0, top + 46., X1 - X0, 13., MUTED);
        if self.hinted {
            let (hx, hy, hw, hh) = (X0 - 6., 402., X1 - X0 + 12., 96.);
            rounded(hx, hy, hw, hh, 8., with_alpha(BRASS, 0.08));
            outline(hx, hy, hw, hh, 8., 1., with_alpha(BRASS, 0.3));
            eyebrow("ENGINEER'S HINT", X0 + 4., hy + 20., BRASS);
            paragraph(
                self.content().hint,
                X0 + 4.,
                hy + 40.,
                X1 - X0 - 6.,
                13.,
                SOFT,
            );
        } else {
            let mut x = X0;
            for (key, action) in [("Space", "Rotate"), ("X", "Undo"), ("C", "Reset")] {
                x += keycap(key, action, x, 418., MUTED) + 16.;
            }
            let w = keycap("4", "Hint", X0, 452., MUTED);
            say(
                "· limits this chamber to 2 stars",
                X0 + w + 8.,
                466.,
                12.,
                with_alpha(MUTED, 0.75),
            );
        }
    }
    fn sealed_panel(&self) {
        let finished = self.phase == Phase::Finished;
        eyebrow(
            if finished {
                "ENGINE AWAKE"
            } else {
                "CHAMBER SEALED"
            },
            X0,
            148.,
            GLASS,
        );
        say_fit(self.content().name, X0, 176., X1 - X0, 22., PAPER);
        let earned = self.stars[self.level];
        let w = Self::stars(earned, X0, 224., 30.);
        say(
            &format!("{earned} of 3 stars"),
            X0 + w + 10.,
            216.,
            14.,
            MUTED,
        );
        say(
            &format!("+{} points", 1000 + u32::from(earned) * 250),
            X0,
            254.,
            15.,
            BRASS,
        );
        divider(X0, 276., X1 - X0, Color::new(1., 1., 1., 0.12));
        eyebrow("TOTAL SCORE", X0, 304., MUTED);
        say(&grouped(self.score()), X0, 336., 28., PAPER);
        let all: u32 = self.stars.iter().map(|s| u32::from(*s)).sum();
        eyebrow("CAMPAIGN STARS", X0 + 150., 304., MUTED);
        say(
            &format!("{all} / 36"),
            X0 + 150.,
            336.,
            28.,
            lighten(BRASS, 0.15),
        );
        divider(X0, 358., X1 - X0, Color::new(1., 1., 1., 0.12));
        let cols = [
            ("TURNS", self.rotations().to_string()),
            ("PAR", self.par().to_string()),
            ("RESETS", self.resets.to_string()),
            ("HINT", if self.hinted { "Used" } else { "None" }.to_owned()),
        ];
        for (n, (name, value)) in cols.iter().enumerate() {
            let x = X0 + [0., 70., 124., 206.][n];
            eyebrow(name, x, 386., MUTED);
            say(value, x, 408., 17., SOFT);
        }
        paragraph(
            if finished {
                "Every lock in the city glows. The Daybreak Engine is awake."
            } else {
                "All targets lit. The next chamber adds a new optical idea."
            },
            X0,
            446.,
            X1 - X0,
            13.,
            MUTED,
        );
    }
}
impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.tick(input)
    }
    fn draw(&self) {
        self.board();
        self.workbench();
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!("Chamber {} / 12", self.chamber()),
            objective: self.content().lesson.into(),
            status: format!(
                "{} of 36 stars · {} chambers sealed",
                self.stars.iter().map(|s| u32::from(*s)).sum::<u32>(),
                self.stars.iter().filter(|s| **s > 0).count()
            ),
            score: self.score(),
        }
    }
    fn outcome(&self) -> Outcome {
        self.outcome()
    }
    fn summary(&self) -> String {
        let stars: u32 = self.stars.iter().map(|s| u32::from(*s)).sum();
        format!(
            "The Daybreak Engine hums. Twelve dark workshops glow again, and dawn returns to the glass city. You restored every chamber with {stars} of 36 stars and {} points. Your rank: {}. Replay for cleaner circuits, fewer turns, and a perfect masterwork.",
            self.score(),
            if stars == 36 {
                "Master of Light"
            } else if stars >= 24 {
                "Prism Engineer"
            } else {
                "Foundry Keeper"
            }
        )
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
}
