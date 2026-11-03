use crate::{
    Game,
    content::{CROPS, MORNINGS, ORDERS, TOOL_DETAILS, TOOL_NAMES, TOOL_PRICES, Weather},
    model::{COLS, GRID_X, GRID_Y, PITCH_X, PITCH_Y, Page},
};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;
const CREAM: Color = Color::new(0.91, 0.87, 0.71, 1.);
const PAPER_BG: Color = Color::new(0.95, 0.92, 0.80, 1.);
const ROW: Color = Color::new(0.90, 0.86, 0.72, 1.);
const ROW_ON: Color = Color::new(0.84, 0.88, 0.69, 1.);
const DARK: Color = Color::new(0.18, 0.24, 0.17, 1.);
const DEEP: Color = Color::new(0.10, 0.15, 0.11, 1.);
const GREEN: Color = Color::new(0.34, 0.49, 0.26, 1.);
/// Green for text and chips on paper (dark enough to need no shadow).
const INK_GREEN: Color = Color::new(0.22, 0.40, 0.16, 1.);
const LEAF: Color = Color::new(0.51, 0.65, 0.32, 1.);
const GOLD: Color = Color::new(0.94, 0.72, 0.36, 1.);
const BROWN: Color = Color::new(0.39, 0.25, 0.16, 1.);
const SOFT: Color = Color::new(0.47, 0.40, 0.29, 1.);
const FAINT: Color = Color::new(0.57, 0.59, 0.43, 1.);
const WATER: Color = Color::new(0.40, 0.70, 0.80, 1.);
const RUST: Color = Color::new(0.72, 0.30, 0.20, 1.);
const SHADE: Color = Color::new(0.05, 0.04, 0.02, 0.22);
const CROP_NAMES: [&str; 3] = ["turnip", "bean", "pumpkin"];

/// Small uppercase, letter-spaced section label.
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, style(11., color).with_letter_spacing(1.5)).width
}
/// Shadowless face for text on flat surfaces (half the vertices of a shadowed label).
fn flat(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
/// Wrapped paragraph limited to `lines`; returns the next baseline.
fn para(value: &str, x: f32, mut y: f32, width: f32, size: f32, lines: usize, color: Color) -> f32 {
    let s = style(size, color);
    for line in wrap_text(value, width, s).take(lines) {
        draw_text_styled(line, x, y, s);
        y += size * 1.32;
    }
    y
}
/// Width a [`chip`] would occupy.
fn chip_width(label: &str, size: f32) -> f32 {
    text_width(label, style(size, PAPER)) + size * 1.3
}
fn chip_right(label: &str, right: f32, y: f32, size: f32, color: Color) {
    chip(label, right - chip_width(label, size), y, size, color);
}
/// A soft contact shadow under a rounded object (one cheap shape).
fn contact(x: f32, y: f32, w: f32, h: f32, r: f32) {
    rounded(x + 1., y + 3., w, h, r, SHADE);
}
/// Raised paper surface: a tight contact shadow, a lit gradient and a top rim.
/// Cheaper than [`card`] and its shadow stays within three units of the edge.
fn paper_card(x: f32, y: f32, w: f32, h: f32) {
    rounded(x, y + 3., w, h, 12., Color::new(0., 0., 0., 0.35));
    rounded_gradient(
        x,
        y,
        w,
        h,
        12.,
        lighten(PAPER_BG, 0.04),
        darken(PAPER_BG, 0.03),
    );
    panel(x + 12., y, w - 24., 1., Color::new(1., 1., 1., 0.6));
}
fn coin(x: f32, y: f32, r: f32) {
    shapes::fill_circle(vec2(x, y), r, darken(GOLD, 0.25));
    shapes::fill_circle(
        vec2(x, y - r * 0.1),
        r * 0.82,
        Fill::vertical(
            Rect::new(x - r, y - r, r * 2., r * 2.),
            lighten(GOLD, 0.25),
            GOLD,
        ),
    );
    panel(
        x - r * 0.12,
        y - r * 0.45,
        r * 0.24,
        r * 0.7,
        darken(GOLD, 0.2),
    );
}
/// Ellipse in one batched draw; `rotation` is in radians.
fn oval(x: f32, y: f32, rx: f32, ry: f32, rotation: f32, color: Color) {
    draw_ellipse(x, y, rx, ry, rotation.to_degrees(), color);
}
/// Small filled circle in one batched draw, tessellated by size.
fn dot(x: f32, y: f32, r: f32, color: Color) {
    draw_poly(x, y, (r * 0.9).clamp(8., 24.) as u8, r, 0., color);
}
fn droplet(x: f32, y: f32, r: f32, color: Color) {
    draw_triangle(
        vec2(x, y - r * 1.9),
        vec2(x - r * 0.85, y - r * 0.3),
        vec2(x + r * 0.85, y - r * 0.3),
        color,
    );
    dot(x, y, r, color);
    dot(
        x - r * 0.35,
        y - r * 0.2,
        r * 0.28,
        Color::new(1., 1., 1., 0.55),
    );
}
fn weather_color(weather: Weather) -> Color {
    match weather {
        Weather::Sun => GOLD,
        Weather::Heat => Color::new(0.90, 0.45, 0.24, 1.),
        Weather::Rain => Color::new(0.38, 0.62, 0.72, 1.),
        Weather::Frost => Color::new(0.40, 0.62, 0.78, 1.),
    }
}
fn weather_icon(weather: Weather, x: f32, y: f32, size: f32) {
    let c = weather_color(weather);
    match weather {
        Weather::Sun | Weather::Heat => {
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::TAU / 8.;
                let d = vec2(a.cos(), a.sin());
                draw_line(
                    x + d.x * size * 0.62,
                    y + d.y * size * 0.62,
                    x + d.x * size * 0.95,
                    y + d.y * size * 0.95,
                    (size * 0.14).max(1.5),
                    c,
                );
            }
            shapes::fill_circle(
                vec2(x, y),
                size * 0.45,
                Fill::radial(
                    vec2(x - size * 0.12, y - size * 0.12),
                    size * 0.5,
                    lighten(c, 0.4),
                    c,
                ),
            );
        }
        Weather::Rain => {
            let cloud = Color::new(0.72, 0.77, 0.80, 1.);
            for i in 0..3 {
                let dx = (i as f32 - 1.) * size * 0.42;
                draw_line(
                    x + dx,
                    y + size * 0.45,
                    x + dx - size * 0.15,
                    y + size * 0.85,
                    (size * 0.12).max(1.5),
                    c,
                );
            }
            dot(
                x - size * 0.35,
                y + size * 0.05,
                size * 0.38,
                darken(cloud, 0.08),
            );
            dot(
                x + size * 0.35,
                y + size * 0.05,
                size * 0.38,
                darken(cloud, 0.08),
            );
            dot(x, y - size * 0.15, size * 0.5, cloud);
        }
        Weather::Frost => {
            for i in 0..3 {
                let a = i as f32 * std::f32::consts::PI / 3. + std::f32::consts::FRAC_PI_2;
                let d = vec2(a.cos(), a.sin()) * size * 0.9;
                draw_line(
                    x - d.x,
                    y - d.y,
                    x + d.x,
                    y + d.y,
                    (size * 0.14).max(1.5),
                    c,
                );
            }
            dot(x, y, size * 0.2, lighten(c, 0.5));
        }
    }
}
/// Leaf-shaped rhombus (two triangles): `half_len` along `angle`, `half_wid` across.
fn kite(x: f32, y: f32, half_len: f32, half_wid: f32, angle: f32, color: Color) {
    let d = vec2(angle.cos(), angle.sin());
    let n = vec2(-d.y, d.x);
    let c = vec2(x, y);
    let (a, b) = (c - d * half_len, c + d * half_len);
    let (l, r) = (c + n * half_wid, c - n * half_wid);
    draw_triangle(a, l, b, color);
    draw_triangle(a, b, r, color);
}
fn crop_icon(crop: usize, x: f32, y: f32, scale: f32, age: f32, wilted: bool) {
    let stem = if wilted {
        Color::new(0.55, 0.42, 0.22, 1.)
    } else {
        LEAF
    };
    let height = (8. + age * 14.) * scale;
    let droop = if wilted { 3. * scale } else { 0. };
    let tilt = if wilted { 1.0 } else { 0.5 };
    draw_line(
        x,
        y + 3. * scale,
        x,
        y - height + droop,
        2. * scale,
        darken(stem, 0.1),
    );
    kite(
        x - 6. * scale,
        y - height + 4. * scale + droop,
        7.5 * scale,
        3. * scale,
        std::f32::consts::PI + tilt,
        darken(stem, 0.18),
    );
    kite(
        x + 6. * scale,
        y - height + 1. * scale + droop,
        7.5 * scale,
        3. * scale,
        -tilt,
        stem,
    );
    if age < 0.9 {
        dot(x, y + 3. * scale, 2.5 * scale, GOLD);
        return;
    }
    match crop {
        0 => {
            let bulb = Color::new(0.95, 0.90, 0.74, 1.);
            draw_triangle(
                vec2(x - 3. * scale, y + 7. * scale),
                vec2(x + 3. * scale, y + 7. * scale),
                vec2(x, y + 15. * scale),
                darken(bulb, 0.1),
            );
            shapes::fill_circle(
                vec2(x, y),
                10. * scale,
                Fill::radial(
                    vec2(x - 3. * scale, y - 2. * scale),
                    12. * scale,
                    bulb,
                    darken(bulb, 0.12),
                ),
            );
            oval(
                x,
                y - 4.5 * scale,
                9.5 * scale,
                5. * scale,
                0.,
                Color::new(0.72, 0.42, 0.55, 1.),
            );
            kite(
                x - 3. * scale,
                y - 6. * scale,
                3. * scale,
                1.2 * scale,
                -0.3,
                Color::new(1., 1., 1., 0.35),
            );
        }
        1 => {
            draw_line(
                x + 9. * scale,
                y + 10. * scale,
                x + 9. * scale,
                y - 28. * scale,
                2. * scale,
                BROWN,
            );
            for i in 0..3 {
                let pod = Color::new(0.58 + i as f32 * 0.04, 0.72, 0.34, 1.);
                let (px, py) = (
                    x + (i as f32 - 1.) * 6. * scale,
                    y - (i as f32 * 6.) * scale,
                );
                let a = std::f32::consts::FRAC_PI_2 + 0.25;
                kite(px, py, 9. * scale, 4. * scale, a, darken(pod, 0.12));
                kite(
                    px - 0.8 * scale,
                    py - 1. * scale,
                    6. * scale,
                    2. * scale,
                    a,
                    pod,
                );
            }
        }
        _ => {
            let orange = Color::new(0.88, 0.45, 0.19, 1.);
            oval(x, y, 15. * scale, 12. * scale, 0., darken(orange, 0.12));
            oval(x, y, 8. * scale, 12. * scale, 0., lighten(orange, 0.08));
            kite(
                x - 7. * scale,
                y - 5. * scale,
                3. * scale,
                1.5 * scale,
                -0.6,
                Color::new(1., 1., 1., 0.3),
            );
            draw_line(
                x,
                y - 11. * scale,
                x + 3. * scale,
                y - 17. * scale,
                4. * scale,
                BROWN,
            );
        }
    }
}
fn tool_icon(tool: usize, x: f32, y: f32, owned: bool) {
    let ink = if owned { GREEN } else { BROWN };
    match tool {
        0 => {
            let brass = Color::new(0.86, 0.66, 0.32, 1.);
            stroke_ring(x - 9., y - 10., 9., 3., ink);
            rounded_gradient(
                x - 15.,
                y - 8.,
                26.,
                20.,
                5.,
                lighten(brass, 0.2),
                darken(brass, 0.1),
            );
            draw_line(x + 9., y - 3., x + 22., y - 13., 5., brass);
            dot(x + 22., y - 13., 4., darken(brass, 0.15));
        }
        1 => {
            for dx in [-15., 3.] {
                rounded_gradient(x + dx, y - 16., 12., 26., 3., lighten(ink, 0.15), ink);
                rounded(x + dx, y + 2., 20., 10., 4., darken(ink, 0.1));
            }
        }
        _ => {
            let glass = Color::new(0.70, 0.84, 0.80, 1.);
            rounded_gradient(
                x - 20.,
                y - 14.,
                40.,
                28.,
                3.,
                lighten(glass, 0.2),
                darken(glass, 0.08),
            );
            outline(x - 20., y - 14., 40., 28., 3., 2.5, ink);
            panel(x - 1., y - 13., 2., 26., ink);
            panel(x - 19., y - 1., 38., 2., ink);
            draw_line(
                x - 15.,
                y - 9.,
                x - 8.,
                y - 9.,
                2.,
                Color::new(1., 1., 1., 0.6),
            );
        }
    }
}
fn stroke_ring(x: f32, y: f32, r: f32, w: f32, color: Color) {
    shapes::stroke_circle(vec2(x, y), r, w, color);
}
fn lantern(x: f32, y: f32, lit: bool) {
    let body = if lit {
        GOLD
    } else {
        Color::new(0.62, 0.62, 0.52, 1.)
    };
    draw_line(x, y - 14., x, y - 9., 1.5, BROWN);
    rounded(x - 5., y - 11., 10., 3., 1.5, BROWN);
    shapes::fill_circle(
        vec2(x, y),
        9.,
        Fill::radial(
            vec2(x - 2., y - 3.),
            11.,
            lighten(body, if lit { 0.45 } else { 0.15 }),
            darken(body, 0.15),
        ),
    );
    for dx in [-4., 0., 4.] {
        draw_line(
            x + dx,
            y - 8.,
            x + dx,
            y + 8.,
            1.,
            with_alpha(darken(body, 0.35), 0.6),
        );
    }
    rounded(x - 4., y + 8., 8., 3., 1.5, BROWN);
}
impl Game {
    fn tabs(&self) {
        rounded(22., 116., 600., 34., 10., with_alpha(DEEP, 0.85));
        for (i, name) in ["Field", "Stall", "Village", "Tools", "Almanac"]
            .iter()
            .enumerate()
        {
            let x = 22. + i as f32 * 120.;
            let on = self.page.index() == i;
            if on {
                rounded(x + 2., 121., 116., 34., 9., Color::new(0., 0., 0., 0.25));
                rounded_gradient(
                    x + 2.,
                    118.,
                    116.,
                    30.,
                    8.,
                    lighten(GREEN, 0.14),
                    darken(GREEN, 0.06),
                );
                panel(x + 12., 119., 96., 1., Color::new(1., 1., 1., 0.18));
            }
            if on {
                rounded(x + 12., 124., 18., 18., 5., Color::new(0., 0., 0., 0.22));
            }
            let num = flat(11., if on { CREAM } else { FAINT }).with_align(TextAlign::Center);
            draw_text_styled(
                &(i + 1).to_string(),
                x + 21.,
                num.centered_baseline(133.),
                num,
            );
            let s = if on {
                style(15., CREAM)
            } else {
                flat(15., lighten(FAINT, 0.12))
            };
            draw_text_styled(name, x + 38., s.centered_baseline(133.), s);
        }
    }
    fn farm(&self) {
        let weather = self.weather();
        let (sky_top, sky_low) = match weather {
            Weather::Sun => (
                Color::new(0.56, 0.76, 0.84, 1.),
                Color::new(0.93, 0.89, 0.72, 1.),
            ),
            Weather::Rain => (
                Color::new(0.42, 0.51, 0.56, 1.),
                Color::new(0.68, 0.74, 0.72, 1.),
            ),
            Weather::Heat => (
                Color::new(0.86, 0.60, 0.42, 1.),
                Color::new(0.98, 0.86, 0.60, 1.),
            ),
            Weather::Frost => (
                Color::new(0.64, 0.75, 0.86, 1.),
                Color::new(0.90, 0.93, 0.95, 1.),
            ),
        };
        // Sky, two layers of hills and the field, inside one rounded frame.
        rounded_gradient(22., 156., 600., 372., 12., sky_top, sky_low);
        for i in 0..4 {
            let cx = 96. + i as f32 * 150.;
            draw_circle(cx, 268., 74., Color::new(0.55, 0.66, 0.43, 1.));
        }
        for i in 0..3 {
            let cx = 170. + i as f32 * 150.;
            draw_circle(cx, 262., 52., Color::new(0.46, 0.59, 0.35, 1.));
        }
        let ground = Rect::new(22., 226., 600., 260.);
        shapes::fill_rounded_rect(
            ground,
            0.,
            Fill::vertical(
                ground,
                Color::new(0.70, 0.66, 0.42, 1.),
                Color::new(0.62, 0.57, 0.36, 1.),
            ),
        );
        rounded(22., 470., 600., 58., 12., Color::new(0.62, 0.57, 0.36, 1.));
        panel(22., 226., 600., 2., Color::new(1., 1., 1., 0.12));
        // Farmhouse with chimney, glowing windows and a water barrel.
        contact(62., 222., 104., 6., 3.);
        panel(132., 160., 10., 22., Color::new(0.45, 0.30, 0.24, 1.));
        rounded_gradient(
            66.,
            182.,
            92.,
            43.,
            2.,
            Color::new(0.74, 0.44, 0.30, 1.),
            Color::new(0.58, 0.33, 0.22, 1.),
        );
        draw_triangle(
            vec2(56., 186.),
            vec2(111., 154.),
            vec2(168., 186.),
            Color::new(0.36, 0.24, 0.20, 1.),
        );
        draw_line(56., 186., 111., 154., 3., Color::new(0.48, 0.33, 0.26, 1.));
        draw_line(111., 154., 168., 186., 3., Color::new(0.48, 0.33, 0.26, 1.));
        rounded(102., 198., 20., 27., 4., DARK);
        dot(117., 212., 1.5, GOLD);
        for wx in [74., 132.] {
            rounded(
                wx - 1.,
                191.,
                20.,
                17.,
                3.,
                Color::new(0.36, 0.24, 0.20, 1.),
            );
            rounded_gradient(wx + 1., 193., 16., 13., 2., lighten(GOLD, 0.3), GOLD);
            panel(wx + 8.5, 193., 1., 13., Color::new(0.36, 0.24, 0.20, 0.8));
        }
        oval(187., 223., 15., 4., 0., SHADE);
        rounded_gradient(
            174.,
            196.,
            26.,
            27.,
            6.,
            Color::new(0.52, 0.34, 0.22, 1.),
            BROWN,
        );
        panel(174., 203., 26., 2., Color::new(0.25, 0.16, 0.10, 1.));
        panel(174., 214., 26., 2., Color::new(0.25, 0.16, 0.10, 1.));
        oval(187., 197., 12., 4., 0., Color::new(0.42, 0.64, 0.70, 1.));
        // Clothesline of order lanterns.
        draw_line(370., 190., 505., 194., 1.5, BROWN);
        draw_line(370., 186., 370., 226., 3., BROWN);
        draw_line(505., 190., 505., 226., 3., BROWN);
        for i in 0..3 {
            if self.delivered[i] {
                dot(399. + i as f32 * 37., 207., 17., with_alpha(GOLD, 0.16));
                dot(399. + i as f32 * 37., 207., 13., with_alpha(GOLD, 0.22));
            }
            lantern(399. + i as f32 * 37., 207., self.delivered[i]);
        }
        weather_icon(weather, 572., 186., 18.);
        // Picket fence.
        let fence = Color::new(0.96, 0.92, 0.80, 1.);
        draw_line(30., 222., 614., 222., 2., darken(fence, 0.1));
        draw_line(30., 230., 614., 230., 2., darken(fence, 0.1));
        for i in 0..22 {
            let x = 31. + i as f32 * 27.5;
            panel(x, 216., 4., 20., fence);
            draw_triangle(vec2(x, 216.), vec2(x + 4., 216.), vec2(x + 2., 213.), fence);
        }
        let selected = self.cursor;
        for (i, plot) in self.plots.iter().enumerate() {
            let x = GRID_X + (i % COLS) as f32 * PITCH_X;
            let y = GRID_Y + (i / COLS) as f32 * PITCH_Y;
            self.plot(x, y, *plot, i == selected);
        }
        // A small gardener stands beside the chosen plot, never over its crop.
        let x = GRID_X + (self.cursor % COLS) as f32 * PITCH_X + 130.;
        let y = GRID_Y + (self.cursor / COLS) as f32 * PITCH_Y + 40.;
        oval(x, y + 13., 7., 2.5, 0., SHADE);
        rounded(x - 4., y + 6., 3., 7., 1.5, BROWN);
        rounded(x + 1., y + 6., 3., 7., 1.5, BROWN);
        rounded_gradient(x - 5., y - 2., 10., 11., 3., lighten(GREEN, 0.15), GREEN);
        shapes::fill_circle(vec2(x, y - 7.), 4.5, Color::new(0.94, 0.80, 0.64, 1.));
        oval(x, y - 10., 8., 2.5, 0., darken(GOLD, 0.1));
        oval(x, y - 12., 4.5, 3., 0., GOLD);
        // Seed selector.
        for (i, crop) in CROPS.iter().enumerate() {
            let x = 44. + i as f32 * 186.;
            let on = self.crop == i;
            if on {
                rounded(x, 497., 180., 30., 9., Color::new(0., 0., 0., 0.22));
                rounded_gradient(
                    x,
                    494.,
                    180.,
                    30.,
                    9.,
                    lighten(GREEN, 0.12),
                    darken(GREEN, 0.04),
                );
                outline(x, 494., 180., 30., 9., 1.5, GOLD);
            } else {
                rounded(x, 494., 180., 30., 9., Color::new(0.18, 0.24, 0.17, 0.72));
            }
            crop_icon(i, x + 18., 510., 0.55, 1., false);
            let s = if on {
                style(14., CREAM)
            } else {
                flat(14., lighten(FAINT, 0.2))
            };
            draw_text_styled(crop.name, x + 36., s.centered_baseline(509.), s);
            let count = format!("{}", self.seeds[i]);
            let c = flat(13., if on { DARK } else { CREAM }).with_align(TextAlign::Center);
            let w = (text_width(&count, c) + 12.).max(22.);
            rounded(
                x + 168. - w,
                500.,
                w,
                18.,
                9.,
                if on {
                    GOLD
                } else {
                    Color::new(1., 1., 1., 0.1)
                },
            );
            draw_text_styled(&count, x + 168. - w * 0.5, c.centered_baseline(509.), c);
        }
    }
    fn plot(&self, x: f32, y: f32, plot: crate::model::Plot, selected: bool) {
        let watered = plot.plant.is_some_and(|p| p.watered);
        let soil = if watered {
            Color::new(0.30, 0.23, 0.17, 1.)
        } else {
            Color::new(0.48 + f32::from(2 - plot.soil) * 0.04, 0.33, 0.21, 1.)
        };
        if selected {
            rounded(x - 5., y - 5., 134., 80., 13., with_alpha(GOLD, 0.25));
        }
        // Raised-bed front lip, then the soil itself.
        panel(x + 4., y + 60., 116., 14., darken(soil, 0.45));
        rounded_gradient(x, y, 124., 70., 9., lighten(soil, 0.08), darken(soil, 0.08));
        panel(x + 9., y + 1., 106., 1., Color::new(1., 1., 1., 0.16));
        for line in 0..4 {
            let ly = y + 16. + line as f32 * 13.;
            panel(x + 10., ly, 104., 2., Color::new(0.12, 0.08, 0.04, 0.24));
        }
        if selected {
            outline(x - 2., y - 2., 128., 74., 10., 2.5, GOLD);
        }
        if let Some(plant) = plot.plant {
            let progress = f32::from(plant.age) / f32::from(plant.nights);
            if plant.ready() {
                oval(
                    x + 62.,
                    y + 53.,
                    16.,
                    4.,
                    0.,
                    Color::new(0.1, 0.06, 0.03, 0.3),
                );
            }
            crop_icon(plant.crop, x + 62., y + 38., 1.15, progress, plant.wilted());
            if watered {
                droplet(x + 14., y + 18., 4., WATER);
            }
            if plant.wilted() {
                chip_right(
                    "Wilted",
                    x + 118.,
                    y + 6.,
                    10.,
                    Color::new(0.98, 0.55, 0.40, 1.),
                );
            } else if plant.ready() {
                chip_right(
                    "Ready",
                    x + 118.,
                    y + 6.,
                    10.,
                    Color::new(0.72, 0.95, 0.52, 1.),
                );
            } else if plant.stress > 0 {
                chip_right(
                    &format!("Dry {}/3", plant.stress),
                    x + 118.,
                    y + 6.,
                    10.,
                    GOLD,
                );
            }
            if !plant.ready() && !plant.wilted() {
                // One pip per growing night.
                let n = usize::from(plant.nights);
                let w = 10.;
                for k in 0..n {
                    let px = x + 10. + k as f32 * (w + 3.);
                    let on = k < usize::from(plant.age);
                    panel(
                        px,
                        y + 61.,
                        w,
                        3.,
                        if on {
                            LEAF
                        } else {
                            Color::new(1., 0.95, 0.85, 0.22)
                        },
                    );
                }
            }
        } else if selected {
            let hint = format!("Plant {}", CROP_NAMES[self.crop]);
            let s = flat(13., CREAM).with_align(TextAlign::Center);
            draw_text_styled(&hint, x + 62., s.centered_baseline(y + 35.), s);
        }
        // Soil richness: two pips at the bottom right.
        for k in 0..2u8 {
            let on = k < plot.soil;
            panel(
                x + 102. + f32::from(k) * 8.,
                y + 60.,
                5.,
                5.,
                if on {
                    GOLD
                } else {
                    Color::new(1., 0.95, 0.85, 0.18)
                },
            );
        }
    }
    fn ledger(&self) {
        let (l, r) = (660., 920.);
        paper_card(640., 118., 298., 446.);
        rounded(660., 118., 64., 3., 1.5, GOLD);
        eyebrow("FARM LEDGER", l, 144., SOFT);
        coin(l + 11., 172., 11.);
        let coins = format!("{}", self.coins);
        draw_text_styled(&coins, l + 30., 183., style(30., BROWN));
        text_right("Oven needs 80", r, 182., 12., SOFT);
        // Energy.
        text("Energy", l, 214., 13., SOFT);
        text_right(
            &format!("{} / {}", self.energy, self.max_energy()),
            r,
            214.,
            15.,
            DARK,
        );
        let f = f32::from(self.energy) / f32::from(self.max_energy());
        rounded(l, 222., r - l, 8., 4., darken(PAPER_BG, 0.13));
        if f > 0. {
            rounded_gradient(
                l,
                222.,
                ((r - l) * f).max(8.),
                8.,
                4.,
                lighten(LEAF, 0.15),
                GREEN,
            );
        }
        divider(l, 246., r - l, darken(PAPER_BG, 0.22));
        // Basket.
        eyebrow("BASKET", l, 266., SOFT);
        let needs = self.order().map(|i| ORDERS[i].needs);
        for (i, crop) in CROPS.iter().enumerate() {
            let y = 292. + i as f32 * 24.;
            crop_icon(i, l + 10., y - 5., 0.55, 1., false);
            text(crop.name, l + 28., y, 15., DARK);
            let have = self.produce(i);
            match needs {
                Some(n) if n[i] > 0 => {
                    let met = have >= n[i];
                    let w = text_right(&format!("/ {}", n[i]), r, y, 13., SOFT);
                    text_right(
                        &have.to_string(),
                        r - w - 7.,
                        y,
                        17.,
                        if met { INK_GREEN } else { BROWN },
                    );
                }
                _ => {
                    text_right(&have.to_string(), r, y, 17., BROWN);
                }
            }
        }
        divider(l, 356., r - l, darken(PAPER_BG, 0.22));
        // Selected plot.
        let plot = self.plots[self.cursor];
        eyebrow(&format!("PLOT {:02}", self.cursor + 1), l, 376., SOFT);
        let (soil, tint) = match plot.soil {
            2 => ("Rich soil", INK_GREEN),
            1 => ("Tired soil", BROWN),
            _ => ("Spent soil", RUST),
        };
        chip_right(soil, r, 364., 11., tint);
        let action = match plot.plant {
            None => Some("Plant selected seed"),
            Some(p) if p.wilted() => Some("Clear wilted crop"),
            Some(p) if p.ready() => Some("Harvest crop"),
            Some(p) if p.watered => None,
            _ => Some("Water this plot"),
        };
        match action {
            Some(a) => {
                keycap("Space", a, l, 388., DARK);
            }
            None => {
                droplet(l + 7., 400., 4.5, WATER);
                text("Watered, ready for night", l + 20., 403., 13., DARK);
            }
        }
        divider(l, 422., r - l, darken(PAPER_BG, 0.22));
        // Weather.
        let weather = self.weather();
        eyebrow("WEATHER", l, 442., SOFT);
        let next = if self.day < 14 {
            MORNINGS[usize::from(self.day)].weather.name()
        } else {
            "Festival"
        };
        text_right(&format!("Next: {next}"), r, 442., 12., SOFT);
        weather_icon(weather, l + 10., 460., 10.);
        text(
            &format!("Today: {}", weather.name()),
            l + 28.,
            466.,
            16.,
            DARK,
        );
        para(weather.advice(), l, 487., r - l, 13., 2, SOFT);
        // Primary action button; its click region is owned by the model.
        let act = match self.page {
            Page::Field => "Work plot",
            Page::Stall => "Buy seed",
            Page::Orders => "Deliver order",
            Page::Tools => "Buy tool",
            Page::Almanac => "Go to sleep",
        };
        rounded(658., 521., 264., 34., 9., Color::new(0.1, 0.12, 0.05, 0.25));
        rounded_gradient(
            658.,
            518.,
            264.,
            34.,
            9.,
            lighten(GREEN, 0.14),
            darken(GREEN, 0.06),
        );
        panel(668., 519., 244., 1., Color::new(1., 1., 1., 0.2));
        keycap("Space", "", 672., 525., CREAM);
        let s = flat(15., CREAM);
        draw_text_styled(act, 728., s.centered_baseline(535.), s);
    }
    fn paper(&self, title: &str, note: &str) {
        paper_card(22., 156., 600., 370.);
        rounded(44., 156., 64., 3., 1.5, GOLD);
        draw_text_styled(title, 44., 189., style(22., DARK));
        text_right(note, 600., 188., 13., SOFT);
    }
    fn row(&self, y: f32, on: bool) {
        if on {
            rounded(
                44.,
                y + 2.,
                556.,
                72.,
                10.,
                Color::new(0.2, 0.25, 0.1, 0.12),
            );
            rounded(44., y, 556., 72., 10., ROW_ON);
            outline(44., y, 556., 72., 10., 2., GREEN);
            rounded(50., y + 14., 4., 44., 2., GREEN);
        } else {
            rounded(44., y, 556., 72., 10., ROW);
        }
        shapes::fill_circle(
            vec2(84., y + 36.),
            25.,
            Color::new(1., 1., 1., if on { 0.42 } else { 0.32 }),
        );
    }
    fn stall(&self) {
        self.paper("Seed Stall", "Stock refills each morning");
        for (i, crop) in CROPS.iter().enumerate() {
            let y = 205. + i as f32 * 80.;
            self.row(y, self.row == i);
            crop_icon(i, 84., y + 40., 1.0, 1., false);
            text(crop.name, 122., y + 31., 19., DARK);
            text(
                &format!(
                    "{} nights to grow  ·  {} in stock  ·  {} in tin",
                    crop.nights, self.stock[i], self.seeds[i]
                ),
                122.,
                y + 53.,
                13.,
                SOFT,
            );
            for (k, (name, value, tint)) in
                [("BUY", crop.seed, INK_GREEN), ("SELL", crop.sale, BROWN)]
                    .into_iter()
                    .enumerate()
            {
                let bx = 452. + k as f32 * 74.;
                eyebrow(name, bx, y + 28., SOFT);
                coin(bx + 6., y + 46., 6.);
                text(&value.to_string(), bx + 17., y + 52., 18., tint);
            }
        }
        let x = 44. + keycap("Space", "Buy one seed", 44., 456., DARK) + 18.;
        keycap("X", "Sell one harvested crop", x, 456., DARK);
        text(
            "Keep ingredients that future village orders still need.",
            44.,
            506.,
            13.,
            SOFT,
        );
    }
    fn village(&self) {
        self.paper("Three Tables, One Village", "Deliver in order");
        let current = self.order();
        for (i, order) in ORDERS.iter().enumerate() {
            let y = 205. + i as f32 * 80.;
            self.row(y, current == Some(i));
            lantern(84., y + 38., self.delivered[i]);
            text(order.name, 122., y + 31., 19., DARK);
            text(
                &format!(
                    "Days {}–{}  ·  pays {} coins",
                    order.opens, order.deadline, order.reward
                ),
                122.,
                y + 53.,
                13.,
                SOFT,
            );
            let (status, tint) = if self.delivered[i] {
                ("Delivered".to_string(), INK_GREEN)
            } else if self.day < order.opens {
                (format!("Opens day {}", order.opens), SOFT)
            } else {
                ("Open now".to_string(), Color::new(0.80, 0.50, 0.12, 1.))
            };
            chip_right(&status, 588., y + 10., 11., tint);
            // Ingredients, right-aligned, green once the basket covers them.
            let mut right = 588.;
            for c in (0..3).rev() {
                let need = order.needs[c];
                if need == 0 {
                    continue;
                }
                let met = self.delivered[i] || self.produce(c) >= need;
                let w = text_right(
                    &format!("×{need}"),
                    right,
                    y + 58.,
                    15.,
                    if met { INK_GREEN } else { DARK },
                );
                crop_icon(c, right - w - 12., y + 53., 0.5, 1., false);
                right -= w + 34.;
            }
        }
        let letter = current.map_or("Orders complete. Keep 80 coins and finish day 14.", |i| {
            ORDERS[i].letter
        });
        eyebrow("LETTER", 44., 466., SOFT);
        para(letter, 44., 485., 556., 13., 3, DARK);
    }
    fn tools_page(&self) {
        self.paper("The Workshop", "Tools last the whole season");
        for i in 0..3 {
            let y = 205. + i as f32 * 80.;
            self.row(y, self.row == i);
            tool_icon(i, 84., y + 37., self.tools[i]);
            text(TOOL_NAMES[i], 122., y + 28., 18., DARK);
            para(TOOL_DETAILS[i], 122., y + 46., 340., 12., 2, SOFT);
            if self.tools[i] {
                chip_right("Owned  ✓", 588., y + 10., 11., INK_GREEN);
            } else {
                let w = text_right(&TOOL_PRICES[i].to_string(), 588., y + 31., 19., BROWN);
                coin(588. - w - 10., y + 25., 7.);
            }
        }
        let x = 44. + keycap("Space", "Buy tool", 44., 456., DARK) + 18.;
        keycap("X", "Compost the selected empty plot", x, 456., DARK);
        text(
            "Compost costs 2 coins and 1 energy, and restores soil for 2 harvests.",
            44.,
            506.,
            13.,
            SOFT,
        );
    }
    fn almanac(&self) {
        self.paper("Fourteen Mornings", "Plan around the weather");
        for (i, morning) in MORNINGS.iter().enumerate() {
            let x = 44. + (i % 7) as f32 * 80.;
            let y = 207. + (i / 7) as f32 * 75.;
            let day = i + 1;
            let today = day == usize::from(self.day);
            let past = day < usize::from(self.day);
            let tint = weather_color(morning.weather);
            if today {
                rounded(x, y + 2., 74., 66., 9., Color::new(0.2, 0.25, 0.1, 0.14));
                rounded(x, y, 74., 66., 9., ROW_ON);
                outline(x, y, 74., 66., 9., 2., GREEN);
            } else {
                rounded(
                    x,
                    y,
                    74.,
                    66.,
                    9.,
                    if past { darken(ROW, 0.04) } else { ROW },
                );
            }
            rounded(
                x + 8.,
                y + 58.,
                58.,
                3.,
                1.5,
                with_alpha(tint, if past { 0.35 } else { 0.9 }),
            );
            let ink = if past { with_alpha(SOFT, 0.7) } else { DARK };
            text(&format!("{day:02}"), x + 9., y + 23., 16., ink);
            weather_icon(morning.weather, x + 54., y + 19., 10.);
            text(
                if today {
                    "Today"
                } else {
                    morning.weather.name()
                },
                x + 9.,
                y + 47.,
                12.,
                if today {
                    INK_GREEN
                } else {
                    with_alpha(SOFT, if past { 0.7 } else { 1. })
                },
            );
        }
        eyebrow("A LETTER ON YOUR DOOR", 44., 378., SOFT);
        para(
            MORNINGS[usize::from(self.day - 1)].letter,
            44.,
            402.,
            556.,
            16.,
            3,
            DARK,
        );
        keycap("Space", "Go to sleep", 44., 482., DARK);
    }
    fn scene(&self) {
        rounded_gradient(
            16.,
            112.,
            928.,
            456.,
            12.,
            Color::new(0.17, 0.23, 0.16, 1.),
            DEEP,
        );
        self.tabs();
        match self.page {
            Page::Field => self.farm(),
            Page::Stall => self.stall(),
            Page::Orders => self.village(),
            Page::Tools => self.tools_page(),
            Page::Almanac => self.almanac(),
        }
        rounded(22., 534., 600., 30., 9., Color::new(0.06, 0.10, 0.07, 0.75));
        shapes::fill_circle(vec2(37., 549.), 4., GOLD);
        let s = flat(14., CREAM);
        let w = text_width(&self.notice, s);
        let s = if w > 562. {
            s.with_size(14. * 562. / w)
        } else {
            s
        };
        draw_text_styled(&self.notice, 50., s.centered_baseline(549.), s);
        self.ledger();
        if self.sleep_prompt {
            self.sleep_card();
        }
    }
    fn sleep_card(&self) {
        rounded(
            16.,
            112.,
            928.,
            456.,
            12.,
            Color::new(0.03, 0.05, 0.04, 0.78),
        );
        paper_card(100., 154., 760., 364.);
        rounded(128., 154., 72., 3., 1.5, GOLD);
        let festival = self.day == 14;
        eyebrow(
            if festival {
                "FESTIVAL NIGHT"
            } else {
                "NIGHTFALL"
            },
            128.,
            190.,
            SOFT,
        );
        fit(
            if festival {
                "Light the village oven?"
            } else {
                "Rest until morning?"
            },
            128.,
            226.,
            690.,
            30.,
            DARK,
        );
        let mut x = 128.;
        x += chip(
            &format!("Energy left  {}", self.energy),
            x,
            244.,
            13.,
            INK_GREEN,
        ) + 10.;
        let dry = self.dry();
        chip(
            &format!("Dry growing plots  {dry}"),
            x,
            244.,
            13.,
            if dry > 0 { RUST } else { SOFT },
        );
        let message = if festival {
            format!(
                "Orders delivered {}/3. Coins {} of the 80 the oven needs.",
                self.orders_completed(),
                self.coins
            )
        } else {
            format!(
                "Tonight: {}. Tomorrow: {}. {}",
                self.weather().name(),
                MORNINGS[usize::from(self.day)].weather.name(),
                self.weather().advice()
            )
        };
        let after = para(&message, 128., 300., 700., 18., 3, DARK);
        let warning = if let Some(i) = self.order()
            && self.day >= ORDERS[i].deadline
        {
            Some((
                format!(
                    "{} is still missing. Sleeping now ends the season.",
                    ORDERS[i].name
                ),
                RUST,
                RUST,
                0.12,
            ))
        } else if dry > 0 {
            Some((
                "Dry crops will not grow tonight; three stress makes them wilt.".to_string(),
                darken(GOLD, 0.2),
                BROWN,
                0.18,
            ))
        } else {
            None
        };
        if let Some((copy, edge, ink, fill)) = warning {
            let top = (after - 6.).min(392.);
            rounded(128., top, 704., 40., 9., with_alpha(edge, fill));
            rounded(128., top, 4., 40., 2., edge);
            fit(
                &copy,
                146.,
                style(16., ink).centered_baseline(top + 20.),
                670.,
                16.,
                ink,
            );
        }
        // Buttons share the model's click regions.
        rounded(
            128.,
            455.,
            302.,
            44.,
            11.,
            Color::new(0.1, 0.12, 0.05, 0.25),
        );
        rounded_gradient(
            128.,
            452.,
            302.,
            44.,
            11.,
            lighten(GREEN, 0.14),
            darken(GREEN, 0.06),
        );
        let k = keycap("Space", "", 146., 464., CREAM);
        let s = flat(17., CREAM);
        draw_text_styled(
            if festival {
                "Start the festival"
            } else {
                "Sleep"
            },
            146. + k + 12.,
            s.centered_baseline(474.),
            s,
        );
        rounded(450., 452., 300., 44., 11., darken(ROW, 0.06));
        outline(450., 452., 300., 44., 11., 1., darken(ROW, 0.2));
        let k = keycap("X", "", 468., 464., CREAM);
        let s = style(17., DARK);
        draw_text_styled(
            "Back to the farm",
            468. + k + 12.,
            s.centered_baseline(474.),
            s,
        );
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
        self.scene();
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!("Day {} of 14  ·  {}", self.day, self.weather().name()),
            objective: if let Some(i) = self.order() {
                let o = &ORDERS[i];
                format!(
                    "{}: {} turnip, {} bean, {} pumpkin by day {}",
                    o.name, o.needs[0], o.needs[1], o.needs[2], o.deadline
                )
            } else {
                "Finish day 14 with 80 coins.".into()
            },
            status: format!(
                "Energy {}/{} | Coins {} | Orders {}/3 | Seed: {}",
                self.energy,
                self.max_energy(),
                self.coins,
                self.orders_completed(),
                CROPS[self.crop].name
            ),
            score: self.score,
        }
    }
    fn outcome(&self) -> Outcome {
        Game::outcome(self)
    }
    fn summary(&self) -> String {
        if self.result == Outcome::Won {
            format!(
                "{}! All three tables are served. {} crops harvested, {} coins saved, {} tools earned. {}",
                self.rank,
                self.harvested,
                self.coins,
                self.tools.iter().filter(|&&v| v).count(),
                self.notice
            )
        } else {
            self.notice.clone()
        }
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.autoplay(tick)
    }
}
