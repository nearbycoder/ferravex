//! The in-play HUD: a hand-written to-do notepad with satisfying pencil
//! strike-throughs, context key prompts, speech bubbles over picnickers,
//! toasts, a rotating park minimap and touch buttons. Drawn after the
//! post-processing boundary, so it stays crisp and un-bloomed.
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
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Cap, Fill, Join, Shadow, Stroke};
use ferravex_render::ui::kit::{self, KitTheme, Tone};

pub const PAPER: u32 = 0xfdf6e3;
pub const INK: u32 = 0x3b3445;
pub const PENCIL: u32 = 0x4a4a58;
pub const RED: u32 = 0xe85d4f;

pub fn c(hex: u32, a: f32) -> Color {
    let k = rgb(hex);
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

pub struct HudState {
    pub notes_open: bool,
    notes: Spring,
    caps: RollCounter,
    prompt_fade: f32,
    hold: f32,
    pub minimap: Option<Minimap>,
    pub age: f32,
}

impl Default for HudState {
    fn default() -> Self {
        Self::new()
    }
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
        }
    }

    pub fn reset(&mut self, compact: bool) {
        self.notes_open = !compact;
        self.notes.snap(if self.notes_open { 1. } else { 0. });
        self.caps.snap(0);
        self.age = 0.;
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
        Prank::Finale if !game.todo.finale_open() && !game.todo.done(Prank::Finale) => {
            "??? (something grand)"
        }
        _ => prank.line(),
    }
}

/// The to-do list notepad (expanded) or its little tab (collapsed).
pub fn notepad(
    tx: &mut Text,
    hud: &HudState,
    l: &Layout,
    game: &Game,
    fx: &Fx,
    t: f32,
    open_key: &str,
) {
    let open = hud.notes.value.clamp(0., 1.);
    let tab = l.notes_tab();
    let done = game.todo.count();
    let pop = 1. + fx.pop * 0.12;
    // Collapsed tab, always visible.
    if open < 0.98 {
        let r = rect(tab.min[0], tab.min[1], tab.size[0], tab.size[1]);
        let r = Rect::new(r.x, r.y + (1. - pop) * 8., r.w * pop.min(1.06), r.h);
        shapes::drop_shadow(
            r,
            10.,
            Shadow::soft(c(0x2a1a0a, 0.3)).offset(vec2(0., 3.)).blur(8.),
        );
        shapes::fill_rounded_rect(r, 10., Fill::vertical(r, rgb(0xfffaf0), rgb(PAPER)));
        shapes::stroke_rounded_rect(r, 10., 1., c(INK, 0.15));
        shapes::fill_rounded_rect(rect(r.x + 8., r.y + 8., 6., r.h - 16.), 3., rgb(RED));
        tx.ink_fmt(
            vec2(r.x + 22., r.y + 6.),
            r.w - 26.,
            BODY,
            rgb(INK),
            Align::Left,
            format_args!("To do  {done}/{PRANKS}"),
        );
        let (h, m, ap) = time_of_day(game.time);
        tx.ink_fmt(
            vec2(r.x + 22., r.y + 25.),
            r.w - 26.,
            SMALL,
            c(PENCIL, 0.8),
            Align::Left,
            format_args!("{h}:{m:02} {ap} · {} to open", key_label(open_key)),
        );
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
    let (h, m, ap) = time_of_day(game.time);
    tx.ink(
        vec2(p.x + 38., p.y + if l.short { 12. } else { 16. }),
        p.w - 110.,
        "Things to do today",
        if l.short { BODY } else { LARGE },
        c(INK, alpha),
        Align::Left,
    );
    tx.ink_fmt(
        vec2(p.x + p.w - 78., p.y + 18.),
        68.,
        SMALL,
        c(PENCIL, 0.8 * alpha),
        Align::Right,
        format_args!("{h}:{m:02} {ap}"),
    );
    // Lines: seven pranks, then the finale and escape when they matter.
    let mut y = top;
    let show = if game.todo.finale_open() || game.todo.done(Prank::Finale) {
        if game.todo.done(Prank::Finale) { 9 } else { 8 }
    } else {
        8
    };
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
        let x = p.x + 40.;
        let tw = p.w - 52.;
        let secret = *prank == Prank::Finale && !game.todo.finale_open() && !done;
        let ink = if secret {
            c(0x9a8f9e, alpha)
        } else if done {
            c(0x7d7686, alpha)
        } else {
            c(INK, alpha)
        };
        // Checkbox with a hand-drawn tick.
        let bx = rect(p.x + 10., y + row * 0.5 - 13., 15., 15.);
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
                    Stroke::new(2.6, c(0x3f7fc1, alpha))
                        .join(Join::Round)
                        .cap(Cap::Round),
                );
            }
        }
        let size = if l.short || tw < 250. {
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
    // Footer: shiny caps, bonuses, and the day so far.
    let fy = p.y + p.h - if l.short { 26. } else { 34. };
    let bonuses = game.todo.bonus.count_ones();
    tx.ink_fmt(
        vec2(p.x + 14., fy),
        p.w - 28.,
        if l.short { SMALL * 1.5 } else { BODY },
        c(PENCIL, alpha),
        Align::Left,
        format_args!(
            "Shinies {}/{}   Bonus {}/{}",
            hud.caps.shown(),
            game.shinies.len(),
            bonuses,
            Bonus::ALL.len()
        ),
    );
    let secs = game.time as u32;
    tx.ink_fmt(
        vec2(p.x + 14., fy),
        p.w - 28.,
        if l.short { SMALL * 1.5 } else { BODY },
        c(PENCIL, alpha),
        Align::Right,
        format_args!("{:02}:{:02}", secs / 60, secs % 60),
    );
    let _ = t;
}

fn key_label(binding: &str) -> &str {
    binding
}

/// Context prompt at the bottom centre, e.g. `[E] Grab the pie`.
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
    let mut buf = [0_u8; 64];
    let action: &str = match game.coon.prompt {
        Prompt::None => return,
        Prompt::Grab(i) => fmt(&mut buf, format_args!("Grab {}", name(i))),
        Prompt::Drop(i) => fmt(&mut buf, format_args!("Drop {}", name(i))),
        Prompt::Place(i) => fmt(&mut buf, format_args!("Place {}", name(i))),
        Prompt::Load => "Load the blanket onto the tray",
        Prompt::Basket => "Hop into the picnic basket",
        Prompt::LeaveBasket => "Hop out",
        Prompt::Ring => "Hold to ring the bell",
        Prompt::Yank => "Hold to pull the tablecloth",
        Prompt::Wriggle => "Wriggle free!",
    };
    let key = if matches!(game.coon.prompt, Prompt::Wriggle) {
        jump_key
    } else {
        grab_key
    };
    let key = key_label(key);
    // Mirror the kit keycap's measure so long labels ("Space") fit the pill.
    let cap = (crate::text::width_in(Face::Ink, key, th.body) + kit::SPACE[2] * 2.)
        .max(th.body * 1.5 - 1.);
    let width = crate::text::width_in(Face::Modern, action, BODY)
        + if touch {
            24.
        } else {
            cap + kit::SPACE[0] * 2. + 6.
        };
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
    if touch {
        tx.label(
            vec2(r.x, r.y + 12.),
            r.w,
            action,
            BODY,
            c(INK, a),
            Align::Center,
        );
    } else {
        let w = kit::key_prompt(x, y, &[key], "", &th);
        tx.label(
            vec2(x + w + 6., y + 4.),
            width,
            action,
            BODY,
            c(INK, a),
            Align::Left,
        );
    }
    // Hold progress ring for the bell and the tablecloth.
    if matches!(game.coon.prompt, Prompt::Ring | Prompt::Yank) && hud.hold > 0. {
        shapes::stroke_arc(
            vec2(r.x + r.w + 14., r.y + r.h * 0.5),
            12.,
            -std::f32::consts::FRAC_PI_2,
            hud.hold * std::f32::consts::TAU,
            Stroke::new(4., c(RED, a)).cap(Cap::Round),
        );
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

pub fn prop_name(kind: PropKind) -> &'static str {
    match kind {
        PropKind::Pie => "Nana's pie",
        PropKind::Marrow => "the prize marrow",
        PropKind::Pinecone => "a pinecone",
        PropKind::Sheet => "the sheet music",
        PropKind::Tray => "the tea tray",
        PropKind::Blanket => "the picnic blanket",
        PropKind::Melon => "the watermelon",
        PropKind::Cushion => "the whoopee cushion",
        PropKind::Crate => "a crate",
        PropKind::Cake => "the cake",
        PropKind::Dish => "a dish",
        PropKind::Teapot => "the teapot",
        PropKind::Jelly => "the jelly",
        PropKind::Apple => "an apple",
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

/// "?" / "!" / "♥" bubbles and speech lines over heads; edge arrows for chasers.
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
            (_, ALARMED) => Some(("!", rgb(0xff5a3c))),
            (_, CURIOUS) => Some(("?", rgb(0xf2b632))),
            (Bubble::Heart, _) => Some(("♥", rgb(0xe8607a))),
            (Bubble::Star, _) => Some(("★", rgb(0xf2b632))),
            (Bubble::Shock, _) => Some(("!?", rgb(0xff5a3c))),
            (Bubble::Sweat, _) => Some(("~", rgb(0x6fb1c9))),
            (Bubble::Huff, _) if f.bubble_age < 1.6 => Some(("hmph", rgb(0x8a7360))),
            (Bubble::Question, _) if f.mode == Mode::Search => Some(("?", rgb(0xf2b632))),
            _ => None,
        };
        let Some(sp) = projected else { continue };
        let distance = flat(pos - game.coon.pos()).length();
        let on_screen =
            sp.x > -20. && sp.x < l.screen.x + 20. && sp.y > -20. && sp.y < l.screen.y + 20.;
        if on_screen && let Some((text, color)) = mark {
            let pop = (f.bubble_age / 0.18).min(1.);
            let bob = (t * 4. + i as f32).sin() * 2. * motion;
            let s = 0.7 + 0.3 * pop;
            let r = (17. + if text.len() > 2 { 12. } else { 0. }) * s;
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
            if text.len() > 2 {
                tx.ink(
                    vec2(center.x - r, center.y - BODY * 0.62),
                    r * 2.,
                    text,
                    BODY,
                    color,
                    Align::Center,
                );
            } else {
                tx.big(
                    vec2(center.x - r, center.y - LARGE * 0.62),
                    r * 2.,
                    text,
                    LARGE,
                    color,
                    Align::Center,
                );
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
            shapes::fill_circle(edge, 14. * pulse.max(0.85), c(0xff5a3c, 0.9));
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

/// Transient toasts (crossed-off pranks, bonuses, bottle caps, burrows).
pub fn toasts(tx: &mut Text, l: &Layout, fx: &Fx, motion: Motion) {
    let th = theme();
    let mut y = if l.compact { 64. } else { 18. };
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
        let mut buf = [0_u8; 96];
        let (icon, text, tone): (char, &str, Tone) = match view.notice.message {
            Toast::Prank(p) => ('✓', p.toast(), Tone::Success),
            Toast::Bonus(b) => (
                '★',
                fmt(&mut buf, format_args!("Bonus! {}", b.title())),
                Tone::Warning,
            ),
            Toast::Den => (
                '●',
                "Found a cosy burrow. You'll be shooed here now.",
                Tone::Info,
            ),
            Toast::Shiny(n) => (
                '◆',
                fmt(&mut buf, format_args!("Shiny! {n} bottle caps")),
                Tone::Info,
            ),
            Toast::Hat(h) => (
                '♣',
                fmt(&mut buf, format_args!("New hat: {}", h.name())),
                Tone::Success,
            ),
            Toast::Unlocked => ('!', "A new line appeared on your list...", Tone::Warning),
            Toast::Line(s) => ('·', s, Tone::Info),
        };
        let max = l.screen.x - 2. * if l.compact { 12. } else { 90. };
        let text = if crate::text::width_in(Face::Modern, text, BODY) > max - 50. {
            // Narrow screens: shorter copy keeps the card on screen.
            match view.notice.message {
                Toast::Den => "Found a burrow!",
                Toast::Unlocked => "New line on your list!",
                _ => text,
            }
        } else {
            text
        };
        let cx = l.screen.x * 0.5;
        let r = kit::toast(cx, y, Some(icon), text, tone, look, &th);
        y = r.y + r.h + 8.;
        let _ = tx;
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
    // Unfinished pranks: little red pins.
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
            shapes::fill_circle(q, 4.5, c(RED, 0.95));
            shapes::fill_circle(q, 2., rgb(0xfff4dc));
        }
    }
    if let Some(q) = at(game.park.dens[0]) {
        shapes::fill_circle(q, 5., c(0x5a3f2a, 0.95));
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
    let _ = PRANKS;
}

/// Hiding / carrying status chips above the prompt.
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

pub fn status(tx: &mut Text, l: &Layout, game: &Game, touch: bool) {
    let mut chips: [Option<&str>; 2] = [None; 2];
    match game.coon.hide {
        Hide::Bush(_) => chips[0] = Some("Hidden in a bush"),
        Hide::Table(_) => chips[0] = Some("Hidden under a table"),
        Hide::Basket => chips[0] = Some("Hidden in the basket"),
        Hide::None => {}
    }
    if game.coon.sneaking {
        chips[1] = Some("Sneaking");
    }
    let y = if touch {
        touch_prompt_y(l) - 44.
    } else {
        l.screen.y - 116.
    };
    let mut x = l.screen.x * 0.5;
    let n = chips.iter().flatten().count();
    if n == 0 {
        return;
    }
    let widths: [f32; 2] =
        chips.map(|s| s.map_or(0., |s| crate::text::width_in(Face::Modern, s, BODY) + 26.));
    x -= (widths[0] + widths[1] + if n > 1 { 8. } else { 0. }) * 0.5;
    for (k, chip) in chips.iter().enumerate() {
        let Some(chip) = chip else { continue };
        let r = rect(x, y, widths[k], 30.);
        shapes::fill_rounded_rect(r, 15., c(0x2f4a2a, 0.82));
        tx.label(
            vec2(x, y + 7.),
            widths[k],
            chip,
            BODY,
            rgb(0xe9f7d6),
            Align::Center,
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
}
