//! Screen-space art for the yard's HUD: pictograms, key caps, meters, glassy
//! panels and the tool palette's icon-first buttons. Shared ideas (trophy,
//! clock, rocket…) come from [`ferravex_render::icons`]; the yard's tools use
//! its own stroke glyphs so the palette reads as one family.
//!
//! Everything here is presentation and allocation-free.
use super::toys::Goal;
use ferravex_play::tween::{Motion, Spring};
use ferravex_render::icons::{self as pict, IconStyle};
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Cap, Fill, Join, Shadow, Stroke};
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_render::{post, ui};

pub(super) const INK: Color = ui::INK;
pub(super) const RED: Color = Color::new(0.98, 0.36, 0.36, 1.);
const RAISED: Color = Color::new(0.15, 0.20, 0.27, 0.97);
const SURFACE: Color = Color::new(0.07, 0.11, 0.17, 0.97);
const LINE: Color = Color::new(1., 1., 1., 0.08);

/// A fixed-capacity UTF-8 line for per-frame HUD text without allocation.
/// Overlong text is truncated at a character boundary.
pub(super) struct Line<const N: usize = 64> {
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
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> std::fmt::Write for Line<N> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let mut take = s.len().min(N - self.len);
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
        let mut line = $crate::look::Line::<64>::new();
        let _ = std::fmt::Write::write_fmt(&mut line, format_args!($($arg)*));
        line
    }};
}
pub(super) use fmt_line;

pub(super) fn faded(color: Color, alpha: f32) -> Color {
    Color {
        a: color.a * alpha.clamp(0., 1.),
        ..color
    }
}

pub(super) fn modern(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color).with_shadow(Color::new(0., 0., 0., 0.55 * color.a))
}

/// A glassy rounded panel with a soft shadow and a top-lit gradient.
pub(super) fn panel(rect: Rect, radius: f32, alpha: f32) {
    if alpha <= 0.01 {
        return;
    }
    shapes::drop_shadow(
        rect,
        radius,
        Shadow::soft(faded(Color::new(0., 0., 0., 0.4), alpha))
            .offset(vec2(0., 4.))
            .blur(14.),
    );
    shapes::fill_rounded_rect(
        rect,
        radius,
        Fill::vertical(rect, faded(RAISED, alpha), faded(SURFACE, alpha)),
    );
    shapes::stroke_rounded_rect(rect, radius, 1., faded(LINE, alpha));
}

/// The yard's own pictograms, drawn from primitives in a `size` box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Icon {
    Crate,
    Ball,
    Drop,
    Pulse,
    Pause,
    Play,
    Step,
    /// Orbit arrow around a dot: restore the camera.
    View,
    Reset,
    /// A heavy ball with finger holes.
    Bowl,
    Chaos,
    /// A ball beside a crate: the body count.
    Shapes,
    /// Two touching discs: contact pairs.
    Contact,
}

pub(super) fn icon(icon: Icon, center: Vec2, size: f32, color: Color) {
    let s = size * 0.5;
    let stroke = Stroke::new((size * 0.11).max(1.5), color)
        .cap(Cap::Round)
        .join(Join::Round);
    match icon {
        Icon::Crate => {
            let r = Rect::new(center.x - s * 0.75, center.y - s * 0.75, s * 1.5, s * 1.5);
            shapes::stroke_rounded_rect(r, s * 0.15, stroke.width, color);
            shapes::polyline(&[vec2(r.x, r.y + r.h), vec2(r.x + r.w, r.y)], stroke);
        }
        Icon::Ball => shapes::fill_circle(center, s * 0.8, color),
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
        Icon::Play => shapes::polyline(
            &[
                center + vec2(-s * 0.35, -s * 0.65),
                center + vec2(s * 0.6, 0.),
                center + vec2(-s * 0.35, s * 0.65),
            ],
            stroke.closed(true),
        ),
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
        Icon::View => {
            shapes::fill_circle(center, s * 0.22, color);
            let mut orbit = [Vec2::ZERO; 18];
            for (i, point) in orbit.iter_mut().enumerate() {
                let a = 0.5 + i as f32 / 17. * 5.1;
                *point = center + vec2(a.cos() * s * 0.85, a.sin() * s * 0.45);
            }
            shapes::polyline(&orbit, stroke);
            let tip = orbit[17];
            shapes::polyline(
                &[
                    tip + vec2(-s * 0.32, -s * 0.2),
                    tip,
                    tip + vec2(-s * 0.08, s * 0.32),
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
        Icon::Bowl => {
            shapes::fill_circle(center, s * 0.82, color);
            let hole = shapes::mix(color, Color::new(0., 0., 0., color.a), 0.65);
            // Two finger holes and a thumb hole, clustered off-center so the
            // ball never reads as a face.
            for (dx, dy) in [(0.02, -0.46), (0.34, -0.3), (0.36, 0.1)] {
                shapes::fill_circle(center + vec2(dx * s, dy * s), s * 0.12, hole);
            }
        }
        Icon::Chaos => {
            for k in 0..4 {
                let a = k as f32 * std::f32::consts::FRAC_PI_4;
                let d = vec2(a.cos(), a.sin()) * s * 0.8;
                shapes::polyline(&[center - d, center + d], stroke);
            }
        }
        Icon::Shapes => {
            shapes::fill_circle(center + vec2(-s * 0.4, s * 0.18), s * 0.42, color);
            let r = Rect::new(center.x + 0.02 * s, center.y - s * 0.66, s * 0.78, s * 0.78);
            shapes::stroke_rounded_rect(r, s * 0.12, stroke.width * 0.85, color);
        }
        Icon::Contact => {
            for dx in [-0.42, 0.42] {
                shapes::stroke_circle(
                    center + vec2(dx * s, s * 0.14),
                    s * 0.4,
                    stroke.width * 0.8,
                    color,
                );
            }
            shapes::fill_circle(center + vec2(0., -s * 0.6), s * 0.14, color);
        }
    }
}

/// A pictogram: one of the yard's own [`Icon`]s or a shared engine icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Glyph {
    Tool(Icon),
    Pict(pict::Icon),
}

impl From<Icon> for Glyph {
    fn from(icon: Icon) -> Self {
        Self::Tool(icon)
    }
}

impl From<pict::Icon> for Glyph {
    fn from(icon: pict::Icon) -> Self {
        Self::Pict(icon)
    }
}

/// Draws any glyph centered in a `size` box.
pub(super) fn glyph(glyph: impl Into<Glyph>, center: Vec2, size: f32, color: Color) {
    if color.a <= 0.01 {
        return;
    }
    match glyph.into() {
        Glyph::Tool(tool) => icon(tool, center, size, color),
        Glyph::Pict(engine) => pict::draw_styled(
            engine,
            center,
            size,
            IconStyle::new(color).shadow(Color::new(0., 0., 0., 0.35 * color.a)),
        ),
    }
}

/// The pictogram for a goal, shared by the goal bar, toasts and the card.
pub(super) fn goal_glyph(goal: Goal) -> Glyph {
    match goal {
        Goal::Strike => Glyph::Tool(Icon::Bowl),
        Goal::Airtime => Glyph::Pict(pict::Icon::Rocket),
        Goal::Avalanche => Glyph::Pict(pict::Icon::Bomb),
        Goal::CleanSweep => Glyph::Pict(pict::Icon::Crate),
    }
}

/// Whether a binding label is short enough for a key cap. Long chords and
/// `Unbound` stay in Controls instead of crowding the playfield.
pub(super) fn shows_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 6
}

fn cap_style(alpha: f32) -> TextStyle {
    TextStyle::modern(8., faded(INK, alpha))
}

/// Width of [`keycap`] for `key`, or zero when it would not be drawn.
pub(super) fn keycap_width(key: &str) -> f32 {
    if shows_key(key) {
        (text_width(key, cap_style(1.)) + 9.).max(15.)
    } else {
        0.
    }
}

/// A small raised key cap (`B`, `Space`) with its left edge at `x`, centered
/// on `cy`. Returns its width (zero when the binding is too long to show).
pub(super) fn keycap(x: f32, cy: f32, key: &str, alpha: f32) -> f32 {
    let w = keycap_width(key);
    if w == 0. || alpha <= 0.01 {
        return w;
    }
    let cap = Rect::new(x, cy - 8.5, w, 15.);
    let lip = Rect::new(cap.x, cap.y + 2., w, 15.);
    shapes::fill_rounded_rect(lip, 4., Color::new(0., 0., 0., 0.55 * alpha));
    shapes::fill_rounded_rect(
        cap,
        4.,
        Fill::vertical(
            cap,
            faded(Color::new(0.26, 0.31, 0.40, 1.), alpha),
            faded(Color::new(0.16, 0.20, 0.27, 1.), alpha),
        ),
    );
    shapes::stroke_rounded_rect(cap, 4., 1., Color::new(1., 1., 1., 0.16 * alpha));
    let style = cap_style(alpha).with_align(TextAlign::Center);
    draw_text_styled(
        key,
        cap.center().x,
        style.centered_baseline(cap.center().y),
        style,
    );
    w
}

/// Width of [`icon_value`].
pub(super) fn icon_value_width(value: &str, size: f32, style: TextStyle) -> f32 {
    if value.is_empty() {
        size
    } else {
        size + 5. + text_width(value, style)
    }
}

/// The standard stat: an icon then its value, both centered on `cy`, starting
/// at `x`. The icon takes the text color. Returns the total width.
pub(super) fn icon_value(
    mark: impl Into<Glyph>,
    value: &str,
    x: f32,
    cy: f32,
    size: f32,
    style: TextStyle,
) -> f32 {
    glyph(mark, vec2(x + size * 0.5, cy), size, style.color);
    if value.is_empty() {
        return size;
    }
    let width = draw_text_styled(value, x + size + 5., style.centered_baseline(cy), style).width;
    size + 5. + width
}

/// Goal pips centered on `cy`: finished goals are filled, `current` glows and
/// the rest are hollow. Returns the row width.
pub(super) fn pips(x: f32, cy: f32, done: &[bool], current: Option<usize>, color: Color) -> f32 {
    let gap = 11.;
    for (i, &finished) in done.iter().enumerate() {
        let at = vec2(x + 4. + i as f32 * gap, cy);
        if finished {
            shapes::fill_circle(at, 3.6, color);
        } else if current == Some(i) {
            shapes::glow_disc(at, 9., faded(color, 0.35));
            shapes::stroke_circle(at, 3.4, 1.6, color);
        } else {
            shapes::stroke_circle(at, 3., 1.2, faded(ui::MUTED, 0.55));
        }
    }
    8. + done.len().saturating_sub(1) as f32 * gap
}

/// Thin rounded progress track with a filled capsule.
pub(super) fn progress(rect: Rect, value: f32, color: Color) {
    let value = if value.is_finite() {
        value.clamp(0., 1.)
    } else {
        0.
    };
    let radius = rect.h * 0.5;
    shapes::fill_rounded_rect(rect, radius, Color::new(1., 1., 1., 0.1));
    if value > 0. {
        let fill = Rect::new(rect.x, rect.y, (rect.w * value).max(rect.h), rect.h);
        shapes::fill_rounded_rect(
            fill,
            radius,
            Fill::horizontal(fill, faded(color, 0.55 * color.a), color),
        );
    }
}

/// Circular progress: a faint track and an arc clockwise from 12 o'clock.
pub(super) fn ring_meter(center: Vec2, radius: f32, width: f32, fraction: f32, color: Color) {
    let fraction = if fraction.is_finite() {
        fraction.clamp(0., 1.)
    } else {
        0.
    };
    shapes::stroke_circle(center, radius, width, Color::new(0., 0., 0., 0.4 * color.a));
    if fraction > 0. {
        shapes::stroke_arc(
            center,
            radius,
            -std::f32::consts::FRAC_PI_2,
            fraction * std::f32::consts::TAU,
            Stroke::new(width, color).cap(Cap::Round),
        );
    }
}

/// A speech-bubble label above `anchor` (its bottom center), clamped to the
/// screen. Used for tool names that do not fit inside their button.
pub(super) fn bubble(text: &str, anchor: Vec2) {
    let style = modern(16., INK);
    let w = text_width(text, style) + 20.;
    let x = (anchor.x - w * 0.5).clamp(6., (screen_width() - w - 6.).max(6.));
    let rect = Rect::new(x, anchor.y - 30., w, 26.);
    panel(rect, 9., 1.);
    let tip = anchor.x.clamp(rect.x + 10., rect.x + rect.w - 10.);
    shapes::fill_triangle(
        vec2(tip - 6., rect.y + rect.h - 1.),
        vec2(tip + 6., rect.y + rect.h - 1.),
        vec2(tip, rect.y + rect.h + 5.),
        SURFACE,
    );
    draw_text_styled(
        text,
        rect.center().x,
        style.centered_baseline(rect.center().y),
        style.with_align(TextAlign::Center),
    );
}

/// Eased hover/press/selection for one palette button, plus a refusal wobble.
#[derive(Clone, Copy, Debug)]
pub(super) struct ToolMotion {
    hover: Spring,
    press: Spring,
    select: Spring,
    /// Seconds left on a refusal shake and red flash.
    deny: f32,
}

const DENY: f32 = 0.45;

impl Default for ToolMotion {
    fn default() -> Self {
        Self {
            hover: Spring::critical(0., 9.),
            press: Spring::critical(0., 18.),
            select: Spring::underdamped(0., 7., 0.55),
            deny: 0.,
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
        self.deny = (self.deny - dt).max(0.);
    }

    /// A click "boing": the press spring snaps back into place.
    pub fn kick(&mut self) {
        self.press.snap(1.);
    }

    /// A refused press: the button shakes and blushes red briefly.
    pub fn deny(&mut self) {
        self.deny = DENY;
    }

    /// Refusal envelope, 1 when just refused and 0 when calm.
    pub fn denied(&self) -> f32 {
        (self.deny / DENY).clamp(0., 1.)
    }
}

/// What one palette button shows: an icon first, its key cap at the right,
/// and its name only while hovered or selected.
#[derive(Clone, Copy, Debug)]
pub(super) struct ToolFace<'a> {
    pub glyph: Icon,
    pub label: &'a str,
    pub key: &'a str,
    pub accent: Color,
    /// Recharge progress (0..1) drawn as a ring around the icon.
    pub charge: Option<f32>,
    /// 0..1 "try me" halo for the tool the current goal needs.
    pub beckon: f32,
    /// Horizontal refusal shake in pixels (zero under reduced motion).
    pub shake: f32,
}

/// Paints one tool button. Selected tools glow and show their name; disabled
/// tools go flat and quiet; refused tools blush red. Returns true when the
/// hovered name does not fit inside and needs a [`bubble`] above.
pub(super) fn tool_button(
    rect: Rect,
    face: &ToolFace,
    state: ToolState,
    motion: &ToolMotion,
) -> bool {
    let hover = motion.hover.value.clamp(0., 1.);
    let press = motion.press.value.clamp(0., 1.);
    let select = motion.select.value.clamp(0., 1.3);
    let deny = motion.denied();
    let lift = 2. * hover - 2. * press;
    let r = Rect::new(rect.x + face.shake, rect.y - lift, rect.w, rect.h);
    let radius = 9_f32.min(r.h * 0.5);
    if face.beckon > 0.01 && state.enabled {
        let halo = Rect::new(r.x - 3., r.y - 3., r.w + 6., r.h + 6.);
        post::with_glow(|| {
            shapes::stroke_rounded_rect(
                halo,
                radius + 3.,
                2.,
                faded(face.accent, 0.6 * face.beckon),
            );
        });
    }
    if !state.enabled {
        shapes::fill_rounded_rect(r, radius, Color::new(0.10, 0.14, 0.19, 0.85));
        shapes::stroke_rounded_rect(r, radius, 1., LINE);
    } else {
        shapes::drop_shadow(
            r,
            radius,
            Shadow::soft(Color::new(0., 0., 0., 0.4 * (1. - press * 0.6)))
                .offset(vec2(0., 3. + lift))
                .blur(10.),
        );
        let base = shapes::mix(RAISED, face.accent, 0.85 * select.min(1.));
        let top = shapes::mix(base, Color::new(1., 1., 1., base.a), 0.1 + 0.08 * hover);
        let bottom = shapes::mix(base, Color::new(0., 0., 0., base.a), 0.12 + 0.15 * press);
        shapes::fill_rounded_rect(r, radius, Fill::vertical(r, top, bottom));
        shapes::stroke_rounded_rect(r, radius, 1., Color::new(1., 1., 1., 0.08 + 0.16 * hover));
        if select > 0.05 {
            post::with_glow(|| {
                shapes::stroke_rounded_rect(
                    r,
                    radius,
                    2.,
                    faded(face.accent, 0.55 * select.min(1.)),
                );
            });
        }
    }
    if deny > 0. {
        shapes::fill_rounded_rect(r, radius, faded(RED, 0.3 * deny));
        shapes::stroke_rounded_rect(r, radius, 1.5, faded(RED, 0.9 * deny));
    }
    let ink = if !state.enabled {
        Color::new(0.45, 0.5, 0.58, 1.)
    } else if select > 0.5 {
        ui::BACKGROUND
    } else {
        INK
    };
    let key_w = keycap_width(face.key);
    let area_x = r.x + 7.;
    let area_w = (r.w - 14. - if key_w > 0. { key_w + 5. } else { 0. }).max(1.);
    let icon_size = (r.h * 0.5).min(18.);
    let style = TextStyle::modern(16., ink);
    let label_w = text_width(face.label, style);
    let fits = icon_size + 6. + label_w <= area_w;
    // Names appear for hovered or selected tools; disabled tools stay icons.
    let shown = if state.enabled {
        hover.max(select.min(1.))
    } else {
        0.
    };
    let reveal = if fits { shown } else { 0. };
    let group = icon_size + reveal * (6. + label_w);
    let x0 = area_x + (area_w - group) * 0.5;
    let cy = r.center().y;
    let at = vec2(x0 + icon_size * 0.5, cy);
    icon(face.glyph, at, icon_size, ink);
    if let Some(charge) = face.charge {
        ring_meter(at, icon_size * 0.78, 2., charge, faded(face.accent, 0.9));
    }
    if reveal > 0.02 {
        draw_text_styled(
            face.label,
            x0 + icon_size + 6.,
            style.centered_baseline(cy),
            style.with_color(faded(ink, reveal * reveal)),
        );
    }
    if key_w > 0. {
        keycap(
            r.x + r.w - 7. - key_w,
            cy,
            face.key,
            if state.enabled { 0.95 } else { 0.45 },
        );
    }
    !fits && hover > 0.3
}
