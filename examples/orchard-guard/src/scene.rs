//! World drawing: the living orchard, towers, pests and accepted-event juice.
//! Drawing reads simulation state and presentation state; it never mutates
//! either, so frame rate and preferences cannot change the match.
use super::*;
use crate::juice::{self, GHOST_LIFE, Juice};

/// Width of bitmap text: 6 font pixels per glyph, 5 lit, size/8 per pixel.
pub(super) fn text_width(s: &str, size: f32) -> f32 {
    (s.chars().count() as f32 * 6. - 1.).max(0.) * size / 8.
}

/// Centered bitmap text whose size shrinks to fit `max` world units.
pub(super) fn fit_center(s: &str, x: f32, y: f32, size: f32, max: f32, color: Color) {
    let size = (size * (max / text_width(s, size).max(1.)).min(1.)).max(6.);
    label(s, x - text_width(s, size) * 0.5, y, size, color);
}

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
        p.x + 7.,
        p.y + 18.,
        25.,
        9.,
        0.,
        Color::new(0.22, 0.32, 0.19, 0.13),
    );
    draw_rectangle(p.x - 3., p.y, 6., 25., Color::from_hex(0x85704c));
    let top = vec2(p.x + sway, p.y);
    draw_poly(top.x, top.y - 8., 7, 25., 15., Color::from_hex(0x69894f));
    draw_poly(
        top.x - 8. + sway * 0.4,
        top.y - 14.,
        7,
        20.,
        0.,
        Color::from_hex(0x78975b),
    );
    draw_circle(
        top.x + 6.,
        top.y - 20.,
        6.,
        Color::new(0.62, 0.74, 0.45, 0.55),
    );
    for i in 0..3 {
        let a = (i as f32 * 2.1 + variant as f32) * 1.4;
        draw_circle(
            top.x + a.cos() * 14.,
            top.y - 10. + a.sin() * 13.,
            3.6,
            juice::APPLE,
        );
    }
}

fn heart(x: f32, y: f32, r: f32, color: Color) {
    draw_circle(x - r * 0.5, y - r * 0.2, r * 0.55, color);
    draw_circle(x + r * 0.5, y - r * 0.2, r * 0.55, color);
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
    draw_circle(x, y, r, juice::GOLD);
    draw_circle_lines(x, y, r * 0.7, 1.5, Color::from_hex(0xb7862c));
    draw_rectangle(
        x - r * 0.12,
        y - r * 0.4,
        r * 0.24,
        r * 0.8,
        Color::from_hex(0xb7862c),
    );
}

fn ground(g: &Game, juice: &Juice) {
    draw_rectangle(0., 70., 1200., 590., Color::from_hex(0xa7b87a));
    // Soft lighter meadows break up the flat field.
    for (x, y, rx, ry) in [
        (170., 300., 160., 90.),
        (640., 470., 230., 70.),
        (900., 260., 160., 70.),
        (420., 620., 260., 40.),
    ] {
        draw_ellipse(x, y, rx, ry, 0., Color::new(0.78, 0.83, 0.55, 0.30));
    }
    for i in 0..130 {
        let x = ((i * 97 + 31) % 1200) as f32;
        let y = 85. + ((i * 67 + 21) % 560) as f32;
        draw_line(x, y, x + 3., y - 5., 1., Color::new(0.35, 0.46, 0.25, 0.20));
    }
    // Tiny wildflowers.
    for i in 0..44 {
        let x = ((i * 271 + 57) % 1180) as f32 + 10.;
        let y = 90. + ((i * 151 + 33) % 550) as f32;
        let color = [
            Color::from_hex(0xfdf3d0),
            Color::from_hex(0xf6d36b),
            Color::from_hex(0xe9a3a0),
        ][i % 3];
        draw_circle(x, y, 2., color);
    }
    // Drifting cloud shadows (still under reduced motion: none).
    if juice.motion() > 0. {
        for (offset, y, r) in [(0., 260., 120.), (720., 520., 150.)] {
            let x = (juice.clock * 14. + offset) % 1560. - 180.;
            draw_ellipse(x, y, r, r * 0.45, 0., Color::new(0.15, 0.25, 0.12, 0.07));
        }
    }
    for pair in g.level.path.windows(2) {
        draw_line(
            pair[0].x + 3.,
            pair[0].y + 6.,
            pair[1].x + 3.,
            pair[1].y + 6.,
            51.,
            Color::new(0.38, 0.43, 0.24, 0.18),
        );
    }
    for pair in g.level.path.windows(2) {
        draw_line(
            pair[0].x,
            pair[0].y,
            pair[1].x,
            pair[1].y,
            45.,
            Color::from_hex(0xd5bf8a),
        );
    }
    for p in &g.level.path {
        draw_circle(p.x, p.y, 22.5, Color::from_hex(0xd5bf8a));
    }
    for pair in g.level.path.windows(2) {
        let len = pair[0].distance(pair[1]);
        let d = (pair[1] - pair[0]) / len;
        for i in 0..(len / 23.) as usize {
            let p = pair[0] + d * (i as f32 * 23.);
            draw_circle(p.x, p.y, 1.3, Color::new(0.52, 0.42, 0.26, 0.28));
        }
    }
    for (i, p) in TREES.into_iter().enumerate() {
        tree(p, i, juice.sway(i));
    }
}

fn farmhouse(g: &Game, juice: &Juice) {
    if juice.gate > 0. {
        draw_circle(
            1150.,
            370.,
            70.,
            Color::new(0.90, 0.30, 0.22, 0.30 * juice.gate),
        );
    }
    draw_rectangle(1120., 385., 69., 43., Color::from_hex(0xf0e5bc));
    draw_triangle(
        vec2(1111., 386.),
        vec2(1154., 351.),
        vec2(1197., 386.),
        Color::from_hex(0xac6848),
    );
    draw_rectangle(1147., 404., 15., 24., Color::from_hex(0x7d6848));
    draw_rectangle(1128., 395., 12., 10., Color::from_hex(0x9ec6b5));
    for y in [302., 353.] {
        for x in [1135., 1156., 1177.] {
            draw_rectangle(x, y, 5., 23., CREAM);
        }
        draw_rectangle(1135., y + 7., 47., 4., CREAM);
    }
    label("ORCHARD", 1106., 450., 15., GREEN);
    // The harvest basket holds one apple per remaining heart.
    draw_rectangle(1108., 492., 80., 22., Color::from_hex(0x9b7448));
    for x in 0..5 {
        draw_line(
            1112. + x as f32 * 17.,
            492.,
            1112. + x as f32 * 17.,
            514.,
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
            draw_circle(x, y, 7., juice::APPLE);
            draw_circle(x - 2., y - 2., 2., Color::new(1., 0.8, 0.7, 0.6));
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
        draw_circle(p.x, p.y, 1.2, INK);
    }
}

fn tower(p: Vec2, t: Tower, scale: Vec2) {
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
            draw_rectangle(
                p.x - s(17.),
                p.y - h(16.),
                s(34.),
                h(8.),
                Color::from_hex(0xbaa477),
            );
            let muzzle = p + t.aim * (25. - (1. - scale.y) * 50.).max(14.);
            draw_line(p.x, p.y - 5., muzzle.x, muzzle.y - 5., 10., GREEN);
            draw_circle(p.x, p.y - h(5.), s(12.), Color::from_hex(0x496c3b));
            if t.level >= 1 {
                draw_circle(p.x, p.y - h(5.), s(6.), Color::from_hex(0x6f9a52));
            }
        }
        Kind::Frost => {
            draw_rectangle(
                p.x - s(14.),
                p.y - h(12.),
                s(28.),
                h(22.),
                Color::from_hex(0x679b99),
            );
            for y in [-12., -4., 4.] {
                draw_line(
                    p.x - s(15.),
                    p.y + h(y),
                    p.x + s(15.),
                    p.y + h(y),
                    3.,
                    Color::from_hex(0x9ec6b5),
                );
            }
            draw_triangle(
                vec2(p.x - s(20.), p.y - h(14.)),
                vec2(p.x, p.y - h(28.)),
                vec2(p.x + s(20.), p.y - h(14.)),
                Color::from_hex(0x447d80),
            );
            draw_circle(p.x, p.y, s(5.), INK);
        }
    }
    if t.level >= 2 {
        // Fully grown towers wear a golden leaf crown.
        for dx in [-8., 0., 8.] {
            draw_triangle(
                vec2(p.x + dx - 4., p.y - h(30.)),
                vec2(p.x + dx, p.y - h(39.)),
                vec2(p.x + dx + 4., p.y - h(30.)),
                juice::GOLD,
            );
        }
    }
    for n in 0..3u8 {
        let x = p.x - 9. + f32::from(n) * 9.;
        draw_circle(x, p.y + 20., 3.4, Color::new(0.24, 0.31, 0.18, 0.5));
        if n <= t.level {
            draw_circle(x, p.y + 20., 2.6, juice::GOLD);
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
        Color::new(0.2, 0.2, 0.15, 0.2),
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
    draw_circle(p.x, p.y - bob, radius, color);
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
        draw_circle(s.x, s.y - bob, radius * 0.18, spot);
    }
    let head = p + dir * (radius - 1.);
    draw_circle(head.x, head.y - bob, radius * 0.5, INK);
    for n in [-1., 1.] {
        let root = head + dir * 3. + side * n * 2.;
        let tip = head + dir * 9. + side * n * (6. + wiggle * 0.5);
        draw_line(root.x, root.y - bob, tip.x, tip.y - bob, 1.2, INK);
        // Googly eyes: pupils jiggle with the walk.
        let eye = head + dir * 2. + side * n * radius * 0.28;
        draw_circle(eye.x, eye.y - bob, radius * 0.24, CREAM);
        let pupil = eye + dir * radius * 0.08 + side * wiggle * 0.25;
        draw_circle(pupil.x, pupil.y - bob, radius * 0.12, INK);
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
        let tag = 11. * boost;
        let tw = text_width("SIR CRUNCHALOT", tag);
        draw_rectangle(
            p.x - tw * 0.5 - 4.,
            p.y - radius - 18. - tag * 0.875 - 3.,
            tw + 8.,
            tag * 0.875 + 6.,
            Color::new(0.98, 0.95, 0.84, 0.85),
        );
        fit_center(
            "SIR CRUNCHALOT",
            p.x,
            p.y - radius - 18.,
            11. * boost,
            200.,
            Color::from_hex(0x5d3a78),
        );
    }
    if let Some(slow) = pest.slow() {
        // Shape and countdown remain legible with flashes/motion disabled.
        draw_circle_lines(p.x, p.y, radius + 6., 1.5, Color::from_hex(0x2f6f78));
        for k in 0..4 {
            let a = k as f32 * std::f32::consts::FRAC_PI_2 + 0.4;
            let q = p + vec2(a.cos(), a.sin()) * (radius + 6.);
            draw_circle(q.x, q.y, 2., juice::ICE);
        }
        center(
            &format!("SLOW {:.1}s", slow.remaining_seconds),
            p.x,
            p.y + radius + 22.,
            (10. * boost).round(),
            INK,
        );
    }
    if pest.hp < pest.max_hp {
        let w = radius * 2. + 2.;
        draw_rectangle(
            p.x - w * 0.5 - 1.,
            p.y - radius - 9.,
            w + 2.,
            5.,
            Color::new(0.2, 0.25, 0.18, 0.55),
        );
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

/// Draw the board. `boost` enlarges world-space text in compact layouts.
pub(super) fn field(
    g: &Game,
    presentation: &Presentation,
    juice: &Juice,
    compact: bool,
    boost: f32,
) {
    ground(g, juice);
    farmhouse(g, juice);
    critters(juice);
    let teach =
        g.wave == 0 && g.towers.iter().all(Option::is_none) && g.outcome == Outcome::Playing;
    for (i, p) in g.level.plots.iter().enumerate() {
        if g.selected == i && g.outcome == Outcome::Playing {
            let range = g.towers[i]
                .unwrap_or_else(|| Tower::planted(Kind::Seed))
                .range();
            draw_circle(p.x, p.y, range, Color::new(0.96, 0.95, 0.72, 0.12));
            draw_circle_lines(p.x, p.y, range, 1.5, Color::new(0.96, 0.95, 0.72, 0.6));
            if g.towers[i].is_none() {
                draw_circle_lines(
                    p.x,
                    p.y,
                    Tower::planted(Kind::Frost).range(),
                    1.5,
                    Color::new(0.40, 0.70, 0.75, 0.65),
                );
            }
        }
        draw_ellipse(
            p.x + 4.,
            p.y + 15.,
            25.,
            11.,
            0.,
            Color::new(0.24, 0.31, 0.18, 0.22),
        );
        draw_circle(p.x, p.y, 25., Color::from_hex(0xc9bd8a));
        let selected = g.selected == i && g.outcome == Outcome::Playing;
        draw_circle_lines(
            p.x,
            p.y,
            25.,
            if selected { 3. } else { 2. },
            if selected {
                CREAM
            } else {
                Color::from_hex(0x8c9866)
            },
        );
        if let Some(t) = g.towers[i] {
            tower(*p, t, juice.tower_scale(i));
        } else {
            draw_line(p.x - 7., p.y, p.x + 7., p.y, 2., Color::from_hex(0x7d8a5f));
            draw_line(p.x, p.y - 7., p.x, p.y + 7., 2., Color::from_hex(0x7d8a5f));
        }
        if !compact {
            center(&format!("{}", i + 1), p.x, p.y + 44., 14., INK);
        }
        if teach && selected {
            // First-run hint: bounce an arrow at the selected plot until a
            // tower exists. Under reduced motion it holds still.
            let bob = (juice.clock * 5.).sin().abs() * 6. * juice.motion();
            let tip = vec2(p.x, p.y - 32. - bob);
            draw_triangle(
                tip,
                tip + vec2(-9., -12.),
                tip + vec2(9., -12.),
                juice::GOLD,
            );
            draw_rectangle(tip.x - 3., tip.y - 24., 6., 12., juice::GOLD);
            fit_center("PLANT HERE", p.x, tip.y - 30., 13. * boost, 240., INK);
        }
    }
    for g_ in juice.ghosts.iter().flatten() {
        ghost(g_, juice);
    }
    for p in &g.pests {
        pest(p, g, presentation, juice, boost);
    }
    if let Some(target) = g.selected_target() {
        draw_circle_lines(target.pos.x, target.pos.y, 22., 2., INK);
    }
    // The royal name tag already sits above its pest; skip the duplicate label.
    if let Some(target) = g.selected_target().filter(|t| t.id != ROYAL_ID) {
        center(
            "TARGET",
            target.pos.x,
            target.pos.y - 30.,
            (10. * boost).round(),
            INK,
        );
    }
    for s in &g.shots {
        let (color, r) = match (s.kind, s.lucky) {
            (_, true) => (juice::GOLD, 6.),
            (Kind::Seed, false) => (Color::from_hex(0xf3d080), 4.),
            (Kind::Frost, false) => (Color::from_hex(0xd1f1e8), 4.),
        };
        if let Some(target) = g.pests.iter().find(|p| p.id == s.target) {
            let back = s.pos - (target.pos - s.pos).normalize_or_zero() * 10.;
            draw_line(back.x, back.y, s.pos.x, s.pos.y, r, with_alpha(color, 0.35));
        }
        draw_circle(s.pos.x, s.pos.y, r, color);
    }
    for ring in juice.rings.iter().flatten() {
        let t = ring.age / ring.life;
        let r = ring.radius * (0.4 + 0.6 * (1. - (1. - t).powi(3)));
        draw_circle_lines(
            ring.at.x,
            ring.at.y,
            r,
            3. * (1. - t) + 1.,
            with_alpha(ring.color, 1. - t),
        );
    }
}

/// Text pops are drawn after particles so they stay readable.
pub(super) fn pops(juice: &Juice, boost: f32) {
    for pop in juice.pops.iter().flatten() {
        let t = pop.age / pop.life;
        let a = if t > 0.7 { (1. - t) / 0.3 } else { 1. };
        let at = pop.at - vec2(0., juice.rise(pop));
        let size = (pop.size * boost).round();
        let text = juice::pop_label(pop.text, pop.value);
        let icon = if pop.value.is_some() { size } else { 0. };
        let w = text_width(&text, size) + icon;
        let x = at.x.clamp(w * 0.5 + 6., 1194. - w * 0.5);
        // A dark plate keeps pops legible over pests, labels and each other.
        draw_rectangle(
            x - w * 0.5 - 5.,
            at.y - size * 0.875 - 4.,
            w + 10.,
            size * 0.875 + 8.,
            with_alpha(Color::new(0.14, 0.2, 0.13, 1.), 0.88 * a),
        );
        if pop.value.is_some() {
            coin_icon(x - w * 0.5 + size * 0.4, at.y - size * 0.44, size * 0.38);
        }
        center(&text, x + icon * 0.5, at.y, size, with_alpha(pop.color, a));
    }
}
