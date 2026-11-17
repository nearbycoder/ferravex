//! The in-play HUD: a hand-written to-do notepad with a pictogram on every
//! line and satisfying pencil strike-throughs, key-cap prompts with a bobbing
//! marker over their target, speech bubbles over picnickers, icon toasts,
//! screen-space popups, a rotating park minimap and touch buttons. Drawn
//! after the post-processing boundary, so it stays crisp and un-bloomed.
#![allow(clippy::too_many_arguments)]
use crate::fx::{Fx, Toast};
use crate::layout::Layout;
use crate::models::rgb;
use crate::sim::folk::{ALARMED, Bubble, CURIOUS, Kind, Mode};
use crate::sim::park::{PropKind, Role};
use crate::sim::pranks::{Bonus, PRANKS, Prank};
use crate::sim::raccoon::{Hide, Prompt};
use crate::sim::{Game, flat};
use crate::text::{Align, BODY, Face, LARGE, SMALL, Text};
use ferravex_play::tween::{Appearance, Entrance, Motion, RollCounter, Spring};
use ferravex_play::ui::minimap::{MapConfig, Minimap, OffMapPolicy, WorldYAxis};
use ferravex_render::icons::{self, Icon, IconStyle};
use ferravex_render::juice::Juice;
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Cap, Fill, Join, Shadow, Stroke};
use ferravex_render::ui::kit::{self, KitTheme};

pub const PAPER: u32 = 0xfdf6e3;
pub const INK: u32 = 0x3b3445;
pub const PENCIL: u32 = 0x4a4a58;
pub const RED: u32 = 0xe85d4f;
/// Blue pencil: ticks, crossed-off pips and placing things down.
pub const TICK: u32 = 0x3f7fc1;
/// Bottle caps, bonus stars and the finale.
pub const GOLD: u32 = 0xf2b632;
/// Leafy green: hiding, sneaking and fresh burrows.
pub const LEAF: u32 = 0x58a84c;

/// Distance between the collapsed tab's progress pips.
const PIP_STEP: f32 = 11.;

pub fn c(hex: u32, a: f32) -> Color {
    let k = rgb(hex);
    Color::new(k.r, k.g, k.b, a)
}

fn with_alpha(k: Color, a: f32) -> Color {
    Color::new(k.r, k.g, k.b, a)
}

/// Warm picnic kit theme: cream cards, gingham-red headers, cocoa ink.
pub fn theme() -> KitTheme {
    KitTheme {
        backdrop: Color::new(0.12, 0.07, 0.03, 0.5),
        surface: rgb(0xfff4dc),
        raised: rgb(0xfffbf0),
        header: rgb(0xe0594b),
        ink: rgb(0x3b2a22),
        muted: rgb(0x8a7360),
        disabled: rgb(0xbfae98),
        accent: rgb(0xe85d4f),
        accent_ink: rgb(0xfff6e2),
        success: rgb(0x58a84c),
        warning: rgb(0xe0a321),
        danger: rgb(0xd9534f),
        focus: rgb(0x3f7fc1),
        line: Color::new(0.23, 0.16, 0.12, 0.14),
        shadow: Color::new(0.16, 0.08, 0.02, 0.35),
        ..KitTheme::default()
    }
}

/// The pictogram for each to-do line, used everywhere that prank appears
/// (notepad, minimap pins and toasts).
pub fn prank_icon(prank: Prank) -> Icon {
    match prank {
        Prank::Pie => Icon::Bag,
        Prank::Marrow => Icon::Trophy,
        Prank::Music => Icon::Music,
        Prank::Sled => Icon::Mountain,
        Prank::Bell => Icon::Bell,
        Prank::Cushion => Icon::Wind,
        Prank::Melon => Icon::Rocket,
        Prank::Finale => Icon::Bolt,
        Prank::Escape => Icon::Home,
    }
}

/// Coloured-pencil tint for each to-do pictogram.
fn prank_tint(prank: Prank) -> u32 {
    match prank {
        Prank::Pie => 0xc9823a,
        Prank::Marrow => 0xd99a1e,
        Prank::Music => 0x7a5cc4,
        Prank::Sled => 0x4f9a5a,
        Prank::Bell => 0xc98f1b,
        Prank::Cushion => 0xe0607e,
        Prank::Melon => 0x4c9a44,
        Prank::Finale => GOLD,
        Prank::Escape => 0x8a5a3a,
    }
}

pub struct HudState {
    pub notes_open: bool,
    notes: Spring,
    caps: RollCounter,
    prompt_fade: f32,
    hold: f32,
    pub minimap: Option<Minimap>,
    pub age: f32,
    /// Screen-space popups and sparkles (bounded; presentation only).
    pub juice: Juice,
    /// Crossed-off lines and bottle caps already celebrated by the HUD.
    seen: Option<(u16, u32)>,
    /// Fresh changes waiting for a camera to place their effects.
    fresh_lines: u16,
    fresh_caps: u32,
}

impl Default for HudState {
    fn default() -> Self {
        Self::new()
    }
}

/// Bit `i` set when `Prank::ALL[i]` is crossed off.
fn done_mask(game: &Game) -> u16 {
    Prank::ALL.iter().enumerate().fold(
        0,
        |m, (i, p)| if game.todo.done(*p) { m | 1 << i } else { m },
    )
}

impl HudState {
    pub fn new() -> Self {
        Self {
            notes_open: true,
            notes: Spring::critical(1., 9.),
            caps: RollCounter::new(0),
            prompt_fade: 0.,
            hold: 0.,
            minimap: None,
            age: 0.,
            juice: Juice::with_capacity(160, 8, 12),
            seen: None,
            fresh_lines: 0,
            fresh_caps: 0,
        }
    }

    pub fn reset(&mut self, compact: bool) {
        self.notes_open = !compact;
        self.notes.snap(if self.notes_open { 1. } else { 0. });
        self.caps.snap(0);
        self.age = 0.;
        self.juice.clear();
        self.seen = None;
        self.fresh_lines = 0;
        self.fresh_caps = 0;
    }

    pub fn update(&mut self, game: &Game, dt: f32, motion: Motion) {
        self.age += dt;
        self.notes.set_target(if self.notes_open { 1. } else { 0. });
        self.notes.advance(dt, motion);
        self.caps.set_target(i64::from(game.shinies_found()));
        self.caps.advance(dt, motion);
        let want = if game.coon.prompt == Prompt::None {
            0.
        } else {
            1.
        };
        self.prompt_fade += (want - self.prompt_fade) * (1. - (-dt * 12.).exp());
        self.hold = game.coon.using.progress();
        self.juice.set_motion(motion);
        self.juice.update(dt);
        // Celebrate accepted progress only; a continued day starts as seen.
        let now = (done_mask(game), game.shinies_found());
        if let Some((lines, caps)) = self.seen {
            self.fresh_lines |= now.0 & !lines;
            if now.1 > caps {
                self.fresh_caps = now.1;
            }
        }
        self.seen = Some(now);
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w, h)
}

/// Pencil line with a gentle wobble (seeded by its position).
fn pencil(a: Vec2, b: Vec2, progress: f32, width: f32, color: Color, seed: f32) {
    let p = progress.clamp(0., 1.);
    if p <= 0. {
        return;
    }
    let mut pts = [Vec2::ZERO; 9];
    let end = a.lerp(b, p);
    for (k, q) in pts.iter_mut().enumerate() {
        let t = k as f32 / 8.;
        let wob = ((t * 9. + seed).sin() * 1.3 + (t * 23. + seed * 2.).sin() * 0.6)
            * (1. - (t - 0.5).abs());
        *q = a.lerp(end, t) + vec2(0., wob);
    }
    shapes::polyline(
        &pts,
        Stroke::new(width, color).join(Join::Round).cap(Cap::Round),
    );
}

fn time_of_day(seconds: f32) -> (u32, u32, &'static str) {
    // The picnic runs from noon toward tea time: a half hour of play is
    // six afternoon hours.
    let minutes = 12 * 60 + (seconds * 0.2) as u32;
    let h = (minutes / 60) % 24;
    let m = minutes % 60;
    let h12 = if h > 12 { h - 12 } else { h };
    (h12, m, if h >= 12 { "pm" } else { "am" })
}

fn line_for(game: &Game, prank: Prank) -> &'static str {
    match prank {
        Prank::Finale if !game.todo.finale_open() && !game.todo.done(Prank::Finale) => "???",
        _ => prank.line(),
    }
}

/// Width of a kit key cap for `key` (mirrors `kit::keycap`'s measure).
fn cap_width(key: &str, th: &KitTheme) -> f32 {
    (crate::text::width_in(Face::Ink, key, th.body) + kit::SPACE[2] * 2.).max(th.body * 1.5 - 1.)
}

/// A pictogram followed by a short pencilled value; returns the right edge.
fn icon_value(
    tx: &mut Text,
    x: f32,
    cy: f32,
    icon: Icon,
    tint: Color,
    value: &str,
    size: f32,
    ink: Color,
) -> f32 {
    let s = size + 3.;
    icons::draw_styled(icon, vec2(x + s * 0.5, cy), s, IconStyle::new(tint));
    let at = x + s + 4.;
    tx.ink(vec2(at, cy - size * 0.5), 0., value, size, ink, Align::Left);
    at + crate::text::width_in(Face::Ink, value, size)
}

/// One progress pip: a blue-pencil dot once crossed off, a pencil ring before.
fn pip(at: Vec2, done: bool, a: f32) {
    if done {
        shapes::fill_circle(at, 4.6, c(TICK, a));
    } else {
        shapes::stroke_circle(at, 4., 1.5, c(PENCIL, 0.5 * a));
    }
}

/// The collapsed to-do card: progress pips, the afternoon clock and the key
/// (or a tap chevron) that opens the full list.
fn tab(tx: &mut Text, l: &Layout, game: &Game, fx: &Fx, key: Option<&str>, open: f32) {
    let tab = l.notes_tab();
    let pop = 1. + fx.pop * 0.12;
    let r = rect(
        tab.min[0],
        tab.min[1] + (1. - pop) * 8.,
        tab.size[0] * pop.min(1.06),
        tab.size[1],
    );
    shapes::drop_shadow(
        r,
        10.,
        Shadow::soft(c(0x2a1a0a, 0.3)).offset(vec2(0., 3.)).blur(8.),
    );
    shapes::fill_rounded_rect(r, 10., Fill::vertical(r, rgb(0xfffaf0), rgb(PAPER)));
    shapes::stroke_rounded_rect(r, 10., 1., c(INK, 0.15));
    shapes::fill_rounded_rect(rect(r.x + 8., r.y + 8., 6., r.h - 16.), 3., rgb(RED));
    let x0 = r.x + 25.;
    let y = r.y + 14.;
    for (k, prank) in Prank::ALL.iter().take(PRANKS).enumerate() {
        pip(
            vec2(x0 + k as f32 * PIP_STEP, y),
            game.todo.done(*prank),
            1.,
        );
    }
    if game.todo.finale_open() || game.todo.done(Prank::Finale) {
        icons::draw_styled(
            prank_icon(Prank::Finale),
            vec2(x0 + PRANKS as f32 * PIP_STEP + 2., y),
            14.,
            IconStyle::new(rgb(GOLD)).shadow(c(0x2a1a0a, 0.25)),
        );
    }
    let (h, m, _) = time_of_day(game.time);
    let mut buf = [0_u8; 8];
    icon_value(
        tx,
        r.x + 21.,
        r.y + 32.,
        Icon::Clock,
        c(PENCIL, 0.85),
        fmt(&mut buf, format_args!("{h}:{m:02}")),
        SMALL * 1.5,
        c(PENCIL, 0.9),
    );
    match key {
        Some(key) => {
            let th = theme();
            let w = cap_width(key, &th);
            kit::keycap(r.x + r.w - 8. - w, r.y + 9., key, &th);
        }
        None => icons::draw(
            if open > 0.5 {
                Icon::ArrowUp
            } else {
                Icon::ArrowDown
            },
            vec2(r.x + r.w - 18., r.y + r.h * 0.5),
            16.,
            c(PENCIL, 0.7),
        ),
    }
}

/// Where a to-do line's checkbox sits right now: on the open notepad, or on
/// the collapsed card's pips.
fn line_anchor(hud: &HudState, l: &Layout, game: &Game, slot: usize) -> Vec2 {
    if hud.notes.value > 0.5 {
        let rows = if game.todo.done(Prank::Finale) { 9 } else { 8 };
        let p = l.notes_page_for(rows);
        let row = if l.short { 26. } else { 30. };
        let top = p.y + if l.short { 46. } else { 54. };
        vec2(p.x + 17.5, top + row * slot as f32 + row * 0.5 - 5.5)
    } else {
        let tab = l.notes_tab();
        let k = slot.min(PRANKS) as f32;
        vec2(tab.min[0] + 25. + k * PIP_STEP, tab.min[1] + 14.)
    }
}

/// The to-do list notepad (expanded) and its card (collapsed, and always on
/// compact screens where it sits above the page). `key` is the binding that
/// toggles it; `None` on touch, where the card itself is tapped.
pub fn notepad(tx: &mut Text, hud: &HudState, l: &Layout, game: &Game, fx: &Fx, key: Option<&str>) {
    let open = hud.notes.value.clamp(0., 1.);
    if open < 0.98 || l.compact {
        tab(tx, l, game, fx, key, open);
    }
    if open < 0.02 {
        return;
    }
    let rows = if game.todo.done(Prank::Finale) { 9 } else { 8 };
    let page = l.notes_page_for(rows);
    let slide = (1. - open) * (page.w + 30.);
    let p = Rect::new(page.x - slide, page.y, page.w, page.h);
    let alpha = open.min(1.);
    // Paper with a soft shadow, red margin, blue rules and a spiral binding.
    shapes::drop_shadow(
        p,
        6.,
        Shadow::soft(c(0x2a1a0a, 0.35 * alpha))
            .offset(vec2(2., 6.))
            .blur(16.),
    );
    shapes::fill_rounded_rect(
        p,
        6.,
        Fill::vertical(p, c(0xfffcf2, alpha), c(0xfbf0d6, alpha)),
    );
    let rule = c(0x9cbfe4, 0.55 * alpha);
    let row = if l.short { 26. } else { 30. };
    let top = p.y + if l.short { 46. } else { 54. };
    let mut y = top;
    while y < p.y + p.h - 8. {
        shapes::fill_rounded_rect(rect(p.x + 6., y + row - 4., p.w - 12., 1.), 0., rule);
        y += row;
    }
    shapes::fill_rounded_rect(
        rect(p.x + 30., p.y + 4., 1.5, p.h - 8.),
        0.,
        c(0xe57373, 0.6 * alpha),
    );
    for k in 0..((p.w / 26.) as i32) {
        let x = p.x + 16. + k as f32 * 26.;
        shapes::fill_circle(vec2(x, p.y + 1.), 5., c(0x7c7f8a, alpha));
        shapes::fill_circle(vec2(x, p.y + 1.), 2.4, c(0x3a3540, alpha));
    }
    // Header: the list's title and the afternoon clock.
    tx.ink(
        vec2(p.x + 38., p.y + if l.short { 12. } else { 16. }),
        p.w - 140.,
        "To do",
        if l.short { BODY } else { LARGE },
        c(INK, alpha),
        Align::Left,
    );
    let (h, m, ap) = time_of_day(game.time);
    let mut buf = [0_u8; 16];
    let clock = fmt(&mut buf, format_args!("{h}:{m:02} {ap}"));
    let size = SMALL * 1.5;
    let cy = p.y + if l.short { 20. } else { 28. };
    let w = crate::text::width_in(Face::Ink, clock, size) + size + 7.;
    icon_value(
        tx,
        p.x + p.w - 12. - w,
        cy,
        Icon::Clock,
        c(PENCIL, 0.85 * alpha),
        clock,
        size,
        c(PENCIL, 0.85 * alpha),
    );
    // Lines: seven pranks, then the finale and escape when they matter.
    let mut y = top;
    let show = if game.todo.done(Prank::Finale) { 9 } else { 8 };
    let bottom = p.y + p.h - if l.short { 34. } else { 44. };
    for (i, prank) in Prank::ALL.iter().take(show).enumerate() {
        if y + row > bottom {
            break;
        }
        let done = game.todo.done(*prank);
        let strike = if done {
            if fx.strike[prank.slot()] > 0. {
                fx.strike[prank.slot()]
            } else {
                1.
            }
        } else {
            0.
        };
        let secret = *prank == Prank::Finale && !game.todo.finale_open() && !done;
        let ink = if secret {
            c(0x9a8f9e, alpha)
        } else if done {
            c(0x7d7686, alpha)
        } else {
            c(INK, alpha)
        };
        let mid = y + row * 0.5 - 5.5;
        // Checkbox with a hand-drawn tick.
        let bx = rect(p.x + 10., mid - 7.5, 15., 15.);
        shapes::stroke_rounded_rect(bx, 3., 1.5, c(PENCIL, 0.75 * alpha));
        if done {
            let k = (strike * 1.6 - 0.4).clamp(0., 1.);
            let a = vec2(bx.x + 2., bx.y + 8.);
            let b = vec2(bx.x + 6.5, bx.y + 13.);
            let e = vec2(bx.x + 17., bx.y - 3.);
            if k > 0. {
                let pts = [
                    a,
                    a.lerp(b, (k * 2.).min(1.)),
                    b.lerp(e, (k * 2. - 1.).max(0.)),
                ];
                shapes::polyline(
                    &pts,
                    Stroke::new(2.6, c(TICK, alpha))
                        .join(Join::Round)
                        .cap(Cap::Round),
                );
            }
        }
        // A coloured-pencil pictogram per line (greyed once done, a padlock
        // while the finale is still a secret).
        let (icon, tint) = if secret {
            (Icon::Lock, c(0x9a8f9e, alpha))
        } else if done {
            (prank_icon(*prank), c(0xa49cad, alpha))
        } else {
            (prank_icon(*prank), c(prank_tint(*prank), alpha))
        };
        let icon_size = if l.short { 16. } else { 19. };
        icons::draw(icon, vec2(p.x + 46., mid), icon_size, tint);
        let x = p.x + 60.;
        let tw = p.w - 68.;
        let size = if l.short || tw < 200. {
            SMALL * 1.5
        } else {
            BODY
        };
        let line = line_for(game, *prank);
        tx.ink(
            vec2(x, y + row * 0.5 - size * 0.75),
            tw,
            line,
            size,
            ink,
            Align::Left,
        );
        if done {
            let w = crate::text::width_in(Face::Ink, line, size).min(tw);
            let my = y + row * 0.5 - size * 0.18;
            pencil(
                vec2(x - 3., my),
                vec2(x + w + 4., my - 1.),
                strike,
                2.2,
                c(0xd94a4a, 0.85 * alpha),
                i as f32 * 1.7,
            );
        }
        y += row;
    }
    // Footer: bottle caps, bonus stars and the day's running time.
    let size = if l.short { SMALL * 1.5 } else { BODY };
    let cy = p.y + p.h - if l.short { 26. } else { 34. } + size * 0.5;
    let ink = c(PENCIL, alpha);
    let gold = c(GOLD, alpha);
    let mut buf = [0_u8; 16];
    let caps = fmt(
        &mut buf,
        format_args!("{}/{}", hud.caps.shown(), game.shinies.len()),
    );
    let x = icon_value(tx, p.x + 38., cy, Icon::Coin, gold, caps, size, ink);
    let mut buf = [0_u8; 16];
    let bonus = fmt(
        &mut buf,
        format_args!("{}/{}", game.todo.bonus.count_ones(), Bonus::ALL.len()),
    );
    icon_value(tx, x + 16., cy, Icon::Star, gold, bonus, size, ink);
    let secs = game.time as u32;
    let mut buf = [0_u8; 16];
    let elapsed = fmt(&mut buf, format_args!("{:02}:{:02}", secs / 60, secs % 60));
    let w = crate::text::width_in(Face::Ink, elapsed, size) + size + 7.;
    icon_value(
        tx,
        p.x + p.w - 14. - w,
        cy,
        Icon::Hourglass,
        c(PENCIL, 0.85 * alpha),
        elapsed,
        size,
        ink,
    );
}

/// Context prompt at the bottom centre: a key cap, a pictogram and a word
/// or two, e.g. `[E] ✋ Grab Nana's pie`. Hold actions show the ring they fill.
pub fn prompt(
    tx: &mut Text,
    hud: &HudState,
    l: &Layout,
    game: &Game,
    grab_key: &str,
    jump_key: &str,
    touch: bool,
) {
    let a = hud.prompt_fade;
    if a < 0.02 {
        return;
    }
    let th = theme().faded(a);
    let name = |i: usize| prop_name(game.props[i].kind);
    let mut buf = [0_u8; 48];
    let (icon, tint, action): (Icon, u32, &str) = match game.coon.prompt {
        Prompt::None => return,
        Prompt::Grab(i) => (
            Icon::Hand,
            0xc9823a,
            fmt(&mut buf, format_args!("Grab {}", name(i))),
        ),
        Prompt::Drop(i) => (
            Icon::ArrowDown,
            TICK,
            fmt(&mut buf, format_args!("Drop {}", name(i))),
        ),
        Prompt::Place(i) => (
            Icon::ArrowDown,
            TICK,
            fmt(&mut buf, format_args!("Place {}", name(i))),
        ),
        Prompt::Load => (prank_icon(Prank::Sled), 0x4f9a5a, "Load the tray"),
        Prompt::Basket => (Icon::EyeOff, LEAF, "Hide"),
        Prompt::LeaveBasket => (Icon::ArrowUp, TICK, "Hop out"),
        Prompt::Ring => (prank_icon(Prank::Bell), 0xc98f1b, "Hold to ring"),
        Prompt::Yank => (prank_icon(Prank::Finale), GOLD, "Hold to yank"),
        Prompt::Wriggle => (Icon::ArrowUp, RED, "Wriggle!"),
    };
    let key = if matches!(game.coon.prompt, Prompt::Wriggle) {
        jump_key
    } else {
        grab_key
    };
    let cap = cap_width(key, &th);
    let icon_w = 22.;
    let text_w = crate::text::width_in(Face::Ink, action, BODY);
    let width = icon_w + 6. + text_w + if touch { 0. } else { cap + kit::SPACE[1] };
    let y = if touch {
        touch_prompt_y(l)
    } else {
        l.screen.y - 74.
    };
    let x = (l.screen.x - width) * 0.5;
    let x = if touch {
        x.max(10.).min(l.screen.x - width - 10.)
    } else {
        x
    };
    let r = rect(x - 12., y - 8., width + 24., 44.);
    shapes::drop_shadow(
        r,
        22.,
        Shadow::soft(c(0x2a1a0a, 0.3 * a))
            .offset(vec2(0., 4.))
            .blur(12.),
    );
    shapes::fill_rounded_rect(
        r,
        22.,
        Fill::vertical(r, c(0xfffaf0, 0.94 * a), c(0xfbefd2, 0.94 * a)),
    );
    let mut cx = x;
    if !touch {
        kit::keycap(cx, y, key, &th);
        cx += cap + kit::SPACE[1];
    }
    icons::draw_styled(
        icon,
        vec2(cx + icon_w * 0.5, y + 11.5),
        icon_w,
        IconStyle::new(c(tint, a)),
    );
    cx += icon_w + 6.;
    tx.ink(
        vec2(cx, y + 4.),
        text_w + 4.,
        action,
        BODY,
        c(INK, a),
        Align::Left,
    );
    // Hold actions: an empty ring that fills while the key is held.
    if matches!(game.coon.prompt, Prompt::Ring | Prompt::Yank) {
        let at = vec2(r.x + r.w + 16., r.y + r.h * 0.5);
        shapes::fill_circle(at, 15., c(0xfffaf0, 0.9 * a));
        shapes::stroke_circle(at, 11., 4., c(INK, 0.15 * a));
        if hud.hold > 0. {
            shapes::stroke_arc(
                at,
                11.,
                -std::f32::consts::FRAC_PI_2,
                hud.hold * std::f32::consts::TAU,
                Stroke::new(4., c(RED, a)).cap(Cap::Round),
            );
        }
    }
    if matches!(game.coon.prompt, Prompt::Wriggle) {
        let n = game.coon.wriggle as usize;
        for k in 0..crate::sim::raccoon::WRIGGLES as usize {
            let on = k < n;
            shapes::fill_circle(
                vec2(r.x + r.w * 0.5 - 50. + k as f32 * 20., r.y + r.h + 10.),
                6.,
                if on { c(RED, a) } else { c(0xffffff, 0.6 * a) },
            );
        }
    }
}

/// A bobbing chevron and a soft glow over whatever the context prompt acts
/// on (the prop to grab, the tray to load, the bell rope, the tablecloth or
/// the basket), so the prompt points at the world instead of describing it.
pub fn target(
    hud: &HudState,
    l: &Layout,
    game: &Game,
    camera: &Camera3D,
    alpha: f32,
    t: f32,
    motion: f32,
) {
    let a = hud.prompt_fade;
    if a < 0.02 {
        return;
    }
    let at = match game.coon.prompt {
        Prompt::Grab(i) => game.prop_pose(i, alpha).0 + Vec3::Y * (game.props[i].radius() + 0.2),
        Prompt::Load => {
            let tray = crate::sim::props::find(game, PropKind::Tray);
            game.prop_pose(tray, alpha).0 + Vec3::Y * 0.55
        }
        Prompt::Ring => game.park.zone(Role::Rope).at + Vec3::Y * 1.4,
        Prompt::Yank => game.park.zone(Role::Cloth).at + Vec3::Y * 1.2,
        Prompt::Basket => game.park.zone(Role::Basket).at + Vec3::Y * 0.9,
        _ => return,
    };
    let Some(p) = project(camera, l.screen, at) else {
        return;
    };
    if p.x < 0. || p.y < 0. || p.x > l.screen.x || p.y > l.screen.y {
        return;
    }
    let bob = ((t * 5.).sin() * 0.5 + 0.5) * 7. * motion;
    let tip = p + vec2(0., -8. - bob);
    shapes::glow_disc(p, 22., c(0xfff2b8, 0.5 * a));
    shapes::fill_triangle(
        tip + vec2(0., 3.),
        tip + vec2(-14., -16.),
        tip + vec2(14., -16.),
        c(0x2a1a0a, 0.28 * a),
    );
    shapes::fill_triangle(
        tip,
        tip + vec2(-13., -18.),
        tip + vec2(13., -18.),
        c(0xfffaf0, a),
    );
    shapes::polyline(
        &[tip + vec2(-13., -18.), tip, tip + vec2(13., -18.)],
        Stroke::new(3., c(RED, a)).join(Join::Round).cap(Cap::Round),
    );
}

/// Place this frame's fresh HUD effects, then draw the screen-space juice: a
/// bottle cap and its count rising from the raccoon, sparkles and a ring on
/// a freshly ticked line.
pub fn effects(hud: &mut HudState, l: &Layout, game: &Game, camera: &Camera3D, alpha: f32) {
    if hud.fresh_caps > 0 {
        let (p, _) = game.coon_pose(alpha);
        if let Some(at) = project(camera, l.screen, p + Vec3::Y * 0.9) {
            let mut buf = [0_u8; 16];
            let text = fmt(
                &mut buf,
                format_args!("{}/{}", hud.fresh_caps, game.shinies.len()),
            );
            hud.juice.popup_icon(at, Icon::Coin, text, rgb(GOLD));
            hud.juice.sparkle(at, rgb(0xfff2b8));
        }
        hud.fresh_caps = 0;
    }
    if hud.fresh_lines != 0 {
        for slot in 0..Prank::ALL.len() {
            if hud.fresh_lines & 1 << slot != 0 {
                let at = line_anchor(hud, l, game, slot);
                hud.juice.sparkle(at, rgb(GOLD));
                hud.juice.ring(at, rgb(TICK), 26.);
            }
        }
        hud.fresh_lines = 0;
    }
    hud.juice.draw();
}

struct Buf<'a> {
    buf: &'a mut [u8],
    len: usize,
}
impl std::fmt::Write for Buf<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let n = s.len().min(self.buf.len() - self.len);
        let mut n2 = n;
        while !s.is_char_boundary(n2) {
            n2 -= 1;
        }
        self.buf[self.len..self.len + n2].copy_from_slice(&s.as_bytes()[..n2]);
        self.len += n2;
        Ok(())
    }
}

/// Format into a stack buffer without allocating.
pub fn fmt<'a>(buf: &'a mut [u8], args: std::fmt::Arguments) -> &'a str {
    let mut b = Buf { buf, len: 0 };
    let _ = std::fmt::Write::write_fmt(&mut b, args);
    let len = b.len;
    std::str::from_utf8(&buf[..len]).unwrap_or("")
}

/// Short names for prompts ("Grab Nana's pie", "Drop melon").
pub fn prop_name(kind: PropKind) -> &'static str {
    match kind {
        PropKind::Pie => "Nana's pie",
        PropKind::Marrow => "marrow",
        PropKind::Pinecone => "pinecone",
        PropKind::Sheet => "sheet music",
        PropKind::Tray => "tea tray",
        PropKind::Blanket => "blanket",
        PropKind::Melon => "melon",
        PropKind::Cushion => "cushion",
        PropKind::Crate => "crate",
        PropKind::Cake => "cake",
        PropKind::Dish => "dish",
        PropKind::Teapot => "teapot",
        PropKind::Jelly => "jelly",
        PropKind::Apple => "apple",
    }
}

/// Project a world point to logical screen pixels (None when behind).
pub fn project(camera: &Camera3D, screen: Vec2, p: Vec3) -> Option<Vec2> {
    let m = camera.matrix(screen.x, screen.y);
    let clip = m * p.extend(1.);
    if clip.w <= 0.05 {
        return None;
    }
    let ndc = clip.truncate() / clip.w;
    Some(vec2(
        (ndc.x + 1.) * 0.5 * screen.x,
        (1. - ndc.y) * 0.5 * screen.y,
    ))
}

/// What a bubble over a picnicker shows.
#[derive(Clone, Copy)]
enum Mark {
    Text(&'static str),
    Icon(Icon),
}

/// "?" / "!" / heart bubbles and speech lines over heads; edge arrows for chasers.
pub fn bubbles(
    tx: &mut Text,
    l: &Layout,
    game: &Game,
    fx: &Fx,
    camera: &Camera3D,
    alpha: f32,
    t: f32,
    motion: f32,
) {
    for (i, f) in game.folk.iter().enumerate() {
        let pos = f.prev.lerp(f.pos, alpha);
        let head = pos
            + Vec3::Y
                * match f.kind {
                    Kind::Pip => 1.55,
                    Kind::Biscuit => 1.0,
                    Kind::Trumble => 2.55,
                    _ => 2.2,
                };
        let projected = project(camera, l.screen, head);
        let mark = match (f.bubble, f.state()) {
            (_, ALARMED) => Some((Mark::Text("!"), rgb(0xff5a3c))),
            (_, CURIOUS) => Some((Mark::Text("?"), rgb(0xf2b632))),
            (Bubble::Heart, _) => Some((Mark::Icon(Icon::Heart), rgb(0xe8607a))),
            (Bubble::Star, _) => Some((Mark::Icon(Icon::Star), rgb(0xf2b632))),
            (Bubble::Shock, _) => Some((Mark::Text("!?"), rgb(0xff5a3c))),
            (Bubble::Sweat, _) => Some((Mark::Icon(Icon::Drop), rgb(0x6fb1c9))),
            (Bubble::Huff, _) if f.bubble_age < 1.6 => Some((Mark::Text("hmph"), rgb(0x8a7360))),
            (Bubble::Question, _) if f.mode == Mode::Search => {
                Some((Mark::Text("?"), rgb(0xf2b632)))
            }
            _ => None,
        };
        let Some(sp) = projected else { continue };
        let distance = flat(pos - game.coon.pos()).length();
        let on_screen =
            sp.x > -20. && sp.x < l.screen.x + 20. && sp.y > -20. && sp.y < l.screen.y + 20.;
        if on_screen && let Some((mark, color)) = mark {
            let wide = matches!(mark, Mark::Text(s) if s.len() > 2);
            let pop = (f.bubble_age / 0.18).min(1.);
            let bob = (t * 4. + i as f32).sin() * 2. * motion;
            let s = 0.7 + 0.3 * pop;
            let r = (17. + if wide { 12. } else { 0. }) * s;
            let center = vec2(sp.x, sp.y - 14. + bob);
            shapes::drop_shadow(
                Rect::new(center.x - r, center.y - r * 0.8, r * 2., r * 1.6),
                r * 0.8,
                Shadow::soft(Color::new(0., 0., 0., 0.25)).blur(8.),
            );
            shapes::fill_rounded_rect(
                Rect::new(center.x - r, center.y - r * 0.8, r * 2., r * 1.6),
                r * 0.8,
                rgb(0xfffdf6),
            );
            shapes::fill_capsule(
                center + vec2(-4., r * 0.75),
                center + vec2(-8., r * 1.05),
                3.,
                rgb(0xfffdf6),
            );
            shapes::stroke_rounded_rect(
                Rect::new(center.x - r, center.y - r * 0.8, r * 2., r * 1.6),
                r * 0.8,
                2.,
                color,
            );
            match mark {
                Mark::Icon(icon) => icons::draw(icon, center, r * 1.15, color),
                Mark::Text(text) if wide => tx.ink(
                    vec2(center.x - r, center.y - BODY * 0.62),
                    r * 2.,
                    text,
                    BODY,
                    color,
                    Align::Center,
                ),
                Mark::Text(text) => tx.big(
                    vec2(center.x - r, center.y - LARGE * 0.62),
                    r * 2.,
                    text,
                    LARGE,
                    color,
                    Align::Center,
                ),
            }
        }
        if let Some(s) = fx.speech[i]
            && on_screen
            && distance < 22.
        {
            let a = (s.age / 0.15).min(1.) * ((2.8 - s.age) / 0.4).clamp(0., 1.);
            let w = (crate::text::width_in(Face::Modern, s.text, BODY) + 24.).min(l.screen.x * 0.6);
            let x = (sp.x - w * 0.5).clamp(8., l.screen.x - w - 8.);
            let y = (sp.y - if mark.is_some() { 82. } else { 52. }).max(l.notes_tab().min[1] + 52.);
            let r = Rect::new(x, y, w, 30.);
            shapes::fill_rounded_rect(r, 12., c(0x2f2530, 0.82 * a));
            tx.label(
                vec2(x + 6., y + 7.),
                w - 12.,
                s.text,
                BODY,
                Color::new(1., 0.97, 0.9, a),
                Align::Center,
            );
        }
        // Edge arrows for anyone chasing from off-screen.
        if f.mode == Mode::Chase && !on_screen {
            let center = l.screen * 0.5;
            let dir = (sp - center).normalize_or_zero();
            let edge = center + dir * (l.screen.min_element() * 0.45);
            let pulse = 0.7 + 0.3 * (t * 8.).sin() * motion;
            let r = 14. * pulse.max(0.85);
            let side = vec2(-dir.y, dir.x);
            shapes::fill_triangle(
                edge + dir * (r + 10.),
                edge + dir * (r - 3.) + side * 9.,
                edge + dir * (r - 3.) - side * 9.,
                c(0xff5a3c, 0.9),
            );
            shapes::fill_circle(edge, r, c(0xff5a3c, 0.9));
            tx.big(
                edge - vec2(14., 9.),
                28.,
                "!",
                BODY,
                rgb(0xffffff),
                Align::Center,
            );
        }
    }
}

/// A cream pill with a coloured icon badge and a few words.
fn icon_toast(
    tx: &mut Text,
    cx: f32,
    top: f32,
    icon: Icon,
    tint: Color,
    text: &str,
    look: Appearance,
    max_w: f32,
) -> Rect {
    let a = look.alpha;
    let text_w = crate::text::width_in(Face::Ink, text, BODY).min(max_w - 58.);
    let h = 38.;
    let w = 6. + 28. + 10. + text_w + 16.;
    let r = kit::appear(Rect::new(cx - w * 0.5, top, w, h), look);
    shapes::drop_shadow(
        r,
        r.h * 0.5,
        Shadow::soft(c(0x2a1a0a, 0.3 * a))
            .offset(vec2(0., 5.))
            .blur(16.),
    );
    shapes::fill_rounded_rect(
        r,
        r.h * 0.5,
        Fill::vertical(r, c(0xfffbf0, 0.97 * a), c(0xfff4dc, 0.97 * a)),
    );
    shapes::stroke_rounded_rect(r, r.h * 0.5, 1.5, with_alpha(tint, 0.45 * a));
    let s = r.h / h;
    icons::badge(
        icon,
        vec2(r.x + 20. * s, r.y + r.h * 0.5),
        28. * s,
        with_alpha(tint, a),
        c(0xffffff, a),
    );
    tx.ink(
        vec2(r.x + 44. * s, r.y + r.h * 0.5 - BODY * 0.5),
        text_w + 2.,
        text,
        BODY,
        c(INK, a),
        Align::Left,
    );
    r
}

/// Transient icon toasts (crossed-off pranks, bonuses, burrows, hats).
pub fn toasts(tx: &mut Text, l: &Layout, fx: &Fx, motion: Motion) {
    let mut y = if l.compact { 64. } else { 18. };
    let max_w = l.screen.x - 2. * if l.compact { 12. } else { 90. };
    for (k, view) in fx.toasts.iter().enumerate() {
        if k >= 2 {
            break;
        }
        let life = view.notice.lifetime_seconds as f32;
        let left = view.remaining_seconds as f32;
        let age = life - left;
        let look = Entrance::fade_up(14.).sample(age, 0, motion);
        let out = (left / 0.35).clamp(0., 1.);
        let look = Appearance {
            alpha: look.alpha * out,
            ..look
        };
        let mut buf = [0_u8; 64];
        let (icon, tint, text): (Icon, u32, &str) = match view.notice.message {
            Toast::Prank(p) => (prank_icon(p), LEAF, p.toast()),
            Toast::Bonus(b) => (Icon::Star, GOLD, b.title()),
            Toast::Den => (Icon::Home, 0x8a5a3a, "New burrow!"),
            Toast::Shiny(n) => (Icon::Coin, GOLD, fmt(&mut buf, format_args!("{n}"))),
            Toast::Hat(h) => (
                Icon::Crown,
                TICK,
                fmt(&mut buf, format_args!("New hat: {}", h.name())),
            ),
            Toast::Unlocked => (prank_icon(Prank::Finale), GOLD, "Something grand!"),
            Toast::Line(s) => (Icon::Chat, TICK, s),
        };
        let r = icon_toast(tx, l.screen.x * 0.5, y, icon, rgb(tint), text, look, max_w);
        y = r.y + r.h + 8.;
    }
}

/// A rotating minimap of the park (desktop only; touch needs that corner).
pub fn minimap(hud: &mut HudState, l: &Layout, game: &Game, cam_yaw: f32, alpha: f32, t: f32) {
    let size = 150.;
    let map = ferravex_play::ui::Rect {
        min: [16., l.screen.y - size - 16.],
        size: [size, size],
    };
    let config = MapConfig {
        world: ferravex_play::ui::Rect {
            min: [-31., -31.],
            size: [62., 62.],
        },
        map,
        y_axis: WorldYAxis::Down,
        rotation_radians: -cam_yaw,
    };
    let mm = match &mut hud.minimap {
        Some(m) => {
            if m.reconfigure(config).is_err() {
                return;
            }
            m
        }
        None => match Minimap::new(config) {
            Ok(m) => hud.minimap.insert(m),
            Err(_) => return,
        },
    };
    let r = Rect::new(map.min[0], map.min[1], size, size);
    shapes::drop_shadow(r, size * 0.5, Shadow::soft(c(0x1a1006, 0.35)).blur(14.));
    shapes::fill_circle(r.center(), size * 0.5, c(0x8fbf62, 0.92));
    shapes::stroke_circle(r.center(), size * 0.5, 3., c(0xfff4dc, 0.95));
    let at = |p: Vec3| {
        mm.project(vec2(p.x, p.z), OffMapPolicy::ClampToEdge)
            .ok()
            .map(|m| m.position)
            .filter(|q| q.distance(r.center()) < size * 0.5 - 4.)
    };
    let pond = game.park.zone(Role::Pond);
    if let Some(q) = at(pond.at) {
        shapes::fill_circle(q, pond.radius * size / 62., c(0x6fb1c9, 0.9));
    }
    for tbl in &game.park.tables {
        if let Some(q) = at(tbl.center) {
            shapes::fill_circle(q, 3., c(0xfff4dc, 0.9));
        }
    }
    if let Some(q) = at(vec3(1., 0., -24.)) {
        shapes::fill_circle(q, 9., c(0x6c9f4b, 0.9));
    }
    // Burrows found so far (home first), as little house badges.
    for (i, den) in game.park.dens.iter().enumerate() {
        if game.dens_found & (1 << i) == 0 {
            continue;
        }
        if let Some(q) = at(*den) {
            icons::badge(
                Icon::Home,
                q,
                if i == 0 { 15. } else { 12. },
                c(0x5a3f2a, 0.95),
                rgb(0xfff4dc),
            );
        }
    }
    // Unfinished pranks: their notepad pictograms on little red pins.
    let targets = [
        (Prank::Pie, game.park.tables[0].center),
        (Prank::Marrow, game.park.zone(Role::Plinth).at),
        (Prank::Music, game.park.zone(Role::Stand).at),
        (Prank::Sled, vec3(-2., 0., -24.)),
        (Prank::Bell, game.park.zone(Role::Rope).at),
        (
            Prank::Cushion,
            game.park.seat(0).map_or(Vec3::ZERO, |s| s.at),
        ),
        (Prank::Melon, game.mech.seesaw_pivot),
        (Prank::Finale, game.park.zone(Role::Cloth).at),
    ];
    for (prank, p) in targets {
        if game.todo.done(prank) || (prank == Prank::Finale && !game.todo.finale_open()) {
            continue;
        }
        if let Some(q) = at(p) {
            icons::badge(prank_icon(prank), q, 14., c(RED, 0.95), rgb(0xfff4dc));
        }
    }
    for f in &game.folk {
        if let Some(q) = at(f.prev.lerp(f.pos, alpha)) {
            let col = match f.state() {
                ALARMED => rgb(0xff5a3c),
                CURIOUS => rgb(0xf2b632),
                _ => rgb(0xfdf6e3),
            };
            shapes::fill_circle(q, 4., c(0x2a1a0a, 0.6));
            shapes::fill_circle(q, 3.2, col);
        }
    }
    let (p, yaw) = game.coon_pose(alpha);
    if let Some(q) = at(p) {
        let rel = yaw - cam_yaw;
        let d = vec2(rel.sin(), -rel.cos());
        let side = vec2(-d.y, d.x);
        let pulse = 1. + 0.1 * (t * 6.).sin();
        let tip = q + d * 8. * pulse;
        shapes::fill_circle(q, 5.5, rgb(0x2a2a30));
        shapes::polyline(
            &[q - d * 3. + side * 4., tip, q - d * 3. - side * 4.],
            Stroke::new(2.5, rgb(0xfdf6e3)),
        );
    }
}

/// Where touch prompts sit: in landscape along the free bottom-centre strip
/// between the move area and the buttons; in tall portrait between the
/// raccoon and the buttons; on short portrait screens above the raccoon.
fn touch_prompt_y(l: &Layout) -> f32 {
    if !l.portrait {
        l.screen.y - 58.
    } else if l.screen.y - 300. > l.screen.y * 0.5 + 60. {
        l.screen.y - 300.
    } else {
        l.screen.y * 0.5 - 110.
    }
}

/// Hiding / sneaking chips (icon and a word) above the prompt.
pub fn status(tx: &mut Text, l: &Layout, game: &Game, touch: bool) {
    let mut chips: [Option<(Icon, &str)>; 2] = [None; 2];
    if game.coon.hide != Hide::None {
        chips[0] = Some((Icon::EyeOff, "Hidden"));
    }
    if game.coon.sneaking {
        chips[1] = Some((Icon::Paw, "Sneak"));
    }
    let n = chips.iter().flatten().count();
    if n == 0 {
        return;
    }
    let y = if touch {
        touch_prompt_y(l) - 44.
    } else {
        l.screen.y - 116.
    };
    let widths: [f32; 2] = chips.map(|s| {
        s.map_or(0., |(_, s)| {
            crate::text::width_in(Face::Modern, s, BODY) + 52.
        })
    });
    let mut x = l.screen.x * 0.5 - (widths[0] + widths[1] + if n > 1 { 8. } else { 0. }) * 0.5;
    for (k, chip) in chips.iter().enumerate() {
        let Some((icon, label)) = chip else { continue };
        let r = rect(x, y, widths[k], 30.);
        shapes::fill_rounded_rect(r, 15., c(0x2f4a2a, 0.82));
        icons::draw(*icon, vec2(x + 20., y + 15.), 18., rgb(0xc9eba8));
        tx.label(
            vec2(x + 36., y + 7.),
            widths[k] - 44.,
            label,
            BODY,
            rgb(0xe9f7d6),
            Align::Left,
        );
        x += widths[k] + 8.;
    }
}

/// Touch buttons (labels on the buttons; pressed ones glow).
pub fn touch(tx: &mut Text, l: &Layout, input: &ferravex_render::input::PlayerInput, game: &Game) {
    for b in l.buttons() {
        let held = match b.code {
            crate::layout::TOUCH_JUMP => input.held("jump"),
            crate::layout::TOUCH_GRAB => input.held("interact"),
            crate::layout::TOUCH_THROW => input.held("throw"),
            crate::layout::TOUCH_CHITTER => input.held("chitter"),
            _ => input.held("slide"),
        };
        let enabled = match b.code {
            crate::layout::TOUCH_THROW => game.coon.held.is_some(),
            _ => true,
        };
        let a = if enabled { 0.85 } else { 0.4 };
        shapes::fill_circle(b.center + vec2(0., 3.), b.radius, c(0x1a1006, 0.25 * a));
        shapes::fill_circle(
            b.center,
            b.radius * if held { 0.94 } else { 1. },
            Fill::radial(b.center, b.radius, c(0xfffaf0, a), c(0xf1e2c0, a)),
        );
        shapes::stroke_circle(b.center, b.radius, 2., c(RED, if held { 1. } else { 0.5 }));
        let label = if b.code == crate::layout::TOUCH_GRAB {
            match game.coon.prompt {
                Prompt::Drop(_) | Prompt::Place(_) | Prompt::Load => "Drop",
                Prompt::Ring | Prompt::Yank => "Hold",
                Prompt::Basket => "Hide",
                Prompt::LeaveBasket => "Out",
                _ => "Grab",
            }
        } else {
            b.label
        };
        let size = if b.radius > 40. { BODY } else { SMALL * 1.5 };
        tx.label(
            vec2(b.center.x - b.radius, b.center.y - size * 0.55),
            b.radius * 2.,
            label,
            size,
            c(INK, a + 0.15),
            Align::Center,
        );
    }
}

/// Floating stick ring while a finger steers.
pub fn stick(origin: Vec2, position: Vec2) {
    shapes::stroke_circle(origin, 54., 3., Color::new(1., 0.97, 0.9, 0.45));
    shapes::fill_circle(position, 24., Color::new(1., 0.97, 0.9, 0.55));
}

/// Cinematic bars while a set piece is framed (skipped under reduced motion).
pub fn letterbox(l: &Layout, weight: f32) {
    if weight <= 0.01 {
        return;
    }
    let h = l.screen.y * 0.075 * weight;
    let col = Color::new(0.08, 0.05, 0.03, 0.9);
    shapes::fill_rounded_rect(rect(0., 0., l.screen.x, h), 0., col);
    shapes::fill_rounded_rect(rect(0., l.screen.y - h, l.screen.x, h), 0., col);
}

/// A soft white flash on big moments (zero under reduced flash).
pub fn flash(l: &Layout, amount: f32) {
    if amount > 0.01 {
        shapes::fill_rounded_rect(
            rect(0., 0., l.screen.x, l.screen.y),
            0.,
            Color::new(1., 0.98, 0.92, amount * 0.35),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::RunConfig;

    #[test]
    fn afternoon_clock_and_stack_formatting() {
        assert_eq!(time_of_day(0.), (12, 0, "pm"));
        assert_eq!(time_of_day(300.), (1, 0, "pm"));
        assert_eq!(time_of_day(1800.), (6, 0, "pm"));
        let mut b = [0_u8; 8];
        assert_eq!(fmt(&mut b, format_args!("{}", 1234567890)), "12345678");
        let cam = Camera3D {
            position: vec3(0., 5., 10.),
            target: Vec3::ZERO,
            ..Camera3D::default()
        };
        let p = project(&cam, vec2(800., 600.), Vec3::ZERO).unwrap();
        assert!((p.x - 400.).abs() < 0.5 && (p.y - 300.).abs() < 0.5);
        assert!(project(&cam, vec2(800., 600.), vec3(0., 5., 30.)).is_none());
    }

    #[test]
    fn every_line_has_its_own_pictogram_and_short_copy() {
        let mut seen = Vec::new();
        for prank in Prank::ALL {
            let icon = prank_icon(prank);
            assert!(!seen.contains(&icon), "{prank:?} shares {icon:?}");
            seen.push(icon);
            assert!(prank.line().split(' ').count() <= 3, "{}", prank.line());
            assert!(prank.toast().split(' ').count() <= 3, "{}", prank.toast());
        }
        for kind in [
            PropKind::Pie,
            PropKind::Sheet,
            PropKind::Tray,
            PropKind::Apple,
        ] {
            assert!(prop_name(kind).split(' ').count() <= 2);
        }
    }

    #[test]
    fn hud_celebrates_only_fresh_progress() {
        let mut game = Game::new(RunConfig {
            seed: 7,
            completed: 0b101,
        });
        let mut hud = HudState::new();
        hud.update(&game, 1. / 60., Motion::Full);
        assert_eq!(hud.fresh_lines, 0, "a continued day starts as already seen");
        let at = game.coon.pos();
        crate::sim::pranks::complete(&mut game, Prank::Bell, at);
        hud.update(&game, 1. / 60., Motion::Full);
        assert_eq!(hud.fresh_lines, 1 << Prank::Bell.slot());
        hud.reset(false);
        hud.update(&game, 1. / 60., Motion::Full);
        assert_eq!(hud.fresh_lines, 0, "restarts never replay old celebrations");
    }
}
