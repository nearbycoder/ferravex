//! An asset-free rolling-ball time trial over a suspended, low-poly aqueduct.
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{
    ActionKind, Binding, InputCode, PlayerInput, Preset,
    touch::{Control, Output, Region},
};
mod course_ui;
mod courses;
mod level;
mod sim;
use ferravex_core::FixedClock;
use ferravex_engine::camera::{Input3, Mode3, NoObstructions, Rig3, Settings3, Target3};
use ferravex_render::{
    demo::Demo,
    mesh::Mesh3d,
    prelude::*,
    shaders::{LitMaterial, LitSettings},
    visibility::{LodChain, LodState},
};
use sim::{Controls, Run};
use std::{collections::BTreeMap, rc::Rc, sync::Arc};

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
    fn draw(&self, game: &Run, position: Vec3, rotation: Quat, time: f32) {
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
            } else {
                &self.checkpoint
            })
            .draw(p.marker, Quat::IDENTITY);
        }
        for (i, p) in game.level.gems.iter().enumerate() {
            if !game.collected[i] {
                self.gem_base
                    .draw(vec3(p.x, p.y - 0.675, p.z), Quat::IDENTITY);
                self.gem.draw(
                    *p + vec3(0., 0.15 + (time * 2. + i as f32).sin() * 0.1, 0.),
                    Quat::from_rotation_y(time * 1.3),
                );
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
    let m = measure_text(s, None, size, 1.);
    text(s, (screen_width() - m.width) * 0.5, y, size as f32, color);
}
ferravex_render::game!(conf, game);
async fn game() {
    let mut demo = Demo::with_content();
    let mut content =
        ferravex_render::content::ContentSource::new(level::SOURCE, demo.content.as_deref())
            .unwrap();
    let mut game = Run::with_level(Arc::new(level::Level::parse(content.current()).unwrap()));
    let mut scene = Scene::new(&game.level);
    let mut records = ferravex_render::storage::Records::new("marble-run").unwrap();
    let mut record_context = courses::context(content.current());
    let mut course = 0;
    let mut picker = !demo.autoplay && demo.content.is_none();
    let mut course_ui = ferravex_render::ui::Ui::new();
    let material = LitMaterial::new().expect("Ferravex's portable lit material must compile");
    let mut clock = FixedClock::new(120., 12);
    let mut sound = GameAudio::new();
    let mut impact_cooldown = 0_f32;
    let mut player_input = PlayerInput::preset("marble-run", Preset::Racer);
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
        player_input
            .global_action(
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
    player_input.set_paused(picker);
    let mut camera = Rig3::new(follow_settings()).unwrap();
    let mut composition = course_composition(&game.level);
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
            picker = false;
            clock.reset();
            camera.cut();
            composition = course_composition(&game.level);
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
            picker = false;
            player_input.set_paused(false);
            scene = next_scene;
            record_context = courses::context(content.current());
            clock.reset();
            camera.cut();
            composition = course_composition(&game.level);
            previous_falls = 0;
            impact_cooldown = 0.;
            sound = GameAudio::new();
        }
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
        player_input.sample_play();
        if player_input.pressed("courses") {
            picker = !picker;
            player_input.set_paused(picker);
        }
        if !picker && !game.finished && player_input.pressed("recover") {
            game.recover();
            clock.reset();
            camera.cut();
            composition = course_composition(&game.level);
            sound.cue(Cue::Failure, 0.3);
        }
        let touch_mode = player_input.touch_seen();
        let touch = player_input
            .touch_stick(0, 1)
            .map_or(Vec2::ZERO, |(_, delta)| delta);
        if player_input.pressed("restart") {
            game = Run::with_level(game.level.clone());
            clock.reset();
            camera.cut();
            composition = course_composition(&game.level);
            previous_falls = 0;
            picker = false;
            player_input.set_paused(false);
        }
        let movement = player_input.movement();
        let direction = right * movement.x + forward * movement.y;
        let input = Controls {
            roll: vec2(direction.x, direction.z).clamp_length_max(1.),
            brake: player_input.held("brake"),
        };
        let dt = if picker || game.finished {
            0.
        } else if demo.autoplay {
            demo.dt()
        } else {
            player_input.simulation_dt(demo.dt())
        };
        impact_cooldown = (impact_cooldown - dt).max(0.);
        let frame = clock.advance(dt as f64);
        for _ in 0..frame.steps {
            let controls = if demo.autoplay {
                game.autopilot()
            } else {
                input
            };
            let before = Vec3::from_array(game.physics.bodies[game.marble].linvel().to_array());
            let gems = game.gems();
            let falls = game.falls;
            let finished = game.finished;
            game.step(controls, frame.dt);
            let after = Vec3::from_array(game.physics.bodies[game.marble].linvel().to_array());
            if game.gems() > gems {
                sound.cue(Cue::Pickup, 0.6);
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
                let _ = records.attempt(
                    courses::COURSES[course].id,
                    record_context,
                    match courses::COURSES[course].medal(game.elapsed, game.falls) {
                        "GOLD" => 3,
                        "SILVER" => 2,
                        _ => 1,
                    },
                    ferravex_render::storage::run_micros(game.elapsed),
                );
                sound.cue(Cue::Success, 0.65);
            }
        }
        let position = game.previous_position.lerp(game.position(), frame.alpha);
        let rotation = game.previous_rotation.slerp(game.rotation(), frame.alpha);
        if game.falls != previous_falls {
            camera.cut();
            composition = course_composition(&game.level);
        }
        previous_falls = game.falls;
        let mut camera_input = follow_input(position);
        camera_input.target.velocity = game.physics.bodies[game.marble].linvel();
        let camera_input = composition
            .update(camera_input, Vec3::ZERO, demo.dt())
            .unwrap();
        let view = camera
            .update(demo.dt(), camera_input, &NoObstructions)
            .expect("valid rolling camera")
            .pose;
        clear_background(SKY);
        let overview = player_input.held("overview");
        let mut lo = game.level.spawn;
        let mut hi = lo;
        for p in &game.level.platforms {
            lo = lo.min(p.center);
            hi = hi.max(p.center);
        }
        let course_center = (lo + hi) * 0.5;
        let course_span =
            (hi - lo).max_element().max(20.) / (screen_width() / screen_height()).min(1.);
        let camera_position = if overview {
            course_center + vec3(course_span * 0.6, course_span, course_span * 0.8)
        } else {
            view.position
        };
        let target = if overview { course_center } else { view.target };
        set_camera(&Camera3D {
            position: camera_position,
            target,
            up: if overview { Vec3::Y } else { view.up },
            fovy: view.fov_y,
            z_near: 0.1,
            z_far: 220.,
            ..Default::default()
        });
        material.begin(LitSettings {
            camera_position,
            fog_color: SKY,
            fog_start: 38.,
            fog_end: 115.,
            light_direction: vec3(-0.55, 0.85, 0.35),
            ambient: 0.48,
        });
        let visual_time = (game.elapsed - frame.dt * (1. - frame.alpha)).max(0.);
        scene.draw(&game, position, rotation, visual_time);
        material.end();
        set_default_camera();
        let compact = screen_width() < 560.;
        text(
            courses::COURSES[course].name,
            if compact { 16. } else { 27. },
            if compact { 29. } else { 39. },
            if compact { 20. } else { 26. },
            INK,
        );
        if screen_width() > 620. {
            text(courses::COURSES[course].challenge, 28., 60., 12., INK);
        }
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
        let status_y = if compact { 55. } else { 39. };
        text(&status, status_x, status_y, status_size as f32, INK);
        draw_poly(status_x - 16., status_y - 8., 4, 6., 0., GOLD);
        if game.notice > 0. && !game.finished {
            centered(
                if game.recovered {
                    "BACK ON TRACK"
                } else if game.checkpoint > 0 {
                    "CHECKPOINT SAVED"
                } else {
                    "BACK ON TRACK"
                },
                89.,
                17,
                INK,
            );
        }
        if touch_mode && !player_input.touch_customized() && !game.finished && !picker {
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
        } else if game.elapsed < 10. && !picker && !game.finished {
            centered(
                if screen_width() > 700. {
                    "WASD roll / SPACE brake / TAB view / V recover / C courses"
                } else {
                    "Arrows roll / Space brake / C courses"
                },
                screen_height() - 27.,
                if compact { 12 } else { 17 },
                INK,
            );
        }
        if let Some(command) = course_ui::draw(
            &mut course_ui,
            &mut player_input,
            &game,
            course,
            picker,
            &records,
            record_context,
        ) {
            use course_ui::Command;
            let selected = match command {
                Command::Select(index) => Some(index),
                Command::Next if course + 1 < courses::COURSES.len() => Some(course + 1),
                Command::Retry => Some(course),
                Command::Next => {
                    picker = true;
                    player_input.set_paused(true);
                    None
                }
                Command::Toggle => {
                    picker = !picker;
                    player_input.set_paused(picker);
                    None
                }
                Command::Recover => {
                    game.recover();
                    clock.reset();
                    camera.cut();
                    composition = course_composition(&game.level);
                    sound.cue(Cue::Failure, 0.3);
                    None
                }
            };
            // Keep the current frame's resident meshes alive through GPU submission.
            pending_course = selected;
        }
        if (!picker && !game.finished) || player_input.controls_open() {
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
    use ferravex_engine::camera::RigidObstructions;
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
