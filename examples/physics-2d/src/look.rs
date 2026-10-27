//! Art direction for the lab: palette, post-processing per state, styled text
//! helpers, glassy panels and the tool palette's icon buttons.
//!
//! Everything here is presentation. Text formatting goes through a fixed
//! stack buffer ([`Line`]) so steady-state frames do not allocate.
use ferravex_play::tween::{Motion, Spring};
use ferravex_render::post::{self, PostFx, Tonemap};
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Cap, Fill, Join, Shadow, Stroke};
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled, text_width};

pub(super) const BG_TOP: Color = Color::new(0.035, 0.055, 0.10, 1.);
pub(super) const BG_BOTTOM: Color = Color::new(0.075, 0.11, 0.17, 1.);
pub(super) const INK: Color = Color::new(0.95, 0.93, 0.89, 1.);
pub(super) const MUTED: Color = Color::new(0.60, 0.66, 0.74, 1.);
pub(super) const SURFACE: Color = Color::new(0.09, 0.12, 0.18, 0.92);
pub(super) const RAISED: Color = Color::new(0.15, 0.19, 0.27, 0.96);
pub(super) const LINE: Color = Color::new(1., 1., 1., 0.08);
pub(super) const SHADOW: Color = Color::new(0., 0., 0., 0.55);
pub(super) const AMBER: Color = Color::new(0.97, 0.71, 0.33, 1.);
pub(super) const CYAN: Color = Color::new(0.36, 0.86, 0.90, 1.);
pub(super) const GOLD: Color = Color::new(1.0, 0.82, 0.40, 1.);
pub(super) const PINK: Color = Color::new(0.98, 0.47, 0.58, 1.);
pub(super) const MINT: Color = Color::new(0.52, 0.90, 0.66, 1.);
pub(super) const TEXT_SHADOW: Color = Color::new(0., 0., 0., 0.6);

/// Which grade the frame uses. Chosen per scene/state; applied only on change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Grade {
    Workshop,
    Dominoes,
    Paused,
}

impl Grade {
    /// Bloom only above 0.9 so authored body colors stay flat and only
    /// additive emissive accents (sparks, glows, pulse rings) bloom.
    pub fn fx(self) -> PostFx {
        let base = PostFx::subtle()
            .bloom(1.8, 0.9, 1.25)
            .bloom_knee(0.2)
            .tonemap(Tonemap::Soft)
            .contrast(1.06)
            .vignette(0.32, 0.62);
        match self {
            Self::Workshop => base.saturation(1.08),
            Self::Dominoes => base.saturation(1.04).tint(Color::new(1.05, 1.0, 0.93, 1.)),
            Self::Paused => base
                .bloom(1.1, 0.9, 1.1)
                .saturation(0.62)
                .tint(Color::new(0.92, 0.98, 1.08, 1.))
                .vignette(0.45, 0.55),
        }
    }
}

/// Applies a grade only when it changes (post settings are sticky).
#[derive(Default)]
pub(super) struct Grading {
    current: Option<Grade>,
}

impl Grading {
    pub fn apply(&mut self, grade: Grade) -> bool {
        if self.current == Some(grade) {
            return false;
        }
        self.current = Some(grade);
        post::set_post_fx(grade.fx());
        true
    }
}

/// A fixed-capacity UTF-8 line for per-frame HUD text without allocation.
/// Overlong text is truncated at a character boundary.
pub(super) struct Line<const N: usize = 160> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> Line<N> {
    pub fn new() -> Self {
        Self {
            buf: [0; N],
            len: 0,
        }
    }

    pub fn as_str(&self) -> &str {
        // Only whole UTF-8 sequences are ever copied in.
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> std::fmt::Write for Line<N> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let room = N - self.len;
        let mut take = s.len().min(room);
        while !s.is_char_boundary(take) {
            take -= 1;
        }
        self.buf[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        Ok(())
    }
}

/// `format!` into a stack [`Line`].
macro_rules! fmt_line {
    ($($arg:tt)*) => {{
        let mut line = $crate::look::Line::<160>::new();
        let _ = std::fmt::Write::write_fmt(&mut line, format_args!($($arg)*));
        line
    }};
}
pub(super) use fmt_line;

pub(super) fn modern(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color).with_shadow(TEXT_SHADOW)
}

pub(super) fn classic(size: f32, color: Color) -> TextStyle {
    TextStyle::classic(size, color).with_shadow(TEXT_SHADOW)
}

pub(super) fn faded(color: Color, alpha: f32) -> Color {
    Color {
        a: color.a * alpha.clamp(0., 1.),
        ..color
    }
}

/// The largest crisp-ish size (steps of 4, down to 8) that fits `max_width`;
/// below 8 it scales down rather than overflow.
pub(super) fn fit(text: &str, style: TextStyle, max_width: f32) -> TextStyle {
    let mut size = style.size;
    loop {
        let candidate = style.with_size(size);
        let width = text_width(text, candidate);
        if width <= max_width || size <= 8. {
            if width > max_width && width > 0. {
                return candidate.with_size(size * (max_width / width).max(0.4));
            }
            return candidate;
        }
        size -= 4.;
    }
}

/// Draws `text` fitted to `max_width`; returns the drawn width.
pub(super) fn text(text: &str, x: f32, y: f32, max_width: f32, style: TextStyle) -> f32 {
    let style = fit(text, style, max_width.max(1.));
    draw_text_styled(text, x, y, style).width
}

pub(super) fn text_center(text: &str, cx: f32, y: f32, max_width: f32, style: TextStyle) {
    let style = fit(text, style, max_width.max(1.)).with_align(TextAlign::Center);
    draw_text_styled(text, cx, y, style);
}

pub(super) fn text_right(text: &str, x: f32, y: f32, max_width: f32, style: TextStyle) {
    let style = fit(text, style, max_width.max(1.)).with_align(TextAlign::Right);
    draw_text_styled(text, x, y, style);
}

/// A glassy rounded panel with a soft shadow and a top-lit gradient.
pub(super) fn panel(rect: Rect, radius: f32, alpha: f32) {
    if alpha <= 0.01 {
        return;
    }
    shapes::drop_shadow(
        rect,
        radius,
        Shadow::soft(faded(SHADOW, alpha))
            .offset(vec2(0., 6.))
            .blur(18.),
    );
    shapes::fill_rounded_rect(
        rect,
        radius,
        Fill::vertical(rect, faded(RAISED, alpha), faded(SURFACE, alpha)),
    );
    shapes::stroke_rounded_rect(rect, radius, 1., faded(LINE, alpha));
}

/// Thin rounded progress track with a glowing head.
pub(super) fn progress(rect: Rect, value: f32, color: Color, alpha: f32) {
    let value = if value.is_finite() {
        value.clamp(0., 1.)
    } else {
        0.
    };
    let radius = rect.h * 0.5;
    shapes::fill_rounded_rect(rect, radius, faded(Color::new(0., 0., 0., 0.45), alpha));
    if value > 0. {
        let fill = Rect::new(rect.x, rect.y, (rect.w * value).max(rect.h), rect.h);
        shapes::fill_rounded_rect(
            fill,
            radius,
            Fill::horizontal(
                fill,
                faded(shapes::mix(color, BG_TOP, 0.35), alpha),
                faded(color, alpha),
            ),
        );
    }
}

/// Icons for the tool palette, drawn from primitives in a `size` box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Icon {
    Ball,
    Crate,
    Drop,
    Pulse,
    Chaos,
    Domino,
    Pause,
    Play,
    Step,
    Reset,
}

pub(super) fn icon(icon: Icon, center: Vec2, size: f32, color: Color) {
    let s = size * 0.5;
    let stroke = Stroke::new((size * 0.11).max(1.5), color)
        .cap(Cap::Round)
        .join(Join::Round);
    match icon {
        Icon::Ball => {
            shapes::fill_circle(center, s * 0.8, color);
        }
        Icon::Crate => {
            let r = Rect::new(center.x - s * 0.75, center.y - s * 0.75, s * 1.5, s * 1.5);
            shapes::stroke_rounded_rect(r, s * 0.15, stroke.width, color);
            shapes::polyline(&[vec2(r.x, r.y + r.h), vec2(r.x + r.w, r.y)], stroke);
        }
        Icon::Drop => {
            shapes::polyline(
                &[center + vec2(0., -s * 0.8), center + vec2(0., s * 0.6)],
                stroke,
            );
            shapes::polyline(
                &[
                    center + vec2(-s * 0.5, s * 0.15),
                    center + vec2(0., s * 0.7),
                    center + vec2(s * 0.5, s * 0.15),
                ],
                stroke,
            );
        }
        Icon::Pulse => {
            shapes::fill_circle(center, s * 0.22, color);
            shapes::stroke_circle(center, s * 0.52, stroke.width * 0.8, faded(color, 0.85));
            shapes::stroke_circle(center, s * 0.85, stroke.width * 0.7, faded(color, 0.5));
        }
        Icon::Chaos => {
            for k in 0..4 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let d = vec2(a.cos(), a.sin()) * s * 0.8;
                shapes::polyline(&[center - d, center + d], stroke);
            }
        }
        Icon::Domino => {
            for (i, lean) in [0_f32, 0.35, 0.7].into_iter().enumerate() {
                let base = center + vec2(-s * 0.6 + i as f32 * s * 0.55, s * 0.75);
                let top = base + vec2(lean.sin(), -lean.cos()) * s * 1.35;
                shapes::polyline(&[base, top], stroke);
            }
        }
        Icon::Pause => {
            for dx in [-0.3, 0.3] {
                shapes::polyline(
                    &[
                        center + vec2(dx * s, -s * 0.6),
                        center + vec2(dx * s, s * 0.6),
                    ],
                    stroke,
                );
            }
        }
        Icon::Play => {
            shapes::polyline(
                &[
                    center + vec2(-s * 0.35, -s * 0.65),
                    center + vec2(s * 0.6, 0.),
                    center + vec2(-s * 0.35, s * 0.65),
                ],
                stroke.closed(true),
            );
        }
        Icon::Step => {
            shapes::polyline(
                &[
                    center + vec2(-s * 0.55, -s * 0.6),
                    center + vec2(s * 0.2, 0.),
                    center + vec2(-s * 0.55, s * 0.6),
                ],
                stroke,
            );
            shapes::polyline(
                &[
                    center + vec2(s * 0.55, -s * 0.6),
                    center + vec2(s * 0.55, s * 0.6),
                ],
                stroke,
            );
        }
        Icon::Reset => {
            shapes::stroke_arc(
                center,
                s * 0.62,
                -2.2,
                4.6,
                Stroke::new(stroke.width, color).cap(Cap::Round),
            );
            let tip = center + vec2((-2.2_f32).cos(), (-2.2_f32).sin()) * s * 0.62;
            shapes::polyline(
                &[
                    tip + vec2(-s * 0.05, -s * 0.38),
                    tip,
                    tip + vec2(s * 0.38, -s * 0.02),
                ],
                stroke,
            );
        }
    }
}

/// Eased hover/press/selection for one palette button.
#[derive(Clone, Copy, Debug)]
pub(super) struct ToolMotion {
    hover: Spring,
    press: Spring,
    select: Spring,
}

impl Default for ToolMotion {
    fn default() -> Self {
        Self {
            hover: Spring::critical(0., 9.),
            press: Spring::critical(0., 18.),
            select: Spring::underdamped(0., 7., 0.55),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct ToolState {
    pub hovered: bool,
    pub pressed: bool,
    pub selected: bool,
    pub enabled: bool,
}

impl ToolMotion {
    pub fn update(&mut self, state: ToolState, dt: f32, motion: Motion) {
        let on = |b: bool| if b { 1. } else { 0. };
        self.hover.set_target(on(state.enabled && state.hovered));
        self.press.set_target(on(state.enabled && state.pressed));
        self.select.set_target(on(state.selected));
        for spring in [&mut self.hover, &mut self.press, &mut self.select] {
            spring.advance(dt, motion);
        }
    }

    /// A click "boing": the selection spring overshoots back into place.
    pub fn kick(&mut self) {
        self.press.snap(1.);
    }
}

/// Paints one tool: rounded key with an icon, label and key hint. Selected
/// tools glow; disabled tools go flat and quiet.
pub(super) fn tool_button(
    rect: Rect,
    glyph: Icon,
    label: &str,
    key: &str,
    state: ToolState,
    motion: &ToolMotion,
    accent: Color,
) {
    let hover = motion.hover.value.clamp(0., 1.);
    let press = motion.press.value.clamp(0., 1.);
    let select = motion.select.value.clamp(0., 1.3);
    let lift = 2. * hover - 2. * press;
    let r = Rect::new(rect.x, rect.y - lift, rect.w, rect.h);
    let radius = 10_f32.min(r.h * 0.5);
    if !state.enabled {
        shapes::fill_rounded_rect(r, radius, Color::new(0.11, 0.14, 0.2, 0.7));
        shapes::stroke_rounded_rect(r, radius, 1., LINE);
    } else {
        shapes::drop_shadow(
            r,
            radius,
            Shadow::soft(Color::new(0., 0., 0., 0.4 * (1. - press * 0.6)))
                .offset(vec2(0., 3. + lift))
                .blur(10.),
        );
        let base = shapes::mix(RAISED, accent, 0.85 * select.min(1.));
        let top = shapes::mix(base, Color::new(1., 1., 1., base.a), 0.1 + 0.08 * hover);
        let bottom = shapes::mix(base, Color::new(0., 0., 0., base.a), 0.12 + 0.15 * press);
        shapes::fill_rounded_rect(r, radius, Fill::vertical(r, top, bottom));
        shapes::stroke_rounded_rect(r, radius, 1., Color::new(1., 1., 1., 0.08 + 0.16 * hover));
        if select > 0.05 {
            // Emissive rim: additive, so it feeds the bloom.
            post::with_glow(|| {
                shapes::stroke_rounded_rect(r, radius, 2., faded(accent, 0.55 * select.min(1.)));
            });
        }
    }
    let ink = if !state.enabled {
        Color::new(0.45, 0.5, 0.58, 1.)
    } else if select > 0.5 {
        Color::new(0.12, 0.08, 0.04, 1.)
    } else {
        INK
    };
    let compact = r.w < 96.;
    let icon_size = (r.h * 0.42).min(18.);
    let style = TextStyle::modern(16., ink);
    let label_w = text_width(label, style);
    if compact || r.h < 34. {
        // Icon above, or icon beside a short label in tight spaces.
        let total = icon_size + 6. + label_w;
        if total <= r.w - 10. {
            let x0 = r.center().x - total * 0.5;
            icon(
                glyph,
                vec2(x0 + icon_size * 0.5, r.center().y),
                icon_size,
                ink,
            );
            draw_text_styled(
                label,
                x0 + icon_size + 6.,
                style.centered_baseline(r.center().y),
                style,
            );
        } else {
            icon(glyph, r.center(), icon_size * 1.1, ink);
        }
    } else {
        let total = icon_size + 8. + label_w;
        let x0 = r.center().x - total * 0.5;
        icon(
            glyph,
            vec2(x0 + icon_size * 0.5, r.center().y),
            icon_size,
            ink,
        );
        draw_text_styled(
            label,
            x0 + icon_size + 8.,
            style.centered_baseline(r.center().y),
            style,
        );
        // Key hint in the corner, quiet.
        let hint = TextStyle::modern(8., faded(ink, 0.6)).with_align(TextAlign::Right);
        if !key.is_empty() && key.len() <= 6 {
            draw_text_styled(key, r.x + r.w - 6., r.y + 11., hint);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_lines_format_without_allocating_and_truncate_on_char_boundaries() {
        let line = fmt_line!("{} / {} bodies · {}", 48, 240, "Ball ready");
        assert_eq!(line.as_str(), "48 / 240 bodies · Ball ready");
        let mut short = Line::<5>::new();
        let _ = std::fmt::Write::write_str(&mut short, "ab·cd");
        assert_eq!(short.as_str(), "ab·c");
        let mut boundary = Line::<3>::new();
        let _ = std::fmt::Write::write_str(&mut boundary, "ab·cd");
        assert_eq!(boundary.as_str(), "ab");
    }

    #[test]
    fn fitting_steps_down_and_never_overflows() {
        let style = modern(16., INK);
        let text = "Rest something 7.5 m up. Gravity disagrees.";
        let fitted = fit(text, style, 200.);
        assert!(text_width(text, fitted) <= 200.5);
        assert_eq!(fit("Hi", style, 200.).size, 16.);
    }

    #[test]
    fn grades_bloom_only_emissive_highlights() {
        for grade in [Grade::Workshop, Grade::Dominoes, Grade::Paused] {
            let fx = grade.fx().sanitized();
            assert!(fx.bloom_threshold >= 0.88, "{grade:?}");
            assert!(fx.vignette > 0.2 && fx.vignette < 0.5);
            assert!(fx.grain == 0. && fx.aberration == 0.);
        }
        assert!(Grade::Paused.fx().saturation < Grade::Workshop.fx().saturation);
        let mut grading = Grading::default();
        assert!(grading.apply(Grade::Workshop));
        assert!(
            !grading.apply(Grade::Workshop),
            "sticky settings apply once"
        );
        assert!(grading.apply(Grade::Paused));
    }
}
