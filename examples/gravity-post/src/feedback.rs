//! Accepted model events become effects where they happened: dust at the
//! feet, popups over the ship, bursts at the pad, coins at the refit desk.
use crate::cards::{pip_at, price_at, row_top};
use crate::model::{Cause, Event, FEET};
use crate::panel::{CLOCK_AT, FUEL_AT, PERMITS_AT};
use crate::view::{BLUE, DUST, GOLD, GREEN, PURPLE, RED, point, role_color};
use crate::{Game, content::Role};
use ferravex_arcade::*;

impl Game {
    /// Request this update's effects. Reads only the recorded events and the
    /// state they left behind.
    pub(crate) fn feedback(&self, fx: &mut Juice) {
        for event in self.events.iter() {
            self.effect(event, fx);
        }
    }

    fn effect(&self, event: Event, fx: &mut Juice) {
        let ship = point(self.pos);
        let feet = ship + vec2(0., FEET);
        let above = ship - vec2(0., 36.);
        match event {
            Event::Launch => fx.ring(ship, GOLD, 36.),
            Event::Liftoff(pad) => {
                fx.burst(feet, DUST, 14);
                fx.ring(feet, role_color(self.planet().roles[pad]), 30.);
            }
            Event::Touchdown { pad, speed } => {
                let soft = speed <= self.limits().1 * 0.5;
                for side in [-1., 1.] {
                    fx.burst(feet + vec2(side * 12., 0.), DUST, 9);
                }
                fx.ring(feet, role_color(self.planet().roles[pad]), 44.);
                if soft {
                    fx.popup(above, "SOFT", GREEN);
                } else {
                    fx.popup(above, "FIRM", GOLD);
                }
            }
            Event::Impact { hull, cargo } => {
                fx.burst(feet, DUST, 18);
                fx.burst(ship, Color::new(1., 0.72, 0.4, 1.), 10);
                fx.shake((0.1 + hull * 0.015).min(0.5));
                fx.popup_icon(above, Icon::Shield, &format!("-{:.0}", hull.max(1.)), RED);
                if cargo > 0.05 {
                    fx.popup_icon(above, Icon::Crate, &format!("-{:.0}", cargo.max(1.)), GOLD);
                }
            }
            Event::Wreck(Cause::Hull | Cause::Parcel) => {
                fx.burst(ship, Color::new(1., 0.62, 0.22, 1.), 64);
                fx.burst(ship, RED, 28);
                fx.burst(ship, DUST, 24);
                fx.ring(ship, Color::new(1., 0.55, 0.25, 1.), 96.);
                fx.flash(Color::new(1., 0.55, 0.35, 0.22));
                fx.shake(0.75);
            }
            Event::Wreck(Cause::Storm) => {
                fx.flash(Color::new(0.85, 0.85, 1., 0.35));
                fx.vignette(Color::new(0.35, 0.22, 0.6, 0.5));
                fx.ring(ship, PURPLE, 70.);
            }
            Event::Wreck(Cause::Recall) => {
                fx.ring(ship, MUTED, 50.);
                fx.popup_icon(above, Icon::Restart, "", MUTED);
            }
            Event::Refuel { amount, .. } => {
                fx.popup_icon(above, Icon::Fuel, &format!("+{amount:.0}"), BLUE);
                fx.ring(feet, BLUE, 36.);
            }
            Event::Repair { amount, .. } => {
                fx.popup_icon(above, Icon::Wrench, &format!("+{amount:.0}"), GREEN);
                fx.sparkle(ship, GREEN);
            }
            Event::Load(_) => {
                fx.burst(ship, GOLD, 26);
                fx.ring(ship, GOLD, 52.);
                fx.popup_icon(above, Icon::Crate, "Loaded", GOLD);
            }
            Event::Deliver { pad, credits, .. } => {
                let at = point(self.pad(pad)) - vec2(0., 12.);
                fx.burst(at, GREEN, 54);
                fx.ring(at, GREEN, 84.);
                fx.confetti(at, 36);
                fx.popup_icon(at - vec2(0., 28.), Icon::Coin, &format!("+{credits}"), GOLD);
            }
            Event::Beacon { fuel } => {
                let at = point(Vec2::from_array(self.planet().beacon));
                fx.burst(at, GOLD, 30);
                fx.ring(at, GOLD, 54.);
                fx.popup_icon(at - vec2(0., 22.), Icon::Coin, "+20", GOLD);
                if fuel > 0.05 {
                    fx.popup_icon(at - vec2(0., 22.), Icon::Fuel, &format!("+{fuel:.0}"), BLUE);
                }
            }
            Event::Refit { slot, cost } => {
                let level = self.upgrades[slot].saturating_sub(1);
                fx.popup_icon(
                    price_at(slot) - vec2(0., 16.),
                    Icon::Coin,
                    &format!("-{cost}"),
                    GOLD,
                );
                fx.sparkle(pip_at(slot, level), BLUE);
                fx.ring(vec2(386., row_top(slot) + 25.), BLUE, 60.);
            }
            Event::RefitRefused(slot) => {
                let at = price_at(slot);
                fx.shake(0.12);
                fx.ring(at, RED, 26.);
                if self.upgrades[slot] >= 2 {
                    fx.popup(at - vec2(0., 16.), "Max", MUTED);
                } else {
                    let need = format!("Need {}", self.upgrade_cost(slot));
                    fx.popup_icon(at - vec2(0., 16.), Icon::Coin, &need, RED);
                }
            }
            Event::Idle(Some(pad)) => {
                fx.ring(feet, RED, 24.);
                if self.planet().roles[pad] == Role::Delivery && !self.loaded {
                    fx.popup_icon(above, Icon::Crate, "None", MUTED);
                } else if self.capacity() - self.fuel <= 0.01 {
                    fx.popup_icon(above, Icon::Fuel, "Full", MUTED);
                } else {
                    fx.popup_icon(above, Icon::Fuel, "Empty", MUTED);
                }
            }
            Event::Idle(None) => fx.popup(above, "Land first", MUTED),
            Event::Recover => {
                fx.ring(ship, GOLD, 56.);
                fx.sparkle(ship, GOLD);
                fx.popup_icon(PERMITS_AT - vec2(0., 18.), Icon::Restart, "-1", GOLD);
            }
            Event::NextContract => fx.ring(ship, BLUE, 40.),
            Event::StormWarning => {
                fx.vignette(Color::new(0.4, 0.25, 0.7, 0.4));
                fx.flash(Color::new(0.8, 0.8, 1., 0.1));
                fx.popup_icon(CLOCK_AT - vec2(0., 18.), Icon::Clock, "30", RED);
            }
            Event::LowFuel => {
                fx.vignette(Color::new(0.95, 0.55, 0.2, 0.25));
                fx.popup_icon(FUEL_AT - vec2(0., 20.), Icon::Fuel, "Low", RED);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Phase;
    fn airborne() -> Game {
        let mut game = Game::new();
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        game.docked = None;
        game.pos = vec2(150., 80.);
        game
    }
    fn effects_of(game: &Game, cue: Cue) -> Juice {
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        fx
    }
    #[test]
    fn touchdown_and_delivery_are_felt_at_the_pad() {
        let mut game = airborne();
        let pad = game.role_pad(Role::Delivery);
        game.pos = game.pad(pad) - vec2(0., FEET + 0.05);
        game.velocity = vec2(0., 4.);
        let cue = game.step(&Input::default());
        assert_eq!(cue, Cue::Good);
        let landed = effects_of(&game, cue).counts();
        assert!(landed.sparks >= 18 && landed.rings == 1 && landed.popups == 1);
        game.loaded = true;
        let cue = game.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(game.phase, Phase::Debrief);
        let delivered = effects_of(&game, cue).counts();
        assert!(delivered.sparks >= 54 && delivered.popups == 1 && delivered.rings == 1);
        // Events last one update: the next idle tick requests nothing.
        let cue = game.step(&Input::default());
        assert!(effects_of(&game, cue).is_idle());
    }
    #[test]
    fn impacts_and_refusals_request_feedback_without_changing_rules() {
        let mut game = airborne();
        game.loaded = true;
        game.pos = vec2(175., game.ground(175.) - FEET);
        game.velocity = vec2(0., 40.);
        let cue = game.step(&Input::default());
        assert_eq!(cue, Cue::Bad);
        let hit = effects_of(&game, cue).counts();
        assert!(hit.sparks > 0 && hit.popups == 2);

        let mut desk = Game::new();
        desk.phase = Phase::Debrief;
        desk.credits = 0;
        let before = desk.clone();
        let cue = desk.step(&Input {
            slot: Some(2),
            ..Input::default()
        });
        assert_eq!((cue, &desk), (Cue::None, &before));
        let refused = effects_of(&desk, cue).counts();
        assert_eq!((refused.popups, refused.rings), (1, 1));

        let mut midair = airborne();
        let cue = midair.step(&Input {
            secondary: true,
            ..Input::default()
        });
        assert_eq!(cue, Cue::None);
        assert_eq!(effects_of(&midair, cue).counts().popups, 1);
    }
}
