//! Marble Run's art direction in one place: post-processing per scene, the
//! HUD/menu palette, rounded panels, the modern/classic type pairing and a
//! fixed-capacity text line so steady-state HUD frames format without heap
//! allocation.
use ferravex_engine::play::tween::Motion;
use ferravex_render::{
    post::PostFx,
    prelude::*,
    shapes::{self, Fill, Shadow},
    text::{TextAlign, TextStyle, draw_text_styled, text_width},
    ui::kit::{self, KitTheme},
};
use std::fmt::{self, Write};

/// Warm cream ink for copy on dark glass.
pub(crate) const INK: Color = Color::new(0.97, 0.94, 0.86, 1.);
pub(crate) const MUTED: Color = Color::new(0.70, 0.78, 0.76, 1.);
pub(crate) const GOLD: Color = Color::new(1.0, 0.76, 0.32, 1.);
pub(crate) const TEAL: Color = Color::new(0.38, 0.80, 0.74, 1.);
pub(crate) const SHADOW: Color = Color::new(0.02, 0.05, 0.06, 0.55);
/// Glass surfaces. Deliberately not the modal overlay's exact (0.07, 0.13)
/// red/green, which the release-Wasm browser contract uses to detect menus.
pub(crate) const GLASS_TOP: Color = Color::new(0.14, 0.22, 0.24, 0.9);
pub(crate) const GLASS: Color = Color::new(0.06, 0.12, 0.14, 0.9);

/// Which post look the frame wants; set only when it changes (sticky API).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scene {
    /// Title / course picker before a run: a calm, filmic attract shot.
    Front,
    /// Rolling: only emissive crystals, sparks and the open arch bloom.
    Play,
    /// Holding the overview: clearer and flatter for reading the route.
    Overview,
    /// Paused, picker mid-run, or results: the world recedes behind menus.
    Menu,
    /// The finish celebration: a touch more glow for the confetti.
    Celebrate,
}

/// Bright pale sky reaches ~0.82; thresholds sit above it so only additive
/// emissive light (crystal glows, sparks, the gold arch) blooms.
pub(crate) fn post_fx(scene: Scene) -> PostFx {
    let base = PostFx::subtle()
        .bloom(1.25, 0.95, 1.2)
        .bloom_knee(0.18)
        .contrast(1.06)
        .saturation(1.1)
        .lift_gamma_gain([0.0, 0.006, 0.012], [1.0, 1.0, 1.0], [1.04, 1.0, 0.95]);
    match scene {
        Scene::Play => base.vignette(0.26, 0.62),
        Scene::Overview => base.bloom(0.9, 1.0, 1.1).vignette(0.16, 0.7),
        Scene::Front => base
            .bloom(1.4, 0.92, 1.35)
            .vignette(0.34, 0.58)
            .exposure(0.97),
        Scene::Menu => base
            .bloom(0.9, 1.0, 1.2)
            .vignette(0.42, 0.55)
            .saturation(0.88)
            .exposure(0.9),
        Scene::Celebrate => base.bloom(1.8, 0.9, 1.4).vignette(0.3, 0.6),
    }
}

/// The kit theme recoloured to Marble Run's teal glass and gold accents.
pub(crate) fn theme() -> KitTheme {
    KitTheme {
        surface: Color::new(0.07, 0.13, 0.15, 0.96),
        raised: Color::new(0.15, 0.25, 0.27, 1.),
        header: Color::new(0.17, 0.29, 0.31, 1.),
        ink: INK,
        muted: MUTED,
        accent: GOLD,
        accent_ink: Color::new(0.16, 0.10, 0.03, 1.),
        focus: Color::new(0.62, 0.95, 0.90, 1.),
        line: Color::new(1., 1., 1., 0.1),
        ..KitTheme::default()
    }
}

pub(crate) fn motion() -> Motion {
    Motion::from(ferravex_render::preferences::feedback_policy())
}

pub(crate) fn fade(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

/// Modern copy with a soft one-pixel shadow.
pub(crate) fn body(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color).with_shadow(fade(SHADOW, color.a))
}

/// Classic 5×7 for the retro punch: clocks, countdown, arcade callouts.
pub(crate) fn punch(size: f32, color: Color) -> TextStyle {
    TextStyle::classic(size, color).with_shadow(fade(SHADOW, color.a))
}

pub(crate) fn centered(style: TextStyle) -> TextStyle {
    style.with_align(TextAlign::Center)
}

/// Shrink `style` (in whole font pixels where possible) until `label` fits.
pub(crate) fn fit(label: &str, width: f32, style: TextStyle) -> TextStyle {
    let w = text_width(label, style);
    if w <= width || w <= 0. {
        return style;
    }
    let size = (style.size * width / w).max(8.);
    // Prefer crisp multiples of 4 when shrinking.
    let snapped = (size / 4.).floor() * 4.;
    style.with_size(
        if snapped >= 8. && text_width(label, style.with_size(snapped)) <= width {
            snapped
        } else {
            size
        },
    )
}

pub(crate) fn draw(label: &str, x: f32, baseline: f32, style: TextStyle) -> f32 {
    draw_text_styled(label, x, baseline, style).width
}

/// Glass panel: soft drop shadow, top-lit gradient and a hairline edge.
pub(crate) fn glass(rect: Rect, radius: f32, alpha: f32) {
    if alpha <= 0. {
        return;
    }
    shapes::drop_shadow(
        rect,
        radius,
        Shadow::soft(fade(Color::new(0., 0., 0., 0.32), alpha))
            .offset(vec2(0., 3.))
            .blur(12.),
    );
    shapes::fill_rounded_rect(
        rect,
        radius,
        Fill::vertical(rect, fade(GLASS_TOP, alpha), fade(GLASS, alpha)),
    );
    shapes::stroke_rounded_rect(rect, radius, 1., fade(Color::new(1., 1., 1., 0.1), alpha));
}

/// Width of a kit key prompt without drawing it (mirrors `kit::key_prompt`).
pub(crate) fn prompt_width(keys: &[&str], action: &str, theme: &KitTheme) -> f32 {
    let style = TextStyle::modern(theme.body, theme.ink);
    let h = theme.body * 1.5 + 2.;
    let caps: f32 = keys
        .iter()
        .map(|k| (text_width(k, style) + kit::SPACE[2] * 2.).max(h - 3.) + kit::SPACE[0])
        .sum();
    caps + kit::SPACE[0] + text_width(action, style)
}

/// A fixed-capacity, stack-allocated text line. Formatting into it never
/// touches the heap; overlong text is truncated at a character boundary.
#[derive(Clone, Copy)]
pub(crate) struct Line {
    buf: [u8; 160],
    len: usize,
}

impl Line {
    pub const fn new() -> Self {
        Self {
            buf: [0; 160],
            len: 0,
        }
    }
    pub fn as_str(&self) -> &str {
        // Only whole UTF-8 characters are ever copied in.
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
    pub fn of(args: fmt::Arguments) -> Self {
        let mut line = Self::new();
        let _ = line.write_fmt(args);
        line
    }
}

impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}

impl Write for Line {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            let n = c.len_utf8();
            if self.len + n > self.buf.len() {
                return Err(fmt::Error);
            }
            c.encode_utf8(&mut self.buf[self.len..self.len + n]);
            self.len += n;
        }
        Ok(())
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

impl fmt::Debug for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// `format!` into a stack [`Line`].
macro_rules! line {
    ($($arg:tt)*) => {
        $crate::look::Line::of(format_args!($($arg)*))
    };
}
pub(crate) use line;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_format_without_growing_and_truncate_on_char_boundaries() {
        let l = line!("Gold pace · {}s left", 27);
        assert_eq!(l, "Gold pace · 27s left");
        let long = line!("{}", "♦".repeat(100));
        assert!(long.as_str().chars().all(|c| c == '♦'));
        assert!(long.as_str().len() <= 160);
    }

    #[test]
    fn bright_scenes_keep_bloom_above_the_pale_sky() {
        for scene in [
            Scene::Front,
            Scene::Play,
            Scene::Overview,
            Scene::Menu,
            Scene::Celebrate,
        ] {
            let fx = post_fx(scene);
            assert!(fx.bloom_threshold >= 0.9, "{scene:?} would haze the sky");
            assert!(fx.vignette <= 0.45);
        }
    }
}
