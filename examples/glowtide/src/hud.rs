//! In-run HUD: drawn after the world/HUD boundary, so it stays crisp and is
//! never bloomed. Panels and meters use the engine UI kit's anti-aliased
//! shapes; icons and light come from the painter; copy from the text queue.
use crate::fx::{Fx, Say};
use crate::gfx::{Painter, Rgba, color, ease, mix3, rgba, with_alpha};
use crate::icons;
use crate::layout::Layout;
use crate::sim::content::*;
use crate::sim::{BossKind, Card, Game};
use crate::text::{Align, BODY, LARGE, SMALL, Text};
use crate::world::View;
use ferravex_play::tween::{Motion, RollCounter, Spring};
use ferravex_render::prelude::{Color, Rect, Vec2, vec2};
use ferravex_render::shapes::{self, Fill};
use ferravex_render::ui::kit::{self, KitTheme};

pub const PAPER: Rgba = [0.94, 0.96, 1., 1.];
pub const MUTED: Rgba = [0.64, 0.73, 0.84, 1.];
pub const CYAN: [f32; 3] = [0.45, 0.95, 1.];
pub const GOLD: [f32; 3] = [1., 0.82, 0.4];
pub const RED: [f32; 3] = [1., 0.36, 0.46];
pub const VIOLET: [f32; 3] = [0.72, 0.6, 1.];

/// Glowtide's kit palette: deep-sea navy surfaces, lantern-cyan accents.
pub fn theme() -> KitTheme {
    KitTheme {
        backdrop: Color::new(0.01, 0.02, 0.05, 0.7),
        surface: Color::from_hex(0x0f1830),
        raised: Color::from_hex(0x1b2a4a),
        header: Color::from_hex(0x22355c),
        ink: Color::from_hex(0xeef4ff),
        muted: Color::from_hex(0x9eb2cc),
        disabled: Color::from_hex(0x56637a),
        accent: Color::from_hex(0x6ff0ff),
        accent_ink: Color::from_hex(0x06142a),
        success: Color::from_hex(0x7ff0a0),
        warning: Color::from_hex(0xffd166),
        danger: Color::from_hex(0xff5d75),
        focus: Color::from_hex(0xbff7ff),
        line: Color::new(0.6, 0.85, 1., 0.16),
        ..KitTheme::default()
    }
}

pub fn r(min: Vec2, size: Vec2) -> Rect {
    Rect::new(min.x, min.y, size.x, size.y)
}

/// Rolling counters and smoothed meters, advanced by presentation time.
pub struct HudState {
    kills: RollCounter,
    coins: RollCounter,
    xp: Spring,
    level: u16,
    hp_ghost: f32,
    hp_hold: f32,
}

impl HudState {
    pub fn new() -> Self {
        Self {
            kills: RollCounter::new(0),
            coins: RollCounter::new(0),
            xp: Spring::critical(0., 9.),
            level: 1,
            hp_ghost: 1.,
            hp_hold: 0.,
        }
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }

    pub fn update(&mut self, game: &Game, dt: f32, motion: Motion) {
        self.kills.set_target(i64::from(game.stats.kills));
        self.coins.set_target(game.stats.coins as i64);
        self.kills.advance(dt, motion);
        self.coins.advance(dt, motion);
        if game.level() != self.level {
            // A level-up empties the bar: restart the fill from zero.
            self.level = game.level();
            self.xp.snap(0.);
        }
        self.xp.set_target(game.xp_fraction());
        self.xp.advance(dt, motion);
        let hp = game.player.hp / game.player.max_hp.max(1.);
        if hp < self.hp_ghost {
            self.hp_hold -= dt;
            if self.hp_hold <= 0. {
                self.hp_ghost = (self.hp_ghost - dt * 0.6).max(hp);
            }
        } else {
            self.hp_ghost = hp;
            self.hp_hold = 0.45;
        }
        if hp < self.hp_ghost && self.hp_hold <= 0. && motion.is_reduced() {
            self.hp_ghost = hp;
        }
    }
}

/// A kit meter with additive glow and a travelling sheen.
#[allow(clippy::too_many_arguments)]
pub fn meter(
    p: &mut Painter,
    min: Vec2,
    size: Vec2,
    frac: f32,
    ghost: Option<f32>,
    c: [f32; 3],
    t: f32,
    glow: f32,
    motion: f32,
) {
    let th = theme();
    kit::meter(r(min, size), frac, ghost, color(rgba(c, 1.)), &th);
    let frac = frac.clamp(0., 1.);
    if frac <= 0. {
        return;
    }
    let w = (size.x * frac).max(size.y);
    p.glow_rect(
        min,
        vec2(w, size.y),
        size.y * 0.5,
        6. + 8. * glow,
        rgba(c, 0.18 + 0.35 * glow),
    );
    if motion > 0. {
        let sheen = min.x + ((t * 0.35) % 1.4 - 0.2) * size.x;
        if sheen > min.x + 8. && sheen < min.x + w - 8. {
            p.rect_h(
                true,
                vec2(sheen - 12., min.y + 1.),
                vec2(12., size.y - 2.),
                rgba([1.; 3], 0.),
                rgba([1.; 3], 0.3),
            );
            p.rect_h(
                true,
                vec2(sheen, min.y + 1.),
                vec2(12., size.y - 2.),
                rgba([1.; 3], 0.3),
                rgba([1.; 3], 0.),
            );
        }
    }
}

/// Anti-aliased surface with an accent hairline and optional additive glow.
pub fn panel(p: &mut Painter, min: Vec2, size: Vec2, radius: f32, accent: [f32; 3], glow: f32) {
    let rect = r(min, size);
    shapes::fill_rounded_rect(
        rect,
        radius,
        Fill::vertical(
            rect,
            Color::new(0.07, 0.11, 0.2, 0.9),
            Color::new(0.03, 0.05, 0.1, 0.92),
        ),
    );
    shapes::stroke_rounded_rect(rect, radius, 1.5, color(rgba(accent, 0.55)));
    if glow > 0. {
        p.glow_rect(min, size, radius, 14., rgba(accent, 0.22 * glow));
    }
}

fn mmss(seconds: f32) -> (u32, u32) {
    let s = seconds.max(0.) as u32;
    (s / 60, s % 60)
}

/// Reserve the numeric HP label within the same corner-safe row as the meter.
fn health_width(l: &Layout, left: f32) -> f32 {
    if l.compact {
        (l.top_right() - left - 108.).clamp(50., 150.)
    } else {
        230.
    }
}

#[allow(clippy::too_many_arguments)]
pub fn draw(
    p: &mut Painter,
    tx: &mut Text,
    state: &HudState,
    l: &Layout,
    game: &Game,
    fx: &Fx,
    view: &View,
    t: f32,
    touch: bool,
) {
    let w = l.screen.x;
    let compact = l.compact;
    let motion = fx.motion();
    let left = if touch { 60. } else { 12. };
    // --- XP bar along the top, clear of the fullscreen corner.
    let xp_y = 12.;
    let badge = vec2(76., 26.);
    panel(p, vec2(left, xp_y - 4.), badge, 13., CYAN, fx.level_glow);
    tx.big_fmt(
        vec2(left, xp_y + 2.),
        badge.x,
        BODY,
        PAPER,
        Align::Center,
        format_args!("LV{}", game.level()),
    );
    let bar_x = left + badge.x + 10.;
    let bar_w = (l.top_right() - bar_x).max(40.);
    meter(
        p,
        vec2(bar_x, xp_y + 2.),
        vec2(bar_w, 12.),
        state.xp.value,
        None,
        CYAN,
        t,
        fx.level_glow,
        motion,
    );
    // --- Health under the level badge, with a trailing damage ghost.
    let hp_y = xp_y + 34.;
    let hp_w = health_width(l, left);
    let hp_frac = game.player.hp / game.player.max_hp.max(1.);
    let danger = (1. - hp_frac / 0.3).clamp(0., 1.);
    heart(
        p,
        vec2(left + 10., hp_y + 7.),
        8. + danger * (t * 8.).sin().abs() * 2. * motion,
        RED,
    );
    let hp_color = mix3([0.4, 1., 0.6], RED, (1. - hp_frac).powf(0.6));
    meter(
        p,
        vec2(left + 24., hp_y),
        vec2(hp_w, 14.),
        hp_frac,
        Some(state.hp_ghost),
        hp_color,
        t,
        fx.hurt,
        motion,
    );
    tx.label_fmt(
        vec2(left + 32. + hp_w, hp_y - 1.),
        if compact { 76. } else { 120. },
        BODY,
        MUTED,
        Align::Left,
        format_args!(
            "{:.0}/{:.0}",
            game.player.hp.max(0.).ceil(),
            game.player.max_hp
        ),
    );
    // --- Timer (Classic, punchy) and tide name (Modern), centered.
    let (m, s) = mmss(game.time);
    let timer_y = if compact { hp_y + 26. } else { xp_y + 26. };
    let center_w = 300.;
    let timer_x = (w - center_w) * 0.5;
    let timer_size = if compact { LARGE } else { 32. };
    tx.big_fmt(
        vec2(timer_x, timer_y),
        center_w,
        timer_size,
        PAPER,
        Align::Center,
        format_args!("{m:02}:{s:02}"),
    );
    let tide = &TIDES[game.tide()];
    tx.label(
        vec2(timer_x, timer_y + timer_size + 6.),
        center_w,
        tide.name,
        if compact { SMALL } else { BODY },
        rgba(tide.light, 0.9),
        Align::Center,
    );
    // --- Rolling kills and glimmer, right side below the corner.
    let right = w - 12.;
    let stat_y = (if compact { hp_y + 22. } else { xp_y + 36. }).max(66.);
    let box_w = 140.;
    let mut buf = [0u8; 16];
    let kills = crate::menus::fmt_into(&mut buf, format_args!("{}", state.kills.shown()));
    let kx = right - crate::text::width_in(crate::text::Face::Classic, kills, BODY);
    skull(p, vec2(kx - 14., stat_y + 7.), 7.);
    tx.big(
        vec2(right - box_w, stat_y),
        box_w,
        kills,
        BODY,
        PAPER,
        Align::Right,
    );
    let mut buf = [0u8; 16];
    let coins = crate::menus::fmt_into(&mut buf, format_args!("{}", state.coins.shown()));
    let gx = right - crate::text::width_in(crate::text::Face::Classic, coins, BODY);
    gem(p, vec2(gx - 13., stat_y + 29.), 6., GOLD);
    tx.big(
        vec2(right - box_w, stat_y + 22.),
        box_w,
        coins,
        BODY,
        rgba(GOLD, 1.),
        Align::Right,
    );
    // --- Weapon and passive slots, bottom-left.
    let slot = if compact { 30. } else { 42. };
    let gap = 6.;
    let base_y = l.screen.y
        - slot
        - 12.
        - if compact {
            slot * 0.75 + 6.
        } else {
            slot * 0.7 + 8.
        };
    for (i, ws) in game.weapons.iter().enumerate() {
        let at = vec2(12. + i as f32 * (slot + gap), base_y);
        match ws {
            Some(ws) => {
                let c = ws.id.info().color;
                panel(
                    p,
                    at,
                    Vec2::splat(slot),
                    8.,
                    if ws.evolved { GOLD } else { c },
                    if ws.evolved { 0.6 } else { 0. },
                );
                icons::weapon(
                    p,
                    ws.id,
                    at + Vec2::splat(slot * 0.5),
                    slot * 0.36,
                    t,
                    ws.evolved,
                );
                pips(
                    at + vec2(4., slot - 6.),
                    slot - 8.,
                    ws.level,
                    WEAPON_MAX,
                    if ws.evolved { GOLD } else { c },
                );
            }
            None => shapes::fill_rounded_rect(
                r(at, Vec2::splat(slot)),
                8.,
                Color::new(0.05, 0.07, 0.12, 0.45),
            ),
        }
    }
    let ps = slot * 0.7;
    let py = base_y + slot + 6.;
    for (i, pa) in game.passives.iter().enumerate() {
        let at = vec2(12. + i as f32 * (ps + gap), py);
        match pa {
            Some((id, level)) => {
                panel(p, at, Vec2::splat(ps), 6., id.info().color, 0.);
                icons::passive(p, *id, at + Vec2::splat(ps * 0.5), ps * 0.34, t);
                pips(
                    at + vec2(3., ps - 5.),
                    ps - 6.,
                    *level,
                    PASSIVE_MAX,
                    id.info().color,
                );
            }
            None => shapes::fill_rounded_rect(
                r(at, Vec2::splat(ps)),
                6.,
                Color::new(0.05, 0.07, 0.12, 0.35),
            ),
        }
    }
    // --- Boss bar.
    if let Some(boss) = game.boss() {
        let kind = if boss.kind == FoeKind::Gulpjaw {
            BossKind::Gulpjaw
        } else {
            BossKind::MotherBrine
        };
        let bw = (w * 0.5).clamp(220., 560.);
        let by = timer_y + timer_size + if compact { 26. } else { 34. };
        let bx = (w - bw) * 0.5;
        let c = boss.kind.info().color;
        tx.label(
            vec2(bx, by),
            bw,
            kind.name(),
            BODY,
            rgba(c, 1.),
            Align::Center,
        );
        meter(
            p,
            vec2(bx, by + 22.),
            vec2(bw, 12.),
            boss.hp / boss.max_hp,
            None,
            c,
            t,
            fx.boss_flash,
            motion,
        );
    }
    // --- Callouts: newest on top, fading.
    let mut cy = l.screen.y * if compact { 0.27 } else { 0.24 };
    for call in fx.callouts.iter().flatten() {
        callout(tx, l, vec2(0., cy), call.say, call.alpha());
        cy += if compact { 44. } else { 56. };
    }
    if let Some((boss, age)) = fx.boss_intro {
        banner(p, tx, l, boss, age, motion);
    }
    // --- Elite labels above their bearers.
    for f in &game.foes {
        if f.elite == 0 || !view.visible(f.pos, 40.) {
            continue;
        }
        let at = view.to_screen(f.pos) - vec2(130., f.radius * view.scale + 34.);
        let name = ELITE_NAMES[usize::from(f.elite - 1) % ELITE_NAMES.len()];
        tx.label(at, 260., name, SMALL, rgba(GOLD, 0.95), Align::Center);
        let frac = f.hp / f.max_hp;
        shapes::fill_rounded_rect(
            r(at + vec2(100., 12.), vec2(60., 4.)),
            2.,
            Color::new(0., 0., 0., 0.6),
        );
        shapes::fill_rounded_rect(
            r(at + vec2(100., 12.), vec2(60. * frac, 4.)),
            2.,
            color(rgba(GOLD, 1.)),
        );
    }
    vignette(p, l, fx, hp_frac);
    if touch {
        touch_controls(p, tx, l, game, t, motion);
    }
}

fn pips(at: Vec2, w: f32, level: u8, max: u8, c: [f32; 3]) {
    let step = w / f32::from(max);
    for k in 0..max {
        let lit = k < level;
        shapes::fill_rounded_rect(
            r(
                at + vec2(k as f32 * step, 0.),
                vec2((step - 1.5).max(1.), 3.),
            ),
            1.,
            if lit {
                color(rgba(c, 1.))
            } else {
                Color::new(0.2, 0.25, 0.35, 0.8)
            },
        );
    }
}

pub fn heart(p: &mut Painter, at: Vec2, r: f32, c: [f32; 3]) {
    p.disc(
        false,
        at + vec2(-r * 0.45, -r * 0.2),
        r * 0.55,
        rgba(c, 1.),
        12,
    );
    p.disc(
        false,
        at + vec2(r * 0.45, -r * 0.2),
        r * 0.55,
        rgba(c, 1.),
        12,
    );
    p.tri(
        false,
        at + vec2(-r * 0.98, 0.),
        at + vec2(r * 0.98, 0.),
        at + vec2(0., r * 0.95),
        rgba(c, 1.),
        rgba(c, 1.),
        rgba(c, 1.),
    );
    p.glow(at, r * 2.5, rgba(c, 0.3));
}

pub fn skull(p: &mut Painter, at: Vec2, r: f32) {
    let c = [0.85, 0.9, 1.];
    p.disc(false, at + vec2(0., -r * 0.15), r, rgba(c, 1.), 12);
    p.rect(
        false,
        at + vec2(-r * 0.55, r * 0.4),
        vec2(r * 1.1, r * 0.6),
        rgba(c, 1.),
    );
    p.disc(
        false,
        at + vec2(-r * 0.38, -r * 0.1),
        r * 0.28,
        [0.05, 0.06, 0.1, 1.],
        6,
    );
    p.disc(
        false,
        at + vec2(r * 0.38, -r * 0.1),
        r * 0.28,
        [0.05, 0.06, 0.1, 1.],
        6,
    );
}

pub fn gem(p: &mut Painter, at: Vec2, r: f32, c: [f32; 3]) {
    p.quad(
        false,
        [
            at + vec2(0., -r * 1.2),
            at + vec2(r, 0.),
            at + vec2(0., r * 1.2),
            at + vec2(-r, 0.),
        ],
        [
            rgba([1., 1., 0.9], 1.),
            rgba(c, 1.),
            rgba(c, 1.),
            rgba(c, 1.),
        ],
    );
    p.glow(at, r * 3., rgba(c, 0.35));
}

fn callout(tx: &mut Text, l: &Layout, at: Vec2, say: Say, a: f32) {
    let w = l.screen.x;
    let size = if l.compact { BODY } else { LARGE };
    let sub = if l.compact { SMALL } else { BODY };
    let sub_at = at + vec2(0., size + 6.);
    let head =
        |tx: &mut Text, s: &str, c: [f32; 3]| tx.big(at, w, s, size, rgba(c, a), Align::Center);
    let tail =
        |tx: &mut Text, s: &str| tx.label(sub_at, w, s, sub, with_alpha(MUTED, a), Align::Center);
    match say {
        Say::Tide(tide) => {
            let t = &TIDES[usize::from(tide).min(TIDES.len() - 1)];
            tx.big_fmt(
                at,
                w,
                size,
                rgba(t.light, a),
                Align::Center,
                format_args!("TIDE SHIFT: {}", t.name),
            );
            tail(
                tx,
                [
                    "The water glows.",
                    "The kelp starts whispering.",
                    "Everything is suddenly pink.",
                    "It gets very, very dark.",
                    "The sun remembers you.",
                ][usize::from(tide).min(4)],
            );
        }
        Say::Elite { name, kind } => {
            tx.big_fmt(
                at,
                w,
                size,
                rgba(GOLD, a),
                Align::Center,
                format_args!(
                    "ELITE: {}",
                    ELITE_NAMES[usize::from(name) % ELITE_NAMES.len()]
                ),
            );
            tx.label_fmt(
                sub_at,
                w,
                sub,
                with_alpha(MUTED, a),
                Align::Center,
                format_args!(
                    "A {} with a name. Defeat it for treasure!",
                    kind.info().name
                ),
            );
        }
        Say::EliteDown => {
            head(tx, "ELITE DOWN!", GOLD);
            tail(tx, "Grab the treasure chest ★");
        }
        Say::Swarm(kind) => {
            let (h, s) = match kind {
                0 => ("MOTHLING MIGRATION!", "They are not here for you. Mostly."),
                1 => ("DRIFTLET BLOOM!", "You are surrounded. Politely."),
                2 => ("SNEEL PACK!", "Watch for the glare, then step aside."),
                _ => ("CRAB PARADE!", "Sir Pinchington brought friends."),
            };
            head(tx, h, [1., 0.7, 0.45]);
            tail(tx, s);
        }
        Say::BossWarning(boss) => {
            head(tx, "SOMETHING BIG STIRS BELOW...", RED);
            tail(
                tx,
                if boss == BossKind::Gulpjaw {
                    "The moon is missing. Suspicious."
                } else {
                    "A motherly disappointment approaches."
                },
            );
        }
        Say::BossPhase(boss, phase) => {
            let h = match (boss, phase) {
                (BossKind::MotherBrine, _) => "MOTHER BRINE IS FURIOUS!",
                (BossKind::Gulpjaw, 1) => "LIGHTS OUT!",
                _ => "GULPJAW IS ENRAGED!",
            };
            head(tx, h, RED);
            tail(
                tx,
                if phase >= 2 {
                    "Everything is faster now. Sorry."
                } else {
                    "New tricks incoming."
                },
            );
        }
        Say::BossDown(boss) => {
            head(
                tx,
                if boss == BossKind::Gulpjaw {
                    "THE ANGLERKING FALLS!"
                } else {
                    "MOTHER BRINE DISSOLVES!"
                },
                GOLD,
            );
            tail(
                tx,
                if boss == BossKind::Gulpjaw {
                    "Dawn breaks over Glowtide."
                } else {
                    "Into perfectly nice sea foam."
                },
            );
        }
        Say::Evolved(wid) => {
            tx.big_fmt(
                at,
                w,
                size,
                rgba(GOLD, a),
                Align::Center,
                format_args!("EVOLVED: {}!", wid.info().evolved),
            );
            tail(tx, wid.info().evolved_summary);
        }
        Say::SecondWind => {
            head(tx, "SECOND WIND!", [1., 0.95, 0.7]);
            tail(tx, "Not today, ocean.");
        }
        Say::Charge => head(tx, "CHARGE! MOVE ASIDE!", RED),
        Say::Lure => {
            head(tx, "THE LURE PULLS...", CYAN);
            tail(tx, "Swim away from the light.");
        }
    }
}

fn banner(p: &mut Painter, tx: &mut Text, l: &Layout, boss: BossKind, age: f32, motion: f32) {
    let w = l.screen.x;
    let inn = ease::out_cubic((age / 0.5).min(1.));
    let out = ((3.4 - age) / 0.5).clamp(0., 1.);
    let a = inn.min(out);
    let y = l.screen.y * 0.38;
    let h = if l.compact { 74. } else { 100. };
    let slide = (1. - inn) * w * 0.4 * motion;
    let band = r(vec2(0., y), vec2(w, h));
    shapes::fill_rounded_rect(
        band,
        0.,
        Fill::horizontal(
            band,
            Color::new(0.02, 0., 0.05, 0.),
            Color::new(0.02, 0., 0.05, 0.85 * a),
        ),
    );
    let half = r(vec2(w * 0.5, y), vec2(w * 0.5, h));
    shapes::fill_rounded_rect(
        half,
        0.,
        Fill::horizontal(
            half,
            Color::new(0.02, 0., 0.05, 0.85 * a),
            Color::new(0.02, 0., 0.05, 0.),
        ),
    );
    let c = if boss == BossKind::Gulpjaw {
        [0.5, 0.95, 1.]
    } else {
        [1., 0.5, 0.85]
    };
    p.rect_h(
        true,
        vec2(0., y),
        vec2(w, 2.),
        rgba(c, 0.),
        rgba(c, 0.8 * a),
    );
    p.rect_h(
        true,
        vec2(0., y + h - 2.),
        vec2(w, 2.),
        rgba(c, 0.8 * a),
        rgba(c, 0.),
    );
    let size = if l.compact { LARGE } else { 40. };
    tx.big(
        vec2(slide, y + 14.),
        w,
        boss_name_caps(boss),
        size,
        rgba(c, a),
        Align::Center,
    );
    tx.label(
        vec2(-slide, y + 26. + size),
        w,
        boss.title(),
        if l.compact { SMALL } else { BODY },
        with_alpha(PAPER, a),
        Align::Center,
    );
}

fn boss_name_caps(boss: BossKind) -> &'static str {
    match boss {
        BossKind::MotherBrine => "MOTHER BRINE",
        BossKind::Gulpjaw => "OLD GULPJAW",
    }
}

fn vignette(p: &mut Painter, l: &Layout, fx: &Fx, hp: f32) {
    let s = l.screen;
    let center = s * 0.5;
    let reach = s.length() * 0.6;
    let low =
        (1. - hp / 0.35).clamp(0., 1.) * (0.5 + 0.5 * (fx.time * 4.).sin().abs() * fx.flashes());
    let hurt = (fx.hurt * fx.flashes()).max(low * 0.6);
    if hurt > 0.01 {
        p.ring(
            false,
            center,
            reach * 0.55,
            reach * 1.05,
            rgba(RED, 0.),
            rgba(RED, 0.4 * hurt),
            48,
        );
    }
    if fx.flash > 0.01 {
        p.rect(true, Vec2::ZERO, s, rgba([1., 0.95, 0.85], 0.3 * fx.flash));
    }
    if fx.victory_glow > 0.01 {
        p.ring(
            true,
            center,
            0.,
            reach,
            rgba(GOLD, 0.25 * fx.victory_glow),
            rgba(GOLD, 0.),
            48,
        );
    }
}

fn touch_controls(p: &mut Painter, tx: &mut Text, l: &Layout, game: &Game, t: f32, motion: f32) {
    let (center, radius) = l.dash_button();
    let ready = game.player.dash_cd <= 0.;
    let c = game.character().color;
    shapes::fill_circle(
        center,
        radius,
        Fill::radial(
            center,
            radius,
            Color::new(0.06, 0.09, 0.16, 0.6),
            Color::new(0.03, 0.05, 0.1, 0.4),
        ),
    );
    shapes::stroke_circle(
        center,
        radius - 1.5,
        3.,
        color(rgba(c, if ready { 0.95 } else { 0.35 })),
    );
    if !ready {
        let total = 1.5 * game.mods.dash_cd;
        let prog = 1. - game.player.dash_cd / total.max(0.01);
        shapes::stroke_arc(
            center,
            radius - 7.,
            -std::f32::consts::FRAC_PI_2,
            prog * std::f32::consts::TAU,
            shapes::Stroke::new(4., color(rgba(c, 0.85))).cap(shapes::Cap::Round),
        );
    } else {
        p.glow(
            center,
            radius * 1.6,
            rgba(c, 0.18 + 0.08 * (t * 3.).sin() * motion),
        );
    }
    tx.label(
        center - vec2(radius, 8.),
        radius * 2.,
        "Dash",
        BODY,
        if ready { PAPER } else { MUTED },
        Align::Center,
    );
    let pb = l.pause_button();
    let at = vec2(pb.min[0], pb.min[1]);
    shapes::fill_rounded_rect(
        r(at, vec2(pb.size[0], pb.size[1])),
        10.,
        Color::new(0.06, 0.09, 0.16, 0.75),
    );
    shapes::fill_rounded_rect(r(at + vec2(15., 11.), vec2(5., 18.)), 2., color(PAPER));
    shapes::fill_rounded_rect(r(at + vec2(25., 11.), vec2(5., 18.)), 2., color(PAPER));
}

/// Floating stick drawn where the finger actually is.
pub fn stick(p: &mut Painter, origin: Vec2, knob: Vec2, c: [f32; 3]) {
    shapes::fill_circle(
        origin,
        56.,
        Fill::radial(
            origin,
            56.,
            Color::new(0.06, 0.09, 0.16, 0.2),
            Color::new(0.06, 0.09, 0.16, 0.45),
        ),
    );
    shapes::stroke_circle(origin, 55., 2., color(rgba(c, 0.6)));
    let d = (knob - origin).clamp_length_max(56.);
    shapes::fill_circle(
        origin + d,
        22.,
        Fill::radial(
            origin + d,
            22.,
            Color::new(1., 1., 1., 0.65),
            color(rgba(c, 0.6)),
        ),
    );
    p.glow(origin + d, 40., rgba(c, 0.25));
}

/// Floating damage/heal/glimmer numbers, projected to screen space.
pub fn numbers(tx: &mut Text, fx: &Fx, view: &View) {
    let motion = fx.motion();
    for n in &fx.numbers {
        if !n.live {
            continue;
        }
        let rise = ease::out_cubic((n.age / 0.6).min(1.)) * 26. * motion;
        let at = view.to_screen(n.pos) - vec2(60., 18. + rise);
        let a = (1. - (n.age - 0.45) / 0.3).clamp(0., 1.);
        let (c, size) = match n.style {
            1 => ([0.55, 1., 0.6], BODY),
            2 => (GOLD, BODY),
            _ if n.crit => ([1., 0.85, 0.35], if n.amount >= 100. { 32. } else { LARGE }),
            _ => ([0.95, 0.97, 1.], if n.amount >= 60. { LARGE } else { BODY }),
        };
        let size = ((size + n.pop * 8. * motion) / 8.).round() * 8.;
        match n.style {
            1 | 2 => tx.big_fmt(
                at,
                120.,
                size,
                rgba(c, a),
                Align::Center,
                format_args!("+{:.0}", n.amount),
            ),
            _ if n.crit => tx.big_fmt(
                at,
                120.,
                size,
                rgba(c, a),
                Align::Center,
                format_args!("{:.0}!", n.amount),
            ),
            _ => tx.big_fmt(
                at,
                120.,
                size,
                rgba(c, a),
                Align::Center,
                format_args!("{:.0}", n.amount),
            ),
        }
    }
}

/// Describe what picking a card does, for the level-up screen.
pub fn card_line(game: &Game, card: Card) -> &'static str {
    match card {
        Card::Weapon(w) => {
            let next = game.card_next_level(card);
            w.info().levels[usize::from(next.min(WEAPON_MAX))]
        }
        Card::Passive(p) => p.info().summary,
        Card::Snack => "Restore 30% of your HP.",
        Card::Pebble => "+25 Glimmer for the Shrine.",
    }
}

#[cfg(test)]
mod responsive_tests {
    use super::*;
    #[test]
    fn health_label_and_stats_clear_the_fullscreen_corner() {
        for size in [
            [320., 480.],
            [390., 844.],
            [844., 390.],
            [1280., 800.],
            [1600., 1000.],
        ] {
            let l = Layout::new(size);
            for left in [12., 60.] {
                let end = left + 32. + health_width(&l, left) + if l.compact { 76. } else { 120. };
                assert!(end <= l.top_right(), "HP row fits {size:?}, left {left}");
            }
        }
    }
}
