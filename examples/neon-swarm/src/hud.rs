//! Wide-layout HUD, banners, announcer and Ready/Paused/Result cards in the
//! authored 1280×800 space. The play HUD shows only what matters now: score
//! and chain on the left, the wave and drones left in the center, health and
//! shield on the right; dash, weapon level and energy cells below the arena.
//! One onboarding line fades out once fire and dash have been used.
use crate::game::{CLUTCH_HEALTH, Game, Style};
use crate::juice::{GOLD, Juice, fade, tier_color};
use crate::quips::{self, Rank, RunSummary};
use crate::{AMBER, CORAL, CYAN, FLOOR, INK, LINE, MUTED, PAPER, hint};
use ferravex_play::encounter::AdmissionPreview;
use ferravex_render::input::PlayerInput;
use ferravex_render::prelude::Color;
use ferravex_render::ui::{Align, ButtonStyle, TextStyle, Ui, UiRect};

/// The card panel color doubles as the shared menu contract in browser tests.
pub(crate) const CARD: Color = Color::new(0.025, 0.055, 0.07, 0.82);
/// The primary action keeps the authored 470..810 × 468..520 rectangle.
pub(crate) const ACTION: UiRect = UiRect {
    min: [470., 468.],
    size: [340., 52.],
};
const DIM: Color = Color::new(0.01, 0.02, 0.03, 0.45);
const SHIELD: Color = crate::draw::BATTERY;

/// Fire/dash onboarding, learned from accepted feedback events.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Learned {
    pub fired: bool,
    pub dashed: bool,
}

impl Learned {
    pub fn complete(self) -> bool {
        self.fired && self.dashed
    }
}

/// A finished run against stored records, captured once at the finish.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Results {
    pub summary: RunSummary,
    pub rank: Rank,
    pub time: f32,
    pub kills: u32,
    /// The stored best score before this run, if any.
    pub previous_best: Option<u64>,
    pub attempt: u32,
}

impl Results {
    pub fn new(game: &Game, previous_best: Option<u64>, attempt: u32) -> Self {
        let summary = summary(game);
        Self {
            summary,
            rank: quips::rank(summary),
            time: game.time,
            kills: game.kills,
            previous_best,
            attempt,
        }
    }

    #[cfg(test)]
    pub fn new_best(&self) -> bool {
        self.summary.score > 0
            && self
                .previous_best
                .is_none_or(|best| self.summary.score > best)
    }

    /// The record line: celebrate a best, otherwise show the gap to beat.
    pub fn record_line(&self) -> (String, Color) {
        let score = self.summary.score;
        match self.previous_best {
            _ if score == 0 => ("SCORE A KILL TO SET A RECORD".into(), MUTED),
            None => ("FIRST RECORD SET! NOW BEAT IT.".into(), GOLD),
            Some(best) if score > best => (
                format!("NEW PERSONAL BEST! +{}", thousands(score - best)),
                GOLD,
            ),
            Some(best) if score == best => ("TIED YOUR BEST. SO CLOSE.".into(), AMBER),
            Some(best) => (
                format!(
                    "BEST {} · {} TO BEAT IT",
                    thousands(best),
                    thousands(best - score)
                ),
                PAPER,
            ),
        }
    }
}

pub(crate) fn summary(game: &Game) -> RunSummary {
    RunSummary {
        won: game.phase == crate::game::Phase::Won,
        wave: game.wave,
        health: game.health,
        score: game.score.total(),
        best_chain: game.score.best(),
        style: game.style,
    }
}

/// Plain digits: the bitmap font's comma reads as a period at HUD sizes.
pub(crate) fn thousands(value: u64) -> String {
    value.to_string()
}

/// The first of several bindings, so hints stay short; Controls lists all.
pub(crate) fn key<'a>(input: &PlayerInput, action: &str, buffer: &'a mut [u8]) -> &'a str {
    let full = hint(input, action, buffer);
    full.split(" / ").next().unwrap_or(full)
}

/// Nonzero style feats as one short line, for results and pause.
pub(crate) fn style_line(style: &Style) -> String {
    let mut parts = Vec::with_capacity(5);
    let plural = |n: u32, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    if style.drive_bys > 0 {
        parts.push(plural(style.drive_bys, "DRIVE-BY", "DRIVE-BYS"));
    }
    if style.close_shaves > 0 {
        parts.push(plural(style.close_shaves, "CLOSE SHAVE", "CLOSE SHAVES"));
    }
    if style.multi_kills > 0 {
        parts.push(plural(style.multi_kills, "MULTI-KILL", "MULTI-KILLS"));
    }
    if style.untouched_waves > 0 {
        parts.push(plural(style.untouched_waves, "CLEAN WAVE", "CLEAN WAVES"));
    }
    if style.beacons_lit() > 0 {
        parts.push(format!("LAMPS {}/4", style.beacons_lit()));
    }
    if parts.is_empty() {
        "NO STYLE YET · DASH THROUGH ATTACKS, CHAIN KILLS".into()
    } else {
        parts.join(" · ")
    }
}

#[allow(clippy::too_many_arguments)] // Position, box and style read best inline.
fn text(
    ui: &mut Ui,
    x: f32,
    baseline: f32,
    width: f32,
    size: f32,
    color: Color,
    align: Align,
    s: &str,
) {
    ui.label(
        UiRect {
            min: [x, baseline - size * 0.875],
            size: [width, size],
        },
        s,
        TextStyle {
            size,
            color,
            align,
            ..Default::default()
        },
    )
    .unwrap();
}

fn center(ui: &mut Ui, baseline: f32, width: f32, size: f32, color: Color, s: &str) {
    text(
        ui,
        640. - width * 0.5,
        baseline,
        width,
        size,
        color,
        Align::Center,
        s,
    );
}

fn bar(ui: &mut Ui, x: f32, y: f32, w: f32, h: f32, fraction: f32, color: Color) {
    ui.panel(
        UiRect {
            min: [x, y],
            size: [w, h],
        },
        LINE,
    )
    .unwrap();
    let filled = w * fraction.clamp(0., 1.);
    if filled > 0.5 {
        ui.panel(
            UiRect {
                min: [x, y],
                size: [filled, h],
            },
            color,
        )
        .unwrap();
    }
}

/// The in-play HUD for the wide layout.
pub(crate) fn play(
    ui: &mut Ui,
    game: &Game,
    input: &PlayerInput,
    learned: Learned,
    onboarding: f32,
) {
    // Solid bands keep HUD copy readable when the camera pushes in or shakes.
    for (y, h) in [(0., 96.), (732., 68.)] {
        ui.panel(
            UiRect {
                min: [0., y],
                size: [1280., h],
            },
            INK,
        )
        .unwrap();
    }
    // Left: score and the live chain.
    text(ui, 72., 34., 200., 12., MUTED, Align::Left, "SCORE");
    ui.text_fmt(
        UiRect {
            min: [72., 38.],
            size: [300., 30.],
        },
        TextStyle {
            size: 30.,
            color: PAPER,
            shadow: true,
            ..Default::default()
        },
        format_args!("{}", thousands(game.score.total())),
    )
    .unwrap();
    let chain = game.score.chain();
    if chain > 0 {
        let tier = tier_color(game.score.multiplier());
        text(
            ui,
            72.,
            92.,
            60.,
            18.,
            tier,
            Align::Left,
            &format!("x{}", game.score.multiplier()),
        );
        text(
            ui,
            112.,
            90.,
            150.,
            12.,
            tier,
            Align::Left,
            &format!("CHAIN {chain}"),
        );
        let life = game.score.remaining_life().unwrap_or(0.) as f32 / 3.5;
        bar(ui, 112., 95., 120., 3., life, tier);
    } else {
        text(
            ui,
            72.,
            90.,
            240.,
            12.,
            MUTED,
            Align::Left,
            "KILL TO START A CHAIN",
        );
    }

    // Center: the wave and what is left of it.
    let name = quips::wave_name(game.wave);
    center(
        ui,
        34.,
        420.,
        13.,
        CYAN,
        &format!("WAVE {}/3 · {name}", game.wave),
    );
    let left = game.remaining_enemies();
    let headline = if game.final_drone() {
        "LAST DRONE STANDING".to_owned()
    } else if left == 0 {
        "WAVE CLEAR".to_owned()
    } else {
        format!("{left} DRONE{} LEFT", if left == 1 { "" } else { "S" })
    };
    ui.text_fmt(
        UiRect {
            min: [440., 42.],
            size: [400., 24.],
        },
        TextStyle {
            size: 24.,
            color: PAPER,
            align: Align::Center,
            shadow: true,
            ..Default::default()
        },
        format_args!("{headline}"),
    )
    .unwrap();
    for wave in 1..=3_u32 {
        let color = if wave < game.wave {
            CYAN
        } else if wave == game.wave {
            AMBER
        } else {
            LINE
        };
        ui.panel(
            UiRect {
                min: [592. + (wave - 1) as f32 * 34., 76.],
                size: [28., 4.],
            },
            color,
        )
        .unwrap();
    }
    if game.wave_clear_remaining().is_none() {
        let (admission, position) = game.spawn_preview();
        let held = match admission {
            AdmissionPreview::AliveLimit { .. } => Some("ENTRY HELD · ARENA FULL"),
            _ if position.is_none() && admission.next_available_in().is_some() => {
                Some("ENTRY HELD · NO SAFE SPOT")
            }
            _ => None,
        };
        if let Some(held) = held {
            center(ui, 98., 400., 12., AMBER, held);
        }
    }

    // Right: health and shield.
    let low = game.health <= 30;
    let hp = if low { CORAL } else { PAPER };
    text(ui, 1008., 34., 200., 12., MUTED, Align::Right, "HEALTH");
    ui.text_fmt(
        UiRect {
            min: [1008., 38.],
            size: [200., 30.],
        },
        TextStyle {
            size: 30.,
            color: hp,
            align: Align::Right,
            shadow: true,
            ..Default::default()
        },
        format_args!("{}", game.health),
    )
    .unwrap();
    bar(
        ui,
        1028.,
        76.,
        180.,
        7.,
        game.health as f32 / 100.,
        if low { CORAL } else { CYAN },
    );
    let shield = game.shield.value() / game.shield.maximum();
    bar(ui, 1028., 88., 180., 4., shield, SHIELD);
    text(
        ui,
        880.,
        94.,
        140.,
        11.,
        if shield > 0. { SHIELD } else { MUTED },
        Align::Right,
        &format!("SHIELD {}", game.shield.value().ceil() as u32),
    );

    // Bottom: dash on the left, weapon in the middle, cells on the right.
    let dash = game.dash_availability();
    let ready = dash.check().is_ok();
    text(
        ui,
        72.,
        748.,
        220.,
        15.,
        if ready { CYAN } else { MUTED },
        Align::Left,
        &if ready {
            "DASH READY".to_owned()
        } else {
            format!("DASH {:.1}s", dash.cooldown_remaining)
        },
    );
    bar(
        ui,
        72.,
        758.,
        150.,
        5.,
        1. - game.dash_cooldown / 1.7,
        if ready { CYAN } else { MUTED },
    );
    let mut buffer = [0; 128];
    if !learned.complete() || onboarding > 0. {
        let alpha = if learned.complete() {
            onboarding.min(1.)
        } else {
            1.
        };
        let line = if !learned.fired {
            format!("AIM + {}: FIRE", key(input, "fire", &mut buffer))
        } else if !learned.dashed {
            format!(
                "{}: DASH · YOU'RE UNTOUCHABLE MID-DASH",
                key(input, "boost", &mut buffer)
            )
        } else {
            "NICE. NOW CHAIN THOSE KILLS.".to_owned()
        };
        center(ui, 760., 520., 15., fade(AMBER, alpha), &line);
    } else {
        let level = game.progression.level();
        let (fraction, label) = match game.progression.next_cost() {
            Some(cost) => (
                game.progression.xp() as f32 / cost as f32,
                format!("WEAPON LV {level} · DMG {}", game.shot_damage()),
            ),
            None => (
                1.,
                format!("WEAPON LV {level} MAX · DMG {}", game.shot_damage()),
            ),
        };
        center(ui, 750., 400., 13., MUTED, &label);
        bar(ui, 540., 758., 200., 3., fraction, CYAN);
    }
    let cells = game.magazine.loaded();
    for cell in 0..12_u16 {
        let x = 1208. - (12 - cell) as f32 * 11.;
        ui.panel(
            UiRect {
                min: [x, 738.],
                size: [8., 14.],
            },
            if game.magazine.reloading() {
                fade(AMBER, 0.35)
            } else if cell < cells {
                CYAN
            } else {
                LINE
            },
        )
        .unwrap();
    }
    text(
        ui,
        1008.,
        770.,
        200.,
        12.,
        if game.magazine.reloading() {
            AMBER
        } else {
            MUTED
        },
        Align::Right,
        &if game.magazine.reloading() {
            format!("RECHARGING {:.1}s", game.magazine.remaining())
        } else {
            format!("{cells}/12 CELLS")
        },
    );
}

/// Wave intro/clear banners sit in the arena's middle band, clear of the
/// perimeter entry markers; the announcer slot sits near the top.
pub(crate) fn announcements(ui: &mut Ui, game: &Game, juice: &Juice) {
    if let Some(remaining) = game.wave_clear_remaining() {
        backing(ui, 1., CYAN);
        center(
            ui,
            378.,
            640.,
            36.,
            CYAN,
            &format!("WAVE {:02} CLEAR", game.wave),
        );
        let next = if game.wave == 3 {
            format!("SECURING SECTOR 07 · {remaining:.1}s")
        } else {
            format!(
                "NEXT UP: {} · {remaining:.1}s",
                quips::wave_name(game.wave + 1)
            )
        };
        center(ui, 408., 640., 15., PAPER, &next);
        if let Some(c) = juice
            .callout
            .as_ref()
            .filter(|c| c.priority >= crate::juice::Priority::Epic)
        {
            center(ui, 432., 640., 15., fade(c.color, c.alpha()), &c.head);
        }
        return;
    }
    if game.wave_banner > 0. {
        let alpha = (game.wave_banner / 0.4).min(1.);
        backing(ui, alpha, AMBER);
        center(
            ui,
            366.,
            640.,
            15.,
            fade(AMBER, alpha),
            &format!("WAVE {:02} / 03", game.wave),
        );
        center(
            ui,
            404.,
            640.,
            38.,
            fade(PAPER, alpha),
            quips::wave_name(game.wave),
        );
        let sub = if game.wave == 3 {
            format!(
                "{} DRONES + ONE VERY LARGE PROBLEM",
                game.remaining_enemies().saturating_sub(1)
            )
        } else {
            format!("{} DRONES INBOUND", game.remaining_enemies())
        };
        center(ui, 430., 640., 15., fade(CYAN, alpha), &sub);
        return;
    }
    if let Some(c) = &juice.callout {
        let alpha = c.alpha();
        let size = (32. * c.scale(juice.motion())).round();
        ui.text_fmt(
            UiRect {
                min: [240., 190. - size * 0.875],
                size: [800., size],
            },
            TextStyle {
                size,
                color: fade(c.color, alpha),
                align: Align::Center,
                shadow: true,
                ..Default::default()
            },
            format_args!("{}", c.head),
        )
        .unwrap();
        if !c.sub.is_empty() {
            center(ui, 214., 640., 14., fade(PAPER, alpha * 0.9), &c.sub);
        }
    }
}

fn backing(ui: &mut Ui, alpha: f32, accent: Color) {
    ui.panel(
        UiRect {
            min: [330., 340.],
            size: [620., 104.],
        },
        fade(INK, 0.78 * alpha),
    )
    .unwrap();
    for y in [340., 441.] {
        ui.panel(
            UiRect {
                min: [330., y],
                size: [620., 3.],
            },
            fade(accent, alpha),
        )
        .unwrap();
    }
}

fn card(ui: &mut Ui, rect: UiRect, accent: Color) {
    ui.panel(
        UiRect {
            min: [0., 0.],
            size: [1280., 800.],
        },
        DIM,
    )
    .unwrap();
    ui.panel(rect, CARD).unwrap();
    ui.panel(
        UiRect {
            min: [rect.min[0], rect.min[1]],
            size: [rect.size[0], 4.],
        },
        accent,
    )
    .unwrap();
}

fn action(ui: &mut Ui, label: &str) {
    ui.button(
        1,
        ACTION,
        label,
        true,
        ButtonStyle {
            fill: FLOOR,
            ink: CYAN,
            focus: CYAN,
            size: 20.,
            ..Default::default()
        },
    )
    .unwrap();
}

/// Minimal card used by the shared overlay path (and its touch tests).
pub(crate) fn simple(ui: &mut Ui, title: &str, detail: &str, label: &str) {
    card(
        ui,
        UiRect {
            min: [370., 284.],
            size: [540., 260.],
        },
        CYAN,
    );
    center(ui, 352., 500., 32., PAPER, title);
    center(ui, 412., 500., 16., MUTED, detail);
    action(ui, label);
}

pub(crate) fn ready(ui: &mut Ui, input: &PlayerInput, best: Option<(u64, u64)>) {
    card(
        ui,
        UiRect {
            min: [330., 136.],
            size: [620., 420.],
        },
        CYAN,
    );
    ui.text_fmt(
        UiRect {
            min: [340., 192. - 48. * 0.875],
            size: [600., 48.],
        },
        TextStyle {
            size: 48.,
            color: CYAN,
            align: Align::Center,
            shadow: true,
            ..Default::default()
        },
        format_args!("NEON SWARM"),
    )
    .unwrap();
    center(
        ui,
        222.,
        600.,
        15.,
        PAPER,
        "THREE WAVES. ONE HEXAGON. ZERO CHILL.",
    );
    let mut buffer = [0; 128];
    let row = |ui: &mut Ui, y: f32, key: &str, what: &str| {
        text(ui, 350., y, 270., 16., AMBER, Align::Right, key);
        text(ui, 642., y, 300., 16., PAPER, Align::Left, what);
    };
    row(ui, 268., &movement_hint(input), "MOVE");
    row(
        ui,
        296.,
        key(input, "fire", &mut buffer),
        "AIM + FIRE (HOLD)",
    );
    row(
        ui,
        324.,
        key(input, "boost", &mut buffer),
        "DASH (UNTOUCHABLE)",
    );
    center(
        ui,
        364.,
        600.,
        14.,
        MUTED,
        "AMBER RING = INCOMING BITE. DASH THROUGH IT FOR STYLE.",
    );
    center(
        ui,
        388.,
        600.,
        14.,
        MUTED,
        "CHAIN KILLS FOR UP TO x5 POINTS. A HIT BREAKS THE CHAIN.",
    );
    match best {
        Some((score, runs)) => center(
            ui,
            432.,
            600.,
            17.,
            GOLD,
            &format!("PERSONAL BEST {} · {runs} RUNS", thousands(score)),
        ),
        None => center(ui, 432., 600., 16., MUTED, "NO RECORD YET. GO MAKE ONE."),
    }
    action(ui, "START");
}

pub(crate) fn paused(ui: &mut Ui, game: &Game, input: &PlayerInput, pauses: u32) {
    card(
        ui,
        UiRect {
            min: [370., 236.],
            size: [540., 320.],
        },
        AMBER,
    );
    center(ui, 290., 500., 36., PAPER, "PAUSED");
    center(ui, 320., 500., 15., CYAN, quips::pause(pauses));
    center(
        ui,
        366.,
        500.,
        16.,
        PAPER,
        &format!(
            "WAVE {}/3 · {} PTS · x{} CHAIN",
            game.wave,
            thousands(game.score.total()),
            game.score.multiplier()
        ),
    );
    center(
        ui,
        392.,
        500.,
        13.,
        MUTED,
        &format!("{} KILLS · STYLE {}", game.kills, game.style.points),
    );
    let mut buffer = [0; 128];
    center(
        ui,
        430.,
        500.,
        13.,
        MUTED,
        &format!("RESTART: {}", key(input, "restart", &mut buffer)),
    );
    action(ui, "RESUME");
}

pub(crate) fn results(ui: &mut Ui, results: &Results, input: &PlayerInput) {
    let won = results.summary.won;
    let accent = if won { CYAN } else { CORAL };
    card(
        ui,
        UiRect {
            min: [310., 116.],
            size: [660., 440.],
        },
        accent,
    );
    ui.text_fmt(
        UiRect {
            min: [320., 164. - 38. * 0.875],
            size: [640., 38.],
        },
        TextStyle {
            size: 38.,
            color: accent,
            align: Align::Center,
            shadow: true,
            ..Default::default()
        },
        format_args!("{}", if won { "SECTOR SECURED" } else { "SIGNAL LOST" }),
    )
    .unwrap();
    center(ui, 194., 640., 13., MUTED, "YOUR RANK");
    center(ui, 220., 640., 24., GOLD, results.rank.title);
    center(ui, 242., 640., 14., PAPER, results.rank.quip);
    let s = results.summary;
    let stats: [(&str, String); 4] = if won {
        [
            ("SCORE", thousands(s.score)),
            ("TIME", format!("{:.0}s", results.time)),
            ("BEST CHAIN", s.best_chain.to_string()),
            ("STYLE", s.style.points.to_string()),
        ]
    } else {
        [
            ("SCORE", thousands(s.score)),
            ("REACHED", format!("WAVE {}", s.wave)),
            ("KILLS", results.kills.to_string()),
            ("STYLE", s.style.points.to_string()),
        ]
    };
    for (i, (name, value)) in stats.iter().enumerate() {
        let x = 330. + i as f32 * 157.;
        ui.panel(
            UiRect {
                min: [x, 258.],
                size: [148., 58.],
            },
            Color::new(0.06, 0.11, 0.13, 0.95),
        )
        .unwrap();
        text(ui, x, 288., 148., 24., PAPER, Align::Center, value);
        text(ui, x, 307., 148., 12., MUTED, Align::Center, name);
    }
    center(ui, 340., 640., 13., MUTED, &style_line(&s.style));
    let (record, color) = results.record_line();
    center(ui, 374., 640., 19., color, &record);
    center(ui, 404., 640., 15., CYAN, quips::next_goal(s));
    let quip = if s.won && s.health <= CLUTCH_HEALTH {
        format!("FINISHED ON {} HP. THE HEXAGON NEEDS A NAP.", s.health)
    } else if won {
        quips::win(results.attempt).to_owned()
    } else {
        quips::loss(results.attempt).to_owned()
    };
    center(ui, 436., 640., 14., MUTED, &quip);
    action(ui, "PLAY AGAIN");
    let mut buffer = [0; 128];
    center(
        ui,
        542.,
        640.,
        12.,
        MUTED,
        &format!("{}: PLAY AGAIN", key(input, "confirm", &mut buffer)),
    );
}

/// "WASD" when every movement binding is a single key, otherwise "MOVE KEYS".
pub(crate) fn movement_hint(input: &PlayerInput) -> String {
    let mut out = String::with_capacity(8);
    for action in ["move_forward", "move_left", "move_back", "move_right"] {
        let mut buffer = [0; 64];
        let first = key(input, action, &mut buffer);
        if first.chars().count() != 1 {
            return "SEE CONTROLS".into();
        }
        out.push_str(first);
    }
    out.to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{Phase, Style};

    #[test]
    fn records_lines_and_numbers_read_cleanly() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(12_345), "12345");
        let mut game = Game::new();
        game.phase = Phase::Lost;
        let none = Results::new(&game, None, 0);
        assert!(!none.new_best(), "a zero score is never a best");
        assert_eq!(none.record_line().1, MUTED);
        game.score.award(100);
        let first = Results::new(&game, None, 0);
        assert!(first.new_best());
        assert!(first.record_line().0.starts_with("FIRST RECORD"));
        let chase = Results::new(&game, Some(400), 1);
        assert!(!chase.new_best());
        assert_eq!(chase.record_line().0, "BEST 400 · 300 TO BEAT IT");
        let beat = Results::new(&game, Some(40), 2);
        assert!(beat.new_best());
        assert_eq!(beat.record_line().0, "NEW PERSONAL BEST! +60");
        assert_eq!(beat.rank.title, "WELCOME MAT");
    }

    #[test]
    fn style_line_lists_only_earned_feats() {
        assert!(style_line(&Style::default()).starts_with("NO STYLE"));
        let style = Style {
            drive_bys: 1,
            close_shaves: 3,
            beacons: 0b0101,
            ..Style::default()
        };
        assert_eq!(
            style_line(&style),
            "1 DRIVE-BY · 3 CLOSE SHAVES · LAMPS 2/4"
        );
        let learned = Learned {
            fired: true,
            dashed: false,
        };
        assert!(!learned.complete());
    }
}
