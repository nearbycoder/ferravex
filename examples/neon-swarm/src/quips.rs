//! Deterministic personality: wave names, announcer callouts, rank titles and
//! the results hook. Every selector is a pure function of accepted counters,
//! so the same run always reads the same way. Rotations use a stride coprime
//! with the table length, so consecutive counters never repeat a line.
use crate::game::{CLUTCH_HEALTH, Style};

/// Silly names for the three waves, shown on intro banners and results.
pub(crate) const WAVES: [&str; 3] = ["WELCOME COMMITTEE", "MIDDLE MANAGEMENT", "THE WHOLE HIVE"];

pub(crate) fn wave_name(wave: u32) -> &'static str {
    WAVES[(wave.clamp(1, 3) - 1) as usize]
}

const LOSS: [&str; 7] = [
    "THE SWARM SAYS THANKS FOR VISITING",
    "HEXAGONS ARE NOT, IN FACT, INVINCIBLE",
    "SIGNAL LOST. DIGNITY: ALSO LOST.",
    "THE DRONES WILL BE INSUFFERABLE ABOUT THIS",
    "YOU WERE DOING SO WELL. ISH.",
    "RESPAWNING CONFIDENCE...",
    "THAT'S ONE WAY TO MEET THE SWARM",
];

const WIN: [&str; 5] = [
    "SECTOR 07 IS SPOTLESS. MOSTLY.",
    "THE DRONES HAVE FILED A COMPLAINT",
    "CONTAINMENT: CONTAINED",
    "YOUR HEXAGON HAS NEVER LOOKED BETTER",
    "PEST CONTROL WOULD LIKE YOUR NUMBER",
];

const PAUSE: [&str; 5] = [
    "THE SWARM IS ALSO ON BREAK",
    "DRONES FROZEN MID-BUZZ",
    "STRETCH. BLINK. HYDRATE.",
    "THE HEXAGON APPRECIATES THE REST",
    "TIME STOPPED. VERY SCI-FI.",
];

const CLOSE: [&str; 6] = [
    "CLOSE SHAVE!",
    "NOT TODAY!",
    "MISSED ME!",
    "TOO SLOW!",
    "NICE DODGE!",
    "WHOOSH!",
];

const DRIVE_BY: [&str; 4] = ["DRIVE-BY!", "DASH & BASH!", "HIT AND RUN!", "SPEED DATING!"];

const BROKEN: [&str; 5] = [
    "CHAIN BROKEN",
    "OOF. THE COMBO.",
    "IT WAS A GOOD CHAIN",
    "COMBO, INTERRUPTED",
    "RUDE. VERY RUDE.",
];

fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let stride = if table.len().is_multiple_of(5) { 3 } else { 5 };
    table[(counter as usize).wrapping_mul(stride) % table.len()]
}

pub(crate) fn loss(attempt: u32) -> &'static str {
    rotate(&LOSS, attempt)
}

pub(crate) fn win(attempt: u32) -> &'static str {
    rotate(&WIN, attempt)
}

pub(crate) fn pause(count: u32) -> &'static str {
    rotate(&PAUSE, count)
}

/// `count` is the run's close shaves including this one.
pub(crate) fn close_shave(count: u32) -> &'static str {
    rotate(&CLOSE, count.saturating_sub(1))
}

pub(crate) fn drive_by(count: u32) -> &'static str {
    rotate(&DRIVE_BY, count.saturating_sub(1))
}

pub(crate) fn chain_broken(count: u32) -> &'static str {
    rotate(&BROKEN, count)
}

/// Announcer line for reaching a new multiplier tier.
pub(crate) fn multiplier(tier: u32) -> &'static str {
    match tier {
        0..=2 => "HEATING UP",
        3 => "ON FIRE",
        4 => "UNSTOPPABLE",
        _ => "MAXIMUM NEON",
    }
}

/// Announcer line for kills inside one short window.
pub(crate) fn multi_kill(count: u32) -> Option<&'static str> {
    match count {
        0 | 1 => None,
        2 => Some("DOUBLE TAP"),
        3 => Some("TRIPLE THREAT"),
        4 => Some("QUAD SQUAD"),
        _ => Some("SWARM SWEEPER"),
    }
}

/// A finished run, independent of storage and rendering.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RunSummary {
    pub won: bool,
    pub wave: u32,
    pub health: i32,
    pub score: u64,
    pub best_chain: u32,
    pub style: Style,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
}

/// Silly, never mean; rarest feats first, then outcome tiers.
pub(crate) fn rank(run: RunSummary) -> Rank {
    let s = run.style;
    let (title, quip) = if run.won {
        if s.hits_taken == 0 {
            ("UNTOUCHABLE HEXAGON", "THEY NEVER LAID A PIXEL ON YOU")
        } else if s.disco() {
            (
                "DISCO INFERNO",
                "YOU FOUND THE PARTY. THE DRONES WERE NOT INVITED.",
            )
        } else if run.health <= CLUTCH_HEALTH {
            ("HANGING BY A PIXEL", "CARDIOLOGISTS HATE THIS ONE TRICK")
        } else if s.drive_bys >= 6 {
            ("DRIVE-BY DIPLOMAT", "NEGOTIATIONS CONDUCTED AT 700 PX/S")
        } else if s.close_shaves >= 6 {
            ("THE MATADOR", "OLE! THE SWARM KEEPS MISSING")
        } else if run.best_chain >= 20 {
            ("CHAIN REACTION", "ONE KILL LED TO ANOTHER. AND ANOTHER.")
        } else if run.score >= 15_000 {
            (
                "CERTIFIED EXTERMINATOR",
                "THE SWARM HAS YOUR PHOTO ON A DARTBOARD",
            )
        } else if run.score >= 9_000 {
            ("PEST CONTROL PRO", "CLEAN WORK. TIDY HEXAGON.")
        } else {
            ("SWARM SURVIVOR", "SCRAPPY, BUT THE SECTOR IS YOURS")
        }
    } else {
        match run.wave {
            0 | 1 if run.score == 0 => ("DRONE SNACK", "TIP: THE MOUSE BUTTON SHOOTS THINGS"),
            0 | 1 => (
                "WELCOME MAT",
                "THE COMMITTEE WELCOMED YOU A LITTLE TOO HARD",
            ),
            2 => ("MIDDLE MANAGED", "YOUR PERFORMANCE REVIEW WAS... BRIEF"),
            _ if s.overseer_down => ("SO, SO CLOSE", "THE OVERSEER FELL. SO DID YOU."),
            _ => ("HIVE-FIVE", "YOU MET THE WHOLE HIVE. THEY HUGGED HARD."),
        }
    };
    Rank { title, quip }
}

/// The single most tempting next goal for "one more try".
pub(crate) fn next_goal(run: RunSummary) -> &'static str {
    let s = run.style;
    if !run.won {
        return match run.wave {
            0 | 1 => "NEXT: SURVIVE THE WELCOME COMMITTEE",
            2 => "NEXT: REACH THE WHOLE HIVE",
            _ => "NEXT: TAKE DOWN THE OVERSEER",
        };
    }
    if s.untouched_waves == 0 {
        "NEXT: CLEAR A WAVE WITHOUT GETTING HIT"
    } else if s.drive_bys < 3 {
        "NEXT: DASH THROUGH A DRONE AND SHOOT IT MID-DASH"
    } else if s.beacons_lit() == 0 {
        "PSST: THE FOUR WALL LAMPS LOOK... SHOOTABLE"
    } else if !s.disco() {
        "NEXT: LIGHT ALL FOUR WALL LAMPS"
    } else if s.hits_taken > 0 {
        "NEXT: A FLAWLESS RUN. YOU KNOW YOU WANT TO."
    } else {
        "NOTHING LEFT TO PROVE. BEAT YOUR SCORE?"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_never_repeat_back_to_back_and_reach_every_line() {
        for table in [
            &LOSS[..],
            &WIN[..],
            &PAUSE[..],
            &CLOSE[..],
            &DRIVE_BY[..],
            &BROKEN[..],
        ] {
            let mut previous = "";
            let mut seen = std::collections::BTreeSet::new();
            for n in 0..64 {
                let line = rotate(table, n);
                assert_ne!(line, previous);
                previous = line;
                seen.insert(line);
            }
            assert_eq!(seen.len(), table.len());
        }
        assert_ne!(close_shave(1), close_shave(2));
        assert_eq!(multi_kill(1), None);
        assert_eq!(multi_kill(3), Some("TRIPLE THREAT"));
        assert_eq!(multiplier(5), "MAXIMUM NEON");
        assert_eq!(wave_name(0), WAVES[0]);
        assert_eq!(wave_name(9), WAVES[2]);
    }

    #[test]
    fn ranks_reward_rare_feats_first_and_every_outcome_has_a_goal() {
        let base = RunSummary {
            won: true,
            wave: 3,
            health: 80,
            score: 5_000,
            best_chain: 4,
            style: Style {
                hits_taken: 2,
                ..Style::default()
            },
        };
        assert_eq!(rank(base).title, "SWARM SURVIVOR");
        let flawless = RunSummary {
            style: Style::default(),
            ..base
        };
        assert_eq!(rank(flawless).title, "UNTOUCHABLE HEXAGON");
        let disco = RunSummary {
            style: Style {
                beacons: 0b1111,
                hits_taken: 1,
                ..Style::default()
            },
            ..base
        };
        assert_eq!(rank(disco).title, "DISCO INFERNO");
        assert_eq!(
            rank(RunSummary { health: 3, ..base }).title,
            "HANGING BY A PIXEL"
        );
        let lost = RunSummary {
            won: false,
            wave: 1,
            score: 0,
            ..base
        };
        assert_eq!(rank(lost).title, "DRONE SNACK");
        assert_eq!(rank(RunSummary { wave: 2, ..lost }).title, "MIDDLE MANAGED");
        for wave in 1..=3 {
            for won in [false, true] {
                let run = RunSummary { won, wave, ..base };
                assert!(!rank(run).title.is_empty() && !rank(run).quip.is_empty());
                assert!(!next_goal(run).is_empty());
            }
        }
        assert_eq!(next_goal(base), "NEXT: CLEAR A WAVE WITHOUT GETTING HIT");
        let veteran = RunSummary {
            style: Style {
                untouched_waves: 1,
                drive_bys: 4,
                hits_taken: 1,
                ..Style::default()
            },
            ..base
        };
        assert!(next_goal(veteran).contains("LAMPS"));
    }
}
