//! Every piece of copy goes through this module.
//!
//! Copy is queued (no allocation after warm-up) and drawn with the engine's
//! styled text in one pass by [`Text::flush`], so a frame's shapes, icons and
//! glows always sit underneath its words. The mixed-case Modern face carries
//! UI and flavor copy; the chunky Classic face carries big punchy numbers and
//! titles. The animated logo uses its own display face through the painter.
#![allow(clippy::too_many_arguments)] // Drawing helpers take explicit geometry.
use crate::gfx::{Painter, Rgba, color, rgba, with_alpha};
use ferravex_render::prelude::{Color, Vec2, vec2};
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use std::fmt::{self, Write};
use std::ops::Range;

/// Crisp sizes are multiples of 8 (one font pixel = size / 8 logical px).
pub const SMALL: f32 = 8.;
pub const BODY: f32 = 16.;
pub const LARGE: f32 = 24.;
pub const HUGE: f32 = 32.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Face {
    Modern,
    Classic,
}

#[derive(Clone)]
struct Cmd {
    range: Range<usize>,
    at: Vec2,
    w: f32,
    style: TextStyle,
    lines: u8,
}

pub struct Text {
    arena: String,
    cmds: Vec<Cmd>,
}

fn style(face: Face, size: f32, c: Rgba, align: Align) -> TextStyle {
    let base = match face {
        Face::Modern => TextStyle::modern(size, color(c)),
        Face::Classic => TextStyle::classic(size, color(c)).with_outline(Color::new(
            0.01,
            0.02,
            0.06,
            0.85 * c[3],
        )),
    };
    base.with_shadow(Color::new(0., 0., 0., 0.55 * c[3]))
        .with_align(match align {
            Align::Left => TextAlign::Left,
            Align::Center => TextAlign::Center,
            Align::Right => TextAlign::Right,
        })
}

/// Width of a single line in logical pixels.
pub fn width_in(face: Face, text: &str, size: f32) -> f32 {
    text_width(text, style(face, size, [1.; 4], Align::Left))
}

struct Arena<'a>(&'a mut String, usize);
impl Write for Arena<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let room = self.1.saturating_sub(self.0.len());
        if s.len() <= room {
            self.0.push_str(s);
        } else {
            // Truncate at a char boundary rather than reallocating.
            let mut cut = room;
            while !s.is_char_boundary(cut) {
                cut -= 1;
            }
            self.0.push_str(&s[..cut]);
        }
        Ok(())
    }
}

impl Text {
    pub fn new() -> Self {
        Self {
            arena: String::with_capacity(48 * 1024),
            cmds: Vec::with_capacity(1024),
        }
    }

    fn push(
        &mut self,
        face: Face,
        at: Vec2,
        w: f32,
        size: f32,
        c: Rgba,
        align: Align,
        lines: u8,
        args: fmt::Arguments,
    ) {
        if c[3] <= 0.004 || self.cmds.len() == self.cmds.capacity() {
            return;
        }
        let start = self.arena.len();
        let cap = self.arena.capacity();
        let _ = Arena(&mut self.arena, cap).write_fmt(args);
        let range = start..self.arena.len();
        let mut st = style(face, size, c, align);
        if lines == 0 {
            // Shrink single lines to fit their box, in crisp steps.
            let text = &self.arena[range.clone()];
            while st.size > 8. && w > 0. && text_width(text, st) > w {
                st = st.with_size(st.size - 8.);
            }
        }
        self.cmds.push(Cmd {
            range,
            at,
            w,
            style: st,
            lines,
        });
    }

    /// Modern-face single line inside the box `at..at+w`, top-aligned.
    pub fn label(&mut self, at: Vec2, w: f32, text: &str, size: f32, c: Rgba, align: Align) {
        self.push(
            Face::Modern,
            at,
            w,
            size,
            c,
            align,
            0,
            format_args!("{text}"),
        );
    }

    pub fn label_fmt(
        &mut self,
        at: Vec2,
        w: f32,
        size: f32,
        c: Rgba,
        align: Align,
        args: fmt::Arguments,
    ) {
        self.push(Face::Modern, at, w, size, c, align, 0, args);
    }

    /// Classic-face punchy line (numbers, titles) with an outline.
    pub fn big(&mut self, at: Vec2, w: f32, text: &str, size: f32, c: Rgba, align: Align) {
        self.push(
            Face::Classic,
            at,
            w,
            size,
            c,
            align,
            0,
            format_args!("{text}"),
        );
    }

    pub fn big_fmt(
        &mut self,
        at: Vec2,
        w: f32,
        size: f32,
        c: Rgba,
        align: Align,
        args: fmt::Arguments,
    ) {
        self.push(Face::Classic, at, w, size, c, align, 0, args);
    }

    /// Word-wrapped Modern copy, at most `lines` lines; returns the height used.
    #[allow(clippy::too_many_arguments)]
    pub fn wrapped(
        &mut self,
        at: Vec2,
        w: f32,
        lines: usize,
        text: &str,
        size: f32,
        c: Rgba,
        align: Align,
    ) -> f32 {
        let st = style(Face::Modern, size, c, align);
        let n = wrap_text(text, w, st).take(lines).count();
        self.push(
            Face::Modern,
            at,
            w,
            size,
            c,
            align,
            lines.clamp(1, 255) as u8,
            format_args!("{text}"),
        );
        n as f32 * st.line_height()
    }

    /// Draw everything queued this pass, in order, then clear the queue.
    pub fn flush(&mut self) {
        for cmd in &self.cmds {
            let text = &self.arena[cmd.range.clone()];
            let st = cmd.style;
            let x = match st.align {
                TextAlign::Left => cmd.at.x,
                TextAlign::Center => cmd.at.x + cmd.w * 0.5,
                _ => cmd.at.x + cmd.w,
            };
            let cap = st.face.metrics().cap_height as f32 * st.pixel();
            let baseline = st.centered_baseline(cmd.at.y + cap * 0.5);
            if cmd.lines == 0 {
                draw_text_styled(text, x, baseline, st);
            } else {
                let mut y = baseline;
                for line in wrap_text(text, cmd.w, st).take(usize::from(cmd.lines)) {
                    draw_text_styled(line, x, y, st);
                    y += st.line_height();
                }
            }
        }
        self.cmds.clear();
        self.arena.clear();
    }
}

pub fn rect(min: Vec2, size: Vec2) -> ferravex_render::ui::UiRect {
    ferravex_render::ui::UiRect {
        min: [min.x, min.y],
        size: [size.x.max(0.), size.y.max(0.)],
    }
}

/// Chunky 7-row display glyphs for the logo (`#` = lit).
fn logo_glyph(c: char) -> [&'static str; 7] {
    match c {
        'G' => [
            ".####.", "##..##", "##....", "##.###", "##..##", "##..##", ".####.",
        ],
        'L' => [
            "##....", "##....", "##....", "##....", "##....", "##....", "######",
        ],
        'O' => [
            ".####.", "##..##", "##..##", "##..##", "##..##", "##..##", ".####.",
        ],
        'W' => [
            "##...##", "##...##", "##...##", "##.#.##", "#######", "###.###", "##...##",
        ],
        'T' => [
            "######", "..##..", "..##..", "..##..", "..##..", "..##..", "..##..",
        ],
        'I' => ["####", ".##.", ".##.", ".##.", ".##.", ".##.", "####"],
        'D' => [
            "#####.", "##..##", "##..##", "##..##", "##..##", "##..##", "#####.",
        ],
        'E' => [
            "######", "##....", "##....", "#####.", "##....", "##....", "######",
        ],
        _ => [""; 7],
    }
}

/// Width of the logo in pixels at a given cell size.
pub fn logo_width(word: &str, cell: f32) -> f32 {
    word.chars()
        .map(|c| logo_glyph(c)[0].len() as f32 + 1.)
        .sum::<f32>()
        .max(1.)
        * cell
        - cell
}

/// Animated logo: per-letter bob and hue drift, with a glowing pixel bloom.
pub fn logo(p: &mut Painter, center: Vec2, cell: f32, t: f32, motion: f32, palette: &[[f32; 3]]) {
    let word = "GLOWTIDE";
    let total = logo_width(word, cell);
    let mut x = center.x - total * 0.5;
    for (i, c) in word.chars().enumerate() {
        let rows = logo_glyph(c);
        let w = rows[0].len() as f32;
        let bob = (t * 2.2 - i as f32 * 0.55).sin() * cell * 0.9 * motion;
        let top = center.y - cell * 3.5 + bob;
        let hue = (t * 0.25 + i as f32 * 0.11).fract() * palette.len() as f32;
        let a = palette[hue as usize % palette.len()];
        let b = palette[(hue as usize + 1) % palette.len()];
        let mix = hue.fract();
        let col = [
            a[0] + (b[0] - a[0]) * mix,
            a[1] + (b[1] - a[1]) * mix,
            a[2] + (b[2] - a[2]) * mix,
        ];
        let glow_center = vec2(x + w * cell * 0.5, top + cell * 3.5);
        p.glow(glow_center, cell * 9., rgba(col, 0.28));
        for (row, line) in rows.iter().enumerate() {
            for (col_i, ch) in line.chars().enumerate() {
                if ch != '#' {
                    continue;
                }
                let px = vec2(x + col_i as f32 * cell, top + row as f32 * cell);
                // Depth: a darker extrusion under each pixel.
                p.rect(
                    false,
                    px + vec2(cell * 0.35, cell * 0.55),
                    Vec2::splat(cell),
                    rgba([col[0] * 0.25, col[1] * 0.25, col[2] * 0.35], 1.),
                );
                let shade = 1. - row as f32 * 0.06;
                let top_c = rgba(
                    [
                        (col[0] * 0.6 + 0.4) * shade,
                        (col[1] * 0.6 + 0.4) * shade,
                        (col[2] * 0.6 + 0.4) * shade,
                    ],
                    1.,
                );
                p.rect_v(false, px, Vec2::splat(cell), top_c, rgba(col, 1.));
                p.rect(true, px, Vec2::splat(cell), with_alpha(rgba(col, 1.), 0.25));
            }
        }
        x += (w + 1.) * cell;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_reuses_storage_and_measures_both_faces() {
        let mut t = Text::new();
        let caps = (t.arena.capacity(), t.cmds.capacity());
        t.label_fmt(
            Vec2::ZERO,
            200.,
            BODY,
            [1.; 4],
            Align::Left,
            format_args!("Level {}", 12),
        );
        t.big(Vec2::ZERO, 200., "00:42", 32., [1.; 4], Align::Center);
        assert_eq!(t.cmds.len(), 2);
        // Too wide for its box: shrinks in crisp 8 px steps.
        t.label(
            Vec2::ZERO,
            30.,
            "A very long name indeed",
            24.,
            [1.; 4],
            Align::Left,
        );
        assert!(t.cmds[2].style.size < 24.);
        t.cmds.clear();
        t.arena.clear();
        assert_eq!((t.arena.capacity(), t.cmds.capacity()), caps);
        assert_eq!(width_in(Face::Modern, "", 16.), 0.);
        assert!(width_in(Face::Classic, "AB", 16.) > width_in(Face::Classic, "A", 16.));
        assert!(logo_width("GLOWTIDE", 4.) > 150.);
        for c in "GLOWTIDE".chars() {
            let rows = logo_glyph(c);
            assert!(rows.iter().all(|r| r.len() == rows[0].len()), "{c}");
        }
    }
}
