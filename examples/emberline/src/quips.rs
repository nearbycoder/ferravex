//! Deterministic, table-driven personality: falls, lanterns, pause and results.
//!
//! Every selector is a pure function of accepted simulation counters, so the
//! same run always reads the same way and no clock, RNG or render cadence is
//! involved. Consecutive selections never repeat the previous line.
use ferravex_play::gameplay::trial::Medal;

/// Why the last fall happened. Chosen by simulation, read by presentation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum DeathCause {
    #[default]
    Spikes,
    Fissure,
}

const SPIKES: [&str; 8] = [
    "SPIKES 1, YOU 0",
    "THAT ONE HAD A POINT",
    "OUCH. ALSO: OUCH.",
    "THE SPIKES SEND THEIR REGARDS",
    "STICK THE LANDING, NOT THE SPIKES",
    "POKED. RESPECTFULLY.",
    "SHARP OBSERVATION: DON'T",
    "THAT'S GOING IN THE BLOOPER REEL",
];

const FISSURE: [&str; 8] = [
    "THE FLOOR IS LAVA. LITERALLY.",
    "GRAVITY: UNDEFEATED",
    "A BOLD STEP INTO NOTHING",
    "THE DARK SAYS HI",
    "MIND THE GAP. PLEASE.",
    "YOU FELL FOR IT",
    "WARM DOWN THERE, ISN'T IT?",
    "THE FISSURE THANKS YOU",
];

/// Lines for a stubborn section: three or more falls since the last lantern.
const STUBBORN: [&str; 6] = [
    "PERSISTENCE IS A FLAME TOO",
    "PRACTICE MAKES PLATFORMS",
    "THIS IS THE ONE. PROBABLY.",
    "THE LANTERN BELIEVES IN YOU",
    "SAME SPOT? BOLD STRATEGY",
    "DETERMINATION: MAXIMUM",
];

const LANTERN: [&str; 4] = [
    "RESPAWN SAVED. FALL WITH CONFIDENCE.",
    "THE DARK IS NOW SLIGHTLY LESS DARK",
    "CHECKPOINT GET. NOTHING CAN STOP YOU.",
    "WARM HANDS, SAVED PROGRESS",
];

const PAUSE: [&str; 5] = [
    "THE EMBERS WILL WAIT. PROBABLY.",
    "STRETCH THOSE THUMBS",
    "THE SPIKES ARE ALSO TAKING FIVE",
    "TIME IS FROZEN. VERY SPOOKY.",
    "HYDRATE. THE LANTERN INSISTS.",
];

/// Advances by a stride coprime with the table length, so the same table
/// never repeats on consecutive counters and every line is visited.
fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let stride = if table.len().is_multiple_of(5) { 3 } else { 5 };
    table[(counter as usize).wrapping_mul(stride) % table.len()]
}

/// One fall quip. `deaths` is the run's total including this fall;
/// `deaths_here` counts falls since the last lantern (or the trailhead).
pub(super) fn fall(cause: DeathCause, deaths: u32, deaths_here: u32) -> &'static str {
    if deaths_here >= 3 && deaths_here % 2 == 1 {
        return rotate(&STUBBORN, deaths_here / 2);
    }
    match cause {
        DeathCause::Spikes => rotate(&SPIKES, deaths),
        DeathCause::Fissure => rotate(&FISSURE, deaths),
    }
}

pub(super) fn lantern(index: usize) -> &'static str {
    LANTERN[index.saturating_sub(1) % LANTERN.len()]
}

pub(super) fn pause(seed: u32) -> &'static str {
    rotate(&PAUSE, seed)
}

const CLOSE_CALL: [&str; 6] = [
    "CLOSE CALL!",
    "SPICY!",
    "WHEW!",
    "TOO CLOSE!",
    "NICE SKIM!",
    "TOES INTACT!",
];

/// `count` is the run's close calls including this one.
pub(super) fn close_call(count: u32) -> &'static str {
    rotate(&CLOSE_CALL, count.saturating_sub(1))
}

/// What a finished run gets called on the results card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
}

/// The finished run's summary, independent of storage and rendering.
#[derive(Clone, Copy, Debug)]
pub(super) struct RunSummary {
    pub medal: Medal,
    pub falls: u32,
    pub embers: usize,
    pub total_embers: usize,
    pub close_calls: u32,
    pub secret: bool,
}

/// Silly, never mean, ordered from rarest feat to the general medal tiers.
pub(super) fn rank(run: RunSummary) -> Rank {
    let all = run.embers == run.total_embers;
    let (title, quip) = if run.secret && all && run.falls == 0 {
        (
            "KEEPER OF ALL LIGHT",
            "EVERY EMBER, THE MOTH AND NOT ONE FALL. LEGEND.",
        )
    } else if run.secret {
        (
            "FRIEND OF MOTHS",
            "THE MOTH WILL TELL ITS GRANDKIDS ABOUT YOU",
        )
    } else if all && run.falls == 0 {
        ("THE LAMPLIGHTER", "FLAWLESS AND FULLY STOCKED. SHOW-OFF.")
    } else if all {
        (
            "EMBER HOARDER",
            "EVERY LAST SPARK. YOUR POCKETS ARE TOASTY.",
        )
    } else if run.falls >= 10 {
        ("PROFESSIONAL FALLER", "GRAVITY THANKS YOU FOR YOUR LOYALTY")
    } else if run.falls >= 4 {
        ("GRAVITY'S FAVOURITE", "THE FISSURES KNOW YOU BY NAME NOW")
    } else if run.close_calls >= 3 {
        (
            "SPIKE WHISPERER",
            "YOU AND DANGER ARE ON A FIRST-NAME BASIS",
        )
    } else {
        match run.medal {
            Medal::Gold if run.falls == 0 => {
                ("SPARK OF SPEED", "BLINK AND THE LANTERNS MISSED YOU")
            }
            Medal::Gold => ("QUICKWICK", "SHORT WICK, FAST FLAME"),
            Medal::Silver => ("STEADY FLAME", "SOLID. WARM. DEPENDABLE. LIKE SOUP."),
            Medal::Bronze => ("SLOW BURNER", "THE SCENIC ROUTE. THE SCENERY APPROVES."),
            Medal::Unranked => (
                "CANDLE IN THE WIND",
                "YOU MADE IT. THAT'S WHAT COUNTS. MOSTLY.",
            ),
        }
    };
    Rank { title, quip }
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
pub(super) fn next_medal(medal: Medal, targets: [f32; 3]) -> Option<(&'static str, f32)> {
    match medal {
        Medal::Gold => None,
        Medal::Silver => Some(("GOLD", targets[0])),
        Medal::Bronze => Some(("SILVER", targets[1])),
        Medal::Unranked => Some(("BRONZE", targets[2])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consecutive_falls_never_repeat_a_quip_and_tables_are_all_reachable() {
        for cause_pattern in 0..4_u32 {
            let mut previous = "";
            let mut here = 0;
            let mut seen = std::collections::BTreeSet::new();
            for deaths in 1..200 {
                // Mix causes and occasional lantern resets deterministically.
                let cause = if (deaths + cause_pattern) % 3 == 0 {
                    DeathCause::Fissure
                } else {
                    DeathCause::Spikes
                };
                here = if deaths % 7 == 0 { 1 } else { here + 1 };
                let line = fall(cause, deaths, here);
                assert_ne!(line, previous, "fall {deaths} repeated {line}");
                previous = line;
                seen.insert(line);
            }
            assert!(seen.len() >= 16, "rotation explores the tables");
        }
        for table in [
            &SPIKES[..],
            &FISSURE[..],
            &STUBBORN[..],
            &PAUSE[..],
            &CLOSE_CALL[..],
        ] {
            let visited: std::collections::BTreeSet<_> =
                (0..table.len() as u32).map(|n| rotate(table, n)).collect();
            assert_eq!(visited.len(), table.len());
        }
    }

    #[test]
    fn ranks_reward_rare_feats_first_and_every_medal_has_a_title() {
        let base = RunSummary {
            medal: Medal::Gold,
            falls: 0,
            embers: 2,
            total_embers: 12,
            close_calls: 0,
            secret: false,
        };
        assert_eq!(rank(base).title, "SPARK OF SPEED");
        assert_eq!(rank(RunSummary { falls: 1, ..base }).title, "QUICKWICK");
        assert_eq!(
            rank(RunSummary { embers: 12, ..base }).title,
            "THE LAMPLIGHTER"
        );
        assert_eq!(
            rank(RunSummary {
                embers: 12,
                secret: true,
                ..base
            })
            .title,
            "KEEPER OF ALL LIGHT"
        );
        assert_eq!(
            rank(RunSummary {
                secret: true,
                falls: 3,
                ..base
            })
            .title,
            "FRIEND OF MOTHS"
        );
        assert_eq!(
            rank(RunSummary { falls: 12, ..base }).title,
            "PROFESSIONAL FALLER"
        );
        assert_eq!(
            rank(RunSummary {
                close_calls: 3,
                ..base
            })
            .title,
            "SPIKE WHISPERER"
        );
        for medal in [Medal::Silver, Medal::Bronze, Medal::Unranked] {
            let r = rank(RunSummary { medal, ..base });
            assert!(!r.title.is_empty() && !r.quip.is_empty());
        }
        assert_eq!(
            next_medal(Medal::Silver, [12., 18., 30.]),
            Some(("GOLD", 12.))
        );
        assert_eq!(next_medal(Medal::Gold, [12., 18., 30.]), None);
    }
}
