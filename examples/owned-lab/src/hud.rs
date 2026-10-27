//! Immediate overlay reuses the host's existing UI vertex stream.
use super::*;
use ferravex_engine::{
    play::objective::ObjectiveProgress,
    render::{self, input::PlayerInput},
};

fn fit(text: &str, width: f32, metrics: render::BitmapFontMetrics) -> &str {
    let mut end = text.len();
    while end > 0 && metrics.measure_line(&text[..end]).unwrap().width > width {
        end = text[..end].char_indices().next_back().unwrap().0;
    }
    &text[..end]
}

// Keep ordinary long remaps and Unbound readable rather than silently cutting
// off the action's binding. Exceptionally large profiles get an explicit route
// to Controls; neither UI storage nor the panel can grow without a bound.
fn wrap(mut text: &str, width: f32, metrics: render::BitmapFontMetrics) -> Vec<&str> {
    let mut lines = Vec::with_capacity(3);
    while !text.is_empty() && lines.len() < 3 {
        let prefix = fit(text, width, metrics);
        let end = if prefix.len() < text.len() {
            prefix
                .rfind(' ')
                .filter(|&end| end != 0)
                .unwrap_or(prefix.len())
        } else {
            prefix.len()
        };
        if end == 0 {
            break;
        }
        lines.push(&text[..end]);
        text = text[end..].trim_start();
    }
    if !text.is_empty() {
        // Do not present a partial final key name as if it were the full binding.
        if let Some(last) = lines.last_mut() {
            *last = "More in Controls";
        }
    }
    lines
}

fn direction(yaw: f32) -> &'static str {
    let slot = ((yaw.rem_euclid(std::f32::consts::TAU) / std::f32::consts::FRAC_PI_4).round()
        as usize)
        % 8;
    ["N", "NE", "E", "SE", "S", "SW", "W", "NW"][slot]
}

const INK: render::Color = render::Color::new(0.94, 0.93, 0.86, 1.);
const MUTED: render::Color = render::Color::new(0.70, 0.74, 0.70, 1.);
const GOLD: render::Color = render::Color::new(0.91, 0.77, 0.47, 1.);
const CORAL: render::Color = render::Color::new(0.94, 0.47, 0.37, 1.);
const TEAL: render::Color = render::Color::new(0.45, 0.84, 0.77, 1.);
const PANEL: render::Color = render::Color::new(0.06, 0.10, 0.13, 0.9);
const SHADOW: render::Color = render::Color::new(0.02, 0.04, 0.05, 0.8);

fn fade(color: render::Color, alpha: f32) -> render::Color {
    render::Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0., 1.))
}

fn metrics(size: u16) -> render::BitmapFontMetrics {
    render::get_default_font().try_metrics(size, 1.).unwrap()
}

/// Rows are `size + 4` logical pixels tall in the 5x7 bitmap font.
fn row_height(size: u16) -> f32 {
    metrics(size).line_height() + 4.
}

/// Whole-word wrapping for authored copy, bounded to `max` lines.
fn lines(text: &str, width: f32, size: u16, max: usize) -> Vec<&str> {
    let metrics = metrics(size);
    let mut out = Vec::with_capacity(max);
    let mut rest = text.trim();
    while !rest.is_empty() && out.len() < max {
        let prefix = fit(rest, width, metrics);
        let end = if prefix.len() < rest.len() {
            prefix
                .rfind(' ')
                .filter(|&end| end != 0)
                .unwrap_or(prefix.len())
        } else {
            prefix.len()
        };
        if end == 0 {
            break;
        }
        out.push(&rest[..end]);
        rest = rest[end..].trim_start();
    }
    out
}

fn text(copy: &str, x: f32, top: f32, size: u16, color: render::Color) -> f32 {
    render::text::draw_text_styled(
        copy,
        x,
        top + metrics(size).baseline(),
        render::text::TextStyle::modern(f32::from(size), color),
    )
    .width
}

/// Floating text over the 3D scene gets a hard drop shadow for contrast.
fn shadowed(copy: &str, x: f32, top: f32, size: u16, color: render::Color) {
    let offset = (f32::from(size) / 16.).max(2.);
    text(copy, x + offset, top + offset, size, fade(SHADOW, color.a));
    text(copy, x, top, size, color);
}

fn centered(copy: &str, center: f32, top: f32, size: u16, color: render::Color, shadow: bool) {
    let width = render::text::text_width(
        copy,
        render::text::TextStyle::modern(f32::from(size), color),
    );
    if shadow {
        shadowed(copy, center - width * 0.5, top, size, color);
    } else {
        text(copy, center - width * 0.5, top, size, color);
    }
}

/// `1234567` as `1,234,567`.
pub(super) fn thousands(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Left/right/ahead and distance from the player's eye to a world point.
fn bearing(player: &ferravex_fps::Player, target: Vec3) -> (&'static str, f32) {
    let delta = target - player.eye_position();
    let yaw = delta.x.atan2(-delta.z);
    let offset = (yaw - player.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    let side = if offset.abs() < 0.15 {
        "ahead"
    } else if offset.abs() > 2.6 {
        "behind"
    } else if offset < 0. {
        "left"
    } else {
        "right"
    };
    (side, delta.length())
}

/// Screen areas for the objective panel, the sergeant's radio and the results card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    pub panel: (f32, f32, f32),
    /// Maximum objective-panel height before rows are dropped.
    pub panel_max: f32,
    pub radio: (f32, f32, f32),
    pub radio_lines: usize,
    pub card: (f32, f32, f32),
    pub card_max: f32,
    pub compact: bool,
}

pub(super) fn layout(width: f32, height: f32) -> Option<Layout> {
    if width < 180. || height < 260. {
        return None;
    }
    let wide = width >= 800.;
    let compact = !wide && height < 480.;
    // Compact panels sit beside the crosshair, so keep them left of the score pops.
    let panel_width = if compact {
        (width * 0.45).max(220.).min(width - 32.).min(340.)
    } else {
        (width - 32.).min(340.)
    };
    let panel_x = if wide { width - panel_width - 80. } else { 16. };
    let panel_y = if compact {
        height - 200.
    } else if wide {
        16.
    } else {
        152.
    };
    // Narrow touch layouts keep the guide in y152..294, above the action rings.
    let panel_max = if compact {
        60.
    } else if wide {
        260.
    } else {
        142.
    };
    let (radio, radio_lines) = if wide {
        ((16., 16., (panel_x - 32.).min(460.)), 3)
    } else if compact {
        ((16., 8., (width - 32.).min(460.)), 3)
    } else {
        ((16., 16., width - 32.), 4)
    };
    // The panel and radio hide while the card is up, so it can be generous.
    let card_width = (width - 32.).min(480.);
    let card_y = if wide {
        (height * 0.14).max(16.)
    } else if compact {
        8.
    } else {
        16.
    };
    let card_max = if wide {
        (height - card_y - 96.).min(420.)
    } else if compact {
        height - card_y - 96.
    } else {
        // Clear the highest touch action ring (UP: y = h - 309, radius 34).
        (height - 351. - card_y).max(270.)
    };
    Some(Layout {
        panel: (panel_x, panel_y, panel_width),
        panel_max,
        radio,
        radio_lines,
        card: ((width - card_width) * 0.5, card_y, card_width),
        card_max,
        compact,
    })
}

enum Row {
    Text(String, u16, render::Color),
    /// Left and right-aligned copy on one line (score and clock).
    Pair(String, String, u16, render::Color, render::Color),
    Bar(f32, render::Color),
}

fn drill_hint(input: &PlayerInput, index: usize) -> String {
    let mut buffer = [0; 256];
    let mut first = |name: &str| {
        input.format_hint(name, &mut buffer).map_or_else(
            |_| "?".to_owned(),
            |hint| hint.split(" / ").next().unwrap_or("?").to_owned(),
        )
    };
    match index {
        0 => {
            let keys = [
                first("move_forward"),
                first("move_left"),
                first("move_back"),
                first("move_right"),
            ];
            if keys.iter().all(|k| k.len() == 1) {
                format!("Move: {}", keys.concat())
            } else {
                format!(
                    "Move: {} / {} / {} / {}",
                    keys[0], keys[1], keys[2], keys[3]
                )
            }
        }
        1 => {
            let mut buffer = [0; 256];
            format!(
                "Fire: {}",
                input.format_hint("fire", &mut buffer).unwrap_or("Unbound")
            )
        }
        2 => {
            let mut buffer = [0; 256];
            format!(
                "Reload: {}",
                input
                    .format_hint("reload", &mut buffer)
                    .unwrap_or("Unbound")
            )
        }
        _ => "Aim for the bullseye. Headshots pay extra.".to_owned(),
    }
}

fn panel_rows(lab: &Lab, input: &PlayerInput, view: u32, compact: bool) -> Vec<Row> {
    let mut rows = full_rows(lab, input, view);
    if compact {
        // Two short rows: drop gold section labels and secondary detail, and
        // shrink the score so the essentials survive.
        rows.retain(
            |row| !matches!(row, Row::Text(_, 14, color) if *color == GOLD || *color == TEAL),
        );
        for row in &mut rows {
            if let Row::Pair(_, _, size, ..) = row {
                *size = 16;
            }
        }
        rows.retain(|row| !matches!(row, Row::Bar(..)));
    }
    rows
}

fn full_rows(lab: &Lab, input: &PlayerInput, view: u32) -> Vec<Row> {
    let player = lab.game.player(0).unwrap();
    let mut rows = Vec::with_capacity(8);
    if lab.peer.mode != 0 {
        rows.push(Row::Text(lab.peer_status().to_owned(), 14, INK));
        if view == 2 {
            rows.push(Row::Text(
                format!("North up / facing {}", direction(player.yaw)),
                14,
                MUTED,
            ));
        }
        return rows;
    }
    let operator = lab.target_hint.map(|target| {
        let (side, distance) = bearing(player, target.position);
        format!("Operator {}: {side} / {distance:.0} m", target.id)
    });
    let status = lab.qualifier.status();
    match status.phase {
        QualifierPhase::Locked => {
            let done = lab.training_mask().count_ones();
            let next = lab
                .training_objectives()
                .find(|view| !lab.objectives.complete(view.index));
            if lab.drill_notice > 0. {
                rows.push(Row::Text(format!("Drill complete! {done}/4"), 14, TEAL));
            } else {
                rows.push(Row::Text(format!("Drill {} of 4", done + 1), 14, GOLD));
            }
            if let Some(next) = next {
                match next.index {
                    0 => {
                        let ObjectiveProgress::Event { count, .. } = next.progress else {
                            unreachable!()
                        };
                        rows.push(Row::Text(
                            format!("Move {:.1}/5 m", f32::from(count) / 100.),
                            16,
                            INK,
                        ));
                        rows.push(Row::Bar(f32::from(count) / 500., GOLD));
                    }
                    1 => rows.push(Row::Text("Fire a shot".to_owned(), 16, INK)),
                    2 if player.reload_ticks > 0 => {
                        rows.push(Row::Text(
                            format!("Reloading: {:.1}s", f32::from(player.reload_ticks) / 64.),
                            16,
                            INK,
                        ));
                        rows.push(Row::Bar(
                            1. - f32::from(player.reload_ticks)
                                / f32::from(ferravex_fps::RELOAD_TICKS),
                            GOLD,
                        ));
                    }
                    2 => rows.push(Row::Text("Reload".to_owned(), 16, INK)),
                    _ => rows.push(Row::Text("Hit an operator".to_owned(), 16, INK)),
                }
                // Onboarding: only the current drill's control, gone once learned.
                rows.push(Row::Text(drill_hint(input, next.index), 14, MUTED));
                if next.index == 3 {
                    rows.extend(operator.map(|line| Row::Text(line, 14, MUTED)));
                }
            }
        }
        QualifierPhase::Countdown(left) => {
            rows.push(Row::Text("Qualifier".to_owned(), 14, GOLD));
            rows.push(Row::Text(
                format!("Starts in {}", left.div_ceil(ferravex_fps::TICK_RATE)),
                16,
                INK,
            ));
            rows.push(Row::Text(
                "Hit operators. Headshots pay more. Misses break your streak.".to_owned(),
                14,
                MUTED,
            ));
        }
        QualifierPhase::Running(left) => {
            let seconds = left.div_ceil(ferravex_fps::TICK_RATE);
            rows.push(Row::Text("Qualifier".to_owned(), 14, GOLD));
            rows.push(Row::Pair(
                format!("{} pts", thousands(status.score)),
                format!("0:{seconds:02}"),
                24,
                INK,
                if seconds <= 5 { CORAL } else { GOLD },
            ));
            rows.push(Row::Bar(
                left as f32 / qualifier::RUN_TICKS as f32,
                if seconds <= 5 { CORAL } else { GOLD },
            ));
            rows.push(if status.streak > 0 {
                Row::Text(
                    format!("Streak {}  ×{}", status.streak, status.multiplier),
                    14,
                    TEAL,
                )
            } else {
                Row::Text("No streak - land a hit".to_owned(), 14, MUTED)
            });
            rows.extend(operator.map(|line| Row::Text(line, 14, MUTED)));
        }
        QualifierPhase::Debrief(_) => {
            rows.push(Row::Text("Free practice".to_owned(), 14, GOLD));
            rows.push(Row::Text("Ring the bell to requalify".to_owned(), 16, INK));
            let (side, distance) = bearing(player, qualifier::BELL);
            rows.push(Row::Text(
                format!("Bell: {side} / {distance:.0} m"),
                14,
                MUTED,
            ));
            if let (Some(best), Some(last)) = (lab.qualifier_best(), lab.qualifier_result()) {
                rows.push(Row::Text(
                    format!(
                        "Best {} / last {} ({})",
                        thousands(best),
                        thousands(last.score),
                        last.grade().letter()
                    ),
                    14,
                    MUTED,
                ));
            }
        }
    }
    if view == 2 {
        rows.push(Row::Text(
            format!("North up / facing {}", direction(player.yaw)),
            14,
            MUTED,
        ));
    }
    rows
}

/// Largest of `size`, 16 or 14 at which both halves fit on one line.
fn pair_size(left: &str, right: &str, size: u16, width: f32) -> u16 {
    [size, 16, 14]
        .into_iter()
        .find(|&size| {
            let m = metrics(size);
            m.measure_line(left).unwrap().width + m.measure_line(right).unwrap().width + 12.
                <= width
        })
        .unwrap_or(14)
}

/// Wrap, measure and drop trailing rows that would overflow `max_height`.
fn measure_rows(rows: &[Row], width: f32, max_height: f32) -> (Vec<(usize, Vec<&str>)>, f32) {
    let mut out = Vec::with_capacity(rows.len());
    let mut height = 0.;
    for (index, row) in rows.iter().enumerate() {
        let (wrapped, h) = match row {
            Row::Text(copy, size, _) => {
                let wrapped = if *size == 14 && copy.contains(':') {
                    // Binding hints keep the long-remap fallback.
                    wrap(copy, width, metrics(*size))
                } else {
                    lines(copy, width, *size, 3)
                };
                let h = row_height(*size) * wrapped.len() as f32;
                (wrapped, h)
            }
            Row::Pair(_, _, size, ..) => (Vec::new(), row_height(*size)),
            Row::Bar(..) => (Vec::new(), 10.),
        };
        if height + h > max_height && !out.is_empty() {
            break;
        }
        height += h;
        out.push((index, wrapped));
    }
    (out, height)
}

fn draw_panel(lab: &Lab, input: &PlayerInput, view: u32, layout: Layout) {
    let rows = panel_rows(lab, input, view, layout.compact);
    if rows.is_empty() {
        return;
    }
    let (x, y, width) = layout.panel;
    let inner = width - 16.;
    let (visible, height) = measure_rows(&rows, inner, layout.panel_max - 16.);
    render::draw_rectangle(x, y, width, height + 16., PANEL);
    render::draw_rectangle(x, y, 3., height + 16., GOLD);
    let mut top = y + 8.;
    for (index, wrapped) in visible {
        match &rows[index] {
            Row::Text(_, size, color) => {
                for line in wrapped {
                    text(line, x + 10., top + 2., *size, *color);
                    top += row_height(*size);
                }
            }
            Row::Pair(left, right, size, left_color, right_color) => {
                let size = pair_size(left, right, *size, inner - 4.);
                text(left, x + 10., top + 2., size, *left_color);
                let width = metrics(size).measure_line(right).unwrap().width;
                text(
                    right,
                    x + 10. + inner - 4. - width,
                    top + 2.,
                    size,
                    *right_color,
                );
                top += row_height(size);
            }
            Row::Bar(fraction, color) => {
                render::draw_rectangle(x + 10., top + 2., inner - 4., 5., fade(MUTED, 0.3));
                render::draw_rectangle(
                    x + 10.,
                    top + 2.,
                    (inner - 4.) * fraction.clamp(0., 1.),
                    5.,
                    *color,
                );
                top += 10.;
            }
        }
    }
}

fn draw_radio(lab: &Lab, layout: Layout) {
    let Some((line, time)) = lab.juice.radio_line() else {
        return;
    };
    let alpha = (time / 0.35)
        .min((juice::RADIO_TIME - time) / 0.12)
        .clamp(0., 1.);
    let (x, y, max_width) = layout.radio;
    let size = if max_width >= 360. { 16 } else { 14 };
    let wrapped = lines(line, max_width - 24., size, layout.radio_lines);
    // Hug the copy so short lines do not leave an empty slab over the scene.
    let header = metrics(14)
        .measure_line("SGT. GRAVEL on the radio")
        .unwrap()
        .width;
    let widest = wrapped
        .iter()
        .map(|copy| metrics(size).measure_line(copy).unwrap().width)
        .fold(header.min(max_width - 24.), f32::max);
    let width = (widest + 24.).min(max_width);
    let height = 8. + row_height(14) + wrapped.len() as f32 * row_height(size) + 8.;
    render::draw_rectangle(x, y, width, height, fade(PANEL, alpha));
    render::draw_rectangle(x, y, 3., height, fade(GOLD, alpha));
    text(quips::CALLSIGN, x + 12., y + 10., 14, fade(GOLD, alpha));
    if header + 24. <= width {
        text(
            "on the radio",
            x + 12. + metrics(14).measure_line(quips::CALLSIGN).unwrap().width + 10.,
            y + 10.,
            14,
            fade(MUTED, alpha),
        );
    }
    for (i, copy) in wrapped.into_iter().enumerate() {
        text(
            copy,
            x + 12.,
            y + 8. + row_height(14) + 2. + i as f32 * row_height(size),
            size,
            fade(INK, alpha),
        );
    }
}

fn draw_big(lab: &Lab, width: f32, height: f32) {
    let Some((copy, time)) = lab.juice.big_text() else {
        return;
    };
    let policy = lab.feedback.policy();
    let alpha = (time / 0.25).clamp(0., 1.);
    let base: f32 = if copy.len() <= 2 { 64. } else { 40. };
    let fitted = base.min((width - 32.) / (copy.len() as f32 * 0.75));
    // A short punch-in when motion is allowed; reduced motion keeps it still.
    let punch = if policy.reduced_motion() {
        1.
    } else {
        1. + ((time - (juice::BIG_TIME - 0.15)) / 0.15).clamp(0., 1.) * 0.35 * policy.intensity()
    };
    let size = ((fitted * punch / 8.).round() * 8.).clamp(16., 96.) as u16;
    let color = if copy.starts_with('G') { TEAL } else { GOLD };
    centered(
        copy,
        width * 0.5,
        height * 0.34 - f32::from(size) * 0.5,
        size,
        fade(color, alpha),
        true,
    );
}

fn draw_pops(lab: &Lab, width: f32, height: f32) {
    let rise = if lab.feedback.policy().reduced_motion() {
        0.
    } else {
        46.
    };
    let count = lab.juice.pops().count();
    for (slot, (pop, age)) in lab.juice.pops().enumerate() {
        let k = age / juice::POP_TIME;
        let alpha = ((1. - k) / 0.4).clamp(0., 1.);
        let (copy, color) = match pop {
            juice::Pop::Points { points, multiplier } if multiplier > 1 => {
                (format!("+{points} ×{multiplier}"), TEAL)
            }
            juice::Pop::Points { points, .. } => (format!("+{points}"), INK),
            juice::Pop::Headshot { points, multiplier } if multiplier > 1 => {
                (format!("Headshot +{points} ×{multiplier}"), GOLD)
            }
            juice::Pop::Headshot { points, .. } => (format!("Headshot +{points}"), GOLD),
            juice::Pop::Eliminated => ("Eliminated!".to_owned(), CORAL),
            juice::Pop::Bell => ("DING! +500".to_owned(), GOLD),
            juice::Pop::HeadshotStreak(n) => (format!("{n} headshots in a row"), GOLD),
        };
        let size = 16;
        let w = metrics(size).measure_line(&copy).unwrap().width;
        let x = (width * 0.5 + 26.).min(width - w - 8.);
        // Newest sits beside the crosshair; older callouts stack and drift upward.
        let y = height * 0.5 - 10. - (count - 1 - slot) as f32 * row_height(size) - rise * k;
        shadowed(&copy, x, y, size, fade(color, alpha));
    }
}

fn draw_marker(lab: &Lab, width: f32, height: f32) {
    if lab.hit_marker <= 0. || lab.feedback.policy().reduced_flash() {
        return;
    }
    let (inner, outer, thickness, color) = match lab.juice.marker {
        juice::Marker::Hit => (7., 12., 2., INK),
        juice::Marker::Headshot => (6., 15., 3., GOLD),
        juice::Marker::Elimination => (5., 18., 3., CORAL),
    };
    for (dx, dy) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
        render::draw_line(
            width * 0.5 + dx * inner,
            height * 0.5 + dy * inner,
            width * 0.5 + dx * outer,
            height * 0.5 + dy * outer,
            thickness,
            color,
        );
    }
}

/// An empty magazine is the most common "why won't it fire?" moment.
fn draw_ammo_prompt(lab: &Lab, input: &PlayerInput, width: f32, height: f32) {
    let player = lab.game.player(0).unwrap();
    if !player.is_alive() || player.reload_ticks > 0 || player.ammo > 0 {
        return;
    }
    let mut buffer = [0; 256];
    let hint = input
        .format_hint("reload", &mut buffer)
        .unwrap_or("Unbound");
    let hint = hint.split(" / ").next().unwrap_or(hint);
    let copy = format!("Empty! Reload: {hint}");
    centered(&copy, width * 0.5, height * 0.5 + 40., 16, CORAL, true);
}

pub(super) fn grade_color(grade: Grade) -> render::Color {
    match grade {
        Grade::S => GOLD,
        Grade::A => TEAL,
        Grade::B => INK,
        Grade::C => CORAL,
        Grade::D => MUTED,
    }
}

/// The verdict line and the "one more try" hook under the results.
pub(super) fn record_lines(
    result: QualifierResult,
    previous: Option<u32>,
) -> (String, render::Color) {
    match previous {
        None => ("First record set!".to_owned(), GOLD),
        Some(best) if result.score > best => (
            format!("New personal best! +{}", thousands(result.score - best)),
            GOLD,
        ),
        Some(best) if result.score == best => ("Tied your best. Spooky.".to_owned(), INK),
        Some(best) => (
            format!(
                "Best {} / {} to beat it",
                thousands(best),
                thousands(best - result.score)
            ),
            INK,
        ),
    }
}

pub(super) fn next_goal(result: QualifierResult) -> String {
    match result.grade().next() {
        Some((grade, at)) => format!(
            "Next: {} at {} ({} to go)",
            grade.letter(),
            thousands(at),
            thousands(at - result.score)
        ),
        None => "Top grade. Now chase a cleaner streak.".to_owned(),
    }
}

/// "TIME!" gets the screen to itself for a beat before the results card.
pub(super) const CARD_DELAY_TICKS: u32 = 48;

/// Whether the debrief results card is on screen.
pub(super) fn card_visible(lab: &Lab) -> bool {
    lab.peer.mode == 0
        && matches!(
            lab.qualifier.status().phase,
            QualifierPhase::Debrief(since)
                if (CARD_DELAY_TICKS..qualifier::DEBRIEF_CARD_TICKS).contains(&since)
        )
}

fn time_up(lab: &Lab) -> bool {
    lab.peer.mode == 0
        && matches!(
            lab.qualifier.status().phase,
            QualifierPhase::Debrief(since) if since < CARD_DELAY_TICKS
        )
}

fn draw_card(lab: &Lab, layout: Layout) {
    let Some(result) = lab.qualifier_result() else {
        return;
    };
    let (x, y, width) = layout.card;
    let inner = width - 32.;
    let grade = result.grade();
    let previous = lab.previous_best;
    let new_best = previous.is_none_or(|best| result.score > best);
    let (record, record_color) = record_lines(result, previous);
    let quip = if new_best && previous.is_some() {
        quips::NEW_BEST
    } else {
        quips::grade(grade, lab.qualifier.status().runs)
    };
    let player = lab.game.player(0).unwrap();
    let (side, distance) = bearing(player, qualifier::BELL);
    let stats = format!(
        "Hits {} / Heads {} / Elims {}",
        result.hits, result.headshots, result.eliminations
    );
    let more = format!(
        "Accuracy {}% / Best streak {}{}",
        result.accuracy(),
        result.best_streak,
        if result.bell { " / Bell!" } else { "" }
    );
    let call = format!("Ring the bell to go again ({side}, {distance:.0} m)");
    let quote = format!("\"{quip}\"");
    // Short or very narrow screens keep the record and the next action first.
    let tight = layout.compact || width < 300.;
    let badge = if tight { 48. } else { 64. };
    let title_size = if tight { 14 } else { 16 };
    // Beside the badge when every word fits there; otherwise its own row.
    let title_width = width - 32. - badge - 14.;
    let title_beside = grade
        .title()
        .split(' ')
        .all(|word| metrics(title_size).measure_line(word).unwrap().width <= title_width);
    let mut rows = if tight {
        vec![
            Row::Text(record, 14, record_color),
            Row::Text(call, 14, TEAL),
            Row::Text(stats, 14, INK),
            Row::Text(quote, 14, GOLD),
        ]
    } else {
        vec![
            Row::Text(stats, 14, INK),
            Row::Text(more, 14, INK),
            Row::Bar(0., MUTED),
            Row::Text(record, 16, record_color),
            Row::Text(next_goal(result), 14, MUTED),
            Row::Text(call, 14, TEAL),
            Row::Text(quote, 14, GOLD),
        ]
    };
    if !title_beside {
        rows.insert(0, Row::Text(grade.title().to_owned(), title_size, INK));
    }
    let head = 8. + row_height(14) + 6. + badge + 10.;
    let (visible, body) = measure_rows(&rows, inner, layout.card_max - head - 16.);
    let height = head + body + 16.;
    render::draw_rectangle(
        x,
        y,
        width,
        height,
        render::Color::new(0.05, 0.08, 0.10, 0.94),
    );
    render::draw_rectangle(x, y, width, 4., grade_color(grade));
    let heading = if metrics(14).measure_line("Qualifier results").unwrap().width <= inner {
        "Qualifier results"
    } else {
        "Results"
    };
    text(heading, x + 16., y + 12., 14, GOLD);
    let run = format!("Run {}", lab.qualifier.status().runs);
    let run_width = metrics(14).measure_line(&run).unwrap().width;
    if metrics(14).measure_line(heading).unwrap().width + run_width + 12. <= inner {
        text(&run, x + width - 16. - run_width, y + 12., 14, MUTED);
    }
    // Grade badge with the letter, then the silly title and the score.
    let top = y + 8. + row_height(14) + 6.;
    render::draw_rectangle(x + 16., top, badge, badge, fade(grade_color(grade), 0.18));
    render::draw_rectangle_lines(x + 16., top, badge, badge, 2., grade_color(grade));
    let letter = if tight { 32 } else { 48 };
    centered(
        grade.letter(),
        x + 16. + badge * 0.5,
        top + (badge - f32::from(letter) * 7. / 8.) * 0.5,
        letter,
        grade_color(grade),
        false,
    );
    let right = x + 16. + badge + 14.;
    let mut line_top = top + 2.;
    if title_beside {
        for copy in lines(grade.title(), title_width, title_size, 2) {
            text(copy, right, line_top, title_size, INK);
            line_top += row_height(title_size);
        }
    }
    let score = format!("{} pts", thousands(result.score));
    let score_size = [if tight { 16 } else { 24 }, 16, 14]
        .into_iter()
        .find(|&size| metrics(size).measure_line(&score).unwrap().width <= title_width)
        .unwrap_or(14);
    text(
        &score,
        right,
        (top + badge - row_height(score_size)).max(line_top),
        score_size,
        GOLD,
    );
    let mut top = y + head;
    for (index, wrapped) in visible {
        match &rows[index] {
            Row::Text(_, size, color) => {
                for line in wrapped {
                    text(line, x + 16., top + 2., *size, *color);
                    top += row_height(*size);
                }
            }
            Row::Bar(..) => {
                render::draw_rectangle(x + 16., top + 4., inner, 1., fade(MUTED, 0.4));
                top += 10.;
            }
            Row::Pair(..) => {}
        }
    }
}

// Native builds compile the HUD (keeping its presentation types checked) but
// only the Wasm host and tests draw it.
#[cfg_attr(not(any(target_arch = "wasm32", test)), allow(dead_code))]
pub(super) fn draw(lab: &Lab, input: &PlayerInput, width: f32, height: f32, view: u32) {
    let Some(layout) = layout(width, height) else {
        return;
    };
    if card_visible(lab) {
        draw_card(lab, layout);
    } else if !time_up(lab) {
        draw_panel(lab, input, view, layout);
        if lab.peer.mode == 0 {
            draw_radio(lab, layout);
        }
    }
    if view == 1 {
        draw_marker(lab, width, height);
        draw_pops(lab, width, height);
        draw_ammo_prompt(lab, input, width, height);
    }
    if lab.peer.mode == 0 {
        draw_big(lab, width, height);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checked_hud_width_and_cardinal_directions() {
        let metrics = render::get_default_font().try_metrics(14, 1.).unwrap();
        assert!(
            metrics
                .measure_line(fit("Controller button 10 / Space", 80., metrics))
                .unwrap()
                .width
                <= 80.
        );
        assert_eq!(fit("ééé", 11., metrics), "é");
        assert_eq!(direction(0.), "N");
        assert_eq!(direction(std::f32::consts::FRAC_PI_2), "E");
        assert_eq!(direction(-std::f32::consts::FRAC_PI_2), "W");
    }

    #[test]
    fn journey200_199_long_current_bindings_unbound_and_orientation_remain_whole() {
        use render::input::{ActionKind, Binding, InputCode};
        let mut input = PlayerInput::transient("journey200-199-hints");
        input
            .action(
                "reload",
                "Reload",
                ActionKind::Button,
                vec![
                    Binding::digital(InputCode::Key(0xffe4)),
                    Binding::digital(InputCode::Key(0xffe2)),
                ],
            )
            .unwrap();
        let mut buffer = [0; 256];
        let metrics = render::get_default_font().try_metrics(14, 1.).unwrap();
        // 320px short host minus its 44px native pan surface, panel margins
        // and text inset: logical dimensions, independent of framebuffer DPR.
        let width = 320. - 44. - 32. - 16.;
        let hint = input.format_hint("reload", &mut buffer).unwrap();
        let copy = format!("Reload: {hint}");
        let lines = wrap(&copy, width, metrics);
        assert!(lines.len() > 1);
        assert_eq!(lines.join(" "), "Reload: Right Ctrl / Right Shift");
        assert!(
            lines
                .iter()
                .all(|line| metrics.measure_line(line).unwrap().width <= width)
        );
        input.rebind("reload", vec![]).unwrap();
        let hint = input.format_hint("reload", &mut buffer).unwrap();
        assert_eq!(
            wrap(&format!("Reload: {hint}"), width, metrics),
            ["Reload: Unbound"]
        );
        for yaw in [0., std::f32::consts::FRAC_PI_4, std::f32::consts::PI] {
            let copy = format!("North up / facing {}", direction(yaw));
            assert_eq!(wrap(&copy, width, metrics), [copy.as_str()]);
        }
        let huge = "Right Ctrl / ".repeat(64);
        let bounded = wrap(&huge, width, metrics);
        assert_eq!(bounded.len(), 3);
        assert_eq!(bounded[2], "More in Controls");
    }
}
