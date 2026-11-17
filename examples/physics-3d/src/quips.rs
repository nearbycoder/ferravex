//! The yard's voice: loading-dock bowling commentary, punchy and never mean.
//!
//! Every selector is a pure function of accepted simulation counters, so the
//! same session reads the same way. Consecutive counters never repeat a line.
//! Callouts shown during play stay within three words; the one-line rank quip
//! appears only on the YARD CERTIFIED card.
use super::toys::Goal;

const BOWL: [&str; 6] = [
    "Roll out!",
    "Fore! Wrong sport.",
    "Big one!",
    "Bowling, but louder",
    "Lane open!",
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
    "Paperwork!",
    "Shipping: expedited",
    "License revoked",
    "Stack left",
    "Gravity clocked in",
];

const CHAOS: [&str; 5] = [
    "Delivery for everyone",
    "No signature!",
    "No regrets!",
    "Same-day shipping",
    "Returns: overwhelmed",
];

const RESET: [&str; 5] = [
    "Fresh stack!",
    "Restocked",
    "Forklift overtime",
    "Suspiciously tidy",
    "Clean yard!",
];

const RETURNED: [&str; 4] = ["Returned!", "Lost & found", "It came back", "No escape!"];

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

/// Celebration headline for a completed goal.
pub(super) fn goal(goal: Goal) -> &'static str {
    match goal {
        Goal::Strike => "Strike!",
        Goal::Airtime => "Airtime!",
        Goal::Avalanche => "Avalanche!",
        Goal::CleanSweep => "Clean sweep!",
    }
}

pub(super) const SECRET: &str = "On a pedestal!";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rank {
    pub title: &'static str,
    /// One short line for the YARD CERTIFIED card.
    pub quip: &'static str,
}

pub(super) fn rank(goals: usize, secret: bool) -> Rank {
    let (title, quip) = match (goals, secret) {
        (4, true) => ("Yardmaster General, Pedestal Artist", "Frame this."),
        (4, false) => ("Yardmaster General", "Every goal. Mostly standing."),
        (3, _) => ("Stack Breaker", "The crates are nervous."),
        (2, _) => ("Pallet Wrangler", "All of the mess."),
        (1, _) => ("Forklift Enthusiast", "Got the knock."),
        _ => ("Yard Trainee", "Hard hat on."),
    };
    Rank { title, quip }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLES: [&[&str]; 6] = [&BOWL, &PULSE, &COLLAPSE, &CHAOS, &RESET, &RETURNED];

    #[test]
    fn rotating_tables_never_repeat_back_to_back_and_visit_every_line() {
        for table in TABLES {
            let mut seen = std::collections::BTreeSet::new();
            for counter in 0..table.len() as u32 * 2 {
                assert_ne!(rotate(table, counter), rotate(table, counter + 1));
                seen.insert(rotate(table, counter));
            }
            assert_eq!(seen.len(), table.len());
        }
    }

    #[test]
    fn bowl_calls_climb_with_crates_and_callouts_keep_the_word_budget() {
        let calls: Vec<_> = [0, 2, 5, 10, 25].map(|n| bowl_call(n).1).into();
        assert!(calls.windows(2).all(|w| w[0] <= w[1]));
        assert_eq!(bowl_call(12).0, "Strike!");
        let titles: std::collections::BTreeSet<_> =
            (0..=4).map(|goals| rank(goals, false).title).collect();
        assert_eq!(titles.len(), 5);
        assert_ne!(rank(4, true), rank(4, false));
        let callouts = TABLES
            .iter()
            .flat_map(|table| table.iter().copied())
            .chain(Goal::ALL.iter().map(|g| goal(*g)))
            .chain([0, 2, 5, 10, 25].map(|n| bowl_call(n).0))
            .chain([SECRET]);
        for line in callouts {
            assert!(line.split_whitespace().count() <= 3, "{line}");
            assert!(line.chars().count() <= 24, "{line}");
        }
        for goals in 0..=4 {
            for secret in [false, true] {
                assert!(rank(goals, secret).quip.split_whitespace().count() <= 5);
            }
        }
    }
}
