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
    "SPLASHDOWN!",
    "THE FISH SIGNED FOR IT",
    "YOU'RE A BOAT NOW",
    "WET MAIL IS STILL MAIL",
    "THE SEA SAYS HELLO",
    "SEAGULLS 1, PILOT 0",
];
const ISLAND: [&str; 5] = [
    "THE ISLAND DIDN'T MOVE",
    "TREE: UNIMPRESSED",
    "ROCK: STILL THERE",
    "LANDSCAPING IS EXTRA",
    "LOCALS REPORT A THUD",
];
const BOUNDARY: [&str; 4] = [
    "OFF THE MAP, OFF COURSE",
    "AIR TRAFFIC IS CROSS",
    "THE SKY HAS A LIMIT",
    "THAT'S NOT THE ROUTE",
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
    "BULLSEYE!",
    "THREADED IT!",
    "DEAD CENTRE!",
    "SURGICAL!",
    "NEEDLE, MEET THREAD",
];
const CLEAN: [&str; 4] = ["NICE LINE", "SMOOTH PASS", "TIDY!", "RIGHT THROUGH"];
const SCRAPE: [&str; 4] = [
    "CLOSE SHAVE!",
    "BY A WHISKER",
    "PAINT: SCRATCHED",
    "RING: GRAZED",
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
        3 => Some("ON A ROLL"),
        4 => Some("THE SKY IS YOUR OFFICE"),
        5 => Some("MAX TIPS! UNSTOPPABLE"),
        _ => None,
    }
}

const BONUS: [&str; 3] = [
    "SECRET RING!",
    "OFF-ROUTE AND ON-TREND",
    "SCENIC DETOUR PAYS",
];

pub(super) fn bonus(found: u32) -> &'static str {
    rotate(&BONUS, found.saturating_sub(1))
}

const PAUSE: [&str; 5] = [
    "THE SEAGULLS WILL WAIT",
    "PROPELLER ON STANDBY",
    "MAIL NAP IN PROGRESS",
    "THE WIND HOLDS ITS BREATH",
    "PILOT SNACK BREAK",
];

pub(super) fn pause(count: u32) -> &'static str {
    rotate(&PAUSE, count)
}

const BULLSEYE_DROP: [&str; 4] = [
    "NOT A SCRATCH. I'M WEEPING.",
    "IT LANDED IN MY HANDS!",
    "FIVE STARS. SIX IF ALLOWED.",
    "WAS THAT A HELICOPTER?",
];
const SMOOTH_DROP: [&str; 4] = [
    "PACKAGE ARRIVED. SLIGHTLY AIRBORNE.",
    "ONLY BOUNCED ONCE. NICE.",
    "GENTLE-ISH. I'LL ALLOW IT.",
    "MY HAT FLEW OFF. WORTH IT.",
];
const BUMPY_DROP: [&str; 4] = [
    "IT'S IN A TREE. THANKS?",
    "MY CAT CAUGHT IT. GOOD CAT.",
    "DELIVERED. TECHNICALLY.",
    "THE BOX IS NOW A PANCAKE.",
];
const SUNK: [&str; 3] = [
    "THE FISH ENJOYED THEIR MAIL.",
    "THE PARCEL JOINED THE REEF.",
    "INSURANCE IS ON ITS WAY. SLOWLY.",
];
const LATE: [&str; 3] = [
    "THE CUSTOMER WENT TO BED.",
    "THE DOG ATE THE DEADLINE.",
    "FASHIONABLY LATE. VERY LATE.",
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
    (0, "PIGEON IN TRAINING"),
    (300, "PARCEL PILOT"),
    (500, "SENIOR SKY COURIER"),
    (700, "AIR MAIL ACE"),
    (900, "LEGEND OF THE LETTERBOX"),
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
            "TAKEOFF ENTHUSIAST"
        } else if hull == 0 {
            "SPLASHDOWN SPECIALIST"
        } else {
            "FASHIONABLY LATE"
        };
        return (title, Some((RANKS[1].0, "DELIVER FOR A REAL RANK")));
    }
    let tier = RANKS
        .iter()
        .rposition(|(tips, _)| ledger.tips >= *tips)
        .unwrap_or(0);
    let title = if ledger.scenic() && tier < RANKS.len() - 1 {
        "SCENIC ROUTE SPECIALIST"
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
        // 280 logical pixels: 23 cells at 16px, 26 at 14px.
        for line in WATER
            .iter()
            .chain(&ISLAND)
            .chain(&BOUNDARY)
            .chain(&BULLSEYE)
            .chain(&CLEAN)
            .chain(&SCRAPE)
            .chain(&BONUS)
            .chain(RANKS.iter().map(|(_, title)| title))
            .chain(&["SCENIC ROUTE SPECIALIST", "SPLASHDOWN SPECIALIST"])
        {
            assert!(line.chars().count() <= 23, "{line}");
        }
        for line in PAUSE.iter().chain(&["THE SKY IS YOUR OFFICE"]) {
            assert!(line.chars().count() <= 26, "{line}");
        }
        // Customer reactions may use two card lines.
        for line in BULLSEYE_DROP
            .iter()
            .chain(&SMOOTH_DROP)
            .chain(&BUMPY_DROP)
            .chain(&SUNK)
            .chain(&LATE)
        {
            assert!(line.chars().count() <= 48, "{line}");
        }
    }

    #[test]
    fn ranks_climb_with_tips_and_secrets_and_never_mock_a_loss() {
        let mut ledger = Ledger::default();
        assert_eq!(rank(&ledger, true, 3, 5).0, "PIGEON IN TRAINING");
        assert_eq!(rank(&ledger, true, 3, 5).1, Some(RANKS[1]));
        ledger.tips = 720;
        assert_eq!(rank(&ledger, true, 3, 5).0, "AIR MAIL ACE");
        ledger.bonus = 0b111;
        assert_eq!(rank(&ledger, true, 3, 5).0, "SCENIC ROUTE SPECIALIST");
        ledger.tips = 950;
        assert_eq!(rank(&ledger, true, 3, 5), ("LEGEND OF THE LETTERBOX", None));
        assert_eq!(rank(&ledger, false, 0, 0).0, "TAKEOFF ENTHUSIAST");
        assert_eq!(rank(&ledger, false, 0, 3).0, "SPLASHDOWN SPECIALIST");
        assert_eq!(rank(&ledger, false, 2, 3).0, "FASHIONABLY LATE");
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
