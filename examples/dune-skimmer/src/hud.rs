//! Race HUD, countdown, pause and results screens, drawn in logical pixels.
//!
//! The play HUD keeps one job per corner: lap (top left), clock and pace
//! (top center), the next gate with this lap's threading pips (top right),
//! speed (bottom left) and boost (bottom right). Every block sits on a dark
//! plate so it stays readable against bright sand and sky. One radio line,
//! one banner and one alert slot keep messages from stacking.
//!
//! Browser contract (web/tests/wasm-ui.mjs): narrow layouts keep the white
//! 14px lap and 20px timer in the top 32px, and wide layouts keep the 14px
//! teal next-gate label above y=52; both stay clear of the host's menu corner.
use super::juice::{GOLD, Juice, MINT, ROSE, signed};
use super::presentation::{binding_hint, fitted_size, text_width};
use super::quips::{self, RunSummary};
use super::race::{BoostStatus, GATES, LAPS, RECOVERY_PENALTY, Race, Traction};
use super::{DARK, ORANGE, PALE, SHORT_START_ROW, TEAL, centered, centered_in, label, time_label};
use ferravex_play::gameplay::trial::Medal;
use ferravex_render::input::PlayerInput;
use ferravex_render::prelude::*;

pub(super) const PANEL: Color = Color::new(0.06, 0.10, 0.10, 0.74);
const CARD: Color = Color::new(0.06, 0.10, 0.10, 0.95);
const SILVER: Color = Color::new(0.82, 0.86, 0.88, 1.0);
const BRONZE: Color = Color::new(0.86, 0.56, 0.33, 1.0);
const MUTED: Color = Color::new(0.62, 0.62, 0.55, 1.0);

/// Result of a finished race against stored records, captured once at the finish.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Results {
    /// Fastest stored race before this one, in seconds.
    pub previous_best: Option<f32>,
    /// Fastest stored lap before this race, in seconds.
    pub previous_lap: Option<f32>,
    /// Autoplay demonstrations are shown but never recorded.
    pub demo: bool,
}

impl Results {
    pub fn new_best(&self, time: f32) -> bool {
        !self.demo && self.previous_best.is_none_or(|best| time < best - 1e-4)
    }
    pub fn new_lap(&self, lap: Option<f32>) -> bool {
        !self.demo && lap.is_some_and(|lap| self.previous_lap.is_none_or(|best| lap < best - 1e-4))
    }
}

/// Everything the HUD reads for one frame. Nothing here writes race state.
pub(super) struct Hud<'a> {
    pub race: &'a Race,
    pub input: &'a PlayerInput,
    pub juice: &'a Juice,
    pub results: &'a Results,
    pub waiting_for_controls: bool,
    /// Stored best race time, shown on the countdown card.
    pub best: Option<f32>,
}

pub(super) fn fade(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0., 1.))
}

pub(super) fn medal_color(medal: Medal) -> Color {
    match medal {
        Medal::Gold => GOLD,
        Medal::Silver => SILVER,
        Medal::Bronze => BRONZE,
        Medal::Unranked => MUTED,
    }
}

fn plate(x: f32, y: f32, w: f32, h: f32, accent: Color) {
    draw_rectangle(x, y, w, h, PANEL);
    draw_rectangle(x, y, w, 2., fade(accent, 0.85));
}

fn width(text: &str, size: f32) -> f32 {
    text_width(text, size.round().max(1.) as u16) * size / size.round().max(1.)
}

fn right_label(text: &str, right: f32, y: f32, size: f32, color: Color) {
    label(text, right - width(text, size), y, size, color);
}

/// Split text into one or two lines so it fits `available` without shrinking
/// past legibility (85% of `size`). Splits at the space nearest the middle.
fn split_lines(text: &str, available: f32, size: u16) -> (&str, Option<&str>) {
    if fitted_size(text, available, size) >= size as f32 * 0.85 {
        return (text, None);
    }
    let mid = text.len() / 2;
    let split = text[..mid]
        .rfind(' ')
        .into_iter()
        .chain(text[mid..].find(' ').map(|i| i + mid))
        .min_by_key(|&i| i.abs_diff(mid));
    match split {
        Some(at) => (&text[..at], Some(text[at + 1..].trim_start())),
        None => (text, None),
    }
}

/// The medal the running clock would still earn, projected from the last
/// completed lap over the remaining gates. Lap one has no honest pace yet.
pub(super) fn pace(race: &Race) -> Option<Medal> {
    if race.finished || race.laps == 0 {
        return None;
    }
    let last = race.lap_times[race.laps as usize - 1];
    let remaining = (GATES * LAPS as usize - race.gates_completed()) as f32;
    Some(quips::medal_for(
        race.elapsed + remaining / GATES as f32 * last,
    ))
}

pub(super) fn summary(race: &Race) -> RunSummary {
    RunSummary {
        medal: quips::medal_for(race.elapsed),
        threaded: race.threaded,
        gates: (GATES * LAPS as usize) as u32,
        best_streak: race.best_streak,
        perfect_laps: race.perfect_laps,
        scrapes: race.scrapes,
        impacts: race.impacts,
        recoveries: race.recovery_count,
        rocket: race.rocket_start,
    }
}

/// The next-goal hook on the results card.
pub(super) fn next_goal(race: &Race, results: &Results) -> String {
    let time = race.elapsed;
    let medal = quips::medal_for(time);
    let best = results
        .previous_best
        .filter(|_| !results.demo)
        .map_or(time, |b| b.min(time));
    if let Some((name, target)) = quips::next_medal(medal) {
        format!(
            "NEXT: {name} UNDER {}  ·  FIND {:.2}s",
            time_label(target),
            (time - target).max(0.01)
        )
    } else if race.perfect_laps == 0 {
        "NEXT: A MIRAGE LAP. THREAD ALL 14 IN ONE LAP.".to_owned()
    } else if race.threaded < (GATES * LAPS as usize) as u32 {
        "NEXT: THREAD ALL 42 GATES FOR SAND SURGEON".to_owned()
    } else {
        format!("NEXT: BEAT {}. THE SAND BELIEVES.", time_label(best - 0.25))
    }
}

/// Speed lines and edge flashes, drawn under the HUD.
pub(super) fn effects(race: &Race, juice: &Juice) {
    let w = screen_width();
    let h = screen_height();
    let lines = if race.finished {
        0.
    } else {
        juice.speed_lines(race.speed(), race.boosting)
    };
    if lines > 0. {
        let center = vec2(w * 0.5, h * 0.55);
        let reach = (w * w + h * h).sqrt() * 0.5;
        for i in 0..26 {
            let angle = i as f32 * 2.399_963 + 0.4;
            let phase = (juice.time * 1.7 + i as f32 * 0.383).fract();
            let dir = vec2(angle.cos(), angle.sin());
            let start = reach * (0.42 + phase * 0.5);
            let length = reach * (0.10 + 0.08 * lines);
            let a = center + dir * start;
            let b = center + dir * (start + length);
            let alpha = lines * 0.32 * (1. - phase) * phase * 4.;
            draw_line(a.x, a.y, b.x, b.y, 2., fade(PALE, alpha));
        }
    }
    if juice.flash > 0.01 {
        let edge = w.min(h) * 0.07;
        for step in 0..4 {
            let inset = edge * step as f32 / 4.;
            let alpha = juice.flash * 0.22 * (1. - step as f32 / 4.);
            let c = fade(juice.flash_color, alpha);
            draw_rectangle(inset, inset, w - inset * 2., edge / 4., c);
            draw_rectangle(inset, h - inset - edge / 4., w - inset * 2., edge / 4., c);
            draw_rectangle(
                inset,
                inset + edge / 4.,
                edge / 4.,
                h - inset * 2. - edge / 2.,
                c,
            );
            draw_rectangle(
                w - inset - edge / 4.,
                inset + edge / 4.,
                edge / 4.,
                h - inset * 2. - edge / 2.,
                c,
            );
        }
    }
}

/// The browser host's fullscreen menu button reserves the top-right corner:
/// nothing readable may sit at `x > width - MENU_CORNER.0` and `y < MENU_CORNER.1`.
pub(super) const MENU_CORNER: (f32, f32) = (72., 60.);
/// Right edge of the wide right-hand cluster, inset past the menu corner.
const WIDE_RIGHT: f32 = 76.;

fn gate_label(completed: usize) -> String {
    format!("NEXT GATE {:02} / {GATES}", completed % GATES + 1)
}

/// Mid-width wide layouts use the shorter label so all three plates fit.
fn wide_gate_label(w: f32, completed: usize) -> String {
    if w < 860. {
        format!("GATE {:02} / {GATES}", completed % GATES + 1)
    } else {
        gate_label(completed)
    }
}

fn streak_chip_text(streak: u32) -> String {
    format!(
        "STREAK {streak}  x{}",
        streak.min(super::race::MULTIPLIER_CAP)
    )
}

/// Wide next-gate plate: (x, y, w, h), right-aligned inside the inset.
fn wide_gate_plate(w: f32, next: &str) -> Rect {
    let pips_w = GATES as f32 * 12. - 3.;
    let gate_w = pips_w.max(text_width(next, 14)) + 28.;
    Rect::new(w - WIDE_RIGHT - gate_w, 12., gate_w, 60.)
}

/// Wide top plates (lap, clock, gate). Mid-width hosts drop the best-lap
/// column and slide the clock between its neighbours so nothing touches.
fn wide_plates(w: f32, sub_w: f32, next: &str) -> [Rect; 3] {
    let margin = (w * 0.035).clamp(18.0, 44.0);
    let gate = wide_gate_plate(w, next);
    let mid = w < 860.;
    let lap = Rect::new(margin - 12., 12., if mid { 76. } else { 196. }, 60.);
    let clock_w = (sub_w + 32.).max(if mid { 220. } else { 236. });
    let mut x = (w - clock_w) * 0.5;
    if x < lap.x + lap.w + 8. || x + clock_w > gate.x - 8. {
        x = (lap.x + lap.w + gate.x - clock_w) * 0.5;
    }
    [lap, Rect::new(x, 12., clock_w, 60.), gate]
}

/// Wide streak chip under the gate plate, right-aligned inside the inset.
fn wide_streak_chip(w: f32, chip: &str) -> Rect {
    let chip_w = width(chip, 14.) + 20.;
    Rect::new(w - WIDE_RIGHT - chip_w, 76., chip_w, 22.)
}

/// Very narrow screens shorten the label so the streak badge fits beside it.
fn compact_gate_label(w: f32, completed: usize) -> String {
    if w < 360. {
        format!("GATE {:02} / {GATES}", completed % GATES + 1)
    } else {
        gate_label(completed)
    }
}

/// Compact next-gate label left x, centered between the margin and the menu corner.
fn compact_gate_x(w: f32, next: &str) -> f32 {
    let margin = (w * 0.035).clamp(18.0, 44.0);
    let right = w - MENU_CORNER.0;
    margin + ((right - margin) - text_width(next, 14)) * 0.5
}

/// Compact timer left x; its right edge stays 8px clear of the menu corner.
fn compact_timer_x(w: f32, timer: &str) -> f32 {
    w - 80. - measure_text(timer, None, 20, 1.0).width
}

fn lap_pips(race: &Race, x: f32, y: f32, pip: f32, gap: f32) {
    let earned = race.gates_completed() % GATES;
    let threads = race.lap_threads();
    for i in 0..GATES {
        // Lap order starts after the start/finish gate: 1..13 then 0.
        let gate = (i + 1) % GATES;
        let px = x + i as f32 * (pip + gap);
        let color = if i < earned {
            if threads & (1 << gate) != 0 {
                GOLD
            } else {
                PALE
            }
        } else {
            Color::new(0.36, 0.44, 0.42, 0.9)
        };
        draw_rectangle(px, y, pip, 5., color);
        if i == earned && !race.finished {
            draw_rectangle_lines(px - 1.5, y - 1.5, pip + 3., 8., 1.5, TEAL);
        }
    }
}

impl Hud<'_> {
    pub fn draw(&self) {
        let race = self.race;
        if race.finished {
            self.finish_card();
            return;
        }
        let w = screen_width();
        let compact = w < 600.;
        if compact {
            self.top_compact();
        } else {
            self.top_wide();
        }
        self.bottom();
        self.center();
        self.callouts();
    }

    fn top_wide(&self) {
        let race = self.race;
        let w = screen_width();
        let timer = time_label(race.elapsed);
        let pace_text =
            pace(race).map(|medal| (format!("{} PACE", quips::medal_name(medal)), medal));
        // Mid-width hosts keep the plates apart by showing pace instead of the lap clock.
        let lap_line = if pace_text.is_some() && w < 1000. {
            String::new()
        } else {
            format!("LAP {}", time_label(race.lap_elapsed()))
        };
        let gap = if lap_line.is_empty() { 0. } else { 14. };
        let sub_w = width(&lap_line, 13.)
            + pace_text
                .as_ref()
                .map_or(0., |(text, _)| width(text, 13.) + gap);
        let next = wide_gate_label(w, race.gates_completed());
        let [lap_plate, clock, gate] = wide_plates(w, sub_w, &next);
        // Lap plate.
        let x = lap_plate.x;
        plate(x, 12., lap_plate.w, 60., TEAL);
        label("LAP", x + 14., 32., 13., PALE);
        let lap = (race.laps + 1).min(LAPS);
        let lap_text = format!("{lap}");
        label(&lap_text, x + 14., 64., 30., WHITE);
        label(
            &format!("/{LAPS}"),
            x + 16. + width(&lap_text, 30.),
            64.,
            16.,
            PALE,
        );
        if lap_plate.w > 100. {
            let column = x + 82.;
            if race.laps + 1 == LAPS {
                label("FINAL LAP", column, 32., 13., ORANGE);
            } else {
                label("BEST LAP", column, 32., 11., MUTED);
            }
            let best = race.best_lap().map_or("--:--.--".to_owned(), time_label);
            label(
                &best,
                column,
                58.,
                16.,
                if race.laps > 0 { WHITE } else { MUTED },
            );
        }
        // Clock plate.
        let cx = clock.x + clock.w * 0.5;
        plate(clock.x, 12., clock.w, 60., TEAL);
        label(&timer, cx - width(&timer, 30.) * 0.5, 44., 30., WHITE);
        if race.recovery_count > 0 {
            // The clock already includes recovery penalties; say so.
            let penalty = format!("+{:.0}s", race.penalty_seconds());
            right_label(&penalty, clock.x + clock.w - 8., 30., 12., ORANGE);
        }
        match pace_text {
            Some((pace_text, medal)) => {
                let left = cx - sub_w * 0.5;
                label(&lap_line, left, 64., 13., PALE);
                label(
                    &pace_text,
                    left + width(&lap_line, 13.) + gap,
                    64.,
                    13.,
                    medal_color(medal),
                );
            }
            None => label(&lap_line, cx - width(&lap_line, 13.) * 0.5, 64., 13., PALE),
        }
        // Next-gate plate, inset clear of the host's fullscreen menu corner.
        plate(gate.x, gate.y, gate.w, gate.h, TEAL);
        label(&next, gate.x + 14., 36., 14., TEAL);
        lap_pips(race, gate.x + 14., 52., 9., 3.);
        if race.streak >= 2 {
            let chip = streak_chip_text(race.streak);
            let r = wide_streak_chip(w, &chip);
            draw_rectangle(r.x, r.y, r.w, r.h, PANEL);
            draw_rectangle(r.x, r.y, 3., r.h, GOLD);
            label(&chip, r.x + 11., r.y + 16., 14., GOLD);
        }
    }

    fn top_compact(&self) {
        let race = self.race;
        let w = screen_width();
        let h = screen_height();
        let margin = (w * 0.035).clamp(18.0, 44.0);
        let short_start = h < 460. && (race.countdown > 0. || race.elapsed < 0.8);
        draw_rectangle(0., 0., w, if short_start { 62. } else { 74. }, PANEL);
        let lap_text = format!("LAP {}/{LAPS}", (race.laps + 1).min(LAPS));
        label(&lap_text, margin, 28., 14., WHITE);
        let timer = time_label(race.elapsed);
        label(&timer, compact_timer_x(w, &timer), 28., 20.0, WHITE);
        let completed = race.gates_completed();
        if short_start {
            // Reuse the next-gate row before driving starts. A separate center cue
            // would overlap boost telemetry at the 352px minimum in touch mode.
            let text = if race.countdown > 0. {
                format!("{}", race.countdown.ceil() as u32)
            } else {
                "GO".to_owned()
            };
            centered(&text, SHORT_START_ROW.0, SHORT_START_ROW.1, TEAL);
            return;
        }
        let next = compact_gate_label(w, completed);
        let gate_x = compact_gate_x(w, &next);
        label(&next, gate_x, 52., 14., TEAL);
        if race.streak >= 2 {
            // The multiplier leads the gate row, away from the menu corner.
            let badge = format!("x{}", race.streak.min(super::race::MULTIPLIER_CAP));
            label(&badge, margin, 52., 14., GOLD);
        }
        let pip_gap = 3.;
        let pip = ((w - margin * 2. - pip_gap * (GATES as f32 - 1.)) / GATES as f32).min(14.);
        let pips_w = GATES as f32 * (pip + pip_gap) - pip_gap;
        lap_pips(race, (w - pips_w) * 0.5, 62., pip, pip_gap);
    }

    fn bottom_y(&self) -> f32 {
        let h = screen_height();
        if self.input.touch_seen() {
            h - 150.0
        } else {
            h - 75.0
        }
    }

    fn bottom(&self) {
        let race = self.race;
        let w = screen_width();
        let margin = (w * 0.035).clamp(18.0, 44.0);
        let bottom = self.bottom_y();
        // Speedometer.
        let kmh = race.speed() * 3.6;
        let speed = format!("{:03}", kmh as u32);
        let speed_size = if w < 400.0 { 36 } else { 47 };
        let speed_width = measure_text(&speed, None, speed_size, 1.).width;
        // Narrow screens use short traction words in the unit column.
        let compact = w < 600.;
        let (traction, traction_color) = match (race.traction(), compact) {
            (Traction::Track, _) => ("", PALE),
            (Traction::Sliding, false) => ("SLIDING", ORANGE),
            (Traction::Sliding, true) => ("SLIDE", ORANGE),
            (Traction::Reversing, false) => ("REVERSE", PALE),
            (Traction::Reversing, true) => ("REV", PALE),
            (Traction::Sand, false) => ("LOOSE SAND", ORANGE),
            (Traction::Sand, true) => ("SAND", ORANGE),
        };
        let unit_x = margin + speed_width + 8.;
        let column = if compact {
            width("SLIDE", 12.)
        } else {
            width("LOOSE SAND", 12.)
        }
        .max(width("KM/H", 13.));
        let speed_plate_w = speed_width + 24. + column;
        let accent = if race.boosting { ORANGE } else { TEAL };
        plate(margin - 12., bottom - 50., speed_plate_w, 70., accent);
        label(&speed, margin, bottom, speed_size as f32, WHITE);
        label("KM/H", unit_x, bottom, 13.0, PALE);
        label(traction, unit_x, bottom - 22., 12., traction_color);
        let bar_w = speed_plate_w - 24.;
        let fill = (race.speed() / 44.).clamp(0., 1.);
        draw_rectangle(margin, bottom + 8., bar_w, 4., DARK);
        draw_rectangle(margin, bottom + 8., bar_w * fill, 4., accent);
        // Boost meter: a fat bar with the 25% rearm notch.
        let bar_w = (w * 0.38).min(220.0);
        let bx = w - margin - bar_w;
        let cooling_hint = format!("COOLING {:.1}s", race.cooling_seconds());
        let mut boost_buffer = [0; 256];
        let boost_key = binding_hint(self.input, "boost", &mut boost_buffer);
        let ready_hint = format!("HOLD {boost_key}");
        let (boost_label, boost_hint, boost_color) = match race.boost_status() {
            BoostStatus::Countdown => ("BOOST", "", PALE),
            BoostStatus::Ready => (
                "BOOST READY",
                if self.juice.learned.boost || self.juice.veteran {
                    ""
                } else {
                    ready_hint.as_str()
                },
                TEAL,
            ),
            BoostStatus::Active => ("BOOSTING", "", ORANGE),
            BoostStatus::Overheated => ("OVERHEATED", cooling_hint.as_str(), ROSE),
            BoostStatus::Sand => ("BOOST BLOCKED", "GET OFF THE SAND", PALE),
            BoostStatus::Finished => ("TRIAL COMPLETE", "", TEAL),
        };
        plate(bx - 12., bottom - 50., bar_w + 24., 70., boost_color);
        let percent = format!("{:03}%", (race.boost * 100.0) as u32);
        let pct_w = width(&percent, 13.);
        label(
            boost_label,
            bx,
            bottom - 28.0,
            fitted_size(boost_label, bar_w - pct_w - 8., 14),
            boost_color,
        );
        right_label(&percent, bx + bar_w, bottom - 28., 13., PALE);
        let bar_y = bottom - 18.;
        let bar_h = 14.;
        draw_rectangle(bx, bar_y, bar_w, bar_h, Color::new(0.1, 0.16, 0.16, 0.95));
        draw_rectangle(bx, bar_y, bar_w * race.boost, bar_h, boost_color);
        // Segment ticks read as "chunks" of boost at a glance.
        for i in 1..10 {
            let tx = bx + bar_w * i as f32 / 10.;
            draw_rectangle(tx, bar_y, 1., bar_h, Color::new(0.06, 0.1, 0.1, 0.55));
        }
        draw_rectangle(bx + bar_w * 0.25 - 1., bar_y - 3., 2., bar_h + 6., PALE);
        if !boost_hint.is_empty() {
            label(
                boost_hint,
                bx,
                bottom + 14.,
                fitted_size(boost_hint, bar_w, 12),
                PALE,
            );
        }
    }

    /// Countdown, alerts and the one onboarding line.
    fn center(&self) {
        let race = self.race;
        let w = screen_width();
        let h = screen_height();
        let compact = w < 600.;
        let short = h < 460.;
        let touch = self.input.touch_seen();
        let motion = self.juice.motion();
        if race.countdown > 0.0 && !(compact && short) {
            let number = race.countdown.ceil() as u32;
            let phase = 1. - race.countdown.fract();
            let punch = 1. + 0.45 * (1. - phase.min(1.)).powi(3) * motion.min(1.);
            if short {
                centered(&format!("{number}"), 150., 26, WHITE);
            } else {
                let card_w = (w - 32.).min(420.);
                // Narrow screens wrap the radio line, so the card starts below it.
                let top = if compact { 156. } else { h * 0.26 };
                draw_rectangle((w - card_w) * 0.5, top, card_w, 54., PANEL);
                centered_in(
                    "THREE LAPS  ·  FOURTEEN GATES",
                    top + 24.,
                    16,
                    PALE,
                    card_w - 24.,
                );
                let sub = match self.best {
                    Some(best) => format!("YOUR BEST {}", time_label(best)),
                    None => format!(
                        "GOLD UNDER {}  ·  DUSTY THE BOT DID {}",
                        time_label(quips::MEDALS[0]),
                        time_label(quips::DUSTY)
                    ),
                };
                centered_in(&sub, top + 44., 13, GOLD, card_w - 24.);
                let text = format!("{number}");
                let size = if compact { 56. } else { 80. } * punch;
                let baseline = if compact {
                    (h * 0.5).max(top + 120.)
                } else {
                    h * 0.5
                };
                label(&text, (w - width(&text, size)) * 0.5, baseline, size, WHITE);
            }
        }
        // Alerts outrank the coach; they share one slot.
        let alert_y = if touch {
            h - 106.
        } else if compact {
            self.bottom_y() - 64.
        } else {
            h - 22.
        };
        let alert_w = if compact {
            w - 32.
        } else {
            (w * 0.42).min(520.)
        };
        let alert = if self.waiting_for_controls {
            Some(("RELEASE DRIVE CONTROLS TO REARM".to_owned(), PALE))
        } else if race.recovery_timer > 0.0 {
            Some((
                format!("SAFE RETURN  ·  +{RECOVERY_PENALTY:.2}s ADDED"),
                ORANGE,
            ))
        } else if let Some(seconds) = race.recovery_in() {
            Some((
                format!("BACK TO THE TRACK!  RECOVERY IN {seconds:.1}s"),
                ORANGE,
            ))
        } else if race.off_track {
            Some(("LOOSE SAND  ·  BOOST DISABLED".to_owned(), PALE))
        } else {
            None
        };
        if let Some((text, color)) = alert {
            let alert_x = (w - alert_w) * 0.5;
            draw_rectangle(alert_x, alert_y - 20., alert_w, 28., PANEL);
            centered_in(&text, alert_y, 16, color, alert_w - 16.);
            return;
        }
        if let Some(hint) = self.coach() {
            let alpha = 1. - self.juice.hint_fade;
            if alpha > 0.01 {
                let hint_w = (width(&hint, 15.) + 32.).min(alert_w);
                draw_rectangle(
                    (w - hint_w) * 0.5,
                    alert_y - 20.,
                    hint_w,
                    28.,
                    fade(PANEL, alpha),
                );
                centered_in(&hint, alert_y, 15, fade(WHITE, alpha), hint_w - 16.);
            }
        }
    }

    /// One onboarding line for a first-time pilot, or `None` once learned.
    fn coach(&self) -> Option<String> {
        let juice = self.juice;
        let race = self.race;
        if juice.veteran || race.finished {
            return None;
        }
        let mut drive = [0; 256];
        let mut boost = [0; 256];
        let drive = binding_hint(self.input, "move_forward", &mut drive);
        let boost = binding_hint(self.input, "boost", &mut boost);
        if race.countdown > 0. {
            Some(format!("HOLD {drive} TO DRIVE  ·  {boost} TO BOOST"))
        } else if !juice.learned.drive {
            Some(format!("HOLD {drive} TO DRIVE"))
        } else if !juice.learned.thread {
            Some("FLY THROUGH THE CENTER OF THE TEAL GATE".to_owned())
        } else if !juice.learned.boost {
            Some(format!("HOLD {boost} WHILE DRIVING TO BOOST"))
        } else if juice.hint_fade < 1. {
            Some("NICE. THREAD GATES TO REFILL BOOST.".to_owned())
        } else {
            None
        }
    }

    /// Rising pops, the lap banner and the radio toast.
    pub fn callouts(&self) {
        let juice = self.juice;
        let w = screen_width();
        let h = screen_height();
        let compact = w < 600.;
        let rise = juice.motion().min(1.);
        let short = h < 460.;
        if let Some(pop) = &juice.pop {
            let alpha = pop.alpha();
            let t = pop.age / pop.life;
            let scale = if rise > 0. {
                1. + 0.35 * (1. - pop.grow()).powi(2)
            } else {
                1.
            };
            let size = pop.size * scale * if compact { 0.8 } else { 1. };
            let travel = if short { 24. } else { 46. };
            let y = h * if short { 0.62 } else { 0.44 } - travel * t * rise;
            let head_w = width(&pop.head, size);
            let sub_w = width(&pop.sub, 14.);
            let box_w = head_w.max(sub_w) + 28.;
            let box_h = if pop.sub.is_empty() {
                size + 12.
            } else {
                size + 32.
            };
            draw_rectangle(
                (w - box_w) * 0.5,
                y - size - 4.,
                box_w,
                box_h,
                fade(PANEL, alpha * 0.8),
            );
            label(
                &pop.head,
                (w - head_w) * 0.5,
                y,
                size,
                fade(pop.color, alpha),
            );
            if !pop.sub.is_empty() {
                label(
                    &pop.sub,
                    (w - sub_w) * 0.5,
                    y + 20.,
                    14.,
                    fade(WHITE, alpha),
                );
            }
        }
        if let Some(banner) = &juice.banner {
            let alpha = banner.alpha();
            let y = if short && !compact {
                172.
            } else {
                h * if compact { 0.30 } else { 0.27 }
            };
            let card_w = (w - 32.).min(440.);
            draw_rectangle((w - card_w) * 0.5, y - 40., card_w, 70., fade(PANEL, alpha));
            draw_rectangle(
                (w - card_w) * 0.5,
                y - 40.,
                card_w,
                3.,
                fade(banner.color, alpha),
            );
            centered_in(&banner.head, y, 30, fade(banner.color, alpha), card_w - 24.);
            centered_in(
                &banner.sub,
                y + 22.,
                15,
                fade(banner.sub_color, alpha),
                card_w - 24.,
            );
        }
        if let Some(toast) = &juice.toast {
            let alpha = toast.alpha();
            let tag = "KDUNE";
            let size = if compact { 13. } else { 15. };
            let y = if compact {
                if h < 460. && self.race.countdown > 0. {
                    108.
                } else {
                    96.
                }
            } else if self.race.streak >= 2 && w < 1000. {
                122.
            } else {
                98.
            };
            let available = (w - 32.).min(820.);
            let tag_w = width(tag, 11.) + 12.;
            let line_w = width(&toast.line, size);
            if compact || line_w + tag_w + 36. > available {
                // Stacked layout: tag line, then one or two text lines.
                let (first, second) = split_lines(&toast.line, available - 16., size as u16);
                let extra = if second.is_some() { size + 4. } else { 0. };
                draw_rectangle(
                    (w - available) * 0.5,
                    y - 30.,
                    available,
                    42. + extra,
                    fade(PANEL, alpha * 0.9),
                );
                label(
                    tag,
                    (w - width(tag, 11.)) * 0.5,
                    y - 16.,
                    11.,
                    fade(TEAL, alpha),
                );
                for (i, line) in [Some(first), second].into_iter().flatten().enumerate() {
                    let f = fitted_size(line, available - 16., size as u16);
                    label(
                        line,
                        (w - width(line, f)) * 0.5,
                        y + 6. + i as f32 * (size + 4.),
                        f,
                        fade(PALE, alpha),
                    );
                }
            } else {
                let pill_w = line_w + tag_w + 36.;
                let x = (w - pill_w) * 0.5;
                draw_rectangle(x, y - 19., pill_w, 28., fade(PANEL, alpha * 0.9));
                draw_rectangle(x + 8., y - 14., tag_w, 18., fade(TEAL, alpha));
                label(
                    tag,
                    x + 14.,
                    y,
                    11.,
                    fade(Color::new(0.04, 0.08, 0.08, 1.), alpha),
                );
                label(
                    &toast.line,
                    x + 8. + tag_w + 12.,
                    y,
                    size,
                    fade(PALE, alpha),
                );
            }
        }
    }

    pub fn pause_card(&self) {
        let race = self.race;
        let w = screen_width();
        let h = screen_height();
        draw_rectangle(0., 0., w, h, Color::new(0.02, 0.04, 0.05, 0.55));
        let card_w = (w - 32.).min(420.);
        let card_h = 196.;
        let x = (w - card_w) * 0.5;
        let y = (h - card_h) * 0.5;
        draw_rectangle(x, y, card_w, card_h, CARD);
        draw_rectangle(x, y, card_w, 3., TEAL);
        let inner = card_w - 32.;
        centered_in("PAUSED", y + 46., 30, WHITE, inner);
        let seed = race.gates_completed() as u32 + race.impacts;
        centered_in(quips::pause(seed), y + 76., 14, PALE, inner);
        let mut status = format!(
            "LAP {}/{LAPS}  ·  {}",
            (race.laps + 1).min(LAPS),
            time_label(race.elapsed)
        );
        if race.streak >= 2 {
            status.push_str(&format!("  ·  STREAK {}", race.streak));
        }
        centered_in(&status, y + 108., 14, MUTED, inner);
        let mut pause = [0; 256];
        let mut restart = [0; 256];
        let pause = binding_hint(self.input, "pause", &mut pause);
        let restart = binding_hint(self.input, "restart", &mut restart);
        centered_in(
            &format!("RESUME {pause}  ·  RESTART {restart}  ·  CONTROLS F2"),
            y + 158.,
            15,
            TEAL,
            inner,
        );
    }

    fn finish_card(&self) {
        let race = self.race;
        let results = self.results;
        let juice = self.juice;
        let w = screen_width();
        let h = screen_height();
        let time = race.elapsed;
        let medal = quips::medal_for(time);
        let rank = quips::rank(summary(race));
        let new_best = results.new_best(time);
        let full = h >= 600.;
        let card_w = (w - 32.).min(540.);
        let card_h: f32 = if full { 470. } else { 330. };
        // Slide-and-settle entrance; reduced motion appears in place.
        let enter = (juice.finish_age / 0.35).min(1.);
        let slide = (1. - enter).powi(3) * 40. * juice.motion().min(1.);
        let x = (w - card_w) * 0.5;
        let y = ((h - card_h) * 0.5).max(8.) + slide;
        draw_rectangle(0., 0., w, h, Color::new(0.02, 0.04, 0.05, 0.35 * enter));
        draw_rectangle(x, y, card_w, card_h, CARD);
        draw_rectangle(x, y, card_w, 4., medal_color(medal));
        let inner = card_w - 32.;
        let c = |text: &str, y: f32, size: u16, color: Color| {
            centered_in(text, y, size, color, inner);
        };
        let mut row = y + 34.;
        if new_best && results.previous_best.is_some() {
            c("NEW PERSONAL BEST!", row, 18, GOLD);
        } else {
            c("CIRCUIT COMPLETE", row, 16, PALE);
        }
        row += 40.;
        let title_color = if medal == Medal::Unranked {
            GOLD
        } else {
            medal_color(medal)
        };
        c(rank.title, row, 30, title_color);
        if full {
            row += 24.;
            c(rank.quip, row, 14, PALE);
        }
        row += if full { 58. } else { 50. };
        let clock = time_label(time);
        c(&clock, row, if full { 50 } else { 42 }, WHITE);
        row += 26.;
        // Medal pill and penalty line.
        let medal_text = format!("{} MEDAL", quips::medal_name(medal));
        let medal_text = if medal == Medal::Unranked {
            "NO MEDAL YET".to_owned()
        } else {
            medal_text
        };
        let medal_w = width(&medal_text, 15.) + 34.;
        let mx = (w - medal_w) * 0.5;
        draw_rectangle(mx, row - 16., medal_w, 24., fade(medal_color(medal), 0.18));
        draw_circle(mx + 13., row - 4., 6., medal_color(medal));
        label(&medal_text, mx + 25., row + 1., 15., medal_color(medal));
        if full {
            row += 26.;
            c(
                &format!(
                    "DRIVING {} + {:.2}s RECOVERY  ·  IMPACTS COST SPEED, NOT TIME",
                    time_label(race.driving_seconds()),
                    race.penalty_seconds()
                ),
                row,
                12,
                MUTED,
            );
        }
        row += if full { 22. } else { 18. };
        // Stats grid.
        let best_lap = race.best_lap().unwrap_or(0.);
        let stats: [(&str, String, Color); 6] = [
            (
                "BEST LAP",
                time_label(best_lap),
                if results.new_lap(race.best_lap()) {
                    GOLD
                } else {
                    WHITE
                },
            ),
            (
                "THREADED",
                format!("{}/{}", race.threaded, GATES * LAPS as usize),
                if race.perfect_laps > 0 { GOLD } else { WHITE },
            ),
            ("BEST STREAK", format!("{}", race.best_streak), WHITE),
            ("SCRAPES", format!("{}", race.scrapes), WHITE),
            ("IMPACTS", format!("{}", race.impacts), WHITE),
            ("RECOVERIES", format!("{}", race.recovery_count), WHITE),
        ];
        let rows = if full { 2 } else { 1 };
        let col_w = inner / 3.;
        for (i, (name, value, color)) in stats.iter().take(rows * 3).enumerate() {
            let cx = x + 16. + col_w * ((i % 3) as f32 + 0.5);
            let ry = row + (i / 3) as f32 * 50.;
            let name_size = fitted_size(name, col_w - 8., 11);
            label(
                name,
                cx - width(name, name_size) * 0.5,
                ry + 14.,
                name_size,
                MUTED,
            );
            let value_size = fitted_size(value, col_w - 8., 20);
            label(
                value,
                cx - width(value, value_size) * 0.5,
                ry + 36.,
                value_size,
                *color,
            );
        }
        row += rows as f32 * 50. + 12.;
        if full {
            let laps = race
                .lap_times
                .iter()
                .enumerate()
                .map(|(i, t)| format!("L{} {}", i + 1, time_label(*t)))
                .collect::<Vec<_>>()
                .join("   ");
            c(&laps, row, 13, PALE);
            row += 24.;
        }
        // Personal best and the next-goal hook.
        let best_line = match (results.demo, results.previous_best) {
            (true, _) => "DEMO RUN  ·  DUSTY'S TIMES ARE NOT RECORDED".to_owned(),
            (false, None) => "FIRST FINISH RECORDED. NOW BEAT IT.".to_owned(),
            (false, Some(best)) if new_best => {
                format!(
                    "NEW BEST BY {:.2}s  ·  OLD {}",
                    best - time,
                    time_label(best)
                )
            }
            (false, Some(best)) => format!(
                "PERSONAL BEST {}  ({})",
                time_label(best),
                signed(time - best)
            ),
        };
        c(
            &best_line,
            row,
            14,
            if new_best && !results.demo {
                GOLD
            } else {
                PALE
            },
        );
        row += 24.;
        c(&next_goal(race, results), row, 14, MINT);
        let mut retry = [0; 256];
        let mut confirm = [0; 256];
        let retry = binding_hint(self.input, "restart", &mut retry);
        let confirm = binding_hint(self.input, "confirm", &mut confirm);
        let action = format!("ONE MORE RUN  ·  {retry} / {confirm} / CLICK");
        let button_w = (width(&action, 16.) + 40.).min(inner);
        let by = y + card_h - 46.;
        draw_rectangle((w - button_w) * 0.5, by, button_w, 32., fade(TEAL, 0.22));
        draw_rectangle_lines((w - button_w) * 0.5, by, button_w, 32., 2., TEAL);
        centered_in(&action, by + 22., 16, WHITE, button_w - 16.);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_goal_always_names_a_concrete_target() {
        let mut race = Race::new();
        race.finished = true;
        let results = Results::default();
        race.elapsed = 40.;
        assert!(next_goal(&race, &results).starts_with("NEXT: BRONZE"));
        race.elapsed = 30.;
        assert!(next_goal(&race, &results).contains("FIND 0.50s"));
        race.elapsed = 29.;
        assert!(next_goal(&race, &results).contains("MIRAGE"));
        race.perfect_laps = 1;
        assert!(next_goal(&race, &results).contains("ALL 42"));
        race.threaded = 42;
        assert!(next_goal(&race, &results).contains("BEAT 00:28.75"));
    }

    #[test]
    fn personal_bests_ignore_demo_runs_and_ties() {
        let results = Results {
            previous_best: Some(31.),
            previous_lap: Some(10.),
            demo: false,
        };
        assert!(results.new_best(30.9));
        assert!(!results.new_best(31.));
        assert!(results.new_lap(Some(9.9)) && !results.new_lap(None));
        let demo = Results {
            demo: true,
            ..results
        };
        assert!(!demo.new_best(1.) && !demo.new_lap(Some(1.)));
        assert!(Results::default().new_best(99.));
    }

    #[test]
    fn pace_waits_for_a_lap_then_projects_the_remaining_gates() {
        let mut race = Race::new();
        race.elapsed = 8.;
        race.next_gate = 12;
        assert_eq!(pace(&race), None, "lap one has no honest pace");
        race.laps = 1;
        race.next_gate = 1;
        race.lap_times[0] = 9.7;
        race.elapsed = 9.7; // 9.7 + 2 * 9.7 = 29.1
        assert_eq!(pace(&race), Some(Medal::Gold));
        race.lap_times[0] = 10.5;
        race.elapsed = 10.5; // 31.5
        assert_eq!(pace(&race), Some(Medal::Silver));
        race.elapsed = 14.; // A slow start to lap two shows up immediately.
        assert_eq!(pace(&race), Some(Medal::Bronze));
        race.finished = true;
        assert_eq!(pace(&race), None);
    }

    #[test]
    fn top_right_menu_corner_stays_clear_at_every_reviewed_size() {
        // Worst-case strings; text ink spans [baseline - size, baseline + 2 shadow].
        let corner_clear =
            |w: f32, right: f32, top: f32| top >= MENU_CORNER.1 || right <= w - MENU_CORNER.0;
        let timer = time_label(599.99);
        for w in [1280., 1600., 844., 768., 700., 600.] {
            let next = wide_gate_label(w, GATES - 1);
            let gate = wide_gate_plate(w, &next);
            assert!(
                corner_clear(w, gate.x + gate.w, gate.y),
                "gate plate at {w}"
            );
            assert!(gate.x + 14. + width(&next, 14.) + 1. <= w - MENU_CORNER.0);
            let chip = streak_chip_text(99);
            let r = wide_streak_chip(w, &chip);
            assert!(corner_clear(w, r.x + r.w, r.y), "streak chip at {w}");
            // The clock plate stays left of the gate plate.
            let lap = width("LAP 09:59.99", 13.);
            let pace = width("NO MEDAL PACE", 13.);
            let sub = if w < 1000. {
                lap.max(pace)
            } else {
                lap + 14. + pace
            };
            let [lap_plate, clock, plate] = wide_plates(w, sub, &next);
            assert_eq!(plate, gate);
            assert!(
                lap_plate.x + lap_plate.w + 8. <= clock.x,
                "lap/clock touch at {w}"
            );
            assert!(clock.x + clock.w + 8. <= gate.x, "clock/gate touch at {w}");
        }
        for w in [390., 320., 359., 360., 599.] {
            let next = compact_gate_label(w, GATES - 1);
            let gate_right = compact_gate_x(w, &next) + width(&next, 14.) + 1.;
            assert!(
                corner_clear(w, gate_right, 52. - 14.),
                "compact gate at {w}"
            );
            let timer_right = compact_timer_x(w, &timer) + width(&timer, 20.) + 1.;
            assert!(
                corner_clear(w, timer_right, 28. - 20.),
                "compact timer at {w}"
            );
            // The compact multiplier leads the gate row without touching it.
            let margin = (w * 0.035).clamp(18.0, 44.0);
            assert!(
                margin + width("x5", 14.) + 8. <= compact_gate_x(w, &next),
                "{w}"
            );
        }
    }
}
