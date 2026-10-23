//! Flat-shaded 3D rigid bodies, interpolated fixed-tick physics, and an orbit camera.
//!
//! The lab starts playing immediately. Its host uses the shared session clock;
//! a paused manual step advances physics by exactly `session.dt()` without
//! resuming the session or consuming any wall time:
//! ```
//! use ferravex_core::session::GameSession;
//! use ferravex_physics::d3::PhysicsWorld;
//! let mut session = GameSession::new(120.0, 8, true);
//! let mut physics = PhysicsWorld::new([0.0, -9.81, 0.0]);
//! let frame = session.advance(1.0 / 60.0);
//! for _ in 0..frame.steps { physics.step(frame.dt); }
//! session.set_paused(true);
//! assert_eq!(session.advance(10.0).steps, 0);
//! for dt in session.step_paused().unwrap().ticks() { physics.step(dt); }
//! assert_eq!(session.tick(), 3); // Two automatic ticks and one manual tick.
//! assert!(session.is_paused());
//! ```
use ferravex_core::session::GameSession;
use ferravex_physics::d3::{
    ColliderBuilder, JointBuilder, PhysicsWorld, QueryOptions, QueryReport, RayHit,
    RigidBodyBuilder, RigidBodyHandle,
};
use ferravex_physics::{ContactEvent, JointHandle, JointMotor};
use ferravex_play::camera::{Input3, Mode3, NoObstructions, Rig3, Settings3, Target3};
use ferravex_play::feedback::FeedbackPolicy;
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{
    ActionKind, Binding, InputCode, PlayerInput, Preset,
    touch::{Control, Output, Region},
};
use ferravex_render::{
    mesh::Mesh3d,
    preferences,
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
    terrain: Vec<(Mesh3d, ferravex_play::environment::BoxPiece)>,
    bodies: Vec<Body>,
    sequence: u32,
    links: Vec<JointHandle>,
    impact: Option<(f32, Surface)>,
    pulse_time: f32,
    pulse_center: Vec3,
}

impl Sandbox {
    fn new() -> Self {
        let mut sandbox = Self {
            physics: PhysicsWorld::new([0.0, -9.81, 0.0]),
            terrain: Vec::new(),
            bodies: Vec::new(),
            sequence: 0,
            links: Vec::new(),
            impact: None,
            pulse_time: 0.,
            pulse_center: Vec3::ZERO,
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
        let mut plan = ferravex_play::environment::EnvironmentPlan::new(16).unwrap();
        plan.stairs(vec3(5., 0., -5.), 2., 0.2, 0.6, 6, 0).unwrap();
        plan.ramp(vec3(-7., 0., -5.), 2., 1.2, 3.6, 0.2, 1).unwrap();
        for &part in plan.pieces() {
            sandbox.physics.insert(
                RigidBodyBuilder::fixed().translation(part.center),
                ColliderBuilder::cuboid(
                    part.half_extents.x,
                    part.half_extents.y,
                    part.half_extents.z,
                )
                .rotation(part.rotation),
            );
            sandbox.terrain.push((
                Mesh3d::cuboid(
                    part.half_extents * 2.,
                    if part.material == 0 { CONCRETE } else { PAINT },
                ),
                part,
            ));
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

    fn drop_body(&mut self, ball: bool) -> bool {
        if self.bodies.len() >= MAX_BODIES {
            return false;
        }
        self.sequence = self.sequence.wrapping_add(1);
        let phase = self.sequence as f32 * 2.399_963;
        self.spawn(vec3(phase.cos() * 2.3, 10.0, phase.sin() * 2.3), ball);
        true
    }

    fn pulse(&mut self, center: Vec3) -> bool {
        if self.pulse_time > 0. {
            return false;
        }
        self.pulse_time = 0.65;
        self.pulse_center = center;
        for body in &self.bodies {
            let rigid = &mut self.physics.bodies[body.handle];
            let position = rigid.translation();
            if let Some(mut kick) =
                ferravex_play::motion::radial_velocity(center, position, 8., 11.)
            {
                kick.y = kick.y.max(0.) + kick.length() * 0.55;
                rigid.apply_impulse(kick * rigid.mass(), true);
            }
        }
        true
    }

    fn step(&mut self, dt: f32) {
        self.pulse_time = (self.pulse_time - dt).max(0.);
        let mut velocities = [Vec3::ZERO; MAX_BODIES];
        for (i, body) in self.bodies.iter_mut().enumerate() {
            velocities[i] = Vec3::from_array(self.physics.bodies[body.handle].linvel().to_array());
            let rigid_body = &self.physics.bodies[body.handle];
            body.previous_position = Vec3::from_array(rigid_body.translation().to_array());
            body.previous_rotation = Quat::from_array(rigid_body.rotation().to_array());
        }
        self.physics.step(dt);
        self.impact = None;
        // A constraint/motor velocity change alone is not an impact. Only
        // accepted solid contact starts can produce a sound, including CCD hits.
        for event in self.physics.contact_events() {
            let &ContactEvent::Started(a, b) = event else {
                continue;
            };
            let Some(pair) = self.physics.contact_pair(a, b).filter(|p| !p.sensor) else {
                continue;
            };
            let index = |collider| {
                let parent = self.physics.colliders[collider].parent();
                self.bodies
                    .iter()
                    .position(|body| Some(body.handle) == parent)
            };
            let (a, b) = (index(a), index(b));
            let va = a.map_or(Vec3::ZERO, |i| velocities[i]);
            let vb = b.map_or(Vec3::ZERO, |i| velocities[i]);
            let strength = (va - vb).dot(Vec3::from_array(pair.normal)).abs();
            if strength <= 1.2 || self.impact.is_some_and(|(old, _)| old >= strength) {
                continue;
            }
            let Some(body) = a.or(b).map(|i| &self.bodies[i]) else {
                continue;
            };
            self.impact = Some((
                strength,
                if body.compound {
                    Surface::Metal
                } else if body.ball {
                    Surface::Stone
                } else {
                    Surface::Wood
                },
            ));
        }
    }

    fn snap_interpolation(&mut self) {
        for body in &mut self.bodies {
            let rigid = &self.physics.bodies[body.handle];
            body.previous_position = rigid.translation();
            body.previous_rotation = *rigid.rotation();
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DropShape {
    #[default]
    Crate,
    Ball,
}

struct Orbit {
    yaw: f32,
    pitch: f32,
    radius: f32,
    rig: Rig3,
}

impl Default for Orbit {
    fn default() -> Self {
        Self {
            yaw: 0.68,
            pitch: 0.48,
            radius: 25.,
            rig: Rig3::new(Settings3 {
                position_half_life: 0.045,
                target_half_life: 0.,
                ..Default::default()
            })
            .unwrap(),
        }
    }
}

impl Orbit {
    fn input(&mut self, dt: f32, look: Vec2, movement: Vec2, zoom: f32) {
        if !look.is_finite() || !movement.is_finite() || !zoom.is_finite() {
            return;
        }
        let dt = presentation_dt(dt);
        let tau = std::f32::consts::TAU;
        self.yaw = (self.yaw - look.x.clamp(-tau, tau) + movement.x.clamp(-1., 1.) * dt * 1.2)
            .rem_euclid(tau);
        self.pitch = (self.pitch + look.y.clamp(-tau, tau) + movement.y.clamp(-1., 1.) * dt * 1.2)
            .clamp(0.12, 1.35);
        self.radius = (self.radius - zoom).clamp(12., 42.);
    }

    fn reset(&mut self) {
        self.yaw = 0.68;
        self.pitch = 0.48;
        self.radius = 25.;
        self.rig.cut();
    }

    fn view(&mut self, dt: f32) -> ferravex_play::camera::Pose3 {
        self.rig
            .update(
                presentation_dt(dt),
                Input3 {
                    target: Target3::at(vec3(0., 2.2, 0.)),
                    mode: Mode3::Orbit {
                        yaw: self.yaw,
                        pitch: self.pitch,
                        distance: self.radius,
                        pivot: Vec3::ZERO,
                    },
                    fov_y: 43_f32.to_radians(),
                },
                &NoObstructions,
            )
            .expect("valid laboratory orbit")
            .pose
    }
}

// Inspector edits and lifecycle controls are render-frame commands so drops
// remain available while paused. Pulses are queued until an actual running tick.
#[derive(Default)]
struct Commands {
    crate_drop: bool,
    ball_drop: bool,
    drop: bool,
    pulse: bool,
    pause: bool,
    step: bool,
    view: bool,
    reset: bool,
}

struct LabFrame {
    alpha: f32,
    pulse: bool,
    impact: Option<(f32, Surface)>,
    reset: bool,
}

struct Lab {
    sandbox: Sandbox,
    session: GameSession,
    orbit: Orbit,
    shape: DropShape,
    pending_pulse: bool,
    dropped_seconds: f64,
    impact_cooldown: f32,
    feedback: &'static str,
    feedback_time: f32,
}

impl Lab {
    fn new() -> Self {
        Self {
            sandbox: Sandbox::new(),
            session: GameSession::try_new(120., 8, true).expect("valid laboratory clock"),
            orbit: Orbit::default(),
            shape: DropShape::Crate,
            pending_pulse: false,
            dropped_seconds: 0.,
            impact_cooldown: 0.,
            feedback: "",
            feedback_time: 0.,
        }
    }

    fn notice(&mut self, text: &'static str) {
        self.feedback = text;
        self.feedback_time = 2.5;
    }

    fn reset(&mut self) {
        self.sandbox = Sandbox::new();
        self.session.restart();
        self.orbit.reset();
        self.shape = DropShape::Crate;
        self.pending_pulse = false;
        self.dropped_seconds = 0.;
        self.impact_cooldown = 0.;
        self.notice("Reset complete: yard + view.");
    }

    fn drop(&mut self) {
        let dropped = self.sandbox.drop_body(self.shape == DropShape::Ball);
        self.notice(if !dropped {
            "Body limit reached. RESET yard."
        } else if self.shape == DropShape::Ball {
            "Ball dropped. DROP to repeat."
        } else {
            "Crate dropped. DROP to repeat."
        });
    }

    fn step(&mut self, dt: f32, frame: &mut LabFrame) {
        self.sandbox.step(dt);
        self.impact_cooldown = (self.impact_cooldown - dt).max(0.);
        if self.impact_cooldown == 0. && self.sandbox.impact.is_some() {
            frame.impact = self.sandbox.impact;
            self.impact_cooldown = 0.055;
        }
    }

    #[cfg(test)]
    fn advance(&mut self, elapsed: f64, commands: Commands) -> LabFrame {
        self.advance_with_input(elapsed, commands, || false)
    }

    fn advance_with_input(
        &mut self,
        elapsed: f64,
        commands: Commands,
        mut pulse_input: impl FnMut() -> bool,
    ) -> LabFrame {
        let mut result = LabFrame {
            alpha: 1.,
            pulse: false,
            impact: None,
            reset: false,
        };
        if elapsed.is_finite() && elapsed > 0. {
            self.feedback_time = (self.feedback_time - elapsed as f32).max(0.);
        }
        // Reset wins over all simultaneous input and renders the fresh pose.
        if commands.reset {
            self.reset();
            result.reset = true;
            return result;
        }
        if commands.view {
            self.orbit.reset();
            self.notice("View restored. Yard unchanged.");
        }
        if commands.pause {
            self.session.toggle_pause();
            if self.session.is_paused() {
                self.sandbox.snap_interpolation();
            }
            self.notice(if self.session.is_paused() {
                "Paused. STEP advances 1 tick."
            } else {
                "Resumed at 120 Hz."
            });
        }
        if commands.crate_drop || commands.ball_drop || commands.drop {
            if commands.crate_drop {
                self.shape = DropShape::Crate;
            } else if commands.ball_drop {
                self.shape = DropShape::Ball;
            }
            self.drop();
        }
        if self.session.is_paused() {
            self.pending_pulse = false;
            if pulse_input() | commands.pulse {
                self.notice("Resume to pulse. DROP/STEP work.");
            }
        } else {
            self.pending_pulse |= commands.pulse;
        }
        let frame = self.session.advance(elapsed);
        self.dropped_seconds += frame.dropped_seconds;
        for _ in 0..frame.steps {
            let requested = pulse_input() | std::mem::take(&mut self.pending_pulse);
            if requested && self.sandbox.pulse(vec3(0., 0.5, 0.)) {
                result.pulse = true;
                self.notice("Pulse applied. Recharge 0.65s.");
            } else if requested {
                self.notice("Pulse recharging.");
            }
            self.step(frame.dt, &mut result);
        }
        if commands.step {
            if self.session.is_paused() {
                let step = self.session.step_paused().expect("paused inspector");
                self.step(step.dt, &mut result);
                self.sandbox.snap_interpolation();
                self.notice("Stepped 8.33 ms; still paused.");
            } else {
                self.notice("PAUSE before stepping.");
            }
        }
        result.alpha = if self.session.is_paused() {
            1.
        } else {
            frame.alpha
        };
        result
    }
}

fn presentation_dt(dt: f32) -> f32 {
    if dt.is_finite() {
        dt.clamp(0., 0.1)
    } else {
        0.
    }
}

// Presentation owns the pulse ring's lifetime, independently of its simulation
// cooldown. Clearing it under either visual preference is permanent for that
// pulse, including when the world is paused or Controls has focus.
#[derive(Default)]
struct Presentation {
    policy: FeedbackPolicy,
    pulse: Option<(Vec3, f32)>,
}

impl Presentation {
    fn begin_frame(&mut self, dt: f32) {
        self.policy = preferences::feedback_policy();
        if !self.pulse_allowed() {
            self.pulse = None;
        } else if let Some((_, remaining)) = &mut self.pulse {
            *remaining -= presentation_dt(dt);
            if *remaining <= 0. {
                self.pulse = None;
            }
        }
    }

    fn pulse_allowed(&self) -> bool {
        !self.policy.reduced_motion()
            && !self.policy.reduced_flash()
            && self.policy.intensity() > 0.
    }

    fn accept(&mut self, frame: &LabFrame, center: Vec3) {
        if frame.reset {
            self.pulse = None;
        } else if frame.pulse && self.pulse_allowed() {
            self.pulse = Some((center, 0.65));
        }
    }
}

// One current-pose camera-center ray per presentation frame. Collider hits
// (including individual compound children) are distinct from body counts.
struct Inspector {
    hits: [Option<RayHit>; 8],
    query: QueryReport,
}

impl Inspector {
    fn new() -> Self {
        Self {
            hits: [None; 8],
            query: QueryReport::default(),
        }
    }

    fn sample(&mut self, physics: &PhysicsWorld, eye: Vec3, target: Vec3) {
        self.query = physics
            .raycast_into(
                eye.to_array(),
                (target - eye).to_array(),
                QueryOptions::new(60.),
                &mut self.hits,
            )
            .expect("finite laboratory camera ray");
    }
}

#[cfg(test)]
fn binding_hint<'a>(input: &PlayerInput, name: &str, buffer: &'a mut [u8]) -> &'a str {
    input.format_hint(name, buffer).unwrap_or("Controls")
}

fn footer_layout(width: f32, height: f32) -> (Rect, [Rect; 8]) {
    let columns: usize = if width >= 880. {
        8
    } else if width < 360. {
        3
    } else {
        4
    };
    let rows = 8_usize.div_ceil(columns);
    let text_height = 48.;
    let panel = Rect::new(
        12.,
        height - (text_height + 12. + rows as f32 * 44.),
        width - 24.,
        text_height + rows as f32 * 44.,
    );
    let button_width = (panel.w - 24. - (columns - 1) as f32 * 6.) / columns as f32;
    let buttons = std::array::from_fn(|i| {
        Rect::new(
            panel.x + 12. + (i % columns) as f32 * (button_width + 6.),
            panel.y + text_height - 4. + (i / columns) as f32 * 44.,
            button_width,
            36.,
        )
    });
    (panel, buttons)
}

// These four inspector fields each have two fixed-size rows on narrow views.
// Borrowed spans keep the existing bounded strings and avoid a second UI system.
fn hud_lines(mut text: &str, width: f32, size: u16) -> impl Iterator<Item = &str> {
    let metrics = get_default_font().try_metrics(size, 1.).expect("HUD font");
    let columns = (width / metrics.glyph_advance()).floor().max(1.) as usize;
    std::iter::from_fn(move || {
        text = text.trim_start();
        if text.is_empty() {
            return None;
        }
        let mut end = text
            .char_indices()
            .nth(columns)
            .map_or(text.len(), |(i, _)| i);
        if end < text.len()
            && !text[end..].starts_with(' ')
            && let Some(space) = text[..end].rfind(' ')
            && space > 0
        {
            end = space;
        }
        let line = &text[..end];
        text = &text[end..];
        Some(line)
    })
}

#[cfg(test)]
fn inspector_hint(text: &str, width: f32, narrow: bool) -> &str {
    let (size, rows) = if narrow { (14, 2) } else { (16, 1) };
    if hud_lines(text, width, size).count() <= rows {
        text
    } else {
        "Bindings: Controls"
    }
}

fn hud_text(text: &str, x: f32, y: f32, width: f32, color: Color) {
    if screen_width() < 540. {
        for (row, line) in hud_lines(text, width, 14).enumerate() {
            draw_text_ex(
                line,
                x,
                y + row as f32 * 20.,
                TextParams {
                    font_size: 14,
                    font_scale: 1.,
                    color,
                },
            );
        }
    } else {
        let metrics = get_default_font().try_metrics(16, 1.).expect("HUD font");
        let text_width = metrics.measure_line(text).expect("bounded HUD label").width;
        draw_text_ex(
            text,
            x,
            y,
            TextParams {
                font_size: 16,
                font_scale: (width / text_width.max(1.)).clamp(0.1, 1.),
                color,
            },
        );
    }
}

// Fit only narrow views into the same scene band used by pointer/touch input.
// Pose, clipping depth, fog and relative orbit/zoom input remain authored.
struct SceneCamera {
    camera: Camera3D,
    scene: Rect,
}

impl Camera for SceneCamera {
    fn matrix(&self, width: f32, height: f32) -> Mat4 {
        if width >= 540. {
            return self.camera.matrix(width, height);
        }
        let aspect = self.scene.w / self.scene.h.max(1.);
        let camera = Camera3D {
            fovy: 2. * ((self.camera.fovy * 0.5).tan() * (1.6 / aspect).max(1.)).atan(),
            ..self.camera
        };
        Mat4::from_translation(vec3(
            self.scene.center().x * 2. / width - 1.,
            1. - self.scene.center().y * 2. / height,
            0.,
        )) * Mat4::from_scale(vec3(self.scene.w / width, self.scene.h / height, 1.))
            * camera.matrix(self.scene.w, self.scene.h)
    }

    fn depth(&self) -> bool {
        self.camera.depth()
    }
    fn position(&self) -> Option<Vec3> {
        Some(self.camera.position)
    }
}

fn scene_rect(width: f32, panel: Rect) -> Rect {
    Rect::new(0., 44., width, (panel.y - 44.).max(1.))
}

fn window_conf() -> Conf {
    ferravex_render::window_conf("Ferravex / low-poly physics laboratory")
}

ferravex_render::game!(window_conf, game);
async fn game() {
    let mut demo = ferravex_render::demo::Demo::new();
    let mut lab = Lab::new();
    let mut presentation = Presentation::default();
    let mut inspector = Inspector::new();
    let mut sound = GameAudio::new();
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
    let mut player_input = PlayerInput::preset("physics-3d", Preset::Lab3);
    player_input.set_mouse_look_button(Some(1)).unwrap();
    for (name, label, key, pad) in [
        ("drop", "Drop selected shape", KeyCode::Enter, 2),
        ("step", "Step once while paused", KeyCode::N, 5),
        ("reset_view", "Reset orbit view", KeyCode::C, 3),
    ] {
        player_input
            .action(
                name,
                label,
                ActionKind::Button,
                vec![
                    Binding::digital(InputCode::Key(key as u16)),
                    Binding::digital(InputCode::PadButton(pad)),
                ],
            )
            .unwrap();
    }
    let mut pending = Commands::default();
    loop {
        let dt = demo.dt();
        // Always consume the shared policy before accepted feedback and camera
        // sampling, even when input or simulation is inactive. This lab owns no
        // FeedbackController, flash overlay, camera shake or particle emitter.
        presentation.begin_frame(dt);
        ui::warm_up_font();
        let (panel, buttons) = footer_layout(screen_width(), screen_height());
        let scene = scene_rect(screen_width(), panel);
        player_input
            .set_touch_layout(&[Control {
                region: Region::Rect(scene),
                output: Output::Look { x: 0, y: 1 },
            }])
            .unwrap();
        player_input.sample(true);
        let pointer = Vec2::from(mouse_position());
        let in_scene = scene.contains(pointer);
        let mut look = player_input.look(presentation_dt(dt));
        if !(in_scene
            || player_input.touch_active()
            || player_input.value("look_rate_x").abs() > 0.001
            || player_input.value("look_rate_y").abs() > 0.001)
        {
            look = Vec2::ZERO;
        }
        let zoom = player_input.take("zoom").value;
        let movement = player_input.movement();
        lab.orbit.input(
            dt,
            look,
            movement,
            if in_scene { zoom * 0.8 } else { 0. }
                + player_input.value("zoom_rate") * presentation_dt(dt) * 4.,
        );
        pending.pause |= player_input.pressed("pause");
        pending.reset |= player_input.pressed("restart");
        pending.crate_drop |= player_input.pressed("select_one");
        pending.ball_drop |= player_input.pressed("select_two");
        pending.drop |= player_input.pressed("drop");
        pending.step |= player_input.pressed("step");
        pending.view |= player_input.pressed("reset_view");
        if !player_input.gameplay_active() {
            pending = Commands::default();
            lab.pending_pulse = false;
        }
        let frame = lab.advance_with_input(
            player_input.simulation_dt(dt) as f64,
            std::mem::take(&mut pending),
            || player_input.take_tick(["pulse"])[0].pressed,
        );
        if frame.reset {
            // Discard a pulse sampled in the same frame as RESET.
            player_input.take("pulse");
        }
        presentation.accept(&frame, lab.sandbox.pulse_center);
        if frame.pulse {
            sound.cue(Cue::Impact(Surface::Metal), 0.5);
        }
        if let Some((strength, surface)) = frame.impact {
            sound.cue(Cue::Impact(surface), (strength * 0.09).clamp(0.15, 0.7));
        }
        let alpha = frame.alpha;
        let sandbox = &lab.sandbox;
        clear_background(ui::BACKGROUND);
        if presentation.policy.reduced_motion() {
            lab.orbit.rig.cut();
        }
        let view = lab.orbit.view(dt);
        let eye = view.position;
        let target = view.target;
        inspector.sample(&sandbox.physics, eye, target);
        set_camera(&SceneCamera {
            camera: Camera3D {
                position: eye,
                target,
                up: view.up,
                fovy: view.fov_y,
                z_near: 0.1,
                z_far: 100.0,
                ..Default::default()
            },
            scene,
        });
        material.begin(LitSettings {
            camera_position: eye,
            fog_start: 22.0,
            fog_end: 65.0,
            ambient: 0.48,
            ..Default::default()
        });
        floor_mesh.draw(vec3(0.0, -0.4, 0.0), Quat::IDENTITY);
        for (mesh, part) in &sandbox.terrain {
            mesh.draw(part.center, part.rotation);
        }
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
        if let Some((center, remaining)) = presentation.pulse {
            let radius = (0.65 - remaining) * 12.;
            for i in 0..32 {
                let a = i as f32 / 32. * std::f32::consts::TAU;
                let b = (i + 1) as f32 / 32. * std::f32::consts::TAU;
                draw_line_3d(
                    center + vec3(a.cos() * radius, 0.02, a.sin() * radius),
                    center + vec3(b.cos() * radius, 0.02, b.sin() * radius),
                    Color {
                        a: presentation.policy.intensity(),
                        ..ui::CYAN
                    },
                );
            }
        }
        set_default_camera();
        draw_text("PHYSICS 3D", 16., 29., 18., ui::INK);
        ui::panel(panel);
        let paused = lab.session.is_paused();
        let state = if paused { "PAUSED" } else { "PLAYING" };
        hud_text(
            &format!("{state} / {} BODIES", sandbox.bodies.len()),
            panel.x + 12.,
            panel.y + 21.,
            panel.w - 24.,
            if paused { ui::AMBER } else { ui::INK },
        );
        let feedback = match lab.feedback {
            "Resume to pulse. DROP/STEP work." => "RESUME TO PULSE",
            "Pulse recharging." => "PULSE RECHARGING",
            "PAUSE before stepping." => "PAUSE TO STEP",
            "Body limit reached. RESET yard." => "BODY LIMIT REACHED",
            _ => "",
        };
        if lab.feedback_time > 0. {
            hud_text(
                feedback,
                panel.x + 12.,
                panel.y + 42.,
                panel.w - 24.,
                ui::CYAN,
            );
        }
        pending.crate_drop = ui::button(buttons[0], "CRATE", lab.shape == DropShape::Crate);
        pending.ball_drop = ui::button(buttons[1], "BALL", lab.shape == DropShape::Ball);
        pending.drop = ui::button(buttons[2], "DROP", false);
        pending.pulse = ui::button_with_options(
            buttons[3],
            "PULSE",
            ui::ButtonOptions {
                enabled: !paused && sandbox.pulse_time == 0.,
                selected: false,
            },
        );
        pending.pause = ui::button(buttons[4], if paused { "RESUME" } else { "PAUSE" }, paused);
        pending.step = ui::button_with_options(
            buttons[5],
            "STEP",
            ui::ButtonOptions {
                enabled: paused,
                selected: false,
            },
        );
        pending.view = ui::button(buttons[6], "VIEW", false);
        pending.reset = ui::button(buttons[7], "RESET", false);
        if !player_input.gameplay_active() {
            pending = Commands::default();
        }
        player_input.draw_settings();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod journey200_198;
#[cfg(test)]
mod journey200_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_waits_for_one_tick_and_reset_discards_pending_requests() {
        let mut lab = Lab::new();
        let before = poses(&lab);
        let mut edge = true;
        let frame = lab.advance_with_input(0., Commands::default(), || std::mem::take(&mut edge));
        assert!(edge, "zero-tick frames must not consume the gameplay edge");
        assert!(!frame.pulse);
        assert_eq!(poses(&lab), before);
        let frame =
            lab.advance_with_input(8. / 120., Commands::default(), || std::mem::take(&mut edge));
        assert!(frame.pulse);
        assert!(!edge);
        assert_eq!(lab.session.tick(), 8);
        assert!((lab.sandbox.pulse_time - (0.65 - 8. / 120.)).abs() < 1e-6);
        lab.advance(
            0.,
            Commands {
                pulse: true,
                ..Default::default()
            },
        );
        assert!(lab.pending_pulse, "footer input also survives zero ticks");
        let reset = lab.advance(
            0.,
            Commands {
                reset: true,
                ..Default::default()
            },
        );
        assert!(reset.reset);
        assert!(!lab.pending_pulse);
        assert!(!lab.advance(1. / 120., Commands::default()).pulse);
        assert_eq!(lab.sandbox.pulse_time, 0.);
    }

    #[test]
    fn paused_step_preserves_fractional_time_and_live_inspector_counts() {
        let mut lab = Lab::new();
        lab.advance(0.5 / 120., Commands::default());
        lab.advance(
            0.,
            Commands {
                pause: true,
                ..Default::default()
            },
        );
        lab.advance(
            0.,
            Commands {
                drop: true,
                ..Default::default()
            },
        );
        let sleep = lab.sandbox.physics.sleep_stats();
        assert_eq!(sleep.awake_bodies + sleep.sleeping_bodies, 40);
        assert_eq!(lab.sandbox.physics.contacts().count(), 0);
        assert!(lab.sandbox.physics.contact_events().is_empty());
        lab.advance(
            50.,
            Commands {
                step: true,
                ..Default::default()
            },
        );
        assert_eq!(lab.session.tick(), 1);
        assert!(lab.session.is_paused());
        let contacts = lab.sandbox.physics.contacts().count();
        assert!(contacts > 0);
        let before = poses(&lab);
        lab.advance(10., Commands::default());
        assert_eq!(lab.sandbox.physics.contacts().count(), contacts);
        assert_eq!(poses(&lab), before);
        lab.advance(
            0.5 / 120.,
            Commands {
                pause: true,
                ..Default::default()
            },
        );
        assert_eq!(lab.session.tick(), 2);
        assert_eq!(lab.dropped_seconds, 0.);
    }

    #[test]
    fn resume_before_next_tick_keeps_the_inspected_pose() {
        for manual_step in [false, true] {
            let mut lab = Lab::new();
            lab.advance(2.25 / 120., Commands::default());
            lab.advance(
                0.,
                Commands {
                    pause: true,
                    ..Default::default()
                },
            );
            if manual_step {
                lab.advance(
                    0.,
                    Commands {
                        step: true,
                        ..Default::default()
                    },
                );
            }
            let before = poses(&lab);
            let frame = lab.advance(
                0.,
                Commands {
                    pause: true,
                    ..Default::default()
                },
            );
            assert_eq!(poses(&lab), before);
            for body in &lab.sandbox.bodies {
                let rigid = &lab.sandbox.physics.bodies[body.handle];
                assert_eq!(body.previous_position, rigid.translation());
                assert_eq!(body.previous_rotation, *rigid.rotation());
                assert!(
                    body.previous_position
                        .lerp(rigid.translation(), frame.alpha)
                        .distance(rigid.translation())
                        < 1e-6
                );
            }
        }
    }

    #[test]
    fn inspector_ray_reports_truncation_current_edits_and_misses_without_ticks() {
        let mut physics = PhysicsWorld::new([0.; 3]);
        let handles: Vec<_> = (1..=10)
            .map(|x| physics.add_ball([x as f32 * 2., 0., 0.], 0.4, true))
            .collect();
        let mut inspector = Inspector::new();
        inspector.sample(&physics, Vec3::ZERO, Vec3::X);
        assert_eq!((inspector.query.written, inspector.query.total), (8, 10));
        assert!(inspector.query.is_truncated());
        assert_eq!(inspector.hits[0].unwrap().body, Some(handles[0]));
        assert!((inspector.hits[0].unwrap().distance - 1.6).abs() < 1e-4);
        for handle in handles {
            assert!(physics.remove_body(handle));
        }
        inspector.sample(&physics, Vec3::ZERO, Vec3::X);
        assert_eq!(inspector.query.total, 0);
        assert!(inspector.hits.iter().all(Option::is_none));
        assert_eq!(
            physics.last_step.substeps, 0,
            "inspection must not step physics"
        );
    }

    #[test]
    fn contact_sounds_exclude_free_fall_and_unrelated_joint_forces() {
        let mut sandbox = Sandbox::new();
        sandbox.physics = PhysicsWorld::new([0., -9.81, 0.]);
        sandbox.bodies.clear();
        sandbox.spawn(vec3(0., 100., 0.), true);
        let body = sandbox.bodies[0].handle;
        sandbox.physics.bodies[body].set_linvel(vec3(0., -10., 0.), true);
        for _ in 0..4 {
            sandbox.step(1. / 120.);
            assert!(sandbox.impact.is_none());
        }
        // A distance constraint causes a large velocity change without contact.
        let anchor = sandbox
            .physics
            .insert_body(RigidBodyBuilder::fixed().translation(vec3(0., 105., 0.)))
            .unwrap();
        sandbox
            .physics
            .insert_joint(anchor, body, JointBuilder::distance(1.))
            .unwrap();
        sandbox.step(1. / 120.);
        assert!(sandbox.physics.contact_events().is_empty());
        assert!(sandbox.impact.is_none());
    }

    #[test]
    fn visual_policy_clears_existing_and_new_pulses_without_changing_physics() {
        let saved = preferences::feedback_policy();
        for (motion, flash) in [(true, false), (false, true), (true, true)] {
            preferences::set_feedback_policy(FeedbackPolicy::default());
            let mut presentation = Presentation::default();
            let mut lab = Lab::new();
            let frame = lab.advance(
                1. / 120.,
                Commands {
                    pulse: true,
                    ..Default::default()
                },
            );
            presentation.begin_frame(0.);
            presentation.accept(&frame, lab.sandbox.pulse_center);
            assert!(presentation.pulse.is_some());
            lab.advance(
                0.,
                Commands {
                    pause: true,
                    ..Default::default()
                },
            );
            let before = poses(&lab);
            let cooldown = lab.sandbox.pulse_time;
            preferences::set_feedback_policy(FeedbackPolicy::new(motion, flash, 1.).unwrap());
            presentation.begin_frame(0.); // Also the Controls / zero-tick path.
            assert!(presentation.pulse.is_none());
            presentation.accept(&frame, lab.sandbox.pulse_center);
            assert!(
                presentation.pulse.is_none(),
                "suppressed accepted events stay hidden"
            );
            preferences::set_feedback_policy(FeedbackPolicy::default());
            presentation.begin_frame(0.);
            assert!(presentation.pulse.is_none(), "old effects cannot revive");
            assert_eq!(poses(&lab), before);
            assert_eq!(lab.sandbox.pulse_time, cooldown);
            assert!(lab.session.is_paused());
            presentation.accept(&frame, lab.sandbox.pulse_center);
            for _ in 0..7 {
                presentation.begin_frame(0.1);
            }
            assert!(
                presentation.pulse.is_none(),
                "cosmetics expire even while physics is paused"
            );
            presentation.accept(&frame, lab.sandbox.pulse_center);
            let reset = lab.advance(
                0.,
                Commands {
                    reset: true,
                    ..Default::default()
                },
            );
            presentation.accept(&reset, lab.sandbox.pulse_center);
            assert!(presentation.pulse.is_none());
        }
        preferences::set_feedback_policy(saved);
    }

    #[test]
    fn camera_inputs_are_finite_bounded_and_rate_independent() {
        let mut thirty = Orbit::default();
        let mut fast = Orbit::default();
        for _ in 0..30 {
            thirty.input(1. / 30., Vec2::ZERO, vec2(1., 0.), 0.);
        }
        for _ in 0..144 {
            fast.input(1. / 144., Vec2::ZERO, vec2(1., 0.), 0.);
        }
        assert!((thirty.yaw - fast.yaw).abs() < 1e-5);
        assert!(thirty.view(f32::NAN).position.is_finite());
        let before = (thirty.yaw, thirty.pitch, thirty.radius);
        thirty.input(0.1, vec2(f32::NAN, 0.), Vec2::ZERO, 0.);
        assert_eq!((thirty.yaw, thirty.pitch, thirty.radius), before);
        thirty.input(0.1, vec2(f32::MAX, f32::MAX), Vec2::ZERO, -f32::MAX);
        assert!((0. ..std::f32::consts::TAU).contains(&thirty.yaw));
        assert_eq!(thirty.pitch, 1.35);
        assert_eq!(thirty.radius, 42.);
        assert!(thirty.view(f32::INFINITY).position.is_finite());
    }

    #[test]
    fn remapped_hints_use_current_binding_and_bounded_fallback() {
        let mut input = PlayerInput::transient("journey200-143-hints");
        input
            .action(
                "step",
                "Step",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(KeyCode::N as u16))],
            )
            .unwrap();
        let mut buffer = [0; 128];
        assert_eq!(binding_hint(&input, "step", &mut buffer), "N");
        input
            .rebind(
                "step",
                vec![Binding::digital(InputCode::Key(KeyCode::J as u16))],
            )
            .unwrap();
        assert_eq!(binding_hint(&input, "step", &mut buffer), "J");
        assert_eq!(binding_hint(&input, "step", &mut []), "Controls");
        assert_eq!(binding_hint(&input, "missing", &mut buffer), "Controls");
    }

    #[test]
    fn player_pulse_tap_survives_zero_tick_frame_and_menu_cancels_it() {
        use ferravex_render::input::{PadState, gamepad_sample, host_frame};
        let mut input = PlayerInput::transient("journey200-143-pulse");
        input
            .action(
                "pulse",
                "Impulse pulse",
                ActionKind::Button,
                vec![Binding::digital(InputCode::PadButton(4))],
            )
            .unwrap();
        let mut pad = PadState {
            connected: true,
            ..Default::default()
        };
        assert!(gamepad_sample(0, pad));
        host_frame(0., 640, 480, || input.sample(true));
        pad.buttons[4] = 1.;
        assert!(gamepad_sample(0, pad));
        pad.buttons[4] = 0.;
        assert!(gamepad_sample(0, pad));
        let mut lab = Lab::new();
        host_frame(0., 640, 480, || {
            input.sample(true);
            let frame = lab.advance_with_input(0., Commands::default(), || {
                input.take_tick(["pulse"])[0].pressed
            });
            assert!(!frame.pulse);
        });
        host_frame(0.05, 640, 480, || {
            input.sample(true);
            let frame = lab.advance_with_input(0.05, Commands::default(), || {
                input.take_tick(["pulse"])[0].pressed
            });
            assert!(frame.pulse);
        });
        assert_eq!(lab.session.tick(), 6);
        assert!((lab.sandbox.pulse_time - 0.6).abs() < 1e-6);
        lab.reset();
        pad.buttons[4] = 1.;
        assert!(gamepad_sample(0, pad));
        pad.buttons[4] = 0.;
        assert!(gamepad_sample(0, pad));
        host_frame(0., 640, 480, || input.sample(true));
        host_frame(0., 640, 480, || input.sample(false));
        host_frame(1. / 120., 640, 480, || {
            input.sample(true);
            let frame = lab.advance_with_input(1. / 120., Commands::default(), || {
                input.take_tick(["pulse"])[0].pressed
            });
            assert!(!frame.pulse, "menu transition discards the pending tap");
        });
        assert!(gamepad_sample(0, PadState::default()));
    }

    fn poses(lab: &Lab) -> Vec<(Vec3, Vec3, Quat)> {
        lab.sandbox
            .bodies
            .iter()
            .map(|body| {
                let rigid = &lab.sandbox.physics.bodies[body.handle];
                (rigid.translation(), rigid.linvel(), *rigid.rotation())
            })
            .collect()
    }

    #[test]
    fn session_autoplays_and_paused_time_never_catches_up() {
        let mut lab = Lab::new();
        assert!(lab.session.is_playing());
        lab.advance(1. / 60., Commands::default());
        assert_eq!(lab.session.tick(), 2);
        lab.advance(
            0.,
            Commands {
                pause: true,
                ..Default::default()
            },
        );
        let paused = poses(&lab);
        let frame = lab.advance(3600., Commands::default());
        assert_eq!(poses(&lab), paused);
        assert_eq!(lab.session.tick(), 2);
        assert_eq!(frame.alpha, 1.);
        assert!(frame.impact.is_none());
        lab.advance(
            1. / 120.,
            Commands {
                pause: true,
                ..Default::default()
            },
        );
        assert_eq!(lab.session.tick(), 3);
    }

    #[test]
    fn manual_step_advances_exactly_one_tick_and_remains_paused() {
        let mut lab = Lab::new();
        lab.advance(
            0.,
            Commands {
                pause: true,
                ball_drop: true,
                ..Default::default()
            },
        );
        let body = lab.sandbox.bodies.last().unwrap().handle;
        assert_eq!(lab.sandbox.physics.bodies[body].translation().y, 10.);
        let frame = lab.advance(
            60.,
            Commands {
                step: true,
                ..Default::default()
            },
        );
        assert!(lab.session.is_paused());
        assert_eq!(lab.session.tick(), 1);
        assert_eq!(frame.alpha, 1.);
        assert!(lab.sandbox.physics.bodies[body].translation().y < 10.);
        assert!((lab.sandbox.physics.bodies[body].linvel().y + 9.81 / 120.).abs() < 0.001);
        let after = poses(&lab);
        lab.advance(60., Commands::default());
        assert_eq!(poses(&lab), after);
        lab.advance(
            0.,
            Commands {
                pause: true,
                step: true,
                ..Default::default()
            },
        );
        assert_eq!(lab.session.tick(), 1, "STEP must not add a running tick");
    }

    #[test]
    fn reset_restores_world_view_clock_and_feedback_even_when_paused() {
        let mut lab = Lab::new();
        let home = lab.orbit.view(1. / 60.);
        lab.advance(
            0.02,
            Commands {
                ball_drop: true,
                pulse: true,
                ..Default::default()
            },
        );
        lab.orbit.yaw = -1.4;
        lab.orbit.pitch = 1.;
        lab.orbit.radius = 40.;
        lab.orbit.view(1. / 60.);
        lab.advance(
            0.,
            Commands {
                pause: true,
                ..Default::default()
            },
        );
        lab.impact_cooldown = 0.05;
        let frame = lab.advance(
            1.,
            Commands {
                reset: true,
                ball_drop: true,
                step: true,
                pulse: true,
                ..Default::default()
            },
        );
        let fresh = Lab::new();
        assert_eq!(poses(&lab), poses(&fresh));
        assert_eq!(lab.shape, DropShape::Crate);
        assert!(lab.session.is_playing());
        assert_eq!(lab.session.tick(), 0);
        assert_eq!(lab.sandbox.sequence, 0);
        assert_eq!(lab.sandbox.pulse_time, 0.);
        assert_eq!(lab.impact_cooldown, 0.);
        assert!(frame.impact.is_none() && !frame.pulse);
        assert_eq!(
            lab.orbit.view(0.),
            home,
            "camera must snap, not ease from the old view"
        );
        assert!(lab.feedback.starts_with("Reset complete"));
        lab.advance(0.5 / 120., Commands::default());
        assert_eq!(lab.session.tick(), 0, "old fractional time must be cleared");
    }

    #[test]
    fn view_reset_does_not_change_paused_physics_or_shape() {
        let mut lab = Lab::new();
        let home = lab.orbit.view(0.);
        lab.advance(
            0.,
            Commands {
                ball_drop: true,
                pause: true,
                ..Default::default()
            },
        );
        let before = poses(&lab);
        lab.orbit.yaw = 2.;
        lab.orbit.radius = 12.;
        lab.orbit.view(0.1);
        lab.advance(
            30.,
            Commands {
                view: true,
                ..Default::default()
            },
        );
        assert_eq!(lab.orbit.view(0.), home);
        assert_eq!(poses(&lab), before);
        assert_eq!(lab.shape, DropShape::Ball);
        assert!(lab.session.is_paused());
    }

    #[test]
    fn drop_selection_is_repeated_once_and_limit_reports_rejection() {
        let mut lab = Lab::new();
        lab.advance(
            0.,
            Commands {
                ball_drop: true,
                drop: true,
                ..Default::default()
            },
        );
        assert_eq!(lab.sandbox.bodies.len(), 40, "combined inputs drop once");
        assert!(lab.sandbox.bodies.last().unwrap().ball);
        lab.advance(
            8. / 120.,
            Commands {
                drop: true,
                ..Default::default()
            },
        );
        assert_eq!(
            lab.sandbox.bodies.len(),
            41,
            "catch-up must not repeat the input edge"
        );
        assert!(lab.sandbox.bodies.last().unwrap().ball);
        lab.advance(
            0.,
            Commands {
                pause: true,
                crate_drop: true,
                ..Default::default()
            },
        );
        assert_eq!(lab.shape, DropShape::Crate);
        assert!(!lab.sandbox.bodies.last().unwrap().ball);
        for _ in 0..MAX_BODIES {
            lab.advance(
                0.,
                Commands {
                    drop: true,
                    ..Default::default()
                },
            );
        }
        assert_eq!(lab.sandbox.bodies.len(), MAX_BODIES);
        let sequence = lab.sandbox.sequence;
        lab.advance(
            0.,
            Commands {
                drop: true,
                ..Default::default()
            },
        );
        assert_eq!(lab.sandbox.sequence, sequence);
        assert!(lab.feedback.starts_with("Body limit reached"));
        let before = poses(&lab);
        let frame = lab.advance(
            0.,
            Commands {
                pulse: true,
                ..Default::default()
            },
        );
        assert!(!frame.pulse);
        assert_eq!(poses(&lab), before);
    }

    #[test]
    fn autoplay_fixed_physics_matches_across_render_rates() {
        let mut sixty = Lab::new();
        let mut thirty = Lab::new();
        for _ in 0..60 {
            sixty.advance(1. / 60., Commands::default());
        }
        for _ in 0..30 {
            thirty.advance(1. / 30., Commands::default());
        }
        assert_eq!(sixty.session.tick(), 120);
        assert_eq!(thirty.session.tick(), 120);
        assert_eq!(poses(&sixty), poses(&thirty));
        let before = sixty.session.tick();
        sixty.advance(100., Commands::default());
        assert_eq!(sixty.session.tick() - before, 8, "catch-up remains bounded");
    }

    #[test]
    fn footer_buttons_fit_narrow_and_wide_viewports_without_overlap() {
        for (width, height) in [(320., 480.), (640., 360.), (1280., 720.)] {
            let (panel, buttons) = footer_layout(width, height);
            assert!(panel.y >= 104.);
            for (i, button) in buttons.iter().enumerate() {
                assert!(button.w >= 50. && button.h >= 36.);
                assert!(button.x >= panel.x && button.x + button.w <= panel.x + panel.w);
                assert!(button.y >= panel.y && button.y + button.h <= panel.y + panel.h);
                for other in &buttons[i + 1..] {
                    assert!(
                        button.x + button.w <= other.x
                            || other.x + other.w <= button.x
                            || button.y + button.h <= other.y
                            || other.y + other.h <= button.y
                    );
                }
            }
        }
    }

    #[test]
    fn procedural_stair_render_and_collision_share_dimensions() {
        let s = Sandbox::new();
        let (mesh, p) = &s.terrain[5];
        assert_eq!(mesh.mesh.vertices.len(), 24);
        let origin = p.center + Vec3::Y * 5.;
        let hit = s
            .physics
            .raycast(origin.to_array(), (-Vec3::Y).to_array(), 10.)
            .unwrap();
        assert!((hit.distance - (5. - p.half_extents.y)).abs() < 0.01);
    }
    #[test]
    fn pulse_wakes_and_lifts_props_and_cannot_be_spammed() {
        let mut sandbox = Sandbox::new();
        assert!(sandbox.pulse(vec3(0., 0.5, 0.)));
        assert!(
            sandbox
                .bodies
                .iter()
                .any(|body| sandbox.physics.bodies[body.handle].linvel().y > 0.1)
        );
        let velocities: Vec<_> = sandbox
            .bodies
            .iter()
            .map(|body| sandbox.physics.bodies[body.handle].linvel())
            .collect();
        assert!(!sandbox.pulse(vec3(0., 0.5, 0.)));
        for (body, velocity) in sandbox.bodies.iter().zip(velocities) {
            assert_eq!(sandbox.physics.bodies[body.handle].linvel(), velocity);
        }
        for _ in 0..80 {
            sandbox.step(1. / 120.);
        }
        assert!(sandbox.pulse(vec3(0., 0.5, 0.)));
    }

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
        assert_eq!(
            sandbox.physics.bodies.len(),
            MAX_BODIES + 12 + sandbox.terrain.len()
        );
        sandbox = Sandbox::new();
        assert_eq!(sandbox.bodies.len(), 39);
        assert_eq!(sandbox.physics.bodies.len(), 51 + sandbox.terrain.len());
    }
}
