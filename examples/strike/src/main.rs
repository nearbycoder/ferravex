//! Original low-poly FPS vertical slice on Ferravex; native UDP or offline practice on the web.
mod controls;
mod geometry;
mod hud;

use controls::Controls;
use ferravex_core::{FixedClock, FrameStats};
#[cfg(not(target_arch = "wasm32"))]
use ferravex_fps::GameClient;
use ferravex_fps::{Arena, Match, Player, TICK_RATE, forward};
use ferravex_net::{BUTTON_FIRE, InputFrame};
use ferravex_render::shaders::{LitMaterial, LitSettings};
use geometry::{build_operator, build_rifle, build_yard, draw_transformed, rgb};
use macroquad::prelude::*;

fn window_conf() -> macroquad::conf::Conf {
    macroquad::conf::Conf {
        miniquad_conf: Conf {
            window_title: "Ferravex / Strike — Dockyard".into(),
            window_width: 1440,
            window_height: 900,
            high_dpi: true,
            sample_count: 4,
            ..Default::default()
        },
        draw_call_vertex_capacity: 16_384,
        draw_call_index_capacity: 24_576,
        ..Default::default()
    }
}

#[derive(Default)]
struct Options {
    connect: Option<String>,
    smoke_frames: Option<u64>,
    #[cfg(not(target_arch = "wasm32"))]
    screenshot: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    capture_dir: Option<String>,
    play: bool,
}
#[cfg(target_arch = "wasm32")]
fn options() -> Options {
    Options::default()
}

#[cfg(not(target_arch = "wasm32"))]
fn options() -> Options {
    let mut out = Options::default();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut args = std::env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--connect" => out.connect = args.next(),
                "--smoke-frames" => out.smoke_frames = args.next().and_then(|s| s.parse().ok()),
                "--screenshot" => out.screenshot = args.next(),
                "--capture-dir" => out.capture_dir = args.next(),
                "--play" => out.play = true,
                "--help" => {
                    println!(
                        "strike [--connect 127.0.0.1:4000] [--play] [--smoke-frames N] [--screenshot path.png] [--capture-dir directory]"
                    );
                    std::process::exit(0);
                }
                _ => eprintln!("Unknown option: {arg}"),
            }
        }
    }
    out
}

fn practice() -> Match {
    let mut game = Match::new(Arena::default());
    for id in 0..6 {
        game.spawn_player(id);
    }
    game
}

fn bot_input(bot: &Player, target: &Player, tick: u32) -> InputFrame {
    let mut to = target.position - bot.eye_position();
    to.y += 1.0;
    let yaw = to.x.atan2(-to.z);
    let pitch = (to.y / to.length().max(0.001)).asin();
    // Slow, imperfect opponents; same authoritative collision/fire code as human players.
    let phase = tick as f32 * 0.014 + bot.id as f32 * 1.7;
    InputFrame {
        sequence: tick,
        move_x: phase.sin() * 0.42,
        move_z: if to.length() > 12. { 0.28 } else { -0.12 },
        yaw: yaw + phase.cos() * 0.09,
        pitch,
        buttons: if tick > 320 && tick.wrapping_add(bot.id as u32 * 47).is_multiple_of(320) {
            BUTTON_FIRE
        } else if bot.ammo == 0 {
            ferravex_net::BUTTON_RELOAD
        } else {
            0
        },
    }
}

const MAX_TRACERS: usize = 32;
struct Tracer {
    origin: Vec3,
    end: Vec3,
    life: f32,
}

// Both offline authority and replicated network events feed the same effects.
// Client prediction never manufactures a hit confirmation.
fn apply_game_events(
    events: impl IntoIterator<Item = ferravex_fps::GameEvent>,
    local_id: u8,
    tracers: &mut Vec<Tracer>,
    hit_timer: &mut f32,
    damage_timer: &mut f32,
) {
    for event in events {
        match event {
            ferravex_fps::GameEvent::Shot {
                player,
                origin,
                end,
            } if player != local_id => {
                if tracers.len() == MAX_TRACERS {
                    tracers.remove(0);
                }
                tracers.push(Tracer {
                    origin: Vec3::from_array(origin.to_array()),
                    end: Vec3::from_array(end.to_array()),
                    life: 0.09,
                });
            }
            ferravex_fps::GameEvent::Hit {
                attacker, victim, ..
            } => {
                if attacker == local_id {
                    *hit_timer = 0.16;
                }
                if victim == local_id {
                    *damage_timer = 0.25;
                }
            }
            _ => {}
        }
    }
}

// Optional browser chrome follows actual gameplay state, including touch input.
#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn ferravex_set_playing(playing: i32);
}
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn ferravex_ui_crate_version() -> u32 {
    1
}

fn sync_browser_ui(playing: bool) {
    #[cfg(target_arch = "wasm32")]
    // SAFETY: web/index.html registers this import before instantiating the game.
    unsafe {
        ferravex_set_playing(i32::from(playing));
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = playing;
}

#[macroquad::main(window_conf)]
async fn main() {
    let opts = options();
    simulate_mouse_with_touch(false);
    let mut game = practice();
    let arena = build_yard(&game.arena);
    let operator = build_operator();
    let rifle = build_rifle();
    let mut tracer_model = geometry::Model::new();
    tracer_model.cube(Vec3::ZERO, vec3(0.018, 0.018, 1.), rgb(0xe8b56a));
    let mut tracers = Vec::with_capacity(MAX_TRACERS);

    let shader = LitMaterial::new();
    #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
    let mut error = shader.as_ref().err().cloned().unwrap_or_default();
    #[cfg(not(target_arch = "wasm32"))]
    let mut client = opts
        .connect
        .as_ref()
        .and_then(|addr| match GameClient::connect(addr) {
            Ok(client) => Some(client),
            Err(e) => {
                error = format!("Connection error: {e}");
                None
            }
        });
    let network = opts.connect.is_some();
    let mut active = opts.play;
    let mut has_played = active;
    let mut diagnostics = false;
    let mut browser_playing = active;
    sync_browser_ui(active);
    let mut controls = Controls::default();
    let mut clock = FixedClock::new(TICK_RATE as f64, 8);
    let mut stats = FrameStats::<120>::default();
    let mut frames = 0u64;
    let mut pending_buttons = 0;
    let mut shot_timer = 0f32;
    let mut hit_timer = 0f32;
    let mut damage_timer = 0f32;
    let mut local = game.players[0].clone();
    let mut remote = game.players.clone();
    let mut server_tick = 0;
    let mut was_ammo = local.ammo;
    let mut was_health = local.health;
    let mut previous_local_position = local.position;
    if active && opts.smoke_frames.is_none() {
        set_cursor_grab(true);
        show_mouse(false);
    }

    loop {
        let dt = if opts.smoke_frames.is_some() {
            1.0 / 60.0
        } else {
            get_frame_time().min(0.25)
        };
        stats.record(get_frame_time());
        if is_key_pressed(KeyCode::Escape) {
            active = !active;
            has_played |= active;
            set_cursor_grab(active && !controls.touch_mode);
            show_mouse(!active || controls.touch_mode);
        }
        if is_key_pressed(KeyCode::F3) {
            diagnostics = !diagnostics;
        }
        let mut input = controls.sample(active);
        #[cfg(not(target_arch = "wasm32"))]
        if opts.capture_dir.is_some() && active {
            let t = frames as f32 / 60.0;
            input.yaw = (t * 0.18).sin() * 1.2;
            input.pitch = -0.03;
            input.move_z = if t < 8.0 { 0.8 } else { 0.2 };
            input.move_x = (t * 0.7).sin() * 0.6;
            input.buttons = if (frames / 120).is_multiple_of(3) {
                ferravex_fps::BUTTON_RELOAD
            } else {
                ferravex_fps::BUTTON_FIRE
            };
        }
        if active {
            pending_buttons |= input.buttons;
        } else {
            pending_buttons = 0;
        }
        input.buttons = pending_buttons;
        let frame = clock.advance(dt as f64);
        if frame.steps > 0 {
            pending_buttons = 0;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(c) = client.as_mut() {
            if let Err(e) = c.poll() {
                error = e.to_string();
            }
            let local_id = c.player_id().unwrap_or(local.id);
            apply_game_events(
                c.drain_events().into_iter().map(|message| message.event),
                local_id,
                &mut tracers,
                &mut hit_timer,
                &mut damage_timer,
            );
        }
        for _ in 0..frame.steps {
            previous_local_position = local.position;
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(c) = client.as_mut() {
                if let Err(e) = c.tick(input) {
                    error = e.to_string();
                }
                if let Some(player) = c.local_player() {
                    local = player.clone();
                }
                remote = c.players().to_vec();
                server_tick = c.server_tick();
                continue;
            }
            if !network && active {
                let mut commands = Vec::with_capacity(6);
                commands.push((
                    0,
                    InputFrame {
                        sequence: game.tick,
                        ..input
                    },
                ));
                let target = game.players[0].clone();
                for bot in game.players.iter().skip(1) {
                    commands.push((bot.id, bot_input(bot, &target, game.tick)));
                }
                let events = game.step(&commands);
                apply_game_events(events, 0, &mut tracers, &mut hit_timer, &mut damage_timer);
                local = game.players[0].clone();
                remote = game.players.clone();
                server_tick = game.tick;
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(c) = client.as_ref() {
            remote = c.interpolated_players(c.interpolation_alpha());
            if !c.is_connected() {
                error = "Connecting to server...".into();
            } else if error == "Connecting to server..." {
                error.clear();
            }
        }
        if local.ammo < was_ammo {
            shot_timer = 0.09;
        }

        if local.health < was_health {
            damage_timer = 0.25;
        }
        was_ammo = local.ammo;
        was_health = local.health;
        shot_timer = (shot_timer - dt).max(0.);
        hit_timer = (hit_timer - dt).max(0.);
        damage_timer = (damage_timer - dt).max(0.);
        tracers.retain_mut(|tracer| {
            tracer.life -= dt;
            tracer.life > 0.
        });
        let sky = rgb(0xb8c4b7);
        clear_background(sky);
        let eye = if active {
            let feet = if previous_local_position.distance_squared(local.position) < 4. {
                previous_local_position.lerp(local.position, frame.alpha)
            } else {
                local.position
            };
            Vec3::from_array(feet.to_array()) + Vec3::Y * ferravex_fps::EYE_HEIGHT
        } else {
            let t = get_time() as f32 * 0.035;
            vec3(17. + t.sin() * 2., 11., 24.)
        };
        let direction = Vec3::from_array(forward(controls.yaw, controls.pitch).to_array());
        let target = if active {
            eye + direction
        } else {
            vec3(-3., 1., -3.)
        };
        let camera = Camera3D {
            position: eye,
            target,
            up: Vec3::Y,
            fovy: 68f32.to_radians(),
            z_near: 0.04,
            z_far: 150.,
            ..Default::default()
        };
        set_camera(&camera);
        let light = LitSettings {
            camera_position: eye,
            fog_color: sky,
            fog_start: 28.,
            fog_end: 100.,
            ambient: 0.56,
            light_direction: vec3(-0.5, 0.85, 0.3),
        };
        if let Ok(s) = &shader {
            s.begin(light);
        }
        arena.draw();
        for p in &remote {
            if (active && p.id == local.id) || p.health == 0 {
                continue;
            }
            draw_transformed(
                &operator,
                Mat4::from_rotation_translation(
                    Quat::from_rotation_y(-p.yaw),
                    Vec3::from_array(p.position.to_array()),
                ),
            );
        }
        if active {
            for tracer in &tracers {
                let delta = tracer.end - tracer.origin;
                let length = delta.length();
                if length > 0.001 {
                    draw_transformed(
                        &tracer_model,
                        Mat4::from_scale_rotation_translation(
                            vec3(1., 1., length),
                            Quat::from_rotation_arc(Vec3::Z, delta / length),
                            (tracer.origin + tracer.end) * 0.5,
                        ),
                    );
                }
            }
        }
        if active && local.health > 0 {
            let side = direction.cross(Vec3::Y).normalize_or_zero();
            let up = side.cross(direction).normalize_or_zero();
            let speed = vec2(local.velocity.x, local.velocity.z).length();
            let bob = (get_time() as f32 * 10.).sin() * 0.008 * (speed / 7.).min(1.);
            let offset = eye + side * 0.27 - up * (0.25 + shot_timer * 0.28)
                + direction * (0.47 - shot_timer * 0.2)
                + Vec3::Y * bob;
            let rot = Quat::from_rotation_y(-controls.yaw) * Quat::from_rotation_x(controls.pitch);
            draw_transformed(&rifle, Mat4::from_rotation_translation(rot, offset));
            if shot_timer > 0.055 {
                let muzzle = offset + rot * vec3(0., 0.05, -0.80);
                draw_cube(muzzle, vec3(0.065, 0.06, 0.12), None, rgb(0xffda87));
            }
        }
        if let Ok(s) = &shader {
            s.end();
        }
        set_default_camera();
        if damage_timer > 0. {
            draw_rectangle(
                0.,
                0.,
                screen_width(),
                screen_height(),
                Color::new(0.6, 0.12, 0.05, damage_timer * 0.3),
            );
        }
        let triangles = if active && local.health > 0 && shot_timer > 0.055 {
            12
        } else {
            0
        } + arena.triangles()
            + if active {
                tracers.len() * tracer_model.triangles()
            } else {
                0
            }
            + remote
                .iter()
                .filter(|p| p.health > 0 && (!active || p.id != local.id))
                .count()
                * operator.triangles()
            + if active && local.health > 0 {
                rifle.triangles()
            } else {
                0
            };
        hud::overlay(
            &local,
            hud::HudState {
                active,
                touch_mode: controls.touch_mode,
                diagnostics,
                frame_ms: stats.mean_ms(),
                triangles,
                tick: server_tick,
                hit_timer,
                shot_timer,
                error: &error,
            },
        );
        if active {
            controls.draw_touch();
            if is_key_down(KeyCode::Tab) {
                hud::roster(&remote, local.id);
            }
        } else if hud::menu(network, has_played) {
            active = true;
            has_played = true;
            controls.touch_mode |= !ferravex_render::input::logical_touches().is_empty();
            set_cursor_grab(!controls.touch_mode);
            show_mouse(controls.touch_mode);
            clock.reset();
        }
        if browser_playing != active {
            sync_browser_ui(active);
            browser_playing = active;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(directory) = &opts.capture_dir {
            std::fs::create_dir_all(directory).expect("create capture directory");
            get_screen_data().export_png(&format!("{directory}/{frames:06}.png"));
        }
        frames += 1;
        if opts.smoke_frames.is_some_and(|limit| frames >= limit) {
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(path) = &opts.screenshot {
                get_screen_data().export_png(path);
            }
            break;
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod event_effect_tests {
    use super::*;
    #[test]
    fn replicated_hits_confirm_only_authority_and_remote_tracers_are_bounded() {
        let mut tracers = Vec::new();
        let mut hit = 0.;
        let mut damage = 0.;
        apply_game_events(
            [ferravex_fps::GameEvent::Hit {
                attacker: 0,
                victim: 1,
                damage: 34,
                headshot: false,
            }],
            0,
            &mut tracers,
            &mut hit,
            &mut damage,
        );
        assert_eq!(hit, 0.16);
        assert_eq!(damage, 0.);
        for player in std::iter::repeat_n(1, MAX_TRACERS + 10).chain([0]) {
            apply_game_events(
                [ferravex_fps::GameEvent::Shot {
                    player,
                    origin: [0., 1., 0.].into(),
                    end: [0., 1., -5.].into(),
                }],
                0,
                &mut tracers,
                &mut hit,
                &mut damage,
            );
        }
        assert_eq!(tracers.len(), MAX_TRACERS);
        apply_game_events(
            [ferravex_fps::GameEvent::Hit {
                attacker: 1,
                victim: 0,
                damage: 34,
                headshot: false,
            }],
            0,
            &mut tracers,
            &mut hit,
            &mut damage,
        );
        assert_eq!(damage, 0.25);
    }
}
