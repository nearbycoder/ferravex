//! Small screen-space marks that replace HUD words: the golf-ball glyph, hole
//! pips, scorecard notation (circles under par, squares over), the round's
//! to-par badge, icon + number pairs and short tag pills. Allocation-free.
//!
//! The same idea always uses the same mark: a flag is par, the ball is
//! strokes, a trophy is a personal best, a drop is a water penalty, a gear is
//! moving copper, a flame is a streak, a target is aim and the restart arrow
//! is practice.
use super::*;
use ferravex_render::{
    icons::{self, Icon, IconStyle},
    shapes::{self, Fill, Stroke},
    text::{TextAlign, TextStyle as Type, draw_text_styled, text_width},
};

/// Space between a mark and its number.
const GAP: f32 = 4.;
/// Sand-yellow used for the bunker mark.
pub(super) const SAND: Color = Color::from_rgba(236, 202, 128, 255);
/// Pool blue used for water marks.
pub(super) const POOL: Color = Color::from_rgba(120, 196, 222, 255);

/// Small counts as static text, so per-frame HUD numbers never format.
pub(super) fn num(n: u32) -> &'static str {
    const N: [&str; 25] = [
        "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
        "17", "18", "19", "20", "21", "22", "23", "24",
    ];
    N.get(n as usize).copied().unwrap_or("24+")
}

/// The game's ball as a glyph: the same white, lit from the top-left.
pub(super) fn ball(c: Vec2, r: f32, a: f32) {
    shapes::fill_circle(
        c + vec2(r * 0.2, r * 0.35),
        r * 1.05,
        Color::new(0., 0., 0., 0.28 * a),
    );
    shapes::fill_circle(
        c,
        r,
        Fill::radial(
            c - vec2(r * 0.35, r * 0.4),
            r * 1.6,
            Color::new(1., 1., 1., a),
            Color::new(0.8, 0.77, 0.7, a),
        ),
    );
}

/// A pictogram from the shared set, or the game's own ball.
#[derive(Clone, Copy, Debug)]
pub(super) enum Mark {
    Icon(Icon, Color),
    Ball,
}

impl Mark {
    pub fn draw(self, c: Vec2, size: f32, a: f32) {
        match self {
            Mark::Icon(icon, color) => icons::draw_styled(
                icon,
                c,
                size,
                IconStyle::new(fade(color, a)).shadow(Color::new(0., 0., 0., 0.35 * a)),
            ),
            Mark::Ball => ball(c, size * 0.36, a),
        }
    }
}

/// A mark then `text`, vertically centered on `cy`. Returns the pair's width.
pub(super) fn mark_value(mark: Mark, x: f32, cy: f32, size: f32, text: &str, style: Type) -> f32 {
    mark.draw(vec2(x + size * 0.5, cy), size, style.color.a);
    let style = style.with_align(TextAlign::Left);
    let w = draw_text_styled(text, x + size + GAP, style.centered_baseline(cy), style).width;
    size + GAP + w
}

/// Width `mark_value` will use.
pub(super) fn mark_value_width(size: f32, text: &str, style: Type) -> f32 {
    size + GAP + text_width(text, style)
}

/// Color of a round's to-par: mint under, paper even, clay over.
pub(super) fn to_par_color(to_par: i16) -> Color {
    match to_par {
        i16::MIN..=-1 => MINT,
        0 => PAPER,
        _ => CLAY,
    }
}

/// The round's to-par as a tinted pill; `pop` (≥ 1) briefly enlarges the
/// number after it changes.
pub(super) fn to_par_badge(r: Rect, to_par: i16, text: &str, size: f32, a: f32, pop: f32) {
    let color = to_par_color(to_par);
    let radius = r.h.min(r.w) * 0.5;
    shapes::fill_rounded_rect(
        r,
        radius,
        Fill::vertical(r, fade(color, 0.3 * a), fade(color, 0.12 * a)),
    );
    shapes::stroke_rounded_rect(r, radius, 1.5, fade(color, 0.8 * a));
    let base = Type::classic(size, fade(color, a));
    let mut style = base;
    while style.size > 8. && text_width(text, style) > r.w - 8. {
        style.size -= 8.;
    }
    let style = style
        .with_size(style.size * pop)
        .with_align(TextAlign::Center)
        .with_shadow(Color::new(0., 0., 0., 0.4 * a));
    let c = r.center();
    draw_text_styled(text, c.x, style.centered_baseline(c.y), style);
}

/// Twelve hole pips from `x`, `step` apart: played holes in their result
/// color, the current hole a breathing copper ring, holes ahead hollow and
/// holes a practice run skipped faint.
#[allow(clippy::too_many_arguments)]
pub(super) fn hole_pips(
    x: f32,
    cy: f32,
    step: f32,
    round: &Round,
    pars: &[u8; 12],
    a: f32,
    breathe: f32,
) {
    let r = (step * 0.3).clamp(2., 4.);
    for (i, &par) in pars.iter().enumerate() {
        let c = vec2(x + step * (i as f32 + 0.5), cy);
        let current = i == round.hole_index && round.phase != Phase::Finished;
        if current {
            shapes::glow_disc(c, r * 3.2, fade(GOLD, (0.18 + 0.2 * breathe) * a));
        }
        match round.scores[i] {
            Some(v) => {
                let name = ScoreName::of(v, par, round.picked_up[i]);
                shapes::fill_circle(c, r, fade(tone_color(name.tone()), a));
            }
            None if current => {
                shapes::fill_circle(c, r + 0.5, fade(CLAY, a));
                shapes::fill_circle(c, r * 0.4, fade(DARK, a));
            }
            None if round.practice && i < round.hole_index => {
                shapes::fill_circle(c, r * 0.5, fade(DIM, 0.25 * a));
            }
            None => shapes::stroke_circle(c, r - 0.4, 1.2, fade(DIM, 0.6 * a)),
        }
        if current {
            shapes::stroke_circle(c, r + 2.2, 1.2, fade(GOLD, (0.55 + 0.45 * breathe) * a));
        }
    }
}

fn square(c: Vec2, half: f32, width: f32, color: Color) {
    shapes::stroke_rounded_rect(
        Rect::new(c.x - half, c.y - half, half * 2., half * 2.),
        2.,
        width,
        color,
    );
}

/// Scorecard notation centered on `c` (a `size` box): a circle for a birdie,
/// two for an eagle or better, a solid gold medal for an ace, a square for a
/// bogey and two for worse. Returns the color to write the number in.
pub(super) fn notation(c: Vec2, size: f32, name: ScoreName, a: f32) -> Color {
    let color = fade(tone_color(name.tone()), a);
    let r = size * 0.5;
    let width = 1.5;
    match name {
        ScoreName::Ace => {
            shapes::glow_disc(c, r * 1.9, fade(GOLD, 0.35 * a));
            shapes::fill_circle(
                c,
                r,
                Fill::radial(c - vec2(r * 0.3, r * 0.3), r * 1.4, fade(PAPER, a), color),
            );
            return fade(DARK, a);
        }
        ScoreName::Albatross | ScoreName::Eagle => {
            shapes::stroke_circle(c, r, width, color);
            shapes::stroke_circle(c, r - 3., width, color);
        }
        ScoreName::Birdie => shapes::stroke_circle(c, r, width, color),
        ScoreName::Par => {}
        ScoreName::Bogey => square(c, r - 1., width, color),
        ScoreName::PickedUp => {
            square(c, r - 1., width, fade(DIM, a));
            return fade(DIM, a);
        }
        ScoreName::DoubleBogey | ScoreName::TripleBogey | ScoreName::Over(_) => {
            square(c, r - 1., width, color);
            square(c, r - 4., width, color);
        }
    }
    color
}

/// A rounded tag pill with an optional mark, left edge at `x` and centered
/// on `cy`. Returns its width (zero when `text` and `mark` are both empty).
pub(super) fn tag(x: f32, cy: f32, mark: Option<Mark>, text: &str, color: Color, a: f32) -> f32 {
    let w = tag_width(mark.is_some(), text);
    if w <= 0. {
        return 0.;
    }
    let r = Rect::new(x, cy - 9., w, 18.);
    shapes::fill_rounded_rect(r, 9., fade(color, 0.16 * a));
    shapes::stroke_rounded_rect(r, 9., 1., fade(color, 0.55 * a));
    let mut tx = x + 7.;
    if let Some(mark) = mark {
        mark.draw(vec2(tx + 6., cy), 12., a);
        tx += 12. + 3.;
    }
    let style = Type::modern(8., fade(color, a));
    draw_text_styled(text, tx, style.centered_baseline(cy), style);
    w
}

/// Width `tag` will draw.
pub(super) fn tag_width(mark: bool, text: &str) -> f32 {
    let text_w = text_width(text, Type::modern(8., PAPER));
    if !mark && text.is_empty() {
        return 0.;
    }
    7. + if mark { 12. + 3. } else { 0. } + text_w + 7.
}

/// A larger tag for the hole title card: a mark and an optional short value
/// in the classic face. Returns its width.
pub(super) fn big_tag(x: f32, cy: f32, mark: Mark, text: &str, color: Color, a: f32) -> f32 {
    let w = big_tag_width(text);
    let r = Rect::new(x, cy - 13., w, 26.);
    shapes::fill_rounded_rect(r, 13., fade(color, 0.14 * a));
    shapes::stroke_rounded_rect(r, 13., 1., fade(color, 0.5 * a));
    mark.draw(vec2(x + 9. + 9., cy), 18., a);
    if !text.is_empty() {
        let style =
            Type::classic(16., fade(color, a)).with_shadow(Color::new(0., 0., 0., 0.35 * a));
        draw_text_styled(text, x + 9. + 18. + 4., style.centered_baseline(cy), style);
    }
    w
}

/// Width `big_tag` will draw.
pub(super) fn big_tag_width(text: &str) -> f32 {
    let text_w = if text.is_empty() {
        0.
    } else {
        4. + text_width(text, Type::classic(16., PAPER))
    };
    9. + 18. + text_w + 9.
}

/// The bank-shot mark: a ball's path bouncing off a rail.
pub(super) fn bank(c: Vec2, size: f32, color: Color) {
    let s = size * 0.5;
    shapes::polyline(
        &[c + vec2(-s, s * 0.9), c + vec2(-s, -s)],
        Stroke::new(2., fade(color, color.a * 0.6)),
    );
    shapes::polyline(
        &[
            c + vec2(s * 0.9, s * 0.7),
            c + vec2(-s * 0.7, -s * 0.05),
            c + vec2(s * 0.7, -s * 0.8),
        ],
        Stroke::new(1.6, color),
    );
    shapes::fill_circle(c + vec2(s * 0.7, -s * 0.8), size * 0.14, color);
}

fn hole_number_style(r: f32) -> Type {
    Type::classic(if r >= 11. { 16. } else { 8. }, DARK).with_align(TextAlign::Center)
}

/// Width of a hole-number sign of radius `r`: round for one digit, a pill
/// for two.
pub(super) fn hole_number_width(r: f32, number: &str) -> f32 {
    (text_width(number, hole_number_style(r)) + r * 0.9).max(r * 2.)
}

/// A lip-out: the ball perched on the cup's rim.
pub(super) fn lip(c: Vec2, size: f32, a: f32) {
    let r = size * 0.36;
    let cup = c + vec2(-size * 0.08, size * 0.08);
    shapes::fill_circle(cup, r, fade(Color::from_rgba(10, 22, 18, 255), a));
    shapes::stroke_circle(cup, r, 1.5, fade(PAPER, 0.8 * a));
    ball(cup + vec2(r * 0.75, -r * 0.75), size * 0.2, a);
}

/// A golf hole's number on a copper tee sign centered on `c`.
pub(super) fn hole_number(c: Vec2, r: f32, number: &str, a: f32) {
    let w = hole_number_width(r, number);
    let sign = Rect::new(c.x - w * 0.5, c.y - r, w, r * 2.);
    shapes::fill_rounded_rect(
        Rect::new(sign.x, sign.y + 1.5, sign.w, sign.h),
        r,
        Color::new(0., 0., 0., 0.3 * a),
    );
    shapes::fill_rounded_rect(
        sign,
        r,
        Fill::vertical(sign, fade(CLAY, a), fade(COPPER, a)),
    );
    shapes::stroke_rounded_rect(sign, r, 1., Color::new(1., 0.9, 0.75, 0.5 * a));
    let style = hole_number_style(r);
    let style = style.with_color(fade(style.color, a));
    draw_text_styled(number, c.x, style.centered_baseline(c.y), style);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_tags_cover_every_hud_count() {
        for n in 0..=24 {
            assert_eq!(num(n), n.to_string());
        }
        assert_eq!(num(99), "24+");
        assert_eq!(tag_width(false, ""), 0.);
        assert!(tag_width(true, "") > 0.);
        assert!(tag_width(true, "NEW BEST") > tag_width(false, "NEW BEST"));
        assert_eq!(to_par_color(-2), MINT);
        assert_eq!(to_par_color(0), PAPER);
        assert_eq!(to_par_color(3), CLAY);
    }
}
