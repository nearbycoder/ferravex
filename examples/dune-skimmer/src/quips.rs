//! KDUNE desert radio: deterministic, table-driven personality.
//!
//! Every selector is a pure function of accepted race counters, so the same
//! run always reads the same way. No clock, RNG or render cadence is involved,
//! and consecutive picks from one table never repeat.
use ferravex_play::gameplay::trial::Medal;

/// Medal targets in race seconds (penalties included): gold, silver, bronze.
/// Gold sits just under Dusty, the demo bot's autoplay time (29.92s).
pub(super) const MEDALS: [f32; 3] = [29.5, 32.5, 37.0];
pub(super) const DUSTY: f32 = 29.92;

const START: [&str; 6] = [
    "GOOD MORNING, SAND FANS. CONDITIONS: DRY.",
    "THE CAMELS HAVE PLACED THEIR BETS",
    "FORECAST: 100% CHANCE OF SAND",
    "REMEMBER: THE GATES DO NOT MOVE. YOU DO.",
    "DUSTY THE DEMO BOT SAYS HI. AND GOOD LUCK.",
    "TODAY'S SPONSOR: SUNSCREEN",
];

const ROCKET: [&str; 4] = [
    "ROCKET START! THE SAND DIDN'T SEE THAT COMING",
    "WHAT A LAUNCH. SOMEBODY CHECK THE PAINT.",
    "PERFECT JUMP. DUSTY IS TAKING NOTES.",
    "AND THEY'RE OFF. LIKE, REALLY OFF.",
];

const EAGER: [&str; 4] = [
    "EAGER! TAP THE GAS ON '1' FOR A ROCKET START",
    "TOO SOON. THE ENGINE WAS STILL YAWNING.",
    "PATIENCE, PILOT. PRESS ON THE '1'.",
    "THE THROTTLE APPRECIATES THE ENTHUSIASM",
];

const STREAK: [&str; 6] = [
    "THREADING LIKE A TAILOR WITH A DEADLINE",
    "THE GATES ARE BLUSHING",
    "ONE MORE AND WE NAME A DUNE AFTER YOU",
    "SURGICAL. THE PILLARS FEEL SEEN.",
    "THAT'S NOT DRIVING, THAT'S EMBROIDERY",
    "SOMEBODY GET THIS PILOT A NEEDLE",
];

const SCRAPE: [&str; 6] = [
    "THAT ROCK FELT A BREEZE",
    "PAINT? WHAT PAINT?",
    "CLOSE ENOUGH TO READ THE ROCK'S DIARY",
    "THE BOULDER FLINCHED",
    "SPICY LINE!",
    "SCRAPE BONUS. YOUR MECHANIC IS CRYING.",
];

const IMPACT: [&str; 8] = [
    "THE ROCK WAS THERE FIRST",
    "SANDSTONE: 1, SKIMMER: 0",
    "THAT'S ONE WAY TO STOP",
    "THE PILLAR WOULD LIKE A WORD",
    "BONK. RESPECTFULLY.",
    "DID SOMEONE MOVE THAT? NO. NO THEY DID NOT.",
    "THE ROCKS ARE UNIONIZED. THEY DON'T MOVE.",
    "PHYSICS: STILL UNDEFEATED",
];

const STREAK_LOST: [&str; 4] = [
    "STREAK SNAPPED. THE NEEDLE HAS BEEN DROPPED.",
    "AND THE THREAD... HAS FRAYED",
    "STREAK OVER. THE GATES EXHALE.",
    "BACK TO ONE. IT BUILDS CHARACTER.",
];

const RECOVERY: [&str; 6] = [
    "A PASSING CAMEL TOWED YOU BACK. +3s.",
    "RECALCULATING ROUTE...",
    "THE DESERT IS BIG. THE TRACK IS NOT.",
    "SCENIC DETOUR COMPLETE. +3s.",
    "WE FOUND YOU. YOU WERE NEAR A ROCK. AGAIN.",
    "THE SAND RETURNS YOU, SLIGHTLY EMBARRASSED",
];

const SAND: [&str; 4] = [
    "THAT'S NOT TRACK, THAT'S JUST SAND",
    "LOOSE SAND! BOOST IS ON A COFFEE BREAK",
    "OFF-ROADING IS A LIFESTYLE, NOT A STRATEGY",
    "THE DUNES WANT A HUG. SAY NO.",
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
/// previous lap (negative is faster).
pub(super) fn lap(lap: u32, laps: u32, perfect: bool, delta: Option<f32>) -> &'static str {
    if perfect {
        "MIRAGE LAP! ALL 14 THREADED. FULL TANK, ON THE HOUSE."
    } else if lap + 1 == laps {
        match delta {
            Some(d) if d < 0. => "FINAL LAP! FASTER EVERY TIME. THE CAMELS ARE STANDING.",
            _ => "FINAL LAP! THE CAMELS ARE ON THEIR FEET.",
        }
    } else {
        match delta {
            Some(d) if d < -0.5 => "THAT LAP WAS SPICY. DO IT AGAIN.",
            Some(d) if d < 0. => "QUICKER! THE SAND IS LEARNING YOUR NAME.",
            Some(_) => "A LITTLE SLOWER. THE VIEW WAS NICE, THOUGH.",
            None => "LAP ONE IN THE BOOKS. THE SUN IS STILL HOT.",
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
        (
            "SAND SURGEON",
            "FORTY-TWO GATES, FORTY-TWO BULLSEYES. SCALPEL, PLEASE.",
        )
    } else if run.perfect_laps >= 1 {
        (
            "MIRAGE MAKER",
            "A WHOLE LAP OF PERFECTION. WAS IT EVEN REAL?",
        )
    } else if run.recoveries >= 3 {
        ("CAMEL'S FAVOURITE", "THE TOW SERVICE KNOWS YOU BY NAME")
    } else if run.impacts >= 8 {
        ("ROCK ENTHUSIAST", "YOU VISITED EVERY BOULDER. PERSONALLY.")
    } else if run.scrapes >= 4 {
        (
            "PAINT SCRAPER",
            "YOU AND THE ROCKS ARE ON A FIRST-NAME BASIS",
        )
    } else if run.best_streak >= 14 {
        ("THREAD COUNT: HIGH", "LUXURY-GRADE GATE THREADING")
    } else {
        match run.medal {
            Medal::Gold if run.impacts == 0 => {
                ("DUNE DEMON", "CLEAN AND QUICK. DUSTY IS IN SHAMBLES.")
            }
            Medal::Gold => ("SANDSTORM", "FASTER THAN THE DEMO BOT. TELL EVERYONE."),
            Medal::Silver => ("HOT WIND", "QUICK, DRY AND MOSTLY ON THE TRACK"),
            Medal::Bronze => ("TUMBLEWEED", "ROLLING ALONG AT A RESPECTABLE CLIP"),
            Medal::Unranked if run.rocket => (
                "ALL LAUNCH, NO LUNCH",
                "WHAT A START. THE REST WAS A JOURNEY.",
            ),
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

pub(super) fn medal_name(medal: Medal) -> &'static str {
    match medal {
        Medal::Gold => "GOLD",
        Medal::Silver => "SILVER",
        Medal::Bronze => "BRONZE",
        Medal::Unranked => "NO MEDAL",
    }
}

/// The authored target above `medal`, as (name, seconds), if one exists.
pub(super) fn next_medal(medal: Medal) -> Option<(&'static str, f32)> {
    match medal {
        Medal::Gold => None,
        Medal::Silver => Some(("GOLD", MEDALS[0])),
        Medal::Bronze => Some(("SILVER", MEDALS[1])),
        Medal::Unranked => Some(("BRONZE", MEDALS[2])),
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
        assert_eq!(next_medal(Medal::Silver), Some(("GOLD", MEDALS[0])));
        assert_eq!(next_medal(Medal::Gold), None);
        const { assert!(MEDALS[0] < DUSTY) };
    }
}
