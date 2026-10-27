//! World rendering for the 2D lab: a lit blueprint workshop with soft contact
//! shadows, top-lit bodies with anti-aliased rims, emissive accents (joint
//! lamps, impact flares, the pulse wave, selection halo and drop guide) that
//! feed the bloom, and drifting dust motes.
//!
//! Everything here reads the simulation; nothing writes it. Ambient motion
//! (motes, lamp breathing) stops under reduced motion.
use super::juice::Juice;
use super::look::{self, AMBER, BG_BOTTOM, BG_TOP, CYAN, GOLD, faded};
use super::presentation::LabLayout;
use super::{Lab, MAX_BODIES, Shape};
use ferravex_physics::d2;
use ferravex_play::tween::Motion;
use ferravex_render::post;
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Cap, Fill, Join, Stroke};

pub(super) struct World<'a> {
    pub lab: &'a Lab,
    pub view: LabLayout,
    pub juice: &'a Juice,
    /// Pulse ring (center, age) and its intensity, from the policy-aware visual.
    pub ring: Option<(Vec2, f32)>,
    pub ring_intensity: f32,
    pub alpha: f32,
    /// Drop preview position and whether it's a ball.
    pub preview: Option<(Vec2, bool)>,
    pub time: f32,
    pub motion: Motion,
}

const WHITE_HOT: Color = Color::new(1., 1., 1., 1.);

fn lighten(c: Color, k: f32) -> Color {
    let m = shapes::mix(c, WHITE_HOT, k);
    Color::new(m.r, m.g, m.b, c.a)
}

fn darken(c: Color, k: f32) -> Color {
    let m = shapes::mix(c, Color::new(0., 0., 0., 1.), k);
    Color::new(m.r, m.g, m.b, c.a)
}

/// Authored body colors stay below the bloom threshold (0.9) so only
/// emissive accents glow.
fn tame(c: Color) -> Color {
    let peak = c.r.max(c.g).max(c.b);
    if peak > 0.86 {
        let k = 0.86 / peak;
        Color::new(c.r * k, c.g * k, c.b * k, c.a)
    } else {
        c
    }
}

pub(super) fn draw(f: &World) {
    let w = screen_width();
    let h = screen_height();
    let view = f.view;
    let scale = view.scale;
    let to_screen = |p: Vec2| view.to_screen(p);
    let lab = f.lab;
    let sandbox = &lab.sandbox;
    let breathe = if f.motion.is_reduced() {
        0.
    } else {
        (f.time * 0.7).sin()
    };

    // Backdrop: a deep gradient with warm and cool lamp pools on the back wall.
    let back = Rect::new(0., 0., w, h);
    shapes::fill_rounded_rect(back, 0., Fill::vertical(back, BG_TOP, BG_BOTTOM));
    let floor_y = to_screen(vec2(0., 0.)).y;
    for (x, color, size) in [
        (-6.5, Color::new(0.30, 0.55, 0.85, 0.10), 9.),
        (6.5, Color::new(0.90, 0.60, 0.30, 0.07), 8.),
        (0., Color::new(0.35, 0.75, 0.80, 0.06), 12.),
    ] {
        let c = to_screen(vec2(x, 9.5));
        let r = size * scale * (1. + 0.03 * breathe);
        shapes::fill_circle(
            c,
            r,
            Fill::radial(c, r, color, Color::new(color.r, color.g, color.b, 0.)),
        );
    }

    // Blueprint grid: quiet minor lines, brighter every 4 m.
    for x in -12..=12 {
        let major = x % 4 == 0;
        let a = to_screen(vec2(x as f32, 0.));
        let b = to_screen(vec2(x as f32, 13.));
        let c = if major {
            Color::new(0.45, 0.65, 0.85, 0.11)
        } else {
            Color::new(0.45, 0.65, 0.85, 0.05)
        };
        draw_rectangle(a.x - 0.5, b.y, 1., a.y - b.y, c);
    }
    for y in 0..=13 {
        let major = y % 4 == 0;
        let a = to_screen(vec2(-12., y as f32));
        let b = to_screen(vec2(12., y as f32));
        let c = if major {
            Color::new(0.45, 0.65, 0.85, 0.11)
        } else {
            Color::new(0.45, 0.65, 0.85, 0.05)
        };
        draw_rectangle(a.x, a.y - 0.5, b.x - a.x, 1., c);
    }
    // Tall narrow screens show open sky above the lab, where launches go.
    let sky = view.to_world(vec2(0., 100.)).y.floor().clamp(13., 40.) as i32;
    for y in 14..=sky {
        let fade = 1. - (y - 13) as f32 / (sky - 12) as f32;
        let a = to_screen(vec2(-12., y as f32));
        let b = to_screen(vec2(12., y as f32));
        draw_rectangle(
            a.x,
            a.y - 0.5,
            b.x - a.x,
            1.,
            Color::new(0.45, 0.65, 0.85, 0.05 * fade),
        );
    }

    // Dust motes drifting up through the lamp light.
    for i in 0..28_u32 {
        let seed = i as f32 * 12.9898;
        let fx = (seed.sin() * 43758.547).fract().abs();
        let fy = ((seed * 1.7).cos() * 24634.633).fract().abs();
        let speed = 0.15 + 0.2 * fx;
        let drift = if f.motion.is_reduced() {
            0.
        } else {
            f.time * speed
        };
        let y = (fy * 13. + drift) % 13.;
        let x = -11.5
            + fx * 23.
            + if f.motion.is_reduced() {
                0.
            } else {
                (f.time * 0.4 + seed).sin() * 0.3
            };
        let p = to_screen(vec2(x, y));
        let a = 0.10 + 0.14 * fy;
        draw_rectangle(p.x, p.y, 2., 2., Color::new(0.7, 0.85, 1., a));
    }

    // Platforms and walls: top-lit slabs with a crisp highlight edge.
    for (position, half) in &sandbox.platforms {
        let top_left = to_screen(*position + vec2(-half.x, half.y));
        let rect = Rect::new(
            top_left.x,
            top_left.y,
            half.x * scale * 2.,
            half.y * scale * 2.,
        );
        shapes::fill_rounded_rect(
            rect,
            (0.12 * scale).min(6.),
            Fill::vertical(
                rect,
                Color::new(0.20, 0.27, 0.36, 1.),
                Color::new(0.11, 0.15, 0.21, 1.),
            ),
        );
        draw_rectangle(
            rect.x + 2.,
            rect.y,
            rect.w - 4.,
            2.,
            Color::new(0.55, 0.68, 0.82, 0.8),
        );
    }
    // An emissive rail along the floor anchors the scene and blooms softly.
    let rail_a = to_screen(vec2(-12., 0.));
    let rail_b = to_screen(vec2(12., 0.));
    post::with_glow(|| {
        draw_rectangle(
            rail_a.x,
            rail_a.y - 1.,
            rail_b.x - rail_a.x,
            2.,
            Color::new(0.25, 0.75, 0.9, 0.55),
        );
    });

    let pose = |index: usize| {
        let body = &sandbox.bodies[index];
        let rigid = &sandbox.physics.bodies[body.handle];
        let position = body
            .previous_position
            .lerp(Vec2::from_array(rigid.translation().to_array()), f.alpha);
        let axis = *rigid.rotation() * d2::Vector::X;
        let now = axis.y.atan2(axis.x);
        let difference = (now - body.previous_angle + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        (position, body.previous_angle + difference * f.alpha)
    };

    // Soft contact shadows on the surface below each free body.
    for (index, body) in sandbox.bodies.iter().enumerate() {
        let (position, _) = pose(index);
        let extent = match body.shape {
            Shape::Ball(r) => r,
            Shape::Box(half) | Shape::Plank(half) => half.x.max(half.y * 0.6),
            Shape::Dumbbell => 0.9,
        };
        let surface = sandbox
            .platforms
            .iter()
            .filter(|(p, half)| {
                (position.x - p.x).abs() < half.x && position.y >= p.y + half.y && half.y < 2.
            })
            .map(|(p, half)| p.y + half.y)
            .fold(0_f32, f32::max);
        let height = position.y - surface - extent;
        if !(-0.2..6.).contains(&height) || position.x.abs() > 12. {
            continue;
        }
        let k = 1. - (height / 6.).clamp(0., 1.);
        let c = to_screen(vec2(position.x, surface));
        let half_w = extent * scale * (1.1 + 0.6 * (1. - k));
        shapes::fill_capsule(
            c + vec2(-half_w, 0.),
            c + vec2(half_w, 0.),
            (0.07 * scale).max(1.5),
            Color::new(0., 0., 0., 0.32 * k * k),
        );
    }

    // Joints: cables with glowing anchor lamps.
    for id in &sandbox.links {
        if let Some([a, b]) = sandbox.physics.joint_anchors(*id) {
            let a = to_screen(vec2(a.x, a.y));
            let b = to_screen(vec2(b.x, b.y));
            shapes::polyline(
                &[a, b],
                Stroke::new(2., Color::new(0.55, 0.64, 0.74, 0.9)).cap(Cap::Round),
            );
            shapes::glow_disc(a, 10., faded(AMBER, 0.55));
            shapes::fill_circle(a, 3.5, lighten(AMBER, 0.2));
        }
    }

    for (index, body) in sandbox.bodies.iter().enumerate() {
        let (position, angle) = pose(index);
        let center = to_screen(position);
        let rotation = Vec2::from_angle(angle);
        // Spawn pop-in scales the drawing only; the collider never changes.
        let pop = f.juice.body_scale(index);
        let flash = f.juice.body_flash(index);
        let base = tame(body.color);
        match body.shape {
            Shape::Dumbbell => {
                let a = to_screen(position + rotation.rotate(vec2(-0.75, 0.)));
                let b = to_screen(position + rotation.rotate(vec2(0.75, 0.)));
                shapes::fill_capsule(a, b, 0.1 * scale, Color::new(0.5, 0.58, 0.68, 1.));
                for p in [a, b] {
                    let r = 0.28 * scale;
                    shapes::fill_circle(
                        p,
                        r,
                        Fill::radial(
                            p + vec2(-r * 0.35, -r * 0.35),
                            r * 1.3,
                            lighten(base, 0.35),
                            darken(base, 0.25),
                        ),
                    );
                    shapes::stroke_circle(p, r, 1.5, darken(AMBER, 0.1));
                }
            }
            Shape::Ball(radius) => {
                let r = radius * scale * pop * (1. + 0.07 * flash);
                // Light from the upper left, regardless of spin.
                let light = center + vec2(-r * 0.38, -r * 0.42);
                shapes::fill_circle(
                    center,
                    r,
                    Fill::radial(light, r * 1.45, lighten(base, 0.45), darken(base, 0.35)),
                );
                shapes::stroke_circle(center, r - 0.75, 1.5, darken(base, 0.45));
                // Spin marker: a short groove that rotates with the body.
                let dir = vec2(rotation.x, -rotation.y);
                shapes::polyline(
                    &[center + dir * r * 0.25, center + dir * r * 0.7],
                    Stroke::new((r * 0.12).max(1.5), darken(base, 0.55)).cap(Cap::Round),
                );
                if flash > 0.01 {
                    shapes::glow_disc(center, r * 1.6, faded(WHITE_HOT, 0.5 * flash));
                }
            }
            Shape::Box(half) | Shape::Plank(half) => {
                let plank = matches!(body.shape, Shape::Plank(_));
                let corner =
                    |x: f32, y: f32| to_screen(position + rotation.rotate(vec2(x, y) * pop));
                let c = [
                    corner(-half.x, -half.y),
                    corner(half.x, -half.y),
                    corner(half.x, half.y),
                    corner(-half.x, half.y),
                ];
                let fill = shapes::mix(base, Color::new(1., 1., 1., 1.), flash * 0.6);
                draw_triangle(c[0], c[1], c[2], darken(fill, 0.12));
                draw_triangle(c[0], c[2], c[3], darken(fill, 0.12));
                // Inset face lit from above for a soft bevel.
                let inset = 0.8;
                let i = [
                    corner(-half.x * inset, -half.y * inset),
                    corner(half.x * inset, -half.y * inset),
                    corner(half.x * inset, half.y * inset),
                    corner(-half.x * inset, half.y * inset),
                ];
                draw_triangle(i[0], i[1], i[2], fill);
                draw_triangle(i[0], i[2], i[3], lighten(fill, 0.08));
                let edge = if plank {
                    Color::new(0.24, 0.21, 0.18, 1.)
                } else {
                    darken(base, 0.5)
                };
                shapes::polyline(&c, Stroke::new(1.5, edge).join(Join::Miter).closed(true));
                if plank {
                    let m0 = corner(-half.x, 0.);
                    let m1 = corner(half.x, 0.);
                    shapes::polyline(&[m0, m1], Stroke::new(1.2, edge));
                    for y in [-0.45, 0.45] {
                        shapes::fill_circle(
                            corner(0., y * half.y),
                            (half.x * 0.42 * scale).max(1.4),
                            edge,
                        );
                    }
                } else {
                    // Crate bracing.
                    shapes::polyline(&[i[0], i[2]], Stroke::new(1.2, darken(base, 0.35)));
                }
                if flash > 0.01 {
                    shapes::glow_disc(
                        center,
                        half.length() * scale * 1.6,
                        faded(WHITE_HOT, 0.45 * flash),
                    );
                }
            }
        }
        if lab.selected == Some(body.handle) {
            let radius = match body.shape {
                Shape::Ball(radius) => radius,
                Shape::Box(half) | Shape::Plank(half) => half.length(),
                Shape::Dumbbell => 1.03,
            };
            // A steady halo carries selection without flashing or shake.
            let r = radius * scale + 6.;
            shapes::glow_ring(center, r, 6., faded(CYAN, 0.6));
            shapes::stroke_circle(center, r, 1.5, look::INK);
        }
    }

    // Emissive effects feed the bloom.
    post::with_glow(|| {
        for particle in f.juice.particles.iter() {
            let p = to_screen(vec2(particle.position[0], particle.position[1]));
            let [r, g, b, a] = particle.color;
            draw_poly(
                p.x,
                p.y,
                4,
                (particle.size * scale).max(1.),
                particle.rotation.to_degrees(),
                Color::new(r * 1.4, g * 1.4, b * 1.4, a),
            );
        }
    });
    if !ferravex_render::preferences::feedback_policy().reduced_flash() {
        for spark in &lab.fun.events.sparks {
            let p = to_screen(spark.at);
            post::glow_circle(
                p.x,
                p.y,
                0.45 * scale,
                GOLD,
                (spark.strength / 12.).clamp(0.1, 1.),
            );
        }
    }
    if let Some((center, age)) = f.ring {
        let p = to_screen(center);
        let k = (1. - age / 0.65).max(0.) * f.ring_intensity;
        let r = age * 12. * scale;
        shapes::glow_ring(
            p,
            r,
            (0.5 * scale).max(6.),
            Color::new(0.4, 0.9, 1., 0.9 * k),
        );
        shapes::stroke_circle(p, r, 1.5, Color::new(0.8, 1., 1., 0.8 * k));
    }

    if let Some((at, ball)) = f.preview
        && sandbox.bodies.len() < MAX_BODIES
    {
        let p = to_screen(at);
        let floor = to_screen(vec2(at.x, 0.));
        // Dotted guide down to the floor, then a landing tick.
        let mut y = p.y + 0.6 * scale;
        while y < floor.y - 4. {
            draw_rectangle(p.x - 1., y, 2., 4., Color::new(0.6, 0.8, 1., 0.22));
            y += 10.;
        }
        let tick = Rect::new(floor.x - 10., floor_y - 2., 20., 3.);
        shapes::fill_rounded_rect(tick, 1.5, faded(CYAN, 0.5));
        let color = if ball { CYAN } else { AMBER };
        let ghost = faded(color, 0.18 + 0.06 * breathe.abs());
        if ball {
            let r = 0.44 * scale;
            shapes::fill_circle(p, r, ghost);
            shapes::stroke_circle(p, r, 1.5, faded(color, 0.8));
        } else {
            let r = Rect::new(
                p.x - 0.48 * scale,
                p.y - 0.48 * scale,
                0.96 * scale,
                0.96 * scale,
            );
            shapes::fill_rounded_rect(r, 3., ghost);
            shapes::stroke_rounded_rect(r, 3., 1.5, faded(color, 0.8));
        }
        shapes::glow_disc(p, 0.9 * scale, faded(color, 0.18));
    }
    let _ = GOLD;
}
