//! Title, wardrobe, pause and results cards built from the engine UI kit.
//!
//! Input always goes through the engine [`Ui`] (stable IDs, keyboard/pad
//! focus, touch capture); the kit only paints. Entrances, button motion and
//! roll-up counters honour reduced motion through [`Motion`]. Copy stays to
//! headlines, button labels and names; stats are pictograms and numbers.
#![allow(clippy::too_many_arguments)]
use crate::hud::{GOLD, INK, LEAF, PAPER, RED, TICK, c, theme};
use crate::layout::Layout;
use crate::meta::{Meta, STAR_MS};
use crate::models::{Hat, rgb};
use crate::sim::pranks::{Bonus, PRANKS};
use crate::text::{Align, BODY, Face, LARGE, SMALL, Text};
use ferravex_play::tween::{Entrance, Motion, RollCounter};
use ferravex_render::icons::{self, Icon, IconStyle};
use ferravex_render::prelude::*;
use ferravex_render::shapes::{self, Fill, Shadow};
use ferravex_render::text::{TextStyle, draw_text_styled, text_width};
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
    /// A kit button; returns how far its face sank (press feedback) so
    /// decorations drawn on it can follow.
    pub fn button(
        &mut self,
        id: WidgetId,
        r: Rect,
        label: &str,
        enabled: bool,
        kind: ButtonKind,
    ) -> f32 {
        let _ = self
            .ui
            .button(id, ur(r), "", enabled, kit::INVISIBLE_BUTTON);
        let state = kit::ui_button_state(self.ui, id, enabled);
        let m = &mut self.buttons.motion[(id as usize).min(id::COUNT - 1)];
        m.update(state, self.dt, self.motion);
        let th = theme();
        kit::button(r, label, kind, m, &th);
        2. * m.press()
    }

    /// A kit button whose label is led by a pictogram, the pair centred.
    /// Returns the press sink like [`Menu::button`].
    pub fn icon_button(
        &mut self,
        id: WidgetId,
        r: Rect,
        icon: Icon,
        label: &str,
        enabled: bool,
        kind: ButtonKind,
    ) -> f32 {
        let sink = self.button(id, r, "", enabled, kind);
        let th = theme();
        let ink = if !enabled {
            th.disabled
        } else if matches!(kind, ButtonKind::Primary | ButtonKind::Danger) {
            th.accent_ink
        } else {
            th.ink
        };
        let mut style = TextStyle::modern(th.body, ink);
        if matches!(kind, ButtonKind::Secondary | ButtonKind::Ghost) {
            style = style.with_shadow(Color::new(0., 0., 0., 0.35));
        }
        let size = th.body + 4.;
        let gap = if label.is_empty() { 0. } else { 8. };
        let total = size + gap + text_width(label, style);
        let center = r.center() + vec2(0., sink);
        let x = center.x - total * 0.5;
        icons::draw(icon, vec2(x + size * 0.5, center.y), size, ink);
        draw_text_styled(
            label,
            x + size + gap,
            style.centered_baseline(center.y),
            style,
        );
        sink
    }
}

fn dim(l: &Layout, a: f32) {
    shapes::fill_rounded_rect(
        Rect::new(0., 0., l.screen.x, l.screen.y),
        0.,
        Color::new(0.16, 0.09, 0.04, 0.45 * a.clamp(0., 1.)),
    );
}

/// A row of to-do pips centred on `center`: filled once crossed off.
fn pips(center: Vec2, mask: u8, step: f32, radius: f32, done: Color, todo: Color) {
    let x0 = center.x - step * (PRANKS as f32 - 1.) * 0.5;
    for k in 0..PRANKS {
        let at = vec2(x0 + k as f32 * step, center.y);
        if mask & (1 << k) != 0 {
            shapes::fill_circle(at, radius, done);
        } else {
            shapes::stroke_circle(at, radius - 0.5, 1.5, todo);
        }
    }
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
            let s = &word[i..i + ch.len_utf8()];
            // Measure through this letter so the font's letter spacing
            // survives ("Picnic" used to read "Ficnic").
            let prefix = crate::text::width_in(Face::Ink, &word[..i + ch.len_utf8()], size)
                - crate::text::width_in(Face::Ink, s, size);
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

/// Default bindings as key caps, each with one word (title screen only).
const CONTROLS: [(&str, &str); 6] = [
    ("WASD", "Move"),
    ("Space", "Jump"),
    ("E", "Grab"),
    ("F", "Throw"),
    ("C", "Chitter"),
    ("Shift", "Slide"),
];

fn controls_strip(m: &mut Menu, l: &Layout, cy: f32) {
    let th = theme();
    let cap = |key: &str| crate::text::width_in(Face::Ink, key, th.body) + kit::SPACE[2] * 2.;
    let word = |w: &str| crate::text::width_in(Face::Modern, w, BODY);
    let total: f32 = CONTROLS
        .iter()
        .map(|(k, w)| cap(k).max(23.) + 6. + word(w))
        .sum::<f32>()
        + 18. * (CONTROLS.len() as f32 - 1.);
    let mut x = (l.screen.x - total) * 0.5;
    for (key, label) in CONTROLS {
        let r = kit::keycap(x, cy - 12., key, &th);
        x += r.w + 6.;
        m.tx.label(
            vec2(x, cy - BODY * 0.5),
            0.,
            label,
            BODY,
            rgb(0xfff4dc),
            Align::Left,
        );
        x += word(label) + 18.;
    }
}

/// The best day so far: time, its stars and how many days were panicked.
fn records_strip(m: &mut Menu, l: &Layout, meta: &Meta, cy: f32) {
    let size = if l.compact { SMALL * 1.5 } else { BODY };
    let icon = size + 6.;
    let s = meta.best_ms / 1000;
    let mut buf = [0_u8; 16];
    let time = crate::hud::fmt(&mut buf, format_args!("{:02}:{:02}", s / 60, s % 60));
    let mut buf2 = [0_u8; 16];
    let days = crate::hud::fmt(&mut buf2, format_args!("{}", meta.clears));
    let tw = crate::text::width_in(Face::Modern, time, size);
    let dw = crate::text::width_in(Face::Modern, days, size);
    let star_w = icon * 3. + 8.;
    let gap = 26.;
    let total = icon + 6. + tw + gap + star_w + gap + icon + 6. + dw;
    let mut x = (l.screen.x - total) * 0.5;
    let cream = rgb(0xfff4dc);
    let shadow = Color::new(0., 0., 0., 0.4);
    icons::draw_styled(
        Icon::Hourglass,
        vec2(x + icon * 0.5, cy),
        icon,
        IconStyle::new(cream).shadow(shadow),
    );
    x += icon + 6.;
    m.tx.label(vec2(x, cy - size * 0.5), 0., time, size, cream, Align::Left);
    x += tw + gap;
    for k in 0..3 {
        let lit = k < meta.best_stars;
        icons::draw_styled(
            Icon::Star,
            vec2(x + icon * 0.5, cy),
            icon,
            IconStyle::new(if lit { rgb(0xffc94a) } else { c(0xfff4dc, 0.3) }).shadow(shadow),
        );
        x += icon + 4.;
    }
    x += gap - 4.;
    icons::draw_styled(
        Icon::Calendar,
        vec2(x + icon * 0.5, cy),
        icon,
        IconStyle::new(cream).shadow(shadow),
    );
    x += icon + 6.;
    m.tx.label(vec2(x, cy - size * 0.5), 0., days, size, cream, Align::Left);
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
    let mut row = |m: &mut Menu, id: u64, icon: Icon, label: &str, kind: ButtonKind| {
        let look = Entrance::fade_up(16.)
            .with_stagger(0.06)
            .sample(age, k, m.motion);
        let r = kit::appear(Rect::new(x, y, bw, bh), look);
        let sink = m.icon_button(id, r, icon, label, true, kind);
        y += bh + gap;
        k += 1;
        (r, sink)
    };
    if let Some(mask) = saved {
        // Continue shows the saved list as pips on the button itself.
        let (r, sink) = row(m, id::CONTINUE, Icon::Play, "Continue", ButtonKind::Primary);
        pips(
            vec2(r.x + r.w - 52., r.y + r.h * 0.5 + sink),
            mask,
            10.,
            3.6,
            rgb(0xfff6e2),
            c(0xfff6e2, 0.7),
        );
        row(
            m,
            id::NEW_DAY,
            Icon::Restart,
            "New day",
            ButtonKind::Secondary,
        );
    } else {
        row(
            m,
            id::NEW_DAY,
            Icon::Play,
            "Start the day",
            ButtonKind::Primary,
        );
    }
    row(
        m,
        id::WARDROBE,
        Icon::Crown,
        "Wardrobe",
        ButtonKind::Secondary,
    );
    // Bottom strip: the best day as pictograms, else the controls as key caps.
    let cy = h - if l.short { 22. } else { 34. };
    if meta.best_ms > 0 {
        records_strip(m, l, meta, cy);
    } else if !l.compact {
        controls_strip(m, l, cy);
    }
}

/// What unlocks each hat, as a pictogram and a number (no sentences).
pub fn unlock_badge(hat: Hat) -> Option<(Icon, &'static str)> {
    match hat {
        Hat::None => None,
        Hat::PieTin => Some((Icon::Check, "1")),
        Hat::TeaCosy => Some((Icon::Check, "All")),
        Hat::PaperCrown => Some((Icon::Hourglass, "<15:00")),
        Hat::FlowerCrown => Some((Icon::Star, "3")),
        Hat::CapCrown => Some((Icon::Coin, "All")),
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
        let sink = m.button(
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
        let r = Rect::new(r.x, r.y + sink, r.w, r.h);
        let ink = if worn { rgb(0xfff6e2) } else { rgb(INK) };
        m.tx.ink(
            vec2(r.x + 6., r.y + 10.),
            r.w - 12.,
            hat.name(),
            BODY,
            if owned { ink } else { c(INK, 0.45) },
            Align::Center,
        );
        let row = vec2(r.center().x, r.y + r.h * 0.62);
        if worn {
            icons::badge(Icon::Check, row, 26., rgb(0xfff6e2), rgb(RED));
        } else if !owned {
            // Locked: a padlock, then what earns the hat.
            if let Some((icon, value)) = unlock_badge(*hat) {
                let size = BODY;
                let vw = crate::text::width_in(Face::Ink, value, size);
                let total = 14. + 8. + size + 4. + 6. + vw;
                let x = row.x - total * 0.5;
                icons::draw(Icon::Lock, vec2(x + 7., row.y), 14., c(INK, 0.4));
                let x = x + 22.;
                icons::draw(
                    icon,
                    vec2(x + (size + 4.) * 0.5, row.y),
                    size + 4.,
                    rgb(match icon {
                        Icon::Coin | Icon::Star => 0xd99a1e,
                        Icon::Hourglass => 0x8a7360,
                        _ => TICK,
                    }),
                );
                m.tx.ink(
                    vec2(x + size + 10., row.y - size * 0.5),
                    0.,
                    value,
                    size,
                    c(INK, 0.75),
                    Align::Left,
                );
            }
        }
    }
    let bw2 = 180_f32.min(inner.w);
    m.icon_button(
        id::BACK,
        Rect::new(cx + (cw - bw2) * 0.5, inner.y + inner.h - 50., bw2, 44.),
        Icon::ArrowLeft,
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

/// An on/off switch drawn at the right end of a toggle button.
fn switch(r: Rect, on: bool, sink: f32) {
    let track = Rect::new(r.x + r.w - 52., r.y + r.h * 0.5 - 10. + sink, 38., 20.);
    shapes::fill_rounded_rect(track, 10., if on { rgb(LEAF) } else { c(INK, 0.2) });
    let knob = vec2(
        if on {
            track.x + track.w - 10.
        } else {
            track.x + 10.
        },
        track.y + 10.,
    );
    shapes::fill_circle(knob, 7.5, rgb(0xfffbf0));
}

/// Pause card: one quip, the day's pips, then actions (toggles show their
/// state as an eye and a switch instead of "shown"/"hidden").
pub fn paused(m: &mut Menu, l: &Layout, meta: &Meta, quip: &str, age: f32, done: u8) {
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
    pips(
        vec2(inner.center().x, inner.y + 34.),
        done,
        18.,
        5.5,
        c(TICK, look.alpha),
        c(INK, 0.35 * look.alpha),
    );
    let mut y = inner.y + 52.;
    let mut b = |m: &mut Menu, id: u64, icon: Icon, label: &str, kind: ButtonKind| {
        let r = Rect::new(inner.x, y, inner.w, bh);
        let sink = m.icon_button(id, r, icon, label, true, kind);
        y += bh + 8.;
        (r, sink)
    };
    b(m, id::RESUME, Icon::Play, "Resume", ButtonKind::Primary);
    let (r, sink) = b(
        m,
        id::GHOST,
        if meta.ghost { Icon::Eye } else { Icon::EyeOff },
        "Best-run ghost",
        ButtonKind::Secondary,
    );
    switch(r, meta.ghost, sink);
    let (r, sink) = b(
        m,
        id::CONES,
        if meta.cones { Icon::Eye } else { Icon::EyeOff },
        "Vision cones",
        ButtonKind::Secondary,
    );
    switch(r, meta.cones, sink);
    b(
        m,
        id::RESTART,
        Icon::Restart,
        "Restart",
        ButtonKind::Secondary,
    );
    b(
        m,
        id::TITLE,
        Icon::Home,
        "Save and leave",
        ButtonKind::Danger,
    );
}

pub fn touch_pause(m: &mut Menu, l: &Layout) {
    let r = l.pause_button();
    let rr = Rect::new(r.min[0], r.min[1], r.size[0], r.size[1]);
    let _ =
        m.ui.button(id::TOUCH_PAUSE, r, "", true, kit::INVISIBLE_BUTTON);
    shapes::fill_rounded_rect(rr, 12., c(PAPER, 0.85));
    icons::draw(Icon::Pause, rr.center(), 24., rgb(INK));
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

/// Width of one bonus chip (star and title).
fn chip_width(title: &str) -> f32 {
    crate::text::width_in(Face::Ink, title, BODY) + 40.
}

/// How many rows the found bonus chips need in `width`.
fn chip_rows(width: f32, bonuses: u8) -> usize {
    let mut rows = 0;
    let mut x = width;
    for b in Bonus::ALL {
        if bonuses & b.bit() == 0 {
            continue;
        }
        let w = chip_width(b.title());
        if x + w > width {
            rows += 1;
            x = 0.;
        }
        x += w + 8.;
    }
    rows
}

/// One results stat: a pictogram over its number on a paper tile.
fn stat_tile(
    m: &mut Menu,
    r: Rect,
    icon: Icon,
    tint: u32,
    size: f32,
    accent: bool,
    value: std::fmt::Arguments,
) {
    shapes::fill_rounded_rect(r, 10., Fill::vertical(r, rgb(0xfbefd2), rgb(0xf6e6c4)));
    shapes::stroke_rounded_rect(
        r,
        10.,
        if accent { 2. } else { 1. },
        if accent { rgb(RED) } else { c(INK, 0.12) },
    );
    let icon_size = (r.h * 0.4).min(28.);
    icons::draw_styled(
        icon,
        vec2(r.center().x, r.y + 6. + icon_size * 0.5),
        icon_size,
        IconStyle::new(rgb(tint)),
    );
    m.tx.ink_fmt(
        vec2(r.x + 4., r.y + r.h - size - 6.),
        r.w - 8.,
        size,
        if accent { rgb(RED) } else { rgb(INK) },
        Align::Center,
        value,
    );
}

pub fn results(m: &mut Menu, l: &Layout, s: &Summary, age: f32) {
    let w = l.screen.x;
    let h = l.screen.y;
    dim(l, (age / 0.4).min(1.));
    let cw = (w - 24.).min(520.);
    let compact = l.short || l.compact && h < 600.;
    let rows = if compact {
        0
    } else {
        chip_rows(cw - 40., s.bonuses)
    };
    let ch = if compact {
        (h - 16.).min(290.)
    } else {
        282. + rows as f32 * 32. + if s.hat.is_some() { 32. } else { 0. }
    };
    let cx = (w - cw) * 0.5;
    let cy = ((h - ch) * 0.5).max(8.);
    let look = Entrance::pop(0.92).sample(age, 0, m.motion);
    let inner = kit::card(
        kit::appear(Rect::new(cx, cy, cw, ch), look),
        Some("Picnic panicked!"),
        &theme().faded(look.alpha),
    );
    // Stars pop in one by one; under each, what earns it.
    let star_size = if compact { 30. } else { 44. };
    let sy = inner.y + 4.;
    let crit = [
        (Icon::Check, None),
        (Icon::Hourglass, Some((STAR_MS / 60_000, "m"))),
        (Icon::Hand, Some((2, ""))),
    ];
    for (k, (icon, rule)) in crit.iter().enumerate() {
        let t = ((age - 0.6 - k as f32 * 0.3) / 0.25).clamp(0., 1.);
        let lit = k < s.stars as usize;
        let scale = if m.sway > 0. {
            0.6 + 0.4 * t + 0.2 * (1. - (t * 2. - 1.).abs())
        } else {
            1.
        };
        let x = inner.x + inner.w * 0.5 + (k as f32 - 1.) * star_size * 2.;
        let on = lit && t > 0.;
        star(
            vec2(x, sy + star_size * 0.5),
            star_size * 0.5 * if on { scale } else { 0.9 },
            if on { rgb(0xffc94a) } else { c(INK, 0.12) },
            if on { rgb(0xc98a1a) } else { c(INK, 0.25) },
        );
        if !compact {
            let ink = if lit { c(INK, 0.75) } else { c(INK, 0.35) };
            let cy = sy + star_size + 10.;
            let mut buf = [0_u8; 12];
            let value = match rule {
                Some((n, unit)) => crate::hud::fmt(&mut buf, format_args!("≤{n}{unit}")),
                None => "",
            };
            let vw = crate::text::width_in(Face::Ink, value, SMALL * 1.5);
            let total = 14. + if value.is_empty() { 0. } else { 4. + vw };
            let x0 = x - total * 0.5;
            icons::draw(*icon, vec2(x0 + 7., cy), 14., ink);
            if !value.is_empty() {
                m.tx.ink(
                    vec2(x0 + 18., cy - 6.),
                    0.,
                    value,
                    SMALL * 1.5,
                    ink,
                    Align::Left,
                );
            }
        }
    }
    // Four stat tiles: time, scruffs, bottle caps, bonus stars.
    let ty = sy + star_size + if compact { 10. } else { 28. };
    let th = if compact { 56. } else { 70. };
    let tw = (inner.w - 3. * 8.) / 4.;
    let size = if compact || tw < 96. { BODY } else { LARGE };
    let tile = |k: usize| Rect::new(inner.x + k as f32 * (tw + 8.), ty, tw, th);
    let shown = s.rolls[0].shown();
    stat_tile(
        m,
        tile(0),
        Icon::Hourglass,
        0x8a7360,
        size,
        s.best,
        format_args!("{:02}:{:02}", shown / 60, shown % 60),
    );
    if s.best {
        // A "new best" sticker on the time tile's corner.
        let r = tile(0);
        let tag = Rect::new(r.x + r.w - 46., r.y - 9., 52., 20.);
        shapes::fill_rounded_rect(tag, 10., rgb(RED));
        icons::draw(
            Icon::Crown,
            vec2(tag.x + 12., tag.y + 10.),
            13.,
            rgb(0xffe08a),
        );
        m.tx.ink(
            vec2(tag.x + 20., tag.y + 4.),
            tag.w - 22.,
            "Best",
            SMALL * 1.5,
            rgb(0xfff6e2),
            Align::Left,
        );
    }
    stat_tile(
        m,
        tile(1),
        Icon::Hand,
        0xc9823a,
        size,
        false,
        format_args!("{}", s.rolls[1].shown()),
    );
    stat_tile(
        m,
        tile(2),
        Icon::Coin,
        GOLD,
        size,
        false,
        format_args!("{}/{}", s.rolls[2].shown(), s.shiny_total),
    );
    stat_tile(
        m,
        tile(3),
        Icon::Star,
        GOLD,
        size,
        false,
        format_args!("{}/{}", s.bonuses.count_ones(), Bonus::ALL.len()),
    );
    // Found bonuses as little named chips.
    let mut y = ty + th + 12.;
    if !compact {
        let left = inner.x + 12.;
        let right = inner.x + inner.w - 12.;
        let mut x = right;
        let mut first = true;
        for b in Bonus::ALL {
            if s.bonuses & b.bit() == 0 {
                continue;
            }
            let cw = chip_width(b.title());
            if x + cw > right {
                if !first {
                    y += 32.;
                }
                first = false;
                x = left;
            }
            let r = Rect::new(x, y, cw, 26.);
            shapes::fill_rounded_rect(r, 13., c(0xffc94a, 0.25));
            icons::draw(Icon::Star, vec2(r.x + 15., r.y + 13.), 16., rgb(0xd99a1e));
            m.tx.ink(
                vec2(r.x + 28., r.y + 5.),
                0.,
                b.title(),
                BODY,
                c(INK, 0.85),
                Align::Left,
            );
            x += cw + 8.;
        }
        if !first {
            y += 34.;
        }
    }
    if let Some(hat) = s.hat {
        let mut buf = [0_u8; 64];
        let text = crate::hud::fmt(&mut buf, format_args!("New hat: {}", hat.name()));
        let tw = crate::text::width_in(Face::Ink, text, BODY);
        let x = inner.center().x - (tw + 30.) * 0.5;
        icons::badge(
            Icon::Crown,
            vec2(x + 11., y + 10.),
            22.,
            rgb(TICK),
            rgb(0xffffff),
        );
        m.tx.ink(
            vec2(x + 30., y + 2.),
            0.,
            text,
            BODY,
            rgb(TICK),
            Align::Left,
        );
    }
    let bh = 46.;
    let bw = (inner.w - 8.) * 0.5;
    let by = inner.y + inner.h - bh - 2.;
    m.icon_button(
        id::AGAIN,
        Rect::new(inner.x, by, bw, bh),
        Icon::Restart,
        "Another day",
        true,
        ButtonKind::Primary,
    );
    m.icon_button(
        id::RESULTS_TITLE,
        Rect::new(inner.x + bw + 8., by, bw, bh),
        Icon::Home,
        "Title",
        true,
        ButtonKind::Secondary,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_hats_show_a_pictogram_not_a_sentence() {
        for hat in Hat::ALL {
            match unlock_badge(hat) {
                None => assert_eq!(hat, Hat::None, "only the bare head is free"),
                Some((_, value)) => assert!(value.len() <= 6 && !value.contains(' ')),
            }
        }
    }

    #[test]
    fn bonus_chips_flow_into_rows() {
        assert_eq!(chip_rows(400., 0), 0);
        assert_eq!(chip_rows(400., Bonus::Basket.bit()), 1);
        let all = Bonus::ALL.iter().fold(0, |m, b| m | b.bit());
        let rows = chip_rows(400., all);
        assert!((2..=Bonus::ALL.len()).contains(&rows), "{rows}");
        assert!(chip_rows(2000., all) < rows);
    }
}
