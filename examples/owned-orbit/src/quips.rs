//! Deterministic, table-driven personality for Orbit Breaker.
//!
//! Every selector is a pure function of accepted simulation counters, so the
//! same run always reads the same way: no clock, RNG or render cadence is
//! involved. Consecutive counters never repeat the previous line.

/// Advances by a stride coprime with the table length, so consecutive counters
/// never repeat a line and every entry is eventually visited.
fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let stride = if table.len().is_multiple_of(5) { 3 } else { 5 };
    table[(counter as usize).wrapping_mul(stride) % table.len()]
}

/// Sector name and a one-line tagline, shown on the ready card and in the HUD.
pub(crate) fn sector(sector: u32) -> (&'static str, &'static str) {
    match sector {
        1 => ("LOW ORBIT", "MIND THE SPACE JUNK. IT MINDS YOU BACK."),
        _ => ("THE ARMORED BELT", "NOW WITH EXTRA BOLTS. SOME HIT TWICE."),
    }
}

const DRAIN: [&str; 8] = [
    "GRAVITY 1, PADDLE 0",
    "THE BALL CHOSE FREEDOM",
    "HOUSTON, WE HAVE A HOLE",
    "IT'S FINE. SPACE IS BIG.",
    "THAT BALL HAD SOMEWHERE TO BE",
    "PADDLE.EXE STOPPED RESPONDING",
    "LOST IN SPACE. LITERALLY.",
    "THE VOID SAYS THANKS",
];

/// `lost` is the number of balls lost this run, including this one.
pub(crate) fn drain(lost: u32) -> &'static str {
    rotate(&DRAIN, lost.saturating_sub(1))
}

const SAVE: [&str; 5] = [
    "NOPE. NOT TODAY.",
    "THE BALL GOT A REFUND",
    "RETURN TO SENDER",
    "PHEW! FREEBIE USED.",
    "SAFETY NET: DEPLOYED",
];

/// `saves` counts opening ball saves this run, including this one.
pub(crate) fn save(saves: u32) -> &'static str {
    rotate(&SAVE, saves.saturating_sub(1))
}

const RETRY: [&str; 6] = [
    "THE PANELS REMEMBER NOTHING",
    "SAME ORBIT, NEW YOU",
    "THIS IS THE ONE. PROBABLY.",
    "DUST OFF THAT PADDLE",
    "ROUND TWO. OR TWELVE. WHO'S COUNTING?",
    "THE BALL MISSED YOU TOO",
];

/// `runs` counts completed runs in this session.
pub(crate) fn retry(runs: u32) -> &'static str {
    rotate(&RETRY, runs.saturating_sub(1))
}

const PAUSE: [&str; 5] = [
    "THE PANELS ARE ALSO ON BREAK",
    "THE BALL IS HOVERING POLITELY",
    "SPACE IS PATIENT. MOSTLY.",
    "STRETCH THOSE THUMBS",
    "ORBIT ON HOLD. PLEASE ENJOY THE MUZAK.",
];

pub(crate) fn pause(seed: u32) -> &'static str {
    rotate(&PAUSE, seed)
}

/// Volley milestones (panel hits since the last paddle touch) and their titles.
pub(crate) const VOLLEY_MILESTONES: [u32; 7] = [3, 5, 8, 12, 16, 20, 25];
const VOLLEY_TITLES: [&str; 7] = [
    "TRIPLE TAP!",
    "PINBALL WIZARD!",
    "RICOCHET RAMPAGE!",
    "ORBITAL MELTDOWN!",
    "PHYSICS IS CRYING!",
    "SUPERNOVA!",
    "PLEASE STOP. IT'S TOO POWERFUL.",
];

/// The callout for a volley milestone, if `hits` is one.
pub(crate) fn volley(hits: u32) -> Option<&'static str> {
    VOLLEY_MILESTONES
        .iter()
        .position(|&m| m == hits)
        .map(|i| VOLLEY_TITLES[i])
        .or_else(|| (hits > 25 && hits.is_multiple_of(5)).then_some(VOLLEY_TITLES[6]))
}

/// Is `hits` a milestone that deserves a callout?
pub(crate) fn is_volley_milestone(hits: u32) -> bool {
    volley(hits).is_some()
}

const LAST_PANEL: [&str; 3] = [
    "IT KNOWS. IT'S NERVOUS.",
    "ONE PANEL. ALL THE PRESSURE.",
    "FINISH HIM. GENTLY.",
];

pub(crate) fn last_panel(sector: u32) -> &'static str {
    LAST_PANEL[(sector as usize + 2) % LAST_PANEL.len()]
}

/// What a finished run gets called on the results card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
}

/// A finished run, independent of storage and rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RunSummary {
    pub won: bool,
    pub score: u32,
    pub sector: u32,
    pub clears: u32,
    pub lives_lost: u32,
    pub saves: u32,
    pub satellites: u32,
}

/// Winning titles by score, lowest first.
const WON_TIERS: [(u32, &str, &str); 4] = [
    (0, "ASTEROID JANITOR", "THE SECTOR IS SPOTLESS. MOSTLY."),
    (9000, "GRAVITY TAMER", "NEWTON IS TAKING NOTES"),
    (12000, "COMET WRANGLER", "YEEHAW, BUT IN SPACE"),
    (
        15000,
        "THE ORBIT BREAKER",
        "THEY'LL NAME A CRATER AFTER YOU",
    ),
];

/// Losing titles by panels cleared, lowest first.
const LOST_TIERS: [(u32, &str, &str); 4] = [
    (0, "SPACE TOURIST", "LOVELY VIEW THOUGH, RIGHT?"),
    (10, "ORBIT INTERN", "COFFEE RUNS COUNT AS TRAINING"),
    (
        25,
        "JUNK JUGGLER",
        "SO CLOSE TO SECTOR TWO YOU COULD SMELL IT",
    ),
    (35, "BELT BUSTER", "THE ARMOR WAS SWEATING. METAPHORICALLY."),
];

/// Silly, never mean, rarest feats first, then the ordinary tiers.
pub(crate) fn rank(run: RunSummary) -> Rank {
    let (title, quip) = if run.won && run.lives_lost == 0 && run.saves == 0 {
        ("UNTOUCHABLE", "NOT ONE WOBBLE. THE PADDLE IS BLUSHING.")
    } else if run.satellites >= 2 {
        ("SPACE JUNK ARTIST", "MISSION CONTROL HAS QUESTIONS")
    } else if run.won {
        let tier = WON_TIERS.iter().rev().find(|t| run.score >= t.0).unwrap();
        (tier.1, tier.2)
    } else {
        let tier = LOST_TIERS.iter().rev().find(|t| run.clears >= t.0).unwrap();
        (tier.1, tier.2)
    };
    Rank { title, quip }
}

/// The next ordinary title to chase: (amount still needed, unit, title).
pub(crate) fn next_goal(run: RunSummary) -> Option<(u32, &'static str, &'static str)> {
    if run.won {
        WON_TIERS
            .iter()
            .find(|t| t.0 > run.score)
            .map(|t| (t.0 - run.score, "POINTS", t.1))
    } else {
        LOST_TIERS
            .iter()
            .find(|t| t.0 > run.clears)
            .map(|t| (t.0 - run.clears, "PANELS", t.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn consecutive_quips_never_repeat_and_every_line_is_reachable() {
        for table in [&DRAIN[..], &SAVE[..], &RETRY[..], &PAUSE[..]] {
            let mut previous = "";
            for n in 0..64 {
                let line = rotate(table, n);
                assert_ne!(line, previous);
                previous = line;
            }
            let seen: BTreeSet<_> = (0..table.len() as u32).map(|n| rotate(table, n)).collect();
            assert_eq!(seen.len(), table.len());
        }
        assert_ne!(drain(1), drain(2));
        assert_ne!(save(1), save(2));
        assert_ne!(retry(1), retry(2));
    }

    #[test]
    fn volley_milestones_escalate_and_ordinary_counts_stay_quiet() {
        assert_eq!(volley(3), Some("TRIPLE TAP!"));
        assert_eq!(volley(4), None);
        assert_eq!(volley(25), Some("PLEASE STOP. IT'S TOO POWERFUL."));
        assert_eq!(volley(30), Some("PLEASE STOP. IT'S TOO POWERFUL."));
        assert!(!is_volley_milestone(31));
        let titles: BTreeSet<_> = VOLLEY_MILESTONES
            .iter()
            .filter_map(|&m| volley(m))
            .collect();
        assert_eq!(titles.len(), VOLLEY_MILESTONES.len());
    }

    #[test]
    fn ranks_reward_rare_feats_first_and_always_name_the_next_title() {
        let base = RunSummary {
            won: true,
            score: 9500,
            sector: 2,
            clears: 70,
            lives_lost: 1,
            saves: 0,
            satellites: 0,
        };
        assert_eq!(rank(base).title, "GRAVITY TAMER");
        assert_eq!(next_goal(base), Some((2500, "POINTS", "COMET WRANGLER")));
        assert_eq!(
            rank(RunSummary {
                lives_lost: 0,
                ..base
            })
            .title,
            "UNTOUCHABLE"
        );
        assert_eq!(
            rank(RunSummary {
                satellites: 2,
                ..base
            })
            .title,
            "SPACE JUNK ARTIST"
        );
        assert_eq!(
            next_goal(RunSummary {
                score: 20000,
                ..base
            }),
            None
        );
        let lost = RunSummary {
            won: false,
            clears: 12,
            sector: 1,
            ..base
        };
        assert_eq!(rank(lost).title, "ORBIT INTERN");
        assert_eq!(next_goal(lost), Some((13, "PANELS", "JUNK JUGGLER")));
        assert_eq!(
            rank(RunSummary { clears: 0, ..lost }).title,
            "SPACE TOURIST"
        );
        for (_, title, quip) in WON_TIERS.iter().chain(&LOST_TIERS) {
            assert!(!title.is_empty() && !quip.is_empty());
        }
    }
}
