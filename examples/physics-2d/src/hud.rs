//! The lab's screen furniture: top bar with the current goal, toasts and pops,
//! the pause chip, onboarding hint, footer status/tooltips, the optional engine
//! diagnostics panel (G) and the LAB CERTIFIED card. Also owns personal bests.
//!
//! Drawing reads lab state and never changes it. Records are optional: storage
//! failure leaves a fully playable lab, and autoplay never writes a best.
use super::juice::{GOLD, Juice, MINT, PINK};
use super::presentation::{LabLayout, label_lines};
use super::quips;
use super::toys::{
    DEMOLITION_BONKS, Fun, Goal, LIFTOFF_HEIGHT, MOON_HEIGHT, Scenario, TOWER_HEIGHT,
};
use super::{Inspection, Lab, MAX_BODIES, TICK_HZ};
use ferravex_render::prelude::*;
use ferravex_render::storage::Records;
use ferravex_render::ui;

const RECORD_RULES: u64 = 0x5048_3244_0001;
const CARD_LIFE: f32 = 12.;

/// Tooltip copy per stable button index: (label, action, what it does).
pub(super) const TOOLS: [(&str, &str, &str); 9] = [
    ("BALL", "select_one", "BOUNCY. CLICK THE SCENE TO DROP ONE"),
    ("CRATE", "select_two", "STACKABLE. MOSTLY. CLICK TO DROP"),
    ("DROP", "drop", "DROP YOUR SHAPE FROM THE TOP"),
    ("PULSE", "pulse", "SHOVE EVERYTHING NEAR THE CURSOR"),
    ("PAUSE", "pause", "FREEZE TIME. STEP AND INSPECT"),
    ("STEP", "step", "ONE 1/120 S TICK WHILE PAUSED"),
    ("RESET", "restart", "REBUILD THIS SCENE"),
    ("CHAOS", "chaos", "FOURTEEN SHAPES. NO REFUNDS"),
    ("DOMINO", "domino", "SWAP TO THE DOMINO RUN AND BACK"),
];

pub(super) fn hint(input: &ferravex_render::input::PlayerInput, action: &str) -> String {
    let mut buffer = [0; 96];
    let text = input.format_hint(action, &mut buffer).unwrap_or("CONTROLS");
    // Keep only the first family/binding to stay compact; Controls lists all.
    text.split(" / ")
        .next()
        .unwrap_or(text)
        .to_ascii_uppercase()
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
    let width = 330_f32.min(w - 32.);
    let top = if w >= 780. { MENU_CORNER.y + 4. } else { 98. };
    Rect::new(w - 16. - width, top, width, 20. + rows as f32 * 18.)
}

/// Widest centered toast text; toasts reaching above y = 60 stay clear of
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
    let width = (w - 32.).min(560.);
    let height = height.min(h - 32.);
    let top = (h * 0.5 - height * 0.5).max(MENU_CORNER.y + 4.);
    Rect::new(w * 0.5 - width * 0.5, top, width, height)
}

/// Room for the rank beside the scene name, short of the menu corner.
fn rank_width(w: f32, used: f32) -> f32 {
    (w - MENU_CORNER.x - 28. - used).min(300.)
}

fn width_of(text: &str, size: f32) -> f32 {
    measure_text(text, None, size as u16, 1.).width
}

/// Draw text scaled down (never up) to fit `max_width`.
pub(super) fn fitted(text: &str, x: f32, y: f32, max_width: f32, size: f32, color: Color) -> f32 {
    let measured = width_of(text, size).max(1.);
    let scale = (max_width.max(1.) / measured).min(1.);
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font_size: size as u16,
            font_scale: scale,
            color,
        },
    );
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

fn faded(color: Color, alpha: f32) -> Color {
    Color {
        a: color.a * alpha.clamp(0., 1.),
        ..color
    }
}

/// A stored best for the card: "—" when none exists, gold when this session
/// set it.
fn best_cell(
    session: f32,
    stored: Option<f32>,
    lower_is_better: bool,
    show: impl Fn(f32) -> String,
) -> (String, Color) {
    match stored {
        None => ("—".into(), ui::MUTED),
        Some(best) => {
            let set = if lower_is_better {
                session <= best + 0.001
            } else {
                session >= best - 0.001
            };
            if set {
                (format!("{} NEW!", show(best)), GOLD)
            } else {
                (show(best), ui::AMBER)
            }
        }
    }
}

fn bar(x: f32, y: f32, w: f32, h: f32, ratio: f32, color: Color) {
    draw_rectangle(x, y, w, h, ui::LINE);
    draw_rectangle(x, y, w * ratio.clamp(0., 1.), h, color);
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
                headline = Some(format!("NEW BEST TOWER · {:.1} M", fun.tower));
            }
            self.tower = Some(fun.tower);
            store(&mut self.records, "tower", (fun.tower * 100.) as u64, None);
        }
        if improved(self.altitude, fun.altitude, 8., false) {
            if self.altitude.is_some() && fun.altitude > self.altitude.unwrap_or(0.) + 0.5 {
                headline = Some(format!("NEW BEST FLIGHT · {:.1} M", fun.altitude));
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
                headline = Some(format!("NEW BEST · {} BONKS/S", fun.bonk_peak));
            }
            self.bonks = Some(fun.bonk_peak);
            store(&mut self.records, "bonks", u64::from(fun.bonk_peak), None);
        }
        if let Some(seconds) = fun.domino_seconds()
            && improved(self.domino, seconds, 0., true)
        {
            if self.domino.is_some() {
                headline = Some(format!("NEW BEST DOMINO RUN · {seconds:.2} S"));
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
    pub fn next_hook(&self, fun: &Fun) -> String {
        if !fun.secret {
            return format!("SECRET: SOMETHING CAN FLY PAST {MOON_HEIGHT:.0} M...");
        }
        if let Some(best) = self.tower {
            return format!("NEXT: BEAT YOUR TALLEST TOWER ({best:.1} M)");
        }
        if let Some(best) = self.bonks {
            return format!("NEXT: BEAT {best} BONKS PER SECOND");
        }
        "NEXT: STACK HIGHER. ALWAYS HIGHER.".into()
    }
}

/// Screen-space HUD state that is not simulation.
#[derive(Default)]
pub(super) struct Hud {
    /// Engine diagnostics panel (G). Off by default to keep the scene clean.
    pub stats: bool,
    /// Seconds since LAB CERTIFIED appeared.
    pub card: Option<f32>,
    hint: f32,
    active: f32,
}

pub(super) struct Frame<'a> {
    pub lab: &'a Lab,
    pub layout: &'a LabLayout,
    pub view: &'a LabLayout,
    pub juice: &'a Juice,
    pub bests: &'a Bests,
    pub input: &'a ferravex_render::input::PlayerInput,
    pub hovered: Option<usize>,
    pub inspection: Option<&'a Inspection>,
    pub active: bool,
}

impl Hud {
    /// Onboarding stays until the player has dropped and pulsed something, or
    /// 30 seconds of active play pass; then it fades out for good.
    pub fn learned(fun: &Fun, active_seconds: f32) -> bool {
        (fun.stats.drops > 0 && fun.stats.pulses > 0) || active_seconds > 30.
    }

    pub fn advance(&mut self, lab: &Lab, active: bool, dt: f32) {
        let dt = if dt.is_finite() {
            dt.clamp(0., 0.1)
        } else {
            0.
        };
        if active && lab.session.is_playing() {
            self.active += dt;
        }
        let target = if Self::learned(&lab.fun, self.active) {
            0.
        } else {
            1.
        };
        self.hint += (target - self.hint).clamp(-dt * 1.5, dt * 4.);
        if let Some(age) = &mut self.card {
            *age += dt;
            if *age > CARD_LIFE {
                self.card = None;
            }
        }
        if lab
            .fun
            .events
            .goals
            .iter()
            .any(|_| lab.fun.goals_done() == 4)
        {
            self.card = Some(0.);
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

    pub fn draw(&self, f: &Frame) {
        self.top_bar(f);
        self.sky_card(f);
        self.world_text(f);
        self.toast(f);
        self.footer(f);
        if self.stats {
            self.diagnostics(f);
        }
        if let Some(age) = self.card {
            self.certificate(f, age);
        }
    }

    fn top_bar(&self, f: &Frame) {
        let w = screen_width();
        let fun = &f.lab.fun;
        let wide = w >= 780.;
        draw_rectangle(0., 0., w, 44., Color::from_rgba(12, 19, 30, 235));
        draw_line(0., 44., w, 44., 1., ui::LINE);
        fitted("2D PHYSICS LAB", 16., 21., 220., 16., ui::INK);
        let rank = quips::rank(fun.goals_done(), fun.secret);
        let scene = fun.scenario.name();
        let used = fitted(scene, 16., 37., 120., 11., ui::AMBER);
        fitted(
            &format!("· {}", rank.title),
            22. + used,
            37.,
            rank_width(w, used),
            11.,
            ui::MUTED,
        );

        // The goal block sits right of the title on wide screens and below
        // it on narrow screens, where the scene leaves vertical room.
        let (x, y, row, width) = goal_block(w, wide, 50.);
        if !wide && f.layout.scene_top() < 96. {
            return;
        }
        if !wide {
            draw_rectangle(0., 44., w, 44., Color::from_rgba(12, 19, 30, 200));
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
                    format!(
                        "DOMINO RUN · BEST {:.2} S",
                        fun.domino_seconds().unwrap_or(0.)
                    )
                } else {
                    format!("GOAL {}/4 · {}", goal.index() + 1, goal.title())
                };
                let amount = match goal {
                    Goal::Liftoff | Goal::Skyscraper => format!("{value:.1} / {target:.1} M"),
                    Goal::Demolition => format!("{value:.0} / {target:.0} PER S"),
                    Goal::DominoRun => format!("{value:.0} / {target:.0} DOWN"),
                };
                let amount_w = width_of(&amount, 12.).min(row * 0.4);
                fitted(
                    &head,
                    x,
                    y + 20.,
                    row - amount_w - 16.,
                    14.,
                    if done { MINT } else { GOLD },
                );
                right(&amount, x + row, y + 20., amount_w, 12., ui::INK);
                let brief = if goal == Goal::DominoRun && fun.scenario != Scenario::Dominoes {
                    format!(
                        "{} PRESS {} (DOMINO) TO SET IT UP.",
                        goal.brief(),
                        hint(f.input, "domino")
                    )
                } else if done {
                    "KNOCK THEM DOWN AGAIN: RESET REBUILDS THE RUN.".to_owned()
                } else {
                    goal.brief().to_owned()
                };
                fitted(&brief.to_ascii_uppercase(), x, y + 36., width, 11., ui::INK);
                // Progress underlines the whole goal block.
                bar(
                    x,
                    y + 41.,
                    width,
                    2.,
                    value / target,
                    if done { MINT } else { GOLD },
                );
            }
            None => {
                fitted(
                    &format!("ALL GOALS DONE · {}", rank.title),
                    x,
                    y + 20.,
                    row,
                    14.,
                    MINT,
                );
                fitted(&f.bests.next_hook(fun), x, y + 36., width, 11., ui::INK);
            }
        }
    }

    fn world_text(&self, f: &Frame) {
        // Dashed target line for height goals, labelled at the right wall.
        let fun = &f.lab.fun;
        let line = match fun.current_goal() {
            Some(Goal::Liftoff) => Some((LIFTOFF_HEIGHT, "LIFTOFF")),
            Some(Goal::Skyscraper) => Some((TOWER_HEIGHT, "SKYSCRAPER")),
            _ => None,
        };
        if let Some((height, label)) = line
            && fun.scenario == Scenario::Workshop
        {
            let a = f.view.to_screen(vec2(-12., height));
            let b = f.view.to_screen(vec2(12., height));
            let mut x = a.x;
            while x < b.x {
                draw_line(x, a.y, (x + 10.).min(b.x), a.y, 2., faded(GOLD, 0.55));
                x += 18.;
            }
            right(
                &format!("{label} {height:.1} M"),
                b.x - 4.,
                a.y - 6.,
                200.,
                11.,
                faded(GOLD, 0.85),
            );
        }
        for pop in f.juice.pops.iter().flatten() {
            let t = pop.age / pop.life;
            let alpha = if t > 0.7 { (1. - t) / 0.3 } else { 1. };
            let at = f.view.to_screen(pop.at) - vec2(0., f.juice.pop_rise(pop));
            let w = screen_width();
            let size = (pop.size * (w / 900.).clamp(0.7, 1.)).min(pop.size);
            let width = width_of(&pop.text, size).min(w - 24.);
            let size = size.min(size * (w - 24.) / width_of(&pop.text, size).max(1.));
            let at = vec2(at.x.clamp(12. + width * 0.5, w - 12. - width * 0.5), at.y);
            let height = size * 7. / 8.;
            // Pops never rise into the top bar or the menu corner.
            let at = vec2(at.x, at.y.max(MENU_CORNER.y + height + 9.));
            draw_rectangle(
                at.x - width * 0.5 - 6.,
                at.y - height - 5.,
                width + 12.,
                height + 10.,
                faded(Color::from_rgba(10, 16, 26, 255), alpha * 0.82),
            );
            draw_text(
                &pop.text,
                at.x - width * 0.5,
                at.y,
                size,
                faded(pop.color, alpha),
            );
        }
    }

    fn toast(&self, f: &Frame) {
        let w = screen_width();
        let band = Self::band_bottom(f);
        let void = f.layout.scene_top() - band;
        // Narrow portrait screens leave sky above the lab: use it.
        let mut top = if void >= 110. {
            band + void * 0.5 - 40.
        } else {
            (f.layout.scene_top() + 10.).max(band + 10.)
        };
        if self.stats && w < 780. {
            // The stats panel spans narrow screens; callouts go below it.
            top = top.max(98. + 20. + 6. * 18. + 8.);
        }
        let (head, sub, color, alpha) = match &f.juice.toast {
            Some(toast) => (
                toast.head.as_str(),
                toast.sub.clone(),
                toast.color,
                toast.alpha(),
            ),
            None if f.lab.session.is_paused() && f.active => (
                "PAUSED",
                format!(
                    "{} · {} STEPS · {} INSPECTS",
                    quips::pause(f.lab.session.tick() as u32),
                    hint(f.input, "step"),
                    hint(f.input, "inspect")
                ),
                ui::AMBER,
                1.,
            ),
            None => return,
        };
        // With engine stats open, center in the space left of that panel.
        let room = if self.stats && w >= 780. { w - 346. } else { w };
        let w = room;
        let max = toast_max(w, top);
        // Wrap long punchlines at a readable size instead of shrinking them.
        // `label_lines` measures 16 px glyphs; scale the width for 12 px text.
        let lines: Vec<&str> = label_lines(&sub, max * 16. / 12.).take(2).collect();
        let widest = lines.iter().map(|l| width_of(l, 12.)).fold(0., f32::max);
        let width = width_of(head, 24.).max(widest).min(max) + 36.;
        let height = 42. + lines.len() as f32 * 17.;
        let rect = Rect::new(w * 0.5 - width * 0.5, top, width, height);
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, faded(ui::PANEL, alpha));
        draw_rectangle(rect.x, rect.y, rect.w, 3., faded(color, alpha));
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1., faded(ui::LINE, alpha));
        centered(head, w * 0.5, top + 30., max, 24., faded(color, alpha));
        for (row, line) in lines.iter().enumerate() {
            centered(
                line,
                w * 0.5,
                top + 49. + row as f32 * 17.,
                max,
                12.,
                faded(ui::INK, alpha),
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

    /// On narrow portrait screens the sky above the lab holds a how-to card.
    fn sky_card(&self, f: &Frame) {
        let band = Self::band_bottom(f);
        let void = f.layout.scene_top() - band;
        let w = screen_width();
        if void < 170. || self.hint <= 0.01 || f.juice.toast.is_some() {
            return;
        }
        let alpha = self.hint;
        let top = band + void * 0.5 - 52.;
        centered("HOW TO LAB", w * 0.5, top, w - 40., 16., faded(GOLD, alpha));
        let lines = [
            "TAP OR CLICK THE LAB TO DROP".to_owned(),
            format!(
                "{} PULSE · {} CHAOS",
                hint(f.input, "pulse"),
                hint(f.input, "chaos")
            ),
            format!(
                "{} DOMINO RUN · {} STATS",
                hint(f.input, "domino"),
                hint(f.input, "stats")
            ),
            "PAUSE, THEN STEP ONE TICK".to_owned(),
        ];
        for (row, line) in lines.iter().enumerate() {
            centered(
                line,
                w * 0.5,
                top + 30. + row as f32 * 22.,
                w - 40.,
                13.,
                faded(ui::INK, alpha),
            );
        }
    }

    fn onboarding_text(f: &Frame) -> String {
        if screen_width() < 560. {
            // Short form keeps a readable size in a narrow footer.
            return format!(
                "CLICK DROPS · {} PULSE · {} CHAOS · {} DOMINO",
                hint(f.input, "pulse"),
                hint(f.input, "chaos"),
                hint(f.input, "domino"),
            );
        }
        format!(
            "CLICK: DROP · RIGHT-CLICK: CRATE · {}: PULSE · {}: CHAOS · {}: DOMINOES · {}: STATS",
            hint(f.input, "pulse"),
            hint(f.input, "chaos"),
            hint(f.input, "domino"),
            hint(f.input, "stats"),
        )
    }

    fn footer(&self, f: &Frame) {
        let w = screen_width();
        let lab = f.lab;
        let y = f.layout.status_y();
        let wide = f.layout.single_row();
        let (state, color) = if !f.active {
            ("SUSPENDED", ui::MUTED)
        } else if lab.session.is_paused() {
            ("PAUSED", ui::AMBER)
        } else {
            ("RUNNING", MINT)
        };
        let used = fitted(state, 28., y, 120., 16., color);
        let shape = if lab.ball { "BALL" } else { "CRATE" };
        let summary = format!(
            " · {} / {} BODIES · {} READY",
            lab.sandbox.bodies.len(),
            MAX_BODIES,
            shape
        );
        let summary_w = fitted(
            &summary,
            28. + used,
            y,
            if wide { w * 0.45 } else { w - 56. - used },
            16.,
            ui::INK,
        );
        // Tooltip for the hovered tool, else a fresh notice, else a tip.
        let tip = if let Some(index) = f.hovered {
            let (label, action, what) = TOOLS[index];
            Some((
                format!("{label} [{}] · {what}", hint(f.input, action)),
                GOLD,
            ))
        } else if lab.feedback_time > 0. {
            Some((lab.feedback.to_ascii_uppercase(), ui::CYAN))
        } else {
            None
        };
        let tip_line = |text: &str, color: Color, x: f32, y: f32, width: f32| {
            fitted(text, x, y, width, 13., color);
        };
        let selected = lab
            .selected
            .map(|_| lab.selected_label().to_ascii_uppercase());
        if wide {
            let x = 28. + used + summary_w + 24.;
            if let Some((text, color)) = &tip {
                tip_line(text, *color, x, y, w - 28. - x);
            }
            self.second_line(f, selected, 28., y + 20., w - 56.);
        } else {
            if let Some((text, color)) = &tip {
                tip_line(text, *color, 28., y + 20., w - 56.);
            }
            self.second_line(f, selected, 28., y + 40., w - 56.);
        }
    }

    /// Selected-body diagnostics win; otherwise onboarding (fading once
    /// learned) cross-fades into a quieter standing tip.
    fn second_line(&self, f: &Frame, selected: Option<String>, x: f32, y: f32, width: f32) {
        if let Some(selected) = selected {
            fitted(&selected, x, y, width, 13., ui::MUTED);
            return;
        }
        let sky = f.layout.scene_top() - Self::band_bottom(f) >= 170.;
        let onboarding = if sky { 0. } else { self.hint };
        if onboarding > 0.01 {
            fitted(
                &Self::onboarding_text(f),
                x,
                y,
                width,
                13.,
                faded(ui::INK, onboarding),
            );
        }
        if onboarding < 0.99 {
            let tip = format!(
                "TIP: {} INSPECTS THE BODY UNDER THE CURSOR · {} SHOWS ENGINE STATS",
                hint(f.input, "inspect"),
                hint(f.input, "stats")
            );
            fitted(&tip, x, y, width, 13., faded(ui::MUTED, 1. - onboarding));
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
            Some(q) if q.report.is_truncated() => format!(
                "QUERY {}/{} HITS AT CURSOR (TRUNCATED)",
                q.report.written, q.report.total
            ),
            Some(q) => format!("QUERY {} HITS AT CURSOR", q.report.total),
            None => "QUERY: CURSOR OUTSIDE THE SCENE".into(),
        };
        let pulse = if lab.sandbox.pulse_time > 0. {
            format!("PULSE RECHARGING {:.2} S", lab.sandbox.pulse_time)
        } else {
            "PULSE READY".into()
        };
        let lines = [
            format!(
                "TICK {} · {:.0} HZ · {:.2} MS",
                lab.session.tick(),
                TICK_HZ,
                1000. / TICK_HZ
            ),
            format!(
                "AWAKE {} · ASLEEP {} · {}/{} BODIES",
                sleep.awake_bodies,
                sleep.sleeping_bodies,
                lab.sandbox.bodies.len(),
                MAX_BODIES
            ),
            format!(
                "LAST TICK: {} CONTACTS · +{started} -{stopped}",
                physics.contacts().count()
            ),
            query,
            pulse,
            format!(
                "BONKS {}/S · PEAK {} · GOAL {DEMOLITION_BONKS}",
                lab.fun.bonks_now, lab.fun.bonk_peak
            ),
        ];
        let rect = stats_rect(w, lines.len());
        let width = rect.w;
        ui::panel(rect);
        fitted(
            "ENGINE STATS",
            rect.x + 10.,
            rect.y + 16.,
            width - 20.,
            11.,
            ui::AMBER,
        );
        for (row, line) in lines.iter().enumerate() {
            fitted(
                line,
                rect.x + 10.,
                rect.y + 34. + row as f32 * 18.,
                width - 20.,
                12.,
                ui::INK,
            );
        }
    }

    fn certificate(&self, f: &Frame, age: f32) {
        let w = screen_width();
        let h = screen_height();
        let fun = &f.lab.fun;
        let alpha = (age / 0.3).min(1.) * ((CARD_LIFE - age) / 0.5).clamp(0., 1.);
        draw_rectangle(0., 0., w, h, faded(Color::new(0., 0., 0., 0.45), alpha));
        let rect = card_rect(w, h, 318.);
        let (width, height) = (rect.w, rect.h);
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, faded(ui::PANEL, alpha));
        draw_rectangle(rect.x, rect.y, rect.w, 4., faded(GOLD, alpha));
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1., faded(GOLD, alpha));
        let cx = w * 0.5;
        let inner = width - 40.;
        let rank = quips::rank(fun.goals_done(), fun.secret);
        centered(
            "LAB CERTIFIED",
            cx,
            rect.y + 34.,
            inner,
            14.,
            faded(ui::AMBER, alpha),
        );
        centered(rank.title, cx, rect.y + 66., inner, 26., faded(GOLD, alpha));
        centered(
            rank.quip,
            cx,
            rect.y + 90.,
            inner,
            11.,
            faded(ui::INK, alpha),
        );
        let rows = [
            (
                "TALLEST TOWER",
                format!("{:.1} M", fun.tower),
                best_cell(fun.tower, f.bests.tower, false, |b| format!("{b:.1} M")),
            ),
            (
                "HIGHEST FLIGHT",
                format!("{:.1} M", fun.altitude),
                best_cell(fun.altitude, f.bests.altitude, false, |b| {
                    format!("{b:.1} M")
                }),
            ),
            (
                "BONKS PER SECOND",
                fun.bonk_peak.to_string(),
                best_cell(
                    fun.bonk_peak as f32,
                    f.bests.bonks.map(|b| b as f32),
                    false,
                    |b| format!("{b:.0}"),
                ),
            ),
            (
                "DOMINO RUN",
                fun.domino_seconds()
                    .map_or("—".into(), |s| format!("{s:.2} S")),
                best_cell(
                    fun.domino_seconds().unwrap_or(f32::INFINITY),
                    f.bests.domino,
                    true,
                    |b| format!("{b:.2} S"),
                ),
            ),
        ];
        let x0 = rect.x + 24.;
        let x1 = rect.x + width * 0.58;
        let x2 = rect.x + width - 24.;
        fitted(
            "THIS SESSION",
            x1 - 40.,
            rect.y + 122.,
            120.,
            10.,
            faded(ui::MUTED, alpha),
        );
        right("BEST", x2, rect.y + 122., 80., 10., faded(ui::MUTED, alpha));
        for (row, (label, value, best)) in rows.iter().enumerate() {
            let y = rect.y + 144. + row as f32 * 24.;
            fitted(label, x0, y, x1 - x0 - 50., 13., faded(ui::INK, alpha));
            fitted(value, x1 - 40., y, 110., 13., faded(MINT, alpha));
            right(&best.0, x2, y, 130., 13., faded(best.1, alpha));
        }
        let s = fun.stats;
        centered(
            &format!(
                "DROPS {} · PULSES {} · CHAOS {} · COLLAPSES {}",
                s.drops, s.pulses, s.chaos, s.collapses
            ),
            cx,
            rect.y + height - 50.,
            inner,
            12.,
            faded(ui::INK, alpha),
        );
        centered(
            &f.bests.next_hook(fun),
            cx,
            rect.y + height - 28.,
            inner,
            12.,
            faded(PINK, alpha),
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
            // First row glyphs span roughly y + 8 ..= y + 20.
            assert!(clear(x + row, y + 8.), "{w}: goal row");
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
                let half = (toast_max(w, top) + 36.) * 0.5;
                assert!(clear(w * 0.5 + half, top), "{w}: toast at {top}");
                assert!(w * 0.5 - half >= 0.);
            }
            for height in [300., 318.] {
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
        assert!(bests.next_hook(&fun).starts_with("SECRET"));
        fun.secret = true;
        assert!(bests.next_hook(&fun).contains("8.2 M"));
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
        let mut input = ferravex_render::input::PlayerInput::transient("physics-2d-tools");
        crate::register_lab_actions(&mut input);
        for (label, action, what) in TOOLS {
            assert!(label.len() <= 6);
            assert!(what.len() <= 40, "{what}");
            let mut buffer = [0; 96];
            let _ = input.format_hint(action, &mut buffer);
        }
        for action in ["chaos", "domino", "stats"] {
            let mut buffer = [0; 96];
            assert_ne!(input.format_hint(action, &mut buffer).unwrap(), "Unbound");
        }
    }
}
