use crate::{BALL_RADIUS, BUMPERS, FLIPPER_LENGTH, PIVOTS, PinballFeedback, PinballGame, RAILS};
use ferravex_core::session::SessionState;
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{
    ActionKind, Binding, HintDevice, InputCode, PlayerInput, Preset,
    touch::{Control, Output, Region},
};
use ferravex_render::{demo::Demo, prelude::*};
mod input;
mod presentation;
use presentation::{Presentation, hint};

const INK: Color = Color::from_rgba(19, 27, 27, 255);
const PANEL: Color = Color::from_rgba(34, 49, 46, 255);
const BRASS: Color = Color::from_rgba(190, 157, 102, 255);
const CREAM: Color = Color::from_rgba(235, 220, 185, 255);
const RUST: Color = Color::from_rgba(167, 77, 47, 255);
const MINT: Color = Color::from_rgba(128, 166, 147, 255);

fn window_conf() -> Conf {
    ferravex_render::window_conf("Salvage Pinball / Ferravex")
}

fn centered(text: &str, center: Vec2, size: f32, color: Color) {
    centered_in(text, center, size, (screen_width() - 24.).max(1.), color);
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

fn capsule(a: Vec2, b: Vec2, radius: f32, color: Color) {
    draw_line(a.x, a.y, b.x, b.y, radius * 2.0, color);
    draw_circle(a.x, a.y, radius, color);
    draw_circle(b.x, b.y, radius, color);
}

fn table_layout(w: f32, h: f32) -> (f32, Vec2, Rect) {
    let scale = ((h - 180.0) / 16.2).min((w - 28.0) / 10.5).max(1.0);
    let origin = vec2(w * 0.5 - 5.15 * scale, 84.0);
    // The HUD uses logical pixels independently of the physics table scale.
    // A short window must not squeeze Reset into the game title.
    let hud_width = (w - 24.).clamp(1., 516.);
    let reset = Rect::new((w + hud_width) * 0.5 - 124., 8., 68., 44.);
    (scale, origin, reset)
}

fn readable_label<'a>(text: &'a str, fallback: &'a str, size: f32, width: f32) -> &'a str {
    match fitted_text(text, size, width) {
        Some((fitted, _)) if fitted >= size - 0.01 => text,
        _ => fallback,
    }
}

fn hud_line(text: &str, fallback: &str, y: f32, size: f32, color: Color) {
    let width = (screen_width() - 24.).clamp(1., 516.);
    centered_in(
        readable_label(text, fallback, size, width),
        vec2(screen_width() * 0.5, y),
        size,
        width,
        color,
    );
}

fn table_render(game: &PinballGame, presentation: &Presentation, input: &PlayerInput) {
    let table = game.table();
    let state = game.session().state();
    let touch_mode = input.current_device() == HintDevice::Touch;
    let mut buffers = [[0; 128]; 4];
    let [launch_buf, pause_buf, restart_buf, confirm_buf] = &mut buffers;
    let launch = hint(input, "launch", launch_buf);
    let pause = hint(input, "pause", pause_buf);
    let restart = hint(input, "restart", restart_buf);
    let confirm = hint(input, "confirm", confirm_buf);
    let w = screen_width();
    let h = screen_height();
    let (scale, origin, reset) = table_layout(w, h);
    let p = |xy: [f32; 2]| origin + Vec2::from_array(xy) * scale;
    clear_background(INK);
    ferravex_render::set_world_colors(true);
    // A sparse workshop backdrop leaves the actual table as the focal point.
    for i in 0..8 {
        let y = i as f32 * 113.0;
        draw_line(0.0, y, w, y + 80.0, 1.0, Color::from_rgba(31, 39, 36, 255));
    }
    let board = Rect::new(origin.x, origin.y, 10.3 * scale, 16.0 * scale);
    draw_rectangle(board.x + 7.0, board.y + 9.0, board.w, board.h, BLACK);
    draw_rectangle(
        board.x - 4.0,
        board.y - 4.0,
        board.w + 8.0,
        board.h + 8.0,
        BRASS,
    );
    draw_rectangle(board.x, board.y, board.w, board.h, PANEL);
    for y in 0..22 {
        let a = p([0.2, 0.4 + y as f32 * 0.7]);
        draw_line(
            a.x,
            a.y,
            a.x + 9.9 * scale,
            a.y,
            1.0,
            Color::from_rgba(40, 54, 48, 255),
        );
    }
    for xy in [[0.2, 0.2], [10.1, 0.2], [0.2, 15.8], [10.1, 15.8]] {
        let v = p(xy);
        draw_circle(v.x, v.y, 2.5, INK);
    }
    let lane = p([8.8, 3.8]);
    draw_rectangle(
        lane.x,
        lane.y,
        0.9 * scale,
        11.3 * scale,
        Color::from_rgba(25, 38, 37, 255),
    );
    // Reclaimed painted metal shapes give the lane and apron a coherent industrial theme.
    for (a, b, c) in [
        ([0.8, 8.0], [2.7, 10.9], [0.8, 11.2]),
        ([8.6, 8.0], [6.6, 10.9], [8.6, 11.2]),
    ] {
        draw_triangle(p(a), p(b), p(c), RUST);
    }
    for i in 0..4 {
        let y = 8.8 + i as f32 * 0.55;
        let a = p([1.0, y]);
        let b = p([1.55, y + 0.35]);
        draw_line(a.x, a.y, b.x, b.y, 2.0, BRASS);
        let a = p([8.35, y]);
        let b = p([7.8, y + 0.35]);
        draw_line(a.x, a.y, b.x, b.y, 2.0, BRASS);
    }
    centered("SALVAGE", p([4.5, 2.1]), 0.62 * scale, CREAM);
    centered("YARD  /  03", p([4.5, 2.65]), 0.24 * scale, MINT);
    centered("KEEP IT IN PLAY", p([4.6, 8.9]), 0.3 * scale, MINT);
    centered(
        "RECOVER. REBOUND. REPEAT.",
        p([4.6, 9.42]),
        0.18 * scale,
        BRASS,
    );
    for (a, b) in RAILS {
        capsule(
            p(a) + vec2(2.0, 3.0),
            p(b) + vec2(2.0, 3.0),
            0.16 * scale,
            INK,
        );
        capsule(p(a), p(b), 0.1 * scale, BRASS);
        capsule(
            p(a) - vec2(0.0, 1.0),
            p(b) - vec2(0.0, 1.0),
            0.035 * scale,
            CREAM,
        );
    }
    for (i, point) in BUMPERS.into_iter().enumerate() {
        let v = p(point);
        let flash = presentation.flash(i);
        let ring = presentation.ring(i);
        draw_poly(v.x + 3.0, v.y + 4.0, 10, 0.86 * scale, 18.0, INK);
        draw_poly(v.x, v.y, 10, 0.78 * scale, 18.0, BRASS);
        draw_circle(
            v.x,
            v.y,
            0.62 * scale,
            Color::new(
                RUST.r + (CREAM.r - RUST.r) * flash,
                RUST.g + (CREAM.g - RUST.g) * flash,
                RUST.b + (CREAM.b - RUST.b) * flash,
                1.,
            ),
        );
        draw_circle_lines(v.x, v.y, 0.49 * scale, 1.5, BRASS);
        centered(
            "100",
            v + vec2(0.0, 0.11 * scale),
            0.31 * scale,
            if flash > 0.5 { INK } else { CREAM },
        );
        if ring > 0.0 {
            draw_circle_lines(
                v.x,
                v.y,
                (1.05 - ring * 0.25) * scale,
                2.0,
                Color::new(0.95, 0.73, 0.38, ring),
            );
        }
    }
    for (i, pivot) in PIVOTS.into_iter().enumerate() {
        let end = Vec2::from_array(pivot) + Vec2::from_angle(table.angles[i]) * FLIPPER_LENGTH;
        let a = p(pivot);
        let b = p(end.to_array());
        capsule(a + vec2(2.0, 4.0), b + vec2(2.0, 4.0), 0.2 * scale, INK);
        capsule(a, b, 0.16 * scale, CREAM);
        capsule(
            a,
            b,
            0.07 * scale,
            if table.flipper_pressed[i] { MINT } else { RUST },
        );
        draw_circle(a.x, a.y, 0.09 * scale, BRASS);
        draw_circle(a.x, a.y, 0.035 * scale, INK);
    }
    let spring = p([9.25, 15.15]);
    for i in 0..5 {
        let y = spring.y + i as f32 * 0.1 * scale;
        draw_line(
            spring.x - 0.2 * scale,
            y,
            spring.x + 0.2 * scale,
            y + 0.06 * scale,
            1.0,
            BRASS,
        );
    }
    centered("DRAIN", p([4.95, 15.6]), 0.22 * scale, BRASS);
    if !table.over() {
        for (i, point) in table.trail[..presentation.trail_len]
            .iter()
            .enumerate()
            .rev()
        {
            let v = p(*point);
            draw_circle(
                v.x,
                v.y,
                ((12 - i) as f32 / 12.0) * BALL_RADIUS * scale,
                Color::new(0.72, 0.81, 0.78, 0.12 * presentation.motion_scale),
            );
        }
        let v = p(table.position());
        draw_circle(
            v.x + 2.5,
            v.y + 3.5,
            BALL_RADIUS * scale,
            Color::new(0.0, 0.0, 0.0, 0.6),
        );
        draw_circle(v.x, v.y, BALL_RADIUS * scale, MINT);
        draw_circle(v.x - 1.0, v.y - 1.5, BALL_RADIUS * scale * 0.7, CREAM);
        draw_circle(v.x - 2.0, v.y - 2.5, BALL_RADIUS * scale * 0.25, WHITE);
    }
    ferravex_render::set_world_colors(false);
    // Text accompanies the three ball lamps so lives are not color-only feedback.
    let hud_width = (w - 24.).clamp(1., 516.);
    let hud_x = (w - hud_width) * 0.5;
    draw_text("SALVAGE", hud_x, 31.0, 16.0, BRASS);
    let score = format!("{:06}", table.score);
    if let Some((size, _)) = fitted_text(&score, 32., hud_width - 88.) {
        draw_text(&score, hud_x, 72., size, CREAM);
    }
    if presentation.score_points > 0 {
        centered(
            &format!("+{} RECOVERED", presentation.score_points),
            p([4.7, 7.8]),
            13.,
            CREAM,
        );
    }
    centered(
        &format!("THIS BALL / {}", table.ball_points()),
        p([4.7, 14.6]),
        13.,
        MINT,
    );
    for i in 0..3 {
        draw_circle(
            hud_x + hud_width - 49.0 + i as f32 * 16.0,
            62.0,
            4.0,
            if i < table.lives { CREAM } else { PANEL },
        );
    }
    draw_rectangle_lines(reset.x, reset.y, reset.w, reset.h, 1., MINT);
    centered("RESET", vec2(reset.x + reset.w * 0.5, 35.), 14., MINT);
    draw_text(
        &format!("{} BALLS", table.lives),
        hud_x + hud_width - 61.,
        80.,
        13.,
        MINT,
    );
    let footer = board.y + board.h;
    if table.save_notice > 0. {
        hud_line(
            "BALL SAVED / NO LIFE LOST",
            "BALL SAVED",
            footer + 22.,
            13.,
            MINT,
        );
    } else if table.drain_notice > 0. && !table.over() {
        hud_line(
            &format!(
                "DRAIN / {} POINTS / {} BALLS LEFT",
                table.last_ball_points().unwrap_or(0),
                table.lives
            ),
            "BALL DRAINED / SERVE AGAIN",
            footer + 22.,
            13.,
            CREAM,
        );
    } else if table.ball_save > 0. {
        hud_line(
            &format!("BALL SAVE / {}s", table.ball_save.ceil() as u32),
            "BALL SAVE ACTIVE",
            footer + 22.,
            13.,
            MINT,
        );
    }
    if table.waiting && state == SessionState::Playing {
        hud_line(
            &format!("{launch} TO LAUNCH / RELEASE FIRST"),
            "RELEASE / LAUNCH: SEE CONTROLS",
            footer + 44.,
            13.,
            CREAM,
        );
    }
    if matches!(
        state,
        SessionState::Ready | SessionState::Paused | SessionState::Lost
    ) {
        let center = vec2(w * 0.5, p([5.15, 7.7]).y);
        let panel_width = (w - 24.).clamp(1., 460.);
        draw_rectangle(
            center.x - panel_width * 0.5,
            center.y - 67.0,
            panel_width,
            136.0,
            Color::new(0.07, 0.1, 0.1, 0.96),
        );
        let (title, detail, action) = match state {
            SessionState::Ready => (
                "READY FOR YOUR SHIFT?",
                "THREE BALLS / 100 PER BUMPER".to_owned(),
                format!("{launch} TO START AND LAUNCH"),
            ),
            SessionState::Paused => (
                "TABLE PAUSED",
                "BALL AND SAVE TIMER HELD".to_owned(),
                if input.controls_open() {
                    "CLOSE CONTROLS TO RETURN".to_owned()
                } else if input.paused() {
                    format!("{pause} RESUME / {restart} RESTART")
                } else {
                    "RETURN FOCUS TO THE TABLE".to_owned()
                },
            ),
            _ => (
                "SHIFT COMPLETE",
                format!("{} POINTS RECOVERED", table.score),
                if touch_mode {
                    "TAP TO RETRY / THEN LAUNCH".to_owned()
                } else {
                    format!("{restart} / {confirm} TO RETRY")
                },
            ),
        };
        centered_in(
            title,
            center - vec2(0.0, 17.0),
            20.,
            panel_width - 24.,
            CREAM,
        );
        centered_in(
            readable_label(&detail, "SCORE SHOWN ABOVE", 13., panel_width - 24.),
            center + vec2(0.0, 13.0),
            13.,
            panel_width - 24.,
            BRASS,
        );
        centered_in(
            readable_label(&action, "BINDINGS: SEE CONTROLS", 13., panel_width - 24.),
            center + vec2(0.0, 39.0),
            13.,
            panel_width - 24.,
            MINT,
        );
    }
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
        sound.cue(Cue::Impact(Surface::Metal), 0.65);
    }
    if events.launches > 0 {
        sound.cue(Cue::Jump, 0.55);
    }
    if events.saves > 0 {
        sound.cue(Cue::Pickup, 0.5);
    }
    if events.drains > events.saves {
        sound.cue(Cue::Failure, 0.4);
    }
}

ferravex_render::game!(window_conf, game);
async fn game() {
    let mut demo = Demo::new();
    let mut sound = GameAudio::new();
    let mut pinball = PinballGame::new(demo.autoplay);
    let mut presentation = Presentation::default();
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
        ferravex_render::ui::warm_up_font_sizes(&[13, 16, 17, 25, 32]);
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
        let (_, _, reset) = table_layout(screen_width(), screen_height());
        let reset_tapped = clicked && reset.contains(pointer)
            || touches
                .iter()
                .any(|t| t.phase == TouchPhase::Started && reset.contains(t.position));
        let active = demo.autoplay || player_input.gameplay_active();
        pinball.set_paused(!active);
        if active {
            sound.resume();
        } else {
            sound.pause();
        }
        let confirm = player_input.pressed("confirm");
        let restart = player_input.pressed("restart")
            || active && (reset_tapped || pinball.table().over() && (confirm || tapped))
            || demo.autoplay && game_over_time > 2.;
        let dt = demo.dt();
        if restart {
            pinball.restart();
            presentation.reset();
            sound.reset();
            // Suppress held physical actions as well as the game's buffered edges.
            player_input.set_paused(true);
            player_input.set_paused(false);
            game_over_time = 0.;
        } else {
            if active && confirm {
                pinball.start();
            }
            let events = input::advance(&mut pinball, &mut player_input, dt as f64, pointer);
            presentation.finish_frame(dt, pinball.table(), events);
            play_feedback(&mut sound, events);
        }
        if pinball.table().over() && active {
            game_over_time += dt;
        }
        table_render(&pinball, &presentation, &player_input);
        player_input.draw_controls();
        if demo.finish_frame() {
            println!(
                "salvage-pinball: score={} hits={} launches={} drains={}",
                pinball.table().score,
                pinball.table().hits,
                pinball.table().launches,
                pinball.table().drains
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
    fn narrow_hud_keeps_reset_separate_and_footer_inside_the_canvas() {
        for (w, h) in [(320., 540.), (390., 694.), (1280., 650.), (800., 1000.)] {
            let (scale, origin, reset) = table_layout(w, h);
            let hud_width = (w - 24.0_f32).clamp(1., 516.);
            let hud_x = (w - hud_width) * 0.5;
            let (_, title_width) = fitted_text("SALVAGE", 16., w).unwrap();
            assert!(hud_x + title_width + 12. <= reset.x);
            assert!(reset.w >= 44. && reset.h >= 44.);
            assert!(reset.x >= 0. && reset.x + reset.w <= w);
            assert!(origin.y + 16. * scale + 66. <= h - 12.);
            assert!(origin.x >= 4. && origin.x + 10.3 * scale <= w - 4.);
            let panel_width = (w - 24.).clamp(1., 460.) - 24.;
            let (title_size, _) = fitted_text("READY FOR YOUR SHIFT?", 20., panel_width).unwrap();
            assert!(title_size >= 17.);
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
