//! Procedural Valle Verde art: soft pastel farmland, anti-aliased roads,
//! round tree canopies with soft shadows, a duck pond, drifting cloud shadows
//! and emissive accents (headlights, brake lights, gates) that feed the bloom.
//!
//! Everything here is presentation. Static layout (tree placement, field
//! plots, curbs) is computed once in [`World::new`], so a frame only draws.
use crate::sim::{Pose, Track};
use crate::style::POND;
use ferravex_render::post;
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Fill, Join, Stroke};

pub const CREAM: Color = Color::new(0.96, 0.92, 0.81, 1.);
const HEDGE: Color = Color::new(0.47, 0.58, 0.37, 1.);
const PLOTS: [Color; 5] = [
    Color::new(0.64, 0.73, 0.48, 1.),
    Color::new(0.70, 0.77, 0.52, 1.),
    Color::new(0.85, 0.79, 0.55, 1.),
    Color::new(0.58, 0.68, 0.45, 1.),
    Color::new(0.76, 0.80, 0.56, 1.),
];
const VERGE: Color = Color::new(0.90, 0.84, 0.69, 1.);
const VERGE_EDGE: Color = Color::new(0.80, 0.74, 0.58, 1.);
const ASPHALT: Color = Color::new(0.33, 0.36, 0.38, 1.);
const CURB_RED: Color = Color::new(0.85, 0.37, 0.30, 1.);
const BODY: Color = Color::new(0.86, 0.29, 0.20, 1.);
const BODY_DARK: Color = Color::new(0.66, 0.20, 0.15, 1.);
const GLASS: Color = Color::new(0.20, 0.29, 0.32, 1.);
const TIRE: Color = Color::new(0.11, 0.13, 0.12, 1.);
const AMBER: Color = Color::new(1.0, 0.80, 0.42, 1.);

struct Tree {
    at: Vec2,
    size: f32,
    variant: u32,
}

/// Precomputed static scenery.
pub struct World {
    trees: Vec<Tree>,
    plots: Vec<(Rect, Color, bool)>,
}

fn rgba(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}

fn shade(c: Color, k: f32) -> Color {
    Color::new(c.r * k, c.g * k, c.b * k, c.a)
}

fn quad(points: [Vec2; 4], color: Color) {
    draw_triangle(points[0], points[1], points[2], color);
    draw_triangle(points[0], points[2], points[3], color);
}

fn local_quad(pose: Pose, center: Vec2, half: Vec2, color: Color) {
    let rotation = Vec2::from_angle(pose.angle);
    quad(
        [
            vec2(-half.x, -half.y),
            vec2(half.x, -half.y),
            vec2(half.x, half.y),
            vec2(-half.x, half.y),
        ]
        .map(|p| pose.position + rotation.rotate(p + center)),
        color,
    );
}

impl World {
    pub fn new(track: &Track) -> Self {
        let mut trees = Vec::new();
        for i in 0..150_u32 {
            let x = ((i * 197 + 43) % 1090) as f32 + 5.;
            let y = ((i * 157 + i * i * 3 + 31) % 640) as f32;
            let p = vec2(x, y);
            if track.nearest(p).1 > 67.
                && !Rect::new(270., 245., 430., 90.).contains(p)
                && !(x > 900. && y > 515.)
            {
                trees.push(Tree {
                    at: p,
                    size: 8. + (i % 8) as f32,
                    variant: i,
                });
            }
        }
        let mut plots = Vec::new();
        for y in 0..5 {
            for x in 0..8 {
                let k = ((x * 13 + y * 7) % PLOTS.len() as i32) as usize;
                let rect = Rect::new(
                    x as f32 * 160. - 60. + 5.,
                    y as f32 * 160. - 60. + 5.,
                    150.,
                    150.,
                );
                plots.push((rect, PLOTS[k], k == 2));
            }
        }
        Self { trees, plots }
    }

    /// Ground, fields, pond, barns, road and trees.
    pub fn draw_land(&self, track: &Track, time: f32, motion: f32) {
        // Generous base land: portrait follow views can extend past the fields.
        draw_rectangle(-700., -500., 2500., 1650., HEDGE);
        for (rect, color, wheat) in &self.plots {
            shapes::fill_rounded_rect(
                *rect,
                10.,
                Fill::vertical(*rect, shade(*color, 1.04), shade(*color, 0.97)),
            );
            if *wheat {
                for row in 0..9 {
                    let y = rect.y + 14. + row as f32 * 15.;
                    draw_line(
                        rect.x + 12.,
                        y,
                        rect.x + rect.w - 12.,
                        y - 6.,
                        2.,
                        shade(*color, 0.9),
                    );
                }
            }
        }
        self.pond(time, motion);
        barn(vec2(620., 260.));
        barn(vec2(275., 280.));
        // Road: a sandy verge, a darker lip and smooth asphalt, all one AA polyline each.
        for (width, color) in [(88., VERGE_EDGE), (82., VERGE), (66., ASPHALT)] {
            shapes::polyline(
                &track.points,
                Stroke::new(width, color).join(Join::Miter).closed(true),
            );
        }
        let n = track.points.len();
        for i in 0..n {
            let a = track.points[i];
            let b = track.points[(i + 1) % n];
            if i % 4 == 0 {
                let mid = a.lerp(b, 0.15);
                let end = a.lerp(b, 0.85);
                draw_line(
                    mid.x,
                    mid.y,
                    end.x,
                    end.y,
                    1.6,
                    Color::new(0.93, 0.91, 0.84, 0.55),
                );
            }
            if i % 2 == 0 && i % 48 < 15 {
                let normal = vec2(-track.tangent(i).y, track.tangent(i).x);
                let (ca, cb) = (a + normal * 35., b + normal * 35.);
                draw_line(
                    ca.x,
                    ca.y,
                    cb.x,
                    cb.y,
                    4.,
                    if i % 4 == 0 { CURB_RED } else { CREAM },
                );
            }
        }
        finish_line(track);
        for tree in &self.trees {
            tree_shadow(tree);
        }
        for tree in &self.trees {
            draw_tree(tree);
        }
    }

    fn pond(&self, time: f32, motion: f32) {
        // A soft shore ring under the water body.
        shapes::polyline(
            &POND,
            Stroke::new(10., Color::new(0.74, 0.83, 0.70, 1.))
                .join(Join::Round)
                .closed(true),
        );
        for i in 1..POND.len() - 1 {
            draw_triangle(POND[0], POND[i], POND[i + 1], rgba(108, 160, 172));
        }
        draw_triangle(POND[1], POND[2], vec2(975., 603.), rgba(126, 178, 186));
        // Glints drift on the water.
        for (i, base) in [vec2(960., 575.), vec2(1035., 568.), vec2(1010., 605.)]
            .into_iter()
            .enumerate()
        {
            let t = time * 0.8 * motion + i as f32 * 2.1;
            let p = base + vec2(t.sin() * 6., (t * 0.7).cos() * 2.);
            draw_line(
                p.x - 4.,
                p.y,
                p.x + 4.,
                p.y,
                1.5,
                Color::new(1., 1., 1., 0.45 + 0.2 * t.sin()),
            );
        }
    }

    /// Soft cloud shadows and a warm light pool drifting over the farm.
    pub fn draw_atmosphere(&self, time: f32, motion: f32) {
        let drift = time * motion;
        for (i, (base, radius)) in [
            (vec2(150., 120.), 190.),
            (vec2(700., 520.), 230.),
            (vec2(1050., 80.), 170.),
        ]
        .into_iter()
        .enumerate()
        {
            let x = (base.x + drift * (7. + i as f32 * 2.)).rem_euclid(1700.) - 300.;
            let c = vec2(x, base.y + (drift * 0.1 + i as f32).sin() * 20.);
            shapes::fill_circle(
                c,
                radius,
                Fill::radial(
                    c,
                    radius,
                    Color::new(0.10, 0.16, 0.20, 0.11),
                    Color::new(0.10, 0.16, 0.20, 0.),
                ),
            );
        }
        // Pollen motes catching the light.
        for i in 0..14_u32 {
            let seed = i as f32 * 1.618;
            let t = drift * (0.2 + (i % 5) as f32 * 0.04) + seed * 10.;
            let x = ((i * 389 + 70) % 1100) as f32 + (t * 7.).rem_euclid(140.) - 70.;
            let y = ((i * 211 + 40) % 640) as f32 + (t * 0.9).sin() * 18.;
            let a = 0.35 + 0.25 * (t * 1.7).sin();
            draw_poly(x, y, 6, 1.6, 0., Color::new(1., 0.97, 0.82, a));
        }
    }
}

fn barn(p: Vec2) {
    let body = Rect::new(p.x, p.y, 58., 32.);
    shapes::drop_shadow(
        body,
        4.,
        shapes::Shadow::soft(Color::new(0.08, 0.14, 0.08, 0.35))
            .offset(vec2(5., 7.))
            .blur(12.),
    );
    shapes::fill_rounded_rect(body, 3., rgba(232, 218, 182));
    let roof = Rect::new(p.x - 3., p.y - 3., 64., 34.);
    shapes::fill_rounded_rect(
        roof,
        4.,
        Fill::vertical(roof, rgba(204, 100, 72), rgba(150, 72, 54)),
    );
    draw_line(
        p.x - 3.,
        p.y + 14.,
        p.x + 61.,
        p.y + 14.,
        2.,
        rgba(232, 150, 104),
    );
}

fn tree_shadow(tree: &Tree) {
    let c = tree.at + vec2(4., 6.);
    let r = tree.size * 1.15;
    shapes::fill_circle(
        c,
        r,
        Fill::radial(
            c,
            r,
            Color::new(0.10, 0.18, 0.10, 0.30),
            Color::new(0.10, 0.18, 0.10, 0.),
        ),
    );
}

fn draw_tree(tree: &Tree) {
    let (base, light) = if tree.variant.is_multiple_of(2) {
        (rgba(72, 120, 78), rgba(122, 166, 102))
    } else {
        (rgba(60, 108, 74), rgba(108, 152, 96))
    };
    let p = tree.at;
    let s = tree.size;
    shapes::fill_circle(
        p,
        s,
        Fill::radial(p - vec2(s, s) * 0.35, s * 1.6, light, base),
    );
    shapes::fill_circle(
        p - vec2(s * 0.3, s * 0.32),
        s * 0.32,
        Color::new(1., 1., 0.85, 0.16),
    );
}

fn finish_line(track: &Track) {
    let (start, tangent) = track.gate(0);
    let normal = vec2(-tangent.y, tangent.x);
    for x in 0..2 {
        for y in 0..10 {
            let center = start + tangent * (x as f32 * 5. - 2.5) + normal * (y as f32 * 6. - 27.);
            local_quad(
                Pose {
                    position: center,
                    angle: tangent.y.atan2(tangent.x),
                },
                Vec2::ZERO,
                vec2(2.5, 3.),
                if (x + y) % 2 == 0 {
                    CREAM
                } else {
                    Color::new(0.16, 0.19, 0.20, 1.)
                },
            );
        }
    }
    for side in [-1., 1.] {
        let post = start + normal * 46. * side;
        shapes::fill_circle(post + vec2(2., 3.), 5., Color::new(0.1, 0.15, 0.1, 0.22));
        shapes::fill_circle(post, 3.5, CREAM);
        draw_line(
            post.x,
            post.y,
            post.x - tangent.x * 16.,
            post.y - tangent.y * 16.,
            3.,
            CREAM,
        );
    }
}

/// Glowing posts and pulsing arrows at the next gate.
pub fn next_gate(track: &Track, gate: usize, time: f32, motion: f32) {
    let (center, tangent) = track.gate(gate);
    let normal = vec2(-tangent.y, tangent.x);
    let pulse = 1. + 0.18 * (time * 6.).sin() * motion;
    for side in [-1., 1.] {
        let p = center + normal * 44. * side;
        post::glow_circle(p.x, p.y, 16. * pulse, AMBER, 1.3);
        draw_triangle(
            p - (tangent * 6. + normal * 3.) * pulse,
            p + tangent * 5. * pulse,
            p - (tangent * 6. - normal * 3.) * pulse,
            CREAM,
        );
    }
}

/// Amber guidance back to the nearest road point.
pub fn recovery_arrow(from: Vec2, target: Vec2) {
    let direction = (target - from).normalize_or_zero();
    let end = from + direction * from.distance(target).min(85.);
    let normal = vec2(-direction.y, direction.x);
    shapes::polyline(
        &[from + direction * 18., end - direction * 6.],
        Stroke::new(3., AMBER).cap(shapes::Cap::Round),
    );
    draw_triangle(
        end,
        end - direction * 10. + normal * 5.,
        end - direction * 10. - normal * 5.,
        AMBER,
    );
    shapes::stroke_circle(target, 7., 2.5, AMBER);
    post::glow_circle(target.x, target.y, 18., AMBER, 1.2);
}

/// What the car is doing this frame, for its lights.
#[derive(Clone, Copy, Debug, Default)]
pub struct CarLights {
    pub braking: bool,
    pub night_glow: f32,
}

/// The rally car: soft shadow, rounded body and emissive lights. Drawn 15%
/// larger than its point-collision footprint so it reads at follow zoom;
/// `scale` adds the bump squash (length, width).
pub fn car(pose: Pose, scale: Vec2, lights: CarLights) {
    let s = scale * 1.15;
    let forward = Vec2::from_angle(pose.angle);
    let side = vec2(-forward.y, forward.x);
    let at = |x: f32, y: f32| pose.position + forward * x * s.x + side * y * s.y;
    // Two-layer soft contact shadow.
    let shadow = vec2(3., 5.);
    for (r, a) in [(9., 0.12), (7., 0.18)] {
        shapes::fill_capsule(
            at(-6., 0.) + shadow,
            at(6., 0.) + shadow,
            r * s.y,
            Color::new(0.05, 0.1, 0.07, a),
        );
    }
    let q = |center: Vec2, half: Vec2, color: Color| local_quad(pose, center * s, half * s, color);
    for x in [-7., 7.] {
        for y in [-6.2, 6.2] {
            q(vec2(x, y), vec2(3., 1.5), TIRE);
        }
    }
    shapes::fill_capsule(
        at(-6.5, 0.),
        at(6.5, 0.),
        6. * s.y,
        Fill::linear(at(0., -6.), at(0., 6.), BODY, BODY_DARK),
    );
    q(vec2(1., 0.), vec2(10., 1.8), CREAM);
    shapes::fill_capsule(at(-4.5, 0.), at(2.5, 0.), 4.2 * s.y, GLASS);
    shapes::fill_capsule(
        at(1.5, -2.),
        at(1.5, 2.),
        0.8 * s.y,
        Color::new(0.75, 0.88, 0.92, 0.55),
    );
    q(vec2(-1., 0.), vec2(2.4, 4.4), shade(BODY, 1.05));
    q(vec2(-11.2, 0.), vec2(1.1, 6.6), rgba(90, 46, 33));
    for y in [-3.7, 3.7] {
        let head = at(11.5, y);
        shapes::fill_circle(head, 1.4 * s.y, Color::new(1., 0.97, 0.85, 1.));
        post::glow_circle(
            head.x + forward.x * 4.,
            head.y + forward.y * 4.,
            9.,
            Color::new(1., 0.93, 0.7, 1.),
            0.8 + lights.night_glow,
        );
        let tail = at(-12., y);
        let (radius, intensity) = if lights.braking {
            (10., 2.2)
        } else {
            (5., 0.7)
        };
        shapes::fill_circle(tail, 1.3 * s.y, Color::new(1., 0.3, 0.25, 1.));
        post::glow_circle(
            tail.x,
            tail.y,
            radius,
            Color::new(1., 0.25, 0.2, 1.),
            intensity,
        );
    }
}

/// The best-lap ghost: a translucent cool silhouette that softly glows.
pub fn ghost(pose: Pose) {
    let forward = Vec2::from_angle(pose.angle) * 1.15;
    let (a, b) = (pose.position - forward * 6.5, pose.position + forward * 6.5);
    post::with_glow(|| shapes::fill_capsule(a, b, 7.5, Color::new(0.45, 0.75, 1., 0.16)));
    shapes::fill_capsule(a, b, 6.4, Color::new(0.80, 0.92, 1., 0.30));
    shapes::fill_capsule(
        a + forward,
        b - forward * 3.,
        3.8,
        Color::new(0.80, 0.92, 1., 0.25),
    );
}

/// Three pond ducks bob on the water and paddle away from a nearby car.
pub fn ducks(time: f32, motion: f32, car: Vec2) {
    for (i, home) in [vec2(990., 572.), vec2(1032., 590.), vec2(962., 598.)]
        .into_iter()
        .enumerate()
    {
        let bob = vec2(
            (time * 0.7 + i as f32).sin(),
            (time * 0.9 + i as f32 * 2.).cos(),
        ) * 3.;
        let away = home - car;
        let flee = away.normalize_or_zero() * (70. - away.length()).clamp(0., 26.);
        let p = home + bob * motion + flee;
        let facing = if flee.length() > 1. {
            flee.normalize()
        } else {
            vec2(if i % 2 == 0 { 1. } else { -1. }, 0.)
        };
        // Ripple ring, body, head and beak.
        shapes::stroke_circle(
            p,
            7. + (time * 2. + i as f32).sin().abs() * 2. * motion,
            1.,
            Color::new(1., 1., 1., 0.25),
        );
        shapes::fill_circle(p + vec2(1., 2.), 5., Color::new(0.1, 0.2, 0.2, 0.2));
        shapes::fill_circle(p, 5., rgba(250, 246, 232));
        let head = p + facing * 5.;
        shapes::fill_circle(head, 3., rgba(62, 112, 74));
        let beak = head + facing * 3.;
        let side = vec2(-facing.y, facing.x) * 1.5;
        draw_triangle(
            beak + side,
            beak - side,
            beak + facing * 3.,
            rgba(238, 152, 56),
        );
    }
}
