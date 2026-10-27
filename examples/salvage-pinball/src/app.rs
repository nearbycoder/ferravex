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
mod hud;
mod input;
mod presentation;
mod quips;
use presentation::{GOLD, Presentation, hint};

const INK: Color = Color::from_rgba(19, 27, 27, 255);
const PANEL: Color = Color::from_rgba(34, 49, 46, 255);
const BRASS: Color = Color::from_rgba(190, 157, 102, 255);
const CREAM: Color = Color::from_rgba(235, 220, 185, 255);
const RUST: Color = Color::from_rgba(167, 77, 47, 255);
const MINT: Color = Color::from_rgba(128, 166, 147, 255);
const DECAL: Color = Color::from_rgba(52, 70, 63, 255);
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
const MENU_CORNER: (f32, f32) = (72., 60.);

/// Header geometry in logical pixels. Wide canvases use one row with Reset at
/// the right; when that row would reach the menu corner, the score row stops
/// short of it and Reset, best and multiplier move to a second row below it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct HudLayout {
    /// Height reserved above the table.
    header: f32,
    left: f32,
    /// Right edge of the ball lamps and their label.
    balls_right: f32,
    /// Width available to the score line.
    score_room: f32,
    /// Width available to the best line before the multiplier label.
    best_room: f32,
    best_y: f32,
    mult_right: f32,
    mult_y: f32,
    reset: Rect,
}

fn hud_layout(w: f32) -> HudLayout {
    let hud_width = (w - 24.).clamp(1., HUD_MAX);
    let left = (w - hud_width) * 0.5;
    let right = (w + hud_width) * 0.5;
    if right <= w - MENU_CORNER.0 {
        let reset = Rect::new(right - 68., 8., 68., 44.);
        let balls_right = reset.x - 14.;
        HudLayout {
            header: HEADER,
            left,
            balls_right,
            score_room: (balls_right - 70. - left).max(40.),
            best_room: (balls_right - 70. - left).max(40.),
            best_y: 74.,
            mult_right: balls_right,
            mult_y: 74.,
            reset,
        }
    } else {
        let balls_right = w - MENU_CORNER.0 - 4.;
        let reset = Rect::new(right - 68., MENU_CORNER.1 + 4., 68., 44.);
        HudLayout {
            header: MENU_CORNER.1 + 52.,
            left,
            balls_right,
            score_room: (balls_right - 70. - left).max(40.),
            best_room: (reset.x - 12. - 76. - left).max(40.),
            best_y: reset.y + 28.,
            mult_right: reset.x - 12.,
            mult_y: reset.y + 28.,
            reset,
        }
    }
}

/// Table scale/origin plus the Reset target. The footer below the table keeps
/// room for the dot-matrix display and one hint line at every window size.
fn table_layout(w: f32, h: f32) -> (f32, Vec2, Rect) {
    let hud = hud_layout(w);
    let scale = ((h - hud.header - 106.0) / 16.0)
        .min((w - 28.0) / 10.5)
        .max(1.0);
    let origin = vec2(w * 0.5 - 5.15 * scale, hud.header + 4.);
    (scale, origin, hud.reset)
}

/// The dot-matrix strip under the table, at least as wide as a card's text.
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

fn table_render(game: &PinballGame, view: &Presentation, input: &PlayerInput, frame: &Frame) {
    let table = game.table();
    let state = game.session().state();
    let mut buffers = [[0; 128]; 6];
    let [
        launch_buf,
        pause_buf,
        restart_buf,
        confirm_buf,
        left_buf,
        right_buf,
    ] = &mut buffers;
    let hints = hud::Hints {
        launch: hint(input, "launch", launch_buf),
        pause: hint(input, "pause", pause_buf),
        restart: hint(input, "restart", restart_buf),
        confirm: hint(input, "confirm", confirm_buf),
        left: hint(input, "flipper_left", left_buf),
        right: hint(input, "flipper_right", right_buf),
        touch: input.current_device() == HintDevice::Touch,
    };
    let w = screen_width();
    let h = screen_height();
    let (scale, base, _) = table_layout(w, h);
    let [sx, sy] = view.shake_offset();
    let origin = base + vec2(sx, sy);
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
    // Painted floor decals sit under the ball's main traffic.
    centered("SALVAGE", p([4.65, 8.75]), 0.62 * scale, DECAL);
    centered("YARD  /  03", p([4.65, 9.25]), 0.24 * scale, DECAL);
    multiplier_inserts(table, view, &p, scale);
    save_insert(table, view, &p, scale);
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
    crane(table, view, &p, scale);
    bumpers(table, view, &p, scale, state);
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
    // The plunger spring compresses while a ball waits and kicks on launch.
    let spring = p([9.25, 15.15]);
    let stretch = if table.waiting {
        0.75
    } else {
        1. + view.launch_seconds * 0.6 * view.motion_scale.min(1.)
    };
    for i in 0..5 {
        let y = spring.y + i as f32 * 0.1 * scale * stretch;
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
        for (i, point) in table.trail[..view.trail_len].iter().enumerate().rev() {
            let v = p(*point);
            draw_circle(
                v.x,
                v.y,
                ((12 - i) as f32 / 12.0) * BALL_RADIUS * scale,
                Color::new(0.72, 0.81, 0.78, 0.12 * view.motion_scale),
            );
        }
        let v = p(table.position());
        if table.crane_hold > 0. {
            draw_circle(v.x, v.y, BALL_RADIUS * scale * 1.9, with_alpha(GOLD, 0.25));
        }
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
    for spark in view.sparks.iter().filter(|s| s.alive()) {
        let v = p(spark.at);
        let fade = 1. - spark.age / spark.life;
        let size = (0.09 * scale * fade).max(1.5);
        draw_rectangle(
            v.x - size * 0.5,
            v.y - size * 0.5,
            size,
            size,
            with_alpha(spark.color, fade),
        );
    }
    let flash = view.board_flash();
    if flash > 0. {
        draw_rectangle(
            board.x,
            board.y,
            board.w,
            board.h,
            with_alpha(view.board_flash_color, flash * 0.35),
        );
    }
    for pop in view.pops.iter().flatten() {
        let at = p([pop.at[0], pop.at[1] - pop.rise(view.motion_scale)]);
        let text = format!("+{}", quips::grouped(pop.points));
        let size = if pop.big { 0.62 } else { 0.36 } * scale;
        let size = size.max(13.);
        let color = if pop.big { GOLD } else { CREAM };
        centered(
            &text,
            at + vec2(1.5, 2.),
            size,
            with_alpha(INK, pop.alpha()),
        );
        centered(&text, at, size, with_alpha(color, pop.alpha()));
    }
    ferravex_render::set_world_colors(false);

    hud::header(table, view, frame.best, hud_layout(w));
    let dmd = dmd_rect(w, scale, base);
    hud::dmd(dmd, table, view, state);
    if state == SessionState::Playing {
        hud::footer_hint(dmd.y + dmd.h + 22., table, view, &hints);
    }
    let center = vec2(w * 0.5, base.y + 7.4 * scale);
    match state {
        SessionState::Ready => hud::ready(center, frame.best, &hints, frame.seed),
        SessionState::Paused => hud::paused(center, input, &hints, frame.seed),
        SessionState::Lost => hud::results(center, table, frame.results, &hints),
        _ => {}
    }
}

/// Per-frame values the renderer needs from the loop.
struct Frame {
    best: Option<u32>,
    results: hud::Results,
    seed: u32,
}

fn bumpers(
    table: &Table,
    view: &Presentation,
    p: &impl Fn([f32; 2]) -> Vec2,
    scale: f32,
    state: SessionState,
) {
    let value = table.bumper_value().to_string();
    // The skill-shot lamp hops while a ball waits and holds during its flight.
    let skill = (table.waiting && state == SessionState::Playing || table.launch_flight)
        .then_some(table.skill_target);
    for (i, point) in BUMPERS.into_iter().enumerate() {
        let v = p(point);
        let flash = view.flash(i);
        let ring = view.ring(i);
        let punch = view.punch(i);
        let lit = table.lamps[i] || table.crane_armed;
        draw_poly(v.x + 3.0, v.y + 4.0, 10, 0.86 * scale, 18.0, INK);
        let rim = if lit && (!table.crane_armed || view.blink(2.)) {
            GOLD
        } else {
            BRASS
        };
        draw_poly(v.x, v.y, 10, 0.78 * scale * punch, 18.0, rim);
        draw_circle(v.x, v.y, 0.62 * scale * punch, mix(RUST, CREAM, flash));
        draw_circle_lines(v.x, v.y, 0.49 * scale * punch, 1.5, BRASS);
        centered(
            &value,
            v + vec2(0.0, 0.11 * scale),
            0.31 * scale,
            if flash > 0.5 { INK } else { CREAM },
        );
        // A lamp insert under each bumper shows progress toward the crane.
        let lamp = p([point[0], point[1] + 0.98]);
        draw_circle(lamp.x, lamp.y, 0.13 * scale, INK);
        draw_circle(lamp.x, lamp.y, 0.09 * scale, if lit { GOLD } else { DECAL });
        if skill == Some(i) {
            let color = if view.blink(3.) { CREAM } else { BRASS };
            draw_circle_lines(v.x, v.y, 0.98 * scale, 3., color);
            centered(
                "SKILL",
                v - vec2(0., 0.98 * scale + 5.),
                0.22 * scale,
                color,
            );
        }
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
}

/// The overhead gantry and its electromagnet. The magnet lowers when armed
/// and the catch target lights; a caught ball hangs under it.
fn crane(table: &Table, view: &Presentation, p: &impl Fn([f32; 2]) -> Vec2, scale: f32) {
    let target = p(CRANE);
    let armed = table.crane_armed;
    if armed {
        let color = if view.blink(1.5) { GOLD } else { BRASS };
        draw_circle(
            target.x,
            target.y,
            CRANE_RADIUS * scale,
            with_alpha(GOLD, 0.12),
        );
        draw_circle_lines(target.x, target.y, CRANE_RADIUS * scale, 2., color);
        centered(
            "JACKPOT",
            p([CRANE[0], CRANE[1] + 0.86]),
            0.24 * scale,
            with_alpha(color, 1. - view.big_pop()),
        );
    } else {
        draw_circle_lines(target.x, target.y, CRANE_RADIUS * scale, 1., DECAL);
        // The jackpot pop rises through this label; fade it out meanwhile.
        let label = with_alpha(DECAL, 1. - view.big_pop());
        centered("CRANE", p([CRANE[0], CRANE[1] + 0.86]), 0.22 * scale, label);
    }
    let left = p([3.35, 0.98]);
    let right = p([5.45, 0.98]);
    draw_line(
        left.x,
        left.y + 3.,
        right.x,
        right.y + 3.,
        0.16 * scale,
        INK,
    );
    draw_line(left.x, left.y, right.x, right.y, 0.14 * scale, BRASS);
    for x in [3.5, 4.0, 4.8, 5.3] {
        let r = p([x, 0.98]);
        draw_circle(r.x, r.y, 0.03 * scale, INK);
    }
    let magnet_y = 1.3 + (CRANE[1] - BALL_RADIUS - 0.18 - 1.3) * view.crane_drop;
    let trolley = p([CRANE[0], 0.98]);
    let magnet = p([CRANE[0], magnet_y]);
    for dx in [-0.25, 0.25] {
        let top = trolley + vec2(dx * scale, 0.);
        draw_line(
            top.x,
            top.y,
            magnet.x + dx * 0.8 * scale,
            magnet.y,
            2.,
            CREAM,
        );
    }
    draw_rectangle(
        trolley.x - 0.2 * scale,
        trolley.y - 0.12 * scale,
        0.4 * scale,
        0.24 * scale,
        INK,
    );
    let body = Rect::new(
        magnet.x - 0.55 * scale,
        magnet.y - 0.18 * scale,
        1.1 * scale,
        0.36 * scale,
    );
    draw_rectangle(body.x + 2., body.y + 3., body.w, body.h, INK);
    draw_rectangle(
        body.x,
        body.y,
        body.w,
        body.h,
        if armed { RUST } else { PANEL },
    );
    draw_rectangle_lines(body.x, body.y, body.w, body.h, 1.5, BRASS);
    centered(
        "MAG",
        vec2(magnet.x, magnet.y + 0.08 * scale),
        0.2 * scale,
        if armed { CREAM } else { BRASS },
    );
    if armed || table.crane_hold > 0. {
        let glow = if view.blink(4.) { 0.9 } else { 0.5 };
        draw_rectangle(
            body.x + 3.,
            body.y + body.h - 3.,
            body.w - 6.,
            2.,
            with_alpha(GOLD, glow),
        );
    }
}

/// Multiplier lamps between the inlanes: lit up to the current multiplier.
fn multiplier_inserts(
    table: &Table,
    view: &Presentation,
    p: &impl Fn([f32; 2]) -> Vec2,
    scale: f32,
) {
    for (n, level) in (2..=MAX_MULTIPLIER).enumerate() {
        let c = p([3.6 + n as f32 * 0.72, 11.8]);
        let lit = table.multiplier >= level;
        let newest = table.multiplier == level && !view.blink(2.);
        let fill = if lit && !newest {
            GOLD
        } else if lit {
            BRASS
        } else {
            DECAL
        };
        draw_circle(c.x + 1.5, c.y + 2., 0.3 * scale, INK);
        draw_circle(c.x, c.y, 0.3 * scale, BRASS);
        draw_circle(c.x, c.y, 0.25 * scale, fill);
        centered(
            &format!("{level}X"),
            c + vec2(0., 0.1 * scale),
            0.26 * scale,
            if lit { INK } else { MINT },
        );
    }
}

/// The ball-save lamp above the drain; it blinks as the save runs out.
fn save_insert(table: &Table, view: &Presentation, p: &impl Fn([f32; 2]) -> Vec2, scale: f32) {
    let c = p([4.95, 14.85]);
    let active = table.ball_save > 0.;
    let on = active && (table.ball_save > 1.5 || view.blink(4.));
    draw_rectangle(
        c.x - 0.5 * scale,
        c.y - 0.17 * scale,
        scale,
        0.34 * scale,
        INK,
    );
    draw_rectangle(
        c.x - 0.45 * scale,
        c.y - 0.13 * scale,
        0.9 * scale,
        0.26 * scale,
        if on { MINT } else { DECAL },
    );
    centered(
        "SAVE",
        c + vec2(0., 0.08 * scale),
        0.2 * scale,
        if on { INK } else { PANEL },
    );
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
        let (_, _, reset) = table_layout(screen_width(), screen_height());
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
        table_render(&pinball, &presentation, &player_input, &frame);
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
