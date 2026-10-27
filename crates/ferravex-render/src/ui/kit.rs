//! Modern UI kit: cards, buttons, meters, toasts, key caps and screen
//! transitions drawn with [`crate::shapes`] and the modern text face.
//!
//! The kit is **visual only**. Input, focus, hit testing and activation stay in
//! [`super::Ui`]: submit buttons there with [`INVISIBLE_BUTTON`] (or your own
//! style), then paint them here with [`ui_button_state`] and [`ui_rect`].
//! Everything draws in screen units (the default 2D camera), writes straight to
//! the frame's vertex stream and allocates nothing.
//!
//! Recommended style (see `docs/UI.md`, "Modern UI kit"):
//!
//! | token        | values                                          |
//! |--------------|-------------------------------------------------|
//! | spacing      | [`SPACE`] = 4, 8, 12, 16, 24, 32                |
//! | radii        | [`RADIUS_SMALL`] 6 (chips), [`RADIUS`] 10 (buttons), [`RADIUS_LARGE`] 16 (cards) |
//! | type (modern)| [`TYPE_CAPTION`] 8, [`TYPE_BODY`] 16, [`TYPE_TITLE`] 24, [`TYPE_DISPLAY`] 32 |
//!
//! Type sizes are multiples of 8 so every font pixel lands on whole screen
//! pixels. Pair the modern face for copy with the classic face for big
//! retro numerals or titles when a game wants more attitude.
//!
//! ```
//! use ferravex_render::{Rect, ui::kit::{self, ButtonKind, ButtonMotion, ButtonState, KitTheme}};
//! use ferravex_play::tween::Motion;
//! let theme = KitTheme::default();
//! let content = kit::card(Rect::new(40., 40., 320., 200.), Some("Settings"), &theme);
//! let mut motion = ButtonMotion::default();
//! let state = ButtonState { hovered: true, ..ButtonState::default() };
//! motion.update(state, 1. / 60., Motion::Full);
//! let button = Rect::new(content.x, content.y, 140., 36.);
//! kit::button(button, "Resume", ButtonKind::Primary, &motion, &theme);
//! kit::meter(Rect::new(content.x, content.y + 52., 200., 12.), 0.7, Some(0.85), theme.success, &theme);
//! ```
use super::{Ui, UiRect, Viewport, WidgetId};
use crate::shapes::{self, Fill, Shadow, mix};
use crate::text::{TextAlign, TextStyle, draw_text_styled, text_width};
use crate::{Color, Rect, Vec2, vec2};
use ferravex_play::tween::{Appearance, Motion, Spring, TransitionKind};
use ferravex_play::ui::theme::{ColorRole, UiTheme};

/// Spacing scale in screen units.
pub const SPACE: [f32; 6] = [4., 8., 12., 16., 24., 32.];
/// Corner radius for chips, key caps and small tags.
pub const RADIUS_SMALL: f32 = 6.;
/// Corner radius for buttons, fields and toasts.
pub const RADIUS: f32 = 10.;
/// Corner radius for cards and dialogs.
pub const RADIUS_LARGE: f32 = 16.;
/// Modern-face sizes (multiples of 8 stay pixel-crisp).
pub const TYPE_CAPTION: f32 = 8.;
pub const TYPE_BODY: f32 = 16.;
pub const TYPE_TITLE: f32 = 24.;
pub const TYPE_DISPLAY: f32 = 32.;

/// Colors and sizes for kit widgets. `alpha` multiplies everything, which is
/// how fades from [`Appearance`] are applied ([`KitTheme::faded`]).
///
/// ```
/// use ferravex_play::ui::theme::{ThemeMode, UiTheme};
/// use ferravex_render::ui::kit::KitTheme;
/// let theme = KitTheme::from_ui_theme(UiTheme::preset(ThemeMode::HighContrast));
/// assert_eq!(theme.faded(0.5).alpha, 0.5);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KitTheme {
    pub backdrop: Color,
    pub surface: Color,
    pub raised: Color,
    pub header: Color,
    pub ink: Color,
    pub muted: Color,
    pub disabled: Color,
    pub accent: Color,
    /// Text on an accent fill.
    pub accent_ink: Color,
    pub success: Color,
    pub warning: Color,
    pub danger: Color,
    pub focus: Color,
    pub line: Color,
    pub shadow: Color,
    pub radius: f32,
    /// Modern text size for labels and body copy.
    pub body: f32,
    /// Modern text size for titles.
    pub title: f32,
    pub alpha: f32,
}

const fn hex(rgb: u32) -> Color {
    Color::from_hex(rgb)
}

impl Default for KitTheme {
    fn default() -> Self {
        Self {
            backdrop: Color::new(0.02, 0.03, 0.06, 0.6),
            surface: hex(0x1d2436),
            raised: hex(0x2b3550),
            header: hex(0x343f5c),
            ink: hex(0xf2ede3),
            muted: hex(0x9aa7b8),
            disabled: hex(0x5d6678),
            accent: hex(0xf7b553),
            accent_ink: hex(0x24170a),
            success: hex(0x7fdc8f),
            warning: hex(0xffd166),
            danger: hex(0xff6b6b),
            focus: hex(0x8fd8ff),
            line: Color::new(1., 1., 1., 0.09),
            shadow: Color::new(0., 0., 0., 0.5),
            radius: RADIUS,
            body: TYPE_BODY,
            title: TYPE_TITLE,
            alpha: 1.,
        }
    }
}

impl KitTheme {
    /// Maps an accessibility-checked [`UiTheme`] palette onto kit roles.
    pub fn from_ui_theme(theme: UiTheme) -> Self {
        let c = |role| theme.color(role);
        let surface = c(ColorRole::Surface);
        let lift = |amount| mix(surface, Color::new(1., 1., 1., surface.a), amount);
        Self {
            backdrop: Color::new(0., 0., 0., 0.6),
            surface,
            raised: lift(0.08),
            header: lift(0.14),
            ink: c(ColorRole::Text),
            muted: c(ColorRole::MutedText),
            disabled: c(ColorRole::DisabledText),
            accent: c(ColorRole::Accent),
            accent_ink: c(ColorRole::Canvas),
            success: c(ColorRole::Success),
            warning: c(ColorRole::Warning),
            danger: c(ColorRole::Danger),
            focus: c(ColorRole::Focus),
            line: c(ColorRole::Border),
            ..Self::default()
        }
    }
    /// Copy with every color's alpha multiplied by `alpha`.
    pub fn faded(mut self, alpha: f32) -> Self {
        self.alpha = (self.alpha * alpha).clamp(0., 1.);
        self
    }
    fn c(&self, color: Color) -> Color {
        Color::new(color.r, color.g, color.b, color.a * self.alpha)
    }
    fn text(&self, size: f32, color: Color) -> TextStyle {
        TextStyle::modern(size, self.c(color))
    }
}

/// Screen rectangle of a [`Ui`] layout rectangle.
///
/// ```
/// use ferravex_render::ui::{UiRect, Viewport, kit::ui_rect};
/// let viewport = Viewport::fit([1280., 720.], [640., 360.], [0.; 4]).unwrap();
/// let r = ui_rect(viewport, UiRect { min: [10., 20.], size: [100., 30.] });
/// assert_eq!((r.x, r.y, r.w, r.h), (20., 40., 200., 60.));
/// ```
pub fn ui_rect(viewport: Viewport, rect: UiRect) -> Rect {
    let p = viewport.pixels();
    let s = viewport.scale();
    Rect::new(
        p.min[0] + rect.min[0] * s,
        p.min[1] + rect.min[1] * s,
        rect.size[0] * s,
        rect.size[1] * s,
    )
}

/// Moves and scales `rect` about its center per an [`Appearance`].
///
/// ```
/// use ferravex_play::tween::Appearance;
/// use ferravex_render::{Rect, ui::kit::appear};
/// let r = appear(Rect::new(0., 0., 100., 50.), Appearance { alpha: 1., offset: [0., 10.], scale: 0.5 });
/// assert_eq!((r.x, r.y, r.w, r.h), (25., 22.5, 50., 25.));
/// ```
pub fn appear(rect: Rect, look: Appearance) -> Rect {
    let c = rect.center() + vec2(look.offset[0], look.offset[1]);
    let size = vec2(rect.w, rect.h) * look.scale;
    Rect::new(c.x - size.x * 0.5, c.y - size.y * 0.5, size.x, size.y)
}

/// Raised surface: soft shadow, subtle top-lit gradient and a hairline edge.
///
/// ```
/// use ferravex_render::{Rect, ui::kit::{self, KitTheme}};
/// kit::panel(Rect::new(20., 20., 200., 120.), &KitTheme::default());
/// ```
pub fn panel(rect: Rect, theme: &KitTheme) {
    panel_with_radius(rect, RADIUS_LARGE.max(theme.radius), theme);
}

fn panel_with_radius(rect: Rect, radius: f32, theme: &KitTheme) {
    shapes::drop_shadow(
        rect,
        radius,
        Shadow::soft(theme.c(theme.shadow))
            .offset(vec2(0., 6.))
            .blur(24.),
    );
    let top = mix(theme.surface, theme.raised, 0.6);
    shapes::fill_rounded_rect(
        rect,
        radius,
        Fill::vertical(rect, theme.c(top), theme.c(theme.surface)),
    );
    shapes::stroke_rounded_rect(rect, radius, 1., theme.c(theme.line));
}

/// [`panel`] with an optional header band and title; returns the content area
/// inset by the spacing scale.
///
/// ```
/// use ferravex_render::{Rect, ui::kit::{self, KitTheme}};
/// let inner = kit::card(Rect::new(0., 0., 300., 200.), Some("Loadout"), &KitTheme::default());
/// assert!(inner.y > 40. && inner.x == 16.);
/// ```
pub fn card(rect: Rect, title: Option<&str>, theme: &KitTheme) -> Rect {
    let radius = RADIUS_LARGE.max(theme.radius);
    panel_with_radius(rect, radius, theme);
    let pad = SPACE[3];
    let Some(title) = title else {
        return Rect::new(
            rect.x + pad,
            rect.y + pad,
            rect.w - 2. * pad,
            rect.h - 2. * pad,
        );
    };
    let band = (theme.title * 0.5 + SPACE[4]).min(rect.h);
    let header = theme.c(theme.header);
    // Round only the top corners: a rounded band squared off below.
    let r = radius.min(band);
    shapes::fill_rounded_rect(Rect::new(rect.x, rect.y, rect.w, band), r, header);
    shapes::fill_rounded_rect(Rect::new(rect.x, rect.y + band - r, rect.w, r), 0., header);
    shapes::fill_rounded_rect(
        Rect::new(rect.x, rect.y + band, rect.w, 1.),
        0.,
        theme.c(theme.line),
    );
    let style = theme.text(theme.body, theme.ink);
    draw_text_styled(
        title,
        rect.x + pad,
        style.centered_baseline(rect.y + band * 0.5),
        style.with_shadow(theme.c(Color::new(0., 0., 0., 0.35))),
    );
    let top = rect.y + band + pad;
    Rect::new(
        rect.x + pad,
        top,
        rect.w - 2. * pad,
        (rect.y + rect.h - pad - top).max(0.),
    )
}

/// Pointer/focus state of a button for one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButtonState {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    pub enabled: bool,
}

impl Default for ButtonState {
    fn default() -> Self {
        Self {
            hovered: false,
            pressed: false,
            focused: false,
            enabled: true,
        }
    }
}

/// State of a [`Ui`] button `id` after [`Ui::finish`] (hover, pointer capture
/// and keyboard/gamepad focus), for painting it with the kit.
///
/// ```
/// use ferravex_render::ui::{Ui, kit::ui_button_state};
/// let ui = Ui::new();
/// assert!(!ui_button_state(&ui, 7, true).hovered);
/// ```
pub fn ui_button_state(ui: &Ui, id: WidgetId, enabled: bool) -> ButtonState {
    ButtonState {
        hovered: enabled && ui.hovered() == Some(id),
        pressed: enabled && ui.interaction().pointer_capture == Some(id),
        focused: enabled && ui.focused() == Some(id),
        enabled,
    }
}

/// A [`super::ButtonStyle`] whose own paint is fully transparent, for buttons
/// that keep [`Ui`] input/focus but are drawn by the kit.
pub const INVISIBLE_BUTTON: super::ButtonStyle = super::ButtonStyle {
    fill: Color::new(0., 0., 0., 0.),
    ink: Color::new(0., 0., 0., 0.),
    focus: Color::new(0., 0., 0., 0.),
    disabled: Color::new(0., 0., 0., 0.),
    size: 18.,
};

/// Eased hover/press/focus amounts for one button (critically damped springs,
/// so state changes glide without overshoot). Keep one per button.
///
/// ```
/// use ferravex_play::tween::Motion;
/// use ferravex_render::ui::kit::{ButtonMotion, ButtonState};
/// let mut m = ButtonMotion::default();
/// let hover = ButtonState { hovered: true, ..ButtonState::default() };
/// m.update(hover, 1. / 60., Motion::Full);
/// assert!(m.hover() > 0. && m.hover() < 1.);
/// m.update(hover, 1. / 60., Motion::Reduced);
/// assert_eq!(m.hover(), 1.);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ButtonMotion {
    hover: Spring,
    press: Spring,
    focus: Spring,
    enabled: bool,
}

impl Default for ButtonMotion {
    fn default() -> Self {
        Self {
            hover: Spring::critical(0., 7.),
            press: Spring::critical(0., 14.),
            focus: Spring::critical(0., 6.),
            enabled: true,
        }
    }
}

impl ButtonMotion {
    pub fn update(&mut self, state: ButtonState, dt: f32, motion: Motion) {
        let on = |b: bool| if b { 1. } else { 0. };
        self.enabled = state.enabled;
        self.hover.set_target(on(state.enabled && state.hovered));
        self.press.set_target(on(state.enabled && state.pressed));
        self.focus.set_target(on(state.enabled && state.focused));
        for spring in [&mut self.hover, &mut self.press, &mut self.focus] {
            spring.advance(dt, motion);
        }
    }
    /// Snaps all amounts to `state` (e.g. when a menu opens).
    pub fn snap(&mut self, state: ButtonState) {
        self.update(state, 0., Motion::Reduced);
    }
    pub fn hover(&self) -> f32 {
        self.hover.value.clamp(0., 1.)
    }
    pub fn press(&self) -> f32 {
        self.press.value.clamp(0., 1.)
    }
    pub fn focus(&self) -> f32 {
        self.focus.value.clamp(0., 1.)
    }
}

/// Button emphasis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ButtonKind {
    /// Accent fill for the main action.
    #[default]
    Primary,
    /// Raised neutral fill.
    Secondary,
    /// Text-only until hovered.
    Ghost,
    /// Destructive action.
    Danger,
}

/// Paints a button: rounded fill that brightens on hover, sinks and darkens
/// on press, and an eased focus ring outside the edge. Disabled buttons are
/// flat and muted. The label is centered in the modern face.
///
/// ```
/// use ferravex_render::{Rect, ui::kit::{self, ButtonKind, ButtonMotion, KitTheme}};
/// kit::button(Rect::new(20., 20., 160., 40.), "New game", ButtonKind::Primary,
///     &ButtonMotion::default(), &KitTheme::default());
/// ```
pub fn button(rect: Rect, label: &str, kind: ButtonKind, motion: &ButtonMotion, theme: &KitTheme) {
    let (hover, press, focus) = (motion.hover(), motion.press(), motion.focus());
    let radius = theme.radius.min(rect.h * 0.5);
    let sink = 2. * press;
    let r = Rect::new(rect.x, rect.y + sink, rect.w, rect.h);
    let (base, ink) = match kind {
        ButtonKind::Primary => (theme.accent, theme.accent_ink),
        ButtonKind::Secondary => (theme.raised, theme.ink),
        ButtonKind::Ghost => (
            Color::new(theme.raised.r, theme.raised.g, theme.raised.b, 0.),
            theme.ink,
        ),
        ButtonKind::Danger => (theme.danger, theme.accent_ink),
    };
    if !motion.enabled {
        shapes::fill_rounded_rect(r, radius, theme.c(mix(theme.surface, theme.raised, 0.5)));
        shapes::stroke_rounded_rect(r, radius, 1., theme.c(theme.line));
        let style = theme
            .text(theme.body, theme.disabled)
            .with_align(TextAlign::Center);
        let c = r.center();
        draw_text_styled(label, c.x, style.centered_baseline(c.y), style);
        return;
    }
    if kind != ButtonKind::Ghost {
        shapes::drop_shadow(
            r,
            radius,
            Shadow::soft(theme.c(Color::new(0., 0., 0., 0.35 * (1. - press))))
                .offset(vec2(0., 3. - sink))
                .blur(10.),
        );
    }
    let white = Color::new(1., 1., 1., 1.);
    let lit = |c: Color, k: f32| {
        let m = mix(c, white, k);
        Color::new(m.r, m.g, m.b, c.a)
    };
    let mut fill = lit(base, 0.12 * hover);
    if kind == ButtonKind::Ghost {
        fill.a = 0.9 * hover.max(press);
    }
    fill = mix(fill, Color::new(0., 0., 0., fill.a), 0.18 * press);
    let top = lit(fill, 0.08);
    shapes::fill_rounded_rect(r, radius, Fill::vertical(r, theme.c(top), theme.c(fill)));
    shapes::stroke_rounded_rect(
        r,
        radius,
        1.,
        theme.c(Color::new(1., 1., 1., 0.10 + 0.12 * hover)),
    );
    if focus > 0.01 {
        let grow = 3. + 1. * (1. - focus);
        let ring = Rect::new(r.x - grow, r.y - grow, r.w + 2. * grow, r.h + 2. * grow);
        let f = theme.focus;
        shapes::stroke_rounded_rect(
            ring,
            radius + grow,
            2.,
            theme.c(Color::new(f.r, f.g, f.b, f.a * focus)),
        );
    }
    let mut style = theme.text(theme.body, ink).with_align(TextAlign::Center);
    if matches!(kind, ButtonKind::Secondary | ButtonKind::Ghost) {
        // Light ink on a dark fill reads crisper with a 1 px shadow.
        style = style.with_shadow(theme.c(Color::new(0., 0., 0., 0.35)));
    }
    let c = r.center();
    draw_text_styled(label, c.x, style.centered_baseline(c.y), style);
}

/// Horizontal meter (health, XP, loading): an inset rounded track, a gradient
/// fill with a top highlight and an optional lighter `ghost` value trailing
/// behind (show the pre-damage value and tween it down for a "chip" effect).
/// Values are clamped to `[0, 1]`.
///
/// ```
/// use ferravex_render::{Rect, ui::kit::{self, KitTheme}};
/// let theme = KitTheme::default();
/// kit::meter(Rect::new(20., 20., 180., 12.), 0.4, Some(0.65), theme.danger, &theme);
/// ```
pub fn meter(rect: Rect, value: f32, ghost: Option<f32>, color: Color, theme: &KitTheme) {
    let clamp = |v: f32| if v.is_finite() { v.clamp(0., 1.) } else { 0. };
    let radius = rect.h * 0.5;
    shapes::fill_rounded_rect(rect, radius, theme.c(Color::new(0., 0., 0., 0.45)));
    shapes::stroke_rounded_rect(rect, radius, 1., theme.c(Color::new(1., 1., 1., 0.06)));
    let span = |v: f32| Rect::new(rect.x, rect.y, (rect.w * v).max(rect.h.min(rect.w)), rect.h);
    if let Some(g) = ghost.map(clamp).filter(|&g| g > clamp(value)) {
        let r = span(g);
        shapes::fill_rounded_rect(r, radius, theme.c(Color::new(1., 0.95, 0.85, 0.55)));
    }
    let v = clamp(value);
    if v <= 0. {
        return;
    }
    let r = span(v);
    let white = Color::new(1., 1., 1., color.a);
    shapes::fill_rounded_rect(
        r,
        radius,
        Fill::vertical(r, theme.c(mix(color, white, 0.25)), theme.c(color)),
    );
    if r.h >= 6. {
        let gloss = Rect::new(
            r.x + radius * 0.5,
            r.y + r.h * 0.2,
            (r.w - radius).max(0.),
            r.h * 0.22,
        );
        shapes::fill_rounded_rect(gloss, gloss.h * 0.5, theme.c(Color::new(1., 1., 1., 0.22)));
    }
}

/// Notification tone for [`toast`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tone {
    #[default]
    Info,
    Success,
    Warning,
    Danger,
}

/// A toast centered horizontally on `center_x` with its top at `top`. Pass the
/// [`Appearance`] from a [`ferravex_play::tween::Entrance`] to slide/fade it.
/// `icon` is drawn in the tone's color before the text. Returns its rectangle.
///
/// ```
/// use ferravex_play::tween::Appearance;
/// use ferravex_render::ui::kit::{self, KitTheme, Tone};
/// let r = kit::toast(640., 24., Some('★'), "Achievement unlocked", Tone::Success,
///     Appearance::SHOWN, &KitTheme::default());
/// assert!(r.w > 100.);
/// ```
pub fn toast(
    center_x: f32,
    top: f32,
    icon: Option<char>,
    text: &str,
    tone: Tone,
    look: Appearance,
    theme: &KitTheme,
) -> Rect {
    let theme = theme.faded(look.alpha);
    let tint = match tone {
        Tone::Info => theme.focus,
        Tone::Success => theme.success,
        Tone::Warning => theme.warning,
        Tone::Danger => theme.danger,
    };
    let style = theme.text(theme.body, theme.ink);
    let mut buffer = [0_u8; 4];
    let icon_text = icon.map(|c| &*c.encode_utf8(&mut buffer));
    let icon_w = icon_text.map_or(0., |i| text_width(i, style) + SPACE[1]);
    let w = icon_w + text_width(text, style) + SPACE[3] * 2. + SPACE[0];
    let h = theme.body * 1.25 + SPACE[2];
    let rect = appear(Rect::new(center_x - w * 0.5, top, w, h), look);
    let radius = rect.h * 0.5;
    shapes::drop_shadow(
        rect,
        radius,
        Shadow::soft(theme.c(theme.shadow))
            .offset(vec2(0., 6.))
            .blur(20.),
    );
    shapes::fill_rounded_rect(
        rect,
        radius,
        Fill::vertical(rect, theme.c(theme.raised), theme.c(theme.surface)),
    );
    shapes::stroke_rounded_rect(
        rect,
        radius,
        1.,
        theme.c(Color::new(tint.r, tint.g, tint.b, 0.35)),
    );
    let s = rect.w / w.max(1e-3);
    let style = style.with_size(theme.body * s);
    let baseline = style.centered_baseline(rect.y + rect.h * 0.5);
    let mut x = rect.x + (SPACE[3] + SPACE[0]) * s;
    if let Some(icon) = icon_text {
        let icon_style = style.with_color(theme.c(tint));
        draw_text_styled(icon, x, baseline, icon_style);
        x += icon_w * s;
    }
    draw_text_styled(text, x, baseline, style.with_pixel_snap(look.scale == 1.));
    rect
}

/// A keyboard/gamepad key cap (e.g. `"Space"`, `"E"`, `"A"`) with a raised
/// lip, its top-left at (`x`, `y`). Returns its rectangle so prompts can flow.
///
/// ```
/// use ferravex_render::ui::kit::{self, KitTheme};
/// let theme = KitTheme::default();
/// let cap = kit::keycap(20., 20., "E", &theme);
/// assert!(cap.w >= cap.h - 3.); // single letters stay square-ish
/// ```
pub fn keycap(x: f32, y: f32, label: &str, theme: &KitTheme) -> Rect {
    let style = theme.text(theme.body, theme.ink);
    let h = theme.body * 1.5 + 2.;
    let w = (text_width(label, style) + SPACE[2] * 2.).max(h - 3.);
    let rect = Rect::new(x, y, w, h - 3.);
    let lip = Rect::new(x, y + 3., w, h - 3.);
    shapes::fill_rounded_rect(
        lip,
        RADIUS_SMALL,
        theme.c(mix(theme.surface, Color::new(0., 0., 0., 1.), 0.35)),
    );
    shapes::fill_rounded_rect(
        rect,
        RADIUS_SMALL,
        Fill::vertical(
            rect,
            theme.c(mix(theme.raised, theme.header, 0.7)),
            theme.c(theme.raised),
        ),
    );
    shapes::stroke_rounded_rect(
        rect,
        RADIUS_SMALL,
        1.,
        theme.c(Color::new(1., 1., 1., 0.14)),
    );
    let c = rect.center();
    let style = style.with_align(TextAlign::Center);
    draw_text_styled(label, c.x, style.centered_baseline(c.y), style);
    Rect::new(x, y, w, h)
}

/// Key caps followed by an action label, e.g. `[E] Open`. Returns the total width.
///
/// ```
/// use ferravex_render::ui::kit::{self, KitTheme};
/// let w = kit::key_prompt(20., 20., &["Shift", "Space"], "Dash", &KitTheme::default());
/// assert!(w > 100.);
/// ```
pub fn key_prompt(x: f32, y: f32, keys: &[&str], action: &str, theme: &KitTheme) -> f32 {
    let mut cursor = x;
    let mut height: f32 = 0.;
    for key in keys {
        let cap = keycap(cursor, y, key, theme);
        cursor += cap.w + SPACE[0];
        height = height.max(cap.h);
    }
    let style = theme.text(theme.body, theme.muted);
    let baseline = style.centered_baseline(y + (height - 3.) * 0.5);
    let w = draw_text_styled(action, cursor + SPACE[0], baseline, style).width;
    cursor + SPACE[0] + w - x
}

/// Draws a full-screen scene transition at `coverage` (0 clear … 1 covered)
/// in `color`. Wipes have a soft leading edge; the iris closes on `focus`
/// (screen units). Use `ScreenTransition::kind(motion)` so reduced motion fades.
///
/// ```
/// use ferravex_play::tween::TransitionKind;
/// use ferravex_render::{Color, vec2, ui::kit};
/// kit::draw_screen_transition(TransitionKind::Iris, 0.5, Color::new(0., 0., 0., 1.), vec2(640., 360.));
/// ```
pub fn draw_screen_transition(kind: TransitionKind, coverage: f32, color: Color, focus: Vec2) {
    let t = if coverage.is_finite() {
        coverage.clamp(0., 1.)
    } else {
        0.
    };
    if t <= 0. {
        return;
    }
    let (w, h) = (crate::screen_width(), crate::screen_height());
    let screen = Rect::new(0., 0., w, h);
    match kind {
        TransitionKind::Fade => {
            shapes::fill_rounded_rect(
                screen,
                0.,
                Color::new(color.r, color.g, color.b, color.a * t),
            );
        }
        _ if t >= 1. => shapes::fill_rounded_rect(screen, 0., color),
        TransitionKind::Wipe { direction } => {
            let d = vec2(direction.cos(), direction.sin());
            let d = if d.is_finite() { d } else { Vec2::X };
            let center = vec2(w, h) * 0.5;
            let half = (w * w + h * h).sqrt() * 0.5 + 2.;
            let soft = 0.12 * half;
            // Leading edge travels from -half to half + soft.
            let edge = -half + t * (2. * half + soft);
            crate::edit(|s| {
                let mut sink = shapes::Sink::new(s);
                let p = d.perp() * half;
                let at = |a: f32| center + d * a;
                let clear = Color::new(color.r, color.g, color.b, 0.);
                let solid_end = (edge - soft).max(-half);
                if solid_end > -half {
                    let (a, b) = (at(-half), at(solid_end));
                    sink.tri(a - p, color, b - p, color, a + p, color);
                    sink.tri(a + p, color, b - p, color, b + p, color);
                }
                let (a, b) = (at(solid_end), at(edge));
                sink.tri(a - p, color, b - p, clear, a + p, color);
                sink.tri(a + p, color, b - p, clear, b + p, clear);
            });
        }
        TransitionKind::Iris => {
            let reach = [vec2(0., 0.), vec2(w, 0.), vec2(0., h), vec2(w, h)]
                .iter()
                .map(|c| (*c - focus).length())
                .fold(0., f32::max)
                + 2.;
            let inner = reach * (1. - t);
            let outer = reach + 4.;
            if inner < 0.5 {
                return shapes::fill_rounded_rect(screen, 0., color);
            }
            // The ring's outer radius reaches past the farthest screen corner.
            shapes::stroke_circle(focus, (inner + outer) * 0.5, outer - inner, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shapes::tests::capture;
    use ferravex_play::tween::Entrance;

    #[test]
    fn kit_widgets_draw_bounded_finite_geometry() {
        let theme = KitTheme::default();
        let v = capture(1280, 720, || {
            let inner = card(Rect::new(40., 40., 400., 300.), Some("Settings"), &theme);
            assert!(inner.w > 0. && inner.h > 0.);
            let mut m = ButtonMotion::default();
            m.update(
                ButtonState {
                    hovered: true,
                    focused: true,
                    ..Default::default()
                },
                0.1,
                Motion::Full,
            );
            for kind in [
                ButtonKind::Primary,
                ButtonKind::Secondary,
                ButtonKind::Ghost,
                ButtonKind::Danger,
            ] {
                button(Rect::new(60., 100., 160., 40.), "Play", kind, &m, &theme);
            }
            m.update(
                ButtonState {
                    enabled: false,
                    ..Default::default()
                },
                0.1,
                Motion::Full,
            );
            button(
                Rect::new(60., 160., 160., 40.),
                "Locked",
                ButtonKind::Primary,
                &m,
                &theme,
            );
            meter(
                Rect::new(60., 220., 200., 12.),
                0.3,
                Some(0.6),
                theme.success,
                &theme,
            );
            meter(
                Rect::new(60., 240., 200., 12.),
                f32::NAN,
                None,
                theme.success,
                &theme,
            );
            toast(
                640.,
                20.,
                Some('★'),
                "Saved",
                Tone::Success,
                Entrance::fade_up(10.).sample(0.05, 0, Motion::Full),
                &theme,
            );
            key_prompt(60., 300., &["E"], "Open", &theme);
        });
        assert!(!v.is_empty());
        assert!(v.iter().flatten().all(|x| x.is_finite()));
        assert!(v.len() < 40_000, "{}", v.len());
    }

    #[test]
    fn transitions_cover_the_screen_monotonically() {
        use crate::specimen::rasterize;
        for kind in [
            TransitionKind::Fade,
            TransitionKind::Wipe { direction: 0.3 },
            TransitionKind::Iris,
        ] {
            let mut previous = -1.;
            for step in 0..=4 {
                let t = step as f32 / 4.;
                capture(64, 48, || {
                    draw_screen_transition(kind, t, Color::new(1., 1., 1., 1.), vec2(20., 20.))
                });
                let image = rasterize(64, 48, [0.; 3]);
                let covered: f32 = image.iter().map(|p| p[0]).sum::<f32>() / image.len() as f32;
                assert!(
                    covered >= previous - 1e-3,
                    "{kind:?} at {t}: {covered} < {previous}"
                );
                if step == 0 {
                    assert_eq!(covered, 0.);
                }
                if step == 4 {
                    assert!(covered > 0.999, "{kind:?} fully covers: {covered}");
                }
                previous = covered;
            }
        }
    }

    #[test]
    fn ui_rect_and_button_state_follow_the_shared_ui() {
        let viewport = Viewport::fit([800., 600.], [400., 300.], [0.; 4]).unwrap();
        let r = ui_rect(
            viewport,
            UiRect {
                min: [0., 0.],
                size: [400., 300.],
            },
        );
        assert_eq!((r.w, r.h), (800., 600.));
        let ui = Ui::new();
        assert_eq!(
            ui_button_state(&ui, 1, false),
            ButtonState {
                enabled: false,
                ..Default::default()
            }
        );
    }
}
