//! Read-only field notebook and museum presentation.
//!
//! Drawing reads expedition state plus the host's presentation clock and
//! pointer, which only animate and highlight. The model's per-tick events
//! become effects in [`ArcadeGame::effects`]; nothing here feeds the rules.
use crate::content::{SITES, SPECIES};
use crate::model::{
    CELLS, COLS, DIG_BUTTON, Event, GRID_X, GRID_Y, Game, Lack, PITCH_X, PITCH_Y, Page, Phase,
    RECOVER_BUTTON, SPECIES_BAR, SURVEY_BUTTON, Spot, atlas_card, inside, placard,
};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;
use std::borrow::Cow;

const EARTH: Color = Color::new(0.17, 0.12, 0.09, 1.);
const CARD: Color = Color::new(0.215, 0.165, 0.12, 1.);
const SAND: Color = Color::new(0.83, 0.65, 0.40, 1.);
const BONE: Color = Color::new(0.97, 0.90, 0.71, 1.);
const GOLD: Color = Color::new(1., 0.70, 0.29, 1.);
const GREEN: Color = Color::new(0.50, 0.76, 0.57, 1.);
const RED: Color = Color::new(0.95, 0.48, 0.35, 1.);
const TEAL: Color = Color::new(0.47, 0.80, 0.82, 1.);
const SUN: Color = Color::new(1., 0.82, 0.40, 1.);
/// Secondary marks on warm surfaces.
const DUST: Color = Color::new(0.72, 0.62, 0.5, 1.);
/// Spent, locked or missing things.
const DIM: Color = Color::new(0.45, 0.39, 0.32, 1.);
const HAIRLINE: Color = Color::new(1., 0.9, 0.75, 0.12);
/// A section of the Giant that is not in the museum yet.
const GHOST: Color = Color::new(0.42, 0.38, 0.32, 0.45);
/// Brush, chisel and drill: safe, risky and rough near bone.
const TOOL_TONE: [Color; 3] = [GREEN, GOLD, RED];
const TOOL_NAME: [&str; 3] = ["Brush", "Chisel", "Drill"];
/// Small numbers without per-frame allocation.
const DIGITS: [&str; 13] = [
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12",
];
/// Vertical centre of the resource strip under the tabs.
const STRIP_Y: f32 = 163.;
/// Left edge of each resource: coins, supplies, daylight, day, renown, grants.
const STAT_X: [f32; 6] = [40., 128., 214., 392., 476., 556.];
/// The Lab's research button (the model's click band).
const IDENTIFY: [f32; 4] = [332., 480., 586., 42.];
/// The Lab's specimen well and the candidate species beside it.
const WELL: Vec2 = vec2(410., 292.);
const CANDIDATE: Vec2 = vec2(840., 292.);
/// Approximate centre of each Giant section in museum coordinates.
const PART_AT: [(f32, f32); 6] = [
    (185., 236.),
    (292., 282.),
    (446., 338.),
    (552., 326.),
    (462., 382.),
    (705., 296.),
];

fn ink(size: f32, color: Color) -> TextStyle {
    TextStyle::modern(size, color)
}
/// Text vertically centred on `cy`; returns its width.
fn say(value: &str, x: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = ink(size, color);
    draw_text_styled(value, x, s.centered_baseline(cy), s).width
}
fn say_center(value: &str, cx: f32, cy: f32, size: f32, color: Color) {
    let s = ink(size, color).with_align(TextAlign::Center);
    draw_text_styled(value, cx, s.centered_baseline(cy), s);
}
/// Like [`say`], shrunk to `width` if needed and never enlarged.
fn label(value: &str, x: f32, cy: f32, width: f32, size: f32, color: Color) -> f32 {
    let w = text_width(value, ink(size, color));
    say(
        value,
        x,
        cy,
        if w > width { size * width / w } else { size },
        color,
    )
}
fn number(n: u32) -> Cow<'static, str> {
    DIGITS
        .get(n as usize)
        .map_or_else(|| Cow::Owned(n.to_string()), |d| Cow::Borrowed(*d))
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
/// A species' two anatomical traits, without full stops.
fn traits(kind: usize) -> impl Iterator<Item = &'static str> {
    SPECIES[kind]
        .clue
        .split(". ")
        .map(|t| t.trim_end_matches('.'))
}
/// Interface card in the expedition's warm palette.
fn slab(x: f32, y: f32, w: f32, h: f32) {
    card(x, y, w, h, CARD);
}
/// Selectable surface: raised when chosen, lifted on hover, quiet otherwise.
fn row_surface(x: f32, y: f32, w: f32, h: f32, selected: bool, hover: bool) {
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
    } else if hover {
        rounded(x, y, w, h, 8., Color::new(1., 0.92, 0.8, 0.1));
        outline(x, y, w, h, 8., 1., with_alpha(SAND, 0.5));
    } else {
        rounded(x, y, w, h, 8., Color::new(1., 0.92, 0.8, 0.045));
    }
}
fn quality_tone(quality: u8) -> Color {
    match quality {
        0..=49 => RED,
        50..=79 => GOLD,
        _ => GREEN,
    }
}
/// An icon and short value ending at `right`, centred on `cy`; returns the
/// right edge for the next item to its left.
fn stat_left(glyph: Icon, value: &str, right: f32, cy: f32, size: f32, color: Color) -> f32 {
    let s = style(size, color);
    let text = if value.is_empty() {
        0.
    } else {
        text_width(value, s) + size * 0.25
    };
    let left = right - size - text;
    icon(glyph, left + size * 0.5, cy, size, color);
    if !value.is_empty() {
        draw_text_styled(value, left + size * 1.25, s.centered_baseline(cy), s);
    }
    left - size * 0.75
}
/// Corner brackets around a rectangle (a survey's reach).
fn brackets(x: f32, y: f32, w: f32, h: f32, color: Color) {
    let l = 11.;
    for (cx, cy, dx, dy) in [
        (x, y, 1., 1.),
        (x + w, y, -1., 1.),
        (x, y + h, 1., -1.),
        (x + w, y + h, -1., -1.),
    ] {
        draw_line(cx, cy, cx + l * dx, cy, 2., color);
        draw_line(cx, cy, cx, cy + l * dy, 2., color);
    }
}
/// A four-point glint of radius `r`.
fn glint(at: Vec2, r: f32, color: Color) {
    if r < 0.6 {
        return;
    }
    let thin = r * 0.28;
    shapes::fill_polygon(
        &[
            at - vec2(r, 0.),
            at - vec2(0., thin),
            at + vec2(r, 0.),
            at + vec2(0., thin),
        ],
        color,
    );
    shapes::fill_polygon(
        &[
            at - vec2(0., r),
            at + vec2(thin, 0.),
            at + vec2(0., r),
            at - vec2(thin, 0.),
        ],
        color,
    );
}
/// Hardness as three diamonds, `level` of them filled.
fn hardness(x: f32, cy: f32, level: u8, color: Color) {
    for k in 0..3_u8 {
        let at = vec2(x + 5. + f32::from(k) * 12., cy);
        let r = 4.5;
        let points = [
            at - vec2(0., r),
            at + vec2(r, 0.),
            at + vec2(0., r),
            at - vec2(r, 0.),
        ];
        if k < level {
            shapes::fill_polygon(&points, color);
        } else {
            shapes::polyline(
                &points,
                shapes::Stroke::new(1.2, with_alpha(color, 0.45)).closed(true),
            );
        }
    }
}
fn tile_origin(i: usize) -> Vec2 {
    vec2(
        GRID_X + (i % COLS) as f32 * PITCH_X,
        GRID_Y + (i / COLS) as f32 * PITCH_Y,
    )
}
fn tile_center(i: usize) -> Vec2 {
    tile_origin(i) + vec2(PITCH_X * 0.5, PITCH_Y * 0.5)
}
fn tab_center(i: usize) -> Vec2 {
    vec2(112. + i as f32 * 184., 130.)
}
fn center(r: [f32; 4]) -> Vec2 {
    vec2(r[0] + r[2] * 0.5, r[1] + r[3] * 0.5)
}
/// Tool button `i` (the model's 45-unit click bands).
fn tool_row(i: usize) -> [f32; 4] {
    [614., 390. + i as f32 * 45., 304., 40.]
}
/// Tray slot `n` (the model's 70-unit click bands).
fn tray_row(n: usize) -> [f32; 4] {
    [30., 190. + n as f32 * 70., 280., 62.]
}
/// Camp action `n` (the model's 56-unit click bands).
fn camp_row(n: usize) -> [f32; 4] {
    [50., 194. + n as f32 * 56., 516., 50.]
}
fn species_chip(i: usize) -> [f32; 4] {
    let [x, y, w, h] = SPECIES_BAR;
    let cw = w / 6.;
    [x + i as f32 * cw + 2., y, cw - 4., h]
}
/// Where each strip value sits: coins, supplies, daylight, day, renown, grants.
fn strip_at(k: usize) -> Vec2 {
    vec2(STAT_X[k] + [30., 30., 156., 30., 26., 26.][k], STRIP_Y)
}
fn hovered(r: [f32; 4]) -> bool {
    inside(pointer(), r)
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
/// Brush, chisel and drill glyphs (`s` 1 is about 20 units).
fn tool_glyph(tool: usize, x: f32, y: f32, s: f32, color: Color) {
    let handle = Color::new(0.55, 0.38, 0.22, color.a);
    let p = |dx: f32, dy: f32| vec2(x + dx * s, y + dy * s);
    let line = |a: Vec2, b: Vec2, w: f32, c: Color| draw_line(a.x, a.y, b.x, b.y, w * s, c);
    match tool {
        0 => {
            line(p(-8., 8.), p(2., -2.), 3., handle);
            draw_triangle(p(0., -4.), p(4., 0.), p(10., -10.), color);
            line(p(2., -2.), p(9., -9.), 4., color);
        }
        1 => {
            line(p(-8., 8.), p(0., 0.), 4., handle);
            line(p(0., 0.), p(7., -7.), 3., color);
            line(p(6., -9.), p(10., -5.), 2., color);
        }
        _ => {
            rounded(x - 10. * s, y - 5. * s, 12. * s, 10. * s, 2. * s, handle);
            line(p(2., 0.), p(11., 0.), 3., color);
            for k in 0..3 {
                let xx = 4. + k as f32 * 3.;
                line(p(xx, -2.), p(xx + 2., 2.), 1., darken(color, 0.4));
            }
        }
    }
}
/// The Dawn Giant's six sections (skull, neck, ribs, pelvis, legs, tail),
/// authored around (150, 212) at museum scale and drawn there at `at`,
/// scaled by `s`.
fn skeleton(at: Vec2, s: f32, tones: [Color; 6]) {
    let p = |x: f32, y: f32| at + (vec2(x, y) - vec2(150., 212.)) * s;
    let line = |a: Vec2, b: Vec2, w: f32, c: Color| {
        draw_line(a.x, a.y, b.x, b.y, (w * s).max(1.), c);
    };
    // Skull.
    let head = p(196., 234.);
    draw_circle(head.x, head.y, 22. * s, tones[0]);
    let jaw = p(150., 226.);
    rounded(jaw.x, jaw.y, 52. * s, 24. * s, 9. * s, tones[0]);
    if s > 0.3 {
        let dark = Color::new(0.1, 0.08, 0.06, tones[0].a);
        let eye = p(186., 228.);
        draw_circle(eye.x, eye.y, 5. * s, dark);
        line(
            p(156., 243.),
            p(198., 245.),
            2.,
            with_alpha(dark, 0.8 * dark.a),
        );
    }
    // Rising neck.
    for n in 0..7 {
        let t = n as f32 / 6.;
        bone(
            p(220. + t * 142., 245. + t * 71.),
            p(231. + t * 142., 250. + t * 71.),
            6. * s,
            tones[1],
        );
    }
    // Rib cage.
    bone(p(362., 316.), p(530., 312.), 8. * s, tones[2]);
    for n in 0..7 {
        let x = 370. + n as f32 * 21.;
        line(p(x, 318.), p(x + 4., 352.), 5., tones[2]);
        line(p(x + 4., 352.), p(x + 16., 362.), 4., tones[2]);
    }
    // Pelvis.
    bone(p(535., 312.), p(568., 339.), 14. * s, tones[3]);
    // Paired legs.
    for (x, y) in [(386., 352.), (535., 340.)] {
        bone(p(x, y), p(x - 14., 382.), 7. * s, tones[4]);
        bone(p(x - 14., 382.), p(x - 3., 410.), 5. * s, tones[4]);
        line(p(x - 10., 410.), p(x + 10., 410.), 6., tones[4]);
    }
    // Tapering tail.
    for n in 0..10 {
        let t = n as f32 / 10.;
        bone(
            p(571. + t * 244., 318. - t * 40.),
            p(590. + t * 244., 315. - t * 40.),
            (6. - t * 4.) * s,
            tones[5],
        );
    }
}
fn part_at(at: Vec2, s: f32, part: usize) -> Vec2 {
    let (x, y) = PART_AT[part];
    at + (vec2(x, y) - vec2(150., 212.)) * s
}
/// The missing resource behind a refusal, as an icon and one or two words.
fn lack_badge(lack: Lack) -> (Icon, Cow<'static, str>) {
    match lack {
        Lack::Supplies(n) => (Icon::Crate, format!("Need {n}").into()),
        Lack::Daylight(n) => (Icon::Sun, format!("Need {n}").into()),
        Lack::Coins(n) => (Icon::Coin, format!("Need {n}").into()),
        Lack::Renown(n) => (Icon::Crown, format!("Need {n}").into()),
        Lack::Stock => (Icon::Crate, "Sold out".into()),
        Lack::Storage => (Icon::Crate, "Full".into()),
        Lack::Maxed => (Icon::Check, "Max".into()),
        Lack::Days => (Icon::Calendar, "Last day".into()),
        Lack::Morning => (Icon::Sun, "Full".into()),
        Lack::Ready => (Icon::Bone, "Not ready".into()),
        Lack::Nothing => (Icon::Cross, "".into()),
    }
}
/// The strip resource a refusal points at, if any.
fn lack_stat(lack: Lack) -> Option<usize> {
    match lack {
        Lack::Coins(_) => Some(0),
        Lack::Supplies(_) | Lack::Storage | Lack::Stock => Some(1),
        Lack::Daylight(_) | Lack::Morning => Some(2),
        Lack::Days => Some(3),
        Lack::Renown(_) => Some(4),
        _ => None,
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
            stage: title_case(self.content().name),
            objective: self.objective_text(),
            status: format!(
                "{} coins, {} supplies, {} light, day {} of 3, {} renown, {} recovery grants",
                self.coins, self.kits, self.daylight, self.day, self.renown, self.retries
            ),
            score: self.score,
        }
    }
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.site() as u32, SITES.len() as u32))
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        for event in self.feed.iter() {
            self.effect(event, fx);
        }
    }
    fn draw_objective(&self, area: Rect) -> bool {
        self.objective(area);
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        self.result(area);
        true
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
        if self.phase == Phase::Complete {
            self.draw_museum();
            return;
        }
        self.strip();
        match self.page {
            Page::Dig => self.draw_dig(),
            Page::Lab => self.draw_lab(),
            Page::Camp => self.draw_camp(),
            Page::Museum => self.draw_museum(),
            Page::Atlas => self.draw_atlas(),
        }
        match self.phase {
            Phase::Briefing => self.draw_briefing(),
            Phase::Setback => self.draw_setback(),
            Phase::Field | Phase::Complete | Phase::Lost => {}
        }
    }
}
impl Game {
    fn soil(&self, shade: f32) -> Color {
        let rgb = self.content().color;
        Color::new(
            f32::from(rgb[0]) / 255. * shade,
            f32::from(rgb[1]) / 255. * shade,
            f32::from(rgb[2]) / 255. * shade,
            1.,
        )
    }
    /// An uncovered, intact specimen waiting to be jacketed.
    fn exposed(&self, i: usize) -> bool {
        let c = self.cells[i];
        c.find.is_some() && c.depth == 0 && !c.taken && c.quality > 0
    }
    /// Jacketed samples still waiting for identification.
    fn waiting(&self) -> usize {
        self.samples
            .iter()
            .flatten()
            .filter(|s| !s.identified)
            .count()
    }
    fn objective_text(&self) -> String {
        match self.phase {
            Phase::Complete => "Museum open".into(),
            Phase::Lost => "Expedition closed".into(),
            Phase::Setback => "Recover this permit".into(),
            _ if self.ready() && self.index == 5 => "Open the museum at Camp".into(),
            _ if self.ready() => "Open next site at Camp".into(),
            _ => format!("Identify {} + 1 more", self.content().piece.to_lowercase()),
        }
    }
    /// Skeleton tones: museum sections in bone, `focus` in its color and
    /// the rest as `ghost`.
    fn giant_tones(&self, focus: Option<(usize, Color)>, ghost: Color) -> [Color; 6] {
        core::array::from_fn(|i| match focus {
            Some((part, tone)) if part == i => tone,
            _ if self.museum[i * 4].identified => BONE,
            _ => ghost,
        })
    }

    // ----- Feedback -------------------------------------------------------

    fn effect(&self, event: Event, fx: &mut Juice) {
        match event {
            Event::Began => {
                fx.ring(vec2(300., 360.), with_alpha(SAND, 0.7), 280.);
            }
            Event::Tool(tool) => {
                let [x, y, _, h] = tool_row(tool);
                fx.ring(vec2(x + 22., y + h * 0.5), TOOL_TONE[tool], 26.);
            }
            Event::Survey { at, revealed } => {
                fx.ring(tile_center(at), TEAL, 130.);
                for i in (0..CELLS).filter(|&i| revealed & (1 << i) != 0) {
                    let c = tile_center(i);
                    if self.cells[i].find.is_some() {
                        fx.sparkle(c, GOLD);
                        fx.popup_icon(c - vec2(0., 10.), Icon::Bone, "", GOLD);
                    } else {
                        fx.ring(c, with_alpha(TEAL, 0.45), 24.);
                    }
                }
                self.spend(fx);
            }
            Event::Cut {
                at,
                removed,
                loss,
                exposed,
                empty,
            } => {
                let c = tile_center(at);
                fx.burst(c, lighten(self.soil(0.9), 0.15), 6 + 5 * u32::from(removed));
                fx.ring(c, with_alpha(lighten(self.soil(1.), 0.2), 0.7), 26.);
                if loss > 0 {
                    fx.burst(c, RED, 16);
                    fx.ring(c, RED, 42.);
                    fx.popup_icon(c - vec2(0., 18.), Icon::Bone, &format!("-{loss}%"), RED);
                }
                // A clean reveal is celebrated; a cracked one is not.
                if exposed && loss == 0 && self.cells[at].quality > 0 {
                    fx.sparkle(c, BONE);
                    fx.burst(c, BONE, 10);
                    fx.ring(c, GOLD, 48.);
                    if self.cells[at].find == Some(0) {
                        let x = c.x.clamp(110., 850.);
                        let piece = title_case(self.content().piece);
                        fx.callout(vec2(x, c.y - 34.), &piece, GOLD);
                    } else {
                        fx.popup_icon(c - vec2(0., 18.), Icon::Bone, "", BONE);
                    }
                }
                if empty {
                    fx.popup(c - vec2(0., 12.), "Empty", DUST);
                }
                self.spend(fx);
            }
            Event::Jacket { at, .. } => {
                let c = tile_center(at);
                fx.burst(c, GOLD, 24);
                fx.sparkle(c, BONE);
                fx.ring(c, GOLD, 54.);
                fx.popup_icon(c - vec2(0., 16.), Icon::Bone, "+1", GOLD);
                fx.ring(tab_center(1), GOLD, 40.);
                fx.sparkle(tab_center(1), GOLD);
                self.spend(fx);
            }
            Event::Ruined { at } => {
                let c = tile_center(at);
                fx.burst(c, RED, 18);
                fx.ring(c, RED, 40.);
                fx.popup_icon(c - vec2(0., 14.), Icon::Cross, "", RED);
            }
            Event::Refused { spot, lack } => {
                let at = match spot {
                    Spot::Tile(i) => tile_center(i),
                    Spot::Lab => center(IDENTIFY),
                    Spot::Camp(n) => center(camp_row(n)),
                };
                if lack == Lack::Nothing {
                    fx.ring(at, with_alpha(DUST, 0.6), 26.);
                    return;
                }
                fx.shake(0.16);
                fx.ring(at, RED, 34.);
                let (glyph, text) = lack_badge(lack);
                fx.popup_icon(at - vec2(0., 16.), glyph, &text, RED);
                if let Some(k) = lack_stat(lack) {
                    fx.ring(strip_at(k), RED, 22.);
                }
            }
            Event::Identified {
                slot,
                grant,
                renown,
            } => {
                fx.ring(WELL, GOLD, 86.);
                fx.sparkle(WELL, GOLD);
                fx.burst(WELL, GOLD, 22);
                fx.popup_icon(WELL - vec2(0., 52.), Icon::Coin, &format!("+{grant}"), GOLD);
                fx.popup_icon(
                    WELL + vec2(78., -30.),
                    Icon::Crown,
                    &format!("+{renown}"),
                    BONE,
                );
                fx.sparkle(center(tray_row(slot)) - vec2(108., 0.), GREEN);
                if slot == 0 {
                    let piece = title_case(self.content().piece);
                    fx.callout(vec2(625., 214.), &piece, GOLD);
                    fx.sparkle(tab_center(3), GOLD);
                }
                self.spend(fx);
            }
            Event::Rejected { coins, renown, .. } => {
                let chip = center(species_chip(self.choice));
                fx.ring(chip, RED, 34.);
                fx.popup_icon(chip - vec2(0., 24.), Icon::Cross, "", RED);
                if coins > 0 {
                    fx.popup_icon(WELL - vec2(0., 52.), Icon::Coin, &format!("-{coins}"), RED);
                }
                if renown > 0 {
                    fx.popup_icon(WELL + vec2(78., -30.), Icon::Crown, "-1", RED);
                }
                self.spend(fx);
            }
            Event::Rested { light } => {
                let row = center(camp_row(0));
                fx.ring(row, GOLD, 44.);
                fx.popup_icon(row - vec2(0., 12.), Icon::Moon, "", BONE);
                fx.popup_icon(
                    strip_at(2) + vec2(0., 16.),
                    Icon::Sun,
                    &format!("+{light}"),
                    SUN,
                );
                fx.ring(strip_at(3), GOLD, 26.);
                fx.popup(strip_at(0) + vec2(0., 16.), "-12", GOLD);
            }
            Event::Bought => {
                let row = center(camp_row(1));
                fx.burst(row, SAND, 14);
                fx.popup_icon(row - vec2(0., 12.), Icon::Crate, "+12", BONE);
                fx.popup(strip_at(1) + vec2(0., 16.), "+12", BONE);
                fx.popup(strip_at(0) + vec2(0., 16.), "-10", GOLD);
            }
            Event::Upgraded(u) => {
                let row = center(camp_row(2 + u));
                fx.sparkle(row, GOLD);
                fx.ring(row, GOLD, 46.);
                fx.popup_icon(row - vec2(0., 12.), Icon::ArrowUp, "", GOLD);
            }
            Event::Advanced { site, medals } => {
                // The next briefing opens at once: celebrate around it, with
                // the medals rising from the finished permit's pip.
                fx.toast(Some(Icon::Flag), "Site cleared", GOLD);
                let pip = vec2(748. + site as f32 * 13., 216.);
                self.medal_popups(fx, pip, medals);
                fx.confetti(vec2(480., 190.), 40);
            }
            Event::Opened { medals } => {
                fx.toast(Some(Icon::Trophy), "Museum open", GOLD);
                self.medal_popups(fx, vec2(480., 300.), medals);
            }
            Event::Recovered => {
                fx.ring(vec2(480., 340.), SAND, 220.);
                fx.toast(Some(Icon::Restart), "Permit reset", SAND);
            }
            Event::Setback | Event::Closed => {}
        }
    }
    fn medal_popups(&self, fx: &mut Juice, at: Vec2, medals: u8) {
        for k in 0..medals {
            let x = at.x + (f32::from(k) - f32::from(medals - 1) * 0.5) * 38.;
            fx.popup_icon(vec2(x, at.y), Icon::Star, "", GOLD);
        }
    }
    /// Spent supplies and daylight pulse where they drain on the strip.
    fn spend(&self, fx: &mut Juice) {
        fx.ring(vec2(STAT_X[1] + 8., STRIP_Y), with_alpha(BONE, 0.7), 16.);
        fx.ring(vec2(STAT_X[2] + 8., STRIP_Y), with_alpha(SUN, 0.8), 18.);
    }

    // ----- Header ---------------------------------------------------------

    /// The permit's goal: its Giant section, one more find and then camp.
    fn objective(&self, area: Rect) {
        let t = clock();
        let cy = area.y + area.h * 0.5;
        let mut x = area.x + 2.;
        let s = 0.135;
        let at = vec2(x, cy - 14.);
        let done = self.phase == Phase::Complete;
        let part = self.index;
        let signature = self.samples[0];
        let found = signature.is_some_and(|s| s.identified) || done;
        let tone = if found { GREEN } else { GOLD };
        if !done {
            let beat = pulse(t, 0.9);
            let p = part_at(at, s, part);
            glow(
                p.x,
                p.y,
                12. + 3. * beat,
                with_alpha(tone, 0.25 + 0.3 * beat),
            );
        }
        let focus = (!done).then_some((part, tone));
        skeleton(
            at,
            s,
            self.giant_tones(focus, Color::new(0.62, 0.66, 0.72, 0.42)),
        );
        x += 104.;
        if done {
            icon_badge(Icon::Trophy, x + 11., cy, 24., GOLD, EARTH);
            say("Dawn Giant", x + 30., cy, 18., BONE);
            return;
        }
        let name = title_case(self.content().piece);
        x += say(&name, x, cy, 18., tone) + 8.;
        // The signature part: missing, waiting in the tray, or identified.
        if found {
            icon_badge(Icon::Check, x + 10., cy, 20., GREEN, INK);
        } else if signature.is_some() {
            icon_badge(Icon::Search, x + 10., cy, 20., GOLD, INK);
        } else {
            shapes::stroke_circle(vec2(x + 10., cy), 8., 1.5, with_alpha(GOLD, 0.6));
        }
        x += 32.;
        // Plus any one of the other three finds.
        let others = self.samples[1..]
            .iter()
            .flatten()
            .filter(|s| s.identified)
            .count();
        let plus = if others > 0 { GREEN } else { BONE };
        x += say("+1", x, cy, 18., plus) + 8.;
        for n in 1..4 {
            let c = vec2(x + 9., cy);
            match self.samples[n] {
                Some(s) if s.identified => icon_badge(Icon::Check, c.x, c.y, 18., GREEN, INK),
                Some(_) => icon(Icon::Bone, c.x, c.y, 16., BONE),
                None => shapes::stroke_circle(c, 6., 1.5, with_alpha(MUTED, 0.6)),
            }
            x += 22.;
        }
        // Then camp opens the next site.
        x += 6.;
        icon(Icon::ArrowRight, x + 8., cy, 14., with_alpha(MUTED, 0.7));
        x += 24.;
        let ready = self.ready() && self.phase == Phase::Field;
        let goal = if self.index == 5 {
            Icon::Trophy
        } else {
            Icon::Flag
        };
        if ready {
            let beat = pulse(t, 1.2);
            glow(
                x + 10.,
                cy,
                16. + 4. * beat,
                with_alpha(GREEN, 0.3 + 0.2 * beat),
            );
            icon(goal, x + 10., cy, 20., GREEN);
            keycap("3", "", x + 26., cy - 10., GREEN);
        } else {
            icon(goal, x + 10., cy, 18., with_alpha(MUTED, 0.55));
        }
    }
    /// The ending's key results: the assembled Giant and four icon stats.
    fn result(&self, area: Rect) {
        let s = 0.34;
        let at = vec2(area.x, area.y + 36.);
        skeleton(
            at,
            s,
            self.giant_tones(None, Color::new(0.45, 0.48, 0.52, 0.35)),
        );
        let sites = self.medals.iter().filter(|&&m| m > 0).count();
        let medals: u32 = self.medals.iter().map(|&m| u32::from(m)).sum();
        let cols = [area.x + 260., area.x + 450.];
        let rows = [area.y + 42., area.y + 104.];
        icon_value(
            Icon::Flag,
            &format!("{sites}/6"),
            cols[0],
            rows[0],
            28.,
            GOLD,
        );
        icon_value(
            Icon::Bone,
            &format!("{}/24", self.identified()),
            cols[1],
            rows[0],
            28.,
            BONE,
        );
        icon_value(
            Icon::Star,
            &format!("{medals}/18"),
            cols[0],
            rows[1],
            28.,
            GOLD,
        );
        icon_value(
            Icon::Coin,
            &self.coins.to_string(),
            cols[1],
            rows[1],
            28.,
            GOLD,
        );
    }

    // ----- Shared chrome --------------------------------------------------

    fn tabs(&self) {
        const TABS: [(Icon, &str); 5] = [
            (Icon::Pickaxe, "Dig"),
            (Icon::Search, "Lab"),
            (Icon::Home, "Camp"),
            (Icon::Bone, "Museum"),
            (Icon::Book, "Atlas"),
        ];
        rounded(18., 114., 924., 32., 9., Color::new(0., 0., 0., 0.22));
        let t = clock();
        let field = self.phase == Phase::Field;
        for (i, (glyph, name)) in TABS.into_iter().enumerate() {
            let x = 20. + i as f32 * 184.;
            let on = self.page.index() == i && self.phase != Phase::Complete;
            let hover = !on && field && hovered([x, 112., 184., 34.]);
            if on {
                rounded_gradient(
                    x + 2.,
                    116.,
                    180.,
                    28.,
                    7.,
                    lighten(SAND, 0.12),
                    darken(SAND, 0.08),
                );
            } else if hover {
                rounded(x + 2., 116., 180., 28., 7., Color::new(1., 0.92, 0.8, 0.1));
            }
            let key = if on {
                Color::new(0.17, 0.12, 0.09, 0.22)
            } else {
                Color::new(1., 0.92, 0.8, 0.08)
            };
            rounded(x + 12., 121., 18., 18., 5., key);
            say_center(
                DIGITS[i + 1],
                x + 21.,
                130.,
                12.,
                if on { EARTH } else { DUST },
            );
            icon(glyph, x + 48., 130., 18., if on { EARTH } else { SAND });
            let w = say(name, x + 64., 130., 16., if on { EARTH } else { BONE });
            // What needs attention, just after the label.
            let badge = vec2(x + 64. + w + 18., 130.);
            let beat = pulse(t, 1.1);
            match i {
                0 if !on && field && (0..CELLS).any(|c| self.exposed(c)) => {
                    glow(badge.x, badge.y, 12. + 3. * beat, with_alpha(GOLD, 0.4));
                    icon(Icon::Bone, badge.x, badge.y, 16., GOLD);
                }
                1 if self.waiting() > 0 && self.phase != Phase::Complete => {
                    glow(badge.x, badge.y, 13. + 4. * beat, with_alpha(GOLD, 0.45));
                    shapes::fill_circle(badge, 9., GOLD);
                    say_center(DIGITS[self.waiting()], badge.x, badge.y, 12., EARTH);
                }
                2 if field && self.ready() => {
                    glow(badge.x, badge.y, 13. + 4. * beat, with_alpha(GREEN, 0.45));
                    icon_badge(Icon::Flag, badge.x, badge.y, 20., GREEN, EARTH);
                }
                2 if field && (self.daylight < 4 || self.kits < 3) => {
                    icon(
                        Icon::Alert,
                        badge.x,
                        badge.y,
                        18.,
                        with_alpha(RED, 0.55 + 0.45 * beat),
                    );
                }
                _ => {}
            }
        }
    }
    /// Expedition resources, visible on every page.
    fn strip(&self) {
        let t = clock();
        let cy = STRIP_Y;
        rounded(24., 151., 912., 24., 8., Color::new(0., 0., 0., 0.2));
        icon_value(
            Icon::Coin,
            &self.coins.to_string(),
            STAT_X[0],
            cy,
            15.,
            GOLD,
        );
        let short = self.kits < 3;
        icon_value(
            Icon::Crate,
            &number(u32::from(self.kits)),
            STAT_X[1],
            cy,
            15.,
            if short { RED } else { BONE },
        );
        // Daylight drains as work is done.
        let full = self.content().daylight.max(1);
        let dark = self.daylight < 4;
        let beat = pulse(t, 1.6);
        icon(
            Icon::Sun,
            STAT_X[2] + 8.,
            cy,
            17.,
            if dark {
                with_alpha(RED, 0.55 + 0.45 * beat)
            } else {
                SUN
            },
        );
        meter(
            STAT_X[2] + 22.,
            cy - 4.,
            118.,
            8.,
            f32::from(self.daylight) / f32::from(full),
            if dark { RED } else { SUN },
        );
        say(
            &number(u32::from(self.daylight)),
            STAT_X[2] + 146.,
            cy,
            15.,
            if dark { RED } else { BONE },
        );
        icon(Icon::Calendar, STAT_X[3] + 8., cy, 15., BONE);
        pips(STAT_X[3] + 22., cy, u32::from(self.day), 3, SAND);
        icon_value(
            Icon::Crown,
            &number(u32::from(self.renown)),
            STAT_X[4],
            cy,
            15.,
            BONE,
        );
        for k in 0..3_u8 {
            let lit = k < self.retries;
            icon(
                Icon::Restart,
                STAT_X[5] + 8. + f32::from(k) * 18.,
                cy,
                14.,
                if lit { SAND } else { with_alpha(DIM, 0.6) },
            );
        }
        // This site's rock: hardness and strata.
        icon(Icon::Hammer, 818., cy, 15., DUST);
        hardness(830., cy, self.content().rock, SAND);
        for k in 0..4 {
            let shade = 0.95 - k as f32 * 0.17;
            panel(878., cy - 8. + k as f32 * 4., 46., 4., self.soil(shade));
        }
    }
    /// Dim the work area and raise a centred dialog card.
    fn dialog(&self, x: f32, y: f32, w: f32, h: f32, accent: Color) {
        panel(16., 112., 928., 456., Color::new(0.04, 0.03, 0.02, 0.62));
        card(x, y, w, h, Color::new(0.16, 0.12, 0.09, 1.));
        rounded(x + 26., y, 64., 3., 1.5, accent);
    }

    // ----- Dig ------------------------------------------------------------

    fn draw_dig(&self) {
        let t = clock();
        let field = self.phase == Phase::Field;
        let hover = Game::tile_at(pointer()).filter(|_| field);
        for i in 0..CELLS {
            self.draw_tile(i, t, hover == Some(i));
        }
        if field {
            self.survey_reach(t);
            self.cursor_mark(t);
        }
        self.dig_panel(t);
    }
    fn draw_tile(&self, i: usize, t: f32, hover: bool) {
        let c = self.cells[i];
        let o = tile_origin(i) + vec2(2., 2.);
        let (x, y, tw, th) = (o.x, o.y, 64., 56.);
        let removed = self.initial_depth(i).saturating_sub(c.depth);
        if c.depth == 0 && c.known {
            // Excavated pit: dark floor with a shaded upper lip.
            panel(x, y, tw, th, self.soil(0.3));
            panel(x, y, tw, 5., Color::new(0., 0., 0., 0.32));
            panel(x, y + th - 2., tw, 2., Color::new(1., 0.9, 0.7, 0.08));
            if c.find.is_none() {
                for k in 0..3 {
                    let yy = y + 17. + k as f32 * 12.;
                    draw_line(
                        x + 9.,
                        yy,
                        x + tw - 12.,
                        yy + 2. - k as f32,
                        1.5,
                        Color::new(0., 0., 0., 0.25),
                    );
                }
            }
        } else {
            // Surveyed ground shows its true strata color; unsurveyed
            // topsoil stays dusty and muted.
            let ground = if c.known {
                self.soil(0.6 + f32::from(c.depth.min(10)) * 0.04)
            } else {
                shapes::mix(self.soil(0.5), Color::new(0.25, 0.22, 0.19, 1.), 0.5)
            };
            panel(x, y, tw, th, ground);
            panel(x, y, tw, 2., Color::new(1., 0.95, 0.85, 0.16));
            panel(x, y + th - 3., tw, 3., Color::new(0., 0., 0., 0.2));
            if c.known {
                for k in 0..2 {
                    let yy = y + 20. + k as f32 * 16.;
                    draw_line(
                        x + 4.,
                        yy,
                        x + tw - 14. - ((i + k * 3) % 4) as f32 * 4.,
                        yy + (i % 3) as f32 - 1.,
                        2.,
                        Color::new(0.1, 0.07, 0.04, 0.2),
                    );
                }
            } else {
                for k in 0..3 {
                    let h = (i * 7 + k * 13) % 23;
                    let px = x + 8. + (h * 37 % 48) as f32;
                    let py = y + 10. + (h * 11 % 36) as f32;
                    panel(px, py, 3., 2., Color::new(0.1, 0.07, 0.04, 0.28));
                    panel(px, py - 1., 3., 1., Color::new(1., 0.95, 0.85, 0.12));
                }
            }
            self.cracks(i, x, y);
            if removed > 0 {
                // Every stroke deepens a visible pit.
                let r = f32::from(removed.min(6));
                let inset = 3. + r * 2.2;
                rounded(
                    x + inset,
                    y + inset,
                    tw - inset * 2. - 8.,
                    th - inset * 2.,
                    4.,
                    Color::new(0., 0., 0., 0.08 + 0.035 * r),
                );
                panel(
                    x + inset + 2.,
                    y + inset,
                    tw - inset * 2. - 12.,
                    2.,
                    Color::new(0., 0., 0., 0.2),
                );
            }
        }
        let cx = x + tw * 0.5;
        let cy = y + th * 0.5 + 2.;
        if c.known && c.depth > 0 {
            self.gauge(i, x + tw - 10., y, th, t);
            draw_text_styled(
                DIGITS[usize::from(c.depth.min(12))],
                x + 6.,
                y + 16.,
                style(13., BONE),
            );
        }
        if c.known
            && let Some(n) = c.find
        {
            let kind = self.content().finds[n].kind;
            if c.taken {
                specimen(kind, cx, cy, 14., Color::new(1., 1., 1., 0.1), EARTH);
                icon_badge(Icon::Check, cx, cy, 22., with_alpha(GREEN, 0.9), EARTH);
            } else if c.quality == 0 {
                specimen(kind, cx, cy, 15., RED, self.soil(0.3));
            } else if c.depth == 0 {
                let beat = pulse(t + i as f32 * 0.37, 0.9);
                glow(cx, cy, 24. + 5. * beat, with_alpha(GOLD, 0.2 + 0.14 * beat));
                specimen(
                    kind,
                    cx + 1.,
                    cy + 2.,
                    15.,
                    Color::new(0., 0., 0., 0.45),
                    EARTH,
                );
                specimen(kind, cx, cy, 15., BONE, self.soil(0.3));
                glint(
                    vec2(cx + 13., cy - 13.),
                    5. * pulse(t * 0.7 + i as f32 * 0.21, 1.),
                    BONE,
                );
            } else {
                let danger = i == self.cursor
                    && self.phase == Phase::Field
                    && self.cut_loss(i, self.tool) > 0;
                let a = 0.3 + 0.16 * pulse(t + i as f32 * 0.37, 0.5);
                let tone = if danger {
                    with_alpha(RED, 0.55 + 0.3 * pulse(t, 2.))
                } else {
                    with_alpha(BONE, a)
                };
                specimen(kind, cx - 3., cy, 14., tone, self.soil(0.5));
            }
        }
        if hover && i != self.cursor {
            outline(
                x - 2.,
                y - 2.,
                68.,
                60.,
                6.,
                1.5,
                Color::new(1., 1., 1., 0.38),
            );
        }
    }
    /// Hardness shows as fractures: none in silt, many in basalt.
    fn cracks(&self, i: usize, x: f32, y: f32) {
        for k in 0..usize::from(self.cells[i].hard) {
            let h = (i * 31 + k * 17) % 97;
            let sx = x + 10. + (h % 34) as f32;
            let sy = y + 8. + ((h / 3) % 30) as f32;
            let dir = if (h + k).is_multiple_of(2) { 1. } else { -1. };
            let points = [
                vec2(sx, sy),
                vec2(sx + 5. * dir, sy + 4.),
                vec2(sx + 3. * dir, sy + 9.),
                vec2(sx + 8. * dir, sy + 13.),
            ];
            shapes::polyline(
                &points,
                shapes::Stroke::new(1.3, Color::new(0.06, 0.04, 0.03, 0.42)),
            );
        }
    }
    /// Remaining layers as stacked slabs on a tile's right edge. Spent layers
    /// darken; on the selected tile the tool's next cut pulses in its color.
    fn gauge(&self, i: usize, x: f32, y: f32, th: f32, t: f32) {
        let c = self.cells[i];
        let total = self.initial_depth(i).max(c.depth);
        let removed = total - c.depth;
        let pitch = (44. / f32::from(total.max(1))).min(5.5);
        let top = y + th * 0.5 - pitch * f32::from(total) * 0.5;
        let cut = if i == self.cursor && self.phase == Phase::Field {
            self.tool_stats(i).0.min(c.depth)
        } else {
            0
        };
        let tone = lighten(TOOL_TONE[self.tool], 0.15);
        let beat = pulse(t, 1.6);
        rounded(
            x - 2.,
            top - 2.,
            10.,
            pitch * f32::from(total) + 2.6,
            3.,
            Color::new(0., 0., 0., 0.25),
        );
        for k in 0..total {
            let by = top + f32::from(k) * pitch;
            let h = (pitch - 1.4).max(1.);
            let color = if k < removed {
                Color::new(0., 0., 0., 0.25)
            } else if k < removed + cut {
                with_alpha(tone, 0.5 + 0.5 * beat)
            } else {
                with_alpha(BONE, 0.75)
            };
            panel(x, by, 6., h, color);
        }
    }
    /// The cross a survey from the cursor would reveal.
    fn survey_reach(&self, t: f32) {
        let a = if hovered(SURVEY_BUTTON) {
            0.9
        } else {
            0.22 + 0.2 * pulse(t, 0.8)
        };
        for i in 0..CELLS {
            if Game::in_cross(self.cursor, i) && !self.cells[i].known {
                let o = tile_origin(i);
                brackets(o.x + 4., o.y + 4., 60., 52., with_alpha(TEAL, a));
            }
        }
    }
    fn cursor_mark(&self, t: f32) {
        let o = tile_origin(self.cursor);
        let beat = pulse(t, 1.2);
        outline(
            o.x - 2.,
            o.y - 2.,
            72.,
            64.,
            8.,
            2.,
            with_alpha(GOLD, 0.3 * beat),
        );
        outline(o.x, o.y, 68., 60., 6., 2.5, GOLD);
        let c = self.cells[self.cursor];
        if c.known && c.depth > 0 && self.cut_loss(self.cursor, self.tool) > 0 {
            icon_badge(
                Icon::Alert,
                o.x + 14.,
                o.y + 46.,
                18.,
                with_alpha(RED, 0.75 + 0.25 * beat),
                EARTH,
            );
        }
    }
    fn tile_state(&self, at: usize) -> (Icon, &'static str, Color) {
        let c = self.cells[at];
        if !c.known {
            (Icon::EyeOff, "Unsurveyed", DUST)
        } else if c.taken {
            (Icon::Check, "Jacketed", GREEN)
        } else if c.find.is_some() && c.quality == 0 {
            (Icon::Skull, "Destroyed", RED)
        } else if c.find.is_some() && c.depth == 0 {
            (Icon::Star, "Exposed", GOLD)
        } else if c.find.is_some() {
            (Icon::Bone, "Fossil", GOLD)
        } else if c.depth == 0 {
            (Icon::Mountain, "Bedrock", DUST)
        } else {
            (Icon::Cross, "Empty", DUST)
        }
    }
    fn dig_panel(&self, t: f32) {
        slab(594., 182., 338., 358.);
        let at = self.cursor;
        let c = self.cells[at];
        self.core(at, 614., 196., 44., 100., t);
        let (glyph, word, tone) = self.tile_state(at);
        icon_chip(glyph, word, 678., 198., 15., tone);
        let depth: Cow<str> = if c.known {
            number(u32::from(c.depth))
        } else {
            "?".into()
        };
        icon_value(Icon::ArrowDown, &depth, 680., 248., 18., BONE);
        if c.known && c.find.is_some() && !c.taken {
            let q = c.quality;
            icon(Icon::Bone, 742., 248., 18., quality_tone(q));
            meter(756., 244., 104., 8., f32::from(q) / 100., quality_tone(q));
            say(&format!("{q}%"), 868., 248., 15., BONE);
        }
        // What the selected tool will do to the fossil below.
        if self.phase == Phase::Field && c.known && c.depth > 0 {
            let (power, _, _) = self.tool_stats(at);
            let left = c.depth.saturating_sub(power);
            tool_glyph(self.tool, 690., 282., 0.9, TOOL_TONE[self.tool]);
            icon(Icon::ArrowRight, 716., 281., 14., DUST);
            icon_value(
                Icon::ArrowDown,
                DIGITS[usize::from(left)],
                732.,
                281.,
                16.,
                BONE,
            );
            let loss = self.cut_loss(at, self.tool);
            if loss > 0 {
                let beat = pulse(t, 2.);
                icon_value(
                    Icon::Bone,
                    &format!("-{loss}%"),
                    790.,
                    281.,
                    16.,
                    with_alpha(RED, 0.7 + 0.3 * beat),
                );
            } else if c.find.is_some() && left == 0 {
                icon_value(Icon::Shield, "", 790., 281., 16., GREEN);
            }
        }
        self.dig_buttons(t);
        self.tool_rows();
    }
    /// A cross-section of the selected tile: spent layers as outlines, rock
    /// still to remove in strata tones, and what waits at the bottom. The
    /// selected tool's next stroke is bracketed; red means it reaches bone.
    fn core(&self, at: usize, x: f32, y: f32, w: f32, h: f32, t: f32) {
        let c = self.cells[at];
        rounded(
            x - 4.,
            y - 4.,
            w + 8.,
            h + 8.,
            7.,
            Color::new(0., 0., 0., 0.3),
        );
        if !c.known {
            rounded_gradient(x, y, w, h, 4., self.soil(0.42), self.soil(0.22));
            icon(
                Icon::Question,
                x + w * 0.5,
                y + h * 0.5,
                22.,
                with_alpha(BONE, 0.45),
            );
            return;
        }
        let total = self.initial_depth(at).max(c.depth).max(1);
        let removed = total - c.depth;
        let bed = 28.;
        let unit = (h - bed) / f32::from(total);
        for k in 0..total {
            let ly = y + f32::from(k) * unit;
            if k < removed {
                outline(x + 1., ly + 1., w - 2., unit - 2., 2., 1., HAIRLINE);
            } else {
                let shade = 0.9 - 0.035 * f32::from(k) - if k % 2 == 1 { 0.08 } else { 0. };
                rounded(x, ly + 0.5, w, unit - 1., 2., self.soil(shade));
            }
        }
        let by = y + h - bed;
        rounded(x, by, w, bed, 4., Color::new(0.07, 0.05, 0.04, 1.));
        let bed_at = vec2(x + w * 0.5, by + bed * 0.5);
        match c.find {
            Some(n) if !c.taken => {
                let kind = self.content().finds[n].kind;
                let tone = if c.quality == 0 {
                    RED
                } else if c.depth == 0 {
                    BONE
                } else {
                    with_alpha(BONE, 0.6)
                };
                specimen(kind, bed_at.x, bed_at.y, 10., tone, EARTH);
            }
            Some(_) => icon(Icon::Check, bed_at.x, bed_at.y, 16., GREEN),
            None => {
                for k in 0..3 {
                    let xx = x + 8. + k as f32 * 12.;
                    draw_line(
                        xx,
                        by + bed - 6.,
                        xx + 8.,
                        by + 6.,
                        1.5,
                        Color::new(1., 0.95, 0.85, 0.12),
                    );
                }
            }
        }
        if self.phase != Phase::Field || c.depth == 0 {
            return;
        }
        let (power, _, _) = self.tool_stats(at);
        let cut = power.min(c.depth);
        let tone = TOOL_TONE[self.tool];
        let beat = pulse(t, 1.6);
        let top = y + f32::from(removed) * unit;
        let bottom = top + f32::from(cut) * unit;
        rounded(
            x,
            top,
            w,
            bottom - top,
            2.,
            with_alpha(tone, 0.22 + 0.25 * beat),
        );
        let bx = x + w + 8.;
        draw_line(bx, top + 1., bx, bottom - 1., 2., tone);
        draw_line(bx - 4., top + 1., bx, top + 1., 2., tone);
        draw_line(bx - 4., bottom - 1., bx, bottom - 1., 2., tone);
        if self.cut_loss(at, self.tool) > 0 {
            glow(
                bed_at.x,
                bed_at.y,
                20. + 4. * beat,
                with_alpha(RED, 0.35 + 0.2 * beat),
            );
            outline(x, by, w, bed, 4., 2., RED);
        }
    }
    fn dig_buttons(&self, t: f32) {
        let at = self.cursor;
        let c = self.cells[at];
        let jacket = c.depth == 0 && c.find.is_some() && !c.taken;
        let open = !(c.taken || (c.depth == 0 && c.find.is_none()));
        let (kits, time) = if jacket {
            (1, 2)
        } else {
            let (_, kits, time) = self.tool_stats(at);
            (kits, time)
        };
        let (glyph, word, tone) = if jacket {
            (Icon::Bone, "Jacket", GOLD)
        } else {
            (Icon::Pickaxe, "Dig", TOOL_TONE[self.tool])
        };
        let right = self.action(DIG_BUTTON, "Space", glyph, word, open, tone, kits, time);
        let loss = if c.known && c.depth > 0 {
            self.cut_loss(at, self.tool)
        } else {
            0
        };
        let cy = DIG_BUTTON[1] + DIG_BUTTON[3] * 0.5;
        if open && loss > 0 {
            let beat = pulse(t, 2.);
            stat_left(
                Icon::Bone,
                &format!("-{loss}"),
                right,
                cy,
                15.,
                with_alpha(RED, 0.7 + 0.3 * beat),
            );
        } else if open && jacket {
            stat_left(Icon::Bone, "+1", right, cy, 15., GOLD);
        }
        let fresh = (0..CELLS)
            .filter(|&i| Game::in_cross(at, i) && !self.cells[i].known)
            .count();
        let right = self.action(
            SURVEY_BUTTON,
            "X",
            Icon::Search,
            "Survey",
            fresh > 0,
            TEAL,
            1,
            2,
        );
        if fresh > 0 {
            let cy = SURVEY_BUTTON[1] + SURVEY_BUTTON[3] * 0.5;
            stat_left(Icon::Eye, &format!("+{fresh}"), right, cy, 15., TEAL);
        }
    }
    /// A clickable action: its key, icon and word on the left and its costs
    /// on the right, red when unaffordable. Returns where previews may end.
    #[allow(clippy::too_many_arguments)]
    fn action(
        &self,
        r: [f32; 4],
        key: &str,
        glyph: Icon,
        word: &str,
        open: bool,
        tone: Color,
        kits: u16,
        time: u16,
    ) -> f32 {
        let [x, y, w, h] = r;
        let afford = self.kits >= kits && self.daylight >= time;
        let live = open && afford && self.phase == Phase::Field;
        let hover = live && hovered(r);
        if live {
            rounded_gradient(
                x,
                y,
                w,
                h,
                9.,
                with_alpha(lighten(tone, 0.1), if hover { 0.34 } else { 0.22 }),
                with_alpha(darken(tone, 0.25), if hover { 0.26 } else { 0.16 }),
            );
            outline(
                x,
                y,
                w,
                h,
                9.,
                if hover { 2. } else { 1. },
                with_alpha(tone, if hover { 0.95 } else { 0.6 }),
            );
        } else {
            rounded(x, y, w, h, 9., Color::new(1., 0.92, 0.8, 0.04));
            outline(x, y, w, h, 9., 1., HAIRLINE);
        }
        let cy = y + h * 0.5;
        let text = if live { BONE } else { DIM };
        let kw = keycap(key, "", x + 8., cy - 10., text);
        icon(
            glyph,
            x + kw + 24.,
            cy,
            18.,
            if live { lighten(tone, 0.25) } else { DIM },
        );
        say(word, x + kw + 40., cy, 16., text);
        let right = x + w - 10.;
        if !open {
            return right;
        }
        let right = stat_left(
            Icon::Sun,
            DIGITS[usize::from(time)],
            right,
            cy,
            14.,
            if self.daylight >= time { text } else { RED },
        );
        stat_left(
            Icon::Crate,
            DIGITS[usize::from(kits)],
            right,
            cy,
            14.,
            if self.kits >= kits { text } else { RED },
        )
    }
    fn tool_rows(&self) {
        let field = self.phase == Phase::Field;
        for i in 0..3 {
            let r = tool_row(i);
            let [x, y, w, h] = r;
            let on = self.tool == i;
            row_surface(x, y, w, h, on, !on && field && hovered(r));
            rounded(x + 2., y + 9., 3., h - 18., 1.5, TOOL_TONE[i]);
            let cy = y + h * 0.5;
            tool_glyph(i, x + 22., cy + 1., 1.1, if on { BONE } else { DUST });
            say(TOOL_NAME[i], x + 44., cy, 15., if on { BONE } else { SAND });
            let (power, kits, time) = self.tool_power(i, self.cursor);
            let text = if on { BONE } else { DUST };
            let (risk, tone) = if i == 0 {
                (Icon::Shield, GREEN)
            } else {
                (Icon::Alert, TOOL_TONE[i])
            };
            let mut right = x + w - 10.;
            icon(risk, right - 9., cy, 18., tone);
            right -= 30.;
            right = stat_left(Icon::Sun, DIGITS[usize::from(time)], right, cy, 14., text);
            right = stat_left(Icon::Crate, DIGITS[usize::from(kits)], right, cy, 14., text);
            stat_left(
                Icon::ArrowDown,
                DIGITS[usize::from(power)],
                right - 4.,
                cy,
                14.,
                if on { GOLD } else { SAND },
            );
        }
    }

    // ----- Lab ------------------------------------------------------------

    fn draw_lab(&self) {
        let t = clock();
        for n in 0..4 {
            self.tray_slot(n, t);
        }
        self.evidence();
        for i in 0..SPECIES.len() {
            let r = species_chip(i);
            let [x, y, w, h] = r;
            let on = self.choice == i;
            let hover = !on && self.phase == Phase::Field && hovered(r);
            row_surface(x, y, w, h, on, hover);
            let tone = if on {
                BONE
            } else if hover {
                lighten(SAND, 0.25)
            } else {
                SAND
            };
            specimen(i, x + w * 0.5, y + h * 0.5, 12., tone, EARTH);
        }
        let sample = self.samples[self.sample];
        let ready = sample.is_some_and(|s| !s.identified);
        let afford = self.kits >= 1 && self.daylight >= 2;
        let live = ready && afford && self.phase == Phase::Field;
        let [x, y, w, h] = IDENTIFY;
        let hover = live && hovered(IDENTIFY);
        if live {
            rounded_gradient(
                x,
                y,
                w,
                h,
                10.,
                lighten(GREEN, if hover { 0.14 } else { 0.02 }),
                darken(GREEN, 0.18),
            );
            outline(x, y, w, h, 10., 1., Color::new(1., 1., 1., 0.25));
        } else {
            rounded(x, y, w, h, 10., Color::new(1., 0.92, 0.8, 0.06));
        }
        let cy = y + h * 0.5;
        let text = if live { EARTH } else { DIM };
        keycap("Space", "", x + 12., cy - 10., BONE);
        icon(Icon::Search, x + 78., cy, 18., text);
        say("Identify", x + 94., cy, 17., text);
        if ready {
            let right = stat_left(
                Icon::Sun,
                "2",
                x + w - 14.,
                cy,
                15.,
                if self.daylight >= 2 { text } else { RED },
            );
            stat_left(
                Icon::Crate,
                "1",
                right,
                cy,
                15.,
                if self.kits >= 1 { text } else { RED },
            );
        }
    }
    fn tray_slot(&self, n: usize, t: f32) {
        let r = tray_row(n);
        let [x, y, w, h] = r;
        let on = n == self.sample;
        row_surface(
            x,
            y,
            w,
            h,
            on,
            !on && self.phase == Phase::Field && hovered(r),
        );
        let well = vec2(x + 32., y + h * 0.5);
        match self.samples[n] {
            Some(s) => {
                displayed(s.kind, well.x, well.y, 23., BONE);
                ring_meter(
                    well.x,
                    well.y,
                    27.,
                    3.,
                    f32::from(s.quality) / 100.,
                    quality_tone(s.quality),
                );
                if s.identified {
                    label(
                        &title_case(SPECIES[s.kind].name),
                        x + 70.,
                        well.y,
                        170.,
                        15.,
                        BONE,
                    );
                    icon_badge(Icon::Check, x + w - 22., well.y, 22., GREEN, EARTH);
                } else {
                    let beat = pulse(t + n as f32 * 0.3, 1.);
                    icon_badge(
                        Icon::Question,
                        x + 84.,
                        well.y,
                        26.,
                        with_alpha(GOLD, 0.7 + 0.3 * beat),
                        EARTH,
                    );
                    let grant = SPECIES[s.kind].value * u32::from(s.quality) / 100;
                    stat_left(
                        Icon::Coin,
                        &grant.to_string(),
                        x + w - 12.,
                        well.y,
                        15.,
                        GOLD,
                    );
                }
            }
            None => {
                shapes::stroke_circle(well, 22., 1., Color::new(1., 0.9, 0.75, 0.18));
                if n == 0 {
                    specimen(5, well.x, well.y, 14., with_alpha(BONE, 0.14), EARTH);
                }
            }
        }
        if n == 0 {
            // The signature part's slot.
            shapes::stroke_circle(well, 30.5, 1.5, with_alpha(GOLD, 0.75));
            icon_badge(Icon::Star, well.x + 22., well.y - 20., 15., GOLD, EARTH);
        }
    }
    /// The selected specimen beside the chosen species, the traits to read
    /// and what a right or wrong identification pays.
    fn evidence(&self) {
        slab(332., 182., 586., 232.);
        let sample = self.samples[self.sample];
        match sample {
            Some(s) => {
                displayed(s.kind, WELL.x, WELL.y, 58., BONE);
                ring_meter(
                    WELL.x,
                    WELL.y,
                    64.,
                    4.,
                    f32::from(s.quality) / 100.,
                    quality_tone(s.quality),
                );
                say_center(
                    &format!("{}%", s.quality),
                    WELL.x,
                    WELL.y + 86.,
                    15.,
                    quality_tone(s.quality),
                );
            }
            None => {
                shapes::stroke_circle(WELL, 58., 1., HAIRLINE);
                icon(Icon::Pickaxe, WELL.x, WELL.y, 34., with_alpha(DUST, 0.45));
                keycap("1", "", WELL.x - 9., WELL.y + 74., DUST);
            }
        }
        let known = sample.filter(|s| s.identified).map(|s| s.kind);
        let candidate = known.unwrap_or(self.choice);
        displayed(
            candidate,
            CANDIDATE.x,
            CANDIDATE.y,
            58.,
            if known.is_some() { BONE } else { SAND },
        );
        say_center(
            &title_case(SPECIES[candidate].name),
            CANDIDATE.x,
            CANDIDATE.y + 86.,
            15.,
            BONE,
        );
        let mid = 625.;
        match sample {
            Some(s) if s.identified => {
                icon_badge(Icon::Check, mid, WELL.y, 56., GREEN, EARTH);
                let grant = SPECIES[s.kind].value * u32::from(s.quality) / 100;
                let w = 15. * 1.25 + text_width(&grant.to_string(), style(15., DUST));
                icon_value(
                    Icon::Coin,
                    &grant.to_string(),
                    mid - w * 0.5,
                    WELL.y + 52.,
                    15.,
                    DUST,
                );
            }
            Some(s) => {
                for (k, line) in traits(s.kind).enumerate() {
                    let w = text_width(line, style(14., BONE)) + 14. * 1.3;
                    chip(line, mid - w * 0.5, 202. + k as f32 * 30., 14., BONE);
                }
                icon(Icon::Question, mid, 284., 26., with_alpha(SAND, 0.6));
                // Outcomes, right then wrong.
                let grant = SPECIES[s.kind].value * u32::from(s.quality) / 100;
                let renown = if s.quality >= 80 { 3 } else { 1 };
                let x = mid - 74.;
                icon_badge(Icon::Check, x, 330., 22., GREEN, EARTH);
                let w = icon_value(Icon::Coin, &format!("+{grant}"), x + 20., 330., 16., GOLD);
                icon_value(
                    Icon::Crown,
                    &format!("+{renown}"),
                    x + 36. + w,
                    330.,
                    16.,
                    BONE,
                );
                icon_badge(Icon::Cross, x, 364., 22., RED, EARTH);
                let w = icon_value(
                    Icon::Coin,
                    &format!("-{}", self.coins.min(6)),
                    x + 20.,
                    364.,
                    16.,
                    RED,
                );
                if self.renown > 0 {
                    icon_value(Icon::Crown, "-1", x + 36. + w, 364., 16., RED);
                }
            }
            None => {}
        }
    }

    // ----- Camp -----------------------------------------------------------

    fn camp_title(&self, n: usize) -> &'static str {
        match n {
            0 => "Rest",
            1 => "Supplies",
            2 => "Precision",
            3 => "Motor",
            4 if self.index == 5 => "Museum",
            4 => "Next site",
            _ => "Recover",
        }
    }
    fn camp_glyph(&self, n: usize, x: f32, y: f32, size: f32, color: Color) {
        let s = size / 20.;
        match n {
            0 => icon(Icon::Moon, x, y, size, color),
            1 => icon(Icon::Crate, x, y, size, color),
            2 => tool_glyph(0, x - s, y + s, s * 1.15, color),
            3 => tool_glyph(2, x, y, s * 1.15, color),
            4 if self.index == 5 => icon(Icon::Trophy, x, y, size, color),
            4 => icon(Icon::Flag, x, y, size, color),
            _ => icon(Icon::Restart, x, y, size, color),
        }
    }
    fn draw_camp(&self) {
        let t = clock();
        let field = self.phase == Phase::Field;
        let hover = (0..6).find(|&n| field && hovered(camp_row(n)));
        for n in 0..6 {
            self.camp_card(n, hover == Some(n), t);
        }
        self.camp_preview(hover.unwrap_or(self.row));
    }
    fn camp_card(&self, n: usize, hover: bool, t: f32) {
        let [x, y, w, h] = camp_row(n);
        let on = self.row == n;
        let lack = self.camp_lack(n);
        let live = lack.is_none();
        row_surface(x, y, w, h, on, hover && !on);
        if n == 5 {
            rounded(x, y, w, h, 8., with_alpha(RED, 0.05));
        }
        let cy = y + h * 0.5;
        let go = n == 4 && live;
        if go {
            let beat = pulse(t, 1.);
            rounded(x + 1., y + 8., 3., h - 16., 1.5, GREEN);
            glow(
                x + 28.,
                cy,
                26. + 4. * beat,
                with_alpha(GREEN, 0.25 + 0.15 * beat),
            );
        }
        let a = if live { 1. } else { 0.5 };
        shapes::fill_circle(vec2(x + 28., cy), 17., Color::new(0., 0., 0., 0.22));
        let tone = if n == 5 {
            RED
        } else if go {
            GREEN
        } else {
            SAND
        };
        self.camp_glyph(n, x + 28., cy, 20., with_alpha(tone, a));
        say(
            self.camp_title(n),
            x + 56.,
            cy,
            17.,
            with_alpha(if go { GREEN } else { BONE }, a),
        );
        // Costs and status, right-aligned before the Space key.
        let shade = |bad: bool, color: Color| if bad { RED } else { with_alpha(color, a) };
        let coins = |need: u32| matches!(lack, Some(Lack::Coins(c)) if c == need);
        let mut right = x + w - 64.;
        match n {
            0 => {
                right = stat_left(Icon::Coin, "12", right, cy, 15., shade(coins(12), GOLD));
                pips(
                    right - 36.,
                    cy,
                    u32::from(self.day),
                    3,
                    shade(lack == Some(Lack::Days), SAND),
                );
                icon(
                    Icon::Calendar,
                    right - 52.,
                    cy,
                    15.,
                    shade(lack == Some(Lack::Days), BONE),
                );
            }
            1 => {
                right = stat_left(Icon::Coin, "10", right, cy, 15., shade(coins(10), GOLD));
                right = stat_left(
                    Icon::Crate,
                    "+12",
                    right,
                    cy,
                    15.,
                    shade(lack == Some(Lack::Storage), BONE),
                );
                for k in 0..5_u8 {
                    let px = right - 50. + f32::from(k) * 10.;
                    if k < self.packs {
                        rounded(px, cy - 4., 7., 8., 2., with_alpha(SAND, a));
                    } else {
                        outline(px, cy - 4., 7., 8., 2., 1., shade(self.packs == 0, DIM));
                    }
                }
            }
            2 | 3 => {
                let u = n - 2;
                let level = self.upgrades[u];
                if lack == Some(Lack::Maxed) {
                    right = stat_left(Icon::Check, "Max", right, cy, 15., GREEN);
                } else {
                    let cost = self.upgrade_cost(u);
                    let need = 3 + u16::from(level) * 3;
                    right = stat_left(
                        Icon::Coin,
                        &cost.to_string(),
                        right,
                        cy,
                        15.,
                        shade(coins(cost), GOLD),
                    );
                    right = stat_left(
                        Icon::Crown,
                        &number(u32::from(need)),
                        right,
                        cy,
                        15.,
                        shade(lack == Some(Lack::Renown(need)), BONE),
                    );
                }
                pips(
                    right - 23.,
                    cy,
                    u32::from(level) + 1,
                    2,
                    with_alpha(GOLD, a),
                );
            }
            4 => {
                right = stat_left(Icon::Coin, "+30", right, cy, 15., with_alpha(GOLD, a));
                let signature = self.samples[0].is_some_and(|s| s.identified);
                let others = self.samples[1..].iter().flatten().any(|s| s.identified);
                for (k, ok) in [others, signature].into_iter().enumerate() {
                    let c = vec2(right - 12. - k as f32 * 30., cy);
                    if ok {
                        icon_badge(Icon::Check, c.x, c.y, 20., GREEN, EARTH);
                    } else {
                        shapes::stroke_circle(c, 9., 1.5, with_alpha(GOLD, 0.7));
                        icon(Icon::Bone, c.x, c.y, 12., with_alpha(GOLD, 0.7));
                    }
                }
                icon_badge(Icon::Star, right - 54., cy - 9., 12., GOLD, EARTH);
            }
            _ => {
                for k in 0..3_u8 {
                    icon(
                        Icon::Restart,
                        right - 8. - f32::from(2 - k) * 20.,
                        cy,
                        15.,
                        if k < self.retries {
                            SAND
                        } else {
                            with_alpha(DIM, 0.6)
                        },
                    );
                }
            }
        }
        if on && self.phase == Phase::Field {
            keycap("Space", "", x + w - 54., cy - 10., BONE);
        }
    }
    /// Before and after for every resource the focused action changes.
    fn camp_preview(&self, n: usize) {
        let (x, y, w) = (592., 182., 332.);
        slab(x, y, w, 354.);
        let l = x + 22.;
        let lack = self.camp_lack(n);
        let a = if lack.is_none() { 1. } else { 0.55 };
        shapes::fill_circle(vec2(l + 22., y + 38.), 22., Color::new(0., 0., 0., 0.25));
        self.camp_glyph(n, l + 22., y + 38., 26., if n == 5 { RED } else { GOLD });
        say(self.camp_title(n), l + 56., y + 38., 22., BONE);
        divider(l, y + 72., w - 44., HAIRLINE);
        let mut row = y + 104.;
        let daylight = u32::from(self.daylight);
        let coins = self.coins;
        match n {
            0 => {
                let full = u32::from(self.content().daylight);
                self.delta(l, &mut row, Icon::Sun, daylight, full, SUN, a);
                let day = u32::from(self.day);
                self.delta(l, &mut row, Icon::Calendar, day, (day + 1).min(3), BONE, a);
                let after = coins.saturating_sub(12);
                self.delta(l, &mut row, Icon::Coin, coins, after, GOLD, a);
            }
            1 => {
                let kits = u32::from(self.kits);
                self.delta(l, &mut row, Icon::Crate, kits, kits + 12, BONE, a);
                let after = coins.saturating_sub(10);
                self.delta(l, &mut row, Icon::Coin, coins, after, GOLD, a);
            }
            2 | 3 => {
                let u = n - 2;
                let mut after = self.clone();
                after.upgrades[u] = (after.upgrades[u] + 1).min(2);
                if u == 0 {
                    let (_, _, now) = self.tool_power(0, self.cursor);
                    let (_, _, next) = after.tool_power(0, self.cursor);
                    let (now, next) = (u32::from(now), u32::from(next));
                    self.delta(l, &mut row, Icon::Sun, now, next, SUN, a);
                    let guard = |level: u8| u32::from(level) * 8;
                    let (now, next) = (guard(self.upgrades[0]), guard(after.upgrades[0]));
                    self.delta(l, &mut row, Icon::Shield, now, next, GREEN, a);
                } else {
                    let (now, _, _) = self.tool_power(2, self.cursor);
                    let (next, _, _) = after.tool_power(2, self.cursor);
                    let (now, next) = (u32::from(now), u32::from(next));
                    self.delta(l, &mut row, Icon::ArrowDown, now, next, GOLD, a);
                }
                if self.upgrades[u] < 2 {
                    let after = coins.saturating_sub(self.upgrade_cost(u));
                    self.delta(l, &mut row, Icon::Coin, coins, after, GOLD, a);
                }
            }
            4 => {
                let medals = self.permit_medals();
                for k in 0..3_u8 {
                    icon(
                        Icon::Star,
                        l + 14. + f32::from(k) * 34.,
                        row,
                        28.,
                        if k < medals {
                            with_alpha(GOLD, a)
                        } else {
                            with_alpha(DIM, 0.5)
                        },
                    );
                }
                row += 48.;
                self.delta(l, &mut row, Icon::Coin, coins, coins + 30, GOLD, a);
                if self.index < 5 {
                    let kits = u32::from(self.kits);
                    let after = (kits + 16).min(120);
                    self.delta(l, &mut row, Icon::Crate, kits, after, BONE, a);
                }
                let score = self.score + self.permit_score(medals.max(1));
                self.delta(l, &mut row, Icon::Star, self.score, score, GOLD, a);
            }
            _ => {
                let retries = u32::from(self.retries);
                let after = retries.saturating_sub(1);
                self.delta(l, &mut row, Icon::Restart, retries, after, RED, a);
                let here = self.museum[self.index * 4..self.index * 4 + 4]
                    .iter()
                    .filter(|r| r.identified)
                    .count() as u32;
                self.delta(l, &mut row, Icon::Bone, here, 0, RED, a);
            }
        }
        if let Some(lack) = lack {
            let (glyph, text) = lack_badge(lack);
            let by = y + 354. - 52.;
            rounded(l, by, w - 44., 34., 9., with_alpha(RED, 0.14));
            outline(l, by, w - 44., 34., 9., 1., with_alpha(RED, 0.5));
            icon_value(glyph, &text, l + 14., by + 17., 17., RED);
        }
    }
    /// One "icon from -> to" line of a camp preview at `row`, which advances.
    #[allow(clippy::too_many_arguments)]
    fn delta(
        &self,
        x: f32,
        row: &mut f32,
        glyph: Icon,
        from: u32,
        to: u32,
        tone: Color,
        alpha: f32,
    ) {
        let cy = *row;
        *row += 44.;
        let tone = with_alpha(tone, alpha);
        icon(glyph, x + 14., cy, 22., tone);
        let w = say(
            &from.to_string(),
            x + 40.,
            cy,
            20.,
            with_alpha(DUST, tone.a),
        );
        icon(
            Icon::ArrowRight,
            x + 54. + w,
            cy,
            16.,
            with_alpha(DUST, tone.a),
        );
        let change = if to > from {
            GREEN
        } else if to < from {
            RED
        } else {
            BONE
        };
        say(
            &to.to_string(),
            x + 70. + w,
            cy,
            22.,
            with_alpha(change, tone.a),
        );
    }

    // ----- Museum and atlas -------------------------------------------------

    fn draw_museum(&self) {
        let t = clock();
        let open = self.phase == Phase::Complete;
        let (x, y, w, h) = (24., 182., 912., 358.);
        card(x, y, w, h, Color::new(0.15, 0.165, 0.14, 1.));
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
                y + 46.,
                1.,
                190.,
                Color::new(0., 0., 0., 0.12),
            );
        }
        let floor = Rect::new(x + 1., 432., w - 2., 12.);
        shapes::fill_rounded_rect(
            floor,
            0.,
            Fill::vertical(
                floor,
                Color::new(0., 0., 0., 0.25),
                Color::new(0., 0., 0., 0.),
            ),
        );
        say("The Dawn Giant", 44., 210., 24., BONE);
        let pieces = self.museum_pieces();
        if open {
            let w = 15. * 0.6 * 2. + 15. * 0.95 + 15. * 0.35 + measure("Open", 15.) + 4.;
            icon_chip(Icon::Trophy, "Open", 916. - w, 198., 15., GREEN);
        } else {
            let label = format!("{pieces}/6");
            let w = 15. * 0.6 * 2. + 15. * 0.95 + 15. * 0.35 + measure(&label, 15.) + 4.;
            icon_chip(Icon::Bone, &label, 916. - w, 198., 15., GOLD);
        }
        // Plinth and armature.
        let dy = 14.;
        rounded(
            90.,
            412. + dy,
            770.,
            8.,
            3.,
            Color::new(0.42, 0.34, 0.24, 1.),
        );
        panel(90., 412. + dy, 770., 2., Color::new(1., 0.92, 0.75, 0.18));
        for px in [240., 400., 620.] {
            draw_line(
                px,
                300. + dy,
                px,
                412. + dy,
                2.,
                Color::new(0.48, 0.41, 0.28, 1.),
            );
        }
        // Each identified signature specimen adds its section; the one this
        // permit seeks waits as a breathing outline.
        let at = vec2(150., 212. + dy);
        let beat = pulse(t, 0.8);
        let seeking = (!open).then_some(self.index);
        for i in 0..6 {
            if self.museum[i * 4].identified {
                let p = part_at(at, 1., i);
                glow(p.x, p.y, 70., with_alpha(BONE, 0.08));
            } else if seeking == Some(i) {
                let p = part_at(at, 1., i);
                glow(
                    p.x,
                    p.y,
                    60. + 8. * beat,
                    with_alpha(GOLD, 0.12 + 0.1 * beat),
                );
            }
        }
        let ghost = Color::new(0.27, 0.29, 0.24, 1.);
        let tones = core::array::from_fn(|i| {
            if self.museum[i * 4].identified {
                BONE
            } else if seeking == Some(i) {
                shapes::mix(ghost, GOLD, 0.25 + 0.2 * beat)
            } else {
                ghost
            }
        });
        skeleton(at, 1., tones);
        // Site placards.
        for (i, site) in SITES.iter().enumerate() {
            let r = placard(i);
            let [px, py, pw, ph] = r;
            let on = self.choice == i;
            let hover = !on && !open && self.phase == Phase::Field && hovered(r);
            row_surface(px, py, pw, ph, on, hover);
            if !on && !hover {
                outline(px, py, pw, ph, 8., 1., HAIRLINE);
            }
            let done = self.museum[i * 4].identified;
            label(
                &title_case(site.piece),
                px + 12.,
                py + 20.,
                96.,
                15.,
                if done { BONE } else { DUST },
            );
            if done {
                icon_badge(Icon::Check, px + pw - 18., py + 20., 18., GREEN, EARTH);
            } else if i == self.index && !open {
                icon(Icon::Pickaxe, px + pw - 18., py + 20., 15., GOLD);
            }
            for k in 0..4 {
                let c = vec2(px + 17. + k as f32 * 14., py + 44.);
                if self.museum[i * 4 + k].identified {
                    shapes::fill_circle(c, 4., BONE);
                } else {
                    shapes::stroke_circle(c, 3.5, 1.2, with_alpha(DUST, 0.5));
                }
            }
            for k in 0..3_u8 {
                icon(
                    Icon::Star,
                    px + 20. + f32::from(k) * 19.,
                    py + 63.,
                    14.,
                    if k < self.medals[i] {
                        GOLD
                    } else {
                        with_alpha(DIM, 0.55)
                    },
                );
            }
        }
    }
    fn draw_atlas(&self) {
        let field = self.phase == Phase::Field;
        for (i, species) in SPECIES.iter().enumerate() {
            let r = atlas_card(i);
            let [x, y, w, h] = r;
            let on = self.choice == i;
            let hover = !on && field && hovered(r);
            if on || hover {
                row_surface(x, y, w, h, on, hover);
            } else {
                slab(x, y, w, h);
            }
            displayed(i, x + 46., y + 50., 32., BONE);
            label(
                &title_case(species.name),
                x + 90.,
                y + 36.,
                w - 104.,
                17.,
                BONE,
            );
            icon_value(
                Icon::Coin,
                &species.value.to_string(),
                x + 90.,
                y + 64.,
                15.,
                GOLD,
            );
            divider(x + 14., y + 98., w - 28., HAIRLINE);
            for (k, line) in traits(i).enumerate() {
                let cy = y + 120. + k as f32 * 26.;
                shapes::fill_circle(vec2(x + 20., cy), 3., SAND);
                label(line, x + 32., cy, w - 46., 14., DUST);
            }
        }
    }

    // ----- Dialogs ----------------------------------------------------------

    fn draw_briefing(&self) {
        let (x, y, w, h) = (110., 176., 740., 352.);
        self.dialog(x, y, w, h, GOLD);
        let t = clock();
        let beat = pulse(t, 0.9);
        let l = x + 32.;
        say(&title_case(self.content().name), l, y + 50., 32., BONE);
        pips(x + w - 107., y + 50., self.site() as u32, 6, GOLD);
        // The goal: this site's section of the Giant, plus one more find.
        let s = 0.42;
        let at = vec2(l + 4., y + 94.);
        let part = self.index;
        let focus = part_at(at, s, part);
        glow(
            focus.x,
            focus.y,
            34. + 6. * beat,
            with_alpha(GOLD, 0.28 + 0.14 * beat),
        );
        skeleton(at, s, self.giant_tones(Some((part, GOLD)), GHOST));
        let gx = l + 340.;
        icon(Icon::Bone, gx + 12., y + 118., 24., GOLD);
        say(
            &title_case(self.content().piece),
            gx + 32.,
            y + 118.,
            26.,
            GOLD,
        );
        let w2 = say("+1", gx, y + 156., 22., BONE);
        icon(Icon::Bone, gx + w2 + 18., y + 156., 20., BONE);
        // The site: finds, days, daylight and rock.
        let fy = y + 222.;
        divider(l, fy - 28., w - 64., HAIRLINE);
        let mut fx = l;
        fx += icon_value(Icon::Bone, "4", fx, fy, 20., BONE) + 36.;
        icon(Icon::Calendar, fx + 10., fy, 20., BONE);
        fx += 26.;
        fx += pips(fx, fy, 1, 3, SAND) + 36.;
        fx += icon_value(
            Icon::Sun,
            &self.content().daylight.to_string(),
            fx,
            fy,
            20.,
            SUN,
        ) + 36.;
        icon(Icon::Hammer, fx + 10., fy, 20., DUST);
        hardness(fx + 26., fy, self.content().rock, SAND);
        for k in 0..4 {
            let shade = 0.95 - k as f32 * 0.17;
            panel(
                x + w - 92.,
                fy - 12. + k as f32 * 6.,
                60.,
                6.,
                self.soil(shade),
            );
        }
        // The three field controls.
        let mut kx = l;
        for (key, action) in [("X", "Survey"), ("C", "Tool"), ("Space", "Dig")] {
            kx += keycap(key, action, kx, fy + 30., BONE) + 22.;
        }
        // Start.
        let (bx, by, bw, bh) = (l, y + h - 64., w - 64., 44.);
        rounded_gradient(
            bx,
            by,
            bw,
            bh,
            11.,
            lighten(GOLD, 0.1 + 0.08 * beat),
            darken(GOLD, 0.1),
        );
        outline(bx, by, bw, bh, 11., 1., Color::new(1., 1., 1., 0.25));
        icon(Icon::Play, bx + 30., by + bh * 0.5, 18., EARTH);
        say("Start", bx + 48., by + bh * 0.5, 19., EARTH);
        keycap("Space", "", bx + bw - 62., by + 12., BONE);
    }
    fn draw_setback(&self) {
        let (x, y, w, h) = (150., 196., 660., 296.);
        self.dialog(x, y, w, h, RED);
        let t = clock();
        let beat = pulse(t, 1.2);
        let l = x + 32.;
        icon_badge(
            Icon::Alert,
            l + 22.,
            y + 50.,
            44.,
            with_alpha(RED, 0.22),
            RED,
        );
        say("Setback", l + 56., y + 50., 28., BONE);
        // What ended the dig: the signature bone, supplies or daylight.
        let sig = self.content().finds[0];
        let cell = self.cells[sig.y * COLS + sig.x];
        let (glyph, tone) = if cell.quality == 0 {
            (Icon::Bone, BONE)
        } else if self.kits == 0 {
            (Icon::Crate, BONE)
        } else {
            (Icon::Sun, SUN)
        };
        let cause = vec2(l + 50., y + 144.);
        icon(glyph, cause.x, cause.y, 60., with_alpha(tone, 0.8));
        icon(Icon::Cross, cause.x + 24., cause.y + 22., 28., RED);
        // Grants: the one this recovery spends pulses.
        icon(Icon::ArrowRight, l + 136., cause.y, 22., DUST);
        if self.retries == 0 {
            icon(Icon::Skull, l + 210., cause.y, 52., RED);
        }
        for k in 0..self.retries {
            let spend = k + 1 == self.retries;
            icon(
                Icon::Restart,
                l + 190. + f32::from(k) * 56.,
                cause.y,
                44.,
                if spend {
                    with_alpha(RED, 0.6 + 0.4 * beat)
                } else {
                    SAND
                },
            );
        }
        let [bx, by, bw, bh] = RECOVER_BUTTON;
        let lift = if hovered(RECOVER_BUTTON) { 0.2 } else { 0.1 };
        rounded_gradient(bx, by, bw, bh, 11., lighten(RED, lift), darken(RED, 0.15));
        outline(bx, by, bw, bh, 11., 1., Color::new(1., 1., 1., 0.25));
        let ink_color = Color::new(0.12, 0.05, 0.03, 1.);
        let (glyph, word) = if self.retries > 0 {
            (Icon::Restart, "Recover")
        } else {
            (Icon::Skull, "End career")
        };
        icon(glyph, bx + 30., by + bh * 0.5, 18., ink_color);
        say(word, bx + 48., by + bh * 0.5, 19., ink_color);
        keycap("Space", "", bx + bw - 62., by + 12., PAPER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn primary() -> Input {
        Input {
            primary: true,
            ..Input::default()
        }
    }
    #[test]
    fn progress_counts_permits_from_first_to_finished_campaign() {
        let mut g = Game::new();
        assert_eq!(ArcadeGame::progress(&g), Some((1, 6)));
        for tick in 0..20_000 {
            g.step(&g.pilot(tick));
            if g.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(g.outcome(), Outcome::Won);
        assert_eq!(ArcadeGame::progress(&g), Some((6, 6)));
        assert_eq!(g.hud().objective, "Museum open");
    }
    #[test]
    fn hud_names_the_site_and_keeps_the_objective_short() {
        let g = Game::new();
        let hud = g.hud();
        assert_eq!(hud.stage, "Riverbend");
        assert!(hud.objective.split_whitespace().count() <= 5);
        assert!(hud.objective.contains("skull"));
        assert!(g.summary().ends_with('.'));
        assert_eq!(g.summary().matches(". ").count(), 0, "one sentence");
    }
    #[test]
    fn accepted_digs_and_surveys_request_local_effects() {
        let mut g = Game::new();
        g.step(&primary());
        let survey = Input {
            secondary: true,
            ..Input::default()
        };
        let cue = g.step(&survey);
        let mut fx = Juice::new();
        g.effects(cue, &mut fx);
        // A sonar ring, one per revealed tile and a pulse on spent resources.
        let counts = fx.counts();
        assert_eq!(counts.rings, 1 + 3 + 2, "{counts:?}");
        // A repeated survey is refused quietly and costs nothing.
        let cue = g.step(&survey);
        let mut quiet = Juice::new();
        g.effects(cue, &mut quiet);
        assert_eq!(quiet.counts().popups, 0);
        assert_eq!(quiet.counts().rings, 1);
        // A rough cut into the Giant's skull bursts and reports the loss
        // instead of celebrating the reveal.
        let f = g.content().finds[0];
        g.step(&Input {
            click: true,
            pointer: tile_center(f.y * COLS + f.x),
            ..Input::default()
        });
        g.tool = 2;
        let cue = g.step(&primary());
        assert_eq!(cue, Cue::Bad);
        let mut hit = Juice::new();
        g.effects(cue, &mut hit);
        let counts = hit.counts();
        assert!(counts.sparks > 20, "{counts:?}");
        assert_eq!(counts.popups, 1, "only the condition loss");
        // Effects come only from the latest tick.
        let cue = g.step(&Input::default());
        let mut idle = Juice::new();
        g.effects(cue, &mut idle);
        assert!(idle.is_idle());
    }
    #[test]
    fn refused_purchases_point_at_the_missing_resource() {
        let mut g = Game::new();
        g.step(&primary());
        g.step(&Input {
            slot: Some(Page::Camp.index() + 1),
            ..Input::default()
        });
        // An untouched morning cannot be rested away.
        let cue = g.step(&primary());
        assert_eq!(cue, Cue::None);
        let mut fx = Juice::new();
        g.effects(cue, &mut fx);
        assert_eq!(fx.counts().popups, 1);
        assert!(fx.counts().rings >= 2);
    }
}
