//! The in-play HUD: icon + number stats, pips and ring meters instead of
//! labels; one centred callout at a time; key cap + pictogram prompts instead
//! of instructions. Everything reads already-accepted state and formats into
//! stack [`Line`]s, so steady-state frames do not allocate.
use crate::{
    courses::Course,
    juice::{self, BRONZE, CORAL, Counter, GREEN, Juice, VIOLET},
    look::{self, GOLD, Glyph, INK, Item, Line, MUTED, TEAL, glyph, line},
    sim::Run,
    text,
    timing::Split,
    trial::Trial,
};
use ferravex_render::{
    icons::{self, Icon},
    input::PlayerInput,
    prelude::*,
    shapes::{self, Fill, Shadow, Stroke},
    text::{TextStyle, text_width},
    ui::kit::{self, KitTheme},
};
use std::fmt::Write;

/// Remapped hint for `action` in a stack line (no heap allocation).
pub(crate) fn hint_line(player: &PlayerInput, action: &str) -> Line {
    let mut buffer = [0; 160];
    Line::of(format_args!(
        "{}",
        player
            .format_hint(action, &mut buffer)
            .unwrap_or("Controls")
    ))
}

#[cfg(test)]
pub(crate) fn hint(player: &PlayerInput, action: &str) -> String {
    let mut buffer = [0; 512];
    player
        .format_hint(action, &mut buffer)
        .unwrap_or("Controls")
        .to_owned()
}

/// Fit to actual bitmap advances; abbreviate exceptionally long remapped labels
/// rather than shrinking them to unreadable sizes or drawing over neighboring UI.
pub(crate) fn fitted(label: &str, width: f32, size: u16) -> (String, f32, f32) {
    let font = get_default_font();
    let base = font.try_metrics(size, 1.).unwrap();
    let measured = base.measure_line(label).unwrap().width;
    let scale = (width.max(1.) / measured.max(1.)).clamp(10. / size.max(10) as f32, 1.);
    let metrics = font.try_metrics(size, scale).unwrap();
    let mut label = label.to_owned();
    if metrics.measure_line(&label).unwrap().width > width {
        let suffix = metrics.measure_line("...").unwrap().width;
        let count = ((width - suffix).max(0.) / metrics.glyph_advance()) as usize;
        if let Some((end, _)) = label.char_indices().nth(count) {
            label.truncate(end);
        }
        // The final measurement accounts for accumulated fractional advances.
        while !label.is_empty() && metrics.measure_line(&label).unwrap().width + suffix > width {
            label.pop();
        }
        label.push_str("...");
    }
    let measured = metrics.measure_line(&label).unwrap().width;
    (label, size as f32 * scale, measured)
}

pub(crate) fn centered(label: &str, y: f32, size: u16, color: Color) {
    centered_in(label, y, size, color, (screen_width() - 32.).max(32.));
}

pub(crate) fn centered_in(label: &str, y: f32, size: u16, color: Color, width: f32) {
    let (label, size, width) = fitted(label, width, size);
    text(&label, (screen_width() - width) * 0.5, y, size, color);
}

/// What the overview pin points at: the next checkpoint, then any missing
/// crystal, then the finish arch once it accepts the marble.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Target {
    Checkpoint(usize),
    Crystal(usize),
    Finish,
}

pub(crate) fn goal(run: &Run) -> (Target, Vec3) {
    if run.gems() == run.level.gems.len() {
        (Target::Finish, run.level.finish)
    } else if let Some(cp) = run.level.checkpoints.get(run.checkpoint) {
        (Target::Checkpoint(run.checkpoint), cp.position)
    } else if let Some(index) = run.collected.iter().position(|&collected| !collected) {
        (Target::Crystal(index), run.level.gems[index])
    } else {
        (Target::Finish, run.level.finish)
    }
}

/// One centred callout, chosen from accepted state. Transient notices yield
/// to pause/focus/Controls overlays; their tick-owned lifetime is retained,
/// so resuming shows the remaining notice. Quips and splits come from lines
/// chosen at the accepted event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Notice {
    /// 3, 2, 1.
    Count(u32),
    Go,
    /// Back at checkpoint `checkpoint` (0 is the start) with every crystal kept.
    Fall {
        line: &'static str,
        checkpoint: usize,
        falls: u32,
    },
    /// Checkpoint `index` of `total`; `teach` shows the recover key the first
    /// time a checkpoint is banked without a fall.
    Checkpoint {
        line: &'static str,
        index: usize,
        total: usize,
        split: Option<Split>,
        teach: bool,
    },
}

pub(crate) fn notice(run: &Run, trial: &Trial, juice: &Juice) -> Option<Notice> {
    if !trial.session.is_playing() {
        return None;
    }
    if let Some(count) = trial.countdown() {
        Some(Notice::Count(count))
    } else if trial.showing_go() {
        Some(Notice::Go)
    } else if run.notice > 0. {
        Some(if run.recovered {
            Notice::Fall {
                line: if juice.fall_line.is_empty() {
                    "BACK ON TRACK"
                } else {
                    juice.fall_line
                },
                checkpoint: run.checkpoint,
                falls: run.falls,
            }
        } else {
            Notice::Checkpoint {
                line: if juice.checkpoint_line.is_empty() {
                    "CHECKPOINT!"
                } else {
                    juice.checkpoint_line
                },
                index: run.checkpoint,
                total: run.level.checkpoints.len(),
                split: juice.split,
                teach: run.falls == 0 && run.checkpoint == 1,
            }
        })
    } else {
        None
    }
}

/// A signed seconds delta such as `-1.00s` (negative is faster).
pub(crate) fn signed_seconds(delta_us: i128) -> Line {
    line!("{:+.2}s", delta_us as f64 / 1e6)
}

/// Keyboard roll keys as key-cap labels, e.g. (`WASD`, true when the arrow
/// keys are also bound), following remaps; `STICK` for pads.
pub(crate) fn roll_keys(player: &PlayerInput) -> (Line, bool) {
    let keys =
        ["move_forward", "move_left", "move_back", "move_right"].map(|a| hint_line(player, a));
    if keys
        .iter()
        .any(|k| k.as_str().starts_with("Pad") || k.as_str().starts_with("Touch"))
    {
        return (line!("Stick"), false);
    }
    let first = keys.map(|k| {
        let mut l = Line::new();
        let _ = l.write_str(k.as_str().split(" / ").next().unwrap_or(""));
        l
    });
    let arrows = keys
        .iter()
        .zip(["Up", "Left", "Down", "Right"])
        .all(|(k, a)| k.as_str().split(" / ").nth(1) == Some(a));
    let single = first.iter().all(|k| k.as_str().chars().count() == 1);
    let mut label = Line::new();
    for (i, k) in first.iter().enumerate() {
        if i > 0 && !single {
            let _ = label.write_str("/");
        }
        let _ = label.write_str(k.as_str());
    }
    (label, arrows)
}

fn clock(seconds: f32) -> Line {
    line!("{:02}:{:04.1}", (seconds / 60.) as u32, seconds % 60.)
}

/// `M:SS.cc` for finished times and bests.
pub(crate) fn stopwatch_line(seconds: f32) -> Line {
    let centis = (seconds.max(0.) * 100.).round() as u64;
    line!(
        "{}:{:02}.{:02}",
        centis / 6000,
        centis / 100 % 60,
        centis % 100
    )
}

/// Medal targets: `0:35` for whole seconds, `0:52.5` otherwise.
pub(crate) fn target_time(seconds: f32) -> Line {
    let tenths = (seconds.max(0.) * 10.).round() as u64;
    if tenths.is_multiple_of(10) {
        line!("{}:{:02}", tenths / 600, tenths / 10 % 60)
    } else {
        line!("{}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
    }
}

/// What the bottom of the screen teaches right now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Tip {
    /// Roll / brake / overview key caps, fading as the player learns.
    Controls(f32),
    /// After two falls in one section: brake and recover.
    Brake,
}

/// The countdown's one-more-try hook: attempt, gold target and best.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Countdown {
    pub attempt: u32,
    pub gold: f32,
    pub best: Option<f32>,
}

/// Everything the in-play HUD reads; all of it is already-accepted state.
pub(crate) struct Hud<'a> {
    pub run: &'a Run,
    pub course: &'a Course,
    pub trial: &'a Trial,
    pub player: &'a PlayerInput,
    pub juice: &'a Juice,
    pub touch: bool,
    pub overview: bool,
    pub tip: Option<Tip>,
    pub countdown: Countdown,
}

/// Browser fullscreen pins a 44×44 menu button inset 8 px at the top-right;
/// the HUD keeps this corner (x > w - 72, y < 60) free of text and controls.
pub(crate) const MENU_CORNER: (f32, f32) = (72., 60.);

fn wide(w: f32) -> bool {
    w >= 560.
}

/// Styles for the crystal count and the clock.
fn status_styles(w: f32) -> (TextStyle, TextStyle) {
    if wide(w) {
        (look::body(16., GOLD), look::punch(24., INK))
    } else {
        (look::body(16., GOLD), look::punch(16., INK))
    }
}

fn status_icon(w: f32) -> f32 {
    if wide(w) { 22. } else { 18. }
}

/// Crystal pips read at a glance up to this many; larger courses show a count.
const MAX_PIPS: usize = 8;
const GEM_STEP: f32 = 14.;

/// Gem icon then one diamond per crystal (or a count for big courses).
fn gems_width(w: f32, collected: usize, total: usize) -> f32 {
    let (g, _) = status_styles(w);
    status_icon(w)
        + 8.
        + if total <= MAX_PIPS {
            (total as f32 * GEM_STEP - 3.).max(0.)
        } else {
            text_width(line!("{collected}/{total}").as_str(), g)
        }
}

/// Size of the crystals/clock card for these values.
fn status_size(w: f32, collected: usize, total: usize, time: &str) -> Vec2 {
    let (_, t) = status_styles(w);
    let icon = status_icon(w) - 2.;
    vec2(
        14. + gems_width(w, collected, total) + 20. + icon + 7. + text_width(time, t) + 16.,
        if wide(w) { 44. } else { 38. },
    )
}

/// Top-left of the crystals/clock card: inset left of the menu corner on wide
/// screens, stacked under the course button on narrow ones.
fn status_origin(w: f32, card_w: f32) -> (f32, f32) {
    if wide(w) {
        (w - MENU_CORNER.0 - 12. - card_w, 8.)
    } else {
        // Stacked rows start below the corner, so long clocks may run wide.
        (12., MENU_CORNER.1 + 2.)
    }
}

const CHIP_H: f32 = 28.;
const RING_R: f32 = 10.5;

/// Seconds the current medal stays on; blank once only bronze remains.
fn pace_label(deadline: Option<f32>, elapsed: f32, counting: bool) -> Line {
    match deadline {
        Some(deadline) if deadline - elapsed < 5. && !counting => {
            line!("{:.1}s", (deadline - elapsed).max(0.))
        }
        Some(deadline) => line!("{:.0}s", (deadline - elapsed).ceil()),
        None => Line::new(),
    }
}

/// Medal ring, then the seconds left (or a finish flag for bronze).
fn pace_width(label: &str) -> f32 {
    10. + 2. * RING_R
        + 7.
        + if label.is_empty() {
            16.
        } else {
            text_width(label, look::body(16., INK))
        }
        + 12.
}

/// Right edge of the wide pace chip, clear of the corner and the touch
/// Recover button.
fn pace_right(w: f32, touch: bool) -> f32 {
    if touch {
        w - 110.
    } else {
        w - MENU_CORNER.0 - 12.
    }
}

/// Bottom of the stacked top HUD, so centred notices never overlap it.
fn hud_bottom(w: f32, h: f32) -> f32 {
    if wide(w) {
        92.
    } else if h >= 300. {
        170.
    } else {
        104.
    }
}

fn chip(x: f32, y: f32, width: f32) {
    look::glass(Rect::new(x, y, width, CHIP_H), CHIP_H * 0.5, 1.);
}

/// A crystal pip: a small gold diamond, hollow until collected.
fn diamond(c: Vec2, r: f32, filled: bool) {
    let points = [
        c + vec2(0., -r),
        c + vec2(r * 0.72, 0.),
        c + vec2(0., r),
        c + vec2(-r * 0.72, 0.),
    ];
    if filled {
        shapes::fill_polygon(&points, GOLD);
        shapes::fill_triangle(
            points[3],
            points[0],
            c + vec2(0., -r * 0.1),
            look::fade(Color::new(1., 1., 1., 1.), 0.45),
        );
    } else {
        shapes::polyline(
            &points,
            Stroke::new(1.5, look::fade(INK, 0.45)).closed(true),
        );
    }
}

pub(crate) fn draw(hud: &Hud) {
    let Hud { run, course, .. } = *hud;
    let (w, h) = (screen_width(), screen_height());
    let wide = wide(w);
    let motion = hud.juice.motion();
    let theme = look::theme();

    // Crystals and the clock: the two numbers that matter most.
    let (collected, total) = (run.gems(), run.level.gems.len());
    let time = clock(run.elapsed);
    let size = status_size(w, collected, total, time.as_str());
    let (cx, cy) = status_origin(w, size.x);
    look::glass(Rect::new(cx, cy, size.x, size.y), 12., 1.);
    let all = collected == total;
    let mid = cy + size.y * 0.5;
    let (g, t) = status_styles(w);
    let icon = status_icon(w);
    let bounce = hud.juice.counter_scale(Counter::Crystals);
    let mut x = cx + 14.;
    if all {
        shapes::glow_disc(vec2(x + icon * 0.5, mid), icon, look::fade(GOLD, 0.4));
    }
    look::icon(
        glyph::CRYSTAL,
        vec2(x + icon * 0.5, mid),
        icon * bounce,
        GOLD,
    );
    x += icon + 8.;
    if total <= MAX_PIPS {
        for i in 0..total {
            let latest = i + 1 == collected;
            let r = 6. * if latest { bounce } else { 1. };
            diamond(vec2(x + 5. + i as f32 * GEM_STEP, mid), r, i < collected);
        }
    } else {
        let count = line!("{collected}/{total}");
        look::draw(count.as_str(), x, g.centered_baseline(mid), g);
    }
    let tx = cx + size.x - 16. - text_width(time.as_str(), t);
    let clock_size = icon - 2.;
    look::icon(
        glyph::TIME,
        vec2(tx - 7. - clock_size * 0.5, mid),
        clock_size,
        look::fade(INK, 0.85),
    );
    look::draw(time.as_str(), tx, t.centered_baseline(mid), t);

    // Medal pace: a trophy in the medal still on, ringed by the time it has left.
    let mut stack = cy + size.y + 6.;
    if wide || h >= 300. {
        let (medal, deadline) = course.pace(run.elapsed, run.falls);
        let counting = hud.trial.countdown().is_some();
        let label = pace_label(deadline, run.elapsed, counting);
        let urgent = deadline.is_some_and(|d| d - run.elapsed < 5.) && !counting;
        let chip_w = pace_width(label.as_str());
        let (x, y) = if wide {
            (pace_right(w, hud.touch) - chip_w, 58.)
        } else {
            (12., stack)
        };
        stack = y + CHIP_H + 6.;
        chip(x, y, chip_w);
        let color = juice::medal_color(medal);
        let pulse = if urgent && motion > 0. {
            1. + 0.18 * (hud.juice.time * 9.).sin().abs()
        } else {
            1.
        };
        let centre = vec2(x + 10. + RING_R, y + CHIP_H * 0.5);
        let left = deadline.map_or(1., |d| ((d - run.elapsed) / d.max(0.01)).clamp(0., 1.));
        shapes::stroke_circle(centre, RING_R, 2.5, look::fade(INK, 0.16));
        shapes::stroke_arc(
            centre,
            RING_R,
            -std::f32::consts::FRAC_PI_2,
            std::f32::consts::TAU * left,
            Stroke::new(2.5, if urgent { CORAL } else { color }).cap(shapes::Cap::Round),
        );
        look::icon(glyph::MEDAL, centre, 13. * pulse, color);
        let lx = centre.x + RING_R + 7.;
        if label.as_str().is_empty() {
            // Only bronze remains: just reach the finish.
            look::icon(
                glyph::CHECKPOINT,
                vec2(lx + 8., y + CHIP_H * 0.5),
                16.,
                BRONZE,
            );
        } else {
            let style = look::body(16., if urgent { CORAL } else { INK });
            look::draw(
                label.as_str(),
                lx,
                style.centered_baseline(y + CHIP_H * 0.5),
                style,
            );
        }
    }

    // Checkpoint pips, falls and a found moonstone under the course button.
    if h >= 240. && (wide || h >= 300.) {
        let (x, y) = if wide { (12., 58.) } else { (12., stack) };
        let total = run.level.checkpoints.len();
        let pips = total.min(8);
        let falls = line!("{}", run.falls);
        let falls_style = look::body(16., if run.falls == 0 { INK } else { CORAL });
        let count = (total > 8).then(|| line!("{}/{}", run.checkpoint, total));
        let count_style = look::body(16., TEAL);
        let mut width = 12. + 16. + 8.;
        width += match &count {
            Some(c) => text_width(c.as_str(), count_style),
            None => pips as f32 * 16. - 6.,
        };
        width += 14. + 15. + 5. + text_width(falls.as_str(), falls_style) + 12.;
        if run.secret_found {
            width += 22.;
        }
        chip(x, y, width);
        let base = y + CHIP_H * 0.5;
        let mut cursor = x + 12.;
        let cp_bounce = hud.juice.counter_scale(Counter::Checkpoints);
        look::icon(
            glyph::CHECKPOINT,
            vec2(cursor + 8., base),
            16. * cp_bounce,
            TEAL,
        );
        cursor += 16. + 8.;
        if let Some(c) = &count {
            cursor += look::draw(
                c.as_str(),
                cursor,
                count_style.centered_baseline(base),
                count_style,
            );
        } else {
            for i in 0..pips {
                let p = vec2(cursor + 5. + i as f32 * 16., base);
                if i < run.checkpoint {
                    let r = 5.
                        * if i + 1 == run.checkpoint {
                            cp_bounce
                        } else {
                            1.
                        };
                    shapes::fill_circle(p, r, TEAL);
                } else if i == run.checkpoint {
                    shapes::glow_disc(p, 10., look::fade(GOLD, 0.35));
                    shapes::stroke_circle(p, 5., 2., GOLD);
                } else {
                    shapes::stroke_circle(p, 5., 1.5, look::fade(INK, 0.45));
                }
            }
            cursor += pips as f32 * 16. - 6.;
        }
        cursor += 14.;
        let fall_bounce = hud.juice.counter_scale(Counter::Falls);
        look::icon(
            glyph::FALL,
            vec2(cursor + 7.5, base),
            15. * fall_bounce,
            if run.falls == 0 { MUTED } else { CORAL },
        );
        cursor += 15. + 5.;
        cursor += look::draw(
            falls.as_str(),
            cursor,
            falls_style.centered_baseline(base),
            falls_style,
        );
        if run.secret_found {
            look::icon(glyph::CRYSTAL, vec2(cursor + 15., base), 15., VIOLET);
        }
    }

    // One centred message at a time: countdown, then sim notices, then banners.
    let top = hud_bottom(w, h) + 20.;
    let centre_y = (h * 0.3).max(top + 36.).min(h - 64.);
    match notice(run, hud.trial, hud.juice) {
        Some(Notice::Count(count)) => countdown(hud, w, centre_y, count, &theme),
        Some(Notice::Go) => {
            let c = vec2(w * 0.5, centre_y);
            if motion > 0. {
                shapes::glow_disc(c, 90., look::fade(GOLD, 0.35));
            }
            let style =
                look::centered(look::punch(64., GOLD).with_outline(look::fade(look::SHADOW, 0.9)));
            look::draw("GO!", c.x, style.centered_baseline(c.y), style);
        }
        Some(Notice::Fall {
            line, checkpoint, ..
        }) => {
            let number = line!("{checkpoint}");
            let back: &[Item] = if checkpoint == 0 {
                &[Item::Icon(Glyph::Icon(glyph::START), TEAL)]
            } else {
                &[
                    Item::Icon(Glyph::Icon(glyph::CHECKPOINT), TEAL),
                    Item::Text(number.as_str(), look::body(16., INK)),
                ]
            };
            let mut row = [Item::Space(0.); 7];
            row[0] = Item::Icon(Glyph::Icon(glyph::RECOVER), MUTED);
            let n = back.len();
            row[1..1 + n].copy_from_slice(back);
            row[1 + n] = Item::Space(16.);
            row[2 + n] = Item::Icon(Glyph::Icon(glyph::CRYSTAL), GOLD);
            row[3 + n] = Item::Small(Glyph::Icon(glyph::DONE), GREEN, 14.);
            message(w, centre_y, glyph::RECOVER, line, CORAL, 1., &row[..4 + n]);
        }
        Some(Notice::Checkpoint {
            line,
            index,
            total,
            split,
            teach,
        }) => {
            let key = hint_line(hud.player, "recover");
            let time = split.map(|s| line!("{:.2}s", s.time_us as f64 / 1e6));
            let delta = split.and_then(|s| s.delta_us).map(signed_seconds);
            let delta_color = if split.and_then(|s| s.delta_us).is_some_and(|d| d <= 0) {
                GREEN
            } else {
                CORAL
            };
            let mut row = [Item::Space(0.); 8];
            row[0] = Item::Pips(index, total.min(8), TEAL);
            let mut n = 1;
            if teach {
                // Contextual prompt: this key brings the marble back here.
                for item in [
                    Item::Space(16.),
                    Item::Key(key.as_str()),
                    Item::Icon(Glyph::Icon(glyph::RECOVER), INK),
                    Item::Icon(Glyph::Icon(glyph::CHECKPOINT), TEAL),
                ] {
                    row[n] = item;
                    n += 1;
                }
            } else if let Some(time) = &time {
                row[n] = Item::Space(16.);
                row[n + 1] = Item::Icon(Glyph::Icon(glyph::TIME), MUTED);
                row[n + 2] = Item::Text(time.as_str(), look::body(16., INK));
                n += 3;
                if let Some(delta) = &delta {
                    row[n] = Item::Text(delta.as_str(), look::body(16., delta_color));
                    n += 1;
                }
            }
            message(w, centre_y, glyph::CHECKPOINT, line, TEAL, 1., &row[..n]);
        }
        None => {
            if let Some(banner) = &hud.juice.banner {
                let deadline = banner.deadline.map(target_time);
                let row: &[Item] = match &deadline {
                    Some(d) => &[
                        Item::Icon(Glyph::Icon(glyph::TIME), MUTED),
                        Item::Text(d.as_str(), look::body(16., banner.color)),
                    ],
                    None => &[],
                };
                message(
                    w,
                    centre_y,
                    banner.icon,
                    banner.head,
                    banner.color,
                    banner.alpha(),
                    row,
                );
            }
        }
    }

    // Bottom: onboarding key caps (the overview draws a pin in the world).
    if !hud.overview
        && !hud.touch
        && let Some(tip) = hud.tip
    {
        tips(w, h, hud.player, tip);
    }
}

/// The countdown disc and the one-more-try icon row under it.
fn countdown(hud: &Hud, w: f32, centre_y: f32, count: u32, theme: &KitTheme) {
    let motion = hud.juice.motion();
    let progress = hud.trial.countdown_progress();
    let pop = if motion > 0. {
        1. - ferravex_engine::play::tween::Ease::BackOut.apply((progress / 0.22).min(1.))
    } else {
        0.
    };
    let c = vec2(w * 0.5, centre_y);
    let r = 46. * (1. + 0.12 * pop);
    shapes::drop_shadow(
        Rect::new(c.x - r, c.y - r, 2. * r, 2. * r),
        r,
        Shadow::soft(Color::new(0., 0., 0., 0.35))
            .offset(vec2(0., 5.))
            .blur(18.),
    );
    shapes::fill_circle(c, r, Fill::radial(c, r, look::GLASS_TOP, look::GLASS));
    shapes::stroke_arc(
        c,
        r - 2.,
        -std::f32::consts::FRAC_PI_2,
        std::f32::consts::TAU * (1. - progress),
        Stroke::new(4., GOLD).cap(shapes::Cap::Round),
    );
    let style = look::centered(
        look::punch(56. * (1. + 0.25 * pop), INK).with_outline(look::fade(look::SHADOW, 0.8)),
    );
    let label = line!("{count}");
    look::draw(label.as_str(), c.x, style.centered_baseline(c.y), style);

    // Attempt, the gold target (time, no falls) and the best to beat.
    let Countdown {
        attempt,
        gold,
        best,
    } = hud.countdown;
    let attempt = line!("{attempt}");
    let target = target_time(gold);
    let best = best.map(stopwatch_line);
    let text = look::body(16., INK);
    let mut row = [
        Item::Icon(Glyph::Icon(glyph::ATTEMPT), MUTED),
        Item::Text(attempt.as_str(), text),
        Item::Space(18.),
        Item::Icon(Glyph::Icon(glyph::MEDAL), GOLD),
        Item::Text(target.as_str(), look::body(16., GOLD)),
        Item::Space(10.),
        Item::Small(Glyph::Icon(glyph::FALL), MUTED, 15.),
        Item::Text("0", text),
        Item::Space(18.),
        Item::Icon(Glyph::Icon(glyph::BEST), GOLD),
        Item::Text("", text),
    ];
    let items = match &best {
        Some(best) => {
            row[10] = Item::Text(best.as_str(), text);
            &row[..]
        }
        None => &row[..8],
    };
    // Drop the best, then the attempt, before the row could overflow.
    let fits = |items: &[Item]| look::row_width(items, 18., theme) + 32. <= w - 24.;
    let items = if fits(items) {
        items
    } else if fits(&row[..8]) {
        &row[..8]
    } else {
        &row[3..8]
    };
    let width = look::row_width(items, 18., theme);
    let y = centre_y + r + 14.;
    chip((w - width) * 0.5 - 16., y, width + 32.);
    look::draw_row_centered(items, w * 0.5, y + CHIP_H * 0.5, 18., theme);
}

/// A centred glass pill: an icon badge and a punchy headline, then an
/// optional icon row (the details, as pictograms and numbers).
fn message(w: f32, y: f32, icon: Icon, head: &str, accent: Color, alpha: f32, row: &[Item]) {
    if alpha <= 0. {
        return;
    }
    let theme = look::theme().faded(alpha);
    let badge = 30.;
    let room = (w - 24.).min(520.) - 40.;
    let style = look::fit(
        head,
        room - badge - 10.,
        look::punch(if wide(w) { 24. } else { 16. }, look::fade(INK, alpha))
            .with_outline(look::fade(look::SHADOW, 0.8 * alpha)),
    );
    let head_w = text_width(head, style);
    let row_w = look::row_width(row, 18., &theme);
    let group = badge + 10. + head_w;
    let width = (group.max(row_w) + 40.).min(w - 24.);
    let height = if row.is_empty() { 52. } else { 82. };
    let rect = Rect::new((w - width) * 0.5, y - 26., width, height);
    look::glass(rect, 18., alpha);
    let line_y = rect.y + 26.;
    let left = (w - group) * 0.5;
    icons::badge(
        icon,
        vec2(left + badge * 0.5, line_y),
        badge,
        look::fade(accent, alpha),
        look::fade(Color::new(0.05, 0.1, 0.11, 1.), alpha),
    );
    look::draw(
        head,
        left + badge + 10.,
        style.centered_baseline(line_y),
        style,
    );
    if !row.is_empty() && row_w <= width - 16. {
        look::draw_row_centered(row, w * 0.5, rect.y + 60., 18., &theme);
    }
}

/// Key cap + pictogram onboarding that follows remaps; drops prompts that do
/// not fit rather than crowding them.
fn tips(w: f32, h: f32, player: &PlayerInput, tip: Tip) {
    let (roll, arrows) = roll_keys(player);
    let brake = hint_line(player, "brake");
    let overview = hint_line(player, "overview");
    let recover = hint_line(player, "recover");
    let roll_group = [
        Item::Key(roll.as_str()),
        Item::Key("←↑↓→"),
        Item::Icon(Glyph::Marble, INK),
    ];
    let rolls: &[Item] = if arrows {
        &roll_group
    } else {
        &[roll_group[0], roll_group[2]]
    };
    let brake_group = [
        Item::Key(brake.as_str()),
        Item::Icon(Glyph::Icon(glyph::BRAKE), INK),
    ];
    let overview_group = [
        Item::Key(overview.as_str()),
        Item::Icon(Glyph::Icon(glyph::OVERVIEW), INK),
    ];
    let recover_group = [
        Item::Key(recover.as_str()),
        Item::Icon(Glyph::Icon(glyph::RECOVER), INK),
        Item::Icon(Glyph::Icon(glyph::CHECKPOINT), TEAL),
    ];
    let (alpha, groups): (f32, [&[Item]; 3]) = match tip {
        Tip::Controls(alpha) => (alpha, [rolls, &brake_group, &overview_group]),
        Tip::Brake => (1., [&brake_group, &recover_group, &[]]),
    };
    if alpha <= 0. {
        return;
    }
    let theme = look::theme().faded(alpha);
    let gap = 26.;
    let size = 22.;
    let mut row = [Item::Space(0.); 12];
    let mut n = 0;
    let mut total = 0.;
    for group in groups.iter().filter(|g| !g.is_empty()) {
        let width = look::row_width(group, size, &theme) + if n > 0 { gap } else { 0. };
        if total + width > w - 48. {
            break;
        }
        total += width;
        if n > 0 {
            row[n] = Item::Space(gap);
            n += 1;
        }
        row[n..n + group.len()].copy_from_slice(group);
        n += group.len();
    }
    if n == 0 {
        return;
    }
    let y = h - 22. - 30.;
    let back = Rect::new((w - total) * 0.5 - 14., y - 7., total + 28., 40.);
    look::glass(back, 20., alpha * 0.85);
    look::draw_row_centered(&row[..n], w * 0.5, y + 12., size, &theme);
}

/// The overview's pin over the next target, with its straight-line distance.
pub(crate) fn goal_pin(run: &Run, time: f32, motion: f32, project: impl Fn(Vec3) -> Option<Vec2>) {
    let (target, at) = goal(run);
    let Some(p) = project(at + Vec3::Y * 0.4) else {
        return;
    };
    let (w, h) = (screen_width(), screen_height());
    let (icon, color) = match target {
        Target::Checkpoint(_) => (glyph::CHECKPOINT, TEAL),
        Target::Crystal(_) => (glyph::CRYSTAL, GOLD),
        Target::Finish => (glyph::CHECKPOINT, GOLD),
    };
    let bob = if motion > 0. {
        (time * 3.).sin() * 4.
    } else {
        0.
    };
    let p = vec2(p.x.clamp(30., w - 30.), p.y.clamp(80., h - 40.));
    let head = vec2(p.x, p.y - 34. + bob);
    shapes::glow_disc(head, 30., look::fade(color, 0.35));
    shapes::fill_triangle(
        head + vec2(-8., 10.),
        head + vec2(8., 10.),
        vec2(p.x, p.y - 4. + bob),
        color,
    );
    icons::badge(icon, head, 30., color, Color::new(0.05, 0.1, 0.11, 1.));
    shapes::fill_circle(p, 3., look::fade(color, 0.8));
    let distance = line!("{:.0}m", run.position().distance(at));
    let style = look::centered(look::body(16., INK));
    let dw = text_width(distance.as_str(), style) + 16.;
    let chip_y = head.y - 15. - 6. - 22.;
    look::glass(Rect::new(head.x - dw * 0.5, chip_y, dw, 22.), 11., 1.);
    look::draw(
        distance.as_str(),
        head.x,
        style.centered_baseline(chip_y + 11.),
        style,
    );
}

/// A calm pause card: the quip, then key caps with pictograms for the
/// actions that matter. Controls draws above it.
pub(crate) fn draw_pause(
    player: &PlayerInput,
    quip: &str,
    look_in: ferravex_engine::play::tween::Appearance,
) {
    let (w, h) = (screen_width(), screen_height());
    shapes::fill_rounded_rect(
        Rect::new(0., 0., w, h),
        0.,
        Color::new(0.03, 0.06, 0.07, 0.45 * look_in.alpha),
    );
    let theme: KitTheme = look::theme().faded(look_in.alpha);
    let width = (w - 24.).min(440.);
    let rect = kit::appear(
        Rect::new((w - width) * 0.5, h * 0.5 - 84., width, 168.),
        look_in,
    );
    let inner = kit::card(rect, Some("Paused"), &theme);
    let q = look::fit(
        quip,
        inner.w,
        look::centered(look::body(16., look::fade(GOLD, look_in.alpha))),
    );
    look::draw(
        quip,
        rect.x + rect.w * 0.5,
        q.centered_baseline(inner.y + 10.),
        q,
    );
    let keys = [
        hint_line(player, "pause"),
        hint_line(player, "restart"),
        hint_line(player, "courses"),
        line!("F2"),
    ];
    let rows = [
        [
            [
                Item::Key(keys[0].as_str()),
                Item::Icon(Glyph::Icon(glyph::RESUME), GOLD),
            ],
            [
                Item::Key(keys[1].as_str()),
                Item::Icon(Glyph::Icon(glyph::ATTEMPT), INK),
            ],
        ],
        [
            [
                Item::Key(keys[2].as_str()),
                Item::Icon(Glyph::Icon(glyph::COURSES), INK),
            ],
            [
                Item::Key(keys[3].as_str()),
                Item::Icon(Glyph::Icon(glyph::CONTROLS), INK),
            ],
        ],
    ];
    let cx = rect.x + rect.w * 0.5;
    for (r, row) in rows.iter().enumerate() {
        let mid = inner.y + 46. + r as f32 * 40.;
        let gap = 40.;
        let widths = row.map(|items| look::row_width(&items, 22., &theme));
        if widths[0] + widths[1] + gap <= inner.w {
            let mut x = cx - (widths[0] + widths[1] + gap) * 0.5;
            for (items, width) in row.iter().zip(widths) {
                look::draw_row(items, x, mid, 22., &theme);
                x += width + gap;
            }
        } else {
            for (i, items) in row.iter().enumerate() {
                look::draw_row(
                    items,
                    rect.x + 12. + i as f32 * (rect.w * 0.5),
                    mid,
                    22.,
                    &theme,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::look::line;
    use ferravex_render::input::{Binding, InputCode, Preset};

    #[test]
    fn remapped_hints_and_fitted_labels_follow_real_bindings_and_font_widths() {
        let mut player = PlayerInput::preset("marble-run-hints", Preset::Racer);
        player
            .rebind(
                "brake",
                vec![Binding::digital(InputCode::Key(KeyCode::B as u16))],
            )
            .unwrap();
        assert_eq!(hint(&player, "brake"), "B");
        assert!(!hint(&player, "brake").contains("Space"));
        let (label, size, width) = fitted(&"LONG CUSTOM BINDING / ".repeat(20), 288., 17);
        assert!(label.ends_with("..."));
        assert!(size >= 10.);
        assert!(width <= 288.);
        assert_eq!(
            get_default_font()
                .try_metrics(17, size / 17.)
                .unwrap()
                .measure_line(&label)
                .unwrap()
                .width,
            width
        );
    }

    #[test]
    fn hud_and_buttons_keep_the_fullscreen_menu_corner_clear() {
        let clear = |w: f32, x: f32, y: f32, rw: f32, rh: f32, what: &str| {
            let (cw, ch) = MENU_CORNER;
            assert!(
                x + rw <= w - cw || y >= ch,
                "{what} at {w}: x {x}..{} y {y}..{}",
                x + rw,
                y + rh
            );
        };
        for (w, h) in [
            (1280., 800.),
            (1600., 1000.),
            (390., 844.),
            (844., 390.),
            (320., 480.),
        ] {
            // Widest realistic contents: double-digit crystals and a long clock,
            // and a full row of crystal pips.
            for (collected, total) in [(12, 12), (8, 8)] {
                let size = status_size(w, collected, total, "59:59.9");
                let (x, y) = status_origin(w, size.x);
                clear(w, x, y, size.x, size.y, "status card");
                assert!(x >= 0. && x + size.x <= w);
            }
            let size = status_size(w, 12, 12, "59:59.9");
            let (_, y) = status_origin(w, size.x);
            let chip_w = pace_width(pace_label(Some(52.5), 47.6, false).as_str());
            for touch in [false, true] {
                let (px, py) = if wide(w) {
                    (pace_right(w, touch) - chip_w, 58.)
                } else {
                    (12., y + size.y + 6.)
                };
                clear(w, px, py, chip_w, CHIP_H, "pace chip");
                assert!(px + chip_w <= w, "pace chip fits at {w}");
            }
            let course = crate::course_ui::course_rect(w);
            clear(
                w,
                course.min[0],
                course.min[1],
                course.size[0],
                course.size[1],
                "course",
            );
            let recover = crate::course_ui::recover_rect(w);
            clear(
                w,
                recover.min[0],
                recover.min[1],
                recover.size[0],
                recover.size[1],
                "recover",
            );
            assert!(recover.min[1] + recover.size[1] < h);
        }
    }

    #[test]
    fn times_and_deltas_read_compactly() {
        assert_eq!(stopwatch_line(31.095), "0:31.10");
        assert_eq!(stopwatch_line(75.004), "1:15.00");
        assert_eq!(target_time(35.), "0:35");
        assert_eq!(target_time(52.5), "0:52.5");
        assert_eq!(target_time(90.), "1:30");
        assert_eq!(signed_seconds(-1_000_000), "-1.00s");
        assert_eq!(signed_seconds(2_000_000), "+2.00s");
        // The pace chip counts whole seconds, then tenths in the last five.
        assert_eq!(pace_label(Some(35.), 8.2, false), "27s");
        assert_eq!(pace_label(Some(35.), 30.1, false), "4.9s");
        assert_eq!(pace_label(Some(35.), 0., true), "35s");
        assert_eq!(pace_label(None, 60., false), "");
    }

    #[test]
    fn roll_keys_follow_remaps() {
        let mut player = PlayerInput::preset("marble-run-roll", Preset::Racer);
        let (keys, arrows) = roll_keys(&player);
        assert_eq!((keys.as_str(), arrows), ("WASD", true));
        player
            .rebind(
                "move_forward",
                vec![Binding::digital(InputCode::Key(KeyCode::I as u16))],
            )
            .unwrap();
        assert_eq!(roll_keys(&player), (line!("IASD"), false));
    }

    #[test]
    fn guidance_names_checkpoint_then_missing_crystals_then_finish() {
        let mut run = Run::new();
        assert_eq!(goal(&run).0, Target::Checkpoint(0));
        run.checkpoint = run.level.checkpoints.len();
        assert_eq!(goal(&run), (Target::Crystal(0), run.level.gems[0]));
        run.collected.fill(true);
        assert_eq!(goal(&run), (Target::Finish, run.level.finish));
    }

    #[test]
    fn transient_notices_yield_to_overlays_without_losing_tick_owned_lifetime() {
        let mut run = Run::new();
        let mut trial = Trial::new(false);
        let juice = Juice::new();
        let first = notice(&run, &trial, &juice).unwrap();
        assert_eq!(first, Notice::Count(3));
        assert_eq!(trial.advance(30., true).steps, 0);
        assert_eq!(notice(&run, &trial, &juice), None);
        assert_eq!(trial.advance(0., false).steps, 0);
        assert_eq!(notice(&run, &trial, &juice), Some(first));

        // Actual countdown and GO ticks, then an accepted recovery at spawn.
        for _ in 0..481 {
            let frame = trial.advance(1. / 120., false);
            for _ in 0..frame.steps {
                trial.step(&mut run, crate::sim::Controls::default());
            }
        }
        trial.request_recovery();
        let frame = trial.advance(1. / 120., false);
        for _ in 0..frame.steps {
            trial.step(&mut run, crate::sim::Controls::default());
        }
        let recovered = notice(&run, &trial, &juice).unwrap();
        // Back at the start with every crystal kept; the first fall.
        assert_eq!(
            recovered,
            Notice::Fall {
                line: "BACK ON TRACK",
                checkpoint: 0,
                falls: 1
            }
        );
        let frozen = (
            run.elapsed,
            run.notice,
            trial.pose(&run),
            trial.visual_time(&run),
        );
        assert_eq!(trial.advance(30., true).steps, 0);
        assert_eq!(notice(&run, &trial, &juice), None);
        assert_eq!(
            (
                run.elapsed,
                run.notice,
                trial.pose(&run),
                trial.visual_time(&run)
            ),
            frozen
        );
        trial.advance(0., false);
        assert_eq!(notice(&run, &trial, &juice), Some(recovered));
        for _ in 0..360 {
            let frame = trial.advance(1. / 120., false);
            for _ in 0..frame.steps {
                trial.step(&mut run, crate::sim::Controls::default());
            }
        }
        assert_eq!(notice(&run, &trial, &juice), None);
    }

    #[test]
    fn first_banked_checkpoint_teaches_recovery_then_shows_splits() {
        let mut run = Run::new();
        let mut trial = Trial::new(false);
        let mut juice = Juice::new();
        for _ in 0..481 {
            let frame = trial.advance(1. / 120., false);
            for _ in 0..frame.steps {
                trial.step(&mut run, crate::sim::Controls::default());
            }
        }
        run.checkpoint = 1;
        run.notice = 2.;
        run.recovered = false;
        let Some(Notice::Checkpoint { teach, total, .. }) = notice(&run, &trial, &juice) else {
            panic!("checkpoint notice");
        };
        assert!(
            teach,
            "the recover key is shown at the first clean checkpoint"
        );
        assert_eq!(total, run.level.checkpoints.len());
        run.falls = 1;
        juice.split = Some(Split {
            index: 0,
            time_us: 9_000_000,
            delta_us: Some(-1_000_000),
        });
        let Some(Notice::Checkpoint { teach, split, .. }) = notice(&run, &trial, &juice) else {
            panic!("checkpoint notice");
        };
        assert!(!teach);
        assert_eq!(split.unwrap().delta_us, Some(-1_000_000));
    }
}
