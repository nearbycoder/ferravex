//! HUD, in-world overlays, Foreman Gus's speech bubble and the
//! Ready/Paused/Results cards.
//!
//! The play HUD shows only what matters now: targets left and the score on
//! top; shells, one status/coaching line and three buttons at the bottom. One
//! banner slot and one speech bubble keep messages from stacking. Cards drop
//! their least important rows instead of overlapping the bottom panel on
//! short screens, and every line is fitted to its width.
use super::juice::{PopText, PuffKind};
use super::quips::{self, RunSummary};
use super::score::{self, Callout};
use super::*;

pub(super) const GOLD: Color = Color::from_rgba(247, 198, 88, 255);
const MUTED: Color = Color::from_rgba(178, 180, 160, 255);
const GREEN: Color = Color::from_rgba(156, 196, 122, 255);
const PANEL: Color = Color::new(0.10, 0.14, 0.13, 0.94);
const EDGE: Color = Color::new(0.30, 0.36, 0.33, 1.0);
const SHADOW: Color = Color::new(0.0, 0.0, 0.0, 0.35);

/// Stored records shown on the Ready and Results cards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Best {
    pub points: Option<u64>,
    pub stars: u8,
}

/// The finished run against stored records, captured once at the result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Results {
    pub previous: Best,
    pub new_best: bool,
    pub new_stars: bool,
}

/// Onboarding progress for this session; hints fade once each is learned.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Coach {
    pub aimed: bool,
    pub fired: bool,
    pub knocked: bool,
}

/// Current binding text for prompts, refreshed after input sampling. The
/// strings are reused, so steady-state frames do not allocate here.
#[derive(Default)]
pub(super) struct Hints {
    pub fire: String,
    pub confirm: String,
    pub pause: String,
    pub touch: bool,
}

impl Hints {
    pub fn refresh(&mut self, input: &PlayerInput) {
        let mut buffer = [0; 96];
        for (slot, action) in [
            (&mut self.fire, "fire"),
            (&mut self.confirm, "confirm"),
            (&mut self.pause, "pause"),
        ] {
            slot.clear();
            slot.push_str(action_hint(input, action, &mut buffer));
        }
        self.touch = input.current_device() == HintDevice::Touch;
    }
}

pub(super) struct View<'a> {
    pub foundry: &'a Foundry,
    pub layout: &'a HudLayout,
    pub camera: &'a RangeCamera,
    pub w: f32,
    pub h: f32,
    pub controls_open: bool,
    pub hints: &'a Hints,
    pub results: &'a Results,
    pub best: Best,
    pub coach: &'a Coach,
}

pub(super) fn draw(view: &View) {
    let state = view.foundry.session.state();
    world_overlay(view, state);
    if state == SessionState::Playing {
        bubble(view);
    }
    top_bar(view, state);
    bottom_panel(view, state);
    match state {
        SessionState::Playing => banner(view),
        SessionState::Ready => ready_card(view),
        SessionState::Paused => paused_card(view),
        SessionState::Won | SessionState::Lost => {
            let reveal = view.foundry.juice.card_reveal();
            if reveal > 0.0 {
                results_card(view, reveal);
            } else {
                banner(view);
            }
        }
    }
}

// --- Text and shape helpers -------------------------------------------------

pub(super) fn text_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * 0.75 * size
}

/// The largest of `sizes` at which `text` fits in `width`, else the smallest.
fn fitting_size(text: &str, width: f32, sizes: &[f32]) -> f32 {
    sizes
        .iter()
        .copied()
        .find(|size| text_width(text, *size) <= width)
        .unwrap_or(sizes[sizes.len() - 1])
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Center,
    Right,
}

/// Draw `text` with its baseline at `y`, ellipsized to `width`.
fn fit(text: &str, x: f32, y: f32, width: f32, size: f32, color: Color, align: Align) {
    let Ok(fitted) = ui::fit_label(text, width.max(0.0), 0.75 * size) else {
        return;
    };
    let left = match align {
        Align::Left => x,
        Align::Center => x + (width - fitted.width) * 0.5,
        Align::Right => x + width - fitted.width,
    };
    label(fitted.prefix, left, y, size, color);
    if !fitted.suffix.is_empty() {
        label(
            fitted.suffix,
            left + text_width(fitted.prefix, size),
            y,
            size,
            color,
        );
    }
}

fn fade(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0.0, 1.0))
}

fn lighten(color: Color, amount: f32) -> Color {
    Color::new(
        color.r + (1.0 - color.r) * amount,
        color.g + (1.0 - color.g) * amount,
        color.b + (1.0 - color.b) * amount,
        color.a,
    )
}

/// Thousands separators: 12,345.
pub(super) fn grouped(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A five-point star; `filled` stars use `color`, others are outlines.
fn star(center: Vec2, radius: f32, filled: bool, color: Color) {
    let point = |i: usize| {
        let r = if i.is_multiple_of(2) {
            radius
        } else {
            radius * 0.45
        };
        let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
        center + vec2(angle.cos(), angle.sin()) * r
    };
    for i in 0..10 {
        let (a, b) = (point(i), point(i + 1));
        if filled {
            draw_triangle(center, a, b, color);
        } else {
            draw_line(a.x, a.y, b.x, b.y, (radius * 0.12).max(1.5), color);
        }
    }
}

fn stars_row(center: Vec2, radius: f32, earned: u8, color: Color) {
    let spacing = radius * 2.5;
    for i in 0..3 {
        let at = center + vec2((i as f32 - 1.0) * spacing, 0.0);
        let filled = i < earned;
        if filled {
            star(at + vec2(1.5, 2.0), radius, true, SHADOW);
        }
        star(
            at,
            radius,
            filled,
            if filled { color } else { fade(MUTED, 0.7) },
        );
    }
}

fn panel(rect: Rect, accent: Option<Color>) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, PANEL);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, EDGE);
    if let Some(accent) = accent {
        draw_rectangle(rect.x, rect.y, rect.w, 3.0, accent);
    }
}

/// Pixels per metre at a world point, for sizing in-world 2D effects.
fn pixels_per_metre(view: &View, at: Vec3) -> f32 {
    match (
        view.camera.project(view.w, view.h, at),
        view.camera.project(view.w, view.h, at + Vec3::Y),
    ) {
        (Some(a), Some(b)) => a.distance(b),
        _ => 0.0,
    }
}

// --- In-world overlays --------------------------------------------------------

fn world_overlay(view: &View, state: SessionState) {
    let juice = &view.foundry.juice;
    let game = &view.foundry.range;
    let project = |p: Vec3| view.camera.project(view.w, view.h, p);
    // Smoke trail behind the newest ball.
    let len = juice.trail_len;
    for (i, p) in juice.trail[..len].iter().enumerate().skip(1) {
        if let Some(at) = project(*p) {
            let t = i as f32 / len as f32;
            let r = pixels_per_metre(view, *p) * 0.22 * (1.0 - 0.6 * t);
            draw_poly(
                at.x,
                at.y,
                7,
                r,
                i as f32,
                Color::new(0.55, 0.55, 0.52, 0.45 * (1.0 - t)),
            );
        }
    }
    for puff in juice.puffs.iter().flatten() {
        let Some(at) = project(puff.position) else {
            continue;
        };
        let r = puff.current_radius() * pixels_per_metre(view, puff.position);
        let alpha = puff.alpha();
        let color = match puff.kind {
            PuffKind::Dust => Color::new(0.84, 0.78, 0.64, 0.72 * alpha),
            PuffKind::Smoke => Color::new(0.62, 0.62, 0.59, 0.55 * alpha),
            PuffKind::Flash => Color::new(1.0, 0.93, 0.66, 0.9 * alpha),
        };
        draw_poly(at.x, at.y, 7, r, puff.spin, color);
    }
    for ring in juice.rings.iter().flatten() {
        if let Some(at) = project(ring.at) {
            let t = ring.age / ring.life;
            let r = ring.radius * (0.4 + t) * pixels_per_metre(view, ring.at);
            draw_circle_lines(at.x, at.y, r, 3.0 * (1.0 - t) + 1.0, fade(GOLD, 1.0 - t));
        }
    }
    if juice.flash > 0.0 {
        let muzzle = PIVOT + game.direction() * 1.8;
        if let Some(at) = project(muzzle) {
            let r = pixels_per_metre(view, muzzle) * (0.5 + 0.6 * juice.flash);
            draw_poly(
                at.x,
                at.y,
                8,
                r,
                0.3,
                Color::new(1.0, 0.9, 0.55, juice.flash),
            );
        }
    }
    if state == SessionState::Playing && game.phase == Phase::Aiming {
        aim_preview(view);
    }
    let card = juice.card_reveal() > 0.0;
    for pop in juice.pops.iter().flatten().filter(|_| !card) {
        let Some(at) = project(pop.at + Vec3::Y * juice.pop_rise(pop)) else {
            continue;
        };
        let alpha = pop.alpha();
        let (text, size, color) = match pop.text {
            PopText::Points { points, chain } if chain > 1 => {
                (format!("+{points} ×{chain}"), 24.0, GOLD)
            }
            PopText::Points { points, .. } if points >= score::TARGET_POINTS => {
                (format!("+{points}"), 24.0, GOLD)
            }
            PopText::Points { points, .. } => (format!("+{points}"), 16.0, SAND),
            PopText::Word(word) => (word.to_string(), 16.0, PAPER),
        };
        let width = text_width(&text, size);
        let x = (at.x - width * 0.5).clamp(8.0, (view.w - width - 8.0).max(8.0));
        label(&text, x + 1.0, at.y + 1.0, size, fade(SHADOW, alpha));
        label(&text, x, at.y, size, fade(color, alpha));
    }
}

/// Dotted arc plus a reticle that says what the ball will hit first.
fn aim_preview(view: &View) {
    let game = &view.foundry.range;
    if game.ammo == 0 || game.cooldown > 0.0 {
        return;
    }
    let project = |p: Vec3| view.camera.project(view.w, view.h, p);
    for (i, point) in game.preview.iter().enumerate().skip(1) {
        if let Some(at) = project(*point) {
            let r = if i % 3 == 0 { 3.4 } else { 2.4 };
            draw_circle(at.x + 1.0, at.y + 1.0, r + 0.6, SHADOW);
            draw_circle(at.x, at.y, r, Color::new(1.0, 0.95, 0.80, 0.95));
        }
    }
    let Some(hit) = game.preview_hit else {
        return;
    };
    let Some(at) = project(hit) else {
        return;
    };
    let (color, tag) = match game.block_at(hit) {
        Some(block) if block.target && !block.down => (GOLD, "TARGET"),
        Some(block) if !block.target => (lighten(SAND, 0.35), "SUPPORT"),
        Some(_) => (PAPER, ""),
        None => (PAPER, ""),
    };
    draw_circle_lines(at.x, at.y, 13.0, 4.0, SHADOW);
    draw_circle_lines(at.x, at.y, 12.0, 2.5, color);
    for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        draw_line(
            at.x + dx * 7.0,
            at.y + dy * 7.0,
            at.x + dx * 17.0,
            at.y + dy * 17.0,
            2.0,
            color,
        );
    }
    // The tag teaches what to hit; it fades away once something has fallen.
    if !view.coach.knocked && !tag.is_empty() {
        // Below the reticle, clear of Gus's bubble and the incoming arc.
        let width = text_width(tag, 16.0);
        let x = (at.x - width * 0.5).clamp(12.0, view.w - width - 12.0);
        draw_rectangle(x - 6.0, at.y + 20.0, width + 10.0, 24.0, PANEL);
        label(tag, x, at.y + 38.0, 16.0, color);
    }
}

// --- Play HUD -----------------------------------------------------------------

/// Browser fullscreen pins a 44-pixel menu button inset 8 pixels into the
/// canvas's top-right corner; HUD text and panels keep this square clear.
pub(super) const MENU_CORNER: Vec2 = vec2(72.0, 60.0);

/// Top-bar geometry. Wide screens use a targets panel on the left and a score
/// panel inset from the menu corner; compact screens fold both into one panel.
struct TopBar {
    compact: bool,
    pip: f32,
    gap: f32,
    left: Rect,
    /// Separate score panel (wide only).
    right: Option<Rect>,
    pace_w: f32,
}

impl TopBar {
    fn new(w: f32, count: &str, value: &str) -> Self {
        let compact = w < 560.0;
        let (pip, gap) = if compact { (11.0, 4.0) } else { (14.0, 5.0) };
        let heading_w = if compact {
            0.0
        } else {
            text_width("TARGETS", 16.0) + 12.0
        };
        let pips = 6.0 * pip + 5.0 * gap;
        let targets_w = 14.0 + heading_w + pips + 10.0 + text_width(count, 16.0) + 12.0;
        let value_w = text_width(value, 16.0).max(48.0);
        if compact {
            let width = (targets_w + 8.0 + value_w + 12.0).min(w - 16.0 - MENU_CORNER.x);
            return Self {
                compact,
                pip,
                gap,
                left: Rect::new(16.0, 12.0, width.max(1.0), 32.0),
                right: None,
                pace_w: 0.0,
            };
        }
        let pace_w = 3.0 * 17.0 + 10.0;
        let right_w = 14.0 + pace_w + text_width("SCORE ", 16.0) + value_w + 14.0;
        let right_x = w - MENU_CORNER.x - right_w;
        let left = Rect::new(16.0, 12.0, targets_w, 32.0);
        Self {
            compact,
            pip,
            gap,
            left,
            right: (right_x >= left.x + left.w + 8.0)
                .then(|| Rect::new(right_x, 12.0, right_w, 32.0)),
            pace_w,
        }
    }

    #[cfg(test)]
    fn rects(&self) -> impl Iterator<Item = Rect> {
        std::iter::once(self.left).chain(self.right)
    }
}

fn top_bar(view: &View, state: SessionState) {
    if state == SessionState::Ready {
        return;
    }
    let game = &view.foundry.range;
    let juice = &view.foundry.juice;
    let count = format!("{}/{GOAL}", game.score());
    // The counter rolls during play; a frozen result shows the exact total.
    let points = if state == SessionState::Playing {
        juice.shown_points.round() as u64
    } else {
        u64::from(game.tally.points)
    };
    let rolling = points < u64::from(game.tally.points);
    let value = grouped(points);
    let value_color = if rolling { GOLD } else { PAPER };
    let bar = TopBar::new(view.w, &count, &value);
    let (pip, gap) = (bar.pip, bar.gap);
    panel(bar.left, None);
    let mut x = bar.left.x + 14.0;
    if !bar.compact {
        label("TARGETS", x, 34.0, 16.0, MUTED);
        x += text_width("TARGETS", 16.0) + 12.0;
    }
    let top = 28.0 - pip * 0.5;
    for block in game.blocks.iter().filter(|block| block.target) {
        if block.down {
            draw_rectangle(x, top, pip, pip, GREEN);
            let s = pip / 14.0;
            draw_line(x + 3.0 * s, 28.0, x + 6.0 * s, 28.0 + 3.0 * s, 2.0, INK);
            draw_line(
                x + 6.0 * s,
                28.0 + 3.0 * s,
                x + 11.0 * s,
                28.0 - 4.0 * s,
                2.0,
                INK,
            );
        } else {
            draw_rectangle(x, top, pip, pip, RUST);
            draw_rectangle_lines(x, top, pip, pip, 1.0, lighten(RUST, 0.3));
            draw_circle(x + pip * 0.5, 28.0, 2.0, PAPER);
        }
        x += pip + gap;
    }
    label(&count, x + 5.0, 34.0, 16.0, PAPER);
    if bar.compact {
        let start = x + 5.0 + text_width(&count, 16.0) + 8.0;
        let right = bar.left.x + bar.left.w - 12.0;
        fit(
            &value,
            start,
            34.0,
            right - start,
            16.0,
            value_color,
            Align::Right,
        );
        return;
    }
    let Some(right) = bar.right else {
        return;
    };
    // Stars still on offer for this run, shown while playing.
    let pace = match state {
        SessionState::Won | SessionState::Lost => view.foundry.summary().stars,
        _ if game.ammo == 0 => 1,
        _ => score::stars(true, game.tally.shots + 1),
    };
    panel(right, None);
    for i in 0..3 {
        star(
            vec2(right.x + 22.0 + i as f32 * 17.0, 28.0),
            7.0,
            i < pace,
            if i < pace { GOLD } else { fade(MUTED, 0.6) },
        );
    }
    let x = right.x + 14.0 + bar.pace_w;
    label("SCORE ", x, 34.0, 16.0, MUTED);
    let heading_w = text_width("SCORE ", 16.0);
    fit(
        &value,
        x + heading_w,
        34.0,
        right.w - 28.0 - bar.pace_w - heading_w,
        16.0,
        value_color,
        Align::Right,
    );
}

fn coach_hint(view: &View) -> Option<String> {
    let coach = view.coach;
    let hints = view.hints;
    let tally = &view.foundry.range.tally;
    if !coach.aimed {
        Some(if hints.touch {
            "DRAG TO AIM · PRESS FIRE TO SHOOT".into()
        } else {
            format!("AIM: MOVE KEYS OR DRAG · FIRE: {}", hints.fire)
        })
    } else if !coach.fired {
        Some("FOLLOW THE DOTTED ARC, THEN FIRE".into())
    } else if !coach.knocked && tally.misses > 0 {
        Some("TIP: KNOCK OUT A LOW SANDSTONE SUPPORT".into())
    } else {
        None
    }
}

fn prompt(hints: &Hints, compact: bool, button: &str, verb: &str) -> String {
    if hints.touch {
        format!("PRESS {button} BELOW")
    } else if compact {
        format!("{}: {button}", hints.confirm)
    } else {
        format!("PRESS {} {verb}", hints.confirm)
    }
}

fn status(view: &View, state: SessionState) -> (String, Color) {
    let game = &view.foundry.range;
    let feedback = &view.foundry.feedback;
    match state {
        // Cards carry the next action; the status line adds flavour instead.
        SessionState::Ready if view.w < 560.0 => ("GUS LOOKS NERVOUS.".into(), MUTED),
        SessionState::Ready => ("FOREMAN GUS IS WATCHING. NERVOUSLY.".into(), MUTED),
        SessionState::Won => ("FOUNDRY CLEARED!".into(), GOLD),
        SessionState::Lost => ("OUT OF SHELLS".into(), PAPER),
        SessionState::Paused => ("RANGE PAUSED".into(), MUTED),
        SessionState::Playing => {
            if feedback.destruction > 0.0 {
                (format!("+{} TARGETS DOWN", feedback.knocked_down), GOLD)
            } else if feedback.shot > 0.0 {
                (format!("SHOT {} AWAY", AMMO - game.ammo), PAPER)
            } else if feedback.rebuilt > 0.0 {
                ("REBUILT: 8 SHELLS READY".into(), GREEN)
            } else if game.ammo == 0 {
                (
                    format!("SETTLING {:.1}s", (SETTLE - game.since_shot).max(0.0)),
                    GOLD,
                )
            } else if game.cooldown > 0.0 {
                (format!("RELOADING {:.1}s", game.cooldown), PAPER)
            } else if let Some(hint) = coach_hint(view) {
                (hint, MUTED)
            } else {
                (String::new(), PAPER)
            }
        }
    }
}

fn button(rect: Rect, title: &str, enabled: bool, primary: bool) {
    let hovered = enabled && rect.contains(Vec2::from(mouse_position()));
    let base = if primary {
        RUST
    } else {
        Color::new(0.17, 0.21, 0.20, 1.0)
    };
    let fill = match (enabled, hovered) {
        (false, _) => Color::new(0.14, 0.17, 0.16, 1.0),
        (true, true) => lighten(base, 0.12),
        (true, false) => base,
    };
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, fill);
    let edge = if enabled && primary {
        lighten(RUST, 0.3)
    } else {
        EDGE
    };
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, edge);
    if enabled && primary {
        draw_rectangle(rect.x, rect.y, rect.w, 2.0, lighten(RUST, 0.45));
    }
    let size = (16.0_f32).min((rect.w - 12.0) / (0.75 * title.chars().count().max(1) as f32));
    let color = if enabled { PAPER } else { fade(MUTED, 0.7) };
    let x = rect.x + (rect.w - text_width(title, size)) * 0.5 + size / 16.0;
    label(title, x, rect.y + rect.h * 0.5 + size * 0.44, size, color);
}

fn bottom_panel(view: &View, state: SessionState) {
    let layout = view.layout;
    let game = &view.foundry.range;
    let p = layout.panel;
    panel(p, Some(fade(RUST, 0.9)));
    // Shells: loaded ones are bright cannonballs, spent ones are rings.
    for i in 0..AMMO {
        let at = vec2(p.x + 22.0 + f32::from(i) * 19.0, p.y + 22.0);
        if i < game.ammo {
            draw_circle(at.x + 1.0, at.y + 1.5, 7.0, SHADOW);
            draw_circle(at.x, at.y, 7.0, Color::from_rgba(214, 208, 190, 255));
            draw_circle(at.x - 2.0, at.y - 2.0, 2.2, PAPER);
        } else {
            draw_circle_lines(at.x, at.y, 6.0, 1.5, fade(MUTED, 0.5));
        }
    }
    let shells_right = p.x + 22.0 + f32::from(AMMO) * 19.0;
    let shells = format!("{} LEFT", game.ammo);
    label(&shells, shells_right, p.y + 29.0, 16.0, MUTED);
    let readout_left = shells_right + text_width(&shells, 16.0) + 24.0;
    if state != SessionState::Ready {
        let aim = format!(
            "YAW {:+.0}°  ELEV {:.0}°",
            game.yaw.to_degrees(),
            game.elevation.to_degrees()
        );
        let width = text_width(&aim, 16.0);
        if p.x + p.w - 16.0 - width >= readout_left {
            label(&aim, p.x + p.w - 16.0 - width, p.y + 29.0, 16.0, MUTED);
        }
    }
    let (text, color) = status(view, state);
    fit(
        &text,
        p.x + 16.0,
        p.y + 52.0,
        p.w - 32.0,
        16.0,
        color,
        Align::Left,
    );
    let controls_open = view.controls_open;
    let paused = view.foundry.session.is_paused();
    let fire_title = match state {
        SessionState::Ready => "START",
        SessionState::Won => "AGAIN",
        SessionState::Lost => "RETRY",
        SessionState::Paused => "PAUSED",
        SessionState::Playing if game.ammo == 0 => "EMPTY",
        SessionState::Playing if game.cooldown > 0.0 => "LOADING",
        SessionState::Playing => "FIRE",
    };
    let fire_enabled = !controls_open
        && match state {
            SessionState::Ready | SessionState::Won | SessionState::Lost => true,
            SessionState::Playing => game.can_fire(),
            SessionState::Paused => false,
        };
    button(layout.fire, fire_title, fire_enabled, true);
    button(layout.rebuild, "REBUILD", !controls_open, false);
    button(
        layout.pause,
        if paused { "RESUME" } else { "PAUSE" },
        !controls_open && matches!(state, SessionState::Playing | SessionState::Paused),
        paused,
    );
    if game.cooldown > 0.0 {
        let bar = Rect::new(
            layout.fire.x,
            layout.fire.y + layout.fire.h + 4.0,
            layout.fire.w,
            4.0,
        );
        draw_rectangle(bar.x, bar.y, bar.w, bar.h, STEEL);
        draw_rectangle(
            bar.x,
            bar.y,
            bar.w * (1.0 - game.cooldown / RELOAD),
            bar.h,
            GOLD,
        );
    }
}

fn callout_text(callout: Callout, points: u32) -> (String, String, Color) {
    match callout {
        Callout::Chain(n) => (format!("CHAIN ×{n}!"), format!("+{points}"), PAPER),
        Callout::Timber => (
            "TIMBER!".into(),
            format!("TOWER DOWN · +{points}"),
            Color::from_rgba(240, 140, 60, 255),
        ),
        Callout::Domino => (
            "DOMINO EFFECT!".into(),
            format!("TWO TOWERS · +{points}"),
            GOLD,
        ),
        Callout::Overkill => (
            "OVERKILL!".into(),
            format!("KICKING RUBBLE · +{points}"),
            GOLD,
        ),
        Callout::OneShotWonder => ("ONE-SHOT WONDER!".into(), format!("+{points}"), GOLD),
        Callout::Delayed => (
            "WAIT... IT COUNTS!".into(),
            "SLOW-MOTION DEMOLITION".into(),
            GREEN,
        ),
        Callout::Bell => ("DING!".into(), format!("SECRET BELL · +{points}"), GOLD),
        Callout::Spare(n) => (format!("SPARE SHELLS ×{n}"), format!("+{points}"), GOLD),
    }
}

fn banner(view: &View) {
    let juice = &view.foundry.juice;
    let Some(banner) = juice.banner else {
        return;
    };
    let (title, sub, color) = callout_text(banner.callout, banner.points);
    let alpha = banner.alpha();
    let max_w = view.w - 48.0;
    // Compact screens get a shorter band so the towers stay visible.
    let sizes: &[f32] = if view.w < 560.0 || view.h < 500.0 {
        &[24.0, 16.0]
    } else {
        &[32.0, 24.0, 16.0]
    };
    let base = fitting_size(&title, max_w, sizes);
    let size = base * banner.scale(juice.motion().min(1.0));
    let band_w = (text_width(&title, base).max(text_width(&sub, 16.0)) + 48.0).min(view.w - 32.0);
    let top = view.layout.scene.y + 6.0;
    let band = Rect::new((view.w - band_w) * 0.5, top, band_w, base + 40.0);
    draw_rectangle(
        band.x,
        band.y,
        band.w,
        band.h,
        fade(Color::new(0.08, 0.11, 0.10, 0.88), alpha),
    );
    draw_rectangle(band.x, band.y, band.w, 3.0, fade(color, alpha));
    fit(
        &title,
        24.0,
        top + 12.0 + base * 0.5 + size * 0.44,
        view.w - 48.0,
        size,
        fade(color, alpha),
        Align::Center,
    );
    fit(
        &sub,
        24.0,
        top + base + 32.0,
        view.w - 48.0,
        16.0,
        fade(PAPER, alpha),
        Align::Center,
    );
}

/// Split a line at the space that makes the longer half shortest, preferring
/// a " · " separator whose halves fit.
fn wrap(text: &str, width: f32, size: f32) -> (&str, &str) {
    if text_width(text, size) <= width {
        return (text, "");
    }
    let separated = text
        .match_indices(" · ")
        .map(|(i, sep)| (&text[..i], &text[i + sep.len()..]))
        .filter(|(a, b)| text_width(a, size) <= width && text_width(b, size) <= width)
        .min_by_key(|(a, b)| a.chars().count().max(b.chars().count()));
    if let Some(halves) = separated {
        return halves;
    }
    let split = text.match_indices(' ').map(|(i, _)| i).min_by_key(|i| {
        text[..*i]
            .chars()
            .count()
            .max(text[i + 1..].chars().count())
    });
    match split {
        Some(i) => (&text[..i], &text[i + 1..]),
        None => (text, ""),
    }
}

/// A tiny hard hat: Gus's avatar when his bubble cannot point at him.
fn hard_hat(at: Vec2, alpha: f32) {
    let yellow = fade(Color::from_rgba(250, 206, 64, 255), alpha);
    draw_circle(at.x, at.y, 8.0, yellow);
    draw_rectangle(at.x - 9.0, at.y, 18.0, 9.0, fade(PAPER, alpha));
    draw_rectangle(at.x - 11.0, at.y - 1.0, 22.0, 3.0, yellow);
    draw_rectangle(at.x - 1.0, at.y - 8.0, 2.0, 7.0, fade(INK, 0.35 * alpha));
}

fn bubble(view: &View) {
    let juice = &view.foundry.juice;
    let Some(bubble) = juice.bubble else {
        return;
    };
    let head = GUS + Vec3::Y * (1.95 * GUS_SCALE + juice.hop_height());
    let Some(anchor) = view.camera.project(view.w, view.h, head) else {
        return;
    };
    let alpha = bubble.alpha();
    let size = 16.0;
    // Narrow screens put Gus's line below the towers with his hard hat as
    // the speaker tag, instead of covering the range he is talking about.
    let compact = view.w < 560.0 || view.h < 500.0;
    let avatar = if compact { 26.0 } else { 0.0 };
    let max_w = (view.w - 56.0 - avatar).min(340.0);
    let (first, second) = wrap(bubble.text, max_w, size);
    let lines = if second.is_empty() { 1.0 } else { 2.0 };
    let text_w = text_width(first, size)
        .max(text_width(second, size))
        .min(max_w);
    let (bw, bh) = (text_w + 24.0 + avatar, lines * 22.0 + 14.0);
    let scene = view.layout.scene;
    let (x, y) = if compact {
        ((view.w - bw) * 0.5, scene.y + scene.h - bh - 4.0)
    } else {
        // Hang the bubble to Gus's right, over open floor, when there is room.
        (
            (anchor.x - 26.0).clamp(16.0, (view.w - 16.0 - bw).max(16.0)),
            (anchor.y - bh - 16.0).clamp(scene.y + 4.0, (scene.y + scene.h - bh).max(scene.y)),
        )
    };
    let fill = fade(PAPER, alpha);
    let ink = fade(INK, alpha);
    // Tail toward Gus's head when he is just below the bubble.
    if !compact && anchor.y > y + bh {
        let base = anchor.x.clamp(x + 14.0, x + bw - 14.0);
        let tip = vec2(anchor.x, anchor.y.min(y + bh + 16.0));
        draw_triangle(
            vec2(base - 9.0, y + bh - 1.0),
            vec2(base + 9.0, y + bh - 1.0),
            tip,
            fill,
        );
    }
    draw_rectangle(x + 2.0, y + 3.0, bw, bh, fade(SHADOW, alpha));
    draw_rectangle(x, y, bw, bh, fill);
    draw_rectangle_lines(x, y, bw, bh, 2.0, ink);
    if compact {
        hard_hat(vec2(x + 20.0, y + bh * 0.5 + 1.0), alpha);
    }
    for (i, line) in [first, second].into_iter().enumerate() {
        if !line.is_empty() {
            fit(
                line,
                x + 12.0 + avatar,
                y + 7.0 + 14.0 + i as f32 * 22.0,
                bw - 24.0 - avatar,
                size,
                ink,
                Align::Center,
            );
        }
    }
}

// --- Cards --------------------------------------------------------------------

enum Line {
    Text(String, f32, Color),
    Stars(u8, f32),
    Gap(f32),
}

impl Line {
    /// Small text wraps onto a second line rather than losing its punchline.
    fn wrapped(&self, inner: f32) -> bool {
        matches!(self, Line::Text(text, size, _) if text_width(text, size.min(16.0)) > inner)
    }

    fn height(&self, inner: f32) -> f32 {
        match self {
            Line::Text(..) if self.wrapped(inner) => 48.0,
            Line::Text(text, size, _) => {
                fitting_size(text, inner, &[*size, 24.0, 16.0]).min(*size) + 12.0
            }
            Line::Stars(_, radius) => radius * 2.0 + 14.0,
            Line::Gap(gap) => *gap,
        }
    }
}

/// Lines with a drop priority: 0 always stays; larger numbers drop first.
fn card(view: &View, mut lines: Vec<(Line, u8)>, accent: Color, reveal: f32) {
    let top = view.layout.scene.y + 4.0;
    let bottom = view.layout.panel.y - 8.0;
    let padding = 36.0;
    let width = (view.w - 32.0).min(500.0);
    let inner = width - 32.0;
    let total = |lines: &[(Line, u8)]| lines.iter().map(|(l, _)| l.height(inner)).sum::<f32>();
    while total(&lines) + padding > bottom - top {
        let Some(worst) = lines
            .iter()
            .enumerate()
            .filter(|(_, (_, p))| *p > 0)
            .max_by_key(|(i, (_, p))| (*p, *i))
            .map(|(i, _)| i)
        else {
            break;
        };
        lines.remove(worst);
    }
    let height = total(&lines) + padding;
    let x = (view.w - width) * 0.5;
    let slide = (1.0 - reveal) * 18.0 * view.foundry.juice.motion().min(1.0);
    let y = top + ((bottom - top - height) * 0.3).max(0.0) + slide;
    let shade = |color: Color| fade(color, reveal);
    draw_rectangle(x, y, width, height, shade(PANEL));
    draw_rectangle_lines(x, y, width, height, 1.0, shade(EDGE));
    draw_rectangle(x, y, width, 3.0, shade(accent));
    let mut cursor = y + 20.0;
    for (line, _) in &lines {
        match line {
            Line::Text(text, _, color) if line.wrapped(inner) => {
                let (first, second) = wrap(text, inner, 16.0);
                for (i, part) in [first, second].into_iter().enumerate() {
                    fit(
                        part,
                        x + 16.0,
                        cursor + 16.0 + i as f32 * 22.0,
                        inner,
                        16.0,
                        shade(*color),
                        Align::Center,
                    );
                }
            }
            Line::Text(text, size, color) => {
                let size = fitting_size(text, inner, &[*size, 24.0, 16.0]).min(*size);
                fit(
                    text,
                    x + 16.0,
                    cursor + size * 0.875 + 2.0,
                    inner,
                    size,
                    shade(*color),
                    Align::Center,
                );
            }
            Line::Stars(earned, radius) => {
                stars_row(
                    vec2(view.w * 0.5, cursor + radius + 6.0),
                    *radius,
                    *earned,
                    shade(GOLD),
                );
            }
            Line::Gap(_) => {}
        }
        cursor += line.height(inner);
    }
}

fn best_line(best: Best) -> String {
    match best.points {
        Some(points) => format!("BEST {}", grouped(points)),
        None => "NO RECORD YET. MAKE HISTORY.".into(),
    }
}

fn ready_card(view: &View) {
    card(
        view,
        vec![
            (Line::Text("SIEGE FOUNDRY".into(), 32.0, GOLD), 0),
            (
                Line::Text("TOPPLE ALL 6 RUST-RED TARGETS".into(), 16.0, PAPER),
                0,
            ),
            (
                Line::Text("8 SHELLS · FEWER SHOTS, MORE STARS".into(), 16.0, MUTED),
                2,
            ),
            (Line::Stars(view.best.stars, 13.0), 3),
            (Line::Text(best_line(view.best), 16.0, MUTED), 3),
            (Line::Gap(4.0), 4),
            (
                Line::Text(
                    prompt(view.hints, view.w < 480.0, "START", "TO START"),
                    16.0,
                    GOLD,
                ),
                1,
            ),
        ],
        GOLD,
        1.0,
    );
}

fn paused_card(view: &View) {
    let game = &view.foundry.range;
    let resume = if view.hints.touch {
        "PRESS RESUME BELOW".to_string()
    } else {
        format!("PRESS {} OR RESUME", view.hints.pause)
    };
    card(
        view,
        vec![
            (Line::Text("PAUSED".into(), 32.0, GOLD), 0),
            (
                Line::Text(view.foundry.banter.pause_line().into(), 16.0, MUTED),
                2,
            ),
            (
                Line::Text(
                    format!(
                        "TARGETS {}/{GOAL} · SCORE {}",
                        game.score(),
                        grouped(u64::from(game.tally.points))
                    ),
                    16.0,
                    PAPER,
                ),
                3,
            ),
            (Line::Text(resume, 16.0, GOLD), 1),
        ],
        lighten(STEEL, 0.3),
        1.0,
    );
}

/// The hook that makes "one more try" irresistible.
pub(super) fn next_goal(run: RunSummary, previous: Best, points: u64) -> String {
    if run.won {
        if let Some(shots) = score::next_star_shots(run.stars) {
            format!("NEXT STAR: CLEAR IT IN {shots} SHOTS")
        } else if !run.bell {
            "SECRET: SOMETHING UP HIGH GOES DING".into()
        } else if !run.domino {
            "NEXT: TOPPLE TWO TOWERS WITH ONE SHOT".into()
        } else {
            let best = previous.points.unwrap_or(0).max(points);
            format!("BEAT {} FOR A NEW BEST", grouped(best + 1))
        }
    } else {
        let left = GOAL as u32 - run.targets.min(GOAL as u32);
        if left <= 2 {
            format!(
                "SO CLOSE! {left} TARGET{} LEFT",
                if left == 1 { "" } else { "S" }
            )
        } else {
            "TIP: HIT THE LOW SANDY SUPPORTS".into()
        }
    }
}

fn results_card(view: &View, reveal: f32) {
    let foundry = view.foundry;
    let tally = &foundry.range.tally;
    let run = foundry.summary();
    let rank = quips::rank(run);
    let points = u64::from(tally.points);
    let results = view.results;
    let record = if results.new_best {
        ("NEW PERSONAL BEST!".to_string(), GOLD)
    } else if results.new_stars {
        ("NEW STAR RECORD!".to_string(), GOLD)
    } else {
        (best_line(results.previous), MUTED)
    };
    let stats = if run.won {
        format!(
            "{} SHOTS · BEST CHAIN ×{} · {} MISSES",
            tally.shots,
            tally.best_chain.max(1),
            tally.misses
        )
    } else {
        format!("{}/{GOAL} TARGETS · {} MISSES", run.targets, tally.misses)
    };
    let (title, accent) = if run.won {
        ("FOUNDRY CLEARED!", GOLD)
    } else {
        ("OUT OF SHELLS", RUST)
    };
    let action = if run.won {
        prompt(view.hints, view.w < 480.0, "AGAIN", "TO GO AGAIN")
    } else {
        prompt(view.hints, view.w < 480.0, "RETRY", "TO RETRY")
    };
    card(
        view,
        vec![
            (
                Line::Text(title.into(), 32.0, if run.won { GOLD } else { PAPER }),
                0,
            ),
            (Line::Stars(run.stars, 17.0), 0),
            (
                Line::Text(
                    grouped(points),
                    if view.w < 480.0 { 32.0 } else { 40.0 },
                    PAPER,
                ),
                0,
            ),
            (Line::Text(record.0, 16.0, record.1), 3),
            (Line::Text(stats, 16.0, MUTED), 4),
            (Line::Gap(4.0), 5),
            (Line::Text(rank.title.into(), 24.0, GOLD), 1),
            (Line::Text(rank.quip.into(), 16.0, PAPER), 3),
            (
                Line::Text(next_goal(run, results.previous, points), 16.0, GREEN),
                2,
            ),
            (Line::Gap(4.0), 5),
            (Line::Text(action, 16.0, GOLD), 0),
        ],
        accent,
        reveal,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grouping_and_next_goals_read_naturally() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(950), "950");
        assert_eq!(grouped(1250), "1,250");
        assert_eq!(grouped(1_234_567), "1,234,567");
        let won = RunSummary {
            won: true,
            stars: 2,
            targets: 6,
            ..Default::default()
        };
        assert_eq!(
            next_goal(won, Best::default(), 900),
            "NEXT STAR: CLEAR IT IN 3 SHOTS"
        );
        let three = RunSummary { stars: 3, ..won };
        assert!(next_goal(three, Best::default(), 900).contains("DING"));
        let all = RunSummary {
            bell: true,
            domino: true,
            ..three
        };
        let best = Best {
            points: Some(2000),
            stars: 3,
        };
        assert_eq!(next_goal(all, best, 1500), "BEAT 2,001 FOR A NEW BEST");
        let lost = RunSummary {
            targets: 5,
            ..Default::default()
        };
        assert_eq!(
            next_goal(lost, Best::default(), 0),
            "SO CLOSE! 1 TARGET LEFT"
        );
        assert!(next_goal(RunSummary::default(), Best::default(), 0).starts_with("TIP"));
    }

    #[test]
    fn bubble_wrap_balances_long_lines_and_keeps_short_ones() {
        assert_eq!(
            wrap("WIND, PROBABLY.", 300.0, 16.0),
            ("WIND, PROBABLY.", "")
        );
        let (a, b) = wrap("LOAD-BEARING TOWERS. ALLEGEDLY.", 264.0, 16.0);
        assert_eq!((a, b), ("LOAD-BEARING", "TOWERS. ALLEGEDLY."));
        assert_eq!(
            wrap("NOSPACESATALLHERE", 10.0, 16.0),
            ("NOSPACESATALLHERE", "")
        );
    }

    #[test]
    fn hud_copy_fits_its_boxes_at_narrow_and_desktop_widths() {
        let mut lines: Vec<String> = Vec::new();
        for callout in [
            Callout::Chain(5),
            Callout::Timber,
            Callout::Domino,
            Callout::Overkill,
            Callout::OneShotWonder,
            Callout::Delayed,
            Callout::Bell,
            Callout::Spare(7),
        ] {
            let (title, sub, _) = callout_text(callout, 1400);
            lines.push(title);
            lines.push(sub);
        }
        for width in [320.0_f32, 390.0, 1280.0, 1600.0] {
            let inner = (width - 32.0).min(500.0) - 32.0;
            for text in &lines {
                let size = fitting_size(text, width - 48.0, &[32.0, 24.0, 16.0]);
                assert!(text_width(text, size) <= width - 48.0, "{text} at {width}");
            }
            // Card lines fit at some size, or wrap onto two 16-pixel lines.
            let mut card_lines = vec![
                "FOUNDRY CLEARED!",
                "NEXT STAR: CLEAR IT IN 3 SHOTS",
                "8 SHELLS · FEWER SHOTS, MORE STARS",
                "NEXT: TOPPLE TWO TOWERS WITH ONE SHOT",
            ];
            for run in [
                RunSummary::default(),
                RunSummary {
                    targets: 3,
                    ..Default::default()
                },
                RunSummary {
                    won: true,
                    stars: 1,
                    ..Default::default()
                },
            ] {
                card_lines.push(quips::rank(run).title);
                card_lines.push(quips::rank(run).quip);
            }
            for text in card_lines {
                let line = Line::Text(text.into(), 24.0, PAPER);
                if line.wrapped(inner) {
                    let (a, b) = wrap(text, inner, 16.0);
                    assert!(
                        text_width(a, 16.0) <= inner && text_width(b, 16.0) <= inner,
                        "{text} at {width}"
                    );
                } else {
                    let size = fitting_size(text, inner, &[32.0, 24.0, 16.0]);
                    assert!(text_width(text, size) <= inner, "{text} at {width}");
                }
            }
        }
        // Bubble lines wrap into two lines that fit a 320-pixel screen.
        for line in quips::all_lines() {
            let (a, b) = wrap(line, 264.0, 16.0);
            assert!(
                text_width(a, 16.0) <= 264.0 && text_width(b, 16.0) <= 264.0,
                "{line}"
            );
        }
    }

    #[test]
    fn fullscreen_menu_corner_stays_clear_of_hud_text_and_panels() {
        for (w, h) in [
            (1280.0, 800.0),
            (1600.0, 1000.0),
            (390.0, 844.0),
            (844.0, 390.0),
            (320.0, 480.0),
        ] {
            let corner = Rect::new(w - MENU_CORNER.x, 0.0, MENU_CORNER.x, MENU_CORNER.y);
            let clear = |r: Rect, what: &str| {
                let overlaps = r.x < corner.x + corner.w
                    && r.x + r.w > corner.x
                    && r.y < corner.y + corner.h
                    && r.y + r.h > corner.y;
                assert!(!overlaps, "{what} {r:?} enters the menu corner at {w}x{h}");
            };
            // Worst-case copy: every target down and a four-digit score (every
            // bonus at once stays well under 10,000 points).
            let bar = TopBar::new(w, "6/6", "9,999");
            assert!(bar.compact || bar.right.is_some(), "score shown at {w}x{h}");
            for rect in bar.rects() {
                clear(rect, "top bar");
            }
            // The score must still fit its compact panel.
            if bar.compact {
                let pips = 14.0 + 6.0 * bar.pip + 5.0 * bar.gap + 5.0 + text_width("6/6", 16.0);
                assert!(bar.left.w - 12.0 - pips - 8.0 >= text_width("9,999", 16.0));
            }
            // Cards, banners and Gus's bubble all start at or below the scene.
            let layout = HudLayout::new(w, h);
            assert!(layout.scene.y + 4.0 >= MENU_CORNER.y, "cards at {w}x{h}");
            for rect in [layout.fire, layout.rebuild, layout.pause, layout.panel] {
                clear(rect, "bottom panel");
            }
        }
    }
}
