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
const LINE: Color = Color::from_rgba(40, 62, 68, 255);
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
        clear_background(INK);
        set_camera(&view.camera);
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
        let playing_hud = started && !paused && game.phase == Phase::Playing;
        if playing_hud {
            if screen.compact {
                screen.hud(&mut hud_ui, game, &presentation.juice);
            } else {
                hud::play(&mut hud_ui, game, &player_input, learned, onboarding);
                hud::announcements(&mut hud_ui, game, &presentation.juice);
            }
        }
        if touch_mode
            && !player_input.touch_customized()
            && run.session.is_playing()
            && !screen.draw_touch(movement)
        {
            draw_circle_lines(202.0, 600.0, 66.0, 1.4, Color::new(0.4, 0.8, 0.75, 0.22));
            draw_circle(
                202.0 + movement.x * 34.0,
                600.0 + movement.y * 34.0,
                18.0,
                Color::new(0.4, 0.8, 0.75, 0.25),
            );
            draw_circle_lines(1100.0, 610.0, 40.0, 1.4, Color::new(0.4, 0.8, 0.75, 0.4));
            label("DASH", 1069.0, 616.0, 14, CYAN);
        }
        let best = records
            .get("sector-07", RECORD_RULES)
            .map(|r| (r.score, r.runs));
        if !started {
            if screen.compact {
                let detail = match best {
                    Some((score, _)) => format!(
                        "Clear three waves. Dash through amber rings. Best {}.",
                        hud::thousands(score)
                    ),
                    None => "Clear three waves. Amber rings warn of attacks; dash through them."
                        .to_owned(),
                };
                overlay(
                    &mut hud_ui,
                    &screen,
                    &player_input,
                    "NEON SWARM",
                    &detail,
                    "START",
                );
            } else {
                hud::ready(&mut hud_ui, &player_input, best);
            }
        } else if paused {
            if screen.compact {
                overlay(
                    &mut hud_ui,
                    &screen,
                    &player_input,
                    "PAUSED",
                    quips::pause(pauses),
                    "RESUME",
                );
            } else {
                hud::paused(&mut hud_ui, game, &player_input, pauses);
            }
        } else if game.phase != Phase::Playing {
            let fallback;
            let results = match &results {
                Some(results) => results,
                None => {
                    fallback = hud::Results::new(game, None, attempts);
                    &fallback
                }
            };
            if screen.compact {
                let (record, _) = results.record_line();
                overlay(
                    &mut hud_ui,
                    &screen,
                    &player_input,
                    if game.phase == Phase::Won {
                        "SECTOR SECURED"
                    } else {
                        "SIGNAL LOST"
                    },
                    &format!(
                        "{}! {} pts. {record}",
                        results.rank.title,
                        hud::thousands(results.summary.score)
                    ),
                    "PLAY AGAIN",
                );
            } else {
                hud::results(&mut hud_ui, results, &player_input);
            }
        }
        hud_ui.finish_player(
            &mut player_input,
            !run.session.is_playing() && !finished && !lifecycle_changed,
        );
        hud_ui.paint();
        if hud_ui.activated(1) {
            let resuming = run.session.is_paused();
            if run.activate() {
                if !resuming {
                    presentation.reset();
                    sound.reset();
                    results = None;
                }
                player_input.set_paused(true);
            }
        }
        set_default_camera();
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

fn text_width(text: &str, size: u16) -> f32 {
    get_default_font()
        .try_metrics(size, 1.)
        .expect("authored font size")
        .measure_line(text)
        .expect("bounded game label")
        .width
}

fn hint<'a>(input: &PlayerInput, action: &str, buffer: &'a mut [u8]) -> &'a str {
    // A long valid mapping stays available in Controls rather than overflowing
    // the HUD or silently reverting to a default key. Unbound is kept verbatim.
    input.format_hint(action, buffer).unwrap_or("See Controls")
}

fn overlay(
    ui: &mut Ui,
    screen: &ScreenLayout,
    input: &PlayerInput,
    title: &str,
    detail: &str,
    action: &str,
) {
    if screen.compact {
        screen.overlay(ui, title, detail, action, input);
    } else {
        hud::simple(ui, title, detail, action);
    }
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
