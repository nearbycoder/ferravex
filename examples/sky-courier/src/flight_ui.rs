//! Pixel-sized flight HUD; inactive menus replace conflicting navigation rows.
//!
//! Hierarchy while flying: clock/gates/hull top-left, tips and streak top-right,
//! one navigation capsule, one transient callout, one fading coach hint and
//! the telemetry footer. Menus are compact cards: the title, detail, summary and
//! button rows keep their established geometry, and a short extra block below
//! the button carries the customer reaction, personal best and next goal.
//!
//! [`Ui`] owns layout, pointer/keyboard focus and activation; everything
//! visible is painted with the modern UI kit (mixed-case face, rounded
//! translucent surfaces, key caps and meters). Painting allocates nothing:
//! formatted copy goes through a fixed [`Line`] buffer.
use super::presentation::{GOLD, MINT};
use super::*;
use ferravex_play::tween::Motion;
use ferravex_render::shapes::{self, Fill, Shadow};
use ferravex_render::text::{
    TextAlign, TextStyle as Type, draw_text_styled, text_width, wrap_text,
};
use ferravex_render::ui::kit::{self, ButtonKind, ButtonMotion, KitTheme};
use std::fmt::Write as _;

const MIST: Color = Color::new(0.72, 0.84, 0.84, 1.0);
/// Translucent flight surfaces: the sky stays visible through every chip.
const SURFACE: Color = Color::new(0.04, 0.12, 0.14, 0.74);
const SURFACE_TOP: Color = Color::new(0.09, 0.20, 0.22, 0.78);
/// Opaque card surfaces for inactive menus.
const CARD: Color = Color::new(0.06, 0.14, 0.16, 0.96);
const CARD_TOP: Color = Color::new(0.11, 0.23, 0.25, 0.97);
/// Sea-glass hairline around every surface.
const LINE: Color = Color::new(0.62, 0.86, 0.84, 0.22);
/// Unlit track for pips, strips and meters.
const TRACK: Color = Color::new(1.0, 1.0, 1.0, 0.16);
/// Airmail stripe blue for the card header.
const POST_BLUE: Color = Color::new(0.33, 0.58, 0.78, 1.0);
const SHADOW: Color = Color::new(0.0, 0.0, 0.0, 0.32);
/// Modern body size: one font pixel is two screen pixels.
const BODY: f32 = 16.;
const TITLE: f32 = 24.;
/// Baseline-to-baseline distance for stacked body rows.
const ROW: f32 = 22.;
const EXTRA_ROWS: usize = 4;
const EXTRA_ROW: f32 = ROW;
const TOP_HEIGHT: f32 = 40.;
const NAV_TOP: f32 = 52.;
const NAV_HEIGHT: f32 = 36.;
const GUIDE_HEIGHT: f32 = 88.;
const CALLOUT_TOP: f32 = 146.;
const CALLOUT: [f32; 2] = [420., 54.];
const COACH_HEIGHT: f32 = 36.;
/// The browser host pins a 44px game-menu button at the fullscreen canvas's
/// top-right corner (inset 8px). Keep HUD copy out of this screen region.
const MENU_CORNER: [f32; 2] = [72., 60.];
/// Stable identifier of the single menu button.
const MENU_BUTTON: u64 = 1;

struct Layout {
    viewport: Viewport,
    x: f32,
    width: f32,
    height: f32,
    narrow: bool,
    /// Largest layout-relative x a top-row element may reach while staying
    /// clear of the reserved menu corner.
    top_right: f32,
}

/// Top-row badges: clock/gate/hull on the left, tips and streak on the right.
struct TopBar {
    left: UiRect,
    left_strip: UiRect,
    /// Layout-relative x of the first hull heart.
    pips: f32,
    tips: UiRect,
    tips_text: UiRect,
    chip: UiRect,
    tips_strip: UiRect,
}

impl Layout {
    fn new(screen: [f32; 2]) -> Result<Self, ferravex_play::ui::UiError> {
        let viewport = Viewport::responsive(screen, [8.; 4])?;
        let [available, height] = viewport.bounds().size;
        let width = available.min(960.);
        let x = (available - width) * 0.5;
        let origin = viewport.pixels().min[0];
        Ok(Self {
            viewport,
            x,
            width,
            height,
            narrow: width < 600.,
            top_right: width.min(screen[0] - MENU_CORNER[0] - origin - x),
        })
    }

    fn top_bar(&self) -> TopBar {
        // Compact badges let both clusters fit left of the menu corner at 320px.
        let (left, pips, tips, chip) = if self.narrow {
            (124., 66., 110., 30.)
        } else {
            (260., 196., 170., 40.)
        };
        let right = self.top_right;
        let tips_x = right - tips;
        TopBar {
            left: self.rect(0., 0., left, TOP_HEIGHT),
            left_strip: self.rect(12., 31., left - 24., 3.),
            pips,
            tips: self.rect(tips_x, 0., tips, TOP_HEIGHT),
            tips_text: self.rect(tips_x + 10., 6., tips - chip - 16., 20.),
            chip: self.rect(right - chip - 6., 6., chip, 20.),
            tips_strip: self.rect(tips_x + 12., 31., tips - 24., 3.),
        }
    }

    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> UiRect {
        ui_rect(self.x + x, y, w.max(0.), h.max(0.))
    }

    fn row(&self, y: f32, height: f32) -> UiRect {
        self.rect(12., y, self.width - 24., height)
    }

    /// A horizontally centred capsule, never wider than the HUD.
    fn centered(&self, y: f32, width: f32, height: f32) -> UiRect {
        let width = width.min(self.width);
        self.rect((self.width - width) * 0.5, y, width, height)
    }

    fn menu(&self) -> MenuLayout {
        let width = self.width.min(420.);
        let left = (self.width - width) * 0.5;
        let title = 30.;
        let detail = ROW * 2.;
        let summary = ROW * 2.;
        let extra = EXTRA_ROWS as f32 * EXTRA_ROW;
        let height = 18. + title + 4. + detail + 8. + summary + 12. + 48. + 12. + extra + 10.;
        // Sit a little high so the aircraft stays visible beneath the card.
        // Keep top alignment in smaller native windows instead of shrinking text.
        let y = ((self.height - height) * 0.38).round().max(8.);
        let mut next = y + 18.;
        let mut row = |height, gap| {
            let rect = self.rect(left + 12., next, width - 24., height);
            next += height + gap;
            rect
        };
        let title = row(title, 4.);
        let detail = row(detail, 8.);
        let summary = row(summary, 12.);
        let button_y = next;
        let extra_y = button_y + 48. + 12.;
        let button_width = (width - 24.).min(240.);
        MenuLayout {
            panel: self.rect(left, y, width, height),
            title,
            detail,
            summary,
            button: self.rect(
                left + (width - button_width) * 0.5,
                button_y,
                button_width,
                48.,
            ),
            extra: std::array::from_fn(|i| {
                self.rect(
                    left + 12.,
                    extra_y + i as f32 * EXTRA_ROW,
                    width - 24.,
                    EXTRA_ROW,
                )
            }),
        }
    }

    fn footer(&self) -> UiRect {
        let height = 58.;
        self.rect(0., (self.height - height).max(0.), self.width, height)
    }

    /// Screen rectangle of a layout rectangle.
    fn screen(&self, rect: UiRect) -> Rect {
        kit::ui_rect(self.viewport, rect)
    }
}

struct MenuLayout {
    panel: UiRect,
    title: UiRect,
    detail: UiRect,
    summary: UiRect,
    button: UiRect,
    extra: [UiRect; EXTRA_ROWS],
}

impl MenuLayout {
    /// First row of the two-row detail block.
    fn detail_first(&self) -> UiRect {
        ui_rect(
            self.detail.min[0],
            self.detail.min[1],
            self.detail.size[0],
            ROW,
        )
    }
    /// Second row of the two-row detail block.
    fn detail_second(&self) -> UiRect {
        ui_rect(
            self.detail.min[0],
            self.detail.min[1] + ROW,
            self.detail.size[0],
            ROW,
        )
    }
}

/// Stored progress the cards can brag about; storage is optional.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Progress {
    /// Best delivered tips, if any delivery was ever recorded.
    pub best_tips: Option<u32>,
}

/// Snapshot taken when a shift ends, before this result was recorded.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Results {
    pub previous_best: Option<u32>,
    pub shift: u32,
    /// Autoplay results are shown but never recorded.
    pub demo: bool,
}

/// Onboarding hint shown above the footer until the player has done it once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hint {
    FirstRing,
    Boost,
    Centre,
}

/// Learned-once coaching. Ages on accepted ticks only, so pause freezes it.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Coach {
    passed_ring: bool,
    boosted: bool,
    bullseye: bool,
    current: Option<Hint>,
    alpha: f32,
}

impl Coach {
    pub(super) fn update(&mut self, flight: &Flight, seconds: f32) {
        self.passed_ring |= flight.next > 0;
        self.boosted |= flight.boosting;
        self.bullseye |= flight.ledger.bullseyes > 0;
        let wanted = if flight.outcome != Outcome::Flying || flight.recovery > 0. {
            None
        } else if !self.passed_ring {
            Some(Hint::FirstRing)
        } else if !self.boosted && flight.next < GATES {
            Some(Hint::Boost)
        } else if !self.bullseye && (2..GATES).contains(&flight.next) {
            Some(Hint::Centre)
        } else {
            None
        };
        if wanted == self.current && wanted.is_some() {
            self.alpha = (self.alpha + seconds * 4.).min(1.);
        } else {
            // Fade the old hint out before showing the next one.
            self.alpha = (self.alpha - seconds * 3.).max(0.);
            if self.alpha == 0. {
                self.current = wanted;
            }
        }
    }

    pub(super) fn visible(&self) -> Option<(Hint, f32)> {
        self.current
            .filter(|_| self.alpha > 0.)
            .map(|h| (h, self.alpha))
    }
}

/// Host-owned context the HUD reads; it never mutates game state.
pub(super) struct HudView<'a> {
    pub progress: Progress,
    pub results: &'a Results,
    pub coach: &'a Coach,
    pub pauses: u32,
}

/// Fixed-capacity formatted copy, so per-frame HUD text never allocates.
/// Overlong input is cut at a character boundary.
pub(super) struct Line {
    bytes: [u8; 160],
    len: usize,
}

impl Line {
    pub(super) const fn new() -> Self {
        Self {
            bytes: [0; 160],
            len: 0,
        }
    }
    pub(super) fn format(args: std::fmt::Arguments) -> Self {
        let mut line = Self::new();
        let _ = line.write_fmt(args);
        line
    }
    pub(super) fn as_str(&self) -> &str {
        // Only whole UTF-8 sequences are ever copied in.
        std::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl std::fmt::Write for Line {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let room = self.bytes.len() - self.len;
        let mut take = s.len().min(room);
        while !s.is_char_boundary(take) {
            take -= 1;
        }
        self.bytes[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        if take < s.len() {
            Err(std::fmt::Error)
        } else {
            Ok(())
        }
    }
}

fn faded(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0., 1.))
}

/// Modern-face copy without decoration (surfaces already give contrast).
fn copy(size: f32, color: Color) -> Type {
    Type::modern(size, color)
}

fn fits(text: &str, width: f32, size: f32) -> bool {
    text_width(text, copy(size, CREAM)) <= width
}

/// Kit palette matching the courier's teal, cream and amber livery.
fn theme() -> KitTheme {
    KitTheme {
        surface: CARD,
        raised: Color::new(0.15, 0.28, 0.30, 1.0),
        header: Color::new(0.19, 0.34, 0.36, 1.0),
        ink: CREAM,
        muted: MIST,
        accent: AMBER,
        accent_ink: INK,
        focus: Color::new(0.62, 0.95, 0.90, 1.0),
        line: LINE,
        ..KitTheme::default()
    }
}

/// Translucent rounded chip used for every in-flight surface. No drop shadow
/// or outline: the sky behind is already soft, and this keeps the HUD cheap.
fn surface(rect: Rect, radius: f32, alpha: f32) {
    shapes::fill_rounded_rect(
        rect,
        radius,
        Fill::vertical(rect, faded(SURFACE_TOP, alpha), faded(SURFACE, alpha)),
    );
}

/// [`surface`] plus the sea-glass hairline, for the anchoring footer.
fn edged_surface(rect: Rect, radius: f32, alpha: f32) {
    surface(rect, radius, alpha);
    shapes::stroke_rounded_rect(rect, radius, 1., faded(LINE, alpha));
}

/// Draws `text` vertically centred in `rect`, cut with an ellipsis if it
/// would overflow. Returns the drawn width.
fn text_in(rect: Rect, text: &str, style: Type, align: TextAlign) -> f32 {
    let mut line = Line::new();
    let text = ellipsize(text, rect.w, style, &mut line);
    let x = match align {
        TextAlign::Left => rect.x,
        TextAlign::Center => rect.x + rect.w * 0.5,
        TextAlign::Right => rect.x + rect.w,
    };
    let baseline = style.centered_baseline(rect.y + rect.h * 0.5);
    draw_text_styled(text, x, baseline, style.with_align(align)).width
}

/// `text` if it fits `width`, else its longest prefix plus `…`.
fn ellipsize<'a>(text: &'a str, width: f32, style: Type, out: &'a mut Line) -> &'a str {
    if text_width(text, style) <= width {
        return text;
    }
    let budget = width - text_width("…", style);
    let mut end = 0;
    for (at, c) in text.char_indices() {
        let next = at + c.len_utf8();
        if text_width(&text[..next], style) > budget {
            break;
        }
        end = next;
    }
    let _ = write!(out, "{}…", text[..end].trim_end());
    out.as_str()
}

/// Word-wrapped rows of `ROW` height starting at `rect`; the last permitted
/// row ends with an ellipsis if copy remains.
fn rows_in(rect: Rect, text: &str, style: Type, rows: usize) {
    let mut lines = wrap_text(text, rect.w, style);
    for row in 0..rows {
        let Some(line) = lines.next() else {
            return;
        };
        let slot = Rect::new(rect.x, rect.y + row as f32 * ROW, rect.w, ROW);
        if row + 1 == rows && lines.next().is_some() {
            // Keep the rest of the copy visible as a clear cut, not a clip.
            let rest = &text[line.as_ptr() as usize - text.as_ptr() as usize..];
            text_in(slot, rest, style, TextAlign::Center);
        } else {
            text_in(slot, line, style, TextAlign::Center);
        }
    }
}

/// A run of `count` rounded segments; `color(i)` picks each fill.
fn segments(rect: Rect, count: usize, gap: f32, color: impl Fn(usize) -> Color) {
    if count == 0 {
        return;
    }
    let width = (rect.w - gap * (count - 1) as f32) / count as f32;
    for i in 0..count {
        let r = Rect::new(rect.x + i as f32 * (width + gap), rect.y, width, rect.h);
        shapes::fill_rounded_rect(r, rect.h * 0.5, color(i));
    }
}

fn menu_copy(state: SessionState, hull: u8) -> (&'static str, &'static str, &'static str) {
    match state {
        SessionState::Ready => ("Sky Courier", "5 gates · 1 delivery", "Start flight"),
        SessionState::Paused => ("Flight paused", "", "Resume flight"),
        SessionState::Won => ("Mail delivered!", "", "Fly again"),
        SessionState::Lost if hull == 0 => ("Aircraft grounded", "3 impacts", "Retry flight"),
        SessionState::Lost => ("Time's up", "", "Retry flight"),
        SessionState::Playing => unreachable!(),
    }
}

// Hints are complete current bindings or an explicit route to the editor. Never
// clip a key name, substitute a default, or make the font smaller to fit a remap.
#[cfg(test)]
fn binding_fits(label: &str, first: &str, second: &str, width: f32) -> bool {
    let mut line = Line::new();
    let _ = write!(line, "{label}: {first}");
    if !second.is_empty() {
        let _ = write!(line, " {second}");
    }
    fits(line.as_str(), width, BODY)
}

/// Compact arrows keep navigation on one line even at 320 pixels.
fn nav_copy(nav: &Navigation, narrow: bool) -> (&'static str, &'static str) {
    let turn = match (nav.turn(), narrow) {
        ("TURN LEFT", false) => "← Turn left",
        ("TURN LEFT", true) => "← Left",
        ("TURN RIGHT", false) => "Turn right →",
        ("TURN RIGHT", true) => "Right →",
        (_, false) => "On course",
        (_, true) => "Ahead",
    };
    let climb = match (nav.climb(), narrow) {
        ("CLIMB", _) => "↑ Climb",
        ("DESCEND", false) => "↓ Descend",
        ("DESCEND", true) => "↓ Dive",
        (_, false) => "Hold altitude",
        (_, true) => "Level",
    };
    (turn, climb)
}

/// Friendly name for the simulation's flight mode.
fn mode_copy(mode: &str) -> &'static str {
    match mode {
        "SLOW FLIGHT" => "Slow flight",
        "BOOSTING" => "Boosting",
        "RECHARGING" => "Recharging",
        "FLIGHT ENDED" => "Flight ended",
        _ => "Cruise",
    }
}

/// Draws `pieces` left to right from `x` on one baseline; returns the end x.
fn run(x: f32, baseline: f32, pieces: &[(&str, Color)]) -> f32 {
    let mut cursor = x;
    for (text, color) in pieces {
        cursor += draw_text_styled(text, cursor, baseline, copy(BODY, *color)).width;
    }
    cursor
}

fn run_width(pieces: &[(&str, Color)]) -> f32 {
    pieces
        .iter()
        .map(|(text, _)| text_width(text, copy(BODY, CREAM)))
        .sum()
}

/// Key caps for a formatted binding ("Left Shift / Right Shift" → two caps)
/// followed by `action`: the caps and the prompt's total width.
fn key_caps<'a>(key: &'a str, action: &str) -> Option<([&'a str; 4], usize, f32)> {
    let mut keys = [""; 4];
    let mut count = 0;
    for part in key.split(" / ") {
        *keys.get_mut(count)? = part;
        count += 1;
    }
    // Mirrors kit::keycap/key_prompt spacing.
    Some((keys, count, key_caps_width(&keys[..count], action)))
}

fn key_caps_width(keys: &[&str], action: &str) -> f32 {
    let style = copy(BODY, CREAM);
    keys.iter()
        .map(|k| (text_width(k, style) + 24.).max(23.) + 4.)
        .sum::<f32>()
        + 4.
        + text_width(action, style)
}

/// What the coach pill shows: key caps plus an action, or a dotted line.
enum CoachCopy<'a> {
    Keys([&'a str; 4], usize, &'static str),
    Text(Line, Color),
}

fn queue_flight(
    run_state: &FlightRun,
    player: &PlayerInput,
    layout: &Layout,
    view: &HudView,
) -> Result<(), ferravex_play::ui::UiError> {
    let game = &run_state.flight;
    let ledger = &game.ledger;

    let seconds = game.countdown();
    let narrow = layout.narrow;
    let s = |rect: UiRect| layout.screen(rect);

    // Clock, target and hull, with the six-part route strip beneath.
    let top = layout.top_bar();
    surface(s(top.left), 12., 1.);
    let clock = Line::format(format_args!("{}:{:02}", seconds / 60, seconds % 60));
    let row = s(layout.rect(12., 5., 80., 22.));
    let urgent = if seconds <= 10 {
        CORAL
    } else if seconds <= 15 {
        AMBER
    } else {
        CREAM
    };
    text_in(row, clock.as_str(), copy(BODY, urgent), TextAlign::Left);
    if !narrow {
        let target = if game.next == GATES {
            Line::format(format_args!("Drop"))
        } else {
            Line::format(format_args!("Gate {}/{}", game.next + 1, GATES))
        };
        let label = s(layout.rect(84., 5., 104., 22.));
        shapes::fill_circle(
            vec2(label.x - 10., label.y + label.h * 0.5),
            2.,
            faded(MIST, 0.6),
        );
        text_in(label, target.as_str(), copy(BODY, CREAM), TextAlign::Left);
    }
    let hearts = s(layout.rect(top.pips, 5., 54., 22.));
    let heart = copy(BODY, CORAL);
    let step = text_width("♥", heart) + 4.;
    for i in 0..3 {
        let color = if i < game.hull { CORAL } else { TRACK };
        draw_text_styled(
            "♥",
            hearts.x + i as f32 * step,
            heart.centered_baseline(hearts.y + hearts.h * 0.5),
            heart.with_color(color),
        );
    }
    segments(s(top.left_strip), ROUTE.len(), 4., |i| {
        if i < game.next {
            faded(CREAM, 0.9)
        } else if i == game.next {
            AMBER
        } else {
            TRACK
        }
    });

    // Tips and the streak multiplier, mirrored on the right but kept clear of
    // the browser's fullscreen menu corner.
    let multiplier = ledger.multiplier();
    surface(s(top.tips), 12., 1.);
    let tips = Line::format(format_args!("${}", ledger.tips));
    text_in(
        s(top.tips_text),
        tips.as_str(),
        copy(BODY, GOLD),
        TextAlign::Left,
    );
    let chip = s(top.chip);
    let streak = Line::format(format_args!("×{multiplier}"));
    if multiplier > 1 {
        shapes::fill_rounded_rect(chip, chip.h * 0.5, GOLD);
        text_in(chip, streak.as_str(), copy(BODY, INK), TextAlign::Center);
    } else {
        text_in(chip, streak.as_str(), copy(BODY, MIST), TextAlign::Center);
    }
    segments(
        s(top.tips_strip),
        courier::MAX_MULTIPLIER as usize,
        4.,
        |i| {
            if (i as u32) < multiplier { GOLD } else { TRACK }
        },
    );

    let mut slow_buffer = [0; 96];
    let slow = control_hint(player, "brake", &mut slow_buffer);
    let mut callout_top = CALLOUT_TOP;
    if let Some(nav) = game.navigation() {
        let delivery = game.next == GATES;
        let behind = nav.bearing.abs() > std::f32::consts::FRAC_PI_2;
        let (turn, climb) = nav_copy(&nav, narrow);
        let distance = Line::format(format_args!(
            "{:.0} m{}",
            nav.distance,
            if delivery && !narrow { " to post" } else { "" }
        ));
        let pieces = [
            (turn, AMBER),
            ("  ·  ", MIST),
            (climb, AMBER),
            ("  ·  ", MIST),
            (distance.as_str(), CREAM),
        ];
        // Guidance rows under the capsule: two lines, or three when a long
        // remapped slow key must wrap (never clipped or shrunk).
        let warn = Line::format(format_args!("Slow down: {slow}"));
        let body_rect = s(layout.row(NAV_TOP + NAV_HEIGHT, ROW));
        let (head, detail): (&str, &str) = if behind {
            ("Turn back", "The route is behind you")
        } else if delivery && game.motor.speed > DELIVERY_SPEED {
            (warn.as_str(), "At most 18 m/s (35 kt)")
        } else if delivery {
            ("Speed ready", "Drop at the post · 16 m up")
        } else {
            ("", "")
        };
        let head_rows = if head.is_empty() {
            0
        } else {
            wrap_text(head, body_rect.w, copy(BODY, CREAM))
                .count()
                .clamp(1, 2)
        };
        let guide_rows = head_rows + usize::from(head_rows > 0);
        let line_width = run_width(&pieces);
        let pill_width = if guide_rows > 0 {
            440.
        } else {
            (line_width + 40.).max(200.)
        };
        let height = if guide_rows > 0 {
            NAV_HEIGHT + guide_rows as f32 * ROW + 8.
        } else {
            NAV_HEIGHT
        };
        callout_top += (height - GUIDE_HEIGHT).max(0.);
        let pill = s(layout.centered(NAV_TOP, pill_width, height));
        surface(
            pill,
            if guide_rows > 0 {
                16.
            } else {
                NAV_HEIGHT * 0.5
            },
            1.,
        );
        let style = copy(BODY, AMBER);
        let start = pill.x + (pill.w - line_width).max(0.) * 0.5;
        run(
            start,
            style.centered_baseline(pill.y + NAV_HEIGHT * 0.5),
            &pieces,
        );
        if head_rows > 0 {
            let row =
                |i: usize| Rect::new(body_rect.x, body_rect.y + i as f32 * ROW, body_rect.w, ROW);
            let cream = copy(BODY, CREAM);
            if head_rows == 1 {
                text_in(row(0), head, cream, TextAlign::Center);
            } else {
                rows_in(row(0), head, cream, head_rows);
            }
            text_in(row(head_rows), detail, copy(BODY, MIST), TextAlign::Center);
        }
    }

    // One callout at a time: recovery outranks hidden rings, then gates.
    let mut sub = Line::new();
    let callout = if game.recovery > 0. && game.outcome == Outcome::Flying {
        if game.next == 0 {
            let _ = write!(sub, "-5 s · back at the start");
        } else {
            let _ = write!(sub, "-5 s · back at gate {}", game.next);
        }
        let _ = write!(sub, " · shield {:.1} s", game.recovery);
        Some((quips::impact(ledger.last_impact, ledger.impacts), CORAL, 1.))
    } else if game.bonus_notice > 0. {
        let found = ledger.bonus_count();
        let alpha = (game.bonus_notice / 0.3).min(1.);
        if ledger.scenic() {
            let _ = write!(
                sub,
                "+${} · all 3 hidden rings",
                courier::BONUS_TIP + courier::SCENIC_TIP
            );
            Some(("Scenic route complete!", MINT, alpha))
        } else {
            let _ = write!(
                sub,
                "{found}/3 found · +${} · boost topped up",
                courier::BONUS_TIP
            );
            Some((quips::bonus(found), MINT, alpha))
        }
    } else if game.gate_notice > 0. && game.next > 0 && game.next <= GATES {
        let gate = game.next - 1;
        ledger.gates[gate].map(|pass| {
            let _ = write!(sub, "+${} tip", pass.tip);
            if pass.hot {
                let _ = write!(sub, " · hot");
            }
            if let Some(milestone) = quips::streak(pass.multiplier) {
                let _ = write!(sub, " · {milestone}");
            } else if pass.multiplier > 1 {
                let _ = write!(sub, " · ×{} streak", pass.multiplier);
            }
            (
                quips::gate(pass.line, gate),
                if pass.line == courier::Line::Scrape {
                    CREAM
                } else {
                    GOLD
                },
                (game.gate_notice / 0.3).min(1.),
            )
        })
    } else {
        None
    };
    if let Some((head, color, alpha)) = callout {
        let body = copy(BODY, CREAM);
        let content = text_width(head, body).max(text_width(sub.as_str(), body));
        let rect = s(layout.centered(
            callout_top,
            (content + 48.).clamp(220., CALLOUT[0]),
            CALLOUT[1],
        ));
        surface(rect, 14., alpha);
        // The hairline picks up the event colour (gold tip, mint secret, coral hit).
        shapes::stroke_rounded_rect(rect, 14., 1.5, faded(color, alpha * 0.6));
        let inner = |y: f32| Rect::new(rect.x + 12., rect.y + y, rect.w - 24., ROW);
        text_in(
            inner(5.),
            head,
            copy(BODY, faded(color, alpha)),
            TextAlign::Center,
        );
        text_in(
            inner(27.),
            sub.as_str(),
            copy(BODY, faded(MIST, alpha)),
            TextAlign::Center,
        );
    }

    let footer = layout.footer();
    let bottom = footer.min[1];
    if let Some((hint, alpha)) = view.coach.visible() {
        let mut buffer = [0; 96];
        let limit = layout.width - 24.;
        let content = match hint {
            Hint::FirstRing => CoachCopy::Text(
                Line::format(format_args!("Steer through the amber ring")),
                AMBER,
            ),
            Hint::Centre => CoachCopy::Text(
                Line::format(format_args!("Thread the centre for bigger tips")),
                GOLD,
            ),
            Hint::Boost => {
                let key = control_hint(player, "boost", &mut buffer);
                if key == "Open Controls" || key == "Unbound" {
                    CoachCopy::Text(Line::format(format_args!("Rings refill your boost")), MINT)
                } else {
                    match key_caps(key, "Hold to boost") {
                        Some((keys, count, width)) if width + 36. <= limit => {
                            CoachCopy::Keys(keys, count, "Hold to boost")
                        }
                        // Never clip a remapped key: spell it out instead.
                        _ => CoachCopy::Text(
                            Line::format(format_args!("Hold {key} to boost")),
                            AMBER,
                        ),
                    }
                }
            }
        };
        let style = copy(BODY, faded(CREAM, alpha));
        let content_width = match &content {
            CoachCopy::Keys(keys, count, action) => key_caps_width(&keys[..*count], action),
            CoachCopy::Text(line, _) => text_width(line.as_str(), style) + 14.,
        };
        let pill = s(layout.centered(
            bottom - COACH_HEIGHT - 8.,
            content_width + 36.,
            COACH_HEIGHT,
        ));
        surface(pill, COACH_HEIGHT * 0.5, alpha * 0.95);
        let x = pill.x + (pill.w - content_width).max(0.) * 0.5;
        match &content {
            CoachCopy::Keys(keys, count, action) => {
                let cap_y = pill.y + (pill.h - 26.) * 0.5;
                kit::key_prompt(x, cap_y, &keys[..*count], action, &theme().faded(alpha));
            }
            CoachCopy::Text(line, dot) => {
                shapes::fill_circle(vec2(x + 4., pill.y + pill.h * 0.5), 4., faded(*dot, alpha));
                text_in(
                    Rect::new(x + 14., pill.y, pill.w - 28., pill.h),
                    line.as_str(),
                    style,
                    TextAlign::Left,
                );
            }
        }
    }

    // Telemetry: speed, altitude and climb on the left, mode and boost on the
    // right, with the boost meter along the bottom edge.
    let panel = s(footer);
    edged_surface(panel, 14., 1.);
    let telemetry = game.motor.telemetry().expect("validated motor");
    let speed = Line::format(format_args!("{:.0}", game.motor.speed * 1.94));
    let altitude = Line::format(format_args!("{:.0}", game.pos.y.max(0.)));
    let climb = Line::format(format_args!("{:+.1}", telemetry.vertical_speed));
    let stats = [
        (speed.as_str(), CREAM),
        (" kt", MIST),
        ("   ", MIST),
        (altitude.as_str(), CREAM),
        (" m", MIST),
        ("   ", MIST),
        (climb.as_str(), CREAM),
        (" m/s", MIST),
    ];
    let style = copy(BODY, CREAM);
    let first_mid = panel.y + if narrow { 15. } else { 20. };
    run(panel.x + 14., style.centered_baseline(first_mid), &stats);
    let mode = mode_copy(game.flight_mode());
    let boost = Line::format(format_args!(
        "  ·  Boost {}%",
        (game.boost_energy * 100.) as u32
    ));
    let mode_pieces = [(mode, AMBER), (boost.as_str(), MIST)];
    let mode_width = run_width(&mode_pieces);
    let (mode_x, mode_mid) = if narrow {
        (panel.x + 14., panel.y + 35.)
    } else {
        (panel.x + panel.w - 14. - mode_width, first_mid)
    };
    run(mode_x, style.centered_baseline(mode_mid), &mode_pieces);
    let meter = if narrow {
        Rect::new(panel.x + 14., panel.y + 47., panel.w - 28., 5.)
    } else {
        Rect::new(panel.x + 14., panel.y + 38., panel.w - 28., 8.)
    };
    kit::meter(
        meter,
        game.boost_energy,
        None,
        if game.boost_exhausted { CORAL } else { AMBER },
        &theme(),
    );
    Ok(())
}

/// A finished, paused or ready card. Returns the menu button rectangle and
/// label for painting after input is resolved.
fn queue_menu(
    run_state: &FlightRun,
    ui: &mut Ui,
    player: &PlayerInput,
    layout: &Layout,
    view: &HudView,
) -> Result<(UiRect, &'static str), ferravex_play::ui::UiError> {
    let game = &run_state.flight;
    let ledger = &game.ledger;
    let state = run_state.session.state();
    let menu = layout.menu();
    let s = |rect: UiRect| layout.screen(rect);
    let (title, detail, button) = menu_copy(state, game.hull);
    let text_width_limit = menu.detail.size[0];
    let card = s(menu.panel);
    shapes::drop_shadow(
        card,
        18.,
        Shadow::soft(SHADOW).offset(vec2(0., 8.)).blur(28.),
    );
    shapes::fill_rounded_rect(card, 18., Fill::vertical(card, CARD_TOP, CARD));
    shapes::stroke_rounded_rect(card, 18., 1., LINE);
    // Airmail edging: a short run of coral and blue bars above the title.
    let stripes = 9;
    let bar = 14.;
    let gap = 6.;
    let total = stripes as f32 * bar + (stripes - 1) as f32 * gap;
    let x0 = card.x + (card.w - total) * 0.5;
    for i in 0..stripes {
        let x = x0 + i as f32 * (bar + gap);
        let color = if i % 2 == 0 { CORAL } else { POST_BLUE };
        let y = card.y + 8.;
        draw_triangle(
            vec2(x + 3., y),
            vec2(x + bar + 3., y),
            vec2(x, y + 4.),
            color,
        );
        draw_triangle(
            vec2(x, y + 4.),
            vec2(x + bar + 3., y),
            vec2(x + bar, y + 4.),
            color,
        );
    }
    let title_size = if fits(title, menu.title.size[0], TITLE) {
        TITLE
    } else {
        BODY
    };
    text_in(
        s(menu.title),
        title,
        copy(title_size, CREAM),
        TextAlign::Center,
    );
    // The cream detail line is fixed copy; everything else uses other inks.
    if !detail.is_empty() {
        text_in(
            s(menu.detail_first()),
            detail,
            copy(BODY, CREAM),
            TextAlign::Center,
        );
    }
    let first = s(menu.detail_first());
    let second = s(menu.detail_second());
    let summary = s(menu.summary);
    let summary_rows = [
        Rect::new(summary.x, summary.y, summary.w, ROW),
        Rect::new(summary.x, summary.y + ROW, summary.w, ROW),
    ];
    let finished = matches!(state, SessionState::Won | SessionState::Lost);
    let delivered = state == SessionState::Won;
    let mut extra: [(Line, Color, usize); EXTRA_ROWS] =
        std::array::from_fn(|_| (Line::new(), MIST, 1));
    let set = |slot: &mut (Line, Color, usize), args: std::fmt::Arguments, color, lines| {
        *slot = (Line::format(args), color, lines);
    };
    if finished {
        let (rank, next) = quips::rank(ledger, delivered, game.hull, game.next.min(GATES));
        let standing = if let Some(drop) = ledger.drop {
            Line::format(format_args!(
                "{} · shift #{}",
                drop.grade.name(),
                view.results.shift
            ))
        } else if game.hull == 0 {
            Line::new()
        } else {
            Line::format(format_args!(
                "The clock won · shift #{}",
                view.results.shift
            ))
        };
        if detail.is_empty() {
            text_in(first, rank, copy(BODY, GOLD), TextAlign::Center);
            text_in(
                second,
                standing.as_str(),
                copy(BODY, MIST),
                TextAlign::Center,
            );
        } else {
            text_in(second, rank, copy(BODY, GOLD), TextAlign::Center);
        }
        let (one, two) = if delivered {
            (
                Line::format(format_args!(
                    "Tips ${} · best ×{}",
                    ledger.tips,
                    ledger.best_streak.min(courier::MAX_MULTIPLIER)
                )),
                Line::format(format_args!(
                    "{:.1} s · {} hits · rings {}/3",
                    run_state.elapsed,
                    3 - game.hull,
                    ledger.bonus_count()
                )),
            )
        } else {
            (
                Line::format(format_args!("Unpaid tips ${}", ledger.tips)),
                Line::format(format_args!(
                    "Gates {}/{} · -{} s penalty",
                    game.next.min(GATES),
                    GATES,
                    (3 - game.hull) * 5
                )),
            )
        };
        text_in(
            summary_rows[0],
            one.as_str(),
            copy(BODY, AMBER),
            TextAlign::Center,
        );
        text_in(
            summary_rows[1],
            two.as_str(),
            copy(BODY, AMBER),
            TextAlign::Center,
        );
        let customer = quips::customer(ledger, delivered, game.hull, view.results.shift);
        set(&mut extra[0], format_args!("“{customer}”"), MIST, 2);
        let previous = view.results.previous_best;
        let tips = ledger.tips;
        match (view.results.demo, delivered, previous) {
            (true, _, _) => set(
                &mut extra[2],
                format_args!("Demo flight · not recorded"),
                MIST,
                1,
            ),
            (false, false, Some(best)) => set(
                &mut extra[2],
                format_args!("Best ${best} still stands"),
                GOLD,
                1,
            ),
            (false, false, None) => set(
                &mut extra[2],
                format_args!("Deliver to set a best"),
                GOLD,
                1,
            ),
            (false, true, None) => set(
                &mut extra[2],
                format_args!("First delivery on record!"),
                MINT,
                1,
            ),
            (false, true, Some(best)) if tips > best => set(
                &mut extra[2],
                format_args!("New best! Old ${best}"),
                MINT,
                1,
            ),
            (false, true, Some(best)) if tips == best => set(
                &mut extra[2],
                format_args!("Tied your best ${best}"),
                GOLD,
                1,
            ),
            (false, true, Some(best)) => set(
                &mut extra[2],
                format_args!("Best ${best} · ${} to beat", best - tips),
                GOLD,
                1,
            ),
        }
        match next {
            _ if delivered && ledger.bonus_count() < 3 && ledger.bonus_count() > 0 => set(
                &mut extra[3],
                format_args!("Hidden rings {}/3 · keep looking", ledger.bonus_count()),
                MIST,
                1,
            ),
            Some((tips, title)) if delivered => {
                let long = Line::format(format_args!("Next: {title} at ${tips}"));
                if fits(long.as_str(), text_width_limit, BODY) {
                    extra[3] = (long, MIST, 1);
                } else {
                    set(
                        &mut extra[3],
                        format_args!("Next title at ${tips}"),
                        MIST,
                        1,
                    );
                }
            }
            Some((_, goal)) => set(&mut extra[3], format_args!("{goal}"), MIST, 1),
            None => set(
                &mut extra[3],
                format_args!("Top rank. The gulls bow."),
                MINT,
                1,
            ),
        }
    } else {
        // Ready and pause keep the established rules summary, centred in
        // its two-row block.
        text_in(
            summary,
            "90 seconds · 3 hull",
            copy(BODY, AMBER),
            TextAlign::Center,
        );
        if state == SessionState::Ready {
            text_in(
                second,
                "Thread rings for tips",
                copy(BODY, GOLD),
                TextAlign::Center,
            );
            set(
                &mut extra[0],
                format_args!("Clean streaks multiply ×5"),
                MIST,
                1,
            );
            set(
                &mut extra[1],
                format_args!("Slow down for the drop"),
                MIST,
                1,
            );
            match view.progress.best_tips {
                Some(best) => set(&mut extra[2], format_args!("Your best: ${best}"), GOLD, 1),
                None => set(
                    &mut extra[2],
                    format_args!("First shift. No pressure."),
                    GOLD,
                    1,
                ),
            }
            set(
                &mut extra[3],
                format_args!("3 golden rings hide nearby"),
                MINT,
                1,
            );
        } else {
            text_in(
                first,
                quips::pause(view.pauses),
                copy(BODY, GOLD),
                TextAlign::Center,
            );
            let seconds = game.countdown();
            set(
                &mut extra[0],
                format_args!("Tips ${} · ×{}", ledger.tips, ledger.multiplier()),
                GOLD,
                1,
            );
            if game.next >= GATES {
                set(
                    &mut extra[1],
                    format_args!("{}:{:02} left · drop next", seconds / 60, seconds % 60),
                    MIST,
                    1,
                );
            } else {
                set(
                    &mut extra[1],
                    format_args!(
                        "{}:{:02} left · gate {}/{}",
                        seconds / 60,
                        seconds % 60,
                        game.next + 1,
                        GATES
                    ),
                    MIST,
                    1,
                );
            }
            set(
                &mut extra[2],
                format_args!("Hidden rings {}/3", ledger.bonus_count()),
                MINT,
                1,
            );
            let mut buffer = [0; 96];
            let restart = control_hint(player, "restart", &mut buffer);
            let line = Line::format(format_args!("{restart} restarts"));
            if fits(line.as_str(), text_width_limit, BODY)
                && restart != "Open Controls"
                && restart != "Unbound"
            {
                extra[3] = (line, MIST, 1);
            } else {
                set(
                    &mut extra[3],
                    format_args!("Restart: see Controls"),
                    MIST,
                    1,
                );
            }
        }
    }
    // A hairline separates the action from the card's footnotes.
    let rule = s(menu.extra[0]);
    shapes::fill_rounded_rect(
        Rect::new(rule.x + 24., rule.y - 7., (rule.w - 48.).max(0.), 1.),
        0.,
        LINE,
    );
    // Each entry starts on its own row; a two-line entry owns the next row.
    for (row, (text, color, lines)) in extra.iter().enumerate() {
        if text.as_str().is_empty() {
            continue;
        }
        let lines = (*lines).clamp(1, EXTRA_ROWS - row);
        rows_in(s(menu.extra[row]), text.as_str(), copy(BODY, *color), lines);
    }
    ui.button_with_options(
        MENU_BUTTON,
        menu.button,
        button,
        ButtonOptions {
            enabled: !player.controls_open(),
            selected: false,
        },
        kit::INVISIBLE_BUTTON,
    )?;
    Ok((menu.button, button))
}

/// Queues input widgets and paints everything except the menu button, whose
/// eased state is only known after [`Ui::finish_player`].
fn queue(
    run_state: &FlightRun,
    ui: &mut Ui,
    player: &PlayerInput,
    layout: &Layout,
    view: &HudView,
) -> Result<Option<(UiRect, &'static str)>, ferravex_play::ui::UiError> {
    if run_state.session.state() == SessionState::Playing {
        queue_flight(run_state, player, layout, view).map(|()| None)
    } else {
        // No navigation, recovery or telemetry beneath an inactive menu.
        queue_menu(run_state, ui, player, layout, view).map(Some)
    }
}

pub(super) fn hud(
    run_state: &FlightRun,
    ui: &mut Ui,
    player: &mut PlayerInput,
    accept_input: bool,
    view: &HudView,
    button_motion: &mut ButtonMotion,
    dt: f32,
) -> Result<bool, ferravex_play::ui::UiError> {
    let layout = Layout::new([screen_width(), screen_height()])?;
    ui.begin(layout.viewport);
    let button = queue(run_state, ui, player, &layout, view)?;
    ui.finish_player(
        player,
        accept_input && run_state.session.state() != SessionState::Playing,
    );
    ui.paint();
    if let Some((rect, label)) = button {
        let enabled = !player.controls_open();
        let motion = Motion::from(preferences::feedback_policy());
        button_motion.update(kit::ui_button_state(ui, MENU_BUTTON, enabled), dt, motion);
        kit::button(
            layout.screen(rect),
            label,
            ButtonKind::Primary,
            button_motion,
            &theme(),
        );
    }
    Ok(ui.activated(MENU_BUTTON))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_play::ui::{UiState, Widget};
    use ferravex_render::input::{Binding, InputCode};

    #[test]
    fn fullscreen_telemetry_reserves_most_of_the_viewport_for_flight() {
        for screen in [[320., 480.], [390., 844.], [844., 390.], [1280., 800.]] {
            let layout = Layout::new(screen).unwrap();
            let footer = layout.footer();
            assert_eq!(
                layout.viewport.scale(),
                1.,
                "HUD text stays in logical pixels"
            );
            assert_eq!(footer.size[1], 58.);
            assert!(footer.min[1] >= screen[1] * 0.75);
            let menu = layout.menu();
            // Cards replace flight entirely, so they may use more of a short
            // landscape screen, but must stay fully contained.
            assert!(menu.panel.size[1] <= 320.);
            assert_eq!(
                menu.panel.intersect(layout.viewport.bounds()),
                Some(menu.panel)
            );
        }
    }

    #[test]
    fn portrait_menu_is_readable_and_pointer_targets_match_paint() {
        for screen in [[320., 600.], [390., 600.], [1280., 560.], [1280., 800.]] {
            let layout = Layout::new(screen).unwrap();
            assert_eq!(layout.viewport.scale(), 1., "never shrink HUD fonts");
            let menu = layout.menu();
            let bounds = layout.viewport.bounds();
            assert_eq!(menu.panel.intersect(bounds), Some(menu.panel));
            let mut rows = vec![menu.title, menu.detail, menu.summary, menu.button];
            rows.extend(menu.extra);
            for rect in &rows {
                assert_eq!(rect.intersect(menu.panel), Some(*rect));
            }
            for pair in rows.windows(2) {
                assert!(pair[0].min[1] + pair[0].size[1] <= pair[1].min[1]);
            }
            for row in [menu.detail_first(), menu.detail_second()] {
                assert_eq!(row.intersect(menu.detail), Some(row));
            }
            assert!(menu.panel.size[1] <= 320., "menus stay compact");
            assert!(menu.button.size[0] >= 44. && menu.button.size[1] >= 44.);
            let mut input = UiState::new();
            input
                .add(Widget {
                    id: 1,
                    rect: menu.button,
                    clip: Some(bounds),
                    enabled: true,
                    focusable: true,
                })
                .unwrap();
            let logical = [menu.button.min[0] + 1., menu.button.min[1] + 1.];
            let pixels =
                std::array::from_fn(|axis| layout.viewport.pixels().min[axis] + logical[axis]);
            let point = layout.viewport.unproject(pixels).unwrap();
            assert_eq!(input.pointer_down(point), Some(1));
            assert_eq!(input.pointer_up(point), Some(1));
            input.pointer_down(point);
            assert_eq!(input.pointer_up([menu.button.min[0] - 1., point[1]]), None);
            assert!(
                layout.footer().min[1] - 210. >= 160.,
                "a notice still leaves room to see the aircraft"
            );
        }
    }

    const AUDIT_SIZES: [[f32; 2]; 5] = [
        [1280., 800.],
        [1600., 1000.],
        [390., 844.],
        [844., 390.],
        [320., 480.],
    ];

    /// Logical layout rectangle to CSS-pixel `[left, top, right, bottom]`.
    fn pixels(layout: &Layout, rect: UiRect) -> [f32; 4] {
        let origin = layout.viewport.pixels().min;
        let left = origin[0] + rect.min[0];
        let top = origin[1] + rect.min[1];
        [left, top, left + rect.size[0], top + rect.size[1]]
    }

    fn clear_of_menu_corner(screen: [f32; 2], edges: [f32; 4]) -> bool {
        edges[2] <= screen[0] - MENU_CORNER[0] || edges[1] >= MENU_CORNER[1]
    }

    #[test]
    fn hud_and_cards_keep_the_fullscreen_menu_corner_clear() {
        for screen in AUDIT_SIZES {
            let layout = Layout::new(screen).unwrap();
            let top = layout.top_bar();
            let tips_text = pixels(&layout, top.tips_text);
            // The widest realistic tip total still fits before the chip.
            assert!(fits("$1999", top.tips_text.size[0], 16.), "{screen:?}");
            for rect in [
                top.left,
                top.left_strip,
                top.tips,
                top.tips_text,
                top.chip,
                top.tips_strip,
                layout.rect(top.pips, 5., 54., 22.),
                layout.centered(NAV_TOP, 440., GUIDE_HEIGHT),
                layout.centered(CALLOUT_TOP, CALLOUT[0], CALLOUT[1]),
                layout.footer(),
            ] {
                let edges = pixels(&layout, rect);
                assert!(
                    clear_of_menu_corner(screen, edges),
                    "{screen:?}: {edges:?} enters the menu corner"
                );
                assert!(
                    edges[0] >= 0. && edges[2] <= screen[0],
                    "{screen:?}: {edges:?}"
                );
            }
            // Left and right clusters never overlap each other.
            assert!(pixels(&layout, top.left)[2] + 4. <= pixels(&layout, top.tips)[0]);
            assert!(tips_text[2] <= pixels(&layout, top.chip)[0]);
            let menu = layout.menu();
            let mut rows = vec![
                menu.panel,
                menu.title,
                menu.detail,
                menu.summary,
                menu.button,
            ];
            rows.extend(menu.extra);
            for rect in rows {
                let edges = pixels(&layout, rect);
                assert!(
                    clear_of_menu_corner(screen, edges),
                    "{screen:?}: card {edges:?} enters the menu corner"
                );
            }
        }
    }

    #[test]
    fn navigation_copy_fits_one_line_at_the_narrowest_supported_width() {
        let layout = Layout::new([320., 480.]).unwrap();
        assert!(layout.narrow);
        let nav = Navigation {
            bearing: -1.,
            altitude: -20.,
            distance: 999.,
        };
        let (turn, climb) = nav_copy(&nav, true);
        // The pill draws turn, climb and distance with two separators.
        let line = format!("{turn}  ·  {climb}  ·  999 m");
        assert!(fits(&line, layout.width - 24., BODY), "{line}");
        let wide = Layout::new([1280., 800.]).unwrap();
        let (turn, climb) = nav_copy(&nav, false);
        assert!(fits(
            &format!("{turn}  ·  {climb}  ·  999 m to post"),
            wide.width.min(440.) - 24.,
            BODY
        ));
    }

    #[test]
    fn coach_hints_fade_on_accepted_time_and_stay_learned_after_retry() {
        let mut coach = Coach::default();
        let mut flight = Flight::new();
        coach.update(&flight, 0.);
        assert_eq!(coach.visible(), None, "nothing appears without a tick");
        for _ in 0..30 {
            coach.update(&flight, 1. / 60.);
        }
        assert_eq!(coach.visible().map(|v| v.0), Some(Hint::FirstRing));
        let frozen = coach.visible();
        coach.update(&flight, 0.);
        assert_eq!(coach.visible(), frozen, "pause freezes the fade");
        flight.next = 1;
        for _ in 0..60 {
            coach.update(&flight, 1. / 60.);
        }
        assert_eq!(coach.visible().map(|v| v.0), Some(Hint::Boost));
        flight.boosting = true;
        flight.ledger.bullseyes = 1;
        for _ in 0..60 {
            coach.update(&flight, 1. / 60.);
        }
        assert_eq!(coach.visible(), None);
        // Retrying keeps learned skills; the first-ring hint does not return.
        let retry = Flight::new();
        for _ in 0..60 {
            coach.update(&retry, 1. / 60.);
        }
        assert_eq!(coach.visible(), None);
    }

    #[test]
    fn narrow_binding_rows_keep_whole_remaps_or_explicit_editor_fallback() {
        let mut player = PlayerInput::preset("sky-courier-layout-test", Preset::Flight);
        let mut buffer = [0; 96];
        let mut axis = [[0; 96]; 3];
        player
            .rebind("move_left", vec![Binding::digital(InputCode::Key(0xff55))])
            .unwrap();
        player
            .rebind("move_right", vec![Binding::digital(InputCode::Key(0xff56))])
            .unwrap();
        let hints = movement_hint(&player, ["move_x", "move_left", "move_right"], &mut axis);
        assert!(binding_fits("Turn", hints.0, hints.1, 280.));
        player
            .rebind(
                "boost",
                vec![Binding::digital(InputCode::Key(KeyCode::Backspace as u16))],
            )
            .unwrap();
        let boost = control_hint(&player, "boost", &mut buffer);
        assert_eq!(boost, "Backspace");
        assert!(binding_fits("Boost", boost, "", 280.));
        player.rebind("brake", vec![]).unwrap();
        assert_eq!(control_hint(&player, "brake", &mut buffer), "Unbound");
        assert!(binding_fits("Slow", "Unbound", "", 280.));
        assert!(!binding_fits(
            "Slow",
            "PageUp / PageDown / Backspace",
            "",
            280.
        ));
        assert!(binding_fits("Slow", "Open Controls", "", 280.));
        assert!(!binding_fits("Turn", "A / Left", "D / Right", 180.));
    }

    #[test]
    fn all_menu_and_flight_states_queue_within_existing_ui_budgets_without_ticks() {
        let mut run = FlightRun::new(false);
        let player = PlayerInput::preset("sky-courier-menu-test", Preset::Flight);
        let mut ui = Ui::new();
        let coach = Coach::default();
        // Use real lifecycle/autoplay input for terminal menus, with supplied
        // fixed time. UI construction itself must not advance any state.
        let paint = |run: &FlightRun, ui: &mut Ui| {
            for results in [
                Results::default(),
                Results {
                    previous_best: Some(9999),
                    shift: 120,
                    demo: false,
                },
                Results {
                    previous_best: Some(1),
                    shift: 2,
                    demo: true,
                },
            ] {
                let view = HudView {
                    progress: Progress {
                        best_tips: results.previous_best,
                    },
                    results: &results,
                    coach: &coach,
                    pauses: results.shift,
                };
                for screen in [[320., 600.], [390., 600.], [1280., 560.], [844., 390.]] {
                    let layout = Layout::new(screen).unwrap();
                    ui.begin(layout.viewport);
                    let button = queue(run, ui, &player, &layout, &view).unwrap();
                    assert_eq!(button.is_some(), !run.session.is_playing());
                }
            }
        };
        paint(&run, &mut ui);
        assert_eq!(run.session.tick(), 0);
        assert_eq!(run.flight.remaining, 90.);
        run.session.start();
        paint(&run, &mut ui);
        run.session.set_paused(true);
        paint(&run, &mut ui);
        assert_eq!(run.session.tick(), 0);
        run.session.set_paused(false);
        for _ in 0..12000 {
            if !run.session.is_playing() {
                break;
            }
            run.advance(1. / 120., Input::default(), true);
            if run.session.tick().is_multiple_of(90) {
                paint(&run, &mut ui);
            }
        }
        assert_eq!(run.session.state(), SessionState::Won);
        let tick = run.session.tick();
        let remaining = run.flight.remaining;
        paint(&run, &mut ui);
        assert_eq!(run.session.tick(), tick);
        assert_eq!(run.flight.remaining, remaining);
        run.restart();
        run.advance(0., Input::default(), false);
        for _ in 0..12000 {
            if !run.session.is_playing() {
                break;
            }
            run.advance(
                1. / 120.,
                Input {
                    climb: -1.,
                    boost: true,
                    ..Input::default()
                },
                false,
            );
            paint(&run, &mut ui);
        }
        assert_eq!(run.session.state(), SessionState::Lost);
        assert_eq!(run.flight.hull, 0);
        paint(&run, &mut ui);
    }
}
