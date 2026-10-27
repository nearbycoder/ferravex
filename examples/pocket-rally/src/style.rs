//! Deterministic style scoring and race statistics.
//!
//! [`Style::observe`] runs once per *accepted* vehicle tick, after
//! [`crate::session::RaceSession::step`] returns true. It only reads the race:
//! drift points, splits, bonuses and secrets never change gates, laps, timers,
//! the saved best lap or its ghost. Everything is a pure function of the
//! accepted tick sequence, so a replayed race scores identically.
use crate::sim::{Controls, LAPS, Race, RoadCue, Track};
use ferravex_play::gameplay::combo::{ChainReset, ScoreChain};
use ferravex_render::prelude::*;

/// Lateral slip (world units per second) that reads as a drift.
pub const DRIFT_SLIP: f32 = 24.;
/// Below this speed a slide is a shuffle, not a drift.
pub const DRIFT_SPEED: f32 = 70.;
/// Ticks without slip before a drift is banked (0.3 s at 120 Hz).
const DRIFT_GRACE: u32 = 36;
/// A drift must last a quarter second and earn a few points to count.
const DRIFT_MIN_TICKS: u32 = 30;
const DRIFT_MIN_POINTS: u32 = 15;
/// Back-to-back drifts within this window raise the multiplier, up to x5.
const CHAIN_WINDOW: f32 = 3.;
const SPIN_GRACE: u32 = 60;
pub const CLEAN_LAP_BONUS: u32 = 250;
pub const PERFECT_BONUS: u32 = 500;
pub const DONUT_BONUS: u32 = 300;
pub const SPLASH_BONUS: u32 = 200;
/// The farm pond. Driving into it is the Valle Verde secret.
pub const POND: [Vec2; 5] = [
    vec2(930., 560.),
    vec2(985., 534.),
    vec2(1060., 555.),
    vec2(1080., 612.),
    vec2(980., 625.),
];
/// The authored world boundary in `sim::Car::step`.
const BOUNDS: (Vec2, Vec2) = (vec2(28., 25.), vec2(1072., 625.));
const EVENTS: usize = 8;

/// How stylish a banked drift was, from tidiest to most ridiculous.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rating {
    Tidy,
    Spicy,
    Sideways,
    PureStyle,
    MaximumAttitude,
}

impl Rating {
    pub fn from_points(points: u32) -> Self {
        match points {
            0..40 => Self::Tidy,
            40..100 => Self::Spicy,
            100..200 => Self::Sideways,
            200..350 => Self::PureStyle,
            _ => Self::MaximumAttitude,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Tidy => "TIDY",
            Self::Spicy => "SPICY",
            Self::Sideways => "SIDEWAYS!",
            Self::PureStyle => "PURE STYLE!",
            Self::MaximumAttitude => "MAXIMUM ATTITUDE!",
        }
    }
}

/// One accepted-tick event for presentation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    DriftBanked {
        /// Points added to the style total, including the multiplier.
        points: u32,
        multiplier: u32,
        rating: Rating,
    },
    /// A drift ended on the grass; its unbanked points were lost.
    DriftBotched {
        lost: u32,
    },
    /// Seconds versus the best lap at this gate; negative is faster.
    Split {
        gate: usize,
        delta: f32,
    },
    Lap {
        number: u32,
        clean: bool,
    },
    PerfectLine,
    /// Hit the farm fence at the world edge. Strength is 0..=1.
    Bump {
        strength: f32,
    },
    OffTrack,
    WrongWay,
    Splash {
        first: bool,
    },
    Donut,
    Finished,
}

/// Bounded per-tick event list; never allocates.
#[derive(Clone, Copy, Debug, Default)]
pub struct Events {
    items: [Option<Event>; EVENTS],
    len: usize,
}

impl Events {
    fn push(&mut self, event: Event) {
        if self.len < EVENTS {
            self.items[self.len] = Some(event);
            self.len += 1;
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = Event> + '_ {
        self.items[..self.len].iter().flatten().copied()
    }
}

/// What a finished race looked like, for the results card and rank title.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub style: u32,
    pub best_drift: u32,
    pub drifts: u32,
    pub botched: u32,
    pub best_chain: u32,
    pub clean_laps: u32,
    pub offroad_ticks: u32,
    pub off_track_episodes: u32,
    pub wrong_way_episodes: u32,
    pub bumps: u32,
    pub top_speed: f32,
    pub splashes: u32,
    pub donuts: u32,
}

impl Stats {
    pub fn grass_seconds(&self) -> f32 {
        self.offroad_ticks as f32 / 120.
    }

    pub fn perfect(&self) -> bool {
        self.clean_laps == LAPS
    }
}

/// Snapshot of the car before an accepted tick.
#[derive(Clone, Copy, Debug)]
pub struct Before {
    laps: u32,
    next_gate: usize,
    position: Vec2,
    angle: f32,
    off_track: bool,
}

impl Before {
    pub fn new(race: &Race, off_track: bool) -> Self {
        Self {
            laps: race.laps,
            next_gate: race.next_gate,
            position: race.car.pose.position,
            angle: race.car.pose.angle,
            off_track,
        }
    }
}

pub struct Style {
    pub stats: Stats,
    pub events: Events,
    chain: ScoreChain,
    drift: f32,
    drift_ticks: u32,
    grace: u32,
    lap_offroad: bool,
    spin: f32,
    spin_grace: u32,
    in_pond: bool,
    wrong_way: bool,
}

impl Default for Style {
    fn default() -> Self {
        Self::new()
    }
}

impl Style {
    pub fn new() -> Self {
        Self {
            stats: Stats::default(),
            events: Events::default(),
            chain: ScoreChain::new(CHAIN_WINDOW, 1, 5).expect("authored drift chain"),
            drift: 0.,
            drift_ticks: 0,
            grace: 0,
            lap_offroad: false,
            spin: 0.,
            spin_grace: 0,
            in_pond: false,
            wrong_way: false,
        }
    }

    /// Points of the drift in progress (not yet banked).
    pub fn live_drift(&self) -> u32 {
        if self.drift_ticks >= DRIFT_MIN_TICKS {
            self.drift as u32
        } else {
            0
        }
    }

    /// Multiplier the next banked drift will earn if it chains.
    pub fn next_multiplier(&self) -> u32 {
        if self.chain.remaining_life().is_some() {
            (self.chain.multiplier() + 1).min(5)
        } else {
            1
        }
    }

    pub fn drifting(&self) -> bool {
        self.drift_ticks > 0 && self.grace == 0
    }

    /// Observe one accepted vehicle tick. Events are replaced every call.
    pub fn observe(
        &mut self,
        before: Before,
        race: &Race,
        track: &Track,
        controls: Controls,
        off_track: bool,
        dt: f32,
    ) {
        self.events = Events::default();
        let car = &race.car;
        let speed = car.velocity.length();
        self.stats.top_speed = self.stats.top_speed.max(speed);
        let _ = self.chain.advance(dt);

        // Drifting: sustained sideways slip on the road at speed.
        let forward = Vec2::from_angle(car.pose.angle);
        let slip = car.velocity.dot(vec2(-forward.y, forward.x)).abs();
        let slipping = !car.offroad && speed > DRIFT_SPEED && slip > DRIFT_SLIP;
        if car.offroad {
            self.stats.offroad_ticks += 1;
            self.lap_offroad = true;
            if self.drift_ticks >= DRIFT_MIN_TICKS && self.drift >= DRIFT_MIN_POINTS as f32 {
                self.stats.botched += 1;
                self.events.push(Event::DriftBotched {
                    lost: self.drift as u32,
                });
                self.chain.reset(ChainReset::ChainOnly);
            }
            self.drift = 0.;
            self.drift_ticks = 0;
            self.grace = 0;
        } else if slipping {
            self.drift += slip * (speed / 100.) * dt * 1.5;
            self.drift_ticks += 1;
            self.grace = 0;
        } else if self.drift_ticks > 0 {
            self.grace += 1;
            if self.grace >= DRIFT_GRACE {
                self.bank();
            }
        }

        // Donuts: a full turn of heading while sliding or on the handbrake.
        let turn = (car.pose.angle - before.angle + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        if (car.sliding || controls.handbrake) && speed > 15. {
            self.spin += turn;
            self.spin_grace = 0;
            if self.spin.abs() >= std::f32::consts::TAU {
                self.spin -= std::f32::consts::TAU.copysign(self.spin);
                self.stats.donuts += 1;
                self.stats.style += DONUT_BONUS;
                self.events.push(Event::Donut);
            }
        } else {
            self.spin_grace += 1;
            if self.spin_grace >= SPIN_GRACE {
                self.spin = 0.;
            }
        }

        // Splits and laps.
        if race.next_gate != before.next_gate
            && before.next_gate != 0
            && let Some(delta) = race.split_delta(before.next_gate)
        {
            self.events.push(Event::Split {
                gate: before.next_gate,
                delta,
            });
        }
        if race.laps > before.laps {
            let clean = !self.lap_offroad;
            if clean {
                self.stats.clean_laps += 1;
                self.stats.style += CLEAN_LAP_BONUS;
            }
            self.lap_offroad = false;
            self.events.push(Event::Lap {
                number: race.laps,
                clean,
            });
            if race.finished() {
                if self.drift_ticks > 0 {
                    self.bank();
                }
                if self.stats.perfect() {
                    self.stats.style += PERFECT_BONUS;
                    self.events.push(Event::PerfectLine);
                }
                self.events.push(Event::Finished);
            }
        }

        // Fence bumps at the world edge.
        let at_edge = |p: Vec2| {
            p.x <= BOUNDS.0.x || p.y <= BOUNDS.0.y || p.x >= BOUNDS.1.x || p.y >= BOUNDS.1.y
        };
        let travel = (car.pose.position - before.position).length() / dt.max(1e-6);
        if at_edge(car.pose.position) && !at_edge(before.position) && travel > 30. {
            self.stats.bumps += 1;
            self.events.push(Event::Bump {
                strength: (travel / 220.).clamp(0.2, 1.),
            });
        }

        if off_track && !before.off_track {
            self.stats.off_track_episodes += 1;
            self.events.push(Event::OffTrack);
        }
        let wrong_way = !race.finished() && speed > 20. && track.road_cue(car) == RoadCue::WrongWay;
        if wrong_way && !self.wrong_way {
            self.stats.wrong_way_episodes += 1;
            self.events.push(Event::WrongWay);
        }
        self.wrong_way = wrong_way;

        let in_pond = in_polygon(car.pose.position, &POND);
        if in_pond && !self.in_pond && speed > 25. {
            self.stats.splashes += 1;
            let first = self.stats.splashes == 1;
            if first {
                self.stats.style += SPLASH_BONUS;
            }
            self.events.push(Event::Splash { first });
        }
        self.in_pond = in_pond;
    }

    fn bank(&mut self) {
        let points = self.drift as u32;
        if self.drift_ticks >= DRIFT_MIN_TICKS && points >= DRIFT_MIN_POINTS {
            let award = self
                .chain
                .award_with_receipt(points)
                .expect("nonzero drift points");
            let total = award.nominal_points.min(u64::from(u32::MAX)) as u32;
            self.stats.style = self.stats.style.saturating_add(total);
            self.stats.drifts += 1;
            self.stats.best_drift = self.stats.best_drift.max(total);
            self.stats.best_chain = self.stats.best_chain.max(award.chain);
            self.events.push(Event::DriftBanked {
                points: total,
                multiplier: award.multiplier,
                rating: Rating::from_points(points),
            });
        }
        self.drift = 0.;
        self.drift_ticks = 0;
        self.grace = 0;
    }
}

pub fn in_polygon(p: Vec2, polygon: &[Vec2]) -> bool {
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let (a, b) = (polygon[i], polygon[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::RaceSession;
    use crate::sim::Pose;

    const DT: f32 = 1. / 120.;

    /// Drive the real session; `driver` chooses controls from the race.
    fn race_with(mut driver: impl FnMut(&mut Race, &Track) -> Controls) -> (Race, Style, u32) {
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut session = RaceSession::new();
        let mut style = Style::new();
        let mut events = 0;
        for _ in 0..30_000 {
            let frame = session.advance(1. / 60., true);
            for _ in 0..frame.steps {
                let controls = if session.countdown() == 0 {
                    driver(&mut race, &track)
                } else {
                    Controls::default()
                };
                let before = Before::new(&race, session.off_track());
                if session.step(&mut race, &track, controls, frame.dt) {
                    style.observe(
                        before,
                        &race,
                        &track,
                        controls,
                        session.off_track(),
                        frame.dt,
                    );
                    events += style.events.iter().count() as u32;
                }
            }
            if race.finished() {
                break;
            }
        }
        (race, style, events)
    }

    #[test]
    fn style_observation_is_deterministic_and_never_changes_the_race() {
        let drifty = |race: &mut Race, track: &Track| {
            let mut controls = race.autopilot(track);
            // Periodic handbrake pulses through the autopilot's own steering.
            controls.handbrake = race.ticks % 900 > 820 || controls.steer.abs() > 0.4;
            controls
        };
        let (a, style_a, events_a) = race_with(drifty);
        let (b, style_b, events_b) = race_with(drifty);
        assert!(a.finished() && b.finished());
        assert_eq!(style_a.stats, style_b.stats);
        assert_eq!(events_a, events_b);
        assert_eq!(a.total_time, b.total_time);
        assert!(style_a.stats.drifts > 0, "handbrake corners earn drifts");
        assert!(style_a.stats.style > 0);
        // The observer is read-only: an unobserved race finishes identically.
        let track = Track::new();
        let mut plain = Race::new(&track);
        let mut session = RaceSession::new();
        while !plain.finished() {
            let frame = session.advance(1. / 60., true);
            for _ in 0..frame.steps {
                let controls = if session.countdown() == 0 {
                    drifty(&mut plain, &track)
                } else {
                    Controls::default()
                };
                session.step(&mut plain, &track, controls, frame.dt);
            }
        }
        assert_eq!(plain.total_time, a.total_time);
        assert_eq!(plain.car.pose.position, a.car.pose.position);
    }

    #[test]
    fn clean_autopilot_race_is_a_perfect_line_with_monotonic_splits() {
        let (mut race, style, _) = race_with(|race, track| race.autopilot(track));
        assert!(race.finished());
        assert_eq!(style.stats.clean_laps, LAPS);
        assert!(style.stats.perfect());
        assert!(style.stats.style >= LAPS * CLEAN_LAP_BONUS + PERFECT_BONUS);
        assert!(style.stats.top_speed > 150.);
        let recorded = race.best_splits;
        let times: Vec<f32> = recorded[1..].iter().map(|split| split.unwrap()).collect();
        assert!(times.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(times[times.len() - 1] < race.best_time.unwrap());
        // A restored save has only 20 Hz poses; derived splits agree closely.
        race.derive_splits(&Track::new());
        for (derived, exact) in race.best_splits[1..].iter().zip(&recorded[1..]) {
            assert!((derived.unwrap() - exact.unwrap()).abs() < 0.03);
        }
    }

    #[test]
    fn grass_botches_a_drift_and_breaks_the_chain() {
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut style = Style::new();
        let forward = Vec2::from_angle(race.car.pose.angle);
        let right = vec2(-forward.y, forward.x);
        // A long on-road slide accumulates points without banking.
        style.drift = 80.;
        style.drift_ticks = 90;
        race.car.pose.position = vec2(500., 280.);
        race.car.velocity = forward * 150. + right * 60.;
        race.car.offroad = true;
        let before = Before::new(&race, false);
        style.observe(before, &race, &track, Controls::default(), false, DT);
        assert!(matches!(
            style.events.iter().next(),
            Some(Event::DriftBotched { lost: 80 })
        ));
        assert_eq!(style.stats.botched, 1);
        assert_eq!(style.stats.style, 0);
        assert_eq!(style.live_drift(), 0);
    }

    #[test]
    fn chained_drifts_raise_the_multiplier_and_ratings_escalate() {
        let mut style = Style::new();
        for expected in 1..=6_u32 {
            style.drift = 50.;
            style.drift_ticks = 40;
            style.bank();
            let Some(Event::DriftBanked {
                points, multiplier, ..
            }) = style.events.iter().last()
            else {
                panic!("drift banked");
            };
            assert_eq!(multiplier, expected.min(5));
            assert_eq!(points, 50 * expected.min(5));
            style.events = Events::default();
        }
        assert_eq!(style.stats.best_chain, 6);
        assert!(Rating::from_points(10) < Rating::from_points(450));
        assert_eq!(Rating::from_points(450).label(), "MAXIMUM ATTITUDE!");
        // Too short to count: silently discarded, no chain refresh.
        let mut quick = Style::new();
        quick.drift = 50.;
        quick.drift_ticks = 5;
        quick.bank();
        assert_eq!(quick.events.iter().count(), 0);
        assert_eq!(quick.stats.style, 0);
    }

    #[test]
    fn pond_splash_rewards_once_and_fence_bumps_are_counted() {
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut style = Style::new();
        let pond = vec2(1000., 580.);
        assert!(in_polygon(pond, &POND));
        assert!(!in_polygon(vec2(900., 500.), &POND));
        for round in 0..2 {
            race.car.pose = Pose {
                position: vec2(1000., 500.),
                angle: 0.,
            };
            let before = Before::new(&race, true);
            race.car.pose.position = pond;
            race.car.velocity = vec2(0., 80.);
            style.observe(before, &race, &track, Controls::default(), true, DT);
            assert!(
                style
                    .events
                    .iter()
                    .any(|e| e == Event::Splash { first: round == 0 })
            );
            race.car.pose.position = vec2(1000., 500.);
            style.observe(
                Before::new(&race, true),
                &race,
                &track,
                Controls::default(),
                true,
                DT,
            );
        }
        assert_eq!(style.stats.splashes, 2);
        assert_eq!(style.stats.style, SPLASH_BONUS);

        race.car.pose.position = vec2(1060., 300.);
        let before = Before::new(&race, true);
        race.car.pose.position = vec2(1072., 300.);
        race.car.velocity = vec2(-30., 0.);
        style.observe(before, &race, &track, Controls::default(), true, DT);
        assert_eq!(style.stats.bumps, 1);
    }

    #[test]
    fn a_full_handbrake_spin_is_a_donut() {
        let track = Track::new();
        let mut race = Race::new(&track);
        let mut style = Style::new();
        race.car.velocity = vec2(60., 0.);
        let controls = Controls {
            handbrake: true,
            ..Default::default()
        };
        for _ in 0..40 {
            let before = Before::new(&race, false);
            race.car.pose.angle = (race.car.pose.angle + 0.2).rem_euclid(std::f32::consts::TAU);
            style.observe(before, &race, &track, controls, false, DT);
        }
        assert_eq!(style.stats.donuts, 1);
        assert!(style.stats.style >= DONUT_BONUS);
    }
}
