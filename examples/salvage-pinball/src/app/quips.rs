//! The yard foreman's dot-matrix personality and the results-card ranks.
//!
//! Every selector is a pure function of accepted simulation counters, so a
//! run always reads the same way and no clock, RNG or render cadence is
//! involved. Consecutive counters never select the same line twice in a row.

const DRAIN: [&str; 8] = [
    "DRAIN! THE FOREMAN SIGHS",
    "THAT BALL WAS SCRAP ANYWAY",
    "DOWN THE MIDDLE. A CLASSIC.",
    "THE DRAIN SAYS THANKS",
    "GRAVITY CLOCKED IN EARLY",
    "LOST TO THE COMPACTOR",
    "WE'LL CALL THAT RECYCLING",
    "SHAKE IT OFF, ROOKIE",
];

const LAST_BALL: [&str; 4] = [
    "LAST BALL. NO PRESSURE.",
    "FINAL BALL. MAKE IT CLANK.",
    "ONE LEFT. THE YARD WATCHES.",
    "LAST BALL. BRING IT HOME.",
];

const SAVE: [&str; 5] = [
    "ON THE HOUSE. THIS TIME.",
    "SAFETY NET! TRY AGAIN",
    "RETURNED TO SENDER",
    "FREEBIE. DON'T TELL ACCOUNTING",
    "HARD HATS ON. GO AGAIN",
];

const JACKPOT: [&str; 5] = [
    "GOT ONE!",
    "SCRAP OF THE DAY!",
    "THE MAGNET LOVES YOU",
    "HEAVY LIFTING!",
    "CRANE OPERATOR OF THE MONTH",
];

const MASH: [&str; 5] = [
    "THE FLIPPERS ARE UNION",
    "THOSE ARE NOT DRUMS",
    "I SAW THAT. WE ALL SAW THAT.",
    "GENTLE. THEY'RE VINTAGE.",
    "FLIPPER ABUSE NOTED IN FILE",
];

const PAUSE: [&str; 5] = [
    "TEA BREAK. THE BALL WILL WAIT",
    "HARD HAT OFF. FIVE MINUTES",
    "THE CRANE IS ALSO STRETCHING",
    "THE BREAK ROOM HAS DONUTS",
    "CLOCKED OUT. TABLE FROZEN",
];

const READY: [&str; 4] = [
    "SHIFT STARTS. BREAK SOMETHING.",
    "HARD HATS ON, BALL LOADED",
    "3 BALLS. 1 CRANE. NO REFUNDS.",
    "THE YARD IS OPEN FOR BUSINESS",
];

/// Advance by a stride coprime with the table length, so consecutive
/// counters never repeat a line and every line is eventually visited.
pub(super) fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let stride = if table.len().is_multiple_of(5) { 3 } else { 5 };
    table[(counter as usize).wrapping_mul(stride) % table.len()]
}

/// `drains` counts unsaved drains this game, including this one.
pub(super) fn drain(drains: u32, lives_left: u8) -> &'static str {
    if lives_left == 1 {
        rotate(&LAST_BALL, drains)
    } else {
        rotate(&DRAIN, drains)
    }
}

pub(super) fn save(saves: u32) -> &'static str {
    rotate(&SAVE, saves)
}

pub(super) fn jackpot(jackpots: u32) -> &'static str {
    rotate(&JACKPOT, jackpots.saturating_sub(1))
}

/// Every authored line, for layout tests at the narrowest canvas.
#[cfg(test)]
pub(super) fn all_lines() -> impl Iterator<Item = &'static str> {
    DRAIN
        .iter()
        .chain(&LAST_BALL)
        .chain(&SAVE)
        .chain(&JACKPOT)
        .chain(&MASH)
        .chain(&PAUSE)
        .chain(&READY)
        .chain(RANKS.iter().map(|(_, _, quip)| quip))
        .copied()
}

pub(super) fn mash(count: u32) -> &'static str {
    rotate(&MASH, count)
}

pub(super) fn pause(seed: u32) -> &'static str {
    rotate(&PAUSE, seed)
}

pub(super) fn ready(seed: u32) -> &'static str {
    rotate(&READY, seed)
}

/// Escalating chain callouts. Two hits are routine; three or more shout.
pub(super) fn chain(chain: u32) -> Option<&'static str> {
    Some(match chain {
        0..=2 => return None,
        3 => "CLANK CLANK CLANK!",
        4 => "SCRAP STORM!",
        5 => "HEAVY METAL!",
        6 => "MELTDOWN!",
        7 => "THE YARD IS SHAKING!",
        _ => "ABSOLUTE CARNAGE!",
    })
}

/// Score floors for each rank, lowest first, with the title and a quip.
/// Thresholds are human-scale; the autoplay bot is superhuman.
const RANKS: [(u32, &str, &str); 7] = [
    (0, "GUTTER INSPECTOR", "THE DRAIN SENDS ITS THANKS"),
    (1, "SCRAP INTERN", "EVERYONE STARTS SOMEWHERE"),
    (1_500, "BOLT SORTER", "YOU FOUND MOST OF THE BOLTS"),
    (4_000, "FORKLIFT HOPEFUL", "THEY ALMOST LET YOU DRIVE"),
    (10_000, "MAGNET WRANGLER", "THE CRANE RESPECTS YOU. ISH."),
    (25_000, "YARD FOREMAN", "HERE'S YOUR CLIPBOARD, BOSS"),
    (60_000, "SALVAGE BARON", "THEY'LL NAME A HEAP AFTER YOU"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
    /// The next rank's title and score floor, if any remains.
    pub next: Option<(&'static str, u32)>,
}

/// Rank a finished game. Finding the overtime secret on a modest score earns
/// its own title, so a long, patient ball is celebrated too.
pub(super) fn rank(score: u32, secret: bool) -> Rank {
    let index = RANKS
        .iter()
        .rposition(|(floor, ..)| score >= *floor)
        .unwrap_or(0);
    let (_, title, quip) = RANKS[index];
    let next = RANKS
        .get(index + 1)
        .map(|(floor, title, _)| (*title, *floor));
    if secret && score < 25_000 {
        return Rank {
            title: "OVERTIME LEGEND",
            quip: "PAYROLL IS FURIOUS. WORTH IT.",
            next,
        };
    }
    Rank { title, quip, next }
}

/// Group thousands with commas: `12400` → `12,400`.
pub(super) fn grouped(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub(super) fn grouped_into(out: &mut String, value: u32) {
    use std::fmt::Write;
    if value >= 1000 {
        grouped_into(out, value / 1000);
        let _ = write!(out, ",{value:03}", value = value % 1000);
    } else {
        let _ = write!(out, "{value}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn consecutive_selections_never_repeat_and_every_line_is_reachable() {
        for table in [
            &DRAIN[..],
            &LAST_BALL[..],
            &SAVE[..],
            &JACKPOT[..],
            &MASH[..],
            &PAUSE[..],
            &READY[..],
        ] {
            let mut previous = "";
            let mut seen = BTreeSet::new();
            for n in 0..64 {
                let line = rotate(table, n);
                assert_ne!(line, previous, "counter {n} repeated {line}");
                previous = line;
                seen.insert(line);
            }
            assert_eq!(seen.len(), table.len());
        }
        let mut previous = "";
        for (drains, lives) in [(1, 2), (2, 1), (1, 2), (2, 2), (3, 2)] {
            let line = drain(drains, lives);
            assert_ne!(line, previous);
            previous = line;
        }
        assert!(LAST_BALL.contains(&drain(7, 1)));
    }

    #[test]
    fn ranks_climb_with_score_and_always_offer_a_next_goal_until_the_top() {
        assert_eq!(rank(0, false).title, "GUTTER INSPECTOR");
        assert_eq!(rank(100, false).title, "SCRAP INTERN");
        assert_eq!(rank(100, false).next, Some(("BOLT SORTER", 1_500)));
        assert_eq!(rank(9_999, false).title, "FORKLIFT HOPEFUL");
        assert_eq!(rank(10_000, false).title, "MAGNET WRANGLER");
        assert_eq!(rank(1_000_000, false).title, "SALVAGE BARON");
        assert_eq!(rank(1_000_000, false).next, None);
        let secret = rank(3_000, true);
        assert_eq!(secret.title, "OVERTIME LEGEND");
        assert_eq!(secret.next, Some(("FORKLIFT HOPEFUL", 4_000)));
        assert_eq!(rank(30_000, true).title, "YARD FOREMAN");
        for (floor, title, quip) in RANKS {
            assert!(!title.is_empty() && !quip.is_empty());
            assert_eq!(rank(floor, false).title, title);
        }
    }

    #[test]
    fn chain_callouts_escalate_from_three_and_numbers_group() {
        assert_eq!(chain(2), None);
        assert_eq!(chain(3), Some("CLANK CLANK CLANK!"));
        assert_ne!(chain(4), chain(5));
        assert_eq!(chain(99), Some("ABSOLUTE CARNAGE!"));
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(1_522_050), "1,522,050");
        assert_eq!(grouped(u32::MAX), "4,294,967,295");
    }
}
