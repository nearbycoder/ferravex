//! Table-driven rally personality: a cheeky co-driver who loves the farm.
//!
//! Every selector is a pure function of accepted race counters, so the same
//! race always reads the same way. No clock, RNG or render cadence is involved,
//! and consecutive selections from one table never repeat the previous line.
use crate::style::Stats;
use ferravex_play::gameplay::trial::Medal;

const OFF_TRACK: [&str; 8] = [
    "SCENIC ROUTE SELECTED",
    "THAT'S A FIELD, CHAMP",
    "THE COWS DID NOT ORDER THIS",
    "FREE LAWN MOWING SERVICE",
    "TRACTOR MODE ENGAGED",
    "THE ROAD MISSES YOU",
    "FARMER GIULIA IS TAKING NOTES",
    "OFF-ROAD DLC UNLOCKED",
];

const WRONG_WAY: [&str; 5] = [
    "WRONG WAY! THE GHOST IS CONFUSED",
    "VICTORY LAP? BIT EARLY",
    "BOLD. BACKWARDS, BUT BOLD.",
    "THE FINISH IS THE OTHER WAY",
    "REVERSE RALLY IS NEXT YEAR",
];

const BACK_ON_TRACK: [&str; 4] = [
    "ASPHALT, SWEET ASPHALT",
    "AND WE'RE BACK",
    "THE ROAD FORGIVES YOU",
    "TIRES SAY THANK YOU",
];

const BOTCHED: [&str; 5] = [
    "THE SHEEP SAW THAT",
    "TOO MUCH ATTITUDE",
    "STYLE POINTS REPOSSESSED",
    "DRIFT RETURNED TO SENDER",
    "THE GRASS ATE YOUR POINTS",
];

const BUMP: [&str; 5] = ["BONK!", "FENCE SAYS NO", "BOING!", "OOF!", "STURDY FENCE!"];

const CLEAN_LAP: [&str; 3] = [
    "NOT ONE BLADE OF GRASS",
    "TIDY AS A TRACTOR SHED",
    "THE CO-DRIVER NODS",
];

const PAUSE: [&str; 5] = [
    "ENGINE IDLING. COWS WAITING.",
    "THE CO-DRIVER IS SNACKING",
    "TIRES COOLING. OPINIONS HEATING.",
    "THE GHOST WILL WAIT. IT HAS TIME.",
    "STRETCH THOSE STEERING THUMBS",
];

const READY: [&str; 6] = [
    "AGAIN! WITH FEELING",
    "FRESH TIRES, SAME DRIVER",
    "THE GHOST REMEMBERS YOU",
    "THIS ONE'S THE ONE",
    "DEEP BREATH. FLOOR IT.",
    "THE COWS ARE WATCHING",
];

const SPLASH: [&str; 3] = [
    "THE DUCKS ARE FURIOUS",
    "THE DUCKS HAVE FILED A COMPLAINT",
    "AMPHIBIOUS MODE: NOT INSTALLED",
];

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

/// The countdown title: the track name first, then a retry line.
pub fn ready(attempt: u32) -> &'static str {
    if attempt == 0 {
        "VALLE VERDE · 3 LAPS"
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

pub fn medal_name(medal: Medal) -> &'static str {
    match medal {
        Medal::Gold => "GOLD",
        Medal::Silver => "SILVER",
        Medal::Bronze => "BRONZE",
        Medal::Unranked => "NO MEDAL",
    }
}

/// The next medal up and its target time, if any.
pub fn next_medal(medal: Medal) -> Option<(&'static str, f32)> {
    match medal {
        Medal::Gold => None,
        Medal::Silver => Some(("GOLD", MEDALS[0])),
        Medal::Bronze => Some(("SILVER", MEDALS[1])),
        Medal::Unranked => Some(("BRONZE", MEDALS[2])),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
}

/// Silly, never mean. Rare feats first, then driving style, then medal tiers.
pub fn rank(medal: Medal, stats: &Stats) -> Rank {
    let (title, quip) = if stats.splashes > 0 && medal == Medal::Gold {
        (
            "RALLY DUCK",
            "GOLD PACE AND A SWIM. THE DUCKS ARE IMPRESSED.",
        )
    } else if stats.splashes > 0 {
        ("DUCK BOTHERER", "THE DUCKS HAVE FILED A FORMAL COMPLAINT")
    } else if stats.donuts >= 2 {
        ("DONUT ARTIST", "THE BAKERY WANTS YOUR NUMBER")
    } else if medal == Medal::Gold && stats.perfect() && stats.style >= 2000 {
        ("VALLE VERDE LEGEND", "CLEAN, QUICK AND SIDEWAYS. SHOW-OFF.")
    } else if medal == Medal::Gold && stats.perfect() {
        ("PERFECT LINE", "NOT ONE BLADE OF GRASS DISTURBED")
    } else if stats.style >= 2500 {
        ("SIDEWAYS SPECIALIST", "WHY DRIVE FORWARDS AT ALL?")
    } else if stats.bumps >= 3 {
        ("FENCE INSPECTOR", "EVERY FENCE CHECKED. ALL VERY STURDY.")
    } else if stats.grass_seconds() >= 8. {
        (
            "AGRICULTURAL DRIVER",
            "YOU PLOUGHED. THE FARMERS THANK YOU.",
        )
    } else if stats.wrong_way_episodes >= 2 {
        (
            "COMPASS OPTIONAL",
            "AN UNCONVENTIONAL INTERPRETATION OF LAPS",
        )
    } else {
        match medal {
            Medal::Gold => ("GOLDEN GRAVEL", "THE GHOST IS TAKING NOTES"),
            Medal::Silver => ("SILVER SLIDER", "SO CLOSE THE GOLD CAN SMELL YOU"),
            Medal::Bronze => ("BRONZE BOMBER", "SOLID! THE COWS APPLAUD (SLOWLY)"),
            Medal::Unranked => ("SUNDAY DRIVER", "SCENIC, RELAXED AND VERY LEGAL"),
        }
    };
    Rank { title, quip }
}

/// The next-goal hook under the result: medal chase, clean-lap chase, secret.
pub fn hook(medal: Medal, total: f32, stats: &Stats) -> String {
    if let Some((name, target)) = next_medal(medal) {
        format!("{:.2}S FASTER FOR {name}!", (total - target).max(0.01))
    } else if !stats.perfect() {
        format!("GOLD! NOW TRY {} CLEAN LAPS", LAPS)
    } else if stats.style < 2000 {
        "PERFECT! NOW DO IT SIDEWAYS (2000 STYLE)".to_owned()
    } else if stats.splashes == 0 {
        "PSST... THE DUCKS LOOK BORED".to_owned()
    } else {
        "NOTHING LEFT TO PROVE. EXCEPT SPEED.".to_owned()
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
        assert_eq!(ready(0), "VALLE VERDE · 3 LAPS");
        assert_ne!(ready(1), ready(2));
    }

    #[test]
    fn medals_ranks_and_hooks_always_offer_a_next_goal() {
        assert_eq!(medal(42.), Medal::Gold);
        assert_eq!(medal(43.01), Medal::Silver);
        assert_eq!(medal(50.), Medal::Bronze);
        assert_eq!(medal(90.), Medal::Unranked);
        let mut stats = Stats::default();
        assert_eq!(hook(Medal::Silver, 44.5, &stats), "1.50S FASTER FOR GOLD!");
        assert_eq!(hook(Medal::Gold, 42., &stats), "GOLD! NOW TRY 3 CLEAN LAPS");
        assert_eq!(rank(Medal::Silver, &stats).title, "SILVER SLIDER");
        stats.clean_laps = 3;
        assert_eq!(rank(Medal::Gold, &stats).title, "PERFECT LINE");
        assert!(hook(Medal::Gold, 42., &stats).contains("SIDEWAYS"));
        stats.style = 2400;
        assert_eq!(rank(Medal::Gold, &stats).title, "VALLE VERDE LEGEND");
        assert!(hook(Medal::Gold, 42., &stats).contains("DUCKS"));
        stats.splashes = 1;
        assert_eq!(rank(Medal::Gold, &stats).title, "RALLY DUCK");
        assert!(hook(Medal::Gold, 42., &stats).contains("NOTHING LEFT"));
        let fence = Stats {
            bumps: 3,
            ..Stats::default()
        };
        assert_eq!(rank(Medal::Bronze, &fence).title, "FENCE INSPECTOR");
    }
}
