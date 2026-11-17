//! Modal cards: the route briefing, the refit desk and recovery. Buttons and
//! refit rows rest exactly on the model's pointer targets.
use crate::model::Cause;
use crate::panel::{REFITS, icon_chip_width, stamp};
use crate::view::{
    BLUE, FLOOR, GOLD, GREEN, HAIRLINE, PURPLE, RED, SKY_W, SKY_X, SKY_Y, ease, icon_value_width,
    ink, keycap_width, mix, say,
};
use crate::{Game, Phase};
use ferravex_arcade::shapes;
use ferravex_arcade::typography::{draw_text_styled, text_width};
use ferravex_arcade::*;

const X0: f32 = 132.;
const X1: f32 = 640.;
/// The wide card button, as the model's pointer rules define it.
const BUTTON: Rect = Rect {
    x: 132.,
    y: 486.,
    w: 508.,
    h: 41.,
};
/// The refit desk waits for the delivery celebration; recovery for the crash.
const DEBRIEF_DELAY: f32 = 0.8;
const WRECK_DELAY: f32 = 0.9;

/// Top of refit row `i` (the model's click band is 50 units tall).
pub(crate) fn row_top(i: usize) -> f32 {
    282. + i as f32 * 58.
}
/// Centre of refit row `i`'s price chip.
pub(crate) fn price_at(i: usize) -> Vec2 {
    vec2(X1 - 44., row_top(i) + 25.)
}
/// Centre of level pip `level` (zero-based) on refit row `i`.
pub(crate) fn pip_at(i: usize, level: u8) -> Vec2 {
    vec2(X1 - 143. + f32::from(level) * 18., row_top(i) + 25.)
}
/// What one more level of each refit does.
fn upgrade_effects(i: usize) -> &'static [(Icon, &'static str)] {
    match i {
        0 => &[(Icon::Bolt, "+14"), (Icon::Fuel, "-8%")],
        1 => &[(Icon::Fuel, "+30")],
        _ => &[(Icon::Target, "+3"), (Icon::Shield, "+15%")],
    }
}
fn hovered(r: Rect) -> bool {
    let p = pointer();
    p.x >= r.x && p.x <= r.x + r.w && p.y >= r.y && p.y <= r.y + r.h
}
fn scrim(alpha: f32) {
    rounded(
        SKY_X,
        SKY_Y,
        SKY_W,
        FLOOR - SKY_Y,
        8.,
        with_alpha(INK, 0.55 * alpha),
    );
}
/// The wide card button: key cap, icon and one word. It lifts and brightens
/// under the pointer; `dy` only offsets its entrance.
fn button(key: &str, glyph: Icon, label: &str, color: Color, dy: f32) {
    let hover = hovered(BUTTON);
    let r = Rect::new(
        BUTTON.x,
        BUTTON.y + dy - if hover { 2. } else { 0. },
        BUTTON.w,
        BUTTON.h,
    );
    shapes::drop_shadow(
        r,
        10.,
        shapes::Shadow::soft(Color::new(0., 0., 0., if hover { 0.5 } else { 0.35 }))
            .offset(vec2(0., if hover { 5. } else { 3. }))
            .blur(if hover { 14. } else { 10. }),
    );
    let base = if hover { lighten(color, 0.14) } else { color };
    rounded_gradient(
        r.x,
        r.y,
        r.w,
        r.h,
        10.,
        lighten(base, 0.12),
        darken(base, 0.12),
    );
    outline(
        r.x,
        r.y,
        r.w,
        r.h,
        10.,
        1.,
        Color::new(1., 1., 1., if hover { 0.55 } else { 0.25 }),
    );
    let s = ink(18., INK);
    let key_w = keycap_width(key);
    let total = key_w + 12. + 18. + 8. + text_width(label, s);
    let left = r.x + (r.w - total) * 0.5;
    let cy = r.y + r.h * 0.5;
    keycap(key, "", left, cy - 11., PAPER);
    icon(glyph, left + key_w + 21., cy, 18., INK);
    draw_text_styled(label, left + key_w + 38., s.centered_baseline(cy), s);
}

impl Game {
    pub(crate) fn draw_card(&self) {
        let age = self.phase_age();
        let now = clock();
        match self.phase {
            Phase::Briefing => self.draw_briefing(age, now),
            Phase::Debrief => self.draw_refits(age, now),
            Phase::Wreck => self.draw_recovery(age, now),
            _ => {}
        }
    }

    /// The world's name and conditions over the live chart, where the route
    /// itself is drawn; then a launch button.
    fn draw_briefing(&self, age: f32, now: f32) {
        let enter = ease(age / 0.35);
        let (x, y, w, h) = (44., 134. - (1. - enter) * 14., 608., 92.);
        card(x, y, w, h, Color::new(0.045, 0.06, 0.095, 0.95));
        rounded(x + 22., y, 56., 3., 1.5, GOLD);
        let planet = self.planet();
        let right = x + w - 22.;
        let cy = y + 30.;
        let deadline = planet.deadline.to_string();
        let dw = icon_value_width(&deadline, 16., PAPER);
        icon_value(Icon::Clock, &deadline, right - dw, cy, 16., PAPER);
        let wind_x = right - dw - 150.;
        self.wind_gauge(wind_x, cy, now);
        let gravity = format!("{:.0}", planet.gravity);
        let soft = Color::new(0.78, 0.83, 0.88, 1.);
        let gw = icon_value_width(&gravity, 16., soft);
        icon_value(Icon::Planet, &gravity, wind_x - 20. - gw, cy, 16., soft);
        fit(
            planet.name,
            x + 22.,
            y + 39.,
            wind_x - 40. - gw - x - 22.,
            26.,
            PAPER,
        );
        // The parcel's journey: what it is and who is waiting for it.
        let cy = y + 68.;
        let mut cx = x + 22.;
        icon(Icon::Crate, cx + 9., cy, 18., GOLD);
        cx += 26.;
        cx += fit(planet.parcel, cx, cy + 6., 260., 16., GOLD) + 14.;
        icon(Icon::ArrowRight, cx + 7., cy, 14., MUTED);
        cx += 24.;
        icon(Icon::Flag, cx + 9., cy, 18., GREEN);
        cx += 26.;
        fit(planet.recipient, cx, cy + 6., right - cx, 16., GREEN);
        button("Space", Icon::Play, "Launch", GOLD, 0.);
    }

    /// The refit desk: the stamps just earned, the credits to spend and three
    /// clickable refits with previews of what the next level does.
    fn draw_refits(&self, age: f32, now: f32) {
        let enter = ease((age - DEBRIEF_DELAY) / 0.3);
        if enter <= 0. {
            return;
        }
        let dy = (1. - enter) * 24.;
        scrim(enter);
        card(
            100.,
            150. + dy,
            572.,
            392.,
            Color::new(0.05, 0.065, 0.1, 0.98),
        );
        rounded(150., 150. + dy, 64., 3., 1.5, GREEN);
        text("Delivered", X0, 196. + dy, 28., GREEN);
        let earned = self.stamps[self.index];
        for k in 0..3usize {
            let at = vec2(X1 - 18. - (2 - k) as f32 * 42., 184. + dy);
            if k < usize::from(earned) {
                let t = ((age - DEBRIEF_DELAY - 0.25 - k as f32 * 0.16) / 0.24).clamp(0., 1.);
                if t > 0. {
                    let pop = ease(t) + 0.3 * (t * std::f32::consts::PI).sin();
                    if t < 1. {
                        glow(at.x, at.y, 30. * t, with_alpha(GREEN, 0.4 * (1. - t)));
                    }
                    stamp(at, 30. * pop, GREEN, true);
                }
            } else {
                stamp(at, 30., GREEN, false);
            }
        }
        // What earned those stamps, and the credits on hand.
        let y = 238. + dy;
        let mut x = X0;
        x += icon_value(Icon::Crate, &format!("{:.0}%", self.cargo), x, y, 16., GOLD) + 22.;
        icon_value(
            Icon::Shield,
            &format!("{:.0}%", self.hull),
            x,
            y,
            16.,
            GREEN,
        );
        let credits = self.credits.to_string();
        let cw = icon_value_width(&credits, 20., GOLD);
        icon_value(Icon::Coin, &credits, X1 - cw, y, 20., GOLD);
        let mut spend = None;
        for i in 0..3 {
            if let Some(cost) = self.refit_row(i, dy, now) {
                spend = Some(cost);
            }
        }
        if let Some(cost) = spend {
            // Hovering an affordable refit previews the balance it leaves.
            let label = format!("-{cost}");
            let lw = text_width(&label, ink(15., RED));
            say(&label, X1 - cw - 12. - lw, y + 5., 15., RED);
        }
        button("Space", Icon::Play, "Next", GREEN, dy);
    }

    /// One refit row; returns its price when it is hovered and affordable.
    fn refit_row(&self, i: usize, dy: f32, now: f32) -> Option<u32> {
        let top = row_top(i);
        let hover = hovered(Rect::new(X0, top, X1 - X0, 50.));
        let level = self.upgrades[i];
        let cost = self.upgrade_cost(i);
        let maxed = level >= 2;
        let afford = !maxed && self.credits >= cost;
        let y = top + dy - if hover && !maxed { 2. } else { 0. };
        let fill = if hover {
            Color::new(0.14, 0.18, 0.25, 1.)
        } else {
            Color::new(0.1, 0.13, 0.18, 1.)
        };
        rounded_gradient(
            X0,
            y,
            X1 - X0,
            50.,
            10.,
            fill,
            Color::new(0.07, 0.09, 0.13, 1.),
        );
        let rim = match (hover, afford, maxed) {
            (false, ..) => HAIRLINE,
            (true, true, _) => with_alpha(GOLD, 0.85),
            (true, _, true) => with_alpha(MUTED, 0.5),
            _ => with_alpha(RED, 0.8),
        };
        outline(X0, y, X1 - X0, 50., 10., if hover { 1.5 } else { 1. }, rim);
        keycap(&(i + 1).to_string(), "", X0 + 14., y + 15., PAPER);
        let (glyph, name) = REFITS[i];
        icon(glyph, X0 + 60., y + 25., 24., lighten(BLUE, 0.1));
        say(name, X0 + 80., y + 31., 17., PAPER);
        let chip_c = if maxed {
            with_alpha(MUTED, 0.5)
        } else {
            lighten(BLUE, 0.25)
        };
        let mut cx = X0 + 158.;
        for (chip_icon, label) in upgrade_effects(i) {
            cx += icon_chip(*chip_icon, label, cx, y + 16., 12., chip_c) + 6.;
        }
        for pip in 0..2u8 {
            let at = pip_at(i, pip) + vec2(0., y - top);
            let c = if pip < level {
                BLUE
            } else if pip == level && hover && afford {
                with_alpha(BLUE, 0.3 + 0.6 * pulse(now, 2.))
            } else {
                Color::new(1., 1., 1., 0.1)
            };
            rounded(at.x - 7., at.y - 3., 14., 6., 3., c);
        }
        if maxed {
            let w = icon_chip_width("Max", 12.);
            icon_chip(Icon::Check, "Max", X1 - 14. - w, y + 16., 12., MUTED);
        } else {
            let label = cost.to_string();
            let w = icon_chip_width(&label, 12.);
            let c = if afford { GOLD } else { RED };
            icon_chip(Icon::Coin, &label, X1 - 14. - w, y + 16., 12., c);
        }
        (hover && afford).then_some(cost)
    }

    /// Recovery: what went wrong, what a retry rolls back and the permit it
    /// spends.
    fn draw_recovery(&self, age: f32, now: f32) {
        let enter = ease((age - WRECK_DELAY) / 0.3);
        if enter <= 0. {
            return;
        }
        let dy = (1. - enter) * 24.;
        scrim(enter);
        card(
            100.,
            150. + dy,
            572.,
            392.,
            Color::new(0.05, 0.065, 0.1, 0.98),
        );
        rounded(150., 150. + dy, 64., 3., 1.5, RED);
        let cause = self.cause.unwrap_or(Cause::Recall);
        let (glyph, tint) = match cause {
            Cause::Hull => (Icon::Shield, RED),
            Cause::Parcel => (Icon::Crate, GOLD),
            Cause::Storm => (Icon::Cloud, PURPLE),
            Cause::Recall => (Icon::Restart, MUTED),
        };
        let at = vec2(X0 + 30., 214. + dy);
        glow(at.x, at.y, 44., with_alpha(tint, 0.22));
        icon(glyph, at.x, at.y, 50., tint);
        icon_badge(Icon::Cross, at.x + 20., at.y + 19., 22., RED, PAPER);
        text(cause.label(), X0 + 78., 210. + dy, 26., PAPER);
        // A retry rolls back whatever this attempt earned.
        let lost_score = self.score.saturating_sub(self.checkpoint.0);
        let lost_credits = self.credits.saturating_sub(self.checkpoint.1);
        let mut x = X0 + 78.;
        if lost_score > 0 {
            x += icon_chip(
                Icon::Star,
                &format!("-{lost_score}"),
                x,
                226. + dy,
                13.,
                RED,
            ) + 8.;
        }
        if lost_credits > 0 {
            icon_chip(
                Icon::Coin,
                &format!("-{lost_credits}"),
                x,
                226. + dy,
                13.,
                RED,
            );
        }
        // Recovery permits; the one a retry spends pulses.
        for k in 0..3u8 {
            let at = vec2(386. + (f32::from(k) - 1.) * 78., 336. + dy);
            if k < self.permits {
                let spend = k + 1 == self.permits;
                let beat = if spend { pulse(now, 1.4) } else { 0. };
                if spend {
                    glow(
                        at.x,
                        at.y,
                        40. + 6. * beat,
                        with_alpha(RED, 0.16 + 0.12 * beat),
                    );
                }
                shapes::fill_circle(at, 26., darken(GOLD, 0.6));
                shapes::stroke_circle(at, 26., 2., if spend { mix(GOLD, RED, beat) } else { GOLD });
                icon(Icon::Restart, at.x, at.y, 28., GOLD);
                if spend {
                    let tag = vec2(at.x + 22., at.y - 22.);
                    rounded(tag.x - 14., tag.y - 10., 28., 20., 10., RED);
                    text_centered("-1", tag.x, tag.y + 5., 14., PAPER);
                }
            } else {
                shapes::stroke_circle(at, 25., 1.5, Color::new(1., 1., 1., 0.14));
                icon(Icon::Restart, at.x, at.y, 26., Color::new(1., 1., 1., 0.12));
            }
        }
        button("Space", Icon::Restart, "Retry", GOLD, dy);
    }
}
