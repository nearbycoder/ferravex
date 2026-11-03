use crate::{
    Game,
    content::*,
    model::{Carry, Phase},
};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;
const CREAM: Color = Color::new(0.97, 0.89, 0.72, 1.);
const TEAL: Color = Color::new(0.13, 0.40, 0.42, 1.);
const DARK: Color = Color::new(0.08, 0.19, 0.23, 1.);
const PANEL: Color = Color::new(0.07, 0.14, 0.17, 1.);
const CORAL: Color = Color::new(0.96, 0.43, 0.29, 1.);
const GOLD: Color = Color::new(1., 0.69, 0.30, 1.);
const GREEN: Color = Color::new(0.44, 0.76, 0.42, 1.);
const WOOD: Color = Color::new(0.63, 0.42, 0.26, 1.);
const SAGE: Color = Color::new(0.62, 0.74, 0.70, 1.);
const INKY: Color = Color::new(0.30, 0.38, 0.40, 1.);
const ACCENT: Color = Color::new(1., 0.62, 0.3, 1.);
const SHADE: Color = Color::new(0.02, 0.05, 0.06, 0.28);
const TICKET_TOP: Color = Color::new(1., 0.96, 0.86, 1.);

/// Shadowless face for flat surfaces (half the vertices of a shadowed label).
fn flat(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, flat(11., color).with_letter_spacing(1.5)).width
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
fn para(value: &str, x: f32, mut y: f32, width: f32, size: f32, color: Color) -> f32 {
    let s = flat(size, color);
    for line in wrap_text(value, width, s).take(4) {
        draw_text_styled(line, x, y, s);
        y += size * 1.32;
    }
    y
}
/// Axis-aligned ellipse in one batched draw.
fn oval(x: f32, y: f32, rx: f32, ry: f32, color: Color) {
    draw_ellipse(x, y, rx, ry, 0., color);
}
/// Small filled circle in one batched draw, tessellated by size.
fn dot(x: f32, y: f32, r: f32, color: Color) {
    draw_poly(x, y, (r * 0.9).clamp(8., 24.) as u8, r, 0., color);
}
/// Flat progress bar with rounded ends, cheaper than [`meter`] for small HUD bars.
fn bar(x: f32, y: f32, w: f32, h: f32, f: f32, track: Color, fill: Color) {
    rounded(x, y, w, h, h * 0.5, track);
    let f = if f.is_finite() { f.clamp(0., 1.) } else { 0. };
    if f > 0. {
        rounded(x, y, (w * f).max(h), h, h * 0.5, fill);
    }
}
fn ingredient(i: usize, x: f32, y: f32, scale: f32, chopped: bool) {
    let color = [CORAL, GREEN, Color::new(0.79, 0.64, 0.51, 1.), CREAM][i];
    if chopped {
        for k in 0..5 {
            let dx = (k % 3) as f32 * 6. - 6.;
            let dy = (k / 3) as f32 * 6. - 3.;
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
                x + (i as f32 - 1.5) * 5.5 * scale,
                y - 1. * scale,
                0.36 * scale,
                true,
            );
        }
    }
}
fn flow(recipe: usize) -> &'static str {
    if RECIPES[recipe].cook == 0 {
        "Chop  →  Mix  →  Plate"
    } else {
        "Chop  →  Mix  →  Cook  →  Plate"
    }
}
fn button(x: f32, y: f32, w: f32, h: f32, action: &str) {
    rounded(x, y + 3., w, h, 12., Color::new(0., 0., 0., 0.35));
    rounded_gradient(x, y, w, h, 12., lighten(ACCENT, 0.12), ACCENT);
    panel(x + 12., y + 1., w - 24., 1., Color::new(1., 1., 1., 0.35));
    let s = flat(19., INK);
    let kw = (text_width("Space", flat(13., PAPER)) + 12.).max(20.);
    let tw = text_width(action, s);
    let left = x + (w - kw - 12. - tw) * 0.5;
    keycap("Space", "", left, y + h * 0.5 - 10., PAPER);
    draw_text_styled(action, left + kw + 12., s.centered_baseline(y + h * 0.5), s);
}
/// String lights across the top of the menu card.
fn bunting(ticks: u32) {
    for i in 0..15 {
        let x = 60. + i as f32 * 60.;
        let sag = ((i as f32 / 14.) * std::f32::consts::PI).sin() * 6.;
        let lit = (i + (ticks / 30) as usize).is_multiple_of(3);
        let c = if i % 2 == 0 { GOLD } else { CORAL };
        if i < 14 {
            let x2 = x + 60.;
            let sag2 = (((i + 1) as f32 / 14.) * std::f32::consts::PI).sin() * 6.;
            draw_line(
                x,
                132. + sag,
                x2,
                132. + sag2,
                1.,
                Color::new(1., 1., 1., 0.18),
            );
        }
        dot(
            x,
            137. + sag,
            7.,
            with_alpha(c, if lit { 0.22 } else { 0.1 }),
        );
        dot(x, 137. + sag, 3.5, c);
    }
}
fn menu(game: &Game) {
    card(34., 124., 892., 428., PANEL);
    bunting(game.ticks);
    let shift = format!("SERVICE {} OF 7", game.shift());
    match game.phase {
        Phase::Briefing => {
            eyebrow(&shift, 66., 172., GOLD);
            fit(game.content().name, 66., 208., 820., 32., CREAM);
            label(
                game.content().story,
                66.,
                238.,
                820.,
                17.,
                SAGE,
                TextAlign::Left,
            );
            let mut x = 66.;
            let quota = game.content().quota;
            for (text, tint) in [
                (format!("{} guests", game.content().menu.len()), CREAM),
                (format!("Serve at least {quota}"), GREEN),
                ("1 miss allowed".to_string(), GOLD),
            ] {
                x += chip(&text, x, 254., 13., tint) + 10.;
            }
            eyebrow("TONIGHT'S MENU", 66., 312., SAGE);
            let mut menu = Vec::new();
            for r in game.content().menu {
                if !menu.contains(r) {
                    menu.push(*r);
                }
            }
            for (i, recipe) in menu.iter().enumerate() {
                let x = 66. + (i % 3) as f32 * 278.;
                let y = 322. + (i / 3) as f32 * 70.;
                rounded(x, y, 266., 58., 10., Color::new(1., 1., 1., 0.05));
                dish(*recipe, x + 30., y + 29., 1., true);
                label(
                    RECIPES[*recipe].name,
                    x + 60.,
                    y + 26.,
                    196.,
                    15.,
                    CREAM,
                    TextAlign::Left,
                );
                label(
                    flow(*recipe),
                    x + 60.,
                    y + 45.,
                    196.,
                    11.,
                    SAGE,
                    TextAlign::Left,
                );
            }
            button(290., 484., 380., 52., "Open the doors");
        }
        Phase::Debrief => {
            eyebrow(&(shift + "  ·  COMPLETE"), 66., 172., GREEN);
            fit(game.content().name, 66., 208., 560., 32., CREAM);
            let stars = game.medals[game.shift_index];
            for i in 0..3u8 {
                let s = flat(
                    30.,
                    if i < stars {
                        GOLD
                    } else {
                        Color::new(1., 1., 1., 0.16)
                    },
                )
                .with_align(TextAlign::Right);
                draw_text_styled("★", 892. - f32::from(2 - i) * 40., 208., s);
            }
            let mut x = 66.;
            for (text, tint) in [
                (
                    format!("Served {} of {}", game.served, game.content().menu.len()),
                    GREEN,
                ),
                (format!("{} coins", game.cash), GOLD),
                (
                    format!("Waste {}", game.waste),
                    if game.waste > 0 { CORAL } else { SAGE },
                ),
            ] {
                x += chip(&text, x, 228., 13., tint) + 10.;
            }
            eyebrow("CHOOSE AN UPGRADE", 66., 300., SAGE);
            label(
                "Ingredients still cost 2 coins each, so keep a little cash.",
                892.,
                300.,
                520.,
                13.,
                SAGE,
                TextAlign::Right,
            );
            for i in 0..3 {
                let x = 86. + i as f32 * 270.;
                let level = game.upgrades[i];
                let maxed = level >= 2;
                let cost = upgrade_cost(i, level);
                let afford = !maxed && game.cash >= cost;
                rounded(x, 349., 252., 96., 12., Color::new(0., 0., 0., 0.3));
                rounded_gradient(
                    x,
                    346.,
                    252.,
                    96.,
                    12.,
                    lighten(TEAL, 0.1),
                    darken(TEAL, 0.08),
                );
                if afford {
                    outline(x, 346., 252., 96., 12., 1.5, with_alpha(GOLD, 0.7));
                }
                let kw = keycap(&(i + 1).to_string(), "", x + 14., 358., PAPER);
                label(
                    UPGRADE_NAMES[i],
                    x + 22. + kw,
                    373.,
                    200. - kw,
                    16.,
                    CREAM,
                    TextAlign::Left,
                );
                for k in 0..2u8 {
                    rounded(
                        x + 14. + f32::from(k) * 22.,
                        396.,
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
                let (tag, tint) = if maxed {
                    ("Max level".to_string(), SAGE)
                } else {
                    (format!("{cost} coins"), if afford { GOLD } else { CORAL })
                };
                label(&tag, x + 238., 402., 140., 13., tint, TextAlign::Right);
                label(
                    UPGRADE_HELP[i],
                    x + 14.,
                    428.,
                    224.,
                    12.,
                    SAGE,
                    TextAlign::Left,
                );
            }
            button(290., 484., 380., 52., "Next service");
        }
        _ => {
            eyebrow(&(shift + "  ·  SETBACK"), 66., 172., CORAL);
            fit("A second chance", 66., 208., 820., 32., CREAM);
            rounded(66., 236., 828., 48., 10., with_alpha(CORAL, 0.12));
            rounded(66., 236., 4., 48., 2., CORAL);
            label(
                game.note,
                86.,
                266.,
                790.,
                18.,
                lighten(CORAL, 0.25),
                TextAlign::Left,
            );
            para(
                "Retrying resets this service's ingredients, money and score to how they were when the doors opened.",
                66.,
                338.,
                820.,
                17.,
                SAGE,
            );
            let retries = format!(
                "{} {} left",
                game.retries,
                if game.retries == 1 {
                    "retry"
                } else {
                    "retries"
                }
            );
            chip(&retries, 66., 404., 15., GOLD);
            button(290., 484., 380., 52., "Try this service");
        }
    }
}
fn tickets(game: &Game) {
    let total = game.content().patience as f32;
    for (i, order) in game.orders.iter().enumerate() {
        let x = 28. + i as f32 * 204.;
        if let Some(order) = order {
            let r = RECIPES[order.recipe];
            let f = order.patience as f32 / total;
            let urgent = order.patience < 900;
            let selected = game.book == order.recipe;
            rounded(x, 127., 196., 76., 10., Color::new(0., 0., 0., 0.35));
            rounded_gradient(x, 124., 196., 76., 10., TICKET_TOP, CREAM);
            if selected {
                outline(x, 124., 196., 76., 10., 2., GOLD);
            }
            // Perforated tear line under the header.
            for k in 0..11 {
                panel(
                    x + 12. + k as f32 * 16.,
                    145.,
                    8.,
                    1.,
                    with_alpha(INKY, 0.3),
                );
            }
            eyebrow(&format!("GUEST #{:02}", order.number), x + 12., 139., INKY);
            let value = format!("{} coins", r.value);
            label(
                &value,
                x + 184.,
                139.,
                80.,
                12.,
                darken(GOLD, 0.45),
                TextAlign::Right,
            );
            let cw = text_width(&value, flat(12., INK));
            dot(x + 174. - cw, 135., 4., GOLD);
            dish(order.recipe, x + 30., 170., 0.85, true);
            label(r.name, x + 56., 168., 132., 15., DARK, TextAlign::Left);
            let secs = order.patience.div_ceil(60);
            let pulse = if urgent {
                0.6 + 0.4 * ((game.ticks as f32) * 0.2).sin().abs()
            } else {
                1.
            };
            label(
                &format!("{secs}s left"),
                x + 56.,
                184.,
                132.,
                12.,
                if urgent {
                    with_alpha(darken(CORAL, 0.1), pulse)
                } else {
                    INKY
                },
                TextAlign::Left,
            );
            bar(
                x + 12.,
                190.,
                172.,
                5.,
                f,
                Color::new(0.08, 0.19, 0.23, 0.14),
                if urgent { CORAL } else { TEAL },
            );
        } else {
            rounded(x, 124., 196., 76., 10., Color::new(1., 1., 1., 0.04));
            for k in 0..12 {
                let kx = x + 10. + k as f32 * 15.;
                panel(kx, 124., 8., 1., Color::new(1., 1., 1., 0.14));
                panel(kx, 199., 8., 1., Color::new(1., 1., 1., 0.14));
            }
            label(
                "Table open",
                x + 98.,
                166.,
                170.,
                14.,
                with_alpha(SAGE, 0.75),
                TextAlign::Center,
            );
        }
    }
    let next = if game.next_order < game.content().menu.len() {
        format!(
            "Next guest: {} in {}s",
            RECIPES[game.content().menu[game.next_order]].name,
            (game.next_order as u32 * game.content().interval)
                .saturating_sub(game.ticks)
                .div_ceil(60)
        )
    } else {
        "All guests have arrived".into()
    };
    dot(36., 212., 3., SAGE);
    label(&next, 46., 216., 560., 12., SAGE, TextAlign::Left);
}
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
fn kitchen(game: &Game) {
    let (x0, y0) = (GRID_X, GRID_Y);
    let (w, h) = (11. * TILE, 6. * TILE);
    rounded(x0, y0 + 4., w, h, 12., SHADE);
    rounded_gradient(x0, y0, w, h, 12., lighten(TEAL, 0.07), darken(TEAL, 0.12));
    // Counter seams.
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
    // Floor with a soft checker and the counters' contact shadow.
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
    // Counter lip highlight facing the floor.
    panel(
        fx - 2.,
        fy - 2.,
        9. * TILE + 4.,
        2.,
        Color::new(1., 1., 1., 0.12),
    );
    let blend = 1. - game.move_wait as f32 / game.walk_ticks() as f32;
    let pulse = ((game.ticks as f32) * 0.12).sin() * 0.5 + 0.5;
    for (station, cell, access) in STATIONS {
        let x = GRID_X + cell.x as f32 * TILE;
        let y = GRID_Y + cell.y as f32 * TILE;
        let center = vec2(x + 26., y + 22.);
        let ax = GRID_X + (access.x as f32 + 0.5) * TILE;
        let ay = GRID_Y + (access.y as f32 + 0.5) * TILE;
        let here = game.position == access;
        marker(
            ax,
            ay,
            if here {
                GOLD
            } else {
                Color::new(0.13, 0.40, 0.42, 0.38)
            },
        );
        if here {
            rounded(x + 1., y + 1., 50., 50., 9., with_alpha(GOLD, 0.16));
            outline(x + 1., y + 1., 50., 50., 9., 2., GOLD);
        }
        let mut name_color = with_alpha(CREAM, 0.85);
        let name: String = match station {
            Station::Pantry(i) => {
                rounded(x + 6., y + 9., 40., 32., 5., SHADE);
                rounded_gradient(
                    x + 6.,
                    y + 6.,
                    40.,
                    32.,
                    5.,
                    lighten(WOOD, 0.14),
                    darken(WOOD, 0.08),
                );
                panel(x + 6., y + 17., 40., 1., Color::new(0.2, 0.1, 0.05, 0.35));
                panel(x + 6., y + 28., 40., 1., Color::new(0.2, 0.1, 0.05, 0.35));
                ingredient(i, center.x, center.y - 1., 1.05, false);
                SHORT_INGREDIENTS[i].into()
            }
            Station::Chop => {
                rounded(x + 7., y + 11., 38., 26., 6., SHADE);
                rounded_gradient(
                    x + 7.,
                    y + 8.,
                    38.,
                    26.,
                    6.,
                    Color::new(0.86, 0.66, 0.44, 1.),
                    WOOD,
                );
                rounded(
                    x + 12.,
                    y + 17.,
                    22.,
                    5.,
                    2.,
                    Color::new(0.88, 0.92, 0.94, 1.),
                );
                rounded(x + 32., y + 16., 9., 7., 2., DARK);
                if game.chopped_ticks > 0 {
                    for k in 0..3 {
                        panel(x + 13. + k as f32 * 7., y + 27., 4., 3., GREEN);
                    }
                }
                "Chop".into()
            }
            Station::Mix => {
                oval(center.x, center.y + 4., 19., 6., SHADE);
                oval(center.x, center.y, 19., 12., CREAM);
                oval(center.x, center.y - 3., 16., 8., darken(TEAL, 0.25));
                for i in 0..4 {
                    if game.mixing & (1 << i) != 0 {
                        ingredient(
                            i,
                            center.x + (i as f32 - 1.5) * 7.,
                            center.y - 3.,
                            0.6,
                            true,
                        );
                    }
                }
                "Mix".into()
            }
            Station::Burner(i) => {
                dot(
                    center.x,
                    center.y - 3.,
                    17.,
                    Color::new(0.06, 0.08, 0.09, 1.),
                );
                dot(
                    center.x,
                    center.y - 3.,
                    13.5,
                    Color::new(0.20, 0.22, 0.22, 1.),
                );
                dot(
                    center.x,
                    center.y - 3.,
                    10.5,
                    Color::new(0.06, 0.08, 0.09, 1.),
                );
                if let Some(pot) = game.burners[i] {
                    let burnt = game.pot_burnt(pot);
                    let ready = game.pot_ready(pot);
                    let color = if burnt {
                        CORAL
                    } else if ready {
                        GREEN
                    } else {
                        GOLD
                    };
                    if !burnt {
                        // Flicker of flame around the pot.
                        for k in 0..6 {
                            let a =
                                k as f32 * std::f32::consts::TAU / 6. + game.ticks as f32 * 0.05;
                            let flick = 1. + ((game.ticks as f32) * 0.4 + k as f32).sin() * 0.25;
                            dot(
                                center.x + a.cos() * 14.,
                                center.y - 3. + a.sin() * 14.,
                                2.6 * flick,
                                Color::new(0.4, 0.75, 1., 0.85),
                            );
                        }
                    }
                    if ready {
                        dot(
                            center.x,
                            center.y - 3.,
                            22.,
                            with_alpha(GREEN, 0.12 + 0.12 * pulse),
                        );
                    }
                    let body = if burnt {
                        Color::new(0.16, 0.14, 0.13, 1.)
                    } else {
                        Color::new(0.22, 0.46, 0.62, 1.)
                    };
                    rounded_gradient(
                        center.x - 14.,
                        center.y - 14.,
                        28.,
                        20.,
                        5.,
                        lighten(body, 0.18),
                        darken(body, 0.1),
                    );
                    rounded(
                        center.x - 19.,
                        center.y - 15.,
                        38.,
                        4.,
                        2.,
                        if burnt { INKY } else { CREAM },
                    );
                    if burnt {
                        for k in 0..3 {
                            let a = (game.ticks as f32 / 18. + k as f32 * 2.1).sin();
                            dot(
                                center.x - 8. + k as f32 * 8. + a * 2.,
                                center.y - 24. - k as f32 * 3.,
                                4.,
                                Color::new(0.35, 0.35, 0.35, 0.5),
                            );
                        }
                    } else {
                        for k in 0..3 {
                            let a = (game.ticks as f32 / 15. + k as f32 * 1.8).sin();
                            dot(
                                center.x - 9. + k as f32 * 9.,
                                center.y - 23. + a * 3.,
                                2.,
                                with_alpha(CREAM, 0.8),
                            );
                        }
                    }
                    let f = if ready {
                        1. - (pot.ticks - RECIPES[pot.recipe].cook) as f32
                            / game.burn_grace() as f32
                    } else {
                        pot.ticks as f32 / RECIPES[pot.recipe].cook as f32
                    };
                    bar(
                        x + 6.,
                        y + 36.,
                        40.,
                        4.,
                        f,
                        Color::new(0., 0., 0., 0.4),
                        color,
                    );
                    name_color = color;
                    if burnt {
                        "Burnt!".into()
                    } else if ready {
                        "Ready".into()
                    } else {
                        "Cooking".into()
                    }
                } else {
                    format!("Pot {}", i + 1)
                }
            }
            Station::Plate => {
                oval(center.x, center.y + 2., 19., 7., SHADE);
                oval(center.x, center.y - 2., 19., 12., CREAM);
                oval(center.x, center.y - 2., 14., 8., darken(CREAM, 0.12));
                oval(center.x, center.y - 3., 12., 6., CREAM);
                "Plate".into()
            }
            Station::Serve => {
                rounded(x + 6., y + 6., 40., 32., 5., DARK);
                for k in 0..5 {
                    panel(
                        x + 6. + k as f32 * 8.,
                        y + 6.,
                        8.,
                        7.,
                        if k % 2 == 0 { CORAL } else { CREAM },
                    );
                }
                rounded(x + 4., y + 32., 44., 5., 2., WOOD);
                oval(center.x, center.y + 6., 7., 5., GOLD);
                panel(center.x - 8., center.y + 9., 16., 2., darken(GOLD, 0.3));
                dot(center.x, center.y, 1.8, GOLD);
                "Serve".into()
            }
            Station::Bin => {
                rounded(
                    x + 14.,
                    y + 10.,
                    24.,
                    26.,
                    4.,
                    Color::new(0.16, 0.24, 0.27, 1.),
                );
                panel(x + 20., y + 15., 2., 16., Color::new(1., 1., 1., 0.1));
                panel(x + 30., y + 15., 2., 16., Color::new(1., 1., 1., 0.1));
                rounded(x + 11., y + 6., 30., 5., 2., SAGE);
                "Bin".into()
            }
        };
        label(
            &name,
            x + 26.,
            y + 47.,
            46.,
            11.,
            name_color,
            TextAlign::Center,
        );
    }
    // The cook, with a gentle walking bob.
    let px = game.previous.x as f32 + (game.position.x - game.previous.x) as f32 * blend;
    let py = game.previous.y as f32 + (game.position.y - game.previous.y) as f32 * blend;
    let x = GRID_X + (px + 0.5) * TILE;
    let y = GRID_Y + (py + 0.5) * TILE;
    let bob = if game.move_wait > 0 {
        -(blend * std::f32::consts::PI).sin().abs() * 3.
    } else {
        ((game.ticks as f32) * 0.08).sin() * 0.7
    };
    let y = y + bob;
    oval(x, y + 16. - bob, 14., 5., Color::new(0.1, 0.2, 0.2, 0.3));
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
    match game.carry {
        Carry::Raw(i) => ingredient(i, x + 17., y + 8., 0.8, false),
        Carry::Chopped(i) => ingredient(i, x + 18., y + 8., 0.8, true),
        Carry::Mixed(r) | Carry::Cooked(r) => dish(r, x + 18., y + 7., 0.8, false),
        Carry::Plated(r) => dish(r, x + 18., y + 7., 0.8, true),
        _ => {}
    }
    if game.chopped_ticks > 0 {
        bar(
            x - 22.,
            y - 34.,
            44.,
            6.,
            game.chopped_ticks as f32 / game.chop_ticks() as f32,
            Color::new(0.03, 0.1, 0.12, 0.7),
            GREEN,
        );
    }
    rounded(28., 540., 588., 24., 8., Color::new(0.02, 0.06, 0.08, 0.6));
    dot(42., 552., 3.5, ACCENT);
    label(
        game.note,
        54.,
        flat(13., CREAM).centered_baseline(552.),
        552.,
        13.,
        CREAM,
        TextAlign::Left,
    );
}
fn sidebar(game: &Game) {
    let (l, r) = (668., 914.);
    card(650., 124., 282., 430., PANEL);
    rounded(668., 124., 56., 3., 1.5, ACCENT);
    eyebrow("RECIPE BOOK", l, 150., SAGE);
    let page = format!("{} / 6", game.book + 1);
    let pw = text_width(&page, flat(12., SAGE));
    label(&page, r, 150., 60., 12., SAGE, TextAlign::Right);
    keycap("C", "", r - pw - 28., 136., PAPER);
    let recipe = RECIPES[game.book];
    fit(recipe.name, l, 180., r - l, 21., CREAM);
    dot(r - 32., 214., 28., Color::new(1., 1., 1., 0.04));
    dish(game.book, r - 32., 212., 1.45, true);
    let mut j = 0;
    for (i, name) in INGREDIENTS.iter().enumerate() {
        if recipe.mask & (1 << i) != 0 {
            let y = 204. + j as f32 * 24.;
            ingredient(i, l + 10., y + 1., 0.75, false);
            label(name, l + 28., y + 6., 200., 14., CREAM, TextAlign::Left);
            j += 1;
        }
    }
    let y = 222. + j as f32 * 24.;
    label(flow(game.book), l, y, r - l, 12., GREEN, TextAlign::Left);
    if recipe.cook > 0 {
        label(
            &format!(
                "Cooks in {:.1}s  ·  burns {}s later",
                recipe.cook as f32 / 60.,
                game.burn_grace() / 60
            ),
            l,
            y + 20.,
            r - l,
            12.,
            GOLD,
            TextAlign::Left,
        );
    } else {
        label(
            "No burner needed",
            l,
            y + 20.,
            r - l,
            12.,
            SAGE,
            TextAlign::Left,
        );
    }
    divider(l, 360., r - l, Color::new(1., 1., 1., 0.12));
    eyebrow("IN HAND", l, 382., SAGE);
    let held = match game.carry {
        Carry::Empty => "Empty hands".into(),
        Carry::Raw(i) => format!("Raw {}", INGREDIENTS[i].to_lowercase()),
        Carry::Chopped(i) => format!("Prepped {}", INGREDIENTS[i].to_lowercase()),
        Carry::Mixed(r) => format!("Mixed {}", RECIPES[r].name),
        Carry::Cooked(r) => format!("Cooked {}", RECIPES[r].name),
        Carry::Plated(r) => format!("Plated {}", RECIPES[r].name),
    };
    let tx = match game.carry {
        Carry::Raw(i) => {
            ingredient(i, l + 10., 401., 0.7, false);
            l + 26.
        }
        Carry::Chopped(i) => {
            ingredient(i, l + 12., 401., 0.7, true);
            l + 26.
        }
        Carry::Mixed(r) | Carry::Cooked(r) => {
            dish(r, l + 13., 401., 0.7, false);
            l + 30.
        }
        Carry::Plated(r) => {
            dish(r, l + 13., 401., 0.7, true);
            l + 30.
        }
        Carry::Empty => l,
    };
    label(
        &held,
        tx,
        407.,
        r - tx,
        15.,
        if game.carry == Carry::Empty {
            SAGE
        } else {
            CREAM
        },
        TextAlign::Left,
    );
    let hint = match (game.station(), game.carry) {
        (Some(Station::Chop), Carry::Raw(_)) => Some(("Hold Space", "Prep it")),
        (Some(Station::Mix), Carry::Chopped(_)) => Some(("Space", "Add to bowl")),
        (Some(Station::Mix), Carry::Empty) => Some(("Space", "Lift the recipe")),
        (Some(Station::Burner(_)), Carry::Mixed(_)) => Some(("Space", "Start cooking")),
        (Some(Station::Burner(_)), Carry::Empty) => Some(("Space", "Take the ready pot")),
        (Some(Station::Plate), _) => Some(("Space", "Plate the food")),
        (Some(Station::Serve), _) => Some(("Space", "Serve a guest")),
        (Some(Station::Pantry(_)), Carry::Empty) => Some(("Space", "Buy for 2 coins")),
        _ => None,
    };
    match hint {
        Some((key, action)) => {
            keycap(key, action, l, 420., GOLD);
        }
        None => label(
            "Walk to a marked floor tile",
            l,
            434.,
            r - l,
            13.,
            SAGE,
            TextAlign::Left,
        ),
    }
    divider(l, 458., r - l, Color::new(1., 1., 1., 0.12));
    // Service clock and tallies.
    let left = game.deadline().saturating_sub(game.ticks);
    let secs = left.div_ceil(60);
    eyebrow("CLOSING IN", l, 480., SAGE);
    let urgent = secs <= 20;
    label(
        &format!("{}:{:02}", secs / 60, secs % 60),
        r,
        484.,
        120.,
        20.,
        if urgent { CORAL } else { CREAM },
        TextAlign::Right,
    );
    bar(
        l,
        492.,
        r - l,
        5.,
        left as f32 / game.deadline().max(1) as f32,
        Color::new(1., 1., 1., 0.08),
        if urgent { CORAL } else { TEAL },
    );
    let stats = [
        (
            "WASTE",
            game.waste.to_string(),
            if game.waste > 0 { CORAL } else { CREAM },
        ),
        ("RETRIES", game.retries.to_string(), CREAM),
    ];
    for (k, (name, value, tint)) in stats.into_iter().enumerate() {
        let x = l + k as f32 * 76.;
        eyebrow(name, x, 520., SAGE);
        label(&value, x, 542., 60., 17., tint, TextAlign::Left);
    }
    // Upgrade levels as pips.
    for (k, name) in ["Knife", "Shoes", "Stove"].iter().enumerate() {
        let y = 512. + k as f32 * 16.;
        label(name, r - 28., y, 60., 11., SAGE, TextAlign::Right);
        for p in 0..2u8 {
            rounded(
                r - 22. + f32::from(p) * 12.,
                y - 7.,
                9.,
                5.,
                2.5,
                if p < game.upgrades[k] {
                    GOLD
                } else {
                    Color::new(1., 1., 1., 0.14)
                },
            );
        }
    }
}
impl Game {
    fn scene(&self) {
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
            menu(self);
        } else {
            tickets(self);
            kitchen(self);
            sidebar(self);
        }
    }
}
impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        self.scene();
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!("Service {} of 7", self.shift()),
            objective: format!(
                "Serve {} of {} guests  ·  {}",
                self.content().quota,
                self.content().menu.len(),
                self.content().name
            ),
            status: format!(
                "{} coins   {} served / {} missed   {} total dishes",
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
}
