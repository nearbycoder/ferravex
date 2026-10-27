use crate::{
    CREAM, GOLD, TEAL,
    courses::Course,
    juice::{self, Juice},
    sim::Run,
    text,
    trial::Trial,
};
use ferravex_render::{input::PlayerInput, prelude::*};

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

pub(crate) fn goal(run: &Run) -> String {
    let (label, position) = if run.gems() == run.level.gems.len() {
        ("FINISH ARCH".to_owned(), run.level.finish)
    } else if let Some(cp) = run.level.checkpoints.get(run.checkpoint) {
        (format!("CHECKPOINT {}", run.checkpoint + 1), cp.position)
    } else if let Some(index) = run.collected.iter().position(|&collected| !collected) {
        (format!("CRYSTAL {}", index + 1), run.level.gems[index])
    } else {
        ("FINISH ARCH".to_owned(), run.level.finish)
    };
    format!("NEXT {label} / {:.0}m", run.position().distance(position))
}

/// Transient gameplay notices yield to pause/focus/Controls overlays. Their
/// tick-owned lifetime is retained, so resuming shows the remaining notice.
/// Quips and splits come from lines chosen at the accepted event.
pub(crate) fn notice(
    run: &Run,
    trial: &Trial,
    player: &PlayerInput,
    juice: &Juice,
) -> Option<(String, String)> {
    if !trial.session.is_playing() {
        return None;
    }
    if let Some(count) = trial.countdown() {
        Some((count.to_string(), String::new()))
    } else if trial.showing_go() {
        Some(("GO!".to_string(), String::new()))
    } else if run.notice > 0. {
        Some(if run.recovered {
            (
                if juice.fall_line.is_empty() {
                    "BACK ON TRACK"
                } else {
                    juice.fall_line
                }
                .to_string(),
                format!(
                    "{} / CRYSTALS KEPT / FALL {}",
                    if run.checkpoint == 0 {
                        "START".to_string()
                    } else {
                        format!("CHECKPOINT {}", run.checkpoint)
                    },
                    run.falls
                ),
            )
        } else {
            (
                if juice.checkpoint_line.is_empty() {
                    "CHECKPOINT SAVED"
                } else {
                    juice.checkpoint_line
                }
                .to_string(),
                match (&juice.split, run.falls == 0 && run.checkpoint == 1) {
                    (_, true) => format!(
                        "CHECKPOINT {} OF {} / {} RETURNS YOU HERE",
                        run.checkpoint,
                        run.level.checkpoints.len(),
                        hint(player, "recover")
                    ),
                    (Some(split), false) => spell_seconds(split),
                    (None, false) => format!(
                        "CHECKPOINT {} OF {}",
                        run.checkpoint,
                        run.level.checkpoints.len()
                    ),
                },
            )
        })
    } else {
        None
    }
}

/// Keyboard roll keys as one short label, e.g. `WASD / ARROWS`, following remaps.
pub(crate) fn roll_hint(player: &PlayerInput) -> String {
    let keys = ["move_forward", "move_left", "move_back", "move_right"].map(|a| hint(player, a));
    let first: Vec<&str> = keys
        .iter()
        .map(|k| k.split(" / ").next().unwrap_or(""))
        .collect();
    if keys
        .iter()
        .any(|k| k.starts_with("Pad") || k.starts_with("Touch"))
    {
        return "STICK".into();
    }
    let second: Vec<&str> = keys.iter().filter_map(|k| k.split(" / ").nth(1)).collect();
    let mut label = if first.iter().all(|k| k.chars().count() == 1) {
        first.concat()
    } else {
        first.join("/")
    };
    if second == ["Up", "Left", "Down", "Right"] {
        label.push_str(" / ARROWS");
    }
    label.to_uppercase()
}

/// HUD backing ink. Deliberately not the modal overlay's exact (0.07, 0.13)
/// red/green, which the release-Wasm browser contract uses to detect menus;
/// fading HUD alpha could otherwise briefly match it.
const HUD_INK: Color = Color::new(0.06, 0.12, 0.13, 1.);

fn pill(x: f32, y: f32, w: f32, h: f32, alpha: f32) {
    draw_rectangle(
        x,
        y,
        w,
        h,
        Color::new(HUD_INK.r, HUD_INK.g, HUD_INK.b, 0.78 * alpha),
    );
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, c.a * a)
}

fn width_of(label: &str, size: f32) -> f32 {
    measure_text(label, None, size as u16, 1.).width
}

fn clock(seconds: f32) -> String {
    format!("{:02}:{:04.1}", (seconds / 60.) as u32, seconds % 60.)
}

/// `M:SS.cc`: the bitmap font draws a lowercase `s` unit as `S`, which reads
/// like a `5` after digits, so finished times use clock notation instead.
pub(crate) fn stopwatch(seconds: f32) -> String {
    let centis = (seconds.max(0.) * 100.).round() as u64;
    format!(
        "{}:{:02}.{:02}",
        centis / 6000,
        centis / 100 % 60,
        centis % 100
    )
}

/// Spell out a trailing seconds unit after digits ("-1.00s" -> "-1.00 SEC").
pub(crate) fn spell_seconds(label: &str) -> String {
    let mut out = String::with_capacity(label.len() + 8);
    let mut previous = ' ';
    for c in label.chars() {
        if c == 's' && previous.is_ascii_digit() {
            out.push_str(" SEC");
        } else {
            out.push(c);
        }
        previous = c;
    }
    out
}

/// Baseline that vertically centres bitmap text of `size` on `centre`.
fn baseline(centre: f32, size: f32) -> f32 {
    centre + size * 0.4375
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
    /// Onboarding tip and its opacity; `None` once learned.
    pub tip: Option<(String, f32)>,
    /// Countdown hook: attempt, target and best.
    pub countdown_line: &'a str,
}

/// Browser fullscreen pins a 44×44 menu button inset 8 px at the top-right;
/// the HUD keeps this corner (x > w - 72, y < 60) free of text and controls.
pub(crate) const MENU_CORNER: (f32, f32) = (72., 60.);

/// Top-left of the crystals/clock card: inset left of the menu corner on wide
/// screens, stacked under the course button on narrow ones.
fn status_origin(w: f32, card_w: f32) -> (f32, f32) {
    if w >= 560. {
        (w - MENU_CORNER.0 - 12. - card_w, 8.)
    } else {
        // Stacked rows start below the corner, so long clocks may run wide.
        (12., MENU_CORNER.1 + 2.)
    }
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
    if w >= 560. {
        86.
    } else if h >= 300. {
        160.
    } else {
        104.
    }
}

pub(crate) fn draw(hud: &Hud) {
    let Hud { run, course, .. } = *hud;
    let (w, h) = (screen_width(), screen_height());
    let wide = w >= 560.;
    let motion = hud.juice.motion();

    // Gems and the clock: the two numbers that matter most, top right.
    let (gem_size, time_size) = if wide { (20., 26.) } else { (16., 20.) };
    let gems = format!("{}/{}", run.gems(), run.level.gems.len());
    let time = clock(run.elapsed);
    let inner = 18. + 6. + width_of(&gems, gem_size) + 20. + width_of(&time, time_size);
    let card_w = inner + 28.;
    let card_h = if wide { 44. } else { 40. };
    let (cx, cy) = status_origin(w, card_w);
    pill(cx, cy, card_w, card_h, 1.);
    let all = run.gems() == run.level.gems.len();
    let mid = cy + card_h * 0.5;
    draw_poly(cx + 22., mid, 4, 8., 0., if all { GOLD } else { CREAM });
    draw_poly(cx + 22., mid, 4, 5., 0., GOLD);
    text(&gems, cx + 38., baseline(mid, gem_size), gem_size, GOLD);
    text(
        &time,
        cx + card_w - 14. - width_of(&time, time_size),
        baseline(mid, time_size),
        time_size,
        CREAM,
    );

    // Medal pace: which medal is still on, and how long it stays on.
    let row_h = 26.;
    if wide || h >= 300. {
        let (medal, deadline) = course.pace(run.elapsed, run.falls);
        let counting = hud.trial.countdown().is_some();
        let label = match deadline {
            Some(deadline) if deadline - run.elapsed < 5. => {
                format!("{medal} PACE · {:.1} SEC LEFT", deadline - run.elapsed)
            }
            Some(deadline) => format!(
                "{medal} PACE · {:.0} SEC LEFT",
                (deadline - run.elapsed).ceil()
            ),
            None => "BRONZE · JUST FINISH!".into(),
        };
        let urgent = deadline.is_some_and(|d| d - run.elapsed < 5.) && !counting;
        let (label, size, measured) = fitted(&label, w - 60., 13);
        let chip_w = measured + 34.;
        let right = pace_right(w, hud.touch);
        let (x, y) = if wide {
            (right - chip_w, 58.)
        } else {
            (12., cy + card_h + 6.)
        };
        pill(x, y, chip_w, row_h, 1.);
        let dot = juice::medal_color(medal);
        let pulse = if urgent && motion > 0. {
            1. + 0.25 * (hud.juice.time * 9.).sin().abs()
        } else {
            1.
        };
        draw_circle(x + 14., y + row_h * 0.5, 5. * pulse, dot);
        text(
            &label,
            x + 26.,
            baseline(y + row_h * 0.5, size),
            size,
            if urgent { juice::CORAL } else { CREAM },
        );
    }

    // Checkpoint pips and falls under the course button.
    if h >= 240. {
        let (x, y) = if wide {
            (12., 58.)
        } else {
            (12., cy + card_h + 6. + row_h + 6.)
        };
        if !wide && h < 300. {
            // Short phones keep only the essentials above.
        } else {
            let total = run.level.checkpoints.len();
            let falls = format!("FALLS {}", run.falls);
            let pips = total.min(8);
            let mut width = 12. + width_of("CP", 13.) + 10. + pips as f32 * 16. + 10.;
            let cp_text = (total > 8).then(|| format!("{}/{}", run.checkpoint, total));
            if let Some(t) = &cp_text {
                width += width_of(t, 13.);
            }
            width += width_of(&falls, 13.) + 12.;
            if run.secret_found {
                width += 22.;
            }
            pill(x, y, width, row_h, 1.);
            let base = y + row_h * 0.5;
            let mut cursor = x + 12.;
            text("CP", cursor, baseline(base, 13.), 13., CREAM);
            cursor += width_of("CP", 13.) + 14.;
            if let Some(t) = &cp_text {
                text(t, cursor - 4., baseline(base, 13.), 13., TEAL);
                cursor += width_of(t, 13.);
            } else {
                for i in 0..pips {
                    let px = cursor + i as f32 * 16.;
                    if i < run.checkpoint {
                        draw_circle(px, base, 5., TEAL);
                    } else if i == run.checkpoint {
                        draw_circle_lines(px, base, 5., 2., GOLD);
                    } else {
                        draw_circle_lines(px, base, 5., 1.5, with_alpha(CREAM, 0.5));
                    }
                }
                cursor += pips as f32 * 16.;
            }
            text(
                &falls,
                cursor + 2.,
                baseline(base, 13.),
                13.,
                if run.falls == 0 { CREAM } else { juice::CORAL },
            );
            if run.secret_found {
                let gx = cursor + 2. + width_of(&falls, 13.) + 14.;
                draw_poly(gx, base, 4, 6., 0., juice::VIOLET);
            }
        }
    }

    // One centred message at a time: countdown, then sim notices, then banners.
    let top = hud_bottom(w, h) + 20.;
    let centre_y = (h * 0.3).max(top + 32.).min(h - 60.);
    if let Some(count) = hud.trial.countdown() {
        let progress = hud.trial.countdown_progress();
        let scale = if motion > 0. {
            1. + 0.25 * (1. - (progress / 0.18).min(1.))
        } else {
            1.
        };
        let size = (56. * scale / 8.).round() * 8.;
        let label = count.to_string();
        let lw = width_of(&label, size);
        draw_circle(
            w * 0.5,
            centre_y,
            44.,
            Color::new(HUD_INK.r, HUD_INK.g, HUD_INK.b, 0.86),
        );
        draw_circle_lines(w * 0.5, centre_y, 44., 3., GOLD);
        text(
            &label,
            (w - lw) * 0.5,
            baseline(centre_y, size),
            size,
            CREAM,
        );
        if !hud.countdown_line.is_empty() {
            let (line, size, lw) = fitted(hud.countdown_line, w - 48., 14);
            let y = centre_y + 62.;
            pill((w - lw) * 0.5 - 14., y - 16., lw + 28., 26., 1.);
            text(&line, (w - lw) * 0.5, baseline(y - 3., size), size, CREAM);
        }
    } else if hud.trial.showing_go() {
        let size = 64.;
        let lw = width_of("GO!", size);
        text("GO!", (w - lw) * 0.5, baseline(centre_y, size), size, GOLD);
    } else if let Some((title, detail)) = notice(run, hud.trial, hud.player, hud.juice) {
        let color = if run.recovered { juice::CORAL } else { TEAL };
        message(w, centre_y, &title, &detail, color, 1.);
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
        let line = format!("{} / {}", goal(run), course.challenge.to_uppercase());
        bottom_line(w, h, &line, 1.);
    } else if let Some((tip, alpha)) = &hud.tip
        && !hud.touch
    {
        bottom_line(w, h, tip, *alpha);
    }
}

fn message(w: f32, y: f32, title: &str, detail: &str, accent: Color, alpha: f32) {
    let width = (w - 24.).min(460.);
    let x = (w - width) * 0.5;
    draw_rectangle(
        x,
        y - 30.,
        width,
        62.,
        Color::new(HUD_INK.r, HUD_INK.g, HUD_INK.b, 0.86 * alpha),
    );
    draw_rectangle(x, y - 30., 5., 62., with_alpha(accent, alpha));
    centered_in(title, y - 2., 22, with_alpha(CREAM, alpha), width - 28.);
    if !detail.is_empty() {
        centered_in(detail, y + 20., 13, with_alpha(accent, alpha), width - 28.);
    }
}

fn bottom_line(w: f32, h: f32, line: &str, alpha: f32) {
    let (line, size, lw) = fitted(line, w - 56., 14);
    let y = h - 22.;
    pill((w - lw) * 0.5 - 16., y - 17., lw + 32., 28., alpha);
    text(
        &line,
        (w - lw) * 0.5,
        baseline(y - 3., size),
        size,
        with_alpha(CREAM, alpha),
    );
}

/// A calm pause card with the actions that matter; Controls draws above it.
pub(crate) fn draw_pause(player: &PlayerInput, quip: &str) {
    let (w, h) = (screen_width(), screen_height());
    draw_rectangle(0., 0., w, h, Color::new(0.04, 0.08, 0.09, 0.55));
    let width = (w - 24.).min(420.);
    let x = (w - width) * 0.5;
    let y = h * 0.5 - 62.;
    draw_rectangle(
        x,
        y,
        width,
        124.,
        Color::new(HUD_INK.r, HUD_INK.g, HUD_INK.b, 0.94),
    );
    draw_rectangle(x, y, width, 4., GOLD);
    centered_in("PAUSED", y + 40., 30, CREAM, width - 24.);
    centered_in(quip, y + 66., 13, GOLD, width - 24.);
    let actions = format!(
        "{} RESUME / {} RESTART / {} COURSES / F2 CONTROLS",
        hint(player, "pause"),
        hint(player, "restart"),
        hint(player, "courses"),
    );
    centered_in(&actions.to_uppercase(), y + 100., 12, CREAM, width - 24.);
}

#[cfg(test)]
mod tests {
    use super::*;
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
            let wide = w >= 560.;
            let (gem, time) = if wide { (20., 26.) } else { (16., 20.) };
            // Widest realistic contents: double-digit crystals and a long clock.
            let card_w = 18. + 6. + width_of("12/12", gem) + 20. + width_of("59:59.9", time) + 28.;
            let (x, y) = status_origin(w, card_w);
            clear(w, x, y, card_w, if wide { 44. } else { 40. }, "status card");
            assert!(x >= 0. && x + card_w <= w);
            let (_, _, measured) = fitted("SILVER PACE · 52 SEC LEFT", w - 60., 13);
            let chip_w = measured + 34.;
            for touch in [false, true] {
                let (px, py) = if wide {
                    (pace_right(w, touch) - chip_w, 58.)
                } else {
                    (12., y + 46.)
                };
                clear(w, px, py, chip_w, 26., "pace chip");
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
    fn displayed_times_never_end_in_an_ambiguous_s_after_digits() {
        assert_eq!(stopwatch(31.095), "0:31.10");
        assert_eq!(stopwatch(75.004), "1:15.00");
        assert_eq!(spell_seconds("NEW BEST / -1.00s"), "NEW BEST / -1.00 SEC");
        assert_eq!(
            spell_seconds("CP 1 SPLIT 9.00s / -1.00s SESSION"),
            "CP 1 SPLIT 9.00 SEC / -1.00 SEC SESSION"
        );
        assert_eq!(spell_seconds("SESSION"), "SESSION");
    }

    #[test]
    fn roll_hint_follows_remaps() {
        let mut player = PlayerInput::preset("marble-run-roll", Preset::Racer);
        assert_eq!(roll_hint(&player), "WASD / ARROWS");
        player
            .rebind(
                "move_forward",
                vec![Binding::digital(InputCode::Key(KeyCode::I as u16))],
            )
            .unwrap();
        assert_eq!(roll_hint(&player), "IASD");
    }

    #[test]
    fn guidance_names_checkpoint_then_missing_crystals_then_finish() {
        let mut run = Run::new();
        assert!(goal(&run).starts_with("NEXT CHECKPOINT 1"));
        run.checkpoint = run.level.checkpoints.len();
        assert!(goal(&run).starts_with("NEXT CRYSTAL 1"));
        run.collected.fill(true);
        assert!(goal(&run).starts_with("NEXT FINISH ARCH"));
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
        assert_eq!(recovered.0, "BACK ON TRACK");
        assert!(recovered.1.contains("CRYSTALS KEPT / FALL 1"));
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
