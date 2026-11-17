//! Read-only presentation: goals, tabs, the chart, the header strip and the
//! arcade hooks. Panels live in [`crate::panels`], effects in [`crate::fx`]
//! and the drawing vocabulary in [`crate::look`].
//!
//! Every number is an icon and a value, every count a meter or pips, and every
//! accepted command is felt where it happened. Clickable things share their
//! hit regions with the rules through [`crate::layout`], so hover highlights
//! and previews always match what a click does. Idle motion uses the host's
//! presentation clock and never feeds back into the rules.
use crate::content::*;
use crate::layout::*;
use crate::look::*;
use crate::look::{GREEN, RED};
use crate::model::{Game, Refusal};
use ferravex_arcade::shapes::{self, Cap, Fill, Stroke};
use ferravex_arcade::typography::{TextAlign, draw_text_styled, text_width};
use ferravex_arcade::*;
use std::f32::consts::{PI, TAU};

// ---------------------------------------------------------------------------
// Goals and shared readouts

/// The next step of the campaign, for the header, markers and tab badges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Goal {
    /// Recover relic `i` from its sector.
    Recover(usize),
    /// Forge gate part `i`; its relic is aboard.
    Forge(usize),
    Return,
    Open,
    Done,
}
impl Game {
    pub(crate) fn goal(&self) -> Goal {
        if self.result == Outcome::Won {
            return Goal::Done;
        }
        if let Some(i) = (0..3).find(|&i| !self.components[i] && self.relics[i]) {
            return Goal::Forge(i);
        }
        if let Some(i) = (0..3).find(|&i| !self.components[i]) {
            return Goal::Recover(i);
        }
        if self.sector != 0 {
            Goal::Return
        } else {
            Goal::Open
        }
    }
    /// The sector the current goal points at, if any.
    pub(crate) fn goal_sector(&self) -> Option<usize> {
        match self.goal() {
            Goal::Recover(i) => Some(RELIC_SECTORS[i]),
            Goal::Return => Some(0),
            _ => None,
        }
    }
    /// A tab's attention dot: something worth doing waits there.
    pub(crate) fn tab_alert(&self, tab: usize) -> Option<Color> {
        if self.transit.is_some() || self.result != Outcome::Playing {
            return None;
        }
        // A survey is worth it only where it unlocks deposits or a landmark.
        let site = self.sites[self.sector];
        let hidden = site.stock.iter().any(|&n| n > 0) || (self.sector != 0 && !site.claimed);
        let survey = self.work_refusal(0).is_none() && hidden;
        match tab {
            1 if survey || self.work_refusal(4).is_none() => Some(CYAN),
            2 if (0..3).any(|i| self.forge_refusal(i).is_none()) => Some(GOLD),
            3 if self.hull < 25 || self.fuel == 0 => Some(RED),
            4 if self.goal() == Goal::Open => Some(GOLD),
            _ => None,
        }
    }
    /// One icon and a word or two for a refusal (button wells and popups).
    pub(crate) fn refusal_label(&self, reason: Refusal, row: usize) -> (Icon, String) {
        match reason {
            Refusal::NoLane => (Icon::Cross, "No lane".into()),
            Refusal::Fuel(n) => (Icon::Fuel, format!("Need {n}")),
            Refusal::Credits(n) => (Icon::Coin, format!("Need {n}")),
            Refusal::Cargo(k, n) => (RESOURCE_ICON[k], format!("Need {n}")),
            Refusal::Materials => {
                let cost = RECIPES[row.min(RECIPES.len() - 1)].cost;
                let k = (0..3).find(|&k| self.cargo[k] < cost[k]).unwrap_or(0);
                (RESOURCE_ICON[k], format!("Need {}", cost[k]))
            }
            Refusal::Space => (Icon::Crate, "Full".into()),
            Refusal::Relic(i) => (RELIC_ICON[i], "Missing".into()),
            Refusal::Unsurveyed => (Icon::Lock, "Survey".into()),
            Refusal::Depleted => (Icon::Pickaxe, "Empty".into()),
            Refusal::Done => (Icon::Check, "Done".into()),
            Refusal::TankFull => (Icon::Fuel, "Full".into()),
            Refusal::HullFull => (Icon::Heart, "Full".into()),
            Refusal::Station => (Icon::Home, "Station".into()),
            Refusal::Home => (Icon::Home, "Loom".into()),
            Refusal::Docked => (Icon::Home, "Here".into()),
            Refusal::Mechanisms => (Icon::Flag, format!("{}/3", self.built())),
            Refusal::Gate => (Icon::Flag, "Tab 5".into()),
        }
    }
    /// A landmark's rewards as tags from `x`; returns the end.
    pub(crate) fn landmark_rewards(&self, i: usize, x: f32, cy: f32, size: f32, on: bool) -> f32 {
        let f = |c: Color| faded(c, on);
        match i {
            0 => {
                let parts = format!("{}/3", self.built());
                gate_piece(vec2(x + size * 0.5, cy), size * 1.1, 0, f(CYAN));
                x + size
                    + 3.
                    + label(
                        &parts,
                        x + size + 3.,
                        cy + size * 0.32,
                        tag_text(size),
                        f(CYAN),
                    )
            }
            1 => tags(
                &[bit(RESOURCE_ICON[0], "+2", f(RESOURCE[0]))],
                x,
                cy,
                size,
                9.,
            ),
            2 => tags(&[bit(Icon::Fuel, "+4", f(FUEL))], x, cy, size, 9.),
            3 => tags(
                &[bit(Icon::Key, "", f(GOLD)), bit(Icon::Coin, "+20", f(COIN))],
                x,
                cy,
                size,
                9.,
            ),
            4 => tags(&[bit(Icon::Music, "", f(GOLD))], x, cy, size, 9.),
            5 => tags(
                &[
                    bit(RESOURCE_ICON[1], "+2", f(RESOURCE[1])),
                    bit(Icon::Coin, "+15", f(COIN)),
                ],
                x,
                cy,
                size,
                9.,
            ),
            6 => tags(
                &[
                    bit(RESOURCE_ICON[2], "+3", f(RESOURCE[2])),
                    bit(Icon::Coin, "+40", f(COIN)),
                ],
                x,
                cy,
                size,
                9.,
            ),
            7 => tags(&[bit(Icon::Battery, "", f(GOLD))], x, cy, size, 9.),
            _ => tags(
                &[
                    bit(Icon::People, "9", f(LANDMARK_COLOR[8])),
                    bit(Icon::Heart, "+10", f(HULL)),
                    bit(Icon::Coin, "+25", f(COIN)),
                ],
                x,
                cy,
                size,
                8.,
            ),
        }
    }

    // -----------------------------------------------------------------------
    // Tabs

    pub(crate) fn draw_tabs(&self, t: f32, p: Vec2, live: bool) {
        let hover = if live { tab_at(p) } else { None };
        for i in 0..5 {
            let x = TAB_X + i as f32 * TAB_STEP;
            let active = i == self.page;
            let hovered = hover == Some(i) && !active;
            let y = TAB_TOP - if hovered { 1.5 } else { 0. };
            if active {
                rounded_gradient(x, y, TAB_W, TAB_H, 8., lighten(DIM, 0.14), DIM);
                outline(x, y, TAB_W, TAB_H, 8., 1., with_alpha(CYAN, 0.4));
                shapes::fill_capsule(
                    vec2(x + 24., y + TAB_H - 2.),
                    vec2(x + TAB_W - 24., y + TAB_H - 2.),
                    1.3,
                    CYAN,
                );
            } else if hovered {
                rounded(x, y, TAB_W, TAB_H, 8., Color::new(0.1, 0.16, 0.24, 0.95));
                outline(x, y, TAB_W, TAB_H, 8., 1., with_alpha(CYAN, 0.28));
            } else {
                rounded(x, y, TAB_W, TAB_H, 8., Color::new(0.07, 0.115, 0.18, 0.9));
            }
            let fg = if active {
                PAPER
            } else if hovered {
                SOFT
            } else {
                MUTED
            };
            let named = active || hovered;
            let name_w = if named {
                text_width(TAB_NAME[i], ink(14., fg)) + 8.
            } else {
                0.
            };
            let cy = y + TAB_H * 0.5;
            let left = x + TAB_W * 0.5 - (16. + 7. + 18. + name_w) * 0.5;
            const KEYS: [&str; 5] = ["1", "2", "3", "4", "5"];
            mini_key(KEYS[i], left + 8., cy, if active { 1. } else { 0.7 });
            icon(
                TAB_ICON[i],
                left + 32.,
                cy,
                18.,
                if active { CYAN } else { fg },
            );
            if named {
                line(TAB_NAME[i], left + 49., cy, 90., 14., fg);
            }
            if !active && let Some(color) = self.tab_alert(i) {
                let beat = pulse(t, 1.1);
                let at = vec2(x + TAB_W - 13., y + 8.);
                glow(at.x, at.y, 7. + 4. * beat, with_alpha(color, 0.3));
                shapes::fill_circle(at, 3.5, color);
            }
        }
    }

    // -----------------------------------------------------------------------
    // The chart

    pub(crate) fn draw_map(&self, t: f32, p: Vec2, live: bool) {
        dying_star(self.watch, t);
        let chart = self.page == 0;
        let flying = self.transit.is_some();
        let hover = if live && chart { star_at(p) } else { None };
        let target = if chart && !flying {
            hover.or(Some(self.selected))
        } else {
            None
        }
        .filter(|&i| i != self.sector);
        for &(a, b) in &LINKS {
            let (pa, pb) = (star(a), star(b));
            if !flying && (a == self.sector || b == self.sector) {
                draw_line(
                    pa.x,
                    pa.y,
                    pb.x,
                    pb.y,
                    1.5,
                    Color::new(0.35, 0.7, 0.78, 0.55),
                );
            } else {
                draw_line(pa.x, pa.y, pb.x, pb.y, 1., Color::new(0.3, 0.45, 0.6, 0.28));
            }
        }
        if let Some(to) = target {
            let (path, n) = route(self.sector, to);
            let mut from = self.sector;
            for (k, &next) in path[..n].iter().enumerate() {
                let (color, width) = if k == 0 {
                    (GOLD, 2.)
                } else {
                    (with_alpha(GOLD, 0.45), 1.5)
                };
                marching(star(from), star(next), t, color, width);
                from = next;
            }
        }
        let here = star(self.sector);
        if !flying {
            halo(here, 34., CYAN);
        }
        let goal = self.goal_sector();
        for (i, sector) in CHART.iter().enumerate() {
            let c = star(i);
            let site = self.sites[i];
            if hover == Some(i) {
                halo(c, 40., PAPER);
                shapes::stroke_circle(c, STAR_RADIUS, 1.2, with_alpha(PAPER, 0.55));
            }
            if chart && i == self.selected {
                let spin = t * 0.6;
                for k in 0..4 {
                    shapes::stroke_arc(
                        c,
                        31.,
                        spin + k as f32 * TAU / 4.,
                        TAU / 4. - 0.45,
                        Stroke::new(1.6, GOLD).cap(Cap::Round),
                    );
                }
            }
            if i == self.sector {
                draw_circle_lines(c.x, c.y, 26.5, 2., CYAN);
            }
            if site.scanned {
                // A slow radar sweep marks surveyed sectors.
                let a = t * 0.9 + i as f32 * 1.7;
                shapes::stroke_arc(
                    c,
                    22.,
                    a,
                    1.1,
                    Stroke::new(1.6, with_alpha(CYAN, 0.6)).cap(Cap::Round),
                );
            }
            planet(i, t, self.components);
            if i == 4 || i == 7 {
                let orbit = t * 0.67 + i as f32;
                draw_circle(c.x + orbit.cos() * 23., c.y + orbit.sin() * 13., 2., PAPER);
            }
            label_center(
                sector.name,
                c.x,
                c.y + 40.,
                12.,
                if site.visited || hover == Some(i) {
                    PAPER
                } else {
                    MUTED
                },
            );
            if i != 0 {
                let at = c + vec2(21., -21. + bob(t + i as f32 * 0.37, 0.45, 1.4));
                if site.claimed {
                    icon_badge(Icon::Check, at.x, at.y, 15., darken(GREEN, 0.6), GREEN);
                } else {
                    let color = LANDMARK_COLOR[i];
                    icon_badge(
                        LANDMARK_ICON[i],
                        at.x,
                        at.y,
                        19.,
                        darken(color, 0.74),
                        color,
                    );
                }
            }
            if !site.visited && sector.arrival_damage > 0 {
                let a = 0.5 + 0.4 * pulse(t, 0.9);
                icon(Icon::Alert, c.x - 22., c.y - 20., 13., with_alpha(RED, a));
            }
            if goal == Some(i) && i != self.sector {
                // Sonar rings call the player to the goal; a pin bobs above
                // when the chart has room for it.
                for k in 0..2 {
                    let phase = (t * 0.6 + k as f32 * 0.5).fract();
                    shapes::stroke_circle(
                        c,
                        24. + 26. * phase,
                        1.5,
                        with_alpha(GOLD, 0.5 * (1. - phase)),
                    );
                }
                let y = c.y - 46. + bob(t, 0.8, 3.);
                if y > MAP.y + 14. {
                    glow(c.x, y, 15., with_alpha(GOLD, 0.25));
                    icon(Icon::Pin, c.x, y, 18., GOLD);
                }
            }
        }
        if let Some(to) = hover.filter(|&i| i != self.sector && !flying) {
            self.route_costs(to);
        }
        if let Some(flight) = self.transit {
            let progress = 1. - f32::from(flight.remaining) / f32::from(flight.total);
            let a = star(flight.from);
            let b = star(flight.to);
            let at = |s: f32| a.lerp(b, s) - vec2(0., (s * PI).sin() * 14.);
            for k in 1..6 {
                let s = (progress - k as f32 * 0.03).max(0.);
                let q = at(s);
                draw_circle(
                    q.x,
                    q.y,
                    2.4 - k as f32 * 0.35,
                    with_alpha(CYAN, 0.5 - k as f32 * 0.08),
                );
            }
            let q = at(progress);
            halo(q, 16., CYAN);
            ship(q.x, q.y, 0.6, true);
        } else {
            ship(here.x + 34., here.y - 16. + bob(t, 0.48, 1.5), 0.48, false);
        }
    }
    /// The whole route's fuel, watches and first-entry damage beside a
    /// hovered star.
    pub(crate) fn route_costs(&self, to: usize) {
        let (path, n) = route(self.sector, to);
        if n == 0 {
            return;
        }
        let hurt: u16 = path[..n]
            .iter()
            .filter(|&&s| !self.sites[s].visited)
            .map(|&s| self.shielded(CHART[s].arrival_damage))
            .sum();
        let hops = format!("-{n}");
        let pain = format!("-{hurt}");
        let short = n > usize::from(self.fuel);
        let mut bits = [
            bit(Icon::Fuel, &hops, if short { RED } else { FUEL }),
            bit(Icon::Hourglass, &hops, TIME),
            bit(Icon::Heart, &pain, HULL),
        ];
        let count = if hurt > 0 { 3 } else { 2 };
        let bits = &mut bits[..count];
        let w: f32 = bits
            .iter()
            .map(|b| tag_width(b.text, 14.) + 8.)
            .sum::<f32>()
            - 8.;
        let c = star(to);
        let x = (c.x - w * 0.5).clamp(MAP.x + 14., MAP.x + MAP.w - 14. - w);
        let cy = c.y + 62.;
        rounded(
            x - 7.,
            cy - 12.,
            w + 14.,
            24.,
            12.,
            Color::new(0.03, 0.06, 0.1, 0.9),
        );
        outline(
            x - 7.,
            cy - 12.,
            w + 14.,
            24.,
            12.,
            1.,
            with_alpha(GOLD, 0.4),
        );
        tags(bits, x, cy, 14., 8.);
    }

    /// The lane route to `to` as planet beads, with its total cost.
    pub(crate) fn route_card(&self, to: usize, y: f32) {
        let (path, n) = route(self.sector, to);
        if n == 0 {
            return;
        }
        rounded(COL, y, COL_W, 68., 10., WELL);
        let cy = y + 22.;
        let step = (COL_W - 44.) / n as f32;
        let at = |k: usize| vec2(COL + 22. + step * k as f32, cy);
        let node = |k: usize| if k == 0 { self.sector } else { path[k - 1] };
        for k in 0..n {
            let color = if k == 0 { GOLD } else { with_alpha(GOLD, 0.5) };
            let (a, b) = (at(k), at(k + 1));
            draw_line(
                a.x + 9.,
                a.y,
                b.x - 9.,
                b.y,
                if k == 0 { 2. } else { 1.5 },
                color,
            );
        }
        let mut hurt = 0;
        for k in 0..=n {
            let s = node(k);
            let c = at(k);
            let color = CHART[s].color;
            shapes::fill_circle(
                c,
                7.,
                Fill::radial(c - vec2(3., 3.), 11., tint(color, 1.15), tint(color, 0.4)),
            );
            if k == 0 {
                shapes::stroke_circle(c, 10., 1.5, CYAN);
            } else if !self.sites[s].visited && CHART[s].arrival_damage > 0 {
                hurt += self.shielded(CHART[s].arrival_damage);
                icon(Icon::Alert, c.x + 9., c.y - 9., 10., RED);
            }
            if k == n {
                shapes::stroke_circle(c, 10.5, 1.5, GOLD);
            }
        }
        let hops = format!("-{n}");
        let pain = format!("-{hurt}");
        let short = n > usize::from(self.fuel);
        let bits = [
            bit(Icon::Fuel, &hops, if short { RED } else { FUEL }),
            bit(Icon::Hourglass, &hops, TIME),
            bit(Icon::Heart, &pain, HULL),
        ];
        let count = if hurt > 0 { 3 } else { 2 };
        tags(&bits[..count], COL + 14., y + 50., 15., 14.);
    }

    // -----------------------------------------------------------------------
    // Right-hand panels

    /// Sector eyebrow, name and planet swatch.
    pub(crate) fn sector_title(&self, i: usize) {
        let s = &CHART[i];
        eyebrow(s.kind, COL, 174., MUTED);
        line(s.name, COL, 192., COL_W - 34., 21., tint(s.color, 1.05));
        let c = vec2(COL + COL_W - 11., 185.);
        shapes::fill_circle(
            c,
            10.,
            Fill::radial(
                c - vec2(4., 4.),
                16.,
                tint(s.color, 1.1),
                tint(s.color, 0.35),
            ),
        );
    }
    pub(crate) fn panel_chart(&self, t: f32, p: Vec2, live: bool) {
        let i = self.selected;
        let s = &CHART[i];
        let site = self.sites[i];
        self.sector_title(i);
        // Landmark: what is waiting there, and whether it is still unclaimed.
        let y = 210.;
        rounded(COL, y, COL_W, 52., 10., WELL);
        let claimed = site.claimed;
        let color = LANDMARK_COLOR[i];
        let b = vec2(COL + 26., y + 26.);
        if claimed {
            icon_badge(
                LANDMARK_ICON[i],
                b.x,
                b.y,
                36.,
                Color::new(0.1, 0.15, 0.2, 1.),
                MUTED,
            );
            icon_badge(
                Icon::Check,
                b.x + 13.,
                b.y + 12.,
                16.,
                darken(GREEN, 0.55),
                GREEN,
            );
        } else {
            glow(b.x, b.y, 26., with_alpha(color, 0.12));
            icon_badge(LANDMARK_ICON[i], b.x, b.y, 36., darken(color, 0.72), color);
        }
        line(
            LANDMARK_NAME[i],
            COL + 52.,
            y + 17.,
            COL_W - 60.,
            14.,
            if claimed { MUTED } else { PAPER },
        );
        self.landmark_rewards(i, COL + 52., y + 37., 15., !claimed);
        // Deposits: charted stock until surveyed, then what is left.
        let y = 272.;
        for k in 0..3 {
            let x = COL + k as f32 * 97.;
            rounded(x, y, 88., 48., 9., WELL);
            let original = s.supplies[k];
            let left = if site.scanned {
                site.stock[k]
            } else {
                original
            };
            let value = left.to_string();
            let color = if left > 0 {
                RESOURCE[k]
            } else {
                with_alpha(RESOURCE[k], 0.45)
            };
            tag(RESOURCE_ICON[k], &value, x + 9., y + 17., 17., color);
            stock_pips(x + 9., y + 37., left, original, RESOURCE[k], true);
        }
        // Hazards: first-entry and per-extraction hull damage after shielding.
        let cy = 346.;
        let entry = self.shielded(s.arrival_damage);
        let dig = self.shielded(s.extraction_damage);
        let mut x = COL;
        let entry_text = format!("-{entry}");
        let dig_text = format!("-{dig}");
        if !site.visited && entry > 0 {
            x += tag(Icon::Rocket, "", x, cy, 15., MUTED) + 2.;
            x += tag(Icon::Heart, &entry_text, x, cy, 15., HULL) + 16.;
        }
        if dig > 0 {
            x += tag(Icon::Pickaxe, "", x, cy, 15., MUTED) + 2.;
            x += tag(Icon::Heart, &dig_text, x, cy, 15., HULL) + 16.;
        }
        if x == COL {
            tag(Icon::Shield, "Safe", x, cy, 15., GREEN);
        }
        if s.station {
            let cy = 380.;
            let w = icon_chip(Icon::Home, "Station", COL, cy - 9., 13., GOLD);
            tags(
                &[
                    bit(Icon::Fuel, "", FUEL),
                    bit(Icon::Wrench, "", HULL),
                    bit(Icon::Coin, "", COIN),
                ],
                COL + w + 10.,
                cy,
                15.,
                8.,
            );
        }
        if self.transit.is_some() {
            return;
        }
        self.route_card(i, 410.);
        if i == self.sector {
            let here = Some((Icon::Pin, "Here".to_string()));
            button(Icon::Rocket, "Jump", &[], here, live, p, t);
            return;
        }
        match self.jump_refusal(i) {
            None => {
                let hurt = if site.visited { 0 } else { entry };
                let pain = format!("-{hurt}");
                let bits = [
                    bit(Icon::Fuel, "-1", FUEL),
                    bit(Icon::Hourglass, "-1", TIME),
                    bit(Icon::Heart, &pain, HULL),
                ];
                let n = if hurt > 0 { 3 } else { 2 };
                button(Icon::Rocket, "Jump", &bits[..n], None, live, p, t);
            }
            Some(reason) => {
                let why = self.refusal_label(reason, 0);
                button(Icon::Rocket, "Jump", &[], Some(why), live, p, t);
            }
        }
    }
}

impl Game {
    // -----------------------------------------------------------------------
    // Header strip

    /// Watches left, hull, fuel and credits, then the three cargo kinds and
    /// the hold, at fixed slots in the header.
    pub(crate) fn stat_strip(&self, t: f32) {
        let cy = STAT_Y;
        let left = DEADLINE - self.watch.min(DEADLINE);
        let col = watch_color(left);
        let wx = stat_x(Stat::Watch) + 11.;
        if left < 8 {
            glow(wx, cy, 16. + 4. * pulse(t, 1.4), with_alpha(RED, 0.25));
        }
        shapes::stroke_circle(vec2(wx, cy), 11., 3.5, Color::new(1., 1., 1., 0.1));
        ring_meter(wx, cy, 11., 3.5, f32::from(left) / f32::from(DEADLINE), col);
        icon(Icon::Hourglass, wx, cy, 12., col);
        let value = left.to_string();
        label(&value, wx + 16., cy + 5., 16., PAPER);
        let hull = self.hull.to_string();
        let hull_color = if self.hull < 25 {
            with_alpha(RED, 0.7 + 0.3 * pulse(t, 1.6))
        } else {
            HULL
        };
        tag(Icon::Heart, &hull, stat_x(Stat::Hull), cy, 16., hull_color);
        let fuel = self.fuel.to_string();
        tag(
            Icon::Fuel,
            &fuel,
            stat_x(Stat::Fuel),
            cy,
            16.,
            if self.fuel == 0 { RED } else { FUEL },
        );
        let credits = self.credits.to_string();
        tag(Icon::Coin, &credits, stat_x(Stat::Credits), cy, 16., COIN);
        let split = stat_x(Stat::Cargo(0)) - 12.;
        rounded(split, cy - 10., 1., 20., 0.5, Color::new(1., 1., 1., 0.12));
        for k in 0..3 {
            let n = self.cargo[k].to_string();
            let color = if self.cargo[k] == 0 {
                with_alpha(RESOURCE[k], 0.45)
            } else {
                RESOURCE[k]
            };
            tag(RESOURCE_ICON[k], &n, stat_x(Stat::Cargo(k)), cy, 16., color);
        }
        let hold = format!("{}/{}", self.hold_used(), self.hold_max());
        let full = self.hold_free() == 0;
        tag(
            Icon::Crate,
            &hold,
            stat_x(Stat::Hold),
            cy,
            16.,
            if full { RED } else { SOFT },
        );
    }
}

impl ArcadeGame for Game {
    fn new() -> Self {
        Self::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        let t = clock();
        let p = pointer();
        let live =
            self.result == Outcome::Playing && self.transit.is_none() && self.encounter.is_none();
        backdrop(t);
        surface(624., 150., 312., 418.);
        self.draw_tabs(t, p, live);
        match self.page {
            0 => {
                self.draw_map(t, p, live);
                self.panel_chart(t, p, live);
            }
            1 => {
                self.draw_map(t, p, live);
                self.panel_survey(t, p, live);
            }
            2 => {
                self.forge_left(t, p, live);
                self.panel_forge(t, p, live);
            }
            3 => {
                self.ship_left(t);
                self.panel_ship(t, p, live);
            }
            _ => {
                self.mission_left(t);
                self.panel_mission(t, p, live);
            }
        }
        self.draw_transit();
        if let Some(event) = self.encounter {
            self.draw_encounter(event, t, p);
        }
    }
    fn hud(&self) -> Hud {
        let objective = match self.goal() {
            Goal::Recover(i) => format!("{} at {}", RELIC_NAMES[i], CHART[RELIC_SECTORS[i]].name),
            Goal::Forge(i) => format!("Forge the {}", COMPONENT_NAMES[i]),
            Goal::Return => "Return to Loom Dock".into(),
            Goal::Open => "Open the gate".into(),
            Goal::Done => "The ark is through".into(),
        };
        Hud {
            stage: CHART[self.sector].name.into(),
            objective,
            status: format!(
                "Watch {}/{DEADLINE}, hull {}, fuel {}, credits {}, hold {}/{}",
                self.watch,
                self.hull,
                self.fuel,
                self.credits,
                self.hold_used(),
                self.hold_max()
            ),
            score: self.points,
        }
    }
    fn outcome(&self) -> Outcome {
        self.result
    }
    fn summary(&self) -> String {
        match self.result {
            Outcome::Won => format!(
                "The ark escaped on watch {} with {} hull{}: {} points.",
                self.watch,
                self.hull,
                if self.crew {
                    " and all nine engineers"
                } else {
                    ""
                },
                self.points
            ),
            Outcome::Lost => format!(
                "{} with {} of 3 gate parts built: {} points.",
                if self.hull == 0 {
                    "The ship was lost"
                } else {
                    "The star collapsed"
                },
                self.built(),
                self.points
            ),
            Outcome::Playing => format!(
                "{} of 3 gate parts built; {} watches remain.",
                self.built(),
                DEADLINE - self.watch.min(DEADLINE)
            ),
        }
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
    fn progress(&self) -> Option<(u32, u32)> {
        Some(((self.built() as u32 + 1).min(4), 4))
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        for event in self.events.iter() {
            self.effect(event, fx);
        }
    }
    fn draw_objective(&self, area: Rect) -> bool {
        let t = clock();
        let cy = area.y + area.h * 0.5;
        let goal = self.goal();
        let focus = match goal {
            Goal::Forge(i) | Goal::Recover(i) => Some(i),
            _ => None,
        };
        let mut x = area.x + 4.;
        gate_ring(vec2(x + 13., cy), 11., 4.5, self, focus, t);
        x += 34.;
        for (k, &glyph) in RELIC_ICON.iter().enumerate() {
            let at = vec2(x + 11., cy);
            let (bg, fg) = if self.components[k] {
                (darken(CYAN, 0.6), lighten(CYAN, 0.4))
            } else if self.relics[k] {
                (darken(GOLD, 0.62), GOLD)
            } else {
                (Color::new(0.1, 0.14, 0.2, 1.), with_alpha(MUTED, 0.75))
            };
            if focus == Some(k) {
                glow(at.x, at.y, 15. + 3. * pulse(t, 1.), with_alpha(GOLD, 0.22));
            }
            icon_badge(glyph, at.x, at.y, 23., bg, fg);
            if self.components[k] {
                icon_badge(
                    Icon::Check,
                    at.x + 8.,
                    at.y + 7.,
                    11.,
                    darken(GREEN, 0.5),
                    GREEN,
                );
            }
            x += 27.;
        }
        x += 6.;
        icon(Icon::ArrowRight, x + 6., cy, 12., MUTED);
        x += 18.;
        let room = STRIP - 24. - x;
        match goal {
            Goal::Recover(i) => {
                let s = &CHART[RELIC_SECTORS[i]];
                icon(Icon::Pin, x + 8., cy, 17., GOLD);
                line(s.name, x + 21., cy, room - 21., 17., tint(s.color, 1.15));
            }
            Goal::Forge(i) => {
                icon(Icon::Hammer, x + 8., cy, 17., GOLD);
                line(COMPONENT_NAMES[i], x + 21., cy, room - 21., 17., PAPER);
            }
            Goal::Return => {
                icon(Icon::Home, x + 8., cy, 17., GOLD);
                line(
                    CHART[0].name,
                    x + 21.,
                    cy,
                    room - 21.,
                    17.,
                    tint(CHART[0].color, 1.15),
                );
            }
            Goal::Open => {
                let beat = pulse(t, 1.2);
                glow(x + 8., cy, 14. + 4. * beat, with_alpha(GOLD, 0.25));
                icon(Icon::Flag, x + 8., cy, 17., GOLD);
                line(
                    "Open gate",
                    x + 21.,
                    cy,
                    room - 21.,
                    17.,
                    lighten(GOLD, 0.2 * beat),
                );
            }
            Goal::Done => {
                icon(Icon::Check, x + 8., cy, 17., GREEN);
                line("Gate open", x + 21., cy, room - 21., 17., GREEN);
            }
        }
        self.stat_strip(t);
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        let won = self.result == Outcome::Won;
        let watch = self.watch.to_string();
        let hull = self.hull.to_string();
        let mapped = format!("{}/9", self.scanned());
        let built = format!("{}/3", self.built());
        let crew = if self.crew { "9" } else { "0" };
        let cell = area.w / 4.;
        let cy = area.y + 44.;
        let column = |i: usize, glyph: Option<Icon>, value: &str, color: Color| {
            let cx = area.x + cell * (i as f32 + 0.5);
            shapes::fill_circle(vec2(cx, cy), 31., with_alpha(color, 0.13));
            shapes::stroke_circle(vec2(cx, cy), 31., 1.5, with_alpha(color, 0.45));
            match glyph {
                Some(g) => icon(g, cx, cy, 32., color),
                None => gate_ring(vec2(cx, cy), 17., 6., self, None, 0.),
            }
            draw_text_styled(
                value,
                cx,
                cy + 66.,
                style(28., PAPER).with_align(TextAlign::Center),
            );
        };
        if won {
            column(0, Some(Icon::Hourglass), &watch, TIME);
            column(1, Some(Icon::Heart), &hull, HULL);
        } else if self.hull == 0 {
            column(0, Some(Icon::Heart), "0", RED);
            column(1, None, &built, CYAN);
        } else {
            column(0, Some(Icon::Hourglass), &watch, RED);
            column(1, None, &built, CYAN);
        }
        column(2, Some(Icon::Search), &mapped, CYAN);
        column(
            3,
            Some(Icon::People),
            crew,
            if self.crew { LANDMARK_COLOR[8] } else { MUTED },
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(game: &mut Game) {
        for tick in 0..20_000 {
            let input = game.pilot(tick);
            game.step(&input);
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
    }

    #[test]
    fn progress_counts_gate_parts_from_departure_to_the_gate() {
        let mut game = Game::new();
        assert_eq!(game.progress(), Some((1, 4)));
        assert_eq!(game.hud().stage, "Loom Dock");
        win(&mut game);
        assert_eq!(game.outcome(), Outcome::Won);
        assert_eq!(game.progress(), Some((4, 4)));
        let hud = game.hud();
        assert_eq!(hud.stage, "Loom Dock");
        assert!(hud.objective.split_whitespace().count() <= 5);
        let summary = game.summary();
        assert_eq!(summary.matches(". ").count(), 0, "one sentence: {summary}");
        assert!(summary.contains("4817"));
    }

    #[test]
    fn short_objectives_follow_the_next_step() {
        let mut game = Game::new();
        assert_eq!(game.hud().objective, "Navigator's key at Wayfarer");
        game.relics[0] = true;
        assert_eq!(game.hud().objective, "Forge the Beacon spindle");
        game.components = [true; 3];
        game.sector = 5;
        assert_eq!(game.hud().objective, "Return to Loom Dock");
        game.sector = 0;
        assert_eq!(game.hud().objective, "Open the gate");
        for goal in [
            "Singing lens at Hollow Choir",
            "Sentinel core at Silent Crown",
        ] {
            assert!(goal.split_whitespace().count() <= 5);
        }
    }

    #[test]
    fn accepted_work_is_felt_where_it_happens() {
        // A survey rings out from the star and sparkles.
        let mut game = Game::new();
        game.step(&Input {
            slot: Some(2),
            ..Input::default()
        });
        let cue = game.step(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(cue, Cue::Good);
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        let counts = fx.counts();
        assert!(counts.rings >= 3 && counts.sparks > 0, "{counts:?}");
        // Mining bursts in the ore's color and pops the haul beside its row.
        let mut game = Game::new();
        game.sector = 1;
        game.sites[1].scanned = true;
        game.page = 1;
        game.row = 1;
        let cue = game.step(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(cue, Cue::Good);
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        let counts = fx.counts();
        assert!(
            counts.sparks >= 18 && counts.popups == 1 && counts.rings >= 3,
            "{counts:?}"
        );
        // Recovering a relic earns a callout.
        let mut game = Game::new();
        game.sector = 3;
        game.sites[3].scanned = true;
        game.page = 1;
        game.row = 4;
        let cue = game.step(&Input {
            primary: true,
            ..Input::default()
        });
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        assert!(fx.counts().popups >= 2, "{:?}", fx.counts());
    }

    #[test]
    fn refusals_mark_the_refusing_thing_and_damage_shakes() {
        let mut game = Game::new();
        game.page = 1;
        game.row = 1;
        let cue = game.step(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(cue, Cue::None);
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        let counts = fx.counts();
        assert_eq!((counts.rings, counts.popups), (1, 1));
        fx.update(1. / 60.);
        assert_ne!(fx.shake_offset(), Vec2::ZERO);
        // First entry into Wayfarer costs hull: a burst, a heart popup, shake.
        let mut game = Game::new();
        game.sector = 1;
        game.selected = 3;
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        let mut cue = Cue::None;
        for _ in 0..120 {
            cue = game.step(&Input::default());
        }
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        let counts = fx.counts();
        assert!(counts.sparks >= 16 && counts.popups == 1, "{counts:?}");
        fx.update(1. / 60.);
        assert_ne!(fx.shake_offset(), Vec2::ZERO);
    }

    #[test]
    fn routes_follow_the_charted_lanes() {
        assert_eq!(route(0, 0).1, 0);
        let (path, n) = route(0, 2);
        assert_eq!(&path[..n], &[2]);
        let (path, n) = route(1, 2);
        assert_eq!(&path[..n], &[0, 2]);
        let (path, n) = route(0, 8);
        assert_eq!(n, 3);
        assert_eq!(path[n - 1], 8);
        let mut at = 0;
        for &next in &path[..n] {
            assert!(Game::adjacent(at, next));
            at = next;
        }
    }
}
