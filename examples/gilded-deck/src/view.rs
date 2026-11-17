//! Read-only velvet table, parchment cards and court portraits.
//!
//! The table explains itself: cards carry icon rows with live values, the
//! fighters show meters and badges, the enemy's next move floats overhead,
//! the focused card previews its outcome on the health bars, and accepted
//! events burst where they happen. Words are kept for names.
use crate::{
    content::{Card, ENCOUNTERS, Kind, RELIC_NAMES, REWARDS},
    layout::{self, CARD_H, HAND_TOP, RAISE},
    model::{Event, Fighter, Game, Harm, Phase, Refusal, Side, Spot, Status},
};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;
use ferravex_play::tween::Ease;

const GOLD: Color = Color::new(0.91, 0.73, 0.38, 1.);
const CREAM: Color = Color::new(0.96, 0.90, 0.73, 1.);
const PLUM: Color = Color::new(0.20, 0.10, 0.22, 1.);
const VELVET: Color = Color::new(0.13, 0.08, 0.15, 1.);
const SOFT: Color = Color::new(0.80, 0.76, 0.66, 1.);
const PARCHMENT: Color = Color::new(0.97, 0.92, 0.80, 1.);
const PARCHMENT_LOW: Color = Color::new(0.87, 0.79, 0.63, 1.);
const SEPIA: Color = Color::new(0.24, 0.16, 0.10, 1.);
// One color per idea, everywhere: cards, badges, intents and popups.
const HURT: Color = Color::new(0.98, 0.42, 0.40, 1.);
const SHIELD: Color = Color::new(0.52, 0.76, 1.0, 1.);
const IRON: Color = Color::new(0.74, 0.77, 0.82, 1.);
const LIFE: Color = Color::new(0.96, 0.38, 0.48, 1.);
const MEND: Color = Color::new(0.46, 0.90, 0.60, 1.);
const ENERGY: Color = Color::new(1.0, 0.83, 0.34, 1.);
const POISON: Color = Color::new(0.78, 0.52, 1.0, 1.);
const WEAK: Color = Color::new(0.64, 0.70, 0.86, 1.);
const VULN: Color = Color::new(1.0, 0.46, 0.66, 1.);
const MIGHT: Color = Color::new(1.0, 0.63, 0.30, 1.);
const THORN: Color = Color::new(0.60, 0.86, 0.38, 1.);
const EMBER: Color = Color::new(1.0, 0.50, 0.20, 1.);
const HERO_HP: Color = Color::new(0.42, 0.84, 0.58, 1.);
const FOE_HP: Color = Color::new(0.88, 0.30, 0.32, 1.);
const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];

// ---------------------------------------------------------------------------
// Text and numbers.

fn plain(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
/// Draw `value` centered on `cx`, shrunk (never enlarged) to `max` units.
fn centered(value: &str, cx: f32, cy: f32, max: f32, s: TextStyle) -> f32 {
    let w = text_width(value, s);
    let s = if w > max && w > 0. {
        s.with_size(s.size * max / w)
    } else {
        s
    };
    draw_text_styled(
        value,
        cx,
        s.centered_baseline(cy),
        s.with_align(TextAlign::Center),
    )
    .width
}
/// A short label built on the stack, so per-frame numbers never allocate.
struct Label {
    bytes: [u8; 24],
    len: usize,
}
impl Label {
    fn new() -> Self {
        Self {
            bytes: [0; 24],
            len: 0,
        }
    }
    fn push(mut self, s: &str) -> Self {
        for &b in s.as_bytes() {
            if self.len < self.bytes.len() {
                self.bytes[self.len] = b;
                self.len += 1;
            }
        }
        self
    }
    fn num(self, n: i32) -> Self {
        let mut digits = [0u8; 11];
        let mut k = digits.len();
        let mut v = n.unsigned_abs();
        loop {
            k -= 1;
            digits[k] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        let label = if n < 0 { self.push("-") } else { self };
        label.push(std::str::from_utf8(&digits[k..]).unwrap_or(""))
    }
    /// `+3` for gains, `-3` for losses.
    fn signed(self, n: i32) -> Self {
        if n > 0 {
            self.push("+").num(n)
        } else {
            self.num(n)
        }
    }
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).unwrap_or("")
    }
}
fn number(n: impl Into<i32>) -> Label {
    Label::new().num(n.into())
}
fn signed(n: impl Into<i32>) -> Label {
    Label::new().signed(n.into())
}

// ---------------------------------------------------------------------------
// Shapes.

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
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::new(
        lerp(a.x, b.x, t),
        lerp(a.y, b.y, t),
        lerp(a.w, b.w, t),
        lerp(a.h, b.h, t),
    )
}
/// Soft warm light pool behind a figure.
fn spotlight(x: f32, y: f32, r: f32, color: Color) {
    let c = vec2(x, y);
    shapes::fill_circle(c, r, Fill::radial(c, r, color, with_alpha(color, 0.)));
}
/// A stroke through a symbol: "removed".
fn struck(at: Vec2, half: f32, color: Color) {
    shapes::fill_capsule(
        at + vec2(-half, half * 0.6),
        at + vec2(half, -half * 0.6),
        1.4,
        color,
    );
}
/// Ticks since `stamp`, or a large number when it never happened.
fn age(now: u32, stamp: Option<u32>) -> f32 {
    stamp.map_or(f32::MAX, |t| now.wrapping_sub(t) as f32)
}
/// 0 → 1 → 0 over `0..1`.
fn bump(u: f32) -> f32 {
    if (0. ..1.).contains(&u) {
        (u * std::f32::consts::PI).sin()
    } else {
        0.
    }
}

// ---------------------------------------------------------------------------
// The icon language: one idea, one icon and one color everywhere.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    Damage,
    Block,
    Armor,
    Poison,
    Weak,
    Vulnerable,
    Strength,
    Thorns,
    Heal,
    Pay,
    Energy,
    Draw,
    Cleanse,
    Spend,
    Locked,
    Kindle,
    Wound,
}
impl Mark {
    fn of(status: Status) -> Self {
        match status {
            Status::Strength => Mark::Strength,
            Status::Thorns => Mark::Thorns,
            Status::Poison => Mark::Poison,
            Status::Weak => Mark::Weak,
            Status::Vulnerable => Mark::Vulnerable,
        }
    }
    fn icon(self) -> Icon {
        match self {
            Mark::Damage => Icon::Sword,
            Mark::Block | Mark::Armor | Mark::Spend => Icon::Shield,
            Mark::Poison | Mark::Cleanse => Icon::Drop,
            Mark::Weak => Icon::ArrowDown,
            Mark::Vulnerable => Icon::Target,
            Mark::Strength => Icon::ArrowUp,
            Mark::Thorns => Icon::Leaf,
            Mark::Heal | Mark::Pay => Icon::Heart,
            Mark::Energy => Icon::Bolt,
            Mark::Draw => Icon::Book,
            Mark::Locked => Icon::Lock,
            Mark::Kindle => Icon::Flame,
            Mark::Wound => Icon::Cross,
        }
    }
    fn color(self) -> Color {
        match self {
            Mark::Damage => HURT,
            Mark::Block | Mark::Spend => SHIELD,
            Mark::Armor => IRON,
            Mark::Poison => POISON,
            Mark::Weak => WEAK,
            Mark::Vulnerable => VULN,
            Mark::Strength => MIGHT,
            Mark::Thorns => THORN,
            Mark::Heal => MEND,
            Mark::Pay => LIFE,
            Mark::Energy => ENERGY,
            Mark::Draw | Mark::Cleanse => CREAM,
            Mark::Locked | Mark::Wound => IRON,
            Mark::Kindle => EMBER,
        }
    }
    /// One-word hover name.
    fn name(self) -> &'static str {
        match self {
            Mark::Damage => "Damage",
            Mark::Block | Mark::Spend => "Block",
            Mark::Armor => "Armor",
            Mark::Poison | Mark::Cleanse => "Poison",
            Mark::Weak => "Weak",
            Mark::Vulnerable => "Vulnerable",
            Mark::Strength => "Strength",
            Mark::Thorns => "Thorns",
            Mark::Heal | Mark::Pay => "Health",
            Mark::Energy => "Energy",
            Mark::Draw => "Draw",
            Mark::Locked => "Unplayable",
            Mark::Kindle => "Exhausted",
            Mark::Wound => "Wound",
        }
    }
    fn width(self, size: f32) -> f32 {
        if self == Mark::Cleanse {
            size * 1.55
        } else {
            size
        }
    }
}
fn draw_mark(mark: Mark, at: Vec2, size: f32, color: Color) {
    let shade = Color::new(0., 0., 0., 0.35 * color.a);
    match mark {
        Mark::Draw => cards_glyph(at, size, color),
        Mark::Cleanse => {
            // The afflictions it washes away, struck through.
            let s = size * 0.56;
            for (k, m) in [Mark::Poison, Mark::Weak, Mark::Vulnerable]
                .into_iter()
                .enumerate()
            {
                icons::draw(
                    m.icon(),
                    at + vec2((k as f32 - 1.) * s * 0.92, 0.),
                    s,
                    with_alpha(m.color(), color.a),
                );
            }
            struck(at, size * 0.72, with_alpha(CREAM, color.a));
        }
        Mark::Spend => {
            icons::draw_styled(
                Icon::Shield,
                at,
                size,
                IconStyle::new(with_alpha(color, 0.55 * color.a)).shadow(shade),
            );
            struck(at, size * 0.45, with_alpha(HURT, color.a));
        }
        _ => icons::draw_styled(mark.icon(), at, size, IconStyle::new(color).shadow(shade)),
    }
}
/// The game's own mark for cards: two stacked card backs.
fn cards_glyph(at: Vec2, size: f32, color: Color) {
    let (w, h) = (size * 0.54, size * 0.74);
    let back = Rect::new(
        at.x - w * 0.5 + size * 0.15,
        at.y - h * 0.5 - size * 0.09,
        w,
        h,
    );
    shapes::fill_rounded_rect(back, size * 0.09, darken(color, 0.4));
    let front = Rect::new(
        at.x - w * 0.5 - size * 0.09,
        at.y - h * 0.5 + size * 0.07,
        w,
        h,
    );
    shapes::fill_rounded_rect(front, size * 0.09, color);
    shapes::fill_circle(
        front.center(),
        size * 0.1,
        with_alpha(darken(color, 0.45), color.a),
    );
}
/// One effect on a card or relic: an icon and its number.
#[derive(Clone, Copy, Debug)]
struct Line {
    mark: Mark,
    value: i16,
    /// Show `+` before positive values.
    plus: bool,
    /// Hits per play (`x3`).
    times: u8,
    /// Live value against the printed one: -1 worse, 0 same, 1 better.
    trend: i8,
    /// A symbol only (lock, cleanse, spend).
    bare: bool,
    /// A small icon in front: "after victory", "from block".
    lead: Option<Icon>,
}
impl Line {
    const fn new(mark: Mark, value: i16) -> Self {
        Self {
            mark,
            value,
            plus: false,
            times: 0,
            trend: 0,
            bare: false,
            lead: None,
        }
    }
    const fn plus(mark: Mark, value: i16) -> Self {
        let mut line = Self::new(mark, value);
        line.plus = true;
        line
    }
    const fn bare(mark: Mark) -> Self {
        let mut line = Self::new(mark, 0);
        line.bare = true;
        line
    }
}
/// Draw `line` centered on (`cx`, `cy`) with icons and numbers at `size`.
fn draw_line_item(line: &Line, cx: f32, cy: f32, size: f32, alpha: f32) {
    let value_color = match line.trend {
        1 => Color::new(0.62, 1.0, 0.66, alpha),
        -1 => Color::new(1.0, 0.62, 0.52, alpha),
        _ => with_alpha(CREAM, alpha),
    };
    let text = if line.bare {
        Label::new()
    } else if line.plus {
        signed(line.value)
    } else {
        number(line.value)
    };
    let ts = style(size, value_color);
    let small = style(size * 0.6, with_alpha(SOFT, alpha));
    let lead_w = if line.lead.is_some() { size * 0.7 } else { 0. };
    let mark_w = line.mark.width(size);
    let gap = size * 0.16;
    let text_w = if line.bare {
        0.
    } else {
        gap + text_width(text.as_str(), ts)
    };
    let times = if line.times > 1 {
        Label::new().push("x").num(i32::from(line.times))
    } else {
        Label::new()
    };
    let times_w = if line.times > 1 {
        size * 0.08 + text_width(times.as_str(), small)
    } else {
        0.
    };
    let mut x = cx - (lead_w + mark_w + text_w + times_w) * 0.5;
    if let Some(lead) = line.lead {
        icons::draw(
            lead,
            vec2(x + size * 0.3, cy),
            size * 0.6,
            with_alpha(SOFT, alpha),
        );
        x += lead_w;
    }
    draw_mark(
        line.mark,
        vec2(x + mark_w * 0.5, cy),
        size,
        with_alpha(line.mark.color(), alpha),
    );
    x += mark_w;
    if !line.bare {
        x += gap;
        x += draw_text_styled(text.as_str(), x, ts.centered_baseline(cy), ts).width;
    }
    if line.times > 1 {
        draw_text_styled(
            times.as_str(),
            x + size * 0.08,
            ts.centered_baseline(cy),
            small,
        );
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
/// Effect-window gradient and the light ink for a suit.
fn suit_colors(s: Suit) -> (Color, Color, Color) {
    match s {
        Suit::Attack => (
            Color::new(0.50, 0.15, 0.17, 1.),
            Color::new(0.22, 0.05, 0.08, 1.),
            Color::new(1.0, 0.82, 0.48, 1.),
        ),
        Suit::Skill => (
            Color::new(0.12, 0.38, 0.34, 1.),
            Color::new(0.03, 0.16, 0.15, 1.),
            Color::new(0.62, 0.95, 0.80, 1.),
        ),
        Suit::Arcane => (
            Color::new(0.34, 0.20, 0.50, 1.),
            Color::new(0.13, 0.07, 0.24, 1.),
            Color::new(0.86, 0.74, 1.0, 1.),
        ),
        Suit::Curse => (
            Color::new(0.30, 0.25, 0.26, 1.),
            Color::new(0.12, 0.10, 0.11, 1.),
            Color::new(0.95, 0.50, 0.48, 1.),
        ),
    }
}
/// The picture behind a card's numbers.
fn art(kind: Kind) -> Icon {
    match kind {
        Kind::Strike | Kind::Flurry | Kind::Riposte => Icon::Sword,
        Kind::Guard | Kind::Fortify => Icon::Shield,
        Kind::Venom => Icon::Drop,
        Kind::Insight => Icon::Eye,
        Kind::Crush => Icon::Hammer,
        Kind::Siphon | Kind::Purify => Icon::Heart,
        Kind::Spark => Icon::Bolt,
        Kind::Focus => Icon::ArrowUp,
        Kind::Gambit => Icon::Coin,
        Kind::Ward => Icon::Wave,
        Kind::Reversal => Icon::Undo,
        Kind::Finale => Icon::Sun,
        Kind::Wound => Icon::Cross,
    }
}
/// Cards whose effect lands on the enemy (the rest steady the Liberator).
fn aims_at_enemy(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Strike
            | Kind::Riposte
            | Kind::Venom
            | Kind::Crush
            | Kind::Siphon
            | Kind::Flurry
            | Kind::Gambit
            | Kind::Reversal
            | Kind::Finale
    )
}
/// What a card does, as up to three icon lines. In battle (`live`) damage
/// shows what would actually land after strength, weak, vulnerable and armor.
fn lines(card: Card, live: Option<&Game>) -> [Option<Line>; 3] {
    let a = |normal, gilded| card.amount(normal, gilded);
    let hit = |base: i16| {
        let mut line = Line::new(Mark::Damage, base);
        if let Some(game) = live {
            line.value = game.hit_value(base);
            line.trend = (line.value - base).signum() as i8;
        }
        line
    };
    let one = |line: Line| [Some(line), None, None];
    let two = |first: Line, second: Line| [Some(first), Some(second), None];
    match card.kind {
        Kind::Strike => one(hit(a(7, 10))),
        Kind::Guard => one(Line::new(Mark::Block, a(6, 9))),
        Kind::Riposte => [
            Some(Line::new(Mark::Block, a(5, 7))),
            Some(hit(a(4, 6))),
            Some(Line::new(Mark::Weak, 1)),
        ],
        Kind::Venom => one(Line::new(Mark::Poison, a(4, 6))),
        Kind::Insight => one(Line::plus(Mark::Draw, a(2, 3))),
        Kind::Crush => two(hit(a(16, 21)), Line::new(Mark::Vulnerable, 2)),
        Kind::Siphon => two(hit(a(5, 8)), Line::plus(Mark::Heal, a(3, 5))),
        Kind::Flurry => one(Line {
            times: 3,
            ..hit(a(3, 4))
        }),
        Kind::Fortify => two(
            Line::new(Mark::Block, a(14, 19)),
            Line::new(Mark::Thorns, a(2, 3)),
        ),
        Kind::Spark => one(Line::plus(Mark::Energy, a(1, 2))),
        Kind::Focus => one(Line::plus(Mark::Strength, a(2, 3))),
        Kind::Gambit => two(Line::new(Mark::Pay, -a(3, 2)), hit(a(9, 13))),
        Kind::Ward => two(Line::new(Mark::Block, a(5, 8)), Line::bare(Mark::Cleanse)),
        Kind::Reversal => {
            let strike = match live {
                Some(game) => hit(game.hero.block + a(4, 8)),
                None => Line {
                    lead: Some(Icon::Shield),
                    ..Line::plus(Mark::Damage, a(4, 8))
                },
            };
            two(strike, Line::bare(Mark::Spend))
        }
        Kind::Purify => one(Line::plus(Mark::Heal, a(4, 7))),
        Kind::Finale => {
            let bonus = live.map_or(0, |g| g.exhausted.len() as i16 * 3);
            two(hit(a(24, 31) + bonus), Line::plus(Mark::Kindle, 3))
        }
        Kind::Wound => one(Line::bare(Mark::Locked)),
    }
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
/// A relic's whole effect as one icon line.
fn relic_line(id: usize) -> Line {
    match id {
        0 => Line::plus(Mark::Strength, 1),
        1 => Line::plus(Mark::Draw, 1),
        2 => Line {
            lead: Some(Icon::Trophy),
            ..Line::plus(Mark::Heal, 6)
        },
        3 => Line::plus(Mark::Heal, 16),
        4 => Line::plus(Mark::Energy, 1),
        _ => Line::new(Mark::Thorns, 3),
    }
}

// ---------------------------------------------------------------------------
// Cards.

/// Presentation state for one card face.
#[derive(Clone, Copy, Default)]
struct Look {
    selected: bool,
    hovered: bool,
    playable: bool,
}
/// A parchment card: leather or gilt rim, cost gem, name, and an effect
/// window that shows what the card does as icons and numbers.
fn card_face(card: Card, r: Rect, look: Look, live: Option<&Game>) {
    let Rect { x, y, w, h } = r;
    let radius = (w * 0.075).clamp(6., 10.);
    if look.selected {
        glow(
            x + w * 0.5,
            y + h * 0.45,
            w * 0.85,
            with_alpha(GOLD, 0.2 + 0.1 * pulse(clock(), 1.1)),
        );
        flat_round(
            x - 2.,
            y + 10.,
            w + 4.,
            h,
            radius + 2.,
            Color::new(0., 0., 0., 0.36),
        );
    } else {
        let a = if look.hovered { 0.45 } else { 0.32 };
        flat_round(
            x + 2.,
            y + 5.,
            w - 4.,
            h - 2.,
            radius,
            Color::new(0., 0., 0., a),
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
    } else if look.hovered {
        (
            Color::new(0.64, 0.48, 0.32, 1.),
            Color::new(0.34, 0.22, 0.13, 1.),
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
    // Cost gem, or a lock on an unplayable curse.
    let gr = (w * 0.1).clamp(9., 14.);
    let gc = vec2(x + gr + 4., y + gr + 4.);
    if wound {
        icon(Icon::Lock, gc.x, gc.y, gr * 1.7, SEPIA);
    } else {
        let (inner, outer) = if look.playable {
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
        draw_circle(gc.x, gc.y, gr + 1.5, SEPIA);
        draw_circle(gc.x, gc.y, gr, outer);
        draw_circle(gc.x - gr * 0.12, gc.y - gr * 0.12, gr * 0.78, inner);
        let cs = plain(gr * 1.3, SEPIA);
        draw_text_styled(
            DIGITS[usize::from(card.cost().min(9))],
            gc.x,
            cs.centered_baseline(gc.y),
            cs.with_align(TextAlign::Center),
        );
    }
    // Name; a gilded card ends in a gold plus.
    let size = (w * 0.115).clamp(10., 16.);
    let left = x + gr * 2. + 7.;
    let room = x + w - 7. - left;
    let st = plain(size, if wound { CREAM } else { SEPIA });
    let plus_w = if card.upgraded {
        text_width("+", st)
    } else {
        0.
    };
    let full = text_width(card.name(), st) + plus_w;
    let st = if full > room {
        st.with_size(size * room / full)
    } else {
        st
    };
    let scale = st.size / size;
    let start = left + (room - full * scale) * 0.5;
    let baseline = st.centered_baseline(gc.y);
    let drawn = draw_text_styled(card.name(), start, baseline, st).width;
    if card.upgraded {
        draw_text_styled(
            "+",
            start + drawn,
            baseline,
            st.with_color(darken(GOLD, 0.3)),
        );
    }
    // The effect window: the card's art, faint, behind its icon lines.
    let (wx, wy) = (x + 6., y + gr * 2. + 9.);
    let (ww, wh) = (w - 12., y + h - 7. - wy);
    let (top, bottom, ink) = suit_colors(suit(card.kind));
    rounded_gradient(wx, wy, ww, wh, 6., top, bottom);
    icons::draw(
        art(card.kind),
        vec2(wx + ww * 0.5, wy + wh * 0.5),
        ww.min(wh) * 0.82,
        with_alpha(ink, 0.12),
    );
    let rows = lines(card, live);
    let n = rows.iter().flatten().count().max(1);
    let row = (wh - 8.) / n as f32;
    let s = (row * 0.6).min(ww * 0.24).clamp(11., 30.);
    for (k, line) in rows.iter().flatten().enumerate() {
        draw_line_item(line, wx + ww * 0.5, wy + 4. + row * (k as f32 + 0.5), s, 1.);
    }
    // Exhaust: a flame in the window's corner.
    if card.exhausts() {
        let fs = (w * 0.12).clamp(11., 17.);
        let at = vec2(wx + fs * 0.6, wy + wh - fs * 0.6);
        draw_circle(at.x, at.y, fs * 0.62, Color::new(0.08, 0.03, 0.02, 0.75));
        icon(Icon::Flame, at.x, at.y, fs * 0.9, EMBER);
    }
    if !look.playable && !wound {
        flat_round(
            x + 1.,
            y + 1.,
            w - 2.,
            h - 2.,
            radius - 1.,
            Color::new(0.02, 0.04, 0.05, 0.34),
        );
    }
}
/// The back of a card: plum velvet with a gilt diamond.
fn card_back(r: Rect) {
    let radius = (r.w * 0.1).clamp(2., 8.);
    flat_round(
        r.x + 1.,
        r.y + 3.,
        r.w,
        r.h,
        radius,
        Color::new(0., 0., 0., 0.35),
    );
    rounded_gradient(
        r.x,
        r.y,
        r.w,
        r.h,
        radius,
        Color::new(0.95, 0.80, 0.50, 1.),
        Color::new(0.55, 0.38, 0.16, 1.),
    );
    rounded_gradient(
        r.x + 2.,
        r.y + 2.,
        r.w - 4.,
        r.h - 4.,
        (radius - 1.).max(1.),
        Color::new(0.36, 0.18, 0.38, 1.),
        Color::new(0.18, 0.08, 0.20, 1.),
    );
    let c = r.center();
    let s = r.w.min(r.h) * 0.3;
    draw_poly(c.x, c.y, 4, s, 0., with_alpha(GOLD, 0.85));
    draw_poly(c.x, c.y, 4, s * 0.5, 0., PLUM);
}
/// A wordless card: suit-tinted face and its art, for cards in motion.
fn mini_card(card: Card, r: Rect) {
    let (top, bottom, ink) = suit_colors(suit(card.kind));
    let radius = (r.w * 0.1).clamp(2., 8.);
    rounded_gradient(
        r.x,
        r.y,
        r.w,
        r.h,
        radius,
        if card.upgraded { GOLD } else { PARCHMENT },
        if card.upgraded {
            darken(GOLD, 0.3)
        } else {
            PARCHMENT_LOW
        },
    );
    let inset = (r.w * 0.08).max(1.5);
    rounded_gradient(
        r.x + inset,
        r.y + inset,
        r.w - inset * 2.,
        r.h - inset * 2.,
        (radius - 1.).max(1.),
        top,
        bottom,
    );
    icon(
        art(card.kind),
        r.x + r.w * 0.5,
        r.y + r.h * 0.5,
        r.w.min(r.h) * 0.62,
        ink,
    );
}

// ---------------------------------------------------------------------------
// Surfaces and controls.

/// Raised velvet panel: offset shadow, hairline rim and a lit gradient.
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
/// Key cap with an optional action label; returns the total width.
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
    let st = plain(14., ink);
    cap + 7. + draw_text_styled(label, x + cap + 7., st.centered_baseline(y + 10.), st).width
}
fn key_width(key: &str, label: &str) -> f32 {
    let cap = (text_width(key, plain(13., PARCHMENT)) + 12.).max(20.);
    if label.is_empty() {
        cap
    } else {
        cap + 7. + text_width(label, plain(14., CREAM))
    }
}
/// A tactile button: an icon, a one-word label and the key that does the same.
fn button(r: Rect, glyph: Icon, key: &str, label: &str, primary: bool, enabled: bool) {
    let hover = enabled && r.contains(pointer());
    let a = if enabled { 1. } else { 0.42 };
    let lift = if hover { -2. } else { 0. };
    let (x, y, w, h) = (r.x, r.y + lift, r.w, r.h);
    flat_round(x, r.y + 4., w, h, 10., Color::new(0., 0., 0., 0.35 * a));
    let (top, bottom) = if primary {
        (
            Color::new(1., 0.86, 0.56, a),
            Color::new(0.80, 0.58, 0.24, a),
        )
    } else {
        (
            Color::new(0.42, 0.24, 0.42, a),
            Color::new(0.24, 0.12, 0.26, a),
        )
    };
    let top = if hover { lighten(top, 0.12) } else { top };
    rounded_gradient(x, y, w, h, 10., top, bottom);
    let ink = with_alpha(if primary { SEPIA } else { CREAM }, a);
    let label_w = text_width(label, plain(16., ink));
    let kw = key_width(key, "");
    let total = 22. + 8. + label_w + 10. + kw;
    let mut cx = x + (w - total) * 0.5;
    icon(glyph, cx + 11., y + h * 0.5, 22., ink);
    cx += 30.;
    let st = plain(16., ink);
    cx += draw_text_styled(label, cx, st.centered_baseline(y + h * 0.5), st).width + 10.;
    key_hint(key, "", cx, y + h * 0.5 - 10., with_alpha(CREAM, a));
}
/// The number key that picks a card, in a dark disc on its lower corner.
fn corner_key(r: Rect, index: usize) {
    let at = vec2(r.right() - 9., r.bottom() - 9.);
    draw_circle(at.x, at.y, 8.5, Color::new(0.10, 0.06, 0.04, 0.88));
    let s = plain(11., CREAM);
    draw_text_styled(
        DIGITS[(index + 1).min(9)],
        at.x,
        s.centered_baseline(at.y),
        s.with_align(TextAlign::Center),
    );
}
/// A one-word name that appears over whatever the pointer rests on.
fn tooltip(label: &str, at: Vec2) {
    let s = plain(13., CREAM);
    let w = text_width(label, s) + 16.;
    let r = Rect::new(at.x - w * 0.5, at.y - 26., w, 21.);
    flat_round(r.x, r.y + 2., r.w, r.h, 7., Color::new(0., 0., 0., 0.4));
    rounded(r.x, r.y, r.w, r.h, 7., Color::new(0.07, 0.04, 0.08, 0.96));
    outline(r.x, r.y, r.w, r.h, 7., 1., with_alpha(GOLD, 0.45));
    draw_text_styled(
        label,
        at.x,
        s.centered_baseline(r.y + r.h * 0.5),
        s.with_align(TextAlign::Center),
    );
}
/// Width of an [`icon_chip`] with `label` at `size`, for centering one.
fn chip_width(label: &str, size: f32) -> f32 {
    let text = if label.is_empty() {
        0.
    } else {
        text_width(label, style(size, CREAM)) + size * 0.35
    };
    (size * 1.2 + size * 0.95 + text).max(size * 1.45)
}
/// A number in a small dark pill, for badge counts.
fn count_pill(value: &str, at: Vec2, color: Color) {
    let s = plain(12., color);
    let w = (text_width(value, s) + 8.).max(16.);
    flat_round(
        at.x - w * 0.5,
        at.y - 8.,
        w,
        16.,
        8.,
        Color::new(0.05, 0.03, 0.06, 0.95),
    );
    draw_text_styled(
        value,
        at.x,
        s.centered_baseline(at.y),
        s.with_align(TextAlign::Center),
    );
}

// ---------------------------------------------------------------------------
// Portraits.

/// A robed figure: the Liberator (green eyes) or a captor (red eyes) with
/// its own hat, mask or crown. `(x, y)` is the chest.
fn portrait(x: f32, y: f32, index: usize, hero: bool) {
    let robe = if hero {
        Color::new(0.38, 0.26, 0.46, 1.)
    } else {
        [
            Color::new(0.46, 0.22, 0.21, 1.),
            Color::new(0.30, 0.15, 0.33, 1.),
            Color::new(0.40, 0.34, 0.20, 1.),
        ][index % 3]
    };
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
    let eye = if hero {
        Color::new(0.44, 0.83, 0.66, 1.)
    } else {
        HURT
    };
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
            draw_circle(x + dx, y - 56., 2.5, HURT);
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
    draw_poly(x, y + 37., 4, 8.5, 0., Color::new(0., 0., 0., 0.35));
    draw_poly(x, y + 36., 4, 7., 0., GOLD);
}
/// A small flickering campfire with logs.
fn campfire(x: f32, y: f32, t: f32) {
    spotlight(
        x,
        y - 8.,
        70.,
        Color::new(1., 0.55, 0.2, 0.22 + 0.06 * pulse(t, 2.3)),
    );
    draw_line(
        x - 18.,
        y + 6.,
        x + 16.,
        y - 1.,
        5.,
        Color::new(0.42, 0.26, 0.15, 1.),
    );
    draw_line(
        x - 16.,
        y - 1.,
        x + 18.,
        y + 6.,
        5.,
        Color::new(0.34, 0.20, 0.11, 1.),
    );
    let flick = |hz: f32, phase: f32| 0.85 + 0.15 * ((t + phase) * hz).sin();
    let h1 = 30. * flick(9.1, 0.);
    let h2 = 20. * flick(12.7, 1.3);
    draw_triangle(
        vec2(x - 13., y + 2.),
        vec2(x + (t * 5.).sin() * 2., y - h1),
        vec2(x + 13., y + 2.),
        Color::new(0.96, 0.45, 0.16, 1.),
    );
    draw_triangle(
        vec2(x - 7., y + 2.),
        vec2(x + 1. + (t * 7.).cos() * 2., y - h2),
        vec2(x + 8., y + 2.),
        Color::new(1., 0.84, 0.42, 1.),
    );
}

// ---------------------------------------------------------------------------
// Fighters: health, block and status badges.

/// A predicted outcome to ghost onto a health bar.
#[derive(Clone, Copy)]
struct Ghost {
    /// Health and block right after the previewed action.
    hp: i16,
    block: i16,
    /// Pulse strongly (the action under the pointer or selected).
    strong: bool,
    /// Health after the enemy's next move, drawn faintly.
    threat: Option<i16>,
}
/// Health bar under a fighter: a heart (or a shield carrying block) at its
/// left, the fill, and any predicted change as a pulsing ghost segment.
fn vitals(f: &Fighter, ghost: Option<Ghost>, cx: f32, y: f32, hero: bool, t: f32) {
    let (w, h) = (164., 14.);
    let x = cx - w * 0.5 + 10.;
    let frac = |hp: i16| (f32::from(hp) / f32::from(f.max_hp.max(1))).clamp(0., 1.);
    flat_round(x, y, w, h, h * 0.5, Color::new(0., 0., 0., 0.5));
    let low = hero && frac(f.hp) < 0.3;
    let color = if low {
        shapes::mix(FOE_HP, Color::new(1., 0.6, 0.5, 1.), 0.3 * pulse(t, 1.6))
    } else if hero {
        HERO_HP
    } else {
        FOE_HP
    };
    let fw = w * frac(f.hp);
    if fw > 0. {
        rounded_gradient(x, y, fw.max(h), h, h * 0.5, lighten(color, 0.2), color);
        draw_rectangle(
            x + h * 0.5,
            y + 2.,
            (fw - h).max(0.),
            1.,
            Color::new(1., 1., 1., 0.25),
        );
    }
    if let Some(g) = ghost {
        let to = w * frac(g.hp);
        let top = w * frac(g.hp.max(f.hp));
        if let Some(after) = g.threat.filter(|&after| after < g.hp.max(f.hp)) {
            let from = w * frac(after);
            rounded(
                x + from,
                y,
                (top - from).max(3.),
                h,
                3.,
                Color::new(1., 0.9, 0.85, 0.2 + 0.18 * pulse(t, 1.2)),
            );
        }
        if g.hp < f.hp {
            let a = if g.strong {
                0.5 + 0.4 * pulse(t, 2.2)
            } else {
                0.22 + 0.2 * pulse(t, 1.2)
            };
            rounded(
                x + to,
                y,
                (fw - to).max(3.),
                h,
                3.,
                Color::new(1., 0.95, 0.9, a),
            );
        } else if g.hp > f.hp {
            rounded(
                x + fw,
                y,
                (to - fw).max(3.),
                h,
                3.,
                with_alpha(MEND, 0.45 + 0.35 * pulse(t, 2.)),
            );
        }
    }
    if f.block > 0 {
        outline(x - 1.5, y - 1.5, w + 3., h + 3., h * 0.5 + 1.5, 1.5, SHIELD);
    }
    let hp = Label::new().num(f.hp.into()).push("/").num(f.max_hp.into());
    let hs = plain(12., CREAM).with_shadow(Color::new(0., 0., 0., 0.75));
    draw_text_styled(
        hp.as_str(),
        x + w * 0.5,
        hs.centered_baseline(y + h * 0.5),
        hs.with_align(TextAlign::Center),
    );
    // Block rides on a shield at the bar's left end; otherwise a heart.
    let at = vec2(x - 14., y + h * 0.5);
    let next_block = ghost.map_or(f.block, |g| g.block);
    if f.block > 0 || next_block > 0 {
        icons::draw_styled(
            Icon::Shield,
            at,
            30.,
            IconStyle::new(SHIELD).shadow(Color::new(0., 0., 0., 0.4)),
        );
        count_pill(number(f.block).as_str(), at + vec2(0., 13.), CREAM);
        if next_block != f.block {
            let gain = signed(next_block - f.block);
            let a = 0.6 + 0.4 * pulse(t, 2.);
            let color = if next_block > f.block { SHIELD } else { HURT };
            centered(
                gain.as_str(),
                at.x,
                at.y - 24.,
                60.,
                style(15., with_alpha(color, a)),
            );
        }
    } else {
        icons::draw_styled(
            Icon::Heart,
            at,
            22.,
            IconStyle::new(LIFE).shadow(Color::new(0., 0., 0., 0.4)),
        );
    }
}
/// Up to six status badges centered on `cx`; returns the hovered one.
fn badges(
    f: &Fighter,
    armor: i16,
    next: Option<&Fighter>,
    cx: f32,
    cy: f32,
    t: f32,
) -> Option<(Mark, Vec2)> {
    let n = next.unwrap_or(f);
    let items = [
        (Mark::Armor, armor, armor),
        (Mark::Strength, f.strength, n.strength),
        (Mark::Thorns, f.thorns, n.thorns),
        (Mark::Poison, f.poison.into(), n.poison.into()),
        (Mark::Weak, f.weak.into(), n.weak.into()),
        (Mark::Vulnerable, f.vulnerable.into(), n.vulnerable.into()),
    ];
    let shown = items.iter().filter(|(_, a, b)| *a > 0 || *b > 0).count();
    if shown == 0 {
        return None;
    }
    let (d, gap) = (28., 8.);
    let mut x = cx - (shown as f32 * d + (shown - 1) as f32 * gap) * 0.5 + d * 0.5;
    let p = pointer();
    let mut hovered = None;
    for &(mark, now, then) in items.iter().filter(|(_, a, b)| *a > 0 || *b > 0) {
        let at = vec2(x, cy);
        let color = mark.color();
        let changing = then != now;
        if changing {
            glow(
                at.x,
                at.y,
                d * 0.95,
                with_alpha(color, 0.25 + 0.2 * pulse(t, 2.)),
            );
        }
        if mark == Mark::Armor {
            // Armor is permanent: a squared plate rather than a round badge.
            rounded(
                at.x - d * 0.5,
                at.y - d * 0.5,
                d,
                d,
                6.,
                Color::new(0.07, 0.07, 0.09, 0.94),
            );
            outline(
                at.x - d * 0.5,
                at.y - d * 0.5,
                d,
                d,
                6.,
                1.5,
                with_alpha(color, 0.8),
            );
        } else {
            shapes::fill_circle(at, d * 0.5, Color::new(0.07, 0.05, 0.09, 0.94));
            shapes::stroke_circle(at, d * 0.5, 1.5, with_alpha(color, 0.85));
        }
        icon(mark.icon(), at.x, at.y - 1., 16., color);
        let value = if changing { then } else { now };
        count_pill(
            number(value).as_str(),
            at + vec2(d * 0.38, d * 0.36),
            if changing { lighten(color, 0.3) } else { CREAM },
        );
        if (p - at).length() < d * 0.55 {
            hovered = Some((mark, at));
        }
        x += d + gap;
    }
    hovered
}

// ---------------------------------------------------------------------------
// Screens.

impl Game {
    /// Damage a card's printed `base` would deal to the current enemy after
    /// strength, weak, vulnerable and armor (block is shown separately).
    fn hit_value(&self, base: i16) -> i16 {
        (Game::damage_value(
            base,
            self.hero.strength,
            self.hero.weak,
            self.enemy.vulnerable,
        ) - self.content().armor)
            .max(0)
    }
    /// How far hand card `index` is through its deal-in flight (0..1), or
    /// `None` once it has landed.
    fn dealing(&self, index: usize) -> Option<f32> {
        let (from, count, tick) = self.show.dealt?;
        let (from, count) = (usize::from(from), usize::from(count));
        if !(from..from + count).contains(&index) {
            return None;
        }
        let k = (index - from) as f32;
        let travel = (self.animation.wrapping_sub(tick) as f32 - k * 3.) / 10.;
        (travel < 1.).then_some(travel.clamp(0., 1.))
    }
    fn base_energy(&self) -> u8 {
        if self.relics[4] { 4 } else { 3 }
    }
    fn battle_view(&self) {
        let t = clock();
        let p = pointer();
        let n = self.hand.len();
        let now = self.animation;
        let hovered = layout::hand_at(p, n);
        let over_end = layout::END_TURN.contains(p);
        // Predictions come from running the real rules on a copy: what the
        // focused card would do, and what ending the turn would cost.
        let focus = hovered.or((self.selected < n).then_some(self.selected));
        let card_sim = focus
            .filter(|&i| !over_end && self.dealing(i).is_none() && self.playable(i))
            .map(|i| {
                let mut sim = self.clone();
                sim.play(i);
                (i, sim)
            });
        let mut turn_sim = self.clone();
        turn_sim.end_turn();
        // The enemy's next move after the focused card: a Guard visibly
        // shrinks what is coming. A lethal card ends the fight first.
        let threat = match &card_sim {
            Some((_, sim)) if sim.phase == Phase::Battle => {
                let mut after = sim.clone();
                after.end_turn();
                after.hero.hp
            }
            Some((_, sim)) => sim.hero.hp,
            None => turn_sim.hero.hp,
        };
        let preview = if over_end {
            Some(&turn_sim)
        } else {
            card_sim.as_ref().map(|(_, sim)| sim)
        };
        let mut tip: Option<(&'static str, Vec2)> = None;

        // The duel: the Liberator on the left, the captor on the right.
        let hero_age = age(now, self.show.hero_hurt);
        let foe_age = age(now, self.show.enemy_hurt);
        let lunge = self
            .show
            .enemy_acted
            .filter(|(attack, _)| *attack)
            .map_or(0., |(_, tick)| bump(now.wrapping_sub(tick) as f32 / 18.));
        let swing = self.show.flight.map_or(0., |f| {
            if aims_at_enemy(f.card.kind) {
                bump(now.wrapping_sub(f.tick) as f32 / 14.)
            } else {
                0.
            }
        });
        let shake = |a: f32| {
            if a < 16. {
                (a * 1.7).sin() * 6. * (1. - a / 16.)
            } else {
                0.
            }
        };
        let hero_at = layout::HERO + vec2(shake(hero_age) + swing * 18., bob(t, 0.45, 1.6));
        let foe_at = layout::ENEMY + vec2(shake(foe_age) - lunge * 30., bob(t + 1.3, 0.4, 1.6));
        for (at, warm) in [(layout::HERO, 0.13), (layout::ENEMY, 0.17)] {
            spotlight(at.x, at.y + 10., 104., Color::new(0.95, 0.70, 0.35, warm));
            draw_ellipse(
                at.x,
                at.y + 72.,
                62.,
                12.,
                0.,
                Color::new(0., 0.02, 0.03, 0.5),
            );
        }
        portrait(hero_at.x, hero_at.y, 0, true);
        if hero_age < 18. {
            glow(
                hero_at.x,
                hero_at.y,
                70.,
                with_alpha(HURT, 0.5 * (1. - hero_age / 18.)),
            );
        }
        portrait(foe_at.x, foe_at.y, self.floor, false);
        if foe_age < 18. {
            glow(
                foe_at.x,
                foe_at.y - 6.,
                72.,
                Color::new(1., 0.95, 0.9, 0.45 * (1. - foe_age / 18.)),
            );
        }
        // Target the focused card: a reticle on the captor or a halo on the
        // Liberator, wherever its effect lands.
        if let Some((i, sim)) = &card_sim {
            let kind = self.hand[*i].kind;
            if aims_at_enemy(kind) || sim.enemy != self.enemy {
                reticle(foe_at + vec2(0., 4.), t);
            }
            if !aims_at_enemy(kind) || sim.hero != self.hero {
                shapes::stroke_circle(
                    hero_at + vec2(0., 6.),
                    66. + 3. * pulse(t, 1.5),
                    2.,
                    with_alpha(SHIELD, 0.35 + 0.25 * pulse(t, 1.5)),
                );
            }
        }
        self.enemy_header(t);
        self.intent_bubble(t, &mut tip);

        // Health, block and statuses, with the previewed outcome ghosted.
        let hero_ghost = if over_end {
            Ghost {
                hp: turn_sim.hero.hp,
                block: self.hero.block,
                strong: true,
                threat: None,
            }
        } else {
            let (hp, block) = card_sim
                .as_ref()
                .map_or((self.hero.hp, self.hero.block), |(_, sim)| {
                    (sim.hero.hp, sim.hero.block)
                });
            Ghost {
                hp,
                block,
                strong: true,
                threat: Some(threat),
            }
        };
        vitals(
            &self.hero,
            Some(hero_ghost),
            layout::HERO.x,
            layout::BAR_Y,
            true,
            t,
        );
        if threat == 0 || (over_end && turn_sim.hero.hp == 0) {
            // Ending the turn (after the focused card) would be fatal.
            let a = 0.55 + 0.45 * pulse(t, 1.8);
            icon(
                Icon::Skull,
                layout::HERO.x + 100.,
                layout::BAR_Y + 7.,
                22.,
                with_alpha(HURT, a),
            );
        }
        let foe_ghost = preview.map(|sim| Ghost {
            hp: sim.enemy.hp,
            block: if over_end {
                self.enemy.block
            } else {
                sim.enemy.block
            },
            strong: true,
            threat: None,
        });
        vitals(
            &self.enemy,
            foe_ghost,
            layout::ENEMY.x,
            layout::BAR_Y,
            false,
            t,
        );
        if preview.is_some_and(|sim| sim.enemy.hp == 0) {
            let a = 0.6 + 0.4 * pulse(t, 2.);
            icon(
                Icon::Skull,
                layout::ENEMY.x + 100.,
                layout::BAR_Y + 7.,
                22.,
                with_alpha(GOLD, a),
            );
        }
        let card_next = card_sim.as_ref().map(|(_, sim)| sim);
        if let Some(hit) = badges(
            &self.hero,
            0,
            card_next.map(|s| &s.hero),
            layout::HERO.x,
            layout::BADGE_Y,
            t,
        ) {
            tip = Some((hit.0.name(), hit.1 - vec2(0., 14.)));
        }
        if let Some(hit) = badges(
            &self.enemy,
            self.content().armor,
            card_next.map(|s| &s.enemy),
            layout::ENEMY.x,
            layout::BADGE_Y,
            t,
        ) {
            tip = Some((hit.0.name(), hit.1 - vec2(0., 14.)));
        }

        // Energy and draw pile on the left; end turn and spent piles on the right.
        self.energy_orb(t, card_next, &mut tip);
        let piles = [
            (layout::DRAW_PILE, self.draw.len(), "Draw pile"),
            (layout::DISCARD_PILE, self.discard.len(), "Discard"),
        ];
        for (k, (at, count, name)) in piles.into_iter().enumerate() {
            let fan = if k == 0 { vec2(2., -2.) } else { vec2(3., 1.) };
            let layers = count.min(3);
            for layer in (0..layers.max(1)).rev() {
                let o = fan * layer as f32;
                let r = Rect::new(at.x - 15. + o.x, at.y - 20. + o.y, 30., 40.);
                if count == 0 {
                    outline(r.x, r.y, r.w, r.h, 4., 1.5, with_alpha(SOFT, 0.3));
                } else {
                    card_back(r);
                }
            }
            count_pill(number(count as i32).as_str(), at + vec2(16., 18.), CREAM);
            if (p - at).length() < 24. {
                tip = Some((name, at - vec2(0., 22.)));
            }
        }
        let ex = layout::EXHAUST_PILE;
        let burning = !self.exhausted.is_empty();
        if burning {
            glow(
                ex.x,
                ex.y,
                26.,
                with_alpha(EMBER, 0.2 + 0.1 * pulse(t, 1.7)),
            );
        }
        icon(
            Icon::Flame,
            ex.x,
            ex.y - 2.,
            28.,
            if burning {
                EMBER
            } else {
                with_alpha(SOFT, 0.35)
            },
        );
        count_pill(
            number(self.exhausted.len() as i32).as_str(),
            ex + vec2(14., 18.),
            CREAM,
        );
        if (p - ex).length() < 22. {
            tip = Some(("Exhausted", ex - vec2(0., 22.)));
        }
        self.end_turn_button(t);
        self.deck_button(p);

        // The hand, gently fanned; the selected card rides high.
        self.hand_view(t, hovered);
        self.flight_view();
        if let Some((label, at)) = tip {
            tooltip(label, at);
        }
    }
    /// The captor's name, with a skull for elites and a crown for the boss.
    fn enemy_header(&self, t: f32) {
        let c = self.content();
        let s = style(18., CREAM);
        let w = text_width(c.name, s);
        let x = layout::ENEMY.x - w * 0.5;
        draw_text_styled(c.name, x, s.centered_baseline(142.), s);
        let marker = if self.floor == ENCOUNTERS.len() - 1 {
            Some((Icon::Crown, GOLD))
        } else if c.elite {
            Some((Icon::Skull, HURT))
        } else {
            None
        };
        if let Some((glyph, color)) = marker {
            let a = 0.8 + 0.2 * pulse(t, 0.7);
            icons::draw_styled(
                glyph,
                vec2(x - 16., 141.),
                20.,
                IconStyle::new(with_alpha(color, a)).shadow(Color::new(0., 0., 0., 0.4)),
            );
        }
    }
    /// The captor's next move as icons over its head.
    fn intent_bubble(&self, t: f32, tip: &mut Option<(&'static str, Vec2)>) {
        let i = self.intent();
        let mut items: [Option<(Mark, i16, u8, bool)>; 7] = [None; 7];
        let mut n = 0;
        let mut add = |item: (Mark, i16, u8, bool)| {
            items[n] = Some(item);
            n += 1;
        };
        let attack = i.hits > 0;
        if attack {
            let per_hit = Game::damage_value(
                i.damage,
                self.enemy.strength,
                self.enemy.weak,
                self.hero.vulnerable,
            );
            add((Mark::Damage, per_hit, i.hits, false));
        }
        if i.block > 0 {
            add((Mark::Block, i.block, 0, false));
        }
        if i.strength > 0 {
            add((Mark::Strength, i.strength, 0, true));
        }
        if i.poison > 0 {
            add((Mark::Poison, i.poison.into(), 0, false));
        }
        if i.weak > 0 {
            add((Mark::Weak, i.weak.into(), 0, false));
        }
        if i.vulnerable > 0 {
            add((Mark::Vulnerable, i.vulnerable.into(), 0, false));
        }
        if i.wound {
            add((Mark::Wound, 1, 0, true));
        }
        let (size, gap, pad) = (21., 14., 12.);
        let item_w = |(mark, value, times, plus): (Mark, i16, u8, bool)| {
            let label = if plus { signed(value) } else { number(value) };
            let mut w = mark.width(size) + 4. + text_width(label.as_str(), style(size, CREAM));
            if times > 1 {
                w += 3. + text_width("x2", style(13., CREAM));
            }
            w
        };
        let total =
            items.iter().flatten().map(|&it| item_w(it)).sum::<f32>() + gap * (n.max(1) - 1) as f32;
        let w = total + pad * 2.;
        let h = 36.;
        let cy = layout::INTENT_Y + bob(t, 0.7, 2.2);
        let x = layout::ENEMY.x - w * 0.5;
        let tone = if attack { HURT } else { GOLD };
        let deadly = attack && self.incoming() >= self.hero.hp + self.hero.block;
        let edge = if deadly {
            with_alpha(HURT, 0.6 + 0.4 * pulse(t, 2.))
        } else {
            with_alpha(tone, 0.7)
        };
        flat_round(
            x,
            cy - h * 0.5 + 3.,
            w,
            h,
            12.,
            Color::new(0., 0., 0., 0.35),
        );
        draw_triangle(
            vec2(layout::ENEMY.x - 8., cy + h * 0.5 - 1.),
            vec2(layout::ENEMY.x + 8., cy + h * 0.5 - 1.),
            vec2(layout::ENEMY.x, cy + h * 0.5 + 8.),
            edge,
        );
        rounded(x, cy - h * 0.5, w, h, 12., edge);
        rounded(
            x + 1.5,
            cy - h * 0.5 + 1.5,
            w - 3.,
            h - 3.,
            10.5,
            Color::new(0.10, 0.05, 0.07, 0.96),
        );
        let mut ix = x + pad;
        for &(mark, value, times, plus) in items.iter().flatten() {
            let iw = item_w((mark, value, times, plus));
            draw_mark(mark, vec2(ix + size * 0.5, cy), size, mark.color());
            let label = if plus { signed(value) } else { number(value) };
            let s = style(size, CREAM);
            let tw = draw_text_styled(
                label.as_str(),
                ix + mark.width(size) + 4.,
                s.centered_baseline(cy),
                s,
            )
            .width;
            if times > 1 {
                let small = plain(13., SOFT);
                let x2 = Label::new().push("x").num(times.into());
                draw_text_styled(
                    x2.as_str(),
                    ix + mark.width(size) + 4. + tw + 3.,
                    s.centered_baseline(cy),
                    small,
                );
            }
            if (pointer() - vec2(ix + iw * 0.5, cy)).length() < 18. {
                *tip = Some((mark.name(), vec2(ix + iw * 0.5, cy - h * 0.5)));
            }
            ix += iw + gap;
        }
    }
    /// A gold orb holding this turn's energy; pips below count it out and
    /// blink for what the focused card would spend.
    fn energy_orb(&self, t: f32, next: Option<&Game>, tip: &mut Option<(&'static str, Vec2)>) {
        let c = layout::ENERGY;
        let e = self.energy;
        let spend = next.map_or(0, |g| e.saturating_sub(g.energy));
        let lit = e > 0;
        glow(
            c.x,
            c.y,
            50.,
            with_alpha(ENERGY, if lit { 0.2 + 0.1 * pulse(t, 0.8) } else { 0.05 }),
        );
        let (inner, outer) = if lit {
            (
                Color::new(1., 0.96, 0.74, 1.),
                Color::new(0.86, 0.56, 0.16, 1.),
            )
        } else {
            (
                Color::new(0.45, 0.40, 0.34, 1.),
                Color::new(0.22, 0.18, 0.15, 1.),
            )
        };
        shapes::fill_circle(c, 28., Fill::radial(c - vec2(8., 9.), 40., inner, outer));
        shapes::stroke_circle(c, 28., 2., darken(GOLD, 0.4));
        icon(Icon::Bolt, c.x, c.y, 38., with_alpha(SEPIA, 0.16));
        let s = plain(28., SEPIA);
        draw_text_styled(
            number(e).as_str(),
            c.x,
            s.centered_baseline(c.y),
            s.with_align(TextAlign::Center),
        );
        let total = self.base_energy().max(e).min(10);
        let gap = 12.;
        let x0 = c.x - (total as f32 - 1.) * gap * 0.5;
        for k in 0..total {
            let at = vec2(x0 + k as f32 * gap, c.y + 40.);
            if k < e {
                let blinking = k >= e - spend;
                let a = if blinking {
                    0.25 + 0.75 * pulse(t, 2.5)
                } else {
                    1.
                };
                shapes::fill_circle(at, 4.5, with_alpha(ENERGY, a));
            } else {
                shapes::stroke_circle(at, 4., 1.5, with_alpha(ENERGY, 0.35));
            }
        }
        if (pointer() - c).length() < 32. {
            *tip = Some(("Energy", c - vec2(0., 30.)));
        }
    }
    fn end_turn_button(&self, t: f32) {
        let r = layout::END_TURN;
        // Nothing left to play: ending the turn is the next step, so it glows.
        let idle = !(0..self.hand.len()).any(|i| self.playable(i));
        if idle {
            glow(
                r.center().x,
                r.center().y,
                r.w * 0.75,
                with_alpha(GOLD, 0.18 + 0.14 * pulse(t, 1.2)),
            );
        }
        let hover = r.contains(pointer());
        let lift = if hover { -2. } else { 0. };
        flat_round(r.x, r.y + 4., r.w, r.h, 12., Color::new(0., 0., 0., 0.38));
        let (top, bottom) = if idle {
            (
                Color::new(1., 0.86, 0.56, 1.),
                Color::new(0.80, 0.58, 0.24, 1.),
            )
        } else {
            (
                Color::new(0.44, 0.25, 0.44, 1.),
                Color::new(0.24, 0.12, 0.26, 1.),
            )
        };
        let top = if hover { lighten(top, 0.12) } else { top };
        rounded_gradient(r.x, r.y + lift, r.w, r.h, 12., top, bottom);
        let ink = if idle { SEPIA } else { CREAM };
        let s = plain(15., ink);
        let label_w = 22. + text_width("End turn", s);
        let x = r.center().x - label_w * 0.5;
        icon(Icon::Hourglass, x + 8., r.y + lift + 19., 18., ink);
        draw_text_styled(
            "End turn",
            x + 22.,
            s.centered_baseline(r.y + lift + 19.),
            s,
        );
        key_hint("X", "", r.center().x - 10., r.y + lift + 30., CREAM);
    }
    /// The permanent deck, one click (or C) away in every phase.
    fn deck_button(&self, p: Vec2) {
        let r = layout::DECK_BUTTON;
        let hover = r.contains(p);
        rounded(
            r.x,
            r.y,
            r.w,
            r.h,
            10.,
            Color::new(0.03, 0.06, 0.07, if hover { 0.75 } else { 0.5 }),
        );
        if hover {
            outline(r.x, r.y, r.w, r.h, 10., 1.5, with_alpha(GOLD, 0.6));
        }
        let cy = r.y + r.h * 0.5;
        cards_glyph(vec2(r.x + 18., cy), 22., CREAM);
        let s = style(17., CREAM);
        draw_text_styled(
            number(self.deck.len() as i32).as_str(),
            r.x + 33.,
            s.centered_baseline(cy),
            s,
        );
        key_hint("C", "", r.right() - 28., cy - 10., CREAM);
    }
    fn hand_view(&self, t: f32, hovered: Option<usize>) {
        let n = self.hand.len();
        let mid = (n as f32 - 1.) * 0.5;
        for pass in 0..3 {
            for (i, &card) in self.hand.iter().enumerate() {
                let selected = i == self.selected;
                let hover = hovered == Some(i) && !selected;
                let layer = if selected {
                    2
                } else if hover {
                    1
                } else {
                    0
                };
                if layer != pass {
                    continue;
                }
                let (x, w) = layout::hand_slot(i, n);
                let fan = (i as f32 - mid).abs() * 2.;
                let rest = Rect::new(x, HAND_TOP + fan - if hover { 8. } else { 0. }, w, CARD_H);
                let r = if selected {
                    Rect::new(x - w * 0.03, HAND_TOP - RAISE, w * 1.06, CARD_H + 4.)
                } else {
                    rest
                };
                // Freshly drawn cards fly in face down from the draw pile.
                if let Some(travel) = self.dealing(i) {
                    let e = Ease::CubicOut.apply(travel);
                    let pile = layout::DRAW_PILE;
                    let start = Rect::new(pile.x - 15., pile.y - 20., 30., 40.);
                    card_back(lerp_rect(start, r, e));
                    continue;
                }
                let playable = self.playable(i);
                card_face(
                    card,
                    r,
                    Look {
                        selected,
                        hovered: hover,
                        playable,
                    },
                    Some(self),
                );
                if i < 5 {
                    corner_key(r, i);
                }
                if selected && playable {
                    // Click it again (or press Space) to play.
                    let at = vec2(r.x + r.w * 0.5, r.y - 2. + bob(t, 1.1, 2.));
                    glow(
                        at.x,
                        at.y,
                        22.,
                        with_alpha(GOLD, 0.35 + 0.2 * pulse(t, 1.4)),
                    );
                    icon_badge(Icon::Play, at.x, at.y, 22., GOLD, SEPIA);
                }
            }
        }
    }
    /// The card that was just played sweeps toward its target and shrinks.
    fn flight_view(&self) {
        let Some(f) = self.show.flight else { return };
        let travel = self.animation.wrapping_sub(f.tick) as f32 / 14.;
        if travel >= 1. {
            return;
        }
        let e = Ease::CubicIn.apply(travel.clamp(0., 1.));
        let (x, w) = layout::hand_slot(f.slot.into(), f.of.into());
        let from = Rect::new(x, HAND_TOP - RAISE, w, CARD_H);
        let target = if aims_at_enemy(f.card.kind) {
            layout::ENEMY
        } else {
            layout::HERO
        };
        let to = Rect::new(target.x - 16., target.y - 22., 32., 44.);
        mini_card(f.card, lerp_rect(from, to, e));
    }

    // Between battles ------------------------------------------------------

    fn menu_view(&self) {
        let t = clock();
        let p = pointer();
        let hero = layout::MENU_HERO;
        spotlight(
            hero.x,
            hero.y + 10.,
            104.,
            Color::new(0.95, 0.70, 0.35, 0.13),
        );
        draw_ellipse(
            hero.x,
            hero.y + 72.,
            62.,
            12.,
            0.,
            Color::new(0., 0.02, 0.03, 0.5),
        );
        if self.phase == Phase::Camp {
            campfire(hero.x + 76., hero.y + 64., t);
        }
        portrait(hero.x, hero.y + bob(t, 0.45, 1.6), 0, true);
        vitals(&self.hero, None, hero.x, hero.y + 92., true, t);
        self.deck_button(p);
        match self.phase {
            Phase::Oath | Phase::Relic => {
                let base = if self.phase == Phase::Relic { 3 } else { 0 };
                for slot in 0..3 {
                    self.relic_option(slot, base + slot, t);
                }
            }
            Phase::Reward => self.reward_view(),
            Phase::Camp => self.camp_view(t),
            Phase::Shop => self.shop_view(t),
            Phase::Won | Phase::Lost => self.record_view(),
            _ => {}
        }
    }
    /// A choice card's frame: lifts under the pointer, glows when selected,
    /// and carries its number key. Returns the drawn rectangle.
    fn choice_frame(&self, slot: usize, count: usize, enabled: bool, t: f32) -> Rect {
        let r = layout::option_rect(slot, count);
        let selected = self.selected == slot;
        let hovered = r.contains(pointer());
        let lift = if selected {
            8.
        } else if hovered && enabled {
            5.
        } else {
            0.
        };
        let r = Rect::new(r.x, r.y - lift, r.w, r.h);
        if selected {
            glow(
                r.center().x,
                r.center().y,
                r.w * 0.8,
                with_alpha(GOLD, 0.14 + 0.08 * pulse(t, 1.1)),
            );
        }
        flat_round(
            r.x,
            r.y + 6. + lift,
            r.w,
            r.h,
            14.,
            Color::new(0., 0., 0., 0.36),
        );
        if selected {
            rounded_gradient(
                r.x,
                r.y,
                r.w,
                r.h,
                14.,
                Color::new(1., 0.86, 0.56, 1.),
                darken(GOLD, 0.35),
            );
        } else {
            rounded_gradient(
                r.x,
                r.y,
                r.w,
                r.h,
                14.,
                Color::new(1., 1., 1., if hovered { 0.2 } else { 0.11 }),
                Color::new(1., 1., 1., 0.03),
            );
        }
        rounded_gradient(
            r.x + 2.,
            r.y + 2.,
            r.w - 4.,
            r.h - 4.,
            12.,
            Color::new(0.24, 0.13, 0.26, 1.),
            Color::new(0.12, 0.06, 0.14, 1.),
        );
        r
    }
    /// The number key that picks a choice, on its top-right corner.
    fn choice_key(r: Rect, slot: usize) {
        key_hint(DIGITS[slot + 1], "", r.right() - 26., r.y + 8., CREAM);
    }
    /// Dim a choice that cannot be taken, with the reason as an icon.
    fn choice_closed(r: Rect, why: Icon) {
        flat_round(r.x, r.y, r.w, r.h, 14., Color::new(0.02, 0.02, 0.03, 0.55));
        icon_badge(
            why,
            r.center().x,
            r.y + 30.,
            30.,
            Color::new(0.1, 0.1, 0.12, 0.95),
            SOFT,
        );
    }
    fn relic_option(&self, slot: usize, id: usize, t: f32) {
        let owned = self.phase == Phase::Relic && self.relics[id];
        let r = self.choice_frame(slot, 3, !owned, t);
        let color = relic_color(id);
        let cx = r.center().x;
        let gy = r.y + 86. + bob(t + slot as f32, 0.6, 3.);
        glow(
            cx,
            gy,
            58.,
            with_alpha(color, 0.28 + 0.1 * pulse(t + slot as f32 * 0.7, 0.9)),
        );
        icons::draw_styled(
            Icon::Gem,
            vec2(cx, gy),
            70.,
            IconStyle::new(color).shadow(Color::new(0., 0., 0., 0.4)),
        );
        centered(
            RELIC_NAMES[id],
            cx,
            r.y + 162.,
            r.w - 24.,
            style(18., CREAM),
        );
        draw_line_item(&relic_line(id), cx, r.y + 212., 26., 1.);
        Self::choice_key(r, slot);
        if owned {
            Self::choice_closed(r, Icon::Check);
        }
    }
    fn reward_view(&self) {
        // Headline: the spoils of the battle just won.
        let coins = signed(22 + self.floor as i32 * 5);
        let heal = signed(4 + if self.relics[2] { 6 } else { 0 });
        let head = style(26., GOLD);
        let parts = 30.
            + text_width("Victory", head)
            + 22.
            + chip_width(coins.as_str(), 15.)
            + 10.
            + chip_width(heal.as_str(), 15.);
        let mut x = layout::OPTIONS_CX - parts * 0.5;
        let cy = 150.;
        icons::draw_styled(
            Icon::Trophy,
            vec2(x + 13., cy),
            26.,
            IconStyle::new(GOLD).shadow(Color::new(0., 0., 0., 0.4)),
        );
        x += 30.;
        x += draw_text_styled("Victory", x, head.centered_baseline(cy), head).width + 22.;
        x += icon_chip(Icon::Coin, coins.as_str(), x, cy - 11., 15., GOLD) + 10.;
        icon_chip(Icon::Heart, heal.as_str(), x, cy - 11., 15., MEND);
        let p = pointer();
        for (slot, kind) in REWARDS[self.floor].iter().enumerate() {
            let base = layout::option_rect(slot, 3);
            let selected = self.selected == slot;
            let hovered = base.contains(p) && !selected;
            let lift = if selected {
                10.
            } else if hovered {
                6.
            } else {
                0.
            };
            let r = Rect::new(base.x + 6., base.y + 6. - lift, base.w - 12., base.h - 12.);
            card_face(
                Card::new(*kind),
                r,
                Look {
                    selected,
                    hovered,
                    playable: true,
                },
                None,
            );
            corner_key(r, slot);
        }
        button(layout::SKIP, Icon::ArrowRight, "X", "Skip", false, true);
    }
    fn camp_view(&self, t: f32) {
        let open = !self.camp_used;
        let full = self.hero.hp >= self.hero.max_hp;
        let next = &ENCOUNTERS[(self.floor + 1).min(ENCOUNTERS.len() - 1)];
        let options: [(Icon, &str, Color); 4] = [
            (Icon::Moon, "Rest", Color::new(0.70, 0.78, 1.0, 1.)),
            (Icon::Hammer, "Forge", IRON),
            (Icon::Bag, "Shop", GOLD),
            (Icon::ArrowRight, "Depart", CREAM),
        ];
        for (slot, (glyph, name, color)) in options.into_iter().enumerate() {
            let enabled = slot == 3 || (open && !(slot == 0 && full));
            let r = self.choice_frame(slot, 4, enabled, t);
            let cx = r.center().x;
            let gy = r.y
                + 74.
                + if enabled {
                    bob(t + slot as f32, 0.6, 2.5)
                } else {
                    0.
                };
            if slot == 3 && !open {
                // The camp action is spent: departing is what remains.
                glow(cx, gy, 54., with_alpha(GOLD, 0.2 + 0.18 * pulse(t, 1.2)));
            }
            icons::draw_styled(
                glyph,
                vec2(cx, gy),
                56.,
                IconStyle::new(color).shadow(Color::new(0., 0., 0., 0.4)),
            );
            centered(name, cx, r.y + 146., r.w - 20., style(19., CREAM));
            match slot {
                0 => draw_line_item(&Line::plus(Mark::Heal, 18), cx, r.y + 196., 24., 1.),
                1 => {
                    icon(Icon::Plus, cx - 18., r.y + 196., 20., GOLD);
                    cards_glyph(vec2(cx + 10., r.y + 196.), 30., GOLD);
                }
                2 => {
                    icon_value(
                        Icon::Coin,
                        number(self.coins).as_str(),
                        cx - 34.,
                        r.y + 196.,
                        22.,
                        GOLD,
                    );
                }
                _ => {
                    centered(next.name, cx, r.y + 194., r.w - 20., plain(15., SOFT));
                    if next.elite || std::ptr::eq(next, &ENCOUNTERS[ENCOUNTERS.len() - 1]) {
                        let glyph = if next.elite { Icon::Skull } else { Icon::Crown };
                        icon(
                            glyph,
                            cx,
                            r.y + 222.,
                            18.,
                            if next.elite { HURT } else { GOLD },
                        );
                    }
                }
            }
            Self::choice_key(r, slot);
            if !enabled {
                Self::choice_closed(r, if open { Icon::Heart } else { Icon::Lock });
            }
        }
    }
    fn shop_view(&self, t: f32) {
        // The purse sits above the prices it pays.
        let purse = number(self.coins);
        let w = chip_width(purse.as_str(), 17.);
        icon_chip(
            Icon::Coin,
            purse.as_str(),
            layout::OPTIONS_CX - w * 0.5,
            138.,
            17.,
            GOLD,
        );
        let prices = [35, 25, 20];
        let strike_left = self
            .deck
            .iter()
            .any(|c| c.kind == Kind::Strike && !c.upgraded);
        for slot in 0..4 {
            let affordable = slot == 3 || self.coins >= prices[slot.min(2)];
            let r = self.choice_frame(slot, 4, affordable, t);
            let cx = r.center().x;
            match slot {
                0 => {
                    let face = Rect::new(cx - 50., r.y + 34., 100., 136.);
                    card_face(
                        Card::new(self.shop_card()),
                        face,
                        Look {
                            playable: true,
                            ..Look::default()
                        },
                        None,
                    );
                }
                1 => {
                    icons::draw_styled(
                        Icon::Potion,
                        vec2(cx, r.y + 70. + bob(t, 0.6, 2.)),
                        56.,
                        IconStyle::new(LIFE).shadow(Color::new(0., 0., 0., 0.4)),
                    );
                    centered("Tonic", cx, r.y + 134., r.w - 20., style(18., CREAM));
                    draw_line_item(&Line::plus(Mark::Heal, 28), cx, r.y + 172., 24., 1.);
                }
                2 => {
                    let face = Rect::new(cx - 30., r.y + 28., 60., 82.);
                    mini_card(Card::new(Kind::Strike), face);
                    struck(face.center(), 34., HURT);
                    centered("Trim", cx, r.y + 134., r.w - 20., style(18., CREAM));
                    let line = Line::plus(Mark::Draw, -1);
                    draw_line_item(
                        &line,
                        cx,
                        r.y + 172.,
                        24.,
                        if strike_left { 1. } else { 0.4 },
                    );
                }
                _ => {
                    icons::draw_styled(
                        Icon::Undo,
                        vec2(cx, r.y + 80.),
                        52.,
                        IconStyle::new(CREAM).shadow(Color::new(0., 0., 0., 0.4)),
                    );
                    centered("Back", cx, r.y + 146., r.w - 20., style(19., CREAM));
                }
            }
            if slot < 3 {
                let price = Label::new().num(prices[slot].into());
                let color = if affordable { GOLD } else { HURT };
                let pw = chip_width(price.as_str(), 16.);
                icon_chip(
                    Icon::Coin,
                    price.as_str(),
                    cx - pw * 0.5,
                    r.bottom() - 40.,
                    16.,
                    color,
                );
            }
            Self::choice_key(r, slot);
            if !affordable {
                Self::choice_closed(r, Icon::Coin);
            }
        }
    }
    /// The nine encounters as a row of medallions (behind the results card).
    fn record_view(&self) {
        for (i, encounter) in ENCOUNTERS.iter().enumerate() {
            let at = vec2(300. + i as f32 * 72., 300.);
            let won = self.won_turns[i] > 0;
            let fell = !won && i == self.floor && self.phase == Phase::Lost;
            let color = if won {
                GOLD
            } else if fell {
                HURT
            } else {
                with_alpha(SOFT, 0.35)
            };
            shapes::fill_circle(at, 22., Color::new(0.08, 0.05, 0.09, 0.9));
            shapes::stroke_circle(at, 22., 2., color);
            icon(
                if won {
                    Icon::Check
                } else if fell {
                    Icon::Cross
                } else if encounter.elite {
                    Icon::Skull
                } else {
                    Icon::Lock
                },
                at.x,
                at.y,
                20.,
                color,
            );
        }
    }

    // Forge and deck ----------------------------------------------------------

    fn forge_view(&self) {
        let t = clock();
        let p = pointer();
        let page = self.selected / layout::PAGE;
        let pages = self.deck.len().div_ceil(layout::PAGE).max(1);
        let hovered = layout::forge_at(p)
            .map(|s| page * layout::PAGE + s)
            .filter(|&i| i < self.deck.len());
        for slot in 0..layout::PAGE {
            let i = page * layout::PAGE + slot;
            let Some(&card) = self.deck.get(i) else { break };
            let selected = i == self.selected;
            let hover = hovered == Some(i) && !selected;
            let base = layout::forge_cell(slot);
            let lift = if selected {
                8.
            } else if hover {
                5.
            } else {
                0.
            };
            let r = Rect::new(base.x, base.y - lift, base.w, base.h);
            card_face(
                card,
                r,
                Look {
                    selected,
                    hovered: hover,
                    playable: true,
                },
                None,
            );
            if card.upgraded {
                icon_badge(Icon::Check, r.right() - 6., r.y + 6., 22., GOLD, SEPIA);
            }
        }
        if pages > 1 {
            let w = 10. + (pages - 1) as f32 * 13.;
            pips(306. - w * 0.5, 520., (page + 1) as u32, pages as u32, GOLD);
        }
        // Before and after: the selected card and its gilded self.
        if let Some(&chosen) = self.deck.get(self.selected) {
            let mut gilded = chosen;
            gilded.upgraded = true;
            let big = Rect::new(736., 150., 184., 250.);
            spotlight(
                big.center().x,
                big.center().y,
                150.,
                Color::new(1., 0.78, 0.4, 0.14),
            );
            if !chosen.upgraded {
                card_face(
                    chosen,
                    Rect::new(600., 196., 102., 140.),
                    Look {
                        playable: true,
                        ..Look::default()
                    },
                    None,
                );
                let ax = 719. + bob(t, 0.8, 2.);
                icon(Icon::ArrowRight, ax, 266., 24., GOLD);
            }
            card_face(
                gilded,
                big,
                Look {
                    selected: !chosen.upgraded,
                    playable: true,
                    ..Look::default()
                },
                None,
            );
            if chosen.upgraded {
                icon_badge(Icon::Check, big.right() - 6., big.y + 6., 30., GOLD, SEPIA);
            } else {
                for k in 0..3 {
                    let a = t * 1.3 + k as f32 * 2.1;
                    let at = big.center() + vec2(a.cos() * 104., a.sin() * 132.);
                    glow(
                        at.x,
                        at.y,
                        9.,
                        with_alpha(GOLD, 0.35 + 0.3 * pulse(t + k as f32, 1.5)),
                    );
                }
            }
            button(
                layout::GILD,
                Icon::Hammer,
                "Space",
                "Gild",
                true,
                !chosen.upgraded,
            );
        }
        button(layout::BACK, Icon::Undo, "X", "", false, true);
        self.deck_button(p);
    }
    fn deck_view(&self) {
        rounded(
            16.,
            112.,
            928.,
            456.,
            12.,
            Color::new(0.01, 0.02, 0.03, 0.84),
        );
        slab(28., 120., 904., 440., VELVET);
        // Size and gilded count, top left.
        cards_glyph(vec2(64., 162.), 36., CREAM);
        let s = style(26., CREAM);
        draw_text_styled(
            number(self.deck.len() as i32).as_str(),
            88.,
            s.centered_baseline(162.),
            s,
        );
        let gilded = self.deck.iter().filter(|c| c.upgraded).count();
        if gilded > 0 {
            icon(Icon::Plus, 58., 206., 18., GOLD);
            let g = style(20., GOLD);
            draw_text_styled(
                number(gilded as i32).as_str(),
                74.,
                g.centered_baseline(206.),
                g,
            );
        }
        for (i, &card) in self
            .deck
            .iter()
            .enumerate()
            .skip(self.deck_page * layout::PAGE)
            .take(layout::PAGE)
        {
            card_face(
                card,
                layout::deck_cell(i % layout::PAGE),
                Look {
                    playable: true,
                    ..Look::default()
                },
                None,
            );
        }
        let pages = self.deck.len().div_ceil(layout::PAGE).max(1);
        let p = pointer();
        if pages > 1 {
            let w = 10. + (pages - 1) as f32 * 13.;
            pips(
                480. - w * 0.5,
                540.,
                (self.deck_page + 1) as u32,
                pages as u32,
                GOLD,
            );
            for (r, glyph) in [
                (layout::DECK_PREV, Icon::ArrowLeft),
                (layout::DECK_NEXT, Icon::ArrowRight),
            ] {
                let hover = r.contains(p);
                let c = r.center();
                shapes::fill_circle(
                    c,
                    r.w * 0.5,
                    Color::new(0.30, 0.17, 0.32, if hover { 1. } else { 0.75 }),
                );
                icon(glyph, c.x, c.y, 26., CREAM);
            }
        }
        let r = layout::DECK_CLOSE;
        let c = r.center();
        let hover = r.contains(p);
        shapes::fill_circle(
            c,
            r.w * 0.5,
            Color::new(0.30, 0.17, 0.32, if hover { 1. } else { 0.75 }),
        );
        icon(Icon::Cross, c.x, c.y, 20., CREAM);
        key_hint("C", "", r.x - 30., c.y - 10., CREAM);
    }
    fn render(&self) {
        // Velvet card table.
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
    /// What to do now: a verb phrase, and the captor's name in battle.
    fn goal(&self) -> (&'static str, &'static str) {
        match self.phase {
            Phase::Oath => ("Choose a relic", ""),
            Phase::Battle => ("Defeat", self.content().name),
            Phase::Reward => ("Take a card or skip", ""),
            Phase::Relic => ("Claim a relic", ""),
            Phase::Camp if self.camp_used => ("Depart", ""),
            Phase::Camp => ("Rest, forge or shop", ""),
            Phase::Forge => ("Gild one card", ""),
            Phase::Shop => ("Buy one item", ""),
            Phase::Won => ("The palace is free", ""),
            Phase::Lost => ("Expedition ended", ""),
        }
    }
}

/// A pulsing aiming reticle around a fighter.
fn reticle(at: Vec2, t: f32) {
    let r = 64. + 3. * pulse(t, 1.6);
    let color = with_alpha(GOLD, 0.55 + 0.3 * pulse(t, 1.6));
    for k in 0..4 {
        let start = k as f32 * std::f32::consts::FRAC_PI_2 + 0.35;
        shapes::stroke_arc(
            at,
            r,
            start,
            0.85,
            shapes::Stroke::new(2.5, color).cap(shapes::Cap::Round),
        );
    }
}

// ---------------------------------------------------------------------------
// Header slots shared by drawing and effects (the host's objective row).

const ROW_Y: f32 = 81.;
/// Owned relic `k` of `n`, right-aligned against the purse.
fn relic_slot(k: usize, n: usize) -> Vec2 {
    vec2(COIN_SLOT - 36. - (n - 1 - k.min(n - 1)) as f32 * 26., ROW_Y)
}
const COIN_SLOT: f32 = 792.;
const TURN_SLOT: f32 = 878.;

/// Toward the middle of the table from a fighter: +1 for the Liberator.
fn inward(side: Side) -> f32 {
    if side == Side::Hero { 1. } else { -1. }
}
/// Damage numbers rise between the fighters, clear of the enemy's intent.
fn hit_at(side: Side) -> Vec2 {
    anchor(side) + vec2(inward(side) * 84., -44.)
}
/// Block changes rise just below the damage numbers.
fn guard_at(side: Side) -> Vec2 {
    anchor(side) + vec2(inward(side) * 84., 2.)
}
/// Statuses rise on the outer side of a fighter.
fn status_at(side: Side) -> Vec2 {
    anchor(side) + vec2(-inward(side) * 86., -24.)
}
fn body(side: Side) -> Vec2 {
    anchor(side) + vec2(0., 8.)
}
fn anchor(side: Side) -> Vec2 {
    match side {
        Side::Hero => layout::HERO,
        Side::Enemy => layout::ENEMY,
    }
}
fn refusal(why: Refusal) -> (Icon, Label) {
    match why {
        Refusal::Energy(cost) => (Icon::Bolt, Label::new().push("Need ").num(cost.into())),
        Refusal::Locked => (Icon::Lock, Label::new().push("Locked")),
        Refusal::Health => (Icon::Heart, Label::new().push("Too low")),
        Refusal::Coins(price) => (Icon::Coin, Label::new().push("Need ").num(price.into())),
        Refusal::Full => (Icon::Heart, Label::new().push("Full")),
        Refusal::Used => (Icon::Check, Label::new().push("Used")),
        Refusal::Owned => (Icon::Gem, Label::new().push("Owned")),
        Refusal::Gilded => (Icon::Plus, Label::new().push("Gilded")),
        Refusal::DeckFull => (Icon::Book, Label::new().push("Full")),
        Refusal::Nothing => (Icon::Cross, Label::new().push("None")),
    }
}

impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn hud(&self) -> Hud {
        let (verb, name) = self.goal();
        Hud {
            stage: ENCOUNTERS[self.stage()].place.to_string(),
            objective: if name.is_empty() {
                verb.to_string()
            } else {
                format!("{verb} {name}")
            },
            status: format!(
                "{}/{} HP, {} coins, {} cards",
                self.hero.hp,
                self.hero.max_hp,
                self.coins,
                self.deck.len()
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
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.stage() as u32 + 1, ENCOUNTERS.len() as u32))
    }
    fn draw_objective(&self, area: Rect) -> bool {
        let cy = area.y + area.h * 0.5;
        let accent = GOLD;
        icons::draw(
            Icon::Target,
            vec2(area.x + 10., cy),
            18.,
            with_alpha(accent, 0.9),
        );
        let (verb, name) = self.goal();
        let s = style(17., Color::new(0.84, 0.88, 0.92, 1.));
        let mut x = area.x + 26.;
        x += draw_text_styled(verb, x, s.centered_baseline(cy), s).width;
        if !name.is_empty() {
            draw_text_styled(name, x + 8., s.centered_baseline(cy), s.with_color(CREAM));
        }
        // The run at a glance: relics, deck, purse and (in battle) the turn.
        let p = pointer();
        let owned_count = self.relics.iter().filter(|&&r| r).count();
        let mut k = 0;
        for (id, &owned) in self.relics.iter().enumerate() {
            if owned {
                let at = relic_slot(k, owned_count);
                icons::draw_styled(
                    Icon::Gem,
                    at,
                    20.,
                    IconStyle::new(relic_color(id)).shadow(Color::new(0., 0., 0., 0.4)),
                );
                if (p - at).length() < 12. {
                    tooltip(RELIC_NAMES[id], at + vec2(0., 46.));
                }
                k += 1;
            }
        }
        icon_value(
            Icon::Coin,
            number(self.coins).as_str(),
            COIN_SLOT - 10.,
            cy,
            18.,
            GOLD,
        );
        if self.phase == Phase::Battle {
            let late = self.turn >= 30;
            let color = if late {
                with_alpha(HURT, 0.7 + 0.3 * pulse(clock(), 1.5))
            } else {
                SOFT
            };
            let turn = if late {
                Label::new().num(self.turn.into()).push("/40")
            } else {
                number(self.turn)
            };
            icon_value(
                Icon::Hourglass,
                turn.as_str(),
                TURN_SLOT - 10.,
                cy,
                18.,
                color,
            );
        }
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        let won = self.won_turns.iter().filter(|&&t| t > 0).count();
        let encounters = Label::new().num(won as i32).push("/9");
        let hp = number(self.hero.hp);
        let cards = number(self.deck.len() as i32);
        let coins = number(self.coins);
        let stats: [(Option<Icon>, &str, Color); 4] = [
            (Some(Icon::Flag), encounters.as_str(), GOLD),
            (Some(Icon::Heart), hp.as_str(), LIFE),
            (None, cards.as_str(), CREAM),
            (Some(Icon::Coin), coins.as_str(), GOLD),
        ];
        let col = area.w / stats.len() as f32;
        for (k, (glyph, value, color)) in stats.into_iter().enumerate() {
            let cx = area.x + col * (k as f32 + 0.5);
            let at = vec2(cx, area.y + 30.);
            match glyph {
                Some(g) => icons::draw_styled(
                    g,
                    at,
                    40.,
                    IconStyle::new(color).shadow(Color::new(0., 0., 0., 0.4)),
                ),
                None => cards_glyph(at, 42., color),
            }
            centered(value, cx, area.y + 84., col - 12., style(30., PAPER));
        }
        // Which captors fell: gold for won, red where the run ended.
        let row = area.y + area.h - 14.;
        for i in 0..ENCOUNTERS.len() {
            let at = vec2(area.x + area.w * 0.5 + (i as f32 - 4.) * 26., row);
            if self.won_turns[i] > 0 {
                draw_poly(at.x, at.y, 4, 8., 0., GOLD);
            } else if i == self.floor && self.phase == Phase::Lost {
                icon(Icon::Cross, at.x, at.y, 16., HURT);
            } else {
                shapes::stroke_circle(at, 5., 1.5, with_alpha(SOFT, 0.5));
            }
        }
        true
    }
    fn effects(&self, cue: Cue, fx: &mut Juice) {
        let _ = cue;
        let battle = self.phase == Phase::Battle;
        for event in &self.show.events {
            match *event {
                Event::Battle => {
                    fx.callout(vec2(468., 226.), self.content().name, GOLD);
                    fx.ring(layout::ENEMY, GOLD, 96.);
                }
                Event::Played { card, slot, of } => {
                    let (x, w) = layout::hand_slot(slot.into(), of.into());
                    let at = vec2(x + w * 0.5, HAND_TOP + 30.);
                    if card.exhausts() {
                        fx.burst(at, EMBER, 26);
                        fx.ring(layout::EXHAUST_PILE, EMBER, 34.);
                    } else {
                        fx.burst(at, GOLD, 12);
                    }
                }
                Event::Hurt { side, amount, harm } => match harm {
                    Harm::Attack if amount > 0 => {
                        let color = if side == Side::Enemy { HURT } else { LIFE };
                        fx.popup(hit_at(side), signed(-i32::from(amount)).as_str(), HURT);
                        fx.burst(body(side), color, (10 + amount as u32).min(56));
                        if side == Side::Hero && amount >= 15 {
                            fx.shake(0.25);
                        }
                        if side == Side::Enemy && amount >= 12 {
                            fx.shake((0.12 + f32::from(amount) / 70.).min(0.6));
                        }
                        if amount >= 20 {
                            fx.ring(body(side), HURT, 84.);
                        }
                        if side == Side::Hero && self.hero.hp * 10 < self.hero.max_hp * 3 {
                            fx.vignette(Color::new(0.9, 0.12, 0.12, 0.4));
                        }
                    }
                    Harm::Attack => fx.popup(hit_at(side), "0", IRON),
                    Harm::Poison => {
                        fx.popup_icon(
                            hit_at(side),
                            Icon::Drop,
                            signed(-i32::from(amount)).as_str(),
                            POISON,
                        );
                        fx.burst(body(side), POISON, 14);
                    }
                    Harm::Thorns => {
                        fx.popup_icon(
                            hit_at(side),
                            Icon::Leaf,
                            signed(-i32::from(amount)).as_str(),
                            THORN,
                        );
                        fx.burst(body(side), THORN, 14);
                    }
                    Harm::Price => {
                        fx.popup_icon(
                            hit_at(side),
                            Icon::Heart,
                            signed(-i32::from(amount)).as_str(),
                            LIFE,
                        );
                        fx.burst(body(side), LIFE, 10);
                    }
                },
                Event::Blocked { side, amount } => {
                    fx.popup_icon(
                        guard_at(side),
                        Icon::Shield,
                        signed(-i32::from(amount)).as_str(),
                        SHIELD,
                    );
                    fx.ring(body(side), SHIELD, 58.);
                }
                Event::Guard { side, amount } if amount > 0 => {
                    fx.popup_icon(
                        guard_at(side),
                        Icon::Shield,
                        signed(amount).as_str(),
                        SHIELD,
                    );
                    fx.ring(body(side), SHIELD, 70.);
                }
                Event::Guard { .. } => {}
                Event::Healed { amount } => {
                    let at = if battle {
                        layout::HERO - vec2(0., 92.)
                    } else {
                        layout::MENU_HERO - vec2(0., 92.)
                    };
                    fx.popup_icon(at, Icon::Heart, signed(amount).as_str(), MEND);
                    fx.sparkle(at + vec2(0., 50.), MEND);
                }
                Event::Status {
                    side,
                    status,
                    amount,
                } => {
                    let mark = Mark::of(status);
                    fx.popup_icon(
                        status_at(side),
                        mark.icon(),
                        signed(amount).as_str(),
                        mark.color(),
                    );
                    fx.ring(body(side), mark.color(), 50.);
                }
                Event::Cleansed => {
                    fx.sparkle(body(Side::Hero), CREAM);
                    fx.ring(body(Side::Hero), CREAM, 64.);
                }
                Event::Spent { amount } => {
                    fx.popup_icon(
                        guard_at(Side::Hero),
                        Icon::Shield,
                        signed(-i32::from(amount)).as_str(),
                        SHIELD,
                    );
                }
                Event::Energy { amount } => {
                    fx.popup_icon(
                        layout::ENERGY - vec2(0., 44.),
                        Icon::Bolt,
                        signed(amount).as_str(),
                        ENERGY,
                    );
                    fx.sparkle(layout::ENERGY, ENERGY);
                }
                Event::Wounded => {
                    fx.popup_icon(layout::DISCARD_PILE - vec2(0., 8.), Icon::Cross, "+1", IRON);
                    fx.burst(layout::DISCARD_PILE, IRON, 12);
                }
                Event::Enraged => {
                    fx.callout(layout::ENEMY - vec2(0., 112.), "Enraged", HURT);
                    fx.burst(body(Side::Enemy), EMBER, 40);
                    fx.shake(0.35);
                }
                Event::NewTurn => {
                    fx.ring(layout::ENERGY, ENERGY, 46.);
                    fx.sparkle(layout::ENERGY, ENERGY);
                }
                Event::Victory { coins } => {
                    let at = vec2(layout::OPTIONS_CX, 150.);
                    fx.burst(at, GOLD, 40);
                    fx.ring(at, GOLD, 84.);
                    fx.popup_icon(
                        vec2(COIN_SLOT, ROW_Y + 4.),
                        Icon::Coin,
                        signed(coins).as_str(),
                        GOLD,
                    );
                }
                Event::Defeat => fx.callout(layout::HERO - vec2(0., 100.), "Defeated", HURT),
                Event::TimeUp => {
                    fx.callout(vec2(468., 226.), "Time up", HURT);
                }
                Event::Took { .. } => {
                    let at = layout::DECK_BUTTON.center();
                    fx.popup(at + vec2(0., 8.), "+1", CREAM);
                    fx.sparkle(at, GOLD);
                }
                Event::Camp { option } => {
                    if option == 0 {
                        let r = layout::option_rect(0, 4);
                        fx.ring(r.center(), MEND, 80.);
                    }
                }
                Event::Relic { id } => {
                    let before = self.relics[..usize::from(id)]
                        .iter()
                        .filter(|&&r| r)
                        .count();
                    let total = self.relics.iter().filter(|&&r| r).count().max(1);
                    let at = relic_slot(before, total);
                    fx.sparkle(at, relic_color(id.into()));
                    fx.ring(at, relic_color(id.into()), 30.);
                }
                Event::Gilded { .. } => {
                    let at = layout::option_rect(1, 4).center();
                    fx.sparkle(at, GOLD);
                    fx.burst(at, GOLD, 30);
                    fx.popup(at - vec2(0., 70.), "Gilded", GOLD);
                }
                Event::Bought { item, price } => {
                    fx.popup_icon(
                        vec2(COIN_SLOT, ROW_Y + 4.),
                        Icon::Coin,
                        signed(-i32::from(price)).as_str(),
                        GOLD,
                    );
                    let at = layout::option_rect(2, 4).center();
                    fx.burst(at, GOLD, 24);
                    if item == 0 {
                        fx.popup(layout::DECK_BUTTON.center() + vec2(0., 8.), "+1", CREAM);
                    } else if item == 2 {
                        fx.popup(layout::DECK_BUTTON.center() + vec2(0., 8.), "-1", CREAM);
                    }
                }
                Event::Skipped => fx.ring(layout::SKIP.center(), SOFT, 60.),
                Event::Refused { spot, why } => {
                    let at = match spot {
                        Spot::Hand(i) => {
                            let (x, w) = layout::hand_slot(i.into(), self.hand.len());
                            vec2(x + w * 0.5, HAND_TOP + 50.)
                        }
                        Spot::Choice(i) if self.phase == Phase::Forge => {
                            layout::forge_cell(usize::from(i) % layout::PAGE).center()
                        }
                        Spot::Choice(i) => layout::option_rect(i.into(), self.options()).center(),
                    };
                    fx.ring(at, HURT, 50.);
                    fx.shake(0.12);
                    let (glyph, words) = refusal(why);
                    fx.popup_icon(at - vec2(0., 52.), glyph, words.as_str(), HURT);
                }
                Event::Drew { .. } | Event::EnemyActs { .. } => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::META;
    fn run_to_end(mut visit: impl FnMut(&Game)) -> Game {
        let mut game = Game::new();
        for tick in 0..100_000 {
            let input = game.pilot(tick);
            game.step(&input);
            visit(&game);
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        game
    }
    #[test]
    fn progress_pips_cover_the_whole_campaign() {
        let game = Game::new();
        assert_eq!(ArcadeGame::progress(&game), Some((1, 9)));
        let mut seen_camp = false;
        let end = run_to_end(|g| {
            let (current, total) = ArcadeGame::progress(g).unwrap();
            assert!((1..=total).contains(&current) && total == 9);
            if g.phase == Phase::Camp && g.floor == 0 {
                // Between battles the next captor is already the goal.
                assert_eq!(current, 2);
                seen_camp = true;
            }
        });
        assert!(seen_camp);
        assert_eq!(end.outcome(), Outcome::Won);
        assert_eq!(ArcadeGame::progress(&end), Some((9, 9)));
    }
    #[test]
    fn played_cards_and_refusals_are_felt_where_they_happen() {
        let mut game = Game::new();
        game.step(&Input {
            slot: Some(1),
            ..Input::default()
        });
        let mut fx = Juice::new();
        ArcadeGame::effects(&game, Cue::Good, &mut fx);
        // A battle opens with the captor's name and a ring.
        assert!(fx.counts().popups >= 1 && fx.counts().rings >= 1);
        // Hammerfall: a burst at the card, a hit popup and a status popup.
        game.hand = vec![Card::new(Kind::Crush), Card::new(Kind::Strike)];
        game.selected = 0;
        let cue = game.step(&Input {
            primary: true,
            ..Input::default()
        });
        let mut fx = Juice::new();
        ArcadeGame::effects(&game, cue, &mut fx);
        let counts = fx.counts();
        assert!(counts.sparks > 20, "{counts:?}");
        assert!(counts.popups >= 2 && counts.rings >= 1, "{counts:?}");
        // A refusal shows a red ring and the missing resource.
        game.energy = 0;
        assert_eq!(
            game.step(&Input {
                primary: true,
                ..Input::default()
            }),
            Cue::None
        );
        let mut fx = Juice::new();
        ArcadeGame::effects(&game, Cue::None, &mut fx);
        assert_eq!((fx.counts().rings, fx.counts().popups), (1, 1));
        // An idle tick requests nothing.
        game.step(&Input::default());
        let mut fx = Juice::new();
        ArcadeGame::effects(&game, Cue::None, &mut fx);
        assert!(fx.is_idle());
    }
    #[test]
    fn enemy_turns_pop_damage_on_the_liberator() {
        let mut game = Game::new();
        game.step(&Input {
            slot: Some(1),
            ..Input::default()
        });
        let cue = game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(cue, Cue::Bad);
        let mut fx = Juice::new();
        ArcadeGame::effects(&game, cue, &mut fx);
        assert!(fx.counts().popups >= 1 && fx.counts().sparks > 0);
    }
    #[test]
    fn header_copy_stays_within_the_word_budget() {
        let check = |g: &Game| {
            let hud = ArcadeGame::hud(g);
            assert!(
                hud.objective.split_whitespace().count() <= 5,
                "{}",
                hud.objective
            );
            assert!(
                !hud.stage.chars().any(|c| c.is_ascii_digit()),
                "{}",
                hud.stage
            );
            assert!(hud.stage.split_whitespace().count() <= 3, "{}", hud.stage);
        };
        check(&Game::new());
        let end = run_to_end(check);
        check(&end);
        assert_eq!(end.summary().matches(". ").count(), 0);
        assert!(end.summary().ends_with('.'));
    }
    #[test]
    fn guide_is_four_short_bullets() {
        assert!(META.help.len() <= 4);
        for bullet in META.help {
            assert!(bullet.split_whitespace().count() < 15, "{bullet}");
            assert!(bullet.is_ascii());
            // One line each in the host's guide (600 units at 18), so all
            // four share a single page.
            assert!(text_width(bullet, style(18., PAPER)) <= 600., "{bullet}");
        }
        assert!(META.subtitle.split_whitespace().count() <= 3);
    }
    #[test]
    fn every_card_reads_as_icon_lines() {
        let all = [
            Kind::Strike,
            Kind::Guard,
            Kind::Riposte,
            Kind::Venom,
            Kind::Insight,
            Kind::Crush,
            Kind::Siphon,
            Kind::Flurry,
            Kind::Fortify,
            Kind::Spark,
            Kind::Focus,
            Kind::Gambit,
            Kind::Ward,
            Kind::Reversal,
            Kind::Purify,
            Kind::Finale,
            Kind::Wound,
        ];
        let mut game = Game::new();
        game.step(&Input {
            slot: Some(1),
            ..Input::default()
        });
        for kind in all {
            for upgraded in [false, true] {
                let card = Card { kind, upgraded };
                let printed = lines(card, None);
                assert!(printed[0].is_some(), "{kind:?}");
                let live = lines(card, Some(&game));
                assert_eq!(
                    printed.iter().flatten().count(),
                    live.iter().flatten().count()
                );
            }
        }
        // Live damage includes the Liberator's strength (Gilt Needle).
        let strike = lines(Card::new(Kind::Strike), Some(&game))[0].unwrap();
        assert_eq!((strike.value, strike.trend), (8, 1));
        assert_eq!(number(-12).as_str(), "-12");
        assert_eq!(signed(7).as_str(), "+7");
    }
}
