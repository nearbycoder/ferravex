//! The lab's screen furniture: the title card (front door), top bar with the
//! current goal, toasts and pops, the pause chip, the tool palette, footer
//! status and tooltips, the optional engine diagnostics panel (G), the LAB
//! CERTIFIED card and scene transitions. Also owns personal bests.
//!
//! Drawing reads lab state and never changes it. Records are optional: storage
//! failure leaves a fully playable lab, and autoplay never writes a best.
//! Per-frame strings use stack [`Line`]s, so steady-state frames don't
//! allocate. Every movement here respects reduced motion through [`Motion`].
use super::juice::Juice;
use super::look::{
    self, AMBER, CYAN, GOLD, INK, Icon, Line, MINT, MUTED, PINK, ToolMotion, ToolState, faded,
    fmt_line, modern,
};
use super::presentation::{BUTTONS, LabLayout};
use super::quips;
use super::toys::{
    DEMOLITION_BONKS, Fun, Goal, LIFTOFF_HEIGHT, MOON_HEIGHT, Scenario, TOWER_HEIGHT,
};
use super::{Inspection, Lab, MAX_BODIES, TICK_HZ};
use ferravex_play::tween::{
    Ease, Entrance, Motion, RollCounter, ScreenTransition, Spring, TransitionKind,
};
use ferravex_render::input::PlayerInput;
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Fill};
use ferravex_render::storage::Records;
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_render::ui::kit::{self, KitTheme};

const RECORD_RULES: u64 = 0x5048_3244_0001;
const CARD_LIFE: f32 = 12.;
const INTRO: f32 = 3.4;

/// Tooltip copy per stable button index: (label, action, what it does, icon).
pub(super) const TOOLS: [(&str, &str, &str, Icon); BUTTONS] = [
    (
        "Ball",
        "select_one",
        "Bouncy. Click the lab to drop one.",
        Icon::Ball,
    ),
    (
        "Crate",
        "select_two",
        "Stackable. Mostly. Click to drop.",
        Icon::Crate,
    ),
    ("Drop", "drop", "Drop your shape from the top.", Icon::Drop),
    (
        "Pulse",
        "pulse",
        "Shove everything near the cursor.",
        Icon::Pulse,
    ),
    (
        "Pause",
        "pause",
        "Freeze time. Step and inspect.",
        Icon::Pause,
    ),
    ("Step", "step", "One 1/120 s tick while paused.", Icon::Step),
    ("Reset", "restart", "Rebuild this scene.", Icon::Reset),
    (
        "Chaos",
        "chaos",
        "Fourteen shapes. No refunds.",
        Icon::Chaos,
    ),
    (
        "Domino",
        "domino",
        "Swap to the domino run and back.",
        Icon::Domino,
    ),
];

/// The first binding of `action` (Controls lists the rest), on the stack.
pub(super) fn hint(input: &PlayerInput, action: &str) -> Line<32> {
    let mut buffer = [0; 96];
    let text = input.format_hint(action, &mut buffer).unwrap_or("Controls");
    let mut line = Line::<32>::new();
    let _ = std::fmt::Write::write_str(&mut line, text.split(" / ").next().unwrap_or(text));
    line
}

/// Browser fullscreen pins a 44 px menu button 8 px inside the canvas's
/// top-right corner. Text and panels stay out of x > width - 72, y < 60.
pub(super) const MENU_CORNER: Vec2 = Vec2::new(72., 60.);

/// Goal block `(x, y, first_row_width, full_width)`. The first row (headline
/// and counter) always stops short of the menu corner; on narrow screens the
/// brief and progress bar below y = 60 may use the full width.
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
    let width = 340_f32.min(w - 32.);
    let top = if w >= 780. { MENU_CORNER.y + 4. } else { 98. };
    Rect::new(w - 16. - width, top, width, 40. + rows as f32 * 20.)
}

/// Widest centered toast text; toasts reaching above y = 60 stay clear of
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
    let width = (w - 32.).min(560.);
    let height = height.min(h - MENU_CORNER.y - 16.);
    let top = (h * 0.5 - height * 0.5).max(MENU_CORNER.y + 4.);
    Rect::new(w * 0.5 - width * 0.5, top, width, height)
}

/// Room for the rank beside the scene name, short of the menu corner.
fn rank_width(w: f32, used: f32) -> f32 {
    (w - MENU_CORNER.x - 28. - used).min(300.)
}

/// Draws colored spans left to right; returns the total width.
fn spans(parts: &[(&str, Color)], x: f32, y: f32, max_width: f32, size: f32) -> f32 {
    let base = modern(size, INK);
    let total: f32 = parts.iter().map(|(t, _)| text_width(t, base)).sum();
    let style = if total > max_width && total > 0. {
        base.with_size((size * max_width / total).max(6.))
    } else {
        base
    };
    let mut cursor = x;
    for (part, color) in parts {
        cursor += text_width(part, style);
        draw_text_styled(
            part,
            cursor - text_width(part, style),
            y,
            style.with_color(*color),
        );
    }
    cursor - x
}

/// A stored best for the card: "—" when none exists, gold when this session
/// set it.
fn best_cell(
    session: f32,
    stored: Option<f32>,
    lower_is_better: bool,
    decimals: usize,
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
                format_args!("{best:.decimals$}{}", if set { " ★" } else { "" }),
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

    /// Store improved session values. Returns a headline only when a stored
    /// best existed and was beaten, so a first visit never claims a record.
    pub fn update(&mut self, fun: &Fun) -> Option<String> {
        if !self.enabled {
            return None;
        }
        let mut headline = None;
        let store = |records: &mut Option<Records>, id: &str, score: u64, time: Option<u64>| {
            if let Some(records) = records.as_mut() {
                let _ = records.attempt(id, RECORD_RULES, score, time);
            }
        };
        if improved(self.tower, fun.tower, 6.5, false) {
            if self.tower.is_some() {
                headline = Some(format!("Tallest tower · {:.1} m", fun.tower));
            }
            self.tower = Some(fun.tower);
            store(&mut self.records, "tower", (fun.tower * 100.) as u64, None);
        }
        if improved(self.altitude, fun.altitude, 8., false) {
            if self.altitude.is_some() && fun.altitude > self.altitude.unwrap_or(0.) + 0.5 {
                headline = Some(format!("Highest flight · {:.1} m", fun.altitude));
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
                headline = Some(format!("{} bonks per second", fun.bonk_peak));
            }
            self.bonks = Some(fun.bonk_peak);
            store(&mut self.records, "bonks", u64::from(fun.bonk_peak), None);
        }
        if let Some(seconds) = fun.domino_seconds()
            && improved(self.domino, seconds, 0., true)
        {
            if self.domino.is_some() {
                headline = Some(format!("Fastest domino run · {seconds:.2} s"));
            }
            self.domino = Some(seconds);
            store(
                &mut self.records,
                "domino",
                1,
                Some((seconds * 1_000_000.) as u64),
            );
        }
        headline
    }

    /// The "one more try" hook once every goal is done.
    pub fn next_hook(&self, fun: &Fun) -> Line {
        if !fun.secret {
            return fmt_line!("Secret: something can fly past {MOON_HEIGHT:.0} m…");
        }
        if let Some(best) = self.tower {
            return fmt_line!("Next: beat your tallest tower ({best:.1} m)");
        }
        if let Some(best) = self.bonks {
            return fmt_line!("Next: beat {best} bonks per second");
        }
        fmt_line!("Next: stack higher. Always higher.")
    }
}

/// Screen-space HUD state that is not simulation.
pub(super) struct Hud {
    /// Engine diagnostics panel (G). Off by default to keep the scene clean.
    pub stats: bool,
    /// Seconds since LAB CERTIFIED appeared.
    pub card: Option<f32>,
    hint: f32,
    active: f32,
    /// Seconds since launch while the title card is up.
    intro: Option<f32>,
    tools: [ToolMotion; BUTTONS],
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
            active: 0.,
            intro: Some(0.),
            tools: [ToolMotion::default(); BUTTONS],
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
    pub hovered: Option<usize>,
    pub inspection: Option<&'a Inspection>,
    pub active: bool,
    pub tools: [ToolState; BUTTONS],
}

impl Hud {
    /// Onboarding stays until the player has dropped and pulsed something, or
    /// 30 seconds of active play pass; then it fades out for good.
    pub fn learned(fun: &Fun, active_seconds: f32) -> bool {
        (fun.stats.drops > 0 && fun.stats.pulses > 0) || active_seconds > 30.
    }

    /// Advance HUD motion by bounded render time. `rebuilt` starts the scene
    /// transition; `input` (any accepted press or click) dismisses the title.
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
        let target = if Self::learned(&lab.fun, self.active) {
            0.
        } else {
            1.
        };
        self.hint += (target - self.hint).clamp(-dt * 1.5, dt * 4.);
        if let Some(age) = &mut self.intro {
            *age += dt;
            if *age > INTRO || (input && *age > 0.25) {
                self.intro = None;
            }
        }
        for (motion_state, state) in self.tools.iter_mut().zip(tools) {
            motion_state.update(*state, dt, motion);
        }
        self.stats_open.set_target(if self.stats { 1. } else { 0. });
        self.stats_open.advance(dt, motion);
        if rebuilt {
            self.transition.cover();
            self.transition.advance(10., Motion::Full);
            self.transition.reveal();
        }
        self.transition.advance(dt, motion);
        if let Some(age) = &mut self.card {
            *age += dt;
            if *age > CARD_LIFE {
                self.card = None;
            }
        }
        let fun = &lab.fun;
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
        self.top_bar(f);
        self.sky_card(f);
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
        let pulse =
            0.65 + 0.35 * (self.time * 3.).sin() * if self.motion.is_reduced() { 0. } else { 1. };
        let pip_color = if f.lab.session.is_paused() {
            AMBER
        } else {
            MINT
        };
        shapes::glow_disc(pip, 7., faded(pip_color, 0.55 * pulse));
        shapes::fill_circle(pip, 2.5, pip_color);
        let rank = quips::rank(fun.goals_done(), fun.secret);
        let scene = fun.scenario.name();
        let sub = modern(8., AMBER);
        let used = draw_text_styled(scene, 16., 37., sub).width;
        look::text(
            fmt_line!("· {}", rank.title).as_str(),
            22. + used,
            37.,
            rank_width(w, used),
            sub.with_color(MUTED),
        );

        // The goal block sits right of the title on wide screens and below
        // it on narrow screens, where the scene leaves vertical room.
        let (x, y, row, width) = goal_block(w, wide, 50.);
        if !wide && f.layout.scene_top() < 96. {
            return;
        }
        if !wide {
            let strip = Rect::new(0., 45., w, 44.);
            shapes::fill_rounded_rect(strip, 0., Color::new(0.03, 0.05, 0.09, 0.82));
        }
        // In the domino scene, its own run is the headline even when another
        // goal comes first in teaching order.
        let shown = if fun.scenario == Scenario::Dominoes {
            Some(Goal::DominoRun)
        } else {
            fun.current_goal()
        };
        match shown {
            Some(goal) => {
                let (value, target) = fun.progress(goal);
                let done = fun.goals[goal.index()];
                let head = if done {
                    fmt_line!(
                        "Domino run · best {:.2} s",
                        fun.domino_seconds().unwrap_or(0.)
                    )
                } else {
                    fmt_line!("Goal {}/4 · {}", goal.index() + 1, goal.title())
                };
                let amount = match goal {
                    Goal::Liftoff | Goal::Skyscraper => fmt_line!("{value:.1} / {target:.1} m"),
                    Goal::Demolition => fmt_line!("{value:.0} / {target:.0} per s"),
                    Goal::DominoRun => fmt_line!("{value:.0} / {target:.0} down"),
                };
                let color = if done { MINT } else { GOLD };
                let amount_style = modern(16., INK);
                let amount_w = text_width(amount.as_str(), amount_style).min(row * 0.4);
                look::text(
                    head.as_str(),
                    x,
                    y + 19.,
                    row - amount_w - 16.,
                    modern(16., color),
                );
                look::text_right(amount.as_str(), x + row, y + 19., amount_w, amount_style);
                let brief = if goal == Goal::DominoRun && fun.scenario != Scenario::Dominoes {
                    fmt_line!(
                        "{} Press {} to set it up.",
                        goal.brief(),
                        hint(f.input, "domino").as_str()
                    )
                } else if done {
                    fmt_line!("Knock them down again: Reset rebuilds the run.")
                } else {
                    fmt_line!("{}", goal.brief())
                };
                look::text(brief.as_str(), x, y + 34., width, modern(8., MUTED));
                look::progress(Rect::new(x, y + 39., width, 3.), value / target, color, 1.);
                // Emissive head on the progress line.
                let head_x = x + width * (value / target).clamp(0., 1.);
                if value > 0. {
                    shapes::glow_disc(vec2(head_x, y + 40.5), 6., faded(color, 0.7));
                }
            }
            None => {
                look::text(
                    fmt_line!("All goals done · {}", rank.title).as_str(),
                    x,
                    y + 19.,
                    row,
                    modern(16., MINT),
                );
                look::text(
                    f.bests.next_hook(fun).as_str(),
                    x,
                    y + 34.,
                    width,
                    modern(8., INK),
                );
            }
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

    /// On narrow portrait screens the sky above the lab holds a how-to card.
    fn sky_card(&self, f: &Frame) {
        let band = Self::band_bottom(f);
        let void = f.layout.scene_top() - band;
        let w = screen_width();
        if void < 170. || self.hint <= 0.01 || f.juice.toast.is_some() || self.intro.is_some() {
            return;
        }
        let alpha = self.hint;
        let theme = KitTheme::default().faded(alpha);
        let top = band + void * 0.5 - 62.;
        look::text_center(
            "How to lab",
            w * 0.5,
            top,
            w - 40.,
            modern(16., faded(GOLD, alpha)),
        );
        let rows = [
            ("pulse", "Pulse"),
            ("chaos", "Chaos"),
            ("domino", "Domino run"),
        ];
        look::text_center(
            "Tap or click the lab to drop",
            w * 0.5,
            top + 26.,
            w - 40.,
            modern(16., faded(INK, alpha)),
        );
        for (i, (action, label)) in rows.iter().enumerate() {
            let key = hint(f.input, action);
            let width = text_width(key.as_str(), modern(16., INK))
                + 24.
                + 8.
                + text_width(label, modern(16., INK));
            kit::key_prompt(
                w * 0.5 - width * 0.5,
                top + 38. + i as f32 * 30.,
                &[key.as_str()],
                label,
                &theme,
            );
        }
    }

    fn onboarding(f: &Frame, x: f32, y: f32, width: f32, alpha: f32) {
        let (pulse, chaos, domino, stats) = (
            hint(f.input, "pulse"),
            hint(f.input, "chaos"),
            hint(f.input, "domino"),
            hint(f.input, "stats"),
        );
        let key = faded(AMBER, alpha);
        let ink = faded(INK, alpha);
        if screen_width() < 560. {
            spans(
                &[
                    ("Click", key),
                    (" drops · ", ink),
                    (pulse.as_str(), key),
                    (" pulse · ", ink),
                    (chaos.as_str(), key),
                    (" chaos · ", ink),
                    (domino.as_str(), key),
                    (" domino", ink),
                ],
                x,
                y,
                width,
                16.,
            );
        } else {
            spans(
                &[
                    ("Click", key),
                    (" drop · ", ink),
                    ("Right-click", key),
                    (" crate · ", ink),
                    (pulse.as_str(), key),
                    (" pulse · ", ink),
                    (chaos.as_str(), key),
                    (" chaos · ", ink),
                    (domino.as_str(), key),
                    (" dominoes · ", ink),
                    (stats.as_str(), key),
                    (" stats", ink),
                ],
                x,
                y,
                width,
                16.,
            );
        }
    }

    fn world_marks(&self, f: &Frame) {
        // Target line for height goals, labelled at the right wall.
        let fun = &f.lab.fun;
        let line = match fun.current_goal() {
            Some(Goal::Liftoff) => Some((LIFTOFF_HEIGHT, "Liftoff")),
            Some(Goal::Skyscraper) => Some((TOWER_HEIGHT, "Skyscraper")),
            _ => None,
        };
        if let Some((height, label)) = line
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
            look::text_right(
                fmt_line!("{label} {height:.1} m").as_str(),
                b.x - 4.,
                a.y - 6.,
                200.,
                modern(16., faded(GOLD, 0.9)),
            );
        }
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

    fn toast(&self, f: &Frame) {
        let w = screen_width();
        let band = Self::band_bottom(f);
        let void = f.layout.scene_top() - band;
        // Narrow portrait screens leave sky above the lab: use it.
        let mut top = if void >= 110. {
            band + void * 0.5 - 44.
        } else {
            (f.layout.scene_top() + 10.).max(band + 10.)
        };
        if self.stats && w < 780. {
            // The stats panel spans narrow screens; callouts go below it.
            top = top.max(98. + 40. + 6. * 20. + 8.);
        }
        let paused_quip;
        let (head, sub, color, alpha, age) = match &f.juice.toast {
            Some(toast) => (
                toast.head.as_str(),
                toast.sub.as_str(),
                toast.color,
                toast.alpha(),
                toast.age,
            ),
            None if f.lab.session.is_paused() && f.active && self.intro.is_none() => {
                paused_quip = fmt_line!(
                    "{} · {} steps · {} inspects",
                    quips::pause(f.lab.session.tick() as u32),
                    hint(f.input, "step").as_str(),
                    hint(f.input, "inspect").as_str()
                );
                ("PAUSED", paused_quip.as_str(), AMBER, 1., 1.)
            }
            None => return,
        };
        // With engine stats open, center in the space left of that panel.
        let room = if self.stats && w >= 780. { w - 356. } else { w };
        let w = room;
        let max = toast_max(w, top);
        let head_style =
            TextStyle::classic(24., color).with_outline(Color::new(0.02, 0.03, 0.06, 0.85));
        let sub_style = modern(16., INK);
        let lines = wrap_text(sub, max, sub_style).take(2);
        let mut widest: f32 = 0.;
        let mut count = 0;
        for line in lines.clone() {
            widest = widest.max(text_width(line, sub_style));
            count += 1;
        }
        let width = text_width(head, head_style).max(widest).min(max) + 40.;
        let height = 46. + count as f32 * 20.;
        let look = Entrance::pop(0.86).sample(age, 0, self.motion);
        let alpha = alpha * look.alpha;
        let rect = kit::appear(Rect::new(w * 0.5 - width * 0.5, top, width, height), look);
        look::panel(rect, 14., alpha);
        // Accent: a glowing underline and a soft halo behind the headline.
        let accent = Rect::new(rect.x + 18., rect.y + rect.h - 4., rect.w - 36., 2.);
        shapes::fill_rounded_rect(accent, 1., faded(color, 0.8 * alpha));
        shapes::glow_disc(
            vec2(rect.center().x, rect.y + 22.),
            rect.w * 0.3,
            faded(color, 0.12 * alpha),
        );
        let head_style = look::fit(head, head_style.with_color(faded(color, alpha)), max)
            .with_outline(faded(Color::new(0.02, 0.03, 0.06, 0.85), alpha))
            .with_align(TextAlign::Center);
        draw_text_styled(head, rect.center().x, rect.y + 32., head_style);
        for (row, line) in wrap_text(sub, max, sub_style).take(2).enumerate() {
            draw_text_styled(
                line,
                rect.center().x,
                rect.y + 54. + row as f32 * 20.,
                sub_style
                    .with_color(faded(INK, alpha))
                    .with_shadow(faded(look::TEXT_SHADOW, alpha))
                    .with_align(TextAlign::Center),
            );
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
        for (index, (label, action, _, glyph)) in TOOLS.iter().enumerate() {
            let (label, glyph) = if index == 4 && lab.session.is_paused() {
                ("Resume", Icon::Play)
            } else {
                (*label, *glyph)
            };
            let accent = match index {
                0 => CYAN,
                1 => AMBER,
                7 => PINK,
                8 => MINT,
                _ => GOLD,
            };
            look::tool_button(
                layout.button(index),
                glyph,
                label,
                hint(f.input, action).as_str(),
                f.tools[index],
                &self.tools[index],
                accent,
            );
        }
        let y = layout.status_y() + 4.;
        let wide = layout.single_row();
        let (state, color) = if !f.active {
            ("Suspended", MUTED)
        } else if lab.session.is_paused() {
            ("Paused", AMBER)
        } else {
            ("Running", MINT)
        };
        // Emissive state dot.
        let dot = vec2(34., y - 5.);
        shapes::glow_disc(dot, 9., faded(color, 0.45));
        shapes::fill_circle(dot, 3.5, color);
        let x0 = 46.;
        let used = draw_text_styled(state, x0, y, modern(16., color)).width;
        let shape = if lab.ball { "Ball" } else { "Crate" };
        let summary = fmt_line!(
            " · {} / {} bodies · {} ready",
            lab.sandbox.bodies.len(),
            MAX_BODIES,
            shape
        );
        let summary_w = look::text(
            summary.as_str(),
            x0 + used,
            y,
            if wide { w * 0.42 } else { w - 56. - x0 - used },
            modern(16., INK),
        );
        // Tooltip for the hovered tool, else a fresh notice.
        let tooltip = f.hovered.map(|index| {
            let (label, action, what, _) = TOOLS[index];
            (label, hint(f.input, action), what)
        });
        let notice = (lab.feedback_time > 0.).then_some(lab.feedback);
        let selected = lab.selected.map(|_| lab.selected_label());
        let draw_tip = |x: f32, y: f32, width: f32| -> bool {
            if let Some((label, key, what)) = &tooltip {
                spans(
                    &[
                        (label, GOLD),
                        (" ", INK),
                        (key.as_str(), AMBER),
                        (" · ", MUTED),
                        (what, INK),
                    ],
                    x,
                    y,
                    width,
                    16.,
                );
                true
            } else if let Some(notice) = notice {
                look::text(notice, x, y, width, modern(16., CYAN));
                true
            } else {
                false
            }
        };
        if wide {
            let x = x0 + used + summary_w + 24.;
            draw_tip(x, y, w - 28. - x);
            self.second_line(f, selected, 28., y + 20., w - 56.);
        } else {
            draw_tip(28., y + 20., w - 56.);
            self.second_line(f, selected, 28., y + 40., w - 56.);
        }
    }

    /// Selected-body diagnostics win; otherwise onboarding (fading once
    /// learned) cross-fades into a quieter standing tip.
    fn second_line(&self, f: &Frame, selected: Option<String>, x: f32, y: f32, width: f32) {
        if let Some(selected) = selected {
            look::text(&selected, x, y, width, modern(16., MUTED));
            return;
        }
        let sky = f.layout.scene_top() - Self::band_bottom(f) >= 170.;
        let onboarding = if sky { 0. } else { self.hint };
        if onboarding > 0.01 {
            Self::onboarding(f, x, y, width, onboarding);
        }
        if onboarding < 0.99 {
            let tip = fmt_line!(
                "Tip: {} inspects the body under the cursor · {} shows engine stats",
                hint(f.input, "inspect").as_str(),
                hint(f.input, "stats").as_str()
            );
            look::text(
                tip.as_str(),
                x,
                y,
                width,
                modern(16., faded(MUTED, 1. - onboarding)),
            );
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
            Some(q) if q.report.is_truncated() => fmt_line!(
                "Query {}/{} hits at cursor (truncated)",
                q.report.written,
                q.report.total
            ),
            Some(q) => fmt_line!("Query {} hits at cursor", q.report.total),
            None => fmt_line!("Query: cursor outside the scene"),
        };
        let pulse = if lab.sandbox.pulse_time > 0. {
            fmt_line!("Pulse recharging {:.2} s", lab.sandbox.pulse_time)
        } else {
            fmt_line!("Pulse ready")
        };
        let lines = [
            fmt_line!(
                "Tick {} · {:.0} Hz · {:.2} ms",
                lab.session.tick(),
                TICK_HZ,
                1000. / TICK_HZ
            ),
            fmt_line!(
                "Awake {} · asleep {} · {}/{} bodies",
                sleep.awake_bodies,
                sleep.sleeping_bodies,
                lab.sandbox.bodies.len(),
                MAX_BODIES
            ),
            fmt_line!(
                "Last tick: {} contacts · +{started} -{stopped}",
                physics.contacts().count()
            ),
            query,
            pulse,
            fmt_line!(
                "Bonks {}/s · peak {} · goal {DEMOLITION_BONKS}",
                lab.fun.bonks_now,
                lab.fun.bonk_peak
            ),
        ];
        let open = self.stats_open.value.clamp(0., 1.);
        let rect = stats_rect(w, lines.len());
        let look_in = Ease::CubicOut.apply(open);
        let rect = Rect::new(rect.x + (1. - look_in) * 24., rect.y, rect.w, rect.h);
        look::panel(rect, 14., open);
        let inner = rect.w - 28.;
        look::text(
            "Engine stats",
            rect.x + 14.,
            rect.y + 24.,
            inner,
            modern(16., faded(AMBER, open)),
        );
        for (row, line) in lines.iter().enumerate() {
            look::text(
                line.as_str(),
                rect.x + 14.,
                rect.y + 46. + row as f32 * 20.,
                inner,
                modern(16., faded(INK, open)),
            );
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
        let badge = Entrance::fade_up(10.).sample(age - 0.15, 0, self.motion);
        look::text_center(
            "2D · a workshop of questionable engineering",
            center.x,
            center.y + 30. + badge.offset[1],
            w - 40.,
            modern(16., faded(GOLD, alpha * badge.alpha)),
        );
        let blink = if self.motion.is_reduced() {
            1.
        } else {
            0.55 + 0.45 * (age * 4.).sin().abs()
        };
        let cta = Entrance::fade_up(10.).sample(age - 0.35, 0, self.motion);
        look::text_center(
            "Click anywhere to start making a mess",
            center.x,
            center.y + 60. + cta.offset[1],
            w - 40.,
            modern(16., faded(INK, alpha * cta.alpha * blink)),
        );
    }

    fn certificate(&self, f: &Frame, age: f32) {
        let w = screen_width();
        let h = screen_height();
        let fun = &f.lab.fun;
        let alpha = (age / 0.3).min(1.) * ((CARD_LIFE - age) / 0.5).clamp(0., 1.);
        draw_rectangle(0., 0., w, h, faded(Color::new(0., 0., 0., 0.5), alpha));
        let rect = card_rect(w, h, 340.);
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
        look::text_center(
            "★ Lab certified ★",
            cx,
            rect.y + 30. + l0.offset[1],
            inner,
            modern(16., faded(AMBER, alpha * l0.alpha)),
        );
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
            rect.y + 72. + l1.offset[1],
            look::fit(rank.title, title, inner),
        );
        let l2 = row_look(2);
        for (i, line) in wrap_text(rank.quip, inner, modern(16., INK))
            .take(2)
            .enumerate()
        {
            look::text_center(
                line,
                cx,
                rect.y + 98. + i as f32 * 18. + l2.offset[1],
                inner,
                modern(16., faded(INK, alpha * l2.alpha)),
            );
        }
        let x0 = rect.x + 24.;
        let x1 = rect.x + rect.w * 0.6;
        let x2 = rect.x + rect.w - 24.;
        let l3 = row_look(3);
        let caption = modern(8., faded(MUTED, alpha * l3.alpha));
        look::text_right("This session", x1 + 30., rect.y + 146., 120., caption);
        look::text_right("Best", x2, rect.y + 146., 80., caption);
        let shown = |i: usize| self.rolls[i].shown();
        let rows = [
            (
                "Tallest tower",
                fmt_line!("{:.1} m", shown(0) as f32 / 10.),
                best_cell(fun.tower, f.bests.tower, false, 1),
            ),
            (
                "Highest flight",
                fmt_line!("{:.1} m", shown(1) as f32 / 10.),
                best_cell(fun.altitude, f.bests.altitude, false, 1),
            ),
            (
                "Bonks per second",
                fmt_line!("{}", shown(2)),
                best_cell(
                    fun.bonk_peak as f32,
                    f.bests.bonks.map(|b| b as f32),
                    false,
                    0,
                ),
            ),
            (
                "Domino run",
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
                ),
            ),
        ];
        for (i, (label, value, (best, best_color))) in rows.iter().enumerate() {
            let l = row_look(4 + i);
            let y = rect.y + 170. + i as f32 * 26. + l.offset[1];
            let a = alpha * l.alpha;
            if i % 2 == 0 {
                let stripe = Rect::new(rect.x + 14., y - 17., rect.w - 28., 24.);
                shapes::fill_rounded_rect(stripe, 6., Color::new(1., 1., 1., 0.035 * a));
            }
            look::text(label, x0, y, x1 - x0 - 60., modern(16., faded(INK, a)));
            look::text_right(
                value.as_str(),
                x1 + 30.,
                y,
                110.,
                modern(16., faded(MINT, a)),
            );
            look::text_right(
                best.as_str(),
                x2,
                y,
                110.,
                modern(16., faded(*best_color, a)),
            );
        }
        let s = fun.stats;
        let l8 = row_look(8);
        look::text_center(
            fmt_line!(
                "Drops {} · pulses {} · chaos {} · collapses {}",
                s.drops,
                s.pulses,
                s.chaos,
                s.collapses
            )
            .as_str(),
            cx,
            rect.y + rect.h - 48. + l8.offset[1],
            inner,
            modern(16., faded(MUTED, alpha * l8.alpha)),
        );
        let l9 = row_look(9);
        look::text_center(
            f.bests.next_hook(fun).as_str(),
            cx,
            rect.y + rect.h - 22. + l9.offset[1],
            inner,
            modern(16., faded(PINK, alpha * l9.alpha)),
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
            let (x, y, row, full) = goal_block(w, wide, 50.);
            // First row glyphs span roughly y + 3 ..= y + 19.
            assert!(clear(x + row, y + 3.), "{w}: goal row");
            assert!(full <= w - 32. && x >= 16.);
            if wide {
                assert_eq!(row, full, "wide brief and bar share the corner band");
            }
            assert!(
                rank_width(w, 70.) + 22. + 70. <= w - MENU_CORNER.x,
                "{w}: rank"
            );
            let stats = stats_rect(w, 6);
            assert!(clear(stats.x + stats.w, stats.y), "{w}: stats");
            for top in [56., 58., 94., 104.] {
                let half = (toast_max(w, top) + 40.) * 0.5;
                assert!(clear(w * 0.5 + half, top), "{w}: toast at {top}");
                assert!(w * 0.5 - half >= 0.);
            }
            for height in [300., 340.] {
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
    }

    #[test]
    fn hooks_tease_the_secret_before_records() {
        let bests = Bests {
            tower: Some(8.2),
            ..Default::default()
        };
        let mut fun = Fun::default();
        assert!(bests.next_hook(&fun).as_str().starts_with("Secret"));
        fun.secret = true;
        assert!(bests.next_hook(&fun).as_str().contains("8.2 m"));
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
    fn every_tool_has_a_registered_action_and_short_copy() {
        let mut input = PlayerInput::transient("physics-2d-tools");
        crate::register_lab_actions(&mut input);
        for (label, action, what, _) in TOOLS {
            assert!(label.len() <= 6);
            assert!(what.len() <= 40, "{what}");
            assert!(!hint(&input, action).as_str().is_empty());
        }
        for action in ["chaos", "domino", "stats"] {
            assert_ne!(hint(&input, action).as_str(), "Unbound");
        }
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
