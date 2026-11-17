//! HUD, banners and Ready/Paused/Results cards, drawn with the modern UI kit.
//!
//! Layout is authored in the 1280×800 design space (wide) or in CSS pixels
//! (compact) and mapped to the screen by a [`Painter`]. Body copy, hints and
//! cards use the mixed-case Modern face; big numerals, the logo and arcade
//! callouts keep the Classic face. Menus keep `Ui` for input (an invisible
//! button over the authored action rectangle and the contract card panel)
//! and are painted on top with the kit after `Ui::paint`. Steady-state frames
//! format text into fixed stack buffers and allocate nothing.
use crate::game::{CLUTCH_HEALTH, Game, Phase, Style};
use crate::juice::{GOLD, Juice, MAGENTA, Priority, fade, tier_color};
use crate::quips::{self, Rank, RunSummary};
use crate::{AMBER, CORAL, CYAN, INK, MUTED, PAPER, hint};
use ferravex_play::encounter::AdmissionPreview;
use ferravex_play::tween::{
    Ease, Entrance, Motion, RollCounter, ScreenTransition, TransitionEvent, TransitionKind,
    TransitionPhase,
};
use ferravex_render::icons::{self, Icon, IconStyle};
use ferravex_render::input::PlayerInput;
use ferravex_render::post;
use ferravex_render::prelude::{Color, Rect, Vec2, vec2};
use ferravex_render::shapes::{self, Cap, Fill, Shadow, Stroke};
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_render::ui::kit::{self, ButtonKind, ButtonMotion, KitTheme};
use ferravex_render::ui::{Ui, UiRect, Viewport};
use std::fmt::{self, Write as _};

/// The card panel color doubles as the shared menu contract in browser tests.
pub(crate) const CARD: Color = Color::new(0.025, 0.055, 0.07, 0.82);
/// The primary action keeps the authored 470..810 × 468..520 rectangle.
pub(crate) const ACTION: UiRect = UiRect {
    min: [470., 468.],
    size: [340., 52.],
};
pub(crate) const SHIELD: Color = crate::draw::BATTERY;
/// Readable secondary copy on dark glass (brighter than the world's MUTED).
pub(crate) const SOFT: Color = Color::from_rgba(160, 186, 188, 255);
const GLASS_TOP: Color = Color::new(0.06, 0.12, 0.15, 0.86);
const GLASS_BOTTOM: Color = Color::new(0.03, 0.07, 0.09, 0.9);
const EDGE: Color = Color::new(0.38, 0.93, 0.81, 0.2);

// ---------------------------------------------------------------- text ----

/// Allocation-free formatted text in a fixed stack buffer. Overlong text is
/// truncated at a character boundary.
#[derive(Clone, Copy)]
pub(crate) struct Line {
    buf: [u8; 112],
    len: u8,
}

impl Line {
    pub const fn new() -> Self {
        Self {
            buf: [0; 112],
            len: 0,
        }
    }

    pub fn fmt(args: fmt::Arguments<'_>) -> Self {
        let mut line = Self::new();
        let _ = line.write_fmt(args);
        line
    }

    pub fn from(text: &str) -> Self {
        Self::fmt(format_args!("{text}"))
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len as usize]).unwrap_or("")
    }
}

impl From<&str> for Line {
    fn from(s: &str) -> Self {
        Self::fmt(format_args!("{s}"))
    }
}

impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Write for Line {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            let at = self.len as usize;
            if at + c.len_utf8() > self.buf.len() {
                break;
            }
            c.encode_utf8(&mut self.buf[at..]);
            self.len += c.len_utf8() as u8;
        }
        Ok(())
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Line {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl PartialEq<&str> for Line {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

/// `Line::fmt(format_args!(..))` shorthand.
macro_rules! line {
    ($($arg:tt)*) => {
        $crate::hud::Line::fmt(format_args!($($arg)*))
    };
}
pub(crate) use line;

// ------------------------------------------------------------- painter ----

#[cfg(test)]
thread_local! {
    /// Screen rectangles of text and buttons drawn this test frame.
    pub(crate) static DRAWN: std::cell::RefCell<Vec<Rect>> = const {
        std::cell::RefCell::new(Vec::new())
    };
}

/// Maps authored layout units to screen pixels: the wide design canvas
/// (`Viewport::fit`) or one unit per CSS pixel (`Viewport::responsive`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Painter {
    pub scale: f32,
    origin: Vec2,
}

impl Painter {
    pub fn new(viewport: Viewport) -> Self {
        let pixels = viewport.pixels();
        let scale = viewport.scale();
        Self {
            scale: if scale.is_finite() && scale > 0. {
                scale
            } else {
                1.
            },
            origin: vec2(pixels.min[0], pixels.min[1]),
        }
    }

    pub fn point(&self, x: f32, y: f32) -> Vec2 {
        self.origin + vec2(x, y) * self.scale
    }

    pub fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        let p = self.point(x, y);
        Rect::new(p.x, p.y, w * self.scale, h * self.scale)
    }

    pub fn ui(&self, rect: UiRect) -> Rect {
        self.rect(rect.min[0], rect.min[1], rect.size[0], rect.size[1])
    }

    fn style(&self, style: TextStyle) -> TextStyle {
        style.with_size(style.size * self.scale)
    }

    /// Draws text with its baseline at `y`; returns its width in layout units.
    pub fn text(&self, text: &str, x: f32, y: f32, style: TextStyle) -> f32 {
        let style = self.style(style);
        let p = self.point(x, y);
        let m = draw_text_styled(text, p.x, p.y, style);
        #[cfg(test)]
        if !text.is_empty() {
            let left = match style.align {
                TextAlign::Left => p.x,
                TextAlign::Center => p.x - m.width * 0.5,
                TextAlign::Right => p.x - m.width,
            };
            DRAWN.with(|d| {
                d.borrow_mut().push(Rect::new(
                    left,
                    p.y - m.ascent,
                    m.width,
                    m.ascent + m.descent,
                ))
            });
        }
        m.width / self.scale
    }

    /// Width of `text` in layout units.
    pub fn width(&self, text: &str, style: TextStyle) -> f32 {
        text_width(text, self.style(style)) / self.scale
    }

    /// Frosted HUD glass: a top-lit rounded fill with a faint neon edge.
    pub fn glass(&self, x: f32, y: f32, w: f32, h: f32, alpha: f32) {
        let r = self.rect(x, y, w, h);
        let radius = 12. * self.scale;
        shapes::fill_rounded_rect(
            r,
            radius,
            Fill::vertical(r, fade(GLASS_TOP, alpha), fade(GLASS_BOTTOM, alpha)),
        );
        shapes::stroke_rounded_rect(r, radius, 1., fade(EDGE, alpha));
    }

    /// A kit meter in layout units.
    pub fn meter(
        &self,
        rect: Rect,
        value: f32,
        ghost: Option<f32>,
        color: Color,
        theme: &KitTheme,
    ) {
        kit::meter(
            self.rect(rect.x, rect.y, rect.w, rect.h),
            value,
            ghost,
            color,
            theme,
        );
    }

    pub fn button(&self, rect: UiRect, label: &str, motion: &ButtonMotion, theme: &KitTheme) {
        let r = self.ui(rect);
        #[cfg(test)]
        DRAWN.with(|d| d.borrow_mut().push(r));
        // Neon halo under the primary action, stronger on hover/focus.
        let lit = 0.25 + 0.35 * motion.hover().max(motion.focus());
        shapes::drop_shadow(
            r,
            theme.radius,
            Shadow::soft(fade(CYAN, lit * theme.alpha))
                .blur(18. * self.scale)
                .spread(1.),
        );
        kit::button(r, label, ButtonKind::Primary, motion, theme);
    }

    /// The primary action as `[key] icon Label`: a key cap, a pictogram and
    /// one word, centered on the authored button.
    pub fn key_button(
        &self,
        rect: UiRect,
        key: &str,
        icon: Icon,
        label: &str,
        motion: &ButtonMotion,
        theme: &KitTheme,
    ) {
        self.button(rect, "", motion, theme);
        let text = modern(16., fade(INK, theme.alpha));
        let key_w = self.keycap_width(key) + 8.;
        let w = key_w + 22. + 8. + self.width(label, text);
        let cy = rect.min[1] + rect.size[1] * 0.5 + 2. * motion.press();
        let x = rect.min[0] + rect.size[0] * 0.5 - w * 0.5;
        self.keycap(x, cy - 12., key, theme);
        self.icon(icon, x + key_w + 10., cy, 20., fade(INK, theme.alpha));
        self.text(label, x + key_w + 30., text.centered_baseline(cy), text);
    }

    /// A pictogram centered on a layout point.
    pub fn icon(&self, icon: Icon, x: f32, y: f32, size: f32, color: Color) {
        icons::draw_styled(
            icon,
            self.point(x, y),
            size * self.scale,
            IconStyle::new(color).shadow(fade(SHADOW, color.a * 0.8)),
        );
    }

    /// A kit key cap at a layout point; returns its width in layout units.
    pub fn keycap(&self, x: f32, y: f32, label: &str, theme: &KitTheme) -> f32 {
        let at = self.point(x, y);
        kit::keycap(at.x, at.y, label, theme).w / self.scale
    }

    /// Width of [`Painter::keycap`] without drawing.
    pub fn keycap_width(&self, label: &str) -> f32 {
        (self.width(label, modern(16., PAPER)) + 24.).max(23.)
    }

    /// A ring meter: a faint track and an arc filled clockwise from the top.
    pub fn ring(&self, x: f32, y: f32, radius: f32, fraction: f32, width: f32, color: Color) {
        let c = self.point(x, y);
        shapes::stroke_circle(
            c,
            radius * self.scale,
            width * self.scale,
            Color::new(1., 1., 1., 0.08 * color.a),
        );
        if fraction > 0. {
            shapes::stroke_arc(
                c,
                radius * self.scale,
                -std::f32::consts::FRAC_PI_2,
                std::f32::consts::TAU * fraction.clamp(0., 1.),
                Stroke::new(width * self.scale, color).cap(Cap::Round),
            );
        }
    }

    /// The game's drone silhouette: a neon diamond with one eye.
    pub fn drone(&self, x: f32, y: f32, size: f32, color: Color) {
        let c = self.point(x, y);
        let r = size * 0.5 * self.scale;
        let points = [
            c + vec2(0., -r),
            c + vec2(r, 0.),
            c + vec2(0., r),
            c + vec2(-r, 0.),
        ];
        shapes::fill_polygon(&points, fade(INK, color.a));
        shapes::polyline(
            &points,
            Stroke::new(2. * self.scale, color)
                .closed(true)
                .join(ferravex_render::shapes::Join::Round),
        );
        shapes::fill_circle(
            c + vec2(r * 0.18, -r * 0.05),
            r * 0.22,
            fade(PAPER, color.a),
        );
    }
}

pub(crate) fn modern(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}

pub(crate) fn classic(size: f32, color: Color) -> TextStyle {
    TextStyle::classic(size, color)
}

const fn centered(style: TextStyle) -> TextStyle {
    style.with_align(TextAlign::Center)
}

const fn right(style: TextStyle) -> TextStyle {
    style.with_align(TextAlign::Right)
}

const SHADOW: Color = Color::new(0., 0., 0., 0.55);

/// Neon kit theme at the painter's scale; `alpha` fades every color.
pub(crate) fn theme(scale: f32, alpha: f32) -> KitTheme {
    KitTheme {
        backdrop: Color::new(0.01, 0.02, 0.03, 0.55),
        surface: Color::from_rgba(12, 24, 31, 240),
        raised: Color::from_rgba(22, 44, 52, 255),
        header: Color::from_rgba(26, 54, 62, 255),
        ink: PAPER,
        muted: SOFT,
        disabled: MUTED,
        accent: CYAN,
        accent_ink: INK,
        success: crate::draw::REPAIR,
        warning: AMBER,
        danger: CORAL,
        focus: Color::from_rgba(210, 255, 244, 255),
        line: EDGE,
        shadow: Color::new(0., 0., 0., 0.6),
        radius: 12. * scale,
        body: 16. * scale,
        title: 24. * scale,
        alpha: 1.,
    }
    .faded(alpha)
}

// ---------------------------------------------------------- run state ----

/// Fire/dash onboarding, learned from accepted feedback events.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Learned {
    pub fired: bool,
    pub dashed: bool,
}

impl Learned {
    pub fn complete(self) -> bool {
        self.fired && self.dashed
    }
}

/// A finished run against stored records, captured once at the finish.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Results {
    pub summary: RunSummary,
    pub rank: Rank,
    pub time: f32,
    pub kills: u32,
    /// The stored best score before this run, if any.
    pub previous_best: Option<u64>,
    pub attempt: u32,
}

impl Results {
    pub fn new(game: &Game, previous_best: Option<u64>, attempt: u32) -> Self {
        let summary = summary(game);
        Self {
            summary,
            rank: quips::rank(summary),
            time: game.time,
            kills: game.kills,
            previous_best,
            attempt,
        }
    }

    pub fn new_best(&self) -> bool {
        self.summary.score > 0
            && self
                .previous_best
                .is_none_or(|best| self.summary.score > best)
    }

    /// The record as a trophy chip: celebrate a best, otherwise show the
    /// stored best and the gap to beat it.
    pub fn record_chip(&self) -> Chip {
        let score = self.summary.score;
        match self.previous_best {
            _ if score == 0 => Chip::new(Icon::Trophy, Line::from("—"), SOFT),
            None => Chip::new(Icon::Trophy, Line::from("FIRST RECORD"), GOLD),
            Some(best) if score > best => Chip::new(Icon::Trophy, Line::from("NEW BEST"), GOLD)
                .with_value(line!("+{}", score - best), CYAN),
            Some(best) if score == best => Chip::new(Icon::Trophy, Line::from("TIED"), AMBER),
            Some(best) => Chip::new(Icon::Trophy, line!("{best}"), PAPER)
                .with_value(line!("−{}", best - score), CORAL),
        }
    }

    pub fn quip(&self) -> Line {
        let s = self.summary;
        if s.won && s.health <= CLUTCH_HEALTH {
            line!("Finished on {} HP. The hexagon needs a nap.", s.health)
        } else if s.won {
            Line::from(quips::win(self.attempt))
        } else {
            Line::from(quips::loss(self.attempt))
        }
    }
}

pub(crate) fn summary(game: &Game) -> RunSummary {
    RunSummary {
        won: game.phase == Phase::Won,
        wave: game.wave,
        health: game.health,
        score: game.score.total(),
        best_chain: game.score.best(),
        style: game.style,
    }
}

/// Plain digits; kept as one helper so every score reads the same.
pub(crate) fn thousands(value: u64) -> Line {
    line!("{value}")
}

/// The first of several bindings, so hints stay short; Controls lists all.
pub(crate) fn key<'a>(input: &PlayerInput, action: &str, buffer: &'a mut [u8]) -> &'a str {
    let full = hint(input, action, buffer);
    full.split(" / ").next().unwrap_or(full)
}

/// A pill with a pictogram, short text and an optional trailing number.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Chip {
    pub icon: Icon,
    pub text: Line,
    pub color: Color,
    pub value: Line,
    pub value_color: Color,
}

impl Chip {
    pub fn new(icon: Icon, text: Line, color: Color) -> Self {
        Self {
            icon,
            text,
            color,
            value: Line::new(),
            value_color: color,
        }
    }

    pub fn with_value(mut self, value: Line, color: Color) -> Self {
        self.value = value;
        self.value_color = color;
        self
    }

    fn width(&self, p: &Painter, lead: bool) -> f32 {
        let text = modern(16., PAPER);
        let value = if self.value.as_str().is_empty() {
            0.
        } else {
            10. + p.width(self.value.as_str(), text)
        };
        (if lead { 24. } else { 0. }) + 28. + p.width(self.text.as_str(), text) + value + 24.
    }

    /// Draws the pill centered on (`x`, `y`), optionally led by `lead`.
    fn draw(&self, p: &Painter, x: f32, y: f32, lead: Option<Icon>, alpha: f32) {
        let w = self.width(p, lead.is_some());
        let r = p.rect(x - w * 0.5, y - 15., w, 30.);
        shapes::fill_rounded_rect(r, 15. * p.scale, fade(self.color, 0.1 * alpha));
        shapes::stroke_rounded_rect(r, 15. * p.scale, 1., fade(self.color, 0.4 * alpha));
        let mut cursor = x - w * 0.5 + 12.;
        if let Some(icon) = lead {
            p.icon(icon, cursor + 8., y, 16., fade(SOFT, alpha));
            cursor += 24.;
        }
        p.icon(self.icon, cursor + 10., y, 18., fade(self.color, alpha));
        cursor += 28.;
        let text = modern(16., fade(self.color, alpha));
        cursor += p.text(self.text.as_str(), cursor, text.centered_baseline(y), text);
        if !self.value.as_str().is_empty() {
            let value = text.with_color(fade(self.value_color, alpha));
            p.text(
                self.value.as_str(),
                cursor + 10.,
                value.centered_baseline(y),
                value,
            );
        }
    }
}

/// Earned style feats as pictogram and count pairs: drive-bys (bolt), close
/// shaves (wind), multi-kills (target), clean waves (shield) and lamps (bulb).
pub(crate) fn style_feats(style: &Style) -> impl Iterator<Item = (Icon, Line)> {
    [
        (Icon::Bolt, style.drive_bys, false),
        (Icon::Wind, style.close_shaves, false),
        (Icon::Target, style.multi_kills, false),
        (Icon::Shield, style.untouched_waves, false),
        (Icon::Bulb, style.beacons_lit(), true),
    ]
    .into_iter()
    .filter(|(_, n, _)| *n > 0)
    .map(|(icon, n, lamps)| (icon, if lamps { line!("{n}/4") } else { line!("{n}") }))
}

/// "WASD" when every movement binding is a single key.
pub(crate) fn movement_hint(input: &PlayerInput) -> Line {
    let mut out = Line::new();
    for action in ["move_forward", "move_left", "move_back", "move_right"] {
        let mut buffer = [0; 64];
        let first = key(input, action, &mut buffer);
        if first.chars().count() != 1 {
            return Line::from("Move keys");
        }
        for c in first.chars() {
            let _ = out.write_char(c.to_ascii_uppercase());
        }
    }
    out
}

// --------------------------------------------------------- menu state ----

/// Which full screen the HUD is presenting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Screen {
    Ready,
    Playing,
    Paused,
    Results,
}

/// Presentation-only UI motion: entrance clocks, button springs, rolling
/// numbers, the damage "ghost" on the health meter and scene transitions.
pub(crate) struct Hud {
    pub screen: Screen,
    /// Seconds since the current screen opened (presentation time).
    pub time: f32,
    pub button: ButtonMotion,
    pub score: RollCounter,
    pub stats: [RollCounter; 4],
    ghost: f32,
    ghost_hold: f32,
    ghost_target: f32,
    pub transition: ScreenTransition,
    pub pause_fade: f32,
}

impl Hud {
    pub fn new() -> Self {
        Self {
            screen: Screen::Ready,
            time: 0.,
            button: ButtonMotion::default(),
            score: RollCounter::new(0),
            stats: [RollCounter::new(0); 4],
            ghost: 1.,
            ghost_hold: 0.,
            ghost_target: 1.,
            transition: ScreenTransition::new(TransitionKind::Iris, 0.42),
            pause_fade: 0.,
        }
    }

    /// Track screen changes and age every tween by presentation time.
    pub fn update(
        &mut self,
        screen: Screen,
        game: &Game,
        results: Option<&Results>,
        dt: f32,
        motion: Motion,
    ) {
        if screen != self.screen {
            let from = self.screen;
            self.screen = screen;
            self.time = 0.;
            self.button = ButtonMotion::default();
            if screen == Screen::Playing && matches!(from, Screen::Ready | Screen::Results) {
                // Open on the fresh arena with an iris from the player.
                self.transition = ScreenTransition::new(TransitionKind::Iris, 0.42);
                self.transition.ease = Ease::CubicOut;
                self.transition.cover();
                self.transition.advance(1., Motion::Full);
                self.transition.reveal();
                self.score.snap(0);
                self.ghost = 1.;
            }
            if screen == Screen::Results
                && let Some(r) = results
            {
                for (i, value) in [
                    r.summary.score as i64,
                    if r.summary.won {
                        r.time.round() as i64
                    } else {
                        r.summary.wave as i64
                    },
                    if r.summary.won {
                        r.summary.best_chain as i64
                    } else {
                        r.kills as i64
                    },
                    r.summary.style.points as i64,
                ]
                .into_iter()
                .enumerate()
                {
                    self.stats[i].snap(0);
                    self.stats[i].set_target(value);
                }
            }
        }
        self.time += dt;
        if let Some(TransitionEvent::Covered) = self.transition.advance(dt, motion) {
            self.transition.reveal();
        }
        self.score.set_target(game.score.total() as i64);
        self.score.advance(dt, motion);
        // Delay the ghost slightly after a hit, then drain it (a "chip" meter).
        let health = game.health as f32 / 100.;
        if health > self.ghost {
            self.ghost = health;
        } else if health < self.ghost {
            if health < self.ghost_target {
                self.ghost_hold = 0.45;
            }
            self.ghost_hold -= dt;
            if self.ghost_hold <= 0. || motion.is_reduced() {
                self.ghost = (self.ghost - dt * 0.7).max(health);
                self.ghost_hold = 0.;
            }
        }
        self.ghost_target = health;
        if screen == Screen::Results {
            let delay = 0.35;
            if self.time > delay {
                for (i, roll) in self.stats.iter_mut().enumerate() {
                    if self.time > delay + i as f32 * 0.12 {
                        roll.advance(dt, motion);
                    }
                }
            }
        }
        let target = if screen == Screen::Paused { 1. } else { 0. };
        let rate = if motion.is_reduced() { 100. } else { 6. };
        self.pause_fade += (target - self.pause_fade).clamp(-rate * dt, rate * dt);
    }

    /// Presentation-time restart (Retry/R): reopen with an iris.
    pub fn restart(&mut self) {
        self.screen = Screen::Results;
    }

    pub fn ghost(&self) -> f32 {
        self.ghost
    }

    /// Staggered fade-up for element `index` of the current screen.
    pub fn look(&self, index: usize, motion: Motion) -> ferravex_play::tween::Appearance {
        Entrance::fade_up(14.)
            .with_stagger(0.045)
            .with_duration(0.34)
            .sample(self.time, index, motion)
    }

    /// Draw the scene transition over everything (screen space).
    pub fn draw_transition(&self, motion: Motion, focus: Vec2) {
        if self.transition.phase() != TransitionPhase::Idle {
            kit::draw_screen_transition(
                self.transition.kind(motion),
                self.transition.coverage(),
                INK,
                focus,
            );
        }
    }
}

// ----------------------------------------------------------- play HUD ----

/// Full-bleed bands behind the wide HUD so it stays legible when the camera
/// pushes in or shakes. These may pass under the fullscreen menu corner.
pub(crate) fn bands(p: &Painter, screen: [f32; 2]) {
    let top = p.point(0., 96.).y;
    let bottom = p.point(0., 732.).y;
    let band = |y0: f32, y1: f32, edge: f32| {
        let r = Rect::new(0., y0, screen[0], (y1 - y0).max(0.));
        shapes::fill_rounded_rect(r, 0., Color::new(0.035, 0.063, 0.086, 0.96));
        shapes::fill_rounded_rect(Rect::new(0., edge, screen[0], 1.), 0., fade(CYAN, 0.12));
    };
    band(0., top, top - 1.);
    band(bottom, screen[1], bottom);
}

/// Dash recharge in seconds, for the ring meter (the ability bank owns it).
const DASH_RECHARGE: f32 = 1.7;

/// The in-play HUD for the wide layout: pictograms, numbers and meters only.
pub(crate) fn play(
    p: &Painter,
    game: &Game,
    input: &PlayerInput,
    learned: Learned,
    onboarding: f32,
    hud: &Hud,
    juice: &Juice,
) {
    let theme = theme(p.scale, 1.);
    let chain = game.score.chain();
    let tier = tier_color(if chain > 0 {
        game.score.multiplier()
    } else {
        0
    });

    // Left: a star and the score; the live multiplier, its chain and timer.
    p.glass(64., 14., 300., 78., 1.);
    p.icon(Icon::Star, 92., 53., 26., GOLD);
    let score = line!("{}", hud.score.shown().max(0));
    p.text(
        score.as_str(),
        112.,
        68.,
        classic(32., PAPER).with_shadow(SHADOW),
    );
    if chain > 0 {
        let label = line!("×{}", game.score.multiplier());
        p.text(
            label.as_str(),
            348.,
            60.,
            right(classic(32., tier).with_shadow(SHADOW)),
        );
        let count = line!("{chain}");
        p.icon(Icon::Flame, 254., 43., 14., tier);
        p.text(count.as_str(), 264., 49., modern(16., tier));
        let life = game.score.remaining_life().unwrap_or(0.) as f32 / 3.5;
        p.meter(Rect::new(244., 72., 104., 6.), life, None, tier, &theme);
    } else {
        p.text(
            "×1",
            348.,
            60.,
            right(classic(32., fade(SOFT, 0.35)).with_shadow(SHADOW)),
        );
    }

    // Center: the wave's name, a drone and the count left, and wave pips.
    p.text(
        quips::wave_name(game.wave),
        640.,
        34.,
        centered(modern(16., CYAN)),
    );
    let left = game.remaining_enemies();
    if left == 0 {
        p.icon(Icon::Check, 640., 58., 26., CYAN);
    } else {
        let last = game.final_drone();
        let count = line!("{left}");
        let style = classic(24., if last { CORAL } else { PAPER }).with_shadow(SHADOW);
        let w = p.width(count.as_str(), style) + 30.;
        let x = 640. - w * 0.5;
        if last && juice.motion() > 0. {
            let pulse = 0.5 + 0.5 * (juice.time * 6.).sin();
            p.ring(
                x + 11.,
                58.,
                15. + 3. * pulse,
                1.,
                1.5,
                fade(CORAL, 0.5 * pulse),
            );
        }
        p.drone(x + 11., 58., 20., CORAL);
        p.text(count.as_str(), x + 30., 70., style);
    }
    for w in 1..=3_u32 {
        let color = if w < game.wave {
            CYAN
        } else if w == game.wave {
            AMBER
        } else {
            Color::new(1., 1., 1., 0.12)
        };
        let a = p.point(604. + (w - 1) as f32 * 30., 82.);
        let b = a + vec2(20. * p.scale, 0.);
        shapes::fill_capsule(a, b, 2.5 * p.scale, color);
    }
    if game.wave_clear_remaining().is_none() {
        let (admission, position) = game.spawn_preview();
        let held = match admission {
            AdmissionPreview::AliveLimit { .. } => Some("FULL"),
            _ if position.is_none() && admission.next_available_in().is_some() => Some("BLOCKED"),
            _ => None,
        };
        if let Some(held) = held {
            // Entries are held: a lock beside the wave pips says so.
            p.icon(Icon::Lock, 716., 82., 13., AMBER);
            p.text(held, 727., 87., modern(16., AMBER));
        }
    }

    // Right: heart and shield meters with their numbers. Ends at x 1192.
    p.glass(912., 14., 296., 78., 1.);
    let low = game.health <= 30;
    let pulse = if low && juice.motion() > 0. {
        0.75 + 0.25 * (juice.time * 6.).sin()
    } else {
        1.
    };
    let heart = if low { fade(CORAL, pulse) } else { CYAN };
    p.icon(Icon::Heart, 936., 37., 20., heart);
    p.meter(
        Rect::new(954., 31., 170., 12.),
        game.health as f32 / 100.,
        Some(hud.ghost()),
        heart,
        &theme,
    );
    let health = line!("{}", game.health);
    p.text(
        health.as_str(),
        1192.,
        47.,
        right(classic(24., if low { CORAL } else { PAPER }).with_shadow(SHADOW)),
    );
    let shield_value = game.shield.value();
    let shielded = shield_value > 0.;
    p.icon(
        Icon::Shield,
        936.,
        67.,
        18.,
        if shielded { SHIELD } else { fade(SOFT, 0.5) },
    );
    p.meter(
        Rect::new(954., 63., 170., 8.),
        shield_value / game.shield.maximum(),
        None,
        SHIELD,
        &theme,
    );
    let shield = line!("{}", shield_value.ceil() as u32);
    p.text(
        shield.as_str(),
        1192.,
        74.,
        right(classic(16., if shielded { SHIELD } else { SOFT }).with_shadow(SHADOW)),
    );

    // Bottom left: the dash key and a bolt inside its recharge ring.
    let ready = game.dash_availability().check().is_ok();
    p.glass(64., 740., 132., 50., 1.);
    let mut buffer = [0; 64];
    let dash_key = key(input, "boost", &mut buffer);
    let cap = p.keycap(76., 752., dash_key, &theme).min(64.);
    let ring_x = 76. + cap + 26.;
    let charge = 1. - game.dash_cooldown / DASH_RECHARGE;
    if ready && juice.motion() > 0. {
        let glow = p.point(ring_x, 765.);
        post::glow_circle(glow.x, glow.y, 26. * p.scale, fade(CYAN, 0.25), 0.7);
    }
    p.ring(
        ring_x,
        765.,
        15.,
        charge,
        3.,
        if ready { CYAN } else { fade(SOFT, 0.8) },
    );
    p.icon(
        Icon::Bolt,
        ring_x,
        765.,
        16.,
        if ready { CYAN } else { fade(SOFT, 0.7) },
    );

    // Bottom center: a key-cap prompt until fire and dash are learned, then
    // the weapon: a sword with its damage, level pips and the XP meter.
    if !learned.complete() || onboarding > 0. {
        let alpha = if learned.complete() {
            onboarding.min(1.)
        } else {
            1.
        };
        let theme = crate::hud::theme(p.scale, alpha);
        let breathe = if juice.motion() > 0. {
            0.75 + 0.25 * (juice.time * 4.).sin()
        } else {
            1.
        };
        let mut fire = [0; 64];
        let (keys, icon) = if !learned.fired {
            (key(input, "fire", &mut fire), Icon::Target)
        } else if !learned.dashed {
            (dash_key, Icon::Bolt)
        } else {
            ("", Icon::Check)
        };
        let key_w = if keys.is_empty() {
            0.
        } else {
            p.keycap_width(keys) + 10.
        };
        let x = 640. - (key_w + 24.) * 0.5;
        if !keys.is_empty() {
            p.keycap(x, 752., keys, &theme);
        }
        p.icon(
            icon,
            x + key_w + 12.,
            765.,
            22.,
            fade(if keys.is_empty() { CYAN } else { AMBER }, alpha * breathe),
        );
    } else {
        let level = game.progression.level();
        let damage = line!("{:.0}", game.shot_damage());
        p.icon(Icon::Sword, 556., 765., 18., SOFT);
        p.text(
            damage.as_str(),
            570.,
            773.,
            classic(16., PAPER).with_shadow(SHADOW),
        );
        let maxed = game.progression.next_cost().is_none();
        for pip in 1..=4_u16 {
            let c = p.point(616. + f32::from(pip - 1) * 14., 765.);
            let lit = pip <= level;
            let color = if !lit {
                Color::new(1., 1., 1., 0.12)
            } else if maxed {
                GOLD
            } else {
                CYAN
            };
            shapes::fill_circle(c, 4.5 * p.scale, color);
        }
        let fraction = match game.progression.next_cost() {
            Some(cost) => game.progression.xp() as f32 / cost as f32,
            None => 1.,
        };
        p.meter(
            Rect::new(674., 762., 70., 6.),
            fraction,
            None,
            if maxed { GOLD } else { CYAN },
            &theme,
        );
    }

    // Bottom right: a battery and twelve energy-cell pips that refill amber
    // while recharging.
    p.glass(1040., 740., 168., 50., 1.);
    let cells = game.magazine.loaded();
    let reloading = game.magazine.reloading();
    let battery = if reloading {
        let blink = if juice.motion() > 0. {
            0.6 + 0.4 * (juice.time * 10.).sin().abs()
        } else {
            1.
        };
        fade(AMBER, blink)
    } else if cells <= 3 {
        CORAL
    } else {
        CYAN
    };
    p.icon(Icon::Battery, 1066., 765., 22., battery);
    for cell in 0..12_u16 {
        let x = 1192. - (12 - cell) as f32 * 9. + 3.;
        let lit = !reloading && cell < cells;
        let color = if reloading {
            let fill = 1. - game.magazine.remaining() / 0.65;
            if (cell as f32) < fill * 12. {
                fade(AMBER, 0.8)
            } else {
                Color::new(1., 1., 1., 0.08)
            }
        } else if lit {
            CYAN
        } else {
            Color::new(1., 1., 1., 0.08)
        };
        let a = p.point(x, 752.);
        let b = p.point(x, 776.);
        shapes::fill_capsule(a, b, 2.6 * p.scale, color);
    }
}

/// What a wave banner's short second line shows.
enum Subline {
    /// Drones inbound, with a crown when the Overseer is among them.
    Inbound { drones: u32, overseer: bool },
    /// The next wave's name (or the final securing step) and a countdown ring.
    Next { name: &'static str, remaining: f32 },
}

/// Wave intro/clear banners in the arena's middle band, and the announcer
/// slot near the top. Drawn in the world layer before the HUD boundary, in
/// unshaken design space, so the glow blooms. A banner is one headline and
/// one short line; the announcer is one line with an optional badge.
pub(crate) fn announcements(p: &Painter, game: &Game, juice: &Juice, hud: &Hud, motion: Motion) {
    let banner = if let Some(remaining) = game.wave_clear_remaining() {
        let next = if game.wave == 3 {
            "SECURING SECTOR 07"
        } else {
            quips::wave_name(game.wave + 1)
        };
        let extra = juice
            .callout
            .as_ref()
            .filter(|c| c.priority >= Priority::Epic)
            .map(|c| (c.head, c.badge, c.color, c.alpha()));
        Some((
            Line::from("WAVE CLEAR"),
            Some(Icon::Check),
            Subline::Next {
                name: next,
                remaining,
            },
            CYAN,
            1.6 - remaining,
            1.,
            extra,
        ))
    } else if game.wave_banner > 0. {
        let drones = game.remaining_enemies();
        let overseer = game.wave == 3;
        let total = if game.wave == 1 { 2.7 } else { 2.4 };
        Some((
            Line::from(quips::wave_name(game.wave)),
            None,
            Subline::Inbound {
                drones: if overseer {
                    drones.saturating_sub(1)
                } else {
                    drones
                },
                overseer,
            },
            AMBER,
            total - game.wave_banner,
            (game.wave_banner / 0.4).min(1.),
            None,
        ))
    } else {
        None
    };
    if let Some((title, icon, sub, accent, elapsed, exit, extra)) = banner {
        let look = Entrance::pop(0.86)
            .with_duration(0.36)
            .with_ease(Ease::BackOut)
            .sample(elapsed, 0, motion);
        let alpha = look.alpha * exit;
        let rect = kit::appear(Rect::new(340., 342., 600., 104.), look);
        let r = p.rect(rect.x, rect.y, rect.w, rect.h);
        shapes::fill_rounded_rect(
            r,
            16. * p.scale,
            Fill::vertical(r, fade(GLASS_TOP, alpha), fade(GLASS_BOTTOM, alpha)),
        );
        shapes::stroke_rounded_rect(r, 16. * p.scale, 1.5, fade(accent, 0.55 * alpha));
        let c = rect.center();
        let s = look.scale;
        // Wave pips along the top edge replace "Wave 02 / 03".
        for w in 1..=3_u32 {
            let color = if w < game.wave || (w == game.wave && icon.is_some()) {
                CYAN
            } else if w == game.wave {
                AMBER
            } else {
                Color::new(1., 1., 1., 0.14)
            };
            let a = p.point(c.x - 44. + (w - 1) as f32 * 30., c.y - 34. * s);
            let b = a + vec2(20. * p.scale, 0.);
            shapes::fill_capsule(a, b, 2.5 * p.scale, fade(color, alpha));
        }
        let head = classic(40. * s, fade(PAPER, alpha)).with_outline(fade(INK, alpha));
        let head_w = p.width(title.as_str(), head) + if icon.is_some() { 44. } else { 0. };
        let x = c.x - head_w * 0.5;
        if let Some(icon) = icon {
            p.icon(icon, x + 16., c.y - 2. * s, 32. * s, fade(accent, alpha));
        }
        p.text(
            title.as_str(),
            x + if icon.is_some() { 44. } else { 0. },
            c.y + 12. * s,
            head,
        );
        let line = modern(16. * s, fade(CYAN, alpha));
        let y = c.y + 34. * s;
        match sub {
            Subline::Inbound { drones, overseer } => {
                let count = line!("{drones}");
                let w = 26. + p.width(count.as_str(), line) + if overseer { 52. } else { 0. };
                let x = c.x - w * 0.5;
                p.drone(x + 8., y, 16., fade(CORAL, alpha));
                p.text(count.as_str(), x + 22., line.centered_baseline(y), line);
                if overseer {
                    let plus = x + 26. + p.width(count.as_str(), line);
                    p.text("+", plus + 4., line.centered_baseline(y), line);
                    p.icon(Icon::Crown, plus + 30., y - 1., 20., fade(GOLD, alpha));
                }
            }
            Subline::Next { name, remaining } => {
                let w = 22. + p.width(name, line) + 30.;
                let x = c.x - w * 0.5;
                p.icon(Icon::ArrowRight, x + 8., y, 14., fade(CYAN, alpha));
                let tw = p.text(name, x + 22., line.centered_baseline(y), line);
                // The ring drains over the recovery interval.
                p.ring(
                    x + 22. + tw + 18.,
                    y,
                    7.,
                    remaining / 1.6,
                    2.,
                    fade(CYAN, alpha),
                );
            }
        }
        if let Some((head, badge, color, a)) = extra {
            // An earned epic callout rides the banner as a stamp.
            let a = a * alpha;
            let style = classic(16., fade(color, a)).with_outline(fade(INK, a));
            let badge_w = badge.map_or(0., |b| 30. + p.width(b.text.as_str(), style));
            let w = p.width(head.as_str(), style) + badge_w + 32.;
            let stamp = p.rect(c.x - w * 0.5, rect.y - 16., w, 28.);
            shapes::fill_rounded_rect(stamp, 14. * p.scale, fade(INK, 0.92 * a));
            shapes::stroke_rounded_rect(stamp, 14. * p.scale, 1.5, fade(color, 0.8 * a));
            let x = c.x - w * 0.5 + 16.;
            let tw = p.text(head.as_str(), x, rect.y + 4., style);
            if let Some(b) = badge {
                p.icon(b.icon, x + tw + 16., rect.y - 2., 14., fade(b.color, a));
                p.text(
                    b.text.as_str(),
                    x + tw + 26.,
                    rect.y + 4.,
                    style.with_color(fade(b.color, a)),
                );
            }
        }
        let _ = hud;
        return;
    }
    if let Some(c) = &juice.callout {
        let alpha = c.alpha();
        let size = 32. * c.scale(juice.motion());
        let head = classic(size, fade(c.color, alpha))
            .with_outline(fade(INK, alpha))
            .with_shadow(fade(Color::new(0., 0., 0., 0.6), alpha));
        let badge_style = classic(16., fade(PAPER, alpha)).with_outline(fade(INK, alpha));
        let head_w = p.width(c.head.as_str(), head);
        let badge_w = c
            .badge
            .map_or(0., |b| 32. + p.width(b.text.as_str(), badge_style));
        let x = 640. - (head_w + badge_w) * 0.5;
        let center = p.point(640., 180.);
        let width = (head_w + badge_w) * p.scale;
        post::with_glow(|| {
            shapes::fill_capsule(
                center - vec2(width * 0.42, 0.),
                center + vec2(width * 0.42, 0.),
                18. * p.scale,
                fade(c.color, 0.07 * alpha),
            );
        });
        p.text(c.head.as_str(), x, 192., head);
        if let Some(b) = c.badge {
            // A pictogram and a number beside the callout, never a sentence.
            let bx = x + head_w + 12.;
            p.icon(b.icon, bx + 9., 180., 18., fade(b.color, alpha));
            p.text(
                b.text.as_str(),
                bx + 20.,
                186.,
                badge_style.with_color(fade(b.color, alpha)),
            );
        }
    }
}

// ---------------------------------------------------------------- cards ----

/// Submits menu input to `Ui`: the contract card panel (hidden beneath the
/// kit card) and an invisible primary button at the authored action rect.
pub(crate) fn menu_input(ui: &mut Ui, card: UiRect, action: UiRect) {
    // Inset so the kit card's rounded corners fully cover the square panel.
    let inset = UiRect {
        min: [card.min[0] + 12., card.min[1] + 12.],
        size: [(card.size[0] - 24.).max(1.), (card.size[1] - 24.).max(1.)],
    };
    ui.panel(inset, CARD).unwrap();
    ui.button(1, action, "", true, kit::INVISIBLE_BUTTON)
        .unwrap();
}

pub(crate) const READY_CARD: UiRect = UiRect {
    min: [392., 282.],
    size: [496., 296.],
};
pub(crate) const PAUSE_CARD: UiRect = UiRect {
    min: [400., 244.],
    size: [480., 312.],
};
pub(crate) const RESULTS_CARD: UiRect = UiRect {
    min: [300., 110.],
    size: [680., 458.],
};

fn card_panel(
    p: &Painter,
    rect: UiRect,
    accent: Color,
    look: ferravex_play::tween::Appearance,
) -> Rect {
    let base = p.ui(rect);
    let r = kit::appear(base, look);
    let theme = theme(p.scale, look.alpha);
    kit::panel(r, &theme);
    // Neon top edge with a soft glow.
    let edge = Rect::new(r.x + 24. * p.scale, r.y, r.w - 48. * p.scale, 3. * p.scale);
    shapes::fill_rounded_rect(edge, edge.h * 0.5, fade(accent, look.alpha));
    shapes::drop_shadow(
        edge,
        edge.h,
        Shadow::soft(fade(accent, 0.5 * look.alpha)).blur(16. * p.scale),
    );
    r
}

/// Animated logo for the front door: per-letter bob, outline and a halo.
fn logo(p: &Painter, x: f32, y: f32, size: f32, time: f32, motion: Motion) {
    const TITLE: &str = "NEON SWARM";
    let style = classic(size, CYAN)
        .with_outline(INK)
        .with_shadow(Color::new(0., 0., 0., 0.7));
    let width = p.width(TITLE, style);
    let moving = !motion.is_reduced();
    let halo = p.point(x, y - size * 0.35);
    let breathe = if moving {
        0.85 + 0.15 * (time * 2.).sin()
    } else {
        1.
    };
    post::glow_circle(
        halo.x,
        halo.y,
        width * 0.62 * p.scale,
        fade(CYAN, 0.35),
        0.55 * breathe,
    );
    let mut cursor = x - width * 0.5;
    for (i, c) in TITLE.char_indices() {
        let mut buf = [0; 4];
        let glyph = c.encode_utf8(&mut buf);
        let bob = if moving {
            (time * 3.2 - i as f32 * 0.45).sin() * size * 0.06
        } else {
            0.
        };
        let color = if i >= 5 { MAGENTA } else { CYAN };
        let w = p.text(glyph, cursor, y + bob, style.with_color(color));
        cursor += w + style.pixel() * 1.;
        let _ = buf;
    }
}

/// A key cap followed by a pictogram (`None` draws the four-way move
/// arrows), at a layout point; returns the width.
fn key_icon(p: &Painter, x: f32, y: f32, keys: &str, icon: Option<Icon>, theme: &KitTheme) -> f32 {
    let w = p.keycap(x, y, keys, theme);
    let color = fade(SOFT, theme.alpha);
    match icon {
        Some(icon) => p.icon(icon, x + w + 16., y + 12., 20., color),
        None => move_glyph(p, x + w + 16., y + 12., 20., color),
    }
    w + 30.
}

/// Four outward arrowheads: move in any direction.
pub(crate) fn move_glyph(p: &Painter, x: f32, y: f32, size: f32, color: Color) {
    let c = p.point(x, y);
    let u = size * 0.5 * p.scale;
    for d in [vec2(0., -1.), vec2(1., 0.), vec2(0., 1.), vec2(-1., 0.)] {
        let side = vec2(-d.y, d.x);
        let tip = c + d * u;
        let base = c + d * (u * 0.45);
        shapes::fill_triangle(
            tip,
            base + side * (u * 0.42),
            base - side * (u * 0.42),
            color,
        );
    }
    shapes::fill_circle(c, u * 0.16, color);
}

/// Width of [`key_icon`] without drawing.
fn key_icon_width(p: &Painter, keys: &str) -> f32 {
    p.keycap_width(keys) + 30.
}

/// A centered row of pictogram + number pairs.
fn icon_stats(p: &Painter, items: &[(Icon, Line, Color)], y: f32, alpha: f32) {
    let style = |c: Color| modern(16., fade(c, alpha));
    let item = |text: &Line| 24. + p.width(text.as_str(), style(PAPER));
    let gap = 22.;
    let total: f32 =
        items.iter().map(|(_, t, _)| item(t)).sum::<f32>() + gap * (items.len().max(1) - 1) as f32;
    let mut x = 640. - total * 0.5;
    for (icon, text, color) in items {
        p.icon(*icon, x + 8., y, 17., fade(*color, alpha));
        p.text(
            text.as_str(),
            x + 22.,
            style(PAPER).centered_baseline(y),
            style(PAPER),
        );
        x += item(text) + gap;
    }
}

pub(crate) fn ready(
    p: &Painter,
    input: &PlayerInput,
    best: Option<(u64, u64)>,
    hud: &Hud,
    motion: Motion,
) {
    let t = hud.time;
    logo(p, 640., 196., 64., t, motion);
    let tag = hud.look(1, motion);
    p.text(
        "Three waves. One hexagon. Zero chill.",
        640.,
        244. + tag.offset[1],
        centered(modern(16., fade(PAPER, tag.alpha)).with_shadow(fade(SHADOW, tag.alpha))),
    );
    let look = hud.look(2, motion);
    card_panel(p, READY_CARD, CYAN, look);
    let dy = look.offset[1];
    // Controls as key caps beside pictograms: move, aim + fire, dash.
    let mut buffer = [0; 64];
    let moves = movement_hint(input);
    let fire = Line::from(key(input, "fire", &mut buffer));
    let dash = Line::from(key(input, "boost", &mut [0; 64]));
    let row = [
        (moves.as_str(), None),
        (fire.as_str(), Some(Icon::Target)),
        (dash.as_str(), Some(Icon::Bolt)),
    ];
    let gap = 26.;
    let total: f32 = row
        .iter()
        .map(|(keys, _)| key_icon_width(p, keys))
        .sum::<f32>()
        + gap * 2.;
    let mut x = 640. - total * 0.5;
    for (i, (keys, icon)) in row.into_iter().enumerate() {
        let look = hud.look(3 + i, motion);
        let theme = crate::hud::theme(p.scale, look.alpha);
        x += key_icon(p, x, 304. + dy + look.offset[1], keys, icon, &theme) + gap;
    }

    // Two pictogram lessons: dash through an amber bite; chain kills to ×5.
    let tips = hud.look(6, motion);
    let (ta, ty) = (tips.alpha, 372. + dy + tips.offset[1]);
    let bite = p.point(520., ty);
    shapes::fill_circle(bite, 18. * p.scale, fade(AMBER, 0.12 * ta));
    p.ring(520., ty, 18., 0.7, 3., fade(AMBER, ta));
    let badge = p.point(520., ty - 26.);
    shapes::fill_circle(badge, 8. * p.scale, fade(AMBER, ta));
    p.text("!", 520., ty - 21., centered(classic(16., fade(INK, ta))));
    p.icon(Icon::ArrowRight, 556., ty, 16., fade(SOFT, ta));
    p.icon(Icon::Bolt, 590., ty, 24., fade(CYAN, ta));
    for i in 0..3 {
        p.drone(688. + i as f32 * 22., ty, 16., fade(CORAL, ta));
    }
    p.icon(Icon::ArrowRight, 762., ty, 16., fade(SOFT, ta));
    p.text(
        "×5",
        782.,
        ty + 8.,
        classic(24., fade(MAGENTA, ta)).with_shadow(fade(SHADOW, ta)),
    );

    let record = hud.look(7, motion);
    let ry = 430. + dy + record.offset[1];
    match best {
        Some((score, runs)) => Chip::new(Icon::Trophy, line!("{score}"), GOLD)
            .with_value(line!("· {runs}"), SOFT)
            .draw(p, 640., ry, None, record.alpha),
        None => {
            Chip::new(Icon::Trophy, Line::from("—"), SOFT).draw(p, 640., ry, None, record.alpha)
        }
    }
    let button = hud.look(8, motion);
    let theme_b = crate::hud::theme(p.scale, button.alpha);
    let confirm = key(input, "confirm", &mut buffer);
    p.key_button(ACTION, confirm, Icon::Play, "Start", &hud.button, &theme_b);
}

pub(crate) fn paused(
    p: &Painter,
    game: &Game,
    input: &PlayerInput,
    pauses: u32,
    hud: &Hud,
    motion: Motion,
) {
    let look = hud.look(0, motion);
    card_panel(p, PAUSE_CARD, AMBER, look);
    let a = look.alpha;
    let dy = look.offset[1];
    p.text(
        "PAUSED",
        640.,
        300. + dy,
        centered(classic(40., fade(PAPER, a)).with_outline(fade(INK, a))),
    );
    let q = hud.look(1, motion);
    p.text(
        quips::pause(pauses),
        640.,
        330. + dy + q.offset[1],
        centered(modern(16., fade(CYAN, q.alpha))),
    );
    // The run so far as pictograms: wave, score, chain, kills and style.
    let s = hud.look(2, motion);
    icon_stats(
        p,
        &[
            (Icon::Flag, line!("{}/3", game.wave), CYAN),
            (Icon::Star, line!("{}", game.score.total()), GOLD),
            (
                Icon::Flame,
                line!("×{}", game.score.multiplier()),
                tier_color(game.score.multiplier()),
            ),
            (Icon::Skull, line!("{}", game.kills), CORAL),
            (
                Icon::Gem,
                line!("{}", game.style.points),
                crate::juice::VIOLET,
            ),
        ],
        380. + dy + s.offset[1],
        s.alpha,
    );
    let k = hud.look(3, motion);
    let mut buffer = [0; 64];
    let restart = key(input, "restart", &mut buffer);
    let theme = theme(p.scale, k.alpha);
    let w = key_icon_width(p, restart);
    key_icon(
        p,
        640. - w * 0.5,
        418. + dy + k.offset[1],
        restart,
        Some(Icon::Restart),
        &theme,
    );
    let b = hud.look(4, motion);
    let mut resume = [0; 64];
    let pause_key = key(input, "pause", &mut resume);
    p.key_button(
        ACTION,
        pause_key,
        Icon::Play,
        "Resume",
        &hud.button,
        &crate::hud::theme(p.scale, b.alpha),
    );
}

pub(crate) fn results(
    p: &Painter,
    results: &Results,
    input: &PlayerInput,
    hud: &Hud,
    motion: Motion,
) {
    let won = results.summary.won;
    let accent = if won { CYAN } else { CORAL };
    let look = hud.look(0, motion);
    card_panel(p, RESULTS_CARD, accent, look);
    let dy = look.offset[1];
    let a = look.alpha;
    p.text(
        if won { "SECTOR SECURED" } else { "SIGNAL LOST" },
        640.,
        170. + dy,
        centered(
            classic(40., fade(accent, a))
                .with_outline(fade(INK, a))
                .with_shadow(fade(SHADOW, a)),
        ),
    );
    // The rank and its one-line joke.
    let r = hud.look(1, motion);
    let ra = r.alpha;
    let ry = dy + r.offset[1];
    p.text(
        results.rank.title,
        640.,
        212. + ry,
        centered(modern(24., fade(GOLD, ra)).with_shadow(fade(SHADOW, ra))),
    );
    p.text(
        results.rank.quip,
        640.,
        238. + ry,
        centered(modern(16., fade(PAPER, ra))),
    );
    // Four pictogram tiles: score, time or wave reached, best chain or kills, style.
    let s = results.summary;
    let icons: [Icon; 4] = if won {
        [Icon::Star, Icon::Clock, Icon::Flame, Icon::Gem]
    } else {
        [Icon::Star, Icon::Flag, Icon::Skull, Icon::Gem]
    };
    for (i, icon) in icons.into_iter().enumerate() {
        let tile = hud.look(2 + i, motion);
        let x = 324. + i as f32 * 160.;
        let y = 264. + dy + tile.offset[1];
        let r = p.rect(x, y, 152., 70.);
        shapes::fill_rounded_rect(
            r,
            10. * p.scale,
            Fill::vertical(
                r,
                Color::new(0.09, 0.17, 0.2, 0.95 * tile.alpha),
                Color::new(0.05, 0.1, 0.13, 0.95 * tile.alpha),
            ),
        );
        shapes::stroke_rounded_rect(r, 10. * p.scale, 1., fade(EDGE, tile.alpha));
        let color = [GOLD, CYAN, AMBER, crate::juice::VIOLET][i];
        p.icon(icon, x + 76., y + 20., 20., fade(color, tile.alpha));
        let shown = hud.stats[i].shown();
        let value = match (i, won) {
            (1, true) => line!("{shown}s"),
            (1, false) => line!("{shown}/3"),
            _ => line!("{shown}"),
        };
        p.text(
            value.as_str(),
            x + 76.,
            y + 56.,
            centered(classic(24., fade(PAPER, tile.alpha)).with_shadow(fade(SHADOW, tile.alpha))),
        );
    }
    // Earned style feats as pictograms, then the record and the next goal.
    let l = hud.look(6, motion);
    let ly = dy + l.offset[1];
    let mut feats = [(Icon::Star, Line::new(), PAPER); 5];
    let mut count = 0;
    for (icon, text) in style_feats(&s.style) {
        feats[count] = (icon, text, crate::juice::VIOLET);
        count += 1;
    }
    if count > 0 {
        icon_stats(p, &feats[..count], 366. + ly, l.alpha);
    }
    let record = results.record_chip();
    let best = results.new_best();
    if best && l.alpha > 0. {
        let c = p.point(640., 414. + ly);
        let pulse = if motion.is_reduced() {
            1.
        } else {
            0.8 + 0.2 * (hud.time * 4.).sin()
        };
        post::glow_circle(c.x, c.y, 150. * p.scale, fade(GOLD, 0.22 * l.alpha), pulse);
    }
    let (icon, goal) = quips::next_goal(s);
    let next = Chip::new(icon, Line::from(goal), CYAN);
    let (record_w, next_w) = (record.width(p, false), next.width(p, true));
    let gap = 16.;
    let start = 640. - (record_w + gap + next_w) * 0.5;
    record.draw(p, start + record_w * 0.5, 414. + ly, None, l.alpha);
    next.draw(
        p,
        start + record_w + gap + next_w * 0.5,
        414. + ly,
        Some(Icon::Target),
        l.alpha,
    );
    let b = hud.look(7, motion);
    let mut buffer = [0; 64];
    let confirm = key(input, "confirm", &mut buffer);
    p.key_button(
        ACTION,
        confirm,
        Icon::Restart,
        "Play again",
        &hud.button,
        &theme(p.scale, b.alpha),
    );
}

/// Stroke helper kept for the dash ring and similar arcs.
#[allow(dead_code)]
pub(crate) fn arc(p: &Painter, x: f32, y: f32, r: f32, fraction: f32, color: Color) {
    shapes::stroke_arc(
        p.point(x, y),
        r * p.scale,
        -std::f32::consts::FRAC_PI_2,
        std::f32::consts::TAU * fraction.clamp(0., 1.),
        Stroke::new(3. * p.scale, color).cap(Cap::Round),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Phase, Style};

    #[test]
    fn records_lines_and_numbers_read_cleanly() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(12_345), "12345");
        let mut game = Game::new();
        game.phase = Phase::Lost;
        let none = Results::new(&game, None, 0);
        assert!(!none.new_best(), "a zero score is never a best");
        assert_eq!(none.record_chip().color, SOFT);
        game.score.award(100);
        let first = Results::new(&game, None, 0);
        assert!(first.new_best());
        assert_eq!(first.record_chip().text, "FIRST RECORD");
        let chase = Results::new(&game, Some(400), 1);
        assert!(!chase.new_best());
        let gap = chase.record_chip();
        assert_eq!((gap.text.as_str(), gap.value.as_str()), ("400", "−300"));
        assert_eq!(gap.value_color, CORAL);
        let beat = Results::new(&game, Some(40), 2);
        assert!(beat.new_best());
        let best = beat.record_chip();
        assert_eq!(
            (best.text.as_str(), best.value.as_str()),
            ("NEW BEST", "+60")
        );
        assert_eq!(Results::new(&game, Some(100), 3).record_chip().text, "TIED");
        for chip in [none, first, chase, beat].map(|r| r.record_chip()) {
            assert_eq!(chip.icon, Icon::Trophy);
            assert!(chip.text.as_str().split_whitespace().count() <= 3);
        }
        assert_eq!(beat.rank.title, "Welcome Mat");
    }

    #[test]
    fn style_feats_list_only_earned_feats_and_lines_truncate_safely() {
        assert_eq!(style_feats(&Style::default()).count(), 0);
        let style = Style {
            drive_bys: 1,
            close_shaves: 3,
            beacons: 0b0101,
            ..Style::default()
        };
        let feats: Vec<_> = style_feats(&style)
            .map(|(icon, text)| (icon, text.as_str().to_owned()))
            .collect();
        assert_eq!(
            feats,
            [
                (Icon::Bolt, "1".to_owned()),
                (Icon::Wind, "3".to_owned()),
                (Icon::Bulb, "2/4".to_owned()),
            ]
        );
        let learned = Learned {
            fired: true,
            dashed: false,
        };
        assert!(!learned.complete());
        let long = Line::from(&"é".repeat(200));
        assert!(long.as_str().chars().all(|c| c == 'é'));
        assert!(long.as_str().len() <= 112);
    }

    #[test]
    fn menu_motion_rolls_numbers_tracks_damage_and_opens_with_an_iris() {
        let mut hud = Hud::new();
        let mut game = Game::new();
        hud.update(Screen::Playing, &game, None, 1. / 60., Motion::Full);
        assert_ne!(
            hud.transition.phase(),
            TransitionPhase::Idle,
            "iris opens the run"
        );
        for _ in 0..60 {
            hud.update(Screen::Playing, &game, None, 1. / 60., Motion::Full);
        }
        assert_eq!(hud.transition.phase(), TransitionPhase::Idle);
        game.score.award(100);
        hud.update(Screen::Playing, &game, None, 1. / 60., Motion::Full);
        assert!(hud.score.shown() < 100, "the score rolls up");
        hud.update(Screen::Playing, &game, None, 1. / 60., Motion::Reduced);
        assert_eq!(hud.score.shown(), 100, "reduced motion snaps");
        game.health = 40;
        hud.update(Screen::Playing, &game, None, 0.1, Motion::Full);
        assert_eq!(hud.ghost(), 1., "the ghost holds after a hit");
        for _ in 0..120 {
            hud.update(Screen::Playing, &game, None, 1. / 60., Motion::Full);
        }
        assert_eq!(hud.ghost(), 0.4, "then drains to the new health");
        game.phase = Phase::Won;
        let results = Results::new(&game, None, 0);
        hud.update(Screen::Results, &game, Some(&results), 0., Motion::Full);
        assert_eq!(hud.stats[0].shown(), 0);
        for _ in 0..240 {
            hud.update(
                Screen::Results,
                &game,
                Some(&results),
                1. / 60.,
                Motion::Full,
            );
        }
        assert_eq!(hud.stats[0].shown(), 100);
        assert_eq!(hud.look(3, Motion::Full).alpha, 1.);
    }
}
