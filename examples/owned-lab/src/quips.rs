//! Sergeant Gravel's radio callouts: punchy, table-driven and never repeated
//! back to back. During play every line is at most three words; the results
//! card carries one short verdict line.
//!
//! Selectors are pure functions of accepted simulation counters, so the same
//! inputs always read the same way; no clock, RNG or render cadence is involved.
use crate::qualifier::Grade;

/// Radio callsign shown beside every line.
pub(crate) const CALLSIGN: &str = "SGT. GRAVEL";

pub(crate) const WELCOME: &str = "Welcome, recruit!";

/// One line per basic drill, in move/fire/reload/hit order.
const DRILLS: [&str; 4] = [
    "Legs work!",
    "Technically a shot.",
    "Fresh hope!",
    "Complaint filed!",
];

pub(crate) const ARMED: &str = "Make me proud.";
pub(crate) const REARMED: [&str; 4] = [
    "Places, everyone!",
    "Back for more?",
    "Operators, positions!",
    "Just one more.",
];
pub(crate) const GO: &str = "GO GO GO!";

const STREAK: [&str; 6] = [
    "I taught that!",
    "Alert the pigeons!",
    "Noted. Impressed.",
    "Keep going!",
    "Stones applaud!",
    "Fridge-worthy!",
];

const COLD: [&str; 5] = [
    "They move, recruit.",
    "Aim at cardboard.",
    "That's a wall.",
    "Squeeze, don't yank.",
    "Sky says thanks.",
];

pub(crate) const FIRST_HEADSHOT: &str = "Rude. Effective.";
pub(crate) const BELL_SECRET: &str = "Act natural.";
pub(crate) const NEW_BEST: &str = "Not impressed. (Very.)";

const GRADES: [[&str; 3]; 5] = [
    [
        "Not crying. It's dust.",
        "Nothing to yell about.",
        "Frame that card.",
    ],
    [
        "Sharp. Annoyingly sharp.",
        "Helmet still fit?",
        "Letter-home material.",
    ],
    [
        "Acceptable. Barely.",
        "Solid. Like the walls.",
        "Not bad. Not legendary.",
    ],
    [
        "Weathervanes aim better.",
        "Thanks for the fresh air.",
        "Call it a warm-up.",
    ],
    [
        "Aiming at the sky?",
        "The stones are safe.",
        "Legends start here.",
    ],
];

/// Advances by a stride coprime with the table length, so consecutive
/// counters never pick the same line and every line is eventually visited.
fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let stride = if table.len().is_multiple_of(5) { 3 } else { 5 };
    table[(counter as usize).wrapping_mul(stride) % table.len()]
}

pub(crate) fn drill(index: usize) -> &'static str {
    DRILLS[index.min(DRILLS.len() - 1)]
}

pub(crate) fn rearmed(runs: u32) -> &'static str {
    rotate(&REARMED, runs)
}

/// A celebration for streak 5, 10, 15... `streak / 5` picks the line.
pub(crate) fn streak(streak: u32) -> &'static str {
    rotate(&STREAK, streak / 5)
}

/// A nudge after several misses in a row; `nudges` counts earlier nudges.
pub(crate) fn cold(nudges: u32) -> &'static str {
    rotate(&COLD, nudges)
}

/// Results-card line for a grade; `runs` rotates through the grade's lines.
pub(crate) fn grade(grade: Grade, runs: u32) -> &'static str {
    // A table of three cannot use the stride rule; advance by one instead.
    let lines = &GRADES[grade as usize];
    lines[runs as usize % lines.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotating_lines_never_repeat_back_to_back_and_visit_every_line() {
        for (table, pick) in [
            (&REARMED[..], rearmed as fn(u32) -> &'static str),
            (&COLD[..], cold),
        ] {
            let picks: Vec<_> = (0..table.len() as u32 * 2).map(pick).collect();
            assert!(picks.windows(2).all(|w| w[0] != w[1]));
            for line in table {
                assert!(picks.contains(line));
            }
        }
        let streaks: Vec<_> = (1..=12).map(|n| streak(n * 5)).collect();
        assert!(streaks.windows(2).all(|w| w[0] != w[1]));
        for grade_kind in [Grade::S, Grade::A, Grade::B, Grade::C, Grade::D] {
            assert_ne!(grade(grade_kind, 0), grade(grade_kind, 1));
            assert_ne!(grade(grade_kind, 1), grade(grade_kind, 2));
        }
        assert_eq!(grade(Grade::B, 0), "Acceptable. Barely.");
    }

    /// Every line Sgt. Gravel says during play.
    fn callouts() -> Vec<&'static str> {
        DRILLS
            .iter()
            .chain(&REARMED)
            .chain(&STREAK)
            .chain(&COLD)
            .chain(&[WELCOME, ARMED, GO, FIRST_HEADSHOT, BELL_SECRET])
            .copied()
            .collect()
    }

    #[test]
    fn play_callouts_are_three_words_and_fit_the_narrowest_radio_chip() {
        use ferravex_engine::render::text::{TextStyle, text_width};
        // The radio chip leaves its width minus 64px for the line: 14px copy
        // in a 244px chip on 276px hosts (180px column), 16px copy once the
        // chip reaches 360px (296px column).
        for line in callouts() {
            let words = line
                .split_whitespace()
                .filter(|w| w.chars().any(char::is_alphabetic))
                .count();
            assert!(words <= 3, "{line}: {words} words");
            for (size, column) in [(14., 180.), (16., 296.)] {
                let width = text_width(
                    line,
                    TextStyle::modern(size, ferravex_engine::render::WHITE),
                );
                assert!(width <= column, "{line}: {width} > {column}");
            }
        }
    }

    #[test]
    fn verdicts_fit_one_line_of_the_tightest_results_card() {
        use ferravex_engine::render::text::{TextStyle, text_width};
        // A 276px host's tight card is 244px wide with 16px insets.
        let style = TextStyle::modern(14., ferravex_engine::render::WHITE);
        for line in GRADES.iter().flatten().chain(&[NEW_BEST]) {
            let quoted = format!("\"{line}\"");
            let width = text_width(&quoted, style);
            assert!(width <= 212., "{line}: {width}");
        }
    }
}
