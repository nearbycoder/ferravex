//! Presentation: a lit concert highway between a crowd panel and a song
//! panel, visual song cards and effects where each judgment lands. Drawing
//! only reads the model; the presentation clock and pointer drive idle
//! motion and hover highlights, never rules.
use crate::content::{LANE_KEYS, Lesson, TRACK_COUNT, TRACKS, Track};
use crate::model::{
    Event, Flash, Game, Judgment, Phase, Stats, on_action_button, on_encore_button, on_end_button,
    pointer_lane,
};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;
use std::f32::consts::{FRAC_PI_2, TAU};

const NIGHT: Color = Color::new(0.025, 0.035, 0.085, 1.);
const DUSK: Color = Color::new(0.07, 0.045, 0.16, 1.);
const PANEL: Color = Color::new(0.055, 0.06, 0.13, 1.);
const GOLD: Color = Color::new(1., 0.80, 0.38, 1.);
const SOFT: Color = Color::new(0.84, 0.87, 0.94, 1.);
/// Misses, empty taps and a fading crowd.
const ALARM: Color = Color::new(1., 0.36, 0.33, 1.);
/// The combo flame.
const EMBER: Color = Color::new(1., 0.58, 0.26, 1.);
/// Dark bulbs, spent tickets and other unlit states.
const DIM: Color = Color::new(0.19, 0.22, 0.35, 1.);
const LANE: [Color; 4] = [
    Color::new(0.98, 0.36, 0.60, 1.),
    Color::new(0.42, 0.84, 0.98, 1.),
    Color::new(0.65, 0.93, 0.40, 1.),
    Color::new(0.88, 0.55, 1., 1.),
];
const ARROWS: [Icon; 4] = [
    Icon::ArrowLeft,
    Icon::ArrowDown,
    Icon::ArrowUp,
    Icon::ArrowRight,
];
/// Perfect, Good, Okay, Miss and empty-tap tallies as icon and color.
const TALLY: [(Icon, Color); 5] = [
    (Icon::Gem, LANE[2]),
    (Icon::Check, LANE[1]),
    (Icon::Minus, GOLD),
    (Icon::Cross, ALARM),
    (Icon::Hand, MUTED),
];
/// Highway geometry shared with the model's pointer lanes (x 180..660).
const LANES_X: f32 = 180.;
const LANE_W: f32 = 120.;
const HIT_Y: f32 = 488.;
const SPEED: f32 = 3.1;
const TOP_Y: f32 = 150.;
/// Effect anchors, matching the panels drawn below.
const CROWD_AT: Vec2 = vec2(112., 200.);
const COMBO_AT: Vec2 = vec2(96., 376.);
const MULTIPLIER_AT: Vec2 = vec2(58., 414.);
const BOLT_AT: Vec2 = vec2(706., 426.);
const COUNT_AT: Vec2 = vec2(420., 318.);
const STAGE_AT: Vec2 = vec2(420., 300.);

fn shade(c: Color, f: f32) -> Color {
    Color::new(c.r * f, c.g * f, c.b * f, 1.)
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w, h)
}
/// Shadowless modern text with its baseline at `y`; returns the width.
fn ink(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, TextStyle::modern(size, color)).width
}
/// Modern text vertically centered on `cy`; returns the width.
fn ink_mid(value: &str, x: f32, cy: f32, size: f32, color: Color, align: TextAlign) -> f32 {
    let s = TextStyle::modern(size, color).with_align(align);
    draw_text_styled(value, x, s.centered_baseline(cy), s).width
}
/// Recessed track with a lit fill; a lighter-weight [`meter`].
fn bar(x: f32, y: f32, w: f32, h: f32, fraction: f32, color: Color) {
    let f = if fraction.is_finite() {
        fraction.clamp(0., 1.)
    } else {
        0.
    };
    let radius = (h * 0.5).min(4.);
    shapes::fill_rounded_rect(rect(x, y, w, h), radius, Color::new(0.01, 0.015, 0.04, 0.7));
    if f > 0. {
        let fill = rect(x, y, (w * f).max(h), h);
        shapes::fill_rounded_rect(
            fill,
            radius,
            Fill::vertical(fill, lighten(color, 0.25), color),
        );
    }
}
/// Soft light pool drawn with normal blending (no extra draw call).
fn pool(x: f32, y: f32, radius: f32, color: Color) {
    let c = vec2(x, y);
    shapes::fill_circle(
        c,
        radius,
        Fill::radial(c, radius, color, with_alpha(color, 0.)),
    );
}
fn percent(value: u32) -> String {
    format!("{}.{:01}%", value / 10, value % 10)
}
/// `9,550`-style digit groups.
fn grouped(value: u32) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
/// `m:ss` for a tick count, rounded up so the last second still shows.
fn minutes(ticks: u32) -> String {
    let seconds = ticks.div_ceil(60);
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
fn judgment_color(flash: Flash) -> Color {
    match flash {
        Flash::Perfect => LANE[2],
        Flash::Good => LANE[1],
        Flash::Okay => GOLD,
        Flash::Miss | Flash::Stray => ALARM,
        Flash::None => SOFT,
    }
}
fn grade_color(grade: &str) -> Color {
    match grade {
        "S+" | "S" => GOLD,
        "A" => LANE[2],
        "B" => LANE[1],
        "C" => LANE[3],
        _ => ALARM,
    }
}
/// Perfect, Good, Okay, Miss and empty-tap counts in [`TALLY`] order.
fn tallies(stats: Stats) -> [u32; 5] {
    [
        stats.perfect,
        stats.good,
        stats.okay,
        stats.misses,
        stats.stray,
    ]
}
/// Center of a lane's target socket on the judgment line.
fn socket(lane: usize) -> Vec2 {
    vec2(LANES_X + LANE_W * (lane as f32 + 0.5), HIT_Y)
}
/// Retry ticket `i` in the right panel.
fn ticket_at(i: usize) -> Vec2 {
    vec2(738. + i as f32 * 28., 511.)
}
/// District bulb `i` on the song results card.
fn result_lamp(i: usize) -> Vec2 {
    vec2(190. + i as f32 * 116., 438.)
}
/// Lightweight raised surface: lit gradient and hairline rim, no blur pass.
fn slab(x: f32, y: f32, w: f32, h: f32) {
    let r = rect(x, y, w, h);
    shapes::fill_rounded_rect(r, 12., Fill::vertical(r, lighten(PANEL, 0.04), PANEL));
    shapes::stroke_rounded_rect(r, 12., 1., Color::new(1., 1., 1., 0.07));
}
fn rule(x: f32, y: f32, w: f32) {
    divider(x, y, w, Color::new(1., 1., 1., 0.12));
}
/// A small key cap; `alpha` dims keys that do nothing right now. Returns its width.
fn cap(key: &str, x: f32, y: f32, alpha: f32, tint: Color) -> f32 {
    let s = TextStyle::modern(13., with_alpha(tint, alpha)).with_align(TextAlign::Center);
    let w = (text_width(key, s) + 12.).max(20.);
    shapes::fill_rounded_rect(
        rect(x, y + 2., w, 20.),
        4.,
        Color::new(0., 0., 0., 0.5 * alpha),
    );
    let r = rect(x, y, w, 20.);
    shapes::fill_rounded_rect(
        r,
        4.,
        Fill::vertical(
            r,
            Color::new(0.2, 0.24, 0.3, alpha),
            Color::new(0.13, 0.16, 0.21, alpha),
        ),
    );
    shapes::stroke_rounded_rect(r, 4., 1., with_alpha(tint, 0.3 * alpha));
    draw_text_styled(key, x + w * 0.5, s.centered_baseline(y + 10.), s);
    w
}
/// A district bulb: restored districts glow, the current one breathes.
#[derive(Clone, Copy, PartialEq)]
enum Lamp {
    Dark,
    Next,
    Lit,
}
fn lamp(at: Vec2, size: f32, state: Lamp) {
    match state {
        Lamp::Lit => {
            pool(at.x, at.y, size * 1.25, with_alpha(GOLD, 0.32));
            icon(Icon::Bulb, at.x, at.y, size, GOLD);
        }
        Lamp::Next => {
            let breath = juice::pulse(clock(), 0.7);
            pool(at.x, at.y, size * 1.1, with_alpha(SOFT, 0.1 * breath));
            icon(
                Icon::Bulb,
                at.x,
                at.y,
                size,
                with_alpha(SOFT, 0.4 + 0.5 * breath),
            );
        }
        Lamp::Dark => icon(Icon::Bulb, at.x, at.y, size, DIM),
    }
}
/// A lane key in miniature (arrow and letter) lit by `glow` in 0..=1, as
/// on the song card's demo and lesson hints.
fn mini_key(lane: usize, x: f32, y: f32, w: f32, glow: f32) {
    let color = LANE[lane];
    let glow = glow.clamp(0., 1.);
    let r = rect(x, y + 2. * glow, w, 40.);
    shapes::fill_rounded_rect(
        r,
        9.,
        Fill::vertical(
            r,
            shapes::mix(shade(color, 0.22), color, 0.55 * glow),
            shade(color, 0.12),
        ),
    );
    if glow > 0. {
        shapes::stroke_rounded_rect(r, 9., 1.5, with_alpha(color, 0.5 + 0.5 * glow));
    }
    let cy = r.y + 20.;
    icon(
        ARROWS[lane],
        r.x + r.w * 0.5 - 12.,
        cy,
        18.,
        if glow > 0.5 { PAPER } else { color },
    );
    cap(LANE_KEYS[lane], r.x + r.w * 0.5 + 2., cy - 10., 1., color);
}
/// The card's primary button inside the model's click region; it lifts and
/// brightens under the pointer.
fn action_button(symbol: Icon, label: &str, color: Color) {
    let hover = on_action_button(pointer());
    let lift = if hover { 2. } else { 0. };
    let r = rect(240., 491. - lift, 480., 43.);
    shapes::fill_rounded_rect(rect(r.x, 494., r.w, r.h), 12., darken(color, 0.6));
    shapes::fill_rounded_rect(
        r,
        12.,
        Fill::vertical(
            r,
            lighten(color, if hover { 0.34 } else { 0.2 }),
            darken(color, 0.08),
        ),
    );
    shapes::stroke_rounded_rect(
        r,
        12.,
        1.,
        Color::new(1., 1., 1., if hover { 0.6 } else { 0.3 }),
    );
    let s = TextStyle::modern(18., INK);
    let label_w = text_width(label, s);
    let key_w = text_width("Space", TextStyle::modern(13., PAPER)) + 12.;
    let left = 480. - (20. + label_w + 12. + key_w) * 0.5;
    let cy = r.y + r.h * 0.5;
    icon(symbol, left + 8., cy, 18., INK);
    draw_text_styled(label, left + 20., s.centered_baseline(cy), s);
    keycap("Space", "", left + 20. + label_w + 12., cy - 10., PAPER);
}

impl Game {
    fn chart_time(&self) -> i32 {
        if self.phase == Phase::CountIn {
            self.clock as i32 - TRACKS[self.track].beat_ticks as i32 * 4
        } else {
            self.clock as i32
        }
    }
    fn live(&self) -> bool {
        matches!(self.phase, Phase::Playing | Phase::CountIn)
    }
    /// 1 on each beat, easing to 0 before the next; follows song ticks only.
    fn beat_pulse(&self) -> f32 {
        let beat = TRACKS[self.track].beat_ticks as i32;
        let t = self.chart_time().rem_euclid(beat) as f32 / beat as f32;
        (1. - t) * (1. - t)
    }
    /// The song's beat while it plays; on cards the crowd keeps the tempo
    /// from the presentation clock instead.
    fn groove(&self) -> f32 {
        if self.live() {
            return self.beat_pulse();
        }
        let t = (clock() * TRACKS[self.track].bpm() as f32 / 60.).fract();
        (1. - t) * (1. - t)
    }
    /// 1 right after an accepted note, easing to 0 within a few ticks.
    fn hit_pop(&self) -> f32 {
        self.flashes
            .iter()
            .zip(self.flash_ticks)
            .filter(|(flash, _)| matches!(flash, Flash::Perfect | Flash::Good | Flash::Okay))
            .map(|(_, ticks)| ((f32::from(ticks) / 22. - 0.7) / 0.3).max(0.))
            .fold(0., f32::max)
    }
    /// Every banked song's judgments together, for the concert grade.
    fn concert_stats(&self) -> Stats {
        self.results
            .iter()
            .flatten()
            .fold(Stats::default(), |mut total, song| {
                total.perfect += song.perfect;
                total.good += song.good;
                total.okay += song.okay;
                total.misses += song.misses;
                total.stray += song.stray;
                total.score += song.score;
                total.max_combo = total.max_combo.max(song.max_combo);
                total
            })
    }
    fn lamp_state(&self, i: usize) -> Lamp {
        if self.results[i].is_some() {
            Lamp::Lit
        } else if i == self.track {
            Lamp::Next
        } else {
            Lamp::Dark
        }
    }
    /// District bulbs `from..to` hung on a sagging wire through `y`.
    fn lamp_row(&self, from: usize, to: usize, x: f32, y: f32, spacing: f32, size: f32) {
        let wire = Color::new(1., 1., 1., 0.14);
        let top = y - size * 0.42;
        for i in from..to.saturating_sub(1) {
            let a = vec2(x + (i - from) as f32 * spacing, top);
            let b = a + vec2(spacing, 0.);
            let mid = (a + b) * 0.5 + vec2(0., size * 0.28);
            draw_line(a.x, a.y, mid.x, mid.y, 1.2, wire);
            draw_line(mid.x, mid.y, b.x, b.y, 1.2, wire);
        }
        for i in from..to {
            let at = vec2(x + (i - from) as f32 * spacing, y);
            lamp(at, size, self.lamp_state(i));
        }
    }
    fn draw_city(&self) {
        let area = rect(16., 112., 928., 456.);
        shapes::fill_rounded_rect(area, 14., Fill::vertical(area, DUSK, NIGHT));
        let haze = rect(16., 380., 928., 188.);
        shapes::fill_rounded_rect(
            haze,
            14.,
            Fill::vertical(haze, with_alpha(LANE[3], 0.), with_alpha(LANE[3], 0.09)),
        );
        // The skyline is authored geometry, with one district illuminated per clear.
        let lit_count = self.completed_tracks();
        let twinkle = self.stage_ticks / 24;
        for n in 0..26 {
            let x = 18. + n as f32 * 36.;
            let height = 35. + ((n * 31 + 17) % 89) as f32;
            let lit = n / 5 < lit_count;
            let tone = LANE[(n / 6) % 4];
            let hidden = (20. ..=135.).contains(&x)
                || (170. ..=637.).contains(&x)
                || (672. ..=907.).contains(&x);
            if hidden {
                // Only the base peeks out below the cards and highway.
                panel(x, 552., 29., 14., shade(tone, 0.12));
            } else {
                panel(x, 566. - height, 29., height, shade(tone, 0.12));
                panel(x, 566. - height, 29., 2., shade(tone, 0.32));
            }
            for row in 0..((height as usize / 12).saturating_sub(1)) {
                for col in 0..2 {
                    let wx = x + 6. + col as f32 * 13.;
                    let wy = 558. - row as f32 * 12.;
                    // Windows behind the side cards and highway are never seen.
                    let covered = wy < 552.
                        && ((20. ..164.).contains(&wx)
                            || (170. ..666.).contains(&wx)
                            || (672. ..936.).contains(&wx));
                    if covered {
                        continue;
                    }
                    let seed = n * 7 + row * 3 + col;
                    let on = lit && !(seed as u32 + twinkle).is_multiple_of(9);
                    panel(
                        wx,
                        wy,
                        5.,
                        4.,
                        if on {
                            with_alpha(GOLD, 0.55 + 0.1 * (seed % 3) as f32)
                        } else {
                            Color::new(0.14, 0.17, 0.3, 0.8)
                        },
                    );
                }
            }
        }
    }
    fn draw_lanes(&self) {
        let now = self.chart_time();
        let track = &TRACKS[self.track];
        let pulse = self.beat_pulse();
        let encore = self.encore();
        let hovered = if self.live() {
            pointer_lane(pointer())
        } else {
            None
        };
        let highway = rect(174., 146., 492., 414.);
        shapes::fill_rounded_rect(
            highway,
            14.,
            Fill::vertical(
                highway,
                Color::new(0.02, 0.025, 0.06, 1.),
                Color::new(0.03, 0.03, 0.08, 1.),
            ),
        );
        for (lane, &color) in LANE.iter().enumerate() {
            let x = LANES_X + lane as f32 * LANE_W;
            let r = rect(x, TOP_Y, LANE_W, HIT_Y - TOP_Y + 10.);
            let tint = if encore { GOLD } else { color };
            let lift = if hovered == Some(lane) { 0.1 } else { 0. };
            shapes::fill_rounded_rect(
                r,
                0.,
                Fill::vertical(r, with_alpha(tint, 0.), with_alpha(tint, 0.13 + lift)),
            );
            if lane > 0 {
                draw_line(x, TOP_Y, x, 498., 1., Color::new(1., 1., 1., 0.06));
            }
        }
        // Beat grid scrolls with the actual song tick, not wall time.
        for beat in 0..=track.bars() * 4 + 2 {
            let tick = beat * track.beat_ticks;
            let y = HIT_Y - (tick as i32 - now) as f32 * SPEED;
            if (TOP_Y + 2. ..=HIT_Y - 4.).contains(&y) {
                let bar = beat.is_multiple_of(4);
                draw_line(
                    LANES_X,
                    y,
                    LANES_X + LANE_W * 4.,
                    y,
                    if bar { 2. } else { 1. },
                    Color::new(1., 1., 1., if bar { 0.1 } else { 0.045 }),
                );
            }
        }
        // Speed streaks grow with the combo multiplier and during Encore.
        let energy = if encore {
            1.
        } else {
            (self.combo / 20).min(4) as f32 / 4.
        };
        if self.phase == Phase::Playing && energy > 0. {
            for i in 0..12_usize {
                let lane = i % 4;
                let seed = (i * 73 + 19) % 97;
                let x = LANES_X + lane as f32 * LANE_W + 10. + (seed % 100) as f32;
                let y = TOP_Y + (now as f32 * SPEED * 1.7 + seed as f32 * 37.) % 300.;
                let r = rect(x, y, 2., 34.);
                let c = if encore { GOLD } else { LANE[lane] };
                shapes::fill_rounded_rect(
                    r,
                    1.,
                    Fill::vertical(r, with_alpha(c, 0.), with_alpha(c, 0.1 + 0.25 * energy)),
                );
            }
        }
        // Target sockets on the judgment line.
        for (lane, &color) in LANE.iter().enumerate() {
            let x = LANES_X + 12. + lane as f32 * LANE_W;
            let lit = if self.flash_ticks[lane] > 0 { 0.4 } else { 0. };
            shapes::fill_rounded_rect(
                rect(x, HIT_Y - 9., 96., 18.),
                9.,
                with_alpha(color, 0.12 + 0.08 * pulse + lit * 0.5),
            );
        }
        // Judgment light sits under the notes so the next note stays crisp.
        self.draw_hits();
        // Chord connectors are visual instructions; each lane is judged separately.
        let mut index = 0;
        while index < self.len {
            let target = self.notes[index].tick;
            let mut end = index + 1;
            while end < self.len && self.notes[end].tick == target {
                end += 1;
            }
            let mut pending = self.notes[index..end]
                .iter()
                .filter(|n| n.result == Judgment::Pending);
            let first = pending.next();
            let last = pending.next_back();
            if let (Some(first), Some(last)) = (first, last) {
                let y = HIT_Y - (target as i32 - now) as f32 * SPEED;
                if (158. ..=494.).contains(&y) {
                    let left = 240. + first.lane as f32 * LANE_W;
                    let right = 240. + last.lane as f32 * LANE_W;
                    shapes::fill_capsule(
                        vec2(left, y),
                        vec2(right, y),
                        3.,
                        Color::new(1., 1., 1., 0.8),
                    );
                }
            }
            index = end;
        }
        for note in &self.notes[..self.len] {
            if note.result != Judgment::Pending {
                continue;
            }
            let y = HIT_Y - (note.tick as i32 - now) as f32 * SPEED;
            if !(TOP_Y + 10. ..=512.).contains(&y) {
                continue;
            }
            let x = LANES_X + 12. + note.lane as f32 * LANE_W;
            let color = if encore {
                lighten(LANE[note.lane], 0.15)
            } else {
                LANE[note.lane]
            };
            let near = ((y - 360.) / 128.).clamp(0., 1.);
            if near > 0. {
                shapes::fill_rounded_rect(
                    rect(x - 3., y - 12., 102., 24.),
                    12.,
                    with_alpha(color, 0.4 * near),
                );
            } else {
                panel(x + 6., y + 6., 84., 4., Color::new(0., 0., 0., 0.35));
            }
            let body = rect(x, y - 9., 96., 18.);
            shapes::fill_rounded_rect(
                body,
                9.,
                Fill::vertical(body, lighten(color, 0.5), darken(color, 0.12)),
            );
            panel(x + 12., y - 6., 72., 2., Color::new(1., 1., 1., 0.5));
            icon(
                ARROWS[note.lane],
                x + 48.,
                y + 0.5,
                13.,
                Color::new(0.03, 0.03, 0.09, 0.85),
            );
        }
        // Notes fade in under the top of the highway.
        let fade = rect(LANES_X, TOP_Y, LANE_W * 4., 44.);
        shapes::fill_rounded_rect(
            fade,
            0.,
            Fill::vertical(
                fade,
                Color::new(0.02, 0.025, 0.06, 1.),
                Color::new(0.02, 0.025, 0.06, 0.),
            ),
        );
        // The judgment line breathes on the beat.
        let band = rect(LANES_X, HIT_Y - 22., LANE_W * 4., 22.);
        shapes::fill_rounded_rect(
            band,
            0.,
            Fill::vertical(
                band,
                Color::new(1., 1., 1., 0.),
                Color::new(1., 1., 1., 0.06 + 0.1 * pulse),
            ),
        );
        let under = rect(LANES_X, HIT_Y, LANE_W * 4., 12.);
        shapes::fill_rounded_rect(
            under,
            0.,
            Fill::vertical(
                under,
                Color::new(1., 1., 1., 0.06 + 0.06 * pulse),
                Color::new(1., 1., 1., 0.),
            ),
        );
        shapes::fill_rounded_rect(
            rect(LANES_X, HIT_Y - 2., LANE_W * 4., 4.),
            2.,
            Color::new(1., 1., 1., 0.82 + 0.18 * pulse),
        );
        self.draw_keys(hovered);
        if self.phase == Phase::CountIn {
            self.draw_count_in();
        }
        if encore {
            let left =
                self.encore_until.saturating_sub(self.clock) as f32 / (track.beat_ticks * 8) as f32;
            shapes::stroke_rounded_rect(highway, 14., 2., with_alpha(GOLD, 0.55 + 0.4 * pulse));
            // A draining bolt meter above the highway.
            let tab = rect(352., 117., 136., 24.);
            shapes::fill_rounded_rect(tab, 12., with_alpha(GOLD, 0.18 + 0.08 * pulse));
            icon(Icon::Bolt, 368., 129., 16., GOLD);
            bar(382., 126., 94., 6., left, GOLD);
        } else {
            shapes::stroke_rounded_rect(highway, 14., 1., Color::new(1., 1., 1., 0.08));
        }
    }
    /// Accepted judgments: rising beams, rings and one-word labels; a miss
    /// flashes its whole lane red and an empty tap crosses its socket.
    fn draw_hits(&self) {
        for (lane, &tone) in LANE.iter().enumerate() {
            let ticks = self.flash_ticks[lane];
            if ticks == 0 {
                continue;
            }
            let t = f32::from(ticks) / 22.;
            let flash = self.flashes[lane];
            let x = LANES_X + lane as f32 * LANE_W;
            let cx = x + LANE_W * 0.5;
            match flash {
                Flash::Perfect | Flash::Good | Flash::Okay => {
                    let beam = rect(x + 4., HIT_Y - 190. * t, LANE_W - 8., 190. * t);
                    shapes::fill_rounded_rect(
                        beam,
                        0.,
                        Fill::vertical(beam, with_alpha(tone, 0.), with_alpha(tone, 0.38 * t)),
                    );
                    let grow = (1. - t) * 14.;
                    shapes::stroke_rounded_rect(
                        rect(
                            x + 12. - grow,
                            HIT_Y - 9. - grow * 0.6,
                            96. + grow * 2.,
                            18. + grow * 1.2,
                        ),
                        9. + grow * 0.6,
                        2.,
                        with_alpha(lighten(tone, 0.4), 0.9 * t),
                    );
                    if flash == Flash::Perfect {
                        pool(cx, HIT_Y, 44., with_alpha(tone, 0.45 * t));
                    }
                }
                Flash::Miss => {
                    let column = rect(x + 2., TOP_Y, LANE_W - 4., HIT_Y - TOP_Y + 10.);
                    shapes::fill_rounded_rect(
                        column,
                        0.,
                        Fill::vertical(
                            column,
                            with_alpha(ALARM, 0.1 * t),
                            with_alpha(ALARM, 0.42 * t),
                        ),
                    );
                    shapes::fill_rounded_rect(
                        rect(x + 12., HIT_Y - 9., 96., 18.),
                        9.,
                        with_alpha(ALARM, 0.4 * t),
                    );
                }
                Flash::Stray => {
                    shapes::stroke_rounded_rect(
                        rect(x + 12., HIT_Y - 9., 96., 18.),
                        9.,
                        2.,
                        with_alpha(ALARM, 0.9 * t),
                    );
                    let size = 18. * (1. + 0.4 * (1. - t));
                    icon(
                        Icon::Cross,
                        cx,
                        HIT_Y - 24. - (1. - t) * 10.,
                        size,
                        with_alpha(ALARM, (t * 2.5).min(1.)),
                    );
                }
                Flash::None => {}
            }
            let word = flash.label();
            if word.is_empty() {
                continue;
            }
            // Words pop in large, settle, rise a little and fade quickly so
            // the reading zone above the line stays clear.
            let age = 1. - t;
            let scale = 1. + 0.35 * (1. - (age / 0.2).min(1.));
            let alpha = (t * 3.).min(1.);
            let color = judgment_color(flash);
            draw_text_styled(
                word,
                cx,
                HIT_Y - 24. - age * 16.,
                TextStyle::modern(15. * scale, with_alpha(lighten(color, 0.25), alpha))
                    .with_shadow(Color::new(0., 0., 0., 0.6 * alpha))
                    .with_align(TextAlign::Center)
                    .with_pixel_snap(false),
            );
        }
    }
    /// Lane keys: an arrow and its letter key; they sink when struck, lift
    /// under the pointer and redden after an empty tap.
    fn draw_keys(&self, hovered: Option<usize>) {
        for (lane, &color) in LANE.iter().enumerate() {
            let x = LANES_X + 4. + lane as f32 * LANE_W;
            let flash = self.flashes[lane];
            let pressed = self.flash_ticks[lane] > 0 && flash != Flash::Miss;
            let glow_t = if pressed {
                f32::from(self.flash_ticks[lane]) / 22.
            } else {
                0.
            };
            let tone = if flash == Flash::Stray && pressed {
                ALARM
            } else {
                color
            };
            let hover = hovered == Some(lane);
            let dy = if pressed {
                2. * glow_t
            } else if hover {
                -2.
            } else {
                0.
            };
            let r = rect(x, 506. + dy, 112., 46.);
            let (top, bottom) = if pressed {
                (
                    shapes::mix(shade(tone, 0.3), tone, 0.5 * glow_t),
                    shade(tone, 0.18),
                )
            } else if hover {
                (shade(color, 0.32), shade(color, 0.14))
            } else {
                (shade(color, 0.2), shade(color, 0.1))
            };
            shapes::fill_rounded_rect(r, 10., Fill::vertical(r, top, bottom));
            if pressed || hover {
                shapes::stroke_rounded_rect(
                    r,
                    10.,
                    1.5,
                    with_alpha(tone, if pressed { 0.5 + 0.5 * glow_t } else { 0.5 }),
                );
            }
            panel(
                x + 14.,
                r.y + 1.,
                84.,
                2.,
                with_alpha(color, 0.6 + 0.4 * glow_t),
            );
            let cy = r.y + 23.;
            icon(
                ARROWS[lane],
                x + 42.,
                cy,
                24.,
                if pressed { PAPER } else { color },
            );
            cap(LANE_KEYS[lane], x + 62., cy - 10., 1., color);
        }
    }
    /// Four count-in beats: a big number whose ring fills once per beat.
    fn draw_count_in(&self) {
        let beat_ticks = TRACKS[self.track].beat_ticks;
        let beat = self.clock / beat_ticks;
        let remaining = 4u32.saturating_sub(beat);
        let pulse = self.beat_pulse();
        shapes::fill_circle(COUNT_AT, 52., with_alpha(PANEL, 0.86));
        ring_meter(
            COUNT_AT.x,
            COUNT_AT.y,
            52.,
            4.,
            (self.clock % beat_ticks) as f32 / beat_ticks as f32,
            with_alpha(GOLD, 0.85),
        );
        let s = TextStyle::modern(50. * (1. + 0.14 * pulse), GOLD)
            .with_shadow(Color::new(0., 0., 0., 0.5))
            .with_align(TextAlign::Center)
            .with_pixel_snap(false);
        draw_text_styled(
            &remaining.to_string(),
            COUNT_AT.x,
            s.centered_baseline(COUNT_AT.y - 6.),
            s,
        );
        for i in 0..4 {
            let at = vec2(COUNT_AT.x - 21. + i as f32 * 14., COUNT_AT.y + 30.);
            if i < beat.min(4) {
                shapes::fill_circle(at, 4., GOLD);
            } else {
                shapes::stroke_circle(at, 3.5, 1.5, with_alpha(GOLD, 0.45));
            }
        }
    }
    fn draw_left(&self) {
        slab(24., 124., 140., 432.);
        self.draw_crowd();
        rule(36., 336., 116.);
        self.draw_combo();
        rule(36., 446., 116.);
        self.lamp_row(0, 3, 54., 482., 40., 26.);
        self.lamp_row(3, 6, 54., 526., 40., 26.);
    }
    /// Crowd energy: a people count, a vertical meter with its danger line
    /// and a little audience that lights up, bounces and roars.
    fn draw_crowd(&self) {
        let energy = self.crowd.clamp(0, 100);
        let danger = energy < 25;
        let alarm = if danger && self.live() {
            juice::pulse(clock(), 2.)
        } else {
            0.
        };
        let value_color = if danger {
            shapes::mix(GOLD, ALARM, alarm)
        } else {
            PAPER
        };
        icon_value(
            Icon::People,
            &format!("{energy}%"),
            40.,
            148.,
            18.,
            value_color,
        );
        let track = rect(40., 168., 18., 154.);
        shapes::fill_rounded_rect(track, 6., Color::new(0.01, 0.015, 0.04, 0.8));
        if energy > 0 {
            let h = (track.h * energy as f32 / 100.).max(12.);
            let fill = rect(track.x, track.y + track.h - h, track.w, h);
            let (top, bottom) = if danger {
                (lighten(ALARM, 0.2), ALARM)
            } else {
                (lighten(LANE[2], 0.25), shade(LANE[2], 0.7))
            };
            shapes::fill_rounded_rect(fill, 6., Fill::vertical(fill, top, bottom));
            panel(
                track.x + 4.,
                fill.y + 3.,
                3.,
                (h - 8.).max(0.),
                Color::new(1., 1., 1., 0.25),
            );
        }
        // The danger line at a quarter of the meter.
        panel(
            track.x - 3.,
            track.y + track.h * 0.75,
            track.w + 6.,
            1.5,
            with_alpha(ALARM, 0.75),
        );
        if alarm > 0. {
            shapes::stroke_rounded_rect(track, 6., 1.5, with_alpha(ALARM, 0.4 + 0.5 * alarm));
        }
        self.draw_audience(energy, danger);
    }
    /// Twelve fans, front rows first; lit fans follow the crowd energy.
    fn draw_audience(&self, energy: i32, danger: bool) {
        let lit = (energy as usize * 12).div_ceil(100);
        let groove = self.groove();
        let roar = energy >= 70;
        let amplitude = if danger {
            0.6
        } else if roar {
            4.
        } else {
            2.5
        };
        for row in 0..4 {
            for col in 0..3 {
                let rank = (3 - row) * 3 + col;
                let on = rank < lit;
                let x = 84. + col as f32 * 24. + if row % 2 == 1 { 12. } else { 0. };
                let swing = if (col + row) % 2 == 0 { 1. } else { 0.6 };
                let bounce = if on { amplitude * groove * swing } else { 0. };
                let slump = if danger && on { 2. } else { 0. };
                let y = 188. + row as f32 * 34. - bounce + slump;
                let color = if !on {
                    DIM
                } else if danger {
                    ALARM
                } else {
                    LANE[(rank * 3 + row) % 4]
                };
                if on && roar && groove > 0.35 {
                    let reach = 3. + 3. * groove;
                    let arm = shade(color, 0.8);
                    draw_line(x - 5., y + 8., x - 9., y - reach, 1.6, arm);
                    draw_line(x + 5., y + 8., x + 9., y - reach, 1.6, arm);
                }
                shapes::fill_circle(vec2(x, y + 13.), 8., shade(color, 0.45));
                shapes::fill_circle(vec2(x, y), 5.5, color);
            }
        }
    }
    /// Combo: a flame and a big count that pops on each hit, then the
    /// multiplier chip and its progress toward the next step.
    fn draw_combo(&self) {
        let multiplier = 1 + (self.combo / 20).min(4);
        let hot = self.combo > 0;
        let pop = self.hit_pop();
        let flame = if hot {
            shapes::mix(EMBER, GOLD, (multiplier - 1) as f32 / 4.)
        } else {
            DIM
        };
        let flicker = if hot {
            1. + 0.06 * (clock() * 11.).sin()
        } else {
            1.
        };
        if multiplier >= 3 {
            pool(
                COMBO_AT.x - 44.,
                COMBO_AT.y,
                24.,
                with_alpha(EMBER, 0.12 * multiplier as f32),
            );
        }
        icon(
            Icon::Flame,
            COMBO_AT.x - 44.,
            COMBO_AT.y,
            26. * flicker,
            flame,
        );
        let s = TextStyle::modern(36. * (1. + 0.18 * pop), if hot { PAPER } else { MUTED })
            .with_shadow(Color::new(0., 0., 0., 0.5))
            .with_pixel_snap(pop <= 0.);
        draw_text_styled(
            &self.combo.to_string(),
            COMBO_AT.x - 26.,
            s.centered_baseline(COMBO_AT.y),
            s,
        );
        let w = chip(
            &format!("x{multiplier}"),
            40.,
            MULTIPLIER_AT.y - 10.,
            14.,
            if multiplier > 1 { GOLD } else { MUTED },
        );
        let progress = if multiplier == 5 {
            1.
        } else {
            (self.combo % 20) as f32 / 20.
        };
        bar(48. + w, MULTIPLIER_AT.y - 3., 100. - w, 6., progress, GOLD);
    }
    fn draw_right(&self) {
        let track = &TRACKS[self.track];
        let accent = LANE[self.track % 4];
        slab(676., 124., 260., 432.);
        // The song: name, district, progress, the bar's beats and time left.
        icon(Icon::Music, 702., 148., 18., accent);
        fit(track.title, 718., 156., 202., 22., GOLD);
        icon(Icon::Pin, 701., 175., 12., MUTED);
        ink(track.venue, 712., 180., 13., MUTED);
        let played = (self.chart_time().max(0) as f32 / track.length() as f32).clamp(0., 1.);
        bar(692., 194., 228., 6., played, accent);
        let beat_ticks = track.beat_ticks as i32;
        let in_bar = (self.chart_time().rem_euclid(beat_ticks * 4) / beat_ticks) as usize;
        let pulse = self.beat_pulse();
        for i in 0..4 {
            let at = vec2(698. + i as f32 * 13., 220.);
            if self.live() && i == in_bar {
                shapes::fill_circle(at, 3.5 + 1.5 * pulse, lighten(accent, 0.35 * pulse));
            } else {
                shapes::fill_circle(at, 3., DIM);
            }
        }
        ink(&format!("{} BPM", track.bpm()), 752., 225., 12., SOFT);
        let total = track.length() + 2 * track.beat_ticks;
        let left = total.saturating_sub(self.chart_time().max(0) as u32);
        let w = ink_mid(&minutes(left), 920., 220., 12., MUTED, TextAlign::Right);
        icon(Icon::Hourglass, 912. - w, 220., 12., MUTED);
        rule(692., 242., 228.);
        self.draw_accuracy(track);
        rule(692., 390., 228.);
        self.draw_encore();
        rule(692., 466., 228.);
        self.draw_tickets();
    }
    /// Accuracy ring with its gold goal tick, judgment tallies and the
    /// early/late gauge of the latest note.
    fn draw_accuracy(&self, track: &Track) {
        let accuracy = self.accuracy();
        let center = vec2(744., 304.);
        let tone = if accuracy >= track.target {
            LANE[2]
        } else {
            ALARM
        };
        shapes::fill_circle(center, 36., Color::new(0.01, 0.015, 0.04, 0.45));
        ring_meter(center.x, center.y, 40., 8., accuracy as f32 / 1000., tone);
        let angle = -FRAC_PI_2 + track.target as f32 / 1000. * TAU;
        let dir = vec2(angle.cos(), angle.sin());
        shapes::fill_capsule(center + dir * 33., center + dir * 48., 2., GOLD);
        ink_mid(
            &percent(accuracy),
            center.x,
            center.y,
            15.,
            PAPER,
            TextAlign::Center,
        );
        let counts = tallies(self.attempt);
        let judged = self.attempt.judged().max(1) as f32;
        for (i, (&(symbol, color), count)) in TALLY.iter().zip(counts).enumerate() {
            let cy = 268. + i as f32 * 18.;
            icon(symbol, 812., cy, 13., color);
            ink_mid(
                &count.to_string(),
                824.,
                cy,
                14.,
                if count > 0 { PAPER } else { MUTED },
                TextAlign::Left,
            );
            bar(866., cy - 2., 54., 4., count as f32 / judged, color);
        }
        self.draw_timing(708., 370., 200.);
    }
    /// Early/late gauge for the most recent accepted note.
    fn draw_timing(&self, x: f32, y: f32, w: f32) {
        use crate::model::{GOOD, PERFECT, WINDOW};
        let cx = x + w * 0.5;
        let unit = w * 0.5 / (WINDOW as f32 + 1.);
        shapes::fill_rounded_rect(rect(x, y, w, 6.), 3., Color::new(0.01, 0.015, 0.04, 0.7));
        for (window, color) in [(WINDOW, GOLD), (GOOD, LANE[1]), (PERFECT, LANE[2])] {
            let half = (window as f32 + 0.5) * unit;
            panel(cx - half, y + 1., half * 2., 4., with_alpha(color, 0.22));
        }
        panel(cx - 0.5, y - 2., 1., 10., Color::new(1., 1., 1., 0.4));
        ink_mid("-", x - 10., y + 3., 13., MUTED, TextAlign::Center);
        ink_mid("+", x + w + 10., y + 3., 13., MUTED, TextAlign::Center);
        if self.attempt.judged() > self.attempt.misses {
            let offset = self.last_offset.clamp(-(WINDOW as i32), WINDOW as i32) as f32;
            let color = match self.last_offset.unsigned_abs() {
                0..=PERFECT => LANE[2],
                3..=GOOD => LANE[1],
                _ => GOLD,
            };
            shapes::fill_rounded_rect(rect(cx + offset * unit - 2., y - 4., 4., 14.), 2., color);
        }
    }
    /// Encore: a bolt meter that fills with clean hits, glows and invites X
    /// (or a click) when full, then drains behind a shield while active.
    fn draw_encore(&self) {
        let active = self.encore();
        let ready = self.charge == 100 && !active;
        let hover = ready && self.phase == Phase::Playing && on_encore_button(pointer());
        let beat = self.beat_pulse();
        if ready {
            let zone = rect(684., 398., 244., 56.);
            let glow = 0.5 + 0.5 * beat;
            let boost = if hover { 0.1 } else { 0. };
            shapes::fill_rounded_rect(zone, 12., with_alpha(GOLD, 0.06 + 0.06 * glow + boost));
            shapes::stroke_rounded_rect(
                zone,
                12.,
                if hover { 2. } else { 1. },
                with_alpha(GOLD, 0.25 + 0.35 * glow + boost * 3.),
            );
        }
        let lit = ready || active;
        if lit {
            pool(
                BOLT_AT.x,
                BOLT_AT.y,
                26.,
                with_alpha(GOLD, 0.22 + 0.2 * beat),
            );
        }
        let swell = if ready { 1. + 0.1 * beat } else { 1. };
        icon(
            Icon::Bolt,
            BOLT_AT.x,
            BOLT_AT.y,
            26. * swell,
            if lit { GOLD } else { with_alpha(GOLD, 0.5) },
        );
        let fraction = if active {
            let length = TRACKS[self.track].beat_ticks * 8;
            self.encore_until.saturating_sub(self.clock) as f32 / length as f32
        } else {
            self.charge as f32 / 100.
        };
        bar(
            726.,
            420.,
            152.,
            12.,
            fraction,
            if active { lighten(GOLD, 0.2) } else { GOLD },
        );
        if active {
            // A glint sweeps the draining meter.
            let sweep = (clock() * 1.6).fract();
            panel(
                726. + 152. * fraction * sweep,
                421.,
                3.,
                10.,
                Color::new(1., 1., 1., 0.55),
            );
            icon(Icon::Shield, 903., 426., 20., GOLD);
        } else {
            cap(
                "X",
                893.,
                416.,
                if ready { 1. } else { 0.35 },
                if ready { GOLD } else { PAPER },
            );
        }
    }
    /// Retry tickets: C spends one while a song is under way.
    fn draw_tickets(&self) {
        cap(
            "C",
            694.,
            ticket_at(0).y - 10.,
            if self.live() { 1. } else { 0.35 },
            PAPER,
        );
        for i in 0..3 {
            let at = ticket_at(i);
            if i < usize::from(self.retries) {
                icon(Icon::Restart, at.x, at.y, 20., GOLD);
            } else {
                icon(Icon::Restart, at.x, at.y, 20., DIM);
            }
        }
    }
    /// The whole chart in four lane rows: upcoming notes glow faintly in
    /// their lane color, hits light up, misses turn red, Encore is a gold
    /// band and a playhead marks the song position.
    fn draw_chart_map(&self, x: f32, y: f32, w: f32, h: f32) {
        let track = &TRACKS[self.track];
        let length = track.length() as f32;
        shapes::fill_rounded_rect(
            rect(x - 8., y - 3., w + 16., h + 6.),
            8.,
            Color::new(0.01, 0.015, 0.04, 0.65),
        );
        let row = (h - 6.) / 4.;
        for (lane, &color) in LANE.iter().enumerate() {
            let ry = y + lane as f32 * (row + 2.) + row * 0.5;
            panel(x, ry - 0.5, w, 1., with_alpha(color, 0.08));
        }
        let bar_w = w / track.bars() as f32;
        for bar in 1..track.bars() {
            let alpha = if bar.is_multiple_of(4) { 0.12 } else { 0.05 };
            panel(
                x + bar as f32 * bar_w,
                y,
                1.,
                h,
                Color::new(1., 1., 1., alpha),
            );
        }
        if self.encore_until > 0 && self.phase != Phase::Briefing {
            let from = self.encore_until.saturating_sub(track.beat_ticks * 8);
            let to = self.encore_until.min(track.length());
            let a = x + from as f32 / length * w;
            let b = x + to as f32 / length * w;
            shapes::fill_rounded_rect(rect(a, y - 2., b - a, h + 4.), 3., with_alpha(GOLD, 0.22));
        }
        let played = (self.chart_time().max(0) as f32 / length).clamp(0., 1.);
        let px = x + played * w;
        if played > 0. {
            shapes::fill_rounded_rect(
                rect(x - 4., y - 1., px - x + 4., h + 2.),
                5.,
                Color::new(1., 1., 1., 0.04),
            );
        }
        for note in &self.notes[..self.len] {
            let nx = x + note.tick as f32 / length * w;
            let ny = y + note.lane as f32 * (row + 2.);
            let color = match note.result {
                Judgment::Pending => with_alpha(LANE[note.lane], 0.42),
                Judgment::Miss => ALARM,
                _ => lighten(LANE[note.lane], 0.25),
            };
            shapes::fill_rounded_rect(rect(nx - 1.75, ny, 3.5, row), 1.2, color);
        }
        shapes::fill_rounded_rect(rect(px - 1.25, y - 5., 2.5, h + 10.), 1.2, PAPER);
    }
    /// Dim the stage and raise a song card titled `title` under its district.
    fn card_base(&self, title: &str) {
        let accent = LANE[self.track % 4];
        shapes::fill_rounded_rect(rect(16., 112., 928., 456.), 14., with_alpha(NIGHT, 0.62));
        card(90., 132., 780., 420., Color::new(0.06, 0.065, 0.14, 1.));
        shapes::fill_rounded_rect(rect(130., 132., 72., 3.), 1.5, accent);
        icon(Icon::Pin, 137., 166., 14., GOLD);
        ink(TRACKS[self.track].venue, 150., 171., 15., GOLD);
        fit(title, 130., 212., 700., 34., PAPER);
    }
    /// Song card: name, tempo/length/goal chips, a lesson headline and a
    /// live demo of the song's lesson pattern.
    fn draw_briefing(&self) {
        let track = &TRACKS[self.track];
        let accent = LANE[self.track % 4];
        self.card_base(track.title);
        let mut x = 130.;
        x += icon_chip(
            Icon::Music,
            &format!("{} BPM", track.bpm()),
            x,
            232.,
            14.,
            accent,
        ) + 10.;
        x += icon_chip(
            Icon::Clock,
            &minutes(track.length() + 2 * track.beat_ticks),
            x,
            232.,
            14.,
            SOFT,
        ) + 10.;
        icon_chip(
            Icon::Target,
            &format!("{}%", track.target / 10),
            x,
            232.,
            14.,
            GOLD,
        );
        let (symbol, tint) = match track.lesson {
            Lesson::Tap => (Icon::Music, LANE[1]),
            Lesson::Chord => (Icon::Plus, LANE[3]),
            Lesson::Rest => (Icon::Pause, LANE[1]),
            Lesson::Fast => (Icon::Eye, LANE[2]),
            Lesson::Encore => (Icon::Bolt, GOLD),
            Lesson::Finale => (Icon::Crown, GOLD),
        };
        let bob = juice::bob(clock(), 0.8, 2.);
        icon_badge(symbol, 152., 308. + bob, 44., with_alpha(tint, 0.18), tint);
        fit(track.story, 186., 318., 300., 26., PAPER);
        let glow = self.draw_demo(track, 540., 156., 290., 322.);
        self.draw_lesson_hint(track, 130., 372., glow);
        action_button(Icon::Play, "Start", accent);
    }
    /// What the lesson asks of the hands, lit in time with the demo: the
    /// arrow cluster, a chord's two keys, no taps in the gaps, the Encore
    /// meter and X, or the six district bulbs of the finale.
    fn draw_lesson_hint(&self, track: &Track, x: f32, y: f32, glow: [f32; 4]) {
        match track.lesson {
            Lesson::Tap | Lesson::Fast => {
                // The keyboard's inverted T: up above left, down and right.
                let at = [(0., 46.), (58., 46.), (58., 0.), (116., 46.)];
                for (lane, (dx, dy)) in at.into_iter().enumerate() {
                    mini_key(lane, x + dx, y + dy, 54., glow[lane]);
                }
            }
            Lesson::Chord => {
                let pattern = &track.patterns[track.demo];
                let chord = pattern
                    .iter()
                    .copied()
                    .find(|mask| mask.count_ones() > 1)
                    .unwrap_or(9);
                let mut lanes = (0..4_usize).filter(|lane| chord & (1 << lane) != 0);
                let first = lanes.next().unwrap_or(0);
                let second = lanes.next().unwrap_or(3);
                let together = glow[first].min(glow[second]);
                mini_key(first, x, y + 22., 62., glow[first]);
                icon(
                    Icon::Plus,
                    x + 86.,
                    y + 42.,
                    18.,
                    with_alpha(PAPER, 0.5 + 0.5 * together),
                );
                mini_key(second, x + 110., y + 22., 62., glow[second]);
            }
            Lesson::Rest => {
                // No taps in the gaps.
                icon(Icon::Hand, x + 22., y + 42., 36., MUTED);
                icon_badge(Icon::Cross, x + 42., y + 58., 22., ALARM, PAPER);
            }
            Lesson::Encore => self.draw_encore_lesson(track, x, y + 32.),
            Lesson::Finale => {
                for i in 0..TRACK_COUNT {
                    let at = vec2(x + 18. + i as f32 * 44., y + 42.);
                    lamp(at, 30., self.lamp_state(i));
                }
            }
        }
    }
    /// The Encore lesson: a meter fills over one bar, X lights, and the
    /// boost drains through the next.
    fn draw_encore_lesson(&self, track: &Track, x: f32, y: f32) {
        let bar_ticks = (track.beat_ticks * 4) as f32;
        let now = (clock() * 60.).rem_euclid(bar_ticks * 2.);
        let boosted = now >= bar_ticks;
        let fraction = if boosted {
            2. - now / bar_ticks
        } else {
            now / bar_ticks
        };
        icon(Icon::Bolt, x + 12., y + 10., 24., GOLD);
        bar(x + 32., y + 4., 200., 12., fraction, GOLD);
        let press = boosted && now - bar_ticks < 20.;
        if press {
            pool(x + 254., y + 10., 22., with_alpha(GOLD, 0.45));
        }
        cap(
            "X",
            x + 244.,
            y + if press { 2. } else { 0. },
            1.,
            if press || !boosted { GOLD } else { PAPER },
        );
        if boosted {
            icon(Icon::Shield, x + 292., y + 10., 22., GOLD);
        }
    }
    /// A looping preview of the song's lesson pattern at its real tempo,
    /// driven by the presentation clock: notes fall to a line, chords are
    /// linked and each key lights as its note arrives.
    fn draw_demo(&self, track: &Track, x: f32, y: f32, w: f32, h: f32) -> [f32; 4] {
        let lane_w = w / 4.;
        let line_y = y + h - 64.;
        let keys_y = line_y + 14.;
        let back = rect(x, y, w, h);
        shapes::fill_rounded_rect(
            back,
            12.,
            Fill::vertical(
                back,
                Color::new(0.02, 0.025, 0.06, 1.),
                Color::new(0.035, 0.035, 0.09, 1.),
            ),
        );
        shapes::stroke_rounded_rect(back, 12., 1., Color::new(1., 1., 1., 0.08));
        let beat = track.beat_ticks as f32;
        let bar_ticks = beat * 4.;
        let cycle = bar_ticks * 2.;
        let now = (clock() * 60.).rem_euclid(cycle);
        let boosted = track.lesson == Lesson::Encore && now >= bar_ticks;
        for (lane, &color) in LANE.iter().enumerate() {
            let lx = x + lane as f32 * lane_w;
            let r = rect(lx + 2., y + 10., lane_w - 4., line_y - y - 6.);
            let tint = if boosted { GOLD } else { color };
            shapes::fill_rounded_rect(
                r,
                0.,
                Fill::vertical(r, with_alpha(tint, 0.), with_alpha(tint, 0.14)),
            );
        }
        let speed = (line_y - y - 30.) / bar_ticks;
        let step = beat / 2.;
        let pattern = &track.patterns[track.demo];
        let mut flash = [0f32; 4];
        for repeat in -1..3 {
            for (i, &mask) in pattern.iter().enumerate() {
                if mask == 0 {
                    continue;
                }
                let due = repeat as f32 * bar_ticks + i as f32 * step - now;
                if due < 0. {
                    // A note that just arrived lights its key.
                    let glow = 1. + due / 16.;
                    for (lane, f) in flash.iter_mut().enumerate() {
                        if mask & (1 << lane) != 0 {
                            *f = f.max(glow);
                        }
                    }
                    continue;
                }
                let ny = line_y - due * speed;
                if ny < y + 18. {
                    continue;
                }
                let lanes = (0..4_usize).filter(|lane| mask & (1 << lane) != 0);
                let first = mask.trailing_zeros() as usize;
                let last = 7 - mask.leading_zeros() as usize;
                if last > first {
                    shapes::fill_capsule(
                        vec2(x + (first as f32 + 0.5) * lane_w, ny),
                        vec2(x + (last as f32 + 0.5) * lane_w, ny),
                        2.5,
                        Color::new(1., 1., 1., 0.8),
                    );
                }
                for lane in lanes {
                    let color = if boosted {
                        lighten(LANE[lane], 0.15)
                    } else {
                        LANE[lane]
                    };
                    let body = rect(x + lane as f32 * lane_w + 8., ny - 7., lane_w - 16., 14.);
                    shapes::fill_rounded_rect(
                        body,
                        7.,
                        Fill::vertical(body, lighten(color, 0.5), darken(color, 0.12)),
                    );
                    icon(
                        ARROWS[lane],
                        body.x + body.w * 0.5,
                        ny,
                        10.,
                        Color::new(0.03, 0.03, 0.09, 0.85),
                    );
                }
            }
        }
        let fade = rect(x + 2., y + 10., w - 4., 34.);
        shapes::fill_rounded_rect(
            fade,
            0.,
            Fill::vertical(
                fade,
                Color::new(0.02, 0.025, 0.06, 1.),
                Color::new(0.02, 0.025, 0.06, 0.),
            ),
        );
        shapes::fill_rounded_rect(
            rect(x + 6., line_y - 1.5, w - 12., 3.),
            1.5,
            Color::new(1., 1., 1., 0.85),
        );
        for (lane, &color) in LANE.iter().enumerate() {
            let f = flash[lane].clamp(0., 1.);
            let kx = x + lane as f32 * lane_w + 5.;
            if f > 0. {
                pool(
                    kx + lane_w * 0.5 - 5.,
                    line_y,
                    lane_w * 0.42,
                    with_alpha(color, 0.5 * f),
                );
            }
            mini_key(lane, kx, keys_y, lane_w - 10., f);
        }
        flash
    }
    /// Song results: the grade, three big stats, judgment tallies and the
    /// district bulbs with the newly restored one lit.
    fn draw_results(&self) {
        let stats = self.attempt;
        self.card_base("District restored");
        let grade = stats.grade();
        let tint = grade_color(grade);
        let tile = rect(130., 236., 140., 140.);
        shapes::fill_rounded_rect(
            tile,
            16.,
            Fill::vertical(tile, shade(tint, 0.3), shade(tint, 0.12)),
        );
        shapes::stroke_rounded_rect(tile, 16., 1.5, with_alpha(tint, 0.6));
        pool(
            200.,
            306.,
            66.,
            with_alpha(tint, 0.1 + 0.08 * juice::pulse(clock(), 0.5)),
        );
        let s = TextStyle::modern(72., tint)
            .with_shadow(Color::new(0., 0., 0., 0.5))
            .with_align(TextAlign::Center);
        draw_text_styled(grade, 200., s.centered_baseline(306.), s);
        let columns = [
            (Icon::Target, percent(stats.accuracy()), PAPER, LANE[2]),
            (Icon::Flame, stats.max_combo.to_string(), PAPER, EMBER),
            (Icon::Star, grouped(stats.score), GOLD, GOLD),
        ];
        for (i, (symbol, value, color, tint)) in columns.iter().enumerate() {
            let cx = 380. + i as f32 * 180.;
            icon(*symbol, cx, 258., 28., *tint);
            ink_mid(value, cx, 300., 26., *color, TextAlign::Center);
        }
        let mut x = 300.;
        for (&(symbol, color), count) in TALLY.iter().zip(tallies(stats)) {
            x += icon_chip(symbol, &count.to_string(), x, 340., 14., color) + 10.;
        }
        let wire = Color::new(1., 1., 1., 0.14);
        for i in 0..5 {
            let a = result_lamp(i) - vec2(0., 13.);
            let b = result_lamp(i + 1) - vec2(0., 13.);
            let mid = (a + b) * 0.5 + vec2(0., 12.);
            draw_line(a.x, a.y, mid.x, mid.y, 1.2, wire);
            draw_line(mid.x, mid.y, b.x, b.y, 1.2, wire);
        }
        for i in 0..TRACK_COUNT {
            lamp(result_lamp(i), 30., self.lamp_state(i));
        }
        let last = self.track + 1 == TRACK_COUNT;
        action_button(
            if last { Icon::Crown } else { Icon::Play },
            if last { "Final bow" } else { "Next song" },
            LANE[2],
        );
    }
    /// The failed-take card shows why (the red meter), what a retry costs
    /// (a ticket and this take's points) and both choices.
    fn draw_retry(&self) {
        let track = &TRACKS[self.track];
        self.card_base("Try again");
        let quiet = self.crowd <= 0;
        let accuracy = self.accuracy();
        self.reason(
            Icon::People,
            258.,
            self.crowd.clamp(0, 100) as f32 / 100.,
            &format!("{}%", self.crowd.max(0)),
            quiet,
            None,
        );
        self.reason(
            Icon::Target,
            302.,
            accuracy as f32 / 1000.,
            &percent(accuracy),
            !quiet,
            Some(track.target as f32 / 1000.),
        );
        // The price of another take.
        let blink = juice::pulse(clock(), 1.4);
        let spend = usize::from(self.retries).saturating_sub(1);
        for i in 0..3 {
            let at = vec2(146. + i as f32 * 30., 364.);
            let color = if i == spend {
                with_alpha(GOLD, 0.35 + 0.65 * blink)
            } else if i < usize::from(self.retries) {
                GOLD
            } else {
                DIM
            };
            icon(Icon::Restart, at.x, at.y, 22., color);
        }
        chip("-1", 230., 354., 14., ALARM);
        icon(Icon::Star, 300., 364., 20., MUTED);
        let w = ink_mid(
            &grouped(self.attempt.score),
            316.,
            364.,
            18.,
            MUTED,
            TextAlign::Left,
        );
        panel(312., 363., w + 8., 2., with_alpha(ALARM, 0.9));
        // End the concert instead (X, or click).
        let hover = on_end_button(pointer());
        let lift = if hover { 2. } else { 0. };
        let r = rect(650., 420. - lift, 180., 42.);
        shapes::fill_rounded_rect(
            r,
            12.,
            Color::new(0.13, 0.15, 0.24, if hover { 1. } else { 0.85 }),
        );
        shapes::stroke_rounded_rect(
            r,
            12.,
            1.,
            Color::new(1., 1., 1., if hover { 0.4 } else { 0.14 }),
        );
        let cy = r.y + r.h * 0.5;
        cap("X", r.x + 16., cy - 10., 1., PAPER);
        icon(Icon::Flag, r.x + 58., cy, 18., SOFT);
        ink_mid("End", r.x + 74., cy, 17., PAPER, TextAlign::Left);
        action_button(Icon::Restart, "Retry", GOLD);
    }
    /// One failure-reason meter: red with an alert when it failed the take.
    fn reason(
        &self,
        symbol: Icon,
        cy: f32,
        fraction: f32,
        value: &str,
        failed: bool,
        goal: Option<f32>,
    ) {
        let tone = if failed { ALARM } else { MUTED };
        icon(symbol, 142., cy, 22., tone);
        bar(
            166.,
            cy - 6.,
            500.,
            12.,
            fraction,
            if failed { ALARM } else { DIM },
        );
        if let Some(goal) = goal {
            shapes::fill_rounded_rect(rect(166. + 500. * goal - 1.5, cy - 11., 3., 22.), 1.5, GOLD);
        }
        ink_mid(
            value,
            684.,
            cy,
            18.,
            if failed { ALARM } else { SOFT },
            TextAlign::Left,
        );
        if failed {
            icon(Icon::Alert, 818., cy, 22., ALARM);
        }
    }
    /// One-sentence ending for status and screen-reader consumers.
    fn summary_line(&self) -> String {
        if self.phase == Phase::Won {
            format!(
                "All six songs cleared at {} accuracy for {} points.",
                percent(self.average_accuracy()),
                grouped(self.score())
            )
        } else {
            format!(
                "The concert ended after {} of 6 songs with {} points.",
                self.completed_tracks(),
                grouped(self.score())
            )
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
        self.draw_city();
        self.draw_left();
        self.draw_lanes();
        self.draw_right();
        match self.phase {
            Phase::Briefing => self.draw_briefing(),
            Phase::Results => self.draw_results(),
            Phase::Retry => self.draw_retry(),
            _ => {}
        }
    }
    fn hud(&self) -> Hud {
        let track = &TRACKS[self.track];
        Hud {
            stage: track.title.to_string(),
            objective: format!("Reach {}% accuracy", track.target / 10),
            status: format!(
                "{} of 6 songs cleared, {} retries left",
                self.completed_tracks(),
                self.retries
            ),
            score: self.score(),
        }
    }
    fn outcome(&self) -> Outcome {
        Game::outcome(self)
    }
    fn summary(&self) -> String {
        self.summary_line()
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.autoplay(tick)
    }
    fn music_notes(&self) -> [Option<u8>; 4] {
        Game::music_notes(self)
    }
    fn progress(&self) -> Option<(u32, u32)> {
        Some((self.track as u32 + 1, TRACK_COUNT as u32))
    }
    /// A miss must never shake the highway the next notes are falling down.
    fn shake_on_bad(&self) -> bool {
        false
    }
    fn effects(&self, _cue: Cue, fx: &mut Juice) {
        for event in self.events.iter() {
            match event {
                Event::Hit { lane, judgment } => {
                    let at = socket(lane);
                    let tone = LANE[lane];
                    match judgment {
                        Judgment::Perfect => {
                            fx.burst(at, lighten(tone, 0.35), 14);
                            fx.ring(at, tone, 56.);
                        }
                        Judgment::Good => {
                            fx.burst(at, tone, 8);
                            fx.ring(at, with_alpha(tone, 0.7), 42.);
                        }
                        _ => fx.burst(at, GOLD, 5),
                    }
                }
                Event::Miss { shielded: true, .. } => {
                    fx.popup_icon(CROWD_AT, Icon::Shield, "", GOLD);
                }
                Event::Miss { .. } => fx.popup_icon(CROWD_AT, Icon::People, "-10", ALARM),
                Event::Stray { lane, shielded } => {
                    fx.ring(socket(lane), ALARM, 34.);
                    if !shielded {
                        fx.popup(CROWD_AT, "-2", ALARM);
                    }
                }
                Event::ComboBroken(_) => fx.burst(COMBO_AT, with_alpha(SOFT, 0.9), 16),
                Event::Multiplier(step) => {
                    fx.popup(MULTIPLIER_AT - vec2(-6., 20.), &format!("x{step}"), GOLD);
                    fx.ring(MULTIPLIER_AT, GOLD, 34.);
                }
                Event::EncoreReady => {
                    fx.sparkle(BOLT_AT, GOLD);
                    fx.ring(BOLT_AT, GOLD, 34.);
                }
                Event::Encore => {
                    fx.callout(STAGE_AT, "ENCORE!", GOLD);
                    fx.burst(BOLT_AT, GOLD, 24);
                    fx.ring(STAGE_AT, GOLD, 170.);
                    fx.flash(with_alpha(GOLD, 0.1));
                }
                Event::CountBeat(beat) => fx.ring(
                    COUNT_AT,
                    if beat == 3 {
                        GOLD
                    } else {
                        with_alpha(SOFT, 0.7)
                    },
                    74.,
                ),
                Event::Cleared => {
                    let lamp = result_lamp(self.track);
                    fx.burst(lamp, GOLD, 30);
                    fx.ring(lamp, GOLD, 64.);
                    fx.sparkle(vec2(200., 306.), grade_color(self.attempt.grade()));
                }
                Event::Retry => {
                    let at = ticket_at(usize::from(self.retries));
                    fx.popup_icon(at - vec2(0., 12.), Icon::Restart, "-1", GOLD);
                    fx.ring(at, GOLD, 26.);
                }
                Event::Finale => {
                    for (i, at) in [
                        vec2(70., 200.),
                        vec2(890., 220.),
                        vec2(70., 430.),
                        vec2(890., 450.),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        fx.burst(at, LANE[i], 28);
                        fx.ring(at, LANE[i], 50.);
                    }
                }
            }
        }
    }
    fn draw_objective(&self, area: Rect) -> bool {
        let track = &TRACKS[self.track];
        let cy = area.y + area.h * 0.5;
        icon(Icon::Target, area.x + 10., cy, 18., GOLD);
        let w = ink_mid(
            &format!("{}%", track.target / 10),
            area.x + 24.,
            cy,
            16.,
            GOLD,
            TextAlign::Left,
        );
        // The map stops short of the score column, where gains float up.
        let x = area.x + 24. + w + 22.;
        self.draw_chart_map(x, area.y + 4., 800. - x, area.h - 8.);
        true
    }
    fn draw_result(&self, area: Rect) -> bool {
        let completed = self.completed_tracks();
        let total = self.concert_stats();
        let mut x = area.x;
        if completed > 0 {
            let grade = total.grade();
            let tint = grade_color(grade);
            let tile = rect(x, area.y + 14., 112., 112.);
            shapes::fill_rounded_rect(
                tile,
                14.,
                Fill::vertical(tile, shade(tint, 0.3), shade(tint, 0.12)),
            );
            shapes::stroke_rounded_rect(tile, 14., 1.5, with_alpha(tint, 0.6));
            let s = TextStyle::modern(56., tint)
                .with_shadow(Color::new(0., 0., 0., 0.5))
                .with_align(TextAlign::Center);
            draw_text_styled(grade, tile.x + 56., s.centered_baseline(tile.y + 56.), s);
            x += 140.;
        }
        let stats = [
            (Icon::Bulb, format!("{completed}/6"), GOLD),
            (Icon::Target, percent(self.average_accuracy()), LANE[2]),
            (Icon::Flame, total.max_combo.to_string(), EMBER),
            (Icon::Restart, self.retries.to_string(), SOFT),
        ];
        let column = (area.x + area.w - x) / stats.len() as f32;
        for (i, (symbol, value, tint)) in stats.iter().enumerate() {
            let cx = x + column * (i as f32 + 0.5);
            icon(*symbol, cx, area.y + 46., 32., *tint);
            ink_mid(value, cx, area.y + 98., 24., PAPER, TextAlign::Center);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(lane: usize) -> Input {
        Input {
            left_pressed: lane == 0,
            down_pressed: lane == 1,
            up_pressed: lane == 2,
            right_pressed: lane == 3,
            ..Input::default()
        }
    }
    /// A concert in its first song with the count-in done.
    fn playing() -> Game {
        let mut game = Game::new();
        game.step(&Input {
            primary: true,
            ..Input::default()
        });
        while game.phase != Phase::Playing {
            game.step(&Input::default());
        }
        game
    }

    #[test]
    fn progress_counts_songs_from_first_to_final_bow() {
        let mut game = Game::new();
        assert_eq!(game.progress(), Some((1, 6)));
        for tick in 0..60_000 {
            let command = game.autoplay(tick);
            game.step(&command);
            if game.outcome() != Outcome::Playing {
                break;
            }
        }
        assert_eq!(game.outcome(), Outcome::Won);
        assert_eq!(game.progress(), Some((6, 6)));
        assert_eq!(game.hud().stage, "Voltage Choir");
        assert!(game.hud().objective.split_whitespace().count() <= 5);
        assert!(game.summary().starts_with("All six songs cleared"));
    }

    #[test]
    fn a_perfect_hit_bursts_and_rings_at_its_socket_and_events_clear_next_tick() {
        let mut game = playing();
        let lane = game.notes[0].lane;
        assert_eq!(game.step(&press(lane)), Cue::Good);
        assert_eq!(
            game.events.iter().next(),
            Some(Event::Hit {
                lane,
                judgment: Judgment::Perfect
            })
        );
        let mut fx = Juice::new();
        game.effects(Cue::Good, &mut fx);
        let counts = fx.counts();
        assert!(counts.sparks >= 10, "{counts:?}");
        assert_eq!(counts.rings, 1);
        game.step(&Input::default());
        assert_eq!(game.events.iter().count(), 0);
    }

    #[test]
    fn misses_and_empty_taps_show_the_crowd_cost_where_it_is_paid() {
        let mut game = playing();
        // An empty tap in a lane with no nearby note.
        let empty = (0..4)
            .find(|&lane| {
                game.notes[..game.len]
                    .iter()
                    .all(|n| n.lane != lane || n.tick > 20)
            })
            .expect("an idle lane at the downbeat");
        game.step(&press(empty));
        let mut fx = Juice::new();
        game.effects(Cue::Bad, &mut fx);
        assert_eq!((fx.counts().rings, fx.counts().popups), (1, 1));
        // Let the opening note pass unplayed.
        let mut missed = false;
        for _ in 0..12 {
            game.step(&Input::default());
            if game.events.iter().any(|e| {
                matches!(
                    e,
                    Event::Miss {
                        shielded: false,
                        ..
                    }
                )
            }) {
                missed = true;
                break;
            }
        }
        assert!(missed);
        let mut fx = Juice::new();
        game.effects(Cue::Bad, &mut fx);
        assert_eq!(fx.counts().popups, 1);
    }

    #[test]
    fn encore_by_click_mirrors_x_and_calls_out() {
        let mut game = playing();
        game.charge = 99;
        let click = Input {
            click: true,
            pointer: vec2(800., 426.),
            ..Input::default()
        };
        game.step(&click);
        assert!(!game.encore(), "a partial meter cannot be spent");
        assert_eq!(game.charge, 99);
        game.charge = 100;
        assert_eq!(game.step(&click), Cue::Action);
        assert!(game.encore());
        assert_eq!(game.charge, 0);
        let mut fx = Juice::new();
        game.effects(Cue::Action, &mut fx);
        assert!(fx.counts().popups >= 1 && fx.counts().sparks > 0);
        // The same click outside the meter is not an Encore request.
        let mut elsewhere = playing();
        elsewhere.charge = 100;
        elsewhere.step(&Input {
            click: true,
            pointer: vec2(800., 300.),
            ..Input::default()
        });
        assert!(!elsewhere.encore());
        assert_eq!(elsewhere.charge, 100);
    }

    #[test]
    fn end_button_click_mirrors_x_on_the_failed_take_card() {
        let mut game = playing();
        while game.phase != Phase::Retry {
            game.step(&Input::default());
        }
        let mut miss = game.clone();
        miss.step(&Input {
            click: true,
            pointer: vec2(500., 440.),
            ..Input::default()
        });
        assert_eq!(miss.phase, Phase::Retry);
        assert_eq!(
            game.step(&Input {
                click: true,
                pointer: vec2(700., 440.),
                ..Input::default()
            }),
            Cue::Bad
        );
        assert_eq!(game.outcome(), Outcome::Lost);
        assert_eq!(game.retries(), 3);
    }

    #[test]
    fn retry_and_multiplier_events_point_at_their_widgets() {
        let mut game = playing();
        game.step(&Input {
            tertiary: true,
            ..Input::default()
        });
        assert_eq!(game.events.iter().next(), Some(Event::Retry));
        let mut fx = Juice::new();
        game.effects(Cue::Action, &mut fx);
        assert_eq!((fx.counts().popups, fx.counts().rings), (1, 1));
        let mut streak = playing();
        let mut steps = Vec::new();
        for tick in 0..2000 {
            let command = streak.autoplay(tick);
            streak.step(&command);
            steps.extend(streak.events.iter().filter_map(|e| match e {
                Event::Multiplier(step) => Some(step),
                _ => None,
            }));
            if streak.combo >= 41 {
                break;
            }
        }
        assert_eq!(steps, [2, 3]);
    }
}
