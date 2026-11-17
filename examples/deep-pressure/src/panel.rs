//! Instruments drawn as dials, seats, icons and gauges: the right-hand panel,
//! the strip above the water, the header's rescue row and the results card.
use crate::{
    Game,
    art::*,
    content::{MISSIONS, TOTAL_DIVERS},
    model::{
        BALLAST_BUTTON, GOLD_TICKS, LOW_AIR, Phase, RescueState, SONAR_BUTTON, Trail, WINCH_SPEED,
        button_hit,
    },
};
use ferravex_arcade::shapes::{self, Cap, Fill, Stroke};
use ferravex_arcade::*;
use std::f32::consts::{FRAC_PI_2, TAU};

pub(crate) const AIR_DIAL: Vec2 = vec2(768., 170.);
pub(crate) const HULL_DIAL: Vec2 = vec2(876., 170.);
const DIAL_R: f32 = 34.;
/// The cabin cutaway: one seat per berth.
pub(crate) const CABIN: Rect = Rect::new(716., 226., 212., 44.);
const MANIFEST_Y: f32 = 294.;
const SPEED_Y: f32 = 466.;
const PURSE_Y: f32 = 526.;
/// Top speed of an empty boat, the speed gauge's full scale.
const TOP_SPEED: f32 = 96.;

/// Centre of seat `index` of `capacity` in the cabin cutaway.
pub(crate) fn seat_center(index: usize, capacity: usize) -> Vec2 {
    let inner = CABIN.w - 70.;
    let step = inner / capacity.max(1) as f32;
    vec2(
        CABIN.x + 30. + step * (index as f32 + 0.5),
        CABIN.y + CABIN.h * 0.5,
    )
}
/// The manifest entry under canvas point `p` among `count` divers.
pub(crate) fn manifest_at(p: Vec2, count: usize) -> Option<usize> {
    (0..count).find(|&i| {
        let x = 718. + (i % 2) as f32 * 106.;
        let cy = MANIFEST_Y + (i / 2) as f32 * 26.;
        p.x >= x - 4. && p.x <= x + 102. && (p.y - cy).abs() <= 12.
    })
}
/// Centre of an instrument button.
pub(crate) fn button_center((x, y, w, h): (f32, f32, f32, f32)) -> Vec2 {
    vec2(x + w * 0.5, y + h * 0.5)
}
/// Centre of recovery tug `index` in the panel's purse row.
pub(crate) fn tug_center(index: usize) -> Vec2 {
    vec2(880. + index as f32 * 22., PURSE_Y)
}

impl Game {
    pub(crate) fn draw_instruments(&self, home: Option<&Trail>) {
        surface(700., 112., 244., 456.);
        let t = clock();
        self.draw_dials(home, t);
        self.draw_cabin(t);
        self.draw_manifest(t);
        divider(718., 370., 208., HAIRLINE);
        self.draw_buttons(t);
        self.draw_speed();
        divider(718., 494., 208., HAIRLINE);
        self.draw_purse();
    }

    fn draw_dials(&self, home: Option<&Trail>, t: f32) {
        let diving = self.phase == Phase::Diving;
        // Air: the ring drains; the white mark is what the trip home needs.
        let reserve = home.map(|trail| self.air_reserve(trail.distance));
        let short = diving && reserve.is_some_and(|r| self.oxygen <= r);
        let low = diving && self.oxygen < LOW_AIR;
        let beat = pulse(t, if low { 2.2 } else { 1.2 });
        let air = if low || short {
            mix(CORAL, lighten(CORAL, 0.35), beat)
        } else {
            AQUA
        };
        if low || short {
            glow(
                AIR_DIAL.x,
                AIR_DIAL.y,
                DIAL_R + 16.,
                with_alpha(CORAL, 0.18 + 0.2 * beat),
            );
        }
        dial_face(AIR_DIAL);
        ring_meter(
            AIR_DIAL.x,
            AIR_DIAL.y,
            DIAL_R,
            7.,
            self.oxygen / self.max_air(),
            air,
        );
        if let Some(reserve) = reserve.filter(|_| diving) {
            let a = -FRAC_PI_2 + (reserve / self.max_air()).clamp(0., 1.) * TAU;
            let dir = vec2(a.cos(), a.sin());
            shapes::polyline(
                &[
                    AIR_DIAL + dir * (DIAL_R - 7.),
                    AIR_DIAL + dir * (DIAL_R + 7.),
                ],
                Stroke::new(2.5, with_alpha(PAPER, 0.92)).cap(Cap::Round),
            );
        }
        air_glyph(AIR_DIAL.x, AIR_DIAL.y - 9., 19., air);
        say_mid_centered(
            &format!("{:.0}", self.oxygen.max(0.).ceil()),
            AIR_DIAL.x,
            AIR_DIAL.y + 12.,
            18.,
            if low || short { air } else { PAPER },
        );

        // Hull: steel shield, violet under crushing pressure, coral when weak.
        let fraction = self.hull / self.max_hull();
        let crushing =
            diving && self.mission >= 4 && self.position.y > 315. && self.upgrades[1] == 0;
        let weak = fraction < 0.35;
        let hull = if weak {
            mix(CORAL, lighten(CORAL, 0.35), pulse(t, 1.8))
        } else if crushing {
            mix(VIOLET, lighten(VIOLET, 0.3), pulse(t, 1.5))
        } else {
            STEEL
        };
        if weak || crushing {
            glow(
                HULL_DIAL.x,
                HULL_DIAL.y,
                DIAL_R + 14.,
                with_alpha(hull, 0.16 + 0.14 * pulse(t, 1.5)),
            );
        }
        dial_face(HULL_DIAL);
        ring_meter(HULL_DIAL.x, HULL_DIAL.y, DIAL_R, 7., fraction, hull);
        icon(Icon::Shield, HULL_DIAL.x, HULL_DIAL.y - 9., 18., hull);
        say_mid_centered(
            &format!("{:.0}", self.hull.max(0.).ceil()),
            HULL_DIAL.x,
            HULL_DIAL.y + 12.,
            18.,
            if weak { hull } else { PAPER },
        );
    }

    fn draw_cabin(&self, t: f32) {
        let berths = self.capacity();
        let aboard = self.onboard();
        let diving = self.phase == Phase::Diving;
        let full = diving && aboard >= berths;
        let r = CABIN;
        let cy = r.y + r.h * 0.5;
        // The boat in cutaway: tail fin, hull and a nose lamp.
        let fin = darken(GOLD, 0.45);
        shapes::fill_polygon(
            &[
                vec2(r.x + 8., cy - 4.),
                vec2(r.x - 2., cy - 15.),
                vec2(r.x - 2., cy + 15.),
                vec2(r.x + 8., cy + 4.),
            ],
            fin,
        );
        shapes::fill_rounded_rect(
            r,
            r.h * 0.5,
            Fill::vertical(
                r,
                Color::new(0.14, 0.17, 0.2, 1.),
                Color::new(0.07, 0.09, 0.12, 1.),
            ),
        );
        let rim = if full {
            with_alpha(GOLD, 0.55 + 0.4 * pulse(t, 1.4))
        } else {
            with_alpha(GOLD, 0.45)
        };
        shapes::stroke_rounded_rect(r, r.h * 0.5, 1.5, rim);
        shapes::fill_circle(vec2(r.x + r.w - 12., cy), 3., lighten(GOLD, 0.4));
        for i in 0..berths {
            let c = seat_center(i, berths);
            let seat = Rect::new(c.x - 15., c.y - 16., 30., 32.);
            shapes::fill_rounded_rect(seat, 8., Color::new(0.03, 0.05, 0.07, 0.8));
            if i < aboard {
                glow(c.x, c.y, 20., with_alpha(AQUA, 0.22));
                shapes::stroke_rounded_rect(seat, 8., 1., with_alpha(AQUA, 0.55));
                icon(Icon::Person, c.x, c.y, 22., AQUA);
            } else {
                shapes::stroke_rounded_rect(seat, 8., 1., Color::new(1., 1., 1., 0.08));
                icon(Icon::Person, c.x, c.y, 20., Color::new(1., 1., 1., 0.13));
            }
        }
        if full {
            // A full cabin points home.
            let x = r.x + r.w - 26. + bob(t, 1.4, 1.5);
            icon(Icon::Home, x, cy, 15., GOLD);
        }
    }

    fn draw_manifest(&self, t: f32) {
        let at = pointer();
        let divers = self.chart().divers;
        let linked = manifest_at(at, divers.len()).or_else(|| {
            divers
                .iter()
                .position(|(point, _)| at.distance(crate::view::sea(*point)) < 22.)
        });
        for (index, (_, name)) in divers.iter().enumerate() {
            let x = 718. + (index % 2) as f32 * 106.;
            let cy = MANIFEST_Y + (index / 2) as f32 * 26.;
            if linked == Some(index) {
                let pill = Rect::new(x - 4., cy - 12., 104., 24.);
                shapes::fill_rounded_rect(pill, 12., with_alpha(AQUA, 0.12));
                shapes::stroke_rounded_rect(pill, 12., 1., with_alpha(AQUA, 0.4));
            }
            let (glyph, mark, label) = match self.divers[index] {
                RescueState::Waiting => {
                    let a = 0.45 + 0.35 * pulse(t + index as f32 * 0.31, 0.7);
                    (
                        Icon::Pin,
                        with_alpha(AQUA, a),
                        Color::new(0.8, 0.86, 0.9, 1.),
                    )
                }
                RescueState::Onboard => (Icon::Person, AQUA, AQUA),
                RescueState::Safe => (Icon::Check, GOLD, GOLD),
            };
            icon(glyph, x + 8., cy, 15., mark);
            say_mid(name, x + 21., cy, 14., label);
        }
    }

    fn draw_buttons(&self, t: f32) {
        let diving = self.phase == Phase::Diving;
        let at = pointer();
        // Ballast: rising (light) or holding depth (neutral).
        let hover = diving && button_hit(at, BALLAST_BUTTON);
        let tone = if self.balanced { AQUA } else { PAPER };
        let (x, y, _, _) = instrument_button(BALLAST_BUTTON, hover, self.balanced, tone);
        keycap("X", "", x + 8., y + 8., PAPER);
        let c = vec2(x + 64., y + 29.);
        if self.balanced {
            icon(Icon::Anchor, c.x, c.y, 26., AQUA);
        } else {
            let rise = if diving { -bob(t, 0.9, 2.5).abs() } else { 0. };
            icon(Icon::ArrowUp, c.x, c.y + rise, 24., PAPER);
        }

        // Sonar: the ring recharges; a tap costs two seconds of air.
        let ready = self.sonar_cooldown == 0 && self.oxygen > 3.;
        let hover = diving && ready && button_hit(at, SONAR_BUTTON);
        let tone = if ready { AQUA } else { MUTED };
        let (x, y, _, _) = instrument_button(SONAR_BUTTON, hover, false, tone);
        keycap("C", "", x + 8., y + 8., PAPER);
        let c = vec2(x + 64., y + 29.);
        if ready && diving {
            glow(c.x, c.y, 24., with_alpha(AQUA, 0.12 + 0.12 * pulse(t, 0.8)));
        }
        ring_meter(
            c.x,
            c.y,
            17.,
            4.,
            1. - self.sonar_cooldown as f32 / 240.,
            tone,
        );
        if self.sonar > 0 {
            // Sweep while the danger circles are showing.
            let a = t * 5.;
            shapes::fill_sector(c, 15., a, 0.9, with_alpha(AQUA, 0.35));
        }
        icon(Icon::Target, c.x, c.y, 15., tone);
        air_glyph(x + 14., y + 44., 10., with_alpha(CORAL, 0.85));
        say_mid("-2", x + 22., y + 45., 11., with_alpha(CORAL, 0.85));
    }

    fn draw_speed(&self) {
        let speed = self.velocity.length();
        let slow = speed <= WINCH_SPEED;
        let (x, w, cy) = (742., 184., SPEED_Y);
        icon(Icon::Bolt, 727., cy, 15., MUTED);
        rounded(x, cy - 4., w, 8., 4., Color::new(0.02, 0.03, 0.05, 0.6));
        // The winch zone: slow enough to rescue or dock.
        let zone = w * WINCH_SPEED / TOP_SPEED;
        rounded(x, cy - 4., zone, 8., 4., with_alpha(MINT, 0.16));
        let fill = (w * speed / TOP_SPEED).clamp(0., w);
        if fill > 1. {
            rounded(
                x,
                cy - 4.,
                fill.max(8.),
                8.,
                4.,
                if slow { MINT } else { AMBER },
            );
        }
        panel(x + zone - 0.75, cy - 8., 1.5, 16., with_alpha(MINT, 0.75));
    }

    fn draw_purse(&self) {
        icon_value(
            Icon::Coin,
            &self.credits.to_string(),
            718.,
            PURSE_Y,
            17.,
            GOLD,
        );
        for i in 0..3 {
            let c = tug_center(i);
            let available = i < usize::from(self.recovery);
            icon(
                Icon::Ship,
                c.x,
                c.y,
                17.,
                if available { GOLD } else { DIM },
            );
        }
    }

    /// The strip above the water: dive clock with the medal this pace earns
    /// on the left, the chart's depth, current and pressure on the right.
    pub(crate) fn draw_strip(&self) {
        let cy = 132.;
        let x = 32.
            + icon_value(
                Icon::Clock,
                &clock_label(self.elapsed / 60),
                32.,
                cy,
                15.,
                PAPER,
            );
        let pace = self.medal_pace();
        let c = vec2(x + 18., cy);
        if pace == 3 {
            // Gold is still possible: its time window drains around the medal.
            ring_meter(
                c.x,
                c.y,
                11.,
                2.5,
                1. - self.elapsed as f32 / GOLD_TICKS as f32,
                with_alpha(medal_metal(3), 0.9),
            );
        }
        medal(c, 7., pace, false);
        // Hits taken this sortie, each one costing medal pace.
        for i in 0..self.stage_impacts.min(5) {
            let x = c.x + 22. + i as f32 * 9.;
            shapes::fill_circle(vec2(x, cy), 3., with_alpha(CORAL, 0.85));
        }

        let mut right = 676.;
        let size = 13.;
        let chip_y = cy - size * 0.725;
        let depth = format!("{} m", self.depth());
        icon_chip_right(Icon::ArrowDown, &depth, right, chip_y, size, AQUA);
        right -= icon_chip_width(&depth, size) + 8.;
        let current = self.chart().current;
        if current != 0. {
            right -= current_chip(right, cy, current);
            right -= 8.;
        }
        if self.mission >= 4 {
            // Deep pressure: harmless once armor is fitted.
            let armored = self.upgrades[1] > 0;
            let tone = if armored {
                with_alpha(VIOLET, 0.55)
            } else {
                VIOLET
            };
            icon_chip_right(
                if armored { Icon::Shield } else { Icon::Alert },
                "",
                right,
                chip_y,
                size,
                tone,
            );
        }
    }

    /// Header objective: every survivor of the campaign as a person, grouped
    /// by sortie: gold once home, aqua aboard, outlined while waiting.
    pub(crate) fn draw_rescue_row(&self, area: Rect) {
        let cy = area.y + area.h * 0.5;
        let mut x = area.x + 4.;
        let count = format!("{} / {TOTAL_DIVERS}", self.rescued());
        x += icon_value(Icon::Home, &count, x, cy, 17., GOLD) + 22.;
        let (size, step, gap) = (15., 16., 14.);
        let t = clock();
        for (m, mission) in MISSIONS.iter().enumerate() {
            let n = mission.divers.len();
            let w = n as f32 * step;
            if m == self.mission {
                let pill = Rect::new(x - 6., cy - 13., w + 12. - (step - size), 26.);
                shapes::fill_rounded_rect(pill, 13., with_alpha(AQUA, 0.08));
                shapes::stroke_rounded_rect(pill, 13., 1., with_alpha(AQUA, 0.3));
            }
            for i in 0..n {
                let state = if m < self.mission {
                    Some(RescueState::Safe)
                } else if m == self.mission {
                    Some(self.divers[i])
                } else {
                    None
                };
                let color = match state {
                    Some(RescueState::Safe) => GOLD,
                    Some(RescueState::Onboard) => AQUA,
                    Some(RescueState::Waiting) => {
                        with_alpha(PAPER, 0.5 + 0.25 * pulse(t + i as f32 * 0.2, 0.6))
                    }
                    None => Color::new(1., 1., 1., 0.16),
                };
                icon(
                    Icon::Person,
                    x + i as f32 * step + size * 0.5,
                    cy,
                    size,
                    color,
                );
            }
            x += w + gap;
        }
    }

    /// Results card: people home, medals, time and tugs, as big icon stats.
    pub(crate) fn draw_results(&self, area: Rect) {
        let gap = 12.;
        let w = (area.w - gap * 3.) / 4.;
        let h = area.h.min(116.);
        let top = area.y + (area.h - h) * 0.5;
        for i in 0..4 {
            let x = area.x + i as f32 * (w + gap);
            rounded(x, top, w, h, 12., Color::new(1., 1., 1., 0.05));
            outline(x, top, w, h, 12., 1., HAIRLINE);
        }
        let mid = |i: usize| area.x + i as f32 * (w + gap) + w * 0.5;
        let icon_y = top + 36.;
        let value_y = top + 82.;
        // People home.
        icon(Icon::Person, mid(0), icon_y, 30., GOLD);
        say_mid_centered(
            &format!("{} / {TOTAL_DIVERS}", self.rescued()),
            mid(0),
            value_y,
            26.,
            GOLD,
        );
        // Sortie medals.
        for (i, &m) in self.medals.iter().enumerate() {
            let col = (i % 3) as f32 - 1.;
            let row = (i / 3) as f32;
            let c = vec2(mid(1) + col * 30., icon_y + 4. + row * 34.);
            if m > 0 {
                medal(c, 12., m, false);
            } else {
                shapes::stroke_circle(c, 10., 1.5, Color::new(1., 1., 1., 0.18));
            }
        }
        // Time underwater.
        icon(Icon::Clock, mid(2), icon_y, 28., AQUA);
        say_mid_centered(
            &clock_label((self.total_ticks / 60) as u32),
            mid(2),
            value_y,
            26.,
            PAPER,
        );
        // Recovery tugs left.
        for i in 0..3 {
            let x = mid(3) + (i as f32 - 1.) * 30.;
            let available = i < usize::from(self.recovery);
            icon(
                Icon::Ship,
                x,
                icon_y,
                26.,
                if available { GOLD } else { DIM },
            );
        }
        say_mid_centered(
            &self.recovery.to_string(),
            mid(3),
            value_y,
            26.,
            if self.recovery > 0 { GOLD } else { MUTED },
        );
    }
}

/// Recessed glass face behind a dial.
fn dial_face(c: Vec2) {
    shapes::fill_circle(c, DIAL_R + 6., Color::new(0.01, 0.02, 0.035, 0.6));
    shapes::fill_circle(
        c,
        DIAL_R - 4.,
        Fill::radial(
            c - vec2(9., 11.),
            DIAL_R * 1.2,
            Color::new(0.11, 0.15, 0.19, 1.),
            Color::new(0.04, 0.06, 0.09, 1.),
        ),
    );
    for i in 0..12 {
        let a = i as f32 * TAU / 12.;
        let dir = vec2(a.cos(), a.sin());
        shapes::fill_circle(c + dir * (DIAL_R + 3.5), 0.9, Color::new(1., 1., 1., 0.12));
    }
}
/// A raised instrument button that lifts under the pointer; returns its
/// drawn rectangle.
fn instrument_button(
    (x, y, w, h): (f32, f32, f32, f32),
    hover: bool,
    engaged: bool,
    tone: Color,
) -> (f32, f32, f32, f32) {
    let lift = if hover { 2. } else { 0. };
    let y = y - lift;
    if hover {
        glow(x + w * 0.5, y + h * 0.5, w * 0.62, with_alpha(tone, 0.12));
    }
    rounded_gradient(
        x,
        y,
        w,
        h,
        10.,
        Color::new(0.13, 0.17, 0.21, 1.),
        Color::new(0.08, 0.11, 0.14, 1.),
    );
    let rim = if engaged {
        with_alpha(tone, 0.7)
    } else if hover {
        with_alpha(tone, 0.55)
    } else {
        HAIRLINE
    };
    outline(x, y, w, h, 10., if engaged { 1.5 } else { 1. }, rim);
    (x, y, w, h)
}
/// Current chip ending at `right`: water and one arrow per strength step.
/// Returns its width.
fn current_chip(right: f32, cy: f32, current: f32) -> f32 {
    let steps = if current.abs() <= 3. {
        1
    } else if current.abs() <= 8. {
        2
    } else {
        3
    };
    let h = 19.;
    let w = 30. + steps as f32 * 11.;
    let r = Rect::new(right - w, cy - h * 0.5, w, h);
    let tone = Color::new(0.72, 0.9, 1., 1.);
    shapes::fill_rounded_rect(r, h * 0.5, with_alpha(tone, 0.12));
    shapes::stroke_rounded_rect(r, h * 0.5, 1., with_alpha(tone, 0.32));
    icon(Icon::Wave, r.x + 13., cy, 13., tone);
    let arrow = if current > 0. {
        Icon::ArrowRight
    } else {
        Icon::ArrowLeft
    };
    for i in 0..steps {
        icon(arrow, r.x + 28. + i as f32 * 11., cy, 11., tone);
    }
    w
}
