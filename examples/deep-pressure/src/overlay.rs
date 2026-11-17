//! Cards over the chart between dives: briefing and refit bay, sortie
//! debrief, wreck recovery and the campaign ending. Every card reads as
//! icons, counts and one headline; the button under it mirrors Space.
use crate::{
    Game,
    art::*,
    content::{HazardKind, UPGRADE_NAMES},
    model::{Phase, card_hit, launch_hit},
    view::{SEA_H, SEA_W, SEA_X, SEA_Y},
};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;

/// Refit card rectangle `(x, y, w, h)`, matching [`card_hit`].
pub(crate) fn card_rect(index: usize) -> (f32, f32, f32, f32) {
    (64. + index as f32 * 204., 335., 188., 100.)
}
pub(crate) fn card_center(index: usize) -> Vec2 {
    let (x, y, w, h) = card_rect(index);
    vec2(x + w * 0.5, y + h * 0.5)
}
/// What each refit improves, in its own colour.
pub(crate) fn refit_tone(index: usize) -> Color {
    match index {
        1 => STEEL,
        _ => AQUA,
    }
}
/// The medal's one-word name.
pub(crate) fn medal_name(medal: u8) -> &'static str {
    ["", "Bronze", "Silver", "Gold"][usize::from(medal.min(3))]
}
/// Where the debrief shows the sortie medal.
pub(crate) const MEDAL_AT: Vec2 = vec2(108., 236.);

impl Game {
    pub(crate) fn draw_overlay(&self) {
        // Scrim so the modal card reads above the live chart.
        rounded(SEA_X, SEA_Y, SEA_W, SEA_H, 6., with_alpha(INK, 0.5));
        card(42., 160., 626., 376., Color::new(0.045, 0.085, 0.125, 0.98));
        let accent = match self.phase {
            Phase::Wreck => CORAL,
            Phase::Debrief | Phase::Complete => GOLD,
            _ => AQUA,
        };
        rounded(82., 160., 64., 3., 1.5, accent);
        match self.phase {
            Phase::Briefing => self.draw_briefing(),
            Phase::Debrief => self.draw_debrief(),
            Phase::Wreck => self.draw_wreck(),
            Phase::Complete => self.draw_complete(),
            Phase::Diving => {}
        }
    }

    fn draw_briefing(&self) {
        let t = clock();
        let chart = self.chart();
        say_fit(chart.name, 64., 210., 580., 26., PAPER);
        // The job: these people, home.
        let cy = 248.;
        let mut x = 64.;
        for i in 0..chart.divers.len() {
            let lift = bob(t + i as f32 * 0.21, 0.6, 1.5);
            icon(Icon::Person, x + 11., cy + lift, 22., AQUA);
            x += 25.;
        }
        icon(Icon::ArrowRight, x + 10., cy, 15., MUTED);
        icon_badge(Icon::Home, x + 36., cy, 28., GOLD, INK);
        // What waits below: each hazard kind, the current and the pressure.
        let mut right = 652.;
        let mut kinds = [false; 3];
        for hazard in chart.hazards {
            kinds[hazard.kind as usize] = true;
        }
        if self.mission >= 4 {
            let w = 30.;
            hazard_pill(right - w, cy, w, VIOLET);
            icon(Icon::Alert, right - w * 0.5, cy, 15., VIOLET);
            right -= w + 8.;
        }
        if chart.current != 0. {
            right -= current_pill(right, cy, chart.current) + 8.;
        }
        for kind in [HazardKind::Vent, HazardKind::Mine, HazardKind::Jelly] {
            if kinds[kind as usize] {
                let w = 34.;
                hazard_pill(right - w, cy, w, CORAL);
                hazard_glyph(kind, vec2(right - w * 0.5, cy + 1.), 20.);
                right -= w + 8.;
            }
        }

        divider(64., 280., 588., HAIRLINE);
        icon_chip_right(Icon::Coin, &self.credits.to_string(), 652., 300., 15., GOLD);
        for index in 0..3 {
            self.draw_refit(index);
        }
        launch_button("Dive", Icon::Play, AQUA);
    }

    fn draw_refit(&self, index: usize) {
        let (x, y, w, h) = card_rect(index);
        let hover = card_hit(pointer(), index);
        let selected = self.selected == index;
        let level = self.upgrades[index];
        let maxed = level >= 2;
        let cost = self.upgrade_cost(index);
        let afford = maxed || self.credits >= cost;
        let tone = refit_tone(index);
        let y = y - if hover { 3. } else { 0. };
        if hover {
            glow(x + w * 0.5, y + h * 0.5, w * 0.6, with_alpha(tone, 0.1));
        }
        let (top, bottom) = if selected || hover {
            (
                Color::new(0.1, 0.24, 0.3, 1.),
                Color::new(0.06, 0.15, 0.2, 1.),
            )
        } else {
            (
                Color::new(0.08, 0.14, 0.19, 1.),
                Color::new(0.05, 0.1, 0.14, 1.),
            )
        };
        rounded_gradient(x, y, w, h, 10., top, bottom);
        let rim = if selected {
            with_alpha(AQUA, 0.85)
        } else if hover {
            with_alpha(tone, 0.6)
        } else {
            HAIRLINE
        };
        outline(x, y, w, h, 10., if selected { 1.5 } else { 1. }, rim);
        keycap(&(index + 1).to_string(), "", x + 12., y + 12., PAPER);
        say(UPGRADE_NAMES[index], x + 42., y + 27., 15., PAPER);
        // What it adds, as the instrument it changes.
        let c = vec2(x + 34., y + 61.);
        let delta = match index {
            0 => {
                air_glyph(c.x, c.y, 30., tone);
                "+12s"
            }
            1 => {
                icon(Icon::Shield, c.x, c.y, 28., tone);
                "+20"
            }
            _ => {
                icon(Icon::Person, c.x, c.y, 28., tone);
                "+1"
            }
        };
        say_mid(delta, x + 56., c.y, 22., tone);
        for pip in 0..2u8 {
            let px = x + 12. + f32::from(pip) * 18.;
            let fill = if pip < level {
                tone
            } else {
                Color::new(1., 1., 1., 0.1)
            };
            rounded(px, y + 86., 14., 5., 2.5, fill);
        }
        if maxed {
            icon_chip_right(Icon::Check, "", x + w - 12., y + 72., 13., MUTED);
        } else {
            icon_chip_right(
                Icon::Coin,
                &cost.to_string(),
                x + w - 12.,
                y + 72.,
                13.,
                if afford { GOLD } else { CORAL },
            );
        }
        if !afford {
            rounded(x, y, w, h, 10., Color::new(0.02, 0.04, 0.06, 0.35));
        }
    }

    fn draw_debrief(&self) {
        let t = clock();
        let won = self.medals[self.mission];
        let metal = medal_metal(won);
        glow(
            MEDAL_AT.x,
            MEDAL_AT.y,
            50.,
            with_alpha(metal, 0.22 + 0.1 * pulse(t, 0.6)),
        );
        medal(MEDAL_AT, 28., won, true);
        say_fit(self.chart().name, 156., 228., 470., 24., PAPER);
        say(medal_name(won), 156., 258., 18., metal);
        // This sortie at a glance.
        let tiles: [(Icon, String, Color); 4] = [
            (Icon::Person, format!("+{}", self.delivered()), GOLD),
            (Icon::Coin, self.credits.to_string(), GOLD),
            (Icon::Clock, clock_label(self.elapsed / 60), PAPER),
            (
                Icon::Alert,
                self.stage_impacts.to_string(),
                if self.stage_impacts == 0 {
                    MUTED
                } else {
                    CORAL
                },
            ),
        ];
        stat_tiles(&tiles, 296.);
        // The career's medal case.
        let left = 355. - 2.5 * 32.;
        for (i, &m) in self.medals.iter().enumerate() {
            let c = vec2(left + i as f32 * 32., 420.);
            if i == self.mission {
                glow(c.x, c.y, 18., with_alpha(metal, 0.3));
            }
            if m > 0 {
                medal(c, 10., m, false);
            } else {
                shapes::stroke_circle(c, 9., 1.2, Color::new(1., 1., 1., 0.16));
            }
        }
        launch_button("Next", Icon::Play, AQUA);
    }

    fn draw_wreck(&self) {
        let t = clock();
        let hull = self.hull <= 0.;
        glow(MEDAL_AT.x, MEDAL_AT.y, 46., with_alpha(CORAL, 0.18));
        if hull {
            icon(Icon::Shield, MEDAL_AT.x, MEDAL_AT.y, 48., CORAL);
        } else {
            air_glyph(MEDAL_AT.x, MEDAL_AT.y, 48., CORAL);
        }
        cross_badge(MEDAL_AT.x, MEDAL_AT.y, 48.);
        say_fit(
            if hull { "Hull breach" } else { "Out of air" },
            156.,
            244.,
            470.,
            26.,
            PAPER,
        );
        // The sortie starts over: its divers return to their bells.
        let cy = 306.;
        icon(Icon::Restart, 76., cy, 18., MUTED);
        for i in 0..self.chart().divers.len() {
            icon(
                Icon::Person,
                104. + i as f32 * 22.,
                cy,
                18.,
                with_alpha(AQUA, 0.55),
            );
        }
        // Recovery tugs: the next one out pulses.
        for i in 0..3 {
            let c = vec2(297. + i as f32 * 58., 392.);
            let available = i < usize::from(self.recovery);
            let next = available && i + 1 == usize::from(self.recovery);
            if next {
                glow(c.x, c.y, 34., with_alpha(GOLD, 0.2 + 0.15 * pulse(t, 1.2)));
            }
            if available {
                icon_badge(Icon::Ship, c.x, c.y, 46., GOLD, INK);
            } else {
                shapes::stroke_circle(c, 22., 1.5, Color::new(1., 1., 1., 0.14));
                icon(Icon::Ship, c.x, c.y, 24., DIM);
            }
        }
        if self.recovery > 0 {
            launch_button("Retry", Icon::Restart, GOLD);
        } else {
            let w = keycap_w("R") + 32.;
            let x = 64. + (588. - w) * 0.5;
            keycap("R", "", x, 489., PAPER);
            icon(Icon::Restart, x + w - 10., 499., 18., PAPER);
        }
    }

    fn draw_complete(&self) {
        let t = clock();
        glow(
            MEDAL_AT.x,
            MEDAL_AT.y,
            50.,
            with_alpha(GOLD, 0.25 + 0.1 * pulse(t, 0.6)),
        );
        icon(Icon::Trophy, MEDAL_AT.x, MEDAL_AT.y, 56., GOLD);
        say_fit("All home", 156., 244., 470., 26., PAPER);
        // Every rescued person, and every medal.
        for i in 0..self.rescued() {
            let col = (i % 14) as f32;
            let row = (i / 14) as f32;
            icon(Icon::Person, 86. + col * 26., 312. + row * 30., 20., GOLD);
        }
        let left = 355. - 2.5 * 32.;
        for (i, &m) in self.medals.iter().enumerate() {
            medal(vec2(left + i as f32 * 32., 420.), 10., m, false);
        }
        let w = keycap_w("R") + 32.;
        let x = 64. + (588. - w) * 0.5;
        keycap("R", "", x, 489., PAPER);
        icon(Icon::Restart, x + w - 10., 499., 18., PAPER);
    }
}

/// A row of four icon-and-value tiles across the card at `top`.
fn stat_tiles(tiles: &[(Icon, String, Color); 4], top: f32) {
    let (gap, h) = (12., 76.);
    let w = (588. - gap * 3.) / 4.;
    for (i, (glyph, value, tone)) in tiles.iter().enumerate() {
        let x = 64. + i as f32 * (w + gap);
        rounded(x, top, w, h, 10., Color::new(1., 1., 1., 0.04));
        outline(x, top, w, h, 10., 1., HAIRLINE);
        icon(*glyph, x + 30., top + h * 0.5, 26., *tone);
        say_mid(value, x + 52., top + h * 0.5, 26., *tone);
    }
}
/// Tinted pill behind a hazard glyph, centred vertically on `cy`.
fn hazard_pill(x: f32, cy: f32, w: f32, tone: Color) {
    let h = 28.;
    let r = Rect::new(x, cy - h * 0.5, w, h);
    shapes::fill_rounded_rect(r, h * 0.5, with_alpha(tone, 0.1));
    shapes::stroke_rounded_rect(r, h * 0.5, 1., with_alpha(tone, 0.35));
}
/// Current pill ending at `right`; returns its width.
fn current_pill(right: f32, cy: f32, current: f32) -> f32 {
    let steps = if current.abs() <= 3. {
        1
    } else if current.abs() <= 8. {
        2
    } else {
        3
    };
    let w = 34. + steps as f32 * 12.;
    let tone = Color::new(0.72, 0.9, 1., 1.);
    hazard_pill(right - w, cy, w, tone);
    icon(Icon::Wave, right - w + 16., cy, 16., tone);
    let arrow = if current > 0. {
        Icon::ArrowRight
    } else {
        Icon::ArrowLeft
    };
    for i in 0..steps {
        icon(arrow, right - w + 34. + i as f32 * 12., cy, 12., tone);
    }
    w
}
/// The wide primary button (Space or a click), matching [`launch_hit`].
fn launch_button(label: &str, glyph: Icon, color: Color) {
    let (x, y, w, h) = (64., 479., 588., 40.);
    let hover = launch_hit(pointer());
    let y = y - if hover { 2. } else { 0. };
    let r = Rect::new(x, y, w, h);
    shapes::drop_shadow(
        r,
        10.,
        shapes::Shadow::soft(Color::new(0., 0., 0., 0.35))
            .offset(vec2(0., 3.))
            .blur(10.),
    );
    let fill = if hover { lighten(color, 0.12) } else { color };
    shapes::fill_rounded_rect(
        r,
        10.,
        Fill::vertical(r, lighten(fill, 0.12), darken(fill, 0.12)),
    );
    outline(
        x,
        y,
        w,
        h,
        10.,
        1.,
        Color::new(1., 1., 1., if hover { 0.5 } else { 0.25 }),
    );
    let s = TextStyle::modern(18., INK);
    let label_w = text_width(label, s);
    let key_w = keycap_w("Space");
    let total = key_w + 12. + 20. + 8. + label_w;
    let left = x + (w - total) * 0.5;
    keycap("Space", "", left, y + h * 0.5 - 11., PAPER);
    icon(glyph, left + key_w + 22., y + h * 0.5, 18., INK);
    draw_text_styled(
        label,
        left + key_w + 40.,
        s.centered_baseline(y + h * 0.5),
        s,
    );
}
