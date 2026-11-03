//! Presentation vocabulary shared by every arcade game.
//!
//! All coordinates are logical canvas units (960×640). Text uses the modern
//! proportional pixel face; [`text`] keeps each label within the width its
//! author laid out for the original 5×7 face, so older layouts never overflow.
use crate::*;
use ferravex_render::shapes::{self, Fill, Shadow};
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};

/// Modern-face style used by [`text`] at `size` in `color`.
///
/// Light ink at display sizes gets a one-pixel drop shadow so it stays crisp
/// over busy art. Small labels and dark ink (for light paper panels) are drawn
/// without one, which halves their geometry. Use [`TextStyle::with_shadow`]
/// to add one explicitly.
pub fn style(size: f32, color: Color) -> TextStyle {
    let s = TextStyle::modern(size, color);
    if luminance(color) > 0.42 && color.a > 0.35 && size >= 15. {
        s.with_shadow(Color::new(0., 0., 0., 0.42 * color.a))
    } else {
        s
    }
}
fn luminance(c: Color) -> f32 {
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}
/// Width the original 5×7 face reserved for `value` at `size`.
fn classic_width(value: &str, size: f32) -> f32 {
    // Six font pixels per glyph, minus the trailing gap.
    (value.chars().count() as f32 * 6. - 1.).max(0.) * size / 8.
}
/// Style for `value`, shrunk only when the modern face would be wider than
/// the classic layout reserved.
fn fitted(value: &str, size: f32, color: Color) -> TextStyle {
    let s = style(size, color);
    let budget = classic_width(value, size);
    let actual = text_width(value, s);
    if actual > budget && actual > 0. {
        s.with_size(size * budget / actual)
    } else {
        s
    }
}
/// Draw a label with its baseline at `y`; returns the drawn width.
///
/// ```no_run
/// use ferravex_arcade::{PAPER, text};
/// let w = text("Collect the key", 18., 80., 18., PAPER);
/// assert!(w > 0.);
/// ```
pub fn text(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, fitted(value, size, color)).width
}
/// Draw a label centered on `cx`; returns the drawn width.
pub fn text_centered(value: &str, cx: f32, y: f32, size: f32, color: Color) -> f32 {
    let s = fitted(value, size, color).with_align(TextAlign::Center);
    draw_text_styled(value, cx, y, s).width
}
/// Draw a label whose right edge is `right`; returns the drawn width.
pub fn text_right(value: &str, right: f32, y: f32, size: f32, color: Color) -> f32 {
    let s = fitted(value, size, color).with_align(TextAlign::Right);
    draw_text_styled(value, right, y, s).width
}
/// Width [`text`] would draw for `value` at `size`.
///
/// ```
/// let w = ferravex_arcade::measure("Harbour", 16.);
/// assert!(w > 0. && w < 7. * 6. * 2.);
/// ```
pub fn measure(value: &str, size: f32) -> f32 {
    text_width(value, fitted(value, size, PAPER))
}
/// Draw a label shrunk to fit `width`, never enlarged.
pub fn fit(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) -> f32 {
    let s = style(size, color);
    let actual = text_width(value, s);
    let s = if actual > width && actual > 0. {
        s.with_size(size * width / actual)
    } else {
        s
    };
    draw_text_styled(value, x, y, s).width
}
/// Draw a flat rectangle. Use it for tiles, bars and pixel art that must meet
/// edge to edge; prefer [`card`] for interface surfaces.
pub fn panel(x: f32, y: f32, w: f32, h: f32, color: Color) {
    draw_rectangle(x, y, w, h, color);
}
/// Anti-aliased rounded rectangle.
pub fn rounded(x: f32, y: f32, w: f32, h: f32, radius: f32, color: Color) {
    shapes::fill_rounded_rect(Rect::new(x, y, w, h), radius.min(w.min(h) * 0.5), color);
}
/// Rounded rectangle with a top-to-bottom gradient.
pub fn rounded_gradient(x: f32, y: f32, w: f32, h: f32, radius: f32, top: Color, bottom: Color) {
    let r = Rect::new(x, y, w, h);
    shapes::fill_rounded_rect(
        r,
        radius.min(w.min(h) * 0.5),
        Fill::vertical(r, top, bottom),
    );
}
/// Rounded outline of `width` units.
pub fn outline(x: f32, y: f32, w: f32, h: f32, radius: f32, width: f32, color: Color) {
    shapes::stroke_rounded_rect(
        Rect::new(x, y, w, h),
        radius.min(w.min(h) * 0.5),
        width,
        color,
    );
}
/// Raised interface surface: soft shadow, gently lit gradient fill and a
/// hairline rim. `fill` is the surface's mid-tone.
///
/// ```no_run
/// use ferravex_arcade::{Color, card};
/// card(620., 130., 300., 420., Color::new(0.07, 0.1, 0.13, 1.));
/// ```
pub fn card(x: f32, y: f32, w: f32, h: f32, fill: Color) {
    let radius = 10_f32.min(w.min(h) * 0.5);
    let r = Rect::new(x, y, w, h);
    shapes::drop_shadow(
        r,
        radius,
        Shadow::soft(Color::new(0., 0., 0., 0.38 * fill.a))
            .offset(vec2(0., 4.))
            .blur(14.),
    );
    let lift = if luminance(fill) > 0.5 { 0.05 } else { 0.035 };
    shapes::fill_rounded_rect(
        r,
        radius,
        Fill::vertical(r, lighten(fill, lift), darken(fill, 0.02)),
    );
    let rim = if luminance(fill) > 0.5 {
        Color::new(0., 0., 0., 0.12 * fill.a)
    } else {
        Color::new(1., 1., 1., 0.07 * fill.a)
    };
    shapes::stroke_rounded_rect(r, radius, 1., rim);
}
/// Small rounded label (stage names, tags, states); returns its width.
/// The pill is tinted with `color`, which is also the text color.
pub fn chip(label: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    let s = style(size, color);
    let w = text_width(label, s);
    let h = size * 1.45;
    let pad = size * 0.65;
    let r = Rect::new(x, y, w + pad * 2., h);
    shapes::fill_rounded_rect(r, h * 0.5, with_alpha(color, 0.14));
    shapes::stroke_rounded_rect(r, h * 0.5, 1., with_alpha(color, 0.38));
    draw_text_styled(label, x + pad, s.centered_baseline(y + h * 0.5), s);
    r.w
}
/// Keyboard key cap with an action label after it; returns the total width.
pub fn keycap(key: &str, action: &str, x: f32, y: f32, ink: Color) -> f32 {
    keycap_faded(key, action, x, y, ink, 1.)
}
pub(crate) fn keycap_faded(key: &str, action: &str, x: f32, y: f32, ink: Color, a: f32) -> f32 {
    let s = TextStyle::modern(13., with_alpha(PAPER, a));
    let h = 20.;
    let w = (text_width(key, s) + 12.).max(h);
    let lip = Rect::new(x, y + 2., w, h);
    let cap = Rect::new(x, y, w, h);
    shapes::fill_rounded_rect(lip, 4., Color::new(0., 0., 0., 0.5 * a));
    shapes::fill_rounded_rect(
        cap,
        4.,
        Fill::vertical(
            cap,
            Color::new(0.2, 0.24, 0.3, a),
            Color::new(0.13, 0.16, 0.21, a),
        ),
    );
    shapes::stroke_rounded_rect(cap, 4., 1., Color::new(1., 1., 1., 0.1 * a));
    draw_text_styled(
        key,
        x + w * 0.5,
        s.centered_baseline(y + h * 0.5),
        s.with_align(TextAlign::Center),
    );
    if action.is_empty() {
        return w;
    }
    let label = TextStyle::modern(13., with_alpha(ink, ink.a * a));
    let lw = draw_text_styled(
        action,
        x + w + 7.,
        label.centered_baseline(y + h * 0.5),
        label,
    )
    .width;
    w + 7. + lw
}
/// Hairline that fades out at both ends.
pub fn divider(x: f32, y: f32, w: f32, color: Color) {
    let half = w * 0.5;
    let clear = with_alpha(color, 0.);
    let left = Rect::new(x, y, half, 1.);
    let right = Rect::new(x + half, y, half, 1.);
    shapes::fill_rounded_rect(left, 0., Fill::horizontal(left, clear, color));
    shapes::fill_rounded_rect(right, 0., Fill::horizontal(right, color, clear));
}
/// Additive soft glow, for lamps, pickups, highlights and selections.
pub fn glow(x: f32, y: f32, radius: f32, color: Color) {
    shapes::glow_disc(vec2(x, y), radius, color);
}
/// Draw a bounded fraction over a recessed rounded track.
pub fn meter(x: f32, y: f32, w: f32, h: f32, fraction: f32, color: Color) {
    let f = if fraction.is_finite() {
        fraction.clamp(0., 1.)
    } else {
        0.
    };
    let radius = (h * 0.5).min(4.);
    let track = Rect::new(x, y, w, h);
    shapes::fill_rounded_rect(track, radius, Color::new(0.02, 0.03, 0.05, 0.55));
    if f > 0. {
        let fill = Rect::new(x, y, (w * f).max(h.min(w * f * 4.)), h);
        shapes::fill_rounded_rect(
            fill,
            radius,
            Fill::vertical(fill, lighten(color, 0.16), color),
        );
        if h >= 6. {
            let shine = Rect::new(fill.x + radius, y + 1., (fill.w - radius * 2.).max(0.), 1.);
            shapes::fill_rounded_rect(shine, 0., Color::new(1., 1., 1., 0.22));
        }
    }
}
/// Wrap a paragraph at measured word boundaries; return the next baseline.
pub fn wrapped(value: &str, x: f32, y: f32, max_width: f32, size: f32, color: Color) -> f32 {
    let s = style(size, color);
    let mut baseline = y;
    for line in wrap_text(value, max_width, s) {
        draw_text_styled(line, x, baseline, s);
        baseline += size * 1.3;
    }
    baseline
}
/// `color` with its alpha replaced.
pub fn with_alpha(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, alpha)
}
/// Mix `color` toward white by `amount` (0..1), keeping alpha.
pub fn lighten(color: Color, amount: f32) -> Color {
    let m = shapes::mix(color, Color::new(1., 1., 1., color.a), amount);
    with_alpha(m, color.a)
}
/// Mix `color` toward black by `amount` (0..1), keeping alpha.
pub fn darken(color: Color, amount: f32) -> Color {
    let m = shapes::mix(color, Color::new(0., 0., 0., color.a), amount);
    with_alpha(m, color.a)
}
/// Borrow the renderer's measured line wrapping, then paginate without dropping
/// words or shrinking instructions. Empty separator lines preserve paragraphs.
pub(crate) fn guide_pages<'a>(paragraphs: &[&'a str]) -> Vec<Vec<&'a str>> {
    let mut pages = Vec::new();
    let mut page = Vec::new();
    for paragraph in paragraphs {
        let lines: Vec<_> = wrap_text(paragraph, GUIDE_WIDTH, style(18., PAPER)).collect();
        if !page.is_empty() && page.len() + lines.len() + 1 > GUIDE_LINES {
            pages.push(std::mem::take(&mut page));
        } else if !page.is_empty() {
            page.push("");
        }
        for line in lines {
            if page.len() == GUIDE_LINES {
                pages.push(std::mem::take(&mut page));
            }
            page.push(line);
        }
    }
    if !page.is_empty() {
        pages.push(page);
    }
    if pages.is_empty() {
        pages.push(Vec::new());
    }
    pages
}
pub(crate) const GUIDE_WIDTH: f32 = 600.;
pub(crate) const GUIDE_LINES: usize = 9;
#[derive(Clone, Copy)]
pub(crate) struct Viewport {
    pub scale: f32,
    pub offset: Vec2,
    pub size: Vec2,
}
impl Viewport {
    pub fn new(w: f32, h: f32) -> Self {
        Self::with_bottom_inset(w, h, 0.)
    }
    pub fn with_bottom_inset(w: f32, h: f32, inset: f32) -> Self {
        let size = vec2(w.max(1.), h.max(1.));
        let available = vec2(size.x, (size.y - inset).max(1.));
        let scale = (available.x / WIDTH).min(available.y / HEIGHT);
        Self {
            scale,
            offset: (available - vec2(WIDTH, HEIGHT) * scale) * 0.5,
            size,
        }
    }
    pub fn pointer(self, p: Vec2) -> Vec2 {
        (p - self.offset) / self.scale
    }
    pub fn apply(self) {
        self.apply_shifted(Vec2::ZERO);
    }
    /// Apply with a logical-unit offset (screen shake).
    pub fn apply_shifted(self, shift: Vec2) {
        set_camera(&Camera2D {
            target: (self.size * 0.5 - self.offset) / self.scale - shift,
            zoom: vec2(2. * self.scale / self.size.x, 2. * self.scale / self.size.y),
            ..Default::default()
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn letterboxing_preserves_aspect_and_pointer_roundtrip() {
        for (w, h) in [(1200., 800.), (390., 844.), (1600., 600.)] {
            let v = Viewport::new(w, h);
            let logical = vec2(200., 300.);
            assert!((v.pointer(logical * v.scale + v.offset) - logical).length() < 0.001);
            assert_eq!(v.pointer(v.offset), Vec2::ZERO);
            assert!(v.offset.x >= 0. && v.offset.y >= 0.);
        }
    }
    #[test]
    fn guide_pagination_keeps_every_word_at_readable_size() {
        let long = "A careful captain checks the winds before sailing. ".repeat(24);
        let paragraphs = [
            long.as_str(),
            "The final instruction must remain reachable.",
        ];
        let pages = guide_pages(&paragraphs);
        assert!(pages.len() > 2);
        assert!(pages.iter().all(|page| page.len() <= GUIDE_LINES));
        let actual = pages
            .iter()
            .flatten()
            .flat_map(|line| line.split_whitespace())
            .collect::<Vec<_>>();
        let expected = paragraphs
            .iter()
            .flat_map(|line| line.split_whitespace())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
        assert!(
            pages
                .iter()
                .flatten()
                .all(|line| text_width(line, style(18., PAPER)) <= GUIDE_WIDTH + 0.5)
        );
    }
    #[test]
    fn modern_labels_never_exceed_their_classic_layout() {
        for label in [
            "WWWWWWWW",
            "MOSSLIGHT",
            "Bramble knight falls. Its attack is cancelled.",
            "000990",
            "IIIIII",
        ] {
            for size in [12., 17., 25.] {
                assert!(measure(label, size) <= classic_width(label, size) + 0.01);
            }
        }
        // Lowercase-heavy prose keeps its full authored size.
        let prose = "the quick brown fox";
        assert_eq!(fitted(prose, 16., PAPER).size, 16.);
    }
    #[test]
    fn dark_ink_has_no_shadow_and_light_ink_does() {
        assert!(style(16., INK).shadow.is_none());
        assert!(style(16., PAPER).shadow.is_some());
    }
}
