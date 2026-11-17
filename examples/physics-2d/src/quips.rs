//! The lab's voice: punchy, table-driven and never mean.
//!
//! Every selector is a pure function of accepted simulation counters, so a
//! replayed session reads the same way. Consecutive counters never repeat the
//! previous line and every line in a table is eventually visited. Callouts
//! shown during play stay within three words; the one-line rank quip appears
//! only on the LAB CERTIFIED card.
use super::toys::Goal;

const PULSE: [&str; 7] = [
    "Boop!",
    "Back off!",
    "Science!",
    "Newton nods",
    "Shove!",
    "Everybody out!",
    "Gentle-ish nudge",
];

const COLLAPSE: [&str; 8] = [
    "Timber!",
    "Integrity: zero",
    "Load-bearing, oops",
    "Not insured!",
    "Gravity wins",
    "Peer reviewed",
    "Encore!",
    "Who built that?",
];

const CHAOS: [&str; 6] = [
    "Raining stuff!",
    "Sign here!",
    "No regrets!",
    "Hazard pay!",
    "Incoming!",
    "Goggles on!",
];

const RESET: [&str; 5] = [
    "Clean slate!",
    "Never happened",
    "Tidy lab",
    "Crates forgive",
    "Square one",
];

const RETURNED: [&str; 4] = ["Returned!", "Lost & found", "It came back!", "No escape!"];

const TOWER: [&str; 5] = [
    "New height!",
    "Taller!",
    "Hold still...",
    "Vertigo!",
    "No glue!",
];

const DOMINO: [&str; 4] = [
    "Clack clack clack",
    "Total topple!",
    "Everybody down!",
    "Flawless tumble",
];

/// Advances by a stride coprime with the table length.
fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let stride = if table.len().is_multiple_of(5) { 3 } else { 5 };
    table[(counter as usize).wrapping_mul(stride) % table.len()]
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

pub(super) fn tower(count: u32) -> &'static str {
    rotate(&TOWER, count)
}

pub(super) fn domino(count: u32) -> &'static str {
    rotate(&DOMINO, count)
}

/// Celebration headline for a completed goal.
pub(super) fn goal(goal: Goal) -> &'static str {
    match goal {
        Goal::Liftoff => "Liftoff!",
        Goal::Skyscraper => "Skyscraper!",
        Goal::Demolition => "Demolition!",
        Goal::DominoRun => "Domino run!",
    }
}

pub(super) const SECRET: &str = "Moon shot!";

/// A rank title for the session, from goals completed and the secret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rank {
    pub title: &'static str,
    /// One short line for the LAB CERTIFIED card.
    pub quip: &'static str,
}

pub(super) fn rank(goals: usize, secret: bool) -> Rank {
    let (title, quip) = match (goals, secret) {
        (4, true) => ("Professor of Chaos, Astronaut", "The lab salutes you."),
        (4, false) => ("Professor of Chaos", "Every goal. Mostly intact."),
        (3, _) => ("Head of Demolition", "One goal to go."),
        (2, _) => ("Senior Stack Whisperer", "Fully unsupervised."),
        (1, _) => ("Junior Gravity Tester", "Gravity: still works."),
        _ => ("Lab Intern", "Touch everything."),
    };
    Rank { title, quip }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLES: [&[&str]; 7] = [
        &PULSE, &COLLAPSE, &CHAOS, &RESET, &RETURNED, &TOWER, &DOMINO,
    ];

    #[test]
    fn rotating_tables_never_repeat_back_to_back_and_visit_every_line() {
        for table in TABLES {
            let mut seen = std::collections::BTreeSet::new();
            for counter in 0..table.len() as u32 * 2 {
                let line = rotate(table, counter);
                assert_ne!(line, rotate(table, counter + 1));
                seen.insert(line);
            }
            assert_eq!(seen.len(), table.len());
        }
    }

    #[test]
    fn play_callouts_keep_the_three_word_budget_and_ranks_climb() {
        let font = ferravex_render::prelude::get_default_font();
        let metrics = font.try_metrics(16, 1.).unwrap();
        let callouts = TABLES
            .iter()
            .flat_map(|table| table.iter().copied())
            .chain(Goal::ALL.iter().map(|g| goal(*g)))
            .chain([SECRET]);
        for line in callouts {
            assert!(line.split_whitespace().count() <= 3, "{line}");
            assert!(line.chars().count() <= 20, "{line}");
            assert!(metrics.measure_line(line).is_ok());
        }
        for goals in 0..=4 {
            for secret in [false, true] {
                let quip = rank(goals, secret).quip;
                assert!(quip.split_whitespace().count() <= 5, "{quip}");
            }
        }
        let titles: Vec<_> = (0..=4).map(|goals| rank(goals, false).title).collect();
        let unique: std::collections::BTreeSet<_> = titles.iter().collect();
        assert_eq!(unique.len(), titles.len());
        assert_ne!(rank(4, true), rank(4, false));
    }
}
