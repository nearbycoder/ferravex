//! A low-poly hover time trial with a 120 Hz [`ferravex_core::session::GameSession`].
//!
//! The host starts in the countdown, pauses without spending race time, and freezes
//! the result after the final gate. Its lifecycle uses the shared session contract:
//!
//! ```
//! use ferravex_core::session::{GameSession, SessionState};
//! let mut session = GameSession::new(120.0, 16, true);
//! assert_eq!(session.advance(1.0 / 60.0).steps, 2);
//! session.set_paused(true);
//! assert_eq!(session.advance(60.0).steps, 0);
//! session.restart();
//! assert_eq!(session.tick(), 0);
//! session.finish(true);
//! assert_eq!(session.state(), SessionState::Won);
//! ```
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{
    PlayerInput, Preset,
    touch::{Control, Output, Region},
};
mod presentation;
mod race;
mod session;

#[cfg(test)]
mod journey200_tests;

use ferravex_play::camera::{Blocker, ObstructionSettings, Rig3, Settings3};
use ferravex_play::particles::{Emitter, EmitterTransform, ParticlePool, presets};
use ferravex_render::effects::{BillboardView, ParticleRenderer, ParticleStyle};
use ferravex_render::{
    demo::Demo,
    material::{MaterialOptions, ShaderMaterial},
    mesh::Mesh3d,
    prelude::*,
    shaders::{LitMaterial, LitSettings},
    ui,
    visibility::{LodChain, LodState},
};
use presentation::{
    Presentation, binding_hint, chase_input, exhaust_source, fitted_size, text_width,
};
use race::{
    BoostStatus, Course, GATES, HALF_WIDTH, HOVER_HEIGHT, Input, LAPS, RECOVERY_PENALTY, Race,
    Traction, heading, path, right, tangent, world,
};
use session::RaceSession;
use std::f32::consts::{PI, TAU};

const SKY: Color = Color::from_rgba(181, 201, 195, 255);
const SAND: Color = Color::from_rgba(194, 154, 106, 255);
const STONE: Color = Color::from_rgba(170, 116, 74, 255);
const PALE: Color = Color::from_rgba(238, 219, 179, 255);
const TEAL: Color = Color::from_rgba(56, 159, 151, 255);
const DARK: Color = Color::from_rgba(35, 55, 55, 255);
const ORANGE: Color = Color::from_rgba(239, 125, 58, 255);
const DUST_SEED: u64 = 0x44554e45;
const SHORT_START_ROW: (f32, u16) = (77., 18);

fn reset_drive_feedback(camera: &mut Rig3, dust: &mut ParticlePool, emitter: &mut Emitter) {
    camera.cut();
    dust.clear();
    emitter.reset(DUST_SEED);
}

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
                exhaust_source(),
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
    fn draw(&self, course: &Course, race: &Race, position: Vec2, yaw: f32, effects: &Presentation) {
        let time = effects.motion_time;
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
        let bank = if effects.policy.reduced_motion() {
            0.
        } else {
            race.bank
        };
        let rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_z(bank);
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
                self.exhaust.begin(effects.exhaust_parameters()).unwrap();
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
    centered_in(text, y, size, color, screen_width() - 32.);
}
fn centered_in(text: &str, y: f32, size: u16, color: Color, available: f32) {
    let fitted = fitted_size(text, available, size);
    let width = text_width(text, size) * fitted / size as f32;
    label(text, (screen_width() - width) * 0.5, y, fitted, color);
}
fn time_label(time: f32) -> String {
    let centiseconds = (time.max(0.) * 100.).round() as u64;
    format!(
        "{:02}:{:02}.{:02}",
        centiseconds / 6000,
        centiseconds / 100 % 60,
        centiseconds % 100
    )
}
fn draw_hud(race: &Race, input: &PlayerInput, waiting_for_controls: bool) {
    let touch_mode = input.touch_seen();
    let w = screen_width();
    let h = screen_height();
    let margin = (w * 0.035).clamp(18.0, 44.0);
    let compact = w < 600.;
    if race.finished {
        let panel_w = (w - 32.0).clamp(1.0, 360.0);
        let panel = Rect::new((w - panel_w) * 0.5, h * 0.5 - 130.0, panel_w, 265.0);
        draw_rectangle(
            panel.x,
            panel.y,
            panel.w,
            panel.h,
            Color::new(0.10, 0.19, 0.18, 0.94),
        );
        let centered =
            |text: &str, y, size, color| centered_in(text, y, size, color, panel.w - 24.);
        centered("CIRCUIT COMPLETE", h * 0.5 - 94.0, 22, PALE);
        centered("FINAL TIME / PENALTIES INCLUDED", h * 0.5 - 65.0, 13, PALE);
        centered(&time_label(race.elapsed), h * 0.5 - 20.0, 49, WHITE);
        centered(
            &format!(
                "DRIVING {} + {:.2}s RECOVERY",
                time_label(race.driving_seconds()),
                race.penalty_seconds()
            ),
            h * 0.5 + 10.,
            14,
            PALE,
        );
        centered(
            &format!(
                "3 laps / {} impacts / {} recoveries",
                race.impacts, race.recovery_count
            ),
            h * 0.5 + 39.0,
            16,
            PALE,
        );
        centered(
            "IMPACTS SCRUB SPEED / NO TIME ADDED",
            h * 0.5 + 62.,
            12,
            PALE,
        );
        let mut retry_buffer = [0; 256];
        let mut confirm_buffer = [0; 256];
        let retry = binding_hint(input, "restart", &mut retry_buffer);
        let confirm = binding_hint(input, "confirm", &mut confirm_buffer);
        centered(
            &format!("RETRY {retry} / {confirm}"),
            h * 0.5 + 92.0,
            15,
            TEAL,
        );
        centered("OR CLICK / TAP THE GAME", h * 0.5 + 115., 12, PALE);
        return;
    }
    label(
        &format!("LAP {}/3", (race.laps + 1).min(LAPS)),
        margin,
        if compact { 28. } else { 66.0 },
        if compact { 14. } else { 18.0 },
        WHITE,
    );
    let timer = time_label(race.elapsed);
    let width = measure_text(&timer, None, 20, 1.0).width;
    label(
        &timer,
        if compact {
            w - 80. - width
        } else {
            (w - width) * 0.5
        },
        28.0,
        20.0,
        WHITE,
    );
    let completed = race.gates_completed();
    let total = GATES * LAPS as usize;
    let short_start = compact && h < 460. && (race.countdown > 0. || race.elapsed < 0.8);
    let next = format!("NEXT GATE {:02} / {GATES}", completed % GATES + 1);
    if short_start {
        // Reuse the next-gate row before driving starts. A separate center cue
        // would overlap boost telemetry at the 352px minimum in touch mode.
        let text = if race.countdown > 0. {
            format!("THREE LAPS / {}", race.countdown.ceil() as u32)
        } else {
            "GO".to_owned()
        };
        centered(&text, SHORT_START_ROW.0, SHORT_START_ROW.1, TEAL);
    } else if compact {
        centered(&next, 54., 14, TEAL);
    } else {
        label(
            &next,
            w - margin - 64. - text_width(&next, 14),
            39.,
            14.,
            TEAL,
        );
    }
    let progress_y = if compact {
        if short_start { 100. } else { 68. }
    } else if race.recovery_count > 0 {
        109.0
    } else {
        83.0
    };
    if race.recovery_count > 0 {
        centered(
            &format!("TOTAL INCLUDES +{:.2}s RECOVERY", race.penalty_seconds()),
            if compact { 130.0 } else { 92.0 },
            13,
            ORANGE,
        );
    }
    let progress_w = (w - margin * 2.0).max(1.0);
    draw_rectangle(margin, progress_y, progress_w, 4.0, DARK);
    draw_rectangle(
        margin,
        progress_y,
        progress_w * completed as f32 / total as f32,
        4.0,
        TEAL,
    );
    let bottom = if touch_mode { h - 150.0 } else { h - 75.0 };
    let speed = format!("{:03}", (race.speed() * 3.6) as u32);
    let speed_size = if w < 400.0 { 36 } else { 47 };
    let speed_width = measure_text(&speed, None, speed_size, 1.).width;
    label(&speed, margin, bottom, speed_size as f32, WHITE);
    label("KM/H", margin + speed_width + 7., bottom, 13.0, PALE);
    let (traction, traction_color) = match race.traction() {
        Traction::Track => ("", PALE),
        Traction::Sliding => ("SLIDING", ORANGE),
        Traction::Reversing => ("REVERSING", PALE),
        Traction::Sand => ("LOOSE SAND", ORANGE),
    };
    label(traction, margin, bottom + 25., 13., traction_color);
    let bar_w = (w * 0.38).min(170.0);
    let cooling_hint = format!("COOL {:.1}s >25%", race.cooling_seconds());
    let (boost_label, boost_hint, boost_color) = match race.boost_status() {
        BoostStatus::Countdown => ("BOOST", "", PALE),
        BoostStatus::Ready => ("BOOST", "", TEAL),
        BoostStatus::Active => ("BOOSTING", "", ORANGE),
        BoostStatus::Overheated => ("OVERHEATED", cooling_hint.as_str(), ORANGE),
        BoostStatus::Sand => ("BOOST BLOCKED", "RETURN TO TRACK", PALE),
        BoostStatus::Finished => ("TRIAL COMPLETE", "", TEAL),
    };
    label(
        boost_label,
        w - margin - bar_w,
        bottom - 28.0,
        fitted_size(boost_label, bar_w, 14),
        boost_color,
    );
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
        boost_color,
    );
    draw_rectangle(w - margin - bar_w * 0.75, bottom - 19.0, 2.0, 10.0, PALE);
    label(
        &format!("{:03}%", (race.boost * 100.0) as u32),
        w - margin - bar_w,
        bottom + 8.0,
        13.0,
        PALE,
    );
    label(
        boost_hint,
        w - margin - bar_w,
        bottom + 26.0,
        fitted_size(boost_hint, bar_w, 13),
        PALE,
    );
    if race.countdown > 0.0 && !short_start {
        if h < 460. {
            // Short landscape canvases also need a single line; the compact
            // portrait case uses the next-gate row above instead.
            centered(
                &format!("THREE LAPS / {}", race.countdown.ceil() as u32),
                progress_y + 27.,
                22,
                WHITE,
            );
        } else {
            centered("THREE LAPS THROUGH THE RUINS", h * 0.34, 20, PALE);
            centered(
                &format!("{}", race.countdown.ceil() as u32),
                h * 0.48,
                74,
                WHITE,
            );
        }
    } else if race.countdown <= 0. && race.elapsed < 0.8 && !short_start {
        if h < 460. {
            centered("GO", progress_y + 27., 22, TEAL);
        } else {
            centered("GO", h * 0.42, 54, TEAL);
        }
    }
    if waiting_for_controls && !race.finished {
        centered(
            "RELEASE DRIVE CONTROLS TO REARM",
            h - if touch_mode { 106.0 } else { 22.0 },
            16,
            PALE,
        );
    } else if race.off_track && !race.finished {
        centered(
            "LOOSE SAND / BOOST DISABLED",
            h - if touch_mode { 106.0 } else { 22.0 },
            16,
            PALE,
        );
    }
    if race.recovery_timer > 0.0 && !race.finished {
        centered(
            &format!("SAFE RETURN / +{RECOVERY_PENALTY:.2}s ADDED"),
            progress_y + 29.0,
            17,
            ORANGE,
        );
    } else if let Some(seconds) = race.recovery_in() {
        centered(
            &format!("RETURN TO TRACK / RECOVERY IN {:.1}s", seconds),
            progress_y + 29.0,
            17,
            ORANGE,
        );
    } else if race.last_gate_timer > 0.0 && race.countdown == 0.0 && !race.finished {
        let text = if race.next_gate == 1 {
            "LAP COMPLETE".to_owned()
        } else {
            format!("GATE {:02} / {GATES} CLEAR", completed % GATES)
        };
        centered(&text, progress_y + 29.0, 17, PALE);
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
    let mut trial = RaceSession::new();
    let mut demo = Demo::new();
    let mut sound = GameAudio::new();
    let mut dust = ParticlePool::new(256);
    let mut dust_emitter =
        Emitter::new(presets::dust([SAND.r, SAND.g, SAND.b, 0.32]), DUST_SEED).unwrap();
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
    let mut effects = Presentation::default();
    loop {
        ferravex_render::ui::warm_up_font_sizes(&[
            10, 13, 14, 16, 17, 18, 20, 22, 28, 36, 47, 49, 54, 74,
        ]);
        ui::warm_up_font();
        let rects = touch_controls();
        let layout = std::array::from_fn::<_, 4, _>(|i| Control {
            region: Region::Rect(rects[i]),
            output: Output::Button([12, 13, 14, 7][i]),
        });
        player_input.set_touch_layout(&layout).unwrap();
        player_input.sample_play();
        effects.sync(&mut camera, &mut dust, &mut dust_emitter);
        let touch_mode = player_input.touch_seen();
        if !player_input.controls_open()
            && (player_input.pressed("restart")
                || (trial.race.finished
                    && player_input.gameplay_active()
                    && (player_input.pressed("confirm")
                        || is_mouse_button_pressed(MouseButton::Left)
                        || ferravex_render::input::logical_touches()
                            .iter()
                            .any(|t| t.phase == TouchPhase::Started))))
        {
            trial.restart();
            player_input.set_paused(false);
            sound.reset();
            reset_drive_feedback(&mut camera, &mut dust, &mut dust_emitter);
            effects.reset();
            previous_impacts = 0;
            previous_recoveries = 0;
        }
        let movement = player_input.movement();
        let input = Input {
            throttle: movement.y,
            steer: movement.x,
            boost: player_input.held("boost"),
        };
        trial
            .session
            .set_paused(!demo.autoplay && !player_input.gameplay_active());
        let elapsed = demo.dt();
        let laps = trial.race.laps;
        let gates = trial.race.gates_completed();
        let finished = trial.race.finished;
        trial.advance(&course, input, demo.autoplay, elapsed as f64);
        let race = &trial.race;
        let dt = if trial.session.is_playing() {
            elapsed
        } else {
            0.0
        };
        if race.finished && !finished {
            sound.cue(Cue::Success, 0.65);
        } else if race.gates_completed() > gates {
            sound.cue(Cue::Pickup, if race.laps > laps { 0.5 } else { 0.3 });
        }
        sound.motor(race.speed(), 48., trial.session.is_playing());
        let position = race.previous_position.lerp(race.position, trial.alpha);
        let yaw =
            race.previous_yaw + race::angle_difference(race.yaw, race.previous_yaw) * trial.alpha;
        if race.recovery_count != previous_recoveries {
            reset_drive_feedback(&mut camera, &mut dust, &mut dust_emitter);
            effects.reset();
            sound.reset();
            dust_emitter
                .teleport(EmitterTransform {
                    position: world(position - vec2(yaw.sin(), yaw.cos()) * 2., 0.25).to_array(),
                    rotation: Quat::from_rotation_y(yaw).to_array(),
                })
                .unwrap();
        } else if race.impacts > previous_impacts {
            sound.cue(Cue::Impact(Surface::Metal), 0.7);
            effects.impact(&mut camera);
        }
        previous_impacts = race.impacts;
        previous_recoveries = race.recovery_count;
        effects.advance(dt);
        dust.update(dt).unwrap();
        let density = match ferravex_render::quality() {
            ferravex_render::lighting::Quality::Low => 0.5,
            ferravex_render::lighting::Quality::Medium => 1.,
            ferravex_render::lighting::Quality::High => 1.5,
        };
        dust_emitter
            .set_rate(
                if !effects.policy.reduced_motion() && race.speed() > 3. && !race.finished {
                    32. * density * if race.boosting { 1.5 } else { 1. }
                } else {
                    0.
                },
            )
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
                chase_input(race, position, yaw, effects.policy),
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
        scene.draw(&course, race, position, yaw, &effects);
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
        draw_hud(
            race,
            &player_input,
            trial.waiting_for_controls && !demo.autoplay,
        );
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

#[cfg(test)]
mod presentation_tests {
    use super::*;
    use ferravex_play::camera::{Input3, Mode3, NoObstructions, ShakeImpulse, Target3};

    #[test]
    fn short_start_row_clears_timer_and_minimum_touch_telemetry() {
        // Use actual bitmap ink bounds, including the label's shadow, rather
        // than assuming that the requested size equals the painted height.
        let bounds = |text: &str, y: f32, size| {
            let metrics = Font.try_metrics(size, 1.).unwrap();
            let ink = metrics.measure_line(text).unwrap().ink_bounds.unwrap();
            (
                y - metrics.baseline() + ink.y,
                y - metrics.baseline() + ink.y + ink.h + 2.,
            )
        };
        for cue in ["THREE LAPS / 3", "THREE LAPS / 2", "THREE LAPS / 1", "GO"] {
            let (top, bottom) = bounds(cue, SHORT_START_ROW.0, SHORT_START_ROW.1);
            assert!(top > bounds("LAP 01 / 03", 54., 14).1);
            assert!(bottom < bounds("00:00.00", 108., 28).0);
            for height in [352., 365., 409.] {
                let telemetry_y = height - 150.; // Authored touch HUD reservation.
                assert!(bottom < bounds("BOOST ON GO", telemetry_y - 28., 14).0);
                assert!(bottom < bounds("000", telemetry_y, 36).0);
            }
            for width in [320., 390.] {
                assert_eq!(fitted_size(cue, width - 32., SHORT_START_ROW.1), 18.);
            }
        }
    }

    #[test]
    fn displayed_time_carries_rounding_into_minutes() {
        assert_eq!(time_label(59.999), "01:00.00");
        assert_eq!(time_label(119.999), "02:00.00");
        assert_eq!(time_label(62.345), "01:02.35");
        assert_eq!(time_label(0.), "00:00.00");
    }

    #[test]
    fn restart_feedback_removes_shake_dust_and_camera_smoothing_debt() {
        let mut camera = Rig3::default();
        let camera_input = |position, fov_y| Input3 {
            target: Target3::at(position),
            mode: Mode3::Follow {
                eye_offset: Vec3::Z * 10.0,
                look_offset: Vec3::ZERO,
            },
            fov_y,
        };
        camera
            .update(0.0, camera_input(Vec3::X * 100.0, 1.2), &NoObstructions)
            .unwrap();
        camera
            .shake
            .push(ShakeImpulse {
                translation: Vec3::splat(0.1),
                rotation: Vec3::splat(0.01),
                duration: 1.0,
                frequency: 20.0,
            })
            .unwrap();
        let mut dust = ParticlePool::new(32);
        let mut emitter = Emitter::new(presets::dust([1.0; 4]), DUST_SEED).unwrap();
        emitter.set_rate(60.0).unwrap();
        emitter
            .update(0.1, EmitterTransform::at([100.0, 0.0, 0.0]), &mut dust)
            .unwrap();
        assert!(!dust.is_empty());
        assert!(emitter.sequence() > 0);
        reset_drive_feedback(&mut camera, &mut dust, &mut emitter);
        assert!(dust.is_empty());
        assert_eq!(emitter.sequence(), 0);
        assert_eq!(camera.shake.active(), 0);
        let pose = camera
            .update(0.0, camera_input(Vec3::ZERO, 0.9), &NoObstructions)
            .unwrap()
            .pose;
        assert_eq!(pose.target, Vec3::ZERO);
        assert_eq!(pose.position, Vec3::Z * 10.0);
        assert_eq!(pose.fov_y, 0.9);
    }
}
