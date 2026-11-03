//! Read-only illuminated council presentation.
//!
//! The left page holds the current petition (or briefing, works, chronicle);
//! the right page is the realm at a glance. Pointer hit regions live in
//! `model.rs`, and every clickable card here is drawn inside its region.
use crate::{
    Game,
    model::{
        ACT_NAMES, ACT_TEXT, LEGACY_NAMES, PROJECTS, ProjectState, RESOURCE_NAMES, Record, Screen,
    },
};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;

const FRAME_TOP: Color = Color::new(0.10, 0.13, 0.14, 1.);
const FRAME_LOW: Color = Color::new(0.06, 0.08, 0.09, 1.);
const VELLUM: Color = Color::new(0.93, 0.87, 0.70, 1.);
const VELLUM_SIDE: Color = Color::new(0.89, 0.81, 0.63, 1.);
const CARD_OFF: Color = Color::new(0.89, 0.815, 0.645, 1.);
const BROWN: Color = Color::new(0.19, 0.13, 0.11, 1.);
const FADED: Color = Color::new(0.45, 0.36, 0.27, 1.);
const GOLD: Color = Color::new(0.71, 0.50, 0.19, 1.);
const RUBY: Color = Color::new(0.57, 0.16, 0.18, 1.);
const TEAL: Color = Color::new(0.17, 0.35, 0.32, 1.);
const BLUE: Color = Color::new(0.20, 0.29, 0.43, 1.);
const CREAM: Color = Color::new(0.99, 0.94, 0.80, 1.);
const GAIN: Color = Color::new(0.12, 0.40, 0.30, 1.);
const SPEAKERS: [(&str, &str); 4] = [
    ("Iona", "Treasurer"),
    ("Mara", "Voice of the Commons"),
    ("Vey", "Captain of the Guard"),
    ("Sorel", "Abbot"),
];
const SHORT: [&str; 4] = ["Coin", "Folk", "Guard", "Faith"];
const WORKS: [&str; 4] = ["Granary", "Archive", "Muster", "Exchange"];
const COLORS: [Color; 4] = [GOLD, TEAL, BLUE, RUBY];
/// What a pillar reaching zero means, in the order of `RESOURCE_NAMES`.
const RUIN: [&str; 4] = [
    "Empties the treasury",
    "Provokes revolt",
    "The guard will mutiny",
    "The chapel will split",
];
const ACT_MARK: [&str; 3] = ["ACT I", "ACT II", "ACT III"];
/// Left page text column.
const LX: f32 = 44.;
/// Width of the decree and project cards (matches the pointer regions).
const CARD_W: f32 = 588.;
/// Body copy layout shared by drawing and the fit test.
const BODY_SIZE: f32 = 16.;
const BODY_X: f32 = 150.;
const BODY_W: f32 = 470.;
const CHOICE_SIZE: f32 = 16.;
const CHOICE_W: f32 = 500.;

fn shade(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new(r, g, b, a)
}

// ---------------------------------------------------------------- type

/// Dark ink on vellum needs no drop shadow (which would double glyph cost).
fn plain(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, plain(11., color).with_letter_spacing(1.5)).width
}
fn eyebrow_right(value: &str, right: f32, y: f32, color: Color) {
    let s = plain(11., color)
        .with_letter_spacing(1.5)
        .with_align(TextAlign::Right);
    draw_text_styled(value, right, y, s);
}
/// Draw a label shrunk (never enlarged) to `width`; returns its width.
fn line(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) -> f32 {
    let s = plain(size, color);
    let w = text_width(value, s);
    let s = if w > width && w > 0. {
        s.with_size(size * width / w)
    } else {
        s
    };
    draw_text_styled(value, x, y, s).width
}
fn line_right(value: &str, right: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(
        value,
        right,
        y,
        plain(size, color).with_align(TextAlign::Right),
    )
    .width
}
fn prose(value: &str, x: f32, y: f32, width: f32, size: f32, leading: f32, color: Color) -> f32 {
    let s = plain(size, color);
    let mut baseline = y;
    for row in wrap_text(value, width, s) {
        draw_text_styled(row, x, baseline, s);
        baseline += leading;
    }
    baseline
}
fn signed(value: i16) -> String {
    if value > 0 {
        format!("+{value}")
    } else if value < 0 {
        format!("\u{2212}{}", -value)
    } else {
        "0".into()
    }
}
fn delta_color(value: i16) -> Color {
    match value.signum() {
        1 => GAIN,
        -1 => RUBY,
        _ => FADED,
    }
}

// ---------------------------------------------------------------- surfaces

fn lip(r: Rect, radius: f32, depth: f32, alpha: f32) {
    let s = Rect::new(r.x, r.y + depth, r.w, r.h);
    shapes::fill_rounded_rect(s, radius, shade(0.12, 0.07, 0.03, alpha));
}
/// Small gilded lozenge marking a corner of the page's inner rule.
fn corner(x: f32, y: f32) {
    let d = 4.;
    draw_triangle(vec2(x - d, y), vec2(x, y - d), vec2(x + d, y), GOLD);
    draw_triangle(vec2(x - d, y), vec2(x, y + d), vec2(x + d, y), GOLD);
}
fn page(x: f32, y: f32, w: f32, h: f32, fill: Color) {
    let r = Rect::new(x, y, w, h);
    lip(r, 10., 4., 0.55);
    shapes::fill_rounded_rect(
        r,
        10.,
        Fill::vertical(r, lighten(fill, 0.05), darken(fill, 0.03)),
    );
    outline(
        x + 6.,
        y + 6.,
        w - 12.,
        h - 12.,
        6.,
        1.,
        with_alpha(GOLD, 0.55),
    );
    corner(x + 6., y + 6.);
    corner(x + w - 6., y + 6.);
    corner(x + 6., y + h - 6.);
    corner(x + w - 6., y + h - 6.);
}
fn parchment() {
    let frame = Rect::new(16., 112., 928., 456.);
    shapes::fill_rounded_rect(frame, 12., Fill::vertical(frame, FRAME_TOP, FRAME_LOW));
    page(24., 120., 614., 440., VELLUM);
    page(650., 120., 286., 440., VELLUM_SIDE);
}
/// The reign as a gilded rule: progress, act boundaries and this council.
fn reign_track(game: &Game) {
    let (left, right, y) = (54., 610., 545.);
    let span = right - left;
    let at = |n: usize| left + span * n as f32 / 30.;
    let rail = Rect::new(left, y - 1.5, span, 3.);
    shapes::fill_rounded_rect(rail, 1.5, with_alpha(GOLD, 0.35));
    if game.count > 0 {
        let done = Rect::new(left, y - 2., at(game.count) - left, 4.);
        shapes::fill_rounded_rect(done, 2., Fill::horizontal(done, lighten(TEAL, 0.2), TEAL));
    }
    for act in 1..3 {
        panel(at(act * 10) - 0.5, y - 6., 1., 12., with_alpha(GOLD, 0.8));
    }
    if game.count < 30 {
        let c = vec2(at(game.count) + span / 60., y);
        shapes::fill_circle(
            c,
            5.,
            Fill::radial(c - vec2(1.5, 1.5), 6., lighten(RUBY, 0.4), RUBY),
        );
    }
}
/// Illuminated initial: a gilded tile holding a numeral or letter.
fn initial(value: &str, x: f32, y: f32, w: f32, h: f32, size: f32, active: bool) {
    let r = Rect::new(x, y, w, h);
    if active {
        shapes::fill_rounded_rect(
            r,
            6.,
            Fill::vertical(r, lighten(RUBY, 0.12), darken(RUBY, 0.15)),
        );
        shapes::stroke_rounded_rect(r, 6., 1.5, GOLD);
    } else {
        shapes::fill_rounded_rect(r, 6., with_alpha(RUBY, 0.1));
    }
    let s = plain(size, if active { CREAM } else { RUBY }).with_align(TextAlign::Center);
    draw_text_styled(value, x + w * 0.5, s.centered_baseline(y + h * 0.5), s);
}
/// The wide teal call to action used by the briefing (pointer region
/// `42,483 588x54`).
fn seal_button(key: &str, value: &str) {
    let r = Rect::new(42., 483., CARD_W, 54.);
    lip(r, 10., 3., 0.3);
    shapes::fill_rounded_rect(
        r,
        10.,
        Fill::vertical(r, lighten(TEAL, 0.12), darken(TEAL, 0.1)),
    );
    shapes::stroke_rounded_rect(r, 10., 1., with_alpha(GOLD, 0.7));
    let kw = keycap(key, "", 62., 500., CREAM);
    let s = plain(19., CREAM).with_shadow(shade(0., 0., 0., 0.35));
    draw_text_styled(value, 62. + kw + 12., s.centered_baseline(510.), s);
}

// ---------------------------------------------------------------- portrait

fn portrait(id: usize, x: f32, y: f32, s: f32) {
    let skin = [
        shade(0.68, 0.43, 0.28, 1.),
        shade(0.84, 0.63, 0.43, 1.),
        shade(0.52, 0.35, 0.26, 1.),
        shade(0.85, 0.69, 0.49, 1.),
    ][id];
    let c = vec2(x, y);
    let back = shade(0.80, 0.66, 0.40, 1.);
    shapes::fill_circle(
        c,
        53. * s,
        Fill::radial(
            c - vec2(0., 18. * s),
            62. * s,
            lighten(back, 0.25),
            darken(back, 0.1),
        ),
    );
    draw_triangle(
        vec2(x - 40. * s, y + 40. * s),
        vec2(x + 40. * s, y + 40. * s),
        vec2(x, y - 10. * s),
        COLORS[id],
    );
    shapes::fill_circle(vec2(x, y - 8. * s), 25. * s, BROWN);
    shapes::fill_circle(vec2(x, y - 5. * s), 21. * s, skin);
    match id {
        0 => {
            panel(x - 26. * s, y - 26. * s, 52. * s, 10. * s, BROWN);
            rounded(x - 18. * s, y - 40. * s, 36. * s, 17. * s, 3. * s, RUBY);
            draw_line(
                x - 17. * s,
                y - 35. * s,
                x + 17. * s,
                y - 35. * s,
                2. * s,
                GOLD,
            );
        }
        1 => {
            shapes::fill_circle(vec2(x - 18. * s, y - 16. * s), 12. * s, BROWN);
            shapes::fill_circle(vec2(x + 18. * s, y - 16. * s), 12. * s, BROWN);
            draw_line(
                x - 16. * s,
                y - 24. * s,
                x + 17. * s,
                y - 24. * s,
                4. * s,
                TEAL,
            );
        }
        2 => {
            rounded(x - 25. * s, y - 30. * s, 50. * s, 15. * s, 4. * s, BLUE);
            panel(x - 28. * s, y - 17. * s, 56. * s, 5. * s, GOLD);
            panel(x + 20. * s, y - 13. * s, 6. * s, 31. * s, BLUE);
        }
        _ => {
            draw_triangle(
                vec2(x - 23. * s, y - 20. * s),
                vec2(x + 23. * s, y - 20. * s),
                vec2(x, y - 52. * s),
                RUBY,
            );
            draw_line(x, y - 48. * s, x, y - 23. * s, 3. * s, GOLD);
        }
    }
    shapes::fill_circle(vec2(x - 8. * s, y - 6. * s), 2. * s, BROWN);
    shapes::fill_circle(vec2(x + 8. * s, y - 6. * s), 2. * s, BROWN);
    draw_line(
        x - 1. * s,
        y - 3. * s,
        x - 3. * s,
        y + 5. * s,
        1.5 * s,
        BROWN,
    );
    draw_line(
        x - 6. * s,
        y + 10. * s,
        x + 6. * s,
        y + 10. * s,
        1.5 * s,
        BROWN,
    );
    if id == 3 {
        draw_triangle(
            vec2(x - 12. * s, y + 12. * s),
            vec2(x + 12. * s, y + 12. * s),
            vec2(x, y + 30. * s),
            CREAM,
        );
    }
    if id == 2 {
        draw_line(
            x - 14. * s,
            y + 3. * s,
            x - 8. * s,
            y + 12. * s,
            2. * s,
            RUBY,
        );
    }
    draw_line(x, y + 23. * s, x, y + 39. * s, 2. * s, GOLD);
    shapes::fill_circle(vec2(x, y + 29. * s), 4. * s, GOLD);
    shapes::stroke_circle(c, 53. * s, 2.5, GOLD);
}

// ---------------------------------------------------------------- realm

/// The right page: four pillars (with the selected decree's forecast),
/// policy leanings and royal works.
fn realm(game: &Game) {
    let preview = (game.screen == Screen::Council).then(|| game.forecast(game.selected));
    eyebrow("THE FOUR PILLARS", 670., 150., FADED);
    for i in 0..4 {
        let y = 178. + i as f32 * 52.;
        let value = game.resources[i];
        let danger = value <= 20;
        line(RESOURCE_NAMES[i], 670., y, 120., 15., BROWN);
        let shown = value.to_string();
        let vw = line_right(&shown, 914., y + 1., 18., if danger { RUBY } else { BROWN });
        if let Some(delta) = preview.map(|d| d[i]) {
            line_right(&signed(delta), 914. - vw - 8., y, 12., delta_color(delta));
        }
        let track = Rect::new(670., y + 9., 244., 9.);
        shapes::fill_rounded_rect(track, 4.5, darken(VELLUM_SIDE, 0.14));
        // Danger band: the bottom fifth of every pillar.
        shapes::fill_rounded_rect(
            Rect::new(670., y + 9., 244. * 0.2, 9.),
            4.5,
            with_alpha(RUBY, 0.18),
        );
        let fraction = f32::from(value.clamp(0, 100)) / 100.;
        if fraction > 0. {
            let fill = Rect::new(670., y + 9., (244. * fraction).max(9.), 9.);
            shapes::fill_rounded_rect(
                fill,
                4.5,
                Fill::vertical(fill, lighten(COLORS[i], 0.2), COLORS[i]),
            );
        }
        if let Some(delta) = preview.map(|d| d[i]) {
            let after = f32::from((value + delta).clamp(0, 100)) / 100.;
            let (a, b) = if after > fraction {
                (fraction, after)
            } else {
                (after, fraction)
            };
            if b - a > 0.004 {
                let seg = Rect::new(670. + 244. * a, y + 9., 244. * (b - a), 9.);
                let tint = if after > fraction {
                    with_alpha(lighten(COLORS[i], 0.35), 0.55)
                } else {
                    with_alpha(VELLUM_SIDE, 0.7)
                };
                shapes::fill_rounded_rect(seg, 2., tint);
            }
        }
    }
    divider(670., 370., 244., with_alpha(GOLD, 0.6));
    eyebrow("POLICY LEANINGS", 670., 392., FADED);
    let lead = (0..4).max_by_key(|&i| (game.legacy[i], std::cmp::Reverse(i)));
    for (i, name) in LEGACY_NAMES.iter().enumerate() {
        let x = 670. + (i % 2) as f32 * 132.;
        let y = 416. + (i / 2) as f32 * 22.;
        let leading = lead == Some(i) && game.legacy[i] > 0;
        line(name, x, y, 84., 13., if leading { RUBY } else { FADED });
        line_right(
            count(game.legacy[i]),
            x + 106.,
            y,
            14.,
            if leading { RUBY } else { BROWN },
        );
    }
    divider(670., 452., 244., with_alpha(GOLD, 0.6));
    eyebrow("ROYAL WORKS", 670., 474., FADED);
    for (i, work) in WORKS.iter().enumerate() {
        let x = 670. + i as f32 * 62.;
        let r = Rect::new(x, 484., 56., 50.);
        let (fill, state, tone): (Color, String, Color) = match game.projects[i] {
            ProjectState::Complete => (with_alpha(GOLD, 0.28), "\u{2713} Built".into(), BROWN),
            ProjectState::Building(n) => (with_alpha(TEAL, 0.16), format!("{n} left"), TEAL),
            ProjectState::Unbuilt => (with_alpha(BROWN, 0.06), "\u{2014}".into(), FADED),
        };
        shapes::fill_rounded_rect(r, 6., fill);
        let s = plain(11., BROWN).with_align(TextAlign::Center);
        let s = if text_width(work, s) > 50. {
            s.with_size(11. * 50. / text_width(work, s))
        } else {
            s
        };
        draw_text_styled(work, x + 28., 503., s);
        let t = plain(11., tone).with_align(TextAlign::Center);
        draw_text_styled(&state, x + 28., 523., t);
    }
}
fn count(value: u8) -> &'static str {
    const N: [&str; 31] = [
        "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
        "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "30",
    ];
    N[usize::from(value.min(30))]
}

// ---------------------------------------------------------------- left page

/// Petition header: speaker medallion, petition number, title and body.
fn petition_header(eyebrow_text: &str, speaker: usize, title: &str, body: &str) {
    eyebrow(eyebrow_text, LX, 150., FADED);
    portrait(speaker, 92., 222., 0.82);
    let (name, role) = SPEAKERS[speaker];
    let tag = plain(11., COLORS[speaker]).with_letter_spacing(1.5);
    let w = draw_text_styled(&name.to_uppercase(), BODY_X, 196., tag).width;
    line(role, BODY_X + w + 10., 196., BODY_W - w - 10., 13., FADED);
    line(title, BODY_X, 228., BODY_W, 24., BROWN);
    prose(body, BODY_X, 256., BODY_W, BODY_SIZE, 22., BROWN);
}
fn council(game: &Game) {
    let p = game.current();
    let number = format!("PETITION {} OF 30", game.count + 1);
    petition_header(&number, p.speaker, p.title, p.body);
    eyebrow_right(ACT_MARK[game.act()], 620., 150., RUBY);
    for (i, choice) in p.choices.iter().enumerate() {
        let y = 303. + i as f32 * 76.;
        let r = Rect::new(42., y, CARD_W, 70.);
        let on = game.selected == i;
        if on {
            lip(r, 9., 3., 0.22);
            shapes::fill_rounded_rect(r, 9., Fill::vertical(r, shade(1., 0.97, 0.87, 1.), CREAM));
            shapes::stroke_rounded_rect(r, 9., 2., TEAL);
        } else {
            shapes::fill_rounded_rect(r, 9., CARD_OFF);
        }
        initial(["1", "2", "3"][i], 54., y + 17., 34., 36., 20., on);
        line(choice.label, 102., y + 30., CHOICE_W, CHOICE_SIZE, BROWN);
        let forecast = game.forecast(i);
        let legacy = choice.legacy;
        // A decree that empties a pillar ends the reign: say so in place of
        // its policy tag.
        if let Some(pillar) = (0..4).find(|&k| game.resources[k] + forecast[k] <= 0) {
            let warn = format!("\u{25b2} {}", RUIN[pillar]);
            line(&warn, 102., y + 54., 200., 13., RUBY);
        } else {
            shapes::fill_circle(vec2(106., y + 49.5), 3.5, COLORS[legacy]);
            line(LEGACY_NAMES[legacy], 115., y + 54., 120., 13., FADED);
        }
        // Forecast ledger along the bottom right of the card.
        for (k, name) in SHORT.iter().enumerate() {
            let x = 300. + k as f32 * 81.;
            let w = line(name, x, y + 54., 40., 12., FADED);
            line(
                &signed(forecast[k]),
                x + w + 5.,
                y + 54.,
                30.,
                14.,
                delta_color(forecast[k]),
            );
        }
    }
}
fn brief(game: &Game) {
    let act = game.act();
    eyebrow("A NEW ACT BEGINS", LX, 152., RUBY);
    line(ACT_NAMES[act], LX, 200., 300., 40., BROWN);
    let range = [
        "Councils 1 \u{2013} 10",
        "Councils 11 \u{2013} 20",
        "Councils 21 \u{2013} 30",
    ][act];
    line_right(range, 620., 200., 15., FADED);
    divider(LX, 218., 576., with_alpha(GOLD, 0.8));
    prose(ACT_TEXT[act], LX, 252., 576., 18., 27., BROWN);
    crown(331., 400.);
    seal_button("Enter", "Open the council");
}
/// A gilded crown emblem for the act briefing.
fn crown(x: f32, y: f32) {
    let base = y + 12.;
    // Alternating peaks and valleys form one continuous silhouette.
    let top = [
        vec2(x - 56., base - 40.),
        vec2(x - 40., base - 14.),
        vec2(x - 26., base - 34.),
        vec2(x - 12., base - 12.),
        vec2(x, base - 50.),
        vec2(x + 12., base - 12.),
        vec2(x + 26., base - 34.),
        vec2(x + 40., base - 14.),
        vec2(x + 56., base - 40.),
    ];
    shapes::fill_rounded_rect(
        Rect::new(x - 60., base + 26., 120., 6.),
        3.,
        shade(0.12, 0.07, 0.03, 0.1),
    );
    for pair in top.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let tone = if a.y < b.y { lighten(GOLD, 0.12) } else { GOLD };
        draw_triangle(vec2(a.x, base), a, b, tone);
        draw_triangle(vec2(a.x, base), b, vec2(b.x, base), tone);
    }
    for (i, p) in top.iter().enumerate().filter(|(i, _)| i % 2 == 0) {
        let gem = if i == 4 { RUBY } else { lighten(GOLD, 0.45) };
        shapes::fill_circle(*p, if i == 4 { 5.5 } else { 4. }, gem);
    }
    let band = Rect::new(x - 58., base - 2., 116., 24.);
    shapes::fill_rounded_rect(
        band,
        5.,
        Fill::vertical(band, lighten(GOLD, 0.35), darken(GOLD, 0.18)),
    );
    for (i, gem) in [TEAL, RUBY, BLUE, RUBY, TEAL].iter().enumerate() {
        let c = vec2(x - 40. + i as f32 * 20., base + 10.);
        shapes::fill_circle(
            c,
            5.,
            Fill::radial(c - vec2(1.5, 1.5), 6., lighten(*gem, 0.45), *gem),
        );
    }
}
fn report(game: &Game, record: Record, index: usize) {
    let mut number = format!("CHRONICLE \u{b7} DECREE {} OF {}", index + 1, game.count);
    if index + 1 == game.count {
        number.push_str(" \u{b7} LATEST");
    }
    let choice = record.petition.choices[record.choice];
    petition_header(
        &number,
        record.petition.speaker,
        record.petition.title,
        record.petition.body,
    );
    eyebrow_right(ACT_MARK[(index / 10).min(2)], 620., 150., RUBY);
    divider(LX, 296., 576., with_alpha(GOLD, 0.8));
    eyebrow("THE CROWN DECREED", LX, 322., RUBY);
    line(choice.label, LX, 350., 576., 19., BROWN);
    shapes::fill_circle(vec2(LX + 4., 369.), 3.5, COLORS[choice.legacy]);
    line(
        LEGACY_NAMES[choice.legacy],
        LX + 14.,
        374.,
        160.,
        13.,
        FADED,
    );
    for (i, name) in RESOURCE_NAMES.iter().enumerate() {
        let x = LX + (i % 2) as f32 * 292.;
        let y = 408. + (i / 2) as f32 * 30.;
        line(name, x, y, 100., 15., BROWN);
        let before = record.before[i];
        let after = record.after[i];
        let text = format!("{before} \u{2192} {after}");
        line_right(
            &text,
            x + 210.,
            y,
            15.,
            if after <= 20 { RUBY } else { BROWN },
        );
        line_right(
            &signed(after - before),
            x + 262.,
            y,
            13.,
            delta_color(after - before),
        );
    }
    if record.completed != 0 {
        let names = PROJECTS
            .iter()
            .enumerate()
            .filter(|(i, _)| record.completed & (1 << i) != 0)
            .map(|(i, _)| WORKS[i])
            .collect::<Vec<_>>()
            .join(", ");
        chip(&format!("\u{2713} Completed: {names}"), LX, 458., 12., TEAL);
    }
    let mut x = LX;
    for (key, action) in [
        ("\u{2190}\u{2192}", "Browse"),
        ("Enter", "Return"),
        ("C", "Close"),
    ] {
        x += keycap(key, action, x, 500., BROWN) + 18.;
    }
}
fn projects(game: &Game) {
    eyebrow("ROYAL WORKS", LX, 150., RUBY);
    line(
        "Costs are paid now; returns arrive later.",
        LX,
        170.,
        330.,
        13.,
        FADED,
    );
    let mut x = 620.;
    for (key, action) in [("X", "Return"), ("1-4", "Invest")] {
        let w = keycap_width(key, action);
        x -= w;
        keycap(key, action, x, 146., BROWN);
        x -= 14.;
    }
    for (i, p) in PROJECTS.iter().enumerate() {
        let y = 183. + i as f32 * 83.;
        let r = Rect::new(42., y, CARD_W, 77.);
        let on = game.selected == i;
        let state = game.projects[i];
        if on {
            lip(r, 9., 3., 0.22);
            shapes::fill_rounded_rect(r, 9., Fill::vertical(r, shade(1., 0.97, 0.87, 1.), CREAM));
            shapes::stroke_rounded_rect(r, 9., 2., TEAL);
        } else {
            shapes::fill_rounded_rect(r, 9., CARD_OFF);
        }
        initial(["1", "2", "3", "4"][i], 54., y + 20., 34., 36., 20., on);
        line(p.name, 102., y + 26., 300., 17., BROWN);
        line(p.body, 102., y + 46., 300., 13., FADED);
        for (k, name) in SHORT.iter().enumerate() {
            let x = 102. + k as f32 * 82.;
            let w = line(name, x, y + 66., 44., 12., FADED);
            line(
                &signed(p.reward[k]),
                x + w + 6.,
                y + 66.,
                30.,
                13.,
                delta_color(p.reward[k]),
            );
        }
        let right = 616.;
        match state {
            ProjectState::Complete => {
                let w = text_width("\u{2713} Complete", plain(12., GOLD)) + 16.;
                chip(
                    "\u{2713} Complete",
                    right - w,
                    y + 12.,
                    12.,
                    darken(GOLD, 0.2),
                );
            }
            ProjectState::Building(n) => {
                let label = format!("{n} councils left");
                line_right(&label, right, y + 26., 13., TEAL);
                let total = f32::from(p.duration.max(1));
                let done = (total - f32::from(n)) / total;
                let track = Rect::new(right - 120., y + 36., 120., 6.);
                shapes::fill_rounded_rect(track, 3., with_alpha(TEAL, 0.15));
                if done > 0. {
                    shapes::fill_rounded_rect(
                        Rect::new(track.x, track.y, (track.w * done).max(6.), 6.),
                        3.,
                        TEAL,
                    );
                }
            }
            ProjectState::Unbuilt => {
                let ok = game.affordable(i);
                line_right(
                    &format!("{} coin \u{b7} {} councils", p.cost, p.duration),
                    right,
                    y + 26.,
                    13.,
                    if ok { TEAL } else { RUBY },
                );
                line_right(
                    if ok { "Available" } else { "Unavailable" },
                    right,
                    y + 46.,
                    12.,
                    if ok { FADED } else { RUBY },
                );
            }
        }
    }
    line(
        "One commission each. Zero in any pillar ends the reign.",
        LX,
        530.,
        576.,
        12.,
        RUBY,
    );
}
fn keycap_width(key: &str, action: &str) -> f32 {
    let s = plain(13., PAPER);
    let cap = (text_width(key, s) + 12.).max(20.);
    if action.is_empty() {
        cap
    } else {
        cap + 7. + text_width(action, s)
    }
}
impl ArcadeGame for Game {
    fn new() -> Self {
        Game::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.step(input)
    }
    fn draw(&self) {
        parchment();
        realm(self);
        match self.screen {
            Screen::Briefing => brief(self),
            Screen::Council => council(self),
            Screen::Projects => projects(self),
            Screen::Chronicle => {
                if let Some(record) = self.history[self.page] {
                    report(self, record, self.page);
                }
            }
        }
        if self.screen != Screen::Projects {
            reign_track(self);
        }
    }
    fn hud(&self) -> Hud {
        let status = match self.ending() {
            Some(ending) => format!(
                "{}. Grade {}. {}",
                ending.name(),
                self.grade(),
                ending.story()
            ),
            None => format!(
                "{} / {} works complete",
                match self.screen {
                    Screen::Briefing => "Read the new act",
                    Screen::Council => "Choose a decree",
                    Screen::Projects => "Commission a project",
                    Screen::Chronicle => "Browse past decrees",
                },
                self.finished_projects()
            ),
        };
        Hud {
            stage: format!(
                "Act {} \u{b7} Council {} of 30",
                self.act() + 1,
                (self.count + 1).min(30)
            ),
            objective: "Keep every pillar above zero for 30 decisions.".into(),
            status,
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        Game::outcome(self)
    }
    fn summary(&self) -> String {
        Game::summary(self)
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::petition;
    #[test]
    fn all_authored_petitions_fit_the_allocated_reading_area() {
        for flags in [0, 127] {
            for turn in 0..30 {
                let p = petition(turn, flags);
                // Body: at most two lines between the title and the first card.
                let lines = wrap_text(p.body, BODY_W, plain(BODY_SIZE, BROWN)).count();
                assert!(
                    256. + (lines.saturating_sub(1) as f32) * 22. < 290.,
                    "{}: {lines} lines",
                    p.title
                );
                assert!(
                    text_width(p.title, plain(24., BROWN)) <= BODY_W,
                    "{}",
                    p.title
                );
                for choice in p.choices {
                    // Decree labels stay on one line above the forecast ledger.
                    let w = text_width(choice.label, plain(CHOICE_SIZE, BROWN));
                    assert!(w <= CHOICE_W, "{}: {w}", choice.label);
                    assert!(text_width(choice.label, plain(19., BROWN)) <= 576. * 1.05);
                }
            }
        }
        for body in ACT_TEXT {
            let lines = wrap_text(body, 576., plain(18., BROWN)).count();
            assert!(252. + lines as f32 * 27. < 330.);
        }
    }
}
