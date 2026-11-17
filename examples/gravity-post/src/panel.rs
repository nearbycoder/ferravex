//! Instruments, the header objective and the ending's result stats: icons,
//! meters and a landing scope instead of labelled readouts.
use crate::view::{BLUE, GOLD, GREEN, HAIRLINE, RED, icon_value_width, lander, mix, say};
use crate::{Game, Phase};
use ferravex_arcade::shapes;
use ferravex_arcade::*;

/// Instrument column content edges.
const X0: f32 = 706.;
const X1: f32 = 926.;
const W: f32 = X1 - X0;
/// Where the fuel gauge, storm clock and permit tokens sit (effects aim here).
pub(crate) const FUEL_AT: Vec2 = vec2(780., 138.);
pub(crate) const CLOCK_AT: Vec2 = vec2(732., 422.);
pub(crate) const PERMITS_AT: Vec2 = vec2(890., 498.);
const SCOPE_TOP: f32 = 220.;
const SCOPE_H: f32 = 150.;
/// Refit tracks in order: engine, tank, landing gear.
pub(crate) const REFITS: [(Icon, &str); 3] = [
    (Icon::Flame, "Engine"),
    (Icon::Fuel, "Tank"),
    (Icon::Gear, "Gear"),
];

/// A postage stamp: scalloped paper edge around a coloured face. Unearned
/// stamps are a faint outline.
pub(crate) fn stamp(at: Vec2, size: f32, color: Color, earned: bool) {
    let half = size * 0.5;
    if !earned {
        outline(
            at.x - half,
            at.y - half,
            size,
            size,
            size * 0.12,
            1.2,
            Color::new(1., 1., 1., 0.2),
        );
        return;
    }
    let paper = Color::new(0.95, 0.93, 0.86, 1.);
    let bite = size * 0.075;
    let steps = 5;
    for i in 0..=steps {
        let t = -half + size * i as f32 / steps as f32;
        for p in [
            vec2(at.x + t, at.y - half),
            vec2(at.x + t, at.y + half),
            vec2(at.x - half, at.y + t),
            vec2(at.x + half, at.y + t),
        ] {
            shapes::fill_circle(p, bite, paper);
        }
    }
    rounded(at.x - half, at.y - half, size, size, 1., paper);
    let inset = size * 0.16;
    let face = size - inset * 2.;
    rounded_gradient(
        at.x - half + inset,
        at.y - half + inset,
        face,
        face,
        size * 0.06,
        lighten(color, 0.15),
        darken(color, 0.25),
    );
    icon(
        Icon::Check,
        at.x,
        at.y,
        face * 0.72,
        Color::new(1., 1., 1., 0.92),
    );
}

/// Crack marks over a crate icon, one per stamp threshold lost.
fn cracks(at: Vec2, size: f32, integrity: f32) {
    let count = [90., 60., 30.].iter().filter(|&&t| integrity < t).count();
    let k = size / 18.;
    const PATHS: [[(f32, f32); 4]; 3] = [
        [(-6., -7.), (-2., -3.), (-4., 0.), (0., 3.)],
        [(6., -6.), (2., -2.), (4., 1.), (1., 6.)],
        [(-7., 4.), (-3., 3.), (-1., 6.), (2., 8.)],
    ];
    for path in PATHS.iter().take(count) {
        let points = path.map(|(x, y)| at + vec2(x, y) * k);
        shapes::polyline(
            &points,
            shapes::Stroke::new(1.4 * k, Color::new(0.16, 0.07, 0.03, 0.95)),
        );
    }
}

impl Game {
    pub(crate) fn draw_instruments(&self) {
        let now = clock();
        surface(688., 112., 256., 456.);
        self.draw_tanks(now);
        self.draw_scope(now);
        self.draw_conditions(now, 400.);
        self.draw_career(474.);
    }

    /// Fuel across the top, hull and parcel integrity beneath it.
    fn draw_tanks(&self, now: f32) {
        let capacity = self.capacity();
        let fraction = (self.fuel / capacity).clamp(0., 1.);
        let low = fraction < 0.2;
        let alarm = if low { pulse(now, 2.) } else { 0. };
        let fuel_c = if low { mix(GOLD, RED, alarm) } else { BLUE };
        let w = icon_value(
            Icon::Fuel,
            &format!("{:.0}", self.fuel),
            X0,
            136.,
            22.,
            fuel_c,
        );
        say(
            &format!("/{capacity:.0}"),
            X0 + w + 5.,
            143.,
            13.,
            with_alpha(MUTED, 0.8),
        );
        meter(X0, 154., W, 10., fraction, fuel_c);
        if low {
            outline(
                X0 - 2.,
                152.,
                W + 4.,
                14.,
                6.,
                1.5,
                with_alpha(RED, 0.25 + 0.6 * alarm),
            );
        }
        let half = (W - 16.) * 0.5;
        let hull_c = if self.hull < 35. { RED } else { GREEN };
        icon_value(
            Icon::Shield,
            &format!("{:.0}", self.hull),
            X0,
            188.,
            17.,
            hull_c,
        );
        meter(X0, 202., half, 6., self.hull / 100., hull_c);
        let px = X0 + half + 16.;
        if self.loaded {
            let c = if self.cargo < 60. {
                mix(GOLD, RED, 0.5)
            } else {
                GOLD
            };
            icon_value(Icon::Crate, &format!("{:.0}", self.cargo), px, 188., 17., c);
            cracks(vec2(px + 8.5, 188.), 17., self.cargo);
            meter(px, 202., half, 6., self.cargo / 100., c);
        } else {
            // An empty hold: a ghosted crate over an empty track.
            icon(Icon::Crate, px + 8.5, 188., 17., with_alpha(MUTED, 0.35));
            meter(px, 202., half, 6., 0., GOLD);
        }
    }

    /// The landing scope: the ship marker sits at its sideways and downward
    /// speed and turns with its attitude. Inside the green box and within the
    /// green tilt fan, a touchdown is safe.
    fn draw_scope(&self, now: f32) {
        let top = SCOPE_TOP;
        rounded(X0, top, W, SCOPE_H, 8., Color::new(0.02, 0.03, 0.05, 0.6));
        outline(X0, top, W, SCOPE_H, 8., 1., HAIRLINE);
        let (lx, ly, lt) = self.limits();
        let inner = Rect::new(X0 + 12., top + 10., W - 24., SCOPE_H - 20.);
        let (up, down) = (-ly * 1.25, ly * 3.);
        let map = |v: Vec2| {
            vec2(
                inner.x + (v.x / (lx * 3.) * 0.5 + 0.5) * inner.w,
                inner.y + (v.y - up) / (down - up) * inner.h,
            )
        };
        let zero = map(Vec2::ZERO);
        panel(inner.x, zero.y, inner.w, 1., Color::new(1., 1., 1., 0.08));
        panel(zero.x, inner.y, 1., inner.h, Color::new(1., 1., 1., 0.08));
        // Faint axis arrows: sideways speed across, climb and descent down.
        let axis = Color::new(1., 1., 1., 0.22);
        icon(Icon::ArrowLeft, X0 + 7., zero.y, 9., axis);
        icon(Icon::ArrowRight, X1 - 7., zero.y, 9., axis);
        icon(Icon::ArrowUp, zero.x, top + 6., 9., axis);
        icon(Icon::ArrowDown, zero.x, top + SCOPE_H - 6., 9., axis);
        // Faint speed rings every limit-width for scale.
        for k in [2., 3.] {
            let a = map(vec2(-lx * k, 0.)).x;
            let b = map(vec2(lx * k, 0.)).x;
            for x in [a, b] {
                if x > inner.x + 1. && x < inner.x + inner.w - 1. {
                    panel(x, zero.y - 3., 1., 6., Color::new(1., 1., 1., 0.12));
                }
            }
        }
        let flying = self.phase == Phase::Flight && self.docked.is_none();
        let (sx, sy, st) = self.within_limits();
        let ready = flying && sx && sy && st;
        let near = if flying {
            (1. - self.altitude() / 120.).clamp(0., 1.)
        } else {
            0.
        };
        let a = map(vec2(-lx, 0.));
        let b = map(vec2(lx, ly));
        let fill = if ready {
            0.16 + 0.22 * near * pulse(now, 1.6)
        } else {
            0.08
        };
        rounded(a.x, a.y, b.x - a.x, b.y - a.y, 3., with_alpha(GREEN, fill));
        outline(
            a.x,
            a.y,
            b.x - a.x,
            b.y - a.y,
            3.,
            1.2,
            with_alpha(GREEN, 0.75),
        );
        if ready && near > 0.2 {
            glow(
                (a.x + b.x) * 0.5,
                (a.y + b.y) * 0.5,
                34.,
                with_alpha(GREEN, 0.14 * near),
            );
        }
        // The marker, clamped inside the scope with an overflow chevron.
        let m = map(self.velocity);
        let pad = 8.;
        let at = vec2(
            m.x.clamp(inner.x + pad, inner.x + inner.w - pad),
            m.y.clamp(inner.y + pad, inner.y + inner.h - pad),
        );
        let c = if !flying {
            MUTED
        } else if ready {
            GREEN
        } else {
            GOLD
        };
        for side in [-1., 1.] {
            let edge = side * lt;
            let tip = at + vec2(edge.sin(), -edge.cos()) * 17.;
            draw_line(at.x, at.y, tip.x, tip.y, 1., with_alpha(GREEN, 0.45));
        }
        lander(at, 16., self.angle, c);
        let out = m - at;
        if out.length() > 0.5 {
            let dir = out.normalize_or_zero();
            let side = vec2(-dir.y, dir.x);
            let tip = at + dir * 13.;
            shapes::fill_triangle(
                tip + dir * 4.,
                tip + side * 4.,
                tip - side * 4.,
                with_alpha(c, 0.9),
            );
        }
        // Exact readouts in the matching colours.
        let y = top + SCOPE_H + 14.;
        let tone = |ok: bool| {
            if !flying {
                MUTED
            } else if ok {
                GREEN
            } else {
                GOLD
            }
        };
        let vx = self.velocity.x;
        let vy = self.velocity.y;
        icon_value(
            if vx < 0. {
                Icon::ArrowLeft
            } else {
                Icon::ArrowRight
            },
            &format!("{:.1}", vx.abs()),
            X0,
            y,
            14.,
            tone(sx),
        );
        icon_value(
            if vy < 0. {
                Icon::ArrowUp
            } else {
                Icon::ArrowDown
            },
            &format!("{:.1}", vy.abs()),
            X0 + 78.,
            y,
            14.,
            tone(sy),
        );
        let tilt = format!("{:.0}", self.angle.to_degrees().abs());
        let tw = icon_value_width(&tilt, 14., tone(st));
        lander(vec2(X1 - tw + 6., y), 14., self.angle, tone(st));
        text_right(&tilt, X1, y + 5., 14., tone(st));
    }

    /// The storm clock, then gravity and live wind.
    fn draw_conditions(&self, now: f32, top: f32) {
        divider(X0, top, W, HAIRLINE);
        let left = self.seconds_left();
        let urgent = left < 30 && matches!(self.phase, Phase::Flight | Phase::Wreck);
        let c = if urgent {
            mix(RED, PAPER, 0.35 * pulse(now, 1.5))
        } else {
            PAPER
        };
        let w = icon_value(Icon::Clock, &left.to_string(), X0, top + 22., 18., c);
        meter(
            X0 + w + 12.,
            top + 19.,
            W - w - 12.,
            6.,
            left as f32 / self.planet().deadline as f32,
            if urgent { RED } else { MUTED },
        );
        let y = top + 52.;
        let ink = Color::new(0.78, 0.83, 0.88, 1.);
        icon_value(
            Icon::Planet,
            &format!("{:.0}", self.planet().gravity),
            X0,
            y,
            16.,
            ink,
        );
        self.wind_gauge(X0 + 78., y, now);
    }

    /// Wind: icon, a direction arrow whose length follows the live wind,
    /// and five strength pips.
    pub(crate) fn wind_gauge(&self, x: f32, cy: f32, now: f32) {
        let wind = self.wind();
        let strength = wind.abs();
        let calm = strength < 0.05;
        let c = if calm { with_alpha(MUTED, 0.6) } else { BLUE };
        icon(Icon::Wind, x + 8., cy, 16., c);
        let mid = x + 46.;
        if calm {
            shapes::fill_circle(vec2(mid, cy), 2.5, c);
        } else {
            let dir = wind.signum();
            let len = 8. + 16. * (strength / 7.).min(1.);
            let shimmer = bob(now, 1.2, 1.);
            let (tail, tip) = (
                mid - dir * len * 0.5 + dir * shimmer,
                mid + dir * len * 0.5 + dir * shimmer,
            );
            draw_line(tail, cy, tip, cy, 2.2, c);
            shapes::fill_triangle(
                vec2(tip + dir * 5., cy),
                vec2(tip - dir * 2., cy - 5.),
                vec2(tip - dir * 2., cy + 5.),
                c,
            );
        }
        let lit = (strength / 1.5).ceil().min(5.) as usize;
        for k in 0..5 {
            let at = vec2(x + 76. + k as f32 * 11., cy);
            if k < lit {
                shapes::fill_circle(at, 3.5, c);
            } else {
                shapes::stroke_circle(at, 3., 1.2, Color::new(1., 1., 1., 0.18));
            }
        }
    }

    /// Credits, recovery permits and the installed refits.
    fn draw_career(&self, top: f32) {
        divider(X0, top, W, HAIRLINE);
        let y = top + 24.;
        icon_value(Icon::Coin, &self.credits.to_string(), X0, y, 17., GOLD);
        for k in 0..3u8 {
            let at = vec2(PERMITS_AT.x - 24. + f32::from(k) * 24., y);
            if k < self.permits {
                shapes::fill_circle(at, 10., darken(GOLD, 0.6));
                icon(Icon::Restart, at.x, at.y, 13., GOLD);
            } else {
                shapes::stroke_circle(at, 9.5, 1.2, Color::new(1., 1., 1., 0.15));
            }
        }
        let y = top + 56.;
        for (i, (glyph, _)) in REFITS.iter().enumerate() {
            let x = X0 + i as f32 * 78.;
            let level = self.upgrades[i];
            let c = if level > 0 { BLUE } else { MUTED };
            icon(*glyph, x + 8., y, 16., c);
            for pip in 0..2u8 {
                let at = vec2(x + 26. + f32::from(pip) * 12., y);
                if pip < level {
                    shapes::fill_circle(at, 4., BLUE);
                } else {
                    shapes::stroke_circle(at, 3.5, 1.2, Color::new(1., 1., 1., 0.18));
                }
            }
        }
    }

    /// Header: load the parcel, then deliver it to its recipient; the
    /// optional survey beacon sits at the far right.
    pub(crate) fn objective(&self, area: Rect) -> bool {
        let now = clock();
        let cy = area.y + area.h * 0.5;
        let finished = matches!(self.phase, Phase::Debrief | Phase::Certified);
        let first = if self.loaded || finished {
            Step::Done
        } else {
            Step::Active
        };
        let second = if finished {
            Step::Done
        } else if self.loaded {
            Step::Active
        } else {
            Step::Waiting
        };
        let mut x = area.x + 2.;
        x = step(x, cy, Icon::Crate, GOLD, self.planet().parcel, first, now);
        icon(Icon::ArrowRight, x + 16., cy, 14., with_alpha(MUTED, 0.7));
        x += 34.;
        x = step(
            x,
            cy,
            Icon::Flag,
            GREEN,
            self.planet().recipient,
            second,
            now,
        );
        // The optional survey beacon, kept clear of the host's score popups.
        let x = (x + 26.).min(area.x + area.w - 230.);
        let size = 14.;
        if self.beacon[self.index] {
            icon_chip(Icon::Gem, "", x, cy - size * 0.725, size, GREEN);
            icon_badge(Icon::Check, x + size * 1.45 - 2., cy + 6., 11., GREEN, INK);
        } else {
            icon_chip(
                Icon::Gem,
                "+250",
                x,
                cy - size * 0.725,
                size,
                with_alpha(GOLD, 0.85),
            );
        }
        true
    }

    /// Ending: deliveries, stamps, survey beacons and unused permits.
    pub(crate) fn results(&self, area: Rect) -> bool {
        let column = area.w / 4.;
        let stats = [
            (0, self.delivered() as u32, 8, GOLD),
            (1, self.stamps(), 24, GREEN),
            (2, self.beacons() as u32, 8, lighten(GOLD, 0.2)),
            (3, u32::from(self.permits), 3, BLUE),
        ];
        for (i, value, total, c) in stats {
            let cx = area.x + column * (i as f32 + 0.5);
            let cy = area.y + 34.;
            match i {
                0 => icon(Icon::Crate, cx, cy, 38., c),
                1 => stamp(vec2(cx, cy), 32., c, true),
                2 => icon(Icon::Gem, cx, cy, 38., c),
                _ => icon(Icon::Restart, cx, cy, 36., c),
            }
            let number = value.to_string();
            let of = format!("/{total}");
            let big = style(30., PAPER);
            let small = style(16., MUTED);
            let w = typography::text_width(&number, big) + typography::text_width(&of, small);
            let x = cx - w * 0.5;
            let nw = typography::draw_text_styled(&number, x, area.y + 98., big).width;
            typography::draw_text_styled(&of, x + nw + 2., area.y + 98., small);
            meter(
                cx - 46.,
                area.y + 114.,
                92.,
                6.,
                value as f32 / total as f32,
                c,
            );
        }
        true
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Done,
    Active,
    Waiting,
}

/// Width [`icon_chip`] draws for `label` at `size`.
pub(crate) fn icon_chip_width(label: &str, size: f32) -> f32 {
    let text = typography::text_width(label, style(size, PAPER)) + size * 0.35;
    (size * 0.6 * 2. + size * 0.95 + text).max(size * 1.45)
}

/// One objective step: a role badge and the name it stands for.
fn step(x: f32, cy: f32, glyph: Icon, color: Color, name: &str, state: Step, now: f32) -> f32 {
    let r = 13.;
    let at = vec2(x + r, cy);
    match state {
        Step::Active => {
            glow(
                at.x,
                at.y,
                20. + 4. * pulse(now, 1.2),
                with_alpha(color, 0.32),
            );
            shapes::fill_circle(at, r, darken(color, 0.62));
            shapes::stroke_circle(at, r, 1.6, color);
            icon(glyph, at.x, at.y, 15., lighten(color, 0.25));
        }
        Step::Done => {
            shapes::fill_circle(at, r, darken(color, 0.78));
            icon(glyph, at.x, at.y, 15., with_alpha(color, 0.5));
            icon_badge(Icon::Check, at.x + 9., at.y + 8., 12., GREEN, INK);
        }
        Step::Waiting => {
            shapes::stroke_circle(at, r - 0.5, 1.2, with_alpha(color, 0.4));
            icon(glyph, at.x, at.y, 15., with_alpha(color, 0.45));
        }
    }
    let ink = match state {
        Step::Active => PAPER,
        Step::Done => with_alpha(MUTED, 0.75),
        Step::Waiting => MUTED,
    };
    let w = fit(name, x + r * 2. + 10., cy + 6., 330., 17., ink);
    x + r * 2. + 10. + w
}
