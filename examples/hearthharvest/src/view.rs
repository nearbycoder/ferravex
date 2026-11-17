//! Farm art, the five management pages, effects and the `ArcadeGame` adapter.
//!
//! Everything here reads the model; nothing here changes it. Pointer hover
//! and the presentation clock only shape highlights, previews and idle motion.
use crate::{
    Game,
    content::{CROPS, MORNINGS, ORDERS, TOOL_NAMES, TOOL_PRICES, Weather},
    model::{
        ACT, Area, CANCEL, COLS, COMPOST, CONFIRM, Event, Night, PLOTS, Page, Refusal, SLEEP, Spot,
        Work, button_area, inside, plot_area, plot_at, plot_center, row_area, seed_area, tab_area,
    },
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
/// Green for text and marks on paper (dark enough to need no shadow).
const INK_GREEN: Color = Color::new(0.22, 0.40, 0.16, 1.);
const LEAF: Color = Color::new(0.51, 0.65, 0.32, 1.);
const GOLD: Color = Color::new(0.94, 0.72, 0.36, 1.);
const BROWN: Color = Color::new(0.39, 0.25, 0.16, 1.);
const SOFT: Color = Color::new(0.47, 0.40, 0.29, 1.);
const FAINT: Color = Color::new(0.57, 0.59, 0.43, 1.);
const WATER: Color = Color::new(0.40, 0.70, 0.80, 1.);
const RUST: Color = Color::new(0.72, 0.30, 0.20, 1.);
const SHADE: Color = Color::new(0.05, 0.04, 0.02, 0.22);
/// Coin gold on paper.
const COIN_INK: Color = Color::new(0.80, 0.54, 0.14, 1.);
/// Energy: a bolt on dark surfaces and on paper.
const ENERGY: Color = Color::new(0.60, 0.84, 0.36, 1.);
const ENERGY_INK: Color = Color::new(0.33, 0.58, 0.20, 1.);
/// Nights of growth: moons on dark surfaces and on paper.
const NIGHT: Color = Color::new(0.74, 0.78, 1.0, 1.);
const NIGHT_INK: Color = Color::new(0.38, 0.42, 0.68, 1.);
const WATER_INK: Color = Color::new(0.20, 0.50, 0.66, 1.);
const RED: Color = Color::new(0.90, 0.36, 0.28, 1.);
const FLAME: Color = Color::new(0.96, 0.54, 0.22, 1.);
const ICE: Color = Color::new(0.64, 0.84, 0.97, 1.);
const WOOD: Color = Color::new(0.60, 0.41, 0.24, 1.);
const BAG: Color = Color::new(0.62, 0.48, 0.28, 1.);
/// Field glyphs on soil.
const ON_SOIL: Color = Color::new(1., 0.95, 0.85, 1.);
const TABS: [(Icon, &str); 5] = [
    (Icon::Sprout, "Field"),
    (Icon::Crate, "Stall"),
    (Icon::People, "Village"),
    (Icon::Hammer, "Tools"),
    (Icon::Calendar, "Almanac"),
];

/// A short ASCII label built on the stack so per-frame numbers never allocate.
#[derive(Clone, Copy)]
struct Label {
    buf: [u8; 24],
    len: usize,
}
impl Label {
    fn new() -> Self {
        Self {
            buf: [0; 24],
            len: 0,
        }
    }
    fn put(mut self, text: &str) -> Self {
        for &b in text.as_bytes() {
            if self.len < self.buf.len() {
                self.buf[self.len] = b;
                self.len += 1;
            }
        }
        self
    }
    fn num(mut self, mut value: u32) -> Self {
        let mut digits = [0u8; 10];
        let mut n = 0;
        loop {
            digits[n] = b'0' + (value % 10) as u8;
            n += 1;
            value /= 10;
            if value == 0 {
                break;
            }
        }
        for &d in digits[..n].iter().rev() {
            if self.len < self.buf.len() {
                self.buf[self.len] = d;
                self.len += 1;
            }
        }
        self
    }
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}
fn num(value: u32) -> Label {
    Label::new().num(value)
}
/// "+12" or "-4".
fn signed(sign: &str, value: u32) -> Label {
    Label::new().put(sign).num(value)
}
fn day_label(day: u8) -> Label {
    Label::new().put("Day ").num(u32::from(day))
}

/// Shadowless face for text on flat surfaces.
fn flat(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
/// Left-aligned label vertically centred on `cy`; returns its width.
fn label(value: &str, x: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = style(size, color);
    draw_text_styled(value, x, s.centered_baseline(cy), s).width
}
/// Label centred on (`cx`, `cy`).
fn label_mid(value: &str, cx: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = style(size, color).with_align(TextAlign::Center);
    draw_text_styled(value, cx, s.centered_baseline(cy), s).width
}
fn label_width(value: &str, size: f32) -> f32 {
    text_width(value, style(size, PAPER))
}
/// Icon with a soft shadow, for dark surfaces.
fn glyph_lit(icon: Icon, cx: f32, cy: f32, size: f32, color: Color) {
    icons::draw_styled(
        icon,
        vec2(cx, cy),
        size,
        IconStyle::new(color).shadow(Color::new(0., 0., 0., 0.4 * color.a)),
    );
}
fn faded(c: Color, alpha: f32) -> Color {
    with_alpha(c, c.a * alpha)
}
/// `total` small dots from `x`, centred on `cy`; the first `filled` are solid.
/// Returns the drawn width.
fn dots(
    x: f32,
    cy: f32,
    (filled, total): (u32, u32),
    (r, gap): (f32, f32),
    (on, off): (Color, Color),
) -> f32 {
    for k in 0..total {
        let c = vec2(x + r + k as f32 * gap, cy);
        if k < filled {
            shapes::fill_circle(c, r, on);
        } else {
            shapes::stroke_circle(c, (r - 0.6).max(0.8), 1.2, off);
        }
    }
    if total == 0 {
        0.
    } else {
        r * 2. + (total - 1) as f32 * gap
    }
}
/// Small key cap badge centred on (`cx`, `cy`), shown next to a control.
fn key_badge(key: &str, cx: f32, cy: f32) {
    let s = flat(11., PAPER);
    let w = (text_width(key, s) + 8.).max(15.);
    let r = Rect::new(cx - w * 0.5, cy - 8., w, 16.);
    shapes::fill_rounded_rect(
        Rect::new(r.x, r.y + 1.5, r.w, r.h),
        3.5,
        Color::new(0., 0., 0., 0.45),
    );
    shapes::fill_rounded_rect(
        r,
        3.5,
        Fill::vertical(
            r,
            Color::new(0.21, 0.25, 0.31, 1.),
            Color::new(0.13, 0.16, 0.21, 1.),
        ),
    );
    shapes::stroke_rounded_rect(r, 3.5, 1., Color::new(1., 1., 1., 0.12));
    draw_text_styled(
        key,
        cx,
        s.centered_baseline(cy),
        s.with_align(TextAlign::Center),
    );
}
/// A soft contact shadow under a rounded object.
fn contact(x: f32, y: f32, w: f32, h: f32, r: f32) {
    rounded(x + 1., y + 3., w, h, r, SHADE);
}
/// Raised paper surface: a tight contact shadow, a lit gradient and a top rim.
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
/// A raised button; lifts and gains a rim when hovered. Returns the lift.
fn button(a: Area, fill: Color, hovered: bool, enabled: bool) -> f32 {
    let lift = if hovered && enabled { 2. } else { 0. };
    let [x, y, w, h] = a;
    rounded(x, y + 3., w, h, 10., Color::new(0.08, 0.06, 0.02, 0.22));
    let f = if enabled {
        fill
    } else {
        shapes::mix(fill, ROW, 0.65)
    };
    let top = y - lift;
    rounded_gradient(
        x,
        top,
        w,
        h,
        10.,
        lighten(f, if hovered { 0.2 } else { 0.12 }),
        darken(f, 0.05),
    );
    panel(x + 10., top + 1., w - 20., 1., Color::new(1., 1., 1., 0.25));
    if hovered {
        outline(x, top, w, h, 10., 1.5, with_alpha(GOLD, 0.95));
    }
    lift
}
/// Ellipse in one batched draw; `rotation` is in radians.
fn oval(x: f32, y: f32, rx: f32, ry: f32, rotation: f32, color: Color) {
    draw_ellipse(x, y, rx, ry, rotation.to_degrees(), color);
}
/// Small filled circle in one batched draw, tessellated by size.
fn dot(x: f32, y: f32, r: f32, color: Color) {
    draw_poly(x, y, (r * 0.9).clamp(8., 24.) as u8, r, 0., color);
}
fn fract(v: f32) -> f32 {
    v - v.floor()
}
fn weather_icon(weather: Weather) -> Icon {
    match weather {
        Weather::Sun => Icon::Sun,
        Weather::Rain => Icon::Rain,
        Weather::Heat => Icon::Thermometer,
        Weather::Frost => Icon::Snowflake,
    }
}
fn weather_color(weather: Weather) -> Color {
    match weather {
        Weather::Sun => GOLD,
        Weather::Heat => Color::new(0.92, 0.42, 0.22, 1.),
        Weather::Rain => Color::new(0.38, 0.62, 0.82, 1.),
        Weather::Frost => Color::new(0.52, 0.76, 0.95, 1.),
    }
}
/// Weather glyph readable on paper.
fn weather_ink(weather: Weather) -> Color {
    darken(weather_color(weather), 0.18)
}
fn crop_color(crop: usize) -> Color {
    match crop {
        0 => Color::new(0.80, 0.50, 0.68, 1.),
        1 => Color::new(0.50, 0.72, 0.30, 1.),
        _ => Color::new(0.93, 0.53, 0.20, 1.),
    }
}
/// Order colors follow their signature crop: soup turnips, founders' beans,
/// lantern pumpkins.
fn order_color(order: usize) -> Color {
    crop_color(order.min(2))
}
/// Village order whose window holds `day`.
fn order_of_day(day: u8) -> usize {
    ORDERS
        .iter()
        .position(|o| day <= o.deadline)
        .unwrap_or(ORDERS.len() - 1)
}
/// Icon and cost for the field work Space would do.
fn work_mark(work: Work) -> (Icon, &'static str, u32) {
    match work {
        Work::Plant => (Icon::Sprout, "Plant", 2),
        Work::Water => (Icon::Drop, "Water", 1),
        Work::Harvest => (Icon::Hand, "Harvest", 1),
        Work::Clear => (Icon::Cross, "Clear", 1),
    }
}
/// The missing thing's icon and at most two words.
fn refusal_mark(why: Refusal) -> (Icon, &'static str) {
    match why {
        Refusal::NoSeed => (Icon::Bag, "Empty"),
        Refusal::Tired => (Icon::Bolt, "Tired"),
        Refusal::ShelfFull | Refusal::TinFull => (Icon::Bag, "Full"),
        Refusal::Watered => (Icon::Drop, "Watered"),
        Refusal::Occupied => (Icon::Sprout, "Planted"),
        Refusal::Rich => (Icon::Leaf, "Rich"),
        Refusal::Broke => (Icon::Coin, "Short"),
        Refusal::SoldOut => (Icon::Crate, "Sold out"),
        Refusal::NoProduce => (Icon::Cross, "None"),
        Refusal::Done => (Icon::Check, "Done"),
        Refusal::Locked => (Icon::Lock, "Locked"),
        Refusal::Short => (Icon::Cross, "Missing"),
        Refusal::Owned => (Icon::Check, "Owned"),
    }
}
/// First sentence of an authored letter: its essential line.
fn first_sentence(text: &str) -> &str {
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if matches!(b, b'.' | b'!' | b'?') && bytes.get(i + 1).is_none_or(|&n| n == b' ') {
            return &text[..=i];
        }
    }
    text
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
/// A crop at `age` (0 seedling .. 1 ripe); `alpha` ghosts it for previews.
fn crop_icon(crop: usize, x: f32, y: f32, scale: f32, age: f32, wilted: bool, alpha: f32) {
    let stem = faded(
        if wilted {
            Color::new(0.55, 0.42, 0.22, 1.)
        } else {
            LEAF
        },
        alpha,
    );
    let height = (8. + age.min(1.) * 14.) * scale;
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
        dot(x, y + 3. * scale, 2.5 * scale, faded(GOLD, alpha));
        return;
    }
    match crop {
        0 => {
            let bulb = faded(Color::new(0.95, 0.90, 0.74, 1.), alpha);
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
                faded(Color::new(0.72, 0.42, 0.55, 1.), alpha),
            );
            kite(
                x - 3. * scale,
                y - 6. * scale,
                3. * scale,
                1.2 * scale,
                -0.3,
                Color::new(1., 1., 1., 0.35 * alpha),
            );
        }
        1 => {
            draw_line(
                x + 9. * scale,
                y + 10. * scale,
                x + 9. * scale,
                y - 28. * scale,
                2. * scale,
                faded(BROWN, alpha),
            );
            for i in 0..3 {
                let pod = faded(Color::new(0.58 + i as f32 * 0.04, 0.72, 0.34, 1.), alpha);
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
            let orange = faded(Color::new(0.88, 0.45, 0.19, 1.), alpha);
            oval(x, y, 15. * scale, 12. * scale, 0., darken(orange, 0.12));
            oval(x, y, 8. * scale, 12. * scale, 0., lighten(orange, 0.08));
            kite(
                x - 7. * scale,
                y - 5. * scale,
                3. * scale,
                1.5 * scale,
                -0.6,
                Color::new(1., 1., 1., 0.3 * alpha),
            );
            draw_line(
                x,
                y - 11. * scale,
                x + 3. * scale,
                y - 17. * scale,
                4. * scale,
                faded(BROWN, alpha),
            );
        }
    }
}
/// A ripe crop as a small badge-sized glyph centred on (`cx`, `cy`).
fn crop_glyph(crop: usize, cx: f32, cy: f32, size: f32) {
    let scale = size / 30.;
    crop_icon(crop, cx, cy + 4. * scale, scale, 1., false, 1.);
}
fn tool_icon(tool: usize, x: f32, y: f32, owned: bool) {
    let ink = if owned { GREEN } else { BROWN };
    match tool {
        0 => {
            let brass = Color::new(0.86, 0.66, 0.32, 1.);
            shapes::stroke_circle(vec2(x - 9., y - 10.), 9., 3., ink);
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
/// A village lantern; lit lanterns glow and flicker.
fn lantern(x: f32, y: f32, lit: bool, t: f32) {
    let body = if lit {
        GOLD
    } else {
        Color::new(0.62, 0.62, 0.52, 1.)
    };
    if lit {
        let flicker = 0.8 + 0.2 * (t * 9.3 + x * 0.1).sin() * (t * 5.1).cos();
        glow(x, y, 22., with_alpha(GOLD, 0.32 * flicker));
    }
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
/// The almanac card for day index `i` (0 = day one).
fn day_area(i: u8) -> Area {
    [
        40. + f32::from(i % 7) * 80.,
        192. + f32::from(i / 7) * 128.,
        74.,
        116.,
    ]
}
/// A paper note with an icon, wrapped to at most four lines, kept on screen.
fn note(text: &str, anchor: Vec2) {
    let s = flat(13., DARK);
    let width = 300.;
    let mut lines = [""; 4];
    let mut count = 0;
    for line in wrap_text(text, width - 44., s).take(4) {
        lines[count] = line;
        count += 1;
    }
    let h = 18. + count as f32 * 17.;
    let x = (anchor.x - width * 0.5).clamp(26., 934. - width);
    let y = if anchor.y + 18. + h < 560. {
        anchor.y + 18.
    } else {
        anchor.y - 18. - h
    };
    rounded(x, y + 4., width, h, 10., Color::new(0., 0., 0., 0.3));
    rounded_gradient(x, y, width, h, 10., lighten(PAPER_BG, 0.05), PAPER_BG);
    outline(x, y, width, h, 10., 1., with_alpha(BROWN, 0.35));
    icon(Icon::Scroll, x + 18., y + 18., 16., SOFT);
    for (i, line) in lines[..count].iter().enumerate() {
        draw_text_styled(line, x + 34., y + 22. + i as f32 * 17., s);
    }
}

impl Game {
    /// The plot under the pointer (or the selected one), for previews.
    fn focus_plot(&self) -> usize {
        plot_at(pointer())
            .filter(|_| self.page == Page::Field && !self.sleep_prompt)
            .unwrap_or(self.cursor)
    }
    /// Energy the page's Space action would spend, for the meter preview.
    fn action_cost(&self) -> u32 {
        if self.page != Page::Field {
            return 0;
        }
        self.field_check(self.cursor)
            .map_or(0, |work| work_mark(work).2)
    }
    /// True when no plot accepts work: the day's chores are done.
    fn chores_done(&self) -> bool {
        (0..PLOTS).all(|i| self.field_check(i).is_err())
    }
    fn tabs(&self) {
        rounded(22., 116., 600., 34., 10., with_alpha(DEEP, 0.85));
        let p = pointer();
        let t = clock();
        for (i, (glyph, name)) in TABS.iter().enumerate() {
            let a = tab_area(i);
            let x = a[0];
            let on = self.page.index() == i;
            let hot = !on && !self.sleep_prompt && inside(a, p);
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
            } else if hot {
                rounded(x + 2., 118., 116., 30., 8., Color::new(1., 1., 1., 0.08));
            }
            let ink = if on {
                CREAM
            } else if hot {
                lighten(FAINT, 0.35)
            } else {
                lighten(FAINT, 0.12)
            };
            let n = flat(10., faded(ink, 0.7)).with_align(TextAlign::Center);
            draw_text_styled(
                num(i as u32 + 1).as_str(),
                x + 13.,
                n.centered_baseline(133.),
                n,
            );
            icon(*glyph, x + 31., 133., 16., ink);
            let s = if on { style(14., ink) } else { flat(14., ink) };
            draw_text_styled(name, x + 44., s.centered_baseline(133.), s);
            // Attention badges: thirsty crops, a deliverable order.
            let badge = vec2(x + 110., 122.);
            match i {
                0 if !on && self.dry() > 0 => {
                    shapes::fill_circle(badge, 8., WATER_INK);
                    label_mid(
                        num(self.dry() as u32).as_str(),
                        badge.x,
                        badge.y,
                        10.,
                        PAPER,
                    );
                }
                0 if !on && self.ripe() > 0 => {
                    shapes::fill_circle(badge, 8., darken(GOLD, 0.2));
                    label_mid(
                        num(self.ripe() as u32).as_str(),
                        badge.x,
                        badge.y,
                        10.,
                        PAPER,
                    );
                }
                2 if !on && self.deliver_check().is_ok() => {
                    glow(badge.x, badge.y, 16., with_alpha(GOLD, 0.3 * pulse(t, 1.4)));
                    shapes::fill_circle(badge, 8., darken(GOLD, 0.1));
                    icon(Icon::Check, badge.x, badge.y, 10., DEEP);
                }
                _ => {}
            }
        }
    }
    fn sky(&self, weather: Weather, t: f32) {
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
        rounded_gradient(22., 156., 600., 408., 12., sky_top, sky_low);
        // The sun (or its absence) sits over the hills.
        let (sx, sy) = (566., 184.);
        match weather {
            Weather::Sun | Weather::Heat => {
                let c = weather_color(weather);
                let heat = weather == Weather::Heat;
                glow(
                    sx,
                    sy,
                    if heat { 46. } else { 34. },
                    with_alpha(c, 0.35 + 0.1 * pulse(t, 0.5)),
                );
                let spin = t * 0.25;
                for k in 0..10 {
                    let a = spin + k as f32 * std::f32::consts::TAU / 10.;
                    let d = vec2(a.cos(), a.sin());
                    let reach = 22. + 3. * (t * 2. + k as f32).sin();
                    draw_line(
                        sx + d.x * 15.,
                        sy + d.y * 15.,
                        sx + d.x * reach,
                        sy + d.y * reach,
                        2.5,
                        with_alpha(c, 0.85),
                    );
                }
                shapes::fill_circle(
                    vec2(sx, sy),
                    12.,
                    Fill::radial(vec2(sx - 3., sy - 3.), 14., lighten(c, 0.45), c),
                );
            }
            Weather::Rain | Weather::Frost => {}
        }
        // Drifting clouds, darker under rain.
        let cloud = match weather {
            Weather::Rain => Color::new(0.58, 0.63, 0.66, 0.95),
            Weather::Frost => Color::new(0.95, 0.97, 1., 0.85),
            _ => Color::new(1., 1., 1., 0.7),
        };
        let clouds = if weather == Weather::Rain { 4 } else { 2 };
        for k in 0..clouds {
            let span = 640.;
            let x = 10. + fract(t * 0.008 * (1. + k as f32 * 0.35) + k as f32 * 0.29) * span;
            let y = 168. + (k % 2) as f32 * 14.;
            // Fade at the frame edges instead of clipping.
            let edge = ((x - 40.) / 40.).min((600. - x) / 40.).clamp(0., 1.);
            if edge <= 0. {
                continue;
            }
            let c = faded(cloud, edge);
            dot(x, y, 9., c);
            dot(x + 12., y - 5., 11., c);
            dot(x + 25., y, 9., c);
            rounded(x - 8., y, 42., 9., 4.5, c);
        }
    }
    fn weather_overlay(&self, weather: Weather, t: f32) {
        match weather {
            Weather::Rain => {
                for k in 0..34 {
                    let fk = k as f32;
                    let x = 30. + fract(fk * 0.618 + t * 0.05) * 584.;
                    let y = 160. + fract(fk * 0.377 + t * 0.9) * 390.;
                    draw_line(
                        x,
                        y,
                        x - 3.,
                        y + 11.,
                        1.5,
                        Color::new(0.82, 0.9, 0.98, 0.42),
                    );
                }
            }
            Weather::Frost => {
                for k in 0..26 {
                    let fk = k as f32;
                    let x = 30. + fract(fk * 0.618) * 584. + (t * 0.8 + fk).sin() * 8.;
                    let y = 160. + fract(fk * 0.377 + t * 0.07) * 390.;
                    dot(
                        x,
                        y,
                        1.8 + (k % 3) as f32 * 0.5,
                        Color::new(1., 1., 1., 0.75),
                    );
                }
            }
            Weather::Heat => {
                for k in 0..3 {
                    let y = 238. + k as f32 * 6.;
                    let mut prev = vec2(30., y);
                    for s in 1..=24 {
                        let x = 30. + s as f32 * 24.;
                        let p = vec2(x, y + (x * 0.05 + t * 3. + k as f32).sin() * 2.);
                        draw_line(
                            prev.x,
                            prev.y,
                            p.x,
                            p.y,
                            1.2,
                            Color::new(1., 0.9, 0.7, 0.18),
                        );
                        prev = p;
                    }
                }
            }
            Weather::Sun => {}
        }
    }
    fn farm(&self) {
        let weather = self.weather();
        let t = clock();
        self.sky(weather, t);
        for i in 0..4 {
            let cx = 96. + i as f32 * 150.;
            draw_circle(cx, 268., 74., Color::new(0.55, 0.66, 0.43, 1.));
        }
        for i in 0..3 {
            let cx = 170. + i as f32 * 150.;
            draw_circle(cx, 262., 52., Color::new(0.46, 0.59, 0.35, 1.));
        }
        let ground = Rect::new(22., 226., 600., 326.);
        shapes::fill_rounded_rect(
            ground,
            0.,
            Fill::vertical(
                ground,
                Color::new(0.70, 0.66, 0.42, 1.),
                Color::new(0.60, 0.55, 0.34, 1.),
            ),
        );
        rounded(22., 506., 600., 58., 12., Color::new(0.60, 0.55, 0.34, 1.));
        panel(22., 226., 600., 2., Color::new(1., 1., 1., 0.12));
        // Farmhouse with a smoking chimney, glowing windows and a water barrel.
        contact(62., 222., 104., 6., 3.);
        for k in 0..4 {
            let phase = fract(t * 0.32 + k as f32 * 0.25);
            let x = 142. + phase * 46.;
            let y = 162. - phase * 5. + (t * 1.7 + k as f32).sin() * 1.5;
            dot(
                x,
                y,
                3. + phase * 5.,
                Color::new(0.94, 0.94, 0.92, 0.42 * (1. - phase)),
            );
        }
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
        let warm = 0.85 + 0.15 * (t * 3.1).sin() * (t * 1.7).cos();
        for wx in [74., 132.] {
            rounded(
                wx - 1.,
                191.,
                20.,
                17.,
                3.,
                Color::new(0.36, 0.24, 0.20, 1.),
            );
            rounded_gradient(
                wx + 1.,
                193.,
                16.,
                13.,
                2.,
                lighten(GOLD, 0.3 * warm),
                faded(GOLD, warm),
            );
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
        // Clothesline of order lanterns: one lights per delivered table.
        draw_line(370., 190., 505., 194., 1.5, BROWN);
        draw_line(370., 186., 370., 226., 3., BROWN);
        draw_line(505., 190., 505., 226., 3., BROWN);
        for i in 0..3 {
            let sway = (t * 1.3 + i as f32).sin() * 1.2;
            lantern(399. + i as f32 * 37. + sway, 207., self.delivered[i], t);
        }
        // Picket fence.
        let fence = Color::new(0.96, 0.92, 0.80, 1.);
        draw_line(30., 222., 614., 222., 2., darken(fence, 0.1));
        draw_line(30., 230., 614., 230., 2., darken(fence, 0.1));
        for i in 0..22 {
            let x = 31. + i as f32 * 27.5;
            panel(x, 216., 4., 20., fence);
            draw_triangle(vec2(x, 216.), vec2(x + 4., 216.), vec2(x + 2., 213.), fence);
        }
        let focus = self.focus_plot();
        let hovered = plot_at(pointer()).filter(|_| !self.sleep_prompt);
        // Brass-can reach: the neighbors one watering would also soak.
        let reach = if self.tools[0] && self.field_check(focus) == Ok(Work::Water) {
            self.reach(focus)
        } else {
            [None; 3]
        };
        for i in 0..PLOTS {
            self.plot(i, i == self.cursor, hovered == Some(i), t);
        }
        for i in reach.into_iter().flatten().skip(1) {
            if self.plots[i]
                .plant
                .is_some_and(|p| !p.watered && !p.ready() && !p.wilted())
            {
                let [x, y, w, h] = plot_area(i);
                outline(
                    x - 2.,
                    y - 2.,
                    w + 4.,
                    h + 4.,
                    10.,
                    2.,
                    with_alpha(WATER, 0.6 + 0.3 * pulse(t, 1.5)),
                );
                icon(
                    Icon::Drop,
                    x + w * 0.5,
                    y + h * 0.5 - 4.,
                    18.,
                    faded(WATER, 0.7),
                );
            }
        }
        // A small gardener stands beside the chosen plot, never over its crop.
        let [px, py, _, _] = plot_area(self.cursor);
        let x = px + 130.;
        let y = py + 40. + bob(t, 0.9, 0.8);
        oval(x, py + 53., 7., 2.5, 0., SHADE);
        rounded(x - 4., y + 6., 3., 7., 1.5, BROWN);
        rounded(x + 1., y + 6., 3., 7., 1.5, BROWN);
        rounded_gradient(x - 5., y - 2., 10., 11., 3., lighten(GREEN, 0.15), GREEN);
        shapes::fill_circle(vec2(x, y - 7.), 4.5, Color::new(0.94, 0.80, 0.64, 1.));
        oval(x, y - 10., 8., 2.5, 0., darken(GOLD, 0.1));
        oval(x, y - 12., 4.5, 3., 0., GOLD);
        self.weather_overlay(weather, t);
        if !self.sleep_prompt {
            self.work_pill(focus);
        }
        self.seed_packets(t);
    }
    fn plot(&self, i: usize, selected: bool, hovered: bool, t: f32) {
        let [x, y, w, h] = plot_area(i);
        let plot = self.plots[i];
        let watered = plot.plant.is_some_and(|p| p.watered);
        // Richer soil is darker; spent soil is pale and cracked.
        let base = match plot.soil {
            2 => Color::new(0.45, 0.30, 0.19, 1.),
            1 => Color::new(0.53, 0.38, 0.25, 1.),
            _ => Color::new(0.64, 0.52, 0.38, 1.),
        };
        let soil = if watered {
            shapes::mix(base, Color::new(0.17, 0.13, 0.11, 1.), 0.55)
        } else {
            base
        };
        let breathe = pulse(t, 0.7);
        if selected {
            glow(
                x + w * 0.5,
                y + h * 0.5,
                86.,
                with_alpha(GOLD, 0.12 + 0.08 * breathe),
            );
        }
        // Raised-bed lip, then the soil itself.
        panel(x + 4., y + 60., w - 8., 14., darken(soil, 0.45));
        rounded_gradient(x, y, w, h, 9., lighten(soil, 0.08), darken(soil, 0.08));
        panel(x + 9., y + 1., w - 18., 1., Color::new(1., 1., 1., 0.16));
        for line in 0..4 {
            let ly = y + 16. + line as f32 * 13.;
            panel(x + 10., ly, w - 20., 2., Color::new(0.12, 0.08, 0.04, 0.24));
        }
        if plot.soil == 0 {
            let crack = Color::new(0.35, 0.26, 0.18, 0.6);
            draw_line(x + 20., y + 10., x + 30., y + 22., 1.2, crack);
            draw_line(x + 30., y + 22., x + 26., y + 30., 1.2, crack);
            draw_line(x + 96., y + 40., x + 108., y + 48., 1.2, crack);
        }
        if watered {
            // A slow glint travels across wet soil.
            let s = fract(t * 0.22 + i as f32 * 0.37);
            panel(
                x + 12. + s * (w - 44.),
                y + 7.,
                20.,
                1.5,
                Color::new(0.8, 0.9, 1., 0.22),
            );
        }
        if hovered {
            outline(
                x - 3.,
                y - 3.,
                w + 6.,
                h + 6.,
                11.,
                2.,
                with_alpha(CREAM, 0.75),
            );
        }
        if selected {
            outline(
                x - 2.,
                y - 2.,
                w + 4.,
                h + 4.,
                10.,
                2.5,
                with_alpha(GOLD, 0.7 + 0.3 * breathe),
            );
        }
        match plot.plant {
            Some(plant) => {
                let progress = f32::from(plant.age) / f32::from(plant.nights);
                let ready = plant.ready();
                let wilted = plant.wilted();
                let lift = if ready {
                    2. + (t * 2.4 + i as f32).sin() * 1.6
                } else {
                    0.
                };
                if ready {
                    glow(
                        x + 62.,
                        y + 34.,
                        34.,
                        with_alpha(GOLD, 0.2 + 0.14 * pulse(t, 0.9)),
                    );
                    oval(
                        x + 62.,
                        y + 53.,
                        16. - lift,
                        4.,
                        0.,
                        Color::new(0.1, 0.06, 0.03, 0.3),
                    );
                }
                crop_icon(
                    plant.crop,
                    x + 62.,
                    y + 38. - lift,
                    1.15,
                    progress,
                    wilted,
                    1.,
                );
                if wilted {
                    icon_badge(Icon::Cross, x + w - 13., y + 13., 18., RED, PAPER);
                } else if ready {
                    let pop = 1. + 0.08 * pulse(t, 1.2);
                    icon_badge(
                        Icon::Check,
                        x + w - 13.,
                        y + 13.,
                        18. * pop,
                        darken(LEAF, 0.15),
                        PAPER,
                    );
                } else {
                    // Water state: a full drop, or an empty one that asks.
                    if plant.watered {
                        glyph_lit(Icon::Drop, x + 14., y + 14., 15., WATER);
                    } else {
                        // Thirsty: a pale drop inside a breathing ring asks for water.
                        let ask = pulse(t, 1.1);
                        let c = if plant.stress > 0 {
                            RED
                        } else {
                            lighten(WATER, 0.45)
                        };
                        shapes::stroke_circle(
                            vec2(x + 14., y + 14.),
                            9. + 2. * ask,
                            1.5,
                            faded(c, 0.25 + 0.45 * ask),
                        );
                        glyph_lit(Icon::Drop, x + 14., y + 14., 14., faded(c, 0.6 + 0.4 * ask));
                        if plant.stress > 0 {
                            dots(
                                x + 7.,
                                y + 29.,
                                (u32::from(plant.stress), 3),
                                (2., 5.),
                                (RED, faded(ON_SOIL, 0.45)),
                            );
                        }
                    }
                    // One moon per growing night; lit moons have passed.
                    for k in 0..plant.nights {
                        let lit = k < plant.age;
                        icon(
                            Icon::Moon,
                            x + 13. + f32::from(k) * 12.,
                            y + 61.,
                            10.,
                            if lit { NIGHT } else { faded(ON_SOIL, 0.3) },
                        );
                    }
                }
            }
            None => {
                // Ghost the selected seed on the focused empty plot.
                if (selected || hovered)
                    && !self.sleep_prompt
                    && self.field_check(i) == Ok(Work::Plant)
                {
                    let ghost = 0.35 + 0.15 * breathe;
                    crop_icon(self.crop, x + 62., y + 40., 1.15, 0., false, ghost);
                    let nights = CROPS[self.crop].nights + u8::from(plot.soil == 0);
                    for k in 0..nights {
                        icon(
                            Icon::Moon,
                            x + 13. + f32::from(k) * 12.,
                            y + 61.,
                            10.,
                            faded(ON_SOIL, 0.4),
                        );
                    }
                }
            }
        }
        // Soil richness: two dots at the bottom right.
        dots(
            x + w - 22.,
            y + 61.,
            (u32::from(plot.soil), 2),
            (2.6, 8.),
            (GOLD, faded(ON_SOIL, 0.35)),
        );
    }
    /// Above the focused plot: what Space (or a second click) would do.
    fn work_pill(&self, i: usize) {
        let [x, y, w, _] = plot_area(i);
        let cx = x + w * 0.5;
        let cy = y - 2.;
        let (glyph, ok, cost) = match self.field_check(i) {
            Ok(work) => {
                let (g, _, cost) = work_mark(work);
                (g, true, cost)
            }
            Err(Refusal::Watered) => (Icon::Moon, true, 0),
            Err(why) => (refusal_mark(why).0, false, 0),
        };
        let cost_label = signed("-", cost);
        let cost_w = if cost > 0 {
            16. + label_width(cost_label.as_str(), 13.)
        } else {
            0.
        };
        let pw = 30. + cost_w;
        let left = cx - pw * 0.5;
        rounded(
            left,
            cy - 10.,
            pw,
            21.,
            10.5,
            Color::new(0.07, 0.09, 0.07, 0.86),
        );
        outline(
            left,
            cy - 10.,
            pw,
            21.,
            10.5,
            1.,
            if ok {
                with_alpha(GOLD, 0.7)
            } else {
                with_alpha(RED, 0.9)
            },
        );
        icon(glyph, left + 15., cy, 14., if ok { CREAM } else { RED });
        if !ok {
            draw_line(left + 7., cy + 7., left + 23., cy - 7., 2., RED);
        }
        if cost > 0 {
            icon(Icon::Bolt, left + 32., cy, 12., ENERGY);
            label(cost_label.as_str(), left + 39., cy, 13., ENERGY);
        }
    }
    fn seed_packets(&self, t: f32) {
        let p = pointer();
        for (i, crop) in CROPS.iter().enumerate() {
            let [x, y, w, h] = seed_area(i);
            let on = self.crop == i;
            let hot = !self.sleep_prompt && inside(seed_area(i), p);
            let have = self.seeds[i];
            let lift = if on {
                3.
            } else if hot {
                1.5
            } else {
                0.
            };
            let top = y - lift;
            rounded(x, y + 3., w, h, 10., Color::new(0.1, 0.08, 0.02, 0.28));
            if on {
                glow(x + w * 0.5, top + h * 0.5, 70., with_alpha(GOLD, 0.16));
                rounded_gradient(x, top, w, h, 10., lighten(GREEN, 0.16), darken(GREEN, 0.04));
                outline(x, top, w, h, 10., 2., GOLD);
            } else {
                rounded(
                    x,
                    top,
                    w,
                    h,
                    10.,
                    Color::new(0.18, 0.24, 0.17, if hot { 0.86 } else { 0.72 }),
                );
                if hot {
                    outline(x, top, w, h, 10., 1.5, with_alpha(CREAM, 0.6));
                }
            }
            let cy = top + h * 0.5;
            let wiggle = if on { (t * 2.).sin() * 0.06 } else { 0. };
            crop_icon(i, x + 24., cy + 4., 0.7 + wiggle * 0.2, 1., false, 1.);
            // Nights to grow, then the seed tin.
            for k in 0..crop.nights {
                icon(
                    Icon::Moon,
                    x + 50. + f32::from(k) * 11.,
                    cy,
                    10.,
                    if on { NIGHT } else { faded(NIGHT, 0.55) },
                );
            }
            let ink = if have == 0 {
                RED
            } else if on {
                CREAM
            } else {
                lighten(FAINT, 0.25)
            };
            let count = num(have);
            let cw = label_width(count.as_str(), 16.);
            label(count.as_str(), x + w - 12. - cw, cy, 16., ink);
            icon(Icon::Bag, x + w - 24. - cw, cy, 15., faded(ink, 0.9));
        }
        // X cycles the packets.
        let [x, y, w, h] = seed_area(2);
        key_badge("X", x + w + 18., y + h * 0.5);
    }
    /// The ledger: coins, energy, basket, weather and the two actions.
    fn ledger(&self) {
        let (l, r) = (660., 918.);
        let t = clock();
        paper_card(640., 118., 298., 446.);
        rounded(660., 118., 64., 3., 1.5, GOLD);
        // Coins, and the oven's 80-coin ring.
        let cy = 152.;
        icons::draw_styled(
            Icon::Coin,
            vec2(l + 14., cy),
            26.,
            IconStyle::new(COIN_INK).shadow(Color::new(0.3, 0.2, 0., 0.25)),
        );
        label(num(self.coins).as_str(), l + 34., cy + 1., 30., BROWN);
        let oven = (self.coins as f32 / 80.).min(1.);
        let lit = self.coins >= 80;
        let (ox, oy) = (r - 18., cy);
        if lit {
            glow(ox, oy, 30., with_alpha(FLAME, 0.25 + 0.12 * pulse(t, 0.8)));
        }
        ring_meter(ox, oy, 16., 4., oven, if lit { FLAME } else { COIN_INK });
        icon(
            Icon::Flame,
            ox,
            oy,
            16.,
            if lit { FLAME } else { faded(SOFT, 0.6) },
        );
        let w = label_width("80", 13.);
        label("80", ox - 24. - w, oy, 13., SOFT);
        icon(Icon::Coin, ox - 31. - w, oy, 11., faded(COIN_INK, 0.8));
        // Energy: a bolt, the number and one segment per point.
        let cy = 196.;
        icon(Icon::Bolt, l + 14., cy, 22., ENERGY_INK);
        label(num(u32::from(self.energy)).as_str(), l + 34., cy, 22., DARK);
        let max = u32::from(self.max_energy());
        let have = u32::from(self.energy);
        let cost = self.action_cost().min(have);
        let (bx, bw) = (l + 70., r - l - 70.);
        let gap = 2.;
        let seg = (bw - gap * (max - 1) as f32) / max as f32;
        let blink = 0.35 + 0.65 * pulse(t, 1.6);
        for k in 0..max {
            let sx = bx + k as f32 * (seg + gap);
            let c = if k >= have {
                darken(PAPER_BG, 0.12)
            } else if k >= have - cost {
                faded(ENERGY_INK, blink)
            } else {
                ENERGY_INK
            };
            rounded(sx, cy - 6., seg, 12., 2.5, c);
        }
        divider(l, 220., r - l, darken(PAPER_BG, 0.22));
        // Basket: each crop's harvest, with pips for the current order.
        let order = self.order();
        for c in 0..3 {
            let cx = l + 44. + c as f32 * 86.;
            let have = self.produce(c);
            crop_glyph(c, cx - 18., 246., 26.);
            label(num(have).as_str(), cx + 2., 248., 22., BROWN);
            if let Some(o) = order {
                let need = ORDERS[o].needs[c];
                if need > 0 {
                    let met = have >= need;
                    let w = 7. + (need - 1) as f32 * 9.;
                    dots(
                        cx - w * 0.5 - 4.,
                        274.,
                        (have.min(need), need),
                        (3.5, 9.),
                        (
                            if met { INK_GREEN } else { order_color(o) },
                            faded(SOFT, 0.5),
                        ),
                    );
                }
            }
        }
        divider(l, 292., r - l, darken(PAPER_BG, 0.22));
        // Today's weather, then tomorrow's.
        let today = self.weather();
        let tile = Rect::new(l, 306., 120., 76.);
        shapes::fill_rounded_rect(tile, 10., with_alpha(weather_color(today), 0.2));
        shapes::stroke_rounded_rect(tile, 10., 1., with_alpha(weather_ink(today), 0.35));
        let quiet = today == Weather::Sun;
        glyph_lit(
            weather_icon(today),
            if quiet { l + 60. } else { l + 36. },
            344.,
            40.,
            weather_ink(today),
        );
        self.weather_effect(today, l + 88., 344., true);
        icon(Icon::ArrowRight, l + 138., 344., 16., faded(SOFT, 0.6));
        match self.tomorrow() {
            Some(next) => {
                icon(
                    weather_icon(next),
                    l + 178.,
                    344.,
                    30.,
                    faded(weather_ink(next), 0.85),
                );
                self.weather_effect(next, l + 220., 344., false);
            }
            None => lantern(l + 178., 346., true, t),
        }
        // Tonight: what sleeping now would grow, or lose.
        let band = Rect::new(l - 6., 398., r - l + 12., 84.);
        shapes::fill_rounded_rect(band, 12., with_alpha(NIGHT_INK, 0.1));
        self.night_row(l, 440., self.night(), false);
        // Actions.
        self.act_button();
        self.sleep_button(t);
    }
    /// A tiny glyph for what a weather does to crops.
    fn weather_effect(&self, weather: Weather, cx: f32, cy: f32, big: bool) {
        let s = if big { 1. } else { 0.8 };
        match weather {
            Weather::Rain => {
                icon(Icon::Drop, cx - 6. * s, cy, 16. * s, WATER_INK);
                icon(Icon::Check, cx + 9. * s, cy + 4. * s, 12. * s, INK_GREEN);
            }
            Weather::Heat => {
                icon(Icon::Drop, cx - 6. * s, cy, 16. * s, RUST);
                label("x2", cx + 3. * s, cy + 3. * s, 13. * s, RUST);
            }
            Weather::Frost => {
                icon(Icon::Sprout, cx - 6. * s, cy, 16. * s, INK_GREEN);
                icon(
                    if self.tools[2] {
                        Icon::Check
                    } else {
                        Icon::Pause
                    },
                    cx + 9. * s,
                    cy + 4. * s,
                    12. * s,
                    if self.tools[2] { INK_GREEN } else { NIGHT_INK },
                );
            }
            Weather::Sun => {}
        }
    }
    /// Moon, then chips for crops that grow, ripen, freeze, go dry or wilt.
    fn night_row(&self, x: f32, cy: f32, night: Night, dark: bool) {
        let moon = if dark { NIGHT } else { NIGHT_INK };
        icon(Icon::Moon, x + 14., cy, 26., moon);
        let mut cx = x + 36.;
        let size = 16.;
        let chips: [(Icon, u8, Color); 5] = [
            (
                Icon::Sprout,
                night.grow,
                if dark { ENERGY } else { INK_GREEN },
            ),
            (Icon::Check, night.ripen, if dark { GOLD } else { COIN_INK }),
            (
                Icon::Snowflake,
                night.frozen,
                if dark { ICE } else { WATER_INK },
            ),
            (Icon::Drop, night.dry, RUST),
            (Icon::Cross, night.wilt, RED),
        ];
        let mut any = false;
        for (glyph, count, color) in chips {
            if count == 0 {
                continue;
            }
            any = true;
            let n = num(u32::from(count));
            cx += icon_chip(glyph, n.as_str(), cx, cy - size * 0.725, size, color) + 5.;
        }
        if !any {
            icon(Icon::Minus, cx + 8., cy, 14., faded(moon, 0.5));
        }
    }
    /// The page's Space action as a face: icon, word, cost, and whether it
    /// would be accepted right now.
    fn act_face(&self) -> (Icon, &'static str, Option<(Icon, Label)>, bool) {
        match self.page {
            Page::Field => match self.field_check(self.cursor) {
                Ok(work) => {
                    let (g, word, cost) = work_mark(work);
                    (g, word, Some((Icon::Bolt, signed("-", cost))), true)
                }
                Err(Refusal::Watered) => (Icon::Moon, "Growing", None, false),
                Err(why) => {
                    let (g, word) = refusal_mark(why);
                    (g, word, None, false)
                }
            },
            Page::Stall => {
                let crop = self.row.min(2);
                let price = Some((Icon::Coin, signed("-", CROPS[crop].seed)));
                match self.seed_check(crop) {
                    Ok(()) => (Icon::Bag, "Buy", price, true),
                    Err(Refusal::Broke) => (Icon::Bag, "Buy", price, false),
                    Err(why) => {
                        let (g, word) = refusal_mark(why);
                        (g, word, None, false)
                    }
                }
            }
            Page::Orders => match self.deliver_check() {
                Ok(i) => (
                    Icon::Check,
                    "Deliver",
                    Some((Icon::Coin, signed("+", ORDERS[i].reward))),
                    true,
                ),
                Err(why) => {
                    let (g, word) = refusal_mark(why);
                    (g, word, None, false)
                }
            },
            Page::Tools => {
                let tool = self.row.min(2);
                let price = Some((Icon::Coin, signed("-", TOOL_PRICES[tool])));
                match self.tool_check(tool) {
                    Ok(()) => (Icon::Hammer, "Buy", price, true),
                    Err(Refusal::Broke) => (Icon::Hammer, "Buy", price, false),
                    Err(why) => {
                        let (g, word) = refusal_mark(why);
                        (g, word, None, false)
                    }
                }
            }
            Page::Almanac => (Icon::Moon, "Sleep", None, true),
        }
    }
    fn act_button(&self) {
        let (glyph, word, cost, ok) = self.act_face();
        let hot = !self.sleep_prompt && inside(ACT, pointer());
        let lift = button(ACT, GREEN, hot, ok);
        let [x, y, w, h] = ACT;
        let cy = y + h * 0.5 - lift;
        let ink = if ok { CREAM } else { faded(DARK, 0.6) };
        let s = if ok { style(16., ink) } else { flat(16., ink) };
        // Icon, word and cost, centred as one group.
        let ww = text_width(word, s);
        let cost_w = cost.map_or(0., |(_, value)| 26. + label_width(value.as_str(), 15.));
        let mut cx = x + (w - (24. + ww + cost_w)) * 0.5;
        icon(glyph, cx + 9., cy, 19., ink);
        cx += 24.;
        cx += draw_text_styled(word, cx, s.centered_baseline(cy), s).width;
        if let Some((g, value)) = cost {
            let tint = if ok {
                if g == Icon::Bolt { ENERGY } else { GOLD }
            } else {
                RUST
            };
            icon(g, cx + 16., cy, 15., tint);
            label(value.as_str(), cx + 25., cy, 15., tint);
        }
        key_badge("Space", x + 26., y - lift);
    }
    fn sleep_button(&self, t: f32) {
        let hot = !self.sleep_prompt && inside(SLEEP, pointer());
        // When no plot accepts work, the moon calls the day to a close.
        let call = self.chores_done();
        let [x, y, w, h] = SLEEP;
        if call {
            glow(
                x + w * 0.5,
                y + h * 0.5,
                40.,
                with_alpha(NIGHT_INK, 0.2 + 0.2 * pulse(t, 1.)),
            );
        }
        let lift = button(SLEEP, Color::new(0.30, 0.33, 0.52, 1.), hot || call, true);
        icon(Icon::Moon, x + w * 0.5, y + h * 0.5 - lift + 2., 22., NIGHT);
        key_badge("C", x + 10., y - lift + 2.);
    }
    /// Row background for the stall, village and workshop pages.
    fn row_bg(&self, a: Area, on: bool, hot: bool) {
        let [x, y, w, h] = a;
        if on {
            rounded(x, y + 2., w, h, 10., Color::new(0.2, 0.25, 0.1, 0.12));
            rounded(x, y, w, h, 10., ROW_ON);
            outline(x, y, w, h, 10., 2., GREEN);
            rounded(x + 6., y + 14., 4., h - 28., 2., GREEN);
        } else {
            rounded(x, y, w, h, 10., if hot { lighten(ROW, 0.3) } else { ROW });
            if hot {
                outline(x, y, w, h, 10., 1.5, with_alpha(GREEN, 0.45));
            }
        }
        shapes::fill_circle(
            vec2(x + 44., y + h * 0.5),
            28.,
            Color::new(1., 1., 1., if on { 0.42 } else { 0.32 }),
        );
    }
    fn page_card(&self) {
        paper_card(22., 156., 600., 408.);
    }
    /// A striped market awning across the top of the stall.
    fn awning(&self) {
        let (y0, depth) = (156., 26.);
        let stripes = [
            Color::new(0.80, 0.36, 0.30, 1.),
            Color::new(0.97, 0.93, 0.82, 1.),
        ];
        rounded(22., y0, 600., depth, 12., stripes[0]);
        for k in 0..20 {
            let x = 22. + k as f32 * 30.;
            let c = stripes[k % 2];
            if k > 0 && k < 19 {
                panel(x, y0, 30., depth, c);
            }
            shapes::fill_sector(vec2(x + 15., y0 + depth), 15., 0., std::f32::consts::PI, c);
        }
        panel(34., y0 + depth - 1., 576., 1., Color::new(0., 0., 0., 0.12));
    }
    fn stall(&self) {
        self.page_card();
        self.awning();
        let p = pointer();
        let needs = self.needs();
        let t = clock();
        for (i, crop) in CROPS.iter().enumerate() {
            let a = row_area(i);
            let [x, y, _, h] = a;
            let on = self.row == i;
            self.row_bg(a, on, !self.sleep_prompt && inside(a, p));
            crop_icon(i, x + 44., y + h * 0.5 + 4., 1.15, 1., false, 1.);
            draw_text_styled(crop.name, x + 86., y + 33., style(20., DARK));
            // Nights to grow, the stall's stock today and the seed tin.
            let cy = y + 60.;
            for k in 0..crop.nights {
                icon(Icon::Moon, x + 93. + f32::from(k) * 14., cy, 12., NIGHT_INK);
            }
            let sx = x + 160.;
            icon(Icon::Crate, sx, cy, 15., WOOD);
            let daily = [4, 3, 2][i];
            dots(
                sx + 11.,
                cy,
                (self.stock[i], daily),
                (3.4, 9.),
                (WOOD, faded(WOOD, 0.5)),
            );
            let bx = sx + 66.;
            icon(Icon::Bag, bx, cy, 15., BAG);
            label(num(self.seeds[i]).as_str(), bx + 11., cy, 15., DARK);
            // Buy and sell buttons, with their keys on the selected row.
            let buy = button_area(i, 0);
            let ok = self.seed_check(i).is_ok();
            let hot = !self.sleep_prompt && inside(buy, p);
            let lift = button(buy, GREEN, hot, ok);
            self.price_face(
                buy,
                lift,
                "Buy",
                signed("-", crop.seed),
                if ok { CREAM } else { faded(DARK, 0.55) },
            );
            if self.stock[i] == 0 {
                icon_badge(
                    Icon::Crate,
                    buy[0] + buy[2] - 4.,
                    buy[1] - lift + 2.,
                    16.,
                    RUST,
                    PAPER,
                );
            }
            let sell = button_area(i, 1);
            let have = self.produce(i);
            let hot = !self.sleep_prompt && inside(sell, p);
            let lift = button(sell, Color::new(0.78, 0.56, 0.20, 1.), hot, have > 0);
            self.price_face(
                sell,
                lift,
                "Sell",
                signed("+", crop.sale),
                if have > 0 { PAPER } else { faded(DARK, 0.55) },
            );
            if have > 0 {
                // How many you hold: green when spare, amber with a lock when
                // every one is promised to a village order.
                let spare = have > needs[i];
                let n = num(have);
                let bw = label_width(n.as_str(), 11.) + if spare { 12. } else { 24. };
                let (bx, by) = (sell[0] + sell[2] - bw + 6., sell[1] - lift - 6.);
                let fill = if spare { INK_GREEN } else { darken(GOLD, 0.25) };
                if !spare && on {
                    glow(
                        bx + bw * 0.5,
                        by + 8.,
                        18.,
                        with_alpha(GOLD, 0.25 * pulse(t, 1.2)),
                    );
                }
                rounded(bx, by, bw, 16., 8., fill);
                let mut tx = bx + 6.;
                if !spare {
                    icon(Icon::Lock, tx + 5., by + 8., 10., PAPER);
                    tx += 12.;
                }
                label(n.as_str(), tx, by + 8., 11., PAPER);
            }
            if on {
                key_badge("Space", buy[0] + 22., buy[1] - lift);
                key_badge("X", sell[0] + 12., sell[1] - lift);
            }
        }
    }
    /// "Buy (coin) -4" centred on a button.
    fn price_face(&self, a: Area, lift: f32, word: &str, value: Label, ink: Color) {
        let [x, y, w, h] = a;
        let cy = y + h * 0.5 - lift;
        let ww = label_width(word, 14.);
        let vw = label_width(value.as_str(), 16.);
        let total = ww + 8. + 16. + 3. + vw;
        let mut cx = x + (w - total) * 0.5;
        cx += label(word, cx, cy, 14., ink) + 8.;
        icon(Icon::Coin, cx + 8., cy, 16., ink);
        label(value.as_str(), cx + 19., cy, 16., ink);
    }
    /// Bunting across the top of the village page.
    fn bunting(&self, t: f32) {
        draw_line(22., 160., 622., 166., 1.5, BROWN);
        for k in 0..24 {
            let x = 30. + k as f32 * 24.5;
            let y = 160. + (x - 22.) / 600. * 6.;
            let sway = (t * 1.6 + k as f32 * 0.7).sin() * 1.5;
            let c = order_color(k % 3);
            draw_triangle(
                vec2(x, y),
                vec2(x + 16., y + 0.6),
                vec2(x + 8. + sway, y + 17.),
                faded(c, 0.9),
            );
        }
    }
    fn village(&self) {
        self.page_card();
        let t = clock();
        self.bunting(t);
        let p = pointer();
        let current = self.order();
        for (i, order) in ORDERS.iter().enumerate() {
            let a = row_area(i);
            let [x, y, _, h] = a;
            let hot = !self.sleep_prompt && inside(a, p);
            self.row_bg(a, current == Some(i), hot);
            lantern(x + 44., y + h * 0.5 + 2., self.delivered[i], t);
            draw_text_styled(order.name, x + 86., y + 33., style(20., DARK));
            // The fourteen days, with this table's window, today and its deadline.
            let cy = y + 60.;
            let tint = order_color(i);
            for d in 1..=14u8 {
                let cx = x + 89. + f32::from(d - 1) * 8.;
                let inside_window = d >= order.opens && d <= order.deadline;
                let c = if inside_window {
                    if d < self.day {
                        faded(tint, 0.45)
                    } else {
                        tint
                    }
                } else {
                    faded(SOFT, 0.18)
                };
                rounded(cx - 3., cy - 5., 6., 10., 2., c);
                if d == self.day {
                    draw_triangle(
                        vec2(cx - 4., cy - 11.),
                        vec2(cx + 4., cy - 11.),
                        vec2(cx, cy - 6.),
                        DARK,
                    );
                }
            }
            let rx = x + 88. + 14. * 8. + 6.;
            icon(Icon::Coin, rx + 8., cy, 15., COIN_INK);
            label(
                signed("+", order.reward).as_str(),
                rx + 18.,
                cy,
                15.,
                COIN_INK,
            );
            // Ingredients: crop and pips, filled by the basket.
            let mut ix = x + 272.;
            for c in 0..3 {
                let need = order.needs[c];
                if need == 0 {
                    continue;
                }
                let have = if self.delivered[i] {
                    need
                } else {
                    self.produce(c).min(need)
                };
                crop_glyph(c, ix + 10., y + h * 0.5, 24.);
                let met = have >= need;
                ix += 22.;
                ix += dots(
                    ix,
                    y + h * 0.5 + 2.,
                    (have, need),
                    (3.6, 9.),
                    (if met { INK_GREEN } else { tint }, faded(SOFT, 0.5)),
                ) + 12.;
            }
            // Status: served, deliver now, days left, or still locked.
            let b = button_area(i, 1);
            let bcy = b[1] + b[3] * 0.5;
            let bcx = b[0] + b[2] * 0.5;
            if self.delivered[i] {
                icon_badge(Icon::Check, bcx, bcy, 34., darken(LEAF, 0.15), PAPER);
            } else if current == Some(i) && self.day >= order.opens {
                let ready = self.deliver_check().is_ok();
                let hot = !self.sleep_prompt && inside(b, p);
                if ready {
                    glow(bcx, bcy, 70., with_alpha(GOLD, 0.18 + 0.14 * pulse(t, 1.2)));
                }
                let lift = button(b, GREEN, hot, ready);
                let cy = bcy - lift;
                if ready {
                    icon(Icon::Check, b[0] + 22., cy, 18., CREAM);
                    label("Deliver", b[0] + 36., cy, 15., CREAM);
                    key_badge("Space", b[0] + 24., b[1] - lift);
                } else {
                    let left = u32::from(order.deadline - self.day) + 1;
                    let ink = if left <= 1 { RUST } else { faded(DARK, 0.7) };
                    icon(Icon::Hourglass, bcx - 12., cy, 18., ink);
                    label(num(left).as_str(), bcx + 2., cy, 18., ink);
                }
            } else {
                icon(Icon::Lock, bcx - 22., bcy, 16., faded(SOFT, 0.8));
                label(day_label(order.opens).as_str(), bcx - 10., bcy, 15., SOFT);
            }
        }
    }
    /// A wooden beam with hooks across the top of the workshop.
    fn beam(&self) {
        rounded_gradient(
            34.,
            162.,
            576.,
            14.,
            4.,
            lighten(WOOD, 0.12),
            darken(WOOD, 0.1),
        );
        for k in 0..12 {
            let x = 58. + k as f32 * 48.;
            dot(x, 169., 2.2, darken(WOOD, 0.4));
            draw_line(x, 176., x, 184., 2., darken(WOOD, 0.3));
        }
    }
    fn tools_page(&self) {
        self.page_card();
        self.beam();
        let p = pointer();
        for i in 0..3 {
            let a = row_area(i);
            let [x, y, _, h] = a;
            let on = self.row == i;
            self.row_bg(a, on, !self.sleep_prompt && inside(a, p));
            tool_icon(i, x + 44., y + h * 0.5 + 2., self.tools[i]);
            draw_text_styled(TOOL_NAMES[i], x + 86., y + 33., style(19., DARK));
            // What the tool does, drawn instead of described.
            let cy = y + 60.;
            let ex = x + 90.;
            match i {
                0 => {
                    // One watering soaks the plot, its right and its lower neighbor.
                    for (cx, cyy, wet) in [
                        (0., 0., true),
                        (1., 0., true),
                        (0., 1., true),
                        (1., 1., false),
                    ] {
                        rounded(
                            ex + cx * 15.,
                            cy - 9. + cyy * 11.,
                            13.,
                            9.,
                            2.,
                            if wet { WATER_INK } else { faded(SOFT, 0.3) },
                        );
                    }
                    icon(Icon::Bolt, ex + 46., cy, 14., ENERGY_INK);
                    label("1", ex + 54., cy, 15., DARK);
                }
                1 => {
                    icon(Icon::Bolt, ex + 6., cy, 16., ENERGY_INK);
                    label("+6", ex + 16., cy, 16., ENERGY_INK);
                    icon(Icon::Sun, ex + 52., cy, 14., faded(COIN_INK, 0.9));
                }
                _ => {
                    icon(Icon::Snowflake, ex + 6., cy, 16., WATER_INK);
                    icon(Icon::ArrowRight, ex + 24., cy, 12., faded(SOFT, 0.7));
                    icon(Icon::Sprout, ex + 42., cy, 16., INK_GREEN);
                }
            }
            let b = button_area(i, 1);
            if self.tools[i] {
                icon_badge(
                    Icon::Check,
                    b[0] + b[2] * 0.5,
                    b[1] + b[3] * 0.5,
                    34.,
                    darken(LEAF, 0.15),
                    PAPER,
                );
            } else {
                let ok = self.tool_check(i).is_ok();
                let hot = !self.sleep_prompt && inside(b, p);
                let lift = button(b, GREEN, hot, ok);
                self.price_face(
                    b,
                    lift,
                    "Buy",
                    signed("-", TOOL_PRICES[i]),
                    if ok { CREAM } else { faded(DARK, 0.55) },
                );
                if on {
                    key_badge("Space", b[0] + 22., b[1] - lift);
                }
            }
        }
        self.compost_card(p);
    }
    /// Compost the selected field plot: a mini map shows which one.
    fn compost_card(&self, p: Vec2) {
        let t = clock();
        let (x, y, w, h) = (40., 492., 564., 60.);
        rounded(x, y, w, h, 10., darken(ROW, 0.03));
        let cy = y + h * 0.5;
        shapes::fill_circle(vec2(x + 44., cy), 24., Color::new(1., 1., 1., 0.32));
        icon(Icon::Leaf, x + 44., cy, 26., INK_GREEN);
        // Field map: soil per plot, the selected plot outlined.
        let (mx, my) = (x + 90., cy - 16.);
        for i in 0..PLOTS {
            let cx = mx + (i % COLS) as f32 * 15.;
            let cyy = my + (i / COLS) as f32 * 11.;
            let plot = self.plots[i];
            let c = match plot.soil {
                2 => Color::new(0.45, 0.30, 0.19, 1.),
                1 => Color::new(0.60, 0.45, 0.30, 1.),
                _ => Color::new(0.78, 0.68, 0.52, 1.),
            };
            rounded(cx, cyy, 13., 9., 2., c);
            if plot.plant.is_some() {
                dot(cx + 6.5, cyy + 4.5, 2., LEAF);
            }
            if i == self.cursor {
                outline(
                    cx - 1.5,
                    cyy - 1.5,
                    16.,
                    12.,
                    3.,
                    1.5,
                    with_alpha(GOLD, 0.7 + 0.3 * pulse(t, 1.)),
                );
            }
        }
        // Soil now, then after composting.
        let soil = u32::from(self.plots[self.cursor].soil);
        let check = self.compost_check();
        let sx = mx + 76.;
        dots(sx, cy, (soil, 2), (4., 11.), (GOLD, faded(SOFT, 0.5)));
        icon(Icon::ArrowRight, sx + 32., cy, 13., faded(SOFT, 0.7));
        dots(sx + 44., cy, (2, 2), (4., 11.), (GOLD, faded(SOFT, 0.5)));
        // Cost.
        let cx = sx + 86.;
        icon(Icon::Coin, cx, cy, 15., COIN_INK);
        label("-2", cx + 9., cy, 15., COIN_INK);
        icon(Icon::Bolt, cx + 40., cy, 15., ENERGY_INK);
        label("-1", cx + 49., cy, 15., ENERGY_INK);
        // Button, or the reason it would be refused.
        let ok = check.is_ok();
        let hot = !self.sleep_prompt && inside(COMPOST, p);
        let lift = button(COMPOST, GREEN, hot, ok);
        let bcy = COMPOST[1] + COMPOST[3] * 0.5 - lift;
        let ink = if ok { CREAM } else { faded(DARK, 0.55) };
        match check {
            Ok(()) | Err(Refusal::Tired) | Err(Refusal::Broke) => {
                icon(Icon::Leaf, COMPOST[0] + 22., bcy, 16., ink);
                label("Compost", COMPOST[0] + 34., bcy, 14., ink);
            }
            Err(why) => {
                let (g, word) = refusal_mark(why);
                icon(g, COMPOST[0] + 22., bcy, 16., ink);
                label(word, COMPOST[0] + 34., bcy, 14., ink);
            }
        }
        key_badge("X", COMPOST[0] + 12., COMPOST[1] - lift);
    }
    /// Calendar rings across the top of the almanac.
    fn rings(&self) {
        for k in 0..14 {
            let x = 58. + k as f32 * 40.;
            shapes::fill_circle(vec2(x, 172.), 4., Color::new(0.2, 0.18, 0.14, 0.5));
            draw_line(x, 160., x, 172., 2.5, Color::new(0.55, 0.55, 0.58, 1.));
        }
    }
    fn almanac(&self) {
        self.page_card();
        self.rings();
        let t = clock();
        let p = pointer();
        for i in 0..14u8 {
            let day = i + 1;
            let morning = &MORNINGS[usize::from(i)];
            let [x, y, w, h] = day_area(i);
            let today = day == self.day;
            let past = day < self.day;
            let hot = !self.sleep_prompt && inside(day_area(i), p);
            if today {
                glow(x + w * 0.5, y + h * 0.5, 70., with_alpha(GOLD, 0.16));
                rounded(x, y + 2., w, h, 9., Color::new(0.2, 0.25, 0.1, 0.14));
                rounded(x, y, w, h, 9., ROW_ON);
                outline(x, y, w, h, 9., 2., GREEN);
            } else {
                rounded(
                    x,
                    y,
                    w,
                    h,
                    9.,
                    if past {
                        darken(ROW, 0.05)
                    } else if hot {
                        lighten(ROW, 0.3)
                    } else {
                        ROW
                    },
                );
            }
            let o = order_of_day(day);
            rounded(
                x + 6.,
                y + h - 10.,
                w - 12.,
                5.,
                2.5,
                faded(order_color(o), if past { 0.4 } else { 1. }),
            );
            let ink = if past { faded(SOFT, 0.6) } else { DARK };
            label(num(u32::from(day)).as_str(), x + 9., y + 15., 15., ink);
            if day == ORDERS[o].deadline {
                // A lantern marks each table's last day; it lights once served.
                lantern(x + w - 16., y + 22., self.delivered[o], t);
            }
            let wx = x + w * 0.5;
            let wy = y + 50. + if today { bob(t, 0.8, 2.) } else { 0. };
            let tint = if past {
                faded(weather_ink(morning.weather), 0.45)
            } else {
                weather_ink(morning.weather)
            };
            glyph_lit(weather_icon(morning.weather), wx, wy, 30., tint);
            if !past {
                self.weather_effect(morning.weather, wx, y + 84., false);
            }
            if today {
                let py = y - 6. + bob(t, 1.2, 2.);
                icon(Icon::Pin, wx, py, 18., RUST);
            }
        }
        // Each table's window as a colored key under the calendar.
        let ky = 470.;
        let mut kx = 44.;
        for (i, order) in ORDERS.iter().enumerate() {
            rounded(kx, ky - 3., 18., 6., 3., order_color(i));
            lantern(kx + 32., ky + 1., self.delivered[i], t);
            let w = label(order.name, kx + 46., ky, 14., DARK);
            kx += 46. + w + 30.;
        }
    }
    /// On hover, a village row or a past almanac day shows its letter. Drawn
    /// last so the note sits above the ledger.
    fn hover_note(&self) {
        if self.sleep_prompt {
            return;
        }
        let p = pointer();
        let letter = match self.page {
            Page::Orders => (0..3)
                .find(|&i| inside(row_area(i), p))
                .map(|i| ORDERS[i].letter),
            Page::Almanac => (0..self.day)
                .find(|&i| inside(day_area(i), p))
                .map(|i| MORNINGS[usize::from(i)].letter),
            _ => None,
        };
        if let Some(letter) = letter {
            note(letter, p);
        }
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
        match self.page {
            Page::Field => self.farm(),
            Page::Stall => self.stall(),
            Page::Orders => self.village(),
            Page::Tools => self.tools_page(),
            Page::Almanac => self.almanac(),
        }
        self.tabs();
        self.ledger();
        self.hover_note();
        if self.sleep_prompt {
            self.sleep_card();
        }
    }
    fn sleep_card(&self) {
        let t = clock();
        let p = pointer();
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
        let deadline = self.deadline_tonight();
        // Title: a moon (or the festival flame) and one word.
        let (glyph, tint, title) = if festival {
            (Icon::Flame, FLAME, "Festival")
        } else {
            (Icon::Moon, NIGHT_INK, "Sleep")
        };
        glow(
            150.,
            206.,
            36.,
            with_alpha(tint, 0.18 + 0.1 * pulse(t, 0.7)),
        );
        icons::draw_styled(
            glyph,
            vec2(150., 206.),
            44.,
            IconStyle::new(tint).shadow(Color::new(0., 0., 0., 0.2)),
        );
        draw_text_styled(title, 186., 218., style(32., DARK));
        // Tonight's weather into tomorrow's, top right.
        let today = self.weather();
        glyph_lit(weather_icon(today), 700., 204., 34., weather_ink(today));
        icon(Icon::ArrowRight, 744., 206., 18., faded(SOFT, 0.7));
        match self.tomorrow() {
            Some(next) => glyph_lit(weather_icon(next), 790., 204., 34., weather_ink(next)),
            None => lantern(790., 206., true, t),
        }
        if let Some(i) = deadline {
            // Sleeping now ends the season: the unserved table, in red.
            let (bx, by, bw, bh) = (128., 262., 704., 150.);
            rounded(bx, by, bw, bh, 12., with_alpha(RUST, 0.12));
            rounded(bx, by, 5., bh, 2.5, RUST);
            icon(Icon::Alert, bx + 44., by + 50., 40., RUST);
            lantern(bx + 104., by + 52., false, t);
            draw_text_styled(ORDERS[i].name, bx + 128., by + 60., style(26., DARK));
            let mut ix = bx + 128.;
            for c in 0..3 {
                let need = ORDERS[i].needs[c];
                if need == 0 {
                    continue;
                }
                let have = self.produce(c).min(need);
                crop_glyph(c, ix + 12., by + 104., 28.);
                ix += 28.;
                ix += dots(ix, by + 106., (have, need), (5., 13.), (INK_GREEN, RUST)) + 20.;
            }
            icon(Icon::Cross, bx + bw - 50., by + bh * 0.5, 44., RUST);
        } else if festival {
            // The feast needs every table served and 80 coins for the oven.
            let served = self.orders_completed() as u32;
            let all = served == 3;
            let r = Rect::new(252., 266., 200., 132.);
            let tint = if all { COIN_INK } else { RUST };
            shapes::fill_rounded_rect(r, 12., with_alpha(tint, 0.1));
            shapes::stroke_rounded_rect(r, 12., 1., with_alpha(tint, 0.3));
            for k in 0..3 {
                lantern(312. + k as f32 * 40., 314., self.delivered[k], t);
            }
            label_mid(num(served).as_str(), 352., 366., 30., darken(tint, 0.15));
            let ready = self.coins >= 80;
            let tint = if ready { FLAME } else { RUST };
            let r = Rect::new(468., 266., 240., 132.);
            shapes::fill_rounded_rect(r, 12., with_alpha(tint, 0.1));
            shapes::stroke_rounded_rect(r, 12., 1., with_alpha(tint, 0.3));
            if ready {
                glow(
                    588.,
                    312.,
                    40.,
                    with_alpha(FLAME, 0.2 + 0.1 * pulse(t, 0.8)),
                );
            }
            icons::draw_styled(
                Icon::Flame,
                vec2(588., 312.),
                40.,
                IconStyle::new(if ready { FLAME } else { faded(SOFT, 0.6) })
                    .shadow(Color::new(0., 0., 0., 0.15)),
            );
            meter(
                492.,
                344.,
                192.,
                10.,
                self.coins as f32 / 80.,
                if ready { ENERGY_INK } else { RUST },
            );
            icon(Icon::Coin, 548., 372., 18., COIN_INK);
            label(num(self.coins).as_str(), 560., 372., 22., BROWN);
            if ready {
                icon(Icon::Check, 660., 372., 18., INK_GREEN);
            }
        } else {
            // What the night does to the field, as big tiles: growth first,
            // then losses, then any energy left unspent tonight.
            let night = self.night();
            let tiles: [(Icon, u32, Color, bool); 6] = [
                (Icon::Sprout, u32::from(night.grow), INK_GREEN, true),
                (
                    Icon::Check,
                    u32::from(night.ripen),
                    COIN_INK,
                    night.ripen > 0,
                ),
                (
                    Icon::Snowflake,
                    u32::from(night.frozen),
                    WATER_INK,
                    night.frozen > 0,
                ),
                (Icon::Drop, u32::from(night.dry), RUST, night.dry > 0),
                (Icon::Cross, u32::from(night.wilt), RED, night.wilt > 0),
                (
                    Icon::Bolt,
                    u32::from(self.energy),
                    if self.energy > 0 {
                        COIN_INK
                    } else {
                        faded(SOFT, 0.6)
                    },
                    true,
                ),
            ];
            // Centre the row of tiles in the card.
            let shown = tiles.iter().filter(|t| t.3).count() as f32;
            let mut tx = 480. - (shown * 116. - 12. + 26.) * 0.5;
            for (glyph, count, color, show) in tiles {
                if !show {
                    continue;
                }
                let energy = glyph == Icon::Bolt;
                if energy {
                    // Energy sits apart: it is what you leave behind.
                    tx += 26.;
                }
                let r = Rect::new(tx, 266., 104., 132.);
                shapes::fill_rounded_rect(r, 12., with_alpha(color, 0.1));
                shapes::stroke_rounded_rect(r, 12., 1., with_alpha(color, 0.3));
                if energy {
                    icon(Icon::Moon, tx + 86., 284., 14., faded(NIGHT_INK, 0.7));
                }
                icons::draw_styled(
                    glyph,
                    vec2(tx + 52., 312.),
                    40.,
                    IconStyle::new(color).shadow(Color::new(0., 0., 0., 0.15)),
                );
                label_mid(
                    num(count).as_str(),
                    tx + 52.,
                    366.,
                    30.,
                    darken(color, 0.15),
                );
                tx += 116.;
            }
        }
        // Buttons share the model's click regions.
        let hot = inside(CONFIRM, p);
        let fill = if deadline.is_some() { RUST } else { GREEN };
        let lift = button(CONFIRM, fill, hot, true);
        let cy = CONFIRM[1] + CONFIRM[3] * 0.5 - lift;
        let k = keycap("Space", "", CONFIRM[0] + 18., cy - 10., CREAM);
        icon(glyph, CONFIRM[0] + 18. + k + 22., cy, 20., CREAM);
        label(title, CONFIRM[0] + 18. + k + 38., cy, 17., CREAM);
        let hot = inside(CANCEL, p);
        let lift = button(CANCEL, darken(ROW, 0.06), hot, true);
        let cy = CANCEL[1] + CANCEL[3] * 0.5 - lift;
        let k = keycap("X", "", CANCEL[0] + 18., cy - 10., CREAM);
        icon(Icon::ArrowLeft, CANCEL[0] + 18. + k + 22., cy, 18., DARK);
        label("Back", CANCEL[0] + 18. + k + 38., cy, 17., DARK);
    }
    /// Effects for one presentation event, in canvas units.
    fn event_effects(&self, event: Event, fx: &mut Juice) {
        let field = self.page == Page::Field;
        let button_center = |row: usize, slot: usize| {
            let a = button_area(row, slot);
            vec2(a[0] + a[2] * 0.5, a[1] + a[3] * 0.5)
        };
        let soil = Color::new(0.45, 0.30, 0.19, 1.);
        match event {
            Event::Plant { plot } => {
                let at = plot_center(plot);
                fx.burst(at + vec2(0., 10.), soil, 12);
                fx.ring(at, LEAF, 34.);
                fx.popup_icon(at - vec2(0., 30.), Icon::Bolt, "-2", ENERGY);
            }
            Event::Water { plot } => {
                let at = plot_center(plot);
                fx.ring(at, WATER, 48.);
                fx.burst(at - vec2(0., 6.), lighten(WATER, 0.2), 14);
            }
            Event::Harvest { plot, crop } => {
                let at = plot_center(plot);
                fx.burst(at, crop_color(crop), 26);
                fx.sparkle(at - vec2(0., 8.), GOLD);
                fx.ring(at, GOLD, 52.);
                fx.popup_icon(
                    at - vec2(0., 30.),
                    Icon::Hand,
                    "+1",
                    lighten(crop_color(crop), 0.25),
                );
            }
            Event::Clear { plot } => {
                let at = plot_center(plot);
                fx.burst(at, Color::new(0.55, 0.42, 0.22, 1.), 16);
                fx.ring(at, RUST, 36.);
            }
            Event::Compost { .. } => {
                let at = vec2(COMPOST[0] + COMPOST[2] * 0.5, COMPOST[1] + 4.);
                fx.burst(at, LEAF, 18);
                fx.sparkle(vec2(206., 522.), GOLD);
                fx.popup_icon(at - vec2(0., 18.), Icon::Coin, "-2", GOLD);
            }
            Event::Buy { crop } => {
                let at = button_center(crop, 0);
                fx.ring(at, LEAF, 40.);
                fx.popup_icon(
                    at - vec2(0., 28.),
                    Icon::Coin,
                    signed("-", CROPS[crop].seed).as_str(),
                    GOLD,
                );
            }
            Event::Sell { crop } => {
                let at = button_center(crop, 1);
                fx.burst(at, GOLD, 16);
                fx.popup_icon(
                    at - vec2(0., 28.),
                    Icon::Coin,
                    signed("+", CROPS[crop].sale).as_str(),
                    GOLD,
                );
            }
            Event::Deliver { order } => {
                let at = button_center(order, 1);
                let lamp = vec2(84., row_area(order)[1] + 46.);
                fx.burst(at, GOLD, 44);
                fx.ring(lamp, GOLD, 90.);
                fx.sparkle(lamp, GOLD);
                fx.confetti(at, 40);
                fx.popup_icon(
                    at - vec2(0., 30.),
                    Icon::Coin,
                    signed("+", ORDERS[order].reward).as_str(),
                    GOLD,
                );
                fx.callout(vec2(322., 300.), ORDERS[order].name, GOLD);
            }
            Event::Tool { tool } => {
                let at = button_center(tool, 1);
                fx.sparkle(at, GOLD);
                fx.ring(at, GOLD, 60.);
                fx.popup_icon(
                    at - vec2(0., 28.),
                    Icon::Coin,
                    signed("-", TOOL_PRICES[tool]).as_str(),
                    GOLD,
                );
                fx.toast(Some(Icon::Hammer), TOOL_NAMES[tool], GOLD);
            }
            Event::Grow { plot, ripe } if field => {
                let at = plot_center(plot);
                if ripe {
                    fx.sparkle(at - vec2(0., 10.), GOLD);
                    fx.ring(at, GOLD, 44.);
                } else {
                    fx.burst(at - vec2(0., 8.), LEAF, 6);
                }
            }
            Event::Frozen { plot } if field => {
                fx.popup_icon(plot_center(plot) - vec2(0., 20.), Icon::Snowflake, "", ICE);
            }
            Event::Thirst { plot } if field => {
                let at = plot_center(plot);
                fx.ring(at, RUST, 38.);
                fx.popup_icon(at - vec2(0., 20.), Icon::Drop, "", RED);
            }
            Event::Wilt { plot } => {
                if field {
                    let at = plot_center(plot);
                    fx.burst(at, Color::new(0.55, 0.42, 0.22, 1.), 18);
                    fx.ring(at, RED, 44.);
                    fx.popup_icon(at - vec2(0., 22.), Icon::Cross, "", RED);
                }
                fx.vignette(Color::new(0.85, 0.3, 0.15, 0.18));
            }
            Event::Grow { .. } | Event::Frozen { .. } | Event::Thirst { .. } => {}
            Event::Dawn => {
                fx.flash(Color::new(1., 0.92, 0.72, 0.16));
                // The last morning of an unserved table's window glows amber.
                if self.deadline_tonight().is_some() {
                    fx.vignette(Color::new(0.95, 0.55, 0.15, 0.32));
                }
                fx.callout(vec2(322., 330.), day_label(self.day).as_str(), CREAM);
                let morning = &MORNINGS[usize::from(self.day - 1)];
                fx.toast(
                    Some(weather_icon(morning.weather)),
                    first_sentence(morning.letter),
                    weather_color(morning.weather),
                );
            }
            Event::Festival => {
                fx.callout(vec2(322., 300.), "Festival!", GOLD);
                for k in 0..3 {
                    fx.sparkle(vec2(399. + k as f32 * 37., 207.), GOLD);
                }
            }
            Event::Failed => {
                fx.callout(vec2(322., 300.), self.rank, RED);
            }
            Event::Refuse(why, spot) => {
                let at = match spot {
                    Spot::Plot(i) => plot_center(i),
                    Spot::Buy(row) => button_center(row, 0),
                    Spot::Sell(row) | Spot::Tool(row) | Spot::Order(row) => button_center(row, 1),
                    Spot::Compost => {
                        vec2(COMPOST[0] + COMPOST[2] * 0.5, COMPOST[1] + COMPOST[3] * 0.5)
                    }
                };
                fx.ring(at, RED, 32.);
                fx.shake(0.22);
                let (glyph, word) = refusal_mark(why);
                if why == Refusal::Locked
                    && let Spot::Order(i) = spot
                {
                    fx.popup_icon(
                        at - vec2(0., 26.),
                        glyph,
                        day_label(ORDERS[i].opens).as_str(),
                        RED,
                    );
                } else {
                    fx.popup_icon(at - vec2(0., 26.), glyph, word, RED);
                }
                if why == Refusal::NoSeed {
                    let a = seed_area(self.crop);
                    fx.ring(vec2(a[0] + a[2] * 0.5, a[1] + a[3] * 0.5), RED, 40.);
                }
            }
        }
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
            stage: match self.result {
                Outcome::Playing => self.order().map_or("Festival", |i| ORDERS[i].name),
                _ => self.rank,
            }
            .to_string(),
            objective: match self.order() {
                Some(i) => format!("{} by day {}", ORDERS[i].name, ORDERS[i].deadline),
                None => "80 coins for the oven".into(),
            },
            status: format!(
                "Day {} of 14, {} | Energy {}/{} | Coins {} | Orders {}/3 | Seed: {}",
                self.day,
                self.weather().name(),
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
        match self.result {
            Outcome::Won => format!(
                "{}: all three village tables served, {} crops harvested and {} coins saved.",
                self.rank, self.harvested, self.coins
            ),
            Outcome::Lost => match self.order() {
                Some(i) => format!(
                    "{}: {} was not served by day {}.",
                    self.rank, ORDERS[i].name, ORDERS[i].deadline
                ),
                None => format!(
                    "{}: the oven needed 80 coins and the farm had {}.",
                    self.rank, self.coins
                ),
            },
            Outcome::Playing => format!(
                "Day {} of 14 with {} coins and {} of 3 village orders served.",
                self.day,
                self.coins,
                self.orders_completed()
            ),
        }
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.autoplay(tick)
    }
    fn progress(&self) -> Option<(u32, u32)> {
        Some((u32::from(self.day), 14))
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        for &event in self.events() {
            self.event_effects(event, fx);
        }
    }
    fn draw_objective(&self, area: Rect) -> bool {
        let cy = area.y + area.h * 0.5;
        let mut x = area.x + 6.;
        let t = clock();
        match self.order() {
            Some(i) => {
                let order = &ORDERS[i];
                let tint = order_color(i);
                // Ingredients: the crop and one pip per item, filled from the basket.
                for c in 0..3 {
                    let need = order.needs[c];
                    if need == 0 {
                        continue;
                    }
                    let have = self.produce(c).min(need);
                    let met = have >= need;
                    crop_glyph(c, x + 10., cy, 24.);
                    x += 24.;
                    x += dots(
                        x,
                        cy,
                        (have, need),
                        (4.5, 13.),
                        (
                            if met { ENERGY } else { lighten(tint, 0.2) },
                            Color::new(0.84, 0.88, 0.92, 0.45),
                        ),
                    );
                    if met {
                        icon(Icon::Check, x + 12., cy, 14., ENERGY);
                        x += 18.;
                    }
                    x += 22.;
                }
                // Time: days left in the window, or the day it opens.
                if self.day < order.opens {
                    glyph_lit(Icon::Lock, x + 8., cy, 16., MUTED);
                    x += label(day_label(order.opens).as_str(), x + 20., cy, 17., MUTED) + 34.;
                } else {
                    let left = u32::from(order.deadline - self.day) + 1;
                    let urgent = left <= 1;
                    let ink = if urgent {
                        lighten(RED, 0.1 + 0.2 * pulse(t, 1.5))
                    } else {
                        Color::new(0.84, 0.88, 0.92, 1.)
                    };
                    glyph_lit(Icon::Hourglass, x + 8., cy, 17., ink);
                    x += label(num(left).as_str(), x + 20., cy, 18., ink) + 34.;
                }
                // Reward.
                glyph_lit(Icon::Coin, x + 8., cy, 17., GOLD);
                x += label(signed("+", order.reward).as_str(), x + 20., cy, 17., GOLD) + 34.;
                // Ready to serve: point at the village page.
                if self.deliver_check().is_ok() {
                    let a = pulse(t, 1.2);
                    glow(x + 40., cy, 46., with_alpha(GOLD, 0.15 + 0.15 * a));
                    keycap("3", "Deliver", x, cy - 10., lighten(GOLD, 0.2 * a));
                }
            }
            None => {
                // Every table is served: keep 80 coins for the oven.
                let lit = self.coins >= 80;
                glyph_lit(
                    Icon::Flame,
                    x + 9.,
                    cy,
                    20.,
                    if lit { FLAME } else { MUTED },
                );
                x += 26.;
                glyph_lit(Icon::Coin, x + 8., cy, 16., GOLD);
                x += 20.;
                meter(
                    x,
                    cy - 4.,
                    150.,
                    8.,
                    self.coins as f32 / 80.,
                    if lit { ENERGY } else { GOLD },
                );
                x += 158.;
                x += label(
                    num(self.coins).as_str(),
                    x,
                    cy,
                    17.,
                    if lit { ENERGY } else { GOLD },
                );
                x += 30.;
                let left = 14 - u32::from(self.day) + 1;
                glyph_lit(Icon::Hourglass, x + 8., cy, 17., MUTED);
                label(
                    num(left).as_str(),
                    x + 20.,
                    cy,
                    18.,
                    Color::new(0.84, 0.88, 0.92, 1.),
                );
            }
        }
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        let t = clock();
        let won = self.result == Outcome::Won;
        let gap = 14.;
        let tw = (area.w - gap * 3.) / 4.;
        let th = area.h.min(116.);
        let top = area.y + (area.h - th) * 0.5;
        let ink = Color::new(0.93, 0.95, 0.96, 1.);
        for k in 0..4 {
            let x = area.x + k as f32 * (tw + gap);
            let r = Rect::new(x, top, tw, th);
            shapes::fill_rounded_rect(r, 12., Color::new(1., 1., 1., 0.05));
            shapes::stroke_rounded_rect(r, 12., 1., Color::new(1., 1., 1., 0.08));
            let cx = x + tw * 0.5;
            let icy = top + th * 0.38;
            let vcy = top + th * 0.76;
            match k {
                0 => {
                    // Tables served, as lanterns.
                    for i in 0..3 {
                        lantern(cx - 30. + i as f32 * 30., icy + 2., self.delivered[i], t);
                    }
                    let served = num(self.orders_completed() as u32);
                    label_mid(served.as_str(), cx - 8., vcy, 24., ink);
                    label("/3", cx + 2., vcy, 15., MUTED);
                }
                1 => {
                    glyph_lit(Icon::Sprout, cx, icy, 34., ENERGY);
                    label_mid(num(self.harvested).as_str(), cx, vcy, 26., ink);
                }
                2 => {
                    glyph_lit(Icon::Coin, cx, icy, 34., GOLD);
                    let short = !won && self.order().is_none();
                    label_mid(
                        num(self.coins).as_str(),
                        cx,
                        vcy,
                        26.,
                        if short { RED } else { ink },
                    );
                }
                _ => {
                    if won {
                        let owned = self.tools.iter().filter(|&&v| v).count() as u32;
                        glyph_lit(Icon::Hammer, cx, icy, 32., lighten(WOOD, 0.3));
                        let w = 9. + 2. * 16.;
                        dots(cx - w * 0.5, vcy, (owned, 3), (5.5, 16.), (GOLD, MUTED));
                    } else if let Some(i) = self.order() {
                        // The table that went unserved.
                        glyph_lit(Icon::Hourglass, cx, icy, 32., RED);
                        label_mid(day_label(ORDERS[i].deadline).as_str(), cx, vcy, 22., RED);
                    } else {
                        // The oven stayed cold.
                        glyph_lit(Icon::Flame, cx, icy, 32., MUTED);
                        glyph_lit(Icon::Coin, cx - 18., vcy, 18., GOLD);
                        label("80", cx - 6., vcy, 22., RED);
                    }
                }
            }
        }
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Plant;
    use ferravex_play::gameplay::inventory::Stack;
    fn primary(g: &mut Game) -> Cue {
        g.update(&Input {
            primary: true,
            ..Input::default()
        })
    }
    fn felt(g: &Game, cue: Cue) -> juice::JuiceCounts {
        let mut fx = Juice::new();
        g.effects(cue, &mut fx);
        fx.counts()
    }
    #[test]
    fn progress_counts_mornings_from_day_one_to_the_festival() {
        let mut g = Game::new();
        assert_eq!(g.progress(), Some((1, 14)));
        assert_eq!(g.hud().stage, "Soup Night");
        for tick in 0..100_000 {
            let input = g.bot_input(tick);
            g.update(&input);
            if g.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(g.outcome(), Outcome::Won);
        assert_eq!(g.progress(), Some((14, 14)));
        // The results eyebrow names the rank; the summary is one sentence.
        assert_eq!(g.hud().stage, g.rank);
        let summary = g.summary();
        assert!(
            summary.ends_with('.') && !summary.contains(". "),
            "{summary}"
        );
    }
    #[test]
    fn harvests_deliveries_and_dawn_are_felt_where_they_happen() {
        let mut g = Game::new();
        g.plots[0].plant = Some(Plant {
            crop: 0,
            age: 2,
            nights: 2,
            watered: true,
            stress: 0,
            quality: 2,
        });
        let cue = primary(&mut g);
        assert_eq!(cue, Cue::Good);
        let c = felt(&g, cue);
        assert!(c.sparks >= 20 && c.popups == 1 && c.rings == 1, "{c:?}");
        // Delivering a table: a burst with confetti, a coin popup and a callout.
        g.page = Page::Orders;
        g.produce
            .add(Stack {
                item: 0,
                amount: 3,
                limit: 99,
            })
            .unwrap();
        let cue = primary(&mut g);
        assert_eq!(cue, Cue::Good);
        let c = felt(&g, cue);
        assert!(c.sparks >= 60 && c.popups == 2 && c.rings == 1, "{c:?}");
        // A new morning: a callout and the letter's first line as a toast.
        g.update(&Input {
            tertiary: true,
            ..Input::default()
        });
        let cue = primary(&mut g);
        assert_eq!((cue, g.day), (Cue::Good, 2));
        let c = felt(&g, cue);
        assert!(c.toast && c.popups == 1, "{c:?}");
        // An idle tick asks for nothing.
        let cue = g.update(&Input::default());
        assert_eq!(felt(&g, cue), juice::JuiceCounts::default());
    }
    #[test]
    fn refusals_ring_the_thing_that_refused() {
        let mut g = Game::new();
        g.energy = 1;
        let cue = primary(&mut g);
        assert_eq!(cue, Cue::None);
        assert_eq!(g.energy, 1);
        let c = felt(&g, cue);
        assert_eq!((c.rings, c.popups), (1, 1));
        // Running out of seeds also rings the empty packet.
        g.energy = 18;
        g.seeds[0] = 0;
        let cue = primary(&mut g);
        let c = felt(&g, cue);
        assert_eq!((c.rings, c.popups), (2, 1));
    }
    #[test]
    fn header_copy_stays_short_and_toasts_fit() {
        let mut g = Game::new();
        for delivered in [
            [false; 3],
            [true, false, false],
            [true, true, false],
            [true; 3],
        ] {
            g.delivered = delivered;
            let hud = g.hud();
            assert!(
                hud.objective.split_whitespace().count() <= 5,
                "{}",
                hud.objective
            );
            assert!(hud.stage.split_whitespace().count() <= 3, "{}", hud.stage);
        }
        for morning in &MORNINGS[1..] {
            let line = first_sentence(morning.letter);
            assert!(line.len() <= juice::MAX_LABEL, "{line}");
            assert!(line.ends_with(['.', '!']), "{line}");
        }
        assert_eq!(
            first_sentence("Rain on the roof! More."),
            "Rain on the roof!"
        );
        assert_eq!(num(0).as_str(), "0");
        assert_eq!(signed("+", 120).as_str(), "+120");
        assert_eq!(day_label(14).as_str(), "Day 14");
        // Both failure summaries are single sentences too.
        let mut g = Game::new();
        g.day = 5;
        g.update(&Input {
            tertiary: true,
            ..Input::default()
        });
        primary(&mut g);
        assert_eq!(g.outcome(), Outcome::Lost);
        assert_eq!(
            g.summary(),
            "An Empty Table: Soup Night was not served by day 5."
        );
        let mut g = Game::new();
        g.day = 14;
        g.delivered = [true; 3];
        g.coins = 79;
        g.update(&Input {
            tertiary: true,
            ..Input::default()
        });
        primary(&mut g);
        assert_eq!(
            g.summary(),
            "A Cold Oven: the oven needed 80 coins and the farm had 79."
        );
    }
}
