//! Read-only illuminated council presentation.
//!
//! The left page holds the current petition (or the act briefing, the royal
//! works, the chronicle); the right page is the realm at a glance (see
//! `realm.rs`). Words are kept to proper names, the petition's question and
//! its three decrees: everything else is an icon, a meter, a seal or a pip.
//! Pointer regions live in `model.rs`; everything clickable here is drawn
//! inside its region and lifts under the pointer.
use crate::{
    Game,
    content::{CHARTER, COURTS, LEDGER, LEVEE, PEACE, PETITIONS, SANCTUARY, SCHOOLS, echo},
    feedback,
    model::{
        ACT_NAMES, BEGIN, CHRONICLE_BUTTON, PAGE_NEXT, PAGE_PREV, PROJECTS, ProjectState,
        RESOURCE_NAMES, Record, Refusal, Screen, WORKS_BUTTON, choice_rect, project_rect,
    },
    realm,
};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;

pub(crate) const FRAME_TOP: Color = Color::new(0.10, 0.13, 0.14, 1.);
pub(crate) const FRAME_LOW: Color = Color::new(0.06, 0.08, 0.09, 1.);
pub(crate) const VELLUM: Color = Color::new(0.93, 0.87, 0.70, 1.);
pub(crate) const VELLUM_SIDE: Color = Color::new(0.89, 0.81, 0.63, 1.);
const CARD_OFF: Color = Color::new(0.89, 0.815, 0.645, 1.);
pub(crate) const BROWN: Color = Color::new(0.19, 0.13, 0.11, 1.);
pub(crate) const FADED: Color = Color::new(0.45, 0.36, 0.27, 1.);
/// The petitioner's question: softer than a title, firmer than a caption.
const QUESTION: Color = Color::new(0.33, 0.24, 0.18, 1.);
pub(crate) const GOLD: Color = Color::new(0.71, 0.50, 0.19, 1.);
const RUBY: Color = Color::new(0.57, 0.16, 0.18, 1.);
pub(crate) const TEAL: Color = Color::new(0.17, 0.35, 0.32, 1.);
const BLUE: Color = Color::new(0.20, 0.29, 0.43, 1.);
pub(crate) const CREAM: Color = Color::new(0.99, 0.94, 0.80, 1.);
/// A rise, a completed work.
pub(crate) const GAIN: Color = Color::new(0.15, 0.50, 0.24, 1.);
/// A fall, a refusal, danger.
pub(crate) const LOSS: Color = Color::new(0.76, 0.17, 0.14, 1.);

/// The four pillars in resource order: treasury, commons, guard, chapel.
pub(crate) const PILLARS: [(Icon, Color); 4] = [
    (Icon::Coin, Color::new(0.80, 0.56, 0.16, 1.)),
    (Icon::People, Color::new(0.16, 0.47, 0.43, 1.)),
    (Icon::Shield, Color::new(0.25, 0.39, 0.63, 1.)),
    (Icon::Bell, Color::new(0.51, 0.31, 0.62, 1.)),
];
/// Policy seals in legacy order: Charter, Iron, Commerce, Concordat.
pub(crate) const POLICIES: [(Icon, Color); 4] = [
    (Icon::Scroll, Color::new(0.22, 0.35, 0.66, 1.)),
    (Icon::Sword, Color::new(0.36, 0.39, 0.45, 1.)),
    (Icon::Ship, Color::new(0.73, 0.45, 0.12, 1.)),
    (Icon::Flame, Color::new(0.58, 0.18, 0.35, 1.)),
];
/// Royal works: granary, archive, muster house, river exchange.
pub(crate) const WORK_ICONS: [Icon; 4] = [Icon::Bag, Icon::Book, Icon::Home, Icon::Anchor];
/// Named promises in flag-bit order, with the toast shown when one is made.
pub(crate) const PROMISES: [(u16, Icon, &str); 7] = [
    (LEVEE, Icon::Wave, "Crown's Reach"),
    (CHARTER, Icon::Crate, "The Salt Charter"),
    (SANCTUARY, Icon::Tree, "The Orchard Oath"),
    (LEDGER, Icon::Eye, "The Open Ledger"),
    (COURTS, Icon::Hand, "The Common Bench"),
    (PEACE, Icon::Flag, "The White Flag Compact"),
    (SCHOOLS, Icon::Bulb, "The Lantern Schools"),
];
pub(crate) const ACT_MARK: [&str; 3] = ["ACT I", "ACT II", "ACT III"];
const NAMES: [&str; 4] = ["IONA", "MARA", "VEY", "SOREL"];

/// Petition header layout shared by drawing and the fit test.
const HEAD_X: f32 = 156.;
const TITLE_SIZE: f32 = 24.;
const TITLE_W: f32 = 466.;
const QUESTION_SIZE: f32 = 16.;
const QUESTION_W: f32 = 470.;
/// Decree text, kept clear of the wax seal at the card's right end.
const CHOICE_X: f32 = 102.;
const CHOICE_SIZE: f32 = 16.;
const CHOICE_W: f32 = 466.;
/// Centre of each decree card's wax seal.
pub(crate) const SEAL_X: f32 = 598.;
/// Delta chips (pillar icon, arrow, size).
pub(crate) const CHIP_W: f32 = 64.;
const CHIP_H: f32 = 22.;
const CHIP_PITCH: f32 = 70.;
/// Centre of each project card's state badge.
pub(crate) const BADGE_X: f32 = 598.;

/// Allocation-free labels for the small numbers this game draws (0..=100).
pub(crate) fn num(value: i32) -> &'static str {
    const N: [&str; 101] = [
        "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16",
        "17", "18", "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "29", "30", "31",
        "32", "33", "34", "35", "36", "37", "38", "39", "40", "41", "42", "43", "44", "45", "46",
        "47", "48", "49", "50", "51", "52", "53", "54", "55", "56", "57", "58", "59", "60", "61",
        "62", "63", "64", "65", "66", "67", "68", "69", "70", "71", "72", "73", "74", "75", "76",
        "77", "78", "79", "80", "81", "82", "83", "84", "85", "86", "87", "88", "89", "90", "91",
        "92", "93", "94", "95", "96", "97", "98", "99", "100",
    ];
    N[value.clamp(0, 100) as usize]
}
/// The promise drawn for a flag bit.
pub(crate) fn promise(flag: u16) -> (Icon, &'static str) {
    let (_, glyph, name) = PROMISES[(flag.trailing_zeros() as usize).min(6)];
    (glyph, name)
}

// ---------------------------------------------------------------- type

/// Dark ink on vellum needs no drop shadow (which would double glyph cost).
pub(crate) fn plain(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, plain(12., color).with_letter_spacing(1.8)).width
}
/// Draw a label shrunk (never enlarged) to `width`; returns its width.
pub(crate) fn line(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) -> f32 {
    let s = plain(size, color);
    let w = text_width(value, s);
    let s = if w > width && w > 0. {
        s.with_size(size * width / w)
    } else {
        s
    };
    draw_text_styled(value, x, y, s).width
}
pub(crate) fn centered(value: &str, cx: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(
        value,
        cx,
        y,
        plain(size, color).with_align(TextAlign::Center),
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

// ---------------------------------------------------------------- vocabulary

/// A direction arrow that grows with the size of the change; a dash for none.
pub(crate) fn arrow(cx: f32, cy: f32, delta: i16, color: Color) {
    let s = match delta.unsigned_abs() {
        0 => {
            shapes::fill_rounded_rect(Rect::new(cx - 4., cy - 1., 8., 2.), 1., color);
            return;
        }
        1..=5 => 3.2,
        6..=11 => 4.2,
        _ => 5.3,
    };
    let dir = if delta > 0 { -1. } else { 1. };
    shapes::fill_triangle(
        vec2(cx, cy + dir * s),
        vec2(cx - s, cy - dir * s * 0.8),
        vec2(cx + s, cy - dir * s * 0.8),
        color,
    );
}
/// A pillar change: the pillar's icon, a sized arrow and the amount. A fatal
/// change shows a skull and a throbbing red rim instead of the arrow.
pub(crate) fn delta_chip(k: usize, delta: i16, x: f32, y: f32, fatal: bool, alpha: f32) {
    let (glyph, tint) = PILLARS[k];
    let tone = if fatal || delta < 0 {
        LOSS
    } else if delta > 0 {
        GAIN
    } else {
        FADED
    };
    let r = Rect::new(x, y, CHIP_W, CHIP_H);
    let mid = y + CHIP_H * 0.5;
    shapes::fill_rounded_rect(r, CHIP_H * 0.5, with_alpha(tone, 0.13 * alpha));
    if fatal {
        let throb = pulse(clock(), 1.4);
        shapes::stroke_rounded_rect(
            r,
            CHIP_H * 0.5,
            1.5,
            with_alpha(LOSS, (0.55 + 0.45 * throb) * alpha),
        );
        icons::draw(
            Icon::Skull,
            vec2(x + 31., mid),
            13.,
            with_alpha(LOSS, alpha),
        );
    } else {
        // No change is a lone dash; otherwise an arrow sized by the amount.
        let at = if delta == 0 { x + 38. } else { x + 30. };
        arrow(at, mid, delta, with_alpha(tone, alpha));
    }
    icons::draw(glyph, vec2(x + 13., mid), 15., with_alpha(tint, alpha));
    if delta != 0 || fatal {
        let s = plain(14., with_alpha(tone, alpha));
        draw_text_styled(
            num(i32::from(delta.unsigned_abs())),
            x + 38.,
            s.centered_baseline(mid),
            s,
        );
    }
}
/// A policy's wax seal: a rippled disc in the policy's color with its icon.
pub(crate) fn seal(policy: usize, cx: f32, cy: f32, r: f32) {
    let (glyph, color) = POLICIES[policy];
    let c = vec2(cx, cy);
    if r >= 10. {
        let wax = darken(color, 0.12);
        for i in 0..11 {
            let a = i as f32 * std::f32::consts::TAU / 11. + 0.4;
            shapes::fill_circle(c + vec2(a.cos(), a.sin()) * r * 0.9, r * 0.2, wax);
        }
    }
    shapes::fill_circle(
        c,
        r,
        Fill::radial(
            c - vec2(r * 0.3, r * 0.35),
            r * 1.4,
            lighten(color, 0.3),
            darken(color, 0.12),
        ),
    );
    if r >= 10. {
        shapes::stroke_circle(c, r * 0.76, 1.2, with_alpha(lighten(color, 0.5), 0.5));
    }
    icons::draw(glyph, c, r * 1.05, CREAM);
}
/// A promise medallion: gold and lit once made, an empty socket before.
pub(crate) fn promise_badge(flag: u16, cx: f32, cy: f32, d: f32, made: bool) {
    let (glyph, _) = promise(flag);
    if made {
        icon_badge(glyph, cx, cy, d, GOLD, CREAM);
        shapes::stroke_circle(vec2(cx, cy), d * 0.5, 1.5, lighten(GOLD, 0.35));
    } else {
        shapes::stroke_circle(vec2(cx, cy), d * 0.5 - 1., 1.5, with_alpha(FADED, 0.45));
        icons::draw(glyph, vec2(cx, cy), d * 0.55, with_alpha(FADED, 0.35));
    }
}

// ---------------------------------------------------------------- surfaces

fn lip(r: Rect, radius: f32, depth: f32, alpha: f32) {
    let s = Rect::new(r.x, r.y + depth, r.w, r.h);
    shapes::fill_rounded_rect(s, radius, Color::new(0.12, 0.07, 0.03, alpha));
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
/// A card's surface: selected cards lift in cream with a breathing teal rim,
/// hovered ones rise a little further with a gilt rim, alarming ones throb red.
fn card_surface(r: Rect, selected: bool, hovered: bool, alarm: bool) {
    let t = clock();
    if selected || hovered {
        lip(r, 9., if hovered { 4. } else { 3. }, 0.24);
        shapes::fill_rounded_rect(
            r,
            9.,
            Fill::vertical(r, Color::new(1., 0.97, 0.87, 1.), CREAM),
        );
    } else {
        shapes::fill_rounded_rect(r, 9., CARD_OFF);
    }
    if alarm {
        shapes::stroke_rounded_rect(r, 9., 2., with_alpha(LOSS, 0.45 + 0.55 * pulse(t, 1.4)));
    } else if selected {
        shapes::stroke_rounded_rect(r, 9., 2., with_alpha(TEAL, 0.65 + 0.35 * pulse(t, 0.7)));
    } else if hovered {
        shapes::stroke_rounded_rect(r, 9., 1.5, GOLD);
    }
}
/// Illuminated initial: a gilded tile holding the card's number key.
fn initial(value: &str, x: f32, y: f32, active: bool) {
    let (w, h) = (34., 36.);
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
    let s = plain(20., if active { CREAM } else { RUBY }).with_align(TextAlign::Center);
    draw_text_styled(value, x + w * 0.5, s.centered_baseline(y + h * 0.5), s);
}
/// A key-cap button with an icon (works, chronicle, page turners).
fn tool_button(r: Rect, key: &str, glyph: Icon, active: bool, enabled: bool) {
    let hovered = enabled && r.contains(pointer());
    let b = Rect::new(r.x, r.y - if hovered { 1.5 } else { 0. }, r.w, r.h);
    if active {
        shapes::fill_rounded_rect(b, 8., with_alpha(TEAL, 0.2));
        shapes::stroke_rounded_rect(b, 8., 1.5, with_alpha(TEAL, 0.8));
    } else if hovered {
        lip(b, 8., 2.5, 0.2);
        shapes::fill_rounded_rect(b, 8., CREAM);
        shapes::stroke_rounded_rect(b, 8., 1.5, GOLD);
    } else {
        shapes::fill_rounded_rect(b, 8., with_alpha(BROWN, 0.08));
    }
    let ink = if active { TEAL } else { BROWN };
    if key.is_empty() {
        icons::draw(glyph, vec2(b.x + b.w * 0.5, b.y + b.h * 0.5), 18., ink);
    } else {
        keycap(key, "", b.x + 5., b.y + 4., BROWN);
        icons::draw(glyph, vec2(b.x + b.w - 14., b.y + b.h * 0.5), 18., ink);
    }
    if !enabled {
        shapes::fill_rounded_rect(b, 8., with_alpha(VELLUM, 0.6));
    }
}

// ---------------------------------------------------------------- portrait

fn portrait(id: usize, x: f32, y: f32, s: f32) {
    let skin = [
        Color::new(0.68, 0.43, 0.28, 1.),
        Color::new(0.84, 0.63, 0.43, 1.),
        Color::new(0.52, 0.35, 0.26, 1.),
        Color::new(0.85, 0.69, 0.49, 1.),
    ][id];
    let robe = PILLARS[id].1;
    let c = vec2(x, y);
    let back = Color::new(0.80, 0.66, 0.40, 1.);
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
        robe,
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

// ---------------------------------------------------------------- left page

/// Speaker medallion (with their pillar pinned to it and, when a promise
/// changed this petition, that promise's medal), name, title and question.
fn petition_header(speaker: usize, title: &str, question: &str, echoed: Option<u16>) {
    let t = clock();
    let (cx, cy) = (96., 208. + bob(t, 0.3, 1.2));
    portrait(speaker, cx, cy, 0.84);
    let (glyph, color) = PILLARS[speaker];
    shapes::fill_circle(vec2(cx + 33., cy + 33.), 15., VELLUM);
    icon_badge(glyph, cx + 33., cy + 33., 26., color, CREAM);
    if let Some(flag) = echoed {
        let at = vec2(cx + 33., cy - 33.);
        shapes::glow_disc(at, 22., with_alpha(GOLD, 0.25 + 0.2 * pulse(t, 0.6)));
        shapes::fill_circle(at, 15., VELLUM);
        promise_badge(flag, at.x, at.y, 26., true);
    }
    eyebrow(NAMES[speaker], HEAD_X, 176., darken(color, 0.15));
    line(title, HEAD_X, 210., TITLE_W, TITLE_SIZE, BROWN);
    prose(
        question,
        HEAD_X,
        240.,
        QUESTION_W,
        QUESTION_SIZE,
        21.,
        QUESTION,
    );
}
/// The three decrees: each card is its sentence, the four pillar changes and
/// the policy's wax seal. A decree that would empty a pillar throbs red.
fn council(game: &Game) {
    let p = game.current();
    petition_header(p.speaker, p.title, p.body, echo(game.count, game.flags));
    let hover = hovered(game);
    for (i, choice) in p.choices.iter().enumerate() {
        let forecast = game.forecast(i);
        let fatal = (0..4).find(|&k| game.resources[k] + forecast[k] <= 0);
        let on = hover == Some(i);
        let base = choice_rect(i);
        let r = Rect::new(base.x, base.y - if on { 2. } else { 0. }, base.w, base.h);
        card_surface(r, game.selected == i, on, fatal.is_some());
        initial(num(i as i32 + 1), 54., r.y + 17., game.selected == i || on);
        line(
            choice.label,
            CHOICE_X,
            r.y + 27.,
            CHOICE_W,
            CHOICE_SIZE,
            BROWN,
        );
        for (k, delta) in forecast.into_iter().enumerate() {
            let x = CHOICE_X + k as f32 * CHIP_PITCH;
            delta_chip(k, delta, x, r.y + 38., fatal == Some(k), 1.);
        }
        seal(choice.legacy, SEAL_X, r.y + 35., 19.);
    }
    // After a decree the next petition inks onto the page.
    if game.count > 0 && game.since < INK_TICKS {
        let blank = 1. - f32::from(game.since) / f32::from(INK_TICKS);
        let page = Rect::new(31., 168., 600., 386.);
        shapes::fill_rounded_rect(page, 6., with_alpha(VELLUM, 0.85 * blank * blank));
    }
}
/// Executed ticks for a new petition to ink onto the page.
const INK_TICKS: u16 = 14;
/// Royal works as equations: pay coin, wait, receive.
fn projects(game: &Game) {
    icon_badge(Icon::Hammer, 57., 148., 28., GOLD, CREAM);
    line("Royal Works", 79., 155., 240., 20., BROWN);
    let hover = hovered(game);
    let t = clock();
    for (j, p) in PROJECTS.iter().enumerate() {
        let state = game.projects[j];
        let refusal = game.refusal(j);
        let on = hover == Some(j);
        let base = project_rect(j);
        let r = Rect::new(base.x, base.y - if on { 2. } else { 0. }, base.w, base.h);
        card_surface(r, game.selected == j, on, false);
        initial(num(j as i32 + 1), 54., r.y + 20., game.selected == j || on);
        let ready = state == ProjectState::Unbuilt;
        let tint = match state {
            ProjectState::Complete => GOLD,
            ProjectState::Building(_) => TEAL,
            ProjectState::Unbuilt if refusal.is_some() => with_alpha(FADED, 0.55),
            ProjectState::Unbuilt => darken(GOLD, 0.15),
        };
        icon_badge(WORK_ICONS[j], 121., r.y + 38., 40., tint, CREAM);
        line(p.name, 152., r.y + 29., 290., 17., BROWN);
        // Pay now, wait, then receive.
        let fade = if ready { 1. } else { 0.4 };
        let y = r.y + 42.;
        delta_chip(0, -p.cost, 152., y, false, fade);
        if refusal == Some(Refusal::Coin) {
            shapes::stroke_rounded_rect(
                Rect::new(152., y, CHIP_W, CHIP_H),
                CHIP_H * 0.5,
                1.5,
                LOSS,
            );
        }
        let wait = Rect::new(222., y, 48., CHIP_H);
        let late = refusal == Some(Refusal::Late);
        shapes::fill_rounded_rect(
            wait,
            CHIP_H * 0.5,
            with_alpha(if late { LOSS } else { TEAL }, 0.12 * fade),
        );
        if late {
            shapes::stroke_rounded_rect(wait, CHIP_H * 0.5, 1.5, LOSS);
        }
        icons::draw(
            Icon::Hourglass,
            vec2(wait.x + 13., y + CHIP_H * 0.5),
            14.,
            with_alpha(TEAL, fade),
        );
        let s = plain(14., with_alpha(TEAL, fade));
        draw_text_styled(
            num(i32::from(p.duration)),
            wait.x + 25.,
            s.centered_baseline(y + CHIP_H * 0.5),
            s,
        );
        icons::draw(
            Icon::ArrowRight,
            vec2(286., y + CHIP_H * 0.5),
            14.,
            with_alpha(FADED, 0.8),
        );
        let mut x = 300.;
        for (k, reward) in p.reward.into_iter().enumerate() {
            if reward != 0 {
                delta_chip(k, reward, x, y, false, 1.);
                x += CHIP_PITCH;
            }
        }
        // State badge at the card's right end.
        let c = vec2(BADGE_X, r.y + 38.);
        match state {
            ProjectState::Complete => {
                shapes::glow_disc(c, 30., with_alpha(GOLD, 0.25 + 0.15 * pulse(t, 0.5)));
                icon_badge(Icon::Check, c.x, c.y, 36., GAIN, CREAM);
            }
            ProjectState::Building(left) => {
                let total = f32::from(p.duration.max(1));
                let done = (total - f32::from(left)) / total;
                ring_meter(c.x, c.y, 16., 4., done, TEAL);
                centered(num(i32::from(left)), c.x, c.y + 6., 17., TEAL);
            }
            ProjectState::Unbuilt if refusal.is_some() => {
                shapes::fill_circle(c, 17., with_alpha(FADED, 0.12));
                icons::draw(Icon::Lock, c, 18., with_alpha(FADED, 0.7));
            }
            ProjectState::Unbuilt => {
                let lift = if on || game.selected == j {
                    0.25 + 0.2 * pulse(t, 0.9)
                } else {
                    0.12
                };
                shapes::glow_disc(c, 26., with_alpha(GOLD, lift));
                icon_badge(Icon::Hammer, c.x, c.y, 34., darken(GOLD, 0.1), CREAM);
            }
        }
    }
}
/// A new act: the crown, the act's name and the promises sealed so far.
fn brief(game: &Game) {
    let act = game.act();
    let t = clock();
    let cy = 212. + bob(t, 0.45, 3.);
    shapes::glow_disc(
        vec2(331., cy),
        92.,
        with_alpha(GOLD, 0.22 + 0.1 * pulse(t, 0.45)),
    );
    crown(331., cy);
    // A new act's gold callout rises off the heading; the heading settles in
    // beneath it (the first act opens already settled).
    let settle = if game.count == 0 {
        1.
    } else {
        ((f32::from(game.since) - 30.) / 30.).clamp(0., 1.)
    };
    centered(ACT_MARK[act], 331., 318., 42., with_alpha(BROWN, settle));
    centered(ACT_NAMES[act], 331., 350., 20., FADED);
    for (n, (flag, _, _)) in PROMISES.iter().enumerate() {
        let x = 331. + (n as f32 - 3.) * 46.;
        promise_badge(*flag, x, 412., 32., game.flags & flag != 0);
    }
    begin_button();
}
/// The briefing's wide "begin" seal (pointer region `BEGIN`).
fn begin_button() {
    let hovered = BEGIN.contains(pointer());
    let r = Rect::new(
        BEGIN.x,
        BEGIN.y - if hovered { 2. } else { 0. },
        BEGIN.w,
        BEGIN.h,
    );
    lip(r, 10., if hovered { 4. } else { 3. }, 0.3);
    let top = lighten(TEAL, if hovered { 0.22 } else { 0.12 });
    shapes::fill_rounded_rect(r, 10., Fill::vertical(r, top, darken(TEAL, 0.1)));
    shapes::stroke_rounded_rect(
        r,
        10.,
        1.5,
        with_alpha(GOLD, if hovered { 1. } else { 0.7 }),
    );
    let label = plain(20., CREAM).with_shadow(Color::new(0., 0., 0., 0.35));
    let kw = text_width("Enter", plain(13., PAPER)) + 12.;
    let total = kw + 14. + 20. + 10. + text_width("Begin", label);
    let x = r.x + (r.w - total) * 0.5;
    let mid = r.y + r.h * 0.5;
    keycap("Enter", "", x, mid - 10., CREAM);
    icons::draw(Icon::Play, vec2(x + kw + 24., mid), 20., CREAM);
    draw_text_styled("Begin", x + kw + 44., label.centered_baseline(mid), label);
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
        Color::new(0.12, 0.07, 0.03, 0.1),
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
/// One past decree: the petition, the sealed decree and what each pillar
/// actually did (before as a ghost, after as the fill).
fn report(game: &Game, record: Record, index: usize) {
    icons::draw(Icon::Calendar, vec2(56., 148.), 20., FADED);
    let s = plain(16., BROWN);
    let w = draw_text_styled(num(index as i32 + 1), 72., 154., s).width;
    let tail = plain(13., FADED);
    draw_text_styled("/", 74. + w, 154., tail);
    draw_text_styled(num(game.count as i32), 82. + w, 154., tail);
    tool_button(PAGE_PREV, "", Icon::ArrowLeft, false, index > 0);
    tool_button(
        PAGE_NEXT,
        "",
        Icon::ArrowRight,
        false,
        index + 1 < game.count,
    );
    let petition = record.petition;
    let echoed = (petition.title != PETITIONS[index].title)
        .then(|| echo(index, u16::MAX))
        .flatten();
    petition_header(petition.speaker, petition.title, petition.body, echoed);
    let choice = petition.choices[record.choice];
    divider(44., 296., 576., with_alpha(GOLD, 0.8));
    seal(choice.legacy, 66., 334., 18.);
    line(choice.label, 94., 341., 530., 18., BROWN);
    for (k, (glyph, color)) in PILLARS.into_iter().enumerate() {
        let x = 44. + (k % 2) as f32 * 292.;
        let y = 378. + (k / 2) as f32 * 48.;
        icons::draw(glyph, vec2(x + 11., y + 11.), 20., color);
        let (before, after) = (record.before[k], record.after[k]);
        let track = Rect::new(x + 28., y + 6., 156., 10.);
        shapes::fill_rounded_rect(track, 5., darken(VELLUM, 0.12));
        let w = |v: i16| track.w * f32::from(v.clamp(0, 100)) / 100.;
        if w(after) > 0. {
            let fill = Rect::new(track.x, track.y, w(after).max(10.), track.h);
            shapes::fill_rounded_rect(fill, 5., Fill::vertical(fill, lighten(color, 0.25), color));
        }
        if after < before {
            let lost = Rect::new(track.x + w(after), track.y, w(before) - w(after), track.h);
            shapes::fill_rounded_rect(lost, 3., with_alpha(LOSS, 0.35));
        }
        let ink = if after <= realm::DANGER { LOSS } else { BROWN };
        line(num(i32::from(after)), x + 192., y + 17., 30., 16., ink);
        delta_chip(k, after - before, x + 222., y, false, 1.);
    }
    if record.completed != 0 {
        let mut x = 58.;
        for j in (0..PROJECTS.len()).filter(|j| record.completed & (1 << j) != 0) {
            icon_badge(WORK_ICONS[j], x, 488., 30., GOLD, CREAM);
            icon_badge(Icon::Check, x + 11., 477., 14., GAIN, CREAM);
            x += 42.;
        }
    }
}
/// Persistent collapse warning: the frame's edges glow red while any pillar
/// sits in its danger zone, stronger the closer it is to zero.
fn danger_edges(game: &Game) {
    if game.outcome() != Outcome::Playing {
        return;
    }
    let low = *game.resources.iter().min().unwrap_or(&100);
    if low > realm::DANGER {
        return;
    }
    let reach = f32::from(realm::DANGER + 5);
    let severity = (f32::from(realm::DANGER + 5 - low) / reach).clamp(0.45, 1.);
    let a = (0.12 + 0.16 * pulse(clock(), 0.8)) * severity;
    let area = Rect::new(16., 112., 928., 456.);
    let edge = 46.;
    let solid = with_alpha(LOSS, a);
    let clear = with_alpha(LOSS, 0.);
    let top = Rect::new(area.x, area.y, area.w, edge);
    let bottom = Rect::new(area.x, area.y + area.h - edge, area.w, edge);
    let left = Rect::new(area.x, area.y, edge, area.h);
    let right = Rect::new(area.x + area.w - edge, area.y, edge, area.h);
    shapes::fill_rounded_rect(top, 0., Fill::vertical(top, solid, clear));
    shapes::fill_rounded_rect(bottom, 0., Fill::vertical(bottom, clear, solid));
    shapes::fill_rounded_rect(left, 0., Fill::horizontal(left, solid, clear));
    shapes::fill_rounded_rect(right, 0., Fill::horizontal(right, clear, solid));
}

/// The card under the pointer on the decree or works page.
pub(crate) fn hovered(game: &Game) -> Option<usize> {
    if game.outcome() != Outcome::Playing {
        return None;
    }
    let p = pointer();
    match game.screen {
        Screen::Council => (0..3).find(|&i| choice_rect(i).contains(p)),
        Screen::Projects => (0..4).find(|&j| project_rect(j).contains(p)),
        _ => None,
    }
}
/// What the player is weighing: the pillar change of the hovered (else the
/// selected) card, and on decree pages its policy.
pub(crate) fn focus(game: &Game) -> (Option<[i16; 4]>, Option<usize>) {
    if game.outcome() != Outcome::Playing {
        return (None, None);
    }
    let i = hovered(game).unwrap_or(game.selected);
    match game.screen {
        Screen::Council => (
            Some(game.forecast(i)),
            Some(game.current().choices[i].legacy),
        ),
        Screen::Projects if game.affordable(i) => (Some([-PROJECTS[i].cost, 0, 0, 0]), None),
        _ => (None, None),
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
        let (preview, policy) = focus(self);
        realm::realm(self, preview, policy);
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
        if self.screen != Screen::Briefing {
            let playing = self.outcome() == Outcome::Playing;
            tool_button(
                WORKS_BUTTON,
                "X",
                Icon::Hammer,
                self.screen == Screen::Projects,
                playing,
            );
            tool_button(
                CHRONICLE_BUTTON,
                "C",
                Icon::Calendar,
                self.screen == Screen::Chronicle,
                playing && self.count > 0,
            );
        }
        danger_edges(self);
    }
    fn hud(&self) -> Hud {
        let status = match self.ending() {
            Some(ending) => format!(
                "{}, grade {}. {}",
                ending.name(),
                self.grade(),
                ending.story()
            ),
            None => format!(
                "Council {} of 30. {} {}, {} {}, {} {}, {} {}.",
                (self.count + 1).min(30),
                RESOURCE_NAMES[0],
                self.resources[0],
                RESOURCE_NAMES[1],
                self.resources[1],
                RESOURCE_NAMES[2],
                self.resources[2],
                RESOURCE_NAMES[3],
                self.resources[3]
            ),
        };
        Hud {
            stage: ACT_NAMES[self.act()].into(),
            objective: "Keep every pillar above zero".into(),
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
    /// The three acts as header pips; the council track is the objective.
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.act() as u32 + 1, 3))
    }
    fn effects(&self, cue: Cue, fx: &mut Juice) {
        feedback::effects(self, cue, fx);
    }
    fn draw_objective(&self, area: Rect) -> bool {
        realm::objective(self, area)
    }
    fn draw_result(&self, area: Rect) -> bool {
        realm::result(self, area)
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
                // The question: at most two lines between the title and the first card.
                let lines = wrap_text(p.body, QUESTION_W, plain(QUESTION_SIZE, BROWN)).count();
                assert!(lines <= 2, "{}: {lines} lines", p.title);
                assert!(
                    text_width(p.title, plain(TITLE_SIZE, BROWN)) <= TITLE_W,
                    "{}",
                    p.title
                );
                for choice in p.choices {
                    // Decrees stay on one line, clear of the wax seal.
                    let w = text_width(choice.label, plain(CHOICE_SIZE, BROWN));
                    assert!(w <= CHOICE_W, "{}: {w}", choice.label);
                    assert!(CHOICE_X + w < SEAL_X - 22.);
                    assert!(text_width(choice.label, plain(18., BROWN)) <= 530. * 1.05);
                }
            }
        }
        // Four pillar chips and the seal share a card without touching.
        const { assert!(CHOICE_X + 3. * CHIP_PITCH + CHIP_W < SEAL_X - 22.) };
        // Project rewards never run into the state badge.
        let widest = PROJECTS
            .iter()
            .map(|p| p.reward.iter().filter(|r| **r != 0).count())
            .max()
            .unwrap();
        assert!(300. + (widest as f32 - 1.) * CHIP_PITCH + CHIP_W < BADGE_X - 20.);
    }
    #[test]
    fn header_pips_follow_the_three_acts_and_copy_stays_short() {
        let mut game = Game::new();
        assert_eq!(ArcadeGame::progress(&game), Some((1, 3)));
        assert_eq!(game.hud().stage, ACT_NAMES[0]);
        assert!(game.hud().objective.split_whitespace().count() <= 5);
        let mut middle = None;
        for tick in 0..10_000 {
            let input = game.pilot(tick);
            game.step(&input);
            if game.count == 15 && middle.is_none() {
                middle = ArcadeGame::progress(&game);
            }
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(middle, Some((2, 3)));
        assert_eq!(game.outcome(), Outcome::Won);
        assert_eq!(ArcadeGame::progress(&game), Some((3, 3)));
        assert_eq!(game.hud().stage, ACT_NAMES[2]);
        // One sentence for status readers.
        let summary = game.summary();
        assert!(summary.ends_with('.') && !summary.trim_end_matches('.').contains('.'));
        for line in crate::META.help {
            assert!(line.split_whitespace().count() < 15, "{line}");
        }
        assert!(crate::META.help.len() <= 4);
    }
    #[test]
    fn small_number_labels_are_exact_and_clamped() {
        assert_eq!(num(0), "0");
        assert_eq!(num(47), "47");
        assert_eq!(num(100), "100");
        assert_eq!(num(-5), "0");
        assert_eq!(num(250), "100");
        for (bit, (flag, _, name)) in PROMISES.iter().enumerate() {
            assert_eq!(*flag, 1 << bit);
            assert_eq!(promise(*flag).1, *name);
            assert!(name.len() <= ferravex_arcade::juice::MAX_LABEL);
        }
    }
}
