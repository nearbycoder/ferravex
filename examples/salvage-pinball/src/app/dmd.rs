//! The dot-matrix display. Text is drawn with the classic 5×7 face at one dot
//! per font pixel on a device-aligned grid, then a dark lattice is laid over
//! the whole glass so every lit pixel reads as a separate glowing dot and the
//! unlit ones show as a dim matrix. It is drawn before the HUD boundary, so
//! the amber dots feed the bloom like a real plasma display.
//!
//! Lines wider than the glass scroll as a marquee. Reduced motion stops the
//! scroll and shrinks the line to fit instead.
use super::layout::DMD_BEZEL;
use super::*;
use ferravex_render::shapes::{self, Fill, Shadow};
use ferravex_render::text::{TextStyle, draw_text_styled, text_width};

const OFF_DOT: Color = Color::from_rgba(46, 24, 10, 255);
const LATTICE: Color = Color::from_rgba(14, 8, 4, 255);
const GLASS_TOP: Color = Color::from_rgba(255, 220, 170, 18);
const MARQUEE_SPEED: f32 = 70.;
/// Seconds a scrolling line rests at its start and at its end.
const HOLD: f32 = 1.1;

pub(super) struct Line<'a> {
    pub text: &'a str,
    pub color: Color,
}

/// Draw the display in `rect` (bezel included) with a head and a sub line.
/// `time` drives the marquee; `motion` is zero under reduced motion.
pub(super) fn draw(rect: Rect, pitch: f32, head: Line, sub: Line, time: f32, motion: f32) {
    shapes::drop_shadow(
        rect,
        10.,
        Shadow::soft(Color::new(0., 0., 0., 0.55))
            .offset(vec2(0., 5.))
            .blur(18.),
    );
    shapes::fill_rounded_rect(
        rect,
        10.,
        Fill::vertical(
            rect,
            Color::from_rgba(70, 64, 54, 255),
            Color::from_rgba(28, 26, 24, 255),
        ),
    );
    shapes::stroke_rounded_rect(rect, 10., 1., with_alpha(BRASS, 0.7));
    let inner = Rect::new(
        rect.x + DMD_BEZEL,
        rect.y + DMD_BEZEL,
        rect.w - 2. * DMD_BEZEL,
        rect.h - 2. * DMD_BEZEL,
    );
    // Snap the glass to whole dots so text and lattice share one grid.
    let cols = (inner.w / pitch).floor();
    let rows = (inner.h / pitch).floor();
    let glass = Rect::new(
        (inner.x + (inner.w - cols * pitch) * 0.5).round(),
        (inner.y + (inner.h - rows * pitch) * 0.5).round(),
        cols * pitch,
        rows * pitch,
    );
    draw_rectangle(glass.x, glass.y, glass.w, glass.h, OFF_DOT);
    let gap = (rows - 16.).max(0.);
    let top = (gap * 0.4).floor().max(1.);
    let head_base = glass.y + (top + 7.) * pitch;
    let sub_base = head_base + (2. + (gap * 0.25).floor() + 7.) * pitch;
    let lip_w = (glass.x - rect.x - 2.).max(0.);
    line(glass, pitch, lip_w, head_base, head, time, motion);
    line(glass, pitch, lip_w, sub_base, sub, time, motion);
    // The lattice: thin dark lines between dots.
    let lw = (pitch * 0.34).max(0.6);
    let mut x = glass.x;
    while x <= glass.x + glass.w + 0.01 {
        draw_rectangle(x - lw * 0.5, glass.y, lw, glass.h, LATTICE);
        x += pitch;
    }
    let mut y = glass.y;
    while y <= glass.y + glass.h + 0.01 {
        draw_rectangle(glass.x, y - lw * 0.5, glass.w, lw, LATTICE);
        y += pitch;
    }
    // A faint top-lit glass sheen and the bezel lip over the edges.
    shapes::fill_rounded_rect(
        Rect::new(glass.x, glass.y, glass.w, glass.h * 0.45),
        2.,
        Fill::vertical(glass, GLASS_TOP, Color::new(1., 1., 1., 0.)),
    );
    let lip = |r: Rect| draw_rectangle(r.x, r.y, r.w, r.h, Color::from_rgba(36, 33, 30, 255));
    lip(Rect::new(
        rect.x + 2.,
        glass.y - 1.,
        glass.x - rect.x - 2.,
        glass.h + 2.,
    ));
    let right = glass.x + glass.w;
    lip(Rect::new(
        right,
        glass.y - 1.,
        rect.x + rect.w - 2. - right,
        glass.h + 2.,
    ));
}

fn line(glass: Rect, pitch: f32, lip: f32, baseline: f32, line: Line, time: f32, motion: f32) {
    if line.text.is_empty() {
        return;
    }
    let style = TextStyle::classic(8. * pitch, line.color).with_pixel_snap(false);
    let width = text_width(line.text, style);
    let room = glass.w - 2. * pitch;
    if width <= room {
        let offset = ((glass.w - width) * 0.5 / pitch).floor() * pitch;
        draw_text_styled(line.text, glass.x + offset, baseline, style);
        return;
    }
    if motion <= 0. {
        // No scrolling: shrink to fit. The lattice still reads as glass.
        let fitted = style.with_size(8. * pitch * room / width);
        draw_text_styled(line.text, glass.x + pitch, baseline, fitted);
        return;
    }
    // Marquee in whole dots: hold the start, scroll to the end, hold, repeat.
    // Glyphs are drawn one at a time; anything past the glass edge by more
    // than the bezel lip is skipped, and the lip hides partial dots.
    let overflow = width - room;
    let cycle = HOLD * 2. + overflow / MARQUEE_SPEED;
    let shift = ((time.rem_euclid(cycle) - HOLD) * MARQUEE_SPEED).clamp(0., overflow);
    let mut x = glass.x + pitch - (shift / pitch).floor() * pitch;
    for (i, c) in line.text.char_indices() {
        let glyph = &line.text[i..i + c.len_utf8()];
        let advance = text_width(glyph, style);
        if x >= glass.x - lip && x + advance - pitch <= glass.x + glass.w + lip {
            draw_text_styled(glyph, x, baseline, style);
        }
        x += advance;
        if x > glass.x + glass.w + lip {
            break;
        }
    }
}
