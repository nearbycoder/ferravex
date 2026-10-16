//! A compact low-poly hover time trial built on Ferravex's fixed clock and lit renderer.
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{
    PlayerInput, Preset,
    touch::{Control, Output, Region},
};
mod race;

use ferravex_core::FixedClock;
use ferravex_play::camera::{
    Blocker, Input3, Mode3, ObstructionSettings, Rig3, Settings3, ShakeImpulse, Target3,
};
use ferravex_play::particles::{Emitter, EmitterTransform, ParticlePool, presets};
use ferravex_render::effects::{BillboardView, ParticleRenderer, ParticleStyle};
use ferravex_render::{
    demo::Demo,
    material::{MaterialOptions, ShaderMaterial, ShaderSource},
    mesh::Mesh3d,
    prelude::*,
    shaders::{LitMaterial, LitSettings},
    ui,
    visibility::{LodChain, LodState},
};
use race::{
    Course, GATES, HALF_WIDTH, HOVER_HEIGHT, Input, LAPS, Race, heading, path, right, tangent,
    world,
};
use std::f32::consts::{PI, TAU};

const SKY: Color = Color::from_rgba(181, 201, 195, 255);
const SAND: Color = Color::from_rgba(194, 154, 106, 255);
const STONE: Color = Color::from_rgba(170, 116, 74, 255);
const PALE: Color = Color::from_rgba(238, 219, 179, 255);
const TEAL: Color = Color::from_rgba(56, 159, 151, 255);
const DARK: Color = Color::from_rgba(35, 55, 55, 255);
const ORANGE: Color = Color::from_rgba(239, 125, 58, 255);

fn conf() -> Conf {
    let mut conf = ferravex_render::window_conf("Dune Skimmer / Ferravex");
    conf.window_width = 1280;
    conf.window_height = 800;
    conf
}

struct Prop {
    mesh: usize,
    position: Vec3,
    rotation: Quat,
}
struct Scene {
    exhaust: ShaderMaterial,
    ground: Mesh3d,
    road: Mesh3d,
    curbs: Mesh3d,
    props: Vec<Prop>,
    prop_meshes: Vec<Mesh3d>,
    rock_lods: Vec<LodChain>,
    rock_lod_states: std::cell::RefCell<Vec<LodState>>,
    pillar: Mesh3d,
    lintel: Mesh3d,
    cap: Mesh3d,
    marker: Mesh3d,
    active_marker: Mesh3d,
    arrow: Mesh3d,
    hull: Mesh3d,
    nose: Mesh3d,
    cockpit: Mesh3d,
    rail: Mesh3d,
    engine: Mesh3d,
    glow: Mesh3d,
    stripe: Mesh3d,
    shadow: Mesh3d,
    sun: Mesh3d,
}

/// Build one indexed flat ribbon; rendering never reconstructs road geometry.
fn ribbon(inner: f32, outer: f32, height: f32, colors: [Color; 2]) -> Mesh3d {
    let mut vertices = Vec::with_capacity(192 * 4);
    let mut indices = Vec::with_capacity(192 * 6);
    for i in 0..192 {
        let a = i as f32 / 192.0 * TAU;
        let b = (i + 1) as f32 / 192.0 * TAU;
        let color = colors[(i / 3) % 2];
        let base = vertices.len() as u16;
        for (t, side) in [(a, inner), (a, outer), (b, outer), (b, inner)] {
            let mut vertex = Vertex::new2(
                world(path(t) + right(tangent(t)) * side, height),
                Vec2::ZERO,
                color,
            );
            vertex.normal = Vec3::Y.extend(0.0);
            vertices.push(vertex);
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Mesh3d::from_mesh(Mesh {
        vertices,
        indices,
        texture: None,
    })
}

fn merge_meshes(first: Mesh3d, second: Mesh3d) -> Mesh3d {
    let mut mesh = first.mesh;
    let offset = mesh.vertices.len() as u16;
    mesh.vertices.extend(second.mesh.vertices);
    mesh.indices
        .extend(second.mesh.indices.into_iter().map(|i| i + offset));
    Mesh3d::from_mesh(mesh)
}

impl Scene {
    fn new() -> Self {
        let mut props = Vec::new();
        // Wind-shaped dunes, flat-topped sandstone mesas, and collapsed columns.
        // Deterministic placement stays clear of the racing surface.
        let prop_meshes = vec![
            Mesh3d::frustum(16.0, 7.0, 15.0, STONE, 6),
            Mesh3d::frustum(11.0, 6.0, 8.0, Color::from_rgba(193, 139, 88, 255), 5),
            Mesh3d::cone(21.0, 7.0, SAND, 7),
            Mesh3d::cuboid(vec3(2.8, 7.0, 2.8), STONE),
            Mesh3d::cuboid(vec3(9.0, 1.8, 2.8), PALE),
            Mesh3d::frustum(3.0, 1.8, 3.0, Color::from_rgba(158, 107, 69, 255), 5),
        ];
        for i in 0..33 {
            let angle = i as f32 / 33.0 * TAU;
            let radius = 100.0 + (i % 4) as f32 * 10.0;
            let mesh = i % 3;
            let height = [7.1, 3.6, 3.1][mesh];
            props.push(Prop {
                mesh,
                position: vec3(angle.cos() * radius, height, angle.sin() * radius),
                rotation: Quat::from_rotation_y(angle),
            });
        }
        for i in 0..12 {
            let angle = (i as f32 + 0.3) / 12.0 * TAU;
            let p = path(angle) - right(tangent(angle)) * (18.0 + (i % 3) as f32 * 2.0);
            props.push(Prop {
                mesh: 3,
                position: world(p, 3.3),
                rotation: Quat::from_rotation_y(angle),
            });
            if i % 3 == 0 {
                props.push(Prop {
                    mesh: 3,
                    position: world(p + tangent(angle) * 6.0, 3.3),
                    rotation: Quat::from_rotation_y(angle),
                });
                props.push(Prop {
                    mesh: 4,
                    position: world(p + tangent(angle) * 3.0, 7.0),
                    rotation: Quat::from_rotation_y(heading(tangent(angle)) + PI * 0.5),
                });
            }
        }
        for i in 0..25 {
            let angle = i as f32 * 2.399_963;
            let radius = 11.0 + (i % 6) as f32 * 2.5;
            props.push(Prop {
                mesh: if i % 4 == 0 { 1 } else { 5 },
                position: vec3(
                    angle.cos() * radius,
                    if i % 4 == 0 { 3.6 } else { 1.3 },
                    angle.sin() * radius * 0.7,
                ),
                rotation: Quat::from_rotation_y(angle),
            });
        }
        Self {
            exhaust: ShaderMaterial::new(
                ShaderSource {
                    vertex: include_str!("../shaders/exhaust.vert").into(),
                    fragment: include_str!("../shaders/exhaust.frag").into(),
                },
                MaterialOptions {
                    blend: ferravex_render::material::BlendMode::Opaque,
                    bounds_padding: 0.1,
                    casts_shadows: false,
                    receives_shadows: false,
                },
            )
            .expect("exhaust material"),
            ground: Mesh3d::plane(vec2(440.0, 440.0), SAND, 22),
            road: ribbon(
                -HALF_WIDTH,
                HALF_WIDTH,
                0.012,
                [
                    Color::from_rgba(170, 140, 101, 255),
                    Color::from_rgba(174, 143, 104, 255),
                ],
            ),
            curbs: merge_meshes(
                ribbon(-HALF_WIDTH - 0.30, -HALF_WIDTH, 0.055, [PALE, STONE]),
                ribbon(HALF_WIDTH, HALF_WIDTH + 0.30, 0.055, [PALE, STONE]),
            ),
            props,
            prop_meshes,
            rock_lods: (0..3)
                .map(|i| {
                    let radius = 1.1 + i as f32 * 0.24;
                    LodChain::new(
                        vec![
                            Mesh3d::sphere(radius, STONE, 6, 4).resident().clone(),
                            Mesh3d::sphere(radius, STONE, 4, 3).resident().clone(),
                            Mesh3d::sphere(radius, STONE, 3, 2).resident().clone(),
                        ],
                        vec![35., 75.],
                        0.12,
                    )
                    .unwrap()
                })
                .collect(),
            rock_lod_states: Default::default(),
            pillar: Mesh3d::cuboid(vec3(0.75, 7.6, 1.15), STONE),
            lintel: Mesh3d::cuboid(vec3(HALF_WIDTH * 2.0 + 1.5, 0.7, 1.15), PALE),
            cap: Mesh3d::cuboid(vec3(1.1, 0.5, 1.5), PALE),
            marker: Mesh3d::cuboid(vec3(0.8, 1.0, 1.2), ORANGE),
            active_marker: Mesh3d::cuboid(vec3(0.82, 1.0, 1.22), TEAL),
            arrow: Mesh3d::cone(0.65, 1.1, TEAL, 3),
            hull: Mesh3d::cuboid(vec3(1.8, 0.42, 3.2), TEAL),
            nose: Mesh3d::cone(1.05, 2.3, TEAL, 4),
            cockpit: Mesh3d::cuboid(vec3(1.05, 0.46, 1.3), DARK),
            rail: Mesh3d::cuboid(vec3(0.3, 0.35, 3.8), PALE),
            engine: Mesh3d::cylinder(0.42, 1.5, DARK, 8),
            glow: Mesh3d::cone(0.29, 1.25, ORANGE, 6),
            stripe: Mesh3d::cuboid(vec3(0.18, 0.015, 2.6), ORANGE),
            shadow: Mesh3d::cylinder(1.0, 0.006, Color::from_rgba(114, 100, 77, 255), 8),
            sun: Mesh3d::sphere(6.0, Color::from_rgba(255, 226, 166, 255), 12, 8),
        }
    }
    fn camera_blockers(&self, course: &Course) -> Vec<Blocker> {
        let mut blockers = vec![Blocker::Plane {
            normal: Vec3::Y,
            offset: 0.,
        }];
        for prop in &self.props {
            let bounds = self.prop_meshes[prop.mesh].resident().bounds();
            blockers.push(Blocker::Box {
                center: prop.position + prop.rotation * bounds.center(),
                half: bounds.half_extents(),
                rotation: prop.rotation,
            });
        }
        blockers.extend(course.rocks.iter().map(|&(p, radius)| Blocker::Sphere {
            center: world(p, 0.65),
            radius,
        }));
        for gate in &course.gates {
            let rotation = Quat::from_rotation_y(heading(gate.direction));
            for side in [-1., 1.] {
                blockers.push(Blocker::Box {
                    center: world(
                        gate.position + right(gate.direction) * (HALF_WIDTH + 0.4) * side,
                        3.7,
                    ),
                    half: vec3(0.375, 3.8, 0.575),
                    rotation,
                });
            }
            blockers.push(Blocker::Box {
                center: world(gate.position, 7.55),
                half: vec3(HALF_WIDTH + 0.75, 0.35, 0.575),
                rotation,
            });
        }
        blockers
    }
    fn draw(&self, course: &Course, race: &Race, position: Vec2, yaw: f32, time: f32) {
        use ferravex_render::lighting::{PointLight, SunShadow};
        ferravex_render::set_sun_shadow(Some(SunShadow {
            center: world(position, 0.),
            direction: vec3(-0.45, 0.9, -0.4),
            extent: 38.,
            depth: 100.,
            ..Default::default()
        }))
        .unwrap();
        let engine_light = if race.speed() > 1. { 1.4 } else { 0. };
        ferravex_render::set_point_lights(&[PointLight {
            position: world(position - race.forward() * 1.8, HOVER_HEIGHT),
            color: ORANGE,
            radius: 5.,
            intensity: engine_light,
        }])
        .unwrap();

        self.ground.draw(vec3(0.0, -0.03, 0.0), Quat::IDENTITY);
        self.road.draw(Vec3::ZERO, Quat::IDENTITY);
        self.curbs.draw(Vec3::ZERO, Quat::IDENTITY);
        self.sun.draw(vec3(-95.0, 75.0, -130.0), Quat::IDENTITY);
        for prop in &self.props {
            self.prop_meshes[prop.mesh].draw(prop.position, prop.rotation);
        }
        {
            let mut states = self.rock_lod_states.borrow_mut();
            states.resize(course.rocks.len(), LodState::default());
            for (index, &(p, _)) in course.rocks.iter().enumerate() {
                ferravex_render::draw_lod(
                    &self.rock_lods[index % 3],
                    Mat4::from_rotation_translation(
                        Quat::from_rotation_y(index as f32 * 1.7),
                        world(p, 0.65),
                    ),
                    &mut states[index],
                );
            }
        }
        for (index, gate) in course.gates.iter().enumerate() {
            let rotation = Quat::from_rotation_y(heading(gate.direction));
            for side in [-1.0, 1.0] {
                let p = gate.position + right(gate.direction) * (HALF_WIDTH + 0.4) * side;
                self.pillar.draw(world(p, 3.7), rotation);
                self.cap.draw(world(p, 0.4), rotation);
                if index == race.next_gate {
                    self.active_marker.draw(world(p, 4.7), rotation);
                } else {
                    self.marker.draw(world(p, 4.7), rotation);
                }
            }
            self.lintel.draw(world(gate.position, 7.55), rotation);
            if index == race.next_gate && !race.finished {
                self.arrow.draw(
                    world(gate.position, 4.7 + time.sin() * 0.18),
                    Quat::from_rotation_z(PI),
                );
            }
        }
        let forward = vec2(yaw.sin(), yaw.cos());
        let hover = HOVER_HEIGHT + (time * 5.0).sin() * 0.07;
        let center = world(position, hover);
        let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(race.bank);
        let draw = |mesh: &Mesh3d, offset: Vec3, local: Quat| {
            mesh.draw(center + rotation * offset, rotation * local);
        };
        if ferravex_render::shadow_quality() == ferravex_render::lighting::Quality::Low {
            for z in [-0.8, 0.8] {
                self.shadow
                    .draw(world(position + forward * z, 0.022), Quat::IDENTITY);
            }
        }
        draw(&self.hull, Vec3::ZERO, Quat::IDENTITY);
        draw(
            &self.nose,
            vec3(0.0, -0.03, 1.9),
            Quat::from_rotation_x(PI * 0.5),
        );
        draw(
            &self.cockpit,
            vec3(0.0, 0.40, -0.3),
            Quat::from_rotation_x(-0.08),
        );
        draw(&self.stripe, vec3(0.0, 0.22, 0.1), Quat::IDENTITY);
        for x in [-1.12, 1.12] {
            draw(&self.rail, vec3(x, -0.14, -0.1), Quat::IDENTITY);
            draw(
                &self.engine,
                vec3(x, -0.05, -1.2),
                Quat::from_rotation_x(PI * 0.5),
            );
            if race.speed() > 1.0 {
                self.exhaust
                    .begin([[time, 0., 0., 0.], [0.; 4], [0.; 4], [0.; 4]])
                    .unwrap();
                draw(
                    &self.glow,
                    vec3(x, -0.05, -2.0 - if race.boosting { 0.45 } else { 0.0 }),
                    Quat::from_rotation_x(-PI * 0.5),
                );
                self.exhaust.end();
            }
        }
    }
}

fn label(text: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(
        text,
        x + 1.0,
        y + 2.0,
        size,
        Color::new(0.1, 0.14, 0.13, 0.5),
    );
    draw_text(text, x, y, size, color);
}
fn centered(text: &str, y: f32, size: u16, color: Color) {
    let width = measure_text(text, None, size, 1.0).width;
    label(text, (screen_width() - width) * 0.5, y, size as f32, color);
}
fn time_label(time: f32) -> String {
    format!("{:02}:{:05.2}", (time / 60.0) as u32, time % 60.0)
}
fn draw_hud(race: &Race, touch_mode: bool) {
    let w = screen_width();
    let h = screen_height();
    let margin = (w * 0.035).clamp(18.0, 44.0);
    label("DUNE SKIMMER", margin, 39.0, 22.0, PALE);
    label(
        &format!("LAP {:02} / 03", (race.laps + 1).min(LAPS)),
        margin,
        66.0,
        18.0,
        WHITE,
    );
    let timer = time_label(race.elapsed);
    let width = measure_text(&timer, None, 28, 1.0).width;
    label(
        &timer,
        (w - width) * 0.5,
        if w < 600.0 { 97.0 } else { 42.0 },
        28.0,
        WHITE,
    );
    let bottom = if touch_mode { h - 140.0 } else { h - 50.0 };
    let speed = format!("{:03}", (race.speed() * 3.6) as u32);
    let speed_width = measure_text(&speed, None, 47, 1.).width;
    label(&speed, margin, bottom, 47.0, WHITE);
    label("KM/H", margin + speed_width + 10., bottom, 14.0, PALE);
    let bar_w = 136.0;
    draw_rectangle(
        w - margin - bar_w,
        bottom - 16.0,
        bar_w,
        4.0,
        Color::new(0.1, 0.2, 0.19, 0.45),
    );
    draw_rectangle(
        w - margin - bar_w,
        bottom - 16.0,
        bar_w * race.boost,
        4.0,
        if race.boosting { ORANGE } else { TEAL },
    );
    label("BOOST", w - margin - bar_w, bottom + 8.0, 13.0, PALE);
    if race.countdown > 0.0 {
        centered("THREE LAPS THROUGH THE RUINS", h * 0.34, 20, PALE);
        centered(
            &format!("{}", race.countdown.ceil() as u32),
            h * 0.48,
            74,
            WHITE,
        );
    } else if race.elapsed < 0.8 {
        centered("GO", h * 0.42, 54, TEAL);
    }
    if race.off_track && !race.finished {
        centered("LOOSE SAND", h - 27.0, 16, PALE);
    } else if race.elapsed < 10.0 && !touch_mode {
        centered(
            "WASD / ARROWS  steer & accelerate     SPACE  boost     R  restart",
            h - 19.0,
            16,
            PALE,
        );
    }
    if race.last_gate_timer > 0.0 && race.countdown == 0.0 && !race.finished {
        let text = if race.next_gate == 1 {
            "LAP COMPLETE".to_owned()
        } else {
            format!(
                "GATE {:02} / {GATES}",
                if race.next_gate == 0 {
                    GATES
                } else {
                    race.next_gate
                }
            )
        };
        centered(&text, 67.0, 17, PALE);
    }
    if race.finished {
        let panel = Rect::new(w * 0.5 - 180.0, h * 0.5 - 113.0, 360.0, 220.0);
        draw_rectangle(
            panel.x,
            panel.y,
            panel.w,
            panel.h,
            Color::new(0.10, 0.19, 0.18, 0.94),
        );
        centered("CIRCUIT COMPLETE", h * 0.5 - 65.0, 22, PALE);
        centered(&time_label(race.elapsed), h * 0.5 - 6.0, 49, WHITE);
        centered(
            &format!("3 laps  /  {} impacts", race.impacts),
            h * 0.5 + 28.0,
            18,
            PALE,
        );
        centered("R  /  TAP TO RACE AGAIN", h * 0.5 + 76.0, 16, TEAL);
    }
}
fn touch_controls() -> [Rect; 4] {
    let h = screen_height();
    let w = screen_width();
    let size = (w * 0.14).clamp(48.0, 72.0);
    [
        Rect::new(18.0, h - size - 20.0, size, size),
        Rect::new(size + 32.0, h - size - 20.0, size, size),
        Rect::new(w - size * 2.0 - 32.0, h - size - 20.0, size, size),
        Rect::new(w - size - 18.0, h - size - 20.0, size, size),
    ]
}
ferravex_render::game!(conf, game);
async fn game() {
    let course = Course::new();
    let scene = Scene::new();
    let material = LitMaterial::new().expect("Ferravex's portable lit material should compile");
    let mut race = Race::new();
    let mut clock = FixedClock::new(120.0, 16);
    let mut demo = Demo::new();
    let mut sound = GameAudio::new();
    let mut dust = ParticlePool::new(256);
    let mut dust_emitter =
        Emitter::new(presets::dust([SAND.r, SAND.g, SAND.b, 0.32]), 0x44554e45).unwrap();
    let mut dust_renderer = ParticleRenderer::new(256).unwrap();
    ferravex_render::set_quality(ferravex_render::lighting::Quality::Medium);
    let mut player_input = PlayerInput::preset("dune-skimmer", Preset::HoverRacer);
    player_input
        .add_default_binding(
            "move_forward",
            ferravex_render::input::Binding::digital(ferravex_render::input::InputCode::Touch(7)),
        )
        .unwrap();
    let mut camera = Rig3::new(Settings3 {
        position_half_life: std::f32::consts::LN_2 / 5.5,
        target_half_life: std::f32::consts::LN_2 / 8.,
        obstruction: Some(ObstructionSettings {
            radius: 0.23,
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    let blockers = scene.camera_blockers(&course);
    let mut previous_impacts = 0;
    let mut previous_recoveries = 0;
    loop {
        ferravex_render::ui::warm_up_font_sizes(&[13, 14, 16, 17, 18, 20, 22, 28, 47, 49, 54, 74]);
        ui::warm_up_font();
        let rects = touch_controls();
        let layout = std::array::from_fn::<_, 4, _>(|i| Control {
            region: Region::Rect(rects[i]),
            output: Output::Button([12, 13, 14, 7][i]),
        });
        player_input.set_touch_layout(&layout).unwrap();
        player_input.sample_play();
        let touch_mode = player_input.touch_seen();
        if player_input.pressed("restart")
            || (race.finished
                && (player_input.pressed("confirm")
                    || is_mouse_button_pressed(MouseButton::Left)
                    || ferravex_render::input::logical_touches()
                        .iter()
                        .any(|t| t.phase == TouchPhase::Started)))
        {
            race = Race::new();
            dust.clear();
            dust_emitter.reset(0x44554e45);
            clock.reset();
            camera.cut();
            previous_impacts = 0;
            previous_recoveries = 0;
        }
        let movement = player_input.movement();
        let input = Input {
            throttle: movement.y,
            steer: movement.x,
            boost: player_input.held("boost"),
        };
        let dt = if demo.autoplay {
            demo.dt()
        } else {
            player_input.simulation_dt(demo.dt())
        };
        let frame = clock.advance(dt as f64);
        for _ in 0..frame.steps {
            let laps = race.laps;
            let finished = race.finished;
            race.step(
                &course,
                if demo.autoplay {
                    race.autoplay(&course)
                } else {
                    input
                },
                frame.dt,
            );
            if race.laps > laps {
                sound.cue(Cue::Pickup, 0.5);
            }
            if race.finished && !finished {
                sound.cue(Cue::Success, 0.65);
            }
        }
        sound.motor(
            race.speed(),
            48.,
            !race.finished && (demo.autoplay || player_input.gameplay_active()),
        );
        let position = race.previous_position.lerp(race.position, frame.alpha);
        let yaw =
            race.previous_yaw + race::angle_difference(race.yaw, race.previous_yaw) * frame.alpha;
        if race.recovery_count != previous_recoveries {
            camera.cut();
            dust.clear();
            dust_emitter
                .teleport(EmitterTransform {
                    position: world(position - vec2(yaw.sin(), yaw.cos()) * 2., 0.25).to_array(),
                    rotation: Quat::from_rotation_y(yaw).to_array(),
                })
                .unwrap();
        }
        if race.impacts > previous_impacts {
            sound.cue(Cue::Impact(Surface::Metal), 0.7);
            camera
                .shake
                .push(ShakeImpulse {
                    translation: vec3(0.025, 0.015, 0.),
                    rotation: vec3(0.006, 0.004, 0.018),
                    duration: 0.22,
                    frequency: 24.,
                })
                .expect("impact cooldown bounds shake layers");
        }
        previous_impacts = race.impacts;
        previous_recoveries = race.recovery_count;
        dust.update(dt).unwrap();
        let density = match ferravex_render::quality() {
            ferravex_render::lighting::Quality::Low => 0.5,
            ferravex_render::lighting::Quality::Medium => 1.,
            ferravex_render::lighting::Quality::High => 1.5,
        };
        dust_emitter
            .set_rate(if race.speed() > 3. && !race.finished {
                32. * density * if race.boosting { 1.5 } else { 1. }
            } else {
                0.
            })
            .unwrap();
        dust_emitter
            .update(
                dt,
                EmitterTransform {
                    position: world(position - vec2(yaw.sin(), yaw.cos()) * 2., 0.25).to_array(),
                    rotation: Quat::from_rotation_y(yaw).to_array(),
                },
                &mut dust,
            )
            .unwrap();
        let view = camera
            .update(
                dt,
                Input3 {
                    target: Target3 {
                        position: world(position, HOVER_HEIGHT),
                        rotation: Quat::from_rotation_y(yaw),
                        velocity: world(race.velocity, 0.),
                    },
                    mode: Mode3::Chase {
                        distance: 10.7,
                        height: 4.8 - HOVER_HEIGHT,
                        look_ahead: 6.,
                        look_height: 1.6 - HOVER_HEIGHT,
                        speed_distance: 0.035,
                        speed_height: 0.015,
                        prediction: 0.,
                        level: true,
                    },
                    fov_y: if race.boosting {
                        54_f32.to_radians()
                    } else {
                        51_f32.to_radians()
                    },
                },
                &blockers[..],
            )
            .expect("valid hover camera")
            .pose;
        let camera_position = view.position;
        clear_background(SKY);
        set_camera(&Camera3D {
            position: camera_position,
            target: view.target,
            up: view.up,
            fovy: view.fov_y,
            z_near: 0.12,
            z_far: 370.0,
            ..Default::default()
        });
        material.begin(LitSettings {
            camera_position,
            light_direction: vec3(-0.45, 0.9, -0.4),
            fog_color: SKY,
            fog_start: 75.0,
            fog_end: 245.0,
            ambient: 0.62,
        });
        scene.draw(&course, &race, position, yaw, demo.time());
        material.end();
        dust_renderer
            .draw_3d(
                &dust,
                BillboardView::new(
                    camera_position,
                    (view.target - camera_position).cross(view.up),
                    view.up,
                )
                .unwrap(),
                ParticleStyle::default(),
            )
            .unwrap();
        set_default_camera();
        draw_hud(&race, touch_mode);
        if touch_mode && !player_input.touch_customized() && !race.finished {
            for (rect, text) in touch_controls().into_iter().zip(["<", ">", "GO", "BOOST"]) {
                draw_rectangle(
                    rect.x,
                    rect.y,
                    rect.w,
                    rect.h,
                    Color::new(0.1, 0.19, 0.18, 0.50),
                );
                let text_width = measure_text(text, None, 18, 1.0).width;
                label(
                    text,
                    rect.x + (rect.w - text_width) * 0.5,
                    rect.y + rect.h * 0.5 + 6.0,
                    18.0,
                    PALE,
                );
            }
        }
        player_input.draw_settings();
        if demo.finish_frame() {
            println!(
                "DUNE SKIMMER laps={} next_gate={} time={:.2} finished={} impacts={} recoveries={} finite={}",
                race.laps,
                race.next_gate,
                race.elapsed,
                race.finished,
                race.impacts,
                race.recovery_count,
                race.position.is_finite()
            );
            break;
        }
        next_frame().await;
    }
}
