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
use super::glyph::{self, Glyph, Item};
use super::juice::PopText;
use super::quips::{self, RunSummary};
use super::score::{self, Callout};
use super::*;
use ferravex_render::icons::Icon;
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
    pub restart: String,
    pub touch: bool,
}

impl Hints {
    pub fn refresh(&mut self, input: &PlayerInput) {
        let mut buffer = [0; 96];
        for (slot, action) in [
            (&mut self.fire, "fire"),
            (&mut self.confirm, "confirm"),
            (&mut self.pause, "pause"),
            (&mut self.restart, "restart"),
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
    /// Contextual prompts, 0..=1: the aim arrows around the cannon and the
    /// fire button's extra glow. Each fades once the player has done it.
    pub coach_fade: [f32; 2],
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
            coach_fade: [1.0; 2],
        }
    }
}

impl HudAnim {
    pub fn update(
        &mut self,
        dt: f32,
        state: SessionState,
        layout: &HudLayout,
        enabled: [bool; 3],
        coach: &Coach,
    ) {
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
        let fade = if self.motion > 0.0 { dt * 3.0 } else { 1.0 };
        for (level, learned) in self.coach_fade.iter_mut().zip([coach.aimed, coach.fired]) {
            let target = if learned { 0.0 } else { 1.0 };
            *level += (target - *level).clamp(-fade, fade);
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
        support_hint(view);
        aim_preview(view);
        aim_prompt(view);
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

pub(super) fn fade(color: Color, alpha: f32) -> Color {
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

/// A start/resume/retry prompt: the key cap beside the action's icon. Touch
/// shows the icon with an arrow down to the visible button instead.
fn prompt(view: &View, key: &str, cx: f32, cy: f32, icon: Icon, alpha: f32) {
    let max = view.w - 48.0;
    if view.hints.touch {
        glyph::row(
            &[
                Item::Glyph(Glyph::Icon(icon), GOLD),
                Item::Glyph(Glyph::Icon(Icon::ArrowDown), PAPER),
            ],
            vec2(cx, cy),
            26.0,
            max,
            alpha,
        );
        return;
    }
    glyph::row(
        &[Item::Key(key), Item::Glyph(Glyph::Icon(icon), GOLD)],
        vec2(cx, cy),
        22.0,
        max,
        alpha,
    );
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
        Some(block) if block.target && !block.down => (GOLD, Some(Glyph::Target)),
        Some(block) if !block.target => (lighten(SAND, 0.35), Some(Glyph::Support)),
        _ => (PAPER, None),
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
    // A swatch of the block under the reticle teaches what to hit; it fades
    // away once something has fallen.
    if !view.coach.knocked
        && let Some(tag) = tag
    {
        let x = at.x.clamp(32.0, view.w - 32.0);
        let pill = Rect::new(x - 16.0, at.y + 21.0, 32.0, 28.0);
        shapes::fill_rounded_rect(pill, 14.0, Color::new(0.08, 0.06, 0.06, 0.85));
        shapes::stroke_rounded_rect(pill, 14.0, 1.5, fade(color, 0.8));
        glyph::draw(tag, pill.center(), 24.0, PAPER, 1.0);
    }
}

/// After a miss, before anything has fallen, the low sandstone supports
/// breathe with a ring and an arrow instead of a written tip.
fn support_hint(view: &View) {
    let game = &view.foundry.range;
    if view.coach.knocked || game.tally.misses == 0 {
        return;
    }
    let beat = if view.anim.pulse {
        0.5 + 0.5 * (view.anim.time * 4.0).sin()
    } else {
        0.5
    };
    let bob = (view.anim.time * 3.0).sin().abs() * 5.0 * view.anim.motion;
    let sand = lighten(SAND, 0.45);
    for column in 0..3 {
        let block = &game.blocks[column * 4];
        if block.down {
            continue;
        }
        let Some(at) = view.camera.project(view.w, view.h, block.home) else {
            continue;
        };
        shapes::glow_ring(at, 20.0, 7.0, fade(sand, 0.2 + 0.25 * beat));
        shapes::stroke_circle(at, 17.0 + 3.0 * beat, 2.0, fade(sand, 0.55 + 0.35 * beat));
        glyph::draw(
            Glyph::Icon(Icon::ArrowDown),
            at - vec2(0.0, 34.0 + bob),
            22.0,
            GOLD,
            0.95,
        );
    }
}

/// Until the player has aimed: arrows around the cannon (a hand for touch)
/// say "move me". They fade once aiming has been learned.
fn aim_prompt(view: &View) {
    let a = view.anim.coach_fade[0];
    if a <= 0.01 || view.foundry.range.ammo == 0 {
        return;
    }
    let Some(at) = view.camera.project(view.w, view.h, PIVOT) else {
        return;
    };
    let bob = (view.anim.time * 3.0).sin().abs() * 4.0 * view.anim.motion;
    let r = 46.0 + bob;
    for (icon, dir) in [
        (Icon::ArrowLeft, vec2(-1.0, 0.0)),
        (Icon::ArrowRight, vec2(1.0, 0.0)),
        (Icon::ArrowUp, vec2(0.0, -1.0)),
        (Icon::ArrowDown, vec2(0.0, 1.0)),
    ] {
        glyph::draw(Glyph::Icon(icon), at + dir * r, 22.0, GOLD, a * 0.9);
    }
    if view.hints.touch {
        glyph::draw(
            Glyph::Icon(Icon::Hand),
            at + vec2(r * 0.8, -r * 0.8),
            26.0,
            PAPER,
            a,
        );
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

/// Gap between the cells of a mini tower.
fn cell_gap(cell: f32) -> f32 {
    (cell * 0.18).max(1.0)
}

/// Width of the three mini towers at `cell`.
fn towers_width(cell: f32) -> f32 {
    5.2 * cell
}

/// Top-bar geometry. Wide screens use a structure panel on the left and a
/// score panel inset from the menu corner; compact screens fold both into
/// one panel.
struct TopBar {
    compact: bool,
    /// Mini-tower cell size.
    cell: f32,
    left: Rect,
    /// Separate score panel (wide only).
    right: Option<Rect>,
    pace_w: f32,
}

impl TopBar {
    fn new(w: f32, value: &str) -> Self {
        let compact = w < 560.0;
        let cell = if compact { 8.0 } else { 9.0 };
        let height = 4.0 * cell + 3.0 * cell_gap(cell) + 10.0;
        let icon_w = if compact { 0.0 } else { 28.0 };
        let targets_w = 14.0 + icon_w + towers_width(cell) + 14.0;
        let value_w = width(value, count_style()).max(48.0);
        if compact {
            let wide = targets_w + 8.0 + value_w + 14.0;
            return Self {
                compact,
                cell,
                left: Rect::new(
                    16.0,
                    4.0,
                    wide.min(w - 16.0 - MENU_CORNER.x).max(1.0),
                    height,
                ),
                right: None,
                pace_w: 0.0,
            };
        }
        let pace_w = 3.0 * 17.0 + 12.0;
        let right_w = 14.0 + pace_w + value_w + 14.0;
        let right_x = w - MENU_CORNER.x - right_w;
        let left = Rect::new(16.0, 4.0, targets_w, height);
        Self {
            compact,
            cell,
            left,
            right: (right_x >= left.x + left.w + 8.0)
                .then(|| Rect::new(right_x, 4.0, right_w, height)),
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
    with_fmt(format_args!("{}", Grouped(points)), |value| {
        let bar = TopBar::new(view.w, value);
        draw_top_bar(view, state, &bar, value, rolling);
    });
}

/// The three towers as mini stacks of supports and targets: the structure's
/// integrity at a glance. Fallen blocks hollow out (targets with a tick) and
/// a wobbling tower leans.
fn structure(view: &View, left: f32, mid: f32, cell: f32, alpha: f32) {
    let game = &view.foundry.range;
    let height = 4.0 * cell + 3.0 * cell_gap(cell);
    for column in 0..3 {
        let down = std::array::from_fn(|layer| game.blocks[column * 4 + layer].down);
        let lean = view.foundry.juice.wobble[column] * (view.anim.time * 14.0).sin();
        let x = left + cell * 0.5 + column as f32 * cell * 2.1;
        glyph::mini_tower(vec2(x, mid + height * 0.5), cell, down, lean, alpha);
    }
}

/// Stars still on offer for this run.
fn pace(view: &View, state: SessionState) -> u8 {
    let game = &view.foundry.range;
    match state {
        SessionState::Won | SessionState::Lost => view.foundry.summary().stars,
        _ if game.ammo == 0 => 1,
        _ => score::stars(true, game.tally.shots + 1),
    }
}

fn draw_top_bar(view: &View, state: SessionState, bar: &TopBar, value: &str, rolling: bool) {
    panel(bar.left, RADIUS, None, 1.0);
    // Knockdowns light up the structure panel while they land.
    let destruction = (view.foundry.feedback.destruction / 2.0).clamp(0.0, 1.0);
    if destruction > 0.0 {
        shapes::stroke_rounded_rect(bar.left, RADIUS, 2.0, fade(GOLD, 0.85 * destruction));
    }
    let mid = bar.left.y + bar.left.h * 0.5;
    let mut x = bar.left.x + 14.0;
    if !bar.compact {
        glyph::draw(
            Glyph::Icon(Icon::Target),
            vec2(x + 10.0, mid),
            22.0,
            GOLD,
            1.0,
        );
        x += 28.0;
    }
    structure(view, x, mid, bar.cell, 1.0);
    x += towers_width(bar.cell);
    let score = display(16.0, if rolling { GOLD } else { PAPER });
    if bar.compact {
        let start = x + 8.0;
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
    let pace = pace(view, state);
    panel(right, RADIUS, None, 1.0);
    let mid = right.y + right.h * 0.5;
    for i in 0..3 {
        star(
            vec2(right.x + 22.0 + i as f32 * 17.0, mid),
            7.0,
            i < pace,
            if i < pace { GOLD } else { fade(MUTED, 0.6) },
        );
    }
    let start = right.x + 14.0 + bar.pace_w;
    line(
        value,
        start,
        score.centered_baseline(mid),
        right.x + right.w - 14.0 - start,
        score,
        Align::Right,
    );
}

/// What a control button shows: an icon, an optional short label (a
/// countdown) and the key cap that triggers it.
#[derive(Clone, Copy)]
struct Face<'a> {
    icon: Icon,
    label: Option<&'a str>,
    key: Option<&'a str>,
}

fn button(view: &View, index: usize, rect: Rect, face: Face, enabled: bool, primary: bool) {
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
        // Ready-to-fire glow, steady under reduced flash; brighter until the
        // first shot has been fired.
        let pulse = if anim.pulse {
            0.5 + 0.5 * (anim.time * 3.0).sin()
        } else {
            0.5
        };
        let invite = if index == 0 { anim.coach_fade[1] } else { 0.0 };
        shapes::glow_disc(
            vec2(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5),
            rect.w * (0.45 + 0.1 * invite),
            fade(
                EMBER,
                0.12 + 0.06 * pulse + 0.1 * hover + 0.12 * invite * pulse,
            ),
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
    let ink = if enabled { PAPER } else { fade(MUTED, 0.6) };
    let key = face.key.filter(|_| !view.hints.touch && enabled);
    let mut items = [Item::Gap(0.0); 4];
    let mut n = 0;
    items[n] = Item::Glyph(Glyph::Icon(face.icon), ink);
    n += 1;
    if let Some(label) = face.label {
        items[n] = Item::Text(label, ink);
        n += 1;
    }
    let bare = n;
    if let Some(key) = key {
        items[n] = Item::Gap(0.35);
        items[n + 1] = Item::Key(key);
        n += 2;
    }
    // Drop the key cap before shrinking the icon on narrow buttons.
    let size = 24.0;
    let n = if glyph::row_width(&items[..n], size) > rect.w - 16.0 {
        bare
    } else {
        n
    };
    glyph::row(
        &items[..n],
        vec2(rect.x + rect.w * 0.5, rect.y + rect.h * 0.5),
        size,
        rect.w - 16.0,
        if enabled { 1.0 } else { 0.8 },
    );
}

/// The aim as two gauges: a side-view fan for elevation and a top-down fan
/// for heading, each with the barrel as its needle and the angle beside it.
/// Draws nothing when there is no room right of the shells.
fn aim_gauges(view: &View, right: f32, top: f32, room: f32) {
    use std::f32::consts::FRAC_PI_2;
    let game = &view.foundry.range;
    let with_numbers = room >= 230.0;
    if room < 128.0 {
        return;
    }
    let numbers = body(16.0, PAPER);
    let fan = fade(MUTED, 0.22);
    let rim = fade(MUTED, 0.5);
    let barrel = Color::from_rgba(214, 200, 184, 255);
    let hub = Color::from_rgba(70, 60, 56, 255);
    // Heading: up is straight down the range, needle turned by yaw.
    let yaw_w = if with_numbers { 112.0 } else { 62.0 };
    let pivot = vec2(right - yaw_w + 31.0, top + 46.0);
    shapes::fill_sector(pivot, 30.0, -FRAC_PI_2 - 0.55, 1.1, fan);
    shapes::stroke_arc(pivot, 30.0, -FRAC_PI_2 - 0.55, 1.1, Stroke::new(2.0, rim));
    let a = -FRAC_PI_2 + game.yaw;
    shapes::fill_capsule(pivot, pivot + vec2(a.cos(), a.sin()) * 28.0, 3.4, barrel);
    shapes::fill_circle(pivot, 6.5, hub);
    if with_numbers {
        with_fmt(format_args!("{:+.0}°", game.yaw.to_degrees()), |t| {
            text::draw_text_styled(
                t,
                pivot.x + 34.0,
                numbers.centered_baseline(pivot.y - 16.0),
                numbers,
            )
        });
    }
    // Elevation: a side view, needle raised by the barrel angle.
    let elev_w = if with_numbers { 104.0 } else { 56.0 };
    let pivot = vec2(right - yaw_w - elev_w + 6.0, top + 48.0);
    shapes::fill_sector(pivot, 34.0, -0.68, 0.68, fan);
    shapes::stroke_arc(pivot, 34.0, -0.68, 0.68, Stroke::new(2.0, rim));
    let tip = pivot + vec2(game.elevation.cos(), -game.elevation.sin()) * 31.0;
    shapes::fill_capsule(pivot, tip, 3.4, barrel);
    shapes::fill_circle(pivot + vec2(1.0, 2.0), 7.0, hub);
    if with_numbers {
        with_fmt(format_args!("{:.0}°", game.elevation.to_degrees()), |t| {
            text::draw_text_styled(
                t,
                pivot.x + 40.0,
                numbers.centered_baseline(pivot.y - 10.0),
                numbers,
            )
        });
    }
}

fn bottom_panel(view: &View, state: SessionState) {
    let layout = view.layout;
    let game = &view.foundry.range;
    let p = layout.panel;
    panel(p, RADIUS, Some(fade(EMBER, 0.8)), 1.0);
    // Shells: iron balls; the next one in the breech glows when loaded and
    // fills a gold ring while it reloads.
    let compact = view.compact();
    let (r, step) = if compact { (7.0, 18.0) } else { (8.5, 22.0) };
    let row = p.y + 24.0;
    for i in 0..AMMO {
        let at = vec2(p.x + 16.0 + r + f32::from(i) * step, row);
        if i < game.ammo {
            let next = i + 1 == game.ammo && state == SessionState::Playing;
            if next && game.can_fire() {
                shapes::glow_disc(at, r * 1.9, fade(EMBER, 0.5));
            }
            glyph::shell(at, r, 1.0);
            if next && game.cooldown > 0.0 {
                shapes::stroke_arc(
                    at,
                    r + 3.5,
                    -std::f32::consts::FRAC_PI_2,
                    std::f32::consts::TAU * (1.0 - game.cooldown / RELOAD).clamp(0.0, 1.0),
                    Stroke::new(2.5, GOLD).cap(shapes::Cap::Round),
                );
            }
        } else {
            shapes::stroke_circle(at, r - 1.0, 1.5, fade(MUTED, 0.45));
        }
    }
    // Accepted-shot and rebuild confirmations: the slot just fired rings out
    // and a fresh rack shimmers (both static under reduced motion).
    let feedback = &view.foundry.feedback;
    if feedback.shot > 0.0 && game.ammo < AMMO {
        let at = vec2(p.x + 16.0 + r + f32::from(game.ammo) * step, row);
        let t = feedback.shot.clamp(0.0, 1.0);
        shapes::stroke_circle(
            at,
            r + 2.0 + (1.0 - t) * 9.0 * view.anim.motion,
            2.0,
            fade(GOLD, t),
        );
    }
    if feedback.rebuilt > 0.0 {
        let t = (feedback.rebuilt / 1.8).clamp(0.0, 1.0);
        for i in 0..game.ammo {
            let at = vec2(p.x + 16.0 + r + f32::from(i) * step, row);
            shapes::glow_disc(at, r * 1.7, fade(GOLD, 0.3 * t));
        }
    }
    let shells_right = p.x + 16.0 + f32::from(AMMO) * step;
    if state != SessionState::Ready {
        aim_gauges(
            view,
            p.x + p.w - 12.0,
            p.y + 4.0,
            p.x + p.w - 12.0 - shells_right - 16.0,
        );
    }
    // The last shell's settling window as an hourglass countdown.
    if state == SessionState::Playing && game.ammo == 0 {
        with_fmt(
            format_args!("{:.1}s", (SETTLE - game.since_shot).max(0.0)),
            |t| {
                glyph::row_at(
                    &[
                        Item::Glyph(Glyph::Icon(Icon::Hourglass), GOLD),
                        Item::Text(t, GOLD),
                    ],
                    p.x + 16.0,
                    p.y + 48.0,
                    16.0,
                    1.0,
                );
            },
        );
    }

    let paused = view.foundry.session.is_paused();
    let hints = view.hints;
    let settle = (SETTLE - game.since_shot).max(0.0);
    let [fire_enabled, rebuild_enabled, pause_enabled] = button_enabled(view, state);
    with_fmt(format_args!("{settle:.1}"), |countdown| {
        let fire = match state {
            SessionState::Ready => Face {
                icon: Icon::Play,
                label: None,
                key: Some(hints.confirm.as_str()),
            },
            SessionState::Won | SessionState::Lost => Face {
                icon: Icon::Restart,
                label: None,
                key: Some(hints.confirm.as_str()),
            },
            SessionState::Playing if game.ammo == 0 => Face {
                icon: Icon::Hourglass,
                label: Some(countdown),
                key: None,
            },
            _ => Face {
                icon: Icon::Bomb,
                label: None,
                key: Some(hints.fire.as_str()),
            },
        };
        button(view, 0, layout.fire, fire, fire_enabled, true);
    });
    button(
        view,
        1,
        layout.rebuild,
        Face {
            icon: Icon::Hammer,
            label: None,
            key: Some(hints.restart.as_str()),
        },
        rebuild_enabled,
        false,
    );
    button(
        view,
        2,
        layout.pause,
        Face {
            icon: if paused { Icon::Play } else { Icon::Pause },
            label: None,
            key: Some(hints.pause.as_str()),
        },
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

/// A callout's face: an optional pictogram, its Classic title and accent.
/// The points it paid follow on the same line.
fn callout_face(callout: Callout, out: &mut dyn FnMut(Option<Glyph>, &str, Color)) {
    match callout {
        Callout::Chain(n) => with_fmt(format_args!("CHAIN ×{n}!"), |t| out(None, t, PAPER)),
        Callout::Timber => out(
            Some(Glyph::Icon(Icon::Tree)),
            "TIMBER!",
            Color::from_rgba(255, 150, 70, 255),
        ),
        Callout::Domino => out(None, "DOMINO EFFECT!", GOLD),
        Callout::Overkill => out(Some(Glyph::Icon(Icon::Skull)), "OVERKILL!", GOLD),
        Callout::OneShotWonder => out(Some(Glyph::Icon(Icon::Star)), "ONE-SHOT WONDER!", GOLD),
        Callout::Delayed => out(
            Some(Glyph::Icon(Icon::Hourglass)),
            "WAIT... IT COUNTS!",
            GREEN,
        ),
        Callout::Bell => out(Some(Glyph::Icon(Icon::Bell)), "DING!", GOLD),
        Callout::Spare(n) => with_fmt(format_args!("SPARE ×{n}"), |t| {
            out(Some(Glyph::Shell), t, GOLD)
        }),
    }
}

/// One banner line: pictogram, title and the points it paid.
fn callout_items<'a>(
    icon: Option<Glyph>,
    title: &'a str,
    color: Color,
    points: Option<&'a str>,
    out: &mut [Item<'a>; 4],
) -> usize {
    let mut n = 0;
    if let Some(icon) = icon {
        out[n] = Item::Glyph(icon, color);
        n += 1;
    }
    out[n] = Item::Display(title, color);
    n += 1;
    if let Some(points) = points {
        out[n] = Item::Gap(0.35);
        out[n + 1] = Item::Display(points, GOLD);
        n += 2;
    }
    n
}

/// The largest banner size whose line fits `max`; the decorative pictogram
/// goes first when even the smallest size would not fit.
fn fit_callout<'a>(
    icon: Option<Glyph>,
    title: &'a str,
    color: Color,
    points: Option<&'a str>,
    sizes: &[f32],
    max: f32,
    out: &mut [Item<'a>; 4],
) -> (usize, f32) {
    let n = callout_items(icon, title, color, points, out);
    if let Some(size) = sizes
        .iter()
        .copied()
        .find(|size| glyph::row_width(&out[..n], *size) <= max)
    {
        return (n, size);
    }
    let smallest = sizes.last().copied().unwrap_or(16.0);
    (callout_items(None, title, color, points, out), smallest)
}

fn banner(view: &View) {
    let juice = &view.foundry.juice;
    let Some(banner) = juice.banner else {
        return;
    };
    let alpha = banner.alpha();
    let compact = view.compact();
    callout_face(banner.callout, &mut |icon, title, color| {
        with_fmt(format_args!("+{}", banner.points), |points| {
            let points = (banner.points > 0).then_some(points);
            let max = view.w - 64.0;
            let sizes: &[f32] = if compact {
                &[24.0, 16.0]
            } else {
                &[32.0, 24.0, 16.0]
            };
            let mut items = [Item::Gap(0.0); 4];
            let (n, base) = fit_callout(icon, title, color, points, sizes, max, &mut items);
            let scale = banner.scale(juice.motion().min(1.0));
            let band_w = (glyph::row_width(&items[..n], base) + 56.0).min(view.w - 32.0);
            let top = view.layout.scene.y + 6.0;
            let band = Rect::new((view.w - band_w) * 0.5, top, band_w, base + 26.0);
            shapes::glow_disc(
                vec2(view.w * 0.5, top + base * 0.6),
                band_w * 0.45,
                fade(color, 0.22 * alpha),
            );
            panel(band, RADIUS, Some(fade(color, alpha)), alpha * 0.92);
            glyph::row(
                &items[..n],
                vec2(view.w * 0.5, band.y + band.h * 0.5),
                base * scale,
                view.w - 48.0,
                alpha,
            );
        })
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
    /// A centered row of pictograms, numbers and key caps at a size.
    Items(&'a [Item<'a>], f32),
    /// The three towers' integrity glyphs at a cell size.
    Structure(f32),
    /// A key cap and the icon of the action it triggers.
    Prompt(&'a str, Icon),
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
            Row::Items(_, size) => size * 1.3 + 12.0,
            Row::Structure(cell) => 4.0 * cell + 3.0 * cell_gap(cell) + 14.0,
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
            Row::Items(items, size) => {
                glyph::row(items, vec2(cx, cursor + dy + h * 0.5), size, inner, a);
            }
            Row::Structure(cell) => {
                structure(
                    view,
                    cx - towers_width(cell) * 0.5,
                    cursor + dy + h * 0.5,
                    cell,
                    a,
                );
            }
            Row::Prompt(key, icon) => {
                prompt(view, key, cx, cursor + dy + 20.0, icon, a);
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
    let mut y = top + slide + logo * 2.0 + 34.0;
    // The job as pictograms: six rust targets, eight shells.
    let (a1, d1) = entrance(age, 2, anim.motion);
    glyph::row(
        &[
            Item::Glyph(Glyph::Target, PAPER),
            Item::Display("×6", PAPER),
            Item::Gap(0.9),
            Item::Glyph(Glyph::Shell, PAPER),
            Item::Display("×8", PAPER),
        ],
        vec2(w * 0.5, y + d1),
        if compact { 22.0 } else { 30.0 },
        w - 48.0,
        a1,
    );
    y += 40.0;
    // Fewer shells, more stars: three stars within three shells, two within five.
    let (a2, d2) = entrance(age, 3, anim.motion);
    glyph::row(
        &[
            Item::Glyph(Glyph::Star(true), GOLD),
            Item::Glyph(Glyph::Star(true), GOLD),
            Item::Glyph(Glyph::Star(true), GOLD),
            Item::Gap(0.25),
            Item::Glyph(Glyph::Shell, PAPER),
            Item::Text("3", MUTED),
            Item::Gap(1.1),
            Item::Glyph(Glyph::Star(true), GOLD),
            Item::Glyph(Glyph::Star(true), GOLD),
            Item::Glyph(Glyph::Star(false), MUTED),
            Item::Gap(0.25),
            Item::Glyph(Glyph::Shell, PAPER),
            Item::Text("5", MUTED),
        ],
        vec2(w * 0.5, y + d2),
        if compact { 18.0 } else { 22.0 },
        w - 48.0,
        a2,
    );
    y += 38.0;
    let (a3, d3) = entrance(age, 4, anim.motion);
    if y + 60.0 < view.layout.panel.y
        && let Some(points) = view.best.points
    {
        with_fmt(format_args!("{}", Grouped(points)), |t| {
            glyph::row(
                &[
                    Item::Glyph(Glyph::Icon(Icon::Trophy), GOLD),
                    Item::Text(t, MUTED),
                    Item::Gap(0.5),
                    Item::Glyph(Glyph::Star(view.best.stars >= 1), GOLD),
                    Item::Glyph(Glyph::Star(view.best.stars >= 2), GOLD),
                    Item::Glyph(Glyph::Star(view.best.stars >= 3), GOLD),
                ],
                vec2(w * 0.5, y + d3),
                18.0,
                w - 48.0,
                a3,
            );
        });
        y += 36.0;
    }
    let (a4, d4) = entrance(age, 5, anim.motion);
    // A slow breathing prompt; steady under reduced flash.
    let breathe = if anim.pulse {
        0.75 + 0.25 * (anim.time * 2.2).sin()
    } else {
        1.0
    };
    if y + 30.0 < view.layout.panel.y {
        prompt(
            view,
            view.hints.confirm.as_str(),
            w * 0.5,
            y + 8.0 + d4,
            Icon::Play,
            a4 * breathe,
        );
    }
}

fn paused_card(view: &View) {
    let game = &view.foundry.range;
    let pause = view.foundry.banter.pause_line();
    let pace = pace(view, SessionState::Paused);
    card(
        view,
        lighten(STEEL, 0.4),
        view.anim.state_age,
        None,
        &|emit| {
            emit(Row::Title("PAUSED", GOLD), 0);
            emit(Row::Text(pause, 16.0, MUTED), 2);
            emit(Row::Structure(13.0), 3);
            with_fmt(
                format_args!("{}", Grouped(u64::from(game.tally.points))),
                |t| {
                    emit(
                        Row::Items(
                            &[
                                Item::Glyph(Glyph::Star(pace >= 1), GOLD),
                                Item::Glyph(Glyph::Star(pace >= 2), GOLD),
                                Item::Glyph(Glyph::Star(pace >= 3), GOLD),
                                Item::Gap(0.4),
                                Item::Display(t, PAPER),
                            ],
                            22.0,
                        ),
                        3,
                    )
                },
            );
            emit(Row::Prompt(view.hints.pause.as_str(), Icon::Play), 1);
        },
    );
}

/// The hook that makes "one more try" irresistible.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Goal {
    /// Win again within this many shots for the next star.
    NextStar { shots: u8 },
    /// The secret bell is still unrung.
    Secret,
    /// Topple two towers with one shot.
    Domino,
    /// Score at least this many points.
    BeatBest(u64),
    /// Only this many targets were left standing.
    SoClose(u32),
    /// Aim at the low sandstone supports.
    Supports,
}

pub(super) fn next_goal(run: RunSummary, previous: Best, points: u64) -> Goal {
    if run.won {
        if let Some(shots) = score::next_star_shots(run.stars) {
            Goal::NextStar { shots }
        } else if !run.bell {
            Goal::Secret
        } else if !run.domino {
            Goal::Domino
        } else {
            Goal::BeatBest(previous.points.unwrap_or(0).max(points) + 1)
        }
    } else {
        let left = GOAL as u32 - run.targets.min(GOAL as u32);
        if left <= 2 {
            Goal::SoClose(left)
        } else {
            Goal::Supports
        }
    }
}

/// The goal as one pictogram row after a flag.
fn goal_items(goal: Goal, out: &mut dyn FnMut(&[Item])) {
    let flag = Item::Glyph(Glyph::Icon(Icon::Flag), GREEN);
    match goal {
        Goal::NextStar { shots } => with_fmt(format_args!("{shots}"), |t| {
            out(&[
                flag,
                Item::Glyph(Glyph::Star(true), GOLD),
                Item::Gap(0.3),
                Item::Glyph(Glyph::Shell, PAPER),
                Item::Text(t, GREEN),
            ])
        }),
        Goal::Secret => out(&[
            flag,
            Item::Glyph(Glyph::Icon(Icon::Bell), GOLD),
            Item::Text("?", GREEN),
        ]),
        Goal::Domino => out(&[
            flag,
            Item::Glyph(Glyph::Shell, PAPER),
            Item::Text("1", GREEN),
            Item::Glyph(Glyph::Icon(Icon::ArrowRight), MUTED),
            Item::Glyph(Glyph::Target, PAPER),
            Item::Gap(0.3),
            Item::Glyph(Glyph::Target, PAPER),
        ]),
        Goal::BeatBest(points) => with_fmt(format_args!("{}", Grouped(points)), |t| {
            out(&[
                flag,
                Item::Glyph(Glyph::Icon(Icon::Trophy), GOLD),
                Item::Text(t, GREEN),
            ])
        }),
        Goal::SoClose(left) => {
            let target = Item::Glyph(Glyph::Target, PAPER);
            let items = [flag, target, target, Item::Text("So close!", GREEN)];
            if left >= 2 {
                out(&items);
            } else {
                out(&[flag, target, Item::Text("So close!", GREEN)]);
            }
        }
        Goal::Supports => out(&[
            flag,
            Item::Glyph(Glyph::Icon(Icon::ArrowDown), GOLD),
            Item::Glyph(Glyph::Support, PAPER),
            Item::Glyph(Glyph::Support, PAPER),
        ]),
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
            emit(
                Row::Items(
                    &[
                        Item::Glyph(Glyph::Icon(Icon::Trophy), GOLD),
                        Item::Text("New best!", GOLD),
                    ],
                    20.0,
                ),
                3,
            );
        } else if results.new_stars {
            emit(
                Row::Items(
                    &[
                        Item::Glyph(Glyph::Star(true), GOLD),
                        Item::Text("New stars!", GOLD),
                    ],
                    20.0,
                ),
                3,
            );
        } else if let Some(best) = results.previous.points {
            with_fmt(format_args!("{}", Grouped(best)), |t| {
                emit(
                    Row::Items(
                        &[
                            Item::Glyph(Glyph::Icon(Icon::Trophy), MUTED),
                            Item::Text(t, MUTED),
                        ],
                        20.0,
                    ),
                    3,
                )
            });
        }
        with_fmt(format_args!("{}", tally.misses), |misses| {
            if run.won {
                with_fmt(format_args!("{}", tally.shots), |shots| {
                    with_fmt(format_args!("×{}", tally.best_chain.max(1)), |chain| {
                        emit(
                            Row::Items(
                                &[
                                    Item::Glyph(Glyph::Shell, PAPER),
                                    Item::Text(shots, MUTED),
                                    Item::Gap(0.7),
                                    Item::Glyph(Glyph::Target, PAPER),
                                    Item::Text(chain, MUTED),
                                    Item::Gap(0.7),
                                    Item::Glyph(Glyph::Icon(Icon::Cloud), MUTED),
                                    Item::Text(misses, MUTED),
                                ],
                                22.0,
                            ),
                            4,
                        )
                    })
                });
            } else {
                with_fmt(format_args!("{}/{GOAL}", run.targets), |targets| {
                    emit(
                        Row::Items(
                            &[
                                Item::Glyph(Glyph::Target, PAPER),
                                Item::Text(targets, MUTED),
                                Item::Gap(0.7),
                                Item::Glyph(Glyph::Icon(Icon::Cloud), MUTED),
                                Item::Text(misses, MUTED),
                            ],
                            22.0,
                        ),
                        4,
                    )
                });
            }
        });
        emit(Row::Gap(4.0), 5);
        emit(Row::Display(rank.title, 24.0, GOLD), 1);
        emit(Row::Text(rank.quip, 16.0, PAPER), 3);
        goal_items(next_goal(run, results.previous, points), &mut |items| {
            emit(Row::Items(items, 22.0), 2)
        });
        emit(Row::Gap(4.0), 5);
        emit(Row::Prompt(view.hints.confirm.as_str(), Icon::Restart), 0);
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
            next_goal(won, Best::default(), 900),
            Goal::NextStar { shots: 3 }
        );
        let three = RunSummary { stars: 3, ..won };
        assert_eq!(next_goal(three, Best::default(), 900), Goal::Secret);
        let rung = RunSummary {
            bell: true,
            ..three
        };
        assert_eq!(next_goal(rung, Best::default(), 900), Goal::Domino);
        let all = RunSummary {
            domino: true,
            ..rung
        };
        let best = Best {
            points: Some(2000),
            stars: 3,
        };
        assert_eq!(next_goal(all, best, 1500), Goal::BeatBest(2001));
        assert_eq!(next_goal(all, best, 2500), Goal::BeatBest(2501));
        let lost = RunSummary {
            targets: 5,
            ..Default::default()
        };
        assert_eq!(next_goal(lost, Best::default(), 0), Goal::SoClose(1));
        assert_eq!(
            next_goal(RunSummary::default(), Best::default(), 0),
            Goal::Supports
        );
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
        let mut lines: Vec<(Option<Glyph>, String, Color, bool)> = Vec::new();
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
            // A delayed topple pays nothing new, so it shows no points chip.
            let paid = callout != Callout::Delayed;
            callout_face(callout, &mut |icon, title, color| {
                lines.push((icon, title.into(), color, paid))
            });
        }
        for w in [320.0_f32, 390.0, 844.0, 1280.0, 1600.0] {
            let max = w - 64.0;
            for (icon, title, color, paid) in &lines {
                // One line with a four-digit payout, at the size the banner picks.
                let mut items = [Item::Gap(0.0); 4];
                let (n, size) = fit_callout(
                    *icon,
                    title,
                    *color,
                    paid.then_some("+1400"),
                    &[24.0, 16.0],
                    max,
                    &mut items,
                );
                assert!(size >= 16.0);
                assert!(glyph::row_width(&items[..n], size) <= max, "{title} at {w}");
            }
            // Card text fits at some size, or wraps onto two lines that fit.
            let inner = (w - 32.0).min(500.0) - 40.0;
            let mut copy: Vec<&str> = vec![];
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
            // Worst-case copy: a four-digit score (every bonus at once stays
            // well under 10,000 points).
            let bar = TopBar::new(w, "9,999");
            assert!(bar.compact || bar.right.is_some(), "score shown at {w}x{h}");
            for rect in bar.rects() {
                clear(rect, "top bar");
                assert!(rect.y + rect.h <= 56.0, "top bar stays above the scene");
            }
            if bar.compact {
                let used = 14.0 + towers_width(bar.cell) + 8.0;
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
