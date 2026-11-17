//! The cave's dry personality: zone names, echo lines, whispered hints, the
//! lost miner's note, pause lines and results-card ranks.
//!
//! Every selector is a pure function of accepted simulation counters, so the
//! same run always reads the same way. No clock, RNG or render cadence is
//! involved. Rotating tables never repeat a line on consecutive counters.

/// Named areas of the authored map, in [`super::zone_at`] order, each with
/// the cave's three-word (or shorter) answer. Base Camp shows the objective
/// chain as pictograms instead of a line.
pub(super) const ZONES: [(&str, &str); 6] = [
    ("BASE CAMP", ""),
    ("THE DRIP GALLERY", "Plink. Plonk. Plink."),
    ("THE CROSSROADS", "Choose wisely."),
    ("THE ECHOING GROTTO", "Hello? ...ello?"),
    ("THE GLITTER POCKET", "Ooh. Sparkly."),
    ("HALL OF ALTARS", "Both asleep. Relatable."),
];

// The echo's lines are three words at most: a glance, then its own echo.
const PICKUP: [&str; 5] = [
    "Shiny. Very shiny.",
    "Pocketed.",
    "Very relieved.",
    "Smugness rising.",
    "Happy little hum.",
];

const LOCKED_ALTAR: [&str; 4] = [
    "Ignored. Rude.",
    "Crystals first.",
    "Crystals, please.",
    "Fake sleeping.",
];

const LOCKED_SEAL: [&str; 4] = [
    "Does not budge.",
    "Nobody home.",
    "Pickle-jar tight.",
    "Deeply unimpressed.",
];

const BONK: [&str; 6] = [
    "Wall was first.",
    "Solid rock.",
    "Wall says hi.",
    "Ow, says wall.",
    "Bonk. Bonk. Bonk.",
    "Walls: undefeated.",
];

const PAUSE: [&str; 5] = [
    "The cave will wait. It's good at that.",
    "The crystals are holding their hum.",
    "Stalactites: still hanging in there.",
    "Take five. The rocks took millions.",
    "The echo is on a break too.",
];

const RETRY: [&str; 4] = [
    "Cave forgets. Mostly.",
    "Crystals: who?",
    "Steve says hi.",
    "New you.",
];

const NOTE: [&str; 4] = [
    "DAY 3: The little purple crystal is Steve. Steve is shy.",
    "DAY 9: The echo keeps finishing my sentences. Rude.",
    "DAY 14: Found daylight! Went back for my hat. Bad idea.",
    "P.S. If found, please return hat. -Wendell",
];

/// Advances by a stride coprime with the table length, so the same table never
/// repeats on consecutive counters and every line is visited.
fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let stride = if table.len().is_multiple_of(5) { 3 } else { 5 };
    table[(counter as usize).wrapping_mul(stride) % table.len()]
}

/// The repeated, fading tail of a line: "Rude." gives "...rude ...ude ...de".
pub(super) fn echo_tail(line: &str) -> String {
    let word: String = line
        .split_whitespace()
        .last()
        .unwrap_or("")
        .chars()
        .filter(char::is_ascii_alphabetic)
        .collect::<String>()
        .to_ascii_lowercase();
    let mut tail = String::new();
    for skip in 0..3 {
        if word.len() > skip + 1 {
            if !tail.is_empty() {
                tail.push(' ');
            }
            tail.push_str("...");
            tail.push_str(&word[skip..]);
        }
    }
    tail
}

/// `count` is the run's crystal total including this pickup; `run` varies the
/// first line between expeditions without repeating within one.
pub(super) fn pickup(count: usize, run: u32) -> &'static str {
    if count >= 3 {
        return "Three for three!";
    }
    rotate(&PICKUP, run.wrapping_mul(2).wrapping_add(count as u32))
}

/// `attempts` counts rejected uses this run, including this one.
pub(super) fn locked(seal: bool, attempts: u32) -> &'static str {
    if seal {
        rotate(&LOCKED_SEAL, attempts)
    } else {
        rotate(&LOCKED_ALTAR, attempts)
    }
}

/// `bonks` is the run's total including this one.
pub(super) fn bonk(bonks: u32) -> &'static str {
    rotate(&BONK, bonks)
}

pub(super) fn pause(pauses: u32) -> &'static str {
    rotate(&PAUSE, pauses)
}

pub(super) fn retry(runs: u32) -> &'static str {
    rotate(&RETRY, runs)
}

/// Pages repeat in order; the note is short enough to read in passing.
pub(super) const NOTE_PAGES: u32 = NOTE.len() as u32;

pub(super) fn note(page: u32) -> &'static str {
    NOTE[page as usize % NOTE.len()]
}

/// The echo's comment on an awakened altar (0) or the opening seal (1).
pub(super) fn altar(index: u8) -> &'static str {
    if index == 0 {
        "Grumpy, but helpful."
    } else {
        "Big clunk."
    }
}

/// Escalating whispers, three words at most; the HUD adds the visuals (the
/// HUM meter glows at the first whisper, a heading marker from the second).
/// `zone` names where the nearest missing crystal is.
pub(super) fn hint(stage: usize, level: u8, zone: usize) -> &'static str {
    match (stage, level) {
        (_, 0) => "",
        (0..=2, 1) => "Listen closer.",
        (3, 1) => "West side.",
        (4, 1) => "East side.",
        (_, 1) => "Head north.",
        (_, 2) => "Follow the diamond.",
        // The third whisper names the place (a proper name, not a sentence).
        (0..=2, _) => ZONES[zone].0,
        (3 | 4, _) => "Get closer.",
        _ => "Into the sun.",
    }
}

/// Seconds a clean (hint-free) run must beat for the fastest titles.
pub(super) const PRODIGY_SECONDS: f32 = 70.;
pub(super) const SURE_FOOTED_SECONDS: f32 = 110.;

/// The finished run's summary, independent of storage and rendering.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct RunSummary {
    pub seconds: f32,
    pub hints: u32,
    pub bonks: u32,
    pub shy: bool,
    pub note: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
}

/// Silly, never mean, ordered from the rarest feat to the general titles.
pub(super) fn rank(run: RunSummary) -> Rank {
    let (title, quip) = if run.shy && run.note && run.hints == 0 {
        (
            "LEGEND OF THE DEEP",
            "Every secret, no hints. The cave wants your autograph.",
        )
    } else if run.shy {
        (
            "STEVE'S BEST FRIEND",
            "The shy crystal finally has a friend.",
        )
    } else if run.note {
        (
            "WENDELL'S PEN PAL",
            "He'd be proud. He's still down there, though.",
        )
    } else if run.hints == 0 && run.seconds < PRODIGY_SECONDS {
        (
            "SPELUNKING PRODIGY",
            "You didn't need the echo. The echo is a bit hurt.",
        )
    } else if run.bonks >= 8 {
        (
            "PROFESSIONAL WALL TESTER",
            "Every wall checked. Some of them twice.",
        )
    } else if run.hints == 0 && run.seconds < SURE_FOOTED_SECONDS {
        ("SURE-FOOTED SPELUNKER", "Steady boots. Steady lamp. Nice.")
    } else if run.hints >= 3 {
        (
            "WELL-ADVISED WANDERER",
            "The echo would like a thank-you card.",
        )
    } else {
        (
            "SCENIC ROUTE SURVEYOR",
            "You saw a lot of rocks. Lovely rocks.",
        )
    };
    Rank { title, quip }
}

/// The one thing worth chasing on the next expedition, drawn as pictograms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NextGoal {
    /// Neither secret yet: a shy fourth crystal hides somewhere.
    Steve,
    /// The note is found, Steve is not (he is behind you, at Base Camp).
    LookBehind,
    /// Steve is found; the note waits in the Echoing Grotto.
    Note,
    /// Every secret, but with hints: now with none.
    NoHints,
    /// Everything, hint-free: now faster.
    Faster,
}

pub(super) fn next_goal(run: RunSummary) -> NextGoal {
    if !run.shy && !run.note {
        NextGoal::Steve
    } else if !run.shy {
        NextGoal::LookBehind
    } else if !run.note {
        NextGoal::Note
    } else if run.hints > 0 {
        NextGoal::NoHints
    } else {
        NextGoal::Faster
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotating_tables_never_repeat_back_to_back_and_visit_every_line() {
        for table in [
            &PICKUP[..],
            &LOCKED_ALTAR[..],
            &LOCKED_SEAL[..],
            &BONK[..],
            &PAUSE[..],
            &RETRY[..],
        ] {
            let visited: std::collections::BTreeSet<_> =
                (0..table.len() as u32).map(|n| rotate(table, n)).collect();
            assert_eq!(visited.len(), table.len());
            for n in 0..40 {
                assert_ne!(rotate(table, n), rotate(table, n + 1));
            }
        }
        for run in 0..8 {
            assert_ne!(pickup(1, run), pickup(2, run));
        }
        assert_eq!(pickup(3, 7), "Three for three!");
    }

    #[test]
    fn echo_tails_fade_out_of_the_last_word() {
        assert_eq!(echo_tail("Ignored. Rude."), "...rude ...ude ...de");
        assert_eq!(echo_tail("Hi"), "...hi");
        assert_eq!(echo_tail(""), "");
    }

    #[test]
    fn hints_escalate_and_name_real_places() {
        assert!(hint(0, 0, 3).is_empty());
        assert_eq!(hint(1, 1, 3), "Listen closer.");
        assert_eq!(hint(2, 2, 3), "Follow the diamond.");
        assert_eq!(hint(0, 3, 3), "THE ECHOING GROTTO");
        assert_eq!(hint(0, 3, 5), "HALL OF ALTARS");
        assert_eq!(hint(3, 1, 0), "West side.");
        assert_eq!(hint(5, 1, 0), "Head north.");
        for stage in 0..6 {
            for level in 1..4 {
                let line = hint(stage, level, 1);
                assert!(!line.is_empty() && line.split_whitespace().count() <= 3);
            }
        }
    }

    #[test]
    fn ranks_reward_rare_feats_first_and_always_offer_a_next_goal() {
        let base = RunSummary {
            seconds: 50.,
            ..Default::default()
        };
        assert_eq!(rank(base).title, "SPELUNKING PRODIGY");
        assert_eq!(
            rank(RunSummary {
                shy: true,
                note: true,
                ..base
            })
            .title,
            "LEGEND OF THE DEEP"
        );
        assert_eq!(
            rank(RunSummary {
                shy: true,
                note: true,
                hints: 1,
                ..base
            })
            .title,
            "STEVE'S BEST FRIEND"
        );
        assert_eq!(
            rank(RunSummary { note: true, ..base }).title,
            "WENDELL'S PEN PAL"
        );
        assert_eq!(
            rank(RunSummary {
                seconds: 90.,
                ..base
            })
            .title,
            "SURE-FOOTED SPELUNKER"
        );
        assert_eq!(
            rank(RunSummary {
                seconds: 90.,
                bonks: 9,
                ..base
            })
            .title,
            "PROFESSIONAL WALL TESTER"
        );
        assert_eq!(
            rank(RunSummary {
                seconds: 200.,
                hints: 4,
                ..base
            })
            .title,
            "WELL-ADVISED WANDERER"
        );
        assert_eq!(
            rank(RunSummary {
                seconds: 200.,
                hints: 1,
                ..base
            })
            .title,
            "SCENIC ROUTE SURVEYOR"
        );
        assert_eq!(next_goal(base), NextGoal::Steve);
        assert_eq!(
            next_goal(RunSummary { note: true, ..base }),
            NextGoal::LookBehind
        );
        assert_eq!(next_goal(RunSummary { shy: true, ..base }), NextGoal::Note);
        let all = RunSummary {
            shy: true,
            note: true,
            ..base
        };
        assert_eq!(next_goal(all), NextGoal::Faster);
        assert_eq!(next_goal(RunSummary { hints: 2, ..all }), NextGoal::NoHints);
    }

    #[test]
    fn short_copy_fits_the_narrow_overlay() {
        // 320px overlays wrap at 21 columns; three lines keep notices whole.
        for page in 0..4 {
            assert!(note(page).len() <= 60, "{}", note(page));
        }
        for line in PICKUP
            .iter()
            .chain(&LOCKED_ALTAR)
            .chain(&LOCKED_SEAL)
            .chain(&BONK)
            .chain(&RETRY)
            .chain(&[pickup(3, 0), altar(0), altar(1)])
            .chain(ZONES.iter().flat_map(|(name, echo)| [name, echo]))
        {
            assert!(line.len() <= 38, "{line}");
            // In-play callouts are a glance: three words at most.
            assert!(line.split_whitespace().count() <= 3, "{line}");
        }
    }
}
