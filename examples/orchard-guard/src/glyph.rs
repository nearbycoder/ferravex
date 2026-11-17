//! The orchard's wordless HUD vocabulary: icon + number rows, key caps, pips
//! and the game's own pictograms (beetles, towers, coins, hearts). Engine
//! [`icons`] cover generic ideas (sword, bolt, target, flag, pause, …); the
//! orchard keeps its established coin/heart/beetle/tower art so the HUD reads
//! as the same things the player sees on the board.
//!
//! Everything is immediate drawing in the current camera space, allocation
//! free, and reads only the values it is given.
use crate::game::{Kind, Tower};
use crate::juice;
use crate::style::{self, INK};
use ferravex_render::icons::{self, Icon, IconStyle};
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Fill};
use ferravex_render::text::text_width;

/// A pictogram that can sit in a row next to numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Glyph {
    /// A shared engine pictogram, tinted with the item color.
    Icon(Icon),
    Coin,
    Heart,
    /// A beetle still on its way.
    Beetle,
    /// A speedy zoomie.
    Zoomie,
    /// A stopped pest: on its back with x-eyes.
    Squashed,
    /// A tower portrait.
    Tower(Kind),
    /// Two chevrons: fast-forward.
    FastForward,
    /// A pest health bar.
    Health,
    /// Damage dealt twice: a sword with a small `2×` mark.
    Lucky,
}

/// One element of a centered row.
#[derive(Clone, Copy)]
pub(super) enum Item<'a> {
    Glyph(Glyph, Color),
    Text(&'a str, Color),
    /// A key cap, such as `U` or `Space`.
    Key(&'a str),
    /// Extra space in units of the row size.
    Gap(f32),
}

fn fade(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a.clamp(0., 1.))
}

fn glyph_width(glyph: Glyph, size: f32) -> f32 {
    match glyph {
        Glyph::Tower(_) => size * 1.05,
        Glyph::Health => size * 1.2,
        Glyph::FastForward => size * 1.1,
        _ => size,
    }
}

fn key_width(label: &str, size: f32) -> f32 {
    (text_width(label, style::body(size * 0.8, INK)) + size * 0.62).max(size * 1.15)
}

fn width(item: Item, size: f32) -> f32 {
    match item {
        Item::Glyph(glyph, _) => glyph_width(glyph, size),
        Item::Text(s, _) => text_width(s, style::body(size, INK)),
        Item::Key(label) => key_width(label, size),
        Item::Gap(units) => units * size,
    }
}

/// Spacing between two adjacent items: tight between an icon and its value.
fn spacing(a: Item, b: Item, size: f32) -> f32 {
    match (a, b) {
        (Item::Gap(_), _) | (_, Item::Gap(_)) => 0.,
        (Item::Glyph(..), Item::Text(..)) => size * 0.16,
        _ => size * 0.3,
    }
}

/// Total width of a row at `size` (the text size; glyphs use the same box).
pub(super) fn row_width(items: &[Item], size: f32) -> f32 {
    let mut total = 0.;
    for (i, item) in items.iter().enumerate() {
        total += width(*item, size);
        if let Some(next) = items.get(i + 1) {
            total += spacing(*item, *next, size);
        }
    }
    total
}

/// Draw a row starting at `left`, vertically centered on `cy`. Returns its width.
pub(super) fn row_at(items: &[Item], left: f32, cy: f32, size: f32, alpha: f32) -> f32 {
    let mut x = left;
    for (i, item) in items.iter().enumerate() {
        let w = width(*item, size);
        match *item {
            Item::Glyph(glyph, color) => draw(glyph, vec2(x + w * 0.5, cy), size, color, alpha),
            Item::Text(s, color) => {
                let style = style::body(size, fade(color, alpha));
                style::text(s, x, style.centered_baseline(cy), style);
            }
            Item::Key(label) => keycap(label, x, cy, size, alpha),
            Item::Gap(_) => {}
        }
        x += w;
        if let Some(next) = items.get(i + 1) {
            x += spacing(*item, *next, size);
        }
    }
    x - left
}

/// Draw a row centered on `center`. Returns its bounds.
pub(super) fn row(items: &[Item], center: Vec2, size: f32, alpha: f32) -> Rect {
    let w = row_width(items, size);
    let left = center.x - w * 0.5;
    row_at(items, left, center.y, size, alpha);
    Rect::new(left, center.y - size * 0.6, w, size * 1.2)
}

/// A cream key cap with a darker lip, its left edge at `x`, centered on `cy`.
pub(super) fn keycap(label: &str, x: f32, cy: f32, size: f32, alpha: f32) {
    let w = key_width(label, size);
    let h = size * 1.2;
    let rect = Rect::new(x, cy - h * 0.5 - 1., w, h);
    shapes::fill_rounded_rect(
        Rect::new(rect.x, rect.y + 2.5, rect.w, rect.h),
        4.,
        fade(Color::new(0.47, 0.42, 0.28, 0.85), alpha),
    );
    shapes::fill_rounded_rect(
        rect,
        4.,
        Fill::vertical(
            rect,
            fade(Color::new(1., 0.99, 0.95, 1.), alpha),
            fade(Color::new(0.93, 0.89, 0.76, 1.), alpha),
        ),
    );
    shapes::stroke_rounded_rect(rect, 4., 1., fade(Color::new(0.3, 0.32, 0.2, 0.35), alpha));
    let text = style::body(size * 0.8, fade(INK, alpha));
    style::text_center(
        label,
        rect.x + rect.w * 0.5,
        text.centered_baseline(rect.y + rect.h * 0.5),
        text,
    );
}

/// A gold coin of `radius` (the HUD and pops share this look).
pub(super) fn coin(c: Vec2, r: f32, alpha: f32) {
    shapes::fill_circle(
        c,
        r,
        Fill::radial(
            c - vec2(r * 0.3, r * 0.3),
            r * 1.4,
            fade(Color::from_hex(0xffe08a), alpha),
            fade(juice::GOLD, alpha),
        ),
    );
    let rim = fade(Color::from_hex(0xb7862c), alpha);
    shapes::stroke_circle(c, r * 0.68, (r * 0.11).max(1.), rim);
    shapes::fill_rounded_rect(
        Rect::new(c.x - r * 0.12, c.y - r * 0.4, r * 0.24, r * 0.8),
        r * 0.1,
        rim,
    );
}

/// A rounded heart of half-width `r`.
pub(super) fn heart(c: Vec2, r: f32, color: Color) {
    shapes::fill_circle(c + vec2(-r * 0.5, -r * 0.2), r * 0.55, color);
    shapes::fill_circle(c + vec2(r * 0.5, -r * 0.2), r * 0.55, color);
    shapes::fill_triangle(
        c + vec2(-r * 1.02, -r * 0.05),
        c + vec2(r * 1.02, -r * 0.05),
        c + vec2(0., r * 0.95),
        color,
    );
}

/// A top-down beetle facing right, sized to a `size` box.
fn beetle(c: Vec2, size: f32, body: Color, fast: bool, alpha: f32) {
    let r = size * if fast { 0.3 } else { 0.35 };
    let ink = fade(INK, alpha);
    let leg = (size * 0.06).max(1.);
    for n in [-1., 1.] {
        for t in [-0.45, 0., 0.45] {
            let root = c + vec2(t * r, 0.);
            let tip = c + vec2(t * r * 1.4, n * r * 1.45);
            shapes::fill_capsule(root, tip, leg * 0.5, ink);
        }
        // Antennae.
        let base = c + vec2(r * 1.05, n * r * 0.2);
        shapes::fill_capsule(base, base + vec2(r * 0.55, n * r * 0.55), leg * 0.45, ink);
    }
    if fast {
        for n in [-1., 0., 1.] {
            let a = c + vec2(-r * 1.55, n * r * 0.55);
            shapes::fill_capsule(
                a,
                a - vec2(r * 0.9, 0.),
                leg * 0.45,
                fade(Color::new(0.4, 0.45, 0.3, 0.8), alpha),
            );
        }
    }
    shapes::fill_circle(
        c,
        r,
        Fill::radial(
            c - vec2(r * 0.3, r * 0.35),
            r * 1.5,
            fade(
                shapes::mix(body, Color::new(1., 0.95, 0.85, 1.), 0.3),
                alpha,
            ),
            fade(body, alpha),
        ),
    );
    shapes::fill_capsule(
        c - vec2(r * 0.75, 0.),
        c + vec2(r * 0.55, 0.),
        leg * 0.35,
        ink,
    );
    shapes::fill_circle(c + vec2(r * 0.95, 0.), r * 0.48, ink);
    for n in [-1., 1.] {
        let eye = c + vec2(r * 1.05, n * r * 0.22);
        shapes::fill_circle(eye, r * 0.2, fade(Color::new(0.98, 0.95, 0.84, 1.), alpha));
        shapes::fill_circle(eye + vec2(r * 0.06, 0.), r * 0.1, ink);
    }
}

/// A stopped beetle on its back, legs up, with x-eyes.
fn squashed(c: Vec2, size: f32, alpha: f32) {
    let r = size * 0.34;
    let ink = fade(INK, alpha);
    let leg = (size * 0.06).max(1.);
    for n in [-1., 1.] {
        for k in [-0.5, 0., 0.5] {
            let root = c + vec2(k * r, -r * 0.45);
            shapes::fill_capsule(root, root + vec2(n * r * 0.3, -r * 0.75), leg * 0.5, ink);
        }
    }
    let body = juice::BEETLE;
    shapes::fill_circle(
        c + vec2(0., r * 0.12),
        r,
        fade(
            shapes::mix(body, Color::new(1., 0.95, 0.85, 1.), 0.1),
            alpha,
        ),
    );
    for n in [-1., 1.] {
        let e = c + vec2(n * r * 0.42, r * 0.05);
        let d = r * 0.22;
        shapes::fill_capsule(e - vec2(d, d), e + vec2(d, d), leg * 0.45, ink);
        shapes::fill_capsule(e + vec2(-d, d), e + vec2(d, -d), leg * 0.45, ink);
    }
}

fn fast_forward(c: Vec2, size: f32, color: Color) {
    let h = size * 0.36;
    for dx in [-0.27, 0.2] {
        let x = c.x + dx * size;
        shapes::fill_polygon(
            &[
                vec2(x - h * 0.55, c.y - h),
                vec2(x + h * 0.75, c.y),
                vec2(x - h * 0.55, c.y + h),
            ],
            color,
        );
    }
}

fn health(c: Vec2, size: f32, alpha: f32) {
    let track = Rect::new(
        c.x - size * 0.55,
        c.y - size * 0.16,
        size * 1.1,
        size * 0.32,
    );
    shapes::fill_rounded_rect(
        track,
        track.h * 0.5,
        fade(Color::new(0.2, 0.25, 0.18, 0.6), alpha),
    );
    let fill = Rect::new(
        track.x + 2.,
        track.y + 2.,
        (track.w - 4.) * 0.72,
        track.h - 4.,
    );
    shapes::fill_rounded_rect(
        fill,
        fill.h * 0.5,
        Fill::horizontal(fill, fade(juice::APPLE, alpha), fade(juice::LEAF, alpha)),
    );
}

/// Draw one glyph centered at `c` in a `size` box.
pub(super) fn draw(glyph: Glyph, c: Vec2, size: f32, color: Color, alpha: f32) {
    match glyph {
        Glyph::Icon(icon) => icons::draw_styled(icon, c, size, IconStyle::new(fade(color, alpha))),
        Glyph::Coin => coin(c, size * 0.42, alpha),
        Glyph::Heart => heart(c + vec2(0., size * 0.04), size * 0.46, fade(color, alpha)),
        Glyph::Beetle => beetle(c, size, juice::BEETLE, false, alpha),
        Glyph::Zoomie => beetle(c, size, juice::ZOOMIE, true, alpha),
        Glyph::Squashed => squashed(c, size, alpha),
        Glyph::Tower(kind) => {
            let scale = size / 42.;
            crate::scene::tower(
                c + vec2(0., size * 0.2),
                Tower::planted(kind),
                Vec2::splat(scale),
                false,
            );
        }
        Glyph::FastForward => fast_forward(c, size, fade(color, alpha)),
        Glyph::Health => health(c, size, alpha),
        Glyph::Lucky => {
            icons::draw_styled(
                Icon::Sword,
                c - vec2(size * 0.1, 0.),
                size * 0.86,
                IconStyle::new(fade(color, alpha)),
            );
            let badge = c + vec2(size * 0.32, size * 0.3);
            shapes::fill_circle(badge, size * 0.24, fade(juice::GOLD, alpha));
            let style = style::body(size * 0.3, fade(INK, alpha));
            style::text_center("2×", badge.x, style.centered_baseline(badge.y), style);
        }
    }
}

/// Center of pip `i` in a row of `total` pips of radius `r`.
pub(super) fn pip_center(center: Vec2, total: usize, i: usize, r: f32) -> Vec2 {
    let gap = r * 3.2;
    let start = center.x - gap * (total.saturating_sub(1)) as f32 * 0.5;
    vec2(start + gap * i as f32, center.y)
}

/// A row of `total` progress pips centered on `center`: lit ones are filled,
/// `active` (index, fraction) draws a filling ring, `next` pulses by `beat`.
#[allow(clippy::too_many_arguments)]
pub(super) fn pips(
    center: Vec2,
    total: usize,
    lit: usize,
    active: Option<(usize, f32)>,
    next: Option<usize>,
    beat: f32,
    r: f32,
    fill: Color,
) {
    let track = Color::new(0.23, 0.39, 0.24, 0.22);
    for i in 0..total {
        let c = pip_center(center, total, i, r);
        if i < lit {
            shapes::fill_circle(c, r, fill);
            shapes::fill_circle(
                c - vec2(r * 0.3, r * 0.3),
                r * 0.3,
                Color::new(1., 1., 1., 0.35),
            );
        } else if let Some((_, frac)) = active.filter(|(index, _)| *index == i) {
            shapes::stroke_circle(c, r, 2.5, track);
            shapes::stroke_arc(
                c,
                r,
                -std::f32::consts::FRAC_PI_2,
                std::f32::consts::TAU * frac.clamp(0., 1.),
                shapes::Stroke::new(3.2, juice::GOLD).cap(shapes::Cap::Round),
            );
            shapes::fill_circle(c, r * 0.42, fill);
        } else if next == Some(i) {
            let glow = 0.5 + 0.5 * beat;
            shapes::fill_circle(c, r + 3. * glow, Color::new(0.97, 0.76, 0.27, 0.25 * glow));
            shapes::stroke_circle(c, r, 2.5, juice::GOLD);
        } else {
            shapes::stroke_circle(c, r, 2., track);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_measure_consistently_and_tight_pairs_stay_tight() {
        let items = [
            Item::Glyph(Glyph::Coin, INK),
            Item::Text("65", INK),
            Item::Gap(0.5),
            Item::Key("U"),
        ];
        let w = row_width(&items, 16.);
        assert!(w > 0. && w.is_finite());
        // Removing an element always shrinks the row.
        assert!(row_width(&items[..2], 16.) < w);
        // An icon sits closer to its own number than to the next group.
        let pair = spacing(items[0], items[1], 16.);
        let groups = spacing(items[1], Item::Glyph(Glyph::Heart, INK), 16.);
        assert!(pair < groups);
        assert_eq!(row_width(&[], 16.), 0.);
    }

    #[test]
    fn key_caps_never_shrink_below_a_square_touch_of_text() {
        assert!(key_width("U", 14.) >= 14. * 1.15);
        assert!(key_width("Space", 14.) > key_width("U", 14.));
    }
}
