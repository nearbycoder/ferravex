//! The yard's voice: loading-dock bowling commentary, short and never mean.
//!
//! Every selector is a pure function of accepted simulation counters, so the
//! same session reads the same way. Consecutive counters never repeat a line.
use super::toys::Goal;

const BOWL: [&str; 6] = [
    "ROLL OUT!",
    "FORE! WRONG SPORT.",
    "HERE COMES THE BIG ONE",
    "BOWLING, BUT LOUDER",
    "LANE ONE IS OPEN",
    "SPECIAL DELIVERY",
];

const PULSE: [&str; 5] = [
    "SHOCKWAVE!",
    "STAND BACK",
    "YARD-WIDE BOOP",
    "NEWTON APPROVES",
    "EVERYBODY JUMP",
];

const COLLAPSE: [&str; 8] = [
    "TIMBER!",
    "PALLET PARTY",
    "INVENTORY: REDISTRIBUTED",
    "THAT'S A LOT OF PAPERWORK",
    "SHIPPING: EXPEDITED",
    "FORKLIFT CERTIFICATION: REVOKED",
    "THE STACK HAS LEFT THE CHAT",
    "GRAVITY CLOCKED IN",
];

const CHAOS: [&str; 5] = [
    "DELIVERY FOR: EVERYONE",
    "SIXTEEN PACKAGES. NO SIGNATURE.",
    "THE YARD REGRETS NOTHING",
    "SAME-DAY SHIPPING",
    "RETURNS DEPARTMENT: OVERWHELMED",
];

const RESET: [&str; 5] = [
    "FRESH STACK. SAME DREAMS.",
    "RESTOCKED",
    "THE FORKLIFTS WORKED OVERTIME",
    "SHELVES: SUSPICIOUSLY TIDY",
    "CLEAN YARD. FOR NOW.",
];

const PAUSE: [&str; 5] = [
    "THE YARD HOLDS ITS BREATH",
    "INVENTORY CHECK",
    "COFFEE BREAK. PHYSICS PAUSED.",
    "FREEZE! NOBODY MOVE.",
    "ONE TICK AT A TIME",
];

const RETURNED: [&str; 4] = [
    "RETURNED TO SENDER",
    "LOST & FOUND DELIVERY",
    "IT CAME BACK",
    "NOTHING LEAVES THE YARD",
];

/// Advances by a stride coprime with the table length.
fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let stride = if table.len().is_multiple_of(5) { 3 } else { 5 };
    table[(counter as usize).wrapping_mul(stride) % table.len()]
}

pub(super) fn bowl(count: u32) -> &'static str {
    rotate(&BOWL, count)
}

pub(super) fn pulse(count: u32) -> &'static str {
    rotate(&PULSE, count)
}

pub(super) fn collapse(count: u32) -> &'static str {
    rotate(&COLLAPSE, count)
}

pub(super) fn chaos(count: u32) -> &'static str {
    rotate(&CHAOS, count)
}

pub(super) fn reset(count: u32) -> &'static str {
    rotate(&RESET, count)
}

pub(super) fn pause(count: u32) -> &'static str {
    rotate(&PAUSE, count)
}

pub(super) fn returned(count: u32) -> &'static str {
    rotate(&RETURNED, count)
}

/// A bowling-style call for one bowl: (headline, color class 0..=3).
pub(super) fn bowl_call(knocked: usize) -> (&'static str, u8) {
    match knocked {
        0 => ("GUTTER BALL", 0),
        1..=3 => ("A LITTLE NUDGE", 1),
        4..=7 => ("SPARE-ISH", 2),
        8..=19 => ("STRIKE!", 3),
        _ => ("TURKEY! MEGA STRIKE!", 3),
    }
}

pub(super) fn goal(goal: Goal) -> (&'static str, &'static str) {
    match goal {
        Goal::Strike => ("GOAL: STRIKE!", "THE CRATES NEVER SAW IT COMING"),
        Goal::Airtime => ("GOAL: AIRTIME!", "CLEARED FOR TAKEOFF. NOT FOR LANDING."),
        Goal::Avalanche => ("GOAL: AVALANCHE!", "THE YARD WILL NEED A MINUTE"),
        Goal::CleanSweep => ("GOAL: CLEAN SWEEP!", "NOT ONE CRATE LEFT STANDING. ART."),
    }
}

pub(super) const SECRET: (&str, &str) = (
    "SECRET: ON A PEDESTAL!",
    "AS IT SHOULD BE. THE YARD BOWS TO YOU.",
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
}

pub(super) fn rank(goals: usize, secret: bool) -> Rank {
    let (title, quip) = match (goals, secret) {
        (4, true) => (
            "YARDMASTER GENERAL, PEDESTAL ARTIST",
            "EVERY GOAL AND THE PEDESTAL. FRAME THIS.",
        ),
        (4, false) => (
            "YARDMASTER GENERAL",
            "EVERY GOAL DONE. RUMOR SAYS A CORNER PEDESTAL IS LONELY.",
        ),
        (3, _) => ("STACK BREAKER", "ONE GOAL LEFT. THE CRATES ARE NERVOUS."),
        (2, _) => ("PALLET WRANGLER", "HALF THE CHECKLIST. ALL OF THE MESS."),
        (1, _) => (
            "FORKLIFT ENTHUSIAST",
            "YOU'VE GOT THE KNACK. AND THE KNOCK.",
        ),
        _ => ("YARD TRAINEE", "HARD HAT ON. LET'S BREAK SOMETHING."),
    };
    Rank { title, quip }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotating_tables_never_repeat_back_to_back_and_visit_every_line() {
        for table in [
            &BOWL[..],
            &PULSE,
            &COLLAPSE,
            &CHAOS,
            &RESET,
            &PAUSE,
            &RETURNED,
        ] {
            let mut seen = std::collections::BTreeSet::new();
            for counter in 0..table.len() as u32 * 2 {
                assert_ne!(rotate(table, counter), rotate(table, counter + 1));
                seen.insert(rotate(table, counter));
            }
            assert_eq!(seen.len(), table.len());
        }
    }

    #[test]
    fn bowl_calls_climb_with_crates_and_ranks_with_goals() {
        let calls: Vec<_> = [0, 2, 5, 10, 25].map(|n| bowl_call(n).1).into();
        assert!(calls.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(bowl_call(12).0, "STRIKE!");
        let titles: std::collections::BTreeSet<_> =
            (0..=4).map(|goals| rank(goals, false).title).collect();
        assert_eq!(titles.len(), 5);
        assert_ne!(rank(4, true), rank(4, false));
        for line in Goal::ALL.iter().flat_map(|g| {
            let (a, b) = goal(*g);
            [a, b]
        }) {
            assert!(line.len() <= 56);
        }
    }
}
