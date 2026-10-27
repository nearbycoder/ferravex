//! Screen-space HUD, banners, countdown, pause and results cards.
//!
//! Nothing here advances the race or writes records. Copy uses the mixed-case
//! Modern pixel face; big numerals and arcade callouts keep the Classic face
//! for retro punch. Surfaces are anti-aliased rounded panels with soft
//! shadows. Every label is measured, fitted and kept inside its panel at
//! desktop, portrait and short landscape sizes, and steady-state frames format
//! into stack buffers instead of allocating.
use crate::juice::{AMBER, CREAM, GOLD, GREEN, Juice, MUTED, RED, WATER};
use crate::presentation::hint_for;
use crate::quips;
use crate::session::RaceSession;
use crate::sim::{LAPS, Pose, Race, RoadCue, Track};
use crate::style::Style;
use ferravex_play::gameplay::trial::Medal;
use ferravex_play::tween::{Ease, Entrance, Motion};
use ferravex_render::input::{HintDevice, PlayerInput};
use ferravex_render::post;
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Cap, Fill, Join, Shadow, Stroke};
use ferravex_render::text::{self, TextStyle, draw_text_styled};
use ferravex_render::ui::kit::{self, KitTheme};
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
        ink: Color::from_hex(0xf4efe2),
        muted: Color::from_hex(0xb6c3b3),
        accent: Color::from_hex(0xffc857),
        accent_ink: Color::from_hex(0x2a1d08),
        success: Color::from_hex(0x8be38f),
        danger: Color::from_hex(0xff7a66),
        focus: Color::from_hex(0x9ad9ff),
        ..KitTheme::default()
    }
}

/// Width of a `kit::key_prompt` before drawing it, for centering.
fn prompt_width(keys: &[&str], action: &str, theme: &KitTheme) -> f32 {
    let style = TextStyle::modern(theme.body, theme.ink);
    let h = theme.body * 1.5 + 2.;
    let caps: f32 = keys
        .iter()
        .map(|k| (width(k, style) + kit::SPACE[2] * 2.).max(h - 3.) + kit::SPACE[0])
        .sum();
    caps + kit::SPACE[0] + width(action, style)
}

/// Centered row of key prompts; wraps onto a second row when narrow.
fn prompts(items: &[(&str, &str)], cx: f32, y: f32, max_width: f32, theme: &KitTheme) -> f32 {
    let gap = kit::SPACE[4];
    let widths: [f32; 4] = std::array::from_fn(|i| {
        items
            .get(i)
            .map_or(0., |(k, a)| prompt_width(&[k], a, theme))
    });
    let total: f32 = widths.iter().sum::<f32>() + gap * (items.len().max(1) - 1) as f32;
    let row_h = theme.body * 1.5 + 8.;
    if total <= max_width {
        let mut x = cx - total / 2.;
        for (i, (key, action)) in items.iter().enumerate() {
            kit::key_prompt(x, y, &[key], action, theme);
            x += widths[i] + gap;
        }
        row_h
    } else {
        for (i, (key, action)) in items.iter().enumerate() {
            let row = y + i as f32 * row_h;
            kit::key_prompt(cx - widths[i] / 2., row, &[key], action, theme);
        }
        row_h * items.len() as f32
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

    /// Glyph bounds including the shadow/outline and descenders.
    #[cfg(test)]
    pub fn bounds(&self) -> Rect {
        let px = self.style.pixel();
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

/// The chips' labels. Cream labels are lap, speed, lap time, gates and race
/// time in that order; the browser glyph test reads exactly those five runs.
/// The right-hand chip ends before `width - 72` to keep the menu corner clear.
pub fn bar_labels(
    width: f32,
    lap: Line,
    speed: Line,
    lap_time: Line,
    gates: Line,
    race_time: Line,
    delta: Option<(Line, Color)>,
) -> [Option<Label>; 6] {
    let [a, b, c] = bar_chips(width);
    if let (Some(left), Some(mid), Some(right)) = (a, b, c) {
        let pad = 14.;
        let (delta, color) = delta.unwrap_or_else(|| (line!("Lap time"), MUTED));
        [
            Some(Label::at(lap, left.x + pad, 36., modern(24., CREAM))),
            Some(Label::right(
                speed,
                right.x + right.w - pad,
                34.,
                modern(24., CREAM),
            )),
            Some(Label::centered(
                lap_time,
                mid.x + mid.w / 2.,
                42.,
                classic(32., CREAM),
                mid.w - 16.,
            )),
            Some(Label::at(gates, left.x + pad, 57., modern(16., CREAM))),
            Some(Label::right(
                race_time,
                right.x + right.w - pad,
                57.,
                modern(16., CREAM),
            )),
            Some(Label::centered(
                delta,
                mid.x + mid.w / 2.,
                62.,
                modern(16., color),
                mid.w - 16.,
            )),
        ]
    } else {
        let main = a.expect("narrow chip");
        let strip = b.expect("narrow strip");
        let lap_style = classic(24., CREAM);
        let lap_right = main.x + 10. + width_of(&lap_time, lap_style);
        let delta = delta.and_then(|(text, color)| {
            [16., 8.]
                .into_iter()
                .map(|size| modern(size, color))
                .find(|style| lap_right + 8. + width_of(&text, *style) <= main.x + main.w - 10.)
                .map(|style| Label::at(text, lap_right + 8., 51., style))
        });
        [
            Some(Label::at(lap, main.x + 10., 25., modern(16., CREAM))),
            Some(Label::right(
                speed,
                main.x + main.w - 10.,
                25.,
                modern(16., CREAM),
            )),
            Some(Label::at(lap_time, main.x + 10., 52., lap_style)),
            Some(Label::at(gates, strip.x + 10., 86., modern(16., CREAM))),
            Some(Label::right(
                race_time,
                strip.x + strip.w - 10.,
                86.,
                modern(16., CREAM),
            )),
            delta,
        ]
    }
}

fn width_of(s: &str, style: TextStyle) -> f32 {
    width(s, style)
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
    // After the finish the lap timer has reset; show the final lap instead.
    let lap_time = clock(if race.finished() {
        race.last_lap.unwrap_or(0.)
    } else {
        race.lap_time
    });
    let labels = bar_labels(
        w,
        line!("Lap {}/{LAPS}", (race.laps + 1).min(LAPS)),
        line!("{} km/h", speed_kmh(race)),
        lap_time,
        line!("Gate {}/16", race.checkpoints_passed()),
        line!("Race {}", clock(race.total_time)),
        juice
            .last_split
            .map(crate::juice::split_label)
            .map(|(s, c)| (Line::of(format_args!("{s}")), c)),
    );
    for label in labels.iter().flatten() {
        put(&label.text, label.x, label.y, label.style);
    }
    // Speed gauge: an amber speedometer arc (wide) or meter (narrow).
    let fraction = (speed_kmh(race) as f32 / (265. * 0.48)).clamp(0., 1.);
    let hot = |t: f32| {
        shapes::mix(
            Color::new(1., 0.86, 0.45, 1.),
            Color::new(1., 0.48, 0.3, 1.),
            t,
        )
    };
    if let [_, _, Some(right)] = chips {
        let c = vec2(right.x + 30., right.y + 32.);
        let start = std::f32::consts::PI * 0.75;
        let sweep = std::f32::consts::PI * 1.5;
        shapes::stroke_arc(
            c,
            16.,
            start,
            sweep,
            Stroke::new(5., Color::new(0., 0., 0., 0.35)).cap(Cap::Round),
        );
        if fraction > 0.01 {
            shapes::stroke_arc(
                c,
                16.,
                start,
                sweep * fraction,
                Stroke::new(5., hot(fraction)).cap(Cap::Round),
            );
        }
        let needle = start + sweep * fraction;
        shapes::fill_circle(
            c + Vec2::from_angle(needle) * 16.,
            3.,
            Color::new(1., 0.97, 0.88, 1.),
        );
    } else {
        let main = chips[0].expect("narrow chip");
        let gauge = Rect::new(main.x + main.w - 10. - 64., 32., 64., 4.);
        shapes::fill_rounded_rect(gauge, 2., Color::new(0., 0., 0., 0.35));
        if fraction > 0.02 {
            let fill = Rect::new(gauge.x, gauge.y, gauge.w * fraction, gauge.h);
            shapes::fill_rounded_rect(fill, 2., hot(fraction));
        }
    }
    // Lap pips beside the gauge on wide screens.
    if let [Some(left), _, _] = chips
        && w >= 640.
    {
        for i in 0..LAPS {
            let c = vec2(
                left.x + left.w - 20. - (LAPS - 1 - i) as f32 * 14.,
                left.y + 18.,
            );
            let done = i < race.laps;
            let current = i == race.laps.min(LAPS - 1) && !race.finished();
            shapes::fill_circle(
                c,
                4.,
                if done {
                    GOLD
                } else if current {
                    AMBER
                } else {
                    Color::new(1., 1., 1., 0.18)
                },
            );
        }
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

pub fn minimap(
    track: &Track,
    race: &Race,
    ghost: Option<Pose>,
    rect: Rect,
    best: Option<Line>,
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
        let style = fit(&best, modern(16., MUTED), rect.w - 12.);
        put_centered(
            &best,
            rect.x + rect.w / 2.,
            rect.y + rect.h + 14.,
            style,
            rect.w - 12.,
        );
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

/// Banner below the chips: off-track and wrong-way guidance outrank toasts,
/// and secrets (alert toasts) briefly outrank everything.
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
    let entering = |age: f32| {
        Entrance::fade_up(10.)
            .with_duration(0.28)
            .sample(age, 0, motion)
    };
    let (head, sub, color, sub_color, alpha, offset): (&str, &str, Color, Color, f32, f32) =
        if let Some(toast) = alert {
            let look = entering(toast.age);
            (
                &toast.head,
                &toast.sub,
                toast.color,
                toast.sub_color,
                toast.alpha() * look.alpha,
                look.offset[1],
            )
        } else if session.off_track() {
            (
                quips::off_track(style.stats.off_track_episodes.max(1)),
                "Follow the amber arrow back to the road",
                AMBER,
                CREAM,
                1.,
                0.,
            )
        } else if wrong_way && !autoplay_reversing {
            (
                "Wrong way!",
                quips::wrong_way(style.stats.wrong_way_episodes.max(1)),
                RED,
                CREAM,
                1.,
                0.,
            )
        } else if let Some(toast) = &juice.toast {
            let look = entering(toast.age);
            (
                &toast.head,
                &toast.sub,
                toast.color,
                toast.sub_color,
                toast.alpha() * look.alpha,
                look.offset[1],
            )
        } else if autoplay_reversing {
            ("Auto recovery", "Reversing", MUTED, CREAM, 1., 0.)
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
    let (head_style, sub_style) = if wide {
        (modern(24., color), modern(16., sub_color))
    } else {
        (modern(16., color), modern(16., sub_color))
    };
    let content = width_of(head, head_style)
        .max(width_of(sub, sub_style))
        .min(max_width - 40.);
    let panel = Rect::new(
        (width - content - 40.) / 2.,
        banner_top(width) + offset,
        content + 40.,
        if sub.is_empty() { 40. } else { 62. },
    );
    surface(panel, 16., SURFACE, alpha);
    shapes::fill_capsule(
        vec2(panel.x + 12., panel.y + 12.),
        vec2(panel.x + 12., panel.y + panel.h - 12.),
        2.5,
        fade(color, alpha),
    );
    let cx = panel.x + panel.w / 2. + 4.;
    put_centered(
        head,
        cx,
        panel.y + if wide { 30. } else { 26. },
        fade_style(head_style, alpha),
        content,
    );
    if !sub.is_empty() {
        put_centered(
            sub,
            cx,
            panel.y + 50.,
            fade_style(sub_style, alpha),
            content,
        );
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
        let at = view.screen(race.car.pose.position) + vec2(0., -44.);
        let multiplier = style.next_multiplier();
        let label = if multiplier > 1 {
            line!("DRIFT {live} X{multiplier}")
        } else {
            line!("DRIFT {live}")
        };
        put_centered(&label, at.x, at.y, classic(16., GOLD), view.width - 16.);
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
            -std::f32::consts::FRAC_PI_2,
            std::f32::consts::TAU * progress,
            Stroke::new(2.5, GOLD).cap(Cap::Round),
        );
    }
}

/// Short onboarding lessons that disappear once each is learned.
pub fn coach(
    race: &Race,
    session: &RaceSession,
    style: &Style,
    hints: &Hints,
    attempt: u32,
) -> Option<Line> {
    if race.finished() || session.countdown() > 0 || session.off_track() {
        return None;
    }
    if race.ticks > 150 && race.ticks < 600 && race.car.velocity.length() < 15. {
        return Some(line!("Hold {} to drive!", hints.gas));
    }
    if attempt == 0 && style.stats.drifts == 0 && race.laps < 2 && race.ticks > 240 {
        return Some(line!(
            "Tip: hold {} in bends to drift for style",
            hints.drift
        ));
    }
    None
}

pub fn coach_chip(label: &str, width: f32, height: f32, touch: bool) {
    let style = modern(16., Color::from_hex(0xffe3a3));
    let w = (width_of(label, style) + 36.).min(width - 16.);
    let y = height - if touch { 172. } else { 54. };
    let rect = Rect::new((width - w) / 2., y, w, 32.);
    surface(rect, 16., SURFACE, 1.);
    shapes::fill_circle(vec2(rect.x + 14., rect.y + 16.), 3., GOLD);
    put_centered(label, width / 2. + 6., rect.y + 22., style, w - 30.);
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
    let sub_look = Entrance::fade_up(10.)
        .with_duration(0.5)
        .sample(t, 2, motion);
    let cy = (card.y - 52.).max(top + 52.) + look.offset[1];
    let bob = (juice.time * 2.).sin() * 2. * juice.motion().min(1.);
    let size = if width >= 640. { 48. } else { 32. };
    let style = TextStyle::classic(size, fade(Color::from_hex(0xffd166), look.alpha))
        .with_outline(fade(OUTLINE, look.alpha))
        .with_shadow(fade(Color::new(0., 0., 0., 0.35), look.alpha));
    put_centered("POCKET RALLY", width / 2., cy + bob, style, width - 32.);
    put_centered(
        "A tiny time trial through Valle Verde",
        width / 2.,
        cy + 30.,
        modern(16., fade(Color::from_hex(0xf4efe2), sub_look.alpha)),
        width - 32.,
    );
}

/// Countdown card: exactly two cream labels (title and number) inside the
/// panel, plus start lamps and key prompts. Browser glyph tests read it.
#[allow(clippy::too_many_arguments)]
pub fn countdown(
    session: &RaceSession,
    juice: &Juice,
    hints: &Hints,
    attempt: u32,
    record: Option<Line>,
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
        prompts(
            &[(&hints.gas, "Drive"), (&hints.drift, "Drift")],
            cx,
            top + 142.,
            content,
            &theme,
        );
    } else {
        let label = record.unwrap_or_else(|| line!("Gold medal under {:.0}s", quips::MEDALS[0]));
        put_centered(&label, cx, top + 160., modern(16., GOLD), content);
    }
}

fn shade(c: Color, k: f32) -> Color {
    Color::new(c.r * k, c.g * k, c.b * k, c.a)
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

// ---------------------------------------------------------------------------
// Pause.

pub fn pause_rect(width: f32, height: f32) -> Rect {
    let (w, h) = ((width - 32.).clamp(1., 400.), 206.);
    Rect::new(
        (width - w) / 2.,
        ((height - h) / 2.).max(MENU_CORNER.y + 4.),
        w,
        h,
    )
}

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
    put_centered(
        quips::pause(seed),
        cx,
        inner.y + 18.,
        modern(16., fade(theme.muted, look.alpha)),
        inner.w,
    );
    put_centered(
        &line!(
            "Lap {}/{LAPS}  ·  Race {}",
            (race.laps + 1).min(LAPS),
            clock(race.total_time)
        ),
        cx,
        inner.y + 46.,
        modern(16., fade(theme.ink, look.alpha)),
        inner.w,
    );
    let mut small = theme;
    small.body = if inner.w < 300. { 12. } else { 16. };
    let items: [(&str, &str); 3] = [
        (&hints.pause, "Resume"),
        (&hints.restart, "Restart"),
        ("F2", "Controls"),
    ];
    let count = if hints.keyboard { 3 } else { 2 };
    prompts(&items[..count], cx, inner.y + 64., inner.w, &small);
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

pub fn record_line(time: f32, results: &Results) -> (Line, Color) {
    if results.demo {
        (line!("Autoplay demo · not recorded"), MUTED)
    } else if let Some(best) = results.previous_best {
        if results.new_best(time) {
            (line!("New personal best!  -{:.2}s", best - time), GOLD)
        } else {
            (
                line!("Personal best {}  (+{:.2}s)", clock(best), time - best),
                CREAM,
            )
        }
    } else {
        (line!("First finish! That's the time to beat"), GOLD)
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

/// The podium card. Layout is authored at 440×440 and scaled to fit; rows
/// enter with a staggered fade-up and the time and style roll up.
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

    // Medal-coloured accent band across the top.
    shapes::fill_rounded_rect(
        Rect::new(rect.x + 24. * k, rect.y, rect.w - 48. * k, 4. * k),
        2. * k,
        fade(medal_color(medal), a0),
    );
    let l = row(1);
    put_centered(
        "Finish!",
        cx,
        at(34., l),
        modern_k(16., GREEN, l.alpha),
        content,
    );
    let l = row(2);
    let stamp = if motion.is_reduced() {
        1.
    } else {
        1. + 0.5 * (1. - Ease::BackOut.apply(((elapsed - 0.1) / 0.3).clamp(0., 1.)))
    };
    let title = TextStyle::classic(32. * k * stamp, fade(GOLD, l.alpha))
        .with_outline(fade(OUTLINE, l.alpha));
    put_centered(rank.title, cx, at(76., l), title, content);
    let l = row(3);
    put_centered(
        rank.quip,
        cx,
        at(102., l),
        modern_k(16., CREAM, l.alpha),
        content,
    );

    // Medal badge and rolling race time.
    let l = row(4);
    let centis = (f64::from(total) * 100.).round() as i64;
    rolls.time.set_target(centis);
    let shown = rolls.time.shown() as f32 / 100.;
    let time = clock(shown);
    let size = 40. * k;
    let time_style =
        TextStyle::classic(size, fade(CREAM, l.alpha)).with_outline(fade(OUTLINE, l.alpha));
    let group = 58. * k + width_of(&time, time_style);
    let badge = vec2(cx - group / 2. + 24. * k, at(150., l));
    let mc = medal_color(medal);
    post::glow_circle(badge.x, badge.y, 44. * k, fade(mc, l.alpha), 0.5);
    shapes::fill_circle(
        badge,
        24. * k,
        Fill::radial(
            badge - vec2(8., 8.) * k,
            34. * k,
            fade(shade(mc, 1.15), l.alpha),
            fade(shade(mc, 0.75), l.alpha),
        ),
    );
    shapes::stroke_circle(
        badge,
        24. * k,
        2. * k,
        fade(Color::new(1., 1., 1., 0.5), l.alpha),
    );
    put_centered(
        &quips::medal_name(medal)[..1],
        badge.x,
        badge.y + 12. * k,
        TextStyle::classic(24. * k, fade(INK, l.alpha)),
        40. * k,
    );
    put(&time, badge.x + 34. * k, badge.y + 18. * k, time_style);

    let l = row(5);
    let medal_label = match medal {
        Medal::Unranked => line!("No medal yet"),
        other => line!("{} medal", title_case(quips::medal_name(other))),
    };
    put_centered(
        &medal_label,
        cx,
        at(200., l),
        modern_k(16., mc, l.alpha),
        content,
    );
    let l = row(6);
    let (record, record_color) = record_line(total, results);
    put_centered(
        &record,
        cx,
        at(226., l),
        modern_k(16., record_color, l.alpha),
        content,
    );
    let l = row(7);
    let hook = quips::hook(medal, total, &style.stats);
    put_centered(
        &hook,
        cx,
        at(252., l),
        modern_k(16., GREEN, l.alpha),
        content,
    );

    // Stats tiles.
    let stats = &style.stats;
    rolls.style.set_target(i64::from(stats.style));
    let style_shown = rolls.style.shown();
    let style_record = !results.demo && results.previous_style.is_none_or(|s| stats.style > s);
    let best_lap = race.best_time.map_or(line!("--"), clock);
    let cells: [(&str, Line, Color); 6] = [
        ("Best lap", best_lap, CREAM),
        (
            if style_record {
                "Style record"
            } else {
                "Style"
            },
            line!("{style_shown}"),
            if style_record && stats.style > 0 {
                GOLD
            } else {
                CREAM
            },
        ),
        ("Best drift", line!("{}", stats.best_drift), CREAM),
        (
            "Clean laps",
            line!("{}/{LAPS}", stats.clean_laps),
            if stats.perfect() { GREEN } else { CREAM },
        ),
        ("Grass", line!("{:.1}s", stats.grass_seconds()), CREAM),
        (
            "Ducks",
            if stats.splashes > 0 {
                line!("Bothered")
            } else {
                line!("?")
            },
            if stats.splashes > 0 { WATER } else { MUTED },
        ),
    ];
    let gap = 8. * k;
    let tile_w = (rect.w - 32. * k - gap * 2.) / 3.;
    for (i, (label, value, color)) in cells.iter().enumerate() {
        let l = row(8 + i);
        let col = (i % 3) as f32;
        let line_row = (i / 3) as f32;
        let tile = Rect::new(
            rect.x + 16. * k + col * (tile_w + gap),
            at(272. + line_row * 58., l),
            tile_w,
            50. * k,
        );
        shapes::fill_rounded_rect(tile, 10. * k, fade(Color::new(1., 1., 1., 0.05), l.alpha));
        let tc = tile.x + tile.w / 2.;
        put_centered(
            label,
            tc,
            tile.y + 18. * k,
            modern_k(16., MUTED, l.alpha),
            tile.w - 8.,
        );
        put_centered(
            value,
            tc,
            tile.y + 42. * k,
            modern_k(16., *color, l.alpha),
            tile.w - 8.,
        );
    }
    let l = row(14);
    let pulse = if motion.is_reduced() {
        1.
    } else {
        0.8 + 0.2 * (juice.celebrate * 4.).sin()
    };
    let mut kit_theme = theme().faded(l.alpha * pulse);
    kit_theme.body = 16. * k;
    if hints.touch {
        put_centered(
            "Tap to race again",
            cx,
            at(418., l),
            modern_k(16., GOLD, l.alpha * pulse),
            content,
        );
    } else {
        prompts(
            &[(&hints.restart, "Race again")],
            cx,
            at(398., l),
            content,
            &kit_theme,
        );
    }
}

fn title_case(word: &str) -> Line {
    let mut out = Line::new();
    for (i, c) in word.chars().enumerate() {
        if i == 0 {
            let _ = out.write_char(c);
        } else {
            for lower in c.to_lowercase() {
                let _ = out.write_char(lower);
            }
        }
    }
    out
}

/// Advance the results roll counters by render time.
pub fn advance_rolls(rolls: &mut CardRolls, dt: f32, motion: Motion) {
    rolls.time.advance(dt, motion);
    rolls.style.advance(dt, motion);
}

// The restart/startup iris centered on the car.
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
        assert_eq!(title_case("SILVER"), "Silver");
    }

    #[test]
    fn record_lines_never_claim_a_demo_best() {
        let first = Results::default();
        assert!(first.new_best(40.));
        assert!(record_line(40., &first).0.starts_with("First finish"));
        let chase = Results {
            previous_best: Some(44.),
            ..Results::default()
        };
        assert_eq!(record_line(43.5, &chase).0, "New personal best!  -0.50s");
        assert_eq!(
            record_line(45., &chase).0,
            "Personal best 00:44.00  (+1.00s)"
        );
        let demo = Results {
            demo: true,
            ..Results::default()
        };
        assert!(!demo.new_best(1.));
        assert!(record_line(1., &demo).0.contains("not recorded"));
    }

    #[test]
    fn onboarding_tips_wait_for_go_and_fade_once_learned() {
        use crate::sim::Controls;
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut session = RaceSession::new();
        let mut style = Style::new();
        let hints = Hints {
            gas: "W".into(),
            drift: "Space".into(),
            restart: "R".into(),

            pause: "Escape".into(),

            touch: false,
            keyboard: true,
        };
        assert!(
            coach(&race, &session, &style, &hints, 0).is_none(),
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
            coach(&race, &session, &style, &hints, 0).unwrap(),
            "Hold W to drive!"
        );
        drive(&mut race, &mut session, 120, 1.);
        let tip = coach(&race, &session, &style, &hints, 0).unwrap();
        assert!(tip.contains("hold Space in bends"), "{tip}");
        assert!(
            coach(&race, &session, &style, &hints, 1).is_none(),
            "retries skip it"
        );
        style.stats.drifts = 1;
        assert!(
            coach(&race, &session, &style, &hints, 0).is_none(),
            "learned"
        );
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
            // Worst-case (widest) bar strings, with and without a split delta.
            for delta in [None, Some((line!("+9.99"), RED))] {
                let labels = bar_labels(
                    w,
                    line!("Lap 3/3"),
                    line!("127 km/h"),
                    line!("59:59.99"),
                    line!("Gate 15/16"),
                    line!("Race 59:59.99"),
                    delta,
                );
                assert!(labels.iter().flatten().count() >= 5);
                for label in labels.iter().flatten() {
                    clear(label.bounds(), &label.text);
                    // Every label sits inside one of the chips.
                    let b = label.bounds();
                    assert!(
                        bar_chips(w).iter().flatten().any(|c| b.x >= c.x
                            && b.x + b.w <= c.x + c.w
                            && b.y >= c.y - 2.
                            && b.y + b.h <= c.y + c.h + 2.),
                        "{} {b:?} escapes its chip at {w}x{h}",
                        label.text
                    );
                }
                // The five cream labels keep the browser glyph-test spacing.
                let [lap, speed, _, gates, race, _] = labels.map(|l| l.map(|l| l.bounds()));
                let (lap, speed, gates, race) =
                    (lap.unwrap(), speed.unwrap(), gates.unwrap(), race.unwrap());
                assert!(speed.x - (lap.x + lap.w) >= 8.);
                assert!(race.x - (gates.x + gates.w) >= 8.);
                assert!(gates.y + 1. < 80. && race.y + 1. < 80.);
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
}
