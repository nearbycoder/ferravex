//! Read-only heist art and the shared arcade adapter.
//!
//! Floor tiles stay flat quads with a one-unit grout gap; walls gain a
//! three-quarter face and gold trim, and anti-aliased shapes are reserved for
//! characters, objectives and the interface cards.
use crate::model::{COLS, Game, MAP_X, MAP_Y, Phase, Pos, ROWS, TILE};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, wrap_text};
use ferravex_arcade::*;

const GOLD: Color = Color::new(0.88, 0.71, 0.39, 1.);
const VELVET: Color = Color::new(0.61, 0.33, 0.67, 1.);
const ORCHID: Color = Color::new(0.80, 0.60, 0.88, 1.);
const MINT: Color = Color::new(0.42, 0.86, 0.75, 1.);
const RED: Color = Color::new(0.99, 0.34, 0.35, 1.);
const CARD: Color = Color::new(0.085, 0.06, 0.11, 1.);
const MAP_W: f32 = TILE * COLS as f32;
const MAP_H: f32 = TILE * ROWS as f32;
/// Sidebar content column.
const SX: f32 = 688.;
const SR: f32 = 928.;
const SW: f32 = SR - SX;

fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new(r, g, b, a)
}
fn center(p: Pos) -> Vec2 {
    vec2(
        MAP_X + (f32::from(p.x) + 0.5) * TILE,
        MAP_Y + (f32::from(p.y) + 0.5) * TILE,
    )
}
/// Card text style: headline sizes keep the host's crisp drop shadow; body
/// text on the dark cards skips it, which halves its geometry.
fn ink(size: f32, color: Color) -> TextStyle {
    if size >= 18. {
        style(size, color)
    } else {
        TextStyle::modern(size, color)
    }
}
fn label(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(size, color)).width
}
fn label_right(value: &str, right: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(
        value,
        right,
        y,
        ink(size, color).with_align(TextAlign::Right),
    )
    .width
}
fn eyebrow_style(color: Color) -> TextStyle {
    TextStyle::modern(10., color).with_letter_spacing(1.5)
}
/// Wrapped body copy; returns the next baseline.
fn para(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color);
    let mut baseline = y;
    for line in wrap_text(value, width, s) {
        draw_text_styled(line, x, baseline, s);
        baseline += size * 1.35;
    }
    baseline
}
/// Small letter-spaced section header.
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, eyebrow_style(color)).width
}
/// Tiny map tag naming the help-text letter of an object.
fn tag(letter: &str, x: f32, y: f32, color: Color) {
    draw_rectangle(x, y, 10., 11., rgba(0.04, 0.03, 0.06, 0.85));
    draw_text_styled(
        letter,
        x + 5.,
        y + 9.,
        TextStyle::modern(8., color).with_align(TextAlign::Center),
    );
}
fn record_icon(x: f32, y: f32, scale: f32) {
    let (w, h) = (15. * scale, 19. * scale);
    draw_rectangle(x + 1.5, y + 1.5, w, h, rgba(0., 0., 0., 0.4));
    draw_rectangle(x, y, w, h, PAPER);
    draw_rectangle(
        x + 2. * scale,
        y + 3. * scale,
        w - 4. * scale,
        2.5 * scale,
        GOLD,
    );
    for i in 0..3 {
        draw_rectangle(
            x + 2. * scale,
            y + (8. + i as f32 * 3.) * scale,
            w - 5. * scale,
            1.,
            MUTED,
        );
    }
    draw_poly(
        x + w - 2. * scale,
        y + h - 2. * scale,
        8,
        3.5 * scale,
        0.,
        RED,
    );
}
fn painting_icon(x: f32, y: f32, w: f32, h: f32) {
    draw_rectangle(x, y, w, h, GOLD);
    draw_rectangle(x + 0.5, y + 0.5, w - 1., 1., rgba(1., 0.94, 0.75, 1.));
    let (ix, iy, iw, ih) = (x + 2.5, y + 2.5, w - 5., h - 5.);
    draw_rectangle(ix, iy, iw, ih, rgba(0.16, 0.22, 0.42, 1.));
    draw_triangle(
        vec2(ix, iy + ih),
        vec2(ix + iw * 0.4, iy + ih * 0.35),
        vec2(ix + iw * 0.8, iy + ih),
        rgba(0.30, 0.52, 0.42, 1.),
    );
    draw_triangle(
        vec2(ix + iw * 0.45, iy + ih),
        vec2(ix + iw * 0.75, iy + ih * 0.55),
        vec2(ix + iw, iy + ih),
        rgba(0.22, 0.40, 0.34, 1.),
    );
    draw_poly(
        ix + iw * 0.78,
        iy + ih * 0.3,
        8,
        ih * 0.14,
        0.,
        rgba(1., 0.85, 0.5, 1.),
    );
}
fn coin_icon(x: f32, y: f32) {
    draw_poly(x, y, 12, 5.5, 0., darken(GOLD, 0.25));
    draw_poly(x - 0.5, y - 0.5, 12, 4.5, 0., GOLD);
    draw_rectangle(x - 1.5, y - 3., 1.5, 4., rgba(1., 0.95, 0.75, 0.9));
}
fn key_icon(x: f32, y: f32, color: Color, hole: Color) {
    draw_poly(x - 4., y, 10, 5., 0., color);
    draw_poly(x - 4., y, 10, 2.2, 0., hole);
    draw_line(x, y, x + 9., y, 2., color);
    draw_line(x + 7., y, x + 7., y + 4., 2., color);
    draw_line(x + 4., y, x + 4., y + 3., 2., color);
}
impl ArcadeGame for Game {
    fn new() -> Self {
        Game::new()
    }
    fn update(&mut self, i: &Input) -> Cue {
        self.step(i)
    }
    fn outcome(&self) -> Outcome {
        Game::outcome(self)
    }
    fn summary(&self) -> String {
        Game::summary(self)
    }
    fn bot_input(&self, t: u64) -> Input {
        self.pilot(t)
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!("Heist {} / 6", self.heist()),
            objective: format!("{}: take every record, then exit.", self.content().object),
            status: format!(
                "{} records safe \u{b7} {} favors \u{b7} {} medals",
                self.total_loot,
                self.retries,
                self.medals.iter().sum::<u8>()
            ),
            score: self.score,
        }
    }
    fn draw(&self) {
        rounded_gradient(
            16.,
            112.,
            928.,
            456.,
            0.,
            rgba(0.06, 0.04, 0.08, 1.),
            rgba(0.035, 0.025, 0.05, 1.),
        );
        self.draw_map();
        self.draw_sidebar();
        if matches!(
            self.phase,
            Phase::Briefing | Phase::Debrief | Phase::Setback | Phase::Escaped | Phase::Caught
        ) {
            self.draw_card();
        }
    }
}
impl Game {
    /// Model tick driving idle motion; frozen outside active infiltration.
    fn wave(&self, speed: f32, phase: f32) -> f32 {
        ((self.elapsed % 100_000) as f32 * speed + phase).sin()
    }
    fn draw_map(&self) {
        card(16., 116., 642., 448., rgba(0.07, 0.05, 0.09, 1.));
        label(self.content().name, 32., 142., 18., GOLD);
        // Legend, right-aligned in the header row.
        let mut x = 642.;
        for (name, color) in [
            ("Exit", MINT),
            ("Record", PAPER),
            ("Sight", GOLD),
            ("Shadow", ORCHID),
        ] {
            x -= label_right(name, x, 141., 11., MUTED);
            x -= 6.;
            shapes::fill_rounded_rect(Rect::new(x - 9., 132., 9., 9.), 2.5, color);
            x -= 9. + 14.;
        }
        draw_rectangle(
            MAP_X - 1.,
            MAP_Y - 1.,
            MAP_W + 1.,
            MAP_H + 1.,
            rgba(0.03, 0.02, 0.04, 1.),
        );
        for y in 0..ROWS {
            for x in 0..COLS {
                self.draw_tile(Pos::new(x as u8, y as u8));
            }
        }
        if let Some((p, t)) = self.noise {
            let c = center(p);
            for k in 0..2 {
                let phase = ((180 - t) as f32 + k as f32 * 18.) % 36. / 36.;
                draw_circle_lines(
                    c.x,
                    c.y,
                    5. + phase * 16.,
                    1.5,
                    with_alpha(GOLD, 0.9 * (1. - phase)),
                );
            }
            coin_icon(c.x, c.y);
        }
        if let Some(p) = self.aim {
            let c = center(p);
            draw_circle_lines(c.x, c.y, 13., 1.5, with_alpha(PAPER, 0.85));
            for (dx, dy) in [(1., 0.), (-1., 0.), (0., 1.), (0., -1.)] {
                draw_line(
                    c.x + dx * 9.,
                    c.y + dy * 9.,
                    c.x + dx * 18.,
                    c.y + dy * 18.,
                    1.5,
                    PAPER,
                );
            }
        }
        for g in &self.guards[..self.guard_count] {
            self.draw_guard(g.pos, g.facing, g.search > 0);
        }
        self.draw_thief();
    }
    fn draw_tile(&self, p: Pos) {
        let (x, y) = (usize::from(p.x), usize::from(p.y));
        let t = self.tile(p);
        let c = center(p);
        let (px, py) = (c.x - TILE / 2., c.y - TILE / 2.);
        if t == b'#' {
            let below = p.shifted(0, 1).is_some_and(|b| self.tile(b) != b'#');
            let above = p.shifted(0, -1).is_some_and(|b| self.tile(b) != b'#');
            draw_rectangle(px, py, TILE, TILE, rgba(0.115, 0.095, 0.15, 1.));
            draw_rectangle(px, py, TILE, 1., rgba(0.17, 0.14, 0.21, 1.));
            draw_rectangle(px + TILE - 1., py, 1., TILE, rgba(0.07, 0.055, 0.09, 1.));
            if above {
                draw_rectangle(px, py, TILE, 2., with_alpha(GOLD, 0.55));
            }
            if below {
                // Wainscot face with a gold rail.
                draw_rectangle(px, py + 25., TILE, 14., rgba(0.07, 0.05, 0.09, 1.));
                draw_rectangle(px, py + 24., TILE, 1.5, GOLD);
                draw_rectangle(px, py + 30., TILE, 1., rgba(0.36, 0.27, 0.16, 1.));
                for i in 0..2 {
                    draw_rectangle(
                        px + 9. + i as f32 * 19.,
                        py + 32.,
                        1.,
                        6.,
                        rgba(0.2, 0.15, 0.12, 1.),
                    );
                }
            } else if (x + y) % 3 == 0 {
                // Deco medallion on the cap.
                let m = vec2(c.x, c.y - 2.);
                draw_poly(m.x, m.y, 4, 5., 45., rgba(0.2, 0.16, 0.2, 1.));
                draw_poly(m.x, m.y, 4, 2.5, 45., rgba(0.32, 0.25, 0.2, 1.));
            }
            return;
        }
        let shadow = self.shadow(p);
        let base = if shadow {
            if (x + y) % 2 == 0 {
                rgba(0.17, 0.10, 0.23, 1.)
            } else {
                rgba(0.155, 0.09, 0.21, 1.)
            }
        } else if (x + y) % 2 == 0 {
            rgba(0.27, 0.15, 0.20, 1.)
        } else {
            rgba(0.235, 0.13, 0.175, 1.)
        };
        draw_rectangle(px, py, TILE - 1., TILE - 1., base);
        draw_rectangle(px, py, TILE - 1., 1., lighten(base, 0.08));
        if shadow {
            draw_line(
                px + 4.,
                py + TILE - 6.,
                px + TILE - 6.,
                py + 4.,
                1.,
                rgba(0.25, 0.16, 0.33, 1.),
            );
        } else if (x + y) % 2 == 0 {
            // Carpet diamond.
            draw_poly(c.x, c.y, 4, 4., 0., darken(base, 0.18));
        }
        // Occlusion under a wall face.
        if p.shifted(0, -1).is_some_and(|a| self.tile(a) == b'#') {
            draw_rectangle(px, py, TILE - 1., 5., rgba(0., 0., 0., 0.3));
            draw_rectangle(px, py + 5., TILE - 1., 4., rgba(0., 0., 0., 0.12));
        }
        for g in &self.guards[..self.guard_count] {
            if self.cone(*g, p, false) {
                draw_rectangle(
                    px,
                    py,
                    TILE - 1.,
                    TILE - 1.,
                    if g.search > 0 {
                        rgba(1., 0.35, 0.2, 0.17)
                    } else {
                        rgba(1., 0.82, 0.34, 0.12)
                    },
                );
            }
        }
        let bob = self.wave(0.06, f32::from(p.x)) * 1.2;
        match t {
            b'=' => {
                let r = Rect::new(px + 6., py + 2., TILE - 13., TILE - 5.);
                shapes::fill_rounded_rect(
                    r,
                    3.,
                    Fill::vertical(r, rgba(0.42, 0.30, 0.22, 1.), rgba(0.27, 0.19, 0.15, 1.)),
                );
                shapes::stroke_rounded_rect(r, 3., 2., GOLD);
                for i in 0..2 {
                    draw_rectangle(
                        px + 11. + i as f32 * 9.,
                        py + 7.,
                        6.,
                        11.,
                        rgba(0.2, 0.14, 0.11, 0.8),
                    );
                    draw_rectangle(
                        px + 11. + i as f32 * 9.,
                        py + 21.,
                        6.,
                        11.,
                        rgba(0.2, 0.14, 0.11, 0.8),
                    );
                }
                draw_poly(c.x + 6., c.y + 3., 8, 2.5, 0., GOLD);
            }
            b'E' => {
                let r = Rect::new(px + 4., py + 3., TILE - 9., TILE - 7.);
                shapes::fill_rounded_rect(
                    r,
                    5.,
                    Fill::vertical(r, rgba(0.1, 0.35, 0.3, 0.9), rgba(0.04, 0.16, 0.15, 0.9)),
                );
                shapes::stroke_rounded_rect(r, 5., 1.5, MINT);
                let pulse = self.wave(0.08, 0.) * 2.;
                for i in 0..2 {
                    let ax = c.x - 6. + i as f32 * 7. + pulse;
                    draw_line(ax - 3., c.y - 6., ax + 3., c.y, 2., MINT);
                    draw_line(ax + 3., c.y, ax - 3., c.y + 6., 2., MINT);
                }
                tag("E", px + 1., py + 1., MINT);
            }
            b'K' => {
                let ky = c.y + bob;
                draw_ellipse(c.x, c.y + 12., 9., 2.5, 0., rgba(0., 0., 0., 0.4));
                key_icon(c.x - 2., ky, GOLD, rgba(0.2, 0.12, 0.16, 1.));
                draw_rectangle(c.x - 8., ky - 3., 2., 2., rgba(1., 0.96, 0.8, 1.));
                tag("K", px + 1., py + 1., GOLD);
            }
            b'L' => {
                draw_ellipse(c.x, c.y + 13., 10., 2.5, 0., rgba(0., 0., 0., 0.4));
                record_icon(c.x - 7.5, c.y - 11. + bob, 1.);
                tag("L", px + 1., py + 1., PAPER);
            }
            b'$' => {
                draw_ellipse(c.x, c.y + 13., 11., 2.5, 0., rgba(0., 0., 0., 0.4));
                painting_icon(c.x - 12., c.y - 9. + bob * 0.5, 24., 19.);
                tag("$", px + 1., py + 1., GOLD);
            }
            b'T' => {
                let r = Rect::new(px + 6., py + 5., TILE - 13., TILE - 11.);
                shapes::fill_rounded_rect(r, 4., rgba(0.05, 0.16, 0.16, 1.));
                shapes::stroke_rounded_rect(r, 4., 1.5, if self.disabled { MUTED } else { MINT });
                if self.disabled {
                    draw_line(c.x - 6., c.y - 1., c.x - 2., c.y + 3., 2., MINT);
                    draw_line(c.x - 2., c.y + 3., c.x + 6., c.y - 5., 2., MINT);
                } else {
                    for i in 0..3 {
                        let w = [12., 7., 10.][i];
                        draw_rectangle(px + 10., py + 10. + i as f32 * 5., w, 2., MINT);
                    }
                    if (self.elapsed / 20).is_multiple_of(2) {
                        draw_rectangle(px + 22., py + 20., 4., 3., MINT);
                    }
                }
                tag("T", px + 1., py + 1., MINT);
            }
            b'!' => {
                let tint = if self.disabled {
                    rgba(0.25, 0.4, 0.35, 1.)
                } else {
                    RED
                };
                for ex in [px + 3., px + TILE - 9.] {
                    draw_rectangle(ex, c.y - 5., 5., 10., rgba(0.12, 0.1, 0.14, 1.));
                    draw_rectangle(ex + 1., c.y - 1., 3., 2., tint);
                }
                if !self.disabled {
                    let flicker = 0.75 + 0.25 * self.wave(0.5, f32::from(p.x));
                    draw_line(
                        px + 8.,
                        c.y,
                        px + TILE - 9.,
                        c.y,
                        5.,
                        rgba(1., 0.3, 0.3, 0.22 * flicker),
                    );
                    draw_line(
                        px + 8.,
                        c.y,
                        px + TILE - 9.,
                        c.y,
                        2.,
                        with_alpha(RED, flicker),
                    );
                    draw_line(
                        px + 8.,
                        c.y,
                        px + TILE - 9.,
                        c.y,
                        0.8,
                        rgba(1., 0.85, 0.85, flicker),
                    );
                } else {
                    draw_line(px + 8., c.y, px + TILE - 9., c.y, 1., with_alpha(tint, 0.6));
                }
            }
            b'H' => {
                let r = Rect::new(px + 7., py + 3., TILE - 15., TILE - 7.);
                draw_rectangle(
                    px + 9.,
                    py + TILE - 5.,
                    TILE - 17.,
                    2.,
                    rgba(0., 0., 0., 0.4),
                );
                shapes::fill_rounded_rect(
                    r,
                    3.,
                    Fill::vertical(r, rgba(0.30, 0.18, 0.36, 1.), rgba(0.18, 0.11, 0.24, 1.)),
                );
                shapes::stroke_rounded_rect(r, 3., 1., ORCHID);
                draw_rectangle(
                    c.x - 0.5,
                    py + 6.,
                    1.,
                    TILE - 13.,
                    rgba(0.08, 0.05, 0.1, 1.),
                );
                draw_rectangle(c.x - 3., c.y, 1.5, 3., GOLD);
                draw_rectangle(c.x + 1.5, c.y, 1.5, 3., GOLD);
                tag("H", px + 1., py + 1., ORCHID);
            }
            _ => {}
        }
    }
    fn draw_guard(&self, pos: Pos, facing: (i8, i8), searching: bool) {
        let c = center(pos);
        let (fx, fy) = (f32::from(facing.0), f32::from(facing.1));
        draw_ellipse(c.x, c.y + 12., 12., 4., 0., rgba(0., 0., 0., 0.5));
        // Flashlight beam hint.
        let tip = vec2(c.x + fx * 16., c.y - 3. + fy * 16.);
        draw_line(
            c.x + fx * 6.,
            c.y - 3. + fy * 6.,
            tip.x,
            tip.y,
            4.,
            if searching {
                rgba(1., 0.4, 0.3, 0.45)
            } else {
                rgba(1., 0.88, 0.5, 0.4)
            },
        );
        // Coat, head and peaked cap.
        let coat = Rect::new(c.x - 9., c.y - 2., 18., 15.);
        shapes::fill_rounded_rect(
            coat,
            5.,
            Fill::vertical(coat, rgba(0.16, 0.19, 0.25, 1.), rgba(0.07, 0.08, 0.11, 1.)),
        );
        draw_rectangle(c.x - 1., c.y, 2., 10., GOLD);
        draw_circle(c.x, c.y - 6., 7., rgba(0.84, 0.67, 0.58, 1.));
        draw_rectangle(c.x - 9., c.y - 12., 18., 4., rgba(0.32, 0.17, 0.22, 1.));
        draw_rectangle(c.x - 6., c.y - 16., 12., 5., rgba(0.40, 0.21, 0.27, 1.));
        draw_rectangle(c.x - 6., c.y - 16., 12., 1., rgba(0.6, 0.35, 0.4, 1.));
        draw_rectangle(c.x - 1.5, c.y - 14., 3., 2., GOLD);
        // Eyes look along the facing direction.
        let ex = c.x + fx * 2.;
        draw_rectangle(ex - 3., c.y - 6. + fy, 2., 2., INK);
        draw_rectangle(ex + 1., c.y - 6. + fy, 2., 2., INK);
        if searching {
            let (bx, by) = (c.x + 10., c.y - 18.);
            shapes::fill_circle(vec2(bx, by), 6.5, RED);
            draw_rectangle(bx - 1., by - 4., 2., 5., PAPER);
            draw_rectangle(bx - 1., by + 2., 2., 2., PAPER);
        }
    }
    fn draw_thief(&self) {
        let c = center(self.player);
        let hidden = self.crouched && self.shadow(self.player);
        let lowered = if self.crouched { 5. } else { 0. };
        let breathe = self.wave(0.07, 0.) * 0.8;
        let a = if hidden { 0.62 } else { 1. };
        let ring = if self.seen {
            RED
        } else if hidden {
            ORCHID
        } else {
            VELVET
        };
        draw_ellipse(
            c.x,
            c.y + 12.,
            15.,
            5.,
            0.,
            with_alpha(ring, if self.seen { 0.45 } else { 0.28 }),
        );
        draw_ellipse(c.x, c.y + 12., 11., 3.5, 0., rgba(0., 0., 0., 0.55));
        let top = c.y - 13. + lowered + breathe;
        draw_triangle(
            vec2(c.x, top),
            vec2(c.x - 11., c.y + 12.),
            vec2(c.x + 11., c.y + 12.),
            with_alpha(VELVET, a),
        );
        draw_triangle(
            vec2(c.x, top),
            vec2(c.x + 2., c.y + 12.),
            vec2(c.x + 11., c.y + 12.),
            with_alpha(darken(VELVET, 0.25), a),
        );
        let hy = c.y - 6. + lowered + breathe;
        draw_circle(c.x, hy, 8., with_alpha(ORCHID, a));
        draw_circle(c.x - 2.5, hy - 2.5, 3., rgba(1., 0.9, 1., 0.35 * a));
        let mask = Rect::new(c.x - 6., hy - 2., 12., 5.);
        shapes::fill_rounded_rect(mask, 2.5, with_alpha(INK, a));
        let lx = f32::from(self.facing.0) * 1.5;
        draw_rectangle(c.x - 3.5 + lx, hy - 1., 2., 2., PAPER);
        draw_rectangle(c.x + 1.5 + lx, hy - 1., 2., 2., PAPER);
        if self.seen {
            let pulse = 0.5 + 0.5 * self.wave(0.25, 0.);
            draw_circle_lines(
                c.x,
                c.y,
                18. + pulse * 2.,
                2.,
                with_alpha(RED, 0.6 + pulse * 0.4),
            );
        }
    }
    fn draw_sidebar(&self) {
        card(670., 116., 274., 448., CARD);
        // Stealth state.
        let (state, color) = if self.seen {
            ("Seen \u{b7} break line of sight", RED)
        } else if self.crouched && self.shadow(self.player) {
            ("Hidden in shadow", ORCHID)
        } else if self.crouched {
            ("Crouched \u{b7} exposed", GOLD)
        } else {
            ("Standing \u{b7} audible", MUTED)
        };
        chip(state, SX, 132., 12., color);
        // Clock and alarm.
        let left = self.seconds_left();
        let urgent = left <= 20 && self.phase == Phase::Infiltrating;
        eyebrow("TIME LEFT", SX, 176., MUTED);
        label(
            &format!("{}:{:02}", left / 60, left % 60),
            SX,
            212.,
            26.,
            if urgent { RED } else { GOLD },
        );
        let alarm = self.alarm / 10;
        eyebrow("ALARM", 818., 176., MUTED);
        label(
            &format!("{alarm}%"),
            818.,
            212.,
            26.,
            if alarm >= 50 {
                RED
            } else if alarm > 0 {
                lighten(RED, 0.4)
            } else {
                PAPER
            },
        );
        meter(SX, 224., SW, 8., f32::from(self.alarm) / 1000., RED);
        divider(SX, 248., SW, rgba(1., 1., 1., 0.1));
        // Haul and kit.
        let rows: [(&str, String, Color); 6] = [
            (
                "Records",
                format!("{} / {}", self.stolen, self.needed),
                if self.stolen >= self.needed {
                    MINT
                } else {
                    PAPER
                },
            ),
            (
                "Artworks",
                format!("{} / {}", self.extras, self.extra_total),
                GOLD,
            ),
            (
                "Coins",
                self.coins.to_string(),
                if self.coins > 0 { PAPER } else { MUTED },
            ),
            ("Favors", self.retries.to_string(), ORCHID),
            (
                "Brass pass",
                if self.key { "Acquired" } else { "Find K" }.to_owned(),
                if self.key { GOLD } else { MUTED },
            ),
            (
                "Beams",
                if self.disabled { "Disabled" } else { "Active" }.to_owned(),
                if self.disabled { MINT } else { RED },
            ),
        ];
        for (i, (name, value, color)) in rows.iter().enumerate() {
            let y = 274. + i as f32 * 24.;
            let ix = SX + 7.;
            match i {
                0 => record_icon(ix - 4.5, y - 11., 0.6),
                1 => painting_icon(ix - 7., y - 10., 14., 11.),
                2 => coin_icon(ix, y - 4.5),
                3 => {
                    for k in 0..3u8 {
                        draw_poly(
                            ix - 5. + f32::from(k) * 5.,
                            y - 4.5,
                            4,
                            2.5,
                            0.,
                            if k < self.retries {
                                ORCHID
                            } else {
                                rgba(1., 1., 1., 0.15)
                            },
                        );
                    }
                }
                4 => key_icon(ix - 1., y - 4.5, if self.key { GOLD } else { MUTED }, CARD),
                _ => {
                    draw_line(
                        ix - 6.,
                        y - 4.5,
                        ix + 6.,
                        y - 4.5,
                        2.,
                        if self.disabled { MINT } else { RED },
                    );
                }
            }
            label(name, SX + 22., y, 13., rgba(0.78, 0.74, 0.82, 1.));
            label_right(value, SR, y, 14., *color);
        }
        divider(SX, 406., SW, rgba(1., 1., 1., 0.1));
        if let Some(w) = self.work {
            let name = match w.kind {
                b'=' => "OPENING DOOR",
                b'T' => "REWIRING CONSOLE",
                b'L' => "TAKING RECORD",
                b'K' => "TAKING PASS",
                _ => "PACKING ARTWORK",
            };
            eyebrow(name, SX, 432., ORCHID);
            meter(
                SX,
                442.,
                SW,
                8.,
                1. - f32::from(w.remaining) / f32::from(w.total),
                VELVET,
            );
        } else {
            let x = SX + keycap("Space", "Work", SX, 418., MUTED) + 14.;
            keycap("X", "Crouch", x, 418., MUTED);
            let x = SX + keycap("C", "Coin", SX, 444., MUTED) + 14.;
            keycap("Click", "Aim", x, 444., MUTED);
        }
        para(self.note, SX, 496., SW, 13., rgba(0.88, 0.85, 0.9, 1.));
        let searching = self.guards[..self.guard_count]
            .iter()
            .filter(|g| g.search > 0)
            .count();
        let guards = format!("{} guards \u{b7} {} searching", self.guard_count, searching);
        label(
            &guards,
            SX,
            552.,
            12.,
            if searching > 0 { RED } else { MUTED },
        );
    }
    fn draw_card(&self) {
        draw_rectangle(
            MAP_X - 1.,
            MAP_Y - 1.,
            MAP_W + 1.,
            MAP_H + 1.,
            rgba(0.02, 0.01, 0.03, 0.62),
        );
        card(72., 190., 535., 336., rgba(0.1, 0.07, 0.13, 0.99));
        let x = 98.;
        let w = 483.;
        match self.phase {
            Phase::Briefing => {
                eyebrow(
                    &format!("HEIST {} OF 6 \u{b7} THE ASSIGNMENT", self.heist()),
                    x,
                    222.,
                    GOLD,
                );
                label(self.content().name, x, 254., 24., PAPER);
                label(
                    &format!("Target: {}", self.content().object),
                    x,
                    280.,
                    14.,
                    ORCHID,
                );
                divider(x, 298., w, rgba(1., 1., 1., 0.12));
                let y = para(self.content().briefing, x, 328., w, 16., PAPER);
                let secs = self.content().seconds;
                let mut cx = x;
                for (text, color) in [
                    (format!("{}:{:02} on the clock", secs / 60, secs % 60), GOLD),
                    (format!("{} records", self.needed), PAPER),
                    (format!("{} guards", self.guard_count), RED),
                ] {
                    cx += chip(&text, cx, y - 2., 12., color) + 8.;
                }
                label("The timer starts when you enter.", x, 452., 13., MUTED);
                let kx = x + keycap("Space", "or", x, 478., PAPER) + 7.;
                keycap("Click", "Enter the heist", kx, 478., PAPER);
            }
            Phase::Debrief => {
                eyebrow(&format!("HEIST {} COMPLETE", self.heist()), x, 222., MINT);
                label("Another story freed", x, 254., 24., PAPER);
                self.medals_row(x, 300., self.medals[self.level]);
                label(
                    &format!("{} / 3 medals", self.medals[self.level]),
                    x + 130.,
                    318.,
                    18.,
                    GOLD,
                );
                divider(x, 352., w, rgba(1., 1., 1., 0.12));
                label(
                    &format!(
                        "{} records \u{b7} {} artworks recovered",
                        self.stolen, self.extras
                    ),
                    x,
                    382.,
                    16.,
                    PAPER,
                );
                para(
                    "Coins are refilled for the next job.",
                    x,
                    408.,
                    w,
                    14.,
                    MUTED,
                );
                keycap("Space", "Next heist", x, 478., PAPER);
            }
            Phase::Setback => {
                eyebrow("CAUGHT", x, 222., RED);
                label("The velvet trap", x, 254., 24., PAPER);
                let y = para(self.note, x, 290., w, 16., PAPER) + 10.;
                para(
                    "Spending a favor resets this heist: its score, objects, time and guards.",
                    x,
                    y,
                    w,
                    13.,
                    MUTED,
                );
                eyebrow("FAVORS LEFT", x, 420., MUTED);
                for k in 0..3u8 {
                    let filled = k < self.retries;
                    shapes::fill_circle(
                        vec2(x + 7. + f32::from(k) * 20., 438.),
                        6.,
                        if filled {
                            ORCHID
                        } else {
                            rgba(1., 1., 1., 0.1)
                        },
                    );
                }
                keycap("Space", "Spend a favor and retry", x, 478., PAPER);
            }
            Phase::Escaped => {
                eyebrow("CAREER COMPLETE", x, 222., GOLD);
                label("The midnight broadcast", x, 254., 24., PAPER);
                para(
                    "All six heists complete. Six heists, one true history.",
                    x,
                    290.,
                    w,
                    16.,
                    PAPER,
                );
                let total: u8 = self.medals.iter().sum();
                eyebrow("MEDALS", x, 360., MUTED);
                label_right(&format!("{total} / 18"), x + w, 362., 18., GOLD);
                meter(x, 372., w, 8., f32::from(total) / 18., GOLD);
            }
            Phase::Caught => {
                eyebrow("CAREER OVER", x, 222., RED);
                label("The contact has gone", x, 254., 24., PAPER);
                para("No favors remaining.", x, 290., w, 16., PAPER);
                keycap("R", "Start a new career", x, 478., PAPER);
            }
            _ => {}
        }
    }
    /// Three medal discs, `earned` of them lit.
    fn medals_row(&self, x: f32, y: f32, earned: u8) {
        for k in 0..3u8 {
            let m = vec2(x + 18. + f32::from(k) * 38., y + 12.);
            if k < earned {
                shapes::fill_circle(
                    m,
                    15.,
                    Fill::radial(
                        m - vec2(4., 4.),
                        18.,
                        rgba(1., 0.9, 0.6, 1.),
                        darken(GOLD, 0.25),
                    ),
                );
                let star =
                    TextStyle::modern(18., rgba(0.5, 0.33, 0.1, 0.9)).with_align(TextAlign::Center);
                draw_text_styled("\u{2605}", m.x, star.centered_baseline(m.y), star);
            } else {
                shapes::fill_circle(m, 15., rgba(1., 1., 1., 0.06));
                shapes::stroke_circle(m, 15., 1., rgba(1., 1., 1., 0.15));
            }
        }
    }
}
