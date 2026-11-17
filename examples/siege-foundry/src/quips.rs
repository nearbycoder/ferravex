//! Foreman Gus, his opinions, and the silly titles on the results card.
//!
//! Selectors are pure functions of counters, so the same run reads the same
//! way. Lines Gus says during play are punchy callouts of about three words;
//! the pause card and results ranks may carry one longer joke. Each category advances by a stride coprime with its table length, so
//! a category never says the same line twice in a row, and [`Banter`] never
//! lets two consecutive bubbles match even across categories.

/// What just happened, from the foreman's point of view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mood {
    Start,
    Retry,
    Wobble,
    Knocked,
    Miss,
    MissStreak,
    Domino,
    Wonder,
    Bell,
    LastShell,
    Settling,
}

const START: [&str; 5] = [
    "No pressure, chief.",
    "Just stacked those!",
    "Mind the paint!",
    "Load-bearing. Allegedly.",
    "Safety first!",
];

const RETRY: [&str; 6] = [
    "Extra glue now.",
    "Back again?",
    "Round two!",
    "Kettle's on.",
    "Oof. My back.",
    "Gently this time.",
];

const WOBBLE: [&str; 5] = [
    "Wobble... wobble...",
    "Easy... easy...",
    "It's fine. Fine.",
    "Don't you dare!",
    "Steady, old girl...",
];

const KNOCKED: [&str; 7] = [
    "My tower!",
    "That was load-bearing!",
    "Fetch the broom.",
    "My weekend!",
    "Who signed off?!",
    "Not insured!",
    "Named that one!",
];

const MISS: [&str; 7] = [
    "Birds say thanks.",
    "Wind, probably.",
    "Warning shot?",
    "Sky's no target.",
    "Bold. Wrong.",
    "Clouds felt that.",
    "Noted. In ink.",
];

const MISS_STREAK: [&str; 4] = [
    "Aim low, chief!",
    "Follow the dots!",
    "Lower. Please.",
    "Supports, chief!",
];

const DOMINO: [&str; 3] = ["Holding hands!", "Domino rules!", "Two for one!"];

const WONDER: [&str; 2] = ["One ball?! One?!", "Calling my mum."];

const BELL: [&str; 3] = ["Lunch bell?!", "Ding! Lunch?", "Not the bell!"];

const LAST_SHELL: [&str; 3] = ["Last shell!", "Make it count.", "One left!"];

const SETTLING: [&str; 3] = ["C'mon... c'mon...", "Any second...", "Lean, you!"];

const PAUSE: [&str; 5] = [
    "Union-mandated tea break.",
    "The rubble will wait.",
    "Gus is on his sandwich.",
    "The towers hold their breath.",
    "Hard hats off. Briefly.",
];

/// Advances by a stride coprime with the table length, so a table never
/// repeats on consecutive counters and every line is visited.
pub(super) fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    fn gcd(a: usize, b: usize) -> usize {
        if b == 0 { a } else { gcd(b, a % b) }
    }
    let len = table.len();
    let stride = [3, 5, 7, 1]
        .into_iter()
        .find(|stride| gcd(*stride, len) == 1)
        .unwrap_or(1);
    table[(counter as usize).wrapping_mul(stride) % len]
}

fn table(mood: Mood) -> &'static [&'static str] {
    match mood {
        Mood::Start => &START,
        Mood::Retry => &RETRY,
        Mood::Wobble => &WOBBLE,
        Mood::Knocked => &KNOCKED,
        Mood::Miss => &MISS,
        Mood::MissStreak => &MISS_STREAK,
        Mood::Domino => &DOMINO,
        Mood::Wonder => &WONDER,
        Mood::Bell => &BELL,
        Mood::LastShell => &LAST_SHELL,
        Mood::Settling => &SETTLING,
    }
}

fn index(mood: Mood) -> usize {
    mood as usize
}

/// Per-category counters. Kept across rebuilds so a retry hears new lines.
#[derive(Clone, Debug, Default)]
pub(super) struct Banter {
    counters: [u32; 11],
    last: &'static str,
    pauses: u32,
}

impl Banter {
    /// The next line for `mood`, never equal to the previous line.
    pub(super) fn say(&mut self, mood: Mood) -> &'static str {
        let table = table(mood);
        let counter = &mut self.counters[index(mood)];
        let mut line = rotate(table, *counter);
        *counter = counter.wrapping_add(1);
        if line == self.last {
            line = rotate(table, *counter);
            *counter = counter.wrapping_add(1);
        }
        self.last = line;
        line
    }

    /// A new pause line each time the range is paused.
    pub(super) fn pause(&mut self) {
        self.pauses = self.pauses.wrapping_add(1);
    }

    pub(super) fn pause_line(&self) -> &'static str {
        rotate(&PAUSE, self.pauses)
    }
}

/// Every line Gus or the cards can show, for layout checks.
#[cfg(test)]
pub(super) fn all_lines() -> impl Iterator<Item = &'static str> {
    [
        &START[..],
        &RETRY,
        &WOBBLE,
        &KNOCKED,
        &MISS,
        &MISS_STREAK,
        &DOMINO,
        &WONDER,
        &BELL,
        &LAST_SHELL,
        &SETTLING,
        &PAUSE,
    ]
    .into_iter()
    .flatten()
    .copied()
}

/// What a finished run gets called on the results card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
}

/// A finished run, independent of storage and rendering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct RunSummary {
    pub won: bool,
    pub stars: u8,
    pub targets: u32,
    pub misses: u32,
    pub wonder: bool,
    pub domino: bool,
    pub bell: bool,
}

/// Silly, never mean, ordered from rarest feat to the general tiers.
pub(super) fn rank(run: RunSummary) -> Rank {
    let (title, quip) = if run.won && run.wonder {
        ("WRECKING BALL WIZARD", "Physics has filed a complaint.")
    } else if run.won && run.stars == 3 && run.bell {
        ("FOUNDRY LEGEND", "Three stars and a ding. Show-off.")
    } else if run.bell && !run.won {
        ("BELL RINGER", "Wrong target. Great tone.")
    } else if run.won && run.stars == 3 {
        ("MASTER DEMOLISHER", "Surgical. Slightly terrifying.")
    } else if run.won && run.domino {
        ("DOMINO DIRECTOR", "Why hit two when one will do?")
    } else if run.won && run.stars == 2 {
        ("CERTIFIED TOPPLER", "The towers took early retirement.")
    } else if run.won {
        ("PERSISTENT PLINKER", "They fell eventually. Out of pity?")
    } else if run.misses >= 5 {
        ("SKY ARTILLERIST", "The clouds never saw it coming.")
    } else if run.targets >= 4 {
        ("SO CLOSE, SO RUBBLE", "One more shell. Just one.")
    } else if run.targets >= 1 {
        ("APPRENTICE LOBBER", "The towers are being smug about it.")
    } else {
        ("PEACEFUL FOREMAN", "Not one block harmed. Very polite.")
    };
    Rank { title, quip }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOODS: [Mood; 11] = [
        Mood::Start,
        Mood::Retry,
        Mood::Wobble,
        Mood::Knocked,
        Mood::Miss,
        Mood::MissStreak,
        Mood::Domino,
        Mood::Wonder,
        Mood::Bell,
        Mood::LastShell,
        Mood::Settling,
    ];

    #[test]
    fn rotation_visits_every_line_and_never_repeats_back_to_back() {
        for table in MOODS.map(table).into_iter().chain([&PAUSE[..]]) {
            let visited: std::collections::BTreeSet<_> =
                (0..table.len() as u32).map(|n| rotate(table, n)).collect();
            assert_eq!(visited.len(), table.len());
            for n in 0..40 {
                if table.len() > 1 {
                    assert_ne!(rotate(table, n), rotate(table, n + 1));
                }
            }
        }
        let mut banter = Banter::default();
        let mut previous = "";
        for n in 0..500_u32 {
            // A deterministic mix that frequently repeats the same mood.
            let mood = MOODS[(n as usize * 7 / 3) % MOODS.len()];
            let line = banter.say(mood);
            assert_ne!(line, previous, "bubble {n} repeated {line}");
            assert!(table(mood).contains(&line));
            previous = line;
        }
        for _ in 0..3 {
            assert_ne!(banter.say(Mood::Wonder), banter.say(Mood::Wonder));
        }
    }

    #[test]
    fn play_lines_stay_punchy() {
        for mood in MOODS {
            for line in table(mood) {
                assert!(line.split_whitespace().count() <= 3, "{line}");
            }
        }
    }

    #[test]
    fn pause_lines_rotate_with_each_pause() {
        let mut banter = Banter::default();
        let first = banter.pause_line();
        banter.pause();
        assert_ne!(banter.pause_line(), first);
    }

    #[test]
    fn ranks_reward_rare_feats_first_and_every_outcome_has_a_title() {
        let win = RunSummary {
            won: true,
            stars: 3,
            targets: 6,
            ..Default::default()
        };
        assert_eq!(rank(win).title, "MASTER DEMOLISHER");
        assert_eq!(
            rank(RunSummary { bell: true, ..win }).title,
            "FOUNDRY LEGEND"
        );
        assert_eq!(
            rank(RunSummary {
                wonder: true,
                bell: true,
                ..win
            })
            .title,
            "WRECKING BALL WIZARD"
        );
        assert_eq!(
            rank(RunSummary {
                stars: 2,
                domino: true,
                ..win
            })
            .title,
            "DOMINO DIRECTOR"
        );
        assert_eq!(
            rank(RunSummary { stars: 2, ..win }).title,
            "CERTIFIED TOPPLER"
        );
        assert_eq!(
            rank(RunSummary { stars: 1, ..win }).title,
            "PERSISTENT PLINKER"
        );
        let loss = RunSummary::default();
        assert_eq!(rank(loss).title, "PEACEFUL FOREMAN");
        assert_eq!(
            rank(RunSummary { targets: 2, ..loss }).title,
            "APPRENTICE LOBBER"
        );
        assert_eq!(
            rank(RunSummary { targets: 5, ..loss }).title,
            "SO CLOSE, SO RUBBLE"
        );
        assert_eq!(
            rank(RunSummary { misses: 6, ..loss }).title,
            "SKY ARTILLERIST"
        );
        assert_eq!(rank(RunSummary { bell: true, ..loss }).title, "BELL RINGER");
    }
}
