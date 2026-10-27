//! Emberline's modern art direction: palette, kit theme, type scale, post
//! looks and small allocation-free text helpers shared by every overlay.
//!
//! The Modern face carries copy, hints and quips; the Classic 5×7 face is kept
//! where its arcade punch is the point (logo, clock, counters, callouts).
use ferravex_play::tween::Motion;
use ferravex_render::post::{PostFx, Tonemap};
use ferravex_render::prelude::{Color, Rect, vec2};
use ferravex_render::shapes::{self, Fill, Shadow};
use ferravex_render::text::{self, TextAlign, TextStyle};
use ferravex_render::ui::kit::{self, KitTheme};
use std::fmt::Write;

pub(super) const INK: Color = Color::from_hex(0xf6ead2);
pub(super) const MUTED: Color = Color::from_hex(0xa9b29c);
pub(super) const EMBER: Color = Color::from_hex(0xff8a40);
pub(super) const GOLD: Color = Color::from_hex(0xffcc5c);
pub(super) const LEAF: Color = Color::from_hex(0x8fd18a);
pub(super) const DANGER: Color = Color::from_hex(0xff6a52);
pub(super) const NIGHT: Color = Color::from_hex(0x0b1216);
pub(super) const SHADOW: Color = Color::new(0.02, 0.03, 0.03, 0.75);

/// Translucent "glass" behind HUD chips and toasts.
pub(super) const GLASS: Color = Color::new(0.05, 0.08, 0.08, 0.72);
pub(super) const GLASS_EDGE: Color = Color::new(1., 0.92, 0.78, 0.10);

pub(super) fn fade(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha)
}

/// Kit roles mapped onto Emberline's ember, cream and moss palette.
pub(super) fn theme() -> KitTheme {
    KitTheme {
        backdrop: Color::new(0.01, 0.02, 0.02, 0.55),
        surface: Color::from_hex(0x141d1c),
        raised: Color::from_hex(0x22302d),
        header: Color::from_hex(0x2a3a36),
        ink: INK,
        muted: MUTED,
        disabled: Color::from_hex(0x5b6559),
        accent: EMBER,
        accent_ink: Color::from_hex(0x2a1306),
        success: LEAF,
        warning: GOLD,
        danger: DANGER,
        focus: GOLD,
        line: Color::new(1., 0.9, 0.75, 0.10),
        shadow: Color::new(0., 0., 0., 0.55),
        ..KitTheme::default()
    }
}

/// Body copy: the Modern face with a soft 1-pixel drop shadow.
pub(super) fn body(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color).with_shadow(fade(SHADOW, color.a))
}

/// Arcade numerals and callouts in the Classic face.
pub(super) fn arcade(size: f32, color: Color) -> TextStyle {
    TextStyle::classic(size, color).with_shadow(fade(SHADOW, color.a))
}

/// Big outlined Classic type for the logo, ranks and score pops.
pub(super) fn display(size: f32, color: Color) -> TextStyle {
    TextStyle::classic(size, color).with_outline(fade(Color::from_hex(0x1a0e06), color.a))
}

pub(super) fn draw(text: &str, x: f32, y: f32, style: TextStyle) -> f32 {
    text::draw_text_styled(text, x, y, style).width
}

pub(super) fn center(text: &str, x: f32, y: f32, style: TextStyle) -> f32 {
    draw(text, x, y, style.with_align(TextAlign::Center))
}

pub(super) fn right(text: &str, x: f32, y: f32, style: TextStyle) -> f32 {
    draw(text, x, y, style.with_align(TextAlign::Right))
}

pub(super) fn width(text: &str, style: TextStyle) -> f32 {
    text::text_width(text, style)
}

/// Centered text shrunk (never grown) to fit `max_width`.
pub(super) fn center_fit(text: &str, x: f32, y: f32, max_width: f32, style: TextStyle) {
    let w = width(text, style).max(1.);
    let style = if w > max_width {
        style.with_size(style.size * max_width / w)
    } else {
        style
    };
    center(text, x, y, style);
}

/// A rounded translucent HUD chip. No drop shadow: chips are frequent and the
/// hairline edge reads well over the dark scenery at a fraction of the cost.
pub(super) fn chip(rect: Rect, alpha: f32) {
    shapes::fill_rounded_rect(
        rect,
        12.,
        Fill::vertical(
            rect,
            fade(Color::new(0.09, 0.13, 0.12, 0.78), alpha),
            fade(GLASS, alpha),
        ),
    );
    shapes::stroke_rounded_rect(rect, 12., 1., fade(GLASS_EDGE, alpha));
}

/// A raised overlay card with a soft shadow and ember top accent.
pub(super) fn card(rect: Rect, alpha: f32) {
    kit::panel(rect, &theme().faded(alpha));
    let accent = Rect::new(rect.x + 24., rect.y, rect.w - 48., 3.);
    shapes::fill_rounded_rect(
        accent,
        1.5,
        Fill::horizontal(accent, fade(EMBER, 0.), fade(EMBER, alpha)),
    );
}

/// A small inset tile used for stats and goals inside cards.
pub(super) fn tile(rect: Rect, alpha: f32) {
    shapes::fill_rounded_rect(rect, 10., fade(Color::new(0., 0., 0., 0.28), alpha));
    shapes::stroke_rounded_rect(rect, 10., 1., fade(Color::new(1., 1., 1., 0.05), alpha));
}

pub(super) fn soft_shadow(rect: Rect, radius: f32, alpha: f32) {
    shapes::drop_shadow(
        rect,
        radius,
        Shadow::soft(Color::new(0., 0., 0., 0.45 * alpha))
            .offset(vec2(0., 4.))
            .blur(14.),
    );
}

/// Draws up to two key caps for a hint such as `"Space / W / Up"`, then a
/// muted action label, returning the total width. Hints that are not short
/// key names (fallbacks such as "SEE CONTROLS") are drawn as plain text.
pub(super) fn key_prompt(x: f32, y: f32, hint: &str, action: &str, theme: &KitTheme) -> f32 {
    let mut keys = [""; 2];
    let mut count = 0;
    for key in hint.split(" / ") {
        if count < keys.len() {
            keys[count] = key;
            count += 1;
        }
    }
    if hint.chars().count() > 24 || keys[..count].iter().any(|k| k.len() > 10) {
        let style = body(16., fade(theme.muted, theme.alpha));
        let w = draw(
            hint,
            x,
            y + 17.,
            style.with_color(fade(theme.ink, theme.alpha)),
        );
        return w + 8. + draw(action, x + w + 8., y + 17., style);
    }
    kit::key_prompt(x, y, &keys[..count], action, theme)
}

/// Measures [`key_prompt`] without drawing, for centering rows of prompts.
pub(super) fn key_prompt_width(hint: &str, action: &str) -> f32 {
    let style = TextStyle::modern(16., INK);
    let mut total = 0.;
    let mut count = 0;
    for key in hint.split(" / ").take(2) {
        total += (width(key, style) + 24.).max(23.) + 4.;
        count += 1;
    }
    if hint.chars().count() > 24 || hint.split(" / ").take(2).any(|k| k.len() > 10) {
        return width(hint, style) + 8. + width(action, style);
    }
    let _ = count;
    total + 4. + width(action, style)
}

/// Reduced-motion aware oscillation in `-1..=1`, frozen when motion is off.
pub(super) fn wave(time: f32, rate: f32, motion: Motion) -> f32 {
    if motion.is_reduced() {
        0.
    } else {
        (time * rate).sin()
    }
}

/// Fixed-capacity UTF-8 text, so HUD labels format without heap allocation.
/// Text past the capacity is dropped at a character boundary.
#[derive(Clone, Copy)]
pub(super) struct Line {
    bytes: [u8; 64],
    len: u8,
}

impl Line {
    pub const fn new() -> Self {
        Self {
            bytes: [0; 64],
            len: 0,
        }
    }

    pub fn from_str(text: &str) -> Self {
        let mut line = Self::new();
        let _ = line.write_str(text);
        line
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or("")
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Line {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}

impl PartialEq for Line {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl PartialEq<&str> for Line {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl std::ops::Deref for Line {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl Write for Line {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let mut used = usize::from(self.len);
        for c in s.chars() {
            let n = c.len_utf8();
            if used + n > self.bytes.len() {
                return Err(std::fmt::Error);
            }
            c.encode_utf8(&mut self.bytes[used..used + n]);
            used += n;
            self.len = used as u8;
        }
        Ok(())
    }
}

/// `line!("{} pts", score)` without allocating.
macro_rules! line {
    ($($arg:tt)*) => {{
        let mut line = $crate::style::Line::new();
        let _ = ::std::fmt::Write::write_fmt(&mut line, format_args!($($arg)*));
        line
    }};
}
pub(super) use line;

/// Post-processing look per scene. Thresholds sit near 1 so only emissive
/// accents (embers, lanterns, lava, beacons and additive glows) bloom; the
/// scenery and cream labels stay crisp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Scene {
    Title,
    Play,
    Paused,
    Results,
}

impl Scene {
    pub fn post(self) -> PostFx {
        let dusk = PostFx::subtle()
            .bloom(1.5, 0.93, 1.25)
            .bloom_knee(0.2)
            .tonemap(Tonemap::Soft)
            .contrast(1.07)
            .saturation(1.08)
            .lift_gamma_gain([0.0, 0.008, 0.018], [1., 1., 1.], [1.05, 1.0, 0.93])
            .vignette(0.3, 0.6);
        match self {
            Self::Play => dusk,
            // Menus push the world back: dimmer, cooler, deeper corners.
            Self::Title => dusk.exposure(0.86).vignette(0.48, 0.55),
            Self::Paused => dusk.exposure(0.72).saturation(0.7).vignette(0.55, 0.5),
            Self::Results => dusk.bloom(2.0, 0.9, 1.4).vignette(0.42, 0.55),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_format_without_allocating_and_truncate_on_char_boundaries() {
        let mut clock = line!("{:.2}", 9.104_f64);
        assert_eq!(clock, "9.10");
        let _ = clock.write_str(" ×");
        assert_eq!(clock.as_str(), "9.10 ×");
        let long = Line::from_str(&"·".repeat(40));
        assert!(long.as_str().len() <= 64);
        assert!(long.as_str().chars().all(|c| c == '·'));
        assert!(Line::new().is_empty());
    }

    #[test]
    fn every_scene_has_a_bounded_finite_post_look() {
        for scene in [Scene::Title, Scene::Play, Scene::Paused, Scene::Results] {
            let fx = scene.post().sanitized();
            assert!(!fx.is_off());
            assert!(fx.bloom_threshold >= 0.9, "only emissive accents bloom");
            assert!(fx.vignette > 0. && fx.grain == 0. && fx.aberration == 0.);
        }
    }
}
