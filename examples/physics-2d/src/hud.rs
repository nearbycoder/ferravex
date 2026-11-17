//! The lab's screen furniture, drawn icon-first: the title card, a top bar with
//! goal pips, a progress bar and the key for the tool the goal needs, headline
//! toasts and world pops, the paused chip, the tool palette with key caps, an
//! icon status row, the inspection tag, contextual prompts, the optional engine
//! stats panel (G), the LAB CERTIFIED card and scene transitions. Also owns
//! personal bests.
//!
//! Drawing reads lab state and never changes it. Records are optional: storage
//! failure leaves a fully playable lab, and autoplay never writes a best.
//! Per-frame strings use stack [`Line`]s, so steady-state frames don't
//! allocate. Every movement here respects reduced motion through [`Motion`].
use super::juice::Juice;
use super::look::{
    self, AMBER, CYAN, GOLD, Glyph, INK, Icon, Line, MINT, MUTED, PINK, RED, ToolFace, ToolMotion,
    ToolState, faded, fmt_line, modern,
};
use super::presentation::{BUTTONS, LabLayout};
use super::quips;
use super::toys::{
    DEMOLITION_BONKS, Fun, Goal, LIFTOFF_HEIGHT, MOON_HEIGHT, Scenario, TOWER_HEIGHT,
};
use super::{Inspection, Lab, MAX_BODIES, Notice, Rest, Shape, TICK_HZ};
use ferravex_play::tween::{
    Ease, Entrance, Motion, RollCounter, ScreenTransition, Spring, TransitionKind,
};
use ferravex_render::icons::Icon as Pict;
use ferravex_render::input::PlayerInput;
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Fill};
use ferravex_render::storage::Records;
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_render::ui::kit;

const RECORD_RULES: u64 = 0x5048_3244_0001;
const CARD_LIFE: f32 = 12.;
const INTRO: f32 = 3.4;
const NOTE_LIFE: f32 = 1.1;

/// Palette entries per stable button index: (name, action, icon). Names show
/// only while a tool is hovered or selected.
pub(super) const TOOLS: [(&str, &str, Icon); BUTTONS] = [
    ("Ball", "select_one", Icon::Ball),
    ("Crate", "select_two", Icon::Crate),
    ("Drop", "drop", Icon::Drop),
    ("Pulse", "pulse", Icon::Pulse),
    ("Pause", "pause", Icon::Pause),
    ("Step", "step", Icon::Step),
    ("Reset", "restart", Icon::Reset),
    ("Chaos", "chaos", Icon::Chaos),
    ("Domino", "domino", Icon::Domino),
];

/// Accent per stable button index.
fn accent(index: usize) -> Color {
    match index {
        0 => CYAN,
        1 => AMBER,
        7 => PINK,
        8 => MINT,
        _ => GOLD,
    }
}

/// The palette button that advances `goal` right now, if any.
pub(super) fn goal_tool(fun: &Fun, goal: Goal) -> Option<usize> {
    match goal {
        Goal::Liftoff => Some(3),
        Goal::Skyscraper => Some(1),
        Goal::Demolition => Some(7),
        Goal::DominoRun if fun.scenario != Scenario::Dominoes => Some(8),
        Goal::DominoRun if fun.goals[goal.index()] => Some(6),
        Goal::DominoRun => None,
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
    let width = 300_f32.min(w - 32.);
    let top = if w >= 780. { MENU_CORNER.y + 4. } else { 98. };
    Rect::new(w - 16. - width, top, width, 40. + rows as f32 * 20.)
}

/// Widest centered toast content; toasts reaching above y = 60 stay clear of
/// the menu corner on both sides.
fn toast_max(w: f32, top: f32) -> f32 {
    let max = (w - 48.).min(640.);
    if top < MENU_CORNER.y {
        max.min(w - 2. * MENU_CORNER.x - 40.)
    } else {
        max
    }
}

/// Centered certification card, never above the menu corner band.
fn card_rect(w: f32, h: f32, height: f32) -> Rect {
    let width = (w - 32.).min(520.);
    let height = height.min(h - MENU_CORNER.y - 16.);
    let top = (h * 0.5 - height * 0.5).max(MENU_CORNER.y + 4.);
    Rect::new(w * 0.5 - width * 0.5, top, width, height)
}

/// Room for the rank title starting at `x`, short of the menu corner.
fn rank_width(w: f32, x: f32) -> f32 {
    (w - MENU_CORNER.x - 8. - x).clamp(0., 300.)
}

/// A stored best for the card: "—" when none exists, gold when this session
/// set it.
fn best_cell(
    session: f32,
    stored: Option<f32>,
    lower_is_better: bool,
    decimals: usize,
    unit: &str,
) -> (Line<32>, Color) {
    match stored {
        None => (fmt_line_32("—"), MUTED),
        Some(best) => {
            let set = if lower_is_better {
                session <= best + 0.001
            } else {
                session >= best - 0.001
            };
            let mut line = Line::<32>::new();
            let _ = std::fmt::Write::write_fmt(
                &mut line,
                format_args!("{best:.decimals$}{unit}{}", if set { " ★" } else { "" }),
            );
            (line, if set { GOLD } else { AMBER })
        }
    }
}

fn fmt_line_32(text: &str) -> Line<32> {
    let mut line = Line::<32>::new();
    let _ = std::fmt::Write::write_str(&mut line, text);
    line
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
    pub tower: Option<f32>,
    pub altitude: Option<f32>,
    pub bonks: Option<u32>,
    pub domino: Option<f32>,
}

/// Whether a session value deserves storing over a stored best.
fn improved(stored: Option<f32>, session: f32, floor: f32, lower_is_better: bool) -> bool {
    if !session.is_finite() || (!lower_is_better && session < floor) {
        return false;
    }
    match stored {
        None => true,
        Some(best) if lower_is_better => session < best - 0.005,
        Some(best) => session > best + 0.05,
    }
}

impl Bests {
    /// `enabled` is false for autoplay demos: they never claim a best.
    pub fn load(enabled: bool) -> Self {
        let records = Records::new("physics-2d").ok();
        let read = |id: &str| {
            records
                .as_ref()
                .and_then(|r| r.get(id, RECORD_RULES))
                .map(|r| r.score)
        };
        let domino = records
            .as_ref()
            .and_then(|r| r.get("domino", RECORD_RULES))
            .and_then(|r| r.fastest_us)
            .map(|us| us as f32 / 1_000_000.);
        Self {
            tower: read("tower").map(|cm| cm as f32 / 100.),
            altitude: read("altitude").map(|cm| cm as f32 / 100.),
            bonks: read("bonks").map(|n| n as u32),
            domino,
            records,
            enabled,
        }
    }

    /// Store improved session values. Returns the beaten best only when a
    /// stored best existed, so a first visit never claims a record.
    pub fn update(&mut self, fun: &Fun) -> Option<Best> {
        if !self.enabled {
            return None;
        }
        let mut beaten = None;
        let store = |records: &mut Option<Records>, id: &str, score: u64, time: Option<u64>| {
            if let Some(records) = records.as_mut() {
                let _ = records.attempt(id, RECORD_RULES, score, time);
            }
        };
        if improved(self.tower, fun.tower, 6.5, false) {
            if self.tower.is_some() {
                beaten = Some(Best {
                    glyph: look::goal_glyph(Goal::Skyscraper),
                    value: format!("{:.1} m", fun.tower),
                });
            }
            self.tower = Some(fun.tower);
            store(&mut self.records, "tower", (fun.tower * 100.) as u64, None);
        }
        if improved(self.altitude, fun.altitude, 8., false) {
            if self.altitude.is_some() && fun.altitude > self.altitude.unwrap_or(0.) + 0.5 {
                beaten = Some(Best {
                    glyph: look::goal_glyph(Goal::Liftoff),
                    value: format!("{:.1} m", fun.altitude),
                });
            }
            self.altitude = Some(fun.altitude);
            store(
                &mut self.records,
                "altitude",
                (fun.altitude * 100.) as u64,
                None,
            );
        }
        if improved(
            self.bonks.map(|b| b as f32),
            fun.bonk_peak as f32,
            20.,
            false,
        ) {
            if self.bonks.is_some() {
                beaten = Some(Best {
                    glyph: look::goal_glyph(Goal::Demolition),
                    value: format!("{}/s", fun.bonk_peak),
                });
            }
            self.bonks = Some(fun.bonk_peak);
            store(&mut self.records, "bonks", u64::from(fun.bonk_peak), None);
        }
        if let Some(seconds) = fun.domino_seconds()
            && improved(self.domino, seconds, 0., true)
        {
            if self.domino.is_some() {
                beaten = Some(Best {
                    glyph: look::goal_glyph(Goal::DominoRun),
                    value: format!("{seconds:.2} s"),
                });
            }
            self.domino = Some(seconds);
            store(
                &mut self.records,
                "domino",
                1,
                Some((seconds * 1_000_000.) as u64),
            );
        }
        beaten
    }

    /// The "one more try" hook once every goal is done: a glyph and at most
    /// a word or two with a number.
    pub fn next_hook(&self, fun: &Fun) -> (Glyph, Line<32>) {
        let mut line = Line::<32>::new();
        let glyph = if !fun.secret {
            let _ = std::fmt::Write::write_fmt(&mut line, format_args!("{MOON_HEIGHT:.0} m?"));
            Glyph::Pict(Pict::Rocket)
        } else if let Some(best) = self.tower {
            let _ = std::fmt::Write::write_fmt(&mut line, format_args!("Beat {best:.1} m"));
            look::goal_glyph(Goal::Skyscraper)
        } else if let Some(best) = self.bonks {
            let _ = std::fmt::Write::write_fmt(&mut line, format_args!("Beat {best}/s"));
            look::goal_glyph(Goal::Demolition)
        } else {
            let _ = std::fmt::Write::write_str(&mut line, "Higher!");
            look::goal_glyph(Goal::Skyscraper)
        };
        (glyph, line)
    }
}

/// A short popup over the tool (or cursor) that accepted or refused a command.
struct Note {
    /// Palette button index, or `None` to float at the cursor.
    tool: Option<usize>,
    glyph: Glyph,
    text: Line<16>,
    color: Color,
    age: f32,
}

/// Screen-space HUD state that is not simulation.
pub(super) struct Hud {
    /// Engine diagnostics panel (G). Off by default to keep the scene clean.
    pub stats: bool,
    /// Seconds since LAB CERTIFIED appeared.
    pub card: Option<f32>,
    /// Onboarding emphasis (beckoning tool halos), fading once learned.
    hint: f32,
    /// The "click to drop" cursor prompt, fading after the first drop.
    drop_hint: f32,
    /// The "I inspects" prompt, fading after the first successful inspection.
    inspect_hint: f32,
    inspected: bool,
    active: f32,
    /// Seconds since launch while the title card is up.
    intro: Option<f32>,
    tools: [ToolMotion; BUTTONS],
    notes: [Option<Note>; 4],
    /// The last lab notice already reacted to.
    seen: u32,
    /// Red flash on the body counter after a capacity refusal.
    full: f32,
    transition: ScreenTransition,
    stats_open: Spring,
    rolls: [RollCounter; 4],
    time: f32,
    motion: Motion,
}

impl Default for Hud {
    fn default() -> Self {
        Self {
            stats: false,
            card: None,
            hint: 1.,
            drop_hint: 1.,
            inspect_hint: 1.,
            inspected: false,
            active: 0.,
            intro: Some(0.),
            tools: [ToolMotion::default(); BUTTONS],
            notes: Default::default(),
            seen: 0,
            full: 0.,
            transition: ScreenTransition::new(TransitionKind::Iris, 0.42),
            stats_open: Spring::critical(0., 10.),
            rolls: [RollCounter::new(0); 4],
            time: 0.,
            motion: Motion::Full,
        }
    }
}

pub(super) struct Frame<'a> {
    pub lab: &'a Lab,
    pub layout: &'a LabLayout,
    pub view: &'a LabLayout,
    pub juice: &'a Juice,
    pub bests: &'a Bests,
    pub input: &'a PlayerInput,
    pub inspection: Option<&'a Inspection>,
    /// The pointer, when it is over the scene.
    pub pointer: Option<Vec2>,
    pub active: bool,
    pub tools: [ToolState; BUTTONS],
}

/// Fades `value` toward 1 while `show` holds and toward 0 otherwise.
fn approach(value: &mut f32, show: bool, dt: f32) {
    let target = if show { 1. } else { 0. };
    *value += (target - *value).clamp(-dt * 1.5, dt * 4.);
}

impl Hud {
    /// Onboarding stays until the player has dropped and pulsed something, or
    /// 30 seconds of active play pass; then it fades out for good.
    pub fn learned(fun: &Fun, active_seconds: f32) -> bool {
        (fun.stats.drops > 0 && fun.stats.pulses > 0) || active_seconds > 30.
    }

    /// Advance HUD motion by bounded render time and react to the lab's new
    /// notice. `rebuilt` starts the scene transition; `input` (any accepted
    /// press or click) dismisses the title.
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &mut self,
        lab: &Lab,
        active: bool,
        dt: f32,
        motion: Motion,
        tools: &[ToolState; BUTTONS],
        rebuilt: bool,
        input: bool,
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
        let fun = &lab.fun;
        approach(&mut self.hint, !Self::learned(fun, self.active), dt);
        approach(
            &mut self.drop_hint,
            fun.stats.drops == 0 && self.active <= 30.,
            dt,
        );
        approach(&mut self.inspect_hint, !self.inspected, dt);
        if let Some(age) = &mut self.intro {
            *age += dt;
            if *age > INTRO || (input && *age > 0.25) {
                self.intro = None;
            }
        }
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
        self.stats_open.set_target(if self.stats { 1. } else { 0. });
        self.stats_open.advance(dt, motion);
        if rebuilt {
            self.transition.cover();
            self.transition.advance(10., Motion::Full);
            self.transition.reveal();
            self.notes = Default::default();
        }
        self.transition.advance(dt, motion);
        if let Some(age) = &mut self.card {
            *age += dt;
            if *age > CARD_LIFE {
                self.card = None;
            }
        }
        if !fun.events.goals.is_empty() && fun.goals_done() == 4 {
            self.card = Some(0.);
            for roll in &mut self.rolls {
                roll.snap(0);
            }
            self.rolls[0].set_target((fun.tower * 10.) as i64);
            self.rolls[1].set_target((fun.altitude * 10.) as i64);
            self.rolls[2].set_target(i64::from(fun.bonk_peak));
            self.rolls[3].set_target((fun.domino_seconds().unwrap_or(0.) * 100.) as i64);
        }
        if let Some(age) = self.card
            && age > 0.45
        {
            for roll in &mut self.rolls {
                roll.advance(dt, motion);
            }
        }
    }

    /// Feedback for a command: refusals wobble the tool red and pop a short
    /// note over it; a few accepted commands pop a quiet note.
    fn react(&mut self, lab: &Lab, notice: Notice) {
        let cross = Glyph::Pict(Pict::Cross);
        let pause = Glyph::Tool(Icon::Pause);
        let (tool, glyph, word, color, refused) = match notice {
            Notice::DropFull => (Some(2), cross, "Full", RED, true),
            Notice::ChaosFull => (Some(7), cross, "Full", RED, true),
            Notice::DropRejected => (Some(2), cross, "Nope", RED, true),
            Notice::PulseRejected => (Some(3), cross, "Nope", RED, true),
            Notice::PulseRecharging => (Some(3), Pict::Hourglass.into(), "", AMBER, true),
            Notice::PulsePaused => (Some(3), pause, "Paused", AMBER, true),
            Notice::StepWhileRunning => (Some(5), pause, "Pause first", AMBER, true),
            Notice::Stepped => (Some(5), Pict::Clock.into(), "+1", CYAN, false),
            Notice::Missed => (None, Pict::Search.into(), "Miss", MUTED, false),
            Notice::Selected => {
                self.inspected = true;
                return;
            }
            _ => return,
        };
        // A recharge refusal shows the seconds left instead of a word.
        let text = if notice == Notice::PulseRecharging {
            short(format_args!("{:.1} s", lab.sandbox.pulse_time))
        } else {
            short(format_args!("{word}"))
        };
        if let Some(index) = tool
            && refused
        {
            self.tools[index].deny();
        }
        if matches!(notice, Notice::DropFull | Notice::ChaosFull) {
            self.full = 1.;
        }
        // One note per anchor: a repeat refreshes it instead of stacking.
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
            2 => Notice::DropFull,
            7 => Notice::ChaosFull,
            3 if lab.session.is_paused() => Notice::PulsePaused,
            3 => Notice::PulseRecharging,
            5 => Notice::StepWhileRunning,
            _ => return,
        };
        self.react(lab, notice);
    }

    pub fn tool_kick(&mut self, index: usize) {
        if let Some(tool) = self.tools.get_mut(index) {
            tool.kick();
        }
    }

    /// Any click or key after a short grace closes the certification card.
    pub fn dismiss_card(&mut self) -> bool {
        if self.card.is_some_and(|age| age > 0.6) {
            self.card = None;
            return true;
        }
        false
    }

    /// Overlays that belong to the world image (drawn after the post
    /// boundary, before the HUD): collapse letterbox and scene transitions.
    pub fn draw_world_overlay(&self, f: &Frame, beat: f32) {
        let w = screen_width();
        if beat > 0. {
            let bar = 30. * beat;
            let band = Rect::new(0., 44., w, bar);
            shapes::fill_rounded_rect(
                band,
                0.,
                Fill::vertical(
                    band,
                    Color::new(0., 0., 0., 0.92),
                    Color::new(0., 0., 0., 0.7),
                ),
            );
            let low = Rect::new(0., f.layout.footer_y - bar, w, bar);
            shapes::fill_rounded_rect(
                low,
                0.,
                Fill::vertical(
                    low,
                    Color::new(0., 0., 0., 0.7),
                    Color::new(0., 0., 0., 0.92),
                ),
            );
        }
        let coverage = self.transition.coverage();
        if coverage > 0. {
            let focus = vec2(w * 0.5, (44. + f.layout.footer_y) * 0.5);
            kit::draw_screen_transition(
                self.transition.kind(self.motion),
                coverage,
                look::BG_TOP,
                focus,
            );
        }
    }

    pub fn draw(&self, f: &Frame) {
        self.world_marks(f);
        self.prompts(f);
        self.top_bar(f);
        self.tap_demo(f);
        self.toast(f);
        self.footer(f);
        if self.stats_open.value > 0.01 {
            self.diagnostics(f);
        }
        if let Some(age) = self.intro {
            self.title_card(f, age);
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
        let bar = Rect::new(0., 0., w, 44.);
        shapes::fill_rounded_rect(
            bar,
            0.,
            Fill::vertical(
                bar,
                Color::new(0.05, 0.08, 0.13, 0.96),
                Color::new(0.03, 0.05, 0.09, 0.9),
            ),
        );
        draw_rectangle(0., 44., w, 1., Color::new(1., 1., 1., 0.07));
        // Logo: retro face with an emissive status pip.
        let logo = look::classic(16., INK);
        let logo_w = draw_text_styled("2D PHYSICS LAB", 16., 21., logo).width;
        let pip = vec2(16. + logo_w + 10., 14.);
        let pulse = 0.65
            + 0.35
                * if self.motion.is_reduced() {
                    0.
                } else {
                    (self.time * 3.).sin()
                };
        let pip_color = if f.lab.session.is_paused() {
            AMBER
        } else {
            MINT
        };
        shapes::glow_disc(pip, 7., faded(pip_color, 0.55 * pulse));
        shapes::fill_circle(pip, 2.5, pip_color);
        // Scene and rank: names beside their pictograms.
        let sub = modern(8., AMBER);
        look::glyph(look::scene_glyph(fun.scenario), vec2(21., 33.), 10., AMBER);
        let used = draw_text_styled(fun.scenario.name(), 30., 37., sub).width;
        let crown = 30. + used + 14.;
        look::glyph(Pict::Crown, vec2(crown, 33.), 10., MUTED);
        let rank = quips::rank(fun.goals_done(), fun.secret);
        look::text(
            rank.title,
            crown + 9.,
            37.,
            rank_width(w, crown + 9.),
            sub.with_color(MUTED),
        );

        // The goal block sits right of the title on wide screens and below
        // it on narrow screens, where the scene leaves vertical room.
        let (x, y, row, width) = goal_block(w, wide, 48.);
        if !wide && f.layout.scene_top() < 96. {
            return;
        }
        if !wide {
            let strip = Rect::new(0., 45., w, 44.);
            shapes::fill_rounded_rect(strip, 0., Color::new(0.03, 0.05, 0.09, 0.82));
        }
        let cy = y + 15.;
        let low = y + 34.;
        let pips = look::pips(
            x,
            cy,
            &fun.goals,
            fun.current_goal().map(Goal::index),
            GOLD,
            1.,
        );
        let head_x = x + pips + 14.;
        // In the domino scene its own run is the headline even when another
        // goal comes first in teaching order.
        let shown = if fun.scenario == Scenario::Dominoes {
            Some(Goal::DominoRun)
        } else {
            fun.current_goal()
        };
        let Some(goal) = shown else {
            // Every goal done: the rank, then a one-more-try hook.
            look::glyph(Pict::Trophy, vec2(head_x + 8., cy), 16., MINT);
            look::text(
                rank.title,
                head_x + 22.,
                cy + 8.,
                (x + row - head_x - 22.).max(1.),
                modern(16., MINT),
            );
            let (glyph, text) = f.bests.next_hook(fun);
            look::icon_value(glyph, text.as_str(), x, low, 14., modern(16., PINK));
            return;
        };
        let (value, target) = fun.progress(goal);
        let done = fun.goals[goal.index()];
        let color = if done { MINT } else { GOLD };
        // Row 1: pips, goal pictogram and name, then the tool that helps.
        let chip = goal_tool(fun, goal).map(|index| {
            let (_, action, icon) = TOOLS[index];
            (index, icon, hint(f.input, action))
        });
        let chip_w = chip
            .as_ref()
            .map_or(0., |(_, _, key)| 26. + look::keycap_width(key.as_str()));
        look::glyph(look::goal_glyph(goal), vec2(head_x + 8., cy), 16., color);
        let title_w = look::text(
            goal.title(),
            head_x + 22.,
            cy + 8.,
            (x + row - chip_w - head_x - 34.).max(1.),
            modern(16., color),
        );
        if let Some((index, icon, key)) = &chip {
            let at = head_x + 22. + title_w + 14.;
            let glow = if self.hint > 0.01 {
                0.2 + 0.25 * self.breathe(0.9)
            } else {
                0.12
            };
            shapes::glow_disc(vec2(at + 9., cy), 14., faded(accent(*index), glow));
            look::glyph(*icon, vec2(at + 9., cy), 15., accent(*index));
            look::keycap(at + 22., cy, key.as_str(), 1.);
        }
        // Row 2: a bar toward the target flag, then the live value.
        let readout = match goal {
            Goal::Liftoff | Goal::Skyscraper => fmt_line!("{value:.1} m"),
            Goal::Demolition => fmt_line!("{value:.0}/s"),
            Goal::DominoRun if done => fmt_line!("{:.2} s", fun.domino_seconds().unwrap_or(0.)),
            Goal::DominoRun => fmt_line!("{value:.0}"),
        };
        let target_text = match goal {
            // Whole targets drop their decimal; 7.5 m keeps it.
            Goal::Liftoff | Goal::Skyscraper if target.fract().abs() < 0.05 => {
                fmt_line!("{target:.0} m")
            }
            Goal::Liftoff | Goal::Skyscraper => fmt_line!("{target:.1} m"),
            Goal::Demolition => fmt_line!("{target:.0}/s"),
            Goal::DominoRun => fmt_line!("{target:.0}"),
        };
        let value_style = modern(16., INK);
        let flag_style = modern(16., faded(color, 0.85));
        let readout_glyph: Glyph = if goal == Goal::DominoRun && done {
            Pict::Clock.into()
        } else {
            Pict::Flag.into()
        };
        let tail = if goal == Goal::DominoRun && done {
            look::icon_value_width(readout.as_str(), 14., value_style)
        } else {
            text_width(readout.as_str(), value_style)
                + 10.
                + look::icon_value_width(target_text.as_str(), 14., flag_style)
        };
        let track = Rect::new(x, low - 2., (width - tail - 12.).max(8.), 4.);
        shapes::fill_rounded_rect(track, 2., Color::new(1., 1., 1., 0.08));
        if goal == Goal::DominoRun && !done {
            // A countable goal reads as one pip per domino.
            let count = target.max(1.) as usize;
            let gap = 2.;
            let seg = ((track.w - gap * (count - 1) as f32) / count as f32).max(1.);
            for i in 0..count {
                let cell = Rect::new(track.x + i as f32 * (seg + gap), track.y, seg, track.h);
                let lit = (i as f32) < value;
                shapes::fill_rounded_rect(
                    cell,
                    1.5,
                    if lit {
                        color
                    } else {
                        Color::new(1., 1., 1., 0.12)
                    },
                );
            }
        } else {
            look::progress(track, if done { 1. } else { value / target }, color, 1.);
            let head = track.x + track.w * (value / target).clamp(0., 1.);
            if value > 0. && !done {
                shapes::glow_disc(vec2(head, low), 6., faded(color, 0.7));
            }
        }
        let mut tx = track.x + track.w + 12.;
        if goal == Goal::DominoRun && done {
            look::icon_value(readout_glyph, readout.as_str(), tx, low, 14., value_style);
        } else {
            tx += draw_text_styled(
                readout.as_str(),
                tx,
                value_style.centered_baseline(low),
                value_style,
            )
            .width
                + 10.;
            look::icon_value(
                readout_glyph,
                target_text.as_str(),
                tx,
                low,
                14.,
                flag_style,
            );
        }
    }

    /// Bottom of the top bar (and the narrow goal block).
    fn band_bottom(f: &Frame) -> f32 {
        if screen_width() >= 780. || f.layout.scene_top() < 96. {
            44.
        } else {
            92.
        }
    }

    /// On tall narrow screens the sky above the lab shows a tapping hand
    /// until the first drop: a demonstration instead of instructions.
    fn tap_demo(&self, f: &Frame) {
        let band = Self::band_bottom(f);
        let void = f.layout.scene_top() - band;
        let alpha = self.drop_hint;
        // Toasts and the paused chip use the same sky; they take precedence.
        let paused_chip = f.lab.session.is_paused() && f.active;
        if void < 170.
            || alpha <= 0.01
            || f.juice.toast.is_some()
            || self.intro.is_some()
            || paused_chip
        {
            return;
        }
        let w = screen_width();
        let center = vec2(w * 0.5, band + void * 0.5 - 10.);
        let tap = if self.motion.is_reduced() {
            0.
        } else {
            (self.time * 1.4).fract()
        };
        let press = if tap < 0.25 { tap / 0.25 } else { 0. };
        // Ripple where the finger lands, then arrows down into the lab.
        let ring = 10. + 26. * press;
        shapes::stroke_circle(center, ring, 2., faded(CYAN, alpha * (1. - press) * 0.9));
        shapes::glow_disc(center, 30., faded(CYAN, 0.12 * alpha));
        look::glyph(
            Pict::Hand,
            center + vec2(10., 14. + 4. * (1. - press)),
            40.,
            faded(INK, alpha),
        );
        look::glyph(Icon::Ball, center + vec2(-34., 4.), 16., faded(CYAN, alpha));
        for k in 0..3 {
            let y = center.y + 46. + k as f32 * 12.;
            let a = alpha * (0.35 + 0.65 * self.breathe(1.2 + k as f32 * 0.1));
            shapes::polyline(
                &[
                    vec2(w * 0.5 - 7., y),
                    vec2(w * 0.5, y + 6.),
                    vec2(w * 0.5 + 7., y),
                ],
                shapes::Stroke::new(2., faded(CYAN, a)).cap(shapes::Cap::Round),
            );
        }
    }

    fn world_marks(&self, f: &Frame) {
        // Target line for height goals, flagged at the right wall.
        let fun = &f.lab.fun;
        let line = match fun.current_goal() {
            Some(Goal::Liftoff) => Some(LIFTOFF_HEIGHT),
            Some(Goal::Skyscraper) => Some(TOWER_HEIGHT),
            _ => None,
        };
        if let Some(height) = line
            && fun.scenario == Scenario::Workshop
        {
            let a = f.view.to_screen(vec2(-12., height));
            let b = f.view.to_screen(vec2(12., height));
            let march = if self.motion.is_reduced() {
                0.
            } else {
                (self.time * 24.) % 18.
            };
            let mut x = a.x - 18. + march;
            while x < b.x {
                let x0 = x.max(a.x);
                let x1 = (x + 10.).min(b.x);
                if x1 > x0 {
                    draw_rectangle(x0, a.y - 1., x1 - x0, 2., faded(GOLD, 0.6));
                }
                x += 18.;
            }
            let label = fmt_line!("{height:.1} m");
            let style = modern(16., faded(GOLD, 0.95));
            let label_w = look::icon_value_width(label.as_str(), 16., style);
            look::icon_value(
                Pict::Flag,
                label.as_str(),
                b.x - 4. - label_w,
                a.y - 13.,
                16.,
                style,
            );
        }
        self.inspection_tag(f);
        let w = screen_width();
        for pop in f.juice.pops.iter().flatten() {
            let t = pop.age / pop.life;
            let alpha = if t > 0.7 { (1. - t) / 0.3 } else { 1. };
            let appear = Entrance::pop(0.6).sample(pop.age, 0, self.motion);
            let at = f.view.to_screen(pop.at) - vec2(0., f.juice.pop_rise(pop));
            let size = if w < 600. {
                pop.size.min(16.)
            } else {
                pop.size
            };
            let style = TextStyle::modern(size * appear.scale, faded(pop.color, alpha))
                .with_outline(faded(Color::new(0.02, 0.03, 0.06, 0.9), alpha))
                .with_align(TextAlign::Center);
            let width = text_width(&pop.text, style).min(w - 24.);
            let style = look::fit(&pop.text, style, w - 24.);
            // Pops never rise into the top bar or the menu corner.
            let x = at.x.clamp(12. + width * 0.5, w - 12. - width * 0.5);
            let y = at.y.max(MENU_CORNER.y + size + 9.);
            draw_text_styled(&pop.text, x, y, style);
        }
    }

    /// The inspected body's diagnostics as a pill of icon readouts riding
    /// above it: shape and number, sleep state, speed and contacts.
    fn inspection_tag(&self, f: &Frame) {
        let Some(selection) = f.lab.selection() else {
            return;
        };
        let radius = match selection.shape {
            Shape::Ball(radius) => radius,
            Shape::Box(half) | Shape::Plank(half) => half.length(),
            Shape::Dumbbell => 1.03,
        };
        let body = f.view.to_screen(selection.position);
        let shape: Glyph = match selection.shape {
            Shape::Ball(_) => Icon::Ball.into(),
            Shape::Box(_) => Icon::Crate.into(),
            Shape::Plank(_) => Icon::Domino.into(),
            Shape::Dumbbell => Icon::Dumbbell.into(),
        };
        let (rest, rest_color): (Glyph, Color) = match selection.rest {
            Rest::Asleep => (Pict::Moon.into(), Color::new(0.62, 0.72, 1., 1.)),
            Rest::Waking => (Pict::Hourglass.into(), AMBER),
            Rest::Awake => (Pict::Bolt.into(), MINT),
        };
        let number = fmt_line!("#{}", selection.number);
        let speed = fmt_line!("{:.1}", selection.speed);
        let contacts = fmt_line!("{}", selection.contacts);
        let style = modern(8., INK);
        let gap = 10.;
        let parts = look::icon_value_width(number.as_str(), 12., style)
            + 12.
            + look::icon_value_width(speed.as_str(), 12., style)
            + look::icon_value_width(contacts.as_str(), 12., style)
            + gap * 3.;
        let w = screen_width();
        let width = parts + 20.;
        let top = (body.y - radius * f.view.scale - 34.).clamp(
            MENU_CORNER.y + 4.,
            (f.layout.footer_y - 30.).max(MENU_CORNER.y + 4.),
        );
        let x = (body.x - width * 0.5).clamp(8., (w - width - 8.).max(8.));
        let rect = Rect::new(x, top, width, 22.);
        look::panel(rect, 11., 0.95);
        shapes::stroke_rounded_rect(rect, 11., 1., faded(CYAN, 0.6));
        let cy = rect.center().y;
        let mut cx = rect.x + 10.;
        cx += look::icon_value(shape, number.as_str(), cx, cy, 12., style) + gap;
        look::glyph(rest, vec2(cx + 6., cy), 12., rest_color);
        cx += 12. + gap;
        cx += look::icon_value(Pict::Wind, speed.as_str(), cx, cy, 12., style) + gap;
        look::icon_value(Icon::Contact, contacts.as_str(), cx, cy, 12., style);
    }

    /// Key caps and icons beside the cursor, only when they are relevant:
    /// "click drops" until the first drop and "I inspects" over a body while
    /// paused until the first inspection. Tool popups float here on a miss.
    fn prompts(&self, f: &Frame) {
        let Some(pointer) = f.pointer else {
            return;
        };
        if self.intro.is_some() || self.card.is_some() {
            return;
        }
        if self.drop_hint > 0.01 && f.lab.sandbox.bodies.len() < MAX_BODIES {
            let alpha = self.drop_hint * (0.6 + 0.4 * self.breathe(0.8));
            let at = pointer + vec2(18., 20.);
            shapes::glow_disc(at, 14., faded(CYAN, 0.18 * alpha));
            look::glyph(Pict::Pointer, at, 18., faded(INK, alpha));
            if f.input.cursor_is_virtual() {
                look::keycap(at.x + 12., at.y, hint(f.input, "primary").as_str(), alpha);
            }
        }
        let over_body = f
            .inspection
            .and_then(|query| query.body(&f.lab.sandbox))
            .is_some_and(|body| f.lab.selected != Some(body));
        if f.lab.session.is_paused() && over_body && self.inspect_hint > 0.01 {
            let key = hint(f.input, "inspect");
            let alpha = self.inspect_hint;
            let at = pointer + vec2(14., -18.);
            let key_w = look::keycap(at.x, at.y, key.as_str(), alpha);
            look::glyph(
                Pict::Search,
                vec2(at.x + key_w + 11., at.y),
                15.,
                faded(CYAN, alpha),
            );
        }
        for note in self.notes.iter().flatten().filter(|n| n.tool.is_none()) {
            self.note(note, pointer + vec2(0., -16.));
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
        let style = modern(16., faded(note.color, alpha))
            .with_outline(faded(Color::new(0.02, 0.03, 0.06, 0.9), alpha));
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

    fn toast(&self, f: &Frame) {
        let w = screen_width();
        let band = Self::band_bottom(f);
        let void = f.layout.scene_top() - band;
        // Narrow portrait screens leave sky above the lab: use it.
        let mut top = if void >= 110. {
            band + void * 0.5 - 30.
        } else {
            (f.layout.scene_top() + 10.).max(band + 10.)
        };
        if self.stats && w < 780. {
            // The stats panel spans narrow screens; callouts go below it.
            top = top.max(98. + 40. + 6. * 20. + 8.);
        }
        // With engine stats open, center in the space left of that panel.
        let room = if self.stats && w >= 780. { w - 326. } else { w };
        let max = toast_max(room, top);
        let Some(toast) = &f.juice.toast else {
            if f.lab.session.is_paused() && f.active && self.intro.is_none() {
                self.paused_chip(f, room * 0.5, top);
            }
            return;
        };
        let alpha = toast.alpha();
        let color = toast.color;
        let head_style =
            TextStyle::classic(24., color).with_outline(Color::new(0.02, 0.03, 0.06, 0.85));
        let glyph_w = if toast.glyph.is_some() { 30. } else { 0. };
        let value_style = modern(16., INK);
        let value_w = toast.value.as_ref().map_or(0., |(_, text)| {
            look::icon_value_width(text, 16., value_style) + 18.
        });
        let head_room = (max - glyph_w).max(40.);
        let head_style = look::fit(&toast.head, head_style, head_room);
        let head_w = text_width(&toast.head, head_style);
        // One row when it fits; otherwise the icon stat drops below.
        let one_row = glyph_w + head_w + value_w <= max;
        let content = if one_row {
            glyph_w + head_w + value_w
        } else {
            (glyph_w + head_w).max(value_w - 18.)
        };
        let width = content.min(max) + 40.;
        let height = if one_row || toast.value.is_none() {
            48.
        } else {
            72.
        };
        let look_in = Entrance::pop(0.86).sample(toast.age, 0, self.motion);
        let alpha = alpha * look_in.alpha;
        let rect = kit::appear(
            Rect::new(room * 0.5 - width * 0.5, top, width, height),
            look_in,
        );
        look::panel(rect, 14., alpha);
        // Accent: a glowing underline and a soft halo behind the headline.
        let accent_bar = Rect::new(rect.x + 18., rect.y + rect.h - 4., rect.w - 36., 2.);
        shapes::fill_rounded_rect(accent_bar, 1., faded(color, 0.8 * alpha));
        shapes::glow_disc(
            vec2(rect.center().x, rect.y + 24.),
            rect.w * 0.3,
            faded(color, 0.12 * alpha),
        );
        let row_w = if one_row {
            glyph_w + head_w + value_w
        } else {
            glyph_w + head_w
        };
        let mut x = rect.center().x - row_w * 0.5;
        let cy = rect.y + 24.;
        if let Some(glyph) = toast.glyph {
            look::glyph(glyph, vec2(x + 12., cy), 24., faded(color, alpha));
            x += glyph_w;
        }
        let head = head_style
            .with_color(faded(color, alpha))
            .with_outline(faded(Color::new(0.02, 0.03, 0.06, 0.85), alpha));
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
                    cy + 26.,
                    16.,
                    style,
                );
            }
        }
    }

    /// While paused: a calm chip with the keys that matter now.
    fn paused_chip(&self, f: &Frame, cx: f32, top: f32) {
        let step = hint(f.input, "step");
        let inspect = hint(f.input, "inspect");
        let head = modern(16., AMBER);
        let head_w = text_width("Paused", head);
        let keys = [
            (step.as_str(), Glyph::Tool(Icon::Step)),
            (inspect.as_str(), Glyph::Pict(Pict::Search)),
        ];
        let keys_w: f32 = keys
            .iter()
            .filter(|(key, _)| look::shows_key(key))
            .map(|(key, _)| look::keycap_width(key) + 22. + 12.)
            .sum();
        let width = 22. + head_w + 16. + keys_w + 24.;
        let rect = Rect::new(cx - width * 0.5, top, width, 32.);
        look::panel(rect, 16., 1.);
        shapes::stroke_rounded_rect(rect, 16., 1., faded(AMBER, 0.45));
        let cy = rect.center().y;
        let mut x = rect.x + 14.;
        look::glyph(Icon::Pause, vec2(x + 7., cy), 14., AMBER);
        x += 20.;
        x += draw_text_styled("Paused", x, head.centered_baseline(cy), head).width + 16.;
        for (key, glyph) in keys {
            if !look::shows_key(key) {
                continue;
            }
            x += look::keycap(x, cy, key, 1.) + 5.;
            look::glyph(glyph, vec2(x + 8., cy), 15., INK);
            x += 29.;
        }
    }

    fn footer(&self, f: &Frame) {
        let w = screen_width();
        let lab = f.lab;
        let layout = f.layout;
        let panel = Rect::new(
            14.,
            layout.footer_y + 2.,
            w - 28.,
            layout.footer_height - 8.,
        );
        look::panel(panel, 16., 1.);
        // Group dividers on the single-row palette.
        if layout.single_row() {
            for (left, right) in [(2, 3), (8, 4)] {
                let a = layout.button(left);
                let b = layout.button(right);
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
        let beckon_tool = lab
            .fun
            .current_goal()
            .and_then(|goal| goal_tool(&lab.fun, goal))
            .filter(|&index| match index {
                3 => lab.fun.stats.pulses == 0,
                1 => lab.fun.stats.drops == 0,
                7 => lab.fun.stats.chaos == 0,
                8 => lab.fun.scenario == Scenario::Workshop,
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
                beckon: if beckon_tool == Some(index) {
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
            let rect = layout.button(index);
            if look::tool_button(rect, &face, f.tools[index], motion) {
                bubble = Some((label, rect));
            }
        }
        // While learning, a bobbing chevron points at the tool the goal needs.
        if let Some(index) = beckon_tool
            && f.tools[index].enabled
            && self.hint > 0.01
            && bubble.is_none()
            && layout.single_row()
        {
            let rect = layout.button(index);
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
            look::bubble(label, vec2(rect.center().x, rect.y - 4.), INK, 1.);
        }
        for note in self.notes.iter().flatten() {
            if let Some(index) = note.tool {
                let rect = layout.button(index);
                self.note(note, vec2(rect.center().x, rect.y - 8.));
            }
        }
        self.status_row(f);
    }

    /// Status as icon readouts: run state, the body count against its cap,
    /// and the stats key.
    fn status_row(&self, f: &Frame) {
        let lab = f.lab;
        let w = screen_width();
        let cy = f.layout.status_y() - 2.;
        let (state, color): (Glyph, Color) = if !f.active {
            (Pict::EyeOff.into(), MUTED)
        } else if lab.session.is_paused() {
            (Icon::Pause.into(), AMBER)
        } else {
            (Icon::Play.into(), MINT)
        };
        let dot = vec2(36., cy);
        shapes::glow_disc(dot, 11., faded(color, 0.35));
        look::glyph(state, dot, 12., color);
        let count = lab.sandbox.bodies.len();
        let full = count >= MAX_BODIES;
        let warn = if full { 1. } else { self.full };
        let ink = shapes::mix(INK, RED, warn);
        let bodies = fmt_line!("{count}");
        let x = 56.;
        let used = look::icon_value(Icon::Shapes, bodies.as_str(), x, cy, 14., modern(16., ink));
        let meter = Rect::new(x + used + 8., cy - 2., 64., 4.);
        look::progress(
            meter,
            count as f32 / MAX_BODIES as f32,
            shapes::mix(CYAN, RED, warn),
            1.,
        );
        // The stats key: lit while the panel is open.
        let key = hint(f.input, "stats");
        let key_w = look::keycap_width(key.as_str());
        if key_w > 0. {
            let lit = self.stats_open.value.clamp(0., 1.);
            let right = w - 30.;
            look::glyph(
                Pict::Gear,
                vec2(right - 8., cy),
                14.,
                shapes::mix(faded(MUTED, 0.8), CYAN, lit),
            );
            look::keycap(right - 20. - key_w, cy, key.as_str(), 0.6 + 0.4 * lit);
        }
    }

    fn diagnostics(&self, f: &Frame) {
        let w = screen_width();
        let lab = f.lab;
        let physics = &lab.sandbox.physics;
        let sleep = physics.sleep_stats();
        let (started, stopped) = physics
            .contact_events()
            .iter()
            .fold((0, 0), |(a, b), event| match event {
                ferravex_physics::ContactEvent::Started(..) => (a + 1, b),
                ferravex_physics::ContactEvent::Stopped(..) => (a, b + 1),
            });
        let query = match f.inspection {
            Some(q) if q.report.is_truncated() => {
                fmt_line!("{}/{}", q.report.written, q.report.total)
            }
            Some(q) => fmt_line!("{}", q.report.total),
            None => fmt_line!("—"),
        };
        let pulse = if lab.sandbox.pulse_time > 0. {
            fmt_line!("{:.2} s", lab.sandbox.pulse_time)
        } else {
            fmt_line!("ready")
        };
        // Each row is icon readouts: ticks, sleep, contacts, the cursor query,
        // pulse recharge and bonks against the Demolition target.
        let clock = [
            (
                Glyph::Pict(Pict::Clock),
                fmt_line!("{}", lab.session.tick()),
            ),
            (
                Glyph::Pict(Pict::Bolt),
                fmt_line!("{:.0} Hz · {:.2} ms", TICK_HZ, 1000. / TICK_HZ),
            ),
        ];
        let rest = [
            (Glyph::Pict(Pict::Sun), fmt_line!("{}", sleep.awake_bodies)),
            (
                Glyph::Pict(Pict::Moon),
                fmt_line!("{}", sleep.sleeping_bodies),
            ),
            (
                Glyph::Tool(Icon::Shapes),
                fmt_line!("{}/{}", lab.sandbox.bodies.len(), MAX_BODIES),
            ),
        ];
        let contacts = [
            (
                Glyph::Tool(Icon::Contact),
                fmt_line!("{}", physics.contacts().count()),
            ),
            (Glyph::Pict(Pict::Plus), fmt_line!("{started}")),
            (Glyph::Pict(Pict::Minus), fmt_line!("{stopped}")),
        ];
        let probe = [(Glyph::Pict(Pict::Search), query)];
        let charge = [(Glyph::Tool(Icon::Pulse), pulse)];
        let bonks = [
            (
                look::goal_glyph(Goal::Demolition),
                fmt_line!("{}/s", lab.fun.bonks_now),
            ),
            (
                Glyph::Pict(Pict::Trophy),
                fmt_line!("{}", lab.fun.bonk_peak),
            ),
            (Glyph::Pict(Pict::Flag), fmt_line!("{DEMOLITION_BONKS}")),
        ];
        let rows: [&[(Glyph, Line)]; 6] = [&clock, &rest, &contacts, &probe, &charge, &bonks];
        let open = self.stats_open.value.clamp(0., 1.);
        let rect = stats_rect(w, rows.len());
        let look_in = Ease::CubicOut.apply(open);
        let rect = Rect::new(rect.x + (1. - look_in) * 24., rect.y, rect.w, rect.h);
        look::panel(rect, 14., open);
        let x = rect.x + 14.;
        look::icon_value(
            Pict::Gear,
            "Engine",
            x,
            rect.y + 20.,
            14.,
            modern(16., faded(AMBER, open)),
        );
        let column = (rect.w - 28.) / 3.;
        for (row, cells) in rows.iter().enumerate() {
            let cy = rect.y + 46. + row as f32 * 20.;
            let mut cx = x;
            for (glyph, text) in cells.iter() {
                let style = modern(16., faded(INK, open));
                let used = look::icon_value(*glyph, text.as_str(), cx, cy, 13., style);
                cx += used.max(column - 6.) + 6.;
            }
        }
    }

    fn title_card(&self, f: &Frame, age: f32) {
        let w = screen_width();
        let fade_out = ((INTRO - age) / 0.5).clamp(0., 1.);
        let fade_in = (age / 0.25).clamp(0., 1.);
        let alpha = fade_in.min(fade_out);
        let center = vec2(w * 0.5, (44. + f.layout.footer_y) * 0.5);
        let back = Rect::new(0., 45., w, f.layout.footer_y - 45.);
        shapes::fill_rounded_rect(
            back,
            0.,
            Fill::radial(
                center,
                w.max(back.h) * 0.6,
                Color::new(0.02, 0.03, 0.07, 0.72 * alpha),
                Color::new(0.02, 0.03, 0.07, 0.35 * alpha),
            ),
        );
        let pop = Entrance::pop(0.7)
            .with_duration(0.5)
            .sample(age, 0, self.motion);
        let size = if w < 600. { 32. } else { 48. };
        shapes::glow_disc(
            center + vec2(0., -size * 0.4),
            size * 4.,
            faded(CYAN, 0.16 * alpha),
        );
        let logo = TextStyle::classic(size * pop.scale, faded(INK, alpha * pop.alpha))
            .with_outline(faded(Color::new(0.02, 0.03, 0.06, 0.9), alpha))
            .with_align(TextAlign::Center);
        let logo = look::fit("PHYSICS LAB", logo, w - 40.);
        draw_text_styled("PHYSICS LAB", center.x, center.y, logo);
        // A "2D" badge riding the logo's corner.
        let logo_w = text_width("PHYSICS LAB", logo);
        let badge = vec2(
            (center.x + logo_w * 0.5 + 20.).min(w - 20.),
            center.y - size * 1.05,
        );
        shapes::fill_circle(badge, 15., faded(GOLD, alpha * pop.alpha));
        let badge_style = TextStyle::classic(16., faded(Color::new(0.1, 0.07, 0.02, 1.), alpha))
            .with_align(TextAlign::Center);
        draw_text_styled(
            "2D",
            badge.x,
            badge_style.centered_baseline(badge.y),
            badge_style,
        );
        // The toy box parades in: what you can play with, not a tagline.
        let parade = [
            (Icon::Ball, CYAN),
            (Icon::Crate, AMBER),
            (Icon::Pulse, GOLD),
            (Icon::Chaos, PINK),
            (Icon::Domino, MINT),
        ];
        let stagger = Entrance::fade_up(14.).with_stagger(0.08);
        let spacing = 44_f32.min((w - 60.) / parade.len() as f32);
        let x0 = center.x - spacing * (parade.len() - 1) as f32 * 0.5;
        for (i, (icon, color)) in parade.into_iter().enumerate() {
            let look_in = stagger.sample(age - 0.15, i, self.motion);
            let bob = if self.motion.is_reduced() {
                0.
            } else {
                (age * 3. + i as f32 * 0.9).sin() * 3.
            };
            let at = vec2(
                x0 + i as f32 * spacing,
                center.y + 34. + look_in.offset[1] + bob,
            );
            shapes::glow_disc(at, 16., faded(color, 0.15 * alpha * look_in.alpha));
            look::glyph(icon, at, 22., faded(color, alpha * look_in.alpha));
        }
        let blink = if self.motion.is_reduced() {
            1.
        } else {
            0.55 + 0.45 * (age * 4.).sin().abs()
        };
        let cta = Entrance::fade_up(10.).sample(age - 0.45, 0, self.motion);
        let style = modern(16., faded(INK, alpha * cta.alpha * blink));
        let text = "Make a mess";
        let width = look::icon_value_width(text, 18., style);
        look::icon_value(
            Pict::Pointer,
            text,
            center.x - width * 0.5,
            center.y + 74. + cta.offset[1],
            18.,
            style,
        );
    }

    fn certificate(&self, f: &Frame, age: f32) {
        let w = screen_width();
        let h = screen_height();
        let fun = &f.lab.fun;
        let alpha = (age / 0.3).min(1.) * ((CARD_LIFE - age) / 0.5).clamp(0., 1.);
        draw_rectangle(0., 0., w, h, faded(Color::new(0., 0., 0., 0.5), alpha));
        let rect = card_rect(w, h, 320.);
        let look_card = Entrance::pop(0.9).sample(age, 0, self.motion);
        let rect = kit::appear(rect, look_card);
        let alpha = alpha * look_card.alpha;
        look::panel(rect, 18., alpha);
        let header = Rect::new(rect.x, rect.y, rect.w, 4.);
        shapes::fill_rounded_rect(header, 2., faded(GOLD, alpha));
        let cx = rect.center().x;
        let inner = rect.w - 48.;
        let rank = quips::rank(fun.goals_done(), fun.secret);
        let stagger = Entrance::fade_up(12.).with_stagger(0.07);
        let row_look = |i: usize| stagger.sample(age - 0.15, i, self.motion);
        let l0 = row_look(0);
        let banner = modern(16., faded(AMBER, alpha * l0.alpha));
        let banner_w = text_width("Lab certified", banner);
        let by = rect.y + 26. + l0.offset[1];
        draw_text_styled(
            "Lab certified",
            cx,
            banner.centered_baseline(by),
            banner.with_align(TextAlign::Center),
        );
        for side in [-1., 1.] {
            look::glyph(
                Pict::Star,
                vec2(cx + side * (banner_w * 0.5 + 14.), by),
                14.,
                faded(GOLD, alpha * l0.alpha),
            );
        }
        let l1 = row_look(1);
        shapes::glow_disc(
            vec2(cx, rect.y + 60.),
            rect.w * 0.35,
            faded(GOLD, 0.12 * alpha * l1.alpha),
        );
        let title = TextStyle::classic(32., faded(GOLD, alpha * l1.alpha))
            .with_outline(faded(Color::new(0.1, 0.06, 0.0, 0.9), alpha * l1.alpha))
            .with_align(TextAlign::Center);
        draw_text_styled(
            rank.title,
            cx,
            rect.y + 70. + l1.offset[1],
            look::fit(rank.title, title, inner),
        );
        let l2 = row_look(2);
        look::text_center(
            rank.quip,
            cx,
            rect.y + 96. + l2.offset[1],
            inner,
            modern(16., faded(INK, alpha * l2.alpha)),
        );
        // Session vs best, one pictogram per row, a trophy over the bests.
        let x0 = rect.x + 32.;
        let x1 = rect.x + rect.w * 0.6;
        let x2 = rect.x + rect.w - 24.;
        let l3 = row_look(3);
        look::glyph(
            Pict::Trophy,
            vec2(x2 - 10., rect.y + 126.),
            14.,
            faded(AMBER, alpha * l3.alpha),
        );
        let shown = |i: usize| self.rolls[i].shown();
        let rows = [
            (
                Goal::Skyscraper,
                fmt_line!("{:.1} m", shown(0) as f32 / 10.),
                best_cell(fun.tower, f.bests.tower, false, 1, " m"),
            ),
            (
                Goal::Liftoff,
                fmt_line!("{:.1} m", shown(1) as f32 / 10.),
                best_cell(fun.altitude, f.bests.altitude, false, 1, " m"),
            ),
            (
                Goal::Demolition,
                fmt_line!("{}/s", shown(2)),
                best_cell(
                    fun.bonk_peak as f32,
                    f.bests.bonks.map(|b| b as f32),
                    false,
                    0,
                    "/s",
                ),
            ),
            (
                Goal::DominoRun,
                if fun.domino_seconds().is_some() {
                    fmt_line!("{:.2} s", shown(3) as f32 / 100.)
                } else {
                    fmt_line!("—")
                },
                best_cell(
                    fun.domino_seconds().unwrap_or(f32::INFINITY),
                    f.bests.domino,
                    true,
                    2,
                    " s",
                ),
            ),
        ];
        for (i, (goal, value, (best, best_color))) in rows.iter().enumerate() {
            let l = row_look(4 + i);
            let y = rect.y + 152. + i as f32 * 26. + l.offset[1];
            let a = alpha * l.alpha;
            if i % 2 == 0 {
                let stripe = Rect::new(rect.x + 14., y - 12., rect.w - 28., 24.);
                shapes::fill_rounded_rect(stripe, 6., Color::new(1., 1., 1., 0.035 * a));
            }
            look::glyph(look::goal_glyph(*goal), vec2(x0, y), 16., faded(INK, a));
            look::text_right(
                value.as_str(),
                x1 + 30.,
                y + 8.,
                110.,
                modern(16., faded(MINT, a)),
            );
            look::text_right(
                best.as_str(),
                x2,
                y + 8.,
                130.,
                modern(16., faded(*best_color, a)),
            );
        }
        // Session tallies as icon chips, then the one-more-try hook.
        let s = fun.stats;
        let l8 = row_look(8);
        let chips = [
            (Glyph::Tool(Icon::Drop), fmt_line!("{}", s.drops)),
            (Glyph::Tool(Icon::Pulse), fmt_line!("{}", s.pulses)),
            (Glyph::Tool(Icon::Chaos), fmt_line!("{}", s.chaos)),
            (Glyph::Pict(Pict::Skull), fmt_line!("{}", s.collapses)),
        ];
        let style = modern(16., faded(MUTED, alpha * l8.alpha));
        let total: f32 = chips
            .iter()
            .map(|(_, text)| look::icon_value_width(text.as_str(), 14., style) + 18.)
            .sum::<f32>()
            - 18.;
        let mut x = cx - total * 0.5;
        let cy = rect.y + rect.h - 50. + l8.offset[1];
        for (glyph, text) in &chips {
            x += look::icon_value(*glyph, text.as_str(), x, cy, 14., style) + 18.;
        }
        let l9 = row_look(9);
        let (glyph, hook) = f.bests.next_hook(fun);
        let hook_style = modern(16., faded(PINK, alpha * l9.alpha));
        let hook_w = look::icon_value_width(hook.as_str(), 16., hook_style);
        look::icon_value(
            glyph,
            hook.as_str(),
            cx - hook_w * 0.5,
            rect.y + rect.h - 24. + l9.offset[1],
            16.,
            hook_style,
        );
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
            let (x, y, row, full) = goal_block(w, wide, 48.);
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
                let half = (toast_max(w, top) + 40.) * 0.5;
                assert!(clear(w * 0.5 + half, top), "{w}: toast at {top}");
                assert!(w * 0.5 - half >= 0.);
            }
            for height in [300., 320.] {
                let card = card_rect(w, h, height);
                assert!(clear(card.x + card.w, card.y), "{w}x{h}: card");
                assert!(card.y + card.h <= h, "{w}x{h}: card fits");
            }
        }
    }

    #[test]
    fn records_store_only_meaningful_improvements_and_never_from_autoplay() {
        assert!(improved(None, 7., 6.5, false));
        assert!(
            !improved(None, 6., 6.5, false),
            "the authored pyramid is not a feat"
        );
        assert!(improved(Some(7.), 7.2, 6.5, false));
        assert!(!improved(Some(7.), 7.02, 6.5, false));
        assert!(improved(Some(3.), 2.5, 0., true));
        assert!(!improved(Some(2.5), 2.6, 0., true));
        assert!(!improved(None, f32::NAN, 0., false));
        let mut demo = Bests::default();
        let mut fun = Fun::default();
        fun.tower = 9.;
        assert!(demo.update(&fun).is_none());
        assert!(demo.tower.is_none(), "autoplay never writes a best");
        let mut player = Bests {
            enabled: true,
            tower: Some(8.),
            ..Default::default()
        };
        let best = player.update(&fun).expect("a beaten stored best");
        assert_eq!(best.glyph, look::goal_glyph(Goal::Skyscraper));
        assert_eq!(best.value, "9.0 m", "a value, not a sentence");
    }

    #[test]
    fn hooks_tease_the_secret_before_records() {
        let bests = Bests {
            tower: Some(8.2),
            ..Default::default()
        };
        let mut fun = Fun::default();
        let (glyph, text) = bests.next_hook(&fun);
        assert_eq!(glyph, Glyph::Pict(Pict::Rocket));
        assert_eq!(text.as_str(), "20 m?");
        fun.secret = true;
        let (glyph, text) = bests.next_hook(&fun);
        assert_eq!(glyph, look::goal_glyph(Goal::Skyscraper));
        assert!(text.as_str().contains("8.2 m"));
        assert!(text.as_str().split_whitespace().count() <= 3);
    }

    #[test]
    fn onboarding_fades_after_dropping_and_pulsing_or_half_a_minute() {
        let mut fun = Fun::default();
        assert!(!Hud::learned(&fun, 5.));
        fun.stats.drops = 1;
        assert!(!Hud::learned(&fun, 5.));
        fun.stats.pulses = 1;
        assert!(Hud::learned(&fun, 5.));
        assert!(Hud::learned(&Fun::default(), 31.));
    }

    #[test]
    fn every_tool_has_a_registered_action_and_a_short_name() {
        let mut input = PlayerInput::transient("physics-2d-tools");
        crate::register_lab_actions(&mut input);
        for (label, action, _) in TOOLS {
            assert!(label.len() <= 6);
            assert!(!hint(&input, action).as_str().is_empty());
        }
        for action in ["chaos", "domino", "stats", "step", "inspect"] {
            let key = hint(&input, action);
            assert_ne!(key.as_str(), "Unbound");
            assert!(look::shows_key(key.as_str()), "{action} gets a key cap");
        }
    }

    #[test]
    fn each_goal_points_at_the_tool_that_advances_it() {
        let mut fun = Fun::default();
        assert_eq!(goal_tool(&fun, Goal::Liftoff), Some(3));
        assert_eq!(goal_tool(&fun, Goal::Skyscraper), Some(1));
        assert_eq!(goal_tool(&fun, Goal::Demolition), Some(7));
        assert_eq!(goal_tool(&fun, Goal::DominoRun), Some(8));
        fun.scenario = Scenario::Dominoes;
        assert_eq!(goal_tool(&fun, Goal::DominoRun), None, "watch it fall");
        fun.goals[Goal::DominoRun.index()] = true;
        assert_eq!(goal_tool(&fun, Goal::DominoRun), Some(6), "reset to retry");
        for (index, (_, action, _)) in TOOLS.iter().enumerate() {
            let expected = match index {
                3 => "pulse",
                1 => "select_two",
                7 => "chaos",
                8 => "domino",
                6 => "restart",
                _ => continue,
            };
            assert_eq!(*action, expected);
        }
    }

    #[test]
    fn refusals_wobble_their_tool_and_pop_a_short_note_once() {
        let mut lab = Lab::new();
        let tools = [ToolState::default(); BUTTONS];
        let mut hud = Hud::default();
        hud.advance(&lab, true, 0.016, Motion::Full, &tools, false, false);
        assert!(
            hud.notes.iter().all(Option::is_none),
            "a fresh lab says nothing"
        );
        lab.update(
            0.,
            crate::LabInput {
                single_step: true,
                ..Default::default()
            },
        );
        assert_eq!(lab.notice, Notice::StepWhileRunning);
        hud.advance(&lab, true, 0.016, Motion::Full, &tools, false, false);
        assert!(hud.tools[5].denied() > 0.9, "the step tool wobbles");
        let note = hud.notes.iter().flatten().next().unwrap();
        assert_eq!((note.tool, note.text.as_str()), (Some(5), "Pause first"));
        assert!(note.text.as_str().split_whitespace().count() <= 2);
        // The same notice is not replayed on later frames.
        for _ in 0..12 {
            hud.advance(&lab, true, 0.1, Motion::Full, &tools, false, false);
        }
        assert!(hud.notes.iter().all(Option::is_none));
        assert_eq!(hud.tools[5].denied(), 0.);
        // A disabled-button click explains itself the same way.
        while lab.sandbox.spawn(vec2(0., 10.), true) {}
        hud.refuse(&lab, 2);
        assert!(hud.tools[2].denied() > 0.9 && hud.full > 0.9);
        let note = hud.notes.iter().flatten().next().unwrap();
        assert_eq!((note.tool, note.text.as_str()), (Some(2), "Full"));
        // Accepted steps pop a quiet "+1" without a wobble.
        lab.session.set_paused(true);
        lab.update(
            0.,
            crate::LabInput {
                single_step: true,
                ..Default::default()
            },
        );
        hud.advance(&lab, true, 0.016, Motion::Full, &tools, false, false);
        assert!(hud.notes.iter().flatten().any(|n| n.text.as_str() == "+1"));
    }

    #[test]
    fn title_card_leaves_on_input_or_time_and_rebuilds_play_a_reduced_safe_transition() {
        let lab = Lab::new();
        let tools = [ToolState::default(); BUTTONS];
        let mut hud = Hud::default();
        hud.advance(&lab, true, 0.1, Motion::Full, &tools, false, true);
        assert!(hud.intro.is_some(), "a launch-frame click doesn't skip it");
        for _ in 0..5 {
            hud.advance(&lab, true, 0.1, Motion::Full, &tools, false, false);
        }
        hud.advance(&lab, true, 0.01, Motion::Full, &tools, false, true);
        assert!(hud.intro.is_none());
        let mut idle = Hud::default();
        for _ in 0..40 {
            idle.advance(&lab, true, 0.1, Motion::Full, &tools, false, false);
        }
        assert!(idle.intro.is_none());
        hud.advance(&lab, true, 0., Motion::Reduced, &tools, true, false);
        assert!(hud.transition.coverage() > 0.9);
        assert_eq!(hud.transition.kind(Motion::Reduced), TransitionKind::Fade);
        for _ in 0..10 {
            hud.advance(&lab, true, 0.05, Motion::Reduced, &tools, false, false);
        }
        assert_eq!(hud.transition.coverage(), 0.);
    }
}
