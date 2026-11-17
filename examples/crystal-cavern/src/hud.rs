//! Heads-up display, callouts and the Ready / Paused / Won cards. The cave
//! explains itself with pictograms: the game's faceted gem for crystals, the
//! carved I/II plaques for altars, the sun for the way out, a lock for what is
//! not ready yet, key caps for actions and plain numbers for stats. Words are
//! limited to names, three-word callouts, button labels and Wendell's note.
//!
//! Layout contract kept from the browser overlay regression: menus keep a
//! 172 px core at `top = max((h - 172) / 2, 8)` whose first vertex in
//! [`MENU_PANEL`] color precedes the warm stat numbers at `top + 62..116` and
//! the action button at `top + 116..160`. During play the warm color is
//! reserved for locked targets, and the prompt pill keeps its unique
//! [`PROMPT_PANEL`] color. Strings are written into reused buffers or stack
//! rows, so steady-state frames allocate nothing.
use super::juice::{ECHO, Extra, GOLD, Juice, Mark, STONE, VIOLET, heading};
use super::quips::{self, NextGoal, Rank, RunSummary};
use super::{Blocker, Game, Interactable, MINT, Prompt, WARM};
use ferravex_core::session::SessionState;
use ferravex_play::tween::{Ease, Entrance, Motion, RollCounter};
use ferravex_render::icons::{self, Icon, IconStyle};
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Fill, Shadow};
use ferravex_render::text::{self as styled, TextAlign, TextStyle, draw_text_styled};
use ferravex_render::ui::kit::{self, ButtonKind, ButtonMotion, ButtonState, KitTheme};
use std::fmt::Write;

pub(super) const PANEL: f32 = 172.;
pub(super) const BAR: f32 = 48.;
/// Menu card fill; the overlay regression finds the card by this exact color.
pub(super) const MENU_PANEL: Color = Color::new(0.03, 0.08, 0.08, 0.94);
/// Nearby-prompt fill; likewise unique so the regression can find it.
pub(super) const PROMPT_PANEL: Color = Color::new(0.03, 0.08, 0.08, 0.85);
const DIM: Color = Color::new(0.2, 0.3, 0.3, 1.);
const PALE: Color = Color::new(0.86, 0.92, 0.9, 1.);
const MUTED: Color = Color::new(0.62, 0.72, 0.72, 1.);
const DEEP: Color = Color::new(0.02, 0.05, 0.06, 1.);
/// Daylight: the way out, lit once the seal opens.
const SUN: Color = Color::new(1.0, 0.86, 0.5, 1.);

pub(super) fn fade(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0., 1.))
}

/// Body copy: the Modern face with a one-pixel drop shadow.
pub(super) fn body(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color).with_shadow(Color::new(0., 0., 0., 0.6 * color.a))
}
/// Retro punch: the Classic face with a dark outline (logo, ranks, pops).
pub(super) fn display(size: f32, color: Color) -> TextStyle {
    TextStyle::classic(size, color).with_outline(fade(DEEP, color.a))
}
/// Copy size: crisp 16 on roomy screens, 12 where 16 would need extra lines.
pub(super) fn copy_size(w: f32) -> f32 {
    if w >= 480. { 16. } else { 12. }
}

pub(super) fn theme() -> KitTheme {
    KitTheme {
        surface: Color::from_hex(0x10222a),
        raised: Color::from_hex(0x1b3640),
        header: Color::from_hex(0x24464f),
        ink: Color::from_hex(0xe8f2ee),
        muted: Color::from_hex(0x93b0b0),
        // Warm amber action color; deliberately not the notice's WARM.
        accent: Color::from_hex(0xf4b85e),
        accent_ink: Color::from_hex(0x1d1206),
        focus: MINT,
        line: Color::new(1., 1., 1., 0.1),
        ..KitTheme::default()
    }
}

/// Lines and widest line of `text` wrapped to `width` (allocation-free).
fn block(text: &str, width: f32, style: TextStyle) -> (usize, f32) {
    styled::wrap_text(text, width, style).fold((0, 0.), |(n, w), line| {
        (n + 1, f32::max(w, styled::text_width(line, style)))
    })
}

/// Centered wrapped copy with at most `max_lines`; returns the used height.
fn centered(
    text: &str,
    cx: f32,
    baseline: f32,
    width: f32,
    style: TextStyle,
    max_lines: usize,
) -> f32 {
    let style = style.with_align(TextAlign::Center);
    let mut y = baseline;
    let mut lines = 0;
    for line in styled::wrap_text(text, width, style).take(max_lines) {
        draw_text_styled(line, cx, y, style);
        y += style.line_height();
        lines += 1;
    }
    lines as f32 * style.line_height()
}

/// The game's crystal: a faceted gem, filled (with an additive glow) when
/// owned, an empty socket otherwise.
fn gem(cx: f32, cy: f32, r: f32, filled: bool, color: Color) {
    let hw = r * 0.62;
    let (l, rt, t, b) = (
        vec2(cx - hw, cy),
        vec2(cx + hw, cy),
        vec2(cx, cy - r),
        vec2(cx, cy + r),
    );
    if filled {
        shapes::glow_disc(vec2(cx, cy), r * 2.2, fade(color, 0.45));
        draw_triangle(l, rt, t, color);
        draw_triangle(
            l,
            rt,
            b,
            Color::new(color.r * 0.7, color.g * 0.7, color.b * 0.75, color.a),
        );
        draw_triangle(
            vec2(cx - hw * 0.45, cy - r * 0.1),
            vec2(cx, cy - r * 0.1),
            vec2(cx - hw * 0.1, cy - r * 0.7),
            fade(WHITE, 0.75 * color.a),
        );
    } else {
        draw_triangle(l, rt, t, Color::new(0.05, 0.12, 0.12, 0.9 * color.a));
        draw_triangle(l, rt, b, Color::new(0.05, 0.12, 0.12, 0.9 * color.a));
        shapes::polyline(
            &[l, t, rt, b],
            shapes::Stroke::new(1.5, fade(color, 0.55)).closed(true),
        );
    }
}

/// An altar's carved plaque with its numeral: gold and lit once awake.
fn plaque(c: Vec2, scale: f32, numeral: &str, awake: bool, color: Color) {
    let r = Rect::new(
        c.x - 15. * scale,
        c.y - 12. * scale,
        30. * scale,
        24. * scale,
    );
    if awake {
        shapes::glow_disc(c, 24. * scale, fade(color, 0.35));
        shapes::fill_rounded_rect(
            r,
            6. * scale,
            Fill::vertical(r, fade(Color::new(1., 0.9, 0.6, 1.), color.a), color),
        );
    } else {
        shapes::fill_rounded_rect(r, 6. * scale, Color::new(0.07, 0.13, 0.13, 0.95 * color.a));
        shapes::stroke_rounded_rect(r, 6. * scale, 1.5, fade(color, 0.45));
    }
    let style = TextStyle::classic(
        16. * scale.clamp(0.5, 1.5),
        if awake {
            fade(DEEP, color.a)
        } else {
            fade(color, 0.75)
        },
    )
    .with_align(TextAlign::Center);
    draw_text_styled(numeral, c.x, style.centered_baseline(c.y), style);
}

/// A stock pictogram with the HUD's soft shadow.
fn icon(icon: Icon, c: Vec2, size: f32, color: Color) {
    if color.a <= 0. {
        return;
    }
    icons::draw_styled(
        icon,
        c,
        size,
        IconStyle::new(color).shadow(Color::new(0., 0., 0., 0.45 * color.a)),
    );
}

fn draw_mark(mark: Mark, c: Vec2, size: f32, alpha: f32) {
    match mark {
        Mark::Gem(color) => gem(c.x, c.y, size * 0.45, true, fade(color, alpha)),
        Mark::Icon(i, color) => icon(i, c, size, fade(color, alpha)),
    }
}

/// One element of a centred pictogram row (prompts, card stats, callouts).
#[derive(Clone, Copy)]
pub(super) enum Item<'a> {
    /// The game's gem at the row size: filled, or an empty socket.
    Gem(Color, bool),
    /// An altar plaque: numeral, awake.
    Plaque(&'a str, bool, Color),
    Icon(Icon, Color),
    Text(&'a str, TextStyle),
    Key(&'a str),
    /// A key cap the player has already used: quieter, so the target leads.
    SoftKey(&'a str),
    /// `lit` of `total` dots.
    Pips(u32, u32, Color),
    Space(f32),
}

const ITEM_GAP: f32 = 6.;

fn keycap_height(theme: &KitTheme) -> f32 {
    theme.body * 1.5 + 2.
}

fn keycap_width(label: &str, theme: &KitTheme) -> f32 {
    let style = TextStyle::modern(theme.body, theme.ink);
    (styled::text_width(label, style) + kit::SPACE[2] * 2.).max(keycap_height(theme) - 3.)
}

impl Item<'_> {
    fn width(&self, size: f32, theme: &KitTheme) -> f32 {
        match *self {
            Self::Gem(..) => size * 0.62,
            Self::Plaque(..) => size * 1.25,
            Self::Icon(..) => size,
            Self::Text(label, style) => styled::text_width(label, style),
            Self::Key(label) | Self::SoftKey(label) => keycap_width(label, theme),
            Self::Pips(_, total, _) => (total as f32 * 12. - 4.).max(0.),
            Self::Space(gap) => gap,
        }
    }
}

fn row_width(items: &[Item], size: f32, theme: &KitTheme) -> f32 {
    let mut width = 0.;
    for (i, item) in items.iter().enumerate() {
        if i > 0 && !matches!(item, Item::Space(_)) && !matches!(items[i - 1], Item::Space(_)) {
            width += ITEM_GAP;
        }
        width += item.width(size, theme);
    }
    width
}

/// Draw `items` centred on `cx`, vertically centred on `mid`; colors fade
/// with `alpha`. Returns the row width.
fn draw_row(items: &[Item], cx: f32, mid: f32, size: f32, alpha: f32) -> f32 {
    let theme = theme().faded(alpha);
    let total = row_width(items, size, &theme);
    let mut x = cx - total * 0.5;
    for (i, item) in items.iter().enumerate() {
        if i > 0 && !matches!(item, Item::Space(_)) && !matches!(items[i - 1], Item::Space(_)) {
            x += ITEM_GAP;
        }
        let w = item.width(size, &theme);
        let c = vec2(x + w * 0.5, mid);
        match *item {
            Item::Gem(color, filled) => gem(c.x, c.y, size * 0.5, filled, fade(color, alpha)),
            Item::Plaque(numeral, awake, color) => {
                plaque(c, size / 24., numeral, awake, fade(color, alpha))
            }
            Item::Icon(i, color) => icon(i, c, size, fade(color, alpha)),
            Item::Text(label, style) => {
                let style = style.with_color(fade(style.color, alpha));
                draw_text_styled(label, x, style.centered_baseline(mid), style);
            }
            Item::Key(label) => {
                kit::keycap(x, mid - (keycap_height(&theme) - 3.) * 0.5, label, &theme);
            }
            Item::SoftKey(label) => {
                let soft = theme.faded(0.55);
                kit::keycap(x, mid - (keycap_height(&soft) - 3.) * 0.5, label, &soft);
            }
            Item::Pips(lit, total, color) => {
                for p in 0..total {
                    let dot = vec2(x + 4. + p as f32 * 12., mid);
                    if p < lit {
                        shapes::fill_circle(dot, 4., fade(color, alpha));
                    } else {
                        shapes::stroke_circle(dot, 4., 1.5, fade(PALE, 0.4 * alpha));
                    }
                }
            }
            Item::Space(_) => {}
        }
        x += w;
    }
    total
}

fn clock(out: &mut String, seconds: f32) {
    let whole = seconds.max(0.) as u32;
    out.clear();
    let _ = write!(out, "{}:{:02}", whole / 60, whole % 60);
}

/// Results-card time with tenths, e.g. "0:54.6".
pub(super) fn stopwatch(out: &mut String, seconds: f32) {
    let tenths = (seconds.max(0.) * 10.).round() as u32;
    let _ = write!(
        out,
        "{}:{:02}.{}",
        tenths / 600,
        tenths / 10 % 60,
        tenths % 10
    );
}

/// Browser fullscreen pins a 44 px menu button 8 px from the canvas's top-right
/// corner. HUD text, counters and controls stay out of `x > w - 72, y < 60`;
/// only the full-bleed bar background may pass under it.
pub(super) const CORNER: (f32, f32) = (72., 60.);
fn content_right(w: f32) -> f32 {
    w - CORNER.0 - 8.
}

fn bar_style() -> TextStyle {
    body(16., MUTED)
}

/// Horizontal positions of the top bar's items. Optional items are dropped,
/// lowest priority first (clock, hum icon), before any item could reach the
/// reserved menu corner. The hum bars always fit while they matter.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct BarLayout {
    /// Left edge of the gem row and the spacing between gems.
    pub gem_step: f32,
    pub shy: Option<f32>,
    pub altars: f32,
    pub altar_step: f32,
    /// Centre of the daylight (way out) pictogram.
    pub exit: f32,
    pub hum_icon: Option<f32>,
    pub hum_bars: Option<f32>,
    /// Left edge of the clock icon; its time follows.
    pub timer: Option<f32>,
    /// Right edge of the six-part objective strip.
    pub strip_right: f32,
    /// Rightmost edge of any drawn item (icon, meter, time or strip).
    pub right: f32,
}

const HUM_BARS: f32 = 28.;
const CLOCK: f32 = 16.;

pub(super) fn bar_layout(w: f32, shy: bool, humming: bool, timer: &str) -> BarLayout {
    let limit = content_right(w);
    let compact = w < 480.;
    let gem_step = if compact { 20. } else { 24. };
    let altar_step = if compact { 34. } else { 36. };
    let mut x = 16. + 3. * gem_step;
    let shy_x = shy.then(|| {
        let at = x;
        x += 18.;
        at
    });
    x += 8.;
    let altars = x;
    x += 2. * altar_step + 6.;
    let exit = x + 10.;
    x += 20. + 10.;
    let mut layout = BarLayout {
        gem_step,
        shy: shy_x,
        altars,
        altar_step,
        exit,
        strip_right: limit.max(16. + 6.),
        ..Default::default()
    };
    if humming {
        if x + 22. + HUM_BARS <= limit {
            layout.hum_icon = Some(x);
            x += 22.;
        }
        if x + HUM_BARS <= limit {
            layout.hum_bars = Some(x);
            x += HUM_BARS + 8.;
        }
    }
    let timer_w = CLOCK + 6. + styled::text_width(timer, bar_style());
    let timer_x = limit - timer_w;
    if x <= timer_x {
        layout.timer = Some(timer_x);
    }
    layout.right = [
        layout.strip_right,
        layout.timer.map_or(0., |t| t + timer_w),
        layout.hum_bars.map_or(exit + 10., |b| b + HUM_BARS),
    ]
    .into_iter()
    .fold(0., f32::max);
    layout
}

/// 0..1 breathing for the current objective's pictogram; steady when reduced.
fn breathe(juice: &Juice) -> f32 {
    if juice.motion() > 0. {
        0.5 + 0.5 * (juice.time * 3.2).sin()
    } else {
        1.
    }
}

/// The slim top bar: the objective chain as pictograms (crystal sockets, the
/// altar plaques, the daylight) with the current target breathing, the hum
/// meter, the clock, and the steady six-part objective strip.
pub(super) fn top_bar(game: &Game, juice: &Juice, w: f32, scratch: &mut String) {
    // Full-bleed glass; every item stays clear of the fullscreen menu corner.
    let bar = Rect::new(0., 0., w.max(1.), BAR);
    shapes::fill_rounded_rect(
        bar,
        0.,
        Fill::vertical(
            bar,
            Color::new(0.02, 0.05, 0.06, 0.9),
            Color::new(0.03, 0.08, 0.09, 0.62),
        ),
    );
    draw_rectangle(0., BAR - 1., w, 1., fade(MINT, 0.12));
    clock(scratch, game.elapsed);
    let stage = game.stage();
    let layout = bar_layout(w, game.shy_found, game.count() < 3, scratch);
    let pulse = breathe(juice);
    for i in 0..3 {
        let c = vec2(16. + i as f32 * layout.gem_step + 8., 22.);
        if stage <= 2 && !game.collected[i] {
            // Still to find: the empty sockets breathe.
            shapes::glow_disc(c, 16., fade(MINT, 0.12 + 0.18 * pulse));
        }
        gem(c.x, c.y, 10. * juice.icon_scale(i), game.collected[i], MINT);
    }
    if let Some(x) = layout.shy {
        gem(x + 6., 24., 7.5 * juice.icon_scale(5), true, VIOLET);
    }
    for (index, name) in ["I", "II"].into_iter().enumerate() {
        let c = vec2(layout.altars + index as f32 * layout.altar_step + 16., 22.);
        let awake = usize::from(game.switches) > index;
        if stage == 3 + index {
            shapes::glow_disc(c, 26., fade(GOLD, 0.15 + 0.25 * pulse));
        }
        plaque(c, juice.icon_scale(3 + index), name, awake, GOLD);
    }
    // The way out: dim while sealed, then the target once the seal opens.
    let exit = vec2(layout.exit, 22.);
    if stage == 5 {
        shapes::glow_disc(exit, 22., fade(SUN, 0.2 + 0.3 * pulse));
        icon(Icon::Sun, exit, 20., SUN);
    } else {
        icon(Icon::Sun, exit, 18., fade(MUTED, 0.35));
    }
    let whisper = game.hint_level >= 1 && stage <= 2;
    if let Some(x) = layout.hum_icon {
        icon(
            Icon::SoundOn,
            vec2(x + 8., 22.),
            16.,
            fade(if whisper { MINT } else { PALE }, 0.85),
        );
    }
    if let Some(x) = layout.hum_bars {
        // The hum: signal bars that fill as the nearest missing crystal nears.
        let lit = (game.resonance() * 5.).ceil() as usize;
        let boost = 0.25 * juice.ping;
        if whisper {
            // The first whisper says "listen": the meter itself glows.
            shapes::glow_disc(vec2(x + 14., 24.), 26., fade(MINT, 0.2 + 0.2 * pulse));
        }
        for bar in 0..5 {
            let height = 6. + bar as f32 * 4.;
            let r = Rect::new(x + bar as f32 * 6., 32. - height, 4., height);
            if bar < lit {
                let c = Color::new(
                    (MINT.r + boost).min(1.),
                    (MINT.g + boost).min(1.),
                    (MINT.b + boost).min(1.),
                    1.,
                );
                shapes::fill_rounded_rect(r, 1.5, c);
            } else {
                shapes::fill_rounded_rect(r, 1.5, DIM);
            }
        }
        if juice.ping > 0. {
            shapes::glow_disc(vec2(x + 14., 22.), 22., fade(MINT, 0.25 * juice.ping));
        }
    }
    if let Some(x) = layout.timer {
        icon(
            Icon::Clock,
            vec2(x + CLOCK * 0.5, 23.),
            CLOCK,
            fade(MUTED, 0.9),
        );
        let style = bar_style();
        draw_text_styled(scratch, x + CLOCK + 6., style.centered_baseline(23.), style);
    }
    // A steady six-part strip includes walking into daylight, not just opening
    // the seal. It reads accepted graph completion without mutating it.
    let strip = (layout.strip_right - 16.).max(1.);
    for view in game.objectives.iter() {
        let r = Rect::new(
            16. + view.index as f32 * strip / 6.,
            BAR - 6.,
            (strip / 6. - 4.).max(1.),
            3.,
        );
        let done = view.state == ferravex_play::objective::ObjectiveState::Complete;
        shapes::fill_rounded_rect(r, 1.5, if done { MINT } else { DIM });
    }
}

/// Top of the hint marker's glyphs: below the bar and the reserved corner.
pub(super) const MARKER_Y: f32 = BAR + 22.;

/// A heading marker under the bar once the whispers escalate.
pub(super) fn hint_marker(game: &Game, yaw: f32, w: f32, scratch: &mut String) -> bool {
    if game.hint_level < 2 {
        return false;
    }
    let Some(target) = game.hint_target() else {
        return false;
    };
    let rel = heading(game.position, target, yaw);
    let span = (w * 0.5 - 48.).max(16.);
    let x = w * 0.5 + (rel / std::f32::consts::FRAC_PI_2).clamp(-1., 1.) * span;
    // The diamond reaches 9 px above its centre, so it clears the menu corner.
    let y = MARKER_Y;
    shapes::fill_rounded_rect(
        Rect::new(w * 0.5 - span, y - 0.5, span * 2., 1.),
        0.,
        Fill::horizontal(
            Rect::new(w * 0.5 - span, y - 0.5, span * 2., 1.),
            fade(GOLD, 0.),
            fade(GOLD, 0.3),
        ),
    );
    shapes::glow_disc(vec2(x, y), 18., fade(GOLD, 0.4));
    draw_triangle(vec2(x - 7., y), vec2(x + 7., y), vec2(x, y - 9.), GOLD);
    draw_triangle(vec2(x - 7., y), vec2(x + 7., y), vec2(x, y + 9.), GOLD);
    if rel.abs() > std::f32::consts::FRAC_PI_2 {
        // Behind you: an arrow says which way to turn.
        let (arrow, ax) = if rel < 0. {
            (Icon::ArrowLeft, x + 20.)
        } else {
            (Icon::ArrowRight, x - 20.)
        };
        icon(arrow, vec2(ax, y), 16., GOLD);
    }
    scratch.clear();
    let _ = write!(scratch, "{:.0} m", target.distance(game.position));
    let small = body(8., fade(GOLD, 0.9)).with_align(TextAlign::Center);
    let dw = styled::text_width(scratch, small);
    draw_text_styled(
        scratch,
        x.clamp(4. + dw * 0.5, w - dw * 0.5 - 4.),
        y + 24.,
        small,
    );
    true
}

/// Callout block on a soft glass pill: a pictogram and the headline, then the
/// cave's short echo or a pictogram row. Returns its used height.
pub(super) fn callout(juice: &Juice, w: f32, top: f32, max_height: f32) -> f32 {
    let Some(c) = &juice.callout else {
        return 0.;
    };
    let motion = Motion::from_reduced(juice.motion() == 0.);
    let look = Entrance::fade_up(10.).sample(c.age, 0, motion);
    let alpha = c.alpha() * look.alpha;
    let top = top + look.offset[1];
    let width = (w - 48.).max(1.);
    let shouting = !c.head.bytes().any(|b| b.is_ascii_lowercase());
    let head_size = if w >= 520. { 24. } else { 16. };
    let head_style = if shouting {
        display(head_size, fade(c.color, alpha))
    } else {
        body(head_size, fade(c.color, alpha))
    };
    let sub_color = if c.color == ECHO { ECHO } else { PALE };
    let sub_style = body(16., fade(sub_color, alpha * 0.8));
    let head_lh = head_style.line_height();
    let sub_lh = sub_style.line_height();
    if max_height < head_lh {
        return 0.;
    }
    let mark = if c.mark.is_some() { head_size + 8. } else { 0. };
    let head_style = {
        let hw = styled::text_width(&c.head, head_style);
        if hw + mark > width {
            head_style.with_size((head_style.size * (width - mark) / hw).max(8.))
        } else {
            head_style
        }
    };
    let hw = styled::text_width(&c.head, head_style) + mark;
    let room = max_height - head_lh;
    let (row_h, row_w) = match c.extra {
        Extra::None if !c.sub.is_empty() && room >= sub_lh => {
            (sub_lh, styled::text_width(&c.sub, sub_style).min(width))
        }
        Extra::Chain if room >= 26. => (26., row_width(&CHAIN, 18., &theme())),
        Extra::Page(_) if room >= 18. => (18., quips::NOTE_PAGES as f32 * 12.),
        _ => (0., 0.),
    };
    let height = head_lh + row_h;
    let pill_w = hw.max(row_w) + 32.;
    let pill = Rect::new((w - pill_w) * 0.5, top - 6., pill_w, height + 10.);
    shapes::fill_rounded_rect(pill, 14., Color::new(0.02, 0.05, 0.07, 0.42 * alpha));
    let ascent = |s: &TextStyle| {
        s.size
            * if s.face == styled::FontFace::Modern {
                11. / 8.
            } else {
                7. / 8.
            }
    };
    let baseline = top + ascent(&head_style);
    let x = (w - hw) * 0.5;
    if let Some(m) = c.mark {
        draw_mark(
            m,
            vec2(x + head_size * 0.5, top + head_lh * 0.45),
            head_size,
            alpha,
        );
    }
    draw_text_styled(&c.head, x + mark, baseline, head_style);
    let row_mid = top + head_lh + row_h * 0.5;
    match c.extra {
        _ if row_h <= 0. => {}
        Extra::None => {
            centered(&c.sub, w * 0.5, baseline + sub_lh, width, sub_style, 1);
        }
        Extra::Chain => {
            draw_row(&CHAIN, w * 0.5, row_mid, 18., alpha);
        }
        Extra::Page(page) => {
            let lit = page % quips::NOTE_PAGES + 1;
            draw_row(
                &[Item::Pips(lit, quips::NOTE_PAGES, STONE)],
                w * 0.5,
                row_mid,
                18.,
                alpha,
            );
        }
    }
    height
}

/// The expedition at a glance: three crystals, two altars, then daylight.
const CHAIN: [Item<'static>; 9] = [
    Item::Gem(MINT, true),
    Item::Gem(MINT, true),
    Item::Gem(MINT, true),
    Item::Icon(Icon::ArrowRight, MUTED),
    Item::Plaque("I", true, GOLD),
    Item::Plaque("II", true, GOLD),
    Item::Icon(Icon::ArrowRight, MUTED),
    Item::Icon(Icon::Sun, SUN),
    Item::Space(0.),
];

/// Rising arcade pops above the reticle, in the Classic face, led by their
/// pictogram (the gem for a recovered crystal).
pub(super) fn pops(juice: &Juice, w: f32, h: f32) {
    for pop in juice.pops.iter().flatten() {
        let t = pop.age / pop.life;
        let alpha = ((1. - t) / 0.35).clamp(0., 1.);
        let grow = if juice.motion() > 0. {
            0.55 + 0.45 * Ease::BackOut.apply((pop.age / 0.22).min(1.))
        } else {
            1.
        };
        let y = h * 0.5 - 34. - juice.pop_rise(pop);
        let mut size = if pop.size >= 30 { 32. } else { 24. };
        let style = display(size, pop.color);
        let lead = if pop.mark.is_some() { size + 8. } else { 0. };
        let fit = (w - 32. - lead) / styled::text_width(&pop.text, style).max(1.);
        if fit < 1. {
            size = (size * fit).max(8.);
        }
        let size = size * grow;
        let style = display(size, fade(pop.color, alpha)).with_align(TextAlign::Center);
        let tw = styled::text_width(&pop.text, style);
        shapes::glow_disc(
            vec2(w * 0.5, y - size * 0.4),
            size * 2.2,
            fade(pop.color, 0.22 * alpha),
        );
        if let Some(mark) = pop.mark {
            let lead = size + 8.;
            draw_mark(
                mark,
                vec2(w * 0.5 - (tw + lead) * 0.5 + size * 0.5, y - size * 0.45),
                size,
                alpha,
            );
            draw_text_styled(&pop.text, w * 0.5 + lead * 0.5, y, style);
        } else {
            draw_text_styled(&pop.text, w * 0.5, y, style);
        }
    }
}

/// A soft edge glow for accepted milestones; absent under reduced flash.
pub(super) fn edge_glow(juice: &Juice, w: f32, h: f32) {
    if juice.flash <= 0. {
        return;
    }
    let c = fade(juice.flash_color, juice.flash * 0.35);
    let clear = fade(juice.flash_color, 0.);
    let band = 48.;
    let edges = [
        (Rect::new(0., 0., w, band), c, clear, true),
        (Rect::new(0., h - band, w, band), clear, c, true),
        (Rect::new(0., 0., band, h), c, clear, false),
        (Rect::new(w - band, 0., band, h), clear, c, false),
    ];
    for (r, a, b, vertical) in edges {
        let fill = if vertical {
            Fill::vertical(r, a, b)
        } else {
            Fill::horizontal(r, a, b)
        };
        shapes::fill_rounded_rect(r, 0., fill);
    }
}

/// The nearby-object prompt: a compact pill with the interact key cap and the
/// target's pictogram when ready (mint), or a lock and the missing pieces when
/// not (warm). A refused use shakes the pill and flares the lock.
/// After the first accepted use (`learned`) the key cap quiets down.
pub(super) fn prompt(
    top: f32,
    w: f32,
    key: &str,
    prompt: Prompt,
    (note_reads, learned): (u32, bool),
    juice: &Juice,
) {
    let ready = !matches!(prompt, Prompt::Locked(..));
    let color = if ready { MINT } else { WARM };
    let page = note_reads % quips::NOTE_PAGES + 1;
    let gems = |count: u16| {
        [
            Item::Gem(WARM, count > 0),
            Item::Gem(WARM, count > 1),
            Item::Gem(WARM, count > 2),
        ]
    };
    let lock = Item::Icon(Icon::Lock, WARM);
    let key = if learned {
        Item::SoftKey(key)
    } else {
        Item::Key(key)
    };
    let items: [Item; 4] = match prompt {
        Prompt::Use(Interactable::Crystal(_)) => {
            [key, Item::Gem(MINT, true), Item::Space(0.), Item::Space(0.)]
        }
        Prompt::Use(Interactable::Altar(i)) => [
            key,
            Item::Plaque(if i == 0 { "I" } else { "II" }, false, MINT),
            Item::Space(0.),
            Item::Space(0.),
        ],
        Prompt::Use(Interactable::Shy) => [
            key,
            Item::Gem(VIOLET, true),
            Item::Space(0.),
            Item::Space(0.),
        ],
        Prompt::Use(Interactable::Note) => [
            key,
            Item::Icon(Icon::Scroll, MINT),
            Item::Pips(page, quips::NOTE_PAGES, MINT),
            Item::Space(0.),
        ],
        Prompt::Use(Interactable::Gate) | Prompt::Exit => [
            Item::Icon(Icon::ArrowUp, MINT),
            Item::Icon(Icon::Sun, SUN),
            Item::Space(0.),
            Item::Space(0.),
        ],
        Prompt::Locked(_, Blocker::Crystals(count, _)) => {
            let [a, b, c] = gems(count);
            [lock, a, b, c]
        }
        Prompt::Locked(_, Blocker::AltarI) => [
            lock,
            Item::Plaque("I", false, WARM),
            Item::Space(0.),
            Item::Space(0.),
        ],
        Prompt::Locked(_, Blocker::Altars(awake)) => [
            lock,
            Item::Plaque("I", awake > 0, WARM),
            Item::Plaque("II", awake > 1, WARM),
            Item::Space(0.),
        ],
    };
    let theme = theme();
    let size = 24.;
    let width = row_width(&items, size, &theme) + 40.;
    let shake = juice.nope_offset();
    let cx = w * 0.5 + shake;
    let card = Rect::new(cx - width * 0.5, top - 8., width, 52.);
    shapes::drop_shadow(
        card,
        18.,
        Shadow::soft(Color::new(0., 0., 0., 0.35)).blur(16.),
    );
    shapes::fill_rounded_rect(card, 18., PROMPT_PANEL);
    shapes::stroke_rounded_rect(card, 18., 1., fade(color, 0.35));
    let mid = top + 18.;
    if !ready && juice.nope > 0. {
        // The thing that refused flares where it stands.
        let lock_x = cx - width * 0.5 + 20. + size * 0.5;
        shapes::glow_disc(
            vec2(lock_x, mid),
            26.,
            Color::new(1., 0.42, 0.3, 0.5 * juice.nope),
        );
    }
    draw_row(&items, cx, mid, size, 1.);
}

/// Wendell's note, read in passing: a paper slip above the bottom edge.
pub(super) fn note(page: u32, w: f32, h: f32) {
    let text = quips::note(page);
    let style =
        body(copy_size(w), Color::new(0.2, 0.15, 0.1, 1.)).with_shadow(Color::new(0., 0., 0., 0.));
    let width = (w - 32.).min(520.);
    let (lines, widest) = block(text, width - 56., style);
    let lines = lines.min(3);
    let height = lines as f32 * style.line_height() + 20.;
    let slip = Rect::new((w - widest - 56.) * 0.5, h - 150., widest + 56., height);
    shapes::drop_shadow(
        slip,
        6.,
        Shadow::soft(Color::new(0., 0., 0., 0.4)).blur(12.),
    );
    shapes::fill_rounded_rect(
        slip,
        6.,
        Fill::vertical(
            slip,
            Color::new(0.93, 0.89, 0.78, 0.96),
            Color::new(0.84, 0.79, 0.66, 0.96),
        ),
    );
    icon(
        Icon::Scroll,
        vec2(slip.x + 20., slip.y + height * 0.5),
        18.,
        Color::new(0.45, 0.32, 0.2, 1.),
    );
    centered(
        text,
        slip.x + 36. + (slip.w - 44.) * 0.5,
        slip.y + 10. + style.size * 1.125,
        slip.w - 48.,
        style,
        lines,
    );
}

/// Movement/look onboarding: key caps beside pictograms, fading after a few
/// metres of walking.
pub(super) fn onboarding(game: &Game, keys: [&str; 4], w: f32, h: f32) {
    let alpha = (1. - game.walked / 6.).clamp(0., 1.);
    if alpha <= 0. {
        return;
    }
    let move_row = [
        Item::Key(keys[0]),
        Item::Key(keys[1]),
        Item::Key(keys[2]),
        Item::Key(keys[3]),
        Item::Icon(Icon::Person, PALE),
    ];
    let look_row = [
        Item::Key("←"),
        Item::Key("→"),
        Item::Icon(Icon::Eye, PALE),
        Item::Icon(Icon::Pointer, MUTED),
    ];
    let theme = theme();
    let a = row_width(&move_row, 22., &theme);
    let b = row_width(&look_row, 22., &theme);
    let gap = 32.;
    let y = h - 32.;
    if a + gap + b <= w - 32. {
        let left = (w - a - gap - b) * 0.5;
        draw_row(&move_row, left + a * 0.5, y, 22., alpha);
        draw_row(&look_row, left + a + gap + b * 0.5, y, 22., alpha);
    } else if a <= w - 32. {
        draw_row(&move_row, w * 0.5, y, 22., alpha);
    }
}

/// Bottom help rows (toggle with the help binding): key caps and pictograms.
pub(super) fn help(keys: [&str; 4], w: f32, h: f32) {
    let rows: [[Item; 5]; 2] = [
        [
            Item::Key(keys[0]),
            Item::Icon(Icon::Hand, PALE),
            Item::Space(24.),
            Item::Key(keys[1]),
            Item::Icon(Icon::Pause, PALE),
        ],
        [
            Item::Key(keys[2]),
            Item::Icon(Icon::Restart, PALE),
            Item::Space(24.),
            Item::Key(keys[3]),
            Item::Icon(Icon::Question, PALE),
        ],
    ];
    for (i, row) in rows.iter().enumerate() {
        draw_row(row, w * 0.5, h - 96. + i as f32 * 34., 22., 1.);
    }
    draw_row(
        &[Item::Key("F2"), Item::Icon(Icon::Gamepad, PALE)],
        w * 0.5,
        h - 96. + 2. * 34.,
        22.,
        1.,
    );
}

/// How the finish compares with the stored best.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Best {
    /// Autoplay: personal bests are for humans.
    Demo,
    First,
    /// Faster by this many seconds.
    Faster(f32),
    /// Within a twentieth of a second of the stored best.
    Tied(f32),
    /// The stored best and the gap to it.
    Behind(f32, f32),
}

/// What the results card shows, computed once at the finish.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Results {
    pub summary: RunSummary,
    pub previous_best: Option<f32>,
    /// Demo runs are never recorded as personal bests.
    pub recorded: bool,
}

impl Results {
    pub fn rank(&self) -> Rank {
        quips::rank(self.summary)
    }
    /// The personal-best comparison.
    pub fn best(&self) -> Best {
        let time = self.summary.seconds;
        match (self.recorded, self.previous_best) {
            (false, _) => Best::Demo,
            (true, None) => Best::First,
            (true, Some(best)) if time < best - 0.05 => Best::Faster(best - time),
            (true, Some(best)) if time <= best + 0.05 => Best::Tied(best),
            (true, Some(best)) => Best::Behind(best, time - best),
        }
    }
}

/// Inputs to the overlay cards that are not part of the game state.
pub(super) struct Card<'a> {
    pub state: SessionState,
    pub confirm_hint: &'a str,
    pub retry_hint: &'a str,
    pub results: &'a Results,
    pub best: Option<(f32, u64)>,
    pub pauses: u32,
    pub controls_open: bool,
}

/// Per-card presentation state: time since the card opened (render time),
/// the button's eased hover/press and the results time roll-up.
#[derive(Default)]
pub(super) struct CardMotion {
    state: Option<SessionState>,
    time: f32,
    button: ButtonMotion,
    roll: Option<RollCounter>,
    pub text: [String; 6],
}

impl CardMotion {
    pub fn new() -> Self {
        Self {
            text: std::array::from_fn(|_| String::with_capacity(96)),
            ..Self::default()
        }
    }
}

pub(super) fn card_top(h: f32) -> f32 {
    ((h - PANEL) * 0.5).max(8.)
}

/// Ready, Paused and Won cards. Returns true when the action was clicked.
pub(super) fn card(
    card: Card,
    game: &Game,
    juice: &Juice,
    motion_state: &mut CardMotion,
    dt: f32,
    w: f32,
    h: f32,
) -> bool {
    let motion = Motion::from_reduced(juice.motion() == 0.);
    if motion_state.state != Some(card.state) {
        motion_state.state = Some(card.state);
        motion_state.time = 0.;
        motion_state.roll = None;
        motion_state.button.snap(ButtonState::default());
    }
    motion_state.time += dt.clamp(0., 0.1);
    let t = motion_state.time;
    let theme = theme();
    let top = card_top(h);
    let header_room = top - 8. >= 42. && card.state != SessionState::Paused;
    let up = if header_room { 42. } else { 0. };
    let below = top + PANEL;
    let card_w = (w - 24.).clamp(1., 560.);
    let card_x = (w - card_w) * 0.5;
    let [detail, a, b, c, header, stat] = &mut motion_state.text;
    for s in [
        &mut *detail,
        &mut *a,
        &mut *b,
        &mut *c,
        &mut *header,
        &mut *stat,
    ] {
        s.clear();
    }
    let warm = body(16., WARM);
    let (title, title_color, action) = match card.state {
        SessionState::Ready => {
            header.push_str("Wendell went in.");
            ("CRYSTAL CAVERN", MINT, "Begin expedition")
        }
        SessionState::Paused => {
            detail.push_str(quips::pause(card.pauses));
            ("PAUSED", MINT, "Resume")
        }
        _ => {
            header.push_str("Daylight!");
            (card.results.rank().title, MINT, "Explore again")
        }
    };
    // Dim the scene toward the bottom so the card leads.
    let screen = Rect::new(0., 0., w, h);
    shapes::fill_rounded_rect(
        screen,
        0.,
        Fill::vertical(
            screen,
            Color::new(0., 0.02, 0.03, 0.15),
            Color::new(0., 0.02, 0.03, 0.55),
        ),
    );
    // The rows below the card: one pictogram row each, as many as fit.
    const ROW: f32 = 26.;
    let budget = (h - below - 14.).max(0.);
    let fit = ((budget - 12.) / (ROW + 4.)).floor().max(0.) as usize;
    let shown = rows_needed(card.state).min(fit);
    let down = if shown > 0 {
        12. + shown as f32 * (ROW + 4.)
    } else {
        0.
    };
    let look = Entrance::fade_up(14.).sample(t, 0, motion);
    let panel = Rect::new(card_x, top - up + look.offset[1], card_w, PANEL + up + down);
    shapes::drop_shadow(
        panel,
        18.,
        Shadow::soft(Color::new(0., 0., 0., 0.45 * look.alpha))
            .offset(vec2(0., 8.))
            .blur(28.),
    );
    shapes::fill_rounded_rect(panel, 18., MENU_PANEL);
    shapes::stroke_rounded_rect(panel, 18., 1., fade(MINT, 0.16 * look.alpha));
    if up > 0. && !header.is_empty() {
        let item = Entrance::fade_up(8.).sample(t, 1, motion);
        let style = body(copy_size(w), fade(STONE, 0.9 * item.alpha));
        centered(
            header,
            w * 0.5,
            top - 14. + item.offset[1],
            card_w - 32.,
            style,
            1,
        );
    }
    // The logo or rank title in the Classic face, with a breathing glow.
    let mut size = 32.;
    while size > 8. && styled::text_width(title, display(size, MINT)) > card_w - 40. {
        size -= 8.;
    }
    let reveal = Entrance::pop(0.7).sample(t, 1, motion);
    let breathe = if motion.is_reduced() {
        0.
    } else {
        (t * 1.6).sin() * 0.08
    };
    shapes::glow_disc(
        vec2(w * 0.5, top + 32.),
        card_w * 0.35,
        fade(title_color, (0.18 + breathe) * reveal.alpha),
    );
    let title_style = display(size, fade(title_color, reveal.alpha)).with_align(TextAlign::Center);
    if card.state == SessionState::Ready && !motion.is_reduced() {
        // Attract-mode logo: letters ripple gently, staggered left to right.
        let advance = styled::text_width("A", title_style) + size / 8.;
        let total = styled::text_width(title, title_style);
        let left = w * 0.5 - total * 0.5;
        for (i, ch) in title.char_indices() {
            let bob = (t * 2.2 - i as f32 * 0.35).sin() * size / 16.;
            let glyph = &title[i..i + ch.len_utf8()];
            draw_text_styled(
                glyph,
                left + i as f32 * advance,
                top + 45. + bob,
                title_style.with_align(TextAlign::Left),
            );
        }
    } else {
        draw_text_styled(title, w * 0.5, top + 45., title_style);
    }
    if card.state == SessionState::Won {
        // Twinkling crystal stars beside the rank title; static when reduced.
        let tw = styled::text_width(title, title_style);
        for side in [-1., 1.] {
            let cx = w * 0.5 + side * (tw * 0.5 + 20.);
            if cx < card_x + 10. || cx > card_x + card_w - 10. {
                continue;
            }
            let twinkle = if motion.is_reduced() {
                1.
            } else {
                0.75 + 0.25 * (t * 5. + side).sin()
            };
            let r = 9. * twinkle * reveal.alpha;
            shapes::glow_disc(vec2(cx, top + 30.), 20., fade(GOLD, 0.4 * reveal.alpha));
            draw_triangle(
                vec2(cx - r * 0.55, top + 30.),
                vec2(cx + r * 0.55, top + 30.),
                vec2(cx, top + 30. - r),
                GOLD,
            );
            draw_triangle(
                vec2(cx - r * 0.55, top + 30.),
                vec2(cx + r * 0.55, top + 30.),
                vec2(cx, top + 30. + r),
                GOLD,
            );
        }
    }
    // Detail at top + 62..116: warm numbers beside pictograms (Ready and Won),
    // or the echo's one-line joke when paused.
    let item = Entrance::fade_up(8.).sample(t, 2, motion);
    let detail_mid = top + 80.;
    match card.state {
        SessionState::Ready => {
            // Three crystals, two altars, one way out.
            draw_row(
                &[
                    Item::Text("3", warm),
                    Item::Gem(MINT, true),
                    Item::Space(22.),
                    Item::Text("2", warm),
                    Item::Plaque("I", false, GOLD),
                    Item::Space(22.),
                    Item::Text("1", warm),
                    Item::Icon(Icon::Sun, SUN),
                ],
                w * 0.5,
                detail_mid,
                22.,
                1.,
            );
        }
        SessionState::Paused => {
            centered(
                detail,
                w * 0.5,
                detail_mid,
                card_w - 32.,
                body(16., fade(ECHO, item.alpha)),
                2,
            );
        }
        _ => {
            let summary = card.results.summary;
            // The time rolls up in tenths when the card opens; the final value
            // (what the overlay regression reads) is exact.
            let tenths = (summary.seconds.max(0.) * 10.).round() as i64;
            let roll = motion_state.roll.get_or_insert_with(|| {
                let mut r = RollCounter::new(0);
                r.set_target(tenths);
                r
            });
            let shown = roll.advance(dt.clamp(0., 0.1), motion) as f32 / 10.;
            stopwatch(stat, shown);
            let _ = write!(a, "{}", summary.hints);
            let _ = write!(b, "{}/2", u32::from(summary.shy) + u32::from(summary.note));
            draw_row(
                &[
                    Item::Icon(Icon::Clock, PALE),
                    Item::Text(stat, warm),
                    Item::Space(20.),
                    Item::Icon(Icon::Bulb, ECHO),
                    Item::Text(a, warm),
                    Item::Space(20.),
                    Item::Gem(VIOLET, summary.shy || summary.note),
                    Item::Text(b, warm),
                ],
                w * 0.5,
                detail_mid,
                22.,
                1.,
            );
            a.clear();
            b.clear();
        }
    }
    // The action button: kit visuals, click-on-press input like the legacy button.
    let button_w = (w - 32.).clamp(1., 320.);
    let rect = Rect::new((w - button_w) * 0.5, top + 116., button_w, 44.);
    let enabled = !card.controls_open;
    let (mx, my) = mouse_position();
    let hovered = enabled && rect.contains(vec2(mx, my));
    let state = ButtonState {
        hovered,
        pressed: hovered && is_mouse_button_down(MouseButton::Left),
        focused: enabled,
        enabled,
    };
    motion_state.button.update(state, dt.clamp(0., 0.1), motion);
    kit::button(rect, "", ButtonKind::Primary, &motion_state.button, &theme);
    let label = TextStyle::modern(16., theme.accent_ink);
    let cap_w = keycap_width(card.confirm_hint, &theme);
    let label_w = styled::text_width(action, label);
    let with_cap = cap_w + 10. + label_w <= button_w - 16.;
    let total = if with_cap {
        cap_w + 10. + label_w
    } else {
        label_w
    };
    let x = (w - total) * 0.5;
    let cy = rect.y + rect.h * 0.5 + 2. * motion_state.button.press();
    if with_cap {
        kit::keycap(x, cy - 12., card.confirm_hint, &theme);
    }
    draw_text_styled(
        action,
        x + if with_cap { cap_w + 10. } else { 0. },
        label.centered_baseline(cy),
        label,
    );
    // Below-card rows arrive with a staggered fade-up.
    for row in 0..shown {
        let item = Entrance::fade_up(10.).sample(t, 3 + row, motion);
        let mid = below + 6. + row as f32 * (ROW + 4.) + ROW * 0.5 + item.offset[1];
        below_row(&card, game, row, w, card_w, mid, item.alpha, c);
    }
    hovered && is_mouse_button_pressed(MouseButton::Left)
}

/// Rows under each card: Ready has its record (or the rumour); Paused its
/// progress and restart key; Won its quip, best and next goal.
fn rows_needed(state: SessionState) -> usize {
    match state {
        SessionState::Ready => 1,
        SessionState::Paused => 2,
        _ => 3,
    }
}

#[allow(clippy::too_many_arguments)]
fn below_row(
    card: &Card,
    game: &Game,
    row: usize,
    w: f32,
    card_w: f32,
    mid: f32,
    alpha: f32,
    scratch: &mut String,
) {
    let text = |color: Color| body(16., color);
    scratch.clear();
    match (card.state, row) {
        (SessionState::Ready, _) => match card.best {
            Some((best, secrets)) => {
                stopwatch(scratch, best);
                let split = scratch.len();
                let _ = write!(scratch, "{secrets}/2");
                let (time, found) = scratch.split_at(split);
                draw_row(
                    &[
                        Item::Icon(Icon::Crown, GOLD),
                        Item::Text(time, text(GOLD)),
                        Item::Space(20.),
                        Item::Gem(VIOLET, secrets > 0),
                        Item::Text(found, text(VIOLET)),
                    ],
                    w * 0.5,
                    mid,
                    20.,
                    alpha,
                );
            }
            // Rumour: one of the crystals is shy.
            None => {
                draw_row(
                    &[Item::Gem(VIOLET, true), Item::Icon(Icon::Question, VIOLET)],
                    w * 0.5,
                    mid,
                    20.,
                    alpha,
                );
            }
        },
        (SessionState::Paused, 0) => {
            clock(scratch, game.elapsed);
            let count = game.count();
            let awake = game.switches;
            draw_row(
                &[
                    Item::Gem(MINT, count > 0),
                    Item::Gem(MINT, count > 1),
                    Item::Gem(MINT, count > 2),
                    Item::Space(14.),
                    Item::Plaque("I", awake > 0, GOLD),
                    Item::Plaque("II", awake > 1, GOLD),
                    Item::Space(14.),
                    Item::Icon(Icon::Clock, MUTED),
                    Item::Text(scratch, text(PALE)),
                ],
                w * 0.5,
                mid,
                20.,
                alpha,
            );
        }
        (SessionState::Paused, _) => {
            draw_row(
                &[Item::Key(card.retry_hint), Item::Icon(Icon::Restart, MUTED)],
                w * 0.5,
                mid,
                20.,
                alpha,
            );
        }
        (_, 0) => {
            // The rank's one joke line.
            let quip = card.results.rank().quip;
            let style = text(fade(ECHO, 1.));
            let fitted = {
                let tw = styled::text_width(quip, style);
                if tw > card_w - 32. {
                    style.with_size((16. * (card_w - 32.) / tw).max(8.))
                } else {
                    style
                }
            };
            draw_row(&[Item::Text(quip, fitted)], w * 0.5, mid, 20., alpha);
        }
        (_, 1) => match card.results.best() {
            Best::Demo => {
                draw_row(
                    &[
                        Item::Icon(Icon::Gamepad, MUTED),
                        Item::Text("DEMO", text(MUTED)),
                    ],
                    w * 0.5,
                    mid,
                    20.,
                    alpha,
                );
            }
            Best::First => {
                draw_row(
                    &[
                        Item::Icon(Icon::Crown, GOLD),
                        Item::Text("FIRST CLEAR", text(GOLD)),
                    ],
                    w * 0.5,
                    mid,
                    20.,
                    alpha,
                );
            }
            Best::Faster(faster) => {
                let _ = write!(scratch, "-{faster:.1}s");
                draw_row(
                    &[
                        Item::Icon(Icon::Crown, MINT),
                        Item::Text("NEW BEST", text(MINT)),
                        Item::Text(scratch, text(MINT)),
                    ],
                    w * 0.5,
                    mid,
                    20.,
                    alpha,
                );
            }
            Best::Tied(best) => {
                stopwatch(scratch, best);
                draw_row(
                    &[
                        Item::Icon(Icon::Crown, PALE),
                        Item::Text("TIED", text(PALE)),
                        Item::Text(scratch, text(PALE)),
                    ],
                    w * 0.5,
                    mid,
                    20.,
                    alpha,
                );
            }
            Best::Behind(best, gap) => {
                stopwatch(scratch, best);
                let split = scratch.len();
                let _ = write!(scratch, "+{gap:.1}s");
                let (best, gap) = scratch.split_at(split);
                draw_row(
                    &[
                        Item::Icon(Icon::Crown, MUTED),
                        Item::Text(best, text(PALE)),
                        Item::Text(gap, text(Color::new(1., 0.55, 0.42, 1.))),
                    ],
                    w * 0.5,
                    mid,
                    20.,
                    alpha,
                );
            }
        },
        _ => {
            // The next goal, as a target followed by what it takes.
            let aim = Item::Icon(Icon::Target, MUTED);
            let goal: [Item; 3] = match quips::next_goal(card.results.summary) {
                NextGoal::Steve => [
                    aim,
                    Item::Gem(VIOLET, true),
                    Item::Icon(Icon::Question, VIOLET),
                ],
                NextGoal::LookBehind => {
                    [aim, Item::Gem(VIOLET, true), Item::Icon(Icon::Undo, VIOLET)]
                }
                NextGoal::Note => [
                    aim,
                    Item::Icon(Icon::Scroll, STONE),
                    Item::Icon(Icon::Question, STONE),
                ],
                NextGoal::NoHints => [
                    aim,
                    Item::Icon(Icon::Bulb, ECHO),
                    Item::Text("0", text(ECHO)),
                ],
                NextGoal::Faster => [
                    aim,
                    Item::Icon(Icon::Clock, GOLD),
                    Item::Icon(Icon::Bolt, GOLD),
                ],
            };
            draw_row(&goal, w * 0.5, mid, 20., alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_compares_against_the_previous_record() {
        let summary = RunSummary {
            seconds: 60.,
            ..Default::default()
        };
        let best = |previous_best, recorded| {
            Results {
                summary,
                previous_best,
                recorded,
            }
            .best()
        };
        assert_eq!(best(None, true), Best::First);
        assert_eq!(best(Some(65.), true), Best::Faster(5.));
        assert_eq!(best(Some(60.), true), Best::Tied(60.));
        assert_eq!(best(Some(50.), true), Best::Behind(50., 10.));
        assert_eq!(best(Some(50.), false), Best::Demo);
    }

    #[test]
    fn clock_reads_minutes_and_seconds() {
        let mut out = String::new();
        clock(&mut out, 0.);
        assert_eq!(out, "0:00");
        clock(&mut out, 67.9);
        assert_eq!(out, "1:07");
        clock(&mut out, -3.);
        assert_eq!(out, "0:00");
        out.clear();
        stopwatch(&mut out, 54.66);
        assert_eq!(out, "0:54.7");
        out.clear();
        stopwatch(&mut out, 59.96);
        assert_eq!(out, "1:00.0");
    }

    #[test]
    fn top_bar_items_keep_the_fullscreen_menu_corner_clear() {
        const SIZES: [(f32, f32); 5] = [
            (1280., 800.),
            (1600., 1000.),
            (390., 844.),
            (844., 390.),
            (320., 480.),
        ];
        for (w, _) in SIZES {
            for shy in [false, true] {
                for humming in [false, true] {
                    let layout = bar_layout(w, shy, humming, "12:34");
                    assert!(
                        layout.right <= w - CORNER.0,
                        "{w}: right edge {} enters the menu corner",
                        layout.right
                    );
                    // The crystals, altars, daylight and (while humming) the
                    // meter always fit.
                    assert!(layout.altars + 2. * layout.altar_step <= w - CORNER.0);
                    assert!(layout.exit + 10. <= w - CORNER.0);
                    assert_eq!(layout.hum_bars.is_some(), humming);
                }
            }
        }
        let wide = bar_layout(1280., false, true, "0:42");
        assert!(wide.timer.is_some() && wide.hum_icon.is_some());
        assert_eq!(wide.strip_right, 1280. - CORNER.0 - 8.);
        // Marker glyphs (diamond reaches 9 px up) start below the corner.
        assert!(MARKER_Y - 9. >= CORNER.1);
    }

    #[test]
    fn pictogram_rows_fit_the_narrowest_overlay() {
        // Card stats and the widest locked prompt fit a 320 px canvas whole,
        // so every size paints the same glyphs.
        let theme = theme();
        let warm = body(16., WARM);
        let won = [
            Item::Icon(Icon::Clock, PALE),
            Item::Text("12:34.5", warm),
            Item::Space(20.),
            Item::Icon(Icon::Bulb, ECHO),
            Item::Text("12", warm),
            Item::Space(20.),
            Item::Gem(VIOLET, true),
            Item::Text("2/2", warm),
        ];
        assert!(row_width(&won, 22., &theme) <= 320. - 48.);
        let locked = [
            Item::Icon(Icon::Lock, WARM),
            Item::Gem(WARM, false),
            Item::Gem(WARM, false),
            Item::Gem(WARM, false),
        ];
        assert!(row_width(&locked, 24., &theme) + 40. <= 320. - 32.);
        assert!(row_width(&CHAIN, 18., &theme) + 32. <= 320. - 16.);
    }
}
