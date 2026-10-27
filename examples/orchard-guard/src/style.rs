//! Cozy art direction for menus and HUD: palette, type scale, eased button
//! motion and allocation-free formatting. Everything here is presentation:
//! it reads `Ui` state and accepted game state but never changes either.
//!
//! Buttons keep `Ui` input and focus (submitted with an invisible style) and
//! are painted here after the `Ui` frame finishes, so hover/focus visuals use
//! the current frame's state. The base fill of each button is one solid,
//! anti-aliased rounded rectangle exactly covering its layout rectangle;
//! browser tests locate controls by that fill.
use ferravex_play::tween::{Ease, Entrance, Motion, RollCounter};
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Fill, Shadow};
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_render::ui::{Ui, UiRect, kit};
use std::fmt::Write;

pub(super) const INK: Color = super::INK;
pub(super) const CREAM: Color = super::CREAM;
/// Primary control fill. Browser tests look for this exact color.
pub(super) const GREEN: Color = super::GREEN;
/// Focus ring. Browser tests look for this exact color.
pub(super) const FOCUS: Color = super::GOLD;
/// Disabled control fill. Browser tests look for this exact color.
pub(super) const DISABLED: Color = Color::from_hex(0x616956);
pub(super) const HONEY: Color = Color::new(0.96, 0.69, 0.27, 1.);
pub(super) const SAGE: Color = Color::new(0.42, 0.53, 0.36, 1.);
pub(super) const PAPER: Color = Color::new(1., 0.98, 0.93, 1.);
pub(super) const BAR_TOP: Color = Color::new(0.99, 0.96, 0.87, 1.);
pub(super) const BAR_BOTTOM: Color = Color::new(0.94, 0.89, 0.74, 1.);
pub(super) const SHADOW: Color = Color::new(0.12, 0.16, 0.08, 0.35);

pub(super) fn alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a.clamp(0., 1.))
}

/// Fixed-capacity text for per-frame labels: formatting never allocates.
/// Overlong text is truncated on a character boundary.
#[derive(Clone, Copy)]
pub(super) struct StackStr<const N: usize> {
    bytes: [u8; N],
    len: usize,
}

impl<const N: usize> StackStr<N> {
    pub fn new() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> Write for StackStr<N> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        for c in s.chars() {
            let mut buffer = [0; 4];
            let encoded = c.encode_utf8(&mut buffer).as_bytes();
            if self.len + encoded.len() > N {
                return Ok(());
            }
            self.bytes[self.len..self.len + encoded.len()].copy_from_slice(encoded);
            self.len += encoded.len();
        }
        Ok(())
    }
}

/// `format!` into a stack buffer.
macro_rules! sfmt {
    ($($arg:tt)*) => {{
        let mut s = $crate::style::StackStr::<96>::new();
        let _ = std::fmt::Write::write_fmt(&mut s, format_args!($($arg)*));
        s
    }};
}
pub(super) use sfmt;

pub(super) fn body(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
pub(super) fn punch(size: f32, color: Color) -> TextStyle {
    TextStyle::classic(size, color)
}

/// Draw `s` and return its width.
pub(super) fn text(s: &str, x: f32, baseline: f32, style: TextStyle) -> f32 {
    draw_text_styled(s, x, baseline, style).width
}
pub(super) fn text_center(s: &str, x: f32, baseline: f32, style: TextStyle) {
    draw_text_styled(s, x, baseline, style.with_align(TextAlign::Center));
}
/// Centered text shrunk to fit `max` units.
pub(super) fn text_fit(s: &str, x: f32, baseline: f32, style: TextStyle, max: f32) {
    let w = text_width(s, style);
    let style = if w > max && w > 0. {
        style.with_size((style.size * max / w).max(6.))
    } else {
        style
    };
    text_center(s, x, baseline, style);
}

pub(super) fn grow(r: Rect, by: f32) -> Rect {
    Rect::new(r.x - by, r.y - by, r.w + 2. * by, r.h + 2. * by)
}

/// Paper card with a soft shadow and hairline edge.
pub(super) fn card(r: Rect, radius: f32, a: f32) {
    shapes::drop_shadow(
        r,
        radius,
        Shadow::soft(alpha(SHADOW, a))
            .offset(vec2(0., 5.))
            .blur(18.),
    );
    shapes::fill_rounded_rect(
        r,
        radius,
        Fill::vertical(
            r,
            alpha(PAPER, a),
            alpha(Color::new(0.97, 0.93, 0.82, 1.), a),
        ),
    );
    shapes::stroke_rounded_rect(r, radius, 1., alpha(Color::new(0.45, 0.38, 0.22, 0.25), a));
}

/// Dark rounded chip (notices, toasts, pop plates).
pub(super) fn dark_chip(r: Rect, a: f32) {
    shapes::drop_shadow(
        r,
        r.h * 0.5,
        Shadow::soft(alpha(SHADOW, a))
            .offset(vec2(0., 3.))
            .blur(10.),
    );
    shapes::fill_rounded_rect(
        r,
        r.h * 0.5,
        Fill::vertical(
            r,
            alpha(Color::new(0.24, 0.30, 0.22, 0.95), a),
            alpha(Color::new(0.17, 0.22, 0.16, 0.95), a),
        ),
    );
}

// A small key cap ("Space", "U"). Returns its width.
// Caption under a control: optional key cap, then a short line.
/// How a control is drawn.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Look {
    /// Solid [`GREEN`]: the default action.
    Primary,
    /// Warm honey call-to-action (send wave).
    Hero,
    /// Soft sage secondary action.
    Quiet,
    /// Gold when toggled on.
    Toggle(bool),
}

const BUTTON_IDS: usize = 18;

/// Eased per-control hover/press/focus amounts and menu timers.
pub(super) struct Anim {
    buttons: [kit::ButtonMotion; BUTTON_IDS],
    pending: [Option<Paint>; BUTTON_IDS],
    pub motion: Motion,
    pub dt: f32,
    /// Seconds the current overlay (pause/results) has been shown.
    pub overlay: f32,
    overlay_key: u8,
    /// Seconds the banner slot has shown the preparation card.
    pub prep: f32,
    /// Front-door logo visibility, 0..=1.
    pub front: f32,
    pub clock: f32,
    pub coins: RollCounter,
    pub hearts: RollCounter,
    pub stopped: RollCounter,
}

fn slot(id: u64) -> Option<usize> {
    match id {
        1..=10 => Some(id as usize - 1),
        200..=207 => Some(id as usize - 190),
        _ => None,
    }
}

impl Anim {
    pub fn new() -> Self {
        Self {
            buttons: [kit::ButtonMotion::default(); BUTTON_IDS],
            pending: [None; BUTTON_IDS],
            motion: Motion::Full,
            dt: 0.,
            overlay: 0.,
            overlay_key: 0,
            prep: 0.,
            front: 1.,
            clock: 0.,
            coins: RollCounter::new(0),
            hearts: RollCounter::new(0),
            stopped: RollCounter::new(0),
        }
    }

    /// Advance timers once per presentation frame. `overlay` identifies the
    /// visible overlay (0 none, 1 paused, 2 won, 3 lost); a change restarts
    /// its entrance.
    pub fn frame(&mut self, dt: f32, motion: Motion, overlay: u8, prep: bool, front: bool) {
        self.pending.fill(None);
        self.dt = if dt.is_finite() {
            dt.clamp(0., 0.1)
        } else {
            0.
        };
        self.motion = motion;
        self.clock += self.dt;
        if overlay != self.overlay_key {
            self.overlay_key = overlay;
            self.overlay = 0.;
        } else {
            self.overlay += self.dt;
        }
        self.prep = if prep { self.prep + self.dt } else { 0. };
        let target = if front { 1. } else { 0. };
        let step = self.dt * 2.5;
        self.front = if motion.is_reduced() {
            target
        } else if self.front < target {
            (self.front + step).min(target)
        } else {
            (self.front - step).max(target)
        };
        for counter in [&mut self.coins, &mut self.hearts, &mut self.stopped] {
            counter.advance(self.dt, motion);
        }
    }

    /// Staggered fade-up for overlay item `index`.
    pub fn enter(&self, index: usize) -> ferravex_play::tween::Appearance {
        Entrance::fade_up(14.)
            .with_stagger(0.05)
            .with_duration(0.32)
            .with_ease(Ease::CubicOut)
            .sample(self.overlay, index, self.motion)
    }

    /// Submit (`paint == false`) or paint (`paint == true`) one control.
    /// Painting reads the `Ui` state from the frame that just finished.
    #[allow(clippy::too_many_arguments)]
    pub fn button(
        &mut self,
        ui: &mut Ui,
        paint: bool,
        id: u64,
        rect: Rect,
        label: &str,
        enabled: bool,
        look: Look,
    ) -> Option<Rect> {
        if !paint {
            ui.hud_button(
                id,
                UiRect {
                    min: [rect.x, rect.y],
                    size: [rect.w, rect.h],
                },
                label,
                enabled,
                kit::INVISIBLE_BUTTON,
            )
            .unwrap();
            return None;
        }
        let state = kit::ui_button_state(ui, id, enabled);
        let motion = slot(id).map(|i| &mut self.buttons[i]);
        let (hover, press, focus) = match motion {
            Some(m) => {
                m.update(state, self.dt, self.motion);
                (m.hover(), m.press(), m.focus())
            }
            None => (0., 0., 0.),
        };
        paint_button(rect, enabled, look, hover, press, focus);
        Some(rect)
    }
}

/// Fill colour and ink for a control.
pub(super) fn colors(enabled: bool, look: Look) -> (Color, Color) {
    if !enabled {
        return (DISABLED, Color::new(0.88, 0.87, 0.78, 1.));
    }
    match look {
        Look::Primary => (GREEN, CREAM),
        Look::Hero => (HONEY, INK),
        Look::Quiet => (SAGE, CREAM),
        Look::Toggle(true) => (FOCUS, INK),
        Look::Toggle(false) => (GREEN, CREAM),
    }
}

/// Rounded control: soft shadow, exact solid base, top gloss, eased hover
/// lift, press shade and a [`FOCUS`] ring that eases in from outside.
pub(super) fn paint_button(
    rect: Rect,
    enabled: bool,
    look: Look,
    hover: f32,
    press: f32,
    focus: f32,
) {
    let radius = 10_f32.min(rect.h * 0.5);
    let (fill, _) = colors(enabled, look);
    if enabled {
        shapes::drop_shadow(
            rect,
            radius,
            Shadow::soft(Color::new(0.10, 0.14, 0.06, 0.30 * (1. - 0.6 * press)))
                .offset(vec2(0., 4. - 2. * press))
                .blur(10.),
        );
    }
    shapes::fill_rounded_rect(rect, radius, fill);
    if enabled {
        let gloss = Rect::new(rect.x + 4., rect.y + 3., rect.w - 8., rect.h * 0.38);
        shapes::fill_rounded_rect(
            gloss,
            radius * 0.7,
            Color::new(1., 1., 1., 0.10 + 0.06 * hover),
        );
        if press > 0.01 {
            shapes::fill_rounded_rect(rect, radius, Color::new(0., 0., 0., 0.16 * press));
        }
    }
    if focus > 0.01 {
        let ring = grow(rect, 3. + 4. * (1. - focus));
        shapes::stroke_rounded_rect(ring, radius + 3., 2.5, FOCUS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_strings_format_without_heap_and_truncate_on_char_boundaries() {
        let s = sfmt!("{} coins · {}", 1234, "ok");
        assert_eq!(s.as_str(), "1234 coins · ok");
        let mut short = StackStr::<5>::new();
        let _ = write!(short, "ab·cd");
        assert_eq!(
            short.as_str(),
            "ab·c",
            "fills capacity without splitting UTF-8"
        );
    }

    #[test]
    fn controls_keep_their_contract_colors() {
        assert_eq!(colors(true, Look::Primary).0, GREEN);
        assert_eq!(colors(false, Look::Hero).0, DISABLED);
        assert_eq!(colors(true, Look::Toggle(true)).0, FOCUS);
        assert_eq!(slot(1), Some(0));
        assert_eq!(slot(207), Some(17));
        assert_eq!(slot(100), None);
    }

    #[test]
    fn overlay_entrances_restart_and_snap_under_reduced_motion() {
        let mut anim = Anim::new();
        anim.frame(0.05, Motion::Full, 1, false, false);
        anim.frame(0.05, Motion::Full, 1, false, false);
        assert!(anim.enter(0).alpha < 1.);
        for _ in 0..30 {
            anim.frame(0.05, Motion::Full, 1, false, false);
        }
        assert_eq!(anim.enter(3).alpha, 1.);
        anim.frame(0.05, Motion::Full, 2, false, false);
        assert_eq!(anim.overlay, 0., "a new overlay restarts its entrance");
        anim.frame(0.05, Motion::Reduced, 2, false, true);
        assert_eq!(anim.front, 1., "reduced motion snaps the front door");
        assert_eq!(anim.enter(0).offset, [0., 0.]);
    }
}

thread_local! { static UI_ANIM: std::cell::RefCell<Anim> = std::cell::RefCell::new(Anim::new()); }

pub(super) fn frame(dt: f32, campaign: &super::Campaign) {
    let outcome = campaign.game.outcome;
    let overlay = if campaign.session.is_paused() {
        1
    } else {
        match outcome {
            super::Outcome::Won => 2,
            super::Outcome::Lost => 3,
            _ => 0,
        }
    };
    UI_ANIM.with(|anim| {
        anim.borrow_mut().frame(
            dt,
            Motion::from(ferravex_render::preferences::feedback_policy()),
            overlay,
            !campaign.game.active,
            !campaign.session.is_started(),
        )
    });
}

#[derive(Clone, Copy)]
struct Paint {
    id: u64,
    rect: Rect,
    label: StackStr<96>,
    enabled: bool,
    size: f32,
    look: Look,
}

pub(super) fn control(ui: &mut Ui, id: u64, rect: Rect, label: &str, enabled: bool, size: f32) {
    UI_ANIM.with(|anim| {
        let mut anim = anim.borrow_mut();
        let look = if id == 4 {
            Look::Hero
        } else if id == 10 {
            Look::Toggle(false)
        } else if id == 9 {
            Look::Quiet
        } else {
            Look::Primary
        };
        anim.button(ui, false, id, rect, label, enabled, look);
        if let Some(index) = slot(id) {
            anim.pending[index] = Some(Paint {
                id,
                rect,
                label: sfmt!("{label}"),
                enabled,
                size,
                look,
            });
        }
    });
}

/// Paint after Ui has resolved this frame's keyboard, pointer and touch input.
pub(super) fn paint(ui: &mut Ui) {
    UI_ANIM.with(|anim| {
        let mut anim = anim.borrow_mut();
        let scale = ui.viewport().scale();
        for command in anim.pending.into_iter().flatten() {
            let Paint {
                id,
                rect,
                label,
                enabled,
                size,
                look,
            } = command;
            let screen = kit::ui_rect(
                ui.viewport(),
                UiRect {
                    min: [rect.x, rect.y],
                    size: [rect.w, rect.h],
                },
            );
            // `button` only reads Ui when painting; the shape uses screen pixels.
            anim.button(ui, true, id, screen, label.as_str(), enabled, look);
            let (_, ink) = colors(enabled, look);
            let a = anim.enter(0);
            let style = body(size * scale, ink);
            text_fit(
                label.as_str(),
                screen.center().x,
                style.centered_baseline(screen.center().y + a.offset[1] * scale * 0.15),
                style,
                screen.w - 12. * scale,
            );
        }
    });
}
