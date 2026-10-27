//! Original low-poly FPS with shared native UDP/browser byte-link authority.
//!
//! Practice uses the shared host lifecycle; match ticks still belong to the FPS
//! simulation. Network authority and replay recordings retain their own clocks.
//!
//! ```
//! use ferravex_core::session::GameSession;
//! use ferravex_fps::{Arena, Match, TICK_RATE};
//! let mut game = Match::new(Arena::default());
//! game.spawn_player(0);
//! let mut session = GameSession::new(TICK_RATE as f64, 8, false);
//! session.start();
//! for _ in 0..session.advance(1.0 / TICK_RATE as f64).steps {
//!     game.step(&[]);
//! }
//! session.set_paused(true);
//! assert_eq!(session.advance(30.0).steps, 0);
//! assert_eq!(game.tick, 1);
//! ```
mod animation;
mod bots;
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
mod capture;
mod cards;
mod controls;
#[cfg(test)]
mod corner_tests;
mod crew;
mod effects;
mod geometry;
mod host;
mod hud;
#[cfg(test)]
mod journey200_tests;
mod juice;
#[cfg(target_arch = "wasm32")]
mod multiplayer;
mod replay;
mod scoreboard;
mod style;

use controls::Controls;
use effects::CombatEffects;
use ferravex_core::FrameStats;
#[cfg(not(target_arch = "wasm32"))]
use ferravex_fps::GameClient;
use ferravex_fps::{Arena, Match, forward};
use ferravex_play::camera::{Input3, Mode3, NoObstructions, Rig3, Settings3, Target3};
use ferravex_render::audio::{self, Cue, GameAudio, Surface};
use ferravex_render::prelude::*;
use ferravex_render::shaders::{LitMaterial, LitSettings};
use geometry::{build_operator, build_yard, rgb};
use host::{Host, Mode};

fn window_conf() -> Conf {
    #[allow(unused_mut)]
    let mut conf = ferravex_render::window_conf("Ferravex / Strike — Dockyard");
    conf.window_width = 1440;
    conf.window_height = 900;
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    capture::window(&mut conf);
    conf
}

#[derive(Default)]
struct Options {
    #[cfg(not(target_arch = "wasm32"))]
    connect: Option<String>,
    smoke_frames: Option<u64>,
    #[cfg(not(target_arch = "wasm32"))]
    screenshot: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    capture_dir: Option<String>,
    play: bool,
    #[cfg(not(target_arch = "wasm32"))]
    record: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    replay: Option<String>,
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
                "--record" => out.record = args.next(),
                "--replay" => out.replay = args.next(),
                "--help" => {
                    println!(
                        "strike [--connect 127.0.0.1:4000] [--play] [--smoke-frames N] [--screenshot path.png] [--capture-dir directory] [--record clip.fxrt] [--replay clip.fxrt]"
                    );
                    std::process::exit(0);
                }
                _ => eprintln!("Unknown option: {arg}"),
            }
        }
    }
    if out.connect.is_some() && (out.record.is_some() || out.replay.is_some()) {
        eprintln!("Replay tools require offline practice; omit --connect");
        std::process::exit(2);
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

fn reset_practice(game: &mut Match, bots: &mut bots::PracticeBots, replays: &mut replay::Session) {
    let recording = replays.recording();
    replays.reset_practice();
    *game = practice();
    *bots = bots::PracticeBots::new(&game.arena);
    if recording {
        replays.start_record(game);
    }
}

const MAX_TRACERS: usize = 32;
// Both offline authority and replicated network events feed the same effects.
// Client prediction never manufactures a hit confirmation.
fn apply_game_events(
    events: impl IntoIterator<Item = ferravex_fps::GameEvent>,
    local_id: u8,
    effects: &mut CombatEffects,
    sound: &mut GameAudio,
) {
    for event in events {
        match event {
            ferravex_fps::GameEvent::Shot {
                player,
                origin,
                end,
            } => {
                if player == local_id {
                    if effects.local_shot(origin) {
                        sound.cue(Cue::Fire, 0.75);
                    }
                } else {
                    sound.at(Cue::Fire, origin, 0.75);
                }
                effects.shot(player != local_id, origin, end);
            }
            ferravex_fps::GameEvent::Hit {
                attacker, victim, ..
            } => {
                effects.hit(attacker == local_id, victim == local_id);
            }
            ferravex_fps::GameEvent::Killed { victim, .. } if victim == local_id => {
                sound.cue(Cue::Failure, 0.5);
            }
            _ => {}
        }
    }
}

/// Accepted events also drive match bookkeeping and presentation-only juice.
/// `tick` is the authority/replay/server tick after these events.
#[allow(clippy::too_many_arguments)]
fn present_events(
    events: &[ferravex_fps::GameEvent],
    tick: u32,
    local_id: u8,
    bots: bool,
    board: &mut scoreboard::Tracker,
    juice: &mut juice::Juice,
    effects: &mut CombatEffects,
    sound: &mut GameAudio,
) {
    apply_game_events(events.iter().copied(), local_id, effects, sound);
    for event in events {
        if let ferravex_fps::GameEvent::Hit {
            attacker,
            victim,
            damage,
            headshot,
        } = *event
        {
            if attacker == local_id && victim != local_id {
                juice.hit(headshot, sound);
            } else if victim == local_id {
                juice.hurt(damage);
            }
        }
    }
    board.observe(tick, events, |moment| {
        juice.moment(moment, local_id, bots, sound);
    });
}

/// The live practice match has ended and its results own the view.
fn practice_over(host: &Host, replays: &replay::Session) -> bool {
    host.finished() && !replays.playing()
}

fn practice_board() -> scoreboard::Tracker {
    scoreboard::Tracker::new(0, true)
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
    let mut bots = bots::PracticeBots::new(&game.arena);
    let mut commands = Vec::with_capacity(ferravex_fps::MAX_PLAYERS);
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
    let weapons = geometry::Weapons::new();
    let objectives = geometry::Objectives::new();
    let operator_rig = animation::OperatorRig::new();
    let mut operator_motion: Vec<_> = (0..ferravex_fps::MAX_PLAYERS)
        .map(|_| animation::OperatorMotion::new(&operator_rig))
        .collect();
    let mut weapon_motion = animation::WeaponMotion::new();
    let mut look_state = style::LookState::default();
    let mut effects = CombatEffects::new();
    let duck = geometry::build_duck();
    let mut juice = juice::Juice::new();
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    let mut capture = capture::Capture::from_env();
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    let new_board =
        |capture: &capture::Capture| practice_board().with_seconds(capture.match_seconds);
    #[cfg(not(all(debug_assertions, not(target_arch = "wasm32"))))]
    let new_board = |_: &()| practice_board();
    #[cfg(not(all(debug_assertions, not(target_arch = "wasm32"))))]
    let capture = ();
    // The live practice match and a clockless view for network/replay events.
    let mut board = new_board(&capture);
    let mut tuning = bots::Tuning::new();
    let mut view_board = scoreboard::Tracker::new(0, false);
    let mut onboarding = juice::Onboarding::default();
    let mut info_rows: [String; 3] = Default::default();
    // Personal bests are optional: storage failure still leaves a playable game.
    let mut records = ferravex_render::storage::Records::new("strike").ok();
    let mut results: Option<cards::Results> = None;
    #[cfg(not(target_arch = "wasm32"))]
    let mut net_events = Vec::with_capacity(64);

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
    #[cfg(not(target_arch = "wasm32"))]
    let network = opts.connect.is_some();
    #[cfg(target_arch = "wasm32")]
    let mut webnet = multiplayer::Session::default();
    let mut host = Host::new(opts.play || ferravex_render::autoplay_enabled());
    let mut diagnostics = false;
    let mut browser_playing = host.active();
    sync_browser_ui(host.active());
    let mut controls = Controls::default();
    let mut hud_ui = ferravex_render::ui::Ui::new();
    let mut menu_fx = hud::MenuFx::new();
    let mut replays = replay::Session::default();
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = &opts.replay {
        match ferravex_render::storage::read_file(std::path::Path::new(path), 8 * 1024 * 1024) {
            Ok(Some(bytes)) => {
                if let Err(e) = replays.import(&bytes, &game) {
                    error = format!("Replay: {e}");
                }
            }
            other => error = format!("Replay read failed: {other:?}"),
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    if opts.record.is_some() {
        replays.start_record(&game);
    }
    let mut sound = GameAudio::new();
    let mut was_reload = 0;
    let mut stats = FrameStats::<120>::default();
    let mut frames = 0u64;
    let mut last_command = None;
    let mut command_notice: Option<(&'static str, f32)> = None;
    let mut local = game.players[0].clone();
    let mut remote = game.players.clone();
    let mut server_tick = 0;
    let mut previous_local_position = local.position;
    let mut view_rig = Rig3::new(Settings3 {
        position_half_life: 0.,
        target_half_life: 0.,
        fov_half_life: 0.,
        teleport_distance: Some(2.),
        ..Default::default()
    })
    .unwrap();
    if host.active() && opts.smoke_frames.is_none() {
        set_cursor_grab(true);
        show_mouse(false);
    }

    let mut replay_initial_cut = replays.playing();
    loop {
        let dt = if opts.smoke_frames.is_some() {
            1.0 / 60.0
        } else {
            get_frame_time().min(0.25)
        };
        stats.record(get_frame_time());
        if let Some((_, time)) = &mut command_notice {
            *time -= dt;
            if *time <= 0. {
                command_notice = None;
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            controls.show_interact = client.as_ref().and_then(|c| c.tactical_view()).is_some();
        }
        #[cfg(target_arch = "wasm32")]
        {
            controls.show_interact = webnet.round().is_some();
        }
        controls.prepare(host.active());
        // Debug captures may ignore window focus so parallel desktop activity
        // cannot pause a scripted screenshot run. Release/Wasm: always false.
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        let keep_focus = capture.keep_focus;
        #[cfg(not(all(debug_assertions, not(target_arch = "wasm32"))))]
        let keep_focus = false;
        macro_rules! gameplay {
            () => {
                keep_focus || controls.input.gameplay_active()
            };
        }
        effects.begin_frame();
        let policy = ferravex_render::preferences::feedback_policy();
        weapon_motion.set_reduced_motion(policy.reduced_motion() || policy.intensity() == 0.);
        juice.apply_policy(policy);
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        if capture.pause_now() && host.active() {
            host.open_menu();
        }
        if controls.input.pressed("pause") {
            if host.active() {
                host.open_menu();
            } else if !practice_over(&host, &replays) {
                host.resume();
                controls.input.sample(true);
                view_rig.cut();
            }
        }
        if host.active() && !gameplay!() {
            // Focus loss and settings must not silently resume firing when the
            // browser/native window becomes active again.
            host.open_menu();
        }
        #[cfg(target_arch = "wasm32")]
        let finished_practice = practice_over(&host, &replays) && !webnet.active();
        #[cfg(not(target_arch = "wasm32"))]
        let finished_practice = practice_over(&host, &replays) && !network;
        if host.active() && finished_practice {
            // Leaving a replay returns to the results, not a frozen world.
            host.open_menu();
        }
        let active = host.active();
        if active && gameplay!() {
            sound.resume();
        } else {
            sound.pause();
        }
        if controls.input.pressed("diagnostics") {
            diagnostics = !diagnostics;
        }
        #[cfg(target_arch = "wasm32")]
        let net_changed = {
            let changed = webnet.before_frame();
            if changed && webnet.active() {
                replays.stop_record();
                if replays.playing() {
                    replays.toggle_playback(&game);
                }
            }
            if webnet.active() {
                if let Some(p) = webnet.local() {
                    local = p.clone();
                }
                remote = webnet.players();
                server_tick = webnet.tick_number();
                let local_id = local.id;
                view_board.set_local(local_id);
                let events = webnet.drain_events();
                present_events(
                    &events,
                    server_tick,
                    local_id,
                    false,
                    &mut view_board,
                    &mut juice,
                    &mut effects,
                    &mut sound,
                );
            }
            changed
        };
        #[cfg(target_arch = "wasm32")]
        let network = webnet.active();
        let mut cut_replay = replays.controls(&mut controls.input, &game, network);
        #[cfg(target_arch = "wasm32")]
        {
            cut_replay |= net_changed;
        }
        cut_replay |= std::mem::take(&mut replay_initial_cut);
        cut_replay |= replays.browser_poll(&game, network);
        cut_replay |= replays.pump();
        let mode = if network {
            Mode::Network
        } else if replays.playing() {
            Mode::Replay
        } else {
            Mode::Practice
        };
        if cut_replay {
            controls.cut();
            command_notice = None;
            last_command = None;
            if !network {
                let source = replays.world.as_ref().map_or(&game, |w| &w.game);
                local = source
                    .player(0)
                    .cloned()
                    .unwrap_or_else(|| game.players[0].clone());
                remote.clone_from(&source.players);
                server_tick = source.tick;
            }
            controls.yaw = local.yaw;
            controls.pitch = local.pitch;
            previous_local_position = local.position;
            was_reload = local.reload_ticks;
            effects = CombatEffects::new();
            effects.begin_frame();
            juice.reset();
            view_board = scoreboard::Tracker::new(local.id, false);
            sound.reset();
            host.cut_replay();
            view_rig.cut();
            for motion in &mut operator_motion {
                *motion = animation::OperatorMotion::new(&operator_rig);
            }
            weapon_motion = animation::WeaponMotion::new();
            weapon_motion.set_reduced_motion(policy.reduced_motion() || policy.intensity() == 0.);
        }
        #[cfg_attr(target_arch = "wasm32", allow(unused_mut))]
        let mut input = controls.sample(active);
        if replays.playing() {
            controls.yaw = local.yaw;
            controls.pitch = local.pitch;
        }
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        if active
            && mode == Mode::Practice
            && let Some((yaw, pitch, fire)) = capture.aim(&game, board.duck_found())
        {
            controls.yaw = yaw;
            controls.pitch = pitch;
            input.yaw = yaw;
            input.pitch = pitch;
            input.buttons = if fire { ferravex_fps::BUTTON_FIRE } else { 0 };
            if capture.walk() {
                input.move_z = 1.;
            }
        }
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
        let frame = host.advance(mode, dt as f64, gameplay!(), !cut_replay && !replays.paused);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(c) = client.as_mut() {
            if let Err(e) = c.poll() {
                error = e.to_string();
            }
            let local_id = c.player_id().unwrap_or(local.id);
            view_board.set_local(local_id);
            net_events.clear();
            net_events.extend(c.drain_events().into_iter().map(|message| message.event));
            present_events(
                &net_events,
                c.server_tick(),
                local_id,
                false,
                &mut view_board,
                &mut juice,
                &mut effects,
                &mut sound,
            );
        }
        #[cfg(not(target_arch = "wasm32"))]
        let action_result = client.as_ref().and_then(|c| c.last_action_result());
        #[cfg(target_arch = "wasm32")]
        let action_result = webnet.action_result();
        match action_result {
            Some((sequence, result)) if last_command != Some(sequence) => {
                last_command = Some(sequence);
                command_notice = result.err().map(|e| (hud::rule_feedback(e), 2.5));
            }
            None => last_command = None,
            _ => {}
        }
        for _ in 0..frame.steps {
            let input = host.command(controls.tick(input), gameplay!());
            #[cfg(not(target_arch = "wasm32"))]
            let actions = controls.tactical_actions(
                client.as_ref().and_then(|c| c.tactical_view()),
                &local,
                active,
            );
            #[cfg(target_arch = "wasm32")]
            let actions = controls.tactical_actions(webnet.round(), &local, active);
            for action in actions.into_iter().flatten() {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(c) = &mut client
                    && let Err(e) = c.queue_action(action)
                {
                    error = e.to_string();
                }
                #[cfg(target_arch = "wasm32")]
                webnet.act(action);
            }
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
            #[cfg(target_arch = "wasm32")]
            if webnet.active() {
                let events = webnet.tick(input);
                if let Some(p) = webnet.local() {
                    local = p.clone();
                }
                remote = webnet.players();
                server_tick = webnet.tick_number();
                present_events(
                    &events,
                    server_tick,
                    local.id,
                    false,
                    &mut view_board,
                    &mut juice,
                    &mut effects,
                    &mut sound,
                );
                continue;
            }
            if replays.playing() {
                if !cut_replay && replays.advance() {
                    let w = replays.world.as_ref().unwrap();
                    present_events(
                        &w.events,
                        w.game.tick,
                        0,
                        true,
                        &mut view_board,
                        &mut juice,
                        &mut effects,
                        &mut sound,
                    );
                    local = w
                        .game
                        .player(0)
                        .cloned()
                        .unwrap_or_else(|| game.players[0].clone());
                    remote.clone_from(&w.game.players);
                    server_tick = w.game.tick;
                    controls.yaw = local.yaw;
                    controls.pitch = local.pitch;
                }
                continue;
            }
            if !network && active && gameplay!() {
                bots.commands(&game, input, &mut commands);
                tuning.adjust(&game, &mut commands);
                let events = game.step(&commands);
                replays.capture(&game, &commands, &events);
                bots.events(&events);
                tuning.events(game.tick, &events);
                present_events(
                    &events,
                    game.tick,
                    0,
                    true,
                    &mut board,
                    &mut juice,
                    &mut effects,
                    &mut sound,
                );
                local = game.players[0].clone();
                remote.clone_from(&game.players);
                server_tick = game.tick;
                if board.finished() && !host.finished() {
                    // Automation never claims or overwrites a personal best.
                    #[cfg(not(target_arch = "wasm32"))]
                    let recorded = opts.smoke_frames.is_none() && opts.capture_dir.is_none();
                    #[cfg(target_arch = "wasm32")]
                    let recorded = true;
                    let stored = records
                        .as_ref()
                        .and_then(|r| r.get(cards::RECORD_ID, cards::RECORD_RULES))
                        .map(|r| (r.score, r.runs));
                    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
                    let stored = capture.fake_best.map(|b| (b, 3)).or(stored);
                    if recorded && let Some(records) = records.as_mut() {
                        let _ = records.attempt(
                            cards::RECORD_ID,
                            cards::RECORD_RULES,
                            u64::from(board.summary.frags),
                            None,
                        );
                    }
                    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
                    let recorded = recorded || capture.fake_best.is_some();
                    let r = cards::Results::new(&board, stored.map(|(best, _)| best), recorded);
                    sound.cue(
                        if r.new_best {
                            Cue::Success
                        } else {
                            Cue::Pickup
                        },
                        0.7,
                    );
                    host.finish_match(board.summary.frags > board.summary.deaths);
                    results = Some(r);
                    break;
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(c) = client.as_ref() {
            remote = c.rendered_players();
            if !c.is_connected() {
                error = "Connecting to server...".into();
            } else if error == "Connecting to server..." {
                error.clear();
            }
        }
        let fired = effects.fired;
        onboarding.fired |= fired && mode == Mode::Practice;
        onboarding.roster |= active && controls.input.held("roster");
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        if capture.learned {
            onboarding = juice::Onboarding {
                fired: true,
                roster: true,
            };
        }
        if !cut_replay && local.reload_ticks > 0 && was_reload == 0 {
            sound.cue(Cue::Reload, 0.6);
        }
        was_reload = local.reload_ticks;
        audio::listener(
            Vec3::from_array(local.eye_position().to_array()),
            vec3(controls.yaw.cos(), 0., controls.yaw.sin()),
        );
        sound.walk(
            Vec3::from_array(local.position.to_array()),
            local.grounded,
            active && gameplay!() && local.health > 0,
            Surface::Stone,
            1.65,
        );
        effects.update(dt);
        if let Some(playback) = &replays.playback {
            ferravex_render::performance::context(Some(playback.position()), replays.diagnostic_id);
        } else if !network {
            ferravex_render::performance::context(Some(u64::from(game.tick)), None);
        } else {
            ferravex_render::performance::context(None, None);
        }
        let animation_dt = if cut_replay
            || (mode == Mode::Practice && (!active || !gameplay!()))
            || (replays.playing() && replays.paused)
        {
            0.
        } else if active {
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
                motion.ground_contacts(&operator_rig, p, &game.arena, animation_dt);
                if active && p.id != local.id && p.health > 0 && p.grounded {
                    for _ in 0..motion.footsteps() {
                        sound.at(Cue::Footstep(Surface::Stone), p.position, 0.28);
                    }
                }
            }
        }
        // Predicted local state drives the first-person body even when remote
        // interpolation includes an older copy of the local player.
        if let Some(motion) = operator_motion.get_mut(local.id as usize) {
            motion.update(&operator_rig, &local, animation_dt);
            motion.ground_contacts(&operator_rig, &local, &game.arena, animation_dt);
        }
        #[cfg(not(target_arch = "wasm32"))]
        let round = client.as_ref().and_then(|c| c.tactical_view());
        #[cfg(target_arch = "wasm32")]
        let round = webnet.round();
        juice.update(animation_dt);
        juice.tactical(round, local.id, &mut sound);
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
            let sway = if policy.reduced_motion() || policy.intensity() == 0. {
                0.
            } else {
                (get_time() as f32 * 0.035).sin() * 2. * policy.intensity()
            };
            vec3(17. + sway, 11., 24.)
        };
        let direction = Vec3::from_array(forward(controls.yaw, controls.pitch).to_array());
        let camera_input = if active {
            // The damage kick moves only the camera; commands keep their aim.
            let (kick_yaw, kick_pitch) = juice.kick();
            Input3 {
                target: Target3::at(eye),
                mode: Mode3::FirstPerson {
                    eye_offset: Vec3::ZERO,
                    yaw: controls.yaw + kick_yaw,
                    pitch: controls.pitch + kick_pitch,
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
        if effects.muzzle_timer > 0. {
            ferravex_render::set_point_lights(&[ferravex_render::lighting::PointLight {
                position: eye + direction * 0.8,
                color: rgb(0xffb65c),
                radius: 5.,
                intensity: effects.muzzle_timer * 18.,
            }])
            .unwrap();
        } else {
            ferravex_render::set_point_lights(&[]).unwrap();
        }
        if let Some(g) = replays.ghost_sample(frame.alpha) {
            replay::draw_ghost(g);
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
        if !network
            && let Some((position, yaw, scale)) =
                juice.duck(mode == Mode::Practice && board.duck_found())
        {
            ferravex_render::draw_resident(
                duck.resident(),
                Mat4::from_scale_rotation_translation(
                    Vec3::splat(scale),
                    Quat::from_rotation_y(yaw),
                    position,
                ),
            );
        }
        if let Some(round) = round {
            objectives.draw(round);
        }
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
        let kind = round
            .and_then(|r| r.player(local.id))
            .map_or(ferravex_play::rounds::Weapon::Rifle, |p| p.weapon);
        let (weapon, muzzle_local) = weapons.select(kind);
        if active && local.health > 0 {
            let side = direction.cross(Vec3::Y).normalize_or_zero();
            let up = side.cross(direction).normalize_or_zero();
            let offset = eye + side * 0.27 - up * 0.25 + direction * 0.47;
            let rot = Quat::from_rotation_y(-controls.yaw) * Quat::from_rotation_x(controls.pitch);
            ferravex_render::draw_skinned_viewmodel(
                weapon,
                Mat4::from_rotation_translation(rot, offset),
                weapon_motion.palette(),
            );
            if fired {
                let muzzle = offset
                    + rot
                        * weapon_motion
                            .palette()
                            .transform_point(
                                muzzle_local,
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
        look_state.apply(if !active {
            style::Look::Menu
        } else if local.health == 0 {
            style::Look::Down
        } else {
            style::Look::Play
        });
        set_default_camera();
        if effects.damage_flash > 0. {
            draw_rectangle(
                0.,
                0.,
                screen_width(),
                screen_height(),
                Color::new(0.6, 0.12, 0.05, effects.damage_flash),
            );
        }
        if active {
            juice.draw_glow();
        }
        let triangles = effect_triangles
            + arena.triangles()
            + remote
                .iter()
                .filter(|p| p.health > 0 && (!active || p.id != local.id))
                .count()
                * (operator.resident().vertex_count() / 3)
            + if active && local.health > 0 {
                weapon.resident().vertex_count() / 3
            } else {
                0
            };
        hud_ui.begin(
            ferravex_render::ui::Viewport::fit(
                [screen_width(), screen_height()],
                [screen_width(), screen_height()],
                [0.; 4],
            )
            .unwrap(),
        );
        hud::overlay(
            &mut hud_ui,
            &local,
            hud::HudState {
                active,
                touch_mode: controls.touch_mode,
                diagnostics,
                frame_ms: stats.mean_ms(),
                triangles,
                tick: server_tick,
                hit_timer: effects.hit_timer,
                shot_timer: effects.shot_timer,
                input: &controls.input,
                round_phase: round.map(|r| r.phase),
                error: if error.is_empty() {
                    command_notice.map_or("", |n| n.0)
                } else {
                    &error
                },
                weapon: match kind {
                    ferravex_play::rounds::Weapon::Rifle => "Rifle",
                    ferravex_play::rounds::Weapon::Pistol => "Pistol",
                    ferravex_play::rounds::Weapon::Sniper => "Sniper",
                },
                marker: juice.marker,
                death: juice.death,
            },
        );
        #[cfg(not(target_arch = "wasm32"))]
        {
            let round = client.as_ref().and_then(|c| c.tactical_view());
            let lobby = client.as_ref().and_then(|c| c.tactical_lobby());
            hud::tactical(&mut hud_ui, round, lobby, local.id, active, &controls.input);
        }
        #[cfg(target_arch = "wasm32")]
        hud::tactical(
            &mut hud_ui,
            webnet.round(),
            webnet.lobby(),
            local.id,
            active,
            &controls.input,
        );
        let footer = (!network || diagnostics) && replays.footer_visible(diagnostics);
        let bots_named = !network;
        let shown_board = if mode == Mode::Practice {
            &board
        } else {
            &view_board
        };
        if active {
            #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
            let roster = controls.input.held("roster") || capture.roster();
            #[cfg(not(all(debug_assertions, not(target_arch = "wasm32"))))]
            let roster = controls.input.held("roster");
            let hint_kind = if mode == Mode::Practice && !roster && local.health > 0 {
                onboarding.hint(board.summary.frags)
            } else {
                None
            };
            let mut key_a = [0; 64];
            let mut key_b = [0; 64];
            let hint = hint_kind.map(|kind| {
                let (a, b) = match kind {
                    juice::HintKind::FireReload => ("fire", "reload"),
                    juice::HintKind::Scoreboard => ("roster", "roster"),
                };
                juice::Hint {
                    kind,
                    keys: [
                        controls::hint(&controls.input, a, &mut key_a),
                        controls::hint(&controls.input, b, &mut key_b),
                    ],
                }
            });
            juice.draw_play(
                &board,
                juice::PlayLayout {
                    practice: mode == Mode::Practice,
                    round: round.is_some(),
                    touch: controls.touch_mode,
                    notice: !(error.is_empty() && command_notice.is_none()),
                    footer,
                    roster,
                    alive: local.health > 0,
                    hint,
                    local: local.id,
                    bots: bots_named,
                },
            );
            controls.draw_touch();
            if roster {
                hud::roster(&mut hud_ui, &remote, local.id, shown_board, bots_named);
            }
        } else if let Some(r) = results.as_ref().filter(|_| finished_practice) {
            cards::results(&mut hud_ui, r);
        } else {
            hud::menu(
                &mut hud_ui,
                mode,
                host.has_played(),
                controls.touch_mode,
                &controls.input,
            );
            if mode == Mode::Practice {
                let (left, bottom) = hud::menu_extent(
                    screen_width(),
                    screen_height(),
                    mode,
                    host.has_played(),
                    controls.touch_mode,
                    &controls.input,
                );
                let best = records
                    .as_ref()
                    .and_then(|r| r.get(cards::RECORD_ID, cards::RECORD_RULES))
                    .map(|r| (r.score, r.runs));
                let colors = if host.has_played() {
                    cards::pause_rows(&board, best.map(|b| b.0), game.tick / 640, &mut info_rows);
                    cards::PAUSE_COLORS
                } else {
                    cards::briefing_rows(best, &controls.input, &mut info_rows);
                    cards::BRIEFING_COLORS
                };
                cards::info(&mut hud_ui, left, bottom, &info_rows, colors);
            }
        }
        if footer && !finished_practice {
            replays.overlay(&mut hud_ui, diagnostics, &controls.input);
        }
        hud_ui.finish_player(&mut controls.input, !active);
        menu_fx.update(
            &hud_ui,
            !active && !finished_practice,
            dt.clamp(0., 0.1),
            style::motion(),
        );
        hud_ui.paint();
        if !active && !finished_practice {
            hud::menu_overlay(
                &menu_fx,
                mode,
                host.has_played(),
                controls.touch_mode,
                &controls.input,
            );
        }
        if !active && hud_ui.activated(1) {
            host.resume();
            controls.touch_mode |= !ferravex_render::input::logical_touches().is_empty();
            view_rig.cut();
        }
        if !active && hud_ui.activated(2) && host.restart_practice(mode) {
            reset_practice(&mut game, &mut bots, &mut replays);
            board = new_board(&capture);
            tuning = bots::Tuning::new();
            results = None;
            replay_initial_cut = true;
        }
        if !active && hud_ui.activated(3) && mode != Mode::Network {
            if mode == Mode::Replay {
                replays.toggle_playback(&game);
            } else {
                replays.stop_record();
                reset_practice(&mut game, &mut bots, &mut replays);
                board = new_board(&capture);
                tuning = bots::Tuning::new();
                results = None;
                host.return_to_briefing();
            }
            replay_initial_cut = true;
        }
        replays.browser_sync();
        #[cfg(target_arch = "wasm32")]
        webnet.sync();
        controls.input.draw_settings();
        let capture_mouse = host.active()
            && !controls.touch_mode
            && !controls.input.controls_open()
            && opts.smoke_frames.is_none();
        set_cursor_grab(capture_mouse);
        show_mouse(!capture_mouse);
        if browser_playing != host.active() {
            sync_browser_ui(host.active());
            browser_playing = host.active();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(directory) = &opts.capture_dir {
            std::fs::create_dir_all(directory).expect("create capture directory");
            get_screen_data().export_png(&format!("{directory}/{frames:06}.png"));
        }
        frames += 1;
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        capture.end_frame();
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
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(path) = &opts.record
        && let Some(bytes) = replays.export()
    {
        let path = std::path::Path::new(path);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir().unwrap().join(path)
        };
        match ferravex_render::storage::write_file(&path, &bytes) {
            Ok(()) => println!(
                "REPLAY exported {} bytes to {}",
                bytes.len(),
                path.display()
            ),
            Err(e) => eprintln!("Replay export failed: {e:?}"),
        }
    }
}

#[cfg(test)]
mod event_effect_tests {
    use super::*;
    #[test]
    fn actual_accepted_shots_drive_feedback_and_rejected_fire_stays_silent() {
        let mut game = practice();
        let mut effects = CombatEffects::new();
        let mut sound = GameAudio::new();
        let fire = [(
            0,
            ferravex_fps::InputFrame {
                buttons: ferravex_fps::BUTTON_FIRE,
                ..Default::default()
            },
        )];
        effects.begin_frame();
        let events = game.step(&fire);
        let hash = game.state_hash();
        apply_game_events(events, 0, &mut effects, &mut sound);
        assert!(effects.fired);
        assert_eq!(game.state_hash(), hash);
        effects.update(0.);
        effects.begin_frame();
        apply_game_events(game.step(&fire), 0, &mut effects, &mut sound);
        assert!(!effects.fired, "cooldown rejects another shot");
        game.players[0].fire_cooldown = 0;
        game.players[0].ammo = 0;
        apply_game_events(game.step(&fire), 0, &mut effects, &mut sound);
        assert!(!effects.fired, "empty magazine rejects fire");
        sound.pause();
        let before = audio::stats().active;
        apply_game_events(
            [ferravex_fps::GameEvent::Shot {
                player: 1,
                origin: Vec3::Y,
                end: Vec3::ZERO,
            }],
            0,
            &mut effects,
            &mut sound,
        );
        assert_eq!(
            audio::stats().active,
            before,
            "menu audio is suppressed independently"
        );
    }

    #[test]
    fn shots_and_other_players_hits_never_confirm_a_local_hit() {
        let mut effects = CombatEffects::new();
        effects.begin_frame();
        let mut sound = GameAudio::new();
        apply_game_events(
            [
                ferravex_fps::GameEvent::Shot {
                    player: 0,
                    origin: [0., 1., 0.].into(),
                    end: [0., 1., -5.].into(),
                },
                ferravex_fps::GameEvent::Hit {
                    attacker: 2,
                    victim: 3,
                    damage: 34,
                    headshot: false,
                },
            ],
            0,
            &mut effects,
            &mut sound,
        );
        assert_eq!(effects.hit_timer, 0.);
        effects.update(0.);
        assert_eq!(effects.damage_flash, 0.);
    }

    #[test]
    fn replicated_hits_confirm_only_authority_and_remote_tracers_are_bounded() {
        let mut effects = CombatEffects::new();
        effects.begin_frame();
        let mut sound = GameAudio::new();
        apply_game_events(
            [ferravex_fps::GameEvent::Hit {
                attacker: 0,
                victim: 1,
                damage: 34,
                headshot: false,
            }],
            0,
            &mut effects,
            &mut sound,
        );
        assert_eq!(effects.hit_timer, 0.25);
        effects.update(0.);
        assert_eq!(effects.damage_flash, 0.);
        for player in std::iter::repeat_n(1, MAX_TRACERS + 10).chain([0]) {
            apply_game_events(
                [ferravex_fps::GameEvent::Shot {
                    player,
                    origin: [0., 1., 0.].into(),
                    end: [0., 1., -5.].into(),
                }],
                0,
                &mut effects,
                &mut sound,
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
            &mut sound,
        );
        effects.update(0.);
        assert!(effects.damage_flash > 0.);
    }
}
