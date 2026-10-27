//! The lab's voice: short, table-driven and never mean.
//!
//! Every selector is a pure function of accepted simulation counters, so a
//! replayed session reads the same way. Consecutive counters never repeat the
//! previous line and every line in a table is eventually visited.
use super::toys::Goal;

const PULSE: [&str; 7] = [
    "BOOP.",
    "PERSONAL SPACE: ENFORCED",
    "SCIENCE!",
    "NEWTON NODS SOLEMNLY",
    "SHOVE OF APPROVAL",
    "EVERYBODY OUT OF THE POOL",
    "A GENTLE NUDGE. ISH.",
];

const COLLAPSE: [&str; 8] = [
    "TIMBER!",
    "STRUCTURAL INTEGRITY: ZERO",
    "THAT WAS LOAD-BEARING",
    "THE INSURANCE WON'T COVER THIS",
    "GRAVITY WINS. AGAIN.",
    "PEER REVIEW: NEGATIVE",
    "BEAUTIFUL. DO IT AGAIN.",
    "WHO PUT THAT THERE?",
];

const CHAOS: [&str; 6] = [
    "IT'S RAINING STUFF",
    "CHAOS DELIVERED. SIGN HERE.",
    "THE LAB REGRETS NOTHING",
    "HAZARD PAY APPROVED",
    "YOU ORDERED FOURTEEN THINGS",
    "SAFETY GOGGLES ON",
];

const RESET: [&str; 5] = [
    "FRESH LAB. NO ONE SAW ANYTHING.",
    "AS IF IT NEVER HAPPENED",
    "CLEANUP CREW: HEROES",
    "THE CRATES FORGIVE YOU",
    "BACK TO SQUARE ONE. AND TRIANGLE.",
];

const PAUSE: [&str; 5] = [
    "TIME: STOPPED. PHYSICS: CONFUSED.",
    "THE CRATES HOLD THEIR BREATH",
    "FREEZE FRAME. VERY DRAMATIC.",
    "STEP CAREFULLY. ONE TICK AT A TIME.",
    "INSPECTION MODE. NO TOUCHING. KIDDING.",
];

const RETURNED: [&str; 4] = [
    "RETURNED TO SENDER",
    "LOST & FOUND DELIVERY",
    "IT CAME BACK. THEY ALWAYS DO.",
    "NOTHING LEAVES THE LAB",
];

const TOWER: [&str; 5] = [
    "NEW HEIGHT!",
    "TALLER!",
    "HOLD STILL...",
    "VERTIGO!",
    "LOOK MA, NO GLUE",
];

const DOMINO: [&str; 4] = [
    "CLACK CLACK CLACK",
    "THE PLANKS HAVE SPOKEN",
    "EVERYBODY DOWN!",
    "A FLAWLESS TUMBLE",
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
        Goal::Liftoff => ("GOAL: LIFTOFF!", "HOUSTON, WE HAVE A CRATE PROBLEM"),
        Goal::Skyscraper => (
            "GOAL: SKYSCRAPER!",
            "ARCHITECTURALLY QUESTIONABLE. LOVE IT.",
        ),
        Goal::Demolition => ("GOAL: DEMOLITION!", "THE NEIGHBORS HAVE QUESTIONS"),
        Goal::DominoRun => ("GOAL: DOMINO RUN!", "CLACK CLACK CLACK CLACK. PERFECT."),
    }
}

pub(super) const SECRET: (&str, &str) = (
    "SECRET: MOON SHOT!",
    "SOMETHING LEFT THE BUILDING. SAY HI TO THE ASTRONAUTS.",
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
            "PROFESSOR OF CHAOS, ASTRONAUT",
            "EVERY GOAL AND A MOON SHOT. THE LAB SALUTES YOU.",
        ),
        (4, false) => (
            "PROFESSOR OF CHAOS",
            "EVERY GOAL DONE. RUMOR SAYS SOMETHING CAN FLY HIGHER.",
        ),
        (3, _) => (
            "HEAD OF DEMOLITION",
            "ONE GOAL LEFT. THE PLANKS ARE WAITING.",
        ),
        (2, _) => (
            "SENIOR STACK WHISPERER",
            "HALFWAY CERTIFIED. FULLY UNSUPERVISED.",
        ),
        (1, _) => (
            "JUNIOR GRAVITY TESTER",
            "GRAVITY: TESTED. RESULTS: STILL WORKS.",
        ),
        _ => (
            "LAB INTERN",
            "PLEASE DON'T TOUCH ANYTHING. KIDDING. TOUCH EVERYTHING.",
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
