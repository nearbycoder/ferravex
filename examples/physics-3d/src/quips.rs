//! The yard's voice: loading-dock bowling commentary, short and never mean.
//!
//! Every selector is a pure function of accepted simulation counters, so the
//! same session reads the same way. Consecutive counters never repeat a line.
use super::toys::Goal;

const BOWL: [&str; 6] = [
    "Roll out!",
    "Fore! Wrong sport.",
    "Here comes the big one",
    "Bowling, but louder",
    "Lane one is open",
    "Special delivery",
];

const PULSE: [&str; 5] = [
    "Shockwave!",
    "Stand back",
    "Yard-wide boop",
    "Newton approves",
    "Everybody jump",
];

const COLLAPSE: [&str; 8] = [
    "Timber!",
    "Pallet party",
    "Inventory: redistributed",
    "That's a lot of paperwork",
    "Shipping: expedited",
    "Forklift certification: revoked",
    "The stack has left the chat",
    "Gravity clocked in",
];

const CHAOS: [&str; 5] = [
    "Delivery for: everyone",
    "Sixteen packages. No signature.",
    "The yard regrets nothing",
    "Same-day shipping",
    "Returns department: overwhelmed",
];

const RESET: [&str; 5] = [
    "Fresh stack. Same dreams.",
    "Restocked",
    "The forklifts worked overtime",
    "Shelves: suspiciously tidy",
    "Clean yard. For now.",
];

const PAUSE: [&str; 5] = [
    "The yard holds its breath",
    "Inventory check",
    "Coffee break. Physics paused.",
    "Freeze! Nobody move.",
    "One tick at a time",
];

const RETURNED: [&str; 4] = [
    "Returned to sender",
    "Lost & found delivery",
    "It came back",
    "Nothing leaves the yard",
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
        0 => ("Gutter ball", 0),
        1..=3 => ("A little nudge", 1),
        4..=7 => ("Spare-ish", 2),
        8..=19 => ("Strike!", 3),
        _ => ("Turkey! Mega strike!", 3),
    }
}

pub(super) fn goal(goal: Goal) -> (&'static str, &'static str) {
    match goal {
        Goal::Strike => ("Goal: Strike!", "The crates never saw it coming"),
        Goal::Airtime => ("Goal: Airtime!", "Cleared for takeoff. Not for landing."),
        Goal::Avalanche => ("Goal: Avalanche!", "The yard will need a minute"),
        Goal::CleanSweep => ("Goal: Clean sweep!", "Not one crate left standing. Art."),
    }
}

pub(super) const SECRET: (&str, &str) = (
    "Secret: On a pedestal!",
    "As it should be. The yard bows to you.",
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
}

pub(super) fn rank(goals: usize, secret: bool) -> Rank {
    let (title, quip) = match (goals, secret) {
        (4, true) => (
            "Yardmaster General, Pedestal Artist",
            "Every goal and the pedestal. Frame this.",
        ),
        (4, false) => (
            "Yardmaster General",
            "Every goal done. Rumor says a corner pedestal is lonely.",
        ),
        (3, _) => ("Stack Breaker", "One goal left. The crates are nervous."),
        (2, _) => ("Pallet Wrangler", "Half the checklist. All of the mess."),
        (1, _) => (
            "Forklift Enthusiast",
            "You've got the knack. And the knock.",
        ),
        _ => ("Yard Trainee", "Hard hat on. Let's break something."),
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
        assert_eq!(bowl_call(12).0, "Strike!");
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
