//! Race HUD, countdown, title, pause and results screens in logical pixels.
//!
//! The play HUD keeps one job per corner on soft rounded glass plates: a flag
//! and the lap with a star and the best lap (top left), the clock with a clock
//! icon, the lap time and the medal the pace projects (top center), a gate
//! arch, the next-gate count and this lap's threading pips (top right), a
//! speed gauge (bottom left) and the boost meter (bottom right). Words are
//! reserved for names, numbers and callouts of at most three words: traction,
//! boost state, alerts and onboarding are icons, colour and key caps. Copy
//! uses the mixed-case Modern face; big numerals and arcade callouts keep the
//! Classic face for retro punch. One radio toast, one banner and one alert
//! slot keep messages from stacking.
//!
//! Browser contract (web/tests/wasm-ui.mjs): narrow layouts draw the white
//! 14px lap and 20px timer in the top 32px, and wide layouts draw a 14px teal
//! next-gate count above y=52, all through the classic per-pixel glyph path;
//! every readable element stays out of the host's top-right menu corner.
use super::fmtbuf::{Buf, buf, clock};
use super::juice::{Detail, GOLD, Juice, MINT, ROSE, signed};
use super::presentation::{binding_hint, text_width as classic_width};
use super::quips::{self, RunSummary};
use super::race::{BoostStatus, GATES, LAPS, MULTIPLIER_CAP, Race, Traction};
use super::{ORANGE, SHORT_START_ROW, TEAL, label};
use ferravex_play::gameplay::trial::Medal;
use ferravex_play::tween::{Ease, Entrance, Motion, RollCounter, Spring};
use ferravex_render::icons::{self, Icon, IconStyle};
use ferravex_render::input::PlayerInput;
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Cap, Fill, Shadow, Stroke};
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_render::ui::kit::{self, ButtonKind, ButtonMotion, ButtonState, KitTheme};
use std::f32::consts::PI;

pub(super) const INK: Color = Color::new(0.98, 0.95, 0.89, 1.0);
pub(super) const MUTED: Color = Color::new(0.74, 0.71, 0.64, 1.0);
pub(super) const AQUA: Color = Color::new(0.45, 0.90, 0.82, 1.0);
const SHADOW: Color = Color::new(0.0, 0.0, 0.0, 0.5);
const OUTLINE: Color = Color::new(0.10, 0.06, 0.03, 0.9);
const GLASS_TOP: Color = Color::new(0.14, 0.17, 0.19, 0.80);
const GLASS_BOTTOM: Color = Color::new(0.06, 0.08, 0.10, 0.86);
const LINE: Color = Color::new(1.0, 1.0, 1.0, 0.10);
const TRACK: Color = Color::new(0.03, 0.05, 0.06, 0.85);
const SILVER: Color = Color::new(0.84, 0.88, 0.90, 1.0);
const BRONZE: Color = Color::new(0.90, 0.60, 0.36, 1.0);
const RADIUS: f32 = 12.;

/// Result of a finished race against stored records, captured once at the finish.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Results {
    /// Fastest stored race before this one, in seconds.
    pub previous_best: Option<f32>,
    /// Fastest stored lap before this race, in seconds.
    pub previous_lap: Option<f32>,
    /// Autoplay demonstrations are shown but never recorded.
    pub demo: bool,
}

impl Results {
    pub fn new_best(&self, time: f32) -> bool {
        !self.demo && self.previous_best.is_none_or(|best| time < best - 1e-4)
    }
    pub fn new_lap(&self, lap: Option<f32>) -> bool {
        !self.demo && lap.is_some_and(|lap| self.previous_lap.is_none_or(|best| lap < best - 1e-4))
    }
}

/// Presentation-only UI motion, advanced on frame time (menus) so cards and
/// buttons stay responsive while the race is paused or finished.
pub(super) struct UiMotion {
    /// Boost "ghost" that trails a drain, like a chip-damage bar.
    ghost: Spring,
    /// Displayed gauge fraction, smoothed so the needle never jitters.
    gauge: Spring,
    pub retry: ButtonMotion,
    /// Finish clock roll-up, in centiseconds.
    roll: RollCounter,
    pub pause_age: f32,
    pub title_age: f32,
    pub motion: Motion,
}

impl UiMotion {
    pub fn new() -> Self {
        Self {
            ghost: Spring::critical(1., 2.5),
            gauge: Spring::critical(0., 9.),
            retry: ButtonMotion::default(),
            roll: RollCounter::new(0),
            pause_age: 0.,
            title_age: 0.,
            motion: Motion::Full,
        }
    }
    pub fn reset(&mut self) {
        let motion = self.motion;
        let title_age = self.title_age;
        *self = Self::new();
        self.motion = motion;
        self.title_age = title_age;
    }
    pub fn update(&mut self, frame: f32, race: &Race, paused: bool, retry: ButtonState) {
        let dt = frame.clamp(0., 0.1);
        let motion = self.motion;
        if race.boost >= self.ghost.value {
            self.ghost.snap(race.boost);
        } else {
            self.ghost.set_target(race.boost);
            if !paused {
                self.ghost.advance(dt, motion);
            }
        }
        self.gauge.set_target((race.speed() / 44.).clamp(0., 1.));
        if !paused {
            self.gauge.advance(dt, motion);
        }
        self.pause_age = if paused { self.pause_age + dt } else { 0. };
        self.title_age += dt;
        if race.finished {
            let target = (race.elapsed * 100.).round() as i64;
            if self.roll.target() != target {
                self.roll.set_target(target);
            }
            self.roll.advance(dt, motion);
        } else if self.roll.target() != 0 {
            self.roll.snap(0);
        }
        self.retry.update(retry, dt, motion);
    }
}

/// Everything the HUD reads for one frame. Nothing here writes race state.
pub(super) struct Hud<'a> {
    pub race: &'a Race,
    pub input: &'a PlayerInput,
    pub juice: &'a Juice,
    pub results: &'a Results,
    pub ui: &'a UiMotion,
    pub waiting_for_controls: bool,
    /// Stored best race time, shown on the countdown card.
    pub best: Option<f32>,
}

pub(super) fn fade(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0., 1.))
}

pub(super) fn medal_color(medal: Medal) -> Color {
    match medal {
        Medal::Gold => GOLD,
        Medal::Silver => SILVER,
        Medal::Bronze => BRONZE,
        Medal::Unranked => MUTED,
    }
}

pub(super) fn modern(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color).with_shadow(fade(SHADOW, color.a))
}

pub(super) fn classic(size: f32, color: Color) -> TextStyle {
    TextStyle::classic(size, color).with_shadow(fade(SHADOW, color.a))
}

/// Arcade callout: classic face with a dark outline that reads on any sand.
pub(super) fn callout(size: f32, color: Color) -> TextStyle {
    TextStyle::classic(size, color)
        .with_outline(fade(OUTLINE, color.a))
        .with_shadow(fade(SHADOW, color.a))
}

fn centered_text(text: &str, cx: f32, y: f32, style: TextStyle) {
    draw_text_styled(text, cx, y, style.with_align(TextAlign::Center));
}

/// Rounded "glass" plate: soft shadow, top-lit gradient, hairline edge and an
/// optional accent stripe along the top.
pub(super) fn glass(rect: Rect, radius: f32, accent: Option<Color>, alpha: f32) {
    shapes::drop_shadow(
        rect,
        radius,
        Shadow::soft(Color::new(0., 0., 0., 0.32 * alpha))
            .offset(vec2(0., 4.))
            .blur(14.),
    );
    shapes::fill_rounded_rect(
        rect,
        radius,
        Fill::vertical(rect, fade(GLASS_TOP, alpha), fade(GLASS_BOTTOM, alpha)),
    );
    shapes::stroke_rounded_rect(rect, radius, 1., fade(LINE, alpha));
    if let Some(accent) = accent {
        let stripe = Rect::new(rect.x + radius, rect.y + 1., rect.w - radius * 2., 2.);
        shapes::fill_rounded_rect(stripe, 1., fade(accent, 0.9 * alpha));
    }
}

/// Split text into one or two lines at the space nearest the middle so it
/// fits `available` at its authored size instead of shrinking.
fn split_lines(text: &str, available: f32, style: TextStyle) -> (&str, Option<&str>) {
    if text_width(text, style) <= available {
        return (text, None);
    }
    let mid = text.len() / 2;
    let split = text[..mid]
        .rfind(' ')
        .into_iter()
        .chain(text[mid..].find(' ').map(|i| i + mid))
        .min_by_key(|&i| i.abs_diff(mid));
    match split {
        Some(at) => (&text[..at], Some(text[at + 1..].trim_start())),
        None => (text, None),
    }
}

// ---------------------------------------------------------------------------
// Pictograms. Engine icons where the shared set has the idea; the skimmer's
// own medals, gate arches, threading bullseye and boulders are drawn here.

/// A HUD icon with a soft drop shadow so it reads over sand and sky.
pub(super) fn icon(kind: Icon, at: Vec2, size: f32, color: Color) {
    icons::draw_styled(
        kind,
        at,
        size,
        IconStyle::new(color).shadow(fade(SHADOW, color.a * 0.7)),
    );
}

fn lighten(c: Color, t: f32) -> Color {
    let m = shapes::mix(c, Color::new(1., 1., 1., c.a), t);
    Color::new(m.r, m.g, m.b, c.a)
}

fn darken(c: Color, k: f32) -> Color {
    Color::new(c.r * k, c.g * k, c.b * k, c.a)
}

/// A five-point star.
fn star(c: Vec2, r: f32, color: Color) {
    let points: [Vec2; 10] = std::array::from_fn(|i| {
        let a = -PI * 0.5 + i as f32 * PI / 5.;
        c + Vec2::from_angle(a) * if i % 2 == 0 { r } else { r * 0.46 }
    });
    shapes::fill_polygon(&points, color);
}

/// A medal on a two-tone ribbon, starred in its own colour; unranked is a
/// plain disc.
pub(super) fn medal_glyph(c: Vec2, size: f32, medal: Medal, alpha: f32) {
    let color = medal_color(medal);
    let r = size * 0.34;
    let disc = c + vec2(0., size * 0.14);
    let top = c.y - size * 0.5;
    if medal != Medal::Unranked {
        let left = fade(Color::new(0.86, 0.33, 0.28, 1.), alpha);
        let right = fade(Color::new(0.24, 0.62, 0.62, 1.), alpha);
        shapes::fill_polygon(
            &[
                vec2(c.x - size * 0.34, top),
                vec2(c.x - size * 0.1, top),
                vec2(c.x + size * 0.08, disc.y - r * 0.5),
                vec2(c.x - size * 0.14, disc.y - r * 0.3),
            ],
            left,
        );
        shapes::fill_polygon(
            &[
                vec2(c.x + size * 0.1, top),
                vec2(c.x + size * 0.34, top),
                vec2(c.x + size * 0.14, disc.y - r * 0.3),
                vec2(c.x - size * 0.08, disc.y - r * 0.5),
            ],
            right,
        );
    }
    shapes::fill_circle(
        disc,
        r,
        Fill::radial(
            disc - vec2(r * 0.35, r * 0.4),
            r * 1.5,
            fade(lighten(color, 0.35), alpha),
            fade(darken(color, 0.78), alpha),
        ),
    );
    shapes::stroke_circle(
        disc,
        r * 0.76,
        (size * 0.045).max(1.),
        fade(darken(color, 0.68), alpha),
    );
    if medal != Medal::Unranked {
        star(disc, r * 0.5, fade(darken(color, 0.62), alpha));
    }
}

/// A gate arch like the ones on the course: two posts and a lintel. With
/// `bullseye`, a ring in its opening says "thread the centre".
pub(super) fn gate_glyph(c: Vec2, size: f32, color: Color, bullseye: bool) {
    let s = size * 0.5;
    for x in [-0.66, 0.42] {
        shapes::fill_rounded_rect(
            Rect::new(c.x + x * s, c.y - 0.62 * s, 0.24 * s, 1.5 * s),
            0.08 * s,
            color,
        );
    }
    shapes::fill_rounded_rect(
        Rect::new(c.x - 0.86 * s, c.y - 0.86 * s, 1.72 * s, 0.3 * s),
        0.1 * s,
        color,
    );
    if bullseye {
        let centre = c + vec2(0., 0.16 * s);
        shapes::stroke_circle(centre, 0.24 * s, (0.09 * s).max(1.), color);
        shapes::fill_circle(centre, 0.09 * s, color);
    }
}

/// A low-poly boulder, the course's rocks.
fn rock_glyph(c: Vec2, size: f32, color: Color) {
    let s = size * 0.5;
    let p = |x: f32, y: f32| c + vec2(x, y) * s;
    shapes::fill_polygon(
        &[
            p(-0.72, 0.52),
            p(-0.8, 0.02),
            p(-0.42, -0.5),
            p(0.18, -0.62),
            p(0.7, -0.18),
            p(0.76, 0.52),
        ],
        color,
    );
    shapes::fill_polygon(
        &[p(-0.42, -0.5), p(0.18, -0.62), p(0.02, 0.0), p(-0.5, -0.06)],
        lighten(color, 0.3),
    );
}

/// A rock with whoosh lines: a scrape that missed it.
fn scrape_glyph(c: Vec2, size: f32, color: Color) {
    rock_glyph(c + vec2(size * 0.16, 0.), size * 0.72, color);
    for (dy, len) in [(-0.22, 0.42), (0.08, 0.56), (0.36, 0.38)] {
        let y = c.y + dy * size * 0.5;
        let x = c.x - size * 0.5;
        shapes::fill_capsule(
            vec2(x, y),
            vec2(x + len * size * 0.5, y),
            (size * 0.04).max(0.8),
            color,
        );
    }
}

/// A rock with a burst: an impact.
fn impact_glyph(c: Vec2, size: f32, color: Color) {
    rock_glyph(
        c + vec2(-size * 0.12, size * 0.06),
        size * 0.72,
        darken(color, 0.85),
    );
    star(c + vec2(size * 0.26, -size * 0.22), size * 0.22, color);
}

/// A diagonal slash: blocked.
fn slash(c: Vec2, size: f32, color: Color) {
    let s = size * 0.5;
    shapes::fill_capsule(
        c + vec2(-0.62, -0.62) * s,
        c + vec2(0.62, 0.62) * s,
        (0.1 * s).max(1.),
        color,
    );
}

/// What a key cap or callout means, drawn as a pictogram.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Glyph {
    Icon(Icon),
    /// A course gate.
    Gate,
    /// Thread the centre of a gate.
    Thread,
    /// Boost blocked: a slashed bolt.
    NoBoost,
    /// A medal in its own colours (the piece colour is ignored).
    Medal(Medal),
}

impl Glyph {
    fn draw(self, c: Vec2, size: f32, color: Color) {
        match self {
            Glyph::Icon(kind) => icon(kind, c, size, color),
            Glyph::Gate => gate_glyph(c, size, color, false),
            Glyph::Thread => gate_glyph(c, size, color, true),
            Glyph::NoBoost => {
                icon(Icon::Bolt, c, size, color);
                slash(c, size, fade(ROSE, color.a));
            }
            Glyph::Medal(medal) => medal_glyph(c, size * 1.1, medal, color.a),
        }
    }
}

/// One element of an icon prompt row.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Piece<'a> {
    /// A key cap showing the live binding.
    Key(&'a str),
    Glyph(Glyph, Color),
    /// A small "leads to" arrow between a cause and its reward.
    Then,
    /// Plain text: numbers and callouts of at most three words.
    Text(&'a str, Color),
}

const PIECE_GAP: f32 = 8.;

fn piece_width(piece: Piece, theme: &KitTheme) -> f32 {
    let style = TextStyle::modern(theme.body, theme.ink);
    let h = theme.body * 1.5 + 2.;
    match piece {
        Piece::Key(key) => (text_width(key, style) + kit::SPACE[2] * 2.).max(h - 3.),
        Piece::Glyph(..) => theme.body * 1.4,
        Piece::Then => theme.body * 0.8,
        Piece::Text(text, _) => text_width(text, modern(theme.body, INK)),
    }
}

fn pieces_width(pieces: &[Piece], theme: &KitTheme) -> f32 {
    pieces.iter().map(|&p| piece_width(p, theme)).sum::<f32>()
        + PIECE_GAP * pieces.len().saturating_sub(1) as f32
}

/// Draws a prompt row with its vertical centre at `cy`, starting at `x`.
fn draw_pieces(pieces: &[Piece], x: f32, cy: f32, theme: &KitTheme) {
    let mut x = x;
    let alpha = theme.alpha;
    for &piece in pieces {
        let w = piece_width(piece, theme);
        match piece {
            Piece::Key(key) => {
                let h = theme.body * 1.5 + 2.;
                kit::keycap(x, cy - (h - 3.) * 0.5, key, theme);
            }
            Piece::Glyph(glyph, color) => {
                glyph.draw(vec2(x + w * 0.5, cy), w, fade(color, alpha));
            }
            Piece::Then => {
                let c = vec2(x + w * 0.5, cy);
                let s = w * 0.5;
                shapes::fill_triangle(
                    c + vec2(-s * 0.5, -s * 0.7),
                    c + vec2(s * 0.7, 0.),
                    c + vec2(-s * 0.5, s * 0.7),
                    fade(MUTED, alpha),
                );
            }
            Piece::Text(text, color) => {
                let style = modern(theme.body, fade(color, alpha));
                draw_text_styled(text, x, style.centered_baseline(cy), style);
            }
        }
        x += w + PIECE_GAP;
    }
}

/// The medal the running clock would still earn, projected from the last
/// completed lap over the remaining gates. Lap one has no honest pace yet.
pub(super) fn pace(race: &Race) -> Option<Medal> {
    if race.finished || race.laps == 0 {
        return None;
    }
    let last = race.lap_times[race.laps as usize - 1];
    let remaining = (GATES * LAPS as usize - race.gates_completed()) as f32;
    Some(quips::medal_for(
        race.elapsed + remaining / GATES as f32 * last,
    ))
}

pub(super) fn summary(race: &Race) -> RunSummary {
    RunSummary {
        medal: quips::medal_for(race.elapsed),
        threaded: race.threaded,
        gates: (GATES * LAPS as usize) as u32,
        best_streak: race.best_streak,
        perfect_laps: race.perfect_laps,
        scrapes: race.scrapes,
        impacts: race.impacts,
        recoveries: race.recovery_count,
        rocket: race.rocket_start,
    }
}

/// The next goal on the results card, drawn as icons and numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Goal {
    /// Find `cut` seconds for the next `medal`.
    Medal { medal: Medal, cut: f32 },
    /// Thread all fourteen gates in one lap.
    Mirage,
    /// Thread every gate of the race (the Sand Surgeon).
    AllGates(u32),
    /// Beat this race time.
    Beat(f32),
}

/// The next-goal hook on the results card.
pub(super) fn next_goal(race: &Race, results: &Results) -> Goal {
    let time = race.elapsed;
    let medal = quips::medal_for(time);
    let best = results
        .previous_best
        .filter(|_| !results.demo)
        .map_or(time, |b| b.min(time));
    if let Some((medal, target)) = quips::next_medal(medal) {
        Goal::Medal {
            medal,
            cut: (time - target).max(0.01),
        }
    } else if race.perfect_laps == 0 {
        Goal::Mirage
    } else if race.threaded < (GATES * LAPS as usize) as u32 {
        Goal::AllGates((GATES * LAPS as usize) as u32)
    } else {
        Goal::Beat(best - 0.25)
    }
}

/// Speed lines and edge flashes, drawn on top of the world under the HUD.
pub(super) fn effects(race: &Race, juice: &Juice) {
    let w = screen_width();
    let h = screen_height();
    let lines = if race.finished {
        0.
    } else {
        juice.speed_lines(race.speed(), race.boosting)
    };
    if lines > 0. {
        let center = vec2(w * 0.5, h * 0.55);
        let reach = (w * w + h * h).sqrt() * 0.5;
        let tint = if race.boosting {
            Color::new(1., 0.86, 0.62, 1.)
        } else {
            Color::new(1., 0.97, 0.9, 1.)
        };
        for i in 0..26 {
            let angle = i as f32 * 2.399_963 + 0.4;
            let phase = (juice.time * 1.7 + i as f32 * 0.383).fract();
            let dir = vec2(angle.cos(), angle.sin());
            let start = reach * (0.42 + phase * 0.5);
            let length = reach * (0.10 + 0.08 * lines);
            let a = center + dir * start;
            let b = center + dir * (start + length);
            let alpha = lines * 0.3 * (1. - phase) * phase * 4.;
            draw_line(a.x, a.y, b.x, b.y, 2., fade(tint, alpha));
        }
    }
    if juice.flash > 0.01 {
        // A soft radial edge glow rather than hard bands.
        let c = juice.flash_color;
        let edge = w.min(h) * 0.09;
        for (rect, fill) in [
            (
                Rect::new(0., 0., w, edge),
                Fill::vertical(
                    Rect::new(0., 0., w, edge),
                    fade(c, 0.3 * juice.flash),
                    fade(c, 0.),
                ),
            ),
            (
                Rect::new(0., h - edge, w, edge),
                Fill::vertical(
                    Rect::new(0., h - edge, w, edge),
                    fade(c, 0.),
                    fade(c, 0.3 * juice.flash),
                ),
            ),
            (
                Rect::new(0., 0., edge, h),
                Fill::horizontal(
                    Rect::new(0., 0., edge, h),
                    fade(c, 0.3 * juice.flash),
                    fade(c, 0.),
                ),
            ),
            (
                Rect::new(w - edge, 0., edge, h),
                Fill::horizontal(
                    Rect::new(w - edge, 0., edge, h),
                    fade(c, 0.),
                    fade(c, 0.3 * juice.flash),
                ),
            ),
        ] {
            shapes::fill_rounded_rect(rect, 0., fill);
        }
    }
}

/// The browser host's fullscreen menu button reserves the top-right corner:
/// nothing readable may sit at `x > width - MENU_CORNER.0` and `y < MENU_CORNER.1`.
pub(super) const MENU_CORNER: (f32, f32) = (72., 60.);
/// Right edge of the wide right-hand cluster, inset past the menu corner.
const WIDE_RIGHT: f32 = 76.;
/// Room the compact flag icon takes before the lap count.
const COMPACT_FLAG: f32 = 20.;

fn gate_count(completed: usize) -> Buf<16> {
    buf_small(format_args!("{:02} / {GATES}", completed % GATES + 1))
}

fn buf_small(args: std::fmt::Arguments) -> Buf<16> {
    let mut out = Buf::new();
    let _ = std::fmt::Write::write_fmt(&mut out, args);
    out
}

/// The streak chip: a flame, the streak length and the boost multiplier.
fn streak_chip_width(streak: u32) -> f32 {
    let style = modern(16., GOLD);
    let n = buf_small(format_args!("{streak}"));
    let m = buf_small(format_args!("×{}", streak.min(MULTIPLIER_CAP)));
    36. + text_width(&n, style) + 10. + text_width(&m, style) + 12.
}

fn streak_chip(r: Rect, streak: u32) {
    let style = modern(16., GOLD);
    let cy = r.y + r.h * 0.5;
    icon(Icon::Flame, vec2(r.x + 21., cy), 18., GOLD);
    let n = buf_small(format_args!("{streak}"));
    let x = r.x + 36.;
    draw_text_styled(&n, x, style.centered_baseline(cy), style);
    let m = buf_small(format_args!("×{}", streak.min(MULTIPLIER_CAP)));
    draw_text_styled(
        &m,
        x + text_width(&n, style) + 10.,
        style.centered_baseline(cy),
        modern(16., ORANGE),
    );
}

/// Wide next-gate plate, right-aligned inside the inset.
fn wide_gate_plate(w: f32) -> Rect {
    let pips_w = GATES as f32 * 12. - 3.;
    let label_w = 22. + 10. + classic_width("14 / 14", 14);
    let gate_w = pips_w.max(label_w) + 28.;
    Rect::new(w - WIDE_RIGHT - gate_w, 12., gate_w, 60.)
}

/// Mid-width hosts drop the best-lap column and use a smaller clock.
fn mid_width(w: f32) -> bool {
    w < 860.
}

/// Wide top plates (lap, clock, gate). The clock slides between its
/// neighbours when centering would make them touch.
fn wide_plates(w: f32, sub_w: f32) -> [Rect; 3] {
    let margin = (w * 0.035).clamp(18.0, 44.0);
    let gate = wide_gate_plate(w);
    let mid = mid_width(w);
    let lap = Rect::new(margin - 12., 12., if mid { 92. } else { 204. }, 60.);
    let timer_w = text_width("00:00.00", classic(if mid { 24. } else { 32. }, INK));
    let clock_w = (sub_w.max(timer_w) + 32.).max(if mid { 200. } else { 236. });
    let mut x = (w - clock_w) * 0.5;
    if x < lap.x + lap.w + 8. || x + clock_w > gate.x - 8. {
        x = (lap.x + lap.w + gate.x - clock_w) * 0.5;
    }
    [lap, Rect::new(x, 12., clock_w, 60.), gate]
}

/// Wide streak chip under the gate plate, right-aligned inside the inset.
fn wide_streak_chip(w: f32, streak: u32) -> Rect {
    let chip_w = streak_chip_width(streak);
    Rect::new(w - WIDE_RIGHT - chip_w, 78., chip_w, 26.)
}

/// Compact gate row: an arch and the count.
fn compact_gate_width(completed: usize) -> f32 {
    22. + text_width(&gate_count(completed), modern(16., INK))
}

/// Compact next-gate row left x, centered between the margin and the menu corner.
fn compact_gate_x(w: f32, completed: usize) -> f32 {
    let margin = (w * 0.035).clamp(18.0, 44.0);
    let right = w - MENU_CORNER.0;
    margin + ((right - margin) - compact_gate_width(completed)) * 0.5
}

/// Compact timer left x; its right edge stays 8px clear of the menu corner.
fn compact_timer_x(w: f32, timer: &str) -> f32 {
    w - 80. - measure_text(timer, None, 20, 1.0).width
}

fn lap_pips(race: &Race, x: f32, y: f32, pip: f32, gap: f32) {
    let earned = race.gates_completed() % GATES;
    let threads = race.lap_threads();
    for i in 0..GATES {
        // Lap order starts after the start/finish gate: 1..13 then 0.
        let gate = (i + 1) % GATES;
        let px = x + i as f32 * (pip + gap);
        let color = if i < earned {
            if threads & (1 << gate) != 0 {
                GOLD
            } else {
                MUTED
            }
        } else {
            Color::new(0.30, 0.36, 0.36, 0.9)
        };
        draw_rectangle(px, y, pip, 4., color);
        if i == earned && !race.finished {
            draw_rectangle(px - 1., y - 2., pip + 2., 8., AQUA);
        }
    }
}

fn speed_kmh(race: &Race) -> Buf<16> {
    buf_small(format_args!("{:03}", (race.speed() * 3.6) as u32))
}

/// Body-size theme for prompt rows.
fn prompt_theme(alpha: f32) -> KitTheme {
    KitTheme {
        surface: Color::new(0.08, 0.10, 0.12, 0.9),
        raised: Color::new(0.20, 0.23, 0.25, 1.),
        header: Color::new(0.30, 0.33, 0.34, 1.),
        ink: INK,
        muted: INK,
        ..KitTheme::default()
    }
    .faded(alpha)
}

/// A glass pill holding one prompt row, centred on `cx` with its middle at `cy`.
fn prompt_pill(pieces: &[Piece], cx: f32, cy: f32, accent: Option<Color>, theme: &KitTheme) {
    let w = pieces_width(pieces, theme);
    let pill = Rect::new(cx - w * 0.5 - 16., cy - 17., w + 32., 34.);
    glass(pill, 17., accent, theme.alpha);
    draw_pieces(pieces, pill.x + 16., cy, theme);
}

/// The three medal targets, or the stored best and the gold target, as
/// medals and times. Compact screens keep only the first target.
fn medal_targets(cx: f32, cy: f32, best: Option<f32>, compact: bool, theme: &KitTheme) {
    let times: [Buf<16>; 3] = std::array::from_fn(|i| clock(quips::MEDALS[i]));
    let best_time = best.map(clock);
    let mut pieces: [Piece; 6] = [Piece::Then; 6];
    let mut n = 0;
    if let Some(best) = &best_time {
        pieces[0] = Piece::Glyph(Glyph::Icon(Icon::Crown), GOLD);
        pieces[1] = Piece::Text(best, GOLD);
        n = 2;
    }
    let medals = [Medal::Gold, Medal::Silver, Medal::Bronze];
    let count = if compact || best.is_some() { 1 } else { 3 };
    for (medal, time) in medals.into_iter().zip(&times).take(count) {
        pieces[n] = Piece::Glyph(Glyph::Medal(medal), WHITE);
        pieces[n + 1] = Piece::Text(time, medal_color(medal));
        n += 2;
    }
    let pieces = &pieces[..n];
    draw_pieces(pieces, cx - pieces_width(pieces, theme) * 0.5, cy, theme);
}

impl Hud<'_> {
    pub fn draw(&self) {
        let race = self.race;
        if self.waiting_for_controls {
            title_screen(self.ui, self.input, self.best, 1.);
            return;
        }
        if race.finished {
            self.finish_card();
            return;
        }
        if screen_width() < 600. {
            self.top_compact();
        } else {
            self.top_wide();
        }
        self.gauge();
        self.boost();
        self.center();
        self.callouts();
    }

    fn top_wide(&self) {
        let race = self.race;
        let w = screen_width();
        let mid = mid_width(w);
        let pace_medal = pace(race);
        let lap_clock = clock(race.lap_elapsed());
        let sub_style = modern(16., MUTED);
        // Mid-width hosts show pace instead of the lap clock when both exist.
        let show_lap = !(pace_medal.is_some() && mid);
        let lap_w = 20. + text_width(&lap_clock, sub_style);
        let gap = if show_lap && pace_medal.is_some() {
            14.
        } else {
            0.
        };
        let sub_w = if show_lap { lap_w } else { 0. } + pace_medal.map_or(0., |_| gap + 18.);
        let [lap_plate, clock_plate, gate] = wide_plates(w, sub_w);
        let motion = self.juice.motion().min(1.);
        // Lap plate: a flag and the lap (orange and beating on the final
        // lap), then a star and the best lap.
        glass(lap_plate, RADIUS, Some(AQUA), 1.);
        let cy = lap_plate.y + lap_plate.h * 0.5;
        let final_lap = race.laps + 1 == LAPS;
        let beat = if final_lap {
            1. + 0.12 * (self.juice.time * 6.).sin() * motion
        } else {
            1.
        };
        icon(
            Icon::Flag,
            vec2(lap_plate.x + 24., cy),
            20. * beat,
            if final_lap { ORANGE } else { INK },
        );
        let x = lap_plate.x + 40.;
        let lap = (race.laps + 1).min(LAPS);
        let lap_text = buf_small(format_args!("{lap}"));
        let lap_style = classic(24., if final_lap { ORANGE } else { INK });
        let base = lap_style.centered_baseline(cy);
        draw_text_styled(&lap_text, x, base, lap_style);
        draw_text_styled(
            &buf_small(format_args!("/{LAPS}")),
            x + text_width(&lap_text, lap_style) + 4.,
            base,
            modern(16., MUTED),
        );
        if !mid {
            let column = lap_plate.x + 92.;
            icon(Icon::Star, vec2(column + 9., cy), 18., GOLD);
            let style = modern(16., INK);
            match race.best_lap() {
                Some(best) => draw_text_styled(
                    &clock(best),
                    column + 24.,
                    style.centered_baseline(cy),
                    style,
                ),
                None => draw_text_styled(
                    "--:--.--",
                    column + 24.,
                    style.centered_baseline(cy),
                    modern(16., MUTED),
                ),
            };
        }
        // Clock plate: the race clock; under it a clock icon with the lap
        // time and the medal this pace projects.
        glass(clock_plate, RADIUS, Some(AQUA), 1.);
        let cx = clock_plate.x + clock_plate.w * 0.5;
        let timer = clock(race.elapsed);
        centered_text(&timer, cx, 44., classic(if mid { 24. } else { 32. }, INK));
        if race.recovery_count > 0 {
            // The clock already includes recovery penalties; show them.
            draw_text_styled(
                &buf!("+{:.0}s", race.penalty_seconds()),
                clock_plate.x + clock_plate.w - 10.,
                30.,
                modern(16., ORANGE).with_align(TextAlign::Right),
            );
        }
        let row = 57.;
        let mut left = cx - sub_w * 0.5;
        if show_lap {
            icon(Icon::Clock, vec2(left + 7., row), 14., MUTED);
            draw_text_styled(
                &lap_clock,
                left + 20.,
                sub_style.centered_baseline(row),
                sub_style,
            );
            left += lap_w + gap;
        }
        if let Some(medal) = pace_medal {
            medal_glyph(vec2(left + 9., row - 1.), 18., medal, 1.);
        }
        // Next-gate plate, inset clear of the host's fullscreen menu corner.
        glass(gate, RADIUS, Some(AQUA), 1.);
        gate_glyph(vec2(gate.x + 25., 29.), 20., AQUA, false);
        let count = gate_count(race.gates_completed());
        // Classic teal 14px: the browser glyph contract inspects this count.
        label(
            &count,
            gate.x + gate.w - 14. - classic_width(&count, 14),
            36.,
            14.,
            TEAL,
        );
        lap_pips(race, gate.x + 14., 52., 9., 3.);
        if race.streak >= 2 {
            let r = wide_streak_chip(w, race.streak);
            glass(r, r.h * 0.5, None, 1.);
            streak_chip(r, race.streak);
        }
    }

    fn top_compact(&self) {
        let race = self.race;
        let w = screen_width();
        let h = screen_height();
        let margin = (w * 0.035).clamp(18.0, 44.0);
        let short_start = h < 460. && (race.countdown > 0. || race.elapsed < 0.8);
        let band = Rect::new(0., 0., w, if short_start { 62. } else { 76. });
        shapes::fill_rounded_rect(
            band,
            0.,
            Fill::vertical(band, fade(GLASS_BOTTOM, 0.95), fade(GLASS_TOP, 0.7)),
        );
        // Classic white 14px lap and 20px timer: the browser glyph contract.
        let lap_text = buf!("{}/{LAPS}", (race.laps + 1).min(LAPS));
        label(&lap_text, margin + COMPACT_FLAG, 28., 14., WHITE);
        let timer = clock(race.elapsed);
        label(&timer, compact_timer_x(w, &timer), 28., 20.0, WHITE);
        let final_lap = race.laps + 1 == LAPS;
        icon(
            Icon::Flag,
            vec2(margin + 7., 22.),
            15.,
            if final_lap { ORANGE } else { INK },
        );
        if short_start {
            // Reuse the next-gate row before driving starts. A separate center cue
            // would overlap boost telemetry at the 352px minimum in touch mode.
            let text = if race.countdown > 0. {
                buf!("{}", race.countdown.ceil() as u32)
            } else {
                buf!("GO")
            };
            centered_text(
                &text,
                w * 0.5,
                SHORT_START_ROW.0,
                callout(SHORT_START_ROW.1 as f32, AQUA),
            );
            return;
        }
        let completed = race.gates_completed();
        let gx = compact_gate_x(w, completed);
        gate_glyph(vec2(gx + 9., 48.), 18., AQUA, false);
        draw_text_styled(&gate_count(completed), gx + 22., 54., modern(16., AQUA));
        if race.streak >= 2 {
            // The multiplier leads the gate row, away from the menu corner.
            icon(Icon::Flame, vec2(margin + 7., 48.), 15., GOLD);
            draw_text_styled(
                &buf!("×{}", race.streak.min(MULTIPLIER_CAP)),
                margin + 17.,
                54.,
                modern(16., GOLD),
            );
        }
        let pip_gap = 3.;
        let pip = ((w - margin * 2. - pip_gap * (GATES as f32 - 1.)) / GATES as f32).min(14.);
        let pips_w = GATES as f32 * (pip + pip_gap) - pip_gap;
        lap_pips(race, (w - pips_w) * 0.5, 64., pip, pip_gap);
    }

    fn bottom_y(&self) -> f32 {
        let h = screen_height();
        if self.input.touch_seen() {
            h - 150.0
        } else {
            h - 75.0
        }
    }

    /// Radial speed gauge: a 270° arc that turns ember-orange and glows on
    /// boost. A small pill beside it shows traction as an icon: waves when
    /// sliding, a down arrow in reverse, a dune on loose sand.
    fn gauge(&self) {
        let race = self.race;
        let w = screen_width();
        let margin = (w * 0.035).clamp(18.0, 44.0);
        let small = w < 400.;
        let radius = if small { 42. } else { 54. };
        let center = vec2(margin - 4. + radius, self.bottom_y() + 20. - radius - 6.);
        let disc = Rect::new(
            center.x - radius - 8.,
            center.y - radius - 8.,
            (radius + 8.) * 2.,
            (radius + 8.) * 2.,
        );
        glass(disc, radius + 8., None, 1.);
        let start = PI * 0.75;
        let sweep = PI * 1.5;
        shapes::stroke_arc(
            center,
            radius - 3.,
            start,
            sweep,
            Stroke::new(7., TRACK).cap(Cap::Round),
        );
        let fill = self.ui.gauge.value.clamp(0., 1.);
        let color = if race.boosting { ORANGE } else { AQUA };
        if fill > 0.005 {
            shapes::stroke_arc(
                center,
                radius - 3.,
                start,
                sweep * fill,
                Stroke::new(7., color).cap(Cap::Round),
            );
            if race.boosting {
                let tip = start + sweep * fill;
                let p = center + vec2(tip.cos(), tip.sin()) * (radius - 3.);
                shapes::glow_disc(p, 16., fade(Color::new(1., 0.7, 0.3, 1.), 0.8));
            }
        }
        // Thin ticks every 50 km/h make the dial read as an instrument.
        for i in 0..=3 {
            let a = start + sweep * (i as f32 * 50. / 158.4);
            let dir = vec2(a.cos(), a.sin());
            let p0 = center + dir * (radius - 12.);
            let p1 = center + dir * (radius - 16.);
            draw_line(p0.x, p0.y, p1.x, p1.y, 1.5, fade(MUTED, 0.7));
        }
        let speed = speed_kmh(race);
        let size = if small { 24. } else { 32. };
        centered_text(
            &speed,
            center.x,
            classic(size, INK).centered_baseline(center.y - 2.),
            classic(size, INK),
        );
        centered_text(
            "km/h",
            center.x,
            center.y + radius * 0.52,
            modern(16., MUTED),
        );
        let traction = match race.traction() {
            Traction::Track => None,
            Traction::Sliding => Some((Icon::Wave, ORANGE)),
            Traction::Reversing => Some((Icon::ArrowDown, MUTED)),
            Traction::Sand => Some((Icon::Mountain, ORANGE)),
        };
        if let Some((kind, color)) = traction {
            let pill = Rect::new(disc.x + disc.w + 8., center.y - 16., 36., 32.);
            glass(pill, 16., Some(color), 1.);
            icon(kind, pill.center(), 20., color);
        }
    }

    /// Boost meter with a trailing ghost, the 25% rearm notch and a hot glow.
    /// A bolt shows the state by colour (aqua ready, orange boosting, slashed
    /// on sand); a thermometer means overheated. Under the bar: the boost key
    /// cap until boost is learned, the cooling time, or the dune that blocks it.
    fn boost(&self) {
        let race = self.race;
        let w = screen_width();
        let margin = (w * 0.035).clamp(18.0, 44.0);
        let bottom = self.bottom_y();
        let bar_w = (w * 0.38).min(220.0);
        let bx = w - margin - bar_w;
        let mut boost_buffer = [0; 256];
        let boost_key = binding_hint(self.input, "boost", &mut boost_buffer);
        let learned = self.juice.learned.boost || self.juice.veteran;
        let status = race.boost_status();
        let (kind, color) = match status {
            BoostStatus::Countdown => (Icon::Bolt, MUTED),
            BoostStatus::Ready => (Icon::Bolt, AQUA),
            BoostStatus::Active => (Icon::Bolt, ORANGE),
            BoostStatus::Overheated => (Icon::Thermometer, ROSE),
            BoostStatus::Sand => (Icon::Bolt, MUTED),
            BoostStatus::Finished => (Icon::Check, AQUA),
        };
        let cooling = buf!("{:.1}s", race.cooling_seconds());
        let hint: Option<[Piece; 2]> = match status {
            BoostStatus::Ready if !learned => Some([
                Piece::Key(boost_key),
                Piece::Glyph(Glyph::Icon(Icon::Bolt), AQUA),
            ]),
            BoostStatus::Overheated => Some([
                Piece::Glyph(Glyph::Icon(Icon::Hourglass), ROSE),
                Piece::Text(&cooling, ROSE),
            ]),
            BoostStatus::Sand => Some([
                Piece::Glyph(Glyph::Icon(Icon::Mountain), ORANGE),
                Piece::Text("SAND", ORANGE),
            ]),
            _ => None,
        };
        let panel = Rect::new(
            bx - 14.,
            bottom - 50.,
            bar_w + 28.,
            if hint.is_some() { 80. } else { 62. },
        );
        glass(panel, RADIUS, Some(color), 1.);
        let head = vec2(bx + 10., bottom - 32.);
        if status == BoostStatus::Active {
            let throb = 0.5 + 0.5 * (self.juice.time * 14.).sin() * self.juice.motion().min(1.);
            shapes::glow_disc(
                head,
                16. + 6. * throb,
                fade(Color::new(1., 0.6, 0.25, 1.), 0.6),
            );
        }
        icon(kind, head, 20., color);
        if status == BoostStatus::Sand {
            slash(head, 22., ROSE);
        }
        let percent = buf!("{}%", (race.boost * 100.0) as u32);
        draw_text_styled(
            &percent,
            bx + bar_w,
            bottom - 26.,
            classic(16., INK).with_align(TextAlign::Right),
        );
        let bar = Rect::new(bx, bottom - 16., bar_w, 14.);
        shapes::fill_rounded_rect(bar, 7., TRACK);
        let ghost = self.ui.ghost.value.clamp(0., 1.);
        if ghost > race.boost + 0.005 {
            let g = Rect::new(bx, bar.y, bar_w * ghost, bar.h);
            shapes::fill_rounded_rect(g, 7., fade(Color::new(1., 0.9, 0.75, 1.), 0.35));
        }
        if race.boost > 0.01 {
            let f = Rect::new(bx, bar.y, (bar_w * race.boost).max(bar.h), bar.h);
            let (a, b) = if race.boosting {
                (
                    Color::new(1., 0.42, 0.12, 1.),
                    Color::new(1., 0.86, 0.45, 1.),
                )
            } else {
                (fade(color, 0.75), color)
            };
            shapes::fill_rounded_rect(f, 7., Fill::horizontal(f, a, b));
            let gloss = Rect::new(f.x + 4., f.y + 2., (f.w - 8.).max(1.), 3.);
            shapes::fill_rounded_rect(gloss, 1.5, Color::new(1., 1., 1., 0.25));
            if race.boosting {
                shapes::glow_disc(
                    vec2(f.x + f.w - 4., f.y + f.h * 0.5),
                    22.,
                    fade(Color::new(1., 0.6, 0.25, 1.), 0.7),
                );
            }
        }
        draw_rectangle(bx + bar_w * 0.25 - 1., bar.y - 3., 2., bar.h + 6., INK);
        if let Some(hint) = hint {
            draw_pieces(&hint, bx, bottom + 16., &prompt_theme(1.));
        }
    }

    /// Countdown, alerts and the one onboarding prompt.
    fn center(&self) {
        let race = self.race;
        let w = screen_width();
        let h = screen_height();
        let compact = w < 600.;
        let short = h < 460.;
        let touch = self.input.touch_seen();
        let motion = self.juice.motion();
        if race.countdown > 0.0 && !(compact && short) {
            let number = race.countdown.ceil() as u32;
            let phase = 1. - race.countdown.fract();
            let punch = 1. + 0.5 * (1. - phase.min(1.)).powi(3) * motion.min(1.);
            let text = buf_small(format_args!("{number}"));
            if short {
                centered_text(&text, w * 0.5, 160., callout(32., INK));
            } else {
                let card_w = (w - 32.).min(440.);
                let top = if compact { 156. } else { h * 0.24 };
                let card = Rect::new((w - card_w) * 0.5, top, card_w, 62.);
                glass(card, RADIUS, Some(GOLD), 1.);
                // Three laps of fourteen gates, then the targets to beat.
                let theme = prompt_theme(1.);
                let laps = buf_small(format_args!("{LAPS}"));
                let gates = buf_small(format_args!("{GATES}"));
                let course = [
                    Piece::Glyph(Glyph::Icon(Icon::Flag), INK),
                    Piece::Text(&laps, INK),
                    Piece::Text("·", MUTED),
                    Piece::Glyph(Glyph::Gate, AQUA),
                    Piece::Text(&gates, INK),
                ];
                draw_pieces(
                    &course,
                    w * 0.5 - pieces_width(&course, &theme) * 0.5,
                    top + 19.,
                    &theme,
                );
                medal_targets(w * 0.5, top + 44., self.best, compact, &theme);
                let size = (if compact { 48. } else { 80. } * punch).round();
                let baseline = if compact {
                    (h * 0.5).max(top + 130.)
                } else {
                    h * 0.52
                };
                centered_text(&text, w * 0.5, baseline, callout(size, INK));
            }
        }
        // Alerts outrank the coach; they share one slot.
        let alert_y = if touch {
            h - 106.
        } else if compact {
            self.bottom_y() - 92.
        } else {
            h - 30.
        };
        let mut drive = [0; 256];
        let drive_key = binding_hint(self.input, "move_forward", &mut drive);
        let penalty = buf!("+{:.2}s", super::race::RECOVERY_PENALTY);
        let countdown = race.recovery_in().map(|s| buf!("{s:.1}s"));
        let theme = prompt_theme(1.);
        let alert: Option<([Piece; 4], usize, Color)> = if self.waiting_for_controls {
            // Let go of the drive key to rearm it.
            Some((
                [
                    Piece::Glyph(Glyph::Icon(Icon::Hand), INK),
                    Piece::Key(drive_key),
                    Piece::Then,
                    Piece::Then,
                ],
                2,
                INK,
            ))
        } else if race.recovery_timer > 0.0 {
            Some((
                [
                    Piece::Glyph(Glyph::Icon(Icon::Restart), ORANGE),
                    Piece::Text(&penalty, ORANGE),
                    Piece::Then,
                    Piece::Then,
                ],
                2,
                ORANGE,
            ))
        } else if let Some(countdown) = &countdown {
            Some((
                [
                    Piece::Glyph(Glyph::Icon(Icon::Alert), ORANGE),
                    Piece::Text("OFF TRACK", ORANGE),
                    Piece::Glyph(Glyph::Icon(Icon::Hourglass), ORANGE),
                    Piece::Text(countdown, INK),
                ],
                4,
                ORANGE,
            ))
        } else if race.off_track {
            Some((
                [
                    Piece::Glyph(Glyph::Icon(Icon::Mountain), ORANGE),
                    Piece::Text("SAND", INK),
                    Piece::Glyph(Glyph::NoBoost, MUTED),
                    Piece::Then,
                ],
                3,
                ORANGE,
            ))
        } else {
            None
        };
        if let Some((pieces, n, color)) = alert {
            prompt_pill(&pieces[..n], w * 0.5, alert_y - 5., Some(color), &theme);
            return;
        }
        self.coach(alert_y);
    }

    /// One key-cap prompt for a first-time pilot, fading once learned: drive,
    /// thread the glowing gate's centre for boost, then boost.
    fn coach(&self, y: f32) {
        let juice = self.juice;
        let race = self.race;
        if juice.veteran || race.finished || juice.hint_fade >= 1. {
            return;
        }
        let alpha = 1. - juice.hint_fade;
        let mut drive = [0; 256];
        let mut boost = [0; 256];
        let drive = binding_hint(self.input, "move_forward", &mut drive);
        let boost = binding_hint(self.input, "boost", &mut boost);
        let up = Piece::Glyph(Glyph::Icon(Icon::ArrowUp), INK);
        let bolt = Piece::Glyph(Glyph::Icon(Icon::Bolt), AQUA);
        let pieces: &[Piece] = if race.countdown > 0. {
            &[
                Piece::Key(drive),
                up,
                Piece::Text(" ", INK),
                Piece::Key(boost),
                bolt,
            ]
        } else if !juice.learned.drive {
            &[Piece::Key(drive), up]
        } else if !juice.learned.thread {
            &[Piece::Glyph(Glyph::Thread, AQUA), Piece::Then, bolt]
        } else if !juice.learned.boost {
            &[Piece::Key(boost), bolt]
        } else {
            &[
                Piece::Glyph(Glyph::Thread, GOLD),
                Piece::Then,
                Piece::Glyph(Glyph::Icon(Icon::Bolt), GOLD),
            ]
        };
        prompt_pill(
            pieces,
            screen_width() * 0.5,
            y - 5.,
            None,
            &prompt_theme(alpha),
        );
    }

    /// Rising pops, the lap banner and the radio toast.
    pub fn callouts(&self) {
        let juice = self.juice;
        let w = screen_width();
        let h = screen_height();
        let compact = w < 600.;
        let short = h < 460.;
        let rise = juice.motion().min(1.);
        if let Some(pop) = &juice.pop {
            let alpha = pop.alpha();
            let t = pop.age / pop.life;
            // Pop in with a back-out overshoot, then float up and fade.
            let grow = if rise > 0. {
                Ease::BackOut.apply(pop.grow())
            } else {
                1.
            };
            let base = if compact { 24. } else { pop.size.max(24.) };
            let size = (base * (0.6 + 0.4 * grow)).round().max(8.);
            let travel = if short { 24. } else { 46. };
            let y = h * if short { 0.62 } else { 0.44 } - travel * Ease::CubicOut.apply(t) * rise;
            centered_text(&pop.head, w * 0.5, y, callout(size, fade(pop.color, alpha)));
            let theme = prompt_theme(alpha);
            let percent;
            let multiplier;
            let detail: &[Piece] = match pop.detail {
                Detail::None => &[],
                Detail::Boost {
                    percent: p,
                    multiplier: m,
                } => {
                    percent = buf_small(format_args!("+{p}%"));
                    multiplier = buf_small(format_args!("×{m}"));
                    let bolt = Piece::Glyph(Glyph::Icon(Icon::Bolt), pop.color);
                    if m > 1 {
                        &[
                            bolt,
                            Piece::Text(&percent, INK),
                            Piece::Text(&multiplier, GOLD),
                        ]
                    } else {
                        &[bolt, Piece::Text(&percent, INK)]
                    }
                }
                Detail::ThreadHint => &[
                    Piece::Glyph(Glyph::Thread, AQUA),
                    Piece::Then,
                    Piece::Glyph(Glyph::Icon(Icon::Bolt), AQUA),
                ],
                Detail::Rocket => &[Piece::Glyph(Glyph::Icon(Icon::Rocket), GOLD)],
            };
            if !detail.is_empty() {
                draw_pieces(
                    detail,
                    w * 0.5 - pieces_width(detail, &theme) * 0.5,
                    y + 18.,
                    &theme,
                );
            }
        }
        if let Some(banner) = &juice.banner {
            let alpha = banner.alpha();
            let look = Entrance::pop(0.85).sample(banner.age, 0, self.ui.motion);
            let y = if short && !compact {
                172.
            } else {
                h * if compact { 0.30 } else { 0.27 }
            };
            let card_w = (w - 32.).min(440.);
            let card = kit::appear(Rect::new((w - card_w) * 0.5, y - 44., card_w, 78.), look);
            glass(card, 16., Some(banner.color), alpha);
            centered_text(
                &banner.head,
                w * 0.5,
                y,
                callout(32., fade(banner.color, alpha)),
            );
            // The lap time and its delta against the previous lap.
            let theme = prompt_theme(alpha);
            let time = clock(banner.time);
            let delta = banner.delta.map(signed);
            let mut row = [
                Piece::Glyph(Glyph::Icon(Icon::Clock), MUTED),
                Piece::Text(&time, INK),
                Piece::Then,
            ];
            let n = if let Some(delta) = &delta {
                row[2] = Piece::Text(delta, banner.sub_color);
                3
            } else {
                2
            };
            let row = &row[..n];
            draw_pieces(
                row,
                w * 0.5 - pieces_width(row, &theme) * 0.5,
                y + 18.,
                &theme,
            );
        }
        if let Some(toast) = &juice.toast {
            let alpha = toast.alpha();
            let look = Entrance::fade_up(10.).sample(toast.age, 0, self.ui.motion);
            let y = if compact {
                if h < 460. && self.race.countdown > 0. {
                    116.
                } else {
                    104.
                }
            } else if self.race.streak >= 2 && w < 1000. {
                126.
            } else {
                100.
            } + look.offset[1];
            let style = modern(16., fade(INK, alpha));
            let tag = modern(16., fade(AQUA, alpha));
            let tag_w = text_width("KDUNE", tag) + 22.;
            let available = (w - 32.).min(820.);
            let (first, second) = split_lines(&toast.line, available - tag_w - 36., style);
            let lines_w = text_width(first, style).max(second.map_or(0., |s| text_width(s, style)));
            let pill_w = (lines_w + tag_w + 36.).min(available);
            let pill_h = if second.is_some() { 50. } else { 30. };
            let pill = Rect::new((w - pill_w) * 0.5, y - 20., pill_w, pill_h);
            glass(pill, 15., None, alpha);
            let dot = vec2(pill.x + 16., pill.y + 15.);
            shapes::fill_circle(dot, 4., fade(Color::new(1., 0.36, 0.3, 1.), alpha));
            let base = style.centered_baseline(pill.y + 15.);
            draw_text_styled("KDUNE", pill.x + 26., base, tag);
            let x = pill.x + tag_w + 18.;
            draw_text_styled(first, x, base, style);
            if let Some(second) = second {
                draw_text_styled(second, x, base + 20., style);
            }
        }
    }

    pub fn pause_card(&self) {
        let race = self.race;
        let w = screen_width();
        let h = screen_height();
        let motion = self.ui.motion;
        let fade_in = (self.ui.pause_age / 0.18).min(1.);
        draw_rectangle(0., 0., w, h, Color::new(0.03, 0.03, 0.04, 0.45 * fade_in));
        let card_w = (w - 32.).min(440.);
        let card_h = 184.;
        let base = Rect::new(
            (w - card_w) * 0.5,
            ((h - card_h) * 0.5).max(64.),
            card_w,
            card_h,
        );
        let look = Entrance::fade_up(16.).sample(self.ui.pause_age, 0, motion);
        let theme = desert_theme().faded(look.alpha);
        let inner = kit::card(kit::appear(base, look), Some("Paused"), &theme);
        let cx = inner.x + inner.w * 0.5;
        let seed = race.gates_completed() as u32 + race.impacts;
        let quip = Entrance::fade_up(10.).sample(self.ui.pause_age, 1, motion);
        let style = modern(16., fade(INK, quip.alpha));
        let (first, second) = split_lines(quips::pause(seed), inner.w, style);
        centered_text(first, cx, inner.y + 18. + quip.offset[1], style);
        if let Some(second) = second {
            centered_text(second, cx, inner.y + 38. + quip.offset[1], style);
        }
        // The race so far: lap, clock and any running streak, as icons.
        let info = Entrance::fade_up(10.).sample(self.ui.pause_age, 2, motion);
        let lap = buf_small(format_args!("{}/{LAPS}", (race.laps + 1).min(LAPS)));
        let time = clock(race.elapsed);
        let streak = buf_small(format_args!("{}", race.streak));
        let status = [
            Piece::Glyph(Glyph::Icon(Icon::Flag), INK),
            Piece::Text(&lap, INK),
            Piece::Text(" ", INK),
            Piece::Glyph(Glyph::Icon(Icon::Clock), MUTED),
            Piece::Text(&time, INK),
            Piece::Text(" ", INK),
            Piece::Glyph(Glyph::Icon(Icon::Flame), GOLD),
            Piece::Text(&streak, GOLD),
        ];
        let status = if race.streak >= 2 {
            &status[..]
        } else {
            &status[..5]
        };
        let info_theme = prompt_theme(info.alpha);
        draw_pieces(
            status,
            cx - pieces_width(status, &info_theme) * 0.5,
            inner.y + 62. + info.offset[1],
            &info_theme,
        );
        let mut pause = [0; 256];
        let mut restart = [0; 256];
        let pause = binding_hint(self.input, "pause", &mut pause);
        let restart = binding_hint(self.input, "restart", &mut restart);
        let keys = Entrance::fade_up(10.).sample(self.ui.pause_age, 3, motion);
        let theme = desert_theme().faded(keys.alpha);
        let prompts = [
            [
                Piece::Key(pause),
                Piece::Glyph(Glyph::Icon(Icon::Play), theme.accent),
            ],
            [
                Piece::Key(restart),
                Piece::Glyph(Glyph::Icon(Icon::Restart), theme.accent),
            ],
            [
                Piece::Key("F2"),
                Piece::Glyph(Glyph::Icon(Icon::Gamepad), theme.accent),
            ],
        ];
        let gap = 24.;
        let total: f32 = prompts.iter().map(|p| pieces_width(p, &theme)).sum::<f32>() + gap * 2.;
        if total <= inner.w {
            let mut x = cx - total * 0.5;
            for prompt in &prompts {
                draw_pieces(prompt, x, inner.y + 108. + keys.offset[1], &theme);
                x += pieces_width(prompt, &theme) + gap;
            }
        } else {
            for (i, prompt) in prompts.iter().enumerate() {
                draw_pieces(
                    prompt,
                    cx - pieces_width(prompt, &theme) * 0.5,
                    inner.y + 96. + i as f32 * 30. + keys.offset[1],
                    &theme,
                );
            }
        }
    }

    /// Rectangle of the results card's retry button for the current screen.
    pub fn retry_rect() -> Rect {
        let w = screen_width();
        let h = screen_height();
        let (card, _) = finish_layout(w, h);
        let bw = (card.w - 64.).min(300.);
        Rect::new(card.x + (card.w - bw) * 0.5, card.y + card.h - 60., bw, 42.)
    }

    fn finish_card(&self) {
        let race = self.race;
        let results = self.results;
        let juice = self.juice;
        let motion = self.ui.motion;
        let w = screen_width();
        let h = screen_height();
        let time = race.elapsed;
        let medal = quips::medal_for(time);
        let rank = quips::rank(summary(race));
        let new_best = results.new_best(time);
        let (card, full) = finish_layout(w, h);
        let age = juice.finish_age;
        let enter = Entrance::fade_up(28.)
            .with_duration(0.4)
            .sample(age, 0, motion);
        draw_rectangle(
            0.,
            0.,
            w,
            h,
            Color::new(0.03, 0.02, 0.02, 0.4 * enter.alpha),
        );
        let header = if new_best && results.previous_best.is_some() {
            "New best!"
        } else {
            "Finish!"
        };
        let theme = desert_theme().faded(enter.alpha);
        let inner = kit::card(kit::appear(card, enter), Some(header), &theme);
        let cx = inner.x + inner.w * 0.5;
        let item = |i: usize| {
            Entrance::fade_up(12.)
                .with_stagger(0.06)
                .sample(age - 0.15, i, motion)
        };
        let title_color = if medal == Medal::Unranked {
            GOLD
        } else {
            medal_color(medal)
        };
        // Rank title with a medal glow behind it, and one line of radio.
        let a = item(0);
        let mut row = inner.y + 26. + a.offset[1];
        shapes::glow_disc(vec2(cx, row - 10.), 90., fade(title_color, 0.22 * a.alpha));
        centered_text(
            rank.title,
            cx,
            row,
            callout(
                if inner.w < 300. { 24. } else { 32. },
                fade(title_color, a.alpha),
            ),
        );
        if full {
            row += 26.;
            let style = modern(16., fade(MUTED, a.alpha));
            let (first, second) = split_lines(rank.quip, inner.w, style);
            centered_text(first, cx, row, style);
            if let Some(second) = second {
                row += 18.;
                centered_text(second, cx, row, style);
            }
        }
        // The medal and the rolling clock.
        let a = item(1);
        row += if full { 60. } else { 52. };
        let shown = self.ui.roll.shown() as f32 / 100.;
        let size = if inner.w < 300. { 40. } else { 48. };
        let clock_text = clock(shown);
        let clock_style = classic(size, fade(INK, a.alpha));
        let group = size + 12. + text_width(&clock_text, clock_style);
        let gx = cx - group * 0.5;
        if medal != Medal::Unranked {
            shapes::glow_disc(
                vec2(gx + size * 0.5, row - size * 0.4 + a.offset[1]),
                size * 0.9,
                fade(medal_color(medal), 0.3 * a.alpha),
            );
        }
        medal_glyph(
            vec2(gx + size * 0.5, row - size * 0.42 + a.offset[1]),
            size * 1.1,
            medal,
            a.alpha,
        );
        draw_text_styled(&clock_text, gx + size + 12., row + a.offset[1], clock_style);
        row += 24.;
        // When recoveries added time: the driving time and the penalty that
        // the clock above already includes. Impacts cost speed, not time.
        let a = item(2);
        let driving = clock(race.driving_seconds());
        let penalty = buf!("+{:.2}s", race.penalty_seconds());
        if race.recovery_count > 0 {
            let pieces = [
                Piece::Glyph(Glyph::Icon(Icon::Clock), MUTED),
                Piece::Text(&driving, INK),
                Piece::Glyph(Glyph::Icon(Icon::Restart), ORANGE),
                Piece::Text(&penalty, ORANGE),
            ];
            let t = prompt_theme(a.alpha);
            draw_pieces(
                &pieces,
                cx - pieces_width(&pieces, &t) * 0.5,
                row + a.offset[1],
                &t,
            );
        }
        row += if full { 22. } else { 14. };
        // Stat tiles: a pictogram over each number.
        let best_lap = race.best_lap().unwrap_or(0.);
        let threaded = buf!("{}", race.threaded);
        let streak = buf!("{}", race.best_streak);
        let scrapes = buf!("{}", race.scrapes);
        let impacts = buf!("{}", race.impacts);
        let recoveries = buf!("{}", race.recovery_count);
        let lap_clock = clock(best_lap);
        #[derive(Clone, Copy)]
        enum Mark {
            Icon(Icon, Color),
            Thread,
            Scrape,
            Impact,
        }
        let all_gates = (GATES * LAPS as usize) as f32;
        let stats: [(Mark, &str, Color); 6] = [
            (
                Mark::Icon(Icon::Star, GOLD),
                &lap_clock,
                if results.new_lap(race.best_lap()) {
                    GOLD
                } else {
                    INK
                },
            ),
            (
                Mark::Thread,
                &threaded,
                if race.perfect_laps > 0 { GOLD } else { INK },
            ),
            (Mark::Icon(Icon::Flame, GOLD), &streak, INK),
            (Mark::Scrape, &scrapes, INK),
            (Mark::Impact, &impacts, INK),
            (Mark::Icon(Icon::Restart, ORANGE), &recoveries, INK),
        ];
        let rows = if full { 2 } else { 1 };
        let col_w = inner.w / 3.;
        for (i, (mark, value, color)) in stats.iter().take(rows * 3).enumerate() {
            let a = item(3 + i);
            let x = inner.x + col_w * ((i % 3) as f32 + 0.5);
            let y = row + (i / 3) as f32 * 58. + a.offset[1];
            let ic = vec2(x, y + 8.);
            match *mark {
                Mark::Icon(kind, tint) => icon(kind, ic, 22., fade(tint, a.alpha)),
                Mark::Thread => {
                    // The bullseye inside a ring that fills with gates threaded.
                    shapes::stroke_arc(ic, 14., 0., PI * 2., Stroke::new(3., fade(TRACK, a.alpha)));
                    shapes::stroke_arc(
                        ic,
                        14.,
                        -PI * 0.5,
                        PI * 2. * race.threaded as f32 / all_gates,
                        Stroke::new(3., fade(GOLD, a.alpha)).cap(Cap::Round),
                    );
                    gate_glyph(ic, 16., fade(AQUA, a.alpha), true);
                }
                Mark::Scrape => scrape_glyph(ic, 28., fade(ORANGE, a.alpha)),
                Mark::Impact => impact_glyph(ic, 28., fade(ROSE, a.alpha)),
            }
            let value_size = if col_w < 100. { 16. } else { 24. };
            centered_text(
                value,
                x,
                y + 44.,
                classic(value_size, fade(*color, a.alpha)),
            );
        }
        row += rows as f32 * 58. + 18.;
        let a = item(9);
        let t = prompt_theme(a.alpha);
        if full {
            // Three lap times; the quickest in gold.
            let laps: [Buf<16>; 3] = std::array::from_fn(|i| clock(race.lap_times[i]));
            let fastest = (0..3)
                .min_by(|&i, &j| race.lap_times[i].total_cmp(&race.lap_times[j]))
                .unwrap_or(0);
            let lap_color = |i: usize| if i == fastest { GOLD } else { MUTED };
            let pieces = [
                Piece::Glyph(Glyph::Icon(Icon::Flag), MUTED),
                Piece::Text(&laps[0], lap_color(0)),
                Piece::Text("·", MUTED),
                Piece::Text(&laps[1], lap_color(1)),
                Piece::Text("·", MUTED),
                Piece::Text(&laps[2], lap_color(2)),
            ];
            draw_pieces(
                &pieces,
                cx - pieces_width(&pieces, &t) * 0.5,
                row - 5. + a.offset[1],
                &t,
            );
            row += 30.;
        }
        // Personal best as a crown chip.
        let delta = results.previous_best.map(|best| signed(time - best));
        let old = results.previous_best.map(clock);
        let best: [Piece; 3] = match (results.demo, &old, &delta) {
            (true, ..) => [
                Piece::Glyph(Glyph::Icon(Icon::Gamepad), MUTED),
                Piece::Text("DEMO", MUTED),
                Piece::Then,
            ],
            (false, Some(old), Some(delta)) if new_best => [
                Piece::Glyph(Glyph::Icon(Icon::Crown), GOLD),
                Piece::Text(delta, GOLD),
                Piece::Text(old, MUTED),
            ],
            (false, Some(old), Some(delta)) => [
                Piece::Glyph(Glyph::Icon(Icon::Crown), MUTED),
                Piece::Text(old, INK),
                Piece::Text(delta, ROSE),
            ],
            _ => [
                Piece::Glyph(Glyph::Icon(Icon::Crown), GOLD),
                Piece::Text("NEW BEST", GOLD),
                Piece::Then,
            ],
        };
        let best = if matches!(best[2], Piece::Then) {
            &best[..2]
        } else {
            &best[..]
        };
        draw_pieces(
            best,
            cx - pieces_width(best, &t) * 0.5,
            row - 5. + a.offset[1],
            &t,
        );
        row += 32.;
        // The next goal, as icons.
        let cut;
        let target;
        let all = buf_small(format_args!("{}", GATES * LAPS as usize));
        let goal: [Piece; 2] = match next_goal(race, results) {
            Goal::Medal { medal, cut: c } => {
                cut = buf_small(format_args!("-{c:.2}s"));
                [
                    Piece::Glyph(Glyph::Medal(medal), WHITE),
                    Piece::Text(&cut, MINT),
                ]
            }
            Goal::Mirage => [
                Piece::Glyph(Glyph::Thread, GOLD),
                Piece::Text("MIRAGE LAP", GOLD),
            ],
            Goal::AllGates(_) => [Piece::Glyph(Glyph::Thread, GOLD), Piece::Text(&all, GOLD)],
            Goal::Beat(t) => {
                target = clock(t);
                [
                    Piece::Glyph(Glyph::Icon(Icon::Crown), MINT),
                    Piece::Text(&target, MINT),
                ]
            }
        };
        let goal_theme = prompt_theme(a.alpha);
        let gw = pieces_width(&goal, &goal_theme);
        let pill = Rect::new(cx - gw * 0.5 - 14., row - 18. + a.offset[1], gw + 28., 30.);
        shapes::fill_rounded_rect(pill, 15., fade(Color::new(1., 1., 1., 0.06), a.alpha));
        draw_pieces(&goal, pill.x + 14., pill.y + 15., &goal_theme);
        // One more run: the restart icon and its live key on the button.
        let mut retry = [0; 256];
        let retry = binding_hint(self.input, "restart", &mut retry);
        let a = item(10);
        let button = kit::appear(Self::retry_rect(), a);
        let button_theme = desert_theme().faded(a.alpha);
        kit::button(
            button,
            "",
            ButtonKind::Primary,
            &self.ui.retry,
            &button_theme,
        );
        let ink = fade(button_theme.accent_ink, a.alpha);
        let style = TextStyle::modern(16., ink);
        let label_w = 26. + text_width(retry, style);
        let c = button.center();
        icon(
            Icon::Restart,
            vec2(c.x - label_w * 0.5 + 10., c.y),
            20.,
            ink,
        );
        draw_text_styled(
            retry,
            c.x - label_w * 0.5 + 26.,
            style.centered_baseline(c.y),
            style,
        );
    }
}

/// Results card rectangle and whether it has room for every row.
fn finish_layout(w: f32, h: f32) -> (Rect, bool) {
    let full = h >= 600.;
    let card_w = (w - 32.).min(560.);
    let card_h: f32 = if full { 520. } else { 360. };
    let y = ((h - card_h) * 0.5).max(8.);
    (
        Rect::new((w - card_w) * 0.5, y, card_w, card_h.min(h - 16.)),
        full,
    )
}

/// Warm desert kit theme: dark warm-teal glass, sunset accent.
pub(super) fn desert_theme() -> KitTheme {
    KitTheme {
        backdrop: Color::new(0.03, 0.02, 0.02, 0.55),
        surface: Color::new(0.07, 0.09, 0.10, 0.96),
        raised: Color::new(0.15, 0.18, 0.19, 0.98),
        header: Color::new(0.16, 0.20, 0.21, 1.0),
        ink: INK,
        muted: MUTED,
        accent: Color::new(1.0, 0.70, 0.32, 1.0),
        accent_ink: Color::new(0.16, 0.08, 0.02, 1.0),
        focus: AQUA,
        line: LINE,
        ..KitTheme::default()
    }
}

/// Title-screen layout: logo baseline, chip top and prompt top.
pub(super) fn title_layout(w: f32, h: f32) -> (f32, f32, f32) {
    let logo = (h * 0.30).clamp(110., 260.);
    let prompt = if h < 460. { h - 70. } else { h * 0.74 };
    let _ = w;
    (logo, logo + 40., prompt)
}

/// The front door: an animated wordmark over the orbiting craft, the best
/// time (or the gold target) as a chip, and a pulsing start prompt.
pub(super) fn title_screen(ui: &UiMotion, input: &PlayerInput, best: Option<f32>, alpha: f32) {
    let w = screen_width();
    let h = screen_height();
    let motion = ui.motion;
    let t = ui.title_age;
    let (logo_y, sub_y, prompt_y) = title_layout(w, h);
    // Letterbox bars give the attract shot a cinematic frame.
    let bar = (h * 0.07).min(48.) * alpha;
    draw_rectangle(0., 0., w, bar, Color::new(0.02, 0.02, 0.02, 0.85));
    draw_rectangle(0., h - bar, w, bar, Color::new(0.02, 0.02, 0.02, 0.85));
    let size = if w < 420. {
        32.
    } else if w < 900. {
        48.
    } else {
        64.
    };
    let words = ["DUNE", "SKIMMER"];
    let style = callout(size, fade(INK, alpha));
    let space = size * 0.5;
    let total: f32 = words.iter().map(|w| text_width(w, style)).sum::<f32>() + space;
    let stacked = total > w - 32.;
    let mut index = 0;
    for (line, word) in words.iter().enumerate() {
        let word_w = text_width(word, style);
        let (mut x, y) = if stacked {
            (
                (w - word_w) * 0.5,
                logo_y - size * 0.6 + line as f32 * size * 1.15,
            )
        } else {
            (
                (w - total) * 0.5
                    + if line == 1 {
                        text_width(words[0], style) + space
                    } else {
                        0.
                    },
                logo_y,
            )
        };
        for (i, ch) in word.char_indices() {
            let glyph = &word[i..i + ch.len_utf8()];
            // Staggered entrance, then a gentle heat-haze bob per letter.
            let look = Entrance::fade_up(24.)
                .with_stagger(0.045)
                .sample(t, index, motion);
            let bob = if motion.is_reduced() {
                0.
            } else {
                (t * 2.2 - index as f32 * 0.45).sin() * 2.5
            };
            let color = if line == 0 {
                Color::new(1., 0.80, 0.44, 1.)
            } else {
                INK
            };
            draw_text_styled(
                glyph,
                x,
                y + look.offset[1] + bob,
                callout(size, fade(color, look.alpha * alpha)),
            );
            x += text_width(glyph, style) + style.pixel();
            index += 1;
        }
    }
    // The record to beat: a crown and the stored best with its medal, or
    // the gold medal and its target time.
    let sub_y = if stacked { sub_y + size * 0.7 } else { sub_y };
    let chip_look = Entrance::fade_up(10.).sample(t - 0.6, 0, motion);
    let a = chip_look.alpha * alpha;
    let theme = prompt_theme(a);
    let time = clock(best.unwrap_or(quips::MEDALS[0]));
    let chip: [Piece; 3] = match best {
        Some(best) => [
            Piece::Glyph(Glyph::Icon(Icon::Crown), GOLD),
            Piece::Text(&time, GOLD),
            Piece::Glyph(Glyph::Medal(quips::medal_for(best)), WHITE),
        ],
        None => [
            Piece::Glyph(Glyph::Medal(Medal::Gold), WHITE),
            Piece::Text(&time, GOLD),
            Piece::Then,
        ],
    };
    let chip = if best.is_some() {
        &chip[..]
    } else {
        &chip[..2]
    };
    let chip_w = pieces_width(chip, &theme) + 28.;
    let chip_rect = Rect::new(
        (w - chip_w) * 0.5,
        sub_y - 4. + chip_look.offset[1],
        chip_w,
        30.,
    );
    glass(chip_rect, 15., None, a);
    draw_pieces(chip, chip_rect.x + 14., chip_rect.y + 15., &theme);
    // Pulsing start prompt with real bindings: key caps and a play arrow.
    let prompt = Entrance::fade_up(12.).sample(t - 0.8, 0, motion);
    let pulse = if motion.is_reduced() {
        1.
    } else {
        0.75 + 0.25 * (t * 3.2).sin()
    };
    let a = prompt.alpha * alpha;
    let theme = KitTheme {
        raised: Color::new(1.0, 0.72, 0.36, 1.),
        header: Color::new(1.0, 0.82, 0.5, 1.),
        surface: Color::new(0.5, 0.3, 0.12, 1.),
        ink: Color::new(0.16, 0.08, 0.02, 1.),
        muted: INK,
        ..desert_theme()
    }
    .faded(a * pulse);
    let mut confirm = [0; 256];
    let mut drive = [0; 256];
    let confirm = binding_hint(input, "confirm", &mut confirm);
    let drive = binding_hint(input, "move_forward", &mut drive);
    let play = Piece::Glyph(Glyph::Icon(Icon::Play), GOLD);
    let pieces: &[Piece] = if input.touch_seen() {
        &[Piece::Glyph(Glyph::Icon(Icon::Hand), INK), play]
    } else {
        &[Piece::Key(confirm), Piece::Key(drive), play]
    };
    let total = pieces_width(pieces, &theme);
    let pill = Rect::new(
        (w - total - 40.) * 0.5,
        prompt_y - 6. + prompt.offset[1],
        total + 40.,
        40.,
    );
    glass(pill, 20., Some(GOLD), a);
    draw_pieces(pieces, pill.x + 20., pill.y + 20., &theme);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time_label;

    #[test]
    fn next_goal_always_names_a_concrete_target() {
        let mut race = Race::new();
        race.finished = true;
        let results = Results::default();
        race.elapsed = 40.;
        let Goal::Medal { medal, .. } = next_goal(&race, &results) else {
            panic!("an unranked run chases bronze");
        };
        assert_eq!(medal, Medal::Bronze);
        race.elapsed = 30.;
        let Goal::Medal { medal, cut } = next_goal(&race, &results) else {
            panic!("silver chases gold");
        };
        assert_eq!(medal, Medal::Gold);
        assert!((cut - 0.5).abs() < 1e-4, "{cut}");
        race.elapsed = 29.;
        assert_eq!(next_goal(&race, &results), Goal::Mirage);
        race.perfect_laps = 1;
        assert_eq!(next_goal(&race, &results), Goal::AllGates(42));
        race.threaded = 42;
        let Goal::Beat(target) = next_goal(&race, &results) else {
            panic!("a perfect gold run chases its own time");
        };
        assert_eq!(&*clock(target), "00:28.75");
    }

    #[test]
    fn personal_bests_ignore_demo_runs_and_ties() {
        let results = Results {
            previous_best: Some(31.),
            previous_lap: Some(10.),
            demo: false,
        };
        assert!(results.new_best(30.9));
        assert!(!results.new_best(31.));
        assert!(results.new_lap(Some(9.9)) && !results.new_lap(None));
        let demo = Results {
            demo: true,
            ..results
        };
        assert!(!demo.new_best(1.) && !demo.new_lap(Some(1.)));
        assert!(Results::default().new_best(99.));
    }

    #[test]
    fn pace_waits_for_a_lap_then_projects_the_remaining_gates() {
        let mut race = Race::new();
        race.elapsed = 8.;
        race.next_gate = 12;
        assert_eq!(pace(&race), None, "lap one has no honest pace");
        race.laps = 1;
        race.next_gate = 1;
        race.lap_times[0] = 9.7;
        race.elapsed = 9.7; // 9.7 + 2 * 9.7 = 29.1
        assert_eq!(pace(&race), Some(Medal::Gold));
        race.lap_times[0] = 10.5;
        race.elapsed = 10.5; // 31.5
        assert_eq!(pace(&race), Some(Medal::Silver));
        race.elapsed = 14.; // A slow start to lap two shows up immediately.
        assert_eq!(pace(&race), Some(Medal::Bronze));
        race.finished = true;
        assert_eq!(pace(&race), None);
    }

    #[test]
    fn top_right_menu_corner_stays_clear_at_every_reviewed_size() {
        let corner_clear =
            |w: f32, right: f32, top: f32| top >= MENU_CORNER.1 || right <= w - MENU_CORNER.0;
        let timer = time_label(599.99);
        // Worst-case sub row: clock icon and lap time, then the pace medal.
        let lap = 20. + text_width("09:59.99", modern(16., INK));
        let pace = 18.;
        for w in [1280., 1600., 844., 768., 700., 600.] {
            let gate = wide_gate_plate(w);
            assert!(
                corner_clear(w, gate.x + gate.w, gate.y),
                "gate plate at {w}"
            );
            let count_right = gate.x + gate.w - 14. + 1.;
            assert!(count_right <= w - MENU_CORNER.0, "gate count at {w}");
            // The arch glyph sits left of the widest count.
            assert!(
                gate.x + 25. + 10. + 8. <= gate.x + gate.w - 14. - classic_width("14 / 14", 14),
                "gate glyph meets the count at {w}"
            );
            let r = wide_streak_chip(w, 99);
            assert!(corner_clear(w, r.x + r.w, r.y), "streak chip at {w}");
            let sub = if mid_width(w) { pace } else { lap + 14. + pace };
            let [lap_plate, clock_plate, plate] = wide_plates(w, sub);
            assert_eq!(plate, gate);
            assert!(
                lap_plate.x + lap_plate.w + 8. <= clock_plate.x,
                "lap/clock touch at {w}"
            );
            assert!(
                clock_plate.x + clock_plate.w + 8. <= gate.x,
                "clock/gate touch at {w}"
            );
            if !mid_width(w) {
                // Star and the widest best lap fit the lap plate's column.
                let best = 24. + text_width("09:59.99", modern(16., INK));
                assert!(92. + best <= lap_plate.w, "best lap column at {w}");
            }
        }
        for w in [390., 320., 359., 360., 599.] {
            let gate_right = compact_gate_x(w, GATES - 1) + compact_gate_width(GATES - 1) + 2.;
            assert!(
                corner_clear(w, gate_right, 54. - 16.),
                "compact gate at {w}"
            );
            let timer_right = compact_timer_x(w, &timer) + classic_width(&timer, 20) + 1.;
            assert!(
                corner_clear(w, timer_right, 28. - 20.),
                "compact timer at {w}"
            );
            let margin = (w * 0.035).clamp(18.0, 44.0);
            // The flag and lap count stay clear of the timer.
            let lap_right = margin + COMPACT_FLAG + classic_width("3/3", 14);
            assert!(
                lap_right + 8. <= compact_timer_x(w, &timer),
                "lap meets timer at {w}"
            );
            // The compact multiplier leads the gate row without touching it.
            assert!(
                margin + 17. + text_width("×5", modern(16., INK)) + 8.
                    <= compact_gate_x(w, GATES - 1),
                "{w}"
            );
        }
    }

    #[test]
    fn results_card_fits_every_reviewed_viewport() {
        for (w, h) in [
            (1280., 800.),
            (1600., 1000.),
            (390., 844.),
            (844., 390.),
            (320., 480.),
        ] {
            let (card, _) = finish_layout(w, h);
            assert!(card.x >= 8. && card.x + card.w <= w - 8.);
            assert!(card.y >= 8. && card.y + card.h <= h - 8.);
            // The card never reaches into the menu corner.
            assert!(card.y >= MENU_CORNER.1 || card.x + card.w <= w - MENU_CORNER.0 || h < 400.);
        }
    }

    #[test]
    fn prompt_rows_measure_their_key_caps_icons_and_numbers() {
        let theme = prompt_theme(1.);
        let key = pieces_width(&[Piece::Key("Space")], &theme);
        assert!(key > text_width("Space", modern(16., INK)));
        let row = [
            Piece::Key("Space"),
            Piece::Glyph(Glyph::Icon(Icon::Bolt), AQUA),
        ];
        assert_eq!(
            pieces_width(&row, &theme),
            key + PIECE_GAP + theme.body * 1.4
        );
        assert_eq!(pieces_width(&[], &theme), 0.);
        // The widest alert fits the narrowest supported screen.
        let alert = [
            Piece::Glyph(Glyph::Icon(Icon::Alert), ORANGE),
            Piece::Text("OFF TRACK", ORANGE),
            Piece::Glyph(Glyph::Icon(Icon::Hourglass), ORANGE),
            Piece::Text("2.5s", INK),
        ];
        assert!(pieces_width(&alert, &theme) + 32. <= 320. - 16.);
    }
}
