//! Deterministic personality: wave names, announcer callouts, rank titles and
//! the results hook. Every selector is a pure function of accepted counters,
//! so the same run always reads the same way. Rotations use a stride coprime
//! with the table length, so consecutive counters never repeat a line.
use crate::game::{CLUTCH_HEALTH, Style};

/// Silly names for the three waves, shown on intro banners and results.
pub(crate) const WAVES: [&str; 3] = ["Welcome Committee", "Middle Management", "The Whole Hive"];

pub(crate) fn wave_name(wave: u32) -> &'static str {
    WAVES[(wave.clamp(1, 3) - 1) as usize]
}

const LOSS: [&str; 7] = [
    "The swarm says thanks for visiting",
    "Hexagons are not, in fact, invincible",
    "Signal lost. Dignity: also lost.",
    "The drones will be insufferable about this",
    "You were doing so well. Ish.",
    "Respawning confidence…",
    "That's one way to meet the swarm",
];

const WIN: [&str; 5] = [
    "Sector 07 is spotless. Mostly.",
    "The drones have filed a complaint",
    "Containment: contained",
    "Your hexagon has never looked better",
    "Pest control would like your number",
];

const PAUSE: [&str; 5] = [
    "The swarm is also on break",
    "Drones frozen mid-buzz",
    "Stretch. Blink. Hydrate.",
    "The hexagon appreciates the rest",
    "Time stopped. Very sci-fi.",
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
            ("Untouchable Hexagon", "They never laid a pixel on you")
        } else if s.disco() {
            (
                "Disco Inferno",
                "You found the party. The drones were not invited.",
            )
        } else if run.health <= CLUTCH_HEALTH {
            ("Hanging by a Pixel", "Cardiologists hate this one trick")
        } else if s.drive_bys >= 6 {
            ("Drive-by Diplomat", "Negotiations conducted at 700 px/s")
        } else if s.close_shaves >= 6 {
            ("The Matador", "Olé! The swarm keeps missing")
        } else if run.best_chain >= 20 {
            ("Chain Reaction", "One kill led to another. And another.")
        } else if run.score >= 15_000 {
            (
                "Certified Exterminator",
                "The swarm has your photo on a dartboard",
            )
        } else if run.score >= 9_000 {
            ("Pest Control Pro", "Clean work. Tidy hexagon.")
        } else {
            ("Swarm Survivor", "Scrappy, but the sector is yours")
        }
    } else {
        match run.wave {
            0 | 1 if run.score == 0 => ("Drone Snack", "Tip: the mouse button shoots things"),
            0 | 1 => (
                "Welcome Mat",
                "The committee welcomed you a little too hard",
            ),
            2 => ("Middle Managed", "Your performance review was… brief"),
            _ if s.overseer_down => ("So, So Close", "The Overseer fell. So did you."),
            _ => ("Hive-Five", "You met the Whole Hive. They hugged hard."),
        }
    };
    Rank { title, quip }
}

/// The single most tempting next goal for "one more try".
pub(crate) fn next_goal(run: RunSummary) -> &'static str {
    let s = run.style;
    if !run.won {
        return match run.wave {
            0 | 1 => "Next: survive the Welcome Committee",
            2 => "Next: reach the Whole Hive",
            _ => "Next: take down the Overseer",
        };
    }
    if s.untouched_waves == 0 {
        "Next: clear a wave without getting hit"
    } else if s.drive_bys < 3 {
        "Next: dash through a drone and shoot it mid-dash"
    } else if s.beacons_lit() == 0 {
        "Psst: the four wall lamps look… shootable"
    } else if !s.disco() {
        "Next: light all four wall lamps"
    } else if s.hits_taken > 0 {
        "Next: a flawless run. You know you want to."
    } else {
        "Nothing left to prove. Beat your score?"
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
        assert_eq!(rank(base).title, "Swarm Survivor");
        let flawless = RunSummary {
            style: Style::default(),
            ..base
        };
        assert_eq!(rank(flawless).title, "Untouchable Hexagon");
        let disco = RunSummary {
            style: Style {
                beacons: 0b1111,
                hits_taken: 1,
                ..Style::default()
            },
            ..base
        };
        assert_eq!(rank(disco).title, "Disco Inferno");
        assert_eq!(
            rank(RunSummary { health: 3, ..base }).title,
            "Hanging by a Pixel"
        );
        let lost = RunSummary {
            won: false,
            wave: 1,
            score: 0,
            ..base
        };
        assert_eq!(rank(lost).title, "Drone Snack");
        assert_eq!(rank(RunSummary { wave: 2, ..lost }).title, "Middle Managed");
        for wave in 1..=3 {
            for won in [false, true] {
                let run = RunSummary { won, wave, ..base };
                assert!(!rank(run).title.is_empty() && !rank(run).quip.is_empty());
                assert!(!next_goal(run).is_empty());
            }
        }
        assert_eq!(next_goal(base), "Next: clear a wave without getting hit");
        let veteran = RunSummary {
            style: Style {
                untouched_waves: 1,
                drive_bys: 4,
                hits_taken: 1,
                ..Style::default()
            },
            ..base
        };
        assert!(next_goal(veteran).contains("lamps"));
    }
}
