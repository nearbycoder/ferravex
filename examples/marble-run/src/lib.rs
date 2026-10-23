//! Four authored rolling-ball trials with a pausable, fixed-step countdown.
//!
//! The host uses [`ferravex_core::session::GameSession`] to gate both countdown
//! and physics ticks. Course selection, input focus and settings pauses never
//! charge trial time. [`run`] launches the native game; browser exports are
//! supplied by the same renderer game entry point.
//!
//! ```
//! use ferravex_core::session::{GameSession, SessionState};
//! let mut session = GameSession::new(120., 12, false);
//! assert_eq!(session.state(), SessionState::Ready); // course picker
//! session.start();
//! assert_eq!(session.advance(1. / 60.).steps, 2);
//! session.set_paused(true);
//! assert_eq!(session.advance(30.).steps, 0); // no trial/countdown time
//! session.restart(); // selecting another course or retrying starts fresh
//! assert_eq!(session.tick(), 0);
//! ```
//!
//! Finish comparisons use the shared record summary before changing the best:
//!
//! ```
//! use ferravex_engine::persistence::RecordBook;
//! let mut records = RecordBook::default();
//! records.record("aqueduct", 1, 3, Some(30_000_000)).unwrap();
//! let finish = records.record_with_summary("aqueduct", 1, 2, Some(29_000_000)).unwrap();
//! assert_eq!(finish.time_us.unwrap().delta, Some(-1_000_000));
//! // A faster run can earn a lower medal after a recovery; metrics stay separate.
//! assert_eq!(records.get("aqueduct", 1).unwrap().score, 3);
//! ```
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{
    ActionKind, Binding, InputCode, PlayerInput, Preset,
    touch::{Control, Output, Region},
};
mod course_ui;
mod courses;
mod hud;
#[cfg(test)]
mod journey200_tests;
mod level;
mod presentation;
mod sim;
mod timing;
mod trial;
use ferravex_engine::camera::{Input3, Mode3, Settings3, Target3};
use ferravex_render::{
    demo::Demo,
    mesh::Mesh3d,
    prelude::*,
    shaders::{LitMaterial, LitSettings},
    visibility::{LodChain, LodState},
};
use sim::{Controls, Run};
use std::{collections::BTreeMap, rc::Rc, sync::Arc};
use trial::Trial;

const SKY: Color = Color::from_rgba(172, 194, 189, 255);
const CREAM: Color = Color::from_rgba(255, 241, 205, 255);
const INK: Color = Color::from_rgba(32, 57, 60, 255);
const TEAL: Color = Color::from_rgba(62, 143, 133, 255);
const GOLD: Color = Color::from_rgba(236, 175, 65, 255);
fn conf() -> Conf {
    ferravex_render::window_conf("Marble Run / Ferravex")
}
fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba(r, g, b, 255)
}
struct Prop {
    mesh: Rc<Mesh3d>,
    position: Vec3,
    rotation: Quat,
    lod: Option<LodChain>,
    lod_state: std::cell::Cell<LodState>,
}
impl Prop {
    fn new(mesh: Mesh3d, position: Vec3) -> Self {
        Self {
            mesh: Rc::new(mesh),
            position,
            rotation: Quat::IDENTITY,
            lod: None,
            lod_state: Default::default(),
        }
    }
    fn oriented(mut self, rotation: Quat) -> Self {
        self.rotation = rotation;
        self
    }
    fn with_lod(mut self, low: Mesh3d) -> Self {
        self.lod = Some(
            LodChain::new(
                vec![self.mesh.resident().clone(), low.resident().clone()],
                vec![50.],
                0.12,
            )
            .unwrap(),
        );
        self
    }
    fn draw(&self) {
        if let Some(chain) = &self.lod {
            let mut state = self.lod_state.get();
            ferravex_render::draw_lod(
                chain,
                Mat4::from_rotation_translation(self.rotation, self.position),
                &mut state,
            );
            self.lod_state.set(state);
        } else {
            self.mesh.draw(self.position, self.rotation);
        }
    }
}
// The aqueduct is an open overhead course. Small gameplay poles must never
// retract the camera toward its subject; retain the authored scale near them.
fn follow_settings() -> Settings3 {
    Settings3 {
        position_half_life: std::f32::consts::LN_2 / 6.,
        target_half_life: std::f32::consts::LN_2 / 6.,
        teleport_distance: Some(8.),
        obstruction: None,
        ..Default::default()
    }
}
fn follow_input(position: Vec3) -> Input3 {
    Input3 {
        target: Target3::at(position),
        mode: Mode3::Follow {
            eye_offset: vec3(8., 12., 14.),
            look_offset: vec3(0., -0.2, -5.),
        },
        fov_y: 48_f32.to_radians(),
    }
}
fn course_composition(level: &level::Level) -> ferravex_engine::camera::CameraComposer<1> {
    use ferravex_engine::camera::{CameraComposer, CameraZone};
    CameraComposer::new(
        [CameraZone {
            min: level.finish - Vec3::splat(8.),
            max: level.finish + Vec3::splat(8.),
            feather: 4.,
            priority: 1,
            focus_offset: vec3(0., 0., -0.8),
            boom_scale: 1.,
            fov_scale: 1.,
        }],
        0.2,
        1.5,
        0.18,
    )
    .unwrap()
}
fn touch_hud(size: Vec2) -> (Vec2, Vec2, f32) {
    let radius = (size.x * 0.15).min(size.y * 0.22).clamp(28., 66.);
    let y = (size.y - radius - 14.).max(radius + 14.);
    (
        vec2(radius + 18., y),
        vec2(size.x - radius - 18., y),
        radius,
    )
}
type GeometryKey = (level::Shape, [u32; 3], [u8; 4], u16);
fn cached_prop(bank: &mut BTreeMap<GeometryKey, Rc<Mesh3d>>, d: level::Decoration) -> Prop {
    let key = (
        d.shape,
        d.size.to_array().map(f32::to_bits),
        d.color.to_rgba(),
        d.segments,
    );
    let mesh = bank
        .entry(key)
        .or_insert_with(|| {
            Rc::new(match d.shape {
                level::Shape::Box => Mesh3d::cuboid(d.size, d.color),
                level::Shape::Cylinder => {
                    Mesh3d::cylinder(d.size.x * 0.5, d.size.y, d.color, d.segments)
                }
            })
        })
        .clone();
    Prop {
        mesh,
        position: d.position,
        rotation: d.rotation,
        lod: None,
        lod_state: Default::default(),
    }
}
struct Scene {
    platforms: Vec<Prop>,
    decoration: Vec<Prop>,
    marble: Mesh3d,
    gem: Mesh3d,
    gem_base: Mesh3d,
    obstacles: Vec<Prop>,
    checkpoint: Mesh3d,
    checkpoint_lit: Mesh3d,
    checkpoint_next: Mesh3d,
    shadow: Mesh3d,
    sweepers: Vec<Mesh3d>,
}
impl Scene {
    fn new(level: &level::Level) -> Self {
        let platforms = level
            .platforms
            .iter()
            .enumerate()
            .map(|(i, p)| {
                Prop::new(
                    Mesh3d::cuboid(
                        p.size,
                        if p.friction < 0.2 {
                            rgb(151, 193, 202)
                        } else if i % 2 == 0 {
                            rgb(190, 149, 113)
                        } else {
                            rgb(210, 172, 126)
                        },
                    ),
                    p.center,
                )
                .oriented(p.rotation)
            })
            .collect();
        let mut decoration = Vec::new();
        // Hanging stone buttresses and terracotta trim make the course feel built.
        for (i, p) in level.platforms.iter().enumerate() {
            decoration.push(
                Prop::new(
                    Mesh3d::cuboid(
                        vec3(p.size.x + 0.08, 0.12, p.size.z + 0.08),
                        rgb(129, 102, 85),
                    ),
                    p.at(vec3(0., -p.size.y * 0.5 + 0.04, 0.)),
                )
                .oriented(p.rotation),
            );
            if i % 2 == 0 {
                decoration.push(
                    Prop::new(
                        Mesh3d::frustum(0.65, 2.0, 6.0, rgb(101, 121, 112), 5),
                        p.at(vec3(0., -p.size.y * 0.5 - 3., 0.)),
                    )
                    .oriented(p.rotation),
                );
                decoration.push(
                    Prop::new(
                        Mesh3d::cuboid(vec3(1.15, 1.6, 1.15), rgb(161, 132, 104)),
                        p.at(vec3(0., -p.size.y * 0.5 - 0.8, 0.)),
                    )
                    .oriented(p.rotation),
                );
            }
            // Inset colored edges are visual guides, not invisible collision rails.
            for side in [-1., 1.] {
                let (size, offset) = if p.size.z > p.size.x {
                    (
                        vec3(0.11, 0.018, p.size.z),
                        vec3(side * (p.size.x * 0.5 - 0.15), p.size.y * 0.5 + 0.011, 0.),
                    )
                } else {
                    (
                        vec3(p.size.x, 0.018, 0.11),
                        vec3(0., p.size.y * 0.5 + 0.011, side * (p.size.z * 0.5 - 0.15)),
                    )
                };
                decoration
                    .push(Prop::new(Mesh3d::cuboid(size, TEAL), p.at(offset)).oriented(p.rotation));
            }
        }
        // Thin expansion joints across each lane, using geometry rather than textures.
        for p in level
            .platforms
            .iter()
            .filter(|p| p.size.x.max(p.size.z) > 7.)
        {
            let length = p.size.x.max(p.size.z);
            for i in 1..length as usize {
                let d = i as f32 - length * 0.5;
                let (size, offset) = if p.size.z > p.size.x {
                    (
                        vec3((p.size.x - 0.4).max(0.05), 0.013, 0.025),
                        vec3(0., p.size.y * 0.5 + 0.008, d),
                    )
                } else {
                    (
                        vec3(0.025, 0.013, (p.size.z - 0.4).max(0.05)),
                        vec3(d, p.size.y * 0.5 + 0.008, 0.),
                    )
                };
                decoration.push(
                    Prop::new(Mesh3d::cuboid(size, rgb(160, 128, 99)), p.at(offset))
                        .oriented(p.rotation),
                );
            }
        }
        let mut bank = BTreeMap::new();
        for d in &level.decorations {
            decoration.push(cached_prop(&mut bank, *d));
        }
        let mut obstacles = Vec::new();
        for p in &level.obstacles {
            obstacles.push(cached_prop(
                &mut bank,
                level::Decoration {
                    shape: level::Shape::Cylinder,
                    position: p.position,
                    size: vec3(p.radius * 2., p.height, p.radius * 2.),
                    rotation: p.rotation,
                    color: rgb(146, 80, 61),
                    segments: 8,
                },
            ));
            obstacles.push(cached_prop(
                &mut bank,
                level::Decoration {
                    shape: level::Shape::Cylinder,
                    position: p.position + p.rotation * vec3(0., p.height * 0.5 - 0.1, 0.),
                    size: vec3(p.radius * 2. + 0.02, 0.13, p.radius * 2. + 0.02),
                    rotation: p.rotation,
                    color: CREAM,
                    segments: 8,
                },
            ));
        }
        for x in [-2.1, 2.1] {
            decoration.push(Prop::new(
                Mesh3d::cuboid(vec3(0.3, 3.2, 0.3), TEAL),
                level.finish + vec3(x, 1., -0.8),
            ));
        }
        decoration.push(Prop::new(
            Mesh3d::cuboid(vec3(4.6, 0.5, 0.4), TEAL),
            level.finish + vec3(0., 2.75, -0.8),
        ));
        for x in -4..=4 {
            decoration.push(Prop::new(
                Mesh3d::cuboid(vec3(0.45, 0.02, 1.4), if x % 2 == 0 { CREAM } else { INK }),
                level.finish + vec3(x as f32 * 0.45, -0.58, 0.),
            ));
        }
        // A distant archipelago is intentionally coarse and bounded (25 meshes).
        for i in 0..25 {
            let angle = i as f32 * 2.39996;
            let distance = 40. + (i % 5) as f32 * 9.;
            let p = vec3(angle.cos() * distance, -15., -20. + angle.sin() * distance);
            let radius = 8. + (i % 4) as f32 * 2.;
            let height = 15. + (i % 3) as f32 * 7.;
            let color = if i % 2 == 0 {
                rgb(110, 144, 130)
            } else {
                rgb(131, 155, 139)
            };
            decoration.push(
                Prop::new(Mesh3d::cone(radius, height, color, 5), p)
                    .with_lod(Mesh3d::cone(radius, height, color, 3)),
            );
        }
        let mut marble = Mesh3d::sphere(level.tuning.radius, CREAM, 12, 8);
        for v in &mut marble.mesh.vertices {
            if v.position.y.abs() < 0.18 {
                v.color = GOLD.to_rgba();
            } else if v.position.x < -0.2 {
                v.color = TEAL.to_rgba();
            }
        }
        Self {
            platforms,
            decoration,
            marble,
            gem: Mesh3d::sphere(0.36, GOLD, 4, 2),
            gem_base: Mesh3d::cylinder(0.48, 0.022, rgb(185, 134, 61), 12),
            obstacles,
            checkpoint: Mesh3d::cylinder(1.25, 0.02, rgb(105, 142, 128), 16),
            checkpoint_lit: Mesh3d::cylinder(1.25, 0.025, TEAL, 16),
            checkpoint_next: Mesh3d::cylinder(1.25, 0.025, GOLD, 16),
            shadow: Mesh3d::cylinder(0.62, 0.015, rgb(131, 113, 90), 12),
            sweepers: level
                .sweepers
                .iter()
                .map(|s| {
                    let beam = Mesh3d::cuboid(s.size, rgb(172, 83, 52));
                    let stripe = Mesh3d::cuboid(vec3(0.28, 0.025, s.size.z + 0.02), CREAM);
                    Mesh3d::combine(std::iter::once((&beam.mesh, Mat4::IDENTITY)).chain(
                        [-2., -1., 0., 1., 2.].into_iter().map(|x| {
                            (
                                &stripe.mesh,
                                Mat4::from_translation(vec3(
                                    x * s.size.x / 5.,
                                    s.size.y * 0.5 + 0.02,
                                    0.,
                                )),
                            )
                        }),
                    ))
                    .unwrap()
                })
                .collect(),
        }
    }
    fn draw(
        &self,
        game: &Run,
        position: Vec3,
        rotation: Quat,
        time: f32,
        presentation: &presentation::Presentation,
    ) {
        for prop in &self.platforms {
            prop.draw();
        }
        for prop in &self.decoration {
            prop.draw();
        }
        for prop in &self.obstacles {
            prop.draw();
        }
        for (mesh, sweeper) in self.sweepers.iter().zip(&game.level.sweepers) {
            mesh.draw(sweeper.position, sweeper.rotation(time));
        }
        for (i, p) in game.level.checkpoints.iter().enumerate() {
            (if game.checkpoint > i {
                &self.checkpoint_lit
            } else if game.checkpoint == i {
                &self.checkpoint_next
            } else {
                &self.checkpoint
            })
            .draw(p.marker, Quat::IDENTITY);
        }
        for (i, p) in game.level.gems.iter().enumerate() {
            if !game.collected[i] {
                self.gem_base
                    .draw(vec3(p.x, p.y - 0.675, p.z), Quat::IDENTITY);
                let (position, rotation) = presentation.gem_pose(*p, i);
                self.gem.draw(position, rotation);
            }
        }
        let mut surface: Option<(f32, Vec3, Vec3)> = None;
        for p in &game.level.platforms {
            let normal = p.rotation * Vec3::Y;
            if normal.y <= 0.001 {
                continue;
            }
            let distance = (position - p.at(vec3(0., p.size.y * 0.5, 0.))).dot(normal) / normal.y;
            if distance < 0. || surface.is_some_and(|(old, _, _)| distance >= old) {
                continue;
            }
            let point = position - Vec3::Y * distance;
            let local = p.rotation.inverse() * (point - p.center);
            if local.x.abs() <= p.size.x * 0.5 && local.z.abs() <= p.size.z * 0.5 {
                surface = Some((distance, point, normal));
            }
        }
        if let Some((_, point, normal)) = surface {
            let rotation = Quat::from_rotation_arc(Vec3::Y, normal);
            self.shadow
                .draw(point + rotation * vec3(0.12, 0.022, 0.18), rotation);
        }
        self.marble.draw(position, rotation);
    }
}
fn text(s: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(s, x + 1., y + 1., size, Color::new(0.1, 0.2, 0.2, 0.25));
    draw_text(s, x, y, size, color);
}
fn centered(s: &str, y: f32, size: u16, color: Color) {
    hud::centered(s, y, size, color);
}
fn trial_actions(player_input: &mut PlayerInput) {
    player_input
        .global_action(
            "overview",
            "Course overview",
            ActionKind::Button,
            vec![
                Binding::digital(InputCode::Key(KeyCode::Tab as u16)),
                Binding::digital(InputCode::PadButton(3)),
            ],
        )
        .unwrap();
    for (name, label, key, pad) in [
        ("courses", "Choose course", KeyCode::C, 2),
        ("recover", "Return to checkpoint", KeyCode::V, 4),
    ] {
        let bindings = vec![
            Binding::digital(InputCode::Key(key as u16)),
            Binding::digital(InputCode::PadButton(pad)),
        ];
        if name == "recover" {
            player_input.action(name, label, ActionKind::Button, bindings)
        } else {
            player_input.global_action(name, label, ActionKind::Button, bindings)
        }
        .unwrap();
    }
}

ferravex_render::game!(conf, game);
async fn game() {
    let mut demo = Demo::with_content();
    let mut content =
        ferravex_render::content::ContentSource::new(level::SOURCE, demo.content.as_deref())
            .unwrap();
    let mut game = Run::with_level(Arc::new(level::Level::parse(content.current()).unwrap()));
    let mut scene = Scene::new(&game.level);
    let mut records = timing::TrialRecords::new();
    let mut record_context = courses::context(content.current());
    let mut course = courses::COURSES
        .iter()
        .position(|c| c.source == content.current())
        .unwrap_or(0);
    records.begin(course, record_context);
    let mut trial = Trial::new(!demo.autoplay && demo.content.is_none());
    let mut course_ui = ferravex_render::ui::Ui::new();
    let material = LitMaterial::new().expect("Ferravex's portable lit material must compile");
    let mut sound = GameAudio::new();
    let mut impact_cooldown = 0_f32;
    let mut player_input = PlayerInput::preset("marble-run", Preset::Racer);
    trial_actions(&mut player_input);
    let mut presentation = presentation::Presentation::new(&game.level);
    let mut previous_falls = 0;
    let forward = vec3(-8., 0., -19.).normalize();
    let right = vec3(-forward.z, 0., forward.x);
    let mut pending_course: Option<usize> = None;
    loop {
        if let Some(index) = pending_course.take() {
            if index != course {
                // The shared content editor must show the selected course too.
                content = ferravex_render::content::ContentSource::new(
                    courses::COURSES[index].source,
                    None,
                )
                .unwrap();
            }
            course = index;
            let source = content.current();
            game = Run::with_level(Arc::new(level::Level::parse(source).unwrap()));
            scene = Scene::new(&game.level);
            record_context = courses::context(source);
            trial.restart();
            presentation = presentation::Presentation::new(&game.level);
            records.begin(course, record_context);
            previous_falls = 0;
            impact_cooldown = 0.;
            sound = GameAudio::new();
            player_input.set_paused(true);
            player_input.set_paused(false);
        }
        if let Some((next, next_scene)) = content.poll(demo.dt(), |text| {
            let level = Arc::new(level::Level::parse(text)?);
            let next_scene = Scene::new(&level);
            Ok((Run::with_level(level), next_scene))
        }) {
            game = next;
            if let Some(index) = courses::COURSES
                .iter()
                .position(|c| c.source == content.current())
            {
                course = index;
            }
            trial.restart();
            player_input.set_paused(true);
            player_input.set_paused(false);
            scene = next_scene;
            record_context = courses::context(content.current());
            presentation = presentation::Presentation::new(&game.level);
            records.begin(course, record_context);
            previous_falls = 0;
            impact_cooldown = 0.;
            sound = GameAudio::new();
        }
        presentation.begin_frame();
        ferravex_render::ui::warm_up_font_sizes(&[12, 17, 18, 20, 23, 26, 30]);
        let (stick_center, brake_center, touch_radius) =
            touch_hud(vec2(screen_width(), screen_height()));
        player_input
            .set_touch_layout(&[
                Control {
                    region: Region::Circle {
                        center: stick_center,
                        radius: touch_radius + 8.,
                    },
                    output: Output::Stick {
                        x: 0,
                        y: 1,
                        radius: touch_radius * 0.72,
                        floating: false,
                    },
                },
                Control {
                    region: Region::Circle {
                        center: brake_center,
                        radius: touch_radius + 8.,
                    },
                    output: Output::Button(6),
                },
            ])
            .unwrap();
        // Picker visibility is independent of the player's manual pause.
        player_input.sample(!trial.picker && !player_input.paused() && !game.finished);
        if player_input.pressed("pause") {
            player_input.set_paused(!player_input.paused());
            player_input.sample(!trial.picker && !player_input.paused() && !game.finished);
        }
        if player_input.pressed("courses") {
            trial.toggle_picker();
            player_input.sample(!trial.picker && !player_input.paused() && !game.finished);
        }
        let touch_mode = player_input.touch_seen();
        let touch = player_input
            .touch_stick(0, 1)
            .map_or(Vec2::ZERO, |(_, delta)| delta);
        if player_input.pressed("restart") {
            // Share the course/reload reset path, including input/audio/camera.
            pending_course = Some(course);
        }
        let movement = player_input.movement();
        let direction = right * movement.x + forward * movement.y;
        let input = Controls {
            roll: vec2(direction.x, direction.z).clamp_length_max(1.),
            brake: player_input.held("brake"),
        };
        let frame = trial.advance(
            demo.dt() as f64,
            !player_input.gameplay_active() || pending_course.is_some(),
        );
        ferravex_render::performance::context(Some(trial.session.tick()), None);
        for _ in 0..frame.steps {
            if !trial.session.is_playing() {
                break;
            }
            impact_cooldown = (impact_cooldown - frame.dt).max(0.);
            let controls = if demo.autoplay && trial.countdown().is_none() {
                game.autopilot()
            } else {
                input
            };
            let before = Vec3::from_array(game.physics.bodies[game.marble].linvel().to_array());
            let gems = game.gems();
            let falls = game.falls;
            let checkpoint = game.checkpoint;
            let finished = game.finished;
            let countdown = trial.countdown();
            if player_input.pressed("recover") {
                trial.request_recovery();
            }
            trial.step(&mut game, controls);
            if trial.countdown() != countdown {
                sound.cue(
                    if trial.countdown().is_some() {
                        Cue::Click
                    } else {
                        Cue::Success
                    },
                    0.3,
                );
            }
            let after = Vec3::from_array(game.physics.bodies[game.marble].linvel().to_array());
            if game.gems() > gems {
                sound.cue(Cue::Pickup, 0.6);
            }
            if game.checkpoint > checkpoint {
                sound.cue(Cue::Success, 0.35);
            }
            if game.falls > falls {
                sound.cue(Cue::Failure, 0.5);
            } else if before.length() > 1.
                && (after - before).length() > 1.2
                && impact_cooldown == 0.
            {
                sound.cue(
                    Cue::Impact(Surface::Stone),
                    ((after - before).length() * 0.12).clamp(0.15, 0.65),
                );
                impact_cooldown = 0.055;
            }
            if game.finished && !finished {
                ferravex_render::performance::event(
                    ferravex_engine::performance::DiagnosticKind::Completed,
                    game.elapsed,
                );
                records.finish(course, record_context, &game);
                sound.cue(Cue::Success, 0.65);
            }
        }
        let (position, rotation) = trial.pose(&game);
        if game.falls != previous_falls {
            ferravex_render::performance::event(
                ferravex_engine::performance::DiagnosticKind::Fall,
                game.falls as f32,
            );
            presentation.recover(position);
        }
        previous_falls = game.falls;
        let camera_dt = if trial.session.is_playing() {
            demo.dt()
        } else {
            0.
        };
        let follow = presentation.update(
            position,
            game.physics.bodies[game.marble].linvel(),
            camera_dt,
        );
        clear_background(SKY);
        let overview = player_input.held("overview");
        let view = if overview {
            presentation.overview(screen_width(), screen_height())
        } else {
            follow
        };
        let camera_position = view.position;
        let target = view.target;
        set_camera(&Camera3D {
            position: camera_position,
            target,
            up: view.up,
            fovy: view.fov_y,
            z_near: 0.1,
            z_far: presentation.far_plane(view),
            ..Default::default()
        });
        material.begin(LitSettings {
            camera_position,
            fog_color: SKY,
            fog_start: if overview {
                presentation.far_plane(view) - 20.
            } else {
                38.
            },
            fog_end: if overview {
                presentation.far_plane(view)
            } else {
                115.
            },
            light_direction: vec3(-0.55, 0.85, 0.35),
            ambient: 0.48,
        });
        let visual_time = trial.visual_time(&game);
        scene.draw(&game, position, rotation, visual_time, &presentation);
        material.end();
        set_default_camera();
        let compact = screen_width() < 560.;
        let status = format!(
            "{} / {}     {:02}:{:04.1}",
            game.gems(),
            game.level.gems.len(),
            (game.elapsed / 60.) as u32,
            game.elapsed % 60.
        );
        let status_size = if screen_width() < 360. {
            13
        } else if compact {
            17
        } else {
            23
        };
        let status_width = measure_text(&status, None, status_size, 1.).width;
        let status_x = if compact {
            32.
        } else {
            screen_width() - status_width - 120.
        };
        let status_y = if compact { 72. } else { 39. };
        text(&status, status_x, status_y, status_size as f32, INK);
        draw_poly(status_x - 16., status_y - 8., 4, 6., 0., GOLD);
        if !trial.picker && !game.finished {
            if screen_height() >= 240. {
                let x = if compact { 16. } else { 28. };
                let available = screen_width() - x - if touch_mode { 114. } else { 16. };
                let (label, size, _) = hud::fitted(
                    &format!(
                        "CP {} / {} / FALLS {}",
                        game.checkpoint,
                        game.level.checkpoints.len(),
                        game.falls,
                    ),
                    available,
                    12,
                );
                text(&label, x, if compact { 94. } else { 80. }, size, INK);
            }
            if screen_height() >= 300. {
                let x = if compact { 16. } else { 28. };
                let y = if compact { 116. } else { 100. };
                let available = screen_width() - x - if touch_mode { 114. } else { 16. };
                let (goal, size, _) = hud::fitted(&hud::goal(&game), available, 12);
                if overview {
                    text(&goal, x, y, size, INK);
                }
                if let Some(split) = records.split_label(&game) {
                    let (split, size, _) = hud::fitted(&split, available, 12);
                    text(&split, x, y + 20., size, INK);
                }
            }
            let message = hud::notice(&game, &trial, &player_input);
            if let Some((title, detail)) = message {
                let y = (screen_height() * 0.35)
                    .max(if screen_height() >= 300. { 150. } else { 100. })
                    .min((screen_height() - 36.).max(100.));
                let width = (screen_width() - 24.).min(360.);
                draw_rectangle(
                    (screen_width() - width) * 0.5,
                    y - 25.,
                    width,
                    55.,
                    Color::new(0.07, 0.13, 0.14, 0.86),
                );
                hud::centered_in(
                    &title,
                    y,
                    if trial.countdown().is_some() || trial.showing_go() {
                        26
                    } else {
                        17
                    },
                    CREAM,
                    width - 16.,
                );
                hud::centered_in(
                    &detail,
                    y + 20.,
                    if compact { 10 } else { 12 },
                    CREAM,
                    width - 16.,
                );
            }
        }
        if touch_mode && !player_input.touch_customized() && !game.finished && !trial.picker {
            let center = stick_center;
            draw_circle(
                center.x,
                center.y,
                touch_radius,
                Color::new(0.12, 0.25, 0.24, 0.15),
            );
            draw_circle_lines(center.x, center.y, touch_radius, 2., CREAM);
            draw_circle(
                center.x + touch.x * touch_radius * 0.53,
                center.y + touch.y * touch_radius * 0.53,
                touch_radius * 0.36,
                Color::new(0.98, 0.94, 0.81, 0.7),
            );
            draw_circle(
                brake_center.x,
                brake_center.y,
                touch_radius * 0.8,
                Color::new(0.12, 0.25, 0.24, 0.35),
            );
            text(
                "BRAKE",
                brake_center.x - measure_text("BRAKE", None, 14, 1.).width * 0.5,
                brake_center.y + 5.,
                14.,
                CREAM,
            );
        }
        if overview && !touch_mode && screen_width() > 620. {
            text(
                courses::COURSES[course].challenge,
                28.,
                screen_height() - 32.,
                12.,
                INK,
            );
        }
        if ferravex_render::content::reload_state().status
            == ferravex_render::content::ReloadStatus::Rejected
        {
            centered(
                "EDIT REJECTED / LAST GOOD COURSE ACTIVE",
                screen_height() - 8.,
                10,
                INK,
            );
        }
        if let Some(command) = course_ui::draw(
            &mut course_ui,
            &mut player_input,
            &game,
            course_ui::View {
                course,
                picker: trial.picker,
                resumable: trial.resumable(&game),
                can_recover: trial.can_recover(),
                context: record_context,
            },
            &records,
        ) {
            use course_ui::Command;
            let selected = match command {
                Command::Select(index) => Some(index),
                Command::Next if course + 1 < courses::COURSES.len() => Some(course + 1),
                Command::Retry => Some(course),
                Command::Next => {
                    trial.toggle_picker();
                    None
                }
                Command::Toggle => {
                    trial.toggle_picker();
                    None
                }
                Command::Recover => {
                    trial.request_recovery();
                    None
                }
            };
            // Keep the current frame's resident meshes alive through GPU submission.
            if let Some(selected) = selected {
                pending_course = Some(selected);
            }
        }
        if (!trial.picker && !game.finished) || player_input.controls_open() {
            player_input.draw_settings();
        }
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod content_scene_tests {
    use super::*;
    #[test]
    fn authored_platform_mesh_and_collision_share_dimensions_and_rotation() {
        let source = level::SOURCE
            .replace(
                "transform 0. 0. 2. 0 0 0 1 1 1 1;",
                "transform 0 0 2 0 0.14943813 0 0.98877108 1 1 1;",
            )
            .replace("size vec3 6. 0.6 6.;", "size vec3 8 0.6 6;");
        assert_ne!(source, level::SOURCE);
        let level = Arc::new(level::Level::parse(&source).unwrap());
        let scene = Scene::new(&level);
        let run = Run::with_level(level.clone());
        let p = level.platforms[0];
        assert_eq!(scene.platforms[0].rotation, p.rotation);
        assert!(
            scene.platforms[0]
                .mesh
                .mesh
                .vertices
                .iter()
                .any(|v| (v.position.x - 4.).abs() < 0.001)
        );
        let origin = p.at(vec3(3.5, 4., 0.));
        let hit = run
            .physics
            .raycast(origin.to_array(), (-Vec3::Y).to_array(), 5.)
            .unwrap();
        assert!((hit.point[1] - 0.3).abs() < 0.001);
    }
    #[test]
    fn minimum_authorable_platforms_build_without_invalid_trim_meshes() {
        let source = level::SOURCE.replace("size vec3 3.4 0.6 8.5;", "size vec3 0.05 0.05 10;");
        let level = level::Level::parse(&source).unwrap();
        let scene = Scene::new(&level);
        assert!(
            scene.platforms.iter().all(|p| p
                .mesh
                .mesh
                .vertices
                .iter()
                .all(|v| v.position.is_finite()))
        );
    }
}

#[cfg(test)]
mod presentation_tests {
    use super::*;
    use ferravex_engine::camera::{NoObstructions, Rig3, RigidObstructions};

    #[test]
    fn camera_freezes_on_pause_and_cuts_cleanly_after_retry() {
        let mut run = Run::new();
        let mut camera = Rig3::new(follow_settings()).unwrap();
        let mut composition = course_composition(&run.level);
        let mut input = follow_input(run.level.finish);
        input.target.velocity = vec3(4., 0., -4.);
        let mut view = None;
        for _ in 0..120 {
            let input = composition.update(input, Vec3::ZERO, 1. / 60.).unwrap();
            view = Some(
                camera
                    .update(1. / 60., input, &NoObstructions)
                    .unwrap()
                    .pose,
            );
        }
        let frozen = camera
            .update(
                0.,
                composition.update(input, Vec3::ZERO, 0.).unwrap(),
                &NoObstructions,
            )
            .unwrap()
            .pose;
        assert_eq!(frozen, view.unwrap());

        run.checkpoint = 1;
        run.recover();
        for retry in [false, true] {
            if retry {
                run = Run::with_level(run.level.clone());
            }
            camera.cut();
            composition = course_composition(&run.level);
            let input = composition
                .update(follow_input(run.position()), Vec3::ZERO, 0.)
                .unwrap();
            let frame = camera.update(0., input, &NoObstructions).unwrap();
            assert!(frame.cut);
            assert!(
                frame
                    .pose
                    .position
                    .abs_diff_eq(run.position() + vec3(8., 12., 14.), 1e-5)
            );
            assert!(
                frame
                    .pose
                    .target
                    .abs_diff_eq(run.position() + vec3(0., -0.2, -5.), 1e-5)
            );
        }
    }
    #[test]
    fn follow_distance_stays_constant_beside_every_pole() {
        let run = Run::new();
        let query = RigidObstructions {
            world: &run.physics,
            mask: u32::MAX,
            ignore: Some(run.marble),
        };
        let mut rig = Rig3::new(follow_settings()).unwrap();
        for pole in &run.level.obstacles {
            for step in 0..16 {
                let angle = step as f32 * std::f32::consts::TAU / 16.;
                let radius = pole.radius + run.level.tuning.radius + 0.02;
                let p = pole.position
                    + vec3(
                        angle.cos() * radius,
                        -pole.height * 0.5 + run.level.tuning.radius,
                        angle.sin() * radius,
                    );
                rig.cut();
                let view = rig.update(1. / 60., follow_input(p), &query).unwrap();
                assert!(!view.obstructed);
                assert!(
                    (view.pose.position - view.pose.target).abs_diff_eq(vec3(8., 12.2, 19.), 0.001)
                );
            }
        }
    }
}

#[cfg(test)]
mod journey200_136 {
    use super::*;

    #[test]
    fn recovery_context_change_retains_existing_control_profile_bytes() {
        let mut legacy = PlayerInput::transient("marble-profile");
        legacy
            .global_action(
                "recover",
                "Return to checkpoint",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(KeyCode::B as u16))],
            )
            .unwrap();
        let bytes = legacy.export_config().unwrap();
        let mut current = PlayerInput::transient("marble-profile");
        trial_actions(&mut current);
        assert_eq!(current.import_config(&bytes).unwrap().matched, 1);
        assert_eq!(hud::hint(&current, "recover"), "B");
        assert_eq!(
            ferravex_render::input::decode_config(&bytes)
                .unwrap()
                .actions[0]
                .name,
            "recover"
        );
    }

    #[test]
    fn rejected_reload_keeps_trial_and_splits_then_valid_reload_builds_fresh() {
        use ferravex_render::content::{self, ContentSource, ReloadStatus};
        let mut source = ContentSource::new(level::SOURCE, None).unwrap();
        let mut run = Run::new();
        let mut trial = Trial::new(false);
        for _ in 0..400 {
            trial.step(&mut run, Controls::default());
        }
        run.splits[0] = Some(1_000);
        let before = (run.elapsed, run.position(), run.splits);
        content::queue("not a valid scene").unwrap();
        assert!(
            source
                .poll(0., |text| Ok(Run::with_level(Arc::new(
                    level::Level::parse(text)?
                ))))
                .is_none()
        );
        assert_eq!(content::reload_state().status, ReloadStatus::Rejected);
        assert_eq!(source.current(), level::SOURCE);
        assert_eq!((run.elapsed, run.position(), run.splits), before);
        content::queue(courses::COURSES[2].source).unwrap();
        run = source
            .poll(0., |text| {
                Ok(Run::with_level(Arc::new(level::Level::parse(text)?)))
            })
            .unwrap();
        trial.restart();
        assert_eq!(content::reload_state().status, ReloadStatus::Ready);
        assert_eq!(source.generation(), 1);
        assert_eq!(trial.countdown(), Some(3));
        assert_eq!(run.elapsed, 0.);
        assert!(run.splits.iter().all(Option::is_none));
        assert_eq!(run.sweepers.len(), 3);
    }
}
