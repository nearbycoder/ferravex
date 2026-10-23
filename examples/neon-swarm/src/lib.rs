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
mod game;
mod presentation;
mod run;
mod screen;

#[cfg(test)]
#[path = "journey200_tests.rs"]
mod journey200_162;

use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{PlayerInput, Preset};
use ferravex_render::ui::{Align, ButtonStyle, TextStyle, Ui, UiRect};
use ferravex_render::{demo::Demo, prelude::*};
use game::{ARENA, Bullet, COVER, Enemy, Game, Input, Phase, Pickup, Position};
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
    ferravex_render::window_conf("Neon Swarm · Ferravex")
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
        ferravex_render::ui::warm_up_font_sizes(&[14, 15, 18, 22, 30, 32, 56]);
        let screen = ScreenLayout::new([screen_width(), screen_height()]);
        let canvas = screen.canvas;
        let mouse = canvas.unproject(Vec2::from(mouse_position()));
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
        if player_input.pressed("restart") {
            run.restart();
            presentation.reset();
            sound.reset();
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
                if let Ok(Some(response)) = presentation.accept(event) {
                    sound.cue(response.cue, response.gain);
                }
            },
        );
        presentation.sync(&mut run.game);
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
        } else if !was_cleared && game.wave_clear_remaining().is_some() {
            sound.cue(Cue::Success, 0.35);
        }
        let impact = presentation.sample(presentation_dt);
        sound.walk(
            vec3(game.player.x, 0., game.player.y),
            true,
            run.session.is_playing() && (demo.autoplay || player_input.gameplay_active()),
            Surface::Metal,
            80.,
        );
        clear_background(INK);
        set_camera(&canvas.camera());
        draw_arena();
        draw_spawn_preview(game);
        draw_entities(game, &presentation, impact.emphasis, !screen.compact);
        if impact.flash > 0.001 {
            draw_rectangle_lines(
                ARENA.x,
                ARENA.y,
                ARENA.w,
                ARENA.h,
                5.,
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
            let _ = records.attempt("sector-07", RECORD_RULES, game.score.total(), time);
        }
        if screen.compact {
            screen.hud(&mut hud_ui, game, &player_input);
        } else {
            draw_hud(&mut hud_ui, game, started, &player_input);
        }
        if started && !paused && game.phase == Phase::Playing && !touch_mode && !demo.autoplay {
            draw_circle_lines(mouse.x, mouse.y, 7.0, 1.2, CYAN);
            draw_line(mouse.x - 11.0, mouse.y, mouse.x - 5.0, mouse.y, 1.0, CYAN);
            draw_line(mouse.x + 5.0, mouse.y, mouse.x + 11.0, mouse.y, 1.0, CYAN);
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
            label("TOUCH 7", 1069.0, 616.0, 14, CYAN);
        }
        if !started {
            overlay(
                &mut hud_ui,
                &screen,
                &player_input,
                "NEON SWARM",
                "Clear three waves. Amber rings warn of attacks; dash to evade.",
                "START",
            );
        } else if paused {
            let mut binding = [0; 128];
            let restart = hint(&player_input, "restart", &mut binding);
            overlay(
                &mut hud_ui,
                &screen,
                &player_input,
                "PAUSED",
                &if screen.compact {
                    "Run frozen. Resume when ready.".to_owned()
                } else {
                    format!("Run frozen. Resume when ready. Restart: {restart}")
                },
                "RESUME",
            );
        } else if game.phase != Phase::Playing {
            let won = game.phase == Phase::Won;
            overlay(
                &mut hud_ui,
                &screen,
                &player_input,
                if won { "SECTOR SECURED" } else { "SIGNAL LOST" },
                &format!(
                    "{} points / {:.0}s / best {}",
                    game.score.total(),
                    game.time,
                    records
                        .get("sector-07", RECORD_RULES)
                        .map_or(0, |r| r.score)
                ),
                "PLAY AGAIN",
            );
        } else if let Some(remaining) = game.wave_clear_remaining().filter(|_| !screen.compact) {
            let mut banner = |args| {
                hud_label(
                    &mut hud_ui,
                    [250., 154.],
                    780.,
                    22.,
                    CYAN,
                    Align::Center,
                    args,
                );
            };
            if game.wave == 3 {
                banner(format_args!("ALL WAVES CLEAR / SECURING {:.1}s", remaining));
            } else {
                banner(format_args!(
                    "WAVE 0{} CLEAR / NEXT IN {:.1}s",
                    game.wave, remaining
                ));
            }
        } else if !screen.compact && game.wave_banner > 0.0 {
            let alpha = game.wave_banner.min(1.0);
            let text = format!(
                "WAVE 0{} / 03 - {} DRONES",
                game.wave,
                game.remaining_enemies()
            );
            let width = text_width(&text, 22);
            label(
                &text,
                640.0 - width * 0.5,
                154.0,
                22,
                Color::new(CYAN.r, CYAN.g, CYAN.b, alpha),
            );
        }
        if !screen.compact && !run.session.is_playing() {
            let mut binding = [0; 128];
            let confirm = hint(&player_input, "confirm", &mut binding);
            hud_label(
                &mut hud_ui,
                [280., 551.],
                720.,
                15.,
                MUTED,
                Align::Center,
                format_args!("CONFIRM: {confirm} / CONTROLS: F2"),
            );
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
                }
                player_input.set_paused(true);
            }
        }
        set_default_camera();
        player_input.draw_controls();
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

fn draw_spawn_preview(game: &Game) {
    if game.phase != Phase::Playing {
        return;
    }
    let (admission, _) = game.spawn_preview();
    if admission
        .next_available_in()
        .is_some_and(|wait| wait <= 0.6)
    {
        // Show eligible zones, not a promised spawn: the fixed-tick selector
        // still chooses the farthest safe position when admission succeeds.
        for p in game.entry_candidates() {
            draw_circle_lines(p.x, p.y, 28., 1.5, AMBER);
            for sign in [-1., 1.] {
                draw_line(
                    p.x + sign * 20.,
                    p.y - 8.,
                    p.x + sign * 20.,
                    p.y + 8.,
                    3.,
                    AMBER,
                );
            }
            label(
                "ENTRY",
                (p.x - 25.).clamp(ARENA.x + 8., ARENA.right() - 65.),
                p.y - 35.,
                13,
                AMBER,
            );
        }
    }
}

fn draw_arena() {
    draw_rectangle(
        ARENA.x - 9.0,
        ARENA.y - 9.0,
        ARENA.w + 18.0,
        ARENA.h + 18.0,
        Color::from_rgba(24, 40, 46, 255),
    );
    draw_rectangle(ARENA.x, ARENA.y, ARENA.w, ARENA.h, FLOOR);
    // Sparse floor joints establish scale while leaving combat silhouettes quiet.
    for x in (96..1208).step_by(64) {
        draw_line(
            x as f32,
            ARENA.y,
            x as f32,
            ARENA.bottom(),
            1.0,
            Color::from_rgba(24, 39, 45, 255),
        );
    }
    for y in (136..716).step_by(64) {
        draw_line(
            ARENA.x,
            y as f32,
            ARENA.right(),
            y as f32,
            1.0,
            Color::from_rgba(24, 39, 45, 255),
        );
    }
    draw_circle_lines(640.0, 414.0, 92.0, 1.0, LINE);
    draw_circle_lines(640.0, 414.0, 100.0, 1.0, LINE);
    draw_line(620.0, 414.0, 660.0, 414.0, 1.0, LINE);
    draw_line(640.0, 394.0, 640.0, 434.0, 1.0, LINE);
    label("07", 596.0, 435.0, 56, Color::from_rgba(32, 51, 56, 255));
    for (i, cover) in COVER.iter().enumerate() {
        draw_rectangle(cover.x + 5.0, cover.y + 8.0, cover.w, cover.h, INK);
        draw_rectangle(
            cover.x,
            cover.y,
            cover.w,
            cover.h,
            Color::from_rgba(35, 51, 57, 255),
        );
        draw_rectangle_lines(
            cover.x,
            cover.y,
            cover.w,
            cover.h,
            1.4,
            Color::from_rgba(69, 91, 94, 255),
        );
        draw_line(
            cover.x + 9.0,
            cover.y + 12.0,
            cover.right() - 9.0,
            cover.y + 12.0,
            2.0,
            LINE,
        );
        for x in 0..4 {
            draw_line(
                cover.x + 12.0 + x as f32 * 10.0,
                cover.bottom() - 8.0,
                cover.x + 17.0 + x as f32 * 10.0,
                cover.bottom() - 13.0,
                3.0,
                Color::from_rgba(139, 116, 68, 255),
            );
        }
        label(
            &format!("C{}", i + 1),
            cover.right() - 27.0,
            cover.bottom() - 10.0,
            14,
            MUTED,
        );
    }
    for p in [
        vec2(72.0, 414.0),
        vec2(1208.0, 414.0),
        vec2(640.0, 112.0),
        vec2(640.0, 716.0),
    ] {
        draw_circle(p.x, p.y, 4.0, CORAL);
        draw_circle_lines(
            p.x,
            p.y,
            9.0,
            1.0,
            Color::new(CORAL.r, CORAL.g, CORAL.b, 0.35),
        );
    }
    for (x, y, sx, sy) in [
        (72.0, 112.0, 1.0, 1.0),
        (1208.0, 112.0, -1.0, 1.0),
        (72.0, 716.0, 1.0, -1.0),
        (1208.0, 716.0, -1.0, -1.0),
    ] {
        draw_line(x, y, x + sx * 25.0, y, 2.0, CYAN);
        draw_line(x, y, x, y + sy * 25.0, 2.0, CYAN);
    }
}

fn draw_entities(game: &Game, presentation: &Presentation, emphasis: f32, world_hint: bool) {
    for (p, pickup) in &mut game.world.query::<(&Position, &Pickup)>() {
        let pulse = 1.0 + (game.time * 5.0).sin() * 0.1 * presentation.motion_scale();
        draw_poly(
            p.0.x,
            p.0.y,
            4,
            15.0 * pulse,
            45.0,
            Color::from_rgba(26, 69, 63, 255),
        );
        if pickup.remaining < 3.0 {
            draw_circle_lines(p.0.x, p.0.y, 19., 1., AMBER);
        }
        let color = if pickup.remaining < 3.0
            && presentation.flash_scale() > 0.
            && (game.time * 8.0).sin() < 0.0
        {
            MUTED
        } else {
            CYAN
        };
        draw_line(p.0.x - 5.0, p.0.y, p.0.x + 5.0, p.0.y, 2.0, color);
        draw_line(p.0.x, p.0.y - 5.0, p.0.x, p.0.y + 5.0, 2.0, color);
    }
    for (p, enemy) in &mut game.world.query::<(&Position, &Enemy)>() {
        if enemy.stagger.active() {
            draw_circle_lines(p.0.x, p.0.y, enemy.radius + 8., 2., CYAN);
        }
        let base = if enemy.heavy {
            Color::from_rgba(208, 109, 153, 255)
        } else {
            CORAL
        };
        let color = if enemy.flash > 0. {
            base.lerp(PAPER, presentation.flash_scale())
        } else {
            base
        };
        let heading = (game.player - p.0).to_angle().to_degrees();
        if enemy.attack.phase() == ferravex_play::action::ActionPhase::Windup {
            let progress = enemy.attack.progress();
            // The ring is the actual contact range, not a decorative expanding
            // threat radius. Progress thickens it as the shared action winds up.
            draw_circle_lines(
                p.0.x,
                p.0.y,
                enemy.radius + game::PLAYER_RADIUS,
                1.5 + progress * 2.,
                AMBER,
            );
            label(
                &format!("! {:.1}", enemy.attack.phase_remaining()),
                p.0.x - 25.,
                p.0.y - enemy.radius - 20.,
                14,
                AMBER,
            );
        } else if enemy.attack.phase() == ferravex_play::action::ActionPhase::Active {
            draw_circle_lines(p.0.x, p.0.y, enemy.radius + game::PLAYER_RADIUS, 3., CORAL);
        }
        let sides = if enemy.heavy { 6 } else { 4 };
        draw_poly(
            p.0.x + 2.0,
            p.0.y + 5.0,
            sides,
            enemy.radius + 2.0,
            heading,
            INK,
        );
        draw_poly(p.0.x, p.0.y, sides, enemy.radius, heading, color);
        draw_poly(p.0.x, p.0.y, sides, enemy.radius - 4.0, heading, FLOOR);
        let nose = Vec2::from_angle(heading.to_radians()) * (enemy.radius - 2.0);
        draw_circle(p.0.x + nose.x, p.0.y + nose.y, 3.0, color);
        if enemy.heavy {
            draw_circle(p.0.x, p.0.y, 5.0, color);
            for pip in 0..(enemy.health / 24.0).ceil() as u32 {
                draw_rectangle(
                    p.0.x - 10.0 + pip as f32 * 8.0,
                    p.0.y - 32.0,
                    5.0,
                    2.0,
                    color,
                );
            }
        }
    }
    for (p, bullet) in &mut game.world.query::<(&Position, &Bullet)>() {
        let tail = p.0 - bullet.velocity.normalize() * 16.0;
        draw_line(tail.x, tail.y, p.0.x, p.0.y, 2.5, CYAN);
        draw_circle(p.0.x, p.0.y, 2.0, PAPER);
    }
    if let Some(marker) = game.hit_marker {
        let color = if marker.lethal { CYAN } else { PAPER };
        let reach = (if marker.lethal { 11. } else { 8. }) * (1. + emphasis * 0.35);
        for (x, y) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
            draw_line(
                marker.position.x + x * 4.,
                marker.position.y + y * 4.,
                marker.position.x + x * reach,
                marker.position.y + y * reach,
                2.,
                color,
            );
        }
    }
    let p = game.player;
    let angle = game.facing.to_angle().to_degrees();
    if presentation.dash > 0.0 {
        for index in 1..5 {
            let ghost = p - game.facing * index as f32 * 12.0;
            draw_poly(
                ghost.x,
                ghost.y,
                6,
                14.0,
                angle,
                Color::new(
                    CYAN.r,
                    CYAN.g,
                    CYAN.b,
                    0.15 * presentation.motion_scale() / index as f32,
                ),
            );
        }
    }
    draw_circle(p.x + 2.0, p.y + 5.0, 17.0, INK);
    let player_color = if game.health == 0 {
        MUTED
    } else if presentation.hurt > 0.6 {
        CYAN.lerp(CORAL, presentation.flash_scale())
    } else {
        CYAN
    };
    draw_poly(p.x, p.y, 6, 15.0, angle, player_color);
    draw_poly(p.x, p.y, 6, 10.0, angle, FLOOR);
    let nose = p + game.facing * 24.0;
    draw_line(p.x, p.y, nose.x, nose.y, 6.0, player_color);
    if presentation.muzzle > 0.0 {
        draw_circle(
            nose.x,
            nose.y,
            7.0,
            Color::new(PAPER.r, PAPER.g, PAPER.b, presentation.flash_scale()),
        );
    }
    if presentation.hurt > 0.0 {
        draw_circle_lines(
            p.x,
            p.y,
            23.0,
            1.0,
            Color::new(
                CORAL.r,
                CORAL.g,
                CORAL.b,
                presentation.hurt * presentation.flash_scale(),
            ),
        );
    }
    if world_hint && game.phase == Phase::Playing && game.wave_clear_remaining().is_none() {
        let threats = game
            .world
            .query::<&Enemy>()
            .iter()
            .filter(|enemy| enemy.attack.phase() == ferravex_play::action::ActionPhase::Windup)
            .count();
        if threats > 0 {
            let text = format!(
                "EVADE / {threats} ATTACK WINDUP{}",
                if threats == 1 { "" } else { "S" }
            );
            let width = text_width(&text, 18);
            draw_rectangle(640. - width * 0.5 - 12., 675., width + 24., 28., INK);
            label(&text, 640. - width * 0.5, 695., 18, AMBER);
        }
    }
}

fn hud_label(
    ui: &mut Ui,
    position: [f32; 2],
    width: f32,
    size: f32,
    color: Color,
    align: Align,
    args: std::fmt::Arguments<'_>,
) {
    let [x, y] = position;
    ui.text_fmt(
        UiRect {
            min: [x, y - size * 0.875],
            size: [width, size],
        },
        TextStyle {
            size,
            color,
            align,
            ..Default::default()
        },
        args,
    )
    .unwrap();
}
fn draw_hud(ui: &mut Ui, game: &Game, started: bool, _input: &PlayerInput) {
    hud_label(
        ui,
        [72., 57.],
        380.,
        30.,
        PAPER,
        Align::Left,
        format_args!("NEON SWARM"),
    );
    hud_label(
        ui,
        [73., 80.],
        400.,
        14.,
        MUTED,
        Align::Left,
        format_args!(
            "{} POINTS / x{} CHAIN",
            game.score.total(),
            game.score.multiplier()
        ),
    );
    hud_label(
        ui,
        [465., 57.],
        230.,
        18.,
        CYAN,
        Align::Left,
        format_args!("WAVE {}/3", game.wave),
    );
    for wave in 1..=3 {
        ui.panel(
            UiRect {
                min: [655. + (wave - 1) as f32 * 22., 46.],
                size: [10.; 2],
            },
            if wave <= game.wave { CYAN } else { LINE },
        )
        .unwrap();
    }
    if started {
        hud_label(
            ui,
            [465., 80.],
            350.,
            14.,
            MUTED,
            Align::Left,
            format_args!(
                "{} ACTIVE / {} QUEUED",
                game.enemy_count(),
                game.remaining_enemies() - game.enemy_count() as u32
            ),
        );
    }
    hud_label(
        ui,
        [885., 80.],
        190.,
        14.,
        if game.shield.value() > 0. {
            CYAN
        } else {
            CORAL
        },
        Align::Left,
        format_args!("SHIELD {:02}", game.shield.value().ceil() as u32),
    );
    let hp = if game.health <= 30 { CORAL } else { CYAN };
    ui.panel(
        UiRect {
            min: [1088., 49.5],
            size: [16., 3.],
        },
        hp,
    )
    .unwrap();
    ui.panel(
        UiRect {
            min: [1094.5, 43.],
            size: [3., 16.],
        },
        hp,
    )
    .unwrap();
    hud_label(
        ui,
        [1120., 61.],
        110.,
        32.,
        hp,
        Align::Left,
        format_args!("{:03}", game.health),
    );
    for pip in 0..10 {
        ui.panel(
            UiRect {
                min: [1088. + pip as f32 * 12., 76.],
                size: [8., 3.],
            },
            if game.health > pip * 10 { hp } else { LINE },
        )
        .unwrap();
    }
    let dash = game.dash_availability();
    let ready = dash.check().is_ok();
    let mut dash_label = |args| {
        hud_label(
            ui,
            [73., 756.],
            180.,
            15.,
            if ready { CYAN } else { MUTED },
            Align::Left,
            args,
        );
    };
    if ready {
        dash_label(format_args!("DASH READY"));
    } else if game.phase != Phase::Playing {
        dash_label(format_args!("DASH OFFLINE"));
    } else {
        dash_label(format_args!("DASH {:.1}s", dash.cooldown_remaining));
    }
    ui.panel(
        UiRect {
            min: [73., 768.],
            size: [108., 2.],
        },
        LINE,
    )
    .unwrap();
    ui.panel(
        UiRect {
            min: [73., 768.],
            size: [108. * (1. - game.dash_cooldown / 1.7).clamp(0., 1.), 2.],
        },
        CYAN,
    )
    .unwrap();
    let progress = match game.progression.next_cost() {
        Some(cost) => format!(
            "LV {} / XP {}/{} / DMG {}",
            game.progression.level(),
            game.progression.xp(),
            cost,
            game.shot_damage()
        ),
        None => format!(
            "LV {} MAX / DMG {}",
            game.progression.level(),
            game.shot_damage()
        ),
    };
    hud_label(
        ui,
        [73., 99.],
        650.,
        13.,
        MUTED,
        Align::Left,
        format_args!("{progress}"),
    );
    if started && game.phase == Phase::Playing && game.wave_clear_remaining().is_none() {
        let (admission, position) = game.spawn_preview();
        let status = match admission {
            ferravex_play::encounter::AdmissionPreview::InsufficientCredits { .. } => {
                "CLEAR REMAINING DRONES".to_owned()
            }
            ferravex_play::encounter::AdmissionPreview::AliveLimit { .. } => {
                "ENTRY HELD / ARENA FULL".to_owned()
            }
            _ if position.is_none() => "ENTRY HELD / NO SAFE POSITION".to_owned(),
            _ => format!(
                "ENTRY WINDOW {:.1}s / IF CLEAR",
                admission.next_available_in().unwrap_or(0.)
            ),
        };
        hud_label(
            ui,
            [730., 99.],
            478.,
            13.,
            AMBER,
            Align::Right,
            format_args!("{status}"),
        );
    }
    hud_label(
        ui,
        [1097., 757.],
        160.,
        15.,
        MUTED,
        Align::Left,
        format_args!(
            "{}",
            if game.magazine.reloading() {
                format!("RELOAD {:.1}", game.magazine.remaining())
            } else {
                format!("{:02}/12 CELL", game.magazine.loaded())
            }
        ),
    );
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
        return;
    }
    ui.panel(
        UiRect {
            min: [0., 100.],
            size: [1280., 626.],
        },
        Color::new(0.025, 0.055, 0.07, 0.82),
    )
    .unwrap();
    ui.panel(
        UiRect {
            min: [602., 323.],
            size: [76., 3.],
        },
        CYAN,
    )
    .unwrap();
    hud_label(
        ui,
        [80., 386.],
        1120.,
        52.,
        PAPER,
        Align::Center,
        format_args!("{title}"),
    );
    hud_label(
        ui,
        [80., 430.],
        1120.,
        21.,
        MUTED,
        Align::Center,
        format_args!("{detail}"),
    );
    ui.button(
        1,
        UiRect {
            min: [470., 468.],
            size: [340., 52.],
        },
        action,
        true,
        ButtonStyle {
            fill: FLOOR,
            ink: CYAN,
            focus: CYAN,
            ..Default::default()
        },
    )
    .unwrap();
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
