//! Deterministic orchard personality: pop words, toasts, ranks and goals.
//!
//! Every selector is a pure function of accepted simulation counters, so the
//! same run always reads the same way and no clock, RNG or render cadence is
//! involved. Consecutive counters never repeat a line from the same table.

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
    "SPLAT!",
    "BONK!",
    "POP!",
    "SQUISH!",
    "THWACK!",
    "PLONK!",
    "BOINK!",
    "KERPLUNK!",
];
const ZOOMIE: [&str; 4] = [
    "ZOOMIE BONKED!",
    "NOT SO FAST!",
    "SPEED BUMP!",
    "SKID... SPLAT!",
];
const LEAK: [&str; 8] = [
    "A BEETLE GOT A SNACK",
    "CHOMP. THAT WAS AN APPLE.",
    "ONE SNUCK PAST THE SLINGS",
    "THE PIE FUND TAKES A HIT",
    "RUDE. VERY RUDE.",
    "A FAINT BURP FROM THE ORCHARD",
    "WHO LEFT THE GATE OPEN?",
    "IT TOOK A SELFIE WITH A PEAR",
];
const CLEARED: [&str; 6] = [
    "THE PESTS HAVE BEEN POLITELY REMOVED",
    "BUGS 0, ORCHARD 1",
    "THE APPLES APPLAUD SOFTLY",
    "THE SCARECROW NODS APPROVINGLY",
    "SHAKE IT OFF, PLANT SOME MORE",
    "A TIDY HARVEST SO FAR",
];
const PERFECT: [&str; 4] = [
    "NOT ONE NIBBLE!",
    "FLAWLESS. THE TREES ARE BLUSHING.",
    "ZERO LEAKS. CHEF'S KISS.",
    "SPOTLESS! THE SCARECROW IS JEALOUS.",
];
const PAUSE: [&str; 5] = [
    "THE BEETLES ARE ALSO ON A TEA BREAK",
    "THE APPLES WILL WAIT. PROBABLY.",
    "STRETCH. THE SLINGS INSIST.",
    "EVEN SCARECROWS NEED A BREATHER",
    "SHHH. THE PEARS ARE NAPPING.",
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

/// Flavor for the incoming-wave card. Wave three teases the secret guest.
pub(super) fn incoming(wave: u8) -> &'static str {
    match wave {
        1 => "A FEW CURIOUS BEETLES. MOSTLY HARMLESS.",
        2 => "THEY TOLD THEIR FRIENDS ABOUT THE APPLES.",
        3 => "RUMOUR SAYS ROYALTY IS JOINING THIS ONE...",
        4 => "THE WHOLE FAMILY. AND THE COUSINS.",
        _ => "MORE BEETLES. THERE ARE ALWAYS MORE BEETLES.",
    }
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
                "ORCHARD LEGEND",
                "NOT A SINGLE APPLE NIBBLED. BARDS WILL SING OF THIS.",
            )
        } else if u32::from(run.lives) * 4 >= u32::from(run.max_lives) * 3 {
            ("HEAD GARDENER", "THE PIES ARE SAFE. THE BEETLES ARE NOT.")
        } else if u32::from(run.lives) * 3 >= u32::from(run.max_lives) {
            ("APPLE BODYGUARD", "A FEW BITE MARKS, BUT A FINE HARVEST.")
        } else {
            (
                "BY-A-STALK HERO",
                "ONE APPLE LEFT AND YOU CALLED IT A WIN. IT IS.",
            )
        }
    } else if run.wave >= run.waves {
        (
            "SO CLOSE, SO CRUNCHY",
            "THE LAST WAVE ATE THE LAST APPLE. RUDE.",
        )
    } else if run.wave + 1 >= run.waves {
        ("VALIANT WEEDER", "YOU FOUGHT WELL. THE BEETLES ATE WELL.")
    } else if run.wave >= 2 {
        ("PEST BUFFET MANAGER", "ALL-YOU-CAN-EAT, APPARENTLY.")
    } else {
        (
            "COMPOST ENTHUSIAST",
            "THE BEETLES SEND THEIR WARMEST THANKS.",
        )
    };
    Rank { title, quip }
}

/// The one-more-try line under the results. `best` is the previous best
/// pests-stopped score before this run, if one existed.
pub(super) fn next_goal(run: RunSummary, best: Option<u64>) -> String {
    let kills = u64::from(run.kills);
    if !run.won {
        return match best {
            Some(best) if kills <= best => format!(
                "STOP {} MORE PESTS TO BEAT YOUR BEST OF {best}",
                best - kills + 1
            ),
            _ if run.towers == 0 => {
                "TIP: CLICK A ROUND PLOT AND PLANT A TOWER. BEETLES LOVE AN OPEN DOOR.".into()
            }
            _ if !run.frost_planted => "TIP: FROST HIVES SLOW THE ZOOMIES DOWN".into(),
            _ if run.towers < 4 => "TIP: MORE PLOTS, MORE BONKS. COVER EVERY BEND.".into(),
            _ => "TIP: UPGRADE BEFORE WAVE 3. THE BEETLES GET CHUNKY.".into(),
        };
    }
    if run.lives < run.max_lives {
        format!(
            "NEXT: WIN WITH ALL {} HEARTS TO BECOME ORCHARD LEGEND",
            run.max_lives
        )
    } else if !run.royal {
        "SECRET: SOMEONE IMPORTANT WADDLES THROUGH WAVE 3...".into()
    } else {
        "NEXT: BEAT YOUR FASTEST HARVEST. TRY FAST-FORWARD FOR FUN.".into()
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
        assert_eq!(rank(run(true, 4, 12)).title, "ORCHARD LEGEND");
        assert_eq!(rank(run(true, 4, 9)).title, "HEAD GARDENER");
        assert_eq!(rank(run(true, 4, 4)).title, "APPLE BODYGUARD");
        assert_eq!(rank(run(true, 4, 1)).title, "BY-A-STALK HERO");
        assert_eq!(rank(run(false, 4, 0)).title, "SO CLOSE, SO CRUNCHY");
        assert_eq!(rank(run(false, 3, 0)).title, "VALIANT WEEDER");
        assert_eq!(rank(run(false, 2, 0)).title, "PEST BUFFET MANAGER");
        assert_eq!(rank(run(false, 1, 0)).title, "COMPOST ENTHUSIAST");
    }

    #[test]
    fn next_goal_is_concrete_and_reachable() {
        assert_eq!(
            next_goal(run(false, 2, 0), Some(45)),
            "STOP 6 MORE PESTS TO BEAT YOUR BEST OF 45"
        );
        assert!(next_goal(run(false, 2, 0), Some(10)).starts_with("TIP"));
        let mut lonely = run(false, 1, 0);
        lonely.frost_planted = false;
        assert!(next_goal(lonely, None).contains("FROST"));
        lonely.towers = 0;
        assert!(next_goal(lonely, None).contains("PLANT A TOWER"));
        assert!(next_goal(run(true, 4, 7), Some(64)).contains("ALL 12 HEARTS"));
        assert!(next_goal(run(true, 4, 12), None).contains("WAVE 3"));
        let mut royal = run(true, 4, 12);
        royal.royal = true;
        assert!(next_goal(royal, None).contains("FASTEST"));
    }
}
