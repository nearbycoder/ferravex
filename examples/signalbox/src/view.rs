//! Read-only procedural railway art and the dispatcher's switchboard.
use crate::content::{SHIFTS, Side};
use crate::model::{CONTROL_NAMES, EDGES, Game, Phase, ROUTE_NAMES, TrainState};
use ferravex_arcade::shapes::{self, Fill, Shadow};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;

const BRASS: Color = Color::new(0.92, 0.73, 0.40, 1.);
const CREAM: Color = Color::new(0.94, 0.90, 0.77, 1.);
const WOOD: Color = Color::new(0.19, 0.15, 0.13, 1.);
const GREEN_LAMP: Color = Color::new(0.40, 0.91, 0.66, 1.);
const RED_LAMP: Color = Color::new(0.95, 0.40, 0.34, 1.);
const SOFT: Color = Color::new(0.70, 0.74, 0.70, 1.);
const BOARD: Color = Color::new(0.075, 0.10, 0.11, 1.);
const DESTINATIONS: [Color; 4] = [
    Color::new(0.95, 0.73, 0.42, 1.),
    Color::new(0.55, 0.82, 0.55, 1.),
    Color::new(0.65, 0.75, 0.98, 1.),
    Color::new(0.90, 0.57, 0.67, 1.),
];
/// Short platform names for plates; the model's names carry the letter.
const STATIONS: [&str; 4] = ["Abbey", "Orchard", "Quarry", "Harbour"];
/// Signal lamp shown by each signal control; `None` for the two points.
const CONTROL_SIGNAL: [Option<usize>; 6] = [Some(0), None, Some(1), Some(2), None, Some(3)];

/// Flat text for labels on solid surfaces; shadows double the glyph cost and
/// are only needed over busy art.
fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
fn label(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(size, color)).width
}
fn label_r(value: &str, right: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(
        value,
        right,
        y,
        ink(size, color).with_align(TextAlign::Right),
    )
    .width
}
/// Label shrunk to fit `width`, never enlarged.
fn label_fit(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) {
    let s = ink(size, color);
    let actual = text_width(value, s);
    let s = if actual > width {
        s.with_size(size * width / actual)
    } else {
        s
    };
    draw_text_styled(value, x, y, s);
}
/// Wrapped flat paragraph; returns the next baseline.
fn paragraph(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color);
    let mut baseline = y;
    for line in wrap_text(value, width, s) {
        draw_text_styled(line, x, baseline, s);
        baseline += size * 1.3;
    }
    baseline
}
fn eyebrow_style(color: Color) -> TextStyle {
    ink(10.5, color).with_letter_spacing(1.4)
}
/// Small letter-spaced section header.
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, eyebrow_style(color)).width
}
fn letter(destination: usize) -> char {
    (b'A' + destination as u8) as char
}

fn line(a: Vec2, b: Vec2, width: f32, color: Color) {
    draw_line(a.x, a.y, b.x, b.y, width, color);
}

/// Soft lamp halo: a cheap radial fill rather than an additive glow.
fn halo(at: Vec2, radius: f32, color: Color, strength: f32) {
    shapes::fill_circle(
        at,
        radius,
        Fill::radial(
            at,
            radius,
            with_alpha(color, strength),
            with_alpha(color, 0.),
        ),
    );
}

/// Five-pointed star from ten triangles.
fn star(center: Vec2, outer: f32, color: Color) {
    let inner = outer * 0.45;
    let point = |i: usize| {
        let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.;
        let r = if i.is_multiple_of(2) { outer } else { inner };
        center + vec2(angle.cos(), angle.sin()) * r
    };
    for i in 0..10 {
        draw_triangle(center, point(i), point(i + 1), color);
    }
}

/// Pill-shaped tag centered on `cx` whose text baseline is optically centered.
fn tag(value: &str, cx: f32, cy: f32, size: f32, color: Color, fill: Color) {
    let s = TextStyle::modern(size, color);
    let w = text_width(value, s) + size * 1.2;
    let h = size * 1.55;
    let r = Rect::new(cx - w * 0.5, cy - h * 0.5, w, h);
    shapes::fill_rounded_rect(r, h * 0.5, fill);
    draw_text_styled(
        value,
        cx,
        s.centered_baseline(cy),
        s.with_align(TextAlign::Center),
    );
}

/// A wide call-to-action matching the host's buttons.
fn action_button(x: f32, y: f32, w: f32, h: f32, key: &str, action: &str, base: Color) {
    let r = Rect::new(x, y, w, h);
    shapes::drop_shadow(
        r,
        9.,
        Shadow::soft(Color::new(0., 0., 0., 0.35))
            .offset(vec2(0., 3.))
            .blur(10.),
    );
    shapes::fill_rounded_rect(r, 9., Fill::vertical(r, lighten(base, 0.12), base));
    shapes::stroke_rounded_rect(r, 9., 1., Color::new(1., 1., 1., 0.12));
    let s = ink(16., CREAM);
    let key_w = text_width(key, TextStyle::modern(13., PAPER)) + 12.;
    let total = key_w.max(20.) + 10. + text_width(action, s);
    let left = x + (w - total) * 0.5;
    keycap(key, "", left, y + h * 0.5 - 11., CREAM);
    draw_text_styled(
        action,
        left + key_w.max(20.) + 10.,
        s.centered_baseline(y + h * 0.5),
        s,
    );
}

fn track(a: Vec2, b: Vec2, active: bool, glint: f32) {
    let direction = (b - a).normalize_or_zero();
    let normal = direction.perp();
    // Ballast bed with a darker shoulder.
    line(a, b, 19., Color::new(0.14, 0.17, 0.16, 0.55));
    line(a, b, 15., Color::new(0.27, 0.29, 0.27, 1.));
    let ties = (a.distance(b) / 12.).ceil() as usize;
    for step in 0..=ties {
        let at = a.lerp(b, step as f32 / ties.max(1) as f32);
        line(
            at - normal * 8.,
            at + normal * 8.,
            3.4,
            Color::new(0.42, 0.33, 0.25, 1.),
        );
    }
    let rail = if active {
        lighten(BRASS, 0.08 + glint * 0.12)
    } else {
        Color::new(0.64, 0.68, 0.65, 1.)
    };
    for side in [-3.7, 3.7] {
        line(
            a + normal * (side + 0.9),
            b + normal * (side + 0.9),
            1.4,
            Color::new(0., 0., 0., 0.35),
        );
        line(a + normal * side, b + normal * side, 1.9, rail);
    }
}

impl Game {
    fn landscape(&self) {
        let snow = self.shift == 6;
        let rain = self.shift == 5;
        let (top, bottom) = if snow {
            (
                Color::new(0.64, 0.71, 0.72, 1.),
                Color::new(0.53, 0.61, 0.63, 1.),
            )
        } else if rain {
            (
                Color::new(0.19, 0.27, 0.29, 1.),
                Color::new(0.14, 0.21, 0.23, 1.),
            )
        } else {
            (
                Color::new(0.27, 0.37, 0.31, 1.),
                Color::new(0.21, 0.31, 0.26, 1.),
            )
        };
        let map = Rect::new(20., 120., 704., 389.);
        rounded(
            map.x - 1.,
            map.y + 2.,
            map.w + 2.,
            map.h + 3.,
            13.,
            Color::new(0., 0., 0., 0.35),
        );
        shapes::fill_rounded_rect(map, 12., Fill::vertical(map, top, bottom));
        // Field boundaries and furrows keep the landscape authored and quiet.
        for (x, y, w, h) in [
            (39., 190., 127., 61.),
            (41., 410., 119., 63.),
            (590., 201., 109., 47.),
        ] {
            let field = if snow {
                Color::new(0.72, 0.78, 0.76, 1.)
            } else {
                Color::new(0.35, 0.40, 0.29, 1.)
            };
            rounded(x, y, w, h, 6., field);
            for n in 0..7 {
                let fy = y + 7. + n as f32 * 7.;
                draw_line(
                    x + 6.,
                    fy,
                    x + w - 6.,
                    fy,
                    1.,
                    Color::new(0.47, 0.49, 0.34, 0.4),
                );
            }
        }
        // River with a darker channel and slowly drifting ripples.
        let river = Rect::new(377., 121., 47., 387.);
        shapes::fill_rounded_rect(
            river,
            0.,
            Fill::horizontal(
                river,
                Color::new(0.13, 0.30, 0.36, 1.),
                Color::new(0.17, 0.37, 0.43, 1.),
            ),
        );
        panel(377., 121., 2., 387., Color::new(0.30, 0.42, 0.36, 0.7));
        panel(422., 121., 2., 387., Color::new(0.30, 0.42, 0.36, 0.7));
        let flow = (self.time / 4 % 15) as f32;
        for i in 0..25 {
            let y = 128. + (i as f32 * 15. + flow) % 375.;
            let x = 384. + (i % 3) as f32 * 8.;
            draw_line(
                x,
                y,
                x + 13.,
                y - 2.5,
                1.3,
                Color::new(0.58, 0.78, 0.80, 0.35),
            );
        }
        let nodes = self.nodes();
        for i in 0..26 {
            let x = 44. + ((i * 83 + self.shift * 19) % 646) as f32;
            let y = 152. + ((i * 53) % 327) as f32;
            // Keep vegetation away from all rail centerlines and station buildings.
            let p = vec2(x, y);
            let near_track = EDGES.iter().any(|&(a, b)| {
                let edge = nodes[b] - nodes[a];
                let t = ((p - nodes[a]).dot(edge) / edge.length_squared()).clamp(0., 1.);
                p.distance_squared(nodes[a] + edge * t) < 30. * 30.
            });
            if near_track || (x - 400.).abs() < 40. || (x < 196. && y < 168.) {
                continue;
            }
            draw_poly(
                x + 3.,
                y + 5.,
                12,
                10.,
                0.,
                Color::new(0.04, 0.08, 0.07, 0.35),
            );
            draw_rectangle(x - 1., y + 1., 3., 9., Color::new(0.32, 0.24, 0.17, 1.));
            let (leaf, light) = if snow {
                (Color::new(0.80, 0.85, 0.82, 1.), CREAM)
            } else {
                (
                    Color::new(0.24, 0.42, 0.31, 1.),
                    Color::new(0.43, 0.61, 0.40, 1.),
                )
            };
            draw_poly(x, y, 16, 10., 0., leaf);
            draw_poly(x - 2.5, y - 2.5, 12, 6.5, 0., lighten(leaf, 0.08));
            draw_poly(x - 4., y - 4.5, 8, 3., 0., light);
        }
        // Weather drifts with the railway clock, so planning freezes it too.
        if rain || snow {
            for i in 0..34_u32 {
                let x = 30. + ((i * 97 + 13) % 684) as f32;
                let fall = (self.time * if snow { 1 } else { 5 } + i * 37) % 360;
                let y = 128. + fall as f32;
                if snow {
                    let sway = ((self.time / 20 + i) % 6) as f32 - 3.;
                    draw_rectangle(x + sway, y, 2., 2., Color::new(1., 1., 1., 0.7));
                } else {
                    draw_line(
                        x,
                        y,
                        x - 3.,
                        y + 10.,
                        1.,
                        Color::new(0.70, 0.82, 0.90, 0.35),
                    );
                }
            }
        }
        // The single bridge has visibly separate entrance signals.
        let bridge_y = self.schedule().bridge_y;
        rounded(
            347.,
            bridge_y - 19.,
            105.,
            44.,
            4.,
            Color::new(0., 0., 0., 0.25),
        );
        panel(
            349.,
            bridge_y - 16.,
            101.,
            32.,
            Color::new(0.40, 0.37, 0.32, 1.),
        );
        for i in 0..13 {
            let x = 351. + i as f32 * 8.;
            draw_line(
                x,
                bridge_y - 15.,
                x,
                bridge_y + 15.,
                2.,
                Color::new(0.62, 0.55, 0.41, 1.),
            );
        }
        for side in [-18., 18.] {
            draw_line(
                350.,
                bridge_y + side,
                450.,
                bridge_y + side,
                3.,
                Color::new(0.66, 0.70, 0.66, 1.),
            );
        }
        // District plaque.
        let district = if snow {
            "WINTER LINE"
        } else if rain {
            "WET RAILS"
        } else {
            "DISTRICT 04"
        };
        let w = text_width(district, eyebrow_style(SOFT))
            .max(text_width("Bramble Junction", ink(15., CREAM)))
            + 22.;
        rounded(28., 128., w, 42., 8., Color::new(0.03, 0.05, 0.05, 0.6));
        eyebrow(district, 39., 143., if rain { BRASS } else { SOFT });
        label("Bramble Junction", 39., 162., 15., CREAM);
    }

    fn railway(&self) {
        let nodes = self.nodes();
        let glint = ((self.time / 6) % 20) as f32 / 20.;
        let glint = 1. - (glint * 2. - 1.).abs();
        for (a, b) in EDGES {
            let active = match (a, b) {
                (2, 3) => self.points[0] == 0,
                (2, 8) => self.points[0] == 1,
                (2, 9) => self.points[0] == 2,
                (4, 5) => self.points[1] == 0,
                (5, 10) => self.points[1] == 1,
                (5, 11) => self.points[1] == 2,
                _ => false,
            };
            track(nodes[a], nodes[b], active, glint);
        }
        for station in 0..4 {
            let p = nodes[station + 8];
            let color = DESTINATIONS[station];
            // Platform, building and roof.
            panel(
                p.x - 48.,
                p.y - 12.,
                100.,
                42.,
                Color::new(0., 0., 0., 0.22),
            );
            panel(p.x - 50., p.y + 16., 100., 9., CREAM);
            for stripe in 0..12 {
                panel(
                    p.x - 47. + stripe as f32 * 8.,
                    p.y + 18.,
                    4.,
                    3.,
                    darken(color, 0.1),
                );
            }
            let body = Rect::new(p.x - 44., p.y - 12., 88., 29.);
            shapes::fill_rounded_rect(
                body,
                0.,
                Fill::vertical(
                    body,
                    Color::new(0.74, 0.69, 0.55, 1.),
                    Color::new(0.56, 0.52, 0.41, 1.),
                ),
            );
            rounded_gradient(
                p.x - 47.,
                p.y - 17.,
                94.,
                9.,
                3.,
                lighten(color, 0.15),
                darken(color, 0.15),
            );
            for window in 0..4 {
                let wx = p.x - 33. + window as f32 * 19.;
                panel(wx, p.y - 3., 9., 11., Color::new(0.15, 0.21, 0.23, 1.));
                panel(wx, p.y - 3., 9., 4., Color::new(0.98, 0.85, 0.55, 0.55));
            }
            // Name plate: lettered disc plus the station name.
            let name = STATIONS[station];
            let s = ink(12., CREAM);
            let w = text_width(name, s) + 36.;
            let plate = Rect::new(p.x - w * 0.5, p.y - 44., w, 22.);
            shapes::fill_rounded_rect(plate, 11., Color::new(0.04, 0.06, 0.07, 0.88));
            let disc = vec2(plate.x + 12., plate.y + 11.);
            shapes::fill_circle(disc, 8., color);
            let ds = TextStyle::modern(11., INK).with_align(TextAlign::Center);
            draw_text_styled(
                &letter(station).to_string(),
                disc.x + 0.5,
                ds.centered_baseline(disc.y),
                ds,
            );
            draw_text_styled(name, plate.x + 26., s.centered_baseline(disc.y), s);
        }
        for (node, side) in [(0, "West yard"), (7, "East yard")] {
            let p = nodes[node];
            rounded(p.x - 11., p.y + 11., 22., 5., 2., CREAM);
            if node == 0 {
                label(side, 31., p.y + 46., 11., CREAM);
            } else {
                label_r(side, 714., p.y + 46., 11., CREAM);
            }
        }
        for (idx, node) in [(0, 1), (1, 3), (2, 4), (3, 6)] {
            let base = nodes[node];
            let up = idx < 2;
            let p = base + vec2(0., if up { -28. } else { 28. });
            let green = self.signals[idx];
            let lamp = if green { GREEN_LAMP } else { RED_LAMP };
            line(p, base, 2.5, Color::new(0.80, 0.78, 0.68, 1.));
            let selected = CONTROL_SIGNAL[self.selected] == Some(idx);
            if selected {
                shapes::stroke_circle(p, 14.5, 2., with_alpha(CREAM, 0.85));
            }
            halo(
                p,
                if green { 18. } else { 13. },
                lamp,
                if green { 0.5 } else { 0.3 },
            );
            draw_circle(p.x, p.y, 9.5, Color::new(0.05, 0.06, 0.07, 1.));
            shapes::fill_circle(
                p,
                6.,
                Fill::radial(p - vec2(1.5, 1.5), 7., lighten(lamp, 0.45), lamp),
            );
        }
        for (point, node, control) in [(0, 2, 1), (1, 5, 4)] {
            let p = nodes[node];
            let selected = self.selected == control;
            draw_circle(p.x, p.y + 2.5, 12., Color::new(0., 0., 0., 0.35));
            shapes::fill_circle(p, 12., if selected { CREAM } else { BRASS });
            shapes::fill_circle(
                p,
                10.,
                Fill::radial(
                    p - vec2(3., 3.),
                    13.,
                    Color::new(0.34, 0.27, 0.20, 1.),
                    WOOD,
                ),
            );
            if selected {
                shapes::stroke_circle(p, 15.5, 1.5, with_alpha(CREAM, 0.6));
            }
            let direction = match self.points[point] {
                1 => vec2(0., -1.),
                2 => vec2(0., 1.),
                _ => Vec2::X,
            };
            shapes::fill_capsule(p - direction * 5., p + direction * 6., 2., BRASS);
        }
        let bridge_y = self.schedule().bridge_y;
        if self.bridge_closed() {
            tag(
                "Bridge closed",
                400.,
                bridge_y - 48.,
                12.,
                RED_LAMP,
                Color::new(0.16, 0.06, 0.05, 0.9),
            );
        } else if let Some((start, end)) = self.schedule().closure {
            let value = if self.time < start * 60 {
                format!("Inspection {start}–{end}s")
            } else {
                "Bridge open".into()
            };
            tag(
                &value,
                400.,
                bridge_y - 48.,
                12.,
                BRASS,
                Color::new(0.10, 0.08, 0.04, 0.88),
            );
        } else {
            tag(
                "Single track",
                400.,
                bridge_y - 48.,
                12.,
                CREAM,
                Color::new(0.04, 0.06, 0.07, 0.8),
            );
        }
    }

    fn locomotives(&self) {
        for (index, plan) in self.schedule().trains.iter().enumerate() {
            let Some(position) = self.position(index) else {
                continue;
            };
            let direction = self.direction(index);
            let across = direction.perp();
            let color = DESTINATIONS[plan.destination];
            let rear = position - direction * 12.;
            let front = position + direction * 12.;
            shapes::fill_capsule(
                rear + vec2(2., 4.),
                front + vec2(2., 4.),
                9.,
                Color::new(0., 0., 0., 0.32),
            );
            shapes::fill_capsule(rear, front, 8.5, Color::new(0.06, 0.07, 0.08, 1.));
            shapes::fill_capsule(
                rear,
                front,
                6.5,
                Fill::linear(
                    position - across * 6.,
                    position + across * 6.,
                    lighten(color, 0.25),
                    darken(color, 0.2),
                ),
            );
            // Cab roof and window.
            let cab = position - direction * 7.;
            shapes::fill_capsule(
                cab - direction * 3.,
                cab + direction * 3.,
                5.,
                Color::new(0.88, 0.87, 0.78, 1.),
            );
            shapes::fill_capsule(
                cab - direction * 1.,
                cab + direction * 1.,
                2.6,
                Color::new(0.17, 0.32, 0.37, 1.),
            );
            draw_circle(
                (position + direction * 5.).x,
                (position + direction * 5.).y,
                3.2,
                WOOD,
            );
            let lamp = position + direction * 14.;
            draw_circle(lamp.x, lamp.y, 2.2, Color::new(1., 0.93, 0.7, 1.));
            if matches!(self.trains[index].state, TrainState::Moving { .. }) {
                for puff in 0..2_u32 {
                    let drift = ((self.time / 6 + index as u32 * 3 + puff * 4) % 9) as f32;
                    let smoke = position + vec2(-3. - drift * 0.8, -12. - drift * 1.2);
                    draw_circle(
                        smoke.x,
                        smoke.y,
                        2.5 + drift * 0.45,
                        Color::new(0.92, 0.90, 0.82, 0.32 - drift * 0.03),
                    );
                }
            }
            let tag_text = format!("{} → {}", plan.name, letter(plan.destination));
            tag(
                &tag_text,
                position.x,
                position.y - 26.,
                11.,
                color,
                Color::new(0.04, 0.05, 0.06, 0.9),
            );
        }
    }

    fn timetable(&self) {
        let (x, y, w, h) = (734., 120., 206., 385.);
        card(x, y, w, h, BOARD);
        let left = x + 14.;
        let right = x + w - 14.;
        eyebrow("TIMETABLE", left, y + 22., BRASS);
        label(
            &format!("{:02}:{:02}", self.time / 3600, (self.time / 60) % 60),
            left,
            y + 50.,
            24.,
            CREAM,
        );
        draw_text_styled(
            "CLOSES",
            right,
            y + 22.,
            eyebrow_style(MUTED).with_align(TextAlign::Right),
        );
        label_r(
            &format!("{}s", self.schedule().closing),
            right,
            y + 50.,
            16.,
            CREAM,
        );
        let elapsed = self.time as f32 / (self.schedule().closing * 60) as f32;
        meter(left, y + 60., w - 28., 4., elapsed, BRASS);
        let columns = [left + 2., left + 52., left + 76., left + 116.];
        for (column, name) in columns.iter().zip(["TRAIN", "TO", "DUE", "STATUS"]) {
            draw_text_styled(
                name,
                *column,
                y + 84.,
                ink(10., MUTED).with_letter_spacing(0.8),
            );
        }
        let trains = self.schedule().trains;
        let row_h = if trains.len() > 10 { 19. } else { 21. };
        for (index, plan) in trains.iter().enumerate() {
            let ry = y + 92. + index as f32 * row_h;
            let base = ry + row_h * 0.5 + 4.;
            let color = DESTINATIONS[plan.destination];
            let state = self.trains[index].state;
            let done = state == TrainState::Delivered;
            let live = matches!(state, TrainState::Moving { .. } | TrainState::At(_));
            if live {
                rounded(
                    x + 6.,
                    ry + 1.,
                    w - 12.,
                    row_h - 2.,
                    5.,
                    with_alpha(color, 0.12),
                );
            } else if index % 2 == 0 {
                panel(
                    x + 8.,
                    ry + 1.,
                    w - 16.,
                    row_h - 2.,
                    Color::new(1., 1., 1., 0.025),
                );
            }
            let ink = if done { MUTED } else { CREAM };
            draw_circle(columns[0] + 2., base - 4., 3., color);
            label(plan.name, columns[0] + 10., base, 12., ink);
            label(
                &letter(plan.destination).to_string(),
                columns[1],
                base,
                12.,
                color,
            );
            let late = self.time > plan.due * 60 && !done;
            label(
                &format!("{}s", plan.due),
                columns[2],
                base,
                12.,
                if late { RED_LAMP } else { MUTED },
            );
            let (status, tone) = match state {
                TrainState::Waiting => {
                    if self.time >= plan.release * 60 {
                        ("Queued".to_owned(), BRASS)
                    } else {
                        (
                            format!("in {}s", (plan.release * 60 - self.time).div_ceil(60)),
                            MUTED,
                        )
                    }
                }
                TrainState::At(_) => ("Holding".to_owned(), BRASS),
                TrainState::Moving { .. } => ("Running".to_owned(), CREAM),
                TrainState::Delivered => ("✓ Home".to_owned(), GREEN_LAMP),
            };
            label(&status, columns[3], base, 11., tone);
        }
        let rows_end = y + 92. + trains.len() as f32 * row_h;
        let legend = y + h - 160.;
        if rows_end + 14. < legend {
            eyebrow("PLATFORMS", left, legend, MUTED);
            for (d, name) in STATIONS.iter().enumerate() {
                let ly = legend + 22. + d as f32 * 21.;
                let color = DESTINATIONS[d];
                let disc = vec2(left + 7., ly - 4.5);
                shapes::fill_circle(disc, 7., color);
                let ds = ink(10., INK).with_align(TextAlign::Center);
                draw_text_styled(
                    &letter(d).to_string(),
                    disc.x + 0.5,
                    ds.centered_baseline(disc.y),
                    ds,
                );
                label(name, left + 20., ly, 12., SOFT);
                let (due, home) = trains
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.destination == d)
                    .fold((0, 0), |(due, home), (i, _)| {
                        (
                            due + 1,
                            home + usize::from(self.trains[i].state == TrainState::Delivered),
                        )
                    });
                label_r(
                    &format!("{home}/{due}"),
                    right,
                    ly,
                    12.,
                    if due > 0 && home == due {
                        GREEN_LAMP
                    } else {
                        MUTED
                    },
                );
            }
        }
        divider(left, y + h - 58., w - 28., Color::new(1., 1., 1., 0.12));
        let next = trains
            .iter()
            .enumerate()
            .find(|(i, _)| self.trains[*i].state != TrainState::Delivered);
        if let Some((_, plan)) = next {
            eyebrow("NEXT UP", left, y + h - 40., MUTED);
            let color = DESTINATIONS[plan.destination];
            let w1 = label(plan.name, left, y + h - 20., 15., CREAM);
            label(
                &format!("· {}", plan.service.name()),
                left + w1 + 6.,
                y + h - 20.,
                13.,
                SOFT,
            );
            label_r(
                &format!(
                    "{} → {}",
                    if plan.side == Side::West {
                        "West"
                    } else {
                        "East"
                    },
                    letter(plan.destination)
                ),
                right,
                y + h - 20.,
                13.,
                color,
            );
        } else {
            eyebrow("ALL CLEAR", left, y + h - 40., GREEN_LAMP);
            label("Every service home", left, y + h - 20., 13., CREAM);
        }
    }

    fn switchboard(&self) {
        let base = Rect::new(20., 513., 704., 55.);
        shapes::fill_rounded_rect(
            base,
            10.,
            Fill::vertical(base, Color::new(0.24, 0.19, 0.16, 1.), WOOD),
        );
        panel(30., 513.5, 684., 1., Color::new(1., 0.9, 0.7, 0.1));
        for (control, name) in CONTROL_NAMES.iter().enumerate() {
            let at = Self::control_center(control);
            let selected = self.selected == control;
            let r = Rect::new(at.x - 51., 518., 102., 45.);
            let (top, bottom) = if selected {
                (
                    Color::new(0.38, 0.32, 0.22, 1.),
                    Color::new(0.27, 0.22, 0.15, 1.),
                )
            } else {
                (
                    Color::new(0.17, 0.17, 0.16, 1.),
                    Color::new(0.12, 0.12, 0.11, 1.),
                )
            };
            panel(
                r.x + 4.,
                r.y + r.h - 3.,
                r.w - 8.,
                5.,
                Color::new(0., 0., 0., 0.45),
            );
            shapes::fill_rounded_rect(r, 7., Fill::vertical(r, top, bottom));
            if selected {
                shapes::stroke_rounded_rect(r, 7., 2., BRASS);
            } else {
                panel(
                    r.x + 7.,
                    r.y + 1.,
                    r.w - 14.,
                    1.,
                    Color::new(1., 0.92, 0.75, 0.1),
                );
            }
            label(
                name,
                r.x + 9.,
                533.,
                11.,
                if selected { CREAM } else { SOFT },
            );
            let (value, color) = match CONTROL_SIGNAL[control] {
                None => (
                    ROUTE_NAMES[self.points[if control == 1 { 0 } else { 1 }]],
                    BRASS,
                ),
                Some(signal) if self.signals[signal] => ("Green", GREEN_LAMP),
                Some(_) => ("Red", RED_LAMP),
            };
            let dot = vec2(r.x + 14., 551.);
            if CONTROL_SIGNAL[control].is_some() {
                shapes::fill_circle(dot, 4., color);
            } else {
                shapes::stroke_circle(dot, 4., 1.5, color);
            }
            label(value, r.x + 24., 556., 14., color);
        }
        let live = self.dispatching;
        let r = Rect::new(734., 513., 206., 55.);
        let tint = if live {
            Color::new(0.11, 0.24, 0.20, 1.)
        } else {
            Color::new(0.30, 0.25, 0.15, 1.)
        };
        shapes::fill_rounded_rect(r, 10., Fill::vertical(r, lighten(tint, 0.06), tint));
        shapes::stroke_rounded_rect(
            r,
            10.,
            1.,
            with_alpha(if live { GREEN_LAMP } else { BRASS }, 0.35),
        );
        keycap("X", "", 748., 529., CREAM);
        label(
            if live { "Pause to plan" } else { "Run railway" },
            778.,
            536.,
            15.,
            CREAM,
        );
        let pulse = if live {
            0.55 + 0.45 * (((self.time / 3) % 40) as f32 / 40. * std::f32::consts::TAU).cos()
        } else {
            1.
        };
        let dot = vec2(783., 551.);
        let lamp = if live { GREEN_LAMP } else { BRASS };
        shapes::fill_circle(dot, 3.5, with_alpha(lamp, pulse));
        label(
            if live {
                "Clock running"
            } else {
                "Clock stopped"
            },
            791.,
            555.,
            11.,
            if live { GREEN_LAMP } else { BRASS },
        );
        // Dispatcher notices sit on the map's lower edge.
        let notice = if self.message_ticks > 0 {
            self.message.as_str()
        } else {
            "Select a switch or signal to operate it."
        };
        let bar = Rect::new(26., 481., 692., 24.);
        shapes::fill_rounded_rect(bar, 12., Color::new(0.03, 0.05, 0.06, 0.82));
        let fresh = self.message_ticks > 0;
        shapes::fill_circle(vec2(40., 493.), 3.5, if fresh { BRASS } else { MUTED });
        label_fit(
            notice,
            51.,
            497.,
            659.,
            12.,
            if fresh { CREAM } else { SOFT },
        );
    }

    fn briefing(&self) {
        let (x, y, w, h) = (74., 160., 625., 336.);
        rounded(
            20.,
            120.,
            704.,
            389.,
            12.,
            Color::new(0.01, 0.02, 0.03, 0.45),
        );
        card(x, y, w, h, BOARD);
        rounded(x, y, w, 4., 2., BRASS);
        let left = x + 24.;
        eyebrow(
            &format!("SHIFT {:02} OF {:02}", self.shift + 1, SHIFTS.len()),
            left,
            y + 32.,
            BRASS,
        );
        label(self.schedule().name, left, y + 64., 26., CREAM);
        let next = paragraph(self.schedule().story, left, y + 100., w - 48., 16., PAPER);
        // Tip callout with a brass rule.
        let tip_top = next + 2.;
        let tip_style = ink(14., BRASS);
        let lines = wrap_text(self.schedule().tip, w - 72., tip_style)
            .count()
            .max(1);
        let tip_h = 20. + lines as f32 * 18.;
        rounded(left, tip_top, w - 48., tip_h, 8., with_alpha(BRASS, 0.08));
        rounded(left, tip_top, 3., tip_h, 1.5, BRASS);
        paragraph(
            self.schedule().tip,
            left + 16.,
            tip_top + 24.,
            w - 72.,
            14.,
            BRASS,
        );
        // Shift facts at a glance.
        let schedule = self.schedule();
        let mut cx = left;
        let chip_y = tip_top + tip_h + 14.;
        cx += chip(
            &format!("{} trains", schedule.trains.len()),
            cx,
            chip_y,
            12.,
            CREAM,
        ) + 8.;
        cx += chip(
            &format!("Bell at {}s", schedule.closing),
            cx,
            chip_y,
            12.,
            CREAM,
        ) + 8.;
        cx += match schedule.closure {
            Some((start, end)) => chip(
                &format!("Inspection {start}–{end}s"),
                cx,
                chip_y,
                12.,
                RED_LAMP,
            ),
            None => chip("Bridge open all shift", cx, chip_y, 12., GREEN_LAMP),
        } + 8.;
        if schedule.speed < 1. {
            chip(
                &format!("Speed {}%", (schedule.speed * 100.).round()),
                cx,
                chip_y,
                12.,
                BRASS,
            );
        }
        let mut hint_y = chip_y + 46.;
        if hint_y < 404. {
            label(
                "Points cycle Straight → North → South. A green signal admits one train.",
                left,
                hint_y,
                13.,
                SOFT,
            );
            hint_y += 18.;
        }
        if hint_y < 412. {
            let mut kx = left;
            kx += keycap("X", "Run / pause", kx, hint_y, SOFT) + 20.;
            kx += keycap("C", "Retry shift", kx, hint_y, SOFT) + 20.;
            keycap("1–5", "Pick control", kx, hint_y, SOFT);
        }
        action_button(
            99.,
            444.,
            578.,
            40.,
            "Space",
            "Start shift",
            Color::new(0.20, 0.36, 0.29, 1.),
        );
    }

    fn results(&self, cleared: bool) {
        let (x, y, w, h) = (74., 166., 625., 330.);
        rounded(
            20.,
            120.,
            704.,
            389.,
            12.,
            Color::new(0.01, 0.02, 0.03, 0.45),
        );
        card(x, y, w, h, BOARD);
        rounded(x, y, w, 4., 2., if cleared { BRASS } else { RED_LAMP });
        let left = x + 24.;
        if cleared {
            eyebrow(
                &format!("SHIFT {:02} COMPLETE", self.shift + 1),
                left,
                y + 32.,
                BRASS,
            );
            label("All trains home", left, y + 64., 27., CREAM);
            let stars = self.medals[self.shift];
            for i in 0..3 {
                let c = vec2(left + 18. + i as f32 * 42., y + 104.);
                star(c + vec2(1.5, 2.5), 17., Color::new(0., 0., 0., 0.35));
                if i < u32::from(stars) {
                    halo(c, 26., BRASS, 0.22);
                    star(c, 17., BRASS);
                    star(c - vec2(0., 1.), 9., lighten(BRASS, 0.35));
                } else {
                    star(c, 17., Color::new(0.24, 0.27, 0.28, 1.));
                }
            }
            let (medal, note) = match stars {
                3 => ("Gold", "Every service on time"),
                2 => ("Silver", "A reliable shift"),
                _ => ("Bronze", "Safe delivery"),
            };
            label(medal, left + 150., y + 100., 18., BRASS);
            label(note, left + 150., y + 119., 13., SOFT);
            divider(left, y + 146., w - 48., Color::new(1., 1., 1., 0.12));
            let stats = [
                ("DELIVERED", self.schedule().trains.len().to_string()),
                ("DELAY", format!("{}s", self.delays.div_ceil(60))),
                (
                    "EARNED",
                    (self.provisional + 300 + u32::from(stars) * 150).to_string(),
                ),
                ("CAREER", self.score.to_string()),
            ];
            let col = (w - 48.) / 4.;
            for (i, (name, value)) in stats.iter().enumerate() {
                let sx = left + i as f32 * col;
                eyebrow(name, sx, y + 176., MUTED);
                label(value, sx, y + 204., 22., CREAM);
            }
            label(
                "Shift complete. The next timetable is waiting.",
                left,
                y + 246.,
                14.,
                SOFT,
            );
            action_button(
                99.,
                444.,
                578.,
                40.,
                "Space",
                "Next timetable",
                Color::new(0.20, 0.36, 0.29, 1.),
            );
        } else {
            eyebrow("SAFETY INCIDENT", left, y + 32., RED_LAMP);
            label("The line is stopped", left, y + 64., 27., CREAM);
            let next = paragraph(&self.message, left, y + 98., w - 48., 17., PAPER);
            let chip_y = next + 6.;
            let cw = chip(
                &format!(
                    "{} {} left",
                    self.retries,
                    if self.retries == 1 {
                        "retry"
                    } else {
                        "retries"
                    }
                ),
                left,
                chip_y,
                13.,
                BRASS,
            );
            label(
                "Retrying resets this shift's earnings.",
                left + cw + 12.,
                chip_y + 14.,
                13.,
                SOFT,
            );
            const TIP: &str =
                "Press X to stop the clock, read both points, then clear one signal at a time.";
            let lines = wrap_text(TIP, w - 80., ink(14., SOFT)).count() as f32;
            let tip_top = chip_y + 38.;
            let tip_h = 38. + lines * 18.;
            if tip_top + tip_h < 432. {
                rounded(
                    left,
                    tip_top,
                    w - 48.,
                    tip_h,
                    8.,
                    Color::new(1., 1., 1., 0.04),
                );
                eyebrow("DISPATCHER'S TIP", left + 16., tip_top + 21., BRASS);
                paragraph(TIP, left + 16., tip_top + 42., w - 80., 14., SOFT);
            }
            action_button(
                99.,
                444.,
                578.,
                40.,
                "C",
                "Use one retry and reopen this shift",
                Color::new(0.38, 0.17, 0.15, 1.),
            );
        }
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
        self.landscape();
        self.railway();
        self.locomotives();
        self.timetable();
        self.switchboard();
        match self.phase {
            Phase::Briefing => self.briefing(),
            Phase::Cleared | Phase::Incident => self.results(self.phase == Phase::Cleared),
            _ => {}
        }
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!(
                "Shift {}/{} · {}",
                self.shift + 1,
                SHIFTS.len(),
                self.schedule().name
            ),
            objective: format!(
                "Route {} trains. {} delivered.",
                self.schedule().trains.len(),
                self.delivered()
            ),
            status: format!(
                "{} retries | {} stars | {} provisional | {}",
                self.retries,
                self.medals.iter().map(|s| u32::from(*s)).sum::<u32>(),
                self.provisional,
                if self.dispatching {
                    "Railway live"
                } else {
                    "Railway held"
                }
            ),
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        self.outcome()
    }
    fn summary(&self) -> String {
        if self.outcome() == Outcome::Won {
            format!(
                "The railway board awards you the brass key. {}: all 61 services delivered across eight districts. {} of 24 punctuality stars, {} commission, {} safety incidents. Start a new career to chase a perfect timetable.",
                self.rank(),
                self.medals.iter().map(|m| u32::from(*m)).sum::<u32>(),
                self.score,
                self.incidents
            )
        } else {
            format!(
                "Your safety certificate was withdrawn on shift {}. {} {} completed services and {} commission remain in your career ledger. Start a new career; use planning mode and read both points before opening a signal.",
                self.shift + 1,
                self.message,
                self.delivered_total,
                self.score
            )
        }
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.bot_input(tick)
    }
}
