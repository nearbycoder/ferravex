//! Read-only field notebook and museum presentation.
//!
//! The model has no presentation clock, so this view is purely a function of
//! expedition state: every highlight comes from selection, depth or outcome.
use crate::content::{SITES, SPECIES};
use crate::model::{COLS, GRID_X, GRID_Y, Game, PITCH_X, PITCH_Y, Page, Phase};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use ferravex_arcade::*;

const EARTH: Color = Color::new(0.17, 0.12, 0.09, 1.);
const CARD: Color = Color::new(0.215, 0.165, 0.12, 1.);
const SAND: Color = Color::new(0.83, 0.65, 0.40, 1.);
const BONE: Color = Color::new(0.97, 0.90, 0.71, 1.);
const GOLD: Color = Color::new(1., 0.70, 0.29, 1.);
const GREEN: Color = Color::new(0.50, 0.76, 0.57, 1.);
const RED: Color = Color::new(0.95, 0.48, 0.35, 1.);
/// Secondary copy on warm surfaces.
const DUST: Color = Color::new(0.72, 0.62, 0.5, 1.);
const HAIRLINE: Color = Color::new(1., 0.9, 0.75, 0.12);

/// Shadowless text for flat surfaces: crisp and half the geometry.
fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
fn say(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(size, color)).width
}
fn say_right(value: &str, right: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(
        value,
        right,
        y,
        ink(size, color).with_align(TextAlign::Right),
    )
    .width
}
fn say_center(value: &str, x: f32, y: f32, size: f32, color: Color) {
    draw_text_styled(value, x, y, ink(size, color).with_align(TextAlign::Center));
}
/// Draw shrunk to `width` if needed, never enlarged.
fn label(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) {
    let w = text_width(value, ink(size, color));
    say(
        value,
        x,
        y,
        if w > width { size * width / w } else { size },
        color,
    );
}
/// Wrapped paragraph; returns the next baseline.
fn paragraph(value: &str, x: f32, y: f32, width: f32, size: f32, color: Color) -> f32 {
    let mut baseline = y;
    for line in wrap_text(value, width, ink(size, color)) {
        say(line, x, baseline, size, color);
        baseline += size * 1.35;
    }
    baseline
}
/// Small letter-spaced section label; returns its width.
fn eyebrow(value: &str, x: f32, y: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, ink(11., color).with_letter_spacing(1.5)).width
}
fn chip_width(value: &str, size: f32) -> f32 {
    text_width(value, style(size, PAPER)) + size * 1.3
}
/// `"PREDATOR TOOTH"` -> `"Predator Tooth"`.
fn title_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut start = true;
    for c in value.chars() {
        if start {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
        start = c == ' ' || c == '-';
    }
    out
}
/// Interface card in the expedition's warm palette.
fn slab(x: f32, y: f32, w: f32, h: f32) {
    card(x, y, w, h, CARD);
}
/// Selectable row: raised when chosen, quiet otherwise.
fn row_surface(x: f32, y: f32, w: f32, h: f32, selected: bool) {
    if selected {
        rounded_gradient(
            x,
            y,
            w,
            h,
            8.,
            Color::new(0.4, 0.3, 0.18, 1.),
            Color::new(0.33, 0.245, 0.15, 1.),
        );
        outline(x, y, w, h, 8., 1.5, GOLD);
    } else {
        rounded(x, y, w, h, 8., Color::new(1., 0.92, 0.8, 0.045));
    }
}
fn stars(count: u8, x: f32, y: f32, size: f32) {
    let mut at = x;
    for n in 0..3 {
        let on = n < count;
        at += draw_text_styled(
            "★",
            at,
            y,
            ink(
                size,
                if on {
                    GOLD
                } else {
                    Color::new(0.4, 0.34, 0.27, 1.)
                },
            ),
        )
        .width
            + size * 0.15;
    }
}
fn bone(a: Vec2, b: Vec2, r: f32, color: Color) {
    draw_line(a.x, a.y, b.x, b.y, r * 1.3, color);
    draw_circle(a.x, a.y, r, color);
    draw_circle(b.x, b.y, r, color);
}
/// A specimen silhouette with a fixed visual bounding box of radius `r`.
/// `hollow` is the color used for interior cut lines.
fn specimen(kind: usize, x: f32, y: f32, r: f32, color: Color, hollow: Color) {
    match kind {
        0 => {
            let mut prev = vec2(x, y);
            for i in 1..40 {
                let t = i as f32 / 39.;
                let a = t * 12.5;
                let p = vec2(x + a.cos() * r * t, y + a.sin() * r * t);
                draw_line(prev.x, prev.y, p.x, p.y, 1. + r * 0.06 * t, color);
                prev = p;
            }
        }
        1 => {
            draw_line(x, y - r, x, y + r, 2., color);
            for n in 0..5 {
                let yy = y - r * 0.65 + n as f32 * r * 0.33;
                let len = r * (0.45 + 0.1 * (n.min(4 - n)) as f32);
                draw_line(x, yy, x - len, yy - r * 0.3, 3., color);
                draw_line(x, yy, x + len, yy - r * 0.3, 3., color);
            }
        }
        2 => {
            for i in 0..7 {
                let yy = y - r * 0.75 + i as f32 * r * 0.25;
                let w = if i == 0 || i == 6 { r * 0.45 } else { r * 0.75 };
                draw_line(x - w, yy, x + w, yy, 3., color);
            }
            for side in [-0.2, 0.2] {
                draw_line(
                    x + r * side,
                    y - r * 0.8,
                    x + r * side,
                    y + r * 0.85,
                    2.,
                    hollow,
                );
            }
        }
        3 => {
            draw_triangle(
                vec2(x - r * 0.5, y - r * 0.9),
                vec2(x + r * 0.55, y - r * 0.6),
                vec2(x - r * 0.1, y + r),
                color,
            );
            for i in 0..5 {
                draw_line(
                    x + r * 0.3 - i as f32 * r * 0.08,
                    y - r * 0.35 + i as f32 * r * 0.2,
                    x + r * 0.55 - i as f32 * r * 0.08,
                    y - r * 0.35 + i as f32 * r * 0.2,
                    2.,
                    hollow,
                );
            }
        }
        4 => {
            bone(
                vec2(x - r * 0.9, y + r * 0.6),
                vec2(x - r * 0.2, y - r * 0.1),
                r * 0.08,
                color,
            );
            bone(
                vec2(x - r * 0.2, y - r * 0.1),
                vec2(x + r * 0.9, y - r * 0.7),
                r * 0.08,
                color,
            );
            for i in 0..3 {
                draw_line(
                    x - r * 0.2,
                    y - r * 0.1,
                    x + r * 0.2 + i as f32 * r * 0.2,
                    y + r * 0.6,
                    2.,
                    color,
                );
            }
        }
        _ => {
            bone(
                vec2(x - r * 0.5, y + r * 0.65),
                vec2(x + r * 0.5, y - r * 0.65),
                r * 0.25,
                color,
            );
            draw_circle(x - r * 0.45, y + r * 0.65, r * 0.08, hollow);
        }
    }
}
/// Specimen resting in a recessed display well.
fn displayed(kind: usize, x: f32, y: f32, r: f32, color: Color) {
    let well = Rect::new(x - r, y - r, r * 2., r * 2.);
    shapes::fill_circle(
        vec2(x, y),
        r,
        Fill::vertical(
            well,
            Color::new(0.08, 0.06, 0.045, 1.),
            Color::new(0.16, 0.12, 0.09, 1.),
        ),
    );
    let s = r * 0.62;
    specimen(
        kind,
        x + 1.5,
        y + 2.5,
        s,
        Color::new(0., 0., 0., 0.45),
        EARTH,
    );
    specimen(kind, x, y, s, color, Color::new(0.12, 0.09, 0.07, 1.));
}
/// Small hand-tool glyphs for the tool selector.
fn tool_icon(tool: usize, x: f32, y: f32, color: Color) {
    let handle = Color::new(0.55, 0.38, 0.22, 1.);
    match tool {
        0 => {
            draw_line(x - 8., y + 8., x + 2., y - 2., 3., handle);
            draw_triangle(
                vec2(x, y - 4.),
                vec2(x + 4., y),
                vec2(x + 10., y - 10.),
                color,
            );
            draw_line(x + 2., y - 2., x + 9., y - 9., 4., color);
        }
        1 => {
            draw_line(x - 8., y + 8., x, y, 4., handle);
            draw_line(x, y, x + 7., y - 7., 3., color);
            draw_line(x + 6., y - 9., x + 10., y - 5., 2., color);
        }
        _ => {
            rounded(x - 10., y - 5., 12., 10., 2., handle);
            draw_line(x + 2., y, x + 11., y, 3., color);
            for k in 0..3 {
                let xx = x + 4. + k as f32 * 3.;
                draw_line(xx, y - 2., xx + 2., y + 2., 1., darken(color, 0.4));
            }
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
    fn outcome(&self) -> Outcome {
        Game::outcome(self)
    }
    fn summary(&self) -> String {
        Game::summary(self)
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.pilot(tick)
    }
    fn hud(&self) -> Hud {
        Hud {
            stage: format!(
                "Permit {} of 6 · {}",
                self.site(),
                title_case(self.content().name)
            ),
            objective: format!(
                "Identify the {} and one other find",
                self.content().piece.to_lowercase()
            ),
            status: format!(
                "{} coins · {} supplies · day {}/3 · {} light · {} renown",
                self.coins, self.kits, self.day, self.daylight, self.renown
            ),
            score: self.score,
        }
    }
    fn draw(&self) {
        rounded_gradient(
            16.,
            112.,
            928.,
            456.,
            12.,
            Color::new(0.165, 0.12, 0.088, 1.),
            Color::new(0.11, 0.08, 0.06, 1.),
        );
        self.tabs();
        match self.page {
            _ if self.phase == Phase::Complete => {}
            Page::Dig => self.draw_dig(),
            Page::Lab => self.draw_lab(),
            Page::Camp => self.draw_camp(),
            Page::Museum => self.draw_museum(),
            Page::Atlas => self.draw_atlas(),
        }
        let alert = self.notice.contains("rejected")
            || self.notice.contains("destroyed")
            || self.notice.starts_with("Not enough")
            || self.notice.contains("cannot");
        let tone = if alert { RED } else { SAND };
        draw_circle(31., 557., 3., tone);
        label(
            self.notice,
            42.,
            561.,
            890.,
            13.,
            if alert { RED } else { DUST },
        );
        match self.phase {
            Phase::Briefing => self.draw_briefing(),
            Phase::Setback => self.draw_setback(),
            Phase::Complete => self.draw_museum(),
            Phase::Field | Phase::Lost => {}
        }
    }
}
impl Game {
    fn tabs(&self) {
        rounded(18., 114., 924., 32., 9., Color::new(0., 0., 0., 0.22));
        for (i, name) in ["Dig", "Lab", "Camp", "Museum", "Atlas"].iter().enumerate() {
            let x = 20. + i as f32 * 184.;
            let on = self.page.index() == i;
            if on {
                rounded_gradient(
                    x + 2.,
                    116.,
                    176.,
                    28.,
                    7.,
                    lighten(SAND, 0.12),
                    darken(SAND, 0.08),
                );
            }
            let badge = if on {
                Color::new(0.17, 0.12, 0.09, 0.22)
            } else {
                Color::new(1., 0.92, 0.8, 0.08)
            };
            rounded(x + 14., 121., 18., 18., 5., badge);
            say_center(
                &(i + 1).to_string(),
                x + 23.,
                135.,
                12.,
                if on { EARTH } else { DUST },
            );
            say(name, x + 42., 136., 16., if on { EARTH } else { BONE });
        }
    }
    fn draw_dig(&self) {
        let w = eyebrow("STRATA", 28., 167., DUST);
        label(
            self.content().strata,
            28. + w + 10.,
            168.,
            520. - w,
            15.,
            BONE,
        );
        let rgb = self.content().color;
        let soil = |shade: f32| {
            Color::new(
                f32::from(rgb[0]) / 255. * shade,
                f32::from(rgb[1]) / 255. * shade,
                f32::from(rgb[2]) / 255. * shade,
                1.,
            )
        };
        for (i, c) in self.cells.iter().enumerate() {
            let x = GRID_X + (i % COLS) as f32 * PITCH_X + 2.;
            let y = GRID_Y + (i / COLS) as f32 * PITCH_Y + 2.;
            let (tw, th) = (64., 56.);
            if c.depth == 0 && c.known {
                // Excavated pit: dark floor with a shaded upper lip.
                panel(x, y, tw, th, soil(0.3));
                panel(x, y, tw, 5., Color::new(0., 0., 0., 0.32));
                panel(x, y + th - 2., tw, 2., Color::new(1., 0.9, 0.7, 0.08));
            } else {
                // Surveyed ground shows its true strata color; unsurveyed
                // topsoil stays dusty and muted.
                let ground = if c.known {
                    soil(0.6 + f32::from(c.depth.min(9)) * 0.045)
                } else {
                    shapes::mix(soil(0.5), Color::new(0.25, 0.22, 0.19, 1.), 0.5)
                };
                panel(x, y, tw, th, ground);
                panel(x, y, tw, 2., Color::new(1., 0.95, 0.85, 0.16));
                panel(x, y + th - 3., tw, 3., Color::new(0., 0., 0., 0.2));
                if c.known {
                    // Exposed strata bands.
                    for k in 0..2 {
                        let yy = y + 20. + k as f32 * 16.;
                        draw_line(
                            x + 4.,
                            yy,
                            x + tw - 4. - ((i + k * 3) % 4) as f32 * 4.,
                            yy + (i % 3) as f32 - 1.,
                            2.,
                            Color::new(0.1, 0.07, 0.04, 0.2),
                        );
                    }
                } else {
                    // Unsurveyed topsoil: scattered pebbles.
                    for k in 0..3 {
                        let h = (i * 7 + k * 13) % 23;
                        let px = x + 8. + (h * 37 % 48) as f32;
                        let py = y + 10. + (h * 11 % 36) as f32;
                        panel(px, py, 3., 2., Color::new(0.1, 0.07, 0.04, 0.28));
                        panel(px, py - 1., 3., 1., Color::new(1., 0.95, 0.85, 0.12));
                    }
                }
            }
            let cx = x + tw * 0.5;
            let cy = y + th * 0.5 + 3.;
            if c.known {
                if c.depth > 0 {
                    draw_text_styled(&c.depth.to_string(), x + 6., y + 16., style(13., BONE));
                }
                if let Some(n) = c.find {
                    let kind = self.content().finds[n].kind;
                    if c.taken {
                        specimen(kind, cx, cy, 14., Color::new(1., 1., 1., 0.1), EARTH);
                        say_center("✓", cx, cy + 6., 18., GREEN);
                    } else if c.quality == 0 {
                        specimen(kind, cx, cy, 15., RED, soil(0.3));
                    } else if c.depth == 0 {
                        specimen(
                            kind,
                            cx + 1.,
                            cy + 2.,
                            15.,
                            Color::new(0., 0., 0., 0.45),
                            EARTH,
                        );
                        specimen(kind, cx, cy, 15., BONE, soil(0.3));
                    } else {
                        specimen(kind, cx + 4., cy, 14., with_alpha(BONE, 0.42), soil(0.5));
                    }
                }
            }
        }
        // Cursor.
        let cx = GRID_X + (self.cursor % COLS) as f32 * PITCH_X;
        let cy = GRID_Y + (self.cursor / COLS) as f32 * PITCH_Y;
        outline(cx, cy, 68., 60., 6., 2.5, GOLD);
        self.dig_panel();
    }
    fn dig_panel(&self) {
        let (x0, x1) = (610., 916.);
        slab(594., 152., 338., 388.);
        // Expedition resources.
        let light_low = self.daylight < 4;
        let stats = [
            ("SUPPLIES", self.kits.to_string(), BONE),
            (
                "LIGHT",
                self.daylight.to_string(),
                if light_low { RED } else { BONE },
            ),
            ("DAY", format!("{}/3", self.day), BONE),
            ("COINS", self.coins.to_string(), GOLD),
        ];
        for (n, (name, value, color)) in stats.iter().enumerate() {
            let x = x0 + [0., 94., 162., 228.][n];
            eyebrow(name, x, 174., DUST);
            say(value, x, 197., 19., *color);
        }
        divider(x0, 210., x1 - x0, HAIRLINE);
        // Selected tile.
        let c = self.cells[self.cursor];
        let w = eyebrow("TILE", x0, 232., DUST);
        eyebrow(
            &format!("{}-{}", self.cursor % COLS + 1, self.cursor / COLS + 1),
            x0 + w + 6.,
            232.,
            GOLD,
        );
        let trace = c.known && c.find.is_some();
        let (state, tone) = if c.taken {
            ("In the lab", GREEN)
        } else if trace && c.quality == 0 {
            ("Destroyed", RED)
        } else if trace && c.depth == 0 {
            ("Exposed", GOLD)
        } else if trace {
            ("Fossil trace", GREEN)
        } else if c.known && c.depth == 0 {
            ("Cleared", DUST)
        } else if c.known {
            ("No trace", DUST)
        } else {
            ("Unsurveyed", DUST)
        };
        chip(state, x1 - chip_width(state, 12.), 219., 12., tone);
        let advice = if c.taken {
            "Jacketed and safe in the specimen tray."
        } else if trace && c.depth == 0 && c.quality > 0 {
            "Space jackets it for the lab."
        } else if trace {
            "Switch to the brush before the last layer."
        } else if c.known {
            "Nothing here. Survey elsewhere."
        } else {
            "Survey reveals this tile and its neighbors."
        };
        label(advice, x0, 254., x1 - x0, 14., BONE);
        let cols = [
            (
                "DEPTH",
                if c.known {
                    c.depth.to_string()
                } else {
                    "?".to_owned()
                },
            ),
            ("HARDNESS", c.hard.to_string()),
            (
                "CONDITION",
                if trace {
                    format!("{}%", c.quality)
                } else {
                    "-".to_owned()
                },
            ),
        ];
        for (n, (name, value)) in cols.iter().enumerate() {
            let x = x0 + n as f32 * 104.;
            eyebrow(name, x, 280., DUST);
            say(value, x, 302., 18., BONE);
        }
        if trace {
            let q = f32::from(c.quality) / 100.;
            meter(
                x0 + 208.,
                309.,
                98.,
                5.,
                q,
                if q < 0.5 { RED } else { GREEN },
            );
        }
        say("Survey: 1 kit · 2 light", x0, 334., 13., DUST);
        let mut kx = x0;
        for (key, action) in [("Space", "Dig"), ("X", "Survey"), ("C", "Tool")] {
            kx += keycap(key, action, kx, 350., BONE) + 16.;
        }
        // Tool selector (shares the model's click rows at y 390 + 45n).
        for (i, (name, note)) in [
            ("Brush", "Safe near bone"),
            ("Chisel", "Quick in soft rock"),
            ("Drill", "Deep, risks bone"),
        ]
        .iter()
        .enumerate()
        {
            let y = 390. + i as f32 * 45.;
            let on = self.tool == i;
            row_surface(614., y, 304., 40., on);
            tool_icon(i, 636., y + 20., if on { BONE } else { DUST });
            say(name, 656., y + 18., 15., if on { BONE } else { SAND });
            say(note, 656., y + 33., 12., DUST);
            let mut probe = self.clone();
            probe.tool = i;
            let (power, kits, time) = probe.tool_stats(self.cursor);
            say_right(
                &format!("{} layer{}", power, if power == 1 { "" } else { "s" }),
                906.,
                y + 18.,
                14.,
                if on { GOLD } else { SAND },
            );
            say_right(
                &format!(
                    "{kits} kit{} · {time} light",
                    if kits == 1 { "" } else { "s" }
                ),
                906.,
                y + 33.,
                12.,
                DUST,
            );
        }
    }
    fn draw_lab(&self) {
        eyebrow("SPECIMEN TRAY", 30., 176., DUST);
        for n in 0..4 {
            let y = 190. + n as f32 * 70.;
            let on = n == self.sample;
            row_surface(30., y, 280., 62., on);
            if let Some(s) = self.samples[n] {
                displayed(s.kind, 62., y + 31., 23., BONE);
                let name = if s.identified {
                    title_case(SPECIES[s.kind].name)
                } else {
                    "Unidentified".to_owned()
                };
                label(&name, 96., y + 25., 170., 15., BONE);
                if s.identified {
                    say_right("✓", 298., y + 25., 15., GREEN);
                }
                say(
                    &format!("Condition {}%", s.quality),
                    96.,
                    y + 43.,
                    12.,
                    DUST,
                );
                let q = f32::from(s.quality) / 100.;
                meter(212., y + 38., 80., 5., q, if q < 0.5 { RED } else { GREEN });
            } else {
                draw_circle_lines(62., y + 31., 22., 1., Color::new(1., 0.9, 0.75, 0.18));
                say_center(&(n + 1).to_string(), 62., y + 36., 14., DUST);
                say("Empty slot", 96., y + 36., 14., DUST);
            }
        }
        let mut kx = 30.;
        for key in ["↑", "↓"] {
            kx += keycap(key, "", kx, 482., BONE) + 6.;
        }
        say("Choose a sample", kx + 4., 496., 13., DUST);
        say("Finds stay safe in the tray.", 30., 526., 12., DUST);
        // Evidence.
        slab(332., 154., 586., 250.);
        eyebrow("ANATOMICAL EVIDENCE", 350., 180., DUST);
        if let Some(s) = self.samples[self.sample] {
            displayed(s.kind, 420., 280., 64., BONE);
            paragraph(SPECIES[s.kind].clue, 506., 230., 392., 18., BONE);
            eyebrow("RESEARCH GRANT", 506., 318., DUST);
            say(
                &format!(
                    "{} coins",
                    SPECIES[s.kind].value * u32::from(s.quality) / 100
                ),
                506.,
                342.,
                19.,
                GOLD,
            );
            let (msg, tone) = if s.identified {
                ("Catalogued. Funding already awarded.", GREEN)
            } else {
                ("Compare both traits with the Atlas (5).", DUST)
            };
            label(msg, 506., 378., 392., 14., tone);
        } else {
            draw_circle_lines(420., 280., 62., 1., Color::new(1., 0.9, 0.75, 0.18));
            paragraph(
                "Expose a fossil in the dig, then jacket it with Space.",
                506.,
                262.,
                392.,
                17.,
                DUST,
            );
        }
        // Species choice (click cycles).
        rounded(332., 425., 586., 40., 10., Color::new(0., 0., 0., 0.25));
        outline(332., 425., 586., 40., 10., 1., HAIRLINE);
        keycap("←", "", 344., 435., BONE);
        keycap("→", "", 886., 435., BONE);
        eyebrow("SPECIES", 380., 450., DUST);
        specimen(self.choice, 476., 445., 11., SAND, EARTH);
        say(
            &title_case(SPECIES[self.choice].name),
            496.,
            451.,
            18.,
            BONE,
        );
        // Identify.
        let ready = self.samples[self.sample].is_some_and(|s| !s.identified);
        if ready {
            rounded_gradient(
                332.,
                480.,
                586.,
                42.,
                10.,
                Color::new(0.52, 0.78, 0.58, 1.),
                Color::new(0.38, 0.62, 0.44, 1.),
            );
            outline(332., 480., 586., 42., 10., 1., Color::new(1., 1., 1., 0.25));
        } else {
            rounded(332., 480., 586., 42., 10., Color::new(1., 0.92, 0.8, 0.06));
        }
        let ink_color = if ready { EARTH } else { DUST };
        say("Identify specimen", 350., 507., 17., ink_color);
        say_right("1 kit · 2 light", 836., 506., 13., ink_color);
        keycap("Space", "", 852., 491., BONE);
    }
    fn draw_camp(&self) {
        eyebrow("EXPEDITION CAMP", 50., 180., DUST);
        let actions = [
            (
                "Rest until morning".to_owned(),
                format!("12 coins · day {}/3", self.day),
            ),
            (
                "Buy 12 supplies".to_owned(),
                format!("10 coins · {} packs left", self.packs),
            ),
            (
                format!("Precision brush {}/2", self.upgrades[0]),
                format!(
                    "{} coins · needs {} renown",
                    self.upgrade_cost(0),
                    3 + u16::from(self.upgrades[0]) * 3
                ),
            ),
            (
                format!("Drill motor {}/2", self.upgrades[1]),
                format!(
                    "{} coins · needs {} renown",
                    self.upgrade_cost(1),
                    3 + u16::from(self.upgrades[1]) * 3
                ),
            ),
            (
                if self.index == 5 {
                    "Open the museum".to_owned()
                } else {
                    "Open next site".to_owned()
                },
                if self.ready() {
                    "Ready · 30 coin permit grant".to_owned()
                } else {
                    "Needs the signature part and another find".to_owned()
                },
            ),
            (
                "Recover this permit".to_owned(),
                format!("{} recovery grants remain", self.retries),
            ),
        ];
        for (n, (title, detail)) in actions.iter().enumerate() {
            let y = 194. + n as f32 * 56.;
            let on = self.row == n;
            row_surface(50., y, 516., 50., on);
            let go = n == 4 && self.ready();
            if go {
                rounded(50., y + 8., 3., 34., 1.5, GREEN);
            }
            say(title, 66., y + 22., 16., if go { GREEN } else { BONE });
            label(detail, 66., y + 41., 400., 13., DUST);
            if on {
                keycap("Space", "", 506., y + 15., BONE);
            }
        }
        slab(592., 154., 332., 382.);
        eyebrow("PERMIT LEDGER", 610., 180., DUST);
        let rows = [
            ("Coins", self.coins.to_string()),
            ("Supplies", self.kits.to_string()),
            ("Research renown", self.renown.to_string()),
            (
                "Daylight",
                format!("{}/{}", self.daylight, self.content().daylight),
            ),
            ("Museum pieces", format!("{}/6", self.museum_pieces())),
        ];
        for (i, (key, value)) in rows.iter().enumerate() {
            let y = 214. + i as f32 * 34.;
            say(key, 610., y, 15., DUST);
            say_right(value, 906., y, 17., if i == 0 { GOLD } else { BONE });
            if i < rows.len() - 1 {
                panel(610., y + 12., 296., 1., HAIRLINE);
            }
        }
        meter(
            610.,
            366.,
            296.,
            5.,
            f32::from(self.daylight) / f32::from(self.content().daylight.max(1)),
            SAND,
        );
        divider(610., 392., 296., HAIRLINE);
        eyebrow("UPGRADES", 610., 416., DUST);
        paragraph(
            "Precision brushes protect bone. Drill motors cut deeper.",
            610.,
            440.,
            294.,
            14.,
            BONE,
        );
        paragraph(
            "Recovering resets this site to its arrival state.",
            610.,
            496.,
            294.,
            13.,
            DUST,
        );
    }
    fn draw_museum(&self) {
        let (x, y, w, h) = (24., 152., 912., 390.);
        card(x, y, w, h, Color::new(0.15, 0.165, 0.14, 1.));
        // Gallery lighting: a soft pool behind the skeleton and a polished floor.
        // Gallery wall: a lit upper band and quiet vertical panelling.
        let wall = Rect::new(x + 1., y + 1., w - 2., 230.);
        shapes::fill_rounded_rect(
            wall,
            0.,
            Fill::vertical(
                wall,
                Color::new(1., 0.9, 0.7, 0.06),
                Color::new(1., 0.9, 0.7, 0.),
            ),
        );
        for k in 1..8 {
            panel(
                x + k as f32 * 114.,
                y + 52.,
                1.,
                210.,
                Color::new(0., 0., 0., 0.12),
            );
        }
        let floor = Rect::new(x + 1., 418., w - 2., 12.);
        shapes::fill_rounded_rect(
            floor,
            0.,
            Fill::vertical(
                floor,
                Color::new(0., 0., 0., 0.25),
                Color::new(0., 0., 0., 0.),
            ),
        );
        say("The Dawn Giant", 44., 186., 25., BONE);
        let open = self.phase == Phase::Complete;
        let tag = if open {
            "The Basin Museum is open".to_owned()
        } else {
            format!("{} / 6 signature parts", self.museum_pieces())
        };
        chip(
            &tag,
            916. - chip_width(&tag, 13.),
            170.,
            13.,
            if open { GREEN } else { GOLD },
        );
        // Each accepted signature specimen adds one anatomical section.
        let tones = core::array::from_fn::<_, 6, _>(|i| {
            if self.museum[i * 4].identified {
                BONE
            } else {
                Color::new(0.27, 0.29, 0.24, 1.)
            }
        });
        // Plinth and armature.
        rounded(90., 412., 770., 8., 3., Color::new(0.42, 0.34, 0.24, 1.));
        panel(90., 412., 770., 2., Color::new(1., 0.92, 0.75, 0.18));
        rounded(140., 420., 670., 6., 3., Color::new(0., 0., 0., 0.3));
        for x in [240., 400., 620.] {
            draw_line(x, 300., x, 412., 2., Color::new(0.48, 0.41, 0.28, 1.));
        }
        // Skull, rising neck, ribs, pelvis, paired legs and tapering tail.
        draw_circle(196., 234., 22., tones[0]);
        rounded(150., 226., 52., 24., 9., tones[0]);
        draw_circle(186., 228., 5., Color::new(0.1, 0.08, 0.06, 1.));
        draw_line(156., 243., 198., 245., 2., Color::new(0.1, 0.08, 0.06, 0.8));
        for n in 0..7 {
            let t = n as f32 / 6.;
            bone(
                vec2(220. + t * 142., 245. + t * 71.),
                vec2(231. + t * 142., 250. + t * 71.),
                6.,
                tones[1],
            );
        }
        bone(vec2(362., 316.), vec2(530., 312.), 8., tones[2]);
        for n in 0..7 {
            let x = 370. + n as f32 * 21.;
            draw_line(x, 318., x + 4., 352., 5., tones[2]);
            draw_line(x + 4., 352., x + 16., 362., 4., tones[2]);
        }
        bone(vec2(535., 312.), vec2(568., 339.), 14., tones[3]);
        for (x, yy) in [(386., 352.), (535., 340.)] {
            bone(vec2(x, yy), vec2(x - 14., 382.), 7., tones[4]);
            bone(vec2(x - 14., 382.), vec2(x - 3., 410.), 5., tones[4]);
            draw_line(x - 10., 410., x + 10., 410., 6., tones[4]);
        }
        for n in 0..10 {
            let t = n as f32 / 10.;
            bone(
                vec2(571. + t * 244., 318. - t * 40.),
                vec2(590. + t * 244., 315. - t * 40.),
                6. - t * 4.,
                tones[5],
            );
        }
        // Site placards.
        for (i, site) in SITES.iter().enumerate() {
            let x = 38. + i as f32 * 150.;
            let on = self.choice == i;
            row_surface(x, 438., 142., 78., on);
            if !on {
                outline(x, 438., 142., 78., 8., 1., HAIRLINE);
            }
            let done = self.museum[i * 4].identified;
            label(&title_case(site.piece), x + 12., 460., 100., 15., BONE);
            if done {
                say_right("✓", x + 130., 460., 15., GREEN);
            }
            let finds = self.museum[i * 4..i * 4 + 4]
                .iter()
                .filter(|r| r.identified)
                .count();
            say(&format!("{finds} of 4 finds"), x + 12., 482., 12., DUST);
            stars(self.medals[i], x + 12., 506., 13.);
        }
    }
    fn draw_atlas(&self) {
        eyebrow("BASIN FIELD ATLAS", 42., 180., DUST);
        let mut kx = 846.;
        for key in ["←", "→"] {
            kx += keycap(key, "", kx, 166., BONE) + 6.;
        }
        say_right("Browse", 838., 180., 13., DUST);
        for (i, species) in SPECIES.iter().enumerate() {
            let x = 38. + (i % 3) as f32 * 304.;
            let y = 198. + (i / 3) as f32 * 167.;
            let on = self.choice == i;
            if on {
                row_surface(x, y, 290., 152., true);
            } else {
                slab(x, y, 290., 152.);
            }
            displayed(i, x + 46., y + 46., 32., BONE);
            label(&title_case(species.name), x + 90., y + 38., 190., 17., BONE);
            say(
                &format!("{} coin grant", species.value),
                x + 90.,
                y + 60.,
                13.,
                GOLD,
            );
            divider(x + 14., y + 88., 262., HAIRLINE);
            paragraph(species.clue, x + 14., y + 112., 262., 14., DUST);
        }
    }
    /// Dim the field and raise a centered dialog card.
    fn dialog(&self, x: f32, y: f32, w: f32, h: f32, accent: Color) {
        panel(16., 112., 928., 456., Color::new(0.04, 0.03, 0.02, 0.62));
        card(x, y, w, h, Color::new(0.16, 0.12, 0.09, 1.));
        rounded(x + 26., y, 64., 3., 1.5, accent);
    }
    fn draw_briefing(&self) {
        let (x, y, w, h) = (110., 160., 740., 368.);
        self.dialog(x, y, w, h, GOLD);
        let l = x + 28.;
        eyebrow(
            &format!("FIELD PERMIT {} OF 6", self.site()),
            l,
            y + 34.,
            GOLD,
        );
        say(&title_case(self.content().name), l, y + 70., 30., BONE);
        let next = paragraph(self.content().story, l, y + 104., w - 56., 17., BONE);
        let facts = [
            "4 specimens".to_owned(),
            "3 days".to_owned(),
            format!("{} light a day", self.content().daylight),
            format!("Rock hardness {}", self.content().rock),
        ];
        let mut cx = l;
        for fact in &facts {
            cx += chip(fact, cx, next + 6., 13., SAND) + 8.;
        }
        let n = self.samples.iter().flatten().count();
        let w2 = eyebrow("SIGNATURE FIND", l, next + 62., DUST);
        say(
            &title_case(self.content().piece),
            l + w2 + 10.,
            next + 63.,
            16.,
            GOLD,
        );
        say_right(
            &format!("Tray ready {n} / 4"),
            x + w - 28.,
            next + 63.,
            14.,
            DUST,
        );
        divider(l, next + 82., w - 56., HAIRLINE);
        let mut kx = l;
        for (key, action) in [("X", "Survey"), ("Space", "Dig"), ("C", "Change tool")] {
            kx += keycap(key, action, kx, next + 96., BONE) + 18.;
        }
        say("Brush near a fossil.", kx, next + 111., 13., DUST);
        // Start button.
        let (bx, by, bw, bh) = (l, y + h - 66., w - 56., 44.);
        rounded_gradient(bx, by, bw, bh, 11., lighten(GOLD, 0.12), darken(GOLD, 0.1));
        outline(bx, by, bw, bh, 11., 1., Color::new(1., 1., 1., 0.25));
        say("Start fieldwork", bx + 18., by + 28., 18., EARTH);
        say_right(
            "Space or click",
            bx + bw - 18.,
            by + 27.,
            13.,
            darken(EARTH, 0.1),
        );
    }
    fn draw_setback(&self) {
        let (x, y, w, h) = (150., 196., 660., 296.);
        self.dialog(x, y, w, h, RED);
        let l = x + 28.;
        eyebrow("PERMIT AT RISK", l, y + 34., RED);
        say("The dig cannot be completed", l, y + 70., 26., BONE);
        paragraph(
            "A required fossil was lost or supplies ran out. Recovering resets this site to its arrival state.",
            l,
            y + 106.,
            w - 56.,
            17.,
            DUST,
        );
        eyebrow("RECOVERY GRANTS", l, y + 172., DUST);
        say(&self.retries.to_string(), l, y + 202., 26., GOLD);
        let (bx, by, bw, bh) = (l, y + h - 66., w - 56., 44.);
        let label_text = if self.retries > 0 {
            "Recover this permit"
        } else {
            "Close the expedition"
        };
        rounded_gradient(bx, by, bw, bh, 11., lighten(RED, 0.1), darken(RED, 0.15));
        outline(bx, by, bw, bh, 11., 1., Color::new(1., 1., 1., 0.25));
        say(
            label_text,
            bx + 18.,
            by + 28.,
            18.,
            Color::new(0.12, 0.05, 0.03, 1.),
        );
        keycap("Space", "", bx + bw - 66., by + 12., PAPER);
    }
}
