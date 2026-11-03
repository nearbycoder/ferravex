use crate::{
    Game,
    content::{HazardKind, UPGRADE_HELP, UPGRADE_NAMES},
    model::{BASE, Phase, RescueState},
};
use ferravex_arcade::shapes::{self, Cap, Fill, Stroke};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;
use ferravex_render::effects::{self, EffectBlend, EffectStyle, RibbonTriangle, RibbonVertex};
use std::f32::consts::{FRAC_PI_2, TAU};

const AQUA: Color = Color::new(0.29, 0.91, 0.88, 1.);
const GOLD: Color = Color::new(1., 0.76, 0.27, 1.);
const CORAL: Color = Color::new(1., 0.42, 0.42, 1.);
const HAIRLINE: Color = Color::new(1., 1., 1., 0.09);
/// The water window: simulation units map one-to-one onto this rectangle.
const SEA_X: f32 = 28.;
const SEA_Y: f32 = 148.;
const SEA_W: f32 = 640.;
const SEA_H: f32 = 400.;
const SEA_BOTTOM: f32 = SEA_Y + SEA_H;
/// Instrument card content columns.
const IX0: f32 = 718.;
const IX1: f32 = 926.;

fn sea(p: Vec2) -> Vec2 {
    p + vec2(SEA_X, SEA_Y)
}
fn mix(a: Color, b: Color, t: f32) -> Color {
    shapes::mix(a, b, t)
}
fn hash(i: usize) -> usize {
    (i.wrapping_mul(2_654_435_761) >> 7) % 1009
}

type Vertex = (Vec2, Color);
/// Per-vertex coloured triangles in the ordinary stream (gradients and
/// lights without extra textures or shaders).
fn emit(blend: EffectBlend, triangles: impl IntoIterator<Item = [Vertex; 3]>) {
    effects::draw_triangles(
        triangles.into_iter().map(|t| RibbonTriangle {
            vertices: t.map(|(p, c)| RibbonVertex {
                position: [p.x, p.y, 0.],
                color: [c.r, c.g, c.b, c.a],
                uv: [0., 0.],
            }),
        }),
        EffectStyle {
            blend,
            depth_test: Some(false),
            depth_write: false,
        },
    );
}
/// Alpha-blended gradient triangles.
fn shade(triangles: impl IntoIterator<Item = [Vertex; 3]>) {
    emit(EffectBlend::Alpha, triangles);
}

/// Unit circle in 24 steps, computed once.
fn unit_ring() -> &'static [Vec2; 25] {
    static RING: std::sync::OnceLock<[Vec2; 25]> = std::sync::OnceLock::new();
    RING.get_or_init(|| {
        std::array::from_fn(|i| {
            let a = i as f32 * std::f32::consts::TAU / 24.;
            vec2(a.cos(), a.sin())
        })
    })
}
/// Cheap additive light: three bands approximating a quadratic falloff,
/// built from a cached unit ring so no trigonometry runs per frame.
fn light(center: Vec2, radius: f32, color: Color) {
    const BANDS: [(f32, f32); 4] = [(0., 1.), (0.35, 0.42), (0.7, 0.09), (1., 0.)];
    let ring = unit_ring();
    let at = |band: usize, i: usize| {
        let (r, a) = BANDS[band];
        (
            center + ring[i] * radius * r,
            with_alpha(color, color.a * a),
        )
    };
    emit(
        EffectBlend::Additive,
        (0..24).flat_map(move |i| {
            [
                [at(0, i), at(1, i), at(1, i + 1)],
                [at(1, i), at(2, i), at(1, i + 1)],
                [at(1, i + 1), at(2, i), at(2, i + 1)],
                [at(2, i), at(3, i), at(2, i + 1)],
                [at(2, i + 1), at(3, i), at(3, i + 1)],
            ]
        }),
    );
}
/// Quad `a b / c d` (top pair, bottom pair) as two shaded triangles.
fn quad(a: Vertex, b: Vertex, c: Vertex, d: Vertex) -> [[Vertex; 3]; 2] {
    [[a, b, c], [c, b, d]]
}

/// Flat text for card surfaces, where a drop shadow would be invisible and
/// would double the glyph geometry.
fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
fn say(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(size, color)).width
}
fn say_right(value: &str, right: f32, y: f32, size: f32, color: Color) {
    draw_text_styled(
        value,
        right,
        y,
        ink(size, color).with_align(TextAlign::Right),
    );
}
fn say_centered(value: &str, cx: f32, y: f32, size: f32, color: Color) {
    draw_text_styled(value, cx, y, ink(size, color).with_align(TextAlign::Center));
}
/// Flat text shrunk (never enlarged) to fit `width`.
fn say_fit(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) {
    let s = ink(size, color);
    let w = text_width(value, s);
    let s = if w > width && w > 0. {
        s.with_size(size * width / w)
    } else {
        s
    };
    draw_text_styled(value, x, y, s);
}
/// Flat wrapped paragraph; returns the next baseline.
fn say_wrapped(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color);
    let mut baseline = y;
    for line in wrap_text(value, width, s) {
        draw_text_styled(line, x, baseline, s);
        baseline += size * 1.3;
    }
    baseline
}
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(11., color).with_letter_spacing(1.5)).width
}
fn eyebrow_right(value: &str, right: f32, y: f32, color: Color) {
    let s = ink(11., color)
        .with_letter_spacing(1.5)
        .with_align(TextAlign::Right);
    draw_text_styled(value, right, y, s);
}
fn chip_width(label: &str, size: f32) -> f32 {
    text_width(label, style(size, PAPER)) + size * 1.3
}
fn chip_right(label: &str, right: f32, y: f32, size: f32, color: Color) {
    chip(label, right - chip_width(label, size), y, size, color);
}
fn keycap_width(key: &str, action: &str) -> f32 {
    let s = TextStyle::modern(13., PAPER);
    (text_width(key, s) + 12.).max(20.) + 7. + text_width(action, s)
}
/// Muted label on the left, a larger value right-aligned on the same baseline.
fn readout(label: &str, value: &str, y: f32, color: Color) {
    say(label, IX0, y, 14., MUTED);
    say_right(value, IX1, y, 19., color);
}
/// A wide primary button whose rectangle matches the model's pointer target.
fn button(x: f32, y: f32, w: f32, h: f32, key: &str, label: &str, color: Color) {
    let r = Rect::new(x, y, w, h);
    shapes::drop_shadow(
        r,
        10.,
        shapes::Shadow::soft(Color::new(0., 0., 0., 0.35))
            .offset(vec2(0., 3.))
            .blur(10.),
    );
    rounded_gradient(x, y, w, h, 10., lighten(color, 0.12), darken(color, 0.12));
    outline(x, y, w, h, 10., 1., Color::new(1., 1., 1., 0.25));
    let s = TextStyle::modern(18., INK).with_align(TextAlign::Center);
    let label_w = text_width(label, s);
    let key_w = text_width(key, TextStyle::modern(13., PAPER)) + 12.;
    let total = key_w + 10. + label_w;
    let left = x + (w - total) * 0.5;
    keycap(key, "", left, y + h * 0.5 - 11., PAPER);
    draw_text_styled(
        label,
        left + key_w + 10. + label_w * 0.5,
        s.centered_baseline(y + h * 0.5),
        s,
    );
}
fn medal_metal(medal: u8) -> Color {
    match medal {
        3 => Color::new(1., 0.8, 0.3, 1.),
        2 => Color::new(0.8, 0.86, 0.92, 1.),
        1 => Color::new(0.85, 0.55, 0.32, 1.),
        _ => MUTED,
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
        self.draw_sea();
        self.draw_instruments();
        if self.phase != Phase::Diving {
            self.draw_overlay();
        }
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!("Sortie {} / 6", self.sortie()),
            objective: format!("{} / 27 survivors home", self.rescued()),
            status: format!("{} credits | {} tugs", self.credits, self.recovery),
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        self.outcome()
    }
    fn summary(&self) -> String {
        format!(
            "{} / 27 survivors safe. {} points. {} gold medals. {} recovery tugs remain.",
            self.rescued(),
            self.score,
            self.medals.iter().filter(|&&m| m == 3).count(),
            self.recovery
        )
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
}

impl Game {
    fn depth_fraction(&self) -> f32 {
        self.mission as f32 / 5.
    }
    /// Height of the decorative seabed above the window floor at sea-local `x`.
    fn seabed(&self, x: f32) -> f32 {
        let m = self.mission as f32;
        (5. + 3. * (x * 0.021 + m * 1.3).sin() + 2. * (x * 0.067 + m * 0.7).sin()).max(1.5)
    }

    fn draw_sea(&self) {
        let t = self.elapsed as f32;
        let deep = self.depth_fraction();
        surface(16., 112., 672., 456.);
        // Header: chart name, depth gauge and current.
        let depth = format!("{} m", self.depth());
        let current = self.chart().current;
        let mut right = 674.;
        for (label, color) in [
            (depth.as_str(), AQUA),
            (
                match current {
                    c if c > 0. => "Current east",
                    c if c < 0. => "Current west",
                    _ => "",
                },
                MUTED,
            ),
            (
                if self.mission >= 4 {
                    "High pressure"
                } else {
                    ""
                },
                Color::new(0.82, 0.6, 0.98, 1.),
            ),
        ] {
            if !label.is_empty() {
                chip_right(label, right, 122., 12., color);
                right -= chip_width(label, 12.) + 8.;
            }
        }
        say_fit(self.chart().name, 30., 137., right - 42., 18., PAPER);

        // Water column: brighter near the surface, darker on deeper charts.
        let top = mix(
            Color::new(0.07, 0.36, 0.42, 1.),
            Color::new(0.04, 0.17, 0.26, 1.),
            deep,
        );
        let bottom = mix(
            Color::new(0.015, 0.075, 0.12, 1.),
            Color::new(0.01, 0.025, 0.06, 1.),
            deep,
        );
        rounded_gradient(SEA_X, SEA_Y, SEA_W, SEA_H, 6., top, bottom);
        // Sunlight shafts, fading with depth.
        let light = 0.085 * (1. - deep * 0.65);
        let lit = Color::new(0.65, 0.97, 1., light);
        let clear = with_alpha(lit, 0.);
        shade((0..4).flat_map(|i| {
            let sway = (t / 260. + i as f32 * 1.7).sin() * 16.;
            let x0 = SEA_X + 40. + i as f32 * 140. + sway;
            let w0 = 24. + (i % 2) as f32 * 16.;
            let y1 = SEA_Y + 230. + (i % 3) as f32 * 40.;
            quad(
                (vec2(x0, SEA_Y), lit),
                (vec2(x0 + w0, SEA_Y), lit),
                (vec2(x0 + 50., y1), clear),
                (vec2(x0 + 50. + w0 * 1.8, y1), clear),
            )
        }));
        // Surface shimmer.
        let shimmer = Rect::new(SEA_X, SEA_Y, SEA_W, 18.);
        shapes::fill_rounded_rect(
            shimmer,
            0.,
            Fill::vertical(
                shimmer,
                Color::new(0.7, 1., 1., 0.1 * (1. - deep * 0.7)),
                Color::new(0.7, 1., 1., 0.),
            ),
        );
        // Marine snow drifting through the beam.
        for i in 0..38 {
            let x = SEA_X + 7. + ((i * 113 + 29) % 624) as f32;
            let y = SEA_Y + 6. + ((i * 71 + self.elapsed as usize / 5) % 380) as f32;
            let s = if i % 3 == 0 { 2. } else { 1.2 };
            panel(x, y, s, s, Color::new(0.6, 0.9, 0.95, 0.16));
        }
        self.draw_seabed(t);
        if self.mission >= 4 {
            let y = SEA_Y + 315.;
            let inside = self.position.y > 315. && self.upgrades[1] == 0;
            let tint = Color::new(0.45, 0.12, 0.55, if inside { 0.32 } else { 0.2 });
            shade(quad(
                (vec2(SEA_X, y), with_alpha(tint, 0.)),
                (vec2(SEA_X + SEA_W, y), with_alpha(tint, 0.)),
                (vec2(SEA_X, SEA_BOTTOM), tint),
                (vec2(SEA_X + SEA_W, SEA_BOTTOM), tint),
            ));
            for k in 0..32 {
                let x = SEA_X + 4. + k as f32 * 20.;
                panel(x, y, 10., 1., Color::new(0.75, 0.5, 0.9, 0.4));
            }
        }
        self.draw_reefs();
        self.draw_dock(t);
        self.draw_hazards(t);
        self.draw_divers(t);
        self.draw_sub(t);
        self.draw_lights(t);
        if self.phase == Phase::Diving && self.oxygen < 12. {
            // Pulsing red edges when the air is nearly gone.
            let a = 0.16 + 0.08 * (t / 9.).sin();
            let edge = Color::new(1., 0.25, 0.3, a);
            let none = with_alpha(edge, 0.);
            let (x0, x1, y0, y1) = (SEA_X, SEA_X + SEA_W, SEA_Y, SEA_BOTTOM);
            shade(
                quad(
                    (vec2(x0, y0), edge),
                    (vec2(x0 + 30., y0), none),
                    (vec2(x0, y1), edge),
                    (vec2(x0 + 30., y1), none),
                )
                .into_iter()
                .chain(quad(
                    (vec2(x1 - 30., y0), none),
                    (vec2(x1, y0), edge),
                    (vec2(x1 - 30., y1), none),
                    (vec2(x1, y1), edge),
                )),
            );
        }
        // Notice strip below the water.
        let (status, color) = if self.phase == Phase::Diving && self.oxygen < 12. {
            (
                "Low air: return to the dock now. Slow + Space refills.",
                CORAL,
            )
        } else {
            (self.notice.as_str(), Color::new(0.82, 0.88, 0.92, 1.))
        };
        panel(SEA_X + 2., SEA_BOTTOM + 9., 3., 11., with_alpha(color, 0.8));
        say_fit(
            status,
            SEA_X + 12.,
            SEA_BOTTOM + 18.,
            SEA_W - 14.,
            13.,
            color,
        );
    }

    fn draw_seabed(&self, t: f32) {
        const STEPS: usize = 40;
        let step = SEA_W / STEPS as f32;
        let rim = Color::new(0.1, 0.24, 0.26, 1.);
        let floor = Color::new(0.01, 0.035, 0.055, 1.);
        // Kelp sways behind the sand line.
        for i in 0..15 {
            let local = 18. + i as f32 * 43. + (hash(i) % 17) as f32;
            let base = vec2(SEA_X + local, SEA_BOTTOM - self.seabed(local) + 2.);
            let height = 46. + (hash(i + 40) % 50) as f32;
            let phase = t / 110. + i as f32 * 0.9;
            let mut points = [Vec2::ZERO; 5];
            for (j, point) in points.iter_mut().enumerate() {
                let k = j as f32 / 4.;
                *point = base + vec2((phase + k * 1.6).sin() * 7. * k * k, -height * k);
            }
            let c = if i % 2 == 0 {
                Color::new(0.1, 0.42, 0.34, 0.7)
            } else {
                Color::new(0.08, 0.32, 0.3, 0.55)
            };
            for pair in points.windows(2) {
                draw_line(
                    pair[0].x,
                    pair[0].y,
                    pair[1].x,
                    pair[1].y,
                    3. - pair[0].distance(base) / height,
                    c,
                );
            }
            for leaf in [1, 3] {
                let p = points[leaf];
                let side = if (i + leaf) % 2 == 0 { 1. } else { -1. };
                draw_triangle(p, p + vec2(side * 7., -5.), p + vec2(side * 2., -9.), c);
            }
        }
        shade((0..STEPS).flat_map(|i| {
            let xa = i as f32 * step;
            let xb = xa + step;
            let (ya, yb) = (SEA_BOTTOM - self.seabed(xa), SEA_BOTTOM - self.seabed(xb));
            quad(
                (vec2(SEA_X + xa, ya), rim),
                (vec2(SEA_X + xb, yb), rim),
                (vec2(SEA_X + xa, SEA_BOTTOM), floor),
                (vec2(SEA_X + xb, SEA_BOTTOM), floor),
            )
        }));
        let mut ridge = [Vec2::ZERO; STEPS + 1];
        for (i, p) in ridge.iter_mut().enumerate() {
            let x = i as f32 * step;
            *p = vec2(SEA_X + x, SEA_BOTTOM - self.seabed(x));
        }
        for pair in ridge.windows(2) {
            draw_line(
                pair[0].x,
                pair[0].y,
                pair[1].x,
                pair[1].y,
                1.2,
                Color::new(0.4, 0.75, 0.72, 0.3),
            );
        }
    }

    fn draw_reefs(&self) {
        let rock_top = Color::new(0.2, 0.31, 0.35, 1.);
        let rock_bottom = Color::new(0.06, 0.1, 0.14, 1.);
        let corals = [
            Color::new(1., 0.48, 0.5, 0.9),
            Color::new(1., 0.66, 0.36, 0.85),
            Color::new(0.82, 0.5, 0.95, 0.85),
        ];
        for (i, reef) in self.chart().reefs.iter().enumerate() {
            let p = sea(vec2(reef.x, reef.y));
            let (w, h) = (reef.w, reef.h);
            // Soft contact shadow, kept inside the water window.
            let sh = (h + 2.).min(SEA_BOTTOM - p.y - 6.).max(0.);
            let sw = w.min(SEA_X + SEA_W - p.x - 4.);
            rounded(
                p.x + 4.,
                p.y + 6.,
                sw,
                sh,
                8.,
                Color::new(0., 0.02, 0.04, 0.4),
            );
            rounded_gradient(p.x, p.y, w, h, 7., rock_top, rock_bottom);
            // Shaded flank for a sculpted, three-dimensional read.
            let (fx0, fx1) = (p.x + w * 0.62, p.x + w - 3.);
            let (fy0, fy1) = (p.y + 5., p.y + h - 5.);
            let clear = Color::new(0., 0.02, 0.04, 0.);
            let dusk = Color::new(0., 0.02, 0.04, 0.35);
            shade([
                [
                    (vec2(fx0, fy0), clear),
                    (vec2(fx1, fy0), dusk),
                    (vec2(fx0, fy1), clear),
                ],
                [
                    (vec2(fx0, fy1), clear),
                    (vec2(fx1, fy0), dusk),
                    (vec2(fx1, fy1), dusk),
                ],
            ]);
            let mut y = p.y + 22.;
            let mut k = 0;
            while y < p.y + h - 10. {
                let tilt = if (i + k) % 2 == 0 { 5. } else { -4. };
                draw_line(
                    p.x + 6.,
                    y,
                    p.x + w - 8.,
                    y + tilt,
                    1.,
                    Color::new(0.03, 0.07, 0.1, 0.55),
                );
                y += 26. + (hash(i * 7 + k) % 14) as f32;
                k += 1;
            }
            panel(
                p.x + 5.,
                p.y + 1.5,
                w - 10.,
                1.5,
                Color::new(0.6, 0.92, 0.9, 0.35),
            );
            // Coral tufts along the lit ledge.
            let count = (w / 15.) as usize;
            for k in 0..count {
                let n = hash(i * 31 + k);
                if n % 5 >= 3 {
                    continue;
                }
                let x = p.x + 8. + k as f32 * 15. + (n % 6) as f32;
                if x > p.x + w - 6. {
                    break;
                }
                let r = 2.2 + (n % 3) as f32 * 0.9;
                draw_circle(x, p.y + 1., r, corals[n % 3]);
            }
        }
    }

    fn draw_dock(&self, t: f32) {
        let d = sea(BASE);
        // Tether to the surface support ship.
        rounded_gradient(
            d.x - 3.,
            SEA_Y,
            6.,
            d.y - 30. - SEA_Y,
            2.,
            Color::new(0.45, 0.5, 0.52, 1.),
            Color::new(0.22, 0.26, 0.3, 1.),
        );
        let carrying = self.onboard() > 0 && self.phase == Phase::Diving;
        let pulse = 0.5 + 0.5 * (t / 14.).sin();
        shapes::stroke_circle(
            d,
            29.,
            1.5,
            Color::new(
                1.,
                0.76,
                0.27,
                if carrying { 0.55 + 0.3 * pulse } else { 0.4 },
            ),
        );
        // Docking collar with marker lamps.
        rounded_gradient(
            d.x - 30.,
            d.y - 34.,
            60.,
            9.,
            4.,
            lighten(GOLD, 0.2),
            darken(GOLD, 0.25),
        );
        panel(d.x - 26., d.y - 33., 52., 1.5, Color::new(1., 1., 1., 0.35));
        for side in [-1., 1.] {
            let lamp = vec2(d.x + side * 25., d.y - 24.);
            let on = (self.elapsed / 30).is_multiple_of(2);
            draw_circle(
                lamp.x,
                lamp.y,
                2.5,
                if on { PAPER } else { darken(GOLD, 0.3) },
            );
        }
        chip("Dock", d.x + 36., d.y - 40., 11., GOLD);
    }

    fn draw_hazards(&self, t: f32) {
        for hazard in self.chart().hazards {
            let p = sea(hazard.position(self.elapsed));
            let active = hazard.active(self.elapsed);
            if self.sonar > 0 {
                let c = if active { CORAL } else { MUTED };
                shapes::fill_circle(p, hazard.radius() + 11., with_alpha(c, 0.07));
                shapes::stroke_circle(p, hazard.radius() + 11., 1.5, with_alpha(c, 0.85));
            }
            match hazard.kind {
                HazardKind::Jelly => {
                    let beat = (t / 13. + hazard.phase as f32).sin();
                    for j in -2..=2 {
                        let x = p.x + j as f32 * 3.6;
                        let wave = t / 17. + j as f32;
                        let pts = [
                            vec2(x, p.y + 1.),
                            vec2(x + wave.sin() * 2.5, p.y + 9.),
                            vec2(x + (wave + 1.).sin() * 4., p.y + 17.),
                            vec2(x + (wave + 2.).sin() * 5., p.y + 24. + beat * 2.),
                        ];
                        for pair in pts.windows(2) {
                            draw_line(
                                pair[0].x,
                                pair[0].y,
                                pair[1].x,
                                pair[1].y,
                                1.1,
                                Color::new(0.8, 0.6, 1., 0.6),
                            );
                        }
                    }
                    // Dome: a lit half-disc fan over a frilled rim.
                    let r = 11. + beat * 0.8;
                    let core = vec2(p.x - 2., p.y - 4.);
                    let inner = Color::new(0.98, 0.86, 1., 0.95);
                    let rim = Color::new(0.6, 0.34, 0.86, 0.8);
                    shade((0..10).map(|k| {
                        let a0 = std::f32::consts::PI * (1. + k as f32 / 10.);
                        let a1 = std::f32::consts::PI * (1. + (k + 1) as f32 / 10.);
                        let base = vec2(p.x, p.y + 1.);
                        [
                            (core, inner),
                            (base + vec2(a0.cos(), a0.sin() * 1.05) * r, rim),
                            (base + vec2(a1.cos(), a1.sin() * 1.05) * r, rim),
                        ]
                    }));
                    shade([[
                        (core, inner),
                        (vec2(p.x + r, p.y + 1.), rim),
                        (vec2(p.x - r, p.y + 1.), rim),
                    ]]);
                    panel(p.x - r, p.y, r * 2., 3., Color::new(0.52, 0.3, 0.8, 0.95));
                }
                HazardKind::Mine => {
                    // Mooring chain down to the seabed.
                    let local = p.x - SEA_X;
                    let floor = SEA_BOTTOM - self.seabed(local);
                    let mut y = p.y + 10.;
                    while y < floor - 2. {
                        panel(p.x - 0.7, y, 1.4, 3., Color::new(0.4, 0.42, 0.45, 0.6));
                        y += 5.;
                    }
                    for i in 0..8 {
                        let a = i as f32 * TAU / 8. + 0.2;
                        let dir = vec2(a.cos(), a.sin());
                        let tip = p + dir * 15.;
                        draw_line(
                            p.x,
                            p.y,
                            tip.x,
                            tip.y,
                            2.2,
                            Color::new(0.42, 0.36, 0.38, 1.),
                        );
                        panel(tip.x - 1.6, tip.y - 1.6, 3.2, 3.2, CORAL);
                    }
                    shapes::fill_circle(
                        p,
                        9.5,
                        Fill::radial(
                            p - vec2(3., 3.),
                            12.,
                            Color::new(0.56, 0.48, 0.5, 1.),
                            Color::new(0.16, 0.11, 0.13, 1.),
                        ),
                    );
                    let blink = (self.elapsed / 24).is_multiple_of(3);
                    draw_circle(
                        p.x,
                        p.y,
                        2.6,
                        if blink { CORAL } else { darken(CORAL, 0.55) },
                    );
                }
                HazardKind::Vent => {
                    let cycle = ((self.elapsed + hazard.phase) % 300) as f32;
                    let rock_a = Color::new(0.38, 0.24, 0.2, 1.);
                    let rock_b = Color::new(0.12, 0.08, 0.09, 1.);
                    shade(quad(
                        (vec2(p.x - 6., p.y - 2.), rock_a),
                        (vec2(p.x + 6., p.y - 2.), rock_a),
                        (vec2(p.x - 16., p.y + 15.), rock_b),
                        (vec2(p.x + 16., p.y + 15.), rock_b),
                    ));
                    if active {
                        for i in 0..6 {
                            let rise = ((self.elapsed + i * 9) % 30) as f32;
                            let fade = 1. - rise / 30.;
                            draw_circle(
                                p.x + (i as f32 - 2.5) * 5. + (rise / 5. + i as f32).sin() * 3.,
                                p.y - 4. - rise * 1.1,
                                3. + rise * 0.12,
                                Color::new(1., 0.6, 0.3, 0.55 * fade),
                            );
                        }
                    } else {
                        // Warming ember and a countdown arc to the next burst.
                        let warm = cycle / 180.;
                        shapes::stroke_arc(
                            vec2(p.x, p.y + 4.),
                            15.,
                            -FRAC_PI_2,
                            warm * TAU,
                            Stroke::new(2., Color::new(1., 0.72, 0.3, 0.55)).cap(Cap::Round),
                        );
                    }
                    rounded(
                        p.x - 6.,
                        p.y - 3.,
                        12.,
                        3.,
                        1.5,
                        if active {
                            Color::new(1., 0.65, 0.35, 1.)
                        } else {
                            Color::new(0.85, 0.5, 0.25, 1.)
                        },
                    );
                }
            }
        }
    }

    fn draw_divers(&self, t: f32) {
        for (index, (point, name)) in self.chart().divers.iter().enumerate() {
            if self.divers[index] != RescueState::Waiting {
                continue;
            }
            let p = sea(*point);
            let pulse = (t / 28. + index as f32).sin();
            // Air bell: glass dome on a weighted skirt.
            let dome = Rect::new(p.x - 12., p.y - 14., 24., 26.);
            shapes::fill_rounded_rect(
                dome,
                12.,
                Fill::radial(
                    vec2(p.x - 4., p.y - 8.),
                    18.,
                    Color::new(0.55, 1., 0.96, 0.4),
                    Color::new(0.15, 0.55, 0.6, 0.22),
                ),
            );
            shapes::stroke_rounded_rect(
                dome,
                12.,
                1.4,
                Color::new(0.45, 0.98, 0.94, 0.65 + 0.3 * pulse),
            );
            panel(
                p.x - 14.,
                p.y + 10.,
                28.,
                5.,
                Color::new(0.26, 0.32, 0.36, 1.),
            );
            panel(
                p.x - 14.,
                p.y + 10.,
                28.,
                1.5,
                Color::new(0.55, 0.64, 0.68, 1.),
            );
            // Diver silhouette waving from inside.
            let wave = (t / 9. + index as f32).sin() * 3.;
            draw_circle(p.x, p.y - 5., 3.2, AQUA);
            draw_line(p.x, p.y - 2., p.x, p.y + 7., 4., AQUA);
            draw_line(p.x, p.y, p.x + 6., p.y - 5. + wave, 1.8, AQUA);
            draw_line(p.x, p.y, p.x - 5., p.y + 3., 1.8, AQUA);
            panel(p.x - 7., p.y - 9., 2., 5., Color::new(1., 1., 1., 0.4));
            // Bubbles escaping from the dome.
            for b in 0..2 {
                let rise = ((self.elapsed as usize + b * 23 + index * 11) % 46) as f32;
                draw_circle(
                    p.x + 4. + (rise / 6.).sin() * 2.,
                    p.y - 15. - rise * 0.6,
                    1.2 + b as f32 * 0.4,
                    Color::new(0.7, 1., 1., 0.45 * (1. - rise / 46.)),
                );
            }
            draw_text_styled(
                name,
                p.x,
                p.y - 22.,
                style(12., lighten(AQUA, 0.3)).with_align(TextAlign::Center),
            );
        }
    }

    fn draw_sub(&self, t: f32) {
        let p = sea(self.position);
        let f = self.facing;
        let nose = p + vec2(f * 19., 1.);
        // Headlight cone fading into the water, clipped to the window.
        let reach = 150.;
        let end_x = (nose.x + f * reach).clamp(SEA_X, SEA_X + SEA_W);
        let spread = (end_x - nose.x).abs() / reach * 46.;
        let beam = Color::new(1., 0.93, 0.62, 0.2);
        let fade = with_alpha(beam, 0.);
        shade([[
            (nose, beam),
            (vec2(end_x, (p.y - spread).max(SEA_Y)), fade),
            (vec2(end_x, (p.y + spread).min(SEA_BOTTOM)), fade),
        ]]);
        if self.sonar > 0 {
            let radius = (180 - self.sonar) as f32 * 1.4;
            let alpha = 0.5 * (self.sonar as f32 / 180.);
            for ring in 0..2 {
                let r = radius - ring as f32 * 20.;
                if r > 0. {
                    // Clip the wave to the water using short segments.
                    for segment in 0..90 {
                        let a = segment as f32 * TAU / 90.;
                        let b = (segment + 1) as f32 * TAU / 90.;
                        let aa = p + vec2(a.cos(), a.sin()) * r;
                        let bb = p + vec2(b.cos(), b.sin()) * r;
                        if [aa, bb].iter().all(|v| {
                            v.x >= SEA_X
                                && v.x <= SEA_X + SEA_W
                                && v.y >= SEA_Y
                                && v.y <= SEA_BOTTOM
                        }) {
                            draw_line(
                                aa.x,
                                aa.y,
                                bb.x,
                                bb.y,
                                1.5 - ring as f32 * 0.5,
                                Color::new(0.3, 1., 0.8, alpha),
                            );
                        }
                    }
                }
            }
        }
        // Wake bubbles from the propeller when moving.
        let speed = (self.velocity.length() / 90.).min(1.);
        if speed > 0.1 {
            for b in 0..4 {
                let age = ((self.elapsed as usize + b * 7) % 28) as f32;
                let q = p + vec2(
                    -f * (24. + age * 1.3),
                    (age / 4. + b as f32).sin() * 3. - age * 0.25,
                );
                draw_circle(
                    q.x,
                    q.y,
                    1.3 + age * 0.06,
                    Color::new(0.75, 1., 1., 0.4 * speed * (1. - age / 28.)),
                );
            }
        }
        let hit = self.impact_cooldown > 0 && self.elapsed % 8 < 4;
        let body = if hit { CORAL } else { GOLD };
        let light_body = lighten(body, 0.28);
        let dark_body = darken(body, 0.38);
        // Tail fins.
        let tail = p.x - f * 17.;
        draw_triangle(
            vec2(tail, p.y - 2.),
            vec2(tail - f * 7., p.y - 10.),
            vec2(tail - f * 7., p.y - 1.),
            dark_body,
        );
        draw_triangle(
            vec2(tail, p.y + 2.),
            vec2(tail - f * 7., p.y + 10.),
            vec2(tail - f * 7., p.y + 1.),
            dark_body,
        );
        // Spinning propeller.
        let spin = (t * (0.15 + speed * 0.5)).sin().abs();
        let prop = vec2(p.x - f * 23., p.y);
        draw_ellipse(
            prop.x,
            prop.y,
            1.6,
            2. + 6. * spin,
            0.,
            Color::new(0.85, 0.85, 0.8, 0.9),
        );
        draw_line(p.x - f * 18., p.y, prop.x, prop.y, 2., dark_body);
        // Conning tower and periscope.
        rounded_gradient(p.x - f * 2. - 6., p.y - 16., 12., 9., 3., light_body, body);
        draw_line(
            p.x - f * 1.,
            p.y - 16.,
            p.x - f * 1.,
            p.y - 22.,
            2.,
            dark_body,
        );
        draw_line(
            p.x - f * 1.,
            p.y - 22.,
            p.x + f * 3.,
            p.y - 22.,
            2.,
            dark_body,
        );
        // Hull.
        let hull = Rect::new(p.x - 19., p.y - 10., 38., 20.);
        shapes::fill_rounded_rect(hull, 10., Fill::vertical(hull, light_body, dark_body));
        panel(p.x - 15., p.y + 3., 30., 4., Color::new(0., 0., 0., 0.18));
        panel(p.x - 11., p.y - 8., 22., 1.5, Color::new(1., 1., 1., 0.45));
        // Portholes.
        for dx in [-8., 1., 10.] {
            let c = vec2(p.x + dx * f, p.y - 1.);
            draw_circle(c.x, c.y, 3.8, Color::new(0.08, 0.12, 0.16, 1.));
            draw_circle(c.x, c.y, 2.6, lighten(AQUA, 0.2));
            panel(c.x - 1.4, c.y - 1.4, 1.2, 1.2, Color::new(1., 1., 1., 0.8));
        }
        // Headlamp.
        draw_circle(nose.x, nose.y, 2.4, PAPER);
    }

    /// Every additive light in one contiguous pass, so they share a draw.
    fn draw_lights(&self, t: f32) {
        let d = sea(BASE);
        if self.onboard() > 0 && self.phase == Phase::Diving {
            let pulse = 0.5 + 0.5 * (t / 14.).sin();
            light(
                vec2(d.x, d.y),
                32.,
                Color::new(1., 0.75, 0.3, 0.07 + 0.05 * pulse),
            );
        }
        for hazard in self.chart().hazards {
            let p = sea(hazard.position(self.elapsed));
            match hazard.kind {
                HazardKind::Jelly => light(vec2(p.x, p.y - 3.), 26., Color::new(0.7, 0.4, 1., 0.3)),
                HazardKind::Mine if (self.elapsed / 24).is_multiple_of(3) => {
                    light(vec2(p.x, p.y), 14., Color::new(1., 0.3, 0.3, 0.7));
                }
                HazardKind::Mine => {}
                HazardKind::Vent if hazard.active(self.elapsed) => {
                    light(vec2(p.x, p.y - 8.), 34., Color::new(1., 0.42, 0.18, 0.55));
                }
                HazardKind::Vent => {
                    let warm = ((self.elapsed + hazard.phase) % 300) as f32 / 180.;
                    light(
                        vec2(p.x, p.y - 2.),
                        12. + warm * 6.,
                        Color::new(1., 0.5, 0.2, 0.15 + 0.35 * warm),
                    );
                }
            }
        }
        let p = sea(self.position);
        light(
            vec2(p.x + self.facing * 19., p.y + 1.),
            16.,
            Color::new(1., 0.9, 0.6, 0.55),
        );
    }

    fn draw_instruments(&self) {
        let w = IX1 - IX0;
        surface(700., 112., 244., 456.);
        eyebrow("LIFE SUPPORT", IX0, 138., MUTED);
        let low = self.oxygen < 12.;
        let air = if low { CORAL } else { AQUA };
        readout("Air", &format!("{:.1} s", self.oxygen.max(0.)), 168., air);
        meter(IX0, 176., w, 7., self.oxygen / self.max_air(), air);
        let hull = if self.hull < self.max_hull() * 0.35 {
            CORAL
        } else {
            GOLD
        };
        readout(
            "Hull",
            &format!("{:.0} / {:.0}", self.hull.max(0.), self.max_hull()),
            208.,
            hull,
        );
        meter(IX0, 216., w, 7., self.hull / self.max_hull(), hull);
        divider(IX0, 238., w, HAIRLINE);

        eyebrow("CABIN", IX0, 260., MUTED);
        say_right(
            &format!("{} of {}", self.onboard(), self.capacity()),
            IX1,
            260.,
            13.,
            PAPER,
        );
        let cap = self.capacity();
        let gap = 6.;
        let bw = ((w - gap * (cap as f32 - 1.)) / cap as f32).min(44.);
        for i in 0..cap {
            let x = IX0 + i as f32 * (bw + gap);
            if i < self.onboard() {
                rounded_gradient(x, 270., bw, 12., 6., lighten(AQUA, 0.2), darken(AQUA, 0.2));
                rounded(x + 4., 271.5, bw - 8., 1.5, 1., Color::new(1., 1., 1., 0.5));
            } else {
                rounded(x, 270., bw, 12., 6., Color::new(0.02, 0.04, 0.06, 0.6));
                outline(x, 270., bw, 12., 6., 1., Color::new(0.29, 0.91, 0.88, 0.25));
            }
        }
        divider(IX0, 298., w, HAIRLINE);

        eyebrow("MANIFEST", IX0, 320., MUTED);
        for (index, (_, name)) in self.chart().divers.iter().enumerate() {
            let y = 342. + index as f32 * 21.;
            let (state, c) = match self.divers[index] {
                RescueState::Waiting => ("Waiting", MUTED),
                RescueState::Onboard => ("Aboard", AQUA),
                RescueState::Safe => ("Home", GOLD),
            };
            let dot = if self.divers[index] == RescueState::Waiting {
                Color::new(0.4, 0.46, 0.52, 1.)
            } else {
                c
            };
            draw_circle(IX0 + 3., y - 4.5, 3., dot);
            say(name, IX0 + 13., y, 14., PAPER);
            say_right(state, IX1, y, 13., c);
        }
        divider(IX0, 470., w, HAIRLINE);

        keycap(
            "X",
            if self.balanced {
                "Neutral ballast"
            } else {
                "Light ballast"
            },
            IX0,
            480.,
            if self.balanced { AQUA } else { PAPER },
        );
        let sonar_ready = self.sonar_cooldown == 0;
        let used = keycap(
            "C",
            if sonar_ready {
                "Sonar ready"
            } else {
                "Sonar charging"
            },
            IX0,
            506.,
            if sonar_ready { PAPER } else { MUTED },
        );
        if !sonar_ready {
            let x = IX0 + used + 10.;
            meter(
                x,
                514.,
                (IX1 - x).max(0.),
                5.,
                1. - self.sonar_cooldown as f32 / 240.,
                AQUA,
            );
        }
        say(
            &format!(
                "Speed {:.0}   Dive {:02}:{:02}",
                self.velocity.length(),
                self.elapsed / 3600,
                self.elapsed / 60 % 60
            ),
            IX0,
            550.,
            13.,
            MUTED,
        );
    }

    fn draw_overlay(&self) {
        // Scrim so the modal card reads above the live chart.
        rounded(SEA_X, SEA_Y, SEA_W, SEA_H, 6., with_alpha(INK, 0.5));
        card(42., 160., 626., 376., Color::new(0.045, 0.085, 0.125, 0.98));
        rounded(
            82.,
            160.,
            64.,
            3.,
            1.5,
            if self.phase == Phase::Wreck {
                CORAL
            } else {
                AQUA
            },
        );
        let (eyebrow_text, title, accent) = match self.phase {
            Phase::Briefing => ("DIVE BRIEFING", self.chart().name, AQUA),
            Phase::Debrief => ("SORTIE COMPLETE", "Survivors landed", GOLD),
            Phase::Wreck => ("SIGNAL LOST", "The sub is down", CORAL),
            Phase::Complete => ("CAMPAIGN COMPLETE", "All lights home", GOLD),
            Phase::Diving => ("", "", AQUA),
        };
        eyebrow(eyebrow_text, 64., 192., accent);
        eyebrow_right(&format!("SORTIE {} OF 6", self.sortie()), 646., 192., MUTED);
        say_fit(title, 64., 222., 580., 26., PAPER);
        match self.phase {
            Phase::Briefing => self.draw_briefing(),
            Phase::Debrief => self.draw_debrief(),
            Phase::Wreck => {
                let y = say_wrapped(
                    self.failure,
                    64.,
                    256.,
                    580.,
                    17.,
                    Color::new(1., 0.7, 0.7, 1.),
                );
                say_wrapped(
                    "Retrying resets this sortie's rescues and pay. Earlier sorties and refits are kept.",
                    64.,
                    y + 4.,
                    580.,
                    14.,
                    MUTED,
                );
                eyebrow("RECOVERY TUGS", 64., 352., MUTED);
                for i in 0..3 {
                    let x = 64. + i as f32 * 46.;
                    if i < usize::from(self.recovery) {
                        rounded_gradient(
                            x,
                            364.,
                            38.,
                            22.,
                            6.,
                            lighten(GOLD, 0.15),
                            darken(GOLD, 0.2),
                        );
                        say_centered("Tug", x + 19., 380., 12., INK);
                    } else {
                        rounded(x, 364., 38., 22., 6., Color::new(0.02, 0.04, 0.06, 0.6));
                        outline(x, 364., 38., 22., 6., 1., Color::new(1., 1., 1., 0.1));
                    }
                }
                say(
                    &format!(
                        "{} {} left. Each retry costs one.",
                        self.recovery,
                        if self.recovery == 1 { "tug" } else { "tugs" }
                    ),
                    64.,
                    412.,
                    14.,
                    PAPER,
                );
            }
            Phase::Complete => {
                let y = say_wrapped(
                    "27 divers rescued. The deep beacon burns again.",
                    64.,
                    258.,
                    580.,
                    17.,
                    PAPER,
                );
                say_wrapped(&self.summary(), 64., y + 8., 580., 15., AQUA);
            }
            Phase::Diving => {}
        }
        match self.phase {
            Phase::Briefing => button(64., 479., 588., 40., "Space", "Launch sortie", AQUA),
            Phase::Debrief => button(64., 479., 588., 40., "Space", "Next distress call", AQUA),
            Phase::Wreck if self.recovery > 0 => {
                button(64., 479., 588., 40., "Space", "Send recovery tug", GOLD)
            }
            Phase::Wreck | Phase::Complete => {
                let w = keycap_width("R", "Begin a new career");
                keycap(
                    "R",
                    "Begin a new career",
                    64. + (588. - w) * 0.5,
                    489.,
                    PAPER,
                );
            }
            Phase::Diving => {}
        }
    }

    fn draw_briefing(&self) {
        let y = say_wrapped(self.chart().story, 64., 250., 580., 15., PAPER);
        say_wrapped(self.chart().advice, 64., y, 580., 14., MUTED);
        eyebrow("REFIT BAY", 64., 318., MUTED);
        chip_right(&format!("{} credits", self.credits), 652., 304., 12., GOLD);
        for index in 0..3 {
            let x = 64. + index as f32 * 204.;
            let (y, w, h) = (335., 188., 100.);
            let selected = self.selected == index;
            let level = self.upgrades[index];
            let cost = self.upgrade_cost(index);
            let maxed = level >= 2;
            if selected {
                rounded_gradient(
                    x,
                    y,
                    w,
                    h,
                    10.,
                    Color::new(0.1, 0.25, 0.3, 1.),
                    Color::new(0.06, 0.16, 0.2, 1.),
                );
                outline(x, y, w, h, 10., 1.5, with_alpha(AQUA, 0.85));
            } else {
                rounded_gradient(
                    x,
                    y,
                    w,
                    h,
                    10.,
                    Color::new(0.08, 0.14, 0.19, 1.),
                    Color::new(0.05, 0.1, 0.14, 1.),
                );
                outline(x, y, w, h, 10., 1., HAIRLINE);
            }
            keycap(&(index + 1).to_string(), "", x + 12., y + 12., PAPER);
            say(UPGRADE_NAMES[index], x + 42., y + 27., 15., PAPER);
            say_wrapped(UPGRADE_HELP[index], x + 12., y + 56., w - 24., 13., MUTED);
            for pip in 0..2u8 {
                let px = x + 12. + f32::from(pip) * 18.;
                if pip < level {
                    rounded(px, y + 80., 14., 6., 3., AQUA);
                } else {
                    rounded(px, y + 80., 14., 6., 3., Color::new(1., 1., 1., 0.1));
                }
            }
            if maxed {
                chip_right("Max", x + w - 12., y + 74., 11., MUTED);
            } else {
                let c = if self.credits >= cost { GOLD } else { CORAL };
                chip_right(&format!("{cost} cr"), x + w - 12., y + 74., 11., c);
            }
        }
        let mut x = 64.;
        x += keycap("1-3", "Buy refit", x, 448., MUTED) + 20.;
        x += keycap("Left/Right", "Select", x, 448., MUTED) + 20.;
        keycap("X", "Buy selected", x, 448., MUTED);
    }

    fn draw_debrief(&self) {
        let left = 64. + eyebrow("CAREER MEDALS", 64., 438., MUTED) + 24.;
        for (i, &m) in self.medals.iter().enumerate() {
            let c = vec2(left + i as f32 * 26., 434.);
            if i <= self.mission && m > 0 {
                let metal = medal_metal(m);
                shapes::fill_circle(
                    c,
                    9.,
                    Fill::radial(
                        c - vec2(2., 2.),
                        11.,
                        lighten(metal, 0.45),
                        darken(metal, 0.25),
                    ),
                );
            } else {
                shapes::stroke_circle(c, 8., 1., Color::new(1., 1., 1., 0.14));
            }
        }
        let medal = usize::from(self.medals[self.mission]);
        let tiles: [(&str, String, Color); 3] = [
            ("LANDED", self.delivered().to_string(), AQUA),
            ("CREDITS", self.credits.to_string(), GOLD),
            ("TUGS LEFT", self.recovery.to_string(), PAPER),
        ];
        let tile_w = 136.;
        for (i, (label, value, c)) in tiles.iter().enumerate() {
            let x = 64. + i as f32 * (tile_w + 12.);
            rounded(x, 250., tile_w, 82., 10., Color::new(1., 1., 1., 0.04));
            outline(x, 250., tile_w, 82., 10., 1., HAIRLINE);
            eyebrow(label, x + 16., 274., MUTED);
            say(value, x + 16., 314., 30., *c);
        }
        // Medal tile.
        let x = 64. + 3. * (tile_w + 12.);
        let w = 652. - x;
        rounded(x, 250., w, 82., 10., Color::new(1., 1., 1., 0.04));
        outline(x, 250., w, 82., 10., 1., HAIRLINE);
        eyebrow("MEDAL", x + 16., 274., MUTED);
        let name = ["None", "Bronze", "Silver", "Gold"][medal.min(3)];
        let metal = medal_metal(self.medals[self.mission]);
        let c = vec2(x + 30., 304.);
        draw_triangle(
            c + vec2(-7., -14.),
            c + vec2(-1., -14.),
            c + vec2(-4., 0.),
            darken(AQUA, 0.3),
        );
        draw_triangle(
            c + vec2(7., -14.),
            c + vec2(1., -14.),
            c + vec2(4., 0.),
            darken(AQUA, 0.3),
        );
        shapes::fill_circle(
            c,
            11.,
            Fill::radial(
                c - vec2(3., 3.),
                14.,
                lighten(metal, 0.45),
                darken(metal, 0.25),
            ),
        );
        shapes::stroke_circle(c, 7.5, 1., with_alpha(darken(metal, 0.4), 0.8));
        say(name, x + 50., 312., 20., metal);
        say_wrapped(
            "Credits carry into the refit bay before the next sortie. Recovery tugs are kept for later wrecks.",
            64.,
            370.,
            580.,
            14.,
            MUTED,
        );
    }
}
