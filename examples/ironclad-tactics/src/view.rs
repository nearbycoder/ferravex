//! Presentation for Ironclad Tactics: a riveted deck board, tracked squad
//! machines, telegraphed enemy fire, hover previews and an icon sidebar.
//!
//! Everything here reads the model; nothing writes it. Clickable surfaces come
//! from the model's shared hit-test (`Game::hit`), so a hover highlight always
//! matches what a click does, and every preview comes from a read-only mirror
//! of the rules (`Game::preview`, `Game::skill_preview`, `Game::incoming`).
use crate::content::{CELLS, COLS, MISSIONS, Objective, ROWS};
use crate::fx::HURT_TICKS;
use crate::model::{
    BOARD_X, BOARD_Y, CONCEDE, Failure, Game, HOLD_UNTIL, Hit, Intent, Kind, Phase, Pos, Preview,
    RETRY, Role, SIDE_W, SIDE_X, SkillPreview, TILE_SIZE, Tile, card_area, order_area, patch_area,
    upgrade_area,
};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;
use std::f32::consts::FRAC_1_SQRT_2;

pub(crate) const GOLD: Color = Color::new(0.95, 0.70, 0.29, 1.);
pub(crate) const TEAL: Color = Color::new(0.30, 0.79, 0.75, 1.);
pub(crate) const RED: Color = Color::new(0.98, 0.40, 0.34, 1.);
pub(crate) const BLUE: Color = Color::new(0.48, 0.65, 0.96, 1.);
pub(crate) const GREEN: Color = Color::new(0.47, 0.86, 0.52, 1.);
const DARK: Color = Color::new(0.07, 0.10, 0.13, 1.);
const SOFT: Color = Color::new(0.80, 0.85, 0.89, 1.);
const CARD_FILL: Color = Color::new(0.085, 0.115, 0.145, 1.);
const ROW_FILL: Color = Color::new(0.1, 0.135, 0.165, 1.);
/// The debrief's medal stars.
const MEDAL_X: f32 = 74.;
const MEDAL_Y: f32 = 266.;
/// The debrief and setback cards cover the board.
const SHEET: Rect = Rect::new(28., 140., 550., 400.);

pub(crate) fn role_color(role: Role) -> Color {
    match role {
        Role::Vanguard => GOLD,
        Role::Scout => TEAL,
        Role::Engineer => BLUE,
    }
}
/// Each machine is drawn as its skill: brace, focus or repair.
pub(crate) fn role_icon(role: Role) -> Icon {
    match role {
        Role::Vanguard => Icon::Shield,
        Role::Scout => Icon::Eye,
        Role::Engineer => Icon::Wrench,
    }
}
/// Title-case display name for the squad's all-caps call signs.
fn call_sign(role: Role) -> &'static str {
    match role {
        Role::Vanguard => "Brass",
        Role::Scout => "Kestrel",
        Role::Engineer => "Mica",
    }
}
/// The thing an operation's objective marker stands for.
pub(crate) fn objective_icon(objective: Objective, index: usize) -> Icon {
    let _ = index;
    match objective {
        Objective::Eliminate => Icon::Skull,
        Objective::Rescue => Icon::Person,
        Objective::Capture | Objective::Hold => Icon::Bell,
        Objective::Sabotage => Icon::Flame,
        Objective::Liberate => Icon::Gear,
    }
}
fn objective_verb(objective: Objective) -> &'static str {
    match objective {
        Objective::Eliminate => "Eliminate",
        Objective::Rescue => "Rescue",
        Objective::Capture => "Capture",
        Objective::Hold => "Hold",
        Objective::Sabotage => "Sabotage",
        Objective::Liberate => "Liberate",
    }
}
fn interact_verb(objective: Objective) -> &'static str {
    match objective {
        Objective::Rescue => "Rescue",
        Objective::Sabotage => "Disable",
        _ => "Activate",
    }
}
/// Armor, cannon and drive upgrades.
pub(crate) fn upgrade_icon(i: usize) -> Icon {
    [Icon::Heart, Icon::Bomb, Icon::Bolt][i.min(2)]
}
pub(crate) const UPGRADE_GAIN: [&str; 3] = ["+3", "+1", "+1"];
pub(crate) fn medal_center(k: u8) -> Vec2 {
    vec2(MEDAL_X + f32::from(k) * 46., MEDAL_Y)
}
/// The setback card's large reason badge.
pub(crate) fn setback_badge() -> Vec2 {
    vec2(SHEET.x + SHEET.w * 0.5, SHEET.y + 120.)
}
/// Where an upgrade's stat sits on a squad card: hull, cannon or drive.
pub(crate) fn card_stat(unit: usize, upgrade: usize) -> Vec2 {
    let r = card_area(unit);
    match upgrade {
        0 => vec2(r.x + 81., r.y + 44.),
        1 => vec2(r.x + 128., r.y + 66.),
        _ => vec2(r.x + r.w - 40., r.y + 20.),
    }
}

// --- Text: flat cards need no drop shadow, which halves glyph geometry. ---

fn label(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, TextStyle::modern(size, color)).width
}
/// Draw `value` vertically centered on `cy`, anchored at `x` by `align`.
fn mid(value: &str, x: f32, cy: f32, size: f32, color: Color, align: TextAlign) -> f32 {
    let s = TextStyle::modern(size, color).with_align(align);
    draw_text_styled(value, x, s.centered_baseline(cy), s).width
}
/// Small uppercase letter-spaced headline.
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    let s = TextStyle::modern(12., color).with_letter_spacing(2.);
    draw_text_styled(value, x, y, s).width
}

// --- Small shape vocabulary. ---

/// Four L-shaped corner brackets around a square of half-size `r`.
fn brackets(c: Vec2, r: f32, arm: f32, t: f32, color: Color) {
    for (sx, sy) in [(-1., -1.), (1., -1.), (-1., 1.), (1., 1.)] {
        let x = c.x + sx * r;
        let y = c.y + sy * r;
        let hx = if sx < 0. { x } else { x - arm };
        let vy = if sy < 0. { y } else { y - arm };
        panel(hx, if sy < 0. { y } else { y - t }, arm, t, color);
        panel(if sx < 0. { x } else { x - t }, vy, t, arm, color);
    }
}
/// Unit 12-gon (30 degree steps). Tokens are drawn from this table so the
/// per-frame board never evaluates trigonometry.
const RING: [(f32, f32); 12] = [
    (1., 0.),
    (0.866_025_4, 0.5),
    (0.5, 0.866_025_4),
    (0., 1.),
    (-0.5, 0.866_025_4),
    (-0.866_025_4, 0.5),
    (-1., 0.),
    (-0.866_025_4, -0.5),
    (-0.5, -0.866_025_4),
    (0., -1.),
    (0.5, -0.866_025_4),
    (0.866_025_4, -0.5),
];
/// Unit octagon rotated 22.5 degrees (flat top).
const OCTAGON: [(f32, f32); 8] = [
    (0.923_879_5, 0.382_683_4),
    (0.382_683_4, 0.923_879_5),
    (-0.382_683_4, 0.923_879_5),
    (-0.923_879_5, 0.382_683_4),
    (-0.923_879_5, -0.382_683_4),
    (-0.382_683_4, -0.923_879_5),
    (0.382_683_4, -0.923_879_5),
    (0.923_879_5, -0.382_683_4),
];
/// Convex fan over `points` (unit outline, any length), scaled by `rx`/`ry`.
fn fan(x: f32, y: f32, rx: f32, ry: f32, points: &[(f32, f32)], step: usize, color: Color) {
    let n = points.len() / step;
    let at = |k: usize| {
        let (u, v) = points[(k % n) * step];
        vec2(x + u * rx, y + v * ry)
    };
    let center = vec2(x, y);
    for k in 0..n {
        draw_triangle(center, at(k), at(k + 1), color);
    }
}
/// Filled disc; a 12-gon reads as round at token sizes.
fn dot(x: f32, y: f32, r: f32, color: Color) {
    fan(x, y, r, r, &RING, 1, color);
}
/// Flattened disc for ground shadows.
fn oval(x: f32, y: f32, rx: f32, ry: f32, color: Color) {
    fan(x, y, rx, ry, &RING, 1, color);
}
/// Pointy-top hexagon.
fn hexagon(x: f32, y: f32, r: f32, color: Color) {
    fan(x, y, r, r, &RING[1..], 2, color);
}
fn octagon(x: f32, y: f32, r: f32, color: Color) {
    fan(x, y, r, r, &OCTAGON, 1, color);
}
/// Capsule of height `h` from flat fills and two end discs.
fn pill(x: f32, y: f32, w: f32, h: f32, color: Color) {
    let r = h * 0.5;
    if w <= h {
        dot(x + w * 0.5, y + r, w.min(h) * 0.5, color);
        return;
    }
    panel(x + r, y, w - h, h, color);
    dot(x + r, y + r, r, color);
    dot(x + w - r, y + r, r, color);
}
/// Unit quarter arc in 15 degree steps, from +x to +y.
const QUARTER: [(f32, f32); 7] = [
    (1., 0.),
    (0.965_925_8, 0.258_819),
    (0.866_025_4, 0.5),
    (FRAC_1_SQRT_2, FRAC_1_SQRT_2),
    (0.5, 0.866_025_4),
    (0.258_819, 0.965_925_8),
    (0., 1.),
];
/// Flat rounded rectangle: three fills and four corner fans that never
/// overlap, so translucent colours blend exactly once.
fn slab(x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    let r = r.min(w * 0.5).min(h * 0.5).max(0.);
    if r < 0.5 {
        panel(x, y, w, h, color);
        return;
    }
    panel(x + r, y, w - 2. * r, h, color);
    if h > 2. * r {
        panel(x, y + r, r, h - 2. * r, color);
        panel(x + w - r, y + r, r, h - 2. * r, color);
    }
    for (cx, cy, sx, sy) in [
        (x + w - r, y + h - r, 1., 1.),
        (x + r, y + h - r, -1., 1.),
        (x + r, y + r, -1., -1.),
        (x + w - r, y + r, 1., -1.),
    ] {
        let c = vec2(cx, cy);
        for k in 0..6 {
            let (a, b) = (QUARTER[k], QUARTER[k + 1]);
            draw_triangle(
                c,
                c + vec2(a.0 * sx, a.1 * sy) * r,
                c + vec2(b.0 * sx, b.1 * sy) * r,
                color,
            );
        }
    }
}
/// Opaque slab with a `border`-coloured rim of `t` units.
fn framed(rect: Rect, r: f32, t: f32, border: Color, fill: Color) {
    let Rect { x, y, w, h } = rect;
    slab(x, y, w, h, r, border);
    slab(x + t, y + t, w - 2. * t, h - 2. * t, (r - t).max(0.), fill);
}
/// Raised plate: two soft shadow layers, a rim and a lit top edge.
fn plate(x: f32, y: f32, w: f32, h: f32, fill: Color) {
    slab(
        x - 4.,
        y + 2.,
        w + 8.,
        h + 10.,
        14.,
        Color::new(0., 0., 0., 0.12),
    );
    slab(
        x - 1.,
        y + 2.,
        w + 2.,
        h + 4.,
        11.,
        Color::new(0., 0., 0., 0.22),
    );
    framed(Rect::new(x, y, w, h), 10., 1., lighten(fill, 0.07), fill);
    panel(x + 10., y + 1., w - 20., 1., Color::new(1., 1., 1., 0.05));
}
/// Background the play area's tags sit on, for opaque tint mixing.
const GROUND: Color = Color::new(0.045, 0.06, 0.085, 1.);
fn tint(color: Color, amount: f32) -> Color {
    let m = |a: f32, b: f32| a + (b - a) * amount;
    Color::new(
        m(GROUND.r, color.r),
        m(GROUND.g, color.g),
        m(GROUND.b, color.b),
        1.,
    )
}
/// Key cap with an optional action label, matching the host footer; returns
/// its width.
fn key(cap: &str, action: &str, x: f32, y: f32, ink: Color) -> f32 {
    let s = TextStyle::modern(13., PAPER);
    let h = 20.;
    let w = (text_width(cap, s) + 12.).max(h);
    slab(x, y + 2., w, h, 4., Color::new(0., 0., 0., 0.5));
    framed(
        Rect::new(x, y, w, h),
        4.,
        1.,
        Color::new(0.24, 0.28, 0.34, 1.),
        Color::new(0.165, 0.2, 0.255, 1.),
    );
    draw_text_styled(
        cap,
        x + w * 0.5,
        s.centered_baseline(y + h * 0.5),
        s.with_align(TextAlign::Center),
    );
    if action.is_empty() {
        return w;
    }
    let a = TextStyle::modern(13., ink);
    w + 7. + draw_text_styled(action, x + w + 7., a.centered_baseline(y + h * 0.5), a).width
}
/// A small value pill centered on (`cx`, `cy`): an optional icon and a short
/// number, rimmed in `color`. Returns its width.
fn bubble(cx: f32, cy: f32, icon: Option<Icon>, value: &str, color: Color) -> f32 {
    let s = TextStyle::modern(13., PAPER);
    let tw = if value.is_empty() {
        0.
    } else {
        text_width(value, s)
    };
    let iw = if icon.is_some() { 14. } else { 0. };
    let gap = if icon.is_some() && !value.is_empty() {
        3.
    } else {
        0.
    };
    let w = (tw + iw + gap + 14.).max(22.);
    let (x, y, h) = (cx - w * 0.5, cy - 11., 22.);
    slab(x, y + 2., w, h, 11., Color::new(0., 0., 0., 0.4));
    framed(Rect::new(x, y, w, h), 11., 1.5, color, tint(color, 0.24));
    let mut left = cx - (tw + iw + gap) * 0.5;
    if let Some(i) = icon {
        ferravex_arcade::icon(i, left + iw * 0.5, cy, iw, lighten(color, 0.25));
        left += iw + gap;
    }
    if !value.is_empty() {
        draw_text_styled(value, left, s.centered_baseline(cy), s);
    }
    w
}
/// A right-aligned value pill ending at `right`; returns its left edge.
fn pill_value(right: f32, cy: f32, icon: Option<Icon>, value: &str, color: Color) -> f32 {
    let s = TextStyle::modern(13., lighten(color, 0.3));
    let tw = if value.is_empty() {
        0.
    } else {
        text_width(value, s)
    };
    let iw = if icon.is_some() { 14. } else { 0. };
    let gap = if icon.is_some() && !value.is_empty() {
        3.
    } else {
        0.
    };
    let w = (tw + iw + gap + 14.).max(22.);
    let x = right - w;
    framed(
        Rect::new(x, cy - 11., w, 22.),
        11.,
        1.,
        tint(color, 0.45),
        tint(color, 0.16),
    );
    let mut left = x + 7.;
    if let Some(i) = icon {
        ferravex_arcade::icon(i, left + iw * 0.5, cy, iw, lighten(color, 0.2));
        left += iw + gap;
    }
    if !value.is_empty() {
        draw_text_styled(value, left, s.centered_baseline(cy), s);
    }
    x
}
/// `count` action-point pips ending at `right`, after a bolt; returns the
/// left edge.
fn cost_pips(right: f32, cy: f32, count: u8, color: Color) -> f32 {
    if count == 0 {
        return right;
    }
    let mut x = right;
    for _ in 0..count {
        x -= 11.;
        slab(x, cy - 6., 8., 12., 2., color);
        panel(x + 1., cy - 6., 6., 1., Color::new(1., 1., 1., 0.35));
    }
    x -= 10.;
    icon(Icon::Bolt, x, cy, 14., color);
    x - 8.
}
/// A dashed segment from `a` to `b`.
fn dashed(a: Vec2, b: Vec2, width: f32, color: Color) {
    let d = b - a;
    let length = d.length();
    if length < 1. {
        return;
    }
    let dir = d / length;
    let mut s = 0.;
    while s < length {
        let e = (s + 7.).min(length);
        let (p, q) = (a + dir * s, a + dir * e);
        draw_line(p.x, p.y, q.x, q.y, width, color);
        s += 12.;
    }
}
/// A mini enemy chassis for counters (patrol hexagon, rammer block, sentry
/// disc, Warden octagon).
fn token(kind: Kind, cx: f32, cy: f32, r: f32, color: Color) {
    match kind {
        Kind::Gunner => hexagon(cx, cy, r, color),
        Kind::Bruiser => slab(cx - r * 0.9, cy - r * 0.8, r * 1.8, r * 1.6, r * 0.3, color),
        Kind::Sentry => dot(cx, cy, r * 0.9, color),
        Kind::Warden => octagon(cx, cy, r * 1.15, color),
    }
}
/// One step of an objective chain in the header.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Done,
    Now,
    Later,
}
fn step_marker(glyph: Icon, cx: f32, cy: f32, step: Step, t: f32) {
    let at = vec2(cx, cy);
    match step {
        Step::Done => {
            dot(cx, cy, 13., TEAL);
            icon(glyph, cx, cy, 15., DARK);
        }
        Step::Now => {
            glow(cx, cy, 22., with_alpha(GOLD, 0.12 + 0.14 * pulse(t, 0.9)));
            dot(cx, cy, 13., tint(GOLD, 0.2));
            shapes::stroke_circle(at, 13., 1.5, GOLD);
            icon(glyph, cx, cy, 15., GOLD);
        }
        Step::Later => {
            dot(cx, cy, 13., tint(MUTED, 0.1));
            shapes::stroke_circle(at, 13., 1., with_alpha(MUTED, 0.5));
            icon(glyph, cx, cy, 15., with_alpha(MUTED, 0.8));
        }
    }
}
fn tile_rect(p: Pos) -> (f32, f32) {
    (
        BOARD_X + p.x as f32 * TILE_SIZE,
        BOARD_Y + p.y as f32 * TILE_SIZE,
    )
}
fn unit_dir(from: Vec2, to: Vec2, fallback: Vec2) -> Vec2 {
    let d = to - from;
    if d.length_squared() > 1. {
        d.normalize()
    } else {
        fallback
    }
}
/// Accent stripe on an overlay card's top edge, as on host menus.
fn stripe(color: Color, top: f32) {
    rounded(SHEET.x + 24., top, 64., 3., 1.5, color);
}

/// Per-frame presentation facts computed once and shared by every layer.
struct Frame {
    time: f32,
    hit: Option<Hit>,
    /// The cell the board preview describes: the hovered cell, else the cursor.
    focus: Pos,
    hovering: bool,
    preview: Preview,
    /// What Space (and the order row) would do: always at the cursor.
    order: Preview,
    skill: SkillPreview,
    /// AP the previewed order would spend (its pips blink on the card).
    cost: u8,
    reach: [u8; CELLS],
    incoming: [u8; 3],
}

impl Game {
    fn frame(&self) -> Frame {
        let battle = self.phase == Phase::Battle;
        let hit = self.hit(pointer());
        let (focus, hovering) = match hit {
            Some(Hit::Cell(p)) => (p, true),
            _ => (self.cursor, false),
        };
        let preview = if battle {
            self.preview(focus)
        } else {
            Preview::Stay
        };
        let order = if battle && hovering {
            self.preview(self.cursor)
        } else {
            preview.clone()
        };
        let patient = match hit {
            Some(Hit::Patch(i)) => Some(i),
            _ => None,
        };
        let skill = self.skill_preview(patient);
        let skill_cost = if self.units[self.selected].role == Role::Engineer {
            2
        } else {
            1
        };
        let cost = match (&hit, &preview) {
            (Some(Hit::Skill | Hit::Patch(_)), _) if skill.refusal.is_none() => skill_cost,
            (Some(Hit::Skill | Hit::Patch(_) | Hit::EndTurn | Hit::Card(_)), _) => 0,
            (
                _,
                Preview::Move {
                    path,
                    refusal: None,
                },
            ) => (path.len() - 1) as u8,
            (_, Preview::Fire { refusal: None, .. } | Preview::Interact { refusal: None, .. }) => 2,
            _ => 0,
        };
        Frame {
            time: clock(),
            hit,
            focus,
            hovering,
            preview,
            order,
            skill,
            cost: if battle { cost } else { 0 },
            reach: if battle {
                self.reach()
            } else {
                [u8::MAX; CELLS]
            },
            incoming: self.incoming(),
        }
    }
    fn wall(&self, x: i32, y: i32) -> bool {
        let p = Pos::new(x, y);
        p.inside() && self.tiles[p.index()] == Tile::Wall
    }

    // --- Board. ---

    fn draw_board(&self, f: &Frame, covered: bool) {
        plate(20., 132., 566., 416., Color::new(0.045, 0.06, 0.08, 1.));
        if covered {
            return;
        }
        let battle = self.phase == Phase::Battle;
        self.draw_deck();
        if battle {
            self.draw_reach(f);
            self.draw_threat(f);
        }
        self.draw_guard_zone(f);
        self.draw_objectives(f);
        if battle {
            self.draw_route(f);
        }
        self.draw_intents(f);
        self.draw_enemies(f);
        self.draw_units(f);
        self.draw_tracers();
        if battle {
            self.draw_cursor(f);
        }
        self.draw_forecast_labels();
        if battle {
            self.draw_preview_labels(f);
        }
        self.draw_turn_banner();
    }
    /// "Turn N" over the top bulkhead after each enemy phase; a newer turn
    /// simply replaces it.
    fn draw_turn_banner(&self) {
        let a = self.show.turn_banner();
        if a <= 0. {
            return;
        }
        let rise = (1. - (f32::from(self.show.turn_age) / 6.).min(1.)) * 8.;
        let cx = BOARD_X + TILE_SIZE * COLS as f32 * 0.5;
        let cy = BOARD_Y + 25. + rise;
        let value = format!("Turn {}", self.round);
        let s = TextStyle::modern(22., with_alpha(PAPER, a));
        let w = text_width(&value, s) + 62.;
        slab(
            cx - w * 0.5,
            cy - 17. + 3.,
            w,
            34.,
            17.,
            Color::new(0., 0., 0., 0.35 * a),
        );
        framed(
            Rect::new(cx - w * 0.5, cy - 17., w, 34.),
            17.,
            1.5,
            with_alpha(GOLD, 0.8 * a),
            Color::new(0.08, 0.1, 0.13, 0.94 * a),
        );
        icon(
            Icon::Hourglass,
            cx - w * 0.5 + 22.,
            cy,
            18.,
            with_alpha(GOLD, a),
        );
        draw_text_styled(&value, cx - w * 0.5 + 38., s.centered_baseline(cy), s);
    }
    fn draw_deck(&self) {
        // Deck plates: alternating tone, a top bevel and two bolts each.
        for y in 0..ROWS {
            for x in 0..COLS {
                let p = Pos::new(x as i32, y as i32);
                if self.tiles[p.index()] == Tile::Wall {
                    continue;
                }
                let (px, py) = tile_rect(p);
                let base = if (x + y) % 2 == 0 {
                    Color::new(0.115, 0.155, 0.18, 1.)
                } else {
                    Color::new(0.13, 0.17, 0.195, 1.)
                };
                panel(px + 1., py + 1., 48., 48., base);
                panel(px + 1., py + 1., 48., 1., Color::new(1., 1., 1., 0.05));
                panel(px + 1., py + 48., 48., 1., Color::new(0., 0., 0., 0.28));
                panel(px + 4., py + 4., 2., 2., Color::new(0.25, 0.3, 0.33, 1.));
                panel(px + 44., py + 44., 2., 2., Color::new(0.06, 0.08, 0.1, 1.));
            }
        }
        // Bulkheads join their neighbours into continuous steel.
        let steel = Color::new(0.165, 0.2, 0.225, 1.);
        let face = Color::new(0.25, 0.29, 0.32, 1.);
        for y in 0..ROWS as i32 {
            for x in 0..COLS as i32 {
                if !self.wall(x, y) {
                    continue;
                }
                let (px, py) = tile_rect(Pos::new(x, y));
                panel(px, py, 50., 50., steel);
                if !self.wall(x, y - 1) {
                    panel(px, py, 50., 5., face);
                    panel(px, py, 50., 1., Color::new(1., 1., 1., 0.14));
                }
                if !self.wall(x, y + 1) {
                    panel(px, py + 42., 50., 8., Color::new(0.095, 0.115, 0.13, 1.));
                    if y + 1 < ROWS as i32 {
                        panel(px, py + 50., 50., 7., Color::new(0., 0., 0., 0.22));
                    }
                }
                if !self.wall(x - 1, y) {
                    panel(px, py, 1., 50., Color::new(1., 1., 1., 0.08));
                } else {
                    panel(px, py + 6., 1., 36., Color::new(0., 0., 0., 0.3));
                }
                let rivet = Color::new(0.3, 0.35, 0.38, 1.);
                panel(px + 7., py + 12., 3., 3., rivet);
                panel(px + 40., py + 32., 3., 3., rivet);
            }
        }
        // Low cover: hazard-striped barricades with a ground shadow.
        for y in 0..ROWS {
            for x in 0..COLS {
                let p = Pos::new(x as i32, y as i32);
                if self.tiles[p.index()] != Tile::Cover {
                    continue;
                }
                let (px, py) = tile_rect(p);
                panel(px + 6., py + 41., 40., 4., Color::new(0., 0., 0., 0.35));
                slab(
                    px + 5.,
                    py + 27.,
                    40.,
                    15.,
                    3.,
                    Color::new(0.3, 0.33, 0.28, 1.),
                );
                slab(
                    px + 5.,
                    py + 27.,
                    40.,
                    8.,
                    3.,
                    Color::new(0.4, 0.43, 0.36, 1.),
                );
                for i in 0..4 {
                    let sx = px + 9. + i as f32 * 9.;
                    draw_line(sx, py + 39., sx + 6., py + 30., 3., with_alpha(GOLD, 0.85));
                }
                panel(px + 6., py + 28., 38., 1., Color::new(1., 1., 1., 0.18));
            }
        }
    }
    /// Tiles the selected machine can still reach this turn.
    fn draw_reach(&self, f: &Frame) {
        let color = role_color(self.units[self.selected].role);
        for (index, cost) in f.reach.iter().enumerate() {
            if *cost == 0 || *cost == u8::MAX {
                continue;
            }
            let p = Pos::new((index % COLS) as i32, (index / COLS) as i32);
            let (px, py) = tile_rect(p);
            slab(px + 3., py + 3., 44., 44., 5., with_alpha(color, 0.07));
            panel(px + 22., py + 46., 6., 2., with_alpha(color, 0.35));
        }
    }
    /// Hovering an enemy shows every tile its cannon can reach.
    fn draw_threat(&self, f: &Frame) {
        if !f.hovering {
            return;
        }
        let Some(index) = self.hostile(f.focus) else {
            return;
        };
        let e = self.enemies[index].unwrap();
        for n in 0..CELLS {
            let p = Pos::new((n % COLS) as i32, (n / COLS) as i32);
            if self.tiles[n] == Tile::Wall
                || p == e.pos
                || e.pos.distance(p) > e.kind.range()
                || !self.visible(e.pos, p)
            {
                continue;
            }
            let (px, py) = tile_rect(p);
            slab(px + 2., py + 2., 46., 46., 4., with_alpha(RED, 0.1));
        }
    }
    /// An activated relay shows its guard ring: teal while held, red when an
    /// enemy stands inside it.
    fn draw_guard_zone(&self, f: &Frame) {
        if self.guard().is_none() || !self.completed[0] {
            return;
        }
        let Some(objective) = self.objectives[0] else {
            return;
        };
        let (guarded, contested) = self.guard_state();
        let color = if contested {
            RED
        } else if guarded {
            TEAL
        } else {
            MUTED
        };
        let a = 0.08 + 0.05 * pulse(f.time, 0.7);
        for (dx, dy) in [(0, 0), (0, -1), (1, 0), (0, 1), (-1, 0)] {
            let p = Pos::new(objective.x + dx, objective.y + dy);
            if !p.inside() || self.tiles[p.index()] == Tile::Wall {
                continue;
            }
            let (px, py) = tile_rect(p);
            slab(px + 2., py + 2., 46., 46., 6., with_alpha(color, a));
        }
    }
    fn draw_objectives(&self, f: &Frame) {
        let objective = self.content().objective;
        for (i, p) in self.objectives.iter().enumerate() {
            let Some(p) = p else { continue };
            let done = self.completed[i];
            if objective == Objective::Rescue && done {
                // The operator now rides with the carrier.
                continue;
            }
            let c = p.center();
            let glyph = objective_icon(objective, i);
            if done {
                dot(c.x, c.y, 18., tint(TEAL, 0.3));
                shapes::stroke_circle(c, 18., 2., TEAL);
                icon(glyph, c.x, c.y, 20., lighten(TEAL, 0.2));
                if let Some((held, needed)) = self.guard() {
                    ring_meter(
                        c.x,
                        c.y,
                        23.,
                        3.,
                        f32::from(held) / f32::from(needed.max(1)),
                        lighten(TEAL, 0.25),
                    );
                } else {
                    icon_badge(Icon::Check, c.x + 14., c.y - 14., 14., TEAL, DARK);
                }
                continue;
            }
            let beat = pulse(f.time, 0.8);
            glow(
                c.x,
                c.y,
                26. + 5. * beat,
                with_alpha(GOLD, 0.1 + 0.08 * beat),
            );
            dot(c.x, c.y, 18., DARK);
            shapes::stroke_circle(c, 18., 2., GOLD);
            let size = if objective == Objective::Sabotage {
                // Furnaces flicker.
                20. + 1.5 * (f.time * 11. + i as f32).sin() + (f.time * 7.).sin()
            } else {
                20.
            };
            icon(glyph, c.x, c.y, size, GOLD);
        }
        if objective == Objective::Rescue {
            let c = self.extraction.center();
            let live = self.carrier.is_some();
            let beat = if live { pulse(f.time, 1.2) } else { 0. };
            panel(
                c.x - 21.,
                c.y - 21.,
                42.,
                42.,
                with_alpha(TEAL, 0.1 + 0.1 * beat),
            );
            brackets(c, 21. + 2. * beat, 12., 3., TEAL);
            icon(Icon::Flag, c.x, c.y, 24., if live { TEAL } else { SOFT });
        }
    }
    /// Route dots for a move, a sight line for a shot.
    fn draw_route(&self, f: &Frame) {
        let u = self.units[self.selected];
        let from = self.unit_center(self.selected);
        let color = role_color(u.role);
        match &f.preview {
            Preview::Move { path, refusal } => {
                let tone = if refusal.is_some() {
                    with_alpha(RED, 0.8)
                } else {
                    color
                };
                for pair in path.windows(2) {
                    let (a, b) = (pair[0].center(), pair[1].center());
                    draw_line(a.x, a.y, b.x, b.y, 2., with_alpha(tone, 0.35));
                }
                for p in path.iter().skip(1) {
                    let c = p.center();
                    dot(c.x, c.y, 3.5, tone);
                }
                let end = path.last().copied().unwrap_or(u.pos).center();
                shapes::stroke_circle(end, 14., 2., with_alpha(tone, 0.8));
            }
            Preview::Fire { enemy, refusal, .. } => {
                let to = self.enemy_center(*enemy);
                let tone = if refusal.is_some() {
                    with_alpha(RED, 0.7)
                } else {
                    with_alpha(lighten(GOLD, 0.2), 0.9)
                };
                dashed(from, to, 2., tone);
            }
            _ => {}
        }
    }
    fn draw_intents(&self, f: &Frame) {
        let beat = pulse(f.time, 1.1);
        let end_hover = f.hit == Some(Hit::EndTurn);
        for enemy in self.enemies.iter().flatten().filter(|e| e.hp > 0) {
            let c = enemy.pos.center();
            if !enemy.active {
                // Reserve entry: brackets and the arrival turn.
                panel(c.x - 22., c.y - 22., 44., 44., with_alpha(RED, 0.07));
                brackets(c, 22., 8., 2., with_alpha(RED, 0.75));
                icon(Icon::Hourglass, c.x - 7., c.y, 15., with_alpha(RED, 0.9));
                mid(
                    &enemy.arrives.to_string(),
                    c.x + 4.,
                    c.y,
                    15.,
                    RED,
                    TextAlign::Left,
                );
                continue;
            }
            match enemy.intent {
                Intent::Fire(to) => {
                    let d = to.center();
                    let strong = if end_hover { 1. } else { 0.75 };
                    draw_line(c.x, c.y, d.x, d.y, 5., Color::new(0.98, 0.3, 0.26, 0.14));
                    draw_line(
                        c.x,
                        c.y,
                        d.x,
                        d.y,
                        if end_hover { 2.5 } else { 1.5 },
                        Color::new(0.98, 0.4, 0.34, strong),
                    );
                    panel(
                        d.x - 22.,
                        d.y - 22.,
                        44.,
                        44.,
                        with_alpha(RED, 0.1 + 0.1 * beat),
                    );
                    brackets(d, 22. - 1.5 * beat, 10., 2., RED);
                    draw_line(
                        d.x - 7.,
                        d.y - 7.,
                        d.x + 7.,
                        d.y + 7.,
                        2.,
                        with_alpha(RED, 0.7),
                    );
                    draw_line(
                        d.x + 7.,
                        d.y - 7.,
                        d.x - 7.,
                        d.y + 7.,
                        2.,
                        with_alpha(RED, 0.7),
                    );
                }
                Intent::Move(to) => {
                    let d = to.center();
                    let dir = unit_dir(c, d, Vec2::X);
                    let tip = d - dir * 6.;
                    let side = vec2(-dir.y, dir.x);
                    draw_line(
                        c.x,
                        c.y,
                        tip.x - dir.x * 6.,
                        tip.y - dir.y * 6.,
                        3.,
                        with_alpha(GOLD, 0.8),
                    );
                    draw_triangle(
                        tip,
                        tip - dir * 10. + side * 7.,
                        tip - dir * 10. - side * 7.,
                        GOLD,
                    );
                    dot(d.x, d.y, 4., with_alpha(GOLD, 0.45));
                }
                Intent::Wait => {}
            }
        }
    }
    /// Expected damage on each targeted tile, after cover and brace, summed
    /// when several enemies aim at the same tile.
    fn draw_forecast_labels(&self) {
        let mut total = [0u8; CELLS];
        for enemy in self.enemies.iter().flatten() {
            if enemy.hp == 0 || !enemy.active {
                continue;
            }
            let Intent::Fire(to) = enemy.intent else {
                continue;
            };
            let mut damage = enemy.kind.damage();
            if let Some(i) = self.friendly(to) {
                let cover = u8::from(self.tiles[to.index()] == Tile::Cover);
                let brace = if self.units[i].braced { 2 } else { 0 };
                damage = damage.saturating_sub(cover + brace).max(1);
            }
            total[to.index()] = total[to.index()].saturating_add(damage);
        }
        for (n, damage) in total.iter().enumerate() {
            if *damage == 0 {
                continue;
            }
            let (px, py) = tile_rect(Pos::new((n % COLS) as i32, (n / COLS) as i32));
            let text = format!("-{damage}");
            let w = text_width(&text, TextStyle::modern(12., PAPER)) + 8.;
            slab(
                px + 48. - w,
                py + 2.,
                w,
                15.,
                4.,
                Color::new(0.55, 0.12, 0.1, 0.95),
            );
            mid(
                &text,
                px + 48. - w * 0.5,
                py + 9.5,
                12.,
                PAPER,
                TextAlign::Center,
            );
        }
    }
    fn enemy_center(&self, index: usize) -> Vec2 {
        let e = self.enemies[index].unwrap();
        self.show.enemy_glides[index]
            .at()
            .map_or(e.pos.center(), |(p, _)| p)
    }
    fn unit_center(&self, index: usize) -> Vec2 {
        self.show.glides[index]
            .at()
            .map_or(self.units[index].pos.center(), |(p, _)| p)
    }
    fn draw_enemies(&self, f: &Frame) {
        let battle = self.phase == Phase::Battle;
        let (aimed, aimed_damage, aimed_lethal) = match f.preview {
            Preview::Fire {
                enemy,
                damage,
                lethal,
                refusal: None,
            } => (Some(enemy), damage, lethal),
            _ => (None, 0, false),
        };
        for (index, enemy) in self.enemies.iter().enumerate() {
            let Some(enemy) = enemy.filter(|e| e.active && e.hp > 0) else {
                continue;
            };
            let c = self.enemy_center(index);
            let aim = match enemy.intent {
                Intent::Fire(to) | Intent::Move(to) => unit_dir(c, to.center(), -Vec2::X),
                Intent::Wait => -Vec2::X,
            };
            let hull = Color::new(0.33, 0.13, 0.14, 1.);
            let plate_tone = Color::new(0.52, 0.2, 0.2, 1.);
            let big = enemy.kind == Kind::Warden;
            // Enemies the selected machine can hit right now wear a sight ring.
            if battle && aimed != Some(index) && self.can_hit(index) {
                let beat = pulse(f.time, 1.3);
                shapes::stroke_circle(
                    c,
                    if big { 26. } else { 23. },
                    1.5,
                    with_alpha(lighten(GOLD, 0.3), 0.35 + 0.3 * beat),
                );
            }
            if aimed == Some(index) {
                glow(c.x, c.y, 34., with_alpha(GOLD, 0.25));
                brackets(c, 24., 9., 2., lighten(GOLD, 0.3));
            }
            oval(
                c.x + 2.,
                c.y + 15.,
                if big { 21. } else { 17. },
                5.,
                Color::new(0., 0., 0., 0.4),
            );
            match enemy.kind {
                Kind::Gunner => {
                    hexagon(c.x, c.y, 16., RED);
                    hexagon(c.x, c.y, 14., hull);
                    hexagon(c.x, c.y - 1., 10., plate_tone);
                }
                Kind::Bruiser => {
                    slab(c.x - 15., c.y - 13., 30., 27., 5., RED);
                    slab(c.x - 13., c.y - 11., 26., 23., 4., hull);
                    // Ram plate on the side it is heading for.
                    let side = vec2(-aim.y, aim.x);
                    let front = c + aim * 15.;
                    draw_triangle(
                        front + aim * 6.,
                        front + side * 12.,
                        front - side * 12.,
                        RED,
                    );
                    panel(c.x - 8., c.y - 4., 16., 3., Color::new(1., 0.75, 0.4, 0.9));
                }
                Kind::Sentry => {
                    for leg in [(-14., 14.), (14., 14.), (0., -15.)] {
                        draw_line(
                            c.x,
                            c.y,
                            c.x + leg.0,
                            c.y + leg.1,
                            4.,
                            Color::new(0.24, 0.12, 0.13, 1.),
                        );
                    }
                    dot(c.x, c.y, 12., RED);
                    dot(c.x, c.y, 10., hull);
                }
                Kind::Warden => {
                    glow(
                        c.x,
                        c.y,
                        30.,
                        with_alpha(RED, 0.12 + 0.1 * pulse(f.time, 0.6)),
                    );
                    octagon(c.x, c.y, 21., RED);
                    octagon(c.x, c.y, 19., hull);
                    octagon(c.x, c.y, 13., plate_tone);
                    for (u, v) in [(1., 1.), (-1., 1.), (-1., -1.), (1., -1.)] {
                        dot(c.x + u * 12., c.y + v * 12., 2.5, GOLD);
                    }
                }
            }
            // Turret and barrel aimed at the announced target.
            let side = vec2(-aim.y, aim.x);
            let reach = if enemy.kind == Kind::Sentry { 20. } else { 16. };
            let twin = if big { 3.5 } else { 0. };
            for s in if big { [-1., 1.] } else { [0., 0.] } {
                let o = c + side * (s * twin);
                let tip = o + aim * reach;
                draw_line(o.x, o.y, tip.x, tip.y, 4., Color::new(0.16, 0.07, 0.08, 1.));
                draw_line(o.x, o.y, tip.x, tip.y, 2., Color::new(0.75, 0.32, 0.3, 1.));
            }
            dot(c.x, c.y, 6., Color::new(0.2, 0.08, 0.09, 1.));
            dot(c.x - 1., c.y - 1., 4., Color::new(1., 0.55, 0.45, 1.));
            let hurt = f32::from(self.show.enemy_hurt[index]) / f32::from(HURT_TICKS);
            if hurt > 0. {
                dot(c.x, c.y, 17., with_alpha(PAPER, 0.55 * hurt));
            }
            // Hull strip; an aimed shot's damage blinks at its end.
            let max = f32::from(enemy.kind.hull());
            let frac = (f32::from(enemy.hp) / max).min(1.);
            panel(c.x - 17., c.y + 19., 26., 4., Color::new(0., 0., 0., 0.6));
            panel(c.x - 17., c.y + 19., 26. * frac, 4., RED);
            if aimed == Some(index) {
                let after = f32::from(enemy.hp.saturating_sub(aimed_damage)) / max;
                panel(
                    c.x - 17. + 26. * after,
                    c.y + 19.,
                    26. * (frac - after),
                    4.,
                    with_alpha(PAPER, 0.4 + 0.5 * pulse(f.time, 2.5)),
                );
                if aimed_lethal {
                    icon(Icon::Skull, c.x, c.y - 30., 18., PAPER);
                }
            }
            label(
                &enemy.hp.to_string(),
                c.x + 12.,
                c.y + 24.,
                10.,
                Color::new(1., 0.72, 0.66, 1.),
            );
        }
    }
    fn can_hit(&self, enemy: usize) -> bool {
        matches!(
            self.preview_shot(enemy),
            Some(Preview::Fire { refusal: None, .. })
        )
    }
    fn preview_shot(&self, enemy: usize) -> Option<Preview> {
        let e = self.enemies[enemy]?;
        Some(self.preview(e.pos))
    }
    fn draw_units(&self, f: &Frame) {
        for (i, unit) in self.units.iter().enumerate() {
            let glide = self.show.glides[i].at();
            let c = glide.map_or(unit.pos.center(), |(p, _)| p);
            let color = role_color(unit.role);
            if unit.hp == 0 {
                oval(c.x + 2., c.y + 13., 16., 4., Color::new(0., 0., 0., 0.35));
                slab(
                    c.x - 14.,
                    c.y - 11.,
                    28.,
                    24.,
                    4.,
                    Color::new(0.2, 0.22, 0.24, 1.),
                );
                icon(Icon::Skull, c.x, c.y, 16., Color::new(0.08, 0.1, 0.12, 1.));
                continue;
            }
            let selected = self.selected == i && self.phase == Phase::Battle;
            if selected {
                let beat = pulse(f.time, 0.9);
                draw_circle(c.x, c.y, 24., with_alpha(color, 0.1 + 0.06 * beat));
                draw_circle_lines(
                    c.x,
                    c.y,
                    23. + 1.5 * beat,
                    1.5,
                    with_alpha(lighten(color, 0.5), 0.9),
                );
            }
            oval(c.x + 2., c.y + 16., 19., 5., Color::new(0., 0., 0., 0.42));
            // Treads.
            let tread = Color::new(0.1, 0.12, 0.14, 1.);
            panel(c.x - 19., c.y - 13., 8., 28., tread);
            panel(c.x + 11., c.y - 13., 8., 28., tread);
            panel(c.x - 18., c.y - 14., 6., 30., tread);
            panel(c.x + 12., c.y - 14., 6., 30., tread);
            // Tread links roll while the machine drives.
            let roll = if glide.is_some() {
                (f.time * 40.) % 7.
            } else {
                0.
            };
            for k in 0..4 {
                let ty = c.y - 10. + k as f32 * 7. + roll;
                if ty < c.y + 13. {
                    panel(c.x - 18., ty, 6., 2., Color::new(0.3, 0.33, 0.36, 1.));
                    panel(c.x + 12., ty, 6., 2., Color::new(0.3, 0.33, 0.36, 1.));
                }
            }
            // Hull with a lit top edge.
            slab(c.x - 12., c.y - 13., 24., 28., 5., darken(color, 0.3));
            slab(c.x - 12., c.y - 13., 24., 19., 5., lighten(color, 0.08));
            panel(c.x - 9., c.y - 12., 18., 1., Color::new(1., 1., 1., 0.45));
            let hurt = f32::from(self.show.hurt[i]) / f32::from(HURT_TICKS);
            if hurt > 0. {
                slab(
                    c.x - 12.,
                    c.y - 13.,
                    24.,
                    28.,
                    5.,
                    with_alpha(Color::new(1., 0.5, 0.45, 1.), 0.7 * hurt),
                );
            }
            // Turret faces where it drives, else the selected one tracks the
            // previewed target, else down-range.
            let aim = if let Some((_, heading)) = glide {
                heading
            } else if selected {
                let target = match f.preview {
                    Preview::Fire { enemy, .. } => self.enemy_center(enemy),
                    _ => f.focus.center(),
                };
                unit_dir(c, target, Vec2::X)
            } else {
                Vec2::X
            };
            let tip = c + aim * 20.;
            draw_line(c.x, c.y - 1., tip.x, tip.y - 1., 5., DARK);
            draw_line(c.x, c.y - 1., tip.x, tip.y - 1., 2.5, lighten(color, 0.35));
            dot(c.x, c.y - 1., 8., darken(color, 0.55));
            dot(c.x, c.y - 1., 6.5, lighten(color, 0.1));
            dot(c.x - 2., c.y - 3., 2., Color::new(1., 1., 1., 0.55));
            let b = c + vec2(-18., -18.);
            dot(b.x, b.y, 7.5, darken(color, 0.25));
            mid(&(i + 1).to_string(), b.x, b.y, 10., DARK, TextAlign::Center);
            // Active skills and the rescued operator ride as badges.
            if unit.braced {
                draw_circle_lines(c.x, c.y, 21., 2.5, with_alpha(GOLD, 0.9));
            }
            if unit.braced || unit.focused {
                icon_badge(role_icon(unit.role), c.x + 17., c.y - 17., 16., color, DARK);
            }
            if self.carrier == Some(i) {
                icon_badge(Icon::Person, c.x + 17., c.y + 4., 16., PAPER, DARK);
            }
            // Hull strip; announced fire blinks off its end.
            let max = f32::from(self.max_hull(unit.role));
            let frac = (f32::from(unit.hp) / max).min(1.);
            panel(c.x - 9., c.y + 20., 27., 3., Color::new(0., 0., 0., 0.6));
            panel(c.x - 9., c.y + 20., 27. * frac, 3., color);
            let lose = f.incoming[i];
            if lose > 0 && self.phase == Phase::Battle {
                let after = f32::from(unit.hp - lose) / max;
                panel(
                    c.x - 9. + 27. * after,
                    c.y + 20.,
                    27. * (frac - after),
                    3.,
                    with_alpha(RED, 0.45 + 0.5 * pulse(f.time, 2.)),
                );
            }
        }
    }
    fn draw_tracers(&self) {
        if self.show.flash == 0 {
            return;
        }
        let k = f32::from(self.show.flash) / f32::from(self.show.flash_from.max(1));
        for (a, b, hostile) in self.show.tracers.iter().flatten() {
            let (a, b) = (a.center(), b.center());
            let color = if *hostile { RED } else { PAPER };
            draw_line(a.x, a.y, b.x, b.y, 6., with_alpha(color, 0.25 * k));
            draw_line(a.x, a.y, b.x, b.y, 2., with_alpha(color, k.min(1.)));
            glow(
                b.x,
                b.y,
                22.,
                with_alpha(if *hostile { RED } else { GOLD }, 0.8 * k),
            );
        }
    }
    fn draw_cursor(&self, f: &Frame) {
        let color = lighten(role_color(self.units[self.selected].role), 0.2);
        let c = self.cursor.center();
        let breathe = 1.5 * pulse(f.time, 1.);
        panel(c.x - 23., c.y - 23., 46., 46., with_alpha(color, 0.07));
        brackets(c, 24. + breathe, 10., 2., color);
        if f.hovering && f.focus != self.cursor {
            let (px, py) = tile_rect(f.focus);
            slab(px + 2., py + 2., 46., 46., 5., Color::new(1., 1., 1., 0.06));
            outline(px + 2., py + 2., 46., 46., 5., 1.5, with_alpha(PAPER, 0.45));
        }
    }
    /// Cost and outcome bubbles over the previewed tile.
    fn draw_preview_labels(&self, f: &Frame) {
        let color = role_color(self.units[self.selected].role);
        match &f.preview {
            Preview::Move { path, refusal } => {
                let c = path.last().copied().unwrap_or(f.focus).center();
                let tone = if refusal.is_some() { RED } else { color };
                bubble(
                    c.x,
                    c.y - 31.,
                    Some(Icon::Bolt),
                    &(path.len() - 1).to_string(),
                    tone,
                );
            }
            Preview::Fire {
                enemy,
                damage,
                refusal,
                ..
            } => {
                let c = self.enemy_center(*enemy);
                match refusal {
                    Some(why) => {
                        icon_badge(why.badge().0, c.x, c.y - 33., 24., RED, PAPER);
                    }
                    None => {
                        bubble(c.x + 26., c.y - 26., None, &format!("-{damage}"), GOLD);
                    }
                }
            }
            Preview::Interact { index, refusal } => {
                if let Some(p) = self.objectives[*index] {
                    let c = p.center();
                    let tone = if refusal.is_some() { RED } else { TEAL };
                    bubble(c.x, c.y - 32., Some(Icon::Bolt), "2", tone);
                }
            }
            Preview::Select(i) => {
                let c = self.unit_center(*i);
                let tone = role_color(self.units[*i].role);
                shapes::stroke_circle(c, 25., 2., with_alpha(tone, 0.9));
                icon_badge(Icon::Hand, c.x, c.y - 34., 22., tone, DARK);
            }
            Preview::Blocked if f.hovering => {
                let c = f.focus.center();
                icon(Icon::Cross, c.x, c.y, 18., with_alpha(RED, 0.8));
            }
            _ => {}
        }
        // The engineer's repair preview, while its button is hovered.
        if matches!(f.hit, Some(Hit::Skill | Hit::Patch(_)))
            && self.units[self.selected].role == Role::Engineer
        {
            let c = self.unit_center(f.skill.patient);
            let tone = if f.skill.refusal.is_some() {
                RED
            } else {
                GREEN
            };
            shapes::stroke_circle(c, 25., 2.5, with_alpha(tone, 0.9));
            match f.skill.refusal {
                Some(why) => {
                    icon_badge(why.badge().0, c.x, c.y - 34., 22., RED, PAPER);
                }
                None => {
                    bubble(
                        c.x,
                        c.y - 33.,
                        Some(Icon::Heart),
                        &format!("+{}", f.skill.amount),
                        GREEN,
                    );
                }
            }
        }
    }

    // --- Sidebar. ---

    fn draw_sidebar(&self, f: &Frame) {
        for i in 0..3 {
            self.draw_card(i, f);
        }
        match self.phase {
            Phase::Battle => self.draw_orders(f),
            Phase::Briefing => self.draw_briefing(f),
            Phase::Debrief => self.draw_next(),
            Phase::Setback => self.draw_setback_buttons(f),
            Phase::Won | Phase::Lost => {}
        }
    }
    fn draw_card(&self, i: usize, f: &Frame) {
        let unit = self.units[i];
        let r = card_area(i);
        let alive = unit.hp > 0;
        let color = role_color(unit.role);
        let battle = self.phase == Phase::Battle;
        let selected = i == self.selected && matches!(self.phase, Phase::Battle | Phase::Briefing);
        let hover = battle && alive && f.hit == Some(Hit::Card(i));
        let y = r.y - if hover && !selected { 2. } else { 0. };
        let right = r.x + r.w;
        if selected {
            slab(
                r.x - 3.,
                y - 3.,
                r.w + 6.,
                r.h + 6.,
                13.,
                with_alpha(color, 0.12 + 0.05 * pulse(f.time, 0.6)),
            );
            framed(
                Rect::new(r.x, y, r.w, r.h),
                10.,
                1.5,
                darken(color, 0.1),
                Color::new(0.13, 0.17, 0.21, 1.),
            );
        } else {
            let fill = if hover {
                lighten(CARD_FILL, 0.05)
            } else {
                CARD_FILL
            };
            slab(r.x, y, r.w, r.h, 10., fill);
            if hover {
                outline(r.x, y, r.w, r.h, 10., 1., with_alpha(color, 0.6));
            }
        }
        // Role badge: the machine's skill. It lights up while the skill is on.
        let b = vec2(r.x + 38., y + r.h * 0.5);
        if !alive {
            dot(b.x, b.y, 25., Color::new(0.15, 0.17, 0.19, 1.));
            icon(Icon::Skull, b.x, b.y, 24., MUTED);
        } else if unit.braced || unit.focused {
            glow(b.x, b.y, 34., with_alpha(color, 0.35));
            dot(b.x, b.y, 25., color);
            icon(role_icon(unit.role), b.x, b.y, 26., DARK);
        } else {
            dot(b.x, b.y, 25., tint(color, 0.22));
            shapes::stroke_circle(b, 25., 1.5, with_alpha(color, 0.55));
            icon(role_icon(unit.role), b.x, b.y, 26., lighten(color, 0.1));
        }
        if self.carrier == Some(i) && alive {
            icon_badge(Icon::Person, b.x + 19., b.y - 19., 18., PAPER, DARK);
        }
        let x0 = r.x + 74.;
        // Row 1: key cap, call sign and action points.
        let row1 = y + 20.;
        let kw = key(&(i + 1).to_string(), "", x0, row1 - 10., PAPER);
        mid(
            call_sign(unit.role),
            x0 + kw + 8.,
            row1,
            18.,
            if alive { lighten(color, 0.1) } else { MUTED },
            TextAlign::Left,
        );
        if alive {
            let max = self.max_ap(unit.role);
            let spend = if selected { f.cost.min(unit.ap) } else { 0 };
            let blink = 0.35 + 0.65 * pulse(f.time, 2.5);
            let mut px = right - 12. - f32::from(max) * 12.;
            icon(Icon::Bolt, px - 9., row1, 14., with_alpha(color, 0.9));
            for k in 0..max {
                let lit = k < unit.ap;
                let spent = lit && k >= unit.ap - spend;
                let tone = if spent {
                    with_alpha(PAPER, blink)
                } else if lit {
                    color
                } else {
                    Color::new(1., 1., 1., 0.1)
                };
                slab(px + 2., row1 - 6., 9., 12., 2., tone);
                if lit && !spent {
                    panel(px + 3., row1 - 6., 7., 1., Color::new(1., 1., 1., 0.35));
                }
                px += 12.;
            }
        }
        // Row 2: hull meter and value; announced fire blinks red, a hovered
        // repair or armor upgrade shows green.
        let row2 = y + 44.;
        let max = self.max_hull(unit.role);
        let low = u32::from(unit.hp) * 3 <= u32::from(max);
        icon(
            Icon::Heart,
            x0 + 7.,
            row2,
            14.,
            if !alive {
                MUTED
            } else if low {
                RED
            } else {
                lighten(color, 0.1)
            },
        );
        let meter_x = x0 + 20.;
        let meter_w = right - 52. - meter_x;
        let lose = if battle { f.incoming[i] } else { 0 };
        let repair = battle
            && matches!(f.hit, Some(Hit::Skill | Hit::Patch(_)))
            && self.units[self.selected].role == Role::Engineer
            && f.skill.patient == i
            && f.skill.refusal.is_none();
        // During the debrief, hovering armor previews the bigger hull.
        let armor = self.phase == Phase::Debrief
            && f.hit == Some(Hit::Upgrade(0))
            && self.upgrades[0] < 2
            && alive;
        let (shown_max, gain) = if armor {
            (max + 3, 3)
        } else if repair {
            (max, f.skill.amount)
        } else {
            (max, 0)
        };
        hull_meter(
            meter_x,
            row2 - 4.,
            meter_w,
            8.,
            (unit.hp, shown_max),
            if low { RED } else { color },
            (lose, gain),
            f.time,
        );
        mid(
            &unit.hp.to_string(),
            right - 12.,
            row2,
            16.,
            if alive { PAPER } else { MUTED },
            TextAlign::Right,
        );
        if !alive {
            return;
        }
        // Row 3: range, cannon damage, the engineer's kits, incoming fire.
        let row3 = y + 66.;
        let mut x = x0;
        x += icon_value(
            Icon::Target,
            &unit.role.range().to_string(),
            x,
            row3,
            13.,
            SOFT,
        ) + 14.;
        let cannon =
            self.phase == Phase::Debrief && f.hit == Some(Hit::Upgrade(1)) && self.upgrades[1] < 2;
        let damage = unit.role.damage()
            + self.upgrades[1]
            + if unit.focused { 2 } else { 0 }
            + u8::from(cannon);
        x += icon_value(
            Icon::Bomb,
            &damage.to_string(),
            x,
            row3,
            13.,
            if unit.focused || cannon {
                lighten(TEAL, 0.2)
            } else {
                SOFT
            },
        ) + 14.;
        if unit.role == Role::Engineer {
            icon(Icon::Wrench, x + 6., row3, 13., SOFT);
            for k in 0..2u8 {
                let lit = k < self.kits;
                dot(
                    x + 20. + f32::from(k) * 11.,
                    row3,
                    4.,
                    if lit {
                        GREEN
                    } else {
                        Color::new(1., 1., 1., 0.12)
                    },
                );
            }
        }
        if lose > 0 {
            let a = 0.6 + 0.4 * pulse(f.time, 2.);
            let w = mid(
                &format!("-{lose}"),
                right - 12.,
                row3,
                14.,
                with_alpha(RED, a),
                TextAlign::Right,
            );
            icon(Icon::Alert, right - 22. - w, row3, 14., with_alpha(RED, a));
        }
        if self.phase == Phase::Debrief && f.hit == Some(Hit::Upgrade(2)) && self.upgrades[2] < 2 {
            bubble(right - 30., row1 - 22., Some(Icon::Bolt), "+1", GOLD);
        }
    }
    /// A sidebar order row: background, key cap, icon and word. Returns the
    /// row's vertical center.
    #[allow(clippy::too_many_arguments)]
    fn order_row(
        &self,
        row: usize,
        cap: &str,
        glyph: Icon,
        word: &str,
        accent: Color,
        enabled: bool,
        hover: bool,
    ) -> f32 {
        let r = order_area(row);
        let lift = if hover && enabled { 2. } else { 0. };
        let y = r.y - lift;
        let fill = if row == 2 {
            Color::new(0.2, 0.16, 0.09, 1.)
        } else {
            ROW_FILL
        };
        if hover && enabled {
            slab(
                r.x - 2.,
                y - 2.,
                r.w + 4.,
                r.h + 4.,
                11.,
                with_alpha(accent, 0.2),
            );
        }
        slab(
            r.x,
            y,
            r.w,
            r.h,
            9.,
            if hover { lighten(fill, 0.05) } else { fill },
        );
        if hover && enabled {
            outline(r.x, y, r.w, r.h, 9., 1., with_alpha(accent, 0.7));
        }
        let a = if enabled { 1. } else { 0.42 };
        panel(r.x, y + 10., 3., r.h - 20., with_alpha(accent, a));
        let cy = y + r.h * 0.5;
        key(cap, "", r.x + 12., cy - 10., PAPER);
        icon(
            glyph,
            r.x + 80.,
            cy,
            20.,
            with_alpha(lighten(accent, 0.1), a),
        );
        mid(
            word,
            r.x + 96.,
            cy,
            16.,
            with_alpha(if row == 2 { GOLD } else { PAPER }, a),
            TextAlign::Left,
        );
        cy
    }
    fn draw_orders(&self, f: &Frame) {
        let u = self.units[self.selected];
        let color = role_color(u.role);
        let right = SIDE_X + SIDE_W - 12.;
        // Row 1: the contextual order at the cursor, exactly what Space does.
        let hover = f.hit == Some(Hit::Order);
        let objective = self.content().objective;
        let (glyph, word, ok) = match &f.order {
            Preview::Fire { refusal, .. } => (Icon::Bomb, "Fire", refusal.is_none()),
            Preview::Select(_) => (Icon::Hand, "Select", true),
            Preview::Interact { index, refusal } => (
                objective_icon(objective, *index),
                interact_verb(objective),
                refusal.is_none(),
            ),
            Preview::Move { refusal, .. } => (Icon::Pin, "Move", refusal.is_none()),
            Preview::Stay | Preview::Blocked => (Icon::Pin, "Move", false),
        };
        let cy = self.order_row(0, "Space", glyph, word, color, ok, hover);
        match &f.order {
            Preview::Fire {
                damage,
                lethal,
                refusal,
                ..
            } => match refusal {
                Some(why) => {
                    pill_value(right, cy, Some(why.badge().0), "", RED);
                }
                None => {
                    let x = pill_value(
                        right,
                        cy,
                        lethal.then_some(Icon::Skull),
                        &format!("-{damage}"),
                        GOLD,
                    );
                    cost_pips(x - 8., cy, 2, color);
                }
            },
            Preview::Select(i) => {
                let tone = role_color(self.units[*i].role);
                icon_badge(
                    role_icon(self.units[*i].role),
                    right - 12.,
                    cy,
                    24.,
                    tone,
                    DARK,
                );
            }
            Preview::Interact { refusal, .. } => {
                cost_pips(right, cy, 2, if refusal.is_some() { RED } else { color });
            }
            Preview::Move { path, refusal } => {
                let cost = (path.len() - 1).min(usize::from(u8::MAX)) as u8;
                cost_pips(
                    right,
                    cy,
                    cost.min(9),
                    if refusal.is_some() { RED } else { color },
                );
            }
            Preview::Blocked => {
                icon(Icon::Cross, right - 9., cy, 16., with_alpha(RED, 0.7));
            }
            Preview::Stay => {}
        }
        // Row 2: the selected machine's skill.
        let hover = matches!(f.hit, Some(Hit::Skill | Hit::Patch(_)));
        let skill = self.skill_preview(None);
        let (word, cost) = match u.role {
            Role::Vanguard => ("Brace", 1),
            Role::Scout => ("Focus", 1),
            Role::Engineer => ("Repair", 2),
        };
        let usable = skill.refusal.is_none();
        let cy = self.order_row(1, "X", role_icon(u.role), word, color, usable, hover);
        match u.role {
            Role::Vanguard | Role::Scout => {
                let active = skill.refusal == Some(crate::fx::Refusal::Ready);
                let (glyph, value) = if u.role == Role::Vanguard {
                    (Icon::Shield, "-2")
                } else {
                    (Icon::Bomb, "+2")
                };
                let x = if active {
                    pill_value(right, cy, Some(Icon::Check), "", color)
                } else {
                    pill_value(right, cy, Some(glyph), value, color)
                };
                if !active {
                    cost_pips(
                        x - 8.,
                        cy,
                        cost,
                        if usable { color } else { with_alpha(RED, 0.8) },
                    );
                }
            }
            Role::Engineer => {
                cost_pips(
                    patch_area(0).x - 6.,
                    cy,
                    cost,
                    if u.ap >= 2 {
                        color
                    } else {
                        with_alpha(RED, 0.8)
                    },
                );
                // One button per patient; the X key's current patient is ringed.
                for k in 0..3 {
                    let p = patch_area(k);
                    let c = vec2(p.x + p.w * 0.5, cy);
                    let patient = self.units[k];
                    let check = self.skill_preview(Some(k));
                    let live = patient.hp > 0;
                    let ok = live && check.refusal.is_none();
                    let tone = role_color(patient.role);
                    let hovered = f.hit == Some(Hit::Patch(k));
                    if hovered {
                        glow(
                            c.x,
                            c.y,
                            26.,
                            with_alpha(if ok { GREEN } else { RED }, 0.35),
                        );
                    }
                    dot(
                        c.x,
                        c.y,
                        16.,
                        if ok {
                            tint(tone, 0.3)
                        } else {
                            Color::new(0.11, 0.13, 0.16, 1.)
                        },
                    );
                    if hovered {
                        shapes::stroke_circle(c, 17., 2., if ok { GREEN } else { RED });
                    } else if skill.patient == k && live {
                        shapes::stroke_circle(c, 16., 1.5, with_alpha(lighten(tone, 0.3), 0.9));
                    }
                    icon(
                        if live {
                            role_icon(patient.role)
                        } else {
                            Icon::Skull
                        },
                        c.x,
                        c.y,
                        17.,
                        if ok { lighten(tone, 0.2) } else { MUTED },
                    );
                    if ok {
                        icon_badge(Icon::Plus, c.x + 11., c.y - 11., 12., GREEN, DARK);
                    }
                }
            }
        }
        // Row 3: end the squad turn; the cost is the announced enemy fire.
        let hover = f.hit == Some(Hit::EndTurn);
        let cy = self.order_row(2, "C", Icon::Hourglass, "End turn", GOLD, true, hover);
        let total: u32 = f.incoming.iter().map(|d| u32::from(*d)).sum();
        let lethal = (0..3).any(|i| self.units[i].hp > 0 && f.incoming[i] >= self.units[i].hp);
        if total > 0 {
            pill_value(
                right,
                cy,
                Some(if lethal { Icon::Skull } else { Icon::Heart }),
                &format!("-{total}"),
                RED,
            );
        } else {
            pill_value(right, cy, Some(Icon::Check), "", TEAL);
        }
    }
    /// Before deploying: the operation's name, its stakes and a Deploy button.
    fn draw_briefing(&self, f: &Frame) {
        let top = order_area(0).y;
        let bottom = order_area(2).y + order_area(2).h;
        slab(SIDE_X, top, SIDE_W, bottom - top, 10., CARD_FILL);
        let x = SIDE_X + 16.;
        label(self.content().name, x, top + 30., 20., PAPER);
        let cy = top + 54.;
        let mut sx = x;
        sx += icon_value(
            Icon::Hourglass,
            &self.content().limit.to_string(),
            sx,
            cy,
            15.,
            SOFT,
        ) + 18.;
        token(Kind::Gunner, sx + 7., cy, 7., RED);
        sx += 18.;
        sx += mid(
            &self.remaining().to_string(),
            sx,
            cy,
            15.,
            SOFT,
            TextAlign::Left,
        ) + 18.;
        icon_value(Icon::Wrench, &self.kits.to_string(), sx, cy, 15., SOFT);
        // Deploy: the whole screen accepts the click, this is where to look.
        let button = Rect::new(SIDE_X + 12., bottom - 62., SIDE_W - 24., 50.);
        let hover = button.contains(pointer());
        let beat = pulse(f.time, 0.8);
        glow(
            button.center().x,
            button.center().y,
            120.,
            with_alpha(GOLD, 0.06 + 0.06 * beat),
        );
        rounded_gradient(
            button.x,
            button.y,
            button.w,
            button.h,
            10.,
            lighten(GOLD, if hover { 0.18 } else { 0.08 }),
            darken(GOLD, 0.12),
        );
        let c = button.center();
        icon(Icon::Play, button.x + 26., c.y, 22., INK);
        mid("Deploy", button.x + 46., c.y, 20., INK, TextAlign::Left);
        key("Space", "", button.x + button.w - 62., c.y - 10., PAPER);
    }
    /// After a victory: the next operation's name and goal.
    fn draw_next(&self) {
        let top = order_area(0).y;
        let bottom = order_area(2).y + order_area(2).h;
        slab(SIDE_X, top, SIDE_W, bottom - top, 10., CARD_FILL);
        let Some(next) = MISSIONS.get(self.mission_index + 1) else {
            return;
        };
        let x = SIDE_X + 16.;
        icon(Icon::ArrowRight, x + 8., top + 24., 16., MUTED);
        mid(
            &format!("{} / {}", self.mission() + 1, MISSIONS.len()),
            x + 24.,
            top + 24.,
            14.,
            MUTED,
            TextAlign::Left,
        );
        label(next.name, x, top + 68., 20., PAPER);
        let cy = top + 102.;
        let w = chip(objective_verb(next.objective), x, cy - 9.4, 13., GOLD);
        if next.objective != Objective::Eliminate {
            step_marker(
                objective_icon(next.objective, 0),
                x + w + 26.,
                cy,
                Step::Later,
                0.,
            );
        }
        let limit = next.limit.to_string();
        let lw = TextStyle::modern(15., SOFT);
        let right = SIDE_X + SIDE_W - 16.;
        let tw = text_width(&limit, lw);
        icon_value(Icon::Hourglass, &limit, right - tw - 19., cy, 15., SOFT);
    }
    /// After a setback: retry (keeping upgrades) or concede.
    fn draw_setback_buttons(&self, f: &Frame) {
        let retry_hover = f.hit == Some(Hit::Retry);
        let r = RETRY;
        if retry_hover {
            slab(
                r.x - 3.,
                r.y - 3.,
                r.w + 6.,
                r.h + 6.,
                13.,
                with_alpha(TEAL, 0.25),
            );
        }
        rounded_gradient(
            r.x,
            r.y,
            r.w,
            r.h,
            10.,
            lighten(TEAL, if retry_hover { 0.16 } else { 0.06 }),
            darken(TEAL, 0.18),
        );
        let c = r.center();
        icon(Icon::Restart, r.x + 28., c.y, 24., INK);
        mid("Retry", r.x + 50., c.y, 20., INK, TextAlign::Left);
        let kw = key("Space", "", r.x + 128., c.y - 10., PAPER);
        let _ = kw;
        let lost = self.score.saturating_sub(self.checkpoint);
        if lost > 0 {
            let value = format!("-{lost}");
            let s = TextStyle::modern(16., with_alpha(INK, 0.75));
            let w = text_width(&value, s);
            draw_text_styled(&value, r.x + r.w - 16. - w, s.centered_baseline(c.y), s);
            icon(
                Icon::Star,
                r.x + r.w - 30. - w,
                c.y,
                16.,
                with_alpha(INK, 0.75),
            );
        }
        let concede_hover = f.hit == Some(Hit::Concede);
        let r = CONCEDE;
        slab(
            r.x,
            r.y,
            r.w,
            r.h,
            10.,
            if concede_hover {
                lighten(CARD_FILL, 0.06)
            } else {
                CARD_FILL
            },
        );
        if concede_hover {
            outline(r.x, r.y, r.w, r.h, 10., 1., with_alpha(RED, 0.6));
        }
        let c = r.center();
        icon(Icon::Flag, r.x + 28., c.y, 20., with_alpha(RED, 0.85));
        mid("Concede", r.x + 50., c.y, 16., SOFT, TextAlign::Left);
        key("C", "", r.x + r.w - 36., c.y - 10., PAPER);
    }

    // --- Operation cards over the board. ---

    fn draw_debrief(&self, f: &Frame, dy: f32) {
        let top = SHEET.y + dy;
        card(
            SHEET.x,
            top,
            SHEET.w,
            SHEET.h,
            Color::new(0.075, 0.1, 0.13, 1.),
        );
        stripe(TEAL, top);
        let x = SHEET.x + 24.;
        eyebrow("SECURED", x, top + 36., TEAL);
        label(self.content().name, x, top + 72., 26., PAPER);
        // Medals, then what earned them: survivors and speed.
        let medals = self.medals[self.mission_index];
        for k in 0..3u8 {
            let c = medal_center(k) + vec2(0., dy);
            let lit = k < medals;
            if lit {
                glow(
                    c.x,
                    c.y,
                    26.,
                    with_alpha(GOLD, 0.18 + 0.08 * pulse(f.time, 0.7)),
                );
            }
            icon(
                Icon::Star,
                c.x,
                c.y,
                36.,
                if lit {
                    GOLD
                } else {
                    Color::new(1., 1., 1., 0.12)
                },
            );
        }
        let mut fx = medal_center(2).x + 52.;
        for unit in &self.units {
            let tone = role_color(unit.role);
            let live = unit.hp > 0;
            dot(
                fx,
                MEDAL_Y + dy,
                14.,
                if live {
                    tint(tone, 0.3)
                } else {
                    Color::new(0.12, 0.13, 0.15, 1.)
                },
            );
            icon(
                if live {
                    role_icon(unit.role)
                } else {
                    Icon::Skull
                },
                fx,
                MEDAL_Y + dy,
                15.,
                if live { lighten(tone, 0.2) } else { MUTED },
            );
            fx += 32.;
        }
        let fast = usize::from(self.round) <= usize::from(self.content().limit) / 2;
        icon_value(
            Icon::Hourglass,
            &self.round.to_string(),
            fx + 4.,
            MEDAL_Y + dy,
            17.,
            if fast { TEAL } else { SOFT },
        );
        divider(x, top + 172., SHEET.w - 48., Color::new(1., 1., 1., 0.1));
        // One permanent upgrade: armor, cannon or drive, two ranks each.
        let hovered = match f.hit {
            Some(Hit::Upgrade(i)) => Some(i),
            _ => None,
        };
        for (i, gain) in UPGRADE_GAIN.iter().enumerate() {
            let r = upgrade_area(i);
            let open = self.upgrades[i] < 2;
            let hover = hovered == Some(i) && open;
            let y = r.y + dy - if hover { 4. } else { 0. };
            let accent = if open { GOLD } else { MUTED };
            if hover {
                glow(r.center().x, y + r.h * 0.5, 110., with_alpha(GOLD, 0.16));
            }
            framed(
                Rect::new(r.x, y, r.w, r.h),
                12.,
                if hover { 2. } else { 1. },
                if hover {
                    GOLD
                } else {
                    with_alpha(accent, 0.45)
                },
                if hover {
                    Color::new(0.15, 0.19, 0.23, 1.)
                } else {
                    Color::new(0.11, 0.145, 0.18, 1.)
                },
            );
            key(&(i + 1).to_string(), "", r.x + 12., y + 12., PAPER);
            for rank in 0..2u8 {
                slab(
                    r.x + r.w - 46. + f32::from(rank) * 18.,
                    y + 19.,
                    14.,
                    6.,
                    3.,
                    if rank < self.upgrades[i] {
                        accent
                    } else {
                        Color::new(1., 1., 1., 0.14)
                    },
                );
            }
            let c = vec2(r.center().x, y + 76.);
            if open {
                glow(
                    c.x,
                    c.y,
                    40.,
                    with_alpha(GOLD, if hover { 0.3 } else { 0.12 }),
                );
            }
            icon(
                upgrade_icon(i),
                c.x,
                c.y,
                if hover { 50. } else { 44. },
                if open { lighten(GOLD, 0.1) } else { MUTED },
            );
            if open {
                mid(gain, c.x, y + 136., 30., PAPER, TextAlign::Center);
            } else {
                icon_badge(Icon::Check, c.x, y + 136., 30., MUTED, DARK);
            }
        }
    }
    fn draw_setback(&self, f: &Frame, dy: f32) {
        let top = SHEET.y + dy;
        card(
            SHEET.x,
            top,
            SHEET.w,
            SHEET.h,
            Color::new(0.085, 0.08, 0.1, 1.),
        );
        stripe(RED, top);
        let cx = SHEET.x + SHEET.w * 0.5;
        let c = vec2(cx, top + 120.);
        let (glyph, headline) = match self.failure() {
            Failure::SquadDown => (Icon::Skull, "Squad down"),
            Failure::OperatorLost => (Icon::Person, "Operator lost"),
            Failure::OutOfTime => (Icon::Hourglass, "Out of time"),
        };
        glow(
            c.x,
            c.y,
            80.,
            with_alpha(RED, 0.16 + 0.06 * pulse(f.time, 0.6)),
        );
        dot(c.x, c.y, 50., Color::new(0.3, 0.09, 0.09, 1.));
        shapes::stroke_circle(c, 50., 2., with_alpha(RED, 0.8));
        icon(glyph, c.x, c.y, 56., lighten(RED, 0.15));
        mid(
            self.content().name,
            cx,
            top + 210.,
            26.,
            PAPER,
            TextAlign::Center,
        );
        mid(headline, cx, top + 244., 18., RED, TextAlign::Center);
        // Retries: the one a retry would spend blinks.
        let total = 3;
        let w = f32::from(total) * 26.;
        let left = cx - (w + 30.) * 0.5;
        let y = top + 304.;
        icon(Icon::Restart, left + 10., y, 20., SOFT);
        for k in 0..total {
            let px = left + 38. + f32::from(k) * 26.;
            let lit = k < self.retries;
            let next = lit && k + 1 == self.retries;
            let tone = if next {
                with_alpha(TEAL, 0.4 + 0.6 * pulse(f.time, 1.5))
            } else if lit {
                TEAL
            } else {
                Color::new(1., 1., 1., 0.12)
            };
            dot(px, y, 8., tone);
        }
    }

    // --- Header objective and results. ---

    fn draw_header_objective(&self, area: Rect) -> bool {
        let t = clock();
        let cy = area.y + area.h * 0.5;
        let objective = self.content().objective;
        let mut x = area.x + 2.;
        x += chip(objective_verb(objective), x, cy - 10.15, 14., GOLD) + 22.;
        let state = |index: usize, ready: bool| {
            if self.completed[index] {
                Step::Done
            } else if ready {
                Step::Now
            } else {
                Step::Later
            }
        };
        let arrow = |x: f32| {
            icon(Icon::ArrowRight, x + 2., cy, 12., MUTED);
            x + 20.
        };
        match objective {
            Objective::Eliminate => {
                self.enemy_tokens(x - 8., cy);
            }
            Objective::Rescue => {
                step_marker(Icon::Person, x + 4., cy, state(0, true), t);
                x = arrow(x + 24.);
                let step = if self.completed[0] {
                    Step::Now
                } else {
                    Step::Later
                };
                step_marker(Icon::Flag, x + 13., cy, step, t);
                if let Some(i) = self.carrier {
                    let tone = role_color(self.units[i].role);
                    icon_badge(role_icon(self.units[i].role), x + 46., cy, 22., tone, DARK);
                }
            }
            Objective::Capture | Objective::Hold => {
                step_marker(Icon::Bell, x + 4., cy, state(0, true), t);
                x = arrow(x + 24.);
                let (held, needed) = self.guard().unwrap_or((0, 0));
                icon(
                    Icon::Shield,
                    x + 7.,
                    cy,
                    15.,
                    if self.completed[0] { TEAL } else { MUTED },
                );
                for k in 0..needed {
                    let lit = k < held;
                    dot(
                        x + 26. + f32::from(k) * 14.,
                        cy,
                        5.,
                        if lit {
                            TEAL
                        } else {
                            Color::new(1., 1., 1., 0.14)
                        },
                    );
                }
                x += 26. + f32::from(needed) * 14.;
                if objective == Objective::Hold {
                    x = arrow(x);
                    let ready = self.round >= HOLD_UNTIL;
                    icon_value(
                        Icon::Hourglass,
                        &HOLD_UNTIL.to_string(),
                        x,
                        cy,
                        15.,
                        if ready { TEAL } else { SOFT },
                    );
                }
            }
            Objective::Sabotage => {
                step_marker(Icon::Flame, x + 4., cy, state(0, true), t);
                step_marker(Icon::Flame, x + 36., cy, state(1, true), t);
                icon(Icon::Plus, x + 62., cy, 12., MUTED);
                self.enemy_tokens(x + 72., cy);
            }
            Objective::Liberate => {
                let end = self.enemy_tokens(x - 8., cy);
                icon(Icon::Plus, end + 8., cy, 12., MUTED);
                step_marker(Icon::Gear, end + 34., cy, state(0, true), t);
            }
        }
        self.draw_turn_track(area, cy, t);
        true
    }
    /// One mini chassis per enemy: red while active, hollow in reserve,
    /// crossed out once disabled. Returns the right edge.
    fn enemy_tokens(&self, x: f32, cy: f32) -> f32 {
        let mut x = x;
        for e in self.enemies.iter().flatten() {
            x += 21.;
            let r = 7.5;
            if e.hp == 0 {
                token(e.kind, x, cy, r, Color::new(0.2, 0.22, 0.25, 1.));
                draw_line(x - 6., cy - 6., x + 6., cy + 6., 2., MUTED);
                draw_line(x + 6., cy - 6., x - 6., cy + 6., 2., MUTED);
            } else if !e.active {
                token(e.kind, x, cy, r, with_alpha(RED, 0.8));
                token(e.kind, x, cy, r - 2., tint(RED, 0.08));
            } else {
                token(e.kind, x, cy, r, RED);
                token(e.kind, x, cy - 1., r * 0.55, lighten(RED, 0.25));
            }
        }
        x + 12.
    }
    /// Turn number and the deadline as a segmented track (reserve arrivals in
    /// red, the hold's last turn in teal), then the campaign's retries.
    fn draw_turn_track(&self, area: Rect, cy: f32, t: f32) {
        let right = area.x + area.w - 4.;
        // Retries.
        let pips_x = right - 34.;
        for k in 0..3u8 {
            let lit = k < self.retries;
            dot(
                pips_x + 6. + f32::from(k) * 12.,
                cy,
                4.5,
                if lit {
                    lighten(TEAL, 0.1)
                } else {
                    Color::new(1., 1., 1., 0.12)
                },
            );
        }
        icon(Icon::Restart, pips_x - 10., cy, 16., SOFT);
        // Deadline track.
        let limit = self.content().limit;
        let round = self.round.min(limit);
        let late = self.round + 3 > limit;
        let track_right = pips_x - 34.;
        let width = 220.;
        let left = track_right - width;
        let step = width / f32::from(limit);
        let seg = (step - 2.).max(2.);
        let beat = pulse(t, 1.4);
        for k in 1..=limit {
            let sx = left + f32::from(k - 1) * step;
            if k < round {
                slab(sx, cy - 4., seg, 8., 2., with_alpha(GOLD, 0.55));
            } else if k == round {
                let tone = if late { RED } else { lighten(GOLD, 0.2) };
                glow(
                    sx + seg * 0.5,
                    cy,
                    14.,
                    with_alpha(tone, 0.25 + 0.25 * beat),
                );
                slab(sx - 1., cy - 7., seg + 2., 14., 3., tone);
            } else {
                slab(
                    sx,
                    cy - 4.,
                    seg,
                    8.,
                    2.,
                    if late {
                        with_alpha(RED, 0.3)
                    } else {
                        Color::new(1., 1., 1., 0.1)
                    },
                );
            }
        }
        let marker = |turn: u16, color: Color| {
            if turn == 0 || turn > limit {
                return;
            }
            let mx = left + (f32::from(turn) - 0.5) * step;
            draw_triangle(
                vec2(mx, cy - 8.),
                vec2(mx - 4., cy - 14.),
                vec2(mx + 4., cy - 14.),
                color,
            );
        };
        for e in self.enemies.iter().flatten() {
            if !e.active && e.hp > 0 {
                marker(e.arrives, RED);
            }
        }
        if self.content().objective == Objective::Hold {
            marker(HOLD_UNTIL, TEAL);
        }
        let value = self.round.to_string();
        let w = mid(
            &value,
            left - 10.,
            cy,
            17.,
            if late { RED } else { PAPER },
            TextAlign::Right,
        );
        icon(
            Icon::Hourglass,
            left - 22. - w,
            cy,
            16.,
            if late { RED } else { SOFT },
        );
    }
    fn draw_results(&self, area: Rect) -> bool {
        let liberated = self.medals.iter().filter(|m| **m > 0).count();
        let medals: u32 = self.medals.iter().map(|m| u32::from(*m)).sum();
        let col = area.w / 4.;
        let top = area.y + 30.;
        let value_y = area.y + 86.;
        let detail_y = area.y + 116.;
        let column = |i: usize| area.x + col * (i as f32 + 0.5);
        // Operations liberated.
        let c = column(0);
        icon(Icon::Flag, c, top, 34., TEAL);
        mid(
            &liberated.to_string(),
            c,
            value_y,
            30.,
            PAPER,
            TextAlign::Center,
        );
        for k in 0..6 {
            dot(
                c - 40. + k as f32 * 16.,
                detail_y,
                5.,
                if k < liberated {
                    TEAL
                } else {
                    Color::new(1., 1., 1., 0.14)
                },
            );
        }
        // Medals: three per operation.
        let c = column(1);
        icon(Icon::Star, c, top, 34., GOLD);
        mid(
            &medals.to_string(),
            c,
            value_y,
            30.,
            PAPER,
            TextAlign::Center,
        );
        for (op, earned) in self.medals.iter().enumerate() {
            for k in 0..3u8 {
                dot(
                    c - 61. + op as f32 * 22. + f32::from(k) * 6.,
                    detail_y,
                    2.6,
                    if k < *earned {
                        GOLD
                    } else {
                        Color::new(1., 1., 1., 0.14)
                    },
                );
            }
        }
        // Retries kept.
        let c = column(2);
        icon(Icon::Restart, c, top, 34., lighten(TEAL, 0.1));
        mid(
            &self.retries.to_string(),
            c,
            value_y,
            30.,
            PAPER,
            TextAlign::Center,
        );
        for k in 0..3u8 {
            dot(
                c - 16. + f32::from(k) * 16.,
                detail_y,
                5.,
                if k < self.retries {
                    TEAL
                } else {
                    Color::new(1., 1., 1., 0.14)
                },
            );
        }
        // Upgrades: armor, cannon and drive ranks.
        let c = column(3);
        for (i, rank) in self.upgrades.iter().enumerate() {
            let x = c - 40. + i as f32 * 40.;
            icon(upgrade_icon(i), x, top, 26., GOLD);
            for r in 0..2u8 {
                slab(
                    x - 10. + f32::from(r) * 11.,
                    value_y - 4.,
                    9.,
                    8.,
                    2.,
                    if r < *rank {
                        GOLD
                    } else {
                        Color::new(1., 1., 1., 0.14)
                    },
                );
            }
        }
        true
    }
}
/// A hull meter over an opaque track: `hp` of `max`, `lose` blinking red off
/// its end and `gain` glowing green after it.
#[allow(clippy::too_many_arguments)]
fn hull_meter(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    (hp, max): (u8, u8),
    color: Color,
    (lose, gain): (u8, u8),
    t: f32,
) {
    let max = f32::from(max.max(1));
    pill(x, y, w, h, Color::new(0.03, 0.04, 0.055, 1.));
    let now = (f32::from(hp) / max).clamp(0., 1.);
    if now > 0. {
        pill(x, y, (w * now).max(h), h, color);
        panel(
            x + h * 0.5,
            y + 1.,
            (w * now - h).max(0.),
            1.,
            Color::new(1., 1., 1., 0.25),
        );
    }
    if lose > 0 && now > 0. {
        let after = (f32::from(hp.saturating_sub(lose)) / max).clamp(0., 1.);
        panel(
            x + w * after,
            y,
            w * (now - after),
            h,
            with_alpha(RED, 0.35 + 0.55 * pulse(t, 2.)),
        );
    }
    if gain > 0 {
        let to = (f32::from(hp.saturating_add(gain)) / max).min(1.);
        panel(
            x + w * now,
            y,
            w * (to - now),
            h,
            with_alpha(GREEN, 0.35 + 0.45 * pulse(t, 2.)),
        );
    }
}

impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        let frame = self.frame();
        // After an operation ends, its last moments stay on the board, then
        // the debrief or setback card slides up over it.
        let card = match self.phase {
            Phase::Debrief | Phase::Setback => self.show.reveal(),
            _ => None,
        };
        self.draw_board(&frame, card.is_some_and(|t| t >= 1.));
        self.draw_sidebar(&frame);
        if let Some(t) = card {
            let dy = (1. - t) * 48.;
            if self.phase == Phase::Debrief {
                self.draw_debrief(&frame, dy);
            } else {
                self.draw_setback(&frame, dy);
            }
        }
    }
    fn hud(&self) -> Hud {
        let objective = match self.phase {
            Phase::Debrief => "Choose an upgrade",
            Phase::Setback => "Retry the operation",
            Phase::Won => "The coast is free",
            Phase::Lost => "Campaign over",
            Phase::Briefing | Phase::Battle => self.content().story.trim_end_matches('.'),
        };
        let last = self.notes[0]
            .replace("BRASS", "Brass")
            .replace("KESTREL", "Kestrel")
            .replace("MICA", "Mica");
        Hud {
            stage: self.content().name.to_owned(),
            objective: objective.to_owned(),
            status: format!(
                "Turn {}/{} | {} retries | {} medals | {}",
                self.round,
                self.content().limit,
                self.retries,
                self.medals.iter().map(|m| u32::from(*m)).sum::<u32>(),
                last.trim()
            ),
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        self.outcome()
    }
    fn summary(&self) -> String {
        format!(
            "Liberated {} of {} operations with {} medals.",
            self.medals.iter().filter(|m| **m > 0).count(),
            MISSIONS.len(),
            self.medals.iter().map(|m| u32::from(*m)).sum::<u32>(),
        )
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.autoplay(tick)
    }
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.mission() as u32, MISSIONS.len() as u32))
    }
    fn effects(&self, cue: Cue, fx: &mut Juice) {
        self.spawn_effects(cue, fx);
    }
    fn draw_objective(&self, area: Rect) -> bool {
        self.draw_header_objective(area)
    }
    fn draw_result(&self, area: Rect) -> bool {
        self.draw_results(area)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::{Event, REVEAL_TICKS};

    fn run_until(game: &mut Game, tick: &mut u64, found: impl Fn(&Game) -> bool) -> Cue {
        for _ in 0..20_000 {
            let input = game.autoplay(*tick);
            *tick += 1;
            let cue = game.step(&input);
            if found(game) {
                return cue;
            }
        }
        panic!("the legal controller never reached the wanted moment");
    }
    fn fired(game: &Game) -> bool {
        game.show
            .events
            .iter()
            .any(|e| matches!(e, Event::Fired { .. }))
    }
    fn hit(game: &Game) -> bool {
        game.show
            .events
            .iter()
            .any(|e| matches!(e, Event::Hit { .. }))
    }

    #[test]
    fn progress_pips_follow_the_campaign_from_first_to_last_operation() {
        let mut game = Game::new();
        assert_eq!(ArcadeGame::progress(&game), Some((1, 6)));
        let mut tick = 0;
        run_until(&mut game, &mut tick, |g| g.mission() == 2);
        assert_eq!(ArcadeGame::progress(&game), Some((2, 6)));
        run_until(&mut game, &mut tick, |g| g.outcome() != Outcome::Playing);
        assert_eq!(ArcadeGame::outcome(&game), Outcome::Won);
        assert_eq!(ArcadeGame::progress(&game), Some((6, 6)));
        assert_eq!(
            ArcadeGame::summary(&game),
            "Liberated 6 of 6 operations with 18 medals."
        );
    }

    #[test]
    fn deploys_shots_and_hits_request_effects_where_they_land() {
        let mut game = Game::new();
        let cue = game.step(&Input {
            primary: true,
            ..Input::default()
        });
        let mut fx = Juice::new();
        ArcadeGame::effects(&game, cue, &mut fx);
        let deployed = fx.counts();
        assert!(deployed.rings >= 3 && deployed.popups == 1, "{deployed:?}");
        // Idle ticks request nothing.
        let mut idle = Juice::new();
        let cue = game.step(&Input::default());
        ArcadeGame::effects(&game, cue, &mut idle);
        assert!(idle.is_idle());
        // A landed squad shot sprays sparks and pops its damage.
        let mut tick = 1;
        let cue = run_until(&mut game, &mut tick, fired);
        let mut fx = Juice::new();
        ArcadeGame::effects(&game, cue, &mut fx);
        assert!(fx.counts().sparks >= 16 && fx.counts().popups >= 1);
        // Enemy hits burst on the machine with a hull popup.
        let cue = run_until(&mut game, &mut tick, hit);
        let mut fx = Juice::new();
        ArcadeGame::effects(&game, cue, &mut fx);
        assert!(fx.counts().sparks >= 18 && fx.counts().popups >= 1);
    }

    #[test]
    fn operation_card_waits_for_the_final_hit_then_slides_in_once() {
        let mut game = Game::new();
        let mut tick = 0;
        run_until(&mut game, &mut tick, |g| g.phase == Phase::Debrief);
        assert_eq!(game.show.reveal(), None);
        let mut revealed = 0;
        for _ in 0..REVEAL_TICKS + 30 {
            game.step(&Input::default());
            revealed += game
                .show
                .events
                .iter()
                .filter(|e| *e == Event::Revealed)
                .count();
        }
        assert_eq!(revealed, 1);
        assert_eq!(game.show.reveal(), Some(1.));
        // Choosing the upgrade loads the next board: the finished board's
        // effects are cleared and only the upgrade's own sparkles remain.
        let mut fx = Juice::new();
        fx.burst(vec2(300., 300.), RED, 40);
        fx.callout(vec2(300., 200.), "Turn 9", PAPER);
        let cue = game.step(&Input {
            slot: Some(2),
            ..Input::default()
        });
        ArcadeGame::effects(&game, cue, &mut fx);
        assert_eq!(game.phase, Phase::Briefing);
        assert_eq!(fx.counts().popups, 3, "one +1 per squad card");
        assert!(fx.counts().sparks > 0 && fx.counts().sparks < 40);
    }

    #[test]
    fn header_copy_is_a_name_and_a_short_goal() {
        let mut game = Game::new();
        for mission in MISSIONS.iter() {
            let hud = ArcadeGame::hud(&game);
            assert_eq!(hud.stage, mission.name);
            assert!(
                hud.objective.split_whitespace().count() <= 5,
                "{}",
                hud.objective
            );
            assert!(hud.objective.is_ascii());
            if game.mission() < MISSIONS.len() {
                game.phase = Phase::Debrief;
                game.step(&Input {
                    slot: Some(1 + game.mission() % 3),
                    ..Input::default()
                });
            }
        }
        assert_eq!(game.mission(), MISSIONS.len());
    }

    #[test]
    fn guide_is_four_short_bullets() {
        let help = crate::META.help;
        assert!(!help.is_empty() && help.len() <= 4);
        for bullet in help {
            assert!(bullet.split_whitespace().count() < 15, "{bullet}");
            assert!(bullet.is_ascii());
        }
        assert!(crate::META.subtitle.split_whitespace().count() <= 3);
    }
}
