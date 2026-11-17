//! Starweaver's drawing vocabulary: palette, icon assignments, small widgets
//! (tags, pills, key caps, pips, hold slots, the panel button) and the chart
//! art (planets, the ship, the gate and the dying star).
use crate::content::*;
use crate::layout::*;
use crate::model::Game;
use ferravex_arcade::shapes::{self, Cap, Fill, Stroke};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

// ---------------------------------------------------------------------------
// Palette and vocabulary

pub(crate) const GOLD: Color = Color::new(0.96, 0.78, 0.38, 1.);
pub(crate) const CYAN: Color = Color::new(0.41, 0.85, 0.9, 1.);
pub(crate) const DIM: Color = Color::new(0.11, 0.19, 0.26, 1.);
pub(crate) const BACK: Color = Color::new(0.027, 0.055, 0.10, 1.);
pub(crate) const GREEN: Color = Color::new(0.46, 0.83, 0.57, 1.);
pub(crate) const RED: Color = Color::new(0.95, 0.42, 0.43, 1.);
pub(crate) const SOFT: Color = Color::new(0.74, 0.8, 0.86, 1.);
pub(crate) const WELL: Color = Color::new(0.075, 0.12, 0.18, 0.95);
/// Hull, fuel, credits and watches.
pub(crate) const HULL: Color = Color::new(0.98, 0.5, 0.56, 1.);
pub(crate) const FUEL: Color = Color::new(0.66, 0.9, 0.4, 1.);
pub(crate) const COIN: Color = GOLD;
pub(crate) const TIME: Color = Color::new(0.84, 0.82, 0.98, 1.);
/// Alloy, crystal and circuit.
pub(crate) const RESOURCE: [Color; 3] = [
    Color::new(0.93, 0.62, 0.38, 1.),
    Color::new(0.58, 0.8, 1., 1.),
    Color::new(0.74, 0.62, 0.98, 1.),
];
pub(crate) const RESOURCE_ICON: [Icon; 3] = [Icon::Gear, Icon::Gem, Icon::Bolt];
/// Navigator's key, Singing lens and Sentinel core.
pub(crate) const RELIC_ICON: [Icon; 3] = [Icon::Key, Icon::Music, Icon::Battery];
/// Radiant shield, Cargo cradle and Deep scanner.
pub(crate) const UPGRADE_ICON: [Icon; 3] = [Icon::Shield, Icon::Crate, Icon::Pickaxe];
/// Each sector's landmark, its color and its short name.
pub(crate) const LANDMARK_ICON: [Icon; SECTORS] = [
    Icon::Flag,
    Icon::Crate,
    Icon::Fuel,
    Icon::Key,
    Icon::Music,
    Icon::Book,
    Icon::Sword,
    Icon::Battery,
    Icon::People,
];
pub(crate) const LANDMARK_COLOR: [Color; SECTORS] = [
    CYAN,
    RESOURCE[0],
    FUEL,
    GOLD,
    GOLD,
    GREEN,
    Color::new(0.98, 0.56, 0.42, 1.),
    GOLD,
    Color::new(1., 0.86, 0.52, 1.),
];
pub(crate) const LANDMARK_NAME: [&str; SECTORS] = [
    "The gate",
    "Freight cache",
    "Fuel collector",
    "Navigator's key",
    "Singing lens",
    "Seed archive",
    "Privateers",
    "Sentinel core",
    "Engineers",
];
pub(crate) const TAB_ICON: [Icon; 5] = [
    Icon::Map,
    Icon::Search,
    Icon::Hammer,
    Icon::Rocket,
    Icon::Flag,
];
pub(crate) const TAB_NAME: [&str; 5] = ["Chart", "Survey", "Forge", "Ship", "Mission"];
pub(crate) const SERVICE_ICON: [Icon; 6] = [
    Icon::Drop,
    Icon::Wrench,
    Icon::Fuel,
    Icon::Heart,
    Icon::Coin,
    Icon::Bell,
];
pub(crate) const SERVICE_NAME: [&str; 6] = ["Condense", "Patch", "Refuel", "Repair", "Sell", "Tow"];
/// The chart panel on the left.
pub(crate) const MAP: Rect = Rect {
    x: 24.,
    y: 150.,
    w: 588.,
    h: 418.,
};
/// The header's stat strip: watch ring, hull, fuel, credits, the three cargo
/// kinds and the hold, at fixed offsets so effects can rise from them.
pub(crate) const STRIP: f32 = 414.;
pub(crate) const STAT_Y: f32 = 81.;
#[derive(Clone, Copy)]
pub(crate) enum Stat {
    Watch,
    Hull,
    Fuel,
    Credits,
    Cargo(usize),
    Hold,
}
pub(crate) fn stat_x(stat: Stat) -> f32 {
    STRIP
        + match stat {
            Stat::Watch => 0.,
            Stat::Hull => 58.,
            Stat::Fuel => 118.,
            Stat::Credits => 168.,
            Stat::Cargo(k) => 236. + k as f32 * 46.,
            Stat::Hold => 380.,
        }
}
/// Where effects for `stat` rise from (its icon).
pub(crate) fn stat_anchor(stat: Stat) -> Vec2 {
    let icon = if matches!(stat, Stat::Watch) { 11. } else { 8. };
    vec2(stat_x(stat) + icon, STAT_Y)
}

// ---------------------------------------------------------------------------
// Small drawing vocabulary

/// Plain modern text without a shadow (the panels are already dark).
pub(crate) fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
pub(crate) fn label(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(size, color)).width
}
pub(crate) fn label_center(value: &str, cx: f32, y: f32, size: f32, color: Color) {
    draw_text_styled(value, cx, y, ink(size, color).with_align(TextAlign::Center));
}
/// A label vertically centered on `cy`, shrunk to `width` when necessary.
pub(crate) fn line(value: &str, x: f32, cy: f32, width: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color);
    let w = text_width(value, s);
    let s = if w > width && w > 0. {
        s.with_size(size * width / w)
    } else {
        s
    };
    draw_text_styled(value, x, s.centered_baseline(cy), s).width
}
pub(crate) fn eyebrow(value: &str, x: f32, y: f32, color: Color) {
    let s = ink(11., color).with_letter_spacing(1.5);
    draw_text_styled(&value.to_uppercase(), x, y, s);
}
pub(crate) fn tint(c: [f32; 3], amount: f32) -> Color {
    Color::new(c[0] * amount, c[1] * amount, c[2] * amount, 1.)
}
pub(crate) fn faded(c: Color, on: bool) -> Color {
    if on { c } else { with_alpha(c, c.a * 0.36) }
}

/// One icon-and-number fact: a cost, a reward or a stock.
#[derive(Clone, Copy)]
pub(crate) struct Bit<'a> {
    pub(crate) icon: Icon,
    pub(crate) text: &'a str,
    pub(crate) color: Color,
}
pub(crate) fn bit(icon: Icon, text: &str, color: Color) -> Bit<'_> {
    Bit { icon, text, color }
}
/// Text size used beside an icon of `size`.
pub(crate) fn tag_text(size: f32) -> f32 {
    (size * 0.86).round()
}
pub(crate) fn tag_width(text: &str, size: f32) -> f32 {
    if text.is_empty() {
        size
    } else {
        size + 3. + text_width(text, ink(tag_text(size), PAPER))
    }
}
/// A bare icon and value centered on `cy`; returns its width.
pub(crate) fn tag(icon: Icon, text: &str, x: f32, cy: f32, size: f32, color: Color) -> f32 {
    icons::draw_styled(
        icon,
        vec2(x + size * 0.5, cy),
        size,
        IconStyle::new(color).shadow(Color::new(0., 0., 0., 0.35 * color.a)),
    );
    if text.is_empty() {
        return size;
    }
    let s = ink(tag_text(size), color);
    size + 3. + draw_text_styled(text, x + size + 3., s.centered_baseline(cy), s).width
}
/// Bare tags from `x` rightward; returns the end.
pub(crate) fn tags(bits: &[Bit], x: f32, cy: f32, size: f32, gap: f32) -> f32 {
    let mut x = x;
    for b in bits {
        x += tag(b.icon, b.text, x, cy, size, b.color) + gap;
    }
    x - gap
}
/// Bare tags ending at `right`; returns their left edge.
pub(crate) fn tags_right(bits: &[Bit], right: f32, cy: f32, size: f32, gap: f32) -> f32 {
    let total: f32 = bits
        .iter()
        .map(|b| tag_width(b.text, size) + gap)
        .sum::<f32>()
        - gap;
    let left = right - total.max(0.);
    tags(bits, left, cy, size, gap);
    left
}
pub(crate) fn pill_width(text: &str, size: f32) -> f32 {
    let h = size * 1.45;
    let text_w = if text.is_empty() {
        0.
    } else {
        text_width(text, style(size, PAPER)) + size * 0.35
    };
    (size * 1.2 + size * 0.95 + text_w).max(h)
}
/// Pills (icon chips) from `x` rightward, centered on `cy`; returns the end.
pub(crate) fn pills(bits: &[Bit], x: f32, cy: f32, size: f32, gap: f32) -> f32 {
    let mut x = x;
    for b in bits {
        x += icon_chip(b.icon, b.text, x, cy - size * 0.725, size, b.color) + gap;
    }
    x - gap
}
pub(crate) fn pills_right(bits: &[Bit], right: f32, cy: f32, size: f32, gap: f32) -> f32 {
    let total: f32 = bits
        .iter()
        .map(|b| pill_width(b.text, size) + gap)
        .sum::<f32>()
        - gap;
    let left = right - total.max(0.);
    pills(bits, left, cy, size, gap);
    left
}
/// A small key cap centered on (`cx`, `cy`).
pub(crate) fn mini_key(key: &str, cx: f32, cy: f32, alpha: f32) {
    let s = ink(11., with_alpha(PAPER, 0.92 * alpha)).with_align(TextAlign::Center);
    let w = (text_width(key, s) + 8.).max(16.);
    let r = Rect::new(cx - w * 0.5, cy - 8., w, 16.);
    let lip = Rect::new(r.x, r.y + 1.5, r.w, r.h);
    shapes::fill_rounded_rect(lip, 3.5, Color::new(0., 0., 0., 0.45 * alpha));
    shapes::fill_rounded_rect(
        r,
        3.5,
        Fill::vertical(
            r,
            Color::new(0.23, 0.28, 0.35, alpha),
            Color::new(0.14, 0.17, 0.22, alpha),
        ),
    );
    shapes::stroke_rounded_rect(r, 3.5, 1., Color::new(1., 1., 1., 0.1 * alpha));
    draw_text_styled(key, cx, s.centered_baseline(cy), s);
}
/// Remaining-of-original pips: lit for what is left, hollow for what is gone.
pub(crate) fn stock_pips(x: f32, cy: f32, left: u16, original: u16, color: Color, known: bool) {
    for n in 0..original {
        let at = vec2(x + 3. + f32::from(n) * 7.5, cy);
        if !known {
            shapes::stroke_circle(at, 2.4, 1., with_alpha(color, 0.35));
        } else if n < left {
            shapes::fill_circle(at, 2.8, color);
        } else {
            shapes::stroke_circle(at, 2.4, 1., with_alpha(color, 0.3));
        }
    }
}
/// The hold as one slot per unit of capacity, filled by cargo color, with
/// `ghost` units of a kind shown as pulsing outlines (a preview).
pub(crate) fn hold_slots(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    game: &Game,
    ghost: Option<(usize, u16)>,
    t: f32,
) {
    let max = game.hold_max();
    let gap = 2.;
    let slot = (w - gap * f32::from(max - 1)) / f32::from(max);
    let mut n = 0;
    for (kind, &count) in game.cargo.iter().enumerate() {
        for _ in 0..count {
            let sx = x + f32::from(n) * (slot + gap);
            rounded_gradient(
                sx,
                y,
                slot,
                h,
                2.5,
                lighten(RESOURCE[kind], 0.15),
                RESOURCE[kind],
            );
            n += 1;
        }
    }
    let beat = 0.45 + 0.4 * pulse(t, 1.6);
    let mut ghosts = ghost.map_or(0, |(_, amount)| amount);
    while n < max {
        let sx = x + f32::from(n) * (slot + gap);
        rounded(sx, y, slot, h, 2.5, Color::new(0.13, 0.18, 0.25, 0.55));
        outline(sx, y, slot, h, 2.5, 1., Color::new(1., 1., 1., 0.07));
        if ghosts > 0
            && let Some((kind, _)) = ghost
        {
            outline(sx, y, slot, h, 2.5, 1.5, with_alpha(RESOURCE[kind], beat));
            ghosts -= 1;
        }
        n += 1;
    }
}
/// Soft light from stacked translucent discs (stays in the main batch).
pub(crate) fn halo(c: Vec2, radius: f32, color: Color) {
    for (k, a) in [(1., 0.05), (0.72, 0.06), (0.45, 0.08)] {
        draw_circle(c.x, c.y, radius * k, with_alpha(color, a));
    }
}
/// Marching dashes from `from` to `to`.
pub(crate) fn marching(from: Vec2, to: Vec2, t: f32, color: Color, width: f32) {
    let len = from.distance(to);
    let dashes = (len / 12.).max(1.) as usize;
    let march = (t * 1.8).fract();
    for k in 0..dashes {
        let s = (k as f32 + march) / dashes as f32;
        let e = s + 0.5 / dashes as f32;
        if e <= 1. {
            let (p, q) = (from.lerp(to, s), from.lerp(to, e));
            draw_line(p.x, p.y, q.x, q.y, width, color);
        }
    }
}
/// The jump gate as three arcs, one per mechanism. Built arcs glow, arcs
/// whose relic is aboard are outlined in gold, the rest are dim; `focus`
/// pulses (a preview of what a build would fill).
pub(crate) fn gate_ring(c: Vec2, r: f32, width: f32, game: &Game, focus: Option<usize>, t: f32) {
    let gap = (width / r).clamp(0.08, 0.3) + 0.06;
    for k in 0..3 {
        let start = -FRAC_PI_2 + k as f32 * TAU / 3. + gap * 0.5;
        let sweep = TAU / 3. - gap;
        let built = game.components[k];
        let ready = game.relics[k];
        if built {
            shapes::stroke_arc(
                c,
                r,
                start,
                sweep,
                Stroke::new(width * 2.2, with_alpha(CYAN, 0.12)).cap(Cap::Round),
            );
            shapes::stroke_arc(c, r, start, sweep, Stroke::new(width, CYAN).cap(Cap::Round));
            shapes::stroke_arc(
                c,
                r,
                start,
                sweep,
                Stroke::new(width * 0.35, lighten(CYAN, 0.6)).cap(Cap::Round),
            );
        } else {
            shapes::stroke_arc(
                c,
                r,
                start,
                sweep,
                Stroke::new(width, Color::new(0.13, 0.2, 0.29, 1.)).cap(Cap::Round),
            );
            if ready {
                shapes::stroke_arc(
                    c,
                    r,
                    start,
                    sweep,
                    Stroke::new(width * 0.3, with_alpha(GOLD, 0.75)).cap(Cap::Round),
                );
            }
        }
        if focus == Some(k) && !built {
            let beat = pulse(t, 1.4);
            shapes::stroke_arc(
                c,
                r,
                start,
                sweep,
                Stroke::new(
                    width * (1.2 + 0.5 * beat),
                    with_alpha(GOLD, 0.25 + 0.35 * beat),
                )
                .cap(Cap::Round),
            );
        }
    }
}
/// Midpoint of gate arc `k`, for badges and effects.
pub(crate) fn gate_point(c: Vec2, r: f32, k: usize) -> Vec2 {
    let a = -FRAC_PI_2 + (k as f32 + 0.5) * TAU / 3.;
    c + vec2(a.cos(), a.sin()) * r
}
/// A single gate part (one arc of three) as a small glyph.
pub(crate) fn gate_piece(c: Vec2, size: f32, k: usize, color: Color) {
    let r = size * 0.36;
    for j in 0..3 {
        let start = -FRAC_PI_2 + j as f32 * TAU / 3. + 0.2;
        let (w, col) = if j == k {
            (size * 0.2, color)
        } else {
            (size * 0.09, with_alpha(color, 0.3 * color.a))
        };
        shapes::stroke_arc(
            c,
            r,
            start,
            TAU / 3. - 0.4,
            Stroke::new(w, col).cap(Cap::Round),
        );
    }
}

/// The panel button at the shared hit region: a key cap, the action's icon
/// and verb, and in a dark well either its consequences or why it is refused.
pub(crate) fn button(
    glyph: Icon,
    verb: &str,
    bits: &[Bit],
    refusal: Option<(Icon, String)>,
    live: bool,
    p: Vec2,
    t: f32,
) {
    let enabled = refusal.is_none() && live;
    let hovered = live && button_hit(p);
    let (x, w, h) = (COL, COL_W, BUTTON_H);
    let y = BUTTON_Y - if hovered && enabled { 1.5 } else { 0. };
    if enabled {
        let top = lighten(
            GOLD,
            if hovered {
                0.22
            } else {
                0.06 + 0.06 * pulse(t, 0.7)
            },
        );
        rounded_gradient(x, y, w, h, 9., top, darken(GOLD, 0.12));
    } else {
        rounded(x, y, w, h, 9., with_alpha(DIM, 0.85));
        if hovered {
            outline(x, y, w, h, 9., 1., with_alpha(MUTED, 0.4));
        }
    }
    let cy = y + h * 0.5;
    let cap = keycap("Space", "", x + 6., y + 5., MUTED);
    let fg = if enabled { INK } else { MUTED };
    icon(glyph, x + 6. + cap + 15., cy, 17., fg);
    line(verb, x + 6. + cap + 29., cy, 96., 14., fg);
    let well = Color::new(0.04, 0.07, 0.11, 0.92);
    if let Some((why, word)) = &refusal {
        let ww = tag_width(word, 14.) + 14.;
        let left = x + w - 4. - ww;
        rounded(left, y + 4., ww, h - 8., 7., well);
        tag(
            *why,
            word,
            left + 7.,
            cy,
            14.,
            Color::new(1., 0.64, 0.62, 1.),
        );
    } else if !bits.is_empty() {
        let ww: f32 = bits
            .iter()
            .map(|b| tag_width(b.text, 14.) + 8.)
            .sum::<f32>()
            + 6.;
        let left = x + w - 4. - ww;
        rounded(left, y + 4., ww, h - 8., 7., well);
        tags(bits, left + 7., cy, 14., 8.);
    }
}

// ---------------------------------------------------------------------------
// Chart art

/// Shortest lane route from `from` to `to`, lowest indices winning ties:
/// the sectors after `from` (ending at `to`) and how many there are.
pub(crate) fn route(from: usize, to: usize) -> ([usize; SECTORS], usize) {
    let mut prev = [usize::MAX; SECTORS];
    let mut queue = [0; SECTORS];
    let (mut head, mut tail) = (0, 1);
    queue[0] = from;
    prev[from] = from;
    while head < tail {
        let a = queue[head];
        head += 1;
        for (b, from) in prev.iter_mut().enumerate() {
            if *from == usize::MAX && Game::adjacent(a, b) {
                *from = a;
                queue[tail] = b;
                tail += 1;
            }
        }
    }
    let mut path = [0; SECTORS];
    if from == to || prev[to] == usize::MAX {
        return (path, 0);
    }
    let mut n = 0;
    let mut at = to;
    while at != from {
        path[n] = at;
        n += 1;
        at = prev[at];
    }
    path[..n].reverse();
    (path, n)
}

/// Twinkling starfield inside the left panel.
pub(crate) fn backdrop(t: f32) {
    rounded_gradient(
        MAP.x,
        MAP.y,
        MAP.w,
        MAP.h,
        12.,
        Color::new(0.035, 0.07, 0.125, 1.),
        Color::new(0.02, 0.035, 0.07, 1.),
    );
    for n in 0..85 {
        let x = 31. + ((n * 139 + 29) % 570) as f32;
        let y = 157. + ((n * 71 + 13) % 375) as f32;
        let twinkle = 0.55 + 0.45 * (t * 2.1 + n as f32 * 2.3).sin();
        let big = n % 11 == 0;
        let size = if big { 2. } else { 1.2 };
        let a = if big {
            0.5 + 0.4 * twinkle
        } else {
            0.25 + 0.3 * twinkle
        };
        panel(
            x - size * 0.5,
            y - size * 0.5,
            size,
            size,
            Color::new(0.7, 0.84, 0.95, a),
        );
    }
    outline(
        MAP.x,
        MAP.y,
        MAP.w,
        MAP.h,
        12.,
        1.,
        Color::new(1., 1., 1., 0.06),
    );
}

/// The dying star in the chart's corner: it swells, reddens and beats faster
/// as watches pass, so the deadline is always in view.
pub(crate) fn dying_star(watch: u16, t: f32) {
    let f = f32::from(watch.min(DEADLINE)) / f32::from(DEADLINE);
    let c = vec2(MAP.x + 4., MAP.y + 4.);
    let beat = pulse(t, 0.22 + f * 1.3);
    let r = 58. + 92. * f + 5. * beat;
    let hot = shapes::mix(
        Color::new(1., 0.8, 0.42, 1.),
        Color::new(1., 0.38, 0.26, 1.),
        f,
    );
    let light = lighten(hot, 0.35);
    // A soft corona from stacked fades, steeper toward the edge.
    for (scale, alpha) in [(1.55, 0.09), (1.2, 0.12)] {
        let rr = r * scale * (1. + 0.03 * beat);
        shapes::fill_sector(
            c,
            rr,
            0.,
            FRAC_PI_2,
            Fill::radial(c, rr, with_alpha(light, alpha), with_alpha(light, 0.)),
        );
    }
    let body = r * 0.6;
    shapes::fill_sector(
        c,
        body,
        0.,
        FRAC_PI_2,
        Fill::radial(c, body, lighten(hot, 0.7), hot),
    );
    // Prominences loop off the limb; they grow restless near the end.
    for k in 0..3 {
        let a = 0.3 + k as f32 * 0.48 + 0.06 * (t * 0.3 + k as f32).sin();
        let lift = body * (1.05 + 0.06 * (t * (0.5 + f * 1.5) + k as f32 * 2.1).sin().abs());
        shapes::stroke_arc(
            c,
            lift,
            a,
            0.14 + 0.06 * f,
            Stroke::new(2.5, with_alpha(light, 0.55)).cap(Cap::Round),
        );
    }
    shapes::stroke_arc(
        c,
        body,
        0.04,
        FRAC_PI_2 - 0.08,
        Stroke::new(1.5, with_alpha(lighten(hot, 0.6), 0.8)),
    );
}

pub(crate) fn ship(x: f32, y: f32, scale: f32, burn: bool) {
    let p = |dx: f32, dy: f32| vec2(x + dx * scale, y + dy * scale);
    if burn {
        draw_triangle(
            p(-13., -5.),
            p(-31., 0.),
            p(-13., 5.),
            with_alpha(CYAN, 0.55),
        );
        draw_triangle(
            p(-13., -3.),
            p(-23., 0.),
            p(-13., 3.),
            Color::new(0.85, 1., 1., 0.9),
        );
    }
    // Shadowed lower hull, lit upper hull, canopy stripe.
    draw_triangle(p(22., 0.), p(-16., -12.), p(-9., 0.), PAPER);
    draw_triangle(
        p(22., 0.),
        p(-16., 12.),
        p(-9., 0.),
        tint([0.6, 0.75, 0.88], 0.8),
    );
    draw_triangle(
        p(22., 0.),
        p(-16., -12.),
        p(-2., -5.),
        Color::new(1., 1., 1., 0.35),
    );
    draw_line(x - 4. * scale, y, x + 12. * scale, y, 3. * scale, CYAN);
}

pub(crate) fn planet(i: usize, t: f32, built: [bool; 3]) {
    let p = &CHART[i];
    let (x, y) = (p.x, p.y);
    let c = vec2(x, y);
    let r = 16.;
    match i {
        0 => {
            // A ring station whose outer ring is the gate: one arc per part.
            for (k, &done) in built.iter().enumerate() {
                let start = -FRAC_PI_2 + k as f32 * TAU / 3. + 0.14;
                if done {
                    shapes::stroke_arc(
                        c,
                        r + 7.,
                        start,
                        TAU / 3. - 0.28,
                        Stroke::new(9., with_alpha(GOLD, 0.18)).cap(Cap::Round),
                    );
                }
                shapes::stroke_arc(
                    c,
                    r + 7.,
                    start,
                    TAU / 3. - 0.28,
                    Stroke::new(5., if done { GOLD } else { tint(p.color, 0.42) }).cap(Cap::Round),
                );
            }
            draw_circle_lines(x, y, r + 1., 2., CYAN);
            let spin = t * 0.24;
            let lit = (t * 3.) as usize;
            for j in 0..6 {
                let a = spin + j as f32 * TAU / 6.;
                draw_circle(
                    x + a.cos() * (r + 1.),
                    y + a.sin() * (r + 1.),
                    1.8,
                    if (j + lit).is_multiple_of(6) {
                        CYAN
                    } else {
                        PAPER
                    },
                );
            }
            draw_rectangle(x - 11., y - 2., 22., 4., DIM);
            draw_circle(x, y, 4., tint(p.color, 0.9));
        }
        1 | 6 => {
            // Slowly tumbling asteroid field.
            let spin = t * 0.09 * if i == 1 { 1. } else { -1. };
            for j in 0..9 {
                let a = j as f32 * 2.4 + spin;
                let d = 7. + (j % 3) as f32 * 7.;
                let rr = 3. + (j % 2) as f32 * 2.;
                let (px, py) = (x + a.cos() * d, y + a.sin() * d * 0.7);
                draw_circle(px, py, rr, tint(p.color, 0.42 + j as f32 * 0.03));
                draw_circle(
                    px - rr * 0.3,
                    py - rr * 0.3,
                    rr * 0.62,
                    tint(p.color, 0.7 + j as f32 * 0.03),
                );
            }
        }
        3 => {
            // Derelict wreck with a blinking distress light.
            draw_rectangle(x - 22., y - 4., 42., 8., tint(p.color, 0.75));
            draw_rectangle(x - 22., y - 4., 42., 3., tint(p.color, 0.95));
            draw_rectangle(x - 9., y - 15., 4., 30., PAPER);
            draw_rectangle(x + 6., y - 12., 5., 20., tint(p.color, 0.6));
            let blink = if ((t * 2.) as u32).is_multiple_of(2) {
                GOLD
            } else {
                darken(GOLD, 0.5)
            };
            draw_circle(x + 27., y + 9., 3., blink);
        }
        _ => {
            if i == 2 {
                // Comet tail, flickering gently.
                let flick = 0.18 + 0.06 * (t * 3.1).sin();
                draw_triangle(
                    vec2(x - 12., y - 6.),
                    vec2(x - 60., y - 32.),
                    vec2(x + 2., y + 10.),
                    Color::new(p.color[0], p.color[1], p.color[2], flick),
                );
                draw_triangle(
                    vec2(x - 10., y - 3.),
                    vec2(x - 42., y - 22.),
                    vec2(x, y + 6.),
                    Color::new(p.color[0], p.color[1], p.color[2], 0.22),
                );
            }
            // Lit sphere: radial light from the upper left, then a rim.
            shapes::fill_circle(
                c,
                r,
                Fill::radial(
                    c - vec2(r * 0.45, r * 0.5),
                    r * 1.75,
                    tint(p.color, 1.15),
                    tint(p.color, 0.32),
                ),
            );
            if i == 4 {
                draw_line(x - r * 0.5, y - r, x + r * 0.2, y + r, 4., BACK);
            }
            if i == 5 {
                draw_circle(x - 5., y + 4., 5., tint([0.12, 0.42, 0.35], 1.));
                draw_circle(x + 9., y - 7., 4., tint([0.12, 0.42, 0.35], 1.));
            }
            shapes::stroke_arc(
                c,
                r - 0.5,
                PI * 0.95,
                PI * 0.6,
                Stroke::new(1.5, Color::new(1., 1., 1., 0.35)).cap(Cap::Round),
            );
            if i == 7 {
                draw_circle_lines(x, y, r + 8., 2., GOLD);
                draw_line(x - 28., y + 9., x + 28., y - 9., 3., GOLD);
            }
            if i == 8 {
                draw_line(x + 4., y - r, x + 4., y - 28., 2., PAPER);
                let beat = pulse(t, 0.8);
                draw_circle(x + 4., y - 30., 3., GOLD);
                draw_circle(
                    x + 4.,
                    y - 30.,
                    6.,
                    Color::new(1., 0.8, 0.4, 0.18 + 0.2 * beat),
                );
            }
        }
    }
}

/// Where the gate is drawn on the Forge and Mission panels.
pub(crate) const FORGE_GATE: Vec2 = Vec2::new(178., 300.);
pub(crate) const MISSION_GATE: Vec2 = Vec2::new(186., 338.);
/// Where an encounter's prize floats on its card.
pub(crate) const PRIZE_AT: Vec2 = Vec2::new(790., 238.);
/// The ship on the Ship panel and the upgrade bays on the Forge panel.
pub(crate) const SHIP_AT: Vec2 = Vec2::new(318., 270.);
pub(crate) const BAY: [Vec2; 3] = [
    Vec2::new(392., 392.),
    Vec2::new(452., 392.),
    Vec2::new(512., 392.),
];

/// A selectable row's surface; returns the row's (lifted) vertical center.
pub(crate) fn row_frame(y: f32, selected: bool, hovered: bool, ok: bool, t: f32) -> f32 {
    let y = y - if hovered { 1.5 } else { 0. };
    if selected {
        rounded_gradient(COL, y, COL_W, ROW_H, 8., lighten(DIM, 0.12), DIM);
        outline(
            COL,
            y,
            COL_W,
            ROW_H,
            8.,
            1.,
            with_alpha(GOLD, 0.4 + 0.25 * pulse(t, 0.8)),
        );
        shapes::fill_capsule(vec2(COL + 4.5, y + 8.), vec2(COL + 4.5, y + 22.), 1.5, GOLD);
    } else if hovered {
        rounded(COL, y, COL_W, ROW_H, 8., Color::new(0.12, 0.19, 0.27, 0.95));
        outline(COL, y, COL_W, ROW_H, 8., 1., with_alpha(CYAN, 0.38));
    } else {
        rounded(
            COL,
            y,
            COL_W,
            ROW_H,
            8.,
            with_alpha(DIM, if ok { 0.55 } else { 0.28 }),
        );
    }
    y + ROW_H * 0.5
}

/// Watches-left color: calm, then warning, then alarm.
pub(crate) fn watch_color(left: u16) -> Color {
    if left < 8 {
        RED
    } else if left < 16 {
        GOLD
    } else {
        TIME
    }
}
