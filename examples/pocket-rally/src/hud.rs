//! Screen-space HUD, banners, countdown, pause and results cards.
//!
//! Nothing here advances the race or writes records. Text uses the shared
//! checked bitmap font metrics so every label is measured, fitted and kept
//! inside its panel at desktop, portrait and short landscape sizes.
use crate::juice::{AMBER, CREAM, GOLD, GREEN, Juice, MUTED, RED, WATER};
use crate::presentation::{hint_for, text_width};
use crate::quips;
use crate::session::RaceSession;
use crate::sim::{LAPS, Pose, Race, RoadCue, Track};
use crate::style::Style;
use ferravex_play::gameplay::trial::Medal;
use ferravex_render::input::{HintDevice, PlayerInput};
use ferravex_render::prelude::*;

pub const INK: Color = Color::new(0.12, 0.19, 0.17, 1.);
/// The top bar alpha. Browser glyph tests locate the bar by this exact colour.
const BAR: Color = Color::new(0.12, 0.19, 0.17, 0.78);
/// Countdown and results panels. Browser glyph tests locate them by colour.
const PANEL: Color = Color::new(0.12, 0.19, 0.17, 0.94);
const CHIP: Color = Color::new(0.12, 0.19, 0.17, 0.86);
const LAMP_OFF: Color = Color::new(0.24, 0.27, 0.25, 1.);
const LAMP_RED: Color = Color::new(0.95, 0.30, 0.24, 1.);

/// Stopwatch text from whole centiseconds, so 59.999 s never reads "00:60.00".
pub fn clock(seconds: f32) -> String {
    // Round to whole milliseconds first: accumulated f32 tick time can land a
    // hair under an exact value, which must not read one centisecond short.
    let millis = (f64::from(seconds.max(0.)) * 1000.).round() as u64;
    let centis = millis / 10;
    format!(
        "{:02}:{:02}.{:02}",
        centis / 6000,
        centis / 100 % 60,
        centis % 100
    )
}

pub fn fade(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0., 1.))
}

pub fn text(s: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text(
        s,
        x + 1.,
        y + 1.,
        size,
        Color::new(0.05, 0.1, 0.08, 0.35 * color.a),
    );
    draw_text(s, x, y, size, color);
}

/// Centered text, scaled down only when it would exceed `max_width`.
pub fn text_centered(s: &str, cx: f32, y: f32, size: f32, max_width: f32, color: Color) {
    let width = text_width(s, size);
    let fit = (max_width / width.max(1.)).min(1.);
    text(s, cx - width * fit / 2., y, size * fit, color);
}

/// Device-aware labels for on-screen hints, read from the live bindings.
pub struct Hints {
    pub gas: String,
    pub drift: String,
    pub restart: String,
    pub pause: String,
    pub touch: bool,
    pub keyboard: bool,
}

impl Hints {
    pub fn new(input: &PlayerInput) -> Self {
        let device = input.current_device();
        Self {
            gas: hint_for(input, "move_forward", device),
            drift: hint_for(input, "brake", device),
            restart: hint_for(input, "restart", device),
            pause: hint_for(input, "pause", device),
            touch: device == HintDevice::Touch,
            keyboard: device == HintDevice::KeyboardMouse,
        }
    }
}

/// The world-to-screen transform of the current frame.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub width: f32,
    pub height: f32,
    pub center: Vec2,
    pub scale: f32,
}

impl View {
    pub fn screen(self, p: Vec2) -> Vec2 {
        (p - self.center) * self.scale + vec2(self.width, self.height) * 0.5
    }
}

/// Top bar geometry: wide screens use two rows, narrow screens three.
pub fn bar_height(width: f32) -> f32 {
    if width >= 640. { 66. } else { 80. }
}

fn speed_kmh(race: &Race) -> u32 {
    if race.finished() {
        0
    } else {
        (race.car.velocity.length() * 0.48).round() as u32
    }
}

/// Browser fullscreen pins a 44px menu button at the top-right corner (inset
/// 8px). HUD text and panels keep this `width - 72` by 60 region clear.
pub const MENU_CORNER: Vec2 = vec2(72., 60.);

/// One positioned HUD label; `y` is the text baseline.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub size: f32,
    pub color: Color,
}

impl Label {
    fn at(text: String, x: f32, y: f32, size: f32, color: Color) -> Self {
        Self {
            text,
            x,
            y,
            size,
            color,
        }
    }

    fn right(text: String, right: f32, y: f32, size: f32, color: Color) -> Self {
        let x = right - text_width(&text, size);
        Self::at(text, x, y, size, color)
    }

    fn centered(text: String, cx: f32, y: f32, size: f32, max_width: f32, color: Color) -> Self {
        let fit = (max_width / text_width(&text, size).max(1.)).min(1.);
        let size = size * fit;
        let x = cx - text_width(&text, size) / 2.;
        Self::at(text, x, y, size, color)
    }

    /// Glyph bounds including the one-pixel shadow.
    #[cfg(test)]
    pub fn bounds(&self) -> Rect {
        Rect::new(
            self.x,
            self.y - self.size,
            text_width(&self.text, self.size) + 1.,
            self.size + 1.,
        )
    }
}

/// The bar's texts. Cream labels are lap, speed, lap time, gates and race time
/// in that order; the browser glyph test reads exactly those five runs. The
/// right-hand cluster ends at `width - 72` to keep the menu corner clear.
#[allow(clippy::too_many_arguments)]
pub fn bar_labels(
    width: f32,
    lap: String,
    speed: String,
    lap_time: String,
    gates: String,
    race_time: String,
    delta: Option<(String, Color)>,
) -> Vec<Label> {
    // Two pixels of air so even the text shadow stays out of the corner.
    let right = width - MENU_CORNER.x - 2.;
    if width >= 640. {
        let (delta, color) = delta.unwrap_or_else(|| ("LAP TIME".to_owned(), MUTED));
        vec![
            Label::at(lap, 18., 31., 22., CREAM),
            Label::right(speed, right, 27., 16., CREAM),
            Label::centered(lap_time, width / 2., 34., 28., width * 0.3, CREAM),
            Label::at(gates, 18., 55., 13., CREAM),
            Label::right(race_time, right, 57., 14., CREAM),
            Label::centered(delta, width / 2., 56., 14., width * 0.3, color),
        ]
    } else {
        let lap_right = 12. + text_width(&lap_time, 20.);
        let mut labels = vec![
            Label::at(lap, 12., 24., 16., CREAM),
            Label::right(speed, right, 24., 13., CREAM),
            Label::at(lap_time, 12., 49., 20., CREAM),
            Label::at(gates, 12., 74., 12., CREAM),
            // Row three sits below the corner, so it may use the full width.
            Label::right(race_time, width - 12., 74., 12., CREAM),
        ];
        // The delta shares row two with the lap timer; shrink or drop it
        // rather than letting it reach the menu corner.
        if let Some((text, color)) = delta
            && let Some(size) = [14., 12.]
                .into_iter()
                .find(|&size| lap_right + 10. + text_width(&text, size) <= right)
        {
            labels.push(Label::at(text, lap_right + 10., 48., size, color));
        }
        labels
    }
}

/// Lap, speed, lap time, gates and race time, drawn in that order in cream.
/// The browser glyph test reads exactly these five runs inside the bar.
pub fn top_bar(race: &Race, juice: &Juice, width: f32) {
    let bar = bar_height(width);
    draw_rectangle(0., 0., width, bar, BAR);
    draw_rectangle(0., bar, width, 2., Color::new(0.05, 0.09, 0.08, 0.35));
    // After the finish the lap timer has reset; show the final lap instead.
    let lap_time = clock(if race.finished() {
        race.last_lap.unwrap_or(0.)
    } else {
        race.lap_time
    });
    let labels = bar_labels(
        width,
        format!("LAP {}/{LAPS}", (race.laps + 1).min(LAPS)),
        format!("{} KM/H", speed_kmh(race)),
        lap_time,
        format!("GATE {}/16", race.checkpoints_passed()),
        format!("RACE {}", clock(race.total_time)),
        juice.last_split.map(crate::juice::split_label),
    );
    for label in &labels {
        text(&label.text, label.x, label.y, label.size, label.color);
    }
    // Speed bar under the speed label: amber fill on a dark track.
    let right = width - MENU_CORNER.x - 2.;
    let (bar_w, y, h) = if width >= 640. {
        (120., 34., 4.)
    } else {
        ((width * 0.2).min(90.), 30., 3.)
    };
    let fraction = speed_kmh(race) as f32 / (265. * 0.48);
    draw_rectangle(
        right - bar_w,
        y,
        bar_w,
        h,
        Color::new(0.05, 0.08, 0.07, 0.6),
    );
    draw_rectangle(right - bar_w, y, bar_w * fraction, h, AMBER);
}

/// The minimap rectangle under the bar, top right.
pub fn minimap_rect(width: f32) -> Rect {
    let (w, h, margin) = if width >= 640. {
        (176., 102., 16.)
    } else {
        ((width * 0.34).clamp(90., 120.), 0., 10.)
    };
    let h = if h == 0. { w * 0.58 } else { h };
    Rect::new(width - margin - w, bar_height(width) + 10., w, h)
}

pub fn minimap(track: &Track, race: &Race, ghost: Option<Pose>, rect: Rect, best: Option<String>) {
    let caption = if best.is_some() { 18. } else { 0. };
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w,
        rect.h + caption,
        Color::new(0.12, 0.19, 0.17, 0.66),
    );
    let pad = 7.;
    let scale = ((rect.w - pad * 2.) / 1000.).min((rect.h - pad * 2.) / 560.);
    let origin = rect.center() - vec2(555., 320.) * scale;
    let map = |p: Vec2| origin + p * scale;
    let n = track.points.len();
    for i in (0..n).step_by(2) {
        let a = map(track.points[i]);
        let b = map(track.points[(i + 2) % n]);
        draw_line(a.x, a.y, b.x, b.y, 3., Color::new(0.80, 0.73, 0.57, 0.9));
    }
    let (start, tangent) = track.gate(0);
    let normal = vec2(-tangent.y, tangent.x) * 5.;
    let s = map(start);
    draw_line(
        s.x - normal.x,
        s.y - normal.y,
        s.x + normal.x,
        s.y + normal.y,
        2.,
        INK,
    );
    if !race.finished() {
        let gate = map(track.gate(race.next_gate).0);
        draw_circle_lines(gate.x, gate.y, 4., 1.5, AMBER);
    }
    if let Some(ghost) = ghost {
        let g = map(ghost.position);
        draw_circle(g.x, g.y, 3., Color::new(0.98, 0.92, 0.62, 0.75));
    }
    let car = map(race.car.pose.position);
    draw_circle(car.x, car.y, 4.5, Color::new(0.98, 0.95, 0.88, 0.95));
    draw_circle(car.x, car.y, 3., Color::new(0.85, 0.27, 0.17, 1.));
    if let Some(best) = best {
        let size = if rect.w > 150. { 12. } else { 10. };
        let fit = ((rect.w - 10.) / text_width(&best, size).max(1.)).min(1.);
        text_centered(
            &best,
            rect.x + rect.w / 2.,
            rect.y + rect.h + 13.,
            size * fit,
            rect.w - 10.,
            MUTED,
        );
    }
}

/// Banner below the bar: off-track and wrong-way guidance outrank toasts.
pub fn banner(
    race: &Race,
    track: &Track,
    session: &RaceSession,
    style: &Style,
    juice: &Juice,
    autoplay_reversing: bool,
    width: f32,
) {
    if race.finished() || session.countdown() > 0 {
        return;
    }
    let wrong_way =
        race.car.velocity.length() > 20. && track.road_cue(&race.car) == RoadCue::WrongWay;
    // Secrets (Alert toasts) briefly outrank the off-track guidance.
    let alert = juice
        .toast
        .as_ref()
        .filter(|toast| toast.priority == crate::juice::Priority::Alert);
    let (head, sub, color, sub_color, alpha) = if let Some(toast) = alert {
        (
            toast.head.clone(),
            toast.sub.clone(),
            toast.color,
            toast.sub_color,
            toast.alpha(),
        )
    } else if session.off_track() {
        (
            quips::off_track(style.stats.off_track_episodes.max(1)).to_owned(),
            "FOLLOW THE AMBER ARROW".to_owned(),
            AMBER,
            CREAM,
            1.,
        )
    } else if wrong_way && !autoplay_reversing {
        (
            "WRONG WAY!".to_owned(),
            quips::wrong_way(style.stats.wrong_way_episodes.max(1)).to_owned(),
            RED,
            CREAM,
            1.,
        )
    } else if let Some(toast) = &juice.toast {
        (
            toast.head.clone(),
            toast.sub.clone(),
            toast.color,
            toast.sub_color,
            toast.alpha(),
        )
    } else if autoplay_reversing {
        (
            "AUTO RECOVERY".to_owned(),
            "REVERSING".to_owned(),
            MUTED,
            CREAM,
            1.,
        )
    } else {
        return;
    };
    let wide = width >= 640.;
    let map = minimap_rect(width);
    let top = banner_top(width);
    let (_, max_width) = if wide {
        (
            bar_height(width) + 14.,
            (width - 2. * (map.w + 40.)).max(200.),
        )
    } else {
        (map.y + map.h + 30., width - 24.)
    };
    let (head_size, sub_size) = if wide { (20., 14.) } else { (16., 12.) };
    let content = text_width(&head, head_size)
        .max(text_width(&sub, sub_size))
        .min(max_width - 24.);
    let panel_w = content + 28.;
    let panel_h = if sub.is_empty() { 34. } else { 54. };
    let x = (width - panel_w) / 2.;
    // Slide down a few pixels as it appears (static under reduced motion).
    let y = top - (1. - alpha) * 6. * juice.motion();
    draw_rectangle(x, y, panel_w, panel_h, fade(CHIP, alpha));
    draw_rectangle(x, y, 3., panel_h, fade(color, alpha));
    text_centered(
        &head,
        width / 2.,
        y + 24.,
        head_size,
        content,
        fade(color, alpha),
    );
    if !sub.is_empty() {
        text_centered(
            &sub,
            width / 2.,
            y + 44.,
            sub_size,
            content,
            fade(sub_color, alpha),
        );
    }
}

/// Centered text on a soft dark backing, legible over grass and asphalt.
fn backed_text(label: &str, at: Vec2, size: f32, max_width: f32, color: Color, alpha: f32) {
    let fit = (max_width / text_width(label, size).max(1.)).min(1.);
    let size = size * fit;
    let w = text_width(label, size) + 12.;
    draw_rectangle(
        at.x - w / 2.,
        at.y - size - 1.,
        w,
        size + 7.,
        fade(Color::new(0.08, 0.13, 0.11, 0.5), alpha),
    );
    text_centered(label, at.x, at.y, size, max_width, fade(color, alpha));
}

/// Rising world-space score pops and the live drift counter.
pub fn pops(juice: &Juice, style: &Style, race: &Race, view: &View) {
    for pop in juice.pops.iter().flatten() {
        let t = pop.age / pop.life;
        let rise = 34. * t * juice.motion().min(1.);
        let grow = if juice.motion() > 0. {
            1. + 0.35 * (1. - (pop.age / 0.12).min(1.))
        } else {
            1.
        };
        let at = view.screen(pop.at) - vec2(0., rise);
        let alpha = 1. - t * t;
        backed_text(
            &pop.text,
            at,
            pop.size * grow,
            view.width - 16.,
            pop.color,
            alpha,
        );
    }
    let live = style.live_drift();
    if live > 0 && !race.finished() {
        let at = view.screen(race.car.pose.position) + vec2(0., -42.);
        let multiplier = style.next_multiplier();
        let label = if multiplier > 1 {
            format!("DRIFT {live} x{multiplier}")
        } else {
            format!("DRIFT {live}")
        };
        backed_text(&label, at, 18., view.width - 16., GOLD, 1.);
    }
}

/// Short onboarding lessons that disappear once each is learned.
pub fn coach(
    race: &Race,
    session: &RaceSession,
    style: &Style,
    hints: &Hints,
    attempt: u32,
) -> Option<String> {
    if race.finished() || session.countdown() > 0 || session.off_track() {
        return None;
    }
    if race.ticks > 150 && race.ticks < 600 && race.car.velocity.length() < 15. {
        return Some(format!("HOLD {} TO DRIVE!", hints.gas.to_uppercase()));
    }
    if attempt == 0 && style.stats.drifts == 0 && race.laps < 2 && race.ticks > 240 {
        return Some(format!(
            "TIP: HOLD {} IN BENDS TO DRIFT FOR STYLE",
            hints.drift.to_uppercase()
        ));
    }
    None
}

pub fn coach_chip(label: &str, width: f32, height: f32, touch: bool) {
    let size = if width >= 640. { 15. } else { 12. };
    let w = (text_width(label, size) + 24.).min(width - 16.);
    let y = height - if touch { 168. } else { 48. };
    draw_rectangle((width - w) / 2., y, w, 28., CHIP);
    text_centered(label, width / 2., y + 19., size, w - 16., GOLD);
}

/// Countdown card: exactly two cream labels (title and number) inside the
/// panel, plus start lamps and a non-cream hint. Browser glyph tests read it.
pub fn countdown(
    session: &RaceSession,
    juice: &Juice,
    hints: &Hints,
    attempt: u32,
    record: Option<String>,
    width: f32,
    height: f32,
) {
    let count = session.countdown();
    let panel = countdown_rect(width, height);
    let (panel_w, top, cx) = (panel.w, panel.y, width / 2.);
    let content = (panel_w - 40.).max(1.);
    draw_rectangle(panel.x, panel.y, panel.w, panel.h, PANEL);
    text_centered(quips::ready(attempt), cx, top + 36., 18., content, CREAM);
    // Each new number pops in; reduced motion keeps it still.
    let within = (session.countdown_progress() * 3.).fract();
    let pop = 1. + 0.25 * (1. - (within / 0.18).min(1.)).powi(2) * juice.motion().min(1.);
    text_centered(&count.to_string(), cx, top + 98., 44. * pop, content, CREAM);
    for i in 0..3 {
        let lit = (3 - count as i32) >= i;
        let x = cx + (i as f32 - 1.) * 30.;
        draw_circle(x, top + 122., 9., Color::new(0.05, 0.08, 0.07, 1.));
        draw_circle(x, top + 122., 7., if lit { LAMP_RED } else { LAMP_OFF });
    }
    let hint = if attempt == 0 && !hints.touch {
        format!(
            "HOLD {} TO DRIVE · {} TO DRIFT",
            hints.gas.to_uppercase(),
            hints.drift.to_uppercase()
        )
    } else if let Some(record) = record {
        record
    } else {
        format!("GOLD MEDAL UNDER {:.0}S", quips::MEDALS[0])
    };
    text_centered(&hint, cx, top + 152., 13., content, GOLD);
}

pub fn go_stamp(juice: &Juice, width: f32, height: f32) {
    let Some(t) = juice.since_go else {
        return;
    };
    let alpha = 1. - ((t - 0.55) / 0.6).clamp(0., 1.);
    let pop = 1. + 0.7 * (1. - (t / 0.22).min(1.)).powi(2) * juice.motion().min(1.);
    text_centered(
        "GO!",
        width / 2.,
        height * 0.42,
        64. * pop,
        width - 32.,
        fade(GREEN, alpha),
    );
}

/// Gold edge glow for a new best lap; cleared under reduced flash.
pub fn flash(juice: &Juice, width: f32, height: f32) {
    if juice.flash <= 0. {
        return;
    }
    for i in 0..4 {
        let inset = i as f32 * 6.;
        let c = fade(GOLD, juice.flash * 0.18 * (1. - i as f32 / 4.));
        draw_rectangle(inset, inset, width - 2. * inset, 6., c);
        draw_rectangle(inset, height - inset - 6., width - 2. * inset, 6., c);
        draw_rectangle(inset, inset + 6., 6., height - 2. * inset - 12., c);
        draw_rectangle(
            width - inset - 6.,
            inset + 6.,
            6.,
            height - 2. * inset - 12.,
            c,
        );
    }
}

pub fn paused(race: &Race, hints: &Hints, seed: u32, width: f32, height: f32) {
    draw_rectangle(0., 0., width, height, Color::new(0.04, 0.07, 0.06, 0.45));
    let Rect { x, y, w, h } = pause_rect(width, height);
    draw_rectangle(x, y, w, h, Color::new(0.12, 0.19, 0.17, 0.96));
    draw_rectangle(x, y, w, 3., AMBER);
    let cx = width / 2.;
    let content = w - 32.;
    text_centered("PAUSED", cx, y + 44., 30., content, CREAM);
    text_centered(quips::pause(seed), cx, y + 74., 14., content, MUTED);
    text_centered(
        &format!(
            "LAP {}/{LAPS}  ·  RACE {}",
            (race.laps + 1).min(LAPS),
            clock(race.total_time)
        ),
        cx,
        y + 100.,
        14.,
        content,
        CREAM,
    );
    let actions = format!(
        "{} RESUME  ·  {} RESTART{}",
        hints.pause.to_uppercase(),
        hints.restart.to_uppercase(),
        if hints.keyboard {
            "  ·  F2 CONTROLS"
        } else {
            ""
        }
    );
    text_centered(&actions, cx, y + 136., 13., content, GOLD);
}

/// Personal-best context captured when the race finished.
#[derive(Clone, Copy, Debug, Default)]
pub struct Results {
    pub previous_best: Option<f32>,
    pub previous_style: Option<u32>,
    pub demo: bool,
}

impl Results {
    pub fn new_best(&self, time: f32) -> bool {
        !self.demo && self.previous_best.is_none_or(|best| time < best)
    }
}

pub fn medal_color(medal: Medal) -> Color {
    match medal {
        Medal::Gold => Color::new(1.0, 0.80, 0.30, 1.),
        Medal::Silver => Color::new(0.80, 0.85, 0.88, 1.),
        Medal::Bronze => Color::new(0.85, 0.55, 0.32, 1.),
        Medal::Unranked => Color::new(0.50, 0.55, 0.50, 1.),
    }
}

pub fn record_line(time: f32, results: &Results) -> (String, Color) {
    if results.demo {
        ("AUTOPLAY DEMO · NOT RECORDED".into(), MUTED)
    } else if let Some(best) = results.previous_best {
        if results.new_best(time) {
            (format!("NEW PERSONAL BEST!  -{:.2}S", best - time), GOLD)
        } else {
            (
                format!("PERSONAL BEST {}  (+{:.2}S)", clock(best), time - best),
                CREAM,
            )
        }
    } else {
        ("FIRST FINISH! THAT'S THE TIME TO BEAT".into(), GOLD)
    }
}

/// Wide screens move the results card right so the car and its confetti
/// stay visible on the left; narrow screens center it.
pub fn results_center_x(width: f32) -> f32 {
    if width >= 1000. {
        (width - 260.).min(width * 0.68)
    } else {
        width / 2.
    }
}

fn card_scale(width: f32, height: f32) -> f32 {
    ((width - 24.) / 440.)
        .min((height - 24.) / 400.)
        .clamp(0.3, 1.)
}

/// Screen point where the finish camera frames the car: left of the card on
/// wide screens, below it on tall narrow screens, otherwise centered.
pub fn finish_framing(width: f32, height: f32) -> Vec2 {
    if width >= 1000. {
        return vec2((results_center_x(width) - 220.) / 2., height / 2.);
    }
    let h = 400. * card_scale(width, height);
    let bottom = ((height - h) / 2.).max(8.) + h;
    if height - bottom >= 110. {
        vec2(width / 2., (bottom + height) / 2.)
    } else {
        vec2(width / 2., height / 2.)
    }
}

pub fn countdown_rect(width: f32, height: f32) -> Rect {
    let (w, h) = ((width - 32.).clamp(1., 360.), 168.);
    let top = (height / 2. - h / 2.).max(bar_height(width) + 8.);
    Rect::new((width - w) / 2., top, w, h)
}

pub fn pause_rect(width: f32, height: f32) -> Rect {
    let (w, h) = ((width - 32.).clamp(1., 380.), 160.);
    Rect::new(
        (width - w) / 2.,
        ((height - h) / 2.).max(MENU_CORNER.y + 4.),
        w,
        h,
    )
}

pub fn results_rect(width: f32, height: f32) -> Rect {
    let k = card_scale(width, height);
    let (w, h) = (440. * k, 400. * k);
    let cx = results_center_x(width);
    Rect::new(cx - w / 2., ((height - h) / 2.).max(8.), w, h)
}

/// Top of the event banner, below the bar (and the minimap when narrow).
pub fn banner_top(width: f32) -> f32 {
    if width >= 640. {
        bar_height(width) + 14.
    } else {
        let map = minimap_rect(width);
        map.y + map.h + 30.
    }
}

/// The podium card. Layout is authored at 440x400 and scaled to fit.
pub fn results(
    race: &Race,
    style: &Style,
    juice: &Juice,
    results: &Results,
    retry: &str,
    width: f32,
    height: f32,
) {
    let intro = if juice.motion() > 0. {
        juice.celebrate
    } else {
        10.
    };
    let alpha = ((intro - 0.5) / 0.3).clamp(0., 1.);
    if alpha <= 0. {
        return;
    }
    let k = card_scale(width, height);
    let Rect { x, y, w, h } = results_rect(width, height);
    let cx = x + w / 2.;
    let content = w - 36. * k;
    draw_rectangle(x, y, w, h, fade(PANEL, alpha));
    let total = race.total_time;
    let medal = quips::medal(total);
    let rank = quips::rank(medal, &style.stats);
    draw_rectangle(x, y, w, 4. * k, fade(medal_color(medal), alpha));
    let at = |dy: f32| y + dy * k;
    text_centered("FINISH!", cx, at(32.), 15. * k, content, fade(GREEN, alpha));
    let stamp = if juice.motion() > 0. {
        1. + 0.5 * (1. - ((intro - 0.6) / 0.2).clamp(0., 1.)).powi(2)
    } else {
        1.
    };
    text_centered(
        rank.title,
        cx,
        at(76.),
        32. * k * stamp,
        content,
        fade(GOLD, alpha),
    );
    text_centered(
        rank.quip,
        cx,
        at(102.),
        13. * k,
        content,
        fade(CREAM, alpha),
    );

    let time = clock(total);
    let size = 36. * k;
    let group = 54. * k + text_width(&time, size);
    let badge = cx - group / 2. + 22. * k;
    draw_circle(badge, at(146.), 22. * k, fade(medal_color(medal), alpha));
    draw_circle_lines(badge, at(146.), 22. * k, 3. * k, fade(INK, alpha * 0.5));
    text_centered(
        &quips::medal_name(medal)[..1],
        badge,
        at(155.),
        24. * k,
        40. * k,
        fade(INK, alpha),
    );
    text(&time, badge + 32. * k, at(160.), size, fade(CREAM, alpha));
    let medal_label = match medal {
        Medal::Unranked => "NO MEDAL YET".to_owned(),
        other => format!("{} MEDAL", quips::medal_name(other)),
    };
    text_centered(
        &medal_label,
        cx,
        at(190.),
        14. * k,
        content,
        fade(medal_color(medal), alpha),
    );
    let (record, record_color) = record_line(total, results);
    text_centered(
        &record,
        cx,
        at(214.),
        14. * k,
        content,
        fade(record_color, alpha),
    );
    let hook = quips::hook(medal, total, &style.stats);
    text_centered(&hook, cx, at(238.), 14. * k, content, fade(GREEN, alpha));

    let stats = &style.stats;
    let style_record = !results.demo && results.previous_style.is_none_or(|s| stats.style > s);
    let cells = [
        ("BEST LAP", race.best_time.map_or("--".into(), clock), CREAM),
        (
            if style_record {
                "STYLE RECORD"
            } else {
                "STYLE"
            },
            stats.style.to_string(),
            if style_record && stats.style > 0 {
                GOLD
            } else {
                CREAM
            },
        ),
        ("BEST DRIFT", stats.best_drift.to_string(), CREAM),
        (
            "CLEAN LAPS",
            format!("{}/{LAPS}", stats.clean_laps),
            if stats.perfect() { GREEN } else { CREAM },
        ),
        ("GRASS", format!("{:.1}S", stats.grass_seconds()), CREAM),
        (
            "DUCKS",
            if stats.splashes > 0 { "BOTHERED" } else { "?" }.to_owned(),
            if stats.splashes > 0 { WATER } else { MUTED },
        ),
    ];
    let cell_w = (w - 24. * k) / 3.;
    for (i, (label, value, color)) in cells.iter().enumerate() {
        let col = (i % 3) as f32;
        let row = (i / 3) as f32;
        let ccx = x + 12. * k + cell_w * (col + 0.5);
        let top = at(270. + row * 46.);
        text_centered(label, ccx, top, 11. * k, cell_w - 8., fade(MUTED, alpha));
        text_centered(
            value,
            ccx,
            top + 20. * k,
            17. * k,
            cell_w - 8.,
            fade(*color, alpha),
        );
    }
    let pulse = if juice.motion() > 0. {
        0.75 + 0.25 * (juice.celebrate * 4.).sin()
    } else {
        1.
    };
    text_centered(
        retry,
        cx,
        at(380.),
        17. * k,
        content,
        fade(GOLD, alpha * pulse),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopwatch_uses_whole_centiseconds_and_rolls_minutes() {
        assert_eq!(clock(0.), "00:00.00");
        assert_eq!(clock(6.00079), "00:06.00");
        assert_eq!(clock(7.), "00:07.00");
        assert_eq!(clock(59.999), "00:59.99");
        assert_eq!(clock(60.), "01:00.00");
        assert_eq!(clock(125.5), "02:05.50");
        assert_eq!(clock(-1.), "00:00.00");
        // Accumulated f32 tick time that lands a hair under a whole value.
        let mut t = 0_f32;
        for _ in 0..840 {
            t += 1. / 120.;
        }
        assert_eq!(clock(t), "00:07.00");
    }

    #[test]
    fn record_lines_never_claim_a_demo_best() {
        let first = Results::default();
        assert!(first.new_best(40.));
        assert!(record_line(40., &first).0.starts_with("FIRST FINISH"));
        let chase = Results {
            previous_best: Some(44.),
            ..Results::default()
        };
        assert_eq!(record_line(43.5, &chase).0, "NEW PERSONAL BEST!  -0.50S");
        assert_eq!(
            record_line(45., &chase).0,
            "PERSONAL BEST 00:44.00  (+1.00S)"
        );
        let demo = Results {
            demo: true,
            ..Results::default()
        };
        assert!(!demo.new_best(1.));
        assert!(record_line(1., &demo).0.contains("NOT RECORDED"));
    }

    #[test]
    fn onboarding_tips_wait_for_go_and_fade_once_learned() {
        use crate::sim::Controls;
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut session = RaceSession::new();
        let mut style = Style::new();
        let hints = Hints {
            gas: "W".into(),
            drift: "Space".into(),
            restart: "R".into(),
            pause: "Escape".into(),
            touch: false,
            keyboard: true,
        };
        assert_eq!(coach(&race, &session, &style, &hints, 0), None, "countdown");
        let drive = |race: &mut Race, session: &mut RaceSession, ticks, throttle| {
            for _ in 0..ticks {
                let frame = session.advance(1. / 120., true);
                for _ in 0..frame.steps {
                    let controls = Controls {
                        throttle,
                        ..Default::default()
                    };
                    session.step(race, &track, controls, frame.dt);
                }
            }
        };
        drive(&mut race, &mut session, 360 + 200, 0.);
        assert_eq!(
            coach(&race, &session, &style, &hints, 0).as_deref(),
            Some("HOLD W TO DRIVE!")
        );
        drive(&mut race, &mut session, 120, 1.);
        let tip = coach(&race, &session, &style, &hints, 0).unwrap();
        assert!(tip.contains("HOLD SPACE IN BENDS"), "{tip}");
        assert_eq!(
            coach(&race, &session, &style, &hints, 1),
            None,
            "retries skip it"
        );
        style.stats.drifts = 1;
        assert_eq!(coach(&race, &session, &style, &hints, 0), None, "learned");
    }

    /// Browser fullscreen pins its 44px menu button in the top-right corner.
    #[test]
    fn hud_text_and_panels_keep_the_fullscreen_menu_corner_clear() {
        let red = Some(("+9.99".to_owned(), RED));
        for (w, h) in [
            (1280., 800.),
            (1600., 1000.),
            (390., 844.),
            (844., 390.),
            (320., 480.),
        ] {
            let corner = Rect::new(w - MENU_CORNER.x, 0., MENU_CORNER.x, MENU_CORNER.y);
            let clear = |r: Rect, what: &str| {
                assert!(
                    !(r.x < corner.x + corner.w
                        && r.x + r.w > corner.x
                        && r.y < corner.y + corner.h
                        && r.y + r.h > corner.y),
                    "{what} {r:?} enters the menu corner at {w}x{h}"
                );
                assert!(r.x >= 0. && r.x + r.w <= w, "{what} {r:?} fits {w}x{h}");
            };
            // Worst-case (widest) bar strings, with and without a split delta.
            for delta in [None, red.clone()] {
                let labels = bar_labels(
                    w,
                    "LAP 3/3".into(),
                    "127 KM/H".into(),
                    "59:59.99".into(),
                    "GATE 15/16".into(),
                    "RACE 59:59.99".into(),
                    delta,
                );
                assert!(labels.len() >= 5);
                for label in &labels {
                    clear(label.bounds(), &label.text);
                    assert!(label.bounds().y >= 0. && label.y <= bar_height(w));
                }
            }
            let map = minimap_rect(w);
            clear(Rect::new(map.x, map.y, map.w, map.h + 18.), "minimap");
            clear(countdown_rect(w, h), "countdown");
            clear(pause_rect(w, h), "pause card");
            clear(results_rect(w, h), "results card");
            assert!(banner_top(w) >= MENU_CORNER.y, "banner");
        }
    }

    #[test]
    fn layout_rectangles_fit_desktop_portrait_and_short_landscape() {
        for (w, h) in [
            (1280., 800.),
            (1600., 1000.),
            (390., 844.),
            (320., 568.),
            (844., 390.),
        ] {
            let map = minimap_rect(w);
            assert!(map.x >= 8. && map.x + map.w <= w - 8.);
            assert!(map.y >= bar_height(w));
            assert!(map.y + map.h + 20. < h);
            let k = ((w - 24.) / 440.).min((h - 24.) / 400.).min(1.);
            assert!(440. * k <= w - 24. + 0.01 && 400. * k <= h - 24. + 0.01);
            let cx = results_center_x(w);
            assert!(cx - 220. * k >= 8. && cx + 220. * k <= w - 8.);
            let car = finish_framing(w, h);
            let card_bottom = ((h - 400. * k) / 2.).max(8.) + 400. * k;
            assert!(car.x < cx - 220. * k || car.y > card_bottom || car == vec2(w, h) / 2.);
        }
    }
}
