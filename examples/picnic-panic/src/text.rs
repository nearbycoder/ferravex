//! Every piece of copy goes through this queue.
//!
//! Copy is formatted into a preallocated arena (no allocation after warm-up)
//! and drawn with the engine's styled text in one pass by [`Text::flush`], so a
//! frame's paper, shapes and icons always sit underneath its words. The
//! mixed-case Modern face carries UI and flavor copy; the chunky Classic face
//! carries big numbers and titles.
#![allow(clippy::too_many_arguments)]
use ferravex_render::prelude::{Color, Vec2};
use ferravex_render::text::{TextAlign, TextStyle, draw_text_styled, text_width, wrap_text};
use std::fmt::{self, Write};
use std::ops::Range;

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
    /// Modern face without a drop shadow, for ink on paper.
    Ink,
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
    /// Text scale preference (1 normal, up to 2 for large text).
    pub scale: f32,
}

fn style(face: Face, size: f32, c: Color, align: Align) -> TextStyle {
    let base = match face {
        Face::Modern => TextStyle::modern(size, c).with_shadow(Color::new(0., 0., 0., 0.45 * c.a)),
        Face::Ink => TextStyle::modern(size, c),
        Face::Classic => TextStyle::classic(size, c)
            .with_outline(Color::new(0.16, 0.09, 0.05, 0.9 * c.a))
            .with_shadow(Color::new(0., 0., 0., 0.4 * c.a)),
    };
    base.with_align(match align {
        Align::Left => TextAlign::Left,
        Align::Center => TextAlign::Center,
        Align::Right => TextAlign::Right,
    })
}

/// Width of a single line in logical pixels.
pub fn width_in(face: Face, text: &str, size: f32) -> f32 {
    text_width(
        text,
        style(face, size, Color::new(1., 1., 1., 1.), Align::Left),
    )
}

struct Arena<'a>(&'a mut String, usize);
impl Write for Arena<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let room = self.1.saturating_sub(self.0.len());
        if s.len() <= room {
            self.0.push_str(s);
        } else {
            let mut cut = room;
            while !s.is_char_boundary(cut) {
                cut -= 1;
            }
            self.0.push_str(&s[..cut]);
        }
        Ok(())
    }
}

impl Default for Text {
    fn default() -> Self {
        Self::new()
    }
}

impl Text {
    pub fn new() -> Self {
        Self {
            arena: String::with_capacity(32 * 1024),
            cmds: Vec::with_capacity(512),
            scale: 1.,
        }
    }

    fn push(
        &mut self,
        face: Face,
        at: Vec2,
        w: f32,
        size: f32,
        c: Color,
        align: Align,
        lines: u8,
        args: fmt::Arguments,
    ) {
        if c.a <= 0.004 || self.cmds.len() == self.cmds.capacity() {
            return;
        }
        let start = self.arena.len();
        let cap = self.arena.capacity();
        let _ = Arena(&mut self.arena, cap).write_fmt(args);
        let range = start..self.arena.len();
        let mut st = style(face, size, c, align);
        if lines == 0 {
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

    pub fn label(&mut self, at: Vec2, w: f32, text: &str, size: f32, c: Color, align: Align) {
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

    pub fn ink(&mut self, at: Vec2, w: f32, text: &str, size: f32, c: Color, align: Align) {
        self.push(Face::Ink, at, w, size, c, align, 0, format_args!("{text}"));
    }

    pub fn label_fmt(
        &mut self,
        at: Vec2,
        w: f32,
        size: f32,
        c: Color,
        align: Align,
        args: fmt::Arguments,
    ) {
        self.push(Face::Modern, at, w, size, c, align, 0, args);
    }

    pub fn ink_fmt(
        &mut self,
        at: Vec2,
        w: f32,
        size: f32,
        c: Color,
        align: Align,
        args: fmt::Arguments,
    ) {
        self.push(Face::Ink, at, w, size, c, align, 0, args);
    }

    pub fn big(&mut self, at: Vec2, w: f32, text: &str, size: f32, c: Color, align: Align) {
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
        c: Color,
        align: Align,
        args: fmt::Arguments,
    ) {
        self.push(Face::Classic, at, w, size, c, align, 0, args);
    }

    /// Word-wrapped copy, at most `lines` lines; returns the height used.
    pub fn wrapped(
        &mut self,
        face: Face,
        at: Vec2,
        w: f32,
        lines: usize,
        text: &str,
        size: f32,
        c: Color,
        align: Align,
    ) -> f32 {
        let st = style(face, size, c, align);
        let n = wrap_text(text, w, st).take(lines).count();
        self.push(
            face,
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

    /// Height that [`Self::wrapped`] would use.
    pub fn wrapped_height(face: Face, w: f32, lines: usize, text: &str, size: f32) -> f32 {
        let st = style(face, size, Color::new(1., 1., 1., 1.), Align::Left);
        wrap_text(text, w, st).take(lines).count() as f32 * st.line_height()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_reuses_storage_and_shrinks_long_lines() {
        let mut t = Text::new();
        let caps = (t.arena.capacity(), t.cmds.capacity());
        t.label_fmt(
            Vec2::ZERO,
            200.,
            BODY,
            Color::new(1., 1., 1., 1.),
            Align::Left,
            format_args!("Pranks {}/{}", 3, 7),
        );
        t.label(
            Vec2::ZERO,
            30.,
            "A very long line of picnic copy",
            24.,
            Color::new(1., 1., 1., 1.),
            Align::Left,
        );
        assert!(t.cmds[1].style.size < 24.);
        t.cmds.clear();
        t.arena.clear();
        assert_eq!((t.arena.capacity(), t.cmds.capacity()), caps);
        assert!(width_in(Face::Classic, "AB", 16.) > width_in(Face::Classic, "A", 16.));
    }
}
