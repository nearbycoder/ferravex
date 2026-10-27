//! HUD, Foreman Gus's speech bubble, the title screen and the Paused and
//! Results cards, drawn crisp above the post-processed world.
//!
//! Type: the Modern face (mixed case) carries copy, hints, quips and card
//! bodies; the Classic face keeps its arcade punch for the logo, callout
//! banners, score pops and big numbers. Panels are anti-aliased rounded
//! rectangles with soft shadows and a warm top-lit gradient.
//!
//! Motion: cards fade up line by line, buttons ease on hover and press, the
//! results score rolls up and stars pop in, the logo shimmers and the foundry
//! doors swing open on start and rebuild. All of it follows [`HudAnim`], a
//! render-time presentation clock; reduced motion keeps fades but removes
//! movement, scaling and swinging, and reduced flash keeps glows steady.
//!
//! Allocation: formatting goes through a small reusable string pool and card
//! rows are laid out by calling their builder twice, so steady-state frames do
//! not allocate.
use super::juice::PopText;
use super::quips::{self, RunSummary};
use super::score::{self, Callout};
use super::*;
use ferravex_render::shapes::{self, Fill, Shadow, Stroke};
use ferravex_render::text::{self, TextStyle};
use std::cell::RefCell;
use std::fmt::{self, Write as _};

pub(super) const GOLD: Color = Color::from_rgba(250, 200, 92, 255);
const EMBER: Color = Color::from_rgba(236, 118, 52, 255);
const MUTED: Color = Color::from_rgba(186, 168, 150, 255);
const GREEN: Color = Color::from_rgba(164, 206, 126, 255);
const PANEL_TOP: Color = Color::new(0.16, 0.12, 0.11, 0.95);
const PANEL_BOTTOM: Color = Color::new(0.09, 0.07, 0.07, 0.96);
const EDGE: Color = Color::new(0.42, 0.29, 0.22, 0.9);
const SHADOW: Color = Color::new(0.0, 0.0, 0.0, 0.5);
const OUTLINE: Color = Color::from_rgba(28, 14, 10, 255);
const RADIUS: f32 = 10.0;
const CARD_RADIUS: f32 = 16.0;

/// Stored records shown on the title screen and Results card.
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
            let hint = action_hint(input, action, &mut buffer);
            if slot != hint {
                slot.clear();
                slot.push_str(hint);
            }
        }
        self.touch = input.current_device() == HintDevice::Touch;
    }
}

/// Seconds the foundry doors take to swing open on start and rebuild.
const DOORS: f32 = 0.6;

/// Render-time presentation state for the HUD: never touches the simulation.
#[derive(Clone, Copy, Debug)]
pub(super) struct HudAnim {
    /// Render seconds, always advancing (drives fades).
    pub time: f32,
    /// 0..=1 movement scale; 0 under reduced motion.
    pub motion: f32,
    /// False under reduced flash: pulses and shimmer hold steady.
    pub pulse: bool,
    state: Option<SessionState>,
    /// Render seconds since the session state last changed.
    pub state_age: f32,
    /// Render seconds since the doors started opening.
    pub doors: f32,
    hover: [f32; 3],
    press: [f32; 3],
}

impl Default for HudAnim {
    fn default() -> Self {
        Self {
            time: 0.0,
            motion: 1.0,
            pulse: true,
            state: None,
            state_age: 0.0,
            // The doors open over the title screen on boot.
            doors: 0.0,
            hover: [0.0; 3],
            press: [0.0; 3],
        }
    }
}

impl HudAnim {
    pub fn update(&mut self, dt: f32, state: SessionState, layout: &HudLayout, enabled: [bool; 3]) {
        let policy = preferences::feedback_policy();
        self.motion = if policy.reduced_motion() {
            0.0
        } else {
            policy.intensity().min(1.0)
        };
        self.pulse = !policy.reduced_flash();
        self.time += dt;
        self.doors += dt;
        if self.state != Some(state) {
            self.state = Some(state);
            self.state_age = 0.0;
        } else {
            self.state_age += dt;
        }
        let mouse = Vec2::from(mouse_position());
        let down = is_mouse_button_down(MouseButton::Left);
        let rate = if self.motion > 0.0 {
            (dt * 16.0).min(1.0)
        } else {
            1.0
        };
        for (i, rect) in [layout.fire, layout.rebuild, layout.pause]
            .iter()
            .enumerate()
        {
            let over = enabled[i] && rect.contains(mouse);
            let hover = if over { 1.0 } else { 0.0 };
            let press = if over && down { 1.0 } else { 0.0 };
            self.hover[i] += (hover - self.hover[i]) * rate;
            self.press[i] += (press - self.press[i]) * rate;
        }
    }

    /// Start (or restart) the door reveal.
    pub fn open_doors(&mut self) {
        self.doors = 0.0;
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
    pub anim: &'a HudAnim,
}

impl View<'_> {
    fn compact(&self) -> bool {
        self.w < 560.0 || self.h < 500.0
    }
}

pub(super) fn draw(view: &View) {
    let state = view.foundry.session.state();
    if state == SessionState::Playing && view.foundry.range.phase == Phase::Aiming {
        aim_preview(view);
    }
    pops(view);
    if state == SessionState::Playing {
        bubble(view);
    }
    top_bar(view, state);
    bottom_panel(view, state);
    match state {
        SessionState::Playing => banner(view),
        SessionState::Ready => title_screen(view),
        SessionState::Paused => paused_card(view),
        SessionState::Won | SessionState::Lost => {
            let reveal = view.foundry.juice.card_reveal();
            if reveal > 0.0 {
                results_card(view);
            } else {
                banner(view);
            }
        }
    }
    doors(view);
}

// --- Text, formatting and shape helpers --------------------------------------

thread_local! {
    static POOL: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Format into a pooled string and hand it to `f`; nests freely and stops
/// allocating once the pool has warmed up.
fn with_fmt<R>(args: fmt::Arguments, f: impl FnOnce(&str) -> R) -> R {
    let mut text = POOL
        .with(|pool| pool.borrow_mut().pop())
        .unwrap_or_else(|| String::with_capacity(64));
    text.clear();
    let _ = text.write_fmt(args);
    let result = f(&text);
    POOL.with(|pool| pool.borrow_mut().push(text));
    result
}

/// Thousands separators without allocating: `Grouped(12345)` shows "12,345".
#[derive(Clone, Copy)]
pub(super) struct Grouped(pub u64);

impl fmt::Display for Grouped {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut digits = [0_u8; 20];
        let mut n = self.0;
        let mut len = 0;
        loop {
            digits[len] = b'0' + (n % 10) as u8;
            len += 1;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        for i in (0..len).rev() {
            f.write_char(char::from(digits[i]))?;
            if i > 0 && i % 3 == 0 {
                f.write_char(',')?;
            }
        }
        Ok(())
    }
}

/// Body copy: the Modern face with a soft drop shadow.
pub(super) fn body(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color).with_shadow(Color::new(0.0, 0.0, 0.0, 0.55 * color.a))
}

/// Arcade display type: the Classic face with a dark outline.
pub(super) fn display(size: f32, color: Color) -> TextStyle {
    TextStyle::classic(size, color).with_outline(fade(OUTLINE, color.a))
}

pub(super) fn width(text: &str, style: TextStyle) -> f32 {
    text::text_width(text, style)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    Left,
    Center,
    Right,
}

/// Draw one line with its baseline at `y`, ellipsized to `max` width.
fn line(text: &str, x: f32, y: f32, max: f32, style: TextStyle, align: Align) {
    let full = width(text, style);
    let (shown, ellipsis) = if full <= max {
        (text, false)
    } else {
        let room = max - width("…", style);
        let mut end = 0;
        for (i, c) in text.char_indices() {
            let next = i + c.len_utf8();
            if width(&text[..next], style) > room {
                break;
            }
            end = next;
        }
        (text[..end].trim_end(), true)
    };
    let used = width(shown, style) + if ellipsis { width("…", style) } else { 0.0 };
    let left = match align {
        Align::Left => x,
        Align::Center => x + (max - used) * 0.5,
        Align::Right => x + max - used,
    };
    text::draw_text_styled(shown, left, y, style);
    if ellipsis {
        text::draw_text_styled("…", left + width(shown, style), y, style);
    }
}

/// Up to two balanced lines for `text` at `max` width.
pub(super) fn wrap(text: &str, max: f32, style: TextStyle) -> (&str, &str) {
    if width(text, style) <= max {
        return (text, "");
    }
    let separated = text
        .match_indices(" · ")
        .map(|(i, sep)| (&text[..i], &text[i + sep.len()..]))
        .filter(|(a, b)| width(a, style) <= max && width(b, style) <= max)
        .min_by(|a, b| {
            let wa = width(a.0, style).max(width(a.1, style));
            let wb = width(b.0, style).max(width(b.1, style));
            wa.total_cmp(&wb)
        });
    if let Some(halves) = separated {
        return halves;
    }
    text.match_indices(' ')
        .map(|(i, _)| (&text[..i], &text[i + 1..]))
        .min_by(|a, b| {
            let wa = width(a.0, style).max(width(a.1, style));
            let wb = width(b.0, style).max(width(b.1, style));
            wa.total_cmp(&wb)
        })
        .unwrap_or((text, ""))
}

fn fade(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0.0, 1.0))
}

fn lighten(color: Color, amount: f32) -> Color {
    shapes::mix(color, Color::new(1.0, 1.0, 1.0, color.a), amount)
}

fn cubic_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    t * t * t + 1.0
}

fn back_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    1.0 + 2.70158 * t * t * t + 1.70158 * t * t
}

/// Staggered fade-up for row `index` of a screen that appeared `age` ago:
/// (alpha, y offset). Reduced motion keeps the fade and drops the slide.
fn entrance(age: f32, index: usize, motion: f32) -> (f32, f32) {
    let t = ((age - index as f32 * 0.05) / 0.32).clamp(0.0, 1.0);
    let e = cubic_out(t);
    (e, (1.0 - e) * 14.0 * motion)
}

/// A rounded, top-lit panel with a soft shadow and a warm hairline.
fn panel(rect: Rect, radius: f32, accent: Option<Color>, alpha: f32) {
    shapes::drop_shadow(
        rect,
        radius,
        Shadow::soft(fade(SHADOW, alpha))
            .offset(vec2(0.0, 6.0))
            .blur(20.0),
    );
    shapes::fill_rounded_rect(
        rect,
        radius,
        Fill::vertical(rect, fade(PANEL_TOP, alpha), fade(PANEL_BOTTOM, alpha)),
    );
    shapes::stroke_rounded_rect(rect, radius, 1.0, fade(EDGE, alpha));
    if let Some(accent) = accent {
        let bar = Rect::new(rect.x + radius, rect.y + 1.0, rect.w - 2.0 * radius, 2.0);
        shapes::fill_rounded_rect(bar, 1.0, fade(accent, alpha));
    }
}

fn star_points(center: Vec2, radius: f32) -> [Vec2; 10] {
    std::array::from_fn(|i| {
        let r = if i.is_multiple_of(2) {
            radius
        } else {
            radius * 0.45
        };
        let angle = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
        center + vec2(angle.cos(), angle.sin()) * r
    })
}

/// A five-point star; `filled` stars use `color`, others are outlines.
fn star(center: Vec2, radius: f32, filled: bool, color: Color) {
    let points = star_points(center, radius);
    if filled {
        for i in 0..10 {
            draw_triangle(center, points[i], points[(i + 1) % 10], color);
        }
    }
    shapes::polyline(
        &points,
        Stroke::new((radius * 0.12).max(1.5), color)
            .join(shapes::Join::Round)
            .closed(true),
    );
}

/// Three stars; `age` drives a staggered pop for earned stars on results.
fn stars_row(center: Vec2, radius: f32, earned: u8, alpha: f32, age: Option<f32>, motion: f32) {
    let spacing = radius * 2.6;
    for i in 0..3_u8 {
        let at = center + vec2((f32::from(i) - 1.0) * spacing, 0.0);
        let filled = i < earned;
        let mut scale = 1.0;
        let mut a = alpha;
        if let (Some(age), true) = (age, filled) {
            let t = (age - 0.25 - f32::from(i) * 0.18) / 0.35;
            a *= t.clamp(0.0, 1.0);
            scale = 1.0 + (back_out(t) - 1.0) * motion;
            if t > 0.0 {
                shapes::glow_disc(at, radius * 2.2, fade(GOLD, 0.35 * a));
            }
        }
        if filled {
            star(at + vec2(1.5, 2.0), radius * scale, true, fade(SHADOW, a));
            star(at, radius * scale, true, fade(GOLD, a));
        } else {
            star(at, radius, false, fade(MUTED, 0.6 * alpha));
        }
    }
}

/// A key cap with its label, returning its width.
fn keycap(x: f32, baseline: f32, label: &str, alpha: f32) -> f32 {
    let style = body(16.0, fade(Color::from_rgba(40, 28, 24, 255), alpha))
        .with_shadow(Color::new(1.0, 1.0, 1.0, 0.0));
    let w = width(label, style) + 20.0;
    let rect = Rect::new(x, baseline - 22.0, w, 30.0);
    shapes::fill_rounded_rect(
        Rect::new(rect.x, rect.y + 3.0, rect.w, rect.h),
        6.0,
        fade(Color::from_rgba(120, 84, 60, 255), alpha),
    );
    shapes::fill_rounded_rect(
        rect,
        6.0,
        Fill::vertical(
            rect,
            fade(Color::from_rgba(252, 236, 206, 255), alpha),
            fade(Color::from_rgba(226, 200, 164, 255), alpha),
        ),
    );
    text::draw_text_styled(label, x + 10.0, baseline, style);
    w
}

/// "Press [Enter] to start", or a touch fallback, centered on `cx`.
fn prompt(view: &View, cx: f32, baseline: f32, button: &str, verb: &str, alpha: f32) {
    let style = body(16.0, fade(PAPER, alpha));
    if view.hints.touch {
        with_fmt(format_args!("Press {button} below"), |t| {
            let w = width(t, style);
            text::draw_text_styled(t, cx - w * 0.5, baseline, style);
        });
        return;
    }
    let max_cap = (view.w - 64.0) * 0.5;
    let key = view.hints.confirm.as_str();
    let cap_w = width(key, body(16.0, PAPER)).min(max_cap) + 20.0;
    let lead = "Press";
    let gap = 10.0;
    let total = width(lead, style) + gap + cap_w + gap + width(verb, style);
    let mut x = cx - total * 0.5;
    text::draw_text_styled(lead, x, baseline, style);
    x += width(lead, style) + gap;
    if width(key, body(16.0, PAPER)) > max_cap {
        // Very long remapped bindings fall back to a plain fitted line.
        line(
            key,
            x,
            baseline,
            max_cap,
            body(16.0, fade(GOLD, alpha)),
            Align::Left,
        );
        x += max_cap + 20.0 + gap;
    } else {
        x += keycap(x, baseline, key, alpha) + gap;
    }
    text::draw_text_styled(verb, x, baseline, style);
}

// --- In-world HUD (crisp, above the bloom) -----------------------------------

/// Dotted arc plus a reticle that says what the ball will hit first.
fn aim_preview(view: &View) {
    let game = &view.foundry.range;
    if game.ammo == 0 || game.cooldown > 0.0 {
        return;
    }
    let project = |p: Vec3| view.camera.project(view.w, view.h, p);
    let count = game.preview.len().max(1) as f32;
    for (i, point) in game.preview.iter().enumerate().skip(1) {
        if let Some(at) = project(*point) {
            let r = if i % 3 == 0 { 3.2 } else { 2.2 };
            let a = 1.0 - 0.45 * i as f32 / count;
            shapes::fill_circle(at + vec2(1.0, 1.5), r + 0.8, fade(SHADOW, a));
            shapes::fill_circle(at, r, Color::new(1.0, 0.94, 0.8, 0.95 * a));
        }
    }
    let Some(hit) = game.preview_hit else {
        return;
    };
    let Some(at) = project(hit) else {
        return;
    };
    let (color, tag) = match game.block_at(hit) {
        Some(block) if block.target && !block.down => (GOLD, "Target"),
        Some(block) if !block.target => (lighten(SAND, 0.35), "Support"),
        _ => (PAPER, ""),
    };
    let pulse = if view.anim.pulse {
        (view.anim.time * 5.0).sin() * 0.5 + 0.5
    } else {
        0.5
    };
    shapes::glow_ring(at, 13.0, 8.0, fade(color, 0.25 + 0.2 * pulse));
    shapes::stroke_circle(at, 12.0, 4.0, SHADOW);
    shapes::stroke_circle(at, 12.0, 2.5, color);
    for dir in [
        vec2(1.0, 0.0),
        vec2(-1.0, 0.0),
        vec2(0.0, 1.0),
        vec2(0.0, -1.0),
    ] {
        shapes::polyline(
            &[at + dir * 7.0, at + dir * 17.0],
            Stroke::new(2.0, color).cap(shapes::Cap::Round),
        );
    }
    // The tag teaches what to hit; it fades away once something has fallen.
    if !view.coach.knocked && !tag.is_empty() {
        let style = body(16.0, color);
        let w = width(tag, style);
        let x = (at.x - w * 0.5).clamp(16.0, view.w - w - 16.0);
        let pill = Rect::new(x - 10.0, at.y + 22.0, w + 20.0, 28.0);
        shapes::fill_rounded_rect(pill, 14.0, Color::new(0.08, 0.06, 0.06, 0.85));
        text::draw_text_styled(tag, x, style.centered_baseline(pill.y + 14.0), style);
    }
}

/// Rising score and wobble pops in Classic display type.
fn pops(view: &View) {
    let juice = &view.foundry.juice;
    if juice.card_reveal() > 0.0 {
        return;
    }
    for pop in juice.pops.iter().flatten() {
        let Some(at) = view
            .camera
            .project(view.w, view.h, pop.at + Vec3::Y * juice.pop_rise(pop))
        else {
            continue;
        };
        let alpha = pop.alpha();
        let draw = |text: &str, style: TextStyle| {
            let w = width(text, style);
            let x = (at.x - w * 0.5).clamp(8.0, (view.w - w - 8.0).max(8.0));
            text::draw_text_styled(text, x, at.y, style);
        };
        match pop.text {
            PopText::Points { points, chain } if chain > 1 => {
                with_fmt(format_args!("+{points} ×{chain}"), |t| {
                    draw(t, display(24.0, fade(GOLD, alpha)))
                })
            }
            PopText::Points { points, .. } => with_fmt(format_args!("+{points}"), |t| {
                draw(t, display(24.0, fade(GOLD, alpha)))
            }),
            PopText::Word(word) => draw(word, display(16.0, fade(PAPER, alpha))),
        }
    }
}

// --- Play HUD ----------------------------------------------------------------

/// Browser fullscreen pins a 44-pixel menu button inset 8 pixels into the
/// canvas's top-right corner; HUD text and panels keep this square clear.
pub(super) const MENU_CORNER: Vec2 = vec2(72.0, 60.0);

fn count_style() -> TextStyle {
    display(16.0, PAPER)
}

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
        let caption = body(16.0, MUTED);
        let heading_w = if compact {
            0.0
        } else {
            width("Targets", caption) + 12.0
        };
        let pips = 6.0 * pip + 5.0 * gap;
        let targets_w = 14.0 + heading_w + pips + 10.0 + width(count, count_style()) + 14.0;
        let value_w = width(value, count_style()).max(48.0);
        if compact {
            let wide = targets_w + 8.0 + value_w + 14.0;
            return Self {
                compact,
                pip,
                gap,
                left: Rect::new(
                    16.0,
                    12.0,
                    wide.min(w - 16.0 - MENU_CORNER.x).max(1.0),
                    32.0,
                ),
                right: None,
                pace_w: 0.0,
            };
        }
        let pace_w = 3.0 * 17.0 + 10.0;
        let right_w = 14.0 + pace_w + width("Score", caption) + 10.0 + value_w + 14.0;
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
    // The counter rolls during play; a frozen result shows the exact total.
    let points = if state == SessionState::Playing {
        juice.shown_points.round() as u64
    } else {
        u64::from(game.tally.points)
    };
    let rolling = points < u64::from(game.tally.points);
    with_fmt(format_args!("{}/{GOAL}", game.score()), |count| {
        with_fmt(format_args!("{}", Grouped(points)), |value| {
            let bar = TopBar::new(view.w, count, value);
            draw_top_bar(view, state, &bar, count, value, rolling);
        })
    });
}

fn draw_top_bar(
    view: &View,
    state: SessionState,
    bar: &TopBar,
    count: &str,
    value: &str,
    rolling: bool,
) {
    let game = &view.foundry.range;
    let (pip, gap) = (bar.pip, bar.gap);
    let caption = body(16.0, MUTED);
    panel(bar.left, RADIUS, None, 1.0);
    let mid = bar.left.y + bar.left.h * 0.5;
    let mut x = bar.left.x + 14.0;
    if !bar.compact {
        text::draw_text_styled("Targets", x, caption.centered_baseline(mid), caption);
        x += width("Targets", caption) + 12.0;
    }
    for block in game.blocks.iter().filter(|block| block.target) {
        let rect = Rect::new(x, mid - pip * 0.5, pip, pip);
        if block.down {
            shapes::fill_rounded_rect(rect, 3.0, GREEN);
            let s = pip / 14.0;
            shapes::polyline(
                &[
                    vec2(x + 3.0 * s, mid),
                    vec2(x + 6.0 * s, mid + 3.0 * s),
                    vec2(x + 11.0 * s, mid - 4.0 * s),
                ],
                Stroke::new(2.0, INK).cap(shapes::Cap::Round),
            );
        } else {
            shapes::fill_rounded_rect(rect, 3.0, Fill::vertical(rect, lighten(RUST, 0.15), RUST));
            shapes::fill_circle(vec2(x + pip * 0.5, mid), 2.0, PAPER);
        }
        x += pip + gap;
    }
    let numbers = count_style();
    text::draw_text_styled(count, x + 5.0, numbers.centered_baseline(mid), numbers);
    let score = display(16.0, if rolling { GOLD } else { PAPER });
    if bar.compact {
        let start = x + 5.0 + width(count, numbers) + 8.0;
        let right = bar.left.x + bar.left.w - 14.0;
        line(
            value,
            start,
            score.centered_baseline(mid),
            right - start,
            score,
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
    panel(right, RADIUS, None, 1.0);
    for i in 0..3 {
        star(
            vec2(right.x + 22.0 + i as f32 * 17.0, mid),
            7.0,
            i < pace,
            if i < pace { GOLD } else { fade(MUTED, 0.6) },
        );
    }
    let x = right.x + 14.0 + bar.pace_w;
    text::draw_text_styled("Score", x, caption.centered_baseline(mid), caption);
    let start = x + width("Score", caption) + 10.0;
    line(
        value,
        start,
        score.centered_baseline(mid),
        right.x + right.w - 14.0 - start,
        score,
        Align::Right,
    );
}

/// The onboarding line for a first-time player, or `None` once learned.
fn coach_hint(view: &View, out: &mut dyn FnMut(&str)) -> bool {
    let coach = view.coach;
    let tally = &view.foundry.range.tally;
    if !coach.aimed {
        if view.hints.touch {
            out("Drag to aim · press Fire to shoot");
        } else {
            with_fmt(
                format_args!("Aim with the move keys or drag · Fire: {}", view.hints.fire),
                |t| out(t),
            );
        }
        true
    } else if !coach.fired {
        out("Follow the dotted arc, then fire");
        true
    } else if !coach.knocked && tally.misses > 0 {
        out("Tip: knock out a low sandstone support");
        true
    } else {
        false
    }
}

/// The status line under the shells: events first, then coaching.
fn status(view: &View, state: SessionState, out: &mut dyn FnMut(&str, Color)) {
    let game = &view.foundry.range;
    let feedback = &view.foundry.feedback;
    match state {
        SessionState::Ready if view.compact() => out("Gus looks nervous.", MUTED),
        SessionState::Ready => out("Foreman Gus is watching. Nervously.", MUTED),
        SessionState::Won => out("Foundry cleared!", GOLD),
        SessionState::Lost => out("Out of shells", PAPER),
        SessionState::Paused => out("Range paused", MUTED),
        SessionState::Playing => {
            if feedback.destruction > 0.0 {
                let n = feedback.knocked_down;
                with_fmt(
                    format_args!("+{n} target{} down", if n == 1 { "" } else { "s" }),
                    |t| out(t, GOLD),
                );
            } else if feedback.shot > 0.0 {
                with_fmt(format_args!("Shot {} away", AMMO - game.ammo), |t| {
                    out(t, PAPER)
                });
            } else if feedback.rebuilt > 0.0 {
                out("Rebuilt: 8 shells ready", GREEN);
            } else if game.ammo == 0 {
                with_fmt(
                    format_args!(
                        "Last shell settling… {:.1}s",
                        (SETTLE - game.since_shot).max(0.0)
                    ),
                    |t| out(t, GOLD),
                );
            } else if game.cooldown > 0.0 {
                with_fmt(format_args!("Reloading {:.1}s", game.cooldown), |t| {
                    out(t, PAPER)
                });
            } else {
                coach_hint(view, &mut |t| out(t, MUTED));
            }
        }
    }
}

fn button(view: &View, index: usize, rect: Rect, title: &str, enabled: bool, primary: bool) {
    let anim = view.anim;
    let hover = anim.hover[index];
    let press = anim.press[index];
    let lift = (hover * 2.0 - press * 2.0) * anim.motion;
    let inset = press * 1.5 * anim.motion;
    let rect = Rect::new(
        rect.x + inset,
        rect.y - lift + inset,
        rect.w - inset * 2.0,
        rect.h - inset * 2.0,
    );
    let (top, bottom) = match (enabled, primary) {
        (false, _) => (
            Color::new(0.16, 0.13, 0.12, 0.9),
            Color::new(0.13, 0.11, 0.1, 0.9),
        ),
        (true, true) => (
            Color::from_rgba(232, 118, 54, 255),
            Color::from_rgba(170, 64, 30, 255),
        ),
        (true, false) => (
            Color::new(0.24, 0.19, 0.17, 1.0),
            Color::new(0.17, 0.13, 0.12, 1.0),
        ),
    };
    if enabled && primary {
        // Ready-to-fire glow, steady under reduced flash.
        let pulse = if anim.pulse {
            0.5 + 0.5 * (anim.time * 3.0).sin()
        } else {
            0.5
        };
        shapes::glow_disc(
            vec2(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5),
            rect.w * 0.45,
            fade(EMBER, 0.12 + 0.06 * pulse + 0.1 * hover),
        );
    }
    shapes::drop_shadow(
        rect,
        RADIUS,
        Shadow::soft(Color::new(0.0, 0.0, 0.0, 0.45))
            .offset(vec2(0.0, 3.0 + lift))
            .blur(10.0),
    );
    shapes::fill_rounded_rect(
        rect,
        RADIUS,
        Fill::vertical(
            rect,
            lighten(top, 0.1 * hover),
            lighten(bottom, 0.1 * hover),
        ),
    );
    let edge = if enabled && primary {
        Color::from_rgba(255, 170, 110, 200)
    } else {
        EDGE
    };
    shapes::stroke_rounded_rect(rect, RADIUS, 1.0, edge);
    let style = body(16.0, if enabled { PAPER } else { fade(MUTED, 0.6) });
    let size = if width(title, style) > rect.w - 16.0 {
        body(8.0, style.color)
    } else {
        style
    };
    line(
        title,
        rect.x + 8.0,
        size.centered_baseline(rect.y + rect.h * 0.5),
        rect.w - 16.0,
        size,
        Align::Center,
    );
}

fn bottom_panel(view: &View, state: SessionState) {
    let layout = view.layout;
    let game = &view.foundry.range;
    let p = layout.panel;
    panel(p, RADIUS, Some(fade(EMBER, 0.8)), 1.0);
    // Shells: iron balls; the next one in the breech glows when loaded.
    for i in 0..AMMO {
        let at = vec2(p.x + 24.0 + f32::from(i) * 19.0, p.y + 22.0);
        if i < game.ammo {
            if i + 1 == game.ammo && game.can_fire() && state == SessionState::Playing {
                shapes::glow_disc(at, 14.0, fade(EMBER, 0.45));
            }
            shapes::fill_circle(at + vec2(0.5, 1.5), 7.0, SHADOW);
            shapes::fill_circle(
                at,
                7.0,
                Fill::radial(
                    at - vec2(2.0, 2.5),
                    9.0,
                    Color::from_rgba(196, 180, 164, 255),
                    Color::from_rgba(70, 60, 56, 255),
                ),
            );
        } else {
            shapes::stroke_circle(at, 6.0, 1.5, fade(MUTED, 0.45));
        }
    }
    let caption = body(16.0, MUTED);
    let shells_right = p.x + 24.0 + f32::from(AMMO) * 19.0;
    let row = caption.centered_baseline(p.y + 22.0);
    let left_w = with_fmt(format_args!("{} left", game.ammo), |t| {
        text::draw_text_styled(t, shells_right, row, caption);
        width(t, caption)
    });
    if state != SessionState::Ready {
        with_fmt(
            format_args!(
                "Yaw {:+.0}°   Elev {:.0}°",
                game.yaw.to_degrees(),
                game.elevation.to_degrees()
            ),
            |t| {
                let w = width(t, caption);
                if p.x + p.w - 16.0 - w >= shells_right + left_w + 24.0 {
                    text::draw_text_styled(t, p.x + p.w - 16.0 - w, row, caption);
                }
            },
        );
    }
    status(view, state, &mut |t, color| {
        line(
            t,
            p.x + 16.0,
            p.y + 52.0,
            p.w - 32.0,
            body(16.0, color),
            Align::Left,
        )
    });

    let paused = view.foundry.session.is_paused();
    let fire_title = match state {
        SessionState::Ready => "Start",
        SessionState::Won => "Again",
        SessionState::Lost => "Retry",
        SessionState::Paused => "Paused",
        SessionState::Playing if game.ammo == 0 => "Empty",
        SessionState::Playing if game.cooldown > 0.0 => "Loading",
        SessionState::Playing => "Fire",
    };
    let [fire_enabled, rebuild_enabled, pause_enabled] = button_enabled(view, state);
    button(view, 0, layout.fire, fire_title, fire_enabled, true);
    button(view, 1, layout.rebuild, "Rebuild", rebuild_enabled, false);
    button(
        view,
        2,
        layout.pause,
        if paused { "Resume" } else { "Pause" },
        pause_enabled,
        paused,
    );
    if game.cooldown > 0.0 {
        let bar = Rect::new(
            layout.fire.x + 6.0,
            layout.fire.y + layout.fire.h + 4.0,
            layout.fire.w - 12.0,
            4.0,
        );
        shapes::fill_rounded_rect(bar, 2.0, Color::new(0.0, 0.0, 0.0, 0.45));
        let filled = Rect::new(bar.x, bar.y, bar.w * (1.0 - game.cooldown / RELOAD), bar.h);
        shapes::fill_rounded_rect(filled, 2.0, Fill::horizontal(filled, EMBER, GOLD));
    }
}

/// Fire, Rebuild and Pause availability (also drives hover easing).
pub(super) fn button_enabled(view: &View, state: SessionState) -> [bool; 3] {
    enabled_buttons(view.foundry, state, view.controls_open)
}

pub(super) fn enabled_buttons(
    foundry: &Foundry,
    state: SessionState,
    controls_open: bool,
) -> [bool; 3] {
    let fire = !controls_open
        && match state {
            SessionState::Ready | SessionState::Won | SessionState::Lost => true,
            SessionState::Playing => foundry.range.can_fire(),
            SessionState::Paused => false,
        };
    [
        fire,
        !controls_open,
        !controls_open && matches!(state, SessionState::Playing | SessionState::Paused),
    ]
}

/// Banner title (Classic caps), subtitle (Modern) and accent color.
fn callout_text(callout: Callout, points: u32, out: &mut dyn FnMut(&str, &str, Color)) {
    match callout {
        Callout::Chain(n) => with_fmt(format_args!("CHAIN ×{n}!"), |t| {
            with_fmt(format_args!("+{points}"), |s| out(t, s, PAPER))
        }),
        Callout::Timber => with_fmt(format_args!("Tower down · +{points}"), |s| {
            out("TIMBER!", s, Color::from_rgba(255, 150, 70, 255))
        }),
        Callout::Domino => with_fmt(format_args!("Two towers, one shot · +{points}"), |s| {
            out("DOMINO EFFECT!", s, GOLD)
        }),
        Callout::Overkill => with_fmt(format_args!("Kicking rubble · +{points}"), |s| {
            out("OVERKILL!", s, GOLD)
        }),
        Callout::OneShotWonder => with_fmt(format_args!("Four down · +{points}"), |s| {
            out("ONE-SHOT WONDER!", s, GOLD)
        }),
        Callout::Delayed => out("WAIT... IT COUNTS!", "Slow-motion demolition", GREEN),
        Callout::Bell => with_fmt(format_args!("Secret lunch bell · +{points}"), |s| {
            out("DING!", s, GOLD)
        }),
        Callout::Spare(n) => with_fmt(format_args!("SPARE SHELLS ×{n}"), |t| {
            with_fmt(format_args!("Unused shells pay +{points}"), |s| {
                out(t, s, GOLD)
            })
        }),
    }
}

fn banner(view: &View) {
    let juice = &view.foundry.juice;
    let Some(banner) = juice.banner else {
        return;
    };
    let alpha = banner.alpha();
    let compact = view.compact();
    callout_text(banner.callout, banner.points, &mut |title, sub, color| {
        let max = view.w - 64.0;
        let sizes: &[f32] = if compact {
            &[24.0, 16.0]
        } else {
            &[32.0, 24.0, 16.0]
        };
        let base = sizes
            .iter()
            .copied()
            .find(|size| width(title, display(*size, color)) <= max)
            .unwrap_or(16.0);
        let scale = banner.scale(juice.motion().min(1.0));
        let title_style = display(base * scale, fade(color, alpha));
        let sub_style = body(16.0, fade(PAPER, alpha));
        let band_w = (width(title, display(base, color)).max(width(sub, sub_style)) + 56.0)
            .min(view.w - 32.0);
        let top = view.layout.scene.y + 6.0;
        let band = Rect::new((view.w - band_w) * 0.5, top, band_w, base + 44.0);
        shapes::glow_disc(
            vec2(view.w * 0.5, top + base * 0.6),
            band_w * 0.45,
            fade(color, 0.22 * alpha),
        );
        panel(band, RADIUS, Some(fade(color, alpha)), alpha * 0.92);
        let cy = top + 10.0 + base * 0.5;
        let w = width(title, title_style);
        text::draw_text_styled(
            title,
            view.w * 0.5 - w * 0.5,
            title_style.centered_baseline(cy),
            title_style,
        );
        line(
            sub,
            24.0,
            top + base + 34.0,
            view.w - 48.0,
            sub_style,
            Align::Center,
        );
    });
}

/// A tiny hard hat: Gus's avatar when his bubble cannot point at him.
fn hard_hat(at: Vec2, alpha: f32) {
    let yellow = fade(Color::from_rgba(250, 206, 64, 255), alpha);
    shapes::fill_circle(at, 8.0, yellow);
    shapes::fill_rounded_rect(
        Rect::new(at.x - 9.0, at.y, 18.0, 9.0),
        0.0,
        fade(PAPER, alpha),
    );
    shapes::fill_rounded_rect(Rect::new(at.x - 11.0, at.y - 1.0, 22.0, 3.0), 1.5, yellow);
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
    // A quick pop-in on each new line (motion only).
    let pop = 1.0 + (back_out(bubble.age / 0.22) - 1.0) * view.anim.motion;
    let style = TextStyle::modern(16.0, fade(INK, alpha));
    let compact = view.compact();
    let avatar = if compact { 26.0 } else { 0.0 };
    let max = (view.w - 56.0 - avatar).min(340.0);
    let (first, second) = wrap(bubble.text, max, style);
    let lines = if second.is_empty() { 1.0 } else { 2.0 };
    let text_w = width(first, style).max(width(second, style)).min(max);
    let (bw, bh) = (text_w + 28.0 + avatar, lines * 22.0 + 16.0);
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
    // Scale about the tail corner so the bubble grows out of Gus.
    let origin = vec2(x + 26.0, y + bh);
    let rect = Rect::new(
        origin.x + (x - origin.x) * pop,
        origin.y + (y - origin.y) * pop,
        bw * pop,
        bh * pop,
    );
    let fill = fade(PAPER, alpha);
    shapes::drop_shadow(
        rect,
        RADIUS,
        Shadow::soft(fade(SHADOW, alpha))
            .offset(vec2(0.0, 4.0))
            .blur(12.0),
    );
    if !compact && anchor.y > y + bh {
        let base = anchor.x.clamp(x + 16.0, x + bw - 16.0);
        let tip = vec2(anchor.x, anchor.y.min(y + bh + 16.0));
        draw_triangle(
            vec2(base - 9.0, rect.y + rect.h - 1.0),
            vec2(base + 9.0, rect.y + rect.h - 1.0),
            tip,
            fill,
        );
    }
    shapes::fill_rounded_rect(rect, RADIUS, fill);
    shapes::stroke_rounded_rect(
        rect,
        RADIUS,
        2.0,
        fade(Color::from_rgba(60, 40, 32, 255), alpha),
    );
    if pop < 0.98 {
        return;
    }
    if compact {
        hard_hat(vec2(x + 20.0, y + bh * 0.5 + 1.0), alpha);
    }
    for (i, part) in [first, second].into_iter().enumerate() {
        if !part.is_empty() {
            line(
                part,
                x + 14.0 + avatar,
                y + 8.0 + 16.0 + i as f32 * 22.0,
                bw - 28.0 - avatar,
                style,
                Align::Center,
            );
        }
    }
}

// --- Screens -----------------------------------------------------------------

/// One row of a card. Rows are produced twice (measure, then draw) so their
/// formatted text never needs to be stored.
enum Row<'a> {
    Title(&'a str, Color),
    Text(&'a str, f32, Color),
    Display(&'a str, f32, Color),
    Stars(u8, f32),
    Prompt(&'a str, &'a str),
    Gap(f32),
}

impl Row<'_> {
    fn style(&self, inner: f32) -> TextStyle {
        match *self {
            Row::Title(text, color) => {
                let size = [32.0, 24.0, 16.0]
                    .into_iter()
                    .find(|size| width(text, display(*size, color)) <= inner)
                    .unwrap_or(16.0);
                display(size, color)
            }
            Row::Display(text, size, color) => {
                let size = [size, 32.0, 24.0, 16.0]
                    .into_iter()
                    .find(|s| *s <= size && width(text, display(*s, color)) <= inner)
                    .unwrap_or(16.0);
                display(size, color)
            }
            Row::Text(_, size, color) => body(size, color),
            _ => body(16.0, PAPER),
        }
    }

    fn wrapped(&self, inner: f32) -> bool {
        matches!(self, Row::Text(text, ..) if width(text, self.style(inner)) > inner)
    }

    fn height(&self, inner: f32) -> f32 {
        match *self {
            Row::Text(..) if self.wrapped(inner) => 52.0,
            Row::Text(_, size, _) => size + 14.0,
            Row::Title(..) | Row::Display(..) => self.style(inner).size + 16.0,
            Row::Stars(_, radius) => radius * 2.0 + 16.0,
            Row::Prompt(..) => 40.0,
            Row::Gap(gap) => gap,
        }
    }
}

/// Card layout. `rows` calls `emit(row, priority)`; priority 0 always stays,
/// larger numbers drop first when the card would cover the bottom panel.
#[allow(clippy::type_complexity)] // Borrowed row visitor keeps formatting allocation-free.
fn card(
    view: &View,
    accent: Color,
    age: f32,
    stars_age: Option<f32>,
    rows: &dyn Fn(&mut dyn FnMut(Row, u8)),
) {
    let top = view.layout.scene.y + 4.0;
    let bottom = view.layout.panel.y - 8.0;
    let width_ = (view.w - 32.0).min(500.0);
    let inner = width_ - 40.0;
    let padding = 40.0;
    let mut totals = [0.0_f32; 8];
    rows(&mut |row, priority| {
        for (cut, total) in totals.iter_mut().enumerate() {
            if usize::from(priority) <= cut {
                *total += row.height(inner);
            }
        }
    });
    let cut = (0..8)
        .rev()
        .find(|cut| totals[*cut] + padding <= bottom - top)
        .unwrap_or(0) as u8;
    let height = totals[usize::from(cut)] + padding;
    let motion = view.anim.motion;
    let (alpha, slide) = entrance(age, 0, motion);
    let x = (view.w - width_) * 0.5;
    let y = top + ((bottom - top - height) * 0.3).max(0.0) + slide;
    panel(
        Rect::new(x, y, width_, height),
        CARD_RADIUS,
        Some(accent),
        alpha,
    );
    let mut cursor = y + 22.0;
    let mut index = 0;
    rows(&mut |row, priority| {
        if priority > cut {
            return;
        }
        index += 1;
        let (a, dy) = entrance(age, index, motion);
        let a = a * alpha;
        let h = row.height(inner);
        let cx = view.w * 0.5;
        match row {
            Row::Text(text, _, color) if row.wrapped(inner) => {
                let style = body(16.0, fade(color, a));
                let (first, second) = wrap(text, inner, style);
                for (i, part) in [first, second].into_iter().enumerate() {
                    line(
                        part,
                        x + 20.0,
                        cursor + dy + 18.0 + i as f32 * 24.0,
                        inner,
                        style,
                        Align::Center,
                    );
                }
            }
            Row::Text(text, ..) | Row::Title(text, _) | Row::Display(text, ..) => {
                let mut style = row.style(inner);
                style.color = fade(style.color, a);
                if let Some(outline) = style.outline {
                    style.outline = Some(fade(outline, a));
                }
                if let Some(shadow) = style.shadow {
                    style.shadow = Some(fade(shadow, a));
                }
                let baseline = cursor + dy + 6.0 + style.size * 0.9;
                line(text, x + 20.0, baseline, inner, style, Align::Center);
            }
            Row::Stars(earned, radius) => {
                stars_row(
                    vec2(cx, cursor + dy + radius + 8.0),
                    radius,
                    earned,
                    a,
                    stars_age,
                    motion,
                );
            }
            Row::Prompt(button, verb) => {
                prompt(view, cx, cursor + dy + 26.0, button, verb, a);
            }
            Row::Gap(_) => {}
        }
        cursor += h;
    });
}

/// The front door: an animated logo over the attract scene.
fn title_screen(view: &View) {
    let anim = view.anim;
    let age = anim.state_age;
    let w = view.w;
    // A dark scrim at the top keeps the logo legible over the sky.
    let scrim = Rect::new(0.0, 0.0, w, view.layout.panel.y);
    shapes::fill_rounded_rect(
        scrim,
        0.0,
        Fill::vertical(
            scrim,
            Color::new(0.04, 0.02, 0.02, 0.78),
            Color::new(0.04, 0.02, 0.02, 0.0),
        ),
    );
    let compact = view.compact();
    let logo = if compact { 32.0 } else { 56.0 };
    let top = view.layout.scene.y + if compact { 18.0 } else { 40.0 };
    let (alpha, slide) = entrance(age, 0, anim.motion);
    // Logo: molten letters with a slow wave and a heat shimmer.
    for (row, word) in ["SIEGE", "FOUNDRY"].into_iter().enumerate() {
        let style = display(logo, GOLD).with_letter_spacing(1.0);
        let total = width(word, style);
        let mut x = (w - total) * 0.5;
        let baseline = top + slide + logo * 0.9 + row as f32 * (logo + 8.0);
        if row == 0 {
            shapes::glow_disc(
                vec2(w * 0.5, baseline + logo * 0.2),
                total.max(200.0) * 0.7,
                fade(EMBER, 0.28 * alpha),
            );
        }
        for (i, c) in word.char_indices() {
            let glyph = &word[i..i + c.len_utf8()];
            let k = (i + row * 5) as f32;
            let wave = (anim.time * 2.4 - k * 0.45).sin();
            let heat = if anim.pulse { wave * 0.5 + 0.5 } else { 0.5 };
            let color = shapes::mix(
                Color::from_rgba(255, 150, 60, 255),
                Color::from_rgba(255, 228, 150, 255),
                heat,
            );
            let y = baseline + wave * 3.0 * anim.motion;
            let glyph_style = display(logo, fade(color, alpha)).with_letter_spacing(1.0);
            text::draw_text_styled(glyph, x, y, glyph_style);
            x += width(glyph, glyph_style) + logo / 8.0;
        }
    }
    let mut y = top + slide + logo * 2.0 + 40.0;
    let (a1, d1) = entrance(age, 2, anim.motion);
    let tagline = body(16.0, fade(PAPER, a1));
    line(
        "Topple all six rust-red targets with eight shells.",
        24.0,
        y + d1,
        w - 48.0,
        tagline,
        Align::Center,
    );
    y += 28.0;
    let (a2, d2) = entrance(age, 3, anim.motion);
    line(
        "Fewer shots, more stars. Gus is watching.",
        24.0,
        y + d2,
        w - 48.0,
        body(16.0, fade(MUTED, a2)),
        Align::Center,
    );
    y += 34.0;
    let (a3, d3) = entrance(age, 4, anim.motion);
    if y + 60.0 < view.layout.panel.y {
        stars_row(
            vec2(w * 0.5, y + d3 + 10.0),
            11.0,
            view.best.stars,
            a3,
            None,
            anim.motion,
        );
        y += 34.0;
        let best = body(16.0, fade(MUTED, a3));
        match view.best.points {
            Some(points) => with_fmt(format_args!("Best {}", Grouped(points)), |t| {
                line(t, 24.0, y + d3, w - 48.0, best, Align::Center)
            }),
            None => line(
                "No record yet. Make history.",
                24.0,
                y + d3,
                w - 48.0,
                best,
                Align::Center,
            ),
        }
        y += 40.0;
    }
    let (a4, d4) = entrance(age, 5, anim.motion);
    // A slow breathing prompt; steady under reduced flash.
    let breathe = if anim.pulse {
        0.75 + 0.25 * (anim.time * 2.2).sin()
    } else {
        1.0
    };
    if y + 30.0 < view.layout.panel.y {
        prompt(view, w * 0.5, y + d4, "Start", "to start", a4 * breathe);
    }
}

fn paused_card(view: &View) {
    let game = &view.foundry.range;
    let pause = view.foundry.banter.pause_line();
    card(
        view,
        lighten(STEEL, 0.4),
        view.anim.state_age,
        None,
        &|emit| {
            emit(Row::Title("PAUSED", GOLD), 0);
            emit(Row::Text(pause, 16.0, MUTED), 2);
            with_fmt(
                format_args!(
                    "Targets {}/{GOAL} · Score {}",
                    game.score(),
                    Grouped(u64::from(game.tally.points))
                ),
                |t| emit(Row::Text(t, 16.0, PAPER), 3),
            );
            if view.hints.touch {
                emit(Row::Text("Press Resume below", 16.0, GOLD), 1);
            } else {
                with_fmt(
                    format_args!("Press {} or Resume to continue", view.hints.pause),
                    |t| emit(Row::Text(t, 16.0, GOLD), 1),
                );
            }
        },
    );
}

/// The hook that makes "one more try" irresistible.
pub(super) fn next_goal(run: RunSummary, previous: Best, points: u64, out: &mut dyn FnMut(&str)) {
    if run.won {
        if let Some(shots) = score::next_star_shots(run.stars) {
            with_fmt(format_args!("Next star: clear it in {shots} shots"), |t| {
                out(t)
            });
        } else if !run.bell {
            out("Secret: something up high goes ding");
        } else if !run.domino {
            out("Next: topple two towers with one shot");
        } else {
            let best = previous.points.unwrap_or(0).max(points);
            with_fmt(
                format_args!("Beat {} for a new best", Grouped(best + 1)),
                |t| out(t),
            );
        }
    } else {
        let left = GOAL as u32 - run.targets.min(GOAL as u32);
        if left <= 2 {
            with_fmt(
                format_args!(
                    "So close! {left} target{} left",
                    if left == 1 { "" } else { "s" }
                ),
                |t| out(t),
            );
        } else {
            out("Tip: hit the low sandy supports");
        }
    }
}

fn results_card(view: &View) {
    let foundry = view.foundry;
    let tally = &foundry.range.tally;
    let run = foundry.summary();
    let rank = quips::rank(run);
    let points = u64::from(tally.points);
    let results = view.results;
    let age = foundry.juice.result_age - juice::CARD_DELAY;
    // The total rolls up from zero; reduced motion shows it at once.
    let roll = if view.anim.motion > 0.0 {
        cubic_out((age - 0.15) / 0.9)
    } else {
        1.0
    };
    let shown = (points as f32 * roll).round() as u64;
    let accent = if run.won { GOLD } else { RUST };
    card(view, accent, age, Some(age), &|emit| {
        emit(
            Row::Title(
                if run.won {
                    "FOUNDRY CLEARED!"
                } else {
                    "OUT OF SHELLS"
                },
                if run.won { GOLD } else { PAPER },
            ),
            0,
        );
        emit(Row::Stars(run.stars, 17.0), 0);
        with_fmt(format_args!("{}", Grouped(shown)), |t| {
            emit(
                Row::Display(t, if view.w < 480.0 { 32.0 } else { 40.0 }, PAPER),
                0,
            )
        });
        if results.new_best {
            emit(Row::Text("New personal best!", 16.0, GOLD), 3);
        } else if results.new_stars {
            emit(Row::Text("New star record!", 16.0, GOLD), 3);
        } else {
            match results.previous.points {
                Some(best) => with_fmt(format_args!("Best {}", Grouped(best)), |t| {
                    emit(Row::Text(t, 16.0, MUTED), 3)
                }),
                None => emit(Row::Text("No record yet. Make history.", 16.0, MUTED), 3),
            }
        }
        if run.won {
            with_fmt(
                format_args!(
                    "{} shots · best chain ×{} · {} misses",
                    tally.shots,
                    tally.best_chain.max(1),
                    tally.misses
                ),
                |t| emit(Row::Text(t, 16.0, MUTED), 4),
            );
        } else {
            with_fmt(
                format_args!("{}/{GOAL} targets · {} misses", run.targets, tally.misses),
                |t| emit(Row::Text(t, 16.0, MUTED), 4),
            );
        }
        emit(Row::Gap(4.0), 5);
        emit(Row::Display(rank.title, 24.0, GOLD), 1);
        emit(Row::Text(rank.quip, 16.0, PAPER), 3);
        next_goal(run, results.previous, points, &mut |t| {
            emit(Row::Text(t, 16.0, GREEN), 2)
        });
        emit(Row::Gap(4.0), 5);
        if run.won {
            emit(Row::Prompt("Again", "to go again"), 0);
        } else {
            emit(Row::Prompt("Retry", "to retry"), 0);
        }
    });
}

/// The foundry doors swing open on start and rebuild. Reduced motion fades a
/// dark overlay out instead.
fn doors(view: &View) {
    let t = view.anim.doors / DOORS;
    if t >= 1.0 {
        return;
    }
    let (w, h) = (view.w, view.h);
    if view.anim.motion == 0.0 {
        let a = 1.0 - t;
        draw_rectangle(0.0, 0.0, w, h, Color::new(0.05, 0.03, 0.03, 0.9 * a));
        return;
    }
    // A short hold, then a heavy ease-out swing.
    let open = cubic_out(((t - 0.12) / 0.88).max(0.0));
    let half = w * 0.5;
    let shift = half * open;
    let seam = 1.0 - open;
    for (x, dir) in [(-shift, -1.0_f32), (half + shift, 1.0)] {
        let door = Rect::new(x, 0.0, half, h);
        shapes::fill_rounded_rect(
            door,
            0.0,
            Fill::horizontal(
                door,
                Color::from_rgba(46, 36, 34, 255),
                Color::from_rgba(30, 24, 24, 255),
            ),
        );
        // Riveted bands across each door.
        for band in 0..4 {
            let y = h * (0.14 + band as f32 * 0.24);
            let strip = Rect::new(door.x, y, door.w, 14.0);
            shapes::fill_rounded_rect(strip, 0.0, Color::from_rgba(70, 54, 48, 255));
            let mut rx = door.x + 18.0;
            while rx < door.x + door.w - 8.0 {
                shapes::fill_circle(vec2(rx, y + 7.0), 3.0, Color::from_rgba(120, 92, 74, 255));
                rx += 46.0;
            }
        }
        let edge = if dir < 0.0 { door.x + door.w } else { door.x };
        shapes::fill_rounded_rect(
            Rect::new(edge - 3.0, 0.0, 6.0, h),
            0.0,
            Color::from_rgba(20, 14, 14, 255),
        );
    }
    // Molten light spills through the opening seam.
    if seam > 0.0 {
        let gap = Rect::new(half - shift - 2.0, 0.0, shift * 2.0 + 4.0, h);
        shapes::glow_disc(
            vec2(half, h * 0.5),
            h * 0.4 * (0.3 + open),
            fade(EMBER, 0.6 * seam),
        );
        shapes::fill_rounded_rect(
            gap,
            0.0,
            Fill::horizontal(gap, fade(EMBER, 0.0), fade(GOLD, 0.0)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal(run: RunSummary, best: Best, points: u64) -> String {
        let mut out = String::new();
        next_goal(run, best, points, &mut |t| out.push_str(t));
        out
    }

    #[test]
    fn grouping_and_next_goals_read_naturally() {
        for (n, text) in [
            (0, "0"),
            (950, "950"),
            (1250, "1,250"),
            (1_234_567, "1,234,567"),
        ] {
            assert_eq!(Grouped(n).to_string(), text);
        }
        let won = RunSummary {
            won: true,
            stars: 2,
            targets: 6,
            ..Default::default()
        };
        assert_eq!(
            goal(won, Best::default(), 900),
            "Next star: clear it in 3 shots"
        );
        let three = RunSummary { stars: 3, ..won };
        assert!(goal(three, Best::default(), 900).contains("ding"));
        let all = RunSummary {
            bell: true,
            domino: true,
            ..three
        };
        let best = Best {
            points: Some(2000),
            stars: 3,
        };
        assert_eq!(goal(all, best, 1500), "Beat 2,001 for a new best");
        let lost = RunSummary {
            targets: 5,
            ..Default::default()
        };
        assert_eq!(goal(lost, Best::default(), 0), "So close! 1 target left");
        assert!(goal(RunSummary::default(), Best::default(), 0).starts_with("Tip"));
    }

    #[test]
    fn formatting_pool_reuses_strings_and_nests() {
        let outer = with_fmt(format_args!("{}", Grouped(12345)), |a| {
            with_fmt(format_args!("{a}!"), |b| b.to_string())
        });
        assert_eq!(outer, "12,345!");
        // Warm pool: the same nesting depth reuses both strings.
        let before = POOL.with(|pool| pool.borrow().len());
        with_fmt(format_args!("x"), |_| with_fmt(format_args!("y"), |_| ()));
        assert_eq!(POOL.with(|pool| pool.borrow().len()), before);
    }

    #[test]
    fn bubble_wrap_balances_long_lines_and_keeps_short_ones() {
        let style = TextStyle::modern(16.0, PAPER);
        assert_eq!(
            wrap("Wind, probably.", 300.0, style),
            ("Wind, probably.", "")
        );
        let (a, b) = wrap("Load-bearing towers. Allegedly.", 200.0, style);
        assert!(!a.is_empty() && !b.is_empty());
        assert!(width(a, style) <= 200.0 && width(b, style) <= 200.0);
        assert_eq!(
            wrap("Nospacesatallhere", 10.0, style),
            ("Nospacesatallhere", "")
        );
    }

    #[test]
    fn hud_copy_fits_its_boxes_at_narrow_and_desktop_widths() {
        let mut lines: Vec<(String, String)> = Vec::new();
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
            callout_text(callout, 1400, &mut |t, s, _| {
                lines.push((t.into(), s.into()))
            });
        }
        for w in [320.0_f32, 390.0, 844.0, 1280.0, 1600.0] {
            let max = w - 64.0;
            for (title, sub) in &lines {
                assert!(width(title, display(16.0, PAPER)) <= max, "{title} at {w}");
                assert!(width(sub, body(16.0, PAPER)) <= w - 48.0, "{sub} at {w}");
            }
            // Card text fits at some size, or wraps onto two lines that fit.
            let inner = (w - 32.0).min(500.0) - 40.0;
            let mut copy: Vec<&str> = vec![
                "Next star: clear it in 3 shots",
                "Secret: something up high goes ding",
                "Next: topple two towers with one shot",
                "3 shots · best chain ×2 · 0 misses",
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
                RunSummary {
                    won: true,
                    stars: 3,
                    wonder: true,
                    ..Default::default()
                },
            ] {
                copy.push(quips::rank(run).quip);
                assert!(
                    width(quips::rank(run).title, display(16.0, GOLD)) <= inner,
                    "{} at {w}",
                    quips::rank(run).title
                );
            }
            for text in copy {
                let style = body(16.0, PAPER);
                let (a, b) = wrap(text, inner, style);
                assert!(
                    width(a, style) <= inner && width(b, style) <= inner,
                    "{text} at {w}"
                );
            }
            // Gus's lines wrap into at most two lines of his bubble.
            let bubble = TextStyle::modern(16.0, INK);
            let max = (w - 56.0 - 26.0).min(340.0);
            for quip in quips::all_lines() {
                let (a, b) = wrap(quip, max, bubble);
                assert!(
                    width(a, bubble) <= max && width(b, bubble) <= max,
                    "{quip} at {w}"
                );
            }
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
            if bar.compact {
                let used =
                    14.0 + 6.0 * bar.pip + 5.0 * bar.gap + 5.0 + width("6/6", count_style()) + 8.0;
                assert!(bar.left.w - 14.0 - used >= width("9,999", count_style()));
            }
            // Cards, banners and Gus's bubble all start at or below the scene.
            let layout = HudLayout::new(w, h);
            assert!(layout.scene.y + 4.0 >= MENU_CORNER.y, "cards at {w}x{h}");
            for rect in [layout.fire, layout.rebuild, layout.pause, layout.panel] {
                clear(rect, "bottom panel");
            }
        }
    }

    #[test]
    fn entrance_and_easing_respect_reduced_motion() {
        assert_eq!(cubic_out(0.0), 0.0);
        assert_eq!(cubic_out(1.0), 1.0);
        assert!((back_out(1.0) - 1.0).abs() < 1e-5);
        let (alpha, slide) = entrance(0.05, 0, 0.0);
        assert!(alpha > 0.0 && alpha < 1.0, "reduced motion still fades");
        assert_eq!(slide, 0.0, "but never slides");
        let (alpha, slide) = entrance(0.05, 0, 1.0);
        assert!(slide > 0.0 && alpha < 1.0);
        assert_eq!(entrance(2.0, 3, 1.0), (1.0, 0.0));
        // Later rows start later.
        assert!(entrance(0.1, 0, 1.0).0 > entrance(0.1, 2, 1.0).0);
    }
}
