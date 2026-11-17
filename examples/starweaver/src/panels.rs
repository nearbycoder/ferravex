//! The Survey, Forge, Ship and Mission panels, lane transit and encounters.
use crate::content::*;
use crate::layout::*;
use crate::look::*;
use crate::look::{GREEN, RED};
use crate::model::{Encounter, Game, Refusal};
use crate::view::Goal;
use ferravex_arcade::shapes::{self, Cap, Stroke};
use ferravex_arcade::typography::{TextAlign, draw_text_styled};
use ferravex_arcade::*;
use std::f32::consts::TAU;

impl Game {
    // -----------------------------------------------------------------------
    // Survey

    pub(crate) fn panel_survey(&self, t: f32, p: Vec2, live: bool) {
        let i = self.sector;
        let s = &CHART[i];
        let site = self.sites[i];
        self.sector_title(i);
        let hover = if live { row_at(1, 5, p) } else { None };
        let focus = hover.unwrap_or(self.row);
        // The hold, with the focused work's haul ghosted in.
        let ghost = match focus {
            1..=3 if self.work_refusal(focus).is_none() => {
                Some((focus - 1, self.extraction(focus - 1)))
            }
            4 if self.work_refusal(4).is_none() && (i == 1 || i == 5) => {
                Some((if i == 1 { 0 } else { 1 }, 2))
            }
            _ => None,
        };
        let full = self.hold_free() == 0;
        icon(
            Icon::Crate,
            COL + 9.,
            222.,
            18.,
            if full { RED } else { SOFT },
        );
        hold_slots(COL + 24., 215., COL_W - 24. - 50., 14., self, ghost, t);
        let used = format!("{}/{}", self.hold_used(), self.hold_max());
        draw_text_styled(
            &used,
            COL + COL_W,
            ink(14., if full { RED } else { PAPER })
                .with_align(TextAlign::Right)
                .centered_baseline(222.),
            ink(14., if full { RED } else { PAPER }).with_align(TextAlign::Right),
        );
        let counts = self.cargo.map(|n| n.to_string());
        tags(
            &[
                bit(RESOURCE_ICON[0], &counts[0], RESOURCE[0]),
                bit(RESOURCE_ICON[1], &counts[1], RESOURCE[1]),
                bit(RESOURCE_ICON[2], &counts[2], RESOURCE[2]),
            ],
            COL + 24.,
            248.,
            15.,
            16.,
        );
        let dig = self.shielded(s.extraction_damage);
        let dig_text = format!("-{dig}");
        for r in 0..5 {
            let y = row_top(1) + r as f32 * ROW_STEP;
            let refusal = self.work_refusal(r);
            let ok = refusal.is_none();
            let locked = refusal == Some(Refusal::Unsurveyed);
            let cy = row_frame(y, self.row == r, hover == Some(r), ok, t);
            let right = COL + COL_W - 10.;
            match r {
                0 => {
                    icon(Icon::Search, COL + 20., cy, 17., faded(CYAN, ok));
                    line("Survey", COL + 36., cy, 120., 14., faded(PAPER, ok));
                    if site.scanned {
                        icon(Icon::Check, right - 8., cy, 16., GREEN);
                    } else {
                        tags_right(&[bit(Icon::Hourglass, "1", TIME)], right, cy, 14., 0.);
                    }
                }
                1..=3 => {
                    let k = r - 1;
                    let left = site.stock[k];
                    icon(RESOURCE_ICON[k], COL + 20., cy, 17., faded(RESOURCE[k], ok));
                    line(RESOURCE_NAMES[k], COL + 36., cy, 64., 14., faded(PAPER, ok));
                    stock_pips(COL + 106., cy, left, s.supplies[k], RESOURCE[k], !locked);
                    if locked {
                        icon(Icon::Lock, right - 8., cy, 14., MUTED);
                        continue;
                    }
                    let mut edge = right;
                    if dig > 0 {
                        edge = tags_right(
                            &[bit(Icon::Heart, &dig_text, faded(HULL, ok))],
                            edge,
                            cy,
                            13.,
                            0.,
                        ) - 8.;
                    }
                    let gain = self.extraction(k);
                    let haul = format!("+{gain}");
                    let color = if gain > 0 {
                        RESOURCE[k]
                    } else if refusal == Some(Refusal::Space) {
                        RED
                    } else {
                        with_alpha(MUTED, 0.6)
                    };
                    draw_text_styled(
                        &haul,
                        edge,
                        ink(14., color)
                            .with_align(TextAlign::Right)
                            .centered_baseline(cy),
                        ink(14., color).with_align(TextAlign::Right),
                    );
                }
                _ => {
                    let claimed = site.claimed;
                    let color = if claimed { MUTED } else { LANDMARK_COLOR[i] };
                    icon(LANDMARK_ICON[i], COL + 20., cy, 17., faded(color, !locked));
                    line(LANDMARK_NAME[i], COL + 36., cy, 104., 14., faded(PAPER, ok));
                    if locked {
                        icon(Icon::Lock, right - 8., cy, 14., MUTED);
                    } else if claimed {
                        icon(Icon::Check, right - 8., cy, 16., GREEN);
                    } else {
                        let w = self.landmark_rewards(i, 0., -100., 14., true);
                        self.landmark_rewards(i, right - w, cy, 14., ok);
                    }
                }
            }
        }
        // Jettison: a key cap beside the cargo it would drop.
        if (1..=3).contains(&focus) && self.cargo[focus - 1] > 0 {
            let k = focus - 1;
            let cy = 468.;
            mini_key("X", COL + 10., cy, 0.9);
            tag(
                RESOURCE_ICON[k],
                "-1",
                COL + 24.,
                cy,
                15.,
                with_alpha(RESOURCE[k], 0.8),
            );
        }
        if self.transit.is_some() {
            return;
        }
        let r = self.row;
        let (glyph, verb) = match r {
            0 => (Icon::Search, "Survey"),
            1 | 2 => (Icon::Pickaxe, "Mine"),
            3 => (Icon::Wrench, "Salvage"),
            _ => (LANDMARK_ICON[i], "Explore"),
        };
        match self.work_refusal(r) {
            Some(reason) => {
                let why = self.refusal_label(reason, r);
                button(glyph, verb, &[], Some(why), live, p, t);
            }
            None => {
                let mut bits = [
                    bit(Icon::Hourglass, "-1", TIME),
                    bit(Icon::Heart, &dig_text, HULL),
                ];
                let n = if (1..=3).contains(&r) && dig > 0 {
                    2
                } else {
                    1
                };
                let encounter = r == 4 && matches!(i, 4 | 6 | 7);
                if encounter {
                    bits[0] = bit(Icon::Sword, "", GOLD);
                }
                button(glyph, verb, &bits[..n], None, live, p, t);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Forge

    pub(crate) fn forge_left(&self, t: f32, p: Vec2, live: bool) {
        let hover = if live { row_at(2, 6, p) } else { None };
        let focus = hover.unwrap_or(self.row);
        // Blueprint grid behind the schematics.
        for k in 0..18 {
            let x = 40. + k as f32 * 32.;
            draw_line(x, 166., x, 430., 1., Color::new(0.3, 0.6, 0.8, 0.06));
        }
        for k in 0..9 {
            let y = 170. + k as f32 * 32.;
            draw_line(40., y, 596., y, 1., Color::new(0.3, 0.6, 0.8, 0.06));
        }
        // The gate: built arcs glow, the focused part pulses where it goes.
        let c = FORGE_GATE;
        let complete = self.built() == 3;
        let swirl = t * if complete { 1.2 } else { 0.25 };
        for k in 0..3 {
            let a = swirl + k as f32 * TAU / 3.;
            shapes::stroke_arc(
                c,
                40. + k as f32 * 9.,
                a,
                1.6,
                Stroke::new(2., with_alpha(CYAN, if complete { 0.5 } else { 0.12 }))
                    .cap(Cap::Round),
            );
        }
        gate_ring(c, 84., 16., self, (focus < 3).then_some(focus), t);
        for (k, &glyph) in RELIC_ICON.iter().enumerate() {
            let at = gate_point(c, 84., k);
            let (bg, fg) = if self.components[k] {
                (darken(CYAN, 0.6), lighten(CYAN, 0.4))
            } else if self.relics[k] {
                (darken(GOLD, 0.62), GOLD)
            } else {
                (Color::new(0.09, 0.13, 0.19, 1.), with_alpha(MUTED, 0.8))
            };
            icon_badge(glyph, at.x, at.y, 30., bg, fg);
        }
        // The ship and its three upgrade bays.
        let ship_c = vec2(452., 292.);
        if self.upgrades[0] {
            shapes::stroke_circle(
                ship_c,
                70.,
                2.,
                with_alpha(CYAN, 0.35 + 0.2 * pulse(t, 0.5)),
            );
            shapes::fill_circle(ship_c, 70., with_alpha(CYAN, 0.05));
        }
        ship(ship_c.x + 6., ship_c.y, 2.4, false);
        for (k, &at) in BAY.iter().enumerate() {
            let built = self.upgrades[k];
            let focused = focus == k + 3 && !built;
            if focused {
                let beat = pulse(t, 1.4);
                glow(at.x, at.y, 26. + 6. * beat, with_alpha(GOLD, 0.2));
                shapes::stroke_circle(at, 19. + 2. * beat, 1.5, with_alpha(GOLD, 0.6));
            }
            let (bg, fg) = if built {
                (darken(CYAN, 0.6), lighten(CYAN, 0.4))
            } else {
                (Color::new(0.09, 0.13, 0.19, 1.), with_alpha(MUTED, 0.8))
            };
            icon_badge(UPGRADE_ICON[k], at.x, at.y, 32., bg, fg);
        }
        // The focused recipe: requirement, cost against the hold, effect.
        let r = focus;
        let recipe = &RECIPES[r];
        let y = 446.;
        rounded(40., y, 556., 106., 12., WELL);
        let head = vec2(70., y + 28.);
        if r < 3 {
            gate_piece(head, 30., r, CYAN);
        } else {
            icon(UPGRADE_ICON[r - 3], head.x, head.y, 26., CYAN);
        }
        line(recipe.name, 96., y + 28., 230., 19., PAPER);
        let done = if r < 3 {
            self.components[r]
        } else {
            self.upgrades[r - 3]
        };
        if r < 3 {
            let held = self.relics[r];
            let x = 360.;
            tag(
                RELIC_ICON[r],
                "",
                x,
                y + 28.,
                20.,
                if held { GOLD } else { MUTED },
            );
            icon(
                if held { Icon::Check } else { Icon::Cross },
                x + 30.,
                y + 28.,
                14.,
                if held { GREEN } else { RED },
            );
            if !held {
                let name = CHART[RELIC_SECTORS[r]].name;
                tag(Icon::Pin, name, x + 46., y + 28., 15., MUTED);
            }
        }
        let cy = y + 74.;
        if done {
            icon_chip(Icon::Check, "Built", 60., cy - 10., 14., GREEN);
            return;
        }
        let have: [String; 3] =
            std::array::from_fn(|k| format!("{}/{}", self.cargo[k], recipe.cost[k]));
        let mut x = 60.;
        for k in 0..3 {
            if recipe.cost[k] == 0 {
                continue;
            }
            let color = if self.cargo[k] >= recipe.cost[k] {
                RESOURCE[k]
            } else {
                RED
            };
            x += tag(RESOURCE_ICON[k], &have[k], x, cy, 18., color) + 16.;
        }
        icon(Icon::ArrowRight, x + 4., cy, 14., MUTED);
        x += 22.;
        match r {
            0..=2 => {
                gate_piece(vec2(x + 10., cy), 22., r, CYAN);
                x += 30.;
            }
            3 => {
                x += tag(Icon::Heart, "x1/2", x, cy, 18., HULL) + 14.;
            }
            4 => {
                x += tag(Icon::Crate, "12", x, cy, 18., SOFT) + 6.;
                icon(Icon::ArrowRight, x + 5., cy, 11., MUTED);
                x += tag(Icon::Crate, "18", x + 14., cy, 18., CYAN) + 28.;
            }
            _ => {
                x += tag(Icon::Pickaxe, "2", x, cy, 18., SOFT) + 6.;
                icon(Icon::ArrowRight, x + 5., cy, 11., MUTED);
                x += tag(Icon::Pickaxe, "3", x + 14., cy, 18., CYAN) + 28.;
            }
        }
        tag(Icon::Hourglass, "-1", x + 6., cy, 16., TIME);
    }
    pub(crate) fn panel_forge(&self, t: f32, p: Vec2, live: bool) {
        let hover = if live { row_at(2, 6, p) } else { None };
        // What the hold can spend.
        let counts = self.cargo.map(|n| n.to_string());
        icon(Icon::Crate, COL + 9., 192., 18., SOFT);
        tags(
            &[
                bit(RESOURCE_ICON[0], &counts[0], RESOURCE[0]),
                bit(RESOURCE_ICON[1], &counts[1], RESOURCE[1]),
                bit(RESOURCE_ICON[2], &counts[2], RESOURCE[2]),
            ],
            COL + 30.,
            192.,
            17.,
            18.,
        );
        for r in 0..RECIPES.len() {
            let y = row_top(2) + r as f32 * ROW_STEP;
            let refusal = self.forge_refusal(r);
            let ok = refusal.is_none();
            let cy = row_frame(y, self.row == r, hover == Some(r), ok, t);
            let right = COL + COL_W - 10.;
            let recipe = &RECIPES[r];
            let color = match refusal {
                Some(Refusal::Done) => GREEN,
                Some(Refusal::Relic(_)) => MUTED,
                _ => CYAN,
            };
            if r < 3 {
                gate_piece(vec2(COL + 20., cy), 22., r, color);
            } else {
                icon(UPGRADE_ICON[r - 3], COL + 20., cy, 17., color);
            }
            line(
                recipe.name,
                COL + 36.,
                cy,
                112.,
                14.,
                faded(
                    PAPER,
                    refusal != Some(Refusal::Done) && refusal != Some(Refusal::Relic(r)),
                ),
            );
            match refusal {
                Some(Refusal::Done) => icon(Icon::Check, right - 8., cy, 16., GREEN),
                Some(Refusal::Relic(k)) => {
                    icon(Icon::Lock, right - 8., cy, 13., MUTED);
                    icon(RELIC_ICON[k], right - 26., cy, 15., MUTED);
                }
                _ => {
                    let need = recipe.cost.map(|n| n.to_string());
                    let mut bits = [bit(Icon::Gear, "", PAPER); 3];
                    let mut n = 0;
                    for k in 0..3 {
                        if recipe.cost[k] > 0 {
                            let color = if self.cargo[k] >= recipe.cost[k] {
                                RESOURCE[k]
                            } else {
                                RED
                            };
                            bits[n] = bit(RESOURCE_ICON[k], &need[k], color);
                            n += 1;
                        }
                    }
                    tags_right(&bits[..n], right, cy, 14., 9.);
                }
            }
        }
        let r = self.row;
        let glyph = Icon::Hammer;
        match self.forge_refusal(r) {
            Some(reason) => {
                let why = self.refusal_label(reason, r);
                button(glyph, "Build", &[], Some(why), live, p, t);
            }
            None => button(
                glyph,
                "Build",
                &[bit(Icon::Hourglass, "-1", TIME)],
                None,
                live,
                p,
                t,
            ),
        }
    }

    // -----------------------------------------------------------------------
    // Ship

    pub(crate) fn ship_left(&self, t: f32) {
        line("The Little Light", 44., 180., 300., 20., CYAN);
        for (k, &glyph) in UPGRADE_ICON.iter().enumerate() {
            let at = vec2(526. + k as f32 * 30., 180.);
            let built = self.upgrades[k];
            let (bg, fg) = if built {
                (darken(CYAN, 0.6), lighten(CYAN, 0.4))
            } else {
                (Color::new(0.09, 0.13, 0.19, 1.), with_alpha(MUTED, 0.6))
            };
            icon_badge(glyph, at.x, at.y, 24., bg, fg);
        }
        let c = SHIP_AT + vec2(0., bob(t, 0.4, 3.));
        if self.upgrades[0] {
            shapes::fill_circle(c, 80., with_alpha(CYAN, 0.04));
            shapes::stroke_circle(c, 80., 2., with_alpha(CYAN, 0.3 + 0.2 * pulse(t, 0.5)));
        }
        halo(c - vec2(112., 0.), 34., CYAN);
        ship(c.x, c.y, 4.1, true);
        // Damage shows on the hull: flickering scars grow with missing hull.
        let missing = 100 - self.hull.min(100);
        const SCARS: [(f32, f32); 6] = [
            (-30., 18.),
            (10., 22.),
            (-48., -26.),
            (30., -8.),
            (-12., -30.),
            (-58., 30.),
        ];
        for (k, &(dx, dy)) in SCARS.iter().take(usize::from(missing / 15)).enumerate() {
            let at = c + vec2(dx, dy);
            let flick = pulse(t + k as f32 * 0.31, 2.3 + k as f32 * 0.4);
            draw_line(
                at.x - 5.,
                at.y - 2.,
                at.x + 4.,
                at.y + 3.,
                2.,
                Color::new(0.2, 0.08, 0.08, 0.8),
            );
            glow(
                at.x,
                at.y,
                6. + 4. * flick,
                with_alpha(Color::new(1., 0.55, 0.25, 1.), 0.25 * flick),
            );
        }
        let right = |text: &str, x: f32, cy: f32, size: f32, color: Color| {
            let s = ink(size, color).with_align(TextAlign::Right);
            draw_text_styled(text, x, s.centered_baseline(cy), s);
        };
        // Hull and fuel gauges.
        let cy = 376.;
        rounded(40., cy - 24., 272., 48., 10., WELL);
        rounded(324., cy - 24., 272., 48., 10., WELL);
        let hull_color = if self.hull < 25 {
            RED
        } else if self.hull < 60 {
            GOLD
        } else {
            GREEN
        };
        icon(Icon::Heart, 62., cy, 20., HULL);
        meter(
            82.,
            cy - 5.,
            178.,
            10.,
            f32::from(self.hull) / 100.,
            hull_color,
        );
        right(&self.hull.to_string(), 300., cy, 17., PAPER);
        icon(Icon::Fuel, 346., cy, 20., FUEL);
        for n in 0..self.fuel_max() {
            let x = 366. + f32::from(n) * 19.;
            if n < self.fuel {
                rounded_gradient(x, cy - 7., 15., 14., 3., lighten(FUEL, 0.2), FUEL);
            } else {
                rounded(x, cy - 7., 15., 14., 3., Color::new(0.13, 0.18, 0.25, 0.55));
                outline(x, cy - 7., 15., 14., 3., 1., Color::new(1., 1., 1., 0.07));
            }
        }
        right(
            &self.fuel.to_string(),
            584.,
            cy,
            17.,
            if self.fuel == 0 { RED } else { PAPER },
        );
        // Credits and the one-use distress tow.
        let cy = 432.;
        rounded(40., cy - 24., 272., 48., 10., WELL);
        rounded(324., cy - 24., 272., 48., 10., WELL);
        tag(Icon::Coin, &self.credits.to_string(), 52., cy, 20., COIN);
        let tow = if self.tow {
            GOLD
        } else {
            with_alpha(MUTED, 0.7)
        };
        icon(Icon::Bell, 346., cy, 20., tow);
        tags(
            &[
                bit(Icon::Hourglass, "6", faded(TIME, self.tow)),
                bit(Icon::Heart, "15", faded(HULL, self.tow)),
            ],
            366.,
            cy,
            15.,
            10.,
        );
        icon(Icon::ArrowRight, 470., cy, 11., with_alpha(MUTED, 0.8));
        icon(Icon::Home, 490., cy, 16., faded(CYAN, self.tow));
        icon(
            if self.tow { Icon::Check } else { Icon::Cross },
            580.,
            cy,
            15.,
            if self.tow { GREEN } else { RED },
        );
        // The cargo bay.
        let y = 464.;
        rounded(40., y, 556., 88., 10., WELL);
        let full = self.hold_free() == 0;
        icon(
            Icon::Crate,
            62.,
            y + 26.,
            20.,
            if full { RED } else { SOFT },
        );
        hold_slots(82., y + 17., 440., 18., self, None, t);
        let used = format!("{}/{}", self.hold_used(), self.hold_max());
        right(&used, 584., y + 26., 16., if full { RED } else { PAPER });
        let counts = self.cargo.map(|n| n.to_string());
        tags(
            &[
                bit(RESOURCE_ICON[0], &counts[0], RESOURCE[0]),
                bit(RESOURCE_ICON[1], &counts[1], RESOURCE[1]),
                bit(RESOURCE_ICON[2], &counts[2], RESOURCE[2]),
            ],
            82.,
            y + 62.,
            17.,
            24.,
        );
    }
    /// Costs, an arrow and the predicted gain of ship service `row`.
    pub(crate) fn service_terms(&self, row: usize, right: f32, cy: f32, on: bool) {
        let f = |c: Color| faded(c, on);
        let pay = |ok: bool, c: Color| if ok { f(c) } else { RED };
        let fuel_gain = |n: u16| (self.fuel + n).min(self.fuel_max()) - self.fuel;
        let hull_gain = |n: u16| (self.hull + n).min(100) - self.hull;
        let price = SELL_PRICE[self.sell_kind];
        let (gain_icon, gain, gain_color) = match row {
            0 => (Icon::Fuel, fuel_gain(3), FUEL),
            1 => (Icon::Heart, hull_gain(25), HULL),
            2 => (Icon::Fuel, fuel_gain(5), FUEL),
            3 => (Icon::Heart, hull_gain(45), HULL),
            4 => (Icon::Coin, price, COIN),
            _ => (Icon::Home, 0, CYAN),
        };
        let gain_text = if row == 5 {
            String::new()
        } else {
            format!("+{gain}")
        };
        let gain_color = if row != 5 && gain == 0 {
            with_alpha(MUTED, 0.6)
        } else {
            f(gain_color)
        };
        let left = tags_right(
            &[bit(gain_icon, &gain_text, gain_color)],
            right,
            cy,
            14.,
            0.,
        );
        icon(Icon::ArrowRight, left - 9., cy, 10., with_alpha(MUTED, 0.8));
        let edge = left - 17.;
        let hurt = format!("{}", self.shielded(15));
        match row {
            0 => tags_right(
                &[bit(
                    RESOURCE_ICON[1],
                    "1",
                    pay(self.cargo[1] > 0, RESOURCE[1]),
                )],
                edge,
                cy,
                14.,
                0.,
            ),
            1 => tags_right(
                &[
                    bit(RESOURCE_ICON[0], "1", pay(self.cargo[0] > 0, RESOURCE[0])),
                    bit(RESOURCE_ICON[2], "1", pay(self.cargo[2] > 0, RESOURCE[2])),
                ],
                edge,
                cy,
                14.,
                6.,
            ),
            2 => tags_right(
                &[bit(Icon::Coin, "15", pay(self.credits >= 15, COIN))],
                edge,
                cy,
                14.,
                0.,
            ),
            3 => tags_right(
                &[bit(Icon::Coin, "20", pay(self.credits >= 20, COIN))],
                edge,
                cy,
                14.,
                0.,
            ),
            4 => tags_right(
                &[bit(
                    RESOURCE_ICON[self.sell_kind],
                    "1",
                    pay(self.cargo[self.sell_kind] > 0, RESOURCE[self.sell_kind]),
                )],
                edge,
                cy,
                14.,
                0.,
            ),
            _ => tags_right(
                &[
                    bit(Icon::Hourglass, "6", f(TIME)),
                    bit(Icon::Heart, &hurt, f(HULL)),
                ],
                edge,
                cy,
                14.,
                6.,
            ),
        };
    }
    pub(crate) fn panel_ship(&self, t: f32, p: Vec2, live: bool) {
        let hover = if live { row_at(3, 6, p) } else { None };
        let station = CHART[self.sector].station;
        // What X sells, and for how much.
        let cy = 192.;
        mini_key("X", COL + 9., cy, 0.9);
        for k in 0..3 {
            let x = COL + 26. + k as f32 * 86.;
            let price = SELL_PRICE[k].to_string();
            let on = k == self.sell_kind;
            if on {
                rounded(
                    x - 4.,
                    cy - 13.,
                    80.,
                    26.,
                    13.,
                    with_alpha(RESOURCE[k], 0.16),
                );
                outline(
                    x - 4.,
                    cy - 13.,
                    80.,
                    26.,
                    13.,
                    1.,
                    with_alpha(RESOURCE[k], 0.6),
                );
            }
            let w = tag(
                RESOURCE_ICON[k],
                "",
                x + 2.,
                cy,
                17.,
                faded(RESOURCE[k], on),
            );
            tag(Icon::Coin, &price, x + w + 8., cy, 14., faded(COIN, on));
        }
        for r in 0..6 {
            let y = row_top(3) + r as f32 * ROW_STEP;
            let refusal = self.service_refusal(r);
            let ok = refusal.is_none();
            let cy = row_frame(y, self.row == r, hover == Some(r), ok, t);
            let available = refusal != Some(Refusal::Station)
                && refusal != Some(Refusal::Done)
                && refusal != Some(Refusal::Docked);
            icon(SERVICE_ICON[r], COL + 20., cy, 17., faded(CYAN, available));
            let w = line(
                SERVICE_NAME[r],
                COL + 36.,
                cy,
                80.,
                14.,
                faded(PAPER, available),
            );
            if (2..=4).contains(&r) {
                icon(
                    Icon::Home,
                    COL + 44. + w,
                    cy,
                    12.,
                    if station { GOLD } else { with_alpha(RED, 0.8) },
                );
            }
            if r == 5 && !self.tow {
                icon(Icon::Cross, COL + COL_W - 18., cy, 13., MUTED);
                continue;
            }
            self.service_terms(r, COL + COL_W - 10., cy, available);
        }
        let r = self.row;
        let hours = if r == 5 { "-6" } else { "-1" };
        match self.service_refusal(r) {
            Some(reason) => {
                let why = self.refusal_label(reason, r);
                button(SERVICE_ICON[r], SERVICE_NAME[r], &[], Some(why), live, p, t);
            }
            None => button(
                SERVICE_ICON[r],
                SERVICE_NAME[r],
                &[bit(Icon::Hourglass, hours, TIME)],
                None,
                live,
                p,
                t,
            ),
        }
    }
}

impl Game {
    // -----------------------------------------------------------------------
    // Mission

    pub(crate) fn mission_left(&self, t: f32) {
        let c = MISSION_GATE;
        let complete = self.built() == 3;
        // The portal stirs as parts arrive and blazes once whole.
        let energy = self.built() as f32 / 3.;
        glow(c.x, c.y, 70., with_alpha(CYAN, 0.06 + 0.12 * energy));
        for k in 0..4 {
            let a = t * (0.2 + energy * 0.9) + k as f32 * TAU / 4.;
            shapes::stroke_arc(
                c,
                30. + k as f32 * 13.,
                a,
                1.4 + energy,
                Stroke::new(2., with_alpha(CYAN, 0.1 + 0.35 * energy)).cap(Cap::Round),
            );
        }
        if complete {
            ship(c.x - 6., c.y, 1.2, true);
        }
        let focus = match self.goal() {
            Goal::Forge(i) | Goal::Recover(i) => Some(i),
            _ => None,
        };
        gate_ring(c, 106., 20., self, focus, t);
        for (k, &glyph) in RELIC_ICON.iter().enumerate() {
            let at = gate_point(c, 106., k);
            let (bg, fg) = if self.components[k] {
                (darken(CYAN, 0.6), lighten(CYAN, 0.4))
            } else if self.relics[k] {
                (darken(GOLD, 0.62), GOLD)
            } else {
                (Color::new(0.09, 0.13, 0.19, 1.), with_alpha(MUTED, 0.85))
            };
            icon_badge(glyph, at.x, at.y, 36., bg, fg);
        }
        // One card per gate part: its relic, then what the part still needs.
        for k in 0..3 {
            let y = 172. + k as f32 * 68.;
            let built = self.components[k];
            let held = self.relics[k];
            rounded(338., y, 258., 58., 10., WELL);
            if built {
                outline(338., y, 258., 58., 10., 1., with_alpha(CYAN, 0.45));
            } else if focus == Some(k) {
                outline(
                    338.,
                    y,
                    258.,
                    58.,
                    10.,
                    1.,
                    with_alpha(GOLD, 0.35 + 0.3 * pulse(t, 0.8)),
                );
            }
            gate_piece(
                vec2(362., y + 29.),
                30.,
                k,
                if built { CYAN } else { MUTED },
            );
            line(
                COMPONENT_NAMES[k],
                386.,
                y + 19.,
                196.,
                15.,
                if built { PAPER } else { SOFT },
            );
            let cy = y + 41.;
            if built {
                tag(Icon::Check, "", 386., cy, 15., GREEN);
            } else if held {
                tag(RELIC_ICON[k], "", 386., cy, 15., GOLD);
                icon(Icon::ArrowRight, 412., cy, 10., MUTED);
                mini_key("3", 430., cy, 0.9);
                icon(Icon::Hammer, 450., cy, 15., GOLD);
            } else {
                tag(RELIC_ICON[k], "", 386., cy, 15., MUTED);
                tag(
                    Icon::Pin,
                    CHART[RELIC_SECTORS[k]].name,
                    410.,
                    cy,
                    14.,
                    MUTED,
                );
            }
        }
        // The optional rescue.
        let y = 384.;
        rounded(338., y, 258., 50., 10., WELL);
        let rescued = self.crew;
        let color = LANDMARK_COLOR[8];
        icon_badge(
            Icon::People,
            362.,
            y + 25.,
            30.,
            darken(color, 0.72),
            if rescued {
                color
            } else {
                with_alpha(color, 0.6)
            },
        );
        line(
            CHART[8].name,
            386.,
            y + 17.,
            150.,
            14.,
            if rescued { PAPER } else { SOFT },
        );
        if rescued {
            tag(Icon::Check, "9", 386., y + 36., 14., GREEN);
        } else {
            tag(Icon::Pin, "", 386., y + 36., 14., MUTED);
        }
        // Sectors surveyed.
        let cy = 470.;
        icon(Icon::Search, 352., cy, 18., CYAN);
        for (i, (site, sector)) in self.sites.iter().zip(&CHART).enumerate() {
            let at = vec2(374. + i as f32 * 16., cy);
            if site.scanned {
                shapes::fill_circle(at, 4.5, tint(sector.color, 1.05));
            } else {
                shapes::stroke_circle(at, 4., 1.5, with_alpha(MUTED, 0.5));
            }
        }
    }
    pub(crate) fn panel_mission(&self, t: f32, p: Vec2, live: bool) {
        let left = DEADLINE - self.watch.min(DEADLINE);
        let frac = f32::from(left) / f32::from(DEADLINE);
        let col = watch_color(left);
        let c = vec2(COL + COL_W * 0.5, 248.);
        glow(c.x, c.y, 80., with_alpha(col, 0.06));
        ring_meter(c.x, c.y, 62., 12., frac, col);
        icon(Icon::Hourglass, c.x, c.y - 22., 22., col);
        let value = left.to_string();
        draw_text_styled(
            &value,
            c.x,
            c.y + 26.,
            style(34., PAPER).with_align(TextAlign::Center),
        );
        let built = format!("{}/3", self.built());
        let mapped = format!("{}/9", self.scanned());
        let cy = 360.;
        gate_ring(vec2(COL + 22., cy), 11., 4.5, self, None, t);
        label(&built, COL + 40., cy + 6., 17., PAPER);
        tag(Icon::Search, &mapped, COL + 104., cy, 17., CYAN);
        tag(
            Icon::People,
            if self.crew { "9" } else { "0" },
            COL + 190.,
            cy,
            17.,
            if self.crew { LANDMARK_COLOR[8] } else { MUTED },
        );
        // What crossing on the next watch would add: escape sooner, score more.
        let ready = self.gate_refusal().is_none();
        let y = 400.;
        rounded(COL, y, COL_W, 92., 12., WELL);
        if ready {
            outline(
                COL,
                y,
                COL_W,
                92.,
                12.,
                1.,
                with_alpha(GOLD, 0.3 + 0.25 * pulse(t, 0.8)),
            );
        }
        let bonus = format!("+{}", self.escape_bonus(self.watch.saturating_add(1)));
        icon(Icon::Flag, COL + 24., y + 30., 20., faded(GOLD, ready));
        icon(
            Icon::ArrowRight,
            COL + 46.,
            y + 30.,
            12.,
            with_alpha(MUTED, 0.8),
        );
        icon(Icon::Star, COL + 68., y + 30., 22., faded(GOLD, ready));
        let s = style(26., faded(GOLD, ready));
        draw_text_styled(&bonus, COL + 84., s.centered_baseline(y + 30.), s);
        let watches = format!("{}", DEADLINE.saturating_sub(self.watch.saturating_add(1)));
        let hull = self.hull.to_string();
        let credits = self.credits.min(500).to_string();
        tags(
            &[
                bit(Icon::Hourglass, &watches, faded(TIME, ready)),
                bit(Icon::Heart, &hull, faded(HULL, ready)),
                bit(Icon::Coin, &credits, faded(COIN, ready)),
            ],
            COL + 22.,
            y + 68.,
            15.,
            18.,
        );
        match self.gate_refusal() {
            Some(reason) => {
                let why = self.refusal_label(reason, 0);
                button(Icon::Flag, "Open gate", &[], Some(why), live, p, t);
            }
            None => button(
                Icon::Flag,
                "Open gate",
                &[bit(Icon::Hourglass, "-1", TIME)],
                None,
                live,
                p,
                t,
            ),
        }
    }

    // -----------------------------------------------------------------------
    // Transit and encounters

    pub(crate) fn draw_transit(&self) {
        let Some(flight) = self.transit else {
            return;
        };
        let (x, y, w, h) = (COL - 4., 476., COL_W + 8., 82.);
        rounded_gradient(
            x,
            y,
            w,
            h,
            10.,
            Color::new(0.07, 0.16, 0.22, 1.),
            Color::new(0.04, 0.09, 0.14, 1.),
        );
        outline(x, y, w, h, 10., 1., with_alpha(CYAN, 0.35));
        let to = &CHART[flight.to];
        icon(Icon::Rocket, COL + 14., y + 24., 18., CYAN);
        line(
            to.name,
            COL + 30.,
            y + 24.,
            COL_W - 70.,
            16.,
            tint(to.color, 1.1),
        );
        let progress = 1. - f32::from(flight.remaining) / f32::from(flight.total);
        meter(COL + 8., y + 44., COL_W - 16., 8., progress, CYAN);
        if !self.sites[flight.to].visited {
            let hurt = self.shielded(to.arrival_damage);
            if hurt > 0 {
                let pain = format!("-{hurt}");
                tag(Icon::Alert, "", COL + 8., y + 66., 14., RED);
                tag(Icon::Heart, &pain, COL + 26., y + 66., 14., HULL);
            }
        }
    }
    /// The relevant stores and each approach's costs and rewards.
    pub(crate) fn approach_terms(&self, event: Encounter, i: usize, right: f32, cy: f32, on: bool) {
        let f = |c: Color| faded(c, on);
        let pay = |ok: bool, c: Color| if ok { f(c) } else { RED };
        let hurt = |raw: u16| {
            let n = self.shielded(raw);
            (format!("-{n}"), if n >= self.hull { RED } else { f(HULL) })
        };
        let relic = |k: usize| bit(RELIC_ICON[k], "", f(GOLD));
        let (pain, pain_color) = match (event, i) {
            (Encounter::Choir, 1) => hurt(18),
            (Encounter::Sentinel, 2) => hurt(34),
            (Encounter::Privateers, 2) => hurt(22),
            _ => (String::new(), HULL),
        };
        let watch = bit(Icon::Hourglass, "-1", f(TIME));
        let space = self.hold_free() >= 2;
        let (costs, gains): (&[Bit], &[Bit]) = match (event, i) {
            (Encounter::Choir, 0) => (
                &[bit(Icon::Fuel, "-1", pay(self.fuel >= 1, FUEL)), watch],
                &[relic(1)],
            ),
            (Encounter::Choir, 1) => (
                &[bit(Icon::Heart, &pain, pain_color), watch],
                &[
                    relic(1),
                    bit(
                        RESOURCE_ICON[1],
                        "+1",
                        faded(RESOURCE[1], on && self.hold_free() > 0),
                    ),
                ],
            ),
            (Encounter::Choir, _) => (&[bit(Icon::Hourglass, "0", f(TIME))], &[]),
            (Encounter::Sentinel, 0) => (
                &[
                    bit(RESOURCE_ICON[2], "-2", pay(self.cargo[2] >= 2, RESOURCE[2])),
                    watch,
                ],
                &[relic(2)],
            ),
            (Encounter::Sentinel, 1) => (
                &[bit(Icon::Coin, "-30", pay(self.credits >= 30, COIN)), watch],
                &[relic(2)],
            ),
            (Encounter::Sentinel, _) => {
                (&[bit(Icon::Heart, &pain, pain_color), watch], &[relic(2)])
            }
            (Encounter::Privateers, 0) => (
                &[
                    bit(RESOURCE_ICON[0], "-1", pay(self.cargo[0] >= 1, RESOURCE[0])),
                    bit(Icon::Crate, "2", pay(space, SOFT)),
                    watch,
                ],
                &[bit(RESOURCE_ICON[2], "+3", f(RESOURCE[2]))],
            ),
            (Encounter::Privateers, 1) => (
                &[bit(Icon::Fuel, "-2", pay(self.fuel >= 2, FUEL)), watch],
                &[],
            ),
            (Encounter::Privateers, _) => (
                &[bit(Icon::Heart, &pain, pain_color), watch],
                &[bit(Icon::Coin, "+40", f(COIN))],
            ),
        };
        let mut edge = right;
        if !gains.is_empty() {
            edge = pills_right(gains, edge, cy, 14., 6.);
            icon(
                Icon::ArrowRight,
                edge - 13.,
                cy,
                13.,
                with_alpha(MUTED, 0.9),
            );
            edge -= 26.;
        }
        pills_right(costs, edge, cy, 14., 6.);
    }
    pub(crate) fn draw_encounter(&self, event: Encounter, t: f32, p: Vec2) {
        panel(16., 112., 928., 456., Color::new(0.01, 0.02, 0.05, 0.62));
        let r = Rect::new(102., 170., 758., 384.);
        shapes::drop_shadow(
            r,
            14.,
            shapes::Shadow::soft(Color::new(0., 0., 0., 0.5))
                .offset(vec2(0., 8.))
                .blur(24.),
        );
        rounded_gradient(
            r.x,
            r.y,
            r.w,
            r.h,
            14.,
            Color::new(0.07, 0.11, 0.17, 1.),
            Color::new(0.03, 0.06, 0.1, 1.),
        );
        outline(r.x, r.y, r.w, r.h, 14., 1., with_alpha(GOLD, 0.35));
        rounded(r.x + 28., r.y, 72., 3., 1.5, GOLD);
        let (title, prize, prize_color) = match event {
            Encounter::Choir => ("The Hollow Choir", Icon::Music, GOLD),
            Encounter::Sentinel => ("The Sentinel's Toll", Icon::Battery, GOLD),
            Encounter::Privateers => ("Privateers of Copper Reef", Icon::Sword, LANDMARK_COLOR[6]),
        };
        label(title, 130., 226., 26., PAPER);
        // The prize, floating where it waits.
        let at = PRIZE_AT + vec2(0., bob(t, 0.5, 3.));
        glow(
            at.x,
            at.y,
            52.,
            with_alpha(prize_color, 0.16 + 0.08 * pulse(t, 0.6)),
        );
        icon(prize, at.x, at.y, 48., prize_color);
        // The stores these approaches draw on.
        let fuel = self.fuel.to_string();
        let hull = self.hull.to_string();
        let credits = self.credits.to_string();
        let free = self.hold_free().to_string();
        let alloy = self.cargo[0].to_string();
        let circuits = self.cargo[2].to_string();
        let stores: &[Bit] = match event {
            Encounter::Choir => &[
                bit(Icon::Fuel, &fuel, FUEL),
                bit(Icon::Heart, &hull, HULL),
                bit(Icon::Crate, &free, SOFT),
            ],
            Encounter::Sentinel => &[
                bit(RESOURCE_ICON[2], &circuits, RESOURCE[2]),
                bit(Icon::Coin, &credits, COIN),
                bit(Icon::Heart, &hull, HULL),
            ],
            Encounter::Privateers => &[
                bit(RESOURCE_ICON[0], &alloy, RESOURCE[0]),
                bit(Icon::Crate, &free, SOFT),
                bit(Icon::Fuel, &fuel, FUEL),
                bit(Icon::Heart, &hull, HULL),
            ],
        };
        tags(stores, 132., 270., 18., 20.);
        let hover = if self.result == Outcome::Playing {
            choice_at(p)
        } else {
            None
        };
        const APPROACH: [[(Icon, &str, Color); 3]; 3] = [
            [
                (Icon::Rocket, "Powered probe", FUEL),
                (Icon::ArrowDown, "Dive", HULL),
                (Icon::Undo, "Withdraw", SOFT),
            ],
            [
                (Icon::Unlock, "Override", RESOURCE[2]),
                (Icon::Coin, "Pay the toll", COIN),
                (Icon::Bomb, "Force the vault", HULL),
            ],
            [
                (Icon::Hand, "Trade", RESOURCE[0]),
                (Icon::Wind, "Escape", FUEL),
                (Icon::Sword, "Fight", HULL),
            ],
        ];
        let set = match event {
            Encounter::Choir => 0,
            Encounter::Sentinel => 1,
            Encounter::Privateers => 2,
        };
        for (i, &(glyph, name, tone)) in APPROACH[set].iter().enumerate() {
            let ok = self.approach_refusal(event, i).is_none();
            let on = self.choice == i;
            let hovered = hover == Some(i);
            let y = CHOICE_TOP + i as f32 * CHOICE_STEP - if hovered { 2. } else { 0. };
            if on {
                rounded_gradient(CHOICE_X, y, CHOICE_W, CHOICE_H, 10., lighten(DIM, 0.1), DIM);
                outline(
                    CHOICE_X,
                    y,
                    CHOICE_W,
                    CHOICE_H,
                    10.,
                    1.,
                    with_alpha(GOLD, 0.5 + 0.3 * pulse(t, 0.8)),
                );
            } else if hovered {
                rounded(
                    CHOICE_X,
                    y,
                    CHOICE_W,
                    CHOICE_H,
                    10.,
                    Color::new(0.09, 0.14, 0.21, 1.),
                );
                outline(
                    CHOICE_X,
                    y,
                    CHOICE_W,
                    CHOICE_H,
                    10.,
                    1.,
                    with_alpha(CYAN, 0.4),
                );
            } else {
                rounded(
                    CHOICE_X,
                    y,
                    CHOICE_W,
                    CHOICE_H,
                    10.,
                    Color::new(0.05, 0.08, 0.13, 1.),
                );
                outline(
                    CHOICE_X,
                    y,
                    CHOICE_W,
                    CHOICE_H,
                    10.,
                    1.,
                    Color::new(1., 1., 1., 0.06),
                );
            }
            let cy = y + CHOICE_H * 0.5;
            let accent = if ok { tone } else { with_alpha(MUTED, 0.6) };
            if on && ok {
                glow(158., cy, 24., with_alpha(GOLD, 0.2));
            }
            icon_badge(glyph, 158., cy, 32., darken(accent, 0.75), accent);
            line(
                name,
                184.,
                cy,
                230.,
                17.,
                if !ok {
                    MUTED
                } else if on {
                    GOLD
                } else {
                    PAPER
                },
            );
            self.approach_terms(event, i, CHOICE_X + CHOICE_W - 16., cy, ok);
        }
        let mut x = 130.;
        x += keycap("Up", "", x, 518., MUTED) + 4.;
        x += keycap("Down", "Choose", x, 518., MUTED) + 16.;
        keycap("Space", "Confirm", x, 518., MUTED);
    }
}
