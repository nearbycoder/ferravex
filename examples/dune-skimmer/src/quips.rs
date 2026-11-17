//! KDUNE desert radio: deterministic, table-driven personality.
//!
//! Every selector is a pure function of accepted race counters, so the same
//! run always reads the same way. No clock, RNG or render cadence is involved,
//! and consecutive picks from one table never repeat.
use ferravex_play::gameplay::trial::Medal;

/// Medal targets in race seconds (penalties included): gold, silver, bronze.
/// Gold sits just under Dusty, the demo bot's autoplay time (29.92s).
pub(super) const MEDALS: [f32; 3] = [29.5, 32.5, 37.0];
#[cfg(test)]
pub(super) const DUSTY: f32 = 29.92;

// Radio lines play during the race, so each is a callout of at most three
// words; the longer jokes live on the pause and results cards.
const START: [&str; 6] = [
    "CONDITIONS: DRY.",
    "CAMELS PLACED BETS",
    "FORECAST: SAND.",
    "GATES DON'T MOVE",
    "DUSTY SAYS HI",
    "SPONSOR: SUNSCREEN",
];

const ROCKET: [&str; 4] = [
    "SAND DIDN'T SEE",
    "CHECK THE PAINT",
    "DUSTY TAKES NOTES",
    "REALLY OFF!",
];

const EAGER: [&str; 4] = [
    "GAS ON '1'!",
    "ENGINE WAS YAWNING",
    "PATIENCE, PILOT",
    "SUCH ENTHUSIASM",
];

const STREAK: [&str; 6] = [
    "TAILOR ON DEADLINE",
    "GATES ARE BLUSHING",
    "NAME A DUNE!",
    "SURGICAL.",
    "PURE EMBROIDERY",
    "GET A NEEDLE",
];

const SCRAPE: [&str; 6] = [
    "ROCK FELT THAT",
    "PAINT? WHAT PAINT?",
    "READ ITS DIARY",
    "BOULDER FLINCHED",
    "SPICY LINE!",
    "MECHANIC IS CRYING",
];

const IMPACT: [&str; 8] = [
    "ROCK WAS FIRST",
    "SANDSTONE WINS",
    "THAT STOPPED YOU",
    "PILLAR IS UPSET",
    "BONK. RESPECTFULLY.",
    "IT DIDN'T MOVE",
    "ROCKS ARE UNIONIZED",
    "PHYSICS: UNDEFEATED",
];

const STREAK_LOST: [&str; 4] = [
    "NEEDLE DROPPED",
    "THE THREAD FRAYED",
    "GATES EXHALE",
    "BUILDS CHARACTER",
];

const RECOVERY: [&str; 6] = [
    "CAMEL TOW. +3s",
    "RECALCULATING...",
    "TRACK IS SMALLER",
    "SCENIC DETOUR. +3s",
    "FOUND YOU. AGAIN.",
    "SAND RETURNS YOU",
];

const SAND: [&str; 4] = [
    "THAT'S JUST SAND",
    "BOOST ON BREAK",
    "NOT A STRATEGY",
    "DUNES WANT HUGS",
];

const PAUSE: [&str; 5] = [
    "THE SAND WILL WAIT. IT HAS TIME.",
    "HYDRATE. THE CAMELS INSIST.",
    "DUSTY IS ALSO TAKING FIVE",
    "STRETCH THOSE THUMBS",
    "THE SUN IS NOT PAUSED. BUT YOU ARE.",
];

/// Advances by a stride coprime with the table length, so the same table
/// never repeats on consecutive counters and every line is visited.
fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let stride = if table.len().is_multiple_of(5) { 3 } else { 5 };
    table[(counter as usize).wrapping_mul(stride) % table.len()]
}

/// `attempt` counts recorded finishes; it only changes between races.
pub(super) fn start(attempt: u32) -> &'static str {
    rotate(&START, attempt)
}

pub(super) fn launch(rocket: bool, attempt: u32) -> &'static str {
    if rocket {
        rotate(&ROCKET, attempt)
    } else {
        rotate(&EAGER, attempt)
    }
}

/// Whether a threaded streak just reached a radio-worthy milestone.
pub(super) fn streak_milestone(streak: u32) -> bool {
    streak == 3 || (streak >= 5 && streak.is_multiple_of(5))
}

/// `said` counts streak milestones already announced this race.
pub(super) fn streak(said: u32) -> &'static str {
    rotate(&STREAK, said)
}

pub(super) fn streak_callout(streak: u32) -> &'static str {
    match streak {
        0..=1 => "THREADED!",
        2 => "DOUBLE THREAD!",
        3 => "HAT TRICK!",
        4 => "QUAD THREAD!",
        5..=9 => "MAX MULTIPLIER!",
        10..=13 => "NEEDLEWORK!",
        _ => "UNSTOPPABLE!",
    }
}

/// `count` includes this scrape.
pub(super) fn scrape(count: u32) -> &'static str {
    rotate(&SCRAPE, count.saturating_sub(1))
}

/// `count` includes this impact.
pub(super) fn impact(count: u32) -> &'static str {
    rotate(&IMPACT, count.saturating_sub(1))
}

pub(super) fn streak_lost(count: u32) -> &'static str {
    rotate(&STREAK_LOST, count)
}

/// `count` includes this recovery.
pub(super) fn recovery(count: u32) -> &'static str {
    rotate(&RECOVERY, count.saturating_sub(1))
}

pub(super) fn sand(count: u32) -> &'static str {
    rotate(&SAND, count)
}

pub(super) fn pause(seed: u32) -> &'static str {
    rotate(&PAUSE, seed)
}

/// Commentary for a completed lap. `lap` is 1-based; `delta` compares with the
/// previous lap (negative is faster). The banner already names the lap, so
/// the radio only reacts.
pub(super) fn lap(lap: u32, laps: u32, perfect: bool, delta: Option<f32>) -> &'static str {
    if perfect {
        "FULL TANK, FREE!"
    } else if lap + 1 == laps {
        match delta {
            Some(d) if d < 0. => "CAMELS ARE STANDING",
            _ => "CAMELS ARE UP",
        }
    } else {
        match delta {
            Some(d) if d < -0.5 => "SPICY. AGAIN!",
            Some(d) if d < 0. => "QUICKER!",
            Some(_) => "NICE VIEW, THOUGH",
            None => "SUN STILL HOT",
        }
    }
}

/// What a finished run gets called on the results card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
}

/// The finished race's summary, independent of storage and rendering.
#[derive(Clone, Copy, Debug)]
pub(super) struct RunSummary {
    pub medal: Medal,
    pub threaded: u32,
    pub gates: u32,
    pub best_streak: u32,
    pub perfect_laps: u32,
    pub scrapes: u32,
    pub impacts: u32,
    pub recoveries: u32,
    pub rocket: bool,
}

/// Silly, never mean; rarest feats first, then the general medal tiers.
pub(super) fn rank(run: RunSummary) -> Rank {
    let (title, quip) = if run.perfect_laps >= 1 && run.threaded == run.gates {
        ("SAND SURGEON", "42 GATES. 42 BULLSEYES.")
    } else if run.perfect_laps >= 1 {
        ("MIRAGE MAKER", "A PERFECT LAP. WAS IT REAL?")
    } else if run.recoveries >= 3 {
        ("CAMEL'S FAVOURITE", "THE TOW SERVICE KNOWS YOU BY NAME")
    } else if run.impacts >= 8 {
        ("ROCK ENTHUSIAST", "EVERY BOULDER, PERSONALLY")
    } else if run.scrapes >= 4 {
        ("PAINT SCRAPER", "ON FIRST NAMES WITH ROCKS")
    } else if run.best_streak >= 14 {
        ("THREAD COUNT: HIGH", "LUXURY-GRADE GATE THREADING")
    } else {
        match run.medal {
            Medal::Gold if run.impacts == 0 => ("DUNE DEMON", "DUSTY IS IN SHAMBLES"),
            Medal::Gold => ("SANDSTORM", "FASTER THAN DUSTY. TELL EVERYONE."),
            Medal::Silver => ("HOT WIND", "QUICK, DRY AND MOSTLY ON THE TRACK"),
            Medal::Bronze => ("TUMBLEWEED", "ROLLING ALONG AT A RESPECTABLE CLIP"),
            Medal::Unranked if run.rocket => ("ALL LAUNCH, NO LUNCH", "WHAT A START, THOUGH"),
            Medal::Unranked => ("SUNDAY DRIVER", "THE SCENIC ROUTE. THE CACTI WAVED."),
        }
    };
    Rank { title, quip }
}

pub(super) fn medal_for(time: f32) -> Medal {
    if time <= MEDALS[0] + 1e-4 {
        Medal::Gold
    } else if time <= MEDALS[1] + 1e-4 {
        Medal::Silver
    } else if time <= MEDALS[2] + 1e-4 {
        Medal::Bronze
    } else {
        Medal::Unranked
    }
}

/// The authored target above `medal`, as (medal, seconds), if one exists.
pub(super) fn next_medal(medal: Medal) -> Option<(Medal, f32)> {
    match medal {
        Medal::Gold => None,
        Medal::Silver => Some((Medal::Gold, MEDALS[0])),
        Medal::Bronze => Some((Medal::Silver, MEDALS[1])),
        Medal::Unranked => Some((Medal::Bronze, MEDALS[2])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_never_repeat_back_to_back_and_visit_every_line() {
        for table in [
            &START[..],
            &ROCKET[..],
            &EAGER[..],
            &STREAK[..],
            &SCRAPE[..],
            &IMPACT[..],
            &STREAK_LOST[..],
            &RECOVERY[..],
            &SAND[..],
            &PAUSE[..],
        ] {
            let visited: std::collections::BTreeSet<_> =
                (0..table.len() as u32).map(|n| rotate(table, n)).collect();
            assert_eq!(visited.len(), table.len());
            for n in 0..50 {
                assert_ne!(rotate(table, n), rotate(table, n + 1));
            }
            for line in table {
                // Toasts are single-line; keep them short enough to fit.
                assert!(line.len() <= 56, "{line}");
            }
        }
        // Radio lines play mid-race: callouts of at most three words.
        for table in [
            &START[..],
            &ROCKET[..],
            &EAGER[..],
            &STREAK[..],
            &SCRAPE[..],
            &IMPACT[..],
            &STREAK_LOST[..],
            &RECOVERY[..],
            &SAND[..],
        ] {
            for line in table {
                assert!(line.split_whitespace().count() <= 3, "{line}");
            }
        }
        for delta in [None, Some(-1.), Some(-0.1), Some(0.3)] {
            for (lap_number, perfect) in [(1, false), (2, false), (2, true)] {
                let line = lap(lap_number, 3, perfect, delta);
                assert!(line.split_whitespace().count() <= 3, "{line}");
            }
        }
        assert!(!streak_milestone(2) && streak_milestone(3) && !streak_milestone(4));
        assert!(streak_milestone(5) && streak_milestone(10) && !streak_milestone(11));
        for delta in [None, Some(-1.), Some(-0.1), Some(0.3)] {
            for (lap_number, perfect) in [(1, false), (2, false), (2, true)] {
                assert!(lap(lap_number, 3, perfect, delta).len() <= 56);
            }
        }
    }

    #[test]
    fn ranks_reward_rare_feats_first_and_every_medal_has_a_title() {
        let base = RunSummary {
            medal: Medal::Gold,
            threaded: 30,
            gates: 42,
            best_streak: 8,
            perfect_laps: 0,
            scrapes: 0,
            impacts: 0,
            recoveries: 0,
            rocket: false,
        };
        assert_eq!(rank(base).title, "DUNE DEMON");
        assert_eq!(rank(RunSummary { impacts: 1, ..base }).title, "SANDSTORM");
        assert_eq!(
            rank(RunSummary {
                perfect_laps: 1,
                ..base
            })
            .title,
            "MIRAGE MAKER"
        );
        assert_eq!(
            rank(RunSummary {
                perfect_laps: 3,
                threaded: 42,
                ..base
            })
            .title,
            "SAND SURGEON"
        );
        assert_eq!(
            rank(RunSummary {
                recoveries: 3,
                ..base
            })
            .title,
            "CAMEL'S FAVOURITE"
        );
        assert_eq!(
            rank(RunSummary { scrapes: 5, ..base }).title,
            "PAINT SCRAPER"
        );
        for medal in [Medal::Silver, Medal::Bronze, Medal::Unranked] {
            let r = rank(RunSummary { medal, ..base });
            assert!(!r.title.is_empty() && !r.quip.is_empty());
            assert!(r.quip.len() <= 56, "{}", r.quip);
        }
        assert_eq!(
            rank(RunSummary {
                medal: Medal::Unranked,
                rocket: true,
                ..base
            })
            .title,
            "ALL LAUNCH, NO LUNCH"
        );
    }

    #[test]
    fn medals_follow_authored_targets_and_dusty_is_silver() {
        assert_eq!(medal_for(MEDALS[0]), Medal::Gold);
        assert_eq!(medal_for(DUSTY), Medal::Silver);
        assert_eq!(medal_for(MEDALS[2] + 0.01), Medal::Unranked);
        assert_eq!(next_medal(Medal::Silver), Some((Medal::Gold, MEDALS[0])));
        assert_eq!(next_medal(Medal::Gold), None);
        const { assert!(MEDALS[0] < DUSTY) };
    }
}
