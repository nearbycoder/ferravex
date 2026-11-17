//! Kitchen presentation: a dining room of guests above a tiled kitchen, a
//! recipe card and icon stats. Everything here is read-only. Idle motion
//! follows the host's presentation clock and hover highlights its pointer;
//! effects come only from the model's accepted-event feed.
use crate::{
    Game,
    content::*,
    model::{Carry, Event, Order, Phase, Why, walkable},
};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;
use std::f32::consts::{PI, TAU};

const CREAM: Color = Color::new(0.97, 0.89, 0.72, 1.);
const TEAL: Color = Color::new(0.13, 0.40, 0.42, 1.);
const DARK: Color = Color::new(0.08, 0.19, 0.23, 1.);
const PANEL: Color = Color::new(0.07, 0.14, 0.17, 1.);
const CORAL: Color = Color::new(0.96, 0.43, 0.29, 1.);
const GOLD: Color = Color::new(1., 0.69, 0.30, 1.);
const GREEN: Color = Color::new(0.44, 0.76, 0.42, 1.);
const WOOD: Color = Color::new(0.63, 0.42, 0.26, 1.);
const SAGE: Color = Color::new(0.62, 0.74, 0.70, 1.);
const ACCENT: Color = Color::new(1., 0.62, 0.3, 1.);
const SHADE: Color = Color::new(0.02, 0.05, 0.06, 0.28);
const FLAME: Color = Color::new(0.4, 0.75, 1., 0.85);
const POT: Color = Color::new(0.22, 0.46, 0.62, 1.);
const SOOT: Color = Color::new(0.16, 0.14, 0.13, 1.);
const SMOKE: Color = Color::new(0.32, 0.31, 0.30, 1.);
const STEEL: Color = Color::new(0.88, 0.92, 0.94, 1.);
const CHAIR: Color = Color::new(0.36, 0.22, 0.14, 1.);
const MUSHROOM: Color = Color::new(0.79, 0.64, 0.51, 1.);
const FOOD: [Color; 4] = [CORAL, GREEN, MUSHROOM, CREAM];
const SKIN: [Color; 4] = [
    Color::new(0.96, 0.78, 0.62, 1.),
    Color::new(0.80, 0.58, 0.42, 1.),
    Color::new(0.58, 0.40, 0.28, 1.),
    Color::new(0.93, 0.68, 0.47, 1.),
];
const SHIRT: [Color; 5] = [
    Color::new(0.35, 0.55, 0.85, 1.),
    Color::new(0.62, 0.42, 0.78, 1.),
    Color::new(0.30, 0.66, 0.58, 1.),
    Color::new(0.88, 0.52, 0.62, 1.),
    Color::new(0.90, 0.72, 0.32, 1.),
];
const HAIR: [Color; 3] = [
    Color::new(0.25, 0.16, 0.10, 1.),
    Color::new(0.10, 0.09, 0.09, 1.),
    Color::new(0.80, 0.55, 0.25, 1.),
];
/// Center of the arrival ring in the dining-room door.
const DOOR: Vec2 = Vec2::new(58., 160.);
/// Right edge of the kitchen floor plan.
const KITCHEN_RIGHT: f32 = GRID_X + 11. * TILE;

// ------------------------------------------------------------------ text

fn flat(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
/// Shadowless label shrunk to `width`, aligned as requested.
fn label(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color, align: TextAlign) {
    let s = flat(size, color).with_align(align);
    let w = text_width(value, s);
    let s = if w > width && w > 0. {
        s.with_size(size * width / w)
    } else {
        s
    };
    draw_text_styled(value, x, y, s);
}
/// `ticks` as whole or tenth seconds: `3s`, `3.5s`.
fn secs(ticks: u32) -> String {
    if ticks.is_multiple_of(60) {
        format!("{}s", ticks / 60)
    } else {
        format!("{:.1}s", ticks as f32 / 60.)
    }
}
/// `ticks` as `m:ss`.
fn clock_text(ticks: u32) -> String {
    let secs = ticks.div_ceil(60);
    format!("{}:{:02}", secs / 60, secs % 60)
}

// -------------------------------------------------------------- geometry

fn cell_center(cell: Cell) -> Vec2 {
    vec2(
        GRID_X + (cell.x as f32 + 0.5) * TILE,
        GRID_Y + (cell.y as f32 + 0.5) * TILE,
    )
}
fn station_index(station: Station) -> usize {
    STATIONS.iter().position(|s| s.0 == station).unwrap_or(0)
}
/// Center of a station's artwork.
fn station_center(station: Station) -> Vec2 {
    cell_center(STATIONS[station_index(station)].1)
}
/// Center of table `slot`'s order bubble.
fn bubble_center(slot: usize) -> Vec2 {
    vec2(TABLE_X + slot as f32 * TABLE_STEP + 128., 152.)
}
fn thumb_center(i: usize) -> Vec2 {
    vec2(THUMB_X + i as f32 * THUMB_STEP, THUMB_Y)
}
fn upgrade_center(kind: usize) -> Vec2 {
    vec2(212. + kind as f32 * 270., 394.)
}
/// The cook's drawn position, gliding between cells while walking.
fn cook_center(game: &Game) -> Vec2 {
    let a = cell_center(game.previous);
    let b = cell_center(game.position);
    a + (b - a) * walk_blend(game)
}
fn walk_blend(game: &Game) -> f32 {
    1. - game.move_wait as f32 / game.walk_ticks() as f32
}

// ---------------------------------------------------------------- shapes

/// Axis-aligned ellipse in one batched draw.
fn oval(x: f32, y: f32, rx: f32, ry: f32, color: Color) {
    draw_ellipse(x, y, rx, ry, 0., color);
}
/// Small filled circle in one batched draw, tessellated by size.
fn dot(x: f32, y: f32, r: f32, color: Color) {
    draw_poly(x, y, (r * 0.9).clamp(8., 24.) as u8, r, 0., color);
}
fn disc(c: Vec2, r: f32, color: Color) {
    shapes::fill_circle(c, r, color);
}
fn ring(c: Vec2, r: f32, width: f32, color: Color) {
    shapes::stroke_circle(c, r, width, color);
}
/// A small right-pointing chevron (step arrows).
fn chevron(x: f32, y: f32, s: f32, color: Color) {
    shapes::fill_triangle(
        vec2(x - 3. * s, y - 5. * s),
        vec2(x + 4. * s, y),
        vec2(x - 3. * s, y + 5. * s),
        color,
    );
}
/// Corner brackets marking a station's access tile.
fn marker(ax: f32, ay: f32, color: Color) {
    let (l, t, r, b) = (ax - 19., ay - 19., ax + 19., ay + 19.);
    for (x, y, dx, dy) in [
        (l, t, 1., 1.),
        (r, t, -1., 1.),
        (l, b, 1., -1.),
        (r, b, -1., -1.),
    ] {
        let x0 = if dx > 0. { x } else { x - 9. };
        let y0 = if dy > 0. { y } else { y - 2. };
        panel(x0, y0, 9., 2., color);
        let x1 = if dx > 0. { x } else { x - 2. };
        let y1 = if dy > 0. { y } else { y - 9. };
        panel(x1, y1, 2., 9., color);
    }
}

// ---------------------------------------------------------------- food art

fn ingredient(i: usize, x: f32, y: f32, scale: f32, chopped: bool) {
    let color = FOOD[i];
    if chopped {
        for k in 0..5 {
            let dx = (k % 3) as f32 * 6. - 8.;
            let dy = (k / 3) as f32 * 6. - 5.;
            panel(
                x + dx * scale,
                y + dy * scale,
                4. * scale,
                4. * scale,
                darken(color, 0.12),
            );
            panel(
                x + dx * scale,
                y + dy * scale,
                4. * scale,
                1.5 * scale,
                lighten(color, 0.2),
            );
        }
        return;
    }
    match i {
        0 => {
            dot(x, y, 9. * scale, darken(CORAL, 0.1));
            dot(x - 1. * scale, y - 1. * scale, 7.5 * scale, CORAL);
            draw_line(x, y - 10. * scale, x, y - 6. * scale, 3. * scale, GREEN);
            draw_line(
                x - 4. * scale,
                y - 7. * scale,
                x + 4. * scale,
                y - 7. * scale,
                2. * scale,
                GREEN,
            );
            dot(
                x - 3.5 * scale,
                y - 3. * scale,
                2. * scale,
                Color::new(1., 1., 1., 0.55),
            );
        }
        1 => {
            draw_triangle(
                vec2(x - 8. * scale, y - 6. * scale),
                vec2(x, y + 9. * scale),
                vec2(x - 1. * scale, y - 9. * scale),
                darken(GREEN, 0.15),
            );
            draw_triangle(
                vec2(x + 8. * scale, y - 6. * scale),
                vec2(x, y + 9. * scale),
                vec2(x + 1. * scale, y - 9. * scale),
                GREEN,
            );
            oval(
                x - 4. * scale,
                y,
                4.5 * scale,
                7.5 * scale,
                darken(GREEN, 0.15),
            );
            oval(x + 4. * scale, y, 4.5 * scale, 7.5 * scale, GREEN);
            draw_line(x, y + 8. * scale, x, y - 5. * scale, 1.5 * scale, CREAM);
        }
        2 => {
            panel(x - 3. * scale, y, 6. * scale, 9. * scale, CREAM);
            oval(
                x,
                y - 2. * scale,
                10. * scale,
                7. * scale,
                darken(color, 0.1),
            );
            oval(x, y - 3. * scale, 9. * scale, 5.5 * scale, color);
            dot(x - 4. * scale, y - 4. * scale, 1.5 * scale, WOOD);
            dot(x + 4. * scale, y - 2. * scale, 1.5 * scale, WOOD);
        }
        _ => {
            rounded(
                x - 8. * scale,
                y - 9. * scale,
                16. * scale,
                18. * scale,
                3. * scale,
                CREAM,
            );
            panel(
                x - 8. * scale,
                y - 9. * scale,
                16. * scale,
                3. * scale,
                WOOD,
            );
            for k in 0..4 {
                draw_line(
                    x - 4. * scale + (k % 2) as f32 * 6. * scale,
                    y - 1. * scale + (k / 2) as f32 * 5. * scale,
                    x - 2. * scale + (k % 2) as f32 * 6. * scale,
                    y - 3. * scale + (k / 2) as f32 * 5. * scale,
                    1.5 * scale,
                    GOLD,
                );
            }
        }
    }
}
/// A finished dish: on a cream plate when `plated`, else in a teal bowl.
fn dish(recipe: usize, x: f32, y: f32, scale: f32, plated: bool) {
    oval(x, y + 5. * scale, 16. * scale, 5. * scale, SHADE);
    oval(
        x,
        y + 3. * scale,
        17. * scale,
        10. * scale,
        if plated { CREAM } else { TEAL },
    );
    oval(
        x,
        y + 1. * scale,
        15. * scale,
        8. * scale,
        if plated {
            darken(CREAM, 0.08)
        } else {
            lighten(TEAL, 0.1)
        },
    );
    oval(
        x,
        y,
        12. * scale,
        6.5 * scale,
        if recipe == 0 { GREEN } else { GOLD },
    );
    for i in 0..4 {
        if RECIPES[recipe].mask & (1 << i) != 0 {
            ingredient(
                i,
                x + (i as f32 - 1.5) * 5.5 * scale + 1.5 * scale,
                y - 1. * scale,
                0.36 * scale,
                true,
            );
        }
    }
}
/// The recipe's prepared ingredients in a row centered on `cx`; returns the width.
fn ingredient_row(recipe: usize, cx: f32, cy: f32, scale: f32, step: f32) -> f32 {
    let mask = RECIPES[recipe].mask;
    let n = mask.count_ones() as f32;
    let mut x = cx - (n - 1.) * step * 0.5;
    for i in 0..4 {
        if mask & (1 << i) != 0 {
            ingredient(i, x, cy, scale, false);
            x += step;
        }
    }
    n * step
}
/// Wisps of steam rising from `x`, `y`.
fn steam(x: f32, y: f32, s: f32, t: f32, color: Color) {
    for k in 0..3 {
        let phase = (t * 0.9 + k as f32 / 3.).fract();
        let rise = phase * 12. * s;
        let sway = (t * 3. + k as f32 * 2.1).sin() * 2. * s;
        let a = (1. - phase) * phase * 4. * color.a;
        dot(
            x + (k as f32 - 1.) * 6. * s + sway,
            y - rise,
            (1.6 + phase * 1.6) * s,
            with_alpha(color, a * 0.8),
        );
    }
}

// --------------------------------------------------------- station glyphs

fn board_glyph(cx: f32, cy: f32, s: f32) {
    rounded(cx - 19. * s, cy - 10. * s, 38. * s, 26. * s, 6. * s, SHADE);
    rounded_gradient(
        cx - 19. * s,
        cy - 13. * s,
        38. * s,
        26. * s,
        6. * s,
        Color::new(0.86, 0.66, 0.44, 1.),
        WOOD,
    );
}
fn knife_glyph(cx: f32, cy: f32, s: f32) {
    shapes::fill_polygon(
        &[
            vec2(cx - 15. * s, cy),
            vec2(cx - 11. * s, cy - 3. * s),
            vec2(cx + 6. * s, cy - 3. * s),
            vec2(cx + 6. * s, cy + 3. * s),
            vec2(cx - 9. * s, cy + 3. * s),
        ],
        STEEL,
    );
    panel(
        cx - 11. * s,
        cy - 3. * s,
        17. * s,
        1.2 * s,
        Color::new(1., 1., 1., 0.6),
    );
    rounded(cx + 6. * s, cy - 3.5 * s, 11. * s, 7. * s, 2. * s, DARK);
    dot(cx + 9. * s, cy, 0.9 * s, SAGE);
    dot(cx + 13.5 * s, cy, 0.9 * s, SAGE);
}
fn bowl_glyph(cx: f32, cy: f32, s: f32) {
    oval(cx, cy + 4. * s, 19. * s, 6. * s, SHADE);
    oval(cx, cy, 19. * s, 12. * s, CREAM);
    oval(cx, cy - 3. * s, 16. * s, 8. * s, darken(TEAL, 0.25));
}
fn pot_glyph(cx: f32, cy: f32, s: f32, body: Color, rim: Color) {
    rounded(cx - 19. * s, cy - 7. * s, 6. * s, 4. * s, 2. * s, rim);
    rounded(cx + 13. * s, cy - 7. * s, 6. * s, 4. * s, 2. * s, rim);
    rounded_gradient(
        cx - 14. * s,
        cy - 10. * s,
        28. * s,
        20. * s,
        5. * s,
        lighten(body, 0.18),
        darken(body, 0.1),
    );
    rounded(cx - 16. * s, cy - 11. * s, 32. * s, 4. * s, 2. * s, rim);
}
fn burner_glyph(cx: f32, cy: f32, s: f32) {
    dot(cx, cy, 17. * s, Color::new(0.06, 0.08, 0.09, 1.));
    dot(cx, cy, 13.5 * s, Color::new(0.20, 0.22, 0.22, 1.));
    dot(cx, cy, 10.5 * s, Color::new(0.06, 0.08, 0.09, 1.));
}
fn flames(cx: f32, cy: f32, s: f32, t: f32) {
    for k in 0..6 {
        let a = k as f32 * TAU / 6. + t * 3.;
        let flick = 1. + (t * 24. + k as f32).sin() * 0.25;
        dot(
            cx + a.cos() * 14. * s,
            cy + a.sin() * 14. * s,
            2.6 * flick * s,
            FLAME,
        );
    }
}
fn plate_glyph(cx: f32, cy: f32, s: f32) {
    oval(cx, cy + 4. * s, 19. * s, 7. * s, SHADE);
    oval(cx, cy, 19. * s, 12. * s, CREAM);
    oval(cx, cy, 14. * s, 8. * s, darken(CREAM, 0.12));
    oval(cx, cy - 1. * s, 12. * s, 6. * s, CREAM);
}
fn bell_glyph(cx: f32, cy: f32, s: f32) {
    shapes::fill_sector(vec2(cx, cy + 3. * s), 8. * s, PI, PI, GOLD);
    panel(
        cx - 10. * s,
        cy + 3. * s,
        20. * s,
        2.5 * s,
        darken(GOLD, 0.3),
    );
    dot(cx, cy - 6. * s, 1.8 * s, GOLD);
    dot(
        cx - 3. * s,
        cy - 1. * s,
        1.5 * s,
        Color::new(1., 1., 1., 0.55),
    );
}
fn bin_glyph(cx: f32, cy: f32, s: f32) {
    rounded(
        cx - 12. * s,
        cy - 10. * s,
        24. * s,
        24. * s,
        4. * s,
        Color::new(0.16, 0.24, 0.27, 1.),
    );
    panel(
        cx - 6. * s,
        cy - 5. * s,
        2. * s,
        15. * s,
        Color::new(1., 1., 1., 0.12),
    );
    panel(
        cx + 4. * s,
        cy - 5. * s,
        2. * s,
        15. * s,
        Color::new(1., 1., 1., 0.12),
    );
    rounded(cx - 15. * s, cy - 15. * s, 30. * s, 5. * s, 2. * s, SAGE);
    rounded(cx - 4. * s, cy - 18. * s, 8. * s, 4. * s, 2. * s, SAGE);
}
fn shoe_glyph(cx: f32, cy: f32, s: f32) {
    for k in 0..2 {
        rounded(
            cx - 19. * s,
            cy - 5. * s + k as f32 * 5. * s,
            6. * s,
            2. * s,
            1. * s,
            with_alpha(SAGE, 0.8),
        );
    }
    rounded(cx - 10. * s, cy - 8. * s, 13. * s, 12. * s, 4. * s, CORAL);
    oval(cx + 4. * s, cy + 0.5 * s, 9. * s, 4.5 * s, CORAL);
    rounded(cx - 11. * s, cy + 3. * s, 25. * s, 4. * s, 2. * s, CREAM);
    for k in 0..2 {
        panel(
            cx - 6. * s + k as f32 * 4. * s,
            cy - 5. * s,
            2.5 * s,
            1.2 * s,
            CREAM,
        );
    }
}
/// The glyph for upgrade `kind`: knife, shoe or burner.
fn upgrade_glyph(kind: usize, cx: f32, cy: f32, s: f32, t: f32) {
    match kind {
        0 => knife_glyph(cx, cy, s),
        1 => shoe_glyph(cx, cy, s),
        _ => {
            burner_glyph(cx, cy, s * 0.8);
            flames(cx, cy, s * 0.8, t);
        }
    }
}

// ------------------------------------------------------------- dining room

/// String lights with twinkling bulbs between `x0` and `x1`.
fn lights(x0: f32, x1: f32, y: f32, t: f32) {
    const N: usize = 17;
    let step = (x1 - x0) / (N - 1) as f32;
    let at = |i: usize| {
        let sag = ((i % 4) as f32 / 4. * PI).sin() * 5.;
        vec2(x0 + i as f32 * step, y + sag)
    };
    for i in 0..N - 1 {
        let (a, b) = (at(i), at(i + 1));
        draw_line(a.x, a.y, b.x, b.y, 1., Color::new(1., 1., 1., 0.16));
    }
    for i in 0..N {
        let a = at(i);
        let c = if i % 2 == 0 { GOLD } else { CORAL };
        let lit = 0.10 + 0.16 * pulse(t + i as f32 * 0.37, 0.45);
        glow(a.x, a.y + 4., 10., with_alpha(c, lit));
        dot(a.x, a.y + 4., 3., c);
    }
}
fn dining(game: &Game, t: f32, p: Vec2) {
    let (x0, x1) = (24., 636.);
    let (y0, y1) = (DINING_TOP, DINING_BOTTOM);
    let w = x1 - x0;
    rounded(x0, y0 + 4., w, y1 - y0, 12., SHADE);
    rounded_gradient(
        x0,
        y0,
        w,
        y1 - y0,
        12.,
        Color::new(0.25, 0.16, 0.14, 1.),
        Color::new(0.19, 0.11, 0.10, 1.),
    );
    for k in 0..30 {
        panel(
            x0 + 14. + k as f32 * 20.,
            y0 + 4.,
            7.,
            62.,
            Color::new(1., 1., 1., 0.022),
        );
    }
    panel(x0, y0 + 66., w, 3., Color::new(0.46, 0.29, 0.19, 1.));
    // Plank floor.
    let fy = y1 - 34.;
    let planks = Color::new(0.43, 0.28, 0.18, 1.);
    rounded(x0, fy, w, 34., 12., planks);
    panel(x0, fy, w, 14., planks);
    for k in 0..16 {
        panel(
            x0 + 20. + k as f32 * 38.,
            fy + 2.,
            1.,
            32.,
            Color::new(0., 0., 0., 0.16),
        );
    }
    panel(x0, fy, w, 2., Color::new(1., 1., 1., 0.1));
    lights(x0 + 70., x1 - 14., y0 + 6., t);
    door(game, t);
    for slot in 0..3 {
        table(game, slot, t, p);
    }
}
/// A seated guest whose face shows the remaining patience `f`.
fn guest(x: f32, y: f32, number: usize, f: f32, t: f32) {
    let skin = SKIN[number % SKIN.len()];
    let shirt = SHIRT[number % SHIRT.len()];
    let hair = HAIR[number % HAIR.len()];
    let angry = f < 0.25;
    rounded_gradient(
        x - 15.,
        y + 9.,
        30.,
        36.,
        10.,
        lighten(shirt, 0.1),
        darken(shirt, 0.15),
    );
    let face = if angry {
        shapes::mix(skin, CORAL, 0.35 + 0.15 * pulse(t, 2.))
    } else {
        skin
    };
    disc(vec2(x, y), 11., face);
    if number % 4 == 3 {
        // A little hat.
        rounded(x - 12., y - 10., 24., 4., 2., darken(shirt, 0.3));
        rounded(x - 7., y - 17., 14., 9., 3., darken(shirt, 0.3));
    } else {
        shapes::fill_sector(vec2(x, y - 1.), 11.5, PI, PI, hair);
        if number.is_multiple_of(2) {
            disc(vec2(x + 8., y - 3.), 4.5, hair);
        }
    }
    let eye = DARK;
    dot(x - 4., y + 1., 1.4, eye);
    dot(x + 4., y + 1., 1.4, eye);
    let mouth = vec2(x, y + 4.);
    if f > 0.5 {
        shapes::stroke_arc(
            mouth,
            3.5,
            0.15 * PI,
            0.7 * PI,
            shapes::Stroke::new(1.4, eye).cap(shapes::Cap::Round),
        );
        dot(x - 6.5, y + 4., 1.8, Color::new(0.96, 0.5, 0.45, 0.45));
        dot(x + 6.5, y + 4., 1.8, Color::new(0.96, 0.5, 0.45, 0.45));
    } else if !angry {
        panel(x - 3., y + 5.5, 6., 1.4, eye);
    } else {
        shapes::stroke_arc(
            mouth + vec2(0., 4.),
            3.5,
            1.15 * PI,
            0.7 * PI,
            shapes::Stroke::new(1.4, eye).cap(shapes::Cap::Round),
        );
        draw_line(x - 7., y - 3., x - 2., y - 1.5, 1.4, eye);
        draw_line(x + 7., y - 3., x + 2., y - 1.5, 1.4, eye);
        steam(x, y - 14., 1., t * 1.6, Color::new(1., 1., 1., 0.7));
    }
}
fn candle(x: f32, y: f32, t: f32) {
    rounded(x - 2., y - 9., 4., 10., 1., CREAM);
    let flick = 1. + (t * 13.).sin() * 0.12 + (t * 23.).sin() * 0.08;
    glow(x, y - 12., 12. * flick, with_alpha(GOLD, 0.25));
    oval(x, y - 12., 2. * flick, 3.4 * flick, GOLD);
    oval(x, y - 11.5, 1., 1.6, CREAM);
}
fn table(game: &Game, slot: usize, t: f32, p: Vec2) {
    let x = TABLE_X + slot as f32 * TABLE_STEP;
    let tx = x + 62.;
    let order = game.orders[slot];
    if let Some(order) = order {
        let f = order.patience as f32 / game.content().patience as f32;
        rounded(tx - 20., 150., 40., 48., 10., CHAIR);
        rounded(tx - 15., 155., 30., 38., 8., lighten(CHAIR, 0.08));
        let breathe = bob(t + slot as f32 * 0.9, 0.35, 1.);
        guest(tx, 158. + breathe, order.number, f, t);
    } else {
        rounded(tx - 47., 172., 13., 30., 5., CHAIR);
        rounded(tx + 34., 172., 13., 30., 5., CHAIR);
    }
    oval(tx, 216., 40., 5., Color::new(0., 0., 0., 0.28));
    panel(tx - 3., 202., 6., 14., darken(WOOD, 0.4));
    oval(tx, 202., 38., 10., darken(CREAM, 0.2));
    oval(tx, 199., 37., 9., CREAM);
    candle(tx + 24., 198., t + slot as f32 * 1.7);
    match order {
        Some(order) => bubble(game, slot, order, t, p),
        None => {
            oval(tx - 6., 199., 9., 4., darken(CREAM, 0.12));
            oval(tx - 6., 198.5, 6., 2.6, CREAM);
        }
    }
}
/// A guest's order: the dish in a speech bubble wrapped by a patience ring,
/// with the ingredients it needs underneath.
fn bubble(game: &Game, slot: usize, order: Order, t: f32, p: Vec2) {
    let f = order.patience as f32 / game.content().patience as f32;
    let urgent = f < 0.25;
    let hover = table_at(p) == Some(slot);
    let mut c = bubble_center(slot);
    if urgent {
        c.x += (t * 26.).sin() * 1.3;
    }
    if hover {
        c.y -= 2.;
    }
    let head = vec2(TABLE_X + slot as f32 * TABLE_STEP + 62., 158.);
    shapes::fill_triangle(
        vec2(c.x - 21., c.y - 5.),
        vec2(c.x - 18., c.y + 9.),
        head + vec2(16., 4.),
        CREAM,
    );
    if hover {
        glow(c.x, c.y, 44., with_alpha(CREAM, 0.16));
    }
    disc(c, 22., CREAM);
    let color = if f > 0.5 {
        GREEN
    } else if !urgent {
        GOLD
    } else {
        lighten(CORAL, 0.25 * pulse(t, 2.5))
    };
    ring_meter(c.x, c.y, 26., 4., f, color);
    if game.book == order.recipe {
        ring(c, 31., 1.5, with_alpha(GOLD, 0.9));
    } else if hover {
        ring(c, 31., 1.5, with_alpha(CREAM, 0.7));
    }
    dish(order.recipe, c.x, c.y + 2., 0.9, true);
    let base = bubble_center(slot);
    let n = RECIPES[order.recipe].mask.count_ones() as f32;
    let w = n * 16. + 6.;
    rounded(
        base.x - w * 0.5,
        191.,
        w,
        19.,
        9.5,
        Color::new(0., 0., 0., 0.3),
    );
    ingredient_row(order.recipe, base.x, 201., 0.55, 16.);
}
/// The front door: an arrival ring fills with the next guest's dish.
fn door(game: &Game, t: f32) {
    let (x, y) = (34., 130.);
    let (w, h) = (48., DINING_BOTTOM - 130.);
    rounded(
        x - 4.,
        y - 4.,
        w + 8.,
        h + 4.,
        7.,
        Color::new(0.30, 0.18, 0.11, 1.),
    );
    let menu = game.content().menu;
    if game.next_order >= menu.len() {
        rounded_gradient(x, y, w, h, 4., lighten(WOOD, 0.05), darken(WOOD, 0.22));
        outline(x + 7., y + 8., w - 14., 32., 3., 1.5, darken(WOOD, 0.35));
        outline(x + 7., y + 48., w - 14., 32., 3., 1.5, darken(WOOD, 0.35));
        dot(x + w - 8., y + 46., 2.5, GOLD);
        return;
    }
    rounded_gradient(
        x,
        y,
        w,
        h,
        4.,
        Color::new(0.99, 0.80, 0.52, 1.),
        Color::new(0.80, 0.46, 0.30, 1.),
    );
    let interval = game.content().interval;
    let due = game.next_order as u32 * interval;
    let waiting = game.ticks >= due;
    if waiting {
        // The next guest waits in the doorway for a free table.
        let sway = bob(t, 0.8, 1.2);
        let ink = Color::new(0.20, 0.12, 0.10, 0.85);
        disc(vec2(DOOR.x + sway, 192.), 8., ink);
        rounded(DOOR.x - 11. + sway, 200., 22., 22., 8., ink);
    }
    let f = if waiting {
        1.
    } else {
        1. - (due - game.ticks) as f32 / interval as f32
    };
    disc(DOOR, 17., Color::new(0.10, 0.07, 0.07, 0.85));
    let color = if waiting {
        lighten(GOLD, 0.3 * pulse(t, 1.4))
    } else {
        CREAM
    };
    ring_meter(DOOR.x, DOOR.y, 19., 4., f, color);
    dish(menu[game.next_order], DOOR.x, DOOR.y + 1., 0.58, true);
}

// ------------------------------------------------------------------ kitchen

/// Stations that take the next step with what the cook holds (a gentle,
/// read-only hint; the player is free to do anything else).
fn goals(game: &Game) -> [bool; 11] {
    let mut out = [false; 11];
    let mut mark = |s: Station| out[station_index(s)] = true;
    match game.carry {
        Carry::Raw(_) => mark(Station::Chop),
        Carry::Chopped(i) if game.mixing & (1 << i) == 0 => mark(Station::Mix),
        Carry::Chopped(_) => mark(Station::Bin),
        Carry::Mixed(r) if RECIPES[r].cook == 0 => mark(Station::Plate),
        Carry::Mixed(_) => {
            for b in 0..2 {
                if game.burners[b].is_none() {
                    mark(Station::Burner(b));
                }
            }
        }
        Carry::Cooked(_) => mark(Station::Plate),
        Carry::Plated(r) if game.payout(r, 0).is_some() => mark(Station::Serve),
        Carry::Plated(_) => mark(Station::Bin),
        Carry::Empty => {
            let mut pots = false;
            for b in 0..2 {
                if game.burners[b].is_some_and(|p| game.pot_ready(p) || game.pot_burnt(p)) {
                    mark(Station::Burner(b));
                    pots = true;
                }
            }
            if !pots {
                let want = RECIPES[game.book].mask;
                if recipe_for(game.mixing).is_some() {
                    mark(Station::Mix);
                } else if game.mixing & !want == 0 && game.cash >= 2 {
                    for i in 0..4 {
                        if want & !game.mixing & (1 << i) != 0 {
                            mark(Station::Pantry(i));
                        }
                    }
                }
            }
        }
    }
    out
}
fn room() {
    let (x0, y0) = (GRID_X, GRID_Y);
    let (w, h) = (11. * TILE, 6. * TILE);
    rounded(x0, y0 + 4., w, h, 12., SHADE);
    rounded_gradient(x0, y0, w, h, 12., lighten(TEAL, 0.07), darken(TEAL, 0.12));
    let seam = Color::new(0., 0., 0., 0.16);
    for k in 1..11 {
        let x = x0 + k as f32 * TILE;
        panel(x, y0 + 4., 1., TILE - 4., seam);
        panel(x, y0 + 5. * TILE, 1., TILE - 4., seam);
    }
    for k in 2..5 {
        let y = y0 + k as f32 * TILE;
        panel(x0 + 4., y, TILE - 4., 1., seam);
        panel(x0 + 10. * TILE, y, TILE - 4., 1., seam);
    }
    let (fx, fy) = (x0 + TILE, y0 + TILE);
    panel(
        fx,
        fy,
        9. * TILE,
        4. * TILE,
        Color::new(0.80, 0.82, 0.72, 1.),
    );
    for y in 0..4 {
        for x in 0..9 {
            if (x + y) % 2 == 1 {
                panel(
                    fx + x as f32 * TILE,
                    fy + y as f32 * TILE,
                    TILE,
                    TILE,
                    Color::new(0.72, 0.76, 0.66, 1.),
                );
            }
        }
    }
    panel(fx, fy, 9. * TILE, 7., Color::new(0.03, 0.12, 0.13, 0.16));
    panel(fx, fy, 6., 4. * TILE, Color::new(0.03, 0.12, 0.13, 0.1));
    panel(
        fx,
        fy + 4. * TILE - 2.,
        9. * TILE,
        2.,
        Color::new(1., 1., 1., 0.25),
    );
    panel(
        fx - 2.,
        fy - 2.,
        9. * TILE + 4.,
        2.,
        Color::new(1., 1., 1., 0.12),
    );
}
/// Dots along the cook's walking route to `to` (horizontal first, as the
/// cook walks), with the destination tile outlined.
fn route(from: Cell, to: Cell, color: Color) {
    let mut at = from;
    for _ in 0..16 {
        if at == to {
            break;
        }
        at = if at.x != to.x {
            Cell::new(at.x + (to.x - at.x).signum(), at.y)
        } else {
            Cell::new(at.x, at.y + (to.y - at.y).signum())
        };
        let c = cell_center(at);
        if at != to {
            dot(c.x, c.y, 3., color);
        }
    }
    let c = cell_center(to);
    rounded(
        c.x - 24.,
        c.y - 24.,
        48.,
        48.,
        8.,
        with_alpha(color, color.a * 0.18),
    );
    outline(c.x - 24., c.y - 24., 48., 48., 8., 1.5, color);
}
fn kitchen(game: &Game, t: f32, p: Vec2) {
    room();
    let hover_cell = cell_at(p);
    let hover_station = station_on(hover_cell);
    let aim = if walkable(hover_cell) {
        Some(hover_cell)
    } else {
        hover_station.map(access)
    };
    if let Some(dest) = game.destination {
        route(
            game.position,
            dest,
            with_alpha(GOLD, 0.6 + 0.3 * pulse(t, 1.5)),
        );
    }
    if let Some(cell) = aim.filter(|c| *c != game.position && Some(*c) != game.destination) {
        route(game.position, cell, Color::new(1., 1., 1., 0.55));
    }
    let goals = goals(game);
    let beat = pulse(t, 1.2);
    for (k, &(station, cell, spot)) in STATIONS.iter().enumerate() {
        let x = GRID_X + cell.x as f32 * TILE;
        let y = GRID_Y + cell.y as f32 * TILE;
        let c = vec2(x + 26., y + 26.);
        let a = cell_center(spot);
        let here = game.position == spot;
        let hovered = hover_station == Some(station) || hover_cell == spot;
        let goal = goals[k] && !here;
        let mark = if here {
            GOLD
        } else if goal {
            with_alpha(GOLD, 0.4 + 0.5 * beat)
        } else if hovered {
            with_alpha(CREAM, 0.9)
        } else {
            Color::new(0.13, 0.40, 0.42, 0.38)
        };
        marker(a.x, a.y, mark);
        if here {
            rounded(x + 1., y + 1., 50., 50., 9., with_alpha(GOLD, 0.16));
            outline(x + 1., y + 1., 50., 50., 9., 2., GOLD);
        } else if hovered {
            rounded(x + 1., y + 1., 50., 50., 9., with_alpha(CREAM, 0.12));
            outline(x + 1., y + 1., 50., 50., 9., 1.5, with_alpha(CREAM, 0.75));
        } else if goal {
            glow(c.x, c.y, 34., with_alpha(GOLD, 0.08 + 0.12 * beat));
            outline(
                x + 1.,
                y + 1.,
                50.,
                50.,
                9.,
                1.5,
                with_alpha(GOLD, 0.25 + 0.45 * beat),
            );
        }
        station_art(game, station, c, here, t);
    }
    cook(game, t);
}
fn station_art(game: &Game, station: Station, c: Vec2, here: bool, t: f32) {
    match station {
        Station::Pantry(i) => {
            rounded(c.x - 20., c.y - 13., 40., 32., 5., SHADE);
            rounded_gradient(
                c.x - 20.,
                c.y - 16.,
                40.,
                32.,
                5.,
                lighten(WOOD, 0.14),
                darken(WOOD, 0.08),
            );
            panel(
                c.x - 20.,
                c.y - 5.,
                40.,
                1.,
                Color::new(0.2, 0.1, 0.05, 0.35),
            );
            panel(
                c.x - 20.,
                c.y + 6.,
                40.,
                1.,
                Color::new(0.2, 0.1, 0.05, 0.35),
            );
            ingredient(i, c.x, c.y - 1., 1.05, false);
            if game.cash < 2 {
                rounded(
                    c.x - 20.,
                    c.y - 16.,
                    40.,
                    32.,
                    5.,
                    Color::new(0.02, 0.05, 0.06, 0.6),
                );
            }
        }
        Station::Chop => {
            board_glyph(c.x, c.y + 2., 1.);
            let chopping = game.chopped_ticks > 0;
            let f = game.chopped_ticks as f32 / game.chop_ticks() as f32;
            if let (true, Carry::Raw(i)) = (here, game.carry) {
                ingredient(i, c.x - 6., c.y + 4., 0.62, f > 0.5);
            }
            let lift = if chopping {
                (t * 14.).sin().abs() * 6.
            } else {
                0.
            };
            knife_glyph(c.x + 1., c.y - 5. - lift, 0.95);
            if chopping {
                ring_meter(c.x, c.y, 24., 4., f, GREEN);
            }
        }
        Station::Mix => {
            let valid = recipe_for(game.mixing).is_some();
            if valid {
                glow(
                    c.x,
                    c.y,
                    30.,
                    with_alpha(GREEN, 0.18 + 0.14 * pulse(t, 1.3)),
                );
            }
            bowl_glyph(c.x, c.y + 2., 1.);
            for i in 0..4 {
                if game.mixing & (1 << i) != 0 {
                    ingredient(i, c.x + (i as f32 - 1.5) * 7., c.y - 1., 0.6, true);
                }
            }
            if valid {
                icon_badge(Icon::Check, c.x + 17., c.y - 16., 15., GREEN, PAPER);
            }
        }
        Station::Burner(i) => burner_art(game, i, c, t),
        Station::Plate => plate_glyph(c.x, c.y + 1., 1.),
        Station::Serve => {
            let (x, y) = (c.x - 26., c.y - 26.);
            rounded(x + 6., y + 6., 40., 34., 5., DARK);
            for k in 0..5 {
                panel(
                    x + 6. + k as f32 * 8.,
                    y + 6.,
                    8.,
                    7.,
                    if k % 2 == 0 { CORAL } else { CREAM },
                );
                dot(
                    x + 10. + k as f32 * 8.,
                    y + 13.,
                    4.,
                    if k % 2 == 0 { CORAL } else { CREAM },
                );
            }
            rounded(x + 4., y + 38., 44., 5., 2., WOOD);
            bell_glyph(c.x, c.y + 7., 1.);
        }
        Station::Bin => bin_glyph(c.x, c.y + 3., 1.),
    }
}
fn burner_art(game: &Game, i: usize, c: Vec2, t: f32) {
    let b = vec2(c.x, c.y + 1.);
    burner_glyph(b.x, b.y, 1.);
    let Some(pot) = game.burners[i] else {
        return;
    };
    let burnt = game.pot_burnt(pot);
    let ready = game.pot_ready(pot);
    let cook = RECIPES[pot.recipe].cook as f32;
    if burnt {
        glow(b.x, b.y, 30., with_alpha(CORAL, 0.18 + 0.1 * pulse(t, 0.8)));
    } else {
        flames(b.x, b.y, 1., t);
    }
    let mut warn = false;
    if ready {
        let left = 1. - (pot.ticks as f32 - cook) / game.burn_grace() as f32;
        warn = left < 0.4;
        let color = if warn {
            with_alpha(CORAL, 0.55 + 0.45 * pulse(t, 3.))
        } else {
            GREEN
        };
        let beat = if warn { pulse(t, 3.) } else { pulse(t, 1.4) };
        let strength = if warn {
            0.18 + 0.3 * beat
        } else {
            0.12 + 0.14 * beat
        };
        glow(
            b.x,
            b.y,
            if warn { 38. } else { 32. },
            with_alpha(color, strength),
        );
        ring_meter(b.x, b.y, 24., 4., left, color);
    } else if !burnt {
        ring_meter(
            b.x,
            b.y,
            24.,
            4.,
            pot.ticks as f32 / cook,
            lighten(FLAME, 0.2),
        );
    }
    let (body, rim) = if burnt {
        (SOOT, Color::new(0.35, 0.33, 0.32, 1.))
    } else {
        (POT, CREAM)
    };
    pot_glyph(b.x, b.y - 2., 1., body, rim);
    if burnt {
        steam(
            b.x,
            b.y - 14.,
            2.2,
            t * 0.6,
            Color::new(0.62, 0.6, 0.58, 0.9),
        );
        icon_badge(Icon::Cross, b.x + 16., b.y - 17., 15., CORAL, PAPER);
    } else {
        oval(
            b.x,
            b.y - 12.,
            11.,
            2.,
            if pot.recipe == 0 { GREEN } else { GOLD },
        );
        if ready {
            steam(b.x, b.y - 16., 1.2, t, Color::new(1., 1., 1., 0.85));
            let lift = bob(t, 1.2, 1.5);
            if warn {
                icon_badge(Icon::Alert, b.x + 16., b.y - 17. + lift, 15., CORAL, PAPER);
            } else {
                icon_badge(Icon::Check, b.x + 16., b.y - 17. + lift, 15., GREEN, PAPER);
            }
        }
    }
}
/// What the action key would do where the cook stands.
#[derive(Clone, Copy)]
enum Hint {
    Buy,
    Hold,
    Add,
    Lift(usize),
    Cook(u32),
    Take,
    Clean,
    Plate,
    Serve(u32),
    Bin,
}
fn hint(game: &Game) -> Option<Hint> {
    if game.move_wait > 0 || game.chopped_ticks > 0 {
        return None;
    }
    let station = game.station()?;
    match (station, game.carry) {
        (Station::Pantry(_), Carry::Empty) if game.cash >= 2 => Some(Hint::Buy),
        (Station::Chop, Carry::Raw(_)) => Some(Hint::Hold),
        (Station::Mix, Carry::Chopped(i)) if game.mixing & (1 << i) == 0 => Some(Hint::Add),
        (Station::Mix, Carry::Empty) => recipe_for(game.mixing).map(Hint::Lift),
        (Station::Burner(b), Carry::Mixed(r))
            if RECIPES[r].cook > 0 && game.burners[b].is_none() =>
        {
            Some(Hint::Cook(RECIPES[r].cook))
        }
        (Station::Burner(b), Carry::Empty) => match game.burners[b] {
            Some(pot) if game.pot_ready(pot) => Some(Hint::Take),
            Some(pot) if game.pot_burnt(pot) => Some(Hint::Clean),
            _ => None,
        },
        (Station::Plate, Carry::Cooked(_)) => Some(Hint::Plate),
        (Station::Plate, Carry::Mixed(r)) if RECIPES[r].cook == 0 => Some(Hint::Plate),
        // The tip is previewed as the next update will pay it.
        (Station::Serve, Carry::Plated(r)) => game.payout(r, 1).map(|(_, pay)| Hint::Serve(pay)),
        (Station::Bin, carry) if carry != Carry::Empty => Some(Hint::Bin),
        _ => None,
    }
}
/// A compact "key + outcome" tag beside the cook. `align` places the tag's
/// left edge (0), center (0.5) or right edge (1) at `anchor`.
fn prompt(hint: Hint, anchor: f32, cy: f32, align: f32) {
    let key = if matches!(hint, Hint::Clean) {
        "X"
    } else {
        "Space"
    };
    let (glyph_w, text): (f32, Option<String>) = match hint {
        Hint::Buy => (16., Some("-2".into())),
        Hint::Hold => (0., Some("Hold".into())),
        Hint::Cook(ticks) => (16., Some(secs(ticks))),
        Hint::Serve(pay) => (16., Some(format!("+{pay}"))),
        Hint::Clean => (0., Some("Clean".into())),
        _ => (20., None),
    };
    let s = flat(13., CREAM);
    let tw = text.as_deref().map_or(0., |v| text_width(v, s) + 4.);
    let kw = text_width(key, flat(13., PAPER)) + 12.;
    let w = 6. + kw + 6. + glyph_w + tw + 6.;
    let x = (anchor - w * align).clamp(GRID_X + 4., KITCHEN_RIGHT - 4. - w);
    rounded(x, cy - 14., w, 28., 10., Color::new(0.03, 0.08, 0.1, 0.82));
    keycap(key, "", x + 6., cy - 11., PAPER);
    let gx = x + 6. + kw + 6.;
    let gc = gx + glyph_w * 0.5;
    match hint {
        Hint::Buy | Hint::Serve(_) => icon(Icon::Coin, gc, cy, 15., GOLD),
        Hint::Cook(_) => icon(Icon::Clock, gc, cy, 14., GOLD),
        Hint::Add => bowl_glyph(gc, cy + 1., 0.5),
        Hint::Lift(r) => dish(r, gc, cy, 0.55, false),
        Hint::Take => icon(Icon::Hand, gc, cy, 15., CREAM),
        Hint::Plate => plate_glyph(gc, cy, 0.5),
        Hint::Bin => bin_glyph(gc, cy + 1., 0.55),
        Hint::Hold | Hint::Clean => {}
    }
    if let Some(v) = text {
        let color = match hint {
            Hint::Buy => darken(GOLD, 0.1),
            Hint::Serve(_) => GOLD,
            _ => CREAM,
        };
        draw_text_styled(
            &v,
            gx + glyph_w + 2.,
            s.centered_baseline(cy),
            flat(13., color),
        );
    }
}
/// The cook's hand slot: what they carry, in a bubble above their head.
fn hand_bubble(carry: Carry, c: Vec2, t: f32) {
    shapes::fill_triangle(
        vec2(c.x - 5., c.y + 13.),
        vec2(c.x + 5., c.y + 13.),
        vec2(c.x, c.y + 21.),
        CREAM,
    );
    disc(c + vec2(0., 2.), 17., Color::new(0., 0., 0., 0.25));
    disc(c, 17., CREAM);
    ring(c, 17., 1.5, darken(CREAM, 0.25));
    match carry {
        Carry::Raw(i) => ingredient(i, c.x, c.y + 1., 0.9, false),
        Carry::Chopped(i) => ingredient(i, c.x, c.y + 1., 0.9, true),
        Carry::Mixed(r) => dish(r, c.x, c.y + 1., 0.66, false),
        Carry::Cooked(r) => {
            pot_glyph(c.x, c.y + 6., 0.55, POT, CREAM);
            oval(c.x, c.y - 0.5, 6., 1.4, if r == 0 { GREEN } else { GOLD });
            steam(c.x, c.y - 4., 0.7, t, Color::new(0.55, 0.62, 0.66, 0.9));
        }
        Carry::Plated(r) => dish(r, c.x, c.y + 1., 0.66, true),
        Carry::Empty => {}
    }
}
fn chef(x: f32, y: f32) {
    rounded(x - 7., y + 12., 5., 6., 2., DARK);
    rounded(x + 2., y + 12., 5., 6., 2., DARK);
    rounded_gradient(
        x - 10.,
        y - 3.,
        20.,
        18.,
        6.,
        lighten(CORAL, 0.12),
        darken(CORAL, 0.1),
    );
    rounded(x - 6., y + 3., 12., 11., 3., CREAM);
    dot(x, y - 6., 8.5, Color::new(0.93, 0.68, 0.47, 1.));
    rounded(x - 10., y - 16., 20., 8., 3., CREAM);
    for dx in [-6., 0., 6.] {
        dot(x + dx, y - 18., 6., CREAM);
    }
    dot(x - 3., y - 5., 1.4, DARK);
    dot(x + 3., y - 5., 1.4, DARK);
    dot(x - 5., y - 2.5, 1.6, Color::new(0.96, 0.5, 0.45, 0.5));
    dot(x + 5., y - 2.5, 1.6, Color::new(0.96, 0.5, 0.45, 0.5));
}
fn cook(game: &Game, t: f32) {
    let at = cook_center(game);
    let lift = if game.move_wait > 0 {
        -(walk_blend(game) * PI).sin().abs() * 3.
    } else {
        bob(t, 0.6, 0.7)
    };
    oval(at.x, at.y + 16., 14., 5., Color::new(0.1, 0.2, 0.2, 0.3));
    chef(at.x, at.y + lift);
    // While preparing, the food sits on the board instead of the hand slot.
    let carrying = game.carry != Carry::Empty && game.chopped_ticks == 0;
    let head = vec2(at.x, at.y + lift - 46.);
    if carrying {
        hand_bubble(game.carry, head, t);
    }
    if let Some(h) = hint(game) {
        if !carrying {
            prompt(h, at.x, at.y - 40., 0.5);
        } else if head.x + 21. + 96. < KITCHEN_RIGHT {
            prompt(h, head.x + 21., head.y, 0.);
        } else {
            prompt(h, head.x - 21., head.y, 1.);
        }
    }
}

// -------------------------------------------------------------- side panel

/// The prepared-food route for a recipe: board, bowl, (pot), plate, bell.
fn chain(recipe: usize, x: f32, cy: f32, t: f32) {
    let cooks = RECIPES[recipe].cook > 0;
    let n = if cooks { 5 } else { 4 };
    let (tile, gap) = (36., 13.);
    let mut tx = x;
    for k in 0..n {
        let step = if !cooks && k >= 2 { k + 1 } else { k };
        rounded(tx, cy - 18., tile, tile, 9., lighten(PANEL, 0.07));
        let c = vec2(tx + tile * 0.5, cy);
        match step {
            0 => {
                board_glyph(c.x, c.y + 3., 0.55);
                knife_glyph(c.x + 1., c.y - 2., 0.55);
            }
            1 => bowl_glyph(c.x, c.y + 2., 0.62),
            2 => {
                burner_glyph(c.x, c.y + 3., 0.55);
                flames(c.x, c.y + 3., 0.55, t);
                pot_glyph(c.x, c.y + 1., 0.5, POT, CREAM);
                let time = secs(RECIPES[recipe].cook);
                let s = flat(11., PAPER).with_align(TextAlign::Center);
                let w = text_width(&time, s) + 8.;
                rounded(c.x - w * 0.5, cy + 12., w, 13., 6.5, darken(GOLD, 0.25));
                draw_text_styled(&time, c.x, s.centered_baseline(cy + 18.5), s);
            }
            3 => plate_glyph(c.x, c.y + 1., 0.62),
            _ => bell_glyph(c.x, c.y + 1., 1.1),
        }
        tx += tile;
        if k + 1 < n {
            chevron(tx + gap * 0.5, cy, 0.8, with_alpha(SAGE, 0.7));
            tx += gap;
        }
    }
}
fn sidebar(game: &Game, t: f32, p: Vec2) {
    let (l, r) = (668., 914.);
    card(650., DINING_TOP, 282., 424., PANEL);
    rounded(668., DINING_TOP, 56., 3., 1.5, ACCENT);
    // Recipe pages: the C key, then one thumbnail per recipe.
    keycap("C", "", l, THUMB_Y - 11., PAPER);
    let menu = game.content().menu;
    for i in 0..RECIPES.len() {
        let selected = game.book == i;
        let hovered = thumb_at(p) == Some(i);
        let c = thumb_center(i) - vec2(0., if selected || hovered { 2. } else { 0. });
        if selected {
            glow(c.x, c.y, 26., with_alpha(GOLD, 0.2));
        }
        disc(c, 15., lighten(PANEL, if selected { 0.14 } else { 0.06 }));
        dish(i, c.x, c.y + 1., 0.52, true);
        if !menu.contains(&i) {
            disc(c, 15.5, with_alpha(PANEL, 0.62));
        }
        if selected {
            ring(c, 15., 2., GOLD);
        } else if hovered {
            ring(c, 15., 1.5, with_alpha(CREAM, 0.8));
        }
    }
    // The selected recipe: name, value, dish, ingredients and route.
    let recipe = RECIPES[game.book];
    fit(recipe.name, l, 204., 176., 20., CREAM);
    let value = format!("{}", recipe.value);
    let vw = 16. * 1.25 + measure(&value, 16.);
    icon_value(Icon::Coin, &value, r - vw, 198., 16., GOLD);
    glow(l + 34., 246., 40., with_alpha(GOLD, 0.08));
    dish(game.book, l + 34., 248., 1.5, true);
    let step = if recipe.mask.count_ones() > 3 {
        38.
    } else {
        46.
    };
    let mut x = l + 98.;
    let mut first = true;
    for i in 0..4 {
        if recipe.mask & (1 << i) != 0 {
            if !first {
                let c = vec2(x - step * 0.5, 247.);
                panel(c.x - 4., c.y - 1., 8., 2., SAGE);
                panel(c.x - 1., c.y - 4., 2., 8., SAGE);
            }
            ingredient(i, x, 248., 0.95, false);
            x += step;
            first = false;
        }
    }
    chain(game.book, l, 304., t);
    divider(l, 340., r - l, Color::new(1., 1., 1., 0.12));
    // Purse, retries and waste.
    let cash = format!("{}", game.cash);
    icon_value(Icon::Coin, &cash, l, 372., 26., GOLD);
    let rx = r - 76.;
    icon(Icon::Restart, rx, 372., 20., SAGE);
    for k in 0..3u8 {
        let c = vec2(rx + 20. + f32::from(k) * 15., 372.);
        if k < game.retries {
            disc(c, 4.5, CREAM);
        } else {
            ring(c, 4., 1.5, with_alpha(SAGE, 0.45));
        }
    }
    let waste_color = if game.waste > 0 { CORAL } else { SAGE };
    bin_glyph(l + 13., 420., 0.72);
    let waste = format!("{}", game.waste);
    draw_text_styled(
        &waste,
        l + 34.,
        flat(22., waste_color).centered_baseline(421.),
        style(22., waste_color),
    );
    divider(l, 452., r - l, Color::new(1., 1., 1., 0.12));
    // Equipment levels.
    for kind in 0..3 {
        let cx = l + 38. + kind as f32 * 86.;
        upgrade_glyph(kind, cx, 482., 1., t);
        for k in 0..2u8 {
            let lit = k < game.upgrades[kind];
            rounded(
                cx - 13. + f32::from(k) * 15.,
                506.,
                11.,
                5.,
                2.5,
                if lit {
                    GOLD
                } else {
                    Color::new(1., 1., 1., 0.14)
                },
            );
        }
    }
}

// ----------------------------------------------------------------- header

/// The service's guests as tokens (served, seated, still to come, and
/// missed from the right), then a heart per miss the quota allows.
fn guests(game: &Game, x: f32, cy: f32, size: f32) -> f32 {
    let shift = game.content();
    let len = shift.menu.len();
    let spare = len - usize::from(shift.quota);
    let served = usize::from(game.served);
    let missed = usize::from(game.missed);
    let seated = game.orders.iter().flatten().count();
    let r = size * 0.5;
    let gap = size * 1.28;
    for k in 0..len {
        let c = vec2(x + r + k as f32 * gap, cy);
        if k + missed >= len {
            disc(c, r, CORAL);
            icon(Icon::Cross, c.x, c.y, size * 0.6, PAPER);
        } else if k < served {
            disc(c, r, GREEN);
            icon(Icon::Check, c.x, c.y, size * 0.62, PAPER);
        } else if k < served + seated {
            ring(c, r - 1., 2., GOLD);
            icon(Icon::Person, c.x, c.y, size * 0.62, GOLD);
        } else {
            ring(c, r - 1., 1.5, with_alpha(SAGE, 0.55));
        }
    }
    let mut w = len as f32 * gap + size * 0.3;
    for h in 0..spare {
        let c = vec2(x + w + r, cy);
        if h < spare.saturating_sub(missed) {
            icon(Icon::Heart, c.x, c.y, size * 1.05, CORAL);
        } else {
            icon(Icon::Heart, c.x, c.y, size * 1.05, with_alpha(SAGE, 0.3));
        }
        w += gap;
    }
    w
}
/// The closing clock: a draining ring and `m:ss`.
fn closing(game: &Game, x: f32, cy: f32, size: f32, t: f32) -> f32 {
    let left = game.deadline().saturating_sub(game.ticks);
    let urgent = game.phase == Phase::Service && left <= 20 * 60;
    let color = if urgent {
        lighten(CORAL, 0.3 * pulse(t, 2.))
    } else {
        CREAM
    };
    let radius = size * 0.5;
    ring_meter(
        x + radius,
        cy,
        radius,
        size * 0.18,
        left as f32 / game.deadline().max(1) as f32,
        if urgent { CORAL } else { GOLD },
    );
    icon(Icon::Clock, x + radius, cy, size * 0.6, color);
    let text = clock_text(left);
    let s = style(size * 0.95, color);
    let w = draw_text_styled(&text, x + size + 7., s.centered_baseline(cy), s).width;
    size + 7. + w
}
fn objective(game: &Game, area: Rect) {
    let t = clock();
    let cy = area.y + area.h * 0.5;
    let x = area.x + 6.;
    let w = guests(game, x, cy, 18.);
    closing(game, x + w + 22., cy, 22., t);
}

// ------------------------------------------------------------- menu cards

fn button(x: f32, y: f32, w: f32, h: f32, action: &str, hover: bool) {
    let lift = if hover { 2. } else { 0. };
    let y = y - lift;
    rounded(x, y + 3. + lift, w, h, 12., Color::new(0., 0., 0., 0.35));
    let top = lighten(ACCENT, if hover { 0.22 } else { 0.12 });
    rounded_gradient(x, y, w, h, 12., top, ACCENT);
    panel(x + 12., y + 1., w - 24., 1., Color::new(1., 1., 1., 0.35));
    let s = flat(19., INK);
    let kw = (text_width("Space", flat(13., PAPER)) + 12.).max(20.);
    let tw = text_width(action, s);
    let left = x + (w - kw - 12. - tw) * 0.5;
    keycap("Space", "", left, y + h * 0.5 - 10., PAPER);
    draw_text_styled(action, left + kw + 12., s.centered_baseline(y + h * 0.5), s);
}
fn menu(game: &Game, t: f32, p: Vec2) {
    card(34., 124., 892., 428., PANEL);
    lights(60., 900., 130., t);
    let hover = menu_start(p);
    match game.phase {
        Phase::Briefing => {
            briefing(game, t);
            button(290., 484., 380., 52., "Open", hover);
        }
        Phase::Debrief => {
            debrief(game, t, p);
            button(290., 484., 380., 52., "Next", hover);
        }
        _ => {
            setback(game, t);
            button(290., 484., 380., 52., "Retry", hover);
        }
    }
}
/// True when `recipe` first appears on the menu of service `shift`.
fn new_dish(shift: usize, recipe: usize) -> bool {
    !SHIFTS[..shift].iter().any(|s| s.menu.contains(&recipe))
}
fn briefing(game: &Game, t: f32) {
    let shift = game.content();
    fit(shift.name, 66., 202., 820., 32., CREAM);
    let w = guests(game, 68., 248., 24.);
    closing(game, 68. + w + 30., 248., 28., t);
    // Tonight's menu: each dish once, with how many guests will order it.
    let mut seen = [0usize; 6];
    let mut order = [0usize; 6];
    let mut kinds = 0;
    for &r in shift.menu {
        if seen[r] == 0 {
            order[kinds] = r;
            kinds += 1;
        }
        seen[r] += 1;
    }
    let (tw, th, gap) = (268., 88., 14.);
    let rows = kinds.div_ceil(3);
    for (k, &recipe) in order[..kinds].iter().enumerate() {
        let row = k / 3;
        let in_row = (kinds - row * 3).min(3) as f32;
        let left = 480. - (in_row * tw + (in_row - 1.) * gap) * 0.5;
        let x = left + (k % 3) as f32 * (tw + gap);
        let y = if rows == 1 {
            300.
        } else {
            280. + row as f32 * 96.
        };
        let fresh = new_dish(game.shift_index, recipe);
        rounded(x, y, tw, th, 14., Color::new(1., 1., 1., 0.05));
        if fresh {
            outline(
                x,
                y,
                tw,
                th,
                14.,
                1.5,
                with_alpha(GOLD, 0.45 + 0.35 * pulse(t, 0.7)),
            );
        }
        glow(x + 44., y + 44., 38., with_alpha(GOLD, 0.07));
        dish(recipe, x + 44., y + 46., 1.4, true);
        label(
            RECIPES[recipe].name,
            x + 86.,
            y + 33.,
            120.,
            17.,
            CREAM,
            TextAlign::Left,
        );
        let mut ix = x + 94.;
        for i in 0..4 {
            if RECIPES[recipe].mask & (1 << i) != 0 {
                ingredient(i, ix, y + 62., 0.72, false);
                ix += 21.;
            }
        }
        if RECIPES[recipe].cook > 0 {
            chevron(ix - 2., y + 62., 0.8, with_alpha(SAGE, 0.7));
            pot_glyph(ix + 16., y + 64., 0.5, POT, CREAM);
            let time = secs(RECIPES[recipe].cook);
            label(&time, ix + 30., y + 67., 44., 13., GOLD, TextAlign::Left);
        }
        let count = format!("x{}", seen[recipe]);
        let cw = measure(&count, 13.) + 17.;
        chip(&count, x + tw - 12. - cw, y + 12., 13., GOLD);
        if fresh {
            let nw = measure("New", 12.) + 16.;
            rounded(x + 10., y - 9., nw, 17.4, 8.7, darken(PANEL, 0.1));
            chip("New", x + 10., y - 9., 12., GREEN);
        }
    }
}
fn stars(x: f32, cy: f32, earned: u8, size: f32, t: f32) {
    for i in 0..3u8 {
        let c = vec2(x + f32::from(i) * size * 1.15, cy);
        if i < earned {
            let lift = bob(t + f32::from(i) * 0.3, 0.7, 1.5);
            glow(c.x, c.y + lift, size * 0.9, with_alpha(GOLD, 0.16));
            icon(Icon::Star, c.x, c.y + lift, size, GOLD);
        } else {
            icon(Icon::Star, c.x, c.y, size, Color::new(1., 1., 1., 0.14));
        }
    }
}
const KNIFE_TIME: [&str; 3] = ["1.5s", "1.0s", "0.7s"];
const SHOE_SPEED: [&str; 3] = ["6/s", "7.5/s", "10/s"];
const BURN_GRACE: [&str; 3] = ["5s", "8s", "11s"];
fn debrief(game: &Game, t: f32, p: Vec2) {
    let shift = game.content();
    fit(shift.name, 66., 202., 560., 32., CREAM);
    stars(778., 192., game.medals[game.shift_index], 34., t);
    let mut x = 68.;
    x += guests(game, x, 262., 24.) + 34.;
    let cash = format!("{}", game.cash);
    x += icon_value(Icon::Coin, &cash, x, 262., 26., GOLD) + 34.;
    bin_glyph(x + 12., 264., 0.8);
    let waste = format!("{}", game.waste);
    let color = if game.waste > 0 { CORAL } else { SAGE };
    draw_text_styled(
        &waste,
        x + 34.,
        flat(26., color).centered_baseline(263.),
        style(26., color),
    );
    for i in 0..3 {
        let level = usize::from(game.upgrades[i]);
        let maxed = level >= 2;
        let cost = upgrade_cost(i, game.upgrades[i]);
        let afford = !maxed && game.cash >= cost;
        let hovered = upgrade_at(p) == Some(i);
        let x = 86. + i as f32 * 270.;
        let y = 346. - if hovered && afford { 3. } else { 0. };
        rounded(x, 349., 252., 96., 12., Color::new(0., 0., 0., 0.3));
        let lift = if hovered { 0.16 } else { 0.1 };
        rounded_gradient(
            x,
            y,
            252.,
            96.,
            12.,
            lighten(TEAL, lift),
            darken(TEAL, 0.08),
        );
        if afford {
            let glow_a = if hovered {
                1.
            } else {
                0.45 + 0.35 * pulse(t, 0.8)
            };
            outline(x, y, 252., 96., 12., 1.5, with_alpha(GOLD, glow_a));
        } else if hovered {
            outline(x, y, 252., 96., 12., 1.5, with_alpha(CORAL, 0.6));
        }
        keycap(["1", "2", "3"][i], "", x + 14., y + 14., PAPER);
        upgrade_glyph(i, x + 62., y + 24., 1.05, t);
        label(
            UPGRADE_NAMES[i],
            x + 88.,
            y + 30.,
            150.,
            16.,
            CREAM,
            TextAlign::Left,
        );
        for k in 0..2 {
            rounded(
                x + 14. + k as f32 * 22.,
                y + 52.,
                18.,
                5.,
                2.5,
                if k < level {
                    GOLD
                } else {
                    Color::new(1., 1., 1., 0.15)
                },
            );
        }
        // What the next level changes, then its price.
        let (stat, values) = match i {
            0 => (Icon::Clock, KNIFE_TIME),
            1 => (Icon::Bolt, SHOE_SPEED),
            _ => (Icon::Hourglass, BURN_GRACE),
        };
        let row = y + 76.;
        let mut sx = x + 14.;
        sx += icon_value(stat, values[level], sx, row, 15., SAGE) + 8.;
        if !maxed {
            chevron(sx + 3., row, 0.9, GREEN);
            label(
                values[level + 1],
                sx + 12.,
                row + 5.,
                60.,
                15.,
                GREEN,
                TextAlign::Left,
            );
            let price = format!("{cost}");
            let pw = 15. * 1.25 + measure(&price, 15.);
            icon_value(
                Icon::Coin,
                &price,
                x + 238. - pw,
                y + 56.,
                15.,
                if afford { GOLD } else { CORAL },
            );
        } else {
            icon_badge(Icon::Check, x + 226., y + 56., 20., GREEN, PAPER);
        }
        if !afford && !maxed {
            rounded(x, y, 252., 96., 12., Color::new(0.02, 0.05, 0.06, 0.35));
        }
    }
}
fn setback(game: &Game, t: f32) {
    let shift = game.content();
    fit(shift.name, 66., 202., 600., 32., CREAM);
    // A big emblem says why: guests walked out, or the kitchen closed.
    let emblem = vec2(840., 196.);
    let why = if game.too_many_missed() {
        Icon::People
    } else {
        Icon::Clock
    };
    glow(
        emblem.x,
        emblem.y,
        52.,
        with_alpha(CORAL, 0.18 + 0.08 * pulse(t, 0.8)),
    );
    icon(why, emblem.x, emblem.y, 54., CORAL);
    icon_badge(
        Icon::Cross,
        emblem.x + 24.,
        emblem.y + 22.,
        24.,
        CORAL,
        PAPER,
    );
    let w = guests(game, 68., 262., 24.);
    closing(game, 68. + w + 30., 262., 28., t);
    // Retries left; the one this attempt spends pulses.
    rounded(66., 318., 330., 56., 14., Color::new(1., 1., 1., 0.05));
    icon(Icon::Restart, 98., 346., 30., GOLD);
    // Retrying restores the purse the service opened with.
    chevron(258., 346., 1.1, with_alpha(SAGE, 0.8));
    let purse = format!("{}", game.retry_cash());
    icon_value(Icon::Coin, &purse, 280., 346., 24., GOLD);
    for k in 0..3u8 {
        let c = vec2(140. + f32::from(k) * 34., 346.);
        if k + 1 == game.retries {
            glow(c.x, c.y, 18., with_alpha(GOLD, 0.25 * pulse(t, 1.2)));
            disc(c, 8., lighten(GOLD, 0.2 * pulse(t, 1.2)));
        } else if k < game.retries {
            disc(c, 8., GOLD);
        } else {
            ring(c, 7., 2., with_alpha(SAGE, 0.4));
        }
    }
}

// ------------------------------------------------------------ results card

fn result(game: &Game, area: Rect) {
    let t = clock();
    let (x, y) = (area.x, area.y);
    let won = game.phase == Phase::Won;
    if won {
        icon(Icon::Crown, x + 13., y + 16., 26., GOLD);
        fit(game.rank(), x + 36., y + 24., area.w - 40., 22., GOLD);
    } else {
        icon(Icon::Flag, x + 13., y + 16., 24., CORAL);
        fit(
            game.content().name,
            x + 36.,
            y + 24.,
            area.w - 40.,
            22.,
            CREAM,
        );
    }
    let cy = y + 72.;
    let stars = format!("{}/21", game.stars());
    let dishes = format!("{}", game.total_served);
    let cash = format!("{}", game.cash);
    let mut sx = x;
    for (icon_kind, value, color) in [
        (Icon::Star, stars.as_str(), GOLD),
        (Icon::People, dishes.as_str(), CREAM),
        (Icon::Coin, cash.as_str(), GOLD),
    ] {
        sx += icon_value(icon_kind, value, sx, cy, 30., color) + 44.;
    }
    // Every service's medal, three stars each.
    for s in 0..SHIFTS.len() {
        let gx = x + 8. + s as f32 * 84.;
        let reached = s < game.shift_index || (s == game.shift_index && won);
        for k in 0..3u8 {
            let c = vec2(gx + f32::from(k) * 19., y + 124.);
            let lit = reached && k < game.medals[s];
            let color = if lit {
                GOLD
            } else {
                Color::new(1., 1., 1., 0.14)
            };
            let lift = if lit {
                bob(t + s as f32 * 0.2, 0.5, 1.)
            } else {
                0.
            };
            icon(Icon::Star, c.x, c.y + lift, 16., color);
        }
    }
}

// ---------------------------------------------------------------- effects

fn refusal(fx: &mut Juice, station: Station, why: Why) {
    if why == Why::Nothing {
        return;
    }
    let c = station_center(station);
    let at = c - vec2(0., 26.);
    let calm = matches!(why, Why::Cooking | Why::Hold);
    fx.ring(c, if calm { GOLD } else { CORAL }, 28.);
    if !calm {
        fx.shake(0.12);
    }
    match why {
        Why::Full => fx.popup_icon(at, Icon::Hand, "Full", CORAL),
        Why::Coins => fx.popup_icon(at, Icon::Coin, "Need 2", CORAL),
        Why::Prep => fx.popup(at, "Chop first", CORAL),
        Why::Twice => fx.popup(at, "In bowl", CORAL),
        Why::Recipe => fx.popup(at, "No recipe", CORAL),
        Why::Busy => fx.popup(at, "Busy", CORAL),
        // The pot's own ring already counts down.
        Why::Cooking => {}
        Why::Burnt => fx.popup_icon(at, Icon::Flame, "Burnt", CORAL),
        Why::Cold => fx.popup(at, "Plate it", CORAL),
        Why::Cook => fx.popup_icon(at, Icon::Flame, "Cook first", CORAL),
        Why::Order => fx.popup_icon(at, Icon::Person, "No order", CORAL),
        Why::Hold => fx.popup(at, "Hold", GOLD),
        Why::Wrong => fx.popup_icon(at, Icon::Cross, "", CORAL),
        Why::Nothing => {}
    }
}
fn effects(game: &Game, fx: &mut Juice) {
    for &event in game.feed.events() {
        match event {
            Event::Opened => {
                fx.ring(DOOR, GOLD, 40.);
                fx.sparkle(DOOR, GOLD);
            }
            Event::Seated(slot) => {
                let c = bubble_center(slot);
                fx.ring(c, CREAM, 40.);
                fx.sparkle(c, CREAM);
            }
            Event::Left(slot) => {
                let c = bubble_center(slot);
                fx.ring(c, CORAL, 46.);
                fx.burst(c, SMOKE, 16);
                fx.popup_icon(c - vec2(0., 24.), Icon::Heart, "-1", CORAL);
            }
            Event::Bought(i) => {
                let c = station_center(Station::Pantry(i));
                fx.burst(c, FOOD[i], 10);
                fx.popup_icon(c - vec2(0., 6.), Icon::Coin, "-2", darken(GOLD, 0.05));
            }
            Event::Prepped(i) => {
                let c = station_center(Station::Chop);
                fx.burst(c, FOOD[i], 22);
                fx.ring(c, GREEN, 32.);
                fx.sparkle(c, GREEN);
            }
            Event::Added(i) => {
                let c = station_center(Station::Mix);
                fx.burst(c, FOOD[i], 12);
                fx.ring(c, CREAM, 28.);
            }
            Event::Lifted(_) => {
                let c = station_center(Station::Mix);
                fx.ring(c, GREEN, 34.);
                fx.sparkle(c, GREEN);
            }
            Event::Cooking(b) => {
                let c = station_center(Station::Burner(b));
                fx.ring(c, FLAME, 32.);
                fx.burst(c, FLAME, 12);
            }
            Event::Ready(b) => {
                let c = station_center(Station::Burner(b));
                fx.ring(c, GREEN, 42.);
                fx.sparkle(c, GREEN);
                fx.popup_icon(c - vec2(0., 30.), Icon::Bell, "", GREEN);
            }
            Event::Burnt(b) => {
                let c = station_center(Station::Burner(b));
                fx.burst(c, SMOKE, 28);
                fx.ring(c, CORAL, 42.);
                fx.popup_icon(c - vec2(0., 30.), Icon::Flame, "Burnt", CORAL);
                fx.shake(0.25);
            }
            Event::Took(b) => {
                let c = station_center(Station::Burner(b));
                fx.ring(c, CREAM, 32.);
                fx.sparkle(c, CREAM);
            }
            Event::Cleaned(b) => {
                let c = station_center(Station::Burner(b));
                fx.burst(c, STEEL, 16);
                fx.ring(c, STEEL, 32.);
            }
            Event::Plated => {
                let c = station_center(Station::Plate);
                fx.ring(c, CREAM, 30.);
                fx.sparkle(c, CREAM);
            }
            Event::Served { slot, pay } => {
                let c = bubble_center(slot);
                fx.burst(c, GOLD, 28);
                fx.sparkle(c, GOLD);
                fx.ring(c, GOLD, 48.);
                fx.popup_icon(c - vec2(0., 30.), Icon::Coin, &format!("+{pay}"), GOLD);
                fx.ring(station_center(Station::Serve), GOLD, 30.);
            }
            Event::Binned => {
                let c = if game.station() == Some(Station::Bin) {
                    station_center(Station::Bin)
                } else {
                    cook_center(game)
                };
                fx.burst(c, SMOKE, 14);
                fx.popup(c - vec2(0., 26.), "Waste", CORAL);
            }
            Event::Emptied => {
                let c = station_center(Station::Mix);
                fx.burst(c, SMOKE, 14);
                fx.popup(c - vec2(0., 26.), "Waste", CORAL);
            }
            Event::Refused(station, why) => refusal(fx, station, why),
            Event::Upgraded { kind, cost } => {
                let c = upgrade_center(kind);
                fx.ring(c, GOLD, 80.);
                fx.burst(c, GOLD, 24);
                fx.sparkle(c, GOLD);
                fx.popup_icon(c - vec2(0., 34.), Icon::Coin, &format!("-{cost}"), GOLD);
            }
            Event::Declined(kind) => {
                let c = upgrade_center(kind);
                fx.ring(c, CORAL, 70.);
                fx.shake(0.15);
                if game.upgrades[kind] >= 2 {
                    fx.popup_icon(c - vec2(0., 34.), Icon::Check, "Max", SAGE);
                } else {
                    let need = upgrade_cost(kind, game.upgrades[kind]);
                    fx.popup_icon(
                        c - vec2(0., 34.),
                        Icon::Coin,
                        &format!("Need {need}"),
                        CORAL,
                    );
                }
            }
            Event::Page => fx.ring(thumb_center(game.book), GOLD, 24.),
            Event::Walk(cell) => fx.ring(cell_center(cell), CREAM, 20.),
            Event::Complete(stars) => {
                fx.callout(vec2(480., 300.), "Complete!", GOLD);
                fx.confetti(vec2(480., 150.), 30 + u32::from(stars) * 20);
            }
            Event::Failed => fx.callout(vec2(480., 300.), "Setback", CORAL),
        }
    }
}

// ------------------------------------------------------------------- host

impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        let t = clock();
        let p = pointer();
        rounded_gradient(
            16.,
            112.,
            928.,
            456.,
            12.,
            Color::new(0.06, 0.15, 0.18, 1.),
            Color::new(0.04, 0.10, 0.12, 1.),
        );
        if matches!(
            self.phase,
            Phase::Briefing | Phase::Debrief | Phase::Setback
        ) {
            menu(self, t, p);
        } else {
            dining(self, t, p);
            kitchen(self, t, p);
            sidebar(self, t, p);
        }
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: self.content().name.to_string(),
            objective: format!(
                "Serve {} of {} guests",
                self.content().quota,
                self.content().menu.len()
            ),
            status: format!(
                "{} coins, {} served, {} missed, {} dishes in total",
                self.cash, self.served, self.missed, self.total_served
            ),
            score: self.score(),
        }
    }
    fn outcome(&self) -> Outcome {
        self.outcome()
    }
    fn summary(&self) -> String {
        self.summary()
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.shift() as u32, SHIFTS.len() as u32))
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        effects(self, fx);
    }
    fn draw_objective(&self, area: Rect) -> bool {
        objective(self, area);
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        result(self, area);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn press() -> Input {
        Input {
            primary: true,
            ..Input::default()
        }
    }
    fn active() -> Game {
        let mut game = Game::new();
        game.step(&press());
        game
    }
    fn stand(game: &mut Game, station: Station) {
        game.position = access(station);
        game.previous = game.position;
        game.move_wait = 0;
    }
    #[test]
    fn progress_counts_services_from_first_to_last() {
        let mut game = Game::new();
        assert_eq!(game.progress(), Some((1, 7)));
        for tick in 0..180_000 {
            let input = game.pilot(tick);
            game.step(&input);
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Won);
        assert_eq!(game.progress(), Some((7, 7)));
        assert_eq!(game.summary().matches(". ").count(), 0);
    }
    #[test]
    fn serving_pays_out_with_a_coin_popup_and_sparks_at_the_table() {
        let mut game = active();
        stand(&mut game, Station::Serve);
        game.carry = Carry::Plated(0);
        let cue = game.step(&press());
        assert_eq!(cue, Cue::Good);
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        let counts = fx.counts();
        assert_eq!(counts.popups, 1);
        assert!(counts.sparks > 20 && counts.rings >= 2);
        // Nothing accepted, nothing shown.
        let cue = game.step(&Input::default());
        let mut calm = Juice::new();
        game.effects(cue, &mut calm);
        assert!(calm.is_idle());
    }
    #[test]
    fn refusals_get_a_small_local_effect() {
        let mut game = active();
        stand(&mut game, Station::Serve);
        game.carry = Carry::Plated(3);
        let cue = game.step(&press());
        assert_eq!(cue, Cue::None);
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        assert_eq!((fx.counts().rings, fx.counts().popups), (1, 1));
        assert_eq!(fx.counts().sparks, 0);
    }
    #[test]
    fn a_burnt_pot_smokes_and_shakes_where_it_burned() {
        let mut game = active();
        stand(&mut game, Station::Burner(1));
        game.carry = Carry::Mixed(1);
        game.step(&press());
        let mut burnt = None;
        for _ in 0..600 {
            let cue = game.step(&Input::default());
            if cue == Cue::Bad && game.feed.events().contains(&Event::Burnt(1)) {
                burnt = Some(cue);
                break;
            }
        }
        let mut fx = Juice::new();
        game.effects(burnt.expect("the pot burns"), &mut fx);
        assert!(fx.counts().sparks >= 20 && fx.counts().popups == 1);
        assert!(fx.shake_offset().length() > 0.);
    }
    #[test]
    fn next_step_highlights_follow_the_food_in_hand() {
        let mut game = active();
        game.carry = Carry::Raw(0);
        assert!(goals(&game)[station_index(Station::Chop)]);
        game.carry = Carry::Mixed(1);
        let lit = goals(&game);
        assert!(lit[station_index(Station::Burner(0))] && lit[station_index(Station::Burner(1))]);
        game.carry = Carry::Empty;
        let lit = goals(&game);
        assert!(lit[station_index(Station::Pantry(0))] && lit[station_index(Station::Pantry(1))]);
        assert!(!lit[station_index(Station::Pantry(2))]);
    }
    #[test]
    fn serve_prompt_previews_the_exact_payment() {
        let mut game = active();
        stand(&mut game, Station::Serve);
        game.carry = Carry::Plated(0);
        let Some(Hint::Serve(preview)) = hint(&game) else {
            panic!("no serve prompt");
        };
        let cash = game.cash;
        game.step(&press());
        assert_eq!(game.cash, cash + preview);
    }
    #[test]
    fn header_and_help_stay_within_the_word_budget() {
        let game = Game::new();
        let hud = game.hud();
        assert_eq!(hud.stage, "The First Lunch");
        assert!(hud.objective.split_whitespace().count() <= 5);
        assert!(crate::META.help.len() <= 4);
        for line in crate::META.help {
            assert!(line.split_whitespace().count() < 15, "{line}");
        }
    }
}
