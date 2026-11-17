//! The dockyard crew's personality: bot names, radio banter, callouts, ranks
//! and end-of-match awards.
//!
//! Every selector is a pure function of accepted match counters, so the same
//! practice run always reads the same way and no clock, RNG or render cadence
//! is involved. Rotations never pick the same line twice in a row.
use crate::scoreboard::Summary;

/// Practice bot call signs, indexed by player id. Player 0 is always you.
const NAMES: [&str; 6] = [
    "YOU",
    "BARNACLE BOB",
    "FORKLIFT FRAN",
    "CAPTAIN GULL",
    "TUGBOAT TINA",
    "RUSTY",
];

/// A bot's call sign, or `None` for the local player and network operators.
pub(super) fn bot_name(id: u8) -> Option<&'static str> {
    NAMES.get(usize::from(id)).copied().filter(|_| id != 0)
}

/// Display name: practice bots get call signs; humans stay neutral operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Who {
    pub id: u8,
    pub local: u8,
    pub bots: bool,
}
impl std::fmt::Display for Who {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.id == self.local {
            f.write_str("YOU")
        } else if let Some(name) = bot_name(self.id).filter(|_| self.bots) {
            f.write_str(name)
        } else {
            write!(f, "OPERATOR {:02}", self.id)
        }
    }
}

/// Lines a bot says after fragging you (in play: three words at most).
const GLOAT: [[&str; 3]; 5] = [
    ["JUST FREIGHT.", "SIGNED. SEALED.", "A BOB-SHOT!"],
    ["BEEP BEEP.", "LIFTED. SHIFTED.", "FORKS UP!"],
    ["SKRAAA! GG!", "MINE. MINE.", "CHIPS ARE MINE."],
    ["BIG AIM.", "TOWED HOME.", "TOOT TOOT!"],
    ["PAINT AND SPITE.", "OXIDIZE THIS.", "STILL RUSTY."],
];

/// Lines a bot says as you frag it.
const LAST_WORDS: [[&str; 3]; 5] = [
    ["MY BREAK!", "TELL THE BARNACLES", "UNION!"],
    ["MY FORKS!", "PARKING BRAKE...", "LIFT... THIS..."],
    ["SKRAW?!", "THE SEA... CALLS", "NOT THE FEATHERS!"],
    ["SCUTTLED!", "ABANDON ME!", "TOOT... TOO..."],
    ["FINALLY, REST", "RUSTIER SOON.", "GONNA FLAKE"],
];

/// Lines for when the crew frags itself.
const OOPS: [&str; 5] = [
    "WRONG HARD HAT!",
    "TRAINING EXERCISE!",
    "HR INCOMING",
    "FRIENDLY FIRE!",
    "LOOKED LIKE HIM",
];

/// Encouragement on the elimination panel.
const RETRY: [&str; 6] = [
    "SHAKE IT OFF.",
    "CONTAINERS BELIEVE.",
    "AIM HIGHER.",
    "LUCKY SHOT.",
    "SAME RIFLE. AGAIN.",
    "COVER EXISTS.",
];

const PAUSE: [&str; 5] = [
    "THE CREW IS ON A TEA BREAK",
    "BARNACLE BOB IS STRETCHING",
    "THE CONTAINERS WILL WAIT",
    "HYDRATE. THE GULLS INSIST.",
    "FORKLIFT FRAN IS PARALLEL PARKING",
];

/// Advances by a stride coprime with the table length, so consecutive
/// counters never repeat a line and every line is eventually visited.
fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let stride = if table.len().is_multiple_of(5) { 3 } else { 5 };
    table[(counter as usize).wrapping_mul(stride) % table.len()]
}

fn bot_table(table: &'static [[&'static str; 3]; 5], id: u8) -> Option<&'static [&'static str]> {
    table.get(usize::from(id).checked_sub(1)?).map(|t| &t[..])
}

/// What bot `killer` says after its `nth` frag on you.
pub(super) fn gloat(killer: u8, nth: u32) -> Option<&'static str> {
    bot_table(&GLOAT, killer).map(|t| rotate(t, nth))
}

/// What bot `victim` says on its `nth` elimination by you.
pub(super) fn last_words(victim: u8, nth: u32) -> Option<&'static str> {
    bot_table(&LAST_WORDS, victim).map(|t| rotate(t, nth))
}

pub(super) fn oops(nth: u32) -> &'static str {
    rotate(&OOPS, nth)
}

pub(super) fn retry(deaths: u32) -> &'static str {
    rotate(&RETRY, deaths)
}

pub(super) fn pause(seed: u32) -> &'static str {
    rotate(&PAUSE, seed)
}

/// Kills inside one chaining window.
pub(super) fn multi_kill(count: u8) -> Option<&'static str> {
    match count {
        0 | 1 => None,
        2 => Some("DOUBLE TAP"),
        3 => Some("TRIPLE THREAT"),
        4 => Some("QUAD SQUAD"),
        _ => Some("DOCKYARD SWEEP"),
    }
}

/// Kills without dying. Only milestones get a banner (the banner shows the
/// count as a flame and a number, so no subtitle is needed).
pub(super) fn streak(count: u16) -> Option<&'static str> {
    match count {
        3 => Some("HEATING UP"),
        5 => Some("FREIGHT TRAIN"),
        7 => Some("UNSTOPPABLE CARGO"),
        10 => Some("KRAKEN MODE"),
        15 => Some("DOCK LEGEND"),
        _ => None,
    }
}

/// Rank thresholds by frags, from the bottom. Each has a title and a quip.
const RANKS: [(u32, &str, &str); 7] = [
    (
        0,
        "SEAGULL SPECTATOR",
        "THE GULLS SAW EVERYTHING. THEY WON'T TALK.",
    ),
    (1, "DECKHAND", "EVERYONE STARTS SWABBING SOMEWHERE"),
    (
        4,
        "FORKLIFT CERTIFIED",
        "YOU LIFT. YOU FRAG. YOU PARK NICELY.",
    ),
    (8, "CRANE OPERATOR", "HEAVY LIFTING, LIGHT TRIGGER"),
    (12, "DOCK BOSS", "THE CONTAINERS FILE REPORTS ABOUT YOU"),
    (16, "HARBOR MASTER", "THE TIDES CHECK WITH YOU FIRST"),
    (
        20,
        "KRAKEN OF THE DOCKYARD",
        "THE CREW HAS REQUESTED A TRANSFER",
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
}

/// A silly, never mean title. Rare feats outrank the frag ladder.
pub(super) fn rank(s: &Summary) -> Rank {
    if s.frags >= 5 && s.deaths == 0 {
        return Rank {
            title: "UNSINKABLE",
            quip: "NOT ONE SCRATCH. THE CREW IS IN THERAPY.",
        };
    }
    if s.frags == 0 && s.shots == 0 {
        return Rank {
            title: "CONSCIENTIOUS OBJECTOR",
            quip: "ZERO SHOTS FIRED. A BOLD PEACE PLAN.",
        };
    }
    let (_, title, quip) = RANKS
        .iter()
        .rev()
        .find(|(min, ..)| s.frags >= *min)
        .copied()
        .unwrap_or(RANKS[0]);
    Rank { title, quip }
}

/// The next frag-ladder title and how many more frags it needs.
pub(super) fn next_rank(frags: u32) -> Option<(&'static str, u32)> {
    RANKS
        .iter()
        .find(|(min, ..)| *min > frags)
        .map(|(min, title, _)| (*title, min - frags))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Award {
    Duck,
    Untouchable,
    Headhunter(u32),
    Sharpshooter(u32),
    Combo(u8),
    Revenge(u32),
    Wall(u32),
    Gulls(u32),
    Oops(u32),
    Nemesis(u8, u32),
    Respawn(u32),
    Pacifist,
}

impl Award {
    pub fn title(self) -> &'static str {
        match self {
            Self::Duck => "DUCK HUNTER",
            Self::Untouchable => "UNTOUCHABLE",
            Self::Headhunter(_) => "HEADHUNTER",
            Self::Sharpshooter(_) => "SHARPSHOOTER",
            Self::Combo(_) => "COMBO ARTIST",
            Self::Revenge(_) => "SERVED COLD",
            Self::Wall(_) => "MOST SHOTS INTO A WALL",
            Self::Gulls(_) => "SEAGULL HARASSER",
            Self::Oops(_) => "HR INCIDENT",
            Self::Nemesis(..) => "NEMESIS",
            Self::Respawn(_) => "RESPAWN ENTHUSIAST",
            Self::Pacifist => "PACIFIST",
        }
    }
    /// The pictogram shown beside the award's title.
    pub fn icon(self) -> ferravex_render::icons::Icon {
        use ferravex_render::icons::Icon;
        match self {
            Self::Duck => Icon::Star,
            Self::Untouchable => Icon::Shield,
            Self::Headhunter(_) => Icon::Target,
            Self::Sharpshooter(_) => Icon::Eye,
            Self::Combo(_) => Icon::Bolt,
            Self::Revenge(_) => Icon::Flame,
            Self::Wall(_) => Icon::Crate,
            Self::Gulls(_) => Icon::Cloud,
            Self::Oops(_) => Icon::Alert,
            Self::Nemesis(..) => Icon::Skull,
            Self::Respawn(_) => Icon::Restart,
            Self::Pacifist => Icon::Heart,
        }
    }
    /// Write the award's compact stat: a number, a percentage or a name
    /// (empty when the title says it all).
    pub fn stat(self, f: &mut impl std::fmt::Write) -> std::fmt::Result {
        match self {
            Self::Duck | Self::Untouchable | Self::Pacifist => Ok(()),
            Self::Headhunter(n)
            | Self::Revenge(n)
            | Self::Wall(n)
            | Self::Gulls(n)
            | Self::Respawn(n) => write!(f, "{n}"),
            Self::Sharpshooter(n) => write!(f, "{n}%"),
            Self::Combo(n) => write!(f, "×{n}"),
            Self::Oops(n) => write!(f, "×{n}"),
            Self::Nemesis(id, n) => {
                write!(f, "{} ×{n}", bot_name(id).unwrap_or("SOMEONE"))
            }
        }
    }
}

/// Up to `N` awards, rarest/most flattering first, then the silly ones.
pub(super) fn awards<const N: usize>(s: &Summary) -> [Option<Award>; N] {
    let accuracy = (s.hits * 100).checked_div(s.shots).unwrap_or(0);
    let candidates = [
        s.duck.then_some(Award::Duck),
        (s.deaths == 0 && s.frags > 0).then_some(Award::Untouchable),
        (s.headshots >= 3).then_some(Award::Headhunter(s.headshots)),
        (s.shots >= 15 && accuracy >= 40).then_some(Award::Sharpshooter(accuracy)),
        (s.best_multi >= 2).then_some(Award::Combo(s.best_multi)),
        (s.revenge >= 2).then_some(Award::Revenge(s.revenge)),
        (s.wall_shots >= 12).then_some(Award::Wall(s.wall_shots)),
        (s.sky_shots >= 8).then_some(Award::Gulls(s.sky_shots)),
        (s.crew_oops >= 1).then_some(Award::Oops(s.crew_oops)),
        s.nemesis
            .filter(|(_, n)| *n >= 2)
            .map(|(id, n)| Award::Nemesis(id, n)),
        (s.deaths >= 6).then_some(Award::Respawn(s.deaths)),
        (s.shots == 0).then_some(Award::Pacifist),
    ];
    let mut out = [None; N];
    for (slot, award) in out.iter_mut().zip(candidates.into_iter().flatten()) {
        *slot = Some(award);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_never_repeat_back_to_back_and_visit_every_line() {
        for table in [&OOPS[..], &RETRY[..], &PAUSE[..]]
            .into_iter()
            .chain(GLOAT.iter().map(|t| &t[..]))
            .chain(LAST_WORDS.iter().map(|t| &t[..]))
        {
            let mut previous = "";
            let mut seen = std::collections::BTreeSet::new();
            for n in 0..64 {
                let line = rotate(table, n);
                assert_ne!(line, previous, "repeated {line}");
                previous = line;
                seen.insert(line);
            }
            assert_eq!(seen.len(), table.len());
        }
        assert_eq!(gloat(0, 1), None, "you never gloat at yourself");
        assert_eq!(last_words(6, 1), None);
        assert!(gloat(5, 1).is_some() && last_words(1, 0).is_some());
    }

    #[test]
    fn in_play_lines_stay_punchy() {
        for line in GLOAT
            .iter()
            .chain(LAST_WORDS.iter())
            .flat_map(|t| t.iter())
            .chain(OOPS.iter())
            .chain(RETRY.iter())
        {
            assert!(line.split(' ').count() <= 3, "{line}");
        }
        for n in 0..20 {
            assert!(streak(n).is_none_or(|head| head.split(' ').count() <= 3));
        }
    }

    #[test]
    fn names_are_practice_only_and_operators_stay_neutral() {
        assert_eq!(
            Who {
                id: 0,
                local: 0,
                bots: true
            }
            .to_string(),
            "YOU"
        );
        assert_eq!(
            Who {
                id: 3,
                local: 0,
                bots: true
            }
            .to_string(),
            "CAPTAIN GULL"
        );
        assert_eq!(
            Who {
                id: 3,
                local: 1,
                bots: false
            }
            .to_string(),
            "OPERATOR 03"
        );
        assert_eq!(
            Who {
                id: 9,
                local: 0,
                bots: true
            }
            .to_string(),
            "OPERATOR 09"
        );
    }

    fn summary() -> Summary {
        Summary {
            frags: 9,
            deaths: 3,
            shots: 80,
            hits: 20,
            headshots: 1,
            wall_shots: 30,
            sky_shots: 2,
            best_streak: 3,
            best_multi: 1,
            revenge: 0,
            crew_oops: 0,
            duck: false,
            nemesis: Some((2, 2)),
        }
    }

    #[test]
    fn ranks_climb_with_frags_and_rare_feats_come_first() {
        let s = summary();
        assert_eq!(rank(&s).title, "CRANE OPERATOR");
        assert_eq!(next_rank(9), Some(("DOCK BOSS", 3)));
        assert_eq!(next_rank(40), None);
        assert_eq!(rank(&Summary { frags: 0, ..s }).title, "SEAGULL SPECTATOR");
        assert_eq!(
            rank(&Summary {
                frags: 0,
                shots: 0,
                hits: 0,
                ..s
            })
            .title,
            "CONSCIENTIOUS OBJECTOR"
        );
        assert_eq!(rank(&Summary { deaths: 0, ..s }).title, "UNSINKABLE");
        assert_eq!(
            rank(&Summary { frags: 25, ..s }).title,
            "KRAKEN OF THE DOCKYARD"
        );
        for frags in 0..30 {
            let r = rank(&Summary { frags, ..s });
            assert!(!r.title.is_empty() && !r.quip.is_empty());
        }
    }

    #[test]
    fn awards_are_bounded_ordered_and_describe_themselves() {
        let s = summary();
        let a = awards::<3>(&s);
        assert_eq!(a, [Some(Award::Wall(30)), Some(Award::Nemesis(2, 2)), None]);
        let lucky = Summary {
            duck: true,
            deaths: 0,
            headshots: 5,
            ..s
        };
        assert_eq!(
            awards::<2>(&lucky),
            [Some(Award::Duck), Some(Award::Untouchable)]
        );
        let mut text = String::new();
        Award::Nemesis(2, 4).stat(&mut text).unwrap();
        assert_eq!(text, "FORKLIFT FRAN ×4");
        text.clear();
        Award::Sharpshooter(62).stat(&mut text).unwrap();
        assert_eq!(text, "62%");
        text.clear();
        Award::Untouchable.stat(&mut text).unwrap();
        assert!(text.is_empty(), "the title says it all");
        assert_eq!(multi_kill(2), Some("DOUBLE TAP"));
        assert_eq!(multi_kill(9), Some("DOCKYARD SWEEP"));
        assert_eq!(streak(4), None);
        assert!(streak(5).is_some());
    }
}
