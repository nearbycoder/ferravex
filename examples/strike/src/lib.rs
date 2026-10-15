//! Original low-poly FPS vertical slice on Ferravex; native UDP or offline practice on the web.
mod animation;
mod controls;
mod effects;
mod geometry;
mod hud;

use controls::Controls;
use effects::CombatEffects;
use ferravex_core::{FixedClock, FrameStats};
#[cfg(not(target_arch = "wasm32"))]
use ferravex_fps::GameClient;
use ferravex_fps::{Arena, Match, Player, TICK_RATE, forward};
use ferravex_net::{BUTTON_FIRE, InputFrame};
use ferravex_play::camera::{Input3, Mode3, NoObstructions, Rig3, Settings3, Target3};
use ferravex_render::audio::{self, Cue, GameAudio, Spatial, Surface, VoiceParams};
use ferravex_render::prelude::*;
use ferravex_render::shaders::{LitMaterial, LitSettings};
use geometry::{build_operator, build_rifle, build_yard, rgb};

fn window_conf() -> Conf {
    let mut conf = ferravex_render::window_conf("Ferravex / Strike — Dockyard");
    conf.window_width = 1440;
    conf.window_height = 900;
    conf
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
// Both offline authority and replicated network events feed the same effects.
// Client prediction never manufactures a hit confirmation.
fn apply_game_events(
    events: impl IntoIterator<Item = ferravex_fps::GameEvent>,
    local_id: u8,
    effects: &mut CombatEffects,
    hit_timer: &mut f32,
    damage_timer: &mut f32,
) {
    for event in events {
        if let ferravex_fps::GameEvent::Shot { player, origin, .. } = &event
            && *player != local_id
        {
            audio::play(
                Cue::Fire,
                VoiceParams {
                    position: origin.to_array(),
                    gain: 0.75,
                    priority: 5,
                    spatial: Spatial::Attenuated {
                        min_distance: 1.,
                        max_distance: 65.,
                        rolloff: 0.04,
                    },
                    ..Default::default()
                },
            );
        }
        match event {
            ferravex_fps::GameEvent::Shot {
                player,
                origin,
                end,
            } => {
                effects.shot(
                    player != local_id,
                    Vec3::from_array(origin.to_array()),
                    Vec3::from_array(end.to_array()),
                );
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

ferravex_render::game!(window_conf, game);
async fn game() {
    let opts = options();
    simulate_mouse_with_touch(false);
    let mut game = practice();
    ferravex_render::set_quality(ferravex_render::lighting::Quality::Medium);
    let mut arena = build_yard(&game.arena);
    let ao =
        ferravex_render::ao::AoScene::new([(&arena.mesh, Mat4::IDENTITY)]).expect("yard AO scene");
    ao.bake(
        &mut arena.mesh,
        Mat4::IDENTITY,
        ferravex_render::ao::AoSettings {
            samples: 8,
            radius: 1.5,
            strength: 0.3,
            max_tests: 20_000_000,
            ..Default::default()
        },
    )
    .expect("yard vertex AO");
    let operator = build_operator();
    let rifle = build_rifle();
    let operator_rig = animation::OperatorRig::new();
    let mut operator_motion: Vec<_> = (0..ferravex_fps::MAX_PLAYERS)
        .map(|_| animation::OperatorMotion::new(&operator_rig))
        .collect();
    let mut weapon_motion = animation::WeaponMotion::new();
    let mut effects = CombatEffects::new();

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
    let mut sound = GameAudio::new();
    let mut was_reload = 0;
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
    let mut view_rig = Rig3::new(Settings3 {
        position_half_life: 0.,
        target_half_life: 0.,
        fov_half_life: 0.,
        teleport_distance: Some(2.),
        ..Default::default()
    })
    .unwrap();
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
        controls.prepare(active);
        if controls.input.pressed("pause") {
            active = !active;
            has_played |= active;
            set_cursor_grab(active && !controls.touch_mode);
            show_mouse(!active || controls.touch_mode);
        }
        if controls.input.pressed("diagnostics") {
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
        if active && controls.input.gameplay_active() {
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
                &mut effects,
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
                remote.clear();
                remote.extend_from_slice(c.players());
                server_tick = c.server_tick();
                continue;
            }
            if !network && active && controls.input.gameplay_active() {
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
                apply_game_events(events, 0, &mut effects, &mut hit_timer, &mut damage_timer);
                local = game.players[0].clone();
                remote.clone_from(&game.players);
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
        let fired = local.ammo < was_ammo;
        if fired {
            sound.cue(Cue::Fire, 0.75);
            shot_timer = 0.09;
        }

        if local.health < was_health {
            damage_timer = 0.25;
        }
        if local.reload_ticks > 0 && was_reload == 0 {
            sound.cue(Cue::Reload, 0.6);
        }
        if local.health == 0 && was_health > 0 {
            sound.cue(Cue::Failure, 0.5);
        }
        was_reload = local.reload_ticks;
        audio::listener(
            Vec3::from_array(local.eye_position().to_array()),
            vec3(controls.yaw.cos(), 0., controls.yaw.sin()),
        );
        sound.walk(
            Vec3::from_array(local.position.to_array()),
            local.grounded,
            active && controls.input.gameplay_active() && local.health > 0,
            Surface::Stone,
            1.65,
        );
        was_ammo = local.ammo;
        was_health = local.health;
        shot_timer = (shot_timer - dt).max(0.);
        hit_timer = (hit_timer - dt).max(0.);
        damage_timer = (damage_timer - dt).max(0.);
        effects.update(dt);
        let animation_dt = if active {
            controls.input.simulation_dt(dt)
        } else {
            dt
        };
        for p in &remote {
            if p.id == local.id {
                continue;
            }
            if let Some(motion) = operator_motion.get_mut(p.id as usize) {
                motion.update(&operator_rig, p, animation_dt);
                if active && p.id != local.id && p.health > 0 && p.grounded {
                    for _ in 0..motion.footsteps() {
                        audio::play(
                            Cue::Footstep(Surface::Stone),
                            VoiceParams {
                                position: p.position.to_array(),
                                gain: 0.28,
                                priority: 1,
                                spatial: Spatial::Attenuated {
                                    min_distance: 1.,
                                    max_distance: 25.,
                                    rolloff: 0.45,
                                },
                                ..Default::default()
                            },
                        );
                    }
                }
            }
        }
        // Predicted local state drives the first-person body even when remote
        // interpolation includes an older copy of the local player.
        if let Some(motion) = operator_motion.get_mut(local.id as usize) {
            motion.update(&operator_rig, &local, animation_dt);
        }
        let magazine_events =
            weapon_motion.update(&local, if active { animation_dt } else { 0. }, fired);
        if active {
            for _ in 0..magazine_events {
                sound.cue(Cue::Click, 0.2);
            }
        }
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
        let camera_input = if active {
            Input3 {
                target: Target3::at(eye),
                mode: Mode3::FirstPerson {
                    eye_offset: Vec3::ZERO,
                    yaw: controls.yaw,
                    pitch: controls.pitch,
                },
                fov_y: 68_f32.to_radians(),
            }
        } else {
            let focus = vec3(-3., 1., -3.);
            Input3 {
                target: Target3::at(focus),
                mode: Mode3::Follow {
                    eye_offset: eye - focus,
                    look_offset: Vec3::ZERO,
                },
                fov_y: 68_f32.to_radians(),
            }
        };
        let view = view_rig
            .update(dt, camera_input, &NoObstructions)
            .expect("valid shooter view")
            .pose;
        let eye = view.position;
        let target = view.target;
        let camera = Camera3D {
            position: eye,
            target,
            up: view.up,
            fovy: view.fov_y,
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
        ferravex_render::set_sun_shadow(Some(ferravex_render::lighting::SunShadow {
            center: vec3(eye.x, 0., eye.z),
            direction: light.light_direction,
            extent: 34.,
            depth: 90.,
            ..Default::default()
        }))
        .unwrap();
        if shot_timer > 0. {
            ferravex_render::set_point_lights(&[ferravex_render::lighting::PointLight {
                position: eye + direction * 0.8,
                color: rgb(0xffb65c),
                radius: 5.,
                intensity: shot_timer * 18.,
            }])
            .unwrap();
        }
        if active && local.health > 0 {
            ferravex_render::draw_skinned_shadow(
                &operator,
                Mat4::from_rotation_translation(
                    Quat::from_rotation_y(-local.yaw),
                    Vec3::from_array(local.position.to_array()),
                ),
                operator_motion[local.id as usize].palette(),
            );
        }
        arena.draw();
        for p in &remote {
            if (active && p.id == local.id) || p.health == 0 {
                continue;
            }
            ferravex_render::draw_skinned(
                &operator,
                Mat4::from_rotation_translation(
                    Quat::from_rotation_y(-p.yaw),
                    Vec3::from_array(p.position.to_array()),
                ),
                operator_motion[p.id as usize].palette(),
            );
        }
        if active && local.health > 0 {
            let side = direction.cross(Vec3::Y).normalize_or_zero();
            let up = side.cross(direction).normalize_or_zero();
            let offset = eye + side * 0.27 - up * 0.25 + direction * 0.47;
            let rot = Quat::from_rotation_y(-controls.yaw) * Quat::from_rotation_x(controls.pitch);
            ferravex_render::draw_skinned(
                &rifle,
                Mat4::from_rotation_translation(rot, offset),
                weapon_motion.palette(),
            );
            if fired {
                let muzzle = offset
                    + rot
                        * weapon_motion
                            .palette()
                            .transform_point(
                                vec3(0., 0.05, -0.80),
                                ferravex_render::skinning::SkinWeights::rigid(0),
                            )
                            .expect("valid muzzle bone");
                effects.muzzle(muzzle);
            }
        }
        if let Ok(s) = &shader {
            s.end();
        }
        let effect_triangles = if active {
            effects.draw(ferravex_render::effects::BillboardView::from_camera(&camera).unwrap())
                as usize
        } else {
            0
        };
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
        let triangles = effect_triangles
            + arena.triangles()
            + remote
                .iter()
                .filter(|p| p.health > 0 && (!active || p.id != local.id))
                .count()
                * (operator.resident().vertex_count() / 3)
            + if active && local.health > 0 {
                rifle.resident().vertex_count() / 3
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
            if controls.input.held("roster") {
                hud::roster(&remote, local.id);
            }
        } else if (hud::menu(network, has_played) || controls.input.pressed("confirm"))
            && !controls.input.controls_open()
        {
            active = true;
            has_played = true;
            controls.touch_mode |= !ferravex_render::input::logical_touches().is_empty();
            set_cursor_grab(!controls.touch_mode);
            show_mouse(controls.touch_mode);
            clock.reset();
        }
        controls.input.draw_settings();
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
            println!(
                "STRIKE mode={} connected={} server_tick={} frames={} error={}",
                if network { "network" } else { "offline" },
                client.as_ref().is_some_and(|c| c.player_id().is_some()),
                server_tick,
                frames,
                error
            );
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
        let mut effects = CombatEffects::new();
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
            &mut effects,
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
                &mut effects,
                &mut hit,
                &mut damage,
            );
        }
        assert_eq!(effects.beams.len(), MAX_TRACERS);
        assert!(effects.particles.len() <= effects.particles.capacity());
        apply_game_events(
            [ferravex_fps::GameEvent::Hit {
                attacker: 1,
                victim: 0,
                damage: 34,
                headshot: false,
            }],
            0,
            &mut effects,
            &mut hit,
            &mut damage,
        );
        assert_eq!(damage, 0.25);
    }
}
