//! Heads-up display, callouts and the Ready / Paused / Won cards.
//!
//! Layout contract kept from the browser overlay regression: menus keep a
//! 172 px core panel at `top = max((h - 172) / 2, 8)` with the warm detail
//! copy at `top + 62` and the action button at `top + 116`. Extra card copy
//! sits above/below that core and never uses the warm notice color. During
//! play the warm color is reserved for blockers/notices, and the prompt panel
//! keeps its unique color.
use super::juice::{ECHO, GOLD, Juice, STONE, VIOLET, heading};
use super::quips::{self, Rank, RunSummary};
use super::{Game, MINT, WARM};
use ferravex_core::session::SessionState;
use ferravex_render::prelude::*;
use ferravex_render::ui::text_layout::LineLimit;
use ferravex_render::ui::{Align, TextStyle, Ui, UiRect};

pub(super) const PANEL: f32 = 172.;
pub(super) const BAR: f32 = 48.;
const BAR_FILL: Color = Color::new(0.03, 0.08, 0.08, 0.8);
const DIM: Color = Color::new(0.2, 0.3, 0.3, 1.);
const PALE: Color = Color::new(0.8, 0.86, 0.82, 1.);

pub(super) fn fade(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0., 1.))
}

pub(super) fn text_width(text: &str, size: u16) -> f32 {
    get_default_font()
        .try_metrics(size, 1.)
        .expect("nonzero HUD size")
        .measure_line(text)
        .expect("bounded HUD text")
        .width
}

/// Shadowed single-line text with its left edge at `x` and baseline at `y`.
pub(super) fn label(text: &str, x: f32, y: f32, size: u16, color: Color) {
    for (offset, tint) in [(1., fade(BLACK, color.a)), (0., color)] {
        draw_text_ex(
            text,
            x + offset,
            y + offset,
            TextParams {
                font_size: size,
                font_scale: 1.,
                color: tint,
            },
        );
    }
}

/// A faceted gem icon: filled when owned, an outline socket otherwise.
fn gem(cx: f32, cy: f32, r: f32, filled: bool, color: Color) {
    let hw = r * 0.62;
    let (l, rt, t, b) = (
        vec2(cx - hw, cy),
        vec2(cx + hw, cy),
        vec2(cx, cy - r),
        vec2(cx, cy + r),
    );
    if filled {
        draw_triangle(l, rt, t, color);
        draw_triangle(
            l,
            rt,
            b,
            fade(
                Color::new(color.r * 0.7, color.g * 0.7, color.b * 0.75, 1.),
                color.a,
            ),
        );
        // A small highlight facet keeps the icon readable on dark rock.
        draw_triangle(
            vec2(cx - hw * 0.45, cy - r * 0.1),
            vec2(cx, cy - r * 0.1),
            vec2(cx - hw * 0.1, cy - r * 0.7),
            fade(WHITE, 0.75 * color.a),
        );
    } else {
        draw_triangle(l, rt, t, Color::new(0.05, 0.12, 0.12, 0.9));
        draw_triangle(l, rt, b, Color::new(0.05, 0.12, 0.12, 0.9));
        for (a, c) in [(l, t), (t, rt), (rt, b), (b, l)] {
            draw_line(a.x, a.y, c.x, c.y, 1.5, fade(color, 0.55));
        }
    }
}

fn clock(seconds: f32) -> String {
    let whole = seconds.max(0.) as u32;
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// Results-card time with tenths, e.g. "0:54.6". The bitmap font has no
/// lowercase, so a trailing "s" would read as a 5.
pub(super) fn stopwatch(seconds: f32) -> String {
    let tenths = (seconds.max(0.) * 10.).round() as u32;
    format!("{}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

/// Browser fullscreen pins a 44 px menu button 8 px from the canvas's top-right
/// corner. HUD text, counters and controls stay out of `x > w - 72, y < 60`;
/// only the full-bleed bar background may pass under it.
pub(super) const CORNER: (f32, f32) = (72., 60.);
/// Right edge of every top-bar item: the reserved corner plus a small gap.
fn content_right(w: f32) -> f32 {
    w - CORNER.0 - 8.
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
        let label_w = text_width("HUM", 14) + 6.;
        if x + label_w + HUM_BARS <= limit {
            layout.hum_label = Some(x);
            x += label_w;
        }
        if x + HUM_BARS <= limit {
            layout.hum_bars = Some(x);
            x += HUM_BARS + 8.;
        }
    }
    let timer_w = text_width(timer, 17);
    let timer_x = limit - timer_w;
    let has_timer = x <= timer_x;
    if has_timer {
        layout.timer = Some(timer_x);
    }
    let room = if has_timer { timer_x - 14. } else { limit };
    if x + 6. + text_width(objective, 17) <= room {
        layout.objective = Some(x + 6.);
    }
    layout.right = [
        layout.strip_right,
        layout.timer.map_or(0., |t| t + timer_w),
        layout
            .objective
            .map_or(0., |o| o + text_width(objective, 17)),
        layout.hum_bars.map_or(x, |b| b + HUM_BARS),
    ]
    .into_iter()
    .fold(0., f32::max);
    layout
}

/// The slim top bar: crystals, altars, the hum meter, the current objective
/// and the clock, over the steady six-part objective strip.
pub(super) fn top_bar(game: &Game, juice: &Juice, w: f32) {
    // Full-bleed background; every item stays clear of the fullscreen menu corner.
    draw_rectangle(0., 0., w.max(1.), BAR, BAR_FILL);
    let timer = clock(game.elapsed);
    let objective = quips::objective(game.stage());
    let layout = bar_layout(w, game.shy_found, game.count() < 3, objective, &timer);
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
        let (bw, bh) = (32. * s, 26. * s);
        let (bx, by) = (x + 16. - bw * 0.5, 22. - bh * 0.5);
        draw_rectangle(
            bx,
            by,
            bw,
            bh,
            if awake {
                GOLD
            } else {
                Color::new(0.07, 0.13, 0.13, 0.95)
            },
        );
        if !awake {
            draw_rectangle_lines(bx, by, bw, bh, 1.5, fade(GOLD, 0.45));
        }
        let tw = text_width(name, 17);
        label(
            name,
            x + 16. - tw * 0.5,
            22. + 7.5,
            17,
            if awake {
                Color::new(0.1, 0.08, 0.04, 1.)
            } else {
                fade(GOLD, 0.7)
            },
        );
    }
    if let Some(x) = layout.hum_label {
        label("HUM", x, 29., 14, fade(PALE, 0.8));
    }
    if let Some(x) = layout.hum_bars {
        // The hum: signal bars that fill as the nearest missing crystal nears.
        let lit = (game.resonance() * 5.).ceil() as usize;
        let boost = 0.25 * juice.ping;
        for bar in 0..5 {
            let height = 6. + bar as f32 * 4.;
            let color = if bar < lit {
                Color::new(
                    (MINT.r + boost).min(1.),
                    (MINT.g + boost).min(1.),
                    (MINT.b + boost).min(1.),
                    1.,
                )
            } else {
                DIM
            };
            draw_rectangle(x + bar as f32 * 6., 32. - height, 4., height, color);
        }
    }
    if let Some(x) = layout.objective {
        label(objective, x, 29., 17, PALE);
    }
    if let Some(x) = layout.timer {
        label(&timer, x, 29., 17, fade(PALE, 0.75));
    }
    // A steady six-part bar includes walking into daylight, not just opening
    // the seal. It reads accepted graph completion without mutating it.
    let strip = (layout.strip_right - 16.).max(1.);
    for view in game.objectives.iter() {
        draw_rectangle(
            16. + view.index as f32 * strip / 6.,
            BAR - 4.,
            (strip / 6. - 2.).max(1.),
            3.,
            if view.state == ferravex_play::objective::ObjectiveState::Complete {
                MINT
            } else {
                DIM
            },
        );
    }
}

/// Top of the hint marker's glyphs: below the bar and the reserved corner.
pub(super) const MARKER_Y: f32 = BAR + 22.;

/// A heading marker under the bar once the whispers escalate.
pub(super) fn hint_marker(game: &Game, yaw: f32, w: f32) -> bool {
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
    draw_rectangle(w * 0.5 - span, y, span * 2., 1., fade(GOLD, 0.25));
    draw_triangle(vec2(x - 7., y), vec2(x + 7., y), vec2(x, y - 9.), GOLD);
    draw_triangle(vec2(x - 7., y), vec2(x + 7., y), vec2(x, y + 9.), GOLD);
    if rel.abs() > std::f32::consts::FRAC_PI_2 {
        let arrow = if rel < 0. { "<<" } else { ">>" };
        let ax = if rel < 0. {
            x + 12.
        } else {
            x - 12. - text_width(arrow, 17)
        };
        label(arrow, ax, y + 6., 17, GOLD);
    }
    let distance = format!("{:.0}m", target.distance(game.position));
    let dw = text_width(&distance, 14);
    label(
        &distance,
        (x - dw * 0.5).clamp(4., w - dw - 4.),
        y + 24.,
        14,
        fade(GOLD, 0.85),
    );
    true
}

/// Callout block (head + optional echo/sub line). Returns its used height.
pub(super) fn callout(ui: &mut Ui, juice: &Juice, w: f32, top: f32, max_height: f32) -> f32 {
    let Some(c) = &juice.callout else {
        return 0.;
    };
    let alpha = c.alpha();
    let head_size = if w >= 520. { 24. } else { 20. };
    let width = (w - 32.).max(1.);
    let head_lines = ((max_height / head_size).floor() as usize).min(2);
    if head_lines == 0 {
        return 0.;
    }
    let head = ui
        .wrapped_text(
            UiRect {
                min: [16., top],
                size: [width, head_size * head_lines as f32],
            },
            &c.head,
            TextStyle {
                size: head_size,
                color: fade(c.color, alpha),
                align: Align::Center,
                wrap: true,
                shadow: true,
            },
            LineLimit::Ellipsis(head_lines),
        )
        .map_or(0., |m| m.height);
    let mut used = head;
    let sub_lines = (((max_height - used - 4.) / 17.).floor().max(0.) as usize).min(2);
    if !c.sub.is_empty() && sub_lines > 0 {
        let sub_color = if c.color == ECHO { ECHO } else { PALE };
        used += 4.
            + ui.wrapped_text(
                UiRect {
                    min: [16., top + used + 4.],
                    size: [width, 17. * sub_lines as f32],
                },
                &c.sub,
                TextStyle {
                    size: 17.,
                    color: fade(sub_color, alpha * 0.7),
                    align: Align::Center,
                    wrap: true,
                    shadow: true,
                },
                LineLimit::Ellipsis(sub_lines),
            )
            .map_or(0., |m| m.height);
    }
    used
}

/// Rising text pops above the reticle.
pub(super) fn pops(juice: &Juice, w: f32, h: f32) {
    for pop in juice.pops.iter().flatten() {
        let t = pop.age / pop.life;
        let alpha = ((1. - t) / 0.35).clamp(0., 1.);
        let y = h * 0.5 - 34. - juice.pop_rise(pop);
        let tw = text_width(&pop.text, pop.size);
        let scale = ((w - 32.) / tw.max(1.)).min(1.);
        let x = (w - tw * scale) * 0.5;
        for (offset, tint) in [(2., fade(BLACK, alpha * 0.8)), (0., fade(pop.color, alpha))] {
            draw_text_ex(
                &pop.text,
                x + offset,
                y + offset,
                TextParams {
                    font_size: pop.size,
                    font_scale: scale,
                    color: tint,
                },
            );
        }
    }
}

/// A soft edge glow for accepted milestones; absent under reduced flash.
pub(super) fn edge_glow(juice: &Juice, w: f32, h: f32) {
    if juice.flash <= 0. {
        return;
    }
    let c = juice.flash_color;
    for band in 0..4 {
        let a = juice.flash * 0.22 * (1. - band as f32 / 4.);
        let tint = fade(c, a);
        let inset = band as f32 * 10.;
        draw_rectangle(inset, inset, w - inset * 2., 10., tint);
        draw_rectangle(inset, h - inset - 10., w - inset * 2., 10., tint);
        draw_rectangle(
            inset,
            inset + 10.,
            10.,
            (h - inset * 2. - 20.).max(0.),
            tint,
        );
        draw_rectangle(
            w - inset - 10.,
            inset + 10.,
            10.,
            (h - inset * 2. - 20.).max(0.),
            tint,
        );
    }
}

/// Movement/look onboarding that fades out once the player has walked a bit.
pub(super) fn onboarding(game: &Game, move_hint: &str, w: f32, h: f32) {
    let alpha = (1. - game.walked / 6.).clamp(0., 1.);
    if alpha <= 0. {
        return;
    }
    let line = format!("{move_hint}: MOVE    ARROWS / DRAG: LOOK");
    let tw = text_width(&line, 17);
    let scale = ((w - 32.) / tw.max(1.)).min(1.);
    let x = (w - tw * scale) * 0.5;
    draw_rectangle(
        0.,
        h - 40.,
        w,
        34.,
        fade(Color::new(0.02, 0.05, 0.05, 1.), 0.55 * alpha),
    );
    for (offset, tint) in [(1., fade(BLACK, alpha)), (0., fade(PALE, alpha))] {
        draw_text_ex(
            &line,
            x + offset,
            h - 17. + offset,
            TextParams {
                font_size: 17,
                font_scale: scale,
                color: tint,
            },
        );
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
    /// The personal-best line, or None when there is nothing to compare.
    pub fn best_line(&self) -> String {
        let time = self.summary.seconds;
        match (self.recorded, self.previous_best) {
            (false, _) => "Demo run. Personal bests are for humans.".into(),
            (true, None) => format!("FIRST CLEAR! BEST TIME SAVED: {}", stopwatch(time)),
            (true, Some(best)) if time < best - 0.05 => {
                format!("NEW PERSONAL BEST! {:.1} SEC FASTER", best - time)
            }
            (true, Some(best)) if time <= best + 0.05 => {
                format!("TIED YOUR BEST: {}", stopwatch(best))
            }
            (true, Some(best)) => format!(
                "BEST {} / {:.1} SEC TO BEAT IT",
                stopwatch(best),
                time - best
            ),
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

/// Joins stat fragments with wide gaps, breaking lines only between fragments
/// when `size` text would exceed the width. Gaps have no glyphs, so every
/// width paints exactly the same characters (the overlay regression counts them).
pub(super) fn stats_line(parts: &[&str], size: f32, w: f32) -> String {
    let columns = (((w - 32.).max(1.) / (size * 0.75)).floor() as usize).max(1);
    let mut out = String::new();
    let mut line = 0;
    for part in parts {
        if line == 0 {
            out.push_str(part);
            line = part.len();
        } else if line + 3 + part.len() <= columns {
            out.push_str("   ");
            out.push_str(part);
            line += 3 + part.len();
        } else {
            out.push('\n');
            out.push_str(part);
            line = part.len();
        }
    }
    out
}

pub(super) fn card_top(h: f32) -> f32 {
    ((h - PANEL) * 0.5).max(8.)
}

fn text_center(text: &str, y: f32, size: u16, color: Color, w: f32) -> f32 {
    let width = text_width(text, size).max(1.);
    let scale = ((w - 32.).max(1.) / width).min(1.);
    let x = (w - width * scale) * 0.5;
    for (offset, tint) in [(1., fade(BLACK, color.a)), (0., color)] {
        draw_text_ex(
            text,
            x + offset,
            y + offset,
            TextParams {
                font_size: size,
                font_scale: scale,
                color: tint,
            },
        );
    }
    width * scale
}

fn wrapped(ui: &mut Ui, text: &str, top: f32, budget: f32, size: f32, color: Color, w: f32) -> f32 {
    let lines = ((budget / size).floor() as usize).min(3);
    if lines == 0 || text.is_empty() {
        return 0.;
    }
    ui.wrapped_text(
        UiRect {
            min: [16., top],
            size: [(w - 32.).max(1.), size * lines as f32],
        },
        text,
        TextStyle {
            size,
            color,
            align: Align::Center,
            wrap: true,
            shadow: true,
        },
        LineLimit::Ellipsis(lines),
    )
    .map_or(0., |m| m.height)
}

/// Ready, Paused and Won cards. Returns true when the action was clicked.
pub(super) fn card(ui: &mut Ui, card: Card, game: &Game, juice: &Juice, w: f32, h: f32) -> bool {
    let top = card_top(h);
    let header_room = top - 8. >= 42. && card.state != SessionState::Paused;
    let up = if header_room { 42. } else { 0. };
    let below = top + PANEL;
    let budget = (h - below - 14.).max(0.);
    let mut down = 0.;
    let mut lines: Vec<(String, Color)> = Vec::new();
    let (title, title_color, header, detail, detail_color, action) = match card.state {
        SessionState::Ready => {
            match card.best {
                Some((best, secrets)) => lines.push((
                    format!(
                        "PERSONAL BEST {} / MOST SECRETS {secrets} OF 2",
                        stopwatch(best)
                    ),
                    GOLD,
                )),
                None => lines.push(("Rumor: one of the crystals is shy.".into(), VIOLET)),
            }
            (
                "CRYSTAL CAVERN".to_owned(),
                MINT,
                "Wendell went in. He's still in there.",
                stats_line(&["3 CRYSTALS", "2 ALTARS", "1 WAY OUT"], 18., w),
                WARM,
                "Begin expedition",
            )
        }
        SessionState::Paused => {
            let crystals = format!("{}/3 CRYSTALS", game.count());
            let altars = format!("{}/2 ALTARS", game.switches);
            let time = clock(game.elapsed);
            lines.push((stats_line(&[&crystals, &altars, &time], 17., w), PALE));
            lines.push((
                format!("{}: RESTART THE EXPEDITION", card.retry_hint),
                fade(PALE, 0.7),
            ));
            (
                "EXPEDITION PAUSED".to_owned(),
                MINT,
                "",
                quips::pause(card.pauses).to_owned(),
                ECHO,
                "Resume expedition",
            )
        }
        _ => {
            let results = card.results;
            let rank = results.rank();
            let summary = results.summary;
            lines.push((rank.quip.to_owned(), ECHO));
            lines.push((results.best_line(), GOLD));
            lines.push((quips::next_goal(summary).to_uppercase(), VIOLET));
            let secrets = u32::from(summary.shy) + u32::from(summary.note);
            (
                rank.title.to_owned(),
                MINT,
                "DAYLIGHT! EXPEDITION COMPLETE",
                stats_line(
                    &[
                        &format!("TIME {}", stopwatch(summary.seconds)),
                        &format!("HINTS {}", summary.hints),
                        &format!("SECRETS {secrets}/2"),
                    ],
                    18.,
                    w,
                ),
                WARM,
                "Explore again",
            )
        }
    };
    if card.state == SessionState::Won {
        // Daylight washes over the cave behind the results card.
        let glow = if juice.motion() > 0. {
            (juice.celebrate / 0.8).clamp(0., 1.)
        } else {
            1.
        };
        draw_rectangle(0., 0., w, h, Color::new(1., 0.95, 0.82, 0.3 * glow));
    }
    // Queue the below-card copy first so the panel can fit it exactly. Ui text
    // paints after the panel regardless of submission order.
    let mut y = below + 4.;
    for (text, color) in &lines {
        let used = wrapped(ui, text, y, budget - (y - below), 17., *color, w);
        if used <= 0. {
            break;
        }
        y += used + 6.;
        down = y - below;
    }
    draw_rectangle(
        0.,
        top - up,
        w,
        PANEL + up + down + if down > 0. { 6. } else { 0. },
        Color::new(0.03, 0.08, 0.08, 0.94),
    );
    if up > 0. && !header.is_empty() {
        // Wraps instead of shrinking, so narrow screens keep readable copy.
        let lines = if text_width(header, 17) <= w - 32. {
            1.
        } else {
            2.
        };
        wrapped(
            ui,
            header,
            top - 4. - 17. * lines,
            17. * lines,
            17.,
            fade(STONE, 0.9),
            w,
        );
    }
    let reveal = if juice.motion() > 0. && card.state == SessionState::Won {
        (juice.celebrate / 0.45).clamp(0., 1.)
    } else {
        1.
    };
    let title_width = text_center(
        &title,
        top + 45.,
        38,
        fade(title_color, 0.25 + 0.75 * reveal),
        w,
    );
    if card.state == SessionState::Won {
        // Twinkling crystal stars beside the rank title; static when motion is reduced.
        for side in [-1., 1.] {
            let cx = w * 0.5 + side * (title_width * 0.5 + 18.);
            if cx < 10. || cx > w - 10. {
                continue;
            }
            let twinkle = if juice.motion() > 0. {
                0.75 + 0.25 * (juice.celebrate * 5. + side).sin()
            } else {
                1.
            };
            let r = 9. * twinkle;
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
    let _ = wrapped(ui, &detail, top + 62., 54., 18., detail_color, w);
    let width = (w - 32.).clamp(1., 320.);
    ferravex_render::ui::button_with_options(
        Rect::new((w - width) * 0.5, top + 116., width, 44.),
        &format!("{} / {action}", card.confirm_hint),
        ferravex_render::ui::ButtonOptions {
            enabled: !card.controls_open,
            selected: true,
        },
    )
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
            Results {
                summary,
                previous_best,
                recorded,
            }
            .best_line()
        };
        assert_eq!(line(None, true), "FIRST CLEAR! BEST TIME SAVED: 1:00.0");
        assert!(line(Some(65.), true).contains("5.0 SEC FASTER"));
        assert!(line(Some(60.), true).starts_with("TIED"));
        assert_eq!(line(Some(50.), true), "BEST 0:50.0 / 10.0 SEC TO BEAT IT");
        assert!(line(Some(50.), false).starts_with("Demo run"));
    }

    #[test]
    fn stat_lines_break_between_fragments_not_inside_them() {
        let parts = ["TIME 0:54.6", "HINTS 0", "SECRETS 2/2"];
        assert_eq!(
            stats_line(&parts, 18., 1280.),
            "TIME 0:54.6   HINTS 0   SECRETS 2/2"
        );
        assert_eq!(
            stats_line(&parts, 18., 390.),
            "TIME 0:54.6   HINTS 0\nSECRETS 2/2"
        );
        assert_eq!(
            stats_line(&parts, 18., 200.),
            "TIME 0:54.6\nHINTS 0\nSECRETS 2/2"
        );
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
                    for objective in ["FIND THE CRYSTALS", "WALK INTO DAYLIGHT"] {
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
        let wide = bar_layout(1280., false, true, "FIND THE CRYSTALS", "0:42");
        assert!(wide.objective.is_some() && wide.timer.is_some() && wide.hum_label.is_some());
        assert_eq!(wide.strip_right, 1280. - CORNER.0 - 8.);
        // Marker glyphs (diamond reaches 9 px up, arrow text 7.5 px up) start below the corner.
        assert!(MARKER_Y - 9. >= CORNER.1);
    }

    #[test]
    fn clock_reads_minutes_and_seconds() {
        assert_eq!(clock(0.), "0:00");
        assert_eq!(clock(67.9), "1:07");
        assert_eq!(clock(-3.), "0:00");
        assert_eq!(stopwatch(54.66), "0:54.7");
        assert_eq!(stopwatch(59.96), "1:00.0");
    }
}
