use crate::{
    Game, Phase,
    content::Role,
    model::{FEET, PAD_HALF},
};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;
use ferravex_render::effects::{self, EffectBlend, EffectStyle, RibbonTriangle, RibbonVertex};

const GOLD: Color = Color::new(0.96, 0.72, 0.33, 1.);
const BLUE: Color = Color::new(0.38, 0.72, 0.97, 1.);
const GREEN: Color = Color::new(0.45, 0.86, 0.65, 1.);
const PURPLE: Color = Color::new(0.75, 0.58, 0.94, 1.);
const RED: Color = Color::new(0.98, 0.40, 0.38, 1.);
const HAIRLINE: Color = Color::new(1., 1., 1., 0.09);
/// The flight window: simulation metres map one-to-one onto this rectangle.
const SKY_X: f32 = 28.;
const SKY_Y: f32 = 170.;
const SKY_W: f32 = 640.;
const SKY_H: f32 = 360.;
const SKY_BOTTOM: f32 = SKY_Y + SKY_H;
/// Instrument card content columns.
const IX0: f32 = 706.;
const IX1: f32 = 926.;

fn point(p: Vec2) -> Vec2 {
    p + vec2(SKY_X, SKY_Y)
}
fn mix(a: Color, b: Color, t: f32) -> Color {
    shapes::mix(a, b, t)
}
fn scale(c: Color, k: f32) -> Color {
    Color::new(c.r * k, c.g * k, c.b * k, c.a)
}
fn role_color(role: Role) -> Color {
    match role {
        Role::Depot => BLUE,
        Role::Pickup => GOLD,
        Role::Delivery => GREEN,
        Role::Outpost => PURPLE,
    }
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
/// Flat wrapped paragraph of at most `lines` lines; returns the next baseline.
fn say_wrapped(
    value: &str,
    x: f32,
    y: f32,
    width: f32,
    size: f32,
    color: Color,
    lines: usize,
) -> f32 {
    let s = ink(size, color);
    let mut baseline = y;
    for line in wrap_text(value, width, s).take(lines) {
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
fn chip_right(label: &str, right: f32, y: f32, size: f32, color: Color) -> f32 {
    let w = chip_width(label, size);
    chip(label, right - w, y, size, color);
    w
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
    let s = ink(18., INK).with_align(TextAlign::Center);
    let label_w = text_width(label, s);
    let key_w = keycap_width(key, "") - 7.;
    let left = x + (w - key_w - 10. - label_w) * 0.5;
    keycap(key, "", left, y + h * 0.5 - 11., PAPER);
    draw_text_styled(
        label,
        left + key_w + 10. + label_w * 0.5,
        s.centered_baseline(y + h * 0.5),
        s,
    );
}
/// A horizontal gauge: recessed track over `[lo, hi]`, a highlighted safe band
/// and a needle at `value`.
#[allow(clippy::too_many_arguments)]
fn gauge(y: f32, lo: f32, hi: f32, safe_lo: f32, safe_hi: f32, value: f32, safe: bool) {
    let w = IX1 - IX0;
    let at = |v: f32| IX0 + ((v - lo) / (hi - lo)).clamp(0., 1.) * w;
    panel(IX0, y, w, 4., Color::new(0.02, 0.03, 0.05, 0.6));
    let (a, b) = (at(safe_lo), at(safe_hi));
    panel(a, y, b - a, 4., Color::new(0.45, 0.86, 0.65, 0.3));
    let x = at(value);
    panel(x - 1.5, y - 3., 3., 10., if safe { GREEN } else { GOLD });
}

impl Game {
    fn color(&self) -> Color {
        let c = self.planet().color;
        Color::new(c[0], c[1], c[2], 1.)
    }
    /// The pad the current objective points at.
    fn target_pad(&self) -> usize {
        self.role_pad(if self.loaded {
            Role::Delivery
        } else {
            Role::Pickup
        })
    }
    /// Decorative far ridge height (world metres from the top).
    fn far_ridge(&self, x: f32) -> f32 {
        let k = self.index as f32;
        235. + 26. * (x * 0.012 + k * 1.9).sin() + 14. * (x * 0.034 + k * 0.7).sin()
    }

    fn draw_chart(&self) {
        let planet = self.color();
        let t = self.ticks as f32;
        surface(16., 112., 662., 456.);
        // Header: world name, cargo, and the physics this world imposes.
        let mut right = 664.;
        let wind = self.wind();
        let wind_label = if wind.abs() < 0.05 {
            "Calm".to_string()
        } else {
            format!("Wind {:+.1}", wind)
        };
        right -= chip_right(
            &wind_label,
            right,
            122.,
            12.,
            if wind.abs() < 0.05 { MUTED } else { BLUE },
        ) + 8.;
        right -= chip_right(
            &format!("Gravity {:.0}", self.planet().gravity),
            right,
            122.,
            12.,
            MUTED,
        );
        say_fit(self.planet().name, 30., 140., right - 46., 20., PAPER);
        say_fit(self.planet().parcel, 30., 160., 620., 13., MUTED);

        // Sky: deep space fading to a planet-tinted horizon haze.
        let space = Color::new(0.02, 0.03, 0.065, 1.);
        let haze = mix(Color::new(0.05, 0.07, 0.12, 1.), planet, 0.3);
        rounded_gradient(SKY_X, SKY_Y, SKY_W, SKY_H, 6., space, haze);
        for i in 0..72 {
            let x = SKY_X + ((i * 83 + 17 + self.index * 29) % 638) as f32;
            let y = SKY_Y + ((i * 47 + 23 + self.index * 13) % 250) as f32;
            let twinkle = if i % 7 == 0 {
                0.55 + 0.45 * ((t + i as f32 * 37.) / 23.).sin()
            } else {
                1.
            };
            let bright = if i % 9 == 0 {
                0.95
            } else {
                0.45 + (i % 4) as f32 * 0.1
            };
            let s = if i % 9 == 0 { 2. } else { 1.2 };
            panel(
                x,
                y,
                s,
                s,
                Color::new(
                    0.75,
                    0.82,
                    1.,
                    bright * twinkle * (1. - y.max(SKY_Y) / 900.),
                ),
            );
        }
        // A neighbouring moon, lit from the upper left.
        let beacon_x = self.planet().beacon[0];
        let moon = vec2(
            SKY_X
                + if beacon_x > 330. {
                    120. + (self.index * 37 % 90) as f32
                } else {
                    470. + (self.index * 29 % 90) as f32
                },
            SKY_Y + 46. + (self.index % 3) as f32 * 12.,
        );
        let tone = Color::new(
            1. - planet.r * 0.5,
            0.85 - planet.g * 0.3,
            0.9 - planet.b * 0.2,
            1.,
        );
        let radius = 18. + (self.index % 4) as f32 * 5.;
        // Ringshadow's rings pass behind, then in front of, its moon.
        let ring = |from: usize| {
            let mut arc = [Vec2::ZERO; 13];
            for (i, p) in arc.iter_mut().enumerate() {
                let a = (from + i) as f32 / 24. * std::f32::consts::TAU;
                *p = moon + vec2(a.cos() * radius * 1.9, a.sin() * radius * 0.45);
            }
            shapes::polyline(
                &arc,
                shapes::Stroke::new(2., Color::new(0.85, 0.8, 0.95, 0.45)),
            );
        };
        if self.index == 5 {
            ring(12);
        }
        shapes::fill_circle(
            moon,
            radius,
            Fill::radial(
                moon - vec2(radius * 0.45, radius * 0.45),
                radius * 1.7,
                scale(tone, 0.75),
                scale(tone, 0.12),
            ),
        );
        if self.index == 5 {
            ring(0);
        }
        // Distant ridge silhouette for depth.
        let far = with_alpha(mix(haze, planet, 0.25), 0.75);
        let far_floor = with_alpha(mix(haze, planet, 0.15), 0.9);
        shade((0..32).flat_map(|i| {
            let xa = i as f32 * 20.;
            let xb = xa + 20.;
            quad(
                (point(vec2(xa, self.far_ridge(xa))), far),
                (point(vec2(xb, self.far_ridge(xb))), far),
                (vec2(SKY_X + xa, SKY_BOTTOM), far_floor),
                (vec2(SKY_X + xb, SKY_BOTTOM), far_floor),
            )
        }));
        // Altitude reference lines.
        for level in [100., 200.] {
            let y = SKY_Y + level;
            let mut x = SKY_X + 6.;
            while x < SKY_X + SKY_W - 6. {
                panel(x, y, 6., 1., Color::new(1., 1., 1., 0.05));
                x += 14.;
            }
        }
        self.draw_terrain(planet);
        self.draw_pads(t);
        self.draw_beacon(t);
        self.draw_ship(t);
        self.draw_lights(t);
        // Flight log below the window.
        panel(SKY_X + 2., SKY_BOTTOM + 9., 3., 11., with_alpha(GOLD, 0.8));
        say_wrapped(
            self.note,
            SKY_X + 12.,
            SKY_BOTTOM + 19.,
            624.,
            13.,
            Color::new(0.84, 0.88, 0.92, 1.),
            2,
        );
    }

    fn draw_terrain(&self, planet: Color) {
        const STEP: f32 = 8.;
        let lit = scale(planet, 0.62);
        let deep = scale(planet, 0.12);
        let edge = lighten(planet, 0.15);
        shade((0..80).flat_map(|i| {
            let xa = i as f32 * STEP;
            let xb = xa + STEP;
            quad(
                (point(vec2(xa, self.ground(xa))), lit),
                (point(vec2(xb, self.ground(xb))), lit),
                (vec2(SKY_X + xa, SKY_BOTTOM), deep),
                (vec2(SKY_X + xb, SKY_BOTTOM), deep),
            )
        }));
        // Strata and a lit rim along the ridge line.
        let strata = with_alpha(scale(planet, 0.25), 0.7);
        for i in 0..80 {
            let xa = i as f32 * STEP;
            let xb = xa + STEP;
            let a = point(vec2(xa, self.ground(xa)));
            let b = point(vec2(xb, self.ground(xb)));
            draw_line(a.x, a.y + 1., b.x, b.y + 1., 2.2, edge);
            if a.y + 22. < SKY_BOTTOM {
                draw_line(a.x, a.y + 20., b.x, b.y + 20., 1., strata);
            }
        }
        let pebble = scale(planet, 0.22);
        for i in 0..21 {
            let x = ((i * 83 + 29) % 620 + 10) as f32;
            let ground = self.ground(x);
            let y = ground + 16. + (i % 3) as f32 * 6.;
            if y < 352. {
                let p = point(vec2(x, y));
                draw_poly(p.x, p.y, 7, 2.5 + (i % 4) as f32, (i * 40) as f32, pebble);
            }
        }
    }

    fn draw_pads(&self, t: f32) {
        let target = self.target_pad();
        let flying = self.phase == Phase::Flight;
        for i in 0..4 {
            let p = point(self.pad(i));
            let role = self.planet().roles[i];
            let c = role_color(role);
            let is_target = flying && i == target;
            if is_target {
                // Guide beam rising from the objective pad.
                let a = 0.12 + 0.05 * (t / 20.).sin();
                let beam = with_alpha(c, a);
                let clear = with_alpha(c, 0.);
                shade(quad(
                    (vec2(p.x - 18., p.y - 150.), clear),
                    (vec2(p.x + 18., p.y - 150.), clear),
                    (vec2(p.x - PAD_HALF + 4., p.y - 4.), beam),
                    (vec2(p.x + PAD_HALF - 4., p.y - 4.), beam),
                ));
                let bob = (t / 12.).sin() * 3.;
                let tip = vec2(p.x, p.y - 30. + bob);
                draw_triangle(tip, tip + vec2(-7., -9.), tip + vec2(7., -9.), c);
            }
            // Footings, deck and role strip.
            panel(
                p.x - PAD_HALF + 6.,
                p.y + 1.,
                PAD_HALF * 2. - 12.,
                7.,
                Color::new(0.16, 0.18, 0.23, 1.),
            );
            rounded_gradient(
                p.x - PAD_HALF,
                p.y - 4.,
                PAD_HALF * 2.,
                6.,
                3.,
                Color::new(0.78, 0.82, 0.88, 1.),
                Color::new(0.42, 0.46, 0.54, 1.),
            );
            panel(p.x - PAD_HALF + 6., p.y - 1.5, PAD_HALF * 2. - 12., 2., c);
            // Marker posts with lamps; the objective's blink.
            for side in [-1., 1.] {
                let x = p.x + side * (PAD_HALF - 2.);
                draw_line(
                    x,
                    p.y - 4.,
                    x,
                    p.y - 15.,
                    2.,
                    Color::new(0.5, 0.55, 0.62, 1.),
                );
                let on = !is_target || (self.ticks / 20).is_multiple_of(2);
                draw_poly(
                    x,
                    p.y - 16.,
                    8,
                    2.6,
                    0.,
                    if on { c } else { darken(c, 0.55) },
                );
            }
            if self.docked == Some(i) {
                outline(
                    p.x - PAD_HALF - 5.,
                    p.y - 26.,
                    PAD_HALF * 2. + 10.,
                    32.,
                    8.,
                    1.,
                    Color::new(1., 1., 1., 0.4),
                );
            }
            // Label and remaining supplies.
            text_centered(role.label(), p.x, p.y + 25., 13., lighten(c, 0.15));
            if role != Role::Delivery {
                let reserve = format!("Fuel {:.0}", self.reserves[i]);
                text_centered(&reserve, p.x, p.y + 42., 11., MUTED);
            }
        }
    }

    fn draw_beacon(&self, t: f32) {
        let logged = self.beacon[self.index];
        let b = point(Vec2::from_array(self.planet().beacon));
        let bob = if logged { 0. } else { (t / 18.).sin() * 2. };
        let c = vec2(b.x, b.y + bob);
        let r = 10.;
        let (top, bottom) = if logged {
            (
                Color::new(0.3, 0.34, 0.4, 0.8),
                Color::new(0.15, 0.18, 0.22, 0.8),
            )
        } else {
            (lighten(GOLD, 0.35), darken(GOLD, 0.35))
        };
        let n = c - vec2(0., r);
        let e = c + vec2(r, 0.);
        let s = c + vec2(0., r);
        let w = c - vec2(r, 0.);
        shade(
            [[n, e, w], [w, e, s]]
                .map(|tri| tri.map(|p| (p, if p.y < c.y + 0.1 { top } else { bottom }))),
        );
        let edge = if logged { MUTED } else { lighten(GOLD, 0.5) };
        for (a, z) in [(n, e), (e, s), (s, w), (w, n)] {
            draw_line(a.x, a.y, z.x, z.y, 1.2, edge);
        }
        let label = if logged { "Logged" } else { "Survey +250" };
        let lw = measure(label, 11.);
        text(
            label,
            b.x - lw * 0.5,
            b.y - 17.,
            11.,
            if logged { MUTED } else { GOLD },
        );
    }

    fn draw_ship(&self, t: f32) {
        let p = point(self.pos);
        // Contact shadow, shrinking with altitude.
        let ground = self.ground(self.pos.x);
        let alt = (ground - self.pos.y - FEET).max(0.);
        let k = (1. - alt / 220.).clamp(0.15, 1.);
        let sh = point(vec2(self.pos.x, ground));
        draw_ellipse(
            sh.x,
            sh.y,
            16. * k,
            3. * k,
            0.,
            Color::new(0., 0., 0., 0.45 * k),
        );
        let rotate = |x: f32, y: f32| {
            p + vec2(
                x * self.angle.cos() - y * self.angle.sin(),
                x * self.angle.sin() + y * self.angle.cos(),
            )
        };
        if self.thrusting {
            let flicker = (self.ticks % 5) as f32 * 1.6;
            let core = Color::new(1., 0.98, 0.85, 0.95);
            let flame = Color::new(1., 0.62, 0.22, 0.85);
            let clear = Color::new(1., 0.4, 0.1, 0.);
            shade([
                [
                    (rotate(-5.5, 9.), flame),
                    (rotate(5.5, 9.), flame),
                    (rotate(0., 27. + flicker), clear),
                ],
                [
                    (rotate(-2.8, 9.), core),
                    (rotate(2.8, 9.), core),
                    (rotate(0., 18. + flicker * 0.5), with_alpha(flame, 0.)),
                ],
            ]);
        }
        if self.braking {
            let jet = Color::new(0.55, 0.85, 1., 0.9);
            let clear = with_alpha(jet, 0.);
            for side in [-1., 1.] {
                shade([[
                    (rotate(side * 9., -1.), jet),
                    (rotate(side * 9., 2.), jet),
                    (rotate(side * 21., -9.), clear),
                ]]);
            }
        }
        let damaged = self.hull < 35.;
        let metal = Color::new(0.55, 0.6, 0.68, 1.);
        // Landing legs and feet.
        for side in [-1., 1.] {
            let a = rotate(side * 6., 3.);
            let b = rotate(side * 12., FEET);
            draw_line(a.x, a.y, b.x, b.y, 2., metal);
            let f0 = rotate(side * 9., FEET);
            let f1 = rotate(side * 16., FEET);
            draw_line(f0.x, f0.y, f1.x, f1.y, 2.4, lighten(metal, 0.2));
        }
        // Engine bell.
        shade(quad(
            (rotate(-3.5, 4.), metal),
            (rotate(3.5, 4.), metal),
            (rotate(-5.5, 9.5), darken(metal, 0.5)),
            (rotate(5.5, 9.5), darken(metal, 0.5)),
        ));
        // Hull: lit nose to shaded skirt.
        let (nose, skirt) = if damaged {
            (
                Color::new(1., 0.75, 0.72, 1.),
                Color::new(0.6, 0.2, 0.22, 1.),
            )
        } else {
            (
                Color::new(0.97, 0.98, 1., 1.),
                Color::new(0.55, 0.6, 0.7, 1.),
            )
        };
        shade([[
            (rotate(0., -14.), nose),
            (rotate(-11., 5.5), skirt),
            (rotate(11., 5.5), skirt),
        ]]);
        let rim0 = rotate(-10., 5.);
        let rim1 = rotate(10., 5.);
        draw_line(rim0.x, rim0.y, rim1.x, rim1.y, 1.6, darken(skirt, 0.35));
        // Cockpit glass.
        let cabin = rotate(0., -3.);
        shapes::fill_circle(
            cabin,
            4.3,
            Fill::radial(
                cabin - vec2(1.4, 1.4),
                5.,
                lighten(BLUE, 0.55),
                darken(BLUE, 0.35),
            ),
        );
        if self.loaded {
            let cargo = rotate(0., 8.);
            shapes::fill_circle(
                cargo,
                3.6,
                Fill::radial(
                    cargo - vec2(1., 1.),
                    4.5,
                    lighten(GOLD, 0.4),
                    darken(GOLD, 0.3),
                ),
            );
        }
        if damaged && (self.ticks / 15).is_multiple_of(2) {
            let lamp = rotate(0., -14.);
            shapes::fill_circle(lamp, 1.8, RED);
        }
        if matches!(self.phase, Phase::Wreck | Phase::Grounded) {
            for k in 0..5 {
                let a = k as f32 * 1.256 + t * 0.002;
                let q = p + vec2(a.cos(), a.sin()) * 18.;
                draw_poly(
                    q.x,
                    q.y,
                    6,
                    2.5,
                    k as f32 * 30.,
                    Color::new(0.4, 0.42, 0.48, 0.9),
                );
            }
            shapes::fill_capsule(p - vec2(13., 13.), p + vec2(13., 13.), 2., RED);
            shapes::fill_capsule(p + vec2(13., -13.), p + vec2(-13., 13.), 2., RED);
        }
    }

    /// All additive lights in one contiguous pass so they share a draw.
    fn draw_lights(&self, t: f32) {
        if self.phase == Phase::Flight {
            let p = point(self.pad(self.target_pad()));
            let c = role_color(self.planet().roles[self.target_pad()]);
            light(
                vec2(p.x, p.y - 6.),
                24.,
                with_alpha(c, 0.35 + 0.1 * (t / 20.).sin()),
            );
        }
        if !self.beacon[self.index] {
            let b = point(Vec2::from_array(self.planet().beacon));
            light(vec2(b.x, b.y), 16., Color::new(1., 0.75, 0.35, 0.35));
        }
        let p = point(self.pos);
        if self.thrusting {
            let a = self.angle;
            let q = p + vec2(-a.sin(), a.cos()) * 14.;
            light(vec2(q.x, q.y), 18., Color::new(1., 0.6, 0.25, 0.7));
        }
        if matches!(self.phase, Phase::Wreck | Phase::Grounded) {
            light(vec2(p.x, p.y), 30., Color::new(1., 0.3, 0.25, 0.5));
        }
    }

    fn draw_instruments(&self) {
        let w = IX1 - IX0;
        surface(688., 112., 256., 456.);
        eyebrow("FLIGHT", IX0, 138., MUTED);
        let fuel_low = self.fuel < self.capacity() * 0.2;
        let fuel_c = if fuel_low { GOLD } else { BLUE };
        readout(
            "Fuel",
            &format!("{:.0} / {:.0}", self.fuel, self.capacity()),
            166.,
            fuel_c,
        );
        meter(IX0, 174., w, 7., self.fuel / self.capacity(), fuel_c);
        let hull_c = if self.hull < 35. { RED } else { GREEN };
        readout("Hull", &format!("{:.0}%", self.hull), 204., hull_c);
        meter(IX0, 212., w, 7., self.hull / 100., hull_c);
        if self.loaded {
            readout("Parcel", &format!("{:.0}%", self.cargo), 242., GOLD);
            meter(IX0, 250., w, 7., self.cargo / 100., GOLD);
        } else {
            say("Parcel", IX0, 242., 14., MUTED);
            chip_right("Empty hold", IX1, 229., 12., MUTED);
        }
        divider(IX0, 272., w, HAIRLINE);

        let (vx, vy, angle) = self.limits();
        eyebrow("APPROACH", IX0, 294., MUTED);
        eyebrow_right("SAFE LIMIT", IX1, 294., MUTED);
        let tilt = self.angle.to_degrees();
        let rows = [
            (
                "Side",
                format!("{:+.1}", self.velocity.x),
                vx,
                self.velocity.x.abs() <= vx,
            ),
            (
                "Down",
                format!("{:+.1}", self.velocity.y),
                vy,
                self.velocity.y >= 0. && self.velocity.y <= vy,
            ),
            (
                "Tilt",
                format!("{:+.0}", tilt),
                angle.to_degrees(),
                self.angle.abs() <= angle,
            ),
        ];
        for (i, (label, value, limit, safe)) in rows.iter().enumerate() {
            let y = 318. + i as f32 * 34.;
            let c = if *safe { GREEN } else { GOLD };
            say(label, IX0, y, 14., MUTED);
            say_right(value, IX1 - 52., y, 17., c);
            say_right(&format!("{limit:.0}"), IX1, y, 13., MUTED);
            let value = match i {
                0 => self.velocity.x,
                1 => self.velocity.y,
                _ => tilt,
            };
            if i == 1 {
                gauge(y + 7., -limit, limit * 2.5, 0., *limit, value, *safe);
            } else {
                gauge(
                    y + 7.,
                    -limit * 2.5,
                    limit * 2.5,
                    -limit,
                    *limit,
                    value,
                    *safe,
                );
            }
        }
        divider(IX0, 412., w, HAIRLINE);

        let left = self.planet().deadline.saturating_sub(self.ticks / 60);
        let urgent = self.ticks / 60 + 30 > self.planet().deadline;
        readout(
            "Storm in",
            &format!("{left} s"),
            438.,
            if urgent { RED } else { PAPER },
        );
        meter(
            IX0,
            446.,
            w,
            6.,
            left as f32 / self.planet().deadline as f32,
            if urgent {
                RED
            } else {
                Color::new(0.6, 0.67, 0.74, 1.)
            },
        );
        say(&format!("{} credits", self.credits), IX0, 476., 13., MUTED);
        say_right(
            &format!(
                "{} {}",
                self.permits,
                if self.permits == 1 {
                    "permit"
                } else {
                    "permits"
                }
            ),
            IX1,
            476.,
            13.,
            MUTED,
        );
        divider(IX0, 488., w, HAIRLINE);
        let (key, action, color) = if let Some(pad) = self.docked {
            match self.planet().roles[pad] {
                Role::Pickup if !self.loaded => ("X", "Load parcel", GOLD),
                Role::Delivery if self.loaded => ("X", "Deliver parcel", GREEN),
                _ => ("X", "Service pad", PAPER),
            }
        } else if self.braking {
            ("Down", "Retro-brake active", BLUE)
        } else {
            ("Down", "Retro-brake to land", PAPER)
        };
        keycap(key, action, IX0, 500., color);
        say(
            &format!(
                "Engine {}  Tank {}  Gear {}",
                self.upgrades[0], self.upgrades[1], self.upgrades[2]
            ),
            IX0,
            550.,
            13.,
            MUTED,
        );
    }

    fn draw_card(&self) {
        let (x0, x1) = (132., 640.);
        // Scrim so the modal card reads above the live chart.
        rounded(SKY_X, SKY_Y, SKY_W, SKY_H, 6., with_alpha(INK, 0.55));
        card(100., 194., 572., 344., Color::new(0.05, 0.065, 0.1, 0.98));
        let accent = match self.phase {
            Phase::Debrief => GREEN,
            Phase::Wreck => RED,
            _ => GOLD,
        };
        rounded(150., 194., 64., 3., 1.5, accent);
        let (tag, title) = match self.phase {
            Phase::Briefing => ("ROUTE BRIEFING", self.planet().name),
            Phase::Debrief => ("PARCEL DELIVERED", "Refit desk"),
            Phase::Wreck => ("ROUTE INTERRUPTED", "Recovery needed"),
            _ => ("", ""),
        };
        eyebrow(tag, x0, 224., accent);
        eyebrow_right(
            &format!("CONTRACT {} OF 8", self.contract()),
            x1,
            224.,
            MUTED,
        );
        say_fit(title, x0, 254., 340., 26., PAPER);
        match self.phase {
            Phase::Briefing => {
                let mut x = x0;
                let wind = self.planet().wind;
                for (label, c) in [
                    (format!("Gravity {:.0}", self.planet().gravity), MUTED),
                    (
                        if wind == 0. && self.planet().gust == 0. {
                            "Calm".to_string()
                        } else {
                            format!("Wind {:+.0}", wind)
                        },
                        BLUE,
                    ),
                    (format!("Storm in {} s", self.planet().deadline), RED),
                ] {
                    x += chip(&label, x, 268., 12., c) + 8.;
                }
                say_wrapped(self.planet().briefing, x0, 316., x1 - x0, 15., PAPER, 2);
                rounded(x0, 346., x1 - x0, 66., 10., Color::new(1., 1., 1., 0.04));
                outline(x0, 346., x1 - x0, 66., 10., 1., HAIRLINE);
                let mid = x0 + (x1 - x0) * 0.5;
                eyebrow("PARCEL", x0 + 16., 370., MUTED);
                say_fit(
                    self.planet().parcel,
                    x0 + 16.,
                    396.,
                    mid - x0 - 28.,
                    15.,
                    GOLD,
                );
                eyebrow("DELIVER TO", mid + 4., 370., MUTED);
                say_fit(
                    self.planet().recipient,
                    mid + 4.,
                    396.,
                    x1 - mid - 20.,
                    15.,
                    GREEN,
                );
                let mut x = x0;
                for role in [Role::Depot, Role::Pickup, Role::Outpost, Role::Delivery] {
                    let c = role_color(role);
                    rounded(x, 437., 14., 5., 2.5, c);
                    x += 20. + say(role.label(), x + 20., 444., 13., MUTED) + 22.;
                }
                button(x0, 486., x1 - x0, 41., "Space", "Open route", GOLD);
            }
            Phase::Debrief => {
                // Stamps earned for this delivery.
                let stamps = self.stamps[self.index];
                for k in 0..3u8 {
                    let x = x1 - 3. * 26. + f32::from(k) * 26.;
                    if k < stamps {
                        rounded_gradient(
                            x,
                            236.,
                            20.,
                            20.,
                            4.,
                            lighten(GREEN, 0.25),
                            darken(GREEN, 0.25),
                        );
                        outline(
                            x + 3.,
                            239.,
                            14.,
                            14.,
                            2.,
                            1.,
                            Color::new(0.04, 0.2, 0.1, 0.6),
                        );
                    } else {
                        outline(x, 236., 20., 20., 4., 1., Color::new(1., 1., 1., 0.18));
                    }
                }
                chip_right(
                    &format!("{} credits", self.credits),
                    x1 - 3. * 26. - 10.,
                    238.,
                    12.,
                    GOLD,
                );
                for (i, (name, detail)) in [
                    ("Engine", "More thrust; less main-engine fuel burn"),
                    ("Tank", "30 additional fuel units per level"),
                    ("Landing gear", "Wider safe speeds; less impact damage"),
                ]
                .iter()
                .enumerate()
                {
                    let y = 282. + i as f32 * 58.;
                    let level = self.upgrades[i];
                    let cost = self.upgrade_cost(i);
                    rounded_gradient(
                        x0,
                        y,
                        x1 - x0,
                        50.,
                        10.,
                        Color::new(0.1, 0.13, 0.18, 1.),
                        Color::new(0.07, 0.09, 0.13, 1.),
                    );
                    outline(x0, y, x1 - x0, 50., 10., 1., HAIRLINE);
                    keycap(&(i + 1).to_string(), "", x0 + 14., y + 15., PAPER);
                    say(name, x0 + 46., y + 22., 15., PAPER);
                    say_fit(detail, x0 + 46., y + 40., 300., 12., MUTED);
                    for pip in 0..2u8 {
                        let c = if pip < level {
                            BLUE
                        } else {
                            Color::new(1., 1., 1., 0.1)
                        };
                        rounded(x1 - 150. + f32::from(pip) * 18., y + 22., 14., 6., 3., c);
                    }
                    if level >= 2 {
                        chip_right("Max", x1 - 14., y + 16., 12., MUTED);
                    } else {
                        let c = if self.credits >= cost { GOLD } else { RED };
                        chip_right(&format!("{cost} cr"), x1 - 14., y + 16., 12., c);
                    }
                }
                button(x0, 486., x1 - x0, 41., "Space", "Next contract", GREEN);
            }
            Phase::Wreck => {
                let y = say_wrapped(
                    self.note,
                    x0,
                    290.,
                    x1 - x0,
                    17.,
                    Color::new(1., 0.72, 0.7, 1.),
                    2,
                );
                say_wrapped(
                    "Recovery resets this contract's cargo, supplies, beacon and earnings. Earlier deliveries and refits are kept.",
                    x0,
                    y + 6.,
                    x1 - x0,
                    14.,
                    MUTED,
                    3,
                );
                eyebrow("RECOVERY PERMITS", x0, 408., MUTED);
                for i in 0..3u8 {
                    let x = x0 + f32::from(i) * 72.;
                    if i < self.permits {
                        rounded_gradient(
                            x,
                            420.,
                            64.,
                            24.,
                            6.,
                            lighten(GOLD, 0.15),
                            darken(GOLD, 0.2),
                        );
                        let s = ink(13., INK).with_align(TextAlign::Center);
                        draw_text_styled("Permit", x + 32., s.centered_baseline(432.), s);
                    } else {
                        rounded(x, 420., 64., 24., 6., Color::new(0.02, 0.03, 0.05, 0.6));
                        outline(x, 420., 64., 24., 6., 1., HAIRLINE);
                    }
                }
                button(
                    x0,
                    486.,
                    x1 - x0,
                    41.,
                    "Space",
                    "Recover this contract",
                    GOLD,
                );
            }
            _ => {}
        }
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
        self.draw_chart();
        self.draw_instruments();
        if matches!(self.phase, Phase::Briefing | Phase::Debrief | Phase::Wreck) {
            self.draw_card();
        }
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!("Contract {} / 8", self.contract()),
            objective: if self.loaded {
                format!("Deliver at green: {}.", self.planet().recipient)
            } else {
                "Pick up at the yellow pad.".to_string()
            },
            status: format!(
                "{} delivered / {} stamps / {} permits",
                self.delivered(),
                self.stamps.iter().map(|&s| u32::from(s)).sum::<u32>(),
                self.permits
            ),
            score: self.score,
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
}
