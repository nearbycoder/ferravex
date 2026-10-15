//! An asset-free rolling-ball time trial over a suspended, low-poly aqueduct.
use ferravex_render::input::{
    ActionKind, Binding, InputCode, PlayerInput, Preset,
    touch::{Control, Output, Region},
};
mod sim;
use ferravex_core::FixedClock;
use ferravex_engine::camera::{
    Input3, Mode3, ObstructionSettings, Rig3, RigidObstructions, Settings3, Target3,
};
use ferravex_render::{
    demo::Demo,
    mesh::Mesh3d,
    prelude::*,
    shaders::{LitMaterial, LitSettings},
    visibility::{LodChain, LodState},
};
use sim::{CHECKPOINTS, Controls, FINISH, GEMS, OBSTACLES, PLATFORMS, RADIUS, Run};

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
    mesh: Mesh3d,
    position: Vec3,
    rotation: Quat,
    lod: Option<LodChain>,
    lod_state: std::cell::Cell<LodState>,
}
impl Prop {
    fn new(mesh: Mesh3d, position: Vec3) -> Self {
        Self {
            mesh,
            position,
            rotation: Quat::IDENTITY,
            lod: None,
            lod_state: Default::default(),
        }
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
struct Scene {
    platforms: Vec<Prop>,
    decoration: Vec<Prop>,
    marble: Mesh3d,
    gem: Mesh3d,
    gem_base: Mesh3d,
    pillar: Mesh3d,
    cap: Mesh3d,
    checkpoint: Mesh3d,
    checkpoint_lit: Mesh3d,
    shadow: Mesh3d,
}
impl Scene {
    fn new() -> Self {
        let platforms = PLATFORMS
            .iter()
            .enumerate()
            .map(|(i, p)| {
                Prop::new(
                    Mesh3d::cuboid(
                        p.size,
                        if i % 2 == 0 {
                            rgb(190, 149, 113)
                        } else {
                            rgb(210, 172, 126)
                        },
                    ),
                    p.center,
                )
            })
            .collect();
        let mut decoration = Vec::new();
        // Hanging stone buttresses and terracotta trim make the course feel built.
        for (i, p) in PLATFORMS.iter().enumerate() {
            decoration.push(Prop::new(
                Mesh3d::cuboid(
                    vec3(p.size.x + 0.08, 0.12, p.size.z + 0.08),
                    rgb(129, 102, 85),
                ),
                p.center - vec3(0., 0.26, 0.),
            ));
            if i % 2 == 0 {
                decoration.push(Prop::new(
                    Mesh3d::frustum(0.65, 2.0, 6.0, rgb(101, 121, 112), 5),
                    p.center - vec3(0., 3.3, 0.),
                ));
                decoration.push(Prop::new(
                    Mesh3d::cuboid(vec3(1.15, 1.6, 1.15), rgb(161, 132, 104)),
                    p.center - vec3(0., 1.1, 0.),
                ));
            }
            // Inset colored edges are visual guides, not invisible collision rails.
            for side in [-1., 1.] {
                let (size, offset) = if p.size.z > p.size.x {
                    (
                        vec3(0.11, 0.018, p.size.z),
                        vec3(side * (p.size.x * 0.5 - 0.15), 0.311, 0.),
                    )
                } else {
                    (
                        vec3(p.size.x, 0.018, 0.11),
                        vec3(0., 0.311, side * (p.size.z * 0.5 - 0.15)),
                    )
                };
                decoration.push(Prop::new(Mesh3d::cuboid(size, TEAL), p.center + offset));
            }
        }
        // Thin expansion joints across each lane, using geometry rather than textures.
        for p in PLATFORMS.iter().filter(|p| p.size.x.max(p.size.z) > 7.) {
            let length = p.size.x.max(p.size.z);
            for i in 1..length as usize {
                let d = i as f32 - length * 0.5;
                let (size, offset) = if p.size.z > p.size.x {
                    (vec3(p.size.x - 0.4, 0.013, 0.025), vec3(0., 0.308, d))
                } else {
                    (vec3(0.025, 0.013, p.size.z - 0.4), vec3(d, 0.308, 0.))
                };
                decoration.push(Prop::new(
                    Mesh3d::cuboid(size, rgb(160, 128, 99)),
                    p.center + offset,
                ));
            }
        }
        for p in CHECKPOINTS {
            decoration.push(Prop::new(
                Mesh3d::cylinder(0.07, 2.8, INK, 6),
                p + vec3(-2., 0.6, -1.8),
            ));
            decoration.push(Prop::new(
                Mesh3d::cuboid(vec3(0.95, 0.5, 0.04), TEAL),
                p + vec3(-1.6, 1.65, -1.8),
            ));
        }
        for x in [-2.1, 2.1] {
            decoration.push(Prop::new(
                Mesh3d::cuboid(vec3(0.3, 3.2, 0.3), TEAL),
                FINISH + vec3(x, 1., -0.8),
            ));
        }
        decoration.push(Prop::new(
            Mesh3d::cuboid(vec3(4.6, 0.5, 0.4), TEAL),
            FINISH + vec3(0., 2.75, -0.8),
        ));
        for x in -4..=4 {
            decoration.push(Prop::new(
                Mesh3d::cuboid(vec3(0.45, 0.02, 1.4), if x % 2 == 0 { CREAM } else { INK }),
                FINISH + vec3(x as f32 * 0.45, -0.58, 0.),
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
        let mut marble = Mesh3d::sphere(RADIUS, CREAM, 12, 8);
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
            pillar: Mesh3d::cylinder(0.38, 1.3, rgb(146, 80, 61), 8),
            cap: Mesh3d::cylinder(0.39, 0.13, CREAM, 8),
            checkpoint: Mesh3d::cylinder(1.25, 0.02, rgb(105, 142, 128), 16),
            checkpoint_lit: Mesh3d::cylinder(1.25, 0.025, TEAL, 16),
            shadow: Mesh3d::cylinder(0.62, 0.015, rgb(131, 113, 90), 12),
        }
    }
    fn draw(&self, game: &Run, position: Vec3, rotation: Quat, time: f32) {
        for prop in &self.platforms {
            prop.draw();
        }
        for prop in &self.decoration {
            prop.draw();
        }
        for p in OBSTACLES {
            self.pillar.draw(p, Quat::IDENTITY);
            self.cap.draw(p + vec3(0., 0.55, 0.), Quat::IDENTITY);
        }
        for (i, p) in CHECKPOINTS.iter().enumerate() {
            (if game.checkpoint > i {
                &self.checkpoint_lit
            } else {
                &self.checkpoint
            })
            .draw(vec3(p.x, 0.323, p.z), Quat::IDENTITY);
        }
        for (i, p) in GEMS.iter().enumerate() {
            if !game.collected[i] {
                self.gem_base.draw(vec3(p.x, 0.325, p.z), Quat::IDENTITY);
                self.gem.draw(
                    *p + vec3(0., 0.15 + (time * 2. + i as f32).sin() * 0.1, 0.),
                    Quat::from_rotation_y(time * 1.3),
                );
            }
        }
        if position.y > 0.3
            && PLATFORMS.iter().any(|p| {
                (position.x - p.center.x).abs() < p.size.x * 0.5
                    && (position.z - p.center.z).abs() < p.size.z * 0.5
            })
        {
            self.shadow.draw(
                vec3(position.x + 0.12, 0.322, position.z + 0.18),
                Quat::IDENTITY,
            );
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
    let mut game = Run::new();
    let scene = Scene::new();
    let material = LitMaterial::new().expect("Ferravex's portable lit material must compile");
    let mut clock = FixedClock::new(120., 12);
    let mut demo = Demo::new();
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
    let mut camera = Rig3::new(Settings3 {
        position_half_life: std::f32::consts::LN_2 / 6.,
        target_half_life: std::f32::consts::LN_2 / 6.,
        teleport_distance: Some(8.),
        obstruction: Some(ObstructionSettings {
            radius: 0.16,
            ..Default::default()
        }),
        ..Default::default()
    })
    .unwrap();
    let mut previous_falls = 0;
    let forward = vec3(-8., 0., -19.).normalize();
    let right = vec3(-forward.z, 0., forward.x);
    loop {
        ferravex_render::ui::warm_up_font_sizes(&[12, 17, 18, 20, 23, 26, 30]);
        player_input
            .set_touch_layout(&[
                Control {
                    region: Region::Circle {
                        center: vec2(100., screen_height() - 105.),
                        radius: 100.,
                    },
                    output: Output::Stick {
                        x: 0,
                        y: 1,
                        radius: 48.,
                        floating: false,
                    },
                },
                Control {
                    region: Region::Circle {
                        center: vec2(screen_width() - 75., screen_height() - 90.),
                        radius: 65.,
                    },
                    output: Output::Button(6),
                },
            ])
            .unwrap();
        player_input.sample_play();
        let touch_mode = player_input.touch_seen();
        let touch = player_input
            .touch_stick(0, 1)
            .map_or(Vec2::ZERO, |(_, delta)| delta);
        if player_input.pressed("restart")
            || game.finished
                && (player_input.pressed("confirm") || is_mouse_button_pressed(MouseButton::Left))
        {
            game = Run::new();
            clock.reset();
            camera.cut();
        }
        let movement = player_input.movement();
        let direction = right * movement.x + forward * movement.y;
        let input = Controls {
            roll: vec2(direction.x, direction.z).clamp_length_max(1.),
            brake: player_input.held("brake"),
        };
        let dt = if demo.autoplay {
            demo.dt()
        } else {
            player_input.simulation_dt(demo.dt())
        };
        let frame = clock.advance(dt as f64);
        for _ in 0..frame.steps {
            let controls = if demo.autoplay {
                game.autopilot()
            } else {
                input
            };
            game.step(controls, frame.dt);
        }
        let position = game.previous_position.lerp(game.position(), frame.alpha);
        let rotation = game.previous_rotation.slerp(game.rotation(), frame.alpha);
        if game.falls != previous_falls {
            camera.cut();
        }
        previous_falls = game.falls;
        let view = camera
            .update(
                demo.dt(),
                Input3 {
                    target: Target3::at(position),
                    mode: Mode3::Follow {
                        eye_offset: vec3(8., 12., 14.),
                        look_offset: vec3(0., -0.2, -5.),
                    },
                    fov_y: 48_f32.to_radians(),
                },
                &RigidObstructions {
                    world: &game.physics,
                    mask: u32::MAX,
                    ignore: Some(game.marble),
                },
            )
            .expect("valid rolling camera")
            .pose;
        clear_background(SKY);
        let overview = player_input.held("overview");
        let camera_position = if overview {
            vec3(31., 40., 20.)
        } else {
            view.position
        };
        let target = if overview {
            vec3(3., 0., -18.)
        } else {
            view.target
        };
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
        scene.draw(&game, position, rotation, demo.time());
        material.end();
        set_default_camera();
        text("MARBLE RUN", 27., 39., 26., INK);
        if screen_width() > 620. {
            text("THE HANGING AQUEDUCT", 28., 60., 12., INK);
        }
        let status = format!(
            "{} / 6     {:02}:{:04.1}",
            game.gems(),
            (game.elapsed / 60.) as u32,
            game.elapsed % 60.
        );
        let status_width = measure_text(&status, None, 23, 1.).width;
        text(&status, screen_width() - status_width - 27., 39., 23., INK);
        draw_poly(screen_width() - status_width - 43., 31., 4, 8., 0., GOLD);
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
        if touch_mode && !game.finished {
            let center = vec2(100., screen_height() - 105.);
            draw_circle(center.x, center.y, 66., Color::new(0.12, 0.25, 0.24, 0.15));
            draw_circle_lines(center.x, center.y, 66., 2., CREAM);
            draw_circle(
                center.x + touch.x * 35.,
                center.y + touch.y * 35.,
                24.,
                Color::new(0.98, 0.94, 0.81, 0.7),
            );
            draw_circle(
                screen_width() - 75.,
                screen_height() - 90.,
                45.,
                Color::new(0.12, 0.25, 0.24, 0.35),
            );
            text(
                "BRAKE",
                screen_width() - 102.,
                screen_height() - 85.,
                18.,
                CREAM,
            );
        } else if game.elapsed < 10. {
            centered(
                if screen_width() > 700. {
                    "WASD / ARROWS  roll     SPACE  brake     TAB  course     R  restart"
                } else {
                    "Arrows roll / Space brake / R restart"
                },
                screen_height() - 27.,
                17,
                INK,
            );
        }
        if game.finished {
            let w = 330_f32.min(screen_width() - 30.);
            let y = screen_height() * 0.43;
            draw_rectangle(
                (screen_width() - w) * 0.5,
                y - 62.,
                w,
                182.,
                Color::new(0.12, 0.23, 0.23, 0.94),
            );
            centered("COURSE CLEAR", y - 20., 30, CREAM);
            centered(
                &format!("6 crystals   /   {:.1} seconds", game.elapsed),
                y + 17.,
                20,
                CREAM,
            );
            centered(&format!("{} falls", game.falls), y + 48., 18, CREAM);
            centered("R or tap to roll again", y + 90., 17, CREAM);
        }
        player_input.draw_settings();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}
