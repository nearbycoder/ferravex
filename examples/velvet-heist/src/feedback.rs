//! Effects felt where each accepted event happened, in logical canvas units.
//!
//! Only the latest tick's recorded [`Event`]s and its resulting state feed
//! these requests; nothing here reaches back into the rules.
use crate::hud::{CLOCK_AT, bank_at, card_medal_at, medal_at};
use crate::model::{Cause, Event, Game, Phase, Refusal};
use crate::sprites::*;
use crate::sprites::{GOLD, RED};
use crate::view::{center, head};
use ferravex_arcade::*;

impl Game {
    pub(crate) fn feedback(&self, _cue: Cue, fx: &mut Juice) {
        let me = center(self.player);
        let lift = vec2(0., 18.);
        for event in self.events.as_slice() {
            match *event {
                Event::Start => {
                    fx.ring(me, ORCHID, 34.);
                    fx.sparkle(me, ORCHID);
                }
                Event::Load => {
                    fx.ring(me, MINT, 34.);
                    fx.sparkle(me, MINT);
                }
                Event::Crouch(low) => {
                    fx.ring(me + vec2(0., 8.), if low { ORCHID } else { AMBER }, 22.);
                }
                Event::Heard(i) => {
                    fx.popup(head(self.guards[usize::from(i)].pos), "?", AMBER);
                }
                Event::Spotted(i) => {
                    fx.popup(head(self.guards[usize::from(i)].pos), "!", RED);
                    fx.ring(me, RED, 32.);
                    fx.shake(0.16);
                }
                Event::Throw(p) => {
                    let c = center(p);
                    fx.burst(c, GOLD, 12);
                    fx.ring(c, GOLD, 30.);
                    fx.popup_icon(c - lift, Icon::Coin, "", GOLD);
                }
                Event::Refused(p, why) => {
                    let c = center(p);
                    fx.shake(0.07);
                    match why {
                        Refusal::Nothing => fx.ring(me, with_alpha(MUTED, 0.7), 18.),
                        Refusal::NoCoins => {
                            fx.ring(c, RED, 22.);
                            fx.popup_icon(c - lift, Icon::Coin, "0", RED);
                        }
                        Refusal::BadAim => {
                            fx.ring(c, RED, 22.);
                            fx.popup_icon(c - lift, Icon::Cross, "", RED);
                        }
                        Refusal::NeedPass => {
                            fx.ring(c, RED, 24.);
                            fx.popup_icon(c - lift, Icon::Lock, "", RED);
                        }
                        Refusal::NeedRecords => {
                            fx.ring(c, RED, 24.);
                            let left = self.needed.saturating_sub(self.stolen);
                            fx.popup_icon(c - lift, Icon::Scroll, &format!("{left} left"), PAPER);
                        }
                    }
                }
                Event::WorkStart(p, kind) => fx.ring(center(p), kind_color(kind), 24.),
                Event::WorkCancel(p) => {
                    let c = center(p);
                    fx.burst(c, MUTED, 6);
                    fx.ring(c, with_alpha(MUTED, 0.7), 18.);
                }
                Event::WorkDone(p, kind) => self.finished(p, kind, fx),
                Event::Beam(p) => {
                    let c = center(p);
                    fx.burst(c, RED, 18);
                    fx.ring(c, RED, 36.);
                    fx.popup_icon(c - lift, Icon::Bell, "+45%", RED);
                }
                Event::MedalLost => {
                    for i in 1..3 {
                        fx.burst(medal_at(i), GOLD, 8);
                    }
                    fx.ring(medal_at(1), RED, 24.);
                }
                Event::Second(_) => fx.ring(CLOCK_AT, RED, 24.),
                Event::Escape(medals) => {
                    // The debrief card banks the heist; the final heist
                    // hands over to the host's victory card instead.
                    if self.phase == Phase::Debrief {
                        fx.sparkle(bank_at() + vec2(60., 0.), GOLD);
                        for i in 0..usize::from(medals) {
                            fx.sparkle(card_medal_at(i), GOLD);
                        }
                        fx.confetti(card_medal_at(1), 16 * u32::from(medals));
                    }
                }
                Event::Caught(cause) => {
                    // The capture card names the cause; the scene feels it.
                    let at = match cause {
                        Cause::Time => CLOCK_AT,
                        _ => me,
                    };
                    fx.ring(at, RED, 44.);
                    fx.burst(me, RED, 20);
                    fx.flash(Color::new(0.9, 0.12, 0.12, 0.16));
                }
            }
        }
        // While a guard watches, the edges glow red with the rising alarm.
        if self.phase == Phase::Infiltrating && self.seen {
            let level = f32::from(self.alarm) / 1000.;
            fx.vignette(Color::new(0.92, 0.16, 0.16, 0.12 + 0.3 * level));
        }
    }
    /// Finished work: the object's own reward, felt on its tile.
    fn finished(&self, p: crate::model::Pos, kind: u8, fx: &mut Juice) {
        let c = center(p);
        let lift = vec2(0., 18.);
        match kind {
            b'K' => {
                fx.sparkle(c, GOLD);
                fx.ring(c, GOLD, 32.);
                fx.popup_icon(c - lift, Icon::Key, "", GOLD);
                // Every door it opens answers with a glint.
                for (i, _) in self.tiles.iter().enumerate().filter(|(_, t)| **t == b'=') {
                    fx.ring(center(Self::tile_pos(i)), GOLD, 22.);
                }
            }
            b'=' => {
                fx.burst(c, GOLD, 14);
                fx.ring(c, GOLD, 30.);
                fx.popup_icon(c - lift, Icon::Unlock, "", GOLD);
            }
            b'T' => {
                fx.sparkle(c, MINT);
                fx.ring(c, MINT, 40.);
                fx.popup_icon(c - lift, Icon::Bolt, "Off", MINT);
                for (i, _) in self.tiles.iter().enumerate().filter(|(_, t)| **t == b'!') {
                    let b = center(Self::tile_pos(i));
                    fx.burst(b, MINT, 8);
                    fx.ring(b, MINT, 20.);
                }
            }
            b'L' => {
                fx.sparkle(c, PAPER);
                fx.burst(c, GOLD, 14);
                fx.popup(c - lift, "+250", PAPER);
                if self.stolen >= self.needed
                    && let Some(exit) = self.exit()
                {
                    // The way out opens.
                    fx.ring(center(exit), MINT, 48.);
                    fx.sparkle(center(exit), MINT);
                }
            }
            _ => {
                fx.sparkle(c, GOLD);
                fx.burst(c, GOLD, 16);
                fx.popup(c - lift, "+175", GOLD);
                if self.extras >= self.extra_total && self.peak_alarm == 0 {
                    fx.sparkle(medal_at(2), GOLD);
                    fx.ring(medal_at(2), GOLD, 22.);
                }
            }
        }
    }
    fn tile_pos(index: usize) -> crate::model::Pos {
        crate::model::Pos::new(
            (index % crate::model::COLS) as u8,
            (index / crate::model::COLS) as u8,
        )
    }
}
