//! Sergeant Gravel's radio lines: short, table-driven and never repeated back to back.
//!
//! Selectors are pure functions of accepted simulation counters, so the same
//! inputs always read the same way; no clock, RNG or render cadence is involved.
use crate::qualifier::Grade;

/// Radio callsign shown above every line.
pub(crate) const CALLSIGN: &str = "SGT. GRAVEL";

pub(crate) const WELCOME: &str = "Welcome to the Courtyard, recruit. Mind the stonework.";

/// One line per basic drill, in move/fire/reload/hit order.
const DRILLS: [&str; 4] = [
    "Five meters. Your legs work. Splendid.",
    "That was a shot. Technically.",
    "Reloaded. Fresh magazine, fresh hope.",
    "A hit! That operator is filing a complaint.",
];

pub(crate) const ARMED: &str = "Basic drills done. Now the real test: 30 seconds. Make me proud.";
pub(crate) const REARMED: [&str; 4] = [
    "Bell rung. Places, everyone!",
    "Back for more? I like the attitude.",
    "Again! The operators were just getting comfortable.",
    "One more run. They always say one more.",
];
pub(crate) const GO: &str = "GO GO GO!";

const STREAK: [&str; 6] = [
    "Who taught you that? Me. I did.",
    "Somebody alert the pigeons.",
    "I'm writing this one down.",
    "Stop it. No, keep going.",
    "The stones are applauding. Quietly.",
    "This is going on the fridge.",
];

const COLD: [&str; 5] = [
    "The operators are the ones walking around.",
    "Aim at the people-shaped cardboard, recruit.",
    "That wall is not a target. It's a wall.",
    "Deep breath. Squeeze, don't yank.",
    "The sky thanks you for your attention.",
];

pub(crate) const FIRST_HEADSHOT: &str = "Right between the ears. Rude, but effective.";
pub(crate) const BELL_SECRET: &str = "DING! +500. Nobody told you about that. Act natural.";
pub(crate) const NEW_BEST: &str = "I am not impressed. (I am extremely impressed.)";

const GRADES: [[&str; 3]; 5] = [
    [
        "Outstanding. I'm not crying, it's courtyard dust.",
        "I have nothing to yell about. This is new for me.",
        "Frame that card. Then do it again.",
    ],
    [
        "Sharp. Annoyingly sharp.",
        "Good. Don't let it go to your helmet.",
        "That's a grade I can put in a letter home.",
    ],
    [
        "Acceptable. Barely.",
        "Solid. Like the walls you kept hitting.",
        "Not bad. Not legendary. Not bad.",
    ],
    [
        "I've seen better aim from a weathervane.",
        "The operators thank you for the fresh air.",
        "We'll call that a warm-up. Loudly.",
    ],
    [
        "Were you aiming at the sky on purpose?",
        "The stones are safe. So were the operators.",
        "Every legend starts somewhere. Yours starts here.",
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

    #[test]
    fn every_line_fits_every_radio_layout() {
        // The 5x7 bitmap font is 0.75 * size wide per character. Text columns
        // are the radio card width minus 24: narrow 276px hosts (220px, four
        // lines), compact 320px (264px, three), 800px wide (324px, three).
        let all: Vec<&str> = DRILLS
            .iter()
            .chain(&REARMED)
            .chain(&STREAK)
            .chain(&COLD)
            .chain(GRADES.iter().flatten())
            .chain(&[WELCOME, ARMED, GO, FIRST_HEADSHOT, BELL_SECRET, NEW_BEST])
            .copied()
            .collect();
        for (column, size, max) in [
            (220., 14., 4),
            (264., 14., 3),
            (324., 14., 3),
            (436., 16., 3),
        ] {
            let per_line = (column / (0.75 * size)) as usize;
            for line in &all {
                let mut lines = 1;
                let mut width = 0;
                for word in line.split(' ') {
                    let next = if width == 0 {
                        word.len()
                    } else {
                        width + 1 + word.len()
                    };
                    if next > per_line {
                        lines += 1;
                        width = word.len();
                    } else {
                        width = next;
                    }
                }
                assert!(lines <= max, "{line} needs {lines} lines in {column}px");
                assert!(line.is_ascii(), "bitmap font is ASCII");
            }
        }
    }
}
