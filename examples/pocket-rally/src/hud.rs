//! Screen-space HUD, banners, countdown, pause and results cards.
//!
//! Nothing here advances the race or writes records. The play screen speaks in
//! pictures and numbers: laps are gold pips, the sixteen gates a segmented
//! meter whose next segment pulses amber like the arrows on the road, speed a
//! gauge, and the race clock rides beside a medal that changes colour as each
//! medal target slips away. Callouts are an icon and at most three words, and
//! key caps sit beside the pictogram of what they do. Copy uses the Modern
//! pixel face; big numerals and arcade callouts keep the Classic face for
//! retro punch. Surfaces are anti-aliased rounded panels with soft shadows.
//! Every label is measured, fitted and kept inside its panel at desktop,
//! portrait and short landscape sizes, and steady-state frames format into
//! stack buffers instead of allocating.
use crate::juice::{AMBER, CREAM, GOLD, GREEN, Juice, LapDetail, MUTED, RED, WATER};
use crate::presentation::{LapCompare, hint_for};
use crate::quips::{self, Hook};
use crate::session::RaceSession;
use crate::sim::{GATES, LAPS, Pose, Race, RoadCue, Track};
use crate::style::Style;
use ferravex_play::gameplay::trial::Medal;
use ferravex_play::tween::{Ease, Entrance, Motion};
use ferravex_render::icons::{self, Icon, IconStyle};
use ferravex_render::input::{HintDevice, PlayerInput};
use ferravex_render::post;
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Cap, Fill, Join, Shadow, Stroke};
use ferravex_render::text::{self, TextStyle, draw_text_styled};
use ferravex_render::ui::kit::{self, KitTheme};
use std::f32::consts::{FRAC_PI_2, PI};
use std::fmt::{self, Write as _};

pub const INK: Color = Color::new(0.12, 0.19, 0.17, 1.);
/// HUD chip fill. Browser glyph tests locate the bar by this exact colour.
const BAR: Color = Color::new(0.12, 0.19, 0.17, 0.78);
/// Countdown and results panels. Browser glyph tests locate them by colour.
const PANEL: Color = Color::new(0.12, 0.19, 0.17, 0.94);
const SURFACE: Color = Color::new(0.11, 0.17, 0.155, 0.92);
const SHADOW: Color = Color::new(0.02, 0.05, 0.04, 0.45);
const TEXT_SHADOW: Color = Color::new(0.02, 0.05, 0.04, 0.6);
const OUTLINE: Color = Color::new(0.07, 0.10, 0.09, 0.95);
const HAIRLINE: Color = Color::new(1., 1., 1., 0.08);
const LAMP_OFF: Color = Color::new(0.24, 0.27, 0.25, 1.);
const LAMP_RED: Color = Color::new(1.0, 0.32, 0.26, 1.);
/// Cleared gate segments. Deliberately not [`CREAM`]: the browser glyph test
/// counts cream runs in the bar, and those must stay the three numbers.
const LIT: Color = Color::new(0.99, 0.95, 0.85, 1.);
/// Secondary cream ink for numbers on cards (the kit theme's ink).
const PAPER: Color = Color::from_hex(0xf4efe2);
const DIM: Color = Color::new(1., 1., 1., 0.16);

// ---------------------------------------------------------------------------
// Allocation-free formatting.

/// A short UTF-8 line formatted on the stack. Text that does not fit is
/// dropped piece by piece, so the content always stays valid UTF-8.
#[derive(Clone, Copy)]
pub struct Line {
    buf: [u8; 96],
    len: usize,
}

impl Line {
    pub const fn new() -> Self {
        Self {
            buf: [0; 96],
            len: 0,
        }
    }
    pub fn of(args: fmt::Arguments) -> Self {
        let mut line = Self::new();
        let _ = line.write_fmt(args);
        line
    }
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl Default for Line {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Write for Line {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let end = self.len + s.len();
        if end > self.buf.len() {
            return Err(fmt::Error);
        }
        self.buf[self.len..end].copy_from_slice(s.as_bytes());
        self.len = end;
        Ok(())
    }
}

impl std::ops::Deref for Line {
    type Target = str;
    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for Line {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Line {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Debug::fmt(self.as_str(), f)
    }
}

impl PartialEq<&str> for Line {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

macro_rules! line {
    ($($arg:tt)*) => { $crate::hud::Line::of(format_args!($($arg)*)) };
}

/// Stopwatch text from whole centiseconds, so 59.999 s never reads "00:60.00".
pub fn clock(seconds: f32) -> Line {
    // Round to whole milliseconds first: accumulated f32 tick time can land a
    // hair under an exact value, which must not read one centisecond short.
    let millis = (f64::from(seconds.max(0.)) * 1000.).round() as u64;
    let centis = millis / 10;
    line!(
        "{:02}:{:02}.{:02}",
        centis / 6000,
        centis / 100 % 60,
        centis % 100
    )
}

// ---------------------------------------------------------------------------
// Type and surfaces.

pub fn fade(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0., 1.))
}

/// Modern body face with a soft one-pixel shadow.
pub fn modern(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color).with_shadow(fade(TEXT_SHADOW, color.a))
}

/// Classic arcade face for numerals and callouts, outlined for any backdrop.
pub fn classic(size: f32, color: Color) -> TextStyle {
    TextStyle::classic(size, color).with_outline(fade(OUTLINE, color.a))
}

pub fn width(s: &str, style: TextStyle) -> f32 {
    text::text_width(s, style)
}

/// Draws `s` with its left edge at `x`.
pub fn put(s: &str, x: f32, y: f32, style: TextStyle) {
    draw_text_styled(s, x, y, style);
}

/// Centered text, scaled down only when it would exceed `max_width`.
pub fn put_centered(s: &str, cx: f32, y: f32, style: TextStyle, max_width: f32) {
    let style = fit(s, style, max_width);
    draw_text_styled(s, cx - width(s, style) / 2., y, style);
}

fn fit(s: &str, style: TextStyle, max_width: f32) -> TextStyle {
    let w = width(s, style);
    if w > max_width && w > 0. {
        style.with_size(style.size * (max_width / w).max(0.1))
    } else {
        style
    }
}

/// A raised rounded surface with a soft shadow.
fn surface(rect: Rect, radius: f32, fill: Color, alpha: f32) {
    shapes::drop_shadow(
        rect,
        radius,
        Shadow::soft(fade(SHADOW, alpha))
            .offset(vec2(0., 5.))
            .blur(18.),
    );
    shapes::fill_rounded_rect(rect, radius, fade(fill, alpha));
    shapes::stroke_rounded_rect(rect, radius, 1., fade(HAIRLINE, alpha));
}

/// The rally kit palette: deep pine surfaces, cream ink, sunflower accents.
pub fn theme() -> KitTheme {
    KitTheme {
        surface: Color::from_hex(0x1c2b27),
        raised: Color::from_hex(0x2a3d37),
        header: Color::from_hex(0x34504a),
        ink: PAPER,
        muted: Color::from_hex(0xb6c3b3),
        accent: Color::from_hex(0xffc857),
        accent_ink: Color::from_hex(0x2a1d08),
        success: Color::from_hex(0x8be38f),
        danger: Color::from_hex(0xff7a66),
        focus: Color::from_hex(0x9ad9ff),
        ..KitTheme::default()
    }
}

// ---------------------------------------------------------------------------
// Pictograms. Engine icons where the shared set has the idea; the rally's own
// medal, star, tick, drift and gate glyphs are drawn here so they stay exact.

/// A HUD icon with a soft drop shadow so it reads on grass and asphalt.
pub fn icon(icon: Icon, at: Vec2, size: f32, color: Color) {
    icons::draw_styled(
        icon,
        at,
        size,
        IconStyle::new(color).shadow(fade(TEXT_SHADOW, color.a * 0.8)),
    );
}

fn shade(c: Color, k: f32) -> Color {
    Color::new(c.r * k, c.g * k, c.b * k, c.a)
}

fn lighten(c: Color, t: f32) -> Color {
    let m = shapes::mix(c, Color::new(1., 1., 1., c.a), t);
    Color::new(m.r, m.g, m.b, c.a)
}

/// A five-point star, the rally's mark for a best lap.
pub fn star(c: Vec2, r: f32, color: Color) {
    let points: [Vec2; 10] = std::array::from_fn(|i| {
        let a = -FRAC_PI_2 + i as f32 * PI / 5.;
        c + Vec2::from_angle(a) * if i % 2 == 0 { r } else { r * 0.46 }
    });
    shapes::fill_polygon(&points, color);
}

/// A rounded tick: done, clean.
pub fn tick(c: Vec2, size: f32, color: Color) {
    shapes::polyline(
        &[
            c + vec2(-0.42, 0.02) * size,
            c + vec2(-0.12, 0.3) * size,
            c + vec2(0.42, -0.3) * size,
        ],
        Stroke::new(size * 0.2, color)
            .cap(Cap::Round)
            .join(Join::Round),
    );
}

/// A medal on a two-tone ribbon, starred in its own colour. Unranked is a
/// plain grey disc.
pub fn medal_glyph(c: Vec2, size: f32, medal: Medal, alpha: f32) {
    let color = medal_color(medal);
    let r = size * 0.34;
    let disc = c + vec2(0., size * 0.14);
    let top = c.y - size * 0.5;
    if medal != Medal::Unranked {
        let left = fade(Color::new(0.86, 0.33, 0.28, 1.), alpha);
        let right = fade(Color::new(0.30, 0.52, 0.82, 1.), alpha);
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
            fade(shade(color, 0.78), alpha),
        ),
    );
    shapes::stroke_circle(
        disc,
        r * 0.76,
        (size * 0.045).max(1.),
        fade(shade(color, 0.68), alpha),
    );
    if medal != Medal::Unranked {
        star(disc, r * 0.5, fade(shade(color, 0.62), alpha));
    }
}

/// The handbrake drift, drawn like a traction-control lamp: a little car
/// above two wavy skid marks. The engine set has no car, so the rally draws
/// its own.
pub fn drift_glyph(c: Vec2, size: f32, color: Color) {
    let s = size * 0.5;
    let c = c - vec2(0., s * 0.2);
    let wheels = Color::new(color.r * 0.5, color.g * 0.5, color.b * 0.5, color.a);
    // Cabin, body and wheels.
    shapes::fill_polygon(
        &[
            c + vec2(-0.5, -0.12) * s,
            c + vec2(-0.28, -0.56) * s,
            c + vec2(0.3, -0.56) * s,
            c + vec2(0.56, -0.12) * s,
        ],
        color,
    );
    shapes::fill_rounded_rect(
        Rect::new(c.x - 0.86 * s, c.y - 0.2 * s, 1.72 * s, 0.46 * s),
        0.2 * s,
        color,
    );
    for x in [-0.48, 0.48] {
        shapes::fill_circle(c + vec2(x, 0.27) * s, 0.17 * s, wheels);
    }
    // Two wavy skid marks underneath.
    for x in [-0.42, 0.42] {
        shapes::polyline(
            &[
                c + vec2(x - 0.14, 0.56) * s,
                c + vec2(x + 0.1, 0.72) * s,
                c + vec2(x - 0.1, 0.9) * s,
                c + vec2(x + 0.14, 1.06) * s,
            ],
            Stroke::new(0.13 * s, color)
                .cap(Cap::Round)
                .join(Join::Round),
        );
    }
}

/// A small chevron like the amber gate markers on the road.
fn chevron(c: Vec2, size: f32, color: Color) {
    shapes::fill_triangle(
        c + vec2(-size * 0.35, -size * 0.45),
        c + vec2(size * 0.45, 0.),
        c + vec2(-size * 0.35, size * 0.45),
        color,
    );
}

/// A bold arrow head pointing left (`dir` < 0) or right, for touch buttons.
fn arrow_glyph(c: Vec2, size: f32, dir: f32, color: Color) {
    let s = size * 0.5;
    shapes::fill_polygon(
        &[
            c + vec2(-s * 0.5 * dir, -s * 0.8),
            c + vec2(s * 0.7 * dir, 0.),
            c + vec2(-s * 0.5 * dir, s * 0.8),
            c + vec2(-s * 0.2 * dir, 0.),
        ],
        color,
    );
}

/// What a key cap does, drawn as a pictogram instead of a word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Icon(Icon),
    /// The handbrake slide.
    Drift,
}

impl Glyph {
    fn draw(self, c: Vec2, size: f32, color: Color) {
        match self {
            Glyph::Icon(i) => icon(i, c, size, color),
            Glyph::Drift => drift_glyph(c, size, color),
        }
    }
}

fn key_glyph_width(key: &str, theme: &KitTheme) -> f32 {
    let style = TextStyle::modern(theme.body, theme.ink);
    let h = theme.body * 1.5 + 2.;
    (width(key, style) + kit::SPACE[2] * 2.).max(h - 3.) + kit::SPACE[1] + theme.body * 1.4
}

/// `[key] glyph` with its top-left at (`x`, `y`); returns the width.
fn key_glyph(x: f32, y: f32, key: &str, glyph: Glyph, theme: &KitTheme) -> f32 {
    let cap = kit::keycap(x, y, key, theme);
    let size = theme.body * 1.4;
    let c = vec2(
        cap.x + cap.w + kit::SPACE[1] + size / 2.,
        y + (cap.h - 3.) / 2.,
    );
    glyph.draw(c, size, fade(theme.accent, theme.alpha));
    cap.w + kit::SPACE[1] + size
}

/// A centred row of key-cap prompts; one per row when too narrow.
fn glyph_prompts(
    items: &[(&str, Glyph)],
    cx: f32,
    y: f32,
    max_width: f32,
    theme: &KitTheme,
) -> f32 {
    let gap = kit::SPACE[4];
    let widths: [f32; 4] = std::array::from_fn(|i| {
        items
            .get(i)
            .map_or(0., |(key, _)| key_glyph_width(key, theme))
    });
    let total: f32 = widths.iter().sum::<f32>() + gap * (items.len().max(1) - 1) as f32;
    let row_h = theme.body * 1.5 + 8.;
    if total <= max_width {
        let mut x = cx - total / 2.;
        for (i, (key, glyph)) in items.iter().enumerate() {
            key_glyph(x, y, key, *glyph, theme);
            x += widths[i] + gap;
        }
        row_h
    } else {
        for (i, (key, glyph)) in items.iter().enumerate() {
            let row = y + i as f32 * row_h;
            key_glyph(cx - widths[i] / 2., row, key, *glyph, theme);
        }
        row_h * items.len() as f32
    }
}

/// Lap pips from `first` (the first pip's centre): finished laps gold with a
/// tick, the current lap a breathing amber ring, later laps dim rings.
fn lap_pips(first: Vec2, radius: f32, step: f32, race: &Race, time: f32, motion: f32, alpha: f32) {
    for i in 0..LAPS {
        let c = first + vec2(i as f32 * step, 0.);
        if i < race.laps {
            shapes::fill_circle(c, radius, fade(GOLD, alpha));
            tick(c, radius * 1.1, fade(INK, alpha));
        } else if i == race.laps && !race.finished() {
            let breathe = 0.5 + 0.5 * (time * 4.).sin() * motion;
            post::glow_circle(
                c.x,
                c.y,
                radius * 2.4,
                fade(AMBER, alpha * (0.25 + 0.25 * breathe)),
                0.6,
            );
            shapes::fill_circle(c, radius, fade(AMBER, alpha * (0.3 + 0.25 * breathe)));
            shapes::stroke_circle(c, radius - 1., 2., fade(AMBER, alpha));
        } else {
            shapes::stroke_circle(c, radius - 1., 2., fade(Color::new(1., 1., 1., 0.3), alpha));
        }
    }
}

/// One segment per gate this lap: cleared gates lit, the newest briefly
/// glowing, the next gate pulsing amber like its arrows on the road.
fn gate_meter(rect: Rect, passed: usize, finished: bool, flash: f32, time: f32, motion: f32) {
    let gap = if rect.w > 120. { 3. } else { 2. };
    let seg = (rect.w - gap * (GATES - 1) as f32) / GATES as f32;
    let radius = (seg * 0.3).min(2.);
    for i in 0..GATES {
        let cell = Rect::new(rect.x + i as f32 * (seg + gap), rect.y, seg, rect.h);
        if i < passed {
            let glow = if i + 1 == passed {
                (1. - flash / 0.5).clamp(0., 1.)
            } else {
                0.
            };
            if glow > 0. {
                let c = cell.center();
                post::glow_circle(c.x, c.y, 10. + 8. * glow, fade(AMBER, glow), 0.9);
            }
            shapes::fill_rounded_rect(cell, radius, LIT);
        } else if i == passed && !finished {
            let pulse = 0.5 + 0.5 * (time * 6.).sin() * motion;
            shapes::fill_rounded_rect(cell, radius, fade(AMBER, 0.55 + 0.45 * pulse));
        } else {
            shapes::fill_rounded_rect(cell, radius, DIM);
        }
    }
}

/// Device-aware labels for on-screen hints, read from the live bindings.
/// The host rebuilds these only when the device or bindings may change.
pub struct Hints {
    pub gas: String,
    pub drift: String,
    pub restart: String,
    pub pause: String,
    pub touch: bool,
    pub keyboard: bool,
}

impl Hints {
    pub fn new(input: &PlayerInput) -> Self {
        let device = input.current_device();
        Self {
            gas: hint_for(input, "move_forward", device),
            drift: hint_for(input, "brake", device),
            restart: hint_for(input, "restart", device),

            pause: hint_for(input, "pause", device),

            touch: device == HintDevice::Touch,
            keyboard: device == HintDevice::KeyboardMouse,
        }
    }
}

/// The world-to-screen transform of the current frame.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub width: f32,
    pub height: f32,
    pub center: Vec2,
    pub scale: f32,
}

impl View {
    pub fn screen(self, p: Vec2) -> Vec2 {
        (p - self.center) * self.scale + vec2(self.width, self.height) * 0.5
    }
}

/// Height reserved for the HUD chips: one row of chips when wide, two when narrow.
pub fn bar_height(width: f32) -> f32 {
    if width >= 640. { 72. } else { 100. }
}

fn speed_kmh(race: &Race) -> u32 {
    if race.finished() {
        0
    } else {
        (race.car.velocity.length() * 0.48).round() as u32
    }
}

// ---------------------------------------------------------------------------
// Top HUD chips.

/// Browser fullscreen pins a 44px menu button at the top-right corner (inset
/// 8px). HUD text and panels keep this `width - 72` by 60 region clear.
pub const MENU_CORNER: Vec2 = vec2(72., 60.);

/// One positioned HUD label; `y` is the text baseline.
#[derive(Clone, Copy, Debug)]
pub struct Label {
    pub text: Line,
    pub x: f32,
    pub y: f32,
    pub style: TextStyle,
}

impl Label {
    fn at(text: Line, x: f32, y: f32, style: TextStyle) -> Self {
        Self { text, x, y, style }
    }

    fn right(text: Line, right: f32, y: f32, style: TextStyle) -> Self {
        let x = right - width(&text, style);
        Self::at(text, x, y, style)
    }

    fn centered(text: Line, cx: f32, y: f32, style: TextStyle, max_width: f32) -> Self {
        let style = fit(&text, style, max_width);
        let x = cx - width(&text, style) / 2.;
        Self::at(text, x, y, style)
    }

    fn draw(&self) {
        put(&self.text, self.x, self.y, self.style);
    }

    /// Vertical centre of the label's capitals.
    fn mid_y(&self) -> f32 {
        self.y
            - self.style.size
                * if self.style.face == text::FontFace::Classic {
                    0.4375
                } else {
                    0.5
                }
    }

    /// Capital/digit box without shadow or descenders: what the browser
    /// glyph test measures for the bar's cream numbers.
    #[cfg(test)]
    pub fn caps(&self) -> Rect {
        let cap = if self.style.face == text::FontFace::Classic {
            self.style.size * 0.875
        } else {
            self.style.size
        };
        Rect::new(self.x, self.y - cap, width(&self.text, self.style), cap)
    }

    /// Glyph bounds including the shadow/outline and descenders. The Classic
    /// face has no descenders: its outline hugs seven-pixel capitals.
    #[cfg(test)]
    pub fn bounds(&self) -> Rect {
        let px = self.style.pixel();
        if self.style.face == text::FontFace::Classic {
            return Rect::new(
                self.x - px,
                self.y - 8. * px,
                width(&self.text, self.style) + 2. * px,
                9. * px,
            );
        }
        Rect::new(
            self.x - px,
            self.y - self.style.size - px,
            width(&self.text, self.style) + 3. * px,
            self.style.size * 1.25 + 3. * px,
        )
    }
}

/// HUD chip rectangles: (left, center, right) when wide; (main, strip) when narrow.
pub fn bar_chips(width: f32) -> [Option<Rect>; 3] {
    let right = width - MENU_CORNER.x - 2.;
    if width >= 640. {
        [
            Some(Rect::new(14., 8., 176., 60.)),
            Some(Rect::new(width / 2. - 120., 8., 240., 60.)),
            Some(Rect::new(right - 206., 8., 206., 60.)),
        ]
    } else {
        [
            Some(Rect::new(8., 8., right - 8., 56.)),
            Some(Rect::new(8., 68., width - 16., 26.)),
            None,
        ]
    }
}

/// The bar's text. Its cream runs are speed, lap time and race time in that
/// draw order (the browser glyph test reads exactly those three); laps and
/// gates are pips and a meter, the speed unit is muted, the split coloured.
#[derive(Clone, Copy, Debug)]
pub struct BarText {
    pub speed: Label,
    pub unit: Label,
    pub lap_time: Label,
    pub race_time: Label,
    pub delta: Option<Label>,
}

/// Room left of the lap timer for its clock icon.
const TIMER_ICON: f32 = 28.;

pub fn bar_labels(
    width: f32,
    speed: Line,
    lap_time: Line,
    race_time: Line,
    delta: Option<(Line, Color)>,
) -> BarText {
    let [a, b, c] = bar_chips(width);
    if let (Some(_), Some(mid), Some(right)) = (a, b, c) {
        let pad = 14.;
        let unit = Label::right(
            line!("km/h"),
            right.x + right.w - pad,
            35.,
            modern(13., MUTED),
        );
        let speed = Label::right(speed, unit.x - 5., 35., modern(24., CREAM));
        let race_time = Label::right(race_time, right.x + right.w - pad, 58., modern(16., CREAM));
        // Clock and timer centre as one group. The timer sits in the chip's
        // middle until a split delta claims the second row.
        let style = fit(&lap_time, classic(32., CREAM), mid.w - 20. - TIMER_ICON);
        let group = TIMER_ICON + width_of(&lap_time, style);
        let x = mid.x + (mid.w - group) / 2. + TIMER_ICON;
        let y = if delta.is_some() {
            42.
        } else {
            style.centered_baseline(mid.y + mid.h / 2.)
        };
        let lap_time = Label::at(lap_time, x, y, style);
        let delta = delta.map(|(text, color)| {
            Label::centered(
                text,
                mid.x + mid.w / 2.,
                62.,
                modern(16., color),
                mid.w - 16.,
            )
        });
        BarText {
            speed,
            unit,
            lap_time,
            race_time,
            delta,
        }
    } else {
        let main = a.expect("narrow chip");
        let strip = b.expect("narrow strip");
        let unit = Label::right(
            line!("km/h"),
            main.x + main.w - 10.,
            29.,
            modern(11., MUTED),
        );
        let speed = Label::right(speed, unit.x - 4., 29., modern(16., CREAM));
        let lap_style = classic(24., CREAM);
        let lap_x = main.x + 32.;
        let lap_right = lap_x + width_of(&lap_time, lap_style);
        let delta = delta.and_then(|(text, color)| {
            [16., 8.]
                .into_iter()
                .map(|size| modern(size, color))
                .find(|style| lap_right + 8. + width_of(&text, *style) <= main.x + main.w - 10.)
                .map(|style| Label::at(text, lap_right + 8., 55., style))
        });
        BarText {
            speed,
            unit,
            lap_time: Label::at(lap_time, lap_x, 57., lap_style),
            race_time: Label::right(race_time, strip.x + strip.w - 10., 88., modern(16., CREAM)),
            delta,
        }
    }
}

/// Where the lap pips and the gate meter go.
#[derive(Clone, Copy, Debug)]
pub struct BarMeters {
    pub flag: Vec2,
    pub flag_size: f32,
    pub first_pip: Vec2,
    pub pip_radius: f32,
    pub pip_step: f32,
    pub chevron: Vec2,
    pub gates: Rect,
}

impl BarMeters {
    /// Everything the meters paint, for layout tests.
    #[cfg(test)]
    fn bounds(&self) -> [Rect; 3] {
        let r = self.pip_radius;
        [
            Rect::new(
                self.flag.x - self.flag_size / 2.,
                self.flag.y - self.flag_size / 2.,
                self.first_pip.x + self.pip_step * (LAPS - 1) as f32 + r - self.flag.x
                    + self.flag_size / 2.,
                self.flag_size.max(2. * r),
            ),
            Rect::new(self.chevron.x - 5., self.chevron.y - 5., 10., 10.),
            self.gates,
        ]
    }
}

pub fn bar_meters(width: f32) -> BarMeters {
    let [a, b, _] = bar_chips(width);
    if width >= 640. {
        let left = a.expect("left chip");
        let (radius, step, flag) = (8., 24., 22.);
        let group = flag + 10. + step * (LAPS - 1) as f32 + 2. * radius;
        let x = left.x + (left.w - group) / 2.;
        let y = left.y + 21.;
        BarMeters {
            flag: vec2(x + flag / 2., y),
            flag_size: flag,
            first_pip: vec2(x + flag + 10. + radius, y),
            pip_radius: radius,
            pip_step: step,
            chevron: vec2(left.x + 19., left.y + 45.),
            gates: Rect::new(left.x + 30., left.y + 40., left.w - 44., 10.),
        }
    } else {
        let main = a.expect("narrow chip");
        let strip = b.expect("narrow strip");
        let y = main.y + 14.;
        BarMeters {
            flag: vec2(main.x + 17., y),
            flag_size: 16.,
            first_pip: vec2(main.x + 37., y),
            pip_radius: 6.,
            pip_step: 17.,
            chevron: vec2(strip.x + 15., strip.y + 13.),
            gates: Rect::new(
                strip.x + 25.,
                strip.y + 8.,
                (strip.w * 0.55 - 30.).clamp(60., 240.),
                10.,
            ),
        }
    }
}

fn width_of(s: &str, style: TextStyle) -> f32 {
    width(s, style)
}

/// The speedometer arc around `c`, swept by `fraction` of top speed.
fn speed_gauge(c: Vec2, radius: f32, fraction: f32, thickness: f32) {
    let hot = |t: f32| {
        shapes::mix(
            Color::new(1., 0.86, 0.45, 1.),
            Color::new(1., 0.48, 0.3, 1.),
            t,
        )
    };
    let start = PI * 0.75;
    let sweep = PI * 1.5;
    shapes::stroke_arc(
        c,
        radius,
        start,
        sweep,
        Stroke::new(thickness, Color::new(0., 0., 0., 0.35)).cap(Cap::Round),
    );
    if fraction > 0.01 {
        shapes::stroke_arc(
            c,
            radius,
            start,
            sweep * fraction,
            Stroke::new(thickness, hot(fraction)).cap(Cap::Round),
        );
    }
    let needle = start + sweep * fraction;
    shapes::fill_circle(
        c + Vec2::from_angle(needle) * radius,
        thickness * 0.6,
        Color::new(1., 0.97, 0.88, 1.),
    );
}

/// The medal still in reach beside the race clock. When a target slips by
/// it pops and rings once in its new colour.
fn live_medal(c: Vec2, size: f32, race: &Race, motion: f32) {
    let medal = quips::medal_in_reach(race.total_time);
    let since = quips::MEDALS
        .iter()
        .rev()
        .find(|&&target| race.total_time > target)
        .map(|&target| race.total_time - target);
    let pop = if race.finished() {
        0.
    } else {
        since.map_or(0., |s| (1. - s / 0.7).clamp(0., 1.))
    };
    if pop > 0. {
        shapes::stroke_circle(
            c,
            size * (0.5 + 0.9 * (1. - pop)),
            2.,
            fade(medal_color(medal), pop),
        );
    }
    medal_glyph(c, size * (1. + 0.35 * pop * motion), medal, 1.);
}

pub fn top_bar(race: &Race, juice: &Juice, w: f32) {
    let chips = bar_chips(w);
    for chip in chips.iter().flatten() {
        shapes::drop_shadow(
            *chip,
            14.,
            Shadow::soft(SHADOW).offset(vec2(0., 4.)).blur(16.),
        );
    }
    // Chip fills first, so the first BAR-coloured vertex precedes every label.
    for chip in chips.iter().flatten() {
        shapes::fill_rounded_rect(*chip, 14., BAR);
        shapes::stroke_rounded_rect(*chip, 14., 1., HAIRLINE);
    }
    let motion = juice.motion().min(1.);
    // Laps and gates: pips and a meter, no words.
    let meters = bar_meters(w);
    icon(Icon::Flag, meters.flag, meters.flag_size, GOLD);
    lap_pips(
        meters.first_pip,
        meters.pip_radius,
        meters.pip_step,
        race,
        juice.time,
        motion,
        1.,
    );
    chevron(meters.chevron, 10., AMBER);
    gate_meter(
        meters.gates,
        race.checkpoints_passed(),
        race.finished(),
        juice.gate_flash,
        juice.time,
        motion,
    );
    // After the finish the lap timer has reset; show the final lap instead.
    let lap_time = clock(if race.finished() {
        race.last_lap.unwrap_or(0.)
    } else {
        race.lap_time
    });
    let text = bar_labels(
        w,
        line!("{}", speed_kmh(race)),
        lap_time,
        clock(race.total_time),
        juice
            .last_split
            .map(crate::juice::split_label)
            .map(|(s, c)| (Line::of(format_args!("{s}")), c)),
    );
    // Speed: gauge, number, unit. Cream text order: speed, lap time, race time.
    let fraction = (speed_kmh(race) as f32 / (265. * 0.48)).clamp(0., 1.);
    if let [_, _, Some(right)] = chips {
        speed_gauge(vec2(right.x + 30., right.y + 30.), 16., fraction, 5.);
    } else {
        speed_gauge(
            vec2(text.speed.x - 14., text.speed.mid_y() + 1.),
            8.,
            fraction,
            3.,
        );
    }
    text.speed.draw();
    text.unit.draw();
    let clock_size = if w >= 640. { 22. } else { 16. };
    icon(
        Icon::Clock,
        vec2(
            text.lap_time.x - clock_size / 2. - 7.,
            text.lap_time.mid_y(),
        ),
        clock_size,
        MUTED,
    );
    text.lap_time.draw();
    live_medal(
        vec2(text.race_time.x - 13., text.race_time.mid_y()),
        if w >= 640. { 20. } else { 17. },
        race,
        motion,
    );
    text.race_time.draw();
    if let Some(delta) = text.delta {
        delta.draw();
    }
}

// ---------------------------------------------------------------------------
// Minimap and banners.

/// The minimap rectangle under the chips, top right.
pub fn minimap_rect(width: f32) -> Rect {
    let (w, margin) = if width >= 640. {
        (176., 16.)
    } else {
        ((width * 0.34).clamp(90., 120.), 8.)
    };
    let h = if width >= 640. { 102. } else { w * 0.58 };
    Rect::new(width - margin - w, bar_height(width) + 10., w, h)
}

/// The best lap under the minimap: a gold star and its time, or a grey star
/// and an alert when storage failed and the best lives only this session.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BestLap {
    pub time: f32,
    pub saved: bool,
}

pub fn minimap(
    track: &Track,
    race: &Race,
    ghost: Option<Pose>,
    rect: Rect,
    best: Option<BestLap>,
    time: f32,
) {
    let caption = if best.is_some() { 20. } else { 0. };
    surface(
        Rect::new(rect.x, rect.y, rect.w, rect.h + caption),
        12.,
        Color::new(0.12, 0.19, 0.17, 0.72),
        1.,
    );
    let pad = 9.;
    let scale = ((rect.w - pad * 2.) / 1000.).min((rect.h - pad * 2.) / 560.);
    let origin = rect.center() - vec2(555., 320.) * scale;
    let map = |p: Vec2| origin + p * scale;
    let mut points = [Vec2::ZERO; 96];
    for (i, p) in points.iter_mut().enumerate() {
        *p = map(track.points[i * 2]);
    }
    shapes::polyline(
        &points,
        Stroke::new(4., Color::new(0.88, 0.82, 0.66, 0.9))
            .join(Join::Miter)
            .closed(true),
    );
    let (start, tangent) = track.gate(0);
    let normal = vec2(-tangent.y, tangent.x) * 5.;
    let s = map(start);
    draw_line(
        s.x - normal.x,
        s.y - normal.y,
        s.x + normal.x,
        s.y + normal.y,
        2.,
        INK,
    );
    if !race.finished() {
        let gate = map(track.gate(race.next_gate).0);
        shapes::stroke_circle(gate, 4. + (time * 4.).sin().abs(), 1.5, AMBER);
    }
    if let Some(ghost) = ghost {
        let g = map(ghost.position);
        shapes::fill_circle(g, 3., Color::new(0.7, 0.88, 1., 0.85));
    }
    let car = map(race.car.pose.position);
    post::glow_circle(car.x, car.y, 9., Color::new(1., 0.4, 0.3, 1.), 0.9);
    shapes::fill_circle(car, 4.5, Color::new(0.98, 0.95, 0.88, 0.95));
    shapes::fill_circle(car, 3., Color::new(0.90, 0.30, 0.20, 1.));
    if let Some(best) = best {
        let text = clock(best.time);
        let style = fit(&text, modern(16., MUTED), rect.w - 44.);
        let tw = width(&text, style);
        let alert = if best.saved { 0. } else { 20. };
        let group = 22. + tw + alert;
        let x = rect.x + (rect.w - group) / 2.;
        let cy = rect.y + rect.h + 8.;
        star(vec2(x + 8., cy), 8., if best.saved { GOLD } else { MUTED });
        put(&text, x + 22., style.centered_baseline(cy), style);
        if !best.saved {
            icon(Icon::Alert, vec2(x + 22. + tw + 12., cy), 14., AMBER);
        }
    }
}

/// Top of the event banner, below the chips (and the minimap when narrow).
pub fn banner_top(width: f32) -> f32 {
    if width >= 640. {
        bar_height(width) + 14.
    } else {
        let map = minimap_rect(width);
        map.y + map.h + 30.
    }
}

/// The comparison chip under a lap banner: a signed delta, `BEST` or a tie.
fn compare_chip(compare: LapCompare) -> Option<(Line, Color)> {
    match compare {
        LapCompare::First | LapCompare::Best => Some((line!("BEST"), GOLD)),
        LapCompare::Tie => Some((line!("0.00"), MUTED)),
        LapCompare::Faster(d) => Some((line!("-{d:.2}"), GREEN)),
        LapCompare::Slower(d) => Some((line!("+{d:.2}"), RED)),
        LapCompare::None => None,
    }
}

/// Lap detail chips: clock and lap time, a star with the comparison, and a
/// green tick for a clean lap. Returns the width; draws only when `at` is set.
fn lap_detail(detail: &LapDetail, at: Option<(f32, f32)>, alpha: f32) -> f32 {
    let time = clock(detail.time);
    let time_style = fade_style(modern(16., CREAM), alpha);
    let mut x = 0.;
    let mut draw = |w: f32, paint: &dyn Fn(f32, f32)| {
        if let Some((left, cy)) = at {
            paint(left + x, cy);
        }
        x += w;
    };
    draw(22., &|x, cy| {
        icon(Icon::Clock, vec2(x + 8., cy), 16., fade(MUTED, alpha))
    });
    let tw = width(&time, time_style);
    draw(tw, &|x, cy| {
        put(&time, x, time_style.centered_baseline(cy), time_style)
    });
    if let Some((label, color)) = compare_chip(detail.compare) {
        let style = fade_style(modern(16., color), alpha);
        let lw = width(&label, style);
        let starred = detail.compare.is_best() || detail.compare == LapCompare::Tie;
        let lead = if starred { 20. } else { 0. };
        draw(14. + lead + lw, &|x, cy| {
            if starred {
                star(vec2(x + 14. + 8., cy), 8., fade(GOLD, alpha));
            }
            put(&label, x + 14. + lead, style.centered_baseline(cy), style);
        });
    }
    if detail.clean {
        draw(30., &|x, cy| {
            tick(vec2(x + 20., cy), 16., fade(GREEN, alpha))
        });
    }
    x
}

/// Banner below the chips: off-track and wrong-way guidance outrank toasts,
/// and secrets (alert toasts) briefly outrank everything. Every banner is an
/// icon and a headline of at most three words; lap banners add icon chips.
#[allow(clippy::too_many_arguments)]
pub fn banner(
    race: &Race,
    track: &Track,
    session: &RaceSession,
    style: &Style,
    juice: &Juice,
    autoplay_reversing: bool,
    width: f32,
    motion: Motion,
) {
    if race.finished() || session.countdown() > 0 {
        return;
    }
    let wrong_way =
        race.car.velocity.length() > 20. && track.road_cue(&race.car) == RoadCue::WrongWay;
    let alert = juice
        .toast
        .as_ref()
        .filter(|toast| toast.priority == crate::juice::Priority::Alert);
    fn from_toast(
        toast: &crate::juice::Toast,
        motion: Motion,
    ) -> (&str, Icon, Color, Option<LapDetail>, f32, f32) {
        let look = Entrance::fade_up(10.)
            .with_duration(0.28)
            .sample(toast.age, 0, motion);
        (
            toast.head.as_str(),
            toast.icon,
            toast.color,
            toast.lap,
            toast.alpha() * look.alpha,
            look.offset[1],
        )
    }
    let (head, kind, color, lap, alpha, offset): (&str, Icon, Color, Option<LapDetail>, f32, f32) =
        if let Some(toast) = alert {
            from_toast(toast, motion)
        } else if session.off_track() {
            (
                quips::off_track(style.stats.off_track_episodes.max(1)),
                Icon::Sprout,
                AMBER,
                None,
                1.,
                0.,
            )
        } else if wrong_way && !autoplay_reversing {
            (
                quips::wrong_way(style.stats.wrong_way_episodes.max(1)),
                Icon::Undo,
                RED,
                None,
                1.,
                0.,
            )
        } else if let Some(toast) = &juice.toast {
            from_toast(toast, motion)
        } else if autoplay_reversing {
            ("REVERSING", Icon::ArrowDown, MUTED, None, 1., 0.)
        } else {
            return;
        };
    let wide = width >= 640.;
    let map = minimap_rect(width);
    let max_width = if wide {
        (width - 2. * (map.w + 40.)).max(200.)
    } else {
        width - 16.
    };
    let head_style = if wide {
        modern(24., color)
    } else {
        modern(16., color)
    };
    let icon_size = if wide { 26. } else { 20. };
    let row = icon_size + 10. + width_of(head, head_style);
    let detail = lap.map_or(0., |d| lap_detail(&d, None, alpha));
    let content = row.max(detail).min(max_width - 44.);
    let panel = Rect::new(
        (width - content - 44.) / 2.,
        banner_top(width) + offset,
        content + 44.,
        if lap.is_some() { 72. } else { 46. },
    );
    surface(panel, 16., SURFACE, alpha);
    shapes::fill_capsule(
        vec2(panel.x + 12., panel.y + 12.),
        vec2(panel.x + 12., panel.y + panel.h - 12.),
        2.5,
        fade(color, alpha),
    );
    let cx = panel.x + panel.w / 2. + 4.;
    let cy = panel.y + 23.;
    let row = row.min(content);
    let x = cx - row / 2.;
    icon(
        kind,
        vec2(x + icon_size / 2., cy),
        icon_size,
        fade(color, alpha),
    );
    let head_style = fade_style(fit(head, head_style, content - icon_size - 10.), alpha);
    put(
        head,
        x + icon_size + 10.,
        head_style.centered_baseline(cy),
        head_style,
    );
    if let Some(detail) = lap {
        let w = lap_detail(&detail, None, alpha);
        lap_detail(&detail, Some((cx - w / 2., panel.y + 52.)), alpha);
    }
}

fn fade_style(style: TextStyle, alpha: f32) -> TextStyle {
    let mut s = style.with_color(fade(style.color, alpha));
    s.shadow = s.shadow.map(|c| fade(c, alpha));
    s.outline = s.outline.map(|c| fade(c, alpha));
    s
}

/// Rising world-space score pops and the live drift counter: Classic arcade
/// callouts with a dark outline, legible over grass and asphalt.
pub fn pops(juice: &Juice, style: &Style, race: &Race, view: &View) {
    let motion = juice.motion().min(1.);
    for pop in juice.pops.iter().flatten() {
        let t = pop.age / pop.life;
        let rise = 36. * Ease::CubicOut.apply(t) * motion;
        let grow = 1. + 0.4 * (1. - Ease::BackOut.apply((pop.age / 0.18).min(1.))).max(0.) * motion;
        let at = view.screen(pop.at) - vec2(0., rise);
        let alpha = 1. - t * t;
        let size = if pop.size >= 22. { 24. } else { 16. };
        let s = fade_style(classic(size * grow, pop.color), alpha);
        put_centered(&pop.text, at.x, at.y, s, view.width - 16.);
    }
    let live = style.live_drift();
    if live > 0 && !race.finished() {
        // A drift glyph, the live points and the chain multiplier.
        let at = view.screen(race.car.pose.position) + vec2(0., -44.);
        let multiplier = style.next_multiplier();
        let number = line!("{live}");
        let points = classic(16., GOLD);
        let chain = line!("X{multiplier}");
        let chain_style = classic(16., AMBER);
        let nw = width(&number, points);
        let cw = if multiplier > 1 {
            width(&chain, chain_style) + 6.
        } else {
            0.
        };
        let x = at.x - (22. + nw + cw) / 2.;
        drift_glyph(vec2(x + 9., at.y - 7.), 20., GOLD);
        put(&number, x + 22., at.y, points);
        if multiplier > 1 {
            put(&chain, x + 22. + nw + 6., at.y, chain_style);
        }
        // A small arc fills as the drift grows toward the next rating.
        let next = [40, 100, 200, 350, 600]
            .into_iter()
            .find(|&n| live < n)
            .unwrap_or(600);
        let progress = (live as f32 / next as f32).clamp(0., 1.);
        let c = at + vec2(0., 12.);
        shapes::stroke_arc(
            c,
            7.,
            -FRAC_PI_2,
            std::f32::consts::TAU * progress,
            Stroke::new(2.5, GOLD).cap(Cap::Round),
        );
    }
}

/// A contextual control prompt: a key cap and a pictogram under the car,
/// shown only while it applies and gone once the lesson is learned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tip {
    /// Parked after GO: hold gas.
    Drive,
    /// First race, no drift yet, a bend ahead: hold the handbrake.
    Drift,
}

/// Short onboarding lessons that disappear once each is learned.
pub fn coach(
    race: &Race,
    track: &Track,
    session: &RaceSession,
    style: &Style,
    attempt: u32,
) -> Option<Tip> {
    if race.finished() || session.countdown() > 0 || session.off_track() {
        return None;
    }
    let speed = race.car.velocity.length();
    if race.ticks > 150 && race.ticks < 600 && speed < 15. {
        return Some(Tip::Drive);
    }
    let bend = matches!(track.road_cue(&race.car), RoadCue::Left | RoadCue::Right);
    if attempt == 0
        && style.stats.drifts == 0
        && race.laps < 2
        && race.ticks > 240
        && speed > 60.
        && bend
    {
        return Some(Tip::Drift);
    }
    None
}

/// Draws a tip as `[key] pictogram` on a small chip under the car.
pub fn coach_prompt(tip: Tip, hints: &Hints, car: Vec2, juice: &Juice, width: f32) {
    let mut theme = theme();
    theme.body = 16.;
    let (key, glyph) = match tip {
        Tip::Drive => (hints.gas.as_str(), Glyph::Icon(Icon::ArrowUp)),
        Tip::Drift => (hints.drift.as_str(), Glyph::Drift),
    };
    let w = key_glyph_width(key, &theme);
    let bob = (juice.time * 3.).sin() * 2. * juice.motion().min(1.);
    let h = theme.body * 1.5 + 2.;
    let rect = Rect::new(
        (car.x - w / 2. - 10.).clamp(8., (width - w - 28.).max(8.)),
        car.y + 34. + bob,
        w + 20.,
        h + 9.,
    );
    surface(rect, 12., SURFACE, 0.92);
    key_glyph(rect.x + 10., rect.y + 5., key, glyph, &theme);
}

// ---------------------------------------------------------------------------
// Countdown, GO, flash.

pub fn countdown_rect(width: f32, height: f32) -> Rect {
    let (w, h) = ((width - 32.).clamp(1., 380.), 178.);
    let top = (height / 2. - h / 2.).max(bar_height(width) + 8.);
    Rect::new((width - w) / 2., top, w, h)
}

/// The front door: an animated logo above the first countdown, when it fits.
pub fn logo(session: &RaceSession, juice: &Juice, width: f32, height: f32) {
    let card = countdown_rect(width, height);
    let top = bar_height(width) + 12.;
    if card.y - top < 96. {
        return;
    }
    let t = session.countdown_progress() * 3.;
    let motion = Motion::from_reduced(juice.motion() == 0.);
    let look = Entrance::fade_up(-18.)
        .with_duration(0.6)
        .with_ease(Ease::BackOut)
        .sample(t, 0, motion);
    let cy = (card.y - 44.).max(top + 52.) + look.offset[1];
    let bob = (juice.time * 2.).sin() * 2. * juice.motion().min(1.);
    let size = if width >= 640. { 48. } else { 32. };
    let style = TextStyle::classic(size, fade(Color::from_hex(0xffd166), look.alpha))
        .with_outline(fade(OUTLINE, look.alpha))
        .with_shadow(fade(Color::new(0., 0., 0., 0.35), look.alpha));
    put_centered("POCKET RALLY", width / 2., cy + bob, style, width - 32.);
}

/// The three medal targets as medals with times, plus the best race (a gold
/// crown and its time) when one is recorded. Scales down to fit `max_width`.
fn medal_targets(cx: f32, cy: f32, max_width: f32, best: Option<f32>, alpha: f32) {
    let targets = [Medal::Gold, Medal::Silver, Medal::Bronze];
    let labels: [Line; 3] = std::array::from_fn(|i| line!("{:.0}s", quips::MEDALS[i]));
    let best = best.map(clock);
    let base = modern(16., CREAM);
    let item = |label: &Line| 20. + width(label, base);
    let gap = 14.;
    let total = labels.iter().map(item).sum::<f32>()
        + gap * 2.
        + best.as_ref().map_or(0., |b| 22. + 22. + width(b, base));
    let k = (max_width / total).min(1.);
    let mut x = cx - total * k / 2.;
    for (medal, label) in targets.into_iter().zip(&labels) {
        medal_glyph(vec2(x + 8. * k, cy), 20. * k, medal, alpha);
        let style = fade_style(modern(16. * k, medal_color(medal)), alpha);
        put(label, x + 20. * k, style.centered_baseline(cy), style);
        x += (item(label) + gap) * k;
    }
    if let Some(best) = best {
        x += 8. * k;
        icon(
            Icon::Crown,
            vec2(x + 8. * k, cy),
            18. * k,
            fade(GOLD, alpha),
        );
        let style = fade_style(modern(16. * k, PAPER), alpha);
        put(&best, x + 22. * k, style.centered_baseline(cy), style);
    }
}

/// Countdown card: exactly two cream labels (title and number) inside the
/// panel, plus start lamps and either key-cap prompts (first race) or the
/// medal targets. Browser glyph tests read it.
#[allow(clippy::too_many_arguments)]
pub fn countdown(
    session: &RaceSession,
    juice: &Juice,
    hints: &Hints,
    attempt: u32,
    best_race: Option<f32>,
    width: f32,
    height: f32,
) {
    let count = session.countdown();
    let rect = countdown_rect(width, height);
    let cx = width / 2.;
    let top = rect.y;
    let content = (rect.w - 40.).max(1.);
    shapes::drop_shadow(
        rect,
        18.,
        Shadow::soft(SHADOW).offset(vec2(0., 8.)).blur(28.),
    );
    shapes::fill_rounded_rect(rect, 18., PANEL);
    shapes::stroke_rounded_rect(rect, 18., 1., HAIRLINE);
    put_centered(
        quips::ready(attempt),
        cx,
        top + 40.,
        modern(24., CREAM),
        content,
    );
    // Each new number pops in; reduced motion keeps it still.
    let within = (session.countdown_progress() * 3.).fract();
    let pop = 1.
        + 0.28
            * (1. - Ease::BackOut.apply((within / 0.22).min(1.))).max(0.)
            * juice.motion().min(1.);
    put_centered(
        &line!("{count}"),
        cx,
        top + 104.,
        TextStyle::classic(48. * pop, CREAM).with_outline(OUTLINE),
        content,
    );
    for i in 0..3 {
        let lit = (3 - count as i32) >= i;
        let c = vec2(cx + (i as f32 - 1.) * 30., top + 126.);
        shapes::fill_circle(c, 10., Color::new(0.04, 0.07, 0.06, 1.));
        if lit {
            post::glow_circle(c.x, c.y, 22., LAMP_RED, 0.9);
        }
        shapes::fill_circle(
            c,
            7.,
            Fill::radial(
                c - vec2(2., 2.),
                9.,
                if lit {
                    Color::new(1., 0.6, 0.5, 1.)
                } else {
                    LAMP_OFF
                },
                if lit { LAMP_RED } else { shade(LAMP_OFF, 0.8) },
            ),
        );
    }
    let mut theme = theme();
    if attempt == 0 && !hints.touch {
        theme.body = if rect.w < 320. { 12. } else { 16. };
        glyph_prompts(
            &[
                (&hints.gas, Glyph::Icon(Icon::ArrowUp)),
                (&hints.drift, Glyph::Drift),
            ],
            cx,
            top + 142.,
            content,
            &theme,
        );
    } else {
        medal_targets(cx, top + 156., content, best_race, 1.);
    }
}

pub fn go_stamp(juice: &Juice, width: f32, height: f32) {
    let Some(t) = juice.since_go else {
        return;
    };
    let alpha = 1. - ((t - 0.55) / 0.6).clamp(0., 1.);
    let pop =
        1. + 0.8 * (1. - Ease::BackOut.apply((t / 0.3).min(1.))).max(0.) * juice.motion().min(1.);
    let c = vec2(width / 2., height * 0.42);
    post::glow_circle(c.x, c.y - 24., 120. * pop, fade(GREEN, alpha), 0.35);
    let style = TextStyle::classic(72. * pop, fade(GREEN, alpha))
        .with_outline(fade(OUTLINE, alpha))
        .with_shadow(fade(Color::new(0., 0., 0., 0.4), alpha));
    put_centered("GO!", c.x, c.y, style, width - 32.);
}

/// Gold edge glow for a new best lap; cleared under reduced flash.
pub fn flash(juice: &Juice, width: f32, height: f32) {
    if juice.flash <= 0. {
        return;
    }
    let a = juice.flash * 0.35;
    for (from, to, rect) in [
        (vec2(0., 0.), vec2(0., 60.), Rect::new(0., 0., width, 60.)),
        (
            vec2(0., height),
            vec2(0., height - 60.),
            Rect::new(0., height - 60., width, 60.),
        ),
        (vec2(0., 0.), vec2(60., 0.), Rect::new(0., 0., 60., height)),
        (
            vec2(width, 0.),
            vec2(width - 60., 0.),
            Rect::new(width - 60., 0., 60., height),
        ),
    ] {
        post::with_glow(|| {
            shapes::fill_rounded_rect(
                rect,
                0.,
                Fill::linear(from, to, fade(GOLD, a), fade(GOLD, 0.)),
            )
        });
    }
}

/// A touch button's label: arrow heads for steering, the word for gas and
/// brake (the on-screen hints name those buttons).
pub fn touch_label(label: &str, x: f32, y: f32, max_width: f32) {
    match label {
        "<" => arrow_glyph(vec2(x, y - 7.), 26., -1., CREAM),
        ">" => arrow_glyph(vec2(x, y - 7.), 26., 1., CREAM),
        _ => put_centered(label, x, y, modern(16., CREAM), max_width),
    }
}

// ---------------------------------------------------------------------------
// Pause.

pub fn pause_rect(width: f32, height: f32) -> Rect {
    let (w, h) = ((width - 32.).clamp(1., 400.), 164.);
    Rect::new(
        (width - w) / 2.,
        ((height - h) / 2.).max(MENU_CORNER.y + 4.),
        w,
        h,
    )
}

/// The pause card: one co-driver line, the race so far as pips and a medal
/// clock, and key caps with the pictogram of what they do.
#[allow(clippy::too_many_arguments)]
pub fn paused(race: &Race, hints: &Hints, seed: u32, age: f32, motion: Motion, w: f32, h: f32) {
    let dim = Entrance::fade().with_duration(0.2).sample(age, 0, motion);
    shapes::fill_rounded_rect(
        Rect::new(0., 0., w, h),
        0.,
        Color::new(0.03, 0.06, 0.05, 0.42 * dim.alpha),
    );
    let look = Entrance::fade_up(14.)
        .with_duration(0.3)
        .sample(age, 0, motion);
    let rect = kit::appear(pause_rect(w, h), look);
    let theme = theme().faded(look.alpha);
    let inner = kit::card(rect, Some("Paused"), &theme);
    let cx = inner.x + inner.w / 2.;
    let a = look.alpha;
    put_centered(
        quips::pause(seed),
        cx,
        inner.y + 18.,
        modern(16., fade(theme.muted, a)),
        inner.w,
    );
    // Laps as pips, then the race clock beside the medal still in reach.
    let time = clock(race.total_time);
    let time_style = modern(16., fade(theme.ink, a));
    let (radius, step) = (7., 20.);
    let pips = 20. + 8. + step * (LAPS - 1) as f32 + 2. * radius;
    let clock_w = 24. + width(&time, time_style);
    let group = pips + 28. + clock_w;
    let x = cx - group / 2.;
    let cy = inner.y + 42.;
    icon(Icon::Flag, vec2(x + 10., cy), 18., fade(GOLD, a));
    lap_pips(vec2(x + 28. + radius, cy), radius, step, race, 0., 0., a);
    let x = x + pips + 28.;
    medal_glyph(
        vec2(x + 9., cy),
        18.,
        quips::medal_in_reach(race.total_time),
        a,
    );
    put(&time, x + 24., time_style.centered_baseline(cy), time_style);
    let mut small = theme;
    small.body = if inner.w < 300. { 12. } else { 16. };
    let items: [(&str, Glyph); 3] = [
        (&hints.pause, Glyph::Icon(Icon::Play)),
        (&hints.restart, Glyph::Icon(Icon::Restart)),
        ("F2", Glyph::Icon(Icon::Gamepad)),
    ];
    let count = if hints.keyboard { 3 } else { 2 };
    glyph_prompts(&items[..count], cx, inner.y + 62., inner.w, &small);
}

// ---------------------------------------------------------------------------
// Results.

/// Personal-best context captured when the race finished.
#[derive(Clone, Copy, Debug, Default)]
pub struct Results {
    pub previous_best: Option<f32>,
    pub previous_style: Option<u32>,
    pub demo: bool,
}

impl Results {
    pub fn new_best(&self, time: f32) -> bool {
        !self.demo && self.previous_best.is_none_or(|best| time < best)
    }
}

pub fn medal_color(medal: Medal) -> Color {
    match medal {
        Medal::Gold => Color::new(1.0, 0.80, 0.30, 1.),
        Medal::Silver => Color::new(0.80, 0.85, 0.88, 1.),
        Medal::Bronze => Color::new(0.85, 0.55, 0.32, 1.),
        Medal::Unranked => Color::new(0.50, 0.55, 0.50, 1.),
    }
}

/// How the finish compares with the stored best race, shown as a crown chip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Record {
    /// Autoplay never records.
    Demo,
    /// No stored race yet: this one is the time to beat.
    First,
    /// Beat the stored best by `by` seconds.
    NewBest { by: f32 },
    /// Missed the stored `best` by `by` seconds.
    Behind { best: f32, by: f32 },
}

pub fn record(time: f32, results: &Results) -> Record {
    if results.demo {
        Record::Demo
    } else if let Some(best) = results.previous_best {
        if results.new_best(time) {
            Record::NewBest { by: best - time }
        } else {
            Record::Behind {
                best,
                by: time - best,
            }
        }
    } else {
        Record::First
    }
}

/// Wide screens move the results card right so the car and its confetti
/// stay visible on the left; narrow screens center it.
pub fn results_center_x(width: f32) -> f32 {
    if width >= 1000. {
        (width - 260.).min(width * 0.68)
    } else {
        width / 2.
    }
}

const CARD: Vec2 = vec2(440., 440.);

fn card_scale(width: f32, height: f32) -> f32 {
    ((width - 24.) / CARD.x)
        .min((height - 24.) / CARD.y)
        .clamp(0.3, 1.)
}

/// Screen point where the finish camera frames the car: left of the card on
/// wide screens, below it on tall narrow screens, otherwise centered.
pub fn finish_framing(width: f32, height: f32) -> Vec2 {
    if width >= 1000. {
        return vec2((results_center_x(width) - CARD.x / 2.) / 2., height / 2.);
    }
    let r = results_rect(width, height);
    let bottom = r.y + r.h;
    if height - bottom >= 110. {
        vec2(width / 2., (bottom + height) / 2.)
    } else {
        vec2(width / 2., height / 2.)
    }
}

pub fn results_rect(width: f32, height: f32) -> Rect {
    let k = card_scale(width, height);
    let (w, h) = (CARD.x * k, CARD.y * k);
    let cx = results_center_x(width);
    Rect::new(cx - w / 2., ((height - h) / 2.).max(8.), w, h)
}

/// Rolling numbers on the results card, advanced by render time.
#[derive(Clone, Copy, Debug)]
pub struct CardRolls {
    pub time: ferravex_play::tween::RollCounter,
    pub style: ferravex_play::tween::RollCounter,
}

impl CardRolls {
    pub fn new() -> Self {
        Self {
            time: ferravex_play::tween::RollCounter::new(0),
            style: ferravex_play::tween::RollCounter::new(0),
        }
    }
}

impl Default for CardRolls {
    fn default() -> Self {
        Self::new()
    }
}

/// Paints an icon-led row centred on `cx`: `lead` pictogram width, then text.
fn icon_row(cx: f32, cy: f32, lead: f32, text: &str, style: TextStyle, paint: impl Fn(Vec2)) {
    let w = lead + width(text, style);
    let x = cx - w / 2.;
    paint(vec2(x + lead / 2. - 3., cy));
    put(text, x + lead, style.centered_baseline(cy), style);
}

/// The podium card. Layout is authored at 440×440 and scaled to fit; rows
/// enter with a staggered fade-up and the time and style roll up. Headline,
/// rank title and one co-driver line; everything else is icons and numbers.
#[allow(clippy::too_many_arguments)]
pub fn results(
    race: &Race,
    style: &Style,
    juice: &Juice,
    results: &Results,
    rolls: &mut CardRolls,
    hints: &Hints,
    width: f32,
    height: f32,
) {
    let motion = Motion::from_reduced(juice.motion() == 0.);
    let elapsed = juice.celebrate - 0.45;
    if elapsed < 0. && !motion.is_reduced() {
        return;
    }
    let entrance = Entrance::fade_up(14.)
        .with_duration(0.35)
        .with_stagger(0.06);
    let row = |i: usize| entrance.sample(elapsed.max(0.), i, motion);
    let k = card_scale(width, height);
    let r = results_rect(width, height);
    let card = row(0);
    let rect = kit::appear(r, card);
    let a0 = card.alpha;
    shapes::drop_shadow(
        rect,
        20. * k,
        Shadow::soft(fade(SHADOW, a0))
            .offset(vec2(0., 10.))
            .blur(32.),
    );
    shapes::fill_rounded_rect(rect, 20. * k, fade(PANEL, a0));
    shapes::stroke_rounded_rect(rect, 20. * k, 1., fade(HAIRLINE, a0));
    let total = race.total_time;
    let medal = quips::medal(total);
    let rank = quips::rank(medal, &style.stats);
    let cx = rect.x + rect.w / 2.;
    let content = rect.w - 40. * k;
    let at = |dy: f32, look: ferravex_play::tween::Appearance| rect.y + (dy + look.offset[1]) * k;
    let modern_k = |size: f32, color: Color, alpha: f32| modern(size * k, fade(color, alpha));

    // Medal-coloured accent band across the top, and the chequered finish.
    shapes::fill_rounded_rect(
        Rect::new(rect.x + 24. * k, rect.y, rect.w - 48. * k, 4. * k),
        2. * k,
        fade(medal_color(medal), a0),
    );
    let l = row(1);
    icon(
        Icon::Flag,
        vec2(cx, at(30., l)),
        24. * k,
        fade(GREEN, l.alpha),
    );
    let l = row(2);
    let stamp = if motion.is_reduced() {
        1.
    } else {
        1. + 0.5 * (1. - Ease::BackOut.apply(((elapsed - 0.1) / 0.3).clamp(0., 1.)))
    };
    let title = TextStyle::classic(32. * k * stamp, fade(GOLD, l.alpha))
        .with_outline(fade(OUTLINE, l.alpha));
    put_centered(rank.title, cx, at(78., l), title, content);
    let l = row(3);
    put_centered(
        rank.quip,
        cx,
        at(104., l),
        modern_k(16., MUTED, l.alpha),
        content,
    );

    // The medal and the rolling race time.
    let l = row(4);
    let centis = (f64::from(total) * 100.).round() as i64;
    rolls.time.set_target(centis);
    let shown = rolls.time.shown() as f32 / 100.;
    let time = clock(shown);
    let time_style =
        TextStyle::classic(40. * k, fade(CREAM, l.alpha)).with_outline(fade(OUTLINE, l.alpha));
    let group = 64. * k + width_of(&time, time_style);
    let badge = vec2(cx - group / 2. + 26. * k, at(150., l));
    let mc = medal_color(medal);
    if medal != Medal::Unranked {
        post::glow_circle(badge.x, badge.y, 46. * k, fade(mc, l.alpha), 0.5);
    }
    medal_glyph(badge, 56. * k, medal, l.alpha);
    put(&time, badge.x + 38. * k, badge.y + 18. * k, time_style);

    // The stored best race: a crown chip.
    let l = row(5);
    let cy = at(204., l);
    let crown = |color: Color| move |c: Vec2| icon(Icon::Crown, c, 20. * k, fade(color, l.alpha));
    match record(total, results) {
        Record::Demo => icon_row(
            cx,
            cy,
            28. * k,
            "DEMO",
            modern_k(16., MUTED, l.alpha),
            |c| icon(Icon::Gamepad, c, 20. * k, fade(MUTED, l.alpha)),
        ),
        Record::First => icon_row(
            cx,
            cy,
            28. * k,
            "NEW BEST",
            modern_k(16., GOLD, l.alpha),
            crown(GOLD),
        ),
        Record::NewBest { by } => icon_row(
            cx,
            cy,
            28. * k,
            &line!("NEW BEST  -{by:.2}"),
            modern_k(16., GOLD, l.alpha),
            crown(GOLD),
        ),
        Record::Behind { best, by } => {
            let best = clock(best);
            let behind = line!("+{by:.2}");
            let best_style = modern_k(16., PAPER, l.alpha);
            let behind_style = modern_k(16., RED, l.alpha);
            let w =
                28. * k + width_of(&best, best_style) + 12. * k + width_of(&behind, behind_style);
            let x = cx - w / 2.;
            crown(MUTED)(vec2(x + 11. * k, cy));
            put(
                &best,
                x + 28. * k,
                best_style.centered_baseline(cy),
                best_style,
            );
            put(
                &behind,
                x + w - width_of(&behind, behind_style),
                behind_style.centered_baseline(cy),
                behind_style,
            );
        }
    }

    // The next goal, as icons.
    let l = row(6);
    let cy = at(234., l);
    match quips::hook(medal, total, &style.stats) {
        Hook::Medal { medal: next, cut } => icon_row(
            cx,
            cy,
            30. * k,
            &line!("-{cut:.2}s"),
            modern_k(16., GREEN, l.alpha),
            |c| medal_glyph(c, 22. * k, next, l.alpha),
        ),
        Hook::CleanLaps(laps) => {
            let (radius, step) = (6. * k, 16. * k);
            let w = 26. * k + step * (laps - 1) as f32 + 2. * radius;
            let x = cx - w / 2.;
            tick(vec2(x + 9. * k, cy), 18. * k, fade(GREEN, l.alpha));
            for i in 0..laps {
                let c = vec2(x + 26. * k + radius + i as f32 * step, cy);
                let clean = i < style.stats.clean_laps;
                if clean {
                    shapes::fill_circle(c, radius, fade(GREEN, l.alpha));
                } else {
                    shapes::stroke_circle(c, radius - 1., 2., fade(GREEN, l.alpha));
                }
            }
        }
        Hook::Style(target) => icon_row(
            cx,
            cy,
            28. * k,
            &line!("{target}"),
            modern_k(16., GREEN, l.alpha),
            |c| icon(Icon::Flame, c, 20. * k, fade(GOLD, l.alpha)),
        ),
        Hook::Ducks => icon_row(cx, cy, 28. * k, "?", modern_k(16., WATER, l.alpha), |c| {
            icon(Icon::Wave, c, 20. * k, fade(WATER, l.alpha))
        }),
        Hook::Done => {
            icon(
                Icon::Crown,
                vec2(cx - 30. * k, cy),
                22. * k,
                fade(GOLD, l.alpha),
            );
            for i in 0..3 {
                star(
                    vec2(cx - 6. * k + i as f32 * 18. * k, cy),
                    7. * k,
                    fade(GOLD, l.alpha),
                );
            }
        }
    }

    // Stat tiles: a pictogram over each value.
    let stats = &style.stats;
    rolls.style.set_target(i64::from(stats.style));
    let style_shown = rolls.style.shown();
    let style_record = !results.demo && results.previous_style.is_none_or(|s| stats.style > s);
    let best_lap = race.best_time.map_or(line!("--"), clock);
    #[derive(Clone, Copy)]
    enum Mark {
        Icon(Icon, Color),
        Star,
        Drift,
        Tick,
    }
    let cells: [(Mark, Line, Color); 6] = [
        (Mark::Star, best_lap, CREAM),
        (
            Mark::Icon(Icon::Flame, AMBER),
            line!("{style_shown}"),
            if style_record && stats.style > 0 {
                GOLD
            } else {
                CREAM
            },
        ),
        (Mark::Drift, line!("{}", stats.best_drift), CREAM),
        (Mark::Tick, Line::new(), GREEN),
        (
            Mark::Icon(Icon::Sprout, GREEN),
            line!("{:.1}s", stats.grass_seconds()),
            CREAM,
        ),
        (
            Mark::Icon(Icon::Wave, WATER),
            if stats.splashes > 0 {
                line!("{}", stats.splashes)
            } else {
                line!("?")
            },
            if stats.splashes > 0 { WATER } else { MUTED },
        ),
    ];
    let gap = 8. * k;
    let tile_w = (rect.w - 32. * k - gap * 2.) / 3.;
    for (i, (mark, value, color)) in cells.iter().enumerate() {
        let l = row(7 + i);
        let col = (i % 3) as f32;
        let line_row = (i / 3) as f32;
        let tile = Rect::new(
            rect.x + 16. * k + col * (tile_w + gap),
            at(262. + line_row * 64., l),
            tile_w,
            58. * k,
        );
        shapes::fill_rounded_rect(tile, 10. * k, fade(Color::new(1., 1., 1., 0.05), l.alpha));
        let tc = tile.x + tile.w / 2.;
        let ic = vec2(tc, tile.y + 18. * k);
        match *mark {
            Mark::Icon(kind, tint) => icon(kind, ic, 22. * k, fade(tint, l.alpha)),
            Mark::Star => star(ic, 10. * k, fade(GOLD, l.alpha)),
            Mark::Drift => drift_glyph(ic, 24. * k, fade(GOLD, l.alpha)),
            Mark::Tick => tick(ic, 20. * k, fade(GREEN, l.alpha)),
        }
        if i == 1 && style_record && stats.style > 0 {
            icon(
                Icon::Crown,
                ic + vec2(20. * k, -4. * k),
                14. * k,
                fade(GOLD, l.alpha),
            );
        }
        if matches!(mark, Mark::Tick) {
            // Clean laps as pips, like the HUD's lap pips.
            let (radius, step) = (6. * k, 17. * k);
            for lap in 0..LAPS {
                let c = vec2(
                    tc + (lap as f32 - (LAPS - 1) as f32 / 2.) * step,
                    tile.y + 43. * k,
                );
                if lap < stats.clean_laps {
                    shapes::fill_circle(c, radius, fade(GREEN, l.alpha));
                } else {
                    shapes::stroke_circle(c, radius - 1., 2., fade(DIM, l.alpha * 2.));
                }
            }
            continue;
        }
        put_centered(
            value,
            tc,
            tile.y + 49. * k,
            modern_k(16., *color, l.alpha),
            tile.w - 8.,
        );
    }
    let l = row(13);
    let pulse = if motion.is_reduced() {
        1.
    } else {
        0.8 + 0.2 * (juice.celebrate * 4.).sin()
    };
    let mut kit_theme = theme().faded(l.alpha * pulse);
    kit_theme.body = 16. * k;
    if hints.touch {
        let cy = at(412., l);
        icon(
            Icon::Hand,
            vec2(cx - 16. * k, cy),
            22. * k,
            fade(GOLD, l.alpha * pulse),
        );
        icon(
            Icon::Restart,
            vec2(cx + 16. * k, cy),
            22. * k,
            fade(GOLD, l.alpha * pulse),
        );
    } else {
        glyph_prompts(
            &[(&hints.restart, Glyph::Icon(Icon::Restart))],
            cx,
            at(398., l),
            content,
            &kit_theme,
        );
    }
}

/// Advance the results roll counters by render time.
pub fn advance_rolls(rolls: &mut CardRolls, dt: f32, motion: Motion) {
    rolls.time.advance(dt, motion);
    rolls.style.advance(dt, motion);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopwatch_uses_whole_centiseconds_and_rolls_minutes() {
        assert_eq!(clock(0.), "00:00.00");
        assert_eq!(clock(6.00079), "00:06.00");
        assert_eq!(clock(7.), "00:07.00");
        assert_eq!(clock(59.999), "00:59.99");
        assert_eq!(clock(60.), "01:00.00");
        assert_eq!(clock(125.5), "02:05.50");
        assert_eq!(clock(-1.), "00:00.00");
        // Accumulated f32 tick time that lands a hair under a whole value.
        let mut t = 0_f32;
        for _ in 0..840 {
            t += 1. / 120.;
        }
        assert_eq!(clock(t), "00:07.00");
    }

    #[test]
    fn stack_lines_truncate_whole_pieces_and_never_split_utf8() {
        let long = line!("{}{}", "·".repeat(47), "abc");
        assert_eq!(long.as_str(), "·".repeat(47));
        assert_eq!(line!("Lap {}/{}", 2, 3), "Lap 2/3");
    }

    #[test]
    fn record_chips_never_claim_a_demo_best() {
        let first = Results::default();
        assert!(first.new_best(40.));
        assert_eq!(record(40., &first), Record::First);
        let chase = Results {
            previous_best: Some(44.),
            ..Results::default()
        };
        let Record::NewBest { by } = record(43.5, &chase) else {
            panic!("faster race is a new best");
        };
        assert!((by - 0.5).abs() < 1e-5);
        let Record::Behind { best, by } = record(45., &chase) else {
            panic!("slower race trails the best");
        };
        assert_eq!(best, 44.);
        assert!((by - 1.).abs() < 1e-5);
        let demo = Results {
            demo: true,
            ..Results::default()
        };
        assert!(!demo.new_best(1.));
        assert_eq!(record(1., &demo), Record::Demo);
    }

    #[test]
    fn onboarding_prompts_wait_for_go_show_in_context_and_fade_once_learned() {
        use crate::sim::Controls;
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut session = RaceSession::new();
        let mut style = Style::new();
        assert!(
            coach(&race, &track, &session, &style, 0).is_none(),
            "countdown"
        );
        let drive = |race: &mut Race, session: &mut RaceSession, ticks, throttle| {
            for _ in 0..ticks {
                let frame = session.advance(1. / 120., true);
                for _ in 0..frame.steps {
                    let controls = Controls {
                        throttle,
                        ..Default::default()
                    };
                    session.step(race, &track, controls, frame.dt);
                }
            }
        };
        drive(&mut race, &mut session, 360 + 200, 0.);
        assert_eq!(
            coach(&race, &track, &session, &style, 0),
            Some(Tip::Drive),
            "parked after GO"
        );
        drive(&mut race, &mut session, 120, 1.);
        // The drift prompt appears only at speed with a bend ahead.
        let (mut bends, mut straights) = (0, 0);
        for _ in 0..1200 {
            let frame = session.advance(1. / 120., true);
            for _ in 0..frame.steps {
                let controls = race.autopilot(&track);
                session.step(&mut race, &track, controls, frame.dt);
            }
            let bend = matches!(track.road_cue(&race.car), RoadCue::Left | RoadCue::Right);
            match coach(&race, &track, &session, &style, 0) {
                Some(Tip::Drift) => {
                    assert!(bend && race.car.velocity.length() > 60.);
                    bends += 1;
                }
                Some(Tip::Drive) => panic!("moving cars are not told to drive"),
                None => {
                    if !session.off_track() && race.car.velocity.length() > 60. {
                        assert!(!bend, "a bend at speed shows the drift prompt");
                    }
                    straights += 1;
                }
            }
            assert!(
                coach(&race, &track, &session, &style, 1).is_none(),
                "retries skip it"
            );
        }
        assert!(bends > 0 && straights > 0, "{bends} {straights}");
        style.stats.drifts = 1;
        for _ in 0..600 {
            let frame = session.advance(1. / 120., true);
            for _ in 0..frame.steps {
                let controls = race.autopilot(&track);
                session.step(&mut race, &track, controls, frame.dt);
            }
            assert!(
                coach(&race, &track, &session, &style, 0).is_none(),
                "learned"
            );
        }
    }

    /// The bar's text, worst-case strings, at each supported size.
    fn worst_bar(w: f32, delta: Option<(Line, Color)>) -> BarText {
        bar_labels(w, line!("127"), line!("59:59.99"), line!("59:59.99"), delta)
    }

    /// Browser fullscreen pins its 44px menu button in the top-right corner.
    #[test]
    fn hud_text_and_panels_keep_the_fullscreen_menu_corner_clear() {
        for (w, h) in [
            (1280., 800.),
            (1600., 1000.),
            (390., 844.),
            (844., 390.),
            (320., 480.),
        ] {
            let corner = Rect::new(w - MENU_CORNER.x, 0., MENU_CORNER.x, MENU_CORNER.y);
            let clear = |r: Rect, what: &str| {
                assert!(
                    !(r.x < corner.x + corner.w
                        && r.x + r.w > corner.x
                        && r.y < corner.y + corner.h
                        && r.y + r.h > corner.y),
                    "{what} {r:?} enters the menu corner at {w}x{h}"
                );
                assert!(r.x >= 0. && r.x + r.w <= w, "{what} {r:?} fits {w}x{h}");
            };
            let inside_a_chip = |b: Rect, what: &str| {
                assert!(
                    bar_chips(w).iter().flatten().any(|c| b.x >= c.x
                        && b.x + b.w <= c.x + c.w
                        && b.y >= c.y - 2.
                        && b.y + b.h <= c.y + c.h + 2.),
                    "{what} {b:?} escapes its chip at {w}x{h}"
                );
            };
            // Worst-case (widest) bar strings, with and without a split delta.
            for delta in [None, Some((line!("+9.99"), RED))] {
                let bar = worst_bar(w, delta);
                let labels = [bar.speed, bar.unit, bar.lap_time, bar.race_time];
                for label in labels.iter().chain(bar.delta.as_ref()) {
                    clear(label.bounds(), &label.text);
                    inside_a_chip(label.bounds(), &label.text);
                }
                // The three cream numbers keep the browser glyph-test spacing:
                // separate from each other, with room for their icons.
                let numbers = [bar.speed, bar.lap_time, bar.race_time].map(|l| l.caps());
                for (i, a) in numbers.iter().enumerate() {
                    for b in &numbers[i + 1..] {
                        let apart = a.x + a.w + 8. <= b.x
                            || b.x + b.w + 8. <= a.x
                            || a.y + a.h <= b.y
                            || b.y + b.h <= a.y;
                        assert!(apart, "{a:?} crowds {b:?} at {w}x{h}");
                    }
                    assert!(a.y + 1. < 80., "glyph test reads the bar above y=80");
                }
                assert!(bar.lap_time.x - TIMER_ICON >= 8.);
                // Pips, the gate chevron and meter stay in their chip, clear of text.
                for r in bar_meters(w).bounds() {
                    clear(r, "meter");
                    inside_a_chip(r, "meter");
                    for n in &numbers {
                        let overlap = r.x < n.x + n.w
                            && r.x + r.w > n.x
                            && r.y < n.y + n.h
                            && r.y + r.h > n.y;
                        assert!(!overlap, "meter {r:?} hits text {n:?} at {w}x{h}");
                    }
                }
            }
            for chip in bar_chips(w).iter().flatten() {
                clear(*chip, "chip");
            }
            let map = minimap_rect(w);
            clear(Rect::new(map.x, map.y, map.w, map.h + 20.), "minimap");
            clear(countdown_rect(w, h), "countdown");
            clear(pause_rect(w, h), "pause card");
            clear(results_rect(w, h), "results card");
            assert!(banner_top(w) >= MENU_CORNER.y, "banner");
        }
    }

    #[test]
    fn layout_rectangles_fit_desktop_portrait_and_short_landscape() {
        for (w, h) in [
            (1280., 800.),
            (1600., 1000.),
            (390., 844.),
            (320., 568.),
            (844., 390.),
        ] {
            let map = minimap_rect(w);
            assert!(map.x >= 8. && map.x + map.w <= w - 8.);
            assert!(map.y >= bar_height(w));
            assert!(map.y + map.h + 20. < h);
            let r = results_rect(w, h);
            assert!(r.x >= 8. && r.x + r.w <= w - 8. && r.y >= 8. && r.y + r.h <= h - 8.);
            let p = pause_rect(w, h);
            assert!(p.y + p.h <= h, "pause fits {w}x{h}");
            let car = finish_framing(w, h);
            assert!(car.x < r.x || car.y > r.y + r.h || car == vec2(w, h) / 2.);
        }
    }

    #[test]
    fn lap_comparison_chips_are_signed_numbers_not_sentences() {
        assert_eq!(compare_chip(LapCompare::First).unwrap().0, "BEST");
        let (faster, color) = compare_chip(LapCompare::Faster(0.5)).unwrap();
        assert!(faster == "-0.50" && color == GREEN);
        let (slower, color) = compare_chip(LapCompare::Slower(1.25)).unwrap();
        assert!(slower == "+1.25" && color == RED);
        assert_eq!(compare_chip(LapCompare::Tie).unwrap().0, "0.00");
        assert!(compare_chip(LapCompare::None).is_none());
    }
}
