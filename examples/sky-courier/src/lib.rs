use ferravex_core::FixedClock;
use ferravex_play::camera::{
    Input3, Mode3, NoObstructions, Rig3, Settings3, ShakeImpulse, Target3,
};
use ferravex_play::particles::{
    Emitter, EmitterTransform, ParticlePool, RibbonFacing, Trail, TrailConfig, TrailSample, presets,
};
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::effects::{
    BillboardView, EffectStyle, ParticleRenderer, ParticleStyle, draw_ribbon,
};
use ferravex_render::input::{
    PlayerInput, Preset,
    touch::{Control, Output, Region},
};
use ferravex_render::{
    demo::Demo,
    mesh::Mesh3d,
    prelude::*,
    shaders::{LitMaterial, LitSettings},
};

const SKY: Color = Color::new(0.64, 0.76, 0.77, 1.0);
const INK: Color = Color::new(0.10, 0.20, 0.22, 1.0);
const CREAM: Color = Color::new(0.95, 0.91, 0.78, 1.0);
const CORAL: Color = Color::new(0.82, 0.36, 0.23, 1.0);
const ROUTE: [Vec3; 6] = [
    vec3(0., 24., 100.),
    vec3(44., 40., 205.),
    vec3(-20., 30., 320.),
    vec3(-62., 47., 430.),
    vec3(10., 34., 550.),
    vec3(0., 16., 680.),
];
const ISLANDS: [(Vec3, f32); 9] = [
    (vec3(-42., 1., 73.), 30.),
    (vec3(67., 13., 207.), 30.),
    (vec3(-48., 7., 315.), 31.),
    (vec3(-86., 22., 430.), 30.),
    (vec3(38., 10., 553.), 30.),
    (vec3(0., 8., 680.), 37.),
    (vec3(112., 4., 380.), 40.),
    (vec3(-130., 14., 170.), 36.),
    (vec3(105., 7., 680.), 29.),
];

#[derive(Default, Clone, Copy)]
struct Input {
    turn: f32,
    climb: f32,
    boost: bool,
    slow: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Outcome {
    Flying,
    Delivered,
    Lost,
}
struct Flight {
    pos: Vec3,
    yaw: f32,
    pitch: f32,
    bank: f32,
    speed: f32,
    next: usize,
    hull: u8,
    remaining: f32,
    outcome: Outcome,
    recovery: f32,
}
impl Flight {
    fn new() -> Self {
        Self {
            pos: vec3(0., 20., 0.),
            yaw: 0.,
            pitch: 0.,
            bank: 0.,
            speed: 24.,
            next: 0,
            hull: 3,
            remaining: 90.,
            outcome: Outcome::Flying,
            recovery: 0.,
        }
    }
    fn forward(&self) -> Vec3 {
        vec3(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        )
    }
    fn damage(&mut self) {
        if self.recovery > 0. || self.outcome != Outcome::Flying {
            return;
        }
        self.hull = self.hull.saturating_sub(1);
        if self.hull == 0 {
            self.outcome = Outcome::Lost;
            return;
        }
        self.pos = if self.next == 0 {
            vec3(0., 20., 0.)
        } else {
            ROUTE[self.next - 1]
        };
        let delta = ROUTE[self.next] - self.pos;
        self.yaw = delta.x.atan2(delta.z);
        self.pitch = 0.;
        self.bank = 0.;
        self.speed = 24.;
        self.recovery = 2.5;
        self.remaining = (self.remaining - 5.).max(0.);
    }
    fn tick(&mut self, input: Input, dt: f32) {
        if self.outcome != Outcome::Flying {
            return;
        }
        self.remaining = (self.remaining - dt).max(0.);
        if self.remaining <= 0. {
            self.outcome = Outcome::Lost;
            return;
        }
        self.recovery = (self.recovery - dt).max(0.);
        let turn = input.turn.clamp(-1., 1.);
        self.yaw += turn * 1.15 * dt;
        self.pitch += (input.climb.clamp(-1., 1.) * 0.58 - self.pitch) * (dt * 3.8).min(1.);
        self.bank += (-turn * 0.7 - self.bank) * (dt * 4.).min(1.);
        let target_speed = if input.slow {
            13.
        } else if input.boost {
            35.
        } else {
            24.
        };
        self.speed += (target_speed - self.speed) * (dt * 2.).min(1.);
        let previous = self.pos;
        self.pos += self.forward() * self.speed * dt;
        if self.pos.y < -17.
            || self.pos.y > 110.
            || self.pos.x.abs() > 260.
            || self.pos.z < -90.
            || self.pos.z > 850.
            || island_hit(self.pos)
        {
            self.damage();
            return;
        }
        // Swept sphere capture prevents missing a beacon between fixed steps.
        let radius = if self.next == 5 { 8. } else { 8.5 };
        if segment_hits(previous, self.pos, ROUTE[self.next], radius)
            && (self.next < 5 || self.speed <= 18.)
        {
            self.next += 1;
            if self.next == ROUTE.len() {
                self.outcome = Outcome::Delivered;
            }
        }
    }
    fn autopilot(&self) -> Input {
        if self.outcome != Outcome::Flying {
            return Input::default();
        }
        let delta = ROUTE[self.next] - self.pos;
        let error = (delta.x.atan2(delta.z) - self.yaw + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        let pitch = delta.y.atan2(vec2(delta.x, delta.z).length());
        Input {
            turn: (error * 2.7).clamp(-1., 1.),
            climb: (pitch / 0.58).clamp(-1., 1.),
            slow: self.next == 5 && delta.length() < 90.,
            boost: false,
        }
    }
}
fn segment_hits(a: Vec3, b: Vec3, center: Vec3, radius: f32) -> bool {
    let delta = b - a;
    let t = if delta.length_squared() > 0. {
        ((center - a).dot(delta) / delta.length_squared()).clamp(0., 1.)
    } else {
        0.
    };
    (a + delta * t).distance_squared(center) <= radius * radius
}
fn island_hit(pos: Vec3) -> bool {
    ISLANDS.iter().any(|(center, radius)| {
        let horizontal = vec2(pos.x - center.x, pos.z - center.z).length();
        horizontal < radius * 0.85 + 1. && pos.y < center.y + 2. && pos.y > center.y - 27.
    })
}

struct Scene {
    ocean: Mesh3d,
    islands: Vec<(Mesh3d, Mesh3d)>,
    house: Mesh3d,
    roof: Mesh3d,
    trunk: Mesh3d,
    leaves: Mesh3d,
    body: Mesh3d,
    nose: Mesh3d,
    wing: Mesh3d,
    tail: Mesh3d,
    fin: Mesh3d,
    glass: Mesh3d,
    float: Mesh3d,
    strut: Mesh3d,
    propeller: Mesh3d,
    ring: Mesh3d,
    dim_ring: Mesh3d,
    pad: Mesh3d,
    stripe: Mesh3d,
    cloud: Mesh3d,
    parcel: Mesh3d,
}
impl Scene {
    fn new() -> Self {
        Self {
            ocean: Mesh3d::plane(vec2(1200., 1400.), Color::from_rgba(60, 121, 130, 255), 28),
            islands: ISLANDS
                .iter()
                .map(|(_, r)| {
                    (
                        Mesh3d::frustum(r * 0.35, *r, 24., Color::from_rgba(100, 110, 104, 255), 7),
                        Mesh3d::frustum(*r, r * 0.85, 3., Color::from_rgba(142, 158, 112, 255), 7),
                    )
                })
                .collect(),
            house: Mesh3d::cuboid(vec3(6., 5., 5.), CREAM),
            roof: Mesh3d::cone(5., 3.4, CORAL, 4),
            trunk: Mesh3d::cylinder(0.55, 4., Color::from_rgba(104, 89, 65, 255), 5),
            leaves: Mesh3d::cone(3.6, 8., Color::from_rgba(57, 95, 75, 255), 6),
            body: Mesh3d::cuboid(vec3(1.9, 1.6, 5.5), CREAM),
            nose: Mesh3d::cylinder(1., 1.8, CORAL, 7),
            wing: Mesh3d::cuboid(vec3(11.5, 0.23, 1.9), CORAL),
            tail: Mesh3d::cuboid(vec3(4.5, 0.2, 1.3), CREAM),
            fin: Mesh3d::cuboid(vec3(0.2, 1.8, 1.5), CORAL),
            glass: Mesh3d::cuboid(vec3(1.6, 0.85, 1.8), Color::from_rgba(61, 89, 94, 255)),
            float: Mesh3d::cuboid(vec3(0.65, 0.6, 4.2), Color::from_rgba(76, 85, 81, 255)),
            strut: Mesh3d::cuboid(vec3(0.12, 1.6, 0.12), INK),
            propeller: Mesh3d::cuboid(vec3(0.15, 3.3, 0.12), INK),
            ring: Mesh3d::cuboid(vec3(3.6, 0.45, 0.5), Color::from_rgba(236, 178, 86, 255)),
            dim_ring: Mesh3d::cuboid(vec3(3.6, 0.4, 0.4), Color::from_rgba(121, 163, 157, 255)),
            pad: Mesh3d::cylinder(11., 0.25, Color::from_rgba(228, 206, 146, 255), 16),
            stripe: Mesh3d::cuboid(vec3(1.2, 0.12, 9.), CORAL),
            cloud: Mesh3d::sphere(10., Color::from_rgba(217, 226, 214, 255), 7, 4),
            parcel: Mesh3d::cuboid(vec3(1.4, 1.2, 1.1), Color::from_rgba(165, 121, 71, 255)),
        }
    }
    fn draw(&self, game: &Flight, time: f32) {
        self.ocean.draw(vec3(0., -20., 350.), Quat::IDENTITY);
        for (i, (center, radius)) in ISLANDS.iter().enumerate() {
            let (rock, grass) = &self.islands[i];
            rock.draw(*center - Vec3::Y * 13.5, Quat::IDENTITY);
            grass.draw(*center, Quat::IDENTITY);
            for j in 0..5 {
                let a = j as f32 * 1.23 + i as f32;
                let p = *center + vec3(a.cos() * radius * 0.57, 3.4, a.sin() * radius * 0.57);
                self.trunk.draw(p, Quat::IDENTITY);
                self.leaves.draw(p + Vec3::Y * 4.5, Quat::IDENTITY);
            }
            let p = *center + vec3(radius * 0.3, 4., -radius * 0.15);
            self.house.draw(p, Quat::IDENTITY);
            self.roof.draw(
                p + Vec3::Y * 4.1,
                Quat::from_rotation_y(std::f32::consts::FRAC_PI_4),
            );
        }
        for (i, gate) in ROUTE.iter().enumerate().take(5) {
            if i < game.next {
                continue;
            }
            let mesh = if i == game.next {
                &self.ring
            } else {
                &self.dim_ring
            };
            for segment in 0..16 {
                let angle = segment as f32 * std::f32::consts::TAU / 16.;
                let position = *gate + vec3(angle.cos() * 9., angle.sin() * 9., 0.);
                mesh.draw(
                    position,
                    Quat::from_rotation_z(angle + std::f32::consts::FRAC_PI_2),
                );
            }
        }
        self.pad.draw(vec3(0., 9.7, 680.), Quat::IDENTITY);
        self.stripe.draw(vec3(0., 9.9, 680.), Quat::IDENTITY);
        self.stripe.draw(
            vec3(0., 9.9, 680.),
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        );
        for i in 0..13 {
            let x = ((i * 79) % 330) as f32 - 165.;
            let z = i as f32 * 67. - 50.;
            let y = 79. + (i % 3) as f32 * 10.;
            self.cloud.draw(vec3(x, y, z), Quat::IDENTITY);
            self.cloud
                .draw(vec3(x + 10., y - 2., z + 3.), Quat::IDENTITY);
        }
        let rotation = Quat::from_rotation_y(game.yaw)
            * Quat::from_rotation_x(-game.pitch)
            * Quat::from_rotation_z(game.bank);
        let draw = |mesh: &Mesh3d, offset: Vec3| mesh.draw(game.pos + rotation * offset, rotation);
        draw(&self.body, Vec3::ZERO);
        self.nose.draw(
            game.pos + rotation * vec3(0., 0., 3.2),
            rotation * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        );
        draw(&self.wing, vec3(0., 0.2, 0.6));
        draw(&self.tail, vec3(0., 0.3, -3.));
        draw(&self.fin, vec3(0., 1., -2.8));
        draw(&self.glass, vec3(0., 1., 0.5));
        draw(&self.parcel, vec3(0., -1.2, -0.8));
        for x in [-1.5, 1.5] {
            draw(&self.float, vec3(x, -2., 0.));
            draw(&self.strut, vec3(x, -1.1, 0.7));
            draw(&self.strut, vec3(x, -1.1, -1.3));
        }
        self.propeller.draw(
            game.pos + rotation * vec3(0., 0., 4.2),
            rotation * Quat::from_rotation_z(time * 42.),
        );
    }
}

fn label(text: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(
        text,
        x + 1.,
        y + 1.,
        size,
        Color::new(0.82, 0.88, 0.85, 0.6),
    );
    draw_text(text, x, y, size, color);
}
fn centered(text: &str, y: f32, size: f32, color: Color) {
    let width = measure_text(text, None, size as u16, 1.).width;
    label(text, (screen_width() - width) * 0.5, y, size, color);
}
fn hud(game: &Flight, touch: bool) {
    let width = screen_width();
    label("SKY COURIER", 28., 42., 26., INK);
    label("THE NORTH COAST POST", 29., 63., 13., INK);
    let route = if game.next < 5 {
        format!("BEACON {} / 5", game.next + 1)
    } else {
        "DELIVERY APPROACH".to_owned()
    };
    centered(&route, 37., 19., INK);
    centered(
        &format!(
            "{:02}:{:02}",
            game.remaining as u32 / 60,
            game.remaining as u32 % 60
        ),
        63.,
        22.,
        INK,
    );
    label("HULL", width - 113., 32., 14., INK);
    for i in 0..3 {
        draw_rectangle(
            width - 111. + i as f32 * 24.,
            43.,
            17.,
            5.,
            if i < game.hull as usize {
                INK
            } else {
                Color::new(0.3, 0.4, 0.4, 0.2)
            },
        );
    }
    let bottom = screen_height() - 25.;
    label(
        &format!("{} m", game.pos.y.max(0.) as u32),
        28.,
        bottom - 24.,
        24.,
        CREAM,
    );
    label(
        &format!("{} kt", (game.speed * 1.94) as u32),
        28.,
        bottom,
        16.,
        CREAM,
    );
    if game.next == 5 && game.outcome == Outcome::Flying {
        centered("Slow pass over the landing cross to deliver", 98., 20., INK);
        centered("Hold SHIFT / touch right side", 121., 16., INK);
    } else if game.outcome == Outcome::Flying {
        centered("Fly through the amber beacons", 98., 18., INK);
    }
    if game.recovery > 0. {
        centered("RECOVERED AT LAST BEACON  /  -5 seconds", 148., 17., CORAL);
    }
    if touch {
        draw_circle_lines(90., screen_height() - 100., 60., 2., CREAM);
        draw_line(
            75.,
            screen_height() - 100.,
            105.,
            screen_height() - 100.,
            1.,
            CREAM,
        );
        draw_line(
            90.,
            screen_height() - 115.,
            90.,
            screen_height() - 85.,
            1.,
            CREAM,
        );
        label("SLOW", width - 100., screen_height() - 90., 19., CREAM);
    } else {
        centered(
            "A D  turn    W S  climb / dive    SPACE  boost    SHIFT  slow    R  restart",
            bottom,
            16.,
            CREAM,
        );
    }
    if game.outcome != Outcome::Flying {
        let y = screen_height() * 0.45;
        draw_rectangle(0., y - 65., width, 165., Color::new(0.08, 0.17, 0.19, 0.9));
        centered(
            if game.outcome == Outcome::Delivered {
                "MAIL DELIVERED"
            } else {
                "FLIGHT ENDED"
            },
            y,
            38.,
            CREAM,
        );
        centered(
            if game.outcome == Outcome::Delivered {
                "The north coast is connected again."
            } else if game.hull == 0 {
                "Your aircraft needs repairs."
            } else {
                "The delivery window closed."
            },
            y + 35.,
            20.,
            CREAM,
        );
        centered("Press R or tap to fly again", y + 71., 18., CREAM);
    }
}
fn conf() -> Conf {
    ferravex_render::window_conf("Sky Courier — Ferravex")
}
ferravex_render::game!(conf, game);
async fn game() {
    let mut demo = Demo::new();
    let mut sound = GameAudio::new();
    let mut wing_trails: [Trail; 2] = std::array::from_fn(|_| {
        Trail::new(TrailConfig {
            capacity: 96,
            lifetime: 0.9,
            spacing: 0.5,
            teleport_distance: 35.,
            ..Default::default()
        })
        .unwrap()
    });
    let mut exhaust = ParticlePool::new(128);
    let mut exhaust_emitter =
        Emitter::new(presets::exhaust([0.68, 0.72, 0.69, 0.22]), 0x534b59).unwrap();
    let mut exhaust_renderer = ParticleRenderer::new(128).unwrap();
    let mut flight = Flight::new();
    let mut previous_effect_position = flight.pos;
    let mut clock = FixedClock::new(120., 12);
    let scene = Scene::new();
    let material = LitMaterial::new().expect("portable flight material");
    let mut player_input = PlayerInput::preset("sky-courier", Preset::Flight);
    let mut reported = false;
    let mut camera = Rig3::new(Settings3 {
        position_half_life: 0.1,
        target_half_life: 0.06,
        teleport_distance: Some(35.),
        ..Default::default()
    })
    .unwrap();
    let mut previous_hull = flight.hull;
    loop {
        ferravex_render::ui::warm_up_font_sizes(&[13, 14, 16, 17, 18, 19, 20, 22, 24, 26, 38]);
        let w = screen_width();
        let h = screen_height();
        // Keep the authored stick origin while bounding its hit region to the left half.
        player_input
            .set_touch_layout(&[
                Control {
                    region: Region::Circle {
                        center: vec2(90., h - 100.),
                        radius: 85.,
                    },
                    output: Output::Stick {
                        x: 0,
                        y: 1,
                        radius: 60.,
                        floating: false,
                    },
                },
                Control {
                    region: Region::Rect(Rect::new(w * 0.55, 0., w * 0.45, h)),
                    output: Output::Button(6),
                },
            ])
            .unwrap();
        player_input.sample_play();
        let touch_mode = player_input.touch_seen();
        if player_input.pressed("restart")
            || (flight.outcome != Outcome::Flying
                && (player_input.pressed("confirm")
                    || ferravex_render::input::logical_touches()
                        .iter()
                        .any(|t| t.phase == TouchPhase::Started)))
        {
            flight = Flight::new();
            previous_effect_position = flight.pos;
            for trail in &mut wing_trails {
                trail.clear();
            }
            exhaust.clear();
            exhaust_emitter.reset(0x534b59);
            clock.reset();
            reported = false;
            camera.cut();
            previous_hull = flight.hull;
        }
        let dt = if demo.autoplay {
            demo.dt()
        } else {
            player_input.simulation_dt(demo.dt())
        };
        let steps = clock.advance(dt as f64);
        let movement = player_input.movement();
        let human = Input {
            turn: movement.x,
            climb: movement.y,
            boost: player_input.held("boost"),
            slow: player_input.held("brake"),
        };
        for _ in 0..steps.steps {
            let input = if demo.autoplay {
                flight.autopilot()
            } else {
                human
            };
            let next = flight.next;
            let outcome = flight.outcome;
            flight.tick(input, steps.dt);
            if flight.next > next {
                sound.cue(Cue::Pickup, 0.5);
            }
            if flight.outcome != outcome {
                sound.cue(
                    if flight.outcome == Outcome::Delivered {
                        Cue::Success
                    } else {
                        Cue::Failure
                    },
                    0.65,
                );
            }
        }
        let rotation = Quat::from_rotation_y(flight.yaw)
            * Quat::from_rotation_x(-flight.pitch)
            * Quat::from_rotation_z(flight.bank);
        for (side, trail) in [-1., 1.].into_iter().zip(&mut wing_trails) {
            let position = flight.pos + rotation * vec3(side * 5.65, 0.2, -0.4);
            trail
                .update(
                    dt,
                    (flight.outcome == Outcome::Flying && flight.speed > 28.).then_some(
                        TrailSample {
                            position: position.to_array(),
                            color: [0.91, 0.93, 0.87, 0.2],
                            width: 0.16,
                        },
                    ),
                )
                .unwrap();
        }
        exhaust.update(dt).unwrap();
        exhaust_emitter
            .set_rate(if flight.outcome == Outcome::Flying {
                14.
            } else {
                0.
            })
            .unwrap();
        let nozzle = EmitterTransform {
            position: (flight.pos + rotation * vec3(0.72, -0.35, 1.8)).to_array(),
            rotation: rotation.to_array(),
        };
        if flight.pos.distance(previous_effect_position) > 35. {
            exhaust_emitter.teleport(nozzle).unwrap();
        }
        previous_effect_position = flight.pos;
        exhaust_emitter.update(dt, nozzle, &mut exhaust).unwrap();
        sound.motor(
            flight.speed,
            38.,
            flight.outcome == Outcome::Flying && (demo.autoplay || player_input.gameplay_active()),
        );
        if flight.hull < previous_hull {
            sound.cue(Cue::Impact(Surface::Metal), 0.7);
            camera.cut();
            camera
                .shake
                .push(ShakeImpulse {
                    translation: vec3(0.06, 0.04, 0.02),
                    rotation: vec3(0.008, 0.004, 0.02),
                    duration: 0.25,
                    frequency: 20.,
                })
                .expect("hull changes bound shake layers");
        }
        previous_hull = flight.hull;
        let view = camera
            .update(
                demo.dt(),
                Input3 {
                    target: Target3 {
                        position: flight.pos,
                        rotation: Quat::from_rotation_y(flight.yaw),
                        velocity: flight.forward() * flight.speed,
                    },
                    mode: Mode3::Chase {
                        distance: 24.,
                        height: 10.,
                        look_ahead: 20.,
                        look_height: 0.,
                        speed_distance: 0.,
                        speed_height: 0.,
                        prediction: 0.,
                        level: true,
                    },
                    fov_y: 55_f32.to_radians(),
                },
                &NoObstructions,
            )
            .expect("valid flight camera")
            .pose;
        let camera_pos = view.position;
        clear_background(SKY);
        set_camera(&Camera3D {
            position: camera_pos,
            target: view.target,
            up: view.up,
            fovy: view.fov_y,
            z_near: 0.2,
            z_far: 650.,
            ..Default::default()
        });
        material.begin(LitSettings {
            camera_position: camera_pos,
            fog_color: SKY,
            fog_start: 160.,
            fog_end: 500.,
            ambient: 0.65,
            ..Default::default()
        });
        scene.draw(&flight, demo.time());
        material.end();
        for trail in &wing_trails {
            draw_ribbon(
                trail,
                RibbonFacing::Camera {
                    eye: camera_pos.to_array(),
                    up: view.up.to_array(),
                },
                2.,
                EffectStyle::default(),
            )
            .unwrap();
        }
        exhaust_renderer
            .draw_3d(
                &exhaust,
                BillboardView::new(
                    camera_pos,
                    (view.target - camera_pos).cross(view.up),
                    view.up,
                )
                .unwrap(),
                ParticleStyle::default(),
            )
            .unwrap();
        set_default_camera();
        hud(&flight, touch_mode);
        if demo.autoplay && flight.outcome != Outcome::Flying && !reported {
            println!(
                "Sky Courier autoplay: {:?}; beacons={}; hull={}; remaining={:.1}s",
                flight.outcome, flight.next, flight.hull, flight.remaining
            );
            reported = true;
        }
        player_input.draw_settings();
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
    fn ordinary_flight_inputs_complete_the_route_and_delivery() {
        let mut game = Flight::new();
        for _ in 0..120 * 85 {
            game.tick(game.autopilot(), 1. / 120.);
        }
        assert_eq!(game.outcome, Outcome::Delivered);
        assert_eq!(game.next, 6);
        assert_eq!(game.hull, 3);
        assert!(game.remaining > 0.);
    }
    #[test]
    fn beacons_must_be_visited_in_order() {
        let mut game = Flight::new();
        game.pos = ROUTE[3];
        game.tick(Input::default(), 1. / 120.);
        assert_eq!(game.next, 0);
        game.pos = ROUTE[0];
        game.tick(Input::default(), 1. / 120.);
        assert_eq!(game.next, 1);
    }
    #[test]
    fn damage_recovers_and_three_crashes_end_the_flight() {
        let mut game = Flight::new();
        for hull in [2, 1, 0] {
            game.recovery = 0.;
            game.pos.y = -21.;
            game.tick(Input::default(), 1. / 120.);
            assert_eq!(game.hull, hull);
            if hull > 0 {
                assert_eq!(game.pos, vec3(0., 20., 0.));
            }
        }
        assert_eq!(game.outcome, Outcome::Lost);
        assert!(island_hit(ISLANDS[0].0));
        assert!(!island_hit(ISLANDS[0].0 + Vec3::Y * 30.));
    }
    #[test]
    fn timeout_freezes_and_restart_restores_game() {
        let mut game = Flight::new();
        game.remaining = 0.001;
        game.tick(Input::default(), 0.02);
        assert_eq!(game.outcome, Outcome::Lost);
        let pos = game.pos;
        game.tick(Input::default(), 1.);
        assert_eq!(game.pos, pos);
        game = Flight::new();
        assert_eq!(game.next, 0);
        assert_eq!(game.hull, 3);
        assert_eq!(game.outcome, Outcome::Flying);
    }
    #[test]
    fn cargo_requires_a_slow_pass_and_swept_capture_handles_fast_motion() {
        let mut game = Flight::new();
        game.next = 5;
        game.pos = ROUTE[5];
        game.speed = 30.;
        game.tick(Input::default(), 1. / 120.);
        assert_eq!(game.next, 5);
        game.speed = 13.;
        game.tick(
            Input {
                slow: true,
                ..Input::default()
            },
            1. / 120.,
        );
        assert_eq!(game.outcome, Outcome::Delivered);
        assert!(segment_hits(Vec3::ZERO, Vec3::Z * 20., Vec3::Z * 10., 1.));
    }
}
