//! Briefing/pause info cards and the end-of-match results screen.
//!
//! Layout is computed by pure emitters (like the menu's) so tests can check
//! every block for overlap, clipping and readable sizes at many viewports.
//! Copy stays to numbers, names and short lines; pictograms (emitted as
//! [`Item::Glyph`] and painted after the `Ui` pass) replace stat labels.
use crate::controls::hint;
use crate::crew::{self, Award, Rank};
use crate::juice::{GOLD, INK, MUTED, RED, TEAL, fit_size, measure};
use crate::scoreboard::{MATCH_SECONDS, Summary, Tracker};
use ferravex_render::icons::Icon;
use ferravex_render::input::PlayerInput;
use ferravex_render::prelude::*;
use ferravex_render::ui::{Align, ButtonStyle, TextStyle, Ui, UiRect};
use std::fmt::Write as _;

const CARD: Color = Color::new(0.035, 0.055, 0.05, 0.93);
const CELL: Color = Color::new(0.09, 0.13, 0.12, 0.95);
/// Records context: match length and frag scoring rules, version 1.
pub(super) const RECORD_RULES: u64 = 0x5354_524b_0000 | MATCH_SECONDS as u64;
pub(super) const RECORD_ID: &str = "gauntlet";
/// Side of a glyph beside a line of card text.
const GLYPH: f32 = 18.;

/// A pictogram painted over a card after the `Ui` pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Glyph {
    Icon(Icon),
    /// The kill feed's crosshair-in-circle headshot mark.
    Headshot,
}

/// What sits under a results stat: a glyph, or a tiny label where no
/// pictogram says it better ("K/D").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Stat {
    Glyph(Glyph),
    Label(&'static str),
}

/// A finished practice match, captured once at the final authority tick.
#[derive(Clone, Debug)]
pub(super) struct Results {
    pub rank: Rank,
    pub new_best: bool,
    header: String,
    best_line: String,
    next_line: String,
    stats: [(String, Stat); 6],
    awards: [Option<(Award, String)>; 3],
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
            _ if !recorded => write!(best_line, "NOT RECORDED"),
            None => write!(best_line, "FIRST RECORD: {frags}"),
            Some(best) if frags > best => write!(best_line, "NEW BEST! +{}", frags - best),
            Some(best) if frags == best => write!(best_line, "TIED BEST: {best}"),
            Some(best) => write!(best_line, "BEST {best} · {} SHORT", best - frags),
        };
        let mut next_line = String::new();
        let _ = match crew::next_rank(s.frags) {
            Some((title, more)) => write!(next_line, "+{more} → {title}"),
            None => write!(next_line, "TOP RANK!"),
        };
        let tenths = (s.frags * 10).checked_div(s.deaths).unwrap_or(s.frags * 10);
        let kd = format!("{}.{}", tenths / 10, tenths % 10);
        let stats = [
            (s.frags.to_string(), Stat::Glyph(Glyph::Icon(Icon::Target))),
            (s.deaths.to_string(), Stat::Glyph(Glyph::Icon(Icon::Skull))),
            (kd, Stat::Label("K/D")),
            (
                format!("{}%", s.accuracy().min(100)),
                Stat::Glyph(Glyph::Icon(Icon::Eye)),
            ),
            (s.headshots.to_string(), Stat::Glyph(Glyph::Headshot)),
            (
                s.best_streak.to_string(),
                Stat::Glyph(Glyph::Icon(Icon::Flame)),
            ),
        ];
        let awards = crew::awards::<3>(&s).map(|award| {
            award.map(|a: Award| {
                let mut stat = String::new();
                let _ = a.stat(&mut stat);
                (a, stat)
            })
        });
        let header = format!("{} — {crew}", s.frags);
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
    Glyph(UiRect, Glyph, Color),
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
    let mut items: [Option<Item<'a>>; 48] = std::array::from_fn(|_| None);
    let mut count = 0;
    let mut push = |item: Item<'a>| {
        if let Some(slot) = items.get_mut(count) {
            *slot = Some(item);
            count += 1;
        }
    };
    let mut y = top + 16.;
    // The final score, framed by you (left) and the crew (right).
    let score = 20.;
    let tw = measure(&r.header, score);
    let x0 = x + (inner - tw) * 0.5;
    let tone = if r.new_best { GOLD } else { INK };
    push(Item::Glyph(
        rect(x0 - GLYPH - 10., y + 1., GLYPH, GLYPH),
        Glyph::Icon(Icon::Person),
        GOLD,
    ));
    push(Item::Text(
        rect(x0, y, tw, score),
        &r.header,
        score,
        tone,
        Align::Left,
    ));
    push(Item::Glyph(
        rect(x0 + tw + 10., y + 1., GLYPH, GLYPH),
        Glyph::Icon(Icon::People),
        TEAL,
    ));
    y += score + 10.;
    let title = fit_size(r.rank.title, 34., inner);
    let title_rect = rect(x, y, inner, title);
    y += title + 8.;
    // The one joke line on this screen.
    let (size, lines) = block(r.rank.quip, 14., inner);
    push(Item::Text(
        rect(x, y, inner, size * lines as f32),
        r.rank.quip,
        size,
        INK,
        Align::Center,
    ));
    y += size * lines as f32 + 12.;
    let stats_top = y;
    let gap = 8.;
    let columns = if (inner - gap * 2.) / 3. >= 96. { 3 } else { 2 };
    let cell = (inner - gap * (columns - 1) as f32) / columns as f32;
    y += (6 / columns) as f32 * 52. + 6.;
    // A glyph and a short line, centred; wraps without the glyph if needed.
    let mut glyph_line = |y: &mut f32, glyph: Glyph, value: &'a str, size: f32, color: Color| {
        let (size, lines) = block(value, size, inner - GLYPH - 8.);
        if lines == 1 {
            let tw = measure(value, size);
            let x0 = x + (inner - (GLYPH + 8. + tw)) * 0.5;
            push(Item::Glyph(
                rect(x0, *y + (size - GLYPH) * 0.5, GLYPH, GLYPH),
                glyph,
                color,
            ));
            push(Item::Text(
                rect(x0 + GLYPH + 8., *y, tw, size),
                value,
                size,
                color,
                Align::Left,
            ));
        } else {
            push(Item::Text(
                rect(x, *y, inner, size * lines as f32),
                value,
                size,
                color,
                Align::Center,
            ));
        }
        *y += size * lines as f32 + 8.;
    };
    glyph_line(
        &mut y,
        Glyph::Icon(Icon::Trophy),
        &r.best_line,
        16.,
        if r.new_best { GOLD } else { INK },
    );
    glyph_line(&mut y, Glyph::Icon(Icon::ArrowUp), &r.next_line, 14., TEAL);
    // Awards: pictogram, title and its stat on one row when they fit.
    if shown > 0 {
        y += 4.;
    }
    for (award, stat) in r.awards.iter().flatten().take(shown) {
        let name = award.title();
        let stat_w = if stat.is_empty() {
            0.
        } else {
            measure(stat, 13.) + 12.
        };
        let room = inner - GLYPH - 8. - stat_w;
        let size = fit_size(name, 14., room);
        let icon = Glyph::Icon(award.icon());
        if measure(name, size) <= room {
            push(Item::Glyph(
                rect(x, y + (size - GLYPH) * 0.5, GLYPH, GLYPH),
                icon,
                GOLD,
            ));
            push(Item::Text(
                rect(x + GLYPH + 8., y, measure(name, size), size),
                name,
                size,
                GOLD,
                Align::Left,
            ));
            if !stat.is_empty() {
                let sw = measure(stat, 13.);
                push(Item::Text(
                    rect(x + inner - sw, y, sw, 13.),
                    stat,
                    13.,
                    INK,
                    Align::Right,
                ));
            }
            y += size.max(GLYPH - 2.) + 8.;
        } else {
            let (size, lines) = block(name, 14., inner - GLYPH - 8.);
            push(Item::Glyph(rect(x, y, GLYPH, GLYPH), icon, GOLD));
            push(Item::Text(
                rect(x + GLYPH + 8., y, inner - GLYPH - 8., size * lines as f32),
                name,
                size,
                GOLD,
                Align::Left,
            ));
            y += (size * lines as f32).max(GLYPH) + 4.;
            if !stat.is_empty() {
                let (size, lines) = block(stat, 13., inner - GLYPH - 8.);
                push(Item::Text(
                    rect(x + GLYPH + 8., y, inner - GLYPH - 8., size * lines as f32),
                    stat,
                    size,
                    INK,
                    Align::Left,
                ));
                y += size * lines as f32 + 8.;
            }
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
    for (i, (value, stat)) in r.stats.iter().enumerate() {
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
        match *stat {
            Stat::Glyph(glyph) => emit(Item::Glyph(
                rect(cx + cell * 0.5 - 8., cy + 28., 16., 16.),
                glyph,
                match glyph {
                    Glyph::Headshot => RED,
                    Glyph::Icon(Icon::Target) => GOLD,
                    _ => MUTED,
                },
            )),
            Stat::Label(label) => {
                let size = fit_size(label, 13., cell - 12.);
                emit(Item::Text(
                    rect(cx + 6., cy + 30., cell - 12., size),
                    label,
                    size,
                    MUTED,
                    Align::Center,
                ));
            }
        }
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
        // Pictograms paint after the `Ui` pass (see `glyphs`).
        Item::Glyph(..) => {}
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

/// Paint one card pictogram (after `Ui::paint`, so it sits on the card).
fn glyph(item: Item<'_>) {
    if let Item::Glyph(r, glyph, color) = item {
        let center = vec2(r.min[0] + r.size[0] * 0.5, r.min[1] + r.size[1] * 0.5);
        match glyph {
            Glyph::Icon(icon) => {
                crate::style::icon(icon, center, r.size[0], color);
            }
            Glyph::Headshot => crate::style::headshot_icon(center, r.size[0] * 0.38, color),
        }
    }
}

/// The results screen owns the whole view; the replay footer is hidden.
pub(super) fn results(ui: &mut Ui, r: &Results) {
    let (w, h) = (screen_width(), screen_height());
    let _ = ui.panel(rect(0., 0., w, h), Color::new(0.02, 0.035, 0.03, 0.55));
    results_items(w, h, h - 12., r, |item| paint(ui, item));
}

/// The results card's pictograms; call after `Ui::paint`.
pub(super) fn results_glyphs(r: &Results) {
    let (w, h) = (screen_width(), screen_height());
    results_items(w, h, h - 12., r, glyph);
}

/// Colors and leading pictograms for the three info rows.
#[derive(Clone, Copy, Debug)]
pub(super) struct InfoStyle {
    pub colors: [Color; 3],
    pub icons: [Option<Icon>; 3],
}

/// Short info rows under the briefing or pause menu. `menu_bottom` is the
/// lowest menu block; rows are dropped rather than crowding the footer band.
pub(super) fn info_rows<'a>(
    w: f32,
    h: f32,
    menu_left: f32,
    menu_bottom: f32,
    rows: &[(&'a str, Color, Option<Icon>)],
    mut emit: impl FnMut(Item<'a>),
) {
    let limit = h - 150.;
    let left = menu_left.max(12.);
    let width = (w - left - 12.).min(440.);
    let inner = width - 24.;
    if width < 120. {
        return;
    }
    let lead = |icon: &Option<Icon>| if icon.is_some() { GLYPH + 8. } else { 0. };
    let top = menu_bottom + 14.;
    let mut height = 10.;
    let mut fit = 0;
    for (text, _, icon) in rows {
        let (size, lines) = block(text, 14., inner - lead(icon));
        let next = height + (size * lines as f32).max(if icon.is_some() { 16. } else { 0. }) + 8.;
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
    for (text, color, icon) in rows.iter().take(fit) {
        let lead = lead(icon);
        let (size, lines) = block(text, 14., inner - lead);
        if let Some(icon) = icon {
            emit(Item::Glyph(
                rect(left + 12., y + (size - 16.) * 0.5, 16., 16.),
                Glyph::Icon(*icon),
                *color,
            ));
        }
        emit(Item::Text(
            rect(left + 12. + lead, y, inner - lead, size * lines as f32),
            text,
            size,
            *color,
            Align::Left,
        ));
        y += (size * lines as f32).max(if icon.is_some() { 16. } else { 0. }) + 8.;
    }
}

/// Rows for the opening briefing: your record and the two keys that matter.
pub(super) fn briefing_rows(best: Option<(u64, u64)>, input: &PlayerInput, out: &mut [String; 3]) {
    for s in out.iter_mut() {
        s.clear();
    }
    let _ = match best {
        Some((frags, runs)) => {
            let title = crew::rank(&Summary {
                frags: frags as u32,
                deaths: 1,
                shots: 1,
                ..Default::default()
            })
            .title;
            write!(out[0], "BEST {frags} · {title} · {runs} RUNS")
        }
        None => write!(out[0], "NO RECORD YET"),
    };
    let mut fire = [0; 64];
    let mut roster = [0; 64];
    let fire = hint(input, "fire", &mut fire);
    let roster = hint(input, "roster", &mut roster);
    let _ = write!(out[1], "{fire} FIRE · {roster} SCORES");
}

/// Rows for the paused practice menu: clock and score, one quip, the record.
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
        "{}:{:02} · {} — {crew}",
        seconds / 60,
        seconds % 60,
        tracker.summary.frags
    );
    let _ = write!(out[1], "{}", crew::pause(seed));
    let _ = match best {
        Some(best) if u64::from(tracker.summary.frags) > best => {
            write!(out[2], "BEATING {best}!")
        }
        Some(best) => write!(
            out[2],
            "BEST {best} · {} TO BEAT",
            best + 1 - u64::from(tracker.summary.frags)
        ),
        None => write!(out[2], "NO RECORD YET"),
    };
}

fn info_items<'a>(
    menu_left: f32,
    menu_bottom: f32,
    rows: &'a [String; 3],
    style: InfoStyle,
    emit: impl FnMut(Item<'a>),
) {
    let mut list: [(&str, Color, Option<Icon>); 3] = [("", INK, None); 3];
    let mut n = 0;
    for (k, row) in rows.iter().enumerate() {
        if !row.is_empty() {
            list[n] = (row.as_str(), style.colors[k], style.icons[k]);
            n += 1;
        }
    }
    info_rows(
        screen_width(),
        screen_height(),
        menu_left,
        menu_bottom,
        &list[..n],
        emit,
    );
}

pub(super) fn info(
    ui: &mut Ui,
    menu_left: f32,
    menu_bottom: f32,
    rows: &[String; 3],
    style: InfoStyle,
) {
    info_items(menu_left, menu_bottom, rows, style, |item| paint(ui, item));
}

/// The info rows' pictograms; call after `Ui::paint`.
pub(super) fn info_glyphs(menu_left: f32, menu_bottom: f32, rows: &[String; 3], style: InfoStyle) {
    info_items(menu_left, menu_bottom, rows, style, glyph);
}

pub(super) const BRIEFING_STYLE: InfoStyle = InfoStyle {
    colors: [GOLD, MUTED, INK],
    icons: [Some(Icon::Trophy), Some(Icon::Gamepad), None],
};
pub(super) const PAUSE_STYLE: InfoStyle = InfoStyle {
    colors: [INK, MUTED, GOLD],
    icons: [Some(Icon::Clock), None, Some(Icon::Trophy)],
};

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
        assert_eq!(first.best_line, "FIRST RECORD: 9");
        assert_eq!(first.next_line, "+3 → DOCK BOSS");
        assert_eq!(first.header, "9 — 1");
        let short = Results::new(&t, Some(12), true);
        assert!(!short.new_best);
        assert_eq!(short.best_line, "BEST 12 · 3 SHORT");
        assert_eq!(short.header, "9 — 1");
        let tied = Results::new(&t, Some(9), true);
        assert!(!tied.new_best, "a tie is not a record");
        assert_eq!(tied.best_line, "TIED BEST: 9");
        let beat = Results::new(&t, Some(7), true);
        assert!(beat.new_best);
        assert_eq!(beat.best_line, "NEW BEST! +2");
        let smoke = Results::new(&t, Some(1), false);
        assert!(!smoke.new_best, "automation never claims a record");
        assert_eq!(smoke.best_line, "NOT RECORDED");
        assert_eq!(smoke.stats[2], ("9.0".to_owned(), Stat::Label("K/D")));
        assert_eq!(smoke.stats[3].0, "100%");
        assert_eq!(smoke.stats[4].1, Stat::Glyph(Glyph::Headshot));
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
                    let mut glyphs: Vec<UiRect> = Vec::new();
                    let mut buttons = 0;
                    let mut panel: Option<UiRect> = None;
                    let inside = |rect: UiRect, p: UiRect| {
                        rect.min[0] >= p.min[0] - 0.01
                            && rect.min[0] + rect.size[0] <= p.min[0] + p.size[0] + 0.01
                            && rect.min[1] >= p.min[1]
                            && rect.min[1] + rect.size[1] <= p.min[1] + p.size[1] + 0.01
                    };
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
                            assert!(inside(rect, panel.unwrap()), "{text} outside card");
                            for b in blocks.iter().chain(&glyphs) {
                                assert!(rect.intersect(*b).is_none(), "{w}x{h} {text} overlaps");
                            }
                            blocks.push(rect);
                        }
                        Item::Glyph(rect, glyph, _) => {
                            assert!(inside(rect, panel.unwrap()), "{glyph:?} outside card");
                            for b in blocks.iter().chain(&glyphs) {
                                assert!(
                                    rect.intersect(*b).is_none(),
                                    "{w}x{h} {glyph:?} overlaps {b:?}"
                                );
                            }
                            glyphs.push(rect);
                        }
                        Item::Button(_, rect, ..) => {
                            buttons += 1;
                            assert!(rect.size[1] >= 44.);
                            assert!(
                                rect.min[1] + rect.size[1] <= h - 8.,
                                "{w}x{h} button clipped"
                            );
                            for b in blocks.iter().chain(&glyphs) {
                                assert!(rect.intersect(*b).is_none(), "button overlaps");
                            }
                            blocks.push(rect);
                        }
                    });
                    assert_eq!(buttons, 2);
                    assert!(glyphs.len() >= 8, "stats and lines lead with pictograms");
                }
            }
        }
        let rows = [
            ("A ROW", INK, Some(Icon::Clock)),
            ("ANOTHER ROW", INK, None),
            ("THIRD", INK, Some(Icon::Trophy)),
        ];
        for (w, h) in [(320., 480.), (1280., 576.), (1440., 900.)] {
            let mut last = 0.;
            let mut texts: Vec<UiRect> = Vec::new();
            info_rows(w, h, 40., 250., &rows, |item| match item {
                Item::Panel(r, _) | Item::Text(r, ..) | Item::Glyph(r, ..) => {
                    assert!(r.min[1] > 250. && r.min[1] + r.size[1] <= h - 150. + 0.01);
                    last = r.min[1] + r.size[1];
                    if !matches!(item, Item::Panel(..)) {
                        for t in &texts {
                            assert!(r.intersect(*t).is_none(), "info glyph overlaps its text");
                        }
                        texts.push(r);
                    }
                }
                Item::Button(..) => panic!("info rows have no buttons"),
            });
            assert!(last <= h - 150. + 0.01);
        }
    }
}
