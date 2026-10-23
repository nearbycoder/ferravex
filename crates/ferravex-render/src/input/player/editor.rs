use super::*;
use std::fmt::{self, Write};

fn write_code_label(out: &mut impl Write, code: InputCode) -> fmt::Result {
    match code {
        InputCode::Key(32) => out.write_str("Space"),
        InputCode::Key(0xff1b) => out.write_str("Esc"),
        InputCode::Key(0xff0d) => out.write_str("Enter"),
        InputCode::Key(0xff09) => out.write_str("Tab"),
        InputCode::Key(0xff08) => out.write_str("Backspace"),
        InputCode::Key(0xffff) => out.write_str("Delete"),
        InputCode::Key(0xff51) => out.write_str("Left"),
        InputCode::Key(0xff53) => out.write_str("Right"),
        InputCode::Key(0xff52) => out.write_str("Up"),
        InputCode::Key(0xff54) => out.write_str("Down"),
        InputCode::Key(0xff50) => out.write_str("Home"),
        InputCode::Key(0xff57) => out.write_str("End"),
        InputCode::Key(0xff55) => out.write_str("PageUp"),
        InputCode::Key(0xff56) => out.write_str("PageDown"),
        InputCode::Key(0xff63) => out.write_str("Insert"),
        InputCode::Key(0xffe1) => out.write_str("Left Shift"),
        InputCode::Key(0xffe2) => out.write_str("Right Shift"),
        InputCode::Key(0xffe3) => out.write_str("Left Ctrl"),
        InputCode::Key(0xffe4) => out.write_str("Right Ctrl"),
        InputCode::Key(0xffe9) => out.write_str("Left Alt"),
        InputCode::Key(0xffea) => out.write_str("Right Alt"),
        InputCode::Key(n @ 0xffbe..=0xffc9) => write!(out, "F{}", n - 0xffbe + 1),
        InputCode::Key(n) if (32..=126).contains(&n) => {
            out.write_char((n as u8 as char).to_ascii_uppercase())
        }
        InputCode::Key(n) => write!(out, "Key {n}"),
        InputCode::Mouse(n) => write!(out, "Mouse {n}"),
        InputCode::MouseDelta(0) => out.write_str("Mouse X"),
        InputCode::MouseDelta(_) => out.write_str("Mouse Y"),
        InputCode::Wheel(n) => write!(out, "Wheel {n}"),
        InputCode::PadButton(n) => write!(out, "Pad {n}"),
        InputCode::PadAxis(n) => write!(out, "Pad axis {n}"),
        InputCode::Touch(n) => write!(out, "Touch {n}"),
        InputCode::TouchDelta(n) => write!(out, "Touch look {n}"),
    }
}

struct BindingLabel(Binding);
impl fmt::Display for BindingLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_code_label(f, self.0.code)?;
        if self.0.scale != 1. {
            write!(f, " x{}", self.0.scale)?;
        }
        Ok(())
    }
}

// The first pass measures the exact byte count. The second writes only after
// capacity is checked, so failure cannot leave a misleading partial key name.
struct HintWriter<'a> {
    buffer: Option<&'a mut [u8]>,
    len: usize,
}
impl Write for HintWriter<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len.checked_add(text.len()).ok_or(fmt::Error)?;
        if let Some(buffer) = &mut self.buffer {
            buffer
                .get_mut(self.len..end)
                .ok_or(fmt::Error)?
                .copy_from_slice(text.as_bytes());
        }
        self.len = end;
        Ok(())
    }
}

/// A label actually drawn for one output of an installed touch-control slot.
/// `slot` is the stable slice index passed to [`PlayerInput::set_touch_layout`],
/// also used by [`super::super::touch_layout::TouchSlot`]. A stick can have
/// separate labels for its X and Y codes; a look region uses `TouchDelta` codes.
/// Labels describe the current drawing, including any customized geometry.
/// Omit controls whose labels are hidden. Merely installing a hit region does
/// not establish visibility.
///
/// ```
/// use ferravex_render::input::{InputCode, VisibleTouchLabel};
/// let gas = VisibleTouchLabel { slot: 2, code: InputCode::Touch(14), label: "GAS" };
/// assert_eq!(gas.slot, 2); // Slot identity differs from output channel 14.
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleTouchLabel<'a> {
    pub slot: usize,
    pub code: InputCode,
    pub label: &'a str,
}

/// Errors from the opt-in visible-control formatter. Every error leaves the
/// entire caller buffer unchanged; the legacy [`HintError`] is preserved.
///
/// ```
/// use ferravex_render::input::{HintError, VisibleHintError};
/// let error = VisibleHintError::Hint(HintError::BufferTooSmall { required: 12 });
/// assert_eq!(error.to_string(), "hint buffer requires 12 bytes");
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisibleHintError {
    Hint(HintError),
    /// More than twice `MAX_TOUCH_CONTROLS` descriptors were supplied.
    Capacity,
    /// Descriptor index: missing installed slot, code not emitted by that slot,
    /// empty/over-96-byte/control-character label, or duplicate `(slot, code)`.
    InvalidLabel {
        index: usize,
    },
}
impl fmt::Display for VisibleHintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hint(error) => error.fmt(f),
            Self::Capacity => f.write_str("visible hints exceed 32 labels"),
            Self::InvalidLabel { index } => write!(f, "invalid visible touch label {index}"),
        }
    }
}
impl std::error::Error for VisibleHintError {}

// The suffix identifies analog/relative gestures rather than suggesting that
// they are buttons. Read outputs from the installed effective layout; saved
// mirroring/scale/offset changes geometry, never slot identity or output codes.
fn touch_role(output: touch::Output, code: InputCode) -> Option<&'static str> {
    match (output, code) {
        (touch::Output::Button(a), InputCode::Touch(b)) if a == b => Some(""),
        (touch::Output::Stick { x, .. }, InputCode::Touch(b)) if x == b => Some("stick X"),
        (touch::Output::Stick { y, .. }, InputCode::Touch(b)) if y == b => Some("stick Y"),
        (touch::Output::Look { x, .. }, InputCode::TouchDelta(b)) if x == b => Some("gesture X"),
        (touch::Output::Look { y, .. }, InputCode::TouchDelta(b)) if y == b => Some("gesture Y"),
        _ => None,
    }
}

fn write_visible_hint(
    out: &mut impl Write,
    bindings: &[Binding],
    device: Option<HintDevice>,
    labels: &[VisibleTouchLabel<'_>],
    controls: &[touch::Control],
) -> fmt::Result {
    let mut empty = true;
    for binding in bindings
        .iter()
        .filter(|b| b.scale != 0. && device.is_none_or(|d| d == HintDevice::for_code(b.code)))
    {
        let mut named = false;
        for label in labels.iter().filter(|label| label.code == binding.code) {
            if !empty {
                out.write_str(" / ")?;
            }
            out.write_str(label.label)?;
            let role =
                touch_role(controls[label.slot].output, label.code).expect("validated label");
            if !role.is_empty() {
                write!(out, " ({role})")?;
            }
            if binding.scale != 1. {
                write!(out, " x{}", binding.scale)?;
            }
            named = true;
            empty = false;
        }
        if !named {
            if !empty {
                out.write_str(" / ")?;
            }
            write!(out, "{}", BindingLabel(*binding))?;
            if HintDevice::for_code(binding.code) == HintDevice::Touch {
                out.write_str(if binding.code.is_delta() {
                    " (gesture; not visible)"
                } else {
                    " (not visible)"
                })?;
            }
            empty = false;
        }
    }
    if empty {
        out.write_str("Unbound")?;
    }
    Ok(())
}

fn write_hint(
    out: &mut impl Write,
    bindings: &[Binding],
    device: Option<HintDevice>,
) -> fmt::Result {
    let mut empty = true;
    for binding in bindings
        .iter()
        .filter(|b| b.scale != 0. && device.is_none_or(|d| d == HintDevice::for_code(b.code)))
    {
        if !empty {
            out.write_str(" / ")?;
        }
        write!(out, "{}", BindingLabel(*binding))?;
        empty = false;
    }
    if empty {
        out.write_str("Unbound")?;
    }
    Ok(())
}

impl PlayerInput {
    /// Like [`Self::format_visible_hint_for`], selecting [`Self::current_device`].
    /// Neither hint method samples input or consumes tick edges.
    ///
    /// ```
    /// use ferravex_render::input::{ActionKind, Binding, InputCode, PlayerInput};
    /// let mut input = PlayerInput::transient("visible-current");
    /// input.action("jump", "Jump", ActionKind::Button,
    ///     vec![Binding::digital(InputCode::Key(32))])?;
    /// assert_eq!(input.format_visible_hint("jump", &[], &mut [0; 16])?, "Space");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn format_visible_hint<'a>(
        &self,
        name: &str,
        labels: &[VisibleTouchLabel<'_>],
        buffer: &'a mut [u8],
    ) -> Result<&'a str, VisibleHintError> {
        self.format_visible_hint_for(name, Some(self.current_device()), labels, buffer)
    }

    /// Format current bindings with labels for the touch controls actually drawn
    /// by the caller. A supplied label must name an output of an installed slot;
    /// stale/hidden suffix slots and duplicate `(slot, code)` pairs are errors.
    /// There are at most 32 labels, each 1..=96 UTF-8 bytes without controls.
    /// The caller establishes visibility; geometry alone cannot prove it.
    ///
    /// Omitted touch codes remain explicit `Touch N (not visible)` or
    /// `Touch look N (gesture; not visible)`, rather than `Unbound`. Named stick
    /// axes append `(stick X/Y)` and relative look axes `(gesture X/Y)`.
    /// Multiple visible slots emitting one code are listed in descriptor order.
    /// Binding order, duplicates, scales, device filtering and zero omission
    /// follow [`Self::format_hint_for`]. No cross-device fallback is performed.
    ///
    /// Supply only labels drawn **now**: pass an empty slice when authored labels
    /// are hidden by a custom overlay, or labels matching your resolved custom
    /// drawing. [`Self::touch_customized`] alone does not establish visibility.
    /// Installing new layouts, remapping, capture/import and resetting bindings
    /// are reflected on every read. No labels or borrowed data are retained.
    ///
    /// Formatting allocates nothing and uses two bounded passes over at most
    /// `MAX_BINDINGS * 32` entries, plus bounded descriptor validation. Unknown
    /// actions fail first. All failures preserve the whole byte buffer; capacity
    /// errors report exact UTF-8 bytes, and success preserves the unused tail.
    ///
    /// This headless first-party recipe draws `GAS` in task 278's resolved slot 0
    /// region before advertising that label; it is not a browser playtest.
    /// Hidden slot 1 emits no channel, even if an action is bound to Touch 14.
    ///
    /// ```
    /// use ferravex_render::{input::{ActionKind, Binding, HintDevice, InputCode,
    ///     PlayerInput, VisibleTouchLabel, host_frame, touch::{Output, Region},
    ///     touch_layout::{TouchLayout, TouchSlot, MinimumTouchSize}},
    ///     ui::{UiRect, Viewport}, draw_rectangle, draw_text, set_default_camera,
    ///     Color, WHITE};
    /// let layout = TouchLayout::from_viewport(
    ///     Viewport::responsive([320., 480.], [0.; 4]).unwrap(), &[
    ///         TouchSlot::rect(0, UiRect { min: [240., 400.], size: [64.; 2] },
    ///             Output::Button(6)), TouchSlot::hidden(1),
    ///     ], MinimumTouchSize::Require(44.))?;
    /// let mut input = PlayerInput::transient("visible-recipe");
    /// input.set_touch_layout(layout.controls())?;
    /// let drawing = layout.resolve(input.touch_settings(), [320., 480.])?;
    /// host_frame(0., 320, 480, || {
    ///     set_default_camera();
    ///     let Region::Rect(r) = drawing.controls()[0].region else { unreachable!() };
    ///     draw_rectangle(r.x, r.y, r.w, r.h, Color::new(0.1, 0.3, 0.4, 1.));
    ///     draw_text("GAS", r.x + 8., r.y + 32., 14., WHITE);
    /// });
    /// input.action("gas", "Accelerate", ActionKind::Axis,
    ///     vec![Binding::digital(InputCode::Touch(6))])?;
    /// let labels = [VisibleTouchLabel { slot: 0, code: InputCode::Touch(6), label: "GAS" }];
    /// let mut text = [0; 128];
    /// assert_eq!(input.format_visible_hint_for("gas", Some(HintDevice::Touch),
    ///     &labels, &mut text)?, "GAS");
    /// input.rebind("gas", vec![Binding::digital(InputCode::Touch(14))])?;
    /// assert_eq!(input.format_visible_hint_for("gas", Some(HintDevice::Touch),
    ///     &labels, &mut text)?, "Touch 14 (not visible)");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn format_visible_hint_for<'a>(
        &self,
        name: &str,
        device: Option<HintDevice>,
        labels: &[VisibleTouchLabel<'_>],
        buffer: &'a mut [u8],
    ) -> Result<&'a str, VisibleHintError> {
        let handle =
            self.map
                .action_handle(name)
                .ok_or(VisibleHintError::Hint(HintError::Input(
                    InputError::InvalidHandle,
                )))?;
        let descriptor = self
            .map
            .action_descriptor(handle)
            .map_err(|error| VisibleHintError::Hint(HintError::Input(error)))?;
        if labels.len() > touch::MAX_TOUCH_CONTROLS * 2 {
            return Err(VisibleHintError::Capacity);
        }
        let controls = self.surface.controls();
        for (index, label) in labels.iter().enumerate() {
            if label.label.is_empty()
                || label.label.len() > 96
                || label.label.chars().any(char::is_control)
                || controls
                    .get(label.slot)
                    .is_none_or(|control| touch_role(control.output, label.code).is_none())
                || labels[..index]
                    .iter()
                    .any(|prior| (prior.slot, prior.code) == (label.slot, label.code))
            {
                return Err(VisibleHintError::InvalidLabel { index });
            }
        }
        let mut measure = HintWriter {
            buffer: None,
            len: 0,
        };
        write_visible_hint(&mut measure, descriptor.bindings, device, labels, controls)
            .expect("bounded visible hint length");
        let required = measure.len;
        if buffer.len() < required {
            return Err(VisibleHintError::Hint(HintError::BufferTooSmall {
                required,
            }));
        }
        write_visible_hint(
            &mut HintWriter {
                buffer: Some(&mut buffer[..required]),
                len: 0,
            },
            descriptor.bindings,
            device,
            labels,
            controls,
        )
        .expect("measured visible hint capacity");
        Ok(std::str::from_utf8(&buffer[..required]).expect("UTF-8 visible hint labels"))
    }

    /// Format bindings for [`Self::current_device`] into caller-owned storage,
    /// without allocating, polling the host or consuming edges/motion. There is
    /// no cross-device fallback: a missing binding for this family is `Unbound`.
    /// See [`Self::format_hint_for`] for formatting and capacity rules.
    ///
    /// ```
    /// use ferravex_render::input::{ActionKind, Binding, InputCode, PlayerInput};
    /// let mut input = PlayerInput::transient("jump-hint");
    /// input.action("jump", "Jump", ActionKind::Button,
    ///     vec![Binding::digital(InputCode::Key(32))]).unwrap();
    /// let mut text = [0; 128];
    /// assert_eq!(input.format_hint("jump", &mut text).unwrap(), "Space");
    /// input.rebind("jump", vec![Binding::digital(InputCode::Key(106))]).unwrap();
    /// assert_eq!(input.format_hint("jump", &mut text).unwrap(), "J");
    /// ```
    pub fn format_hint<'a>(&self, name: &str, buffer: &'a mut [u8]) -> Result<&'a str, HintError> {
        self.format_hint_for(name, Some(self.current_device()), buffer)
    }

    /// Format the current validated bindings for one family, or all families
    /// with `None`, in authored order separated by ` / `. Zero-scale bindings
    /// are omitted; an empty selection yields `Unbound`. Other scales append
    /// ` x<scale>` when not one, preserving negative directions/inversions.
    /// Duplicates are retained. Deadzones are not printed. Labels are ASCII;
    /// pad/touch numbers identify channels rather than hardware glyphs/regions.
    ///
    /// Reads [`ActionMap::action_descriptor`] on each call, so rebind, capture,
    /// config import and reset are reflected immediately. A persistent profile
    /// loads on its first sample; before that, hints show registered defaults.
    /// Hints remain inspectable while paused, unfocused or in controls. They do
    /// not claim the action/context is enabled, the pad connected, or a touch
    /// region installed. Use [`Self::gameplay_active`] to gate a gameplay HUD.
    ///
    /// Work is bounded by action lookup and two passes over at most
    /// [`MAX_BINDINGS`] entries; storage is the caller's byte slice. The returned
    /// string borrows only that slice. Unknown names return an input error;
    /// insufficient capacity reports the exact required byte count without
    /// modifying any bytes. Success leaves bytes beyond the string unchanged.
    ///
    /// ```
    /// use ferravex_render::input::{ActionKind, Binding, InputCode, PlayerInput};
    /// let mut input = PlayerInput::transient("all-hints");
    /// input.action("jump", "Jump", ActionKind::Button, vec![
    ///     Binding::digital(InputCode::Key(32)),
    ///     Binding::digital(InputCode::PadButton(0)),
    ///     Binding::digital(InputCode::Touch(3)),
    /// ]).unwrap();
    /// let mut text = [0; 128];
    /// assert_eq!(input.format_hint_for("jump", None, &mut text).unwrap(),
    ///     "Space / Pad 0 / Touch 3");
    /// let device = input.current_device();
    /// assert_eq!(input.format_hint_for("jump", Some(device), &mut text).unwrap(), "Space");
    /// ```
    pub fn format_hint_for<'a>(
        &self,
        name: &str,
        device: Option<HintDevice>,
        buffer: &'a mut [u8],
    ) -> Result<&'a str, HintError> {
        let handle = self
            .map
            .action_handle(name)
            .ok_or(HintError::Input(InputError::InvalidHandle))?;
        let descriptor = self
            .map
            .action_descriptor(handle)
            .map_err(HintError::Input)?;
        let mut measure = HintWriter {
            buffer: None,
            len: 0,
        };
        write_hint(&mut measure, descriptor.bindings, device).expect("bounded hint length");
        let required = measure.len;
        if buffer.len() < required {
            return Err(HintError::BufferTooSmall { required });
        }
        write_hint(
            &mut HintWriter {
                buffer: Some(&mut buffer[..required]),
                len: 0,
            },
            descriptor.bindings,
            device,
        )
        .expect("measured hint capacity");
        Ok(std::str::from_utf8(&buffer[..required]).expect("ASCII hint labels"))
    }
}

fn fit(text: &str, width: f32, size: u16) -> String {
    if measure_text(text, None, size, 1.).width <= width {
        return text.into();
    }
    let mut text = text.to_string();
    while !text.is_empty() && measure_text(&(text.clone() + "..."), None, size, 1.).width > width {
        text.pop();
    }
    text + "..."
}

// Match the immediate renderer's 14px bitmap metrics, retaining a readable 16px
// advance. Use the registered shared wrapper for both pagination and painting.
const EDITOR_LINE: f32 = 16.;
fn editor_lines(text: &str, width: f32) -> crate::ui::text_layout::WrappedLines<'_> {
    crate::ui::text_layout::WrappedText::new(text, width, 14.)
        .expect("bounded editor width")
        .lines()
}
fn editor_copy(text: &str, rect: Rect, first: usize, lines: usize, color: Color) {
    for (line, span) in editor_lines(text, rect.w)
        .skip(first)
        .take(lines)
        .enumerate()
    {
        draw_text(
            span.text,
            rect.x,
            rect.y + 14. + line as f32 * EDITOR_LINE,
            14.,
            color,
        );
    }
}

#[derive(Clone, Copy, Debug)]
struct EditorGeometry {
    panel: Rect,
    narrow: bool,
    body_y: f32,
    footer_y: f32,
    capture_y: f32,
    detail_lines: usize,
    per_page: usize,
}
impl EditorGeometry {
    fn new(w: f32, h: f32) -> Self {
        let width = (w - 24.).clamp(280., 760.);
        let height = (h - 24.).clamp(300., 620.);
        let x = (w - width) * 0.5;
        let y = (h - height) * 0.5;
        let narrow = width < 530.;
        Self {
            panel: Rect::new(x, y, width, height),
            narrow,
            body_y: y + 143.,
            footer_y: y + height - 90.,
            capture_y: y + height - 140.,
            detail_lines: (((height - 287.) / EDITOR_LINE).floor() as usize).max(1),
            per_page: (((height - 245.) / if narrow { 76. } else { 43. }).floor() as usize)
                .clamp(1, 8),
        }
    }
}

struct BindingDetail {
    action: usize,
    first_line: usize,
    text: String,
    pages: usize,
}
impl PlayerInput {
    // One action can span multiple pages, including all 64 bindings. Page is
    // still the existing editor cursor: no save/config/input state is added.
    // At most two bounded action strings coexist; no Vec of all pages is built.
    fn binding_detail(&self, width: f32, lines: usize) -> BindingDetail {
        let mut detail = BindingDetail {
            action: 0,
            first_line: 0,
            text: String::new(),
            pages: 0,
        };
        for (action, info) in self.actions.iter().enumerate() {
            let mut text = format!("Action: {}\nBindings: ", info.label);
            write_hint(
                &mut text,
                self.map.bindings(info.id).expect("registered action"),
                None,
            )
            .expect("String writer");
            let pages = editor_lines(&text, width).count().div_ceil(lines).max(1);
            if self.page >= detail.pages {
                detail.action = action;
                detail.first_line = (self.page - detail.pages).min(pages - 1) * lines;
                detail.text = text;
            }
            detail.pages += pages;
        }
        detail.pages = detail.pages.max(1);
        detail
    }

    fn ui_button(&mut self, rect: Rect, label: &str, selected: bool, index: usize) -> bool {
        let hovered = rect.contains(self.pointer);
        let keyboard = self.ui_focus == index;
        let fill = if selected {
            Color::from_rgba(60, 104, 105, 255)
        } else if hovered || keyboard {
            Color::from_rgba(53, 72, 80, 255)
        } else {
            Color::from_rgba(30, 44, 52, 255)
        };
        draw_rectangle(rect.x, rect.y, rect.w, rect.h, fill);
        if keyboard {
            draw_rectangle_lines(
                rect.x,
                rect.y,
                rect.w,
                rect.h,
                1.,
                Color::from_rgba(141, 196, 187, 255),
            );
        }
        let label = fit(label, (rect.w - 8.).max(0.), 14);
        let width = measure_text(&label, None, 14, 1.).width;
        draw_text(
            &label,
            rect.x + (rect.w - width) * 0.5,
            rect.y + rect.h * 0.5 + 5.,
            14.,
            WHITE,
        );
        !self.ui_block
            && ((self.ui_click && hovered)
                || self.ui_taps[..self.ui_tap_len]
                    .iter()
                    .any(|p| rect.contains(*p))
                || keyboard && self.ui_accept)
    }
    /// Draw after the game's HUD. F2, the browser Controls button or controller
    /// Home opens this editor. Tab/arrows and Enter or the D-pad/A navigate it.
    pub fn draw_settings(&mut self) {
        self.draw_settings_impl(true);
    }

    /// Draw the controls editor and customized touch controls without the
    /// default pause banner. Use this when the game paints its own start,
    /// pause or result screen; input gating remains unchanged.
    ///
    /// Panels narrower than 530 logical pixels show one action's full name and
    /// all nonzero bindings in wrapped 14px text. Long content continues on the
    /// next numbered page; `<`/`>` page through every line without ellipsis or
    /// reduced type. Key/pad capture, Done and Reset remain available on each
    /// page. The minimum panel is 280 by 300 logical pixels (304 by 324 including
    /// its normal outer margins); smaller hosts may clip the panel.
    ///
    /// ```no_run
    /// use ferravex_render::input::{PlayerInput, Preset};
    /// let mut input = PlayerInput::preset("my-game", Preset::TopDown);
    /// input.set_paused(true);
    /// // Paint the game's pause card, then the controls editor above it.
    /// input.draw_controls();
    /// assert!(input.paused());
    /// ```
    pub fn draw_controls(&mut self) {
        self.draw_settings_impl(false);
    }

    fn draw_settings_impl(&mut self, show_pause_hint: bool) {
        if self.external_details {
            return;
        }
        if !self.open {
            if self.touch_seen && self.enabled && !self.paused && self.touch_customized() {
                self.draw_custom_touch();
            }
            if self.paused && show_pause_hint {
                crate::set_default_camera();
                let w = screen_width();
                let h = screen_height();
                let label = "PAUSED";
                let width = measure_text(label, None, 16, 1.).width;
                let badge_width = (w - 24.).clamp(1., 160.);
                draw_rectangle(
                    (w - badge_width) * 0.5,
                    h * 0.5 - 20.,
                    badge_width,
                    40.,
                    Color::new(0.02, 0.04, 0.05, 0.9),
                );
                draw_text(label, (w - width) * 0.5, h * 0.5 + 6., 16., WHITE);
            }
            return;
        }
        crate::set_default_camera();
        crate::ui::warm_up_font_sizes(&[14, 16, 20]);
        let w = screen_width();
        let h = screen_height();
        let geometry = EditorGeometry::new(w, h);
        let Rect {
            x,
            y,
            w: width,
            h: height,
        } = geometry.panel;
        let narrow = geometry.narrow;
        draw_rectangle(0., 0., w, h, Color::new(0.02, 0.04, 0.05, 0.88));
        draw_rectangle(x, y, width, height, Color::from_rgba(18, 29, 37, 255));
        draw_rectangle_lines(x, y, width, height, 1., Color::from_rgba(70, 101, 108, 255));
        draw_text("CONTROLS", x + 18., y + 31., 20., WHITE);
        let per_page = geometry.per_page;
        let count = if self.devices_tab {
            20 + self.raw_touch.len() * 3
        } else {
            self.actions.len()
        };
        let detail = (narrow && !self.devices_tab)
            .then(|| self.binding_detail(width - 36., geometry.detail_lines));
        let pages = detail
            .as_ref()
            .map_or_else(|| count.div_ceil(per_page).max(1), |d| d.pages);
        self.page = self.page.min(pages - 1);
        let start = detail.as_ref().map_or(self.page * per_page, |d| d.action);
        let end = if detail.is_some() {
            (start + 1).min(count)
        } else {
            (start + per_page).min(count)
        };
        let footer = 3 + (end - start) * 2;
        let mut focusable = vec![0, 1, 2];
        for (row, i) in (start..end).enumerate() {
            focusable.push(3 + row * 2);
            if self.devices_tab || self.actions[i].kind != ActionKind::Delta {
                focusable.push(4 + row * 2);
            }
        }
        focusable.extend([footer, footer + 1, footer + 2]);
        let current = focusable
            .iter()
            .position(|&n| n == self.ui_focus)
            .unwrap_or(0);
        self.ui_focus = focusable[if self.ui_down {
            (current + 1) % focusable.len()
        } else if self.ui_up {
            (current + focusable.len() - 1) % focusable.len()
        } else {
            current
        }];
        if self.ui_button(
            Rect::new(x + width - 94., y + 13., 76., 27.),
            "Done",
            false,
            0,
        ) {
            self.set_controls_open(false);
            return;
        }
        if self.ui_button(
            Rect::new(x + 18., y + 47., 92., 28.),
            "Bindings",
            !self.devices_tab,
            1,
        ) {
            self.devices_tab = false;
            self.page = 0;
            self.capture = None;
            return;
        }
        if self.ui_button(
            Rect::new(x + 120., y + 47., 92., 28.),
            "Devices",
            self.devices_tab,
            2,
        ) {
            self.devices_tab = true;
            self.page = 0;
            self.capture = None;
            return;
        }
        let hint = if self.capture.is_some() {
            "Press key/button or move axis right/down. Esc cancels."
        } else {
            "Changes save on this device. F2 / Esc closes."
        };
        editor_copy(
            hint,
            Rect::new(x + 18., y + 87., width - 36., 48.),
            0,
            3,
            Color::from_rgba(161, 185, 189, 255),
        );
        let mut settings = self.settings;
        let mut touch = self.touch_settings;
        let mut preferences = self.preferences();
        if self.devices_tab {
            for (row, i) in (start..end).enumerate() {
                let yy = geometry.body_y + row as f32 * if narrow { 76. } else { 43. };
                let (label, value) = match i {
                    0 => ("Mouse look", format!("{:.2}", settings.mouse_sensitivity)),
                    1 => ("Touch look", format!("{:.4}", settings.touch_sensitivity)),
                    2 => ("Pad look X", format!("{:.2} rad/s", settings.pad_look.x)),
                    3 => ("Pad look Y", format!("{:.2} rad/s", settings.pad_look.y)),
                    4 => ("Dead zone", format!("{:.2}", settings.pad_deadzone)),
                    5 => ("Stick curve", format!("{:.2}", settings.pad_curve)),
                    6 => (
                        "Invert Y",
                        if settings.invert_y {
                            "On".into()
                        } else {
                            "Off".into()
                        },
                    ),
                    7 => (
                        "Controller",
                        format!(
                            "{} {}",
                            settings.gamepad + 1,
                            if gamepads()[settings.gamepad as usize].connected {
                                "connected"
                            } else {
                                "offline"
                            }
                        ),
                    ),
                    8 => (
                        "Master volume",
                        format!("{}%", (preferences.master * 100.).round() as u32),
                    ),
                    9..=12 => (
                        [
                            "Effects volume",
                            "Ambience volume",
                            "Music volume",
                            "UI volume",
                        ][i - 9],
                        format!("{}%", (preferences.buses[i - 9] * 100.).round() as u32),
                    ),
                    13 => (
                        "Sound",
                        if preferences.muted {
                            "Muted".into()
                        } else {
                            "On".into()
                        },
                    ),
                    14 => (
                        "Graphics quality",
                        match preferences.quality {
                            None => "Game default",
                            Some(0) => "Low",
                            Some(1) => "Medium",
                            _ => "High",
                        }
                        .into(),
                    ),
                    15 => (
                        "Visual style",
                        crate::Look::from_index(preferences.look as u32)
                            .unwrap_or_default()
                            .name()
                            .into(),
                    ),
                    16 => (
                        "Touch handedness",
                        if touch.mirrored {
                            "Mirrored"
                        } else {
                            "Authored"
                        }
                        .into(),
                    ),
                    17 => ("Touch scale", format!("{:.0}%", touch.scale * 100.)),
                    18 => ("Touch opacity", format!("{:.0}%", touch.opacity * 100.)),
                    19 => (
                        "Adaptive performance",
                        if preferences.adaptive { "On" } else { "Off" }.into(),
                    ),
                    _ => {
                        let slot = (i - 20) / 3;
                        let axis = (i - 20) % 3;
                        (
                            ["Touch X", "Touch Y", "Touch size"][axis],
                            format!(
                                "{}: {:.0}%",
                                slot + 1,
                                match axis {
                                    0 => touch.edits[slot].offset.x,
                                    1 => touch.edits[slot].offset.y,
                                    _ => touch.edits[slot].scale,
                                } * 100.
                            ),
                        )
                    }
                };
                editor_copy(
                    label,
                    Rect::new(x + 18., yy, width - if narrow { 36. } else { 192. }, 16.),
                    0,
                    1,
                    WHITE,
                );
                editor_copy(
                    &value,
                    Rect::new(x + 18., yy + 18., width - 192., 32.),
                    0,
                    2,
                    Color::from_rgba(161, 185, 189, 255),
                );
                let button_y = yy + if narrow { 32. } else { 0. };
                let minus = self.ui_button(
                    Rect::new(x + width - 158., button_y, 62., 31.),
                    "-",
                    false,
                    3 + row * 2,
                );
                let plus = self.ui_button(
                    Rect::new(x + width - 86., button_y, 62., 31.),
                    "+",
                    false,
                    4 + row * 2,
                );
                if minus || plus {
                    let d = if plus { 1. } else { -1. };
                    match i {
                        0 => {
                            settings.mouse_sensitivity = (settings.mouse_sensitivity
                                * if plus { 1.15 } else { 1. / 1.15 })
                            .clamp(0.01, 20.)
                        }
                        1 => {
                            settings.touch_sensitivity = (settings.touch_sensitivity
                                * if plus { 1.15 } else { 1. / 1.15 })
                            .clamp(0.00005, 0.1)
                        }
                        2 => settings.pad_look.x = (settings.pad_look.x + d * 0.25).clamp(0., 20.),
                        3 => settings.pad_look.y = (settings.pad_look.y + d * 0.25).clamp(0., 20.),
                        4 => {
                            settings.pad_deadzone =
                                (settings.pad_deadzone + d * 0.02).clamp(0., 0.95)
                        }
                        5 => settings.pad_curve = (settings.pad_curve + d * 0.25).clamp(0.25, 4.),
                        6 => settings.invert_y = !settings.invert_y,
                        7 => {
                            settings.gamepad = if plus {
                                (settings.gamepad + 1) % 4
                            } else {
                                (settings.gamepad + 3) % 4
                            }
                        }
                        8 => preferences.master = (preferences.master + d * 0.05).clamp(0., 1.),
                        9..=12 => {
                            preferences.buses[i - 9] =
                                (preferences.buses[i - 9] + d * 0.05).clamp(0., 1.)
                        }
                        13 => preferences.muted = !preferences.muted,
                        14 => {
                            let value = preferences.quality.map_or(0, |q| q + 1);
                            let next = if plus {
                                (value + 1) % 4
                            } else {
                                (value + 3) % 4
                            };
                            preferences.quality = next.checked_sub(1);
                        }
                        15 => preferences.look = (preferences.look + if plus { 1 } else { 3 }) % 4,
                        16 => touch.mirrored = !touch.mirrored,
                        17 => touch.scale = (touch.scale + d * 0.1).clamp(0.6, 1.8),
                        18 => touch.opacity = (touch.opacity + d * 0.1).clamp(0.2, 1.),
                        19 => preferences.adaptive = !preferences.adaptive,
                        _ => {
                            let slot = (i - 20) / 3;
                            match (i - 20) % 3 {
                                0 => {
                                    touch.edits[slot].offset.x =
                                        (touch.edits[slot].offset.x + d * 0.025).clamp(-0.4, 0.4)
                                }
                                1 => {
                                    touch.edits[slot].offset.y =
                                        (touch.edits[slot].offset.y + d * 0.025).clamp(-0.4, 0.4)
                                }
                                _ => {
                                    touch.edits[slot].scale =
                                        (touch.edits[slot].scale + d * 0.1).clamp(0.5, 2.)
                                }
                            }
                        }
                    }
                }
            }
        } else if let Some(detail) = detail.as_ref().filter(|_| start < end) {
            editor_copy(
                &detail.text,
                Rect::new(x + 18., geometry.body_y, width - 36., 0.),
                detail.first_line,
                geometry.detail_lines,
                WHITE,
            );
            let info = &self.actions[detail.action];
            let name = info.name.clone();
            let kind = info.kind;
            if self.ui_button(
                Rect::new(x + 18., geometry.capture_y, 122., 31.),
                "Bind key",
                self.capture.is_some_and(|c| {
                    c.action == detail.action && c.device == CaptureDevice::KeyboardMouse
                }),
                3,
            ) {
                let _ = self.begin_capture(&name, CaptureDevice::KeyboardMouse);
            }
            if kind != ActionKind::Delta
                && self.ui_button(
                    Rect::new(x + width - 140., geometry.capture_y, 122., 31.),
                    "Bind pad",
                    self.capture.is_some_and(|c| {
                        c.action == detail.action && c.device == CaptureDevice::Gamepad
                    }),
                    4,
                )
            {
                let _ = self.begin_capture(&name, CaptureDevice::Gamepad);
            }
        } else {
            let key_x = if narrow {
                x + width - 190.
            } else {
                x + width - 340.
            };
            let pad_x = if narrow {
                x + width - 96.
            } else {
                x + width - 174.
            };
            let bw = if narrow { 88. } else { 156. };
            for (row, i) in (start..end).enumerate() {
                let yy = geometry.body_y + row as f32 * 43.;
                let info = &self.actions[i];
                let name = info.name.clone();
                let kind = info.kind;
                let label = fit(&info.label, key_x - x - 28., if narrow { 14 } else { 16 });
                draw_text(
                    &label,
                    x + 18.,
                    yy + 20.,
                    if narrow { 14. } else { 16. },
                    WHITE,
                );
                let keyboard = self
                    .map
                    .bindings(info.id)
                    .unwrap()
                    .iter()
                    .filter(|b| {
                        matches!(
                            b.code,
                            InputCode::Key(_)
                                | InputCode::Mouse(_)
                                | InputCode::MouseDelta(_)
                                | InputCode::Wheel(_)
                        )
                    })
                    .take(2)
                    .map(|b| BindingLabel(*b).to_string())
                    .collect::<Vec<_>>()
                    .join(" / ");
                let pad = self
                    .map
                    .bindings(info.id)
                    .unwrap()
                    .iter()
                    .find(|b| matches!(b.code, InputCode::PadButton(_) | InputCode::PadAxis(_)))
                    .map_or_else(|| "Bind pad".into(), |b| BindingLabel(*b).to_string());
                if self.ui_button(
                    Rect::new(key_x, yy, bw, 31.),
                    if keyboard.is_empty() {
                        "Bind key"
                    } else {
                        &keyboard
                    },
                    self.capture
                        .is_some_and(|c| c.action == i && c.device == CaptureDevice::KeyboardMouse),
                    3 + row * 2,
                ) {
                    let _ = self.begin_capture(&name, CaptureDevice::KeyboardMouse);
                }
                if kind != ActionKind::Delta
                    && self.ui_button(
                        Rect::new(pad_x, yy, bw, 31.),
                        &pad,
                        self.capture
                            .is_some_and(|c| c.action == i && c.device == CaptureDevice::Gamepad),
                        4 + row * 2,
                    )
                {
                    let _ = self.begin_capture(&name, CaptureDevice::Gamepad);
                }
            }
        }
        if touch != self.touch_settings {
            let _ = self.set_touch_settings(touch);
        }
        if preferences != self.preferences() {
            let _ = self.set_preferences(preferences);
        }
        if settings != self.settings {
            self.set_settings(settings)
                .expect("bounded editor settings");
            self.save();
        }
        let yy = geometry.footer_y;
        if self.ui_button(Rect::new(x + 18., yy, 48., 26.), "<", false, footer) {
            self.page = self.page.saturating_sub(1);
            self.ui_focus = 0;
            self.capture = None;
        }
        // The potentially large page count gets its own full-width line; it
        // cannot collide with navigation/Reset even for maximum-size mappings.
        let page_label = format!("{} / {pages}", self.page + 1);
        draw_text(&page_label, x + 18., yy - 5., 14., WHITE);
        if self.ui_button(Rect::new(x + 76., yy, 48., 26.), ">", false, footer + 1) {
            self.page = (self.page + 1).min(pages - 1);
            self.ui_focus = 0;
            self.capture = None;
        }
        if self.ui_button(
            Rect::new(x + width - 88., yy, 70., 26.),
            "Reset",
            false,
            footer + 2,
        ) {
            self.reset_defaults();
            if self.devices_tab {
                let _ = self.reset_preferences();
                let _ = self.set_touch_settings(touch::TouchLayoutSettings::default());
            }
            self.capture = None;
        }
        let message = if self.devices_tab {
            if self.touch_save_failed
                || self.preferences_error().is_some()
                || matches!(
                    self.status,
                    ControlsStatus::Storage(_) | ControlsStatus::Invalid(_)
                )
            {
                "Settings not saved; changes work for this session"
            } else {
                "Device, audio and graphics settings apply immediately"
            }
        } else {
            match self.status {
                ControlsStatus::Defaults => "Default bindings",
                ControlsStatus::Transient => "Session controls; persistence disabled",
                ControlsStatus::Saved => "Saved on this device",
                ControlsStatus::Loaded { unknown: 0 } => "Loaded saved controls",
                ControlsStatus::Loaded { .. } => "Loaded controls; retired actions ignored",
                ControlsStatus::Invalid(_) => "Invalid file; defaults remain available",
                ControlsStatus::Storage(_) => "Storage unavailable; session changes only",
            }
        };
        editor_copy(
            message,
            Rect::new(x + 18., y + height - 56., width - 36., 48.),
            0,
            3,
            Color::from_rgba(161, 185, 189, 255),
        );
    }
}

impl PlayerInput {
    /// This draws the effective capture regions, so custom hit targets and labels
    /// use exactly the same mirrored/scaled coordinates as input.
    fn draw_custom_touch(&self) {
        crate::set_default_camera();
        let color = Color::new(0.9, 0.94, 0.86, self.touch_settings.opacity);
        for c in self.surface.controls() {
            if matches!(c.output, touch::Output::Look { .. }) {
                continue;
            }
            let center = match c.region {
                touch::Region::Circle { center, radius } => {
                    draw_circle(
                        center.x,
                        center.y,
                        radius,
                        Color::new(0.04, 0.08, 0.09, self.touch_settings.opacity * 0.5),
                    );
                    draw_circle_lines(center.x, center.y, radius, 2., color);
                    center
                }
                touch::Region::Rect(r) => {
                    draw_rectangle(
                        r.x,
                        r.y,
                        r.w,
                        r.h,
                        Color::new(0.04, 0.08, 0.09, self.touch_settings.opacity * 0.5),
                    );
                    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2., color);
                    r.center()
                }
            };
            match c.output {
                touch::Output::Stick { x, y, radius, .. } => {
                    let (origin, delta) = self.surface.stick(x, y).unwrap_or((center, Vec2::ZERO));
                    let p = origin + delta * radius * 0.6;
                    draw_circle(p.x, p.y, 12., color);
                }
                touch::Output::Button(channel) => {
                    let label = self
                        .actions
                        .iter()
                        .find(|a| {
                            self.map
                                .bindings(a.id)
                                .unwrap()
                                .iter()
                                .any(|b| b.code == InputCode::Touch(channel))
                        })
                        .map_or("Action", |a| a.label.as_str());
                    let width = measure_text(label, None, 14, 1.).width;
                    draw_text(label, center.x - width * 0.5, center.y + 5., 14., color);
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod journey400_292 {
    use super::*;

    fn reset(w: u32, h: u32) {
        crate::edit(|s| {
            *s = crate::State::default();
            s.w = w;
            s.h = h;
            s.default_camera();
        });
    }
    fn vertices_in(
        vertices: &[[f32; 8]],
        rect: Rect,
        color: Color,
        screen: [f32; 2],
    ) -> Vec<[f32; 8]> {
        vertices
            .iter()
            .copied()
            .filter(|v| {
                let p = vec2((v[0] + 1.) * screen[0] * 0.5, (1. - v[1]) * screen[1] * 0.5);
                p.x >= rect.x - 0.001
                    && p.x <= rect.x + rect.w + 0.001
                    && p.y >= rect.y - 0.001
                    && p.y <= rect.y + rect.h + 0.001
                    && v[4..] == [color.r, color.g, color.b, color.a]
            })
            .collect()
    }
    fn assert_full_glyphs(
        actual: &[[f32; 8]],
        rect: Rect,
        lines: &[String],
        color: Color,
        screen: [f32; 2],
    ) {
        crate::edit(|s| s.vertices.clear());
        for (i, line) in lines.iter().enumerate() {
            // Independently paint the complete expected labels through the real
            // glyph renderer, then compare every submitted vertex in the row.
            draw_text(line, rect.x, rect.y + 14. + i as f32 * 16., 14., color);
        }
        let expected = crate::read(|s| s.vertices.clone());
        assert!(!expected.is_empty());
        assert_eq!(vertices_in(actual, rect, color, screen), expected);
    }

    #[test]
    fn full_default_long_unbound_and_maximum_binding_glyphs_are_paged_at_minimum_size() {
        for [w, h] in [[320, 480], [320, 324], [304, 324]] {
            reset(w, h);
            let mut input = PlayerInput::preset("journey400-292-glyphs", Preset::TopDown);
            input
                .action(
                    "long",
                    &"LONG ACTION ".repeat(8),
                    ActionKind::Axis,
                    (0..MAX_BINDINGS)
                        .map(|n| Binding {
                            code: InputCode::Key(0xff55 + (n % 2) as u16),
                            scale: if n % 2 == 0 { -f32::MAX } else { 0.5 },
                            deadzone: 0.,
                        })
                        .collect(),
                )
                .unwrap();
            input
                .action(
                    "empty",
                    "Unbound action with a complete readable name",
                    ActionKind::Button,
                    vec![],
                )
                .unwrap();
            let before = input.export_config().unwrap();
            input.set_controls_open(true);
            let g = EditorGeometry::new(w as f32, h as f32);
            let width = g.panel.w - 36.;
            let screen = [w as f32, h as f32];
            let mut expected_pages = Vec::new();
            for info in &input.actions {
                let mut buffer = vec![0; 8192];
                let bindings = input
                    .format_hint_for(&info.name, None, &mut buffer)
                    .unwrap();
                let complete = format!("Action: {}\nBindings: {bindings}", info.label);
                let lines: Vec<_> = crate::ui::text_layout::WrappedText::new(&complete, width, 14.)
                    .unwrap()
                    .lines()
                    .map(|line| line.text.to_owned())
                    .collect();
                for chunk in lines.chunks(g.detail_lines) {
                    expected_pages.push(chunk.to_vec());
                }
            }
            assert!(expected_pages.len() > input.actions.len());
            for (page, lines) in expected_pages.iter().enumerate() {
                input.page = page;
                crate::edit(|s| s.vertices.clear());
                input.draw_controls();
                let actual = crate::read(|s| s.vertices.clone());
                assert_eq!(input.page, page);
                assert!(g.body_y + (g.detail_lines - 1) as f32 * 16. + 14. < g.capture_y);
                assert!(g.capture_y + 31. < g.footer_y - 5. - 12.25);
                assert!(g.footer_y + 26. < g.panel.y + g.panel.h - 56.);
                assert_full_glyphs(
                    &actual,
                    Rect::new(
                        g.panel.x + 18.,
                        g.body_y,
                        width,
                        g.detail_lines as f32 * 16.,
                    ),
                    lines,
                    WHITE,
                    screen,
                );
                // Save/close copy is complete, including its final line.
                let help: Vec<_> = crate::ui::text_layout::WrappedText::new(
                    "Changes save on this device. F2 / Esc closes.",
                    width,
                    14.,
                )
                .unwrap()
                .lines()
                .map(|line| line.text.to_owned())
                .collect();
                assert!(help.len() <= 3);
                assert_full_glyphs(
                    &actual,
                    Rect::new(g.panel.x + 18., g.panel.y + 87., width, 48.),
                    &help,
                    Color::from_rgba(161, 185, 189, 255),
                    screen,
                );
                for v in actual {
                    let p = [(v[0] + 1.) * screen[0] * 0.5, (1. - v[1]) * screen[1] * 0.5];
                    assert!(p[0] >= -0.001 && p[0] <= screen[0] + 0.001, "{p:?}");
                    assert!(p[1] >= -0.001 && p[1] <= screen[1] + 0.001, "{p:?}");
                }
            }
            assert_eq!(
                input.export_config().unwrap(),
                before,
                "drawing preserves exact save bytes"
            );
        }
    }

    fn frame(input: &mut PlayerInput) {
        host_frame(0., 320, 480, || {
            input.sample(true);
            input.draw_controls();
        });
    }
    fn click(input: &mut PlayerInput, position: Vec2) {
        crate::pointer(position.x, position.y, 0., 0.);
        crate::button(1, true);
        frame(input);
        crate::button(1, false);
        frame(input);
    }
    fn tap(input: &mut PlayerInput, key: u32) {
        crate::key(key, true);
        frame(input);
        crate::key(key, false);
        frame(input);
    }

    #[test]
    fn minimum_panel_keeps_capture_help_status_navigation_and_delta_focus_readable() {
        reset(320, 324);
        let mut input = PlayerInput::transient("journey400-292-minimum");
        input
            .action(
                "look",
                "Look",
                ActionKind::Delta,
                vec![Binding::digital(InputCode::MouseDelta(0))],
            )
            .unwrap();
        input.set_controls_open(true);
        input
            .begin_capture("look", CaptureDevice::KeyboardMouse)
            .unwrap();
        input.draw_controls();
        let actual = crate::read(|s| s.vertices.clone());
        let g = EditorGeometry::new(320., 324.);
        let width = g.panel.w - 36.;
        let muted = Color::from_rgba(161, 185, 189, 255);
        for (text, top) in [
            (
                "Press key/button or move axis right/down. Esc cancels.",
                g.panel.y + 87.,
            ),
            (
                "Session controls; persistence disabled",
                g.panel.y + g.panel.h - 56.,
            ),
        ] {
            let lines: Vec<_> = crate::ui::text_layout::WrappedText::new(text, width, 14.)
                .unwrap()
                .lines()
                .map(|line| line.text.to_owned())
                .collect();
            assert!(lines.len() <= 3);
            assert_full_glyphs(
                &actual,
                Rect::new(g.panel.x + 18., top, width, 48.),
                &lines,
                muted,
                [320., 324.],
            );
        }
        for (label, rect) in [
            (
                "Done",
                Rect::new(g.panel.x + g.panel.w - 94., g.panel.y + 13., 76., 27.),
            ),
            (
                "Bind key",
                Rect::new(g.panel.x + 18., g.capture_y, 122., 31.),
            ),
            ("<", Rect::new(g.panel.x + 18., g.footer_y, 48., 26.)),
            (">", Rect::new(g.panel.x + 76., g.footer_y, 48., 26.)),
            (
                "Reset",
                Rect::new(g.panel.x + g.panel.w - 88., g.footer_y, 70., 26.),
            ),
        ] {
            crate::edit(|s| s.vertices.clear());
            let text_width = measure_text(label, None, 14, 1.).width;
            draw_text(
                label,
                rect.x + (rect.w - text_width) * 0.5,
                rect.y + rect.h * 0.5 + 5.,
                14.,
                WHITE,
            );
            let expected = crate::read(|s| s.vertices.clone());
            assert_eq!(
                vertices_in(&actual, rect, WHITE, [320., 324.]),
                expected,
                "{label}"
            );
        }
        // Cancel through sample, then navigate using the actual keyboard journal.
        // Delta actions have no gamepad capture target, so focus skips index 4.
        crate::key(0xff1b, true);
        input.sample(true);
        assert!(input.capture.is_none());
        crate::key(0xff1b, false);
        host_frame(0., 320, 324, || {
            input.sample(true);
            input.draw_controls();
        });
        for expected in [1, 2, 3, 5, 6, 7, 0] {
            crate::key(0xff09, true);
            host_frame(0., 320, 324, || {
                input.sample(true);
                input.draw_controls();
            });
            assert_eq!(input.ui_focus, expected);
            crate::key(0xff09, false);
            host_frame(0., 320, 324, || {
                input.sample(true);
                input.draw_controls();
            });
        }
    }

    #[test]
    fn journal_ticks_editor_capture_cancel_reset_paging_tabs_and_done_use_production_paths() {
        reset(320, 480);
        let mut input = PlayerInput::transient("journey400-292-journal");
        input
            .action(
                "jump",
                "Jump with a long default action label",
                ActionKind::Button,
                vec![
                    Binding::digital(InputCode::Key(32)),
                    Binding::digital(InputCode::PadButton(0)),
                    Binding::digital(InputCode::Touch(6)),
                ],
            )
            .unwrap();
        input
            .action(
                "look",
                "Look",
                ActionKind::Delta,
                vec![Binding::digital(InputCode::MouseDelta(0))],
            )
            .unwrap();
        input
            .set_touch_layout(&[touch::Control {
                region: touch::Region::Rect(Rect::new(10., 300., 80., 80.)),
                output: touch::Output::Button(6),
            }])
            .unwrap();
        let labels = [VisibleTouchLabel {
            slot: 0,
            code: InputCode::Touch(6),
            label: "JUMP",
        }];
        let original = input.export_config().unwrap();
        frame(&mut input);
        crate::key(32, true);
        crate::key(32, false);
        crate::pointer(100., 100., 9., 0.);
        frame(&mut input); // No executed gameplay tick yet.
        let mut buffer = [0; 128];
        for _ in 0..3 {
            assert_eq!(
                input
                    .format_visible_hint_for("jump", None, &labels, &mut buffer)
                    .unwrap(),
                "Space / Pad 0 / JUMP"
            );
            frame(&mut input);
        }
        let [jump, look] = input.take_tick(["jump", "look"]);
        assert!(jump.pressed && jump.released);
        assert_ne!(look.value, 0.);
        assert_eq!(input.take_tick(["jump", "look"]), [Action::default(); 2]);
        // A touch canceled before any tick releases its active capture.
        for phase in [touch::Phase::Start, touch::Phase::Cancel] {
            touch_event(touch::Event {
                id: 292,
                position: vec2(40., 330.),
                phase,
            });
            frame(&mut input);
            input
                .format_visible_hint("jump", &labels, &mut buffer)
                .unwrap();
        }
        assert!(!input.touch_active());
        input.take_tick(["jump"]);
        assert_eq!(input.take_tick(["jump"]), [Action::default()]);
        tap(&mut input, 0xffbf); // F2 opens via the real ordered journal.
        assert!(input.controls_open());
        let g = EditorGeometry::new(320., 480.);
        let key_button = vec2(g.panel.x + 70., g.capture_y + 15.);
        click(&mut input, key_button);
        assert!(input.capture.is_some());
        tap(&mut input, 0xff1b);
        assert!(input.controls_open());
        assert!(input.capture.is_none());
        assert_eq!(input.export_config().unwrap(), original);
        click(&mut input, key_button);
        tap(&mut input, 0xff56); // Real PageDown identity, not the displayed text.
        assert!(input.capture.is_none());
        assert_eq!(
            input.format_hint_for("jump", None, &mut buffer).unwrap(),
            "Pad 0 / Touch 6 / PageDown"
        );
        assert_eq!(
            input
                .format_visible_hint_for("jump", None, &labels, &mut buffer)
                .unwrap(),
            "Pad 0 / JUMP / PageDown"
        );
        let edited = input.export_config().unwrap();
        input.rebind("jump", vec![]).unwrap();
        input.import_config(&edited).unwrap();
        assert_eq!(input.export_config().unwrap(), edited);
        assert!(
            !input.take_tick(["jump"])[0].pressed,
            "capture/menu events cannot leak to gameplay"
        );
        click(&mut input, vec2(g.panel.x + 90., g.footer_y + 13.));
        assert_eq!(input.page, 1);
        click(&mut input, vec2(g.panel.x + 40., g.footer_y + 13.));
        assert_eq!(input.page, 0);
        click(
            &mut input,
            vec2(g.panel.x + g.panel.w - 50., g.footer_y + 13.),
        );
        assert_eq!(input.export_config().unwrap(), original);
        // A previously large page cursor must not index the wrong tab.
        input.page = usize::MAX;
        click(&mut input, vec2(g.panel.x + 160., g.panel.y + 60.));
        assert!(input.devices_tab);
        assert_eq!(input.page, 0);
        click(&mut input, vec2(g.panel.x + 60., g.panel.y + 60.));
        assert!(!input.devices_tab);
        click(
            &mut input,
            vec2(g.panel.x + g.panel.w - 50., g.panel.y + 26.),
        );
        assert!(!input.controls_open());
        frame(&mut input);
        assert_eq!(input.take_tick(["jump", "look"]), [Action::default(); 2]);
    }
}
