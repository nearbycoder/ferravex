//! Read-only brass-and-glass workbench presentation.
//!
//! Idle motion (flowing beams, breathing glows, the cursor and the city's
//! windows) follows the host's presentation clock. Event motion (a mirror
//! swinging to its new angle, stars popping in) follows the model's own tick
//! counters. Drawing never changes the model: previews trace a copy.
use crate::content::CHAMBERS;
use crate::model::{
    BOARD_X, BOARD_Y, COLS, Event, Game, HINT, Kind, Phase, Piece, RESET, ROWS, Refusal,
    SEAL_BUTTON, TILE, TOOL_BUTTONS, Trace, UNDO,
};
use ferravex_arcade::shapes::{self, Cap, Fill, Stroke};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;
use ferravex_play::tween::Ease;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};

const BRASS: Color = Color::new(0.91, 0.73, 0.39, 1.);
const GLASS: Color = Color::new(0.35, 0.83, 0.85, 1.);
const DARK: Color = Color::new(0.055, 0.08, 0.11, 1.);
const WARN: Color = Color::new(1., 0.46, 0.42, 1.);
const SOFT: Color = Color::new(0.8, 0.85, 0.9, 1.);
const DIM: Color = Color::new(0.25, 0.28, 0.32, 1.);
/// Workbench card and its inner content column.
const CARD_X: f32 = 618.;
const CARD_W: f32 = 318.;
const X0: f32 = 636.;
const X1: f32 = 918.;
const MID: f32 = (X0 + X1) * 0.5;
/// The painted seal button, inside the model's slightly larger click area.
const SEAL: Rect = Rect {
    x: 630.,
    y: 510.,
    w: 294.,
    h: 42.,
};
const BOARD_W: f32 = COLS as f32 * TILE;
const BOARD_H: f32 = ROWS as f32 * TILE;
const COLUMN_LABELS: [&str; COLS] = ["A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K"];
const ROW_LABELS: [&str; ROWS] = ["1", "2", "3", "4", "5", "6", "7"];
const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
/// First lock row in the workbench and the distance between rows.
const LOCKS_Y: f32 = 230.;
const LOCK_STEP: f32 = 36.;
/// Lowest point the lock rows, hint or dial preview may reach.
const INFO_BOTTOM: f32 = 448.;
/// The glass city: one tower per chamber below the board.
const CITY_BASE: f32 = 566.;
const TOWERS: [f32; 12] = [19., 25., 21., 28., 20., 26., 23., 30., 22., 27., 25., 32.];
const TOWER_WIDTHS: [f32; 12] = [26., 30., 24., 32., 28., 30., 24., 34., 28., 30., 26., 38.];
/// Ticks a turned device takes to swing to its new setting.
const SWING: f32 = 12.;

fn light(mask: u8) -> Color {
    match mask {
        1 => Color::new(1., 0.33, 0.35, 1.),
        2 => Color::new(0.35, 0.96, 0.52, 1.),
        3 => Color::new(1., 0.88, 0.35, 1.),
        4 => Color::new(0.35, 0.64, 1., 1.),
        5 => Color::new(0.94, 0.45, 0.98, 1.),
        6 => Color::new(0.35, 0.96, 0.98, 1.),
        7 => Color::new(0.95, 0.97, 1., 1.),
        _ => MUTED,
    }
}
/// Letters keep every color legible without relying on hue alone.
fn color_name(mask: u8) -> &'static str {
    match mask {
        1 => "R",
        2 => "G",
        3 => "Y",
        4 => "B",
        5 => "M",
        6 => "C",
        7 => "W",
        _ => "-",
    }
}
/// The primary channels in `mask`, red first.
fn channels(mask: u8) -> impl Iterator<Item = u8> {
    [1_u8, 2, 4].into_iter().filter(move |c| mask & c != 0)
}
/// Shadowless text for flat dark surfaces.
fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
fn say(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(size, color)).width
}
/// Text vertically centered on `cy`; returns its width.
fn say_mid(value: &str, x: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color);
    draw_text_styled(value, x, s.centered_baseline(cy), s).width
}
fn centered(value: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text_styled(value, x, y, ink(size, color).with_align(TextAlign::Center));
}
fn cell_center(x: u8, y: u8) -> Vec2 {
    vec2(
        BOARD_X + (f32::from(x) + 0.5) * TILE,
        BOARD_Y + (f32::from(y) + 0.5) * TILE,
    )
}
/// The board cell under `p`, using the model's own click arithmetic.
fn cell_at(p: Vec2) -> Option<(u8, u8)> {
    let x = (p.x - BOARD_X) / TILE;
    let y = (p.y - BOARD_Y) / TILE;
    (x >= 0. && x < COLS as f32 && y >= 0. && y < ROWS as f32).then_some((x as u8, y as u8))
}
fn area([x, y, w, h]: [f32; 4]) -> Rect {
    Rect::new(x, y, w, h)
}
fn center(r: Rect) -> Vec2 {
    vec2(r.x + r.w * 0.5, r.y + r.h * 0.5)
}
fn tool_center(tool: usize) -> Vec2 {
    center(area(TOOL_BUTTONS[tool]))
}
/// Center of pace star `k` in the workbench.
fn star_center(k: usize) -> Vec2 {
    vec2(X0 + 15. + k as f32 * 36., 150.)
}
/// Center of earned star `k` on the sealed panel.
fn medal_center(k: usize) -> Vec2 {
    vec2(MID + (k as f32 - 1.) * 62., 214.)
}
fn board_center() -> Vec2 {
    vec2(BOARD_X + BOARD_W * 0.5, BOARD_Y + BOARD_H * 0.5)
}
fn tower_top(i: usize) -> Vec2 {
    vec2(52. + i as f32 * 48., CITY_BASE - TOWERS[i])
}
/// Board cells named in a hint ("Fold down at D2"), in reading order.
fn hint_cells(hint: &str) -> impl Iterator<Item = (u8, u8)> + '_ {
    let b = hint.as_bytes();
    (0..b.len().saturating_sub(1)).filter_map(move |i| {
        let (col, row) = (b[i], b[i + 1]);
        let before = i == 0 || !b[i - 1].is_ascii_alphanumeric();
        let after = i + 2 >= b.len() || !b[i + 2].is_ascii_alphanumeric();
        if (b'A'..=b'K').contains(&col) && (b'1'..=b'7').contains(&row) && before && after {
            Some((col - b'A', row - b'1'))
        } else {
            None
        }
    })
}
fn diamond(x: f32, y: f32, radius: f32, color: Color) {
    let (n, e, s, w) = (
        vec2(x, y - radius),
        vec2(x + radius, y),
        vec2(x, y + radius),
        vec2(x - radius, y),
    );
    draw_triangle(n, e, s, color);
    draw_triangle(n, s, w, color);
}
/// Cut-gem diamond: lit upper facets, shaded lower facets.
fn gem(x: f32, y: f32, radius: f32, color: Color) {
    let c = vec2(x, y);
    let (n, e, s, w) = (
        vec2(x, y - radius),
        vec2(x + radius, y),
        vec2(x, y + radius),
        vec2(x - radius, y),
    );
    draw_triangle(c, w, n, lighten(color, 0.3));
    draw_triangle(c, n, e, lighten(color, 0.12));
    draw_triangle(c, e, s, darken(color, 0.12));
    draw_triangle(c, s, w, darken(color, 0.28));
}
/// Contact shadow under a round device.
fn shadow(x: f32, y: f32, r: f32) {
    draw_circle(x + 1.5, y + 4.5, r + 1.5, Color::new(0., 0., 0., 0.16));
    draw_circle(x + 1.5, y + 4., r - 0.5, Color::new(0., 0., 0., 0.26));
}
/// Polished housing: lit top, shaded bottom.
fn housing(x: f32, y: f32, r: f32, top: Color, bottom: Color) {
    let rect = Rect::new(x - r, y - r, r * 2., r * 2.);
    shapes::fill_circle(vec2(x, y), r, Fill::vertical(rect, top, bottom));
}
/// The game's symbol for a turn: a mirror inside a turning arrow, the same
/// arrow that appears around a device under the pointer.
fn turn_glyph(at: Vec2, size: f32, color: Color) {
    let r = size * 0.4;
    let width = (size * 0.1).max(1.5);
    let start = -PI * 0.2;
    let sweep = PI * 1.4;
    shapes::stroke_arc(
        at,
        r,
        start,
        sweep,
        Stroke::new(width, color).cap(Cap::Round),
    );
    let end = start + sweep;
    let normal = vec2(end.cos(), end.sin());
    let ahead = vec2(-end.sin(), end.cos());
    let tip = at + normal * r;
    let k = size * 0.17;
    shapes::fill_triangle(
        tip + ahead * k * 1.4,
        tip - normal * k,
        tip + normal * k,
        color,
    );
    let d = vec2(FRAC_PI_4.cos(), -FRAC_PI_4.sin()) * r * 0.62;
    shapes::fill_capsule(at - d, at + d, width * 0.7, color);
}
/// A lock's identity: one arc per target channel. Arriving channels glow;
/// missing ones stay faint.
fn lock_arcs(at: Vec2, radius: f32, width: f32, target: u8, got: u8) {
    let tone = |c: u8| {
        if got & c != 0 {
            light(c)
        } else {
            with_alpha(light(c), 0.28)
        }
    };
    let n = target.count_ones().max(1);
    if n == 1 {
        shapes::stroke_circle(at, radius, width, tone(target));
        return;
    }
    let sweep = TAU / n as f32;
    let gap = 0.3;
    for (k, c) in channels(target).enumerate() {
        shapes::stroke_arc(
            at,
            radius,
            -FRAC_PI_2 + k as f32 * sweep + gap * 0.5,
            sweep - gap,
            Stroke::new(width, tone(c)),
        );
    }
}
/// A compact lock token for lists: channel arcs, dark face and letter.
fn lock_token(at: Vec2, radius: f32, target: u8, got: u8) {
    let matched = got == target;
    if matched {
        glow(at.x, at.y, radius * 2.2, with_alpha(light(target), 0.22));
    }
    draw_circle(
        at.x,
        at.y,
        radius - 2.,
        if matched {
            shapes::mix(DARK, light(target), 0.35)
        } else {
            DARK
        },
    );
    lock_arcs(at, radius, 3., target, got);
    let s = ink(radius * 1.05, if matched { PAPER } else { light(target) })
        .with_align(TextAlign::Center);
    draw_text_styled(color_name(target), at.x, s.centered_baseline(at.y), s);
}
/// A small round status badge (a tick or a cross) at `at`.
fn badge(icon_kind: Icon, at: Vec2, radius: f32, fill: Color) {
    draw_circle(at.x + 0.5, at.y + 1.5, radius, Color::new(0., 0., 0., 0.35));
    draw_circle(at.x, at.y, radius, fill);
    icon(icon_kind, at.x, at.y, radius * 1.35, DARK);
}
/// A curved arrow around a turnable device: "click to turn".
fn turn_arrow(at: Vec2, radius: f32, color: Color) {
    let start = -PI * 0.92;
    let sweep = PI * 0.62;
    shapes::stroke_arc(
        at,
        radius,
        start,
        sweep,
        Stroke::new(2.5, color).cap(Cap::Round),
    );
    let end = start + sweep;
    let normal = vec2(end.cos(), end.sin());
    let ahead = vec2(-end.sin(), end.cos());
    let tip = at + normal * radius;
    shapes::fill_triangle(
        tip + ahead * 6.5,
        tip - normal * 4.5,
        tip + normal * 4.5,
        color,
    );
}
/// Bright pulses spaced along a beam that march in its direction of travel.
/// `pattern` is `[period, dash, travelled]`; positions follow world
/// coordinates so pulses line up across cells.
fn dashes(from: Vec2, d: Vec2, length: f32, pattern: [f32; 3], width: f32, color: Color) {
    let [period, dash, travelled] = pattern;
    let m = (from.dot(d) - travelled).rem_euclid(period);
    let seg = |a: f32, b: f32| {
        let (p, q) = (from + d * a, from + d * b);
        draw_line(p.x, p.y, q.x, q.y, width, color);
    };
    if m < dash {
        seg(0., (dash - m).min(length));
    }
    let mut u = period - m;
    while u < length {
        seg(u, (u + dash).min(length));
        u += period;
    }
}

/// A miniature of the next chamber's layout.
fn chamber_glimpse(pieces: &[Piece], name: &str) {
    let s = ink(14., BRASS);
    let nw = text_width(name, s);
    let label_w = 18. + nw;
    icon(Icon::ArrowRight, MID - label_w * 0.5 + 7., 368., 14., BRASS);
    draw_text_styled(
        name,
        MID - label_w * 0.5 + 18.,
        s.centered_baseline(368.),
        s,
    );
    let cell = 13.;
    let (w, h) = (COLS as f32 * cell, ROWS as f32 * cell);
    let (ox, oy) = (MID - w * 0.5, 384.);
    rounded(
        ox - 4.,
        oy - 4.,
        w + 8.,
        h + 8.,
        6.,
        Color::new(0.03, 0.045, 0.065, 1.),
    );
    for row in 0..ROWS {
        for col in (row % 2..COLS).step_by(2) {
            panel(
                ox + col as f32 * cell,
                oy + row as f32 * cell,
                cell,
                cell,
                Color::new(1., 1., 1., 0.025),
            );
        }
    }
    for p in pieces {
        let at = vec2(
            ox + (f32::from(p.x) + 0.5) * cell,
            oy + (f32::from(p.y) + 0.5) * cell,
        );
        match p.kind {
            Kind::Emitter { color, .. } => {
                glow(at.x, at.y, 9., with_alpha(light(color), 0.4));
                draw_circle(at.x, at.y, 4., light(color));
            }
            Kind::Receiver(target) => {
                shapes::stroke_circle(at, 4.2, 1.6, light(target));
            }
            Kind::Mirror => draw_circle(at.x, at.y, 3.5, with_alpha(GLASS, 0.8)),
            Kind::Splitter => diamond(at.x, at.y, 4., with_alpha(BRASS, 0.9)),
            Kind::Filter => diamond(at.x, at.y, 4.5, with_alpha(light(1 << p.start), 0.85)),
            Kind::Wall => rounded(
                at.x - 5.,
                at.y - 5.,
                10.,
                10.,
                2.,
                Color::new(0.4, 0.37, 0.34, 1.),
            ),
        }
    }
}

/// What the pointer is over this frame (presentation only).
#[derive(Clone, Copy, Default)]
struct Hover {
    /// A turnable device on the board, while editing.
    device: Option<usize>,
    /// A lock, on the board or in the workbench list.
    lock: Option<usize>,
    /// Undo, reset or hint, while editing.
    tool: Option<usize>,
    seal: bool,
}

impl Game {
    fn editing(&self) -> bool {
        self.phase == Phase::Editing
    }
    fn total_stars(&self) -> u32 {
        self.stars.iter().map(|s| u32::from(*s)).sum()
    }
    fn rank(&self) -> (&'static str, Icon) {
        match self.total_stars() {
            36 => ("Master of Light", Icon::Sun),
            24.. => ("Prism Engineer", Icon::Gear),
            _ => ("Foundry Keeper", Icon::Hammer),
        }
    }
    /// The play state in words for status readers; the screen shows it as
    /// icons and light.
    fn status(&self) -> String {
        match self.phase {
            Phase::Editing => {
                let (lit, total) = self.locks();
                let mut line = format!(
                    "{lit} of {total} locks lit; {} turns, par {}; {} stars on pace. {}",
                    self.rotations(),
                    self.par(),
                    self.pace(),
                    self.content().lesson
                );
                if self.hinted {
                    line.push_str(" Hint: ");
                    line.push_str(self.content().hint);
                }
                line
            }
            Phase::Cleared => format!(
                "Sealed with {} of 3 stars; continue to the next chamber.",
                self.stars[self.level]
            ),
            Phase::Finished => format!(
                "{} of 36 stars; rank {}.",
                self.total_stars(),
                self.rank().0
            ),
        }
    }
    /// 0 when device `i` just changed, 1 once it has settled.
    fn swing(&self, i: usize) -> f32 {
        match self.turned[i] {
            0 => 1.,
            at => (self.age.saturating_sub(at) as f32 / SWING).min(1.),
        }
    }
    /// The drawn angle of mirror or splitter `i`, swinging a quarter turn
    /// into its current setting after each change.
    fn mirror_angle(&self, i: usize) -> f32 {
        let settled = if self.dials[i] == 0 {
            -FRAC_PI_4
        } else {
            FRAC_PI_4
        };
        settled - FRAC_PI_2 * (1. - Ease::BackOut.apply(self.swing(i)))
    }
    fn lock_rows(&self) -> impl Iterator<Item = (usize, u8, f32)> + '_ {
        self.content()
            .pieces
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p.kind {
                Kind::Receiver(target) => Some((i, target)),
                _ => None,
            })
            .enumerate()
            .map(|(row, (i, target))| (i, target, LOCKS_Y + row as f32 * LOCK_STEP))
    }
    fn hover(&self) -> Hover {
        let p = pointer();
        let mut hover = Hover::default();
        if !p.is_finite() {
            return hover;
        }
        let editing = self.editing();
        if let Some((x, y)) = cell_at(p)
            && let Some(i) = self.piece_at(x, y)
        {
            match self.content().pieces[i].kind {
                Kind::Receiver(_) => hover.lock = Some(i),
                kind if editing && kind.dial_count() > 0 => hover.device = Some(i),
                _ => {}
            }
        }
        if editing {
            hover.tool = TOOL_BUTTONS.iter().position(|a| area(*a).contains(p));
            for (i, _, y) in self.lock_rows() {
                if Rect::new(X0 - 6., y, X1 - X0 + 12., LOCK_STEP - 4.).contains(p) {
                    hover.lock = Some(i);
                }
            }
        }
        hover.seal = area(SEAL_BUTTON).contains(p);
        hover
    }
    /// The light paths the hovered command would produce.
    fn ghost(&self, hover: &Hover) -> Option<Trace> {
        if !self.editing() {
            return None;
        }
        if let Some(i) = hover.device {
            return self.preview_turn(i);
        }
        match hover.tool {
            Some(UNDO) => self.preview_undo(),
            Some(RESET) => self.preview_reset(),
            _ => None,
        }
    }
    /// Stars the hovered command would leave on pace (presentation only).
    fn star_preview(&self, hover: &Hover) -> Option<u8> {
        if !self.editing() {
            return None;
        }
        let (moves, hinted, resets, mistakes) =
            (self.rotations(), self.hinted, self.resets, self.mistakes);
        let (lit, total) = self.locks();
        match hover.tool {
            Some(UNDO) if moves > 0 && self.last_change().is_some() => {
                return Some(self.rating(moves - 1, hinted, resets, mistakes));
            }
            Some(RESET) if moves > 0 => {
                return Some(self.rating(0, hinted, resets.saturating_add(1), mistakes));
            }
            Some(HINT) if !hinted => return Some(self.rating(moves, true, resets, mistakes)),
            _ => {}
        }
        if hover.seal && lit != total {
            return Some(self.rating(moves, hinted, resets, mistakes.saturating_add(1)));
        }
        hover
            .device
            .map(|_| self.rating(moves.saturating_add(1), hinted, resets, mistakes))
    }

    // ----- Board ---------------------------------------------------------

    fn board(&self, hover: &Hover) {
        let t = clock();
        let editing = self.editing();
        let label = with_alpha(MUTED, 0.75);
        for (col, name) in COLUMN_LABELS.iter().enumerate() {
            centered(name, BOARD_X + (col as f32 + 0.5) * TILE, 153., 10., label);
        }
        for (row, name) in ROW_LABELS.iter().enumerate() {
            centered(
                name,
                19.,
                BOARD_Y + (row as f32 + 0.5) * TILE + 4.,
                10.,
                label,
            );
        }
        self.plate();
        if !editing {
            // A sealed circuit hums: the whole plate glows.
            outline(
                BOARD_X - 3.,
                BOARD_Y - 3.,
                BOARD_W + 6.,
                BOARD_H + 6.,
                10.,
                2.,
                with_alpha(GLASS, 0.3 + 0.3 * pulse(t, 0.5)),
            );
        }
        let hinted = editing && self.hinted;
        if hinted {
            for (x, y) in hint_cells(self.content().hint) {
                let (cx, cy) = (BOARD_X + f32::from(x) * TILE, BOARD_Y + f32::from(y) * TILE);
                rounded(
                    cx + 3.,
                    cy + 3.,
                    TILE - 6.,
                    TILE - 6.,
                    8.,
                    with_alpha(BRASS, 0.06),
                );
                outline(
                    cx + 3.,
                    cy + 3.,
                    TILE - 6.,
                    TILE - 6.,
                    8.,
                    1.5,
                    with_alpha(BRASS, 0.3 + 0.35 * pulse(t, 0.8)),
                );
            }
        }
        let ghost = self.ghost(hover);
        self.beams(ghost.as_ref(), t);
        self.halos(t);
        // Hover affordances sit under the devices they describe.
        if let Some(i) = hover.device {
            let p = self.content().pieces[i];
            let at = cell_center(p.x, p.y);
            glow(at.x, at.y, 40., with_alpha(GLASS, 0.28));
        }
        if hover.tool == Some(UNDO)
            && let Some(i) = self.last_change()
        {
            let p = self.content().pieces[i];
            let at = cell_center(p.x, p.y);
            glow(at.x, at.y, 40., with_alpha(SOFT, 0.22));
        }
        for (i, p) in self.content().pieces.iter().enumerate() {
            let lift = if hover.device == Some(i) { 1. } else { 0. };
            self.piece(i, p.kind, cell_center(p.x, p.y), lift);
        }
        if let Some(i) = hover.lock {
            let p = self.content().pieces[i];
            let at = cell_center(p.x, p.y);
            shapes::stroke_circle(at, 28., 2., with_alpha(BRASS, 0.55 + 0.35 * pulse(t, 1.2)));
        }
        if hover.tool == Some(UNDO)
            && let Some(i) = self.last_change()
        {
            let p = self.content().pieces[i];
            let at = cell_center(p.x, p.y);
            shapes::stroke_circle(at, 27., 2., with_alpha(SOFT, 0.8));
            icon(Icon::Undo, at.x - 18., at.y - 18., 14., SOFT);
        }
        // Preview outcome at each lock the hovered command would change.
        if let Some(g) = &ghost {
            for (i, p) in self.content().pieces.iter().enumerate() {
                let Kind::Receiver(target) = p.kind else {
                    continue;
                };
                let now = self.trace.received[i] == target;
                let then = g.received[i] == target;
                if now != then {
                    let at = cell_center(p.x, p.y) + vec2(17., -17.);
                    let pop = 1. + 0.12 * pulse(t, 2.);
                    if then {
                        badge(Icon::Check, at, 8. * pop, GLASS);
                    } else {
                        badge(Icon::Cross, at, 8. * pop, WARN);
                    }
                }
            }
        }
        if hinted {
            for (k, (x, y)) in hint_cells(self.content().hint).enumerate().take(9) {
                let (cx, cy) = (
                    BOARD_X + f32::from(x) * TILE + 10.,
                    BOARD_Y + f32::from(y) * TILE + 10.,
                );
                draw_circle(cx, cy + 1., 7.5, Color::new(0., 0., 0., 0.4));
                draw_circle(cx, cy, 7.5, BRASS);
                let s = ink(10., DARK).with_align(TextAlign::Center);
                draw_text_styled(DIGITS[k + 1], cx, s.centered_baseline(cy), s);
            }
        }
        // Cursor.
        let x = BOARD_X + f32::from(self.cursor.0) * TILE;
        let y = BOARD_Y + f32::from(self.cursor.1) * TILE;
        let breathe = if editing { pulse(t, 0.9) } else { 0. };
        rounded(
            x + 2.,
            y + 2.,
            TILE - 4.,
            TILE - 4.,
            8.,
            with_alpha(BRASS, 0.07),
        );
        outline(
            x + 2.,
            y + 2.,
            TILE - 4.,
            TILE - 4.,
            8.,
            2.,
            with_alpha(BRASS, 0.55 + 0.45 * breathe),
        );
        if let Some(i) = hover.device {
            self.next_setting(i, t);
        }
    }
    /// Workbench plate: batched checker cells, bevels, grooves and studs.
    fn plate(&self) {
        card(BOARD_X - 3., BOARD_Y - 3., BOARD_W + 6., BOARD_H + 6., DARK);
        let groove = Color::new(0.05, 0.072, 0.098, 1.);
        let shade = |v: f32| Color::new(v, v + 0.025, v + 0.045, 1.);
        for row in 0..ROWS {
            let y = BOARD_Y + row as f32 * TILE;
            panel(BOARD_X + 1., y + 1., BOARD_W - 2., TILE - 2., shade(0.095));
            for col in (1 - row % 2..COLS).step_by(2) {
                let x = BOARD_X + col as f32 * TILE;
                panel(x + 1., y + 1., TILE - 2., TILE - 2., shade(0.108));
            }
            panel(
                BOARD_X + 1.,
                y + 1.,
                BOARD_W - 2.,
                1.,
                Color::new(1., 1., 1., 0.035),
            );
            panel(
                BOARD_X + 1.,
                y + TILE - 2.,
                BOARD_W - 2.,
                1.,
                Color::new(0., 0., 0., 0.22),
            );
        }
        for col in 1..COLS {
            let x = BOARD_X + col as f32 * TILE;
            panel(x - 1., BOARD_Y, 2., BOARD_H, groove);
        }
        for row in 1..ROWS {
            for col in 1..COLS {
                let x = BOARD_X + col as f32 * TILE;
                let y = BOARD_Y + row as f32 * TILE;
                panel(x - 1., y - 1., 2., 2., Color::new(0.42, 0.47, 0.52, 0.45));
            }
        }
    }
    /// Luminous beams with photons flowing along every lit half-edge. With a
    /// `ghost`, paths that would change dim and their replacements appear as
    /// marching dashed light.
    fn beams(&self, ghost: Option<&Trace>, t: f32) {
        const DIRECTIONS: [Vec2; 4] = [
            Vec2::new(1., 0.),
            Vec2::new(0., 1.),
            Vec2::new(-1., 0.),
            Vec2::new(0., -1.),
        ];
        for row in 0..ROWS {
            for col in 0..COLS {
                let cell = row * COLS + col;
                let from = vec2(
                    BOARD_X + (col as f32 + 0.5) * TILE,
                    BOARD_Y + (row as f32 + 0.5) * TILE,
                );
                for (dir, d) in DIRECTIONS.iter().enumerate() {
                    let now = self.trace.edges[cell][dir];
                    let then = ghost.map_or(now, |g| g.edges[cell][dir]);
                    if now == 0 && then == 0 {
                        continue;
                    }
                    let border = (dir == 0 && col == COLS - 1)
                        || (dir == 1 && row == ROWS - 1)
                        || (dir == 2 && col == 0)
                        || (dir == 3 && row == 0);
                    let length = if border { TILE / 2. - 2. } else { TILE };
                    let to = from + *d * length;
                    if now != 0 {
                        let c = light(now);
                        let k = if then == now { 1. } else { 0.3 };
                        draw_line(from.x, from.y, to.x, to.y, 14., with_alpha(c, 0.06 * k));
                        draw_line(from.x, from.y, to.x, to.y, 7., with_alpha(c, 0.16 * k));
                        draw_line(from.x, from.y, to.x, to.y, 3., with_alpha(c, 0.9 * k));
                        draw_line(
                            from.x,
                            from.y,
                            to.x,
                            to.y,
                            1.2,
                            with_alpha(lighten(c, 0.65), k),
                        );
                        if k >= 1. {
                            dashes(
                                from,
                                *d,
                                length,
                                [26., 9., t * 70.],
                                4.5,
                                with_alpha(lighten(c, 0.6), 0.5),
                            );
                        }
                    }
                    if then != now && then != 0 {
                        let c = light(then);
                        draw_line(from.x, from.y, to.x, to.y, 9., with_alpha(c, 0.08));
                        dashes(
                            from,
                            *d,
                            length,
                            [13., 6.5, t * 40.],
                            2.4,
                            with_alpha(lighten(c, 0.3), 0.85),
                        );
                    }
                }
            }
        }
    }
    /// Additive light around sources and satisfied locks.
    fn halos(&self, t: f32) {
        for (i, p) in self.content().pieces.iter().enumerate() {
            let at = cell_center(p.x, p.y);
            match p.kind {
                Kind::Emitter { color, .. } => glow(
                    at.x,
                    at.y,
                    44.,
                    with_alpha(light(color), 0.34 + 0.12 * pulse(t, 0.7)),
                ),
                Kind::Receiver(target) if self.trace.received[i] == target => glow(
                    at.x,
                    at.y,
                    50.,
                    with_alpha(light(target), 0.42 + 0.18 * pulse(t, 0.75)),
                ),
                _ => {}
            }
        }
    }
    fn piece(&self, i: usize, kind: Kind, at: Vec2, lift: f32) {
        let (x, y) = (at.x, at.y);
        match kind {
            Kind::Wall => {
                panel(x - 20., y - 17., 44., 42., Color::new(0., 0., 0., 0.3));
                rounded_gradient(
                    x - 22.,
                    y - 22.,
                    44.,
                    44.,
                    7.,
                    Color::new(0.44, 0.41, 0.38, 1.),
                    Color::new(0.25, 0.23, 0.22, 1.),
                );
                panel(x - 16., y - 21., 32., 1., Color::new(1., 1., 1., 0.16));
                let seam = Color::new(0.14, 0.13, 0.13, 0.85);
                draw_line(x - 21., y - 1., x + 21., y - 1., 2., seam);
                draw_line(x - 5., y - 21., x - 5., y - 1., 2., seam);
                draw_line(x + 9., y - 1., x + 9., y + 21., 2., seam);
                draw_line(x - 15., y + 7., x - 9., y + 13., 1., seam);
                draw_line(x - 9., y + 13., x - 3., y + 12., 1., seam);
                panel(x + 12., y - 15., 5., 2., Color::new(1., 1., 1., 0.08));
            }
            Kind::Emitter { direction, color } => {
                let c = light(color);
                let d = [vec2(1., 0.), vec2(0., 1.), vec2(-1., 0.), vec2(0., -1.)]
                    [usize::from(direction)];
                shadow(x, y, 20.);
                shapes::fill_capsule(at, at + d * 22., 7., darken(BRASS, 0.35));
                housing(x, y, 20., lighten(BRASS, 0.25), darken(BRASS, 0.35));
                draw_circle(x, y, 15., DARK);
                draw_circle(x, y, 11., with_alpha(c, 0.22));
                let normal = vec2(-d.y, d.x);
                draw_triangle(
                    at + d * 11.,
                    at - d * 7. + normal * 8.,
                    at - d * 7. - normal * 8.,
                    c,
                );
                draw_circle(at.x + d.x, at.y + d.y, 3., lighten(c, 0.7));
            }
            Kind::Mirror | Kind::Splitter => {
                let splitter = kind == Kind::Splitter;
                shadow(x, y, 20. + lift);
                let y = y - 2. * lift;
                let rim = if splitter { BRASS } else { GLASS };
                housing(
                    x,
                    y,
                    20.,
                    lighten(rim, 0.15 + 0.25 * lift),
                    darken(rim, 0.3),
                );
                draw_circle(x, y, 17.5, Color::new(0.08, 0.115, 0.15, 1.));
                draw_circle(x, y - 2.5, 14., Color::new(1., 1., 1., 0.03));
                if splitter {
                    diamond(x, y, 12., Color::new(0.23, 0.36, 0.37, 0.8));
                    draw_line(x - 13., y, x + 13., y, 1., with_alpha(BRASS, 0.6));
                    draw_line(x, y - 13., x, y + 13., 1., with_alpha(BRASS, 0.6));
                }
                let angle = self.mirror_angle(i);
                let dir = vec2(angle.cos(), angle.sin());
                let c = vec2(x, y);
                let (a, b) = (c - dir * 18.4, c + dir * 18.4);
                let silver = if splitter {
                    with_alpha(lighten(GLASS, 0.35), 0.85)
                } else {
                    lighten(GLASS, 0.45)
                };
                shapes::fill_capsule(a, b, 3., silver);
                // Specular glint along the upper face.
                let mut up = vec2(dir.y, -dir.x);
                if up.y > 0. {
                    up = -up;
                }
                let (g0, g1) = (a + dir * 5.6 + up * 0.9, b - dir * 12.7 + up * 0.9);
                draw_line(g0.x, g0.y, g1.x, g1.y, 1., Color::new(1., 1., 1., 0.75));
                draw_circle(x, y, 3.5, BRASS);
                draw_circle(x - 0.8, y - 0.8, 1.4, lighten(BRASS, 0.6));
            }
            Kind::Filter => {
                let c = light(1 << self.dials[i]);
                let pop = 0.62 + 0.38 * Ease::BackOut.apply(self.swing(i));
                let y = y - 2. * lift;
                diamond(
                    x + 1.5,
                    y + 4. + 2. * lift,
                    23.,
                    Color::new(0., 0., 0., 0.38),
                );
                gem(x, y, 23., lighten(BRASS, 0.2 * lift));
                gem(x, y, 18. * pop, c);
                centered(color_name(1 << self.dials[i]), x, y + 5., 14. * pop, INK);
            }
            Kind::Receiver(target) => {
                let got = self.trace.received[i];
                let matched = target == got;
                let tc = light(target);
                shadow(x, y, 22.);
                if matched {
                    housing(x, y, 22., lighten(tc, 0.35), darken(tc, 0.25));
                    draw_circle(x, y, 17., shapes::mix(DARK, tc, 0.3));
                } else {
                    housing(x, y, 22., lighten(BRASS, 0.25), darken(BRASS, 0.35));
                    draw_circle(x, y, 17., DARK);
                }
                lock_arcs(at, 14.5, 3., target, got);
                centered(
                    color_name(target),
                    x,
                    y + 5.,
                    15.,
                    if matched { PAPER } else { tc },
                );
                let extra = got & !target;
                if extra != 0 {
                    // Contamination: an unwanted channel spoils the lock.
                    let wobble = pulse(clock(), 2.);
                    shapes::stroke_circle(at, 25., 2., with_alpha(WARN, 0.5 + 0.4 * wobble));
                    for (k, c) in channels(extra).enumerate() {
                        draw_circle(x - 15. + k as f32 * 7., y + 19., 3., light(c));
                    }
                    badge(Icon::Cross, at + vec2(17., -17.), 7.5, WARN);
                } else if matched {
                    badge(Icon::Check, at + vec2(17., -17.), 7.5, GLASS);
                }
            }
        }
    }
    /// Over a hovered device: a turn arrow and a ghost of its next setting.
    fn next_setting(&self, i: usize, t: f32) {
        let p = self.content().pieces[i];
        let at = cell_center(p.x, p.y) - vec2(0., 2.);
        let fade = 0.55 + 0.35 * pulse(t, 1.4);
        match p.kind {
            Kind::Mirror | Kind::Splitter => {
                let angle = if self.dials[i] == 0 {
                    FRAC_PI_4
                } else {
                    -FRAC_PI_4
                };
                let d = vec2(angle.cos(), angle.sin()) * 16.;
                shapes::fill_capsule(at - d, at + d, 1.6, with_alpha(PAPER, 0.4 * fade));
            }
            Kind::Filter => {
                let next = 1 << ((self.dials[i] + 1) % 3);
                let g = at + vec2(18., 18.);
                diamond(g.x, g.y, 9., DARK);
                gem(g.x, g.y, 7.5, with_alpha(light(next), 0.6 + 0.4 * fade));
            }
            _ => return,
        }
        turn_arrow(at, 27., with_alpha(GLASS, fade));
    }

    // ----- The glass city ------------------------------------------------

    /// One glass tower per chamber. A sealed tower glows in the colors that
    /// chamber's locks demanded, one band of windows per star; the current
    /// tower waits under a beacon; later towers stand dark.
    fn skyline(&self) {
        let t = clock();
        panel(BOARD_X, CITY_BASE, BOARD_W, 1., with_alpha(BRASS, 0.22));
        for (i, &h) in TOWERS.iter().enumerate() {
            let top = tower_top(i);
            let w = TOWER_WIDTHS[i];
            let left = top.x - w * 0.5;
            let stars = self.stars[i];
            let sealed = stars > 0;
            let current = i == self.level && !sealed;
            let hue = CHAMBERS[i]
                .pieces
                .iter()
                .find_map(|p| match p.kind {
                    Kind::Receiver(target) => Some(light(target)),
                    _ => None,
                })
                .unwrap_or(BRASS);
            let body = if sealed {
                shapes::mix(Color::new(0.1, 0.12, 0.15, 1.), hue, 0.22)
            } else if current {
                Color::new(0.1, 0.16, 0.2, 1.)
            } else {
                Color::new(0.1, 0.12, 0.155, 1.)
            };
            if sealed {
                glow(top.x, CITY_BASE - h * 0.45, w, with_alpha(hue, 0.16));
            }
            // Roof first so the facade overlaps its base.
            let roof = lighten(body, 0.08);
            match i % 3 {
                0 => panel(top.x - 0.75, top.y - 7., 1.5, 7., roof),
                1 => rounded(left + w * 0.2, top.y - 5., w * 0.6, 6., 2., roof),
                _ => shapes::fill_triangle(
                    vec2(left + 2., top.y + 1.),
                    vec2(top.x, top.y - 8.),
                    vec2(left + w - 2., top.y + 1.),
                    roof,
                ),
            }
            if i == TOWERS.len() - 1 {
                // The Daybreak Engine wears a dome.
                shapes::fill_sector(top + vec2(0., 1.), w * 0.36, PI, PI, roof);
            }
            rounded_gradient(
                left,
                top.y,
                w,
                h,
                2.5,
                lighten(body, 0.07),
                darken(body, 0.35),
            );
            panel(
                left + 1.,
                top.y + 1.,
                w - 2.,
                1.,
                Color::new(1., 1., 1., 0.07),
            );
            if current {
                outline(
                    left,
                    top.y,
                    w,
                    h,
                    2.5,
                    1.,
                    with_alpha(GLASS, 0.4 + 0.35 * pulse(t, 0.9)),
                );
            }
            // Windows: columns of glass, lit bottom-up in a scattered order.
            let cols = ((w - 6.) / 6.) as usize;
            let rows = ((h - 6.) / 5.) as usize;
            let total = rows * cols;
            let lit = (total * usize::from(stars)).div_ceil(3);
            let span = cols as f32 * 6. - 2.;
            for n in 0..total {
                let (row, col) = (n / cols, n % cols);
                let wx = top.x - span * 0.5 + col as f32 * 6.;
                let wy = CITY_BASE - 6. - row as f32 * 5.;
                let order = (n * 7 + i * 5) % total;
                let color = if order < lit {
                    let twinkle = pulse(t + i as f32 * 0.37 + n as f32 * 0.61, 0.21);
                    with_alpha(lighten(hue, 0.35), 0.62 + 0.38 * twinkle)
                } else if current {
                    with_alpha(GLASS, 0.16 + 0.12 * pulse(t, 0.9))
                } else {
                    Color::new(0.17, 0.2, 0.25, 1.)
                };
                panel(wx, wy, 4., 2.5, color);
            }
            if sealed {
                for k in 0..3_u8 {
                    let color = if k < stars { BRASS } else { DIM };
                    draw_circle(top.x - 6. + f32::from(k) * 6., top.y - 12., 1.8, color);
                }
            } else if current {
                let beacon = pulse(t, 0.9);
                glow(top.x, top.y - 12., 10., with_alpha(GLASS, 0.35 * beacon));
                draw_circle(
                    top.x,
                    top.y - 12.,
                    2.4,
                    with_alpha(GLASS, 0.6 + 0.4 * beacon),
                );
            }
        }
    }

    // ----- Workbench -----------------------------------------------------

    fn workbench(&self, hover: &Hover) {
        surface(CARD_X, 120., CARD_W, 446.);
        rounded(CARD_X + 18., 120., 56., 3., 1.5, BRASS);
        if self.editing() {
            self.editing_panel(hover);
        } else {
            self.sealed_panel();
        }
        self.seal_button(hover);
    }
    fn editing_panel(&self, hover: &Hover) {
        let t = clock();
        let pace = self.pace();
        let preview = self.star_preview(hover);
        // Pace: three stars that dim as costs mount. A hovered command
        // previews its effect: a star at risk flashes red, a regained one
        // glows.
        for k in 0..3_u8 {
            let at = star_center(usize::from(k));
            let lit = k < pace;
            let losing = lit && preview.is_some_and(|p| k >= p);
            let gaining = !lit && preview.is_some_and(|p| k < p);
            let color = if losing {
                shapes::mix(BRASS, WARN, 0.4 + 0.6 * pulse(t, 2.5))
            } else if lit {
                BRASS
            } else if gaining {
                with_alpha(GLASS, 0.5 + 0.4 * pulse(t, 2.))
            } else {
                DIM
            };
            if lit {
                glow(at.x, at.y, 22., with_alpha(color, 0.16));
            }
            icon(Icon::Star, at.x, at.y, 28., color);
        }
        // Turns taken and par, read from the right edge.
        let moves = self.rotations();
        let par = self.par();
        let par_label = par.to_string();
        let pw = text_width(&par_label, ink(17., MUTED));
        say_mid(&par_label, X1 - pw, 150., 17., MUTED);
        icon(Icon::Flag, X1 - pw - 12., 150., 16., MUTED);
        let moves_label = moves.to_string();
        let mw = text_width(&moves_label, ink(24., PAPER));
        let mx = X1 - pw - 34. - mw;
        let over = moves > par;
        say_mid(
            &moves_label,
            mx,
            150.,
            24.,
            if over { lighten(BRASS, 0.2) } else { PAPER },
        );
        turn_glyph(vec2(mx - 15., 150.), 20., SOFT);
        // Turn meter: the left half is the three-star budget (par), the
        // right half the two-star budget.
        let slots = (par * 2).max(1);
        let gap = if slots > 16 { 2. } else { 3. };
        let w = (X1 - X0 - gap * (slots - 1) as f32) / slots as f32;
        let next = if hover.device.is_some() {
            Some(moves)
        } else {
            None
        };
        for s in 0..slots {
            let x = X0 + s as f32 * (w + gap);
            let zone = if s < par { BRASS } else { SOFT };
            let filled = s < moves;
            rounded(
                x,
                174.,
                w,
                8.,
                3.,
                if filled { zone } else { with_alpha(zone, 0.14) },
            );
            if next == Some(s) {
                outline(
                    x - 1.,
                    173.,
                    w + 2.,
                    10.,
                    3.,
                    1.5,
                    with_alpha(GLASS, 0.5 + 0.5 * pulse(t, 2.)),
                );
            }
        }
        icon(
            Icon::Flag,
            X0 + par as f32 * (w + gap) - gap * 0.5,
            166.,
            11.,
            with_alpha(BRASS, 0.8),
        );
        // What has cost stars so far, only when something has.
        self.cost_chips(X0, 191.);
        divider(X0, 220., X1 - X0, Color::new(1., 1., 1., 0.1));
        // Locks: target token, then each channel as wanted (ring), arriving
        // (filled) or spoiling (filled and struck through).
        let mut bottom = LOCKS_Y;
        for (i, target, y) in self.lock_rows() {
            let p = self.content().pieces[i];
            let got = self.trace.received[i];
            let matched = got == target;
            let extra = got & !target != 0;
            rounded(
                X0 - 6.,
                y,
                X1 - X0 + 12.,
                LOCK_STEP - 4.,
                8.,
                if matched {
                    with_alpha(GLASS, 0.1)
                } else {
                    Color::new(1., 1., 1., 0.035)
                },
            );
            if hover.lock == Some(i) {
                outline(
                    X0 - 6.,
                    y,
                    X1 - X0 + 12.,
                    LOCK_STEP - 4.,
                    8.,
                    1.,
                    with_alpha(BRASS, 0.6),
                );
            }
            let cy = y + (LOCK_STEP - 4.) * 0.5;
            lock_token(vec2(X0 + 12., cy), 11., target, got);
            let cw = say_mid(COLUMN_LABELS[usize::from(p.x)], X0 + 32., cy, 12., MUTED);
            say_mid(
                ROW_LABELS[usize::from(p.y)],
                X0 + 32. + cw + 1.,
                cy,
                12.,
                MUTED,
            );
            for n in 0..3 {
                let c = 1_u8 << n;
                let at = vec2(X0 + 104. + n as f32 * 34., cy);
                let (need, have) = (target & c != 0, got & c != 0);
                match (need, have) {
                    (true, true) => {
                        glow(at.x, at.y, 16., with_alpha(light(c), 0.3));
                        draw_circle(at.x, at.y, 7., light(c));
                    }
                    (true, false) => {
                        shapes::stroke_circle(
                            at,
                            6.5,
                            2.,
                            with_alpha(light(c), 0.55 + 0.3 * pulse(t, 1.)),
                        );
                    }
                    (false, true) => {
                        draw_circle(at.x, at.y, 7., light(c));
                        draw_line(at.x - 7., at.y - 7., at.x + 7., at.y + 7., 2.5, WARN);
                        draw_line(at.x - 7., at.y + 7., at.x + 7., at.y - 7., 2.5, WARN);
                    }
                    (false, false) => draw_circle(at.x, at.y, 2., DIM),
                }
            }
            let status = vec2(X1 - 12., cy);
            if matched {
                icon(Icon::Check, status.x, status.y, 18., GLASS);
            } else if extra {
                icon(Icon::Alert, status.x, status.y, 18., WARN);
            } else {
                shapes::stroke_circle(status, 6., 1.5, DIM);
            }
            bottom = y + LOCK_STEP;
        }
        if self.hinted {
            self.hint_note(bottom + 4.);
        } else {
            self.dial_preview(hover, bottom + 4.);
        }
        self.tools(hover, t);
    }
    /// Chips for whatever has cost stars in this chamber: turns beyond twice
    /// par, the hint, resets and rejected seals. Nothing when all is clean.
    fn cost_chips(&self, mut x: f32, y: f32) {
        let cost = with_alpha(WARN, 0.9);
        let (moves, par) = (self.rotations(), self.par());
        if moves > par * 2 {
            let over = (moves - par * 2).to_string();
            x += icon_chip(Icon::Plus, &over, x, y, 13., cost) + 6.;
        }
        if self.hinted {
            x += icon_chip(Icon::Bulb, "", x, y, 13., BRASS) + 6.;
        }
        if self.resets > 0 {
            x += icon_chip(Icon::Restart, &self.resets.to_string(), x, y, 13., cost) + 6.;
        }
        if self.mistakes > 0 {
            icon_chip(Icon::Cross, &self.mistakes.to_string(), x, y, 13., cost);
        }
    }
    /// The engineer's hint, on request. The cells it names are also marked
    /// on the board.
    fn hint_note(&self, top: f32) {
        let hint = self.content().hint;
        let (x, w) = (X0 - 6., X1 - X0 + 12.);
        let text_x = X0 + 24.;
        let room = X1 - text_x;
        let mut size = 13.;
        let lines = |size: f32| wrap_text(hint, room, ink(size, SOFT)).count() as f32;
        while size > 11. && top + 12. + lines(size) * size * 1.32 > INFO_BOTTOM {
            size -= 0.5;
        }
        let h = (12. + lines(size) * size * 1.32).min(INFO_BOTTOM - top);
        rounded(x, top, w, h, 8., with_alpha(BRASS, 0.08));
        outline(x, top, w, h, 8., 1., with_alpha(BRASS, 0.3));
        icon(Icon::Bulb, X0 + 8., top + 16., 18., BRASS);
        let mut baseline = top + 6. + size;
        for line in wrap_text(hint, room, ink(size, SOFT)) {
            say(line, text_x, baseline, size, SOFT);
            baseline += size * 1.32;
        }
    }
    /// The selected device's dial: every setting it cycles through, the
    /// current one ringed.
    fn dial_preview(&self, hover: &Hover, top: f32) {
        let index = hover.device.or_else(|| {
            self.selected()
                .filter(|&i| self.content().pieces[i].kind.dial_count() > 0)
        });
        let Some(i) = index else {
            return;
        };
        let p = self.content().pieces[i];
        let count = p.kind.dial_count();
        let cy = ((top + INFO_BOTTOM) * 0.5).min(top + 46.);
        let spacing = 66.;
        let first = MID + 14. - spacing * f32::from(count - 1) * 0.5;
        let cw = say_mid(COLUMN_LABELS[usize::from(p.x)], X0, cy, 14., BRASS);
        say_mid(ROW_LABELS[usize::from(p.y)], X0 + cw + 1., cy, 14., BRASS);
        for s in 0..count {
            let at = vec2(first + f32::from(s) * spacing, cy);
            let current = s == self.dials[i];
            let alpha = if current { 1. } else { 0.4 };
            if current {
                glow(at.x, at.y, 30., with_alpha(GLASS, 0.18));
                shapes::stroke_circle(at, 21., 2., with_alpha(GLASS, 0.85));
            }
            match p.kind {
                Kind::Filter => {
                    let c = light(1 << s);
                    gem(at.x, at.y, 14., with_alpha(c, alpha));
                    centered(
                        color_name(1 << s),
                        at.x,
                        at.y + 4.,
                        11.,
                        with_alpha(INK, alpha),
                    );
                }
                _ => {
                    let angle = if s == 0 { -FRAC_PI_4 } else { FRAC_PI_4 };
                    let rim = if p.kind == Kind::Splitter {
                        BRASS
                    } else {
                        GLASS
                    };
                    draw_circle(at.x, at.y, 15., with_alpha(darken(rim, 0.45), alpha));
                    draw_circle(at.x, at.y, 12.5, with_alpha(DARK, alpha));
                    let d = vec2(angle.cos(), angle.sin()) * 11.;
                    shapes::fill_capsule(
                        at - d,
                        at + d,
                        2.2,
                        with_alpha(lighten(GLASS, 0.45), alpha),
                    );
                }
            }
            if s + 1 < count {
                icon(
                    Icon::ArrowRight,
                    at.x + spacing * 0.5,
                    cy,
                    14.,
                    with_alpha(MUTED, 0.8),
                );
            }
        }
    }
    /// Undo, reset and hint as icon buttons with their key caps. They dim
    /// when there is nothing to do; the hint bulb stays lit once used.
    fn tools(&self, hover: &Hover, t: f32) {
        const TOOLS: [(Icon, &str); 3] =
            [(Icon::Undo, "X"), (Icon::Restart, "C"), (Icon::Bulb, "4")];
        for (n, (glyph, key)) in TOOLS.iter().enumerate() {
            let r = area(TOOL_BUTTONS[n]);
            let enabled = match n {
                UNDO => self.last_change().is_some(),
                RESET => self.rotations() > 0,
                _ => !self.hinted,
            };
            let hot = enabled && hover.tool == Some(n);
            let y = r.y - if hot { 2. } else { 0. };
            if hot {
                shapes::drop_shadow(
                    Rect::new(r.x, y, r.w, r.h),
                    9.,
                    shapes::Shadow::soft(with_alpha(GLASS, 0.25)).blur(12.),
                );
            }
            rounded_gradient(
                r.x,
                y,
                r.w,
                r.h,
                9.,
                Color::new(0.15, 0.18, 0.22, 1.),
                Color::new(0.1, 0.12, 0.15, 1.),
            );
            outline(
                r.x,
                y,
                r.w,
                r.h,
                9.,
                1.,
                if hot {
                    with_alpha(GLASS, 0.8)
                } else {
                    Color::new(1., 1., 1., 0.08)
                },
            );
            let cy = y + r.h * 0.5;
            let used = n == HINT && self.hinted;
            let color = if used {
                BRASS
            } else if hot {
                lighten(GLASS, 0.3)
            } else if enabled {
                PAPER
            } else {
                DIM
            };
            if used {
                glow(
                    r.x + 30.,
                    cy,
                    18.,
                    with_alpha(BRASS, 0.2 + 0.1 * pulse(t, 0.6)),
                );
            }
            icon(*glyph, r.x + 30., cy, 20., color);
            keycap(key, "", r.x + 52., cy - 10., MUTED);
        }
    }
    fn seal_button(&self, hover: &Hover) {
        let t = clock();
        let (lit, total) = self.locks();
        let ready = lit == total || !self.editing();
        let hot = hover.seal && self.phase != Phase::Finished;
        let refused = hot && !ready;
        let lift = if hot && ready { 2. } else { 0. };
        let (bx, by, bw, bh) = (SEAL.x, SEAL.y - lift, SEAL.w, SEAL.h);
        if ready {
            let breathe = if self.editing() { pulse(t, 0.8) } else { 0.5 };
            shapes::drop_shadow(
                Rect::new(bx, by, bw, bh),
                11.,
                shapes::Shadow::soft(with_alpha(GLASS, 0.22 + 0.16 * breathe + 0.15 * lift))
                    .blur(18.),
            );
            rounded_gradient(
                bx,
                by,
                bw,
                bh,
                11.,
                lighten(GLASS, 0.2 + 0.05 * lift),
                darken(GLASS, 0.12),
            );
            outline(bx, by, bw, bh, 11., 1., Color::new(1., 1., 1., 0.3));
        } else {
            rounded_gradient(
                bx,
                by,
                bw,
                bh,
                11.,
                Color::new(0.16, 0.15, 0.13, 1.),
                Color::new(0.11, 0.11, 0.1, 1.),
            );
            outline(
                bx,
                by,
                bw,
                bh,
                11.,
                1.,
                if refused {
                    with_alpha(WARN, 0.7)
                } else {
                    with_alpha(BRASS, 0.35)
                },
            );
        }
        let ink_color = if ready {
            INK
        } else if refused {
            WARN
        } else {
            BRASS
        };
        let (glyph, label, key) = match self.phase {
            Phase::Editing if ready => (Icon::Unlock, "Seal", "5"),
            Phase::Editing => (Icon::Lock, "Seal", "5"),
            Phase::Cleared => (Icon::ArrowRight, "Next", "Space"),
            Phase::Finished => (Icon::Sun, "Daybreak", ""),
        };
        let cy = by + bh * 0.5;
        icon(glyph, bx + 24., cy, 22., ink_color);
        say_mid(label, bx + 44., cy, 18., ink_color);
        let mut right = bx + bw - 14.;
        if !key.is_empty() {
            let kw = (text_width(key, ink(13., PAPER)) + 12.).max(20.);
            right -= kw;
            keycap(key, "", right, cy - 10., PAPER);
            right -= 12.;
        }
        if self.editing() {
            // One light per lock: how close the circuit is to sealing.
            for (row, (i, target, _)) in self.lock_rows().enumerate() {
                let at = vec2(right - 6. - (total - 1 - row) as f32 * 16., cy);
                if self.trace.received[i] == target {
                    draw_circle(at.x, at.y, 5.5, light(target));
                    shapes::stroke_circle(at, 5.5, 1., with_alpha(INK, 0.5));
                } else {
                    shapes::stroke_circle(at, 5., 1.5, with_alpha(ink_color, 0.6));
                }
            }
        }
    }
    /// After a seal: the stars earned, how they were earned, the campaign
    /// tally and a glimpse of the next chamber.
    fn sealed_panel(&self) {
        let earned = self.stars[self.level];
        let age = self.phase_age as f32;
        icon_badge(Icon::Check, X0 + 13., 148., 26., GLASS, DARK);
        say_mid("Sealed", X0 + 34., 148., 22., PAPER);
        for k in 0..3_u8 {
            let at = medal_center(usize::from(k));
            let start = 8. + f32::from(k) * 10.;
            let pop = Ease::BackOut.apply(((age - start) / 14.).clamp(0., 1.));
            let lit = k < earned;
            if lit && pop > 0. {
                glow(at.x, at.y, 40. * pop, with_alpha(BRASS, 0.25));
            }
            let size = 46. * if lit { pop } else { 1. };
            if size > 0.5 {
                icon(Icon::Star, at.x, at.y, size, if lit { BRASS } else { DIM });
            }
        }
        // How the stars were earned: turns against par, then any costs.
        let cy = 272.;
        let moves = self.rotations();
        let par = self.par();
        turn_glyph(vec2(X0 + 10., cy), 20., SOFT);
        let mut x = X0 + 24.;
        let moves_color = if moves <= par {
            PAPER
        } else {
            lighten(BRASS, 0.2)
        };
        x += say_mid(&moves.to_string(), x, cy, 20., moves_color) + 18.;
        x += icon_value(Icon::Flag, &par.to_string(), x, cy, 16., MUTED) + 18.;
        self.cost_chips(x, cy - 9.5);
        divider(X0, 298., X1 - X0, Color::new(1., 1., 1., 0.1));
        // Campaign tally toward the 24- and 36-star ranks.
        let all = self.total_stars();
        let w = icon_value(Icon::Star, &all.to_string(), X0, 324., 20., BRASS);
        let (mx, mw) = (X0 + w + 14., X1 - X0 - w - 14.);
        meter(mx, 320., mw, 8., all as f32 / 36., BRASS);
        panel(mx + mw * 24. / 36., 316., 1.5, 16., with_alpha(PAPER, 0.35));
        divider(X0, 350., X1 - X0, Color::new(1., 1., 1., 0.1));
        if let Some(next) = CHAMBERS.get(self.level + 1) {
            chamber_glimpse(next.pieces, next.name);
        } else {
            let (rank, glyph) = self.rank();
            icon_badge(glyph, MID, 410., 56., with_alpha(BRASS, 0.18), BRASS);
            let s = ink(18., PAPER).with_align(TextAlign::Center);
            draw_text_styled(rank, MID, 470., s);
        }
    }
    // ----- Header and results -------------------------------------------

    /// The goal as lock tokens; mixed colors show their recipe once.
    fn objective(&self, area: Rect) {
        let cy = area.y + area.h * 0.5;
        let mut x = area.x + 2.;
        icon(Icon::Target, x + 9., cy, 18., with_alpha(BRASS, 0.9));
        x += 30.;
        let mut explained = 0_u8;
        for (i, target, _) in self.lock_rows() {
            let got = self.trace.received[i];
            let at = vec2(x + 12., cy);
            lock_token(at, 12., target, got);
            if got == target {
                badge(Icon::Check, at + vec2(10., -9.), 6., GLASS);
            }
            x += 30.;
            if target.count_ones() > 1 && explained & (1 << target) == 0 {
                explained |= 1 << target;
                x += say_mid("=", x, cy, 15., MUTED) + 7.;
                for (n, c) in channels(target).enumerate() {
                    if n > 0 {
                        x += say_mid("+", x, cy, 14., MUTED) + 5.;
                    }
                    if got & c != 0 {
                        draw_circle(x + 6., cy, 6., light(c));
                    } else {
                        shapes::stroke_circle(vec2(x + 6., cy), 5.5, 2., light(c));
                    }
                    x += 17.;
                }
            }
            x += 16.;
        }
    }
    fn result(&self, area: Rect) {
        let (rank, glyph) = self.rank();
        let all = self.total_stars();
        let perfect = self.stars.iter().filter(|s| **s == 3).count();
        let (x, y) = (area.x, area.y);
        icon_badge(glyph, x + 24., y + 24., 48., with_alpha(BRASS, 0.2), BRASS);
        text(rank, x + 60., y + 34., 28., PAPER);
        // Stars and perfect chambers.
        let w = icon_value(Icon::Star, &all.to_string(), x, y + 78., 24., BRASS);
        meter(x + w + 12., y + 74., 150., 8., all as f32 / 36., BRASS);
        icon_value(
            Icon::Gem,
            &perfect.to_string(),
            x + w + 190.,
            y + 78.,
            24.,
            GLASS,
        );
        // Every chamber's medal.
        let slot = area.w / 12.;
        for (i, stars) in self.stars.iter().enumerate() {
            let cx = x + slot * (i as f32 + 0.5);
            let top = y + 106.;
            rounded(
                cx - slot * 0.42,
                top,
                slot * 0.84,
                34.,
                7.,
                Color::new(1., 1., 1., 0.05),
            );
            for k in 0..3_u8 {
                let color = if k < *stars { BRASS } else { DIM };
                icon(
                    Icon::Star,
                    cx - 13. + f32::from(k) * 13.,
                    top + 11.,
                    12.,
                    color,
                );
            }
            let s = ink(11., MUTED).with_align(TextAlign::Center);
            draw_text_styled(&(i + 1).to_string(), cx, top + 29., s);
        }
    }
}

impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.tick(input)
    }
    fn draw(&self) {
        let hover = self.hover();
        self.board(&hover);
        self.skyline();
        self.workbench(&hover);
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: self.content().name.into(),
            objective: match self.phase {
                Phase::Editing => "Light every lock, then seal",
                Phase::Cleared => "Open the next chamber",
                Phase::Finished => "Foundry restored",
            }
            .into(),
            status: self.status(),
            score: self.score(),
        }
    }
    fn outcome(&self) -> Outcome {
        self.outcome()
    }
    fn summary(&self) -> String {
        format!(
            "The Daybreak Engine is restored with {} of 36 stars and {} points, earning the rank {}.",
            self.total_stars(),
            self.score(),
            self.rank().0
        )
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.chamber() as u32, CHAMBERS.len() as u32))
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        let pieces = self.content().pieces;
        let at = |i: u8| {
            let p = pieces[usize::from(i)];
            cell_center(p.x, p.y)
        };
        for event in self.events.as_slice() {
            match *event {
                Event::Turned(i) => {
                    let color = match pieces[usize::from(i)].kind {
                        Kind::Filter => light(1 << self.dials[usize::from(i)]),
                        _ => GLASS,
                    };
                    fx.ring(at(i), with_alpha(color, 0.8), 32.);
                }
                Event::Undone(i) => {
                    fx.ring(at(i), with_alpha(SOFT, 0.7), 30.);
                    fx.popup_icon(at(i) - vec2(0., 30.), Icon::Undo, "", SOFT);
                }
                Event::Reset => {
                    for (n, p) in pieces.iter().enumerate() {
                        if p.kind.dial_count() > 0 {
                            fx.ring(at(n as u8), with_alpha(BRASS, 0.6), 28.);
                        }
                    }
                    fx.popup_icon(tool_center(RESET) - vec2(0., 30.), Icon::Restart, "", BRASS);
                }
                Event::Hint => {
                    let to = tool_center(HINT);
                    fx.sparkle(to, BRASS);
                    fx.popup_icon(to - vec2(0., 30.), Icon::Bulb, "", BRASS);
                    let before = self.rating(self.rotations(), false, self.resets, self.mistakes);
                    if before > self.pace() {
                        fx.popup_icon(star_center(2) - vec2(0., 22.), Icon::Star, "-1", WARN);
                    }
                }
                Event::Lit(i) => {
                    let Kind::Receiver(target) = pieces[usize::from(i)].kind else {
                        continue;
                    };
                    let c = light(target);
                    fx.burst(at(i), c, 26);
                    fx.ring(at(i), c, 48.);
                    fx.sparkle(at(i), lighten(c, 0.4));
                }
                Event::Dimmed(i) => fx.ring(at(i), with_alpha(WARN, 0.6), 30.),
                Event::Ready => {
                    let lock = vec2(SEAL.x + 24., SEAL.y + SEAL.h * 0.5);
                    fx.ring(lock, GLASS, 40.);
                    fx.sparkle(lock, lighten(GLASS, 0.5));
                    fx.sparkle(center(SEAL) + vec2(40., 0.), lighten(GLASS, 0.5));
                }
                Event::Rejected => {
                    for (n, p) in pieces.iter().enumerate() {
                        if let Kind::Receiver(target) = p.kind
                            && self.trace.received[n] != target
                        {
                            fx.ring(at(n as u8), WARN, 42.);
                            fx.popup_icon(at(n as u8) - vec2(0., 32.), Icon::Cross, "", WARN);
                        }
                    }
                    fx.ring(center(SEAL), with_alpha(WARN, 0.8), 80.);
                }
                Event::Sealed(stars) => {
                    for (n, p) in pieces.iter().enumerate() {
                        if let Kind::Receiver(target) = p.kind {
                            fx.burst(at(n as u8), light(target), 34);
                            fx.sparkle(at(n as u8), lighten(light(target), 0.4));
                        }
                    }
                    let finale = self.phase == Phase::Finished;
                    let text = if finale { "Daybreak!" } else { "Sealed!" };
                    fx.callout(board_center() - vec2(0., 12.), text, lighten(GLASS, 0.35));
                    let tower = tower_top(self.level);
                    fx.burst(tower, BRASS, 30);
                    fx.ring(tower + vec2(0., 10.), BRASS, 34.);
                    for k in 0..usize::from(stars) {
                        fx.sparkle(medal_center(k), lighten(BRASS, 0.3));
                    }
                    if finale {
                        fx.confetti(board_center(), 140);
                    }
                }
                Event::Opened => {
                    for (n, p) in pieces.iter().enumerate() {
                        if let Kind::Emitter { color, .. } = p.kind {
                            fx.ring(at(n as u8), light(color), 44.);
                            fx.burst(at(n as u8), light(color), 14);
                        }
                    }
                    fx.ring(tower_top(self.level) + vec2(0., 8.), GLASS, 26.);
                }
                Event::Refused(refusal) => {
                    let (to, fixed) = match refusal {
                        Refusal::Cell(x, y) => (cell_center(x, y), self.piece_at(x, y).is_some()),
                        Refusal::Undo => (tool_center(UNDO), true),
                        Refusal::Reset => (tool_center(RESET), true),
                        Refusal::Hint => (tool_center(HINT), true),
                    };
                    if fixed {
                        fx.ring(to, with_alpha(WARN, 0.75), 26.);
                        if matches!(refusal, Refusal::Cell(..)) {
                            fx.popup_icon(to - vec2(0., 28.), Icon::Lock, "", WARN);
                        }
                        fx.shake(0.22);
                    } else {
                        fx.ring(to, with_alpha(MUTED, 0.35), 18.);
                    }
                }
            }
        }
    }
    fn draw_objective(&self, area: Rect) -> bool {
        self.objective(area);
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        self.result(area);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::META;

    fn click_at(p: Vec2) -> Input {
        Input {
            click: true,
            pointer: p,
            ..Input::default()
        }
    }
    fn key(slot: usize) -> Input {
        Input {
            slot: Some(slot),
            ..Input::default()
        }
    }
    fn turn() -> Input {
        Input {
            primary: true,
            ..Input::default()
        }
    }

    #[test]
    fn progress_runs_from_the_first_chamber_to_the_twelfth() {
        let mut game = Game::new();
        assert_eq!(game.progress(), Some((1, 12)));
        for tick in 0..20_000 {
            let input = game.pilot(tick);
            game.tick(&input);
            if game.outcome() == Outcome::Won {
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Won);
        assert_eq!(game.progress(), Some((12, 12)));
        let hud = game.hud();
        assert_eq!(hud.stage, "The Daybreak Engine");
        assert!(game.summary().contains("36 of 36 stars"));
    }

    #[test]
    fn a_lit_lock_bursts_where_it_stands_and_a_seal_is_called_out() {
        let mut game = Game::new();
        // The same two taps the browser touch test uses: E6, then E2.
        let cue = game.tick(&click_at(vec2(262., 446.)));
        assert_eq!(game.events.as_slice(), &[Event::Turned(1)]);
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        assert_eq!((fx.counts().rings, fx.counts().sparks), (1, 0));
        let cue = game.tick(&click_at(vec2(262., 238.)));
        assert_eq!(
            game.events.as_slice(),
            &[Event::Turned(2), Event::Lit(3), Event::Ready]
        );
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        let counts = fx.counts();
        assert!(counts.sparks >= 26, "lock burst: {counts:?}");
        assert!(counts.rings >= 3, "turn, lock and seal rings: {counts:?}");
        assert_eq!(game.pace(), 3);
        let cue = game.tick(&key(5));
        assert_eq!(cue, Cue::Good);
        assert_eq!(game.events.as_slice(), &[Event::Sealed(3)]);
        assert_eq!(game.stars[0], 3, "pace predicted the medal");
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        assert_eq!(fx.counts().popups, 1, "one callout");
        // Events describe one tick only.
        game.tick(&Input::default());
        assert!(game.events.as_slice().is_empty());
    }

    #[test]
    fn refusals_mark_the_refusing_thing_and_change_no_rules() {
        let mut game = Game::new();
        let start = game.clone();
        let undo = Input {
            secondary: true,
            ..Input::default()
        };
        assert_eq!(game.tick(&undo), Cue::None);
        assert_eq!(game.events.as_slice(), &[Event::Refused(Refusal::Undo)]);
        let mut fx = Juice::new();
        game.effects(Cue::None, &mut fx);
        assert_eq!(fx.counts().rings, 1);
        // Stone cannot turn: the click only moves the cursor.
        assert_eq!(game.tick(&click_at(cell_center(7, 5))), Cue::None);
        assert_eq!(
            game.events.as_slice(),
            &[Event::Refused(Refusal::Cell(7, 5))]
        );
        let mut fx = Juice::new();
        game.effects(Cue::None, &mut fx);
        assert_eq!((fx.counts().rings, fx.counts().popups), (1, 1));
        assert_eq!(game.dials, start.dials);
        assert_eq!(game.rotations(), 0);
        // A rejected seal rings every unlit lock.
        assert_eq!(game.tick(&key(5)), Cue::Bad);
        assert_eq!(game.events.as_slice(), &[Event::Rejected]);
        let mut fx = Juice::new();
        game.effects(Cue::Bad, &mut fx);
        assert_eq!(fx.counts().rings, 2);
    }

    #[test]
    fn workbench_buttons_take_the_same_path_as_their_keys() {
        let tool = |i: usize| click_at(tool_center(i));
        let pairs = [
            (tool(UNDO), undo_key()),
            (
                tool(RESET),
                Input {
                    tertiary: true,
                    ..Input::default()
                },
            ),
            (tool(HINT), key(4)),
        ];
        for (click, key_input) in pairs {
            let mut by_click = Game::new();
            let mut by_key = Game::new();
            for game in [&mut by_click, &mut by_key] {
                game.tick(&turn());
                game.tick(&turn());
            }
            let cue = by_click.tick(&click);
            assert_eq!(cue, Cue::Action);
            assert_eq!(cue, by_key.tick(&key_input));
            assert_eq!(by_click, by_key);
        }
        // Repeats are refused exactly as the keys are.
        let mut game = Game::new();
        assert_eq!(game.tick(&tool(HINT)), Cue::Action);
        assert_eq!(game.tick(&tool(HINT)), Cue::None);
        assert_eq!(game.tick(&tool(UNDO)), Cue::None);
        assert_eq!(game.tick(&tool(RESET)), Cue::None);
        assert_eq!(game.resets, 0);
        // The painted seal button seals like key 5, with the hint's cap.
        game.tick(&click_at(vec2(262., 446.)));
        game.tick(&click_at(vec2(262., 238.)));
        assert_eq!(game.tick(&click_at(center(SEAL))), Cue::Good);
        assert_eq!(game.stars[0], 2);
        // Buttons are inert while a sealed chamber waits.
        let sealed = game.clone();
        for i in [UNDO, RESET, HINT] {
            assert_eq!(game.tick(&tool(i)), Cue::None);
        }
        assert_eq!(game.dials, sealed.dials);
        assert_eq!(game.chamber(), 1);
    }
    fn undo_key() -> Input {
        Input {
            secondary: true,
            ..Input::default()
        }
    }

    #[test]
    fn previews_match_the_real_outcome_and_change_nothing() {
        let mut game = Game::new();
        let start = game.clone();
        let device = game.selected().unwrap();
        let preview = game.preview_turn(device).unwrap();
        assert_eq!(game, start);
        game.tick(&turn());
        assert_eq!(game.trace, preview);
        assert_eq!(game.last_change(), Some(device));
        assert_eq!(game.preview_undo().unwrap(), start.trace);
        assert_eq!(game.preview_reset().unwrap(), start.trace);
        assert!(start.preview_undo().is_none());
        assert!(start.preview_reset().is_none());
        assert!(start.preview_turn(0).is_none(), "sources cannot turn");
    }

    #[test]
    fn hint_cells_follow_the_hint_text() {
        let cells: Vec<_> = hint_cells(CHAMBERS[1].hint).collect();
        assert_eq!(cells, [(3, 1), (3, 5), (1, 5), (1, 3)]);
        assert_eq!(hint_cells(CHAMBERS[0].hint).count(), 0);
        for chamber in &CHAMBERS {
            for (x, y) in hint_cells(chamber.hint) {
                assert!(usize::from(x) < COLS && usize::from(y) < ROWS);
            }
        }
        assert_eq!(hint_cells("").count(), 0);
        assert_eq!(hint_cells("Z9 AA1 B2x, K7.").collect::<Vec<_>>(), [(10, 6)]);
    }

    #[test]
    fn words_stay_within_the_visual_budget() {
        assert!(META.help.len() <= 4);
        for bullet in META.help {
            assert!(bullet.split_whitespace().count() < 15, "{bullet}");
            // One guide line each (the host wraps at 600 units, 18-unit text),
            // so all four bullets share a single help page.
            let width = text_width(bullet, style(18., PAPER));
            assert!(width <= 600., "{bullet}: {width}");
        }
        let game = Game::new();
        assert_eq!(game.hud().stage, "First Reflection");
        assert!(game.hud().objective.split_whitespace().count() <= 5);
        assert!(!game.summary().is_empty() && game.summary().matches(". ").count() == 0);
    }
}
