//! Table-driven courier personality. Selectors are pure functions of accepted
//! counters, so a replayed flight reads the same and nothing depends on a clock
//! or RNG. Consecutive counters never select the same line twice in a row.
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
    "The fish signed for it",
    "You're a boat now",
    "Wet mail is still mail",
    "The sea says hello",
    "Seagulls 1, pilot 0",
];
const ISLAND: [&str; 5] = [
    "The island didn't move",
    "Tree: unimpressed",
    "Rock: still there",
    "Landscaping is extra",
    "Locals report a thud",
];
const BOUNDARY: [&str; 4] = [
    "Off the map, off course",
    "Air traffic is cross",
    "The sky has a limit",
    "That's not the route",
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
        3 => Some("On a roll"),
        4 => Some("The sky is your office"),
        5 => Some("Max tips! Unstoppable"),
        _ => None,
    }
}

const BONUS: [&str; 3] = [
    "Secret ring!",
    "Off-route and on-trend",
    "Scenic detour pays",
];

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
    "Package arrived. Slightly airborne.",
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
    "Insurance is on its way. Slowly.",
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

/// Title for a finished shift, plus the next tip goal on the ladder.
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
        return (title, Some((RANKS[1].0, "Deliver for a real rank")));
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

    #[test]
    fn callouts_fit_a_narrow_toast_and_card_without_wrapping() {
        use ferravex_render::text::{TextStyle, text_width, wrap_text};
        // The narrowest toast and card rows are 280 logical pixels of 16px
        // modern copy; measure with the face that actually draws them.
        let body = TextStyle::modern(16., ferravex_render::WHITE);
        let width = |line: &str| text_width(line, body);
        for line in WATER
            .iter()
            .chain(&ISLAND)
            .chain(&BOUNDARY)
            .chain(&BULLSEYE)
            .chain(&CLEAN)
            .chain(&SCRAPE)
            .chain(&BONUS)
            .chain(&PAUSE)
            .chain(RANKS.iter().map(|(_, title)| title))
            .chain(&[
                "Scenic Route Specialist",
                "Splashdown Specialist",
                "The sky is your office",
                "Max tips! Unstoppable",
            ])
        {
            assert!(width(line) <= 280., "{line}: {}", width(line));
        }
        // Customer reactions may use two card lines (quoted).
        for line in BULLSEYE_DROP
            .iter()
            .chain(&SMOOTH_DROP)
            .chain(&BUMPY_DROP)
            .chain(&SUNK)
            .chain(&LATE)
        {
            let quoted = format!("“{line}”");
            assert!(wrap_text(&quoted, 280., body).count() <= 2, "{line}");
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
