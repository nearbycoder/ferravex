//! Bounded screen-space HUD/menu drawing. No camera, lighting or material changes.
use super::{AMBER, INK, LINE, PANEL};
use crate::{
    Color,
    input::{InputCode, PlatformEvent, touch},
};
pub use ferravex_play::ui::{Rect as UiRect, UiError, Viewport, WidgetId, slots};
use ferravex_play::ui::{UiState, Widget};
use std::{
    fmt::{self, Write},
    ops::Range,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}
#[derive(Clone, Copy, Debug)]
pub struct TextStyle {
    pub size: f32,
    pub color: Color,
    pub align: Align,
    pub wrap: bool,
    pub shadow: bool,
}
impl Default for TextStyle {
    fn default() -> Self {
        Self {
            size: 18.,
            color: INK,
            align: Align::Left,
            wrap: false,
            shadow: false,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct ButtonStyle {
    pub fill: Color,
    pub ink: Color,
    pub focus: Color,
    pub disabled: Color,
    pub size: f32,
}
impl Default for ButtonStyle {
    fn default() -> Self {
        Self {
            fill: PANEL,
            ink: INK,
            focus: AMBER,
            disabled: LINE,
            size: 18.,
        }
    }
}
#[derive(Clone)]
enum Kind {
    Panel(Color),
    Gradient(Color, Color),
    Text(Range<usize>, TextStyle),
    Button(WidgetId, Range<usize>, ButtonStyle, bool),
}
#[derive(Clone)]
struct Command {
    rect: UiRect,
    clip: UiRect,
    kind: Kind,
}
/// Reuse one instance across frames. Errors reject a command atomically.
/// Budgets: 1024 commands/widgets, 32 KiB UTF-8, 196608 vertices and 16 clip levels.
pub struct Ui {
    state: UiState,
    commands: Vec<Command>,
    text: String,
    clips: [UiRect; 16],
    clip_len: usize,
    viewport: Viewport,
    vertices: usize,
    active: Option<WidgetId>,
    hovered: Option<WidgetId>,
    touch: Option<u64>,
    frame: Option<u64>,
    epoch: u64,
    shift: u8,
    pad_epoch: Option<(u8, u32)>,
    auto_focus: bool,
    mouse_capture: bool,
}
impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}
impl Ui {
    pub fn new() -> Self {
        let viewport = Viewport::fit([1.; 2], [1.; 2], [0.; 4]).unwrap();
        Self {
            state: UiState::with_capacity(1024),
            commands: Vec::with_capacity(1024),
            text: String::with_capacity(32768),
            clips: [viewport.bounds(); 16],
            clip_len: 1,
            viewport,
            vertices: 0,
            active: None,
            hovered: None,
            touch: None,
            frame: None,
            epoch: 0,
            shift: 0,
            pad_epoch: None,
            auto_focus: true,
            mouse_capture: false,
        }
    }
    /// Reserved CPU storage, including this value; excludes the renderer's shared stream.
    pub fn reserved_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.commands.capacity() * std::mem::size_of::<Command>()
            + self.text.capacity()
            + self.state.capacity() * std::mem::size_of::<Widget>()
    }
    pub fn begin(&mut self, viewport: Viewport) {
        self.viewport = viewport;
        self.commands.clear();
        self.text.clear();
        self.vertices = 0;
        self.clips[0] = viewport.bounds();
        self.clip_len = 1;
        self.state.begin_frame();
        self.active = None;
    }
    /// Discard lower widgets from interaction while preserving their paint order.
    /// Queue a full-screen panel after this call and before modal buttons.
    pub fn modal(&mut self, blocker: WidgetId) -> Result<(), UiError> {
        self.state.begin_frame();
        self.state.add(Widget {
            id: blocker,
            rect: self.viewport.bounds(),
            clip: Some(self.viewport.bounds()),
            enabled: true,
            focusable: false,
        })
    }
    /// Menus may accept the first button immediately; gameplay overlays can require
    /// explicit navigation/pointer focus so a shared gamepad action keeps its role.
    pub fn set_auto_focus(&mut self, enabled: bool) {
        self.auto_focus = enabled;
    }
    pub fn focused(&self) -> Option<WidgetId> {
        self.state.focused()
    }
    pub fn activated(&self, id: WidgetId) -> bool {
        self.active == Some(id)
    }
    pub fn cancel(&mut self) {
        self.state.cancel_pointer();
        self.touch = None;
        self.mouse_capture = false;
    }
    pub fn push_clip(&mut self, rect: UiRect) -> Result<(), UiError> {
        if !rect.valid() {
            return Err(UiError::InvalidLayout);
        }
        if self.clip_len == 16 {
            return Err(UiError::Capacity);
        }
        self.clips[self.clip_len] = self.clip().intersect(rect).unwrap_or(UiRect {
            min: [0.; 2],
            size: [0.; 2],
        });
        self.clip_len += 1;
        Ok(())
    }
    pub fn pop_clip(&mut self) {
        self.clip_len = self.clip_len.saturating_sub(1).max(1);
    }
    fn clip(&self) -> UiRect {
        self.clips[self.clip_len - 1]
    }
    fn reserve(&self, rect: UiRect, vertices: usize) -> Result<(), UiError> {
        if !rect.valid() {
            return Err(UiError::InvalidLayout);
        }
        if self.commands.len() == 1024 || vertices > 196608 - self.vertices {
            return Err(UiError::Capacity);
        }
        Ok(())
    }
    pub fn panel(&mut self, rect: UiRect, color: Color) -> Result<(), UiError> {
        self.reserve(rect, 6)?;
        if !valid_color(color) {
            return Err(UiError::InvalidLayout);
        }
        self.commands.push(Command {
            rect,
            clip: self.clip(),
            kind: Kind::Panel(color),
        });
        self.vertices += 6;
        Ok(())
    }
    /// Horizontal color/alpha gradient; clipping preserves the original interpolation.
    pub fn gradient(&mut self, rect: UiRect, left: Color, right: Color) -> Result<(), UiError> {
        self.reserve(rect, 6)?;
        if !valid_color(left) || !valid_color(right) {
            return Err(UiError::InvalidLayout);
        }
        self.commands.push(Command {
            rect,
            clip: self.clip(),
            kind: Kind::Gradient(left, right),
        });
        self.vertices += 6;
        Ok(())
    }
    /// Format directly into the persistent UTF-8 arena; no temporary String.
    pub fn text_fmt(
        &mut self,
        rect: UiRect,
        style: TextStyle,
        args: fmt::Arguments<'_>,
    ) -> Result<(), UiError> {
        if !valid_text(style) {
            return Err(UiError::InvalidLayout);
        }
        self.reserve(rect, 0)?;
        let start = self.text.len();
        if (Arena(&mut self.text)).write_fmt(args).is_err() {
            self.text.truncate(start);
            return Err(UiError::Capacity);
        }
        let vertices = self.text[start..].chars().count() * 126 * if style.shadow { 2 } else { 1 };
        if let Err(e) = self.reserve(rect, vertices) {
            self.text.truncate(start);
            return Err(e);
        }
        self.commands.push(Command {
            rect,
            clip: self.clip(),
            kind: Kind::Text(start..self.text.len(), style),
        });
        self.vertices += vertices;
        Ok(())
    }
    pub fn text(&mut self, rect: UiRect, text: &str, style: TextStyle) -> Result<(), UiError> {
        self.text_fmt(rect, style, format_args!("{text}"))
    }
    pub fn button(
        &mut self,
        id: WidgetId,
        rect: UiRect,
        text: &str,
        enabled: bool,
        style: ButtonStyle,
    ) -> Result<(), UiError> {
        let vertices = text.chars().count() * 126 + 30;
        self.reserve(rect, vertices)?;
        if !valid_color(style.fill)
            || !valid_color(style.ink)
            || !valid_color(style.focus)
            || !valid_color(style.disabled)
            || !style.size.is_finite()
            || !(1. ..=256.).contains(&style.size)
        {
            return Err(UiError::InvalidLayout);
        }
        if text.len() > 32768 - self.text.len() {
            return Err(UiError::Capacity);
        }
        self.state.add(Widget {
            id,
            rect,
            clip: Some(self.clip()),
            enabled,
            focusable: true,
        })?;
        let start = self.text.len();
        self.text.push_str(text);
        self.commands.push(Command {
            rect,
            clip: self.clip(),
            kind: Kind::Button(id, start..self.text.len(), style, enabled),
        });
        self.vertices += vertices;
        Ok(())
    }
    /// Finish layout before input. Block while the host's settings editor is open.
    /// Keyboard Tab/arrows/Enter, gamepad D-pad/A and one captured touch are supported.
    /// Include a player's remapped global confirm action and selected gamepad.
    pub fn finish_player(&mut self, player: &mut crate::input::PlayerInput, enabled: bool) {
        let accept = enabled && player.pressed("confirm");
        self.finish_with_gamepad(enabled, player.settings().gamepad);
        if accept
            && !player.controls_open()
            && crate::read(|s| s.input.focused && !s.input.overflow && !s.controls_open)
        {
            self.accept();
        }
    }
    pub fn finish(&mut self, enabled: bool) {
        self.finish_with_gamepad(enabled, 0);
    }
    /// Use the player profile’s selected pad (0..4).
    pub fn finish_with_gamepad(&mut self, enabled: bool, pad: u8) {
        self.state.end_frame();
        self.hovered = None;
        let mut events = [PlatformEvent::None; crate::input::MAX_EVENTS];
        let (frame, epoch, focused, overflow, blocked, pointer, pad_epoch) = crate::read(|s| {
            let source = s.input.events();
            events[..source.len()].copy_from_slice(source);
            (
                s.frame,
                s.input.focus_epoch,
                s.input.focused,
                s.input.overflow,
                s.controls_open,
                [s.mouse.x, s.mouse.y],
                s.input
                    .pads
                    .get(pad as usize)
                    .filter(|p| p.connected)
                    .map(|p| (pad, p.epoch)),
            )
        });
        let fresh_pad = self.pad_epoch != pad_epoch;
        self.pad_epoch = pad_epoch;
        if epoch != self.epoch || !focused || overflow {
            self.shift = 0;
        }
        if epoch != self.epoch || !focused || overflow || !enabled || blocked {
            self.cancel();
        }
        self.epoch = epoch;
        if self.frame == Some(frame) {
            return;
        }
        self.frame = Some(frame);
        if !enabled || blocked || !focused || overflow {
            return;
        }
        self.hovered = self
            .viewport
            .unproject(pointer)
            .and_then(|p| self.state.hit_test(p));
        let mut event_pointer = pointer;
        for event in events {
            match event {
                PlatformEvent::PointerPosition(p) => event_pointer = [p.x, p.y],
                PlatformEvent::None => break,
                PlatformEvent::Value(InputCode::Mouse(1), v) if self.touch.is_none() => {
                    let p = self
                        .viewport
                        .unproject(event_pointer)
                        .unwrap_or([f32::NEG_INFINITY; 2]);
                    if v >= 0.5 {
                        self.mouse_capture = true;
                        self.state.pointer_down(p);
                    } else if self.mouse_capture {
                        self.mouse_capture = false;
                        self.active = self.state.pointer_up(p).or(self.active);
                    }
                }
                PlatformEvent::Touch(e) => {
                    let p = self
                        .viewport
                        .unproject([e.position.x, e.position.y])
                        .unwrap_or([f32::NEG_INFINITY; 2]);
                    match e.phase {
                        touch::Phase::Start if self.touch.is_none() && !self.mouse_capture => {
                            self.touch = Some(e.id);
                            self.state.pointer_down(p);
                        }
                        touch::Phase::End if self.touch == Some(e.id) => {
                            self.touch = None;
                            self.active = self.state.pointer_up(p).or(self.active);
                        }
                        touch::Phase::Cancel if self.touch == Some(e.id) => self.cancel(),
                        _ => {}
                    }
                }
                PlatformEvent::Value(InputCode::Key(key), v) if key == 0xffe1 || key == 0xffe2 => {
                    let bit = if key == 0xffe1 { 1 } else { 2 };
                    if v >= 0.5 {
                        self.shift |= bit;
                    } else {
                        self.shift &= !bit;
                    }
                }
                PlatformEvent::Value(InputCode::Key(key), v) if v >= 0.5 => match key {
                    0xff09 => {
                        self.state.focus_next(self.shift != 0);
                    }
                    0xff54 | 0xff53 => {
                        self.state.focus_next(false);
                    }
                    0xff52 | 0xff51 => {
                        self.state.focus_next(true);
                    }
                    0xff0d => self.accept(),
                    0xff1b => self.cancel(),
                    _ => {}
                },
                PlatformEvent::Pad {
                    index,
                    code: InputCode::PadButton(b),
                    value,
                } if index == pad && !fresh_pad && value >= 0.5 => match b {
                    12 | 14 => {
                        self.state.focus_next(true);
                    }
                    13 | 15 => {
                        self.state.focus_next(false);
                    }
                    0 => self.accept(),
                    _ => {}
                },
                _ => {}
            }
        }
    }
    fn accept(&mut self) {
        if self.auto_focus && self.state.focused().is_none() {
            self.state.focus_next(false);
        }
        self.active = self.state.activate_focused().or(self.active);
    }
    /// Append unlit clip-space geometry in paint order, independent of the world camera.
    pub fn paint(&self) {
        crate::edit(|s| {
            if s.w == 0 || s.h == 0 {
                return;
            }
            let start = s.vertices.len() as u32;
            let mut painter = Painter {
                vertices: &mut s.vertices,
                viewport: self.viewport,
                screen: [s.w as f32, s.h as f32],
            };
            for c in &self.commands {
                match &c.kind {
                    Kind::Panel(color) => painter.rect(c.rect, c.clip, *color),
                    Kind::Gradient(left, right) => painter.gradient(c.rect, c.clip, *left, *right),
                    Kind::Text(range, style) => {
                        painter.text(c.rect, c.clip, &self.text[range.clone()], *style)
                    }
                    Kind::Button(id, range, style, enabled) => {
                        painter.rect(
                            c.rect,
                            c.clip,
                            if *enabled { style.fill } else { style.disabled },
                        );
                        if *enabled
                            && (self.state.focused() == Some(*id) || self.hovered == Some(*id))
                        {
                            let r = c.rect;
                            for edge in [
                                UiRect {
                                    min: r.min,
                                    size: [r.size[0], 2.],
                                },
                                UiRect {
                                    min: [r.min[0], r.min[1] + r.size[1] - 2.],
                                    size: [r.size[0], 2.],
                                },
                                UiRect {
                                    min: r.min,
                                    size: [2., r.size[1]],
                                },
                                UiRect {
                                    min: [r.min[0] + r.size[0] - 2., r.min[1]],
                                    size: [2., r.size[1]],
                                },
                            ] {
                                painter.rect(edge, c.clip, style.focus);
                            }
                        }
                        let rect = UiRect {
                            min: [
                                c.rect.min[0] + 8.,
                                c.rect.min[1] + (c.rect.size[1] - style.size * 0.875) * 0.5,
                            ],
                            size: [(c.rect.size[0] - 16.).max(0.), style.size],
                        };
                        painter.text(
                            rect,
                            c.clip,
                            &self.text[range.clone()],
                            TextStyle {
                                size: style.size,
                                color: style.ink,
                                align: Align::Center,
                                ..Default::default()
                            },
                        );
                    }
                }
            }
            let count = s.vertices.len() as u32 - start;
            if count > 0 {
                s.record_immediate(start, count, false);
            }
        });
    }
}
fn valid_color(c: Color) -> bool {
    [c.r, c.g, c.b, c.a].into_iter().all(f32::is_finite)
}
fn valid_text(s: TextStyle) -> bool {
    s.size.is_finite() && (1. ..=256.).contains(&s.size) && valid_color(s.color)
}
struct Arena<'a>(&'a mut String);
impl Write for Arena<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        if s.len() > 32768 - self.0.len() {
            return Err(fmt::Error);
        }
        self.0.push_str(s);
        Ok(())
    }
}
struct Painter<'a> {
    vertices: &'a mut Vec<[f32; 8]>,
    viewport: Viewport,
    screen: [f32; 2],
}
impl Painter<'_> {
    fn rect(&mut self, r: UiRect, clip: UiRect, color: Color) {
        if let Some(r) = r.intersect(clip) {
            self.quad(r, color, color);
        }
    }
    fn gradient(&mut self, original: UiRect, clip: UiRect, left: Color, right: Color) {
        let Some(r) = original.intersect(clip) else {
            return;
        };
        let color = |x: f32| {
            let t = ((x - original.min[0]) / original.size[0]).clamp(0., 1.) as f64;
            let mix = |a: f32, b: f32| ((a as f64) * (1. - t) + (b as f64) * t) as f32;
            Color::new(
                mix(left.r, right.r),
                mix(left.g, right.g),
                mix(left.b, right.b),
                mix(left.a, right.a),
            )
        };
        self.quad(r, color(r.min[0]), color(r.min[0] + r.size[0]));
    }
    fn quad(&mut self, r: UiRect, left: Color, right: Color) {
        let point = |x: f32, y: f32, c: Color| {
            let px = self.viewport.pixels().min[0] + x * self.viewport.scale();
            let py = self.viewport.pixels().min[1] + y * self.viewport.scale();
            [
                px / self.screen[0] * 2. - 1.,
                1. - py / self.screen[1] * 2.,
                0.,
                1.,
                c.r,
                c.g,
                c.b,
                c.a,
            ]
        };
        let [x, y] = r.min;
        let [w, h] = r.size;
        self.vertices.extend_from_slice(&[
            point(x, y, left),
            point(x + w, y, right),
            point(x + w, y + h, right),
            point(x, y, left),
            point(x + w, y + h, right),
            point(x, y + h, left),
        ]);
    }
    fn text(&mut self, r: UiRect, clip: UiRect, text: &str, style: TextStyle) {
        let Some(clip) = r.intersect(clip) else {
            return;
        };
        // Fixed bitmap metrics, UTF-8 boundaries, explicit newlines and character wrapping.
        // Unsupported glyphs use '?' rather than silently disappearing.
        let max_columns = text
            .split('\n')
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0)
            .max(1);
        let size = if style.wrap {
            style.size
        } else {
            style.size.min(r.size[0] * 8. / (max_columns as f32 * 6.))
        };
        if size <= 0. {
            return;
        }
        let cell = size / 8.;
        let advance = 6. * cell;
        let columns = (r.size[0] / advance).floor().max(1.) as usize;
        for (line_index, line) in
            Lines::new(text, if style.wrap { columns } else { usize::MAX }).enumerate()
        {
            let y = r.min[1] + line_index as f32 * size;
            if y >= r.min[1] + r.size[1] {
                break;
            }
            let width = line.chars().count() as f32 * advance;
            let x = r.min[0]
                + match style.align {
                    Align::Left => 0.,
                    Align::Center => (r.size[0] - width) * 0.5,
                    Align::Right => r.size[0] - width,
                };
            for (index, c) in line.chars().enumerate() {
                let c = c.to_ascii_uppercase();
                let glyph = crate::font::glyph(if c == ' ' || crate::font::glyph(c) != [0; 7] {
                    c
                } else {
                    '?'
                });
                for (row, bits) in glyph.into_iter().enumerate() {
                    let mut col = 0;
                    while col < 5 {
                        if bits & (1 << (4 - col)) == 0 {
                            col += 1;
                            continue;
                        }
                        let start = col;
                        while col < 5 && bits & (1 << (4 - col)) != 0 {
                            col += 1;
                        }
                        let rect = UiRect {
                            min: [
                                x + index as f32 * advance + start as f32 * cell,
                                y + row as f32 * cell,
                            ],
                            size: [(col - start) as f32 * cell, cell],
                        };
                        if style.shadow {
                            self.rect(
                                UiRect {
                                    min: [rect.min[0] + cell * 0.5, rect.min[1] + cell * 0.5],
                                    ..rect
                                },
                                clip,
                                Color::new(0., 0., 0., 0.7),
                            );
                        }
                        self.rect(rect, clip, style.color);
                    }
                }
            }
        }
    }
}
struct Lines<'a> {
    text: &'a str,
    columns: usize,
}
impl<'a> Lines<'a> {
    fn new(text: &'a str, columns: usize) -> Self {
        Self { text, columns }
    }
}
impl<'a> Iterator for Lines<'a> {
    type Item = &'a str;
    fn next(&mut self) -> Option<Self::Item> {
        if self.text.is_empty() {
            return None;
        }
        let mut end = self.text.len();
        let mut consumed = end;
        for (count, (i, c)) in self.text.char_indices().enumerate() {
            if c == '\n' {
                end = i;
                consumed = i + 1;
                break;
            }
            if count == self.columns {
                end = i;
                consumed = i;
                break;
            }
        }
        let line = &self.text[..end];
        self.text = &self.text[consumed..];
        Some(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const RECT: UiRect = UiRect {
        min: [20.; 2],
        size: [80., 40.],
    };
    fn reset() {
        crate::edit(|s| *s = crate::State::default());
    }
    fn next() {
        crate::edit(|s| {
            s.frame += 1;
            s.input.finish_frame();
            s.vertices.clear();
            s.batches.clear();
            s.retained.commands.clear();
        });
    }
    fn begin(ui: &mut Ui) {
        ui.begin(Viewport::fit([200.; 2], [200.; 2], [0.; 4]).unwrap());
    }
    fn button(ui: &mut Ui, id: u64, enabled: bool) {
        ui.button(id, RECT, "PLAY", enabled, ButtonStyle::default())
            .unwrap();
    }
    #[test]
    fn ordered_taps_release_outside_multitouch_and_focus_loss() {
        reset();
        let mut ui = Ui::new();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::touch(7, 0, 40., 40.);
        crate::touch(8, 0, 40., 40.);
        crate::touch(8, 2, 40., 40.);
        ui.finish(true);
        assert!(!ui.activated(1));
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::touch(7, 2, 150., 150.);
        ui.finish(true);
        assert!(!ui.activated(1));
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::touch(9, 0, 40., 40.);
        crate::touch(9, 2, 40., 40.);
        ui.finish(true);
        assert!(ui.activated(1));
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::touch(10, 0, 40., 40.);
        ui.finish(true);
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::edit(|s| s.input.focus(false));
        ui.finish(true);
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::edit(|s| s.input.focus(true));
        crate::touch(10, 2, 40., 40.);
        ui.finish(true);
        assert!(!ui.activated(1));
    }
    #[test]
    fn event_time_mouse_positions_and_pointer_ownership_prevent_false_clicks() {
        reset();
        let mut ui = Ui::new();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::pointer(150., 150., 0., 0.);
        crate::button(1, true);
        crate::pointer(40., 40., -110., -110.);
        crate::button(1, false);
        ui.finish(true);
        assert!(
            !ui.activated(1),
            "outside press then inside release in one frame"
        );
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::pointer(40., 40., 0., 0.);
        crate::button(1, true);
        crate::touch(7, 0, 40., 40.);
        crate::touch(7, 2, 40., 40.);
        ui.finish(true);
        assert!(!ui.activated(1), "touch cannot steal mouse capture");
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::button(1, false);
        ui.finish(true);
        assert!(ui.activated(1));
    }
    #[test]
    fn keyboard_pad_modal_and_disabled_focus() {
        reset();
        let mut ui = Ui::new();
        begin(&mut ui);
        button(&mut ui, 1, false);
        button(&mut ui, 2, true);
        crate::key(0xff09, true);
        crate::key(0xff09, false);
        crate::key(0xff0d, true);
        crate::key(0xff0d, false);
        ui.finish(true);
        assert!(ui.activated(2));
        next();
        crate::edit(|s| s.input.pad_connected(1, true));
        begin(&mut ui);
        ui.finish_with_gamepad(false, 1);
        next();
        begin(&mut ui);
        button(&mut ui, 2, true);
        ui.modal(100).unwrap();
        button(&mut ui, 3, true);
        crate::edit(|s| {
            s.input.pad_connected(1, true);
            s.input.pad_button(1, 0, 1.);
        });
        ui.finish_with_gamepad(true, 1);
        assert_eq!(ui.focused(), Some(3));
        assert!(ui.activated(3));
        next();
        begin(&mut ui);
        button(&mut ui, 3, false);
        crate::touch(1, 0, 40., 40.);
        crate::touch(1, 2, 40., 40.);
        ui.finish(true);
        assert!(!ui.activated(3));
        assert_eq!(ui.focused(), None);
        next();
        begin(&mut ui);
        button(&mut ui, 3, true);
        crate::touch(2, 0, 40., 40.);
        crate::touch(2, 2, 40., 40.);
        crate::edit(|s| s.controls_open = true);
        ui.finish(true);
        assert!(!ui.activated(3));
    }
    #[test]
    fn remapped_confirm_and_explicit_focus_preserve_gameplay_roles() {
        use crate::input::{ActionKind, Binding, InputCode, PlayerInput};
        reset();
        let mut player = PlayerInput::transient("ui-test");
        player
            .global_action(
                "confirm",
                "Confirm",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(106))],
            )
            .unwrap();
        let mut ui = Ui::new();
        ui.set_auto_focus(false);
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::key(106, true);
        crate::key(106, false);
        player.sample(true);
        ui.finish_player(&mut player, true);
        assert!(!ui.activated(1));
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::key(0xff09, true);
        crate::key(0xff09, false);
        crate::key(106, true);
        crate::key(106, false);
        player.sample(true);
        ui.finish_player(&mut player, true);
        assert!(ui.activated(1));
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::key(106, true);
        crate::key(106, false);
        player.sample(true);
        ui.finish_player(&mut player, false);
        assert!(!ui.activated(1));
    }
    #[test]
    fn clipped_screen_geometry_keeps_camera_and_world_order() {
        reset();
        crate::edit(|s| {
            s.w = 400;
            s.h = 200;
        });
        let matrix = crate::Mat4::from_scale(crate::Vec3::splat(7.));
        crate::edit(|s| {
            s.matrix = matrix;
            s.depth = true;
        });
        let mut ui = Ui::new();
        ui.begin(Viewport::fit([400., 200.], [100.; 2], [0.; 4]).unwrap());
        ui.push_clip(UiRect {
            min: [20., 30.],
            size: [10., 20.],
        })
        .unwrap();
        ui.panel(
            UiRect {
                min: [0.; 2],
                size: [100.; 2],
            },
            INK,
        )
        .unwrap();
        ui.text(
            RECT,
            "?é\nAB",
            TextStyle {
                wrap: true,
                shadow: true,
                ..Default::default()
            },
        )
        .unwrap();
        ui.finish(false);
        ui.paint();
        crate::read(|s| {
            assert_eq!(s.matrix, matrix);
            assert!(s.depth);
            assert!(!s.batches[0].depth);
            assert!(
                s.vertices
                    .iter()
                    .all(|v| (-0.301..=-0.199).contains(&v[0]) && (-0.001..=0.401).contains(&v[1]))
            );
            assert!(s.vertices.len() >= 6);
            assert!(s.vertices.len() <= ui.vertices);
        });
    }
    #[test]
    fn clipped_gradient_interpolates_original_colors() {
        reset();
        crate::edit(|s| {
            s.w = 200;
            s.h = 200;
        });
        let mut ui = Ui::new();
        begin(&mut ui);
        ui.push_clip(UiRect {
            min: [50., 0.],
            size: [100., 200.],
        })
        .unwrap();
        ui.gradient(
            UiRect {
                min: [0.; 2],
                size: [200.; 2],
            },
            Color::new(1., 0., 0., 1.),
            Color::new(0., 0., 1., 0.),
        )
        .unwrap();
        ui.paint();
        crate::read(|s| {
            assert_eq!(s.vertices.len(), 6);
            assert_eq!(&s.vertices[0][4..], &[0.75, 0., 0.25, 0.75]);
            assert_eq!(&s.vertices[1][4..], &[0.25, 0., 0.75, 0.25]);
        });
    }
    #[test]
    fn bounded_commands_text_rollback_and_utf8_wrapping() {
        reset();
        let mut ui = Ui::new();
        begin(&mut ui);
        let huge = "é".repeat(17000);
        assert_eq!(
            ui.text(RECT, &huge, TextStyle::default()),
            Err(UiError::Capacity)
        );
        assert!(ui.text.is_empty());
        assert!(ui.commands.is_empty());
        assert_eq!(
            Lines::new("é日A\nB", 2).collect::<Vec<_>>(),
            vec!["é日", "A", "B"]
        );
        for _ in 1..16 {
            ui.push_clip(RECT).unwrap();
        }
        assert_eq!(ui.push_clip(RECT), Err(UiError::Capacity));
        for _ in 0..1024 {
            ui.panel(RECT, INK).unwrap();
        }
        assert_eq!(ui.panel(RECT, INK), Err(UiError::Capacity));
        let capacities = (ui.commands.capacity(), ui.text.capacity());
        println!("shared UI reserved bytes: {}", ui.reserved_bytes());
        for i in 0..120 {
            next();
            begin(&mut ui);
            ui.text_fmt(RECT, TextStyle::default(), format_args!("HP {i}"))
                .unwrap();
            button(&mut ui, 1, true);
            ui.finish(true);
            ui.paint();
        }
        assert_eq!((ui.commands.capacity(), ui.text.capacity()), capacities);
    }
}
