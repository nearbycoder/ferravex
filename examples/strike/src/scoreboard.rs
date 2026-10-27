//! Match bookkeeping derived only from accepted FPS events and authority ticks.
//!
//! Practice is a two-minute gauntlet: you against the five-bot crew. The clock
//! is the authoritative match tick, so pausing, frame rate and rendering never
//! change when a match ends or what it scored. Network and replay views reuse
//! the same tracker without a clock for kill feed, streaks and callouts. No
//! state here feeds back into the simulation, replay stream or wire format.
use ferravex_fps::{GameEvent, MAX_PLAYERS, MAX_SHOT_DISTANCE, TICK_RATE};
use ferravex_render::Vec3;

pub(super) const MATCH_SECONDS: u32 = 120;
pub(super) const MATCH_TICKS: u32 = MATCH_SECONDS * TICK_RATE;
/// A kill chains a multi-kill when it lands within this many ticks of the last.
pub(super) const MULTI_WINDOW: u32 = 3 * TICK_RATE;
/// The secret: a rubber duck on top of the north-west container.
pub(super) const DUCK: Vec3 = Vec3::new(-10.7, 3.36, -6.8);
const DUCK_RADIUS: f32 = 0.4;

/// Something worth celebrating (or mourning), produced once per accepted event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Moment {
    /// Any elimination, for the kill feed.
    Feed {
        attacker: u8,
        victim: u8,
        headshot: bool,
    },
    /// You fragged `victim`; `nth` counts your frags on that victim.
    Kill {
        victim: u8,
        headshot: bool,
        nth: u32,
        first_blood: bool,
        revenge: bool,
    },
    Multi(u8),
    Streak(u16),
    /// You were fragged; `nth` counts that killer's frags on you.
    Died {
        killer: u8,
        nth: u32,
        deaths: u32,
    },
    /// The crew fragged itself.
    Oops {
        attacker: u8,
        nth: u32,
    },
    Respawned,
    Duck,
    /// Whole seconds left on the practice clock: 30, then 5..=1.
    Clock(u32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Line {
    pub kills: u32,
    pub deaths: u32,
}

/// Final numbers for the results card.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Summary {
    pub frags: u32,
    pub deaths: u32,
    pub shots: u32,
    pub hits: u32,
    pub headshots: u32,
    pub wall_shots: u32,
    pub sky_shots: u32,
    pub best_streak: u16,
    pub best_multi: u8,
    pub revenge: u32,
    pub crew_oops: u32,
    pub duck: bool,
    pub nemesis: Option<(u8, u32)>,
}

impl Summary {
    pub fn accuracy(&self) -> u32 {
        (self.hits * 100).checked_div(self.shots).unwrap_or(0)
    }
}

#[derive(Clone, Debug)]
pub(super) struct Tracker {
    local: u8,
    timed: bool,
    length: u32,
    pub lines: [Line; MAX_PLAYERS],
    /// Frags each attacker scored on you, and yours on each victim.
    against_you: [u32; MAX_PLAYERS],
    yours_on: [u32; MAX_PLAYERS],
    pub summary: Summary,
    pub streak: u16,
    multi: u8,
    last_kill: Option<u32>,
    last_killer: Option<u8>,
    any_kill: bool,
    last_tick: u32,
}

impl Tracker {
    /// `timed` enables the practice clock and its end of match.
    pub fn new(local: u8, timed: bool) -> Self {
        Self {
            local,
            timed,
            length: MATCH_TICKS,
            lines: [Line::default(); MAX_PLAYERS],
            against_you: [0; MAX_PLAYERS],
            yours_on: [0; MAX_PLAYERS],
            summary: Summary::default(),
            streak: 0,
            multi: 0,
            last_kill: None,
            last_killer: None,
            any_kill: false,
            last_tick: 0,
        }
    }

    /// Debug capture only: a shorter match for screenshots of the results.
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    pub fn with_seconds(mut self, seconds: Option<u32>) -> Self {
        if let Some(seconds) = seconds.filter(|s| *s > 0) {
            self.length = seconds.saturating_mul(TICK_RATE);
        }
        self
    }

    pub fn local(&self) -> u8 {
        self.local
    }

    /// Follow a network identity change without inventing past stats.
    pub fn set_local(&mut self, local: u8) {
        if local != self.local {
            *self = Self {
                length: self.length,
                ..Self::new(local, self.timed)
            };
        }
    }

    pub fn duck_found(&self) -> bool {
        self.summary.duck
    }

    /// Seconds left on the practice clock, rounded up; `None` when untimed.
    pub fn remaining_seconds(&self) -> Option<u32> {
        self.timed.then(|| {
            self.length
                .saturating_sub(self.last_tick)
                .div_ceil(TICK_RATE)
        })
    }

    pub fn finished(&self) -> bool {
        self.timed && self.last_tick >= self.length
    }

    /// Fold one authoritative tick's events. `tick` is the authority tick
    /// after those events. Moments are produced in a fixed order.
    pub fn observe(&mut self, tick: u32, events: &[GameEvent], mut out: impl FnMut(Moment)) {
        let local = self.local;
        let mut local_shots = 0u32;
        let mut local_hits = 0u32;
        let mut miss_ends = [false; 2]; // [wall, sky] for the batch's last miss
        let mut headshot_on = [false; MAX_PLAYERS];
        for event in events {
            match *event {
                GameEvent::Shot {
                    player,
                    origin,
                    end,
                } if player == local => {
                    local_shots += 1;
                    let sky = origin.distance(end) >= MAX_SHOT_DISTANCE - 0.01;
                    miss_ends = [!sky, sky];
                    if self.timed && !self.summary.duck && segment_hits(origin, end, DUCK) {
                        self.summary.duck = true;
                        out(Moment::Duck);
                    }
                }
                GameEvent::Hit {
                    attacker,
                    victim,
                    headshot,
                    ..
                } => {
                    if let Some(h) = headshot_on.get_mut(usize::from(victim)) {
                        *h = headshot;
                    }
                    if attacker == local {
                        local_hits += 1;
                        self.summary.headshots += u32::from(headshot);
                    }
                }
                GameEvent::Killed { attacker, victim } => {
                    let headshot = headshot_on
                        .get(usize::from(victim))
                        .copied()
                        .unwrap_or(false);
                    self.kill(tick, attacker, victim, headshot, &mut out);
                }
                GameEvent::Respawned { player } if player == local => out(Moment::Respawned),
                _ => {}
            }
        }
        self.summary.shots += local_shots;
        self.summary.hits += local_hits;
        // At most one local shot per authority tick; a batch without a local
        // hit is a miss that either struck geometry or sailed into the sky.
        if local_shots > local_hits {
            let misses = local_shots - local_hits;
            if miss_ends[1] {
                self.summary.sky_shots += misses;
            } else if miss_ends[0] {
                self.summary.wall_shots += misses;
            }
        }
        self.clock(tick, out);
    }

    fn kill(
        &mut self,
        tick: u32,
        attacker: u8,
        victim: u8,
        headshot: bool,
        out: &mut impl FnMut(Moment),
    ) {
        let local = self.local;
        let (a, v) = (usize::from(attacker), usize::from(victim));
        if a >= MAX_PLAYERS || v >= MAX_PLAYERS {
            return;
        }
        out(Moment::Feed {
            attacker,
            victim,
            headshot,
        });
        let first_blood = !self.any_kill;
        self.any_kill = true;
        if attacker != victim {
            self.lines[a].kills += 1;
        }
        self.lines[v].deaths += 1;
        if victim == local {
            self.summary.deaths += 1;
            self.streak = 0;
            self.multi = 0;
            self.last_kill = None;
            if attacker != local {
                self.against_you[a] += 1;
                self.last_killer = Some(attacker);
                let worst = (0..MAX_PLAYERS as u8)
                    .filter(|&id| id != local)
                    .max_by_key(|&id| (self.against_you[usize::from(id)], std::cmp::Reverse(id)))
                    .filter(|&id| self.against_you[usize::from(id)] > 0);
                self.summary.nemesis = worst.map(|id| (id, self.against_you[usize::from(id)]));
                out(Moment::Died {
                    killer: attacker,
                    nth: self.against_you[a],
                    deaths: self.summary.deaths,
                });
            }
            return;
        }
        if attacker != local {
            if attacker != victim {
                self.summary.crew_oops += 1;
                out(Moment::Oops {
                    attacker,
                    nth: self.summary.crew_oops,
                });
            }
            return;
        }
        self.summary.frags += 1;
        self.yours_on[v] += 1;
        let revenge = self.last_killer == Some(victim);
        if revenge {
            self.summary.revenge += 1;
            self.last_killer = None;
        }
        out(Moment::Kill {
            victim,
            headshot,
            nth: self.yours_on[v],
            first_blood,
            revenge,
        });
        self.multi = match self.last_kill {
            Some(last) if tick.saturating_sub(last) <= MULTI_WINDOW => self.multi + 1,
            _ => 1,
        };
        self.last_kill = Some(tick);
        self.summary.best_multi = self.summary.best_multi.max(self.multi);
        if self.multi >= 2 {
            out(Moment::Multi(self.multi));
        }
        self.streak += 1;
        self.summary.best_streak = self.summary.best_streak.max(self.streak);
        out(Moment::Streak(self.streak));
    }

    fn clock(&mut self, tick: u32, mut out: impl FnMut(Moment)) {
        let before = self.remaining_seconds();
        self.last_tick = self.last_tick.max(tick);
        if let (Some(before), Some(after)) = (before, self.remaining_seconds())
            && after < before
            && (after == 30 || (1..=5).contains(&after))
        {
            out(Moment::Clock(after));
        }
    }
}

/// Whether the accepted shot segment passes within the duck's radius.
fn segment_hits(origin: Vec3, end: Vec3, center: Vec3) -> bool {
    let d = end - origin;
    let length = d.length_squared();
    if length <= f32::EPSILON {
        return false;
    }
    let t = ((center - origin).dot(d) / length).clamp(0., 1.);
    (origin + d * t).distance_squared(center) <= DUCK_RADIUS * DUCK_RADIUS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bots::PracticeBots;
    use ferravex_fps::{BUTTON_FIRE, InputFrame, forward};

    fn collect(t: &mut Tracker, tick: u32, events: &[GameEvent]) -> Vec<Moment> {
        let mut out = Vec::new();
        t.observe(tick, events, |m| out.push(m));
        out
    }

    fn kill(attacker: u8, victim: u8, headshot: bool) -> [GameEvent; 2] {
        [
            GameEvent::Hit {
                attacker,
                victim,
                damage: 100,
                headshot,
            },
            GameEvent::Killed { attacker, victim },
        ]
    }

    #[test]
    fn kills_chain_into_multi_kills_streaks_and_revenge_from_accepted_events_only() {
        let mut t = Tracker::new(0, true);
        let first = collect(&mut t, 10, &kill(0, 1, true));
        assert!(first.contains(&Moment::Kill {
            victim: 1,
            headshot: true,
            nth: 1,
            first_blood: true,
            revenge: false
        }));
        assert!(!first.iter().any(|m| matches!(m, Moment::Multi(_))));
        let second = collect(&mut t, 10 + MULTI_WINDOW, &kill(0, 2, false));
        assert!(second.contains(&Moment::Multi(2)));
        let third = collect(&mut t, 20 + MULTI_WINDOW * 2, &kill(0, 3, false));
        assert!(
            !third.iter().any(|m| matches!(m, Moment::Multi(_))),
            "a gap longer than the window restarts the chain"
        );
        assert!(third.contains(&Moment::Streak(3)));
        assert_eq!(t.summary.best_multi, 2);
        assert_eq!(t.summary.headshots, 1);

        let died = collect(&mut t, 900, &kill(4, 0, false));
        assert!(died.contains(&Moment::Died {
            killer: 4,
            nth: 1,
            deaths: 1
        }));
        assert_eq!(t.streak, 0);
        let payback = collect(&mut t, 1000, &kill(0, 4, false));
        assert!(payback.iter().any(|m| matches!(
            m,
            Moment::Kill {
                revenge: true,
                first_blood: false,
                ..
            }
        )));
        assert_eq!(t.summary.best_streak, 3);
        assert_eq!(t.summary.nemesis, Some((4, 1)));
        let oops = collect(&mut t, 1100, &kill(2, 3, false));
        assert!(oops.contains(&Moment::Oops {
            attacker: 2,
            nth: 1
        }));
        assert_eq!(t.summary.frags, 4);
        assert_eq!(t.lines[2].kills, 1);
    }

    #[test]
    fn shots_split_into_hits_wall_and_sky_and_the_duck_is_found_once() {
        let mut t = Tracker::new(0, true);
        let shot = |end: Vec3| GameEvent::Shot {
            player: 0,
            origin: Vec3::new(0., 1.6, 0.),
            end,
        };
        collect(&mut t, 1, &[shot(Vec3::new(0., 1.6, -5.))]);
        collect(&mut t, 2, &[shot(Vec3::new(0., 1.6, -MAX_SHOT_DISTANCE))]);
        collect(
            &mut t,
            3,
            &[
                shot(Vec3::new(0., 1.6, -5.)),
                GameEvent::Hit {
                    attacker: 0,
                    victim: 2,
                    damage: 20,
                    headshot: false,
                },
            ],
        );
        assert_eq!(
            (
                t.summary.shots,
                t.summary.hits,
                t.summary.wall_shots,
                t.summary.sky_shots
            ),
            (3, 1, 1, 1)
        );
        assert_eq!(t.summary.accuracy(), 33);
        let origin = Vec3::new(-6., 1.62, 2.);
        let through = origin + (DUCK - origin) * 3.;
        let quack = [GameEvent::Shot {
            player: 0,
            origin,
            end: through,
        }];
        assert_eq!(collect(&mut t, 4, &quack), vec![Moment::Duck]);
        assert!(collect(&mut t, 5, &quack).is_empty(), "one duck per match");
        let mut network = Tracker::new(0, false);
        assert!(
            collect(&mut network, 4, &quack).is_empty(),
            "practice-only secret"
        );
    }

    #[test]
    fn the_practice_clock_counts_authority_ticks_and_ends_exactly_once() {
        let mut t = Tracker::new(0, true);
        assert_eq!(t.remaining_seconds(), Some(MATCH_SECONDS));
        let mut clocks = Vec::new();
        for tick in 1..=MATCH_TICKS {
            t.observe(tick, &[], |m| clocks.push(m));
            assert_eq!(t.finished(), tick == MATCH_TICKS);
        }
        assert_eq!(
            clocks,
            [30, 5, 4, 3, 2, 1].map(Moment::Clock).to_vec(),
            "final-seconds callouts happen once each"
        );
        let untimed = Tracker::new(0, false);
        assert_eq!(untimed.remaining_seconds(), None);
        assert!(!untimed.finished());
    }

    /// Run the real bots and authority with a deterministic aim script.
    fn gauntlet(ticks: u32) -> (Summary, u64) {
        let mut game = crate::practice();
        let mut bots = PracticeBots::new(&game.arena);
        let mut commands = Vec::new();
        let mut t = Tracker::new(0, true);
        let mut moments = 0u64;
        for _ in 0..ticks {
            let me = game.players[0].clone();
            let target = game
                .players
                .iter()
                .filter(|p| p.id != 0 && p.is_alive())
                .min_by(|a, b| {
                    a.position
                        .distance_squared(me.position)
                        .total_cmp(&b.position.distance_squared(me.position))
                });
            let mut input = InputFrame::default();
            if let Some(p) = target {
                let d = p.eye_position() - Vec3::Y * 0.3 - me.eye_position();
                input.yaw = d.x.atan2(-d.z);
                input.pitch = (d.y / d.length().max(0.01)).asin();
                debug_assert!(forward(input.yaw, input.pitch).dot(d) > 0.);
                input.buttons = BUTTON_FIRE;
            }
            bots.commands(&game, input, &mut commands);
            let events = game.step(&commands);
            bots.events(&events);
            t.observe(game.tick, &events, |_| moments += 1);
        }
        (t.summary, moments)
    }

    #[test]
    fn real_bot_gauntlets_score_identically_on_every_run() {
        let a = gauntlet(1200);
        assert_eq!(a, gauntlet(1200));
        assert!(a.0.shots > 0, "{a:?}");
        assert!(a.0.hits <= a.0.shots);
        assert!(a.0.wall_shots + a.0.sky_shots + a.0.hits <= a.0.shots);
    }
}
