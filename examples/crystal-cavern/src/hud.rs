//! Heads-up display, callouts and the Ready / Paused / Won cards, drawn with
//! the modern UI kit: the mixed-case Modern face for copy, the Classic 5×7 face
//! for the logo, rank titles and arcade pops, anti-aliased rounded shapes,
//! soft shadows and additive glows.
//!
//! Layout contract kept from the browser overlay regression: menus keep a
//! 172 px core at `top = max((h - 172) / 2, 8)` whose first vertex in
//! [`MENU_PANEL`] color precedes the warm detail copy at `top + 62..116` and the
//! action button at `top + 116..160`. During play the warm color is reserved
//! for blockers/notices, and the prompt panel keeps its unique [`PROMPT_PANEL`]
//! color. Every string here is written into reused buffers, so steady-state
//! frames allocate nothing.
use super::juice::{ECHO, GOLD, Juice, STONE, VIOLET, heading};
use super::quips::{self, Rank, RunSummary};
use super::{Game, MINT, WARM};
use ferravex_core::session::SessionState;
use ferravex_play::tween::{Ease, Entrance, Motion, RollCounter};
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

/// A faceted gem icon: filled (with an additive glow) when owned, a socket otherwise.
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
        draw_triangle(l, rt, t, Color::new(0.05, 0.12, 0.12, 0.9));
        draw_triangle(l, rt, b, Color::new(0.05, 0.12, 0.12, 0.9));
        shapes::polyline(
            &[l, t, rt, b],
            shapes::Stroke::new(1.5, fade(color, 0.55)).closed(true),
        );
    }
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
    body(16., PALE)
}
fn caption() -> TextStyle {
    TextStyle::modern(8., fade(PALE, 0.8))
}

/// Horizontal positions of the top bar's items. Optional items are dropped,
/// lowest priority first (objective, clock, HUM label), before any item could
/// reach the reserved menu corner.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct BarLayout {
    pub shy: Option<f32>,
    pub altars: f32,
    pub hum_label: Option<f32>,
    pub hum_bars: Option<f32>,
    pub objective: Option<f32>,
    pub timer: Option<f32>,
    /// Right edge of the six-part objective strip.
    pub strip_right: f32,
    /// Rightmost edge of any drawn item (text, icon, meter or strip).
    pub right: f32,
}

const GEM_STEP: f32 = 24.;
const ALTAR_STEP: f32 = 36.;
const HUM_BARS: f32 = 28.;

pub(super) fn bar_layout(
    w: f32,
    shy: bool,
    humming: bool,
    objective: &str,
    timer: &str,
) -> BarLayout {
    let limit = content_right(w);
    let mut x = 16. + 3. * GEM_STEP;
    let shy_x = shy.then(|| {
        let at = x;
        x += 20.;
        at
    });
    x += 10.;
    let altars = x;
    x += 2. * ALTAR_STEP + 8.;
    let mut layout = BarLayout {
        shy: shy_x,
        altars,
        strip_right: limit.max(16. + 6.),
        ..Default::default()
    };
    if humming {
        let label_w = styled::text_width("HUM", caption()) + 6.;
        if x + label_w + HUM_BARS <= limit {
            layout.hum_label = Some(x);
            x += label_w;
        }
        if x + HUM_BARS <= limit {
            layout.hum_bars = Some(x);
            x += HUM_BARS + 8.;
        }
    }
    let timer_w = styled::text_width(timer, bar_style());
    let timer_x = limit - timer_w;
    let has_timer = x <= timer_x;
    if has_timer {
        layout.timer = Some(timer_x);
    }
    let room = if has_timer { timer_x - 14. } else { limit };
    let objective_w = styled::text_width(objective, bar_style());
    if x + 6. + objective_w <= room {
        layout.objective = Some(x + 6.);
    }
    layout.right = [
        layout.strip_right,
        layout.timer.map_or(0., |t| t + timer_w),
        layout.objective.map_or(0., |o| o + objective_w),
        layout.hum_bars.map_or(x, |b| b + HUM_BARS),
    ]
    .into_iter()
    .fold(0., f32::max);
    layout
}

/// The slim top bar: crystals, altars, the hum meter, the current objective
/// and the clock, over the steady six-part objective strip.
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
    let objective = quips::objective(game.stage());
    let layout = bar_layout(w, game.shy_found, game.count() < 3, objective, scratch);
    for i in 0..3 {
        gem(
            16. + i as f32 * GEM_STEP + 8.,
            22.,
            10. * juice.icon_scale(i),
            game.collected[i],
            MINT,
        );
    }
    if let Some(x) = layout.shy {
        gem(x + 6., 24., 7.5 * juice.icon_scale(5), true, VIOLET);
    }
    for (index, name) in ["I", "II"].into_iter().enumerate() {
        let x = layout.altars + index as f32 * ALTAR_STEP;
        let awake = usize::from(game.switches) > index;
        let s = juice.icon_scale(3 + index);
        let r = Rect::new(x + 16. - 15. * s, 22. - 12. * s, 30. * s, 24. * s);
        if awake {
            shapes::glow_disc(r.center(), 24., fade(GOLD, 0.35));
            shapes::fill_rounded_rect(r, 6., Fill::vertical(r, Color::new(1., 0.9, 0.6, 1.), GOLD));
        } else {
            shapes::fill_rounded_rect(r, 6., Color::new(0.07, 0.13, 0.13, 0.95));
            shapes::stroke_rounded_rect(r, 6., 1.5, fade(GOLD, 0.45));
        }
        let style = TextStyle::classic(16., if awake { DEEP } else { fade(GOLD, 0.75) })
            .with_align(TextAlign::Center);
        draw_text_styled(
            name,
            r.center().x,
            style.centered_baseline(r.center().y),
            style,
        );
    }
    if let Some(x) = layout.hum_label {
        draw_text_styled("HUM", x, 26., caption());
    }
    if let Some(x) = layout.hum_bars {
        // The hum: signal bars that fill as the nearest missing crystal nears.
        let lit = (game.resonance() * 5.).ceil() as usize;
        let boost = 0.25 * juice.ping;
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
    if let Some(x) = layout.objective {
        draw_text_styled(objective, x, 29., bar_style());
    }
    if let Some(x) = layout.timer {
        draw_text_styled(scratch, x, 29., body(16., MUTED));
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
    let style = body(16., GOLD);
    if rel.abs() > std::f32::consts::FRAC_PI_2 {
        let arrow = if rel < 0. { "←" } else { "→" };
        let ax = if rel < 0. {
            x + 12.
        } else {
            x - 12. - styled::text_width(arrow, style)
        };
        draw_text_styled(arrow, ax, style.centered_baseline(y), style);
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

/// Callout block (head + optional echo/sub line) on a soft glass pill.
/// Returns its used height.
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
    let head_lines = ((max_height / head_lh).floor() as usize).min(2);
    if head_lines == 0 {
        return 0.;
    }
    let (hn, hw) = block(&c.head, width, head_style);
    let hn = hn.min(head_lines);
    let room = max_height - hn as f32 * head_lh;
    let sub_lines = if c.sub.is_empty() {
        0
    } else {
        ((room / sub_lh).floor().max(0.) as usize).min(2)
    };
    let (sn, sw) = if sub_lines > 0 {
        let (n, w) = block(&c.sub, width, sub_style);
        (n.min(sub_lines), w)
    } else {
        (0, 0.)
    };
    let height = hn as f32 * head_lh + sn as f32 * sub_lh;
    let pill_w = hw.max(sw) + 32.;
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
    let mut y = top + ascent(&head_style);
    y += centered(&c.head, w * 0.5, y, width, head_style, hn);
    if sn > 0 {
        centered(&c.sub, w * 0.5, y - head_lh + sub_lh, width, sub_style, sn);
    }
    height
}

/// Rising arcade pops above the reticle, in the Classic face.
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
        let fit = (w - 32.) / styled::text_width(&pop.text, style).max(1.);
        if fit < 1. {
            size = (size * fit).max(8.);
        }
        let style = display(size * grow, fade(pop.color, alpha)).with_align(TextAlign::Center);
        shapes::glow_disc(
            vec2(w * 0.5, y - size * 0.4),
            size * 2.2,
            fade(pop.color, 0.22 * alpha),
        );
        draw_text_styled(&pop.text, w * 0.5, y, style);
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

/// The nearby-object prompt on a glass card. Ready objects show a key cap and
/// the action in mint; blocked ones show the warm blocker copy (identical in
/// style and width to the warm notice, which the overlay regression compares).
pub(super) fn prompt(top: f32, w: f32, key: &str, action: Option<&str>, text: &str, color: Color) {
    let card = Rect::new(12., top - 8., (w - 24.).max(1.), 88.);
    shapes::drop_shadow(
        card,
        14.,
        Shadow::soft(Color::new(0., 0., 0., 0.35)).blur(16.),
    );
    shapes::fill_rounded_rect(card, 14., PROMPT_PANEL);
    shapes::stroke_rounded_rect(card, 14., 1., fade(color, 0.3));
    let theme = theme();
    match action {
        Some(action) => {
            let label = body(16., color);
            let cap_w = keycap_width(key, &theme);
            let total = cap_w + 10. + styled::text_width(action, label);
            let x = (w - total) * 0.5;
            let cy = top + 30.;
            kit::keycap(x, cy - 12., key, &theme);
            draw_text_styled(action, x + cap_w + 10., label.centered_baseline(cy), label);
        }
        None => {
            let style = body(copy_size(w), color);
            centered(text, w * 0.5, top + style.size * 1.125, w - 32., style, 3);
        }
    }
}

/// The warm notice line above the bottom edge, same style as blocked prompts.
pub(super) fn notice(text: &str, w: f32, h: f32) {
    let style = body(copy_size(w), WARM);
    centered(
        text,
        w * 0.5,
        h - 150. + style.size * 1.125,
        w - 32.,
        style,
        3,
    );
}

fn keycap_width(label: &str, theme: &KitTheme) -> f32 {
    let style = TextStyle::modern(theme.body, theme.ink);
    let h = theme.body * 1.5 + 2.;
    (styled::text_width(label, style) + kit::SPACE[2] * 2.).max(h - 3.)
}

fn prompt_row_width(keys: &[&str], action: &str, theme: &KitTheme) -> f32 {
    let caps: f32 = keys
        .iter()
        .map(|k| keycap_width(k, theme) + kit::SPACE[0])
        .sum();
    caps + kit::SPACE[0] + styled::text_width(action, TextStyle::modern(theme.body, theme.muted))
}

/// Movement/look onboarding key caps that fade out after a few metres.
pub(super) fn onboarding(game: &Game, keys: [&str; 4], w: f32, h: f32) {
    let alpha = (1. - game.walked / 6.).clamp(0., 1.);
    if alpha <= 0. {
        return;
    }
    let theme = theme().faded(alpha);
    let look = ["←", "→"];
    let a = prompt_row_width(&keys, "Move", &theme);
    let b = prompt_row_width(&look, "Look (or drag)", &theme);
    let gap = 28.;
    let y = h - 44.;
    if a + gap + b <= w - 32. {
        let x = (w - a - gap - b) * 0.5;
        kit::key_prompt(x, y, &keys, "Move", &theme);
        kit::key_prompt(x + a + gap, y, &look, "Look (or drag)", &theme);
    } else if a <= w - 32. {
        kit::key_prompt((w - a) * 0.5, y, &keys, "Move", &theme);
    }
}

/// Bottom help rows (toggle with the help binding).
pub(super) fn help(keys: [&str; 4], w: f32, h: f32) {
    let theme = theme();
    let rows: [(&[&str], &str); 3] = [
        (&keys[..2], "Use / Pause"),
        (&keys[2..], "Retry / Help"),
        (&[], "Move & look bindings: Controls (F2)"),
    ];
    for (i, (caps, action)) in rows.into_iter().enumerate() {
        let y = h - 98. + i as f32 * 32.;
        let width = prompt_row_width(caps, action, &theme);
        kit::key_prompt(((w - width) * 0.5).max(8.), y, caps, action, &theme);
    }
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
    /// The personal-best line.
    pub fn best_line(&self, out: &mut String) {
        out.clear();
        let time = self.summary.seconds;
        match (self.recorded, self.previous_best) {
            (false, _) => out.push_str("Demo run. Personal bests are for humans."),
            (true, None) => {
                out.push_str("★ First clear! Best time saved: ");
                stopwatch(out, time);
            }
            (true, Some(best)) if time < best - 0.05 => {
                let _ = write!(out, "★ New personal best! {:.1} s faster", best - time);
            }
            (true, Some(best)) if time <= best + 0.05 => {
                out.push_str("Tied your best: ");
                stopwatch(out, best);
            }
            (true, Some(best)) => {
                out.push_str("Best ");
                stopwatch(out, best);
                let _ = write!(out, " · {:.1} s to beat it", time - best);
            }
        }
    }
}

/// Joins stat fragments with wide gaps, breaking lines only between fragments
/// when `style` text would exceed the width. Gaps have no glyphs, so every
/// width paints exactly the same characters (the overlay regression counts them).
pub(super) fn stats_line(out: &mut String, parts: &[&str], style: TextStyle, w: f32) {
    let limit = (w - 48.).max(1.);
    out.clear();
    let mut line = 0.;
    let gap = styled::text_width("   ", style);
    for part in parts {
        let pw = styled::text_width(part, style);
        if out.is_empty() {
            out.push_str(part);
            line = pw;
        } else if line + gap + pw <= limit {
            out.push_str("   ");
            out.push_str(part);
            line += gap + pw;
        } else {
            out.push('\n');
            out.push_str(part);
            line = pw;
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
    let copy = copy_size(w);
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
    let mut colors = [PALE; 3];
    let (title, title_color, action, detail_color) = match card.state {
        SessionState::Ready => {
            header.push_str("Wendell went in. He's still in there.");
            stats_line(
                detail,
                &["3 crystals", "2 altars", "1 way out"],
                body(16., WARM),
                w,
            );
            match card.best {
                Some((best, secrets)) => {
                    a.push_str("★ Personal best ");
                    stopwatch(a, best);
                    let _ = write!(a, " · most secrets {secrets} of 2");
                    colors[0] = GOLD;
                }
                None => {
                    a.push_str("Rumor: one of the crystals is shy.");
                    colors[0] = VIOLET;
                }
            }
            ("CRYSTAL CAVERN", MINT, "Begin expedition", WARM)
        }
        SessionState::Paused => {
            detail.push_str(quips::pause(card.pauses));
            let _ = write!(stat, "{}/3 crystals", game.count());
            let _ = write!(header, "{}/2 altars", game.switches);
            let mut time = String::new();
            clock(&mut time, game.elapsed);
            stats_line(
                a,
                &[stat.as_str(), header.as_str(), time.as_str()],
                body(16., PALE),
                w,
            );
            header.clear();
            let _ = write!(b, "{}: restart the expedition", card.retry_hint);
            colors = [PALE, MUTED, PALE];
            ("PAUSED", MINT, "Resume", ECHO)
        }
        _ => {
            let results = card.results;
            let rank = results.rank();
            let summary = results.summary;
            header.push_str("Daylight! Expedition complete");
            let secrets = u32::from(summary.shy) + u32::from(summary.note);
            // The time rolls up in tenths when the card opens; the final value
            // (what the overlay regression reads) is exact.
            let tenths = (summary.seconds.max(0.) * 10.).round() as i64;
            let roll = motion_state.roll.get_or_insert_with(|| {
                let mut r = RollCounter::new(0);
                r.set_target(tenths);
                r
            });
            let shown = roll.advance(dt.clamp(0., 0.1), motion) as f32 / 10.;
            stat.push_str("Time ");
            stopwatch(stat, shown);
            let hints = format_args!("Hints {}", summary.hints);
            let mut hint_text = String::new();
            let _ = hint_text.write_fmt(hints);
            let mut secret_text = String::new();
            let _ = write!(secret_text, "Secrets {secrets}/2");
            stats_line(
                detail,
                &[stat.as_str(), hint_text.as_str(), secret_text.as_str()],
                body(16., WARM),
                w,
            );
            a.push_str(rank.quip);
            results.best_line(b);
            c.push_str(&quips::next_goal(summary));
            colors = [ECHO, GOLD, VIOLET];
            (rank.title, MINT, "Explore again", WARM)
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
    // Lay out the below-card lines first so the card can fit them exactly.
    let budget = (h - below - 14.).max(0.);
    let line_style = body(copy, PALE);
    let mut heights = [0_f32; 3];
    let mut used = 6.;
    for (i, text) in [&*a, &*b, &*c].into_iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        let (n, _) = block(text, card_w - 32., line_style);
        let room = ((budget - used) / line_style.line_height()).floor().max(0.) as usize;
        let n = n.min(room).min(2);
        if n == 0 {
            break;
        }
        heights[i] = n as f32 * line_style.line_height();
        used += heights[i] + 4.;
    }
    let down = if used > 6. { used + 6. } else { 0. };
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
        let style = body(copy, fade(STONE, 0.9 * item.alpha));
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
    // Detail copy (warm for Ready/Won stats; the echo's voice when paused).
    let item = Entrance::fade_up(8.).sample(t, 2, motion);
    centered(
        detail,
        w * 0.5,
        top + 80.,
        card_w - 32.,
        body(
            16.,
            fade(
                detail_color,
                if detail_color == WARM { 1. } else { item.alpha },
            ),
        ),
        2,
    );
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
    // Below-card lines arrive with a staggered fade-up.
    let mut y = below + 6.;
    for (i, text) in [&*a, &*b, &*c].into_iter().enumerate() {
        if heights[i] <= 0. {
            continue;
        }
        let item = Entrance::fade_up(10.).sample(t, 3 + i, motion);
        let n = (heights[i] / line_style.line_height()).round() as usize;
        centered(
            text,
            w * 0.5,
            y + copy * 1.125 + item.offset[1],
            card_w - 32.,
            body(copy, fade(colors[i], item.alpha)),
            n,
        );
        y += heights[i] + 4.;
    }
    hovered && is_mouse_button_pressed(MouseButton::Left)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn best_line_compares_against_the_previous_record() {
        let summary = RunSummary {
            seconds: 60.,
            ..Default::default()
        };
        let line = |previous_best, recorded| {
            let mut out = String::new();
            Results {
                summary,
                previous_best,
                recorded,
            }
            .best_line(&mut out);
            out
        };
        assert_eq!(line(None, true), "★ First clear! Best time saved: 1:00.0");
        assert!(line(Some(65.), true).contains("5.0 s faster"));
        assert!(line(Some(60.), true).starts_with("Tied"));
        assert_eq!(line(Some(50.), true), "Best 0:50.0 · 10.0 s to beat it");
        assert!(line(Some(50.), false).starts_with("Demo run"));
    }

    #[test]
    fn stat_lines_break_between_fragments_not_inside_them() {
        let parts = ["Time 0:54.6", "Hints 0", "Secrets 2/2"];
        let style = body(16., WARM);
        let mut out = String::new();
        stats_line(&mut out, &parts, style, 1280.);
        assert_eq!(out, "Time 0:54.6   Hints 0   Secrets 2/2");
        stats_line(&mut out, &parts, style, 320.);
        assert!(out.contains('\n'));
        for line in out.lines() {
            assert!(styled::text_width(line, style) <= 320. - 48.);
            assert!(parts.iter().any(|p| line.starts_with(p)), "{line}");
        }
        assert_eq!(
            out.replace('\n', "   "),
            "Time 0:54.6   Hints 0   Secrets 2/2"
        );
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
                    for objective in ["Find the crystals", "Walk into daylight"] {
                        let layout = bar_layout(w, shy, humming, objective, "12:34");
                        assert!(
                            layout.right <= w - CORNER.0,
                            "{w}: right edge {} enters the menu corner",
                            layout.right
                        );
                        // The crystals, altars and (while humming) the meter always fit.
                        assert!(layout.altars + 2. * ALTAR_STEP <= w - CORNER.0);
                        assert_eq!(layout.hum_bars.is_some(), humming);
                    }
                }
            }
        }
        let wide = bar_layout(1280., false, true, "Find the crystals", "0:42");
        assert!(wide.objective.is_some() && wide.timer.is_some() && wide.hum_label.is_some());
        assert_eq!(wide.strip_right, 1280. - CORNER.0 - 8.);
        // Marker glyphs (diamond reaches 9 px up) start below the corner.
        assert!(MARKER_Y - 9. >= CORNER.1);
    }

    #[test]
    fn copy_never_needs_more_lines_than_its_band_at_narrow_widths() {
        // The blocker prompt and notice share a 72 px band in the overlay regression.
        let blocker = "Altar I locked / Three crystals required (0 / 3 recovered)";
        for w in [320., 390., 844., 1280.] {
            let style = body(copy_size(w), WARM);
            let (lines, _) = block(blocker, w - 32., style);
            assert!(
                lines as f32 * style.line_height() <= 72.,
                "{w}: {lines} lines"
            );
        }
        // Stats on Ready/Won fit two detail lines (top + 62..116).
        let mut out = String::new();
        stats_line(
            &mut out,
            &["Time 12:34.5", "Hints 12", "Secrets 2/2"],
            body(16., WARM),
            320.,
        );
        assert!(out.lines().count() <= 2);
    }
}
