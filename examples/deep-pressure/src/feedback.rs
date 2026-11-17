//! Accepted events made felt where they happen. Each effect comes from an
//! [`Event`] the latest tick recorded; nothing here reaches the rules.
use crate::{
    Game,
    art::*,
    content::HazardKind,
    model::{BALLAST_BUTTON, BASE, Cause, Event, SONAR_BUTTON},
    overlay::{MEDAL_AT, card_center, medal_name, refit_tone},
    panel::{AIR_DIAL, CABIN, HULL_DIAL, button_center, seat_center, tug_center},
    view::{SEA_Y, sea},
};
use ferravex_arcade::*;

const BUBBLE: Color = Color::new(0.75, 1., 1., 0.9);
const EMBER: Color = Color::new(1., 0.58, 0.26, 1.);
const ROCK: Color = Color::new(0.62, 0.74, 0.76, 1.);

impl Game {
    pub(crate) fn feedback(&self, _cue: Cue, fx: &mut Juice) {
        let boat = sea(self.position);
        let above = if boat.y - 32. < SEA_Y + 20. {
            boat + vec2(0., 34.)
        } else {
            boat - vec2(0., 32.)
        };
        for event in self.events.as_slice() {
            match *event {
                Event::Launch => {
                    fx.ring(boat, AQUA, 46.);
                    fx.burst(boat, BUBBLE, 18);
                }
                Event::Fitted { index, price } => {
                    let c = card_center(index);
                    let tone = refit_tone(index);
                    fx.burst(c, tone, 26);
                    fx.sparkle(c, tone);
                    fx.ring(c, tone, 72.);
                    fx.popup_icon(c - vec2(0., 40.), Icon::Coin, &format!("-{price}"), GOLD);
                    match index {
                        0 => fx.ring(AIR_DIAL, AQUA, 56.),
                        1 => fx.ring(HULL_DIAL, STEEL, 56.),
                        _ => fx.ring(CABIN.center(), AQUA, 90.),
                    }
                }
                Event::RefitRefused { index, maxed } => {
                    let c = card_center(index);
                    fx.ring(c, CORAL, 64.);
                    fx.shake(0.14);
                    if maxed {
                        fx.popup_icon(c - vec2(0., 40.), Icon::Check, "Max", MUTED);
                    } else {
                        let need = format!("Need {}", self.upgrade_cost(index));
                        fx.popup_icon(c - vec2(0., 40.), Icon::Coin, &need, CORAL);
                    }
                }
                Event::Winched { index } => {
                    let bell = sea(self.chart().divers[index].0);
                    fx.sparkle(bell, AQUA);
                    fx.burst(bell, BUBBLE, 18);
                    fx.ring(bell, AQUA, 38.);
                    fx.popup_icon(bell - vec2(0., 30.), Icon::Person, "+1", AQUA);
                    let seat = seat_center(self.onboard().saturating_sub(1), self.capacity());
                    fx.ring(seat, AQUA, 28.);
                    fx.sparkle(seat, AQUA);
                }
                Event::TooFast { at } => {
                    fx.ring(sea(at), AMBER, 36.);
                    fx.popup(above, "Slow down", AMBER);
                    fx.shake(0.1);
                }
                Event::CabinFull { at } => {
                    fx.ring(sea(at), CORAL, 36.);
                    fx.popup_icon(above, Icon::Person, "Full", CORAL);
                    fx.ring(CABIN.center(), CORAL, 96.);
                    fx.shake(0.1);
                }
                Event::Docked { landed, cleared } => {
                    let dock = sea(BASE);
                    if landed > 0 {
                        fx.burst(dock, GOLD, 24 + landed * 8);
                        fx.sparkle(dock, GOLD);
                        fx.ring(dock, GOLD, 60.);
                        fx.popup_icon(
                            dock + vec2(74., -4.),
                            Icon::Person,
                            &format!("+{landed}"),
                            GOLD,
                        );
                        fx.popup_icon(
                            vec2(760., 506.),
                            Icon::Coin,
                            &format!("+{}", landed * 12),
                            GOLD,
                        );
                    } else {
                        fx.ring(dock, AQUA, 44.);
                    }
                    fx.ring(AIR_DIAL, AQUA, 54.);
                    fx.ring(HULL_DIAL, STEEL, 54.);
                    if cleared {
                        let medal = self.medals[self.mission];
                        let metal = medal_metal(medal);
                        fx.burst(MEDAL_AT, metal, 48);
                        fx.ring(MEDAL_AT, metal, 84.);
                        fx.sparkle(MEDAL_AT, metal);
                        fx.callout(MEDAL_AT - vec2(0., 48.), medal_name(medal), metal);
                    }
                }
                Event::Ballast { neutral } => {
                    let (glyph, tone) = if neutral {
                        (Icon::Anchor, AQUA)
                    } else {
                        (Icon::ArrowUp, PAPER)
                    };
                    fx.ring(boat, tone, 30.);
                    fx.popup_icon(above, glyph, "", tone);
                    fx.ring(button_center(BALLAST_BUTTON), tone, 46.);
                }
                Event::Sonar => {
                    fx.ring(boat, Color::new(0.3, 1., 0.8, 0.7), 64.);
                    fx.ring(button_center(SONAR_BUTTON), AQUA, 46.);
                    // The air it cost, between the dials.
                    fx.popup(vec2(822., 152.), "-2", CORAL);
                    fx.ring(AIR_DIAL, CORAL, 50.);
                }
                Event::Impact { at, loss, cause } => {
                    let p = sea(at);
                    let (tone, count) = match cause {
                        Cause::Reef => (ROCK, 14),
                        Cause::Hazard(HazardKind::Jelly) => (VIOLET, 18),
                        Cause::Hazard(HazardKind::Vent) => (EMBER, 22),
                        Cause::Hazard(HazardKind::Mine) => (EMBER, 44),
                    };
                    fx.burst(p, tone, count);
                    fx.popup_icon(above, Icon::Shield, &format!("-{:.0}", loss.ceil()), CORAL);
                    fx.ring(HULL_DIAL, CORAL, 58.);
                    if cause == Cause::Hazard(HazardKind::Mine) {
                        fx.ring(p, EMBER, 80.);
                        fx.shake(0.35);
                        fx.flash(Color::new(1., 0.55, 0.25, 0.14));
                    }
                }
                Event::LowAir => {
                    fx.vignette(Color::new(1., 0.25, 0.3, 0.34));
                    fx.ring(AIR_DIAL, CORAL, 62.);
                    fx.popup(above, "Low air", CORAL);
                }
                Event::Wreck { hull } => {
                    fx.burst(boat, if hull { EMBER } else { BUBBLE }, 56);
                    fx.ring(boat, CORAL, 90.);
                }
                Event::Recovered => {
                    let dock = sea(BASE);
                    fx.ring(dock, GOLD, 52.);
                    fx.burst(dock, BUBBLE, 16);
                    let tug = tug_center(usize::from(self.recovery));
                    fx.ring(tug, GOLD, 26.);
                    fx.popup_icon(tug - vec2(0., 22.), Icon::Ship, "-1", GOLD);
                }
                Event::NextSortie => {}
            }
        }
    }
}
