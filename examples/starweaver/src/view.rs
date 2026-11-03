//! Read-only chart, ship schematics and expedition panels.
//!
//! Painted rectangles mirror the model's pointer hit regions: tabs at
//! `24 + 184 * i` (y 116..145), action rows at x 640..922 in 34-unit steps,
//! the action button at y 526..556 and encounter choices at y `330 + 62 * i`.
use crate::content::*;
use crate::model::{Encounter, Game};
use ferravex_arcade::shapes::{self, Cap, Fill, Stroke};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;
use std::f32::consts::{PI, TAU};

const GOLD: Color = Color::new(0.96, 0.78, 0.38, 1.);
const CYAN: Color = Color::new(0.41, 0.85, 0.9, 1.);
const DIM: Color = Color::new(0.11, 0.19, 0.26, 1.);
const BACK: Color = Color::new(0.027, 0.055, 0.10, 1.);
const GREEN: Color = Color::new(0.46, 0.83, 0.57, 1.);
const RED: Color = Color::new(0.95, 0.42, 0.43, 1.);
const SOFT: Color = Color::new(0.74, 0.8, 0.86, 1.);
/// Alloy, crystal and circuit swatches.
const RESOURCE: [Color; 3] = [
    Color::new(0.93, 0.62, 0.38, 1.),
    Color::new(0.58, 0.8, 1., 1.),
    Color::new(0.74, 0.62, 0.98, 1.),
];

/// Right-panel content column and the left chart panel.
const COL: f32 = 640.;
const COL_W: f32 = 282.;
const MAP: Rect = Rect {
    x: 24.,
    y: 150.,
    w: 588.,
    h: 418.,
};

// ---------------------------------------------------------------------------
// Typography

/// Plain text over the dark panels; a shadow adds nothing here.
fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
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
fn label_center(value: &str, cx: f32, y: f32, size: f32, color: Color) {
    draw_text_styled(value, cx, y, ink(size, color).with_align(TextAlign::Center));
}
/// Label shrunk to `width` when necessary.
fn fitted(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) {
    let s = ink(size, color);
    let w = text_width(value, s);
    let s = if w > width {
        s.with_size(size * width / w)
    } else {
        s
    };
    draw_text_styled(value, x, y, s);
}
fn eyebrow(value: &str, x: f32, y: f32, color: Color) {
    let s = ink(11., color).with_letter_spacing(1.5);
    draw_text_styled(&value.to_uppercase(), x, y, s);
}
/// Wrapped paragraph limited to `max` lines; returns the next baseline.
fn body(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color, max: usize) -> f32 {
    let s = ink(size, color);
    let step = size * 1.34;
    let mut baseline = y;
    for line in wrap_text(value, width, s).take(max) {
        draw_text_styled(line, x, baseline, s);
        baseline += step;
    }
    baseline
}
/// Muted name on the left of the column, value on the right.
fn pair(name: &str, value: &str, y: f32, color: Color) {
    label(name, COL, y, 13., MUTED);
    label_right(value, COL + COL_W, y, 14., color);
}
fn tint(c: [f32; 3], amount: f32) -> Color {
    Color::new(c[0] * amount, c[1] * amount, c[2] * amount, 1.)
}

// ---------------------------------------------------------------------------
// Widgets

/// Selectable row in the right column (hit region x 640..922, 30 tall).
fn action_row(y: f32, name: &str, selected: bool) {
    if selected {
        rounded_gradient(COL, y, COL_W, 30., 8., lighten(DIM, 0.1), DIM);
        outline(COL, y, COL_W, 30., 8., 1., with_alpha(GOLD, 0.45));
        shapes::fill_capsule(vec2(COL + 4.5, y + 8.), vec2(COL + 4.5, y + 22.), 1.5, GOLD);
    } else {
        rounded(COL, y, COL_W, 30., 8., with_alpha(DIM, 0.45));
    }
    let s = ink(14., if selected { PAPER } else { SOFT });
    let w = text_width(name, s);
    let s = if w > 170. {
        s.with_size(14. * 170. / w)
    } else {
        s
    };
    draw_text_styled(name, COL + 14., s.centered_baseline(y + 15.), s);
}
fn row_tag(y: f32, value: &str, color: Color) {
    let w = text_width(value, style(11., color)) + 14.3;
    chip(value, COL + COL_W - 10. - w, y + 7., 11., color);
}
fn row_value(y: f32, value: &str, color: Color) {
    let s = ink(13., color).with_align(TextAlign::Right);
    draw_text_styled(value, COL + COL_W - 12., s.centered_baseline(y + 15.), s);
}
/// The confirm button at the model's 640..922 x 526..556 hit region.
fn action_button(action: &str, enabled: bool) {
    let (x, y, w, h) = (COL, 526., COL_W, 30.);
    if enabled {
        rounded_gradient(x, y, w, h, 9., lighten(GOLD, 0.08), darken(GOLD, 0.12));
    } else {
        rounded(x, y, w, h, 9., with_alpha(DIM, 0.8));
    }
    let cap = keycap("Space", "", x + 8., y + 4., MUTED);
    let s = ink(14., if enabled { INK } else { MUTED });
    draw_text_styled(action, x + 18. + cap, s.centered_baseline(y + h * 0.5), s);
}
/// Lightweight two-layer progress bar.
fn bar(x: f32, y: f32, w: f32, h: f32, fraction: f32, color: Color) {
    let f = if fraction.is_finite() {
        fraction.clamp(0., 1.)
    } else {
        0.
    };
    rounded(x, y, w, h, h * 0.5, Color::new(0.02, 0.04, 0.07, 0.9));
    if f > 0. {
        let fw = (w * f).max(h);
        rounded_gradient(x, y, fw, h, h * 0.5, lighten(color, 0.18), color);
    }
}
/// Small square swatch and amount, e.g. the cost of a recipe; returns width.
fn amount(x: f32, y: f32, kind: usize, value: u16, color: Color) -> f32 {
    rounded(x, y - 7., 6., 6., 1.5, RESOURCE[kind]);
    8. + label(&format!("{value}"), x + 8., y, 12., color)
}
/// Soft light from stacked translucent discs. Unlike additive glows this
/// stays in the main batch, so it adds no draw call.
fn halo(c: Vec2, radius: f32, color: Color) {
    for (k, a) in [(1., 0.05), (0.72, 0.06), (0.45, 0.08)] {
        draw_circle(c.x, c.y, radius * k, with_alpha(color, a));
    }
}
/// Twinkling starfield and faint nebulae inside the chart panel.
fn backdrop(ticks: u64) {
    rounded_gradient(
        MAP.x,
        MAP.y,
        MAP.w,
        MAP.h,
        12.,
        Color::new(0.035, 0.07, 0.125, 1.),
        Color::new(0.02, 0.035, 0.07, 1.),
    );
    let t = ticks as f32;
    for n in 0..85 {
        let x = 31. + ((n * 139 + 29) % 570) as f32;
        let y = 157. + ((n * 71 + 13) % 375) as f32;
        let twinkle = 0.55 + 0.45 * (t * 0.035 + n as f32 * 2.3).sin();
        let big = n % 11 == 0;
        let size = if big { 2. } else { 1.2 };
        let a = if big {
            0.5 + 0.4 * twinkle
        } else {
            0.25 + 0.3 * twinkle
        };
        panel(
            x - size * 0.5,
            y - size * 0.5,
            size,
            size,
            Color::new(0.7, 0.84, 0.95, a),
        );
    }
    outline(
        MAP.x,
        MAP.y,
        MAP.w,
        MAP.h,
        12.,
        1.,
        Color::new(1., 1., 1., 0.06),
    );
}

// ---------------------------------------------------------------------------
// Art

fn ship(x: f32, y: f32, scale: f32, burn: bool) {
    let p = |dx: f32, dy: f32| vec2(x + dx * scale, y + dy * scale);
    if burn {
        draw_triangle(
            p(-13., -5.),
            p(-31., 0.),
            p(-13., 5.),
            with_alpha(CYAN, 0.55),
        );
        draw_triangle(
            p(-13., -3.),
            p(-23., 0.),
            p(-13., 3.),
            Color::new(0.85, 1., 1., 0.9),
        );
    }
    // Shadowed lower hull, lit upper hull, canopy stripe.
    draw_triangle(p(22., 0.), p(-16., -12.), p(-9., 0.), PAPER);
    draw_triangle(
        p(22., 0.),
        p(-16., 12.),
        p(-9., 0.),
        tint([0.6, 0.75, 0.88], 0.8),
    );
    draw_triangle(
        p(22., 0.),
        p(-16., -12.),
        p(-2., -5.),
        Color::new(1., 1., 1., 0.35),
    );
    draw_line(x - 4. * scale, y, x + 12. * scale, y, 3. * scale, CYAN);
}

fn planet(i: usize, ticks: u64) {
    let p = &CHART[i];
    let (x, y) = (p.x, p.y);
    let c = vec2(x, y);
    let r = 16.;
    let t = ticks as f32;
    match i {
        0 => {
            // Ring station: hub, spokes and running lights.
            draw_circle_lines(x, y, r + 7., 5., tint(p.color, 0.55));
            draw_circle_lines(x, y, r + 1., 2., CYAN);
            let spin = t * 0.004;
            for j in 0..8 {
                let a = spin + j as f32 * TAU / 8.;
                let lit = (j + (ticks / 20) as usize).is_multiple_of(8);
                draw_circle(
                    x + a.cos() * (r + 7.),
                    y + a.sin() * (r + 7.),
                    2.6,
                    if lit { CYAN } else { PAPER },
                );
            }
            draw_rectangle(x - 11., y - 2., 22., 4., DIM);
            draw_circle(x, y, 4., tint(p.color, 0.9));
        }
        1 | 6 => {
            // Slowly tumbling asteroid field.
            let spin = t * 0.0015 * if i == 1 { 1. } else { -1. };
            for j in 0..9 {
                let a = j as f32 * 2.4 + spin;
                let d = 7. + (j % 3) as f32 * 7.;
                let rr = 3. + (j % 2) as f32 * 2.;
                let (px, py) = (x + a.cos() * d, y + a.sin() * d * 0.7);
                draw_circle(px, py, rr, tint(p.color, 0.42 + j as f32 * 0.03));
                draw_circle(
                    px - rr * 0.3,
                    py - rr * 0.3,
                    rr * 0.62,
                    tint(p.color, 0.7 + j as f32 * 0.03),
                );
            }
        }
        3 => {
            // Derelict wreck.
            draw_rectangle(x - 22., y - 4., 42., 8., tint(p.color, 0.75));
            draw_rectangle(x - 22., y - 4., 42., 3., tint(p.color, 0.95));
            draw_rectangle(x - 9., y - 15., 4., 30., PAPER);
            draw_rectangle(x + 6., y - 12., 5., 20., tint(p.color, 0.6));
            let blink = if (ticks / 30).is_multiple_of(2) {
                GOLD
            } else {
                darken(GOLD, 0.5)
            };
            draw_circle(x + 27., y + 9., 3., blink);
        }
        _ => {
            if i == 2 {
                // Comet tail.
                draw_triangle(
                    vec2(x - 12., y - 6.),
                    vec2(x - 60., y - 32.),
                    vec2(x + 2., y + 10.),
                    Color::new(p.color[0], p.color[1], p.color[2], 0.22),
                );
                draw_triangle(
                    vec2(x - 10., y - 3.),
                    vec2(x - 42., y - 22.),
                    vec2(x, y + 6.),
                    Color::new(p.color[0], p.color[1], p.color[2], 0.22),
                );
            }
            // Lit sphere: radial light from the upper left, then a rim.
            shapes::fill_circle(
                c,
                r,
                Fill::radial(
                    c - vec2(r * 0.45, r * 0.5),
                    r * 1.75,
                    tint(p.color, 1.15),
                    tint(p.color, 0.32),
                ),
            );
            if i == 4 {
                draw_line(x - r * 0.5, y - r, x + r * 0.2, y + r, 4., BACK);
            }
            if i == 5 {
                draw_circle(x - 5., y + 4., 5., tint([0.12, 0.42, 0.35], 1.));
                draw_circle(x + 9., y - 7., 4., tint([0.12, 0.42, 0.35], 1.));
            }
            shapes::stroke_arc(
                c,
                r - 0.5,
                PI * 0.95,
                PI * 0.6,
                Stroke::new(1.5, Color::new(1., 1., 1., 0.35)).cap(Cap::Round),
            );
            if i == 7 {
                draw_circle_lines(x, y, r + 8., 2., GOLD);
                draw_line(x - 28., y + 9., x + 28., y - 9., 3., GOLD);
            }
            if i == 8 {
                draw_line(x + 4., y - r, x + 4., y - 28., 2., PAPER);
                let pulse = 0.5 + 0.5 * (t * 0.08).sin();
                draw_circle(x + 4., y - 30., 3., GOLD);
                draw_circle(x + 4., y - 30., 6., Color::new(1., 0.8, 0.4, 0.18 * pulse));
            }
        }
    }
}

fn legend() {
    let s = ink(11., MUTED);
    let x = 40.;
    let row = |i: usize| 172. + i as f32 * 15.;
    draw_circle_lines(x + 4., row(0) - 4., 4.5, 1.5, CYAN);
    draw_text_styled("Here", x + 14., row(0), s);
    draw_circle_lines(x + 4., row(1) - 4., 4.5, 1.5, GOLD);
    draw_text_styled("Selected", x + 14., row(1), s);
    panel(x + 1.5, row(2) - 6.5, 5., 5., CYAN);
    draw_text_styled("Surveyed", x + 14., row(2), s);
    draw_circle(x + 4., row(3) - 4., 3., GREEN);
    draw_text_styled("Relic claimed", x + 14., row(3), s);
}

/// Feedback strip along the bottom of the chart panel (two lines at most).
fn notice(value: &str) {
    rounded(34., 530., 568., 30., 8., Color::new(0.04, 0.09, 0.15, 0.92));
    shapes::fill_capsule(vec2(44., 537.), vec2(44., 553.), 1.5, CYAN);
    let s = ink(11.5, Color::new(0.72, 0.92, 0.95, 1.));
    let lines = wrap_text(value, 536., s).count().min(2);
    let step = 12.5;
    let mut y = s.centered_baseline(545.) - step * 0.5 * (lines as f32 - 1.);
    for (i, line) in wrap_text(value, 536., s).take(2).enumerate() {
        if i == 1 && lines == 2 && wrap_text(value, 536., s).nth(2).is_some() {
            draw_text_styled(&format!("{line}..."), 54., y, s);
        } else {
            draw_text_styled(line, 54., y, s);
        }
        y += step;
    }
}

impl Game {
    fn draw_tabs(&self) {
        for (i, name) in ["Chart", "Survey", "Forge", "Ship", "Mission"]
            .iter()
            .enumerate()
        {
            let x = 24. + i as f32 * 184.;
            let active = i == self.page;
            if active {
                rounded_gradient(x, 116., 176., 29., 8., lighten(DIM, 0.12), DIM);
                outline(x, 116., 176., 29., 8., 1., with_alpha(CYAN, 0.3));
                shapes::fill_capsule(vec2(x + 18., 143.), vec2(x + 158., 143.), 1.2, CYAN);
            } else {
                rounded(x, 116., 176., 29., 8., Color::new(0.07, 0.115, 0.18, 0.9));
            }
            let n = ink(11., if active { CYAN } else { with_alpha(MUTED, 0.7) });
            let s = ink(14., if active { PAPER } else { MUTED });
            let number = format!("{}", i + 1);
            let nw = text_width(&number, n) + 7.;
            let w = nw + text_width(name, s);
            let left = x + 88. - w * 0.5;
            let base = s.centered_baseline(130.5);
            draw_text_styled(&number, left, base, n);
            draw_text_styled(name, left + nw, base, s);
        }
    }

    fn draw_chart(&self) {
        let t = self.ticks as f32;
        let selected_lane = self.page == 0 && Self::adjacent(self.sector, self.selected);
        for &(a, b) in &LINKS {
            let (pa, pb) = (vec2(CHART[a].x, CHART[a].y), vec2(CHART[b].x, CHART[b].y));
            let chosen = selected_lane
                && ((a == self.sector && b == self.selected)
                    || (b == self.sector && a == self.selected));
            if chosen {
                // Marching gold dashes from here to the selected star.
                let (from, to) = if a == self.sector { (pa, pb) } else { (pb, pa) };
                let len = from.distance(to);
                let dashes = (len / 12.) as usize;
                let march = (t * 0.03).fract();
                for k in 0..dashes {
                    let s = (k as f32 + march) / dashes as f32;
                    let e = s + 0.5 / dashes as f32;
                    if e <= 1. {
                        let (p, q) = (from.lerp(to, s), from.lerp(to, e));
                        draw_line(p.x, p.y, q.x, q.y, 2., GOLD);
                    }
                }
            } else if a == self.sector || b == self.sector {
                draw_line(
                    pa.x,
                    pa.y,
                    pb.x,
                    pb.y,
                    1.5,
                    Color::new(0.35, 0.7, 0.78, 0.55),
                );
            } else {
                draw_line(pa.x, pa.y, pb.x, pb.y, 1., Color::new(0.3, 0.45, 0.6, 0.28));
            }
        }
        let here = vec2(CHART[self.sector].x, CHART[self.sector].y);
        if self.transit.is_none() {
            halo(here, 34., CYAN);
        }
        for (i, sector) in CHART.iter().enumerate() {
            let c = vec2(sector.x, sector.y);
            if i == self.selected && self.page == 0 {
                let spin = t * 0.01;
                for k in 0..4 {
                    shapes::stroke_arc(
                        c,
                        31.,
                        spin + k as f32 * TAU / 4.,
                        TAU / 4. - 0.45,
                        Stroke::new(1.6, GOLD).cap(Cap::Round),
                    );
                }
            }
            if i == self.sector {
                draw_circle_lines(sector.x, sector.y, 26.5, 2., CYAN);
            }
            planet(i, self.ticks);
            if i == 4 || i == 7 {
                let orbit = t / 90. + i as f32;
                draw_circle(
                    sector.x + orbit.cos() * 23.,
                    sector.y + orbit.sin() * 13.,
                    2.,
                    PAPER,
                );
            }
            let visited = self.sites[i].visited;
            label_center(
                sector.name,
                sector.x,
                sector.y + 40.,
                12.,
                if visited { PAPER } else { MUTED },
            );
            if self.sites[i].claimed {
                draw_circle(sector.x + 22., sector.y - 21., 3., GREEN);
            }
            if self.sites[i].scanned {
                panel(sector.x - 25., sector.y - 23., 5., 5., CYAN);
            }
        }
        if let Some(flight) = self.transit {
            let progress = 1. - f32::from(flight.remaining) / f32::from(flight.total);
            let a = &CHART[flight.from];
            let b = &CHART[flight.to];
            let at = |t: f32| {
                vec2(
                    a.x + (b.x - a.x) * t,
                    a.y + (b.y - a.y) * t - (t * PI).sin() * 14.,
                )
            };
            // Fading exhaust trail behind the ship.
            for k in 1..6 {
                let s = (progress - k as f32 * 0.03).max(0.);
                let p = at(s);
                draw_circle(
                    p.x,
                    p.y,
                    2.4 - k as f32 * 0.35,
                    with_alpha(CYAN, 0.5 - k as f32 * 0.08),
                );
            }
            let p = at(progress);
            halo(p, 16., CYAN);
            ship(p.x, p.y, 0.6, true);
        } else {
            let bob = (t * 0.05).sin() * 1.5;
            ship(here.x + 34., here.y - 16. + bob, 0.48, false);
        }
        legend();
    }

    fn draw_chart_panel(&self) {
        let i = if self.page == 0 {
            self.selected
        } else {
            self.sector
        };
        let p = &CHART[i];
        eyebrow(p.kind, COL, 174., MUTED);
        fitted(p.name, COL, 198., COL_W - 30., 21., tint(p.color, 1.05));
        let swatch = vec2(COL + COL_W - 10., 189.);
        shapes::fill_circle(
            swatch,
            9.,
            Fill::radial(
                swatch - vec2(4., 4.),
                15.,
                tint(p.color, 1.1),
                tint(p.color, 0.35),
            ),
        );
        let transit = self.transit.is_some();
        if self.page == 0 {
            let mut y = body(p.lore, COL, 224., COL_W, 13., PAPER, 3) - 6.;
            let (tag, color) = if p.station {
                ("Station: fuel, repair, trade", GOLD)
            } else {
                ("Remote sector", MUTED)
            };
            chip(tag, COL, y, 12., color);
            y += 32.;
            divider(COL, y, COL_W, Color::new(1., 1., 1., 0.12));
            y += 24.;
            pair(
                "First entry",
                &format!("{} hull risk", p.arrival_damage),
                y,
                if p.arrival_damage > 0 { RED } else { PAPER },
            );
            y += 23.;
            pair(
                "Mining",
                &format!("{} hull per action", p.extraction_damage),
                y,
                if p.extraction_damage > 0 { RED } else { PAPER },
            );
            y += 30.;
            let site = self.sites[i];
            let cache = if site.scanned { site.stock } else { p.supplies };
            eyebrow("Deposits", COL, y, CYAN);
            label_right(
                if site.scanned {
                    "Surveyed"
                } else {
                    "Charted estimate"
                },
                COL + COL_W,
                y,
                11.,
                MUTED,
            );
            y += 10.;
            for (k, name) in RESOURCE_NAMES.iter().enumerate() {
                let x = COL + k as f32 * 96.;
                rounded(x, y, 90., 42., 8., Color::new(0.08, 0.13, 0.2, 1.));
                rounded(x + 9., y + 9., 6., 6., 1.5, RESOURCE[k]);
                label(name, x + 20., y + 16., 11., MUTED);
                label(&format!("{}", cache[k]), x + 9., y + 35., 15., PAPER);
            }
            if !transit {
                let (hint, color) = if i == self.sector {
                    ("You are here. Survey [2] to work this sector.", CYAN)
                } else if Self::adjacent(self.sector, i) {
                    ("Connected lane: costs 1 fuel and 1 watch.", PAPER)
                } else {
                    (
                        "No direct lane. Follow the chart's connecting lines.",
                        MUTED,
                    )
                };
                body(hint, COL, 462., COL_W, 12., color, 2);
                action_button(
                    "Jump to sector",
                    i != self.sector && Self::adjacent(self.sector, i),
                );
            }
        } else {
            pair(
                "Hold",
                &format!("{} / {}", self.hold_used(), self.hold_max()),
                226.,
                PAPER,
            );
            bar(
                COL,
                233.,
                COL_W,
                5.,
                f32::from(self.hold_used()) / f32::from(self.hold_max()),
                if self.hold_used() >= self.hold_max() {
                    RED
                } else {
                    CYAN
                },
            );
            let mut x = COL;
            for (k, name) in RESOURCE_NAMES.iter().enumerate() {
                rounded(x, 250., 6., 6., 1.5, RESOURCE[k]);
                x +=
                    10. + label(
                        &format!("{name} {}", self.cargo[k]),
                        x + 10.,
                        257.,
                        12.,
                        SOFT,
                    ) + 14.;
            }
            let site = self.sites[i];
            let labels = [
                "Survey sector",
                "Mine alloy",
                "Mine crystal",
                "Salvage circuits",
                "Explore landmark",
            ];
            for (r, name) in labels.iter().enumerate() {
                let y = 282. + r as f32 * 34.;
                action_row(y, name, r == self.row);
                match r {
                    0 if site.scanned => row_tag(y, "Done", GREEN),
                    4 if site.claimed => row_tag(y, "Claimed", GREEN),
                    1..=3 => {
                        let left = site.stock[r - 1];
                        row_value(
                            y,
                            &format!("{left} left"),
                            if left == 0 {
                                with_alpha(MUTED, 0.6)
                            } else {
                                RESOURCE[r - 1]
                            },
                        );
                    }
                    _ => {}
                }
            }
            if !transit {
                body(
                    if self.row == 0 {
                        "Each work action costs 1 watch. Scan once to unlock local work; deposits never regenerate."
                    } else if self.row == 4 {
                        "Each work action costs 1 watch. Landmarks can be used once."
                    } else {
                        "Each work action costs 1 watch. Extracts 2 units, 3 with a scanner; X discards one."
                    },
                    COL,
                    474.,
                    COL_W,
                    12.,
                    MUTED,
                    3,
                );
                action_button("Accept selected work", true);
            }
        }
    }

    fn draw_forge(&self) {
        let recipe = &RECIPES[self.row];
        // Blueprint grid behind the schematic.
        for k in 0..9 {
            let x = 250. + k as f32 * 30.;
            draw_line(x, 190., x, 430., 1., Color::new(0.3, 0.6, 0.8, 0.07));
        }
        for k in 0..9 {
            let y = 205. + k as f32 * 30.;
            draw_line(240., y, 500., y, 1., Color::new(0.3, 0.6, 0.8, 0.07));
        }
        eyebrow("Onboard fabricator", 42., 178., CYAN);
        label("Every mechanism has a cost", 42., 202., 19., PAPER);
        for (r, name) in COMPONENT_NAMES.iter().enumerate() {
            let y = 255. + r as f32 * 61.;
            let mounted = self.components[r];
            draw_line(
                222.,
                y + 3.,
                276.,
                330.,
                1.,
                with_alpha(if mounted { GREEN } else { CYAN }, 0.25),
            );
            rounded(
                42.,
                y - 20.,
                180.,
                47.,
                9.,
                Color::new(0.06, 0.1, 0.16, 0.95),
            );
            outline(
                42.,
                y - 20.,
                180.,
                47.,
                9.,
                1.,
                if mounted {
                    with_alpha(GREEN, 0.5)
                } else {
                    Color::new(1., 1., 1., 0.08)
                },
            );
            fitted(
                name,
                54.,
                y - 2.,
                160.,
                14.,
                if mounted { PAPER } else { SOFT },
            );
            label(
                if mounted { "Mounted" } else { "Empty mount" },
                54.,
                y + 15.,
                11.,
                if mounted { GREEN } else { MUTED },
            );
            draw_circle(210., y - 7., 3., if mounted { GREEN } else { DIM });
        }
        let core = vec2(370., 325.);
        draw_circle_lines(core.x, core.y, 93., 1., Color::new(0.3, 0.55, 0.7, 0.35));
        draw_circle_lines(core.x, core.y, 108., 1., Color::new(0.3, 0.55, 0.7, 0.2));
        let spin = self.ticks as f32 * 0.006;
        shapes::stroke_arc(
            core,
            108.,
            spin,
            0.9,
            Stroke::new(2., with_alpha(CYAN, 0.5)).cap(Cap::Round),
        );
        ship(366., 325., 3.4, false);
        if self.upgrades[0] {
            draw_circle_lines(366., 325., 86., 2., CYAN);
        }
        let next = body(recipe.detail, 42., 456., 548., 14., PAPER, 2);
        // Cost chips: green when the hold covers it.
        let mut x = 42.;
        let y = next.max(484.) + 8.;
        x += label("Cost", x, y + 13., 13., MUTED) + 10.;
        for (k, name) in RESOURCE_NAMES.iter().enumerate() {
            let need = recipe.cost[k];
            if need == 0 {
                continue;
            }
            let ok = self.cargo[k] >= need;
            let text = format!("{need} {}", name.to_lowercase());
            x += chip(&text, x, y, 12., if ok { GREEN } else { RED }) + 8.;
        }

        label("Fabricate", COL, 186., 20., CYAN);
        let mut x = COL;
        for (k, name) in RESOURCE_NAMES.iter().enumerate() {
            x += amount(x, 212., k, self.cargo[k], SOFT) + 6.;
            x += label(name, x, 212., 12., MUTED) + 14.;
        }
        for (i, recipe) in RECIPES.iter().enumerate() {
            let y = 230. + i as f32 * 34.;
            action_row(y, recipe.name, i == self.row);
            let done = if i < 3 {
                self.components[i]
            } else {
                self.upgrades[i - 3]
            };
            if done {
                row_tag(y, "Built", GREEN);
            } else {
                // Right-aligned cost swatches.
                let mut right = COL + COL_W - 12.;
                for k in (0..3).rev() {
                    let need = recipe.cost[k];
                    if need == 0 {
                        continue;
                    }
                    let color = if self.cargo[k] >= need { SOFT } else { RED };
                    let w = 8. + text_width(&format!("{need}"), ink(12., color));
                    amount(right - w, y + 19.5, k, need, color);
                    right -= w + 9.;
                }
            }
        }
        body(
            "Build cost: listed materials and 1 watch.",
            COL,
            466.,
            COL_W,
            12.,
            MUTED,
            2,
        );
        action_button("Build selected part", true);
    }

    fn draw_ship(&self) {
        eyebrow("Survey cutter  /  ark escort", 42., 178., MUTED);
        label("The Little Light", 42., 204., 22., CYAN);
        let bob = (self.ticks as f32 * 0.04).sin() * 3.;
        halo(vec2(196., 307. + bob), 30., CYAN);
        ship(313., 307. + bob, 4.2, true);
        let hull_color = if self.hull < 25 { RED } else { GREEN };
        label("Hull", 50., 374., 13., MUTED);
        label_right(&format!("{} / 100", self.hull), 300., 374., 14., PAPER);
        bar(50., 382., 250., 8., f32::from(self.hull) / 100., hull_color);
        label("Fuel", 331., 374., 13., MUTED);
        label_right(
            &format!("{} / {}", self.fuel, self.fuel_max()),
            581.,
            374.,
            14.,
            PAPER,
        );
        bar(
            331.,
            382.,
            250.,
            8.,
            f32::from(self.fuel) / f32::from(self.fuel_max()),
            if self.fuel == 0 { RED } else { CYAN },
        );
        label("Cargo hold", 50., 418., 13., MUTED);
        label_right(
            &format!("{} / {}", self.hold_used(), self.hold_max()),
            300.,
            418.,
            14.,
            PAPER,
        );
        label("Credits", 331., 418., 13., MUTED);
        label_right(&format!("{}", self.credits), 581., 418., 16., GOLD);
        for (k, name) in RESOURCE_NAMES.iter().enumerate() {
            let x = 50. + k as f32 * 180.;
            rounded(x, 430., 171., 32., 8., Color::new(0.06, 0.1, 0.16, 0.95));
            rounded(x + 12., 443., 6., 6., 1.5, RESOURCE[k]);
            label(name, x + 26., 450., 13., SOFT);
            label_right(&format!("{}", self.cargo[k]), x + 159., 451., 16., PAPER);
        }
        chip(
            if self.tow {
                "Distress tow ready: 6 watches + 15 hull"
            } else {
                "Distress charge spent"
            },
            50.,
            484.,
            12.,
            if self.tow { GOLD } else { MUTED },
        );

        label("Ship services", COL, 186., 20., CYAN);
        chip(
            if CHART[self.sector].station {
                "Station link available"
            } else {
                "Field services only"
            },
            COL,
            199.,
            11.,
            if CHART[self.sector].station {
                GOLD
            } else {
                MUTED
            },
        );
        let labels = [
            "Condense fuel".to_owned(),
            "Field patch".to_owned(),
            "Station fuel".to_owned(),
            "Station repair".to_owned(),
            format!("Sell {}", RESOURCE_NAMES[self.sell_kind].to_lowercase()),
            "Distress tow".to_owned(),
        ];
        let costs = [
            "1 crystal",
            "1 alloy + 1 circuit",
            "15 credits",
            "20 credits",
            "",
            "6 watches",
        ];
        for (i, name) in labels.iter().enumerate() {
            let y = 230. + i as f32 * 34.;
            action_row(y, name, i == self.row);
            if i == 4 {
                let price = [8, 12, 16][self.sell_kind];
                row_value(y, &format!("+{price} credits"), GOLD);
            } else if i == 5 && !self.tow {
                row_tag(y, "Spent", MUTED);
            } else {
                row_value(y, costs[i], MUTED);
            }
        }
        let details = [
            "1 crystal makes up to 3 fuel. One watch.",
            "1 alloy and 1 circuit repair up to 25 hull. One watch.",
            "15 credits for up to 5 fuel. Station only. One watch.",
            "20 credits for up to 45 hull. Station only. One watch.",
            "X changes cargo type. Sells 1 unit: alloy 8, crystal 12, circuit 16. One watch.",
            "One-use rescue to Loom: 5 fuel, 15 hull damage, six watches. Cargo is kept.",
        ];
        body(details[self.row], COL, 460., COL_W, 12., MUTED, 3);
        action_button("Accept service", true);
    }

    fn draw_mission(&self) {
        eyebrow("Objective", 42., 178., GOLD);
        label("Components", 42., 204., 22., PAPER);
        body(
            "Build all three components and return to Loom Dock.",
            42.,
            230.,
            545.,
            14.,
            SOFT,
            2,
        );
        for i in 0..3 {
            let y = 256. + i as f32 * 71.;
            let built = self.components[i];
            let held = self.relics[i];
            rounded(42., y, 550., 61., 10., Color::new(0.06, 0.1, 0.16, 0.95));
            outline(
                42.,
                y,
                550.,
                61.,
                10.,
                1.,
                if built {
                    with_alpha(GREEN, 0.45)
                } else {
                    Color::new(1., 1., 1., 0.07)
                },
            );
            let badge = vec2(76., y + 30.5);
            let accent = if built {
                GREEN
            } else if held {
                GOLD
            } else {
                MUTED
            };
            shapes::fill_circle(badge, 17., with_alpha(accent, 0.14));
            draw_circle_lines(badge.x, badge.y, 17., 1.5, with_alpha(accent, 0.7));
            label_center(&format!("{}", i + 1), badge.x, badge.y + 6., 17., accent);
            fitted(COMPONENT_NAMES[i], 108., y + 26., 300., 17., PAPER);
            label(
                &format!("{} at {}", RELIC_NAMES[i], CHART[RELIC_SECTORS[i]].name),
                108.,
                y + 46.,
                12.,
                MUTED,
            );
            let (state, color) = if built {
                ("Mechanism built", GREEN)
            } else if held {
                ("Relic held: forge [3]", GOLD)
            } else {
                ("Survey, then explore", MUTED)
            };
            let w = text_width(state, style(12., color)) + 15.6;
            chip(state, 576. - w, y + 21.5, 12., color);
        }
        chip(
            if self.crew {
                "Far Beacon: all nine engineers aboard"
            } else {
                "Optional: rescue nine at Far Beacon"
            },
            42.,
            478.,
            12.,
            if self.crew { GREEN } else { GOLD },
        );

        eyebrow("Expedition", COL, 174., CYAN);
        let w = label(&format!("{}", self.built()), COL, 214., 36., PAPER);
        label("/ 3 mechanisms", COL + w + 8., 214., 14., MUTED);
        pair(
            "Sectors mapped",
            &format!("{} / 9", self.scanned()),
            248.,
            PAPER,
        );
        divider(COL, 262., COL_W, Color::new(1., 1., 1., 0.1));
        let left = DEADLINE - self.watch.min(DEADLINE);
        pair(
            "Watches remain",
            &format!("{left}"),
            288.,
            if left < 8 { RED } else { GOLD },
        );
        bar(
            COL,
            298.,
            COL_W,
            7.,
            1. - f32::from(self.watch) / f32::from(DEADLINE),
            if left < 8 { RED } else { GOLD },
        );
        body(
            "Each action costs 1 watch. Planning is free.",
            COL,
            338.,
            COL_W,
            13.,
            SOFT,
            2,
        );
        body(
            "The star collapses at watch 52. Escape sooner for a higher score.",
            COL,
            390.,
            COL_W,
            12.,
            MUTED,
            3,
        );
        action_button("Open gate at Loom", self.built() == 3 && self.sector == 0);
    }

    fn draw_transit(&self) {
        let Some(flight) = self.transit else {
            return;
        };
        let (x, y, w, h) = (COL - 4., 470., COL_W + 8., 88.);
        rounded_gradient(
            x,
            y,
            w,
            h,
            10.,
            Color::new(0.07, 0.16, 0.22, 1.),
            Color::new(0.04, 0.09, 0.14, 1.),
        );
        outline(x, y, w, h, 10., 1., with_alpha(CYAN, 0.35));
        eyebrow("Lane transit", COL + 10., y + 22., CYAN);
        label_right(
            &format!("to {}", CHART[flight.to].name),
            COL + COL_W - 8.,
            y + 22.,
            13.,
            PAPER,
        );
        bar(
            COL + 10.,
            y + 36.,
            COL_W - 18.,
            7.,
            1. - f32::from(flight.remaining) / f32::from(flight.total),
            CYAN,
        );
        label("Arrival is automatic.", COL + 10., y + 68., 12., MUTED);
    }

    fn draw_encounter(&self, event: Encounter) {
        panel(16., 112., 928., 456., Color::new(0.01, 0.02, 0.05, 0.62));
        let r = Rect::new(102., 170., 758., 384.);
        shapes::drop_shadow(
            r,
            14.,
            shapes::Shadow::soft(Color::new(0., 0., 0., 0.5))
                .offset(vec2(0., 8.))
                .blur(24.),
        );
        rounded_gradient(
            r.x,
            r.y,
            r.w,
            r.h,
            14.,
            Color::new(0.07, 0.11, 0.17, 1.),
            Color::new(0.03, 0.06, 0.1, 1.),
        );
        outline(r.x, r.y, r.w, r.h, 14., 1., with_alpha(GOLD, 0.35));
        rounded(r.x + 28., r.y, 72., 3., 1.5, GOLD);
        let (title, story, choices) = match event {
            Encounter::Choir => (
                "The Hollow Choir",
                "Choose how to retrieve the lens.",
                [
                    ("Powered probe", "1 fuel, no damage"),
                    ("Dive", "18 hull risk, lens + 1 crystal"),
                    ("Withdraw", "No cost, return later"),
                ],
            ),
            Encounter::Sentinel => (
                "The Sentinel's Toll",
                "Choose how to retrieve the core.",
                [
                    ("Override", "Spend 2 circuits"),
                    ("Pay the toll", "Spend 30 credits"),
                    ("Force the vault", "34 hull risk"),
                ],
            ),
            Encounter::Privateers => (
                "Privateers of Copper Reef",
                "Choose trade, escape or combat.",
                [
                    ("Trade", "1 alloy for 3 circuits"),
                    ("Escape", "Spend 2 fuel"),
                    ("Fight", "22 hull risk, gain 40 credits"),
                ],
            ),
        };
        eyebrow("Encounter", 130., 204., GOLD);
        label(title, 130., 236., 26., PAPER);
        label(story, 130., 266., 15., SOFT);
        let pulse = 0.5 + 0.5 * (self.ticks as f32 * 0.08).sin();
        for (i, (name, detail)) in choices.iter().enumerate() {
            let y = 330. + i as f32 * 62.;
            let on = self.choice == i;
            if on {
                rounded_gradient(130., y, 700., 48., 10., lighten(DIM, 0.1), DIM);
                outline(
                    130.,
                    y,
                    700.,
                    48.,
                    10.,
                    1.,
                    with_alpha(GOLD, 0.5 + 0.3 * pulse),
                );
            } else {
                rounded(130., y, 700., 48., 10., Color::new(0.05, 0.08, 0.13, 1.));
                outline(130., y, 700., 48., 10., 1., Color::new(1., 1., 1., 0.06));
            }
            let badge = vec2(156., y + 24.);
            shapes::fill_circle(badge, 12., with_alpha(if on { GOLD } else { MUTED }, 0.16));
            label_center(
                &format!("{}", i + 1),
                badge.x,
                badge.y + 5.,
                13.,
                if on { GOLD } else { MUTED },
            );
            let s = ink(17., if on { GOLD } else { PAPER });
            let base = s.centered_baseline(y + 24.);
            let w = draw_text_styled(name, 182., base, s).width;
            fitted(
                detail,
                182. + w + 16.,
                base,
                610. - w,
                14.,
                if on { SOFT } else { MUTED },
            );
        }
        let mut x = 130.;
        x += keycap("Up", "", x, 524., MUTED) + 4.;
        x += keycap("Down", "Choose", x, 524., MUTED) + 16.;
        x += keycap("Space", "Confirm", x, 524., MUTED) + 16.;
        label("or click an approach", x, 538., 13., MUTED);
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
        backdrop(self.ticks);
        surface(624., 150., 312., 418.);
        self.draw_tabs();
        match self.page {
            0 | 1 => {
                self.draw_chart();
                self.draw_chart_panel();
            }
            2 => self.draw_forge(),
            3 => self.draw_ship(),
            _ => self.draw_mission(),
        }
        // Reserved strip keeps accepted-event feedback legible above the footer.
        notice(&self.notice);
        self.draw_transit();
        if let Some(event) = self.encounter {
            self.draw_encounter(event);
        }
    }
    fn hud(&self) -> Hud {
        let objective = if let Some(i) = (0..3).find(|&i| !self.components[i]) {
            if self.relics[i] {
                format!("Forge {} [3]", COMPONENT_NAMES[i])
            } else {
                format!(
                    "Recover {} at {}",
                    RELIC_NAMES[i], CHART[RELIC_SECTORS[i]].name
                )
            }
        } else {
            "Return to Loom Dock; open the gate [5]".into()
        };
        Hud {
            stage: format!("Watch {:02}/52 | {}", self.watch, CHART[self.sector].name),
            objective,
            status: format!(
                "Fuel {}  Hull {}  Hold {}/{}  Credits {}",
                self.fuel,
                self.hull,
                self.hold_used(),
                self.hold_max(),
                self.credits
            ),
            score: self.points,
        }
    }
    fn outcome(&self) -> Outcome {
        self.result
    }
    fn summary(&self) -> String {
        match self.result {
            Outcome::Won => format!(
                "Ark escaped in {} watches. {} of 9 sectors mapped. Hull {}. {}. {} points.",
                self.watch,
                self.scanned(),
                self.hull,
                if self.crew {
                    "All nine engineers rescued"
                } else {
                    "Far Beacon left behind"
                },
                self.points
            ),
            Outcome::Lost => format!(
                "{}. {} mechanisms built; {} sectors mapped. Score {}. Restart to plan another route.",
                if self.hull == 0 {
                    "Your ship was lost"
                } else {
                    "The star collapsed"
                },
                self.built(),
                self.scanned(),
                self.points
            ),
            Outcome::Playing => format!(
                "{} of 3 mechanisms fitted. {} watches remain.",
                self.built(),
                DEADLINE - self.watch
            ),
        }
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
}
