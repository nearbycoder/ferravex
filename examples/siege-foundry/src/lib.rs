//! A compact destruction range: all knockdowns come from rigid-body contacts.
//!
//! The host uses a shared session to gate fixed physics ticks. Starting or
//! rebuilding never fires a shot; only a fresh Fire edge can spend ammunition.
//!
//! ```
//! use ferravex_core::session::{GameSession, SessionState};
//! let mut session = GameSession::new(120.0, 12, false);
//! assert!(session.start());
//! let frame = session.advance(1.0 / 60.0);
//! assert_eq!(frame.steps, 2);
//! assert!(session.finish(true));
//! assert_eq!(session.state(), SessionState::Won);
//! assert_eq!(session.advance(1.0).steps, 0);
//! session.restart(); // The host also rebuilds bodies, ammunition and feedback.
//! assert_eq!(session.tick(), 0);
//! ```
use ferravex_core::session::{GameSession, SessionState};
use ferravex_physics::d3::{
    ColliderBuilder, PhysicsWorld, RigidBodyBuilder, RigidBodyHandle, Vector,
};
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{
    HintDevice, PlayerInput, Preset,
    touch::{Control, Output, Region},
};
use ferravex_render::{
    demo::Demo,
    mesh::Mesh3d,
    preferences,
    prelude::*,
    shaders::{LitMaterial, LitSettings},
    ui::{self, ButtonOptions},
};

#[cfg(test)]
const DT: f32 = 1.0 / 120.0;
const AMMO: u8 = 8;
const GOAL: usize = 6;
const SPEED: f32 = 28.0;
const RELOAD: f32 = 0.8;
const SETTLE: f32 = 6.0;
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
    preview: Vec<Vec3>,
    preview_hit: Option<Vec3>,
    preview_aim: (u32, u32),
}

impl Range {
    fn new() -> Self {
        let mut physics = PhysicsWorld::new([0.0, -9.81, 0.0]);
        physics.add_box([0.0, -0.4, 4.0], [14.0, 0.4, 20.0], false);
        let mut blocks = Vec::new();
        for x in [-4.0, 0.0, 4.0] {
            let mut previous = None;
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
                if let Some(a) = previous {
                    physics
                        .insert_joint(
                            a,
                            handle,
                            ferravex_physics::d3::JointBuilder::fixed()
                                .local_anchor1(Vec3::Y * 0.601)
                                .local_anchor2(-Vec3::Y * 0.601)
                                .break_limits(500., 80.),
                        )
                        .unwrap();
                }
                previous = Some(handle);
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
            preview: Vec::with_capacity(45),
            preview_hit: None,
            preview_aim: (u32::MAX, u32::MAX),
        }
    }

    fn refresh_preview(&mut self, simulated: bool) {
        if self.cooldown > 0. {
            return;
        }
        let aim = (self.yaw.to_bits(), self.elevation.to_bits());
        let moving = simulated
            && (self.blocks.iter().any(|block| {
                let body = &self.physics.bodies[block.handle];
                Vec3::from_array(body.translation().to_array()) != block.previous_position
                    || Quat::from_array(body.rotation().to_array()) != block.previous_rotation
            }) || self.shots.iter().any(|shot| {
                Vec3::from_array(self.physics.bodies[shot.handle].translation().to_array())
                    != shot.previous_position
            }));
        if aim == self.preview_aim && !moving {
            return;
        }
        self.preview_aim = aim;
        self.preview.clear();
        self.preview_hit = None;
        let direction = self.direction();
        let start = PIVOT + direction * 1.6;
        self.preview.push(start);
        for i in 1..=44 {
            let t = i as f32 * 0.035;
            let next = start + direction * SPEED * t - Vec3::Y * (4.905 * t * t);
            let previous = *self.preview.last().unwrap();
            let delta = next - previous;
            if let Some(hit) = self.physics.spherecast(
                previous.to_array(),
                delta.to_array(),
                delta.length(),
                0.36,
                u32::MAX,
                None,
            ) {
                let point = Vec3::from_array(hit.point);
                self.preview.push(point);
                self.preview_hit = Some(point);
                break;
            }
            self.preview.push(next);
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
        if !self.can_fire() {
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
        self.cooldown = RELOAD;
        self.since_shot = 0.0;
        true
    }

    fn can_fire(&self) -> bool {
        self.phase == Phase::Aiming && self.cooldown <= 0.0 && self.ammo > 0
    }

    fn step(&mut self, dt: f32) {
        // A result is final: late debris must not change a loss into a win.
        if self.phase != Phase::Aiming {
            return;
        }
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
        } else if self.ammo == 0 && self.since_shot > SETTLE {
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

#[derive(Default)]
struct Feedback {
    shot: f32,
    destruction: f32,
    knocked_down: usize,
    rebuilt: f32,
    recoil: f32,
    motion_scale: f32,
}

impl Feedback {
    fn sync_preferences(&mut self) {
        let policy = preferences::feedback_policy();
        self.apply_motion(policy.reduced_motion(), policy.intensity());
        // This range has no flash, camera shake or particle channels. Static
        // confirmations and target colors remain visible under either policy.
    }

    fn apply_motion(&mut self, reduced: bool, intensity: f32) {
        self.motion_scale = if reduced { 0.0 } else { intensity };
        if self.motion_scale == 0.0 {
            // Forget the old pose impulse, including during paused/zero ticks.
            self.recoil = 0.0;
        }
    }

    fn accepted_shot(&mut self) {
        self.shot = 1.0;
        self.rebuilt = 0.0;
        self.recoil = if self.motion_scale > 0.0 { 0.3 } else { 0.0 };
    }

    fn barrel_offset(&self) -> f32 {
        self.recoil * self.motion_scale
    }

    fn step(&mut self, dt: f32) {
        self.shot = (self.shot - dt).max(0.0);
        self.destruction = (self.destruction - dt).max(0.0);
        self.rebuilt = (self.rebuilt - dt).max(0.0);
        self.recoil = (self.recoil - dt * 1.2).max(0.0);
    }
}

#[derive(Default)]
struct FrameFeedback {
    alpha: f32,
    fired: bool,
    destroyed: usize,
    outcome: Option<Phase>,
}

#[derive(Default)]
struct TickInput {
    fire: bool,
    aim: Vec2,
}

/// Session host shared by interactive play and headless tests. Presentation
/// policy is read here, outside the range's fixed physics and gameplay rules.
struct Foundry {
    range: Range,
    session: GameSession,
    pending_fire: bool,
    feedback: Feedback,
}

impl Foundry {
    fn new(autoplay: bool) -> Self {
        Self {
            range: Range::new(),
            session: GameSession::new(120.0, 12, autoplay),
            pending_fire: false,
            feedback: Feedback::default(),
        }
    }

    fn rebuild(&mut self) {
        self.range = Range::new();
        self.session.restart();
        self.pending_fire = false;
        self.feedback = Feedback {
            rebuilt: 1.8,
            ..Feedback::default()
        };
    }

    fn set_paused(&mut self, paused: bool) {
        self.session.set_paused(paused);
        if paused {
            self.pending_fire = false;
        }
    }

    fn queue_fire(&mut self) {
        // Ignore reload/terminal presses instead of deferring an unwanted shot.
        if self.session.is_playing() && self.range.can_fire() {
            self.pending_fire = true;
        }
    }

    #[cfg(test)]
    fn advance(&mut self, elapsed: f64, autoplay: bool) -> FrameFeedback {
        self.advance_input(elapsed, autoplay, TickInput::default)
    }

    fn advance_input(
        &mut self,
        elapsed: f64,
        autoplay: bool,
        mut input: impl FnMut() -> TickInput,
    ) -> FrameFeedback {
        // Presentation settings are sampled even when session.advance produces
        // no ticks (ready, paused, controls, results, or a high render rate).
        self.feedback.sync_preferences();
        let frame = self.session.advance(elapsed);
        let mut events = FrameFeedback {
            alpha: frame.alpha,
            ..FrameFeedback::default()
        };
        for _ in 0..frame.steps {
            self.feedback.step(frame.dt);
            let input = input();
            if !autoplay {
                self.range.yaw =
                    (self.range.yaw + input.aim.x * frame.dt * 0.33).clamp(-0.55, 0.55);
                self.range.elevation =
                    (self.range.elevation + input.aim.y * frame.dt * 0.2).clamp(0.025, 0.65);
                if input.fire {
                    self.queue_fire();
                }
            }
            if autoplay && self.range.autoplay() {
                self.queue_fire();
            }
            if std::mem::take(&mut self.pending_fire) && self.range.fire() {
                self.feedback.accepted_shot();
                events.fired = true;
            }
            let before = self.range.score();
            self.range.step(frame.dt);
            let destroyed = self.range.score() - before;
            if destroyed > 0 {
                self.feedback.knocked_down = if self.feedback.destruction > 0.0 {
                    self.feedback.knocked_down + destroyed
                } else {
                    destroyed
                };
                self.feedback.destruction = 2.0;
                events.destroyed += destroyed;
            }
            if self.range.phase != Phase::Aiming {
                self.session.finish(self.range.phase == Phase::Won);
                self.pending_fire = false;
                events.outcome = Some(self.range.phase);
                break;
            }
        }
        if !self.session.is_playing() {
            // Show the current pose when frozen, not the prior tick's pose.
            events.alpha = 1.0;
        }
        self.range.refresh_preview(frame.steps > 0);
        events
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

    fn draw(&self, game: &Range, alpha: f32, recoil: f32, material: &LitMaterial) {
        let camera = RangeCamera::new(screen_width(), screen_height());
        set_camera(&camera);
        material.begin(LitSettings {
            camera_position: camera.eye,
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
        self.barrel.draw(PIVOT + direction * (0.3 - recoil), q);
        self.muzzle.draw(PIVOT + direction * (1.515 - recoil), q);
        material.end();
        if game.phase == Phase::Aiming && game.ammo > 0 && game.cooldown == 0. {
            for (i, segment) in game.preview.windows(2).enumerate() {
                if i % 2 == 0 {
                    draw_line_3d(segment[0], segment[1], Color::from_rgba(249, 225, 157, 180));
                }
            }
            if let Some(hit) = game.preview_hit {
                let size = 0.38;
                draw_line_3d(hit - Vec3::X * size, hit + Vec3::X * size, PAPER);
                draw_line_3d(hit - Vec3::Y * size, hit + Vec3::Y * size, PAPER);
            }
        }
        set_default_camera();
    }
}

/// Fits the authored range between the HUD panels without moving the eye,
/// changing depth/fog, or coupling the view to moving debris or player aim.
struct RangeCamera {
    matrix: Mat4,
    eye: Vec3,
}

impl RangeCamera {
    fn new(w: f32, h: f32) -> Self {
        let base = Camera3D {
            position: vec3(23.0, 18.0, 28.0),
            target: vec3(0.0, 1.5, 3.5),
            up: Vec3::Y,
            fovy: 43.0_f32.to_radians(),
            z_near: 0.1,
            z_far: 130.0,
            ..Default::default()
        };
        let matrix = base.matrix(w, h);
        let scene = HudLayout::new(w, h).scene;
        let mut low = vec2(f32::INFINITY, f32::INFINITY);
        let mut high = vec2(f32::NEG_INFINITY, f32::NEG_INFINITY);
        // Standing towers, cannon including every legal muzzle direction, and
        // the low arcs that reach the supports. The decoration may extend under
        // the HUD; flying balls/debris never cause camera motion.
        for (min, max) in [
            (vec3(-4.6, 0.0, -4.6), vec3(4.6, 4.81, -3.4)),
            (vec3(-1.4, 0.0, 13.0), vec3(1.4, 3.0, 16.4)),
        ] {
            for x in [min.x, max.x] {
                for y in [min.y, max.y] {
                    for z in [min.z, max.z] {
                        let p = matrix.project_point3(vec3(x, y, z));
                        let p = vec2((p.x + 1.0) * w * 0.5, (1.0 - p.y) * h * 0.5);
                        low = low.min(p);
                        high = high.max(p);
                    }
                }
            }
        }
        let scale = (scene.w / (high.x - low.x))
            .min(scene.h / (high.y - low.y))
            .min(1.0);
        // Smallest translation that contains the fitted bounds. In particular,
        // an already fitting desktop view retains its original composition.
        let shift_axis = |low: f32, high: f32, start: f32, size: f32| {
            if low * scale < start {
                start - low * scale
            } else if high * scale > start + size {
                start + size - high * scale
            } else {
                0.0
            }
        };
        let shift = vec2(
            shift_axis(low.x, high.x, scene.x, scene.w),
            shift_axis(low.y, high.y, scene.y, scene.h),
        );
        let offset = vec3(
            scale - 1.0 + 2.0 * shift.x / w,
            1.0 - scale - 2.0 * shift.y / h,
            0.0,
        );
        Self {
            matrix: Mat4::from_translation(offset)
                * Mat4::from_scale(vec3(scale, scale, 1.0))
                * matrix,
            eye: base.position,
        }
    }
}

impl Camera for RangeCamera {
    fn matrix(&self, _: f32, _: f32) -> Mat4 {
        self.matrix
    }

    fn depth(&self) -> bool {
        true
    }

    fn position(&self) -> Option<Vec3> {
        Some(self.eye)
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

fn clicked(rect: Rect) -> bool {
    rect.contains(Vec2::from(mouse_position())) && is_mouse_button_pressed(MouseButton::Left)
}

fn button(rect: Rect, title: &str, enabled: bool) {
    // Keep click-on-press input resolution before simulation; the common helper
    // supplies the matching disabled/hover palette and bounded label sizing.
    let _ = ui::button_with_options(
        rect,
        title,
        ButtonOptions {
            enabled,
            selected: false,
        },
    );
}

fn fitted_label(text: &str, rect: Rect, size: u16, centered: bool) {
    let metrics = get_default_font()
        .try_metrics(size, 1.0)
        .expect("HUD font size");
    let fitted =
        ui::fit_label(text, rect.w.max(0.0), metrics.glyph_advance()).expect("finite HUD bounds");
    let x = rect.x
        + if centered {
            (rect.w - fitted.width) * 0.5
        } else {
            0.0
        };
    label(fitted.prefix, x, rect.y, f32::from(size), PAPER);
    let prefix_width = metrics
        .measure_line(fitted.prefix)
        .expect("bounded HUD label")
        .width;
    label(
        fitted.suffix,
        x + prefix_width,
        rect.y,
        f32::from(size),
        PAPER,
    );
}

fn action_hint<'a>(input: &PlayerInput, action: &str, buffer: &'a mut [u8]) -> &'a str {
    if input.current_device() == HintDevice::Touch {
        // The preset's Touch 2/confirm channels are not installed here: this
        // range uses its visible pointer buttons and a look-only touch region.
        return "on-screen button";
    }
    // Never silently advertise a default binding after a remap or format error.
    input.format_hint(action, buffer).unwrap_or("See Controls")
}

struct HudLayout {
    scene: Rect,
    panel: Rect,
    fire: Rect,
    rebuild: Rect,
    pause: Rect,
}

impl HudLayout {
    fn new(w: f32, h: f32) -> Self {
        let height = 116.0;
        let panel = Rect::new(16.0, h - height - 16.0, w - 32.0, height);
        let width = (panel.w - 48.0) / 3.0;
        let y = panel.y + panel.h - 54.0;
        Self {
            scene: Rect::new(16.0, 56.0, w - 32.0, (panel.y - 64.0).max(1.0)),
            panel,
            fire: Rect::new(32.0, y, width, 44.0),
            rebuild: Rect::new(40.0 + width, y, width, 44.0),
            pause: Rect::new(48.0 + width * 2.0, y, width, 44.0),
        }
    }
}

fn draw_hud(foundry: &Foundry, layout: &HudLayout, input: &PlayerInput) {
    let game = &foundry.range;
    let controls_open = input.controls_open();
    let w = screen_width();
    let state = foundry.session.state();
    let panel_color = Color::new(0.10, 0.14, 0.13, 0.94);
    draw_rectangle(16.0, 12.0, (w - 96.0).min(280.0), 32.0, panel_color);
    label(
        &format!("TARGETS {} / {}", game.score(), GOAL),
        28.0,
        34.0,
        16.0,
        PAPER,
    );
    let panel = layout.panel;
    draw_rectangle(panel.x, panel.y, panel.w, panel.h, panel_color);
    label(
        &format!("SHOTS {} / {AMMO}", game.ammo),
        28.0,
        panel.y + 24.0,
        16.0,
        PAPER,
    );
    let feedback = &foundry.feedback;
    let status = if feedback.destruction > 0.0 {
        format!("+{} TARGETS DOWN", feedback.knocked_down)
    } else if feedback.shot > 0.0 {
        format!("SHOT {} AWAY", AMMO - game.ammo)
    } else if feedback.rebuilt > 0.0 {
        "REBUILT: 8 SHOTS READY".into()
    } else if game.ammo == 0 && game.phase == Phase::Aiming {
        format!("SETTLING {:.1}s", (SETTLE - game.since_shot).max(0.0))
    } else if game.cooldown > 0.0 {
        format!("RELOADING {:.1}s", game.cooldown)
    } else {
        "".into()
    };
    fitted_label(
        &status,
        Rect::new(28.0, panel.y + 48.0, panel.w - 24.0, 20.0),
        16,
        false,
    );
    let fire_title = match state {
        SessionState::Ready => "START",
        SessionState::Won => "AGAIN",
        SessionState::Lost => "RETRY",
        SessionState::Paused => "PAUSED",
        SessionState::Playing if game.ammo == 0 => "EMPTY",
        SessionState::Playing if game.cooldown > 0.0 => "LOADING",
        SessionState::Playing => "FIRE",
    };
    let fire_enabled = !controls_open
        && match state {
            SessionState::Ready | SessionState::Won | SessionState::Lost => true,
            SessionState::Playing => game.can_fire(),
            SessionState::Paused => false,
        };
    button(layout.fire, fire_title, fire_enabled);
    button(layout.rebuild, "REBUILD", !controls_open);
    button(
        layout.pause,
        if foundry.session.is_paused() {
            "RESUME"
        } else {
            "PAUSE"
        },
        !controls_open && matches!(state, SessionState::Playing | SessionState::Paused),
    );
    if game.cooldown > 0.0 {
        draw_rectangle(
            layout.fire.x,
            layout.fire.y + layout.fire.h + 5.0,
            layout.fire.w,
            3.0,
            STEEL,
        );
        draw_rectangle(
            layout.fire.x,
            layout.fire.y + layout.fire.h + 5.0,
            layout.fire.w * (1.0 - game.cooldown / RELOAD),
            3.0,
            PAPER,
        );
    }
    let (title, detail, action, button) = match state {
        SessionState::Ready => (
            "DEMOLITION RANGE",
            "6 targets. 8 shots.",
            "confirm",
            "START",
        ),
        SessionState::Paused => (
            "RANGE PAUSED",
            "Shots + debris frozen.",
            "pause",
            if controls_open {
                "CLOSE CONTROLS"
            } else {
                "RESUME"
            },
        ),
        SessionState::Won => ("FOUNDRY CLEARED", "All 6 targets down.", "confirm", "AGAIN"),
        SessionState::Lost => ("OUT OF SHOTS", "Break the supports.", "confirm", "RETRY"),
        SessionState::Playing => return,
    };
    let mut hint_buffer = [0; 512];
    let prompt = if controls_open {
        "Close Controls first.".to_owned()
    } else {
        format!(
            "{button} / {}",
            action_hint(input, action, &mut hint_buffer)
        )
    };
    let panel_width = (w - 32.0).min(430.0);
    let y = 104.0;
    draw_rectangle((w - panel_width) * 0.5, y, panel_width, 112.0, panel_color);
    for (text, offset, size) in [
        (title, 30.0, 20),
        (detail, 61.0, 16),
        (prompt.as_str(), 90.0, 16),
    ] {
        fitted_label(
            text,
            Rect::new(
                (w - panel_width) * 0.5 + 12.0,
                y + offset,
                panel_width - 24.0,
                24.0,
            ),
            size,
            true,
        );
    }
}

fn window_conf() -> Conf {
    ferravex_render::window_conf("Siege Foundry / Ferravex")
}

ferravex_render::game!(window_conf, game);
async fn game() {
    let mut demo = Demo::new();
    let mut foundry = Foundry::new(demo.autoplay);
    let art = Art::new();
    let material = LitMaterial::new().expect("Ferravex lit material");
    let mut sound = GameAudio::new();
    let mut player_input = PlayerInput::preset("siege-foundry", Preset::Turret);
    player_input.set_mouse_look_button(Some(1)).unwrap();
    loop {
        ferravex_render::ui::warm_up_font_sizes(&[16, 19, 20, 23]);
        let layout = HudLayout::new(screen_width(), screen_height());
        player_input
            .set_touch_layout(&[Control {
                region: Region::Rect(Rect::new(
                    0.0,
                    90.0,
                    screen_width(),
                    (layout.panel.y - 90.0).max(1.0),
                )),
                output: Output::Look { x: 0, y: 1 },
            }])
            .unwrap();
        // Resolve pointer pause before sampling, so keyboard and pointer resume
        // both re-enable the gameplay context on this frame.
        if !demo.autoplay
            && !player_input.controls_open()
            && matches!(
                foundry.session.state(),
                SessionState::Playing | SessionState::Paused
            )
            && clicked(layout.pause)
        {
            player_input.set_paused(!foundry.session.is_paused());
        }
        player_input.sample_play();
        let dt = demo.dt();
        let active = demo.autoplay || player_input.gameplay_active() && !player_input.paused();
        let controls_open = player_input.controls_open();
        let confirm = player_input.pressed("confirm");
        let rebuild = player_input.pressed("restart") || !controls_open && clicked(layout.rebuild);
        // Drain relative look while inactive, including the rebuild frame.
        let look = player_input.look(dt);
        let state = foundry.session.state();
        let pointer_fire = !controls_open && clicked(layout.fire);
        // Fire doubles as a menu action only outside play. During play, leave
        // its edge in PlayerInput until an actual simulation tick consumes it.
        let menu_fire = if state != SessionState::Playing || rebuild {
            player_input.pressed("fire")
        } else {
            false
        };
        let restarted = rebuild
            || matches!(state, SessionState::Won | SessionState::Lost)
                && (confirm || menu_fire || pointer_fire);
        let started = state == SessionState::Ready && (confirm || menu_fire || pointer_fire);
        if restarted {
            foundry.rebuild();
            player_input.set_paused(false);
            sound.reset();
        } else if started {
            foundry.session.start();
            player_input.set_paused(false);
        } else {
            foundry.set_paused(!active);
        }
        if foundry.session.is_playing() && active && !restarted && !started {
            let game = &mut foundry.range;
            let pointer = Vec2::from(mouse_position());
            if pointer.y > 90.0 && pointer.y < layout.panel.y || player_input.touch_active() {
                game.yaw += look.x * 0.7;
                game.elevation -= look.y * 0.45;
            }
            game.yaw = game.yaw.clamp(-0.55, 0.55);
            game.elevation = game.elevation.clamp(0.025, 0.65);
            if pointer_fire {
                foundry.queue_fire();
            }
        }
        let events = foundry.advance_input(
            if restarted || started {
                0.0
            } else {
                f64::from(dt)
            },
            demo.autoplay,
            || TickInput {
                fire: player_input.take_tick(["fire"])[0].pressed,
                aim: player_input.movement(),
            },
        );
        if events.fired {
            sound.cue(Cue::Fire, 0.8);
        }
        if events.destroyed > 0 {
            sound.cue(Cue::Impact(Surface::Metal), 0.6);
        }
        if let Some(outcome) = events.outcome {
            sound.cue(
                if outcome == Phase::Won {
                    Cue::Success
                } else {
                    Cue::Failure
                },
                0.65,
            );
        }
        clear_background(SKY);
        art.draw(
            &foundry.range,
            events.alpha,
            foundry.feedback.barrel_offset(),
            &material,
        );
        draw_hud(&foundry, &layout, &player_input);
        // The range supplies its own pause panel; retain the shared editor.
        if !player_input.paused() || controls_open {
            player_input.draw_settings();
        }
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod journey200_tests;

#[cfg(test)]
mod journey200_194;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_input_is_not_read_while_ready_paused_or_on_zero_tick_frames() {
        let mut foundry = Foundry::new(false);
        let mut calls = 0;
        let mut fire = true;
        let mut input = || {
            calls += 1;
            TickInput {
                fire: std::mem::take(&mut fire),
                aim: Vec2::ZERO,
            }
        };
        foundry.advance_input(1.0, false, &mut input);
        foundry.session.start();
        foundry.advance_input(0.001, false, &mut input);
        foundry.set_paused(true);
        foundry.advance_input(1.0, false, &mut input);
        foundry.set_paused(false);
        let events = foundry.advance_input(0.05, false, &mut input);
        assert!(events.fired);
        assert_eq!(calls, 6);
        assert!(!fire);
        assert_eq!(foundry.range.shots.len(), 1);
        assert_eq!(foundry.range.ammo, AMMO - 1);
    }

    #[test]
    fn player_input_taps_survive_zero_ticks_and_reload_rejection_is_not_buffered() {
        use ferravex_render::input::{self, ActionKind, Binding, InputCode, PadState};
        let mut player = PlayerInput::transient("journey200-139-ticks");
        player
            .action(
                "fire",
                "Fire",
                ActionKind::Button,
                vec![Binding::digital(InputCode::PadButton(7))],
            )
            .unwrap();
        let mut pad = PadState {
            connected: true,
            ..Default::default()
        };
        let mut foundry = Foundry::new(true);
        foundry.range.yaw = 0.5;
        let mut frame = |dt, button| {
            pad.buttons[7] = button;
            assert!(input::gamepad_sample(0, pad));
            input::host_frame(dt as f32, 320, 448, || {
                player.sample_play();
                foundry
                    .advance_input(dt, false, || TickInput {
                        fire: player.take_tick(["fire"])[0].pressed,
                        aim: Vec2::ZERO,
                    })
                    .fired
            })
        };
        // Connect released first, then press/release entirely before a tick.
        assert!(!frame(0.0, 0.0));
        assert!(!frame(0.001, 1.0));
        assert!(!frame(0.001, 0.0));
        assert!(frame(0.05, 0.0));
        assert!(!frame(0.001, 1.0));
        assert!(!frame(0.1, 0.0)); // Rejected at the tick while still loading.
        for _ in 0..10 {
            assert!(!frame(0.1, 0.0));
        }
        assert!(frame(0.01, 1.0)); // Only a fresh ready press fires again.
        assert_eq!(foundry.range.ammo, AMMO - 2);
        assert_eq!(foundry.range.shots.len(), 2);
        assert!(input::gamepad_sample(0, PadState::default()));
    }

    #[test]
    fn held_aim_uses_equal_fixed_ticks_at_30_60_and_144_hz() {
        let mut reference = None;
        for fps in [30, 60, 144] {
            let mut foundry = Foundry::new(true);
            for _ in 0..fps {
                foundry.advance_input(1.0 / f64::from(fps), false, || TickInput {
                    fire: false,
                    aim: vec2(0.5, 0.25),
                });
            }
            assert_eq!(foundry.session.tick(), 120);
            let aim = (foundry.range.yaw, foundry.range.elevation);
            assert!(aim.0 > 0.16 && aim.0 < 0.17);
            assert!(aim.1 > 0.17 && aim.1 < 0.19);
            if let Some(reference) = reference {
                assert_eq!(aim, reference);
            }
            reference = Some(aim);
        }
    }

    #[test]
    fn motion_policy_cancels_old_recoil_without_erasing_shot_confirmation() {
        let mut feedback = Feedback::default();
        feedback.apply_motion(false, 1.0);
        feedback.accepted_shot();
        assert_eq!(feedback.barrel_offset(), 0.3);
        feedback.apply_motion(false, 0.5);
        assert_eq!(feedback.barrel_offset(), 0.15);
        // No time need pass: changing policy while paused must retire the pose.
        feedback.apply_motion(true, 1.0);
        assert_eq!(feedback.barrel_offset(), 0.0);
        assert_eq!(feedback.shot, 1.0);
        feedback.apply_motion(false, 1.0);
        assert_eq!(feedback.barrel_offset(), 0.0);
        feedback.apply_motion(true, 1.0);
        feedback.accepted_shot();
        feedback.apply_motion(false, 1.0);
        assert_eq!(
            feedback.barrel_offset(),
            0.0,
            "disabled shots cannot revive"
        );
        feedback.accepted_shot();
        assert!(feedback.barrel_offset() > 0.0);
        feedback.apply_motion(false, 0.0);
        feedback.apply_motion(false, 1.0);
        assert_eq!(feedback.barrel_offset(), 0.0);
    }

    #[test]
    fn ready_paused_and_terminal_frames_sample_shared_presentation_policy() {
        preferences::set_feedback_policy(Default::default());
        let mut foundry = Foundry::new(false);
        for state in [
            SessionState::Ready,
            SessionState::Playing,
            SessionState::Paused,
            SessionState::Won,
        ] {
            match state {
                SessionState::Playing => {
                    foundry.session.start();
                }
                SessionState::Paused => foundry.set_paused(true),
                SessionState::Won => {
                    foundry.session.finish(true);
                }
                _ => {}
            }
            foundry.feedback.motion_scale = 0.25;
            foundry.advance(0.0, false);
            assert_eq!(foundry.feedback.motion_scale, 1.0, "{state:?}");
            assert_eq!(foundry.session.tick(), 0);
            assert_eq!(foundry.range.ammo, AMMO);
        }
    }

    #[test]
    fn hud_hints_follow_remaps_unbound_actions_and_bounded_buffer_errors() {
        use ferravex_render::input::{ActionKind, Binding, InputCode};
        let mut player = PlayerInput::transient("journey200-139-hints");
        player
            .action(
                "fire",
                "Fire",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .unwrap();
        let mut buffer = [0; 512];
        assert_eq!(action_hint(&player, "fire", &mut buffer), "Space");
        player
            .rebind("fire", vec![Binding::digital(InputCode::Key(b'f' as u16))])
            .unwrap();
        assert_eq!(action_hint(&player, "fire", &mut buffer), "F");
        player.rebind("fire", vec![]).unwrap();
        assert_eq!(action_hint(&player, "fire", &mut buffer), "Unbound");
        assert_eq!(action_hint(&player, "fire", &mut []), "See Controls");
        assert_eq!(action_hint(&player, "missing", &mut buffer), "See Controls");
    }

    #[test]
    fn narrow_hud_keeps_controls_and_fixed_size_text_inside_panels() {
        let metrics = get_default_font().try_metrics(16, 1.0).unwrap();
        for width in [320.0, 390.0, 700.0, 1280.0] {
            let layout = HudLayout::new(width, 448.0);
            let mut last_right = layout.panel.x;
            for rect in [layout.fire, layout.rebuild, layout.pause] {
                assert!(rect.x >= last_right);
                assert!(rect.x + rect.w <= layout.panel.x + layout.panel.w);
                assert!(rect.y >= layout.panel.y);
                assert!(rect.y + rect.h <= layout.panel.y + layout.panel.h);
                assert!(rect.h >= 44.0);
                last_right = rect.x + rect.w;
            }
            assert!(
                layout.panel.y > 216.0,
                "result panel must not cover buttons"
            );
            for text in [
                "YAW -32 / ELEV 37.2",
                "POWER 28 m/s - fixed",
                "REBUILT: 8 SHOTS READY",
                "SETTLING 6.0s",
            ] {
                let measured = metrics.measure_line(text).unwrap();
                assert_eq!(measured.unsupported_glyphs, 0);
                assert!(measured.width <= layout.panel.w - 24.0, "{text}: {width}");
            }
            let long_hint = "FIRE: Left Shift / Right Shift / Space / Enter";
            let fitted =
                ui::fit_label(long_hint, layout.panel.w - 24.0, metrics.glyph_advance()).unwrap();
            assert!(fitted.width <= layout.panel.w - 24.0);
            assert_eq!(
                metrics.measure_line(fitted.prefix).unwrap().width
                    + metrics.measure_line(fitted.suffix).unwrap().width,
                fitted.width
            );
        }
    }

    #[test]
    fn trajectory_preview_stops_at_real_tower_geometry_and_reuses_storage() {
        let mut game = Range::new();
        game.aim_at(vec3(0., 1.8, -4.));
        game.refresh_preview(false);
        let hit = game
            .preview_hit
            .expect("arc should hit the front of the tower");
        assert!(hit.z > -3.2 && hit.z < -2.8, "{hit:?}");
        assert_eq!(game.preview.last(), Some(&hit));
        let capacity = game.preview.capacity();
        let old = game.preview.clone();
        game.refresh_preview(false);
        assert_eq!(game.preview, old);
        game.yaw += 0.4;
        game.refresh_preview(false);
        assert_ne!(game.preview, old);
        assert_eq!(game.preview.capacity(), capacity);
    }

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
        for fps in [30, 144] {
            let mut foundry = Foundry::new(true);
            let mut fired = 0;
            let mut destroyed = 0;
            let mut outcomes = 0;
            for _ in 0..30 * fps {
                let events = foundry.advance(1.0 / f64::from(fps), true);
                fired += usize::from(events.fired);
                destroyed += events.destroyed;
                outcomes += usize::from(events.outcome.is_some());
                if foundry.session.state() == SessionState::Won {
                    break;
                }
            }
            let game = &foundry.range;
            assert_eq!(game.score(), GOAL, "{fps} FPS, ammo {}", game.ammo);
            assert_eq!(game.phase, Phase::Won);
            assert_eq!(foundry.session.state(), SessionState::Won);
            assert!(game.ammo > 0);
            assert!(fired > 0);
            assert_eq!(fired, game.shots.len());
            assert_eq!(destroyed, GOAL);
            assert_eq!(outcomes, 1);
        }
    }

    #[test]
    fn shot_input_survives_zero_step_frame_and_is_consumed_once_during_catchup() {
        let mut foundry = Foundry::new(true);
        foundry.range.yaw = 0.5;
        foundry.queue_fire();
        assert!(!foundry.advance(0.001, false).fired);
        assert!(foundry.pending_fire);
        assert_eq!(foundry.range.ammo, AMMO);
        assert!(foundry.advance(0.05, false).fired);
        assert!(!foundry.pending_fire);
        assert_eq!(foundry.range.ammo, AMMO - 1);
        for _ in 0..20 {
            assert!(!foundry.advance(0.1, false).fired);
        }
        assert_eq!(foundry.range.shots.len(), 1);
    }

    #[test]
    fn ready_session_cannot_spend_ammo_or_accumulate_physics_time() {
        let mut foundry = Foundry::new(false);
        foundry.queue_fire();
        let events = foundry.advance(50.0, false);
        assert!(!events.fired);
        assert_eq!(foundry.range.elapsed, 0.0);
        assert_eq!(foundry.range.ammo, AMMO);
        assert_eq!(foundry.session.tick(), 0);
        assert!(foundry.session.start());
        assert!(!foundry.advance(1.0 / 120.0, false).fired);
        assert_eq!(foundry.session.tick(), 1);
        foundry.queue_fire();
        assert!(foundry.advance(1.0 / 120.0, false).fired);
    }

    #[test]
    fn rejected_reload_edge_does_not_fire_later_or_refresh_feedback() {
        let mut foundry = Foundry::new(true);
        foundry.range.yaw = 0.5;
        foundry.queue_fire();
        assert!(foundry.advance(1.0 / 120.0, false).fired);
        let shot_feedback = foundry.feedback.shot;
        foundry.queue_fire();
        assert!(!foundry.pending_fire);
        assert_eq!(foundry.feedback.shot, shot_feedback);
        for _ in 0..12 {
            assert!(!foundry.advance(0.1, false).fired);
        }
        assert_eq!(foundry.range.ammo, AMMO - 1);
        assert_eq!(foundry.feedback.shot, 0.0);
        foundry.queue_fire();
        assert!(foundry.advance(1.0 / 120.0, false).fired);
    }

    #[test]
    fn pause_clears_buffered_input_and_freezes_reload_feedback_and_physics() {
        let mut foundry = Foundry::new(true);
        foundry.queue_fire();
        foundry.advance(0.001, false);
        foundry.set_paused(true);
        foundry.queue_fire();
        assert!(!foundry.pending_fire);
        foundry.advance(10.0, false);
        assert_eq!(foundry.range.elapsed, 0.0);
        foundry.set_paused(false);
        assert!(!foundry.advance(1.0 / 120.0, false).fired);
        assert_eq!(foundry.session.tick(), 1);
        foundry.queue_fire();
        foundry.advance(1.0 / 120.0, false);
        let before = (
            foundry.range.elapsed,
            foundry.range.cooldown,
            foundry.feedback.shot,
        );
        let handle = foundry.range.shots[0].handle;
        let position = foundry.range.physics.bodies[handle].translation();
        let tick = foundry.session.tick();
        foundry.set_paused(true);
        let events = foundry.advance(60.0, true);
        assert_eq!(events.alpha, 1.0);
        assert!(!events.fired);
        assert_eq!(
            before,
            (
                foundry.range.elapsed,
                foundry.range.cooldown,
                foundry.feedback.shot
            )
        );
        assert_eq!(position, foundry.range.physics.bodies[handle].translation());
        assert_eq!(tick, foundry.session.tick());
    }

    #[test]
    fn rebuild_restores_resources_and_drops_input_clock_and_feedback_debt() {
        let mut foundry = Foundry::new(true);
        foundry.range.yaw = 0.4;
        foundry.range.elevation = 0.6;
        foundry.queue_fire();
        foundry.advance(0.05, false);
        assert_eq!(foundry.range.physics.bodies.len(), 14);
        for _ in 0..9 {
            foundry.advance(0.1, false);
        }
        foundry.range.blocks[3].down = true;
        foundry.feedback.destruction = 1.0;
        foundry.feedback.knocked_down = 1;
        foundry.queue_fire();
        assert!(foundry.pending_fire);
        foundry.rebuild();
        let game = &foundry.range;
        assert_eq!(foundry.session.state(), SessionState::Playing);
        assert_eq!(foundry.session.tick(), 0);
        assert_eq!(game.ammo, AMMO);
        assert_eq!(game.score(), 0);
        assert_eq!(game.phase, Phase::Aiming);
        assert!(game.shots.is_empty());
        assert_eq!(game.physics.bodies.len(), 13);
        assert_eq!(game.physics.colliders.len(), 13);
        assert_eq!(game.yaw, 0.0);
        assert_eq!(game.elevation, 0.13);
        assert_eq!(
            (game.elapsed, game.cooldown, game.since_shot),
            (0.0, 0.0, 0.0)
        );
        assert!(game.preview.is_empty());
        assert!(game.preview_hit.is_none());
        assert!(!foundry.pending_fire);
        assert_eq!(
            (foundry.feedback.shot, foundry.feedback.destruction),
            (0.0, 0.0)
        );
        assert_eq!(foundry.feedback.knocked_down, 0);
        assert!(foundry.feedback.rebuilt > 0.0);
        assert!(!foundry.advance(0.001, false).fired);
        assert_eq!(foundry.session.tick(), 0);
        assert_eq!(foundry.range.elapsed, 0.0);
        foundry.set_paused(true);
        foundry.rebuild();
        assert_eq!(foundry.session.state(), SessionState::Playing);
        assert_eq!(foundry.range.ammo, AMMO);
    }

    #[test]
    fn last_shot_gets_full_settling_time_before_loss() {
        let mut foundry = Foundry::new(true);
        foundry.range.yaw = 0.5;
        while foundry.range.ammo > 0 {
            foundry.queue_fire();
            foundry.advance(1.0 / 120.0, false);
            if foundry.range.ammo > 0 {
                for _ in 0..9 {
                    foundry.advance(0.1, false);
                }
            }
        }
        for _ in 0..59 {
            foundry.advance(0.1, false);
        }
        assert_eq!(foundry.session.state(), SessionState::Playing);
        assert_eq!(foundry.range.phase, Phase::Aiming);
        let events = foundry.advance(0.1, false);
        assert_eq!(events.outcome, Some(Phase::Lost));
        assert_eq!(foundry.session.state(), SessionState::Lost);
        assert!(!foundry.range.fire());
    }

    #[test]
    fn terminal_result_is_emitted_once_and_stops_remaining_catchup_steps() {
        for won in [true, false] {
            let mut foundry = Foundry::new(true);
            if won {
                for block in &mut foundry.range.blocks {
                    block.down = block.target;
                }
            } else {
                foundry.range.ammo = 0;
                foundry.range.since_shot = SETTLE;
            }
            let phase = if won { Phase::Won } else { Phase::Lost };
            let events = foundry.advance(0.1, false);
            assert_eq!(events.outcome, Some(phase));
            assert_eq!(events.alpha, 1.0);
            assert_eq!(foundry.range.elapsed, DT);
            let handle = foundry.range.blocks[3].handle;
            let position = foundry.range.physics.bodies[handle].translation();
            for block in &mut foundry.range.blocks {
                block.down = block.target;
            }
            foundry.queue_fire();
            assert!(!foundry.advance(1.0, true).fired);
            assert!(foundry.advance(1.0, true).outcome.is_none());
            foundry.range.step(DT);
            assert_eq!(
                foundry.range.phase, phase,
                "a late knockdown cannot reverse a verdict"
            );
            assert_eq!(foundry.range.elapsed, DT);
            assert_eq!(position, foundry.range.physics.bodies[handle].translation());
            foundry.rebuild();
            assert_eq!(foundry.session.state(), SessionState::Playing);
            assert_eq!(foundry.range.ammo, AMMO);
            assert_eq!(foundry.range.score(), 0);
        }
    }
}
