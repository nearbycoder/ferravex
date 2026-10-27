//! Picnic Panic: a small, very determined raccoon versus the village picnic.
//!
//! Work through a hand-written to-do list of pranks in a sunny low-poly park
//! while picnickers go about their routines, notice you, chase you and carry
//! you back to your burrow. Rules live in [`sim`] and run on fixed 60 Hz ticks
//! with a seeded RNG; this host samples input, renders, plays audio and saves.
//!
//! ```
//! use picnic_panic::sim::{Game, Input, RunConfig, bot::Bot, pranks::Prank};
//! let mut game = Game::new(RunConfig { seed: 7, completed: 0 });
//! let mut bot = Bot::new(&game);
//! // The demo raccoon heads straight for the village bell by the gate.
//! for _ in 0..60 * 12 {
//!     let input = bot.input(&game);
//!     game.update(input);
//! }
//! assert!(game.todo.done(Prank::Bell), "DONG. Everyone checks their watch.");
//! // A chitter is an input edge consumed by exactly one tick.
//! let mut game = Game::new(RunConfig::default());
//! game.update(Input { chitter: true, ..Input::default() });
//! assert_eq!(game.stats.chitters, 1);
//! ```
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
mod capture;
pub mod fx;
pub mod hud;
pub mod layout;
pub mod menus;
pub mod meta;
pub mod models;
pub mod rig;
pub mod run;
pub mod scene;
pub mod sfx;
pub mod sim;
pub mod text;

use ferravex_physics::d3::QueryOptions;
use ferravex_play::camera::{
    CameraError, Input3, Mode3, ObstructionSettings, Rig3, Settings3, Sweep, Target3,
};
use ferravex_play::replay::ghost::GhostTrack;
use ferravex_play::tween::{
    Motion, ScreenTransition, TransitionEvent, TransitionKind, TransitionPhase,
};
use ferravex_render::demo::Demo;
use ferravex_render::effects::{
    BillboardView, EffectBlend, EffectStyle, ParticleRenderer, ParticleShape, ParticleStyle,
    draw_ribbon,
};
use ferravex_render::input::{ActionKind, Binding, HintDevice, InputCode, PlayerInput, Preset};
use ferravex_render::post::{self, PostFx};
use ferravex_render::prelude::*;
use ferravex_render::storage::{Records, Slot};
use ferravex_render::ui::{Ui, kit};
use fx::{Fx, Toast};
use hud::HudState;
use layout::Layout;
use menus::{Buttons, Menu, Summary, id};
use meta::{DaySave, Ghost, Meta};
use run::Run;
use scene::{Actors, Frame, Scene};
use sfx::{Sfx, SoundKit};
use sim::bot::Bot;
use sim::{Event, Game, Input, MAX_EVENTS, RunConfig, flat, heading, yaw_of};
use text::Text;

fn conf() -> Conf {
    #[allow(unused_mut)]
    let mut conf = ferravex_render::window_conf("Picnic Panic · Ferravex");
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    capture::window(&mut conf);
    conf
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Title,
    Wardrobe,
    Play,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Swap {
    To(Screen),
    Start(u8),
    ToTitle,
}

/// Warm film: lifted golden shadows, a soft vignette and a high bloom
/// threshold so only lanterns, the sun, water glints and sparkles glow.
fn grade(menu: bool) -> PostFx {
    let fx = PostFx::warm_film()
        .bloom(0.85, 0.93, 1.2)
        .bloom_knee(0.12)
        .exposure(1.05)
        .saturation(1.04)
        .lift_gamma_gain([0.035, 0.018, 0.], [1., 1., 1.02], [1.09, 1.01, 0.86])
        .vignette(0.32, 0.7);
    if menu {
        fx.vignette(0.45, 0.6).exposure(0.98)
    } else {
        fx
    }
}

fn add_actions(input: &mut PlayerInput) {
    let key = |k: KeyCode| Binding::digital(InputCode::Key(k as u16));
    let b = Binding::digital;
    for (name, label, bindings) in [
        (
            "jump",
            "Jump",
            vec![
                key(KeyCode::Space),
                b(InputCode::PadButton(0)),
                b(InputCode::Touch(layout::TOUCH_JUMP)),
            ],
        ),
        (
            "throw",
            "Throw",
            vec![
                key(KeyCode::F),
                b(InputCode::Mouse(3)),
                b(InputCode::PadButton(1)),
                b(InputCode::Touch(layout::TOUCH_THROW)),
            ],
        ),
        (
            "chitter",
            "Chitter (distract)",
            vec![
                key(KeyCode::C),
                b(InputCode::PadButton(3)),
                b(InputCode::Touch(layout::TOUCH_CHITTER)),
            ],
        ),
        (
            "slide",
            "Belly-slide / flop",
            vec![
                key(KeyCode::LeftShift),
                key(KeyCode::RightShift),
                b(InputCode::PadButton(5)),
                b(InputCode::Touch(layout::TOUCH_SLIDE)),
            ],
        ),
    ] {
        input
            .action(name, label, ActionKind::Button, bindings)
            .expect("authored action");
    }
    input
        .global_action(
            "notes",
            "To-do list",
            ActionKind::Button,
            vec![key(KeyCode::N), b(InputCode::PadButton(4))],
        )
        .expect("authored action");
    input
        .action(
            "zoom",
            "Camera zoom",
            ActionKind::Delta,
            vec![b(InputCode::Wheel(1))],
        )
        .expect("authored action");
    for (code, scale) in [(KeyCode::Z, -0.9), (KeyCode::X, 0.9)] {
        let _ = input.add_default_binding(
            "look_rate_x",
            Binding {
                code: InputCode::Key(code as u16),
                scale,
                deadzone: 0.,
            },
        );
    }
    // The interact action (E / X / touch) is our grab, drop and use button.
    let _ = input.set_mouse_look_button(Some(1));
}

fn hint<'a>(input: &PlayerInput, action: &str, buf: &'a mut [u8], fallback: &'a str) -> &'a str {
    match input.format_hint(action, buf) {
        Ok(s) if !s.is_empty() => {
            // Show only the first binding; Controls lists the rest.
            let s: &str = s;
            s.split(['/', ',']).next().map_or(fallback, str::trim)
        }
        _ => fallback,
    }
}

fn seed_now(salt: u64) -> u64 {
    let t = (get_time() * 1_000_003.) as u64;
    t ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ 0x7069_636e
}

struct Saves {
    meta: Meta,
    slot: Option<Slot<Meta>>,
    day: Option<Slot<DaySave>>,
    ghost: Option<Slot<Ghost>>,
    records: Option<Records>,
}

impl Saves {
    fn commit(&mut self) {
        if let Some(slot) = &mut self.slot {
            let _ = slot.commit(self.meta.clone());
        }
    }
    fn saved_day(&self) -> Option<u8> {
        if self.meta.day_ms == 0 {
            return None;
        }
        self.day
            .as_ref()
            .and_then(|d| meta::restore_mask(d.value()))
    }
}

/// Camera presentation state (never touches the rules).
struct CameraState {
    rig: Rig3,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    idle: f32,
}

impl CameraState {
    fn new(yaw: f32) -> Self {
        Self {
            rig: Rig3::new(Settings3 {
                position_half_life: 0.1,
                target_half_life: 0.05,
                fov_half_life: 0.2,
                teleport_distance: Some(9.),
                obstruction: Some(ObstructionSettings {
                    radius: 0.22,
                    padding: 0.08,
                    release_half_life: 0.35,
                    sweep_motion: true,
                }),
                shake_strength: 1.,
            })
            .expect("authored camera"),
            yaw,
            pitch: 0.36,
            zoom: 6.2,
            idle: 10.,
        }
    }
}

ferravex_render::game!(conf, game);
async fn game() {
    let mut demo = Demo::new();
    ferravex_render::set_quality(ferravex_render::lighting::Quality::High);
    let _ = ferravex_render::set_visibility(ferravex_render::visibility::VisibilitySettings {
        frustum_culling: true,
        max_distance: Some(150.),
    });
    let mut sound = SoundKit::new();
    let mut fx = Fx::new();
    let slot = Slot::open(meta::OWNER, "meta", Meta::default()).ok();
    let mut saves = Saves {
        meta: slot.as_ref().map(|s| s.value().clone()).unwrap_or_default(),
        slot,
        day: Slot::open(meta::OWNER, "day", sim::pranks::Todo::new().snapshot()).ok(),
        ghost: Slot::open(meta::OWNER, "ghost", Ghost::default()).ok(),
        records: Records::new(meta::OWNER).ok(),
    };
    let mut ghost_track: Option<GhostTrack> = saves
        .ghost
        .as_ref()
        .filter(|g| !g.value().is_empty())
        .and_then(|g| g.value().track());
    let mut input = PlayerInput::preset("picnic-panic", Preset::Explore);
    add_actions(&mut input);
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    let mut capture = capture::Capture::from_env();
    let mut config = RunConfig {
        seed: if demo.autoplay {
            0x7069_636e
        } else {
            seed_now(1)
        },
        completed: 0,
    };
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    {
        if let Some(done) = capture.done {
            config.completed = done & 0x7f;
        }
        if let Some(h) = capture.hat {
            saves.meta.hat = h.min(models::Hat::ALL.len() as u8 - 1);
            saves.meta.hats = 0xff;
        }
    }
    let mut run = Run::new(config, demo.autoplay);
    let mut bot = Bot::new(&run.game);
    let mut attract = Game::new(RunConfig {
        seed: 3,
        completed: 0,
    });
    let mut attract_bot = Bot::new(&attract);
    let mut attract_acc = 0_f32;
    let mut scene = Scene::new(&run.game);
    let mut actors = Actors::new();
    let mut soft_renderer = ParticleRenderer::new(900).expect("bounded particle scratch");
    let mut glow_renderer = ParticleRenderer::new(400).expect("bounded particle scratch");
    let mut ui = Ui::new();
    let mut tx = Text::new();
    let mut buttons = Buttons::new();
    let mut hud = HudState::new();
    hud.reset(Layout::new([screen_width(), screen_height()]).compact);
    let mut transition = ScreenTransition::new(TransitionKind::Iris, 0.45);
    let mut swap: Option<Swap> = None;
    let mut graded: Option<bool> = None;
    let mut events: Vec<Event> = Vec::with_capacity(MAX_EVENTS * 4);
    let mut cam = CameraState::new(-run.game.coon.yaw);
    let mut screen = if demo.autoplay {
        Screen::Play
    } else {
        Screen::Title
    };
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    match capture.screen.as_deref() {
        Some("title") => screen = Screen::Title,
        Some("wardrobe") => screen = Screen::Wardrobe,
        _ => {}
    }
    if screen == Screen::Play {
        run.activate();
    }
    let mut screen_age = 0_f32;
    let mut pause_age = 0_f32;
    let mut pauses = 0_usize;
    let mut was_paused = false;
    let mut summary: Option<Summary> = None;
    let mut results_age = 0_f32;
    let mut was_over = false;
    let mut visual_time = 0_f32;
    let mut saved_pranks = run.game.completed();
    let mut hint_buf = [[0_u8; 32]; 3];
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
                    attract = Game::new(RunConfig {
                        seed: seed_now(5),
                        completed: 0,
                    });
                    attract_bot = Bot::new(&attract);
                }
                Some(Swap::Start(completed)) => {
                    config = RunConfig {
                        seed: seed_now(saves.meta.runs as u64 + 3),
                        completed,
                    };
                    run.begin(config);
                    bot = Bot::new(&run.game);
                    saved_pranks = run.game.completed();
                    saves.meta.runs = saves.meta.runs.saturating_add(1);
                    if completed == 0 {
                        saves.meta.day_ms = 0;
                    }
                    saves.commit();
                    fx.reset();
                    sound.reset();
                    actors.reset();
                    hud.reset(Layout::new([screen_width(), screen_height()]).compact);
                    cam = CameraState::new(-run.game.coon.yaw);
                    summary = None;
                    was_over = false;
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
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        let force_touch = capture.touch;
        #[cfg(not(all(debug_assertions, not(target_arch = "wasm32"))))]
        let force_touch = false;
        let controls = l.touch_controls();
        input
            .set_touch_layout(if playing {
                &controls[..]
            } else {
                &controls[..0]
            })
            .expect("authored touch layout");
        input.set_paused(!playing);
        input.sample(playing);
        let touch = force_touch || input.current_device() == HintDevice::Touch;
        let mut changed = false;
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        let keep_focus = capture.focus;
        #[cfg(not(all(debug_assertions, not(target_arch = "wasm32"))))]
        let keep_focus = false;
        if !demo.autoplay && !keep_focus && playing && !input.gameplay_active() {
            changed = run.pause();
        }
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        {
            if capture.pause_now() && run.session.is_playing() {
                changed |= run.pause();
            }
            if capture.end_now() && in_run && !run.game.is_over() {
                let at = run.game.coon.pos();
                for p in sim::pranks::Prank::ALL {
                    sim::pranks::complete(&mut run.game, p, at);
                }
            }
            if in_run {
                capture.prepare(&mut run, &mut bot);
            }
        }
        let menu_back = !in_run && input.pressed("pause") && !input.controls_open();
        if in_run
            && !run.game.is_over()
            && run.session.is_started()
            && input.pressed("pause")
            && !changed
            && !input.controls_open()
        {
            changed |= if run.session.is_paused() {
                run.activate()
            } else {
                run.pause()
            };
        }
        if in_run && input.pressed("notes") {
            hud.notes_open = !hud.notes_open;
            sound.play(Sfx::Page, 0.6, 1.);
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
        // --- Camera input (presentation): look orbits, wheel zooms.
        let look = input.look(pdt);
        if playing {
            cam.yaw -= look.x * 1.15;
            cam.pitch = (cam.pitch + look.y * 0.9).clamp(0.12, 1.05);
            let wheel = input.value("zoom");
            cam.zoom = (cam.zoom - wheel * 0.6).clamp(3.5, 11.);
            if look.length() > 1e-4 {
                cam.idle = 0.;
            }
        }
        cam.idle += pdt;
        // --- Simulation: edges are consumed only inside executed ticks.
        let movement = input.movement();
        let held_jump = input.held("jump");
        let held_grab = input.held("interact");
        let dt = if changed {
            0.
        } else if demo.autoplay {
            render_dt
        } else {
            input.simulation_dt(render_dt)
        };
        if in_run {
            let autoplay = demo.autoplay;
            let cam_yaw = -cam.yaw;
            run.advance(f64::from(dt), |game| {
                if autoplay {
                    return bot.input(game);
                }
                let [jump, grab, throw, chitter, slide] =
                    input.take_tick(["jump", "interact", "throw", "chitter", "slide"]);
                Input {
                    movement,
                    cam_yaw,
                    jump: jump.pressed,
                    jump_held: held_jump,
                    grab: grab.pressed,
                    grab_held: held_grab,
                    throw: throw.pressed,
                    chitter: chitter.pressed,
                    slide: slide.pressed,
                }
            });
            run.drain(&mut events);
        } else {
            // Attract mode behind the title: the demo raccoon plays on.
            attract_acc += pdt;
            let mut steps = 0;
            while attract_acc >= sim::DT && steps < 4 {
                attract_acc -= sim::DT;
                steps += 1;
                let input = attract_bot.input(&attract);
                attract.update(input);
            }
            attract.drain_events(&mut events);
            if attract.time > 240. {
                attract = Game::new(RunConfig {
                    seed: seed_now(9),
                    completed: 0,
                });
                attract_bot = Bot::new(&attract);
            }
        }
        let world: &Game = if in_run { &run.game } else { &attract };
        let alpha = if in_run {
            run.alpha
        } else {
            (attract_acc / sim::DT).clamp(0., 1.)
        };
        let running = !in_run || run.session.is_playing();
        let (coon_pos, coon_yaw) = world.coon_pose(alpha);
        fx.consume(&events, world, &mut sound, !in_run);
        // Unlocks and saves follow accepted progress.
        if in_run {
            for e in &events {
                match e {
                    Event::Prank { .. } | Event::Bonus { .. } => {
                        let all_caps = run.game.shinies_found() as usize == run.game.shinies.len();
                        if let Some(hat) = saves.meta.note_progress(
                            run.game.completed(),
                            run.game.todo.bonus,
                            all_caps,
                        ) && !demo.autoplay
                        {
                            fx.toasts
                                .insert(
                                    ferravex_play::ui::notices::Notice {
                                        key: Some(5),
                                        priority:
                                            ferravex_play::ui::notices::NoticePriority::Normal,
                                        message: Toast::Hat(hat),
                                        lifetime_seconds: 3.5,
                                    },
                                    ferravex_play::ui::notices::NoticeDuplicate::Replace,
                                )
                                .ok();
                        }
                    }
                    Event::Shiny { count, .. } if *count as usize == run.game.shinies.len() => {
                        if let Some(hat) = saves.meta.note_progress(0, 0, true) {
                            let _ = hat;
                        }
                    }
                    _ => {}
                }
            }
            if run.game.completed() != saved_pranks && !run.game.is_over() {
                saved_pranks = run.game.completed();
                saves.meta.day_ms = (run.game.time * 1000.) as u64 + 1;
                saves.meta.scruffs = saves.meta.scruffs.max(run.game.stats.scruffs);
                if let Some(day) = &mut saves.day {
                    let _ = day.commit(run.game.todo.snapshot());
                }
                saves.commit();
            }
        }
        events.clear();
        fx.advance(pdt, world, coon_pos, running);
        let chase = world.folk.iter().filter(|f| f.chasing()).count() as f32;
        let trees: [Vec3; 6] =
            std::array::from_fn(|i| world.park.trees.get(i * 3).map_or(Vec3::ZERO, |t| t.at));
        sound.ambience(
            pdt,
            if running { (chase / 2.).min(1.) } else { 0. },
            world.band_offkey,
            cam.rig.pose().map_or(coon_pos, |p| p.position),
            &trees,
        );
        // Footsteps: pitter-patter from accepted grounded travel.
        sound.steps.walk(
            coon_pos,
            world.coon.motor.grounded && world.coon.scruffed.is_none(),
            running && world.coon.speed > 0.4,
            sfx::walk_surface(),
            0.45,
        );
        // --- Run end bookkeeping.
        let over = in_run && run.game.is_over();
        if over && !was_over {
            ferravex_render::performance::event(
                ferravex_play::performance::DiagnosticKind::Completed,
                run.game.time,
            );
            let ms = (run.game.time * 1000.) as u64;
            let all_caps = run.game.shinies_found() as usize == run.game.shinies.len();
            let (best, hat) = saves.meta.record_clear(
                ms,
                run.game.stats.scruffs,
                run.game.todo.bonus.count_ones(),
                run.game.todo.bonus,
                all_caps,
            );
            if !demo.autoplay {
                saves.commit();
                if let Some(records) = &mut saves.records {
                    let _ = records.attempt(
                        meta::RECORD,
                        meta::RECORD_RULES,
                        u64::from(run.game.todo.bonus.count_ones()),
                        ferravex_render::storage::run_micros(run.game.time),
                    );
                }
                if best && let Some(slot) = &mut saves.ghost {
                    let _ = slot.commit(run.ghost.clone());
                    ghost_track = run.ghost.track();
                }
                if let Some(day) = &mut saves.day {
                    let _ = day.commit(sim::pranks::Todo::new().snapshot());
                }
            }
            summary = Some(Summary::new(
                run.game.time,
                run.game.stats.scruffs,
                run.game.todo.bonus,
                run.game.shinies_found(),
                run.game.shinies.len(),
                best,
                hat,
            ));
            results_age = 0.;
            input.set_paused(true);
        }
        was_over = over;
        if over {
            results_age += pdt;
        }
        if let Some(s) = &mut summary {
            s.advance(pdt, motion, results_age);
        }
        if run.session.is_paused() {
            pause_age += pdt;
        } else {
            pause_age = 0.;
        }
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        if let Some(open) = capture.notes {
            hud.notes_open = open;
        }
        if in_run {
            hud.update(&run.game, pdt, motion);
        }
        let want_menu = !in_run;
        if graded != Some(want_menu) {
            graded = Some(want_menu);
            post::set_post_fx(grade(want_menu));
        }
        // --- Camera: orbit rig with obstruction sweeps, set-piece framing.
        let fov = 52_f32.to_radians();
        let auto = in_run && cam.idle > 1.6 && world.coon.speed > 1.5 && fx.motion() > 0.;
        if auto {
            let behind = -coon_yaw;
            cam.yaw = sim::lerp_angle(cam.yaw, behind, (pdt * 1.4).min(1.));
        }
        let mut orbit_yaw = cam.yaw;
        let mut pitch = cam.pitch;
        let mut zoom = cam.zoom;
        if !in_run {
            orbit_yaw = visual_time * 0.05 * fx.motion() + 0.6;
            pitch = 0.42;
            zoom = 9.5;
        }
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        if let Some([y, p, d]) = capture.cam {
            orbit_yaw = y;
            pitch = p;
            zoom = d;
        }
        if world.coon.cape && in_run {
            zoom = zoom.max(7.5);
        }
        let occluders = |s: Sweep| -> Result<Option<f32>, CameraError> {
            let d = s.end - s.start;
            let len = d.length();
            if len < 1e-4 {
                return Ok(None);
            }
            Ok(world
                .world
                .spherecast_with_options(
                    s.start.to_array(),
                    (d / len).to_array(),
                    s.radius,
                    QueryOptions::new(len).layers(sim::L_OCCLUDE),
                )
                .map(|hit| (hit.distance / len).clamp(0., 1.)))
        };
        let focus = if in_run {
            coon_pos
        } else {
            coon_pos * 0.4 + vec3(0., 0., 4.) * 0.6
        };
        let mut pose = cam
            .rig
            .update(
                pdt,
                Input3 {
                    target: Target3 {
                        position: focus,
                        rotation: Quat::IDENTITY,
                        velocity: world.coon.motor.velocity,
                    },
                    mode: Mode3::Orbit {
                        yaw: orbit_yaw,
                        pitch,
                        distance: zoom,
                        pivot: vec3(0., 0.6, 0.),
                    },
                    fov_y: fov,
                },
                &occluders,
            )
            .map(|f| f.pose)
            .unwrap_or_default();
        let mut showcase_weight = 0.;
        if let Some(sc) = fx.showcase {
            let w = sc.weight();
            showcase_weight = w;
            let subject = sc
                .prop
                .map_or(sc.at, |i| world.prop_pose(i, alpha).0)
                .lerp(sc.at, 0.25);
            let side = heading(-orbit_yaw + 1.2);
            let eye = subject - side * 6.5 + Vec3::Y * 3.2;
            pose.position = pose.position.lerp(eye, w);
            pose.target = pose.target.lerp(subject + Vec3::Y * 0.6, w);
        }
        let shake = fx.camera_shake();
        let camera = Camera3D {
            position: pose.position + shake,
            target: pose.target + shake * 0.5,
            up: Vec3::Y,
            fovy: pose.fov_y.max(0.3),
            z_near: 0.08,
            z_far: 260.,
            aspect: None,
        };
        let right = (camera.target - camera.position)
            .cross(Vec3::Y)
            .normalize_or_zero();
        ferravex_render::audio::listener(camera.position, right);
        // --- Animation (presentation, from accepted state).
        let ghost_sample = if in_run && saves.meta.ghost {
            ghost_track
                .as_ref()
                .and_then(|g| g.sample(run.game.tick, alpha))
                .map(|s| {
                    let yaw = yaw_of(s.rotation * Vec3::NEG_Z);
                    (s.position, yaw)
                })
        } else {
            None
        };
        let ghost_speed = ghost_sample.map(|_| 3.);
        let (steps, _) = actors.update(world, if running { pdt } else { 0. }, ghost_speed);
        if steps > 0 && running && world.coon.motor.grounded {
            sound.at(Sfx::Pat, coon_pos, 0.18, 1.3);
        }
        // --- World pass.
        let sky = models::rgb(scene::HAZE);
        clear_background(sky);
        ferravex_render::set_world_colors(true);
        {
            let r = Rect::new(0., 0., l.screen.x, l.screen.y);
            ferravex_render::shapes::fill_rounded_rect(
                r,
                0.,
                ferravex_render::shapes::Fill::vertical(r, models::rgb(scene::SKY_TOP), sky),
            );
        }
        set_camera(&camera);
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        let hat = capture.hat.map_or(saves.meta.selected_hat(), |h| {
            models::Hat::ALL[usize::from(h).min(models::Hat::ALL.len() - 1)]
        });
        #[cfg(not(all(debug_assertions, not(target_arch = "wasm32"))))]
        let hat = saves.meta.selected_hat();
        scene.draw(
            &actors,
            &Frame {
                game: world,
                alpha,
                time: visual_time,
                camera,
                motion: fx.motion(),
                flash: fx.reduced_flash(),
                hat,
                ghost: ghost_sample,
                cones: saves.meta.cones && in_run,
                jelly: fx.jelly,
                showcase: showcase_weight > 0.,
            },
        );
        // Trails and particles after opaque geometry.
        let ground = EffectStyle {
            blend: EffectBlend::Alpha,
            depth_test: Some(true),
            depth_write: false,
        };
        let up = ferravex_play::particles::RibbonFacing::Plane {
            normal: [0., 1., 0.],
        };
        let _ = draw_ribbon(&fx.slide_trail, up, 2., ground);
        let _ = draw_ribbon(&fx.sled_trail, up, 2., ground);
        if let Ok(view) = BillboardView::from_camera(&camera) {
            let _ = soft_renderer.draw_3d(
                &fx.soft,
                view,
                ParticleStyle {
                    shape: ParticleShape::Square,
                    ..ParticleStyle::default()
                },
            );
            let _ = glow_renderer.draw_3d(
                &fx.glow,
                view,
                ParticleStyle {
                    draw: EffectStyle {
                        blend: EffectBlend::Additive,
                        depth_test: Some(true),
                        depth_write: false,
                    },
                    shape: ParticleShape::Diamond,
                    sort_back_to_front: false,
                    ..ParticleStyle::default()
                },
            );
        }
        // --- World/HUD boundary: everything below is crisp and un-bloomed.
        post::post_fx_hud_begin();
        ferravex_render::set_world_colors(false);
        set_default_camera();
        ferravex_render::performance::context(Some(run.session.tick()), None);
        tx.scale = 1.;
        let show_hud = in_run && run.session.is_started() && !over;
        if show_hud {
            hud::letterbox(&l, showcase_weight);
            hud::flash(&l, fx.flash);
            hud::bubbles(
                &mut tx,
                &l,
                &run.game,
                &fx,
                &camera,
                alpha,
                visual_time,
                fx.motion(),
            );
            if !touch && !l.compact && !run.session.is_paused() {
                hud::minimap(&mut hud, &l, &run.game, -orbit_yaw, alpha, visual_time);
            }
            let [grab_buf, jump_buf, notes_buf] = &mut hint_buf;
            let notes_key = if touch {
                "Tap"
            } else {
                hint(&input, "notes", notes_buf, "N")
            };
            hud::notepad(&mut tx, &hud, &l, &run.game, &fx, visual_time, notes_key);
            let grab = hint(&input, "interact", grab_buf, "E");
            let jump = hint(&input, "jump", jump_buf, "Space");
            hud::status(&mut tx, &l, &run.game, touch);
            hud::prompt(&mut tx, &hud, &l, &run.game, grab, jump, touch);
            if touch && run.session.is_playing() {
                hud::touch(&mut tx, &l, &input, &run.game);
                if let Some((origin, position)) = input.touch_stick(0, 1) {
                    hud::stick(origin, position);
                }
            }
            tx.flush();
            hud::toasts(&mut tx, &l, &fx, motion);
        }
        tx.flush();
        // --- Menus and modals.
        ui.begin(l.viewport);
        let mut menu = Menu {
            tx: &mut tx,
            ui: &mut ui,
            buttons: &mut buttons,
            t: visual_time,
            dt: pdt,
            motion,
            sway: fx.motion(),
        };
        let saved_day = saves.saved_day();
        match screen {
            Screen::Title => menus::title(&mut menu, &l, &saves.meta, saved_day, screen_age),
            Screen::Wardrobe => menus::wardrobe(&mut menu, &l, &saves.meta, screen_age),
            Screen::Play => {
                if over {
                    if let Some(s) = &summary {
                        menus::results(&mut menu, &l, s, results_age);
                    }
                } else if run.session.is_paused() {
                    menus::paused(
                        &mut menu,
                        &l,
                        &saves.meta,
                        menus::PAUSE_QUIPS[pauses % menus::PAUSE_QUIPS.len()],
                        pause_age,
                        run.game.todo.count(),
                    );
                } else if touch && run.session.is_playing() {
                    menus::touch_pause(&mut menu, &l);
                    menus::notes_toggle(&mut menu, &l);
                } else if run.session.is_playing() {
                    menus::notes_toggle(&mut menu, &l);
                }
            }
        }
        tx.flush();
        let idle = transition.phase() == TransitionPhase::Idle;
        let menu_live = idle && !changed && (!in_run || !run.session.is_playing() || touch);
        ui.finish_player(&mut input, menu_live && !demo.autoplay);
        ui.paint();
        // --- Menu actions.
        let hit = |i: u64| ui.activated(i);
        match screen {
            Screen::Title => {
                if hit(id::NEW_DAY) {
                    swap = Some(Swap::Start(0));
                    sound.play(Sfx::Select, 0.7, 1.);
                } else if hit(id::CONTINUE) {
                    swap = Some(Swap::Start(saved_day.unwrap_or(0)));
                    sound.play(Sfx::Select, 0.7, 1.);
                } else if hit(id::WARDROBE) {
                    swap = Some(Swap::To(Screen::Wardrobe));
                    sound.play(Sfx::Page, 0.7, 1.);
                }
            }
            Screen::Wardrobe => {
                for (k, h) in models::Hat::ALL.iter().enumerate() {
                    if hit(id::HAT + k as u64) && saves.meta.has_hat(*h) {
                        saves.meta.hat = k as u8;
                        saves.commit();
                        sound.play(Sfx::Ding, 0.6, 1. + k as f32 * 0.05);
                    }
                }
                if hit(id::BACK) || menu_back {
                    swap = Some(Swap::To(Screen::Title));
                }
            }
            Screen::Play => {
                if over {
                    if hit(id::AGAIN) {
                        swap = Some(Swap::Start(0));
                    } else if hit(id::RESULTS_TITLE) {
                        swap = Some(Swap::ToTitle);
                    }
                } else if run.session.is_paused() {
                    if hit(id::RESUME) {
                        run.activate();
                        input.set_paused(true);
                    } else if hit(id::RESTART) || input.pressed("restart") {
                        swap = Some(Swap::Start(0));
                    } else if hit(id::GHOST) {
                        saves.meta.ghost = !saves.meta.ghost;
                        saves.commit();
                        sound.play(Sfx::Click, 0.6, 1.);
                    } else if hit(id::CONES) {
                        saves.meta.cones = !saves.meta.cones;
                        saves.commit();
                        sound.play(Sfx::Click, 0.6, 1.);
                    } else if hit(id::TITLE) {
                        saves.meta.day_ms = if run.game.completed() != 0 {
                            (run.game.time * 1000.) as u64 + 1
                        } else {
                            0
                        };
                        saves.commit();
                        swap = Some(Swap::ToTitle);
                    }
                } else if hit(id::TOUCH_PAUSE) {
                    run.pause();
                    input.set_paused(true);
                } else if hit(id::NOTES) {
                    hud.notes_open = !hud.notes_open;
                    sound.play(Sfx::Page, 0.6, 1.);
                }
            }
        }
        if swap.is_some() && transition.phase() == TransitionPhase::Idle {
            transition.cover();
        }
        let focus =
            hud::project(&camera, l.screen, coon_pos + Vec3::Y * 0.4).unwrap_or(l.screen * 0.5);
        kit::draw_screen_transition(
            transition.kind(motion),
            transition.coverage(),
            Color::new(0.16, 0.09, 0.04, 1.),
            focus,
        );
        set_default_camera();
        input.draw_controls();
        let _ = flat;
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        capture.end_frame();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}
