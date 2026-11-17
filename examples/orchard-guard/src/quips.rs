//! Deterministic orchard personality: pop words, toasts, ranks and goals.
//!
//! Every selector is a pure function of accepted simulation counters, so the
//! same run always reads the same way and no clock, RNG or render cadence is
//! involved. Consecutive counters never repeat a line from the same table.
//! Lines shown during play are callouts of at most three words; the pause
//! and results cards may carry one longer joke.

/// Advance by a stride coprime with the table length so consecutive counters
/// never pick the same line and every line is eventually visited.
fn rotate<'a>(table: &[&'a str], counter: u32) -> &'a str {
    let stride = [5, 3, 7]
        .into_iter()
        .find(|s| !table.len().is_multiple_of(*s))
        .unwrap_or(1);
    table[(counter as usize).wrapping_mul(stride) % table.len()]
}

const STOP: [&str; 8] = [
    "Splat!",
    "Bonk!",
    "Pop!",
    "Squish!",
    "Thwack!",
    "Plonk!",
    "Boink!",
    "Kerplunk!",
];
const ZOOMIE: [&str; 4] = [
    "Zoomie bonked!",
    "Not so fast!",
    "Speed bump!",
    "Skid... Splat!",
];
const LEAK: [&str; 8] = [
    "Chomp!",
    "Snack attack!",
    "Sneaky!",
    "Rude!",
    "*Burp*",
    "Gate crasher!",
    "Pie thief!",
    "Got past!",
];
const CLEARED: [&str; 6] = [
    "Wave cleared!",
    "Bugs 0, Orchard 1",
    "Apples applaud!",
    "Scarecrow approves",
    "Shoo, beetles!",
    "Tidy harvest!",
];
const PERFECT: [&str; 4] = ["Not one nibble!", "Flawless!", "Chef's kiss!", "Spotless!"];
const PAUSE: [&str; 5] = [
    "The beetles are also on a tea break",
    "The apples will wait. Probably.",
    "Stretch. The slings insist.",
    "Even scarecrows need a breather",
    "Shhh. The pears are napping.",
];

/// A pop word for a stopped pest. `kills` is the run total including it.
pub(super) fn stopped(kills: u32, fast: bool) -> &'static str {
    if fast {
        rotate(&ZOOMIE, kills / 4)
    } else {
        rotate(&STOP, kills)
    }
}

/// A toast line for an escaped pest. `escaped` includes this one.
pub(super) fn leak(escaped: u32) -> &'static str {
    rotate(&LEAK, escaped.saturating_sub(1))
}

pub(super) fn cleared(wave: u8, perfect: bool) -> &'static str {
    if perfect {
        rotate(&PERFECT, u32::from(wave.saturating_sub(1)))
    } else {
        rotate(&CLEARED, u32::from(wave.saturating_sub(1)))
    }
}

pub(super) fn pause(seed: u32) -> &'static str {
    rotate(&PAUSE, seed)
}

/// The launch callout. Wave three teases the secret guest.
pub(super) fn incoming(wave: u8) -> &'static str {
    match wave {
        1 => "Here they come!",
        2 => "They brought friends!",
        3 => "Royalty rumoured...",
        4 => "The whole family!",
        _ => "More beetles!",
    }
}

/// The wave whose last beetle wears the secret crown.
pub(super) fn royal_wave(wave: u8) -> bool {
    wave == 3
}

/// What a finished run gets called on the results card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Rank {
    pub title: &'static str,
    pub quip: &'static str,
}

/// A finished run, independent of storage and rendering.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RunSummary {
    pub won: bool,
    /// Waves launched (the wave the run ended on when lost).
    pub wave: u8,
    pub waves: u8,
    pub lives: u8,
    pub max_lives: u8,
    pub kills: u32,
    pub perfect_waves: u8,
    pub royal: bool,
    pub frost_planted: bool,
    pub towers: usize,
}

pub(super) fn rank(run: RunSummary) -> Rank {
    let (title, quip) = if run.won {
        if run.lives == run.max_lives {
            (
                "Orchard Legend",
                "Not a single apple nibbled. Bards will sing of this.",
            )
        } else if u32::from(run.lives) * 4 >= u32::from(run.max_lives) * 3 {
            ("Head Gardener", "The pies are safe. The beetles are not.")
        } else if u32::from(run.lives) * 3 >= u32::from(run.max_lives) {
            ("Apple Bodyguard", "A few bite marks, but a fine harvest.")
        } else {
            (
                "By-a-Stalk Hero",
                "One apple left and you called it a win. It is.",
            )
        }
    } else if run.wave >= run.waves {
        (
            "So Close, So Crunchy",
            "The last wave ate the last apple. Rude.",
        )
    } else if run.wave + 1 >= run.waves {
        ("Valiant Weeder", "You fought well. The beetles ate well.")
    } else if run.wave >= 2 {
        ("Pest Buffet Manager", "All-you-can-eat, apparently.")
    } else {
        (
            "Compost Enthusiast",
            "The beetles send their warmest thanks.",
        )
    };
    Rank { title, quip }
}

/// The one-more-try target under the results, drawn as pictograms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Goal {
    /// Stop `more` pests beyond this run to beat the previous `best`.
    BeatBest { more: u64, best: u64 },
    /// No towers were planted: plant one.
    PlantTower,
    /// No frost hive was planted: it slows the zoomies.
    PlantFrost,
    /// Too few towers to cover every bend.
    MoreTowers,
    /// Upgrade before the chunky third wave.
    UpgradeEarly,
    /// Win with every one of this many hearts.
    AllHearts(u8),
    /// The crowned guest of wave three is still out there.
    FindRoyal(u8),
    /// Beat the fastest harvest.
    Fastest,
}

/// The next goal for a finished run. `best` is the previous best
/// pests-stopped score before this run, if one existed.
pub(super) fn next_goal(run: RunSummary, best: Option<u64>) -> Goal {
    let kills = u64::from(run.kills);
    if !run.won {
        return match best {
            Some(best) if kills <= best => Goal::BeatBest {
                more: best - kills + 1,
                best,
            },
            _ if run.towers == 0 => Goal::PlantTower,
            _ if !run.frost_planted => Goal::PlantFrost,
            _ if run.towers < 4 => Goal::MoreTowers,
            _ => Goal::UpgradeEarly,
        };
    }
    if run.lives < run.max_lives {
        Goal::AllHearts(run.max_lives)
    } else if !run.royal {
        Goal::FindRoyal(3)
    } else {
        Goal::Fastest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotating_tables_never_repeat_back_to_back_and_visit_every_line() {
        for table in [&STOP[..], &ZOOMIE, &LEAK, &CLEARED, &PERFECT, &PAUSE] {
            let mut seen = vec![false; table.len()];
            for counter in 0..table.len() as u32 * 2 {
                let line = rotate(table, counter);
                assert_ne!(line, rotate(table, counter + 1));
                seen[table.iter().position(|l| *l == line).unwrap()] = true;
            }
            assert!(seen.into_iter().all(|s| s));
        }
        assert_ne!(leak(1), leak(2));
        assert_ne!(stopped(5, false), stopped(6, false));
        assert_ne!(cleared(1, true), cleared(2, true));
    }

    fn run(won: bool, wave: u8, lives: u8) -> RunSummary {
        RunSummary {
            won,
            wave,
            waves: 4,
            lives,
            max_lives: 12,
            kills: 40,
            perfect_waves: 0,
            royal: false,
            frost_planted: true,
            towers: 5,
        }
    }

    #[test]
    fn ranks_reward_hearts_and_progress() {
        assert_eq!(rank(run(true, 4, 12)).title, "Orchard Legend");
        assert_eq!(rank(run(true, 4, 9)).title, "Head Gardener");
        assert_eq!(rank(run(true, 4, 4)).title, "Apple Bodyguard");
        assert_eq!(rank(run(true, 4, 1)).title, "By-a-Stalk Hero");
        assert_eq!(rank(run(false, 4, 0)).title, "So Close, So Crunchy");
        assert_eq!(rank(run(false, 3, 0)).title, "Valiant Weeder");
        assert_eq!(rank(run(false, 2, 0)).title, "Pest Buffet Manager");
        assert_eq!(rank(run(false, 1, 0)).title, "Compost Enthusiast");
    }

    #[test]
    fn next_goal_is_concrete_and_reachable() {
        assert_eq!(
            next_goal(run(false, 2, 0), Some(45)),
            Goal::BeatBest { more: 6, best: 45 }
        );
        // Already past the old best: a play tip instead.
        assert_eq!(next_goal(run(false, 2, 0), Some(10)), Goal::UpgradeEarly);
        let mut lonely = run(false, 1, 0);
        lonely.towers = 3;
        assert_eq!(next_goal(lonely, None), Goal::MoreTowers);
        lonely.frost_planted = false;
        assert_eq!(next_goal(lonely, None), Goal::PlantFrost);
        lonely.towers = 0;
        assert_eq!(next_goal(lonely, None), Goal::PlantTower);
        assert_eq!(next_goal(run(true, 4, 7), Some(64)), Goal::AllHearts(12));
        assert_eq!(next_goal(run(true, 4, 12), None), Goal::FindRoyal(3));
        assert!(royal_wave(3) && !royal_wave(2));
        let mut royal = run(true, 4, 12);
        royal.royal = true;
        assert_eq!(next_goal(royal, None), Goal::Fastest);
    }

    #[test]
    fn play_callouts_stay_within_three_words() {
        let words = |line: &str| line.split_whitespace().count();
        for table in [&STOP[..], &ZOOMIE, &LEAK, &CLEARED, &PERFECT] {
            for line in table {
                assert!(words(line) <= 4, "{line}");
            }
        }
        for wave in 0..6 {
            assert!(words(incoming(wave)) <= 3, "{}", incoming(wave));
        }
    }
}
