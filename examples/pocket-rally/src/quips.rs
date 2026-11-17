//! Table-driven rally personality: a cheeky co-driver who loves the farm.
//!
//! Every selector is a pure function of accepted race counters, so the same
//! race always reads the same way. No clock, RNG or render cadence is involved,
//! and consecutive selections from one table never repeat the previous line.
use crate::style::Stats;
use ferravex_play::gameplay::trial::Medal;

// In-race callouts stay at three words or fewer; longer jokes live only on
// the pause and results cards. The first line of each guidance table is the
// plain message, so the first time a player meets it the words are clearest.
const OFF_TRACK: [&str; 8] = [
    "SCENIC ROUTE SELECTED",
    "THAT'S A FIELD",
    "COWS DISAGREE",
    "FREE LAWN MOWING",
    "TRACTOR MODE",
    "ROAD MISSES YOU",
    "GIULIA SAW THAT",
    "OFF-ROAD DLC",
];

const WRONG_WAY: [&str; 5] = [
    "WRONG WAY!",
    "BIT EARLY?",
    "BOLD. BACKWARDS.",
    "OTHER WAY!",
    "REVERSE RALLY?",
];

const BACK_ON_TRACK: [&str; 4] = [
    "BACK ON TRACK",
    "SWEET ASPHALT",
    "AND WE'RE BACK",
    "TIRES SAY THANKS",
];

const BOTCHED: [&str; 5] = [
    "SHEEP SAW THAT",
    "TOO MUCH ATTITUDE",
    "POINTS REPOSSESSED",
    "RETURN TO SENDER",
    "GRASS ATE IT",
];

const BUMP: [&str; 5] = ["BONK!", "FENCE SAYS NO", "BOING!", "OOF!", "STURDY FENCE!"];

const CLEAN_LAP: [&str; 3] = ["NOT ONE BLADE", "SPOTLESS!", "CO-DRIVER NODS"];

const PAUSE: [&str; 5] = [
    "ENGINE IDLING. COWS WAITING.",
    "THE CO-DRIVER IS SNACKING",
    "TIRES COOLING. OPINIONS HEATING.",
    "THE GHOST WILL WAIT. IT HAS TIME.",
    "STRETCH THOSE STEERING THUMBS",
];

const READY: [&str; 6] = [
    "AGAIN! WITH FEELING",
    "FRESH TIRES",
    "GHOST REMEMBERS YOU",
    "THIS ONE'S IT",
    "DEEP BREATH...",
    "COWS ARE WATCHING",
];

const SPLASH: [&str; 3] = ["QUACK!", "DUCKS FURIOUS", "NOT A BOAT"];

/// Advance by a stride coprime with the table length, so consecutive counters
/// never repeat a line and every line is eventually visited.
fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let stride = if table.len().is_multiple_of(5) { 3 } else { 5 };
    table[(counter as usize).wrapping_mul(stride) % table.len()]
}

/// `episode` counts off-track excursions this race, including this one.
pub fn off_track(episode: u32) -> &'static str {
    rotate(&OFF_TRACK, episode.saturating_sub(1))
}

pub fn wrong_way(episode: u32) -> &'static str {
    rotate(&WRONG_WAY, episode.saturating_sub(1))
}

pub fn back_on_track(episode: u32) -> &'static str {
    rotate(&BACK_ON_TRACK, episode.saturating_sub(1))
}

pub fn botched(count: u32) -> &'static str {
    rotate(&BOTCHED, count.saturating_sub(1))
}

pub fn bump(count: u32) -> &'static str {
    rotate(&BUMP, count.saturating_sub(1))
}

pub fn clean_lap(count: u32) -> &'static str {
    rotate(&CLEAN_LAP, count.saturating_sub(1))
}

pub fn pause(seed: u32) -> &'static str {
    rotate(&PAUSE, seed)
}

/// The countdown title: the track name first, then a retry line. The lap
/// count is drawn as pips, so the title carries no rule text.
pub fn ready(attempt: u32) -> &'static str {
    if attempt == 0 {
        "VALLE VERDE"
    } else {
        rotate(&READY, attempt - 1)
    }
}

pub fn splash(count: u32) -> &'static str {
    rotate(&SPLASH, count.saturating_sub(1))
}

/// Race-time medal thresholds in seconds (gold, silver, bronze). The autopilot,
/// which never uses the handbrake, finishes just inside silver.
pub const MEDALS: [f32; 3] = [43., 47., 55.];

pub fn medal(total: f32) -> Medal {
    match total {
        t if t <= MEDALS[0] => Medal::Gold,
        t if t <= MEDALS[1] => Medal::Silver,
        t if t <= MEDALS[2] => Medal::Bronze,
        _ => Medal::Unranked,
    }
}

/// The next medal up and its target time, if any.
pub fn next_medal(medal: Medal) -> Option<(Medal, f32)> {
    match medal {
        Medal::Gold => None,
        Medal::Silver => Some((Medal::Gold, MEDALS[0])),
        Medal::Bronze => Some((Medal::Silver, MEDALS[1])),
        Medal::Unranked => Some((Medal::Bronze, MEDALS[2])),
    }
}

/// The best medal a race can still earn once `elapsed` seconds have passed.
/// Presentation only: the live trophy beside the race clock changes colour
/// as each target slips away; the finish still awards by [`medal`].
pub fn medal_in_reach(elapsed: f32) -> Medal {
    medal(elapsed)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
}

/// Silly, never mean. Rare feats first, then driving style, then medal tiers.
/// The quip is the results card's one line of flavour.
pub fn rank(medal: Medal, stats: &Stats) -> Rank {
    let (title, quip) = if stats.splashes > 0 && medal == Medal::Gold {
        ("RALLY DUCK", "GOLD PACE AND A SWIM!")
    } else if stats.splashes > 0 {
        ("DUCK BOTHERER", "THE DUCKS FILED A COMPLAINT")
    } else if stats.donuts >= 2 {
        ("DONUT ARTIST", "THE BAKERY WANTS YOUR NUMBER")
    } else if medal == Medal::Gold && stats.perfect() && stats.style >= 2000 {
        ("VALLE VERDE LEGEND", "CLEAN, QUICK AND SIDEWAYS")
    } else if medal == Medal::Gold && stats.perfect() {
        ("PERFECT LINE", "NOT ONE BLADE OF GRASS")
    } else if stats.style >= 2500 {
        ("SIDEWAYS SPECIALIST", "WHY DRIVE FORWARDS AT ALL?")
    } else if stats.bumps >= 3 {
        ("FENCE INSPECTOR", "EVERY FENCE CHECKED. STURDY.")
    } else if stats.grass_seconds() >= 8. {
        ("AGRICULTURAL DRIVER", "THE FARMERS THANK YOU")
    } else if stats.wrong_way_episodes >= 2 {
        ("COMPASS OPTIONAL", "LAPS ARE A SUGGESTION")
    } else {
        match medal {
            Medal::Gold => ("GOLDEN GRAVEL", "THE GHOST IS TAKING NOTES"),
            Medal::Silver => ("SILVER SLIDER", "SO CLOSE THE GOLD CAN SMELL YOU"),
            Medal::Bronze => ("BRONZE BOMBER", "THE COWS APPLAUD (SLOWLY)"),
            Medal::Unranked => ("SUNDAY DRIVER", "SCENIC, RELAXED, VERY LEGAL"),
        }
    };
    Rank { title, quip }
}

/// The next goal under the result, drawn as icons: a medal chase, a
/// clean-lap chase, a style target, the duck secret, or nothing left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hook {
    /// Shave `cut` seconds to earn `medal`.
    Medal { medal: Medal, cut: f32 },
    /// Gold already; now all laps clean.
    CleanLaps(u32),
    /// Perfect gold; now reach this style score.
    Style(u32),
    /// Everything but the pond.
    Ducks,
    /// Nothing left to prove.
    Done,
}

/// The next-goal hook under the result: medal chase, clean-lap chase, secret.
pub fn hook(medal: Medal, total: f32, stats: &Stats) -> Hook {
    if let Some((medal, target)) = next_medal(medal) {
        Hook::Medal {
            medal,
            cut: (total - target).max(0.01),
        }
    } else if !stats.perfect() {
        Hook::CleanLaps(LAPS)
    } else if stats.style < 2000 {
        Hook::Style(2000)
    } else if stats.splashes == 0 {
        Hook::Ducks
    } else {
        Hook::Done
    }
}

use crate::sim::LAPS;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_never_repeat_back_to_back_and_visit_every_line() {
        for table in [
            &OFF_TRACK[..],
            &WRONG_WAY,
            &BACK_ON_TRACK,
            &BOTCHED,
            &BUMP,
            &CLEAN_LAP,
            &PAUSE,
            &READY,
            &SPLASH,
        ] {
            let mut seen = std::collections::BTreeSet::new();
            for n in 0..table.len() as u32 * 2 {
                assert_ne!(rotate(table, n), rotate(table, n + 1));
                seen.insert(rotate(table, n));
            }
            assert_eq!(seen.len(), table.len());
            assert!(table.iter().all(|line| line.len() <= 34), "{table:?}");
        }
        // Lines shown while driving are callouts, never sentences.
        for table in [
            &OFF_TRACK[..],
            &WRONG_WAY,
            &BACK_ON_TRACK,
            &BOTCHED,
            &BUMP,
            &CLEAN_LAP,
            &READY,
            &SPLASH,
        ] {
            for line in table {
                assert!(line.split_whitespace().count() <= 3, "{line}");
            }
        }
        assert_eq!(off_track(1), "SCENIC ROUTE SELECTED");
        assert_eq!(wrong_way(1), "WRONG WAY!");
        assert_eq!(back_on_track(1), "BACK ON TRACK");
        assert_eq!(ready(0), "VALLE VERDE");
        assert_ne!(ready(1), ready(2));
    }

    #[test]
    fn medals_ranks_and_hooks_always_offer_a_next_goal() {
        assert_eq!(medal(42.), Medal::Gold);
        assert_eq!(medal(43.01), Medal::Silver);
        assert_eq!(medal(50.), Medal::Bronze);
        assert_eq!(medal(90.), Medal::Unranked);
        assert_eq!(medal_in_reach(10.), Medal::Gold);
        assert_eq!(medal_in_reach(43.5), Medal::Silver);
        assert_eq!(medal_in_reach(60.), Medal::Unranked);
        let mut stats = Stats::default();
        let Hook::Medal { medal: next, cut } = hook(Medal::Silver, 44.5, &stats) else {
            panic!("silver chases gold");
        };
        assert_eq!(next, Medal::Gold);
        assert!((cut - 1.5).abs() < 1e-4);
        assert_eq!(
            hook(Medal::Unranked, 90., &stats),
            Hook::Medal {
                medal: Medal::Bronze,
                cut: 35.
            }
        );
        assert_eq!(hook(Medal::Gold, 42., &stats), Hook::CleanLaps(3));
        assert_eq!(rank(Medal::Silver, &stats).title, "SILVER SLIDER");
        stats.clean_laps = 3;
        assert_eq!(rank(Medal::Gold, &stats).title, "PERFECT LINE");
        assert_eq!(hook(Medal::Gold, 42., &stats), Hook::Style(2000));
        stats.style = 2400;
        assert_eq!(rank(Medal::Gold, &stats).title, "VALLE VERDE LEGEND");
        assert_eq!(hook(Medal::Gold, 42., &stats), Hook::Ducks);
        stats.splashes = 1;
        assert_eq!(rank(Medal::Gold, &stats).title, "RALLY DUCK");
        assert_eq!(hook(Medal::Gold, 42., &stats), Hook::Done);
        let fence = Stats {
            bumps: 3,
            ..Stats::default()
        };
        assert_eq!(rank(Medal::Bronze, &fence).title, "FENCE INSPECTOR");
    }
}
