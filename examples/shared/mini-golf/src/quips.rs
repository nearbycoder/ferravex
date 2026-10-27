//! Deterministic, table-driven personality for Copper Greens.
//!
//! Every selector is a pure function of accepted rules counters, so the same
//! round always reads the same way and no clock, RNG or render cadence is
//! involved. Consecutive counters never select the same line twice in a row.
use crate::card::{HoleOutcome, ScoreName, Style, Tone};

const ACE: [&str; 6] = [
    "FRAME THAT BALL. MOUNT IT.",
    "THE CUP NEVER STOOD A CHANCE",
    "ONE AND DONE. SHOW-OFF.",
    "SOMEONE CALL THE NEWSPAPER",
    "THE WINDMILL IS JEALOUS",
    "THAT WAS PURE COPPER",
];

const UNDER: [&str; 7] = [
    "SMOOTH AS A FRESH-MOWN GREEN",
    "THE FLAG SALUTES YOU",
    "TEXTBOOK. IF THE TEXTBOOK WAS COOL.",
    "CRISP. CLEAN. COPPER.",
    "THE CUP SAW YOU COMING",
    "BIRDS EVERYWHERE ARE NODDING",
    "YOU MADE THAT LOOK EASY",
];

const EVEN: [&str; 7] = [
    "RIGHT ON THE NUMBER",
    "THE COURSE NODS POLITELY",
    "NO DRAMA. JUST GOLF.",
    "PAR: THE SENSIBLE SHOES OF SCORES",
    "STEADY HANDS, STEADY CARD",
    "EXACTLY AS PLANNED. PROBABLY.",
    "SOLID. LIKE A STONE WALL.",
];

const OVER: [&str; 7] = [
    "THE CUP PLAYED HARD TO GET",
    "A SCENIC ROUTE, BUT A ROUTE",
    "SHAKE IT OFF. NEXT GREEN.",
    "THE BALL HAD OTHER PLANS",
    "CHARACTER BUILDING, THAT ONE",
    "THE PUTTER WILL BE SPOKEN TO",
    "NOBODY SAW THAT. PROBABLY.",
];

const ROUGH: [&str; 7] = [
    "AND COUNTING. AND COUNTING.",
    "THE SCORECARD NEEDS A BIGGER BOX",
    "YOU REALLY GOT YOUR MONEY'S WORTH",
    "THE BALL TOOK THE GRAND TOUR",
    "EVERY STROKE A STORY",
    "WE ADMIRE YOUR PERSISTENCE",
    "THIS GREEN OWES YOU A SNACK",
];

const PICKED_UP: [&str; 5] = [
    "THE GREEN WINS THIS ROUND",
    "TACTICAL RETREAT. VERY WISE.",
    "THE BALL NEEDED A BREAK ANYWAY",
    "WE'LL CALL IT A PRACTICE SWING",
    "POCKETED. WITH DIGNITY.",
];

const WATER: [&str; 8] = [
    "THE DUCKS SAY THANK YOU",
    "YOUR BALL WENT FOR A SWIM",
    "GLUB GLUB",
    "THAT'S A SUBMARINE NOW",
    "WATER HAZARD 1, YOU 0",
    "SPLASHDOWN CONFIRMED",
    "FISH GOT A NEW TOY",
    "IT WAS THIRSTY, PROBABLY",
];

const LIP_OUT: [&str; 6] = [
    "THE CUP SAID NO",
    "LIPPED OUT!",
    "TOO HOT TO DROP!",
    "IT PEEKED IN AND LEFT",
    "RIM SHOT!",
    "SO CLOSE IT HURTS",
];

const NEAR: [&str; 6] = [
    "INCHES!",
    "TAP-IN RANGE",
    "GIMME? NOT HERE.",
    "KNOCKING ON THE CUP",
    "ALMOST HAD IT",
    "ONE MORE ROLL...",
];

const BANK: [&str; 5] = [
    "BANK SHOT!",
    "OFF THE RAIL!",
    "GEOMETRY CLASS PAID OFF",
    "CALLED IT. SURELY.",
    "ANGLES!",
];

const TRICK: [&str; 4] = [
    "TRICK SHOT!",
    "PINBALL WIZARD!",
    "THAT WAS ON PURPOSE. RIGHT?",
    "RICOCHET ROYALTY!",
];

const PAUSE: [&str; 8] = [
    "THE WINDMILL IS ON BREAK TOO",
    "THE BALL WILL WAIT. IT'S A BALL.",
    "STRETCH THOSE PUTTING WRISTS",
    "THE DUCKS ARE GOSSIPING",
    "THE CUP ISN'T GOING ANYWHERE",
    "GREENSKEEPER BREAK",
    "RUMOR: THE COPPER LIKES TO HELP",
    "RUMOR: HOLE 1 CAN BE ACED BACKWARDS",
];

const RETURN: [&str; 4] = [
    "FRESH START (+1)",
    "WE NEVER SPEAK OF THAT SHOT",
    "MULLIGAN-ISH (+1)",
    "TAKE TWO (+1)",
];

const STREAK: [&str; 4] = ["ON A ROLL", "HEATING UP", "CAN'T MISS", "COPPER FEVER"];

const SAND: [&str; 4] = ["BEACH DAY", "SANDY!", "SAND TRAP SAYS HI", "THAT'S GRITTY"];

/// Advances by a stride coprime with the table length, so the same table
/// never repeats on consecutive counters and every line is visited.
pub fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let len = table.len();
    let stride = [5, 3, 7, 11]
        .into_iter()
        .find(|s| gcd(*s, len) == 1)
        .unwrap_or(1);
    table[(counter as usize).wrapping_mul(stride) % len]
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// The supporting line under a finished hole's headline.
pub fn hole(outcome: &HoleOutcome) -> &'static str {
    if outcome.name == ScoreName::PickedUp {
        return rotate(&PICKED_UP, outcome.count);
    }
    let table: &[&str] = match outcome.name.tone() {
        Tone::Ace => &ACE,
        Tone::Under => &UNDER,
        Tone::Even => &EVEN,
        Tone::Over => &OVER,
        Tone::Rough => &ROUGH,
    };
    rotate(table, outcome.count)
}

/// Callout for a holed putt that banked, if any.
pub fn style(style: Style, count: u32) -> Option<&'static str> {
    match style {
        Style::Straight => None,
        Style::Bank(_) => Some(rotate(&BANK, count)),
        Style::Trick(_) => Some(rotate(&TRICK, count)),
    }
}

pub fn water(splashes: u32) -> &'static str {
    rotate(&WATER, splashes)
}
pub fn lip_out(count: u32) -> &'static str {
    rotate(&LIP_OUT, count)
}
pub fn near(count: u32) -> &'static str {
    rotate(&NEAR, count)
}
pub fn pause(seed: u32) -> &'static str {
    rotate(&PAUSE, seed)
}
pub fn returned(count: u32) -> &'static str {
    rotate(&RETURN, count)
}
pub fn sand(count: u32) -> &'static str {
    rotate(&SAND, count)
}
pub fn streak(streak: u8) -> String {
    format!(
        "{} ×{streak}",
        rotate(&STREAK, u32::from(streak.saturating_sub(2)))
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card::RoundStats;

    #[test]
    fn rotations_never_repeat_back_to_back_and_visit_every_line() {
        let tables: [&[&str]; 16] = [
            &ACE,
            &UNDER,
            &EVEN,
            &OVER,
            &ROUGH,
            &PICKED_UP,
            &WATER,
            &LIP_OUT,
            &NEAR,
            &BANK,
            &TRICK,
            &PAUSE,
            &RETURN,
            &STREAK,
            &SAND,
            &[],
        ];
        for table in tables.into_iter().filter(|t| !t.is_empty()) {
            let mut seen = std::collections::BTreeSet::new();
            for n in 0..table.len() as u32 * 3 {
                assert_ne!(rotate(table, n), rotate(table, n + 1));
                seen.insert(rotate(table, n));
            }
            assert_eq!(seen.len(), table.len());
            // Bitmap-font copy: uppercase, short enough for a 320px wide sheet.
            for line in table.iter() {
                assert!(line.len() <= 40, "{line}");
                assert_eq!(line.to_uppercase(), *line);
            }
        }
    }

    #[test]
    fn every_outcome_has_personality() {
        let mut stats = RoundStats::default();
        let mut previous = "";
        for (strokes, picked) in [
            (1, false),
            (2, false),
            (3, false),
            (5, false),
            (9, false),
            (12, true),
        ] {
            let outcome = stats.finish_hole(3, strokes, 3, picked, 0, false);
            let line = hole(&outcome);
            assert!(!line.is_empty());
            assert_ne!(line, previous);
            previous = line;
        }
        assert_eq!(style(Style::Straight, 0), None);
        assert!(style(Style::Trick(3), 0).is_some());
        assert_eq!(streak(3), "HEATING UP ×3");
    }
}
