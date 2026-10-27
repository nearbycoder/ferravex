//! The yard's screen furniture: top bar with the current goal, toasts and
//! world-anchored pops, the pause chip, footer status/tooltips, the optional
//! engine diagnostics panel (G) and the YARD CERTIFIED card. Owns personal
//! bests. Drawing reads lab state and never changes it; storage failure leaves
//! a fully playable lab, and autoplay never writes a best.
use super::juice::{GOLD, Juice, MINT, PINK};
use super::quips;
use super::toys::{Fun, Goal, STACK};
use super::{Inspector, Lab, MAX_BODIES};
use ferravex_render::input::PlayerInput;
use ferravex_render::prelude::*;
use ferravex_render::storage::Records;
use ferravex_render::ui;

const RECORD_RULES: u64 = 0x5048_3344_0001;
const CARD_LIFE: f32 = 12.;

/// Tooltip copy per stable button index: (label, action, what it does).
pub(super) const TOOLS: [(&str, &str, &str); 10] = [
    ("CRATE", "select_one", "DROP A CRATE. REPEAT WITH DROP"),
    ("BALL", "select_two", "DROP A BALL. REPEAT WITH DROP"),
    ("DROP", "drop", "DROP THE SELECTED SHAPE AGAIN"),
    ("PULSE", "pulse", "SHOCKWAVE FROM THE YARD CENTER"),
    ("PAUSE", "pause", "FREEZE TIME. STEP AND ORBIT"),
    ("STEP", "step", "ONE 1/120 S TICK WHILE PAUSED"),
    ("VIEW", "reset_view", "RESTORE THE DEFAULT CAMERA"),
    ("RESET", "restart", "REBUILD THE YARD AND THE STACK"),
    ("BOWL", "bowl", "ROLL A HEAVY BALL FROM YOUR SIDE"),
    ("CHAOS", "chaos", "SIXTEEN SHAPES. NO SIGNATURE"),
];

pub(super) fn hint(input: &PlayerInput, action: &str) -> String {
    let mut buffer = [0; 96];
    let text = input.format_hint(action, &mut buffer).unwrap_or("CONTROLS");
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
    ferravex_render::text::text_width(text, ferravex_render::text::TextStyle::modern(size, WHITE))
}

pub(super) fn fitted(text: &str, x: f32, y: f32, max_width: f32, size: f32, color: Color) -> f32 {
    let measured = width_of(text, size).max(1.);
    let scale = (max_width.max(1.) / measured).min(1.);
    ferravex_render::text::draw_text_styled(
        text,
        x,
        y,
        ferravex_render::text::TextStyle::modern(size * scale, color),
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

fn bar(x: f32, y: f32, w: f32, h: f32, ratio: f32, color: Color) {
    draw_rectangle(x, y, w, h, ui::LINE);
    draw_rectangle(x, y, w * ratio.clamp(0., 1.), h, color);
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

    /// Store improvements. A headline is returned only when a stored best
    /// existed and was beaten, so a first visit never claims a record.
    pub fn update(&mut self, fun: &Fun) -> Option<String> {
        if !self.enabled {
            return None;
        }
        let mut headline = None;
        let store = |records: &mut Option<Records>, id: &str, score: u64| {
            if let Some(records) = records.as_mut() {
                let _ = records.attempt(id, RECORD_RULES, score, None);
            }
        };
        if fun.airtime >= 5. && self.airtime.is_none_or(|best| fun.airtime > best + 0.25) {
            if self.airtime.is_some() {
                headline = Some(format!("NEW BEST AIRTIME · {:.1} M", fun.airtime));
            }
            self.airtime = Some(fun.airtime);
            store(&mut self.records, "airtime", (fun.airtime * 100.) as u64);
        }
        if fun.bonk_peak >= 20 && self.bonks.is_none_or(|best| fun.bonk_peak > best) {
            if self.bonks.is_some() {
                headline = Some(format!("NEW BEST · {} BONKS/S", fun.bonk_peak));
            }
            self.bonks = Some(fun.bonk_peak);
            store(&mut self.records, "bonks", u64::from(fun.bonk_peak));
        }
        if let Some(bowls) = fun.sweep_best
            && self.sweep.is_none_or(|best| bowls < best)
        {
            if self.sweep.is_some() {
                headline = Some(format!("NEW BEST SWEEP · {bowls} BOWLS"));
            }
            self.sweep = Some(bowls);
            store(&mut self.records, "sweep", 1000 - u64::from(bowls.min(999)));
        }
        headline
    }

    pub fn next_hook(&self, fun: &Fun) -> String {
        if !fun.secret {
            return "SECRET: ONE OF THE CORNER PEDESTALS LOOKS LONELY...".into();
        }
        match self.sweep {
            Some(1) => "NEXT: A ONE-BOWL SWEEP. AGAIN. FOR SCIENCE.".into(),
            Some(best) => format!("NEXT: CLEAN SWEEP IN FEWER THAN {best} BOWLS"),
            None => "NEXT: CLEAN SWEEP WITH AS FEW BOWLS AS YOU DARE".into(),
        }
    }
}

#[derive(Default)]
pub(super) struct Hud {
    pub stats: bool,
    pub card: Option<f32>,
    hint: f32,
    active: f32,
}

pub(super) struct Frame<'a> {
    pub lab: &'a Lab,
    pub juice: &'a Juice,
    pub bests: &'a Bests,
    pub input: &'a PlayerInput,
    pub hovered: Option<usize>,
    pub panel: Rect,
    pub view_proj: Mat4,
    pub inspector: &'a Inspector,
    pub active: bool,
}

impl Hud {
    /// Onboarding stays until the player has bowled and pulsed, or 30 seconds
    /// of active play pass; then it fades out.
    pub fn learned(fun: &Fun, active_seconds: f32) -> bool {
        (fun.stats.bowls > 0 && fun.stats.pulses > 0) || active_seconds > 30.
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
        if !lab.fun.events.goals.is_empty() && lab.fun.goals_done() == 4 {
            self.card = Some(0.);
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

    fn top_bar(&self, f: &Frame) {
        let w = screen_width();
        let fun = &f.lab.fun;
        let wide = w >= 780.;
        draw_rectangle(0., 0., w, 44., Color::from_rgba(12, 19, 30, 235));
        draw_line(0., 44., w, 44., 1., ui::LINE);
        fitted("3D PHYSICS LAB", 16., 21., 220., 16., ui::INK);
        let rank = quips::rank(fun.goals_done(), fun.secret);
        let used = fitted("LOADING YARD", 16., 37., 120., 11., ui::AMBER);
        fitted(
            &format!("· {}", rank.title),
            22. + used,
            37.,
            rank_width(w, used),
            11.,
            ui::MUTED,
        );
        // Wide: goal block right of the title. Narrow: a translucent strip
        // across the top of the scene.
        if !wide {
            draw_rectangle(0., 44., w, 46., Color::from_rgba(12, 19, 30, 190));
        }
        let (x, y, row, width) = goal_block(w, wide, 48.);
        match fun.current_goal() {
            Some(goal) => {
                let (value, target) = fun.progress(goal);
                let head = format!("GOAL {}/4 · {}", goal.index() + 1, goal.title());
                let amount = match goal {
                    Goal::Airtime => format!("{value:.1} / {target:.1} M"),
                    Goal::Avalanche => format!("{value:.0} / {target:.0} PER S"),
                    _ => format!("{value:.0} / {target:.0} CRATES"),
                };
                let amount_w = width_of(&amount, 12.).min(row * 0.4);
                fitted(&head, x, y + 20., row - amount_w - 16., 14., GOLD);
                right(&amount, x + row, y + 20., amount_w, 12., ui::INK);
                let brief = match goal {
                    Goal::Strike | Goal::CleanSweep => format!(
                        "{} BOWL: {}",
                        goal.brief().to_ascii_uppercase(),
                        hint(f.input, "bowl")
                    ),
                    Goal::Airtime => format!(
                        "{} PULSE: {}",
                        goal.brief().to_ascii_uppercase(),
                        hint(f.input, "pulse")
                    ),
                    Goal::Avalanche => format!(
                        "{} CHAOS: {}",
                        goal.brief().to_ascii_uppercase(),
                        hint(f.input, "chaos")
                    ),
                };
                fitted(&brief, x, y + 36., width, 11., ui::INK);
                bar(x, y + 41., width, 2., value / target, GOLD);
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

    fn project(f: &Frame, point: Vec3) -> Option<Vec2> {
        let clip = f.view_proj * point.extend(1.);
        if clip.w <= 0.01 {
            return None;
        }
        Some(vec2(
            (clip.x / clip.w + 1.) * screen_width() * 0.5,
            (1. - clip.y / clip.w) * screen_height() * 0.5,
        ))
    }

    fn pops(&self, f: &Frame) {
        let w = screen_width();
        for pop in f.juice.pops.iter().flatten() {
            let Some(at) = Self::project(f, pop.at) else {
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
            draw_rectangle(
                at.x - width * 0.5 - 6.,
                at.y - height - 5.,
                width + 12.,
                height + 10.,
                faded(Color::from_rgba(10, 16, 26, 255), alpha * 0.82),
            );
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
            top = 98. + 20. + 6. * 18. + 8.;
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
                    "{} · {} STEPS · DRAG STILL ORBITS",
                    quips::pause(f.lab.session.tick() as u32),
                    hint(f.input, "step")
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
        let lines: Vec<&str> = super::hud_lines(&sub, max, 12).take(2).collect();
        let widest = lines.iter().map(|l| width_of(l, 12.)).fold(0., f32::max);
        let width = width_of(head, 24.).max(widest).min(max) + 36.;
        let height = 42. + lines.len() as f32 * 17.;
        let rect = Rect::new(w * 0.5 - width * 0.5, top, width, height);
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, faded(ui::PANEL, alpha));
        draw_rectangle(rect.x, rect.y, rect.w, 3., faded(color, alpha));
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1., faded(ui::LINE, alpha));
        centered(head, w * 0.5, top + 30., max, 24., faded(color, alpha));
        for (row, line) in lines.iter().enumerate() {
            let y = top + 49. + row as f32 * 17.;
            centered(line, w * 0.5, y, max, 12., faded(ui::INK, alpha));
        }
    }

    fn footer(&self, f: &Frame) {
        let lab = f.lab;
        let panel = f.panel;
        let x = panel.x + 12.;
        let width = panel.w - 24.;
        let wide = screen_width() >= 540.;
        let size = if wide { 16. } else { 14. };
        let (state, color) = if !f.active {
            ("SUSPENDED", ui::MUTED)
        } else if lab.session.is_paused() {
            ("PAUSED", ui::AMBER)
        } else {
            ("PLAYING", MINT)
        };
        let used = fitted(state, x, panel.y + 21., 120., size, color);
        let shape = if lab.shape == super::DropShape::Ball {
            "BALL"
        } else {
            "CRATE"
        };
        let summary = format!(
            " · {} / {MAX_BODIES} BODIES · {shape} READY",
            lab.sandbox.bodies.len()
        );
        let summary_w = fitted(
            &summary,
            x + used,
            panel.y + 21.,
            if screen_width() >= 1000. {
                width * 0.45
            } else {
                width - used
            },
            size,
            ui::INK,
        );
        let tip = if let Some(index) = f.hovered {
            let (label, action, what) = TOOLS[index];
            Some((
                format!("{label} [{}] · {what}", hint(f.input, action)),
                GOLD,
            ))
        } else if lab.feedback_time > 0. && !lab.feedback.is_empty() {
            Some((lab.feedback.to_ascii_uppercase(), ui::CYAN))
        } else {
            None
        };
        // Wide: tooltip beside the status; the second line teaches or tips.
        let second = if screen_width() >= 1000. {
            if let Some((text, color)) = &tip {
                let tx = x + used + summary_w + 24.;
                fitted(
                    text,
                    tx,
                    panel.y + 21.,
                    panel.x + panel.w - 12. - tx,
                    13.,
                    *color,
                );
            }
            None
        } else {
            tip.clone()
        };
        let line = panel.y + 40.;
        if let Some((text, color)) = second {
            fitted(&text, x, line, width, 13., color);
        } else {
            let onboarding = if screen_width() < 560. {
                format!(
                    "DRAG ORBITS · {} BOWL · {} CHAOS · {} PULSE",
                    hint(f.input, "bowl"),
                    hint(f.input, "chaos"),
                    hint(f.input, "pulse")
                )
            } else {
                format!(
                    "DRAG: ORBIT · WHEEL: ZOOM · {}: BOWL · {}: CHAOS · {}: PULSE · {}: STATS",
                    hint(f.input, "bowl"),
                    hint(f.input, "chaos"),
                    hint(f.input, "pulse"),
                    hint(f.input, "stats")
                )
            };
            if self.hint > 0.01 {
                fitted(&onboarding, x, line, width, 13., faded(ui::INK, self.hint));
            }
            if self.hint < 0.99 {
                let tip = format!(
                    "TIP: {} RESTORES THE CAMERA · {} SHOWS ENGINE STATS",
                    hint(f.input, "reset_view"),
                    hint(f.input, "stats")
                );
                fitted(&tip, x, line, width, 13., faded(ui::MUTED, 1. - self.hint));
            }
        }
    }

    fn diagnostics(&self, f: &Frame) {
        let w = screen_width();
        let lab = f.lab;
        let physics = &lab.sandbox.physics;
        let sleep = physics.sleep_stats();
        let query = &f.inspector.query;
        let near = f.inspector.hits[0].map(|hit| hit.distance);
        let lines = [
            format!("TICK {} · 120 HZ · 8.33 MS", lab.session.tick()),
            format!(
                "AWAKE {} · ASLEEP {} · {}/{MAX_BODIES} BODIES",
                sleep.awake_bodies,
                sleep.sleeping_bodies,
                lab.sandbox.bodies.len()
            ),
            format!(
                "CONTACTS {} · EVENTS {}",
                physics.contacts().count(),
                physics.contact_events().len()
            ),
            format!(
                "RAY {}/{}{} · NEAR {}",
                query.written,
                query.total,
                if query.is_truncated() { " CAPPED" } else { "" },
                near.map_or("—".into(), |d| format!("{d:.1} M"))
            ),
            if lab.sandbox.pulse_time > 0. {
                format!("PULSE RECHARGING {:.2} S", lab.sandbox.pulse_time)
            } else {
                "PULSE READY".into()
            },
            format!(
                "BONKS {}/S · PEAK {} · STACK {}/{STACK} DOWN",
                lab.fun.bonks_now, lab.fun.bonk_peak, lab.fun.toppled
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
        let rect = card_rect(w, h, 300.);
        let (width, height) = (rect.w, rect.h);
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, faded(ui::PANEL, alpha));
        draw_rectangle(rect.x, rect.y, rect.w, 4., faded(GOLD, alpha));
        draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1., faded(GOLD, alpha));
        let cx = w * 0.5;
        let inner = width - 40.;
        let rank = quips::rank(fun.goals_done(), fun.secret);
        centered(
            "YARD CERTIFIED",
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
        let best = |stored: Option<String>| stored.unwrap_or_else(|| "—".into());
        let rows = [
            (
                "BOWLS FOR A CLEAN SWEEP",
                fun.sweep_best.map_or("—".into(), |b| b.to_string()),
                best(f.bests.sweep.map(|b| b.to_string())),
            ),
            (
                "HIGHEST AIRTIME",
                format!("{:.1} M", fun.airtime),
                best(f.bests.airtime.map(|b| format!("{b:.1} M"))),
            ),
            (
                "BONKS PER SECOND",
                fun.bonk_peak.to_string(),
                best(f.bests.bonks.map(|b| b.to_string())),
            ),
        ];
        let x0 = rect.x + 24.;
        let x1 = rect.x + width * 0.6;
        let x2 = rect.x + width - 24.;
        fitted(
            "SESSION",
            x1 - 30.,
            rect.y + 122.,
            100.,
            10.,
            faded(ui::MUTED, alpha),
        );
        right("BEST", x2, rect.y + 122., 80., 10., faded(ui::MUTED, alpha));
        for (row, (label, value, stored)) in rows.iter().enumerate() {
            let y = rect.y + 144. + row as f32 * 24.;
            fitted(label, x0, y, x1 - x0 - 40., 13., faded(ui::INK, alpha));
            fitted(value, x1 - 30., y, 100., 13., faded(MINT, alpha));
            right(stored, x2, y, 100., 13., faded(ui::AMBER, alpha));
        }
        let s = fun.stats;
        centered(
            &format!(
                "BOWLS {} · STRIKES {} · PULSES {} · CHAOS {} · COLLAPSES {}",
                s.bowls, s.strikes, s.pulses, s.chaos, s.collapses
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

/// Rounded tool keys retain the lab's click-on-press input and live selection.
pub(super) fn tool_button(rect: Rect, text: &str, options: ui::ButtonOptions) -> bool {
    use ferravex_render::{
        shapes,
        text::{TextAlign, TextStyle, draw_text_styled, text_width},
    };
    let hovered = options.enabled && rect.contains(Vec2::from(mouse_position()));
    let fill = if !options.enabled {
        Color::new(0.10, 0.14, 0.19, 1.)
    } else if options.selected {
        ui::AMBER
    } else if hovered {
        Color::new(0.20, 0.27, 0.34, 1.)
    } else {
        Color::new(0.12, 0.17, 0.23, 1.)
    };
    shapes::fill_rounded_rect(rect, 7., fill);
    shapes::stroke_rounded_rect(
        rect,
        7.,
        1.,
        if hovered || options.selected {
            ui::AMBER
        } else {
            ui::LINE
        },
    );
    let ink = if !options.enabled {
        ui::MUTED
    } else if options.selected {
        ui::BACKGROUND
    } else {
        ui::INK
    };
    let style = TextStyle::modern(16., ink);
    let size = 16. * ((rect.w - 10.) / text_width(text, style).max(1.)).min(1.);
    let style = style.with_size(size).with_align(TextAlign::Center);
    draw_text_styled(
        text,
        rect.x + rect.w * 0.5,
        style.centered_baseline(rect.y + rect.h * 0.5),
        style,
    );
    hovered && is_mouse_button_pressed(MouseButton::Left)
}
pub(super) fn tool(rect: Rect, text: &str, selected: bool) -> bool {
    tool_button(
        rect,
        text,
        ui::ButtonOptions {
            selected,
            enabled: true,
        },
    )
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
        assert!(first.update(&fun).unwrap().contains("1 BOWLS"));
        assert!(first.update(&fun).is_none(), "no repeat headline");
    }

    #[test]
    fn hooks_tease_the_secret_then_the_sweep() {
        let mut fun = Fun::default();
        let bests = Bests {
            sweep: Some(3),
            ..Default::default()
        };
        assert!(bests.next_hook(&fun).starts_with("SECRET"));
        fun.secret = true;
        assert!(bests.next_hook(&fun).contains("FEWER THAN 3"));
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
}
