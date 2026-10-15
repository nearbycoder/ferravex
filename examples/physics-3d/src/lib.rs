//! Flat-shaded 3D rigid bodies, interpolated fixed-tick physics, and an orbit camera.
use ferravex_core::{FixedClock, FrameStats};
use ferravex_physics::d3::{
    ColliderBuilder, JointBuilder, PhysicsWorld, RigidBodyBuilder, RigidBodyHandle,
};
use ferravex_physics::{JointHandle, JointMotor};
use ferravex_play::camera::{Input3, Mode3, NoObstructions, Rig3, Settings3, Target3};
use ferravex_render::{
    mesh::Mesh3d,
    prelude::*,
    shaders::{LitMaterial, LitSettings},
    ui,
};

const MAX_BODIES: usize = 180;
const CONCRETE: Color = Color::from_rgba(106, 116, 116, 255);
const CRATE: Color = Color::from_rgba(173, 125, 68, 255);
const PAINT: Color = Color::from_rgba(102, 143, 141, 255);

struct Body {
    handle: RigidBodyHandle,
    ball: bool,
    compound: bool,
    previous_position: Vec3,
    previous_rotation: Quat,
}

struct Sandbox {
    physics: PhysicsWorld,
    bodies: Vec<Body>,
    sequence: u32,
    links: Vec<JointHandle>,
}

impl Sandbox {
    fn new() -> Self {
        let mut sandbox = Self {
            physics: PhysicsWorld::new([0.0, -9.81, 0.0]),
            bodies: Vec::new(),
            sequence: 0,
            links: Vec::new(),
        };
        sandbox
            .physics
            .add_box([0.0, -0.4, 0.0], [10.0, 0.4, 8.0], false);
        // Low perimeter curbs catch rolling objects without obscuring the scene.
        for (position, half) in [
            ([0.0, 0.25, -8.0], [10.25, 0.25, 0.25]),
            ([0.0, 0.25, 8.0], [10.25, 0.25, 0.25]),
            ([-10.0, 0.25, 0.0], [0.25, 0.25, 8.0]),
            ([10.0, 0.25, 0.0], [0.25, 0.25, 8.0]),
        ] {
            sandbox.physics.add_box(position, half, false);
        }
        for x in [-9.4, 9.4] {
            for z in [-7.4, 7.4] {
                sandbox
                    .physics
                    .add_box([x, 0.4, z], [0.29, 0.4, 0.29], false);
            }
        }
        // A compact crate stack built at physical scale: one unit is one meter.
        for y in 0..5 {
            for x in 0..3 {
                for z in 0..2 {
                    sandbox.spawn(
                        vec3(
                            -1.8 + x as f32 * 1.22,
                            0.6 + y as f32 * 1.21,
                            -0.7 + z as f32 * 1.22,
                        ),
                        false,
                    );
                }
            }
        }
        for index in 0..5 {
            sandbox.spawn(
                vec3(-5.0 + index as f32 * 0.9, 7.5 + index as f32 * 0.45, 2.5),
                true,
            );
        }
        sandbox.mechanisms();
        sandbox
    }

    fn spawn(&mut self, position: Vec3, ball: bool) {
        if self.bodies.len() >= MAX_BODIES {
            return;
        }
        let handle = if ball {
            self.physics.add_ball(position.to_array(), 0.58, true)
        } else {
            self.physics.add_box(position.to_array(), [0.58; 3], true)
        };
        let collider = self.physics.bodies[handle].colliders()[0];
        self.physics.colliders[collider].set_restitution(if ball { 0.45 } else { 0.05 });
        self.physics.bodies[handle].enable_ccd(true);
        self.bodies.push(Body {
            handle,
            ball,
            compound: false,
            previous_position: position,
            previous_rotation: Quat::IDENTITY,
        });
    }

    fn compound(&mut self, position: Vec3) -> RigidBodyHandle {
        let handle = self
            .physics
            .insert_compound(
                RigidBodyBuilder::dynamic()
                    .translation(position)
                    .ccd_enabled(true),
                [
                    ColliderBuilder::cuboid(0.8, 0.13, 0.13),
                    ColliderBuilder::ball(0.36).translation(-Vec3::X * 0.8),
                    ColliderBuilder::ball(0.36).translation(Vec3::X * 0.8),
                ],
            )
            .unwrap();
        self.bodies.push(Body {
            handle,
            ball: false,
            compound: true,
            previous_position: position,
            previous_rotation: Quat::IDENTITY,
        });
        handle
    }
    fn mechanisms(&mut self) {
        let hinge_anchor = self
            .physics
            .insert_body(RigidBodyBuilder::fixed().translation(vec3(6., 6., -4.)))
            .unwrap();
        let door = self.compound(vec3(6., 5., -4.));
        self.links.push(
            self.physics
                .insert_joint(
                    hinge_anchor,
                    door,
                    JointBuilder::hinge()
                        .local_anchor2(Vec3::Y)
                        .limits(-1.2, 1.2)
                        .motor(JointMotor::position(0.9, 8., 2., 20.)),
                )
                .unwrap(),
        );
        self.spawn(vec3(6., 3.7, -4.), false);
        let pendant = self.bodies.last().unwrap().handle;
        self.links.push(
            self.physics
                .insert_joint(
                    door,
                    pendant,
                    JointBuilder::fixed()
                        .local_anchor1(vec3(0., -0.6, 0.))
                        .local_anchor2(vec3(0., 0.7, 0.)),
                )
                .unwrap(),
        );
        let a = self
            .physics
            .insert_body(RigidBodyBuilder::fixed().translation(vec3(-6., 7., -4.)))
            .unwrap();
        self.spawn(vec3(-4.5, 5., -4.), true);
        let bob = self.bodies.last().unwrap().handle;
        self.bodies.last_mut().unwrap().previous_position = vec3(-4.5, 5., -4.);
        self.links.push(
            self.physics
                .insert_joint(a, bob, JointBuilder::distance(2.5))
                .unwrap(),
        );
        let a = self
            .physics
            .insert_body(RigidBodyBuilder::fixed().translation(vec3(5., 7., 4.)))
            .unwrap();
        let bob = self.compound(vec3(5., 4.6, 4.));
        self.links.push(
            self.physics
                .insert_joint(a, bob, JointBuilder::spring(2., 25., 5.))
                .unwrap(),
        );
    }

    fn drop_body(&mut self, ball: bool) {
        self.sequence = self.sequence.wrapping_add(1);
        let phase = self.sequence as f32 * 2.399_963;
        self.spawn(vec3(phase.cos() * 2.3, 10.0, phase.sin() * 2.3), ball);
    }

    fn step(&mut self, dt: f32) {
        for body in &mut self.bodies {
            let rigid_body = &self.physics.bodies[body.handle];
            body.previous_position = Vec3::from_array(rigid_body.translation().to_array());
            body.previous_rotation = Quat::from_array(rigid_body.rotation().to_array());
        }
        self.physics.step(dt);
    }
}

fn window_conf() -> Conf {
    ferravex_render::window_conf("Ferravex / low-poly physics laboratory")
}

ferravex_render::game!(window_conf, game);
async fn game() {
    let mut demo = ferravex_render::demo::Demo::new();
    let mut sandbox = Sandbox::new();
    let mut clock = FixedClock::new(120.0, 8);
    let mut stats = FrameStats::<90>::default();
    let material = LitMaterial::new()
        .expect("The built-in Ferravex material must compile on the selected GPU backend");
    let crate_mesh = Mesh3d::cuboid(vec3(1.16, 1.16, 1.16), CRATE);
    let ball_mesh = Mesh3d::sphere(0.58, PAINT, 10, 6);
    let compound_bar = Mesh3d::cuboid(vec3(1.6, 0.26, 0.26), CONCRETE);
    let compound_end = Mesh3d::sphere(0.36, ui::AMBER, 10, 6);
    let floor_mesh = Mesh3d::cuboid(vec3(20.0, 0.8, 16.0), Color::from_rgba(71, 81, 86, 255));
    let curb_x = Mesh3d::cuboid(vec3(20.5, 0.5, 0.5), CONCRETE);
    let curb_z = Mesh3d::cuboid(vec3(0.5, 0.5, 16.0), CONCRETE);
    let floor_mark = Mesh3d::cuboid(
        vec3(0.12, 0.012, 14.0),
        Color::from_rgba(152, 146, 117, 255),
    );
    let cross_mark = Mesh3d::cuboid(
        vec3(18.0, 0.012, 0.12),
        Color::from_rgba(152, 146, 117, 255),
    );
    let pedestal = Mesh3d::cuboid(vec3(0.55, 0.8, 0.55), Color::from_rgba(82, 92, 98, 255));
    let marker = Mesh3d::cuboid(vec3(0.58, 0.13, 0.58), Color::from_rgba(208, 163, 74, 255));
    let mut paused = false;
    let mut yaw: f32 = 0.68;
    let mut pitch: f32 = 0.48;
    let mut radius: f32 = 25.0;
    let mut previous_mouse = Vec2::from(mouse_position());
    let mut camera = Rig3::new(Settings3 {
        position_half_life: 0.045,
        target_half_life: 0.,
        ..Default::default()
    })
    .unwrap();
    loop {
        ui::warm_up_font();
        stats.record(get_frame_time());
        let pointer = Vec2::from(mouse_position());
        let footer_y = screen_height() - 98.0;
        let in_scene = pointer.y > 126.0 && pointer.y < footer_y;
        if is_mouse_button_down(MouseButton::Left)
            && !is_mouse_button_pressed(MouseButton::Left)
            && in_scene
        {
            let delta = pointer - previous_mouse;
            yaw -= delta.x * 0.006;
            pitch = (pitch + delta.y * 0.004).clamp(0.12, 1.35);
        }
        previous_mouse = pointer;
        if in_scene {
            radius = (radius - mouse_wheel().1 * 0.8).clamp(12.0, 42.0);
        }
        let rotate_speed = get_frame_time() * 1.2;
        if is_key_down(KeyCode::Left) {
            yaw -= rotate_speed;
        }
        if is_key_down(KeyCode::Right) {
            yaw += rotate_speed;
        }
        if is_key_down(KeyCode::Up) {
            pitch = (pitch + rotate_speed).min(1.35);
        }
        if is_key_down(KeyCode::Down) {
            pitch = (pitch - rotate_speed).max(0.12);
        }
        if is_key_pressed(KeyCode::Space) {
            paused = !paused;
        }
        if is_key_pressed(KeyCode::R) {
            sandbox = Sandbox::new();
            clock.reset();
        }
        if is_key_pressed(KeyCode::Key1) {
            sandbox.drop_body(false);
        }
        if is_key_pressed(KeyCode::Key2) {
            sandbox.drop_body(true);
        }
        let frame = clock.advance(if paused { 0.0 } else { get_frame_time() as f64 });
        for _ in 0..frame.steps {
            sandbox.step(frame.dt);
        }
        let alpha = if paused { 1.0 } else { frame.alpha };
        clear_background(ui::BACKGROUND);
        let view = camera
            .update(
                demo.dt(),
                Input3 {
                    target: Target3::at(vec3(0., 2.2, 0.)),
                    mode: Mode3::Orbit {
                        yaw,
                        pitch,
                        distance: radius,
                        pivot: Vec3::ZERO,
                    },
                    fov_y: 43_f32.to_radians(),
                },
                &NoObstructions,
            )
            .expect("valid laboratory orbit")
            .pose;
        let eye = view.position;
        let target = view.target;
        set_camera(&Camera3D {
            position: eye,
            target,
            up: view.up,
            fovy: view.fov_y,
            z_near: 0.1,
            z_far: 100.0,
            ..Default::default()
        });
        material.begin(LitSettings {
            camera_position: eye,
            fog_start: 22.0,
            fog_end: 65.0,
            ambient: 0.48,
            ..Default::default()
        });
        floor_mesh.draw(vec3(0.0, -0.4, 0.0), Quat::IDENTITY);
        for z in [-8.0, 8.0] {
            curb_x.draw(vec3(0.0, 0.25, z), Quat::IDENTITY);
        }
        for x in [-10.0, 10.0] {
            curb_z.draw(vec3(x, 0.25, 0.0), Quat::IDENTITY);
        }
        for x in [-5.8, 5.8] {
            floor_mark.draw(vec3(x, 0.008, 0.0), Quat::IDENTITY);
        }
        for z in [-4.5, 4.5] {
            cross_mark.draw(vec3(0.0, 0.008, z), Quat::IDENTITY);
        }
        for x in [-9.4, 9.4] {
            for z in [-7.4, 7.4] {
                pedestal.draw(vec3(x, 0.4, z), Quat::IDENTITY);
                marker.draw(vec3(x, 0.65, z), Quat::IDENTITY);
            }
        }
        let interpolated = |body: &Body| {
            let rigid = &sandbox.physics.bodies[body.handle];
            (
                body.previous_position.lerp(rigid.translation(), alpha),
                body.previous_rotation.slerp(*rigid.rotation(), alpha),
            )
        };
        // Group opaque parts by resident mesh so assemblies retain instancing.
        for body in sandbox.bodies.iter().filter(|b| !b.compound && !b.ball) {
            let (p, r) = interpolated(body);
            crate_mesh.draw(p, r);
        }
        for body in sandbox.bodies.iter().filter(|b| b.ball) {
            let (p, r) = interpolated(body);
            ball_mesh.draw(p, r);
        }
        for body in sandbox.bodies.iter().filter(|b| b.compound) {
            let (p, r) = interpolated(body);
            compound_bar.draw(p, r);
        }
        for body in sandbox.bodies.iter().filter(|b| b.compound) {
            let (p, r) = interpolated(body);
            for x in [-0.8, 0.8] {
                compound_end.draw(p + r * vec3(x, 0., 0.), r);
            }
        }
        for id in &sandbox.links {
            if let Some([a, _]) = sandbox.physics.joint_anchors(*id) {
                compound_end.draw(a, Quat::IDENTITY);
            }
        }
        material.end();
        // Wire detail belongs only to crates: it reads as a shipping frame and
        // keeps geometry simple rather than requiring a texture atlas.
        for body in &sandbox.bodies {
            if body.ball || body.compound {
                continue;
            }
            let rigid_body = &sandbox.physics.bodies[body.handle];
            let position = body
                .previous_position
                .lerp(Vec3::from_array(rigid_body.translation().to_array()), alpha);
            let rotation = body
                .previous_rotation
                .slerp(Quat::from_array(rigid_body.rotation().to_array()), alpha);
            let points = [
                vec3(-0.582, -0.582, -0.582),
                vec3(0.582, -0.582, -0.582),
                vec3(0.582, 0.582, -0.582),
                vec3(-0.582, 0.582, -0.582),
                vec3(-0.582, -0.582, 0.582),
                vec3(0.582, -0.582, 0.582),
                vec3(0.582, 0.582, 0.582),
                vec3(-0.582, 0.582, 0.582),
            ]
            .map(|v| position + rotation * v);
            for (a, b) in [
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 0),
                (4, 5),
                (5, 6),
                (6, 7),
                (7, 4),
                (0, 4),
                (1, 5),
                (2, 6),
                (3, 7),
            ] {
                draw_line_3d(points[a], points[b], Color::from_rgba(84, 64, 43, 255));
            }
        }
        for id in &sandbox.links {
            if let Some([a, b]) = sandbox.physics.joint_anchors(*id) {
                draw_line_3d(a, b, ui::CYAN);
            }
        }
        set_default_camera();
        ui::header(
            "3D RIGID BODIES",
            "The loading yard.",
            if screen_width() > 660.0 {
                "Drag or use arrow keys to orbit  /  Scroll: zoom  /  1, 2: drop bodies  /  Space: pause"
            } else {
                "Drag to orbit. Drop bodies below."
            },
        );
        ui::panel(Rect::new(20.0, footer_y, screen_width() - 40.0, 78.0));
        let button_width = ((screen_width() - 92.0) / 4.0).clamp(38.0, 90.0);
        let button_rect = |index: f32| {
            Rect::new(
                34.0 + index * (button_width + 8.0),
                footer_y + 20.0,
                button_width,
                36.0,
            )
        };
        if ui::button(button_rect(0.0), "+ CRATE", false) {
            sandbox.drop_body(false);
        }
        if ui::button(button_rect(1.0), "+ BALL", false) {
            sandbox.drop_body(true);
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
        if screen_width() > 760.0 {
            let awake = sandbox
                .bodies
                .iter()
                .filter(|b| !sandbox.physics.bodies[b.handle].is_sleeping())
                .count();
            ui::stat(
                450.0,
                footer_y + 25.0,
                "DYNAMIC BODIES",
                &format!("{} / {MAX_BODIES}", sandbox.bodies.len()),
            );
            ui::stat(625.0, footer_y + 25.0, "AWAKE", &awake.to_string());
        }
        if screen_width() > 940.0 {
            ui::stat(
                765.0,
                footer_y + 25.0,
                "FRAME AVERAGE",
                &format!("{:.1} ms", stats.mean_ms()),
            );
        }
        if screen_width() > 1100.0 {
            ui::stat(955.0, footer_y + 25.0, "SHADING", "Flat + fog");
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
    fn recovered_sandbox_settles_without_floor_tunneling() {
        let mut sandbox = Sandbox::new();
        for _ in 0..960 {
            sandbox.step(1.0 / 120.0);
            for body in &sandbox.bodies {
                let rigid = &sandbox.physics.bodies[body.handle];
                assert!(rigid.translation().is_finite());
                assert!(rigid.linvel().is_finite());
                assert!(rigid.rotation().is_finite());
                assert!(rigid.translation().y > 0.3);
            }
        }
        assert!(
            sandbox
                .bodies
                .iter()
                .any(|body| sandbox.physics.bodies[body.handle].is_sleeping())
        );
    }

    #[test]
    fn dropping_is_bounded_and_rebuild_replaces_the_world() {
        let mut sandbox = Sandbox::new();
        for i in 0..MAX_BODIES + 10 {
            sandbox.drop_body(i % 2 == 0);
        }
        assert_eq!(sandbox.bodies.len(), MAX_BODIES);
        assert_eq!(sandbox.physics.bodies.len(), MAX_BODIES + 12);
        sandbox = Sandbox::new();
        assert_eq!(sandbox.bodies.len(), 39);
        assert_eq!(sandbox.physics.bodies.len(), 51);
    }
}
