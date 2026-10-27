//! The lab's voice: short, table-driven and never mean.
//!
//! Every selector is a pure function of accepted simulation counters, so a
//! replayed session reads the same way. Consecutive counters never repeat the
//! previous line and every line in a table is eventually visited.
use super::toys::Goal;

const PULSE: [&str; 7] = [
    "Boop.",
    "Personal space: enforced",
    "Science!",
    "Newton nods solemnly",
    "Shove of approval",
    "Everybody out of the pool",
    "A gentle nudge. Ish.",
];

const COLLAPSE: [&str; 8] = [
    "Timber!",
    "Structural integrity: zero",
    "That was load-bearing",
    "The insurance won't cover this",
    "Gravity wins. Again.",
    "Peer review: negative",
    "Beautiful. Do it again.",
    "Who put that there?",
];

const CHAOS: [&str; 6] = [
    "It's raining stuff",
    "Chaos delivered. Sign here.",
    "The lab regrets nothing",
    "Hazard pay approved",
    "You ordered fourteen things",
    "Safety goggles on",
];

const RESET: [&str; 5] = [
    "Fresh lab. No one saw anything.",
    "As if it never happened",
    "Cleanup crew: heroes",
    "The crates forgive you",
    "Back to square one. And triangle.",
];

const PAUSE: [&str; 5] = [
    "Time: stopped. Physics: confused.",
    "The crates hold their breath",
    "Freeze frame. Very dramatic.",
    "Step carefully. One tick at a time.",
    "Inspection mode. No touching. Kidding.",
];

const RETURNED: [&str; 4] = [
    "Returned to sender",
    "Lost & found delivery",
    "It came back. They always do.",
    "Nothing leaves the lab",
];

const TOWER: [&str; 5] = [
    "New height!",
    "Taller!",
    "Hold still...",
    "Vertigo!",
    "Look ma, no glue",
];

const DOMINO: [&str; 4] = [
    "Clack clack clack",
    "The planks have spoken",
    "Everybody down!",
    "A flawless tumble",
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

pub(super) fn pause(count: u32) -> &'static str {
    rotate(&PAUSE, count)
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

/// Celebration copy for a completed goal: (headline, punchline).
pub(super) fn goal(goal: Goal) -> (&'static str, &'static str) {
    match goal {
        Goal::Liftoff => ("Goal: Liftoff!", "Houston, we have a crate problem"),
        Goal::Skyscraper => (
            "Goal: Skyscraper!",
            "Architecturally questionable. Love it.",
        ),
        Goal::Demolition => ("Goal: Demolition!", "The neighbors have questions"),
        Goal::DominoRun => ("Goal: Domino run!", "Clack clack clack clack. Perfect."),
    }
}

pub(super) const SECRET: (&str, &str) = (
    "Secret: Moon shot!",
    "Something left the building. Say hi to the astronauts.",
);

/// A rank title for the session, from goals completed and the secret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
}

pub(super) fn rank(goals: usize, secret: bool) -> Rank {
    let (title, quip) = match (goals, secret) {
        (4, true) => (
            "Professor of Chaos, Astronaut",
            "Every goal and a moon shot. The lab salutes you.",
        ),
        (4, false) => (
            "Professor of Chaos",
            "Every goal done. Rumor says something can fly higher.",
        ),
        (3, _) => (
            "Head of Demolition",
            "One goal left. The planks are waiting.",
        ),
        (2, _) => (
            "Senior Stack Whisperer",
            "Halfway certified. Fully unsupervised.",
        ),
        (1, _) => (
            "Junior Gravity Tester",
            "Gravity: tested. Results: still works.",
        ),
        _ => (
            "Lab Intern",
            "Please don't touch anything. Kidding. Touch everything.",
        ),
    };
    Rank { title, quip }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotating_tables_never_repeat_back_to_back_and_visit_every_line() {
        for table in [
            &PULSE[..],
            &COLLAPSE,
            &CHAOS,
            &RESET,
            &PAUSE,
            &RETURNED,
            &TOWER,
            &DOMINO,
        ] {
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
    fn copy_fits_the_bitmap_font_and_ranks_climb_with_goals() {
        let font = ferravex_render::prelude::get_default_font();
        let metrics = font.try_metrics(16, 1.).unwrap();
        let all = PULSE
            .iter()
            .chain(&COLLAPSE)
            .chain(&CHAOS)
            .chain(&RESET)
            .chain(&PAUSE)
            .chain(&RETURNED)
            .chain(&TOWER)
            .chain(&DOMINO)
            .copied()
            .chain(Goal::ALL.iter().flat_map(|g| {
                let (a, b) = goal(*g);
                [a, b]
            }))
            .chain([SECRET.0, SECRET.1]);
        for line in all {
            assert!(line.chars().count() <= 56, "{line}");
            assert!(metrics.measure_line(line).is_ok());
        }
        let titles: Vec<_> = (0..=4).map(|goals| rank(goals, false).title).collect();
        let unique: std::collections::BTreeSet<_> = titles.iter().collect();
        assert_eq!(unique.len(), titles.len());
        assert_ne!(rank(4, true), rank(4, false));
    }
}
