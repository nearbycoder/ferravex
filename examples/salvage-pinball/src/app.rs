use crate::{BALL_RADIUS, BUMPERS, Controls, FLIPPER_LENGTH, PIVOTS, RAILS, Table};
use ferravex_core::FixedClock;
use ferravex_render::audio::{Cue, GameAudio, Surface};
use ferravex_render::input::{
    PlayerInput, Preset,
    touch::{Control, Output, Region},
};
use ferravex_render::{demo::Demo, prelude::*};

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
    let metrics = measure_text(text, None, size as u16, 1.0);
    draw_text(text, center.x - metrics.width * 0.5, center.y, size, color);
}

fn capsule(a: Vec2, b: Vec2, radius: f32, color: Color) {
    draw_line(a.x, a.y, b.x, b.y, radius * 2.0, color);
    draw_circle(a.x, a.y, radius, color);
    draw_circle(b.x, b.y, radius, color);
}

fn table_render(table: &Table, touch_mode: bool) -> Rect {
    let w = screen_width();
    let h = screen_height();
    let scale = ((h - 138.0) / 16.2).min((w - 28.0) / 10.5).max(1.0);
    let origin = vec2(w * 0.5 - 5.15 * scale, 84.0);
    let p = |xy: [f32; 2]| origin + Vec2::from_array(xy) * scale;
    clear_background(INK);
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
        let flash = table.bumper_flash[i] / 0.18;
        draw_poly(v.x + 3.0, v.y + 4.0, 10, 0.86 * scale, 18.0, INK);
        draw_poly(v.x, v.y, 10, 0.78 * scale, 18.0, BRASS);
        draw_circle(
            v.x,
            v.y,
            0.62 * scale,
            if flash > 0.0 { CREAM } else { RUST },
        );
        draw_circle_lines(v.x, v.y, 0.49 * scale, 1.5, BRASS);
        centered(
            "100",
            v + vec2(0.0, 0.11 * scale),
            0.31 * scale,
            if flash > 0.0 { INK } else { CREAM },
        );
        if flash > 0.0 {
            draw_circle_lines(
                v.x,
                v.y,
                (1.05 - flash * 0.25) * scale,
                2.0,
                Color::new(0.95, 0.73, 0.38, flash),
            );
        }
    }
    for (i, pivot) in PIVOTS.into_iter().enumerate() {
        let end = Vec2::from_array(pivot) + Vec2::from_angle(table.angles[i]) * FLIPPER_LENGTH;
        let a = p(pivot);
        let b = p(end.to_array());
        capsule(a + vec2(2.0, 4.0), b + vec2(2.0, 4.0), 0.2 * scale, INK);
        capsule(a, b, 0.16 * scale, CREAM);
        capsule(a, b, 0.07 * scale, RUST);
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
        for (i, point) in table.trail.iter().enumerate().rev() {
            let v = p(*point);
            draw_circle(
                v.x,
                v.y,
                ((12 - i) as f32 / 12.0) * BALL_RADIUS * scale,
                Color::new(0.72, 0.81, 0.78, 0.12),
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
    // Score and remaining balls are the only persistent HUD data.
    draw_text("SALVAGE PINBALL", board.x, 31.0, 16.0, BRASS);
    draw_text(&format!("{:06}", table.score), board.x, 65.0, 32.0, CREAM);
    for i in 0..3 {
        draw_circle(
            board.x + board.w - 49.0 + i as f32 * 16.0,
            57.0,
            4.0,
            if i < table.lives { CREAM } else { PANEL },
        );
    }
    let reset = Rect::new(board.x + board.w - 49.0, 18.0, 49.0, 20.0);
    draw_text("RESET", reset.x, 32.0, 13.0, MINT);
    if table.waiting && !table.over() {
        let v = p([4.7, 11.85]);
        centered(
            if touch_mode {
                "TAP TO LAUNCH"
            } else {
                "SPACE TO LAUNCH"
            },
            v,
            0.32 * scale,
            CREAM,
        );
    }
    if table.over() {
        let center = p([5.15, 7.7]);
        draw_rectangle(
            board.x + 10.0,
            center.y - 57.0,
            board.w - 20.0,
            116.0,
            Color::new(0.07, 0.1, 0.1, 0.96),
        );
        centered("SHIFT COMPLETE", center - vec2(0.0, 17.0), 25.0, CREAM);
        centered(
            &format!("{} POINTS RECOVERED", table.score),
            center + vec2(0.0, 13.0),
            17.0,
            BRASS,
        );
        centered(
            "R / TAP TO PLAY AGAIN",
            center + vec2(0.0, 39.0),
            13.0,
            MINT,
        );
    }
    centered(
        if touch_mode {
            "HOLD LEFT OR RIGHT TO FLIP"
        } else {
            "A / LEFT        SPACE LAUNCH        D / RIGHT"
        },
        vec2(w * 0.5, board.y + board.h + 28.0),
        13.0,
        MINT,
    );
    reset
}

ferravex_render::game!(window_conf, game);
async fn game() {
    let mut demo = Demo::new();
    let mut sound = GameAudio::new();
    let mut flippers = [false; 2];
    let mut table = Table::new();
    let mut clock = FixedClock::new(120.0, 16);
    let mut player_input = PlayerInput::preset("salvage-pinball", Preset::Pinball);
    let mut game_over_time = 0.0;
    let mut launch_pending = false;
    loop {
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
        let touch_mode = player_input.touch_seen();
        let pointer = Vec2::from(mouse_position());
        let tapped = is_mouse_button_pressed(MouseButton::Left)
            || ferravex_render::input::logical_touches()
                .iter()
                .any(|t| t.phase == TouchPhase::Started);
        launch_pending |= tapped && player_input.gameplay_active();
        if !player_input.gameplay_active() {
            launch_pending = false;
        }
        let pointer_down =
            is_mouse_button_down(MouseButton::Left) && player_input.gameplay_active();
        let dt = if demo.autoplay {
            demo.dt()
        } else {
            player_input.simulation_dt(demo.dt())
        };
        let frame = clock.advance(dt as f64);
        for _ in 0..frame.steps {
            let left = player_input.take("flipper_left");
            let right = player_input.take("flipper_right");
            let controls = if demo.autoplay {
                table.autoplay()
            } else {
                Controls {
                    left: left.pressed
                        || left.value > 0.5
                        || pointer_down && pointer.x < split && pointer.y > lower,
                    right: right.pressed
                        || right.value > 0.5
                        || pointer_down && pointer.x >= split && pointer.y > lower,
                    launch: player_input.pressed("launch") || launch_pending,
                }
            };
            let hits = table.hits;
            let launches = table.launches;
            let drains = table.drains;
            for (i, down) in [controls.left, controls.right].into_iter().enumerate() {
                if down && !flippers[i] {
                    sound.cue(Cue::Click, 0.3);
                }
                flippers[i] = down;
            }
            table.step(frame.dt, controls);
            if table.hits > hits {
                sound.cue(Cue::Impact(Surface::Metal), 0.65);
            }
            if table.launches > launches {
                sound.cue(Cue::Jump, 0.55);
            }
            if table.drains > drains {
                sound.cue(Cue::Failure, 0.4);
            }
            launch_pending = false;
        }
        if table.over() {
            game_over_time += demo.dt();
        } else {
            game_over_time = 0.0;
        }
        let reset = table_render(&table, touch_mode);
        if player_input.pressed("restart")
            || table.over() && player_input.pressed("confirm")
            || (tapped && (reset.contains(pointer) || table.over()))
            || (demo.autoplay && game_over_time > 2.0)
        {
            table = Table::new();
            clock.reset();
            launch_pending = false;
        }
        player_input.draw_settings();
        if demo.finish_frame() {
            println!(
                "salvage-pinball: score={} hits={} launches={} drains={}",
                table.score, table.hits, table.launches, table.drains
            );
            break;
        }
        next_frame().await;
    }
}
