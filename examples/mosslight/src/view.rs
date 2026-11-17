//! Read-only dungeon art and the shared arcade adapter.
//!
//! Board tiles stay flat quads (they meet edge to edge); depth comes from a
//! three-quarter wall face, ambient occlusion strips and lantern falloff that
//! only recolor those quads. Rounded and anti-aliased shapes are reserved for
//! characters, pickups and the interface.
//!
//! The interface speaks in icons, meters and pips. Hovering a neighbouring
//! tile, a sidebar action or a camp relic previews its consequences by
//! stepping that exact command on a copy of the model, so a preview can never
//! disagree with the rules. Effects come only from the events the latest
//! executed tick recorded.
use crate::{
    content::{COLS, FLOORS, ROWS},
    model::{
        ACTIONS, BOARD_X as LEFT, BOARD_Y as TOP, Enemy, Event, Game, Phase, Plan, Pos, Species,
        TILE, Tile, action_at, cell_at, relic_at,
    },
};
use ferravex_arcade::shapes::{self, Fill, Shadow};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;

const MOSS: Color = Color::new(0.48, 0.76, 0.40, 1.);
const DEEP_MOSS: Color = Color::new(0.30, 0.47, 0.25, 1.);
const GOLD: Color = Color::new(1., 0.76, 0.34, 1.);
const RED: Color = Color::new(0.97, 0.37, 0.32, 1.);
const CELADON: Color = Color::new(0.74, 0.88, 0.74, 1.);
const BLUE: Color = Color::new(0.39, 0.75, 0.82, 1.);
/// Health, flasks and the blade keep one color each everywhere.
const HEART: Color = Color::new(0.96, 0.40, 0.42, 1.);
const ROSE: Color = Color::new(0.88, 0.40, 0.56, 1.);
const STEEL: Color = Color::new(0.80, 0.86, 0.92, 1.);
const CARD: Color = Color::new(0.075, 0.115, 0.105, 1.);
const RAISED: Color = Color::new(0.11, 0.16, 0.145, 1.);
/// Sidebar content column.
const SX: f32 = 636.;
const SR: f32 = 916.;
const SW: f32 = SR - SX;
/// Foe rows in the sidebar; authored floors hold at most four creatures.
const FOE_TOP: f32 = 356.;
const FOE_ROW: f32 = 47.;
const FOE_ROWS: usize = 4;
/// Where big floor callouts appear: over the board, clear of the sidebar.
const BOARD_CENTER: Vec2 = vec2(311., 300.);
/// Camp relics: icon, proper name, gain and color.
const RELICS: [(Icon, &str, &str, Color); 3] = [
    (Icon::Sword, "Sunsteel", "+1", STEEL),
    (Icon::Heart, "Rootheart", "+5", HEART),
    (Icon::Flame, "Long Wick", "+1", GOLD),
];

fn center(pos: Pos) -> (f32, f32) {
    (
        LEFT + pos.x as f32 * TILE + TILE * 0.5,
        TOP + pos.y as f32 * TILE + TILE * 0.5,
    )
}
fn at(pos: Pos) -> Vec2 {
    let (x, y) = center(pos);
    vec2(x, y)
}
fn rgba(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new(r, g, b, a)
}
/// Scale a color's light, keeping alpha.
fn lit(c: Color, light: f32) -> Color {
    Color::new(c.r * light, c.g * light, c.b * light, c.a)
}
/// Gentle sine wave on the presentation clock; `rate` is radians per 1/60 s.
fn wave(rate: f32, phase: f32) -> f32 {
    (clock() * 60. * rate + phase).sin()
}
/// Lantern falloff: tiles near the hero stay bright, distant ones dim a little.
fn lantern(game: &Game, pos: Pos) -> f32 {
    let dx = (pos.x - game.hero.x) as f32;
    let dy = (pos.y - game.hero.y) as f32;
    let d = (dx * dx + dy * dy).sqrt();
    1. - ((d - 1.5) / 8.).clamp(0., 1.) * 0.34
}

/// A small number formatted on the stack: labels are redrawn every frame.
#[derive(Clone, Copy)]
struct Num {
    buf: [u8; 12],
    start: u8,
}
impl Num {
    fn new(value: impl Into<u32>) -> Self {
        Self::signed(0, value)
    }
    /// `value` after an ASCII `sign` (`b'+'`, `b'-'` or `b'/'`; zero for none).
    fn signed(sign: u8, value: impl Into<u32>) -> Self {
        let mut value = value.into();
        let mut buf = [0; 12];
        let mut start = buf.len();
        loop {
            start -= 1;
            buf[start] = b'0' + (value % 10) as u8;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        if sign != 0 {
            start -= 1;
            buf[start] = sign;
        }
        Self {
            buf,
            start: start as u8,
        }
    }
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[usize::from(self.start)..]).unwrap_or("")
    }
}

/// Body text skips the drop shadow below display sizes.
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
fn label_centered(value: &str, cx: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, cx, y, ink(size, color).with_align(TextAlign::Center)).width
}
/// A label vertically centered on `cy`; returns its width.
fn label_mid(value: &str, x: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color);
    draw_text_styled(value, x, s.centered_baseline(cy), s).width
}
fn keycap_width(key: &str) -> f32 {
    (text_width(key, TextStyle::modern(13., PAPER)) + 12.).max(20.)
}
fn keycap_centered(key: &str, cx: f32, y: f32) {
    keycap(key, "", cx - keycap_width(key) * 0.5, y, PAPER);
}
fn pill_width(value: &str, size: f32) -> f32 {
    if value.is_empty() {
        size * 1.5
    } else {
        size * 2.3 + text_width(value, TextStyle::modern(size, PAPER))
    }
}
/// A dark pill holding an icon and a short value, centered on (`cx`, `cy`):
/// previews and badges drawn over the busy board.
fn pill(kind: Icon, value: &str, cx: f32, cy: f32, size: f32, color: Color) {
    let w = pill_width(value, size);
    let h = size * 1.5;
    let r = Rect::new(cx - w * 0.5, cy - h * 0.5, w, h);
    shapes::fill_rounded_rect(r, h * 0.5, rgba(0.03, 0.045, 0.04, 0.9 * color.a));
    shapes::stroke_rounded_rect(r, h * 0.5, 1.2, with_alpha(color, 0.7 * color.a));
    if value.is_empty() {
        icon(kind, cx, cy, size, color);
        return;
    }
    icon(kind, r.x + size, cy, size, color);
    let s = TextStyle::modern(size, lighten(color, 0.3));
    draw_text_styled(value, r.x + size * 1.8, s.centered_baseline(cy), s);
}

fn diamond(x: f32, y: f32, r: f32, color: Color) {
    draw_triangle(vec2(x, y - r), vec2(x + r, y), vec2(x, y + r), color);
    draw_triangle(vec2(x, y - r), vec2(x, y + r), vec2(x - r, y), color);
}
/// A four-point glint of radius `r`.
fn twinkle(x: f32, y: f32, r: f32, color: Color) {
    if r <= 0.3 {
        return;
    }
    let thin = r * 0.28;
    shapes::fill_polygon(
        &[
            vec2(x - r, y),
            vec2(x, y - thin),
            vec2(x + r, y),
            vec2(x, y + thin),
        ],
        color,
    );
    shapes::fill_polygon(
        &[
            vec2(x, y - r),
            vec2(x + thin, y),
            vec2(x, y + r),
            vec2(x - thin, y),
        ],
        color,
    );
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
            let bar = if open { with_alpha(GOLD, 0.28) } else { GOLD };
            draw_rectangle(
                x + 3.,
                y + 3.,
                32.,
                32.,
                rgba(0.06, 0.05, 0.02, if open { 0.3 } else { 0.55 }),
            );
            for offset in [-10., 0., 10.] {
                draw_rectangle(cx + offset - 2., y + 4., 4., 30., bar);
                if !open {
                    draw_rectangle(
                        cx + offset - 2.,
                        y + 4.,
                        1.5,
                        30.,
                        rgba(1., 0.95, 0.75, 0.45),
                    );
                }
            }
            draw_rectangle(
                x + 4.,
                cy - 3.,
                30.,
                5.,
                if open { bar } else { darken(GOLD, 0.2) },
            );
            if open {
                icon(Icon::Unlock, cx, cy, 14., with_alpha(MOSS, 0.75));
            } else {
                shapes::fill_circle(vec2(cx, cy), 9.5, rgba(0.08, 0.07, 0.03, 0.95));
                icon(Icon::Lock, cx, cy, 13., GOLD);
            }
        }
        Tile::Seed => {
            // This floor's key: it opens the gold seals and the stairs.
            let bob = wave(0.07, 0.) * 1.8;
            let pulse = 0.5 + 0.5 * wave(0.07, 1.);
            shapes::fill_circle(
                vec2(cx, cy + 2.),
                15.,
                Fill::radial(
                    vec2(cx, cy),
                    15.,
                    rgba(0.62, 0.52, 0.2, 0.5),
                    rgba(0.18, 0.28, 0.14, 0.0),
                ),
            );
            draw_ellipse(cx, cy + 11., 10., 3., 0., rgba(0., 0., 0., 0.35));
            shapes::stroke_circle(vec2(cx, cy), 15., 1.5, with_alpha(GOLD, 0.45 + pulse * 0.4));
            glow(cx, cy - 2., 26., rgba(1., 0.85, 0.45, 0.14 + pulse * 0.1));
            icons::draw_styled(
                Icon::Key,
                vec2(cx, cy - 2. + bob),
                22.,
                IconStyle::new(GOLD).shadow(rgba(0., 0., 0., 0.45)),
            );
            twinkle(
                cx + 8.,
                cy - 9. + bob * 0.5,
                3. * pulse,
                rgba(1., 0.97, 0.8, 0.9),
            );
        }
        Tile::Stairs => {
            let open = game.seed && !game.king_alive();
            let rim = if open {
                GOLD
            } else if game.seed {
                with_alpha(GOLD, 0.5)
            } else {
                MUTED
            };
            draw_rectangle(x + 3., y + 3., 32., 32., rgba(0.01, 0.02, 0.02, 0.85));
            for i in 0..5 {
                let f = i as f32;
                let step = lit(rgba(0.30, 0.36, 0.33, 1.), 1. - f * 0.15);
                draw_rectangle(x + 5. + f * 2., y + 6. + f * 5.5, 28. - f * 4., 4., step);
                draw_rectangle(x + 5. + f * 2., y + 6. + f * 5.5, 28. - f * 4., 1., rim);
            }
            if open {
                // Unlocked: the stairs breathe gold until taken.
                let pulse = 0.5 + 0.5 * wave(0.08, 0.);
                glow(cx, cy, 34., rgba(1., 0.78, 0.35, 0.12 + pulse * 0.14));
                outline(
                    x + 2.,
                    y + 2.,
                    34.,
                    34.,
                    5.,
                    1.5 + pulse,
                    with_alpha(GOLD, 0.55 + pulse * 0.4),
                );
            } else {
                // What still bars the way: the key, or the King's crown.
                let (badge, color) = if game.seed {
                    (Icon::Crown, RED)
                } else {
                    (Icon::Lock, GOLD)
                };
                shapes::fill_circle(vec2(x + 29., y + 9.), 8., rgba(0.04, 0.05, 0.05, 0.95));
                icon(badge, x + 29., y + 9., 11., color);
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
            // An occasional glint of trapped daylight.
            let glint = wave(0.045, pos.x as f32 * 1.7 + pos.y as f32);
            if glint > 0.55 {
                twinkle(
                    cx + 6.,
                    cy - 6.,
                    (glint - 0.55) * 14.,
                    rgba(1., 0.96, 0.8, 0.9),
                );
            }
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
                let s = wave(0.05, pos.x as f32) * 2.;
                glow(cx, cy + 1., 18., rgba(0.4, 0.85, 1., 0.1 + 0.05 * s.abs()));
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
/// Telegraphed attack cells: a breathing red wash and corner brackets.
/// Heavier blows read deeper; `focus` marks the hero's cell or a hovered foe.
fn danger(pos: Pos, damage: u8, focus: bool) {
    let (cx, cy) = center(pos);
    let x = cx - TILE * 0.5;
    let y = cy - TILE * 0.5;
    let pulse = 0.5 + 0.5 * wave(0.11, 0.);
    let weight = (f32::from(damage) / 7.).clamp(0.45, 1.3);
    let boost = if focus { 1.4 } else { 1. };
    draw_rectangle(
        x + 2.,
        y + 2.,
        34.,
        34.,
        rgba(0.78, 0.15, 0.11, (0.1 + 0.1 * weight + pulse * 0.1) * boost),
    );
    let inset = 3. + (1. - pulse) * 2.;
    let k = 7. + pulse * 3.;
    let c = with_alpha(RED, 0.65 + pulse * 0.35);
    let width = if focus { 3. } else { 2. };
    let (l, r, t, b) = (x + inset, x + TILE - inset, y + inset, y + TILE - inset);
    for (px, py, sx, sy) in [
        (l, t, 1., 1.),
        (r, t, -1., 1.),
        (l, b, 1., -1.),
        (r, b, -1., -1.),
    ] {
        draw_line(px, py, px + sx * k, py, width, c);
        draw_line(px, py, px, py + sy * k, width, c);
    }
}
/// A knight striking from two tiles away: dashes link it to its target cell.
fn lane(from: Pos, to: Pos) {
    let (a, b) = (at(from), at(to));
    let d = b - a;
    let len = d.length();
    if len < 1. {
        return;
    }
    let dir = d / len;
    let pulse = 0.5 + 0.5 * wave(0.11, 0.);
    let mut s = 16.;
    while s < len - 14. {
        let p0 = a + dir * s;
        let p1 = a + dir * (s + 5.).min(len - 14.);
        draw_line(
            p0.x,
            p0.y,
            p1.x,
            p1.y,
            2.,
            with_alpha(RED, 0.3 + pulse * 0.35),
        );
        s += 10.;
    }
}
/// The lantern burst's reach (two tiles, through thin walls) while previewing.
fn reach(game: &Game) {
    let pulse = 0.5 + 0.5 * wave(0.09, 0.);
    for dy in -2..=2 {
        for dx in -2_i32..=2 {
            let pos = game.hero.offset(dx, dy);
            if (dx == 0 && dy == 0) || dx.abs() + dy.abs() > 2 || !pos.inside() {
                continue;
            }
            let (cx, cy) = center(pos);
            draw_rectangle(
                cx - 17.,
                cy - 17.,
                34.,
                34.,
                rgba(1., 0.76, 0.34, 0.08 + 0.07 * pulse),
            );
            outline(cx - 17., cy - 17., 34., 34., 5., 1., with_alpha(GOLD, 0.4));
        }
    }
}
/// Hover highlight: gold for a step, red for a strike, grey when refused and
/// faint for tiles out of reach.
fn cursor(game: &Game, cell: Pos, preview: Option<&Preview>) {
    let (cx, cy) = center(cell);
    let adjacent = game.hero.distance(cell) == 1;
    let accepted = adjacent && preview.is_some_and(|p| p.accepted);
    let color = if !adjacent {
        with_alpha(PAPER, 0.3)
    } else if !accepted {
        with_alpha(MUTED, 0.5)
    } else if game.enemy_at(cell).is_some() {
        RED
    } else {
        GOLD
    };
    if accepted {
        let pulse = 0.5 + 0.5 * wave(0.12, 0.);
        draw_rectangle(
            cx - 17.,
            cy - 17.,
            34.,
            34.,
            with_alpha(color, 0.08 + 0.07 * pulse),
        );
    }
    outline(
        cx - 18.,
        cy - 18.,
        36.,
        36.,
        7.,
        if adjacent { 2. } else { 1.5 },
        color,
    );
}
fn species_color(species: Species) -> Color {
    match species {
        Species::Sprout => MOSS,
        Species::Knight => Color::new(0.80, 0.64, 0.42, 1.),
        Species::Spore => Color::new(0.80, 0.52, 0.82, 1.),
        Species::King => GOLD,
    }
}
/// A creature's body centered near (`x`, `y`): board pieces and portraits.
fn creature(species: Species, x: f32, y: f32) {
    match species {
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
}
/// A foe's remaining health; a previewed blow blinks the part it would take.
fn foe_meter(x: f32, y: f32, w: f32, h: f32, enemy: Enemy, after: u8) {
    let max = f32::from(enemy.max_health.max(1));
    meter(x, y, w, h, f32::from(after) / max, RED);
    if after < enemy.health {
        let pulse = 0.5 + 0.5 * wave(0.2, 0.);
        let a = x + w * f32::from(after) / max;
        let b = x + w * f32::from(enemy.health) / max;
        shapes::fill_rounded_rect(
            Rect::new(a, y, (b - a).max(1.), h),
            0.,
            with_alpha(PAPER, 0.35 + 0.5 * pulse),
        );
    }
}
/// What a creature will do after the hero's next action, as a small badge
/// drawn at `scale`: resting, moving or about to strike.
fn intent(plan: Plan, x: f32, y: f32, scale: f32) {
    match plan {
        Plan::Rest => {
            let drift = ((clock() * 60.) % 90.) / 90.;
            label(
                "z",
                x - 3. * scale,
                y + 4. * scale - drift * 5.,
                11. * scale,
                with_alpha(CELADON, 0.85 - drift * 0.6),
            );
        }
        Plan::Move(_) => icon(Icon::ArrowRight, x, y, 13. * scale, GOLD),
        _ => {
            let pulse = 0.5 + 0.5 * wave(0.15, 0.);
            glow(x, y, (10. + 3. * pulse) * scale, with_alpha(RED, 0.3));
            shapes::fill_circle(vec2(x, y), (6.5 + pulse) * scale, RED);
            draw_rectangle(x - scale, y - 4. * scale, 2. * scale, 5. * scale, PAPER);
            draw_rectangle(x - scale, y + 2. * scale, 2. * scale, 2. * scale, PAPER);
        }
    }
}
/// The blow a preview lands on the creature at `pos`: (damage, killed).
fn hit_at(next: &Game, pos: Pos) -> Option<(u8, bool)> {
    next.events().find_map(|event| match event {
        Event::Hit {
            at, amount, killed, ..
        } if at == pos => Some((amount, killed)),
        _ => None,
    })
}
fn enemy(enemy: Enemy, slot: usize, focused: bool, next: Option<&Game>) {
    let (x, base_y) = center(enemy.pos);
    let bob = wave(0.06, slot as f32 * 1.7);
    let y = base_y + bob;
    if focused {
        glow(x, base_y, 28., with_alpha(RED, 0.14));
        shapes::stroke_circle(vec2(x, base_y + 2.), 19., 1.5, with_alpha(RED, 0.6));
    }
    draw_ellipse(
        x,
        base_y + 13.,
        13. - bob * 0.5,
        4.5,
        0.,
        rgba(0.01, 0.02, 0.02, 0.6),
    );
    creature(enemy.species, x, y);
    match enemy.plan {
        Plan::Move(target) => {
            let (tx, ty) = center(target);
            let (ex, ey) = (x + (tx - x) * 0.42, base_y + (ty - base_y) * 0.42);
            draw_line(x, base_y, ex, ey, 2., with_alpha(GOLD, 0.8));
            diamond(ex, ey, 3.5, GOLD);
        }
        plan => intent(plan, x + 13., y - 14., 1.),
    }
    let hit = next.and_then(|next| hit_at(next, enemy.pos));
    if enemy.health < enemy.max_health || hit.is_some() {
        let after = hit.map_or(enemy.health, |(amount, _)| {
            enemy.health.saturating_sub(amount)
        });
        foe_meter(x - 13., base_y + 17., 26., 4., enemy, after);
    }
    if let Some((_, true)) = hit {
        let top = if enemy.species == Species::King {
            34.
        } else {
            27.
        };
        let pulse = 0.5 + 0.5 * wave(0.15, 0.);
        pill(
            Icon::Skull,
            "",
            x,
            y - top,
            14.,
            with_alpha(GOLD, 0.75 + 0.25 * pulse),
        );
    }
}
fn hero(game: &Game, hover: Hover, preview: Option<&Preview>) {
    let (x, base_y) = center(game.hero);
    let bob = wave(0.05, 0.) * 1.2;
    let y = base_y + bob;
    let threatened = game.phase == Phase::Explore && game.danger(game.hero) > 0;
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
    let sway = wave(0.05, 1.3) * 0.8;
    let (lx, ly) = (x + 13.5 + sway, y + 4.);
    draw_rectangle(lx - 3.5, ly - 4., 7., 9., darken(GOLD, 0.25));
    draw_rectangle(lx - 2.5, ly - 3., 5., 7., Color::new(1., 0.96, 0.68, 1.));
    draw_rectangle(lx - 4., ly - 5., 8., 1.5, GOLD);
    // Self-targeted previews: a guard bubble, a healing glow.
    let accepted = preview.is_some_and(|p| p.accepted);
    let pulse = 0.5 + 0.5 * wave(0.12, 0.);
    match hover {
        Hover::Action(0) if accepted && game.tile(game.hero) != Tile::Stairs => {
            shapes::fill_circle(vec2(x, base_y), 21., with_alpha(BLUE, 0.1 + 0.06 * pulse));
            shapes::stroke_circle(
                vec2(x, base_y),
                21.,
                2.,
                with_alpha(BLUE, 0.55 + 0.35 * pulse),
            );
        }
        Hover::Action(1) if accepted => {
            glow(x, base_y, 30., with_alpha(MOSS, 0.18 + 0.12 * pulse));
        }
        _ => {}
    }
}
/// Ambient fireflies drifting over the floor.
fn motes() {
    let t = clock();
    for i in 0..9 {
        let f = i as f32;
        let x = LEFT + 40. + (f * 61.7) % 490. + (t * (0.21 + f * 0.023) + f).sin() * 22.;
        let y = TOP + 36. + (f * 47.3) % 340. + (t * (0.17 + f * 0.019) + f * 2.3).cos() * 16.;
        let a = 0.3 + 0.3 * (t * (1.1 + f * 0.13) + f * 0.7).sin();
        if a <= 0.02 {
            continue;
        }
        glow(x, y, 8., rgba(0.7, 1., 0.5, a * 0.35));
        shapes::fill_circle(vec2(x, y), 1.4, rgba(0.92, 1., 0.75, a));
    }
}
/// Over-the-hero prompts: Space on open stairs, and the blow that lands if
/// the hero stays put (hidden while a preview shows the hovered outcome).
fn hero_badges(game: &Game, previewing: bool) {
    if game.phase != Phase::Explore {
        return;
    }
    let t = clock();
    let (x, y) = center(game.hero);
    let mut top = y - 34.;
    if game.tile(game.hero) == Tile::Stairs && game.seed && !game.king_alive() {
        let b = bob(t, 1.1, 2.);
        let w = 16. + 6. + keycap_width("Space");
        let left = x - w * 0.5;
        let r = Rect::new(left - 6., top - 13. + b, w + 12., 26.);
        shapes::fill_rounded_rect(r, 13., rgba(0.06, 0.05, 0.02, 0.88));
        shapes::stroke_rounded_rect(r, 13., 1.2, with_alpha(GOLD, 0.8));
        icon(Icon::ArrowDown, left + 8., top + b, 16., GOLD);
        keycap("Space", "", left + 22., top - 10. + b, PAPER);
        top -= 30.;
    }
    let threat = game.danger(game.hero);
    if threat > 0 && !previewing {
        let p = pulse(t, 1.6);
        pill(
            Icon::Heart,
            Num::signed(b'-', threat).as_str(),
            x,
            top + bob(t, 1.6, 1.),
            13.,
            with_alpha(HEART, 0.78 + 0.22 * p),
        );
    }
}
/// Up to five preview chips: an icon with a number, a word or nothing.
struct Chips {
    list: [(Icon, Option<Num>, &'static str, Color); 5],
    len: usize,
}
impl Chips {
    fn new() -> Self {
        Self {
            list: [(Icon::Star, None, "", PAPER); 5],
            len: 0,
        }
    }
    fn push(&mut self, kind: Icon, value: Option<Num>, word: &'static str, color: Color) {
        if let Some(slot) = self.list.get_mut(self.len) {
            *slot = (kind, value, word, color);
            self.len += 1;
        }
    }
    fn draw(&self, cx: f32, cy: f32) {
        let size = 13.;
        let gap = 4.;
        let chips = &self.list[..self.len];
        let total = chips
            .iter()
            .map(|(_, num, word, _)| pill_width(chip_text(num, word), size) + gap)
            .sum::<f32>()
            - gap;
        let right = LEFT + TILE * COLS as f32 - 4. - total;
        let mut x = (cx - total * 0.5).clamp(LEFT + 4., right.max(LEFT + 4.));
        for (kind, num, word, color) in chips {
            let text = chip_text(num, word);
            let w = pill_width(text, size);
            pill(*kind, text, x + w * 0.5, cy, size, *color);
            x += w + gap;
        }
    }
}
fn chip_text<'a>(num: &'a Option<Num>, word: &'static str) -> &'a str {
    num.as_ref().map_or(word, Num::as_str)
}
/// What the hovered command would do, as chips over `anchor`. Blows on foes
/// show on the foes themselves unless `hits` asks for them here.
fn outcome(game: &Game, preview: &Preview, anchor: Pos, hits: bool) {
    let next = &preview.next;
    let mut chips = Chips::new();
    if !preview.accepted {
        // A refusal previews its reason instead.
        for event in next.events() {
            match event {
                Event::Locked(_) => chips.push(Icon::Lock, None, "", RED),
                Event::Crowned(_) => chips.push(Icon::Crown, None, "", RED),
                Event::NoDrink { empty: true } => {
                    chips.push(Icon::Potion, Some(Num::new(0_u32)), "", MUTED)
                }
                Event::NoDrink { empty: false } => chips.push(Icon::Heart, None, "Full", MOSS),
                Event::NoBurst { empty: true } => {
                    chips.push(Icon::Flame, Some(Num::new(0_u32)), "", MUTED)
                }
                Event::NoBurst { empty: false } => chips.push(Icon::Flame, None, "Too far", MUTED),
                _ => {}
            }
        }
    } else {
        let change = i32::from(next.health) - i32::from(game.health);
        if next.phase == Phase::Lost {
            chips.push(Icon::Skull, None, "", RED);
        } else if change != 0 {
            let (sign, color) = if change < 0 {
                (b'-', HEART)
            } else {
                (b'+', MOSS)
            };
            chips.push(
                Icon::Heart,
                Some(Num::signed(sign, change.unsigned_abs())),
                "",
                color,
            );
        }
        for event in next.events() {
            match event {
                Event::Hit { killed: true, .. } if hits => chips.push(Icon::Skull, None, "", GOLD),
                Event::Hit { amount, .. } if hits => {
                    chips.push(Icon::Sword, Some(Num::signed(b'-', amount)), "", STEEL)
                }
                Event::Key(_) => chips.push(Icon::Key, None, "", GOLD),
                Event::Amber(_) => {
                    chips.push(Icon::Gem, Some(Num::signed(b'+', 140_u32)), "", GOLD)
                }
                Event::Flask(_) => {
                    chips.push(Icon::Potion, Some(Num::signed(b'+', 1_u32)), "", ROSE)
                }
                Event::BeltFull(_) => chips.push(Icon::Potion, None, "Full", MUTED),
                Event::Well { flask, charge, .. } => {
                    if flask > 0 {
                        chips.push(Icon::Potion, Some(Num::signed(b'+', flask)), "", ROSE);
                    }
                    if charge > 0 {
                        chips.push(Icon::Flame, Some(Num::signed(b'+', charge)), "", GOLD);
                    }
                }
                Event::Cleared(_) => chips.push(Icon::ArrowDown, None, "", GOLD),
                Event::Won(_) => chips.push(Icon::Trophy, None, "", GOLD),
                _ => {}
            }
        }
    }
    if chips.len > 0 {
        // Above the anchor, unless the hero stands right there.
        let (x, y) = center(anchor);
        let below = anchor.offset(0, -1) == game.hero;
        chips.draw(x, if below { y + 31. } else { y - 31. });
    }
}

/// The consequence of a command, stepped on a copy of the model.
struct Preview {
    next: Game,
    accepted: bool,
}
fn simulate(game: &Game, input: &Input) -> Preview {
    let mut next = game.clone();
    next.step(input);
    let accepted = next.turns != game.turns || next.phase != game.phase || next.depth != game.depth;
    Preview { next, accepted }
}
/// What the pointer is over this frame (presentation only).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hover {
    None,
    Cell(Pos),
    Action(usize),
    Relic(usize),
    Foe(usize),
}
fn hover_at(game: &Game, p: Vec2) -> Hover {
    match game.phase {
        Phase::Camp => relic_at(p).map_or(Hover::None, Hover::Relic),
        Phase::Explore => action_at(p)
            .map(Hover::Action)
            .or_else(|| cell_at(p).map(Hover::Cell))
            .or_else(|| foe_at(game, p).map(Hover::Foe))
            .unwrap_or(Hover::None),
        Phase::Won | Phase::Lost => Hover::None,
    }
}
/// Preview the hovered command, or in camp the keyboard's selected relic.
fn preview_for(game: &Game, hover: Hover, p: Vec2) -> Option<Preview> {
    let click = Input {
        click: true,
        pointer: p,
        ..Input::default()
    };
    match hover {
        Hover::Cell(cell) if game.hero.distance(cell) == 1 => Some(simulate(game, &click)),
        Hover::Action(_) | Hover::Relic(_) => Some(simulate(game, &click)),
        _ if game.phase == Phase::Camp => Some(simulate(
            game,
            &Input {
                primary: true,
                ..Input::default()
            },
        )),
        _ => None,
    }
}
/// The creature the player is looking at: hovered on the board or in the
/// list, else the last one clicked.
fn focus(game: &Game, hover: Hover) -> Option<usize> {
    match hover {
        Hover::Cell(cell) => game.enemy_at(cell),
        Hover::Foe(slot) => Some(slot),
        _ => game.inspected.and_then(|pos| game.enemy_at(pos)),
    }
}

fn board(game: &Game, hover: Hover, focused: Option<usize>, preview: Option<&Preview>) {
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
            tile(game, Pos::new(x as i32, y as i32));
        }
    }
    let explore = game.phase == Phase::Explore;
    if explore && hover == Hover::Action(2) {
        reach(game);
    }
    let focus_enemy = focused.and_then(|slot| game.enemies[slot]);
    for y in 0..ROWS {
        for x in 0..COLS {
            let pos = Pos::new(x as i32, y as i32);
            if game.tile(pos) == Tile::Wall {
                continue;
            }
            let damage = game.danger(pos);
            if damage > 0 {
                let bright =
                    pos == game.hero || focus_enemy.is_some_and(|e| game.threatens(e, pos));
                danger(pos, damage, bright);
            }
        }
    }
    for enemy in game.enemies.iter().flatten() {
        if let Plan::Strike(target) = enemy.plan
            && enemy.pos.distance(target) > 1
        {
            lane(enemy.pos, target);
        }
    }
    if let Hover::Cell(cell) = hover {
        cursor(game, cell, preview);
    }
    if let Some(pos) = game.inspected.filter(|pos| *pos != game.hero && explore) {
        let (x, y) = center(pos);
        outline(x - 18., y - 18., 36., 36., 6., 1.5, with_alpha(BLUE, 0.7));
    }
    let next = preview.filter(|p| p.accepted).map(|p| &p.next);
    for (slot, e) in game.enemies.iter().enumerate() {
        if let Some(e) = e {
            enemy(*e, slot, focused == Some(slot), next);
        }
    }
    hero(game, hover, preview);
    if game.phase != Phase::Camp {
        let t = clock();
        let flicker = 1. + 0.05 * (t * 9.1).sin() + 0.035 * (t * 14.3 + 1.).sin();
        let (x, y) = center(game.hero);
        glow(
            x + 13.,
            y + 4.,
            54. * flicker,
            rgba(1., 0.72, 0.3, 0.2 * flicker),
        );
    }
    motes();
    let previewing =
        preview.is_some_and(|p| p.accepted) && matches!(hover, Hover::Cell(_) | Hover::Action(_));
    hero_badges(game, previewing);
    if let Some(preview) = preview.filter(|_| explore) {
        match hover {
            Hover::Cell(cell) => outcome(game, preview, cell, true),
            Hover::Action(_) => outcome(game, preview, game.hero, false),
            _ => {}
        }
    }
}

/// Health as a heart and a number, with the blade's damage beside it.
fn vitals(game: &Game, next: Option<&Game>) {
    let t = clock();
    let cy = 150.;
    let low = u16::from(game.health) * 3 <= u16::from(game.max_health);
    let beat = if low { pulse(t, 1.8) } else { 0. };
    icons::draw_styled(
        Icon::Heart,
        vec2(SX + 13., cy),
        26. * (1. + 0.12 * beat),
        IconStyle::new(HEART).shadow(rgba(0., 0., 0., 0.4)),
    );
    let mut x = SX + 34.;
    x += label_mid(
        Num::new(game.health).as_str(),
        x,
        cy,
        26.,
        if low { RED } else { PAPER },
    );
    x += 3.
        + label_mid(
            Num::signed(b'/', game.max_health).as_str(),
            x + 3.,
            cy + 3.,
            16.,
            MUTED,
        );
    if let Some(next) = next.filter(|n| n.health != game.health) {
        let color = if next.health > game.health {
            MOSS
        } else {
            HEART
        };
        icon(Icon::ArrowRight, x + 13., cy, 13., color);
        label_mid(Num::new(next.health).as_str(), x + 24., cy, 22., color);
    }
    // The blade: damage per strike (bursts add three).
    let value = Num::new(game.attack);
    let gain = next.is_some_and(|n| n.attack > game.attack);
    let s = style(24., STEEL);
    let vw = text_width(value.as_str(), s);
    let extra = if gain { 38. } else { 0. };
    let x0 = SR - extra - vw - 28.;
    icons::draw_styled(
        Icon::Sword,
        vec2(x0 + 10., cy),
        22.,
        IconStyle::new(STEEL).shadow(rgba(0., 0., 0., 0.4)),
    );
    draw_text_styled(value.as_str(), x0 + 28., s.centered_baseline(cy), s);
    if let Some(next) = next.filter(|_| gain) {
        icon(Icon::ArrowRight, x0 + 28. + vw + 11., cy, 13., MOSS);
        label_mid(
            Num::new(next.attack).as_str(),
            x0 + 28. + vw + 21.,
            cy,
            22.,
            MOSS,
        );
    }
}
/// Segmented health bar. The ghost shows the hovered command's outcome, or
/// the blow that lands if the hero stays put.
fn health_bar(game: &Game, next: Option<&Game>, threat: u8) {
    let (x, y, w, h) = (SX, 172., SW, 13.);
    let t = clock();
    let next_max = next.map_or(game.max_health, |n| n.max_health);
    let scale = f32::from(game.max_health.max(next_max).max(1));
    let px = |hp: u8| x + w * f32::from(hp) / scale;
    let low = u16::from(game.health) * 3 <= u16::from(game.max_health);
    let color = if low { RED } else { MOSS };
    let track = Rect::new(x, y, w, h);
    shapes::fill_rounded_rect(track, 4., rgba(0.02, 0.03, 0.03, 0.65));
    if next_max > game.max_health {
        // A relic grows the bar.
        let a = px(game.max_health);
        shapes::fill_rounded_rect(
            Rect::new(a, y, x + w - a, h),
            4.,
            with_alpha(MOSS, 0.12 + 0.12 * pulse(t, 1.2)),
        );
    }
    let end = px(game.health);
    if game.health > 0 {
        let fill = Rect::new(x, y, (end - x).max(h), h);
        shapes::fill_rounded_rect(fill, 4., Fill::vertical(fill, lighten(color, 0.18), color));
        shapes::fill_rounded_rect(
            Rect::new(x + 4., y + 1.5, (end - x - 8.).max(0.), 1.5),
            0.,
            rgba(1., 1., 1., 0.25),
        );
    }
    let target = match next {
        Some(next) => Some(next.health),
        None => (threat > 0).then(|| game.health.saturating_sub(threat)),
    };
    if let Some(target) = target.filter(|target| *target != game.health) {
        let blink = 0.5 + 0.5 * (t * 6.).sin();
        if target < game.health {
            let a = px(target);
            shapes::fill_rounded_rect(
                Rect::new(a, y, end - a, h),
                0.,
                with_alpha(shapes::mix(HEART, PAPER, 0.4 * blink), 0.9),
            );
        } else {
            let b = px(target);
            shapes::fill_rounded_rect(
                Rect::new(end, y, b - end, h),
                0.,
                with_alpha(MOSS, 0.3 + 0.45 * blink),
            );
        }
    }
    // Notches every five health make blows countable.
    let mut hp = 5_u8;
    while hp < game.max_health.max(next_max) {
        draw_rectangle(px(hp) - 0.5, y + 2., 1., h - 4., rgba(0., 0., 0., 0.4));
        hp = hp.saturating_add(5);
    }
}
fn foes_in_reach(game: &Game) -> usize {
    game.enemies
        .iter()
        .flatten()
        .filter(|e| game.hero.distance(e.pos) <= 2)
        .count()
}
/// Supply counts for a row of pips: `have` now and `after` the previewed
/// command, within `capacity` slots (a relic may add `slots - capacity`).
#[derive(Clone, Copy)]
struct Supply {
    slots: u8,
    capacity: u8,
    have: u8,
    after: u8,
}
/// A row of supply pips: gains pulse in, spends blink out and new capacity
/// shows as a ghost slot. Flasks draw as vials, charges as embers.
fn supply_pips(cx: f32, cy: f32, supply: Supply, vial: bool, color: Color) {
    let Supply {
        slots,
        capacity,
        have,
        after,
    } = supply;
    let gap = if slots > 6 { 9. } else { 11. };
    let x0 = cx - gap * f32::from(slots.saturating_sub(1)) * 0.5;
    let blink = 0.5 + 0.5 * (clock() * 6.).sin();
    for i in 0..slots {
        let x = x0 + f32::from(i) * gap;
        let (full, will) = (i < have, i < after);
        let fill = match (full, will) {
            (true, true) => Some(color),
            (true, false) => Some(with_alpha(color, 0.25 + 0.6 * blink)),
            (false, true) => Some(with_alpha(lighten(color, 0.3), 0.3 + 0.6 * blink)),
            (false, false) => None,
        };
        let edge = if i >= capacity {
            with_alpha(lighten(color, 0.3), 0.4 + 0.4 * blink)
        } else {
            with_alpha(color, 0.45)
        };
        if vial {
            let r = Rect::new(x - 3., cy - 5., 6., 10.);
            match fill {
                Some(c) => shapes::fill_rounded_rect(r, 2.5, c),
                None => shapes::stroke_rounded_rect(r, 2.5, 1.2, edge),
            }
        } else {
            match fill {
                Some(c) => shapes::fill_circle(vec2(x, cy), 3.8, c),
                None => shapes::stroke_circle(vec2(x, cy), 3.4, 1.2, edge),
            }
        }
    }
}
/// The three action buttons: guard (descend on open stairs), flask, burst.
/// Each mirrors its key; usable ones invite with a glow when they would help.
fn actions(game: &Game, hover: Hover, next: Option<&Game>) {
    let t = clock();
    let explore = game.phase == Phase::Explore;
    let on_stairs = explore && game.tile(game.hero) == Tile::Stairs;
    let open_stairs = on_stairs && game.seed && !game.king_alive();
    let in_reach = foes_in_reach(game);
    for (i, button) in ACTIONS.iter().enumerate() {
        let hovered = explore && hover == Hover::Action(i);
        let usable = explore
            && match i {
                0 => !on_stairs || open_stairs,
                1 => game.flasks > 0 && game.health < game.max_health,
                _ => game.charge > 0 && in_reach > 0,
            };
        let invite = usable
            && match i {
                0 => open_stairs,
                1 => u16::from(game.health) * 2 <= u16::from(game.max_health),
                _ => in_reach >= 2,
            };
        let accent = match i {
            0 if on_stairs => GOLD,
            0 => BLUE,
            1 => ROSE,
            _ => GOLD,
        };
        let lift = if hovered { 3. } else { 0. };
        let r = Rect::new(button.x, button.y - lift, button.w, button.h);
        let breath = pulse(t, 1.1);
        if invite {
            shapes::drop_shadow(
                r,
                12.,
                Shadow::soft(with_alpha(accent, 0.22 + 0.22 * breath)).blur(16.),
            );
        }
        if hovered {
            shapes::drop_shadow(
                r,
                12.,
                Shadow::soft(rgba(0., 0., 0., 0.45))
                    .offset(vec2(0., 5.))
                    .blur(12.),
            );
        }
        let base = if hovered && usable {
            lighten(RAISED, 0.07)
        } else {
            RAISED
        };
        shapes::fill_rounded_rect(r, 12., Fill::vertical(r, lighten(base, 0.04), base));
        let rim = if hovered {
            0.85
        } else if invite {
            0.35 + 0.35 * breath
        } else {
            0.12
        };
        shapes::stroke_rounded_rect(
            r,
            12.,
            if hovered { 1.5 } else { 1. },
            with_alpha(accent, rim),
        );
        let alpha = if usable { 1. } else { 0.4 };
        let kind = match i {
            0 if on_stairs => Icon::ArrowDown,
            0 => Icon::Shield,
            1 => Icon::Potion,
            _ => Icon::Flame,
        };
        let cx = r.center().x;
        let float = if invite { bob(t, 1.1, 2.) } else { 0. };
        let flicker = if i == 2 && usable {
            1. + 0.05 * (t * 10.).sin()
        } else {
            1.
        };
        icons::draw_styled(
            kind,
            vec2(cx, r.y + 30. + float),
            30. * flicker,
            IconStyle::new(with_alpha(accent, alpha)).shadow(rgba(0., 0., 0., 0.4 * alpha)),
        );
        let row = r.y + 57.;
        match i {
            0 if on_stairs && !open_stairs => {
                let (badge, color) = if game.seed {
                    (Icon::Crown, RED)
                } else {
                    (Icon::Key, GOLD)
                };
                icon(badge, cx - 8., row, 13., with_alpha(color, 0.9));
                icon(Icon::Cross, cx + 8., row, 10., with_alpha(RED, 0.9));
            }
            0 if !on_stairs => {
                // Each blow lands three weaker.
                let w = 12. + 3. + text_width("-3", TextStyle::modern(13., PAPER));
                icon(
                    Icon::Sword,
                    cx - w * 0.5 + 6.,
                    row,
                    12.,
                    with_alpha(MUTED, alpha),
                );
                label_mid("-3", cx - w * 0.5 + 15., row, 13., with_alpha(BLUE, alpha));
            }
            0 => {}
            1 => supply_pips(
                cx,
                row,
                Supply {
                    slots: 5,
                    capacity: 5,
                    have: game.flasks,
                    after: next.map_or(game.flasks, |n| n.flasks),
                },
                true,
                with_alpha(ROSE, alpha.max(0.7)),
            ),
            _ => supply_pips(
                cx,
                row,
                Supply {
                    slots: game.max_charge.max(next.map_or(0, |n| n.max_charge)),
                    capacity: game.max_charge,
                    have: game.charge,
                    after: next.map_or(game.charge, |n| n.charge),
                },
                false,
                with_alpha(GOLD, alpha.max(0.7)),
            ),
        }
        keycap_centered(["Space", "X", "C"][i], cx, r.y + r.h - 27.);
    }
}
/// Run stats: turns taken, this floor's amber jars, foes defeated.
fn stats(game: &Game) {
    let cy = 320.;
    icon_value(
        Icon::Hourglass,
        Num::new(game.turns).as_str(),
        SX,
        cy,
        16.,
        MUTED,
    );
    let total: usize = FLOORS[game.depth]
        .map
        .iter()
        .map(|row| row.bytes().filter(|b| *b == b'$').count())
        .sum();
    let left = game.board.iter().filter(|t| **t == Tile::Amber).count();
    let found = total.saturating_sub(left);
    let x0 = SX + 112.;
    for i in 0..total {
        let x = x0 + i as f32 * 22.;
        if i < found {
            glow(x, cy, 12., rgba(1., 0.76, 0.34, 0.2));
            icon(Icon::Gem, x, cy, 17., GOLD);
        } else {
            icon(Icon::Gem, x, cy, 17., with_alpha(MUTED, 0.3));
        }
    }
    let kills = Num::new(game.kills);
    let w = 16. * 1.25 + text_width(kills.as_str(), style(16., MUTED));
    icon_value(Icon::Skull, kills.as_str(), SR - w, cy, 16., MUTED);
}
fn foe_row(row: usize) -> Rect {
    Rect::new(SX, FOE_TOP + row as f32 * FOE_ROW, SW, FOE_ROW - 6.)
}
/// Living creatures in list order: (row, slot, creature).
fn foe_rows(game: &Game) -> impl Iterator<Item = (usize, usize, Enemy)> + '_ {
    game.enemies
        .iter()
        .enumerate()
        .filter_map(|(slot, e)| e.map(|e| (slot, e)))
        .take(FOE_ROWS)
        .enumerate()
        .map(|(row, (slot, e))| (row, slot, e))
}
fn foe_at(game: &Game, p: Vec2) -> Option<usize> {
    foe_rows(game)
        .find(|(row, _, _)| foe_row(*row).contains(p))
        .map(|(_, slot, _)| slot)
}
/// One row per creature: portrait, name, health, blow and intent. In camp the
/// list scouts the floor ahead.
fn foes(game: &Game, focused: Option<usize>, next: Option<&Game>, scouting: bool) {
    let mut any = false;
    for (row, slot, enemy) in foe_rows(game) {
        any = true;
        let r = foe_row(row);
        let cy = r.y + r.h * 0.5;
        let lit = focused == Some(slot) && !scouting;
        shapes::fill_rounded_rect(r, 9., rgba(1., 1., 1., if lit { 0.08 } else { 0.03 }));
        if lit {
            shapes::stroke_rounded_rect(r, 9., 1.2, with_alpha(RED, 0.6));
        }
        creature(enemy.species, SX + 22., cy + 3.);
        let name = if scouting {
            with_alpha(PAPER, 0.75)
        } else {
            PAPER
        };
        label(enemy.species.name(), SX + 50., cy - 3., 14., name);
        let after = next
            .and_then(|next| hit_at(next, enemy.pos))
            .map_or(enemy.health, |(amount, _)| {
                enemy.health.saturating_sub(amount)
            });
        foe_meter(SX + 50., cy + 6., 112., 5., enemy, after);
        icon_value(
            Icon::Sword,
            Num::new(enemy.species.damage()).as_str(),
            SX + 182.,
            cy,
            14.,
            with_alpha(HEART, 0.9),
        );
        if !scouting {
            intent(enemy.plan, SR - 16., cy, 1.3);
        }
    }
    if !any && !scouting {
        icon(
            Icon::Check,
            SX + SW * 0.5,
            FOE_TOP + 44.,
            30.,
            with_alpha(MOSS, 0.5),
        );
    }
}
fn sidebar(game: &Game, hover: Hover, focused: Option<usize>, preview: Option<&Preview>) {
    card(616., 120., 320., 434., CARD);
    let next = preview.filter(|p| p.accepted).map(|p| &p.next);
    let camp = game.phase == Phase::Camp;
    // A camp preview loads the next floor; compare supplies, not positions.
    vitals(game, next);
    let threat = if game.phase == Phase::Explore {
        game.danger(game.hero)
    } else {
        0
    };
    health_bar(game, next, threat);
    actions(game, hover, next);
    stats(game);
    match next.filter(|_| camp) {
        Some(ahead) => {
            // Scout the floor ahead: its name on the divider, then its foes.
            let name = FLOORS[ahead.depth].name;
            let w = text_width(name, TextStyle::modern(13., MUTED));
            divider(SX, 344., (SW - w) * 0.5 - 10., rgba(1., 1., 1., 0.1));
            label_centered(name, SX + SW * 0.5, 348., 13., MUTED);
            divider(
                SX + (SW + w) * 0.5 + 10.,
                344.,
                (SW - w) * 0.5 - 10.,
                rgba(1., 1., 1., 0.1),
            );
            foes(ahead, None, None, true);
        }
        None => {
            divider(SX, 344., SW, rgba(1., 1., 1., 0.1));
            if !camp {
                foes(game, focused, next, false);
            }
        }
    }
}

/// A campfire: crossed logs under a flickering flame.
fn campfire(x: f32, y: f32) {
    let t = clock();
    let flick = 1. + 0.07 * (t * 11.).sin() + 0.04 * (t * 17.3).sin();
    glow(x, y - 4., 30. * flick, rgba(1., 0.6, 0.25, 0.22));
    let log = Color::new(0.45, 0.30, 0.18, 1.);
    shapes::fill_capsule(vec2(x - 12., y + 10.), vec2(x + 12., y + 4.), 3., log);
    shapes::fill_capsule(
        vec2(x - 12., y + 4.),
        vec2(x + 12., y + 10.),
        3.,
        darken(log, 0.2),
    );
    icons::draw_styled(
        Icon::Flame,
        vec2(x, y - 6. - (flick - 1.) * 10.),
        28. * flick,
        IconStyle::new(GOLD).shadow(rgba(0., 0., 0., 0.4)),
    );
}
fn relic_card(game: &Game, index: usize, hovered: bool, selected: bool) {
    let t = clock();
    let (kind, name, gain, color) = RELICS[index];
    // Must match the model's click areas: x = 44 + i * 181, 171 wide, y 278..428.
    let x = 44. + index as f32 * 181.;
    let lift = if hovered { 4. } else { 0. };
    let r = Rect::new(x, 278. - lift, 171., 150.);
    let breath = pulse(t, 0.9);
    if selected {
        shapes::drop_shadow(
            r,
            12.,
            Shadow::soft(with_alpha(GOLD, 0.16 + 0.14 * breath)).blur(14.),
        );
    }
    if hovered {
        shapes::drop_shadow(
            r,
            12.,
            Shadow::soft(rgba(0., 0., 0., 0.5))
                .offset(vec2(0., 6.))
                .blur(14.),
        );
    }
    let (top, bottom) = if selected {
        (rgba(0.20, 0.27, 0.17, 1.), rgba(0.14, 0.19, 0.12, 1.))
    } else if hovered {
        (rgba(0.16, 0.22, 0.18, 1.), rgba(0.11, 0.15, 0.13, 1.))
    } else {
        (rgba(0.12, 0.17, 0.145, 1.), rgba(0.09, 0.13, 0.11, 1.))
    };
    shapes::fill_rounded_rect(r, 12., Fill::vertical(r, top, bottom));
    let (rim, width) = if selected {
        (GOLD, 2.)
    } else if hovered {
        (with_alpha(color, 0.8), 1.5)
    } else {
        (rgba(1., 1., 1., 0.08), 1.)
    };
    shapes::stroke_rounded_rect(r, 12., width, rim);
    keycap(
        Num::new(index as u32 + 1).as_str(),
        "",
        x + 10.,
        r.y + 10.,
        PAPER,
    );
    // One dot per copy already carried.
    for copy in 0..game.choices[index] {
        shapes::fill_circle(
            vec2(x + 158. - f32::from(copy) * 11., r.y + 20.),
            3.5,
            with_alpha(color, 0.9),
        );
    }
    let (cx, cy) = (x + 85.5, r.y + 52.);
    let grow = if selected || hovered {
        1. + 0.05 * breath
    } else {
        1.
    };
    glow(
        cx,
        cy,
        34.,
        with_alpha(color, if selected { 0.22 } else { 0.1 }),
    );
    icons::draw_styled(
        kind,
        vec2(cx, cy),
        42. * grow,
        IconStyle::new(color).shadow(rgba(0., 0., 0., 0.45)),
    );
    label_centered(
        name,
        cx,
        r.y + 103.,
        17.,
        if selected { GOLD } else { PAPER },
    );
    pill(kind, gain, cx, r.y + 126., 14., color);
}
fn camp(game: &Game, hover: Hover) {
    draw_rectangle(
        LEFT,
        TOP,
        TILE * COLS as f32,
        TILE * ROWS as f32,
        rgba(0.01, 0.02, 0.02, 0.62),
    );
    card(32., 194., 558., 284., Color::new(0.07, 0.11, 0.095, 0.99));
    campfire(66., 236.);
    label("Choose a relic", 96., 246., 22., PAPER);
    // Every camp also restores these.
    let mut x = 572.;
    for (kind, gain, color) in [
        (Icon::Flame, "+1", GOLD),
        (Icon::Potion, "+1", ROSE),
        (Icon::Heart, "+7", HEART),
    ] {
        let w = pill_width(gain, 14.);
        x -= w;
        pill(kind, gain, x + w * 0.5, 238., 14., color);
        x -= 6.;
    }
    for index in 0..RELICS.len() {
        relic_card(
            game,
            index,
            hover == Hover::Relic(index),
            game.choice == index,
        );
    }
    // Space confirms the selected relic.
    keycap_centered("Space", 44. + game.choice as f32 * 181. + 85.5, 440.);
}

/// Quest steps in the header: done, current, or still ahead.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Done,
    Active,
    Pending,
}
fn quest_step(kind: Icon, cx: f32, cy: f32, step: Step) {
    let t = clock();
    match step {
        Step::Done => {
            shapes::fill_circle(vec2(cx, cy), 12., MOSS);
            icon(kind, cx, cy, 14., INK);
        }
        Step::Active => {
            let p = pulse(t, 1.1);
            glow(cx, cy, 22. + 4. * p, with_alpha(GOLD, 0.2 + 0.14 * p));
            shapes::fill_circle(vec2(cx, cy), 12., rgba(0.12, 0.1, 0.05, 0.95));
            shapes::stroke_circle(vec2(cx, cy), 12., 2., GOLD);
            icon(kind, cx, cy + bob(t, 1.1, 1.), 15., GOLD);
        }
        Step::Pending => {
            shapes::stroke_circle(vec2(cx, cy), 12., 1.5, with_alpha(MUTED, 0.35));
            icon(kind, cx, cy, 14., with_alpha(MUTED, 0.45));
        }
    }
}
fn rank(score: u32) -> (&'static str, usize) {
    if score >= 7800 {
        ("Sunkeeper", 3)
    } else if score >= 6500 {
        ("Dawnbringer", 2)
    } else {
        ("Lantern Bearer", 1)
    }
}
impl Game {
    /// At most a few words; the header draws the same goal as icons.
    fn objective(&self) -> &'static str {
        match self.phase {
            Phase::Camp => "Choose a relic",
            Phase::Won => "Dawn restored",
            Phase::Lost => "Lantern out",
            Phase::Explore if !self.seed => "Find the key",
            Phase::Explore if self.king_alive() => "Defeat the Root King",
            Phase::Explore => "Take the stairs",
        }
    }
}

impl ArcadeGame for Game {
    fn new() -> Self {
        Game::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        let p = pointer();
        let hover = hover_at(self, p);
        let preview = preview_for(self, hover, p);
        let focused = focus(self, hover);
        board(self, hover, focused, preview.as_ref());
        sidebar(self, hover, focused, preview.as_ref());
        if self.phase == Phase::Camp {
            camp(self, hover);
        }
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: FLOORS[self.depth].name.into(),
            objective: self.objective().into(),
            status: format!(
                "Health {}/{}, blade {}, flasks {}, lantern {}/{}, amber {}, turn {}",
                self.health,
                self.max_health,
                self.attack,
                self.flasks,
                self.charge,
                self.max_charge,
                self.treasure,
                self.turns
            ),
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        Game::outcome(self)
    }
    fn summary(&self) -> String {
        if self.phase == Phase::Won {
            format!(
                "{}: six floors freed in {} turns with {} foes defeated and {} amber recovered.",
                rank(self.score).0,
                self.turns,
                self.kills,
                self.treasure
            )
        } else {
            format!(
                "The ember faded in {} after {} turns.",
                FLOORS[self.depth].name, self.turns
            )
        }
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.autoplay(tick)
    }
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.floor() as u32, FLOORS.len() as u32))
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        let hero = at(self.hero);
        let lift = vec2(0., -22.);
        let mut telegraphed = false;
        for event in self.events() {
            match event {
                Event::Step(pos) => {
                    fx.burst(at(pos) + vec2(0., 13.), rgba(0.42, 0.56, 0.36, 0.7), 5)
                }
                Event::Bump(pos) => {
                    fx.ring(at(pos), with_alpha(PAPER, 0.45), 14.);
                    fx.shake(0.06);
                }
                Event::Locked(pos) => {
                    fx.ring(at(pos), RED, 24.);
                    fx.popup_icon(at(pos) + lift, Icon::Lock, "", RED);
                    fx.shake(0.1);
                }
                Event::Crowned(pos) => {
                    fx.ring(at(pos), RED, 24.);
                    fx.popup_icon(at(pos) + lift, Icon::Crown, "", RED);
                    if let Some(king) = self
                        .enemies
                        .iter()
                        .flatten()
                        .find(|e| e.species == Species::King)
                    {
                        fx.ring(at(king.pos), RED, 34.);
                    }
                    fx.shake(0.1);
                }
                Event::Hit {
                    at: pos,
                    amount,
                    killed,
                    species,
                } => {
                    let c = at(pos);
                    // Numbers lean away from the hero so neither covers the other.
                    let d = c - hero;
                    let away = if d.length() > 0.5 {
                        d / d.length() * 12.
                    } else {
                        Vec2::ZERO
                    };
                    fx.burst(c, lighten(GOLD, 0.3), 8);
                    fx.popup(
                        c + away + vec2(0., -18.),
                        Num::signed(b'-', amount).as_str(),
                        lighten(GOLD, 0.2),
                    );
                    if killed {
                        let color = species_color(species);
                        fx.burst(c, color, 26);
                        fx.ring(c, color, 34.);
                        if species == Species::King {
                            fx.burst(c, GOLD, 72);
                            fx.ring(c, GOLD, 96.);
                            fx.shake(0.5);
                            fx.flash(rgba(1., 0.85, 0.5, 0.18));
                            fx.callout(BOARD_CENTER, "King slain", GOLD);
                        }
                    } else {
                        fx.shake(0.05);
                    }
                }
                Event::Hurt { amount, guarded } => {
                    if amount > 0 {
                        fx.popup_icon(
                            hero + lift,
                            Icon::Heart,
                            Num::signed(b'-', amount).as_str(),
                            HEART,
                        );
                        fx.burst(hero, HEART, 14);
                    } else {
                        fx.popup_icon(hero + lift, Icon::Shield, "0", BLUE);
                    }
                    if guarded {
                        fx.ring(hero, BLUE, 30.);
                    }
                }
                Event::Struck(pos) => fx.ring(at(pos), with_alpha(RED, 0.85), 22.),
                Event::Telegraph(pos) => {
                    telegraphed = true;
                    fx.ring(at(pos), with_alpha(RED, 0.5), 16.);
                }
                Event::Thorns(pos) => {
                    fx.popup_icon(at(pos) + lift, Icon::Heart, "-2", HEART);
                    fx.burst(at(pos) + vec2(0., 8.), rgba(0.56, 0.40, 0.30, 1.), 8);
                }
                Event::Key(pos) => {
                    fx.sparkle(at(pos), GOLD);
                    fx.ring(at(pos), GOLD, 46.);
                    fx.popup_icon(at(pos) + lift, Icon::Key, "", GOLD);
                    // Every lock on the floor answers the key.
                    for (index, tile) in self.board.iter().enumerate() {
                        if matches!(tile, Tile::Seal | Tile::Stairs) {
                            let p = Pos::new((index % COLS) as i32, (index / COLS) as i32);
                            fx.ring(at(p), with_alpha(GOLD, 0.8), 26.);
                        }
                    }
                }
                Event::Amber(pos) => {
                    fx.sparkle(at(pos), GOLD);
                    fx.burst(at(pos), rgba(0.95, 0.62, 0.2, 1.), 10);
                    fx.popup_icon(at(pos) + lift, Icon::Gem, "+140", GOLD);
                }
                Event::Flask(pos) => {
                    fx.sparkle(at(pos), ROSE);
                    fx.popup_icon(at(pos) + lift, Icon::Potion, "+1", ROSE);
                }
                Event::BeltFull(pos) => fx.popup_icon(at(pos) + lift, Icon::Potion, "Full", MUTED),
                Event::Well {
                    at: pos,
                    health,
                    flask,
                    charge,
                } => {
                    let c = at(pos);
                    fx.ring(c, BLUE, 40.);
                    fx.sparkle(c, BLUE);
                    if health > 0 {
                        fx.popup_icon(
                            c + lift,
                            Icon::Heart,
                            Num::signed(b'+', health).as_str(),
                            MOSS,
                        );
                    }
                    if flask > 0 {
                        fx.popup_icon(c + lift, Icon::Potion, "+1", ROSE);
                    }
                    if charge > 0 {
                        fx.popup_icon(c + lift, Icon::Flame, "+1", GOLD);
                    }
                }
                Event::Drink { health } => {
                    fx.popup_icon(
                        hero + lift,
                        Icon::Heart,
                        Num::signed(b'+', health).as_str(),
                        MOSS,
                    );
                    fx.sparkle(hero, MOSS);
                    fx.ring(hero, MOSS, 28.);
                    fx.ring(ACTIONS[1].center(), ROSE, 36.);
                }
                Event::NoDrink { empty } => {
                    fx.ring(ACTIONS[1].center(), RED, 38.);
                    if empty {
                        fx.popup_icon(hero + lift, Icon::Potion, "0", MUTED);
                    } else {
                        fx.popup_icon(hero + lift, Icon::Heart, "Full", MOSS);
                    }
                    fx.shake(0.05);
                }
                Event::Burst(pos) => {
                    let c = at(pos);
                    fx.ring(c, GOLD, TILE * 2.6);
                    fx.ring(c, lighten(GOLD, 0.4), TILE * 1.5);
                    fx.burst(c, GOLD, 40);
                    fx.flash(rgba(1., 0.8, 0.45, 0.12));
                    fx.shake(0.18);
                    fx.ring(ACTIONS[2].center(), GOLD, 36.);
                }
                Event::NoBurst { empty } => {
                    fx.ring(ACTIONS[2].center(), RED, 38.);
                    if empty {
                        fx.popup_icon(hero + lift, Icon::Flame, "0", MUTED);
                    } else {
                        fx.ring(hero, with_alpha(MUTED, 0.7), TILE * 2.5);
                        fx.popup_icon(hero + lift, Icon::Flame, "Too far", MUTED);
                    }
                    fx.shake(0.05);
                }
                Event::Guard(pos) => fx.ring(at(pos), BLUE, 26.),
                Event::Cleared(pos) => {
                    fx.ring(at(pos), GOLD, 60.);
                    fx.ring(at(pos), lighten(GOLD, 0.4), 32.);
                    fx.sparkle(at(pos), GOLD);
                    fx.burst(at(pos), GOLD, 30);
                }
                Event::Camp {
                    relic,
                    health,
                    flask,
                    charge,
                } => {
                    // Gains rise from the sidebar stats they changed.
                    if health > 0 {
                        fx.popup_icon(
                            vec2(SX + 40., 146.),
                            Icon::Heart,
                            Num::signed(b'+', health).as_str(),
                            MOSS,
                        );
                    }
                    if relic == 0 {
                        fx.popup_icon(vec2(SR - 30., 146.), Icon::Sword, "+1", STEEL);
                    }
                    if flask > 0 {
                        fx.popup_icon(ACTIONS[1].center(), Icon::Potion, "+1", ROSE);
                    }
                    if charge > 0 {
                        fx.popup_icon(
                            ACTIONS[2].center(),
                            Icon::Flame,
                            Num::signed(b'+', charge).as_str(),
                            GOLD,
                        );
                    }
                    fx.ring(hero, MOSS, 40.);
                    fx.sparkle(hero, MOSS);
                    fx.callout(BOARD_CENTER, FLOORS[self.depth].name, PAPER);
                }
                Event::Fallen(pos) => {
                    fx.burst(at(pos), HEART, 36);
                    fx.ring(at(pos), HEART, 56.);
                }
                Event::Won(pos) => {
                    fx.ring(at(pos), GOLD, 80.);
                    fx.confetti(at(pos), 60);
                    fx.sparkle(at(pos), GOLD);
                }
            }
        }
        // A fresh threat on the hero's own tile: the edges warn.
        if telegraphed && self.phase == Phase::Explore && self.danger(self.hero) > 0 {
            fx.vignette(rgba(0.85, 0.16, 0.12, 0.22));
        }
    }
    fn draw_objective(&self, area: Rect) -> bool {
        let cy = area.y + area.h * 0.5;
        let won_floor = matches!(self.phase, Phase::Camp | Phase::Won);
        let key = if self.seed || won_floor {
            Step::Done
        } else {
            Step::Active
        };
        let mut x = area.x + 14.;
        quest_step(Icon::Key, x, cy, key);
        if self.depth == FLOORS.len() - 1 {
            let crown = if !self.king_alive() {
                Step::Done
            } else if self.seed {
                Step::Active
            } else {
                Step::Pending
            };
            x += 32.;
            quest_step(Icon::Crown, x, cy, crown);
        }
        let stairs = if won_floor {
            Step::Done
        } else if self.seed && !self.king_alive() {
            Step::Active
        } else {
            Step::Pending
        };
        icon(Icon::ArrowRight, x + 22., cy, 11., with_alpha(MUTED, 0.6));
        x += 44.;
        quest_step(Icon::ArrowDown, x, cy, stairs);
        let s = style(17., Color::new(0.84, 0.88, 0.92, 1.));
        draw_text_styled(self.objective(), x + 24., s.centered_baseline(cy), s);
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        let won = self.phase == Phase::Won;
        let tiles = [
            (Icon::Hourglass, Num::new(self.turns), CELADON),
            (
                Icon::Skull,
                Num::new(self.kills),
                Color::new(0.86, 0.80, 0.72, 1.),
            ),
            (Icon::Gem, Num::new(self.treasure), GOLD),
            (Icon::Heart, Num::new(self.health), HEART),
        ];
        let gap = 12.;
        let w = (area.w - gap * 3.) / 4.;
        for (i, (kind, value, color)) in tiles.iter().enumerate() {
            let r = Rect::new(area.x + i as f32 * (w + gap), area.y + 4., w, 64.);
            shapes::fill_rounded_rect(r, 12., rgba(1., 1., 1., 0.045));
            shapes::stroke_rounded_rect(r, 12., 1., with_alpha(*color, 0.3));
            icons::draw_styled(
                *kind,
                vec2(r.x + 30., r.y + 32.),
                30.,
                IconStyle::new(*color).shadow(rgba(0., 0., 0., 0.4)),
            );
            let s = style(30., PAPER);
            draw_text_styled(value.as_str(), r.x + 54., s.centered_baseline(r.y + 32.), s);
        }
        // The six floors: cleared ones lit, the fatal one marked.
        let cy = area.y + 114.;
        for floor in 0..FLOORS.len() {
            let c = vec2(area.x + 14. + floor as f32 * 26., cy);
            if won || floor < self.depth {
                shapes::fill_circle(c, 8., MOSS);
                icon(Icon::Check, c.x, c.y, 10., INK);
            } else if floor == self.depth {
                shapes::stroke_circle(c, 9., 2., HEART);
                icon(Icon::Skull, c.x, c.y, 12., HEART);
            } else {
                shapes::stroke_circle(c, 7., 1.5, with_alpha(MUTED, 0.4));
            }
        }
        if won {
            // The rank earned: one to three suns and its name.
            let (name, suns) = rank(self.score);
            let x0 = area.x + 196.;
            for i in 0..3 {
                let lit = i < suns;
                if lit {
                    glow(x0 + i as f32 * 32., cy, 20., rgba(1., 0.8, 0.4, 0.18));
                }
                icon(
                    Icon::Sun,
                    x0 + i as f32 * 32.,
                    cy,
                    26.,
                    if lit { GOLD } else { with_alpha(MUTED, 0.3) },
                );
            }
            label_mid(name, x0 + 92., cy, 26., GOLD);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_format_without_allocating() {
        assert_eq!(Num::new(0_u32).as_str(), "0");
        assert_eq!(Num::new(7385_u32).as_str(), "7385");
        assert_eq!(Num::signed(b'-', 12_u8).as_str(), "-12");
        assert_eq!(Num::signed(b'/', 26_u8).as_str(), "/26");
        assert_eq!(Num::new(u32::MAX).as_str(), "4294967295");
    }

    #[test]
    fn progress_follows_the_six_floors_to_the_end() {
        let mut game = Game::new();
        assert_eq!(ArcadeGame::progress(&game), Some((1, 6)));
        for tick in 0..54_000 {
            game.step(&game.autoplay(tick));
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Won);
        assert_eq!(ArcadeGame::progress(&game), Some((6, 6)));
        let hud = ArcadeGame::hud(&game);
        assert_eq!(hud.stage, "The Root King's Hall");
        assert!(hud.objective.split_whitespace().count() <= 5);
        let summary = ArcadeGame::summary(&game);
        assert_eq!(summary.matches(". ").count(), 0, "one sentence: {summary}");
    }

    #[test]
    fn accepted_events_become_effects_where_they_happen() {
        // Taking the key sparkles, rings and pops its icon on the tile.
        let mut game = Game::new();
        game.enemies.fill(None);
        let key = game.hero.offset(1, 0);
        game.board[key.index()] = Tile::Seed;
        let cue = game.step(&Input {
            right_pressed: true,
            ..Input::default()
        });
        assert!(game.events().any(|e| e == Event::Key(key)));
        let mut fx = Juice::new();
        ArcadeGame::effects(&game, cue, &mut fx);
        let counts = fx.counts();
        assert!(
            counts.popups >= 1 && counts.rings >= 2 && counts.sparks > 0,
            "{counts:?}"
        );

        // A landed blow pops its damage over the hero.
        let mut game = Game::new();
        game.enemies.fill(None);
        game.enemies[0] = Some(Enemy {
            pos: Pos::new(2, 1),
            species: Species::Knight,
            health: 10,
            max_health: 10,
            plan: Plan::Strike(game.hero),
            rounds: 0,
        });
        let cue = game.step(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(cue, Cue::Bad);
        assert!(game.events().any(|e| e
            == Event::Hurt {
                amount: 2,
                guarded: true
            }));
        let mut fx = Juice::new();
        ArcadeGame::effects(&game, cue, &mut fx);
        assert!(fx.counts().popups >= 1 && fx.counts().rings >= 2);

        // Idle ticks request nothing.
        let mut idle = Game::new();
        let cue = idle.step(&Input::default());
        let mut fx = Juice::new();
        ArcadeGame::effects(&idle, cue, &mut fx);
        assert!(fx.is_idle());
    }

    #[test]
    fn previews_simulate_without_touching_the_game() {
        let game = Game::new();
        let before = format!("{game:?}");
        let step = simulate(
            &game,
            &Input {
                right_pressed: true,
                ..Input::default()
            },
        );
        assert!(step.accepted);
        assert_eq!(step.next.hero, Pos::new(2, 1));
        let wall = simulate(
            &game,
            &Input {
                up_pressed: true,
                ..Input::default()
            },
        );
        assert!(!wall.accepted);
        assert_eq!(format!("{game:?}"), before);
    }
}
