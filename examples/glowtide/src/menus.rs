//! Menus and modal overlays. Interaction stays in the engine `Ui` (keyboard,
//! gamepad, mouse and touch focus/activation) through invisible buttons; the
//! visuals use the UI kit (`kit::button` with eased `ButtonMotion`, AA shapes),
//! `Entrance` staggers and `RollCounter` roll-ups, all honoring reduced motion.
use crate::gfx::{Painter, color, ease, mix3, rgba, scale3, with_alpha};
use crate::hud::{self, CYAN, GOLD, MUTED, PAPER, RED, VIOLET, panel, r, theme};
use crate::icons;
use crate::layout::Layout;
use crate::meta::Meta;
use crate::sim::content::*;
use crate::sim::{Card, ChestItem, Game, Phase};
use crate::text::{Align, BODY, HUGE, LARGE, SMALL, Text};
use ferravex_play::tween::{Appearance, Entrance, Motion, RollCounter};
use ferravex_render::prelude::{Color, Vec2, vec2};
use ferravex_render::shapes::{self, Fill, Shadow};
use ferravex_render::ui::kit::{self, ButtonKind, ButtonMotion};
use ferravex_render::ui::{Ui, WidgetId};

pub mod id {
    pub const DIVE: u64 = 10;
    pub const SHRINE: u64 = 11;
    pub const CHARACTER: u64 = 20;
    pub const TIDE: u64 = 24;
    pub const GO: u64 = 25;
    pub const BACK: u64 = 26;
    pub const BUY: u64 = 30;
    pub const SHRINE_BACK: u64 = 38;
    pub const RESUME: u64 = 40;
    pub const RESTART: u64 = 41;
    pub const GIVE_UP: u64 = 42;
    pub const TITLE: u64 = 43;
    pub const CARD: u64 = 50;
    pub const REROLL: u64 = 54;
    pub const BANISH: u64 = 55;
    pub const CONTINUE: u64 = 60;
    pub const AGAIN: u64 = 70;
    pub const RESULTS_SHRINE: u64 = 71;
    pub const RESULTS_TITLE: u64 = 72;
    pub const TOUCH_PAUSE: u64 = 80;
    pub const COUNT: usize = 96;
}

/// Eased hover/press/focus state for every button id.
pub struct Buttons {
    motion: [ButtonMotion; id::COUNT],
}

impl Buttons {
    pub fn new() -> Self {
        Self {
            motion: [ButtonMotion::default(); id::COUNT],
        }
    }
    /// Snap when a screen opens so buttons do not ease in from stale state.
    pub fn snap(&mut self) {
        self.motion = [ButtonMotion::default(); id::COUNT];
    }
}

pub struct Menu<'a> {
    pub p: &'a mut Painter,
    pub tx: &'a mut Text,
    pub ui: &'a mut Ui,
    pub buttons: &'a mut Buttons,
    pub t: f32,
    pub dt: f32,
    pub motion: Motion,
    /// 0..=1 multiplier for movement flourishes (0 under reduced motion).
    pub sway: f32,
}

impl Menu<'_> {
    /// A kit button whose input goes through the engine `Ui`.
    #[allow(clippy::too_many_arguments)]
    pub fn button(
        &mut self,
        id: WidgetId,
        min: Vec2,
        size: Vec2,
        label: &str,
        accent: [f32; 3],
        enabled: bool,
        kind: ButtonKind,
    ) {
        let _ = self.ui.button(
            id,
            crate::text::rect(min, size),
            "",
            enabled,
            kit::INVISIBLE_BUTTON,
        );
        let state = kit::ui_button_state(self.ui, id, enabled);
        let m = &mut self.buttons.motion[(id as usize).min(id::COUNT - 1)];
        m.update(state, self.dt, self.motion);
        let mut th = theme();
        th.accent = color(rgba(accent, 1.));
        th.focus = color(rgba(mix3(accent, [1.; 3], 0.5), 1.));
        th.danger = color(rgba(accent, 1.));
        if enabled {
            let heat = m.hover().max(m.focus());
            if heat > 0.01 {
                let pulse = 0.8 + 0.2 * (self.t * 5.).sin() * self.sway;
                self.p
                    .glow_rect(min, size, 10., 18., rgba(accent, 0.32 * heat * pulse));
            }
        }
        kit::button(r(min, size), label, kind, m, &th);
    }

    /// Register an invisible hit target; returns its eased "hot" amount.
    pub fn target(&mut self, id: WidgetId, min: Vec2, size: Vec2, enabled: bool) -> f32 {
        let _ = self.ui.button(
            id,
            crate::text::rect(min, size),
            "",
            enabled,
            kit::INVISIBLE_BUTTON,
        );
        let state = kit::ui_button_state(self.ui, id, enabled);
        let m = &mut self.buttons.motion[(id as usize).min(id::COUNT - 1)];
        m.update(state, self.dt, self.motion);
        m.hover().max(m.focus())
    }

    /// A tall upgrade/character card: AA shadowed surface, accent header band.
    pub fn card_frame(
        &mut self,
        min: Vec2,
        size: Vec2,
        accent: [f32; 3],
        hot: f32,
        glow: f32,
        alpha: f32,
    ) {
        let rect = r(min, size);
        let radius = 16.;
        let heat = hot.max(glow * 0.6);
        if heat > 0.01 {
            let pulse = 0.85 + 0.15 * (self.t * 4.).sin() * self.sway;
            self.p.glow_rect(
                min,
                size,
                radius,
                28.,
                rgba(accent, 0.34 * heat * pulse * alpha),
            );
        }
        shapes::drop_shadow(
            rect,
            radius,
            Shadow::soft(Color::new(0., 0., 0., 0.5 * alpha))
                .offset(vec2(0., 8.))
                .blur(26.),
        );
        shapes::fill_rounded_rect(
            rect,
            radius,
            Fill::vertical(
                rect,
                Color::new(0.1, 0.14, 0.26, 0.97 * alpha),
                Color::new(0.03, 0.05, 0.1, 0.97 * alpha),
            ),
        );
        let band = r(min, vec2(size.x, 46.));
        shapes::fill_rounded_rect(
            band,
            radius,
            Fill::vertical(
                band,
                color(rgba(scale3(accent, 0.5), alpha)),
                color(rgba(scale3(accent, 0.2), alpha)),
            ),
        );
        shapes::fill_rounded_rect(
            r(min + vec2(0., 30.), vec2(size.x, 16.)),
            0.,
            color(rgba(scale3(accent, 0.2), alpha)),
        );
        shapes::fill_rounded_rect(
            r(min + vec2(0., 46.), vec2(size.x, 1.)),
            0.,
            color(rgba(accent, 0.4 * alpha)),
        );
        let edge = 1.5 + 1.5 * hot;
        shapes::stroke_rounded_rect(
            rect,
            radius,
            edge,
            color(rgba(
                mix3(accent, [1.; 3], 0.2 * hot),
                (0.55 + 0.45 * hot) * alpha,
            )),
        );
    }
}

pub fn dim(l: &Layout, a: f32) {
    let rect = r(Vec2::ZERO, l.screen);
    shapes::fill_rounded_rect(
        rect,
        0.,
        Fill::vertical(
            rect,
            Color::new(0.01, 0.02, 0.05, 0.72 * a),
            Color::new(0.01, 0.01, 0.03, 0.86 * a),
        ),
    );
}

fn rays(p: &mut Painter, at: Vec2, len: f32, c: [f32; 3], t: f32, a: f32) {
    for k in 0..14 {
        let ang = k as f32 / 14. * std::f32::consts::TAU + t * 0.25;
        let d = vec2(ang.cos(), ang.sin());
        let side = vec2(-d.y, d.x) * 0.09;
        p.tri(
            true,
            at,
            at + (d + side) * len,
            at + (d - side) * len,
            rgba(c, 0.09 * a),
            rgba(c, 0.),
            rgba(c, 0.),
        );
    }
}

fn shifted(min: Vec2, look: Appearance) -> Vec2 {
    min + vec2(look.offset[0], look.offset[1])
}

pub fn spirit(p: &mut Painter, at: Vec2, r: f32, character: usize, t: f32, a: f32) {
    let ch = &CHARACTERS[character.min(CHARACTERS.len() - 1)];
    let c = ch.color;
    let flick = 1. + 0.05 * (t * 21.).sin();
    p.glow(at, r * 6. * flick, rgba(c, 0.35 * a));
    let tail = at + vec2((t * 3.).sin() * r * 0.3, r * 1.9);
    p.tri(
        false,
        at + vec2(-r * 0.75, 0.),
        at + vec2(r * 0.75, 0.),
        tail,
        rgba(c, 0.9 * a),
        rgba(c, 0.9 * a),
        rgba(c, 0.),
    );
    match character {
        1 => {
            for k in 0..10 {
                let ang = k as f32 / 10. * std::f32::consts::TAU;
                let d = vec2(ang.cos(), ang.sin());
                let s = vec2(-d.y, d.x) * r * 0.22;
                p.tri(
                    false,
                    at + d * r * 0.85 + s,
                    at + d * r * 1.45,
                    at + d * r * 0.85 - s,
                    rgba(scale3(c, 0.7), a),
                    rgba([1.; 3], a),
                    rgba(scale3(c, 0.7), a),
                );
            }
        }
        2 => {
            for k in 0..5 {
                let x = (k as f32 - 2.) * r * 0.35;
                p.line(
                    false,
                    at + vec2(x, r * 0.5),
                    at + vec2(x + (t * 5. + k as f32).sin() * r * 0.3, r * 1.9),
                    r * 0.16,
                    rgba(c, 0.7 * a),
                );
            }
        }
        3 => {
            let shell = at + vec2(r * 0.3, -r * 0.45);
            p.disc_grad(
                false,
                shell,
                r,
                rgba([1., 0.75, 0.6], a),
                rgba([0.6, 0.3, 0.25], a),
                24,
            );
            p.arc(
                false,
                shell,
                r * 0.4,
                r * 0.6,
                0.3,
                4.5,
                rgba([0.4, 0.15, 0.1], a),
                rgba([0.4, 0.15, 0.1], a),
                10,
            );
        }
        _ => {}
    }
    p.disc_grad(false, at, r, rgba(mix3(c, [1.; 3], 0.8), a), rgba(c, a), 24);
    p.disc(
        false,
        at + vec2(-r * 0.3, -r * 0.35),
        r * 0.28,
        rgba([1.; 3], 0.8 * a),
        12,
    );
    for s in [-1., 1.] {
        let blink = if (t * 0.7 + character as f32).fract() < 0.04 {
            0.2
        } else {
            1.
        };
        p.ellipse(
            false,
            at + vec2(s * r * 0.34, r * 0.05),
            r * 0.13,
            r * 0.24 * blink,
            0.,
            rgba([0.12, 0.08, 0.15], a),
            rgba([0.12, 0.08, 0.15], a),
            8,
        );
    }
}

// ---------------------------------------------------------------------------

pub fn title(m: &mut Menu, l: &Layout, meta: &Meta, best: Option<(u64, u64)>, age: f32) {
    let w = l.screen.x;
    let h = l.screen.y;
    let a = ease::out_cubic(age / 0.8);
    dim(l, 0.5);
    let cell = (w / 64.).clamp(4., if l.short { 6. } else { 13. }).floor();
    let logo_y = if l.short { h * 0.22 } else { h * 0.28 };
    let palette = [
        [0.45, 0.95, 1.],
        [0.65, 0.7, 1.],
        [1., 0.55, 0.9],
        [1., 0.8, 0.5],
    ];
    m.p.glow(
        vec2(w * 0.5, logo_y),
        cell * 40.,
        rgba([0.4, 0.7, 1.], 0.12 * a),
    );
    crate::text::logo(m.p, vec2(w * 0.5, logo_y), cell, m.t, m.sway, &palette);
    let sub_y = logo_y + cell * 5.5;
    m.tx.label(
        vec2(0., sub_y),
        w,
        "A tiny lantern spirit vs. the entire ocean",
        if l.compact { BODY } else { LARGE },
        with_alpha(PAPER, a),
        Align::Center,
    );
    let bw = if l.compact { 240. } else { 300. };
    let bh = if l.short { 44. } else { 52. };
    let by = if l.short { sub_y + 30. } else { h * 0.56 };
    let list = Entrance::fade_up(18.).with_stagger(0.08);
    let look = list.sample(age - 0.3, 0, m.motion);
    m.button(
        id::DIVE,
        shifted(vec2((w - bw) * 0.5, by), look),
        vec2(bw, bh),
        "Dive in",
        CYAN,
        true,
        ButtonKind::Primary,
    );
    let look = list.sample(age - 0.3, 1, m.motion);
    m.button(
        id::SHRINE,
        shifted(vec2((w - bw) * 0.5, by + bh + 14.), look),
        vec2(bw, bh * 0.85),
        "The Shrine",
        GOLD,
        true,
        ButtonKind::Secondary,
    );
    let foot = h - if l.short { 46. } else { 84. };
    hud::gem(m.p, vec2(w * 0.5 - 100., foot + 8.), 6., GOLD);
    m.tx.label_fmt(
        vec2(w * 0.5 - 88., foot),
        220.,
        BODY,
        rgba(GOLD, 1.),
        Align::Left,
        format_args!("{} Glimmer", meta.glimmer),
    );
    let size = if l.compact { SMALL } else { BODY };
    if let Some((deciseconds, kills)) = best {
        let s = deciseconds / 10;
        m.tx.label_fmt(
            vec2(0., foot + 26.),
            w,
            size,
            MUTED,
            Align::Center,
            format_args!(
                "Best survival {:02}:{:02} · Most kills {}",
                s / 60,
                s % 60,
                kills
            ),
        );
    } else {
        m.tx.label(
            vec2(0., foot + 26.),
            w,
            "Survive the tide. Defeat what waits at its bottom.",
            size,
            MUTED,
            Align::Center,
        );
    }
}

#[derive(Clone, Copy)]
struct SelectionLayout {
    cols: usize,
    gap: f32,
    cw: f32,
    ch: f32,
    top: f32,
    x0: f32,
    y0: f32,
    footer_y: f32,
}

impl SelectionLayout {
    fn new(l: &Layout) -> Self {
        let w = l.screen.x;
        let h = l.screen.y;
        let cols = if l.portrait { 2 } else { 4 };
        let rows = 4 / cols;
        let gap = if l.compact { 10. } else { 18. };
        let cw = (((w - 24.).min(1180.) - gap * (cols as f32 - 1.)) / cols as f32).min(280.);
        let top = if l.portrait {
            66.
        } else if l.short {
            14.
        } else {
            60.
        };
        let y0 = top + if l.short { 56. } else { 84. };
        let button_height = if l.short { 40. } else { 48. };
        let footer_height = if w < 560. {
            button_height * 2. + 8.
        } else {
            button_height
        };
        let ch = ((h - y0 - footer_height - 28. - gap * rows as f32) / rows as f32).min(300.);
        let x0 = (w - cw * cols as f32 - gap * (cols as f32 - 1.)) * 0.5;
        let footer_y = y0 + rows as f32 * (ch + gap) + 6.;
        Self {
            cols,
            gap,
            cw,
            ch,
            top,
            x0,
            y0,
            footer_y,
        }
    }
}

pub fn select(
    m: &mut Menu,
    l: &Layout,
    meta: &Meta,
    selected: usize,
    age: f32,
    notice: Option<(&str, f32)>,
) {
    let w = l.screen.x;
    dim(l, 0.8);
    let geometry = SelectionLayout::new(l);
    let SelectionLayout {
        cols,
        gap,
        cw,
        ch,
        top,
        x0,
        y0,
        footer_y: by,
    } = geometry;
    let dense = ch < 190.;
    m.tx.big(
        vec2(0., top),
        w,
        "CHOOSE YOUR LIGHT",
        if l.compact { LARGE } else { HUGE },
        PAPER,
        Align::Center,
    );
    hud::gem(m.p, vec2(w * 0.5 - 50., top + 47.), 5., GOLD);
    m.tx.big_fmt(
        vec2(w * 0.5 - 38., top + 40.),
        200.,
        BODY,
        rgba(GOLD, 1.),
        Align::Left,
        format_args!("{}", meta.glimmer),
    );
    let cards = Entrance::fade_up(40.).with_stagger(0.07).with_duration(0.4);
    for (i, c) in CHARACTERS.iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let look = cards.sample(age, i, m.motion);
        let base = vec2(x0 + col as f32 * (cw + gap), y0 + row as f32 * (ch + gap));
        let min = shifted(base, look);
        let size = vec2(cw, ch);
        let unlocked = meta.is_unlocked(i);
        let hot = m.target(id::CHARACTER + i as u64, base, size, true);
        let chosen = i == selected;
        let a = look.alpha;
        m.card_frame(
            min,
            size,
            if unlocked { c.color } else { [0.4, 0.45, 0.55] },
            hot.max(if chosen { 0.7 } else { 0. }),
            if chosen { 1. } else { 0. },
            a,
        );
        m.tx.big(
            min + vec2(0., if dense { 7. } else { 12. }),
            cw,
            c.name,
            if cw < 160. { BODY } else { LARGE },
            with_alpha(if unlocked { PAPER } else { MUTED }, a),
            Align::Center,
        );
        m.tx.label(
            min + vec2(0., if dense { 30. } else { 52. }),
            cw,
            c.title,
            if dense { SMALL } else { BODY },
            rgba(c.color, if unlocked { a } else { 0.5 * a }),
            Align::Center,
        );
        let portrait_r = if dense {
            10.
        } else if ch < 290. {
            12.
        } else {
            16.
        };
        let pc = min
            + vec2(
                cw * 0.5,
                if dense {
                    50.
                } else if ch < 290. {
                    88.
                } else {
                    96.
                },
            );
        spirit(
            m.p,
            pc,
            portrait_r,
            i,
            m.t + i as f32,
            if unlocked { a } else { 0.35 * a },
        );
        let wy = pc.y + portrait_r * 2.4;
        icons::weapon(m.p, c.weapon, vec2(min.x + 26., wy + 8.), 11., m.t, false);
        m.tx.label(
            vec2(min.x + 44., wy),
            cw - 50.,
            c.weapon.info().name,
            if dense { SMALL } else { BODY },
            rgba(c.weapon.info().color, a),
            Align::Left,
        );
        if !dense {
            let used = m.tx.wrapped(
                vec2(min.x + 14., wy + 28.),
                cw - 28.,
                2,
                c.quirk,
                BODY,
                with_alpha(PAPER, a),
                Align::Left,
            );
            let flavor_y = wy + 34. + used;
            let flavor_lines = ((min.y + ch - 44. - flavor_y) / 14.).floor().clamp(0., 2.) as usize;
            if flavor_lines > 0 {
                m.tx.wrapped(
                    vec2(min.x + 14., flavor_y),
                    cw - 28.,
                    flavor_lines,
                    c.flavor,
                    SMALL,
                    with_alpha(MUTED, a),
                    Align::Left,
                );
            }
        }
        if !unlocked {
            let lock = min + vec2(cw * 0.5, ch - if dense { 13. } else { 26. });
            shapes::fill_rounded_rect(
                r(lock - vec2(72., 14.), vec2(144., 28.)),
                14.,
                Color::new(0.02, 0.03, 0.06, 0.9 * a),
            );
            hud::gem(m.p, lock - vec2(48., 0.), 5., GOLD);
            m.tx.label_fmt(
                lock - vec2(36., 8.),
                120.,
                if dense { SMALL } else { BODY },
                rgba(GOLD, a),
                Align::Left,
                format_args!("Unlock {}", c.cost),
            );
        } else if chosen {
            m.tx.label(
                vec2(min.x, min.y + ch - if dense { 18. } else { 26. }),
                cw,
                "✓ Selected",
                if dense { SMALL } else { BODY },
                rgba(c.color, a),
                Align::Center,
            );
        }
    }
    let bh = if l.short { 40. } else { 48. };
    let narrow = w < 560.;
    let bw = if narrow {
        (w - 36.) * 0.5
    } else {
        ((w - 48.) / 3.).min(230.)
    };
    let bx = if narrow {
        12.
    } else {
        (w - bw * 3. - 24.) * 0.5
    };
    let lower = if narrow { by + bh + 8. } else { by };
    let tide_label = if meta.quick {
        "Quick tide · 5:00"
    } else {
        "Full tide · 10:00"
    };
    m.button(
        id::BACK,
        vec2(bx, lower),
        vec2(bw, bh),
        "← Back",
        VIOLET,
        true,
        ButtonKind::Ghost,
    );
    m.button(
        id::TIDE,
        vec2(if narrow { bx } else { bx + bw + 12. }, by),
        vec2(if narrow { w - 24. } else { bw }, bh),
        tide_label,
        [0.55, 0.85, 1.],
        true,
        ButtonKind::Secondary,
    );
    m.button(
        id::GO,
        vec2(bx + (bw + 12.) * if narrow { 1. } else { 2. }, lower),
        vec2(bw, bh),
        "Dive! →",
        CYAN,
        meta.is_unlocked(selected),
        ButtonKind::Primary,
    );
    if let Some((msg, a)) = notice {
        m.tx.label(
            vec2(0., lower + bh + 4.),
            w,
            msg,
            BODY,
            rgba(RED, a.min(1.)),
            Align::Center,
        );
    }
}

pub fn shrine(m: &mut Menu, l: &Layout, meta: &Meta, age: f32, flash: Option<(usize, f32)>) {
    let w = l.screen.x;
    let h = l.screen.y;
    dim(l, 0.85);
    let top = if l.portrait {
        66.
    } else if l.short {
        12.
    } else {
        36.
    };
    m.tx.big(
        vec2(0., top),
        w,
        "THE SHRINE OF GLIMMERS",
        if l.compact { LARGE } else { HUGE },
        rgba(GOLD, 1.),
        Align::Center,
    );
    hud::gem(m.p, vec2(w * 0.5 - 50., top + 45.), 5., GOLD);
    m.tx.big_fmt(
        vec2(w * 0.5 - 38., top + 38.),
        200.,
        BODY,
        rgba(GOLD, 1.),
        Align::Left,
        format_args!("{}", meta.glimmer),
    );
    let dense = l.portrait && h < 620.;
    let cols = if l.portrait && !dense { 1 } else { 2 };
    let rows = SHRINE_ITEMS.div_ceil(cols);
    let gap = 10.;
    let rw = if cols == 1 {
        (w - 24.).min(520.)
    } else {
        ((w - 36.) / 2.).min(520.)
    };
    let y0 = top + 66.;
    let rh = (((h - y0 - 80.) / rows as f32) - gap).clamp(40., 64.);
    let x0 = (w - (rw * cols as f32 + gap * (cols as f32 - 1.))) * 0.5;
    let list = Entrance::fade_up(16.).with_stagger(0.035);
    for (i, item) in SHRINE.iter().enumerate() {
        let col = i / rows;
        let row = i % rows;
        let look = list.sample(age, i, m.motion);
        let min = shifted(
            vec2(x0 + col as f32 * (rw + gap), y0 + row as f32 * (rh + gap)),
            look,
        );
        let rank = meta.shrine[i];
        let cost = meta.next_cost(i);
        let afford = cost.is_some_and(|c| meta.glimmer >= c);
        let glow = flash.filter(|(k, _)| *k == i).map_or(0., |(_, a)| a);
        panel(
            m.p,
            min,
            vec2(rw, rh),
            10.,
            if rank > 0 { GOLD } else { [0.4, 0.5, 0.7] },
            glow,
        );
        let big = rh >= 54.;
        let text_w = if dense { rw - 16. } else { rw - 160. };
        m.tx.label(
            min + vec2(
                if dense { 8. } else { 14. },
                if dense {
                    4.
                } else if big {
                    8.
                } else {
                    5.
                },
            ),
            text_w,
            item.name,
            BODY,
            with_alpha(PAPER, look.alpha),
            Align::Left,
        );
        m.tx.label(
            min + vec2(
                if dense { 8. } else { 14. },
                if dense {
                    21.
                } else if big {
                    28.
                } else {
                    23.
                },
            ),
            text_w,
            item.summary,
            if big && !dense { BODY } else { SMALL },
            with_alpha(MUTED, look.alpha),
            Align::Left,
        );
        for k in 0..if dense { 0 } else { item.max } {
            let lit = k < rank;
            shapes::fill_rounded_rect(
                r(
                    min + vec2(rw - 156. - f32::from(item.max - k) * 14., rh - 12.),
                    vec2(10., 4.),
                ),
                2.,
                if lit {
                    color(rgba(GOLD, 1.))
                } else {
                    Color::new(0.2, 0.25, 0.35, 0.9)
                },
            );
        }
        let bw = 124_f32.min(rw - 16.);
        let bh = if dense { 22. } else { 34. };
        let bmin = vec2(
            min.x + (rw - bw) * 0.5,
            min.y + if dense { rh - bh - 3. } else { (rh - bh) * 0.5 },
        );
        let bmin = if dense {
            bmin
        } else {
            vec2(min.x + rw - bw - 10., bmin.y)
        };
        match cost {
            Some(c) => {
                let mut buf = [0u8; 24];
                let label = fmt_into(&mut buf, format_args!("Buy ♦ {c}"));
                m.button(
                    id::BUY + i as u64,
                    bmin,
                    vec2(bw, bh),
                    label,
                    GOLD,
                    afford,
                    if afford {
                        ButtonKind::Primary
                    } else {
                        ButtonKind::Secondary
                    },
                );
            }
            None => m.tx.label(
                bmin + vec2(0., 9.),
                bw,
                "★ Maxed",
                BODY,
                rgba(GOLD, 1.),
                Align::Center,
            ),
        }
    }
    let bw = 220.;
    m.button(
        id::SHRINE_BACK,
        vec2((w - bw) * 0.5, h - 60.),
        vec2(bw, 44.),
        "← Back",
        VIOLET,
        true,
        ButtonKind::Secondary,
    );
}

/// Format into a stack buffer (no allocation) for button labels.
pub fn fmt_into<'b>(buf: &'b mut [u8], args: std::fmt::Arguments) -> &'b str {
    struct W<'a> {
        buf: &'a mut [u8],
        len: usize,
    }
    impl std::fmt::Write for W<'_> {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            let room = self.buf.len() - self.len;
            let mut n = s.len().min(room);
            while !s.is_char_boundary(n) {
                n -= 1;
            }
            self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
            self.len += n;
            Ok(())
        }
    }
    let mut w = W { buf, len: 0 };
    let _ = std::fmt::write(&mut w, args);
    let len = w.len;
    std::str::from_utf8(&buf[..len]).unwrap_or("")
}

pub fn paused(m: &mut Menu, l: &Layout, game: &Game, quip: &str, age: f32) {
    let w = l.screen.x;
    let h = l.screen.y;
    dim(l, ease::out_cubic(age / 0.2));
    let top = if l.short { 16. } else { h * 0.14 };
    m.tx.big(
        vec2(0., top),
        w,
        "PAUSED",
        if l.compact { HUGE } else { 48. },
        PAPER,
        Align::Center,
    );
    m.tx.label(vec2(0., top + 60.), w, quip, BODY, MUTED, Align::Center);
    let pw = (w - 24.).min(560.);
    let px = (w - pw) * 0.5;
    let py = top + 96.;
    let rows = game.weapon_count().max(1);
    let row_h = if l.short { 24. } else { 34. };
    let look = Entrance::fade_up(14.).sample(age, 0, m.motion);
    let card = kit::card(
        kit::appear(r(vec2(px, py), vec2(pw, rows as f32 * row_h + 64.)), look),
        Some("Your build"),
        &theme().faded(look.alpha),
    );
    let total = game.total_damage().max(1.);
    for (k, ws) in game.weapons.iter().flatten().enumerate() {
        let y = card.y + k as f32 * row_h;
        icons::weapon(
            m.p,
            ws.id,
            vec2(card.x + 10., y + row_h * 0.5),
            row_h * 0.32,
            m.t,
            ws.evolved,
        );
        let name = if ws.evolved {
            ws.id.info().evolved
        } else {
            ws.id.info().name
        };
        m.tx.label_fmt(
            vec2(card.x + 30., y + (row_h - 16.) * 0.5),
            card.w * 0.55,
            BODY,
            PAPER,
            Align::Left,
            format_args!("{name}  Lv {}", ws.level),
        );
        let d = game.stats.damage[ws.id as usize];
        m.tx.label_fmt(
            vec2(card.x + card.w * 0.5, y + (row_h - 16.) * 0.5),
            card.w * 0.5,
            BODY,
            rgba(ws.id.info().color, 1.),
            Align::Right,
            format_args!("{:.0} · {:.0}%", d, d / total * 100.),
        );
    }
    let cols = if w < 560. { 2 } else { 4 };
    let bw = ((w - 24. - 12. * (cols as f32 - 1.)) / cols as f32).min(if l.compact {
        160.
    } else {
        200.
    });
    let bh = if l.short { 40. } else { 46. };
    let button_rows = 4 / cols;
    let by = (card.y + card.h + 30.).min(h - (bh + 10.) * button_rows as f32 - 12.);
    let ids = [id::RESUME, id::RESTART, id::GIVE_UP, id::TITLE];
    let labels = ["Resume", "Restart", "Give up", "Title"];
    let colors = [CYAN, GOLD, RED, VIOLET];
    let kinds = [
        ButtonKind::Primary,
        ButtonKind::Secondary,
        ButtonKind::Danger,
        ButtonKind::Ghost,
    ];
    let total_w = bw * cols as f32 + 12. * (cols as f32 - 1.);
    for k in 0..4 {
        let col = k % cols;
        let row = k / cols;
        let min = vec2(
            (w - total_w) * 0.5 + col as f32 * (bw + 12.),
            by + row as f32 * (bh + 10.),
        );
        m.button(
            ids[k],
            min,
            vec2(bw, bh),
            labels[k],
            colors[k],
            true,
            kinds[k],
        );
    }
}

pub struct CardView {
    pub banish_mode: bool,
    pub age: f32,
}

/// Geometry reserves the action row before dividing space between cards.
/// Dense cards use a short header and two caption lines, including four offers
/// on a 320×480 phone; no content can push the reroll controls offscreen.
#[derive(Clone, Copy)]
struct UpgradeLayout {
    top: f32,
    gap: f32,
    cw: f32,
    ch: f32,
    start: Vec2,
    footer_y: f32,
    dense: bool,
}
impl UpgradeLayout {
    fn new(l: &Layout, count: usize) -> Self {
        let n = count.clamp(1, 4) as f32;
        let w = l.screen.x;
        let h = l.screen.y;
        let top = if l.portrait && h < 620. {
            66.
        } else if l.short {
            10.
        } else {
            (h * 0.08).max(96.)
        };
        let gap = if l.portrait && h < 620. {
            6.
        } else if l.compact {
            10.
        } else {
            18.
        };
        let (cw, ch, start) = if l.portrait {
            let cw = (w - 28.).min(420.);
            let y = top + 86.;
            let reserve = if l.short { 64. } else { 90. };
            let ch = ((h - y - reserve - gap * (n - 1.)) / n).min(150.);
            (cw, ch, vec2((w - cw) * 0.5, y))
        } else {
            let cw = (((w - if l.short { 40. } else { 60. }) / n) - gap).min(if l.short {
                230.
            } else {
                250.
            });
            let ch = if l.short {
                (h - top - 140.).clamp(150., 230.)
            } else {
                330_f32.min(h * 0.5)
            };
            (
                cw,
                ch,
                vec2(
                    (w - cw * n - gap * (n - 1.)) * 0.5,
                    top + if l.short { 76. } else { 116. },
                ),
            )
        };
        let bottom = start.y
            + if l.portrait {
                ch * n + gap * (n - 1.)
            } else {
                ch
            };
        Self {
            top,
            gap,
            cw,
            ch,
            start,
            footer_y: bottom + if l.short { 12. } else { 24. },
            dense: l.portrait && ch < 146.,
        }
    }
}

/// Preserve complete effect copy; captions are used only when body text would
/// exceed the card's reserved content area.
fn card_copy_fit(copy: &str, width: f32, height: f32, preferred: f32) -> (f32, usize) {
    let mut size = preferred;
    loop {
        let style = ferravex_render::text::TextStyle::modern(size, Color::new(1., 1., 1., 1.));
        let lines = ferravex_render::text::wrap_text(copy, width, style).count();
        if size <= SMALL || lines as f32 * style.line_height() <= height {
            return (size, lines.max(1));
        }
        size -= 8.;
    }
}

pub fn level_up(m: &mut Menu, l: &Layout, game: &Game, v: &CardView, keyboard_hint: &str) {
    let w = l.screen.x;
    let h = l.screen.y;
    let a = ease::out_cubic(v.age / 0.25);
    dim(l, a);
    let geometry = UpgradeLayout::new(l, game.offer.cards().len());
    let top = geometry.top;
    let hc = vec2(w * 0.5, top + 22.);
    rays(
        m.p,
        hc,
        w.max(h) * 0.42,
        if v.banish_mode { RED } else { CYAN },
        m.t * m.sway,
        a,
    );
    m.p.glow(hc, 220., rgba(CYAN, 0.22 * a));
    let pop = 1. + (1. - ease::out_elastic((v.age / 0.7).min(1.))) * 0.4 * m.sway;
    let size = (if l.compact { HUGE } else { 48. } * pop / 8.).round() * 8.;
    m.tx.big(
        vec2(0., top + 22. - size * 0.45),
        w,
        if v.banish_mode {
            "BANISH WHICH?"
        } else {
            "LEVEL UP!"
        },
        size,
        rgba(if v.banish_mode { RED } else { CYAN }, a),
        Align::Center,
    );
    m.tx.label_fmt(
        vec2(0., top + 30. + size * 0.6),
        w,
        BODY,
        with_alpha(MUTED, a),
        Align::Center,
        format_args!("Level {} · choose a blessing", game.level()),
    );
    let cards = game.offer.cards();
    let vertical = l.portrait;
    let UpgradeLayout {
        gap, cw, ch, start, ..
    } = geometry;
    let deal = Entrance::fade_up(if vertical { 0. } else { 90. })
        .with_ease(ferravex_play::tween::Ease::BackOut)
        .with_duration(0.42)
        .with_stagger(0.08);
    let deal = if vertical {
        Entrance {
            slide: [80., 0.],
            ..deal
        }
    } else {
        deal
    };
    for (i, &card) in cards.iter().enumerate() {
        let look = deal.sample(v.age - 0.1, i, m.motion);
        let base = if vertical {
            start + vec2(0., i as f32 * (ch + gap))
        } else {
            start + vec2(i as f32 * (cw + gap), 0.)
        };
        let min = shifted(base, look);
        let size = vec2(cw, ch);
        let ca = look.alpha;
        let accent = if v.banish_mode { RED } else { card.color() };
        let hot = m.target(id::CARD + i as u64, base, size, ca > 0.6);
        let level = game.card_next_level(card);
        let is_new = level == 1 && matches!(card, Card::Weapon(_) | Card::Passive(_));
        let evolve_hint = match card {
            Card::Weapon(wid) => level >= WEAPON_MAX && game.passive_level(wid.info().pair) > 0,
            Card::Passive(pid) => game
                .weapons
                .iter()
                .flatten()
                .any(|s| !s.evolved && s.level >= WEAPON_MAX && s.id.info().pair == pid),
            _ => false,
        };
        let lift = -6. * hot * m.sway;
        let min = min + vec2(0., lift);
        if geometry.dense {
            panel(
                m.p,
                min,
                size,
                12.,
                accent,
                if evolve_hint { 0.8 } else { hot },
            );
            let mut key = [0u8; 4];
            m.tx.big(
                min + vec2(10., 9.),
                20.,
                fmt_into(&mut key, format_args!("{}", i + 1)),
                BODY,
                with_alpha(PAPER, ca),
                Align::Center,
            );
            m.tx.label(
                min + vec2(38., 9.),
                cw - 114.,
                card.name(),
                BODY,
                with_alpha(PAPER, ca),
                Align::Left,
            );
            if is_new {
                m.tx.label(
                    min + vec2(cw - 70., 12.),
                    60.,
                    "New!",
                    SMALL,
                    rgba(GOLD, ca),
                    Align::Right,
                );
            } else if matches!(card, Card::Weapon(_) | Card::Passive(_)) {
                m.tx.label_fmt(
                    min + vec2(cw - 76., 12.),
                    66.,
                    SMALL,
                    rgba(accent, ca),
                    Align::Right,
                    format_args!("Lv {} → {}", level - 1, level),
                );
            }
            icons::card(m.p, card, min + vec2(26., ch * 0.67), 13., m.t, false);
            let copy = hud::card_line(game, card);
            let (copy_size, copy_lines) = card_copy_fit(copy, cw - 62., ch - 34., SMALL);
            m.tx.wrapped(
                min + vec2(50., 30.),
                cw - 62.,
                copy_lines,
                copy,
                copy_size,
                with_alpha(PAPER, ca),
                Align::Left,
            );
            if evolve_hint && ch >= 82. {
                m.tx.label(
                    min + vec2(50., ch - 16.),
                    cw - 62.,
                    "★ Find a chest to evolve",
                    SMALL,
                    rgba(GOLD, ca),
                    Align::Left,
                );
            }
            continue;
        }
        m.card_frame(
            min,
            size,
            accent,
            hot,
            if evolve_hint { 0.8 } else { 0. },
            ca,
        );
        let mut key = [0u8; 4];
        let key = fmt_into(&mut key, format_args!("{}", i + 1));
        shapes::fill_rounded_rect(
            r(min + vec2(10., 12.), vec2(22., 22.)),
            6.,
            Color::new(0., 0., 0., 0.35 * ca),
        );
        m.tx.big(
            min + vec2(10., 16.),
            22.,
            key,
            BODY,
            with_alpha(PAPER, 0.8 * ca),
            Align::Center,
        );
        m.tx.label(
            min + vec2(36., 16.),
            cw - 46.,
            card.name(),
            BODY,
            with_alpha(PAPER, ca),
            Align::Center,
        );
        let compact_card = vertical || l.short;
        let icon_at = if l.short && !vertical {
            min + vec2(40., 72.)
        } else if compact_card {
            min + vec2(40., 46. + (ch - 46.) * 0.5)
        } else {
            min + vec2(cw * 0.5, 100.)
        };
        let icon_r = if compact_card { 20. } else { 30. };
        let bounce = (m.t * 3. + i as f32).sin() * 2. * m.sway;
        icons::card(
            m.p,
            card,
            icon_at + vec2(0., bounce),
            icon_r * look.scale,
            m.t,
            false,
        );
        let tx = if compact_card {
            min.x + 76.
        } else {
            min.x + 14.
        };
        let tw = if compact_card { cw - 88. } else { cw - 28. };
        let mut ty = if compact_card {
            min.y + 54.
        } else {
            min.y + 146.
        };
        let align = if compact_card {
            Align::Left
        } else {
            Align::Center
        };
        if is_new {
            let bx = if compact_card {
                tx
            } else {
                tx + tw * 0.5 - 30.
            };
            shapes::fill_rounded_rect(
                r(vec2(bx, ty), vec2(60., 20.)),
                10.,
                color(rgba(GOLD, 0.95 * ca)),
            );
            m.tx.label(
                vec2(bx, ty + 6.),
                60.,
                "New!",
                SMALL,
                [0.12, 0.08, 0.02, ca],
                Align::Center,
            );
        } else if matches!(card, Card::Weapon(_) | Card::Passive(_)) {
            m.tx.label_fmt(
                vec2(tx, ty + 2.),
                tw,
                BODY,
                rgba(accent, ca),
                align,
                format_args!("Lv {} → {}", level - 1, level),
            );
        }
        ty += 28.;
        let (copy_x, copy_w, copy_align) = if l.short && !vertical {
            ty = min.y + 104.;
            (min.x + 14., cw - 28., Align::Center)
        } else {
            (tx, tw, align)
        };
        let copy = hud::card_line(game, card);
        let reserve = if evolve_hint { 30. } else { 8. };
        let (copy_size, copy_lines) = card_copy_fit(copy, copy_w, min.y + ch - ty - reserve, BODY);
        let used = m.tx.wrapped(
            vec2(copy_x, ty),
            copy_w,
            copy_lines,
            copy,
            copy_size,
            with_alpha(PAPER, ca),
            copy_align,
        );
        ty += used + 10.;
        let flavor_limit = if evolve_hint {
            min.y + ch - 30.
        } else {
            min.y + ch - 6.
        };
        let flavor_lines = ((flavor_limit - ty) / 14.).floor().clamp(0., 2.) as usize;
        if flavor_lines > 0 {
            m.tx.wrapped(
                vec2(tx, ty),
                tw,
                flavor_lines,
                card.flavor(),
                SMALL,
                with_alpha(MUTED, ca),
                align,
            );
        }
        if evolve_hint {
            let y = min.y + ch - 24.;
            m.tx.label(
                vec2(min.x, y),
                cw,
                "★ Evolution ready: find a chest!",
                SMALL,
                rgba(GOLD, (0.6 + 0.4 * (m.t * 5.).sin().abs()) * ca),
                Align::Center,
            );
        }
    }
    let by = geometry.footer_y;
    let bw = if l.compact { 150. } else { 200. };
    let bh = if l.short { 38. } else { 44. };
    let mut b1 = [0u8; 24];
    let mut b2 = [0u8; 24];
    let reroll = fmt_into(&mut b1, format_args!("Reroll ({})", game.rerolls));
    let banish = if v.banish_mode {
        "Cancel"
    } else {
        fmt_into(&mut b2, format_args!("× Banish ({})", game.banishes))
    };
    m.button(
        id::REROLL,
        vec2(w * 0.5 - bw - 6., by),
        vec2(bw, bh),
        reroll,
        VIOLET,
        game.rerolls > 0 && !v.banish_mode,
        ButtonKind::Secondary,
    );
    m.button(
        id::BANISH,
        vec2(w * 0.5 + 6., by),
        vec2(bw, bh),
        banish,
        RED,
        game.banishes > 0 || v.banish_mode,
        if v.banish_mode {
            ButtonKind::Danger
        } else {
            ButtonKind::Secondary
        },
    );
    if !l.short {
        m.tx.label(
            vec2(0., by + bh + 12.),
            w,
            keyboard_hint,
            BODY,
            with_alpha(MUTED, 0.8),
            Align::Center,
        );
    }
}

pub fn chest(m: &mut Menu, l: &Layout, game: &Game, age: f32) {
    let w = l.screen.x;
    let h = l.screen.y;
    dim(l, ease::out_cubic(age / 0.3));
    let open = ease::out_back(((age - 0.35) / 0.5).clamp(0., 1.));
    let shake = if age < 0.35 {
        (age * 60.).sin() * 4. * m.sway
    } else {
        0.
    };
    let cy = if l.short { h * 0.3 } else { h * 0.32 };
    let scale = if l.compact { 1. } else { 1.4 };
    crate::world::chest_icon(
        m.p,
        vec2(w * 0.5 + shake, cy),
        scale,
        m.t * m.sway.max(0.2),
        open,
    );
    m.tx.big(
        vec2(
            0.,
            (cy - 110. * scale).max(if l.portrait { 66. } else { 10. }),
        ),
        w,
        "TREASURE!",
        if l.compact { HUGE } else { 48. },
        rgba(GOLD, 1.),
        Align::Center,
    );
    let items = game.chest.items();
    let mut y = cy + 50. * scale;
    let reveal = Entrance::pop(0.85).with_stagger(0.35);
    for (k, item) in items.iter().enumerate() {
        let look = reveal.sample(age - 0.7, k, m.motion);
        if look.alpha <= 0. {
            continue;
        }
        let rw = (w - 30.).min(520.);
        let rh = if l.short && !l.portrait { 38. } else { 48. };
        let rect = kit::appear(r(vec2((w - rw) * 0.5, y), vec2(rw, rh)), look);
        let min = vec2(rect.x, rect.y);
        let (accent, glow) = match item {
            ChestItem::Evolved(_) => (GOLD, 1.),
            ChestItem::Level(card, _) => (card.color(), 0.),
            ChestItem::Snack => ([0.5, 1., 0.55], 0.),
        };
        panel(
            m.p,
            min,
            vec2(rect.w, rect.h),
            12.,
            accent,
            glow * look.alpha,
        );
        let a = look.alpha;
        match *item {
            ChestItem::Evolved(wid) => {
                icons::weapon(m.p, wid, min + vec2(26., rh * 0.5), 14., m.t, true);
                m.tx.label_fmt(
                    min + vec2(48., 16.),
                    rw - 56.,
                    BODY,
                    rgba(GOLD, a),
                    Align::Left,
                    format_args!("Evolved! {} → {}", wid.info().name, wid.info().evolved),
                );
            }
            ChestItem::Level(card, level) => {
                icons::card(m.p, card, min + vec2(26., rh * 0.5), 14., m.t, false);
                m.tx.label_fmt(
                    min + vec2(48., 16.),
                    rw - 56.,
                    BODY,
                    with_alpha(PAPER, a),
                    Align::Left,
                    format_args!("{} → Lv {}", card.name(), level),
                );
            }
            ChestItem::Snack => {
                icons::card(m.p, Card::Snack, min + vec2(26., rh * 0.5), 14., m.t, false);
                m.tx.label(
                    min + vec2(48., 16.),
                    rw - 56.,
                    "A seaweed snack. Delicious.",
                    BODY,
                    with_alpha(PAPER, a),
                    Align::Left,
                );
            }
        }
        y += rh + if l.short && !l.portrait { 8. } else { 10. };
    }
    let done = 0.7 + reveal.total_duration(items.len());
    if age > done {
        m.tx.label_fmt(
            vec2(0., y + 6.),
            w,
            BODY,
            rgba(GOLD, 1.),
            Align::Center,
            format_args!("+{} Glimmer ♦", game.chest.glimmer),
        );
        let bw = 220.;
        m.button(
            id::CONTINUE,
            vec2((w - bw) * 0.5, (y + 34.).min(h - 56.)),
            vec2(bw, 46.),
            "Nice!",
            GOLD,
            true,
            ButtonKind::Primary,
        );
    }
}

pub struct Summary {
    pub title: (&'static str, &'static str),
    pub best_time: bool,
    pub best_kills: bool,
    pub best_level: bool,
    /// Roll-ups for the stat tiles: seconds, level, kills, glimmer.
    pub rolls: [RollCounter; 4],
}

impl Summary {
    pub fn advance(&mut self, dt: f32, motion: Motion, age: f32) {
        for (k, roll) in self.rolls.iter_mut().enumerate() {
            if age > 0.35 + k as f32 * 0.12 {
                roll.advance(dt, motion);
            }
        }
    }
}

pub fn results(m: &mut Menu, l: &Layout, game: &Game, s: &Summary, age: f32) {
    let w = l.screen.x;
    let h = l.screen.y;
    dim(l, ease::out_cubic(age / 0.5));
    let won = game.phase == Phase::Won;
    let head_c = if won { GOLD } else { [0.7, 0.75, 1.] };
    let top = if l.portrait {
        (h * 0.06).max(66.)
    } else if l.short {
        10.
    } else {
        h * 0.06
    };
    if won {
        rays(
            m.p,
            vec2(w * 0.5, top + 26.),
            w * 0.6,
            GOLD,
            m.t * m.sway,
            1.,
        );
    }
    let size = if l.compact { HUGE } else { 48. };
    let head = Entrance::pop(0.6).sample(age, 0, m.motion);
    m.tx.big(
        vec2(0., top),
        w,
        if won { "DAWN RISES!" } else { "SNUFFED OUT" },
        size,
        rgba(head_c, head.alpha),
        Align::Center,
    );
    let ty = top + size + 12.;
    m.tx.label_fmt(
        vec2(0., ty),
        w,
        if l.compact { BODY } else { LARGE },
        rgba(GOLD, head.alpha),
        Align::Center,
        format_args!("★ {} ★", s.title.0),
    );
    m.tx.label(
        vec2(12., ty + 28.),
        w - 24.,
        s.title.1,
        BODY,
        with_alpha(MUTED, head.alpha),
        Align::Center,
    );
    let names = ["Survived", "Level", "Kills", "Glimmer"];
    let colors = [CYAN, VIOLET, RED, GOLD];
    let bests = [s.best_time, s.best_level, s.best_kills, false];
    let cols = if w < 560. { 2 } else { 4 };
    let tw = ((w - 24. - 10. * (cols as f32 - 1.)) / cols as f32).min(180.);
    let th = if l.short { 48. } else { 66. };
    let total_w = tw * cols as f32 + 10. * (cols as f32 - 1.);
    let y0 = ty + 58.;
    let tiles = Entrance::pop(0.8).with_stagger(0.08);
    for k in 0..4 {
        let col = k % cols;
        let row = k / cols;
        let look = tiles.sample(age - 0.2, k, m.motion);
        let rect = kit::appear(
            r(
                vec2(
                    (w - total_w) * 0.5 + col as f32 * (tw + 10.),
                    y0 + row as f32 * (th + 10.),
                ),
                vec2(tw, th),
            ),
            look,
        );
        let min = vec2(rect.x, rect.y);
        let c = colors[k];
        panel(
            m.p,
            min,
            vec2(rect.w, rect.h),
            10.,
            c,
            if bests[k] { 0.8 } else { 0. },
        );
        m.tx.label(
            min + vec2(0., 7.),
            rect.w,
            names[k],
            SMALL,
            rgba(c, look.alpha),
            Align::Center,
        );
        let vy = min.y + if l.short { 20. } else { 28. };
        let vs = if l.short { BODY } else { LARGE };
        let v = s.rolls[k].shown();
        match k {
            0 => m.tx.big_fmt(
                vec2(min.x, vy),
                rect.w,
                vs,
                with_alpha(PAPER, look.alpha),
                Align::Center,
                format_args!("{:02}:{:02}", v / 60, v % 60),
            ),
            3 => m.tx.big_fmt(
                vec2(min.x, vy),
                rect.w,
                vs,
                rgba(GOLD, look.alpha),
                Align::Center,
                format_args!("+{v}"),
            ),
            _ => m.tx.big_fmt(
                vec2(min.x, vy),
                rect.w,
                vs,
                with_alpha(PAPER, look.alpha),
                Align::Center,
                format_args!("{v}"),
            ),
        }
        if bests[k] && !s.rolls[k].is_rolling() && age > 1. {
            let pulse = 0.75 + 0.25 * (m.t * 5.).sin() * m.sway;
            shapes::fill_rounded_rect(
                r(min + vec2(rect.w - 62., -9.), vec2(66., 18.)),
                9.,
                color(rgba(GOLD, pulse)),
            );
            m.tx.label(
                min + vec2(rect.w - 62., -4.),
                66.,
                "New best!",
                SMALL,
                [0.12, 0.08, 0.02, 1.],
                Align::Center,
            );
        }
    }
    let rows = 4usize.div_ceil(cols);
    let mut order = [0usize; WEAPONS];
    let mut n = 0;
    for (i, d) in game.stats.damage.iter().enumerate() {
        if *d > 0. {
            order[n] = i;
            n += 1;
        }
    }
    order[..n].sort_by(|a, b| game.stats.damage[*b].total_cmp(&game.stats.damage[*a]));
    let by = y0 + rows as f32 * (th + 10.) + 8.;
    let bw = (w - 24.).min(640.);
    let bx = (w - bw) * 0.5;
    let row_h = if l.short { 20. } else { 30. };
    let max_rows = (((h - by - 110.) / row_h).floor() as usize).clamp(1, WEAPONS);
    let shown = n.min(max_rows);
    if shown > 0 {
        let look = Entrance::fade_up(16.).sample(age - 0.45, 0, m.motion);
        let card = kit::card(
            kit::appear(r(vec2(bx, by), vec2(bw, shown as f32 * row_h + 64.)), look),
            Some("Damage by weapon"),
            &theme().faded(look.alpha),
        );
        let total = game.total_damage().max(1.);
        let top_d = game.stats.damage[order[0]].max(1.);
        let grow_in = Entrance::fade().with_duration(0.6).with_stagger(0.06);
        for (k, &wi) in order[..shown].iter().enumerate() {
            let wid = WeaponId::ALL[wi];
            let y = card.y + k as f32 * row_h;
            let d = game.stats.damage[wi];
            let evolved = game.weapon(wid).is_some_and(|s| s.evolved);
            let c = wid.info().color;
            icons::weapon(
                m.p,
                wid,
                vec2(card.x + 10., y + row_h * 0.5),
                row_h * 0.3,
                m.t,
                evolved,
            );
            let name = if evolved {
                wid.info().evolved
            } else {
                wid.info().name
            };
            let ts = if l.short { SMALL } else { BODY };
            m.tx.label(
                vec2(card.x + 30., y + (row_h - ts) * 0.5),
                card.w * 0.3,
                name,
                ts,
                with_alpha(PAPER, look.alpha),
                Align::Left,
            );
            let grow = ease::out_cubic(grow_in.sample(age - 0.6, k, Motion::Full).alpha);
            let bar_x = card.x + 30. + card.w * 0.3;
            let bar_w = card.w * 0.36;
            let frac = (d / top_d) as f32 * if m.motion.is_reduced() { 1. } else { grow };
            kit::meter(
                r(vec2(bar_x, y + row_h * 0.3), vec2(bar_w, row_h * 0.4)),
                frac,
                None,
                color(rgba(c, look.alpha)),
                &theme().faded(look.alpha),
            );
            let dps = d / f64::from(game.time.max(1.));
            m.tx.label_fmt(
                vec2(bar_x + bar_w + 8., y + (row_h - ts) * 0.5),
                card.x + card.w - bar_x - bar_w - 8.,
                ts,
                rgba(c, look.alpha),
                Align::Right,
                format_args!("{:.0}% · {:.0}/s", d / total * 100., dps),
            );
        }
    }
    let bw2 = ((w - 44.) / 3.).min(if l.compact { 150. } else { 210. });
    let bh = if l.short { 38. } else { 46. };
    let total_w = bw2 * 3. + 20.;
    let y = h - bh - if l.short { 8. } else { 22. };
    let x = (w - total_w) * 0.5;
    let live = age > 0.6;
    m.button(
        id::RESULTS_TITLE,
        vec2(x, y),
        vec2(bw2, bh),
        "Title",
        VIOLET,
        live,
        ButtonKind::Ghost,
    );
    m.button(
        id::RESULTS_SHRINE,
        vec2(x + bw2 + 10., y),
        vec2(bw2, bh),
        "Shrine ♦",
        GOLD,
        live,
        ButtonKind::Secondary,
    );
    m.button(
        id::AGAIN,
        vec2(x + (bw2 + 10.) * 2., y),
        vec2(bw2, bh),
        if w < 420. {
            "Dive! →"
        } else {
            "Dive again →"
        },
        CYAN,
        live,
        ButtonKind::Primary,
    );
}

pub fn touch_pause(m: &mut Menu, l: &Layout) {
    let r = l.pause_button();
    let _ = m.target(
        id::TOUCH_PAUSE,
        vec2(r.min[0], r.min[1]),
        vec2(r.size[0], r.size[1]),
        true,
    );
}

pub const PAUSE_QUIPS: [&str; 6] = [
    "The ocean will wait. It is very patient.",
    "Take a breath. You are underwater, but still.",
    "The Driftlets are pretending not to notice.",
    "Gulpjaw is also on a snack break.",
    "Hydrate. Ironic, we know.",
    "Your lantern flickers in solidarity.",
];

#[cfg(test)]
mod responsive_tests {
    use super::*;

    #[test]
    fn every_character_and_start_control_fit_small_and_large_screens() {
        for size in [
            [320., 480.],
            [390., 844.],
            [844., 390.],
            [1280., 800.],
            [1600., 1000.],
        ] {
            let l = Layout::new(size);
            let g = SelectionLayout::new(&l);
            assert!(g.ch >= 110., "character cards remain readable: {size:?}");
            assert!(
                g.x0 >= 12.
                    && g.x0 + g.cw * g.cols as f32 + g.gap * (g.cols - 1) as f32 <= size[0] - 11.
            );
            let bh = if l.short { 40. } else { 48. };
            let footer_h = if size[0] < 560. { bh * 2. + 8. } else { bh };
            assert!(
                g.footer_y + footer_h <= size[1] - 12.,
                "start control visible: {size:?}"
            );
            assert!(g.y0 + (4 / g.cols) as f32 * (g.ch + g.gap) <= g.footer_y);
            if l.portrait {
                assert!(g.top >= 60., "title clears fullscreen menu");
            }
        }
    }

    #[test]
    fn four_upgrade_choices_leave_room_for_descriptions_and_actions() {
        for size in [
            [320., 480.],
            [390., 844.],
            [844., 390.],
            [1280., 800.],
            [1600., 1000.],
        ] {
            let l = Layout::new(size);
            for n in 1..=4 {
                let g = UpgradeLayout::new(&l, n);
                let bh = if l.short { 38. } else { 44. };
                assert!(
                    g.footer_y + bh <= size[1] - 12.,
                    "actions fit: {size:?}, {n}"
                );
                assert!(g.ch >= 60., "two caption lines fit: {size:?}, {n}");
                if g.dense {
                    assert!(30. + SMALL * 14. / 8. * 2. <= g.ch);
                }
                if l.portrait {
                    assert!(g.top >= 60.);
                    assert!(g.start.y + g.ch * n as f32 + g.gap * ((n - 1) as f32) < g.footer_y);
                }
            }
        }
    }

    #[test]
    fn actual_menu_controls_are_unclipped_and_clear_the_fullscreen_corner() {
        use ferravex_render::{input::host_frame, ui::UiRole};
        for size in [
            [320., 480.],
            [390., 844.],
            [844., 390.],
            [1280., 800.],
            [1600., 1000.],
        ] {
            let l = Layout::new(size);
            let mut game = Game::new(crate::sim::RunConfig::default());
            for weapon in WeaponId::ALL.into_iter().take(WEAPON_SLOTS) {
                game.apply_card(Card::Weapon(weapon));
            }
            game.stats.damage.fill(20.);
            game.debug_level_up();
            game.offer.count = 4;
            game.offer.cards = [Card::Weapon(WeaponId::WispBolt); 4];
            game.debug_chest();
            let summary = Summary {
                title: crate::meta::title(&game),
                best_time: true,
                best_kills: true,
                best_level: true,
                rolls: std::array::from_fn(|_| RollCounter::new(100)),
            };
            let mut ui = Ui::new();
            ui.set_inspection_enabled(true);
            let mut p = Painter::new();
            let mut tx = Text::new();
            let mut buttons = Buttons::new();
            for screen in 0..7 {
                host_frame(0., size[0] as u32, size[1] as u32, || {
                    ui.begin(l.viewport);
                    let mut m = Menu {
                        p: &mut p,
                        tx: &mut tx,
                        ui: &mut ui,
                        buttons: &mut buttons,
                        t: 3.,
                        dt: 0.,
                        motion: Motion::Reduced,
                        sway: 0.,
                    };
                    match screen {
                        0 => title(&mut m, &l, &Meta::default(), None, 3.),
                        1 => select(&mut m, &l, &Meta::default(), 0, 3., None),
                        2 => shrine(&mut m, &l, &Meta::default(), 3., None),
                        3 => paused(&mut m, &l, &game, PAUSE_QUIPS[0], 3.),
                        4 => level_up(
                            &mut m,
                            &l,
                            &game,
                            &CardView {
                                banish_mode: false,
                                age: 3.,
                            },
                            "Choose a card",
                        ),
                        5 => chest(&mut m, &l, &game, 5.),
                        _ => results(&mut m, &l, &game, &summary, 3.),
                    }
                    let actions: Vec<_> = ui
                        .inspect()
                        .unwrap()
                        .filter(|e| e.role == UiRole::Button)
                        .collect();
                    assert!(!actions.is_empty(), "screen {screen} has actions");
                    for action in actions {
                        let r = action.bounds;
                        let visible = action.visible_bounds.expect("positive visible bounds");
                        for axis in 0..2 {
                            assert!(
                                (r.min[axis] - visible.min[axis]).abs() < 0.01
                                    && (r.size[axis] - visible.size[axis]).abs() < 0.01,
                                "unclipped {size:?} screen {screen}, id {:?}",
                                action.id
                            );
                        }
                        assert!(
                            r.min[0] >= 0. && r.min[1] >= 0. && r.size[0] > 0. && r.size[1] > 0.
                        );
                        assert!(
                            r.min[0] + r.size[0] <= size[0] + 0.01
                                && r.min[1] + r.size[1] <= size[1] + 0.01
                        );
                        assert!(
                            r.min[1] >= crate::layout::CORNER.y
                                || r.min[0] + r.size[0] <= size[0] - crate::layout::CORNER.x,
                            "fullscreen corner: {size:?}, screen {screen}, id {:?}",
                            action.id
                        );
                    }
                });
            }
        }
    }

    #[test]
    fn every_upgrade_effect_fits_in_full_in_four_card_phone_and_landscape_layouts() {
        let mut copies: Vec<&str> = WeaponId::ALL
            .iter()
            .flat_map(|id| id.info().levels)
            .collect();
        copies.extend(PassiveId::ALL.iter().map(|id| id.info().summary));
        copies.extend(["Restore 30% of your HP.", "+25 Glimmer for the Shrine."]);
        for size in [[320., 480.], [390., 844.], [844., 390.]] {
            let l = Layout::new(size);
            let g = UpgradeLayout::new(&l, 4);
            let width = if g.dense { g.cw - 62. } else { g.cw - 28. };
            let height = if g.dense {
                g.ch - 34.
            } else {
                g.ch - 104. - 30.
            };
            for copy in &copies {
                let (font, lines) =
                    card_copy_fit(copy, width, height, if g.dense { SMALL } else { BODY });
                let style =
                    ferravex_render::text::TextStyle::modern(font, Color::new(1., 1., 1., 1.));
                assert_eq!(
                    lines,
                    ferravex_render::text::wrap_text(copy, width, style)
                        .count()
                        .max(1)
                );
                assert!(
                    lines as f32 * style.line_height() <= height,
                    "complete effect fits {size:?}: {copy}"
                );
            }
        }
    }
}
