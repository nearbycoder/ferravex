//! Foreman Gus, his opinions, and the silly titles on the results card.
//!
//! Selectors are pure functions of counters, so the same run reads the same
//! way. Each category advances by a stride coprime with its table length, so
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
    "SIX TARGETS. NO PRESSURE.",
    "I JUST STACKED THOSE.",
    "MIND THE PAINTWORK, CHIEF.",
    "LOAD-BEARING TOWERS. ALLEGEDLY.",
    "SAFETY FIRST. DEMOLITION SECOND.",
];

const RETRY: [&str; 6] = [
    "REBUILT IT. WITH EXTRA GLUE.",
    "BACK AGAIN? BRING IT.",
    "SAME TOWERS. FRESH CONFIDENCE.",
    "I'VE PUT THE KETTLE ON.",
    "ROUND TWO. MY BACK HURTS.",
    "GENTLY THIS TIME. OR NOT.",
];

const WOBBLE: [&str; 5] = [
    "WOBBLE... WOBBLE...",
    "EASY... EASY...",
    "IT'S FINE. IT'S FINE.",
    "DON'T YOU DARE FALL.",
    "STEADY, OLD GIRL...",
];

const KNOCKED: [&str; 7] = [
    "MY BEAUTIFUL TOWER!",
    "THAT WAS LOAD-BEARING!",
    "I'LL GET THE BROOM.",
    "THERE GOES MY WEEKEND.",
    "WHO SIGNED OFF ON THAT?",
    "INSURANCE WON'T COVER THIS.",
    "I HAD A NAME FOR THAT ONE.",
];

const MISS: [&str; 7] = [
    "THE BIRDS THANK YOU.",
    "WIND, PROBABLY.",
    "WARNING SHOT? SURE.",
    "THE SKY IS NOT A TARGET.",
    "BOLD. WRONG, BUT BOLD.",
    "THE CLOUDS FELT THAT.",
    "I'LL PUT IT IN THE REPORT.",
];

const MISS_STREAK: [&str; 4] = [
    "TRY THE LOW SANDY BLOCKS.",
    "FOLLOW THE DOTTED LINE!",
    "AIM LOWER. I'M BEGGING.",
    "SUPPORTS, CHIEF. SUPPORTS.",
];

const DOMINO: [&str; 3] = [
    "THEY'RE HOLDING HANDS!",
    "DOMINO RULES APPLY!",
    "ONE PUSH. TWO TOWERS. RUDE.",
];

const WONDER: [&str; 2] = ["ONE BALL?! ONE?!", "I'M CALLING MY MUM."];

const BELL: [&str; 3] = [
    "WHO RANG THE LUNCH BELL?",
    "DING! ...LUNCH ALREADY?",
    "NOBODY TOUCHES THE BELL!",
];

const LAST_SHELL: [&str; 3] = [
    "LAST SHELL. MAKE IT COUNT.",
    "ONE LEFT. NO PRESSURE.",
    "LAST ONE. I BELIEVE IN YOU.",
];

const SETTLING: [&str; 3] = [
    "COME ON... COME ON...",
    "ANY SECOND NOW...",
    "LEAN, YOU STUBBORN THING!",
];

const PAUSE: [&str; 5] = [
    "UNION-MANDATED TEA BREAK",
    "THE RUBBLE WILL WAIT",
    "GUS IS ON HIS SANDWICH",
    "TOWERS HOLDING THEIR BREATH",
    "HARD HATS OFF. BRIEFLY.",
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
        ("WRECKING BALL WIZARD", "PHYSICS HAS FILED A COMPLAINT")
    } else if run.won && run.stars == 3 && run.bell {
        ("FOUNDRY LEGEND", "THREE STARS AND A DING. SHOW-OFF.")
    } else if run.bell && !run.won {
        ("BELL RINGER", "WRONG TARGET. GREAT TONE.")
    } else if run.won && run.stars == 3 {
        ("MASTER DEMOLISHER", "SURGICAL. SLIGHTLY TERRIFYING.")
    } else if run.won && run.domino {
        ("DOMINO DIRECTOR", "WHY HIT TWO WHEN ONE WILL DO?")
    } else if run.won && run.stars == 2 {
        ("CERTIFIED TOPPLER", "THE TOWERS TOOK EARLY RETIREMENT")
    } else if run.won {
        ("PERSISTENT PLINKER", "THEY FELL EVENTUALLY. OUT OF PITY?")
    } else if run.misses >= 5 {
        ("SKY ARTILLERIST", "THE CLOUDS NEVER SAW IT COMING")
    } else if run.targets >= 4 {
        ("SO CLOSE, SO RUBBLE", "ONE MORE SHELL. JUST ONE.")
    } else if run.targets >= 1 {
        ("APPRENTICE LOBBER", "THE TOWERS ARE BEING SMUG ABOUT IT")
    } else {
        (
            "CONSCIENTIOUS OBJECTOR",
            "NOT ONE BLOCK HARMED. VERY POLITE.",
        )
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
        assert_eq!(rank(loss).title, "CONSCIENTIOUS OBJECTOR");
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
