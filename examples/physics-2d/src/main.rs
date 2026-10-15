//! Interactive Rapier 2D sandbox. No imported textures, fonts, or scene assets.
use ferravex_core::{FixedClock, FrameStats};
use ferravex_physics::d2::{self, PhysicsWorld, RigidBodyHandle};
use ferravex_render::{prelude::*, ui};

const MAX_BODIES: usize = 240;
const TICK_HZ: f64 = 120.0;

#[derive(Clone, Copy)]
enum Shape {
    Ball(f32),
    Box(Vec2),
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
}

impl Sandbox {
    fn new() -> Self {
        let mut world = Self {
            physics: PhysicsWorld::new([0.0, -9.81]),
            bodies: Vec::new(),
            platforms: vec![],
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
        world
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
        for body in &mut self.bodies {
            let rigid_body = &self.physics.bodies[body.handle];
            body.previous_position = Vec2::from_array(rigid_body.translation().to_array());
            let axis = *rigid_body.rotation() * d2::Vector::X;
            body.previous_angle = axis.y.atan2(axis.x);
        }
        self.physics.step(dt);
    }
}

fn window_conf() -> Conf {
    ferravex_render::window_conf("Ferravex / 2D physics laboratory")
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut demo = ferravex_render::demo::Demo::new();
    let mut sandbox = Sandbox::new();
    let mut clock = FixedClock::new(TICK_HZ, 8);
    let mut stats = FrameStats::<90>::default();
    let mut paused = false;
    let mut ball = true;
    loop {
        ui::warm_up_font();
        stats.record(get_frame_time());
        if is_key_pressed(KeyCode::Space) {
            paused = !paused;
        }
        if is_key_pressed(KeyCode::Key1) {
            ball = true;
        }
        if is_key_pressed(KeyCode::Key2) {
            ball = false;
        }
        if is_key_pressed(KeyCode::R) {
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
        let pointer = Vec2::from(mouse_position());
        let pointer_in_scene = pointer.y > 136.0 && pointer.y < footer_y - 8.0;
        let spawn_position = vec2(
            (pointer.x - origin.x) / scale,
            (origin.y - pointer.y) / scale,
        );
        if pointer_in_scene
            && (is_mouse_button_pressed(MouseButton::Left)
                || is_mouse_button_pressed(MouseButton::Right))
        {
            sandbox.spawn(
                vec2(
                    spawn_position.x.clamp(-11.2, 11.2),
                    spawn_position.y.max(0.6),
                ),
                ball && !is_mouse_button_pressed(MouseButton::Right),
            );
        }
        let frame = clock.advance(if paused { 0.0 } else { get_frame_time() as f64 });
        for _ in 0..frame.steps {
            sandbox.step(frame.dt);
        }
        let alpha = if paused { 1.0 } else { frame.alpha };

        clear_background(ui::BACKGROUND);
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
        ui::header(
            "EXAMPLE 01  /  RAPIER 2D",
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
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}
