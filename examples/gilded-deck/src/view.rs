//! Read-only velvet table, parchment cards and court portraits.
use crate::{
    content::{Card, ENCOUNTERS, Kind, RELIC_NAMES, RELIC_TEXT, REWARDS},
    model::{Game, Phase},
};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;

const GOLD: Color = Color::new(0.91, 0.73, 0.38, 1.);
const CREAM: Color = Color::new(0.96, 0.90, 0.73, 1.);
const PLUM: Color = Color::new(0.20, 0.10, 0.22, 1.);
const VELVET: Color = Color::new(0.13, 0.08, 0.15, 1.);
const RED: Color = Color::new(0.96, 0.39, 0.40, 1.);
const GREEN: Color = Color::new(0.44, 0.83, 0.66, 1.);
const VIOLET: Color = Color::new(0.76, 0.58, 0.96, 1.);
const STEEL: Color = Color::new(0.62, 0.70, 0.78, 1.);
const SOFT: Color = Color::new(0.80, 0.76, 0.66, 1.);
const PARCHMENT: Color = Color::new(0.97, 0.92, 0.80, 1.);
const PARCHMENT_LOW: Color = Color::new(0.87, 0.79, 0.63, 1.);
const SEPIA: Color = Color::new(0.24, 0.16, 0.10, 1.);
const SEPIA_MUTED: Color = Color::new(0.45, 0.35, 0.25, 1.);

// ---------------------------------------------------------------------------
// Typography: shadowless faces on solid panels, shadows only for titles.

fn plain(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
fn say(value: &str, x: f32, y: f32, s: TextStyle) -> f32 {
    draw_text_styled(value, x, y, s).width
}
/// Draw `value` shrunk (never enlarged) to `max` units.
fn say_fit(value: &str, x: f32, y: f32, max: f32, s: TextStyle) -> f32 {
    let w = text_width(value, s);
    let s = if w > max && w > 0. {
        s.with_size(s.size * max / w)
    } else {
        s
    };
    draw_text_styled(value, x, y, s).width
}
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    say(value, x, y, plain(11., color).with_letter_spacing(1.5))
}
/// Wrapped paragraph; returns the next baseline.
fn para(value: &str, x: f32, y: f32, max: f32, s: TextStyle, line: f32) -> f32 {
    let mut baseline = y;
    for row in wrap_text(value, max, s) {
        draw_text_styled(row, x, baseline, s);
        baseline += line;
    }
    baseline
}

/// Flat rounded rectangle from a small triangle fan. No anti-aliasing, but
/// roughly a twentieth of the CPU cost of an AA shape: use it for shadows,
/// low-contrast tints and small marks.
fn flat_round(x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    let r = r.min(w * 0.5).min(h * 0.5).max(0.);
    let centre = vec2(x + w * 0.5, y + h * 0.5);
    let corners = [
        (x + w - r, y + h - r, 0.),
        (x + r, y + h - r, 0.5),
        (x + r, y + r, 1.),
        (x + w - r, y + r, 1.5),
    ];
    let mut first = None;
    let mut prev: Option<Vec2> = None;
    for (cx, cy, start) in corners {
        for k in 0..=3 {
            let a = (start + k as f32 / 6.) * std::f32::consts::PI;
            let p = vec2(cx + a.cos() * r, cy + a.sin() * r);
            if let Some(q) = prev {
                draw_triangle(centre, q, p, color);
            } else {
                first = Some(p);
            }
            prev = Some(p);
        }
    }
    if let (Some(a), Some(b)) = (prev, first) {
        draw_triangle(centre, a, b, color);
    }
}

// ---------------------------------------------------------------------------
// Card identity.

#[derive(Clone, Copy, PartialEq, Eq)]
enum Suit {
    Attack,
    Skill,
    Arcane,
    Curse,
}
fn suit(kind: Kind) -> Suit {
    match kind {
        Kind::Guard | Kind::Fortify | Kind::Ward | Kind::Purify => Suit::Skill,
        Kind::Venom | Kind::Insight | Kind::Focus | Kind::Spark => Suit::Arcane,
        Kind::Wound => Suit::Curse,
        _ => Suit::Attack,
    }
}
fn suit_name(s: Suit) -> &'static str {
    match s {
        Suit::Attack => "Attack",
        Suit::Skill => "Skill",
        Suit::Arcane => "Arcane",
        Suit::Curse => "Curse",
    }
}
/// Art-window gradient and the icon's light ink for a suit.
fn suit_colors(s: Suit) -> (Color, Color, Color) {
    match s {
        Suit::Attack => (
            Color::new(0.56, 0.17, 0.18, 1.),
            Color::new(0.27, 0.06, 0.09, 1.),
            Color::new(1.0, 0.82, 0.48, 1.),
        ),
        Suit::Skill => (
            Color::new(0.13, 0.43, 0.37, 1.),
            Color::new(0.04, 0.20, 0.18, 1.),
            Color::new(0.62, 0.95, 0.80, 1.),
        ),
        Suit::Arcane => (
            Color::new(0.38, 0.22, 0.55, 1.),
            Color::new(0.15, 0.08, 0.27, 1.),
            Color::new(0.86, 0.74, 1.0, 1.),
        ),
        Suit::Curse => (
            Color::new(0.33, 0.27, 0.28, 1.),
            Color::new(0.14, 0.11, 0.12, 1.),
            Color::new(0.95, 0.50, 0.48, 1.),
        ),
    }
}
fn card_title(card: Card) -> String {
    if card.upgraded {
        format!("{}+", card.name())
    } else {
        card.name().to_string()
    }
}
fn type_line(card: Card) -> &'static str {
    match (suit(card.kind), card.exhausts()) {
        (Suit::Curse, _) => "Curse",
        (Suit::Attack, true) => "Attack · Exhaust",
        (Suit::Skill, true) => "Skill · Exhaust",
        (Suit::Arcane, true) => "Arcane · Exhaust",
        (s, false) => suit_name(s),
    }
}

fn diamond(x: f32, y: f32, r: f32, color: Color) {
    draw_poly(x, y, 4, r, 0., color);
}
/// A faceted gem: darker lower half, lit upper facet and a glint.
fn gem(x: f32, y: f32, r: f32, color: Color) {
    diamond(x, y + 1., r + 1.5, Color::new(0., 0., 0., 0.35));
    diamond(x, y, r, darken(color, 0.25));
    draw_triangle(
        vec2(x - r, y),
        vec2(x, y - r),
        vec2(x + r, y),
        lighten(color, 0.12),
    );
    draw_triangle(
        vec2(x - r * 0.45, y - r * 0.2),
        vec2(x, y - r * 0.7),
        vec2(x + r * 0.1, y - r * 0.25),
        Color::new(1., 1., 1., 0.55),
    );
}
fn card_icon(kind: Kind, x: f32, y: f32, size: f32, c: Color) {
    let shade = Color::new(0., 0., 0., 0.28);
    match suit(kind) {
        Suit::Skill => {
            // Heraldic shield.
            let top = y - size;
            for (dx, dy, col) in [(1.5, 2., shade), (0., 0., c)] {
                flat_round(
                    x - size + dx,
                    top + dy,
                    size * 2.,
                    size * 0.9,
                    size * 0.2,
                    col,
                );
                draw_triangle(
                    vec2(x - size + dx, top + size * 0.8 + dy),
                    vec2(x + size + dx, top + size * 0.8 + dy),
                    vec2(x + dx, y + size + dy),
                    col,
                );
            }
            draw_line(x, top + 3., x, y + size - 5., 2., darken(c, 0.4));
            draw_line(
                x - size + 4.,
                top + size * 0.45,
                x + size - 4.,
                top + size * 0.45,
                2.,
                darken(c, 0.4),
            );
        }
        Suit::Arcane => match kind {
            Kind::Spark => {
                diamond(x + 1.5, y + 2., size, shade);
                diamond(x, y, size, c);
                diamond(x, y, size * 0.45, CREAM);
            }
            Kind::Venom => {
                draw_circle_v(vec2(x + 1.5, y + size * 0.3 + 2.), size * 0.7, shade);
                draw_circle_v(vec2(x, y + size * 0.3), size * 0.7, c);
                draw_triangle(
                    vec2(x - size * 0.55, y),
                    vec2(x, y - size),
                    vec2(x + size * 0.55, y),
                    c,
                );
                draw_circle_v(vec2(x - size * 0.25, y + size * 0.15), size * 0.18, CREAM);
            }
            _ => {
                // An open eye.
                draw_ellipse(x + 1.5, y + 2., size, size * 0.55, 0., shade);
                draw_ellipse(x, y, size, size * 0.55, 0., c);
                draw_circle_v(vec2(x, y), size * 0.36, darken(c, 0.65));
                draw_circle_v(vec2(x - size * 0.12, y - size * 0.12), size * 0.12, CREAM);
            }
        },
        Suit::Curse => {
            draw_line(x - size, y - size, x + size, y + size, 4., c);
            draw_line(x + size, y - size, x - size, y + size, 4., c);
        }
        Suit::Attack => match kind {
            Kind::Siphon | Kind::Gambit => {
                // A blood drop.
                draw_circle_v(vec2(x + 1.5, y + size * 0.3 + 2.), size * 0.65, shade);
                draw_circle_v(vec2(x, y + size * 0.3), size * 0.65, c);
                draw_triangle(
                    vec2(x - size * 0.58, y + size * 0.05),
                    vec2(x, y - size),
                    vec2(x + size * 0.58, y + size * 0.05),
                    c,
                );
                draw_circle_v(vec2(x - size * 0.22, y + size * 0.2), size * 0.16, CREAM);
            }
            Kind::Crush => {
                // Hammer.
                draw_line(
                    x - size * 0.6,
                    y + size,
                    x + size * 0.3,
                    y - size * 0.2,
                    3.5,
                    CREAM,
                );
                flat_round(x - size * 0.2, y - size, size * 1.3, size * 0.7, 2., c);
            }
            Kind::Finale => {
                diamond(x + 1.5, y + 2., size, shade);
                diamond(x, y, size, c);
                diamond(x, y, size * 0.5, CREAM);
                diamond(x, y, size * 0.2, c);
            }
            _ => {
                // A sword: blade, guard and grip.
                let tip = vec2(x + size * 0.85, y - size);
                let hilt = vec2(x - size * 0.45, y + size * 0.45);
                draw_line(
                    hilt.x + 1.5,
                    hilt.y + 2.,
                    tip.x + 1.5,
                    tip.y + 2.,
                    4.5,
                    shade,
                );
                draw_line(hilt.x, hilt.y, tip.x, tip.y, 4.5, c);
                draw_line(hilt.x + 1., hilt.y - 1., tip.x, tip.y, 1.5, CREAM);
                draw_line(
                    hilt.x - size * 0.45,
                    hilt.y - size * 0.45,
                    hilt.x + size * 0.45,
                    hilt.y + size * 0.45,
                    3.,
                    CREAM,
                );
                draw_line(
                    hilt.x,
                    hilt.y,
                    hilt.x - size * 0.45,
                    hilt.y + size * 0.45,
                    3.5,
                    darken(c, 0.35),
                );
            }
        },
    }
}

/// Presentation state for one card face.
#[derive(Clone, Copy)]
struct Look {
    selected: bool,
    affordable: bool,
}

/// A parchment card: leather or gilt rim, cost gem, suit art and rules text.
fn card_face(card: Card, x: f32, y: f32, w: f32, h: f32, look: Look) {
    let radius = 9.;
    // Contact shadow; the raised card casts a longer, softer one.
    if look.selected {
        flat_round(
            x - 3.,
            y + 6.,
            w + 6.,
            h + 5.,
            radius + 3.,
            Color::new(0., 0., 0., 0.2),
        );
        flat_round(x, y + 9., w, h, radius, Color::new(0., 0., 0., 0.3));
        rounded(
            x - 3.,
            y - 3.,
            w + 6.,
            h + 6.,
            radius + 3.,
            with_alpha(GOLD, 0.75),
        );
    } else {
        draw_rectangle(
            x + 5.,
            y + 8.,
            w - 10.,
            h - 4.,
            Color::new(0., 0., 0., 0.38),
        );
    }
    let wound = card.kind == Kind::Wound;
    let (rim_top, rim_bottom) = if card.upgraded {
        (
            Color::new(1., 0.88, 0.55, 1.),
            Color::new(0.68, 0.48, 0.18, 1.),
        )
    } else if look.selected {
        (
            Color::new(0.95, 0.80, 0.50, 1.),
            Color::new(0.58, 0.40, 0.18, 1.),
        )
    } else {
        (
            Color::new(0.40, 0.28, 0.18, 1.),
            Color::new(0.22, 0.14, 0.09, 1.),
        )
    };
    rounded_gradient(x, y, w, h, radius, rim_top, rim_bottom);
    let (body_top, body_bottom) = if wound {
        (
            Color::new(0.62, 0.55, 0.54, 1.),
            Color::new(0.44, 0.37, 0.37, 1.),
        )
    } else {
        (PARCHMENT, PARCHMENT_LOW)
    };
    rounded_gradient(
        x + 3.,
        y + 3.,
        w - 6.,
        h - 6.,
        radius - 2.,
        body_top,
        body_bottom,
    );
    // Title, kept clear of the cost gem.
    let gem_r = (w * 0.075).clamp(9., 13.);
    let title = card_title(card);
    let title_size = (h * 0.095).clamp(12., 17.);
    let left = x + gem_r * 2. + 6.;
    let room = x + w - 8. - left;
    let st = plain(title_size, SEPIA);
    let tw = text_width(&title, st);
    let st = if tw > room {
        st.with_size(title_size * room / tw)
    } else {
        st
    };
    let cx = left + room * 0.5;
    draw_text_styled(
        &title,
        cx,
        st.centered_baseline(y + gem_r + 4.),
        st.with_align(TextAlign::Center),
    );
    // Cost gem.
    let gc = vec2(x + gem_r + 3., y + gem_r + 3.);
    if !wound {
        let (inner, outer) = if look.affordable {
            (
                Color::new(1., 0.90, 0.60, 1.),
                Color::new(0.80, 0.56, 0.20, 1.),
            )
        } else {
            (
                Color::new(0.80, 0.55, 0.52, 1.),
                Color::new(0.52, 0.24, 0.24, 1.),
            )
        };
        draw_circle(gc.x, gc.y, gem_r + 1.5, SEPIA);
        draw_circle(gc.x, gc.y, gem_r, outer);
        draw_circle(
            gc.x - gem_r * 0.12,
            gc.y - gem_r * 0.12,
            gem_r * 0.78,
            inner,
        );
        let cs = plain(gem_r * 1.25, SEPIA);
        draw_text_styled(
            &card.cost().to_string(),
            gc.x,
            cs.centered_baseline(gc.y),
            cs.with_align(TextAlign::Center),
        );
    }
    // Art window with a suit-tinted light pool.
    let (top, bottom, ink) = suit_colors(suit(card.kind));
    let ax = x + 8.;
    let ay = y + gem_r * 2. + 9.;
    let aw = w - 16.;
    let ah = h * 0.4;
    rounded_gradient(ax, ay, aw, ah, 5., top, bottom);
    let centre = vec2(ax + aw * 0.5, ay + ah * 0.48);
    draw_circle(
        centre.x,
        centre.y,
        ah * 0.42,
        with_alpha(lighten(top, 0.3), 0.35),
    );
    card_icon(card.kind, centre.x, centre.y, ah * 0.3, ink);
    // Type banner straddling the art window's lower edge.
    let line = type_line(card);
    let bs = plain(10., if card.upgraded { GOLD } else { CREAM });
    let bw = text_width(line, bs) + 14.;
    let by = ay + ah - 7.;
    let bx = x + (w - bw) * 0.5;
    draw_triangle(
        vec2(bx - 4., by + 2.),
        vec2(bx + 4., by + 7.5),
        vec2(bx - 4., by + 13.),
        darken(SEPIA, 0.3),
    );
    draw_triangle(
        vec2(bx + bw + 4., by + 2.),
        vec2(bx + bw - 4., by + 7.5),
        vec2(bx + bw + 4., by + 13.),
        darken(SEPIA, 0.3),
    );
    draw_rectangle(bx - 4., by + 2., 4., 11., darken(SEPIA, 0.3));
    draw_rectangle(bx + bw, by + 2., 4., 11., darken(SEPIA, 0.3));
    draw_rectangle(bx, by, bw, 15., SEPIA);
    draw_text_styled(
        line,
        x + w * 0.5,
        bs.centered_baseline(by + 7.5),
        bs.with_align(TextAlign::Center),
    );
    // Rules text, centred and sized to the remaining space.
    let desc = card.description();
    let max_w = w - 18.;
    let mut size = (h * 0.075).clamp(10.5, 14.);
    let top_line = by + 21. + size * 0.8;
    let room_h = y + h - 8. - top_line + size * 0.9;
    let mut ds = plain(size, if wound { CREAM } else { SEPIA });
    while size > 9. && wrap_text(&desc, max_w, ds).count() as f32 * size * 1.22 > room_h {
        size -= 0.5;
        ds = ds.with_size(size);
    }
    let mut baseline = top_line;
    for row in wrap_text(&desc, max_w, ds) {
        draw_text_styled(row, x + w * 0.5, baseline, ds.with_align(TextAlign::Center));
        baseline += size * 1.22;
    }
    if !look.affordable && !wound {
        flat_round(
            x + 1.,
            y + 1.,
            w - 2.,
            h - 2.,
            radius - 1.,
            Color::new(0.02, 0.04, 0.05, 0.32),
        );
    }
}

/// Compact card chip for lists: suit-tinted tile with the cost gem.
fn mini_card(card: Card, x: f32, y: f32, w: f32, h: f32) {
    let (top, bottom, ink) = suit_colors(suit(card.kind));
    flat_round(x, y + 2., w, h, 5., Color::new(0., 0., 0., 0.35));
    rounded_gradient(
        x,
        y,
        w,
        h,
        5.,
        if card.upgraded { GOLD } else { PARCHMENT },
        if card.upgraded {
            darken(GOLD, 0.3)
        } else {
            PARCHMENT_LOW
        },
    );
    flat_round(
        x + 3.,
        y + 3.,
        w - 6.,
        h - 6.,
        3.,
        shapes::mix(top, bottom, 0.5),
    );
    card_icon(card.kind, x + w * 0.5, y + h * 0.5, h * 0.26, ink);
}

// ---------------------------------------------------------------------------
// Surfaces.

/// Raised velvet panel: offset shadow, hairline rim and a lit gradient.
/// A lighter-weight sibling of the host's `card` for in-world panels.
fn slab(x: f32, y: f32, w: f32, h: f32, fill: Color) {
    flat_round(x, y + 4., w, h, 11., Color::new(0., 0., 0., 0.32));
    rounded_gradient(
        x,
        y,
        w,
        h,
        11.,
        Color::new(1., 1., 1., 0.13),
        Color::new(1., 1., 1., 0.04),
    );
    rounded_gradient(
        x + 1.,
        y + 1.,
        w - 2.,
        h - 2.,
        10.,
        lighten(fill, 0.04),
        darken(fill, 0.04),
    );
}
/// Rimmed list tile: gilt when selected, quiet velvet otherwise.
fn tile(x: f32, y: f32, w: f32, h: f32, selected: bool) {
    flat_round(x, y + 3., w, h, 9., Color::new(0., 0., 0., 0.3));
    if selected {
        rounded_gradient(
            x,
            y,
            w,
            h,
            9.,
            Color::new(1., 0.85, 0.55, 1.),
            darken(GOLD, 0.35),
        );
        flat_round(
            x + 1.5,
            y + 1.5,
            w - 3.,
            h - 3.,
            7.5,
            Color::new(0.27, 0.15, 0.28, 1.),
        );
    } else {
        rounded_gradient(
            x,
            y,
            w,
            h,
            9.,
            Color::new(1., 1., 1., 0.1),
            Color::new(1., 1., 1., 0.03),
        );
        flat_round(
            x + 1.,
            y + 1.,
            w - 2.,
            h - 2.,
            8.,
            Color::new(0.085, 0.12, 0.13, 1.),
        );
    }
}
/// Small tinted pill with text; returns its width.
fn pill(label: &str, x: f32, y: f32, color: Color) -> f32 {
    let s = plain(12., color);
    let w = text_width(label, s) + 14.;
    flat_round(x, y, w, 19., 9.5, with_alpha(color, 0.16));
    draw_text_styled(label, x + 7., s.centered_baseline(y + 9.5), s);
    w
}
fn pills_width(items: &[(String, Color)]) -> f32 {
    items
        .iter()
        .map(|(l, _)| text_width(l, plain(12., CREAM)) + 14. + 6.)
        .sum::<f32>()
        - 6.
}
/// Lay out pills left to right, wrapping at `right`.
fn pill_flow(items: &[(String, Color)], x: f32, y: f32, right: f32) {
    let (mut px, mut py) = (x, y);
    for (label, color) in items {
        let w = text_width(label, plain(12., *color)) + 14.;
        if px + w > right && px > x {
            px = x;
            py += 24.;
        }
        px += pill(label, px, py, *color) + 6.;
    }
}
fn button(r: Rect, key: &str, label: &str, primary: bool, enabled: bool) {
    let (x, y, w, h) = (r.x, r.y, r.w, r.h);
    let a = if enabled { 1. } else { 0.45 };
    flat_round(x, y + 3., w, h, 10., Color::new(0., 0., 0., 0.35 * a));
    if primary {
        rounded_gradient(
            x,
            y,
            w,
            h,
            10.,
            Color::new(1., 0.86, 0.56, a),
            Color::new(0.80, 0.58, 0.24, a),
        );
    } else {
        rounded_gradient(
            x,
            y,
            w,
            h,
            10.,
            Color::new(0.42, 0.24, 0.42, a),
            Color::new(0.24, 0.12, 0.26, a),
        );
    }
    let ink = with_alpha(if primary { SEPIA } else { CREAM }, a);
    let total = key_width(key, label);
    key_hint(key, label, x + (w - total) * 0.5, y + (h - 20.) * 0.5, ink);
}
fn draw_circle_v(c: Vec2, r: f32, color: Color) {
    draw_circle(c.x, c.y, r, color);
}
/// Health bar: a recessed flat track and an anti-aliased lit fill.
fn hp_bar(x: f32, y: f32, w: f32, h: f32, fraction: f32, color: Color) {
    let f = if fraction.is_finite() {
        fraction.clamp(0., 1.)
    } else {
        0.
    };
    flat_round(x, y, w, h, h * 0.5, Color::new(0., 0., 0., 0.45));
    if f > 0. {
        let fw = (w * f).max(h);
        rounded_gradient(x, y, fw, h, h * 0.5, lighten(color, 0.2), color);
        draw_rectangle(
            x + h * 0.5,
            y + 1.,
            (fw - h).max(0.),
            1.,
            Color::new(1., 1., 1., 0.25),
        );
    }
}
fn key_width(key: &str, label: &str) -> f32 {
    let cap = (text_width(key, plain(13., CREAM)) + 12.).max(20.);
    if label.is_empty() {
        cap
    } else {
        cap + 7. + text_width(label, plain(13., CREAM))
    }
}
/// Key cap with an action label; a lighter-weight twin of the host's
/// `keycap` for hint rows drawn every frame. Returns the total width.
fn key_hint(key: &str, label: &str, x: f32, y: f32, ink: Color) -> f32 {
    let ks = plain(13., PARCHMENT);
    let cap = (text_width(key, ks) + 12.).max(20.);
    flat_round(x, y + 2., cap, 20., 4., Color::new(0., 0., 0., 0.4 * ink.a));
    flat_round(x, y, cap, 20., 4., Color::new(0.19, 0.17, 0.21, ink.a));
    draw_rectangle(
        x + 3.,
        y,
        cap - 6.,
        1.,
        Color::new(1., 1., 1., 0.12 * ink.a),
    );
    draw_text_styled(
        key,
        x + cap * 0.5,
        ks.centered_baseline(y + 10.),
        ks.with_align(TextAlign::Center),
    );
    if label.is_empty() {
        return cap;
    }
    let st = plain(13., ink);
    cap + 7. + draw_text_styled(label, x + cap + 7., st.centered_baseline(y + 10.), st).width
}
/// Soft warm light pool behind a figure.
fn spotlight(x: f32, y: f32, r: f32, color: Color) {
    let c = vec2(x, y);
    shapes::fill_circle(c, r, Fill::radial(c, r, color, with_alpha(color, 0.)));
}

fn portrait(x: f32, y: f32, index: usize, hero: bool, t: f32) {
    let robe = if hero {
        Color::new(0.38, 0.26, 0.46, 1.)
    } else {
        [
            Color::new(0.46, 0.22, 0.21, 1.),
            Color::new(0.30, 0.15, 0.33, 1.),
            Color::new(0.40, 0.34, 0.20, 1.),
        ][index % 3]
    };
    draw_ellipse(x, y + 70., 60., 11., 0., Color::new(0., 0.02, 0.03, 0.55));
    let y = y + (t * 0.045).sin() * 1.5;
    // Robe with a shaded fold and gilt hem.
    draw_triangle(
        vec2(x, y - 5.),
        vec2(x - 50., y + 68.),
        vec2(x + 50., y + 68.),
        robe,
    );
    draw_triangle(
        vec2(x, y - 5.),
        vec2(x + 12., y + 68.),
        vec2(x + 50., y + 68.),
        darken(robe, 0.3),
    );
    draw_triangle(
        vec2(x, y - 5.),
        vec2(x - 50., y + 68.),
        vec2(x - 36., y + 68.),
        lighten(robe, 0.12),
    );
    draw_line(x - 49., y + 66.5, x + 49., y + 66.5, 3., GOLD);
    draw_triangle(
        vec2(x - 11., y + 6.),
        vec2(x + 11., y + 6.),
        vec2(x, y + 22.),
        CREAM,
    );
    draw_line(x, y + 14., x, y + 64., 2., GOLD);
    // Head with mask and eyes.
    shapes::fill_circle(
        vec2(x, y - 11.),
        22.,
        Fill::radial(
            vec2(x - 7., y - 19.),
            32.,
            Color::new(1., 0.95, 0.82, 1.),
            Color::new(0.82, 0.70, 0.55, 1.),
        ),
    );
    if !hero && (index == 2 || index == 6) {
        rounded(x - 25., y - 38., 50., 49., 6., GOLD);
        rounded_gradient(
            x - 18.,
            y - 28.,
            36.,
            27.,
            4.,
            Color::new(0.30, 0.16, 0.32, 1.),
            Color::new(0.14, 0.07, 0.16, 1.),
        );
    } else {
        draw_ellipse(x, y - 10., 23., 10., 0., PLUM);
    }
    let eye = if hero { GREEN } else { RED };
    for dx in [-8., 8.] {
        draw_circle(x + dx, y - 11., 3., eye);
        draw_circle(x + dx - 0.8, y - 11.8, 1., CREAM);
    }
    if !hero && index == 8 {
        for dx in [-16., 0., 16.] {
            draw_triangle(
                vec2(x + dx - 8., y - 36.),
                vec2(x + dx, y - 58.),
                vec2(x + dx + 8., y - 36.),
                GOLD,
            );
            draw_circle(x + dx, y - 56., 2.5, RED);
        }
        rounded(x - 24., y - 41., 48., 9., 2., GOLD);
    } else if !hero && index == 3 {
        draw_triangle(
            vec2(x - 35., y - 31.),
            vec2(x + 8., y - 68.),
            vec2(x + 33., y - 31.),
            robe,
        );
        draw_triangle(
            vec2(x + 8., y - 68.),
            vec2(x + 33., y - 31.),
            vec2(x + 6., y - 31.),
            darken(robe, 0.25),
        );
    } else {
        rounded(x - 29., y - 39., 58., 9., 3., darken(robe, 0.15));
        rounded_gradient(
            x - 17.,
            y - 58.,
            34.,
            21.,
            3.,
            lighten(robe, 0.1),
            darken(robe, 0.2),
        );
        draw_line(x - 17., y - 42., x + 17., y - 42., 2.5, GOLD);
    }
    gem(x, y + 36., 7., GOLD);
}

// ---------------------------------------------------------------------------
// Screens.

impl Game {
    fn time(&self) -> f32 {
        (self.animation % 100_000) as f32
    }
    fn battle_view(&self) {
        let t = self.time();
        // Title row: place and turn clock.
        let place = self.content().place.to_uppercase();
        let w = eyebrow(&place, 36., 140., with_alpha(GOLD, 0.9));
        if self.content().elite {
            pill("Elite", 36. + w + 10., 126., RED);
        }
        let turn = format!("Turn {} of 40", self.turn);
        let tw = text_width(&turn, plain(12., CREAM)) + 14.;
        pill(&turn, 928. - tw, 126., SOFT);

        self.hero_panel();
        self.enemy_stage(t);
        self.inspector();

        let playable = self.playable(self.selected);
        button(
            Rect::new(620., 340., 146., 37.),
            "Space",
            "Play",
            true,
            playable,
        );
        button(
            Rect::new(778., 340., 150., 37.),
            "X",
            "End turn",
            false,
            true,
        );

        // Feedback line and pile counts.
        let piles = format!(
            "Draw {}  ·  Discard {}  ·  Exhausted {}",
            self.draw.len(),
            self.discard.len(),
            self.exhausted.len()
        );
        let pw = text_width(&piles, plain(11., MUTED));
        say(&piles, 928. - pw, 393., plain(11., MUTED));
        say_fit(
            &self.notice,
            36.,
            393.,
            870. - pw,
            plain(13., with_alpha(CREAM, 0.9)),
        );

        // The hand, gently fanned, with the selected card raised.
        let n = self.hand.len();
        let mid = (n as f32 - 1.) * 0.5;
        for (index, &card) in self.hand.iter().enumerate() {
            if index == self.selected {
                continue;
            }
            let (x, w) = self.card_bounds(index);
            let dy = (index as f32 - mid).abs() * 2.;
            card_face(
                card,
                x,
                408. + dy,
                w,
                146.,
                Look {
                    selected: false,
                    affordable: self.playable(index),
                },
            );
        }
        if let Some(&card) = self.hand.get(self.selected) {
            let (x, w) = self.card_bounds(self.selected);
            card_face(
                card,
                x,
                400.,
                w,
                146.,
                Look {
                    selected: true,
                    affordable: self.playable(self.selected),
                },
            );
        }
        for index in 0..n.min(5) {
            let (x, w) = self.card_bounds(index);
            let dy = if index == self.selected {
                -8.
            } else {
                (index as f32 - mid).abs() * 2.
            };
            say(
                &(index + 1).to_string(),
                x + w - 14.,
                408. + 146. - 7. + dy,
                plain(10., SEPIA_MUTED),
            );
        }
    }
    fn hero_panel(&self) {
        slab(32., 154., 196., 214., VELVET);
        let x = 50.;
        eyebrow("THE LIBERATOR", x, 178., with_alpha(GOLD, 0.85));
        let hp_frac = f32::from(self.hero.hp) / f32::from(self.hero.max_hp.max(1));
        let hp_color = if hp_frac < 0.3 { RED } else { GREEN };
        let w = say(&self.hero.hp.to_string(), x, 212., style(28., CREAM));
        say(
            &format!("/ {} HP", self.hero.max_hp),
            x + w + 6.,
            212.,
            plain(13., MUTED),
        );
        hp_bar(x, 221., 160., 8., hp_frac, hp_color);
        // Block and energy.
        eyebrow("BLOCK", x, 250., MUTED);
        let shield = if self.hero.block > 0 {
            STEEL
        } else {
            with_alpha(STEEL, 0.4)
        };
        flat_round(x, 258., 16., 11., 3., shield);
        draw_triangle(
            vec2(x, 267.),
            vec2(x + 16., 267.),
            vec2(x + 8., 277.),
            shield,
        );
        say(
            &self.hero.block.to_string(),
            x + 23.,
            278.,
            style(22., CREAM),
        );
        let ex = 128.;
        eyebrow("ENERGY", ex, 250., MUTED);
        say(&self.energy.to_string(), ex, 278., style(22., GOLD));
        let base = if self.relics[4] { 4 } else { 3 };
        let orbs = base.max(self.energy) as usize;
        for i in 0..orbs.min(6) {
            let c = vec2(ex + 24. + i as f32 * 11., 270.);
            if i < usize::from(self.energy) {
                draw_circle(c.x, c.y, 4.5, GOLD);
                draw_circle(c.x - 1., c.y - 1., 2.5, CREAM);
            } else {
                draw_circle(c.x, c.y, 4.5, with_alpha(GOLD, 0.18));
            }
        }
        // Lasting effects.
        let effects = fighter_effects(&self.hero, 0, true);
        if effects.is_empty() {
            say(
                "No lasting effects",
                x,
                307.,
                plain(12., with_alpha(MUTED, 0.7)),
            );
        } else {
            pill_flow(&effects, x, 294., 212.);
        }
        // Relics carried.
        let mut rx = x + 6.;
        for (i, &owned) in self.relics.iter().enumerate() {
            if owned {
                gem(rx, 351., 6., relic_color(i));
                rx += 20.;
            }
        }
        if rx > x + 6. {
            eyebrow("RELICS", rx, 355., with_alpha(MUTED, 0.75));
        }
    }
    fn enemy_stage(&self, t: f32) {
        let cx = 416.;
        say(
            self.content().name,
            cx,
            170.,
            style(19., CREAM).with_align(TextAlign::Center),
        );
        // Intent banner.
        let i = self.intent();
        let mut parts: Vec<String> = Vec::with_capacity(6);
        if i.hits > 0 {
            let dmg = Self::damage_value(
                i.damage,
                self.enemy.strength,
                self.enemy.weak,
                self.hero.vulnerable,
            );
            parts.push(if i.hits > 1 {
                format!("Attack {dmg} × {}", i.hits)
            } else {
                format!("Attack {dmg}")
            });
        }
        if i.block > 0 {
            parts.push(format!("Block {}", i.block));
        }
        if i.strength > 0 {
            parts.push(format!("Strength +{}", i.strength));
        }
        if i.wound {
            parts.push("Wound".into());
        }
        if i.poison > 0 {
            parts.push(format!("Poison {}", i.poison));
        }
        if i.weak > 0 {
            parts.push("Weak".into());
        }
        if i.vulnerable > 0 {
            parts.push("Vulnerable".into());
        }
        let intent = if parts.is_empty() {
            "Plotting".to_string()
        } else {
            parts.join("  ·  ")
        };
        let attack = i.hits > 0;
        let tone = if attack { RED } else { GOLD };
        let is = plain(13., lighten(tone, 0.25));
        let iw = text_width(&intent, is).min(290.);
        let bw = iw + 44.;
        let bx = cx - bw * 0.5;
        rounded(bx, 180., bw, 24., 12., with_alpha(tone, 0.5));
        rounded(
            bx + 1.,
            181.,
            bw - 2.,
            22.,
            11.,
            Color::new(0.10, 0.05, 0.07, 0.92),
        );
        // Intent glyph: a blade for attacks, a sigil otherwise.
        let gx = bx + 16.;
        if attack {
            draw_line(gx - 5., 197., gx + 5., 186., 3., tone);
            draw_line(gx - 7., 192., gx - 1., 198., 2., tone);
        } else {
            diamond(gx, 192., 5., tone);
        }
        say_fit(&intent, bx + 30., is.centered_baseline(192.), 290., is);
        // The captor.
        spotlight(cx, 258., 92., Color::new(0.95, 0.70, 0.35, 0.16));
        portrait(cx, 262., self.floor, false, t);
        // Health.
        let frac = f32::from(self.enemy.hp) / f32::from(self.enemy.max_hp.max(1));
        hp_bar(296., 342., 190., 9., frac, RED);
        say(
            &format!("{} / {}", self.enemy.hp, self.enemy.max_hp),
            494.,
            351.,
            plain(13., CREAM),
        );
        let effects = fighter_effects(&self.enemy, self.content().armor, false);
        if !effects.is_empty() {
            let w = pills_width(&effects);
            pill_flow(&effects, cx - w * 0.5, 358., 600.);
        }
    }
    fn inspector(&self) {
        slab(620., 154., 308., 176., Color::new(0.09, 0.11, 0.13, 1.));
        let x = 638.;
        let Some(&chosen) = self.hand.get(self.selected) else {
            eyebrow("EMPTY HAND", x, 180., MUTED);
            para(
                "No cards remain. End your turn to draw a new hand.",
                x,
                210.,
                272.,
                plain(16., CREAM),
                21.,
            );
            return;
        };
        let (_, _, ink) = suit_colors(suit(chosen.kind));
        eyebrow(
            &type_line(chosen).to_uppercase(),
            x,
            178.,
            with_alpha(ink, 0.9),
        );
        say_fit(
            &card_title(chosen),
            x,
            204.,
            272.,
            style(21., if chosen.upgraded { GOLD } else { CREAM }),
        );
        let cost = if chosen.kind == Kind::Wound {
            "Cannot be played".to_string()
        } else if chosen.cost() == 0 {
            "Free to play".to_string()
        } else {
            format!("Costs {} energy", chosen.cost())
        };
        let ready = self.playable(self.selected);
        say(&cost, x, 224., plain(12., if ready { MUTED } else { RED }));
        divider(x, 234., 272., with_alpha(GOLD, 0.35));
        para(&chosen.description(), x, 258., 272., plain(16., CREAM), 21.);
        if !ready && chosen.kind != Kind::Wound {
            say("Not enough energy or HP", x, 318., plain(12., RED));
        } else if let Some(note) = keyword_note(&chosen.description()) {
            say_fit(note, x, 318., 272., plain(12., with_alpha(SOFT, 0.75)));
        }
    }
    fn choice_tile(&self, id: usize, title: &str, detail: &str, available: bool, icon: Icon) {
        let priced = self.phase == Phase::Shop && id < 3;
        let x = 480.;
        let y = 190. + id as f32 * 77.;
        let w = 444.;
        let h = 69.;
        let selected = self.selected == id;
        tile(x, y, w, h, selected);
        let a = if available { 1. } else { 0.45 };
        // Icon well.
        flat_round(x + 10., y + 9.5, 50., 50., 8., Color::new(0., 0., 0., 0.28));
        match icon {
            Icon::Card(card) => mini_card(card, x + 18., y + 12., 34., 45.),
            Icon::Relic(i) => gem(x + 35., y + 35., 13., relic_color(i)),
            Icon::Rest => {
                spotlight(x + 35., y + 38., 20., Color::new(1., 0.6, 0.2, 0.35));
                draw_triangle(
                    vec2(x + 25., y + 46.),
                    vec2(x + 35., y + 18.),
                    vec2(x + 45., y + 46.),
                    Color::new(0.96, 0.52, 0.22, 1.),
                );
                draw_triangle(
                    vec2(x + 30., y + 46.),
                    vec2(x + 36., y + 28.),
                    vec2(x + 41., y + 46.),
                    Color::new(1., 0.85, 0.45, 1.),
                );
                draw_line(
                    x + 22.,
                    y + 49.,
                    x + 48.,
                    y + 45.,
                    3.,
                    Color::new(0.45, 0.28, 0.16, 1.),
                );
            }
            Icon::Forge => {
                draw_line(
                    x + 24.,
                    y + 48.,
                    x + 40.,
                    y + 28.,
                    4.,
                    Color::new(0.55, 0.38, 0.22, 1.),
                );
                rounded(x + 30., y + 16., 20., 13., 2., STEEL);
                rounded(x + 30., y + 16., 20., 4., 2., lighten(STEEL, 0.3));
            }
            Icon::Coin => {
                let c = vec2(x + 35., y + 34.5);
                shapes::fill_circle(
                    c,
                    13.,
                    Fill::radial(c - vec2(4., 4.), 18., CREAM, darken(GOLD, 0.2)),
                );
                shapes::stroke_circle(c, 9., 1.5, darken(GOLD, 0.35));
            }
            Icon::Tonic => {
                let c = vec2(x + 35., y + 40.);
                shapes::fill_circle(
                    c,
                    11.,
                    Fill::radial(c - vec2(3., 3.), 14., lighten(RED, 0.3), darken(RED, 0.3)),
                );
                rounded(x + 31., y + 17., 8., 14., 2., CREAM);
            }
            Icon::Trim => {
                draw_line(x + 24., y + 22., x + 46., y + 48., 3., STEEL);
                draw_line(x + 46., y + 22., x + 24., y + 48., 3., STEEL);
                shapes::stroke_circle(vec2(x + 26., y + 46.), 4., 2., GOLD);
                shapes::stroke_circle(vec2(x + 44., y + 46.), 4., 2., GOLD);
            }
            Icon::Depart => {
                draw_line(x + 22., y + 34.5, x + 44., y + 34.5, 4., CREAM);
                draw_triangle(
                    vec2(x + 42., y + 25.),
                    vec2(x + 52., y + 34.5),
                    vec2(x + 42., y + 44.),
                    CREAM,
                );
            }
        }
        let tc = if selected { lighten(GOLD, 0.15) } else { CREAM };
        say_fit(
            title,
            x + 74.,
            y + 30.,
            w - 150.,
            plain(17., with_alpha(tc, a)),
        );
        say_fit(
            detail,
            x + 74.,
            y + 51.,
            if priced { w - 196. } else { w - 120. },
            plain(12.5, with_alpha(SOFT, 0.9 * a)),
        );
        key_hint(&(id + 1).to_string(), "", x + w - 34., y + 24.5, CREAM);
    }
    fn price(&self, id: usize, coins: u16, label: &str) {
        let y = 190. + id as f32 * 77. + 25.;
        let color = if self.coins >= coins { GOLD } else { RED };
        let w = text_width(label, plain(12., color)) + 14.;
        pill(label, 924. - 44. - w, y, color);
    }
    fn menu_view(&self) {
        let (kicker, title) = match self.phase {
            Phase::Oath => ("YOUR HEIRLOOM", "Choose a relic"),
            Phase::Reward => ("VICTORY SPOILS", "Card reward"),
            Phase::Relic => ("VAULT SPOILS", "Relic reward"),
            Phase::Camp => ("BETWEEN BATTLES", "Camp"),
            Phase::Shop => ("THE NIGHT MARKET", "Shop"),
            Phase::Won => ("THE PALACE IS FREE", "Victory"),
            Phase::Lost => ("THE LEDGER CLOSES", "Defeat"),
            _ => ("GILDED DECK", "Gilded Deck"),
        };
        // Left: the Liberator's story card.
        slab(32., 128., 420., 404., VELVET);
        eyebrow(kicker, 54., 158., with_alpha(GOLD, 0.9));
        say(title, 54., 194., style(27., CREAM));
        spotlight(242., 284., 96., Color::new(0.95, 0.70, 0.35, 0.14));
        portrait(242., 284., 0, true, self.time());
        let story = match self.phase {
            Phase::Oath => {
                "Choose the heirloom you carried into the palace. It lasts the whole run."
            }
            Phase::Reward => self.content().story,
            Phase::Relic => "The vault yields one permanent relic.",
            Phase::Camp => {
                if self.camp_used {
                    "Camp action used. Depart when ready."
                } else {
                    "One action per camp: rest, gild a card or visit the shop."
                }
            }
            Phase::Shop => "One purchase per camp. X returns to camp.",
            Phase::Won => "All nine battles won.",
            _ => "Run ended.",
        };
        para(story, 54., 392., 376., plain(16., CREAM), 21.);
        divider(54., 448., 376., with_alpha(GOLD, 0.3));
        eyebrow("HEALTH", 54., 472., MUTED);
        let frac = f32::from(self.hero.hp) / f32::from(self.hero.max_hp.max(1));
        let w = say(&self.hero.hp.to_string(), 54., 503., style(24., CREAM));
        say(
            &format!("/ {}", self.hero.max_hp),
            60. + w,
            503.,
            plain(13., MUTED),
        );
        hp_bar(
            54.,
            511.,
            170.,
            6.,
            frac,
            if frac < 0.3 { RED } else { GREEN },
        );
        eyebrow("PURSE", 262., 472., MUTED);
        let c = vec2(270., 494.);
        shapes::fill_circle(
            c,
            8.,
            Fill::radial(c - vec2(2., 2.), 11., CREAM, darken(GOLD, 0.2)),
        );
        say(&self.coins.to_string(), 284., 503., style(24., GOLD));
        eyebrow("DECK", 360., 472., MUTED);
        say(&self.deck.len().to_string(), 360., 503., style(24., CREAM));

        // Right: choices.
        let prompt = match self.phase {
            Phase::Reward => "CHOOSE ONE CARD, OR SKIP",
            Phase::Camp if self.camp_used => "READY TO DEPART",
            Phase::Won | Phase::Lost => "BATTLE RECORD",
            _ => "CHOOSE ONE",
        };
        eyebrow(prompt, 482., 176., MUTED);
        match self.phase {
            Phase::Oath | Phase::Relic => {
                for i in 0..3 {
                    let id = i + if self.phase == Phase::Relic { 3 } else { 0 };
                    self.choice_tile(
                        i,
                        RELIC_NAMES[id],
                        RELIC_TEXT[id],
                        !self.relics[id],
                        Icon::Relic(id),
                    );
                }
            }
            Phase::Reward => {
                for (i, kind) in REWARDS[self.floor].iter().enumerate() {
                    let card = Card::new(*kind);
                    self.choice_tile(i, card.name(), &card.description(), true, Icon::Card(card));
                }
            }
            Phase::Camp => {
                let open = !self.camp_used;
                self.choice_tile(
                    0,
                    "Rest",
                    "Heal 18 HP. Uses your camp action.",
                    open,
                    Icon::Rest,
                );
                self.choice_tile(1, "Forge", "Gild one card permanently.", open, Icon::Forge);
                self.choice_tile(
                    2,
                    "Visit shop",
                    "Cards, healing or trimming your deck.",
                    open,
                    Icon::Coin,
                );
                self.choice_tile(
                    3,
                    "Depart",
                    "Face the next captor with your deck.",
                    true,
                    Icon::Depart,
                );
            }
            Phase::Shop => {
                let card = Card::new(self.shop_card());
                self.choice_tile(
                    0,
                    card.name(),
                    "Add one permanent card to the deck.",
                    self.coins >= 35,
                    Icon::Card(card),
                );
                self.choice_tile(
                    1,
                    "Tonic",
                    "Heal 28 HP, up to your maximum.",
                    self.coins >= 25,
                    Icon::Tonic,
                );
                self.choice_tile(
                    2,
                    "Trim",
                    "Remove one ungilded Gilt Strike.",
                    self.coins >= 20,
                    Icon::Trim,
                );
                self.choice_tile(
                    3,
                    "Back to camp",
                    "Leave without spending your action.",
                    true,
                    Icon::Depart,
                );
                self.price(0, 35, "35 coins");
                self.price(1, 25, "25 coins");
                self.price(2, 20, "20 coins");
            }
            Phase::Won | Phase::Lost => {
                for (i, encounter) in ENCOUNTERS.iter().enumerate() {
                    let y = 188. + i as f32 * 36.;
                    let turns = self.won_turns[i];
                    let won = turns > 0;
                    flat_round(
                        480.,
                        y,
                        444.,
                        30.,
                        8.,
                        Color::new(1., 1., 1., if won { 0.06 } else { 0.03 }),
                    );
                    gem(
                        498.,
                        y + 15.,
                        6.,
                        if won { GOLD } else { with_alpha(MUTED, 0.5) },
                    );
                    say(
                        encounter.name,
                        516.,
                        y + 20.,
                        plain(14., if won { CREAM } else { MUTED }),
                    );
                    let note = if won {
                        format!("Won in {turns} turns")
                    } else if i == self.floor && self.phase == Phase::Lost {
                        "Fell here".to_string()
                    } else {
                        "Not reached".to_string()
                    };
                    let ns = plain(12., if won { GOLD } else { MUTED });
                    say(&note, 908. - text_width(&note, ns), y + 20., ns);
                }
            }
            _ => {}
        }
        if !matches!(self.phase, Phase::Won | Phase::Lost) {
            let count = if matches!(self.phase, Phase::Camp | Phase::Shop) {
                4
            } else {
                3
            };
            let y = 190. + count as f32 * 77. + 8.;
            let mut kx = 482.;
            kx += key_hint(
                if count == 4 { "1-4" } else { "1-3" },
                "Choose",
                kx,
                y,
                SOFT,
            ) + 18.;
            kx += key_hint("↑↓", "Select", kx, y, SOFT) + 18.;
            match self.phase {
                Phase::Reward => kx += key_hint("X", "Skip", kx, y, SOFT) + 18.,
                Phase::Shop => kx += key_hint("X", "Back", kx, y, SOFT) + 18.,
                _ => {}
            }
            key_hint("C", "Deck", kx, y, SOFT);
        }
        say_fit(
            &self.notice,
            36.,
            556.,
            888.,
            plain(13., with_alpha(CREAM, 0.85)),
        );
    }
    fn forge_view(&self) {
        let w = say("Gild one card", 42., 150., style(21., CREAM));
        eyebrow("THE FORGE", 54. + w, 149., with_alpha(GOLD, 0.8));
        let page = self.selected / 8;
        let pages = self.deck.len().div_ceil(8).max(1);
        if pages > 1 {
            let label = format!("Page {} of {}", page + 1, pages);
            let w = text_width(&label, plain(12., SOFT)) + 14.;
            pill(&label, 600. - w, 137., SOFT);
        }
        for (i, card) in self.deck.iter().enumerate().skip(page * 8).take(8) {
            let y = 162. + (i % 8) as f32 * 40.;
            let selected = i == self.selected;
            tile(40., y, 560., 35., selected);
            mini_card(*card, 50., y + 5., 19., 25.);
            let gc = vec2(84., y + 17.5);
            draw_circle(
                gc.x,
                gc.y,
                9.,
                if card.kind == Kind::Wound {
                    STEEL
                } else {
                    GOLD
                },
            );
            let cs = plain(11., SEPIA);
            draw_text_styled(
                &card.cost().to_string(),
                gc.x,
                cs.centered_baseline(gc.y),
                cs.with_align(TextAlign::Center),
            );
            say(
                &card_title(*card),
                102.,
                y + 23.,
                plain(16., if card.upgraded { GOLD } else { CREAM }),
            );
            let tl = type_line(*card);
            let (_, _, ink) = suit_colors(suit(card.kind));
            let w = text_width(tl, plain(12., ink));
            say(tl, 586. - w, y + 22., plain(12., with_alpha(ink, 0.8)));
        }
        slab(620., 128., 310., 336., VELVET);
        eyebrow("AFTER GILDING", 640., 154., with_alpha(GOLD, 0.9));
        if let Some(chosen) = self.deck.get(self.selected) {
            let mut upgraded = *chosen;
            upgraded.upgraded = true;
            spotlight(775., 300., 120., Color::new(1., 0.78, 0.4, 0.12));
            card_face(
                upgraded,
                700.,
                170.,
                150.,
                204.,
                Look {
                    selected: false,
                    affordable: true,
                },
            );
            let note = if chosen.upgraded {
                "Already gilded. Forging it again changes nothing."
            } else {
                "The improved effect lasts for the whole run."
            };
            para(note, 640., 404., 270., plain(13., SOFT), 18.);
        }
        button(
            Rect::new(630., 472., 290., 50.),
            "Space",
            "Gild card",
            true,
            true,
        );
        let mut kx = 42.;
        kx += key_hint("↑↓", "Select", kx, 536., SOFT) + 18.;
        kx += key_hint("X", "Back to camp", kx, 536., SOFT) + 18.;
        key_hint("C", "Deck", kx, 536., SOFT);
    }
    fn deck_view(&self) {
        rounded(
            16.,
            112.,
            928.,
            456.,
            0.,
            Color::new(0.02, 0.03, 0.04, 0.82),
        );
        slab(28., 120., 904., 440., VELVET);
        let mut hx = 48. + say("Your deck", 48., 160., style(21., CREAM)) + 12.;
        hx += pill(&format!("{} cards", self.deck.len()), hx, 145., SOFT) + 6.;
        let gilded = self.deck.iter().filter(|c| c.upgraded).count();
        if gilded > 0 {
            pill(&format!("{gilded} gilded"), hx, 145., GOLD);
        }
        for (i, card) in self
            .deck
            .iter()
            .enumerate()
            .skip(self.deck_page * 8)
            .take(8)
        {
            let at = i % 8;
            let x = 48. + (at % 2) as f32 * 438.;
            let y = 182. + (at / 2) as f32 * 82.;
            tile(x, y, 422., 74., false);
            mini_card(*card, x + 10., y + 10., 40., 54.);
            let nw = say(
                &card_title(*card),
                x + 62.,
                y + 26.,
                plain(16., if card.upgraded { GOLD } else { CREAM }),
            );
            let cost = if card.kind == Kind::Wound {
                "Unplayable".to_string()
            } else {
                format!("{} energy", card.cost())
            };
            say(&cost, x + 72. + nw, y + 25., plain(12., MUTED));
            let tl = type_line(*card);
            let (_, _, ink) = suit_colors(suit(card.kind));
            let tw = text_width(tl, plain(11., ink));
            say(tl, x + 408. - tw, y + 25., plain(11., with_alpha(ink, 0.8)));
            para(
                &card.description(),
                x + 62.,
                y + 46.,
                346.,
                plain(12., SOFT),
                15.,
            );
        }
        let pages = self.deck.len().div_ceil(8).max(1);
        say(
            &format!("Page {} of {}", self.deck_page + 1, pages),
            48.,
            546.,
            plain(13., MUTED),
        );
        let mut kx = 160.;
        kx += key_hint("←→", "Turn page", kx, 532., SOFT) + 18.;
        key_hint("C", "Close", kx, 532., SOFT);
    }
}

/// A one-line reminder for the first keyword a card's rules mention.
fn keyword_note(rules: &str) -> Option<&'static str> {
    [
        ("Exhaust", "Exhaust: removed until the next battle."),
        ("poison", "Poison ticks each turn and ignores block."),
        ("vulnerable", "Vulnerable: takes 50% more attack damage."),
        ("weak", "Weak: deals 25% less attack damage."),
        ("thorns", "Thorns hurt attackers that cost you HP."),
        ("strength", "Strength adds damage to every hit."),
        ("block", "Block absorbs damage until your next turn."),
    ]
    .into_iter()
    .find(|(word, _)| rules.contains(word))
    .map(|(_, note)| note)
}
#[derive(Clone, Copy)]
enum Icon {
    Card(Card),
    Relic(usize),
    Rest,
    Forge,
    Coin,
    Tonic,
    Trim,
    Depart,
}
fn relic_color(id: usize) -> Color {
    [
        Color::new(0.95, 0.75, 0.40, 1.),
        Color::new(0.50, 0.70, 0.98, 1.),
        Color::new(0.55, 0.90, 0.70, 1.),
        Color::new(0.95, 0.42, 0.42, 1.),
        Color::new(1.0, 0.62, 0.25, 1.),
        Color::new(0.76, 0.58, 0.96, 1.),
    ][id % 6]
}
/// Non-zero effects on a fighter as labelled pills.
fn fighter_effects(f: &crate::model::Fighter, armor: i16, hero: bool) -> Vec<(String, Color)> {
    let mut out = Vec::with_capacity(7);
    if armor > 0 {
        out.push((format!("Armor {armor}"), STEEL));
    }
    if !hero && f.block > 0 {
        out.push((format!("Block {}", f.block), STEEL));
    }
    if f.strength > 0 {
        out.push((format!("Str {}", f.strength), GOLD));
    }
    if f.thorns > 0 {
        out.push((format!("Thorns {}", f.thorns), GREEN));
    }
    if f.poison > 0 {
        out.push((format!("Poison {}", f.poison), VIOLET));
    }
    if f.weak > 0 {
        out.push((format!("Weak {}", f.weak), Color::new(0.6, 0.75, 0.95, 1.)));
    }
    if f.vulnerable > 0 {
        out.push((format!("Vuln {}", f.vulnerable), RED));
    }
    out
}

impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!("Captor {} of 9", self.encounter()),
            objective: if self.phase == Phase::Battle {
                format!("Defeat {}.", self.content().name)
            } else {
                "Win all nine battles.".into()
            },
            status: format!(
                "{} cards, {} relics, {} coins",
                self.deck.len(),
                self.relics.iter().filter(|&&v| v).count(),
                self.coins
            ),
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        self.outcome()
    }
    fn summary(&self) -> String {
        self.summary()
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
    fn draw(&self) {
        self.render();
    }
}
impl Game {
    fn render(&self) {
        // Velvet card table with a warm pool of lamplight.
        let table = Rect::new(16., 112., 928., 456.);
        shapes::fill_rounded_rect(
            table,
            12.,
            Fill::vertical(
                table,
                Color::new(0.08, 0.21, 0.19, 1.),
                Color::new(0.03, 0.10, 0.10, 1.),
            ),
        );
        match self.phase {
            Phase::Battle => self.battle_view(),
            Phase::Forge => self.forge_view(),
            _ => self.menu_view(),
        }
        if self.deck_open {
            self.deck_view();
        }
    }
}
