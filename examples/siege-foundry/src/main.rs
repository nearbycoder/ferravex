//! A compact destruction range: all knockdowns come from Rapier contacts.
use ferravex_core::FixedClock;
use ferravex_physics::d3::{
    ColliderBuilder, PhysicsWorld, RigidBodyBuilder, RigidBodyHandle, Vector,
};
use ferravex_render::{
    demo::Demo,
    mesh::Mesh3d,
    prelude::*,
    shaders::{LitMaterial, LitSettings},
};

#[cfg(test)]
const DT: f32 = 1.0 / 120.0;
const AMMO: u8 = 8;
const GOAL: usize = 6;
const SPEED: f32 = 28.0;
const PIVOT: Vec3 = Vec3::new(0.0, 1.3, 15.0);
const SKY: Color = Color::from_rgba(173, 177, 161, 255);
const INK: Color = Color::from_rgba(34, 40, 39, 255);
const PAPER: Color = Color::from_rgba(246, 235, 211, 255);
const RUST: Color = Color::from_rgba(176, 86, 48, 255);
const SAND: Color = Color::from_rgba(194, 169, 124, 255);
const STEEL: Color = Color::from_rgba(65, 81, 78, 255);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Aiming,
    Won,
    Lost,
}

struct Block {
    handle: RigidBodyHandle,
    home: Vec3,
    target: bool,
    down: bool,
    previous_position: Vec3,
    previous_rotation: Quat,
}

struct Shot {
    handle: RigidBodyHandle,
    previous_position: Vec3,
}

struct Range {
    physics: PhysicsWorld,
    blocks: Vec<Block>,
    shots: Vec<Shot>,
    yaw: f32,
    elevation: f32,
    ammo: u8,
    cooldown: f32,
    since_shot: f32,
    elapsed: f32,
    phase: Phase,
}

impl Range {
    fn new() -> Self {
        let mut physics = PhysicsWorld::new([0.0, -9.81, 0.0]);
        physics.add_box([0.0, -0.4, 4.0], [14.0, 0.4, 20.0], false);
        let mut blocks = Vec::new();
        for x in [-4.0, 0.0, 4.0] {
            for layer in 0..4 {
                let home = vec3(x, 0.6 + layer as f32 * 1.202, -4.0);
                let (handle, _) = physics.insert(
                    RigidBodyBuilder::dynamic()
                        .translation(Vector::from_array(home.to_array()))
                        .linear_damping(0.12)
                        .angular_damping(0.18)
                        .ccd_enabled(true),
                    ColliderBuilder::cuboid(0.6, 0.6, 0.6)
                        .density(0.85)
                        .friction(0.55)
                        .restitution(0.025),
                );
                blocks.push(Block {
                    handle,
                    home,
                    target: layer >= 2,
                    down: false,
                    previous_position: home,
                    previous_rotation: Quat::IDENTITY,
                });
            }
        }
        Self {
            physics,
            blocks,
            shots: Vec::with_capacity(AMMO as usize),
            yaw: 0.0,
            elevation: 0.13,
            ammo: AMMO,
            cooldown: 0.0,
            since_shot: 0.0,
            elapsed: 0.0,
            phase: Phase::Aiming,
        }
    }

    fn direction(&self) -> Vec3 {
        vec3(
            self.yaw.sin() * self.elevation.cos(),
            self.elevation.sin(),
            -self.yaw.cos() * self.elevation.cos(),
        )
    }

    fn score(&self) -> usize {
        self.blocks.iter().filter(|b| b.target && b.down).count()
    }

    fn fire(&mut self) -> bool {
        if self.phase != Phase::Aiming || self.cooldown > 0.0 || self.ammo == 0 {
            return false;
        }
        let direction = self.direction();
        let muzzle = PIVOT + direction * 1.6;
        let (handle, _) = self.physics.insert(
            RigidBodyBuilder::dynamic()
                .translation(Vector::from_array(muzzle.to_array()))
                .linvel(Vector::from_array((direction * SPEED).to_array()))
                .ccd_enabled(true),
            ColliderBuilder::ball(0.36)
                .density(18.0)
                .friction(0.6)
                .restitution(0.08),
        );
        self.shots.push(Shot {
            handle,
            previous_position: muzzle,
        });
        self.ammo -= 1;
        self.cooldown = 0.8;
        self.since_shot = 0.0;
        true
    }

    fn step(&mut self, dt: f32) {
        self.elapsed += dt;
        self.since_shot += dt;
        self.cooldown = (self.cooldown - dt).max(0.0);
        for block in &mut self.blocks {
            let body = &self.physics.bodies[block.handle];
            block.previous_position = Vec3::from_array(body.translation().to_array());
            block.previous_rotation = Quat::from_array(body.rotation().to_array());
        }
        for shot in &mut self.shots {
            shot.previous_position =
                Vec3::from_array(self.physics.bodies[shot.handle].translation().to_array());
        }
        self.physics.step(dt);
        for block in &mut self.blocks {
            if !block.target || block.down {
                continue;
            }
            let body = &self.physics.bodies[block.handle];
            let p = Vec3::from_array(body.translation().to_array());
            let rotation = Quat::from_array(body.rotation().to_array());
            block.down = p.distance(block.home) > 1.3
                || p.y < block.home.y - 0.75
                || (rotation * Vec3::Y).y < 0.45;
        }
        if self.score() == GOAL {
            self.phase = Phase::Won;
        } else if self.ammo == 0 && self.since_shot > 6.0 {
            self.phase = Phase::Lost;
        }
    }

    /// A physical low arc from the barrel muzzle to a selected point.
    fn aim_at(&mut self, point: Vec3) {
        let delta = point - PIVOT;
        self.yaw = delta.x.atan2(-delta.z);
        let distance = vec2(delta.x, delta.z).length();
        let mut low: f32 = 0.025;
        let mut high: f32 = 0.7;
        for _ in 0..24 {
            let angle = (low + high) * 0.5;
            let time = (distance - 1.6 * angle.cos()) / (SPEED * angle.cos());
            let height = PIVOT.y + distance * angle.tan() - 4.905 * time * time;
            if height < point.y {
                low = angle;
            } else {
                high = angle;
            }
        }
        self.elevation = (low + high) * 0.5;
    }

    /// Autoplay chooses the next standing tower and uses the same fire operation.
    fn autoplay(&mut self) -> bool {
        if self.phase != Phase::Aiming || self.elapsed < 0.7 || self.since_shot < 2.0 {
            return false;
        }
        if let Some(target) = self.blocks.iter().find(|b| b.target && !b.down) {
            self.aim_at(vec3(target.home.x, 1.8, target.home.z));
            true
        } else {
            false
        }
    }
}

struct Art {
    floor: Mesh3d,
    stone: Mesh3d,
    target: Mesh3d,
    fallen: Mesh3d,
    ball: Mesh3d,
    barrel: Mesh3d,
    muzzle: Mesh3d,
    wheel: Mesh3d,
    chassis: Mesh3d,
    line: Mesh3d,
    column: Mesh3d,
    girder: Mesh3d,
    chimney: Mesh3d,
    roof: Mesh3d,
    building: Mesh3d,
    rivet: Mesh3d,
}

impl Art {
    fn new() -> Self {
        Self {
            floor: Mesh3d::cuboid(vec3(28.0, 0.8, 40.0), Color::from_rgba(132, 130, 111, 255)),
            stone: Mesh3d::cuboid(Vec3::splat(1.17), SAND),
            target: Mesh3d::cuboid(Vec3::splat(1.17), RUST),
            fallen: Mesh3d::cuboid(Vec3::splat(1.2), Color::from_rgba(103, 111, 91, 255)),
            ball: Mesh3d::sphere(0.36, INK, 10, 6),
            barrel: Mesh3d::frustum(0.34, 0.27, 2.4, STEEL, 12),
            muzzle: Mesh3d::cylinder(0.2, 0.035, INK, 12),
            wheel: Mesh3d::cylinder(0.6, 0.3, INK, 10),
            chassis: Mesh3d::cuboid(vec3(1.5, 0.45, 1.6), RUST),
            line: Mesh3d::cuboid(
                vec3(0.075, 0.008, 26.0),
                Color::from_rgba(190, 181, 139, 255),
            ),
            column: Mesh3d::cuboid(vec3(0.32, 7.0, 0.32), STEEL),
            girder: Mesh3d::cuboid(vec3(18.0, 0.3, 0.34), RUST),
            chimney: Mesh3d::frustum(1.0, 0.7, 12.0, Color::from_rgba(139, 118, 91, 255), 8),
            roof: Mesh3d::cuboid(vec3(8.5, 0.3, 6.5), STEEL),
            building: Mesh3d::cuboid(vec3(8.0, 4.0, 6.0), Color::from_rgba(158, 145, 114, 255)),
            rivet: Mesh3d::cylinder(0.16, 0.015, PAPER, 8),
        }
    }

    fn draw(&self, game: &Range, alpha: f32, material: &LitMaterial) {
        let eye = vec3(23.0, 18.0, 28.0);
        set_camera(&Camera3D {
            position: eye,
            target: vec3(0.0, 1.5, 3.5),
            up: Vec3::Y,
            fovy: 43.0_f32.to_radians(),
            z_near: 0.1,
            z_far: 130.0,
            ..Default::default()
        });
        material.begin(LitSettings {
            camera_position: eye,
            fog_color: SKY,
            fog_start: 40.0,
            fog_end: 100.0,
            ambient: 0.55,
            ..Default::default()
        });
        self.floor.draw(vec3(0.0, -0.4, 4.0), Quat::IDENTITY);
        for x in [-8.0, -6.0, 6.0, 8.0] {
            self.line.draw(vec3(x, 0.012, 4.0), Quat::IDENTITY);
        }
        for x in [-9.0, 9.0] {
            self.column.draw(vec3(x, 3.5, -9.0), Quat::IDENTITY);
        }
        self.girder.draw(vec3(0.0, 6.9, -9.0), Quat::IDENTITY);
        self.building.draw(vec3(-15.0, 2.0, -15.0), Quat::IDENTITY);
        self.roof
            .draw(vec3(-15.0, 4.15, -15.0), Quat::from_rotation_z(-0.1));
        self.chimney.draw(vec3(-12.0, 6.0, -17.0), Quat::IDENTITY);
        self.chimney.draw(vec3(-16.0, 5.0, -18.0), Quat::IDENTITY);
        for block in &game.blocks {
            let body = &game.physics.bodies[block.handle];
            let p = block
                .previous_position
                .lerp(Vec3::from_array(body.translation().to_array()), alpha);
            let q = block
                .previous_rotation
                .slerp(Quat::from_array(body.rotation().to_array()), alpha);
            let mesh = if block.down {
                &self.fallen
            } else if block.target {
                &self.target
            } else {
                &self.stone
            };
            mesh.draw(p, q);
            if block.target && !block.down {
                self.rivet.draw(
                    p + q * vec3(0.0, 0.0, 0.594),
                    q * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
                );
            }
        }
        for shot in &game.shots {
            let p = shot.previous_position.lerp(
                Vec3::from_array(game.physics.bodies[shot.handle].translation().to_array()),
                alpha,
            );
            self.ball.draw(p, Quat::IDENTITY);
        }
        let direction = game.direction();
        let q = Quat::from_rotation_arc(Vec3::Y, direction);
        self.chassis
            .draw(vec3(0.0, 0.7, 15.0), Quat::from_rotation_y(-game.yaw));
        for x in [-0.95, 0.95] {
            self.wheel.draw(
                vec3(x, 0.6, 15.0),
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            );
        }
        let recoil = (game.cooldown - 0.55).max(0.0) * 1.2;
        self.barrel.draw(PIVOT + direction * (0.3 - recoil), q);
        self.muzzle.draw(PIVOT + direction * (1.515 - recoil), q);
        material.end();
        if game.phase == Phase::Aiming && game.ammo > 0 {
            let start = PIVOT + direction * 1.6;
            let mut previous = start;
            for i in 1..=44 {
                let t = i as f32 * 0.025;
                let p = start + direction * SPEED * t - Vec3::Y * (4.905 * t * t);
                if p.y <= 0.05 {
                    break;
                }
                if i % 2 == 0 {
                    draw_line_3d(previous, p, Color::from_rgba(249, 225, 157, 180));
                }
                previous = p;
            }
        }
        set_default_camera();
    }
}

fn label(text: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(
        text,
        x + 1.0,
        y + 1.0,
        size,
        Color::new(0.0, 0.0, 0.0, 0.22),
    );
    draw_text(text, x, y, size, color);
}

fn button(rect: Rect, title: &str, enabled: bool) -> bool {
    let pointer = Vec2::from(mouse_position());
    let hover = rect.contains(pointer);
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        if hover && enabled { RUST } else { STEEL },
    );
    let size = 19;
    let width = measure_text(title, None, size, 1.0).width;
    label(
        title,
        rect.x + (rect.w - width) * 0.5,
        rect.y + rect.h * 0.5 + 7.0,
        size as f32,
        if enabled { PAPER } else { GRAY },
    );
    enabled && hover && is_mouse_button_pressed(MouseButton::Left)
}

fn window_conf() -> Conf {
    ferravex_render::window_conf("Siege Foundry / Ferravex")
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut game = Range::new();
    let art = Art::new();
    let material = LitMaterial::new().expect("Ferravex lit material");
    let mut demo = Demo::new();
    let mut clock = FixedClock::new(120.0, 12);
    let mut pending_fire = false;
    let mut previous_mouse = Vec2::from(mouse_position());
    loop {
        ferravex_render::ui::warm_up_font_sizes(&[16, 17, 18, 19, 23, 26, 28, 36]);
        let dt = demo.dt();
        let pointer = Vec2::from(mouse_position());
        let w = screen_width();
        let h = screen_height();
        let compact = w < 640.0;
        if is_key_pressed(KeyCode::R) {
            game = Range::new();
            clock.reset();
            pending_fire = false;
        }
        if game.phase == Phase::Aiming {
            game.yaw += (is_key_down(KeyCode::Right) as i32 - is_key_down(KeyCode::Left) as i32)
                as f32
                * dt
                * 0.33;
            game.elevation += (is_key_down(KeyCode::Up) as i32 - is_key_down(KeyCode::Down) as i32)
                as f32
                * dt
                * 0.2;
            if is_mouse_button_down(MouseButton::Left)
                && !is_mouse_button_pressed(MouseButton::Left)
                && pointer.y > 90.0
                && pointer.y < h - 100.0
            {
                let delta = pointer - previous_mouse;
                game.yaw += delta.x * 0.002;
                game.elevation -= delta.y * 0.0012;
            }
            game.yaw = game.yaw.clamp(-0.55, 0.55);
            game.elevation = game.elevation.clamp(0.025, 0.65);
            pending_fire |= is_key_pressed(KeyCode::Space);
            if demo.autoplay {
                pending_fire |= game.autoplay();
            }
        }
        previous_mouse = pointer;
        let frame = clock.advance(dt as f64);
        for _ in 0..frame.steps {
            if pending_fire {
                game.fire();
                pending_fire = false;
            }
            game.step(frame.dt);
        }
        clear_background(SKY);
        art.draw(&game, frame.alpha, &material);
        label("SIEGE FOUNDRY", 28.0, 37.0, 26.0, INK);
        label("Topple the six rust-red blocks.", 28.0, 61.0, 18.0, INK);
        label(
            &format!("{} / {} DOWN", game.score(), GOAL),
            28.0,
            if compact { 97.0 } else { h - 57.0 },
            23.0,
            PAPER,
        );
        label(&format!("{} SHOTS", game.ammo), 28.0, h - 30.0, 19.0, PAPER);
        let fire_rect = Rect::new(w - 146.0, h - 74.0, 118.0, 44.0);
        let reset_rect = Rect::new(w - 274.0, h - 74.0, 118.0, 44.0);
        pending_fire |= button(
            fire_rect,
            if game.cooldown > 0.0 {
                "LOADING"
            } else {
                "FIRE"
            },
            game.ammo > 0 && game.cooldown <= 0.0 && game.phase == Phase::Aiming,
        );
        if button(reset_rect, "REBUILD", true) {
            game = Range::new();
            clock.reset();
            pending_fire = false;
        }
        if compact && game.phase == Phase::Aiming {
            label("Drag to aim; tap Fire.", 28.0, 149.0, 16.0, INK);
        }
        if w > 720.0 {
            label(
                "Arrows / drag: aim    Space: fire    R: rebuild",
                28.0,
                h - 102.0,
                17.0,
                PAPER,
            );
            label(
                &format!("ELEV {:.1}°", game.elevation.to_degrees()),
                w - 146.0,
                h - 90.0,
                17.0,
                INK,
            );
        }
        if game.phase != Phase::Aiming {
            let text = if game.phase == Phase::Won {
                "FOUNDRY CLEARED"
            } else {
                "OUT OF SHOTS"
            };
            let result_size = if compact { 28 } else { 36 };
            let bounds = measure_text(text, None, result_size, 1.0);
            let panel_width = (w - 32.0).min(380.0);
            draw_rectangle(
                (w - panel_width) * 0.5,
                96.0,
                panel_width,
                86.0,
                Color::new(0.12, 0.16, 0.15, 0.92),
            );
            label(
                text,
                (w - bounds.width) * 0.5,
                133.0,
                result_size as f32,
                PAPER,
            );
            let detail = if compact {
                "Rebuild to play again."
            } else if game.phase == Phase::Won {
                "Every target brought down. Rebuild to play again."
            } else {
                "Adjust the low arc, then rebuild to try again."
            };
            let width = measure_text(detail, None, 16, 1.0).width;
            label(detail, (w - width) * 0.5, 161.0, 16.0, PAPER);
        }
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tick(game: &mut Range, seconds: f32) {
        for _ in 0..(seconds / DT).round() as usize {
            game.step(DT);
        }
    }

    #[test]
    fn stack_stays_upright_without_projectiles() {
        let mut game = Range::new();
        tick(&mut game, 8.0);
        assert_eq!(game.score(), 0);
        assert_eq!(game.phase, Phase::Aiming);
    }

    #[test]
    fn ammunition_cooldown_ccd_and_loss_are_real() {
        let mut game = Range::new();
        game.yaw = 0.5;
        assert!(game.fire());
        assert!(!game.fire());
        assert!(game.physics.bodies[game.shots[0].handle].is_ccd_enabled());
        while game.ammo > 0 {
            tick(&mut game, 0.9);
            assert!(game.fire());
        }
        tick(&mut game, 6.2);
        assert_eq!(game.phase, Phase::Lost);
        assert_eq!(game.shots.len(), AMMO as usize);
        assert!(!game.fire());
    }

    #[test]
    fn autoplay_wins_through_ballistic_contacts() {
        let mut game = Range::new();
        for _ in 0..(30.0 / DT) as usize {
            if game.autoplay() {
                game.fire();
            }
            game.step(DT);
            if game.phase == Phase::Won {
                break;
            }
        }
        assert_eq!(
            game.score(),
            GOAL,
            "score {}, ammo {}",
            game.score(),
            game.ammo
        );
        assert_eq!(game.phase, Phase::Won);
        assert!(game.ammo > 0);
        assert!(!game.shots.is_empty());
    }

    #[test]
    fn shot_input_survives_zero_step_render_frame() {
        let mut clock = FixedClock::new(120.0, 12);
        let mut game = Range::new();
        let mut pending = true;
        assert_eq!(clock.advance(0.001).steps, 0);
        let frame = clock.advance(0.008);
        for _ in 0..frame.steps {
            if pending {
                game.fire();
                pending = false;
            }
            game.step(frame.dt);
        }
        assert_eq!(game.ammo, AMMO - 1);
        assert!(!pending);
    }
}
