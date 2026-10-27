//! Course rendering for both views plus the shared screen-space world pass:
//! backdrop, lawn texture, hazards, soft shadows, the glowing cup, the aim
//! line, ambient pollen and juice. Everything here is drawn before the
//! post-processing HUD boundary, so emissive accents feed the bloom.
use super::*;
use ferravex_render::{
    post,
    shapes::{self, Fill, Shadow, Stroke},
    text::{FontFace, TextAlign, TextStyle as Type, draw_text_styled},
};

const STONE: Color = Color::from_rgba(226, 214, 182, 255);
const STONE_SIDE: Color = Color::from_rgba(176, 162, 128, 255);
const HEDGE: Color = Color::from_rgba(46, 86, 56, 255);
const WATER_TOP: Color = Color::from_rgba(92, 172, 196, 255);
const WATER_DEEP: Color = Color::from_rgba(44, 112, 146, 255);
const SAND: Color = Color::from_rgba(236, 202, 128, 255);
const CUP_GLOW: Color = Color::new(1., 0.82, 0.45, 1.);

fn shadow(a: f32) -> Color {
    Color::new(0.02, 0.06, 0.04, a)
}

/// Full-screen night-garden gradient behind both views.
pub(super) fn backdrop(w: f32, h: f32, center: Vec2, reach: f32) {
    let full = Rect::new(0., 0., w, h);
    shapes::fill_rounded_rect(
        full,
        0.,
        Fill::vertical(
            full,
            Color::from_rgba(30, 62, 50, 255),
            Color::from_rgba(13, 30, 25, 255),
        ),
    );
    shapes::fill_circle(
        center,
        reach,
        Fill::radial(
            center,
            reach,
            Color::new(0.35, 0.55, 0.38, 0.28),
            Color::new(0.35, 0.55, 0.38, 0.),
        ),
    );
}

/// A circle on the green, projected so it reads as flat in both views.
pub(super) fn ground_ring(
    p: &Projector,
    at: Vec2,
    ground: f32,
    radius: f32,
    width: f32,
    color: Color,
) {
    let mut points = [Vec2::ZERO; 32];
    for (k, q) in points.iter_mut().enumerate() {
        let a = k as f32 / 32. * std::f32::consts::TAU;
        *q = p.screen(vec3(
            at.x + radius * a.cos(),
            ground,
            at.y + radius * a.sin(),
        ));
    }
    shapes::polyline(&points, Stroke::new(width, color).closed(true));
}

impl App {
    /// Shared world-space accents for both views, still inside the post pass.
    pub(super) fn draw_world_overlay(&self, p: &Projector) {
        let ppm = p.ppm();
        let course = &self.round.course;
        // Water glints: tiny additive sparkles drifting with the rules clock.
        for (i, patch) in course
            .patches
            .iter()
            .filter(|s| s.surface == Surface::Water)
            .enumerate()
        {
            for k in 0..6 {
                let seed = (i * 7 + k) as f32;
                let u = ((seed * 0.37 + self.ambient * 0.05).fract() + 1.).fract();
                let v = (seed * 0.61).fract();
                let at = patch.min + patch.size * vec2(0.1 + 0.8 * v, 0.1 + 0.8 * u);
                let twinkle = (0.5 + 0.5 * (self.ambient * 2.3 + seed).sin()).powi(3);
                let q = p.screen(vec3(at.x, 0.03, at.y));
                shapes::glow_disc(q, ppm * 0.22, Color::new(0.8, 0.95, 1., 0.55 * twinkle));
            }
        }
        // The cup breathes a warm light so the target always reads first.
        let cup = course.cup;
        let q = p.screen(vec3(cup.x, course.height(cup), cup.y));
        let breathe = 0.85 + 0.15 * (self.ambient * 2.).sin();
        let done = matches!(self.round.phase, Phase::Holed | Phase::Finished);
        let boost = if done { 1.6 } else { 1. };
        shapes::glow_disc(q, ppm * 1.1 * breathe, fade(CUP_GLOW, 0.32 * boost));
        post::glow_circle(q.x, q.y, ppm * 0.42, CUP_GLOW, 0.55 * breathe * boost);
        if self.round.phase == Phase::Aiming && !self.modal_open() {
            self.draw_aim(p);
        }
        // Ambient pollen over the green (frozen under reduced motion).
        for i in 0..scenery::MOTES {
            let (at, twinkle) = self.scenery.mote(i, self.ambient);
            let q = p.screen(at);
            shapes::glow_disc(
                q,
                ppm * 0.16,
                Color::new(1., 0.93, 0.7, 0.35 + 0.35 * twinkle),
            );
        }
        self.draw_juice(p);
    }

    /// Dotted aim preview that grows and warms with power, plus a target ring.
    fn draw_aim(&self, p: &Projector) {
        let ppm = p.ppm();
        let length = preview_length(self.power);
        let color = power_color(self.power);
        let spacing = (9. / ppm).max(0.26);
        let steps = (length / spacing).ceil() as usize;
        let lift = |g: Vec2| self.round.course.height(g) + 0.03;
        for i in 1..=steps {
            let d = (i as f32 * spacing).min(length);
            let t = d / length;
            let g = self.round.ball + self.aim * d;
            let q = p.screen(vec3(g.x, lift(g), g.y));
            let r = if i % 3 == 0 { 3. } else { 2.2 };
            shapes::fill_circle(q, r + 1., shadow(0.25 * (1. - 0.5 * t)));
            shapes::fill_circle(q, r, fade(mix(PAPER, color, t), 1. - 0.45 * t));
        }
        let end = self.round.ball + self.aim * length;
        let pulse = 0.2 + 0.03 * (self.ambient * 5.).sin() * self.juice.motion();
        ground_ring(p, end, lift(end), pulse, 2., fade(color, 0.95));
        let q = p.screen(vec3(end.x, lift(end), end.y));
        shapes::glow_disc(q, ppm * 0.35, fade(color, 0.35));
    }

    /// Trail, rings, particles and callouts from accepted events.
    pub(super) fn draw_juice(&self, p: &Projector) {
        let ppm = p.ppm();
        for (i, &(at, age)) in self.juice.trail[..self.juice.trail_len].iter().enumerate() {
            let a = (1. - age / 0.4).clamp(0., 1.) * (i + 1) as f32 / juice::TRAIL as f32;
            let q = p.screen(vec3(at.x, self.round.course.height(at) + RADIUS, at.y));
            shapes::glow_disc(q, (RADIUS * ppm * 1.4).max(3.), fade(PAPER, 0.45 * a));
        }
        for ring in self.juice.rings.iter().flatten() {
            let t = ring.age / ring.life;
            let r = ring.radius * (0.3 + 0.7 * ease_out(t));
            let color = fade(ring.color, 1. - t);
            let ground = self.round.course.height(ring.at) + 0.03;
            ground_ring(p, ring.at, ground, r, 2.5 * (1. - t) + 1., color);
        }
        for v in self.juice.particles.iter() {
            let q = p.screen(Vec3::from(v.position));
            let s = (v.size * ppm).max(1.5);
            let c = Color::new(
                v.color[0],
                v.color[1],
                v.color[2],
                (v.color[3] * 1.8).min(1.),
            );
            draw_poly(q.x, q.y, 4, s, v.rotation.to_degrees(), c);
        }
        for pop in self.juice.pops.iter().flatten() {
            let t = pop.age / pop.life;
            let rise = if self.juice.motion() > 0. {
                ease_out(t) * 0.9
            } else {
                0.
            };
            let q = p.screen(pop.at + vec3(0., rise, 0.)) - vec2(0., f32::from(pop.row) * 24.);
            let a = ((pop.life - pop.age) / 0.3).clamp(0., 1.) * (pop.age / 0.08).min(1.);
            // Shouted callouts keep the arcade face; sentences read modern.
            let shout = !pop.text.chars().any(char::is_lowercase);
            let size = if shout { pop.size.round() } else { 16. };
            let style = Type::modern(size, fade(pop.color, a))
                .with_face(if shout {
                    FontFace::Classic
                } else {
                    FontFace::Modern
                })
                .with_outline(fade(DARK, 0.9 * a))
                .with_shadow(shadow(0.5 * a))
                .with_align(TextAlign::Center);
            let half = ferravex_render::text::text_width(&pop.text, style) * 0.5 + 6.;
            let x = q.x.clamp(half, (p.w - half).max(half));
            let y = q.y.clamp(size + 4., p.h - 4.);
            draw_text_styled(&pop.text, x, y, style);
        }
    }

    pub(super) fn draw_2d(&self, scene: Scene) {
        let scale = top_scale(scene);
        let origin = top_map(scene, Vec2::ZERO);
        let size = SIZE * scale;
        let green = Rect::new(origin.x, origin.y, size.x, size.y);
        let rail = scale * 0.3;
        let frame = Rect::new(
            green.x - rail,
            green.y - rail,
            green.w + 2. * rail,
            green.h + 2. * rail,
        );
        // Hedges and flowers outside the rails, with soft contact shadows.
        for z in [-0.78, 12.78] {
            for x in [0.4, 2.2, 4., 5.8, 7.6, 9.4, 11.2, 13., 14.8, 16.6] {
                let c = top_map(scene, vec2(x + 0.5, z));
                let r = scale * 0.48;
                shapes::fill_circle(
                    c + vec2(0., r * 0.35),
                    r * 1.05,
                    Fill::radial(c + vec2(0., r * 0.35), r * 1.05, shadow(0.35), shadow(0.)),
                );
                shapes::fill_circle(
                    c,
                    r,
                    Fill::radial(
                        c - vec2(r * 0.35, r * 0.4),
                        r * 1.4,
                        Color::from_rgba(78, 128, 82, 255),
                        HEDGE,
                    ),
                );
            }
        }
        for f in &self.scenery.flowers {
            let c = top_map(scene, f.at);
            shapes::fill_circle(c, (scale * 0.07).max(1.6), f.color);
        }
        shapes::drop_shadow(
            frame,
            rail * 1.4,
            Shadow::soft(shadow(0.6))
                .offset(vec2(0., scale * 0.35))
                .blur(scale * 0.9),
        );
        shapes::fill_rounded_rect(frame, rail * 1.4, Fill::vertical(frame, STONE, STONE_SIDE));
        shapes::fill_rounded_rect(
            green,
            rail * 0.5,
            Fill::vertical(
                green,
                Color::from_rgba(116, 172, 116, 255),
                Color::from_rgba(92, 146, 96, 255),
            ),
        );
        // Mowing stripes, two metres wide, as soft translucent bands.
        for x in (0..18).step_by(2) {
            draw_rectangle(
                origin.x + x as f32 * scale,
                origin.y,
                scale,
                size.y,
                Color::new(1., 1., 1., 0.045),
            );
        }
        // Inner rail shadow along the top and left edges gives the green depth.
        let lip = scale * 0.45;
        shapes::fill_rounded_rect(
            Rect::new(green.x, green.y, green.w, lip),
            0.,
            Fill::vertical(
                Rect::new(green.x, green.y, green.w, lip),
                shadow(0.32),
                shadow(0.),
            ),
        );
        shapes::fill_rounded_rect(
            Rect::new(green.x, green.y, lip, green.h),
            0.,
            Fill::horizontal(
                Rect::new(green.x, green.y, lip, green.h),
                shadow(0.22),
                shadow(0.),
            ),
        );
        // Grass tufts: two short blades each, light or dark by their shade.
        for tuft in &self.scenery.tufts {
            let c = top_map(scene, tuft.at);
            let len = scale * 0.16;
            let col = if tuft.shade > 0. {
                Color::new(0.75, 0.9, 0.6, 0.22 * tuft.shade)
            } else {
                Color::new(0.1, 0.25, 0.12, -0.28 * tuft.shade)
            };
            for side in [-1., 1.] {
                let tip = c + vec2(side * len * 0.35 + tuft.lean * len, -len);
                draw_line(c.x + side, c.y, tip.x, tip.y, (scale * 0.035).max(1.), col);
            }
        }
        for p in &self.round.course.patches {
            let a = top_map(scene, p.min);
            let r = Rect::new(a.x, a.y, p.size.x * scale, p.size.y * scale);
            match p.surface {
                Surface::Water => {
                    shapes::fill_rounded_rect(
                        r,
                        scale * 0.18,
                        Fill::vertical(r, WATER_TOP, WATER_DEEP),
                    );
                    for j in 1..6 {
                        let yy = r.y + r.h * j as f32 / 6.;
                        let drift = (self.round.time * 1.3 + j as f32).sin() * scale * 0.12;
                        draw_line(
                            r.x + scale * 0.2 + drift.max(0.),
                            yy,
                            r.x + r.w - scale * 0.2 + drift.min(0.),
                            yy,
                            1.5,
                            Color::new(1., 1., 1., 0.14),
                        );
                    }
                    shapes::stroke_rounded_rect(
                        r,
                        scale * 0.18,
                        2.,
                        Color::new(0.85, 0.95, 1., 0.35),
                    );
                }
                Surface::Sand => {
                    let c = vec2(r.x + r.w * 0.45, r.y + r.h * 0.4);
                    shapes::fill_rounded_rect(
                        r,
                        scale * 0.5,
                        Fill::radial(
                            c,
                            r.w.max(r.h) * 0.75,
                            SAND,
                            Color::from_rgba(214, 172, 98, 255),
                        ),
                    );
                    for &(g, tone) in &self.scenery.grains {
                        let q = top_map(scene, g);
                        let col = if tone > 0. {
                            Color::new(1., 0.95, 0.8, 0.5 * tone)
                        } else {
                            Color::new(0.55, 0.4, 0.2, -0.4 * tone)
                        };
                        draw_rectangle(q.x, q.y, 2., 2., col);
                    }
                }
                Surface::Slope => {
                    let c = vec2(r.x + r.w * 0.5, r.y + r.h * 0.5);
                    shapes::fill_rounded_rect(
                        r,
                        scale * 0.6,
                        Fill::radial(
                            c - vec2(r.w * 0.08, r.h * 0.12),
                            r.w.max(r.h) * 0.6,
                            Color::new(0.85, 1., 0.7, 0.28),
                            Color::new(0.2, 0.4, 0.2, 0.08),
                        ),
                    );
                    for k in 1..4 {
                        let f = k as f32 / 4.;
                        let mut points = [Vec2::ZERO; 28];
                        for (i, q) in points.iter_mut().enumerate() {
                            let a = i as f32 / 28. * std::f32::consts::TAU;
                            *q = c + vec2(a.cos() * r.w, a.sin() * r.h) * 0.42 * f;
                        }
                        shapes::polyline(
                            &points,
                            Stroke::new(1.5, Color::new(1., 1., 1., 0.12)).closed(true),
                        );
                    }
                }
            }
        }
        let tee = top_map(scene, self.round.course.tee);
        shapes::fill_rounded_rect(
            Rect::new(
                tee.x - scale * 0.45,
                tee.y - scale * 0.45,
                scale * 0.9,
                scale * 0.9,
            ),
            scale * 0.2,
            Color::new(0.1, 0.25, 0.12, 0.18),
        );
        shapes::stroke_circle(tee, scale * 0.34, 1.5, Color::new(0.96, 0.94, 0.83, 0.45));
        for &wall in &self.round.course.walls {
            let (c, a) = wall.pose(self.round.time);
            let r = Vec2::from_angle(a);
            let moving = wall.spin != 0. || wall.travel != Vec2::ZERO;
            let corners = |half: Vec2| {
                [
                    vec2(-half.x, -half.y),
                    vec2(half.x, -half.y),
                    vec2(half.x, half.y),
                    vec2(-half.x, half.y),
                ]
                .map(|q| top_map(scene, c + r.rotate(q)))
            };
            let v = corners(wall.half);
            let off = vec2(scale * 0.12, scale * 0.2);
            for (i, j, k) in [(0, 1, 2), (0, 2, 3)] {
                draw_triangle(v[i] + off, v[j] + off, v[k] + off, shadow(0.35));
            }
            let (side, top) = if moving {
                (Color::from_rgba(150, 78, 44, 255), COPPER)
            } else {
                (STONE_SIDE, STONE)
            };
            for (i, j, k) in [(0, 1, 2), (0, 2, 3)] {
                draw_triangle(v[i], v[j], v[k], side);
            }
            let inset = (wall.half - Vec2::splat(0.06)).max(Vec2::splat(0.02));
            let t = corners(inset).map(|q| q - vec2(0., scale * 0.05));
            for (i, j, k) in [(0, 1, 2), (0, 2, 3)] {
                draw_triangle(t[i], t[j], t[k], top);
            }
            let edge = [t[0], t[1], t[2], t[3]];
            let rim = if moving {
                Color::new(1., 0.75, 0.5, 0.75)
            } else {
                Color::new(1., 1., 0.95, 0.45)
            };
            shapes::polyline(&edge, Stroke::new(1.2, rim).closed(true));
            if moving {
                // Copper machinery is emissive: a faint warm sheen feeds bloom.
                post::with_glow(|| {
                    shapes::polyline(
                        &edge,
                        Stroke::new(2., Color::new(0.9, 0.45, 0.2, 0.25)).closed(true),
                    )
                });
            }
        }
        let cup = top_map(scene, self.round.course.cup);
        shapes::fill_circle(cup, scale * 0.42, PAPER);
        shapes::fill_circle(
            cup,
            scale * 0.32,
            Fill::radial(
                cup - vec2(0., scale * 0.08),
                scale * 0.32,
                Color::from_rgba(8, 20, 16, 255),
                Color::from_rgba(34, 58, 46, 255),
            ),
        );
        let sway = (self.ambient * 2.2).sin() * 1.5 * self.juice.motion();
        let wave = self.juice.flag * (self.juice.time * 14.).sin() * 5. + sway;
        let top = (scale * 1.1).max(24.);
        let pole_shadow = cup + vec2(top * 0.45, top * 0.12);
        shapes::polyline(&[cup, pole_shadow], Stroke::new(2., shadow(0.3)));
        shapes::polyline(&[cup, cup - vec2(0., top)], Stroke::new(2.2, PAPER));
        let flag = [
            cup - vec2(0., top),
            cup + vec2(top * 0.58, -top * 0.82 + wave),
            cup - vec2(0., top * 0.58),
        ];
        draw_triangle(flag[0], flag[1], flag[2], COPPER);
        shapes::polyline(
            &flag,
            Stroke::new(1., Color::new(1., 0.8, 0.6, 0.6)).closed(true),
        );
        let sink = self.juice.sink_scale();
        let show = match self.round.phase {
            Phase::Water => false,
            Phase::Holed | Phase::Finished => self.juice.sink.is_some() && sink > 0.,
            _ => true,
        };
        if show {
            let b = top_map(scene, self.round.ball);
            let r = (RADIUS * scale).max(3.5)
                * if self.round.phase == Phase::Holed {
                    sink
                } else {
                    1.
                };
            if self.round.phase == Phase::Aiming {
                self.draw_putter_2d(scene, scale);
            }
            let sh = b + vec2(r * 0.45, r * 0.7);
            shapes::fill_circle(
                sh,
                r * 1.35,
                Fill::radial(sh, r * 1.35, shadow(0.45), shadow(0.)),
            );
            let s = self.juice.ball_scale();
            if (s - Vec2::ONE).abs().max_element() > 0.02 {
                let rot = self.juice.squash_dir.to_angle().to_degrees();
                draw_ellipse(b.x, b.y, r * s.x, r * s.y, rot, PAPER);
            } else {
                shapes::fill_circle(
                    b,
                    r,
                    Fill::radial(
                        b - vec2(r * 0.35, r * 0.4),
                        r * 1.5,
                        Color::new(1., 1., 1., 1.),
                        Color::from_rgba(214, 206, 186, 255),
                    ),
                );
            }
        }
    }

    fn draw_putter_2d(&self, scene: Scene, scale: f32) {
        // The putter head pulls back further with more power.
        let back = 0.45 + self.power * 0.55;
        let p = top_map(scene, self.round.ball - self.aim * back);
        let t = vec2(-self.aim.y, self.aim.x) * scale * 0.35;
        let grip = top_map(scene, self.round.ball - self.aim * (back + 1.2));
        shapes::polyline(&[p, grip], Stroke::new(2., fade(PAPER, 0.6)));
        shapes::fill_capsule(p - t, p + t, 3., COPPER);
    }

    pub(super) fn draw_3d(&self, scene: Scene, w: f32, h: f32) {
        let cam = camera(scene, w, h);
        set_camera(&cam);
        self.material.begin(ferravex_render::shaders::LitSettings {
            camera_position: cam.eye,
            ambient: 0.58,
            fog_start: 100.,
            fog_end: 200.,
            ..Default::default()
        });
        if let Some(course) = &self.scenery.course {
            course.draw_transformed(Mat4::IDENTITY);
        }
        if let Some(terrain) = &self.terrain {
            terrain.draw(Vec3::ZERO, Quat::IDENTITY);
        }
        for wall in &self.round.course.walls {
            let (c, a) = wall.pose(self.round.time);
            let mesh = if wall.spin != 0. || wall.travel != Vec2::ZERO {
                &self.copper_mesh
            } else {
                &self.stone_mesh
            };
            let model = Mat4::from_scale_rotation_translation(
                vec3(wall.half.x * 2., 0.65, wall.half.y * 2.),
                Quat::from_rotation_y(-a),
                vec3(c.x, 0.325, c.y),
            );
            mesh.draw_transformed(model);
        }
        let cup = self.round.course.cup;
        self.cup_mesh
            .draw(vec3(cup.x, 0.025, cup.y), Quat::IDENTITY);
        self.post_mesh.draw(vec3(cup.x, 0.8, cup.y), Quat::IDENTITY);
        let sway = (self.ambient * 2.2).sin() * 0.06 * self.juice.motion();
        let wave = self.juice.flag * (self.juice.time * 14.).sin() * 0.3 + sway;
        self.copper_mesh
            .draw_transformed(Mat4::from_scale_rotation_translation(
                vec3(0.55, 0.3, 0.04),
                Quat::from_rotation_y(wave * 0.6),
                vec3(cup.x + 0.28, 1.45, cup.y + wave * 0.2),
            ));
        if self.round.phase == Phase::Aiming {
            let back = 0.45 + self.power * 0.55;
            let p = self.round.ball - self.aim * back;
            let yaw = self.aim.y.atan2(self.aim.x);
            self.copper_mesh
                .draw_transformed(Mat4::from_scale_rotation_translation(
                    vec3(0.1, 0.12, 0.5),
                    Quat::from_rotation_y(-yaw),
                    vec3(p.x, self.round.course.height(p) + 0.08, p.y),
                ));
        }
        let show = match self.round.phase {
            Phase::Water => false,
            Phase::Holed | Phase::Finished => {
                self.juice.sink.is_some() && self.juice.sink_scale() > 0.
            }
            _ => true,
        };
        if show {
            let b = self.round.ball;
            let mut y = self.round.course.height(b);
            if self.round.phase == Phase::Holed {
                y -= (1. - self.juice.sink_scale()) * 0.35;
            } else {
                self.shadow_mesh
                    .draw(vec3(b.x + 0.05, y + 0.006, b.y + 0.05), Quat::IDENTITY);
            }
            let s = self.juice.ball_scale();
            let dir = self.juice.squash_dir;
            let yaw = Quat::from_rotation_y(-dir.y.atan2(dir.x));
            let model = Mat4::from_translation(vec3(b.x, y + RADIUS * s.y.min(1.), b.y))
                * Mat4::from_quat(yaw)
                * Mat4::from_scale(vec3(s.x, s.y, s.y))
                * Mat4::from_quat(yaw.inverse());
            self.ball_mesh.draw_transformed(model);
        }
        self.material.end();
        set_default_camera();
    }
}

pub(super) fn terrain_mesh(course: &course::Hole) -> Option<Mesh3d> {
    let patch = course
        .patches
        .iter()
        .find(|p| p.surface == Surface::Slope)?;
    let mut vertices = Vec::with_capacity(25 * 17);
    let mut indices = Vec::with_capacity(24 * 16 * 6);
    for z in 0..=16 {
        for x in 0..=24 {
            let p = patch.min + patch.size * vec2(x as f32 / 24., z as f32 / 16.);
            let g = course.slope(p);
            // Crest tint: higher ground catches more light.
            let lift = (course.height(p) / 0.65).clamp(0., 1.);
            let tone = mix(
                Color::from_rgba(102, 158, 100, 255),
                Color::from_rgba(150, 196, 120, 255),
                lift,
            );
            let mut v = Vertex::new2(vec3(p.x, course.height(p) + 0.025, p.y), Vec2::ZERO, tone);
            v.normal = vec3(-g.x, 1., -g.y).normalize().extend(0.);
            vertices.push(v);
        }
    }
    for z in 0..16 {
        for x in 0..24 {
            let a = (z * 25 + x) as u16;
            indices.extend_from_slice(&[a, a + 25, a + 1, a + 1, a + 25, a + 26]);
        }
    }
    Some(Mesh3d::from_mesh(Mesh {
        vertices,
        indices,
        texture: None,
    }))
}
