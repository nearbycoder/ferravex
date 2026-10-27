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
    "Spikes 1, you 0.",
    "That one had a point.",
    "Ouch. Also: ouch.",
    "The spikes send their regards.",
    "Stick the landing, not the spikes.",
    "Poked. Respectfully.",
    "Sharp observation: don't.",
    "That's going in the blooper reel.",
];

const FISSURE: [&str; 8] = [
    "The floor is lava. Literally.",
    "Gravity: still undefeated.",
    "A bold step into nothing.",
    "The dark says hi.",
    "Mind the gap. Please.",
    "You fell for it.",
    "Warm down there, isn't it?",
    "The fissure thanks you.",
];

/// Lines for a stubborn section: three or more falls since the last lantern.
const STUBBORN: [&str; 6] = [
    "Persistence is a flame too.",
    "Practice makes platforms.",
    "This is the one. Probably.",
    "The lantern believes in you.",
    "Same spot? Bold strategy.",
    "Determination: maximum.",
];

const LANTERN: [&str; 4] = [
    "Respawn saved. Fall with confidence.",
    "The dark is now slightly less dark.",
    "Checkpoint get. Nothing can stop you now.",
    "Warm hands, saved progress.",
];

const PAUSE: [&str; 5] = [
    "The embers will wait. Probably.",
    "Stretch those thumbs.",
    "The spikes are also taking five.",
    "Time is frozen. Very spooky.",
    "Hydrate. The lantern insists.",
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
            "Every ember, the moth and not one fall. Legend.",
        )
    } else if run.secret {
        (
            "FRIEND OF MOTHS",
            "The moth will tell its grandkids about you.",
        )
    } else if all && run.falls == 0 {
        ("THE LAMPLIGHTER", "Flawless and fully stocked. Show-off.")
    } else if all {
        (
            "EMBER HOARDER",
            "Every last spark. Your pockets are toasty.",
        )
    } else if run.falls >= 10 {
        (
            "PROFESSIONAL FALLER",
            "Gravity thanks you for your loyalty.",
        )
    } else if run.falls >= 4 {
        ("GRAVITY'S FAVOURITE", "The fissures know you by name now.")
    } else if run.close_calls >= 3 {
        (
            "SPIKE WHISPERER",
            "You and danger are on a first-name basis.",
        )
    } else {
        match run.medal {
            Medal::Gold if run.falls == 0 => {
                ("SPARK OF SPEED", "Blink and the lanterns missed you.")
            }
            Medal::Gold => ("QUICKWICK", "Short wick, fast flame."),
            Medal::Silver => ("STEADY FLAME", "Solid. Warm. Dependable. Like soup."),
            Medal::Bronze => ("SLOW BURNER", "The scenic route. The scenery approves."),
            Medal::Unranked => (
                "CANDLE IN THE WIND",
                "You made it. That's what counts. Mostly.",
            ),
        }
    };
    Rank { title, quip }
}

/// Mixed-case medal name for Modern-face copy.
pub(super) fn medal_label(medal: Medal) -> &'static str {
    match medal {
        Medal::Gold => "Gold",
        Medal::Silver => "Silver",
        Medal::Bronze => "Bronze",
        Medal::Unranked => "No medal",
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
pub(super) fn next_medal(medal: Medal, targets: [f32; 3]) -> Option<(&'static str, f32)> {
    match medal {
        Medal::Gold => None,
        Medal::Silver => Some(("gold", targets[0])),
        Medal::Bronze => Some(("silver", targets[1])),
        Medal::Unranked => Some(("bronze", targets[2])),
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
            Some(("gold", 12.))
        );
        assert_eq!(next_medal(Medal::Gold, [12., 18., 30.]), None);
    }
}
