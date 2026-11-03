use crate::content::{LANE_KEYS, LANE_NAMES, TRACKS};
use crate::model::{Flash, Game, Judgment, Phase};
use ferravex_arcade::shapes::{self, Fill};
use ferravex_arcade::typography::{TextAlign, TextStyle, draw_text_styled, text_width};
use ferravex_arcade::*;

const NIGHT: Color = Color::new(0.025, 0.035, 0.085, 1.);
const DUSK: Color = Color::new(0.07, 0.045, 0.16, 1.);
const PANEL: Color = Color::new(0.055, 0.06, 0.13, 1.);
const FRAME: Color = Color::new(0.12, 0.16, 0.27, 1.);
const GOLD: Color = Color::new(1., 0.80, 0.38, 1.);
const SOFT: Color = Color::new(0.84, 0.87, 0.94, 1.);
const LANE: [Color; 4] = [
    Color::new(0.98, 0.36, 0.60, 1.),
    Color::new(0.42, 0.84, 0.98, 1.),
    Color::new(0.65, 0.93, 0.40, 1.),
    Color::new(0.88, 0.55, 1., 1.),
];
/// Highway geometry shared with the model's pointer lanes (x 180..660).
const LANES_X: f32 = 180.;
const LANE_W: f32 = 120.;
const HIT_Y: f32 = 488.;
const SPEED: f32 = 3.1;
const TOP_Y: f32 = 150.;

fn shade(c: Color, f: f32) -> Color {
    Color::new(c.r * f, c.g * f, c.b * f, 1.)
}
fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::new(x, y, w, h)
}
/// Shadowless modern text for clean card surfaces; returns the width.
fn ink(value: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    draw_text_styled(value, x, y, TextStyle::modern(size, color)).width
}
fn ink_at(value: &str, x: f32, y: f32, size: f32, color: Color, align: TextAlign) -> f32 {
    draw_text_styled(
        value,
        x,
        y,
        TextStyle::modern(size, color).with_align(align),
    )
    .width
}
/// Small letter-spaced section label.
fn eyebrow(value: &str, x: f32, y: f32, color: Color) {
    draw_text_styled(
        value,
        x,
        y,
        TextStyle::modern(11., color).with_letter_spacing(1.5),
    );
}
fn chip_width(label: &str, size: f32) -> f32 {
    text_width(label, TextStyle::modern(size, PAPER)) + size * 1.3
}
fn chip_right(label: &str, right: f32, y: f32, size: f32, color: Color) {
    tag(label, right - chip_width(label, size), y, size, color);
}
/// Filled pill label in the host chip's proportions, without its outline pass.
fn tag(label: &str, x: f32, y: f32, size: f32, color: Color) -> f32 {
    let s = TextStyle::modern(size, color);
    let w = text_width(label, s) + size * 1.3;
    let h = size * 1.45;
    shapes::fill_rounded_rect(rect(x, y, w, h), h * 0.5, with_alpha(color, 0.16));
    draw_text_styled(label, x + size * 0.65, s.centered_baseline(y + h * 0.5), s);
    w
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
fn judgment_color(flash: Flash) -> Color {
    match flash {
        Flash::Perfect => LANE[2],
        Flash::Good => LANE[1],
        Flash::Okay => GOLD,
        Flash::Miss | Flash::Stray => LANE[0],
        Flash::None => SOFT,
    }
}
fn arrow(lane: usize, x: f32, y: f32, size: f32, c: Color) {
    let (dir, side) = match lane {
        0 => (vec2(-1., 0.), vec2(0., 1.)),
        1 => (vec2(0., 1.), vec2(1., 0.)),
        2 => (vec2(0., -1.), vec2(1., 0.)),
        _ => (vec2(1., 0.), vec2(0., 1.)),
    };
    let center = vec2(x, y);
    draw_triangle(
        center + dir * size,
        center - dir * size * 0.45 + side * size * 0.75,
        center - dir * size * 0.45 - side * size * 0.75,
        c,
    );
}
/// Lightweight raised surface: lit gradient and hairline rim, no blur pass.
fn slab(x: f32, y: f32, w: f32, h: f32) {
    let r = rect(x, y, w, h);
    shapes::fill_rounded_rect(r, 12., Fill::vertical(r, lighten(PANEL, 0.04), PANEL));
    shapes::stroke_rounded_rect(r, 12., 1., Color::new(1., 1., 1., 0.07));
}
/// Small key letter on a dark cap (cheaper than a full keycap).
fn key_letter(key: &str, x: f32, y: f32) {
    shapes::fill_rounded_rect(
        rect(x, y, 20., 20.),
        4.,
        Color::new(0.02, 0.025, 0.06, 0.55),
    );
    ink_at(key, x + 10., y + 15., 13., SOFT, TextAlign::Center);
}
/// Six district pips; complete tracks glow in their lane color.
fn district_pips(game: &Game, x: f32, y: f32, w: f32, h: f32) {
    let gap = 4.;
    let cell = (w - gap * 5.) / 6.;
    let pulse = 0.65 + 0.35 * (game.stage_ticks as f32 * 0.09).sin().abs();
    for i in 0..6 {
        let px = x + (cell + gap) * i as f32;
        let fill = if game.results[i].is_some() {
            LANE[i % 4]
        } else if i == game.track {
            with_alpha(GOLD, pulse)
        } else {
            with_alpha(FRAME, 0.9)
        };
        shapes::fill_rounded_rect(rect(px, y, cell, h), h * 0.5, fill);
    }
}
/// Primary action strip inside the model's click region (240..720, 491..534).
fn action_button(label: &str, key: &str, color: Color) {
    let r = rect(240., 491., 480., 43.);
    shapes::fill_rounded_rect(rect(r.x, r.y + 3., r.w, r.h), 12., darken(color, 0.6));
    shapes::fill_rounded_rect(
        r,
        12.,
        Fill::vertical(r, lighten(color, 0.2), darken(color, 0.08)),
    );
    shapes::stroke_rounded_rect(r, 12., 1., Color::new(1., 1., 1., 0.3));
    let s = TextStyle::modern(18., INK);
    let label_w = text_width(label, s);
    let key_w = (text_width(key, TextStyle::modern(13., PAPER)) + 12.).max(20.);
    let left = 480. - (label_w + 12. + key_w) * 0.5;
    draw_text_styled(label, left, s.centered_baseline(512.5), s);
    keycap(key, "", left + label_w + 12., 502.5, PAPER);
}
impl Game {
    fn chart_time(&self) -> i32 {
        if self.phase == Phase::CountIn {
            self.clock as i32 - TRACKS[self.track].beat_ticks as i32 * 4
        } else {
            self.clock as i32
        }
    }
    /// 1 on each beat, easing to 0 before the next; follows song ticks only.
    fn beat_pulse(&self) -> f32 {
        let beat = TRACKS[self.track].beat_ticks as i32;
        let t = self.chart_time().rem_euclid(beat) as f32 / beat as f32;
        (1. - t) * (1. - t)
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
            shapes::fill_rounded_rect(
                r,
                0.,
                Fill::vertical(r, with_alpha(tint, 0.), with_alpha(tint, 0.13)),
            );
            if lane > 0 {
                draw_line(x, TOP_Y, x, 498., 1., Color::new(1., 1., 1., 0.06));
            }
            // Lane number badge: keys 1-4 and taps also play lanes.
            let cx = x + LANE_W * 0.5;
            draw_poly(cx, 130., 16, 10.5, 0., with_alpha(color, 0.16));
            ink_at(
                ["1", "2", "3", "4"][lane],
                cx,
                135.,
                13.,
                color,
                TextAlign::Center,
            );
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
            if !(TOP_Y..=512.).contains(&y) {
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
            arrow(
                note.lane,
                x + 48.,
                y + 1.,
                5.,
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
        self.draw_hits();
        self.draw_keys();
        if self.phase == Phase::CountIn {
            self.draw_count_in();
        }
        if encore {
            let left =
                self.encore_until.saturating_sub(self.clock) as f32 / (track.beat_ticks * 8) as f32;
            shapes::stroke_rounded_rect(highway, 14., 2., with_alpha(GOLD, 0.55 + 0.4 * pulse));
            let w = chip_width("Encore", 13.);
            tag("Encore", 420. - w * 0.5, 117., 13., GOLD);
            bar(420. - w * 0.5 + 6., 139., w - 12., 3., left, GOLD);
        } else {
            shapes::stroke_rounded_rect(highway, 14., 1., Color::new(1., 1., 1., 0.08));
        }
    }
    /// Accepted judgments: rising beams, expanding rings and labels.
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
            let color = judgment_color(flash);
            let hit = matches!(flash, Flash::Perfect | Flash::Good | Flash::Okay);
            if hit {
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
            } else {
                shapes::fill_rounded_rect(
                    rect(x + 12., HIT_Y - 9., 96., 18.),
                    9.,
                    with_alpha(LANE[0], 0.28 * t),
                );
            }
            let rise = (1. - t) * 16.;
            let alpha = (t * 3.).min(1.);
            draw_text_styled(
                flash.label(),
                cx,
                HIT_Y - 22. - rise,
                TextStyle::modern(16., with_alpha(lighten(color, 0.25), alpha))
                    .with_shadow(Color::new(0., 0., 0., 0.6 * alpha))
                    .with_align(TextAlign::Center),
            );
        }
    }
    fn draw_keys(&self) {
        for (lane, &color) in LANE.iter().enumerate() {
            let x = LANES_X + 4. + lane as f32 * LANE_W;
            let active = self.flash_ticks[lane] > 0;
            let glow_t = f32::from(self.flash_ticks[lane]) / 22.;
            let r = rect(x, 506., 112., 46.);
            let (top, bottom) = if active {
                (
                    shapes::mix(shade(color, 0.3), color, 0.5 * glow_t),
                    shade(color, 0.18),
                )
            } else {
                (shade(color, 0.2), shade(color, 0.1))
            };
            shapes::fill_rounded_rect(r, 10., Fill::vertical(r, top, bottom));
            if active {
                shapes::stroke_rounded_rect(r, 10., 1.5, with_alpha(color, 0.5 + 0.5 * glow_t));
            }
            panel(
                x + 14.,
                507.,
                84.,
                2.,
                with_alpha(color, 0.6 + 0.4 * glow_t),
            );
            arrow(lane, x + 22., 530., 7., if active { PAPER } else { color });
            ink(LANE_NAMES[lane], x + 36., 535., 14., SOFT);
            key_letter(LANE_KEYS[lane], x + 84., 519.);
        }
    }
    fn draw_count_in(&self) {
        let track = &TRACKS[self.track];
        let beat = self.clock / track.beat_ticks;
        let remaining = 4u32.saturating_sub(beat);
        let r = rect(335., 262., 170., 126.);
        card(r.x, r.y, r.w, r.h, PANEL);
        let s = TextStyle::modern(11., MUTED)
            .with_letter_spacing(1.5)
            .with_align(TextAlign::Center);
        draw_text_styled("COUNT IN", 420., 286., s);
        let scale = 1. + 0.12 * self.beat_pulse();
        ink_at(
            &remaining.to_string(),
            420.,
            344.,
            48. * scale,
            GOLD,
            TextAlign::Center,
        );
        for i in 0..4 {
            let done = i < beat.min(4);
            shapes::fill_rounded_rect(
                rect(378. + i as f32 * 22., 364., 18., 6.),
                3.,
                if done { GOLD } else { with_alpha(FRAME, 1.) },
            );
        }
    }
    fn draw_left(&self) {
        slab(24., 124., 140., 432.);
        let x = 40.;
        eyebrow("CROWD", x, 150., MUTED);
        let fill = self.crowd.clamp(0, 100) as f32 / 100.;
        let danger = self.crowd < 25;
        let track = rect(x, 164., 20., 148.);
        shapes::fill_rounded_rect(track, 6., Color::new(0.01, 0.015, 0.04, 0.8));
        if fill > 0. {
            let h = (track.h * fill).max(12.);
            let bar = rect(x, track.y + track.h - h, 20., h);
            let (top, bottom) = if danger {
                (lighten(LANE[0], 0.2), LANE[0])
            } else {
                (lighten(LANE[2], 0.25), shade(LANE[2], 0.7))
            };
            shapes::fill_rounded_rect(bar, 6., Fill::vertical(bar, top, bottom));
            panel(
                x + 4.,
                bar.y + 3.,
                3.,
                (h - 8.).max(0.),
                Color::new(1., 1., 1., 0.25),
            );
        }
        // Danger threshold mark at 25%.
        panel(
            x - 3.,
            track.y + track.h * 0.75,
            26.,
            1.,
            with_alpha(GOLD, 0.6),
        );
        ink(
            &format!("{}%", self.crowd),
            70.,
            188.,
            24.,
            if danger { GOLD } else { PAPER },
        );
        ink("energy", 70., 206., 12., MUTED);
        let (mood, mood_color) = if danger {
            ("Fading", LANE[0])
        } else if self.crowd >= 70 {
            ("Roaring", LANE[2])
        } else {
            ("Singing", LANE[1])
        };
        self.draw_audience(danger);
        tag(mood, 70., 294., 11., mood_color);
        divider(36., 334., 116., Color::new(1., 1., 1., 0.12));
        eyebrow("COMBO", x, 358., MUTED);
        ink(&self.combo.to_string(), x, 402., 40., PAPER);
        let multiplier = 1 + (self.combo / 20).min(4);
        tag(&format!("x{multiplier} score"), x, 412., 12., GOLD);
        let progress = if multiplier == 5 {
            1.
        } else {
            (self.combo % 20) as f32 / 20.
        };
        bar(x, 440., 108., 5., progress, GOLD);
        divider(36., 464., 116., Color::new(1., 1., 1., 0.12));
        eyebrow("DISTRICTS", x, 488., MUTED);
        district_pips(self, x, 500., 108., 8.);
        ink(
            &format!("{} of 6 lit", self.completed_tracks()),
            x,
            532.,
            13.,
            SOFT,
        );
    }
    fn draw_right(&self) {
        let track = &TRACKS[self.track];
        let x = 696.;
        let right = 916.;
        slab(676., 124., 260., 432.);
        eyebrow("NOW PLAYING", x, 150., MUTED);
        chip_right(
            &format!("{}/6", self.track + 1),
            right,
            139.,
            11.,
            LANE[self.track % 4],
        );
        fit(track.title, x, 180., 220., 22., GOLD);
        ink(track.venue, x, 199., 13., MUTED);
        bar(
            x,
            212.,
            220.,
            6.,
            self.clock as f32 / track.length() as f32,
            LANE[1],
        );
        ink(&format!("{} BPM", track.bpm()), x, 236., 12., SOFT);
        let seconds = self.clock / 60;
        let length = (track.length() + 2 * track.beat_ticks) / 60;
        ink_at(
            &format!(
                "{}:{:02} / {}:{:02}",
                seconds / 60,
                seconds % 60,
                length / 60,
                length % 60
            ),
            right,
            236.,
            12.,
            MUTED,
            TextAlign::Right,
        );
        divider(692., 252., 228., Color::new(1., 1., 1., 0.12));
        eyebrow("ACCURACY", x, 276., MUTED);
        chip_right(
            &format!("Goal {}%", track.target / 10),
            right,
            265.,
            11.,
            GOLD,
        );
        let accuracy = self.accuracy();
        ink(
            &percent(accuracy),
            x,
            312.,
            30.,
            if accuracy >= track.target {
                PAPER
            } else {
                LANE[0]
            },
        );
        for (i, (label, value, color)) in [
            ("Perfect", self.attempt.perfect, LANE[2]),
            ("Good", self.attempt.good, LANE[1]),
            ("Okay", self.attempt.okay, GOLD),
            ("Miss", self.attempt.misses, LANE[0]),
        ]
        .into_iter()
        .enumerate()
        {
            let px = x + [0., 62., 116., 170.][i];
            ink(label, px, 336., 11., color);
            ink(&value.to_string(), px, 358., 17., PAPER);
        }
        self.draw_timing(x, 370., 220.);
        divider(692., 402., 228., Color::new(1., 1., 1., 0.12));
        eyebrow("ENCORE", x, 424., GOLD);
        ink_at(
            &format!("{}%", self.charge),
            right,
            424.,
            12.,
            MUTED,
            TextAlign::Right,
        );
        bar(x, 434., 220., 10., self.charge as f32 / 100., GOLD);
        if self.encore() {
            tag("Boost active", x, 452., 12., GOLD);
        } else if self.charge == 100 {
            pool(
                x + 214.,
                439.,
                20.,
                with_alpha(GOLD, 0.3 + 0.3 * self.beat_pulse()),
            );
            keycap("X", "Activate encore", x, 452., GOLD);
        } else {
            ink("Perfect notes charge it", x, 467., 12., MUTED);
        }
        divider(692., 484., 228., Color::new(1., 1., 1., 0.12));
        wrapped(self.notice, x, 502., 220., 12., SOFT);
        ink("Retry tickets", x, 543., 12., MUTED);
        for i in 0..3_u8 {
            let px = right - 14. - f32::from(2 - i) * 19.;
            let r = rect(px, 534., 14., 10.);
            if i < self.retries {
                shapes::fill_rounded_rect(r, 3., Fill::vertical(r, lighten(GOLD, 0.3), GOLD));
            } else {
                shapes::stroke_rounded_rect(r, 3., 1., with_alpha(MUTED, 0.5));
            }
        }
    }
    /// A small audience that lights up with crowd energy and bounces on the beat.
    fn draw_audience(&self, danger: bool) {
        let lit = (self.crowd.clamp(0, 100) as usize * 12).div_ceil(100);
        let pulse = if self.phase == Phase::Playing {
            self.beat_pulse()
        } else {
            0.
        };
        for i in 0..12 {
            let (row, col) = (i / 4, i % 4);
            let on = i < lit;
            let x = 80. + col as f32 * 20. + if row == 1 { 6. } else { 0. };
            let bounce = if on {
                3. * pulse
                    * if (col + row).is_multiple_of(2) {
                        1.
                    } else {
                        0.6
                    }
            } else {
                0.
            };
            let y = 226. + row as f32 * 22. - bounce;
            let color = if !on {
                Color::new(0.16, 0.18, 0.3, 1.)
            } else if danger {
                LANE[0]
            } else {
                LANE[(i * 3 + row) % 4]
            };
            draw_poly(x, y + 10., 12, 5.5, 0., shade(color, 0.5));
            draw_poly(x, y, 10, 4.5, 0., color);
            if on && self.crowd >= 70 && pulse > 0.4 {
                draw_line(
                    x - 5.,
                    y + 7.,
                    x - 8.,
                    y - 3. - bounce,
                    1.5,
                    shade(color, 0.8),
                );
                draw_line(
                    x + 5.,
                    y + 7.,
                    x + 8.,
                    y - 3. - bounce,
                    1.5,
                    shade(color, 0.8),
                );
            }
        }
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
        let s = TextStyle::modern(10., MUTED);
        draw_text_styled("Early", x, y + 19., s);
        draw_text_styled("Late", x + w, y + 19., s.with_align(TextAlign::Right));
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
    fn card_base(&self, title: &str) {
        let accent = LANE[self.track % 4];
        shapes::fill_rounded_rect(rect(16., 112., 928., 456.), 14., with_alpha(NIGHT, 0.62));
        card(90., 132., 780., 420., Color::new(0.06, 0.065, 0.14, 0.98));
        shapes::fill_rounded_rect(rect(130., 132., 72., 3.), 1.5, accent);
        eyebrow(
            &format!("CONCERT  /  TRACK {} OF 6", self.track + 1),
            130.,
            170.,
            accent,
        );
        fit(title, 130., 210., 700., 34., PAPER);
    }
    fn draw_briefing(&self) {
        let track = &TRACKS[self.track];
        self.card_base(track.title);
        ink(track.venue, 130., 236., 15., GOLD);
        wrapped(track.story, 130., 270., 700., 17., PAPER);
        for (lane, &color) in LANE.iter().enumerate() {
            let x = 130. + lane as f32 * 178.;
            let r = rect(x, 292., 166., 46.);
            shapes::fill_rounded_rect(
                r,
                10.,
                Fill::vertical(r, shade(color, 0.22), shade(color, 0.12)),
            );
            shapes::stroke_rounded_rect(r, 10., 1., with_alpha(color, 0.4));
            arrow(lane, x + 22., 315., 8., color);
            ink(LANE_NAMES[lane], x + 40., 320., 15., PAPER);
            key_letter(LANE_KEYS[lane], x + 132., 305.);
        }
        wrapped(track.lesson, 130., 372., 700., 15., SOFT);
        let mut cx = 130.;
        for (label, color) in [
            (format!("{} BPM", track.bpm()), LANE[1]),
            (format!("{} bars", track.bars()), LANE[3]),
            (format!("Target {}%", track.target / 10), GOLD),
        ] {
            cx += tag(&label, cx, 412., 13., color) + 10.;
        }
        action_button("Start count-in", "Space", LANE[1]);
    }
    fn draw_results(&self) {
        let stats = self.attempt;
        self.card_base("District restored");
        ink(TRACKS[self.track].venue, 130., 236., 15., GOLD);
        let grade = stats.grade();
        let grade_color = match grade {
            "S+" | "S" => GOLD,
            "A" => LANE[2],
            "B" => LANE[1],
            "C" => LANE[3],
            _ => LANE[0],
        };
        let tile = rect(130., 256., 150., 150.);
        shapes::fill_rounded_rect(
            tile,
            16.,
            Fill::vertical(tile, shade(grade_color, 0.3), shade(grade_color, 0.12)),
        );
        shapes::stroke_rounded_rect(tile, 16., 1.5, with_alpha(grade_color, 0.6));
        let s = TextStyle::modern(11., with_alpha(grade_color, 0.9))
            .with_letter_spacing(1.5)
            .with_align(TextAlign::Center);
        draw_text_styled("GRADE", 205., 280., s);
        draw_text_styled(
            grade,
            205.,
            368.,
            TextStyle::modern(64., grade_color)
                .with_shadow(Color::new(0., 0., 0., 0.5))
                .with_align(TextAlign::Center),
        );
        let rows = [
            ("Accuracy", percent(stats.accuracy()), PAPER),
            ("Max combo", stats.max_combo.to_string(), PAPER),
            ("Song score", stats.score.to_string(), GOLD),
        ];
        for (i, (label, value, color)) in rows.iter().enumerate() {
            let y = 284. + i as f32 * 38.;
            ink(label, 310., y, 16., SOFT);
            ink_at(value, 830., y, 22., *color, TextAlign::Right);
            if i < 2 {
                divider(310., y + 14., 520., Color::new(1., 1., 1., 0.1));
            }
        }
        let mut cx = 310.;
        for (label, value, color) in [
            ("Perfect", stats.perfect, LANE[2]),
            ("Good", stats.good, LANE[1]),
            ("Okay", stats.okay, GOLD),
            ("Missed", stats.misses, LANE[0]),
            ("Empty", stats.stray, MUTED),
        ] {
            cx += tag(&format!("{label} {value}"), cx, 384., 12., color) + 8.;
        }
        eyebrow("DISTRICTS RESTORED", 130., 440., MUTED);
        district_pips(self, 130., 452., 700., 8.);
        action_button(
            if self.track == 5 {
                "Final bow"
            } else {
                "Next district"
            },
            "Space",
            LANE[2],
        );
    }
    fn draw_retry(&self) {
        let track = &TRACKS[self.track];
        self.card_base("The song needs another take");
        wrapped(self.notice, 130., 250., 700., 17., GOLD);
        eyebrow("ACCURACY", 130., 312., MUTED);
        ink(&percent(self.accuracy()), 130., 346., 28., LANE[0]);
        ink_at(
            &format!("Target {}%", track.target / 10),
            830.,
            346.,
            15.,
            GOLD,
            TextAlign::Right,
        );
        bar(
            130.,
            360.,
            700.,
            10.,
            self.accuracy() as f32 / 1000.,
            LANE[0],
        );
        let mark = 130. + 700. * track.target as f32 / 1000.;
        shapes::fill_rounded_rect(rect(mark - 1., 355., 3., 20.), 1.5, GOLD);
        wrapped(
            "Retry costs one ticket and resets this track's score.",
            130.,
            404.,
            700.,
            15.,
            SOFT,
        );
        eyebrow("RETRY TICKETS", 130., 446., MUTED);
        for i in 0..3_u8 {
            let r = rect(130. + f32::from(i) * 24., 456., 18., 12.);
            if i < self.retries {
                shapes::fill_rounded_rect(r, 3., Fill::vertical(r, lighten(GOLD, 0.3), GOLD));
            } else {
                shapes::stroke_rounded_rect(r, 3., 1., with_alpha(MUTED, 0.5));
            }
        }
        let w = text_width("Finish concert", TextStyle::modern(13., PAPER)) + 27.;
        keycap("X", "Finish concert", 830. - w, 452., MUTED);
        action_button("Retry song", "Space", GOLD);
    }
    fn ending(&self) -> String {
        let completed = self.completed_tracks();
        if self.phase == Phase::Won {
            let perfect = self
                .results
                .iter()
                .flatten()
                .map(|s| s.misses + s.stray)
                .sum::<u32>()
                == 0;
            format!(
                "Six tracks cleared. Accuracy {}. Score {}. Rank {}. Retries {}.",
                percent(self.average_accuracy()),
                self.score(),
                if perfect {
                    "A flawless citywide concert!"
                } else {
                    "The windows stay lit until your next encore."
                },
                self.retries
            )
        } else {
            format!("{completed}/6 tracks cleared. Score {}.", self.score())
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
        Hud {
            stage: format!("Track {}/6 - {}", self.track + 1, TRACKS[self.track].title),
            objective: format!(
                "Finish with at least {}% accuracy.",
                TRACKS[self.track].target / 10
            ),
            status: format!(
                "{} tracks cleared / {} retries / {} notes",
                self.completed_tracks(),
                self.retries,
                self.len
            ),
            score: self.score(),
        }
    }
    fn outcome(&self) -> Outcome {
        Game::outcome(self)
    }
    fn summary(&self) -> String {
        self.ending()
    }
    fn bot_input(&self, tick: u64) -> Input {
        self.autoplay(tick)
    }
    fn music_notes(&self) -> [Option<u8>; 4] {
        Game::music_notes(self)
    }
}
