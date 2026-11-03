//! Read-only presentation: a living sea chart, ships, shot arcs and the gun deck.
use crate::{
    Game, Phase,
    model::{Ammo, Ship, Trace},
};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;

const GOLD: Color = Color::new(0.97, 0.76, 0.37, 1.);
const CREAM: Color = Color::new(0.93, 0.88, 0.72, 1.);
const TEAL: Color = Color::new(0.37, 0.82, 0.75, 1.);
const RED: Color = Color::new(0.96, 0.46, 0.35, 1.);
const STEEL: Color = Color::new(0.62, 0.70, 0.78, 1.);
const SOFT: Color = Color::new(0.76, 0.80, 0.82, 1.);
const DARK: Color = Color::new(0.055, 0.09, 0.12, 1.);
const SKY_TOP: Color = Color::new(0.04, 0.07, 0.13, 1.);
const SKY_LOW: Color = Color::new(0.16, 0.24, 0.31, 1.);
const SEA_TOP: Color = Color::new(0.11, 0.33, 0.38, 1.);
const SEA_LOW: Color = Color::new(0.02, 0.11, 0.16, 1.);
const WOOD: Color = Color::new(0.45, 0.32, 0.21, 1.);
const PARTS: [&str; 4] = ["Stern", "Hold", "Bow", "Mast"];
const AMMO: [&str; 3] = ["Round", "Chain", "Shell"];
/// The waterline in canvas units.
const WATER: f32 = 429.;

fn screen(p: Vec2) -> Vec2 {
    vec2(30. + p.x * 0.79, 432. - p.y * 0.56)
}
fn in_sea(p: Vec2) -> bool {
    (23. ..=668.).contains(&p.x) && (117. ..=458.).contains(&p.y)
}

// ---------------------------------------------------------------------------
// Typography and surfaces.

fn plain(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
fn say(value: &str, x: f32, y: f32, s: TextStyle) -> f32 {
    draw_text_styled(value, x, y, s).width
}
fn say_fit(value: &str, x: f32, y: f32, max: f32, s: TextStyle) -> f32 {
    let w = text_width(value, s);
    let s = if w > max && w > 0. {
        s.with_size(s.size * max / w)
    } else {
        s
    };
    draw_text_styled(value, x, y, s).width
}
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    say(value, x, y, plain(11., color).with_letter_spacing(1.5))
}
fn para(value: &str, x: f32, y: f32, max: f32, s: TextStyle, line: f32) -> f32 {
    let mut baseline = y;
    for row in wrap_text(value, max, s) {
        draw_text_styled(row, x, baseline, s);
        baseline += line;
    }
    baseline
}
/// Raised panel: offset shadow, hairline rim and a softly lit gradient.
fn slab(x: f32, y: f32, w: f32, h: f32, fill: Color) {
    rounded(x, y + 4., w, h, 11., Color::new(0., 0., 0., 0.32 * fill.a));
    rounded_gradient(
        x,
        y,
        w,
        h,
        11.,
        Color::new(1., 1., 1., 0.12 * fill.a),
        Color::new(1., 1., 1., 0.04 * fill.a),
    );
    rounded_gradient(
        x + 1.,
        y + 1.,
        w - 2.,
        h - 2.,
        10.,
        lighten(fill, 0.035),
        darken(fill, 0.04),
    );
}
/// Rimmed tile: gold when selected, a quiet hairline otherwise.
fn tile(x: f32, y: f32, w: f32, h: f32, selected: bool, dim: bool) {
    let a = if dim { 0.55 } else { 1. };
    if selected {
        rounded_gradient(x, y, w, h, 8., lighten(GOLD, 0.2), darken(GOLD, 0.3));
        rounded_gradient(
            x + 1.5,
            y + 1.5,
            w - 3.,
            h - 3.,
            6.5,
            Color::new(0.20, 0.22, 0.20, 1.),
            Color::new(0.12, 0.14, 0.14, 1.),
        );
    } else {
        rounded(x, y, w, h, 8., Color::new(1., 1., 1., 0.08 * a));
        rounded_gradient(
            x + 1.,
            y + 1.,
            w - 2.,
            h - 2.,
            7.,
            Color::new(0.11, 0.16, 0.20, a),
            Color::new(0.08, 0.12, 0.15, a),
        );
    }
}
/// Small tinted pill; returns its width.
fn pill(label: &str, x: f32, y: f32, color: Color) -> f32 {
    let s = plain(12., color);
    let w = text_width(label, s) + 14.;
    rounded(x, y, w, 19., 9.5, with_alpha(color, 0.16));
    draw_text_styled(label, x + 7., s.centered_baseline(y + 9.5), s);
    w
}
fn pill_width(label: &str) -> f32 {
    text_width(label, plain(12., CREAM)) + 14.
}
fn button(x: f32, y: f32, w: f32, h: f32, key: &str, label: &str, primary: bool) {
    rounded(x, y + 3., w, h, 10., Color::new(0., 0., 0., 0.35));
    if primary {
        rounded_gradient(x, y, w, h, 10., lighten(GOLD, 0.25), darken(GOLD, 0.15));
    } else {
        rounded_gradient(
            x,
            y,
            w,
            h,
            10.,
            Color::new(0.20, 0.28, 0.34, 1.),
            Color::new(0.12, 0.18, 0.23, 1.),
        );
    }
    let ink = if primary { INK } else { CREAM };
    let st = plain(13., ink);
    let cap = (text_width(key, st) + 12.).max(20.);
    let total = cap + 7. + text_width(label, st);
    keycap(key, label, x + (w - total) * 0.5, y + (h - 20.) * 0.5, ink);
}
/// A thin bar for dense instrument rows (cheaper than a full meter).
fn bar(x: f32, y: f32, w: f32, fraction: f32, color: Color) {
    let f = if fraction.is_finite() {
        fraction.clamp(0., 1.)
    } else {
        0.
    };
    draw_rectangle(x, y, w, 4., Color::new(0., 0., 0., 0.45));
    if f > 0. {
        draw_rectangle(x, y, w * f, 4., color);
        draw_rectangle(x, y, w * f, 1., with_alpha(lighten(color, 0.4), 0.7));
    }
}
/// Soft alpha-blended light pool (stays in the main batch, unlike additive glows).
fn halo(x: f32, y: f32, r: f32, color: Color) {
    let c = vec2(x, y);
    shapes::fill_circle(c, r, Fill::radial(c, r, color, with_alpha(color, 0.)));
}
fn fract(v: f32) -> f32 {
    v - v.floor()
}
/// Gradient colour of the sky at canvas height `y`.
fn sky_at(y: f32) -> Color {
    shapes::mix(SKY_TOP, SKY_LOW, ((y - 116.) / 344.).clamp(0., 1.))
}
fn sea_at(y: f32) -> Color {
    shapes::mix(SEA_TOP, SEA_LOW, ((y - WATER) / 31.).clamp(0., 1.))
}
fn ammo_icon(ammo: Ammo, x: f32, y: f32, r: f32, a: f32) {
    let iron = Color::new(0.16, 0.17, 0.19, a);
    let shine = with_alpha(CREAM, 0.8 * a);
    match ammo {
        Ammo::Round => {
            draw_circle(x, y, r, iron);
            draw_circle(x, y, r * 0.75, Color::new(0.30, 0.31, 0.33, a));
            draw_circle(x - r * 0.35, y - r * 0.35, r * 0.28, shine);
        }
        Ammo::Chain => {
            draw_line(
                x - r,
                y + r * 0.4,
                x + r,
                y - r * 0.4,
                1.5,
                with_alpha(STEEL, a),
            );
            for d in [-1., 1.] {
                draw_circle(
                    x + d * r,
                    y - d * r * 0.4,
                    r * 0.6,
                    Color::new(0.30, 0.31, 0.33, a),
                );
                draw_circle(x + d * r - 1., y - d * r * 0.4 - 1., r * 0.2, shine);
            }
        }
        Ammo::Shell => {
            draw_circle(x, y + 1., r, Color::new(0.30, 0.10, 0.08, a));
            draw_circle(x, y + 1., r * 0.7, Color::new(0.55, 0.18, 0.12, a));
            draw_line(
                x + r * 0.4,
                y - r * 0.6,
                x + r,
                y - r * 1.3,
                1.5,
                with_alpha(CREAM, a),
            );
            draw_circle(x + r, y - r * 1.3, 1.6, Color::new(1., 0.75, 0.3, a));
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
        self.render();
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!(
                "Island {} of 8 · {}",
                self.battle_number(),
                self.battle_info().island
            ),
            objective: "Sink the enemy ship.".into(),
            status: format!(
                "{} harbors freed | {} salvage | {} lifeboats",
                self.wins, self.credits, self.lifeboats
            ),
            score: self.score,
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

impl Game {
    fn render(&self) {
        self.draw_sea();
        self.draw_instruments();
        self.draw_log();
        if matches!(
            self.phase,
            Phase::Briefing | Phase::Dock | Phase::Setback | Phase::Liberated | Phase::Sunk
        ) {
            self.draw_overlay();
        }
    }
    fn overlay_open(&self) -> bool {
        matches!(
            self.phase,
            Phase::Briefing | Phase::Dock | Phase::Setback | Phase::Liberated | Phase::Sunk
        )
    }
    fn time(&self) -> f32 {
        (self.ticks % 1_000_000) as f32
    }
    fn enemy_color(&self) -> Color {
        let [r, g, b] = self.battle_info().color;
        Color::new(r, g, b, 1.)
    }
    fn draw_sea(&self) {
        let t = self.time();
        // Night sky with a warm haze along the horizon.
        rounded_gradient(20., 116., 650., 344., 12., SKY_TOP, SKY_LOW);
        shapes::fill_rounded_rect(
            Rect::new(20., 330., 650., 99.),
            0.,
            Fill::vertical(
                Rect::new(20., 330., 650., 99.),
                Color::new(0.55, 0.40, 0.30, 0.),
                Color::new(0.55, 0.40, 0.30, 0.16),
            ),
        );
        for i in 0..24_u32 {
            let x = 36. + ((i * 211 + 37) % 610) as f32;
            let y = 176. + ((i * 97 + 11) % 150) as f32;
            let a = 0.18 + 0.22 * (0.5 + 0.5 * (t * 0.035 + i as f32 * 1.7).sin());
            let s = if i % 5 == 0 { 2. } else { 1.3 };
            draw_rectangle(x, y, s, s, Color::new(0.9, 0.92, 1., a));
        }
        // A full moon with soft craters and halo.
        let moon = vec2(598., 214.);
        halo(moon.x, moon.y, 60., Color::new(0.95, 0.85, 0.55, 0.2));
        shapes::fill_circle(
            moon,
            19.,
            Fill::radial(
                moon - vec2(6., 6.),
                28.,
                Color::new(0.98, 0.95, 0.82, 1.),
                Color::new(0.78, 0.72, 0.55, 1.),
            ),
        );
        draw_circle(
            moon.x + 6.,
            moon.y + 4.,
            4.,
            Color::new(0.70, 0.64, 0.48, 0.45),
        );
        draw_circle(
            moon.x - 5.,
            moon.y + 8.,
            2.5,
            Color::new(0.70, 0.64, 0.48, 0.4),
        );
        self.draw_weather(t);
        self.draw_islands();
        self.draw_water(t, moon.x);
        if !self.overlay_open() {
            self.draw_chart_labels();
        }
        // Shot history and the gunner's forecast.
        trajectory(&self.history, Color::new(0.85, 0.72, 0.42, 0.42), 1.5);
        if self.phase == Phase::Aim {
            let trace = &self.forecast.trace;
            for (i, p) in trace.points[..trace.len].iter().enumerate() {
                let q = screen(*p);
                if i % 2 == 0 && in_sea(q) {
                    let fade = 1. - 0.5 * i as f32 / trace.len.max(1) as f32;
                    draw_poly(
                        q.x,
                        q.y,
                        6,
                        1.6,
                        0.,
                        Color::new(0.62, 0.92, 0.90, 0.85 * fade),
                    );
                }
            }
            let end = screen(self.forecast.end);
            if in_sea(end) {
                let color = match self.forecast.contact {
                    Some((1, _)) => GOLD,
                    Some(_) => RED,
                    None => SOFT,
                };
                let pulse = 0.5 + 0.5 * (t * 0.12).sin();
                shapes::stroke_circle(end, 7., 1.5, color);
                shapes::stroke_circle(
                    end,
                    10. + pulse * 3.,
                    1.,
                    with_alpha(color, 0.45 * (1. - pulse)),
                );
                for (dx, dy) in [(1., 0.), (-1., 0.), (0., 1.), (0., -1.)] {
                    draw_line(
                        end.x + dx * 4.,
                        end.y + dy * 4.,
                        end.x + dx * 10.,
                        end.y + dy * 10.,
                        1.2,
                        color,
                    );
                }
            }
        }
        self.draw_ship(self.ships[0], true, TEAL, t);
        self.draw_ship(self.ships[1], false, self.enemy_color(), t);
        // Live shot: a trail that brightens toward the ball.
        let n = self.trace.len;
        for (i, pair) in self.trace.points[..n].windows(2).enumerate() {
            let a = screen(pair[0]);
            let b = screen(pair[1]);
            if in_sea(a) && in_sea(b) {
                let k = (i + 1) as f32 / n as f32;
                draw_line(
                    a.x,
                    a.y,
                    b.x,
                    b.y,
                    1. + k * 1.5,
                    Color::new(1., 0.90, 0.65, 0.12 + 0.6 * k),
                );
            }
        }
        if let Some(ball) = self.ball {
            let p = screen(ball.pos);
            if in_sea(p) {
                let hot = if ball.ammo == Ammo::Shell {
                    Color::new(1., 0.62, 0.32, 0.5)
                } else {
                    Color::new(1., 0.92, 0.7, 0.3)
                };
                halo(p.x, p.y, 9., hot);
                match ball.ammo {
                    Ammo::Chain => {
                        let spin = t * 0.4;
                        let d = vec2(spin.cos(), spin.sin()) * 4.5;
                        draw_line(p.x - d.x, p.y - d.y, p.x + d.x, p.y + d.y, 1.4, STEEL);
                        draw_circle(p.x - d.x, p.y - d.y, 2.6, Color::new(0.2, 0.2, 0.22, 1.));
                        draw_circle(p.x + d.x, p.y + d.y, 2.6, Color::new(0.2, 0.2, 0.22, 1.));
                    }
                    _ => {
                        let r = if ball.ammo == Ammo::Shell { 4.6 } else { 3.8 };
                        draw_circle(p.x, p.y, r, Color::new(0.14, 0.14, 0.16, 1.));
                        draw_circle(p.x - r * 0.35, p.y - r * 0.35, r * 0.35, CREAM);
                    }
                }
            }
        }
        self.draw_impact();
    }
    fn draw_weather(&self, t: f32) {
        // Clouds drift downwind; calm air still breathes slowly east.
        let drift = if self.wind == 0. {
            0.04
        } else {
            self.wind * 0.012
        };
        for (i, (bx, by, w)) in [(120., 214., 74.), (330., 246., 96.), (520., 292., 64.)]
            .into_iter()
            .enumerate()
        {
            let span = 520.;
            let x = 80. + fract((bx - 80. + t * drift * (1. + i as f32 * 0.25)) / span) * span;
            let edge = ((x - 80.) / 40.).min((600. - x) / 40.).clamp(0., 1.);
            let c = Color::new(0.62, 0.70, 0.80, 0.13 * edge);
            for (dx, dy, len, r) in [
                (0., 0., 1., 3.5),
                (-0.2, -7., 0.6, 3.),
                (0.25, 7., 0.45, 2.5),
            ] {
                let cx = x + dx * w;
                shapes::fill_capsule(
                    vec2(cx - w * len * 0.5, by + dy),
                    vec2(cx + w * len * 0.5, by + dy),
                    r,
                    c,
                );
            }
        }
        if self.wind != 0. {
            let len = 14. + self.wind.abs() * 1.4;
            for i in 0..7 {
                let fi = i as f32;
                let y = 200. + ((i * 41) % 180) as f32;
                let span = 590. - len;
                let x =
                    40. + fract((fi * 0.37 + t * self.wind * 0.0009 * (1. + fi * 0.1)) * 1.) * span;
                draw_line(x, y, x + len, y, 1., Color::new(0.8, 0.88, 0.95, 0.11));
            }
        }
    }
    fn draw_islands(&self) {
        // Far, hazy range, then the nearer islands with a lit flank.
        for n in 0..5 {
            let x = 150. + n as f32 * 112.;
            let peak = 390. - ((self.battle * 13 + n * 29) % 34) as f32;
            draw_triangle(
                vec2(x - 70., WATER + 1.),
                vec2(x, peak),
                vec2(x + 70., WATER + 1.),
                Color::new(0.12, 0.19, 0.25, 0.9),
            );
        }
        for n in 0..4 {
            let x = 235. + n as f32 * 105.;
            let peak = 367. - ((self.battle * 17 + n * 23) % 63) as f32;
            let base = WATER + 1.;
            draw_triangle(
                vec2(x - 55., base),
                vec2(x, peak),
                vec2(x + 58., base),
                Color::new(0.09, 0.21, 0.24, 1.),
            );
            draw_triangle(
                vec2(x - 55., base),
                vec2(x, peak),
                vec2(x - 6., base),
                Color::new(0.13, 0.28, 0.30, 1.),
            );
            draw_triangle(
                vec2(x - 15., peak + 19.),
                vec2(x, peak),
                vec2(x + 17., peak + 21.),
                Color::new(0.30, 0.42, 0.40, 1.),
            );
        }
    }
    fn draw_water(&self, t: f32, moon_x: f32) {
        let top = Rect::new(20., WATER, 650., 19.);
        shapes::fill_rounded_rect(
            top,
            0.,
            Fill::vertical(top, sea_at(WATER), sea_at(WATER + 19.)),
        );
        rounded_gradient(20., 436., 650., 24., 12., sea_at(436.), sea_at(460.));
        draw_line(
            20.,
            WATER,
            670.,
            WATER,
            1.,
            Color::new(0.55, 0.78, 0.80, 0.35),
        );
        // Moonlit glitter and rolling crests.
        for k in 0..5 {
            let y = WATER + 3. + k as f32 * 4.;
            let w = 22. - k as f32 * 3. + (t * 0.06 + k as f32).sin() * 4.;
            draw_line(
                moon_x - w,
                y,
                moon_x + w,
                y,
                1.,
                Color::new(1., 0.95, 0.75, 0.22 - k as f32 * 0.03),
            );
        }
        for row in 0..3 {
            let alpha = 0.32 - row as f32 * 0.08;
            let y0 = WATER + 6. + row as f32 * 8.;
            for n in 0..14 {
                let span = 620.;
                let x = 24.
                    + fract(
                        (n as f32 * 47. + row as f32 * 19. + t * (0.18 + row as f32 * 0.05)) / span,
                    ) * span;
                let y = y0 + (t * 0.035 + n as f32 * 0.9).sin() * 1.5;
                let len = 10. + ((n * 7 + row * 3) % 9) as f32;
                if x + len < 666. {
                    draw_line(x, y, x + len, y, 1.2, Color::new(0.55, 0.80, 0.82, alpha));
                }
            }
        }
    }
    fn draw_chart_labels(&self) {
        let info = self.battle_info();
        say(info.island, 36., 146., style(19., CREAM));
        say(
            &format!("{} aboard {}", info.captain, info.ship),
            36.,
            165.,
            plain(12., with_alpha(SOFT, 0.85)),
        );
        // Wind gauge and engagement facts.
        let right = 656.;
        let range = format!("Range {}", (self.ships[1].x - self.ships[0].x) as u16);
        let exchange = format!("Exchange {}", self.round);
        let rw = pill_width(&range);
        let ew = pill_width(&exchange);
        pill(&range, right - rw, 128., SOFT);
        pill(&exchange, right - rw - 6. - ew, 128., SOFT);
        let strength = self.wind.abs() as u16;
        let (dir, label) = if self.wind < 0. {
            (-1., format!("{strength} west"))
        } else if self.wind > 0. {
            (1., format!("{strength} east"))
        } else {
            (0., "Calm".to_string())
        };
        let vs = plain(13., GOLD);
        let arrow = if dir == 0. {
            10.
        } else {
            12. + self.wind.abs() * 1.1
        };
        let ew = text_width("WIND", plain(11., MUTED).with_letter_spacing(1.5));
        let w = 12. + ew + 8. + text_width(&label, vs) + 10. + arrow + 12.;
        let x = right - w;
        let y = 154.;
        rounded(x, y, w, 24., 12., Color::new(0.02, 0.05, 0.08, 0.6));
        eyebrow(
            "WIND",
            x + 12.,
            plain(11., MUTED).centered_baseline(y + 12.),
            MUTED,
        );
        let tx = x + 12. + ew + 8.;
        let lw = say(&label, tx, vs.centered_baseline(y + 12.), vs);
        let ax = tx + lw + 10.;
        let cy = y + 12.;
        if dir == 0. {
            shapes::stroke_circle(vec2(ax + 5., cy), 4., 1.5, GOLD);
        } else {
            let (a, b) = if dir > 0. {
                (ax, ax + arrow)
            } else {
                (ax + arrow, ax)
            };
            draw_line(a, cy, b, cy, 2., GOLD);
            draw_triangle(
                vec2(b, cy),
                vec2(b - dir * 6., cy - 4.),
                vec2(b - dir * 6., cy + 4.),
                GOLD,
            );
        }
    }
    fn draw_ship(&self, ship: Ship, player: bool, color: Color, t: f32) {
        let bob = (t * 0.05 + if player { 0. } else { 1.7 }).sin() * 0.8;
        let lift = vec2(0., bob);
        let center = screen(vec2(ship.x, 24.)) + lift;
        let hull_dark = darken(color, 0.55);
        // Each solid hull section matches its collision box; wrecked sections
        // are charred stubs, never invisible intact hitboxes.
        for part in 0..3 {
            let p = screen(ship.section_center(part)) + lift;
            let live = ship.hp[part] > 0;
            let (mut l, mut r) = (p.x - 11.9, p.x + 11.9);
            let (mut bl, mut br) = (l, r);
            if part == 0 {
                l -= 2.;
                bl += 7.;
            }
            if part == 2 {
                r += 3.;
                br -= 7.;
            }
            let (top, mid, bottom) = (p.y - 7.8, p.y - 1.5, p.y + 7.8);
            if live {
                draw_triangle(vec2(l, top), vec2(r, top), vec2(r, mid), color);
                draw_triangle(vec2(l, top), vec2(r, mid), vec2(l, mid), color);
                let ml = l + (bl - l) * 0.35;
                let mr = r + (br - r) * 0.35;
                draw_triangle(vec2(ml, mid), vec2(mr, mid), vec2(br, bottom), hull_dark);
                draw_triangle(vec2(ml, mid), vec2(br, bottom), vec2(bl, bottom), hull_dark);
                draw_line(l, top, r, top, 1.2, with_alpha(lighten(color, 0.5), 0.9));
                draw_rectangle(p.x - 3., p.y - 5., 6., 4., Color::new(0.04, 0.05, 0.06, 1.));
                if ship.armor[part] > 0 {
                    draw_rectangle(ml + 1., p.y + 2., mr - ml - 2., 3., STEEL);
                    draw_rectangle(ml + 1., p.y + 2., mr - ml - 2., 1., lighten(STEEL, 0.4));
                }
            } else {
                let char = Color::new(0.10, 0.08, 0.07, 1.);
                draw_triangle(
                    vec2(bl, p.y + 2.),
                    vec2(br, p.y + 4.),
                    vec2(br, bottom),
                    char,
                );
                draw_triangle(vec2(bl, p.y + 2.), vec2(br, bottom), vec2(bl, bottom), char);
                draw_line(bl + 2., p.y + 2., bl + 6., p.y - 3., 2., darken(color, 0.4));
                draw_line(br - 2., p.y + 3., br - 7., p.y - 2., 2., darken(color, 0.4));
                let ember = 0.5 + 0.5 * (t * 0.2 + part as f32 * 2.).sin();
                draw_rectangle(
                    p.x - 1.5,
                    p.y + 1.,
                    3.,
                    2.,
                    Color::new(1., 0.5, 0.2, 0.4 + 0.5 * ember),
                );
                for k in 0..3 {
                    let ph = fract(t * 0.012 + k as f32 / 3. + part as f32 * 0.37);
                    let q = vec2(
                        p.x + (ph * 6. + k as f32).sin() * 3. + ph * self.wind * 0.8,
                        p.y - 4. - ph * 34.,
                    );
                    if in_sea(q) {
                        draw_poly(
                            q.x,
                            q.y,
                            10,
                            2.5 + ph * 5.,
                            0.,
                            Color::new(0.55, 0.58, 0.62, 0.3 * (1. - ph)),
                        );
                    }
                }
            }
            // Foam at the waterline and a section integrity bar below it.
            draw_line(
                bl - 2.,
                WATER - 1. + bob,
                br + 2.,
                WATER - 1. + bob,
                1.4,
                Color::new(0.85, 0.95, 0.95, 0.3),
            );
            let frac = f32::from(ship.hp[part]) / f32::from(ship.max[part].max(1));
            bar(p.x - 10., 440., 20., frac, if live { color } else { RED });
        }
        let side = if player { 1. } else { -1. };
        // Bowsprit at the leading end and a lantern at the stern.
        let bow_part = if player { 2 } else { 0 };
        if ship.hp[bow_part] > 0 {
            let b = screen(ship.section_center(bow_part)) + lift;
            let bx = b.x + side * 12.;
            draw_line(bx, b.y - 6., bx + side * 13., b.y - 12., 2., WOOD);
        }
        let stern_part = 2 - bow_part;
        if ship.hp[stern_part] > 0 {
            let s = screen(ship.section_center(stern_part)) + lift;
            let lx = s.x - side * 9.;
            let flicker = 0.75 + 0.25 * (t * 0.23 + side).sin();
            draw_line(lx, s.y - 8., lx, s.y - 13., 1., WOOD);
            halo(lx, s.y - 14., 6., Color::new(1., 0.8, 0.4, 0.45 * flicker));
            draw_rectangle(
                lx - 1.5,
                s.y - 16.,
                3.,
                3.,
                Color::new(1., 0.86, 0.5, flicker),
            );
        }
        // Flags stream downwind; in calm air they hang toward the bow.
        let stream = if self.wind > 0. {
            1.
        } else if self.wind < 0. {
            -1.
        } else {
            side
        };
        let base = screen(vec2(ship.x, 38.)) + lift;
        let top = screen(vec2(ship.x, 110.)) + lift;
        if ship.hp[3] > 0 {
            draw_line(base.x, base.y, top.x, top.y - 2., 3.5, WOOD);
            draw_line(
                base.x - 1.,
                base.y,
                top.x - 1.,
                top.y - 2.,
                1.,
                lighten(WOOD, 0.3),
            );
            draw_line(top.x - 17., top.y + 7., top.x + 17., top.y + 7., 2., WOOD);
            draw_line(
                top.x,
                top.y + 2.,
                center.x - 33.,
                center.y - 8.,
                0.8,
                with_alpha(CREAM, 0.35),
            );
            draw_line(
                top.x,
                top.y + 2.,
                center.x + 33.,
                center.y - 8.,
                0.8,
                with_alpha(CREAM, 0.35),
            );
            let billow = (self.wind * 0.3).clamp(-4., 4.) + (t * 0.05).sin() * 0.6;
            let torn = ship.hp[3] * 2 < ship.max[3];
            let drop = if torn { 18. } else { 27. };
            let (a, b) = (vec2(top.x - 15., top.y + 8.), vec2(top.x + 15., top.y + 8.));
            let (c, d) = (
                vec2(top.x + 16. + billow, top.y + 8. + drop),
                vec2(top.x - 14. + billow, top.y + 8. + drop),
            );
            let sail = Color::new(0.94, 0.90, 0.78, 1.);
            draw_triangle(a, b, c, sail);
            draw_triangle(a, c, d, sail);
            let mid_top = vec2(top.x, top.y + 8.);
            let mid_bottom = vec2(top.x + 1. + billow, top.y + 8. + drop);
            let shade = Color::new(0.70, 0.66, 0.56, 1.);
            if billow >= 0. {
                draw_triangle(mid_top, b, c, shade);
                draw_triangle(mid_top, c, mid_bottom, shade);
            } else {
                draw_triangle(a, mid_top, mid_bottom, shade);
                draw_triangle(a, mid_bottom, d, shade);
            }
            draw_line(d.x, d.y, c.x, c.y, 1.5, with_alpha(color, 0.9));
            if torn {
                draw_triangle(
                    vec2(top.x - 4. + billow, d.y),
                    vec2(top.x + 2. + billow, d.y - 7.),
                    vec2(top.x + 6. + billow, d.y),
                    sky_at(d.y),
                );
            }
            let flutter = (t * 0.16 + side).sin() * 2.;
            draw_triangle(
                vec2(top.x, top.y - 3.),
                vec2(top.x + stream * 19., top.y + 0.5 + flutter),
                vec2(top.x, top.y + 5.),
                color,
            );
        } else {
            draw_line(base.x, base.y, base.x, base.y - 9., 3.5, darken(WOOD, 0.3));
            draw_line(
                center.x - 22.,
                center.y - 12.,
                center.x + 20.,
                center.y - 7.,
                2.5,
                darken(WOOD, 0.2),
            );
            draw_triangle(
                vec2(center.x - 12., center.y - 11.),
                vec2(center.x + 4., center.y - 9.),
                vec2(center.x - 6., center.y - 4.),
                Color::new(0.7, 0.66, 0.56, 1.),
            );
        }
        // Cannon on its carriage.
        let start = screen(vec2(ship.x + side * 40., 38.)) + lift;
        let angle = if player {
            f32::from(self.setting.angle)
        } else {
            35.
        }
        .to_radians();
        let tip = vec2(
            start.x + side * 13. * angle.cos(),
            start.y - 13. * angle.sin(),
        );
        draw_rectangle(start.x - 4., start.y - 1., 8., 4., darken(WOOD, 0.2));
        draw_line(
            start.x,
            start.y,
            tip.x,
            tip.y,
            5.,
            Color::new(0.10, 0.11, 0.13, 1.),
        );
        draw_line(
            start.x,
            start.y - 1.5,
            tip.x,
            tip.y - 1.5,
            1.,
            Color::new(0.45, 0.48, 0.52, 1.),
        );
        let firing = self
            .ball
            .is_some_and(|b| b.from_player == player && self.trace.len <= 3);
        if firing {
            halo(tip.x, tip.y, 16., Color::new(1., 0.7, 0.3, 0.8));
            draw_poly(
                tip.x + side * 4.,
                tip.y - 2.,
                8,
                5.,
                0.,
                Color::new(0.85, 0.85, 0.82, 0.5),
            );
        } else if !player && self.phase == Phase::EnemyReady {
            let fuse = 0.5 + 0.5 * (t * 0.3).sin();
            halo(
                start.x,
                start.y,
                7.,
                Color::new(1., 0.6, 0.2, 0.35 + 0.4 * fuse),
            );
        }
        let name = if player {
            "Free Flag"
        } else {
            self.battle_info().ship
        };
        let ns = plain(11., with_alpha(lighten(color, 0.2), 0.9))
            .with_letter_spacing(0.5)
            .with_align(TextAlign::Center);
        let half = text_width(name, ns) * 0.5;
        draw_text_styled(name, center.x.clamp(24. + half, 666. - half), 474., ns);
    }
    fn draw_impact(&self) {
        let Some(impact) = self.impact else {
            return;
        };
        if self.impact_ticks == 0 {
            return;
        }
        let p = screen(impact);
        if !in_sea(p) {
            return;
        }
        let splash = impact.y <= 0.5;
        let total = if splash { 35. } else { 50. };
        let k = ((total - f32::from(self.impact_ticks)) / total).clamp(0., 1.);
        if splash {
            let h = 28. * (k * std::f32::consts::PI).sin();
            let foam = Color::new(0.85, 0.95, 0.97, 0.75 * (1. - k));
            draw_triangle(
                vec2(p.x - 6., WATER),
                vec2(p.x - 1., WATER - h),
                vec2(p.x + 1., WATER),
                foam,
            );
            draw_triangle(
                vec2(p.x - 1., WATER),
                vec2(p.x + 2., WATER - h * 0.75),
                vec2(p.x + 6., WATER),
                foam,
            );
            let spread = 6. + k * 30.;
            draw_line(
                p.x - spread - 8.,
                WATER + 2.,
                p.x - spread,
                WATER + 2.,
                1.5,
                foam,
            );
            draw_line(
                p.x + spread,
                WATER + 2.,
                p.x + spread + 8.,
                WATER + 2.,
                1.5,
                foam,
            );
            for n in 0..6 {
                let a = 0.5 + n as f32 * 0.42;
                let q = vec2(
                    p.x + a.cos() * k * 26.,
                    WATER - (a.sin() * 34. * k - 42. * k * k).max(0.),
                );
                if in_sea(q) {
                    draw_rectangle(q.x - 1., q.y - 1., 2., 2., foam);
                }
            }
            return;
        }
        let hit = self.last_damage > 0;
        if hit && k < 0.6 {
            halo(
                p.x,
                p.y,
                10. + 16. * (1. - k),
                Color::new(1., 0.55, 0.2, 0.9 * (1. - k / 0.6)),
            );
        }
        let ring = if hit { GOLD } else { STEEL };
        shapes::stroke_circle(
            p,
            5. + k * 20.,
            2. * (1. - k) + 0.5,
            with_alpha(ring, 1. - k),
        );
        for n in 0..7 {
            let a = 0.3 + n as f32 * 0.42;
            let q = p + vec2(a.cos() * k * 30., -a.sin() * k * 32. + k * k * 30.);
            if in_sea(q) {
                let c = if n % 2 == 0 {
                    Color::new(1., 0.7, 0.35, 1. - k)
                } else {
                    Color::new(0.35, 0.25, 0.18, 1. - k)
                };
                draw_rectangle(q.x - 1.5, q.y - 1.5, 3., 3., c);
            }
        }
        let label = if hit {
            format!("-{}", self.last_damage)
        } else {
            "Armor held".to_string()
        };
        let s = style(
            if hit { 17. } else { 12. },
            with_alpha(if hit { GOLD } else { STEEL }, 1. - k * k),
        )
        .with_align(TextAlign::Center);
        let half = text_width(&label, s) * 0.5;
        draw_text_styled(
            &label,
            p.x.clamp(30. + half, 660. - half),
            (p.y - 16. - k * 14.).max(190.),
            s,
        );
    }
    fn draw_instruments(&self) {
        slab(682., 116., 258., 452., DARK);
        let x = 700.;
        let right = 922.;
        eyebrow("GUN DECK", x, 142., with_alpha(GOLD, 0.85));
        let (phase, tone) = match self.phase {
            Phase::Aim => ("Your shot", TEAL),
            Phase::Flight => {
                if self.ball.is_some_and(|b| b.from_player) {
                    ("Shot away", GOLD)
                } else {
                    ("Incoming", RED)
                }
            }
            Phase::EnemyReady => ("Enemy aims", RED),
            Phase::Dock => ("In harbor", TEAL),
            Phase::Briefing => ("Briefing", SOFT),
            Phase::Setback => ("Lifeboats", RED),
            Phase::Liberated => ("Victory", GOLD),
            Phase::Sunk => ("Sunk", RED),
        };
        let pw = pill_width(phase);
        pill(phase, right - pw, 129., tone);
        divider(x, 156., 222., with_alpha(GOLD, 0.25));

        // Elevation and powder.
        eyebrow("ANGLE", x, 174., MUTED);
        say(
            "← →",
            796. - text_width("← →", plain(11., MUTED)),
            174.,
            plain(11., MUTED),
        );
        say(
            &format!("{}°", self.setting.angle),
            x,
            207.,
            style(26., TEAL),
        );
        bar(
            x,
            214.,
            96.,
            (f32::from(self.setting.angle) - 15.) / 55.,
            TEAL,
        );
        let px = 812.;
        let limit = self.power_limit();
        eyebrow("POWDER", px, 174., MUTED);
        say(
            "↑ ↓",
            right - text_width("↑ ↓", plain(11., MUTED)),
            174.,
            plain(11., MUTED),
        );
        let w = say(&self.setting.power.to_string(), px, 207., style(26., TEAL));
        say(
            &format!("/ {limit}"),
            px + w + 5.,
            207.,
            plain(13., if limit < 90 { RED } else { MUTED }),
        );
        bar(
            px,
            214.,
            110.,
            (f32::from(self.setting.power) - 25.) / f32::from(limit.saturating_sub(25).max(1)),
            TEAL,
        );

        // Ammunition racks.
        for (i, name) in AMMO.iter().enumerate() {
            let tx = x + i as f32 * 76.;
            let ty = 228.;
            let selected = self.setting.ammo.index() == i;
            let empty = self.ammo[i] == 0;
            tile(tx, ty, 70., 48., selected, empty && !selected);
            let a = if empty { 0.45 } else { 1. };
            let ammo = [Ammo::Round, Ammo::Chain, Ammo::Shell][i];
            ammo_icon(ammo, tx + 16., ty + 18., 6.5, a);
            let count = format!("×{}", self.ammo[i]);
            let cs = plain(16., with_alpha(if selected { GOLD } else { CREAM }, a));
            say(&count, tx + 62. - text_width(&count, cs), ty + 24., cs);
            say(name, tx + 8., ty + 41., plain(11., with_alpha(SOFT, a)));
            let key = (i + 1).to_string();
            let ks = plain(10., MUTED);
            say(&key, tx + 62. - text_width(&key, ks), ty + 41., ks);
        }

        // Forecast and navigator.
        eyebrow("FORECAST", x, 298., MUTED);
        let (forecast, fc) = match self.forecast.contact {
            Some((1, part)) => (format!("Enemy {}", PARTS[part].to_lowercase()), GOLD),
            Some((_, part)) => (format!("Own {}!", PARTS[part].to_lowercase()), RED),
            None => ("Open water".to_string(), SOFT),
        };
        say_fit(&forecast, x, 316., 104., plain(14., fc));
        eyebrow("TABLE", px, 298., MUTED);
        if self.show_table {
            say_fit(
                &format!(
                    "{}° · {} · {}",
                    self.table.angle,
                    self.table.power,
                    self.table.ammo.name()
                ),
                px,
                316.,
                110.,
                plain(14., TEAL),
            );
        } else {
            say("Hidden", px, 316., plain(14., with_alpha(SOFT, 0.6)));
        }
        if let Some(last) = self.last_setting {
            say(
                &format!("Last shot {}° · {} powder", last.angle, last.power),
                x,
                336.,
                plain(12., MUTED),
            );
        }
        divider(x, 348., 222., with_alpha(GOLD, 0.25));

        // Both hulls, section by section, under one shared header.
        let col = |part: usize| x + 52. + part as f32 * 43.;
        eyebrow("HULL", x, 368., MUTED);
        for (part, label) in PARTS.iter().enumerate() {
            say(label, col(part), 368., plain(10., MUTED));
        }
        for side in 0..2 {
            let y = 392. + side as f32 * 40.;
            let ship = self.ships[side];
            let color = if side == 0 { TEAL } else { self.enemy_color() };
            say(
                if side == 0 { "You" } else { "Foe" },
                x,
                y,
                plain(14., color),
            );
            say(&ship.hull().to_string(), x, y + 15., plain(11., MUTED));
            for part in 0..4 {
                let cx = col(part);
                let live = ship.hp[part] > 0;
                bar(
                    cx,
                    y - 9.,
                    38.,
                    f32::from(ship.hp[part]) / f32::from(ship.max[part].max(1)),
                    color,
                );
                say(
                    &ship.hp[part].to_string(),
                    cx,
                    y + 9.,
                    plain(12., if live { CREAM } else { RED }),
                );
            }
        }
        let mut ux = x;
        for (label, level) in ["Guns", "Plating", "Magazine"].iter().zip(self.upgrades) {
            let text = format!("{label} {level}");
            ux += pill(&text, ux, 476., if level > 0 { GOLD } else { MUTED }) + 6.;
        }
        let k = 506.;
        let w = keycap("Space", "Fire", x, k, SOFT);
        keycap("X", "Table", x + w + 16., k, SOFT);
        let w = keycap("C", "Retreat", x, k + 28., SOFT);
        keycap("1-3", "Ammo", x + w + 16., k + 28., SOFT);
    }
    fn draw_log(&self) {
        slab(20., 480., 650., 88., DARK);
        eyebrow("SHIP'S LOG", 36., 502., with_alpha(GOLD, 0.85));
        if self.phase == Phase::EnemyReady {
            let label = format!("{} readies a volley", self.battle_info().captain);
            let w = pill_width(&label);
            pill(&label, 654. - w, 489., RED);
        } else {
            // Legend for the chart's two arcs.
            let ls = plain(11., MUTED);
            let fw = text_width("Forecast", ls);
            let fx = 654. - fw;
            say("Forecast", fx, 502., ls);
            for d in 0..4 {
                draw_poly(
                    fx - 26. + d as f32 * 5.,
                    498.,
                    6,
                    1.4,
                    0.,
                    Color::new(0.62, 0.92, 0.90, 0.85),
                );
            }
            let lw = text_width("Last shot", ls);
            let lx = fx - 40. - lw;
            say("Last shot", lx, 502., ls);
            draw_line(
                lx - 22.,
                498.,
                lx - 6.,
                498.,
                1.5,
                Color::new(0.85, 0.72, 0.42, 0.7),
            );
        }
        para(self.log, 36., 527., 618., plain(15., CREAM), 19.);
    }
    fn draw_overlay(&self) {
        let (x, y, w, h) = (40., 134., 610., 316.);
        rounded(
            20.,
            116.,
            650.,
            344.,
            12.,
            Color::new(0.01, 0.03, 0.05, 0.55),
        );
        slab(x, y, w, h, Color::new(0.06, 0.09, 0.12, 0.98));
        rounded(x + 24., y, 72., 3., 1.5, GOLD);
        let lx = x + 24.;
        let max = w - 48.;
        match self.phase {
            Phase::Briefing => {
                let info = self.battle_info();
                eyebrow(
                    &format!("CHAPTER {} OF 8", self.battle_number()),
                    lx,
                    y + 32.,
                    GOLD,
                );
                say(info.island, lx, y + 64., style(26., CREAM));
                say(
                    &format!("{} aboard {}", info.captain, info.ship),
                    lx,
                    y + 89.,
                    plain(13., MUTED),
                );
                let after = para(info.story, lx, y + 120., max, plain(16., CREAM), 21.);
                let cy = after - 6.;
                rounded(lx, cy, max, 32., 8., with_alpha(TEAL, 0.12));
                rounded(lx, cy, 3., 32., 1.5, TEAL);
                let ns = plain(14., TEAL);
                let nw = say("Navigator", lx + 14., ns.centered_baseline(cy + 16.), ns);
                say_fit(
                    info.advice,
                    lx + 24. + nw,
                    ns.centered_baseline(cy + 16.),
                    max - nw - 36.,
                    plain(14., CREAM),
                );
                let ky = y + 214.;
                let mut kx = lx;
                for (key, label) in [
                    ("← →", "Angle"),
                    ("↑ ↓", "Powder"),
                    ("1-3", "Ammo"),
                    ("X", "Firing table"),
                ] {
                    kx += keycap(key, label, kx, ky, SOFT) + 18.;
                }
                button(
                    lx,
                    y + 252.,
                    300.,
                    40.,
                    "Space",
                    "Raise the free flag",
                    true,
                );
            }
            Phase::Dock => {
                eyebrow("HARBOR LIBERATED", lx, y + 32., GOLD);
                say("Shipwright", lx, y + 64., style(26., CREAM));
                let credits = format!("{} salvage", self.credits);
                let cw = pill_width(&credits);
                pill(&credits, x + w - 24. - cw, y + 46., GOLD);
                say(
                    &format!(
                        "Hull {} / 195  ·  Mast {} / 50",
                        self.hull(),
                        self.ships[0].hp[3]
                    ),
                    lx,
                    y + 86.,
                    plain(13., MUTED),
                );
                for (i, (name, detail)) in [
                    ("Patch", "+80 hull, +25 mast"),
                    ("Long guns", "+8 direct damage"),
                    ("Plating", "+5 armor per level"),
                    ("Magazine", "+1 shell per island"),
                ]
                .iter()
                .enumerate()
                {
                    let tx = lx + (i % 2) as f32 * 287.;
                    let ty = y + 102. + (i / 2) as f32 * 62.;
                    let cost = self.cost(i + 1);
                    let affordable = cost.is_some_and(|v| v <= self.credits);
                    tile(tx, ty, 275., 54., false, cost.is_none());
                    keycap(&(i + 1).to_string(), "", tx + 12., ty + 17., SOFT);
                    let a = if cost.is_some() { 1. } else { 0.5 };
                    say(name, tx + 44., ty + 24., plain(15., with_alpha(CREAM, a)));
                    say(
                        detail,
                        tx + 44.,
                        ty + 42.,
                        plain(12., with_alpha(SOFT, 0.85 * a)),
                    );
                    let price = cost.map_or(
                        if i == 0 {
                            "Full".to_string()
                        } else {
                            "Maxed".to_string()
                        },
                        |v| format!("{v}"),
                    );
                    let color = if affordable {
                        GOLD
                    } else if cost.is_some() {
                        RED
                    } else {
                        MUTED
                    };
                    let pw = pill_width(&price);
                    pill(&price, tx + 263. - pw, ty + 8., color);
                }
                button(
                    lx,
                    y + 240.,
                    300.,
                    40.,
                    "Space",
                    "Sail on, ammo refilled",
                    true,
                );
                para(
                    "Hull damage carries over. Ammo and plating refill.",
                    lx + 318.,
                    y + 256.,
                    max - 318.,
                    plain(12., MUTED),
                    16.,
                );
            }
            Phase::Setback => {
                eyebrow("SETBACK", lx, y + 32., RED);
                say("The sea claims this sortie", lx, y + 64., style(24., CREAM));
                let after = para(self.log, lx, y + 98., max, plain(16., CREAM), 21.);
                para(
                    "Retry resets this island. Missed shots still count toward accuracy.",
                    lx,
                    after + 6.,
                    max,
                    plain(13., MUTED),
                    18.,
                );
                if self.lifeboats > 0 {
                    button(
                        lx,
                        y + 240.,
                        270.,
                        40.,
                        "C",
                        &format!("Retry ({} lifeboats left)", self.lifeboats),
                        true,
                    );
                    button(lx + 286., y + 240., 160., 40., "Space", "Retire", false);
                } else {
                    button(lx, y + 240., 220., 40., "Space", "Retire", true);
                }
            }
            Phase::Liberated => {
                eyebrow("ALL EIGHT HARBORS FREE", lx, y + 32., GOLD);
                say(
                    "Eight harbors. One free sea.",
                    lx,
                    y + 64.,
                    style(26., CREAM),
                );
                eyebrow("RANK", lx, y + 112., MUTED);
                say(self.rank(), lx, y + 144., style(28., TEAL));
                eyebrow("SCORE", lx + 300., y + 112., MUTED);
                say(
                    &self.score.to_string(),
                    lx + 300.,
                    y + 144.,
                    style(28., GOLD),
                );
                say(
                    &format!(
                        "{} of {} shots hit  ·  {} lifeboats kept",
                        self.hits, self.shots, self.lifeboats
                    ),
                    lx,
                    y + 178.,
                    plain(14., SOFT),
                );
                say(
                    "Try fewer shells and no firing table for a higher rank.",
                    lx,
                    y + 204.,
                    plain(13., MUTED),
                );
                keycap("R", "Start a new voyage", lx, y + 254., SOFT);
            }
            Phase::Sunk => {
                eyebrow("VOYAGE ENDED", lx, y + 32., RED);
                say("The free flag falls", lx, y + 64., style(26., CREAM));
                say("No lifeboats remaining.", lx, y + 92., plain(15., SOFT));
                eyebrow("HARBORS FREED", lx, y + 132., MUTED);
                say(
                    &format!("{} of 8", self.wins),
                    lx,
                    y + 164.,
                    style(28., TEAL),
                );
                eyebrow("SCORE", lx + 300., y + 132., MUTED);
                say(
                    &self.score.to_string(),
                    lx + 300.,
                    y + 164.,
                    style(28., GOLD),
                );
                keycap("R", "Start a new voyage", lx, y + 254., SOFT);
            }
            _ => {}
        }
    }
}
/// Faded polyline of a recorded flight, clipped to the chart.
fn trajectory(trace: &Trace, color: Color, width: f32) {
    for pair in trace.points[..trace.len].windows(2) {
        let a = screen(pair[0]);
        let b = screen(pair[1]);
        if in_sea(a) && in_sea(b) {
            draw_line(a.x, a.y, b.x, b.y, width, color);
        }
    }
}
