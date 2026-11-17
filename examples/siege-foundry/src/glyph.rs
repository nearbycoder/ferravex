//! The range's wordless HUD vocabulary: iron shells, rust targets, sandstone
//! supports, mini towers (structure integrity), key caps and centered rows of
//! icons and numbers. Engine [`icons`] cover generic ideas (bomb, hammer,
//! hourglass, bell, trophy …); the range keeps its own block and shell art so
//! the HUD shows the same things the player sees in the scene.
//!
//! Everything is immediate screen-space drawing and allocation free.
use super::hud::{body, display, fade, width};
use super::{PAPER, RUST, SAND};
use ferravex_render::icons::{self, Icon, IconStyle};
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Fill, Stroke};

/// A pictogram that can sit in a row next to numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Glyph {
    /// A shared engine pictogram tinted with the item color.
    Icon(Icon),
    /// An iron cannonball.
    Shell,
    /// A rust-red target block (with its white rivet).
    Target,
    /// A sandstone support block.
    Support,
    /// A gold star, filled or as an outline.
    Star(bool),
}

/// One element of a centered row.
#[derive(Clone, Copy)]
pub(super) enum Item<'a> {
    Glyph(Glyph, Color),
    /// Modern body text.
    Text(&'a str, Color),
    /// Classic arcade display text.
    Display(&'a str, Color),
    /// A key cap such as `Space` or `Esc`.
    Key(&'a str),
    /// Extra space in units of the row size.
    Gap(f32),
}

fn key_width(label: &str, size: f32) -> f32 {
    (width(label, body(size * 0.78, PAPER)) + size * 0.7).max(size * 1.25)
}

fn item_width(item: Item, size: f32) -> f32 {
    match item {
        Item::Glyph(..) => size,
        Item::Text(text, color) => width(text, body(size, color)),
        Item::Display(text, color) => width(text, display(size, color)),
        Item::Key(label) => key_width(label, size),
        Item::Gap(units) => units * size,
    }
}

/// Tight between an icon and its own value, roomier between groups.
fn spacing(a: Item, b: Item, size: f32) -> f32 {
    match (a, b) {
        (Item::Gap(_), _) | (_, Item::Gap(_)) => 0.0,
        (Item::Glyph(..), Item::Text(..) | Item::Display(..)) => size * 0.2,
        (Item::Glyph(..), Item::Glyph(..)) => size * 0.12,
        _ => size * 0.32,
    }
}

/// Total width of a row at `size`.
pub(super) fn row_width(items: &[Item], size: f32) -> f32 {
    let mut total = 0.0;
    for (i, item) in items.iter().enumerate() {
        total += item_width(*item, size);
        if let Some(next) = items.get(i + 1) {
            total += spacing(*item, *next, size);
        }
    }
    total
}

/// Draw a row from `left`, vertically centered on `cy`. Returns its width.
pub(super) fn row_at(items: &[Item], left: f32, cy: f32, size: f32, alpha: f32) -> f32 {
    let mut x = left;
    for (i, item) in items.iter().enumerate() {
        let w = item_width(*item, size);
        match *item {
            Item::Glyph(glyph, color) => draw(glyph, vec2(x + w * 0.5, cy), size, color, alpha),
            Item::Text(text, color) => {
                let style = body(size, fade(color, alpha));
                ferravex_render::text::draw_text_styled(
                    text,
                    x,
                    style.centered_baseline(cy),
                    style,
                );
            }
            Item::Display(text, color) => {
                let style = display(size, fade(color, alpha));
                ferravex_render::text::draw_text_styled(
                    text,
                    x,
                    style.centered_baseline(cy),
                    style,
                );
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

/// Draw a row centered on `center`; shrinks to fit `max` when given.
pub(super) fn row(items: &[Item], center: Vec2, size: f32, max: f32, alpha: f32) -> f32 {
    let natural = row_width(items, size);
    let size = if natural > max && natural > 0.0 {
        (size * max / natural).max(8.0)
    } else {
        size
    };
    let w = row_width(items, size);
    row_at(items, center.x - w * 0.5, center.y, size, alpha);
    w
}

/// A warm key cap with a raised lip, its left edge at `x`, centered on `cy`.
pub(super) fn keycap(label: &str, x: f32, cy: f32, size: f32, alpha: f32) {
    let w = key_width(label, size);
    let h = size * 1.3;
    let rect = Rect::new(x, cy - h * 0.5 - 1.5, w, h);
    shapes::fill_rounded_rect(
        Rect::new(rect.x, rect.y + 3.0, rect.w, rect.h),
        5.0,
        fade(Color::from_rgba(120, 84, 60, 255), alpha),
    );
    shapes::fill_rounded_rect(
        rect,
        5.0,
        Fill::vertical(
            rect,
            fade(Color::from_rgba(252, 236, 206, 255), alpha),
            fade(Color::from_rgba(226, 200, 164, 255), alpha),
        ),
    );
    let style = ferravex_render::text::TextStyle::modern(
        size * 0.78,
        fade(Color::from_rgba(40, 28, 24, 255), alpha),
    );
    let tw = width(label, style);
    ferravex_render::text::draw_text_styled(
        label,
        rect.x + (rect.w - tw) * 0.5,
        style.centered_baseline(rect.y + rect.h * 0.5),
        style,
    );
}

/// An iron cannonball with a top-left highlight.
pub(super) fn shell(at: Vec2, r: f32, alpha: f32) {
    shapes::fill_circle(
        at + vec2(0.5, 1.5),
        r,
        fade(Color::new(0.0, 0.0, 0.0, 0.5), alpha),
    );
    shapes::fill_circle(
        at,
        r,
        Fill::radial(
            at - vec2(r * 0.3, r * 0.35),
            r * 1.3,
            fade(Color::from_rgba(196, 180, 164, 255), alpha),
            fade(Color::from_rgba(70, 60, 56, 255), alpha),
        ),
    );
}

/// A block face: rust targets carry a white rivet, sandstone is plain.
pub(super) fn block(rect: Rect, target: bool, alpha: f32) {
    let base = if target { RUST } else { SAND };
    let lit = shapes::mix(base, Color::new(1.0, 1.0, 1.0, 1.0), 0.18);
    shapes::fill_rounded_rect(
        rect,
        (rect.w * 0.18).min(3.0),
        Fill::vertical(rect, fade(lit, alpha), fade(base, alpha)),
    );
    if target {
        shapes::fill_circle(rect.center(), (rect.w * 0.16).max(1.2), fade(PAPER, alpha));
    }
}

/// One tower as a stack of four cells, bottom to top: two supports, two
/// targets. Fallen cells become faint outlines; fallen targets get a tick.
/// `wobble` (0..=1) leans the stack.
pub(super) fn mini_tower(base: Vec2, cell: f32, down: [bool; 4], wobble: f32, alpha: f32) {
    let gap = (cell * 0.18).max(1.0);
    for (layer, fallen) in down.iter().enumerate() {
        let lean = wobble * (layer as f32) * cell * 0.12;
        let y = base.y - (layer as f32 + 1.0) * cell - layer as f32 * gap;
        let rect = Rect::new(base.x - cell * 0.5 + lean, y, cell, cell);
        let target = layer >= 2;
        if *fallen {
            let green = Color::from_rgba(164, 206, 126, 255);
            if target {
                // A knocked target turns into a green tick, like a done task.
                shapes::fill_rounded_rect(rect, 2.0, fade(green, 0.28 * alpha));
                let c = rect.center();
                let s = cell / 14.0;
                shapes::polyline(
                    &[
                        c + vec2(-4.5 * s, 0.0),
                        c + vec2(-1.2 * s, 3.5 * s),
                        c + vec2(5.0 * s, -4.0 * s),
                    ],
                    Stroke::new((2.2 * s).max(1.4), fade(green, alpha)).cap(shapes::Cap::Round),
                );
            } else {
                shapes::stroke_rounded_rect(
                    rect,
                    2.0,
                    1.0,
                    fade(Color::new(1.0, 1.0, 1.0, 0.25), alpha),
                );
            }
        } else {
            block(rect, target, alpha);
        }
    }
}

/// Draw one glyph centered at `c` in a `size` box.
pub(super) fn draw(glyph: Glyph, c: Vec2, size: f32, color: Color, alpha: f32) {
    match glyph {
        Glyph::Icon(icon) => icons::draw_styled(
            icon,
            c,
            size,
            IconStyle::new(fade(color, alpha)).shadow(fade(Color::new(0.0, 0.0, 0.0, 0.45), alpha)),
        ),
        Glyph::Shell => shell(c, size * 0.36, alpha),
        Glyph::Target | Glyph::Support => {
            let s = size * 0.72;
            block(
                Rect::new(c.x - s * 0.5, c.y - s * 0.5, s, s),
                glyph == Glyph::Target,
                alpha,
            )
        }
        Glyph::Star(filled) => {
            let r = size * 0.46;
            let points: [Vec2; 10] = std::array::from_fn(|i| {
                let radius = if i % 2 == 0 { r } else { r * 0.45 };
                let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
                c + vec2(a.cos(), a.sin()) * radius
            });
            if filled {
                for i in 0..10 {
                    shapes::fill_triangle(c, points[i], points[(i + 1) % 10], fade(color, alpha));
                }
            } else {
                shapes::polyline(
                    &points,
                    Stroke::new((r * 0.14).max(1.2), fade(color, 0.6 * alpha))
                        .join(shapes::Join::Round)
                        .closed(true),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_measure_consistently_and_shrink_to_fit() {
        let items = [
            Item::Glyph(Glyph::Shell, PAPER),
            Item::Text("3", PAPER),
            Item::Gap(0.5),
            Item::Key("Space"),
        ];
        let w = row_width(&items, 18.0);
        assert!(w > 0.0 && w.is_finite());
        assert!(row_width(&items[..2], 18.0) < w);
        assert!(spacing(items[0], items[1], 18.0) < spacing(items[1], items[3], 18.0));
        assert_eq!(row_width(&[], 18.0), 0.0);
        assert!(key_width("Esc", 16.0) >= 16.0 * 1.25);
    }
}
