//! Course rendering for both views plus the shared screen-space juice pass.
use super::*;

impl App {
    /// Screen-space effects shared by both views.
    pub(super) fn draw_juice(&self, p: &Projector) {
        let ppm = p.ppm();
        if self.round.phase == Phase::Aiming && !self.modal_open() {
            // Aim target ring, projected onto the green in either view.
            let end = self.round.ball + self.aim * preview_length(self.power);
            let ground = self.round.course.height(end) + 0.03;
            let color = fade(power_color(self.power), 0.9);
            let pulse = 0.2 + 0.03 * (self.round.time * 5.).sin() * self.juice.motion();
            ground_ring(p, end, ground, pulse, 2., color);
        }
        for (i, &(at, age)) in self.juice.trail[..self.juice.trail_len].iter().enumerate() {
            let a = (1. - age / 0.4).clamp(0., 1.) * (i + 1) as f32 / juice::TRAIL as f32;
            let q = p.screen(vec3(at.x, self.round.course.height(at) + RADIUS, at.y));
            draw_circle(
                q.x,
                q.y,
                (RADIUS * ppm * 0.7).max(1.5),
                fade(PAPER, 0.35 * a),
            );
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
            let size = pop.size;
            let width = pop.text.chars().count() as f32 * size * 0.75;
            let x = (q.x - width * 0.5).clamp(6., (p.w - width - 6.).max(6.));
            let y = q.y.clamp(size + 4., p.h - 4.);
            draw_text(&pop.text, x + 2., y + 2., size, fade(DARK, 0.85 * a));
            draw_text(&pop.text, x, y, size, fade(pop.color, a));
        }
    }
    pub(super) fn draw_aim_2d(&self, scene: Scene, scale: f32) {
        let length = preview_length(self.power);
        let color = power_color(self.power);
        let spacing = (8. / scale).max(0.26);
        let steps = (length / spacing).ceil() as usize;
        for i in 1..=steps {
            let d = i as f32 * spacing;
            let t = d / length;
            let p = top_map(scene, self.round.ball + self.aim * d.min(length));
            let r = if i % 3 == 0 { 2.6 } else { 1.8 };
            draw_circle(p.x, p.y, r, fade(mix(PAPER, color, t), 1. - 0.55 * t));
        }
        // The putter head pulls back further with more power.
        let back = 0.45 + self.power * 0.55;
        let p = top_map(scene, self.round.ball - self.aim * back);
        let t = vec2(-self.aim.y, self.aim.x) * scale * 0.35;
        draw_line(p.x - t.x, p.y - t.y, p.x + t.x, p.y + t.y, 5., COPPER);
        let grip = top_map(scene, self.round.ball - self.aim * (back + 1.2));
        draw_line(p.x, p.y, grip.x, grip.y, 2., fade(PAPER, 0.6));
    }
    pub(super) fn draw_2d(&self, scene: Scene) {
        let scale = top_scale(scene);
        let origin = top_map(scene, Vec2::ZERO);
        draw_rectangle(
            origin.x - 6.,
            origin.y - 4.,
            SIZE.x * scale + 12.,
            SIZE.y * scale + 12.,
            Color::from_rgba(18, 39, 31, 255),
        );
        for x in 0..18 {
            draw_rectangle(
                origin.x + x as f32 * scale,
                origin.y,
                scale,
                SIZE.y * scale,
                if x % 2 == 0 {
                    GREEN
                } else {
                    Color::from_rgba(91, 143, 98, 255)
                },
            );
        }
        for p in &self.round.course.patches {
            let a = top_map(scene, p.min);
            let col = match p.surface {
                Surface::Water => Color::from_rgba(62, 134, 157, 255),
                Surface::Sand => GOLD,
                Surface::Slope => Color::from_rgba(122, 169, 105, 255),
            };
            draw_rectangle(a.x, a.y, p.size.x * scale, p.size.y * scale, col);
            for j in 1..5 {
                let yy = a.y + p.size.y * scale * j as f32 / 5.;
                // Water ripples drift with the rules clock; still when paused.
                let drift = if p.surface == Surface::Water {
                    (self.round.time * 1.3 + j as f32).sin() * 4.
                } else {
                    0.
                };
                draw_line(
                    a.x + 3. + drift.max(0.),
                    yy,
                    a.x + p.size.x * scale - 3. + drift.min(0.),
                    yy,
                    1.,
                    Color::new(1., 1., 1., 0.12),
                );
            }
        }
        for i in 0..12 {
            let p = top_map(scene, vec2(0.6 + i as f32 * 1.5, -0.55));
            draw_circle(p.x, p.y, scale * 0.19, Color::from_rgba(73, 102, 66, 255));
            let p = top_map(scene, vec2(0.6 + i as f32 * 1.5, 12.55));
            draw_circle(p.x, p.y, scale * 0.19, Color::from_rgba(73, 102, 66, 255));
        }
        for z in [-0.75, 12.75] {
            for x in [2., 5., 8., 11., 14., 17.] {
                let p = top_map(scene, vec2(x, z));
                draw_circle(p.x, p.y, scale * 0.28, Color::from_rgba(47, 82, 54, 255));
                for v in [vec2(-0.1, -0.05), vec2(0.12, 0.1)] {
                    let q = p + v * scale;
                    draw_circle(q.x, q.y, (scale * 0.075).max(1.2), GOLD);
                }
            }
        }
        let tee = top_map(scene, self.round.course.tee);
        draw_circle_lines(
            tee.x,
            tee.y,
            scale * 0.34,
            1.,
            Color::new(0.96, 0.94, 0.83, 0.4),
        );
        for &wall in &self.round.course.walls {
            let (c, a) = wall.pose(self.round.time);
            let r = Vec2::from_angle(a);
            let v = [
                vec2(-wall.half.x, -wall.half.y),
                vec2(wall.half.x, -wall.half.y),
                vec2(wall.half.x, wall.half.y),
                vec2(-wall.half.x, wall.half.y),
            ]
            .map(|q| top_map(scene, c + r.rotate(q)));
            let col = if wall.spin != 0. || wall.travel != Vec2::ZERO {
                COPPER
            } else {
                Color::from_rgba(217, 207, 175, 255)
            };
            draw_triangle(
                v[0] + vec2(2., 3.),
                v[1] + vec2(2., 3.),
                v[2] + vec2(2., 3.),
                DARK,
            );
            draw_triangle(
                v[0] + vec2(2., 3.),
                v[2] + vec2(2., 3.),
                v[3] + vec2(2., 3.),
                DARK,
            );
            draw_triangle(v[0], v[1], v[2], col);
            draw_triangle(v[0], v[2], v[3], col);
        }
        let cup = top_map(scene, self.round.course.cup);
        draw_circle(cup.x, cup.y, scale * 0.4, PAPER);
        draw_circle(cup.x, cup.y, scale * 0.3, DARK);
        let wave = self.juice.flag * (self.juice.time * 14.).sin() * 5.;
        let top = (scale * 1.05).max(22.);
        draw_line(cup.x, cup.y, cup.x, cup.y - top, 2., PAPER);
        draw_triangle(
            vec2(cup.x, cup.y - top),
            vec2(cup.x + top * 0.55, cup.y - top * 0.82 + wave),
            vec2(cup.x, cup.y - top * 0.6),
            COPPER,
        );
        if self.round.phase == Phase::Aiming {
            self.draw_aim_2d(scene, scale);
        }
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
            let s = self.juice.ball_scale();
            let rot = self.juice.squash_dir.to_angle().to_degrees();
            draw_ellipse(
                b.x + 2.,
                b.y + 3.,
                r * s.x,
                r * s.y,
                rot,
                Color::new(0., 0., 0., 0.24),
            );
            draw_ellipse(b.x, b.y, r * s.x, r * s.y, rot, PAPER);
            draw_circle(b.x - r * 0.3, b.y - r * 0.3, (r * 0.3).max(1.), WHITE);
        }
    }
    pub(super) fn draw_3d(&self, scene: Scene, w: f32, h: f32) {
        let cam = camera(scene, w, h);
        set_camera(&cam);
        self.material.begin(ferravex_render::shaders::LitSettings {
            camera_position: cam.eye,
            ambient: 0.55,
            fog_start: 100.,
            fog_end: 200.,
            ..Default::default()
        });
        draw_cube(vec3(9., -0.42, 6.), vec3(19., 0.7, 13.), None, DARK);
        for x in 0..18 {
            draw_cube(
                vec3(x as f32 + 0.5, -0.05, 6.),
                vec3(1., 0.1, 12.),
                None,
                if x % 2 == 0 {
                    GREEN
                } else {
                    Color::from_rgba(91, 143, 98, 255)
                },
            );
        }
        // The raised surface is tessellated from the exact shared heightfield.
        for patch in &self.round.course.patches {
            let p = patch;
            let col = match p.surface {
                Surface::Water => Color::from_rgba(53, 121, 146, 255),
                Surface::Sand => GOLD,
                Surface::Slope => Color::from_rgba(122, 169, 105, 255),
            };
            if p.surface == Surface::Slope {
                // Prepared terrain below samples the same heightfield as physics.
            } else {
                draw_cube(
                    vec3(p.min.x + p.size.x * 0.5, 0.015, p.min.y + p.size.y * 0.5),
                    vec3(p.size.x, 0.035, p.size.y),
                    None,
                    col,
                );
            }
        }
        if let Some(terrain) = &self.terrain {
            terrain.draw(Vec3::ZERO, Quat::IDENTITY);
        }
        for (x, z, sx, sz) in [
            (9., -0.16, 18.6, 0.3),
            (9., 12.16, 18.6, 0.3),
            (-0.16, 6., 0.3, 12.),
            (18.16, 6., 0.3, 12.),
        ] {
            draw_cube(
                vec3(x, 0.18, z),
                vec3(sx, 0.36, sz),
                None,
                Color::from_rgba(214, 203, 169, 255),
            );
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
        let wave = self.juice.flag * (self.juice.time * 14.).sin() * 0.25;
        draw_cube(
            vec3(cup.x + 0.28, 1.45, cup.y),
            vec3(0.55, 0.3, 0.04),
            None,
            COPPER,
        );
        if wave != 0. {
            draw_cube(
                vec3(cup.x + 0.5, 1.45 + wave * 0.3, cup.y + wave),
                vec3(0.12, 0.26, 0.05),
                None,
                COPPER,
            );
        }
        for i in 0..10 {
            let x = 0.6 + i as f32 * 1.85;
            for z in [-0.7, 12.7] {
                draw_cube(
                    vec3(x, 0.12, z),
                    vec3(0.4, 0.25, 0.4),
                    None,
                    Color::from_rgba(61, 91, 58, 255),
                );
            }
        }
        if self.round.phase == Phase::Aiming {
            let length = preview_length(self.power);
            let color = power_color(self.power);
            let steps = (length / 0.26).ceil() as usize;
            for i in 1..=steps {
                let d = (i as f32 * 0.26).min(length);
                let t = d / length;
                let p = self.round.ball + self.aim * d;
                let s = if i % 3 == 0 { 0.1 } else { 0.07 };
                draw_cube(
                    vec3(p.x, self.round.course.height(p) + 0.04, p.y),
                    vec3(s, 0.025, s),
                    None,
                    mix(PAPER, color, t),
                );
            }
            let back = 0.45 + self.power * 0.55;
            let p = self.round.ball - self.aim * back;
            let yaw = self.aim.y.atan2(self.aim.x);
            draw_mesh_like_putter(&self.copper_mesh, p, self.round.course.height(p), yaw);
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
                draw_cube(
                    vec3(b.x, y + 0.025, b.y),
                    vec3(0.35, 0.02, 0.35),
                    None,
                    Color::from_rgba(47, 83, 56, 255),
                );
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

/// A circle on the green, projected so it reads as flat in both views.
pub(super) fn ground_ring(
    p: &Projector,
    at: Vec2,
    ground: f32,
    radius: f32,
    width: f32,
    color: Color,
) {
    let mut last: Option<Vec2> = None;
    for k in 0..=28 {
        let a = k as f32 / 28. * std::f32::consts::TAU;
        let q = p.screen(vec3(
            at.x + radius * a.cos(),
            ground,
            at.y + radius * a.sin(),
        ));
        if let Some(l) = last {
            draw_line(l.x, l.y, q.x, q.y, width, color);
        }
        last = Some(q);
    }
}
/// A small copper putter head behind the ball, aligned across the aim line.
pub(super) fn draw_mesh_like_putter(mesh: &Mesh3d, at: Vec2, ground: f32, yaw: f32) {
    let model = Mat4::from_scale_rotation_translation(
        vec3(0.1, 0.12, 0.5),
        Quat::from_rotation_y(-yaw),
        vec3(at.x, ground + 0.08, at.y),
    );
    mesh.draw_transformed(model);
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
            let mut v = Vertex::new2(
                vec3(p.x, course.height(p) + 0.025, p.y),
                Vec2::ZERO,
                Color::from_rgba(122, 169, 105, 255),
            );
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
