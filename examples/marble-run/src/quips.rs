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

const VOID: [&str; 8] = [
    "MARBLE OVERBOARD!",
    "THE VOID SAYS HI",
    "GRAVITY: STILL UNDEFEATED",
    "YOU LOST YOUR MARBLE",
    "THAT WAS NOT THE SHORTCUT",
    "A BOLD LINE. A WRONG LINE.",
    "ROLLED RIGHT OFF THE MAP",
    "PHYSICS: CONFIRMED WORKING",
];

const ICE: [&str; 6] = [
    "ICE TO MEET YOU",
    "SLIP. SLIDE. SAYONARA.",
    "FRICTION HAS LEFT THE CHAT",
    "THAT ICE WAS VERY ICY",
    "WHEEE... OH NO",
    "A GRACEFUL, FROSTY EXIT",
];

const SWEPT: [&str; 6] = [
    "SWEPT CLEAN OFF THE COURSE",
    "THE SWEEPER SAYS NO",
    "BONKED INTO ORBIT",
    "CLOCKWORK 1, MARBLE 0",
    "TIMING IS EVERYTHING",
    "SWATTED LIKE A FLY",
];

const MANUAL: [&str; 5] = [
    "TACTICAL RESET",
    "STRATEGIC RETREAT",
    "LET'S PRETEND THAT NEVER HAPPENED",
    "REWIND. REROLL.",
    "NOTHING TO SEE HERE",
];

/// Three or more falls since the latest checkpoint.
const STUBBORN: [&str; 6] = [
    "PRACTICE MAKES PLATFORMS",
    "THIS IS THE ONE. PROBABLY.",
    "SAME SPOT? BOLD STRATEGY",
    "THE MARBLE BELIEVES IN YOU",
    "DETERMINATION: MAXIMUM",
    "BRAKING IS ALLOWED, YOU KNOW",
];

const CHECKPOINT: [&str; 4] = [
    "CHECKPOINT! FALL WITH CONFIDENCE",
    "PROGRESS BANKED",
    "SAFE AND SOUND. FOR NOW.",
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
    "THE MARBLE WILL WAIT. IT'S A MARBLE.",
    "STRETCH THOSE THUMBS",
    "THE SWEEPERS ARE ON BREAK TOO",
    "TIME IS FROZEN. VERY SCIENTIFIC.",
    "HYDRATE. THE ICE INSISTS.",
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
            "MOONSTONE MONARCH",
            "GOLD, NO FALLS AND THE MOONSTONE. SHOW-OFF.",
        )
    } else if run.secret {
        (
            "MAGPIE OF THE PEAKS",
            "YOU FOUND THE MOONSTONE. SHINY THINGS: SECURED.",
        )
    } else if flawless && run.elapsed <= run.gold * 0.85 {
        ("GLASS LIGHTNING", "BLINK AND THE ARCH MISSED YOU")
    } else if run.falls >= 8 {
        ("GRAVITY'S PEN PAL", "YOU AND THE VOID WRITE EVERY WEEK")
    } else if run.falls >= 4 {
        ("PROFESSIONAL FALLER", "THE ABYSS KNOWS YOU BY NAME")
    } else if run.bonks >= 3 {
        ("PINBALL WIZARD", "THE SWEEPERS DID MOST OF THE STEERING")
    } else if run.clutches >= 2 {
        ("EDGE WHISPERER", "YOU LIVE ON THE EDGE. LITERALLY.")
    } else if run.best_air >= 0.7 {
        (
            "FREQUENT FLYER",
            "TECHNICALLY A MARBLE. SPIRITUALLY A BIRD.",
        )
    } else {
        match run.medal {
            "GOLD" => ("POLISHED TO A SHINE", "SMOOTH, QUICK AND SPOTLESS"),
            "SILVER" => ("SMOOTH OPERATOR", "SOLID ROLL. GOLD IS RIGHT THERE."),
            _ => ("SUNDAY ROLLER", "THE SCENIC ROUTE. THE SCENERY APPROVES."),
        }
    };
    Rank { title, quip }
}

/// The "one more try" line: the next medal, then the moonstone rumour, then the
/// stored personal best. `best` is the fastest time after recording this run.
pub(crate) fn next_goal(course: &Course, medal: &str, best: Option<f32>, secret: bool) -> String {
    match medal {
        "BRONZE" => format!(
            "NEXT: SILVER UNDER {:.1} SEC WITH 2 FALLS OR FEWER",
            course.gold_seconds * 1.5
        ),
        "SILVER" => format!(
            "NEXT: GOLD UNDER {:.0} SEC WITH NO FALLS",
            course.gold_seconds
        ),
        _ if !secret => "RUMOUR: A MOONSTONE HIDES OFF THE BEATEN PATH".into(),
        _ => best.map_or_else(
            || "NEXT: GO FASTER. THE ARCH IS WAITING.".into(),
            |best| format!("NEXT: BEAT YOUR BEST OF {}", crate::hud::stopwatch(best)),
        ),
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
        assert_eq!(rank(base).title, "POLISHED TO A SHINE");
        assert_eq!(
            rank(Summary {
                elapsed: 29.,
                ..base
            })
            .title,
            "GLASS LIGHTNING"
        );
        assert_eq!(
            rank(Summary {
                secret: true,
                ..base
            })
            .title,
            "MOONSTONE MONARCH"
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
            "MAGPIE OF THE PEAKS"
        );
        assert_eq!(
            rank(Summary { falls: 9, ..silver }).title,
            "GRAVITY'S PEN PAL"
        );
        assert_eq!(rank(Summary { bonks: 3, ..silver }).title, "PINBALL WIZARD");
        assert_eq!(
            rank(Summary {
                clutches: 2,
                ..silver
            })
            .title,
            "EDGE WHISPERER"
        );
        assert_eq!(
            rank(Summary {
                best_air: 0.8,
                ..silver
            })
            .title,
            "FREQUENT FLYER"
        );
        assert_eq!(rank(silver).title, "SMOOTH OPERATOR");
        assert_eq!(
            rank(Summary {
                medal: "BRONZE",
                ..silver
            })
            .title,
            "SUNDAY ROLLER"
        );
    }

    #[test]
    fn next_goal_climbs_medals_then_hints_the_secret_then_chases_the_best() {
        let c = &COURSES[0];
        assert!(next_goal(c, "BRONZE", None, false).contains("SILVER UNDER 52.5 SEC"));
        assert!(next_goal(c, "SILVER", None, true).contains("GOLD UNDER 35 SEC"));
        assert!(next_goal(c, "GOLD", Some(30.), false).contains("MOONSTONE"));
        assert_eq!(
            next_goal(c, "GOLD", Some(30.256), true),
            "NEXT: BEAT YOUR BEST OF 0:30.26"
        );
    }
}
