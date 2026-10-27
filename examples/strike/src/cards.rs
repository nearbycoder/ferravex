//! Briefing/pause info cards and the end-of-match results screen.
//!
//! Layout is computed by pure emitters (like the menu's) so tests can check
//! every block for overlap, clipping and readable sizes at many viewports.
use crate::controls::hint;
use crate::crew::{self, Award, Rank};
#[cfg(test)]
use crate::juice::measure;
use crate::juice::{GOLD, INK, MUTED, TEAL, fit_size};
use crate::scoreboard::{MATCH_SECONDS, Summary, Tracker};
use ferravex_render::input::PlayerInput;
use ferravex_render::prelude::*;
use ferravex_render::ui::{Align, ButtonStyle, TextStyle, Ui, UiRect};
use std::fmt::Write as _;

const CARD: Color = Color::new(0.035, 0.055, 0.05, 0.93);
const CELL: Color = Color::new(0.09, 0.13, 0.12, 0.95);
/// Records context: match length and frag scoring rules, version 1.
pub(super) const RECORD_RULES: u64 = 0x5354_524b_0000 | MATCH_SECONDS as u64;
pub(super) const RECORD_ID: &str = "gauntlet";

/// A finished practice match, captured once at the final authority tick.
#[derive(Clone, Debug)]
pub(super) struct Results {
    pub rank: Rank,
    pub new_best: bool,
    header: String,
    best_line: String,
    next_line: String,
    stats: [(String, &'static str); 6],
    awards: [Option<(&'static str, String)>; 3],
}

impl Results {
    /// `previous_best` is the stored best before this match; `recorded` is
    /// false for automation runs, which never claim a personal best.
    pub fn new(tracker: &Tracker, previous_best: Option<u64>, recorded: bool) -> Self {
        let s = tracker.summary;
        let crew: u32 = tracker
            .lines
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != usize::from(tracker.local()))
            .map(|(_, l)| l.kills)
            .sum();
        let frags = u64::from(s.frags);
        let new_best = recorded && previous_best.is_none_or(|best| frags > best);
        let mut best_line = String::new();
        let _ = match previous_best {
            _ if !recorded => write!(best_line, "PRACTICE RUN · NOT RECORDED"),
            None => write!(best_line, "FIRST RECORD SET: {frags} FRAGS"),
            Some(best) if frags > best => {
                write!(
                    best_line,
                    "NEW PERSONAL BEST! +{} OVER {best}",
                    frags - best
                )
            }
            Some(best) if frags == best => write!(best_line, "TIED YOUR BEST OF {best}. SO CLOSE."),
            Some(best) => write!(best_line, "BEST {best} · {} SHORT", best - frags),
        };
        let mut next_line = String::new();
        let _ = match crew::next_rank(s.frags) {
            Some((title, more)) => write!(next_line, "{more} MORE FOR {title}"),
            None => write!(next_line, "TOP RANK. THE KRAKEN SALUTES YOU."),
        };
        let tenths = (s.frags * 10).checked_div(s.deaths).unwrap_or(s.frags * 10);
        let kd = format!("{}.{}", tenths / 10, tenths % 10);
        let stats = [
            (s.frags.to_string(), "FRAGS"),
            (s.deaths.to_string(), "DEATHS"),
            (kd, "K/D"),
            (format!("{}%", s.accuracy().min(100)), "ACCURACY"),
            (s.headshots.to_string(), "HEADSHOTS"),
            (s.best_streak.to_string(), "STREAK"),
        ];
        let awards = crew::awards::<3>(&s).map(|award| {
            award.map(|a: Award| {
                let mut detail = String::new();
                let _ = a.detail(&mut detail);
                (a.title(), detail)
            })
        });
        let header = format!(
            "{} YOU {} — {crew} CREW",
            if new_best { "RECORD!" } else { "FINAL:" },
            s.frags
        );
        Self {
            rank: crew::rank(&s),
            new_best,
            header,
            best_line,
            next_line,
            stats,
            awards,
        }
    }
}

/// One positioned block of a card.
pub(super) enum Item<'a> {
    Panel(UiRect, Color),
    Text(UiRect, &'a str, f32, Color, Align),
    Button(u64, UiRect, &'static str, f32, bool),
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> UiRect {
    UiRect {
        min: [x, y],
        size: [w, h],
    }
}

/// Lines `text` needs at `size` when greedily word-wrapped into `width`.
pub(super) fn wrap_lines(text: &str, size: f32, width: f32) -> usize {
    let columns = ((width / (6. * size / 8.)).floor() as usize).max(1);
    let mut lines = 1;
    let mut used = 0;
    for word in text.split(' ') {
        let n = word.chars().count();
        if used == 0 {
            used = n;
        } else if used + 1 + n <= columns {
            used += 1 + n;
        } else {
            lines += 1;
            used = n;
        }
        while used > columns {
            lines += 1;
            used -= columns;
        }
    }
    lines
}

/// A readable size for `text`: shrink to fit one line down to 13px, else
/// wrap at 13px. Returns (size, lines).
fn block(text: &str, size: f32, width: f32) -> (f32, usize) {
    let fitted = fit_size(text, size, width);
    if fitted >= 13. {
        (fitted, 1)
    } else {
        (13., wrap_lines(text, 13., width))
    }
}

/// Results layout. `bottom` is the last usable row (above any replay footer).
pub(super) fn results_items<'a>(
    w: f32,
    h: f32,
    bottom: f32,
    r: &'a Results,
    mut emit: impl FnMut(Item<'a>),
) {
    let corner = crate::hud::MENU_CORNER;
    let mut width = (w - 24.).min(580.);
    // Clear the fullscreen menu corner by narrowing a centered card when it
    // stays roomy; very narrow views move the card below the corner instead.
    let narrowed = w - 2. * (corner[0] + 4.);
    if (w + width) * 0.5 > w - corner[0] && narrowed >= 300. {
        width = narrowed;
    }
    let left = (w - width) * 0.5;
    let floor = if left + width > w - corner[0] {
        corner[1] + 4.
    } else {
        12.
    };
    let avail = bottom.min(h) - floor;
    // Measure first, then drop award rows until the card fits.
    let mut shown = r.awards.iter().flatten().count();
    let mut height = results_layout(left, 0., width, r, shown, &mut |_| {});
    while shown > 0 && height > avail {
        shown -= 1;
        height = results_layout(left, 0., width, r, shown, &mut |_| {});
    }
    let top = (floor + (avail - height) * 0.42).max(floor);
    // A card reaching the right edge starts below the fullscreen menu corner.
    let corner = crate::hud::MENU_CORNER;
    let top = if left + width > w - corner[0] {
        top.max(corner[1] + 4.)
    } else {
        top
    };
    results_layout(left, top, width, r, shown, &mut emit);
}

fn results_layout<'a>(
    left: f32,
    top: f32,
    width: f32,
    r: &'a Results,
    shown: usize,
    emit: &mut impl FnMut(Item<'a>),
) -> f32 {
    let inner = width - 32.;
    let x = left + 16.;
    let mut items: [Option<Item<'a>>; 40] = std::array::from_fn(|_| None);
    let mut count = 0;
    let mut push = |item: Item<'a>| {
        if let Some(slot) = items.get_mut(count) {
            *slot = Some(item);
            count += 1;
        }
    };
    let mut y = top + 16.;
    let mut text =
        |y: &mut f32, value: &'a str, size: f32, color: Color, align: Align, gap: f32| {
            let (size, lines) = block(value, size, inner);
            push(Item::Text(
                rect(x, *y, inner, size * lines as f32),
                value,
                size,
                color,
                align,
            ));
            *y += size * lines as f32 + gap;
        };
    text(
        &mut y,
        &r.header,
        14.,
        if r.new_best { GOLD } else { MUTED },
        Align::Center,
        10.,
    );
    let title = fit_size(r.rank.title, 34., inner);
    let title_rect = rect(x, y, inner, title);
    y += title + 8.;
    text(&mut y, r.rank.quip, 14., INK, Align::Center, 12.);
    let stats_top = y;
    let gap = 8.;
    let columns = if (inner - gap * 2.) / 3. >= 96. { 3 } else { 2 };
    let cell = (inner - gap * (columns - 1) as f32) / columns as f32;
    y += (6 / columns) as f32 * 52. + 6.;
    text(
        &mut y,
        &r.best_line,
        16.,
        if r.new_best { GOLD } else { INK },
        Align::Center,
        6.,
    );
    text(&mut y, &r.next_line, 14., TEAL, Align::Center, 6.);
    if shown > 0 {
        y += 4.;
        text(&mut y, "AWARDS", 13., MUTED, Align::Left, 6.);
        for (title, detail) in r.awards.iter().flatten().take(shown) {
            text(&mut y, title, 14., GOLD, Align::Left, 4.);
            text(&mut y, detail, 13., INK, Align::Left, 8.);
        }
    }
    y += 10.;
    let height = y + 44. + 16. - top;
    emit(Item::Panel(rect(left, top, width, height), CARD));
    emit(Item::Text(
        title_rect,
        r.rank.title,
        title,
        GOLD,
        Align::Center,
    ));
    for (i, (value, label)) in r.stats.iter().enumerate() {
        let cx = x + (i % columns) as f32 * (cell + gap);
        let cy = stats_top + (i / columns) as f32 * 52.;
        emit(Item::Panel(rect(cx, cy, cell, 46.), CELL));
        let size = fit_size(value, 22., cell - 12.);
        let color = if i == 0 { GOLD } else { INK };
        emit(Item::Text(
            rect(cx + 6., cy + 5., cell - 12., size),
            value,
            size,
            color,
            Align::Center,
        ));
        let size = fit_size(label, 13., cell - 12.);
        emit(Item::Text(
            rect(cx + 6., cy + 30., cell - 12., size),
            label,
            size,
            MUTED,
            Align::Center,
        ));
    }
    for item in items.into_iter().flatten() {
        emit(item);
    }
    let button = (inner - gap) * 0.5;
    emit(Item::Button(
        2,
        rect(x, y, button, 44.),
        "Play again",
        18.,
        true,
    ));
    emit(Item::Button(
        3,
        rect(x + button + gap, y, button, 44.),
        "Briefing",
        16.,
        false,
    ));
    height
}

fn paint(ui: &mut Ui, item: Item<'_>) {
    match item {
        Item::Panel(r, color) => {
            let _ = ui.panel(r, color);
        }
        Item::Text(r, text, size, color, align) => {
            let _ = ui.text(
                r,
                text,
                TextStyle {
                    size,
                    color,
                    align,
                    shadow: false,
                    wrap: true,
                },
            );
        }
        Item::Button(id, r, text, size, primary) => {
            let _ = ui.button(
                id,
                r,
                text,
                true,
                ButtonStyle {
                    fill: if primary {
                        crate::geometry::rgb(0xd7b56d)
                    } else {
                        crate::geometry::rgb(0x263b35)
                    },
                    ink: if primary {
                        crate::geometry::rgb(0x17251f)
                    } else {
                        INK
                    },
                    focus: INK,
                    size,
                    ..Default::default()
                },
            );
        }
    }
}

/// The results screen owns the whole view; the replay footer is hidden.
pub(super) fn results(ui: &mut Ui, r: &Results) {
    let (w, h) = (screen_width(), screen_height());
    let _ = ui.panel(rect(0., 0., w, h), Color::new(0.02, 0.035, 0.03, 0.55));
    results_items(w, h, h - 12., r, |item| paint(ui, item));
}

/// Short info rows under the briefing or pause menu. `menu_bottom` is the
/// lowest menu block; rows are dropped rather than crowding the footer band.
pub(super) fn info_rows<'a>(
    w: f32,
    h: f32,
    menu_left: f32,
    menu_bottom: f32,
    rows: &[(&'a str, Color)],
    mut emit: impl FnMut(Item<'a>),
) {
    let limit = h - 150.;
    let left = menu_left.max(12.);
    let width = (w - left - 12.).min(440.);
    let inner = width - 24.;
    if width < 120. {
        return;
    }
    let top = menu_bottom + 14.;
    let mut height = 10.;
    let mut fit = 0;
    for (text, _) in rows {
        let (size, lines) = block(text, 14., inner);
        let next = height + size * lines as f32 + 8.;
        if top + next > limit {
            break;
        }
        height = next;
        fit += 1;
    }
    if fit == 0 {
        return;
    }
    emit(Item::Panel(
        rect(left, top, width, height),
        Color::new(0.03, 0.05, 0.045, 0.82),
    ));
    let mut y = top + 9.;
    for (text, color) in rows.iter().take(fit) {
        let (size, lines) = block(text, 14., inner);
        emit(Item::Text(
            rect(left + 12., y, inner, size * lines as f32),
            text,
            size,
            *color,
            Align::Left,
        ));
        y += size * lines as f32 + 8.;
    }
}

/// Rows for the opening briefing.
pub(super) fn briefing_rows(best: Option<(u64, u64)>, input: &PlayerInput, out: &mut [String; 3]) {
    for s in out.iter_mut() {
        s.clear();
    }
    let _ = write!(out[0], "2:00 GAUNTLET VS THE CREW");
    let _ = match best {
        Some((frags, runs)) => {
            let title = crew::rank(&Summary {
                frags: frags as u32,
                deaths: 1,
                shots: 1,
                ..Default::default()
            })
            .title;
            write!(out[1], "YOUR BEST: {frags} FRAGS · {title} · {runs} RUNS")
        }
        None => write!(out[1], "NO RECORD YET. SET THE BAR."),
    };
    let mut fire = [0; 64];
    let mut roster = [0; 64];
    let fire = hint(input, "fire", &mut fire);
    let roster = hint(input, "roster", &mut roster);
    let _ = write!(out[2], "{fire} FIRE · HOLD {roster} FOR SCORES");
}

/// Rows for the paused practice menu.
pub(super) fn pause_rows(tracker: &Tracker, best: Option<u64>, seed: u32, out: &mut [String; 3]) {
    for s in out.iter_mut() {
        s.clear();
    }
    let seconds = tracker.remaining_seconds().unwrap_or(0);
    let crew: u32 = tracker
        .lines
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != usize::from(tracker.local()))
        .map(|(_, l)| l.kills)
        .sum();
    let _ = write!(
        out[0],
        "{}:{:02} LEFT · YOU {} · CREW {crew}",
        seconds / 60,
        seconds % 60,
        tracker.summary.frags
    );
    let _ = write!(out[1], "{}", crew::pause(seed));
    let _ = match best {
        Some(best) if u64::from(tracker.summary.frags) > best => {
            write!(out[2], "ON PACE FOR A NEW BEST ({best})")
        }
        Some(best) => write!(
            out[2],
            "BEST {best} · {} TO BEAT IT",
            best + 1 - u64::from(tracker.summary.frags)
        ),
        None => write!(out[2], "NO RECORD YET"),
    };
}

pub(super) fn info(
    ui: &mut Ui,
    menu_left: f32,
    menu_bottom: f32,
    rows: &[String; 3],
    colors: [Color; 3],
) {
    let rows = [
        (rows[0].as_str(), colors[0]),
        (rows[1].as_str(), colors[1]),
        (rows[2].as_str(), colors[2]),
    ];
    let rows: Vec<_> = rows.into_iter().filter(|(s, _)| !s.is_empty()).collect();
    info_rows(
        screen_width(),
        screen_height(),
        menu_left,
        menu_bottom,
        &rows,
        |item| paint(ui, item),
    );
}

pub(super) const BRIEFING_COLORS: [Color; 3] = [GOLD, INK, MUTED];
pub(super) const PAUSE_COLORS: [Color; 3] = [INK, MUTED, GOLD];

#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_fps::GameEvent;

    fn tracker(frags: u32) -> Tracker {
        let mut t = Tracker::new(0, true);
        for n in 0..frags {
            let victim = 1 + (n % 5) as u8;
            t.observe(
                n * 400 + 1,
                &[
                    GameEvent::Shot {
                        player: 0,
                        origin: Vec3::new(0., 1.6, 0.),
                        end: Vec3::new(0., 1.6, -3.),
                    },
                    GameEvent::Hit {
                        attacker: 0,
                        victim,
                        damage: 100,
                        headshot: n % 2 == 0,
                    },
                    GameEvent::Killed {
                        attacker: 0,
                        victim,
                    },
                ],
                |_| {},
            );
        }
        t.observe(
            9000,
            &[GameEvent::Killed {
                attacker: 3,
                victim: 0,
            }],
            |_| {},
        );
        t
    }

    #[test]
    fn personal_best_lines_and_next_goal_hooks() {
        let t = tracker(9);
        let first = Results::new(&t, None, true);
        assert!(first.new_best);
        assert_eq!(first.best_line, "FIRST RECORD SET: 9 FRAGS");
        assert_eq!(first.next_line, "3 MORE FOR DOCK BOSS");
        assert_eq!(first.header, "RECORD! YOU 9 — 1 CREW");
        let short = Results::new(&t, Some(12), true);
        assert!(!short.new_best);
        assert_eq!(short.best_line, "BEST 12 · 3 SHORT");
        assert_eq!(short.header, "FINAL: YOU 9 — 1 CREW");
        let beat = Results::new(&t, Some(7), true);
        assert!(beat.new_best);
        assert_eq!(beat.best_line, "NEW PERSONAL BEST! +2 OVER 7");
        let smoke = Results::new(&t, Some(1), false);
        assert!(!smoke.new_best, "automation never claims a record");
        assert_eq!(smoke.stats[2].0, "9.0");
        assert_eq!(smoke.stats[3].0, "100%");
    }

    #[test]
    fn results_and_info_blocks_fit_without_overlap_at_every_viewport() {
        for frags in [0, 9, 25] {
            let r = Results::new(&tracker(frags), Some(10), true);
            for (w, h) in [
                (320., 480.),
                (390., 608.),
                (640., 360.),
                (1280., 576.),
                (1280., 800.),
                (1600., 1000.),
            ] {
                for footer in [false, true] {
                    let bottom = if footer { h - 150. } else { h - 12. };
                    let mut blocks: Vec<UiRect> = Vec::new();
                    let mut buttons = 0;
                    let mut panel: Option<UiRect> = None;
                    results_items(w, h, bottom, &r, |item| match item {
                        Item::Panel(rect, _) => {
                            assert!(rect.min[0] >= 0. && rect.min[0] + rect.size[0] <= w + 0.01);
                            if panel.is_none() {
                                panel = Some(rect);
                            }
                        }
                        Item::Text(rect, text, size, ..) => {
                            assert!(size >= 12., "{text} at {size}");
                            let lines = wrap_lines(text, size, rect.size[0]);
                            assert!(
                                (lines == 1 && measure(text, size) <= rect.size[0] + 0.01)
                                    || (lines > 1 && size >= 13.),
                                "{w}x{h} {text}"
                            );
                            assert!(lines as f32 * size <= rect.size[1] + 0.01, "{text} lines");
                            let p = panel.unwrap();
                            assert!(
                                rect.min[1] >= p.min[1]
                                    && rect.min[1] + rect.size[1] <= p.min[1] + p.size[1] + 0.01,
                                "{text} outside card"
                            );
                            for b in &blocks {
                                assert!(rect.intersect(*b).is_none(), "{w}x{h} {text} overlaps");
                            }
                            blocks.push(rect);
                        }
                        Item::Button(_, rect, ..) => {
                            buttons += 1;
                            assert!(rect.size[1] >= 44.);
                            assert!(
                                rect.min[1] + rect.size[1] <= h - 8.,
                                "{w}x{h} button clipped"
                            );
                            for b in &blocks {
                                assert!(rect.intersect(*b).is_none(), "button overlaps");
                            }
                            blocks.push(rect);
                        }
                    });
                    assert_eq!(buttons, 2);
                }
            }
        }
        let rows = [("A ROW", INK), ("ANOTHER ROW", INK), ("THIRD", INK)];
        for (w, h) in [(320., 480.), (1280., 576.), (1440., 900.)] {
            let mut last = 0.;
            info_rows(w, h, 40., 250., &rows, |item| {
                if let Item::Panel(r, _) | Item::Text(r, ..) = item {
                    assert!(r.min[1] > 250. && r.min[1] + r.size[1] <= h - 150. + 0.01);
                    last = r.min[1] + r.size[1];
                }
            });
            assert!(last <= h - 150. + 0.01);
        }
    }
}
