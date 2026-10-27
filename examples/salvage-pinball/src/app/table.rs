//! The world pass: backdrop, cabinet, playfield art, lamps, crane, bumpers,
//! flippers, ball, effects and the dot-matrix display. Everything here is
//! drawn before the HUD boundary, so emissive accents feed the bloom.
//! Physics geometry comes from the crate constants; this module only reads
//! the table and cosmetic state.
use super::layout::Layout;
use super::presentation::{AMBER, GOLD, Presentation};
use super::*;
use ferravex_render::post;
use ferravex_render::shapes::{self, Fill, Shadow, Stroke};
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled};

const HOT: Color = Color::new(1.0, 0.93, 0.7, 1.0);
const FIELD_TOP: Color = Color::from_rgba(47, 66, 61, 255);
const FIELD_BOTTOM: Color = Color::from_rgba(20, 30, 28, 255);
const SKY_TOP: Color = Color::from_rgba(24, 33, 33, 255);
const SKY_BOTTOM: Color = Color::from_rgba(9, 13, 14, 255);
/// Perimeter chaser lamps along the playfield walls, in table units.
pub(super) const RIM_LAMPS: [[f32; 2]; 13] = [
    [0.95, 10.4],
    [0.95, 9.4],
    [0.95, 8.4],
    [0.95, 7.4],
    [0.95, 6.4],
    [0.95, 5.4],
    [0.95, 4.4],
    [0.95, 3.4],
    [8.45, 4.4],
    [8.45, 5.4],
    [8.45, 6.4],
    [8.45, 7.4],
    [8.45, 8.4],
];

/// Crisp classic sizes: multiples of 8, at least 8.
fn crisp(size: f32) -> f32 {
    ((size / 8.).round() * 8.).max(8.)
}

fn label(text: &str, at: Vec2, size: f32, color: Color) {
    let style = TextStyle::classic(crisp(size), color).with_align(TextAlign::Center);
    draw_text_styled(text, at.x, at.y, style);
}

pub(super) struct World<'a> {
    pub game: &'a PinballGame,
    pub view: &'a Presentation,
    pub layout: &'a Layout,
    /// Ready screen: lamps run the attract loop.
    pub attract: bool,
    pub state: SessionState,
}

pub(super) fn draw(world: &World, bufs: &mut hud::Buffers) {
    let World {
        game,
        view,
        layout,
        attract,
        state,
    } = *world;
    let table = game.table();
    let scale = layout.scale;
    let [sx, sy] = view.shake_offset();
    let origin = layout.origin + vec2(sx, sy);
    let p = |xy: [f32; 2]| origin + Vec2::from_array(xy) * scale;
    let board = Rect::new(origin.x, origin.y, layout.board.w, layout.board.h);
    ferravex_render::set_world_colors(true);
    backdrop(layout, view);
    cabinet(board, scale, &p, table, view);
    rim_lamps(view, attract, &p, scale);
    multiplier_inserts(table, view, attract, &p, scale);
    save_insert(table, view, &p, scale);
    for (a, b) in RAILS {
        let (a, b) = (p(a), p(b));
        shapes::fill_capsule(
            a + vec2(2., 3.5),
            b + vec2(2., 3.5),
            0.13 * scale,
            Color::new(0., 0., 0., 0.45),
        );
        shapes::fill_capsule(a, b, 0.1 * scale, BRASS);
        shapes::fill_capsule(
            a - vec2(0., 0.03 * scale),
            b - vec2(0., 0.03 * scale),
            0.035 * scale,
            with_alpha(CREAM, 0.8),
        );
    }
    crane(table, view, &p, scale);
    bumpers(table, view, &p, scale, state, bufs);
    flippers(table, &p, scale);
    plunger(table, view, &p, scale);
    ball(table, view, &p, scale);
    effects(view, &p, scale, board, bufs);
    let dmd = super::dmd_lines(table, view, state, bufs);
    let pitch = layout::dot_pitch(screen_dpi_scale(), layout.wide && layout.dmd.w >= 340.);
    dmd::draw(layout.dmd, pitch, dmd.0, dmd.1, dmd.2, view.motion_scale);
    ferravex_render::set_world_colors(false);
}

fn backdrop(layout: &Layout, view: &Presentation) {
    let (w, h) = (screen_width(), screen_height());
    let screen = Rect::new(0., 0., w, h);
    clear_background(SKY_BOTTOM);
    shapes::fill_rounded_rect(screen, 0., Fill::vertical(screen, SKY_TOP, SKY_BOTTOM));
    // Faint diagonal girders read as a scrapyard at night without busy detail.
    for i in 0..10 {
        let x = i as f32 * w / 9. - 60.;
        draw_line(
            x,
            h,
            x + h * 0.55,
            0.,
            10.,
            Color::from_rgba(28, 37, 36, 120),
        );
    }
    let c = layout.board.center();
    post::glow_circle(c.x, c.y, layout.board.h * 0.8, AMBER, 0.09);
    // Slow dust motes catching the light; static under reduced motion.
    let motion = view.motion_scale.min(1.);
    for i in 0..26_u32 {
        let seed = i.wrapping_mul(2_654_435_761);
        let fx = (seed >> 8) as f32 / (1 << 24) as f32;
        let fy = (seed.rotate_left(13) >> 8) as f32 / (1 << 24) as f32;
        let speed = 6. + 10. * fx;
        let y = (fy * (h + 40.) - view.time * speed * motion).rem_euclid(h + 40.) - 20.;
        let x = fx * w + (view.time * 0.4 + i as f32).sin() * 12. * motion;
        let a = 0.05 + 0.08 * (view.time * 0.7 + i as f32 * 1.3).sin().abs();
        shapes::glow_disc(vec2(x, y), 3. + 4. * fy, Color::new(1., 0.8, 0.55, a));
    }
}

fn cabinet(
    board: Rect,
    scale: f32,
    p: &impl Fn([f32; 2]) -> Vec2,
    table: &Table,
    view: &Presentation,
) {
    let frame = Rect::new(board.x - 9., board.y - 9., board.w + 18., board.h + 18.);
    shapes::drop_shadow(
        frame,
        16.,
        Shadow::soft(Color::new(0., 0., 0., 0.7))
            .offset(vec2(0., 12.))
            .blur(34.),
    );
    shapes::fill_rounded_rect(
        frame,
        16.,
        Fill::vertical(
            frame,
            Color::from_rgba(222, 189, 128, 255),
            Color::from_rgba(120, 92, 54, 255),
        ),
    );
    shapes::stroke_rounded_rect(frame, 16., 1., Color::new(1., 0.95, 0.8, 0.5));
    shapes::fill_rounded_rect(board, 8., Fill::vertical(board, FIELD_TOP, FIELD_BOTTOM));
    for y in 0..22 {
        let a = p([0.3, 0.4 + y as f32 * 0.7]);
        draw_line(
            a.x,
            a.y,
            a.x + 9.7 * scale,
            a.y,
            1.,
            Color::from_rgba(255, 255, 255, 8),
        );
    }
    for xy in [[0.25, 0.25], [10.05, 0.25], [0.25, 15.75], [10.05, 15.75]] {
        let v = p(xy);
        shapes::fill_circle(v, 3., Color::from_rgba(12, 18, 18, 255));
        shapes::fill_circle(v - vec2(0.8, 0.8), 1.2, with_alpha(CREAM, 0.4));
    }
    let lane = Rect::new(p([8.8, 3.8]).x, p([8.8, 3.8]).y, 0.9 * scale, 11.3 * scale);
    shapes::fill_rounded_rect(
        lane,
        4.,
        Fill::vertical(
            lane,
            Color::from_rgba(22, 33, 33, 255),
            Color::from_rgba(12, 18, 18, 255),
        ),
    );
    // Light pools under the bumpers, warmer when a lamp is lit.
    for (i, point) in BUMPERS.into_iter().enumerate() {
        let lit = if table.lamps[i] || table.crane_armed {
            1.
        } else {
            0.35
        };
        let glow = lit + view.flash(i) * 0.8;
        let v = p(point);
        shapes::fill_circle(
            v,
            1.9 * scale,
            Fill::radial(
                v,
                1.9 * scale,
                Color::new(1., 0.65, 0.3, 0.16 * glow),
                Color::new(1., 0.65, 0.3, 0.),
            ),
        );
    }
    // Reclaimed painted-metal slingshots with brass hazard stripes.
    for (a, b, c) in [
        ([0.8, 8.0], [2.7, 10.9], [0.8, 11.2]),
        ([8.6, 8.0], [6.6, 10.9], [8.6, 11.2]),
    ] {
        draw_triangle(p(a), p(b), p(c), RUST);
    }
    for i in 0..4 {
        let y = 8.8 + i as f32 * 0.55;
        for (a, b) in [([1.0, y], [1.55, y + 0.35]), ([8.35, y], [7.8, y + 0.35])] {
            shapes::polyline(
                &[p(a), p(b)],
                Stroke::new(2., with_alpha(BRASS, 0.85)).cap(shapes::Cap::Round),
            );
        }
    }
    let decal = with_alpha(CREAM, 0.07);
    label("SALVAGE", p([4.65, 8.75]), 0.62 * scale, decal);
    label("YARD 03", p([4.65, 9.3]), 0.24 * scale, decal);
}

/// One lamp: a dark socket, a colored lens and an additive halo when lit.
fn lamp(at: Vec2, radius: f32, color: Color, level: f32) {
    let level = level.clamp(0., 1.);
    shapes::fill_circle(at, radius * 1.3, Color::from_rgba(10, 14, 14, 255));
    shapes::fill_circle(
        at,
        radius,
        Fill::radial(
            at - vec2(radius * 0.3, radius * 0.3),
            radius * 1.2,
            mix(Color::from_rgba(60, 52, 40, 255), HOT, level),
            mix(Color::from_rgba(34, 30, 26, 255), color, level),
        ),
    );
    if level > 0.05 {
        post::glow_circle(at.x, at.y, radius * 4., color, 0.55 * level);
    }
}

fn rim_lamps(view: &Presentation, attract: bool, p: &impl Fn([f32; 2]) -> Vec2, scale: f32) {
    for (i, xy) in RIM_LAMPS.into_iter().enumerate() {
        let breathe: f32 = 0.12 + 0.06 * (view.time * 1.6 + i as f32 * 0.5).sin();
        let level = breathe.max(view.chase(i, attract));
        lamp(p(xy), 0.09 * scale, AMBER, level);
    }
}

fn bumpers(
    table: &Table,
    view: &Presentation,
    p: &impl Fn([f32; 2]) -> Vec2,
    scale: f32,
    state: SessionState,
    bufs: &mut hud::Buffers,
) {
    use std::fmt::Write;
    bufs.pop.clear();
    let _ = write!(bufs.pop, "{}", table.bumper_value());
    // The skill-shot lamp hops while a ball waits and holds during its flight.
    let skill = (table.waiting && state == SessionState::Playing || table.launch_flight)
        .then_some(table.skill_target);
    for (i, point) in BUMPERS.into_iter().enumerate() {
        let v = p(point);
        let flash = view.flash(i);
        let ring = view.ring(i);
        let punch = view.punch(i);
        let lit = table.lamps[i] || table.crane_armed;
        let pulse = if table.crane_armed && !view.blink(2.) {
            0.6
        } else {
            1.
        };
        let emissive = if lit { 0.55 * pulse } else { 0.12 } + flash * 1.6;
        post::glow_circle(v.x, v.y, 1.45 * scale * punch, GOLD, emissive);
        shapes::fill_circle(v + vec2(3., 5.), 0.84 * scale, Color::new(0., 0., 0., 0.5));
        let rim = if lit { GOLD } else { BRASS };
        shapes::fill_circle(
            v,
            0.8 * scale * punch,
            Fill::radial(
                v - vec2(0.25, 0.3) * scale,
                1.1 * scale,
                mix(rim, HOT, 0.45),
                mix(rim, Color::from_rgba(80, 56, 30, 255), 0.4),
            ),
        );
        shapes::fill_circle(
            v,
            0.62 * scale * punch,
            Fill::radial(
                v - vec2(0.2, 0.25) * scale,
                0.85 * scale,
                mix(Color::from_rgba(214, 112, 70, 255), HOT, flash),
                mix(Color::from_rgba(120, 48, 28, 255), CREAM, flash),
            ),
        );
        shapes::stroke_circle(v, 0.49 * scale * punch, 1.5, with_alpha(BRASS, 0.9));
        let ink = if flash > 0.5 { INK } else { CREAM };
        let style = TextStyle::classic(crisp(0.3 * scale), ink)
            .with_align(TextAlign::Center)
            .with_shadow(Color::new(0., 0., 0., 0.45));
        draw_text_styled(&bufs.pop, v.x, style.centered_baseline(v.y), style);
        // A lamp insert under each bumper shows progress toward the crane.
        lamp(
            p([point[0], point[1] + 0.98]),
            0.1 * scale,
            GOLD,
            if lit { pulse } else { 0. },
        );
        if skill == Some(i) {
            let on = view.blink(3.);
            let color = if on { HOT } else { BRASS };
            shapes::stroke_circle(v, 0.98 * scale, 2.5, color);
            if on {
                shapes::glow_ring(v, 0.98 * scale, 6., with_alpha(GOLD, 0.5));
            }
            label("SKILL", v - vec2(0., 1.1 * scale), 0.22 * scale, color);
        }
        if ring > 0.0 {
            shapes::stroke_circle(
                v,
                (1.05 + (1. - ring) * 0.5) * scale,
                2.,
                Color::new(1., 0.8, 0.45, ring),
            );
        }
    }
}

/// The overhead gantry and its electromagnet. The magnet lowers when armed
/// and the catch target lights; a caught ball hangs under it.
fn crane(table: &Table, view: &Presentation, p: &impl Fn([f32; 2]) -> Vec2, scale: f32) {
    let target = p(CRANE);
    let armed = table.crane_armed;
    let held = table.crane_hold > 0.;
    let label_alpha = 1. - view.big_pop();
    if armed {
        let pulse = 0.65 + 0.35 * (view.time * 5.).sin() * view.flash_scale().min(1.);
        shapes::fill_circle(
            target,
            CRANE_RADIUS * scale,
            Fill::radial(
                target,
                CRANE_RADIUS * scale,
                with_alpha(GOLD, 0.28 * pulse),
                with_alpha(GOLD, 0.04),
            ),
        );
        shapes::stroke_circle(target, CRANE_RADIUS * scale, 2., with_alpha(GOLD, pulse));
        post::glow_circle(
            target.x,
            target.y,
            CRANE_RADIUS * scale * 2.2,
            GOLD,
            0.35 * pulse,
        );
        // Rotating field arcs sell the magnetism; frozen under reduced motion.
        let spin = view.time * 2.4 * view.motion_scale.min(1.);
        for k in 0..3 {
            let start = spin + k as f32 * std::f32::consts::TAU / 3.;
            shapes::stroke_arc(
                target,
                CRANE_RADIUS * scale * 1.25,
                start,
                0.9,
                Stroke::new(2., with_alpha(HOT, 0.6 * pulse)).cap(shapes::Cap::Round),
            );
        }
        label(
            "JACKPOT",
            p([CRANE[0], CRANE[1] + 0.86]),
            0.24 * scale,
            with_alpha(GOLD, label_alpha),
        );
    } else {
        shapes::stroke_circle(target, CRANE_RADIUS * scale, 1., with_alpha(CREAM, 0.12));
        label(
            "CRANE",
            p([CRANE[0], CRANE[1] + 0.86]),
            0.22 * scale,
            with_alpha(CREAM, 0.16 * label_alpha),
        );
    }
    let left = p([3.35, 0.98]);
    let right = p([5.45, 0.98]);
    let beam = Rect::new(
        left.x,
        left.y - 0.08 * scale,
        right.x - left.x,
        0.16 * scale,
    );
    shapes::fill_rounded_rect(
        Rect::new(beam.x + 2., beam.y + 3., beam.w, beam.h),
        3.,
        Color::new(0., 0., 0., 0.45),
    );
    shapes::fill_rounded_rect(
        beam,
        3.,
        Fill::vertical(beam, mix(BRASS, HOT, 0.3), mix(BRASS, INK, 0.3)),
    );
    for x in [3.5, 4.0, 4.8, 5.3] {
        shapes::fill_circle(p([x, 0.98]), 0.03 * scale, INK);
    }
    let magnet_y = 1.3 + (CRANE[1] - BALL_RADIUS - 0.18 - 1.3) * view.crane_drop;
    let trolley = p([CRANE[0], 0.98]);
    let magnet = p([CRANE[0], magnet_y]);
    for dx in [-0.25, 0.25] {
        shapes::polyline(
            &[
                trolley + vec2(dx * scale, 0.),
                vec2(magnet.x + dx * 0.8 * scale, magnet.y),
            ],
            Stroke::new(1.5, with_alpha(CREAM, 0.85)),
        );
    }
    let body = Rect::new(
        magnet.x - 0.55 * scale,
        magnet.y - 0.18 * scale,
        1.1 * scale,
        0.36 * scale,
    );
    shapes::fill_rounded_rect(
        Rect::new(body.x + 2., body.y + 3., body.w, body.h),
        4.,
        Color::new(0., 0., 0., 0.5),
    );
    let (top, bottom) = if armed || held {
        (
            Color::from_rgba(214, 104, 60, 255),
            Color::from_rgba(120, 44, 24, 255),
        )
    } else {
        (
            Color::from_rgba(70, 86, 80, 255),
            Color::from_rgba(36, 46, 43, 255),
        )
    };
    shapes::fill_rounded_rect(body, 4., Fill::vertical(body, top, bottom));
    shapes::stroke_rounded_rect(body, 4., 1.5, BRASS);
    label(
        "MAG",
        vec2(magnet.x, magnet.y + 0.09 * scale),
        0.2 * scale,
        if armed { CREAM } else { BRASS },
    );
    if armed || held {
        let glow = if view.blink(4.) { 1. } else { 0.55 };
        let face = Rect::new(body.x + 4., body.y + body.h - 3., body.w - 8., 2.5);
        draw_rectangle(face.x, face.y, face.w, face.h, with_alpha(HOT, glow));
        post::glow_circle(magnet.x, body.y + body.h, 0.6 * scale, GOLD, 0.6 * glow);
    }
}

/// Multiplier lamps between the inlanes: lit up to the current multiplier.
fn multiplier_inserts(
    table: &Table,
    view: &Presentation,
    attract: bool,
    p: &impl Fn([f32; 2]) -> Vec2,
    scale: f32,
) {
    for (n, level) in (2..=MAX_MULTIPLIER).enumerate() {
        let c = p([3.6 + n as f32 * 0.72, 11.8]);
        let lit = table.multiplier >= level;
        let newest = table.multiplier == level && !view.blink(2.);
        let chase = view.chase(RIM_LAMPS.len() + n, attract);
        let on = if lit && !newest {
            1.
        } else if lit {
            0.6
        } else {
            chase * 0.8
        };
        lamp(c, 0.27 * scale, GOLD, on);
        let ink = if on > 0.5 {
            INK
        } else {
            with_alpha(CREAM, 0.45)
        };
        let style = TextStyle::classic(crisp(0.24 * scale), ink).with_align(TextAlign::Center);
        let text = ["2X", "3X", "4X", "5X"][n];
        draw_text_styled(text, c.x, style.centered_baseline(c.y), style);
    }
}

/// The ball-save lamp above the drain; it blinks as the save runs out.
fn save_insert(table: &Table, view: &Presentation, p: &impl Fn([f32; 2]) -> Vec2, scale: f32) {
    let c = p([4.95, 14.85]);
    let active = table.ball_save > 0.;
    let on = active && (table.ball_save > 1.5 || view.blink(4.));
    let rect = Rect::new(c.x - 0.5 * scale, c.y - 0.17 * scale, scale, 0.34 * scale);
    shapes::fill_rounded_rect(rect, 0.17 * scale, Color::from_rgba(10, 14, 14, 255));
    let lens = Rect::new(rect.x + 2., rect.y + 2., rect.w - 4., rect.h - 4.);
    let fill = if on {
        Fill::vertical(lens, Color::from_rgba(190, 255, 225, 255), MINT)
    } else {
        Fill::Solid(Color::from_rgba(36, 48, 44, 255))
    };
    shapes::fill_rounded_rect(lens, 0.15 * scale, fill);
    if on {
        post::glow_circle(c.x, c.y, 1.1 * scale, MINT, 0.6);
    }
    label(
        "SAVE",
        c + vec2(0., 0.09 * scale),
        0.2 * scale,
        if on { INK } else { with_alpha(CREAM, 0.3) },
    );
    label(
        "DRAIN",
        p([4.95, 15.6]),
        0.22 * scale,
        with_alpha(BRASS, 0.8),
    );
}

fn flippers(table: &Table, p: &impl Fn([f32; 2]) -> Vec2, scale: f32) {
    for (i, pivot) in PIVOTS.into_iter().enumerate() {
        let end = Vec2::from_array(pivot) + Vec2::from_angle(table.angles[i]) * FLIPPER_LENGTH;
        let a = p(pivot);
        let b = p(end.to_array());
        shapes::fill_capsule(
            a + vec2(2., 5.),
            b + vec2(2., 5.),
            0.2 * scale,
            Color::new(0., 0., 0., 0.5),
        );
        shapes::fill_capsule(
            a,
            b,
            0.17 * scale,
            Fill::vertical(
                Rect::new(
                    a.x.min(b.x),
                    a.y.min(b.y) - 0.17 * scale,
                    (a.x - b.x).abs(),
                    (a.y - b.y).abs() + 0.34 * scale,
                ),
                HOT,
                BRASS,
            ),
        );
        let pressed = table.flipper_pressed[i];
        let inset = if pressed { MINT } else { RUST };
        shapes::fill_capsule(a, b, 0.07 * scale, inset);
        if pressed {
            post::with_glow(|| shapes::fill_capsule(a, b, 0.05 * scale, with_alpha(MINT, 0.7)));
        }
        shapes::fill_circle(a, 0.1 * scale, BRASS);
        shapes::fill_circle(a, 0.04 * scale, INK);
    }
}

fn plunger(table: &Table, view: &Presentation, p: &impl Fn([f32; 2]) -> Vec2, scale: f32) {
    // The spring compresses while a ball waits and kicks on launch.
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
            1.5,
            BRASS,
        );
    }
}

fn ball(table: &Table, view: &Presentation, p: &impl Fn([f32; 2]) -> Vec2, scale: f32) {
    if table.over() {
        return;
    }
    let r = BALL_RADIUS * scale;
    for (i, point) in table.trail[..view.trail_len].iter().enumerate().rev() {
        let fade = (12 - i) as f32 / 12.;
        shapes::glow_disc(
            p(*point),
            r * (0.6 + fade),
            Color::new(0.6, 0.95, 0.85, 0.18 * fade * view.motion_scale.min(1.)),
        );
    }
    let v = p(table.position());
    if table.crane_hold > 0. {
        post::glow_circle(v.x, v.y, r * 4., GOLD, 0.8);
    }
    shapes::fill_circle(v + vec2(2.5, 4.), r, Color::new(0., 0., 0., 0.55));
    // Chrome: a cool rim, a bright upper-left body and a hard specular.
    shapes::fill_circle(
        v,
        r,
        Fill::radial(
            v - vec2(r * 0.35, r * 0.4),
            r * 1.5,
            Color::from_rgba(250, 252, 245, 255),
            Color::from_rgba(70, 104, 98, 255),
        ),
    );
    shapes::stroke_circle(v, r, 1., Color::new(0., 0., 0., 0.35));
    shapes::fill_circle(v - vec2(r * 0.38, r * 0.42), r * 0.28, WHITE);
}

fn effects(
    view: &Presentation,
    p: &impl Fn([f32; 2]) -> Vec2,
    scale: f32,
    board: Rect,
    bufs: &mut hud::Buffers,
) {
    // Light-show rays from the crane during a jackpot or arming.
    if view.show > 0. && view.flash_scale() > 0. && view.motion_scale > 0. {
        let c = p(CRANE);
        let level = (view.show / 2.4).min(1.) * view.flash_scale().min(1.);
        post::with_glow(|| {
            for k in 0..12 {
                let angle = view.time * 1.3 + k as f32 * std::f32::consts::TAU / 12.;
                let end = c + Vec2::from_angle(angle) * board.h * 0.7;
                shapes::polyline(
                    &[c, end],
                    Stroke::new(0.25 * scale, Color::new(1., 0.75, 0.4, 0.07 * level)),
                );
            }
        });
    }
    post::with_glow(|| {
        for spark in view.sparks.iter().filter(|s| s.alive()) {
            let fade = 1. - spark.age / spark.life;
            let size = (0.08 * scale * fade).max(1.5);
            let v = p(spark.at);
            draw_rectangle(
                v.x - size * 0.5,
                v.y - size * 0.5,
                size,
                size,
                with_alpha(spark.color, fade),
            );
        }
    });
    let flash = view.board_flash();
    if flash > 0. {
        let c = p(view.flash_at);
        let r = board.h * 0.75;
        post::with_glow(|| {
            shapes::fill_circle(
                c,
                r,
                Fill::radial(
                    c,
                    r,
                    with_alpha(view.board_flash_color, flash * 0.55),
                    with_alpha(view.board_flash_color, 0.),
                ),
            );
        });
    }
    for pop in view.pops.iter().flatten() {
        let at = p([pop.at[0], pop.at[1] - pop.rise(view.motion_scale)]);
        bufs.pop.clear();
        bufs.pop.push('+');
        quips::grouped_into(&mut bufs.pop, pop.points);
        let size = crisp(if pop.big { 0.62 } else { 0.34 } * scale).max(16.);
        let color = if pop.big { GOLD } else { HOT };
        let alpha = pop.alpha();
        if pop.big {
            post::glow_circle(at.x, at.y - size * 0.4, size * 2.2, GOLD, 0.45 * alpha);
        }
        let style = TextStyle::classic(size, with_alpha(color, alpha))
            .with_align(TextAlign::Center)
            .with_outline(with_alpha(Color::from_rgba(30, 16, 8, 255), alpha));
        draw_text_styled(&bufs.pop, at.x, at.y, style);
    }
}
