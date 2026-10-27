//! Platform-free scorecard semantics: score names, round statistics, rank
//! titles and the "one more round" goal. Everything here is a pure function of
//! accepted rules outcomes, so both views and tests read the same results.
//!
//! ```
//! use mini_golf::card::{self, RoundStats, ScoreName, Style};
//! let mut stats = RoundStats::default();
//! // Hole 2 (par 3) holed in two after one bank.
//! let hole = stats.finish_hole(1, 2, 3, false, 1, false);
//! assert_eq!(hole.name, ScoreName::Birdie);
//! assert_eq!(hole.style, Style::Bank(1));
//! assert_eq!(card::rank(-5).title, "WINDMILL WHISPERER");
//! assert_eq!(card::to_par_text(0), "E");
//! ```
use crate::{course, game::MAX_STROKES};

/// Course par for the full twelve-hole round.
pub fn course_par() -> u16 {
    (0..12).map(|i| u16::from(course::hole(i).par)).sum()
}

/// Golf's name for one finished hole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScoreName {
    Ace,
    Albatross,
    Eagle,
    Birdie,
    Par,
    Bogey,
    DoubleBogey,
    TripleBogey,
    /// Four or more over par; carries the overage.
    Over(u8),
    /// Conceded or capped at the stroke limit.
    PickedUp,
}

/// How a result should feel: drives color, sound and quip table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tone {
    Ace,
    Under,
    Even,
    Over,
    Rough,
}

impl ScoreName {
    pub fn of(strokes: u8, par: u8, picked_up: bool) -> Self {
        if picked_up || strokes >= MAX_STROKES {
            return Self::PickedUp;
        }
        if strokes == 1 {
            return Self::Ace;
        }
        match i16::from(strokes) - i16::from(par) {
            i16::MIN..=-3 => Self::Albatross,
            -2 => Self::Eagle,
            -1 => Self::Birdie,
            0 => Self::Par,
            1 => Self::Bogey,
            2 => Self::DoubleBogey,
            3 => Self::TripleBogey,
            n => Self::Over(n.clamp(4, 99) as u8),
        }
    }
    /// Short headline used by callouts and the between-hole sheet.
    pub fn headline(self) -> String {
        match self {
            Self::Ace => "HOLE IN ONE!".into(),
            Self::Albatross => "ALBATROSS!".into(),
            Self::Eagle => "EAGLE!".into(),
            Self::Birdie => "BIRDIE!".into(),
            Self::Par => "PAR".into(),
            Self::Bogey => "BOGEY".into(),
            Self::DoubleBogey => "DOUBLE BOGEY".into(),
            Self::TripleBogey => "TRIPLE BOGEY".into(),
            Self::Over(n) => format!("+{n} AND COUNTING"),
            Self::PickedUp => "PICKED UP".into(),
        }
    }
    pub fn tone(self) -> Tone {
        match self {
            Self::Ace => Tone::Ace,
            Self::Albatross | Self::Eagle | Self::Birdie => Tone::Under,
            Self::Par => Tone::Even,
            Self::Bogey | Self::DoubleBogey => Tone::Over,
            Self::TripleBogey | Self::Over(_) | Self::PickedUp => Tone::Rough,
        }
    }
}

/// How many surfaces a holed putt kissed on the way in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Straight,
    /// One or two banks before dropping.
    Bank(u8),
    /// Three or more banks before dropping.
    Trick(u8),
}

impl Style {
    pub fn of(banks: u8) -> Self {
        match banks {
            0 => Self::Straight,
            1 | 2 => Self::Bank(banks),
            n => Self::Trick(n),
        }
    }
}

/// Everything presentation needs to celebrate one finished hole.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HoleOutcome {
    pub hole: usize,
    pub strokes: u8,
    pub par: u8,
    pub name: ScoreName,
    pub style: Style,
    /// A moving copper barrier helped this final putt drop: the hidden trick.
    pub assist: bool,
    /// First time this round's assist was found on this hole.
    pub assist_found: bool,
    /// Holes in a row at par or better, including this one (0 when broken).
    pub streak: u8,
    /// Ordinal of completed holes this session, for non-repeating quips.
    pub count: u32,
}

/// Round-wide counters for the results card. Practice uses its own instance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoundStats {
    pub holes: u8,
    pub aces: u8,
    pub under: u8,
    pub pars: u8,
    pub over: u8,
    pub picked_up: u8,
    pub bank_shots: u8,
    pub trick_shots: u8,
    pub splashes: u8,
    pub lip_outs: u8,
    pub streak: u8,
    pub best_streak: u8,
    /// Bitmask of holes where the hidden copper assist was found.
    pub assists: u16,
    /// Completed holes in this app session; never reset, keeps quips fresh.
    pub session_holes: u32,
}

impl RoundStats {
    /// Start a new round while keeping the session counter for quip rotation.
    pub fn reset(&mut self) {
        *self = Self {
            session_holes: self.session_holes,
            ..Self::default()
        };
    }
    /// Record one finished hole and describe it for presentation.
    pub fn finish_hole(
        &mut self,
        hole: usize,
        strokes: u8,
        par: u8,
        picked_up: bool,
        banks: u8,
        assist: bool,
    ) -> HoleOutcome {
        let name = ScoreName::of(strokes, par, picked_up);
        let style = if picked_up {
            Style::Straight
        } else {
            Style::of(banks)
        };
        self.holes = self.holes.saturating_add(1);
        self.session_holes = self.session_holes.wrapping_add(1);
        match name.tone() {
            Tone::Ace => {
                self.aces += 1;
                if strokes < par {
                    self.under += 1;
                } else {
                    self.pars += 1;
                }
            }
            Tone::Under => self.under += 1,
            Tone::Even => self.pars += 1,
            Tone::Over | Tone::Rough => self.over += 1,
        }
        if name == ScoreName::PickedUp {
            self.picked_up += 1;
        }
        match style {
            Style::Straight => {}
            Style::Bank(_) => self.bank_shots += 1,
            Style::Trick(_) => self.trick_shots += 1,
        }
        if name.tone() <= Tone::Even {
            self.streak = self.streak.saturating_add(1);
        } else {
            self.streak = 0;
        }
        self.best_streak = self.best_streak.max(self.streak);
        let assist = assist && !picked_up && hole < 16;
        let assist_found = assist && self.assists & (1 << hole) == 0;
        if assist {
            self.assists |= 1 << hole;
        }
        HoleOutcome {
            hole,
            strokes,
            par,
            name,
            style,
            assist,
            assist_found,
            streak: self.streak,
            count: self.session_holes,
        }
    }
    pub fn assist_count(&self) -> u32 {
        self.assists.count_ones()
    }
}

/// Holes where a moving copper barrier exists, so the hidden assist is possible.
pub fn assist_holes() -> u32 {
    (0..12)
        .filter(|&i| {
            course::hole(i)
                .walls
                .iter()
                .any(|w| w.spin != 0. || w.travel != ferravex_math::Vec2::ZERO)
        })
        .count() as u32
}

/// A silly title earned by a finished round.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
    /// Highest to-par score that still earns this title.
    pub threshold: i16,
}

/// Ordered best first. Thresholds are relative to the course par.
const RANKS: [Rank; 9] = [
    Rank {
        title: "COPPER DEITY",
        quip: "THE WINDMILL BOWS AS YOU PASS",
        threshold: -12,
    },
    Rank {
        title: "GREENS WIZARD",
        quip: "WAS THAT PUTTER ENCHANTED?",
        threshold: -8,
    },
    Rank {
        title: "WINDMILL WHISPERER",
        quip: "THE SAILS PART FOR YOU",
        threshold: -4,
    },
    Rank {
        title: "CUP MAGNET",
        quip: "BALLS JUST LIKE YOU. IT'S A GIFT.",
        threshold: -1,
    },
    Rank {
        title: "SCRATCH PUTTER",
        quip: "PAR FOR THE COURSE. LITERALLY.",
        threshold: 0,
    },
    Rank {
        title: "CLUBHOUSE REGULAR",
        quip: "THE SNACK BAR KNOWS YOUR ORDER",
        threshold: 4,
    },
    Rank {
        title: "WEEKEND PUTTER",
        quip: "RESPECTABLE. MOSTLY.",
        threshold: 10,
    },
    Rank {
        title: "SCENIC ROUTE TOURIST",
        quip: "YOU SAW EVERY PART OF EVERY HOLE",
        threshold: 20,
    },
    Rank {
        title: "DUCK POND PATRON",
        quip: "THE DUCKS HAVE NAMED A BALL AFTER YOU",
        threshold: i16::MAX,
    },
];

pub fn rank(to_par: i16) -> Rank {
    *RANKS
        .iter()
        .find(|r| to_par <= r.threshold)
        .unwrap_or(&RANKS[RANKS.len() - 1])
}

/// The next better title and how many strokes it would take to reach it.
pub fn next_rank(to_par: i16) -> Option<(Rank, i16)> {
    let index = RANKS.iter().position(|r| to_par <= r.threshold)?;
    let better = RANKS.get(index.checked_sub(1)?)?;
    Some((*better, to_par - better.threshold))
}

/// Personal-best comparison for a finished full round (lower is better).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Best {
    /// No previous full round on record.
    First,
    /// Beat the previous best by this many strokes.
    New(u16),
    Tied,
    /// Missed the previous best by this many strokes.
    Short(u16),
}

impl Best {
    pub fn compare(total: u16, previous: Option<u16>) -> Self {
        match previous {
            None => Self::First,
            Some(p) if total < p => Self::New(p - total),
            Some(p) if total == p => Self::Tied,
            Some(p) => Self::Short(total - p),
        }
    }
    /// One line that makes another round tempting.
    pub fn line(self) -> String {
        match self {
            Self::First => "FIRST ROUND ON RECORD. NOW BEAT IT.".into(),
            Self::New(n) => format!("NEW PERSONAL BEST! {n} BETTER THAN BEFORE"),
            Self::Tied => "TIED YOUR BEST. ONE STROKE FOR GLORY.".into(),
            Self::Short(1) => "ONE STROKE OFF YOUR BEST. SO CLOSE.".into(),
            Self::Short(n) => format!("{n} STROKES OFF YOUR BEST"),
        }
    }
}

/// Signed to-par display: E for even.
pub fn to_par_text(to_par: i16) -> String {
    if to_par == 0 {
        "E".into()
    } else {
        format!("{to_par:+}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_names_follow_golf_and_pickups() {
        assert_eq!(ScoreName::of(1, 2, false), ScoreName::Ace);
        assert_eq!(ScoreName::of(1, 4, false), ScoreName::Ace);
        assert_eq!(ScoreName::of(2, 4, false), ScoreName::Eagle);
        assert_eq!(ScoreName::of(2, 3, false), ScoreName::Birdie);
        assert_eq!(ScoreName::of(3, 3, false), ScoreName::Par);
        assert_eq!(ScoreName::of(4, 3, false), ScoreName::Bogey);
        assert_eq!(ScoreName::of(5, 3, false), ScoreName::DoubleBogey);
        assert_eq!(ScoreName::of(6, 3, false), ScoreName::TripleBogey);
        assert_eq!(ScoreName::of(7, 3, false), ScoreName::Over(4));
        assert_eq!(ScoreName::of(7, 3, false).headline(), "+4 AND COUNTING");
        assert_eq!(ScoreName::of(3, 3, true), ScoreName::PickedUp);
        assert_eq!(ScoreName::of(MAX_STROKES, 3, false), ScoreName::PickedUp);
        assert!(ScoreName::Ace.tone() < ScoreName::Birdie.tone());
        assert!(ScoreName::Par.tone() < ScoreName::Bogey.tone());
    }

    #[test]
    fn stats_track_streaks_styles_and_hidden_assists_once() {
        let mut s = RoundStats::default();
        let a = s.finish_hole(0, 1, 2, false, 0, false);
        assert_eq!(
            (a.name, a.streak, a.style),
            (ScoreName::Ace, 1, Style::Straight)
        );
        let b = s.finish_hole(5, 2, 3, false, 1, true);
        assert_eq!(
            (b.name, b.streak, b.style),
            (ScoreName::Birdie, 2, Style::Bank(1))
        );
        assert!(b.assist && b.assist_found);
        let c = s.finish_hole(5, 3, 3, false, 4, true);
        assert!(
            c.assist && !c.assist_found,
            "the secret is announced only once"
        );
        assert_eq!(c.style, Style::Trick(4));
        let d = s.finish_hole(6, 12, 4, true, 3, true);
        assert_eq!(
            (d.name, d.streak, d.style),
            (ScoreName::PickedUp, 0, Style::Straight)
        );
        assert!(!d.assist, "a pickup never earns the trick");
        assert_eq!(
            (s.aces, s.under, s.pars, s.over, s.picked_up),
            (1, 2, 1, 1, 1)
        );
        assert_eq!((s.bank_shots, s.trick_shots, s.best_streak), (1, 1, 3));
        assert_eq!(s.assist_count(), 1);
        let session = s.session_holes;
        s.reset();
        assert_eq!(s.holes, 0);
        assert_eq!(s.session_holes, session, "quip rotation survives restart");
    }

    #[test]
    fn ranks_are_ordered_and_always_offer_a_next_goal_until_the_top() {
        assert_eq!(course_par(), 40);
        assert_eq!(assist_holes(), 3);
        let mut last = rank(-40);
        assert_eq!(last.title, "COPPER DEITY");
        assert!(next_rank(-40).is_none());
        for to_par in -39..60 {
            let r = rank(to_par);
            assert!(to_par <= r.threshold);
            assert!(r.threshold >= last.threshold);
            last = r;
            if r.title != "COPPER DEITY" {
                let (better, strokes) = next_rank(to_par).unwrap();
                assert!(better.threshold < r.threshold);
                assert!(strokes > 0);
                assert_eq!(rank(to_par - strokes).title, better.title);
            }
        }
        assert_eq!(rank(80).title, "DUCK POND PATRON");
    }

    #[test]
    fn personal_best_lines_compare_lower_is_better() {
        assert_eq!(Best::compare(40, None), Best::First);
        assert_eq!(Best::compare(38, Some(40)), Best::New(2));
        assert_eq!(Best::compare(40, Some(40)), Best::Tied);
        assert_eq!(Best::compare(41, Some(40)), Best::Short(1));
        assert!(Best::Short(1).line().contains("ONE STROKE"));
        assert_eq!(to_par_text(0), "E");
        assert_eq!(to_par_text(-3), "-3");
        assert_eq!(to_par_text(5), "+5");
    }
}
