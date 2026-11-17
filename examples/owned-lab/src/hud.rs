//! Canvas overlay for the Courtyard, reusing the host's UI vertex stream.
//!
//! Everything reads at a glance: the four drills are icon badges with the one
//! control you need as key caps, the qualifier is a score, a ring timer and
//! streak pips, Sgt. Gravel speaks in short radio callouts, score pops are
//! icons and numbers beside the crosshair, and the results card is a big
//! letter grade over icon stats. The DOM strip below the canvas keeps the
//! health, ammo and match-time text for assistive technology.
//!
//! Drawing is allocation-free: copy is formatted into a fixed [`Line`].
use super::*;
use ferravex_engine::{
    play::objective::ObjectiveProgress,
    render::{
        self,
        icons::{self, Icon, IconStyle},
        input::PlayerInput,
        prelude::{Rect, Vec2, vec2},
        shapes::{self, Cap, Stroke},
        text::{TextStyle, draw_text_styled, text_width},
        ui::kit::{self, KitTheme},
    },
};
use std::fmt::Write as _;

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
/// Unlit track for pips, rings and meters.
const TRACK: render::Color = render::Color::new(1., 1., 1., 0.16);
const ICON_SHADOW: render::Color = render::Color::new(0.02, 0.04, 0.05, 0.55);

fn fade(color: render::Color, alpha: f32) -> render::Color {
    render::Color::new(color.r, color.g, color.b, color.a * alpha.clamp(0., 1.))
}

fn metrics(size: u16) -> render::BitmapFontMetrics {
    render::get_default_font().try_metrics(size, 1.).unwrap()
}

fn style(size: f32, color: render::Color) -> TextStyle {
    TextStyle::modern(size, color)
}

fn width_of(copy: &str, size: f32) -> f32 {
    text_width(copy, style(size, INK))
}

/// Draws `copy` with its left edge at `x`, vertically centred on `cy`;
/// returns the drawn width.
fn label(copy: &str, x: f32, cy: f32, size: f32, color: render::Color) -> f32 {
    let style = style(size, color);
    draw_text_styled(copy, x, style.centered_baseline(cy), style).width
}

/// Floating copy over the 3D scene gets a hard drop shadow for contrast.
fn shadowed(copy: &str, x: f32, cy: f32, size: f32, color: render::Color) -> f32 {
    let offset = (size / 16.).max(2.);
    label(copy, x + offset, cy + offset, size, fade(SHADOW, color.a));
    label(copy, x, cy, size, color)
}

fn icon(kind: Icon, center: Vec2, size: f32, color: render::Color) {
    icons::draw_styled(
        kind,
        center,
        size,
        IconStyle::new(color).shadow(fade(ICON_SHADOW, color.a)),
    );
}

/// Fixed-capacity formatted copy, so per-frame HUD text never allocates.
struct Line {
    bytes: [u8; 96],
    len: usize,
}

impl Line {
    fn format(args: std::fmt::Arguments) -> Self {
        let mut line = Self {
            bytes: [0; 96],
            len: 0,
        };
        let _ = line.write_fmt(args);
        line
    }
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}

impl std::fmt::Write for Line {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let room = self.bytes.len() - self.len;
        let mut take = s.len().min(room);
        while !s.is_char_boundary(take) {
            take -= 1;
        }
        self.bytes[self.len..self.len + take].copy_from_slice(&s.as_bytes()[..take]);
        self.len += take;
        if take < s.len() {
            Err(std::fmt::Error)
        } else {
            Ok(())
        }
    }
}

/// `1234567` as `1,234,567`.
#[cfg(test)]
pub(super) fn thousands(value: u32) -> String {
    Grouped(value).to_string()
}

/// Display adapter for digit grouping without allocating.
struct Grouped(u32);

impl std::fmt::Display for Grouped {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut groups = [0_u32; 4];
        let mut count = 0;
        let mut value = self.0;
        loop {
            groups[count] = value % 1000;
            count += 1;
            value /= 1000;
            if value == 0 {
                break;
            }
        }
        write!(f, "{}", groups[count - 1])?;
        for group in groups[..count - 1].iter().rev() {
            write!(f, ",{group:03}")?;
        }
        Ok(())
    }
}

/// Which way to turn toward a world point, and how far it is from the eye.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Ahead,
    Left,
    Right,
    Behind,
}

impl Side {
    fn arrow(self) -> Icon {
        match self {
            Self::Ahead => Icon::ArrowUp,
            Self::Left => Icon::ArrowLeft,
            Self::Right => Icon::ArrowRight,
            Self::Behind => Icon::ArrowDown,
        }
    }
}

/// Left/right/ahead and distance from the player's eye to a world point.
fn bearing(player: &ferravex_fps::Player, target: Vec3) -> (Side, f32) {
    let delta = target - player.eye_position();
    let yaw = delta.x.atan2(-delta.z);
    let offset = (yaw - player.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    let side = if offset.abs() < 0.15 {
        Side::Ahead
    } else if offset.abs() > 2.6 {
        Side::Behind
    } else if offset < 0. {
        Side::Left
    } else {
        Side::Right
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
    /// Wide screens anchor the panel's right edge; others its left edge.
    pub wide: bool,
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
        wide,
    })
}

/// A 0–1 attention pulse on simulation time (frozen in pause); steady when
/// motion is reduced.
fn beat(lab: &Lab, hz: f32) -> f32 {
    if lab.feedback.policy().reduced_motion() {
        1.
    } else {
        render::juice::pulse(lab.time, hz)
    }
}

/// Key caps in the courtyard's stone-and-brass palette.
fn theme() -> KitTheme {
    KitTheme {
        surface: render::Color::new(0.06, 0.10, 0.13, 1.),
        raised: render::Color::new(0.17, 0.22, 0.24, 1.),
        header: render::Color::new(0.23, 0.29, 0.31, 1.),
        ink: INK,
        muted: MUTED,
        accent: GOLD,
        ..KitTheme::default()
    }
}

const CAP_GAP: f32 = 4.;

fn cap_width(key: &str) -> f32 {
    (width_of(key, 16.) + 24.).max(23.)
}

/// Splits a formatted binding ("Mouse 1 / Pad RT") into at most six caps.
fn split_caps(binding: &str) -> Option<([&str; 6], usize)> {
    let mut keys = [""; 6];
    let mut count = 0;
    for part in binding.split(" / ") {
        *keys.get_mut(count)? = part;
        count += 1;
    }
    Some((keys, count))
}

fn caps_width(keys: &[&str]) -> f32 {
    keys.iter().map(|k| cap_width(k) + CAP_GAP).sum()
}

/// Draws key caps from `x`, vertically centred on `cy`; returns the end x.
fn draw_caps(x: f32, cy: f32, keys: &[&str], alpha: f32) -> f32 {
    let theme = theme().faded(alpha);
    let mut cursor = x;
    for key in keys {
        cursor += kit::keycap(cursor, cy - 11.5, key, &theme).w + CAP_GAP;
    }
    cursor
}

/// Horizontal meter: a rounded track and a fill.
fn meter(rect: Rect, fraction: f32, color: render::Color) {
    shapes::fill_rounded_rect(rect, rect.h * 0.5, TRACK);
    let fill = Rect::new(rect.x, rect.y, rect.w * fraction.clamp(0., 1.), rect.h);
    if fill.w > 0.5 {
        shapes::fill_rounded_rect(fill, rect.h * 0.5, color);
    }
}

/// Circular progress clockwise from 12 o'clock over a faint track.
fn ring(center: Vec2, radius: f32, width: f32, fraction: f32, color: render::Color) {
    shapes::stroke_circle(center, radius, width, TRACK);
    let fraction = fraction.clamp(0., 1.);
    if fraction > 0. {
        shapes::stroke_arc(
            center,
            radius,
            -std::f32::consts::FRAC_PI_2,
            fraction * std::f32::consts::TAU,
            Stroke::new(width, color).cap(Cap::Round),
        );
    }
}

/// Small filled dots: `lit` of `total`.
fn pips(x: f32, cy: f32, lit: u32, total: u32, color: render::Color) -> f32 {
    for i in 0..total {
        let center = vec2(x + 5. + i as f32 * 13., cy);
        if i < lit {
            shapes::fill_circle(center, 4.5, color);
        } else {
            shapes::stroke_circle(center, 4., 1.5, fade(color, 0.45));
        }
    }
    total as f32 * 13.
}

/// One drill per badge, in move/fire/reload/hit order.
const DRILL_ICONS: [Icon; 4] = [Icon::Paw, Icon::Flame, Icon::Restart, Icon::Target];

/// The panel surface with its brass edge, hugging `content` (never wider
/// than the layout's panel); returns the inner left x.
fn panel_surface(layout: Layout, content: f32, height: f32) -> f32 {
    let (x, y, max) = layout.panel;
    let width = (content + 24.).min(max);
    let x = if layout.wide { x + max - width } else { x };
    render::draw_rectangle(x, y, width, height, PANEL);
    render::draw_rectangle(x, y, 3., height, GOLD);
    x + 12.
}

/// What the current drill asks for under the badges.
enum Prompt<'a> {
    /// Movement caps (when they fit) and the distance meter.
    Move {
        keys: [&'a str; 4],
        caps: bool,
        meters: f32,
    },
    /// The fire or reload binding as caps, then the drill's icon.
    Caps {
        keys: [&'a str; 6],
        count: usize,
        kind: Icon,
    },
    /// An exceptionally long binding as whole wrapped words.
    Text { lines: Vec<&'a str>, kind: Icon },
    /// The reload in progress.
    Reloading(f32),
    /// Find and hit an operator.
    Operator,
}

const MOVE_METER: f32 = 96.;

/// Four drill badges (done, current, ahead) with the current drill's
/// control and progress underneath, hugging their content.
fn draw_drills(lab: &Lab, input: &PlayerInput, layout: Layout) {
    let (_, y, width) = layout.panel;
    let player = lab.game.player(0).unwrap();
    let mask = lab.training_mask();
    let next = lab
        .training_objectives()
        .find(|view| !lab.objectives.complete(view.index));
    let compact = layout.compact;
    let height = if compact { 60. } else { 92. };
    let room = width - 24.;
    // Badges: done drills are teal checks, the current one pulses gold.
    let badge = if compact { 22. } else { 28. };
    let step = if compact { 34. } else { 44. };
    let mut move_buffers = [[0_u8; 64]; 4];
    let mut buffer = [0; 256];
    let prompt = next.map(|next| match next.index {
        0 => {
            let ObjectiveProgress::Event { count, .. } = next.progress else {
                unreachable!()
            };
            let mut keys = [""; 4];
            for ((key, buffer), action) in keys.iter_mut().zip(&mut move_buffers).zip([
                "move_forward",
                "move_left",
                "move_back",
                "move_right",
            ]) {
                *key = input
                    .format_hint(action, buffer)
                    .map_or("?", |hint| hint.split(" / ").next().unwrap_or("?"));
            }
            let caps = caps_width(&keys) + 4. + MOVE_METER + 52. <= room;
            Prompt::Move {
                keys,
                caps,
                meters: f32::from(count) / 100.,
            }
        }
        2 if player.reload_ticks > 0 => Prompt::Reloading(
            1. - f32::from(player.reload_ticks) / f32::from(ferravex_fps::RELOAD_TICKS),
        ),
        1 | 2 => {
            let action = if next.index == 1 { "fire" } else { "reload" };
            let binding = input.format_hint(action, &mut buffer).unwrap_or("Unbound");
            let kind = DRILL_ICONS[next.index];
            match split_caps(binding) {
                Some((keys, count)) if caps_width(&keys[..count]) + 28. <= room => {
                    Prompt::Caps { keys, count, kind }
                }
                // Exceptionally long profiles: whole words, then Controls.
                _ => {
                    let mut lines = wrap(binding, room - 26., metrics(14));
                    if compact && lines.len() > 1 {
                        lines.truncate(1);
                        lines[0] = "More in Controls";
                    }
                    Prompt::Text { lines, kind }
                }
            }
        }
        _ => Prompt::Operator,
    });
    let badges: f32 = 3. * step + badge;
    let content = match &prompt {
        Some(Prompt::Move { keys, caps, .. }) => {
            (if *caps { caps_width(keys) + 4. } else { 0. }) + MOVE_METER + 52.
        }
        Some(Prompt::Caps { keys, count, .. }) => caps_width(&keys[..*count]) + 28.,
        Some(Prompt::Text { lines, .. }) => {
            26. + lines.iter().map(|l| width_of(l, 14.)).fold(0., f32::max)
        }
        Some(Prompt::Reloading(_)) => 26. + MOVE_METER + 40.,
        Some(Prompt::Operator) => 120.,
        None => 0.,
    };
    let inner = badges.max(content).min(room);
    let left = panel_surface(layout, inner, height);
    let row = y + if compact { 5. } else { 8. } + badge * 0.5;
    // A finished drill's badge pops in; reduced motion keeps it still.
    let fresh = if lab.feedback.policy().reduced_motion() {
        0.
    } else {
        (lab.drill_notice / 2.5).clamp(0., 1.)
    };
    for (index, kind) in DRILL_ICONS.into_iter().enumerate() {
        let center = vec2(left + badge * 0.5 + index as f32 * step, row);
        if index > 0 {
            let from = center.x - step + badge * 0.5 + 2.;
            let color = if mask & (1 << (index - 1)) != 0 {
                fade(TEAL, 0.6)
            } else {
                TRACK
            };
            render::draw_rectangle(from, row - 1., step - badge - 4., 2., color);
        }
        let done = mask & (1 << index) != 0;
        let current = next.is_some_and(|view| view.index == index);
        if done {
            // The drill that just finished pops before settling.
            let pop = if index == mask.count_ones() as usize - 1 && fresh > 0. {
                1. + 0.35 * fresh * fresh
            } else {
                1.
            };
            icons::badge(Icon::Check, center, badge * pop, TEAL, PANEL);
        } else if current {
            let glow = 0.55 + 0.45 * beat(lab, 1.4);
            shapes::fill_circle(center, badge * 0.5, fade(GOLD, 0.18));
            shapes::stroke_circle(center, badge * 0.5 - 1., 2., fade(GOLD, glow));
            icon(kind, center, badge * 0.6, GOLD);
        } else {
            shapes::stroke_circle(center, badge * 0.5 - 1., 1.5, TRACK);
            icon(kind, center, badge * 0.55, fade(MUTED, 0.6));
        }
    }
    let cy = if compact { y + 42. } else { y + 66. };
    match prompt {
        None => {}
        Some(Prompt::Move { keys, caps, meters }) => {
            let mut cursor = left;
            if caps {
                cursor = draw_caps(cursor, cy, &keys, 1.) + 4.;
            }
            let distance = Line::format(format_args!("{meters:.1} m"));
            meter(
                Rect::new(cursor, cy - 3., MOVE_METER, 6.),
                meters / 5.,
                GOLD,
            );
            label(distance.as_str(), cursor + MOVE_METER + 8., cy, 14., MUTED);
        }
        Some(Prompt::Caps { keys, count, kind }) => {
            let end = draw_caps(left, cy, &keys[..count], 1.);
            icon(kind, vec2(end + 10., cy), 20., GOLD);
        }
        Some(Prompt::Text { lines, kind }) => {
            icon(kind, vec2(left + 9., cy), 18., GOLD);
            let top = cy - (lines.len() as f32 - 1.) * 8.;
            for (i, line) in lines.iter().enumerate() {
                label(line, left + 26., top + i as f32 * 16., 14., MUTED);
            }
        }
        Some(Prompt::Reloading(fraction)) => {
            // Reloading: the magazine refills along the meter.
            icon(DRILL_ICONS[2], vec2(left + 9., cy), 18., GOLD);
            meter(
                Rect::new(left + 26., cy - 3., (inner - 26.).min(MOVE_METER + 40.), 6.),
                fraction,
                GOLD,
            );
        }
        Some(Prompt::Operator) => operator_row(lab, left, cy, true),
    }
}

/// The nearest visible operator: a person, a turn arrow and its distance.
/// `target` marks the drill's bullseye goal first.
fn operator_row(lab: &Lab, x: f32, cy: f32, target: bool) {
    let mut cursor = x;
    if target {
        icon(Icon::Target, vec2(cursor + 10., cy), 20., GOLD);
        cursor += 26.;
    }
    let Some(hint) = lab.target_hint else {
        icon(Icon::Search, vec2(cursor + 9., cy), 16., fade(MUTED, 0.7));
        return;
    };
    let player = lab.game.player(0).unwrap();
    let (side, distance) = bearing(player, hint.position);
    icon(Icon::Person, vec2(cursor + 8., cy), 16., MUTED);
    icon(side.arrow(), vec2(cursor + 26., cy), 16., INK);
    let meters = Line::format(format_args!("{distance:.0} m"));
    label(meters.as_str(), cursor + 38., cy, 14., MUTED);
}

/// Qualifier panel: score, a ring timer and streak pips toward the next
/// multiplier, plus the nearest operator while running.
fn draw_qualifier(lab: &Lab, layout: Layout, left_ticks: u32, running: bool) {
    let (_, y, width) = layout.panel;
    let status = lab.qualifier.status();
    let compact = layout.compact;
    let height = if compact {
        60.
    } else if running && lab.target_hint.is_some() {
        96.
    } else {
        74.
    };
    let inner = (width - 24.).min(if compact { 200. } else { 236. });
    let left = panel_surface(layout, inner, height);
    let seconds = left_ticks.div_ceil(ferravex_fps::TICK_RATE);
    let urgent = running && seconds <= 5;
    let clock_color = if urgent { CORAL } else { GOLD };
    // Ring timer on the right: drains clockwise around a clock that swells
    // with each of the final five seconds; the digits sit beside it.
    let radius = if compact { 13. } else { 16. };
    let center = vec2(left + inner - radius - 2., y + 8. + radius);
    let fraction = left_ticks as f32 / qualifier::RUN_TICKS as f32;
    ring(
        center,
        radius,
        3.5,
        if running { fraction } else { 1. },
        clock_color,
    );
    let tick = if urgent && !lab.feedback.policy().reduced_motion() {
        let into =
            1. - (left_ticks % ferravex_fps::TICK_RATE) as f32 / ferravex_fps::TICK_RATE as f32;
        1. + 0.3 * (1. - into).powi(3)
    } else {
        1.
    };
    icon(Icon::Clock, center, radius * 1.05 * tick, clock_color);
    let time = Line::format(format_args!("0:{seconds:02}"));
    let time_size = if compact { 16. } else { 24. };
    let time_width = width_of(time.as_str(), time_size);
    label(
        time.as_str(),
        center.x - radius - 8. - time_width,
        center.y,
        time_size,
        clock_color,
    );
    // Score with a star.
    let score_size = if compact { 16. } else { 24. };
    let score = Line::format(format_args!("{}", Grouped(status.score)));
    let row = y + 8. + radius;
    // The star swells as each score callout lands.
    let landed = lab
        .juice
        .pops()
        .last()
        .filter(|(pop, _)| !matches!(pop, juice::Pop::Eliminated { bonus: 0 }))
        .map_or(0., |(_, age)| (1. - age / 0.25).max(0.));
    let swell = if lab.feedback.policy().reduced_motion() {
        1.
    } else {
        1. + 0.45 * landed
    };
    icon(Icon::Star, vec2(left + 10., row), 20. * swell, GOLD);
    label(score.as_str(), left + 26., row, score_size, INK);
    // Streak: a flame, three pips per multiplier tier and the multiplier.
    let streak_y = if compact { y + 46. } else { y + 58. };
    let hot = status.streak > 0;
    icon(
        Icon::Flame,
        vec2(left + 9., streak_y),
        18.,
        if hot { CORAL } else { fade(MUTED, 0.5) },
    );
    let tier = qualifier::HITS_PER_TIER;
    let lit = if status.multiplier >= qualifier::MAX_MULTIPLIER {
        tier
    } else {
        status.streak % tier
    };
    let end = left + 24. + pips(left + 24., streak_y, lit, tier, TEAL);
    let multiplier = Line::format(format_args!("×{}", status.multiplier));
    label(
        multiplier.as_str(),
        end + 6.,
        streak_y,
        16.,
        if status.multiplier > 1 { TEAL } else { MUTED },
    );
    if running && !compact && lab.target_hint.is_some() {
        operator_row(lab, left, y + 82., false);
    }
}

/// Free practice: the bell to shoot for another run and the personal best.
fn draw_debrief(lab: &Lab, layout: Layout) {
    let (_, y, _) = layout.panel;
    let compact = layout.compact;
    let has_best = lab.qualifier_best().is_some() && !compact;
    let height = if has_best { 66. } else { 40. };
    let left = panel_surface(layout, 150., height);
    let player = lab.game.player(0).unwrap();
    let (side, distance) = bearing(player, qualifier::BELL);
    let row = y + 20.;
    // Shoot the bell (pulsing) that way to go again.
    let swell = 18. + 3. * beat(lab, 1.2);
    icon(Icon::Bell, vec2(left + 10., row), swell, GOLD);
    icon(side.arrow(), vec2(left + 32., row), 16., INK);
    let meters = Line::format(format_args!("{distance:.0} m"));
    let end = left + 44. + label(meters.as_str(), left + 44., row, 16., GOLD);
    icon(Icon::Restart, vec2(end + 14., row), 16., TEAL);
    if let (true, Some(best)) = (has_best, lab.qualifier_best()) {
        let row = y + 48.;
        icon(Icon::Trophy, vec2(left + 9., row), 16., GOLD);
        let best = Line::format(format_args!("{}", Grouped(best)));
        let end = left + 24. + label(best.as_str(), left + 24., row, 16., INK);
        if let Some(last) = lab.qualifier_result() {
            grade_chip(vec2(end + 18., row), 20., last.grade());
        }
    }
}

/// A small rounded letter chip in the grade's colour.
fn grade_chip(center: Vec2, size: f32, grade: Grade) {
    let color = grade_color(grade);
    let rect = Rect::new(center.x - size * 0.5, center.y - size * 0.5, size, size);
    shapes::fill_rounded_rect(rect, 4., fade(color, 0.2));
    shapes::stroke_rounded_rect(rect, 4., 1.5, color);
    let width = width_of(grade.letter(), 16.);
    label(grade.letter(), center.x - width * 0.5, center.y, 16., color);
}

/// Peer matches keep the authority/prediction status as text: it is state,
/// not instruction.
fn draw_peer(lab: &Lab, layout: Layout) {
    let (_, y, width) = layout.panel;
    let mut line = Line::format(format_args!("{}", lab.peer_status()));
    let content = (width_of(line.as_str(), 14.) + 26.).min(width - 24.);
    let left = panel_surface(layout, content, 36.);
    icon(Icon::People, vec2(left + 9., y + 18.), 18., GOLD);
    let budget = width - 24. - 26.;
    while width_of(line.as_str(), 14.) > budget && line.len > 0 {
        line.len -= 1;
        while !line.as_str().is_char_boundary(line.len) {
            line.len -= 1;
        }
    }
    label(line.as_str(), left + 26., y + 18., 14., INK);
}

/// Overview compass: north is up; the arrow shows the player's facing.
fn draw_compass(lab: &Lab, width: f32, height: f32) {
    let player = lab.game.player(0).unwrap();
    let center = vec2(width - 44., height * 0.5);
    render::draw_rectangle(center.x - 30., center.y - 40., 60., 92., PANEL);
    shapes::fill_circle(center, 24., fade(TRACK, 0.4));
    shapes::stroke_circle(center, 23., 1.5, fade(GOLD, 0.7));
    // North is a gold bezel marker at the top of the dial.
    shapes::fill_circle(center - vec2(0., 23.), 8., GOLD);
    let n = width_of("N", 14.);
    label("N", center.x - n * 0.5, center.y - 23., 14., PANEL);
    let yaw = player.yaw;
    let heading = vec2(yaw.sin(), -yaw.cos());
    let side = vec2(-heading.y, heading.x);
    // A slim needle: bright toward the facing, dim behind.
    shapes::fill_triangle(
        center + heading * 17.,
        center + side * 4.5,
        center - side * 4.5,
        INK,
    );
    shapes::fill_triangle(
        center - heading * 12.,
        center + side * 4.5,
        center - side * 4.5,
        fade(MUTED, 0.6),
    );
    let facing = direction(yaw);
    let w = width_of(facing, 16.);
    label(facing, center.x - w * 0.5, center.y + 38., 16., INK);
}

fn draw_panel(lab: &Lab, input: &PlayerInput, view: u32, layout: Layout, width: f32, height: f32) {
    if lab.peer.mode != 0 {
        draw_peer(lab, layout);
    } else {
        match lab.qualifier.status().phase {
            QualifierPhase::Locked => draw_drills(lab, input, layout),
            QualifierPhase::Countdown(_) => {
                draw_qualifier(lab, layout, qualifier::RUN_TICKS, false)
            }
            QualifierPhase::Running(left) => draw_qualifier(lab, layout, left, true),
            QualifierPhase::Debrief(_) => draw_debrief(lab, layout),
        }
    }
    if view == 2 {
        draw_compass(lab, width, height);
    }
}

/// Sgt. Gravel's radio chip: a speech badge, his name and a short callout.
fn draw_radio(lab: &Lab, layout: Layout) {
    let Some((line, time)) = lab.juice.radio_line() else {
        return;
    };
    let alpha = (time / 0.35)
        .min((juice::RADIO_TIME - time) / 0.12)
        .clamp(0., 1.);
    let (x, y, max_width) = layout.radio;
    let size = if max_width >= 360. { 16. } else { 14. };
    let text = width_of(line, size).min(max_width - 64.);
    let name = width_of(quips::CALLSIGN, 14.);
    let width = (text.max(name) + 64.).min(max_width);
    let height = 46.;
    render::draw_rectangle(x, y, width, height, fade(PANEL, alpha));
    render::draw_rectangle(x, y, 3., height, fade(GOLD, alpha));
    // The badge leans in as the line arrives.
    let pop = if lab.feedback.policy().reduced_motion() {
        1.
    } else {
        1. + 0.25 * ((time - (juice::RADIO_TIME - 0.2)) / 0.2).clamp(0., 1.)
    };
    icons::badge(
        Icon::Chat,
        vec2(x + 27., y + height * 0.5),
        28. * pop,
        fade(GOLD, alpha),
        fade(PANEL, alpha),
    );
    label(quips::CALLSIGN, x + 50., y + 14., 14., fade(GOLD, alpha));
    label(line, x + 50., y + 32., size, fade(INK, alpha));
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
    let size = ((fitted * punch / 8.).round() * 8.).clamp(16., 96.);
    let color = if copy.starts_with('G') { TEAL } else { GOLD };
    let center = vec2(width * 0.5, height * 0.34);
    // Countdown digits sit in a ring that drains with the beat.
    if copy.len() == 1 && !policy.reduced_flash() {
        let k = (time / juice::BIG_TIME).clamp(0., 1.);
        ring(center, size * 0.75, 4., k, fade(GOLD, alpha));
    }
    let w = width_of(copy, size);
    shadowed(copy, center.x - w * 0.5, center.y, size, fade(color, alpha));
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
        let size = 16.;
        // Newest sits beside the crosshair; older callouts stack and drift upward.
        let cy = height * 0.5 - 2. - (count - 1 - slot) as f32 * 20. - rise * k;
        let x = width * 0.5 + 26.;
        let glyph = |kind: Icon, color: render::Color, at: f32| {
            icon(kind, vec2(at + 9., cy), 18., fade(color, alpha));
            at + 21.
        };
        match pop {
            juice::Pop::Points { points, multiplier } => {
                let copy = Line::format(format_args!("+{points}"));
                let end = x + shadowed(copy.as_str(), x, cy, size, fade(INK, alpha));
                multiplier_tag(end + 6., cy, multiplier, alpha);
            }
            juice::Pop::Headshot { points, multiplier } => {
                let at = glyph(Icon::Target, GOLD, x);
                let copy = Line::format(format_args!("+{points}"));
                let end = at + shadowed(copy.as_str(), at, cy, size, fade(GOLD, alpha));
                multiplier_tag(end + 6., cy, multiplier, alpha);
            }
            juice::Pop::Eliminated { bonus } => {
                let at = glyph(Icon::Skull, CORAL, x);
                if bonus > 0 {
                    let copy = Line::format(format_args!("+{bonus}"));
                    shadowed(copy.as_str(), at, cy, size, fade(CORAL, alpha));
                }
            }
            juice::Pop::Bell => {
                let at = glyph(Icon::Bell, GOLD, x);
                let copy = Line::format(format_args!("+{}", qualifier::BELL_BONUS));
                shadowed(copy.as_str(), at, cy, size, fade(GOLD, alpha));
            }
            juice::Pop::HeadshotStreak(n) => {
                // One bullseye per headshot in a row, then a count.
                let shown = n.min(5);
                let mut at = x;
                for _ in 0..shown {
                    at = glyph(Icon::Target, GOLD, at) - 3.;
                }
                if n > shown {
                    let copy = Line::format(format_args!("×{n}"));
                    shadowed(copy.as_str(), at + 4., cy, size, fade(GOLD, alpha));
                }
            }
        }
    }
}

/// A teal `×N` beside a pop when a streak multiplied it.
fn multiplier_tag(x: f32, cy: f32, multiplier: u32, alpha: f32) {
    if multiplier > 1 {
        let copy = Line::format(format_args!("×{multiplier}"));
        shadowed(copy.as_str(), x, cy, 16., fade(TEAL, alpha));
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

/// Weapon state at the crosshair: a refill ring while reloading, a few
/// pips when the magazine runs low and the reload key when it is empty.
fn draw_weapon(lab: &Lab, input: &PlayerInput, width: f32, height: f32) {
    let player = lab.game.player(0).unwrap();
    if !player.is_alive() {
        return;
    }
    let center = vec2(width * 0.5, height * 0.5);
    if player.reload_ticks > 0 {
        let fraction = 1. - f32::from(player.reload_ticks) / f32::from(ferravex_fps::RELOAD_TICKS);
        ring(center, 22., 3., fraction, GOLD);
        return;
    }
    let below = center.y + 40.;
    if (1..=5).contains(&player.ammo) {
        let total = u32::from(player.ammo);
        let span = total as f32 * 13.;
        pips(
            center.x - span * 0.5 - 1.,
            below - 14.,
            total,
            total,
            fade(CORAL, 0.85),
        );
        return;
    }
    if player.ammo > 0 {
        return;
    }
    // Empty: the most common "why won't it fire?" moment.
    let mut buffer = [0; 256];
    let hint = input
        .format_hint("reload", &mut buffer)
        .unwrap_or("Unbound");
    let key = hint.split(" / ").next().unwrap_or(hint);
    let alpha = 0.7 + 0.3 * beat(lab, 2.5);
    let width = cap_width(key) + CAP_GAP + 22.;
    let x = center.x - width * 0.5;
    let end = draw_caps(x, below, &[key], alpha);
    icon(
        Icon::Restart,
        vec2(end + 9., below),
        20.,
        fade(CORAL, alpha),
    );
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

/// Personal-best standing after a run: a few words and numbers.
pub(super) fn record_lines(
    result: QualifierResult,
    previous: Option<u32>,
) -> (String, render::Color) {
    match previous {
        None => ("New record!".to_owned(), GOLD),
        Some(best) if result.score > best => {
            (format!("New best! +{}", Grouped(result.score - best)), GOLD)
        }
        Some(best) if result.score == best => ("Tied!".to_owned(), INK),
        Some(best) => (
            format!("{} · {} to go", Grouped(best), Grouped(best - result.score)),
            INK,
        ),
    }
}

/// The next grade's letter, its threshold and the points still needed.
pub(super) fn next_goal(result: QualifierResult) -> Option<(Grade, String)> {
    result.grade().next().map(|(grade, at)| {
        (
            grade,
            format!("{} · {} to go", Grouped(at), Grouped(at - result.score)),
        )
    })
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

/// One icon-and-number stat for the results card.
fn stat(x: f32, cy: f32, kind: Icon, value: &str, color: render::Color) -> f32 {
    icon(kind, vec2(x + 9., cy), 18., color);
    22. + label(value, x + 22., cy, 16., INK) + 16.
}

fn draw_card(lab: &Lab, layout: Layout) {
    let Some(result) = lab.qualifier_result() else {
        return;
    };
    let (x, y, width) = layout.card;
    let inner = width - 32.;
    let left = x + 16.;
    let grade = result.grade();
    let color = grade_color(grade);
    let previous = lab.previous_best;
    let new_best = previous.is_none_or(|best| result.score > best);
    let quip = if new_best && previous.is_some() {
        quips::NEW_BEST
    } else {
        quips::grade(grade, lab.qualifier.status().runs)
    };
    let tight = layout.compact || width < 300.;
    let badge = if tight { 52. } else { 72. };
    // Rows under the grade header, dropped from the end when space runs out.
    let row = 26.;
    let rows = if tight { 4 } else { 6 };
    let head = 14. + badge + 10.;
    let fit = (((layout.card_max - head - 12.) / row).floor().max(0.) as usize).min(rows);
    let height = head + fit as f32 * row + 12.;
    render::draw_rectangle(
        x,
        y,
        width,
        height,
        render::Color::new(0.05, 0.08, 0.10, 0.94),
    );
    render::draw_rectangle(x, y, width, 4., color);
    // The run number as a small chip in the corner.
    let run = Line::format(format_args!("#{}", lab.qualifier.status().runs));
    let run_width = width_of(run.as_str(), 14.);
    label(
        run.as_str(),
        x + width - 16. - run_width,
        y + 18.,
        14.,
        MUTED,
    );
    // Big letter grade, the silly title and the score.
    let top = y + 14.;
    let fresh = if new_best && !lab.feedback.policy().reduced_flash() {
        0.2 + 0.15 * beat(lab, 1.)
    } else {
        0.18
    };
    render::draw_rectangle(left, top, badge, badge, fade(color, fresh));
    render::draw_rectangle_lines(left, top, badge, badge, 2., color);
    let letter = if tight { 40. } else { 56. };
    let letter_width = width_of(grade.letter(), letter);
    label(
        grade.letter(),
        left + (badge - letter_width) * 0.5,
        top + badge * 0.5 + 1.,
        letter,
        color,
    );
    let right = left + badge + 14.;
    let title_size = if tight { 14. } else { 16. };
    let title_room = x + width - 16. - run_width - 8. - right;
    if width_of(grade.title(), title_size) <= title_room {
        label(grade.title(), right, top + 12., title_size, INK);
    }
    let score = Line::format(format_args!("{}", Grouped(result.score)));
    let score_size = if tight { 16. } else { 32. };
    icon(Icon::Star, vec2(right + 10., top + badge - 18.), 20., GOLD);
    label(
        score.as_str(),
        right + 26.,
        top + badge - 18.,
        score_size,
        GOLD,
    );
    // Body rows, in priority order for tight cards.
    let player = lab.game.player(0).unwrap();
    let (side, distance) = bearing(player, qualifier::BELL);
    let hits = Line::format(format_args!("{}", result.hits));
    let heads = Line::format(format_args!("{}", result.headshots));
    let elims = Line::format(format_args!("{}", result.eliminations));
    let accuracy = Line::format(format_args!("{}%", result.accuracy()));
    let streak = Line::format(format_args!("{}", result.best_streak));
    let (record, record_color) = record_lines(result, previous);
    let goal = next_goal(result);
    let meters = Line::format(format_args!("{distance:.0} m"));
    let quote = Line::format(format_args!("\"{quip}\""));
    // Tight cards lead with the record and the bell, then hits and the quip.
    let order: &[usize] = if tight {
        &[3, 5, 1, 6]
    } else {
        &[1, 2, 3, 4, 5, 6]
    };
    for (i, &kind) in order.iter().take(fit).enumerate() {
        let cy = y + head + row * (i as f32 + 0.5);
        match kind {
            1 => {
                let mut at = left;
                at += stat(at, cy, Icon::Check, hits.as_str(), TEAL);
                at += stat(at, cy, Icon::Target, heads.as_str(), GOLD);
                stat(at, cy, Icon::Skull, elims.as_str(), CORAL);
            }
            2 => {
                let mut at = left;
                at += stat(at, cy, Icon::Eye, accuracy.as_str(), TEAL);
                at += stat(at, cy, Icon::Flame, streak.as_str(), CORAL);
                if result.bell {
                    icon(Icon::Bell, vec2(at + 9., cy), 18., GOLD);
                }
            }
            3 => {
                icon(Icon::Trophy, vec2(left + 9., cy), 18., GOLD);
                label(&record, left + 24., cy, 16., record_color);
            }
            4 => match &goal {
                Some((next, copy)) => {
                    icon(Icon::ArrowUp, vec2(left + 9., cy), 16., MUTED);
                    grade_chip(vec2(left + 32., cy), 20., *next);
                    label(copy, left + 48., cy, 14., MUTED);
                }
                None => {
                    icon(Icon::Crown, vec2(left + 9., cy), 18., GOLD);
                    label("Top grade!", left + 24., cy, 16., GOLD);
                }
            },
            5 => {
                // Shoot the bell to go again: bell, direction, distance.
                icon(Icon::Target, vec2(left + 9., cy), 16., TEAL);
                icon(Icon::Bell, vec2(left + 30., cy), 18., GOLD);
                icon(side.arrow(), vec2(left + 50., cy), 16., INK);
                let end = left + 62. + label(meters.as_str(), left + 62., cy, 14., TEAL);
                icon(Icon::Restart, vec2(end + 14., cy), 16., TEAL);
            }
            _ => {
                let mut line = Line::format(format_args!("{}", quote.as_str()));
                while width_of(line.as_str(), 14.) > inner && line.len > 0 {
                    line.len -= 1;
                    while !line.as_str().is_char_boundary(line.len) {
                        line.len -= 1;
                    }
                }
                label(line.as_str(), left, cy, 14., GOLD);
            }
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
        draw_panel(lab, input, view, layout, width, height);
        if lab.peer.mode == 0 {
            draw_radio(lab, layout);
        }
    }
    if view == 1 {
        draw_marker(lab, width, height);
        draw_pops(lab, width, height);
        draw_weapon(lab, input, width, height);
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
        // Ordinary remaps become whole key caps from a 390px host's panel
        // (316px of content); the 276px host spells them out as above.
        let (keys, count) = split_caps("Right Ctrl / Right Shift").unwrap();
        assert_eq!(&keys[..count], ["Right Ctrl", "Right Shift"]);
        let needed = caps_width(&keys[..count]) + 28.;
        let narrow = layout(390., 682.).unwrap().panel.2 - 24.;
        assert!(needed <= narrow, "{needed} > {narrow}");
        assert!(needed > layout(276., 640.).unwrap().panel.2 - 24.);
        assert!(
            split_caps(&huge).is_none(),
            "huge profiles fall back to text"
        );
    }

    #[test]
    fn grouped_digits_match_thousands_without_allocating_per_frame() {
        for value in [0, 7, 999, 1000, 10_310, 1_234_567, u32::MAX] {
            let line = Line::format(format_args!("{}", Grouped(value)));
            assert_eq!(line.as_str(), thousands(value));
        }
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(thousands(1_000_001), "1,000,001");
    }
}
