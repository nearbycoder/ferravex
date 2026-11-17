//! Deterministic, table-driven personality for falls, saves, style moments and
//! the results card.
//!
//! Every selector is a pure function of accepted simulation counters, so a run
//! always reads the same way: no clock, RNG or render cadence is involved.
//! Consecutive selections from one table never repeat the previous line.
use crate::courses::Course;

/// Why the marble went back to a checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fall {
    /// Rolled off into the void.
    Void,
    /// Rolled off an ice course.
    Ice,
    /// Fell shortly after a sweeper bonk.
    Swept,
    /// Pressed Recover.
    Manual,
}

// In-play callouts are three words at most; the joke lands in a glance.
const VOID: [&str; 8] = [
    "MARBLE OVERBOARD!",
    "VOID SAYS HI",
    "GRAVITY WINS",
    "LOST YOUR MARBLE",
    "NOT A SHORTCUT",
    "BOLD. WRONG.",
    "OFF THE MAP",
    "PHYSICS WORKS!",
];

const ICE: [&str; 6] = [
    "COLD SHOULDER",
    "SLIP. SLIDE. BYE.",
    "NO FRICTION",
    "VERY ICY",
    "WHEEE... OH NO",
    "FROSTY EXIT",
];

const SWEPT: [&str; 6] = [
    "SWEPT AWAY",
    "SWEEPER SAYS NO",
    "INTO ORBIT",
    "CLOCKWORK WINS",
    "TIMING!",
    "SWATTED!",
];

const MANUAL: [&str; 5] = [
    "TACTICAL RESET",
    "STRATEGIC RETREAT",
    "NEVER HAPPENED",
    "REWIND. REROLL.",
    "NOTHING TO SEE",
];

/// Three or more falls since the latest checkpoint.
const STUBBORN: [&str; 6] = [
    "PRACTICE MAKES PLATFORMS",
    "THIS ONE. PROBABLY.",
    "SAME SPOT? BOLD.",
    "MARBLE BELIEVES",
    "MAXIMUM DETERMINATION",
    "BRAKES EXIST",
];

const CHECKPOINT: [&str; 4] = [
    "CHECKPOINT!",
    "PROGRESS BANKED",
    "SAFE. FOR NOW.",
    "CHECKPOINT GET!",
];

const BONK: [&str; 5] = ["BONK!", "BOING!", "THWACK!", "CLONK!", "BONK!!"];
const CLUTCH: [&str; 5] = [
    "CLUTCH!",
    "WHEW!",
    "ON THE EDGE!",
    "NICE SAVE!",
    "TOO CLOSE!",
];
const AIR: [&str; 4] = ["AIRBORNE!", "BIG AIR!", "WHEEE!", "LIFTOFF!"];

const PAUSE: [&str; 5] = [
    "The marble will wait. It's a marble.",
    "Stretch those thumbs",
    "The sweepers are on break too",
    "Time is frozen. Very scientific.",
    "Hydrate. The ice insists.",
];

pub(crate) fn pause(count: u32) -> &'static str {
    rotate(&PAUSE, count)
}

/// Advances by a stride coprime with the table length, so the same table never
/// repeats on consecutive counters and every line is visited.
pub(crate) fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let stride = if table.len().is_multiple_of(5) { 3 } else { 5 };
    table[(counter as usize).wrapping_mul(stride) % table.len()]
}

/// `falls` is the run's total including this one; `here` counts falls since
/// the latest checkpoint (or the start).
pub(crate) fn fall(kind: Fall, falls: u32, here: u32) -> &'static str {
    if kind != Fall::Manual && here >= 3 && here % 2 == 1 {
        return rotate(&STUBBORN, here / 2);
    }
    match kind {
        Fall::Void => rotate(&VOID, falls),
        Fall::Ice => rotate(&ICE, falls),
        Fall::Swept => rotate(&SWEPT, falls),
        Fall::Manual => rotate(&MANUAL, falls),
    }
}

pub(crate) fn checkpoint(index: usize) -> &'static str {
    CHECKPOINT[index.saturating_sub(1) % CHECKPOINT.len()]
}

/// Each `count` includes the moment being announced.
pub(crate) fn bonk(count: u32) -> &'static str {
    rotate(&BONK, count.saturating_sub(1))
}
pub(crate) fn clutch(count: u32) -> &'static str {
    rotate(&CLUTCH, count.saturating_sub(1))
}
pub(crate) fn air(count: u32) -> &'static str {
    rotate(&AIR, count.saturating_sub(1))
}

/// What a finished run gets called on the results card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
}

/// A finished run, independent of storage and rendering.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Summary {
    pub medal: &'static str,
    pub elapsed: f32,
    pub gold: f32,
    pub falls: u32,
    pub bonks: u32,
    pub clutches: u32,
    pub best_air: f32,
    pub secret: bool,
}

/// Silly, never mean; rarest feats first, then the medal tiers.
pub(crate) fn rank(run: Summary) -> Rank {
    let flawless = run.medal == "GOLD" && run.falls == 0;
    let (title, quip) = if run.secret && flawless {
        (
            "Moonstone Monarch",
            "Gold, no falls and the moonstone. Show-off.",
        )
    } else if run.secret {
        (
            "Magpie of the Peaks",
            "You found the moonstone. Shiny things: secured.",
        )
    } else if flawless && run.elapsed <= run.gold * 0.85 {
        ("Glass Lightning", "Blink and the arch missed you")
    } else if run.falls >= 8 {
        ("Gravity's Pen Pal", "You and the void write every week")
    } else if run.falls >= 4 {
        ("Professional Faller", "The abyss knows you by name")
    } else if run.bonks >= 3 {
        ("Pinball Wizard", "The sweepers did most of the steering")
    } else if run.clutches >= 2 {
        ("Edge Whisperer", "You live on the edge. Literally.")
    } else if run.best_air >= 0.7 {
        (
            "Frequent Flyer",
            "Technically a marble. Spiritually a bird.",
        )
    } else {
        match run.medal {
            "GOLD" => ("Polished to a Shine", "Smooth, quick and spotless"),
            "SILVER" => ("Smooth Operator", "Solid roll. Gold is right there."),
            _ => ("Sunday Roller", "The scenic route. The scenery approves."),
        }
    };
    Rank { title, quip }
}

/// The "one more try" hook, drawn as icons on the results card.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum NextGoal {
    /// The next medal up: its name, the time to beat and the falls allowed.
    Medal {
        medal: &'static str,
        seconds: f32,
        falls: u32,
    },
    /// Gold is done; a moonstone hides somewhere on this course.
    Moonstone,
    /// Everything found: beat the stored best time.
    Best(f32),
    /// No stored best to show (storage unavailable): just go faster.
    Faster,
}

/// The next medal, then the moonstone rumour, then the stored personal best.
/// `best` is the fastest time after recording this run.
pub(crate) fn next_goal(course: &Course, medal: &str, best: Option<f32>, secret: bool) -> NextGoal {
    match medal {
        "BRONZE" => NextGoal::Medal {
            medal: "SILVER",
            seconds: course.gold_seconds * 1.5,
            falls: 2,
        },
        "SILVER" => NextGoal::Medal {
            medal: "GOLD",
            seconds: course.gold_seconds,
            falls: 0,
        },
        _ if !secret => NextGoal::Moonstone,
        _ => best.map_or(NextGoal::Faster, NextGoal::Best),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::courses::COURSES;
    use std::collections::BTreeSet;

    #[test]
    fn consecutive_falls_never_repeat_and_every_table_is_reachable() {
        for kind in [Fall::Void, Fall::Ice, Fall::Swept, Fall::Manual] {
            let mut previous = "";
            let mut here = 0;
            let mut seen = BTreeSet::new();
            for falls in 1..200 {
                here = if falls % 7 == 0 { 1 } else { here + 1 };
                let line = fall(kind, falls, here);
                assert_ne!(line, previous, "{kind:?} fall {falls} repeated {line}");
                previous = line;
                seen.insert(line);
            }
            assert!(seen.len() >= 5);
        }
        for table in [
            &VOID[..],
            &ICE[..],
            &SWEPT[..],
            &MANUAL[..],
            &STUBBORN[..],
            &BONK[..],
            &CLUTCH[..],
            &AIR[..],
            &PAUSE[..],
        ] {
            let visited: BTreeSet<_> = (0..table.len() as u32).map(|n| rotate(table, n)).collect();
            assert_eq!(visited.len(), table.len());
            for n in 0..40 {
                assert_ne!(rotate(table, n), rotate(table, n + 1));
            }
        }
        assert_ne!(checkpoint(1), checkpoint(2));
    }

    #[test]
    fn ranks_reward_rare_feats_first_and_every_medal_has_a_title() {
        let base = Summary {
            medal: "GOLD",
            elapsed: 33.,
            gold: 35.,
            falls: 0,
            bonks: 0,
            clutches: 0,
            best_air: 0.,
            secret: false,
        };
        assert_eq!(rank(base).title, "Polished to a Shine");
        assert_eq!(
            rank(Summary {
                elapsed: 29.,
                ..base
            })
            .title,
            "Glass Lightning"
        );
        assert_eq!(
            rank(Summary {
                secret: true,
                ..base
            })
            .title,
            "Moonstone Monarch"
        );
        let silver = Summary {
            medal: "SILVER",
            falls: 2,
            ..base
        };
        assert_eq!(
            rank(Summary {
                secret: true,
                ..silver
            })
            .title,
            "Magpie of the Peaks"
        );
        assert_eq!(
            rank(Summary { falls: 9, ..silver }).title,
            "Gravity's Pen Pal"
        );
        assert_eq!(rank(Summary { bonks: 3, ..silver }).title, "Pinball Wizard");
        assert_eq!(
            rank(Summary {
                clutches: 2,
                ..silver
            })
            .title,
            "Edge Whisperer"
        );
        assert_eq!(
            rank(Summary {
                best_air: 0.8,
                ..silver
            })
            .title,
            "Frequent Flyer"
        );
        assert_eq!(rank(silver).title, "Smooth Operator");
        assert_eq!(
            rank(Summary {
                medal: "BRONZE",
                ..silver
            })
            .title,
            "Sunday Roller"
        );
    }

    #[test]
    fn next_goal_climbs_medals_then_hints_the_secret_then_chases_the_best() {
        let c = &COURSES[0];
        assert_eq!(
            next_goal(c, "BRONZE", None, false),
            NextGoal::Medal {
                medal: "SILVER",
                seconds: 52.5,
                falls: 2
            }
        );
        assert_eq!(
            next_goal(c, "SILVER", None, true),
            NextGoal::Medal {
                medal: "GOLD",
                seconds: 35.,
                falls: 0
            }
        );
        assert_eq!(next_goal(c, "GOLD", Some(30.), false), NextGoal::Moonstone);
        assert_eq!(
            next_goal(c, "GOLD", Some(30.256), true),
            NextGoal::Best(30.256)
        );
        assert_eq!(next_goal(c, "GOLD", None, true), NextGoal::Faster);
    }

    #[test]
    fn in_play_callouts_stay_within_three_words() {
        for line in VOID
            .iter()
            .chain(&ICE)
            .chain(&SWEPT)
            .chain(&MANUAL)
            .chain(&STUBBORN)
            .chain(&CHECKPOINT)
            .chain(&BONK)
            .chain(&CLUTCH)
            .chain(&AIR)
        {
            assert!(line.split_whitespace().count() <= 3, "{line}");
            assert!(line.is_ascii(), "{line} must render in the classic face");
        }
    }
}
