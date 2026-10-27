//! Race HUD, countdown, title, pause and results screens in logical pixels.
//!
//! The play HUD keeps one job per corner on soft rounded glass plates: lap
//! (top left), clock and pace (top center), the next gate with this lap's
//! threading pips (top right), a speed gauge (bottom left) and the boost meter
//! (bottom right). Copy uses the mixed-case Modern face; big numerals and
//! arcade callouts keep the Classic face for retro punch. One radio toast, one
//! banner and one alert slot keep messages from stacking.
//!
//! Browser contract (web/tests/wasm-ui.mjs): narrow layouts draw the white
//! 14px lap and 20px timer in the top 32px, and wide layouts draw a 14px teal
//! next-gate count above y=52, all through the classic per-pixel glyph path;
//! every readable element stays out of the host's top-right menu corner.
use super::fmtbuf::{Buf, buf, clock};
use super::juice::{GOLD, Juice, MINT, ROSE};
use super::presentation::{binding_hint, text_width as classic_width};
use super::quips::{self, RunSummary};
use super::race::{BoostStatus, GATES, LAPS, MULTIPLIER_CAP, RECOVERY_PENALTY, Race, Traction};
use super::{ORANGE, SHORT_START_ROW, TEAL, label};
use ferravex_play::gameplay::trial::Medal;
use ferravex_play::tween::{Ease, Entrance, Motion, RollCounter, Spring};
use ferravex_render::input::PlayerInput;
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Cap, Fill, Shadow, Stroke};
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_render::ui::kit::{self, ButtonKind, ButtonMotion, ButtonState, KitTheme};
use std::f32::consts::{FRAC_PI_4, PI};

pub(super) const INK: Color = Color::new(0.98, 0.95, 0.89, 1.0);
pub(super) const MUTED: Color = Color::new(0.74, 0.71, 0.64, 1.0);
pub(super) const AQUA: Color = Color::new(0.45, 0.90, 0.82, 1.0);
const SHADOW: Color = Color::new(0.0, 0.0, 0.0, 0.5);
const OUTLINE: Color = Color::new(0.10, 0.06, 0.03, 0.9);
const GLASS_TOP: Color = Color::new(0.14, 0.17, 0.19, 0.80);
const GLASS_BOTTOM: Color = Color::new(0.06, 0.08, 0.10, 0.86);
const LINE: Color = Color::new(1.0, 1.0, 1.0, 0.10);
const TRACK: Color = Color::new(0.03, 0.05, 0.06, 0.85);
const SILVER: Color = Color::new(0.84, 0.88, 0.90, 1.0);
const BRONZE: Color = Color::new(0.90, 0.60, 0.36, 1.0);
const RADIUS: f32 = 12.;

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

/// Presentation-only UI motion, advanced on frame time (menus) so cards and
/// buttons stay responsive while the race is paused or finished.
pub(super) struct UiMotion {
    /// Boost "ghost" that trails a drain, like a chip-damage bar.
    ghost: Spring,
    /// Displayed gauge fraction, smoothed so the needle never jitters.
    gauge: Spring,
    pub retry: ButtonMotion,
    /// Finish clock roll-up, in centiseconds.
    roll: RollCounter,
    pub pause_age: f32,
    pub title_age: f32,
    pub motion: Motion,
}

impl UiMotion {
    pub fn new() -> Self {
        Self {
            ghost: Spring::critical(1., 2.5),
            gauge: Spring::critical(0., 9.),
            retry: ButtonMotion::default(),
            roll: RollCounter::new(0),
            pause_age: 0.,
            title_age: 0.,
            motion: Motion::Full,
        }
    }
    pub fn reset(&mut self) {
        let motion = self.motion;
        let title_age = self.title_age;
        *self = Self::new();
        self.motion = motion;
        self.title_age = title_age;
    }
    pub fn update(&mut self, frame: f32, race: &Race, paused: bool, retry: ButtonState) {
        let dt = frame.clamp(0., 0.1);
        let motion = self.motion;
        if race.boost >= self.ghost.value {
            self.ghost.snap(race.boost);
        } else {
            self.ghost.set_target(race.boost);
            if !paused {
                self.ghost.advance(dt, motion);
            }
        }
        self.gauge.set_target((race.speed() / 44.).clamp(0., 1.));
        if !paused {
            self.gauge.advance(dt, motion);
        }
        self.pause_age = if paused { self.pause_age + dt } else { 0. };
        self.title_age += dt;
        if race.finished {
            let target = (race.elapsed * 100.).round() as i64;
            if self.roll.target() != target {
                self.roll.set_target(target);
            }
            self.roll.advance(dt, motion);
        } else if self.roll.target() != 0 {
            self.roll.snap(0);
        }
        self.retry.update(retry, dt, motion);
    }
}

/// Everything the HUD reads for one frame. Nothing here writes race state.
pub(super) struct Hud<'a> {
    pub race: &'a Race,
    pub input: &'a PlayerInput,
    pub juice: &'a Juice,
    pub results: &'a Results,
    pub ui: &'a UiMotion,
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

pub(super) fn modern(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color).with_shadow(fade(SHADOW, color.a))
}

pub(super) fn classic(size: f32, color: Color) -> TextStyle {
    TextStyle::classic(size, color).with_shadow(fade(SHADOW, color.a))
}

/// Arcade callout: classic face with a dark outline that reads on any sand.
pub(super) fn callout(size: f32, color: Color) -> TextStyle {
    TextStyle::classic(size, color)
        .with_outline(fade(OUTLINE, color.a))
        .with_shadow(fade(SHADOW, color.a))
}

fn centered_text(text: &str, cx: f32, y: f32, style: TextStyle) {
    draw_text_styled(text, cx, y, style.with_align(TextAlign::Center));
}

/// Rounded "glass" plate: soft shadow, top-lit gradient, hairline edge and an
/// optional accent stripe along the top.
pub(super) fn glass(rect: Rect, radius: f32, accent: Option<Color>, alpha: f32) {
    shapes::drop_shadow(
        rect,
        radius,
        Shadow::soft(Color::new(0., 0., 0., 0.32 * alpha))
            .offset(vec2(0., 4.))
            .blur(14.),
    );
    shapes::fill_rounded_rect(
        rect,
        radius,
        Fill::vertical(rect, fade(GLASS_TOP, alpha), fade(GLASS_BOTTOM, alpha)),
    );
    shapes::stroke_rounded_rect(rect, radius, 1., fade(LINE, alpha));
    if let Some(accent) = accent {
        let stripe = Rect::new(rect.x + radius, rect.y + 1., rect.w - radius * 2., 2.);
        shapes::fill_rounded_rect(stripe, 1., fade(accent, 0.9 * alpha));
    }
}

/// Split text into one or two lines at the space nearest the middle so it
/// fits `available` at its authored size instead of shrinking.
fn split_lines(text: &str, available: f32, style: TextStyle) -> (&str, Option<&str>) {
    if text_width(text, style) <= available {
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
pub(super) fn next_goal(race: &Race, results: &Results) -> Buf<96> {
    let time = race.elapsed;
    let medal = quips::medal_for(time);
    let best = results
        .previous_best
        .filter(|_| !results.demo)
        .map_or(time, |b| b.min(time));
    if let Some((name, target)) = quips::next_medal(medal) {
        buf!(
            "Next: {} under {} · find {:.2}s",
            title_case(name),
            &*clock(target),
            (time - target).max(0.01)
        )
    } else if race.perfect_laps == 0 {
        buf!("Next: a mirage lap. Thread all 14 in one lap.")
    } else if race.threaded < (GATES * LAPS as usize) as u32 {
        buf!("Next: thread all 42 gates for Sand Surgeon")
    } else {
        buf!("Next: beat {}. The sand believes.", &*clock(best - 0.25))
    }
}

fn title_case(medal: &str) -> &'static str {
    match medal {
        "GOLD" => "Gold",
        "SILVER" => "Silver",
        "BRONZE" => "Bronze",
        _ => "No medal",
    }
}

fn medal_word(medal: Medal) -> &'static str {
    title_case(quips::medal_name(medal))
}

/// Speed lines and edge flashes, drawn on top of the world under the HUD.
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
        let tint = if race.boosting {
            Color::new(1., 0.86, 0.62, 1.)
        } else {
            Color::new(1., 0.97, 0.9, 1.)
        };
        for i in 0..26 {
            let angle = i as f32 * 2.399_963 + 0.4;
            let phase = (juice.time * 1.7 + i as f32 * 0.383).fract();
            let dir = vec2(angle.cos(), angle.sin());
            let start = reach * (0.42 + phase * 0.5);
            let length = reach * (0.10 + 0.08 * lines);
            let a = center + dir * start;
            let b = center + dir * (start + length);
            let alpha = lines * 0.3 * (1. - phase) * phase * 4.;
            draw_line(a.x, a.y, b.x, b.y, 2., fade(tint, alpha));
        }
    }
    if juice.flash > 0.01 {
        // A soft radial edge glow rather than hard bands.
        let c = juice.flash_color;
        let edge = w.min(h) * 0.09;
        for (rect, fill) in [
            (
                Rect::new(0., 0., w, edge),
                Fill::vertical(
                    Rect::new(0., 0., w, edge),
                    fade(c, 0.3 * juice.flash),
                    fade(c, 0.),
                ),
            ),
            (
                Rect::new(0., h - edge, w, edge),
                Fill::vertical(
                    Rect::new(0., h - edge, w, edge),
                    fade(c, 0.),
                    fade(c, 0.3 * juice.flash),
                ),
            ),
            (
                Rect::new(0., 0., edge, h),
                Fill::horizontal(
                    Rect::new(0., 0., edge, h),
                    fade(c, 0.3 * juice.flash),
                    fade(c, 0.),
                ),
            ),
            (
                Rect::new(w - edge, 0., edge, h),
                Fill::horizontal(
                    Rect::new(w - edge, 0., edge, h),
                    fade(c, 0.),
                    fade(c, 0.3 * juice.flash),
                ),
            ),
        ] {
            shapes::fill_rounded_rect(rect, 0., fill);
        }
    }
}

/// The browser host's fullscreen menu button reserves the top-right corner:
/// nothing readable may sit at `x > width - MENU_CORNER.0` and `y < MENU_CORNER.1`.
pub(super) const MENU_CORNER: (f32, f32) = (72., 60.);
/// Right edge of the wide right-hand cluster, inset past the menu corner.
const WIDE_RIGHT: f32 = 76.;

fn gate_count(completed: usize) -> Buf<16> {
    buf_small(format_args!("{:02} / {GATES}", completed % GATES + 1))
}

fn buf_small(args: std::fmt::Arguments) -> Buf<16> {
    let mut out = Buf::new();
    let _ = std::fmt::Write::write_fmt(&mut out, args);
    out
}

fn streak_chip_text(streak: u32) -> Buf<96> {
    buf!("★ Streak {streak}  ×{}", streak.min(MULTIPLIER_CAP))
}

/// Wide next-gate plate, right-aligned inside the inset.
fn wide_gate_plate(w: f32) -> Rect {
    let pips_w = GATES as f32 * 12. - 3.;
    let label_w = text_width("Next gate", modern(16., INK)) + 10. + classic_width("14 / 14", 14);
    let gate_w = pips_w.max(label_w) + 28.;
    Rect::new(w - WIDE_RIGHT - gate_w, 12., gate_w, 60.)
}

/// Mid-width hosts drop the best-lap column and use a smaller clock.
fn mid_width(w: f32) -> bool {
    w < 860.
}

/// Wide top plates (lap, clock, gate). The clock slides between its
/// neighbours when centering would make them touch.
fn wide_plates(w: f32, sub_w: f32) -> [Rect; 3] {
    let margin = (w * 0.035).clamp(18.0, 44.0);
    let gate = wide_gate_plate(w);
    let mid = mid_width(w);
    let lap = Rect::new(margin - 12., 12., if mid { 92. } else { 204. }, 60.);
    let timer_w = text_width("00:00.00", classic(if mid { 24. } else { 32. }, INK));
    let clock_w = (sub_w.max(timer_w) + 32.).max(if mid { 200. } else { 236. });
    let mut x = (w - clock_w) * 0.5;
    if x < lap.x + lap.w + 8. || x + clock_w > gate.x - 8. {
        x = (lap.x + lap.w + gate.x - clock_w) * 0.5;
    }
    [lap, Rect::new(x, 12., clock_w, 60.), gate]
}

/// Wide streak chip under the gate plate, right-aligned inside the inset.
fn wide_streak_chip(w: f32, chip: &str) -> Rect {
    let chip_w = text_width(chip, modern(16., GOLD)) + 24.;
    Rect::new(w - WIDE_RIGHT - chip_w, 78., chip_w, 26.)
}

fn compact_gate_text(completed: usize) -> Buf<96> {
    buf!("Next gate {}", &*gate_count(completed))
}

/// Compact next-gate label left x, centered between the margin and the menu corner.
fn compact_gate_x(w: f32, text: &str) -> f32 {
    let margin = (w * 0.035).clamp(18.0, 44.0);
    let right = w - MENU_CORNER.0;
    margin + ((right - margin) - text_width(text, modern(16., INK))) * 0.5
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
                MUTED
            }
        } else {
            Color::new(0.30, 0.36, 0.36, 0.9)
        };
        draw_rectangle(px, y, pip, 4., color);
        if i == earned && !race.finished {
            draw_rectangle(px - 1., y - 2., pip + 2., 8., AQUA);
        }
    }
}

fn speed_kmh(race: &Race) -> Buf<16> {
    buf_small(format_args!("{:03}", (race.speed() * 3.6) as u32))
}

impl Hud<'_> {
    pub fn draw(&self) {
        let race = self.race;
        if self.waiting_for_controls {
            title_screen(self.ui, self.input, self.best, 1.);
            return;
        }
        if race.finished {
            self.finish_card();
            return;
        }
        if screen_width() < 600. {
            self.top_compact();
        } else {
            self.top_wide();
        }
        self.gauge();
        self.boost();
        self.center();
        self.callouts();
    }

    fn top_wide(&self) {
        let race = self.race;
        let w = screen_width();
        let mid = mid_width(w);
        let pace_medal = pace(race);
        let lap_clock = clock(race.lap_elapsed());
        let sub_style = modern(16., MUTED);
        // Mid-width hosts show pace instead of the lap clock when both exist.
        let show_lap = !(pace_medal.is_some() && mid);
        let lap_line = buf!("Lap {}", &*lap_clock);
        let pace_line = pace_medal.map(|m| buf!("● {} pace", medal_word(m)));
        let gap = if show_lap && pace_line.is_some() {
            16.
        } else {
            0.
        };
        let sub_w = if show_lap {
            text_width(&lap_line, sub_style)
        } else {
            0.
        } + pace_line
            .as_ref()
            .map_or(0., |p| text_width(p, sub_style) + gap);
        let [lap_plate, clock_plate, gate] = wide_plates(w, sub_w);
        // Lap plate.
        glass(lap_plate, RADIUS, Some(AQUA), 1.);
        let x = lap_plate.x + 14.;
        draw_text_styled("Lap", x, 32., modern(16., MUTED));
        let lap = (race.laps + 1).min(LAPS);
        let lap_text = buf_small(format_args!("{lap}"));
        let lap_style = classic(24., INK);
        draw_text_styled(&lap_text, x, 62., lap_style);
        draw_text_styled(
            &buf_small(format_args!("/{LAPS}")),
            x + text_width(&lap_text, lap_style) + 4.,
            62.,
            modern(16., MUTED),
        );
        if !mid {
            let column = lap_plate.x + 92.;
            if race.laps + 1 == LAPS {
                draw_text_styled("Final lap!", column, 32., modern(16., ORANGE));
            } else {
                draw_text_styled("Best lap", column, 32., modern(16., MUTED));
            }
            match race.best_lap() {
                Some(best) => draw_text_styled(&clock(best), column, 60., modern(16., INK)),
                None => draw_text_styled("--:--.--", column, 60., modern(16., MUTED)),
            };
        }
        // Clock plate.
        glass(clock_plate, RADIUS, Some(AQUA), 1.);
        let cx = clock_plate.x + clock_plate.w * 0.5;
        let timer = clock(race.elapsed);
        centered_text(&timer, cx, 44., classic(if mid { 24. } else { 32. }, INK));
        if race.recovery_count > 0 {
            // The clock already includes recovery penalties; say so.
            draw_text_styled(
                &buf!("+{:.0}s", race.penalty_seconds()),
                clock_plate.x + clock_plate.w - 10.,
                30.,
                modern(16., ORANGE).with_align(TextAlign::Right),
            );
        }
        let mut left = cx - sub_w * 0.5;
        if show_lap {
            draw_text_styled(&lap_line, left, 64., sub_style);
            left += text_width(&lap_line, sub_style) + gap;
        }
        if let (Some(line), Some(medal)) = (&pace_line, pace_medal) {
            draw_text_styled(line, left, 64., modern(16., medal_color(medal)));
        }
        // Next-gate plate, inset clear of the host's fullscreen menu corner.
        glass(gate, RADIUS, Some(AQUA), 1.);
        draw_text_styled("Next gate", gate.x + 14., 36., modern(16., MUTED));
        let count = gate_count(race.gates_completed());
        // Classic teal 14px: the browser glyph contract inspects this count.
        label(
            &count,
            gate.x + gate.w - 14. - classic_width(&count, 14),
            36.,
            14.,
            TEAL,
        );
        lap_pips(race, gate.x + 14., 52., 9., 3.);
        if race.streak >= 2 {
            let chip = streak_chip_text(race.streak);
            let r = wide_streak_chip(w, &chip);
            glass(r, r.h * 0.5, None, 1.);
            draw_text_styled(
                &chip,
                r.x + 12.,
                modern(16., GOLD).centered_baseline(r.y + r.h * 0.5),
                modern(16., GOLD),
            );
        }
    }

    fn top_compact(&self) {
        let race = self.race;
        let w = screen_width();
        let h = screen_height();
        let margin = (w * 0.035).clamp(18.0, 44.0);
        let short_start = h < 460. && (race.countdown > 0. || race.elapsed < 0.8);
        let band = Rect::new(0., 0., w, if short_start { 62. } else { 76. });
        shapes::fill_rounded_rect(
            band,
            0.,
            Fill::vertical(band, fade(GLASS_BOTTOM, 0.95), fade(GLASS_TOP, 0.7)),
        );
        // Classic white 14px lap and 20px timer: the browser glyph contract.
        let lap_text = buf!("LAP {}/{LAPS}", (race.laps + 1).min(LAPS));
        label(&lap_text, margin, 28., 14., WHITE);
        let timer = clock(race.elapsed);
        label(&timer, compact_timer_x(w, &timer), 28., 20.0, WHITE);
        if short_start {
            // Reuse the next-gate row before driving starts. A separate center cue
            // would overlap boost telemetry at the 352px minimum in touch mode.
            let text = if race.countdown > 0. {
                buf!("{}", race.countdown.ceil() as u32)
            } else {
                buf!("GO")
            };
            centered_text(
                &text,
                w * 0.5,
                SHORT_START_ROW.0,
                callout(SHORT_START_ROW.1 as f32, AQUA),
            );
            return;
        }
        let next = compact_gate_text(race.gates_completed());
        let style = modern(16., AQUA);
        draw_text_styled(&next, compact_gate_x(w, &next), 54., style);
        if race.streak >= 2 {
            // The multiplier leads the gate row, away from the menu corner.
            draw_text_styled(
                &buf!("×{}", race.streak.min(MULTIPLIER_CAP)),
                margin,
                54.,
                modern(16., GOLD),
            );
        }
        let pip_gap = 3.;
        let pip = ((w - margin * 2. - pip_gap * (GATES as f32 - 1.)) / GATES as f32).min(14.);
        let pips_w = GATES as f32 * (pip + pip_gap) - pip_gap;
        lap_pips(race, (w - pips_w) * 0.5, 64., pip, pip_gap);
    }

    fn bottom_y(&self) -> f32 {
        let h = screen_height();
        if self.input.touch_seen() {
            h - 150.0
        } else {
            h - 75.0
        }
    }

    /// Radial speed gauge: a 270° arc that turns ember-orange and glows on boost.
    fn gauge(&self) {
        let race = self.race;
        let w = screen_width();
        let margin = (w * 0.035).clamp(18.0, 44.0);
        let small = w < 400.;
        let radius = if small { 42. } else { 54. };
        let center = vec2(margin - 4. + radius, self.bottom_y() + 20. - radius - 6.);
        let disc = Rect::new(
            center.x - radius - 8.,
            center.y - radius - 8.,
            (radius + 8.) * 2.,
            (radius + 8.) * 2.,
        );
        glass(disc, radius + 8., None, 1.);
        let start = PI * 0.75;
        let sweep = PI * 1.5;
        shapes::stroke_arc(
            center,
            radius - 3.,
            start,
            sweep,
            Stroke::new(7., TRACK).cap(Cap::Round),
        );
        let fill = self.ui.gauge.value.clamp(0., 1.);
        let color = if race.boosting { ORANGE } else { AQUA };
        if fill > 0.005 {
            shapes::stroke_arc(
                center,
                radius - 3.,
                start,
                sweep * fill,
                Stroke::new(7., color).cap(Cap::Round),
            );
            if race.boosting {
                let tip = start + sweep * fill;
                let p = center + vec2(tip.cos(), tip.sin()) * (radius - 3.);
                shapes::glow_disc(p, 16., fade(Color::new(1., 0.7, 0.3, 1.), 0.8));
            }
        }
        // Thin ticks every 50 km/h make the dial read as an instrument.
        for i in 0..=3 {
            let a = start + sweep * (i as f32 * 50. / 158.4);
            let dir = vec2(a.cos(), a.sin());
            let p0 = center + dir * (radius - 12.);
            let p1 = center + dir * (radius - 16.);
            draw_line(p0.x, p0.y, p1.x, p1.y, 1.5, fade(MUTED, 0.7));
        }
        let speed = speed_kmh(race);
        let size = if small { 24. } else { 32. };
        centered_text(
            &speed,
            center.x,
            classic(size, INK).centered_baseline(center.y - 2.),
            classic(size, INK),
        );
        centered_text(
            "km/h",
            center.x,
            center.y + radius * 0.52,
            modern(16., MUTED),
        );
        let compact = w < 600.;
        let traction = match (race.traction(), compact) {
            (Traction::Track, _) => None,
            (Traction::Sliding, false) => Some(("Sliding", ORANGE)),
            (Traction::Sliding, true) => Some(("Slide", ORANGE)),
            (Traction::Reversing, false) => Some(("Reverse", MUTED)),
            (Traction::Reversing, true) => Some(("Rev", MUTED)),
            (Traction::Sand, false) => Some(("Loose sand", ORANGE)),
            (Traction::Sand, true) => Some(("Sand", ORANGE)),
        };
        if let Some((text, color)) = traction {
            let style = modern(16., color);
            let pill = Rect::new(
                disc.x + disc.w + 8.,
                center.y - 13.,
                text_width(text, style) + 20.,
                26.,
            );
            glass(pill, 13., None, 1.);
            draw_text_styled(
                text,
                pill.x + 10.,
                style.centered_baseline(pill.y + 13.),
                style,
            );
        }
    }

    /// Boost meter with a trailing ghost, the 25% rearm notch and a hot glow.
    fn boost(&self) {
        let race = self.race;
        let w = screen_width();
        let margin = (w * 0.035).clamp(18.0, 44.0);
        let bottom = self.bottom_y();
        let compact = w < 600.;
        let bar_w = (w * 0.38).min(220.0);
        let bx = w - margin - bar_w;
        let cooling = buf!("Cooling {:.1}s", race.cooling_seconds());
        let mut boost_buffer = [0; 256];
        let boost_key = binding_hint(self.input, "boost", &mut boost_buffer);
        let ready_hint = if compact {
            buf!("Hold {boost_key}")
        } else {
            buf!("Hold {boost_key} to boost")
        };
        let learned = self.juice.learned.boost || self.juice.veteran;
        let (status, hint, color) = match race.boost_status() {
            BoostStatus::Countdown => ("Boost", "", MUTED),
            BoostStatus::Ready => ("Boost ready", if learned { "" } else { &*ready_hint }, AQUA),
            BoostStatus::Active => ("Boosting!", "", ORANGE),
            BoostStatus::Overheated => ("Overheated", &*cooling, ROSE),
            BoostStatus::Sand => (
                "Blocked",
                if compact {
                    "Off sand!"
                } else {
                    "Get off the sand"
                },
                MUTED,
            ),
            BoostStatus::Finished => ("Complete", "", AQUA),
        };
        let panel = Rect::new(
            bx - 14.,
            bottom - 50.,
            bar_w + 28.,
            if hint.is_empty() { 62. } else { 76. },
        );
        glass(panel, RADIUS, Some(color), 1.);
        let header = modern(16., color);
        draw_text_styled(status, bx, bottom - 26., header);
        let percent = buf!("{}%", (race.boost * 100.0) as u32);
        draw_text_styled(
            &percent,
            bx + bar_w,
            bottom - 26.,
            classic(16., INK).with_align(TextAlign::Right),
        );
        let bar = Rect::new(bx, bottom - 16., bar_w, 14.);
        shapes::fill_rounded_rect(bar, 7., TRACK);
        let ghost = self.ui.ghost.value.clamp(0., 1.);
        if ghost > race.boost + 0.005 {
            let g = Rect::new(bx, bar.y, bar_w * ghost, bar.h);
            shapes::fill_rounded_rect(g, 7., fade(Color::new(1., 0.9, 0.75, 1.), 0.35));
        }
        if race.boost > 0.01 {
            let f = Rect::new(bx, bar.y, (bar_w * race.boost).max(bar.h), bar.h);
            let (a, b) = if race.boosting {
                (
                    Color::new(1., 0.42, 0.12, 1.),
                    Color::new(1., 0.86, 0.45, 1.),
                )
            } else {
                (fade(color, 0.75), color)
            };
            shapes::fill_rounded_rect(f, 7., Fill::horizontal(f, a, b));
            let gloss = Rect::new(f.x + 4., f.y + 2., (f.w - 8.).max(1.), 3.);
            shapes::fill_rounded_rect(gloss, 1.5, Color::new(1., 1., 1., 0.25));
            if race.boosting {
                shapes::glow_disc(
                    vec2(f.x + f.w - 4., f.y + f.h * 0.5),
                    22.,
                    fade(Color::new(1., 0.6, 0.25, 1.), 0.7),
                );
            }
        }
        draw_rectangle(bx + bar_w * 0.25 - 1., bar.y - 3., 2., bar.h + 6., INK);
        if !hint.is_empty() {
            draw_text_styled(hint, bx, bottom + 18., modern(16., MUTED));
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
            let punch = 1. + 0.5 * (1. - phase.min(1.)).powi(3) * motion.min(1.);
            let text = buf_small(format_args!("{number}"));
            if short {
                centered_text(&text, w * 0.5, 160., callout(32., INK));
            } else {
                let card_w = (w - 32.).min(440.);
                let top = if compact { 156. } else { h * 0.24 };
                let card = Rect::new((w - card_w) * 0.5, top, card_w, 62.);
                glass(card, RADIUS, Some(GOLD), 1.);
                centered_text(
                    "Three laps · fourteen gates",
                    w * 0.5,
                    top + 26.,
                    modern(16., INK),
                );
                let sub = match self.best {
                    Some(best) => buf!("Your best {} · beat it", &*clock(best)),
                    None if compact => buf!("Gold under {}", &*clock(quips::MEDALS[0])),
                    None => buf!(
                        "Gold under {} · Dusty the bot did {}",
                        &*clock(quips::MEDALS[0]),
                        &*clock(quips::DUSTY)
                    ),
                };
                centered_text(&sub, w * 0.5, top + 50., modern(16., GOLD));
                let size = (if compact { 48. } else { 80. } * punch).round();
                let baseline = if compact {
                    (h * 0.5).max(top + 130.)
                } else {
                    h * 0.52
                };
                centered_text(&text, w * 0.5, baseline, callout(size, INK));
            }
        }
        // Alerts outrank the coach; they share one slot.
        let alert_y = if touch {
            h - 106.
        } else if compact {
            self.bottom_y() - 92.
        } else {
            h - 30.
        };
        let alert = if self.waiting_for_controls {
            Some((buf!("Release drive controls to rearm"), INK))
        } else if race.recovery_timer > 0.0 {
            Some((buf!("Safe return · +{RECOVERY_PENALTY:.2}s added"), ORANGE))
        } else if let Some(seconds) = race.recovery_in() {
            Some((buf!("Back to the track! Recovery in {seconds:.1}s"), ORANGE))
        } else if race.off_track {
            Some((buf!("Loose sand · boost disabled"), INK))
        } else {
            None
        };
        if let Some((text, color)) = alert {
            let style = modern(16., color);
            let pill_w = (text_width(&text, style) + 52.).min(w - 24.);
            let pill = Rect::new((w - pill_w) * 0.5, alert_y - 20., pill_w, 30.);
            glass(pill, 15., Some(color), 1.);
            draw_text_styled(
                "▲",
                pill.x + 14.,
                style.centered_baseline(pill.y + 15.),
                modern(16., ORANGE),
            );
            draw_text_styled(
                &text,
                pill.x + 34.,
                style.centered_baseline(pill.y + 15.),
                style,
            );
            return;
        }
        self.coach(alert_y);
    }

    /// One key-cap prompt for a first-time pilot, fading once learned.
    fn coach(&self, y: f32) {
        let juice = self.juice;
        let race = self.race;
        if juice.veteran || race.finished || juice.hint_fade >= 1. {
            return;
        }
        let alpha = 1. - juice.hint_fade;
        let mut drive = [0; 256];
        let mut boost = [0; 256];
        let drive = binding_hint(self.input, "move_forward", &mut drive);
        let boost = binding_hint(self.input, "boost", &mut boost);
        let prompts: [(&str, &str); 2] = if race.countdown > 0. {
            [(drive, "Drive"), (boost, "Boost")]
        } else if !juice.learned.drive {
            [(drive, "Hold to drive"), ("", "")]
        } else if !juice.learned.thread {
            [("", "Fly through the center of the glowing gate"), ("", "")]
        } else if !juice.learned.boost {
            [(boost, "Hold while driving to boost"), ("", "")]
        } else {
            [("", "Nice. Thread gates to refill boost."), ("", "")]
        };
        let theme = KitTheme {
            surface: Color::new(0.08, 0.10, 0.12, 0.9),
            raised: Color::new(0.20, 0.23, 0.25, 1.),
            header: Color::new(0.30, 0.33, 0.34, 1.),
            ink: INK,
            muted: INK,
            ..KitTheme::default()
        }
        .faded(alpha);
        let style = modern(16., INK);
        let key_w = |key: &str| (text_width(key, style) + 24.).max(23.);
        let mut total = 0.;
        for (key, action) in prompts
            .iter()
            .filter(|(k, a)| !k.is_empty() || !a.is_empty())
        {
            total +=
                if key.is_empty() { 0. } else { key_w(key) + 8. } + text_width(action, style) + 24.;
        }
        let w = screen_width();
        let pill = Rect::new((w - total - 8.) * 0.5, y - 22., total + 8., 34.);
        glass(pill, 17., None, alpha);
        let mut x = pill.x + 14.;
        for (key, action) in prompts
            .iter()
            .filter(|(k, a)| !k.is_empty() || !a.is_empty())
        {
            if !key.is_empty() {
                let cap = kit::keycap(x, y - 18., key, &theme);
                x += cap.w + 8.;
            }
            draw_text_styled(
                action,
                x,
                style.centered_baseline(y - 5.),
                modern(16., fade(INK, alpha)),
            );
            x += text_width(action, style) + 24.;
        }
    }

    /// Rising pops, the lap banner and the radio toast.
    pub fn callouts(&self) {
        let juice = self.juice;
        let w = screen_width();
        let h = screen_height();
        let compact = w < 600.;
        let short = h < 460.;
        let rise = juice.motion().min(1.);
        if let Some(pop) = &juice.pop {
            let alpha = pop.alpha();
            let t = pop.age / pop.life;
            // Pop in with a back-out overshoot, then float up and fade.
            let grow = if rise > 0. {
                Ease::BackOut.apply(pop.grow())
            } else {
                1.
            };
            let base = if compact { 24. } else { pop.size.max(24.) };
            let size = (base * (0.6 + 0.4 * grow)).round().max(8.);
            let travel = if short { 24. } else { 46. };
            let y = h * if short { 0.62 } else { 0.44 } - travel * Ease::CubicOut.apply(t) * rise;
            centered_text(&pop.head, w * 0.5, y, callout(size, fade(pop.color, alpha)));
            if !pop.sub.is_empty() {
                centered_text(&pop.sub, w * 0.5, y + 24., modern(16., fade(INK, alpha)));
            }
        }
        if let Some(banner) = &juice.banner {
            let alpha = banner.alpha();
            let look = Entrance::pop(0.85).sample(banner.age, 0, self.ui.motion);
            let y = if short && !compact {
                172.
            } else {
                h * if compact { 0.30 } else { 0.27 }
            };
            let card_w = (w - 32.).min(440.);
            let card = kit::appear(Rect::new((w - card_w) * 0.5, y - 44., card_w, 78.), look);
            glass(card, 16., Some(banner.color), alpha);
            centered_text(
                &banner.head,
                w * 0.5,
                y,
                callout(32., fade(banner.color, alpha)),
            );
            centered_text(
                &banner.sub,
                w * 0.5,
                y + 24.,
                modern(16., fade(banner.sub_color, alpha)),
            );
        }
        if let Some(toast) = &juice.toast {
            let alpha = toast.alpha();
            let look = Entrance::fade_up(10.).sample(toast.age, 0, self.ui.motion);
            let y = if compact {
                if h < 460. && self.race.countdown > 0. {
                    116.
                } else {
                    104.
                }
            } else if self.race.streak >= 2 && w < 1000. {
                126.
            } else {
                100.
            } + look.offset[1];
            let style = modern(16., fade(INK, alpha));
            let tag = modern(16., fade(AQUA, alpha));
            let tag_w = text_width("KDUNE", tag) + 22.;
            let available = (w - 32.).min(820.);
            let (first, second) = split_lines(&toast.line, available - tag_w - 36., style);
            let lines_w = text_width(first, style).max(second.map_or(0., |s| text_width(s, style)));
            let pill_w = (lines_w + tag_w + 36.).min(available);
            let pill_h = if second.is_some() { 50. } else { 30. };
            let pill = Rect::new((w - pill_w) * 0.5, y - 20., pill_w, pill_h);
            glass(pill, 15., None, alpha);
            let dot = vec2(pill.x + 16., pill.y + 15.);
            shapes::fill_circle(dot, 4., fade(Color::new(1., 0.36, 0.3, 1.), alpha));
            let base = style.centered_baseline(pill.y + 15.);
            draw_text_styled("KDUNE", pill.x + 26., base, tag);
            let x = pill.x + tag_w + 18.;
            draw_text_styled(first, x, base, style);
            if let Some(second) = second {
                draw_text_styled(second, x, base + 20., style);
            }
        }
    }

    pub fn pause_card(&self) {
        let race = self.race;
        let w = screen_width();
        let h = screen_height();
        let motion = self.ui.motion;
        let fade_in = (self.ui.pause_age / 0.18).min(1.);
        draw_rectangle(0., 0., w, h, Color::new(0.03, 0.03, 0.04, 0.45 * fade_in));
        let card_w = (w - 32.).min(440.);
        let card_h = 214.;
        let base = Rect::new(
            (w - card_w) * 0.5,
            ((h - card_h) * 0.5).max(64.),
            card_w,
            card_h,
        );
        let look = Entrance::fade_up(16.).sample(self.ui.pause_age, 0, motion);
        let theme = desert_theme().faded(look.alpha);
        let inner = kit::card(kit::appear(base, look), Some("Paused"), &theme);
        let cx = inner.x + inner.w * 0.5;
        let seed = race.gates_completed() as u32 + race.impacts;
        let quip = Entrance::fade_up(10.).sample(self.ui.pause_age, 1, motion);
        let style = modern(16., fade(INK, quip.alpha));
        let (first, second) = split_lines(quips::pause(seed), inner.w, style);
        centered_text(first, cx, inner.y + 18. + quip.offset[1], style);
        if let Some(second) = second {
            centered_text(second, cx, inner.y + 38. + quip.offset[1], style);
        }
        let status = if race.streak >= 2 {
            buf!(
                "Lap {}/{LAPS} · {} · streak {}",
                (race.laps + 1).min(LAPS),
                &*clock(race.elapsed),
                race.streak
            )
        } else {
            buf!(
                "Lap {}/{LAPS} · {}",
                (race.laps + 1).min(LAPS),
                &*clock(race.elapsed)
            )
        };
        let info = Entrance::fade_up(10.).sample(self.ui.pause_age, 2, motion);
        centered_text(
            &status,
            cx,
            inner.y + 66. + info.offset[1],
            modern(16., fade(MUTED, info.alpha)),
        );
        let mut pause = [0; 256];
        let mut restart = [0; 256];
        let pause = binding_hint(self.input, "pause", &mut pause);
        let restart = binding_hint(self.input, "restart", &mut restart);
        let keys = Entrance::fade_up(10.).sample(self.ui.pause_age, 3, motion);
        let theme = desert_theme().faded(keys.alpha);
        let prompts = [(pause, "Resume"), (restart, "Restart"), ("F2", "Controls")];
        let style = modern(16., INK);
        let item_w = |(key, action): (&str, &str)| {
            (text_width(key, style) + 24.).max(23.) + 8. + text_width(action, style)
        };
        let total: f32 = prompts.iter().map(|&p| item_w(p)).sum::<f32>() + 20. * 2.;
        if total <= inner.w {
            let mut x = cx - total * 0.5;
            for prompt in prompts {
                kit::key_prompt(
                    x,
                    inner.y + 96. + keys.offset[1],
                    &[prompt.0],
                    prompt.1,
                    &theme,
                );
                x += item_w(prompt) + 20.;
            }
        } else {
            for (i, prompt) in prompts.into_iter().enumerate() {
                let x = cx - item_w(prompt) * 0.5;
                kit::key_prompt(
                    x,
                    inner.y + 84. + i as f32 * 30. + keys.offset[1],
                    &[prompt.0],
                    prompt.1,
                    &theme,
                );
            }
        }
    }

    /// Rectangle of the results card's retry button for the current screen.
    pub fn retry_rect() -> Rect {
        let w = screen_width();
        let h = screen_height();
        let (card, _) = finish_layout(w, h);
        let bw = (card.w - 64.).min(300.);
        Rect::new(card.x + (card.w - bw) * 0.5, card.y + card.h - 60., bw, 42.)
    }

    fn finish_card(&self) {
        let race = self.race;
        let results = self.results;
        let juice = self.juice;
        let motion = self.ui.motion;
        let w = screen_width();
        let h = screen_height();
        let time = race.elapsed;
        let medal = quips::medal_for(time);
        let rank = quips::rank(summary(race));
        let new_best = results.new_best(time);
        let (card, full) = finish_layout(w, h);
        let age = juice.finish_age;
        let enter = Entrance::fade_up(28.)
            .with_duration(0.4)
            .sample(age, 0, motion);
        draw_rectangle(
            0.,
            0.,
            w,
            h,
            Color::new(0.03, 0.02, 0.02, 0.4 * enter.alpha),
        );
        let header = if new_best && results.previous_best.is_some() {
            "New personal best!"
        } else {
            "Circuit complete"
        };
        let theme = desert_theme().faded(enter.alpha);
        let inner = kit::card(kit::appear(card, enter), Some(header), &theme);
        let cx = inner.x + inner.w * 0.5;
        let item = |i: usize| {
            Entrance::fade_up(12.)
                .with_stagger(0.06)
                .sample(age - 0.15, i, motion)
        };
        let title_color = if medal == Medal::Unranked {
            GOLD
        } else {
            medal_color(medal)
        };
        // Rank title with a medal glow behind it.
        let a = item(0);
        let mut row = inner.y + 26. + a.offset[1];
        shapes::glow_disc(vec2(cx, row - 10.), 90., fade(title_color, 0.22 * a.alpha));
        centered_text(
            rank.title,
            cx,
            row,
            callout(
                if inner.w < 300. { 24. } else { 32. },
                fade(title_color, a.alpha),
            ),
        );
        if full {
            row += 26.;
            let style = modern(16., fade(MUTED, a.alpha));
            let (first, second) = split_lines(rank.quip, inner.w, style);
            centered_text(first, cx, row, style);
            if let Some(second) = second {
                row += 18.;
                centered_text(second, cx, row, style);
            }
        }
        // Rolling clock.
        let a = item(1);
        row += if full { 58. } else { 50. };
        let shown = self.ui.roll.shown() as f32 / 100.;
        let size = if inner.w < 300. { 40. } else { 48. };
        centered_text(
            &clock(shown),
            cx,
            row + a.offset[1],
            classic(size, fade(INK, a.alpha)),
        );
        row += 30.;
        // Medal badge.
        let a = item(2);
        let medal_text = if medal == Medal::Unranked {
            buf!("No medal yet")
        } else {
            buf!("{} medal", medal_word(medal))
        };
        let style = modern(16., fade(medal_color(medal), a.alpha));
        let badge_w = text_width(&medal_text, style) + 44.;
        let badge = Rect::new(cx - badge_w * 0.5, row - 14. + a.offset[1], badge_w, 28.);
        shapes::fill_rounded_rect(badge, 14., fade(medal_color(medal), 0.16 * a.alpha));
        shapes::stroke_rounded_rect(badge, 14., 1., fade(medal_color(medal), 0.5 * a.alpha));
        shapes::fill_circle(
            vec2(badge.x + 16., badge.y + 14.),
            6.,
            fade(medal_color(medal), a.alpha),
        );
        draw_text_styled(
            &medal_text,
            badge.x + 30.,
            style.centered_baseline(badge.y + 14.),
            style,
        );
        if full {
            row += 30.;
            centered_text(
                &buf!(
                    "Driving {} + {:.2}s recovery · impacts cost speed, not time",
                    &*clock(race.driving_seconds()),
                    race.penalty_seconds()
                ),
                cx,
                row,
                modern(16., fade(MUTED, a.alpha)),
            );
        }
        row += if full { 20. } else { 16. };
        // Stats grid, staggered.
        let best_lap = race.best_lap().unwrap_or(0.);
        let threaded = buf!("{}/{}", race.threaded, GATES * LAPS as usize);
        let streak = buf!("{}", race.best_streak);
        let scrapes = buf!("{}", race.scrapes);
        let impacts = buf!("{}", race.impacts);
        let recoveries = buf!("{}", race.recovery_count);
        let lap_clock = clock(best_lap);
        let stats: [(&str, &str, Color); 6] = [
            (
                "Best lap",
                &lap_clock,
                if results.new_lap(race.best_lap()) {
                    GOLD
                } else {
                    INK
                },
            ),
            (
                "Threaded",
                &threaded,
                if race.perfect_laps > 0 { GOLD } else { INK },
            ),
            ("Best streak", &streak, INK),
            ("Scrapes", &scrapes, INK),
            ("Impacts", &impacts, INK),
            ("Recoveries", &recoveries, INK),
        ];
        let rows = if full { 2 } else { 1 };
        let col_w = inner.w / 3.;
        for (i, (name, value, color)) in stats.iter().take(rows * 3).enumerate() {
            let a = item(3 + i);
            let x = inner.x + col_w * ((i % 3) as f32 + 0.5);
            let y = row + (i / 3) as f32 * 50. + a.offset[1];
            centered_text(name, x, y + 16., modern(16., fade(MUTED, a.alpha)));
            let value_size = if col_w < 100. { 16. } else { 24. };
            centered_text(
                value,
                x,
                y + 40.,
                classic(value_size, fade(*color, a.alpha)),
            );
        }
        row += rows as f32 * 50. + 14.;
        let a = item(9);
        if full {
            let laps = buf!(
                "Laps  {}  ·  {}  ·  {}",
                &*clock(race.lap_times[0]),
                &*clock(race.lap_times[1]),
                &*clock(race.lap_times[2])
            );
            centered_text(
                &laps,
                cx,
                row + a.offset[1],
                modern(16., fade(MUTED, a.alpha)),
            );
            row += 24.;
        }
        // Personal best and the next-goal hook.
        let best_line = match (results.demo, results.previous_best) {
            (true, _) => buf!("Demo run · Dusty's times are not recorded"),
            (false, None) => buf!("First finish recorded. Now beat it."),
            (false, Some(best)) if new_best => {
                buf!("New best by {:.2}s · old {}", best - time, &*clock(best))
            }
            (false, Some(best)) => buf!("Personal best {} ({:+.2})", &*clock(best), time - best),
        };
        let color = if new_best && !results.demo { GOLD } else { INK };
        centered_text(
            &best_line,
            cx,
            row + a.offset[1],
            modern(16., fade(color, a.alpha)),
        );
        row += 22.;
        let goal = next_goal(race, results);
        let style = modern(16., fade(MINT, a.alpha));
        let (first, second) = split_lines(&goal, inner.w, style);
        centered_text(first, cx, row + a.offset[1], style);
        if let Some(second) = second {
            centered_text(second, cx, row + 18. + a.offset[1], style);
        }
        let mut retry = [0; 256];
        let retry = binding_hint(self.input, "restart", &mut retry);
        let a = item(10);
        let label = buf!("One more run · {retry}");
        let button = kit::appear(Self::retry_rect(), a);
        kit::button(
            button,
            &label,
            ButtonKind::Primary,
            &self.ui.retry,
            &desert_theme().faded(a.alpha),
        );
    }
}

/// Results card rectangle and whether it has room for every row.
fn finish_layout(w: f32, h: f32) -> (Rect, bool) {
    let full = h >= 600.;
    let card_w = (w - 32.).min(560.);
    let card_h: f32 = if full { 520. } else { 360. };
    let y = ((h - card_h) * 0.5).max(8.);
    (
        Rect::new((w - card_w) * 0.5, y, card_w, card_h.min(h - 16.)),
        full,
    )
}

/// Warm desert kit theme: dark warm-teal glass, sunset accent.
pub(super) fn desert_theme() -> KitTheme {
    KitTheme {
        backdrop: Color::new(0.03, 0.02, 0.02, 0.55),
        surface: Color::new(0.07, 0.09, 0.10, 0.96),
        raised: Color::new(0.15, 0.18, 0.19, 0.98),
        header: Color::new(0.16, 0.20, 0.21, 1.0),
        ink: INK,
        muted: MUTED,
        accent: Color::new(1.0, 0.70, 0.32, 1.0),
        accent_ink: Color::new(0.16, 0.08, 0.02, 1.0),
        focus: AQUA,
        line: LINE,
        ..KitTheme::default()
    }
}

/// Title-screen layout: logo baseline, subtitle baseline and prompt top.
pub(super) fn title_layout(w: f32, h: f32) -> (f32, f32, f32) {
    let logo = (h * 0.30).clamp(110., 260.);
    let prompt = if h < 460. { h - 70. } else { h * 0.74 };
    let _ = w;
    (logo, logo + 40., prompt)
}

/// The front door: an animated wordmark over the orbiting craft, the best
/// time and a pulsing start prompt.
pub(super) fn title_screen(ui: &UiMotion, input: &PlayerInput, best: Option<f32>, alpha: f32) {
    let w = screen_width();
    let h = screen_height();
    let motion = ui.motion;
    let t = ui.title_age;
    let (logo_y, sub_y, prompt_y) = title_layout(w, h);
    // Letterbox bars give the attract shot a cinematic frame.
    let bar = (h * 0.07).min(48.) * alpha;
    draw_rectangle(0., 0., w, bar, Color::new(0.02, 0.02, 0.02, 0.85));
    draw_rectangle(0., h - bar, w, bar, Color::new(0.02, 0.02, 0.02, 0.85));
    let size = if w < 420. {
        32.
    } else if w < 900. {
        48.
    } else {
        64.
    };
    let words = ["DUNE", "SKIMMER"];
    let style = callout(size, fade(INK, alpha));
    let space = size * 0.5;
    let total: f32 = words.iter().map(|w| text_width(w, style)).sum::<f32>() + space;
    let stacked = total > w - 32.;
    let mut index = 0;
    for (line, word) in words.iter().enumerate() {
        let word_w = text_width(word, style);
        let (mut x, y) = if stacked {
            (
                (w - word_w) * 0.5,
                logo_y - size * 0.6 + line as f32 * size * 1.15,
            )
        } else {
            (
                (w - total) * 0.5
                    + if line == 1 {
                        text_width(words[0], style) + space
                    } else {
                        0.
                    },
                logo_y,
            )
        };
        for (i, ch) in word.char_indices() {
            let glyph = &word[i..i + ch.len_utf8()];
            // Staggered entrance, then a gentle heat-haze bob per letter.
            let look = Entrance::fade_up(24.)
                .with_stagger(0.045)
                .sample(t, index, motion);
            let bob = if motion.is_reduced() {
                0.
            } else {
                (t * 2.2 - index as f32 * 0.45).sin() * 2.5
            };
            let color = if line == 0 {
                Color::new(1., 0.80, 0.44, 1.)
            } else {
                INK
            };
            draw_text_styled(
                glyph,
                x,
                y + look.offset[1] + bob,
                callout(size, fade(color, look.alpha * alpha)),
            );
            x += text_width(glyph, style) + style.pixel();
            index += 1;
        }
    }
    let sub = Entrance::fade_up(12.).sample(t - 0.45, 0, motion);
    let sub_y = if stacked { sub_y + size * 0.7 } else { sub_y };
    centered_text(
        "A hover time trial through the sandstone ruins",
        w * 0.5,
        sub_y + sub.offset[1],
        modern(16., fade(INK, sub.alpha * alpha * 0.92)),
    );
    let chip = match best {
        Some(best) => buf!(
            "★ Personal best {} · {}",
            &*clock(best),
            medal_word(quips::medal_for(best))
        ),
        None => buf!(
            "★ Gold under {} · beat Dusty the demo bot",
            &*clock(quips::MEDALS[0])
        ),
    };
    let chip_look = Entrance::fade_up(10.).sample(t - 0.6, 0, motion);
    let chip_style = modern(16., fade(GOLD, chip_look.alpha * alpha));
    let chip_w = (text_width(&chip, chip_style) + 28.).min(w - 24.);
    let chip_rect = Rect::new(
        (w - chip_w) * 0.5,
        sub_y + 18. + chip_look.offset[1],
        chip_w,
        28.,
    );
    glass(chip_rect, 14., None, chip_look.alpha * alpha);
    centered_text(
        &chip,
        w * 0.5,
        chip_style.centered_baseline(chip_rect.y + 14.),
        chip_style,
    );
    // Pulsing start prompt with real bindings.
    let prompt = Entrance::fade_up(12.).sample(t - 0.8, 0, motion);
    let pulse = if motion.is_reduced() {
        1.
    } else {
        0.75 + 0.25 * (t * 3.2).sin()
    };
    let a = prompt.alpha * alpha;
    let theme = KitTheme {
        raised: Color::new(1.0, 0.72, 0.36, 1.),
        header: Color::new(1.0, 0.82, 0.5, 1.),
        surface: Color::new(0.5, 0.3, 0.12, 1.),
        ink: Color::new(0.16, 0.08, 0.02, 1.),
        muted: INK,
        ..desert_theme()
    }
    .faded(a * pulse);
    let mut confirm = [0; 256];
    let mut drive = [0; 256];
    let confirm = binding_hint(input, "confirm", &mut confirm);
    let drive = binding_hint(input, "move_forward", &mut drive);
    let action = if input.touch_seen() {
        "Tap or press to race"
    } else {
        "to race"
    };
    let style = modern(16., INK);
    let key_w = |k: &str| (text_width(k, style) + 24.).max(23.) + 4.;
    let total = key_w(confirm) + key_w(drive) + 8. + text_width(action, style);
    let pill = Rect::new(
        (w - total - 40.) * 0.5,
        prompt_y - 6. + prompt.offset[1],
        total + 40.,
        40.,
    );
    glass(pill, 20., Some(GOLD), a);
    kit::key_prompt(pill.x + 20., pill.y + 7., &[confirm, drive], action, &theme);
    let _ = FRAC_PI_4;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time_label;

    #[test]
    fn next_goal_always_names_a_concrete_target() {
        let mut race = Race::new();
        race.finished = true;
        let results = Results::default();
        race.elapsed = 40.;
        assert!(next_goal(&race, &results).starts_with("Next: Bronze"));
        race.elapsed = 30.;
        assert!(next_goal(&race, &results).contains("find 0.50s"));
        race.elapsed = 29.;
        assert!(next_goal(&race, &results).contains("mirage"));
        race.perfect_laps = 1;
        assert!(next_goal(&race, &results).contains("all 42"));
        race.threaded = 42;
        assert!(next_goal(&race, &results).contains("beat 00:28.75"));
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
        let corner_clear =
            |w: f32, right: f32, top: f32| top >= MENU_CORNER.1 || right <= w - MENU_CORNER.0;
        let timer = time_label(599.99);
        let lap = text_width("Lap 09:59.99", modern(16., INK));
        let pace = text_width("● No medal pace", modern(16., INK));
        for w in [1280., 1600., 844., 768., 700., 600.] {
            let gate = wide_gate_plate(w);
            assert!(
                corner_clear(w, gate.x + gate.w, gate.y),
                "gate plate at {w}"
            );
            let count_right = gate.x + gate.w - 14. + 1.;
            assert!(count_right <= w - MENU_CORNER.0, "gate count at {w}");
            let r = wide_streak_chip(w, &streak_chip_text(99));
            assert!(corner_clear(w, r.x + r.w, r.y), "streak chip at {w}");
            let sub = if mid_width(w) {
                lap.max(pace)
            } else {
                lap + 16. + pace
            };
            let [lap_plate, clock_plate, plate] = wide_plates(w, sub);
            assert_eq!(plate, gate);
            assert!(
                lap_plate.x + lap_plate.w + 8. <= clock_plate.x,
                "lap/clock touch at {w}"
            );
            assert!(
                clock_plate.x + clock_plate.w + 8. <= gate.x,
                "clock/gate touch at {w}"
            );
        }
        for w in [390., 320., 359., 360., 599.] {
            let next = compact_gate_text(GATES - 1);
            let gate_right = compact_gate_x(w, &next) + text_width(&next, modern(16., INK)) + 2.;
            assert!(
                corner_clear(w, gate_right, 54. - 16.),
                "compact gate at {w}"
            );
            let timer_right = compact_timer_x(w, &timer) + classic_width(&timer, 20) + 1.;
            assert!(
                corner_clear(w, timer_right, 28. - 20.),
                "compact timer at {w}"
            );
            // The compact multiplier leads the gate row without touching it.
            let margin = (w * 0.035).clamp(18.0, 44.0);
            assert!(
                margin + text_width("×5", modern(16., INK)) + 8. <= compact_gate_x(w, &next),
                "{w}"
            );
        }
    }

    #[test]
    fn results_card_fits_every_reviewed_viewport() {
        for (w, h) in [
            (1280., 800.),
            (1600., 1000.),
            (390., 844.),
            (844., 390.),
            (320., 480.),
        ] {
            let (card, _) = finish_layout(w, h);
            assert!(card.x >= 8. && card.x + card.w <= w - 8.);
            assert!(card.y >= 8. && card.y + card.h <= h - 8.);
            // The card never reaches into the menu corner.
            assert!(card.y >= MENU_CORNER.1 || card.x + card.w <= w - MENU_CORNER.0 || h < 400.);
        }
    }
}
