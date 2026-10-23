//! Interactive original 2D physics sandbox. No imported textures, fonts, or scene assets.
//!
//! The lab starts running immediately. Space pauses, Enter drops the selected
//! shape, and N advances one 120 Hz tick while paused. Reset preserves the
//! selected shape and pause state, but clears simulation time and cooldowns.
//! I inspects a body under the cursor without waking it. Canvas hints follow
//! remapped controls; reduced motion/flash only affect the cosmetic pulse ring.
//!
//! Launch the native renderer with the existing entry point:
//! ```no_run
//! physics_2d::run();
//! ```
use ferravex_core::session::GameSession;
use ferravex_core::{FrameStats, FrameStep};
use ferravex_physics::d2::{
    self, ColliderBuilder, JointBuilder, PhysicsWorld, QueryOptions, QueryReport, RayHit,
    RigidBodyBuilder, RigidBodyHandle,
};
use ferravex_physics::{ContactEvent, JointHandle};
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{ActionKind, Binding, InputCode, PlayerInput, Preset};
use ferravex_render::{preferences, prelude::*, ui};

mod presentation;

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
    pulse_time: f32,
    pulse_center: Vec2,
}

impl Sandbox {
    fn new() -> Self {
        let mut world = Self {
            physics: PhysicsWorld::new([0.0, -9.81]),
            bodies: Vec::new(),
            platforms: vec![],
            links: Vec::new(),
            impact: None,
            pulse_time: 0.,
            pulse_center: Vec2::ZERO,
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

    fn spawn(&mut self, position: Vec2, ball: bool) -> bool {
        if self.bodies.len() >= MAX_BODIES || !position.is_finite() {
            return false;
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
        true
    }

    fn pulse(&mut self, center: Vec2) -> bool {
        if self.pulse_time > 0. || !center.is_finite() {
            return false;
        }
        self.pulse_time = 0.65;
        self.pulse_center = center;
        for body in &self.bodies {
            let rigid = &mut self.physics.bodies[body.handle];
            let position = rigid.translation();
            if let Some(mut kick) = ferravex_play::motion::radial_velocity(
                center.extend(0.),
                position.extend(0.),
                8.,
                11.,
            ) {
                kick.y = kick.y.max(0.) + kick.length() * 0.55;
                rigid.apply_impulse(kick.truncate() * rigid.mass(), true);
            }
        }
        true
    }

    fn step(&mut self, dt: f32) {
        self.pulse_time = (self.pulse_time - dt).max(0.);
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
            // Joint corrections and gravity alone are not collision feedback.
            // Only an accepted solid contact start can produce an impact cue.
            let started = self.physics.contact_events().iter().any(|event| {
                let ContactEvent::Started(a, b) = *event else {
                    return false;
                };
                self.physics
                    .contact_pair(a, b)
                    .is_some_and(|pair| !pair.sensor)
                    && [a, b]
                        .iter()
                        .any(|id| self.physics.colliders[*id].parent() == Some(body.handle))
            });
            if !started {
                continue;
            }
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

#[derive(Default)]
struct LabInput {
    reset: bool,
    toggle_pause: bool,
    single_step: bool,
    ball: Option<bool>,
    // A secondary click overrides the selected shape for this drop only.
    drop: Option<(Vec2, bool)>,
    pulse: Option<Vec2>,
}

struct Lab {
    sandbox: Sandbox,
    session: GameSession,
    ball: bool,
    impact_cooldown: f32,
    impact: Option<(f32, Surface)>,
    pulsed: bool,
    feedback: &'static str,
    feedback_time: f32,
    pending: LabCommands,
    selected: Option<RigidBodyHandle>,
}

/// At most one pending UI drop and pulse; repeated zero-tick requests coalesce.
#[derive(Default)]
struct LabCommands {
    drop: Option<(Vec2, bool)>,
    pulse: Option<Vec2>,
}

struct Inspection {
    hits: [Option<RayHit>; 8],
    report: QueryReport,
}

impl Sandbox {
    fn inspect(&self, point: Vec2) -> Result<Inspection, d2::QueryError> {
        let mut hits = [None; 8];
        // Zero travel inspects actual collider geometry at the current pose,
        // including compound children. No simulation step or refresh is needed.
        let report = self.physics.spherecast_into(
            point.to_array(),
            [1., 0.],
            0.04,
            QueryOptions::new(0.),
            &mut hits,
        )?;
        Ok(Inspection { hits, report })
    }
}

impl Inspection {
    fn body(&self, sandbox: &Sandbox) -> Option<RigidBodyHandle> {
        self.hits
            .iter()
            .flatten()
            .filter_map(|hit| hit.body)
            .find(|handle| sandbox.bodies.iter().any(|body| body.handle == *handle))
    }
}

/// Cosmetic time is independent of the simulation's pulse cooldown.
#[derive(Default)]
struct PulseVisual {
    ring: Option<(Vec2, f32)>,
    enabled: bool,
    intensity: f32,
}

impl PulseVisual {
    fn begin_frame(&mut self, elapsed: f32) {
        let policy = preferences::feedback_policy();
        self.enabled =
            !policy.reduced_motion() && !policy.reduced_flash() && policy.intensity() > 0.;
        self.intensity = policy.intensity();
        if !self.enabled {
            self.ring = None;
        } else if let Some((_, age)) = &mut self.ring {
            if elapsed.is_finite() {
                *age += elapsed.clamp(0., 0.1);
            }
            if *age >= 0.65 {
                self.ring = None;
            }
        }
    }

    fn accepted_pulse(&mut self, center: Vec2) {
        if self.enabled {
            self.ring = Some((center, 0.));
        }
    }
}

impl Lab {
    fn new() -> Self {
        Self {
            sandbox: Sandbox::new(),
            // A sandbox has no start screen or terminal state.
            session: GameSession::try_new(TICK_HZ, 8, true).expect("valid lab fixed clock"),
            ball: true,
            impact_cooldown: 0.,
            impact: None,
            pulsed: false,
            feedback: "Drop a shape. Pause to inspect or single-step.",
            feedback_time: 5.,
            pending: LabCommands::default(),
            selected: None,
        }
    }

    fn notice(&mut self, message: &'static str) {
        self.feedback = message;
        self.feedback_time = 2.5;
    }

    fn reset(&mut self) {
        let paused = self.session.is_paused();
        self.sandbox = Sandbox::new();
        self.session.restart();
        self.session.set_paused(paused);
        self.impact_cooldown = 0.;
        self.impact = None;
        self.pulsed = false;
        self.pending = LabCommands::default();
        self.selected = None;
        self.notice(if paused {
            "Scene reset. Still paused; STEP advances one tick."
        } else {
            "Scene reset. Original 34 bodies restored."
        });
    }

    fn drop_position(position: Vec2) -> Vec2 {
        vec2(position.x.clamp(-11.2, 11.2), position.y.clamp(0.6, 12.4))
    }

    fn button_enabled(&self, index: usize) -> bool {
        match index {
            2 => self.sandbox.bodies.len() < MAX_BODIES,
            3 => self.session.is_playing() && self.sandbox.pulse_time == 0.,
            5 => self.session.is_paused(),
            _ => true,
        }
    }

    fn select_body(&mut self, inspection: Option<&Inspection>) {
        self.selected = inspection.and_then(|query| query.body(&self.sandbox));
        self.notice(if self.selected.is_some() {
            "Body selected. Outline follows its pose; diagnostics are read-only."
        } else {
            "No dynamic body at cursor. Selection cleared."
        });
    }

    fn selected_label(&self) -> String {
        let Some(handle) = self.selected else {
            return "No body selected / inspect at cursor or drop a shape".into();
        };
        let Some((index, body)) = self
            .sandbox
            .bodies
            .iter()
            .enumerate()
            .find(|(_, body)| body.handle == handle)
        else {
            return "Selected body unavailable".into();
        };
        let Some(sleep) = self.sandbox.physics.sleep_state(handle) else {
            return "Selected body unavailable".into();
        };
        let rigid = &self.sandbox.physics.bodies[handle];
        let contacts = self
            .sandbox
            .physics
            .contacts()
            .filter(|pair| {
                !pair.sensor
                    && [pair.collider1, pair.collider2]
                        .iter()
                        .any(|id| self.sandbox.physics.colliders[*id].parent() == Some(handle))
            })
            .count();
        let shape = match body.shape {
            Shape::Ball(_) => "BALL",
            Shape::Box(_) => "BOX",
            Shape::Dumbbell => "COMPOUND",
        };
        let state = if sleep.sleeping {
            "ASLEEP"
        } else if sleep.wake_requested {
            "WAKE PENDING"
        } else {
            "AWAKE"
        };
        format!(
            "#{} {shape} {state} / quiet {:.2}s / speed {:.2} / {contacts} contacts",
            index + 1,
            sleep.quiet_time,
            rigid.linvel().length()
        )
    }

    #[cfg(test)]
    fn update(&mut self, elapsed_seconds: f64, input: LabInput) -> FrameStep {
        self.update_with_actions(elapsed_seconds, input, LabCommands::default)
    }

    fn update_with_actions(
        &mut self,
        elapsed_seconds: f64,
        input: LabInput,
        mut actions: impl FnMut() -> LabCommands,
    ) -> FrameStep {
        self.impact = None;
        self.pulsed = false;
        if elapsed_seconds.is_finite() {
            self.feedback_time =
                (self.feedback_time - elapsed_seconds.clamp(0., 0.1) as f32).max(0.);
        }
        // Reset wins over every edge from this frame, including pending drops.
        if input.reset {
            self.reset();
            return self.session.advance(0.);
        }
        if input.toggle_pause {
            self.pending = LabCommands::default();
            self.session.toggle_pause();
            self.notice(if self.session.is_paused() {
                "Paused. Drops stay still; STEP advances one tick."
            } else {
                "Simulation resumed."
            });
        }
        if let Some(ball) = input.ball {
            self.ball = ball;
        }
        if self.pending.drop.is_none() {
            self.pending.drop = input.drop;
        }
        if self.pending.pulse.is_none() {
            self.pending.pulse = input.pulse;
        }

        let frame = if input.single_step && self.session.is_paused() {
            let frame = self.session.step_paused().expect("paused lab step");
            self.notice("Advanced one tick (8.33 ms). Still paused.");
            frame
        } else {
            if input.single_step {
                self.notice("Pause before single-stepping.");
            }
            self.session.advance(if input.toggle_pause {
                0.
            } else {
                elapsed_seconds
            })
        };
        // Paused placement is an editor operation, deliberately available
        // without a tick. Running gameplay edges stay in PlayerInput until here.
        if self.session.is_paused() && frame.steps == 0 {
            self.apply_commands(actions());
        }
        for _ in 0..frame.steps {
            self.apply_commands(actions());
            self.impact_cooldown = (self.impact_cooldown - frame.dt).max(0.);
            self.sandbox.step(frame.dt);
            if self.impact_cooldown == 0.
                && let Some(impact) = self.sandbox.impact
            {
                self.impact = Some(impact);
                self.impact_cooldown = 0.055;
            }
        }
        frame
    }

    fn apply_commands(&mut self, actions: LabCommands) {
        let pending = std::mem::take(&mut self.pending);
        if let Some((position, force_crate)) = pending.drop.or(actions.drop) {
            let ball = self.ball && !force_crate;
            if !position.is_finite() {
                self.notice("Drop rejected: position must be finite.");
            } else if self.sandbox.spawn(Self::drop_position(position), ball) {
                self.selected = self.sandbox.bodies.last().map(|body| body.handle);
                self.notice(if self.session.is_paused() {
                    "Shape placed. Paused until STEP or RESUME."
                } else if ball {
                    "Ball dropped."
                } else {
                    "Crate dropped."
                });
            } else {
                self.notice("Body limit reached (240). RESET clears the scene.");
            }
        }
        if let Some(center) = pending.pulse.or(actions.pulse) {
            if self.session.is_paused() {
                self.notice("Resume the simulation to pulse.");
            } else if center.is_finite() && self.sandbox.pulse(center) {
                self.pulsed = true;
                self.notice("Pulse applied.");
            } else if !center.is_finite() {
                self.notice("Pulse rejected: position must be finite.");
            } else {
                self.notice("Pulse recharging.");
            }
        }
    }
}

fn fitted_label(text: &str, x: f32, y: f32, width: f32, color: Color) {
    let measured = get_default_font()
        .try_metrics(16, 1.)
        .and_then(|font| font.measure_line(text))
        .expect("bounded lab text")
        .width;
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font_size: 16,
            font_scale: (width.max(1.) / measured.max(1.)).min(1.),
            color,
        },
    );
}

fn window_conf() -> Conf {
    ferravex_render::window_conf("Ferravex / 2D physics laboratory")
}

fn action_hint(input: &PlayerInput, action: &str) -> String {
    let mut buffer = [0; 512];
    input
        .format_hint(action, &mut buffer)
        .unwrap_or("Controls")
        .into()
}

fn register_lab_actions(input: &mut PlayerInput) {
    for (name, label, key, pad) in [
        ("drop", "Drop selected shape", KeyCode::Enter, 7),
        ("step", "Single step while paused", KeyCode::N, 5),
        ("inspect", "Inspect body at cursor", KeyCode::I, 6),
    ] {
        input
            .action(
                name,
                label,
                ActionKind::Button,
                vec![
                    Binding::digital(InputCode::Key(key as u16)),
                    Binding::digital(InputCode::PadButton(pad)),
                ],
            )
            .expect("valid lab input binding");
    }
}

ferravex_render::game!(window_conf, game);
async fn game() {
    let mut demo = ferravex_render::demo::Demo::new();
    let mut lab = Lab::new();
    let mut sound = GameAudio::new();
    let mut pulse_visual = PulseVisual::default();
    let mut stats = FrameStats::<90>::default();
    let mut player_input = PlayerInput::preset("physics-2d", Preset::Lab2);
    register_lab_actions(&mut player_input);
    loop {
        ui::warm_up_font();
        stats.record(get_frame_time());
        // Editing and manual stepping remain available while the session pauses.
        // Controls/Details and focus still suspend all sandbox input and time.
        player_input.sample(true);
        let active = player_input.gameplay_active();
        let dt = demo.dt();
        // Always reconcile policy before accepted feedback and presentation,
        // including controls/focus suspension, paused and zero-tick frames.
        pulse_visual.begin_frame(dt);
        if !active {
            lab.pending = LabCommands::default();
        }
        let layout = presentation::LabLayout::new(screen_width(), screen_height());
        let narrow = layout.narrow;
        let footer_height = layout.footer_height;
        let footer_y = layout.footer_y;
        let scale = layout.scale;
        let to_screen = |point| layout.to_screen(point);
        let pointer = player_input.cursor(dt, 600.);
        let pointer_in_scene = active && layout.contains_scene(pointer);
        let spawn_position = Lab::drop_position(layout.to_world(pointer));
        // Off-scene primary activates menus. Scene placement remains a fixed-
        // tick action (or a paused editor action) below.
        let primary = !pointer_in_scene && player_input.pressed("primary");
        let mut input = LabInput {
            reset: player_input.pressed("restart"),
            toggle_pause: player_input.pressed("pause"),
            single_step: player_input.pressed("step"),
            ..LabInput::default()
        };
        if player_input.pressed("select_one") {
            input.ball = Some(true);
        }
        if player_input.pressed("select_two") {
            input.ball = Some(false);
        }
        let inspect = player_input.pressed("inspect");

        clear_background(ui::BACKGROUND);
        let button_rect = |i| layout.button(i);
        let mut clicked = [false; 7];
        for (i, click) in clicked.iter_mut().enumerate() {
            let rect = button_rect(i);
            let mouse_clicked = rect.contains(Vec2::from(mouse_position()))
                && is_mouse_button_pressed(MouseButton::Left);
            *click = active
                && lab.button_enabled(i)
                && (mouse_clicked
                    || (primary && player_input.cursor_is_virtual() && rect.contains(pointer)));
        }
        if clicked[0] {
            input.ball = Some(true);
        }
        if clicked[1] {
            input.ball = Some(false);
        }
        if clicked[2] {
            input.drop = Some((vec2(0., 11.5), false));
        }
        if clicked[3] {
            input.pulse = Some(vec2(0., 1.));
        }
        input.toggle_pause |= clicked[4];
        input.single_step |= clicked[5];
        input.reset |= clicked[6];
        let reset = input.reset;
        if input.reset || input.toggle_pause {
            // Lifecycle transitions discard stale gameplay requests.
            player_input.take_tick(["primary", "secondary", "drop", "pulse"]);
        }
        let frame = lab.update_with_actions(player_input.simulation_dt(dt) as f64, input, || {
            if !active {
                return LabCommands::default();
            }
            let [primary, secondary, drop, pulse] =
                player_input.take_tick(["primary", "secondary", "drop", "pulse"]);
            LabCommands {
                drop: (drop.pressed
                    || (pointer_in_scene && (primary.pressed || secondary.pressed)))
                    .then_some((
                        if pointer_in_scene {
                            spawn_position
                        } else {
                            vec2(0., 11.5)
                        },
                        pointer_in_scene && secondary.pressed,
                    )),
                pulse: pulse.pressed.then_some(if pointer_in_scene {
                    spawn_position
                } else {
                    vec2(0., 1.)
                }),
            }
        });
        if reset {
            pulse_visual.ring = None;
            stats.clear();
        }
        if lab.pulsed {
            pulse_visual.accepted_pulse(lab.sandbox.pulse_center);
            sound.cue(Cue::Impact(Surface::Metal), 0.5);
        }
        if let Some((strength, surface)) = lab.impact {
            sound.cue(Cue::Impact(surface), (strength * 0.09).clamp(0.15, 0.7));
        }
        let alpha = if lab.session.is_paused() || !active {
            1.
        } else {
            frame.alpha
        };
        let inspection = pointer_in_scene.then(|| lab.sandbox.inspect(layout.to_world(pointer)));
        if inspect && !reset {
            lab.select_body(inspection.as_ref().and_then(|result| result.as_ref().ok()));
        }
        let sandbox = &lab.sandbox;
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
            if lab.selected == Some(body.handle) {
                let radius = match body.shape {
                    Shape::Ball(radius) => radius,
                    Shape::Box(half) => half.length(),
                    Shape::Dumbbell => 1.03,
                };
                // A steady outline carries selection without flashing or shake.
                draw_circle_lines(center.x, center.y, radius * scale + 4., 2., ui::INK);
            }
        }
        if pointer_in_scene && sandbox.bodies.len() < MAX_BODIES {
            let preview = to_screen(spawn_position);
            if lab.ball {
                draw_circle_lines(preview.x, preview.y, 0.44 * scale, 1.0, ui::CYAN);
            } else {
                draw_rectangle_lines(
                    preview.x - 0.48 * scale,
                    preview.y - 0.48 * scale,
                    0.96 * scale,
                    0.96 * scale,
                    1.0,
                    ui::AMBER,
                );
            }
        }
        if let Some((center, age)) = pulse_visual.ring {
            let point = to_screen(center);
            draw_circle_lines(
                point.x,
                point.y,
                age * 12. * scale,
                2.,
                Color::new(
                    0.4,
                    0.9,
                    0.85,
                    (1. - age / 0.65).max(0.) * pulse_visual.intensity,
                ),
            );
        }
        ferravex_render::set_world_colors(false);
        if narrow {
            presentation::narrow_header(
                [
                    &action_hint(&player_input, "drop"),
                    &action_hint(&player_input, "pause"),
                    &action_hint(&player_input, "inspect"),
                ],
                screen_width(),
            );
        } else {
            let hints = format!(
                "{}: drop / {}: pause / {}: step / {}: pulse / {}: inspect / {}: reset",
                action_hint(&player_input, "drop"),
                action_hint(&player_input, "pause"),
                action_hint(&player_input, "step"),
                action_hint(&player_input, "pulse"),
                action_hint(&player_input, "inspect"),
                action_hint(&player_input, "restart")
            );
            ui::header("2D RIGID BODIES", "Gravity, in two dimensions.", &hints);
        }
        ui::panel(Rect::new(
            16.,
            footer_y,
            screen_width() - 32.,
            footer_height,
        ));
        let labels = [
            "BALL",
            "CRATE",
            "DROP",
            "PULSE",
            if lab.session.is_paused() {
                "RESUME"
            } else {
                "PAUSE"
            },
            "STEP",
            "RESET",
        ];
        for (i, label) in labels.iter().enumerate() {
            let selected = match i {
                0 => lab.ball,
                1 => !lab.ball,
                3 => lab.sandbox.pulse_time > 0.,
                4 => lab.session.is_paused(),
                _ => false,
            };
            ui::button_with_options(
                button_rect(i),
                label,
                ui::ButtonOptions {
                    enabled: active && lab.button_enabled(i),
                    selected,
                },
            );
        }
        let sleep = sandbox.physics.sleep_stats();
        let state = if !active {
            "SUSPENDED"
        } else if lab.session.is_paused() {
            "PAUSED"
        } else {
            "RUNNING"
        };
        let shape = if lab.ball { "BALL" } else { "CRATE" };
        let status_y = footer_y + 72.;
        let label = |text: &str, slot: usize, color: Color| {
            if narrow {
                presentation::wrapped_label(
                    text,
                    28.,
                    presentation::slot_y(footer_y, screen_width(), slot),
                    screen_width() - 56.,
                    color,
                );
            } else {
                fitted_label(
                    text,
                    28.,
                    status_y + slot as f32 * 21.,
                    screen_width() - 56.,
                    color,
                );
            }
        };
        let hint = |action| {
            let hint = action_hint(&player_input, action);
            if narrow {
                presentation::bounded_hint(&hint, 144.).to_owned()
            } else {
                hint
            }
        };
        label(
            &format!(
                "{state} / {shape} / {} of {MAX_BODIES} bodies / {} awake / {} asleep",
                sandbox.bodies.len(),
                sleep.awake_bodies,
                sleep.sleeping_bodies
            ),
            0,
            ui::INK,
        );
        let pulse_status = if sandbox.pulse_time > 0. {
            format!("pulse {:.2}s", sandbox.pulse_time)
        } else {
            "pulse ready".into()
        };
        let settings = if narrow {
            format!("120 Hz / tick {} / {pulse_status}", lab.session.tick())
        } else {
            format!(
                "120 Hz / tick {} / {pulse_status} / {:.1} ms mean / {:.1} ms p95",
                lab.session.tick(),
                stats.mean_ms(),
                stats.percentile_ms(95.).unwrap_or(0.)
            )
        };
        label(&settings, 1, ui::MUTED);
        let idle_hint = if lab.session.is_paused() {
            format!(
                "Drops stay still. {} / STEP advances one tick.",
                hint("step")
            )
        } else {
            format!(
                "{}: place / {}: inspect at cursor",
                hint("primary"),
                hint("inspect")
            )
        };
        label(
            if lab.feedback_time > 0. {
                lab.feedback
            } else {
                &idle_hint
            },
            2,
            ui::AMBER,
        );
        let query_status = match &inspection {
            Some(Ok(query)) => format!(
                "query {} / {} collider hits{}",
                query.report.written,
                query.report.total,
                if query.report.is_truncated() {
                    " (truncated)"
                } else {
                    ""
                }
            ),
            Some(Err(_)) => "query invalid".into(),
            None => "query: move cursor into scene".into(),
        };
        let starts = sandbox
            .physics
            .contact_events()
            .iter()
            .filter(|event| matches!(event, ContactEvent::Started(..)))
            .count();
        let stops = sandbox.physics.contact_events().len() - starts;
        label(
            &format!(
                "Last tick: {} pairs / +{starts} -{stops} / {query_status}",
                sandbox.physics.contacts().count()
            ),
            3,
            ui::MUTED,
        );
        label(&lab.selected_label(), 4, ui::INK);
        if player_input.gameplay_active() && player_input.cursor_is_virtual() {
            draw_circle_lines(pointer.x, pointer.y, 5., 1.3, WHITE);
        }
        player_input.draw_controls();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod journey200_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn poses(sandbox: &Sandbox) -> Vec<(Vec2, Vec2, f32)> {
        sandbox
            .bodies
            .iter()
            .map(|body| {
                let rigid = &sandbox.physics.bodies[body.handle];
                (
                    rigid.translation(),
                    rigid.linvel(),
                    rigid.rotation().angle(),
                )
            })
            .collect()
    }

    #[test]
    fn pause_freezes_poses_and_cooldowns_without_resume_catchup() {
        let mut lab = Lab::new();
        lab.update(
            1. / TICK_HZ,
            LabInput {
                pulse: Some(vec2(0., 1.)),
                ..LabInput::default()
            },
        );
        lab.update(
            0.,
            LabInput {
                toggle_pause: true,
                ..LabInput::default()
            },
        );
        let before = poses(&lab.sandbox);
        let pulse_time = lab.sandbox.pulse_time;
        let impact_cooldown = lab.impact_cooldown;
        let tick = lab.session.tick();
        for _ in 0..3 {
            assert_eq!(lab.update(60., LabInput::default()).steps, 0);
        }
        assert_eq!(poses(&lab.sandbox), before);
        assert_eq!(lab.sandbox.pulse_time, pulse_time);
        assert_eq!(lab.impact_cooldown, impact_cooldown);
        assert_eq!(lab.session.tick(), tick);
        assert_eq!(
            lab.update(
                60.,
                LabInput {
                    toggle_pause: true,
                    ..LabInput::default()
                }
            )
            .steps,
            0
        );
        assert_eq!(lab.update(1. / TICK_HZ, LabInput::default()).steps, 1);
        assert_eq!(lab.session.tick(), tick + 1);
    }

    #[test]
    fn manual_step_advances_exactly_one_tick_and_stays_paused() {
        let mut lab = Lab::new();
        let mut reference = Lab::new();
        // Keep a partial tick to prove that stepping neither loses it nor
        // admits wall time supplied while paused.
        lab.update(0.5 / TICK_HZ, LabInput::default());
        reference.update(0.5 / TICK_HZ, LabInput::default());
        lab.update(
            0.,
            LabInput {
                toggle_pause: true,
                ..LabInput::default()
            },
        );
        for tick in 1..=10 {
            let frame = lab.update(
                20.,
                LabInput {
                    single_step: true,
                    ..LabInput::default()
                },
            );
            reference.update(1. / TICK_HZ, LabInput::default());
            assert_eq!(frame.steps, 1);
            assert_eq!(frame.dt, 1_f32 / 120.);
            assert_eq!(frame.alpha, 1., "manual steps present the current pose");
            assert!(lab.session.is_paused());
            assert_eq!(lab.session.tick(), tick);
            assert_eq!(poses(&lab.sandbox), poses(&reference.sandbox));
            assert_eq!(lab.update(100., LabInput::default()).steps, 0);
        }
        // A step edge while running never adds an extra simulation interval.
        reference.update(
            0.,
            LabInput {
                single_step: true,
                ..LabInput::default()
            },
        );
        assert_eq!(reference.session.tick(), 10);
        lab.session.set_paused(false);
        assert_eq!(lab.update(0.5 / TICK_HZ, LabInput::default()).steps, 1);
        reference.update(0.5 / TICK_HZ, LabInput::default());
        assert_eq!(poses(&lab.sandbox), poses(&reference.sandbox));
    }

    #[test]
    fn reset_preserves_pause_and_shape_but_clears_world_time_and_stale_edges() {
        for paused in [false, true] {
            let mut lab = Lab::new();
            lab.update(
                2.5 / TICK_HZ,
                LabInput {
                    ball: Some(false),
                    drop: Some((vec2(0., 9.), false)),
                    pulse: Some(vec2(0., 1.)),
                    ..LabInput::default()
                },
            );
            lab.session.set_paused(paused);
            lab.impact_cooldown = 0.04;
            let frame = lab.update(
                10.,
                LabInput {
                    reset: true,
                    toggle_pause: true,
                    single_step: true,
                    ball: Some(true),
                    drop: Some((vec2(0., 12.), false)),
                    pulse: Some(vec2(0., 1.)),
                },
            );
            assert_eq!(frame.steps, 0);
            assert_eq!(lab.session.tick(), 0);
            assert_eq!(lab.session.is_paused(), paused);
            assert!(!lab.ball);
            assert_eq!(poses(&lab.sandbox), poses(&Sandbox::new()));
            assert_eq!(lab.sandbox.bodies.len(), 34);
            assert_eq!(lab.sandbox.physics.bodies.len(), 42);
            assert_eq!(lab.sandbox.links.len(), 4);
            assert_eq!(lab.sandbox.pulse_time, 0.);
            assert_eq!(lab.impact_cooldown, 0.);
            assert!(lab.impact.is_none());
            assert!(!lab.pulsed);
            assert!(lab.feedback.starts_with("Scene reset."));
            lab.session.set_paused(false);
            assert_eq!(lab.update(0.5 / TICK_HZ, LabInput::default()).steps, 0);
        }
    }

    #[test]
    fn paused_drops_use_selection_and_clamped_preview_without_moving() {
        let mut lab = Lab::new();
        lab.session.set_paused(true);
        let before = poses(&lab.sandbox);
        lab.update(
            1.,
            LabInput {
                ball: Some(false),
                drop: Some((vec2(90., -10.), false)),
                ..LabInput::default()
            },
        );
        let body = lab.sandbox.bodies.last().unwrap();
        assert!(matches!(body.shape, Shape::Box(_)));
        assert_eq!(body.previous_position, vec2(11.2, 0.6));
        assert_eq!(
            lab.sandbox.physics.bodies[body.handle].translation(),
            body.previous_position
        );
        assert_eq!(lab.sandbox.physics.bodies[body.handle].linvel(), Vec2::ZERO);
        assert_eq!(&poses(&lab.sandbox)[..before.len()], &before);
        assert_eq!(lab.session.tick(), 0);

        lab.update(
            0.,
            LabInput {
                ball: Some(true),
                drop: Some((vec2(0., 100.), true)),
                ..LabInput::default()
            },
        );
        assert!(lab.ball, "secondary click does not change selection");
        assert!(matches!(
            lab.sandbox.bodies.last().unwrap().shape,
            Shape::Box(_)
        ));
        assert_eq!(lab.sandbox.bodies.last().unwrap().previous_position.y, 12.4);
        lab.update(
            0.,
            LabInput {
                drop: Some((vec2(0., 10.), false)),
                ..LabInput::default()
            },
        );
        assert!(matches!(
            lab.sandbox.bodies.last().unwrap().shape,
            Shape::Ball(_)
        ));
    }

    #[test]
    fn drop_is_once_per_frame_and_reports_capacity_and_invalid_positions() {
        let mut lab = Lab::new();
        let initial_count = lab.sandbox.bodies.len();
        let frame = lab.update(
            1.,
            LabInput {
                drop: Some((vec2(0., 10.), false)),
                ..LabInput::default()
            },
        );
        assert_eq!(frame.steps, 8);
        assert_eq!(lab.sandbox.bodies.len(), initial_count + 1);
        lab.session.set_paused(true);
        lab.update(
            0.,
            LabInput {
                drop: Some((vec2(f32::NAN, 10.), false)),
                ..LabInput::default()
            },
        );
        assert_eq!(lab.sandbox.bodies.len(), initial_count + 1);
        assert!(lab.feedback.starts_with("Drop rejected"));
        for _ in 0..MAX_BODIES {
            lab.update(
                0.,
                LabInput {
                    drop: Some((vec2(0., 10.), false)),
                    ..LabInput::default()
                },
            );
        }
        assert_eq!(lab.sandbox.bodies.len(), MAX_BODIES);
        assert_eq!(lab.sandbox.physics.bodies.len(), MAX_BODIES + 8);
        assert!(lab.feedback.starts_with("Body limit reached"));
    }

    #[test]
    fn paused_pulse_is_rejected_and_manual_steps_consume_cooldown() {
        let mut lab = Lab::new();
        lab.session.set_paused(true);
        let before = poses(&lab.sandbox);
        lab.update(
            1.,
            LabInput {
                pulse: Some(vec2(0., 1.)),
                ..LabInput::default()
            },
        );
        assert_eq!(poses(&lab.sandbox), before);
        assert!(!lab.pulsed);
        assert_eq!(lab.sandbox.pulse_time, 0.);
        lab.session.set_paused(false);
        lab.update(
            0.,
            LabInput {
                pulse: Some(vec2(0., 1.)),
                ..LabInput::default()
            },
        );
        assert!(!lab.pulsed, "a zero-tick frame cannot apply an impulse");
        assert_eq!(lab.update(1. / TICK_HZ, LabInput::default()).steps, 1);
        assert!(lab.pulsed);
        lab.session.set_paused(true);
        lab.update(
            10.,
            LabInput {
                single_step: true,
                ..LabInput::default()
            },
        );
        assert!((lab.sandbox.pulse_time - (0.65 - 2. / 120.)).abs() < 1e-6);
    }

    #[test]
    fn autoplay_settling_is_identical_across_render_rates() {
        let mut expected = Lab::new();
        for _ in 0..240 {
            expected.update(1. / TICK_HZ, LabInput::default());
        }
        for fps in [30, 60, 144] {
            let mut lab = Lab::new();
            for _ in 0..fps * 2 {
                lab.update(1. / fps as f64, LabInput::default());
            }
            assert_eq!(lab.session.tick(), 240);
            assert_eq!(poses(&lab.sandbox), poses(&expected.sandbox));
            assert_eq!(lab.impact_cooldown, expected.impact_cooldown);
        }
    }

    #[test]
    fn pulse_wakes_and_lifts_props_and_cannot_be_spammed() {
        let mut sandbox = Sandbox::new();
        assert!(sandbox.pulse(vec2(0., 0.5)));
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
        assert!(!sandbox.pulse(vec2(0., 0.5)));
        for (body, velocity) in sandbox.bodies.iter().zip(velocities) {
            assert_eq!(sandbox.physics.bodies[body.handle].linvel(), velocity);
        }
        for _ in 0..80 {
            sandbox.step(1. / 120.);
        }
        assert!(sandbox.pulse(vec2(0., 0.5)));
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

    #[test]
    fn running_commands_wait_for_a_tick_and_edges_are_taken_only_on_ticks() {
        let mut lab = Lab::new();
        let before = poses(&lab.sandbox);
        let mut commands = LabCommands {
            drop: Some((vec2(0., 10.), false)),
            pulse: Some(vec2(0., 1.)),
        };
        let mut reads = 0;
        let mut actions = || {
            reads += 1;
            std::mem::take(&mut commands)
        };
        assert_eq!(
            lab.update_with_actions(0.5 / TICK_HZ, LabInput::default(), &mut actions)
                .steps,
            0
        );
        assert_eq!(poses(&lab.sandbox), before);
        assert_eq!(lab.sandbox.pulse_time, 0.);
        assert_eq!(
            lab.update_with_actions(3.5 / TICK_HZ, LabInput::default(), &mut actions)
                .steps,
            4
        );
        assert_eq!(reads, 4);
        assert_eq!(lab.sandbox.bodies.len(), before.len() + 1);
        assert!(lab.pulsed);
        assert_eq!(
            lab.selected,
            Some(lab.sandbox.bodies.last().unwrap().handle)
        );
        assert!((lab.sandbox.pulse_time - (0.65 - 4. / 120.)).abs() < 1e-6);
    }

    #[test]
    fn pending_ui_commands_coalesce_and_reset_discards_them() {
        let mut lab = Lab::new();
        for x in [0., 1., 2.] {
            lab.update(
                0.,
                LabInput {
                    drop: Some((vec2(x, 10.), false)),
                    pulse: Some(vec2(x, 1.)),
                    ..Default::default()
                },
            );
        }
        assert_eq!(lab.sandbox.bodies.len(), 34);
        assert_eq!(lab.pending.drop.unwrap().0, vec2(0., 10.));
        lab.update(1. / TICK_HZ, LabInput::default());
        assert_eq!(lab.sandbox.bodies.len(), 35);
        assert!(lab.pending.drop.is_none() && lab.pending.pulse.is_none());
        lab.update(
            0.,
            LabInput {
                drop: Some((vec2(0., 12.), false)),
                ..Default::default()
            },
        );
        lab.update_with_actions(
            30.,
            LabInput {
                reset: true,
                ..Default::default()
            },
            || {
                panic!("reset must not consume gameplay actions");
            },
        );
        assert!(lab.selected.is_none());
        lab.update(1. / TICK_HZ, LabInput::default());
        assert_eq!(lab.sandbox.bodies.len(), 34);
        assert_eq!(lab.sandbox.pulse_time, 0.);
    }

    #[test]
    fn queries_inspect_compound_current_poses_without_waking_or_ticking() {
        let mut lab = Lab::new();
        lab.session.set_paused(true);
        let handle = lab.sandbox.bodies.last().unwrap().handle;
        let before = poses(&lab.sandbox);
        let sleep = lab.sandbox.physics.sleep_state(handle).unwrap();
        let query = lab.sandbox.inspect(vec2(8.95, 10.)).unwrap();
        assert_eq!(query.body(&lab.sandbox), Some(handle));
        assert!(query.report.total > 0);
        lab.select_body(Some(&query));
        assert_eq!(lab.selected, Some(handle));
        assert!(lab.selected_label().contains("COMPOUND"));
        assert_eq!(lab.sandbox.physics.sleep_state(handle), Some(sleep));
        assert_eq!(poses(&lab.sandbox), before);
        assert_eq!(lab.session.tick(), 0);
        lab.sandbox.physics.bodies[handle].set_translation(vec2(0., 10.), true);
        assert_eq!(
            lab.sandbox
                .inspect(vec2(0.75, 10.))
                .unwrap()
                .body(&lab.sandbox),
            Some(handle)
        );
        assert_ne!(
            lab.sandbox
                .inspect(vec2(8.95, 10.))
                .unwrap()
                .body(&lab.sandbox),
            Some(handle)
        );
        lab.sandbox.physics.remove_body(handle);
        assert_eq!(lab.selected_label(), "Selected body unavailable");
        lab.reset();
        assert!(lab.selected.is_none());
        let miss = lab.sandbox.inspect(vec2(0., 12.)).unwrap();
        lab.select_body(Some(&miss));
        assert!(lab.selected.is_none());
    }

    #[test]
    fn inspection_reports_bounded_hits_and_invalid_points() {
        let mut sandbox = Sandbox::new();
        for _ in 0..12 {
            assert!(sandbox.spawn(vec2(0., 10.), true));
        }
        let query = sandbox.inspect(vec2(0., 10.)).unwrap();
        assert_eq!(query.report.written, 8);
        assert_eq!(query.report.total, 12);
        assert!(query.report.is_truncated());
        assert_eq!(
            query.body(&sandbox),
            Some(sandbox.bodies.last().unwrap().handle)
        );
        assert!(sandbox.inspect(vec2(f32::NAN, 10.)).is_err());
        assert!(sandbox.inspect(vec2(f32::INFINITY, 10.)).is_err());
        let miss = sandbox.inspect(vec2(0., 12.)).unwrap();
        assert_eq!(miss.report.total, 0);
        assert!(miss.hits.iter().all(Option::is_none));
    }

    #[test]
    fn collision_feedback_requires_a_real_solid_contact_start() {
        let mut sandbox = Sandbox {
            physics: PhysicsWorld::new([0., -180.]),
            bodies: Vec::new(),
            platforms: Vec::new(),
            links: Vec::new(),
            impact: None,
            pulse_time: 0.,
            pulse_center: Vec2::ZERO,
        };
        sandbox.spawn(vec2(0., 10.), true);
        let handle = sandbox.bodies[0].handle;
        sandbox.physics.bodies[handle].set_linvel(vec2(0., -3.), true);
        sandbox.step(1. / 120.);
        assert!(sandbox.physics.contact_events().is_empty());
        assert!(
            sandbox.impact.is_none(),
            "large gravity velocity changes are silent"
        );
        let (_, floor) = sandbox.physics.insert(
            RigidBodyBuilder::fixed(),
            ColliderBuilder::cuboid(5., 0.5).sensor(true),
        );
        sandbox.physics.bodies[handle].set_translation(vec2(0., 0.7), true);
        sandbox.step(1. / 120.);
        assert!(!sandbox.physics.contact_events().is_empty());
        assert!(sandbox.impact.is_none(), "sensor starts are silent");
        sandbox.physics.colliders[floor].set_sensor(false);
        sandbox.physics.bodies[handle].set_translation(vec2(0., 3.), true);
        sandbox.step(1. / 120.);
        sandbox.physics.bodies[handle].set_translation(vec2(0., 0.95), true);
        sandbox.physics.bodies[handle].set_linvel(vec2(0., -10.), true);
        sandbox.step(1. / 120.);
        assert!(
            sandbox.impact.is_some(),
            "a new solid impact produces feedback"
        );
    }

    #[test]
    fn presentation_policy_clears_existing_rings_without_changing_simulation() {
        use ferravex_play::feedback::FeedbackPolicy;
        let original = preferences::feedback_policy();
        for (motion, flash) in [(true, false), (false, true), (true, true)] {
            preferences::set_feedback_policy(FeedbackPolicy::default());
            let mut visual = PulseVisual::default();
            let mut lab = Lab::new();
            visual.begin_frame(0.);
            lab.update(
                1. / TICK_HZ,
                LabInput {
                    pulse: Some(vec2(0., 1.)),
                    ..Default::default()
                },
            );
            assert!(lab.pulsed);
            visual.accepted_pulse(lab.sandbox.pulse_center);
            assert!(visual.ring.is_some());
            lab.session.set_paused(true);
            let before = poses(&lab.sandbox);
            let cooldown = lab.sandbox.pulse_time;
            preferences::set_feedback_policy(FeedbackPolicy::new(motion, flash, 1.).unwrap());
            visual.begin_frame(0.); // menus/paused/zero-tick policy changes still apply
            assert!(visual.ring.is_none());
            visual.accepted_pulse(vec2(0., 1.));
            assert!(visual.ring.is_none());
            assert_eq!(poses(&lab.sandbox), before);
            assert_eq!(lab.sandbox.pulse_time, cooldown);
            assert_eq!(lab.session.tick(), 1);
            preferences::set_feedback_policy(FeedbackPolicy::default());
            visual.begin_frame(0.);
            assert!(
                visual.ring.is_none(),
                "re-enabling must not revive old effects"
            );
            visual.accepted_pulse(vec2(0., 1.));
            assert!(visual.ring.is_some());
            for _ in 0..7 {
                visual.begin_frame(0.1);
            }
            assert!(visual.ring.is_none());
            assert_eq!(
                lab.sandbox.pulse_time, cooldown,
                "cosmetic time does not recharge physics"
            );
        }
        preferences::set_feedback_policy(original);
    }

    #[test]
    fn all_presentation_policies_preserve_running_impulses_and_ticks() {
        use ferravex_play::feedback::FeedbackPolicy;
        let original = preferences::feedback_policy();
        let mut expected = Lab::new();
        for frame in 0..20 {
            expected.update(
                1. / 60.,
                LabInput {
                    pulse: (frame == 0).then_some(vec2(0., 1.)),
                    ..Default::default()
                },
            );
        }
        for (motion, flash, intensity) in [
            (false, false, 1.),
            (true, false, 1.),
            (false, true, 1.),
            (true, true, 1.),
            (false, false, 0.),
        ] {
            preferences::set_feedback_policy(
                FeedbackPolicy::new(motion, flash, intensity).unwrap(),
            );
            let mut lab = Lab::new();
            let mut visual = PulseVisual::default();
            for frame in 0..20 {
                visual.begin_frame(1. / 60.);
                lab.update(
                    1. / 60.,
                    LabInput {
                        pulse: (frame == 0).then_some(vec2(0., 1.)),
                        ..Default::default()
                    },
                );
                if lab.pulsed {
                    visual.accepted_pulse(lab.sandbox.pulse_center);
                }
            }
            assert_eq!(lab.session.tick(), 40);
            assert_eq!(poses(&lab.sandbox), poses(&expected.sandbox));
            assert_eq!(lab.sandbox.pulse_time, expected.sandbox.pulse_time);
            assert_eq!(visual.ring.is_none(), motion || flash || intensity == 0.);
        }
        preferences::set_feedback_policy(original);
    }

    #[test]
    fn lab_hints_follow_rebinding_and_unbound_actions() {
        let mut input = PlayerInput::transient("journey200-142-hints");
        register_lab_actions(&mut input);
        assert_eq!(action_hint(&input, "step"), "N");
        assert_eq!(action_hint(&input, "drop"), "Enter");
        assert_eq!(action_hint(&input, "inspect"), "I");
        input
            .rebind(
                "step",
                vec![Binding::digital(InputCode::Key(KeyCode::J as u16))],
            )
            .unwrap();
        assert_eq!(action_hint(&input, "step"), "J");
        input.rebind("inspect", vec![]).unwrap();
        assert_eq!(action_hint(&input, "inspect"), "Unbound");
    }

    #[test]
    fn button_availability_matches_pause_cooldown_and_capacity() {
        let mut lab = Lab::new();
        assert!(!lab.button_enabled(5));
        assert!(lab.button_enabled(3));
        lab.update(
            1. / TICK_HZ,
            LabInput {
                pulse: Some(vec2(0., 1.)),
                ..Default::default()
            },
        );
        assert!(!lab.button_enabled(3));
        lab.session.set_paused(true);
        assert!(lab.button_enabled(5));
        assert!(!lab.button_enabled(3));
        while lab.sandbox.spawn(vec2(0., 10.), true) {}
        assert!(!lab.button_enabled(2));
        lab.reset();
        assert!(lab.button_enabled(2));
        assert!(lab.button_enabled(5));
    }
}
