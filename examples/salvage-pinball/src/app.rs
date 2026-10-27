use crate::{
    BALL_RADIUS, BUMPERS, CRANE, CRANE_RADIUS, FLIPPER_LENGTH, MAX_MULTIPLIER, PIVOTS,
    PinballFeedback, PinballGame, RAILS, Table,
};
use ferravex_core::session::SessionState;
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{
    ActionKind, Binding, HintDevice, InputCode, PlayerInput, Preset,
    touch::{Control, Output, Region},
};
use ferravex_render::{demo::Demo, prelude::*};
#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
mod capture;
mod dmd;
mod hud;
mod input;
mod layout;
mod presentation;
mod quips;
mod table;
use presentation::{Presentation, hint};

const INK: Color = Color::from_rgba(19, 27, 27, 255);
const PANEL: Color = Color::from_rgba(34, 49, 46, 255);
const BRASS: Color = Color::from_rgba(190, 157, 102, 255);
const CREAM: Color = Color::from_rgba(235, 220, 185, 255);
const RUST: Color = Color::from_rgba(167, 77, 47, 255);
const MINT: Color = Color::from_rgba(128, 166, 147, 255);
/// Header height and widest HUD column in logical pixels.
const HEADER: f32 = 84.;
const HUD_MAX: f32 = 516.;

/// Records context for this table's rules (multiplier, crane and secrets).
const RECORD_RULES: u64 = 0x5341_4c56_0002;
const RECORD_ID: &str = "table";

fn window_conf() -> Conf {
    #[allow(unused_mut)]
    let mut conf = ferravex_render::window_conf("Salvage Pinball / Ferravex");
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    capture::window(&mut conf);
    conf
}

fn fitted_text(text: &str, size: f32, width: f32) -> Option<(f32, f32)> {
    if !width.is_finite() || width <= 0. {
        return None;
    }
    let font = get_default_font();
    let metrics = font
        .try_metrics(8, size / 8.)
        .ok()?
        .measure_line(text)
        .ok()?;
    let size = size * (width / metrics.width.max(1.)).min(1.);
    let metrics = font
        .try_metrics(8, size / 8.)
        .ok()?
        .measure_line(text)
        .ok()?;
    Some((size, metrics.width))
}

fn centered_in(text: &str, center: Vec2, size: f32, width: f32, color: Color) {
    if let Some((size, width)) = fitted_text(text, size, width) {
        draw_text(text, center.x - width * 0.5, center.y, size, color);
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0., 1.))
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0., 1.);
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

/// Browser fullscreen pins a 44×44 menu button at the canvas's top-right,
/// inset 8px. HUD text and controls stay out of this `(width, height)` corner.
#[cfg(test)]
const MENU_CORNER: (f32, f32) = (72., 60.);

use layout::{HudLayout, hud_layout};

fn table_layout(w: f32, h: f32) -> (f32, Vec2, Rect) {
    let hud = hud_layout(w);
    let scale = ((h - hud.header - 106.0) / 16.0)
        .min((w - 28.0) / 10.5)
        .max(1.0);
    let origin = vec2(w * 0.5 - 5.15 * scale, hud.header + 4.);
    (scale, origin, hud.reset)
}

/// The dot-matrix strip under the table, at least as wide as a card's text.
#[cfg(test)]
fn dmd_rect(w: f32, scale: f32, origin: Vec2) -> Rect {
    let width = (10.3 * scale).max((w - 24.).clamp(1., 400.));
    Rect::new(
        w * 0.5 - width * 0.5,
        origin.y + 16. * scale + 12.,
        width,
        44.,
    )
}

fn readable_label<'a>(text: &'a str, fallback: &'a str, size: f32, width: f32) -> &'a str {
    match fitted_text(text, size, width) {
        Some((fitted, _)) if fitted >= size - 0.01 => text,
        _ => fallback,
    }
}

fn table_render(
    game: &PinballGame,
    view: &Presentation,
    input: &PlayerInput,
    frame: &Frame,
    bufs: &mut hud::Buffers,
) {
    let table = game.table();
    let state = game.session().state();
    let mut buffers = [[0; 128]; 6];
    let [launch, pause, restart, confirm, left, right] = &mut buffers;
    let hints = hud::Hints {
        launch: hint(input, "launch", launch),
        pause: hint(input, "pause", pause),
        restart: hint(input, "restart", restart),
        confirm: hint(input, "confirm", confirm),
        left: hint(input, "flipper_left", left),
        right: hint(input, "flipper_right", right),
        touch: input.current_device() == HintDevice::Touch,
    };
    let layout = layout::layout(screen_width(), screen_height());
    clear_background(INK);
    table::draw(
        &table::World {
            game,
            view,
            layout: &layout,
            attract: state == SessionState::Ready,
            state,
        },
        bufs,
    );
    if layout.wide {
        hud::side_panels(&layout, table, view, frame.best, &hints);
    } else {
        hud::header(table, view, frame.best, layout.header);
        if state == SessionState::Playing {
            hud::footer_hint(layout.hint_y, table, view, &hints);
        }
    }
    let center = vec2(screen_width() * 0.5, layout.origin.y + 7.4 * layout.scale);
    match state {
        SessionState::Ready => hud::ready(center, frame.best, &hints, frame.seed),
        SessionState::Paused => hud::paused(center, input, &hints, frame.seed),
        SessionState::Lost => hud::results(center, table, frame.results, &hints),
        _ => {}
    }
    if view.cover > 0. {
        draw_rectangle(
            0.,
            0.,
            screen_width(),
            screen_height(),
            Color::new(0., 0., 0., view.cover),
        );
    }
}

fn dmd_lines<'a>(
    table: &Table,
    view: &Presentation,
    state: SessionState,
    buffers: &'a mut hud::Buffers,
) -> (dmd::Line<'a>, dmd::Line<'a>, f32) {
    buffers.head.clear();
    buffers.sub.clear();
    let (color, alpha, time) = if let Some(callout) = &view.callout {
        buffers.head.push_str(&callout.head);
        buffers.sub.push_str(callout.sub);
        (
            callout.color,
            ((callout.life - callout.age) / 0.2).clamp(0., 1.),
            callout.age,
        )
    } else {
        let (head, sub) = hud::objective(table, state);
        buffers.head.push_str(&head);
        buffers.sub.push_str(sub);
        (presentation::AMBER, 1., view.menu_time)
    };
    (
        dmd::Line {
            text: &buffers.head,
            color: with_alpha(color, alpha),
        },
        dmd::Line {
            text: &buffers.sub,
            color: with_alpha(presentation::AMBER, alpha * 0.72),
        },
        time,
    )
}

/// Per-frame values the renderer needs from the loop.
struct Frame {
    best: Option<u32>,
    results: hud::Results,
    seed: u32,
}

fn play_feedback(sound: &mut GameAudio, events: PinballFeedback) {
    for pressed in events.flipper_presses {
        if pressed {
            // Positional Click is a gameplay voice: local pause/reset must
            // retire a flipper strike, unlike an independent menu/UI click.
            sound.at(Cue::Click, Vec3::ZERO, 0.3);
        }
    }
    if events.hits > 0 {
        // Chains ring a little louder; the mixer caps total voices.
        let gain = 0.6 + 0.05 * events.chain.min(6) as f32;
        sound.cue(Cue::Impact(Surface::Metal), gain);
    }
    if events.launches > 0 {
        sound.cue(Cue::Jump, 0.55);
    }
    if events.crane_arms > 0 {
        sound.cue(Cue::Reload, 0.6);
    }
    if events.jackpots > 0 || events.skill_shots > 0 || events.secrets > 0 {
        sound.cue(Cue::Success, 0.7);
    }
    if events.crane_drops > 0 {
        sound.cue(Cue::Impact(Surface::Stone), 0.45);
    }
    if events.saves > 0 {
        sound.cue(Cue::Pickup, 0.5);
    }
    if events.drains > events.saves {
        sound.cue(Cue::Failure, 0.4);
    }
}

fn best_score(records: Option<&ferravex_render::storage::Records>) -> Option<u32> {
    records?
        .get(RECORD_ID, RECORD_RULES)
        .map(|record| record.score.min(u64::from(u32::MAX)) as u32)
}

ferravex_render::game!(window_conf, game);
async fn game() {
    let mut demo = Demo::new();
    let mut sound = GameAudio::new();
    let mut pinball = PinballGame::new(demo.autoplay);
    let mut presentation = Presentation::default();
    let mut hud_buffers = hud::Buffers::new();
    let mut post_state = None;
    // Personal bests are optional: a storage failure still leaves a playable game.
    let mut records = ferravex_render::storage::Records::new("salvage-pinball").ok();
    let mut results = hud::Results::default();
    let mut games: u32 = 0;
    let mut pauses: u32 = 0;
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    let mut capture = capture::Capture::from_env();
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    {
        pinball.lazy_bot = capture.lazy_bot();
    }
    let mut player_input = PlayerInput::preset("salvage-pinball", Preset::Pinball);
    player_input
        .action(
            "table_pointer",
            "Mouse launch / flip",
            ActionKind::Button,
            vec![Binding::digital(InputCode::Mouse(1))],
        )
        .unwrap();
    for button in [0, 1] {
        player_input
            .add_default_binding("launch", Binding::digital(InputCode::Touch(button)))
            .unwrap();
    }
    let mut game_over_time = 0.0;
    loop {
        presentation.begin_frame();
        ferravex_render::ui::warm_up_font_sizes(&[13, 14, 16, 17, 20, 21, 32, 34]);
        let split = screen_width() * 0.5;
        let lower = screen_height() * 0.5;
        player_input
            .set_touch_layout(&[
                Control {
                    region: Region::Rect(Rect::new(0., lower, split, lower)),
                    output: Output::Button(0),
                },
                Control {
                    region: Region::Rect(Rect::new(split, lower, split, lower)),
                    output: Output::Button(1),
                },
                Control {
                    region: Region::Rect(Rect::new(0., 0., split * 2., lower)),
                    output: Output::Button(8),
                },
            ])
            .unwrap();
        player_input.sample_play();
        let pointer = Vec2::from(mouse_position());
        let touches = ferravex_render::input::logical_touches();
        let clicked = is_mouse_button_pressed(MouseButton::Left);
        let tapped = clicked || touches.iter().any(|t| t.phase == TouchPhase::Started);
        let reset = layout::layout(screen_width(), screen_height()).reset;
        let reset_tapped = clicked && reset.contains(pointer)
            || touches
                .iter()
                .any(|t| t.phase == TouchPhase::Started && reset.contains(t.position));
        #[allow(unused_mut)]
        let mut active = demo.autoplay || player_input.gameplay_active();
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        {
            active &= !capture.paused();
        }
        if pinball.set_paused(!active) && !active {
            pauses = pauses.wrapping_add(1);
        }
        if active {
            sound.resume();
        } else {
            sound.pause();
        }
        let confirm = player_input.pressed("confirm");
        let restart = player_input.pressed("restart")
            || active && (reset_tapped || pinball.table().over() && (confirm || tapped))
            || demo.autoplay && game_over_time > 2.5;
        let dt = demo.dt();
        if restart {
            pinball.restart();
            presentation.reset();
            sound.reset();
            // Suppress held physical actions as well as the game's buffered edges.
            player_input.set_paused(true);
            player_input.set_paused(false);
            game_over_time = 0.;
            games = games.wrapping_add(1);
        } else {
            if active && confirm {
                pinball.start();
            }
            #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
            if capture.start_now() {
                pinball.start();
            }
            let was_over = pinball.table().over();
            let events = input::advance(&mut pinball, &mut player_input, dt as f64, pointer);
            presentation.finish_frame(dt, pinball.table(), events);
            play_feedback(&mut sound, events);
            if pinball.table().over() && !was_over {
                // Compare with the stored best before recording this game.
                results = hud::Results {
                    previous_best: best_score(records.as_ref()),
                    demo: demo.autoplay,
                };
                #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
                if let Some(best) = capture.fake_best() {
                    results = hud::Results {
                        previous_best: Some(best),
                        demo: false,
                    };
                }
                if !demo.autoplay
                    && let Some(records) = records.as_mut()
                {
                    let score = u64::from(pinball.table().score);
                    let _ = records.attempt(RECORD_ID, RECORD_RULES, score, None);
                }
            }
        }
        if pinball.table().over() && active {
            game_over_time += dt;
        }
        #[allow(unused_mut)]
        let mut best = best_score(records.as_ref());
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        if let Some(fake) = capture.fake_best() {
            best = Some(fake.max(pinball.table().score * u32::from(pinball.table().over())));
        }
        let frame = Frame {
            best,
            results,
            seed: games.wrapping_add(pauses),
        };
        presentation.observe_state(pinball.session().state(), dt);
        presentation.hover(reset.contains(pointer), dt);
        let state = pinball.session().state();
        if post_state != Some(state) {
            set_post_fx(
                ferravex_render::post::PostFx::neon()
                    .bloom(1.15, 0.76, 1.1)
                    .vignette(
                        if state == SessionState::Playing {
                            0.28
                        } else {
                            0.4
                        },
                        0.65,
                    )
                    .grain(0.)
                    .aberration(0.),
            );
            post_state = Some(state);
        }
        table_render(
            &pinball,
            &presentation,
            &player_input,
            &frame,
            &mut hud_buffers,
        );
        player_input.draw_controls();
        #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
        capture.end_frame();
        if demo.finish_frame() {
            println!(
                "salvage-pinball: score={} hits={} launches={} drains={} jackpots={}",
                pinball.table().score,
                pinball.table().hits,
                pinball.table().launches,
                pinball.table().drains,
                pinball.table().jackpots,
            );
            break;
        }
        next_frame().await;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Controls;

    #[test]
    fn hud_keeps_the_fullscreen_menu_corner_clear_and_fits_every_canvas() {
        for (w, h) in [
            (1280., 800.),
            (1600., 1000.),
            (390., 844.),
            (844., 390.),
            (320., 480.),
            (320., 540.),
            (390., 694.),
            (800., 1000.),
        ] {
            let hud = hud_layout(w);
            let (scale, origin, reset) = table_layout(w, h);
            assert_eq!(reset, hud.reset);
            let text = |t: &str, size: f32| fitted_text(t, size, f32::MAX).unwrap().1;
            // Every header element's rectangle: (left, top, right, bottom).
            let mult = text("MULT 5X", 13.);
            let elements = [
                ("score", hud.left, 0., hud.left + hud.score_room, 58.),
                (
                    "best",
                    hud.left,
                    hud.best_y - 13.,
                    hud.left + hud.best_room,
                    hud.best_y,
                ),
                ("balls", hud.balls_right - 64., 8., hud.balls_right, 47.),
                (
                    "mult",
                    hud.mult_right - mult,
                    hud.mult_y - 13.,
                    hud.mult_right,
                    hud.mult_y,
                ),
                (
                    "reset",
                    reset.x,
                    reset.y,
                    reset.x + reset.w,
                    reset.y + reset.h,
                ),
            ];
            for (name, left, top, right, bottom) in elements {
                let in_corner = right > w - MENU_CORNER.0 && top < MENU_CORNER.1;
                assert!(!in_corner, "{name} enters the menu corner at {w}x{h}");
                assert!(left >= 4. && right <= w - 4., "{name} inside at {w}x{h}");
                assert!(bottom <= hud.header, "{name} stays in the header");
            }
            // Row items never collide with one another.
            assert!(hud.left + hud.score_room <= hud.balls_right - 64.);
            assert!(hud.left + hud.best_room <= hud.mult_right - mult);
            assert!(hud.mult_right <= reset.x || hud.mult_y < reset.y);
            let (size, _) = fitted_text("1,234,567", 32., hud.score_room).unwrap();
            assert!(size >= 18., "score shrinks to {size} at {w}x{h}");
            let (size, _) = fitted_text("BEST 1,234,567", 13., hud.best_room).unwrap();
            assert!(size >= 12., "best shrinks to {size} at {w}x{h}");
            assert!(reset.w >= 44. && reset.h >= 44.);
            assert!(origin.y >= hud.header, "table clears the header");
            assert!(origin.x >= 4. && origin.x + 10.3 * scale <= w - 4.);
            // The dot-matrix display and the hint line stay inside the canvas.
            let dmd = dmd_rect(w, scale, origin);
            assert!(dmd.x >= 4. && dmd.x + dmd.w <= w - 4.);
            assert!(dmd.y >= origin.y + 16. * scale + 8.);
            assert!(dmd.y + dmd.h + 22. <= h - 6., "hint fits at {w}x{h}");
            let panel_width = (w - 24.).clamp(1., 460.) - 28.;
            let (title_size, _) = fitted_text("READY FOR YOUR SHIFT?", 21., panel_width).unwrap();
            assert!(title_size >= 17.);
            // The tallest card (results) fits between the header and canvas end.
            let center = origin.y + 7.4 * scale;
            let tall = hud::RESULTS_HEIGHT;
            assert!((center - tall * 0.5).max(hud.header + 4.) + tall <= h);
        }
    }

    #[test]
    fn oversized_hud_bindings_use_readable_controls_guidance() {
        let long = "LEFT SHIFT / SPACE / J / SOUTH BUTTON TO START AND LAUNCH";
        for width in [272., 296., 366., 516.] {
            let label = readable_label(long, "BINDINGS: SEE CONTROLS", 13., width);
            assert_eq!(label, "BINDINGS: SEE CONTROLS");
            assert_eq!(fitted_text(label, 13., width).unwrap().0, 13.);
            for fallback in [
                "FLIPPERS: SEE CONTROLS",
                "RELEASE / LAUNCH: SEE CONTROLS",
                "SCORE SHOWN ABOVE",
            ] {
                assert_eq!(fitted_text(fallback, 13., width.max(296.)).unwrap().0, 13.);
            }
        }
        assert_eq!(
            readable_label("SPACE TO LAUNCH", "BINDINGS: SEE CONTROLS", 13., 296.),
            "SPACE TO LAUNCH"
        );
    }

    #[test]
    fn accepted_flipper_and_launch_audio_pauses_without_a_late_resume_burst() {
        let mut game = PinballGame::new(false);
        let mut sound = GameAudio::new();
        let pressed = Controls {
            left: true,
            right: true,
            launch: true,
        };
        play_feedback(&mut sound, game.advance(0., pressed));
        assert_eq!(ferravex_render::audio::stats().active, 0);
        let events = game.advance(1. / 120., pressed);
        play_feedback(&mut sound, events);
        assert_eq!(ferravex_render::audio::stats().active, 3);
        sound.pause();
        assert_eq!(ferravex_render::audio::stats().active, 0);
        play_feedback(&mut sound, events);
        assert_eq!(ferravex_render::audio::stats().active, 0);
        sound.resume();
        play_feedback(&mut sound, game.advance(0., pressed));
        assert_eq!(ferravex_render::audio::stats().active, 0);
        game.advance(1. / 120., Controls::default());
        play_feedback(&mut sound, game.advance(1. / 120., pressed));
        assert_eq!(ferravex_render::audio::stats().active, 2);
        sound.reset();
        assert_eq!(ferravex_render::audio::stats().active, 0);
    }

    #[test]
    fn fractional_text_measurement_fits_long_remaps_inside_the_panel() {
        let label = "LEFT SHIFT / SPACE / J TO START AND LAUNCH";
        let (size, width) = fitted_text(label, 13.75, 140.).unwrap();
        assert!(size < 13.75);
        assert!(width <= 140.001);
        let drawn_width = get_default_font()
            .try_metrics(8, size / 8.)
            .unwrap()
            .measure_line(label)
            .unwrap()
            .width;
        assert_eq!(drawn_width, width);
        assert!(fitted_text(label, f32::NAN, 140.).is_none());
        assert!(fitted_text(label, 13.75, 0.).is_none());
    }
}
