use crate::{
    courses::Course,
    juice::{self, Juice},
    look::{self, GOLD, INK, Line, MUTED, TEAL, line},
    sim::Run,
    text,
    trial::Trial,
};
use ferravex_render::{
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

pub(crate) fn goal(run: &Run) -> Line {
    let (position, what) = if run.gems() == run.level.gems.len() {
        (run.level.finish, line!("the finish arch"))
    } else if let Some(cp) = run.level.checkpoints.get(run.checkpoint) {
        (cp.position, line!("checkpoint {}", run.checkpoint + 1))
    } else if let Some(index) = run.collected.iter().position(|&collected| !collected) {
        (run.level.gems[index], line!("crystal {}", index + 1))
    } else {
        (run.level.finish, line!("the finish arch"))
    };
    line!("Next: {what} · {:.0}m", run.position().distance(position))
}

/// Transient gameplay notices yield to pause/focus/Controls overlays. Their
/// tick-owned lifetime is retained, so resuming shows the remaining notice.
/// Quips and splits come from lines chosen at the accepted event.
pub(crate) fn notice(
    run: &Run,
    trial: &Trial,
    player: &PlayerInput,
    juice: &Juice,
) -> Option<(Line, Line)> {
    if !trial.session.is_playing() {
        return None;
    }
    if let Some(count) = trial.countdown() {
        Some((line!("{count}"), Line::new()))
    } else if trial.showing_go() {
        Some((line!("GO!"), Line::new()))
    } else if run.notice > 0. {
        Some(if run.recovered {
            let title = if juice.fall_line.is_empty() {
                "Back on track"
            } else {
                juice.fall_line
            };
            let mut detail = if run.checkpoint == 0 {
                line!("Back at the start")
            } else {
                line!("Back at checkpoint {}", run.checkpoint)
            };
            let _ = write!(detail, " · crystals kept · fall {}", run.falls);
            (line!("{title}"), detail)
        } else {
            let title = if juice.checkpoint_line.is_empty() {
                "Checkpoint saved"
            } else {
                juice.checkpoint_line
            };
            let detail = match (&juice.split, run.falls == 0 && run.checkpoint == 1) {
                (_, true) => line!(
                    "Checkpoint {} of {} · {} brings you back here",
                    run.checkpoint,
                    run.level.checkpoints.len(),
                    hint_line(player, "recover")
                ),
                (Some(split), false) => pretty_split(split),
                (None, false) => line!(
                    "Checkpoint {} of {}",
                    run.checkpoint,
                    run.level.checkpoints.len()
                ),
            };
            (line!("{title}"), detail)
        })
    } else {
        None
    }
}

/// "CP 1 SPLIT 9.00s / -1.00s SESSION" → "Split 9.00s · −1.00s vs session best".
pub(crate) fn pretty_split(label: &str) -> Line {
    let mut parts = label.split(" / ");
    let split = parts
        .next()
        .and_then(|s| s.split(" SPLIT ").nth(1))
        .unwrap_or(label);
    match parts.next().and_then(|d| d.strip_suffix(" SESSION")) {
        Some(delta) => line!("Split {split} · {delta} vs session best"),
        None => line!("Split {split}"),
    }
}

/// Finish comparison from the record summary, in friendly mixed case.
pub(crate) fn pretty_finish(label: &str) -> (Line, bool) {
    if let Some(delta) = label.strip_prefix("NEW BEST / ") {
        (line!("New personal best! {delta}"), true)
    } else if let Some(delta) = label.strip_prefix("VS BEST / ") {
        (line!("{delta} vs your best"), false)
    } else {
        (
            match label {
                "FIRST RECORDED TIME" => line!("First recorded time"),
                "MATCHED BEST TIME" => line!("Matched your best, to the hundredth"),
                _ => line!("Time comparison unavailable"),
            },
            false,
        )
    }
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
pub(crate) fn stopwatch(seconds: f32) -> String {
    stopwatch_line(seconds).as_str().to_owned()
}

pub(crate) fn stopwatch_line(seconds: f32) -> Line {
    let centis = (seconds.max(0.) * 100.).round() as u64;
    line!(
        "{}:{:02}.{:02}",
        centis / 6000,
        centis / 100 % 60,
        centis % 100
    )
}

/// What the bottom of the screen teaches right now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Tip {
    /// Roll / brake / overview key caps, fading as the player learns.
    Controls(f32),
    /// After two falls in one section: brake and recover.
    Brake,
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
    /// Countdown hook: attempt, target and best.
    pub countdown_line: &'a str,
}

/// Browser fullscreen pins a 44×44 menu button inset 8 px at the top-right;
/// the HUD keeps this corner (x > w - 72, y < 60) free of text and controls.
pub(crate) const MENU_CORNER: (f32, f32) = (72., 60.);

fn wide(w: f32) -> bool {
    w >= 560.
}

/// Styles for the crystals count and the clock.
fn status_styles(w: f32) -> (TextStyle, TextStyle) {
    if wide(w) {
        (look::body(16., GOLD), look::punch(24., INK))
    } else {
        (look::body(16., GOLD), look::punch(16., INK))
    }
}

/// Size of the crystals/clock card for these labels.
fn status_size(w: f32, gems: &str, time: &str) -> Vec2 {
    let (g, t) = status_styles(w);
    vec2(
        16. + text_width(gems, g) + 18. + text_width(time, t) + 16.,
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

fn chip_width(label: &str) -> f32 {
    28. + text_width(label, look::body(16., INK)) + 14.
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

pub(crate) fn draw(hud: &Hud) {
    let Hud { run, course, .. } = *hud;
    let (w, h) = (screen_width(), screen_height());
    let wide = wide(w);
    let motion = hud.juice.motion();

    // Crystals and the clock: the two numbers that matter most.
    let gems = line!("♦ {}/{}", run.gems(), run.level.gems.len());
    let time = clock(run.elapsed);
    let size = status_size(w, gems.as_str(), time.as_str());
    let (cx, cy) = status_origin(w, size.x);
    let card = Rect::new(cx, cy, size.x, size.y);
    look::glass(card, 12., 1.);
    let all = run.gems() == run.level.gems.len();
    let mid = cy + size.y * 0.5;
    let (g, t) = status_styles(w);
    if all {
        shapes::glow_disc(vec2(cx + 22., mid), 18., look::fade(GOLD, 0.35));
    }
    look::draw(gems.as_str(), cx + 16., g.centered_baseline(mid), g);
    look::draw(
        time.as_str(),
        cx + size.x - 16. - text_width(time.as_str(), t),
        t.centered_baseline(mid),
        t,
    );

    // Medal pace: which medal is still on, and how long it stays on.
    let mut stack = cy + size.y + 6.;
    if wide || h >= 300. {
        let (medal, deadline) = course.pace(run.elapsed, run.falls);
        let name = match medal {
            "GOLD" => "Gold",
            "SILVER" => "Silver",
            _ => "Bronze",
        };
        let counting = hud.trial.countdown().is_some();
        let label = match deadline {
            Some(deadline) if deadline - run.elapsed < 5. && !counting => {
                line!("{name} pace · {:.1}s left", deadline - run.elapsed)
            }
            Some(deadline) => line!("{name} pace · {:.0}s left", (deadline - run.elapsed).ceil()),
            None => line!("Bronze · just finish!"),
        };
        let urgent = deadline.is_some_and(|d| d - run.elapsed < 5.) && !counting;
        let chip_w = chip_width(label.as_str());
        let (x, y) = if wide {
            (pace_right(w, hud.touch) - chip_w, 58.)
        } else {
            (12., stack)
        };
        stack = y + CHIP_H + 6.;
        chip(x, y, chip_w);
        let dot = juice::medal_color(medal);
        let pulse = if urgent && motion > 0. {
            1. + 0.3 * (hud.juice.time * 9.).sin().abs()
        } else {
            1.
        };
        let centre = vec2(x + 15., y + CHIP_H * 0.5);
        shapes::glow_disc(centre, 11. * pulse, look::fade(dot, 0.45));
        shapes::fill_circle(centre, 4.5 * pulse, dot);
        let style = look::body(16., if urgent { juice::CORAL } else { INK });
        look::draw(
            label.as_str(),
            x + 28.,
            style.centered_baseline(y + CHIP_H * 0.5),
            style,
        );
    }

    // Checkpoint pips and falls under the course button.
    if h >= 240. && (wide || h >= 300.) {
        let (x, y) = if wide { (12., 58.) } else { (12., stack) };
        let total = run.level.checkpoints.len();
        let pips = total.min(8);
        let label_style = look::body(16., MUTED);
        let falls = line!("Falls {}", run.falls);
        let falls_style = look::body(16., if run.falls == 0 { INK } else { juice::CORAL });
        let count = (total > 8).then(|| line!("{}/{}", run.checkpoint, total));
        let mut width = 14. + text_width("CP", label_style) + 12.;
        width += match &count {
            Some(c) => text_width(c.as_str(), label_style) + 10.,
            None => pips as f32 * 16.,
        };
        width += text_width(falls.as_str(), falls_style) + 14.;
        if run.secret_found {
            width += 22.;
        }
        chip(x, y, width);
        let base = y + CHIP_H * 0.5;
        let mut cursor = x + 14.;
        cursor += look::draw(
            "CP",
            cursor,
            label_style.centered_baseline(base),
            label_style,
        ) + 14.;
        if let Some(c) = &count {
            let s = look::body(16., TEAL);
            cursor += look::draw(c.as_str(), cursor - 4., s.centered_baseline(base), s) + 10.;
        } else {
            for i in 0..pips {
                let p = vec2(cursor + i as f32 * 16., base);
                if i < run.checkpoint {
                    shapes::fill_circle(p, 5., TEAL);
                } else if i == run.checkpoint {
                    shapes::glow_disc(p, 10., look::fade(GOLD, 0.35));
                    shapes::stroke_circle(p, 5., 2., GOLD);
                } else {
                    shapes::stroke_circle(p, 5., 1.5, look::fade(INK, 0.45));
                }
            }
            cursor += pips as f32 * 16. - 2.;
        }
        cursor += look::draw(
            falls.as_str(),
            cursor,
            falls_style.centered_baseline(base),
            falls_style,
        );
        if run.secret_found {
            let s = look::body(16., juice::VIOLET);
            look::draw("♦", cursor + 8., s.centered_baseline(base), s);
        }
    }

    // One centred message at a time: countdown, then sim notices, then banners.
    let top = hud_bottom(w, h) + 20.;
    let centre_y = (h * 0.3).max(top + 36.).min(h - 64.);
    if let Some(count) = hud.trial.countdown() {
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
        if !hud.countdown_line.is_empty() {
            let s = look::fit(hud.countdown_line, w - 64., look::body(16., INK));
            let lw = text_width(hud.countdown_line, s);
            let y = centre_y + r + 14.;
            chip((w - lw) * 0.5 - 16., y, lw + 32.);
            look::draw(
                hud.countdown_line,
                (w - lw) * 0.5,
                s.centered_baseline(y + CHIP_H * 0.5),
                s,
            );
        }
    } else if hud.trial.showing_go() {
        let c = vec2(w * 0.5, centre_y);
        if motion > 0. {
            shapes::glow_disc(c, 90., look::fade(GOLD, 0.35));
        }
        let style =
            look::centered(look::punch(64., GOLD).with_outline(look::fade(look::SHADOW, 0.9)));
        look::draw("GO!", c.x, style.centered_baseline(c.y), style);
    } else if let Some((title, detail)) = notice(run, hud.trial, hud.player, hud.juice) {
        let color = if run.recovered { juice::CORAL } else { TEAL };
        message(w, centre_y, title.as_str(), detail.as_str(), color, 1.);
    } else if let Some(banner) = &hud.juice.banner {
        message(
            w,
            centre_y,
            &banner.head,
            &banner.sub,
            banner.color,
            banner.alpha(),
        );
    }

    // Bottom line: the overview's route guidance, otherwise onboarding tips.
    if hud.overview && !hud.touch {
        let mut l = goal(run);
        let _ = write!(l, " · {}", course.challenge);
        bottom_line(w, h, l.as_str(), 1.);
    } else if let Some(tip) = hud.tip
        && !hud.touch
    {
        tips(w, h, hud.player, tip);
    }
}

fn message(w: f32, y: f32, title: &str, detail: &str, accent: Color, alpha: f32) {
    if alpha <= 0. {
        return;
    }
    let width = (w - 24.).min(480.);
    let rect = Rect::new((w - width) * 0.5, y - 34., width, 68.);
    look::glass(rect, 16., alpha);
    shapes::fill_rounded_rect(
        Rect::new(rect.x + 8., rect.y + 12., 4., rect.h - 24.),
        2.,
        look::fade(accent, alpha),
    );
    let t = look::fit(
        title,
        width - 40.,
        look::centered(look::body(24., look::fade(INK, alpha))),
    );
    look::draw(title, w * 0.5, t.centered_baseline(rect.y + 24.), t);
    if !detail.is_empty() {
        let tint = shapes::mix(accent, INK, 0.35);
        let d = look::fit(
            detail,
            width - 40.,
            look::centered(look::body(16., look::fade(tint, alpha))),
        );
        look::draw(detail, w * 0.5, d.centered_baseline(rect.y + 50.), d);
    }
}

fn bottom_line(w: f32, h: f32, label: &str, alpha: f32) {
    let s = look::fit(label, w - 64., look::body(16., look::fade(INK, alpha)));
    let lw = text_width(label, s);
    let y = h - 18. - CHIP_H;
    look::glass(
        Rect::new((w - lw) * 0.5 - 16., y, lw + 32., CHIP_H),
        CHIP_H * 0.5,
        alpha,
    );
    look::draw(
        label,
        (w - lw) * 0.5,
        s.centered_baseline(y + CHIP_H * 0.5),
        s,
    );
}

/// Key-cap onboarding that follows remaps; drops prompts that do not fit.
fn tips(w: f32, h: f32, player: &PlayerInput, tip: Tip) {
    let theme = look::theme();
    let (roll, arrows) = roll_keys(player);
    let brake = hint_line(player, "brake");
    let overview = hint_line(player, "overview");
    let recover = hint_line(player, "recover");
    let roll_keys: [&str; 2] = [roll.as_str(), "←↑↓→"];
    let rolls: &[&str] = if arrows { &roll_keys } else { &roll_keys[..1] };
    let (alpha, prompts): (f32, [(&[&str], &str); 3]) = match tip {
        Tip::Controls(alpha) => (
            alpha,
            [
                (rolls, "Roll"),
                (&[brake.as_str()], "Brake"),
                (&[overview.as_str()], "Overview"),
            ],
        ),
        Tip::Brake => (
            1.,
            [
                (&[brake.as_str()], "Brake into turns"),
                (&[recover.as_str()], "Back to checkpoint"),
                (&[], ""),
            ],
        ),
    };
    if alpha <= 0. {
        return;
    }
    let theme = theme.faded(alpha);
    let gap = 24.;
    let mut count = 0;
    let mut total = 0.;
    for (keys, action) in prompts.iter().filter(|(k, _)| !k.is_empty()) {
        let width = look::prompt_width(keys, action, &theme);
        if total + width + if count > 0 { gap } else { 0. } > w - 48. {
            break;
        }
        total += width + if count > 0 { gap } else { 0. };
        count += 1;
    }
    if count == 0 {
        return;
    }
    let y = h - 22. - 30.;
    let back = Rect::new((w - total) * 0.5 - 14., y - 7., total + 28., 40.);
    look::glass(back, 20., alpha * 0.85);
    let mut x = (w - total) * 0.5;
    for (keys, action) in prompts.iter().filter(|(k, _)| !k.is_empty()).take(count) {
        x += kit::key_prompt(x, y, keys, action, &theme) + gap;
    }
}

/// A calm pause card with the actions that matter; Controls draws above it.
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
        Rect::new((w - width) * 0.5, h * 0.5 - 92., width, 184.),
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
    let rows = [
        [
            (hint_line(player, "pause"), "Resume"),
            (hint_line(player, "restart"), "Restart"),
        ],
        [
            (hint_line(player, "courses"), "Courses"),
            (line!("F2"), "Controls"),
        ],
    ];
    for (r, row) in rows.iter().enumerate() {
        let widths = row.map(|(k, a)| look::prompt_width(&[k.as_str()], a, &theme));
        let gap = 24.;
        let total = widths[0] + widths[1] + gap;
        let scale_ok = total <= inner.w;
        let y = inner.y + 34. + r as f32 * 38.;
        if scale_ok {
            let mut x = rect.x + (rect.w - total) * 0.5;
            for ((k, a), width) in row.iter().zip(widths) {
                kit::key_prompt(x, y, &[k.as_str()], a, &theme);
                x += width + gap;
            }
        } else {
            for (i, (k, a)) in row.iter().enumerate() {
                let x = rect.x + 12. + i as f32 * (rect.w * 0.5);
                kit::key_prompt(x, y, &[k.as_str()], a, &theme);
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
            // Widest realistic contents: double-digit crystals and a long clock.
            let size = status_size(w, "♦ 12/12", "59:59.9");
            let (x, y) = status_origin(w, size.x);
            clear(w, x, y, size.x, size.y, "status card");
            assert!(x >= 0. && x + size.x <= w);
            let chip_w = chip_width("Silver pace · 4.9s left");
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
    fn record_labels_read_as_friendly_mixed_case() {
        assert_eq!(stopwatch(31.095), "0:31.10");
        assert_eq!(stopwatch(75.004), "1:15.00");
        assert_eq!(
            pretty_split("CP 1 SPLIT 9.00s / -1.00s SESSION"),
            "Split 9.00s · -1.00s vs session best"
        );
        assert_eq!(pretty_split("CP 2 SPLIT 20.00s"), "Split 20.00s");
        assert_eq!(
            pretty_finish("NEW BEST / -1.00s"),
            (line!("New personal best! -1.00s"), true)
        );
        assert_eq!(pretty_finish("VS BEST / +2.00s").0, "+2.00s vs your best");
        assert_eq!(
            pretty_finish("FIRST RECORDED TIME").0,
            "First recorded time"
        );
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
        assert!(goal(&run).as_str().starts_with("Next: checkpoint 1"));
        run.checkpoint = run.level.checkpoints.len();
        assert!(goal(&run).as_str().starts_with("Next: crystal 1"));
        run.collected.fill(true);
        assert!(goal(&run).as_str().starts_with("Next: the finish arch"));
    }

    #[test]
    fn transient_notices_yield_to_overlays_without_losing_tick_owned_lifetime() {
        let mut run = Run::new();
        let mut trial = Trial::new(false);
        let mut player = PlayerInput::transient("marble-notice");
        crate::trial_actions(&mut player);
        let juice = Juice::new();
        let first = notice(&run, &trial, &player, &juice).unwrap();
        assert_eq!(first.0, "3");
        assert_eq!(trial.advance(30., true).steps, 0);
        assert_eq!(notice(&run, &trial, &player, &juice), None);
        assert_eq!(trial.advance(0., false).steps, 0);
        assert_eq!(notice(&run, &trial, &player, &juice), Some(first));

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
        let recovered = notice(&run, &trial, &player, &juice).unwrap();
        assert_eq!(recovered.0, "Back on track");
        assert!(recovered.1.as_str().contains("crystals kept · fall 1"));
        let frozen = (
            run.elapsed,
            run.notice,
            trial.pose(&run),
            trial.visual_time(&run),
        );
        assert_eq!(trial.advance(30., true).steps, 0);
        assert_eq!(notice(&run, &trial, &player, &juice), None);
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
        assert_eq!(notice(&run, &trial, &player, &juice), Some(recovered));
        for _ in 0..360 {
            let frame = trial.advance(1. / 120., false);
            for _ in 0..frame.steps {
                trial.step(&mut run, crate::sim::Controls::default());
            }
        }
        assert_eq!(notice(&run, &trial, &player, &juice), None);
    }
}
