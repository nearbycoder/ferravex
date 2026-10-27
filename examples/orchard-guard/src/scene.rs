//! World drawing: the living orchard, towers, pests and accepted-event juice.
//! Drawing reads simulation state and presentation state; it never mutates
//! either, so frame rate and preferences cannot change the match.
use super::*;
use crate::juice::{self, GHOST_LIFE, Juice};
use crate::look::{self, DayLight};
use crate::style::{self, body, punch, sfmt, text_center};
use ferravex_render::post;
use ferravex_render::shapes::{self, Fill, Join, Stroke};
use ferravex_render::text::{TextStyle, text_width as styled_width};

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a.clamp(0., 1.))
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

/// Small filled dot with as few triangles as reads round at its size.
fn dot(x: f32, y: f32, r: f32, c: Color) {
    let sides = if r < 2.5 {
        6
    } else if r < 6. {
        10
    } else {
        16
    };
    draw_poly(x, y, sides, r, 0., c);
}

/// World-space label in the modern face with a soft cream shadow.
fn tag(s: &str, x: f32, baseline: f32, size: f32, color: Color) {
    text_center(
        s,
        x,
        baseline,
        body(size, color).with_shadow(Color::new(0.98, 0.95, 0.84, 0.7)),
    );
}

const TREES: [Vec2; 18] = [
    vec2(65., 365.),
    vec2(100., 455.),
    vec2(180., 495.),
    vec2(80., 565.),
    vec2(180., 595.),
    vec2(335., 590.),
    vec2(540., 570.),
    vec2(620., 570.),
    vec2(870., 590.),
    vec2(1090., 590.),
    vec2(1150., 560.),
    vec2(1140., 180.),
    vec2(1035., 180.),
    vec2(930., 175.),
    vec2(670., 125.),
    vec2(570., 125.),
    vec2(375., 130.),
    vec2(80., 115.),
];

fn tree(p: Vec2, variant: usize, sway: f32) {
    draw_ellipse(
        p.x + 9.,
        p.y + 19.,
        27.,
        9.,
        0.,
        Color::new(0.20, 0.30, 0.16, 0.18),
    );
    draw_rectangle(p.x - 3., p.y, 6., 25., Color::from_hex(0x85704c));
    let top = vec2(p.x + sway, p.y);
    draw_poly(top.x, top.y - 8., 7, 25., 15., Color::from_hex(0x5f8048));
    draw_poly(
        top.x - 7. + sway * 0.4,
        top.y - 14.,
        7,
        20.,
        0.,
        Color::from_hex(0x78975b),
    );
    dot(
        top.x + 6.,
        top.y - 21.,
        7.,
        Color::new(0.70, 0.80, 0.50, 0.45),
    );
    for i in 0..3 {
        let a = (i as f32 * 2.1 + variant as f32) * 1.4;
        let c = vec2(top.x + a.cos() * 14., top.y - 10. + a.sin() * 13.);
        dot(c.x, c.y, 3.8, juice::APPLE);
        dot(c.x - 1., c.y - 1.2, 1.2, Color::new(1., 0.85, 0.75, 0.8));
    }
}

fn heart(x: f32, y: f32, r: f32, color: Color) {
    shapes::fill_circle(vec2(x - r * 0.5, y - r * 0.2), r * 0.55, color);
    shapes::fill_circle(vec2(x + r * 0.5, y - r * 0.2), r * 0.55, color);
    draw_triangle(
        vec2(x - r * 1.02, y - r * 0.05),
        vec2(x + r * 1.02, y - r * 0.05),
        vec2(x, y + r * 0.95),
        color,
    );
}
pub(super) fn heart_icon(x: f32, y: f32, r: f32, color: Color) {
    heart(x, y, r, color);
}

pub(super) fn coin_icon(x: f32, y: f32, r: f32) {
    let c = vec2(x, y);
    shapes::fill_circle(
        c,
        r,
        Fill::radial(
            c - vec2(r * 0.3, r * 0.3),
            r * 1.4,
            Color::from_hex(0xffe08a),
            juice::GOLD,
        ),
    );
    shapes::stroke_circle(c, r * 0.68, 1.4, Color::from_hex(0xb7862c));
    draw_rectangle(
        x - r * 0.12,
        y - r * 0.4,
        r * 0.24,
        r * 0.8,
        Color::from_hex(0xb7862c),
    );
}

fn ground(g: &Game, juice: &Juice, light: &DayLight) {
    let board = Rect::new(0., 70., 1200., 590.);
    shapes::fill_rounded_rect(
        board,
        0.,
        Fill::vertical(board, Color::from_hex(0xb1c283), Color::from_hex(0x9cb072)),
    );
    look::sky(light);
    // Soft meadows break up the field.
    for (x, y, rx, ry) in [
        (170., 300., 160., 90.),
        (640., 470., 230., 70.),
        (900., 260., 160., 70.),
        (420., 620., 260., 40.),
    ] {
        draw_ellipse(x, y, rx, ry, 0., Color::new(0.80, 0.85, 0.57, 0.32));
    }
    for i in 0..110 {
        let x = ((i * 97 + 31) % 1200) as f32;
        let y = 85. + ((i * 67 + 21) % 560) as f32;
        draw_line(
            x,
            y,
            x + 3.,
            y - 5.,
            1.2,
            Color::new(0.33, 0.45, 0.24, 0.22),
        );
    }
    for i in 0..44 {
        let x = ((i * 271 + 57) % 1180) as f32 + 10.;
        let y = 90. + ((i * 151 + 33) % 550) as f32;
        let color = [
            Color::from_hex(0xfdf3d0),
            Color::from_hex(0xf6d36b),
            Color::from_hex(0xe9a3a0),
        ][i % 3];
        dot(x, y, 2.2, color);
    }
    if juice.motion() > 0. {
        for (offset, y, r) in [(0., 260., 120.), (720., 520., 150.)] {
            let x = (juice.clock * 14. + offset) % 1560. - 180.;
            draw_ellipse(x, y, r, r * 0.45, 0., Color::new(0.15, 0.25, 0.12, 0.07));
        }
    }
    // A rounded dirt path with a soft bank and worn centre line.
    let path = &g.level.path;
    let points: &[Vec2] = path;
    let n = points.len().min(64);
    let shadow: [Vec2; 64] =
        std::array::from_fn(|i| points.get(i).copied().unwrap_or(Vec2::ZERO) + vec2(3., 7.));
    shapes::polyline(
        &shadow[..n],
        Stroke::new(52., Color::new(0.30, 0.38, 0.18, 0.20)).join(Join::Round),
    );
    shapes::polyline(
        &points[..n],
        Stroke::new(49., Color::from_hex(0xc4ab74)).join(Join::Round),
    );
    shapes::polyline(
        &points[..n],
        Stroke::new(43., Color::from_hex(0xdcc794)).join(Join::Round),
    );
    shapes::polyline(
        &points[..n],
        Stroke::new(14., Color::new(0.93, 0.86, 0.66, 0.5)).join(Join::Round),
    );
    for pair in g.level.path.windows(2) {
        let len = pair[0].distance(pair[1]);
        let d = (pair[1] - pair[0]) / len;
        for i in 0..(len / 23.) as usize {
            let p = pair[0] + d * (i as f32 * 23.) + vec2(d.y, -d.x) * 11.;
            draw_rectangle(
                p.x - 1.,
                p.y - 1.,
                2.4,
                2.4,
                Color::new(0.55, 0.44, 0.27, 0.30),
            );
        }
    }
    for (i, p) in TREES.into_iter().enumerate() {
        tree(p, i, juice.sway(i));
    }
    look::lamp_posts(light);
}

fn farmhouse(g: &Game, juice: &Juice) {
    if juice.gate > 0. {
        shapes::fill_circle(
            vec2(1150., 370.),
            80.,
            Fill::radial(
                vec2(1150., 370.),
                80.,
                Color::new(0.95, 0.30, 0.22, 0.38 * juice.gate),
                Color::new(0.95, 0.30, 0.22, 0.),
            ),
        );
    }
    draw_ellipse(1158., 430., 48., 8., 0., Color::new(0.2, 0.28, 0.15, 0.22));
    draw_rectangle(1120., 385., 69., 43., Color::from_hex(0xf3e8c2));
    draw_triangle(
        vec2(1111., 386.),
        vec2(1154., 351.),
        vec2(1197., 386.),
        Color::from_hex(0xb4694a),
    );
    draw_rectangle(1147., 404., 15., 24., Color::from_hex(0x7d6848));
    draw_rectangle(1128., 395., 12., 10., Color::from_hex(0xf6d88a));
    for y in [302., 353.] {
        for x in [1135., 1156., 1177.] {
            draw_rectangle(x, y, 5., 23., CREAM);
        }
        draw_rectangle(1135., y + 7., 47., 4., CREAM);
    }
    tag("Orchard", 1150., 452., 14., GREEN);
    // The harvest basket holds one apple per remaining heart.
    let basket = Rect::new(1108., 492., 80., 22.);
    shapes::fill_rounded_rect(
        basket,
        5.,
        Fill::vertical(basket, Color::from_hex(0xab8152), Color::from_hex(0x8a6540)),
    );
    for x in 0..5 {
        draw_line(
            1112. + x as f32 * 17.,
            494.,
            1112. + x as f32 * 17.,
            512.,
            1.5,
            Color::from_hex(0x7a5935),
        );
    }
    let mut n = 0;
    for (row, count) in [(0, 5), (1, 4), (2, 3)] {
        for i in 0..count {
            if n >= usize::from(g.lives) {
                break;
            }
            let x = 1116. + row as f32 * 8. + i as f32 * 16.;
            let y = 488. - row as f32 * 11.;
            dot(x, y, 7., juice::APPLE);
            dot(x - 2., y - 2., 2., Color::new(1., 0.8, 0.7, 0.6));
            n += 1;
        }
    }
}

fn critters(juice: &Juice) {
    if juice.motion() == 0. {
        return;
    }
    let t = juice.clock;
    for (i, (cx, cy, rx, ry, color)) in [
        (420., 600., 60., 26., Color::from_hex(0xfdf3d0)),
        (860., 120., 80., 20., Color::from_hex(0xf6d36b)),
        (90., 250., 40., 50., Color::from_hex(0xe9a3a0)),
    ]
    .into_iter()
    .enumerate()
    {
        let a = t * (0.5 + i as f32 * 0.13) + i as f32 * 2.;
        let p = vec2(cx + a.cos() * rx, cy + (a * 1.7).sin() * ry);
        let flap = (t * 14. + i as f32).sin().abs() * 5. + 1.;
        draw_triangle(p, p + vec2(-6., -flap), p + vec2(-6., flap * 0.4), color);
        draw_triangle(p, p + vec2(6., -flap), p + vec2(6., flap * 0.4), color);
        dot(p.x, p.y, 1.2, INK);
    }
}

/// A tower at `p`; `scale` carries recoil squash and grow-in. Also used for
/// the HUD's tower portraits.
pub(super) fn tower(p: Vec2, t: Tower, scale: Vec2, pips: bool) {
    let s = |v: f32| v * scale.x;
    let h = |v: f32| v * scale.y;
    match t.kind {
        Kind::Seed => {
            draw_rectangle(
                p.x - s(12.),
                p.y - h(8.),
                s(24.),
                h(19.),
                Color::from_hex(0x84623e),
            );
            shapes::fill_rounded_rect(
                Rect::new(p.x - s(17.), p.y - h(16.), s(34.), h(8.)),
                s(3.),
                Color::from_hex(0xc0aa7c),
            );
            let muzzle = p + t.aim * (25. - (1. - scale.y) * 50.).max(14.) * scale.x;
            shapes::fill_capsule(
                vec2(p.x, p.y - h(5.)),
                vec2(muzzle.x, muzzle.y - h(5.)),
                s(5.),
                GREEN,
            );
            let head = vec2(p.x, p.y - h(5.));
            shapes::fill_circle(
                head,
                s(12.),
                Fill::radial(
                    head - vec2(s(4.), s(4.)),
                    s(16.),
                    Color::from_hex(0x6a9550),
                    Color::from_hex(0x41633a),
                ),
            );
            if t.level >= 1 {
                dot(head.x, head.y, s(5.), Color::from_hex(0x86b062));
            }
        }
        Kind::Frost => {
            shapes::fill_rounded_rect(
                Rect::new(p.x - s(14.), p.y - h(12.), s(28.), h(22.)),
                s(4.),
                Color::from_hex(0x6aa3a0),
            );
            for y in [-6., 1.] {
                draw_line(
                    p.x - s(14.),
                    p.y + h(y),
                    p.x + s(14.),
                    p.y + h(y),
                    2.5,
                    Color::from_hex(0xa9d3c2),
                );
            }
            draw_triangle(
                vec2(p.x - s(20.), p.y - h(13.)),
                vec2(p.x, p.y - h(28.)),
                vec2(p.x + s(20.), p.y - h(13.)),
                Color::from_hex(0x447d80),
            );
            dot(p.x, p.y - h(1.), s(4.5), INK);
            dot(p.x, p.y - h(1.), s(2.), Color::from_hex(0xbff3f0));
        }
    }
    if t.level >= 2 {
        // Fully grown towers wear a golden leaf crown.
        for dx in [-8., 0., 8.] {
            draw_triangle(
                vec2(p.x + s(dx) - 4., p.y - h(30.)),
                vec2(p.x + s(dx), p.y - h(39.)),
                vec2(p.x + s(dx) + 4., p.y - h(30.)),
                juice::GOLD,
            );
        }
    }
    if pips {
        for n in 0..3u8 {
            let x = p.x - 9. + f32::from(n) * 9.;
            dot(x, p.y + 20., 3.4, Color::new(0.24, 0.31, 0.18, 0.5));
            if n <= t.level {
                dot(x, p.y + 20., 2.6, juice::GOLD);
            }
        }
    }
}

fn pest(pest: &Pest, g: &Game, presentation: &Presentation, juice: &Juice, boost: f32) {
    let p = pest.pos;
    let royal = pest.id == ROYAL_ID;
    let radius = if royal {
        14.
    } else if pest.fast {
        9.
    } else {
        12.
    };
    draw_ellipse(
        p.x + 3.,
        p.y + 8.,
        radius + 3.,
        6.,
        0.,
        Color::new(0.2, 0.2, 0.15, 0.22),
    );
    let dir =
        (g.level.path[(pest.segment + 1).min(g.level.path.len() - 1)] - p).normalize_or_zero();
    let side = vec2(-dir.y, dir.x);
    let wiggle = presentation.wiggle(pest.id);
    if pest.fast && juice.motion() > 0. {
        for n in [-1., 0., 1.] {
            let a = p - dir * (radius + 6.) + side * n * 5.;
            let b = a - dir * (8. + (wiggle * 2.).abs());
            draw_line(a.x, a.y, b.x, b.y, 1.5, Color::new(0.98, 0.95, 0.84, 0.7));
        }
    }
    for n in [-1., 1.] {
        for t in [-6., 0., 6.] {
            let leg = p + side * n * (radius + 3.) + dir * (t + wiggle * n);
            let root = p + dir * t;
            draw_line(root.x, root.y, leg.x, leg.y, 2., INK);
        }
    }
    let base = if pest.slow().is_some() {
        Color::from_hex(0x75bfca)
    } else if royal {
        Color::from_hex(0x7b4aa0)
    } else if pest.fast {
        juice::ZOOMIE
    } else {
        juice::BEETLE
    };
    let color = mix(base, CREAM, presentation.flash(pest.id));
    // A gentle body bob doubles as the wobble walk.
    let bob = wiggle.abs() * 0.25;
    let body_at = vec2(p.x, p.y - bob);
    shapes::fill_circle(
        body_at,
        radius,
        Fill::radial(
            body_at - side * radius * 0.35 - dir * radius * 0.2,
            radius * 1.4,
            mix(color, CREAM, 0.25),
            color,
        ),
    );
    draw_line(
        p.x - dir.x * radius * 0.8,
        p.y - dir.y * radius * 0.8 - bob,
        p.x + dir.x * radius * 0.6,
        p.y + dir.y * radius * 0.6 - bob,
        1.5,
        INK,
    );
    let spot = mix(base, INK, 0.35);
    for n in [-1., 1.] {
        let s = p - dir * 3. + side * n * radius * 0.45;
        dot(s.x, s.y - bob, radius * 0.18, spot);
    }
    let head = p + dir * (radius - 1.);
    dot(head.x, head.y - bob, radius * 0.5, INK);
    for n in [-1., 1.] {
        let root = head + dir * 3. + side * n * 2.;
        let tip = head + dir * 9. + side * n * (6. + wiggle * 0.5);
        draw_line(root.x, root.y - bob, tip.x, tip.y - bob, 1.2, INK);
        // Googly eyes: pupils jiggle with the walk.
        let eye = head + dir * 2. + side * n * radius * 0.28;
        dot(eye.x, eye.y - bob, radius * 0.24, CREAM);
        let pupil = eye + dir * radius * 0.08 + side * wiggle * 0.25;
        dot(pupil.x, pupil.y - bob, radius * 0.12, INK);
    }
    if royal {
        let c = vec2(p.x, p.y - radius - 4. - bob);
        draw_rectangle(c.x - 9., c.y - 2., 18., 5., juice::GOLD);
        for dx in [-9., -1., 7.] {
            draw_triangle(
                vec2(c.x + dx, c.y - 1.),
                vec2(c.x + dx + 1., c.y - 10.),
                vec2(c.x + dx + 3., c.y - 1.),
                juice::GOLD,
            );
        }
        post::glow_circle(c.x, c.y - 3., 14., Color::new(1., 0.8, 0.3, 1.), 0.6);
        let style = body(12. * boost, Color::from_hex(0x5d3a78));
        let w = styled_width("Sir Crunchalot", style);
        let plate = Rect::new(
            p.x - w * 0.5 - 5.,
            p.y - radius - 32. * boost.sqrt(),
            w + 10.,
            14. * boost + 4.,
        );
        shapes::fill_rounded_rect(plate, plate.h * 0.5, Color::new(0.99, 0.96, 0.87, 0.92));
        text_center(
            "Sir Crunchalot",
            p.x,
            style.centered_baseline(plate.y + plate.h * 0.5),
            style,
        );
    }
    if let Some(slow) = pest.slow() {
        // Shape and countdown remain legible with flashes/motion disabled.
        shapes::stroke_circle(p, radius + 6., 1.5, Color::from_hex(0x2f6f78));
        for k in 0..4 {
            let a = k as f32 * std::f32::consts::FRAC_PI_2 + 0.4;
            let q = p + vec2(a.cos(), a.sin()) * (radius + 6.);
            dot(q.x, q.y, 2., juice::ICE);
        }
        tag(
            sfmt!("Slow {:.1}s", slow.remaining_seconds).as_str(),
            p.x,
            p.y + radius + 22.,
            (11. * boost).round(),
            INK,
        );
    }
    if pest.hp < pest.max_hp {
        let w = radius * 2. + 2.;
        let track = Rect::new(p.x - w * 0.5 - 1., p.y - radius - 9., w + 2., 5.);
        shapes::fill_rounded_rect(track, 2.5, Color::new(0.2, 0.25, 0.18, 0.55));
        let frac = (pest.hp / pest.max_hp).max(0.);
        draw_rectangle(
            p.x - w * 0.5,
            p.y - radius - 8.,
            w * frac,
            3.,
            mix(juice::APPLE, juice::LEAF, frac),
        );
    }
}

fn ghost(ghost: &juice::Ghost, juice: &Juice) {
    let t = ghost.age / GHOST_LIFE;
    let a = (1. - t).powf(0.7);
    let p = ghost.at - vec2(0., t * 12. * juice.motion());
    let r = if ghost.fast { 9. } else { 12. };
    let color = if ghost.royal {
        Color::from_hex(0x7b4aa0)
    } else if ghost.fast {
        juice::ZOOMIE
    } else {
        juice::BEETLE
    };
    // Legs up, flailing, x-eyes: the classic dramatic exit.
    let flail = (ghost.age * 40.).sin() * 4. * juice.motion();
    for n in [-1., 1.] {
        for k in [-5., 0., 5.] {
            let root = p + vec2(k, -r * 0.6);
            let tip = root + vec2(n * 3. + flail * n, -9.);
            draw_line(root.x, root.y, tip.x, tip.y, 2., with_alpha(INK, a));
        }
    }
    draw_ellipse(
        p.x,
        p.y,
        r,
        r * 0.75,
        0.,
        with_alpha(mix(color, CREAM, 0.25), a),
    );
    for n in [-1., 1.] {
        let e = p + vec2(n * r * 0.4, -1.);
        draw_line(
            e.x - 2.5,
            e.y - 2.5,
            e.x + 2.5,
            e.y + 2.5,
            1.5,
            with_alpha(INK, a),
        );
        draw_line(
            e.x - 2.5,
            e.y + 2.5,
            e.x + 2.5,
            e.y - 2.5,
            1.5,
            with_alpha(INK, a),
        );
    }
}

fn plots(g: &Game, juice: &Juice, compact: bool, boost: f32, hover: Option<usize>) {
    let teach =
        g.wave == 0 && g.towers.iter().all(Option::is_none) && g.outcome == Outcome::Playing;
    for (i, p) in g.level.plots.iter().enumerate() {
        let selected = g.selected == i && g.outcome == Outcome::Playing;
        if selected {
            let range = g.towers[i]
                .unwrap_or_else(|| Tower::planted(Kind::Seed))
                .range();
            shapes::fill_circle(
                *p,
                range,
                Fill::radial(
                    *p,
                    range,
                    Color::new(1., 0.97, 0.78, 0.04),
                    Color::new(1., 0.97, 0.78, 0.16),
                ),
            );
            shapes::stroke_circle(*p, range, 2., Color::new(1., 0.97, 0.80, 0.75));
            if g.towers[i].is_none() {
                shapes::stroke_circle(
                    *p,
                    Tower::planted(Kind::Frost).range(),
                    2.,
                    Color::new(0.45, 0.78, 0.82, 0.75),
                );
            }
        }
        let lift = if hover == Some(i) && g.outcome == Outcome::Playing {
            3.
        } else {
            0.
        };
        draw_ellipse(
            p.x + 4.,
            p.y + 15.,
            26.,
            11.,
            0.,
            Color::new(0.22, 0.30, 0.16, 0.24),
        );
        let top = *p - vec2(0., lift);
        shapes::fill_circle(
            top,
            25.,
            Fill::radial(
                top - vec2(8., 9.),
                36.,
                Color::from_hex(0xe0d4a2),
                Color::from_hex(0xbfb07c),
            ),
        );
        shapes::stroke_circle(
            top,
            25.,
            if selected { 3. } else { 2. },
            if selected {
                CREAM
            } else if lift > 0. {
                Color::from_hex(0xf0e6c0)
            } else {
                Color::from_hex(0x8c9866)
            },
        );
        if let Some(t) = g.towers[i] {
            tower(top, t, juice.tower_scale(i), true);
        } else {
            draw_line(
                top.x - 7.,
                top.y,
                top.x + 7.,
                top.y,
                2.5,
                Color::from_hex(0x7d8a5f),
            );
            draw_line(
                top.x,
                top.y - 7.,
                top.x,
                top.y + 7.,
                2.5,
                Color::from_hex(0x7d8a5f),
            );
        }
        if !compact {
            tag(sfmt!("{}", i + 1).as_str(), p.x, p.y + 46., 14., INK);
        }
        if teach && selected {
            // First-run hint: bounce an arrow at the selected plot until a
            // tower exists. Under reduced motion it holds still.
            let bob = (juice.clock * 5.).sin().abs() * 6. * juice.motion();
            let tip = vec2(p.x, p.y - 32. - bob);
            post::glow_circle(tip.x, tip.y - 14., 18., Color::new(1., 0.8, 0.35, 1.), 0.5);
            draw_triangle(
                tip,
                tip + vec2(-10., -13.),
                tip + vec2(10., -13.),
                juice::GOLD,
            );
            shapes::fill_rounded_rect(
                Rect::new(tip.x - 3.5, tip.y - 26., 7., 14.),
                2.,
                juice::GOLD,
            );
            tag("Plant here", p.x, tip.y - 32., 16. * boost, INK);
        }
    }
}

/// Draw the board. `boost` enlarges world-space text in compact layouts;
/// `hover` is the plot under the pointer, if any.
pub(super) fn field(
    g: &Game,
    presentation: &Presentation,
    juice: &Juice,
    light: &DayLight,
    compact: bool,
    boost: f32,
    hover: Option<usize>,
) {
    ground(g, juice, light);
    farmhouse(g, juice);
    critters(juice);
    plots(g, juice, compact, boost, hover);
    for g_ in juice.ghosts.iter().flatten() {
        ghost(g_, juice);
    }
    for p in &g.pests {
        pest(p, g, presentation, juice, boost);
    }
    if let Some(target) = g.selected_target() {
        shapes::stroke_circle(target.pos, 22., 2., INK);
    }
    // The royal name tag already sits above its pest; skip the duplicate label.
    if let Some(target) = g.selected_target().filter(|t| t.id != ROYAL_ID) {
        tag(
            "Target",
            target.pos.x,
            target.pos.y - 28.,
            (11. * boost).round(),
            INK,
        );
    }
    look::atmosphere(g, juice, light, juice.flash_amount());
    // Projectiles are emissive: a warm or icy core with a soft bloom halo.
    for s in &g.shots {
        let (color, r) = match (s.kind, s.lucky) {
            (_, true) => (juice::GOLD, 6.),
            (Kind::Seed, false) => (Color::from_hex(0xf3d080), 4.),
            (Kind::Frost, false) => (Color::from_hex(0xd1f1e8), 4.),
        };
        if let Some(target) = g.pests.iter().find(|p| p.id == s.target) {
            let back = s.pos - (target.pos - s.pos).normalize_or_zero() * 12.;
            draw_line(back.x, back.y, s.pos.x, s.pos.y, r, with_alpha(color, 0.35));
        }
        post::with_glow(|| dot(s.pos.x, s.pos.y, r * 2.2, with_alpha(color, 0.35)));
        dot(s.pos.x, s.pos.y, r, color);
        if s.lucky {
            post::glow_circle(s.pos.x, s.pos.y, 16., Color::new(1., 0.8, 0.3, 1.), 1.2);
        }
    }
    for ring in juice.rings.iter().flatten() {
        let t = ring.age / ring.life;
        let r = ring.radius * (0.4 + 0.6 * (1. - (1. - t).powi(3)));
        shapes::stroke_circle(
            ring.at,
            r,
            3. * (1. - t) + 1.,
            with_alpha(ring.color, 1. - t),
        );
    }
}

/// Text pops are drawn after particles so they stay readable: arcade words in
/// the classic face, rewards with a coin, all on a soft plate.
pub(super) fn pops(juice: &Juice, boost: f32) {
    for pop in juice.pops.iter().flatten() {
        let t = pop.age / pop.life;
        let a = if t > 0.7 { (1. - t) / 0.3 } else { 1. };
        let at = pop.at - vec2(0., juice.rise(pop));
        let size = (pop.size * boost).round();
        let word: TextStyle =
            punch(size, with_alpha(pop.color, a)).with_shadow(with_alpha(INK, a * 0.6));
        let reward = body(size, with_alpha(juice::GOLD, a)).with_shadow(with_alpha(INK, a * 0.6));
        let value = pop.value.map(|v| sfmt!("+{v}"));
        let word_w = if pop.text.is_empty() {
            0.
        } else {
            styled_width(pop.text, word)
        };
        let icon = if value.is_some() { size * 0.9 } else { 0. };
        let value_w = value
            .as_ref()
            .map_or(0., |v| styled_width(v.as_str(), reward));
        let gap = if word_w > 0. && value_w > 0. {
            size * 0.4
        } else {
            0.
        };
        let w = word_w + gap + icon + value_w;
        let x = at.x.clamp(w * 0.5 + 8., 1192. - w * 0.5);
        let plate = Rect::new(
            x - w * 0.5 - 6.,
            at.y - size * 0.95 - 4.,
            w + 12.,
            size * 0.95 + 9.,
        );
        shapes::fill_rounded_rect(
            plate,
            plate.h * 0.5,
            with_alpha(Color::new(0.14, 0.2, 0.13, 1.), 0.82 * a),
        );
        let mut left = x - w * 0.5;
        if word_w > 0. {
            style::text(pop.text, left, at.y, word);
            left += word_w + gap;
        }
        if let Some(v) = value {
            coin_icon(left + icon * 0.42, at.y - size * 0.42, size * 0.36);
            style::text(v.as_str(), left + icon, at.y, reward);
        }
    }
}

// HUD uses the same measured Modern face as the field labels.
pub(super) fn text_width(s: &str, size: f32) -> f32 {
    ferravex_render::text::text_width(s, body(size, INK))
}
pub(super) fn fit_center(s: &str, x: f32, y: f32, size: f32, max: f32, color: Color) {
    style::text_fit(s, x, y, body(size, color), max);
}
