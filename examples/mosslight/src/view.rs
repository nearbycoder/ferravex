//! Read-only dungeon art and the shared arcade adapter.
//!
//! Board tiles stay flat quads (they meet edge to edge); depth comes from a
//! three-quarter wall face, ambient occlusion strips and lantern falloff that
//! only recolor those quads. Rounded and anti-aliased shapes are reserved for
//! characters, pickups and the interface cards.
use crate::{
    content::{COLS, FLOORS, ROWS},
    model::{Enemy, Game, Phase, Plan, Pos, Species, Tile},
};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;

const MOSS: Color = Color::new(0.48, 0.76, 0.40, 1.);
const DEEP_MOSS: Color = Color::new(0.30, 0.47, 0.25, 1.);
const GOLD: Color = Color::new(1., 0.76, 0.34, 1.);
const RED: Color = Color::new(0.97, 0.37, 0.32, 1.);
const CELADON: Color = Color::new(0.74, 0.88, 0.74, 1.);
const BLUE: Color = Color::new(0.39, 0.75, 0.82, 1.);
const CARD: Color = Color::new(0.075, 0.115, 0.105, 1.);
const RAISED: Color = Color::new(0.11, 0.16, 0.145, 1.);
const TILE: f32 = 38.;
const LEFT: f32 = 26.;
const TOP: f32 = 128.;
/// Sidebar content column.
const SX: f32 = 636.;
const SR: f32 = 916.;
const SW: f32 = SR - SX;

fn center(pos: Pos) -> (f32, f32) {
    (
        LEFT + pos.x as f32 * TILE + TILE * 0.5,
        TOP + pos.y as f32 * TILE + TILE * 0.5,
    )
}
fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new(r, g, b, a)
}
/// Scale a color's light, keeping alpha.
fn lit(c: Color, light: f32) -> Color {
    Color::new(c.r * light, c.g * light, c.b * light, c.a)
}
/// Gentle sine wave driven by the model's own tick counter.
fn wave(game: &Game, speed: f32, phase: f32) -> f32 {
    ((game.ticks % 100_000) as f32 * speed + phase).sin()
}
/// Lantern falloff: tiles near the hero stay bright, distant ones dim a little.
fn lantern(game: &Game, pos: Pos) -> f32 {
    let dx = (pos.x - game.hero.x) as f32;
    let dy = (pos.y - game.hero.y) as f32;
    let d = (dx * dx + dy * dy).sqrt();
    1. - ((d - 1.5) / 8.).clamp(0., 1.) * 0.34
}
/// Card text style: headline sizes keep the host's crisp drop shadow; body
/// text on the dark cards skips it, which halves its geometry.
fn ink(size: f32, color: Color) -> TextStyle {
    if size >= 18. {
        style(size, color)
    } else {
        TextStyle::modern(size, color)
    }
}
fn label(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(size, color)).width
}
fn label_right(value: &str, right: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(
        value,
        right,
        y,
        ink(size, color).with_align(TextAlign::Right),
    )
    .width
}
fn label_centered(value: &str, cx: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, cx, y, ink(size, color).with_align(TextAlign::Center)).width
}
/// Small letter-spaced section header.
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, eyebrow_style(color)).width
}
fn eyebrow_style(color: Color) -> TextStyle {
    TextStyle::modern(10., color).with_letter_spacing(1.5)
}
fn diamond(x: f32, y: f32, r: f32, color: Color) {
    draw_triangle(vec2(x, y - r), vec2(x + r, y), vec2(x, y + r), color);
    draw_triangle(vec2(x, y - r), vec2(x, y + r), vec2(x - r, y), color);
}
fn leaf(x: f32, y: f32, color: Color) {
    draw_ellipse(x, y, 6., 10., 0.55, color);
    draw_ellipse(x - 1.5, y - 2., 2.5, 5., 0.55, lighten(color, 0.35));
    draw_line(x - 3., y + 7., x + 3., y - 6., 1.2, darken(color, 0.62));
}
fn flask(x: f32, y: f32, color: Color) {
    shapes::fill_rounded_rect(Rect::new(x - 3., y - 11., 6., 6.), 1.5, GOLD);
    shapes::fill_circle(
        vec2(x, y + 2.),
        7.,
        Fill::radial(
            vec2(x - 2., y),
            8.,
            lighten(color, 0.25),
            darken(color, 0.2),
        ),
    );
    draw_rectangle(x - 3., y - 6., 6., 4., color);
    draw_line(
        x - 3.5,
        y - 1.,
        x - 3.5,
        y + 4.,
        1.6,
        rgba(1., 1., 1., 0.65),
    );
}
fn wall(game: &Game, pos: Pos, x: f32, y: f32, light: f32) {
    let open_below = pos.y + 1 < ROWS as i32 && game.tile(pos.offset(0, 1)) != Tile::Wall;
    let open_above = game.tile(pos.offset(0, -1)) != Tile::Wall && pos.y > 0;
    let top = lit(rgba(0.20, 0.275, 0.24, 1.), light);
    let seam = lit(rgba(0.13, 0.18, 0.16, 1.), light);
    let grout = lit(rgba(0.07, 0.10, 0.09, 1.), light);
    draw_rectangle(x, y, TILE, TILE, top);
    // Each block reads as a separate stone: grout right and bottom, a
    // staggered seam on the cap.
    let stagger = if pos.y % 2 == 0 { 12. } else { 25. };
    draw_rectangle(x + 37., y, 1., TILE, grout);
    draw_rectangle(x + stagger, y + 2., 1., 10., seam);
    draw_rectangle(x, y + 13., TILE, 1., seam);
    if open_above {
        draw_rectangle(x, y, TILE, 2., lit(rgba(0.34, 0.43, 0.35, 1.), light));
    }
    let hash = (pos.x * 23 + pos.y * 31 + game.depth as i32 * 13) as u32;
    if open_below {
        // Front face of a raised block, lit along its upper edge.
        let face = lit(rgba(0.07, 0.105, 0.095, 1.), light);
        draw_rectangle(x, y + 26., TILE, 12., face);
        draw_rectangle(
            x,
            y + 25.,
            TILE,
            1.5,
            lit(rgba(0.36, 0.46, 0.36, 1.), light),
        );
        draw_rectangle(
            x,
            y + 32.,
            TILE,
            1.,
            lit(rgba(0.035, 0.055, 0.05, 1.), light),
        );
        draw_rectangle(x + 38. - stagger, y + 27., 1., 5., grout);
        draw_rectangle(x + stagger * 0.5 + 6., y + 33., 1., 5., grout);
        if hash.is_multiple_of(3) {
            // Moss drip over the lip.
            let mx = x + 6. + (hash % 24) as f32;
            draw_rectangle(
                mx,
                y + 25.,
                3.,
                4. + (hash % 4) as f32,
                lit(DEEP_MOSS, light),
            );
        }
    } else {
        draw_rectangle(x, y + 37., TILE, 1., grout);
    }
    // A small clump of moss on some caps.
    if !hash.is_multiple_of(3) {
        let moss = lit(DEEP_MOSS, light);
        let bright = lit(rgba(0.45, 0.64, 0.33, 1.), light);
        let mx = x + 7. + (hash % 22) as f32;
        let my = y + if open_below { 19. } else { 24. } + (hash % 5) as f32;
        draw_poly(mx, my, 6, 3., 0., moss);
        draw_poly(mx + 4., my + 1., 6, 2.2, 0., moss);
        draw_rectangle(mx - 1.5, my - 2., 2., 1.5, bright);
    }
}
fn floor(game: &Game, pos: Pos, x: f32, y: f32, light: f32) {
    let shade = if (pos.x + pos.y) % 2 == 0 { 0.088 } else { 0.1 };
    draw_rectangle(
        x,
        y,
        TILE,
        TILE,
        lit(rgba(shade, shade + 0.058, shade + 0.034, 1.), light),
    );
    // Lit top edge, grout on the bottom and right.
    draw_rectangle(x, y, TILE, 1., lit(rgba(0.16, 0.22, 0.19, 1.), light));
    draw_rectangle(x, y + 37., TILE, 1., rgba(0.03, 0.05, 0.045, 0.8));
    draw_rectangle(x + 37., y, 1., TILE, rgba(0.03, 0.05, 0.045, 0.6));
    // Ambient occlusion where a wall stands above or to the left.
    if game.tile(pos.offset(0, -1)) == Tile::Wall {
        draw_rectangle(x, y, TILE, 5., rgba(0., 0.01, 0.01, 0.34));
        draw_rectangle(x, y + 5., TILE, 5., rgba(0., 0.01, 0.01, 0.14));
    }
    if game.tile(pos.offset(-1, 0)) == Tile::Wall {
        draw_rectangle(x, y, 4., TILE, rgba(0., 0.01, 0.01, 0.2));
    }
    if (pos.x * 3 + pos.y * 5) % 4 == 0 {
        let grass = lit(rgba(0.26, 0.38, 0.24, 1.), light);
        draw_line(x + 5., y + 31., x + 8., y + 25., 1.2, grass);
        draw_line(x + 10., y + 32., x + 8., y + 25., 1.2, grass);
        draw_line(x + 11., y + 32., x + 13., y + 27., 1.2, grass);
    }
}
fn tile(game: &Game, pos: Pos) {
    let (cx, cy) = center(pos);
    let x = cx - TILE * 0.5;
    let y = cy - TILE * 0.5;
    let kind = game.tile(pos);
    let light = lantern(game, pos);
    if kind == Tile::Wall {
        wall(game, pos, x, y, light);
        return;
    }
    floor(game, pos, x, y, light);
    match kind {
        Tile::Seal => {
            let open = game.seed;
            let bar = if open { with_alpha(GOLD, 0.35) } else { GOLD };
            draw_rectangle(x + 3., y + 3., 32., 32., rgba(0.06, 0.05, 0.02, 0.55));
            for offset in [-10., 0., 10.] {
                draw_rectangle(cx + offset - 2., y + 4., 4., 30., bar);
                draw_rectangle(
                    cx + offset - 2.,
                    y + 4.,
                    1.5,
                    30.,
                    rgba(1., 0.95, 0.75, 0.45),
                );
            }
            draw_rectangle(
                x + 4.,
                cy - 3.,
                30.,
                5.,
                if open { bar } else { darken(GOLD, 0.2) },
            );
            shapes::fill_circle(vec2(cx, cy), 8., rgba(0.08, 0.1, 0.06, 0.92));
            leaf(cx, cy, if open { MOSS } else { GOLD });
        }
        Tile::Seed => {
            let bob = wave(game, 0.07, 0.) * 1.6;
            shapes::fill_circle(
                vec2(cx, cy + 2.),
                15.,
                Fill::radial(
                    vec2(cx, cy),
                    15.,
                    rgba(0.42, 0.62, 0.26, 0.55),
                    rgba(0.18, 0.28, 0.14, 0.0),
                ),
            );
            draw_ellipse(cx, cy + 10., 11., 3.5, 0., rgba(0., 0., 0., 0.35));
            let pulse = 0.5 + 0.5 * wave(game, 0.07, 1.);
            shapes::stroke_circle(vec2(cx, cy), 15., 1.5, with_alpha(GOLD, 0.45 + pulse * 0.4));
            glow(cx, cy - 2., 24., rgba(0.6, 1., 0.45, 0.16 + pulse * 0.08));
            leaf(cx, cy - 2. + bob, MOSS);
            draw_rectangle(
                cx + 6.,
                cy - 11. + bob * 0.5,
                2.,
                2.,
                rgba(1., 0.95, 0.7, 0.9),
            );
        }
        Tile::Stairs => {
            let rim = if game.seed { GOLD } else { MUTED };
            draw_rectangle(x + 3., y + 3., 32., 32., rgba(0.01, 0.02, 0.02, 0.85));
            for i in 0..5 {
                let f = i as f32;
                let step = lit(rgba(0.30, 0.36, 0.33, 1.), 1. - f * 0.15);
                draw_rectangle(x + 5. + f * 2., y + 6. + f * 5.5, 28. - f * 4., 4., step);
                draw_rectangle(x + 5. + f * 2., y + 6. + f * 5.5, 28. - f * 4., 1., rim);
            }
            if game.seed {
                draw_rectangle_lines(
                    x + 2.,
                    y + 2.,
                    34.,
                    34.,
                    1.5,
                    with_alpha(GOLD, 0.6 + wave(game, 0.08, 0.) * 0.25),
                );
            }
        }
        Tile::Amber => {
            draw_ellipse(cx, cy + 11., 10., 3., 0., rgba(0., 0., 0., 0.4));
            shapes::fill_rounded_rect(
                Rect::new(cx - 9., cy - 8., 18., 19.),
                4.,
                Fill::vertical(
                    Rect::new(cx - 9., cy - 8., 18., 19.),
                    rgba(0.62, 0.42, 0.14, 1.),
                    rgba(0.34, 0.22, 0.08, 1.),
                ),
            );
            shapes::fill_rounded_rect(Rect::new(cx - 10., cy - 12., 20., 5.), 2., GOLD);
            diamond(cx, cy + 1., 6., GOLD);
            draw_rectangle(cx - 6., cy - 5., 2., 10., rgba(1., 0.92, 0.7, 0.5));
        }
        Tile::Flask => {
            draw_ellipse(cx, cy + 10., 8., 2.5, 0., rgba(0., 0., 0., 0.4));
            flask(cx, cy, Color::new(0.79, 0.27, 0.37, 1.));
        }
        Tile::Well | Tile::EmptyWell => {
            let full = kind == Tile::Well;
            draw_ellipse(cx, cy + 3., 15., 11., 0., rgba(0.06, 0.09, 0.085, 1.));
            draw_ellipse(cx, cy + 1., 14., 10., 0., rgba(0.27, 0.33, 0.30, 1.));
            draw_ellipse(
                cx,
                cy + 1.,
                10.5,
                6.5,
                0.,
                if full { rgba(0.18, 0.5, 0.6, 1.) } else { INK },
            );
            if full {
                let s = wave(game, 0.05, pos.x as f32) * 2.;
                draw_line(
                    cx - 6. + s,
                    cy - 1.,
                    cx + 2. + s,
                    cy - 1.,
                    1.5,
                    rgba(0.85, 0.97, 1., 0.85),
                );
                draw_line(
                    cx - 1. - s,
                    cy + 3.,
                    cx + 4. - s,
                    cy + 3.,
                    1.,
                    rgba(0.6, 0.9, 1., 0.6),
                );
            }
        }
        Tile::Thorns => {
            draw_rectangle(x + 4., y + 26., 30., 8., rgba(0.12, 0.08, 0.06, 0.7));
            for i in 0..3 {
                let dx = i as f32 * 9. - 9.;
                draw_line(
                    cx + dx - 3.,
                    cy + 12.,
                    cx + dx,
                    cy - 9.,
                    2.,
                    Color::new(0.56, 0.40, 0.30, 1.),
                );
                draw_triangle(
                    vec2(cx + dx, cy - 5.),
                    vec2(cx + dx + 6., cy - 2.),
                    vec2(cx + dx, cy + 1.),
                    RED,
                );
            }
        }
        _ => {}
    }
}
/// Telegraphed attack cells: pulsing wash and corner brackets.
fn danger(game: &Game, pos: Pos) {
    let (cx, cy) = center(pos);
    let x = cx - TILE * 0.5;
    let y = cy - TILE * 0.5;
    let pulse = 0.5 + 0.5 * wave(game, 0.11, 0.);
    draw_rectangle(
        x + 2.,
        y + 2.,
        34.,
        34.,
        rgba(0.75, 0.16, 0.12, 0.2 + pulse * 0.1),
    );
    let c = with_alpha(RED, 0.75 + pulse * 0.25);
    let (l, r, t, b, k) = (x + 3., x + 35., y + 3., y + 35., 8.);
    for (px, py, sx, sy) in [
        (l, t, 1., 1.),
        (r, t, -1., 1.),
        (l, b, 1., -1.),
        (r, b, -1., -1.),
    ] {
        draw_line(px, py, px + sx * k, py, 2., c);
        draw_line(px, py, px, py + sy * k, 2., c);
    }
}
fn enemy(game: &Game, enemy: Enemy, slot: usize) {
    let (x, base_y) = center(enemy.pos);
    let bob = wave(game, 0.06, slot as f32 * 1.7);
    let y = base_y + bob;
    draw_ellipse(
        x,
        base_y + 13.,
        13. - bob * 0.5,
        4.5,
        0.,
        rgba(0.01, 0.02, 0.02, 0.6),
    );
    match enemy.species {
        Species::Sprout => {
            draw_ellipse(x, y + 4., 11., 10., 0., darken(MOSS, 0.25));
            draw_ellipse(x - 1., y + 2., 9., 8., 0., MOSS);
            draw_ellipse(x - 4., y - 1., 3., 2., 0., lighten(MOSS, 0.45));
            leaf(x - 3., y - 9., MOSS);
            draw_rectangle(x - 5., y + 1., 3., 3., INK);
            draw_rectangle(x + 3., y + 1., 3., 3., INK);
        }
        Species::Knight => {
            draw_rectangle(x - 10., y - 6., 20., 21., Color::new(0.36, 0.28, 0.20, 1.));
            draw_rectangle(x - 10., y - 6., 20., 3., Color::new(0.52, 0.42, 0.30, 1.));
            draw_poly(x, y - 5., 6, 12., 30., Color::new(0.64, 0.54, 0.36, 1.));
            draw_poly(x - 2., y - 7., 6, 6., 30., Color::new(0.78, 0.68, 0.48, 1.));
            draw_rectangle(x - 8., y - 6., 16., 4., INK);
            draw_rectangle(x - 5., y - 5., 2., 2., RED);
            draw_rectangle(x + 3., y - 5., 2., 2., RED);
            draw_line(x + 13., y - 11., x + 13., y + 12., 3., darken(GOLD, 0.2));
            draw_line(x + 12.4, y - 11., x + 12.4, y + 12., 1., lighten(GOLD, 0.4));
            draw_triangle(
                vec2(x + 13., y - 17.),
                vec2(x + 8., y - 9.),
                vec2(x + 18., y - 9.),
                GOLD,
            );
        }
        Species::Spore => {
            draw_rectangle(x - 4., y - 1., 8., 15., CELADON);
            draw_rectangle(x - 4., y - 1., 2., 15., rgba(1., 1., 1., 0.4));
            draw_ellipse(x, y - 3., 14., 9., 0., Color::new(0.55, 0.30, 0.57, 1.));
            draw_ellipse(
                x - 1.,
                y - 5.,
                12.,
                6.5,
                0.,
                Color::new(0.69, 0.41, 0.70, 1.),
            );
            for offset in [-6., 2., 8.] {
                draw_rectangle(
                    x + offset - 1.5,
                    y - 7.5,
                    3.,
                    3.,
                    Color::new(0.95, 0.82, 0.86, 1.),
                );
            }
            draw_rectangle(x - 4., y + 4., 2., 2., INK);
            draw_rectangle(x + 2., y + 4., 2., 2., INK);
        }
        Species::King => {
            for (dx, dy) in [(-17., 9.), (17., 9.), (-15., -5.), (15., -5.)] {
                draw_line(
                    x,
                    y + 6.,
                    x + dx,
                    y + dy,
                    4.,
                    Color::new(0.40, 0.28, 0.17, 1.),
                );
                draw_line(
                    x,
                    y + 5.,
                    x + dx,
                    y + dy - 1.,
                    1.5,
                    Color::new(0.62, 0.46, 0.30, 1.),
                );
            }
            draw_poly(x, y + 2., 6, 15., 30., Color::new(0.42, 0.30, 0.19, 1.));
            draw_poly(x - 1.5, y, 6, 11., 30., Color::new(0.53, 0.38, 0.24, 1.));
            draw_rectangle(x - 10., y - 13., 20., 5., GOLD);
            for offset in [-9., 0., 9.] {
                draw_triangle(
                    vec2(x + offset - 4., y - 11.),
                    vec2(x + offset, y - 21.),
                    vec2(x + offset + 4., y - 11.),
                    GOLD,
                );
            }
            draw_rectangle(x - 10., y - 13., 20., 1.5, lighten(GOLD, 0.5));
            draw_line(x - 8., y - 2., x - 3., y, 2.5, RED);
            draw_line(x + 8., y - 2., x + 3., y, 2.5, RED);
        }
    }
    match enemy.plan {
        Plan::Rest => {
            let drift = (game.ticks % 90) as f32 / 90.;
            let a = 0.85 - drift * 0.6;
            label(
                "z",
                x + 9.,
                y - 12. - drift * 5.,
                11.,
                with_alpha(CELADON, a),
            );
        }
        Plan::Move(target) => {
            let (tx, ty) = center(target);
            let (ex, ey) = (x + (tx - x) * 0.42, base_y + (ty - base_y) * 0.42);
            draw_line(x, base_y, ex, ey, 2., with_alpha(GOLD, 0.8));
            diamond(ex, ey, 3.5, GOLD);
        }
        _ => {
            let (bx, by) = (x + 13., y - 14.);
            shapes::fill_circle(vec2(bx, by), 6.5, RED);
            draw_rectangle(bx - 1., by - 4., 2., 5., PAPER);
            draw_rectangle(bx - 1., by + 2., 2., 2., PAPER);
        }
    }
    if enemy.health < enemy.max_health {
        meter(
            x - 13.,
            base_y + 17.,
            26.,
            4.,
            enemy.health as f32 / enemy.max_health as f32,
            RED,
        );
    }
}
fn hero(game: &Game) {
    let (x, base_y) = center(game.hero);
    let bob = wave(game, 0.05, 0.) * 1.2;
    let y = base_y + bob;
    let threatened = game.danger(game.hero) > 0;
    let ring = if threatened { RED } else { MOSS };
    draw_ellipse(x, base_y + 12., 15., 5.5, 0., with_alpha(ring, 0.28));
    draw_ellipse(x, base_y + 12., 11., 4., 0., rgba(0.01, 0.02, 0.02, 0.7));
    if game.flash > 0 {
        let f = game.flash as f32 / 22.;
        draw_ellipse(x, base_y + 12., 17., 6.5, 0., with_alpha(PAPER, 0.35 * f));
    }
    // Cloak with a shaded fold.
    draw_triangle(
        vec2(x, y - 9.),
        vec2(x - 11., y + 12.),
        vec2(x + 11., y + 12.),
        Color::new(0.20, 0.43, 0.42, 1.),
    );
    draw_triangle(
        vec2(x, y - 9.),
        vec2(x + 2., y + 12.),
        vec2(x + 11., y + 12.),
        Color::new(0.15, 0.33, 0.33, 1.),
    );
    draw_circle(x, y - 7., 8., Color::new(0.27, 0.55, 0.46, 1.));
    draw_circle(x, y - 5., 5., Color::new(0.93, 0.78, 0.58, 1.));
    draw_rectangle(x - 2.5, y - 6., 1.5, 1.5, INK);
    draw_rectangle(x + 1., y - 6., 1.5, 1.5, INK);
    draw_line(x - 7., y + 3., x - 13., y - 5., 2., PAPER);
    draw_line(x - 15., y - 1., x - 10., y - 6., 2., GOLD);
    // Lantern on a short pole.
    draw_line(x + 7., y + 4., x + 14., y + 1., 2., CELADON);
    let sway = wave(game, 0.05, 1.3) * 0.8;
    let (lx, ly) = (x + 13.5 + sway, y + 4.);
    draw_rectangle(lx - 3.5, ly - 4., 7., 9., darken(GOLD, 0.25));
    draw_rectangle(lx - 2.5, ly - 3., 5., 7., Color::new(1., 0.96, 0.68, 1.));
    draw_rectangle(lx - 4., ly - 5., 8., 1.5, GOLD);
}
/// Bounded paragraph: wraps at measured widths and ellipsizes past
/// `max_lines`. Returns the next baseline.
fn paragraph(
    value: &str,
    x: f32,
    y: f32,
    width: f32,
    size: f32,
    max_lines: usize,
    color: Color,
) -> f32 {
    let s = ink(size, color);
    let step = size * 1.35;
    let mut baseline = y;
    let mut lines = wrap_text(value, width, s).peekable();
    let mut count = 0;
    while let Some(line) = lines.next() {
        count += 1;
        if count == max_lines && lines.peek().is_some() {
            let mut cut = line.to_owned();
            while !cut.is_empty() && text_width(&format!("{cut}..."), s) > width {
                cut.pop();
            }
            draw_text_styled(&format!("{}...", cut.trim_end()), x, baseline, s);
            return baseline + step;
        }
        draw_text_styled(line, x, baseline, s);
        baseline += step;
    }
    baseline
}
/// Compact stat tile: eyebrow, icon slot, value and optional key.
fn stat(x: f32, y: f32, w: f32, name: &str, value: &str, color: Color, key: &str) {
    shapes::fill_rounded_rect(Rect::new(x, y, w, 58.), 8., RAISED);
    shapes::fill_rounded_rect(Rect::new(x, y, w, 1.5), 0., rgba(1., 1., 1., 0.04));
    eyebrow(name, x + 10., y + 18., MUTED);
    label(value, x + 10., y + 46., 18., color);
    if !key.is_empty() {
        keycap(key, "", x + w - 30., y + 30., PAPER);
    }
}
fn sidebar(game: &Game) {
    card(616., 120., 320., 434., CARD);
    // Expedition header.
    eyebrow(
        &format!("FLOOR {} OF {}", game.floor(), FLOORS.len()),
        SX,
        146.,
        MOSS,
    );
    label_right(&format!("Turn {}", game.turns), SR, 147., 13., MUTED);
    label(FLOORS[game.depth].name, SX, 174., 20., PAPER);
    let gap = 6.;
    let seg = (SW - gap * 5.) / 6.;
    for depth in 0..6 {
        let color = if depth < game.depth {
            MOSS
        } else if depth == game.depth {
            GOLD
        } else {
            rgba(1., 1., 1., 0.08)
        };
        shapes::fill_rounded_rect(
            Rect::new(SX + depth as f32 * (seg + gap), 186., seg, 4.),
            2.,
            color,
        );
    }
    divider(SX, 206., SW, rgba(1., 1., 1., 0.1));
    // Health.
    let low = game.health < 10;
    eyebrow("HEALTH", SX, 228., MUTED);
    label_right(
        &format!("{} / {}", game.health, game.max_health),
        SR,
        230.,
        18.,
        if low { RED } else { PAPER },
    );
    meter(
        SX,
        240.,
        SW,
        10.,
        game.health as f32 / game.max_health as f32,
        if low { RED } else { MOSS },
    );
    // Supplies.
    let w = (SW - 16.) / 3.;
    stat(SX, 264., w, "BLADE", &game.attack.to_string(), GOLD, "");
    stat(
        SX + w + 8.,
        264.,
        w,
        "FLASKS",
        &game.flasks.to_string(),
        if game.flasks > 0 { PAPER } else { MUTED },
        "X",
    );
    stat(
        SX + (w + 8.) * 2.,
        264.,
        w,
        "LANTERN",
        &format!("{}/{}", game.charge, game.max_charge),
        if game.charge > 0 { GOLD } else { MUTED },
        "C",
    );
    // Objective and context.
    let (goal, color) = if !game.seed {
        ("Find the key", GOLD)
    } else if game.king_alive() {
        ("Key found \u{b7} defeat the Root King", MOSS)
    } else {
        ("Key found \u{b7} take the stairs", MOSS)
    };
    chip(goal, SX, 340., 12., color);
    let contextual = game
        .inspected
        .and_then(|pos| game.enemy_at(pos))
        .and_then(|index| game.enemies[index]);
    if let Some(enemy) = contextual {
        label(enemy.species.name(), SX, 386., 16., PAPER);
        let intent = match enemy.plan {
            Plan::Rest => "Resting",
            Plan::Move(_) => "Moving",
            _ => "Attacks next turn",
        };
        label_right(
            intent,
            SR,
            386.,
            12.,
            if attacking(enemy.plan) { RED } else { MUTED },
        );
        paragraph(
            &format!(
                "Health {}/{} \u{b7} hits for {}",
                enemy.health,
                enemy.max_health,
                enemy.species.damage()
            ),
            SX,
            408.,
            SW,
            13.,
            1,
            CELADON,
        );
    } else {
        paragraph(FLOORS[game.depth].hint, SX, 386., SW, 14., 3, CELADON);
    }
    divider(SX, 440., SW, rgba(1., 1., 1., 0.1));
    eyebrow("LAST ACTION", SX, 462., MUTED);
    paragraph(&game.messages[2], SX, 482., SW, 13., 2, PAPER);
    let x = SX + keycap("Space", "Guard", SX, 522., MUTED) + 16.;
    keycap("Click", "Inspect", x, 522., MUTED);
}
fn camp(game: &Game) {
    draw_rectangle(
        LEFT,
        TOP,
        TILE * COLS as f32,
        TILE * ROWS as f32,
        rgba(0.01, 0.02, 0.02, 0.6),
    );
    card(32., 178., 558., 340., Color::new(0.07, 0.11, 0.095, 0.99));
    eyebrow("SAFE CAMP", 54., 204., GOLD);
    label("Choose a relic", 54., 232., 22., PAPER);
    label(
        "One permanent upgrade for the rest of the run.",
        54.,
        258.,
        13.,
        MUTED,
    );
    let entries = [
        ("Sunsteel", "+1 blade damage", "Hit harder."),
        ("Rootheart", "+5 maximum health", "Stay alive."),
        ("Long Wick", "+1 charge capacity", "Burn brighter."),
    ];
    for (index, (name, benefit, flavor)) in entries.iter().enumerate() {
        // Must match the model's click areas: x = 44 + i * 181, 171 wide, y 278..428.
        let x = 44. + index as f32 * 181.;
        let selected = game.choice == index;
        let r = Rect::new(x, 278., 171., 150.);
        if selected {
            shapes::drop_shadow(
                r,
                10.,
                shapes::Shadow::soft(with_alpha(GOLD, 0.18)).blur(12.),
            );
        }
        shapes::fill_rounded_rect(
            r,
            10.,
            Fill::vertical(
                r,
                if selected {
                    rgba(0.20, 0.27, 0.17, 1.)
                } else {
                    rgba(0.12, 0.17, 0.145, 1.)
                },
                if selected {
                    rgba(0.14, 0.19, 0.12, 1.)
                } else {
                    rgba(0.09, 0.13, 0.11, 1.)
                },
            ),
        );
        shapes::stroke_rounded_rect(
            r,
            10.,
            if selected { 2. } else { 1. },
            if selected {
                GOLD
            } else {
                rgba(1., 1., 1., 0.08)
            },
        );
        keycap(&(index + 1).to_string(), "", x + 10., 288., PAPER);
        let owned = game.choices[index];
        if owned > 0 {
            let s = style(11., CELADON);
            let w = text_width(&format!("Owned {owned}"), s) + 14.3;
            chip(&format!("Owned {owned}"), x + 161. - w, 289., 11., CELADON);
        }
        let (ix, iy) = (x + 85.5, 330.);
        shapes::fill_circle(
            vec2(ix, iy),
            24.,
            Fill::radial(
                vec2(ix, iy),
                24.,
                if selected {
                    rgba(0.42, 0.40, 0.16, 0.9)
                } else {
                    rgba(0.2, 0.28, 0.18, 0.9)
                },
                rgba(0.08, 0.12, 0.1, 0.0),
            ),
        );
        match index {
            0 => {
                draw_line(ix - 8., iy - 14., ix + 8., iy + 12., 5., PAPER);
                draw_line(
                    ix - 8.,
                    iy - 14.,
                    ix + 8.,
                    iy + 12.,
                    1.5,
                    rgba(1., 1., 1., 1.),
                );
                draw_line(ix - 6., iy + 10., ix + 10., iy + 1., 4., GOLD);
            }
            1 => {
                leaf(ix, iy, MOSS);
                draw_circle_lines(ix, iy, 16., 1.5, with_alpha(MOSS, 0.7));
            }
            _ => {
                diamond(ix, iy, 13., GOLD);
                diamond(ix - 2., iy - 2., 6., lighten(GOLD, 0.5));
            }
        }
        label_centered(name, ix, 378., 16., if selected { GOLD } else { PAPER });
        label_centered(benefit, ix, 398., 13., CELADON);
        label_centered(flavor, ix, 416., 12., MUTED);
    }
    label(
        "Every camp restores +7 health, +1 flask and +1 charge.",
        54.,
        456.,
        13.,
        CELADON,
    );
    let mut x = 54.;
    x += keycap("1-3", "Choose", x, 476., MUTED) + 18.;
    x += keycap("\u{2190}\u{2192}", "Select", x, 476., MUTED) + 18.;
    keycap("Space", "Confirm", x, 476., MUTED);
}

/// True when a creature's telegraphed plan resolves as an attack.
fn attacking(plan: Plan) -> bool {
    !matches!(plan, Plan::Rest | Plan::Move(_))
}

impl ArcadeGame for Game {
    fn new() -> Self {
        Game::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        rounded_gradient(
            16.,
            112.,
            928.,
            456.,
            0.,
            rgba(0.04, 0.075, 0.065, 1.),
            rgba(0.025, 0.045, 0.04, 1.),
        );
        card(18., 120., 586., 434., rgba(0.03, 0.05, 0.045, 1.));
        for y in 0..ROWS {
            for x in 0..COLS {
                tile(self, Pos::new(x as i32, y as i32));
            }
        }
        for y in 0..ROWS {
            for x in 0..COLS {
                let pos = Pos::new(x as i32, y as i32);
                if self.tile(pos) != Tile::Wall && self.danger(pos) > 0 {
                    danger(self, pos);
                }
            }
        }
        if let Some(pos) = self.inspected {
            let (x, y) = center(pos);
            outline(x - 18., y - 18., 36., 36., 6., 2., BLUE);
        }
        for (slot, e) in self.enemies.iter().enumerate() {
            if let Some(e) = e {
                enemy(self, *e, slot);
            }
        }
        hero(self);
        if self.phase != Phase::Camp {
            let (x, y) = center(self.hero);
            glow(x + 13., y + 4., 54., rgba(1., 0.72, 0.3, 0.2));
        }
        sidebar(self);
        if self.phase == Phase::Camp {
            camp(self);
        }
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!("{}/6  {}", self.floor(), FLOORS[self.depth].name),
            objective: if self.phase == Phase::Camp {
                "Choose a relic".into()
            } else if self.king_alive() && self.seed {
                "Defeat the boss".into()
            } else if self.seed {
                "Reach the stairs".into()
            } else {
                "Collect the key".into()
            },
            status: format!(
                "HP {}/{} | Blade {} | Flasks {} | Lantern {}/{} | Amber {}",
                self.health,
                self.max_health,
                self.attack,
                self.flasks,
                self.charge,
                self.max_charge,
                self.treasure
            ),
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        Game::outcome(self)
    }
    fn summary(&self) -> String {
        if self.phase == Phase::Won {
            let rank = if self.score >= 7800 {
                "Sunkeeper"
            } else if self.score >= 6500 {
                "Dawnbringer"
            } else {
                "Lantern Bearer"
            };
            format!(
                "Dawn returns to the village. Rank: {rank}. Six floors freed in {} turns; {} creatures defeated, {} amber jars recovered. Your lantern will hang above the well.",
                self.turns, self.kills, self.treasure
            )
        } else {
            format!(
                "The ember faded in {}. You lasted {} turns and recovered {} jars of daylight. Watch fixed red attack cells, save flasks for missing health, and try a different relic path.",
                FLOORS[self.depth].name, self.turns, self.treasure
            )
        }
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.autoplay(tick)
    }
}
