//! Strike's art direction: palette, post-processing looks, the modern type
//! scale and a small allocation-free painter for HUD panels and text.
//!
//! Everything here draws immediate geometry (anti-aliased shapes and the
//! engine's styled text), which emits far fewer vertices than per-pixel
//! classic glyph quads. Strings the browser tests inspect (vitals, replay
//! footer and the primary menu button fill) stay on the classic `Ui` path.
use ferravex_play::tween::Motion;
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Fill, Shadow};
use ferravex_render::text::{self, FontFace, TextAlign, TextStyle};

pub(super) const INK: Color = Color::from_hex(0xf2efe6);
pub(super) const MUTED: Color = Color::from_hex(0xa3b3ab);
/// Safety amber: the local player, primary actions and rewards.
pub(super) const AMBER: Color = Color::from_hex(0xffbf47);
/// Hi-vis lime: streaks, hints and "go" states.
pub(super) const LIME: Color = Color::from_hex(0xc9f25a);
/// Signal red: enemies, damage and headshots.
pub(super) const SIGNAL: Color = Color::from_hex(0xff5d4a);
/// Harbor teal: information and the crew.
pub(super) const TEAL: Color = Color::from_hex(0x63dccb);
/// Translucent slate for HUD plates.
pub(super) const PLATE: Color = Color::new(0.035, 0.055, 0.06, 0.74);
pub(super) const PLATE_SOLID: Color = Color::new(0.045, 0.07, 0.075, 0.94);
pub(super) const HAIRLINE: Color = Color::new(1., 1., 1., 0.09);
pub(super) const SHADOW: Color = Color::new(0., 0., 0., 0.55);

/// Sizes in multiples of 8 stay pixel-crisp; 12 is the compact HUD step.
pub(super) const SMALL: f32 = 12.;
pub(super) const BODY: f32 = 16.;
pub(super) const TITLE: f32 = 24.;
pub(super) const DISPLAY: f32 = 32.;

pub(super) fn fade(c: Color, alpha: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * alpha.clamp(0., 1.))
}

/// Modern body face with a one-pixel drop shadow.
pub(super) fn modern(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color).with_shadow(fade(SHADOW, color.a))
}

/// Classic arcade face with an outline, for numbers and callouts.
pub(super) fn classic(size: f32, color: Color) -> TextStyle {
    TextStyle::classic(size, color).with_outline(Color::new(0.02, 0.03, 0.03, 0.85 * color.a))
}

pub(super) fn width(text: &str, style: TextStyle) -> f32 {
    text::text_width(text, style)
}

/// Largest size not above `style.size` (and at least `min`) that fits `max`.
pub(super) fn fit(text: &str, style: TextStyle, max: f32, min: f32) -> TextStyle {
    let w = width(text, style);
    if w <= max || w <= 0. {
        style
    } else {
        style.with_size((style.size * max / w).max(min))
    }
}

/// Draws one line at baseline `y`; returns its painted rectangle.
pub(super) fn line(text: &str, x: f32, y: f32, style: TextStyle, align: TextAlign) -> Rect {
    let style = style.with_align(align);
    let metrics = text::draw_text_styled(text, x, y, style);
    let left = match align {
        TextAlign::Left => x,
        TextAlign::Center => x - metrics.width * 0.5,
        TextAlign::Right => x - metrics.width,
    };
    let rect = Rect::new(left, y - style.size, metrics.width, style.size * 1.25);
    record(rect);
    rect
}

/// [`line`] for formatted values, without allocating.
pub(super) fn line_fmt(
    x: f32,
    y: f32,
    style: TextStyle,
    align: TextAlign,
    args: std::fmt::Arguments<'_>,
) -> Rect {
    let buf = Buf::<128>::fmt(args);
    line(buf.as_str(), x, y, style, align)
}

/// A HUD plate: soft shadow, rounded translucent fill and a hairline edge.
pub(super) fn plate(rect: Rect, radius: f32, fill: Color, alpha: f32) {
    if rect.w <= 0. || rect.h <= 0. || alpha <= 0. {
        return;
    }
    shapes::drop_shadow(
        rect,
        radius,
        Shadow::soft(fade(Color::new(0., 0., 0., 0.35), alpha))
            .offset(vec2(0., 3.))
            .blur(12.),
    );
    let top = Color::new(
        (fill.r + 0.03).min(1.),
        (fill.g + 0.035).min(1.),
        (fill.b + 0.035).min(1.),
        fill.a,
    );
    shapes::fill_rounded_rect(
        rect,
        radius,
        Fill::vertical(rect, fade(top, alpha), fade(fill, alpha)),
    );
    shapes::stroke_rounded_rect(rect, radius, 1., fade(HAIRLINE, alpha));
    record(rect);
}

/// A thin colored accent bar on the left edge of a plate.
pub(super) fn accent(rect: Rect, color: Color, alpha: f32) {
    shapes::fill_rounded_rect(
        Rect::new(rect.x, rect.y + 3., 3., (rect.h - 6.).max(0.)),
        1.5,
        fade(color, alpha),
    );
}

/// A tiny crosshair-in-circle glyph marking headshots.
pub(super) fn headshot_icon(center: Vec2, radius: f32, color: Color) {
    shapes::stroke_circle(center, radius, 1.5, color);
    shapes::fill_circle(center, radius * 0.32, color);
}

/// `Motion` from the shared preferences, read once per frame by callers.
pub(super) fn motion() -> Motion {
    Motion::from(ferravex_render::preferences::feedback_policy())
}

/// Post-processing mood per screen. Applied only when it changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Look {
    /// Live play: bloom only on muzzle flashes, tracers, visors and hi-vis;
    /// a cool-shadow/warm-highlight harbor grade and a gentle vignette.
    Play,
    /// Eliminated: desaturated, darker edges, a red cast.
    Down,
    /// Briefing/pause/results: cinematic, darker and softer behind cards.
    Menu,
}

impl Look {
    pub fn post(self) -> PostFx {
        match self {
            Look::Play => PostFx::subtle()
                .bloom(1.35, 0.86, 1.15)
                .bloom_knee(0.2)
                .contrast(1.07)
                .saturation(1.1)
                .lift_gamma_gain([0., 0.006, 0.016], [1., 1., 1.], [1.04, 1., 0.95])
                .vignette(0.26, 0.6),
            Look::Down => PostFx::subtle()
                .bloom(1.0, 0.86, 1.2)
                .contrast(1.1)
                .saturation(0.45)
                .lift_gamma_gain([0.02, 0., 0.], [1., 0.97, 0.97], [1.12, 0.9, 0.86])
                .vignette(0.5, 0.5),
            Look::Menu => PostFx::subtle()
                .bloom(1.1, 0.84, 1.3)
                .exposure(0.94)
                .contrast(1.05)
                .saturation(0.92)
                .lift_gamma_gain([0., 0.008, 0.02], [1., 1., 1.], [1.02, 1., 0.96])
                .vignette(0.42, 0.55),
        }
    }
}

/// Remembers the applied look so `set_post_fx` runs only on changes.
#[derive(Default)]
pub(super) struct LookState(Option<Look>);

impl LookState {
    pub fn apply(&mut self, look: Look) {
        if self.0 != Some(look) {
            self.0 = Some(look);
            set_post_fx(look.post());
        }
    }
}

/// Fixed-capacity UTF-8 formatting buffer: per-frame text without heap use.
/// Overlong output is truncated at a character boundary.
pub(super) struct Buf<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> Buf<N> {
    pub fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }
    pub fn fmt(args: std::fmt::Arguments<'_>) -> Self {
        let mut buf = Self::new();
        let _ = std::fmt::Write::write_fmt(&mut buf, args);
        buf
    }
    pub fn as_str(&self) -> &str {
        // Only whole `str` pieces are copied, so this is always valid UTF-8.
        std::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> std::fmt::Write for Buf<N> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let room = N - self.len;
        let mut take = s.len().min(room);
        while !s.is_char_boundary(take) {
            take -= 1;
        }
        self.bytes[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

/// Classic face, re-exported for callers that pick a face explicitly.
#[allow(dead_code)]
pub(super) const CLASSIC: FontFace = FontFace::Classic;

#[cfg(test)]
thread_local! {
    static RECORDED: std::cell::RefCell<Option<Vec<Rect>>> = const { std::cell::RefCell::new(None) };
}

/// Tests can collect every painted text/plate rectangle (see corner tests).
#[cfg(test)]
pub(super) fn recording<T>(run: impl FnOnce() -> T) -> (T, Vec<Rect>) {
    RECORDED.with(|r| *r.borrow_mut() = Some(Vec::new()));
    let out = run();
    let rects = RECORDED.with(|r| r.borrow_mut().take()).unwrap_or_default();
    (out, rects)
}

#[cfg(test)]
fn record(rect: Rect) {
    RECORDED.with(|r| {
        if let Some(list) = r.borrow_mut().as_mut() {
            list.push(rect);
        }
    });
}

#[cfg(not(test))]
#[inline(always)]
fn record(_: Rect) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_buffers_format_without_allocating_and_truncate_on_char_boundaries() {
        assert_eq!(
            Buf::<16>::fmt(format_args!("{}:{:02}", 1, 7)).as_str(),
            "1:07"
        );
        assert_eq!(Buf::<5>::fmt(format_args!("ab→cd")).as_str(), "ab→");
        assert_eq!(Buf::<4>::fmt(format_args!("ab→cd")).as_str(), "ab");
    }

    #[test]
    fn every_look_is_a_valid_bloom_on_emissives_only_grade() {
        for look in [Look::Play, Look::Down, Look::Menu] {
            let fx = look.post();
            assert_eq!(fx, fx.sanitized(), "{look:?} stays in documented ranges");
            assert!(
                fx.bloom_threshold >= 0.84,
                "the pale harbor sky must not bloom"
            );
            assert!(fx.vignette > 0. && fx.vignette < 0.6);
        }
        assert!(Look::Down.post().saturation < Look::Play.post().saturation);
    }
}
