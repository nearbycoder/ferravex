//! Header, dot-matrix display, footer hint and the Ready/Paused/Results cards.
//!
//! The header carries only score, best, balls and multiplier. One dot-matrix
//! display (DMD) under the table shows a single message at a time: an event
//! callout when one is fresh, otherwise the next objective. One footer line
//! carries the launch prompt and a flipper hint that retires once learned.
use super::presentation::{FLIPS_TO_LEARN, GOLD, Presentation};
use super::quips;
use super::*;

const CARD: Color = Color::new(0.06, 0.09, 0.09, 0.97);
pub(super) const RESULTS_HEIGHT: f32 = 248.;
const READY_GOALS: [&str; 2] = ["3 BUMPERS ARM THE CRANE", "SHOOT UNDER IT FOR A JACKPOT"];

/// Bindings formatted once per frame from the live input map.
pub(super) struct Hints<'a> {
    pub launch: &'a str,
    pub left: &'a str,
    pub right: &'a str,
    pub pause: &'a str,
    pub restart: &'a str,
    pub confirm: &'a str,
    pub touch: bool,
}

/// The finished game's comparison with stored records, captured once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Results {
    /// The best stored score before this game, if any.
    pub previous_best: Option<u32>,
    /// Autoplay demonstrations are shown but never recorded or celebrated.
    pub demo: bool,
}

impl Results {
    pub fn new_best(&self, score: u32) -> bool {
        !self.demo && score > 0 && self.previous_best.is_none_or(|best| score > best)
    }
}

pub(super) fn right_text(text: &str, right: f32, y: f32, size: f32, color: Color) {
    if let Some((size, width)) = fitted_text(text, size, f32::MAX) {
        draw_text(text, right - width, y, size, color);
    }
}

/// Score, best, balls and multiplier; the Reset target sits at the far right.
pub(super) fn header(table: &Table, view: &Presentation, best: Option<u32>, hud: HudLayout) {
    let (hud_x, balls_right, score_room, reset) =
        (hud.left, hud.balls_right, hud.score_room, hud.reset);
    draw_text("SCORE", hud_x, 22., 13., BRASS);
    let score = quips::grouped(view.shown_score.round() as u32);
    // The balls group needs ~64px; the score shrinks before it can collide.
    if let Some((size, _)) = fitted_text(&score, 32., score_room) {
        draw_text(&score, hud_x, 54., size, CREAM);
    }
    let best_line = match best {
        Some(best) if table.score > best && best > 0 => "NEW BEST PACE!".to_owned(),
        Some(best) => format!("BEST {}", quips::grouped(best)),
        None => "NO BEST YET".to_owned(),
    };
    let best_color = if best.is_some_and(|b| table.score > b && b > 0) {
        GOLD
    } else {
        MINT
    };
    if let Some((size, _)) = fitted_text(&best_line, 13., hud.best_room) {
        draw_text(&best_line, hud_x, hud.best_y, size, best_color);
    }
    right_text("BALLS", balls_right, 22., 13., BRASS);
    for i in 0..3_u8 {
        let x = balls_right - 6. - f32::from(2 - i) * 17.;
        let lit = i < table.lives;
        draw_circle(x, 40., 6., if lit { CREAM } else { PANEL });
        draw_circle_lines(x, 40., 6., 1., if lit { BRASS } else { MINT });
        if lit {
            draw_circle(x - 1.5, 38., 2., WHITE);
        }
    }
    let mult = format!("MULT {}X", table.multiplier);
    right_text(
        &mult,
        hud.mult_right,
        hud.mult_y,
        13.,
        if table.multiplier > 1 { GOLD } else { MINT },
    );
    draw_rectangle(
        reset.x,
        reset.y,
        reset.w,
        reset.h,
        Color::new(0.1, 0.15, 0.14, 1.),
    );
    draw_rectangle_lines(reset.x, reset.y, reset.w, reset.h, 1., MINT);
    centered_in(
        "RESET",
        vec2(reset.x + reset.w * 0.5, reset.y + 27.),
        14.,
        reset.w - 8.,
        MINT,
    );
}

/// The idle objective: what the player should do next, in priority order.
pub(super) fn objective(table: &Table, state: SessionState) -> (String, &'static str) {
    if state == SessionState::Ready {
        ("INSERT HARD HAT".into(), "FREE PLAY  /  3 BALLS")
    } else if table.over() {
        ("SHIFT OVER".into(), "FOREMAN IS WRITING YOUR REVIEW")
    } else if state == SessionState::Paused {
        ("PAUSED".into(), "THE CRANE IS ON BREAK TOO")
    } else if table.crane_hold > 0. {
        ("MAGNET ENGAGED".into(), "HOLD ON TO YOUR HARD HAT")
    } else if table.waiting {
        ("SKILL SHOT IS LIT".into(), "HIT THE LIT BUMPER FIRST")
    } else if table.ball_save > 0. {
        (
            format!("BALL SAVE  {}", table.ball_save.ceil() as u32),
            "DRAINS ARE FREE. FOR NOW.",
        )
    } else if table.crane_armed {
        (
            format!(
                "CRANE ARMED  {}",
                quips::grouped(crate::JACKPOT_POINTS * table.multiplier)
            ),
            "SHOOT UNDER THE MAGNET",
        )
    } else {
        let lit = table.lamps.iter().filter(|l| **l).count();
        (
            format!("LIGHT THE BUMPERS  {lit}/3"),
            "ALL THREE ARM THE CRANE",
        )
    }
}

// The DMD strip: one message, amber on a dotted panel.
/// The launch prompt while a ball waits; otherwise a flipper lesson that
/// fades out once the player has flipped a few times.
pub(super) fn footer_hint(y: f32, table: &Table, view: &Presentation, hints: &Hints) {
    let width = (screen_width() - 24.).clamp(1., HUD_MAX);
    let center = vec2(screen_width() * 0.5, y);
    if table.waiting {
        let (long, short) = if hints.touch {
            ("TAP THE TABLE TO LAUNCH".to_owned(), String::new())
        } else {
            (
                format!("{} TO LAUNCH  /  RELEASE FIRST", hints.launch),
                format!("{} TO LAUNCH", hints.launch),
            )
        };
        let short = readable_label(&short, "RELEASE / LAUNCH: SEE CONTROLS", 13., width);
        centered_in(
            readable_label(&long, short, 13., width),
            center,
            13.,
            width,
            CREAM,
        );
    } else if view.learned.flips < FLIPS_TO_LEARN {
        let fade = 1. - view.learned.flips as f32 / FLIPS_TO_LEARN as f32;
        let text = if hints.touch {
            "HOLD LOWER LEFT / RIGHT TO FLIP".to_owned()
        } else {
            format!("FLIPPERS  {}  AND  {}", hints.left, hints.right)
        };
        centered_in(
            readable_label(&text, "FLIPPERS: SEE CONTROLS", 13., width),
            center,
            13.,
            width,
            Color::new(MINT.r, MINT.g, MINT.b, 0.35 + 0.65 * fade),
        );
    }
}

/// A card centered on the table, clamped to the canvas width.
fn card(center: Vec2, height: f32, accent: Color) -> (Rect, f32) {
    let width = (screen_width() - 24.).clamp(1., 460.);
    let top = (center.y - height * 0.5).max(hud_layout(screen_width()).header + 4.);
    let rect = Rect::new(center.x - width * 0.5, top, width, height);
    use ferravex_render::shapes::{self, Shadow};
    shapes::drop_shadow(
        rect,
        16.,
        Shadow::soft(Color::new(0., 0., 0., 0.45))
            .offset(vec2(0., 7.))
            .blur(18.),
    );
    shapes::fill_rounded_rect(rect, 16., CARD);
    shapes::fill_rounded_rect(
        Rect::new(rect.x + 16., rect.y, rect.w - 32., 4.),
        2.,
        accent,
    );
    shapes::stroke_rounded_rect(rect, 16., 1., BRASS);
    (rect, width - 28.)
}

fn action_bar(rect: Rect, y: f32, text: &str, inner: f32) {
    let bar = Rect::new(rect.x + 14., y - 19., rect.w - 28., 28.);
    draw_rectangle(
        bar.x,
        bar.y,
        bar.w,
        bar.h,
        Color::new(0.75, 0.62, 0.40, 0.18),
    );
    draw_rectangle_lines(bar.x, bar.y, bar.w, bar.h, 1., BRASS);
    centered_in(
        readable_label(text, "BINDINGS: SEE CONTROLS", 13., inner - 8.),
        vec2(rect.x + rect.w * 0.5, y),
        13.,
        inner - 8.,
        CREAM,
    );
}

pub(super) fn ready(center: Vec2, best: Option<u32>, hints: &Hints, seed: u32) {
    let (rect, inner) = card(center, 196., BRASS);
    let mid = rect.x + rect.w * 0.5;
    let mut y = rect.y + 36.;
    centered_in("READY FOR YOUR SHIFT?", vec2(mid, y), 21., inner, CREAM);
    y += 22.;
    centered_in(quips::ready(seed), vec2(mid, y), 13., inner, BRASS);
    y += 30.;
    for line in READY_GOALS {
        centered_in(line, vec2(mid, y), 13., inner, MINT);
        y += 19.;
    }
    y += 6.;
    let best = match best {
        Some(best) => format!("PERSONAL BEST  {}", quips::grouped(best)),
        None => "NO RECORD YET. SET THE BAR.".to_owned(),
    };
    centered_in(&best, vec2(mid, y), 13., inner, GOLD);
    let action = if hints.touch {
        "TAP TO START AND LAUNCH".to_owned()
    } else {
        format!("{} TO START AND LAUNCH", hints.launch)
    };
    action_bar(rect, rect.y + rect.h - 18., &action, inner);
}

pub(super) fn paused(center: Vec2, input: &PlayerInput, hints: &Hints, seed: u32) {
    let (rect, inner) = card(center, 128., MINT);
    let mid = rect.x + rect.w * 0.5;
    centered_in("TABLE PAUSED", vec2(mid, rect.y + 36.), 21., inner, CREAM);
    centered_in(
        quips::pause(seed),
        vec2(mid, rect.y + 60.),
        13.,
        inner,
        BRASS,
    );
    let action = if input.controls_open() {
        "CLOSE CONTROLS TO RETURN".to_owned()
    } else if input.paused() {
        format!("{} RESUME  /  {} RESTART", hints.pause, hints.restart)
    } else {
        "RETURN FOCUS TO THE TABLE".to_owned()
    };
    action_bar(rect, rect.y + rect.h - 18., &action, inner);
}

pub(super) fn results(center: Vec2, table: &Table, results: Results, hints: &Hints) {
    let rank = quips::rank(table.score, table.overtime);
    let (rect, inner) = card(center, RESULTS_HEIGHT, GOLD);
    let mid = rect.x + rect.w * 0.5;
    let mut y = rect.y + 28.;
    centered_in("SHIFT COMPLETE", vec2(mid, y), 13., inner, BRASS);
    y += 38.;
    centered_in(
        &quips::grouped(table.score),
        vec2(mid, y),
        34.,
        inner,
        CREAM,
    );
    y += 32.;
    centered_in(rank.title, vec2(mid, y), 20., inner, GOLD);
    y += 20.;
    centered_in(rank.quip, vec2(mid, y), 13., inner, MINT);
    y += 28.;
    let stats = format!(
        "JACKPOTS {} / BEST CHAIN {} / SKILL SHOTS {}",
        table.jackpots, table.best_chain, table.skill_shots
    );
    let short = format!("JACKPOTS {}  /  CHAIN {}", table.jackpots, table.best_chain);
    centered_in(
        readable_label(&stats, &short, 13., inner),
        vec2(mid, y),
        13.,
        inner,
        CREAM,
    );
    y += 24.;
    let (record, record_color) = if results.demo {
        ("DEMO SHIFT  /  NOT RECORDED".to_owned(), MINT)
    } else if results.new_best(table.score) {
        match results.previous_best {
            Some(best) if best > 0 => (
                format!(
                    "NEW PERSONAL BEST!  +{}",
                    quips::grouped(table.score - best)
                ),
                GOLD,
            ),
            _ => ("FIRST RECORD ON THE BOARD!".to_owned(), GOLD),
        }
    } else {
        let best = results.previous_best.unwrap_or(0);
        (
            format!(
                "BEST {}  /  {} SHORT",
                quips::grouped(best),
                quips::grouped(best.saturating_sub(table.score).max(1))
            ),
            BRASS,
        )
    };
    centered_in(&record, vec2(mid, y), 13., inner, record_color);
    y += 20.;
    if let Some((title, floor)) = rank.next {
        let goal = format!(
            "{} MORE FOR {title}",
            quips::grouped(floor.saturating_sub(table.score))
        );
        centered_in(&goal, vec2(mid, y), 13., inner, MINT);
    } else {
        centered_in(
            "TOP RANK. THE YARD IS YOURS.",
            vec2(mid, y),
            13.,
            inner,
            MINT,
        );
    }
    let action = if hints.touch {
        "TAP TO CLOCK IN AGAIN".to_owned()
    } else {
        format!("{} / {} TO CLOCK IN AGAIN", hints.restart, hints.confirm)
    };
    action_bar(rect, rect.y + rect.h - 18., &action, inner);
}

/// Reused text storage for the table and dot matrix display.
pub(super) struct Buffers {
    pub pop: String,
    pub head: String,
    pub sub: String,
}
impl Buffers {
    pub fn new() -> Self {
        Self {
            pop: String::with_capacity(64),
            head: String::with_capacity(128),
            sub: String::with_capacity(128),
        }
    }
}

pub(super) fn side_panels(
    layout: &layout::Layout,
    table: &Table,
    view: &Presentation,
    best: Option<u32>,
    hints: &Hints,
) {
    use ferravex_render::{
        shapes,
        text::{self, TextStyle},
    };
    for rect in [
        layout.left,
        Rect::new(
            layout.right.x,
            layout.dmd.y + layout.dmd.h + 16.,
            layout.right.w,
            layout.right.h - layout.dmd.h - 16.,
        ),
    ] {
        shapes::fill_rounded_rect(rect, 18., PANEL);
        shapes::stroke_rounded_rect(rect, 18., 1., with_alpha(BRASS, 0.5));
    }
    let x = layout.left.x + 20.;
    let y = layout.left.y + 34.;
    for (label, offset, size, color) in [
        ("Salvage Pinball", 0., 24., CREAM),
        ("Your shift", 38., 16., MINT),
        ("Score", 100., 16., BRASS),
    ] {
        text::draw_text_styled(label, x, y + offset, TextStyle::modern(size, color));
    }
    text::draw_text_styled(
        &quips::grouped(view.shown_score.round() as u32),
        x,
        y + 144.,
        TextStyle::classic(32., CREAM),
    );
    text::draw_text_styled(
        &best.map_or_else(
            || "First shift? Make it count.".into(),
            |v| format!("Best {}", quips::grouped(v)),
        ),
        x,
        y + 180.,
        TextStyle::modern(16., MINT),
    );
    text::draw_text_styled(
        &format!(
            "{} balls left · {}× multiplier",
            table.lives, table.multiplier
        ),
        x,
        y + 220.,
        TextStyle::modern(16., GOLD),
    );
    let reset = layout.reset;
    shapes::fill_rounded_rect(
        reset,
        10.,
        Color::new(0.1 + view.reset_hover * 0.05, 0.15, 0.14, 1.),
    );
    centered_in(
        "Reset",
        reset.center() + vec2(0., 6.),
        16.,
        reset.w - 12.,
        MINT,
    );
    let x = layout.right.x + 18.;
    let y = layout.dmd.y + layout.dmd.h + 40.;
    text::draw_text_styled("Work orders", x, y, TextStyle::modern(24., CREAM));
    for (i, line) in [
        "Light all three bumpers",
        "Catch a crane jackpot",
        "Try the lit skill shot",
        "Keep the ball alive",
    ]
    .into_iter()
    .enumerate()
    {
        text::draw_text_styled(
            line,
            x,
            y + 38. + i as f32 * 30.,
            TextStyle::modern(16., MINT),
        );
    }
    text::draw_text_styled(
        &format!("{} / {} to flip", hints.left, hints.right),
        x,
        (y + 228.).min(layout.right.y + layout.right.h - 20.),
        TextStyle::modern(16., BRASS),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_celebrate_only_real_improvements() {
        let first = Results::default();
        assert!(first.new_best(100));
        assert!(!first.new_best(0), "an empty game is not a record");
        let chase = Results {
            previous_best: Some(5_000),
            demo: false,
        };
        assert!(!chase.new_best(5_000) && chase.new_best(5_001));
        let demo = Results {
            previous_best: None,
            demo: true,
        };
        assert!(!demo.new_best(1_000_000));
    }

    #[test]
    fn objective_names_the_next_step_in_priority_order() {
        let mut table = Table::new();
        let objective = |table: &Table| objective(table, SessionState::Playing);
        assert_eq!(
            super::objective(&table, SessionState::Ready).0,
            "INSERT HARD HAT"
        );
        assert_eq!(super::objective(&table, SessionState::Paused).0, "PAUSED");
        assert!(objective(&table).0.starts_with("SKILL SHOT"));
        table.waiting = false;
        table.ball_save = 3.2;
        assert_eq!(objective(&table).0, "BALL SAVE  4");
        table.ball_save = 0.;
        table.lamps = [true, false, true];
        assert_eq!(objective(&table).0, "LIGHT THE BUMPERS  2/3");
        table.crane_armed = true;
        table.multiplier = 2;
        assert_eq!(objective(&table).0, "CRANE ARMED  5,000");
        table.crane_hold = 0.5;
        assert_eq!(objective(&table).0, "MAGNET ENGAGED");
        table.lives = 0;
        assert_eq!(objective(&table).0, "SHIFT OVER");
    }

    #[test]
    fn every_card_and_dmd_line_fits_readably_at_the_narrowest_supported_width() {
        // 320px canvas: 296px card with 268px of text; 296px DMD with 276px.
        let inner = (320_f32 - 24.).clamp(1., 460.) - 28.;
        let mut lines: Vec<String> = vec![
            "NO RECORD YET. SET THE BAR.".into(),
            "FIRST RECORD ON THE BOARD!".into(),
            "TOP RANK. THE YARD IS YOURS.".into(),
            "DEMO SHIFT  /  NOT RECORDED".into(),
            "NEW PERSONAL BEST!  +12,300".into(),
            "BEST 1,234,567  /  1,200 SHORT".into(),
            "4,100 MORE FOR MAGNET WRANGLER".into(),
            "JACKPOTS 12  /  CHAIN 14".into(),
            "PERSONAL BEST  1,234,567".into(),
            "HOLD ON TO YOUR HARD HAT".into(),
            "HIT THE LIT BUMPER FIRST".into(),
            "DRAINS ARE FREE. FOR NOW.".into(),
            "SHOOT UNDER THE MAGNET".into(),
            "ALL THREE ARM THE CRANE".into(),
            "CHAIN BONUS ON EVERY HIT".into(),
            "THE FOREMAN NODS. SLOWLY.".into(),
            "SECRET: 45S ON ONE BALL".into(),
            "FOREMAN IS WRITING YOUR REVIEW".into(),
            "THE CRANE IS ON BREAK TOO".into(),
            "FREE PLAY  /  3 BALLS".into(),
        ];
        lines.extend(READY_GOALS.map(String::from));
        lines.extend(quips::all_lines().map(String::from));
        for line in &lines {
            let (size, _) = fitted_text(line, 13., inner).unwrap();
            assert!(size >= 11.5, "{line} shrinks to {size}");
        }
        let heads = [
            "THE YARD IS SHAKING!  x7",
            "ABSOLUTE CARNAGE!  x12",
            "JACKPOT  +12,500",
            "OVERTIME PAY  +5,000",
            "SKILL SHOT  +1,000",
            "DRAINED  /  2 LEFT",
            "LIGHT THE BUMPERS  2/3",
            "CRANE ARMED  12,500",
            "SKILL SHOT IS LIT",
        ];
        for head in heads {
            let (size, _) = fitted_text(head, 16., inner).unwrap();
            assert!(size >= 13., "{head} shrinks to {size}");
        }
        for title in ["READY FOR YOUR SHIFT?", "TABLE PAUSED", "OVERTIME LEGEND"] {
            assert!(fitted_text(title, 21., inner).unwrap().0 >= 17.);
        }
    }
}
