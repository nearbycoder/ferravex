//! The live chart: sky, weather, terrain, pads, beacon, ship and in-world
//! landing guidance, plus the `ArcadeGame` wiring. Drawing only reads the model.
use crate::{
    Game, Phase,
    content::Role,
    model::{FEET, HALF_BODY, PAD_HALF},
};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;
use ferravex_render::effects::{self, EffectBlend, EffectStyle, RibbonTriangle, RibbonVertex};
use std::cell::Cell;
use std::f32::consts::TAU;

pub(crate) const GOLD: Color = Color::new(0.96, 0.72, 0.33, 1.);
pub(crate) const BLUE: Color = Color::new(0.38, 0.72, 0.97, 1.);
pub(crate) const GREEN: Color = Color::new(0.45, 0.86, 0.65, 1.);
pub(crate) const PURPLE: Color = Color::new(0.75, 0.58, 0.94, 1.);
pub(crate) const RED: Color = Color::new(0.98, 0.40, 0.38, 1.);
pub(crate) const HAIRLINE: Color = Color::new(1., 1., 1., 0.09);
pub(crate) const DUST: Color = Color::new(0.8, 0.76, 0.7, 0.85);
/// The flight window: simulation metres map one-to-one onto this rectangle.
pub(crate) const SKY_X: f32 = 28.;
pub(crate) const SKY_Y: f32 = 124.;
pub(crate) const SKY_W: f32 = 640.;
const SKY_H: f32 = 360.;
/// Bedrock below the simulated chart is drawn down to this floor.
pub(crate) const FLOOR: f32 = 556.;

pub(crate) fn point(p: Vec2) -> Vec2 {
    p + vec2(SKY_X, SKY_Y)
}
pub(crate) fn mix(a: Color, b: Color, t: f32) -> Color {
    shapes::mix(a, b, t)
}
fn scale(c: Color, k: f32) -> Color {
    Color::new(c.r * k, c.g * k, c.b * k, c.a)
}
pub(crate) fn role_color(role: Role) -> Color {
    match role {
        Role::Depot => BLUE,
        Role::Pickup => GOLD,
        Role::Delivery => GREEN,
        Role::Outpost => PURPLE,
    }
}
pub(crate) fn role_icon(role: Role) -> Icon {
    match role {
        Role::Depot => Icon::Home,
        Role::Pickup => Icon::Crate,
        Role::Delivery => Icon::Flag,
        Role::Outpost => Icon::Fuel,
    }
}
/// Deterministic 0..1 variety for presentation details (never rules).
fn noise(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65535.
}
/// Smoothstep-shaped 0..1 entrance curve.
pub(crate) fn ease(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    1. - (1. - t) * (1. - t) * (1. - t)
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
pub(crate) fn shade(triangles: impl IntoIterator<Item = [Vertex; 3]>) {
    emit(EffectBlend::Alpha, triangles);
}
/// Unit circle in 24 steps, computed once.
fn unit_ring() -> &'static [Vec2; 25] {
    static RING: std::sync::OnceLock<[Vec2; 25]> = std::sync::OnceLock::new();
    RING.get_or_init(|| {
        std::array::from_fn(|i| {
            let a = i as f32 * TAU / 24.;
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
pub(crate) fn quad(a: Vertex, b: Vertex, c: Vertex, d: Vertex) -> [[Vertex; 3]; 2] {
    [[a, b, c], [c, b, d]]
}

/// Flat text for card surfaces, where a drop shadow would be invisible and
/// would double the glyph geometry.
pub(crate) fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
pub(crate) fn say(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(size, color)).width
}
/// Width of a key cap drawn by [`keycap`] without an action label.
pub(crate) fn keycap_width(key: &str) -> f32 {
    (text_width(key, TextStyle::modern(13., PAPER)) + 12.).max(20.)
}
/// [`icon_value`]'s width for the same arguments.
pub(crate) fn icon_value_width(value: &str, size: f32, color: Color) -> f32 {
    size * 1.25 + text_width(value, style(size, color))
}
/// Four L-shaped corner marks around a rectangle.
pub(crate) fn corners(l: f32, t: f32, r: f32, b: f32, arm: f32, width: f32, color: Color) {
    for (x, y, dx, dy) in [
        (l, t, 1., 1.),
        (r, t, -1., 1.),
        (l, b, 1., -1.),
        (r, b, -1., -1.),
    ] {
        draw_line(x, y, x + dx * arm, y, width, color);
        draw_line(x, y, x, y + dy * arm, width, color);
    }
}
/// A small lander glyph (nose up when `angle` is zero) for instruments.
pub(crate) fn lander(at: Vec2, size: f32, angle: f32, color: Color) {
    let k = size / 16.;
    let (s, c) = angle.sin_cos();
    let r = |x: f32, y: f32| at + vec2(x * c - y * s, x * s + y * c) * k;
    shapes::fill_triangle(r(0., -8.), r(-6., 3.), r(6., 3.), color);
    for side in [-1., 1.] {
        let a = r(side * 3., 2.);
        let b = r(side * 7., 7.);
        draw_line(a.x, a.y, b.x, b.y, 1.6 * k, color);
        let f = r(side * 9., 7.);
        draw_line(b.x, b.y, f.x, f.y, 1.6 * k, color);
    }
}

/// Where a touchdown right now would end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Landing {
    /// Both feet on a pad, slow and upright enough.
    Safe,
    /// Over a pad but too fast or tilted.
    Risky,
    /// Rough ground or a pad edge.
    Rough,
}

thread_local! {
    /// The phase and contract last drawn and the presentation time it first
    /// appeared: card entrances only, never rules.
    static SHOWN: Cell<(Phase, usize, f32)> = const { Cell::new((Phase::Flight, usize::MAX, 0.)) };
}

impl Game {
    fn color(&self) -> Color {
        let c = self.planet().color;
        Color::new(c[0], c[1], c[2], 1.)
    }
    /// The pad the current objective points at.
    pub(crate) fn target_pad(&self) -> usize {
        self.role_pad(if self.loaded {
            Role::Delivery
        } else {
            Role::Pickup
        })
    }
    /// Whole seconds before the storm closes this route.
    pub(crate) fn seconds_left(&self) -> u32 {
        self.planet().deadline.saturating_sub(self.ticks / 60)
    }
    /// Presentation seconds since this phase of this contract was first drawn.
    pub(crate) fn phase_age(&self) -> f32 {
        let now = clock();
        SHOWN.with(|cell| {
            let (phase, index, since) = cell.get();
            if phase == self.phase && index == self.index {
                now - since
            } else {
                cell.set((self.phase, self.index, now));
                0.
            }
        })
    }
    /// The contact height under the ship's footprint, as the rules test it.
    fn surface_under(&self, x: f32) -> f32 {
        self.ground(x - HALF_BODY)
            .min(self.ground(x))
            .min(self.ground(x + HALF_BODY))
    }
    /// Height of the feet above the surface below.
    pub(crate) fn altitude(&self) -> f32 {
        self.surface_under(self.pos.x) - self.pos.y - FEET
    }
    /// Sideways speed, downward speed and attitude against the landing limits.
    pub(crate) fn within_limits(&self) -> (bool, bool, bool) {
        let (vx, vy, tilt) = self.limits();
        (
            self.velocity.x.abs() <= vx,
            self.velocity.y >= 0. && self.velocity.y <= vy,
            self.angle.abs() <= tilt,
        )
    }
    /// The verdict a touchdown at the current position would receive.
    pub(crate) fn landing(&self) -> Landing {
        let surface = self.surface_under(self.pos.x);
        let on_pad = (0..4).any(|i| {
            let p = self.pad(i);
            (self.pos.x - p.x).abs() <= PAD_HALF - HALF_BODY && (p.y - surface).abs() < 0.1
        });
        match self.within_limits() {
            _ if !on_pad => Landing::Rough,
            (true, true, true) => Landing::Safe,
            _ => Landing::Risky,
        }
    }
    /// What X would do on the current pad, as an icon and colour.
    pub(crate) fn dock_action(&self) -> Option<(Icon, Color)> {
        let pad = self.docked?;
        let role = self.planet().roles[pad];
        if role == Role::Delivery && self.loaded {
            Some((Icon::Flag, GREEN))
        } else if role == Role::Pickup && !self.loaded {
            Some((Icon::Crate, GOLD))
        } else if role == Role::Outpost && !self.repaired[pad] && self.hull < 100. {
            Some((Icon::Wrench, GREEN))
        } else if self.reserves[pad] > 0.01 && self.capacity() - self.fuel > 0.01 {
            Some((Icon::Fuel, BLUE))
        } else {
            None
        }
    }
    /// Decorative far ridge height (world metres from the top).
    fn far_ridge(&self, x: f32) -> f32 {
        let k = self.index as f32;
        235. + 26. * (x * 0.012 + k * 1.9).sin() + 14. * (x * 0.034 + k * 0.7).sin()
    }

    pub(crate) fn draw_chart(&self) {
        let planet = self.color();
        let now = clock();
        surface(16., 112., 662., 456.);
        // Sky: deep space fading to a planet-tinted horizon haze.
        let space = Color::new(0.02, 0.03, 0.065, 1.);
        let haze = mix(Color::new(0.05, 0.07, 0.12, 1.), planet, 0.3);
        rounded_gradient(SKY_X, SKY_Y, SKY_W, SKY_H, 8., space, haze);
        self.draw_stars(now);
        self.draw_moon(planet);
        // Distant ridge silhouette for depth.
        let far = with_alpha(mix(haze, planet, 0.25), 0.75);
        let far_floor = with_alpha(mix(haze, planet, 0.15), 0.9);
        shade((0..32).flat_map(|i| {
            let xa = i as f32 * 20.;
            let xb = xa + 20.;
            quad(
                (point(vec2(xa, self.far_ridge(xa))), far),
                (point(vec2(xb, self.far_ridge(xb))), far),
                (vec2(SKY_X + xa, FLOOR), far_floor),
                (vec2(SKY_X + xb, FLOOR), far_floor),
            )
        }));
        self.draw_storm(now);
        self.draw_wind(now);
        self.draw_terrain(planet);
        if self.phase == Phase::Briefing {
            self.draw_route(now);
        }
        self.draw_pads(now);
        self.draw_beacon(now);
        self.draw_guide();
        self.draw_dust(planet);
        self.draw_ship(now);
        self.draw_lights(now);
        self.draw_prompt(now);
    }

    fn draw_stars(&self, now: f32) {
        for i in 0..72 {
            let x = SKY_X + ((i * 83 + 17 + self.index * 29) % 638) as f32;
            let y = SKY_Y + ((i * 47 + 23 + self.index * 13) % 250) as f32;
            let twinkle = if i % 7 == 0 {
                0.55 + 0.45 * (now * 2.6 + i as f32 * 1.6).sin()
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
                Color::new(0.75, 0.82, 1., bright * twinkle * (1. - y / 900.)),
            );
        }
    }

    /// A neighbouring moon, lit from the upper left.
    fn draw_moon(&self, planet: Color) {
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
                let a = (from + i) as f32 / 24. * TAU;
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
    }

    /// The deadline made visible: during the final minute the sky darkens and
    /// a rolling cloud bank lowers from the top edge, with lightning in the
    /// last half minute.
    fn draw_storm(&self, now: f32) {
        if !matches!(self.phase, Phase::Flight | Phase::Wreck) {
            return;
        }
        let left = self.planet().deadline as f32 - self.ticks as f32 / 60.;
        let u = (1. - left / 60.).clamp(0., 1.);
        if u <= 0. {
            return;
        }
        rounded(
            SKY_X,
            SKY_Y,
            SKY_W,
            SKY_H,
            8.,
            Color::new(0.05, 0.02, 0.1, 0.3 * u),
        );
        let depth = 16. + 74. * u;
        let base = |x: f32| {
            SKY_Y
                + 6.
                + depth
                    * (0.75
                        + 0.17 * (x * 0.031 + now * 0.5).sin()
                        + 0.08 * (x * 0.083 - now * 0.8).sin())
        };
        let a = u.sqrt();
        let high = Color::new(0.09, 0.07, 0.15, 0.96 * a);
        let low = Color::new(0.25, 0.21, 0.36, 0.92 * a);
        let (l, r, top) = (SKY_X + 6., SKY_X + SKY_W - 6., SKY_Y + 3.);
        const STEPS: usize = 40;
        let x_at = |i: usize| l + (r - l) * i as f32 / STEPS as f32;
        shade((0..STEPS).flat_map(|i| {
            let (xa, xb) = (x_at(i), x_at(i + 1));
            quad(
                (vec2(xa, top), high),
                (vec2(xb, top), high),
                (vec2(xa, base(xa)), low),
                (vec2(xb, base(xb)), low),
            )
        }));
        // Billows bulge from the underside.
        for k in 0..15u32 {
            let x = l + 22. + k as f32 * 42. + (now * 0.6 + k as f32).sin() * 4.;
            let radius = depth * (0.14 + 0.12 * noise(k + 40));
            shapes::fill_sector(
                vec2(x, base(x) - 0.5),
                radius,
                0.,
                std::f32::consts::PI,
                low,
            );
        }
        let strike = (self.ticks / 4) % 41;
        if u > 0.5 && strike < 2 {
            let seed = self.ticks / 164;
            let x = l + 40. + noise(seed) * (r - l - 80.);
            let mut bolt = [Vec2::ZERO; 6];
            let mut at = vec2(x, base(x));
            for (i, p) in bolt.iter_mut().enumerate() {
                *p = at;
                at += vec2((noise(seed * 7 + i as u32) - 0.5) * 26., 24.);
            }
            shapes::polyline(
                &bolt,
                shapes::Stroke::new(2.2, Color::new(0.92, 0.92, 1., 0.9)),
            );
            rounded(
                SKY_X,
                SKY_Y,
                SKY_W,
                SKY_H,
                8.,
                Color::new(0.8, 0.8, 1., 0.06),
            );
        }
    }

    /// Wind made visible: faint streaks drift with the same gusting wind the
    /// ship feels; their speed, length and brightness follow its strength.
    fn draw_wind(&self, now: f32) {
        let planet = self.planet();
        if planet.wind == 0. && planet.gust == 0. {
            return;
        }
        let wind = self.wind();
        let strength = (wind.abs() / 6.).clamp(0., 1.);
        if strength < 0.03 {
            return;
        }
        let tau = self.ticks as f32;
        // Integral of wind over flight time, plus a slow idle drift.
        let drift = (planet.wind * tau - 170. * planet.gust * (tau / 170.).cos()) / 60. * 9.
            + now * 5. * planet.wind.signum();
        let dir = wind.signum();
        let len = 10. + 26. * strength;
        let span = SKY_W - 16.;
        for i in 0..18u32 {
            let y = SKY_Y + 16. + ((i as usize * 61 + self.index * 23) % 236) as f32;
            let speed = 0.7 + 0.6 * noise(i + 11);
            let head = SKY_X + 8. + (i as f32 * 151. + drift * speed).rem_euclid(span);
            let tail = (head - dir * len).clamp(SKY_X + 8., SKY_X + SKY_W - 8.);
            let a = (0.05 + 0.22 * strength) * (0.6 + 0.4 * noise(i + 3));
            let lit = Color::new(0.8, 0.88, 1., a);
            let gone = with_alpha(lit, 0.);
            shade(quad(
                (vec2(head, y - 0.7), lit),
                (vec2(tail, y - 0.7), gone),
                (vec2(head, y + 0.7), lit),
                (vec2(tail, y + 0.7), gone),
            ));
        }
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
                (vec2(SKY_X + xa, FLOOR), deep),
                (vec2(SKY_X + xb, FLOOR), deep),
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
            draw_line(a.x, a.y + 20., b.x, b.y + 20., 1., strata);
            draw_line(a.x, a.y + 52., b.x, b.y + 52., 1., with_alpha(strata, 0.4));
        }
        let pebble = scale(planet, 0.22);
        for i in 0..28 {
            let x = ((i * 83 + 29) % 620 + 10) as f32;
            let y = self.ground(x) + 16. + (i % 5) as f32 * 11.;
            let p = point(vec2(x, y));
            if p.y < FLOOR - 12. {
                draw_poly(p.x, p.y, 7, 2.5 + (i % 4) as f32, (i * 40) as f32, pebble);
            }
        }
    }

    /// Briefing preview: each leg climbs above the highest ground between its
    /// pads, crosses, then descends onto the next pad.
    fn draw_route(&self, now: f32) {
        let mut first: Option<(f32, f32, f32)> = None;
        for (k, (from, to, c)) in [
            (Role::Depot, Role::Pickup, GOLD),
            (Role::Pickup, Role::Delivery, GREEN),
        ]
        .into_iter()
        .enumerate()
        {
            let a = self.pad(self.role_pad(from));
            let b = self.pad(self.role_pad(to));
            let (lo, hi) = (a.x.min(b.x), a.x.max(b.x));
            let mut top = f32::MAX;
            let mut x = lo;
            while x <= hi {
                top = top.min(self.ground(x));
                x += 8.;
            }
            let mut cruise = (top - 44.).max(108.);
            // Keep the second leg's crossing apart from the first's.
            if let Some((l0, h0, c0)) = first
                && lo < h0
                && hi > l0
                && (cruise - c0).abs() < 16.
            {
                cruise = if top - (c0 + 18.) >= 22. {
                    c0 + 18.
                } else {
                    (c0 - 18.).max(100.)
                };
            }
            first.get_or_insert((lo, hi, cruise));
            // Each leg's climbs and descents sit to one side of the pad
            // centre, so the legs meeting at the pickup stay distinct.
            let side = if k == 0 { -5. } else { 5. };
            let path = [
                point(vec2(a.x + side, a.y - 30.)),
                point(vec2(a.x + side, cruise)),
                point(vec2(b.x + side, cruise)),
                point(vec2(b.x + side, b.y - 38.)),
            ];
            dashed_path(&path, with_alpha(c, 0.9), now * 1.2 + k as f32 * 0.5);
            let tip = path[3] + vec2(0., 6.);
            shapes::fill_triangle(tip, tip + vec2(-6., -9.), tip + vec2(6., -9.), c);
        }
    }

    fn draw_pads(&self, now: f32) {
        let target = self.target_pad();
        let flying = self.phase == Phase::Flight;
        let wind = self.wind();
        for i in 0..4 {
            let p = point(self.pad(i));
            let role = self.planet().roles[i];
            let c = role_color(role);
            let is_target = flying && i == target && self.docked != Some(i);
            if is_target {
                // Guide beam and breathing target brackets on the objective.
                let a = 0.12 + 0.05 * (now * 3.).sin();
                let beam = with_alpha(c, a);
                let clear = with_alpha(c, 0.);
                shade(quad(
                    (vec2(p.x - 18., p.y - 150.), clear),
                    (vec2(p.x + 18., p.y - 150.), clear),
                    (vec2(p.x - PAD_HALF + 4., p.y - 4.), beam),
                    (vec2(p.x + PAD_HALF - 4., p.y - 4.), beam),
                ));
                let k = pulse(now, 1.1);
                let grow = 3. * k;
                corners(
                    p.x - PAD_HALF - 7. - grow,
                    p.y - 36. - grow,
                    p.x + PAD_HALF + 7. + grow,
                    p.y + 5. + grow * 0.5,
                    8.,
                    2.,
                    with_alpha(c, 0.55 + 0.45 * k),
                );
                let tip = vec2(p.x, p.y - 44. + bob(now, 0.8, 3.));
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
            let on = !is_target || pulse(now, 1.5) > 0.35;
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
                draw_poly(
                    x,
                    p.y - 16.,
                    8,
                    2.6,
                    0.,
                    if on { c } else { darken(c, 0.55) },
                );
            }
            windsock(vec2(p.x + PAD_HALF + 6., p.y + 2.), wind, now, i);
            // Role badge, then the pad's remaining supplies.
            let badge = vec2(p.x, p.y + 21.);
            let spent = role == Role::Pickup && self.loaded;
            let dim = if spent { 0.4 } else { 1. };
            shapes::fill_circle(badge, 11.5, Color::new(0.03, 0.04, 0.07, 0.88));
            shapes::stroke_circle(badge, 11.5, 1.5, with_alpha(c, 0.85 * dim));
            icon(
                role_icon(role),
                badge.x,
                badge.y,
                14.,
                with_alpha(lighten(c, 0.1), dim),
            );
            if spent {
                icon_badge(Icon::Check, badge.x + 9., badge.y - 8., 11., GREEN, INK);
            }
            if role != Role::Delivery {
                let fuel = self.reserves[i];
                let tint = if fuel > 0.5 {
                    lighten(BLUE, 0.25)
                } else {
                    with_alpha(MUTED, 0.45)
                };
                let value = format!("{fuel:.0}");
                let kit = role == Role::Outpost;
                let w = icon_value_width(&value, 11., tint) + if kit { 18. } else { 0. };
                let x = p.x - w * 0.5;
                icon_value(Icon::Fuel, &value, x, p.y + 45., 11., tint);
                if kit {
                    let used = self.repaired[i];
                    icon(
                        Icon::Wrench,
                        x + w - 6.,
                        p.y + 45.,
                        12.,
                        if used {
                            with_alpha(MUTED, 0.35)
                        } else {
                            lighten(GREEN, 0.15)
                        },
                    );
                }
            }
        }
    }

    fn draw_beacon(&self, now: f32) {
        let logged = self.beacon[self.index];
        let b = point(Vec2::from_array(self.planet().beacon));
        let lift = if logged { 0. } else { bob(now, 0.45, 2.) };
        let c = vec2(b.x, b.y + lift);
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
        if logged {
            icon_badge(Icon::Check, c.x, c.y - 19., 13., GREEN, INK);
        } else {
            // The reward, previewed: score for flying through it.
            let w = icon_value_width("250", 11., GOLD);
            icon_value(Icon::Star, "250", c.x - w * 0.5, c.y - 19., 11., GOLD);
        }
    }

    /// The touchdown footprint under the ship: green fits a pad slowly and
    /// upright, amber is a pad at the wrong speed or attitude, red is rock.
    fn draw_guide(&self) {
        if self.phase != Phase::Flight || self.docked.is_some() {
            return;
        }
        let alt = self.altitude();
        if !(0. ..=120.).contains(&alt) {
            return;
        }
        let near = 1. - alt / 120.;
        let c = match self.landing() {
            Landing::Safe => GREEN,
            Landing::Risky => GOLD,
            Landing::Rough => RED,
        };
        let surface = self.surface_under(self.pos.x);
        let g = point(vec2(self.pos.x, surface));
        let mut y = point(self.pos).y + FEET + 4.;
        while y < g.y - 6. {
            let end = (y + 4.).min(g.y - 6.);
            draw_line(g.x, y, g.x, end, 1., with_alpha(c, 0.4 * near));
            y += 9.;
        }
        let half = HALF_BODY + 1.;
        let col = with_alpha(c, 0.3 + 0.65 * near);
        draw_line(g.x - half, g.y - 2., g.x + half, g.y - 2., 2.2, col);
        for side in [-1., 1.] {
            let x = g.x + side * half;
            draw_line(x, g.y - 1., x, g.y - 8., 2.2, col);
        }
    }

    /// Rock dust kicked up by the engines close to the ground.
    fn draw_dust(&self, planet: Color) {
        if self.phase != Phase::Flight || !(self.thrusting || self.braking) {
            return;
        }
        let alt = self.altitude();
        if alt > 50. {
            return;
        }
        let near = 1. - alt.max(0.) / 50.;
        let g = point(vec2(self.pos.x, self.surface_under(self.pos.x)));
        let tint = mix(DUST, planet, 0.35);
        for j in 0..7u32 {
            let age = ((self.ticks % 3) + 3 * j) as f32 / 60.;
            let t = age / 0.35;
            for side in [-1., 1.] {
                let p = vec2(g.x + side * (9. + age * 120.), g.y - 2. - t * 10.);
                shapes::fill_circle(p, 2. + 7. * t, with_alpha(tint, 0.32 * near * (1. - t)));
            }
        }
    }

    fn draw_ship(&self, now: f32) {
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
        let (sin, cos) = self.angle.sin_cos();
        let rotate = |x: f32, y: f32| p + vec2(x * cos - y * sin, x * sin + y * cos);
        if self.thrusting {
            // Exhaust puffs trail behind in world space, cooling to smoke.
            let back = vec2(-sin, cos);
            let side = vec2(cos, sin);
            let nozzle = rotate(0., 11.);
            for j in 0..9u32 {
                let age = ((self.ticks % 2) + 2 * j) as f32 / 60.;
                let t = age / 0.3;
                let spread = (noise((self.ticks / 2).wrapping_sub(j)) - 0.5) * 2.;
                let at = nozzle + back * (age * 130.) + side * (spread * age * 40.)
                    - self.velocity * age;
                let col = mix(
                    Color::new(1., 0.68, 0.3, 0.5),
                    Color::new(0.62, 0.62, 0.68, 0.22),
                    t,
                );
                shapes::fill_circle(at, 1.8 + 5.5 * t, with_alpha(col, col.a * (1. - t)));
            }
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
            // The parcel rides in a small gold crate under the hull.
            let lit = lighten(GOLD, 0.3);
            let dark = darken(GOLD, 0.35);
            shade(quad(
                (rotate(-4., 5.5), lit),
                (rotate(4., 5.5), lit),
                (rotate(-4., 11.), dark),
                (rotate(4., 11.), dark),
            ));
            let (a, b) = (rotate(-4., 8.2), rotate(4., 8.2));
            draw_line(a.x, a.y, b.x, b.y, 1., darken(GOLD, 0.55));
        }
        if damaged && pulse(now, 2.) > 0.5 {
            let lamp = rotate(0., -14.);
            shapes::fill_circle(lamp, 1.8, RED);
        }
        if matches!(self.phase, Phase::Wreck | Phase::Grounded) {
            for k in 0..5 {
                let a = k as f32 * 1.256 + now * 0.12;
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
    fn draw_lights(&self, now: f32) {
        if self.phase == Phase::Flight {
            let p = point(self.pad(self.target_pad()));
            let c = role_color(self.planet().roles[self.target_pad()]);
            light(
                vec2(p.x, p.y - 6.),
                24.,
                with_alpha(c, 0.35 + 0.1 * (now * 3.).sin()),
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

    /// A floating key bubble over a landed ship: X with what it would do
    /// here, or Up to lift off when the pad has nothing left to offer.
    fn draw_prompt(&self, now: f32) {
        if self.phase != Phase::Flight || self.docked.is_none() {
            return;
        }
        let (key, glyph, color) = match self.dock_action() {
            Some((glyph, color)) => ("X", glyph, color),
            None if self.fuel > 0. => ("Up", Icon::ArrowUp, PAPER),
            None => return,
        };
        let ship = point(self.pos);
        let key_w = keycap_width(key);
        let w = key_w + 32.;
        let h = 28.;
        let x = ship.x - w * 0.5;
        let y = ship.y - 56. + bob(now, 0.7, 2.);
        shapes::fill_triangle(
            vec2(ship.x - 5., y + h - 1.),
            vec2(ship.x + 5., y + h - 1.),
            vec2(ship.x, y + h + 6.),
            Color::new(0.06, 0.08, 0.12, 0.94),
        );
        rounded(x, y, w, h, 8., Color::new(0.06, 0.08, 0.12, 0.94));
        outline(x, y, w, h, 8., 1.2, with_alpha(color, 0.75));
        keycap(key, "", x + 4., y + 3., PAPER);
        icon(glyph, x + key_w + 17., y + h * 0.5, 17., color);
    }
}

/// A dashed polyline with rounded corners; dashes march along it as
/// `offset` grows.
fn dashed_path(points: &[Vec2; 4], color: Color, offset: f32) {
    let mut path = [Vec2::ZERO; 12];
    let mut n = 0;
    path[n] = points[0];
    n += 1;
    for i in 1..3 {
        let (p, c, q) = (points[i - 1], points[i], points[i + 1]);
        let r = 18_f32
            .min((c - p).length() * 0.5)
            .min((q - c).length() * 0.5);
        let a = c + (p - c).normalize_or_zero() * r;
        let b = c + (q - c).normalize_or_zero() * r;
        for s in 0..5 {
            let t = s as f32 / 4.;
            path[n] = a * ((1. - t) * (1. - t)) + c * (2. * t * (1. - t)) + b * (t * t);
            n += 1;
        }
    }
    path[n] = points[3];
    n += 1;
    let (on, period) = (9., 16.);
    let mut along = -offset.fract() * period;
    for w in path[..n].windows(2) {
        let (p, q) = (w[0], w[1]);
        let len = (q - p).length();
        if len <= 0.01 {
            continue;
        }
        let dir = (q - p) / len;
        let mut s = 0.;
        while s < len {
            // `rem_euclid` can round a tiny negative input up to exactly
            // `period`; wrap it, and always advance by a visible amount, so
            // float rounding can never stall the dash walk.
            let phase = (along + s).rem_euclid(period);
            let phase = if phase >= period { 0. } else { phase };
            if phase < on {
                let e = (s + (on - phase).max(0.25)).min(len);
                let (u, v) = (p + dir * s, p + dir * e);
                draw_line(u.x, u.y, v.x, v.y, 2.6, color);
                s = e;
            } else {
                s += (period - phase).max(0.25);
            }
        }
        along += len;
    }
}

/// A striped windsock on a mast: limp in calm air, streaming downwind and
/// fluttering as the gusts rise.
fn windsock(base: Vec2, wind: f32, now: f32, seed: usize) {
    let top = base - vec2(0., 28.);
    draw_line(
        base.x,
        base.y,
        top.x,
        top.y,
        1.5,
        Color::new(0.58, 0.62, 0.7, 1.),
    );
    let strength = (wind.abs() / 5.).clamp(0., 1.);
    let dir = if wind < 0. { -1. } else { 1. };
    let flutter = (now * 7. + seed as f32 * 1.7).sin() * 0.07 * (0.3 + strength);
    let droop = (1. - strength) * 1.3 + flutter;
    let axis = vec2(dir * droop.cos(), droop.sin());
    let normal = vec2(-axis.y, axis.x);
    let len = 11. + 6. * strength;
    let at = |t: f32, side: f32| top + axis * (len * t) + normal * (side * (2.8 - 1.5 * t));
    let orange = Color::new(0.98, 0.5, 0.25, 0.95);
    let white = Color::new(0.94, 0.95, 0.97, 0.95);
    shade(quad(
        (at(0., -1.), orange),
        (at(0., 1.), orange),
        (at(0.5, -1.), orange),
        (at(0.5, 1.), orange),
    ));
    shade(quad(
        (at(0.5, -1.), white),
        (at(0.5, 1.), white),
        (at(1., -1.), white),
        (at(1., 1.), white),
    ));
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
        self.draw_card();
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: self.planet().name.to_string(),
            objective: match self.phase {
                Phase::Briefing => "Launch".to_string(),
                Phase::Flight if self.loaded => format!("Deliver to {}", self.planet().recipient),
                Phase::Flight => "Load the parcel".to_string(),
                Phase::Debrief => "Choose refits".to_string(),
                Phase::Wreck => "Spend a permit".to_string(),
                Phase::Certified => "Courier certified".to_string(),
                Phase::Grounded => "Grounded".to_string(),
            },
            status: format!(
                "{} delivered, {} stamps, {} permits",
                self.delivered(),
                self.stamps(),
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
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.contract() as u32, 8))
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        self.feedback(fx);
    }
    fn draw_objective(&self, area: Rect) -> bool {
        self.objective(area)
    }
    fn draw_result(&self, area: Rect) -> bool {
        self.results(area)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn header_reports_campaign_progress_and_short_objectives() {
        let mut game = Game::new();
        assert_eq!(game.progress(), Some((1, 8)));
        assert_eq!(game.hud().stage, "Postmark Moon");
        for index in 0..8 {
            game.index = index;
            for loaded in [false, true] {
                game.loaded = loaded;
                for phase in [
                    Phase::Briefing,
                    Phase::Flight,
                    Phase::Debrief,
                    Phase::Wreck,
                    Phase::Certified,
                    Phase::Grounded,
                ] {
                    game.phase = phase;
                    let objective = game.hud().objective;
                    assert!(objective.split_whitespace().count() <= 5, "{objective}");
                }
            }
        }
        game.index = 7;
        game.phase = Phase::Certified;
        assert_eq!(game.progress(), Some((8, 8)));
    }
    #[test]
    fn landing_verdict_matches_the_touchdown_rules() {
        let mut game = Game::new();
        game.phase = Phase::Flight;
        game.docked = None;
        let pad = game.pad(1);
        game.pos = pad - vec2(0., FEET + 20.);
        game.velocity = vec2(2., 6.);
        assert_eq!(game.landing(), Landing::Safe);
        game.velocity.y = 40.;
        assert_eq!(game.landing(), Landing::Risky);
        game.velocity.y = 6.;
        game.angle = 0.5;
        assert_eq!(game.landing(), Landing::Risky);
        game.angle = 0.;
        game.pos.x += PAD_HALF;
        assert_eq!(game.landing(), Landing::Rough);
    }
}
