//! Deterministic demolition scoring and the accepted events it reports.
//!
//! Everything here is driven by fixed physics ticks in [`super::Range::step`]
//! and [`super::Range::fire`]. Knockdowns are credited to the most recent shot
//! (its *volley*); later knockdowns in the same volley climb a chain
//! multiplier. Presentation reads [`RangeEvent`]s but never feeds back, so the
//! same inputs always produce the same score at any render rate.
use ferravex_render::prelude::{Vec3, vec3};

pub(super) const TARGET_POINTS: u32 = 100;
pub(super) const SUPPORT_POINTS: u32 = 25;
pub(super) const CHAIN_CAP: u32 = 5;
pub(super) const DOMINO_BONUS: u32 = 250;
pub(super) const WONDER_BONUS: u32 = 500;
pub(super) const OVERKILL_BONUS: u32 = 150;
pub(super) const BELL_BONUS: u32 = 500;
pub(super) const SPARE_BONUS: u32 = 200;
/// Seconds after a shot before a volley without knockdowns counts as a miss.
pub(super) const MISS_AFTER: f32 = 2.5;
/// The foundry's lunch bell hangs from the girder behind the centre tower. A
/// ball passing within [`BELL_RADIUS`] rings it once per run. It is a sensor
/// only: no collider is added and no ball changes course.
pub(super) const BELL: Vec3 = vec3(0.0, 5.75, -9.0);
pub(super) const BELL_RADIUS: f32 = 0.95;
/// Each tower has four blocks: two sandstone supports below two targets.
pub(super) const COLUMNS: usize = 3;
pub(super) const EVENT_CAPACITY: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Callout {
    /// The nth target of one volley; n >= 2.
    Chain(u32),
    /// A tower's last standing target fell.
    Timber,
    /// One volley felled targets in two or more towers.
    Domino,
    /// A volley knocked supports out from under a tower that had already
    /// lost both targets: kicking rubble, for style points.
    Overkill,
    /// One volley felled four or more targets.
    OneShotWonder,
    /// A volley already declared a miss knocked something down after all.
    Delayed,
    /// The secret lunch bell.
    Bell,
    /// Bonus for shells left over at a win.
    Spare(u8),
}

impl Callout {
    /// Larger callouts replace smaller ones in the single banner slot.
    pub(super) fn priority(self) -> u8 {
        match self {
            Callout::Chain(_) => 1,
            Callout::Delayed => 2,
            Callout::Timber => 3,
            Callout::Overkill => 4,
            Callout::Domino => 5,
            Callout::Bell => 6,
            Callout::OneShotWonder => 7,
            Callout::Spare(_) => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum RangeEvent {
    Fired {
        muzzle: Vec3,
        direction: Vec3,
        last: bool,
    },
    /// A ball changed velocity sharply: it struck a block or the floor.
    Impact {
        at: Vec3,
        strength: f32,
    },
    Knocked {
        at: Vec3,
        target: bool,
        column: u8,
        points: u32,
        chain: u32,
    },
    /// A standing tower's top target started to lean or slide.
    Wobble {
        column: u8,
        at: Vec3,
    },
    Callout {
        callout: Callout,
        points: u32,
        at: Vec3,
    },
    Miss {
        streak: u32,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Tally {
    pub points: u32,
    pub shots: u8,
    pub targets: u32,
    pub supports: u32,
    pub misses: u32,
    pub miss_streak: u32,
    pub best_chain: u32,
    pub dominoes: u32,
    pub wonders: u32,
    pub overkills: u32,
    pub bell: bool,
    pub spare: u8,
    pub bonus: u32,
    volley_targets: u32,
    volley_points: u32,
    volley_blocks: u32,
    /// Towers whose targets were both down when this volley was fired.
    volley_cleared: u8,
    volley_overkill: bool,
    /// The volley rang the bell: not a miss, even with no blocks down.
    volley_bell: bool,
    volley_columns: u8,
    volley_domino: bool,
    volley_wonder: bool,
    volley_resolved: bool,
    volley_missed: bool,
    wobbled: [bool; COLUMNS],
}

/// Stars for a finished run: fewer shells used is better; a loss earns none.
pub(super) fn stars(won: bool, shots: u8) -> u8 {
    match (won, shots) {
        (false, _) => 0,
        (true, 0..=3) => 3,
        (true, 4..=5) => 2,
        (true, _) => 1,
    }
}

/// Shots allowed for the next star above `current`, if any.
pub(super) fn next_star_shots(current: u8) -> Option<u8> {
    match current {
        0 => Some(8),
        1 => Some(5),
        2 => Some(3),
        _ => None,
    }
}

impl Tally {
    /// A new shot closes the previous volley. `cleared` has bit `n` set when
    /// tower `n` has already lost both targets.
    pub(super) fn fired(&mut self, cleared: u8, events: &mut Events) {
        self.resolve(events);
        self.shots += 1;
        self.volley_targets = 0;
        self.volley_points = 0;
        self.volley_cleared = cleared;
        self.volley_overkill = false;
        self.volley_bell = false;
        self.volley_blocks = 0;
        self.volley_columns = 0;
        self.volley_domino = false;
        self.volley_wonder = false;
        self.volley_resolved = false;
        self.volley_missed = false;
    }

    /// Called every tick with seconds since the last shot.
    pub(super) fn tick(&mut self, since_shot: f32, events: &mut Events) {
        if self.shots > 0 && since_shot >= MISS_AFTER {
            self.resolve(events);
        }
    }

    fn resolve(&mut self, events: &mut Events) {
        if self.shots == 0 || self.volley_resolved {
            return;
        }
        self.volley_resolved = true;
        if self.volley_blocks == 0 && !self.volley_bell {
            self.misses += 1;
            self.miss_streak += 1;
            self.volley_missed = true;
            events.push(RangeEvent::Miss {
                streak: self.miss_streak,
            });
        }
    }

    /// The top target of `column` started to move but has not fallen yet.
    pub(super) fn wobble(&mut self, column: usize, at: Vec3, events: &mut Events) {
        if !std::mem::replace(&mut self.wobbled[column], true) {
            events.push(RangeEvent::Wobble {
                column: column as u8,
                at,
            });
        }
    }

    /// One block changed from standing to down. `column_down` reports which of
    /// that tower's blocks (support, support, target, target) are now down.
    pub(super) fn knocked(
        &mut self,
        column: usize,
        target: bool,
        at: Vec3,
        column_down: [bool; 4],
        events: &mut Events,
    ) {
        if self.volley_missed {
            // A slow topple after the miss verdict: retract it, with a cheer.
            self.volley_missed = false;
            self.misses -= 1;
            self.miss_streak = 0;
            events.push(RangeEvent::Callout {
                callout: Callout::Delayed,
                points: 0,
                at,
            });
        }
        self.miss_streak = 0;
        self.volley_blocks += 1;
        self.volley_columns |= 1 << column;
        let (points, chain) = if target {
            self.targets += 1;
            self.volley_targets += 1;
            let chain = self.volley_targets.min(CHAIN_CAP);
            self.best_chain = self.best_chain.max(self.volley_targets);
            (TARGET_POINTS * chain, chain)
        } else {
            self.supports += 1;
            (SUPPORT_POINTS, 1)
        };
        self.points += points;
        self.volley_points += points;
        events.push(RangeEvent::Knocked {
            at,
            target,
            column: column as u8,
            points,
            chain,
        });
        if target && chain >= 2 {
            self.callout(Callout::Chain(self.volley_targets), points, at, events);
        }
        if target && column_down[2] && column_down[3] {
            self.callout(Callout::Timber, self.volley_points, at, events);
        }
        if !target && self.volley_cleared & (1 << column) != 0 && !self.volley_overkill {
            self.volley_overkill = true;
            self.overkills += 1;
            self.callout(Callout::Overkill, OVERKILL_BONUS, at, events);
        }
        let towers = self.volley_columns.count_ones();
        if target && towers >= 2 && self.volley_targets >= 2 && !self.volley_domino {
            self.volley_domino = true;
            self.dominoes += 1;
            self.callout(Callout::Domino, DOMINO_BONUS, at, events);
        }
        if self.volley_targets >= 4 && !self.volley_wonder {
            self.volley_wonder = true;
            self.wonders += 1;
            self.callout(Callout::OneShotWonder, WONDER_BONUS, at, events);
        }
    }

    pub(super) fn bell(&mut self, at: Vec3, events: &mut Events) {
        if !std::mem::replace(&mut self.bell, true) {
            self.volley_bell = true;
            self.callout(Callout::Bell, BELL_BONUS, at, events);
        }
    }

    /// Victory converts every unused shell into bonus points.
    pub(super) fn won(&mut self, ammo: u8, at: Vec3, events: &mut Events) {
        self.spare = ammo;
        if ammo > 0 {
            self.callout(
                Callout::Spare(ammo),
                SPARE_BONUS * u32::from(ammo),
                at,
                events,
            );
        }
    }

    /// Bonus callouts add their points; chain, timber and delayed callouts
    /// only describe points already awarded.
    fn callout(&mut self, callout: Callout, points: u32, at: Vec3, events: &mut Events) {
        if !matches!(
            callout,
            Callout::Chain(_) | Callout::Timber | Callout::Delayed
        ) {
            self.points += points;
            self.bonus += points;
        }
        events.push(RangeEvent::Callout {
            callout,
            points,
            at,
        });
    }
}

/// Bounded per-tick event storage. Allocated once; excess events are dropped
/// rather than growing, which never affects the score itself.
#[derive(Debug)]
pub(super) struct Events(Vec<RangeEvent>);

impl Events {
    pub(super) fn new() -> Self {
        Self(Vec::with_capacity(EVENT_CAPACITY))
    }

    pub(super) fn push(&mut self, event: RangeEvent) {
        if self.0.len() < EVENT_CAPACITY {
            self.0.push(event);
        }
    }

    pub(super) fn clear(&mut self) {
        self.0.clear();
    }

    pub(super) fn as_slice(&self) -> &[RangeEvent] {
        &self.0
    }

    #[cfg(test)]
    pub(super) fn capacity(&self) -> usize {
        self.0.capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn callouts(events: &Events) -> Vec<Callout> {
        events
            .as_slice()
            .iter()
            .filter_map(|event| match event {
                RangeEvent::Callout { callout, .. } => Some(*callout),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn chain_multiplier_grows_within_a_volley_and_resets_on_the_next_shot() {
        let mut tally = Tally::default();
        let mut events = Events::new();
        tally.fired(0, &mut events);
        tally.knocked(
            1,
            false,
            Vec3::ZERO,
            [true, false, false, false],
            &mut events,
        );
        tally.knocked(1, true, Vec3::ZERO, [true, false, true, false], &mut events);
        tally.knocked(1, true, Vec3::ZERO, [true, false, true, true], &mut events);
        // 25 support + 100 x1 + 100 x2.
        assert_eq!(tally.points, 325);
        assert_eq!(callouts(&events), [Callout::Chain(2), Callout::Timber]);
        assert_eq!(tally.best_chain, 2);
        events.clear();
        tally.fired(0b010, &mut events);
        tally.knocked(
            0,
            true,
            Vec3::ZERO,
            [false, false, true, false],
            &mut events,
        );
        assert_eq!(tally.points, 425, "the next volley starts at x1 again");
        assert!(callouts(&events).is_empty());
    }

    #[test]
    fn domino_overkill_and_wonder_pay_once_and_chain_is_capped() {
        let mut tally = Tally::default();
        let mut events = Events::new();
        tally.fired(0, &mut events);
        tally.knocked(
            0,
            true,
            Vec3::ZERO,
            [false, false, true, false],
            &mut events,
        );
        tally.knocked(
            1,
            true,
            Vec3::ZERO,
            [false, false, true, false],
            &mut events,
        );
        tally.knocked(1, true, Vec3::ZERO, [false, false, true, true], &mut events);
        tally.knocked(0, true, Vec3::ZERO, [false, false, true, true], &mut events);
        tally.knocked(0, false, Vec3::ZERO, [true, false, true, true], &mut events);
        tally.knocked(0, false, Vec3::ZERO, [true, true, true, true], &mut events);
        tally.knocked(
            2,
            true,
            Vec3::ZERO,
            [false, false, true, false],
            &mut events,
        );
        tally.knocked(2, true, Vec3::ZERO, [false, false, true, true], &mut events);
        let all = callouts(&events);
        assert_eq!(all.iter().filter(|c| **c == Callout::Domino).count(), 1);
        assert_eq!(
            all.iter().filter(|c| **c == Callout::OneShotWonder).count(),
            1
        );
        assert_eq!(all.iter().filter(|c| **c == Callout::Timber).count(), 3);
        assert!(
            !all.contains(&Callout::Overkill),
            "flattening a standing tower is not overkill"
        );
        assert!(all.contains(&Callout::Chain(6)));
        // Targets pay 100 x (1, 2, 3, 4, 5, 5); supports 25 each; bonuses once.
        let base = 100 * (1 + 2 + 3 + 4 + 5 + 5) + 50;
        assert_eq!(tally.points, base + DOMINO_BONUS + WONDER_BONUS);
        assert_eq!(tally.bonus, DOMINO_BONUS + WONDER_BONUS);
        assert_eq!(tally.best_chain, 6);
    }

    #[test]
    fn overkill_is_kicking_supports_of_an_already_cleared_tower_once_per_volley() {
        let mut tally = Tally::default();
        let mut events = Events::new();
        tally.fired(0b001, &mut events);
        tally.knocked(
            1,
            false,
            Vec3::ZERO,
            [true, false, false, false],
            &mut events,
        );
        assert!(callouts(&events).is_empty(), "tower 1 still has targets");
        tally.knocked(0, false, Vec3::ZERO, [true, false, true, true], &mut events);
        tally.knocked(0, false, Vec3::ZERO, [true, true, true, true], &mut events);
        assert_eq!(callouts(&events), [Callout::Overkill]);
        assert_eq!(tally.points, 3 * SUPPORT_POINTS + OVERKILL_BONUS);
        assert_eq!(tally.overkills, 1);
    }

    #[test]
    fn misses_are_declared_after_the_window_and_a_late_topple_retracts_them() {
        let mut tally = Tally::default();
        let mut events = Events::new();
        tally.tick(10.0, &mut events);
        assert!(events.as_slice().is_empty(), "no shot, no miss");
        tally.fired(0, &mut events);
        tally.tick(MISS_AFTER - 0.01, &mut events);
        assert_eq!(tally.misses, 0);
        tally.tick(MISS_AFTER, &mut events);
        tally.tick(MISS_AFTER + 1.0, &mut events);
        assert_eq!((tally.misses, tally.miss_streak), (1, 1));
        assert_eq!(events.as_slice(), [RangeEvent::Miss { streak: 1 }]);
        // A second unanswered shot is closed by the third shot.
        tally.fired(0, &mut events);
        tally.fired(0, &mut events);
        assert_eq!((tally.misses, tally.miss_streak), (2, 2));
        tally.tick(MISS_AFTER, &mut events);
        assert_eq!((tally.misses, tally.miss_streak), (3, 3));
        events.clear();
        tally.knocked(
            2,
            false,
            Vec3::ZERO,
            [true, false, false, false],
            &mut events,
        );
        assert_eq!((tally.misses, tally.miss_streak), (2, 0));
        assert_eq!(callouts(&events), [Callout::Delayed]);
    }

    #[test]
    fn bell_wobble_and_spare_bonus_are_single_shot() {
        let mut tally = Tally::default();
        let mut events = Events::new();
        tally.bell(BELL, &mut events);
        tally.bell(BELL, &mut events);
        tally.wobble(1, Vec3::ZERO, &mut events);
        tally.wobble(1, Vec3::ZERO, &mut events);
        tally.won(5, Vec3::ZERO, &mut events);
        assert_eq!(tally.points, BELL_BONUS + 5 * SPARE_BONUS);
        assert_eq!(callouts(&events), [Callout::Bell, Callout::Spare(5)]);
        assert_eq!(
            events
                .as_slice()
                .iter()
                .filter(|e| matches!(e, RangeEvent::Wobble { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn stars_reward_fewer_shells_and_the_next_goal_is_reachable() {
        assert_eq!(stars(true, 2), 3);
        assert_eq!(stars(true, 3), 3);
        assert_eq!(stars(true, 4), 2);
        assert_eq!(stars(true, 5), 2);
        assert_eq!(stars(true, 8), 1);
        assert_eq!(stars(false, 1), 0);
        for current in 0..3 {
            let shots = next_star_shots(current).unwrap();
            assert_eq!(stars(true, shots), current + 1);
        }
        assert_eq!(next_star_shots(3), None);
    }

    #[test]
    fn event_storage_is_bounded() {
        let mut events = Events::new();
        let capacity = events.capacity();
        for _ in 0..100 {
            events.push(RangeEvent::Miss { streak: 1 });
        }
        assert_eq!(events.as_slice().len(), EVENT_CAPACITY);
        assert_eq!(events.capacity(), capacity);
    }
}
