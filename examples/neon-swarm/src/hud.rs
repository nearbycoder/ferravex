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

    /// The record line: celebrate a best, otherwise show the gap to beat.
    pub fn record_line(&self) -> (Line, Color) {
        let score = self.summary.score;
        match self.previous_best {
            _ if score == 0 => (Line::from("Score a kill to set a record"), SOFT),
            None => (Line::from("First record set! Now beat it."), GOLD),
            Some(best) if score > best => (line!("New personal best! +{}", score - best), GOLD),
            Some(best) if score == best => (Line::from("Tied your best. So close."), AMBER),
            Some(best) => (line!("Best {best} · {} to beat it", best - score), PAPER),
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

/// Nonzero style feats as one short line, for results and pause.
pub(crate) fn style_line(style: &Style) -> Line {
    let mut out = Line::new();
    let mut part = |n: u32, one: &str, many: &str| {
        if n > 0 {
            let sep = if out.len > 0 { " · " } else { "" };
            let _ = write!(out, "{sep}{n} {}", if n == 1 { one } else { many });
        }
    };
    part(style.drive_bys, "drive-by", "drive-bys");
    part(style.close_shaves, "close shave", "close shaves");
    part(style.multi_kills, "multi-kill", "multi-kills");
    part(style.untouched_waves, "clean wave", "clean waves");
    if style.beacons_lit() > 0 {
        let sep = if out.len > 0 { " · " } else { "" };
        let _ = write!(out, "{sep}lamps {}/4", style.beacons_lit());
    }
    if out.len == 0 {
        Line::from("No style yet: dash through attacks, chain kills")
    } else {
        out
    }
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

/// The in-play HUD for the wide layout.
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

    // Left: score and the live chain.
    p.glass(64., 14., 300., 78., 1.);
    p.text("Score", 80., 36., modern(16., SOFT));
    let score = line!("{}", hud.score.shown().max(0));
    p.text(
        score.as_str(),
        80.,
        74.,
        classic(32., PAPER).with_shadow(SHADOW),
    );
    if chain > 0 {
        let label = line!("×{}", game.score.multiplier());
        p.text(
            label.as_str(),
            348.,
            66.,
            right(classic(32., tier).with_shadow(SHADOW)),
        );
        let count = line!("chain {chain}");
        p.text(count.as_str(), 348., 36., right(modern(16., tier)));
        let life = game.score.remaining_life().unwrap_or(0.) as f32 / 3.5;
        p.meter(Rect::new(244., 76., 104., 6.), life, None, tier, &theme);
    } else {
        p.text("Kill to chain", 348., 36., right(modern(16., SOFT)));
    }

    // Center: the wave, its name and what is left of it.
    let name = quips::wave_name(game.wave);
    let wave = line!("Wave {}/3 · {name}", game.wave);
    p.text(wave.as_str(), 640., 34., centered(modern(16., CYAN)));
    let left = game.remaining_enemies();
    let headline = if game.final_drone() {
        Line::from("Last drone standing")
    } else if left == 0 {
        Line::from("Wave clear")
    } else {
        line!("{left} drone{} left", if left == 1 { "" } else { "s" })
    };
    p.text(
        headline.as_str(),
        640.,
        66.,
        centered(modern(24., PAPER).with_shadow(SHADOW)),
    );
    for w in 1..=3_u32 {
        let color = if w < game.wave {
            CYAN
        } else if w == game.wave {
            AMBER
        } else {
            Color::new(1., 1., 1., 0.12)
        };
        let a = p.point(604. + (w - 1) as f32 * 30., 80.);
        let b = a + vec2(20. * p.scale, 0.);
        shapes::fill_capsule(a, b, 2.5 * p.scale, color);
    }
    if game.wave_clear_remaining().is_none() {
        let (admission, position) = game.spawn_preview();
        let held = match admission {
            AdmissionPreview::AliveLimit { .. } => Some("Entry held · arena full"),
            _ if position.is_none() && admission.next_available_in().is_some() => {
                Some("Entry held · no safe spot")
            }
            _ => None,
        };
        if let Some(held) = held {
            p.text(held, 640., 94., centered(modern(8., AMBER)));
        }
    }

    // Right: health and shield. Copy ends at x 1192, clear of the corner.
    p.glass(912., 14., 296., 78., 1.);
    let low = game.health <= 30;
    p.text("Health", 928., 36., modern(16., SOFT));
    let shield_value = game.shield.value();
    let shield = line!("Shield {}", shield_value.ceil() as u32);
    p.text(
        shield.as_str(),
        1096.,
        36.,
        right(modern(16., if shield_value > 0. { SHIELD } else { SOFT })),
    );
    let pulse = if low && juice.motion() > 0. {
        0.75 + 0.25 * (juice.time * 6.).sin()
    } else {
        1.
    };
    p.meter(
        Rect::new(928., 46., 168., 12.),
        game.health as f32 / 100.,
        Some(hud.ghost()),
        if low { fade(CORAL, pulse) } else { CYAN },
        &theme,
    );
    p.meter(
        Rect::new(928., 64., 168., 7.),
        shield_value / game.shield.maximum(),
        None,
        SHIELD,
        &theme,
    );
    let health = line!("{}", game.health);
    p.text(
        health.as_str(),
        1192.,
        74.,
        right(classic(32., if low { CORAL } else { PAPER }).with_shadow(SHADOW)),
    );

    // Bottom: dash on the left, weapon in the middle, cells on the right.
    let dash = game.dash_availability();
    let ready = dash.check().is_ok();
    p.glass(64., 740., 240., 50., 1.);
    let mut buffer = [0; 64];
    let dash_key = key(input, "boost", &mut buffer);
    let cap = kit::keycap(p.point(76., 750.).x, p.point(76., 750.).y, dash_key, &theme);
    let after = 76. + cap.w / p.scale + 8.;
    p.text(
        if ready { "Dash ready" } else { "Dash" },
        after,
        764.,
        modern(16., if ready { CYAN } else { SOFT }),
    );
    p.meter(
        Rect::new(after, 772., 288. - after, 6.),
        1. - game.dash_cooldown / 1.7,
        None,
        if ready { CYAN } else { SOFT },
        &theme,
    );
    if ready && juice.motion() > 0. {
        let glow = p.point(after + (288. - after) * 0.5, 775.);
        post::glow_circle(glow.x, glow.y, 30. * p.scale, fade(CYAN, 0.25), 0.7);
    }

    if !learned.complete() || onboarding > 0. {
        let alpha = if learned.complete() {
            onboarding.min(1.)
        } else {
            1.
        };
        let theme = crate::hud::theme(p.scale, alpha);
        let (keys, action) = if !learned.fired {
            (
                key(input, "fire", &mut [0; 64]).to_owned_line(),
                "Hold to aim and fire",
            )
        } else if !learned.dashed {
            (Line::from(dash_key), "Dash: untouchable mid-dash")
        } else {
            (Line::new(), "Nice. Now chain those kills.")
        };
        let width = if keys.as_str().is_empty() {
            p.width(action, modern(16., PAPER))
        } else {
            p.width(keys.as_str(), modern(16., PAPER)) + 34. + p.width(action, modern(16., PAPER))
        };
        let x = 640. - width * 0.5;
        if keys.as_str().is_empty() {
            p.text(action, x, 766., modern(16., fade(AMBER, alpha)));
        } else {
            let at = p.point(x, 748.);
            kit::key_prompt(at.x, at.y, &[keys.as_str()], action, &theme);
        }
    } else {
        let level = game.progression.level();
        let (fraction, label) = match game.progression.next_cost() {
            Some(cost) => (
                game.progression.xp() as f32 / cost as f32,
                line!("Weapon Lv {level} · {} dmg", game.shot_damage()),
            ),
            None => (
                1.,
                line!("Weapon Lv {level} max · {} dmg", game.shot_damage()),
            ),
        };
        p.text(label.as_str(), 640., 760., centered(modern(16., SOFT)));
        p.meter(
            Rect::new(540., 770., 200., 6.),
            fraction,
            None,
            CYAN,
            &theme,
        );
    }

    p.glass(944., 740., 264., 50., 1.);
    let cells = game.magazine.loaded();
    let reloading = game.magazine.reloading();
    let status = if reloading {
        line!("Recharging {:.1}s", game.magazine.remaining())
    } else {
        line!("{cells}/12 cells")
    };
    p.text(
        status.as_str(),
        960.,
        770.,
        modern(16., if reloading { AMBER } else { SOFT }),
    );
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

/// Wave intro/clear banners in the arena's middle band, and the announcer
/// slot near the top. Drawn in the world layer before the HUD boundary, in
/// unshaken design space, so the glow blooms.
pub(crate) fn announcements(p: &Painter, game: &Game, juice: &Juice, hud: &Hud, motion: Motion) {
    let banner = if let Some(remaining) = game.wave_clear_remaining() {
        let title = line!("Wave {:02} clear", game.wave);
        let sub = if game.wave == 3 {
            line!("Securing Sector 07 · {remaining:.1}s")
        } else {
            line!(
                "Next up: {} · {remaining:.1}s",
                quips::wave_name(game.wave + 1)
            )
        };
        let extra = juice
            .callout
            .as_ref()
            .filter(|c| c.priority >= Priority::Epic)
            .map(|c| (c.head, c.color, c.alpha()));
        Some((
            Line::from("All clear"),
            title,
            sub,
            CYAN,
            1.6 - remaining,
            1.,
            extra,
        ))
    } else if game.wave_banner > 0. {
        let top = line!("Wave {:02} / 03", game.wave);
        let sub = if game.wave == 3 {
            line!(
                "{} drones + one very large problem",
                game.remaining_enemies().saturating_sub(1)
            )
        } else {
            line!("{} drones inbound", game.remaining_enemies())
        };
        let total = if game.wave == 1 { 2.7 } else { 2.4 };
        Some((
            top,
            Line::from(quips::wave_name(game.wave)),
            sub,
            AMBER,
            total - game.wave_banner,
            (game.wave_banner / 0.4).min(1.),
            None,
        ))
    } else {
        None
    };
    if let Some((top, title, sub, accent, elapsed, exit, extra)) = banner {
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
        p.text(
            top.as_str(),
            c.x,
            c.y - 26. * s,
            centered(modern(16. * s, fade(accent, alpha))),
        );
        p.text(
            title.as_str(),
            c.x,
            c.y + 12. * s,
            centered(classic(40. * s, fade(PAPER, alpha)).with_outline(fade(INK, alpha))),
        );
        p.text(
            sub.as_str(),
            c.x,
            c.y + 38. * s,
            centered(modern(16. * s, fade(CYAN, alpha))),
        );
        if let Some((head, color, a)) = extra {
            p.text(
                head.as_str(),
                c.x,
                c.y + 72.,
                centered(classic(16., fade(color, a * alpha)).with_outline(fade(INK, a * alpha))),
            );
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
        let center = p.point(640., 180.);
        let width = p.width(c.head.as_str(), head) * p.scale;
        post::with_glow(|| {
            shapes::fill_capsule(
                center - vec2(width * 0.42, 0.),
                center + vec2(width * 0.42, 0.),
                18. * p.scale,
                fade(c.color, 0.07 * alpha),
            );
        });
        p.text(c.head.as_str(), 640., 192., centered(head));
        if !c.sub.as_str().is_empty() {
            p.text(
                c.sub.as_str(),
                640.,
                218.,
                centered(modern(16., fade(PAPER, alpha * 0.92)).with_shadow(fade(SHADOW, alpha))),
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
    let card = card_panel(p, READY_CARD, CYAN, look);
    let theme = theme(p.scale, look.alpha);
    let dy = look.offset[1];
    // Controls as key caps, three columns.
    let mut buffer = [0; 64];
    let moves = movement_hint(input);
    let fire = Line::from(key(input, "fire", &mut buffer));
    let dash = Line::from(key(input, "boost", &mut [0; 64]));
    for (i, (keys, label)) in [
        (moves.as_str(), "Move"),
        (fire.as_str(), "Aim + fire"),
        (dash.as_str(), "Dash"),
    ]
    .into_iter()
    .enumerate()
    {
        let column = 408. + i as f32 * 158.;
        let row = hud.look(3 + i, motion);
        let theme = crate::hud::theme(p.scale, row.alpha);
        let at = p.point(column + 8., 302. + dy + row.offset[1]);
        kit::key_prompt(at.x, at.y, &[keys], label, &theme);
    }
    let tips = hud.look(6, motion);
    p.text(
        "Amber ring = incoming bite. Dash through it for style.",
        640.,
        368. + dy + tips.offset[1],
        centered(modern(16., fade(SOFT, tips.alpha))),
    );
    p.text(
        "Chain kills for up to ×5 points. A hit breaks the chain.",
        640.,
        392. + dy + tips.offset[1],
        centered(modern(16., fade(SOFT, tips.alpha))),
    );
    let record = hud.look(7, motion);
    match best {
        Some((score, runs)) => {
            let text = line!("Personal best {score} · {runs} runs");
            p.text(
                text.as_str(),
                640.,
                436. + dy + record.offset[1],
                centered(
                    modern(16., fade(GOLD, record.alpha)).with_shadow(fade(SHADOW, record.alpha)),
                ),
            );
        }
        None => {
            p.text(
                "No record yet. Go make one.",
                640.,
                436. + dy + record.offset[1],
                centered(modern(16., fade(SOFT, record.alpha))),
            );
        }
    }
    let _ = card;
    let button = hud.look(8, motion);
    let theme_b = crate::hud::theme(p.scale, button.alpha);
    let mut label = Line::from("Start");
    if !motion.is_reduced() {
        // Attract-mode pulse on the call to action.
        let _ = &mut label;
    }
    p.button(ACTION, label.as_str(), &hud.button, &theme_b);
    let confirm = key(input, "confirm", &mut buffer);
    let prompt = line!("Press {confirm} to start");
    let blink = if motion.is_reduced() {
        1.
    } else {
        0.55 + 0.45 * (t * 3.).sin().abs()
    };
    p.text(
        prompt.as_str(),
        640.,
        556.,
        centered(modern(16., fade(SOFT, blink * button.alpha))),
    );
    let _ = theme;
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
    let card = card_panel(p, PAUSE_CARD, AMBER, look);
    let a = look.alpha;
    let dy = look.offset[1];
    let _ = card;
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
    let s = hud.look(2, motion);
    let run = line!(
        "Wave {}/3 · {} pts · ×{} chain",
        game.wave,
        game.score.total(),
        game.score.multiplier()
    );
    p.text(
        run.as_str(),
        640.,
        372. + dy + s.offset[1],
        centered(modern(16., fade(PAPER, s.alpha))),
    );
    let kills = line!("{} kills · style {}", game.kills, game.style.points);
    p.text(
        kills.as_str(),
        640.,
        396. + dy + s.offset[1],
        centered(modern(16., fade(SOFT, s.alpha))),
    );
    let k = hud.look(3, motion);
    let mut buffer = [0; 64];
    let restart = key(input, "restart", &mut buffer);
    let theme = theme(p.scale, k.alpha);
    let w = p.width(restart, modern(16., PAPER)) + 30. + p.width("Restart", modern(16., PAPER));
    let at = p.point(640. - w * 0.5, 418. + dy + k.offset[1]);
    kit::key_prompt(at.x, at.y, &[restart], "Restart", &theme);
    let b = hud.look(4, motion);
    p.button(
        ACTION,
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
    let r = hud.look(1, motion);
    let ra = r.alpha;
    let ry = dy + r.offset[1];
    p.text(
        "Your rank",
        640.,
        200. + ry,
        centered(modern(16., fade(SOFT, ra))),
    );
    p.text(
        results.rank.title,
        640.,
        230. + ry,
        centered(modern(24., fade(GOLD, ra)).with_shadow(fade(SHADOW, ra))),
    );
    p.text(
        results.rank.quip,
        640.,
        254. + ry,
        centered(modern(16., fade(PAPER, ra))),
    );
    let s = results.summary;
    let names: [&str; 4] = if won {
        ["Score", "Time", "Best chain", "Style"]
    } else {
        ["Score", "Reached", "Kills", "Style"]
    };
    for (i, name) in names.iter().enumerate() {
        let tile = hud.look(2 + i, motion);
        let x = 324. + i as f32 * 160.;
        let y = 270. + dy + tile.offset[1];
        let r = p.rect(x, y, 152., 66.);
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
        let shown = hud.stats[i].shown();
        let value = match (i, won) {
            (1, true) => line!("{shown}s"),
            (1, false) => line!("Wave {shown}"),
            _ => line!("{shown}"),
        };
        p.text(
            value.as_str(),
            x + 76.,
            y + 36.,
            centered(classic(24., fade(PAPER, tile.alpha)).with_shadow(fade(SHADOW, tile.alpha))),
        );
        p.text(
            name,
            x + 76.,
            y + 56.,
            centered(modern(16., fade(SOFT, tile.alpha))),
        );
    }
    let l = hud.look(6, motion);
    let ly = dy + l.offset[1];
    let style = style_line(&s.style);
    p.text(
        style.as_str(),
        640.,
        364. + ly,
        centered(modern(16., fade(SOFT, l.alpha))),
    );
    let (record, color) = results.record_line();
    let best = results.new_best();
    if best && l.alpha > 0. {
        let c = p.point(640., 388. + ly);
        let pulse = if motion.is_reduced() {
            1.
        } else {
            0.8 + 0.2 * (hud.time * 4.).sin()
        };
        post::glow_circle(c.x, c.y, 150. * p.scale, fade(GOLD, 0.22 * l.alpha), pulse);
    }
    p.text(
        record.as_str(),
        640.,
        394. + ly,
        centered(modern(24., fade(color, l.alpha)).with_shadow(fade(SHADOW, l.alpha))),
    );
    p.text(
        quips::next_goal(s),
        640.,
        422. + ly,
        centered(modern(16., fade(CYAN, l.alpha))),
    );
    let quip = results.quip();
    p.text(
        quip.as_str(),
        640.,
        446. + ly,
        centered(modern(16., fade(SOFT, l.alpha))),
    );
    let b = hud.look(7, motion);
    p.button(ACTION, "Play again", &hud.button, &theme(p.scale, b.alpha));
    let mut buffer = [0; 64];
    let confirm = key(input, "confirm", &mut buffer);
    let theme = theme(p.scale, b.alpha);
    let w = p.width(confirm, modern(16., PAPER)) + 30. + p.width("Play again", modern(16., PAPER));
    let at = p.point(640. - w * 0.5, 530.);
    kit::key_prompt(at.x, at.y, &[confirm], "Play again", &theme);
}

trait ToLine {
    fn to_owned_line(&self) -> Line;
}

impl ToLine for &str {
    fn to_owned_line(&self) -> Line {
        Line::from(self)
    }
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
        assert_eq!(none.record_line().1, SOFT);
        game.score.award(100);
        let first = Results::new(&game, None, 0);
        assert!(first.new_best());
        assert!(first.record_line().0.as_str().starts_with("First record"));
        let chase = Results::new(&game, Some(400), 1);
        assert!(!chase.new_best());
        assert_eq!(chase.record_line().0, "Best 400 · 300 to beat it");
        let beat = Results::new(&game, Some(40), 2);
        assert!(beat.new_best());
        assert_eq!(beat.record_line().0, "New personal best! +60");
        assert_eq!(beat.rank.title, "Welcome Mat");
    }

    #[test]
    fn style_line_lists_only_earned_feats_and_lines_truncate_safely() {
        assert!(
            style_line(&Style::default())
                .as_str()
                .starts_with("No style")
        );
        let style = Style {
            drive_bys: 1,
            close_shaves: 3,
            beacons: 0b0101,
            ..Style::default()
        };
        assert_eq!(
            style_line(&style),
            "1 drive-by · 3 close shaves · lamps 2/4"
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
