//! The yard's screen furniture, drawn icon-first: a top bar with goal pips, a
//! progress bar and the key for the tool the goal needs, headline toasts and
//! world-anchored pops, projected shockwave rings, the paused chip, the tool
//! palette with key caps, an icon status row, a drag-to-orbit demonstration,
//! the optional engine stats panel (G) and the YARD CERTIFIED card. Owns
//! personal bests. Drawing reads lab state and never changes it; storage
//! failure leaves a fully playable lab, and autoplay never writes a best.
use super::juice::{GOLD, Juice, MINT, PINK};
use super::look::{
    self, Glyph, INK, Icon, Line, RED, ToolFace, ToolMotion, ToolState, faded, fmt_line, modern,
};
use super::quips;
use super::toys::{AVALANCHE_BONKS, Fun, Goal, STACK};
use super::{BUTTONS, Inspector, Lab, MAX_BODIES, Notice};
use ferravex_play::tween::Motion;
use ferravex_render::icons::Icon as Pict;
use ferravex_render::input::PlayerInput;
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Fill, Shadow, Stroke};
use ferravex_render::storage::Records;
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_render::ui;

const RECORD_RULES: u64 = 0x5048_3344_0001;
const CARD_LIFE: f32 = 12.;
const NOTE_LIFE: f32 = 1.1;
/// Height of the status row above the tool buttons.
pub(super) const STATUS_ROW: f32 = 30.;

/// Palette entries per stable button index: (name, action, icon). Names show
/// only while a tool is hovered or selected.
pub(super) const TOOLS: [(&str, &str, Icon); BUTTONS] = [
    ("Crate", "select_one", Icon::Crate),
    ("Ball", "select_two", Icon::Ball),
    ("Drop", "drop", Icon::Drop),
    ("Pulse", "pulse", Icon::Pulse),
    ("Pause", "pause", Icon::Pause),
    ("Step", "step", Icon::Step),
    ("View", "reset_view", Icon::View),
    ("Reset", "restart", Icon::Reset),
    ("Bowl", "bowl", Icon::Bowl),
    ("Chaos", "chaos", Icon::Chaos),
];

/// Accent per stable button index.
pub(super) fn accent(index: usize) -> Color {
    match index {
        0 => ui::AMBER,
        1 => ui::CYAN,
        8 => Color::new(0.86, 0.42, 0.68, 1.),
        9 => PINK,
        _ => GOLD,
    }
}

/// The palette button that advances `goal`.
pub(super) fn goal_tool(goal: Goal) -> usize {
    match goal {
        Goal::Strike | Goal::CleanSweep => 8,
        Goal::Airtime => 3,
        Goal::Avalanche => 9,
    }
}

/// The first binding of `action` (Controls lists the rest), on the stack.
pub(super) fn hint(input: &PlayerInput, action: &str) -> Line<32> {
    let mut buffer = [0; 96];
    let text = input.format_hint(action, &mut buffer).unwrap_or("Controls");
    let mut line = Line::<32>::new();
    let _ = std::fmt::Write::write_str(&mut line, text.split(" / ").next().unwrap_or(text));
    line
}

fn short(args: std::fmt::Arguments) -> Line<16> {
    let mut line = Line::<16>::new();
    let _ = std::fmt::Write::write_fmt(&mut line, args);
    line
}

/// Browser fullscreen pins a 44 px menu button 8 px inside the canvas's
/// top-right corner. Text and panels stay out of x > width - 72, y < 60.
pub(super) const MENU_CORNER: Vec2 = Vec2::new(72., 60.);

/// Goal block `(x, y, first_row_width, full_width)`. The first row (pips,
/// headline and tool key) always stops short of the menu corner; on narrow
/// screens the progress row below y = 60 may use the full width.
fn goal_block(w: f32, wide: bool, narrow_y: f32) -> (f32, f32, f32, f32) {
    if wide {
        let width = (w - MENU_CORNER.x - 330.).min(620.);
        (w - MENU_CORNER.x - width, 0., width, width)
    } else {
        (16., narrow_y, w - 16. - MENU_CORNER.x, w - 32.)
    }
}

/// Engine stats panel, starting below the menu corner.
fn stats_rect(w: f32, rows: usize) -> Rect {
    let width = 290_f32.min(w - 32.);
    let top = if w >= 780. { MENU_CORNER.y + 4. } else { 98. };
    Rect::new(w - 16. - width, top, width, 38. + rows as f32 * 20.)
}

/// Widest centered toast content; toasts reaching above y = 60 stay clear of
/// the menu corner on both sides.
fn toast_max(w: f32, top: f32) -> f32 {
    let max = (w - 48.).min(640.);
    if top < MENU_CORNER.y {
        max.min(w - 2. * MENU_CORNER.x - 36.)
    } else {
        max
    }
}

/// Centered certification card, never above the menu corner band.
fn card_rect(w: f32, h: f32, height: f32) -> Rect {
    let width = (w - 32.).min(500.);
    let height = height.min(h - 32.);
    let top = (h * 0.5 - height * 0.5).max(MENU_CORNER.y + 4.);
    Rect::new(w * 0.5 - width * 0.5, top, width, height)
}

/// Room for the rank title starting at `x`, short of the menu corner.
fn rank_width(w: f32, x: f32) -> f32 {
    (w - MENU_CORNER.x - 8. - x).clamp(0., 300.)
}

fn width_of(text: &str, size: f32) -> f32 {
    text_width(text, TextStyle::modern(size, WHITE))
}

pub(super) fn fitted(text: &str, x: f32, y: f32, max_width: f32, size: f32, color: Color) -> f32 {
    let measured = width_of(text, size).max(1.);
    let scale = (max_width.max(1.) / measured).min(1.);
    draw_text_styled(text, x, y, TextStyle::modern(size * scale, color));
    measured * scale
}

fn centered(text: &str, cx: f32, y: f32, max_width: f32, size: f32, color: Color) {
    let width = width_of(text, size).min(max_width);
    fitted(text, cx - width * 0.5, y, max_width, size, color);
}

fn right(text: &str, x: f32, y: f32, max_width: f32, size: f32, color: Color) {
    let width = width_of(text, size).min(max_width);
    fitted(text, x - width, y, max_width, size, color);
}

/// Raised translucent surface shared by toasts, stats and the card.
fn surface(rect: Rect, radius: f32, alpha: f32, edge: Color) {
    shapes::drop_shadow(
        rect,
        radius,
        Shadow::soft(faded(Color::new(0., 0., 0., 0.35), alpha))
            .offset(vec2(0., 4.))
            .blur(14.),
    );
    shapes::fill_rounded_rect(
        rect,
        radius,
        Fill::vertical(
            rect,
            faded(Color::new(0.11, 0.16, 0.23, 0.97), alpha),
            faded(Color::new(0.07, 0.11, 0.17, 0.97), alpha),
        ),
    );
    shapes::stroke_rounded_rect(rect, radius, 1., faded(edge, alpha));
}

/// A beaten personal best, shown as a glyph and a value in its toast.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Best {
    pub glyph: Glyph,
    pub value: String,
}

/// Stored personal bests, compared against session values.
#[derive(Default)]
pub(super) struct Bests {
    records: Option<Records>,
    enabled: bool,
    pub airtime: Option<f32>,
    pub bonks: Option<u32>,
    /// Fewest bowls for a clean sweep.
    pub sweep: Option<u32>,
}

impl Bests {
    pub fn load(enabled: bool) -> Self {
        let records = Records::new("physics-3d").ok();
        let read = |id: &str| {
            records
                .as_ref()
                .and_then(|r| r.get(id, RECORD_RULES))
                .map(|r| r.score)
        };
        Self {
            airtime: read("airtime").map(|cm| cm as f32 / 100.),
            bonks: read("bonks").map(|n| n as u32),
            // Scores prefer larger values, so the sweep stores 1000 - bowls.
            sweep: read("sweep").map(|score| 1000_u64.saturating_sub(score) as u32),
            records,
            enabled,
        }
    }

    /// Store improvements. A best is returned only when a stored best existed
    /// and was beaten, so a first visit never claims a record.
    pub fn update(&mut self, fun: &Fun) -> Option<Best> {
        if !self.enabled {
            return None;
        }
        let mut beaten = None;
        let store = |records: &mut Option<Records>, id: &str, score: u64| {
            if let Some(records) = records.as_mut() {
                let _ = records.attempt(id, RECORD_RULES, score, None);
            }
        };
        if fun.airtime >= 5. && self.airtime.is_none_or(|best| fun.airtime > best + 0.25) {
            if self.airtime.is_some() {
                beaten = Some(Best {
                    glyph: look::goal_glyph(Goal::Airtime),
                    value: format!("{:.1} m", fun.airtime),
                });
            }
            self.airtime = Some(fun.airtime);
            store(&mut self.records, "airtime", (fun.airtime * 100.) as u64);
        }
        if fun.bonk_peak >= 20 && self.bonks.is_none_or(|best| fun.bonk_peak > best) {
            if self.bonks.is_some() {
                beaten = Some(Best {
                    glyph: look::goal_glyph(Goal::Avalanche),
                    value: format!("{}/s", fun.bonk_peak),
                });
            }
            self.bonks = Some(fun.bonk_peak);
            store(&mut self.records, "bonks", u64::from(fun.bonk_peak));
        }
        if let Some(bowls) = fun.sweep_best
            && self.sweep.is_none_or(|best| bowls < best)
        {
            if self.sweep.is_some() {
                beaten = Some(Best {
                    glyph: Glyph::Tool(Icon::Bowl),
                    value: bowls.to_string(),
                });
            }
            self.sweep = Some(bowls);
            store(&mut self.records, "sweep", 1000 - u64::from(bowls.min(999)));
        }
        beaten
    }

    /// The "one more try" hook once every goal is done: a glyph and at most a
    /// word or two with a number.
    pub fn next_hook(&self, fun: &Fun) -> (Glyph, Line<32>) {
        let mut line = Line::<32>::new();
        let text = |line: &mut Line<32>, args: std::fmt::Arguments| {
            let _ = std::fmt::Write::write_fmt(line, args);
        };
        if !fun.secret {
            text(&mut line, format_args!("Pedestal?"));
            return (Glyph::Pict(Pict::Trophy), line);
        }
        match self.sweep {
            Some(1) => text(&mut line, format_args!("1 again?")),
            Some(best) => text(&mut line, format_args!("Beat {best}")),
            None => text(&mut line, format_args!("Sweep it!")),
        }
        (Glyph::Tool(Icon::Bowl), line)
    }
}

/// A short popup over the tool that accepted or refused a command.
struct Note {
    tool: usize,
    glyph: Glyph,
    text: Line<16>,
    color: Color,
    age: f32,
}

pub(super) struct Hud {
    pub stats: bool,
    pub card: Option<f32>,
    /// Onboarding emphasis (beckoning tools), fading once learned.
    hint: f32,
    /// The drag-to-orbit demonstration, fading after the first orbit.
    orbit_hint: f32,
    orbited: bool,
    active: f32,
    tools: [ToolMotion; BUTTONS],
    notes: [Option<Note>; 3],
    /// The last lab notice already reacted to.
    seen: u32,
    /// Red flash on the body counter after a capacity refusal.
    full: f32,
    time: f32,
    motion: Motion,
}

impl Default for Hud {
    fn default() -> Self {
        Self {
            stats: false,
            card: None,
            hint: 1.,
            orbit_hint: 1.,
            orbited: false,
            active: 0.,
            tools: [ToolMotion::default(); BUTTONS],
            notes: Default::default(),
            seen: 0,
            full: 0.,
            time: 0.,
            motion: Motion::Full,
        }
    }
}

pub(super) struct Frame<'a> {
    pub lab: &'a Lab,
    pub juice: &'a Juice,
    pub bests: &'a Bests,
    pub input: &'a PlayerInput,
    pub panel: Rect,
    pub buttons: [Rect; BUTTONS],
    pub tools: [ToolState; BUTTONS],
    pub view_proj: Mat4,
    pub inspector: &'a Inspector,
    pub active: bool,
}

/// Fades `value` toward 1 while `show` holds and toward 0 otherwise.
fn approach(value: &mut f32, show: bool, dt: f32) {
    let target = if show { 1. } else { 0. };
    *value += (target - *value).clamp(-dt * 1.5, dt * 4.);
}

/// Projects a world point to screen, or `None` behind the camera.
fn project(view_proj: Mat4, point: Vec3) -> Option<Vec2> {
    let clip = view_proj * point.extend(1.);
    if clip.w <= 0.01 {
        return None;
    }
    Some(vec2(
        (clip.x / clip.w + 1.) * screen_width() * 0.5,
        (1. - clip.y / clip.w) * screen_height() * 0.5,
    ))
}

/// A horizontal world circle drawn as a glowing screen-space loop.
fn floor_ring(view_proj: Mat4, center: Vec3, radius: f32, color: Color) {
    let mut points = [Vec2::ZERO; 28];
    for (i, point) in points.iter_mut().enumerate() {
        let a = i as f32 / 28. * std::f32::consts::TAU;
        let Some(screen) = project(
            view_proj,
            center + vec3(a.cos() * radius, 0., a.sin() * radius),
        ) else {
            return;
        };
        *point = screen;
    }
    shapes::polyline(
        &points,
        Stroke::new(6., faded(color, 0.25 * color.a)).closed(true),
    );
    shapes::polyline(&points, Stroke::new(2., color).closed(true));
}

/// Shockwave rings from the juice and the pulse, drawn over the 3D scene.
pub(super) fn rings(juice: &Juice, pulse: Option<(Vec3, f32)>, intensity: f32, view_proj: Mat4) {
    for ring in juice.rings.iter().flatten() {
        let (radius, alpha) = ring.look();
        floor_ring(view_proj, ring.at, radius, faded(ring.color, alpha));
    }
    if let Some((center, remaining)) = pulse {
        let radius = (0.65 - remaining) * 12.;
        let fade = (remaining / 0.65).clamp(0., 1.).sqrt();
        floor_ring(
            view_proj,
            center + vec3(0., 0.02, 0.),
            radius,
            faded(ui::CYAN, intensity * fade),
        );
    }
}

impl Hud {
    /// Onboarding stays until the player has bowled and pulsed, or 30 seconds
    /// of active play pass; then it fades out.
    pub fn learned(fun: &Fun, active_seconds: f32) -> bool {
        (fun.stats.bowls > 0 && fun.stats.pulses > 0) || active_seconds > 30.
    }

    /// Advance HUD motion by bounded render time and react to the lab's new
    /// notice. `orbited` reports a deliberate camera orbit this frame.
    pub fn advance(
        &mut self,
        lab: &Lab,
        active: bool,
        dt: f32,
        motion: Motion,
        tools: &[ToolState; BUTTONS],
        orbited: bool,
    ) {
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.1)
        } else {
            0.
        };
        self.motion = motion;
        self.time += dt;
        if active && lab.session.is_playing() {
            self.active += dt;
        }
        self.orbited |= orbited;
        approach(&mut self.hint, !Self::learned(&lab.fun, self.active), dt);
        approach(
            &mut self.orbit_hint,
            !self.orbited && self.active <= 30.,
            dt,
        );
        for (motion_state, state) in self.tools.iter_mut().zip(tools) {
            motion_state.update(*state, dt, motion);
        }
        for slot in &mut self.notes {
            if let Some(note) = slot {
                note.age += dt;
                if note.age >= NOTE_LIFE {
                    *slot = None;
                }
            }
        }
        self.full = (self.full - dt * 2.).max(0.);
        if lab.notice_serial != self.seen {
            self.seen = lab.notice_serial;
            self.react(lab, lab.notice);
        }
        if let Some(age) = &mut self.card {
            *age += dt;
            if *age > CARD_LIFE {
                self.card = None;
            }
        }
        if !lab.fun.events.goals.is_empty() && lab.fun.goals_done() == 4 {
            self.card = Some(0.);
        }
    }

    /// Feedback for a command: refusals wobble the tool red and pop a short
    /// note over it; manual steps pop a quiet "+1".
    fn react(&mut self, lab: &Lab, notice: Notice) {
        let cross = Glyph::Pict(Pict::Cross);
        let pause = Glyph::Tool(Icon::Pause);
        let (tool, glyph, word, color, refused) = match notice {
            Notice::DropFull => (2, cross, "Full", RED, true),
            Notice::BowlFull => (8, cross, "Full", RED, true),
            Notice::ChaosFull => (9, cross, "Full", RED, true),
            Notice::PulseRecharging => (3, Pict::Hourglass.into(), "", ui::AMBER, true),
            Notice::PulsePaused => (3, pause, "Paused", ui::AMBER, true),
            Notice::StepWhileRunning => (5, pause, "Pause first", ui::AMBER, true),
            Notice::Stepped => (5, Pict::Clock.into(), "+1", ui::CYAN, false),
            _ => return,
        };
        // A recharge refusal shows the seconds left instead of a word.
        let text = if notice == Notice::PulseRecharging {
            short(format_args!("{:.1} s", lab.sandbox.pulse_time))
        } else {
            short(format_args!("{word}"))
        };
        if refused {
            self.tools[tool].deny();
        }
        if matches!(
            notice,
            Notice::DropFull | Notice::BowlFull | Notice::ChaosFull
        ) {
            self.full = 1.;
        }
        // One note per tool: a repeat refreshes it instead of stacking.
        let slot = self
            .notes
            .iter()
            .position(|n| n.as_ref().is_some_and(|n| n.tool == tool))
            .or_else(|| self.notes.iter().position(Option::is_none))
            .unwrap_or(0);
        self.notes[slot] = Some(Note {
            tool,
            glyph,
            text,
            color,
            age: 0.,
        });
    }

    /// A click on a disabled tool: say why with the same refusal feedback.
    pub fn refuse(&mut self, lab: &Lab, index: usize) {
        let notice = match index {
            3 if lab.session.is_paused() => Notice::PulsePaused,
            3 => Notice::PulseRecharging,
            5 => Notice::StepWhileRunning,
            8 => Notice::BowlFull,
            9 => Notice::ChaosFull,
            _ => return,
        };
        self.react(lab, notice);
    }

    pub fn tool_kick(&mut self, index: usize) {
        if let Some(tool) = self.tools.get_mut(index) {
            tool.kick();
        }
    }

    pub fn dismiss_card(&mut self) -> bool {
        if self.card.is_some_and(|age| age > 0.6) {
            self.card = None;
            return true;
        }
        false
    }

    pub fn draw(&self, f: &Frame) {
        self.orbit_demo(f);
        self.top_bar(f);
        self.pops(f);
        self.toast(f);
        self.footer(f);
        if self.stats {
            self.diagnostics(f);
        }
        if let Some(age) = self.card {
            self.certificate(f, age);
        }
    }

    fn breathe(&self, hz: f32) -> f32 {
        if self.motion.is_reduced() {
            1.
        } else {
            0.5 + 0.5 * (self.time * hz * std::f32::consts::TAU).sin()
        }
    }

    fn top_bar(&self, f: &Frame) {
        let w = screen_width();
        let fun = &f.lab.fun;
        let wide = w >= 780.;
        let band = Rect::new(0., 0., w, 44.);
        shapes::fill_rounded_rect(
            band,
            0.,
            Fill::vertical(
                band,
                Color::from_rgba(18, 27, 41, 240),
                Color::from_rgba(10, 16, 26, 235),
            ),
        );
        draw_line(0., 44., w, 44., 1., Color::new(1., 1., 1., 0.08));
        fitted("3D Physics Lab", 16., 21., 220., 16., INK);
        // The yard and the rank: names beside their pictograms.
        let rank = quips::rank(fun.goals_done(), fun.secret);
        look::glyph(Pict::Crate, vec2(21., 33.), 10., ui::AMBER);
        let used = fitted("Loading Yard", 30., 37., 120., 8., ui::AMBER);
        let crown = 30. + used + 14.;
        look::glyph(Pict::Crown, vec2(crown, 33.), 10., ui::MUTED);
        fitted(
            rank.title,
            crown + 9.,
            37.,
            rank_width(w, crown + 9.),
            8.,
            ui::MUTED,
        );
        // Wide: goal block right of the title. Narrow: a translucent strip
        // across the top of the scene.
        if !wide {
            draw_rectangle(0., 44., w, 46., Color::from_rgba(12, 19, 30, 190));
        }
        let (x, y, row, width) = goal_block(w, wide, 46.);
        let cy = y + 15.;
        let low = y + 34.;
        let pips = look::pips(x, cy, &fun.goals, fun.current_goal().map(Goal::index), GOLD);
        let head_x = x + pips + 14.;
        let Some(goal) = fun.current_goal() else {
            // Every goal done: the rank, then a one-more-try hook.
            look::glyph(Pict::Trophy, vec2(head_x + 8., cy), 16., MINT);
            fitted(
                rank.title,
                head_x + 22.,
                cy + 8.,
                (x + row - head_x - 22.).max(1.),
                16.,
                MINT,
            );
            let (glyph, text) = f.bests.next_hook(fun);
            look::icon_value(glyph, text.as_str(), x, low, 14., modern(16., PINK));
            return;
        };
        let (value, target) = fun.progress(goal);
        // Row 1: pips, goal pictogram and name, then the tool that helps.
        look::glyph(look::goal_glyph(goal), vec2(head_x + 8., cy), 16., GOLD);
        let tool = goal_tool(goal);
        let key = hint(f.input, TOOLS[tool].1);
        let chip_w = 26. + look::keycap_width(key.as_str());
        let title_w = fitted(
            goal.title(),
            head_x + 22.,
            cy + 8.,
            (x + row - chip_w - head_x - 34.).max(1.),
            16.,
            GOLD,
        );
        let at = head_x + 22. + title_w + 14.;
        let glow = if self.hint > 0.01 {
            0.2 + 0.25 * self.breathe(0.9)
        } else {
            0.12
        };
        shapes::glow_disc(vec2(at + 9., cy), 14., faded(accent(tool), glow));
        look::glyph(TOOLS[tool].2, vec2(at + 9., cy), 15., accent(tool));
        look::keycap(at + 22., cy, key.as_str(), 1.);
        // Row 2: a bar toward the target flag, then the live value.
        let (readout, target_text) = match goal {
            Goal::Airtime => (fmt_line!("{value:.1} m"), fmt_line!("{target:.0} m")),
            Goal::Avalanche => (fmt_line!("{value:.0}/s"), fmt_line!("{target:.0}/s")),
            Goal::Strike | Goal::CleanSweep => (fmt_line!("{value:.0}"), fmt_line!("{target:.0}")),
        };
        let value_style = modern(16., INK);
        let flag_style = modern(16., faded(GOLD, 0.85));
        let tail = text_width(readout.as_str(), value_style)
            + 10.
            + look::icon_value_width(target_text.as_str(), 14., flag_style);
        let track = Rect::new(x, low - 2., (width - tail - 12.).max(8.), 4.);
        let count = target.max(1.) as usize;
        if goal == Goal::Strike {
            // A countable goal reads as one pip per crate.
            let gap = 2.;
            let seg = ((track.w - gap * (count - 1) as f32) / count as f32).max(1.);
            for i in 0..count {
                let cell = Rect::new(track.x + i as f32 * (seg + gap), track.y, seg, track.h);
                let lit = (i as f32) < value;
                shapes::fill_rounded_rect(
                    cell,
                    1.5,
                    if lit {
                        GOLD
                    } else {
                        Color::new(1., 1., 1., 0.12)
                    },
                );
            }
        } else {
            look::progress(track, value / target, GOLD);
            if value > 0. {
                let head = track.x + track.w * (value / target).clamp(0., 1.);
                shapes::glow_disc(vec2(head, low), 6., faded(GOLD, 0.7));
            }
        }
        let tx = track.x + track.w + 12.;
        let used = draw_text_styled(
            readout.as_str(),
            tx,
            value_style.centered_baseline(low),
            value_style,
        )
        .width;
        look::icon_value(
            Pict::Flag,
            target_text.as_str(),
            tx + used + 10.,
            low,
            14.,
            flag_style,
        );
    }

    fn pops(&self, f: &Frame) {
        let w = screen_width();
        for pop in f.juice.pops.iter().flatten() {
            let Some(at) = project(f.view_proj, pop.at) else {
                continue;
            };
            let t = pop.age / pop.life;
            let alpha = if t > 0.7 { (1. - t) / 0.3 } else { 1. };
            let size = (pop.size * (w / 900.).clamp(0.7, 1.)).min(pop.size);
            let width = width_of(&pop.text, size).min(w - 24.);
            let at = vec2(
                at.x.clamp(12. + width * 0.5, w - 12. - width * 0.5),
                at.y.clamp(60., f.panel.y - 10.) - f.juice.pop_rise(pop),
            );
            let height = size * 7. / 8.;
            // Pops never rise into the top bar or the menu corner.
            let at = vec2(at.x, at.y.max(MENU_CORNER.y + height + 9.));
            let chip = Rect::new(
                at.x - width * 0.5 - 8.,
                at.y - height - 6.,
                width + 16.,
                height + 12.,
            );
            shapes::fill_rounded_rect(
                chip,
                chip.h * 0.5,
                faded(Color::from_rgba(10, 16, 26, 255), alpha * 0.82),
            );
            shapes::stroke_rounded_rect(chip, chip.h * 0.5, 1., faded(pop.color, alpha * 0.45));
            fitted(
                &pop.text,
                at.x - width * 0.5,
                at.y,
                width,
                size,
                faded(pop.color, alpha),
            );
        }
    }

    fn toast(&self, f: &Frame) {
        let w = screen_width();
        let mut top = if w >= 780. { 58. } else { 104. };
        if self.stats && w < 780. {
            // The stats panel spans narrow screens; callouts go below it.
            top = 98. + 38. + 6. * 20. + 8.;
        }
        // With engine stats open, center in the space left of that panel.
        let room = if self.stats && w >= 780. { w - 336. } else { w };
        let max = toast_max(room, top);
        let Some(toast) = &f.juice.toast else {
            if f.lab.session.is_paused() && f.active {
                self.paused_chip(f, room * 0.5, top);
            }
            return;
        };
        let alpha = toast.alpha();
        let color = toast.color;
        let glyph_w = if toast.glyph.is_some() { 30. } else { 0. };
        let value_style = modern(16., INK);
        let value_w = toast.value.as_ref().map_or(0., |(_, text)| {
            look::icon_value_width(text, 16., value_style) + 18.
        });
        let head_size = 24_f32.min(24. * (max - glyph_w).max(40.) / width_of(&toast.head, 24.));
        let head_w = width_of(&toast.head, head_size);
        // One row when it fits; otherwise the icon stat drops below.
        let one_row = glyph_w + head_w + value_w <= max;
        let content = if one_row {
            glyph_w + head_w + value_w
        } else {
            (glyph_w + head_w).max(value_w - 18.)
        };
        let width = content.min(max) + 36.;
        let height = if one_row || toast.value.is_none() {
            46.
        } else {
            70.
        };
        let rect = Rect::new(room * 0.5 - width * 0.5, top, width, height);
        // The hairline carries the event colour.
        surface(rect, 14., alpha, faded(color, 0.55));
        let row_w = if one_row {
            glyph_w + head_w + value_w
        } else {
            glyph_w + head_w
        };
        let mut x = rect.center().x - row_w * 0.5;
        let cy = top + 23.;
        if let Some(glyph) = toast.glyph {
            look::glyph(glyph, vec2(x + 12., cy), 22., faded(color, alpha));
            x += glyph_w;
        }
        let head = TextStyle::modern(head_size, faded(color, alpha)).with_shadow(Color::new(
            0.,
            0.,
            0.,
            0.55 * alpha,
        ));
        draw_text_styled(&toast.head, x, head.centered_baseline(cy), head);
        x += head_w + 18.;
        if let Some((glyph, text)) = &toast.value {
            let style = value_style.with_color(faded(INK, alpha));
            if one_row {
                look::icon_value(*glyph, text, x, cy, 16., style);
            } else {
                let vw = look::icon_value_width(text, 16., style);
                look::icon_value(
                    *glyph,
                    text,
                    rect.center().x - vw * 0.5,
                    cy + 25.,
                    16.,
                    style,
                );
            }
        }
    }

    /// While paused: a calm chip with the keys that matter now. The hand
    /// says the camera still orbits.
    fn paused_chip(&self, f: &Frame, cx: f32, top: f32) {
        let step = hint(f.input, "step");
        let head = modern(16., ui::AMBER);
        let head_w = text_width("Paused", head);
        let step_w = if look::shows_key(step.as_str()) {
            look::keycap_width(step.as_str()) + 5. + 20.
        } else {
            0.
        };
        let width = 20. + head_w + 16. + step_w + 30. + 24.;
        let rect = Rect::new(cx - width * 0.5, top, width, 32.);
        surface(rect, 16., 1., faded(ui::AMBER, 0.45));
        let cy = rect.center().y;
        let mut x = rect.x + 14.;
        look::glyph(Icon::Pause, vec2(x + 7., cy), 14., ui::AMBER);
        x += 20.;
        x += draw_text_styled("Paused", x, head.centered_baseline(cy), head).width + 16.;
        if step_w > 0. {
            x += look::keycap(x, cy, step.as_str(), 1.) + 5.;
            look::glyph(Icon::Step, vec2(x + 8., cy), 15., INK);
            x += 26.;
        }
        look::glyph(Pict::Hand, vec2(x + 9., cy), 16., faded(INK, 0.85));
    }

    /// Until the player first orbits: a hand sweeping along an arc across the
    /// lower scene, showing that dragging turns the yard.
    fn orbit_demo(&self, f: &Frame) {
        let alpha = self.orbit_hint;
        if alpha <= 0.01 || self.card.is_some() || f.juice.toast.is_some() {
            return;
        }
        let w = screen_width();
        let base = vec2(w * 0.5, f.panel.y - 64.);
        let span = (w * 0.18).clamp(60., 140.);
        let mut arc = [Vec2::ZERO; 16];
        for (i, point) in arc.iter_mut().enumerate() {
            let t = i as f32 / 15.;
            *point = base + vec2((t - 0.5) * 2. * span, -(1. - (2. * t - 1.).powi(2)) * 18.);
        }
        shapes::polyline(
            &arc,
            Stroke::new(2., faded(ui::CYAN, 0.55 * alpha)).cap(shapes::Cap::Round),
        );
        for (tip, back) in [(arc[0], arc[2]), (arc[15], arc[13])] {
            let d = (tip - back).normalize_or_zero();
            let n = vec2(-d.y, d.x);
            shapes::fill_triangle(
                tip + d * 5.,
                tip - d * 4. + n * 5.,
                tip - d * 4. - n * 5.,
                faded(ui::CYAN, 0.7 * alpha),
            );
        }
        let sweep = if self.motion.is_reduced() {
            0.5
        } else {
            0.5 + 0.5 * (self.time * 1.3).sin()
        };
        let i = ((sweep * 15.).round() as usize).min(15);
        look::glyph(
            Pict::Hand,
            arc[i] + vec2(6., 16.),
            30.,
            faded(INK, 0.9 * alpha),
        );
    }

    fn footer(&self, f: &Frame) {
        let lab = f.lab;
        let panel = f.panel;
        let w = screen_width();
        shapes::fill_rounded_rect(panel, 12., Color::new(0.06, 0.10, 0.15, 0.95));
        shapes::stroke_rounded_rect(panel, 12., 1., ui::LINE);
        let single_row = f.buttons.iter().all(|b| b.y == f.buttons[0].y);
        if single_row {
            for (left, right) in [(2, 3), (9, 4)] {
                let a = f.buttons[left];
                let b = f.buttons[right];
                let x = (a.x + a.w + b.x) * 0.5;
                draw_rectangle(
                    x - 0.5,
                    a.y + 6.,
                    1.,
                    a.h - 12.,
                    Color::new(1., 1., 1., 0.1),
                );
            }
        }
        let beckon = lab
            .fun
            .current_goal()
            .map(goal_tool)
            .filter(|&index| match index {
                8 => lab.fun.stats.bowls == 0,
                3 => lab.fun.stats.pulses == 0,
                9 => lab.fun.stats.chaos == 0,
                _ => false,
            });
        let mut bubble = None;
        for (index, (label, action, glyph)) in TOOLS.iter().enumerate() {
            let paused = index == 4 && lab.session.is_paused();
            let (label, glyph) = if paused {
                ("Resume", Icon::Play)
            } else {
                (*label, *glyph)
            };
            let key = hint(f.input, action);
            let motion = &self.tools[index];
            let deny = motion.denied();
            let face = ToolFace {
                glyph,
                label,
                key: key.as_str(),
                accent: accent(index),
                charge: (index == 3 && lab.sandbox.pulse_time > 0.)
                    .then(|| 1. - lab.sandbox.pulse_time / 0.65),
                beckon: if beckon == Some(index) {
                    (0.35 + 0.65 * self.breathe(0.9)) * self.hint.max(0.5)
                } else {
                    0.
                },
                shake: if self.motion.is_reduced() {
                    0.
                } else {
                    (deny * 40.).sin() * 5. * deny
                },
            };
            if look::tool_button(f.buttons[index], &face, f.tools[index], motion) {
                bubble = Some((label, f.buttons[index]));
            }
        }
        // While learning, a bobbing chevron points at the tool the goal needs.
        if let Some(index) = beckon
            && f.tools[index].enabled
            && self.hint > 0.01
            && bubble.is_none()
            && single_row
        {
            let rect = f.buttons[index];
            let bob = if self.motion.is_reduced() {
                0.
            } else {
                (self.time * 4.).sin() * 2.
            };
            let tip = vec2(rect.center().x, rect.y - 3. + bob);
            shapes::fill_triangle(
                tip + vec2(-7., -7.),
                tip + vec2(7., -7.),
                tip,
                faded(accent(index), self.hint),
            );
        }
        if let Some((label, rect)) = bubble {
            look::bubble(label, vec2(rect.center().x, rect.y - 4.));
        }
        for note in self.notes.iter().flatten() {
            let rect = f.buttons[note.tool];
            self.note(note, vec2(rect.center().x, rect.y - 8.));
        }
        // Status as icon readouts: run state, the body count against its cap,
        // and the stats key.
        let cy = panel.y + STATUS_ROW * 0.5 + 1.;
        let (state, color): (Glyph, Color) = if !f.active {
            (Pict::EyeOff.into(), ui::MUTED)
        } else if lab.session.is_paused() {
            (Icon::Pause.into(), ui::AMBER)
        } else {
            (Icon::Play.into(), MINT)
        };
        let dot = vec2(panel.x + 22., cy);
        shapes::glow_disc(dot, 11., faded(color, 0.35));
        look::glyph(state, dot, 12., color);
        let count = lab.sandbox.bodies.len();
        let warn = if count >= MAX_BODIES { 1. } else { self.full };
        let ink = shapes::mix(INK, RED, warn);
        let bodies = fmt_line!("{count}");
        let x = panel.x + 42.;
        let used = look::icon_value(Icon::Shapes, bodies.as_str(), x, cy, 14., modern(16., ink));
        look::progress(
            Rect::new(x + used + 8., cy - 2., 64., 4.),
            count as f32 / MAX_BODIES as f32,
            shapes::mix(ui::CYAN, RED, warn),
        );
        let key = hint(f.input, "stats");
        let key_w = look::keycap_width(key.as_str());
        if key_w > 0. {
            let lit = if self.stats { 1. } else { 0. };
            let right = w - 32.;
            look::glyph(
                Pict::Gear,
                vec2(right - 8., cy),
                14.,
                shapes::mix(faded(ui::MUTED, 0.8), ui::CYAN, lit),
            );
            look::keycap(right - 20. - key_w, cy, key.as_str(), 0.6 + 0.4 * lit);
        }
    }

    /// One rising, fading tool note centered above `anchor`.
    fn note(&self, note: &Note, anchor: Vec2) {
        let t = (note.age / NOTE_LIFE).clamp(0., 1.);
        let alpha = if t > 0.6 { (1. - t) / 0.4 } else { 1. };
        let rise = if self.motion.is_reduced() {
            0.
        } else {
            (1. - (1. - t).powi(3)) * 16.
        };
        let style = modern(16., faded(note.color, alpha));
        let width = look::icon_value_width(note.text.as_str(), 16., style);
        let w = screen_width();
        let x = (anchor.x - width * 0.5).clamp(8., (w - width - 8.).max(8.));
        look::icon_value(
            note.glyph,
            note.text.as_str(),
            x,
            anchor.y - rise,
            16.,
            style,
        );
    }

    fn diagnostics(&self, f: &Frame) {
        let w = screen_width();
        let lab = f.lab;
        let physics = &lab.sandbox.physics;
        let sleep = physics.sleep_stats();
        let query = &f.inspector.query;
        let near = f.inspector.hits[0].map(|hit| hit.distance);
        // Each row is icon readouts: ticks, sleep, contacts, the camera ray,
        // pulse recharge and bonks against the Avalanche target.
        let clock = [
            (
                Glyph::Pict(Pict::Clock),
                fmt_line!("{}", lab.session.tick()),
            ),
            (Glyph::Pict(Pict::Bolt), fmt_line!("120 Hz · 8.33 ms")),
        ];
        let rest = [
            (Glyph::Pict(Pict::Sun), fmt_line!("{}", sleep.awake_bodies)),
            (
                Glyph::Pict(Pict::Moon),
                fmt_line!("{}", sleep.sleeping_bodies),
            ),
            (
                Glyph::Tool(Icon::Shapes),
                fmt_line!("{}/{MAX_BODIES}", lab.sandbox.bodies.len()),
            ),
        ];
        let contacts = [
            (
                Glyph::Tool(Icon::Contact),
                fmt_line!("{}", physics.contacts().count()),
            ),
            (
                Glyph::Pict(Pict::Bell),
                fmt_line!("{}", physics.contact_events().len()),
            ),
        ];
        let ray = [
            (
                Glyph::Pict(Pict::Search),
                fmt_line!(
                    "{}/{}{}",
                    query.written,
                    query.total,
                    if query.is_truncated() { "+" } else { "" }
                ),
            ),
            (
                Glyph::Pict(Pict::Target),
                near.map_or(fmt_line!("—"), |d| fmt_line!("{d:.1} m")),
            ),
        ];
        let charge = [(
            Glyph::Tool(Icon::Pulse),
            if lab.sandbox.pulse_time > 0. {
                fmt_line!("{:.2} s", lab.sandbox.pulse_time)
            } else {
                fmt_line!("ready")
            },
        )];
        let bonks = [
            (
                look::goal_glyph(Goal::Avalanche),
                fmt_line!("{}/s", lab.fun.bonks_now),
            ),
            (
                Glyph::Pict(Pict::Trophy),
                fmt_line!("{}", lab.fun.bonk_peak),
            ),
            (
                Glyph::Pict(Pict::Crate),
                fmt_line!("{}/{STACK}", lab.fun.toppled),
            ),
        ];
        let rows: [&[(Glyph, Line)]; 6] = [&clock, &rest, &contacts, &ray, &charge, &bonks];
        let rect = stats_rect(w, rows.len());
        surface(rect, 12., 1., Color::new(1., 1., 1., 0.08));
        let x = rect.x + 12.;
        look::icon_value(
            Pict::Gear,
            "Engine",
            x,
            rect.y + 18.,
            14.,
            modern(16., ui::AMBER),
        );
        let column = (rect.w - 24.) / 3.;
        for (row, cells) in rows.iter().enumerate() {
            let cy = rect.y + 42. + row as f32 * 20.;
            let mut cx = x;
            for (glyph, text) in cells.iter() {
                let used = look::icon_value(*glyph, text.as_str(), cx, cy, 13., modern(16., INK));
                cx += used.max(column - 6.) + 6.;
            }
        }
        let _ = AVALANCHE_BONKS;
    }

    fn certificate(&self, f: &Frame, age: f32) {
        let w = screen_width();
        let h = screen_height();
        let fun = &f.lab.fun;
        let alpha = (age / 0.3).min(1.) * ((CARD_LIFE - age) / 0.5).clamp(0., 1.);
        draw_rectangle(0., 0., w, h, faded(Color::new(0., 0., 0., 0.45), alpha));
        let rect = card_rect(w, h, 290.);
        let height = rect.h;
        surface(rect, 18., alpha, faded(GOLD, 0.55));
        shapes::fill_rounded_rect(
            Rect::new(rect.x + rect.w * 0.5 - 28., rect.y + 8., 56., 4.),
            2.,
            faded(GOLD, alpha),
        );
        let cx = rect.center().x;
        let inner = rect.w - 40.;
        let rank = quips::rank(fun.goals_done(), fun.secret);
        let banner_w = width_of("Yard certified", 16.);
        centered(
            "Yard certified",
            cx,
            rect.y + 36.,
            inner,
            16.,
            faded(ui::AMBER, alpha),
        );
        for side in [-1., 1.] {
            look::glyph(
                Pict::Star,
                vec2(cx + side * (banner_w * 0.5 + 14.), rect.y + 30.),
                14.,
                faded(GOLD, alpha),
            );
        }
        centered(rank.title, cx, rect.y + 70., inner, 24., faded(GOLD, alpha));
        centered(rank.quip, cx, rect.y + 96., inner, 16., faded(INK, alpha));
        // Session vs best, one pictogram per row, a trophy over the bests.
        let x0 = rect.x + 32.;
        let x1 = rect.x + rect.w * 0.6;
        let x2 = rect.x + rect.w - 24.;
        look::glyph(
            Pict::Trophy,
            vec2(x2 - 10., rect.y + 122.),
            14.,
            faded(ui::AMBER, alpha),
        );
        let best = |stored: Option<Line>| stored.unwrap_or_else(|| fmt_line!("—"));
        let rows = [
            (
                Glyph::Tool(Icon::Bowl),
                fun.sweep_best.map_or(fmt_line!("—"), |b| fmt_line!("{b}")),
                best(f.bests.sweep.map(|b| fmt_line!("{b}"))),
            ),
            (
                look::goal_glyph(Goal::Airtime),
                fmt_line!("{:.1} m", fun.airtime),
                best(f.bests.airtime.map(|b| fmt_line!("{b:.1} m"))),
            ),
            (
                look::goal_glyph(Goal::Avalanche),
                fmt_line!("{}/s", fun.bonk_peak),
                best(f.bests.bonks.map(|b| fmt_line!("{b}/s"))),
            ),
        ];
        for (row, (glyph, value, stored)) in rows.iter().enumerate() {
            let y = rect.y + 146. + row as f32 * 26.;
            if row % 2 == 0 {
                let stripe = Rect::new(rect.x + 14., y - 12., rect.w - 28., 24.);
                shapes::fill_rounded_rect(stripe, 6., Color::new(1., 1., 1., 0.035 * alpha));
            }
            look::glyph(*glyph, vec2(x0, y), 16., faded(INK, alpha));
            right(
                value.as_str(),
                x1 + 30.,
                y + 8.,
                110.,
                16.,
                faded(MINT, alpha),
            );
            right(
                stored.as_str(),
                x2,
                y + 8.,
                110.,
                16.,
                faded(ui::AMBER, alpha),
            );
        }
        // Session tallies as icon chips, then the one-more-try hook.
        let s = fun.stats;
        let chips = [
            (Glyph::Tool(Icon::Bowl), fmt_line!("{}", s.bowls)),
            (Glyph::Pict(Pict::Star), fmt_line!("{}", s.strikes)),
            (Glyph::Tool(Icon::Pulse), fmt_line!("{}", s.pulses)),
            (Glyph::Tool(Icon::Chaos), fmt_line!("{}", s.chaos)),
            (Glyph::Pict(Pict::Skull), fmt_line!("{}", s.collapses)),
        ];
        let style = modern(16., faded(ui::MUTED, alpha));
        let total: f32 = chips
            .iter()
            .map(|(_, text)| look::icon_value_width(text.as_str(), 14., style) + 16.)
            .sum::<f32>()
            - 16.;
        let mut x = cx - total * 0.5;
        let cy = rect.y + height - 48.;
        for (glyph, text) in &chips {
            x += look::icon_value(*glyph, text.as_str(), x, cy, 14., style) + 16.;
        }
        let (glyph, hook) = f.bests.next_hook(fun);
        let hook_style = modern(16., faded(PINK, alpha));
        let hook_w = look::icon_value_width(hook.as_str(), 16., hook_style);
        look::icon_value(
            glyph,
            hook.as_str(),
            cx - hook_w * 0.5,
            rect.y + height - 22.,
            16.,
            hook_style,
        );
        let _ = TextAlign::Center;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_menu_corner_stays_clear_at_audited_sizes() {
        let sizes = [
            (1280., 800.),
            (1600., 1000.),
            (390., 844.),
            (844., 390.),
            (320., 480.),
        ];
        for (w, h) in sizes {
            let clear = |right: f32, top: f32| right <= w - MENU_CORNER.x || top >= MENU_CORNER.y;
            let wide = w >= 780.;
            let (x, y, row, full) = goal_block(w, wide, 46.);
            // First row glyphs span roughly y + 5 ..= y + 23.
            assert!(clear(x + row, y + 5.), "{w}: goal row");
            assert!(full <= w - 32. && x >= 16.);
            if wide {
                assert_eq!(row, full, "wide progress row shares the corner band");
            }
            for start in [70., 140.] {
                assert!(
                    rank_width(w, start) + start <= w - MENU_CORNER.x,
                    "{w}: rank"
                );
            }
            let stats = stats_rect(w, 6);
            assert!(clear(stats.x + stats.w, stats.y), "{w}: stats");
            for top in [56., 58., 94., 104.] {
                let half = (toast_max(w, top) + 36.) * 0.5;
                assert!(clear(w * 0.5 + half, top), "{w}: toast at {top}");
                assert!(w * 0.5 - half >= 0.);
            }
            for height in [290., 318.] {
                let card = card_rect(w, h, height);
                assert!(clear(card.x + card.w, card.y), "{w}x{h}: card");
                assert!(card.y + card.h <= h, "{w}x{h}: card fits");
            }
        }
    }

    #[test]
    fn demo_runs_never_store_bests_and_first_records_are_silent() {
        let mut fun = Fun::default();
        fun.airtime = 9.;
        fun.bonk_peak = 70;
        fun.sweep_best = Some(2);
        let mut demo = Bests::default();
        assert!(demo.update(&fun).is_none());
        assert!(demo.airtime.is_none() && demo.sweep.is_none());
        let mut first = Bests {
            enabled: true,
            ..Default::default()
        };
        assert!(first.update(&fun).is_none(), "first visit claims nothing");
        assert_eq!(first.sweep, Some(2));
        fun.sweep_best = Some(1);
        let best = first.update(&fun).unwrap();
        assert_eq!(
            (best.glyph, best.value.as_str()),
            (Glyph::Tool(Icon::Bowl), "1")
        );
        assert!(first.update(&fun).is_none(), "no repeat headline");
    }

    #[test]
    fn hooks_tease_the_secret_then_the_sweep() {
        let mut fun = Fun::default();
        let bests = Bests {
            sweep: Some(3),
            ..Default::default()
        };
        let (glyph, text) = bests.next_hook(&fun);
        assert_eq!(glyph, Glyph::Pict(Pict::Trophy));
        assert_eq!(text.as_str(), "Pedestal?");
        fun.secret = true;
        let (glyph, text) = bests.next_hook(&fun);
        assert_eq!(glyph, Glyph::Tool(Icon::Bowl));
        assert_eq!(text.as_str(), "Beat 3");
    }

    #[test]
    fn onboarding_fades_after_bowling_and_pulsing_or_half_a_minute() {
        let mut fun = Fun::default();
        assert!(!Hud::learned(&fun, 1.));
        fun.stats.bowls = 1;
        fun.stats.pulses = 1;
        assert!(Hud::learned(&fun, 1.));
        assert!(Hud::learned(&Fun::default(), 31.));
    }

    #[test]
    fn every_tool_has_a_registered_action_a_short_name_and_a_key_cap() {
        let mut input =
            PlayerInput::preset("physics-3d-tools", ferravex_render::input::Preset::Lab3);
        crate::register_lab_actions(&mut input);
        for (label, action, _) in TOOLS {
            assert!(label.len() <= 6);
            let key = hint(&input, action);
            assert!(look::shows_key(key.as_str()), "{action}: {}", key.as_str());
        }
        for goal in Goal::ALL {
            let (_, action, _) = TOOLS[goal_tool(goal)];
            assert!(["bowl", "pulse", "chaos"].contains(&action), "{goal:?}");
        }
    }

    #[test]
    fn refusals_wobble_their_tool_and_steps_pop_a_quiet_note() {
        let mut lab = Lab::new();
        let tools = [ToolState::default(); BUTTONS];
        let mut hud = Hud::default();
        hud.advance(&lab, true, 0.016, Motion::Full, &tools, false);
        assert!(
            hud.notes.iter().all(Option::is_none),
            "a fresh yard says nothing"
        );
        lab.advance(
            0.,
            crate::Commands {
                step: true,
                ..Default::default()
            },
        );
        hud.advance(&lab, true, 0.016, Motion::Full, &tools, false);
        assert!(hud.tools[5].denied() > 0.9);
        let note = hud.notes.iter().flatten().next().unwrap();
        assert_eq!((note.tool, note.text.as_str()), (5, "Pause first"));
        for _ in 0..12 {
            hud.advance(&lab, true, 0.1, Motion::Full, &tools, false);
        }
        assert!(hud.notes.iter().all(Option::is_none), "notices play once");
        while lab.sandbox.drop_body(false) {}
        hud.refuse(&lab, 8);
        assert!(hud.tools[8].denied() > 0.9 && hud.full > 0.9);
        lab.advance(
            0.,
            crate::Commands {
                pause: true,
                ..Default::default()
            },
        );
        lab.advance(
            0.,
            crate::Commands {
                step: true,
                ..Default::default()
            },
        );
        hud.advance(&lab, true, 0.016, Motion::Full, &tools, false);
        assert!(hud.notes.iter().flatten().any(|n| n.text.as_str() == "+1"));
        assert!(hud.orbit_hint > 0.9);
        hud.advance(&lab, true, 0.1, Motion::Full, &tools, true);
        for _ in 0..10 {
            hud.advance(&lab, true, 0.1, Motion::Full, &tools, false);
        }
        assert_eq!(hud.orbit_hint, 0., "the orbit demo leaves after an orbit");
    }
}
