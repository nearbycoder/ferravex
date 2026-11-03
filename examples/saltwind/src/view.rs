//! Read-only nautical chart and trading ledger presentation.
//!
//! Layout rectangles here mirror the model's pointer hit regions: tabs at
//! `650 + 57 * i` (y 118..149), list rows per page, and the two action
//! buttons at y 478 and 514 (x 658..926).
use crate::content::{CHARTERS, GOODS, PORTS};
use crate::model::{DAY_TICKS, DEADLINE, DEBT, Game};
use ferravex_arcade::shapes::{self, Cap, Fill, Stroke};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, wrap_text};
use ferravex_arcade::*;
use std::f32::consts::{PI, TAU};

const DEEP: Color = Color::new(0.025, 0.105, 0.14, 1.);
const FOAM: Color = Color::new(0.62, 0.84, 0.82, 1.);
const SAND: Color = Color::new(0.87, 0.80, 0.62, 1.);
const PARCHMENT: Color = Color::new(0.93, 0.88, 0.72, 1.);
const GOLD: Color = Color::new(0.95, 0.73, 0.35, 1.);
const DARK: Color = Color::new(0.14, 0.21, 0.22, 1.);
const LEAF: Color = Color::new(0.25, 0.42, 0.30, 1.);
const RUST: Color = Color::new(0.71, 0.33, 0.22, 1.);
const SOFT: Color = Color::new(0.42, 0.46, 0.40, 1.);
const TEAL_TEXT: Color = Color::new(0.58, 0.74, 0.72, 1.);

/// Sidebar content column (matches the model's row and button hit regions).
const LEFT: f32 = 662.;
const RIGHT: f32 = 916.;
const ROW_X: f32 = 658.;
const ROW_W: f32 = 268.;

// ---------------------------------------------------------------------------
// Typography

/// Ink on parchment: crisp, without the drop shadow light text gets on the sea.
fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
fn label(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(size, color)).width
}
fn label_right(value: &str, right: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(
        value,
        right,
        y,
        ink(size, color).with_align(TextAlign::Right),
    )
    .width
}
fn label_center(value: &str, cx: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, cx, y, ink(size, color).with_align(TextAlign::Center)).width
}
/// Small uppercase, letter-spaced section label.
fn eyebrow(value: &str, x: f32, y: f32, color: Color) {
    let s = TextStyle::modern(11., color).with_letter_spacing(1.4);
    draw_text_styled(&value.to_uppercase(), x, y, s);
}
/// Paragraph clamped to `max` lines; returns the next baseline.
fn paragraph(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color, max: usize) -> f32 {
    let s = ink(size, color);
    let step = size * 1.32;
    let mut baseline = y;
    for line in wrap_text(value, width, s).take(max) {
        draw_text_styled(line, x, baseline, s);
        baseline += step;
    }
    baseline
}
/// Eyebrow and title shared by every ledger page.
fn header(kicker: &str, title: &str, color: Color) {
    eyebrow(kicker, LEFT, 165., SOFT);
    let s = ink(20., color);
    let w = ferravex_arcade::typography::text_width(title, s);
    let room = RIGHT + 8. - LEFT;
    let s = if w > room {
        s.with_size(20. * room / w)
    } else {
        s
    };
    draw_text_styled(title, LEFT, 190., s);
}
/// A muted label on the left and a value on the right of the column.
fn pair(name: &str, value: &str, y: f32, color: Color) {
    label(name, LEFT, y, 13., SOFT);
    label_right(value, RIGHT + 8., y, 15., color);
}

// ---------------------------------------------------------------------------
// Ledger widgets

fn row(y: f32, height: f32, selected: bool) {
    let (top, h) = (y + 1., height - 3.);
    if selected {
        rounded(ROW_X, top, ROW_W, h, 8., lighten(PARCHMENT, 0.45));
        outline(ROW_X, top, ROW_W, h, 8., 1., with_alpha(RUST, 0.55));
        shapes::fill_capsule(
            vec2(ROW_X + 4.5, top + 7.),
            vec2(ROW_X + 4.5, top + h - 7.),
            1.5,
            RUST,
        );
    } else {
        rounded(ROW_X, top, ROW_W, h, 8., darken(PARCHMENT, 0.045));
    }
}

/// Painted action button inside the model's click rectangle.
fn button(y: f32, key: &str, action: &str, primary: bool) {
    let h = 31.;
    if primary {
        rounded_gradient(
            ROW_X,
            y,
            ROW_W,
            h,
            9.,
            lighten(DARK, 0.1),
            darken(DARK, 0.1),
        );
    } else {
        rounded(ROW_X, y, ROW_W, h, 9., with_alpha(DARK, 0.11));
    }
    let mut x = ROW_X + 8.;
    if !key.is_empty() {
        x += keycap(key, "", x, y + 4.5, MUTED) + 9.;
    }
    let s = if primary {
        ink(14., PARCHMENT)
    } else {
        ink(14., DARK)
    };
    draw_text_styled(action, x, s.centered_baseline(y + h * 0.5), s);
}

/// Lightweight progress bar: recessed track and a rounded fill.
fn bar(x: f32, y: f32, w: f32, h: f32, fraction: f32, color: Color) {
    let f = if fraction.is_finite() {
        fraction.clamp(0., 1.)
    } else {
        0.
    };
    let r = h * 0.5;
    rounded(x, y, w, h, r, with_alpha(DARK, 0.16));
    if f > 0. {
        rounded(x, y, (w * f).max(h), h, r, color);
    }
}

fn tag(value: &str, right: f32, y: f32, color: Color) -> f32 {
    let w = measure(value, 12.) + 15.6;
    chip(value, right - w, y, 12., color);
    w
}

/// Wax seal for one charter: filled and checked once delivered.
fn seal(x: f32, y: f32, n: usize, complete: bool, active: bool) {
    let c = vec2(x, y);
    shapes::fill_circle(c + vec2(0., 2.), 15., Color::new(0.3, 0.2, 0.1, 0.18));
    if complete {
        shapes::fill_circle(
            c,
            14.,
            Fill::radial(
                c - vec2(4., 5.),
                20.,
                lighten(LEAF, 0.25),
                darken(LEAF, 0.15),
            ),
        );
        shapes::stroke_circle(c, 10.5, 1., with_alpha(PARCHMENT, 0.4));
        let tick = [c + vec2(-5., 0.), c + vec2(-1.5, 4.), c + vec2(5.5, -4.)];
        shapes::polyline(&tick, Stroke::new(2.2, PARCHMENT).cap(Cap::Round));
    } else {
        shapes::fill_circle(
            c,
            14.,
            if active {
                lighten(PARCHMENT, 0.5)
            } else {
                darken(PARCHMENT, 0.06)
            },
        );
        shapes::stroke_circle(
            c,
            14.,
            if active { 2. } else { 1. },
            if active { RUST } else { with_alpha(DARK, 0.25) },
        );
        label_center(
            &format!("{}", n + 1),
            x,
            y + 5.,
            14.,
            if active { RUST } else { SOFT },
        );
    }
}

// ---------------------------------------------------------------------------
// Chart art

fn line(a: Vec2, b: Vec2, width: f32, color: Color) {
    draw_line(a.x, a.y, b.x, b.y, width, color);
}

/// Sea-motion phase in ticks. Ports are timeless planning, so the water
/// only drifts while a passage advances.
fn phase(game: &Game) -> f32 {
    let at_sea = game.passage.map_or(0, |p| p.day_ticks);
    (game.day * DAY_TICKS + at_sea) as f32
}

fn island(port: usize, chosen: bool, destination: bool, t: f32) {
    let p = Game::position(port);
    // Soft two-step shadow, a shallow-water shelf with a foam line, then land.
    draw_ellipse(
        p.x + 4.,
        p.y + 8.,
        43.,
        26.,
        -17.,
        Color::new(0.0, 0.05, 0.07, 0.42),
    );
    draw_ellipse(p.x, p.y, 41., 29., -17., Color::new(0.16, 0.38, 0.40, 1.));
    draw_ellipse(p.x, p.y - 1., 35., 23., -17., with_alpha(FOAM, 0.55));
    draw_ellipse(p.x, p.y - 2., 33., 21., -17., SAND);
    draw_ellipse(p.x - 1., p.y - 4., 26., 15.5, -17., LEAF);
    draw_ellipse(p.x - 5., p.y - 8., 16., 8., -17., lighten(LEAF, 0.14));
    // Pier.
    panel(
        p.x - 3.,
        p.y + 7.,
        6.,
        18.,
        Color::new(0.47, 0.31, 0.22, 1.),
    );
    panel(
        p.x - 3.,
        p.y + 7.,
        2.,
        18.,
        Color::new(0.58, 0.40, 0.28, 1.),
    );
    // Houses with shaded walls and pitched roofs.
    let roof = if port.is_multiple_of(2) { RUST } else { DARK };
    for house in 0..3 {
        let x = p.x - 17. + house as f32 * 12.;
        let y = p.y - 3. - (house % 2) as f32 * 5.;
        panel(x + 1., y + 1., 9., 9., Color::new(0.1, 0.16, 0.1, 0.35));
        panel(x, y, 9., 9., PARCHMENT);
        panel(x + 6., y, 3., 9., darken(PARCHMENT, 0.12));
        draw_triangle(
            vec2(x - 2., y),
            vec2(x + 4.5, y - 7.),
            vec2(x + 11., y),
            roof,
        );
        draw_triangle(
            vec2(x + 4.5, y - 7.),
            vec2(x + 11., y),
            vec2(x + 6.5, y),
            darken(roof, 0.2),
        );
        panel(x + 3., y + 4., 3., 5., DARK);
    }
    if port == 1 || port == 5 {
        // Lighthouse with a lamp halo.
        panel(p.x + 18., p.y - 27., 6., 24., PARCHMENT);
        panel(p.x + 18., p.y - 19., 6., 4., RUST);
        panel(p.x + 17., p.y - 27., 8., 5., GOLD);
        draw_triangle(
            vec2(p.x + 15., p.y - 27.),
            vec2(p.x + 21., p.y - 33.),
            vec2(p.x + 27., p.y - 27.),
            RUST,
        );
        draw_circle(p.x + 21., p.y - 25., 7., Color::new(1., 0.85, 0.45, 0.18));
    }
    if chosen {
        draw_circle_lines(p.x, p.y, 42., 2., GOLD);
        draw_circle_lines(p.x, p.y, 47., 1., with_alpha(GOLD, 0.3));
    }
    if destination {
        // Charter marker, bobbing with the swell.
        let bob = (t * 0.06).sin() * 2.;
        let tip = vec2(p.x, p.y - 35. + bob);
        draw_triangle(tip + vec2(-7., -10.), tip + vec2(7., -10.), tip, GOLD);
        draw_triangle(
            tip + vec2(0., -10.),
            tip + vec2(7., -10.),
            tip,
            darken(GOLD, 0.18),
        );
    }
}

/// Name plate; drawn after the ship so names stay legible over the wake.
fn island_label(port: usize, visited: bool, destination: bool) {
    let p = Game::position(port);
    let name = PORTS[port].name;
    let size = 15.;
    let width = measure(name, size) + if destination { 12. } else { 0. };
    let center = if p.y > 450. {
        p + vec2(width * 0.5 + 44., 0.)
    } else {
        p + vec2(0., 42.)
    };
    let (x, y, w, h) = (
        center.x - width * 0.5 - 8.,
        center.y - 10.,
        width + 16.,
        20.,
    );
    rounded(x, y, w, h, 10., Color::new(0.02, 0.1, 0.13, 0.84));
    if destination {
        outline(x, y, w, h, 10., 1., with_alpha(GOLD, 0.75));
        shapes::fill_circle(vec2(x + 11., center.y), 3., GOLD);
    } else if !visited {
        outline(x, y, w, h, 10., 1., with_alpha(FOAM, 0.18));
    }
    let s = TextStyle::modern(size, if visited { PARCHMENT } else { TEAL_TEXT });
    let left = center.x - width * 0.5 + if destination { 12. } else { 0. };
    draw_text_styled(name, left, s.centered_baseline(center.y), s);
}

fn boat(position: Vec2, heading: Vec2, reefed: bool, t: f32) {
    let forward = if heading.length_squared() < 0.01 {
        vec2(0.3, -1.).normalize()
    } else {
        heading.normalize()
    };
    let side = vec2(-forward.y, forward.x);
    let bob = (t * 0.11).sin() * 0.8;
    let position = position + side * bob;
    // Fading wake on both quarters.
    for k in 0..3 {
        let a = 12. + k as f32 * 6.;
        let b = a + 6.;
        let alpha = 0.55 - k as f32 * 0.17;
        for s in [-1., 1.] {
            line(
                position - forward * a + side * s * (3. + k as f32 * 2.),
                position - forward * b + side * s * (5. + k as f32 * 2.),
                1.6,
                with_alpha(FOAM, alpha),
            );
        }
    }
    // Shadow, hull, deck, mast, sails and pennant.
    let drop = vec2(2.5, 3.);
    draw_triangle(
        position + forward * 19. + drop,
        position - forward * 12. - side * 9. + drop,
        position - forward * 12. + side * 9. + drop,
        Color::new(0., 0.04, 0.06, 0.4),
    );
    draw_triangle(
        position + forward * 19.,
        position - forward * 12. - side * 9.,
        position - forward * 12. + side * 9.,
        DARK,
    );
    draw_triangle(
        position + forward * 15.,
        position - forward * 8. - side * 6.,
        position - forward * 8. + side * 6.,
        RUST,
    );
    draw_triangle(
        position + forward * 15.,
        position - forward * 8. + side * 6.,
        position - forward * 6.,
        darken(RUST, 0.2),
    );
    line(position - forward * 10., position + forward * 11., 2., GOLD);
    let reach = if reefed { 7. } else { 14. };
    draw_triangle(
        position + forward * 10.,
        position + side * reach - forward * 5.,
        position - forward * 7.,
        PARCHMENT,
    );
    draw_triangle(
        position + forward * 9.,
        position - side * (reach - 3.) - forward * 2.,
        position - forward * 5.,
        SAND,
    );
    draw_triangle(
        position + forward * 13.,
        position + forward * 18. + side * 7.,
        position + forward * 9. + side * 6.,
        GOLD,
    );
}

fn compass(game: &Game) {
    let c = vec2(584., 152.);
    shapes::fill_circle(c, 22., Color::new(0.02, 0.1, 0.13, 0.55));
    draw_circle_lines(c.x, c.y, 22., 1., with_alpha(SAND, 0.55));
    draw_circle_lines(c.x, c.y, 17., 1., with_alpha(SAND, 0.18));
    // Four-point rose, light and dark halves.
    for k in 0..4 {
        let d = vec2((k as f32 * PI * 0.5).cos(), (k as f32 * PI * 0.5).sin());
        let s = vec2(-d.y, d.x);
        draw_triangle(c + d * 19., c + s * 4., c, with_alpha(SAND, 0.75));
        draw_triangle(c + d * 19., c - s * 4., c, with_alpha(SAND, 0.4));
    }
    let wind = game.wind().normalize() * 17.;
    let tail = c - wind * 0.7;
    let head = c + wind;
    shapes::fill_capsule(tail, c + wind * 0.55, 1.4, GOLD);
    let side = vec2(-wind.y, wind.x).normalize() * 5.;
    draw_triangle(head, c + wind * 0.45 + side, c + wind * 0.45 - side, GOLD);
    label_center("N", c.x, c.y - 26., 10., SAND);
    let s = TextStyle::modern(10., with_alpha(SAND, 0.85))
        .with_letter_spacing(1.4)
        .with_align(TextAlign::Center);
    draw_text_styled("WIND", c.x + 1., c.y + 37., s);
}

fn storm(center: Vec2, radius: f32, t: f32) {
    draw_circle(center.x, center.y, radius, Color::new(0.14, 0.26, 0.3, 0.5));
    draw_circle(
        center.x,
        center.y,
        radius * 0.72,
        Color::new(0.2, 0.31, 0.35, 0.45),
    );
    let spin = t * 0.004;
    for k in 0..3 {
        let start = spin + k as f32 * TAU / 3.;
        shapes::stroke_arc(
            center,
            radius - 7.,
            start,
            1.3,
            Stroke::new(2., Color::new(0.62, 0.74, 0.74, 0.42)).cap(Cap::Round),
        );
    }
    draw_circle_lines(
        center.x,
        center.y,
        radius,
        1.,
        Color::new(0.45, 0.58, 0.6, 0.55),
    );
    let s = TextStyle::modern(10., Color::new(0.82, 0.9, 0.88, 1.)).with_letter_spacing(1.2);
    draw_text_styled(
        "SQUALL",
        center.x,
        center.y + 4.,
        s.with_align(TextAlign::Center),
    );
}

fn draw_chart(game: &Game) {
    let t = phase(game);
    rounded_gradient(
        16.,
        112.,
        612.,
        456.,
        10.,
        Color::new(0.07, 0.22, 0.26, 1.),
        DEEP,
    );
    // Neatline and graticule.
    let neat = with_alpha(SAND, 0.16);
    draw_rectangle_lines(24., 120., 596., 440., 1., neat);
    let grid = Color::new(0.12, 0.3, 0.33, 0.45);
    for x in 0..13 {
        let x = 38. + x as f32 * 46.;
        draw_line(x, 178., x, 515., 1., grid);
    }
    for y in 0..8 {
        let y = 189. + y as f32 * 46.;
        draw_line(30., y, 615., y, 1., grid);
    }
    // Swell marks drift gently while underway.
    for i in 0..44 {
        let drift = (t * 0.02 + i as f32 * 1.7).sin() * 3.;
        let x = 43. + ((i * 137) % 548) as f32 + drift;
        let y = 186. + ((i * 79) % 320) as f32;
        let c = Color::new(0.2, 0.42, 0.45, 0.55);
        line(vec2(x - 5., y), vec2(x, y + 2.), 1., c);
        line(vec2(x, y + 2.), vec2(x + 5., y), 1., c);
    }
    draw_text_styled("The Six Isles", 38., 146., style(24., SAND));
    draw_text_styled(
        "A chart for those who come home",
        39.,
        162.,
        TextStyle::modern(12., TEAL_TEXT),
    );
    compass(game);
    for (center, radius) in game.storms() {
        storm(center, radius, t);
    }
    let (from, to, sheltered) = game
        .passage
        .map_or((game.harbor, game.selected_port, game.sheltered), |p| {
            (p.from, p.to, p.sheltered)
        });
    if from != to {
        // Dashed course line; dashes march toward the destination at sea.
        let a = Game::position(from);
        let b = Game::position(to);
        let d = (b - a).normalize_or_zero();
        let side = vec2(-d.y, d.x);
        let at = |t: f32| a.lerp(b, t) + side * if sheltered { (t * PI).sin() * 34. } else { 0. };
        let dashes = 28;
        let march = (t * 0.01).fract();
        for i in 0..dashes {
            let s = (i as f32 + march) / dashes as f32;
            let e = s + 0.45 / dashes as f32;
            if e <= 1. {
                line(at(s), at(e), 2., with_alpha(GOLD, 0.85));
            }
        }
    }
    let destination = CHARTERS.get(game.charter).map(|c| c.destination);
    for port in 0..PORTS.len() {
        island(
            port,
            port == game.selected_port,
            Some(port) == destination,
            t,
        );
    }
    let heading = game.passage.map_or(vec2(0.3, -1.), |p| {
        Game::position(p.to) - Game::position(p.from)
    });
    boat(
        game.boat_position(),
        heading,
        game.passage.is_some_and(|p| p.reefed),
        t,
    );
    for port in 0..PORTS.len() {
        island_label(port, game.visited[port], Some(port) == destination);
    }
    // Dispatch strip.
    rounded(28., 523., 588., 37., 9., Color::new(0.015, 0.075, 0.1, 0.9));
    let fresh = game.notice_ticks > 0;
    let message = if fresh {
        game.notice.as_str()
    } else {
        game.news()
    };
    let dot = vec2(44., 541.5);
    shapes::fill_circle(dot, 4., if fresh { GOLD } else { with_alpha(FOAM, 0.6) });
    let s = TextStyle::modern(12.5, PARCHMENT);
    let count = wrap_text(message, 552., s).take(2).count();
    let step = 15.;
    let mut y = s.centered_baseline(541.5) - step * 0.5 * (count as f32 - 1.);
    for text in wrap_text(message, 552., s).take(2) {
        draw_text_styled(text, 56., y, s);
        y += step;
    }
}

// ---------------------------------------------------------------------------
// Ledger

fn sidebar_frame(game: &Game) {
    card(642., 112., 302., 456., PARCHMENT);
    // Segmented page control over the model's tab hit regions.
    rounded(648., 117., 290., 33., 10., darken(PARCHMENT, 0.1));
    let underway = game.passage.is_some();
    for (i, name) in ["Map", "Buy", "Jobs", "Refit", "Log"].iter().enumerate() {
        let x = 650. + i as f32 * 57.;
        let active = game.page == i && !underway;
        if active {
            rounded(x + 1., 119., 54., 29., 8., DARK);
        }
        let ink = if active {
            PARCHMENT
        } else if underway {
            with_alpha(DARK, 0.4)
        } else {
            DARK
        };
        let number = format!("{}", i + 1);
        let s = TextStyle::modern(13., ink);
        let n = TextStyle::modern(10., with_alpha(if active { GOLD } else { SOFT }, ink.a));
        let nw = ferravex_arcade::typography::text_width(&number, n) + 4.;
        let w = ferravex_arcade::typography::text_width(name, s) + nw;
        let x0 = x + 28. - w * 0.5;
        let base = s.centered_baseline(133.5);
        draw_text_styled(&number, x0, base, n);
        draw_text_styled(name, x0 + nw, base, s);
    }
}

fn chart_page(game: &Game) {
    header(
        &format!("Chart  /  Week {}", game.week() + 1),
        "Set a course",
        DARK,
    );
    let destination = CHARTERS.get(game.charter).map(|c| c.destination);
    for (i, port) in PORTS.iter().enumerate() {
        let y = 194. + i as f32 * 30.;
        let selected = game.selected_port == i;
        row(y, 30., selected);
        let mid = y + 14.5;
        let s = ink(15., DARK);
        draw_text_styled(port.name, 672., s.centered_baseline(mid), s);
        let mut right = RIGHT + 4.;
        if i == game.harbor {
            right -= tag("Docked", right, mid - 8., LEAF);
        } else {
            let days = game.voyage_days(i);
            let short = days > game.provisions;
            let s = ink(13., if short { RUST } else { SOFT });
            let text = format!("{days} days");
            right -= draw_text_styled(
                &text,
                right,
                s.centered_baseline(mid),
                s.with_align(TextAlign::Right),
            )
            .width;
        }
        if Some(i) == destination {
            tag("Charter", right - 8., mid - 8., RUST);
        }
    }
    let (name, note, tint) = if game.sheltered {
        ("Sheltered passage", "Adds 1 day. No storm damage.", LEAF)
    } else {
        (
            "Open sea passage",
            "Faster. Storms can damage the hull.",
            RUST,
        )
    };
    chip(name, LEFT, 382., 12., tint);
    label(note, LEFT, 418., 13., DARK);
    let needed = game.voyage_days(game.selected_port);
    let short = game.provisions < needed;
    label("Food aboard", LEFT, 446., 13., SOFT);
    label_right(
        &format!("{} days", game.provisions),
        RIGHT + 8.,
        446.,
        15.,
        if short { RUST } else { DARK },
    );
    bar(
        LEFT,
        454.,
        RIGHT + 8. - LEFT,
        6.,
        game.provisions as f32 / 12.,
        if short { RUST } else { LEAF },
    );
    button(478., "Space", "Set sail", true);
    button(514., "X", "Change passage type", false);
}

fn market_page(game: &Game) {
    header(
        &format!("Market  /  {}", PORTS[game.harbor].specialty),
        PORTS[game.harbor].name,
        DARK,
    );
    for (good, name) in GOODS.iter().enumerate() {
        let y = 206. + good as f32 * 40.;
        row(y, 40., game.row == good);
        label(name, 672., y + 18., 15., DARK);
        label_right(
            &format!("Buy {}c", game.buy_price(good)),
            RIGHT,
            y + 18.,
            14.,
            DARK,
        );
        label(
            &format!(
                "Hold {}  /  Stock {}",
                game.cargo(good),
                game.stock[game.harbor][good]
            ),
            672.,
            y + 32.,
            12.,
            SOFT,
        );
        label_right(
            &format!("Sell {}c", game.sell_price(good)),
            RIGHT,
            y + 32.,
            12.,
            SOFT,
        );
    }
    row(326., 40., game.row == 3);
    label("Provisions", 672., 344., 15., DARK);
    label_right("2c / day", RIGHT, 344., 14., DARK);
    label(
        &format!("Aboard {}/12  /  up to six days", game.provisions),
        672.,
        358.,
        12.,
        SOFT,
    );
    row(366., 40., game.row == 4);
    label("Hull repair", 672., 384., 15., DARK);
    label_right("1c / 2 hp", RIGHT, 384., 14., DARK);
    label(
        &format!("Hull {}%  /  up to thirty hull", game.hull),
        672.,
        398.,
        12.,
        SOFT,
    );
    // Purse and hold.
    let w = label(&format!("{}", game.coins), LEFT, 438., 20., DARK);
    label("crowns", LEFT + w + 6., 438., 12., SOFT);
    let full = game.cargo_total() >= game.capacity();
    label_right(
        &format!("Hold {}/{}", game.cargo_total(), game.capacity()),
        RIGHT + 8.,
        432.,
        12.,
        if full { RUST } else { SOFT },
    );
    bar(
        820.,
        438.,
        RIGHT + 8. - 820.,
        5.,
        game.cargo_total() as f32 / game.capacity() as f32,
        if full { RUST } else { LEAF },
    );
    label(
        "Buy low here, sell high at another port.",
        LEFT,
        462.,
        12.,
        SOFT,
    );
    button(478., "Space", "Buy / service", true);
    button(514., "X", "Sell one selected crate", false);
}

fn charter_page(game: &Game) {
    if let Some(job) = CHARTERS.get(game.charter) {
        header(
            &format!("Charter {} of 6", game.charter + 1),
            job.name,
            RUST,
        );
        divider(LEFT, 204., RIGHT + 8. - LEFT, with_alpha(DARK, 0.25));
        pair("Destination", PORTS[job.destination].name, 230., DARK);
        pair(
            "Cargo",
            &format!("{} {}", job.amount, GOODS[job.good]),
            256.,
            DARK,
        );
        pair("Reward", &format!("{} crowns", job.reward), 282., LEAF);
        let aboard = game.cargo(job.good);
        pair("Aboard", &format!("{aboard} / {}", job.amount), 314., DARK);
        let loaded = aboard >= job.amount;
        bar(
            LEFT,
            322.,
            RIGHT + 8. - LEFT,
            7.,
            aboard as f32 / job.amount as f32,
            if loaded { LEAF } else { GOLD },
        );
        let (state, tint) = if loaded && game.harbor == job.destination {
            ("Ready to deliver".to_owned(), LEAF)
        } else if loaded {
            (format!("Sail to {}", PORTS[job.destination].name), DARK)
        } else {
            (
                format!(
                    "Buy {} more {}",
                    job.amount - aboard,
                    GOODS[job.good].to_lowercase()
                ),
                RUST,
            )
        };
        chip(&state, LEFT, 342., 12., tint);
        eyebrow("Sealed charters", LEFT, 394., SOFT);
        for i in 0..6 {
            seal(
                679. + i as f32 * 44.,
                430.,
                i,
                i < game.charter,
                i == game.charter,
            );
        }
        button(478., "Space", "Deliver charter", true);
        button(514., "X", "Open market", false);
    } else {
        header("All six charters sealed", "Repay the debt", RUST);
        divider(LEFT, 204., RIGHT + 8. - LEFT, with_alpha(DARK, 0.25));
        paragraph(
            "Settle 900 crowns at Haven's shipyard to win back the Saltwind.",
            LEFT,
            228.,
            RIGHT + 8. - LEFT,
            14.,
            DARK,
            3,
        );
        pair("Treasury", &format!("{} crowns", game.coins), 300., LEAF);
        pair("Debt", &format!("{DEBT} crowns"), 326., RUST);
        bar(
            LEFT,
            336.,
            RIGHT + 8. - LEFT,
            7.,
            game.coins as f32 / DEBT as f32,
            if game.coins >= DEBT { LEAF } else { GOLD },
        );
        eyebrow("Sealed charters", LEFT, 394., SOFT);
        for i in 0..6 {
            seal(679. + i as f32 * 44., 430., i, true, false);
        }
        button(478., "4", "Shipyard / settle", true);
        button(514., "X", "Trade surplus cargo", false);
    }
}

fn shipyard_page(game: &Game) {
    header(
        &format!("Shipyard  /  {}", PORTS[game.harbor].name),
        "Refit and repay",
        DARK,
    );
    let offers = [
        ("Swift canvas", "85c", "Passages one day shorter"),
        ("Cargo expansion", "70c", "Hold grows from 8 to 12"),
        ("Brass plating", "70c", "Prevents 3 storm damage"),
        ("Settle ship debt", "900c", "At Haven, after all 6 charters"),
    ];
    for (i, (title, price, detail)) in offers.iter().enumerate() {
        let y = 199. + i as f32 * 60.;
        row(y, 60., game.row == i);
        label(title, 672., y + 24., 16., DARK);
        label(detail, 672., y + 43., 12., SOFT);
        if i < 3 && game.upgrades[i] {
            tag("Installed", RIGHT, y + 12., LEAF);
        } else {
            label_right(price, RIGHT, y + 24., 15., if i == 3 { RUST } else { DARK });
        }
    }
    label("Treasury", LEFT, 464., 13., SOFT);
    let w = label_right("crowns", RIGHT + 8., 464., 12., SOFT);
    label_right(&format!("{}", game.coins), RIGHT + 2. - w, 464., 18., DARK);
    button(478., "Space", "Purchase / settle", true);
    button(514., "X", "Return to chart", false);
}

fn log_page(game: &Game) {
    header(
        &format!("Captain's log  /  Week {}", game.week() + 1),
        &format!("Day {} of {}", game.day, DEADLINE),
        DARK,
    );
    bar(
        LEFT,
        200.,
        RIGHT + 8. - LEFT,
        5.,
        game.day as f32 / DEADLINE as f32,
        if game.day + 7 > DEADLINE { RUST } else { GOLD },
    );
    rounded(ROW_X, 214., ROW_W, 82., 9., darken(PARCHMENT, 0.05));
    eyebrow("Harbour news", LEFT + 6., 233., SOFT);
    paragraph(game.news(), LEFT + 6., 254., ROW_W - 20., 13., RUST, 3);
    pair("Port", PORTS[game.harbor].specialty, 334., LEAF);
    divider(LEFT, 344., RIGHT + 8. - LEFT, with_alpha(DARK, 0.18));
    pair(
        "Islands seen",
        &format!("{}/6", game.visited.iter().filter(|&&v| v).count()),
        368.,
        DARK,
    );
    divider(LEFT, 378., RIGHT + 8. - LEFT, with_alpha(DARK, 0.18));
    pair(
        "Charters sealed",
        &format!("{}/6", game.charter),
        402.,
        DARK,
    );
    divider(LEFT, 412., RIGHT + 8. - LEFT, with_alpha(DARK, 0.18));
    pair("Debt at Haven", &format!("{DEBT}c"), 436., RUST);
    button(478., "Space", "Wait one day", true);
    button(514., "X", "Return to chart", false);
}

fn sea_page(game: &Game) {
    let passage = game.passage.expect("underway");
    header(
        &format!("Underway from {}", PORTS[passage.from].name),
        PORTS[passage.to].name,
        RUST,
    );
    // Passage progress is stored in hundredths of a tick.
    let unit = DAY_TICKS * 100;
    let remaining = passage
        .total
        .saturating_sub(passage.traveled)
        .div_ceil(unit);
    pair(
        "Passage",
        &if remaining == 1 {
            "1 day to go".to_owned()
        } else {
            format!("{remaining} days to go")
        },
        220.,
        DARK,
    );
    bar(
        LEFT,
        228.,
        RIGHT + 8. - LEFT,
        7.,
        passage.traveled as f32 / passage.total as f32,
        LEAF,
    );
    let left = DEADLINE - game.day.min(DEADLINE);
    pair(
        "Calendar",
        &format!("Day {}, {} left", game.day, left),
        262.,
        if left < 6 { RUST } else { DARK },
    );
    divider(LEFT, 272., RIGHT + 8. - LEFT, with_alpha(DARK, 0.18));
    let hull_low = game.hull < 35;
    pair(
        "Hull",
        &format!("{}%", game.hull),
        298.,
        if hull_low { RUST } else { DARK },
    );
    bar(
        LEFT,
        306.,
        RIGHT + 8. - LEFT,
        7.,
        game.hull as f32 / 100.,
        if hull_low { RUST } else { LEAF },
    );
    let hungry = game.provisions < remaining;
    pair(
        "Food",
        &format!("{} days", game.provisions),
        338.,
        if hungry { RUST } else { DARK },
    );
    bar(
        LEFT,
        346.,
        RIGHT + 8. - LEFT,
        7.,
        game.provisions as f32 / 12.,
        if hungry { RUST } else { GOLD },
    );
    let (state, tint) = if passage.reefed {
        ("Sails reefed", GOLD)
    } else if passage.sheltered {
        ("Sheltered route", LEAF)
    } else {
        ("Open sea", RUST)
    };
    chip(state, LEFT, 372., 12., darken(tint, 0.15));
    paragraph(
        if passage.sheltered {
            "Sheltered route: no squall damage."
        } else {
            "Avoid storm rings. Reef sails to reduce damage."
        },
        LEFT,
        414.,
        RIGHT + 8. - LEFT,
        13.,
        DARK,
        3,
    );
    button(
        478.,
        "Space",
        if passage.reefed {
            "Unfurl sails"
        } else {
            "Reef sails"
        },
        true,
    );
    if passage.reefed {
        button(514., "", "Reefed: slower, less damage", false);
    } else {
        rounded(ROW_X, 514., ROW_W, 31., 9., with_alpha(DARK, 0.11));
        let mut x = ROW_X + 8.;
        x += keycap("Up", "", x, 518.5, MUTED) + 4.;
        x += keycap("Down", "", x, 518.5, MUTED) + 9.;
        let s = ink(14., DARK);
        draw_text_styled("Steer the ship", x, s.centered_baseline(529.5), s);
    }
}

impl ArcadeGame for Game {
    fn new() -> Self {
        Game::new()
    }
    fn update(&mut self, input: &Input) -> Cue {
        self.tick(input)
    }
    fn draw(&self) {
        draw_chart(self);
        sidebar_frame(self);
        if self.passage.is_some() {
            sea_page(self);
        } else {
            match self.page {
                0 => chart_page(self),
                1 => market_page(self),
                2 => charter_page(self),
                3 => shipyard_page(self),
                _ => log_page(self),
            }
        }
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!("Day {}/42 | Charters {}/6", self.day.min(42), self.charter),
            objective: CHARTERS.get(self.charter).map_or_else(
                || format!("Settle {DEBT} crowns at Haven's shipyard"),
                |c| {
                    format!(
                        "Deliver {} {} to {}",
                        c.amount, GOODS[c.good], PORTS[c.destination].name
                    )
                },
            ),
            status: format!(
                "{} crowns | Hull {}% | Food {}d | Cargo {}/{}",
                self.coins,
                self.hull,
                self.provisions,
                self.cargo_total(),
                self.capacity()
            ),
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        self.result()
    }
    fn summary(&self) -> String {
        if self.result == Outcome::Won {
            format!(
                "{}. All six charters fulfilled, debt paid, Saltwind yours. Home on day {} with {} crowns and {}% hull. Try a faster voyage, extra trade profits or a different refit for a higher captain's rank.",
                self.rank(),
                self.day,
                self.coins,
                self.hull
            )
        } else if self.result == Outcome::Lost {
            self.loss.into()
        } else {
            "The islands are waiting. Trade carefully and bring Saltwind home.".into()
        }
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
}
