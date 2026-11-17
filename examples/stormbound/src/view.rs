//! Read-only presentation: a living sea, ships that wear their wounds, shot
//! arcs and a gun deck of dials, meters and ammunition racks. Drawing only
//! reads the model; effects come only from the events a tick accepted.
use crate::{
    Game, Phase,
    content::Tactic,
    layout::{
        AMMO, ANGLE, ANGLE_DOWN, ANGLE_UP, BAR, CARD, FIRE, METER, POWER, POWER_DOWN, POWER_UP,
        RETIRE, RETRY, SAIL, SCENE, SHOP, START, TABLE,
    },
    model::{Ammo, Cause, Event, Preview, Ship, Trace},
};
use ferravex_arcade::shapes::{self, Fill, Shadow, Stroke};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;
use std::f32::consts::PI;

const GOLD: Color = Color::new(0.97, 0.76, 0.37, 1.);
const CREAM: Color = Color::new(0.93, 0.88, 0.72, 1.);
const TEAL: Color = Color::new(0.37, 0.82, 0.75, 1.);
const RED: Color = Color::new(0.96, 0.44, 0.34, 1.);
const STEEL: Color = Color::new(0.62, 0.70, 0.78, 1.);
const SOFT: Color = Color::new(0.76, 0.80, 0.82, 1.);
const FOAM: Color = Color::new(0.82, 0.94, 0.96, 1.);
const FLAME: Color = Color::new(1., 0.6, 0.26, 1.);
const SMOKE: Color = Color::new(0.55, 0.58, 0.62, 1.);
const SPLINTER: Color = Color::new(0.66, 0.47, 0.28, 1.);
const SKY_TOP: Color = Color::new(0.04, 0.07, 0.13, 1.);
const SKY_LOW: Color = Color::new(0.17, 0.25, 0.32, 1.);
const SEA_TOP: Color = Color::new(0.10, 0.31, 0.36, 1.);
const SEA_LOW: Color = Color::new(0.02, 0.10, 0.15, 1.);
const WOOD: Color = Color::new(0.45, 0.32, 0.21, 1.);
const CLOTH: Color = Color::new(0.94, 0.90, 0.78, 1.);
const FORECAST: Color = Color::new(0.62, 0.92, 0.90, 1.);
const HISTORY: Color = Color::new(0.85, 0.72, 0.42, 1.);
const ORDER: [Ammo; 3] = [Ammo::Round, Ammo::Chain, Ammo::Shell];
const DIGITS: [&str; 4] = ["1", "2", "3", "4"];

/// World to canvas: one canvas unit per world unit, x from the chart's left
/// edge and y up from sea level.
const X0: f32 = 60.;
const Y0: f32 = 430.;
/// The drawn waterline (world height 6).
const WATER: f32 = 424.;
/// The wind gauge over the sea.
const GAUGE: Vec2 = vec2(480., 137.);
/// Header spots that effects rise from.
const LIFEBOATS_AT: Vec2 = vec2(812., 81.);
/// The dock card's salvage purse.
const PURSE_AT: Vec2 = vec2(690., 186.);

fn screen(p: Vec2) -> Vec2 {
    vec2(X0 + p.x, Y0 - p.y)
}
fn in_scene(p: Vec2) -> bool {
    p.x >= SCENE.x + 2.
        && p.x <= SCENE.right() - 2.
        && p.y >= SCENE.y + 2.
        && p.y <= SCENE.bottom() - 2.
}
/// Canvas rectangle of a hull section (0-2) or the mast (3); it matches the
/// part's collision box exactly.
fn part_rect(ship: Ship, part: usize) -> Rect {
    let c = screen(ship.section_center(part));
    if part == 3 {
        Rect::new(c.x - 5., c.y - 35., 10., 70.)
    } else {
        Rect::new(c.x - 15., c.y - 14., 30., 28.)
    }
}
/// Where a part's numbers float.
fn part_top(ship: Ship, part: usize) -> Vec2 {
    let r = part_rect(ship, part);
    vec2(r.center().x, r.y - 12.)
}
fn muzzle(ship: Ship, player: bool) -> Vec2 {
    screen(vec2(ship.x + if player { 50. } else { -50. }, 40.))
}

// ---------------------------------------------------------------------------
// Small drawing vocabulary.

fn plain(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
fn say(value: &str, x: f32, y: f32, s: TextStyle) -> f32 {
    draw_text_styled(value, x, y, s).width
}
/// Draw `value` centered on (`cx`, `cy`); returns its width.
fn say_mid(value: &str, cx: f32, cy: f32, s: TextStyle) -> f32 {
    let s = s.with_align(TextAlign::Center);
    draw_text_styled(value, cx, s.centered_baseline(cy), s).width
}
/// Draw `value` vertically centered on `cy` from `x`; returns its width.
fn say_left(value: &str, x: f32, cy: f32, s: TextStyle) -> f32 {
    draw_text_styled(value, x, s.centered_baseline(cy), s).width
}
/// Soft alpha-blended light pool (stays in the main batch, unlike additive glows).
fn halo(x: f32, y: f32, r: f32, color: Color) {
    let c = vec2(x, y);
    shapes::fill_circle(c, r, Fill::radial(c, r, color, with_alpha(color, 0.)));
}
fn fract(v: f32) -> f32 {
    v - v.floor()
}
fn hovered(r: Rect) -> bool {
    r.contains(pointer())
}
fn cross(c: Vec2, r: f32, width: f32, color: Color) {
    draw_line(c.x - r, c.y - r, c.x + r, c.y + r, width, color);
    draw_line(c.x - r, c.y + r, c.x + r, c.y - r, width, color);
}
fn chevron(x: f32, y: f32, dir: f32, s: f32, color: Color) {
    draw_line(x - dir * s * 0.5, y - s, x + dir * s * 0.5, y, 2., color);
    draw_line(x + dir * s * 0.5, y, x - dir * s * 0.5, y + s, 2., color);
}
/// A compact key cap for hints inside buttons; returns its width.
fn mini_cap(key: &str, x: f32, y: f32, a: f32) -> f32 {
    let s = plain(11., with_alpha(PAPER, a));
    let w = (text_width(key, s) + 8.).max(15.);
    let h = 15.;
    rounded(x, y + 1.5, w, h, 3.5, Color::new(0., 0., 0., 0.45 * a));
    rounded_gradient(
        x,
        y,
        w,
        h,
        3.5,
        Color::new(0.24, 0.28, 0.34, a),
        Color::new(0.13, 0.16, 0.21, a),
    );
    say_mid(key, x + w * 0.5, y + h * 0.5, s);
    w
}
/// An iron ball with a moonlit highlight.
fn iron(p: Vec2, r: f32, a: f32) {
    shapes::fill_circle(
        p,
        r,
        Fill::radial(
            p - vec2(r * 0.4, r * 0.4),
            r * 1.6,
            Color::new(0.52, 0.54, 0.58, a),
            Color::new(0.08, 0.09, 0.11, a),
        ),
    );
    shapes::fill_circle(
        p - vec2(r * 0.36, r * 0.4),
        r * 0.24,
        Color::new(1., 0.97, 0.88, 0.7 * a),
    );
}
/// The game's own symbol for each ammunition, used on racks, buttons and shots.
fn ammo_glyph(ammo: Ammo, c: Vec2, r: f32, a: f32) {
    match ammo {
        Ammo::Round => iron(c, r, a),
        Ammo::Chain => {
            let d = vec2(r * 0.9, -r * 0.5);
            for k in 0..5 {
                let p = c - d + d * (k as f32 * 0.5);
                shapes::stroke_circle(p, r * 0.2, r * 0.1 + 0.6, with_alpha(STEEL, a));
            }
            iron(c - d, r * 0.6, a);
            iron(c + d, r * 0.6, a);
        }
        Ammo::Shell => {
            let body = c + vec2(0., r * 0.12);
            shapes::fill_circle(
                body,
                r * 0.9,
                Fill::radial(
                    body - vec2(r * 0.35, r * 0.35),
                    r * 1.5,
                    Color::new(0.86, 0.36, 0.22, a),
                    Color::new(0.24, 0.06, 0.05, a),
                ),
            );
            shapes::fill_circle(
                body - vec2(r * 0.34, r * 0.38),
                r * 0.2,
                Color::new(1., 0.9, 0.8, 0.6 * a),
            );
            let neck = body + vec2(r * 0.45, -r * 0.78);
            shapes::fill_circle(neck, r * 0.24 + 0.5, Color::new(0.3, 0.3, 0.32, a));
            let spark = c + vec2(r * 1.0, -r * 1.25);
            draw_line(neck.x, neck.y, spark.x, spark.y, 1.4, with_alpha(CREAM, a));
            halo(
                spark.x,
                spark.y,
                r * 0.7 + 2.,
                Color::new(1., 0.7, 0.3, 0.6 * a),
            );
            shapes::fill_circle(spark, r * 0.2 + 0.6, Color::new(1., 0.85, 0.4, a));
        }
    }
}
/// A hull or mast meter: the live fraction, a pulsing ghost of what a
/// previewed shot or purchase would change, and a red cross once destroyed.
#[allow(clippy::too_many_arguments)]
fn part_meter(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    hp: u16,
    max: u16,
    ghost: Option<u16>,
    color: Color,
    blink: f32,
) {
    let frac = |v: u16| (f32::from(v) / f32::from(max.max(1))).clamp(0., 1.);
    rounded(
        x - 1.,
        y - 1.,
        w + 2.,
        h + 2.,
        h * 0.5 + 1.,
        Color::new(1., 1., 1., 0.1),
    );
    rounded(x, y, w, h, h * 0.5, Color::new(0.01, 0.03, 0.05, 0.85));
    let now = frac(hp);
    if hp == 0 && ghost.is_none_or(|g| g == 0) {
        cross(vec2(x + w * 0.5, y + h * 0.5), 2.6, 1.4, RED);
        return;
    }
    let tint = if now < 0.34 { RED } else { color };
    if now > 0. {
        rounded(x, y, (w * now).max(h), h, h * 0.5, tint);
    }
    if let Some(g) = ghost {
        let then = frac(g);
        let (a, b) = if then < now { (then, now) } else { (now, then) };
        if b > a {
            let tone = if then < now {
                Color::new(1., 1., 1., 0.3 + 0.55 * blink)
            } else {
                with_alpha(TEAL, 0.3 + 0.4 * blink)
            };
            rounded(x + w * a, y, (w * (b - a)).max(2.), h, h * 0.5, tone);
        }
    }
}
/// Three hull sections as one segmented meter, stern to bow from left to
/// right as on the chart, each chunk sized by its maximum.
fn hull_bar(x: f32, y: f32, w: f32, h: f32, ship: Ship, color: Color, ghost: Option<[u16; 4]>) {
    let total: f32 = ship.max[..3].iter().map(|&m| f32::from(m)).sum::<f32>();
    let gap = 3.;
    let usable = w - gap * 2.;
    let blink = 0.5 + 0.5 * (clock() * 7.).sin();
    let mut sx = x;
    for part in 0..3 {
        let sw = usable * f32::from(ship.max[part]) / total.max(1.);
        part_meter(
            sx,
            y,
            sw,
            h,
            ship.hp[part],
            ship.max[part],
            ghost.map(|g| g[part]),
            color,
            blink,
        );
        sx += sw + gap;
    }
}
/// A raised, clickable button with a key cap, an icon and at most one word.
fn action_button(rect: Rect, key: &str, glyph: Icon, label: &str, primary: bool, enabled: bool) {
    let hover = enabled && hovered(rect);
    let lift = if hover { 2. } else { 0. };
    let r = Rect::new(rect.x, rect.y - lift, rect.w, rect.h);
    let a = if enabled { 1. } else { 0.45 };
    if primary && enabled {
        let breath = pulse(clock(), 0.45);
        shapes::drop_shadow(
            r,
            12.,
            Shadow::soft(with_alpha(GOLD, 0.16 + 0.2 * breath)).blur(16. + 8. * breath),
        );
    }
    shapes::drop_shadow(
        r,
        12.,
        Shadow::soft(Color::new(0., 0., 0., 0.35 * a))
            .offset(vec2(0., 3. + lift))
            .blur(10. + 4. * lift),
    );
    if primary {
        let top = lighten(GOLD, 0.25 + if hover { 0.1 } else { 0. });
        rounded_gradient(
            r.x,
            r.y,
            r.w,
            r.h,
            12.,
            with_alpha(top, a),
            with_alpha(darken(GOLD, 0.12), a),
        );
    } else {
        let top = if hover {
            Color::new(0.25, 0.33, 0.40, a)
        } else {
            Color::new(0.20, 0.27, 0.33, a)
        };
        rounded_gradient(
            r.x,
            r.y,
            r.w,
            r.h,
            12.,
            top,
            Color::new(0.12, 0.17, 0.22, a),
        );
        outline(r.x, r.y, r.w, r.h, 12., 1., Color::new(1., 1., 1., 0.1 * a));
    }
    let ink = if primary { INK } else { CREAM };
    let s = style(19., with_alpha(ink, a));
    let cap = text_width(key, plain(11., PAPER)) + 8.;
    let total = cap.max(15.) + 10. + 20. + 8. + text_width(label, s);
    let mut x = r.center().x - total * 0.5;
    let cy = r.center().y;
    x += mini_cap(key, x, cy - 7.5, a) + 10.;
    icon(glyph, x + 10., cy, 20., with_alpha(ink, a));
    say_left(label, x + 28., cy, s);
}

// ---------------------------------------------------------------------------
// Host hooks.

impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        self.render();
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: self.battle_info().island.to_string(),
            objective: "Sink the enemy ship".into(),
            status: format!(
                "{} Hull {}, enemy hull {}, {} salvage, {} lifeboats.",
                self.log,
                self.hull(),
                self.ships[1].hull(),
                self.credits,
                self.lifeboats
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
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.battle_number() as u32, 8))
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        for event in self.events.iter() {
            self.feedback(event, fx);
        }
    }
    fn draw_objective(&self, area: Rect) -> bool {
        self.versus(area);
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        self.results(area);
        true
    }
}

impl Game {
    fn enemy_color(&self) -> Color {
        let [r, g, b] = self.battle_info().color;
        Color::new(r, g, b, 1.)
    }
    fn modal_open(&self) -> bool {
        matches!(self.phase, Phase::Briefing | Phase::Dock | Phase::Setback)
    }
    /// The ammunition whose damage the forecast previews: the hovered rack,
    /// otherwise the loaded one.
    fn preview_ammo(&self) -> Ammo {
        let p = pointer();
        AMMO.iter()
            .position(|r| r.contains(p))
            .map_or(self.setting.ammo, |i| ORDER[i])
    }
    fn setback_cause(&self) -> Cause {
        if self.hull() == 0 {
            Cause::Sunk
        } else if self.ammo.iter().all(|&n| n == 0) {
            Cause::Empty
        } else {
            Cause::Retreat
        }
    }

    // -----------------------------------------------------------------------
    // Effects: every accepted event is felt where it happened.

    fn feedback(&self, event: Event, fx: &mut Juice) {
        let foe = self.ships[1];
        match event {
            Event::Engaged => {
                let ship = self.ships[0];
                fx.ring(screen(vec2(ship.x, 60.)), TEAL, 64.);
                fx.sparkle(screen(vec2(ship.x, 118.)), TEAL);
            }
            Event::Fired { player, at } => {
                let p = screen(at);
                fx.burst(p, if player { FLAME } else { RED }, 18);
                fx.ring(p, with_alpha(CREAM, 0.75), 26.);
                if player {
                    fx.shake(0.12);
                }
            }
            Event::Hit {
                side,
                part,
                at,
                before,
                after,
                armor,
            } => {
                let p = screen(at);
                let ship = self.ships[side];
                let color = if side == 1 { GOLD } else { RED };
                let lost: u16 = (0..4).map(|q| before[q].saturating_sub(after[q])).sum();
                if lost == 0 {
                    fx.burst(p, STEEL, 10);
                    fx.ring(p, STEEL, 30.);
                    fx.popup_icon(p - vec2(0., 18.), Icon::Shield, "Blocked", STEEL);
                } else {
                    fx.burst(p, FLAME, 26);
                    fx.burst(p, SPLINTER, 12);
                    fx.ring(p, color, 44.);
                    // One number, the shot's total (as the aim preview showed
                    // it), over the struck part; shell splash erupts on each
                    // neighbouring section it reached.
                    fx.popup(part_top(ship, part), &format!("-{lost}"), color);
                    for q in 0..4 {
                        if before[q] == after[q] {
                            continue;
                        }
                        if q != part {
                            let c = part_rect(ship, q).center();
                            fx.burst(c, FLAME, 12);
                            fx.ring(c, with_alpha(color, 0.8), 26.);
                        }
                        if after[q] == 0 {
                            let c = part_rect(ship, q).center();
                            fx.burst(c, FLAME, 30);
                            fx.burst(c, SMOKE, 14);
                            fx.ring(c, color, 64.);
                            if q == 3 {
                                fx.callout(vec2(c.x.clamp(150., 810.), 262.), "Mast down!", color);
                                if side == 0 {
                                    let m = METER.center();
                                    fx.popup_icon(m - vec2(0., 30.), Icon::Lock, "76", RED);
                                }
                            }
                        }
                    }
                    if side == 1 {
                        fx.shake(if part == 3 { 0.2 } else { 0.26 });
                    }
                }
                if armor[0] > armor[1] {
                    // Stacks above the damage number at the same spot.
                    let a = armor[0] - armor[1];
                    let top = part_top(ship, part);
                    fx.popup_icon(top, Icon::Shield, &format!("-{a}"), STEEL);
                }
            }
            Event::Splash { at } => {
                let p = vec2(screen(at).x, WATER);
                if in_scene(p) {
                    fx.burst(p, FOAM, 20);
                    fx.ring(p, with_alpha(FOAM, 0.85), 34.);
                    fx.popup(p - vec2(0., 30.), "Miss", SOFT);
                }
            }
            Event::Loaded(ammo) => {
                let c = AMMO[ammo.index()].center();
                fx.ring(c, GOLD, 34.);
                fx.sparkle(c - vec2(0., 8.), GOLD);
            }
            Event::Table(on) => fx.ring(TABLE.center(), if on { GOLD } else { SOFT }, 30.),
            Event::Empty(ammo) => {
                let c = AMMO[ammo.index()].center();
                fx.shake(0.14);
                fx.ring(c, RED, 36.);
                fx.popup(c - vec2(0., 38.), "Empty", RED);
            }
            Event::EnemyAims => fx.ring(muzzle(foe, false), with_alpha(RED, 0.85), 30.),
            Event::Fouled => {
                fx.popup_icon(
                    muzzle(foe, false) - vec2(0., 34.),
                    Icon::Hourglass,
                    "Skipped",
                    STEEL,
                );
            }
            Event::Gust(wind) => {
                fx.ring(GAUGE, CREAM, 70.);
                let arrow = if wind > 0. {
                    Icon::ArrowRight
                } else {
                    Icon::ArrowLeft
                };
                fx.popup_icon(GAUGE + vec2(0., 30.), arrow, "", CREAM);
            }
            Event::Sank { salvage } => {
                let c = screen(vec2(foe.x, 40.));
                fx.burst(c, GOLD, 44);
                fx.burst(c, FLAME, 26);
                fx.ring(c, GOLD, 96.);
                fx.confetti(c - vec2(0., 20.), 60);
                fx.popup_icon(c - vec2(0., 70.), Icon::Coin, &format!("+{salvage}"), GOLD);
                fx.callout(vec2(480., 236.), "Harbor free!", GOLD);
            }
            Event::Bought { slot, cost } => {
                let c = SHOP[slot - 1].center();
                fx.ring(c, TEAL, 48.);
                fx.sparkle(c, TEAL);
                let (glyph, gain) = shop_gain(slot);
                fx.popup_icon(c - vec2(0., 22.), glyph, gain, TEAL);
                fx.popup_icon(
                    PURSE_AT + vec2(0., 20.),
                    Icon::Coin,
                    &format!("-{cost}"),
                    GOLD,
                );
            }
            Event::Refused { slot, short } => {
                let c = SHOP[slot - 1].center();
                fx.shake(0.12);
                fx.ring(c, RED, 44.);
                if short > 0 {
                    fx.popup_icon(c - vec2(0., 22.), Icon::Coin, &format!("Need {short}"), RED);
                } else {
                    let word = if slot == 1 { "Full" } else { "Max" };
                    fx.popup_icon(c - vec2(0., 22.), Icon::Check, word, SOFT);
                }
            }
            Event::Sailed => {}
            Event::Setback(cause) => {
                let p = screen(vec2(self.ships[0].x, 40.));
                if cause == Cause::Sunk {
                    fx.burst(p, SMOKE, 36);
                    fx.burst(p, FLAME, 26);
                    fx.ring(p, RED, 90.);
                } else {
                    fx.ring(p, SOFT, 70.);
                }
            }
            Event::Rescued => {
                let p = screen(vec2(self.ships[0].x, 60.));
                fx.ring(p, TEAL, 90.);
                fx.sparkle(p, TEAL);
                fx.callout(vec2(480., 236.), "Retry", TEAL);
                fx.popup_icon(LIFEBOATS_AT + vec2(0., 18.), Icon::Ship, "-1", CREAM);
            }
            Event::Retired => fx.vignette(Color::new(0.1, 0.12, 0.2, 0.5)),
        }
    }

    // -----------------------------------------------------------------------
    // Header and results.

    /// Your hull against the enemy's, then lifeboats and salvage.
    fn versus(&self, area: Rect) {
        let cy = area.y + area.h * 0.5;
        let (you, foe) = (self.ships[0], self.ships[1]);
        let foe_color = self.enemy_color();
        let bar_w = 214.;
        let digits = style(18., CREAM);
        let mut x = area.x + 2.;
        icon(Icon::Flag, x + 9., cy, 18., TEAL);
        x += 26.;
        hull_bar(x, cy - 5., bar_w, 10., you, TEAL, None);
        x += bar_w + 10.;
        say_left(&you.hull().to_string(), x, cy, digits);
        // The target: sink every enemy section.
        let mid = area.x + 330.;
        icon(Icon::Target, mid, cy, 20., GOLD);
        let num = style(18., CREAM).with_align(TextAlign::Right);
        draw_text_styled(
            &foe.hull().to_string(),
            mid + 66.,
            num.centered_baseline(cy),
            num,
        );
        hull_bar(mid + 76., cy - 5., bar_w, 10., foe, foe_color, None);
        icon(Icon::Flag, mid + 76. + bar_w + 17., cy, 18., foe_color);
        // Lifeboats (retries) and salvage.
        let x = LIFEBOATS_AT.x - 12.;
        for i in 0..2 {
            let lit = i < self.lifeboats;
            let color = if lit { CREAM } else { with_alpha(MUTED, 0.3) };
            icon(Icon::Ship, x + i as f32 * 24., cy, 19., color);
        }
        icon_value(
            Icon::Coin,
            &self.credits.to_string(),
            x + 54.,
            cy,
            18.,
            GOLD,
        );
    }
    /// Rank, harbors freed, accuracy and lifeboats kept.
    fn results(&self, area: Rect) {
        let y = area.y + 18.;
        icon(Icon::Crown, area.x + 16., y, 28., GOLD);
        say_left(self.rank(), area.x + 40., y, style(26., GOLD));
        let accuracy = if self.shots == 0 {
            0
        } else {
            u32::from(self.hits) * 100 / u32::from(self.shots)
        };
        let gap = 12.;
        let w = (area.w - gap * 2.) / 3.;
        let top = area.y + 50.;
        let h = 78.;
        for i in 0..3 {
            let x = area.x + i as f32 * (w + gap);
            rounded(x, top, w, h, 14., Color::new(1., 1., 1., 0.05));
            outline(x, top, w, h, 14., 1., Color::new(1., 1., 1., 0.07));
            let cy = top + h * 0.5;
            match i {
                0 => {
                    icon(Icon::Anchor, x + 34., cy, 30., TEAL);
                    say_left(&format!("{}/8", self.wins), x + 60., cy, style(30., CREAM));
                }
                1 => {
                    icon(Icon::Target, x + 34., cy, 30., GOLD);
                    say_left(&format!("{accuracy}%"), x + 60., cy, style(30., CREAM));
                }
                _ => {
                    for boat in 0..2 {
                        let lit = boat < self.lifeboats;
                        let color = if lit { CREAM } else { with_alpha(MUTED, 0.3) };
                        icon(Icon::Ship, x + 40. + boat as f32 * 44., cy, 34., color);
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // The chart.

    fn render(&self) {
        let t = clock();
        self.draw_sky(t);
        self.draw_islands();
        self.draw_sea(t);
        trajectory(&self.history, with_alpha(HISTORY, 0.45), 1.6);
        let aiming = self.phase == Phase::Aim;
        let preview = if aiming {
            self.preview(self.preview_ammo())
        } else {
            None
        };
        if aiming {
            self.draw_forecast(t);
        }
        self.draw_ship(self.ships[0], true, TEAL, t);
        self.draw_ship(self.ships[1], false, self.enemy_color(), t);
        self.draw_waterline(t);
        self.draw_meters(0, preview, t);
        self.draw_meters(1, preview, t);
        if aiming {
            self.draw_target(t);
            self.draw_dial();
        }
        self.draw_shot(t);
        self.draw_impact();
        self.draw_telegraphs(t);
        self.draw_wind(t);
        if aiming {
            self.draw_preview(preview, t);
            self.draw_retreat_hint();
        }
        self.draw_tooltip();
        self.draw_deck(t);
        if self.modal_open() {
            self.draw_modal(t);
        }
    }
    fn draw_sky(&self, t: f32) {
        let s = SCENE;
        rounded_gradient(s.x, s.y, s.w, s.h, 12., SKY_TOP, SKY_LOW);
        let haze = Rect::new(s.x, WATER - 120., s.w, 120.);
        shapes::fill_rounded_rect(
            haze,
            0.,
            Fill::vertical(
                haze,
                Color::new(0.55, 0.40, 0.30, 0.),
                Color::new(0.55, 0.40, 0.30, 0.16),
            ),
        );
        for i in 0..34_u32 {
            let x = s.x + 16. + ((i * 263 + 41) % 888) as f32;
            let y = s.y + 46. + ((i * 97 + 13) % 196) as f32;
            let a = 0.14 + 0.26 * (0.5 + 0.5 * (t * 1.9 + i as f32 * 1.7).sin());
            let size = if i % 5 == 0 { 2. } else { 1.3 };
            draw_rectangle(x, y, size, size, Color::new(0.9, 0.92, 1., a));
        }
        let moon = vec2(866., 196.);
        halo(moon.x, moon.y, 66., Color::new(0.95, 0.85, 0.55, 0.18));
        shapes::fill_circle(
            moon,
            20.,
            Fill::radial(
                moon - vec2(6., 6.),
                30.,
                Color::new(0.98, 0.95, 0.82, 1.),
                Color::new(0.78, 0.72, 0.55, 1.),
            ),
        );
        draw_circle(
            moon.x + 6.,
            moon.y + 4.,
            4.,
            Color::new(0.70, 0.64, 0.48, 0.45),
        );
        draw_circle(
            moon.x - 5.,
            moon.y + 8.,
            2.5,
            Color::new(0.70, 0.64, 0.48, 0.4),
        );
        // Clouds and streaks drift downwind; calm air still breathes east.
        let drift = if self.wind == 0. {
            2.4
        } else {
            self.wind * 0.9
        };
        for (i, (bx, by, w)) in [
            (140., 206., 80.),
            (380., 244., 104.),
            (620., 192., 70.),
            (790., 266., 88.),
        ]
        .into_iter()
        .enumerate()
        {
            let span = s.w + 160.;
            let x = s.x - 80.
                + fract((bx + 80. - s.x + t * drift * (1. + i as f32 * 0.2)) / span) * span;
            let edge = ((x - s.x) / 60.).min((s.right() - x) / 60.).clamp(0., 1.);
            let c = Color::new(0.62, 0.70, 0.80, 0.13 * edge);
            for (dx, dy, len, r) in [
                (0., 0., 1., 3.5),
                (-0.2, -7., 0.6, 3.),
                (0.25, 7., 0.45, 2.5),
            ] {
                let cx = x + dx * w;
                shapes::fill_capsule(
                    vec2(cx - w * len * 0.5, by + dy),
                    vec2(cx + w * len * 0.5, by + dy),
                    r,
                    c,
                );
            }
        }
        if self.wind != 0. {
            let len = 16. + self.wind.abs() * 1.8;
            let span = s.w - len - 20.;
            for i in 0..9 {
                let fi = i as f32;
                let y = 176. + ((i * 37) % 196) as f32;
                let x = s.x
                    + 10.
                    + fract(fi * 0.37 + t * self.wind * 9. * (1. + fi * 0.08) / span) * span;
                draw_line(x, y, x + len, y, 1., Color::new(0.8, 0.88, 0.95, 0.12));
            }
        }
    }
    fn draw_islands(&self) {
        let base = WATER + 1.;
        for n in 0..7 {
            let x = 110. + n as f32 * 123.;
            let peak = 362. - ((self.battle * 13 + n * 29) % 40) as f32;
            draw_triangle(
                vec2(x - 84., base),
                vec2(x, peak),
                vec2(x + 84., base),
                Color::new(0.12, 0.19, 0.25, 0.9),
            );
        }
        for n in 0..5 {
            let x = 290. + n as f32 * 128.;
            let peak = 350. - ((self.battle * 17 + n * 23) % 58) as f32;
            draw_triangle(
                vec2(x - 60., base),
                vec2(x, peak),
                vec2(x + 64., base),
                Color::new(0.09, 0.21, 0.24, 1.),
            );
            draw_triangle(
                vec2(x - 60., base),
                vec2(x, peak),
                vec2(x - 6., base),
                Color::new(0.13, 0.28, 0.30, 1.),
            );
            draw_triangle(
                vec2(x - 16., peak + 20.),
                vec2(x, peak),
                vec2(x + 18., peak + 22.),
                Color::new(0.30, 0.42, 0.40, 1.),
            );
        }
    }
    fn draw_sea(&self, t: f32) {
        let s = SCENE;
        let mid = shapes::mix(SEA_TOP, SEA_LOW, 0.55);
        let body = Rect::new(s.x, WATER, s.w, s.bottom() - WATER - 14.);
        shapes::fill_rounded_rect(body, 0., Fill::vertical(body, SEA_TOP, mid));
        rounded_gradient(s.x, s.bottom() - 28., s.w, 28., 12., mid, SEA_LOW);
        draw_line(
            s.x,
            WATER,
            s.right(),
            WATER,
            1.,
            Color::new(0.55, 0.78, 0.80, 0.4),
        );
        // Moonlit glitter and rolling crests.
        for k in 0..6 {
            let y = WATER + 4. + k as f32 * 6.;
            let w = 26. - k as f32 * 3. + (t * 3.6 + k as f32).sin() * 4.;
            draw_line(
                866. - w,
                y,
                866. + w,
                y,
                1.,
                Color::new(1., 0.95, 0.75, 0.22 - k as f32 * 0.03),
            );
        }
        for row in 0..4 {
            let alpha = 0.3 - row as f32 * 0.06;
            let y0 = WATER + 8. + row as f32 * 12.;
            for n in 0..18 {
                let span = s.w - 30.;
                let x = s.x
                    + 6.
                    + fract(
                        (n as f32 * 53. + row as f32 * 19. + t * (11. + row as f32 * 3.)) / span,
                    ) * span;
                let y = y0 + (t * 2.1 + n as f32 * 0.9).sin() * 1.5;
                let len = 10. + ((n * 7 + row * 3) % 9) as f32;
                if x + len < s.right() - 4. {
                    draw_line(x, y, x + len, y, 1.2, Color::new(0.55, 0.80, 0.82, alpha));
                }
            }
        }
    }
    /// The water's near edge lapping over both keels, with foam and reflections.
    fn draw_waterline(&self, t: f32) {
        let s = SCENE;
        let band = Rect::new(s.x, WATER - 2., s.w, 7.);
        shapes::fill_rounded_rect(
            band,
            0.,
            Fill::vertical(band, with_alpha(SEA_TOP, 0.65), with_alpha(SEA_TOP, 0.9)),
        );
        draw_line(
            s.x,
            WATER - 2.,
            s.right(),
            WATER - 2.,
            1.,
            Color::new(0.6, 0.85, 0.86, 0.35),
        );
        for (side, ship) in self.ships.iter().enumerate() {
            let color = if side == 0 { TEAL } else { self.enemy_color() };
            let x = X0 + ship.x;
            for k in 0..3 {
                let y = WATER + 4. + k as f32 * 4.;
                let wobble = (t * 2.4 + k as f32 * 1.3 + side as f32).sin() * 3.;
                let w = 34. - k as f32 * 8.;
                draw_line(
                    x - w + wobble,
                    y,
                    x + w + wobble,
                    y,
                    1.6,
                    with_alpha(color, 0.16 - k as f32 * 0.04),
                );
            }
            for end in [-1., 1.] {
                let fx = x + end * (46. + (t * 3. + end).sin() * 2.);
                draw_line(
                    fx - 6.,
                    WATER - 1.,
                    fx + 6.,
                    WATER - 1.,
                    1.4,
                    Color::new(0.85, 0.95, 0.95, 0.35),
                );
            }
        }
    }
    fn draw_ship(&self, ship: Ship, player: bool, color: Color, t: f32) {
        let side = if player { 1. } else { -1. };
        let sway = (t * 1.5 + if player { 0. } else { 1.9 }).sin() * 1.1;
        let bow = if player { 2 } else { 0 };
        let stern = 2 - bow;
        self.draw_rig(ship, player, color, t, sway);
        let dark = darken(color, 0.58);
        for part in 0..3 {
            let r = part_rect(ship, part);
            let top = r.y + sway;
            let bottom = r.bottom() + 3. + sway;
            let mid = top + 11.;
            let (mut lt, mut rt, mut lb, mut rb) = (r.x, r.right(), r.x, r.right());
            // The bow overhangs and sweeps in below; the stern ends in a transom.
            if part == bow {
                if player {
                    rt += 9.;
                    rb -= 12.;
                } else {
                    lt -= 9.;
                    lb += 12.;
                }
            } else if part == stern {
                if player {
                    lt -= 3.;
                    lb += 7.;
                } else {
                    rt += 3.;
                    rb -= 7.;
                }
            }
            let along = |a: f32, b: f32, y: f32| a + (b - a) * (y - top) / (bottom - top);
            let (lm, rm) = (along(lt, lb, mid), along(rt, rb, mid));
            let cx = r.center().x;
            if ship.hp[part] == 0 {
                draw_wreck(lb, rb, top, bottom, cx, color, t, part, self.wind);
                continue;
            }
            let upper = Rect::new(lt.min(lb), top, rt.max(rb) - lt.min(lb), mid - top);
            shapes::fill_polygon(
                &[vec2(lt, top), vec2(rt, top), vec2(rm, mid), vec2(lm, mid)],
                Fill::vertical(upper, lighten(color, 0.14), color),
            );
            let lower = Rect::new(upper.x, mid, upper.w, bottom - mid);
            shapes::fill_polygon(
                &[
                    vec2(lm, mid),
                    vec2(rm, mid),
                    vec2(rb, bottom),
                    vec2(lb, bottom),
                ],
                Fill::vertical(lower, dark, darken(color, 0.74)),
            );
            // Plank seams, the rail and the wale.
            let seam = with_alpha(darken(color, 0.4), 0.5);
            for y in [top + 8.5, mid + 5., mid + 10.] {
                draw_line(
                    along(lt, lb, y) + 1.,
                    y,
                    along(rt, rb, y) - 1.,
                    y,
                    0.8,
                    seam,
                );
            }
            draw_line(lt, top, rt, top, 1.6, lighten(color, 0.5));
            draw_line(lm, mid, rm, mid, 1.2, darken(color, 0.3));
            draw_rectangle(cx - 4., top + 2.5, 8., 5., Color::new(0.04, 0.05, 0.06, 1.));
            // Armor plating along the waterline, thicker with more armor.
            let armor = ship.armor[part];
            if armor > 0 {
                let th = 2. + f32::from(armor.min(10)) * 0.25;
                let y = bottom - 6.;
                let (pl, pr) = (along(lt, lb, y) + 1., along(rt, rb, y) - 1.);
                draw_rectangle(pl, y - th, pr - pl, th, STEEL);
                draw_line(pl, y - th, pr, y - th, 1., lighten(STEEL, 0.45));
                let mut rx = pl + 3.;
                while rx < pr - 2. {
                    draw_rectangle(rx, y - th * 0.5 - 0.5, 1., 1., darken(STEEL, 0.45));
                    rx += 6.;
                }
            }
            // Wounds grow as integrity falls: cracks, scorch, then flames.
            let health = f32::from(ship.hp[part]) / f32::from(ship.max[part].max(1));
            let ink = Stroke::new(1.3, Color::new(0.10, 0.06, 0.04, 0.85));
            if health < 0.75 {
                shapes::polyline(
                    &[
                        vec2(cx - 9., top + 2.),
                        vec2(cx - 5., top + 7.),
                        vec2(cx - 8., top + 11.),
                        vec2(cx - 3., top + 17.),
                    ],
                    ink,
                );
            }
            if health < 0.45 {
                halo(cx + 5., top + 9., 9., Color::new(0.05, 0.03, 0.02, 0.55));
                shapes::polyline(
                    &[
                        vec2(cx + 10., top + 1.),
                        vec2(cx + 6., top + 6.),
                        vec2(cx + 9., top + 10.),
                    ],
                    ink,
                );
            }
            if health < 0.25 {
                for k in 0..3 {
                    let fx = cx - 6. + k as f32 * 6.;
                    let h = 5. + 3. * (t * 11. + k as f32 * 2.1 + part as f32).sin().abs();
                    draw_triangle(
                        vec2(fx - 3., top),
                        vec2(fx + 3., top),
                        vec2(fx + self.wind * 0.12, top - h),
                        Color::new(1., 0.55 + 0.1 * k as f32, 0.2, 0.9),
                    );
                }
            }
        }
        // Bowsprit forward and a lantern astern.
        if ship.hp[bow] > 0 {
            let r = part_rect(ship, bow);
            let x = if player { r.right() + 7. } else { r.x - 7. };
            let y = r.y + sway + 1.;
            draw_line(x - side * 3., y + 1., x + side * 18., y - 9., 2.4, WOOD);
        }
        if ship.hp[stern] > 0 {
            let r = part_rect(ship, stern);
            let lx = if player { r.x + 1. } else { r.right() - 1. };
            let y = r.y + sway;
            let flicker = 0.75 + 0.25 * (t * 13. + side).sin();
            draw_line(lx, y, lx, y - 7., 1.2, WOOD);
            halo(lx, y - 9., 8., Color::new(1., 0.8, 0.4, 0.45 * flicker));
            draw_rectangle(lx - 2., y - 11., 4., 4., Color::new(1., 0.86, 0.5, flicker));
        }
        self.draw_cannon(ship, player, t, sway);
    }
    fn draw_rig(&self, ship: Ship, player: bool, color: Color, t: f32, sway: f32) {
        let side = if player { 1. } else { -1. };
        let x = X0 + ship.x;
        let deck = 392. + sway;
        let head = 317. + sway;
        if ship.hp[3] == 0 {
            // A stump, a fallen spar and torn cloth on the deck.
            draw_line(x, deck, x, deck - 13., 4., darken(WOOD, 0.3));
            draw_line(
                x - 28.,
                deck - 5.,
                x + 24.,
                deck + 1.,
                3.,
                darken(WOOD, 0.2),
            );
            draw_triangle(
                vec2(x - 18., deck - 4.),
                vec2(x + 2., deck - 2.),
                vec2(x - 10., deck + 3.),
                Color::new(0.72, 0.68, 0.58, 1.),
            );
            return;
        }
        let bow_tip = vec2(x + side * 70., deck - 9.);
        let stern_end = vec2(x - side * 50., deck - 1.);
        let rope = with_alpha(CREAM, 0.32);
        draw_line(x, head + 3., bow_tip.x, bow_tip.y, 0.9, rope);
        draw_line(x, head + 3., stern_end.x, stern_end.y, 0.9, rope);
        draw_triangle(
            vec2(x + side * 3., head + 12.),
            vec2(bow_tip.x - side * 6., bow_tip.y + 1.),
            vec2(x + side * 5., deck - 9.),
            Color::new(0.86, 0.82, 0.70, 0.94),
        );
        shapes::fill_capsule(vec2(x, deck), vec2(x, head), 2.2, WOOD);
        draw_line(x - 1., deck, x - 1., head, 1., lighten(WOOD, 0.3));
        let yard = head + 7.;
        draw_line(x - 22., yard, x + 22., yard, 2.4, WOOD);
        let billow = (self.wind * 0.35).clamp(-5., 5.) + (t * 1.7 + side).sin() * 0.8;
        let torn = ship.hp[3] * 2 < ship.max[3];
        let drop = if torn { 30. } else { 42. };
        let (a, b) = (vec2(x - 20., yard + 1.), vec2(x + 20., yard + 1.));
        let (c, d) = (
            vec2(x + 21. + billow, yard + drop),
            vec2(x - 19. + billow, yard + drop),
        );
        draw_triangle(a, b, c, CLOTH);
        draw_triangle(a, c, d, CLOTH);
        let top_mid = vec2(x, yard + 1.);
        let bottom_mid = vec2(x + 1. + billow, yard + drop);
        let shade = Color::new(0.72, 0.68, 0.58, 1.);
        if billow >= 0. {
            draw_triangle(top_mid, b, c, shade);
            draw_triangle(top_mid, c, bottom_mid, shade);
        } else {
            draw_triangle(a, top_mid, bottom_mid, shade);
            draw_triangle(a, bottom_mid, d, shade);
        }
        // A stripe in the ship's colors across the sail.
        let band = yard + drop * 0.55;
        draw_line(
            x - 19.5 + billow * 0.55,
            band,
            x + 20.5 + billow * 0.55,
            band,
            3.,
            with_alpha(color, 0.75),
        );
        draw_line(
            d.x,
            d.y,
            c.x,
            c.y,
            1.5,
            with_alpha(darken(color, 0.2), 0.95),
        );
        if torn {
            draw_triangle(
                vec2(x - 5. + billow, d.y),
                vec2(x + 2. + billow, d.y - 9.),
                vec2(x + 7. + billow, d.y),
                SKY_LOW,
            );
        }
        let stream = if self.wind > 0. {
            1.
        } else if self.wind < 0. {
            -1.
        } else {
            side
        };
        let flutter = (t * 7. + side).sin() * 2.;
        draw_triangle(
            vec2(x, head - 5.),
            vec2(x + stream * 22., head - 1. + flutter),
            vec2(x, head + 4.),
            color,
        );
    }
    fn draw_cannon(&self, ship: Ship, player: bool, t: f32, sway: f32) {
        let side = if player { 1. } else { -1. };
        let pivot = screen(vec2(ship.x + side * 38., 41.)) + vec2(0., sway);
        let enemy_laying =
            matches!(self.phase, Phase::EnemyReady) || self.ball.is_some_and(|b| !b.from_player);
        let degrees = if player {
            f32::from(self.setting.angle)
        } else if enemy_laying {
            f32::from(self.enemy_setting.angle)
        } else {
            35.
        };
        let angle = degrees.to_radians();
        let tip = pivot + vec2(side * angle.cos(), -angle.sin()) * 15.;
        draw_rectangle(pivot.x - 6., pivot.y - 1., 12., 5., darken(WOOD, 0.15));
        for dx in [-3.5, 3.5] {
            shapes::fill_circle(pivot + vec2(dx, 4.), 2.2, darken(WOOD, 0.45));
        }
        shapes::fill_capsule(pivot, tip, 3.2, Color::new(0.12, 0.13, 0.15, 1.));
        draw_line(
            pivot.x,
            pivot.y - 1.5,
            tip.x,
            tip.y - 1.5,
            1.,
            Color::new(0.47, 0.50, 0.55, 1.),
        );
        let firing = self
            .ball
            .is_some_and(|b| b.from_player == player && self.trace.len <= 3);
        if firing {
            halo(tip.x, tip.y, 20., Color::new(1., 0.7, 0.3, 0.85));
        } else if !player && self.phase == Phase::EnemyReady {
            let fuse = pulse(t, 3.);
            halo(
                pivot.x,
                pivot.y,
                10.,
                Color::new(1., 0.55, 0.2, 0.35 + 0.45 * fuse),
            );
            draw_rectangle(
                pivot.x - side * 5. - 1.,
                pivot.y - 4.,
                2.,
                2.,
                Color::new(1., 0.85, 0.4, 0.5 + 0.5 * fuse),
            );
        }
    }
    /// A protractor around your cannon: the needle is the current elevation,
    /// a gold notch the last shot and a teal diamond the firing table.
    fn draw_dial(&self) {
        let ship = self.ships[0];
        let pivot = screen(vec2(ship.x + 38., 41.));
        let r = 52.;
        let to = |deg: f32| {
            let rad = deg.to_radians();
            vec2(rad.cos(), -rad.sin())
        };
        let (start, sweep) = ((-70_f32).to_radians(), 55_f32.to_radians());
        let angle = f32::from(self.setting.angle);
        shapes::fill_sector(pivot, r, start, sweep, Color::new(0.02, 0.06, 0.09, 0.32));
        shapes::fill_sector(
            pivot,
            r,
            -angle.to_radians(),
            (angle - 15.).to_radians(),
            with_alpha(TEAL, 0.14),
        );
        shapes::stroke_arc(
            pivot,
            r,
            start,
            sweep,
            Stroke::new(1.4, with_alpha(SOFT, 0.55)),
        );
        for k in 0..=11 {
            let d = to(15. + k as f32 * 5.);
            let major = k % 3 == 0;
            let a = pivot + d * (r - if major { 8. } else { 4. });
            let b = pivot + d * r;
            draw_line(
                a.x,
                a.y,
                b.x,
                b.y,
                if major { 1.5 } else { 1. },
                with_alpha(SOFT, if major { 0.75 } else { 0.45 }),
            );
        }
        if let Some(last) = self.last_setting {
            let d = to(f32::from(last.angle));
            let (tip, base) = (pivot + d * (r + 2.), pivot + d * (r + 10.));
            let n = vec2(-d.y, d.x) * 4.;
            draw_triangle(tip, base + n, base - n, with_alpha(GOLD, 0.9));
        }
        if self.show_table {
            let c = pivot + to(f32::from(self.table.angle)) * (r + 9.);
            shapes::fill_polygon(
                &[
                    c + vec2(0., -5.),
                    c + vec2(5., 0.),
                    c + vec2(0., 5.),
                    c + vec2(-5., 0.),
                ],
                TEAL,
            );
        }
        let d = to(angle);
        let (a, b) = (pivot + d * 20., pivot + d * (r + 3.));
        draw_line(a.x, a.y, b.x, b.y, 2., TEAL);
        halo(b.x, b.y, 10., with_alpha(TEAL, 0.5));
        shapes::fill_circle(b, 3.6, lighten(TEAL, 0.35));
    }
    /// The predicted flight as marching dots and an aim marker where it ends.
    fn draw_forecast(&self, t: f32) {
        let trace = &self.forecast.trace;
        let n = trace.len.max(1) as f32;
        for (i, p) in trace.points[..trace.len].iter().enumerate().step_by(2) {
            let q = screen(*p);
            if !in_scene(q) {
                continue;
            }
            let wave = 0.5 + 0.5 * (i as f32 * 0.42 - t * 7.).sin();
            let a = (0.42 + 0.5 * wave) * (1. - 0.35 * i as f32 / n);
            shapes::fill_circle(q, 1.5 + 0.7 * wave, with_alpha(FORECAST, a));
        }
    }
    /// The aim marker and an outline around the part the shot would strike.
    fn draw_target(&self, t: f32) {
        let end = screen(self.forecast.end);
        let end = vec2(end.x, end.y.min(WATER));
        let color = match self.forecast.contact {
            Some((1, _)) => GOLD,
            Some(_) => RED,
            None => SOFT,
        };
        let beat = pulse(t, 1.4);
        if let Some((side, part)) = self.forecast.contact {
            let r = part_rect(self.ships[side], part);
            outline(
                r.x - 3.,
                r.y - 3.,
                r.w + 6.,
                r.h + 6.,
                5.,
                1.6,
                with_alpha(color, 0.45 + 0.45 * beat),
            );
        }
        if !in_scene(end) {
            return;
        }
        shapes::stroke_circle(end, 7., 1.6, color);
        shapes::stroke_circle(
            end,
            11. + beat * 5.,
            1.2,
            with_alpha(color, 0.55 * (1. - beat)),
        );
        for (dx, dy) in [(1., 0.), (-1., 0.), (0., 1.), (0., -1.)] {
            draw_line(
                end.x + dx * 4.,
                end.y + dy * 4.,
                end.x + dx * 10.,
                end.y + dy * 10.,
                1.3,
                color,
            );
        }
    }
    /// Section meters under each hull and a mast meter beneath them.
    fn draw_meters(&self, side: usize, preview: Option<Preview>, t: f32) {
        let ship = self.ships[side];
        let color = if side == 0 { TEAL } else { self.enemy_color() };
        let ghost = preview.filter(|p| p.side == side).map(|p| p.after);
        let blink = 0.5 + 0.5 * (t * 8.).sin();
        for part in 0..3 {
            let r = part_rect(ship, part);
            part_meter(
                r.x + 2.,
                433.,
                r.w - 4.,
                5.,
                ship.hp[part],
                ship.max[part],
                ghost.map(|g| g[part]),
                color,
                blink,
            );
        }
        let x = X0 + ship.x;
        let mast = with_alpha(SOFT, 0.75);
        draw_line(x - 30., 446., x - 30., 438., 1.4, mast);
        draw_triangle(
            vec2(x - 30., 438.),
            vec2(x - 24., 440.),
            vec2(x - 30., 442.),
            mast,
        );
        part_meter(
            x - 20.,
            441.,
            50.,
            4.,
            ship.hp[3],
            ship.max[3],
            ghost.map(|g| g[3]),
            color,
            blink,
        );
    }
    fn draw_shot(&self, t: f32) {
        let n = self.trace.len;
        for (i, pair) in self.trace.points[..n].windows(2).enumerate() {
            let a = screen(pair[0]);
            let b = screen(pair[1]);
            if in_scene(a) && in_scene(b) {
                let k = (i + 1) as f32 / n as f32;
                draw_line(
                    a.x,
                    a.y,
                    b.x,
                    b.y,
                    1. + k * 1.8,
                    Color::new(1., 0.90, 0.65, 0.12 + 0.6 * k),
                );
            }
        }
        let Some(ball) = self.ball else {
            return;
        };
        let p = screen(ball.pos);
        if !in_scene(p) {
            return;
        }
        let hot = if ball.ammo == Ammo::Shell {
            Color::new(1., 0.62, 0.32, 0.55)
        } else {
            Color::new(1., 0.92, 0.7, 0.32)
        };
        halo(p.x, p.y, 12., hot);
        if ball.ammo == Ammo::Chain {
            let spin = t * 14.;
            let d = vec2(spin.cos(), spin.sin()) * 5.5;
            draw_line(p.x - d.x, p.y - d.y, p.x + d.x, p.y + d.y, 1.4, STEEL);
            iron(p - d, 3., 1.);
            iron(p + d, 3., 1.);
        } else {
            ammo_glyph(ball.ammo, p, 4.6, 1.);
        }
    }
    /// A tick-timed water column where a ball splashed down and a flash where
    /// one struck; numbers and debris come from the effects layer.
    fn draw_impact(&self) {
        let Some(impact) = self.impact else {
            return;
        };
        if self.impact_ticks == 0 {
            return;
        }
        let p = screen(impact);
        let splash = impact.y <= 0.5;
        let total = if splash { 35. } else { 50. };
        let k = ((total - f32::from(self.impact_ticks)) / total).clamp(0., 1.);
        if splash {
            if !(SCENE.x + 8. ..=SCENE.right() - 8.).contains(&p.x) {
                return;
            }
            let h = 44. * (k * PI).sin();
            let foam = Color::new(0.85, 0.95, 0.97, 0.8 * (1. - k));
            draw_triangle(
                vec2(p.x - 8., WATER),
                vec2(p.x - 1., WATER - h),
                vec2(p.x + 2., WATER),
                foam,
            );
            draw_triangle(
                vec2(p.x - 2., WATER),
                vec2(p.x + 3., WATER - h * 0.72),
                vec2(p.x + 8., WATER),
                foam,
            );
            let spread = 8. + k * 34.;
            for dir in [-1., 1.] {
                let x = p.x + dir * spread;
                draw_line(x - 5., WATER + 2., x + 5., WATER + 2., 1.5, foam);
            }
        } else if self.last_damage > 0 && k < 0.6 && in_scene(p) {
            halo(
                p.x,
                p.y,
                12. + 22. * (1. - k),
                Color::new(1., 0.55, 0.2, 0.9 * (1. - k / 0.6)),
            );
        }
    }
    /// The enemy's fuse timer while it lays a volley, and a warning when its
    /// next return will be a double broadside.
    fn draw_telegraphs(&self, t: f32) {
        let foe = self.ships[1];
        let at = screen(vec2(foe.x, 140.)) + vec2(0., bob(t, 1.2, 2.5));
        if self.phase == Phase::EnemyReady {
            let done = 1. - f32::from(self.timer) / 65.;
            halo(at.x, at.y, 24., with_alpha(RED, 0.28));
            shapes::fill_circle(at, 13., Color::new(0.06, 0.03, 0.03, 0.85));
            ring_meter(at.x, at.y, 13., 3., done, RED);
            icon(Icon::Alert, at.x, at.y, 13., RED);
        } else if self.phase == Phase::Aim && self.double_volley_due() {
            icon_chip(Icon::Bomb, "x2", at.x - 26., at.y - 10., 14., RED);
        }
    }
    /// Whether the enemy's return to this exchange is a double broadside: the
    /// Bell Brothers fire twice every third exchange unless fallen rigging
    /// skips an odd exchange's volley altogether.
    fn double_volley_due(&self) -> bool {
        self.battle_info().tactic == Tactic::Broadside
            && self.round.is_multiple_of(3)
            && !(self.ships[1].hp[3] == 0 && self.round % 2 == 1)
    }
    fn draw_wind(&self, t: f32) {
        let c = GAUGE;
        let (w, h) = (176., 30.);
        let r = Rect::new(c.x - w * 0.5, c.y - h * 0.5, w, h);
        rounded(r.x, r.y, r.w, r.h, 15., Color::new(0.02, 0.05, 0.08, 0.66));
        outline(r.x, r.y, r.w, r.h, 15., 1., Color::new(1., 1., 1., 0.08));
        icon(Icon::Wind, r.x + 19., c.y, 18., CREAM);
        let strength = self.wind.abs();
        let (x0, x1) = (r.x + 40., r.right() - 46.);
        if self.wind == 0. {
            shapes::stroke_circle(vec2((x0 + x1) * 0.5, c.y), 4., 1.5, SOFT);
        } else {
            let dir = self.wind.signum();
            let span = x1 - x0;
            draw_line(x0, c.y, x1, c.y, 1.2, with_alpha(CREAM, 0.22));
            let n = ((strength / 3.).ceil() as usize).clamp(1, 5);
            let flow = fract(t * (0.35 + strength * 0.04));
            for k in 0..n {
                let u = fract((k as f32 + flow) / n as f32);
                let x = if dir > 0. {
                    x0 + u * span
                } else {
                    x1 - u * span
                };
                let fade = (u * 4.).min((1. - u) * 4.).clamp(0., 1.);
                chevron(x, c.y, dir, 6., with_alpha(GOLD, fade));
            }
        }
        say_mid(
            &(strength.round() as u16).to_string(),
            r.right() - 24.,
            c.y,
            style(18., GOLD),
        );
        // Powder Rose's gusts reverse after every exchange.
        if self.battle_info().tactic == Tactic::Bomber {
            icon_badge(
                Icon::Restart,
                r.right() + 16.,
                c.y,
                24.,
                Color::new(0.30, 0.12, 0.08, 0.95),
                FLAME,
            );
        }
    }
    /// The damage the aimed shot would deal, floating over its target.
    fn draw_preview(&self, preview: Option<Preview>, t: f32) {
        let Some(p) = preview else {
            return;
        };
        let ship = self.ships[p.side];
        let lost: u16 = (0..4).map(|q| p.before[q].saturating_sub(p.after[q])).sum();
        let color = if p.side == 1 { GOLD } else { RED };
        let anchor = part_top(ship, p.part) + vec2(0., -10. + bob(t, 1.1, 2.));
        let label = if lost > 0 {
            format!("-{lost}")
        } else {
            "0".to_string()
        };
        let s = plain(16., color);
        let glyph = if p.side == 0 {
            Some((Icon::Alert, RED))
        } else if lost == 0 {
            Some((Icon::Shield, STEEL))
        } else {
            None
        };
        let gw = if glyph.is_some() { 18. } else { 0. };
        let w = text_width(&label, s) + gw + 18.;
        let x = (anchor.x - w * 0.5).clamp(SCENE.x + 6., SCENE.right() - w - 6.);
        let r = Rect::new(x, anchor.y - 12., w, 24.);
        draw_triangle(
            vec2(anchor.x - 5., r.bottom() - 1.),
            vec2(anchor.x + 5., r.bottom() - 1.),
            vec2(anchor.x, r.bottom() + 5.),
            Color::new(0.02, 0.04, 0.06, 0.88),
        );
        rounded(r.x, r.y, r.w, r.h, 12., Color::new(0.02, 0.04, 0.06, 0.88));
        outline(r.x, r.y, r.w, r.h, 12., 1.2, with_alpha(color, 0.75));
        let mut tx = r.x + 9.;
        if let Some((g, c)) = glyph {
            icon(g, tx + 7., anchor.y, 14., c);
            tx += gw;
        }
        say_left(&label, tx, anchor.y, s);
    }
    fn draw_retreat_hint(&self) {
        let x = SCENE.x + 14.;
        let cy = SCENE.y + 21.;
        let w = mini_cap("C", x, cy - 7.5, 0.85);
        icon(
            Icon::Flag,
            x + w + 13.,
            cy,
            16.,
            Color::new(0.9, 0.92, 0.94, 0.8),
        );
    }
    /// Exact integrity (and armor) of the hovered ship part.
    fn draw_tooltip(&self) {
        if self.modal_open() {
            return;
        }
        let p = pointer();
        if !SCENE.contains(p) {
            return;
        }
        for (side, ship) in self.ships.iter().enumerate() {
            for part in 0..4 {
                if !part_rect(*ship, part).contains(p) {
                    continue;
                }
                let color = if side == 0 { TEAL } else { self.enemy_color() };
                let label = format!("{}/{}", ship.hp[part], ship.max[part]);
                let armor = if part < 3 { ship.armor[part] } else { 0 };
                let s = plain(14., CREAM);
                let armor_label = armor.to_string();
                let aw = if armor > 0 {
                    18. + text_width(&armor_label, s) + 8.
                } else {
                    0.
                };
                let w = text_width(&label, s) + 20. + aw;
                let at = vec2(part_rect(*ship, part).center().x, 464.);
                let x = (at.x - w * 0.5).clamp(SCENE.x + 6., SCENE.right() - w - 6.);
                rounded(
                    x,
                    at.y - 12.,
                    w,
                    24.,
                    12.,
                    Color::new(0.02, 0.04, 0.06, 0.92),
                );
                outline(x, at.y - 12., w, 24., 12., 1.2, with_alpha(color, 0.8));
                let tx = x + 10. + say_left(&label, x + 10., at.y, s);
                if armor > 0 {
                    icon(Icon::Shield, tx + 15., at.y, 13., STEEL);
                    say_left(&armor_label, tx + 24., at.y, plain(14., STEEL));
                }
                return;
            }
        }
    }

    // -----------------------------------------------------------------------
    // The gun deck.

    fn draw_deck(&self, t: f32) {
        let aim = self.phase == Phase::Aim;
        let b = BAR;
        shapes::drop_shadow(
            b,
            12.,
            Shadow::soft(Color::new(0., 0., 0., 0.38))
                .offset(vec2(0., 4.))
                .blur(14.),
        );
        rounded_gradient(
            b.x,
            b.y,
            b.w,
            b.h,
            12.,
            Color::new(0.09, 0.125, 0.16, 1.),
            Color::new(0.05, 0.075, 0.10, 1.),
        );
        outline(b.x, b.y, b.w, b.h, 12., 1., Color::new(1., 1., 1., 0.07));
        let a = if aim { 1. } else { 0.5 };
        for well in [ANGLE, POWER] {
            rounded(
                well.x,
                well.y,
                well.w,
                well.h,
                10.,
                Color::new(0., 0., 0., 0.2),
            );
            outline(
                well.x,
                well.y,
                well.w,
                well.h,
                10.,
                1.,
                Color::new(1., 1., 1., 0.05),
            );
        }
        stepper(ANGLE_DOWN, Icon::ArrowLeft, aim, a);
        stepper(ANGLE_UP, Icon::ArrowRight, aim, a);
        let cx = (ANGLE_DOWN.right() + ANGLE_UP.x) * 0.5 - 3.;
        let cy = ANGLE.center().y;
        let w = say_mid(
            &self.setting.angle.to_string(),
            cx,
            cy,
            style(28., with_alpha(TEAL, a)),
        );
        shapes::stroke_circle(
            vec2(cx + w * 0.5 + 5., cy - 9.),
            3.,
            1.5,
            with_alpha(TEAL, a),
        );
        stepper(POWER_DOWN, Icon::ArrowDown, aim, a);
        stepper(POWER_UP, Icon::ArrowUp, aim, a);
        say_mid(
            &self.setting.power.to_string(),
            (POWER_DOWN.right() + METER.x) * 0.5,
            POWER.center().y,
            style(28., with_alpha(TEAL, a)),
        );
        self.draw_power_meter(a, t);
        for i in 0..3 {
            self.draw_rack(i, aim, t);
        }
        self.draw_table_toggle(aim, a);
        self.draw_fire(aim, t);
    }
    /// Powder as a charge meter: the fill, a gold notch for the last shot, a
    /// teal diamond for the firing table and a locked zone without a mast.
    fn draw_power_meter(&self, a: f32, t: f32) {
        let m = METER;
        let (lo, hi) = (25., 90.);
        let at = |v: f32| m.x + (v - lo) / (hi - lo) * m.w;
        rounded(
            m.x - 1.,
            m.y - 1.,
            m.w + 2.,
            m.h + 2.,
            7.,
            Color::new(1., 1., 1., 0.12 * a),
        );
        rounded(m.x, m.y, m.w, m.h, 6., Color::new(0.01, 0.02, 0.04, 0.9));
        let power = f32::from(self.setting.power);
        let fill = Rect::new(m.x, m.y, (at(power) - m.x).max(m.h), m.h);
        shapes::fill_rounded_rect(
            fill,
            6.,
            Fill::horizontal(m, with_alpha(TEAL, a), with_alpha(FLAME, a)),
        );
        // A glint sweeps along the charge.
        let glint = m.x + fract(t * 0.6) * fill.w;
        halo(glint, m.center().y, 7., Color::new(1., 1., 1., 0.25 * a));
        for v in (30..90).step_by(5) {
            let x = at(v as f32);
            draw_line(
                x,
                m.y + 3.,
                x,
                m.bottom() - 3.,
                1.,
                Color::new(0., 0., 0., 0.3),
            );
        }
        let limit = f32::from(self.power_limit());
        if limit < hi {
            let x = at(limit);
            rounded(
                x,
                m.y,
                m.right() - x,
                m.h,
                6.,
                Color::new(0.5, 0.12, 0.1, 0.85 * a),
            );
            let mut hx = x + 3.;
            while hx < m.right() - 2. {
                draw_line(
                    hx,
                    m.bottom() - 2.,
                    hx + 4.,
                    m.y + 2.,
                    1.,
                    with_alpha(RED, 0.8 * a),
                );
                hx += 6.;
            }
            icon(
                Icon::Lock,
                (x + m.right()) * 0.5,
                m.y - 9.,
                13.,
                with_alpha(RED, a),
            );
        }
        if let Some(last) = self.last_setting {
            let x = at(f32::from(last.power));
            draw_triangle(
                vec2(x, m.bottom() + 2.),
                vec2(x - 4., m.bottom() + 9.),
                vec2(x + 4., m.bottom() + 9.),
                with_alpha(GOLD, 0.9 * a),
            );
        }
        if self.show_table {
            let c = vec2(at(f32::from(self.table.power)), m.y - 7.);
            shapes::fill_polygon(
                &[
                    c + vec2(0., -4.5),
                    c + vec2(4.5, 0.),
                    c + vec2(0., 4.5),
                    c + vec2(-4.5, 0.),
                ],
                with_alpha(TEAL, a),
            );
        }
        let knob = vec2(at(power), m.center().y);
        shapes::fill_circle(knob, 6., with_alpha(CREAM, a));
        shapes::stroke_circle(knob, 6., 1.5, with_alpha(darken(FLAME, 0.2), a));
    }
    fn draw_rack(&self, i: usize, aim: bool, t: f32) {
        let rect = AMMO[i];
        let ammo = ORDER[i];
        let count = self.ammo[i];
        let selected = self.setting.ammo == ammo;
        let empty = count == 0;
        let hover = aim && hovered(rect);
        let lift = if selected {
            3.
        } else if hover {
            2.
        } else {
            0.
        };
        let r = Rect::new(rect.x, rect.y - lift, rect.w, rect.h);
        let base = if aim { 1. } else { 0.55 };
        let a = base * if empty && !selected { 0.55 } else { 1. };
        if selected && aim {
            halo(
                r.center().x,
                r.center().y,
                54.,
                with_alpha(GOLD, 0.1 + 0.08 * pulse(t, 0.8)),
            );
        }
        shapes::drop_shadow(
            r,
            10.,
            Shadow::soft(Color::new(0., 0., 0., 0.4 * a))
                .offset(vec2(0., 2. + lift))
                .blur(8. + lift * 2.),
        );
        if selected {
            rounded_gradient(
                r.x,
                r.y,
                r.w,
                r.h,
                10.,
                with_alpha(lighten(GOLD, 0.2), base),
                with_alpha(darken(GOLD, 0.25), base),
            );
            rounded_gradient(
                r.x + 2.,
                r.y + 2.,
                r.w - 4.,
                r.h - 4.,
                8.,
                Color::new(0.19, 0.20, 0.18, base),
                Color::new(0.10, 0.12, 0.12, base),
            );
        } else {
            let top = if hover {
                Color::new(0.20, 0.27, 0.33, a)
            } else {
                Color::new(0.15, 0.20, 0.25, a)
            };
            rounded_gradient(
                r.x,
                r.y,
                r.w,
                r.h,
                10.,
                top,
                Color::new(0.10, 0.14, 0.18, a),
            );
            let rim = if hover {
                with_alpha(TEAL, 0.7)
            } else {
                Color::new(1., 1., 1., 0.08 * a)
            };
            outline(r.x, r.y, r.w, r.h, 10., 1.2, rim);
        }
        mini_cap(DIGITS[i], r.x + 6., r.y + 6., a);
        let glyph = vec2(r.x + 28., r.y + 27.);
        ammo_glyph(ammo, glyph, 9.5, a);
        if empty {
            cross(glyph, 7., 2.2, with_alpha(RED, 0.9 * base));
        }
        let ink = if selected { GOLD } else { CREAM };
        let s = style(22., with_alpha(ink, a)).with_align(TextAlign::Right);
        draw_text_styled(
            &count.to_string(),
            r.right() - 9.,
            s.centered_baseline(r.y + 25.),
            s,
        );
        // One pip per round this island started with.
        let cap = [8, 2, 2 + self.upgrades[2]][i].max(count);
        let gap = 7.;
        let x0 = r.center().x - (f32::from(cap) - 1.) * gap * 0.5;
        for k in 0..cap {
            let p = vec2(x0 + f32::from(k) * gap, r.bottom() - 9.);
            if k < count {
                shapes::fill_circle(p, 2.3, with_alpha(ink, 0.9 * a));
            } else {
                shapes::stroke_circle(p, 2., 1., with_alpha(SOFT, 0.35 * a));
            }
        }
        if self.show_table && self.table.ammo == ammo {
            icon_badge(
                Icon::Bulb,
                r.right() - 4.,
                r.y + 4.,
                18.,
                with_alpha(TEAL, a),
                with_alpha(INK, a),
            );
        }
    }
    fn draw_table_toggle(&self, aim: bool, a: f32) {
        let r = TABLE;
        let on = self.show_table;
        let hover = aim && hovered(r);
        let lift = if hover { 2. } else { 0. };
        let y = r.y - lift;
        if on {
            halo(r.center().x, y + 22., 26., with_alpha(GOLD, 0.3 * a));
        }
        let top = if hover {
            Color::new(0.20, 0.27, 0.33, a)
        } else {
            Color::new(0.15, 0.20, 0.25, a)
        };
        rounded_gradient(r.x, y, r.w, r.h, 10., top, Color::new(0.10, 0.14, 0.18, a));
        let rim = if on {
            with_alpha(GOLD, 0.8 * a)
        } else if hover {
            with_alpha(TEAL, 0.7)
        } else {
            Color::new(1., 1., 1., 0.08 * a)
        };
        outline(r.x, y, r.w, r.h, 10., 1.2, rim);
        let ink = if on {
            with_alpha(GOLD, a)
        } else {
            with_alpha(MUTED, a)
        };
        icon(Icon::Bulb, r.center().x, y + 21., 24., ink);
        mini_cap("X", r.center().x - 7.5, y + 36., a);
    }
    fn draw_fire(&self, aim: bool, t: f32) {
        let ready = aim && self.ammo[self.setting.ammo.index()] > 0;
        let hover = ready && hovered(FIRE);
        let lift = if hover { 2. } else { 0. };
        let r = Rect::new(FIRE.x, FIRE.y - lift, FIRE.w, FIRE.h);
        if ready {
            let breath = pulse(t, 0.6);
            shapes::drop_shadow(
                r,
                12.,
                Shadow::soft(with_alpha(GOLD, 0.2 + 0.25 * breath)).blur(14. + 10. * breath),
            );
        }
        shapes::drop_shadow(
            r,
            12.,
            Shadow::soft(Color::new(0., 0., 0., 0.4))
                .offset(vec2(0., 3. + lift))
                .blur(10.),
        );
        let a = if ready { 1. } else { 0.45 };
        if ready {
            let top = lighten(GOLD, if hover { 0.32 } else { 0.22 });
            rounded_gradient(r.x, r.y, r.w, r.h, 12., top, darken(GOLD, 0.14));
        } else {
            rounded_gradient(
                r.x,
                r.y,
                r.w,
                r.h,
                12.,
                Color::new(0.20, 0.24, 0.28, 1.),
                Color::new(0.12, 0.15, 0.18, 1.),
            );
        }
        let ink = if ready { INK } else { with_alpha(SOFT, 0.6) };
        let cy = r.center().y;
        ammo_glyph(
            self.setting.ammo,
            vec2(r.x + 26., cy + 1.),
            10.,
            if ready { 1. } else { 0.5 },
        );
        say_left("Fire", r.x + 46., cy - 1., style(22., ink));
        let cap = text_width("Space", plain(11., PAPER)) + 8.;
        mini_cap("Space", r.right() - cap - 8., cy - 7.5, a);
    }

    // -----------------------------------------------------------------------
    // Modal cards: briefing, dock and setback.

    fn draw_modal(&self, t: f32) {
        rounded(
            SCENE.x,
            SCENE.y,
            SCENE.w,
            SCENE.h,
            12.,
            Color::new(0.01, 0.03, 0.05, 0.6),
        );
        let c = CARD;
        card(c.x, c.y, c.w, c.h, Color::new(0.07, 0.10, 0.13, 0.98));
        rounded(c.x + 32., c.y, 72., 3., 1.5, GOLD);
        match self.phase {
            Phase::Briefing => self.briefing_card(t),
            Phase::Dock => self.dock_card(),
            Phase::Setback => self.setback_card(t),
            _ => {}
        }
    }
    fn briefing_card(&self, t: f32) {
        let info = self.battle_info();
        let color = self.enemy_color();
        let x = CARD.x + 32.;
        // The enemy's colors fly over the card.
        let e = vec2(CARD.right() - 96., CARD.y + 104. + bob(t, 0.6, 3.));
        halo(e.x, e.y, 64., with_alpha(color, 0.22));
        icon(Icon::Flag, e.x, e.y, 76., color);
        say(info.island, x, CARD.y + 62., style(32., CREAM));
        let cy = CARD.y + 92.;
        let names = plain(15., lighten(color, 0.3));
        icon(Icon::Person, x + 8., cy, 16., color);
        let w = say_left(info.captain, x + 22., cy, names);
        icon(Icon::Ship, x + 22. + w + 20., cy, 16., color);
        say_left(info.ship, x + 22. + w + 34., cy, names);
        // The one line that matters: how this captain fights.
        let tactic = tactic_icon(info.tactic);
        icon_badge(
            tactic,
            x + 16.,
            CARD.y + 138.,
            32.,
            with_alpha(color, 0.25),
            lighten(color, 0.35),
        );
        say_left(info.story, x + 42., CARD.y + 138., style(18., CREAM));
        // The conditions: wind, armor and the enemy's integrity.
        let cy = CARD.y + 196.;
        let mut cx = x;
        let size = 15.;
        let chip_y = cy - size * 0.725;
        let wind = self.wind.abs().round() as u16;
        cx += icon_chip(Icon::Wind, &wind.to_string(), cx, chip_y, size, CREAM) + 4.;
        if self.wind != 0. {
            let arrow = if self.wind > 0. {
                Icon::ArrowRight
            } else {
                Icon::ArrowLeft
            };
            cx += icon_chip(arrow, "", cx, chip_y, size, CREAM) + 10.;
        } else {
            cx += 6.;
        }
        let armor = self.ships[1].armor[0];
        if armor > 0 {
            cx += icon_chip(Icon::Shield, &armor.to_string(), cx, chip_y, size, STEEL) + 10.;
        }
        icon_chip(
            Icon::Flag,
            &self.ships[1].hull().to_string(),
            cx,
            chip_y,
            size,
            color,
        );
        action_button(START, "Space", Icon::Flag, "Engage", true, true);
    }
    fn dock_card(&self) {
        let x = CARD.x + 32.;
        let you = self.ships[0];
        icon(Icon::Anchor, x + 13., CARD.y + 48., 26., GOLD);
        say_left("Shipyard", x + 34., CARD.y + 48., style(28., CREAM));
        let purse = style(26., GOLD).with_align(TextAlign::Right);
        let w = draw_text_styled(
            &self.credits.to_string(),
            CARD.right() - 32.,
            purse.centered_baseline(PURSE_AT.y),
            purse,
        )
        .width;
        icon(
            Icon::Coin,
            CARD.right() - 32. - w - 16.,
            PURSE_AT.y,
            24.,
            GOLD,
        );
        // Hover a repair to preview its effect on the hull and mast.
        let repaired = (hovered(SHOP[0]) && self.cost(1).is_some()).then(|| {
            let mut ship = you;
            ship.repair(80);
            ship.hp
        });
        let cy = CARD.y + 86.;
        icon(Icon::Flag, x + 8., cy, 16., TEAL);
        hull_bar(x + 24., cy - 5., 196., 10., you, TEAL, repaired);
        let s = plain(14., SOFT);
        let most: u16 = you.max[..3].iter().sum();
        say_left(&format!("{}/{most}", you.hull()), x + 230., cy, s);
        let mx = x + 336.;
        let mast = with_alpha(SOFT, 0.8);
        draw_line(mx, cy + 6., mx, cy - 7., 1.6, mast);
        draw_triangle(
            vec2(mx, cy - 7.),
            vec2(mx + 8., cy - 4.),
            vec2(mx, cy - 1.),
            mast,
        );
        part_meter(
            mx + 14.,
            cy - 4.,
            84.,
            8.,
            you.hp[3],
            you.max[3],
            repaired.map(|hp| hp[3]),
            TEAL,
            0.5 + 0.5 * (clock() * 7.).sin(),
        );
        say_left(&format!("{}/{}", you.hp[3], you.max[3]), mx + 108., cy, s);
        for i in 0..4 {
            self.shop_card(i);
        }
        action_button(SAIL, "Space", Icon::Ship, "Sail", true, true);
        // Sailing refills every magazine.
        let cy = SAIL.center().y;
        let mut gx = SAIL.right() + 28.;
        icon(Icon::Restart, gx, cy, 18., SOFT);
        gx += 22.;
        for ammo in ORDER {
            ammo_glyph(ammo, vec2(gx, cy + 1.), 7., 0.9);
            gx += 22.;
        }
    }
    fn shop_card(&self, i: usize) {
        let rect = SHOP[i];
        let slot = i + 1;
        let cost = self.cost(slot);
        let affordable = cost.is_some_and(|c| c <= self.credits);
        let hover = hovered(rect) && cost.is_some();
        let lift = if hover { 2. } else { 0. };
        let r = Rect::new(rect.x, rect.y - lift, rect.w, rect.h);
        let a = if cost.is_some() { 1. } else { 0.5 };
        shapes::drop_shadow(
            r,
            10.,
            Shadow::soft(Color::new(0., 0., 0., 0.35 * a))
                .offset(vec2(0., 2. + lift))
                .blur(8.),
        );
        let top = if hover {
            Color::new(0.19, 0.26, 0.32, a)
        } else {
            Color::new(0.14, 0.19, 0.24, a)
        };
        rounded_gradient(
            r.x,
            r.y,
            r.w,
            r.h,
            10.,
            top,
            Color::new(0.09, 0.13, 0.17, a),
        );
        let rim = if hover && affordable {
            with_alpha(TEAL, 0.8)
        } else if hover {
            with_alpha(RED, 0.7)
        } else {
            Color::new(1., 1., 1., 0.08)
        };
        outline(r.x, r.y, r.w, r.h, 10., 1.2, rim);
        mini_cap(DIGITS[i], r.x + 8., r.y + 8., a);
        let (glyph, gain) = shop_gain(slot);
        let cy = r.center().y;
        icon(glyph, r.x + 48., cy, 30., with_alpha(TEAL, a));
        let gx = r.x + 70.;
        let gw = say_left(gain, gx, cy - 1., style(22., with_alpha(CREAM, a)));
        if slot == 4 {
            ammo_glyph(Ammo::Shell, vec2(gx + gw + 12., cy), 7., a);
        }
        // Upgrade levels as pips (two each).
        if slot > 1 {
            let level = self.upgrades[slot - 2];
            for k in 0..2_u8 {
                let p = vec2(gx + 4. + f32::from(k) * 11., r.bottom() - 10.);
                if k < level {
                    shapes::fill_circle(p, 3.2, with_alpha(TEAL, a));
                } else {
                    shapes::stroke_circle(p, 3., 1.2, with_alpha(SOFT, 0.5 * a));
                }
            }
        }
        // Price, or a check when nothing is left to buy.
        let right = r.right() - 12.;
        match cost {
            Some(price) => {
                let color = if affordable { GOLD } else { RED };
                let s = style(18., color).with_align(TextAlign::Right);
                let w =
                    draw_text_styled(&price.to_string(), right, s.centered_baseline(cy), s).width;
                icon(Icon::Coin, right - w - 12., cy, 18., color);
            }
            None => icon(Icon::Check, right - 10., cy, 20., TEAL),
        }
    }
    fn setback_card(&self, t: f32) {
        let cx = CARD.center().x;
        let (glyph, title, color) = match self.setback_cause() {
            Cause::Sunk => (Icon::Skull, "Sunk", RED),
            Cause::Empty => (Icon::Crate, "Out of shot", RED),
            Cause::Retreat => (Icon::Flag, "Retreat", CREAM),
        };
        let e = vec2(cx, CARD.y + 80. + bob(t, 0.6, 2.));
        halo(e.x, e.y, 58., with_alpha(color, 0.18));
        icon(glyph, e.x, e.y, 66., color);
        say_mid(title, cx, CARD.y + 152., style(32., CREAM));
        // Lifeboats left; the one a retry would spend pulses while hovered.
        let cy = CARD.y + 200.;
        let spending = self.lifeboats > 0 && hovered(RETRY);
        for i in 0..2_u8 {
            let lit = i < self.lifeboats;
            let next = spending && i + 1 == self.lifeboats;
            let c = if next {
                with_alpha(CREAM, 0.35 + 0.65 * pulse(t, 2.))
            } else if lit {
                CREAM
            } else {
                with_alpha(MUTED, 0.3)
            };
            icon(Icon::Ship, cx - 22. + f32::from(i) * 44., cy, 32., c);
        }
        let lifeboats = self.lifeboats > 0;
        action_button(RETRY, "C", Icon::Restart, "Retry", true, lifeboats);
        if lifeboats {
            // The price, inside the button: one lifeboat.
            let lift = if hovered(RETRY) { 2. } else { 0. };
            let cy = RETRY.center().y - lift;
            let right = RETRY.right() - 14.;
            icon(Icon::Ship, right - 8., cy, 16., INK);
            say_left("-1", right - 36., cy, style(16., INK));
        }
        action_button(RETIRE, "Space", Icon::Flag, "Retire", false, true);
    }
}

/// A stepper key: an arrow that nudges angle or powder by one.
fn stepper(r: Rect, glyph: Icon, enabled: bool, a: f32) {
    let hover = enabled && hovered(r);
    let lift = if hover { 1.5 } else { 0. };
    rounded(
        r.x,
        r.y + 2.,
        r.w,
        r.h,
        7.,
        Color::new(0., 0., 0., 0.45 * a),
    );
    let top = if hover {
        Color::new(0.28, 0.34, 0.40, a)
    } else {
        Color::new(0.22, 0.27, 0.33, a)
    };
    rounded_gradient(
        r.x,
        r.y - lift,
        r.w,
        r.h,
        7.,
        top,
        Color::new(0.13, 0.17, 0.21, a),
    );
    if hover {
        outline(r.x, r.y - lift, r.w, r.h, 7., 1.2, with_alpha(TEAL, 0.85));
    }
    let ink = if hover { PAPER } else { SOFT };
    icon(
        glyph,
        r.center().x,
        r.center().y - lift,
        14.,
        with_alpha(ink, a),
    );
}
/// What each dock purchase adds, as an icon and a number.
fn shop_gain(slot: usize) -> (Icon, &'static str) {
    match slot {
        1 => (Icon::Wrench, "+80"),
        2 => (Icon::Sword, "+8"),
        3 => (Icon::Shield, "+5"),
        _ => (Icon::Crate, "+1"),
    }
}
/// Each captain's signature, as one pictogram.
fn tactic_icon(tactic: Tactic) -> Icon {
    match tactic {
        Tactic::Apprentice => Icon::Sprout,
        Tactic::Gunner => Icon::Bolt,
        Tactic::Rigger => Icon::Wind,
        Tactic::Ironwall => Icon::Shield,
        Tactic::Lobber => Icon::ArrowUp,
        Tactic::Bomber => Icon::Flame,
        Tactic::Broadside => Icon::Bomb,
        Tactic::Admiral => Icon::Target,
    }
}
/// A destroyed section: a charred stub with embers and drifting smoke.
#[allow(clippy::too_many_arguments)]
fn draw_wreck(
    lb: f32,
    rb: f32,
    top: f32,
    bottom: f32,
    cx: f32,
    color: Color,
    t: f32,
    part: usize,
    wind: f32,
) {
    let char = Color::new(0.10, 0.08, 0.07, 1.);
    let stub = top + 15.;
    shapes::fill_polygon(
        &[
            vec2(lb, stub),
            vec2(rb, stub + 2.),
            vec2(rb - 1., bottom),
            vec2(lb + 1., bottom),
        ],
        char,
    );
    // Jagged broken planks.
    for (x, h) in [(lb + 3., 6.), (cx - 2., 9.), (rb - 5., 5.)] {
        draw_triangle(
            vec2(x - 3., stub + 1.),
            vec2(x + 3., stub + 1.),
            vec2(x + 1., stub - h),
            darken(color, 0.55),
        );
    }
    let ember = pulse(t + part as f32 * 0.7, 1.3);
    draw_rectangle(
        cx - 2.,
        stub + 3.,
        4.,
        2.,
        Color::new(1., 0.5, 0.2, 0.4 + 0.5 * ember),
    );
    for k in 0..3 {
        let ph = fract(t * 0.45 + k as f32 / 3. + part as f32 * 0.37);
        let q = vec2(
            cx + (ph * 6. + k as f32).sin() * 3. + ph * wind * 1.2,
            stub - 4. - ph * 42.,
        );
        if in_scene(q) {
            shapes::fill_circle(
                q,
                2.5 + ph * 6.,
                Color::new(0.55, 0.58, 0.62, 0.3 * (1. - ph)),
            );
        }
    }
}
/// Faded polyline of a recorded flight, clipped to the chart.
fn trajectory(trace: &Trace, color: Color, width: f32) {
    for pair in trace.points[..trace.len].windows(2) {
        let a = screen(pair[0]);
        let b = screen(pair[1]);
        if in_scene(a) && in_scene(b) {
            draw_line(a.x, a.y, b.x, b.y, width, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn finish(game: &mut Game) {
        for tick in 0..100_000 {
            game.step(&game.pilot(tick));
            if game.outcome() != Outcome::Playing {
                return;
            }
        }
    }
    #[test]
    fn progress_pips_count_islands_from_first_to_last() {
        let mut game = Game::new();
        assert_eq!(ArcadeGame::progress(&game), Some((1, 8)));
        finish(&mut game);
        assert_eq!(game.outcome(), Outcome::Won);
        assert_eq!(ArcadeGame::progress(&game), Some((8, 8)));
        assert_eq!(game.hud().stage, "Storm Crown");
        assert!(game.hud().objective.split_whitespace().count() <= 5);
    }
    #[test]
    fn the_double_broadside_is_telegraphed_only_when_it_will_come() {
        let mut game = Game::new();
        game.round = 3;
        assert!(
            !game.double_volley_due(),
            "only the Bell Brothers fire twice"
        );
        game.battle = 6;
        assert!(game.double_volley_due());
        game.round = 4;
        assert!(!game.double_volley_due());
        // Fallen rigging skips odd exchanges' volleys entirely.
        game.ships[1].hp[3] = 0;
        game.round = 3;
        assert!(!game.double_volley_due());
        game.round = 6;
        assert!(game.double_volley_due());
    }
    #[test]
    fn a_landed_shot_bursts_and_pops_its_damage_at_the_struck_section() {
        let mut game = Game::new();
        let mut fx = Juice::new();
        for tick in 0..2_000 {
            let cue = game.step(&game.pilot(tick));
            if let Some(Event::Hit { side: 1, .. }) = game.events.iter().next() {
                ArcadeGame::effects(&game, cue, &mut fx);
                break;
            }
        }
        let counts = fx.counts();
        assert!(counts.sparks >= 20, "{counts:?}");
        assert!(counts.popups >= 1 && counts.rings >= 1, "{counts:?}");
    }
    #[test]
    fn refused_fire_is_felt_at_the_empty_rack_and_quiet_ticks_show_nothing() {
        let mut game = Game::new();
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        game.ammo[0] = 0;
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        let mut fx = Juice::new();
        ArcadeGame::effects(&game, Cue::None, &mut fx);
        assert_eq!(game.events.iter().count(), 1);
        assert!(fx.counts().rings == 1 && fx.counts().popups == 1);
        // Nothing accepted, nothing shown.
        game.step(&Input::default());
        let mut quiet = Juice::new();
        ArcadeGame::effects(&game, Cue::None, &mut quiet);
        assert!(quiet.is_idle());
    }
}
