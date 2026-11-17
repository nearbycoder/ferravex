//! Read-only newsroom presentation: a dark desk, case cards and paper dossiers.
//!
//! Every rectangle that accepts pointer input is laid out in `model.rs`; the
//! shapes here stay inside those hit regions so what you see is what you click.
//! Icons carry the vocabulary instead of labels: a magnifier is verified scene
//! evidence, a speech bubble a witness's claim, a key a signed response, red
//! string and a bolt a connected contradiction, and hearts the newspaper's
//! credibility. Only names, the case facts and one-word actions are text.
use crate::{
    content::CASES,
    model::{Event, Game, View},
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
const BLUE_INK: Color = Color::new(0.21, 0.31, 0.52, 1.);
const CREAM: Color = Color::new(0.95, 0.91, 0.82, 1.);
/// Record kinds on the dark desk: verified fact, witness claim, signed response.
const FACT_TONE: Color = Color::new(0.55, 0.80, 0.58, 1.);
const CLAIM_TONE: Color = Color::new(0.63, 0.73, 0.94, 1.);
const SIGNED_TONE: Color = Color::new(0.95, 0.53, 0.47, 1.);
const HEART: Color = Color::new(0.91, 0.32, 0.33, 1.);
const THREAD: Color = Color::new(0.80, 0.17, 0.16, 1.);
/// Ink of a line that is still blacked out (a record not yet collected).
const REDACT: Color = Color::new(0.45, 0.36, 0.28, 0.2);

/// One icon per idea, used everywhere in the game.
const TAB_ICONS: [Icon; 4] = [Icon::Map, Icon::People, Icon::Book, Icon::Target];
/// Hover tooltips only; the tabs themselves are icons.
const TAB_NAMES: [&str; 4] = ["Places", "People", "Notebook", "Accuse"];
const DIGITS: [&str; 4] = ["1", "2", "3", "4"];
const EVIDENCE: Icon = Icon::Search;
const TESTIMONY: Icon = Icon::Chat;
const SIGNATURE: Icon = Icon::Key;
const LINK: Icon = Icon::Bolt;
const FIELDS: [(Icon, &str); 3] = [
    (Icon::Person, "WHO"),
    (Icon::Wrench, "HOW"),
    (Icon::Bulb, "WHY"),
];
/// A pictogram for every authored location, case by case.
const PLACE_ICONS: [[Icon; 3]; 6] = [
    [Icon::Lock, Icon::Gear, Icon::Crate],
    [Icon::Bell, Icon::Music, Icon::Eye],
    [Icon::Flame, Icon::Home, Icon::Mountain],
    [Icon::SoundOn, Icon::Flag, Icon::Music],
    [Icon::Sprout, Icon::Leaf, Icon::Coin],
    [Icon::Crate, Icon::Eye, Icon::Flame],
];
/// Widths of the blacked-out title of each missing notebook record.
const MISSING_TITLE: [f32; 7] = [118., 150., 96., 84., 104., 90., 132.];

/// Left edge and readable width of text on the dossier sheet.
const PAGE_X: f32 = 352.;
const PAGE_W: f32 = 554.;
/// Readable width of the case clipping on the desk.
const CLIP_W: f32 = 228.;
/// Effect anchors, in the same logical units as the hit regions in `model.rs`.
const PRIMARY_AT: Vec2 = vec2(460., 523.);
const SECONDARY_AT: Vec2 = vec2(754., 523.);
const FILE_AT: Vec2 = vec2(634., 523.);
const CLEAR_AT: Vec2 = vec2(174., 523.);
const PLACE_STAMP_AT: Vec2 = vec2(812., 192.);
const PEOPLE_STAMP_AT: Vec2 = vec2(728., 294.);
const BOARD_MID: Vec2 = vec2(625., 446.);
/// The header row the host lends to [`ArcadeGame::draw_objective`].
const HEADER_X: f32 = 22.;
const HEADER_CY: f32 = 81.;

fn shade(r: f32, g: f32, b: f32, a: f32) -> Color {
    Color::new(r, g, b, a)
}
fn hovered(r: Rect) -> bool {
    r.contains(pointer())
}
fn found(game: &Game, records: std::ops::Range<usize>) -> usize {
    game.known[records].iter().filter(|&&known| known).count()
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
/// Small, letter-spaced uppercase label; returns its width.
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, plain(11., color).with_letter_spacing(1.5)).width
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
/// A paragraph struck through in red ink: a denial the record disproves.
fn struck(value: &str, x: f32, y: f32, width: f32, size: f32, leading: f32, color: Color) -> f32 {
    let s = plain(size, color);
    let mut baseline = y;
    for line in wrap_text(value, width, s) {
        let w = draw_text_styled(line, x, baseline, s).width;
        let mid = baseline - size * 0.32;
        let ink = with_alpha(THREAD, 0.85);
        shapes::fill_capsule(vec2(x - 2., mid), vec2(x + w + 2., mid + 1.), 1.1, ink);
        baseline += leading;
    }
    baseline
}

// ---------------------------------------------------------------- marks

/// A filled disc with a check: this step is done.
fn done_mark(at: Vec2, diameter: f32, color: Color) {
    icon_badge(Icon::Check, at.x, at.y, diameter, color, DARK);
}
/// A softly breathing dot: something here still waits for you.
fn todo_mark(at: Vec2, phase: f32) {
    let p = pulse(clock() + phase, 0.8);
    glow(at.x, at.y, 9. + 4. * p, with_alpha(GOLD, 0.16 + 0.22 * p));
    shapes::fill_circle(at, 3.5, lighten(GOLD, 0.25 * p));
}
/// A row of `total` dots of `radius` with the first `filled` lit; returns the width.
fn dots(x: f32, cy: f32, total: usize, filled: usize, radius: f32, on: Color, off: Color) -> f32 {
    let gap = radius * 3.;
    for i in 0..total {
        let c = vec2(x + radius + i as f32 * gap, cy);
        if i < filled {
            shapes::fill_circle(c, radius, on);
        } else {
            shapes::stroke_circle(c, radius - 0.5, 1.2, off);
        }
    }
    radius * 2. + (total.max(1) - 1) as f32 * gap
}
/// The missing requirement's icon, crossed out: why a control is not ready.
fn need_mark(at: Vec2, glyph: Icon) {
    icon(glyph, at.x, at.y, 16., with_alpha(SOFT, 0.7));
    shapes::fill_capsule(
        at + vec2(-8., 8.),
        at + vec2(8., -8.),
        1.2,
        with_alpha(RED_INK, 0.85),
    );
}
/// Blacked-out lines standing in for text that has not been collected.
fn redacted(x: f32, y: f32, widths: &[f32], leading: f32) {
    for (i, w) in widths.iter().enumerate() {
        let r = Rect::new(x, y + i as f32 * leading, *w, 11.);
        shapes::fill_rounded_rect(r, 5.5, REDACT);
    }
}
/// A faint `glyph` drifting over redacted lines: look here to reveal them.
fn searching(glyph: Icon, x: f32, y: f32, span: f32) {
    let t = clock();
    let sweep = 0.5 + 0.5 * (t * 0.75).sin();
    let at = vec2(x + 24. + sweep * span, y + bob(t, 0.55, 3.));
    icon(glyph, at.x, at.y, 38., with_alpha(SOFT, 0.45));
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
fn kind_icon(index: usize) -> Icon {
    match index {
        0..=2 => EVIDENCE,
        3..=5 => TESTIMONY,
        _ => SIGNATURE,
    }
}
/// A record kind's tone on the dark desk.
fn kind_tone(index: usize) -> Color {
    match index {
        0..=2 => FACT_TONE,
        3..=5 => CLAIM_TONE,
        _ => SIGNED_TONE,
    }
}
/// A record kind's ink on paper.
fn kind_ink(index: usize) -> Color {
    match index {
        0..=2 => GREEN_INK,
        3..=5 => BLUE_INK,
        _ => RED_INK,
    }
}
fn kind_stamp(index: usize) -> &'static str {
    match index {
        0..=2 => "FACT",
        3..=5 => "CLAIM",
        _ => "SIGNED",
    }
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
    // The desk lamp's pool of light breathes very slowly.
    let p = pulse(clock(), 0.12);
    glow(176., 320., 300., with_alpha(GOLD, 0.03 + 0.02 * p));
}
/// A list row on the desk; selected rows lift and gain a gold edge, hovered
/// rows brighten.
fn row(x: f32, y: f32, w: f32, h: f32, selected: bool) {
    let r = Rect::new(x, y, w, h);
    let radius = if h < 50. { 8. } else { 10. };
    if selected {
        lip(r, radius, 3., 0.45);
        shapes::fill_rounded_rect(r, radius, Fill::vertical(r, lighten(ROW_ON, 0.05), ROW_ON));
        shapes::stroke_rounded_rect(r, radius, 1., with_alpha(GOLD, 0.6));
        let bar = Rect::new(x + 5., y + h * 0.25, 3., h * 0.5);
        let breath = 0.65 + 0.35 * pulse(clock(), 0.6);
        shapes::fill_rounded_rect(bar, 1.5, with_alpha(lighten(GOLD, 0.15), breath));
    } else if hovered(r) {
        shapes::fill_rounded_rect(
            r,
            radius,
            Fill::vertical(r, lighten(ROW, 0.09), lighten(ROW, 0.04)),
        );
        shapes::stroke_rounded_rect(r, radius, 1., with_alpha(GOLD, 0.3));
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
/// Rubber stamp with an icon, drawn in ink; returns its left edge.
fn stamp(glyph: Icon, label: &str, right: f32, y: f32, color: Color) -> f32 {
    let s = plain(12., with_alpha(color, 0.9)).with_letter_spacing(2.);
    let w = text_width(label, s) + 44.;
    let r = Rect::new(right - w, y, w, 28.);
    shapes::fill_rounded_rect(r, 5., with_alpha(color, 0.07));
    shapes::stroke_rounded_rect(r, 5., 2., with_alpha(color, 0.75));
    icon(glyph, r.x + 19., y + 14., 15., with_alpha(color, 0.9));
    draw_text_styled(label, r.x + 32., s.centered_baseline(y + 14.), s);
    r.x
}
/// How an action button presents its state.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    /// Available.
    Ready,
    /// Available and the obvious next step: it breathes.
    Call,
    /// Already done; pressing it changes nothing.
    Done,
    /// Not available yet.
    Off,
}
/// Paint a button surface in `y = 505..541`; returns the ink for its contents.
fn button_base(r: Rect, look: Look) -> Color {
    let hover = hovered(r);
    match look {
        Look::Ready | Look::Call => {
            if look == Look::Call {
                let p = pulse(clock(), 1.1);
                let halo = Rect::new(r.x - 3., r.y - 3., r.w + 6., r.h + 6.);
                shapes::stroke_rounded_rect(halo, 12., 2., with_alpha(GOLD, 0.2 + 0.55 * p));
            }
            let lift = if hover { 1.5 } else { 0. };
            let face = Rect::new(r.x, r.y - lift, r.w, r.h);
            lip(face, 9., 3. + lift, 0.3);
            let top = lighten(DARK, if hover { 0.2 } else { 0.1 });
            shapes::fill_rounded_rect(face, 9., Fill::vertical(face, top, DARK));
            let shine = Rect::new(face.x + 9., face.y + 1., face.w - 18., 1.);
            shapes::fill_rounded_rect(shine, 0., shade(1., 1., 1., 0.12));
            CREAM
        }
        Look::Done | Look::Off => {
            shapes::fill_rounded_rect(r, 9., shade(0.80, 0.73, 0.60, 1.));
            if hover {
                shapes::stroke_rounded_rect(r, 9., 1., with_alpha(SOFT, 0.4));
            }
            SOFT
        }
    }
}
/// A large clickable action inside `x..x+w`, `y=505..541`: its key cap, its
/// icon and one word.
fn action_button(x: f32, w: f32, key: &str, glyph: Icon, label: &str, look: Look) {
    let r = Rect::new(x, 505., w, 36.);
    let ink = button_base(r, look);
    let lift = if hovered(r) && matches!(look, Look::Ready | Look::Call) {
        1.5
    } else {
        0.
    };
    let cy = 523. - lift;
    let kw = keycap(key, "", x + 14., cy - 10., ink);
    let ix = x + 14. + kw + 18.;
    icon(glyph, ix, cy, 18., ink);
    let room = w - (ix - x) - 48.;
    fit_line(
        label,
        ix + 16.,
        plain(16., ink).centered_baseline(cy),
        room,
        16.,
        ink,
    );
    if look == Look::Done {
        done_mark(vec2(x + w - 22., cy), 18., GREEN_INK);
    }
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
    // Everyone blinks now and then, each on their own rhythm.
    if (clock() * 0.23 + id as f32 * 0.379).fract() < 0.035 {
        let lid = 1.2 * s;
        draw_line(x - 5.5 * s, y - 6. * s, x - 2.5 * s, y - 6. * s, lid, DARK);
        draw_line(x + 3.5 * s, y - 6. * s, x + 6.5 * s, y - 6. * s, lid, DARK);
    } else {
        shapes::fill_circle(vec2(x - 4. * s, y - 6. * s), 1.5 * s, DARK);
        shapes::fill_circle(vec2(x + 5. * s, y - 6. * s), 1.5 * s, DARK);
    }
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
/// A location's pictogram in a round brass-rimmed plate.
fn emblem(at: Vec2, glyph: Icon, selected: bool) {
    let base = shade(0.27, 0.245, 0.255, 1.);
    shapes::fill_circle(
        at,
        25.,
        Fill::radial(
            at - vec2(0., 8.),
            32.,
            lighten(base, 0.12),
            darken(base, 0.2),
        ),
    );
    let rim = if selected {
        with_alpha(GOLD, 0.8)
    } else {
        shade(1., 1., 1., 0.08)
    };
    shapes::stroke_circle(at, 25., 1., rim);
    let ink = if selected { GOLD } else { darken(GOLD, 0.18) };
    icon(glyph, at.x, at.y, 26., ink);
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
/// A glossy red push pin.
fn pushpin(at: Vec2) {
    shapes::fill_circle(at + vec2(1., 2.), 5.5, shade(0., 0., 0., 0.22));
    shapes::fill_circle(
        at,
        5.5,
        Fill::radial(
            at - vec2(1.5, 1.5),
            6.5,
            shade(0.97, 0.47, 0.42, 1.),
            RED_INK,
        ),
    );
}

// ---------------------------------------------------------------- tabs

/// The tab that holds the next useful step (presentation only).
fn next_view(game: &Game) -> Option<View> {
    if game.debrief || game.outcome() != Outcome::Playing {
        None
    } else if found(game, 0..3) < 3 {
        Some(View::Places)
    } else if found(game, 3..6) < 3 {
        Some(View::People)
    } else if !game.linked {
        Some(View::Notebook)
    } else if !game.confronted {
        Some(View::People)
    } else {
        Some(View::Theory)
    }
}
/// A tab's progress marks after its icon; returns their width. With
/// `paint == false` it only measures, so the group can be centered.
fn tab_status(game: &Game, tab: usize, x: f32, on_paper: bool, paint: bool) -> f32 {
    let cy = 129.5;
    let off = if on_paper {
        with_alpha(SOFT, 0.5)
    } else {
        with_alpha(MUTED, 0.45)
    };
    match tab {
        0 | 1 => {
            let (n, tone) = match (tab, on_paper) {
                (0, true) => (found(game, 0..3), GREEN_INK),
                (0, false) => (found(game, 0..3), FACT_TONE),
                (_, true) => (found(game, 3..6), BLUE_INK),
                (_, false) => (found(game, 3..6), CLAIM_TONE),
            };
            let mut w = if n == 3 {
                if paint {
                    done_mark(vec2(x + 8., cy), 16., tone);
                }
                16.
            } else if paint {
                dots(x, cy, 3, n, 3.5, tone, off)
            } else {
                28.
            };
            if tab == 1 && game.linked {
                // Once a contradiction names a witness, the signature waits here.
                let at = vec2(x + w + 15., cy);
                if paint {
                    let signed = if on_paper { RED_INK } else { SIGNED_TONE };
                    if game.confronted {
                        icon(SIGNATURE, at.x, at.y, 16., signed);
                    } else {
                        let p = pulse(clock(), 1.);
                        glow(at.x, at.y, 12., with_alpha(SIGNED_TONE, 0.2 * p));
                        icon(
                            SIGNATURE,
                            at.x,
                            at.y,
                            16.,
                            with_alpha(signed, 0.45 + 0.55 * p),
                        );
                    }
                }
                w += 23.;
            }
            w
        }
        2 if game.linked => {
            if paint {
                icon_badge(LINK, x + 9., cy, 18., GOLD, DARK);
            }
            18.
        }
        2 => {
            if paint {
                let pinned = game.pins.iter().filter(|pin| pin.is_some()).count();
                dots(x, cy, 2, pinned, 3.5, THREAD, off);
                icon(LINK, x + 34., cy, 14., off);
            }
            41.
        }
        _ => 0.,
    }
}
fn tabs(game: &Game) {
    let track = Rect::new(20., 113., 920., 33.);
    shapes::fill_rounded_rect(track, 11., shade(0.05, 0.04, 0.05, 0.55));
    let next = next_view(game);
    let t = clock();
    for (i, glyph) in TAB_ICONS.iter().enumerate() {
        let view = View::from_index(i);
        let x = 24. + i as f32 * 228.;
        let r = Rect::new(x + 1., 115., 224., 29.);
        let active = !game.debrief && game.view == view;
        if active {
            lip(r, 8., 2., 0.45);
            shapes::fill_rounded_rect(r, 8., Fill::vertical(r, SHEET_TOP, SHEET));
        } else if !game.debrief && hovered(r) {
            shapes::fill_rounded_rect(r, 8., with_alpha(GOLD, 0.1));
        }
        let fade = if game.debrief { 0.45 } else { 1. };
        let ink = with_alpha(if active { DARK } else { lighten(GOLD, 0.1) }, fade);
        let status = tab_status(game, i, 0., active, false);
        let group = 19. + 30. + if status > 0. { 12. + status } else { 0. };
        let left = x + 113. - group * 0.5;
        // The number key that opens this tab.
        let cap = Rect::new(left, 120., 19., 19.);
        let cap_fill = if active {
            DARK
        } else {
            with_alpha(GOLD, 0.16 * fade)
        };
        shapes::fill_rounded_rect(cap, 5., cap_fill);
        let digit = plain(
            12.,
            if active {
                SHEET
            } else {
                with_alpha(GOLD, fade)
            },
        )
        .with_align(TextAlign::Center);
        draw_text_styled(DIGITS[i], left + 9.5, digit.centered_baseline(129.5), digit);
        // The icon; the tab that holds the next step glows and bobs.
        let mut at = vec2(left + 19. + 19., 129.5);
        if next == Some(view) && !active {
            // A slow sonar ping around the icon: the next step is here.
            let p = pulse(t, 0.9);
            let ping = (t * 0.7).fract();
            glow(at.x, at.y, 18. + 6. * p, with_alpha(GOLD, 0.3 + 0.3 * p));
            shapes::stroke_circle(
                at,
                12. + 12. * ping,
                1.5,
                with_alpha(GOLD, 0.7 * (1. - ping)),
            );
            at.y += bob(t, 0.9, 1.5);
        }
        icon(*glyph, at.x, at.y, 20., ink);
        if status > 0. && !game.debrief {
            tab_status(game, i, left + 19. + 30. + 12., active, true);
        }
    }
}
/// Hovering an inactive tab names it, just below the tab.
fn tab_tooltip(game: &Game) {
    let at = pointer();
    if game.debrief || !(115. ..144.).contains(&at.y) || !(24. ..936.).contains(&at.x) {
        return;
    }
    let i = (((at.x - 24.) / 228.) as usize).min(3);
    if game.view.index() == i {
        return;
    }
    let name = TAB_NAMES[i];
    let w = text_width(name, style(13., GOLD)) + 13. * 1.3;
    let x = 24. + i as f32 * 228. + 113. - w * 0.5;
    shapes::fill_rounded_rect(
        Rect::new(x, 150., w, 19.),
        9.5,
        shade(0.07, 0.06, 0.07, 0.94),
    );
    chip(name, x, 150., 13., GOLD);
}

// ---------------------------------------------------------------- views

/// The case premise as a newspaper clipping on the desk.
fn clipping(game: &Game) {
    let r = Rect::new(36., 456., 276., 90.);
    lip(r, 4., 3., 0.35);
    shapes::fill_rounded_rect(r, 4., Fill::vertical(r, SHEET_TOP, SHEET_LOW));
    shapes::stroke_rounded_rect(r, 4., 1., shade(0.32, 0.22, 0.12, 0.25));
    icon(Icon::Scroll, 56., 476., 18., SOFT);
    let s = plain(14., DARK);
    let opening = game.case_data().opening;
    let lines = wrap_text(opening, CLIP_W, s).count().max(1) as f32;
    let first = 501. - (lines - 1.) * 9.;
    prose(opening, 72., first.max(482.), CLIP_W, 14., 18., DARK);
}
fn page_divider(y: f32, w: f32) {
    panel(PAGE_X, y, w, 1., shade(0.45, 0.33, 0.22, 0.28));
}
/// The notebook button: its key, a book and the seven records it holds.
fn notebook_button(game: &Game) {
    let (x, w) = (590., 328.);
    let r = Rect::new(x, 505., w, 36.);
    let ink = button_base(r, Look::Ready);
    let cy = 523. - if hovered(r) { 1.5 } else { 0. };
    let kw = keycap("X", "", x + 14., cy - 10., ink);
    let mut rx = x + 14. + kw + 18.;
    icon(Icon::Book, rx, cy, 18., ink);
    rx += 26.;
    for i in 0..7 {
        let tone = if game.known[i] {
            kind_tone(i)
        } else {
            with_alpha(MUTED, 0.3)
        };
        icon(kind_icon(i), rx + 8., cy, 15., tone);
        rx += if i == 2 || i == 5 { 30. } else { 21. };
    }
}
fn draw_places(game: &Game) {
    let case = game.case_data();
    let selected = game.selected[0];
    for (i, place) in case.places.iter().enumerate() {
        let y = 188. + i as f32 * 86.;
        let on = selected == i;
        row(36., y, 276., 76., on);
        emblem(vec2(72., y + 38.), PLACE_ICONS[game.case][i], on);
        let ink = if on { CREAM } else { SHEET };
        fit_line(place.name, 108., y + 44., 160., 17., ink);
        let badge = vec2(287., y + 38.);
        if game.known[i] {
            done_mark(badge, 20., FACT_TONE);
        } else {
            todo_mark(badge, i as f32 * 0.35);
        }
    }
    clipping(game);
    let place = &case.places[selected];
    let known = game.known[selected];
    icon_badge(
        PLACE_ICONS[game.case][selected],
        376.,
        205.,
        46.,
        DARK,
        GOLD,
    );
    fit_line(place.name, 410., 214., 320., 24., DARK);
    if known {
        stamp(Icon::Check, "VERIFIED", 870., 178., GREEN_INK);
    }
    page_divider(240., PAGE_W);
    if known {
        prose(place.evidence, PAGE_X, 290., PAGE_W, 18., 27., DARK);
    } else {
        redacted(PAGE_X, 280., &[520., 470., 330.], 27.);
        searching(EVIDENCE, PAGE_X, 302., 430.);
    }
    let look = if known { Look::Done } else { Look::Call };
    action_button(350., 220., "Space", EVIDENCE, "Examine", look);
    notebook_button(game);
}
fn draw_people(game: &Game) {
    let case = game.case_data();
    let selected = game.selected[1];
    for (i, suspect) in case.suspects.iter().enumerate() {
        let y = 188. + i as f32 * 86.;
        let on = selected == i;
        row(36., y, 276., 76., on);
        portrait(72., y + 38., 1., game.case * 3 + i, on);
        let ink = if on { CREAM } else { SHEET };
        fit_line(suspect.name, 108., y + 34., 160., 17., ink);
        fit_line(suspect.role, 108., y + 55., 160., 12., MUTED);
        let badge = vec2(287., y + 38.);
        let culprit = i == case.culprit;
        if game.confronted && culprit {
            icon_badge(SIGNATURE, badge.x, badge.y, 22., SIGNED_TONE, DARK);
        } else if game.linked && culprit {
            let p = pulse(clock(), 1.);
            glow(badge.x, badge.y, 14. + 4. * p, with_alpha(THREAD, 0.3 * p));
            icon_badge(LINK, badge.x, badge.y, 22., THREAD, CREAM);
        } else if game.known[3 + i] {
            done_mark(badge, 20., CLAIM_TONE);
        } else {
            todo_mark(badge, i as f32 * 0.35);
        }
    }
    clipping(game);
    let suspect = &case.suspects[selected];
    fit_line(suspect.name, PAGE_X, 214., 420., 24., DARK);
    fit_line(suspect.role, PAGE_X, 242., 420., 15., SOFT);
    polaroid(800., 172., game.case * 3 + selected);
    page_divider(266., 430.);
    let known = game.known[3 + selected];
    let culprit = selected == case.culprit;
    let signed = game.confronted && culprit;
    if !known {
        redacted(PAGE_X, 322., &[500., 360.], 27.);
        searching(TESTIMONY, PAGE_X, 342., 380.);
    } else if signed {
        // The disproved denial stays on file, struck through, above the signature.
        let next = struck(suspect.statement, PAGE_X, 330., PAGE_W, 16., 22., SOFT);
        icon(SIGNATURE, PAGE_X + 9., next + 9., 18., RED_INK);
        prose(
            case.confession,
            PAGE_X + 30.,
            next + 15.,
            PAGE_W - 30.,
            18.,
            27.,
            DARK,
        );
        stamp(SIGNATURE, "SIGNED", 782., 280., RED_INK);
    } else {
        prose(suspect.statement, PAGE_X, 334., PAGE_W, 18., 27., DARK);
        let left = stamp(TESTIMONY, "CLAIM", 782., 280., BLUE_INK);
        if game.linked && culprit {
            let p = pulse(clock(), 1.);
            glow(left - 22., 294., 14. + 4. * p, with_alpha(THREAD, 0.3 * p));
            icon_badge(LINK, left - 22., 294., 24., THREAD, CREAM);
        }
    }
    let look = if known { Look::Done } else { Look::Call };
    action_button(350., 220., "Space", TESTIMONY, "Interview", look);
    let confront = if signed {
        Look::Done
    } else if game.linked && culprit {
        Look::Call
    } else {
        Look::Off
    };
    action_button(590., 328., "C", SIGNATURE, "Confront", confront);
    if !game.linked {
        need_mark(vec2(892., 523.), LINK);
    }
}
/// The two pinned index cards, joined by red string; a bolt marks a
/// connected contradiction.
fn pinboard(game: &Game) {
    let t = clock();
    // Pointing at Pin ghosts the board it would leave behind (the oldest card
    // drops off a full board); pointing at Unpin fades the card it removes.
    let selected = game.selected[2];
    let pointing = hovered(Rect::new(350., 505., 220., 36.)) && game.known[selected];
    let pinned = game.pins.contains(&Some(selected));
    let board = match game.pins {
        [None, second] => [Some(selected), second],
        [first, None] => [first, Some(selected)],
        [_, second] => [second, Some(selected)],
    };
    let outcome = (pointing && !pinned).then_some(board);
    for (slot, &now) in game.pins.iter().enumerate() {
        let x = 350. + slot as f32 * 300.;
        let next = outcome.map_or(now, |board| board[slot]);
        match (now, next) {
            (_, Some(ghost)) if next != now => index_card(game, x, ghost, 0.45),
            (Some(index), _) => {
                let leaving = pointing && pinned && index == selected;
                index_card(game, x, index, if leaving { 0.4 } else { 1. });
            }
            _ => {
                let r = Rect::new(x, 428., 250., 62.);
                shapes::stroke_rounded_rect(r, 4., 1.5, with_alpha(SOFT, 0.3));
                icon(Icon::Pin, x + 125., 459., 22., with_alpha(SOFT, 0.3));
            }
        }
    }
    let strung = game.pins.iter().all(Option::is_some);
    let ghost_string = !strung && outcome.is_some_and(|board| board.iter().all(Option::is_some));
    if strung || ghost_string {
        let (a, b) = (pin_head(0), pin_head(1));
        let sag = if game.linked {
            3.
        } else {
            12. + 2.5 * (t * 1.7).sin()
        };
        let points: [Vec2; 13] = std::array::from_fn(|i| {
            let u = i as f32 / 12.;
            a.lerp(b, u) + vec2(0., sag * 4. * u * (1. - u))
        });
        if game.linked {
            let p = pulse(t, 0.7);
            for k in [3, 6, 9] {
                let c = points[k];
                glow(c.x, c.y, 24. + 6. * p, with_alpha(THREAD, 0.1 + 0.08 * p));
            }
        }
        let width = if game.linked { 2.6 } else { 2. };
        let ink = if strung { 0.92 } else { 0.35 };
        shapes::polyline(
            &points,
            Stroke::new(width, with_alpha(THREAD, ink))
                .join(Join::Round)
                .cap(Cap::Round),
        );
        if game.linked {
            let mid = points[6];
            icon_badge(LINK, mid.x, mid.y + 2., 30., THREAD, CREAM);
        }
    }
    for (slot, pin) in game.pins.iter().enumerate() {
        if pin.is_some() {
            pushpin(pin_head(slot));
        } else if outcome.is_some_and(|board| board[slot].is_some()) {
            shapes::fill_circle(pin_head(slot), 5.5, with_alpha(RED_INK, 0.35));
        }
    }
}
/// A pinned record's index card at `x`, faded to `alpha` for previews.
fn index_card(game: &Game, x: f32, index: usize, alpha: f32) {
    let r = Rect::new(x, 428., 250., 62.);
    lip(r, 4., 3., 0.2 * alpha);
    shapes::fill_rounded_rect(
        r,
        4.,
        Fill::vertical(
            r,
            shade(0.99, 0.97, 0.91, alpha),
            shade(0.95, 0.92, 0.84, alpha),
        ),
    );
    panel(x + 8., 450., 234., 1., with_alpha(THREAD, 0.25 * alpha));
    let ink = kind_ink(index);
    icon(kind_icon(index), x + 24., 470., 18., with_alpha(ink, alpha));
    let title = game.note_title(index);
    fit_line(title, x + 42., 476., 196., 15., with_alpha(DARK, alpha));
}
fn pin_head(slot: usize) -> Vec2 {
    vec2(475. + slot as f32 * 300., 436.)
}
fn draw_notebook(game: &Game) {
    let selected = game.selected[2];
    for (i, missing) in MISSING_TITLE.iter().enumerate() {
        let y = 180. + i as f32 * 43.;
        let on = i == selected;
        row(36., y, 276., 37., on);
        let known = game.known[i];
        let cy = y + 18.5;
        let tone = if known {
            kind_tone(i)
        } else {
            with_alpha(MUTED, 0.3)
        };
        icon(kind_icon(i), 60., cy, 18., tone);
        if known {
            let ink = if on { CREAM } else { SHEET };
            fit_line(game.note_title(i), 82., y + 24., 188., 14., ink);
        } else {
            let bar = Rect::new(82., cy - 4., *missing, 8.);
            shapes::fill_rounded_rect(bar, 4., with_alpha(MUTED, 0.16));
        }
        if game.pins.contains(&Some(i)) {
            pushpin(vec2(292., cy));
        }
    }
    let any = game.pins.iter().any(Option::is_some);
    let clear = if any { Look::Ready } else { Look::Off };
    action_button(36., 276., "X", Icon::Cross, "Clear", clear);
    let known = game.known[selected];
    let ink = kind_ink(selected);
    let plate = if known { ink } else { with_alpha(SOFT, 0.25) };
    icon_badge(kind_icon(selected), 376., 205., 44., plate, CREAM);
    if known {
        fit_line(game.note_title(selected), 410., 214., 320., 24., DARK);
        stamp(kind_icon(selected), kind_stamp(selected), 870., 178., ink);
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
        redacted(410., 200., &[230.], 0.);
        redacted(PAGE_X, 270., &[510., 440., 260.], 27.);
    }
    page_divider(240., PAGE_W);
    pinboard(game);
    let pinned = game.pins.contains(&Some(selected));
    let look = if known { Look::Ready } else { Look::Off };
    let label = if pinned { "Unpin" } else { "Pin" };
    action_button(350., 220., "Space", Icon::Pin, label, look);
    let connect = if game.linked {
        Look::Done
    } else if game.pins.iter().all(Option::is_some) {
        Look::Call
    } else {
        Look::Off
    };
    action_button(590., 328., "C", LINK, "Connect", connect);
}
/// The path to an accusation as a rail of icons: scenes, interviews, the
/// contradiction and the signature light up as they are collected.
fn readiness(game: &Game) {
    desk_card(36., 182., 276., 364.);
    let steps = [
        (EVIDENCE, found(game, 0..3), 3, FACT_TONE),
        (TESTIMONY, found(game, 3..6), 3, CLAIM_TONE),
        (LINK, usize::from(game.linked), 1, GOLD),
        (SIGNATURE, usize::from(game.confronted), 1, SIGNED_TONE),
    ];
    let (x, top, gap) = (84., 228., 88.);
    for (k, (_, have, need, _)) in steps.iter().take(3).enumerate() {
        let done = have >= need;
        let rail = if done {
            with_alpha(GOLD, 0.55)
        } else {
            shade(1., 1., 1., 0.08)
        };
        let (y0, y1) = (top + k as f32 * gap + 25., top + (k + 1) as f32 * gap - 25.);
        shapes::fill_capsule(vec2(x, y0), vec2(x, y1), 1.5, rail);
    }
    let mut waiting = true;
    for (k, (glyph, have, need, tone)) in steps.iter().enumerate() {
        let cy = top + k as f32 * gap;
        let c = vec2(x, cy);
        if have >= need {
            glow(c.x, c.y, 30., with_alpha(*tone, 0.12));
            shapes::fill_circle(c, 21., *tone);
            icon(*glyph, x, cy, 22., DARK);
            done_mark(vec2(278., cy), 22., *tone);
        } else {
            shapes::fill_circle(c, 21., shade(1., 1., 1., 0.04));
            shapes::stroke_circle(c, 21., 1.2, with_alpha(*tone, 0.35));
            icon(*glyph, x, cy, 22., with_alpha(*tone, 0.5));
            if waiting {
                todo_mark(vec2(278., cy), 0.);
                waiting = false;
            }
        }
        if *need > 1 {
            dots(124., cy, *need, *have, 5., *tone, with_alpha(MUTED, 0.45));
        }
    }
}
fn draw_theory(game: &Game) {
    let case = game.case_data();
    readiness(game);
    // The signature explains the method and the motive.
    if game.confronted {
        icon(SIGNATURE, 366., 199., 18., RED_INK);
        fit_line(case.confession, 386., 205., 500., 16., DARK);
    } else {
        icon(SIGNATURE, 366., 199., 18., with_alpha(SOFT, 0.4));
        redacted(386., 194., &[320.], 0.);
    }
    for (field, (glyph, name)) in FIELDS.iter().enumerate() {
        let y = 229. + field as f32 * 73.;
        let r = Rect::new(350., y, 568., 65.);
        let on = game.selected[3] == field;
        if on {
            lip(r, 8., 3., 0.16);
            shapes::fill_rounded_rect(
                r,
                8.,
                Fill::vertical(r, shade(0.99, 0.96, 0.89, 1.), shade(0.96, 0.92, 0.83, 1.)),
            );
            shapes::stroke_rounded_rect(r, 8., 2., RED_INK);
        } else if hovered(r) {
            shapes::fill_rounded_rect(r, 8., lighten(FIELD, 0.08));
            shapes::stroke_rounded_rect(r, 8., 1., with_alpha(SOFT, 0.4));
        } else {
            shapes::fill_rounded_rect(r, 8., FIELD);
        }
        let ink = if on { RED_INK } else { SOFT };
        let answer = game.theory[field];
        if field == 0 {
            portrait(388., y + 33., 0.62, game.case * 3 + answer, on);
        } else {
            icon(*glyph, 388., y + 33., 26., ink);
        }
        eyebrow(name, 420., y + 24., ink);
        let value = match field {
            0 => case.suspects[answer].name,
            1 => case.methods[answer],
            _ => case.motives[answer],
        };
        fit_line(value, 420., y + 51., 400., 20., DARK);
        let arrow = if on { RED_INK } else { with_alpha(SOFT, 0.6) };
        chevron(842., y + 33., -1., arrow);
        dots(855., y + 33., 3, 0, 3., ink, with_alpha(SOFT, 0.55));
        shapes::fill_circle(vec2(858. + answer as f32 * 9., y + 33.), 3., ink);
        chevron(898., y + 33., 1., arrow);
    }
    let look = if game.can_file() {
        Look::Call
    } else {
        Look::Off
    };
    action_button(350., 568., "Space", TAB_ICONS[3], "Accuse", look);
}
fn draw_debrief(game: &Game) {
    let case = game.case_data();
    sheet(32., 160., 896., 390.);
    let (left, right) = (56., 904.);
    let rule = shade(0.45, 0.33, 0.22, 0.3);
    // Masthead and dateline.
    let mast = plain(22., DARK).with_align(TextAlign::Center);
    draw_text_styled("The Gazette", 480., 194., mast);
    let dateline = plain(11., SOFT)
        .with_letter_spacing(1.5)
        .with_align(TextAlign::Right);
    draw_text_styled(case.district, 864., 190., dateline);
    panel(left, 204., right - left, 2., with_alpha(DARK, 0.8));
    panel(left, 208., right - left, 1., with_alpha(DARK, 0.45));
    // Headline, stamp and the signed explanation.
    fit_line(case.title, left, 252., 600., 30., DARK);
    stamp(Icon::Check, "SOLVED", right, 222., GREEN_INK);
    fit_line(case.confession, left, 282., right - left, 16., SOFT);
    panel(left, 298., right - left, 1., rule);
    // Who, how and why.
    let answers = [
        case.suspects[case.culprit].name,
        case.methods[case.method],
        case.motives[case.motive],
    ];
    for (f, value) in answers.iter().enumerate() {
        let x = left + f as f32 * 284.;
        if f > 0 {
            panel(x - 16., 308., 1., 54., rule);
        }
        if f == 0 {
            portrait(x + 18., 336., 0.66, game.case * 3 + case.culprit, false);
        } else {
            icon(FIELDS[f].0, x + 18., 336., 26., RED_INK);
        }
        fit_line(value, x + 46., 343., 220., 19., DARK);
    }
    panel(left, 372., right - left, 1., rule);
    // The verified fact that broke the denial.
    icon(EVIDENCE, left + 9., 394., 18., GREEN_INK);
    prose(
        case.places[0].evidence,
        left + 30.,
        400.,
        390.,
        14.,
        19.,
        DARK,
    );
    panel(480., 382., 1., 100., rule);
    icon_badge(LINK, 480., 430., 28., THREAD, CREAM);
    icon(TESTIMONY, 514., 394., 18., BLUE_INK);
    let denial = case.suspects[case.culprit].statement;
    struck(denial, 534., 400., 370., 14., 19., SOFT);
    // Editions printed so far.
    for i in 0..CASES.len() {
        let printed = i < game.solved;
        let tone = if printed {
            darken(GOLD, 0.25)
        } else {
            with_alpha(SOFT, 0.25)
        };
        icon(Icon::Scroll, left + 14. + i as f32 * 44., 523., 26., tone);
    }
    let last = game.solved == CASES.len();
    let (glyph, label) = if last {
        (Icon::Scroll, "Publish")
    } else {
        (Icon::ArrowRight, "Next")
    };
    action_button(350., 568., "Space", glyph, label, Look::Call);
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
    match game.view {
        View::Places => draw_places(game),
        View::People => draw_people(game),
        View::Notebook => draw_notebook(game),
        View::Theory => draw_theory(game),
    }
    clip(890., 160.);
    tab_tooltip(game);
}

// ---------------------------------------------------------------- header

fn heart_at(index: usize) -> Vec2 {
    vec2(HEADER_X + 240. + index as f32 * 25., HEADER_CY)
}
/// Pointing at a ready Accuse button previews its risk on the last heart.
fn risk_preview(game: &Game) -> bool {
    !game.debrief
        && game.view == View::Theory
        && game.can_file()
        && hovered(Rect::new(350., 505., 568., 36.))
}
/// WHO, HOW and WHY as three slots that light up as the investigation
/// earns them, then the newspaper's four hearts of credibility.
fn objective(game: &Game, area: Rect) {
    let cy = area.y + area.h * 0.5;
    let case = game.case_data();
    let closed = game.debrief || game.outcome() == Outcome::Won;
    let earned = [game.linked, game.confronted, game.confronted];
    for (f, (glyph, _)) in FIELDS.iter().enumerate() {
        let x = area.x + 2. + f as f32 * 70.;
        let lit = closed || earned[f];
        let r = Rect::new(x, cy - 14., 62., 28.);
        let (fill, rim) = if lit {
            (with_alpha(GOLD, 0.16), with_alpha(GOLD, 0.5))
        } else {
            (shade(1., 1., 1., 0.05), shade(1., 1., 1., 0.1))
        };
        shapes::fill_rounded_rect(r, 14., fill);
        shapes::stroke_rounded_rect(r, 14., 1., rim);
        if f == 0 && lit {
            portrait(x + 17., cy, 0.42, game.case * 3 + case.culprit, false);
        } else {
            let ink = if lit { GOLD } else { with_alpha(MUTED, 0.7) };
            icon(*glyph, x + 17., cy, 18., ink);
        }
        let mark = vec2(x + 45., cy);
        if closed {
            done_mark(mark, 16., FACT_TONE);
        } else if lit {
            icon(Icon::Check, mark.x, mark.y, 14., GOLD);
        } else {
            icon(Icon::Question, mark.x, mark.y, 14., with_alpha(MUTED, 0.8));
        }
    }
    panel(area.x + 218., cy - 10., 1., 20., shade(1., 1., 1., 0.12));
    let left = usize::from(game.credibility());
    let risk = risk_preview(game);
    let p = pulse(clock(), 2.);
    for i in 0..4 {
        let at = heart_at(i);
        if i < left {
            let warn = risk && i + 1 == left;
            if warn {
                glow(at.x, at.y, 14. + 4. * p, with_alpha(HEART, 0.3 * p));
            }
            let tone = if warn {
                lighten(HEART, 0.35 * p)
            } else {
                HEART
            };
            icon(Icon::Heart, at.x, at.y, 20., tone);
        } else {
            icon(Icon::Heart, at.x, at.y, 20., shade(1., 1., 1., 0.12));
        }
    }
}
/// Cases closed, contradictions found, accusation accuracy and credibility,
/// then the reporter rank on a win.
fn result(game: &Game, area: Rect) {
    let filed = game.solved + usize::from(game.mistakes);
    let accuracy = (game.solved * 100).checked_div(filed).unwrap_or(0);
    let stats = [
        (Icon::Scroll, game.solved.to_string(), GOLD),
        (LINK, game.contradictions().to_string(), GOLD),
        (Icon::Target, format!("{accuracy}%"), GOLD),
        (Icon::Heart, game.credibility().to_string(), HEART),
    ];
    let column = area.w / stats.len() as f32;
    for (i, (glyph, value, tone)) in stats.iter().enumerate() {
        icon_value(
            *glyph,
            value,
            area.x + i as f32 * column,
            area.y + 30.,
            30.,
            *tone,
        );
    }
    if game.outcome() == Outcome::Won {
        icon_chip(Icon::Crown, game.rank(), area.x, area.y + 74., 20., GOLD);
    }
}

// ---------------------------------------------------------------- effects

fn list_icon(view: View, row: usize) -> Vec2 {
    match view {
        View::Notebook => vec2(60., 198.5 + row as f32 * 43.),
        _ => vec2(72., 226. + row as f32 * 86.),
    }
}
fn list_badge(view: View, row: usize) -> Vec2 {
    match view {
        View::Notebook => vec2(292., 198.5 + row as f32 * 43.),
        _ => vec2(287., 226. + row as f32 * 86.),
    }
}
/// Answer one accepted (or refused) command where it happened.
fn effect(game: &Game, event: Event, fx: &mut Juice) {
    let no = HEART;
    match event {
        Event::Tab(view) => {
            let at = vec2(24. + view.index() as f32 * 228. + 113., 129.5);
            fx.ring(at, with_alpha(GOLD, 0.7), 22.);
        }
        Event::Select(..) | Event::NoTarget => {}
        Event::Draft(field, _) => {
            let at = vec2(560., 229. + field as f32 * 73. + 44.);
            fx.ring(at, with_alpha(RED_INK, 0.6), 28.);
        }
        Event::Recorded(index) => {
            let (view, row, stamp_at) = if index < 3 {
                (View::Places, index, PLACE_STAMP_AT)
            } else {
                (View::People, index - 3, PEOPLE_STAMP_AT)
            };
            let tone = kind_tone(index);
            let at = list_badge(view, row);
            fx.burst(at, tone, 18);
            fx.ring(at, tone, 30.);
            fx.popup_icon(at - vec2(0., 22.), kind_icon(index), "", tone);
            fx.ring(stamp_at, with_alpha(kind_ink(index), 0.8), 64.);
        }
        Event::Repeat(_) | Event::AlreadyLinked | Event::AlreadySigned(_) => {
            let at = match event {
                Event::Repeat(_) => PRIMARY_AT,
                _ => SECONDARY_AT,
            };
            fx.popup_icon(at - vec2(0., 30.), Icon::Check, "", with_alpha(MUTED, 0.9));
        }
        Event::Pinned(index) => {
            let tone = shade(0.97, 0.47, 0.42, 1.);
            let at = list_badge(View::Notebook, index);
            fx.burst(at, tone, 10);
            fx.ring(at, tone, 18.);
            let slot = game.pins.iter().position(|pin| *pin == Some(index));
            let head = pin_head(slot.unwrap_or(0));
            fx.burst(head, tone, 12);
            fx.ring(head, tone, 30.);
        }
        Event::Unpinned(index) => {
            fx.ring(
                list_badge(View::Notebook, index),
                with_alpha(MUTED, 0.8),
                18.,
            );
        }
        Event::Missing(index) => {
            fx.shake(0.15);
            fx.ring(list_icon(View::Notebook, index), no, 22.);
            fx.popup(PRIMARY_AT - vec2(0., 30.), "Missing", no);
        }
        Event::Cleared => {
            for slot in 0..2 {
                fx.burst(pin_head(slot) + vec2(0., 20.), MUTED, 8);
            }
            fx.ring(CLEAR_AT, with_alpha(MUTED, 0.8), 30.);
        }
        Event::Linked => {
            // Over the dossier's heading band: it never hides a record, the
            // pinboard burst or a statement read just after switching tabs.
            fx.callout(vec2(625., 250.), "Contradiction!", SIGNED_TONE);
            fx.burst(BOARD_MID, THREAD, 36);
            fx.burst(BOARD_MID, GOLD, 22);
            fx.ring(BOARD_MID, GOLD, 90.);
            fx.sparkle(BOARD_MID, GOLD);
            fx.shake(0.18);
        }
        Event::NoLink => {
            fx.shake(0.28);
            fx.ring(BOARD_MID, no, 44.);
            fx.popup(BOARD_MID - vec2(0., 28.), "No link", no);
        }
        Event::Confessed(witness) => {
            // The stamp slams down; the signature itself stays readable.
            fx.burst(PEOPLE_STAMP_AT, SIGNED_TONE, 28);
            fx.ring(PEOPLE_STAMP_AT, SIGNED_TONE, 70.);
            let at = list_badge(View::People, witness);
            fx.popup_icon(at - vec2(0., 22.), SIGNATURE, "", SIGNED_TONE);
        }
        Event::ConfrontLocked(_) => {
            fx.shake(0.18);
            fx.ring(SECONDARY_AT, no, 40.);
            fx.popup_icon(SECONDARY_AT - vec2(0., 32.), LINK, "?", no);
        }
        Event::WrongWitness(witness) => {
            fx.shake(0.22);
            fx.ring(list_icon(View::People, witness), no, 34.);
            fx.popup(SECONDARY_AT - vec2(0., 32.), "Wrong witness", no);
        }
        Event::FileLocked => {
            fx.shake(0.18);
            fx.ring(FILE_AT, no, 44.);
            fx.popup(FILE_AT - vec2(0., 32.), "Not ready", no);
        }
        Event::Solved => {
            fx.callout(vec2(600., 250.), "Case closed!", GOLD);
            fx.confetti(vec2(480., 236.), 70);
            fx.ring(vec2(840., 236.), FACT_TONE, 90.);
            fx.burst(vec2(840., 236.), FACT_TONE, 24);
        }
        Event::Retracted(remaining) => {
            let heart = heart_at(usize::from(remaining));
            fx.burst(heart, no, 26);
            fx.ring(heart, no, 34.);
            fx.popup_icon(heart + vec2(0., 24.), Icon::Heart, "-1", no);
            fx.callout(vec2(634., 474.), "Wrong!", no);
            fx.ring(FILE_AT, no, 64.);
        }
        Event::NextCase => {
            fx.toast(Some(Icon::Scroll), game.case_data().title, GOLD);
            fx.sparkle(vec2(480., 200.), GOLD);
        }
        Event::Published => {
            fx.callout(vec2(600., 250.), "Final edition!", GOLD);
        }
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
        draw(self);
    }
    fn hud(&self) -> Hud {
        let objective = match self.outcome() {
            Outcome::Won => "Final edition printed",
            Outcome::Lost => "Credibility lost",
            Outcome::Playing if self.debrief => "Case closed",
            Outcome::Playing => "Find who, how and why",
        };
        Hud {
            stage: self.case_data().title.to_string(),
            objective: objective.to_string(),
            status: format!(
                "Credibility {}/4, records {}/7, cases closed {}/6",
                self.credibility,
                self.evidence_count(),
                self.solved
            ),
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        Game::outcome(self)
    }
    fn summary(&self) -> String {
        if self.outcome() == Outcome::Won {
            format!(
                "All six cases closed with {}/4 credibility: {}.",
                self.credibility,
                self.rank()
            )
        } else {
            format!(
                "Credibility lost after closing {} of {} cases.",
                self.solved,
                CASES.len()
            )
        }
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.autoplay(tick)
    }
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.case_number() as u32, CASES.len() as u32))
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        for event in self.events.iter() {
            effect(self, event, fx);
        }
    }
    fn draw_objective(&self, area: Rect) -> bool {
        objective(self, area);
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        result(self, area);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lines(value: &str, width: f32, size: f32) -> usize {
        wrap_text(value, width, plain(size, DARK)).count()
    }
    fn play_until(game: &mut Game, done: impl Fn(&Game) -> bool) {
        for tick in 0..30_000 {
            if done(game) {
                return;
            }
            let input = game.autoplay(tick);
            game.step(&input);
        }
        panic!("the scripted investigator never reached the requested state");
    }
    #[test]
    fn authored_case_text_fits_the_dossier_and_edition_layouts() {
        for case in &CASES {
            // The premise clipping on the desk holds at most three lines.
            assert!(lines(case.opening, CLIP_W, 14.) <= 3, "{}", case.opening);
            assert!(text_width(case.confession, plain(16., DARK)) <= 848.);
            assert!(text_width(case.confession, plain(16., DARK)) <= 500. * 1.12);
            for place in &case.places {
                // Location page: evidence ends above the action buttons.
                assert!(290. + (lines(place.evidence, PAGE_W, 18.) - 1) as f32 * 27. < 490.);
                // Notebook page: evidence ends above the pinned cards.
                assert!(280. + (lines(place.evidence, PAGE_W, 18.) - 1) as f32 * 27. < 424.);
                for (name, size, room) in [(place.name, 24., 320.), (place.title, 24., 320.)] {
                    assert!(text_width(name, plain(size, DARK)) <= room * 1.12, "{name}");
                }
                assert!(
                    text_width(place.name, plain(17., DARK)) <= 160. * 1.12,
                    "{}",
                    place.name
                );
            }
            for suspect in &case.suspects {
                assert!(334. + (lines(suspect.statement, PAGE_W, 18.) - 1) as f32 * 27. < 490.);
                assert!(
                    text_width(suspect.name, plain(17., DARK)) <= 160. * 1.12,
                    "{}",
                    suspect.name
                );
            }
            // A signed witness keeps the struck denial and the signature above the buttons.
            let culprit = &case.suspects[case.culprit];
            let struck_end = 330. + lines(culprit.statement, PAGE_W, 16.) as f32 * 22.;
            let signed_lines = lines(case.confession, PAGE_W - 30., 18.);
            assert!(struck_end + 15. + (signed_lines - 1) as f32 * 27. < 490.);
            // Edition quotes stay above the closing rule and campaign standing.
            let evidence = case.places[0].evidence;
            assert!(lines(evidence, 390., 14.) <= 4, "{evidence}");
            assert!(
                lines(culprit.statement, 370., 14.) <= 4,
                "{}",
                culprit.statement
            );
            for value in case.methods.iter().chain(&case.motives) {
                assert!(text_width(value, plain(20., DARK)) <= 400., "{value}");
            }
        }
    }
    #[test]
    fn progress_pips_span_the_whole_campaign() {
        let mut game = Game::new();
        assert_eq!(game.progress(), Some((1, 6)));
        play_until(&mut game, |g| g.case_number() == 3);
        assert_eq!(game.progress(), Some((3, 6)));
        play_until(&mut game, |g| g.outcome() != Outcome::Playing);
        assert_eq!(game.outcome(), Outcome::Won);
        assert_eq!(game.progress(), Some((6, 6)));
    }
    #[test]
    fn a_connected_contradiction_bursts_on_the_pinboard() {
        let mut game = Game::new();
        for tick in 0..10_000 {
            let input = game.autoplay(tick);
            game.step(&input);
            if game.linked {
                break;
            }
        }
        assert!(game.events.iter().any(|e| e == Event::Linked));
        let mut fx = Juice::new();
        game.effects(Cue::Good, &mut fx);
        let counts = fx.counts();
        assert!(counts.sparks >= 40, "{counts:?}");
        assert!(counts.rings >= 1 && counts.popups >= 1, "{counts:?}");
        // The next quiet tick requests nothing more.
        game.step(&Input::default());
        let mut quiet = Juice::new();
        game.effects(Cue::None, &mut quiet);
        assert!(quiet.is_idle());
    }
    #[test]
    fn a_retracted_accusation_breaks_a_heart_in_the_header() {
        let mut game = Game::new();
        play_until(&mut game, Game::can_file);
        game.step(&Input {
            slot: Some(4),
            ..Input::default()
        });
        // The all-first draft is wrong for the first case.
        let cue = game.step(&Input {
            primary: true,
            ..Input::default()
        });
        assert_eq!(cue, Cue::Bad);
        assert_eq!(game.events.iter().next(), Some(Event::Retracted(3)));
        let mut fx = Juice::new();
        game.effects(cue, &mut fx);
        let counts = fx.counts();
        assert!(counts.sparks > 0 && counts.rings >= 2, "{counts:?}");
        assert!(counts.popups >= 2, "{counts:?}");
        assert_eq!(heart_at(3), vec2(HEADER_X + 315., HEADER_CY));
    }
    #[test]
    fn refusals_answer_with_a_small_local_effect() {
        let mut game = Game::new();
        game.step(&Input {
            slot: Some(3),
            ..Input::default()
        });
        assert_eq!(
            game.step(&Input {
                primary: true,
                ..Input::default()
            }),
            Cue::None
        );
        assert_eq!(game.events.iter().next(), Some(Event::Missing(0)));
        let mut fx = Juice::new();
        game.effects(Cue::None, &mut fx);
        assert_eq!(fx.counts().popups, 1);
        assert_eq!(fx.counts().rings, 1);
        assert_eq!(fx.counts().sparks, 0);
    }
    #[test]
    fn hud_copy_stays_within_the_word_budget() {
        let mut game = Game::new();
        let hud = game.hud();
        assert_eq!(hud.stage, "The Last Carbon");
        assert!(hud.objective.split_whitespace().count() <= 5);
        play_until(&mut game, |g| g.outcome() != Outcome::Playing);
        let hud = game.hud();
        assert!(hud.objective.split_whitespace().count() <= 5);
        let summary = game.summary();
        assert_eq!(
            summary,
            "All six cases closed with 4/4 credibility: Editor of Record."
        );
        assert_eq!(game.contradictions(), 6);
    }
    #[test]
    fn help_is_at_most_four_short_bullets() {
        let help = crate::META.help;
        assert!(!help.is_empty() && help.len() <= 4);
        for bullet in help {
            assert!(bullet.split_whitespace().count() < 15, "{bullet}");
            assert!(bullet.is_ascii());
            // One line each in the host's 600-unit guide column, so the
            // whole guide fits on a single page after the controls.
            assert!(text_width(bullet, style(18., PAPER)) <= 600., "{bullet}");
        }
        assert!(crate::META.subtitle.split_whitespace().count() <= 3);
    }
}
