//! Interactive original 2D physics sandbox. No imported textures, fonts, or scene assets.
use ferravex_core::{FixedClock, FrameStats};
use ferravex_physics::JointHandle;
use ferravex_physics::d2::{
    self, ColliderBuilder, JointBuilder, PhysicsWorld, RigidBodyBuilder, RigidBodyHandle,
};
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{PlayerInput, Preset};
use ferravex_render::{prelude::*, ui};

const MAX_BODIES: usize = 240;
const TICK_HZ: f64 = 120.0;

#[derive(Clone, Copy)]
enum Shape {
    Ball(f32),
    Box(Vec2),
    Dumbbell,
}

struct Body {
    handle: RigidBodyHandle,
    shape: Shape,
    color: Color,
    previous_position: Vec2,
    previous_angle: f32,
}

struct Sandbox {
    physics: PhysicsWorld,
    bodies: Vec<Body>,
    platforms: Vec<(Vec2, Vec2)>,
    links: Vec<JointHandle>,
    impact: Option<(f32, Surface)>,
}

impl Sandbox {
    fn new() -> Self {
        let mut world = Self {
            physics: PhysicsWorld::new([0.0, -9.81]),
            bodies: Vec::new(),
            platforms: vec![],
            links: Vec::new(),
            impact: None,
        };
        for (position, half) in [
            (vec2(0.0, -0.5), vec2(12.0, 0.5)),
            (vec2(-12.25, 6.0), vec2(0.25, 6.5)),
            (vec2(12.25, 6.0), vec2(0.25, 6.5)),
            (vec2(-8.0, 0.8), vec2(2.0, 0.8)),
            (vec2(8.0, 0.8), vec2(2.0, 0.8)),
        ] {
            world
                .physics
                .add_box(position.to_array(), half.to_array(), false);
            world.platforms.push((position, half));
        }
        for row in 0..6 {
            for column in 0..(6 - row) {
                world.spawn(
                    vec2(
                        -4.0 + column as f32 * 1.06 + row as f32 * 0.53,
                        0.51 + row as f32 * 1.03,
                    ),
                    false,
                );
            }
        }
        for index in 0..9 {
            world.spawn(
                vec2(
                    5.8 + (index % 3) as f32 * 1.05,
                    5.0 + (index / 3) as f32 * 1.1,
                ),
                true,
            );
        }
        world.mechanisms();
        world
    }

    fn mechanisms(&mut self) {
        let anchor = self
            .physics
            .insert_body(RigidBodyBuilder::fixed().translation(vec2(-9., 11.)))
            .unwrap();
        let position = vec2(-8.5, 10.);
        let (rod, _) = self.physics.insert(
            RigidBodyBuilder::dynamic().translation(position),
            ColliderBuilder::cuboid(0.18, 1.),
        );
        self.bodies.push(Body {
            handle: rod,
            shape: Shape::Box(vec2(0.18, 1.)),
            color: ui::CYAN,
            previous_position: position,
            previous_angle: 0.,
        });
        self.links.push(
            self.physics
                .insert_joint(
                    anchor,
                    rod,
                    JointBuilder::hinge()
                        .local_anchor2(vec2(-0.5, 1.))
                        .limits(-1.2, 1.2),
                )
                .unwrap(),
        );
        let position = vec2(-8.5, 8.4);
        let (tip, _) = self.physics.insert(
            RigidBodyBuilder::dynamic().translation(position),
            ColliderBuilder::cuboid(0.4, 0.6),
        );
        self.bodies.push(Body {
            handle: tip,
            shape: Shape::Box(vec2(0.4, 0.6)),
            color: ui::AMBER,
            previous_position: position,
            previous_angle: 0.,
        });
        self.links.push(
            self.physics
                .insert_joint(
                    rod,
                    tip,
                    JointBuilder::fixed()
                        .local_anchor1(vec2(0., -1.))
                        .local_anchor2(vec2(0., 0.6)),
                )
                .unwrap(),
        );
        let anchor = self
            .physics
            .insert_body(RigidBodyBuilder::fixed().translation(vec2(-5.8, 11.)))
            .unwrap();
        self.spawn(vec2(-5., 9.), true);
        let bob = self.bodies.last().unwrap().handle;
        self.links.push(
            self.physics
                .insert_joint(
                    anchor,
                    bob,
                    JointBuilder::distance((0.8_f32 * 0.8 + 4.).sqrt()),
                )
                .unwrap(),
        );
        let anchor = self
            .physics
            .insert_body(RigidBodyBuilder::fixed().translation(vec2(8.2, 12.)))
            .unwrap();
        let position = vec2(8.2, 10.);
        let bob = self
            .physics
            .insert_compound(
                RigidBodyBuilder::dynamic().translation(position),
                [
                    ColliderBuilder::cuboid(0.75, 0.1),
                    ColliderBuilder::ball(0.28).translation(vec2(-0.75, 0.)),
                    ColliderBuilder::ball(0.28).translation(vec2(0.75, 0.)),
                ],
            )
            .unwrap();
        self.bodies.push(Body {
            handle: bob,
            shape: Shape::Dumbbell,
            color: ui::CYAN,
            previous_position: position,
            previous_angle: 0.,
        });
        self.links.push(
            self.physics
                .insert_joint(anchor, bob, JointBuilder::spring(2., 20., 3.))
                .unwrap(),
        );
    }

    fn spawn(&mut self, position: Vec2, ball: bool) {
        if self.bodies.len() >= MAX_BODIES {
            return;
        }
        let (handle, shape, color) = if ball {
            (
                self.physics.add_ball(position.to_array(), 0.44, true),
                Shape::Ball(0.44),
                ui::CYAN,
            )
        } else {
            (
                self.physics
                    .add_box(position.to_array(), [0.48, 0.48], true),
                Shape::Box(vec2(0.48, 0.48)),
                ui::AMBER,
            )
        };
        let collider = self.physics.bodies[handle].colliders()[0];
        self.physics.colliders[collider].set_restitution(if ball { 0.42 } else { 0.08 });
        self.physics.bodies[handle].enable_ccd(true);
        self.bodies.push(Body {
            handle,
            shape,
            color,
            previous_position: position,
            previous_angle: 0.0,
        });
    }

    fn step(&mut self, dt: f32) {
        let mut velocities = [Vec2::ZERO; MAX_BODIES];
        for (i, body) in self.bodies.iter_mut().enumerate() {
            velocities[i] = Vec2::from_array(self.physics.bodies[body.handle].linvel().to_array());
            let rigid_body = &self.physics.bodies[body.handle];
            body.previous_position = Vec2::from_array(rigid_body.translation().to_array());
            let axis = *rigid_body.rotation() * d2::Vector::X;
            body.previous_angle = axis.y.atan2(axis.x);
        }
        self.physics.step(dt);
        self.impact = None;
        for (body, before) in self.bodies.iter().zip(velocities) {
            let after = Vec2::from_array(self.physics.bodies[body.handle].linvel().to_array());
            let change = (after - before).length();
            if before.length() > 1.
                && change > 1.2
                && self.impact.is_none_or(|(strength, _)| change > strength)
            {
                self.impact = Some((
                    change,
                    if matches!(body.shape, Shape::Dumbbell) {
                        Surface::Metal
                    } else if matches!(body.shape, Shape::Ball(_)) {
                        Surface::Stone
                    } else {
                        Surface::Wood
                    },
                ));
            }
        }
    }
}

fn window_conf() -> Conf {
    ferravex_render::window_conf("Ferravex / 2D physics laboratory")
}

ferravex_render::game!(window_conf, game);
async fn game() {
    let mut demo = ferravex_render::demo::Demo::new();
    let mut sandbox = Sandbox::new();
    let mut sound = GameAudio::new();
    let mut impact_cooldown = 0_f32;
    let mut clock = FixedClock::new(TICK_HZ, 8);
    let mut stats = FrameStats::<90>::default();
    let mut paused = false;
    let mut ball = true;
    let mut player_input = PlayerInput::preset("physics-2d", Preset::Lab2);
    loop {
        ui::warm_up_font();
        stats.record(get_frame_time());
        player_input.sample(true);
        if player_input.pressed("pause") {
            paused = !paused;
        }
        if player_input.pressed("select_one") {
            ball = true;
        }
        if player_input.pressed("select_two") {
            ball = false;
        }
        if player_input.pressed("restart") {
            sandbox = Sandbox::new();
            clock.reset();
        }

        let footer_y = screen_height() - 98.0;
        let area_height = (footer_y - 143.0).max(100.0);
        let scale = ((screen_width() - 58.0) / 26.0)
            .min(area_height / 14.0)
            .max(1.0);
        let origin = vec2(screen_width() * 0.5, footer_y - 15.0 - scale);
        let to_screen = |point: Vec2| origin + vec2(point.x, -point.y) * scale;
        let pointer = player_input.cursor(demo.dt(), 600.);
        let pointer_in_scene = pointer.y > 136.0 && pointer.y < footer_y - 8.0;
        let spawn_position = vec2(
            (pointer.x - origin.x) / scale,
            (origin.y - pointer.y) / scale,
        );
        let secondary = player_input.pressed("secondary");
        let primary = player_input.pressed("primary");
        if pointer_in_scene && (primary || secondary) {
            sandbox.spawn(
                vec2(
                    spawn_position.x.clamp(-11.2, 11.2),
                    spawn_position.y.max(0.6),
                ),
                ball && !secondary,
            );
        }
        impact_cooldown = (impact_cooldown - get_frame_time()).max(0.);
        let frame = clock.advance(if paused {
            0.0
        } else {
            player_input.simulation_dt(get_frame_time()) as f64
        });
        for _ in 0..frame.steps {
            sandbox.step(frame.dt);
            if impact_cooldown == 0.
                && let Some((strength, surface)) = sandbox.impact
            {
                sound.cue(Cue::Impact(surface), (strength * 0.09).clamp(0.15, 0.7));
                impact_cooldown = 0.055;
            }
        }
        let alpha = if paused { 1.0 } else { frame.alpha };

        clear_background(ui::BACKGROUND);
        ferravex_render::set_world_colors(true);
        for x in -12..=12 {
            let start = to_screen(vec2(x as f32, 0.0));
            let end = to_screen(vec2(x as f32, 13.0));
            draw_line(
                start.x,
                start.y,
                end.x,
                end.y,
                1.0,
                Color::from_rgba(25, 38, 53, 255),
            );
        }
        for y in 0..=13 {
            let start = to_screen(vec2(-12.0, y as f32));
            let end = to_screen(vec2(12.0, y as f32));
            draw_line(
                start.x,
                start.y,
                end.x,
                end.y,
                1.0,
                Color::from_rgba(25, 38, 53, 255),
            );
        }
        for (position, half) in &sandbox.platforms {
            let top_left = to_screen(*position + vec2(-half.x, half.y));
            draw_rectangle(
                top_left.x,
                top_left.y,
                half.x * scale * 2.0,
                half.y * scale * 2.0,
                Color::from_rgba(43, 59, 73, 255),
            );
            draw_line(
                top_left.x,
                top_left.y,
                top_left.x + half.x * scale * 2.0,
                top_left.y,
                2.0,
                ui::MUTED,
            );
        }
        for id in &sandbox.links {
            if let Some([a, b]) = sandbox.physics.joint_anchors(*id) {
                let a = to_screen(vec2(a.x, a.y));
                let b = to_screen(vec2(b.x, b.y));
                draw_line(a.x, a.y, b.x, b.y, 2., ui::MUTED);
                draw_circle(a.x, a.y, 3.5, ui::AMBER);
            }
        }
        for body in &sandbox.bodies {
            let rigid_body = &sandbox.physics.bodies[body.handle];
            let position = body
                .previous_position
                .lerp(Vec2::from_array(rigid_body.translation().to_array()), alpha);
            let axis = *rigid_body.rotation() * d2::Vector::X;
            let angle_now = axis.y.atan2(axis.x);
            let difference = (angle_now - body.previous_angle + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            let angle = body.previous_angle + difference * alpha;
            let center = to_screen(position);
            let rotation = Vec2::from_angle(angle);
            match body.shape {
                Shape::Dumbbell => {
                    let a = to_screen(position + rotation.rotate(vec2(-0.75, 0.)));
                    let b = to_screen(position + rotation.rotate(vec2(0.75, 0.)));
                    draw_line(a.x, a.y, b.x, b.y, 0.2 * scale, ui::MUTED);
                    for p in [a, b] {
                        draw_circle(p.x, p.y, 0.28 * scale, body.color);
                        draw_circle_lines(p.x, p.y, 0.28 * scale, 1.5, ui::AMBER);
                    }
                }

                Shape::Ball(radius) => {
                    draw_circle(center.x, center.y, radius * scale, body.color);
                    draw_circle_lines(
                        center.x,
                        center.y,
                        radius * scale,
                        1.5,
                        Color::from_rgba(191, 239, 230, 255),
                    );
                    let end = to_screen(position + rotation * radius * 0.72);
                    draw_line(center.x, center.y, end.x, end.y, 2.0, ui::BACKGROUND);
                }
                Shape::Box(half) => {
                    let corners = [
                        vec2(-half.x, -half.y),
                        vec2(half.x, -half.y),
                        vec2(half.x, half.y),
                        vec2(-half.x, half.y),
                    ]
                    .map(|p| to_screen(position + rotation.rotate(p)));
                    draw_triangle(corners[0], corners[1], corners[2], body.color);
                    draw_triangle(corners[0], corners[2], corners[3], body.color);
                    for index in 0..4 {
                        let a = corners[index];
                        let b = corners[(index + 1) % 4];
                        draw_line(
                            a.x,
                            a.y,
                            b.x,
                            b.y,
                            1.5,
                            Color::from_rgba(250, 212, 147, 255),
                        );
                    }
                    draw_line(
                        corners[0].x,
                        corners[0].y,
                        corners[2].x,
                        corners[2].y,
                        1.0,
                        Color::from_rgba(189, 126, 57, 255),
                    );
                }
            }
        }
        if pointer_in_scene && sandbox.bodies.len() < MAX_BODIES {
            if ball {
                draw_circle_lines(pointer.x, pointer.y, 0.44 * scale, 1.0, ui::CYAN);
            } else {
                draw_rectangle_lines(
                    pointer.x - 0.48 * scale,
                    pointer.y - 0.48 * scale,
                    0.96 * scale,
                    0.96 * scale,
                    1.0,
                    ui::AMBER,
                );
            }
        }
        ferravex_render::set_world_colors(false);
        ui::header(
            "2D RIGID BODIES",
            "Gravity, in two dimensions.",
            if screen_width() > 660.0 {
                "Click or tap to drop a body  /  Right click: crate  /  Space: pause  /  R: reset"
            } else {
                "Tap to drop. Choose a shape below."
            },
        );
        ui::panel(Rect::new(20.0, footer_y, screen_width() - 40.0, 78.0));
        let button_width = ((screen_width() - 92.0) / 4.0).clamp(38.0, 84.0);
        let button_rect = |index: f32| {
            Rect::new(
                34.0 + index * (button_width + 8.0),
                footer_y + 20.0,
                button_width,
                36.0,
            )
        };
        if ui::button(button_rect(0.0), "1  BALL", ball) {
            ball = true;
        }
        if ui::button(button_rect(1.0), "2  CRATE", !ball) {
            ball = false;
        }
        if ui::button(
            button_rect(2.0),
            if paused { "RESUME" } else { "PAUSE" },
            paused,
        ) {
            paused = !paused;
        }
        if ui::button(button_rect(3.0), "RESET", false) {
            sandbox = Sandbox::new();
            clock.reset();
        }
        if screen_width() > 730.0 {
            let awake = sandbox
                .bodies
                .iter()
                .filter(|b| !sandbox.physics.bodies[b.handle].is_sleeping())
                .count();
            ui::stat(
                430.0,
                footer_y + 25.0,
                "DYNAMIC BODIES",
                &format!("{} / {MAX_BODIES}", sandbox.bodies.len()),
            );
            ui::stat(602.0, footer_y + 25.0, "AWAKE", &awake.to_string());
        }
        if screen_width() > 920.0 {
            ui::stat(
                744.0,
                footer_y + 25.0,
                "FRAME AVERAGE",
                &format!("{:.1} ms", stats.mean_ms()),
            );
        }
        if screen_width() > 1090.0 {
            ui::stat(934.0, footer_y + 25.0, "FIXED SIMULATION", "120 Hz");
        }
        if player_input.gameplay_active() && player_input.cursor_is_virtual() {
            draw_circle_lines(pointer.x, pointer.y, 5., 1.3, WHITE);
        }
        player_input.draw_settings();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falling_bodies_produce_bounded_impact_hints() {
        let mut sandbox = Sandbox::new();
        let mut found = false;
        for _ in 0..360 {
            sandbox.step(1. / 120.);
            if let Some((strength, _)) = sandbox.impact {
                assert!(strength.is_finite() && strength > 1.2);
                found = true;
            }
        }
        assert!(found);
    }
    #[test]
    fn jointed_mechanisms_keep_their_constraints_under_load() {
        let mut sandbox = Sandbox::new();
        for _ in 0..1200 {
            sandbox.step(1. / 120.);
        }
        assert_eq!(sandbox.links.len(), 4);
        for &id in &sandbox.links {
            let joint = &sandbox.physics.joints[id];
            let [a, b] = sandbox.physics.joint_anchors(id).unwrap();
            match joint.description().kind {
                ferravex_physics::JointKind::Distance { length } => {
                    assert!((a.distance(b) - length).abs() < 0.005)
                }
                ferravex_physics::JointKind::Spring {
                    length, stiffness, ..
                } => {
                    let body = joint.bodies()[1];
                    let extension = sandbox.physics.bodies[body].mass() * 9.81 / stiffness;
                    assert!((a.distance(b) - length - extension).abs() < 0.025);
                }
                _ => assert!(a.distance(b) < 0.005),
            }
        }
    }

    #[test]
    fn recovered_sandbox_settles_with_finite_rotating_bodies() {
        let mut sandbox = Sandbox::new();
        let mut rotation_seen = false;
        for _ in 0..720 {
            sandbox.step(1.0 / 120.0);
            for body in &sandbox.bodies {
                let rigid = &sandbox.physics.bodies[body.handle];
                assert!(rigid.translation().is_finite());
                assert!(rigid.linvel().is_finite());
                rotation_seen |= rigid.rotation().angle().abs() > 0.05;
            }
        }
        assert!(rotation_seen, "contacts should rotate the falling bodies");
        assert!(sandbox.bodies.iter().all(|body| {
            let p = sandbox.physics.bodies[body.handle].translation();
            p.y > 0.3 && p.x.abs() < 12.0
        }));
    }

    #[test]
    fn spawning_is_bounded_and_rebuild_replaces_the_world() {
        let mut sandbox = Sandbox::new();
        for i in 0..MAX_BODIES + 10 {
            sandbox.spawn(vec2((i % 10) as f32 - 5.0, 12.0 + (i / 10) as f32), true);
        }
        assert_eq!(sandbox.bodies.len(), MAX_BODIES);
        assert_eq!(
            sandbox.physics.bodies.len(),
            MAX_BODIES + sandbox.platforms.len() + 3
        );
        sandbox = Sandbox::new();
        assert_eq!(sandbox.bodies.len(), 34);
        assert_eq!(sandbox.physics.bodies.len(), 42);
    }
}
