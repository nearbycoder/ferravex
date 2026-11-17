//! Table-driven courier personality. Selectors are pure functions of accepted
//! counters, so a replayed flight reads the same and nothing depends on a clock
//! or RNG. Consecutive counters never select the same line twice in a row.
//!
//! In-flight callouts are punchy headlines of at most three words; the longer
//! jokes live on the pause card and the shift card, one line each.
use super::courier::{DropGrade, Impact, Ledger, Line};

/// Advance by a stride coprime with the table length: consecutive counters
/// never repeat a line and every line is eventually visited.
pub(super) fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let len = table.len();
    let stride = [5, 3, 7, 11]
        .into_iter()
        .find(|s| !len.is_multiple_of(*s) && len > 1)
        .unwrap_or(1);
    table[(counter as usize).wrapping_mul(stride) % len]
}

const WATER: [&str; 6] = [
    "Splashdown!",
    "Fish signed it",
    "Boat mode!",
    "Wet mail!",
    "Sea says hi",
    "Gulls 1, you 0",
];
const ISLAND: [&str; 5] = [
    "Island: unmoved",
    "Tree: unimpressed",
    "Rock: still there",
    "Landscaping: extra",
    "Locals heard that",
];
const BOUNDARY: [&str; 4] = [
    "Off the map!",
    "ATC is cross",
    "Sky has limits",
    "Wrong way!",
];

/// `impacts` counts this run's impacts, including this one.
pub(super) fn impact(cause: Impact, impacts: u32) -> &'static str {
    let table: &[&str] = match cause {
        Impact::Water => &WATER,
        Impact::Island => &ISLAND,
        Impact::Boundary => &BOUNDARY,
    };
    rotate(table, impacts.saturating_sub(1))
}

const BULLSEYE: [&str; 5] = [
    "Bullseye!",
    "Threaded it!",
    "Dead centre!",
    "Surgical!",
    "Needle, meet thread",
];
const CLEAN: [&str; 4] = ["Nice line", "Smooth pass", "Tidy!", "Right through"];
const SCRAPE: [&str; 4] = [
    "Close shave!",
    "By a whisker",
    "Paint: scratched",
    "Ring: grazed",
];

/// Callout for an accepted gate; `gate` is the zero-based ring index.
pub(super) fn gate(line: Line, gate: usize) -> &'static str {
    let table: &[&str] = match line {
        Line::Bullseye => &BULLSEYE,
        Line::Clean => &CLEAN,
        Line::Scrape => &SCRAPE,
    };
    rotate(table, gate as u32)
}

/// A streak milestone, shown once when the multiplier reaches it.
pub(super) fn streak(multiplier: u32) -> Option<&'static str> {
    match multiplier {
        3 => Some("On a roll!"),
        4 => Some("Corner office!"),
        5 => Some("Unstoppable!"),
        _ => None,
    }
}

const BONUS: [&str; 3] = ["Secret ring!", "Off-route, on-trend", "Scenic detour!"];
/// Headline when the third hidden ring completes the scenic route.
pub(super) const SCENIC: &str = "Scenic route!";

pub(super) fn bonus(found: u32) -> &'static str {
    rotate(&BONUS, found.saturating_sub(1))
}

const PAUSE: [&str; 5] = [
    "The seagulls will wait",
    "Propeller on standby",
    "Mail nap in progress",
    "The wind holds its breath",
    "Pilot snack break",
];

pub(super) fn pause(count: u32) -> &'static str {
    rotate(&PAUSE, count)
}

const BULLSEYE_DROP: [&str; 4] = [
    "Not a scratch. I'm weeping.",
    "It landed in my hands!",
    "Five stars. Six if allowed.",
    "Was that a helicopter?",
];
const SMOOTH_DROP: [&str; 4] = [
    "Arrived slightly airborne.",
    "Only bounced once. Nice.",
    "Gentle-ish. I'll allow it.",
    "My hat flew off. Worth it.",
];
const BUMPY_DROP: [&str; 4] = [
    "It's in a tree. Thanks?",
    "My cat caught it. Good cat.",
    "Delivered. Technically.",
    "The box is now a pancake.",
];
const SUNK: [&str; 3] = [
    "The fish enjoyed their mail.",
    "The parcel joined the reef.",
    "Insurance? Eventually.",
];
const LATE: [&str; 3] = [
    "The customer went to bed.",
    "The dog ate the deadline.",
    "Fashionably late. Very late.",
];

/// The customer's reaction on the shift card. `shift` rotates between runs.
pub(super) fn customer(ledger: &Ledger, delivered: bool, hull: u8, shift: u32) -> &'static str {
    let table: &[&str] = match (delivered, ledger.drop.map(|d| d.grade)) {
        (true, Some(DropGrade::Bullseye)) => &BULLSEYE_DROP,
        (true, Some(DropGrade::Smooth)) => &SMOOTH_DROP,
        (true, _) => &BUMPY_DROP,
        (false, _) if hull == 0 => &SUNK,
        (false, _) => &LATE,
    };
    rotate(table, shift)
}

/// Ascending delivered-tip thresholds and their silly titles.
pub(super) const RANKS: [(u32, &str); 5] = [
    (0, "Pigeon in Training"),
    (300, "Parcel Pilot"),
    (500, "Senior Sky Courier"),
    (700, "Air Mail Ace"),
    (900, "Legend of the Letterbox"),
];

/// Title for a finished shift, plus the next rank and its tip goal. An
/// undelivered shift aims at the first real rank.
pub(super) fn rank(
    ledger: &Ledger,
    delivered: bool,
    hull: u8,
    gates: usize,
) -> (&'static str, Option<(u32, &'static str)>) {
    if !delivered {
        let title = if hull == 0 && gates == 0 {
            "Takeoff Enthusiast"
        } else if hull == 0 {
            "Splashdown Specialist"
        } else {
            "Fashionably Late"
        };
        return (title, Some(RANKS[1]));
    }
    let tier = RANKS
        .iter()
        .rposition(|(tips, _)| ledger.tips >= *tips)
        .unwrap_or(0);
    let title = if ledger.scenic() && tier < RANKS.len() - 1 {
        "Scenic Route Specialist"
    } else {
        RANKS[tier].1
    };
    (title, RANKS.get(tier + 1).copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::courier::Drop;

    #[test]
    fn rotations_never_repeat_back_to_back_and_visit_every_line() {
        for table in [
            &WATER[..],
            &ISLAND,
            &BOUNDARY,
            &BULLSEYE,
            &CLEAN,
            &SCRAPE,
            &BONUS,
            &PAUSE,
            &BULLSEYE_DROP,
            &SMOOTH_DROP,
            &BUMPY_DROP,
            &SUNK,
            &LATE,
        ] {
            let mut seen = std::collections::BTreeSet::new();
            for n in 0..64 {
                assert_ne!(rotate(table, n), rotate(table, n + 1));
                seen.insert(rotate(table, n));
            }
            assert_eq!(seen.len(), table.len());
        }
    }

    /// Every in-flight headline: impacts, gate lines, streaks and secrets.
    fn callouts() -> impl Iterator<Item = &'static str> {
        WATER
            .iter()
            .chain(&ISLAND)
            .chain(&BOUNDARY)
            .chain(&BULLSEYE)
            .chain(&CLEAN)
            .chain(&SCRAPE)
            .chain(&BONUS)
            .copied()
            .chain([SCENIC])
            .chain((3..=5).filter_map(streak))
    }

    #[test]
    fn flight_callouts_are_punchy_headlines() {
        for line in callouts() {
            let words = line
                .split_whitespace()
                .filter(|word| word.chars().any(char::is_alphabetic))
                .count();
            assert!(words <= 3, "{line}: {words} words");
        }
    }

    #[test]
    fn callouts_and_card_lines_fit_the_narrowest_card_on_one_line() {
        use ferravex_render::text::{TextStyle, text_width};
        // In flight, headlines are 24px modern copy in a card no wider than
        // the 304px HUD at 320px, with 20px padding each side.
        let title = TextStyle::modern(24., ferravex_render::WHITE);
        for line in callouts() {
            let width = text_width(line, title);
            assert!(width <= 264., "{line}: {width}");
        }
        // Card rows are 280 logical pixels of 16px modern copy; measure with
        // the face that actually draws them. Jokes get exactly one line.
        let body = TextStyle::modern(16., ferravex_render::WHITE);
        let width = |line: &str| text_width(line, body);
        for line in PAUSE
            .iter()
            .chain(RANKS.iter().map(|(_, title)| title))
            .chain(&[
                "Scenic Route Specialist",
                "Splashdown Specialist",
                "Takeoff Enthusiast",
                "Fashionably Late",
            ])
        {
            assert!(width(line) <= 280., "{line}: {}", width(line));
        }
        for line in BULLSEYE_DROP
            .iter()
            .chain(&SMOOTH_DROP)
            .chain(&BUMPY_DROP)
            .chain(&SUNK)
            .chain(&LATE)
        {
            let quoted = format!("“{line}”");
            assert!(width(&quoted) <= 280., "{line}: {}", width(&quoted));
        }
    }

    #[test]
    fn ranks_climb_with_tips_and_secrets_and_never_mock_a_loss() {
        let mut ledger = Ledger::default();
        assert_eq!(rank(&ledger, true, 3, 5).0, "Pigeon in Training");
        assert_eq!(rank(&ledger, true, 3, 5).1, Some(RANKS[1]));
        ledger.tips = 720;
        assert_eq!(rank(&ledger, true, 3, 5).0, "Air Mail Ace");
        ledger.bonus = 0b111;
        assert_eq!(rank(&ledger, true, 3, 5).0, "Scenic Route Specialist");
        ledger.tips = 950;
        assert_eq!(rank(&ledger, true, 3, 5), ("Legend of the Letterbox", None));
        assert_eq!(rank(&ledger, false, 0, 0).0, "Takeoff Enthusiast");
        assert_eq!(rank(&ledger, false, 0, 0).1, Some(RANKS[1]));
        assert_eq!(rank(&ledger, false, 0, 3).0, "Splashdown Specialist");
        assert_eq!(rank(&ledger, false, 2, 3).0, "Fashionably Late");
        ledger.drop = Some(Drop {
            grade: DropGrade::Bullseye,
            multiplier: 1,
            tip: 60,
            early: 0,
        });
        assert!(BULLSEYE_DROP.contains(&customer(&ledger, true, 3, 7)));
        assert!(SUNK.contains(&customer(&ledger, false, 0, 7)));
        assert!(LATE.contains(&customer(&ledger, false, 1, 7)));
        assert_ne!(customer(&ledger, true, 3, 1), customer(&ledger, true, 3, 2));
    }
}
