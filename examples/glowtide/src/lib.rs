//! Glowtide: a bioluminescent bullet-heaven roguelite for Ferravex.
//!
//! A tiny lantern spirit survives a ten-minute tide (or a five-minute quick
//! tide) of glowing sea creatures. Weapons fire on their own; you steer, dash
//! and choose upgrades. Rules live in [`sim`] and run on fixed ticks with a
//! seeded RNG; the host here only samples input, renders and saves.
//!
//! ```
//! use glowtide::sim::{Game, Input, Phase, RunConfig, bot};
//! let mut game = Game::new(RunConfig { seed: 7, ..RunConfig::default() });
//! for _ in 0..600 {
//!     let input = bot::autoplay(&game);
//!     game.update(input);
//! }
//! assert!(game.stats.kills > 0, "ten seconds in, the wisps have found someone");
//! assert!(matches!(game.phase, Phase::Running | Phase::LevelUp | Phase::Chest));
//! // A dash is an input edge consumed by a single tick.
//! let mut game = Game::new(RunConfig::default());
//! game.update(Input { dash: true, ..Input::default() });
//! assert_eq!(game.stats.dashes, 1);
//! ```
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
mod capture;
mod fx;
mod gfx;
mod hud;
mod icons;
mod layout;
mod menus;
pub mod meta;
pub mod run;
mod sfx;
pub mod sim;
mod text;
mod world;

use ferravex_play::tween::{
    Motion, RollCounter, ScreenTransition, TransitionEvent, TransitionKind,
};
use ferravex_render::demo::Demo;
use ferravex_render::effects::{
    EffectBlend, EffectStyle, ParticleRenderer, ParticleShape, ParticleStyle,
};
use ferravex_render::input::{ActionKind, Binding, HintDevice, InputCode, PlayerInput, Preset};
use ferravex_render::post::{self, PostFx};
use ferravex_render::prelude::*;
use ferravex_render::storage::{Records, Slot};
use ferravex_render::ui::{Ui, kit};
use fx::Fx;
use gfx::Painter;
use layout::Layout;
use menus::{Buttons, CardView, Menu, Summary, id};
use meta::Meta;
use run::Run;
use sfx::SoundKit;
use sim::content::{CHARACTERS, WeaponId};
use sim::{Card, Choice, Event, Game, Input, MAX_EVENTS, Phase, RunConfig};
use text::Text;
use world::{Scene, View};

fn conf() -> Conf {
    #[allow(unused_mut)]
    let mut conf = ferravex_render::window_conf("Glowtide · Ferravex");
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    capture::window(&mut conf);
    conf
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Title,
    Select,
    Shrine,
    Play,
}

/// A screen change that waits for the iris transition to cover the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Swap {
    To(Screen),
    StartRun,
    ToTitle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Grade {
    Menu,
    Run,
    LightsOut,
}

/// Bloom tuned for a dark sea: a low threshold so bioluminescence blooms,
/// a wide radius for underwater haze, and a vignette that frames the lantern.
fn grade(kind: Grade) -> PostFx {
    let fx = PostFx::neon()
        .bloom(1.7, 0.52, 1.35)
        .bloom_knee(0.35)
        .vignette(0.36, 0.6);
    match kind {
        Grade::Menu => fx.bloom(2.1, 0.5, 1.45),
        Grade::Run => fx,
        Grade::LightsOut => fx.vignette(0.55, 0.5).exposure(0.92),
    }
}

fn add_actions(input: &mut PlayerInput) {
    let key = |k: KeyCode| Binding::digital(InputCode::Key(k as u16));
    for (name, label, k) in [
        ("pick_1", "Pick card 1", KeyCode::Key1),
        ("pick_2", "Pick card 2", KeyCode::Key2),
        ("pick_3", "Pick card 3", KeyCode::Key3),
        ("pick_4", "Pick card 4", KeyCode::Key4),
    ] {
        input
            .global_action(name, label, ActionKind::Button, vec![key(k)])
            .expect("authored action");
    }
    input
        .global_action(
            "reroll",
            "Reroll cards",
            ActionKind::Button,
            vec![key(KeyCode::E), Binding::digital(InputCode::PadButton(3))],
        )
        .expect("authored action");
    input
        .global_action(
            "banish",
            "Banish a card",
            ActionKind::Button,
            vec![key(KeyCode::Q), Binding::digital(InputCode::PadButton(2))],
        )
        .expect("authored action");
}

/// A lively background run for the title screen.
fn attract_game(seed: u64) -> Game {
    let mut game = Game::new(RunConfig {
        character: (seed % CHARACTERS.len() as u64) as u8,
        seed,
        ..RunConfig::default()
    });
    game.time = 140.;
    for card in [
        Card::Weapon(WeaponId::HaloOrbit),
        Card::Weapon(WeaponId::ChainSpark),
        Card::Weapon(WeaponId::LanternAura),
        Card::Weapon(WeaponId::HaloOrbit),
        Card::Weapon(WeaponId::HaloOrbit),
        Card::Weapon(WeaponId::ChainSpark),
    ] {
        game.apply_card(card);
    }
    game
}

fn seed_now(salt: u64) -> u64 {
    let t = (get_time() * 1_000_003.) as u64;
    t ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x5eed
}

struct Host {
    meta: Meta,
    slot: Option<Slot<Meta>>,
    records: Option<Records>,
}

impl Host {
    fn save(&mut self) {
        if let Some(slot) = &mut self.slot {
            let _ = slot.commit(self.meta.clone());
        }
    }
}

ferravex_render::game!(conf, game);
async fn game() {
    let mut demo = Demo::new();
    let mut sound = SoundKit::new();
    let mut fx = Fx::new();
    let mut painter = Painter::new();
    let mut particles = ParticleRenderer::new(2200).expect("bounded particle scratch");
    let slot = Slot::open(meta::OWNER, "meta", Meta::default()).ok();
    let mut host = Host {
        meta: slot.as_ref().map(|s| s.value().clone()).unwrap_or_default(),
        slot,
        records: Records::new(meta::OWNER).ok(),
    };
    let mut input = PlayerInput::preset("glowtide", Preset::TopDown);
    add_actions(&mut input);
    let mut ui_menu = Ui::new();
    let mut tx = Text::new();
    let mut buttons = Buttons::new();
    let mut hud_state = hud::HudState::new();
    let mut transition = ScreenTransition::new(TransitionKind::Iris, 0.42);
    let mut swap: Option<Swap> = None;
    let mut graded: Option<Grade> = None;
    let mut pause_age = 0_f32;
    let mut events: Vec<Event> = Vec::with_capacity(MAX_EVENTS);
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    let mut capture = capture::Capture::from_env();
    let mut config = RunConfig {
        character: host.meta.character,
        quick: host.meta.quick,
        seed: if demo.autoplay {
            0x6c75_6d69
        } else {
            seed_now(1)
        },
        bonuses: host.meta.bonuses(),
    };
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    {
        if let Some(c) = capture.character {
            config.character = c.min(3);
        }
        config.quick |= capture.quick;
        if let Some(s) = capture.seed {
            config.seed = s;
        }
        if let Some(g) = capture.glimmer {
            host.meta.glimmer = g;
        }
    }
    let mut run = Run::new(config, demo.autoplay);
    // Native benchmark workload: a sustained late-game horde (release builds too).
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(count) = std::env::var("GLOWTIDE_STRESS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        run.game.time = 420.;
        for card in [
            Card::Weapon(WeaponId::HaloOrbit),
            Card::Weapon(WeaponId::LanternAura),
            Card::Weapon(WeaponId::ChainSpark),
            Card::Weapon(WeaponId::StarSkipper),
        ] {
            run.game.apply_card(card);
        }
        run.game.stress(count);
    }
    let mut attract = attract_game(seed_now(2));
    let mut attract_acc = 0.;
    let mut screen = if demo.autoplay {
        Screen::Play
    } else {
        Screen::Title
    };
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    match capture.screen.as_deref() {
        Some("title") => screen = Screen::Title,
        Some("select") => screen = Screen::Select,
        Some("shrine") => screen = Screen::Shrine,
        _ => {}
    }
    let mut shrine_return = Screen::Title;
    let mut selected = usize::from(host.meta.character);
    let mut screen_age = 0_f32;
    let mut pending = Choice::None;
    let mut banish_mode = false;
    let mut offer_serial = u32::MAX;
    let mut offer_age = 0_f32;
    let mut chest_serial = u32::MAX;
    let mut chest_age = 0_f32;
    let mut results_age = 0_f32;
    let mut summary: Option<Summary> = None;
    let mut was_over = false;
    let mut pauses = 0_usize;
    let mut was_paused = false;
    let mut notice: Option<(&'static str, f32)> = None;
    let mut bought: Option<(usize, f32)> = None;
    let mut attempts = 0_u64;
    let mut visual_time = 0_f32;
    loop {
        let render_dt = demo.dt();
        let pdt = if render_dt.is_finite() {
            render_dt.clamp(0., 0.1)
        } else {
            0.
        };
        visual_time += pdt;
        screen_age += pdt;
        fx.sync();
        sound.age(pdt);
        let motion = Motion::from(fx.policy());
        if transition.advance(pdt, motion) == Some(TransitionEvent::Covered) {
            match swap.take() {
                Some(Swap::To(next)) => {
                    screen = next;
                    screen_age = 0.;
                    buttons.snap();
                }
                Some(Swap::ToTitle) => {
                    screen = Screen::Title;
                    screen_age = 0.;
                    run.reset();
                    buttons.snap();
                }
                Some(Swap::StartRun) => {
                    host.meta.character = selected as u8;
                    host.save();
                    config = RunConfig {
                        character: selected as u8,
                        quick: host.meta.quick,
                        seed: seed_now(attempts + 3),
                        bonuses: host.meta.bonuses(),
                    };
                    run.begin(config);
                    fx.reset();
                    sound.reset();
                    hud_state.reset();
                    summary = None;
                    was_over = false;
                    pending = Choice::None;
                    banish_mode = false;
                    screen = Screen::Play;
                    screen_age = 0.;
                    buttons.snap();
                    input.set_paused(true);
                }
                None => {}
            }
            transition.reveal();
        }
        let l = Layout::new([screen_width(), screen_height()]);
        let in_run = screen == Screen::Play;
        let playing = in_run && run.session.is_playing();
        let steering = playing && run.game.phase == Phase::Running;
        let controls = l.touch_controls();
        input
            .set_touch_layout(if steering {
                &controls[..]
            } else {
                &controls[..0]
            })
            .expect("authored touch layout");
        input.set_paused(!playing);
        input.sample(playing);
        let touch = input.current_device() == HintDevice::Touch;
        let mut changed = false;
        if !demo.autoplay && playing && !input.gameplay_active() {
            changed = run.pause();
        }
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        if capture.pause_now() && run.session.is_playing() {
            changed |= run.pause();
        }
        let menu_back = !in_run && input.pressed("pause") && !input.controls_open();
        if in_run && !run.game.is_over() && run.session.is_started() {
            if input.pressed("restart") {
                config.seed = seed_now(attempts);
                config.bonuses = host.meta.bonuses();
                run.begin(config);
                fx.reset();
                sound.reset();
                summary = None;
                was_over = false;
                changed = true;
            } else if input.pressed("pause") && !changed && !input.controls_open() {
                changed |= if run.session.is_paused() {
                    run.activate()
                } else {
                    run.pause()
                };
            }
        }
        if changed {
            input.set_paused(true);
        }
        if run.session.is_paused() && !was_paused {
            pauses += 1;
        }
        was_paused = run.session.is_paused();
        if run.session.is_paused() || !in_run {
            sound.pause();
        } else {
            sound.resume();
        }
        // Quick card keys (render-frame global actions), applied on the next tick.
        let modal = playing && matches!(run.game.phase, Phase::LevelUp | Phase::Chest);
        if modal && run.game.phase == Phase::LevelUp && pending == Choice::None {
            for (k, name) in ["pick_1", "pick_2", "pick_3", "pick_4"]
                .into_iter()
                .enumerate()
            {
                if input.pressed(name) && k < usize::from(run.game.offer.count) {
                    pending = if banish_mode {
                        Choice::Banish(k as u8)
                    } else {
                        Choice::Pick(k as u8)
                    };
                    banish_mode = false;
                }
            }
            if input.pressed("reroll") && run.game.rerolls > 0 {
                pending = Choice::Reroll;
            }
            if input.pressed("banish") && run.game.banishes > 0 {
                banish_mode = !banish_mode;
            }
        }
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        if in_run {
            capture.prepare(&mut run);
        }
        // --- Simulation: edges are consumed only inside executed ticks.
        let movement = input.movement();
        let movement = vec2(movement.x, -movement.y);
        let dt = if changed {
            0.
        } else if demo.autoplay {
            render_dt
        } else {
            input.simulation_dt(render_dt)
        };
        if in_run {
            let autoplay = demo.autoplay;
            #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
            let capture_ref = &capture;
            run.advance(f64::from(dt), |game| {
                if autoplay {
                    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
                    return capture_ref.input(game);
                    #[allow(unreachable_code)]
                    return sim::bot::autoplay(game);
                }
                let [dash] = input.take_tick(["boost"]);
                let choice = std::mem::take(&mut pending);
                Input {
                    movement: if game.phase == Phase::Running {
                        movement
                    } else {
                        Vec2::ZERO
                    },
                    dash: dash.pressed,
                    choice,
                }
            });
            run.drain(&mut events);
        } else {
            // Attract mode behind the title: a muted demo run at 60 Hz.
            attract_acc += pdt;
            let mut steps = 0;
            while attract_acc >= sim::DT && steps < 4 {
                attract_acc -= sim::DT;
                steps += 1;
                let input = sim::bot::autoplay(&attract);
                attract.update(input);
            }
            attract.drain_events(&mut events);
            if attract.is_over() || attract.time > 400. {
                attract = attract_game(seed_now(attempts + 9));
            }
        }
        let world_game = if in_run { &run.game } else { &attract };
        let alpha = if in_run {
            run.alpha
        } else {
            (attract_acc / sim::DT).clamp(0., 1.)
        };
        let running = world_game.phase == Phase::Running && (!in_run || run.session.is_playing());
        fx.age(pdt, running);
        fx.consume(&events, world_game, &mut sound, !in_run);
        events.clear();
        let player_pos = world_game.player.prev.lerp(world_game.player.pos, alpha);
        if running || !in_run {
            fx.follow(player_pos, world_game.player.vel, pdt);
        }
        sound.music(
            pdt,
            if in_run && playing {
                (world_game.foes.len() as f32 / 300.).min(1.) * 0.8
                    + if world_game.boss().is_some() { 0.2 } else { 0. }
            } else {
                0.15
            },
        );
        // --- Run end bookkeeping.
        let over = in_run && run.game.is_over();
        if over && !was_over {
            let won = run.game.phase == Phase::Won;
            ferravex_render::performance::event(
                if won {
                    ferravex_play::performance::DiagnosticKind::Completed
                } else {
                    ferravex_play::performance::DiagnosticKind::Death
                },
                run.game.time,
            );
            let glimmer = meta::run_glimmer(&run.game);
            host.meta.award(glimmer, won);
            host.save();
            let quick = run.game.config.quick;
            let survive = (run.game.time * 10.) as u64;
            let level = u64::from(run.game.level());
            let kills = u64::from(run.game.stats.kills);
            let mut best = [false; 3];
            if let Some(records) = &mut host.records {
                for (k, (metric, value)) in
                    [("survive", survive), ("kills", kills), ("level", level)]
                        .into_iter()
                        .enumerate()
                {
                    let id = meta::record_id(quick, metric);
                    let prev = records.get(id, meta::RECORD_RULES).map(|r| r.score);
                    best[k] = prev.is_none_or(|p| value > p);
                    let time = if won {
                        ferravex_render::storage::run_micros(run.game.time)
                    } else {
                        None
                    };
                    let _ = records.attempt(id, meta::RECORD_RULES, value, time);
                }
            }
            summary = Some(Summary {
                title: meta::title(&run.game),
                best_time: best[0],
                best_kills: best[1],
                best_level: best[2],
                rolls: [
                    run.game.time as i64,
                    i64::from(run.game.level()),
                    kills as i64,
                    glimmer as i64,
                ]
                .map(|v| {
                    let mut roll = RollCounter::new(0);
                    roll.set_target(v);
                    roll
                }),
            });
            attempts += 1;
            results_age = 0.;
            input.set_paused(true);
        }
        was_over = over;
        if over {
            results_age += pdt;
        }
        if in_run {
            hud_state.update(&run.game, pdt, motion);
        }
        if let Some(s) = &mut summary {
            s.advance(pdt, motion, results_age);
        }
        if run.session.is_paused() {
            pause_age += pdt;
        } else {
            pause_age = 0.;
        }
        let want = if !in_run {
            Grade::Menu
        } else if world_game
            .boss()
            .is_some_and(|b| b.kind == sim::content::FoeKind::Gulpjaw && b.hp / b.max_hp < 0.6)
        {
            Grade::LightsOut
        } else {
            Grade::Run
        };
        if graded != Some(want) {
            graded = Some(want);
            post::set_post_fx(grade(want));
        }
        // --- World pass.
        let (deep, _, _) = world::palette(&fx);
        clear_background(Color::new(deep[0], deep[1], deep[2], 1.));
        let shake = fx.shake_offset();
        let view = View::new(l.screen, fx.camera + shake, 1. + fx.zoom_punch);
        set_camera(&view.camera());
        ferravex_render::set_world_colors(true);
        painter.begin_frame();
        {
            let scene = Scene {
                game: world_game,
                fx: &fx,
                view,
                alpha,
                time: visual_time,
                flashes: fx.flashes(),
                motion: fx.motion(),
            };
            world::draw(&mut painter, &scene);
        }
        let _ = particles.draw_2d(
            &fx.pixels,
            ParticleStyle {
                shape: ParticleShape::Square,
                sort_back_to_front: false,
                ..ParticleStyle::default()
            },
        );
        let _ = particles.draw_2d(
            &fx.sparks,
            ParticleStyle {
                draw: EffectStyle {
                    blend: EffectBlend::Additive,
                    depth_test: Some(false),
                    depth_write: false,
                },
                shape: ParticleShape::Diamond,
                stretch: 0.012,
                max_stretch: 3.5,
                sort_back_to_front: false,
            },
        );
        // --- World/HUD boundary: everything below draws crisp, unbloomed.
        post::post_fx_hud_begin();
        ferravex_render::set_world_colors(false);
        set_default_camera();
        if in_run {
            hud::numbers(&mut tx, &fx, &view);
        }
        ferravex_render::performance::context(Some(run.session.tick()), None);
        let show_hud = in_run && run.session.is_started() && !over;
        if show_hud {
            hud::draw(
                &mut painter,
                &mut tx,
                &hud_state,
                &l,
                &run.game,
                &fx,
                &view,
                visual_time,
                touch,
            );
            if touch
                && steering
                && let Some((origin, position)) = input.touch_stick(0, 1)
            {
                hud::stick(&mut painter, origin, position, run.game.character().color);
            }
        }
        painter.flush();
        tx.flush();
        // --- Menus and modals.
        ui_menu.begin(l.viewport);
        let mut menu = Menu {
            p: &mut painter,
            tx: &mut tx,
            ui: &mut ui_menu,
            buttons: &mut buttons,
            t: visual_time,
            dt: pdt,
            motion,
            sway: fx.motion(),
        };
        if let Some((_, a)) = &mut notice {
            *a -= pdt;
        }
        if notice.is_some_and(|(_, a)| a <= 0.) {
            notice = None;
        }
        if let Some((_, a)) = &mut bought {
            *a = (*a - pdt * 1.5).max(0.);
        }
        match screen {
            Screen::Title => {
                let best = host.records.as_ref().and_then(|r| {
                    let t = r.get(
                        meta::record_id(host.meta.quick, "survive"),
                        meta::RECORD_RULES,
                    )?;
                    let k = r.get(
                        meta::record_id(host.meta.quick, "kills"),
                        meta::RECORD_RULES,
                    )?;
                    Some((t.score, k.score))
                });
                menus::title(&mut menu, &l, &host.meta, best, screen_age);
            }
            Screen::Select => {
                menus::select(&mut menu, &l, &host.meta, selected, screen_age, notice)
            }
            Screen::Shrine => menus::shrine(&mut menu, &l, &host.meta, screen_age, bought),
            Screen::Play => {
                let game = &run.game;
                if over {
                    if let Some(s) = &summary {
                        menus::results(&mut menu, &l, game, s, results_age);
                    }
                } else if run.session.is_paused() {
                    menus::paused(
                        &mut menu,
                        &l,
                        game,
                        menus::PAUSE_QUIPS[pauses % menus::PAUSE_QUIPS.len()],
                        pause_age,
                    );
                } else if game.phase == Phase::LevelUp {
                    if game.offer.serial != offer_serial {
                        offer_serial = game.offer.serial;
                        offer_age = 0.;
                    }
                    offer_age += pdt;
                    let hint = match input.current_device() {
                        HintDevice::Gamepad => "D-pad + A to pick · Y reroll · X banish",
                        HintDevice::Touch => "Tap a card to choose it",
                        _ => "1–4 or click to pick · E reroll · Q banish",
                    };
                    menus::level_up(
                        &mut menu,
                        &l,
                        game,
                        &CardView {
                            banish_mode,
                            age: offer_age,
                        },
                        hint,
                    );
                } else if game.phase == Phase::Chest {
                    if game.chest.serial != chest_serial {
                        chest_serial = game.chest.serial;
                        chest_age = 0.;
                    }
                    chest_age += pdt;
                    menus::chest(&mut menu, &l, game, chest_age);
                } else if touch && run.session.is_playing() {
                    menus::touch_pause(&mut menu, &l);
                }
            }
        }
        painter.flush();
        tx.flush();
        let idle = transition.phase() == ferravex_play::tween::TransitionPhase::Idle;
        let menu_live = idle
            && !changed
            && (!in_run || !run.session.is_playing() || run.game.phase != Phase::Running || touch);
        ui_menu.finish_player(&mut input, menu_live && !demo.autoplay);
        ui_menu.paint();
        // --- Menu actions.
        let hit = |id: u64| ui_menu.activated(id);
        let mut start_run = false;
        match screen {
            Screen::Title => {
                if hit(id::DIVE) {
                    swap = Some(Swap::To(Screen::Select));
                    sound.play(sfx::Sfx::Select, 0.6, 1.);
                } else if hit(id::SHRINE) {
                    shrine_return = Screen::Title;
                    swap = Some(Swap::To(Screen::Shrine));
                    sound.play(sfx::Sfx::Select, 0.6, 1.);
                }
            }
            Screen::Select => {
                for i in 0..CHARACTERS.len() {
                    if hit(id::CHARACTER + i as u64) {
                        if host.meta.is_unlocked(i) {
                            selected = i;
                            sound.play(sfx::Sfx::Click, 0.6, 1.2);
                        } else if host.meta.unlock(i) == meta::Purchase::Bought {
                            selected = i;
                            host.save();
                            sound.play(sfx::Sfx::Chest, 0.6, 1.);
                        } else {
                            notice = Some(("NOT ENOUGH GLIMMER. DIVE TO EARN MORE!", 2.5));
                            sound.play(sfx::Sfx::Hurt, 0.4, 1.4);
                        }
                    }
                }
                if hit(id::TIDE) {
                    host.meta.quick = !host.meta.quick;
                    host.save();
                    sound.play(sfx::Sfx::Click, 0.6, 1.);
                }
                if hit(id::GO) && host.meta.is_unlocked(selected) {
                    start_run = true;
                }
                if hit(id::BACK) || menu_back {
                    swap = Some(Swap::To(Screen::Title));
                }
            }
            Screen::Shrine => {
                for i in 0..sim::content::SHRINE_ITEMS {
                    if hit(id::BUY + i as u64) && host.meta.buy(i) == meta::Purchase::Bought {
                        host.save();
                        bought = Some((i, 1.));
                        sound.play(sfx::Sfx::Coin, 0.7, 1.);
                    }
                }
                if hit(id::SHRINE_BACK) || menu_back {
                    // Coming back from results shows the title, not a stale run.
                    swap = Some(if shrine_return == Screen::Play {
                        Swap::ToTitle
                    } else {
                        Swap::To(shrine_return)
                    });
                }
            }
            Screen::Play => {
                if over {
                    if hit(id::AGAIN) {
                        start_run = true;
                    } else if hit(id::RESULTS_SHRINE) {
                        shrine_return = Screen::Play;
                        swap = Some(Swap::To(Screen::Shrine));
                    } else if hit(id::RESULTS_TITLE) {
                        swap = Some(Swap::ToTitle);
                    }
                } else if run.session.is_paused() {
                    if hit(id::RESUME) {
                        run.activate();
                        input.set_paused(true);
                    } else if hit(id::RESTART) {
                        start_run = true;
                    } else if hit(id::GIVE_UP) {
                        run.game.surrender();
                        run.session.set_paused(false);
                        run.session.finish(false);
                    } else if hit(id::TITLE) {
                        let glimmer = meta::run_glimmer(&run.game);
                        host.meta.award(glimmer, false);
                        host.save();
                        swap = Some(Swap::ToTitle);
                    }
                } else if run.game.phase == Phase::LevelUp {
                    for i in 0..4u8 {
                        if hit(id::CARD + u64::from(i)) {
                            pending = if banish_mode {
                                Choice::Banish(i)
                            } else {
                                Choice::Pick(i)
                            };
                            banish_mode = false;
                        }
                    }
                    if hit(id::REROLL) {
                        pending = Choice::Reroll;
                    }
                    if hit(id::BANISH) {
                        banish_mode = !banish_mode;
                    }
                } else if run.game.phase == Phase::Chest {
                    if hit(id::CONTINUE) {
                        pending = Choice::Pick(0);
                    }
                } else if hit(id::TOUCH_PAUSE) {
                    run.pause();
                    input.set_paused(true);
                }
            }
        }
        if start_run {
            swap = Some(Swap::StartRun);
            sound.play(sfx::Sfx::Select, 0.7, 0.9);
        }
        if swap.is_some() && transition.phase() == ferravex_play::tween::TransitionPhase::Idle {
            transition.cover();
        }
        let focus = if in_run {
            view.to_screen(player_pos)
        } else {
            l.screen * 0.5
        };
        kit::draw_screen_transition(
            transition.kind(motion),
            transition.coverage(),
            Color::new(0.01, 0.02, 0.05, 1.),
            focus,
        );
        set_default_camera();
        input.draw_controls();
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        capture.end_frame();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}
