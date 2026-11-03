//! Read-only newsroom presentation: a dark desk, case cards and paper dossiers.
//!
//! Every rectangle that accepts pointer input is laid out in `model.rs`; the
//! shapes here stay inside those hit regions so what you see is what you click.
use crate::{
    content::CASES,
    model::{Game, View},
};
use ferravex_arcade::shapes::{self, Cap, Fill, Join, Stroke};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;

const DESK_TOP: Color = Color::new(0.155, 0.13, 0.15, 1.);
const DESK_LOW: Color = Color::new(0.09, 0.075, 0.095, 1.);
const ROW: Color = Color::new(0.185, 0.16, 0.18, 1.);
const ROW_ON: Color = Color::new(0.29, 0.24, 0.235, 1.);
const INFO: Color = Color::new(0.16, 0.14, 0.155, 1.);
const SHEET: Color = Color::new(0.92, 0.86, 0.72, 1.);
const SHEET_TOP: Color = Color::new(0.96, 0.92, 0.81, 1.);
const SHEET_LOW: Color = Color::new(0.90, 0.835, 0.69, 1.);
const FIELD: Color = Color::new(0.87, 0.80, 0.66, 1.);
const DARK: Color = Color::new(0.16, 0.14, 0.17, 1.);
const SOFT: Color = Color::new(0.42, 0.36, 0.33, 1.);
const GOLD: Color = Color::new(0.87, 0.66, 0.36, 1.);
const RED_INK: Color = Color::new(0.59, 0.19, 0.18, 1.);
const GREEN_INK: Color = Color::new(0.20, 0.42, 0.30, 1.);
const CREAM: Color = Color::new(0.95, 0.91, 0.82, 1.);

const TABS: [&str; 4] = ["Locations", "People", "Notebook", "Theory"];
const OF_THREE: [&str; 4] = ["0/3", "1/3", "2/3", "3/3"];
const OF_SEVEN: [&str; 8] = ["0/7", "1/7", "2/7", "3/7", "4/7", "5/7", "6/7", "7/7"];
/// Left edge and readable width of text on the dossier sheet.
const PAGE_X: f32 = 352.;
const PAGE_W: f32 = 554.;
/// Readable width of text in the left-hand column cards.
const SIDE_W: f32 = 244.;

fn shade(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new(r, g, b, a)
}

/// Cheap contact shadow: one offset rounded fill instead of a blurred ring.
fn lip(r: Rect, radius: f32, depth: f32, alpha: f32) {
    let s = Rect::new(r.x, r.y + depth, r.w, r.h);
    shapes::fill_rounded_rect(s, radius, shade(0.02, 0.01, 0.02, alpha));
}
/// Warm desk-side card without the cost of a blurred shadow.
fn desk_card(x: f32, y: f32, w: f32, h: f32) {
    let r = Rect::new(x, y, w, h);
    lip(r, 10., 3., 0.35);
    shapes::fill_rounded_rect(r, 10., Fill::vertical(r, lighten(INFO, 0.04), INFO));
}

// ---------------------------------------------------------------- type

/// Modern face without the drop shadow `style()` adds to light ink. The desk
/// and the paper are flat, so shadows would only double the glyph cost.
fn plain(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
/// Draw a label shrunk (never enlarged) to fit `width`; returns its width.
fn fit_line(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) -> f32 {
    let s = plain(size, color);
    let w = text_width(value, s);
    let s = if w > width && w > 0. {
        s.with_size(size * width / w)
    } else {
        s
    };
    draw_text_styled(value, x, y, s).width
}
fn label_right(value: &str, right: f32, y: f32, size: f32, color: Color) {
    draw_text_styled(
        value,
        right,
        y,
        plain(size, color).with_align(TextAlign::Right),
    );
}

/// Small, letter-spaced uppercase section label; returns its width.
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, plain(11., color).with_letter_spacing(1.5)).width
}
fn eyebrow_aligned(value: &str, x: f32, y: f32, color: Color, align: TextAlign) {
    let s = plain(11., color).with_letter_spacing(1.5).with_align(align);
    draw_text_styled(value, x, y, s);
}
/// Paragraph with reading leading; returns the baseline after the last line.
fn prose(value: &str, x: f32, y: f32, width: f32, size: f32, leading: f32, color: Color) -> f32 {
    let s = plain(size, color);
    let mut baseline = y;
    for line in wrap_text(value, width, s) {
        draw_text_styled(line, x, baseline, s);
        baseline += leading;
    }
    baseline
}

// ---------------------------------------------------------------- surfaces

fn desk() {
    let r = Rect::new(16., 112., 928., 456.);
    shapes::fill_rounded_rect(r, 12., Fill::vertical(r, DESK_TOP, DESK_LOW));
    // A few faint grain lines give the desk a surface without busy texture.
    for i in 0..6 {
        let y = 196. + i as f32 * 64. + (i % 2) as f32 * 9.;
        panel(30., y, 900., 1., shade(1., 0.86, 0.72, 0.022));
    }
}
/// A list row on the desk; selected rows lift and gain a gold edge.
fn row(x: f32, y: f32, w: f32, h: f32, selected: bool) {
    let r = Rect::new(x, y, w, h);
    let radius = if h < 50. { 8. } else { 10. };
    if selected {
        lip(r, radius, 3., 0.45);
        shapes::fill_rounded_rect(r, radius, Fill::vertical(r, lighten(ROW_ON, 0.05), ROW_ON));
        shapes::stroke_rounded_rect(r, radius, 1., with_alpha(GOLD, 0.6));
        let bar = Rect::new(x + 5., y + h * 0.25, 3., h * 0.5);
        shapes::fill_rounded_rect(bar, 1.5, GOLD);
    } else {
        shapes::fill_rounded_rect(r, radius, Fill::vertical(r, lighten(ROW, 0.03), ROW));
    }
}
/// The paper dossier: a sheet on a small stack, with a clip at the top.
fn sheet(x: f32, y: f32, w: f32, h: f32) {
    let r = Rect::new(x, y, w, h);
    // Two stacked contact shadows read as soft depth at a fraction of the
    // cost of a blurred ring.
    let spread = Rect::new(x - 3., y + 2., w + 6., h + 6.);
    lip(spread, 9., 4., 0.18);
    lip(r, 6., 3., 0.35);
    rounded(
        x + 4.,
        y + 5.,
        w - 1.,
        h - 1.,
        6.,
        shade(0.70, 0.62, 0.49, 1.),
    );
    shapes::fill_rounded_rect(r, 6., Fill::vertical(r, SHEET_TOP, SHEET_LOW));
    shapes::stroke_rounded_rect(r, 6., 1., shade(0.32, 0.22, 0.12, 0.22));
}
/// Brass clip holding the stack; drawn last so it sits over photographs.
fn clip(cx: f32, y: f32) {
    let steel = shade(0.66, 0.62, 0.55, 1.);
    shapes::stroke_rounded_rect(Rect::new(cx - 8., y - 9., 16., 40.), 8., 2.2, steel);
    shapes::stroke_rounded_rect(Rect::new(cx - 4., y - 4., 8., 27.), 4., 2., steel);
}
/// Rubber stamp drawn in ink; returns its left edge.
fn stamp(label: &str, right: f32, y: f32, color: Color) -> f32 {
    let s = plain(12., with_alpha(color, 0.9)).with_letter_spacing(2.);
    let w = text_width(label, s) + 24.;
    let r = Rect::new(right - w, y, w, 28.);
    shapes::fill_rounded_rect(r, 5., with_alpha(color, 0.07));
    shapes::stroke_rounded_rect(r, 5., 2., with_alpha(color, 0.75));
    draw_text_styled(label, r.x + 12., s.centered_baseline(r.y + 14.), s);
    r.x
}
/// A large clickable action inside `x..x+w`, `y=505..543`.
fn action_button(x: f32, w: f32, key: &str, value: &str, active: bool) {
    let r = Rect::new(x, 505., w, 36.);
    let ink = if active {
        lip(r, 9., 3., 0.3);
        shapes::fill_rounded_rect(r, 9., Fill::vertical(r, lighten(DARK, 0.1), DARK));
        let shine = Rect::new(x + 9., 506., w - 18., 1.);
        shapes::fill_rounded_rect(shine, 0., shade(1., 1., 1., 0.12));
        CREAM
    } else {
        shapes::fill_rounded_rect(r, 9., shade(0.80, 0.73, 0.60, 1.));
        SOFT
    };
    let kw = keycap(key, "", x + 14., 513., ink);
    let s = plain(16., ink);
    let room = w - kw - 40.;
    let s = if text_width(value, s) > room {
        s.with_size(16. * room / text_width(value, s))
    } else {
        s
    };
    draw_text_styled(value, x + 14. + kw + 10., s.centered_baseline(523.), s);
}
/// Coloured dot and short label for a row's state.
fn status(label: &str, x: f32, y: f32, color: Color) {
    shapes::fill_circle(vec2(x + 3., y - 4.), 3., color);
    draw_text_styled(label, x + 11., y, plain(12., color));
}
/// Row of rounded pips for small counts (credibility, cases closed).
fn pips(x: f32, y: f32, total: usize, filled: usize, w: f32, on: Color, off: Color) {
    let gap = (w * 0.15).max(5.);
    for i in 0..total {
        let r = Rect::new(x + i as f32 * (w + gap), y, w, 10.);
        if i < filled {
            shapes::fill_rounded_rect(r, 5., Fill::vertical(r, lighten(on, 0.2), on));
        } else {
            shapes::fill_rounded_rect(r, 5., off);
        }
    }
}
fn chevron(x: f32, y: f32, dir: f32, color: Color) {
    let points = [
        vec2(x - 3. * dir, y - 7.),
        vec2(x + 3. * dir, y),
        vec2(x - 3. * dir, y + 7.),
    ];
    shapes::polyline(
        &points,
        Stroke::new(2., color).join(Join::Round).cap(Cap::Round),
    );
}

// ---------------------------------------------------------------- art

fn portrait(x: f32, y: f32, s: f32, id: usize, selected: bool) {
    let skin = [
        shade(0.72, 0.51, 0.35, 1.),
        shade(0.91, 0.70, 0.48, 1.),
        shade(0.53, 0.36, 0.29, 1.),
    ][id % 3];
    let coat = [RED_INK, GREEN_INK, shade(0.26, 0.31, 0.43, 1.)][id % 3];
    let c = vec2(x, y);
    let back = if selected {
        GOLD
    } else {
        shade(0.33, 0.28, 0.29, 1.)
    };
    shapes::fill_circle(
        c,
        27. * s,
        Fill::radial(
            c - vec2(0., 9. * s),
            34. * s,
            lighten(back, 0.22),
            darken(back, 0.12),
        ),
    );
    draw_triangle(
        vec2(x - 18. * s, y + 21. * s),
        vec2(x + 18. * s, y + 21. * s),
        vec2(x, y - 2. * s),
        coat,
    );
    shapes::fill_circle(vec2(x, y - 5. * s), 13. * s, skin);
    panel(x - 14. * s, y - 16. * s, 28. * s, 7. * s, DARK);
    if id.is_multiple_of(2) {
        rounded(x - 10. * s, y - 25. * s, 20. * s, 11. * s, 3. * s, DARK);
        rounded(x - 19. * s, y - 17. * s, 38. * s, 3. * s, 1.5 * s, DARK);
    } else {
        shapes::fill_circle(vec2(x - 7. * s, y - 16. * s), 9. * s, DARK);
        shapes::fill_circle(vec2(x + 7. * s, y - 16. * s), 9. * s, DARK);
    }
    shapes::fill_circle(vec2(x - 4. * s, y - 6. * s), 1.5 * s, DARK);
    shapes::fill_circle(vec2(x + 5. * s, y - 6. * s), 1.5 * s, DARK);
    draw_line(
        x - 2. * s,
        y + 2. * s,
        x + 5. * s,
        y + 2. * s,
        1.5 * s,
        DARK,
    );
    shapes::stroke_circle(
        c,
        27. * s,
        1.5,
        if selected {
            lighten(GOLD, 0.35)
        } else {
            shade(1., 1., 1., 0.1)
        },
    );
}
fn place_icon(x: f32, y: f32, id: usize, selected: bool) {
    let c = vec2(x, y);
    let base = shade(0.27, 0.245, 0.255, 1.);
    shapes::fill_circle(
        c,
        25.,
        Fill::radial(
            c - vec2(0., 8.),
            32.,
            lighten(base, 0.12),
            darken(base, 0.2),
        ),
    );
    shapes::stroke_circle(
        c,
        25.,
        1.,
        if selected {
            with_alpha(GOLD, 0.8)
        } else {
            shade(1., 1., 1., 0.08)
        },
    );
    let ink = if selected { GOLD } else { darken(GOLD, 0.18) };
    match id % 6 {
        0 => {
            outline(x - 14., y - 15., 28., 30., 3., 2., ink);
            outline(x - 9., y - 10., 18., 20., 2., 1., ink);
            shapes::stroke_circle(vec2(x + 3., y), 4., 1.5, ink);
        }
        1 => {
            draw_triangle(
                vec2(x - 14., y + 9.),
                vec2(x + 14., y + 9.),
                vec2(x, y - 14.),
                ink,
            );
            shapes::fill_capsule(vec2(x - 15., y + 10.), vec2(x + 15., y + 10.), 1.5, ink);
            shapes::fill_circle(vec2(x, y + 15.), 2.5, ink);
        }
        2 => {
            panel(x - 7., y - 9., 14., 24., ink);
            panel(x - 11., y - 13., 22., 4., ink);
            draw_triangle(
                vec2(x - 11., y - 13.),
                vec2(x + 11., y - 13.),
                vec2(x, y - 20.),
                ink,
            );
            draw_line(x - 17., y - 6., x - 10., y - 8., 2., ink);
            draw_line(x + 10., y - 8., x + 18., y - 6., 2., ink);
        }
        3 => {
            outline(x - 15., y - 9., 30., 22., 3., 2., ink);
            shapes::stroke_circle(vec2(x + 6., y + 2.), 5.5, 1.5, ink);
            panel(x - 10., y - 2., 7., 2., ink);
            panel(x - 10., y + 4., 7., 2., ink);
            draw_line(x - 10., y - 9., x - 3., y - 19., 2., ink);
        }
        4 => {
            draw_line(x, y + 18., x, y - 11., 2.5, ink);
            draw_ellipse(x - 7., y - 5., 9., 4.5, -0.5, ink);
            draw_ellipse(x + 7., y + 2., 9., 4.5, 0.5, ink);
            shapes::fill_circle(vec2(x, y - 14.), 5.5, ink);
        }
        _ => {
            draw_triangle(
                vec2(x - 18., y - 10.),
                vec2(x + 18., y - 10.),
                vec2(x, y - 20.),
                ink,
            );
            for offset in [-11., 0., 11.] {
                panel(x + offset - 2.5, y - 8., 5., 21., ink);
            }
            panel(x - 18., y + 14., 36., 3., ink);
        }
    }
}
/// Instant-camera print with a portrait, clipped to the dossier.
fn polaroid(x: f32, y: f32, id: usize) {
    let r = Rect::new(x, y, 92., 106.);
    lip(r, 3., 3., 0.22);
    shapes::fill_rounded_rect(r, 3., shade(0.985, 0.975, 0.95, 1.));
    let photo = Rect::new(x + 7., y + 7., 78., 76.);
    shapes::fill_rounded_rect(
        photo,
        2.,
        Fill::vertical(
            photo,
            shade(0.30, 0.27, 0.29, 1.),
            shade(0.18, 0.16, 0.18, 1.),
        ),
    );
    portrait(x + 46., y + 47., 1.25, id, false);
}

// ---------------------------------------------------------------- tabs

fn tabs(game: &Game) {
    let track = Rect::new(20., 113., 920., 33.);
    shapes::fill_rounded_rect(track, 11., shade(0.05, 0.04, 0.05, 0.55));
    let records = game.evidence_count();
    for (i, name) in TABS.iter().enumerate() {
        let x = 24. + i as f32 * 228.;
        let active = game.view.index() == i;
        let r = Rect::new(x + 1., 115., 224., 29.);
        if active {
            lip(r, 8., 2., 0.45);
            shapes::fill_rounded_rect(r, 8., Fill::vertical(r, SHEET_TOP, SHEET));
        }
        let badge = Rect::new(x + 11., 120., 19., 19.);
        shapes::fill_rounded_rect(
            badge,
            5.,
            if active { DARK } else { with_alpha(GOLD, 0.16) },
        );
        let digit = plain(12., if active { SHEET } else { GOLD }).with_align(TextAlign::Center);
        let number = ["1", "2", "3", "4"][i];
        draw_text_styled(number, x + 20.5, digit.centered_baseline(129.5), digit);
        let label = plain(15., if active { DARK } else { lighten(GOLD, 0.1) });
        draw_text_styled(name, x + 40., label.centered_baseline(129.5), label);
        let (progress, done) = match i {
            0 => {
                let n = game.known[..3].iter().filter(|&&v| v).count();
                (OF_THREE[n], n == 3)
            }
            1 => {
                let n = game.known[3..6].iter().filter(|&&v| v).count();
                (OF_THREE[n], n == 3)
            }
            2 => (OF_SEVEN[records.min(7)], game.linked),
            _ => (if game.can_file() { "Ready" } else { "" }, game.can_file()),
        };
        let tone = match (active, done) {
            (true, true) => GREEN_INK,
            (true, false) => SOFT,
            (false, true) => GOLD,
            (false, false) => MUTED,
        };
        let s = plain(12., tone).with_align(TextAlign::Right);
        let right = x + 214.;
        let w = draw_text_styled(progress, right, s.centered_baseline(129.5), s).width;
        if done && i < 3 {
            let check = plain(12., tone).with_align(TextAlign::Right);
            draw_text_styled(
                "\u{2713}",
                right - w - 5.,
                check.centered_baseline(129.5),
                check,
            );
        }
    }
}

// ---------------------------------------------------------------- views

fn side_note(title: &str, body: &str) {
    desk_card(36., 456., 276., 90.);
    eyebrow(title, 52., 480., GOLD);
    prose(body, 52., 504., SIDE_W, 14., 19., with_alpha(SHEET, 0.86));
}
fn page_heading(kind: &str, title: &str, title_width: f32) {
    eyebrow(kind, PAGE_X, 184., RED_INK);
    fit_line(title, PAGE_X, 222., title_width, 24., DARK);
}
fn page_divider(y: f32, w: f32) {
    panel(PAGE_X, y, w, 1., shade(0.45, 0.33, 0.22, 0.28));
}

fn draw_scene(game: &Game) {
    let case = game.case_data();
    let selected = game.selected[0];
    for (i, place) in case.places.iter().enumerate() {
        let y = 188. + i as f32 * 86.;
        let on = selected == i;
        row(36., y, 276., 76., on);
        place_icon(72., y + 38., game.case, on);
        fit_line(
            place.name,
            108.,
            y + 33.,
            192.,
            17.,
            if on { CREAM } else { SHEET },
        );
        if game.known[i] {
            status("Recorded", 108., y + 56., GOLD);
        } else {
            status("Unexamined", 108., y + 56., MUTED);
        }
    }
    side_note(
        "INDEPENDENT RECORDS",
        "Objects and verified alibis are facts. A witness may lie.",
    );
    let place = &case.places[selected];
    let kind = [
        "LOCATION \u{b7} E1",
        "LOCATION \u{b7} E2",
        "LOCATION \u{b7} E3",
    ][selected];
    page_heading(kind, place.name, 380.);
    if game.known[selected] {
        stamp("VERIFIED", 870., 178., GREEN_INK);
    }
    page_divider(240., PAGE_W);
    if game.known[selected] {
        eyebrow("VERIFIED EVIDENCE", PAGE_X, 272., GREEN_INK);
        prose(place.evidence, PAGE_X, 302., PAGE_W, 18., 27., DARK);
    } else {
        eyebrow("THE ASSIGNMENT", PAGE_X, 272., RED_INK);
        let next = prose(case.opening, PAGE_X, 302., PAGE_W, 18., 27., DARK);
        prose(
            "Examine this location to add its record to the notebook.",
            PAGE_X,
            next + 8.,
            PAGE_W,
            14.,
            20.,
            SOFT,
        );
    }
    action_button(
        350.,
        220.,
        "Space",
        if game.known[selected] {
            "Re-examine"
        } else {
            "Examine"
        },
        true,
    );
    action_button(590., 328., "X", "Open notebook", true);
}
fn draw_people(game: &Game) {
    let case = game.case_data();
    let selected = game.selected[1];
    for (i, suspect) in case.suspects.iter().enumerate() {
        let y = 188. + i as f32 * 86.;
        let on = selected == i;
        row(36., y, 276., 76., on);
        portrait(72., y + 38., 1., game.case * 3 + i, on);
        fit_line(
            suspect.name,
            108.,
            y + 25.,
            190.,
            17.,
            if on { CREAM } else { SHEET },
        );
        fit_line(suspect.role, 108., y + 44., 190., 12., MUTED);
        if game.known[i + 3] {
            status("Statement filed", 108., y + 64., GOLD);
        } else {
            status("Awaiting interview", 108., y + 64., MUTED);
        }
    }
    side_note(
        "THE RIGHT TO AN ALIBI",
        "Find a contradiction before confronting its witness.",
    );
    let suspect = &case.suspects[selected];
    let kind = [
        "WITNESS \u{b7} T1",
        "WITNESS \u{b7} T2",
        "WITNESS \u{b7} T3",
    ][selected];
    page_heading(kind, suspect.name, 400.);
    fit_line(suspect.role, PAGE_X, 248., 400., 15., SOFT);
    polaroid(800., 172., game.case * 3 + selected);
    page_divider(266., 430.);
    let signed = game.confronted && selected == case.culprit;
    if signed {
        eyebrow("SIGNED RESPONSE \u{b7} C1", PAGE_X, 304., RED_INK);
        prose(case.confession, PAGE_X, 334., PAGE_W, 18., 27., DARK);
    } else if game.known[3 + selected] {
        eyebrow(
            "STATEMENT \u{2014} A CLAIM, NOT PROOF",
            PAGE_X,
            304.,
            RED_INK,
        );
        prose(suspect.statement, PAGE_X, 334., PAGE_W, 18., 27., DARK);
    } else {
        eyebrow("NOT YET INTERVIEWED", PAGE_X, 304., SOFT);
        prose(
            "Record this witness's statement, then compare it with the evidence.",
            PAGE_X,
            334.,
            PAGE_W,
            17.,
            25.,
            SOFT,
        );
    }
    if signed {
        stamp("SIGNED", 780., 286., RED_INK);
    }
    action_button(350., 220., "Space", "Interview", true);
    action_button(590., 328., "C", "Confront with link", game.linked);
}
fn note_code(index: usize) -> &'static str {
    ["E1", "E2", "E3", "T1", "T2", "T3", "C1"][index.min(6)]
}
fn note_tone(index: usize) -> Color {
    if index < 3 {
        GOLD
    } else if index < 6 {
        shade(0.62, 0.70, 0.86, 1.)
    } else {
        shade(0.90, 0.48, 0.44, 1.)
    }
}
fn draw_notebook(game: &Game) {
    let selected = game.selected[2];
    for i in 0..7 {
        let y = 180. + i as f32 * 43.;
        let on = i == selected;
        row(36., y, 276., 37., on);
        let tone = if game.known[i] { note_tone(i) } else { MUTED };
        let badge = Rect::new(48., y + 9., 26., 19.);
        shapes::fill_rounded_rect(badge, 5., with_alpha(tone, 0.16));
        let code = plain(11., tone).with_align(TextAlign::Center);
        draw_text_styled(note_code(i), 61., code.centered_baseline(y + 18.5), code);
        let (title, color) = if game.known[i] {
            (game.note_title(i), if on { CREAM } else { SHEET })
        } else {
            ("Not collected", with_alpha(MUTED, 0.8))
        };
        fit_line(title, 84., y + 24., 196., 14., color);
        if game.pins.contains(&Some(i)) {
            let c = vec2(296., y + 18.5);
            shapes::fill_circle(
                c,
                5.,
                Fill::radial(c - vec2(1.5, 1.5), 6., shade(0.95, 0.45, 0.4, 1.), RED_INK),
            );
        }
    }
    desk_card(36., 488., 276., 58.);
    if game.linked {
        chip(
            "\u{2713} Contradiction found",
            52.,
            498.,
            11.,
            shade(0.55, 0.85, 0.6, 1.),
        );
    } else {
        let pinned = game.pins.iter().filter(|p| p.is_some()).count();
        chip(
            ["Pin two records", "Pin one more record", "Ready to connect"][pinned],
            52.,
            498.,
            11.,
            GOLD,
        );
    }
    let ink = with_alpha(SHEET, 0.8);
    let mut x = 52.;
    for (key, action) in [("Space", "Pin"), ("C", "Connect"), ("X", "Clear")] {
        x += keycap(key, action, x, 521., ink) + 12.;
    }
    let known = game.known[selected];
    let kind = [
        "E1 \u{b7} VERIFIED FACT",
        "E2 \u{b7} VERIFIED FACT",
        "E3 \u{b7} VERIFIED FACT",
        "T1 \u{b7} WITNESS TESTIMONY",
        "T2 \u{b7} WITNESS TESTIMONY",
        "T3 \u{b7} WITNESS TESTIMONY",
        "C1 \u{b7} SIGNED RESPONSE",
    ][selected.min(6)];
    page_heading(
        kind,
        if known {
            game.note_title(selected)
        } else {
            "Missing record"
        },
        420.,
    );
    if known {
        let (label, color) = if selected < 3 {
            ("FACT", GREEN_INK)
        } else if selected < 6 {
            ("CLAIM", RED_INK)
        } else {
            ("SIGNED", RED_INK)
        };
        stamp(label, 870., 178., color);
    }
    page_divider(240., PAGE_W);
    if known {
        prose(
            game.note_text(selected),
            PAGE_X,
            280.,
            PAGE_W,
            18.,
            27.,
            DARK,
        );
    } else {
        prose(
            "Inspect 3 locations, interview 3 suspects, then connect evidence.",
            PAGE_X,
            280.,
            PAGE_W,
            17.,
            25.,
            SOFT,
        );
    }
    // Pinned index cards, joined with red thread once both are set.
    for (i, pin) in game.pins.iter().enumerate() {
        let x = 350. + i as f32 * 286.;
        let r = Rect::new(x, 430., 270., 58.);
        if pin.is_some() {
            lip(r, 4., 3., 0.18);
            shapes::fill_rounded_rect(
                r,
                4.,
                Fill::vertical(r, shade(0.99, 0.97, 0.91, 1.), shade(0.95, 0.92, 0.84, 1.)),
            );
            panel(x, 448., 270., 1., with_alpha(RED_INK, 0.3));
        } else {
            shapes::fill_rounded_rect(r, 4., with_alpha(darken(SHEET, 0.12), 0.6));
        }
        let pin_c = vec2(x + 16., 439.);
        if pin.is_some() {
            shapes::fill_circle(pin_c + vec2(1., 2.), 5., shade(0., 0., 0., 0.2));
            shapes::fill_circle(
                pin_c,
                5.,
                Fill::radial(
                    pin_c - vec2(1.5, 1.5),
                    6.,
                    shade(0.95, 0.45, 0.4, 1.),
                    RED_INK,
                ),
            );
        }
        eyebrow(
            ["PIN 1", "PIN 2"][i],
            x + 30.,
            443.,
            if pin.is_some() { RED_INK } else { SOFT },
        );
        match pin {
            Some(index) => {
                let s = plain(11., note_tone(*index)).with_align(TextAlign::Right);
                draw_text_styled(
                    note_code(*index),
                    x + 256.,
                    443.,
                    s.with_color(darken(note_tone(*index), 0.35)),
                );
                fit_line(game.note_title(*index), x + 16., 474., 240., 15., DARK);
            }
            None => {
                fit_line("No record pinned", x + 16., 474., 240., 15., SOFT);
            }
        }
    }
    if game.pins.iter().all(Option::is_some) {
        let a = vec2(366., 439.);
        let b = vec2(652., 439.);
        let sag = if game.linked { 3. } else { 9. };
        let points: [Vec2; 9] = std::array::from_fn(|i| {
            let t = i as f32 / 8.;
            a.lerp(b, t) + vec2(0., sag * 4. * t * (1. - t))
        });
        shapes::polyline(
            &points,
            Stroke::new(2., with_alpha(RED_INK, 0.9))
                .join(Join::Round)
                .cap(Cap::Round),
        );
    }
    action_button(350., 220., "Space", "Pin / unpin", known);
    action_button(590., 328., "C", "Test connection", true);
}
fn draw_theory(game: &Game) {
    let case = game.case_data();
    desk_card(36., 182., 276., 262.);
    eyebrow("BEFORE PUBLICATION", 52., 206., GOLD);
    let scenes = game.known[..3].iter().filter(|&&v| v).count();
    let talks = game.known[3..6].iter().filter(|&&v| v).count();
    let checklist = [
        ("Scenes examined", OF_THREE[scenes], scenes == 3),
        ("Interviews", OF_THREE[talks], talks == 3),
        ("Contradiction", "", game.linked),
        ("Signed response", "", game.confronted),
    ];
    for (i, (name, count, done)) in checklist.iter().enumerate() {
        let y = 238. + i as f32 * 33.;
        let b = Rect::new(52., y - 13., 17., 17.);
        if *done {
            shapes::fill_rounded_rect(b, 5., Fill::vertical(b, lighten(GOLD, 0.15), GOLD));
            let check = [vec2(56., y - 4.5), vec2(59.5, y - 1.), vec2(65., y - 8.5)];
            shapes::polyline(
                &check,
                Stroke::new(2., DARK).join(Join::Round).cap(Cap::Round),
            );
        } else {
            shapes::stroke_rounded_rect(b, 5., 1.5, with_alpha(MUTED, 0.6));
        }
        fit_line(name, 80., y, 180., 15., if *done { SHEET } else { MUTED });
        if !count.is_empty() {
            label_right(count, 296., y, 13., if *done { GOLD } else { MUTED });
        }
    }
    divider(52., 368., SIDE_W, with_alpha(GOLD, 0.25));
    prose(
        "Choose culprit, method and motive. A wrong report costs 1 credibility.",
        52.,
        392.,
        SIDE_W,
        14.,
        19.,
        with_alpha(SHEET, 0.86),
    );
    desk_card(36., 456., 276., 90.);
    eyebrow("CREDIBILITY", 52., 480., GOLD);
    let value = ["0 / 4", "1 / 4", "2 / 4", "3 / 4", "4 / 4"][usize::from(game.credibility.min(4))];
    label_right(value, 296., 481., 14., SHEET);
    pips(
        52.,
        494.,
        4,
        usize::from(game.credibility),
        55.,
        GOLD,
        shade(1., 1., 1., 0.07),
    );
    fit_line(
        "Four false reports end the campaign.",
        52.,
        536.,
        SIDE_W,
        12.,
        MUTED,
    );

    fit_line("The accusation", PAGE_X, 205., 400., 24., DARK);
    let fields = [
        "WHO \u{b7} SUSPECT",
        "HOW \u{b7} METHOD",
        "WHY \u{b7} MOTIVE",
    ];
    for (row, field) in fields.iter().enumerate() {
        let y = 229. + row as f32 * 73.;
        let r = Rect::new(350., y, 568., 65.);
        let on = game.selected[3] == row;
        if on {
            lip(r, 8., 3., 0.16);
            shapes::fill_rounded_rect(
                r,
                8.,
                Fill::vertical(r, shade(0.99, 0.96, 0.89, 1.), shade(0.96, 0.92, 0.83, 1.)),
            );
            shapes::stroke_rounded_rect(r, 8., 2., RED_INK);
        } else {
            shapes::fill_rounded_rect(r, 8., FIELD);
        }
        eyebrow(field, 388., y + 22., if on { RED_INK } else { SOFT });
        let value = match row {
            0 => case.suspects[game.theory[row]].name,
            1 => case.methods[game.theory[row]],
            _ => case.motives[game.theory[row]],
        };
        fit_line(value, 388., y + 50., 440., 20., DARK);
        let arrow = if on { RED_INK } else { with_alpha(SOFT, 0.6) };
        chevron(368., y + 37., -1., arrow);
        chevron(900., y + 37., 1., arrow);
        for option in 0..3 {
            let c = vec2(862. + option as f32 * 11., y + 18.);
            if option == game.theory[row] {
                shapes::fill_circle(c, 3., if on { RED_INK } else { SOFT });
            } else {
                shapes::stroke_circle(c, 2.5, 1., with_alpha(SOFT, 0.6));
            }
        }
    }
    let mut x = 352.;
    for (key, action) in [
        ("\u{2191}\u{2193}", "Field"),
        ("\u{2190}\u{2192}", "Answer"),
    ] {
        x += keycap(key, action, x, 466., SOFT) + 18.;
    }
    if !game.can_file() {
        let s = plain(12., RED_INK).with_align(TextAlign::Right);
        draw_text_styled(
            "Collect every record first",
            916.,
            s.centered_baseline(476.),
            s,
        );
    }
    action_button(350., 568., "Space", "File this accusation", game.can_file());
}
fn draw_debrief(game: &Game) {
    let case = game.case_data();
    sheet(32., 160., 896., 390.);
    let (left, right) = (56., 904.);
    let rule = shade(0.45, 0.33, 0.22, 0.3);
    // Masthead.
    let edition = [
        "CASE 1 OF 6",
        "CASE 2 OF 6",
        "CASE 3 OF 6",
        "CASE 4 OF 6",
        "CASE 5 OF 6",
        "CASE 6 OF 6",
    ][game.case.min(5)];
    eyebrow(edition, left, 190., SOFT);
    let mast = plain(22., DARK).with_align(TextAlign::Center);
    draw_text_styled("The Gazette", 480., 194., mast);
    eyebrow_aligned(case.district, 864., 190., SOFT, TextAlign::Right);
    panel(left, 204., right - left, 2., with_alpha(DARK, 0.8));
    panel(left, 208., right - left, 1., with_alpha(DARK, 0.45));
    // Headline, deck and stamp.
    eyebrow("LATE EDITION", left, 230., RED_INK);
    fit_line(case.title, left, 264., 600., 30., DARK);
    stamp("CASE SOLVED", right, 236., GREEN_INK);
    fit_line(case.confession, left, 290., right - left, 16., SOFT);
    panel(left, 304., right - left, 1., rule);
    let facts = [
        ("CULPRIT", case.suspects[case.culprit].name),
        ("METHOD", case.methods[case.method]),
        ("MOTIVE", case.motives[case.motive]),
    ];
    for (i, (label, value)) in facts.iter().enumerate() {
        let x = left + i as f32 * 284.;
        if i > 0 {
            panel(x - 16., 314., 1., 46., rule);
        }
        eyebrow(label, x, 328., RED_INK);
        fit_line(value, x, 354., 260., 19., DARK);
    }
    panel(left, 370., right - left, 1., rule);
    let culprit = &case.suspects[case.culprit];
    eyebrow("THE EVIDENCE \u{b7} E1", left, 394., GREEN_INK);
    prose(case.places[0].evidence, left, 416., 398., 14., 19., DARK);
    panel(480., 382., 1., 100., rule);
    let denial = [
        "THE DENIAL \u{b7} T1",
        "THE DENIAL \u{b7} T2",
        "THE DENIAL \u{b7} T3",
    ][case.culprit.min(2)];
    eyebrow(denial, 504., 394., RED_INK);
    prose(culprit.statement, 504., 416., 398., 14., 19., DARK);
    // Campaign standing beside the action.
    eyebrow("CASES", left, 518., SOFT);
    let off = shade(0.3, 0.2, 0.1, 0.15);
    pips(186., 509., CASES.len(), game.solved, 20., GREEN_INK, off);
    eyebrow("CREDIBILITY", left, 539., SOFT);
    pips(186., 530., 4, usize::from(game.credibility), 20., GOLD, off);
    action_button(
        350.,
        568.,
        "Space",
        if game.solved == CASES.len() {
            "Publish the final edition"
        } else {
            "Next case"
        },
        true,
    );
}
fn draw(game: &Game) {
    desk();
    tabs(game);
    if game.debrief {
        draw_debrief(game);
        clip(890., 160.);
        return;
    }
    sheet(332., 160., 596., 390.);
    eyebrow(game.case_data().district, 40., 172., with_alpha(GOLD, 0.85));
    match game.view {
        View::Places => draw_scene(game),
        View::People => draw_people(game),
        View::Notebook => draw_notebook(game),
        View::Theory => draw_theory(game),
    }
    clip(890., 160.);
    shapes::fill_circle(vec2(42., 560.), 2.5, GOLD);
    fit_line(game.notice, 52., 564., 880., 12., with_alpha(GOLD, 0.9));
}
impl ArcadeGame for Game {
    fn new() -> Self {
        Game::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        draw(self);
    }
    fn hud(&self) -> Hud {
        let rank = if self.mistakes == 0 {
            "Editor of Record"
        } else if self.mistakes == 1 {
            "Trusted Correspondent"
        } else {
            "Persistent Reporter"
        };
        Hud {
            stage: format!("Case {} of 6", self.case_number()),
            objective: if self.outcome() == Outcome::Won {
                "The final edition is published.".into()
            } else if self.outcome() == Outcome::Lost {
                "The Gazette has lost its readers' trust.".into()
            } else if self.debrief {
                format!("{}: case solved. Read the edition.", self.case_data().title)
            } else {
                format!("{}: identify the culprit.", self.case_data().title)
            },
            status: match self.outcome() {
                Outcome::Won => format!(
                    "All six cases closed with {}/4 credibility. Your rank: {rank}.",
                    self.credibility
                ),
                Outcome::Lost => format!(
                    "Four false reports cost the Gazette its readers' trust after {} of 6 cases.",
                    self.solved
                ),
                Outcome::Playing => format!(
                    "Credibility {}/4 | Records {}/7 | Cases solved {}/6",
                    self.credibility,
                    self.evidence_count(),
                    self.solved
                ),
            },
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        Game::outcome(self)
    }
    fn summary(&self) -> String {
        if self.outcome() == Outcome::Won {
            let rank = if self.mistakes == 0 {
                "Editor of Record"
            } else if self.mistakes == 1 {
                "Trusted Correspondent"
            } else {
                "Persistent Reporter"
            };
            format!(
                "Six cases. {} verified records. The grey-seal conspiracy is exposed and the city's originals are safe. Your rank: {}. Credibility: {}/4. Try again for a spotless edition.",
                self.total_notes, rank, self.credibility
            )
        } else {
            format!(
                "Four false conclusions cost the Gazette its readers' trust. You closed {} of {} cases. Start a fresh investigation; collect all records and compare each denial with physical evidence before filing.",
                self.solved,
                CASES.len()
            )
        }
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.autoplay(tick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lines(value: &str, width: f32, size: f32) -> usize {
        wrap_text(value, width, plain(size, DARK)).count()
    }
    #[test]
    fn authored_case_text_fits_the_dossier_and_edition_layouts() {
        for case in &CASES {
            assert!(lines(case.opening, PAGE_W, 18.) <= 3, "{}", case.opening);
            assert!(text_width(case.confession, plain(16., DARK)) <= 848.);
            for place in &case.places {
                // Location page: evidence ends above the action buttons.
                assert!(302. + (lines(place.evidence, PAGE_W, 18.) - 1) as f32 * 27. < 490.);
                // Notebook page: evidence ends above the pinned cards.
                assert!(280. + (lines(place.evidence, PAGE_W, 18.) - 1) as f32 * 27. < 424.);
                assert!(
                    text_width(place.name, plain(24., DARK)) <= 380.,
                    "{}",
                    place.name
                );
                assert!(
                    text_width(place.title, plain(24., DARK)) <= 420.,
                    "{}",
                    place.title
                );
                assert!(
                    text_width(place.name, plain(17., DARK)) <= 192.,
                    "{}",
                    place.name
                );
            }
            for suspect in &case.suspects {
                assert!(334. + (lines(suspect.statement, PAGE_W, 18.) - 1) as f32 * 27. < 490.);
                assert!(
                    text_width(suspect.name, plain(17., DARK)) <= 190.,
                    "{}",
                    suspect.name
                );
            }
            // Edition quotes stay above the closing rule and campaign standing.
            let evidence = case.places[0].evidence;
            let denial = case.suspects[case.culprit].statement;
            assert!(lines(evidence, 398., 14.) <= 4, "{evidence}");
            assert!(lines(denial, 398., 14.) <= 4, "{denial}");
            for value in case.methods.iter().chain(&case.motives) {
                assert!(text_width(value, plain(20., DARK)) <= 440., "{value}");
            }
        }
    }
}
