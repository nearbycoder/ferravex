//! Wide-layout HUD and Ready/Paused/Won cards in the authored 1280×720 space.
//!
//! The play HUD shows only what matters now: section, lantern pips and the
//! earned air jump on the left; the run clock, its medal pace and a course
//! track in the center; embers, points and an active chain on the right. One
//! toast slot and one onboarding line keep messages from stacking or covering
//! the course.
use super::juice::{GOLD, Juice, MOTH, Toast};
use super::presentation::ControlHint;
use super::quips::{self, RunSummary};
use super::*;
use ferravex_play::gameplay::trial::Medal;

pub(super) const MUTED: Color = Color::new(0.64, 0.67, 0.57, 1.0);
const PANEL: Color = Color::new(0.07, 0.11, 0.10, 0.96);
const SILVER: Color = Color::new(0.80, 0.85, 0.87, 1.0);
const BRONZE: Color = Color::new(0.82, 0.55, 0.32, 1.0);
const TRACK: (f32, f32) = (470., 810.);

/// The browser fullscreen game menu (a 44×44 button inset 8) owns this
/// top-right screen region; HUD text and controls must stay out of it.
pub(super) const MENU_CORNER: Vec2 = Vec2::new(72., 60.);
/// Canvas-space top of the right HUD cluster (ember count ink) and its bottom.
const CLUSTER_TOP: f32 = 29.;

/// Wide-layout placements that respect [`MENU_CORNER`] for the current screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct WideLayout {
    /// Right edge of the ember/points cluster, in canvas units.
    pub right: f32,
    /// The default touch pause button, in canvas units.
    pub pause: Rect,
}

impl WideLayout {
    /// Letterboxed screens keep the authored symmetric layout; when the canvas
    /// reaches the top edge, the cluster is inset and the pause button lowered
    /// so neither sits beneath the menu button.
    pub fn new(canvas: ferravex_render::canvas::Canvas2D, screen: Vec2) -> Self {
        let corner_x = canvas.unproject(vec2(screen.x - MENU_CORNER.x - 4., 0.)).x;
        let below = |y: f32| canvas.project(vec2(0., y)).y >= MENU_CORNER.y + 4.;
        let right = if below(CLUSTER_TOP) {
            WIDTH - 32.
        } else {
            (WIDTH - 32.).min(corner_x)
        };
        let pause_y = if below(104.) {
            104.
        } else {
            canvas.unproject(vec2(0., MENU_CORNER.y + 4.)).y
        };
        let pause_x = if below(pause_y) {
            WIDTH - 112.
        } else {
            (WIDTH - 112.).min(corner_x - 96.)
        };
        Self {
            right,
            pause: Rect::new(pause_x, pause_y, 96., 56.),
        }
    }
}

/// The visible card, which is also the default touch confirm region.
pub(super) fn card_rect(state: SessionState) -> Rect {
    if state == SessionState::Won {
        Rect::new(250., 104., 780., 476.)
    } else {
        Rect::new(290., 150., 700., 380.)
    }
}

/// Result of the finished run against stored records, captured once at the finish.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Results {
    /// The fastest stored clear before this run, in seconds.
    pub previous_best: Option<f64>,
    /// Autoplay demonstrations are shown but never recorded.
    pub demo: bool,
}

impl Results {
    pub fn new_best(&self, time: f64) -> bool {
        !self.demo && self.previous_best.is_none_or(|best| time < best - 1e-6)
    }
}

pub(super) fn medal_color(medal: Medal) -> Color {
    match medal {
        Medal::Gold => GOLD,
        Medal::Silver => SILVER,
        Medal::Bronze => BRONZE,
        Medal::Unranked => MUTED,
    }
}

/// The medal the running clock would still earn.
pub(super) fn pace(elapsed: f64) -> Medal {
    let t = elapsed as f32;
    if t <= MEDALS[0] {
        Medal::Gold
    } else if t <= MEDALS[1] {
        Medal::Silver
    } else if t <= MEDALS[2] {
        Medal::Bronze
    } else {
        Medal::Unranked
    }
}

pub(super) fn summary(game: &Game) -> RunSummary {
    RunSummary {
        medal: game.trial.medal().unwrap_or(Medal::Unranked),
        falls: game.deaths,
        embers: game.embers.iter().filter(|(_, found)| *found).count(),
        total_embers: game.embers.len(),
        close_calls: game.close_calls,
        secret: game.secret,
    }
}

pub(super) fn zone(game: &Game) -> usize {
    match game.position().x {
        x if x < 900. => 0,
        x if x < 1800. => 1,
        _ => 2,
    }
}

pub(super) fn fade(color: Color, alpha: f32) -> Color {
    Color::new(color.r, color.g, color.b, color.a * alpha)
}

pub(super) fn right_label(text: &str, right: f32, y: f32, size: f32, color: Color) {
    label(text, right - text_width(text, size), y, size, color);
}

/// Small lantern icon used in the HUD and on the start card.
pub(super) fn lantern_icon(x: f32, y: f32, lit: bool) {
    let color = if lit { GOLD } else { MUTED };
    draw_triangle(
        vec2(x - 8., y - 9.),
        vec2(x + 8., y - 9.),
        vec2(x, y - 15.),
        color,
    );
    draw_rectangle(x - 7., y - 9., 14., 16., Color::new(0.10, 0.14, 0.12, 1.));
    draw_rectangle_lines(x - 7., y - 9., 14., 16., 2., color);
    if lit {
        diamond(vec2(x, y - 1.), 5., ORANGE);
    }
}

/// The one-line coaching hint for a first-time player, or `None` once learned.
pub(super) fn coach(game: &Game, jump: &str) -> Option<String> {
    let x = game.position().x;
    let embers = game.ember_inventory.count(1);
    if game.jumps == 0 && x < 600. {
        Some(format!("{jump}: JUMP  ·  HOLD IT FOR A BIGGER LEAP"))
    } else if game.checkpoints.index() == 0 && (1000. ..1500.).contains(&x) {
        Some("LAND BY THE LANTERN TO SAVE YOUR SPOT".into())
    } else if embers == 2 {
        Some("ONE MORE EMBER UNLOCKS AN AIR JUMP".into())
    } else {
        None
    }
}

fn pill(center: f32, y: f32, w: f32, h: f32, alpha: f32) {
    draw_rectangle(center - w * 0.5, y, w, h, fade(PANEL, alpha));
    draw_rectangle(center - w * 0.5, y, w, 2., fade(ORANGE, 0.7 * alpha));
}

pub(super) fn toast(toast: &Toast, center: f32, y: f32, max_width: f32) {
    let alpha = toast.alpha();
    let head = 24.;
    let sub = 16.;
    let w = text_width(&toast.head, head)
        .max(text_width(&toast.sub, sub))
        .min(max_width)
        + 48.;
    pill(
        center,
        y,
        w,
        if toast.sub.is_empty() { 44. } else { 66. },
        alpha,
    );
    fitted_centered_label(
        &toast.head,
        center,
        y + 32.,
        w - 32.,
        head,
        fade(toast.color, alpha),
    );
    if !toast.sub.is_empty() {
        fitted_centered_label(
            &toast.sub,
            center,
            y + 56.,
            w - 32.,
            sub,
            fade(CREAM, alpha),
        );
    }
}

/// Text pops and expanding rings, in world space.
pub(super) fn world_effects(juice: &Juice, camera: f32) {
    for ring in juice.rings.iter().flatten() {
        let t = ring.age / ring.life;
        draw_circle_lines(
            ring.at.x - camera,
            ring.at.y,
            ring.radius * (0.3 + 0.7 * t.sqrt()),
            3. * (1. - t) + 1.,
            fade(ring.color, (1. - t) * 0.8),
        );
    }
    for pop in juice.pops.iter().flatten() {
        let t = pop.age / pop.life;
        let rise = 46. * (1. - (1. - t).powi(2)) * juice.motion();
        let grow = 1. + 0.35 * (1. - (pop.age / 0.1).min(1.)) * juice.motion();
        let alpha = ((1. - t) / 0.35).min(1.);
        let x = (pop.at.x - camera).clamp(70., WIDTH - 70.);
        centered_label(
            &pop.text,
            x,
            pop.at.y - rise,
            pop.size * grow,
            fade(pop.color, alpha),
        );
    }
}

pub(super) struct Hints {
    pub confirm: ControlHint,
    pub pause: ControlHint,
    pub restart: ControlHint,
    pub jump: ControlHint,
    pub left: ControlHint,
    pub right: ControlHint,
    pub tap_confirm: bool,
    pub keyboard: bool,
}

impl Hints {
    pub fn new(input: &PlayerInput, touch_mode: bool) -> Self {
        Self {
            confirm: ControlHint::new(input, "confirm"),
            pause: ControlHint::new(input, "pause"),
            restart: ControlHint::new(input, "restart"),
            jump: ControlHint::new(input, "jump"),
            left: ControlHint::new(input, "move_left"),
            right: ControlHint::new(input, "move_right"),
            tap_confirm: touch_mode
                && !input.touch_customized()
                && presentation::touch_button_matches(input, "confirm", 10),
            keyboard: input.current_device() == HintDevice::KeyboardMouse,
        }
    }
}

/// The in-run HUD. `veteran` players (a stored clear) skip onboarding lines.
pub(super) fn play(game: &Game, juice: &Juice, hints: &Hints, veteran: bool, cluster: f32) {
    // A soft band unifies the HUD over sky, mountains and the sun.
    draw_rectangle(
        -400.,
        -200.,
        WIDTH + 800.,
        296.,
        Color::new(0.06, 0.09, 0.08, 0.5),
    );
    draw_rectangle(
        -400.,
        96.,
        WIDTH + 800.,
        2.,
        Color::new(0.55, 0.53, 0.38, 0.25),
    );

    // Left: where am I, what have I saved, what can I do.
    label(ZONES[zone(game)], 32., 42., 16., CREAM);
    for index in 1..=2 {
        lantern_icon(
            42. + (index - 1) as f32 * 24.,
            74.,
            game.checkpoints.index() >= index,
        );
    }
    if game.ember_inventory.count(1) >= 3 {
        let (text, color) = if game.air_jump_used {
            ("AIR JUMP USED", MUTED)
        } else {
            ("AIR JUMP READY", GREEN)
        };
        label(text, 86., 80., 16., color);
    } else {
        label(
            &format!("AIR JUMP {}/3", game.ember_inventory.count(1)),
            86.,
            80.,
            16.,
            MUTED,
        );
    }

    // Center: the clock in its medal-pace color, with a course track below.
    let elapsed = game.trial.elapsed();
    let medal = pace(elapsed);
    let clock = format!("{elapsed:.2}");
    centered_label(&clock, WIDTH * 0.5, 50., 32., medal_color(medal));
    let badge_x = WIDTH * 0.5 + text_width(&clock, 32.) * 0.5 + 24.;
    if medal != Medal::Unranked {
        draw_circle(badge_x, 37., 13., medal_color(medal));
        draw_text(
            &quips::medal_name(medal)[..1],
            badge_x - 5.,
            44.,
            16.,
            Color::new(0.12, 0.11, 0.08, 1.),
        );
    }
    let (left, right) = TRACK;
    let at = |x: f32| left + (right - left) * (x / LIFT_X).clamp(0., 1.);
    draw_rectangle(
        left,
        70.,
        right - left,
        3.,
        Color::new(0.55, 0.55, 0.45, 0.5),
    );
    for index in 1..=2 {
        let x = at(lantern_position(index).x);
        let lit = game.checkpoints.index() >= index;
        draw_rectangle(x - 4., 66., 8., 11., if lit { GOLD } else { MUTED });
    }
    draw_rectangle(right - 3., 62., 6., 18., GREEN);
    draw_triangle(
        vec2(right + 3., 62.),
        vec2(right + 15., 67.),
        vec2(right + 3., 72.),
        GREEN,
    );
    draw_rectangle(
        left,
        70.,
        at(game.position().x) - left,
        3.,
        fade(ORANGE, 0.85),
    );
    draw_circle(at(game.position().x + PLAYER.x * 0.5), 71.5, 6., ORANGE);

    // Right: embers, then points and any live chain.
    let collected = game.embers.iter().filter(|(_, found)| *found).count();
    let embers = format!("{collected}/{}", game.embers.len());
    right_label(&embers, cluster, 50., 24., CREAM);
    diamond(
        vec2(cluster - 20. - text_width(&embers, 24.), 41.),
        10.,
        ORANGE,
    );
    let points = format!("{} PTS", game.score.total());
    right_label(&points, cluster, 80., 16., ORANGE);
    if let Some(life) = game
        .score
        .remaining_life()
        .filter(|_| game.score.multiplier() > 1)
    {
        let chain = format!("CHAIN ×{}", game.score.multiplier());
        let right = cluster - 16. - text_width(&points, 16.);
        right_label(&chain, right, 80., 16., GREEN);
        let w = text_width(&chain, 16.);
        draw_rectangle(
            right - w,
            86.,
            w * (life as f32 / 5.).clamp(0., 1.),
            3.,
            GREEN,
        );
    }

    if let Some(toast_state) = &juice.toast {
        toast(toast_state, WIDTH * 0.5, 112., 760.);
    }
    if !veteran
        && juice.toast.is_none()
        && let Some(text) = coach(game, hints.jump.text())
    {
        let w = text_width(&text, 16.).min(700.) + 40.;
        pill(WIDTH * 0.5, 664., w, 36., 1.);
        fitted_centered_label(&text, WIDTH * 0.5, 688., w - 24., 16., CREAM);
    }
}

fn panel(rect: Rect, alpha: f32) {
    draw_rectangle(
        rect.x + 6.,
        rect.y + 8.,
        rect.w,
        rect.h,
        fade(Color::new(0., 0., 0., 0.35), alpha),
    );
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, fade(PANEL, alpha));
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        2.,
        fade(Color::new(0.38, 0.45, 0.36, 1.), alpha),
    );
    draw_rectangle(rect.x, rect.y, rect.w, 4., fade(ORANGE, alpha));
}

/// Draws a column of (label, value) stats evenly across `width`.
fn stats(stats: &[(&str, String, Color)], center: f32, width: f32, y: f32, alpha: f32) {
    let step = width / stats.len() as f32;
    for (i, (name, value, color)) in stats.iter().enumerate() {
        let x = center - width * 0.5 + step * (i as f32 + 0.5);
        fitted_centered_label(name, x, y, step - 12., 16., fade(MUTED, alpha));
        fitted_centered_label(value, x, y + 32., step - 12., 24., fade(*color, alpha));
    }
}

fn call_to_action(text: &str, y: f32, juice: &Juice, alpha: f32) {
    let pulse = 0.92 + 0.08 * (juice.time * 4.).sin() * juice.motion().min(1.);
    fitted_centered_label(text, WIDTH * 0.5, y, 640., 24., fade(GREEN, pulse * alpha));
}

pub(super) fn ready(hints: &Hints, best: Option<(f64, Medal)>, juice: &Juice) {
    let rect = card_rect(SessionState::Ready);
    panel(rect, 1.);
    let c = WIDTH * 0.5;
    centered_label("EMBERLINE", c, rect.y + 64., 40., CREAM);
    centered_label(
        "A LANTERN, A LONG NIGHT, SUSPICIOUSLY MANY SPIKES",
        c,
        rect.y + 94.,
        16.,
        MUTED,
    );
    let goals = [
        ("LIGHT 2 LANTERNS", "TO SAVE YOUR SPOT"),
        ("GRAB EMBERS", "3 = AIR JUMP"),
        ("RIDE THE LIFT", "UNDER 12S = GOLD"),
    ];
    for (i, (head, sub)) in goals.iter().enumerate() {
        let x = c + (i as f32 - 1.) * 220.;
        let icon_y = rect.y + 140.;
        match i {
            0 => {
                lantern_icon(x - 12., icon_y + 4., false);
                lantern_icon(x + 12., icon_y + 4., true);
            }
            1 => diamond(vec2(x, icon_y), 12., ORANGE),
            _ => {
                draw_rectangle_lines(x - 12., icon_y - 14., 24., 28., 3., GREEN);
                draw_triangle(
                    vec2(x - 4., icon_y - 5.),
                    vec2(x + 5., icon_y),
                    vec2(x - 4., icon_y + 5.),
                    GREEN,
                );
            }
        }
        centered_label(head, x, rect.y + 186., 16., CREAM);
        centered_label(sub, x, rect.y + 208., 16., MUTED);
    }
    fitted_centered_label(
        &format!(
            "MOVE {} · {}   JUMP {} (HOLD = HIGHER)",
            hints.left.text(),
            hints.right.text(),
            hints.jump.text()
        ),
        c,
        rect.y + 252.,
        rect.w - 48.,
        16.,
        CREAM,
    );
    if let Some((time, medal)) = best {
        centered_label(
            &format!("YOUR BEST {time:.2}S · {}", quips::medal_name(medal)),
            c,
            rect.y + 282.,
            16.,
            medal_color(medal),
        );
    } else {
        centered_label(
            "GOLD 12S · SILVER 18S · BRONZE 30S",
            c,
            rect.y + 282.,
            16.,
            MUTED,
        );
    }
    call_to_action(
        &if hints.tap_confirm {
            "TAP HERE TO START".to_owned()
        } else {
            format!("PRESS {} TO START", hints.confirm.text())
        },
        rect.y + 340.,
        juice,
        1.,
    );
}

pub(super) fn paused(game: &Game, hints: &Hints, juice: &Juice) {
    let rect = card_rect(SessionState::Paused);
    panel(rect, 1.);
    let c = WIDTH * 0.5;
    centered_label("PAUSED", c, rect.y + 72., 40., CREAM);
    centered_label(
        quips::pause(game.deaths + game.jumps),
        c,
        rect.y + 104.,
        16.,
        MUTED,
    );
    let collected = game.embers.iter().filter(|(_, found)| *found).count();
    stats(
        &[
            (
                "TIME",
                format!("{:.2}S", game.trial.elapsed()),
                medal_color(pace(game.trial.elapsed())),
            ),
            (
                "EMBERS",
                format!("{collected}/{}", game.embers.len()),
                ORANGE,
            ),
            ("LANTERNS", format!("{}/2", game.checkpoints.index()), GOLD),
            ("FALLS", game.deaths.to_string(), CREAM),
        ],
        c,
        rect.w - 60.,
        rect.y + 156.,
        1.,
    );
    call_to_action(
        &if hints.tap_confirm {
            "TAP HERE TO RESUME".to_owned()
        } else {
            format!(
                "{} / {} TO RESUME",
                hints.confirm.text(),
                hints.pause.text()
            )
        },
        rect.y + 276.,
        juice,
        1.,
    );
    fitted_centered_label(
        &if hints.keyboard {
            format!("{} RESTART  ·  F2 CONTROLS", hints.restart.text())
        } else {
            format!("{} RESTART", hints.restart.text())
        },
        c,
        rect.y + 320.,
        rect.w - 48.,
        16.,
        ORANGE,
    );
}

/// The hook line under the result: next medal, PB chase, or a secret nudge.
pub(super) fn hook(game: &Game) -> (String, Color) {
    let run = summary(game);
    let time = game.trial.elapsed() as f32;
    if let Some((name, target)) = quips::next_medal(run.medal, MEDALS) {
        (format!("{:.2}S FASTER FOR {name}!", time - target), GREEN)
    } else if run.embers < run.total_embers {
        (
            format!("GOLD! NOW TRY ALL {} EMBERS", run.total_embers),
            GREEN,
        )
    } else if !run.secret {
        ("PSST... SOMETHING FLUTTERS ABOVE THE FURNACE".into(), MOTH)
    } else if run.falls > 0 {
        ("NOW DO IT WITHOUT FALLING. NO PRESSURE.".into(), GREEN)
    } else {
        ("NOTHING LEFT TO PROVE. EXCEPT SPEED.".into(), GREEN)
    }
}

pub(super) fn record_line(game: &Game, results: &Results) -> (String, Color) {
    let time = game.trial.elapsed();
    if results.demo {
        ("DEMO RUN · NOT RECORDED".into(), MUTED)
    } else if let Some(best) = results.previous_best {
        if results.new_best(time) {
            (format!("NEW PERSONAL BEST! -{:.2}S", best - time), GOLD)
        } else {
            (
                format!("PERSONAL BEST {best:.2}S  (+{:.2}S)", time - best),
                CREAM,
            )
        }
    } else {
        ("FIRST CLEAR! THAT'S YOUR TIME TO BEAT".into(), GOLD)
    }
}

pub(super) fn won(game: &Game, hints: &Hints, juice: &Juice, results: &Results) {
    // Let the lift rise and the confetti fly before the card settles in.
    let intro = if juice.motion() > 0. {
        juice.celebrate
    } else {
        10.
    };
    let alpha = ((intro - 0.45) / 0.25).clamp(0., 1.);
    if alpha <= 0. {
        return;
    }
    let rect = card_rect(SessionState::Won);
    panel(rect, alpha);
    let c = WIDTH * 0.5;
    let run = summary(game);
    let rank = quips::rank(run);
    centered_label(
        "THE LIGHT CARRIES ON",
        c,
        rect.y + 44.,
        16.,
        fade(GREEN, alpha),
    );
    let stamp = 1. + 0.6 * (1. - ((intro - 0.55) / 0.18).clamp(0., 1.)).powi(2);
    centered_label(rank.title, c, rect.y + 96., 40. * stamp, fade(GOLD, alpha));
    fitted_centered_label(
        rank.quip,
        c,
        rect.y + 126.,
        rect.w - 48.,
        16.,
        fade(CREAM, alpha),
    );

    let time = format!("{:.2}S", game.trial.elapsed());
    let medal = run.medal;
    let group = 52. + text_width(&time, 40.);
    let badge = c - group * 0.5 + 22.;
    draw_circle(badge, rect.y + 178., 22., fade(medal_color(medal), alpha));
    draw_circle_lines(
        badge,
        rect.y + 178.,
        22.,
        3.,
        fade(Color::new(0.1, 0.1, 0.08, 1.), alpha * 0.5),
    );
    centered_label(
        &quips::medal_name(medal)[..1],
        badge + 1.,
        rect.y + 190.,
        24.,
        fade(Color::new(0.12, 0.11, 0.08, 1.), alpha),
    );
    label(&time, badge + 30., rect.y + 196., 40., fade(CREAM, alpha));
    centered_label(
        &format!("{} MEDAL", quips::medal_name(medal)).replace("NO MEDAL MEDAL", "NO MEDAL YET"),
        c,
        rect.y + 230.,
        16.,
        fade(medal_color(medal), alpha),
    );
    let (record, record_color) = record_line(game, results);
    fitted_centered_label(
        &record,
        c,
        rect.y + 258.,
        rect.w - 48.,
        16.,
        fade(record_color, alpha),
    );
    let (hook_text, hook_color) = hook(game);
    fitted_centered_label(
        &hook_text,
        c,
        rect.y + 282.,
        rect.w - 48.,
        16.,
        fade(hook_color, alpha),
    );

    stats(
        &[
            (
                "EMBERS",
                format!("{}/{}", run.embers, run.total_embers),
                ORANGE,
            ),
            ("POINTS", game.score.total().to_string(), ORANGE),
            ("FALLS", run.falls.to_string(), CREAM),
            ("CLOSE CALLS", run.close_calls.to_string(), CREAM),
            (
                "MOTH",
                if run.secret {
                    "FOUND".into()
                } else {
                    "?".into()
                },
                if run.secret { MOTH } else { MUTED },
            ),
        ],
        c,
        rect.w - 40.,
        rect.y + 336.,
        alpha,
    );
    call_to_action(
        &if hints.tap_confirm {
            "TAP HERE TO RUN AGAIN".to_owned()
        } else {
            format!(
                "{} / {} TO RUN AGAIN",
                hints.confirm.text(),
                hints.restart.text()
            )
        },
        rect.y + 434.,
        juice,
        alpha,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Screens from the fullscreen audit plus common letterbox-free 16:9 sizes.
    const SCREENS: [Vec2; 8] = [
        Vec2::new(1280., 800.),
        Vec2::new(1600., 1000.),
        Vec2::new(390., 844.),
        Vec2::new(844., 390.),
        Vec2::new(320., 480.),
        Vec2::new(1280., 720.),
        Vec2::new(1920., 1080.),
        Vec2::new(700., 394.),
    ];

    fn clear_of_menu(screen: Vec2, rect: Rect) -> bool {
        rect.right() <= screen.x - MENU_CORNER.x || rect.y >= MENU_CORNER.y
    }

    #[test]
    fn fullscreen_menu_corner_stays_clear_of_hud_text_and_controls() {
        for screen in SCREENS {
            if let Some(hud) = compact::CompactHud::for_screen(screen) {
                // Compact HUD draws in unscaled screen units from the origin.
                let right = compact::bar_right(screen.x);
                assert!(right <= screen.x - MENU_CORNER.x, "{screen:?}");
                for button in hud.touch_layout() {
                    let ferravex_render::input::touch::Region::Rect(rect) = button.region else {
                        unreachable!()
                    };
                    assert!(clear_of_menu(screen, rect), "{screen:?} {rect:?}");
                }
                continue;
            }
            let canvas = ferravex_render::canvas::Canvas2D::fit(vec2(WIDTH, HEIGHT), screen);
            let layout = WideLayout::new(canvas, screen);
            let to_screen = |r: Rect| {
                let a = canvas.project(vec2(r.x, r.y));
                let b = canvas.project(vec2(r.right(), r.bottom()));
                Rect::new(a.x, a.y, b.x - a.x, b.y - a.y)
            };
            // The widest right cluster: a long ember count over points.
            let cluster = Rect::new(layout.right - 160., CLUSTER_TOP, 160., 62.);
            assert!(clear_of_menu(screen, to_screen(cluster)), "{screen:?}");
            assert!(clear_of_menu(screen, to_screen(layout.pause)), "{screen:?}");
            assert!(layout.right >= WIDTH * 0.75 && layout.pause.x >= WIDTH * 0.7);
            for state in [SessionState::Ready, SessionState::Paused, SessionState::Won] {
                assert!(
                    clear_of_menu(screen, to_screen(card_rect(state))),
                    "{screen:?}"
                );
            }
        }
        // Letterboxed desktop windows keep the authored symmetric 32-unit margin.
        for screen in [vec2(1280., 800.), vec2(1600., 1000.)] {
            let canvas = ferravex_render::canvas::Canvas2D::fit(vec2(WIDTH, HEIGHT), screen);
            assert_eq!(WideLayout::new(canvas, screen).right, WIDTH - 32.);
        }
    }

    #[test]
    fn pace_and_records_follow_authored_targets_and_never_claim_a_demo_best() {
        assert_eq!(pace(0.), Medal::Gold);
        assert_eq!(pace(12.), Medal::Gold);
        assert_eq!(pace(12.01), Medal::Silver);
        assert_eq!(pace(29.9), Medal::Bronze);
        assert_eq!(pace(31.), Medal::Unranked);
        let first = Results::default();
        assert!(first.new_best(10.));
        let chase = Results {
            previous_best: Some(9.5),
            demo: false,
        };
        assert!(!chase.new_best(9.5) && !chase.new_best(10.) && chase.new_best(9.4));
        let demo = Results {
            previous_best: None,
            demo: true,
        };
        assert!(!demo.new_best(1.));
    }

    #[test]
    fn coaching_fades_as_each_lesson_is_learned() {
        let mut game = Game::new(true);
        assert!(coach(&game, "Space").unwrap().starts_with("Space: JUMP"));
        game.jumps = 1;
        assert_eq!(coach(&game, "Space"), None);
        game.motor = Character::new(feet(vec2(1100., 520.)));
        assert!(coach(&game, "Space").unwrap().contains("LANTERN"));
        game.motor = Character::new(feet(vec2(1410., 520.)));
        game.update(Input::default(), 1. / 120.);
        assert_eq!(game.checkpoints.index(), 1);
        assert_eq!(
            coach(&game, "Space"),
            None,
            "a lit lantern ends that lesson"
        );
    }

    #[test]
    fn finished_run_hooks_always_offer_a_next_goal() {
        let mut game = Game::new(true);
        for _ in 0..2400 {
            game.update(game.autoplay(), 1. / 120.);
            if game.won() {
                break;
            }
        }
        assert_eq!(game.trial.medal(), Some(Medal::Gold));
        assert_eq!(hook(&game).0, "GOLD! NOW TRY ALL 12 EMBERS");
        let results = Results {
            previous_best: Some(game.trial.elapsed() - 0.5),
            demo: false,
        };
        assert_eq!(
            record_line(&game, &results).0,
            "PERSONAL BEST 8.60S  (+0.50S)"
        );
        let improved = Results {
            previous_best: Some(game.trial.elapsed() + 0.25),
            demo: false,
        };
        assert_eq!(record_line(&game, &improved).0, "NEW PERSONAL BEST! -0.25S");
        for embers in &mut game.embers {
            embers.1 = true;
        }
        assert!(hook(&game).0.contains("FLUTTERS"));
        game.secret = true;
        assert!(hook(&game).0.contains("SPEED"));
    }
}
