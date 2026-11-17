//! Effects for accepted (and refused) commands, placed where they happened.
use crate::content::*;
use crate::layout::*;
use crate::look::RED;
use crate::look::*;
use crate::model::{Encounter, Event, Game};
use ferravex_arcade::*;

impl Game {
    // -----------------------------------------------------------------------
    // Effects

    /// Where things at `sector` happen on the current panel.
    pub(crate) fn site(&self, sector: usize) -> Vec2 {
        match self.page {
            0 | 1 => star(sector),
            3 => SHIP_AT,
            4 => MISSION_GATE,
            _ => FORGE_GATE,
        }
    }
    /// The thing the player just tried to use.
    pub(crate) fn focus_anchor(&self) -> Vec2 {
        if self.encounter.is_some() {
            return choice_center(self.choice) + vec2(230., 0.);
        }
        match self.page {
            0 => star(self.selected),
            1..=3 => row_center(self.page, self.row) + vec2(70., 0.),
            _ => button_center(),
        }
    }
    pub(crate) fn effect(&self, event: Event, fx: &mut Juice) {
        let spend = |fx: &mut Juice, stat: Stat, glyph: Icon, text: &str, color: Color| {
            fx.popup_icon(stat_anchor(stat) + vec2(6., 0.), glyph, text, color);
        };
        let watch = |fx: &mut Juice, n: &str| {
            // Every accepted action spends a watch: a pulse on the dial says
            // so without a popup each time; only the tow's six get a number.
            if n == "-1" {
                fx.ring(stat_anchor(Stat::Watch), TIME, 20.);
            } else {
                spend(fx, Stat::Watch, Icon::Hourglass, n, TIME);
            }
        };
        match event {
            Event::Launch { from, .. } => {
                let at = self.site(from);
                fx.ring(at, CYAN, 44.);
                fx.burst(at, CYAN, 12);
                spend(fx, Stat::Fuel, Icon::Fuel, "-1", FUEL);
                watch(fx, "-1");
            }
            Event::Arrive { sector, first } => {
                let at = self.site(sector);
                let color = tint(CHART[sector].color, 1.15);
                fx.ring(at, color, 50.);
                if first {
                    fx.ring(at, with_alpha(PAPER, 0.6), 30.);
                    fx.sparkle(at, lighten(color, 0.3));
                }
            }
            Event::Damage { sector, amount } => {
                let at = self.site(sector);
                let text = format!("-{amount}");
                fx.burst(at, Color::new(1., 0.45, 0.35, 1.), 16);
                fx.popup_icon(above(at), Icon::Heart, &text, HULL);
                fx.ring(stat_anchor(Stat::Hull), HULL, 18.);
                fx.shake((0.16 + f32::from(amount) / 60.).min(0.6));
                if self.hull < 30 {
                    fx.vignette(Color::new(0.9, 0.18, 0.16, 0.3));
                }
            }
            Event::Survey { sector } => {
                let at = self.site(sector);
                fx.ring(at, CYAN, 76.);
                fx.ring(at, with_alpha(CYAN, 0.7), 44.);
                fx.sparkle(at, CYAN);
                if self.page == 1 {
                    fx.ring(row_center(1, 0) - vec2(120., 0.), CYAN, 22.);
                }
                watch(fx, "-1");
            }
            Event::Extract {
                sector,
                kind,
                amount,
            } => {
                let at = self.site(sector);
                let text = format!("+{amount}");
                fx.burst(at, RESOURCE[kind], 18);
                fx.ring(at, RESOURCE[kind], 34.);
                let row = row_center(1, kind + 1);
                fx.popup_icon(
                    vec2(COL + COL_W - 40., row.y - 4.),
                    RESOURCE_ICON[kind],
                    &text,
                    RESOURCE[kind],
                );
                fx.ring(stat_anchor(Stat::Cargo(kind)), RESOURCE[kind], 16.);
                watch(fx, "-1");
            }
            Event::Landmark { sector, gain } => {
                let at = self.site(sector);
                let lift = above(at);
                fx.sparkle(at, GOLD);
                fx.ring(at, LANDMARK_COLOR[sector], 58.);
                fx.burst(at, LANDMARK_COLOR[sector], 20);
                let gained = format!("+{gain}");
                match sector {
                    1 => fx.popup_icon(lift, RESOURCE_ICON[0], "+2", RESOURCE[0]),
                    2 => fx.popup_icon(lift, Icon::Fuel, &gained, FUEL),
                    3 => {
                        fx.callout(callout_at(at), RELIC_NAMES[0], GOLD);
                        fx.popup_icon(lift, Icon::Coin, "+20", COIN);
                    }
                    5 => {
                        fx.popup_icon(lift, RESOURCE_ICON[1], "+2", RESOURCE[1]);
                        fx.popup_icon(lift + vec2(0., 18.), Icon::Coin, "+15", COIN);
                    }
                    _ => {
                        fx.callout(callout_at(at), "Rescued", LANDMARK_COLOR[8]);
                        fx.popup_icon(lift, Icon::People, "+9", LANDMARK_COLOR[8]);
                        if gain > 0 {
                            fx.popup_icon(lift + vec2(0., 18.), Icon::Heart, &gained, HULL);
                        }
                    }
                }
                watch(fx, "-1");
            }
            Event::Encounter { .. } => {
                fx.ring(PRIZE_AT, GOLD, 70.);
                fx.sparkle(PRIZE_AT, GOLD);
                fx.flash(Color::new(1., 0.85, 0.5, 0.08));
            }
            Event::Resolve {
                sector,
                encounter,
                choice,
                bonus,
            } => {
                let at = self.site(sector);
                let lift = above(at);
                fx.ring(at, GOLD, 60.);
                match encounter {
                    Encounter::Choir | Encounter::Sentinel => {
                        let k = if encounter == Encounter::Choir { 1 } else { 2 };
                        fx.sparkle(at, GOLD);
                        fx.burst(at, GOLD, 24);
                        fx.callout(callout_at(at), RELIC_NAMES[k], GOLD);
                        if bonus {
                            fx.popup_icon(lift, RESOURCE_ICON[1], "+1", RESOURCE[1]);
                        }
                    }
                    Encounter::Privateers => match choice {
                        0 => fx.popup_icon(lift, RESOURCE_ICON[2], "+3", RESOURCE[2]),
                        1 => fx.burst(at, FUEL, 14),
                        _ => {
                            fx.burst(at, COIN, 18);
                            fx.popup_icon(lift, Icon::Coin, "+40", COIN);
                        }
                    },
                }
                match (encounter, choice) {
                    (Encounter::Choir, 0) => spend(fx, Stat::Fuel, Icon::Fuel, "-1", FUEL),
                    (Encounter::Sentinel, 0) => {
                        spend(fx, Stat::Cargo(2), RESOURCE_ICON[2], "-2", RESOURCE[2])
                    }
                    (Encounter::Sentinel, 1) => spend(fx, Stat::Credits, Icon::Coin, "-30", COIN),
                    (Encounter::Privateers, 0) => {
                        spend(fx, Stat::Cargo(0), RESOURCE_ICON[0], "-1", RESOURCE[0])
                    }
                    (Encounter::Privateers, 1) => spend(fx, Stat::Fuel, Icon::Fuel, "-2", FUEL),
                    _ => {}
                }
                watch(fx, "-1");
            }
            Event::Withdraw { sector } => {
                fx.ring(self.site(sector), with_alpha(MUTED, 0.8), 34.);
            }
            Event::Forge { recipe } => {
                let at = if recipe < 3 {
                    gate_point(FORGE_GATE, 84., recipe)
                } else {
                    BAY[recipe - 3]
                };
                let at = if self.page == 2 {
                    at
                } else {
                    self.focus_anchor()
                };
                fx.sparkle(at, CYAN);
                fx.ring(at, CYAN, 48.);
                fx.burst(at, GOLD, 22);
                if recipe < 3 {
                    fx.callout(vec2(300., 250.), COMPONENT_NAMES[recipe], CYAN);
                } else {
                    fx.popup_icon(at - vec2(0., 30.), UPGRADE_ICON[recipe - 3], "", CYAN);
                }
                let cost = RECIPES[recipe].cost;
                for k in 0..3 {
                    if cost[k] > 0 {
                        let text = format!("-{}", cost[k]);
                        spend(fx, Stat::Cargo(k), RESOURCE_ICON[k], &text, RESOURCE[k]);
                    }
                }
                watch(fx, "-1");
            }
            Event::Service { row, kind, gain } => {
                let at = if self.page == 3 {
                    row_center(3, row) + vec2(100., 0.)
                } else {
                    self.focus_anchor()
                };
                let gained = format!("+{gain}");
                let (glyph, color) = match row {
                    0 | 2 => (Icon::Fuel, FUEL),
                    1 | 3 => (Icon::Heart, HULL),
                    _ => (Icon::Coin, COIN),
                };
                fx.popup_icon(at - vec2(0., 8.), glyph, &gained, color);
                fx.burst(at, color, 12);
                match row {
                    0 => spend(fx, Stat::Cargo(1), RESOURCE_ICON[1], "-1", RESOURCE[1]),
                    1 => {
                        spend(fx, Stat::Cargo(0), RESOURCE_ICON[0], "-1", RESOURCE[0]);
                        spend(fx, Stat::Cargo(2), RESOURCE_ICON[2], "-1", RESOURCE[2]);
                    }
                    2 => spend(fx, Stat::Credits, Icon::Coin, "-15", COIN),
                    3 => spend(fx, Stat::Credits, Icon::Coin, "-20", COIN),
                    _ => spend(
                        fx,
                        Stat::Cargo(kind),
                        RESOURCE_ICON[kind],
                        "-1",
                        RESOURCE[kind],
                    ),
                }
                watch(fx, "-1");
            }
            Event::Tow { .. } => {
                let at = if self.page == 3 {
                    row_center(3, 5) + vec2(100., 0.)
                } else {
                    self.focus_anchor()
                };
                fx.flash(Color::new(1., 0.9, 0.6, 0.18));
                fx.shake(0.3);
                fx.ring(at, GOLD, 60.);
                fx.popup_icon(at - vec2(0., 8.), Icon::Home, "", CYAN);
                watch(fx, "-6");
            }
            Event::Jettison { kind } => {
                let at = row_center(1, kind + 1) + vec2(60., 0.);
                fx.burst(at, with_alpha(RESOURCE[kind], 0.7), 8);
                fx.popup_icon(
                    at - vec2(0., 8.),
                    RESOURCE_ICON[kind],
                    "-1",
                    with_alpha(SOFT, 0.9),
                );
            }
            Event::Gate => {
                let at = if self.page == 4 {
                    MISSION_GATE
                } else {
                    star(0)
                };
                fx.ring(at, GOLD, 120.);
                fx.ring(at, CYAN, 170.);
                fx.burst(at, CYAN, 60);
                fx.sparkle(at, GOLD);
                fx.callout(at - vec2(0., 20.), "Gate open", GOLD);
                watch(fx, "-1");
            }
            Event::Collapse => {
                fx.flash(Color::new(1., 0.45, 0.2, 0.35));
                fx.callout(vec2(318., 300.), "Collapse", Color::new(1., 0.5, 0.3, 1.));
            }
            Event::Breach => {
                let at = self.site(self.sector);
                fx.burst(at, Color::new(1., 0.5, 0.3, 1.), 48);
                fx.ring(at, RED, 90.);
                fx.shake(0.7);
                fx.callout(vec2(318., 300.), "Hull breach", RED);
            }
            Event::Refuse(reason) => {
                let at = self.focus_anchor();
                let (glyph, word) = self.refusal_label(reason, self.row);
                fx.ring(at, RED, 24.);
                fx.shake(0.1);
                fx.popup_icon(
                    at - vec2(0., 18.),
                    glyph,
                    &word,
                    Color::new(1., 0.62, 0.6, 1.),
                );
            }
        }
    }
}

/// Keep big callouts over the chart, clear of the tabs as they rise.
pub(crate) fn callout_at(at: Vec2) -> Vec2 {
    vec2(at.x.clamp(150., 486.), (at.y - 50.).clamp(240., 520.))
}
/// Where a popup above `at` starts, clear of the tabs as it rises.
pub(crate) fn above(at: Vec2) -> Vec2 {
    vec2(at.x, (at.y - 32.).max(MAP.y + 34.))
}
