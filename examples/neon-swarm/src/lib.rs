//! A three-wave arena shooter with fixed simulation and explicit local lifecycle.
//!
//! The host uses the shared session API; combat remains deterministic game data:
//!
//! ```
//! use ferravex_core::session::{GameSession, SessionState};
//! let mut session = GameSession::new(120., 12, false);
//! assert!(session.start());
//! assert_eq!(session.advance(1. / 60.).steps, 2);
//! session.set_paused(true);
//! assert_eq!(session.advance(30.).steps, 0);
//! session.set_paused(false);
//! session.finish(true);
//! assert_eq!(session.state(), SessionState::Won);
//! session.restart();
//! assert_eq!(session.tick(), 0);
//! ```
mod ai;
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
mod capture;
mod draw;
mod game;
mod hud;
mod juice;
mod presentation;
mod quips;
mod run;
mod screen;

#[cfg(test)]
#[path = "journey200_tests.rs"]
mod journey200_162;

use ferravex_play::feedback::FeedbackKind;
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{PlayerInput, Preset};
use ferravex_render::ui::Ui;
use ferravex_render::{demo::Demo, prelude::*};
use game::{ARENA, Input, Phase};
use presentation::{Presentation, sync_audio};
use run::{Run, combat_input};
use screen::ScreenLayout;

const INK: Color = Color::from_rgba(9, 16, 22, 255);
const FLOOR: Color = Color::from_rgba(17, 29, 35, 255);
const MUTED: Color = Color::from_rgba(125, 150, 154, 255);
const PAPER: Color = Color::from_rgba(221, 237, 227, 255);
const CYAN: Color = Color::from_rgba(98, 236, 207, 255);
const CORAL: Color = Color::from_rgba(255, 132, 93, 255);
const AMBER: Color = Color::from_rgba(255, 196, 92, 255);
// Version 14 honors consumed dash edges and prevents lethal-hit pickup revival.
const RECORD_RULES: u64 = 14;

fn conf() -> Conf {
    #[allow(unused_mut)]
    let mut conf = ferravex_render::window_conf("Neon Swarm · Ferravex");
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    capture::window(&mut conf);
    conf
}

ferravex_render::game!(conf, game);
async fn game() {
    let mut demo = Demo::new();
    let mut sound = GameAudio::new();
    let mut presentation = Presentation::new();
    let mut particles = ferravex_render::effects::ParticleRenderer::new(512).unwrap();
    let mut run = Run::new(demo.autoplay);
    let mut records = ferravex_render::storage::Records::new("neon-swarm").unwrap();
    let mut player_input = PlayerInput::preset("neon-swarm", Preset::TopDown);
    let mut hud_ui = Ui::new();
    let mut hud_motion = hud::Hud::new();
    let mut post_state = None;
    let mut notes = Vec::with_capacity(64);
    let mut results: Option<hud::Results> = None;
    let mut learned = hud::Learned::default();
    // Seconds the "nice, now chain" line lingers after onboarding completes.
    let mut onboarding = 2.5_f32;
    let mut attempts = 0_u32;
    let mut pauses = 0_u32;
    let mut was_paused = false;
    let mut was_reloading = false;
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    let mut capture = capture::Capture::from_env();
    loop {
        // Host preferences arrive before this frame, including menus/zero ticks.
        presentation.sync(&mut run.game);
        let render_dt = demo.dt();
        let presentation_dt = if render_dt.is_finite() {
            render_dt.clamp(0., 0.25)
        } else {
            0.
        };
        presentation.age(presentation_dt);
        let screen = ScreenLayout::new([screen_width(), screen_height()]);
        let canvas = screen.canvas;
        let view = draw::WorldView::new(canvas, &presentation.juice, &run.game);
        let mouse = view.unproject(Vec2::from(mouse_position()));
        let scene_active = run.session.is_playing();
        let layout = screen.touch_controls();
        player_input
            .set_touch_layout(&layout[..if scene_active { 2 } else { 0 }])
            .unwrap();
        player_input.set_paused(!scene_active);
        player_input.sample(scene_active);
        let touch_mode = player_input.current_device() == ferravex_render::input::HintDevice::Touch;
        let mut lifecycle_changed = false;
        if !demo.autoplay && scene_active && !player_input.gameplay_active() {
            lifecycle_changed = run.pause();
        }
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        if capture.pause_now() && run.session.is_playing() {
            lifecycle_changed |= run.pause();
        }
        if player_input.pressed("restart") {
            run.restart();
            presentation.reset();
            sound.reset();
            results = None;
            lifecycle_changed = true;
        } else if player_input.pressed("pause")
            && !lifecycle_changed
            && !player_input.controls_open()
        {
            lifecycle_changed |= if run.session.is_paused() {
                run.activate()
            } else {
                run.pause()
            };
        }
        if lifecycle_changed {
            // Suppress held gameplay controls now and on next-frame reactivation.
            // Re-sampling here would mix menu input with the old gameplay frame.
            player_input.set_paused(true);
        }
        if run.session.is_paused() && !was_paused {
            pauses += 1;
        }
        was_paused = run.session.is_paused();
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        if run.session.is_playing() {
            capture.prepare(&mut run.game);
        }
        sync_audio(&mut sound, run.session.state());
        let movement = player_input.movement();
        let movement = vec2(movement.x, -movement.y);
        let aim = vec2(player_input.value("aim_x"), player_input.value("aim_y"));
        let touch_aim = player_input.touch_active();
        let previous_phase = run.game.phase;
        let was_cleared = run.game.wave_clear_remaining().is_some();
        let dt = if lifecycle_changed {
            0.
        } else if demo.autoplay {
            render_dt
        } else {
            player_input.simulation_dt(render_dt)
        };
        run.advance(
            f64::from(dt),
            |game| {
                if demo.autoplay {
                    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
                    return capture.input(game);
                    #[allow(unreachable_code)]
                    return game.autoplay();
                }
                let target = if touch_aim {
                    game.nearest_enemy()
                } else {
                    None
                };
                let aim = if touch_aim {
                    target.unwrap_or(game.player + game.facing)
                } else if aim.length_squared() > 0.01 {
                    game.player + aim * 200.
                } else {
                    mouse
                };
                combat_input(
                    Input {
                        movement,
                        aim,
                        ..Default::default()
                    },
                    player_input.take("fire"),
                    player_input.take("boost"),
                    touch_aim && target.is_some(),
                )
            },
            |event| {
                match event.kind {
                    FeedbackKind::Fire => learned.fired = true,
                    FeedbackKind::Dash => learned.dashed = true,
                    _ => {}
                }
                if let Ok(Some(response)) = presentation.accept(event) {
                    sound.cue(response.cue, response.gain);
                }
            },
        );
        // Accepted notes from the ticks just run: callouts, pops and cues.
        run.game.take_notes(&mut notes);
        let style = run.game.style;
        for note in notes.drain(..) {
            if let Some((cue, gain)) = presentation.note(note, &style) {
                sound.cue(cue, gain);
            }
        }
        presentation.sync(&mut run.game);
        if learned.complete() {
            onboarding = (onboarding - presentation_dt).max(0.);
        }
        let game = &run.game;
        let started = run.session.is_started();
        let paused = run.session.is_paused();
        let finished = previous_phase == Phase::Playing && game.phase != Phase::Playing;
        if finished {
            player_input.set_paused(true);
            sound.cue(
                if game.phase == Phase::Won {
                    Cue::Success
                } else {
                    Cue::Failure
                },
                0.65,
            );
            if game.phase == Phase::Won {
                presentation.celebrate();
            }
        } else if !was_cleared && game.wave_clear_remaining().is_some() {
            sound.cue(Cue::Success, 0.35);
        }
        let reloading = game.magazine.reloading();
        if reloading && !was_reloading && run.session.is_playing() {
            sound.cue(Cue::Reload, 0.3);
        }
        was_reloading = reloading;
        let impact = presentation.sample(presentation_dt);
        sound.walk(
            vec3(game.player.x, 0., game.player.y),
            true,
            run.session.is_playing() && (demo.autoplay || player_input.gameplay_active()),
            Surface::Metal,
            80.,
        );
        let post_screen = (!started, paused, game.phase != Phase::Playing);
        if post_state != Some(post_screen) {
            set_post_fx(
                ferravex_render::post::PostFx::neon()
                    .bloom(1.2, 0.72, 1.3)
                    .vignette(if paused { 0.42 } else { 0.28 }, 0.65)
                    .grain(0.)
                    .aberration(0.),
            );
            post_state = Some(post_screen);
        }
        clear_background(INK);
        set_camera(&view.camera);
        draw::backdrop(&presentation);
        draw::arena(game, &presentation);
        draw::spawn_preview(game);
        draw::entities(game, &presentation, impact.emphasis);
        if impact.flash > 0.001 {
            draw_rectangle_lines(
                ARENA.x,
                ARENA.y,
                ARENA.w,
                ARENA.h,
                6.,
                Color::new(1., 0.28, 0.16, impact.flash),
            );
        }
        particles
            .draw_2d(
                &presentation.particles,
                ferravex_render::effects::ParticleStyle {
                    draw: ferravex_render::effects::EffectStyle {
                        blend: ferravex_render::effects::EffectBlend::Additive,
                        ..Default::default()
                    },
                    stretch: 0.006,
                    max_stretch: 3.,
                    ..Default::default()
                },
            )
            .unwrap();
        draw::overlays(game, &presentation, !screen.compact);
        if started && !paused && game.phase == Phase::Playing && !touch_mode && !demo.autoplay {
            draw::reticle(mouse, game);
        }
        ferravex_render::set_world_colors(false);
        hud_ui.begin(screen.viewport);
        ferravex_render::performance::context(Some(run.session.tick()), None);
        if finished {
            ferravex_render::performance::event(
                if game.phase == Phase::Won {
                    ferravex_play::performance::DiagnosticKind::Completed
                } else {
                    ferravex_play::performance::DiagnosticKind::Death
                },
                game.time,
            );
            let time = if game.phase == Phase::Won {
                ferravex_render::storage::run_micros(game.time)
            } else {
                None
            };
            let previous = records.get("sector-07", RECORD_RULES).map(|r| r.score);
            #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
            let previous = capture.fake_best().or(previous);
            let _ = records.attempt("sector-07", RECORD_RULES, game.score.total(), time);
            results = Some(hud::Results::new(game, previous, attempts));
            attempts += 1;
        }
        let motion =
            ferravex_play::tween::Motion::from(ferravex_render::preferences::feedback_policy());
        let hud_screen = if !started {
            hud::Screen::Ready
        } else if paused {
            hud::Screen::Paused
        } else if game.phase == Phase::Playing {
            hud::Screen::Playing
        } else {
            hud::Screen::Results
        };
        hud_motion.update(hud_screen, game, results.as_ref(), presentation_dt, motion);
        if !screen.compact && hud_screen != hud::Screen::Playing {
            let card = match hud_screen {
                hud::Screen::Ready => hud::READY_CARD,
                hud::Screen::Paused => hud::PAUSE_CARD,
                _ => hud::RESULTS_CARD,
            };
            hud::menu_input(&mut hud_ui, card, hud::ACTION);
        }
        if screen.compact {
            if hud_screen == hud::Screen::Playing {
                screen.hud(&mut hud_ui, game, &presentation.juice);
            } else {
                let title = match hud_screen {
                    hud::Screen::Ready => "NEON SWARM",
                    hud::Screen::Paused => "PAUSED",
                    _ if game.phase == Phase::Won => "SECTOR SECURED",
                    _ => "SIGNAL LOST",
                };
                screen.overlay(
                    &mut hud_ui,
                    title,
                    if paused {
                        quips::pause(pauses)
                    } else {
                        "Clear three waves. Dash through amber rings."
                    },
                    if paused { "RESUME" } else { "START" },
                    &player_input,
                );
            }
        }
        hud_ui.finish_player(
            &mut player_input,
            !run.session.is_playing() && !finished && !lifecycle_changed,
        );
        hud_ui.paint();
        if !screen.compact {
            set_default_camera();
            let painter = hud::Painter::new(screen.viewport);
            hud_motion.button.update(
                ferravex_render::ui::kit::ui_button_state(&hud_ui, 1, true),
                presentation_dt,
                motion,
            );
            match hud_screen {
                hud::Screen::Playing => {
                    hud::bands(&painter, [screen_width(), screen_height()]);
                    hud::play(
                        &painter,
                        game,
                        &player_input,
                        learned,
                        onboarding,
                        &hud_motion,
                        &presentation.juice,
                    );
                    hud::announcements(&painter, game, &presentation.juice, &hud_motion, motion);
                }
                hud::Screen::Ready => {
                    let best = records
                        .get("sector-07", RECORD_RULES)
                        .map(|r| (r.score, r.runs));
                    hud::ready(&painter, &player_input, best, &hud_motion, motion);
                }
                hud::Screen::Paused => {
                    hud::paused(&painter, game, &player_input, pauses, &hud_motion, motion)
                }
                hud::Screen::Results => {
                    let fallback;
                    let result = match results.as_ref() {
                        Some(r) => r,
                        None => {
                            fallback = hud::Results::new(game, None, attempts);
                            &fallback
                        }
                    };
                    hud::results(&painter, result, &player_input, &hud_motion, motion);
                }
            }
        }
        if touch_mode && !player_input.touch_customized() && run.session.is_playing() {
            screen.draw_touch(movement);
        }
        if hud_ui.activated(1) {
            let resuming = run.session.is_paused();
            if run.activate() {
                if !resuming {
                    hud_motion.restart();
                    presentation.reset();
                    sound.reset();
                    results = None;
                }
                player_input.set_paused(true);
            }
        }
        set_default_camera();
        hud_motion.draw_transition(motion, view.project(run.game.player));
        player_input.draw_controls();
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        capture.end_frame();
        if demo.finish_frame() {
            break;
        }
        next_frame().await;
    }
}

fn label(text: &str, x: f32, y: f32, size: u16, color: Color) {
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font_size: size,
            color,
            ..Default::default()
        },
    );
}

fn hint<'a>(input: &PlayerInput, action: &str, buffer: &'a mut [u8]) -> &'a str {
    // A long valid mapping stays available in Controls rather than overflowing
    // the HUD or silently reverting to a default key. Unbound is kept verbatim.
    input.format_hint(action, buffer).unwrap_or("See Controls")
}

#[cfg(test)]
mod journey200_tests {
    use super::*;
    use ferravex_render::input::{ActionKind, Binding, InputCode};

    #[test]
    fn hints_follow_rebinding_unbinding_and_bounded_fallback() {
        let mut input = PlayerInput::transient("neon-swarm-hints");
        input
            .action(
                "boost",
                "Dash",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .unwrap();
        let mut buffer = [0; 128];
        assert_eq!(hint(&input, "boost", &mut buffer), "Space");
        input
            .rebind("boost", vec![Binding::digital(InputCode::Key(106))])
            .unwrap();
        assert_eq!(hint(&input, "boost", &mut buffer), "J");
        assert_eq!(hint(&input, "boost", &mut []), "See Controls");
        input.rebind("boost", vec![]).unwrap();
        assert_eq!(hint(&input, "boost", &mut buffer), "Unbound");
        assert_eq!(hint(&input, "missing", &mut buffer), "See Controls");
    }
}
