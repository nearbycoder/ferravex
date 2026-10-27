//! Title, wardrobe, pause and results cards built from the engine UI kit.
//!
//! Input always goes through the engine [`Ui`] (stable IDs, keyboard/pad
//! focus, touch capture); the kit only paints. Entrances, button motion and
//! roll-up counters honour reduced motion through [`Motion`].
#![allow(clippy::too_many_arguments)]
use crate::hud::{INK, PAPER, RED, c, theme};
use crate::layout::Layout;
use crate::meta::Meta;
use crate::models::{Hat, rgb};
use crate::sim::pranks::{Bonus, PRANKS};
use crate::text::{Align, BODY, Face, HUGE, LARGE, SMALL, Text};
use ferravex_play::tween::{Entrance, Motion, RollCounter};
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Fill, Shadow};
use ferravex_render::ui::kit::{self, ButtonKind, ButtonMotion};
use ferravex_render::ui::{Ui, UiRect, WidgetId};

pub mod id {
    pub const NEW_DAY: u64 = 1;
    pub const CONTINUE: u64 = 2;
    pub const WARDROBE: u64 = 3;
    pub const RESUME: u64 = 4;
    pub const RESTART: u64 = 5;
    pub const GHOST: u64 = 6;
    pub const CONES: u64 = 7;
    pub const TITLE: u64 = 8;
    pub const AGAIN: u64 = 9;
    pub const RESULTS_TITLE: u64 = 10;
    pub const BACK: u64 = 11;
    pub const TOUCH_PAUSE: u64 = 12;
    pub const NOTES: u64 = 13;
    pub const HAT: u64 = 20;
    pub const COUNT: usize = 32;
}

pub struct Buttons {
    motion: [ButtonMotion; id::COUNT],
}

impl Default for Buttons {
    fn default() -> Self {
        Self::new()
    }
}

impl Buttons {
    pub fn new() -> Self {
        Self {
            motion: [ButtonMotion::default(); id::COUNT],
        }
    }
    pub fn snap(&mut self) {
        self.motion = [ButtonMotion::default(); id::COUNT];
    }
}

pub struct Menu<'a> {
    pub tx: &'a mut Text,
    pub ui: &'a mut Ui,
    pub buttons: &'a mut Buttons,
    pub t: f32,
    pub dt: f32,
    pub motion: Motion,
    pub sway: f32,
}

fn ur(r: Rect) -> UiRect {
    UiRect {
        min: [r.x, r.y],
        size: [r.w.max(0.), r.h.max(0.)],
    }
}

impl Menu<'_> {
    pub fn button(&mut self, id: WidgetId, r: Rect, label: &str, enabled: bool, kind: ButtonKind) {
        let _ = self
            .ui
            .button(id, ur(r), "", enabled, kit::INVISIBLE_BUTTON);
        let state = kit::ui_button_state(self.ui, id, enabled);
        let m = &mut self.buttons.motion[(id as usize).min(id::COUNT - 1)];
        m.update(state, self.dt, self.motion);
        let th = theme();
        kit::button(r, label, kind, m, &th);
    }
}

fn dim(l: &Layout, a: f32) {
    shapes::fill_rounded_rect(
        Rect::new(0., 0., l.screen.x, l.screen.y),
        0.,
        Color::new(0.16, 0.09, 0.04, 0.45 * a.clamp(0., 1.)),
    );
}

/// The bouncy wordmark on a little gingham picnic blanket.
pub fn logo(tx: &mut Text, l: &Layout, top: f32, t: f32, sway: f32) {
    let size = if l.compact { 40. } else { 64. };
    let line = size * 1.12;
    let wide = crate::text::width_in(Face::Ink, "Picnic", size).max(crate::text::width_in(
        Face::Ink,
        "Panic!",
        size,
    ));
    let pad = size * 0.45;
    let plate = Rect::new(
        (l.screen.x - wide) * 0.5 - pad,
        top - pad * 0.7,
        wide + pad * 2.,
        line * 2. + pad * 1.2,
    );
    // Gingham border squares, a cream middle and a soft drop shadow.
    shapes::drop_shadow(
        plate,
        18.,
        Shadow::soft(c(0x1a1006, 0.4))
            .offset(vec2(0., 8.))
            .blur(22.),
    );
    shapes::fill_rounded_rect(plate, 18., rgb(0xfff4dc));
    let cell = (size * 0.22).max(8.);
    let nx = (plate.w / cell) as i32;
    let ny = (plate.h / cell) as i32;
    for i in 0..nx {
        for j in 0..ny {
            let edge = i == 0 || j == 0 || i == nx - 1 || j == ny - 1;
            if edge && (i + j) % 2 == 0 {
                let x = plate.x + i as f32 * plate.w / nx as f32;
                let y = plate.y + j as f32 * plate.h / ny as f32;
                shapes::fill_rounded_rect(
                    Rect::new(x, y, plate.w / nx as f32, plate.h / ny as f32),
                    2.,
                    c(RED, 0.85),
                );
            }
        }
    }
    let inner = Rect::new(
        plate.x + cell,
        plate.y + cell,
        plate.w - cell * 2.,
        plate.h - cell * 2.,
    );
    shapes::fill_rounded_rect(
        inner,
        10.,
        Fill::vertical(inner, rgb(0xfffaf0), rgb(0xfbefd2)),
    );
    let colors = [rgb(0xe0594b), rgb(0x3b2a22)];
    for (row, word) in ["Picnic", "Panic!"].iter().enumerate() {
        let total = crate::text::width_in(Face::Ink, word, size);
        let x0 = (l.screen.x - total) * 0.5;
        for (k, (i, ch)) in word.char_indices().enumerate() {
            let prefix = crate::text::width_in(Face::Ink, &word[..i], size);
            let s = &word[i..i + ch.len_utf8()];
            let hop = (t * 3.2 - (k + row * 6) as f32 * 0.45).sin().max(0.) * size * 0.12 * sway;
            let y = top + row as f32 * line - hop;
            // A chunky drop shadow, then the letter.
            tx.ink(
                vec2(x0 + prefix + size / 16., y + size / 12.),
                0.,
                s,
                size,
                c(0x7a2e24, 0.35),
                Align::Left,
            );
            tx.ink(vec2(x0 + prefix, y), 0., s, size, colors[row], Align::Left);
        }
    }
    let sub = if l.compact { BODY } else { LARGE };
    tx.label(
        vec2(0., plate.y + plate.h + 14.),
        l.screen.x,
        "A small raccoon. A very long to-do list.",
        sub,
        rgb(0xfff4dc),
        Align::Center,
    );
}

/// A five-pointed star drawn with immediate triangles (fonts lack the glyph).
pub fn star(center: Vec2, r: f32, fill: Color, outline: Color) {
    let mut pts = [Vec2::ZERO; 10];
    for (k, p) in pts.iter_mut().enumerate() {
        let a = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.;
        let rr = if k % 2 == 0 { r } else { r * 0.45 };
        *p = center + vec2(a.cos(), a.sin()) * rr;
    }
    for k in 0..10 {
        draw_triangle(center, pts[k], pts[(k + 1) % 10], fill);
    }
    shapes::polyline(
        &pts,
        shapes::Stroke::new((r * 0.12).max(2.), outline)
            .join(shapes::Join::Round)
            .closed(true),
    );
}

pub fn title(m: &mut Menu, l: &Layout, meta: &Meta, saved: Option<u8>, age: f32) {
    let w = l.screen.x;
    let h = l.screen.y;
    let top = if l.short { 18. } else { h * 0.12 };
    // A soft vignette behind the wordmark keeps it readable over the park.
    shapes::fill_rounded_rect(
        Rect::new(0., 0., w, top + if l.compact { 140. } else { 220. }),
        0.,
        Fill::vertical(
            Rect::new(0., 0., w, top + 200.),
            Color::new(0.2, 0.1, 0.04, 0.42),
            Color::new(0.2, 0.1, 0.04, 0.),
        ),
    );
    logo(m.tx, l, top, m.t, m.sway);
    let bw = (w - 40.).min(300.);
    let bh = if l.short { 42. } else { 50. };
    let gap = if l.short { 8. } else { 12. };
    let n = if saved.is_some() { 3. } else { 2. };
    let by = (h - (bh + gap) * n - if l.short { 46. } else { 90. })
        .max(top + if l.compact { 140. } else { 200. });
    let x = (w - bw) * 0.5;
    let mut y = by;
    let mut k = 0;
    let mut row = |m: &mut Menu, id: u64, label: &str, kind: ButtonKind| {
        let look = Entrance::fade_up(16.)
            .with_stagger(0.06)
            .sample(age, k, m.motion);
        let r = kit::appear(Rect::new(x, y, bw, bh), look);
        m.button(id, r, label, true, kind);
        y += bh + gap;
        k += 1;
    };
    if let Some(mask) = saved {
        let mut buf = [0_u8; 48];
        let label = crate::hud::fmt(
            &mut buf,
            format_args!("Continue the day ({}/{PRANKS})", mask.count_ones()),
        );
        row(m, id::CONTINUE, label, ButtonKind::Primary);
        row(m, id::NEW_DAY, "Start a new day", ButtonKind::Secondary);
    } else {
        row(m, id::NEW_DAY, "Start the day", ButtonKind::Primary);
    }
    row(m, id::WARDROBE, "Wardrobe", ButtonKind::Secondary);
    // Records strip.
    let mut buf = [0_u8; 96];
    let line = if meta.best_ms > 0 {
        let s = meta.best_ms / 1000;
        let stars = "★★★"
            .chars()
            .take(meta.best_stars as usize)
            .collect::<StarStr>();
        crate::hud::fmt(
            &mut buf,
            format_args!(
                "Best day {:02}:{:02}  {}   ·   {} days panicked",
                s / 60,
                s % 60,
                stars.as_str(),
                meta.clears
            ),
        )
    } else if meta.runs > 0 {
        "No picnic has been fully panicked. Yet."
    } else if l.compact {
        "Drag to sneak about. Tap the buttons to make mischief."
    } else {
        "WASD move · Space jump · E grab · F throw · C chitter · Shift slide"
    };
    m.tx.label(
        vec2(12., h - if l.short { 26. } else { 40. }),
        w - 24.,
        line,
        if l.compact { SMALL * 1.5 } else { BODY },
        rgb(0xfff4dc),
        Align::Center,
    );
}

/// Tiny fixed-capacity string for star glyphs (no allocation).
pub struct StarStr {
    buf: [u8; 12],
    len: usize,
}
impl StarStr {
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}
impl FromIterator<char> for StarStr {
    fn from_iter<I: IntoIterator<Item = char>>(iter: I) -> Self {
        let mut s = Self {
            buf: [0; 12],
            len: 0,
        };
        for ch in iter {
            let mut b = [0_u8; 4];
            let e = ch.encode_utf8(&mut b);
            if s.len + e.len() <= s.buf.len() {
                s.buf[s.len..s.len + e.len()].copy_from_slice(e.as_bytes());
                s.len += e.len();
            }
        }
        s
    }
}

pub fn wardrobe(m: &mut Menu, l: &Layout, meta: &Meta, age: f32) {
    let w = l.screen.x;
    let h = l.screen.y;
    dim(l, (age / 0.2).min(1.));
    let cw = (w - 24.).min(560.);
    let cols = if cw < 420. { 2 } else { 3 };
    let rows = Hat::ALL.len().div_ceil(cols);
    let bh = if l.short { 64. } else { 86. };
    let ch = rows as f32 * (bh + 8.) + 120.;
    let cx = (w - cw) * 0.5;
    let cy = ((h - ch) * 0.5).max(12.);
    let look = Entrance::fade_up(14.).sample(age, 0, m.motion);
    let inner = kit::card(
        kit::appear(Rect::new(cx, cy, cw, ch), look),
        Some("Wardrobe"),
        &theme().faded(look.alpha),
    );
    let bw = (inner.w - 8. * (cols as f32 - 1.)) / cols as f32;
    for (k, hat) in Hat::ALL.iter().enumerate() {
        let r = Rect::new(
            inner.x + (k % cols) as f32 * (bw + 8.),
            inner.y + 4. + (k / cols) as f32 * (bh + 8.),
            bw,
            bh,
        );
        let owned = meta.has_hat(*hat);
        let worn = meta.selected_hat() == *hat;
        m.button(
            id::HAT + k as u64,
            r,
            "",
            owned,
            if worn {
                ButtonKind::Primary
            } else {
                ButtonKind::Secondary
            },
        );
        let ink = if worn { rgb(0xfff6e2) } else { rgb(INK) };
        m.tx.ink(
            vec2(r.x + 6., r.y + 10.),
            r.w - 12.,
            if owned { hat.name() } else { "Locked" },
            BODY,
            if owned { ink } else { c(INK, 0.5) },
            Align::Center,
        );
        m.tx.wrapped(
            Face::Ink,
            vec2(r.x + 8., r.y + 32.),
            r.w - 16.,
            2,
            if worn { "Wearing it!" } else { hat.unlock() },
            SMALL * 1.5,
            if worn { c(0xfff6e2, 0.9) } else { c(INK, 0.7) },
            Align::Center,
        );
    }
    let bw2 = 180_f32.min(inner.w);
    m.button(
        id::BACK,
        Rect::new(cx + (cw - bw2) * 0.5, inner.y + inner.h - 50., bw2, 44.),
        "Back",
        true,
        ButtonKind::Secondary,
    );
}

pub const PAUSE_QUIPS: [&str; 5] = [
    "The picnic waits. The pie does not.",
    "Even master criminals need a breather.",
    "Nana has not noticed. Probably.",
    "Plotting. Scheming. Snacking.",
    "Somewhere, a tuba sighs.",
];

pub fn paused(m: &mut Menu, l: &Layout, meta: &Meta, quip: &str, age: f32, done: usize) {
    let w = l.screen.x;
    let h = l.screen.y;
    dim(l, (age / 0.2).min(1.));
    let cw = (w - 24.).min(420.);
    let bh = if l.short { 38. } else { 46. };
    let rows = 5.;
    let ch = 96. + rows * (bh + 8.) + 8.;
    let cx = (w - cw) * 0.5;
    let cy = ((h - ch) * 0.5).max(8.);
    let look = Entrance::fade_up(14.).sample(age, 0, m.motion);
    let inner = kit::card(
        kit::appear(Rect::new(cx, cy, cw, ch), look),
        Some("Paused"),
        &theme().faded(look.alpha),
    );
    m.tx.ink(
        vec2(inner.x, inner.y + 4.),
        inner.w,
        quip,
        BODY,
        c(INK, 0.8 * look.alpha),
        Align::Center,
    );
    let mut buf = [0_u8; 40];
    m.tx.ink(
        vec2(inner.x, inner.y + 26.),
        inner.w,
        crate::hud::fmt(
            &mut buf,
            format_args!("{done} of {PRANKS} pranks crossed off"),
        ),
        SMALL * 1.5,
        c(INK, 0.6 * look.alpha),
        Align::Center,
    );
    let mut y = inner.y + 52.;
    let mut b = |m: &mut Menu, id: u64, label: &str, kind: ButtonKind| {
        m.button(id, Rect::new(inner.x, y, inner.w, bh), label, true, kind);
        y += bh + 8.;
    };
    b(m, id::RESUME, "Resume", ButtonKind::Primary);
    b(
        m,
        id::GHOST,
        if meta.ghost {
            "Best-run ghost: shown"
        } else {
            "Best-run ghost: hidden"
        },
        ButtonKind::Secondary,
    );
    b(
        m,
        id::CONES,
        if meta.cones {
            "Vision cones: shown"
        } else {
            "Vision cones: hidden"
        },
        ButtonKind::Secondary,
    );
    b(m, id::RESTART, "Restart the day", ButtonKind::Secondary);
    b(m, id::TITLE, "Save and leave", ButtonKind::Danger);
}

pub fn touch_pause(m: &mut Menu, l: &Layout) {
    let r = l.pause_button();
    let rr = Rect::new(r.min[0], r.min[1], r.size[0], r.size[1]);
    let _ =
        m.ui.button(id::TOUCH_PAUSE, r, "", true, kit::INVISIBLE_BUTTON);
    shapes::fill_rounded_rect(rr, 12., c(PAPER, 0.85));
    for dx in [14., 24.] {
        shapes::fill_rounded_rect(Rect::new(rr.x + dx, rr.y + 12., 6., 20.), 2., rgb(INK));
    }
}

pub fn notes_toggle(m: &mut Menu, l: &Layout) {
    let _ =
        m.ui.button(id::NOTES, l.notes_tab(), "", true, kit::INVISIBLE_BUTTON);
}

pub struct Summary {
    pub seconds: f32,
    pub scruffs: u32,
    pub bonuses: u8,
    pub shinies: u32,
    pub shiny_total: usize,
    pub stars: u8,
    pub best: bool,
    pub hat: Option<Hat>,
    pub rolls: [RollCounter; 3],
}

impl Summary {
    pub fn new(
        seconds: f32,
        scruffs: u32,
        bonuses: u8,
        shinies: u32,
        shiny_total: usize,
        best: bool,
        hat: Option<Hat>,
    ) -> Self {
        let ms = (seconds * 1000.) as u64;
        let mut rolls = [RollCounter::new(0); 3];
        rolls[0].set_target(seconds as i64);
        rolls[1].set_target(i64::from(scruffs));
        rolls[2].set_target(i64::from(shinies));
        Self {
            seconds,
            scruffs,
            bonuses,
            shinies,
            shiny_total,
            stars: crate::meta::stars(ms, scruffs),
            best,
            hat,
            rolls,
        }
    }
    pub fn advance(&mut self, dt: f32, motion: Motion, age: f32) {
        for (k, r) in self.rolls.iter_mut().enumerate() {
            if age > 0.5 + k as f32 * 0.35 {
                r.advance(dt, motion);
            }
        }
    }
}

pub fn results(m: &mut Menu, l: &Layout, s: &Summary, age: f32) {
    let w = l.screen.x;
    let h = l.screen.y;
    dim(l, (age / 0.4).min(1.));
    let cw = (w - 24.).min(520.);
    let compact = l.short || l.compact && h < 600.;
    let listed = if compact {
        0
    } else {
        s.bonuses.count_ones() as usize
    };
    let ch = if compact {
        (h - 16.).min(330.)
    } else {
        360. + listed as f32 * 18. + if s.hat.is_some() { 30. } else { 0. }
    };
    let cx = (w - cw) * 0.5;
    let cy = ((h - ch) * 0.5).max(8.);
    let look = Entrance::pop(0.92).sample(age, 0, m.motion);
    let inner = kit::card(
        kit::appear(Rect::new(cx, cy, cw, ch), look),
        Some("Picnic: thoroughly panicked"),
        &theme().faded(look.alpha),
    );
    // Stars pop in one by one.
    let star_size = if compact { 30. } else { 44. };
    let sy = inner.y + 4.;
    for k in 0..3 {
        let t = ((age - 0.6 - k as f32 * 0.3) / 0.25).clamp(0., 1.);
        let lit = k < s.stars as usize;
        let scale = if m.sway > 0. {
            0.6 + 0.4 * t + 0.2 * (1. - (t * 2. - 1.).abs())
        } else {
            1.
        };
        let x = inner.x + inner.w * 0.5 + (k as f32 - 1.) * star_size * 1.5;
        let on = lit && t > 0.;
        star(
            vec2(x, sy + star_size * 0.5),
            star_size * 0.5 * if on { scale } else { 0.9 },
            if on { rgb(0xffc94a) } else { c(INK, 0.12) },
            if on { rgb(0xc98a1a) } else { c(INK, 0.25) },
        );
    }
    let mut y = sy + star_size + if compact { 6. } else { 14. };
    let row = if compact { 24. } else { 30. };
    let mut line = |m: &mut Menu, label: &str, value: std::fmt::Arguments, accent: bool| {
        m.tx.ink(
            vec2(inner.x + 8., y),
            inner.w * 0.6,
            label,
            BODY,
            rgb(INK),
            Align::Left,
        );
        m.tx.ink_fmt(
            vec2(inner.x + inner.w * 0.5, y),
            inner.w * 0.5 - 8.,
            BODY,
            if accent { rgb(RED) } else { rgb(INK) },
            Align::Right,
            value,
        );
        y += row;
    };
    let shown = s.rolls[0].shown();
    line(
        m,
        "Time",
        format_args!(
            "{:02}:{:02}{}",
            shown / 60,
            shown % 60,
            if s.best { "  New best!" } else { "" }
        ),
        s.best,
    );
    line(
        m,
        "Times scruffed",
        format_args!("{}", s.rolls[1].shown()),
        false,
    );
    line(
        m,
        "Bottle caps",
        format_args!("{} / {}", s.rolls[2].shown(), s.shiny_total),
        false,
    );
    line(
        m,
        "Hidden bonuses",
        format_args!("{} / {}", s.bonuses.count_ones(), Bonus::ALL.len()),
        false,
    );
    if !compact {
        for b in Bonus::ALL {
            if s.bonuses & b.bit() != 0 {
                m.tx.ink_fmt(
                    vec2(inner.x + 18., y - 6.),
                    inner.w - 24.,
                    SMALL * 1.5,
                    c(INK, 0.7),
                    Align::Left,
                    format_args!("★ {} — {}", b.title(), b.detail()),
                );
                y += 18.;
            }
        }
    }
    if let Some(hat) = s.hat {
        let mut buf = [0_u8; 64];
        m.tx.ink(
            vec2(inner.x, y + 2.),
            inner.w,
            crate::hud::fmt(&mut buf, format_args!("New hat unlocked: {}!", hat.name())),
            BODY,
            rgb(0x3f7fc1),
            Align::Center,
        );
    }
    let bh = 46.;
    let bw = (inner.w - 8.) * 0.5;
    let by = inner.y + inner.h - bh - 2.;
    m.button(
        id::AGAIN,
        Rect::new(inner.x, by, bw, bh),
        "Another day",
        true,
        ButtonKind::Primary,
    );
    m.button(
        id::RESULTS_TITLE,
        Rect::new(inner.x + bw + 8., by, bw, bh),
        "Title",
        true,
        ButtonKind::Secondary,
    );
    let _ = (LARGE, HUGE, Shadow::soft(Color::new(0., 0., 0., 0.)));
}
