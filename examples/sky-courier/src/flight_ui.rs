//! Pixel-sized flight HUD that reads at a glance; inactive menus replace it.
//!
//! Hierarchy while flying: clock, hull hearts and the route strip top-left;
//! tips and the streak top-right; the heading widget (a dial pointing at the
//! target, its distance and climb chevrons); an approach prompt on the
//! delivery leg; one transient callout; one fading coach prompt and the gauge
//! footer (speed dial, altimeter, climb arrow and the bolt meter). Stats are
//! icons plus numbers, progress is pips and state is colour and motion; the
//! only words are short callouts and the action next to a key cap.
//!
//! Menus are compact cards of icon stats around one button: the title, two
//! detail rows, a two-row summary block, the button and three footnote rows
//! keep a fixed geometry so pointer targets match paint.
//!
//! [`Ui`] owns layout, pointer/keyboard focus and activation; everything
//! visible is painted with the modern UI kit and [`icons`]. Painting allocates
//! nothing: formatted copy goes through a fixed [`Line`] buffer.
use super::presentation::{GOLD, MINT};
use super::*;
use ferravex_play::tween::Motion;
use ferravex_render::icons::{self, Icon, IconStyle};
use ferravex_render::juice;
use ferravex_render::shapes::{self, Cap, Fill, Join, Shadow, Stroke};
use ferravex_render::text::{TextAlign, TextStyle as Type, draw_text_styled, text_width};
use ferravex_render::ui::kit::{self, ButtonKind, ButtonMotion, KitTheme};
use std::f32::consts::{FRAC_PI_2, PI};
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
/// Unlit hull heart: visible on dark chips without reading as damage.
const HOLLOW: Color = Color::new(1.0, 1.0, 1.0, 0.24);
/// Gauge needles: close to cream, but never the exact HUD copy ink.
const NEEDLE: Color = Color::new(0.98, 0.95, 0.86, 1.0);
/// Airmail stripe blue for the card header.
const POST_BLUE: Color = Color::new(0.33, 0.58, 0.78, 1.0);
const SHADOW: Color = Color::new(0.0, 0.0, 0.0, 0.32);
const ICON_SHADOW: Color = Color::new(0.02, 0.06, 0.07, 0.45);
/// Modern body size: one font pixel is two screen pixels.
const BODY: f32 = 16.;
const TITLE: f32 = 24.;
/// Baseline-to-baseline distance for stacked body rows.
const ROW: f32 = 22.;
/// Footnote rows under the card button: quote, record and next goal.
const EXTRA_ROWS: usize = 3;
const EXTRA_ROW: f32 = ROW;
const TOP_HEIGHT: f32 = 40.;
const NAV_TOP: f32 = 52.;
const NAV_HEIGHT: f32 = 36.;
/// The approach prompt sits this far below the heading widget.
const PROMPT_GAP: f32 = 6.;
/// One prompt row fits a key cap with a little air.
const PROMPT_ROW: f32 = 30.;
/// Bottom of the tallest (two-row) approach prompt.
#[cfg(test)]
const PROMPT_BOTTOM: f32 = NAV_TOP + NAV_HEIGHT + PROMPT_GAP + 2. * PROMPT_ROW + 8.;
const CALLOUT_TOP: f32 = 146.;
const CALLOUT: [f32; 2] = [420., 64.];
const COACH_HEIGHT: f32 = 36.;
/// Icon size for widget glyphs (parcel, hint icons).
const ICON: f32 = 16.;
/// Icon size inside centred card rows and callout chips.
const ROW_ICON: f32 = 18.;
/// Key caps (see `kit::keycap`): the cap plus its lip.
const CAP_HEIGHT: f32 = 26.;
const CAP_GAP: f32 = 4.;
/// Heading dial radius and the climb chevron column width.
const DIAL: f32 = 13.;
const CHEVRON: f32 = 14.;
/// Label beside the slow-flight key caps on the delivery approach.
const SLOW: &str = "Slow";
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

/// Top-row badges: clock and hull hearts over the route strip on the left,
/// tips and the streak over the multiplier pips on the right.
struct TopBar {
    left: UiRect,
    /// Five gate segments; the parcel marker follows them.
    left_strip: UiRect,
    /// The parcel marker after the strip.
    parcel: UiRect,
    /// Size of the clock, heart and coin icons.
    icon: f32,
    /// Countdown digits after the clock icon.
    clock: UiRect,
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
        let (left, icon, pips, tips, chip) = if self.narrow {
            (120., 13., 70., 112., 30.)
        } else {
            (160., 16., 88., 150., 40.)
        };
        let right = self.top_right;
        let tips_x = right - tips;
        let pad = if self.narrow { 9. } else { 12. };
        TopBar {
            left: self.rect(0., 0., left, TOP_HEIGHT),
            left_strip: self.rect(pad, 31., left - pad * 2. - 16., 3.),
            parcel: self.rect(left - pad - 12., 26.5, 12., 12.),
            icon,
            clock: self.rect(pad + icon + 4., 5., pips - pad - icon - 8., 22.),
            pips,
            tips: self.rect(tips_x, 0., tips, TOP_HEIGHT),
            tips_text: self.rect(
                tips_x + pad + icon + 4.,
                6.,
                tips - pad - icon - chip - 14.,
                20.,
            ),
            chip: self.rect(right - chip - 6., 6., chip, 20.),
            tips_strip: self.rect(tips_x + 12., 31., tips - 24., 3.),
        }
    }

    fn rect(&self, x: f32, y: f32, w: f32, h: f32) -> UiRect {
        ui_rect(self.x + x, y, w.max(0.), h.max(0.))
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
    /// One row of the two-row summary block.
    fn summary_row(&self, index: usize) -> UiRect {
        ui_rect(
            self.summary.min[0],
            self.summary.min[1] + index as f32 * ROW,
            self.summary.size[0],
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

/// Presentation-only HUD emphasis owned by the presentation: `tips`, `hull`
/// and `gate` are 1 right after an accepted event and fade on accepted time,
/// so pause freezes them. `clock` drives idle pulses (frozen in pause and
/// zero under reduced motion); `motion` and `flash` mirror the live policy.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct HudFx {
    pub tips: f32,
    pub hull: f32,
    pub gate: f32,
    pub clock: f32,
    pub motion: f32,
    pub flash: f32,
}

impl HudFx {
    /// A 0–1 attention pulse at `hz`; steady at 1 when motion is reduced.
    fn beat(&self, hz: f32) -> f32 {
        if self.motion > 0. {
            juice::pulse(self.clock, hz)
        } else {
            1.
        }
    }
    /// Extra scale for a pop that starts at `1 + amount` and settles; none
    /// when motion is reduced.
    fn pop(&self, emphasis: f32, amount: f32) -> f32 {
        1. + amount * emphasis.clamp(0., 1.) * self.motion.min(1.)
    }
    /// Alpha for a flash; none when flashes are reduced.
    fn glow(&self, emphasis: f32) -> f32 {
        emphasis.clamp(0., 1.) * self.flash.min(1.)
    }
}

/// Onboarding prompt shown above the footer until the player has done it once.
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
    pub fx: HudFx,
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

/// A two-tone pictogram with a soft shadow so it reads over sky and sea.
fn icon(kind: Icon, center: Vec2, size: f32, color: Color) {
    icons::draw_styled(
        kind,
        center,
        size,
        IconStyle::new(color).shadow(faded(ICON_SHADOW, color.a)),
    );
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

/// One element of a centred stat row: icons and numbers instead of labels.
#[derive(Clone, Copy)]
enum Item<'a> {
    Icon(Icon, Color),
    Text(&'a str, Color),
    /// Hull hearts: this many of three lit.
    Hearts(u8),
    /// Dots: `lit` of `total`, lit in the colour.
    Pips(u32, u32, Color),
    /// A shield icon inside a ring that empties as protection runs out.
    Shield(f32),
    Gap(f32),
}

fn item_width(item: &Item) -> f32 {
    match *item {
        Item::Icon(..) => ROW_ICON + 5.,
        Item::Text(text, _) => text_width(text, copy(BODY, CREAM)),
        Item::Hearts(_) => 3. * HEART + 2. * HEART_GAP,
        Item::Pips(_, total, _) => (total as f32 * PIP_STEP - (PIP_STEP - 8.)).max(0.),
        Item::Shield(_) => ROW_ICON + 8.,
        Item::Gap(width) => width,
    }
}

fn row_width(items: &[Item]) -> f32 {
    items.iter().map(item_width).sum()
}

const HEART: f32 = 16.;
const HEART_GAP: f32 = 2.;
const PIP_STEP: f32 = 11.;

/// Three hull hearts from `x`, centred on `cy`: lit coral, spent hollow.
fn hearts(x: f32, cy: f32, size: f32, gap: f32, lit: u8, alpha: f32) {
    for i in 0..3 {
        let color = if i < lit { CORAL } else { HOLLOW };
        icon(
            Icon::Heart,
            vec2(x + size * 0.5 + f32::from(i) * (size + gap), cy),
            size,
            faded(color, alpha),
        );
    }
}

/// `lit` of `total` dots from `x`, centred on `cy`.
fn pips(x: f32, cy: f32, lit: u32, total: u32, color: Color, alpha: f32) {
    for i in 0..total {
        let center = vec2(x + 4. + i as f32 * PIP_STEP, cy);
        if i < lit {
            shapes::fill_circle(center, 4., faded(color, alpha));
        } else {
            shapes::stroke_circle(center, 3.4, 1.5, faded(color, 0.5 * alpha));
        }
    }
}

/// Draws `items` left to right from `x`, vertically centred on `cy`.
fn draw_items(x: f32, cy: f32, items: &[Item], alpha: f32) {
    let mut cursor = x;
    for item in items {
        match *item {
            Item::Icon(kind, color) => icon(
                kind,
                vec2(cursor + ROW_ICON * 0.5, cy),
                ROW_ICON,
                faded(color, alpha),
            ),
            Item::Text(text, color) => {
                let style = copy(BODY, faded(color, alpha));
                draw_text_styled(text, cursor, style.centered_baseline(cy), style);
            }
            Item::Hearts(lit) => hearts(cursor, cy, HEART, HEART_GAP, lit, alpha),
            Item::Pips(lit, total, color) => pips(cursor, cy, lit, total, color, alpha),
            Item::Shield(left) => {
                let center = vec2(cursor + (ROW_ICON + 8.) * 0.5, cy);
                shapes::stroke_circle(center, ROW_ICON * 0.5 + 2., 2., faded(TRACK, alpha));
                shapes::stroke_arc(
                    center,
                    ROW_ICON * 0.5 + 2.,
                    -FRAC_PI_2,
                    left.clamp(0., 1.) * std::f32::consts::TAU,
                    Stroke::new(2., faded(MINT, alpha)).cap(Cap::Round),
                );
                icon(Icon::Shield, center, ROW_ICON - 4., faded(MINT, alpha));
            }
            Item::Gap(_) => {}
        }
        cursor += item_width(item);
    }
}

/// Draws `items` centred in `rect`.
fn centered_items(rect: Rect, items: &[Item]) {
    let width = row_width(items);
    draw_items(
        rect.x + ((rect.w - width) * 0.5).max(0.),
        rect.y + rect.h * 0.5,
        items,
        1.,
    );
}

fn menu_copy(state: SessionState, hull: u8) -> (&'static str, &'static str) {
    match state {
        SessionState::Ready => ("Sky Courier", "Take off"),
        SessionState::Paused => ("Paused", "Resume"),
        SessionState::Won => ("Delivered!", "Fly again"),
        SessionState::Lost if hull == 0 => ("Grounded!", "Retry"),
        SessionState::Lost => ("Time's up!", "Retry"),
        SessionState::Playing => unreachable!(),
    }
}

// Prompts show complete current bindings or an explicit route to the editor.
// Never clip a key name, substitute a default, or make the font smaller to fit
// a remap.
#[cfg(test)]
fn binding_fits(label: &str, first: &str, second: &str, width: f32) -> bool {
    let mut line = Line::new();
    let _ = write!(line, "{label}: {first}");
    if !second.is_empty() {
        let _ = write!(line, " {second}");
    }
    fits(line.as_str(), width, BODY)
}

/// Width of one `kit::keycap` for `label`.
fn cap_width(label: &str) -> f32 {
    (text_width(label, copy(BODY, CREAM)) + 24.).max(23.)
}

/// Splits a formatted binding ("Left Shift / Right Shift") into key caps.
/// `None` for the explicit "Open Controls" route or more than eight keys.
fn split_keys(key: &str) -> Option<([&str; 8], usize)> {
    if key == "Open Controls" {
        return None;
    }
    let mut keys = [""; 8];
    let mut count = 0;
    for part in key.split(" / ") {
        *keys.get_mut(count)? = part;
        count += 1;
    }
    Some((keys, count))
}

/// Total width of `keys` as caps followed by a `tail` of the given width.
fn caps_width(keys: &[&str], tail: f32) -> f32 {
    keys.iter().map(|k| cap_width(k) + CAP_GAP).sum::<f32>() + tail
}

/// How many caps go on the first of at most two rows when `keys` plus a
/// trailing label of width `tail` must each fit `limit`; `None` if even two
/// rows cannot hold them without clipping.
fn flow(keys: &[&str], tail: f32, limit: f32) -> Option<usize> {
    if caps_width(keys, tail) <= limit {
        return Some(keys.len());
    }
    let mut first = 0;
    let mut used = 0.;
    for key in keys {
        let width = cap_width(key) + CAP_GAP;
        if used + width > limit {
            break;
        }
        used += width;
        first += 1;
    }
    (first > 0 && caps_width(&keys[first..], tail) <= limit).then_some(first)
}

/// Draws a row of caps from `x` with the cap tops at `y`; returns the end x.
fn draw_caps(x: f32, y: f32, keys: &[&str], theme: &KitTheme) -> f32 {
    let mut cursor = x;
    for key in keys {
        cursor += kit::keycap(cursor, y, key, theme).w + CAP_GAP;
    }
    cursor
}

/// The heading dial: an arrow toward the target relative to the nose (up is
/// straight ahead, down is behind) inside a tinted ring.
fn dial(center: Vec2, radius: f32, bearing: f32, color: Color) {
    shapes::fill_circle(center, radius, Color::new(0., 0., 0., 0.25));
    shapes::stroke_circle(center, radius - 0.75, 1.5, faded(color, 0.55));
    let direction = vec2(bearing.sin(), -bearing.cos());
    let side = vec2(-direction.y, direction.x);
    let tip = center + direction * (radius - 2.5);
    let base = center + direction * (radius * 0.08);
    shapes::fill_triangle(
        tip,
        base + side * radius * 0.5,
        base - side * radius * 0.5,
        color,
    );
    shapes::fill_capsule(
        center - direction * (radius * 0.55),
        base,
        radius * 0.14,
        color,
    );
}

/// Climb guidance toward the target's altitude (see [`Navigation::climb`]):
/// a vertical arrow with one to three heads (more for a bigger gap), or a
/// mint level bar when holding altitude is right.
fn chevrons(center: Vec2, nav: &Navigation, alpha: f32) {
    let altitude = nav.altitude;
    if nav.climb() == "HOLD ALTITUDE" {
        for dy in [-2.5, 2.5] {
            shapes::fill_capsule(
                center + vec2(-5., dy),
                center + vec2(5., dy),
                1.25,
                faded(MINT, alpha),
            );
        }
        return;
    }
    let heads = 1 + usize::from(altitude.abs() > 10.) + usize::from(altitude.abs() > 25.);
    // `up` is the screen direction of travel: -1 climbs, +1 descends.
    let up = if nav.climb() == "CLIMB" { -1. } else { 1. };
    let color = faded(AMBER, alpha);
    let lead = center + vec2(0., up * 8.);
    shapes::fill_capsule(center - vec2(0., up * 8.), lead, 1.2, color);
    for i in 0..heads {
        let tip = lead - vec2(0., up * 4.5 * i as f32);
        shapes::polyline(
            &[tip + vec2(-4.5, -up * 4.5), tip, tip + vec2(4.5, -up * 4.5)],
            Stroke::new(2.4, color).cap(Cap::Round).join(Join::Round),
        );
    }
}

/// Clock and hull hearts over the route strip (five gates, then the parcel).
fn clock_badge(layout: &Layout, top: &TopBar, game: &Flight, fx: &HudFx) {
    let s = |rect: UiRect| layout.screen(rect);
    let badge = s(top.left);
    surface(badge, 12., 1.);
    let seconds = game.countdown();
    let urgent = if seconds <= 10 {
        CORAL
    } else if seconds <= 15 {
        AMBER
    } else {
        CREAM
    };
    let clock = s(top.clock);
    let cy = clock.y + clock.h * 0.5;
    // The clock ticks larger every second of the final ten.
    let tick = if seconds <= 10 {
        fx.pop(1. - game.remaining.fract(), 0.25)
    } else {
        1.
    };
    icon(
        Icon::Clock,
        vec2(clock.x - 4. - top.icon * 0.5, cy),
        top.icon * tick,
        urgent,
    );
    let digits = Line::format(format_args!("{}:{:02}", seconds / 60, seconds % 60));
    text_in(clock, digits.as_str(), copy(BODY, urgent), TextAlign::Left);
    let hull = s(layout.rect(top.pips, 5., 3. * top.icon + 6., 22.));
    hearts(hull.x, cy, top.icon, 3., game.hull, 1.);
    // The heart that just broke bursts: a coral ring expands and fades.
    if fx.hull > 0. && game.hull < 3 {
        let at = vec2(
            hull.x + top.icon * 0.5 + f32::from(game.hull) * (top.icon + 3.),
            cy,
        );
        let spread = if fx.motion > 0. { 1. - fx.hull } else { 0.5 };
        shapes::stroke_circle(
            at,
            top.icon * (0.45 + 0.6 * spread),
            2.,
            faded(CORAL, fx.hull),
        );
        let flash = fx.glow(fx.hull);
        if flash > 0. {
            shapes::fill_circle(at, top.icon * 0.55, faded(CREAM, 0.35 * flash));
        }
    }
    let strip = s(top.left_strip);
    segments(strip, GATES, 3., |i| {
        if i < game.next {
            faded(CREAM, 0.9)
        } else if i == game.next {
            AMBER
        } else {
            TRACK
        }
    });
    // A just-cleared segment flashes gold.
    let glow = fx.glow(fx.gate);
    if glow > 0. && (1..=GATES).contains(&game.next) {
        let width = (strip.w - 3. * (GATES - 1) as f32) / GATES as f32;
        let x = strip.x + (game.next - 1) as f32 * (width + 3.);
        shapes::fill_rounded_rect(
            Rect::new(x - 1., strip.y - 1., width + 2., strip.h + 2.),
            2.5,
            faded(GOLD, glow),
        );
    }
    let parcel = s(top.parcel).center();
    let parcel_color = if game.next == GATES {
        faded(AMBER, 0.65 + 0.35 * fx.beat(1.5))
    } else if game.next > GATES {
        CREAM
    } else {
        HOLLOW
    };
    icon(Icon::Crate, parcel, 12., parcel_color);
}

/// Tips and the streak multiplier, kept clear of the browser's menu corner.
fn tips_badge(layout: &Layout, top: &TopBar, ledger: &Ledger, fx: &HudFx) {
    let s = |rect: UiRect| layout.screen(rect);
    let badge = s(top.tips);
    surface(badge, 12., 1.);
    let glow = fx.glow(fx.tips);
    if glow > 0. {
        shapes::fill_rounded_rect(badge, 12., faded(GOLD, 0.22 * glow));
    }
    let text = s(top.tips_text);
    let cy = text.y + text.h * 0.5;
    icon(
        Icon::Coin,
        vec2(text.x - 4. - top.icon * 0.5, cy),
        top.icon * fx.pop(fx.tips, 0.35),
        GOLD,
    );
    let tips = Line::format(format_args!("{}", ledger.tips));
    text_in(text, tips.as_str(), copy(BODY, GOLD), TextAlign::Left);
    let multiplier = ledger.multiplier();
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
}

/// Heading widget: dial, parcel marker on the delivery leg, distance and
/// climb chevrons. Returns its bottom in layout coordinates.
fn heading(layout: &Layout, game: &Flight, nav: &Navigation, fx: &HudFx) -> f32 {
    let delivery = game.next == GATES;
    let behind = nav.bearing.abs() > FRAC_PI_2;
    let distance = Line::format(format_args!("{:.0} m", nav.distance));
    let style = copy(BODY, CREAM);
    let digits = text_width(distance.as_str(), style);
    let parcel = if delivery { ICON + 6. } else { 0. };
    let content = DIAL * 2. + 10. + parcel + digits + 12. + CHEVRON;
    let rect = layout.centered(NAV_TOP, content + 28., NAV_HEIGHT);
    let pill = layout.screen(rect);
    surface(pill, NAV_HEIGHT * 0.5, 1.);
    let color = if behind {
        shapes::stroke_rounded_rect(
            pill,
            NAV_HEIGHT * 0.5,
            1.5,
            faded(CORAL, 0.35 + 0.5 * fx.beat(2.)),
        );
        CORAL
    } else if nav.turn() == "ON COURSE" {
        MINT
    } else {
        AMBER
    };
    let cy = pill.y + NAV_HEIGHT * 0.5;
    let mut x = pill.x + (pill.w - content).max(0.) * 0.5;
    dial(vec2(x + DIAL, cy), DIAL, nav.bearing, color);
    x += DIAL * 2. + 10.;
    if delivery {
        icon(Icon::Crate, vec2(x + ICON * 0.5, cy), ICON, AMBER);
        x += parcel;
    }
    draw_text_styled(distance.as_str(), x, style.centered_baseline(cy), style);
    x += digits + 12.;
    chevrons(vec2(x + CHEVRON * 0.5, cy), nav, 1.);
    rect.min[1] + rect.size[1]
}

/// Delivery approach prompt under the heading widget: the slow-flight key
/// caps while too fast, a mint "ready to drop" check once slow enough.
/// Returns its bottom in layout coordinates.
fn approach(layout: &Layout, player: &PlayerInput, game: &Flight, fx: &HudFx, top: f32) -> f32 {
    if game.motor.speed <= DELIVERY_SPEED {
        let rect = layout.centered(top, 116., PROMPT_ROW + 4.);
        let pill = layout.screen(rect);
        surface(pill, pill.h * 0.5, 1.);
        shapes::stroke_rounded_rect(
            pill,
            pill.h * 0.5,
            1.5,
            faded(MINT, 0.45 + 0.35 * fx.beat(1.2)),
        );
        let cy = pill.y + pill.h * 0.5;
        icons::badge(Icon::Check, vec2(pill.x + 26., cy), 24., MINT, INK);
        let drop = if fx.motion > 0. {
            juice::bob(fx.clock, 1.4, 2.)
        } else {
            0.
        };
        icon(Icon::ArrowDown, vec2(pill.x + 56., cy + drop), 16., MINT);
        icon(Icon::Crate, vec2(pill.x + 86., cy), 20., AMBER);
        return rect.min[1] + rect.size[1];
    }
    let mut buffer = [0; 96];
    let key = control_hint(player, "brake", &mut buffer);
    let label = copy(BODY, CORAL);
    let tail = 6. + text_width(SLOW, label);
    let limit = layout.width - 24. - 28.;
    let caps = split_keys(key).and_then(|(keys, count)| {
        flow(&keys[..count], tail, limit).map(|first| (keys, count, first))
    });
    let urgent = faded(CORAL, 0.4 + 0.45 * fx.beat(2.5));
    let theme = theme();
    let Some((keys, count, first)) = caps else {
        // "Open Controls" (or an unflowable profile): spell it out, wrapped
        // onto a second row rather than clipped.
        let content = text_width(key, copy(BODY, CREAM)) + 10. + tail;
        let rows = if content <= limit { 1 } else { 2 };
        let rect = layout.centered(
            top,
            (if rows == 1 { content } else { limit }) + 28.,
            rows as f32 * PROMPT_ROW + 4.,
        );
        let pill = layout.screen(rect);
        surface(pill, 14., 1.);
        shapes::stroke_rounded_rect(pill, 14., 1.5, urgent);
        let style = copy(BODY, CREAM);
        let row_cy = |row: usize| pill.y + 2. + PROMPT_ROW * (row as f32 + 0.5);
        if rows == 1 {
            let x = pill.x + (pill.w - content) * 0.5;
            draw_text_styled(SLOW, x, label.centered_baseline(row_cy(0)), label);
            draw_text_styled(key, x + tail, style.centered_baseline(row_cy(0)), style);
        } else {
            let row = |i: usize| Rect::new(pill.x + 14., row_cy(i) - ROW * 0.5, pill.w - 28., ROW);
            text_in(row(0), SLOW, label, TextAlign::Center);
            text_in(row(1), key, style, TextAlign::Center);
        }
        return rect.min[1] + rect.size[1];
    };
    let rows = if first == count { 1 } else { 2 };
    let widths = [
        caps_width(&keys[..first], if rows == 1 { tail } else { 0. }),
        caps_width(&keys[first..count], tail),
    ];
    let rect = layout.centered(
        top,
        widths[0].max(if rows == 2 { widths[1] } else { 0. }) + 28.,
        rows as f32 * PROMPT_ROW + 4.,
    );
    let pill = layout.screen(rect);
    surface(pill, 14., 1.);
    shapes::stroke_rounded_rect(pill, 14., 1.5, urgent);
    for row in 0..rows {
        let (range, width) = if row == 0 {
            (0..first, widths[0])
        } else {
            (first..count, widths[1])
        };
        let x = pill.x + (pill.w - width) * 0.5;
        let y = pill.y + 2. + PROMPT_ROW * row as f32 + (PROMPT_ROW - CAP_HEIGHT) * 0.5;
        let end = draw_caps(x, y, &keys[range], &theme);
        if row + 1 == rows {
            draw_text_styled(
                SLOW,
                end + 2.,
                label.centered_baseline(y + (CAP_HEIGHT - 3.) * 0.5),
                label,
            );
        }
    }
    rect.min[1] + rect.size[1]
}

/// U-turn prompt while the target is behind the aircraft.
fn turn_back(layout: &Layout, fx: &HudFx, top: f32) -> f32 {
    let rect = layout.centered(top, 60., PROMPT_ROW + 4.);
    let pill = layout.screen(rect);
    surface(pill, pill.h * 0.5, 1.);
    shapes::stroke_rounded_rect(
        pill,
        pill.h * 0.5,
        1.5,
        faded(CORAL, 0.4 + 0.45 * fx.beat(2.)),
    );
    icon(
        Icon::Undo,
        vec2(pill.x + pill.w * 0.5, pill.y + pill.h * 0.5),
        22. * fx.pop(fx.beat(2.) * 0.5, 0.2),
        CORAL,
    );
    rect.min[1] + rect.size[1]
}

/// A punchy headline (at most three words) over a row of icon chips.
#[allow(clippy::too_many_arguments)]
fn callout_card(
    layout: &Layout,
    top: f32,
    head: &str,
    color: Color,
    alpha: f32,
    age: f32,
    fx: &HudFx,
    items: &[Item],
) {
    let size = TITLE;
    let head_width = text_width(head, copy(size, color));
    let chips = row_width(items);
    let width = (head_width.max(chips) + 40.).clamp(200., CALLOUT[0]);
    let rect = layout.screen(layout.centered(top, width, CALLOUT[1]));
    // Nearly opaque, so world labels behind never show through the chips.
    shapes::fill_rounded_rect(
        rect,
        14.,
        Fill::vertical(
            rect,
            faded(CARD_TOP, 0.92 * alpha),
            faded(CARD, 0.92 * alpha),
        ),
    );
    // The hairline picks up the event colour (gold tip, mint secret, coral hit).
    shapes::stroke_rounded_rect(rect, 14., 1.5, faded(color, alpha * 0.6));
    // Punch in: the headline lands slightly large, then settles.
    let punch = fx.pop(1. - (age / 0.18).min(1.), 0.3);
    let style = copy(size * punch, faded(color, alpha));
    let center = rect.x + rect.w * 0.5;
    draw_text_styled(
        head,
        center,
        style.centered_baseline(rect.y + 22.),
        style.with_align(TextAlign::Center),
    );
    draw_items(center - chips * 0.5, rect.y + 47., items, alpha);
}

/// One callout at a time: recovery outranks hidden rings, then gates.
fn callout(layout: &Layout, game: &Flight, top: f32, fx: &HudFx) {
    let ledger = &game.ledger;
    if game.recovery > 0. && game.outcome == Outcome::Flying {
        let protection = game.recovery / 2.5;
        callout_card(
            layout,
            top,
            quips::impact(ledger.last_impact, ledger.impacts),
            CORAL,
            1.,
            2.5 - game.recovery,
            fx,
            &[
                Item::Icon(Icon::Clock, CORAL),
                Item::Text("-5", CORAL),
                Item::Gap(14.),
                Item::Shield(protection),
            ],
        );
    } else if game.bonus_notice > 0. {
        let found = ledger.bonus_count();
        let alpha = (game.bonus_notice / 0.3).min(1.);
        let scenic = ledger.scenic();
        let tip = Line::format(format_args!(
            "+{}",
            courier::BONUS_TIP + if scenic { courier::SCENIC_TIP } else { 0 }
        ));
        callout_card(
            layout,
            top,
            if scenic {
                quips::SCENIC
            } else {
                quips::bonus(found)
            },
            MINT,
            alpha,
            2.5 - game.bonus_notice,
            fx,
            &[
                Item::Icon(Icon::Star, MINT),
                Item::Pips(found, courier::BONUS_RINGS.len() as u32, MINT),
                Item::Gap(14.),
                Item::Icon(Icon::Coin, GOLD),
                Item::Text(tip.as_str(), GOLD),
                Item::Gap(14.),
                Item::Icon(Icon::Bolt, AMBER),
            ],
        );
    } else if game.gate_notice > 0. && game.next > 0 && game.next <= GATES {
        let gate = game.next - 1;
        let Some(pass) = ledger.gates[gate] else {
            return;
        };
        let tip = Line::format(format_args!("+{}", pass.tip));
        let streak = Line::format(format_args!("×{}", pass.multiplier));
        let mut items = [Item::Gap(0.); 8];
        let mut n = 0;
        let mut push = |item| {
            items[n] = item;
            n += 1;
        };
        push(Item::Icon(Icon::Coin, GOLD));
        push(Item::Text(tip.as_str(), GOLD));
        if pass.hot {
            push(Item::Gap(10.));
            push(Item::Icon(Icon::Bolt, AMBER));
        }
        if pass.multiplier > 1 {
            push(Item::Gap(10.));
            push(Item::Icon(Icon::Flame, CORAL));
            push(Item::Text(streak.as_str(), CREAM));
        }
        // A streak milestone outranks the line quip for the headline.
        let head = quips::streak(pass.multiplier).unwrap_or(quips::gate(pass.line, gate));
        callout_card(
            layout,
            top,
            head,
            if pass.line == courier::Line::Scrape {
                CREAM
            } else {
                GOLD
            },
            (game.gate_notice / 0.3).min(1.),
            2. - game.gate_notice,
            fx,
            &items[..n],
        );
    }
}

/// The learned-once coach prompt above the footer: key caps and icons with
/// at most a couple of words.
fn coach_prompt(layout: &Layout, player: &PlayerInput, coach: &Coach, bottom: f32, fx: &HudFx) {
    let Some((hint, alpha)) = coach.visible() else {
        return;
    };
    let limit = layout.width - 24. - 36.;
    let mut buffer = [0; 96];
    // The ring target breathes until the first ring is cleared.
    let ring = faded(AMBER, 0.6 + 0.4 * fx.beat(1.4));
    let (keys, count, items): ([&str; 8], usize, [Item; 6]) = match hint {
        Hint::FirstRing => (
            [""; 8],
            0,
            [
                Item::Icon(Icon::Target, ring),
                Item::Gap(2.),
                Item::Text("Fly through", CREAM),
                Item::Gap(0.),
                Item::Gap(0.),
                Item::Gap(0.),
            ],
        ),
        Hint::Centre => (
            [""; 8],
            0,
            [
                Item::Icon(Icon::Target, GOLD),
                Item::Gap(2.),
                Item::Text("Thread centre", CREAM),
                Item::Gap(10.),
                Item::Icon(Icon::Coin, GOLD),
                Item::Text("+10", GOLD),
            ],
        ),
        Hint::Boost => {
            let key = control_hint(player, "boost", &mut buffer);
            let tail = [
                Item::Icon(Icon::Bolt, GOLD),
                Item::Text("Hold", MIST),
                Item::Gap(0.),
                Item::Gap(0.),
                Item::Gap(0.),
                Item::Gap(0.),
            ];
            match split_keys(key) {
                Some((keys, count))
                    if key != "Unbound"
                        && caps_width(&keys[..count], row_width(&tail)) <= limit =>
                {
                    (keys, count, tail)
                }
                // Unbound or overlong: rings refill the boost meter.
                _ => (
                    [""; 8],
                    0,
                    [
                        Item::Icon(Icon::Target, AMBER),
                        Item::Icon(Icon::ArrowRight, MIST),
                        Item::Icon(Icon::Bolt, GOLD),
                        Item::Gap(0.),
                        Item::Gap(0.),
                        Item::Gap(0.),
                    ],
                ),
            }
        }
    };
    let caps = caps_width(&keys[..count], 0.);
    let content = caps + row_width(&items);
    let pill =
        layout.screen(layout.centered(bottom - COACH_HEIGHT - 8., content + 36., COACH_HEIGHT));
    surface(pill, COACH_HEIGHT * 0.5, alpha * 0.95);
    let cy = pill.y + pill.h * 0.5;
    let x = pill.x + (pill.w - content).max(0.) * 0.5;
    let end = draw_caps(
        x,
        cy - (CAP_HEIGHT - 3.) * 0.5,
        &keys[..count],
        &theme().faded(alpha),
    );
    draw_items(end, cy, &items, alpha);
}

/// Speed dial: a 270° arc from 0 to 40 m/s with the delivery band in mint,
/// the boost band in gold and a needle; a mint halo while slowing.
fn speed_dial(center: Vec2, radius: f32, game: &Flight, fx: &HudFx) {
    const START: f32 = PI * 0.75;
    const SWEEP: f32 = PI * 1.5;
    const TOP: f32 = 40.;
    if game.flight_mode() == "SLOW FLIGHT" {
        shapes::fill_circle(center, radius + 4., faded(MINT, 0.18 + 0.12 * fx.beat(1.5)));
    }
    shapes::fill_circle(center, radius, Color::new(0., 0., 0., 0.22));
    let arc = |from: f32, to: f32, width: f32, color: Color| {
        shapes::stroke_arc(
            center,
            radius - width * 0.5,
            START + SWEEP * from / TOP,
            SWEEP * (to - from) / TOP,
            Stroke::new(width, color),
        );
    };
    arc(0., TOP, 3., TRACK);
    let delivery = game.next == GATES;
    arc(
        0.,
        DELIVERY_SPEED,
        if delivery { 4. } else { 3. },
        faded(MINT, if delivery { 0.95 } else { 0.55 }),
    );
    arc(28., TOP, 3., faded(GOLD, 0.7));
    let angle = START + SWEEP * (game.motor.speed / TOP).clamp(0., 1.);
    let tip = center + vec2(angle.cos(), angle.sin()) * (radius - 2.);
    shapes::polyline(&[center, tip], Stroke::new(2.2, NEEDLE).cap(Cap::Round));
    shapes::fill_circle(center, 2.6, NEEDLE);
}

/// Altimeter: a slim vertical gauge to the 110 m ceiling, with a mint tick
/// at the drop altitude on the delivery leg.
fn altimeter(rect: Rect, game: &Flight) {
    const CEILING: f32 = 110.;
    shapes::fill_rounded_rect(rect, rect.w * 0.5, TRACK);
    let level = (game.pos.y / CEILING).clamp(0., 1.);
    let fill = Rect::new(
        rect.x,
        rect.y + rect.h * (1. - level),
        rect.w,
        rect.h * level,
    );
    if fill.h > 0.5 {
        shapes::fill_rounded_rect(fill, rect.w * 0.5, faded(NEEDLE, 0.55));
    }
    // The aircraft's level: a small pointer on the gauge's left edge.
    let y = fill.y;
    shapes::fill_triangle(
        vec2(rect.x - 1., y),
        vec2(rect.x - 6., y - 3.5),
        vec2(rect.x - 6., y + 3.5),
        NEEDLE,
    );
    if game.next == GATES {
        let y = rect.y + rect.h * (1. - ROUTE[GATES].y / CEILING);
        shapes::fill_capsule(
            vec2(rect.x - 3., y),
            vec2(rect.x + rect.w + 3., y),
            1.2,
            MINT,
        );
    }
}

/// A number followed by its unit, sharing a baseline; returns the width.
fn value_unit(x: f32, cy: f32, size: f32, value: &str, unit: &str) -> f32 {
    let number = copy(size, CREAM);
    let small = copy(BODY, MIST);
    let baseline = number.centered_baseline(cy);
    let width = draw_text_styled(value, x, baseline, number).width;
    width + 5. + draw_text_styled(unit, x + width + 5., baseline, small).width
}

/// Telemetry footer: speed dial, altimeter and climb arrow with their
/// numbers, then the bolt meter (boost energy) at the right or underneath.
fn footer_gauges(layout: &Layout, game: &Flight, fx: &HudFx) {
    let panel = layout.screen(layout.footer());
    edged_surface(panel, 14., 1.);
    let narrow = layout.narrow;
    let telemetry = game.motor.telemetry().expect("validated motor");
    let cy = if narrow {
        panel.y + 19.
    } else {
        panel.y + panel.h * 0.5
    };
    let gap = if narrow { 10. } else { 22. };
    let radius = if narrow { 11. } else { 16. };
    // Wide screens give the primary numbers a larger face.
    let size = if narrow { BODY } else { TITLE };
    let mut x = panel.x + 14.;
    speed_dial(vec2(x + radius, cy + radius * 0.12), radius, game, fx);
    x += radius * 2. + 8.;
    let speed = Line::format(format_args!("{:.0}", game.motor.speed * 1.94));
    x += value_unit(x, cy, size, speed.as_str(), "kt") + gap;
    let tall = radius * 2.;
    // Leave room for the altimeter's level pointer on its left edge.
    x += 6.;
    altimeter(Rect::new(x, cy - tall * 0.5, 6., tall), game);
    x += 6. + 10.;
    let altitude = Line::format(format_args!("{:.0}", game.pos.y.max(0.)));
    x += value_unit(x, cy, size, altitude.as_str(), "m") + gap;
    let climb = telemetry.vertical_speed;
    let glyph = if narrow { 14. } else { 18. };
    let at = vec2(x + glyph * 0.5, cy);
    if climb > 0.5 {
        icon(Icon::ArrowUp, at, glyph, MINT);
    } else if climb < -0.5 {
        icon(Icon::ArrowDown, at, glyph, AMBER);
    } else {
        // Level flight: the same mint bars as the heading widget.
        for dy in [-2.5, 2.5] {
            shapes::fill_capsule(
                at + vec2(-glyph * 0.3, dy),
                at + vec2(glyph * 0.3, dy),
                1.25,
                faded(MINT, 0.9),
            );
        }
    }
    x += glyph + 3.;
    let rate = Line::format(format_args!("{:.1}", climb.abs()));
    value_unit(x, cy, size, rate.as_str(), "m/s");
    // The bolt meter: gold while boosting, coral while recharging from empty.
    let mode = game.flight_mode();
    let boosting = mode == "BOOSTING";
    let (bolt, meter) = if narrow {
        (
            vec2(panel.x + 24., panel.y + 43.),
            Rect::new(panel.x + 40., panel.y + 39., panel.w - 54., 8.),
        )
    } else {
        let width = (panel.w * 0.3).clamp(140., 260.);
        (
            vec2(panel.x + panel.w - 14. - width - 16., cy),
            Rect::new(panel.x + panel.w - 14. - width, cy - 5., width, 10.),
        )
    };
    let color = if mode == "RECHARGING" || game.boost_exhausted {
        CORAL
    } else {
        AMBER
    };
    if boosting {
        let glow = fx.glow(0.6 + 0.4 * fx.beat(5.));
        shapes::fill_circle(bolt, 12., faded(GOLD, 0.3 * glow));
    }
    icon(
        Icon::Bolt,
        bolt,
        if boosting { 20. } else { 18. },
        if boosting { GOLD } else { color },
    );
    kit::meter(meter, game.boost_energy, None, color, &theme());
}

fn queue_flight(
    run_state: &FlightRun,
    player: &PlayerInput,
    layout: &Layout,
    view: &HudView,
) -> Result<(), ferravex_play::ui::UiError> {
    let game = &run_state.flight;
    let fx = &view.fx;
    let top = layout.top_bar();
    clock_badge(layout, &top, game, fx);
    tips_badge(layout, &top, &game.ledger, fx);
    let mut callout_top = CALLOUT_TOP;
    if let Some(nav) = game.navigation() {
        let bottom = heading(layout, game, &nav, fx);
        let prompt_top = bottom + PROMPT_GAP;
        let bottom = if nav.bearing.abs() > FRAC_PI_2 {
            turn_back(layout, fx, prompt_top)
        } else if game.next == GATES {
            approach(layout, player, game, fx, prompt_top)
        } else {
            bottom
        };
        callout_top = callout_top.max(bottom + 10.);
    }
    callout(layout, game, callout_top, fx);
    let footer = layout.footer();
    coach_prompt(layout, player, view.coach, footer.min[1], fx);
    footer_gauges(layout, game, fx);
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
    let (title, button) = menu_copy(state, game.hull);
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
    let first = s(menu.detail_first());
    let second = s(menu.detail_second());
    let summary = [s(menu.summary_row(0)), s(menu.summary_row(1))];
    let extra: [Rect; EXTRA_ROWS] = std::array::from_fn(|i| s(menu.extra[i]));
    let seconds = game.countdown();
    let clock = Line::format(format_args!("{}:{:02}", seconds / 60, seconds % 60));
    let tips = Line::format(format_args!("{}", ledger.tips));
    let streak = Line::format(format_args!("×{}", ledger.multiplier()));
    let found = ledger.bonus_count();
    let rings = courier::BONUS_RINGS.len() as u32;
    match state {
        SessionState::Ready => {
            let gates = Line::format(format_args!("×{GATES}"));
            // The route at a glance: five rings, then one parcel.
            centered_items(
                Rect::new(first.x, first.y + ROW * 0.5, first.w, ROW),
                &[
                    Item::Icon(Icon::Target, AMBER),
                    Item::Text(gates.as_str(), CREAM),
                    Item::Gap(12.),
                    Item::Icon(Icon::ArrowRight, MIST),
                    Item::Gap(8.),
                    Item::Icon(Icon::Crate, AMBER),
                    Item::Text("×1", CREAM),
                ],
            );
            // The window and the hull, centred in the summary block.
            centered_items(
                Rect::new(summary[0].x, summary[0].y + ROW * 0.5, summary[0].w, ROW),
                &[
                    Item::Icon(Icon::Clock, AMBER),
                    Item::Text(clock.as_str(), AMBER),
                    Item::Gap(18.),
                    Item::Hearts(3),
                ],
            );
            let best = view
                .progress
                .best_tips
                .map(|best| Line::format(format_args!("{best}")));
            match &best {
                Some(best) => centered_items(
                    extra[0],
                    &[
                        Item::Icon(Icon::Trophy, GOLD),
                        Item::Icon(Icon::Coin, GOLD),
                        Item::Text(best.as_str(), GOLD),
                    ],
                ),
                None => centered_items(
                    extra[0],
                    &[
                        Item::Icon(Icon::Trophy, GOLD),
                        Item::Text("First shift!", GOLD),
                    ],
                ),
            }
            // Three secrets hide off the route: never marked, only teased.
            centered_items(
                extra[1],
                &[
                    Item::Icon(Icon::EyeOff, MINT),
                    Item::Gap(2.),
                    Item::Pips(0, rings, MINT),
                    Item::Gap(4.),
                    Item::Icon(Icon::Star, MINT),
                ],
            );
        }
        SessionState::Paused => {
            text_in(
                first,
                quips::pause(view.pauses),
                copy(BODY, GOLD),
                TextAlign::Center,
            );
            centered_items(
                second,
                &[
                    Item::Icon(Icon::Target, AMBER),
                    Item::Pips(game.next.min(GATES) as u32, GATES as u32, CREAM),
                    Item::Gap(4.),
                    Item::Icon(Icon::Crate, if game.next >= GATES { AMBER } else { HOLLOW }),
                ],
            );
            centered_items(
                summary[0],
                &[
                    Item::Icon(Icon::Clock, AMBER),
                    Item::Text(clock.as_str(), AMBER),
                    Item::Gap(18.),
                    Item::Hearts(game.hull),
                ],
            );
            centered_items(
                summary[1],
                &[
                    Item::Icon(Icon::Coin, GOLD),
                    Item::Text(tips.as_str(), GOLD),
                    Item::Gap(16.),
                    Item::Icon(Icon::Flame, CORAL),
                    Item::Text(streak.as_str(), CREAM),
                ],
            );
            centered_items(
                extra[0],
                &[Item::Icon(Icon::Star, MINT), Item::Pips(found, rings, MINT)],
            );
            let mut buffer = [0; 96];
            let restart = control_hint(player, "restart", &mut buffer);
            let tail = row_width(&[Item::Icon(Icon::Restart, MIST)]);
            match split_keys(restart) {
                Some((keys, count))
                    if restart != "Unbound" && caps_width(&keys[..count], tail) <= extra[1].w =>
                {
                    let width = caps_width(&keys[..count], tail);
                    let x = extra[1].x + (extra[1].w - width) * 0.5;
                    let cy = extra[1].y + extra[1].h * 0.5 + 4.;
                    let end = draw_caps(x, cy - (CAP_HEIGHT - 3.) * 0.5, &keys[..count], &theme());
                    draw_items(end, cy, &[Item::Icon(Icon::Restart, MIST)], 1.);
                }
                _ => centered_items(
                    extra[1],
                    &[
                        Item::Icon(Icon::Restart, MIST),
                        Item::Icon(Icon::Gear, MIST),
                        Item::Text("Controls", MIST),
                    ],
                ),
            }
        }
        SessionState::Won | SessionState::Lost => {
            let delivered = state == SessionState::Won;
            let (rank, next) = quips::rank(ledger, delivered, game.hull, game.next.min(GATES));
            text_in(first, rank, copy(BODY, GOLD), TextAlign::Center);
            if let Some(drop) = ledger.drop {
                centered_items(
                    second,
                    &[
                        Item::Icon(Icon::Crate, AMBER),
                        Item::Gap(2.),
                        Item::Text(drop.grade.name(), MIST),
                    ],
                );
            } else if game.hull == 0 {
                centered_items(second, &[Item::Hearts(0)]);
            } else {
                centered_items(
                    second,
                    &[Item::Icon(Icon::Clock, CORAL), Item::Text("0:00", CORAL)],
                );
            }
            let best = Line::format(format_args!(
                "×{}",
                ledger.best_streak.min(courier::MAX_MULTIPLIER)
            ));
            let time = Line::format(format_args!("{:.1} s", run_state.elapsed));
            let penalty = Line::format(format_args!("-{} s", (3 - game.hull) * 5));
            if delivered {
                centered_items(
                    summary[0],
                    &[
                        Item::Icon(Icon::Coin, GOLD),
                        Item::Text(tips.as_str(), GOLD),
                        Item::Gap(18.),
                        Item::Icon(Icon::Flame, CORAL),
                        Item::Text(best.as_str(), CREAM),
                    ],
                );
                centered_items(
                    summary[1],
                    &[
                        Item::Icon(Icon::Clock, AMBER),
                        Item::Text(time.as_str(), AMBER),
                        Item::Gap(14.),
                        Item::Hearts(game.hull),
                        Item::Gap(14.),
                        Item::Icon(Icon::Star, MINT),
                        Item::Pips(found, rings, MINT),
                    ],
                );
            } else {
                // Unpaid: the tips are struck through.
                centered_items(
                    summary[0],
                    &[
                        Item::Icon(Icon::Coin, HOLLOW),
                        Item::Text(tips.as_str(), MIST),
                        Item::Icon(Icon::Cross, CORAL),
                        Item::Gap(14.),
                        Item::Icon(Icon::Target, AMBER),
                        Item::Pips(game.next.min(GATES) as u32, GATES as u32, CREAM),
                    ],
                );
                if game.hull < 3 {
                    centered_items(
                        summary[1],
                        &[
                            Item::Icon(Icon::Clock, AMBER),
                            Item::Text(penalty.as_str(), AMBER),
                        ],
                    );
                }
            }
            let customer = quips::customer(ledger, delivered, game.hull, view.results.shift);
            let quote = Line::format(format_args!("“{customer}”"));
            text_in(
                extra[0],
                quote.as_str(),
                copy(BODY, MIST),
                TextAlign::Center,
            );
            record_row(extra[1], view.results, delivered, ledger.tips);
            let goal = next.map(|(tips, title)| {
                (
                    Line::format(format_args!("{title} · ${tips}")),
                    Line::format(format_args!("${tips}")),
                )
            });
            match &goal {
                _ if delivered && found > 0 && found < rings => centered_items(
                    extra[2],
                    &[
                        Item::Icon(Icon::Star, MINT),
                        Item::Pips(found, rings, MINT),
                        Item::Gap(4.),
                        Item::Icon(Icon::Search, MINT),
                    ],
                ),
                Some((long, short)) => {
                    let text = if row_width(&[
                        Item::Icon(Icon::Crown, MIST),
                        Item::Text(long.as_str(), MIST),
                    ]) <= extra[2].w
                    {
                        long.as_str()
                    } else {
                        short.as_str()
                    };
                    centered_items(
                        extra[2],
                        &[Item::Icon(Icon::Crown, MIST), Item::Text(text, MIST)],
                    );
                }
                None => centered_items(
                    extra[2],
                    &[Item::Icon(Icon::Crown, GOLD), Item::Text("Top rank!", MINT)],
                ),
            }
        }
        SessionState::Playing => unreachable!(),
    }
    // A hairline separates the action from the card's footnotes.
    let rule = extra[0];
    shapes::fill_rounded_rect(
        Rect::new(rule.x + 24., rule.y - 7., (rule.w - 48.).max(0.), 1.),
        0.,
        LINE,
    );
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

/// Personal-best footnote: a trophy and the standing in a few words.
fn record_row(rect: Rect, results: &Results, delivered: bool, tips: u32) {
    let amount = |value: u32| Line::format(format_args!("${value}"));
    match (results.demo, delivered, results.previous_best) {
        (true, _, _) => centered_items(
            rect,
            &[Item::Icon(Icon::Play, MIST), Item::Text("Demo run", MIST)],
        ),
        (false, false, Some(best)) => {
            let best = amount(best);
            centered_items(
                rect,
                &[
                    Item::Icon(Icon::Trophy, GOLD),
                    Item::Text(best.as_str(), GOLD),
                ],
            );
        }
        (false, false, None) => centered_items(
            rect,
            &[
                Item::Icon(Icon::Trophy, HOLLOW),
                Item::Text("No best yet", MIST),
            ],
        ),
        (false, true, None) => centered_items(
            rect,
            &[
                Item::Icon(Icon::Trophy, MINT),
                Item::Text("First record!", MINT),
            ],
        ),
        (false, true, Some(best)) if tips > best => {
            let gain = Line::format(format_args!("+{}", tips - best));
            centered_items(
                rect,
                &[
                    Item::Icon(Icon::Trophy, MINT),
                    Item::Text("New best!", MINT),
                    Item::Gap(8.),
                    Item::Text(gain.as_str(), MINT),
                ],
            );
        }
        (false, true, Some(best)) if tips == best => {
            let best = amount(best);
            centered_items(
                rect,
                &[
                    Item::Icon(Icon::Trophy, GOLD),
                    Item::Text(best.as_str(), GOLD),
                    Item::Gap(8.),
                    Item::Text("Tied!", GOLD),
                ],
            );
        }
        (false, true, Some(best)) => {
            let shown = amount(best);
            let short = Line::format(format_args!("{} to go", best - tips));
            centered_items(
                rect,
                &[
                    Item::Icon(Icon::Trophy, GOLD),
                    Item::Text(shown.as_str(), GOLD),
                    Item::Gap(10.),
                    Item::Text(short.as_str(), MIST),
                ],
            );
        }
    }
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
            // The widest realistic tip total still fits before the chip, and
            // the longest countdown before the hearts.
            assert!(fits("1999", top.tips_text.size[0], 16.), "{screen:?}");
            assert!(fits("1:30", top.clock.size[0], 16.), "{screen:?}");
            let hull = layout.rect(top.pips, 5., 3. * top.icon + 6., 22.);
            assert!(pixels(&layout, top.clock)[2] <= pixels(&layout, hull)[0]);
            assert!(pixels(&layout, hull)[2] <= pixels(&layout, top.left)[2]);
            assert!(pixels(&layout, top.left_strip)[2] <= pixels(&layout, top.parcel)[0]);
            for rect in [
                top.left,
                top.left_strip,
                top.parcel,
                top.clock,
                top.tips,
                top.tips_text,
                top.chip,
                top.tips_strip,
                hull,
                layout.centered(NAV_TOP, 440., PROMPT_BOTTOM - NAV_TOP),
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
    fn heading_widget_and_callouts_fit_the_narrowest_supported_width() {
        let layout = Layout::new([320., 480.]).unwrap();
        assert!(layout.narrow);
        // Dial, parcel, the longest distance and the chevrons, plus padding.
        let content = DIAL * 2. + 10. + ICON + 6. + text_width("999 m", copy(BODY, CREAM)) + 12.;
        assert!(content + CHEVRON + 28. <= layout.width - 24.);
        // The richest gate callout chips (tip, hot bolt, streak) fit a card.
        let chips = row_width(&[
            Item::Icon(Icon::Coin, GOLD),
            Item::Text("+125", GOLD),
            Item::Gap(10.),
            Item::Icon(Icon::Bolt, AMBER),
            Item::Gap(10.),
            Item::Icon(Icon::Flame, CORAL),
            Item::Text("×5", CREAM),
        ]);
        assert!(chips + 40. <= layout.width, "{chips}");
        // A two-row approach prompt pushes the callout down; it still clears
        // the coach prompt above the footer on the shortest screens.
        for screen in AUDIT_SIZES {
            let layout = Layout::new(screen).unwrap();
            let callout_bottom = CALLOUT_TOP.max(PROMPT_BOTTOM + 10.) + CALLOUT[1];
            let coach_top = layout.footer().min[1] - COACH_HEIGHT - 8.;
            assert!(callout_bottom <= coach_top, "{screen:?}");
        }
    }

    #[test]
    fn approach_prompts_flow_whole_bindings_onto_at_most_two_rows() {
        for screen in [[320., 480.], [390., 844.], [1280., 800.]] {
            let layout = Layout::new(screen).unwrap();
            let limit = layout.width - 24. - 28.;
            let tail = 6. + text_width(SLOW, copy(BODY, CORAL));
            for binding in [
                "Left Shift / Right Shift",
                "Left Shift / Right Shift / B",
                "PageUp / PageDown / Backspace",
                "A / B / C / D / E / F / G / H",
                "Backspace",
                "Unbound",
                "Right side",
            ] {
                let (keys, count) = split_keys(binding).expect("caps");
                let parts: Vec<_> = binding.split(" / ").collect();
                assert_eq!(&keys[..count], &parts[..], "every key keeps its own cap");
                let first = flow(&keys[..count], tail, limit)
                    .unwrap_or_else(|| panic!("{screen:?}: {binding} must flow"));
                assert!(caps_width(&keys[..first], 0.) <= limit);
                assert!(caps_width(&keys[first..count], tail) <= limit);
            }
            // The explicit editor route is spelled out, never shown as a cap.
            assert!(split_keys("Open Controls").is_none());
            assert!(fits("Open Controls", limit - tail - 10., BODY));
        }
        // Default bindings stay on one row from the narrowest width up.
        let narrow = Layout::new([320., 480.]).unwrap();
        let tail = 6. + text_width(SLOW, copy(BODY, CORAL));
        let (keys, count) = split_keys("Left Shift / Right Shift").unwrap();
        assert_eq!(flow(&keys[..count], tail, narrow.width - 52.), Some(2));
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
                // Alternate idle and freshly-pulsed HUD emphasis.
                let fx = if results.shift % 2 == 0 {
                    HudFx {
                        tips: 1.,
                        hull: 0.5,
                        gate: 1.,
                        clock: 0.3,
                        motion: 1.,
                        flash: 1.,
                    }
                } else {
                    HudFx::default()
                };
                let view = HudView {
                    progress: Progress {
                        best_tips: results.previous_best,
                    },
                    results: &results,
                    coach: &coach,
                    pauses: results.shift,
                    fx,
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
