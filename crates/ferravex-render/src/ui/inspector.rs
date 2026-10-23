//! Checked, borrowed authoring controls in logical screen units.
//!
//! Build from a fixed model snapshot. Numeric formatting/units and domain rules
//! belong to the caller. No model reference is stored or mutated: only an
//! [`InspectorOutcome::Accepted`] value may be applied, outside UI composition.
//! Validation callbacks must be pure; arbitrary callback side effects cannot be
//! rolled back. An invalid edit returns its error without changing the snapshot.
//! Feed the error back as field copy next frame (for example `FieldState::Invalid`),
//! or in a separate notice. Disabled, unavailable and invalid fields have distinct
//! visible prefixes and never propose changes.
//!
//! [`InspectorLayout::finish`] atomically submits all commands through
//! [`Ui::compose`], then finishes input and validates at most one fresh activation.
//! It deliberately exposes no speculative result from inside a composition.
//! Call once per frame after `Ui::begin`, after other UI submissions. Use
//! [`InspectorLayout::finish_player`] for the existing remapped confirm route;
//! do not separately finish/route that edge through another UI owner. On submit
//! failure input is not finished: finish the restored prior UI or an explicit
//! readable fallback yourself. The inspector is an overlay, not a modal; panels
//! are paint-only. Route scene gestures using [`Ui::interaction`] after finish.
//!
//! All labels, values, units, status messages and controls are measured before
//! insertion. At most 16 fields, eight choices per field and 8192 source bytes.
//! Rows wrap without shrinking fonts; every control stays at least 44 units high.
//! Too narrow/short bounds reject the entire layout with `InvalidLayout`: the
//! caller must choose a larger panel, fewer fields or a separate readable view.
//! There is no silent omission, implicit scroll or text-entry framework. Use
//! bounds fully inside the viewport safe area and inherited clips; later ancestor
//! clipping can still obscure content. Inspection exposes actual button rectangles
//! and complete labels. Theme tokens map directly to ordinary text/button styles;
//! retained button focus outlines keep their existing two-unit stroke.
//!
//! This runnable native recipe uses the real ordered touch journal, a fixed
//! snapshot, pure domain validation and application after finish. It is synthetic
//! native input, not a window/device playtest. Physics lab adoption and complete
//! mass/gravity/friction validator recipes are handed off in task285's receipt.
//!
//! ```
//! use ferravex_play::ui::theme::{ThemeMode, UiTheme};
//! use ferravex_render::{Vec2, input::{host_frame, touch, touch_event},
//!     ui::{Ui, UiError, Viewport, inspector::*}};
//! let mut model = 1.0_f32;
//! let mut ui = Ui::new();
//! let viewport = Viewport::responsive([320., 480.], [0.; 4])?;
//! let fields = [InspectorField {
//!     id: 1, label: "MASS", display: "1.00", unit: "kg", state: FieldState::Enabled,
//!     control: InspectorControl::Stepper { value: model, min: 0.1, max: 100.,
//!         step: 0.25, policy: LimitPolicy::Reject, decrease: 2, increase: 3 },
//! }];
//! let layout = InspectorLayout::new(viewport.bounds(), &fields,
//!     UiTheme::preset(ThemeMode::Normal))?;
//! let rect = layout.control_rect(3).unwrap();
//! for phase in [touch::Phase::Start, touch::Phase::End] {
//!     touch_event(touch::Event { id: 7, phase,
//!         position: Vec2::new(rect.min[0] + 2., rect.min[1] + 2.) });
//! }
//! host_frame(0., 320, 480, || -> Result<(), UiError> {
//!     ui.begin(viewport);
//!     let outcome = layout.finish(&mut ui, true, |change| {
//!         // Substitute the existing physics/body validator, using a candidate copy.
//!         match change.value {
//!             FieldValue::Number(n) if n <= 10. => Ok(()),
//!             _ => Err("mass exceeds this tool's authored limit"),
//!         }
//!     })?;
//!     assert_eq!(model, 1.); // Nothing mutated during composition or validation.
//!     if let InspectorOutcome::Accepted(change) = outcome {
//!         if let FieldValue::Number(n) = change.value { model = n; }
//!     }
//!     assert!(ui.interaction().pointer_consumed);
//!     ui.paint();
//!     Ok(())
//! })?;
//! assert_eq!(model, 1.25);
//! # Ok::<(), UiError>(())
//! ```

use super::text_layout::WrappedText;
use super::{ButtonStyle, TextStyle, Ui, UiComposition, UiError, UiRect, WidgetId};
use ferravex_play::ui::theme::{ColorRole, UiTheme};
use std::fmt::{self, Write};

/// Hard source bounds, checked before scanning field copy or choices.
pub const MAX_INSPECTOR_FIELDS: usize = 16;
/// Maximum options in one choice field.
pub const MAX_INSPECTOR_CHOICES: usize = 8;
/// Combined UTF-8 bytes, including disabled and unselected copy.
pub const MAX_INSPECTOR_BYTES: usize = 8192;
const MAX_CONTROLS: usize = MAX_INSPECTOR_FIELDS * MAX_INSPECTOR_CHOICES;
const EMPTY: UiRect = UiRect {
    min: [0.; 2],
    size: [0.; 2],
};

/// An explicit authored policy; overflow/nonfinite arithmetic always rejects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LimitPolicy {
    /// Reject an out-of-range finite candidate, without silently clamping.
    Reject,
    /// Clamp a finite candidate to the authored limits; unchanged endpoints are idle.
    Clamp,
}

/// Availability and complete contextual message. Only `Enabled` permits edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldState<'a> {
    Enabled,
    Disabled(&'a str),
    Unavailable(&'a str),
    Invalid(&'a str),
}
impl<'a> FieldState<'a> {
    fn message(self) -> (&'static str, &'a str) {
        match self {
            Self::Enabled => ("", ""),
            Self::Disabled(s) => ("X DISABLED: ", s),
            Self::Unavailable(s) => ("X UNAVAILABLE: ", s),
            Self::Invalid(s) => ("! INVALID: ", s),
        }
    }
}

/// Stable ID and complete single-line choice copy; disabled options stay visible.
#[derive(Clone, Copy, Debug)]
pub struct InspectorChoice<'a> {
    pub id: WidgetId,
    pub label: &'a str,
    pub enabled: bool,
}

/// Borrowed control snapshot. Field and control IDs must all be distinct.
///
/// Toggle/choice composition, remapped finish, geometry queries and explicit
/// close/reset, using the same borrowed snapshot contract as numeric fields:
///
/// ```
/// use ferravex_play::ui::theme::{ThemeMode, UiTheme};
/// use ferravex_render::{input::{host_frame, PlayerInput},
///     ui::{Ui, UiError, Viewport, inspector::*}};
/// let choices = [InspectorChoice { id: 12, label: "BALL", enabled: true },
///                InspectorChoice { id: 13, label: "CRATE", enabled: false }];
/// let fields = [
///     InspectorField { id: 1, label: "GRAVITY", display: "ON", unit: "",
///         state: FieldState::Enabled,
///         control: InspectorControl::Toggle { value: true, id: 2, on: "ON", off: "OFF" } },
///     InspectorField { id: 10, label: "SHAPE", display: "BALL", unit: "",
///         state: FieldState::Enabled,
///         control: InspectorControl::Choice { selected: 0, choices: &choices } },
/// ];
/// let viewport = Viewport::responsive([320., 600.], [0.; 4])?;
/// let layout = InspectorLayout::new(viewport.bounds(), &fields,
///     UiTheme::preset(ThemeMode::HighContrast))?;
/// assert_eq!(layout.bounds(), viewport.bounds());
/// assert!(layout.field_rect(10).is_some() && layout.control_rect(13).is_some());
/// let mut ui = Ui::new();
/// let mut player = PlayerInput::transient("inspector-recipe");
/// host_frame(0., 320, 600, || -> Result<(), UiError> {
///     player.sample(true);
///     ui.begin(viewport);
///     let result = layout.finish_player(&mut ui, &mut player, true,
///         |_| Ok::<(), &'static str>(()))?;
///     assert_eq!(result, InspectorOutcome::Idle);
///     ui.paint();
///     Ok(())
/// })?;
/// ui.cancel(); // Closing/reset cancels a live capture; next begin removes widgets.
/// ui.begin(viewport);
/// ui.finish(false); // Keep routing terminal gestures even with the tool closed.
/// # Ok::<(), UiError>(())
/// ```
#[derive(Clone, Copy, Debug)]
pub enum InspectorControl<'a> {
    /// Finite limits, min <= max, finite step > 0. Enabled values must be finite
    /// and within limits. Stepping uses f32 arithmetic; overflow and a rounded
    /// unchanged sum reject, even under Clamp. No held-key repeat is synthesized.
    Stepper {
        value: f32,
        min: f32,
        max: f32,
        step: f32,
        policy: LimitPolicy,
        decrease: WidgetId,
        increase: WidgetId,
    },
    /// The current label is prefixed `[X]` or `[ ]`; both labels reserve width.
    Toggle {
        value: bool,
        id: WidgetId,
        on: &'a str,
        off: &'a str,
    },
    /// A selected index into 1..=8 choices. Each choice has its own real button;
    /// `[X]` indicates selection without relying on color. Selecting it is idle.
    Choice {
        selected: usize,
        choices: &'a [InspectorChoice<'a>],
    },
}

/// A fixed snapshot with caller-formatted value/units. Display must be nonblank;
/// it is never parsed. Units may be empty. Multi-line field/status copy wraps.
#[derive(Clone, Copy, Debug)]
pub struct InspectorField<'a> {
    pub id: WidgetId,
    pub label: &'a str,
    pub display: &'a str,
    pub unit: &'a str,
    pub state: FieldState<'a>,
    pub control: InspectorControl<'a>,
}

/// Typed proposal values, with no references to a live model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FieldValue {
    Number(f32),
    Toggle(bool),
    Choice(usize),
}
/// Previous snapshot value and validated proposed value, keyed by caller field ID.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ValueChange {
    pub field: WidgetId,
    pub previous: FieldValue,
    pub value: FieldValue,
}
/// Numeric rejection is separate from caller-owned domain validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidEdit<E> {
    NumericLimit,
    NoNumericProgress,
    Validation(E),
}
/// One finished activation. Apply Accepted once, then rebuild the snapshot.
/// Idle includes disabled input, cancellation, a held gesture and unchanged choice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InspectorOutcome<E> {
    Idle,
    Accepted(ValueChange),
    Invalid {
        field: WidgetId,
        reason: InvalidEdit<E>,
    },
}

#[derive(Clone, Copy, Debug)]
struct Row {
    bounds: UiRect,
    label: UiRect,
    value: UiRect,
    message: UiRect,
}
const EMPTY_ROW: Row = Row {
    bounds: EMPTY,
    label: EMPTY,
    value: EMPTY,
    message: EMPTY,
};
#[derive(Clone, Copy, Debug)]
struct Control {
    id: WidgetId,
    rect: UiRect,
    field: usize,
    action: usize,
}
const EMPTY_CONTROL: Control = Control {
    id: 0,
    rect: EMPTY,
    field: 0,
    action: 0,
};

/// Immutable measured layout. No heap storage, model/controller or hidden reset
/// state. Rebuild after any model/copy/theme/bounds change; `Ui::begin` clears old
/// submissions, and `Ui::cancel` cancels capture when the tool closes/resets.
#[derive(Debug)]
pub struct InspectorLayout<'a> {
    bounds: UiRect,
    fields: &'a [InspectorField<'a>],
    theme: UiTheme,
    rows: [Row; MAX_INSPECTOR_FIELDS],
    controls: [Control; MAX_CONTROLS],
    count: usize,
}

impl<'a> InspectorLayout<'a> {
    /// Validate bounded source and measure complete rows. Empty fields are valid.
    /// Duplicate IDs return DuplicateId; source limits return Capacity; invalid
    /// numeric specs, blank essential copy and impossible geometry return InvalidLayout.
    pub fn new(
        bounds: UiRect,
        fields: &'a [InspectorField<'a>],
        theme: UiTheme,
    ) -> Result<Self, UiError> {
        validate_source(fields)?;
        checked_rect(bounds.min[0], bounds.min[1], bounds.size[0], bounds.size[1])?;
        let mut result = Self {
            bounds,
            fields,
            theme,
            rows: [EMPTY_ROW; MAX_INSPECTOR_FIELDS],
            controls: [EMPTY_CONTROL; MAX_CONTROLS],
            count: 0,
        };
        let spacing = theme.spacing();
        let inner = bounds.inset([spacing.inset; 4])?;
        checked_rect(inner.min[0], inner.min[1], inner.size[0], inner.size[1])?;
        let mut y = inner.min[1];
        let mut copy = CopyBuffer::new();
        for (index, field) in fields.iter().enumerate() {
            let top = y;
            let label = text_rect(inner, y, field.label, theme.types().label)?;
            y = label.min[1] + label.size[1] + spacing.compact_gap;
            value_copy(&mut copy, field)?;
            let value = text_rect(inner, y, copy.as_str(), theme.types().body)?;
            y = value.min[1] + value.size[1] + spacing.compact_gap;
            let controls_top = y;
            let mut x = inner.min[0];
            let mut line_y = y;
            for action in 0..control_count(field.control) {
                control_copy(&mut copy, field.control, action)?;
                let mut width = button_width(copy.as_str(), theme.types().body)?;
                // Toggle reserves its larger state too, so toggling does not shift hits.
                if let InspectorControl::Toggle { value, .. } = field.control {
                    control_copy(&mut copy, toggle_value(field.control, !value), 0)?;
                    width = width.max(button_width(copy.as_str(), theme.types().body)?);
                }
                if width > inner.size[0] {
                    return Err(UiError::InvalidLayout);
                }
                if x + width > inner.min[0] + inner.size[0] {
                    x = inner.min[0];
                    line_y += spacing.control_height + spacing.compact_gap;
                }
                let rect = checked_rect(x, line_y, width, spacing.control_height)?;
                contained(inner, rect)?;
                let measured =
                    WrappedText::new(copy.as_str(), rect.size[0] - 16., theme.types().body)?
                        .measure();
                if measured.line_count != 1 || measured.width > rect.size[0] - 16. {
                    return Err(UiError::InvalidLayout);
                }
                result.controls[result.count] = Control {
                    id: control_id(field.control, action),
                    rect,
                    field: index,
                    action,
                };
                result.count += 1;
                x = rect.min[0] + rect.size[0] + spacing.compact_gap;
            }
            y = line_y + spacing.control_height;
            debug_assert!(y > controls_top);
            let message = if field.state == FieldState::Enabled {
                EMPTY
            } else {
                y += spacing.compact_gap;
                status_copy(&mut copy, field.state)?;
                let r = text_rect(inner, y, copy.as_str(), theme.types().body)?;
                y = r.min[1] + r.size[1];
                r
            };
            let row = checked_rect(inner.min[0], top, inner.size[0], y - top)?;
            contained(inner, row)?;
            result.rows[index] = Row {
                bounds: row,
                label,
                value,
                message,
            };
            y += spacing.gap;
        }
        Ok(result)
    }

    pub fn bounds(&self) -> UiRect {
        self.bounds
    }
    /// Complete row including its label/value/status/control rows.
    pub fn field_rect(&self, id: WidgetId) -> Option<UiRect> {
        self.fields
            .iter()
            .position(|f| f.id == id)
            .map(|i| self.rows[i].bounds)
    }
    /// Exact rectangle submitted to the existing button painter/hit registry.
    pub fn control_rect(&self, id: WidgetId) -> Option<UiRect> {
        self.controls[..self.count]
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.rect)
    }

    /// Submit atomically, finish built-in input, then run a pure validator once
    /// for an actual change. All UI submission errors occur before validation.
    pub fn finish<E>(
        &self,
        ui: &mut Ui,
        enabled: bool,
        validate: impl FnOnce(ValueChange) -> Result<(), E>,
    ) -> Result<InspectorOutcome<E>, UiError> {
        ui.compose(|group| self.submit(group))?;
        ui.finish(enabled);
        Ok(self.resolve(ui, validate))
    }

    /// Same transaction/result contract, using existing remapped confirm and
    /// selected-pad handling. Unrelated gameplay edges remain caller/tick-owned.
    pub fn finish_player<E>(
        &self,
        ui: &mut Ui,
        player: &mut crate::input::PlayerInput,
        enabled: bool,
        validate: impl FnOnce(ValueChange) -> Result<(), E>,
    ) -> Result<InspectorOutcome<E>, UiError> {
        ui.compose(|group| self.submit(group))?;
        ui.finish_player(player, enabled);
        Ok(self.resolve(ui, validate))
    }

    fn submit(&self, group: &mut UiComposition<'_>) -> Result<(), UiError> {
        let theme = self.theme;
        group.push_clip(self.bounds)?;
        group.panel(self.bounds, theme.color(ColorRole::Surface))?;
        let mut copy = CopyBuffer::new();
        for (field, row) in self.fields.iter().zip(self.rows.iter()) {
            group.text(
                row.label,
                field.label,
                text_style(theme, theme.types().label, ColorRole::MutedText),
            )?;
            value_copy(&mut copy, field)?;
            group.text(
                row.value,
                copy.as_str(),
                text_style(theme, theme.types().body, ColorRole::Text),
            )?;
            if field.state != FieldState::Enabled {
                status_copy(&mut copy, field.state)?;
                let role = if matches!(field.state, FieldState::Invalid(_)) {
                    ColorRole::Danger
                } else {
                    ColorRole::DisabledText
                };
                group.text(
                    row.message,
                    copy.as_str(),
                    text_style(theme, theme.types().body, role),
                )?;
            }
        }
        for c in &self.controls[..self.count] {
            let field = &self.fields[c.field];
            let enabled = field.state == FieldState::Enabled
                && match field.control {
                    InspectorControl::Choice { choices, .. } => choices[c.action].enabled,
                    _ => true,
                };
            control_copy(&mut copy, field.control, c.action)?;
            group.button(
                c.id,
                c.rect,
                copy.as_str(),
                enabled,
                ButtonStyle {
                    fill: theme.color(ColorRole::Surface),
                    ink: theme.color(if enabled {
                        ColorRole::Text
                    } else {
                        ColorRole::DisabledText
                    }),
                    focus: theme.color(ColorRole::Focus),
                    disabled: theme.color(ColorRole::Surface),
                    size: theme.types().body,
                },
            )?;
        }
        group.pop_clip()
    }

    fn resolve<E>(
        &self,
        ui: &Ui,
        validate: impl FnOnce(ValueChange) -> Result<(), E>,
    ) -> InspectorOutcome<E> {
        let Some(c) = self.controls[..self.count]
            .iter()
            .find(|c| ui.activated(c.id))
        else {
            return InspectorOutcome::Idle;
        };
        let field = &self.fields[c.field];
        if field.state != FieldState::Enabled {
            return InspectorOutcome::Idle;
        }
        let candidate = match field.control {
            InspectorControl::Stepper {
                value,
                min,
                max,
                step,
                policy,
                ..
            } => {
                let next = if c.action == 0 {
                    value - step
                } else {
                    value + step
                };
                if !next.is_finite() {
                    return invalid(field.id, InvalidEdit::NumericLimit);
                }
                if next == value {
                    return invalid(field.id, InvalidEdit::NoNumericProgress);
                }
                let next = match policy {
                    LimitPolicy::Reject if !(min..=max).contains(&next) => {
                        return invalid(field.id, InvalidEdit::NumericLimit);
                    }
                    LimitPolicy::Clamp => next.clamp(min, max),
                    _ => next,
                };
                if next == value {
                    return InspectorOutcome::Idle;
                }
                (FieldValue::Number(value), FieldValue::Number(next))
            }
            InspectorControl::Toggle { value, .. } => {
                (FieldValue::Toggle(value), FieldValue::Toggle(!value))
            }
            InspectorControl::Choice { selected, choices } => {
                if !choices[c.action].enabled || selected == c.action {
                    return InspectorOutcome::Idle;
                }
                (FieldValue::Choice(selected), FieldValue::Choice(c.action))
            }
        };
        let change = ValueChange {
            field: field.id,
            previous: candidate.0,
            value: candidate.1,
        };
        match validate(change) {
            Ok(()) => InspectorOutcome::Accepted(change),
            Err(error) => invalid(field.id, InvalidEdit::Validation(error)),
        }
    }
}

fn invalid<E>(field: WidgetId, reason: InvalidEdit<E>) -> InspectorOutcome<E> {
    InspectorOutcome::Invalid { field, reason }
}
fn text_style(theme: UiTheme, size: f32, role: ColorRole) -> TextStyle {
    TextStyle {
        size,
        color: theme.color(role),
        wrap: true,
        ..TextStyle::default()
    }
}
fn checked_rect(x: f32, y: f32, w: f32, h: f32) -> Result<UiRect, UiError> {
    let right = x + w;
    let bottom = y + h;
    if ![x, y, w, h, right, bottom].iter().all(|n| n.is_finite())
        || w <= 0.
        || h <= 0.
        || right <= x
        || bottom <= y
    {
        return Err(UiError::InvalidLayout);
    }
    Ok(UiRect {
        min: [x, y],
        size: [right - x, bottom - y],
    })
}
fn contained(parent: UiRect, child: UiRect) -> Result<(), UiError> {
    if (0..2).any(|a| {
        child.min[a] < parent.min[a]
            || child.min[a] + child.size[a] > parent.min[a] + parent.size[a]
    }) {
        return Err(UiError::InvalidLayout);
    }
    Ok(())
}
fn text_rect(inner: UiRect, y: f32, copy: &str, size: f32) -> Result<UiRect, UiError> {
    let metrics = WrappedText::new(copy, inner.size[0], size)?.measure();
    if metrics.width > inner.size[0] {
        return Err(UiError::InvalidLayout);
    }
    let rect = checked_rect(inner.min[0], y, inner.size[0], metrics.required_height)?;
    if rect.size[0] < metrics.width || rect.size[1] < metrics.required_height {
        return Err(UiError::InvalidLayout);
    }
    Ok(rect)
}
fn button_width(copy: &str, size: f32) -> Result<f32, UiError> {
    let width = WrappedText::new(copy, f32::MAX, size)?.measure().width;
    Ok((width + 16.).ceil().max(44.) + 1.)
}
fn control_count(c: InspectorControl<'_>) -> usize {
    match c {
        InspectorControl::Stepper { .. } => 2,
        InspectorControl::Toggle { .. } => 1,
        InspectorControl::Choice { choices, .. } => choices.len(),
    }
}
fn control_id(c: InspectorControl<'_>, action: usize) -> WidgetId {
    match c {
        InspectorControl::Stepper {
            decrease, increase, ..
        } => {
            if action == 0 {
                decrease
            } else {
                increase
            }
        }
        InspectorControl::Toggle { id, .. } => id,
        InspectorControl::Choice { choices, .. } => choices[action].id,
    }
}
fn toggle_value(c: InspectorControl<'_>, value: bool) -> InspectorControl<'_> {
    if let InspectorControl::Toggle { id, on, off, .. } = c {
        InspectorControl::Toggle { value, id, on, off }
    } else {
        c
    }
}
fn value_copy(out: &mut CopyBuffer, f: &InspectorField<'_>) -> Result<(), UiError> {
    out.clear();
    write!(
        out,
        "{}{}{}",
        f.display,
        if f.unit.is_empty() { "" } else { " " },
        f.unit
    )
    .map_err(|_| UiError::Capacity)
}
fn status_copy(out: &mut CopyBuffer, state: FieldState<'_>) -> Result<(), UiError> {
    out.clear();
    let (prefix, message) = state.message();
    write!(out, "{prefix}{message}").map_err(|_| UiError::Capacity)
}
fn control_copy(
    out: &mut CopyBuffer,
    c: InspectorControl<'_>,
    action: usize,
) -> Result<(), UiError> {
    out.clear();
    match c {
        InspectorControl::Stepper { .. } => out.write_str(if action == 0 { "-" } else { "+" }),
        InspectorControl::Toggle { value, on, off, .. } => write!(
            out,
            "{} {}",
            if value { "[X]" } else { "[ ]" },
            if value { on } else { off }
        ),
        InspectorControl::Choice { selected, choices } => write!(
            out,
            "{} {}",
            if selected == action { "[X]" } else { "[ ]" },
            choices[action].label
        ),
    }
    .map_err(|_| UiError::Capacity)
}
fn single_line(copy: &str) -> bool {
    !copy.trim().is_empty()
        && !copy.chars().any(char::is_control)
        && !copy.contains(['\u{2028}', '\u{2029}'])
}
fn validate_source(fields: &[InspectorField<'_>]) -> Result<(), UiError> {
    if fields.len() > MAX_INSPECTOR_FIELDS {
        return Err(UiError::Capacity);
    }
    // Lengths are bounded before source is scanned or IDs copied to stack storage.
    let mut bytes = 0usize;
    let mut add = |s: &str| -> Result<(), UiError> {
        bytes = bytes.checked_add(s.len()).ok_or(UiError::Capacity)?;
        if bytes > MAX_INSPECTOR_BYTES {
            return Err(UiError::Capacity);
        }
        Ok(())
    };
    for f in fields {
        add(f.label)?;
        add(f.display)?;
        add(f.unit)?;
        add(f.state.message().1)?;
        match f.control {
            InspectorControl::Choice { choices, .. } => {
                if choices.len() > MAX_INSPECTOR_CHOICES {
                    return Err(UiError::Capacity);
                }
                for choice in choices {
                    add(choice.label)?;
                }
            }
            InspectorControl::Toggle { on, off, .. } => {
                add(on)?;
                add(off)?;
            }
            _ => {}
        }
    }
    let mut ids = [0; MAX_INSPECTOR_FIELDS + MAX_CONTROLS];
    let mut count = 0;
    let mut id = |id| -> Result<(), UiError> {
        if ids[..count].contains(&id) {
            return Err(UiError::DuplicateId);
        }
        ids[count] = id;
        count += 1;
        Ok(())
    };
    for f in fields {
        id(f.id)?;
    }
    for f in fields {
        if f.label.trim().is_empty()
            || f.display.trim().is_empty()
            || (f.state != FieldState::Enabled && f.state.message().1.trim().is_empty())
        {
            return Err(UiError::InvalidLayout);
        }
        match f.control {
            InspectorControl::Stepper {
                value,
                min,
                max,
                step,
                ..
            } => {
                if ![min, max, step].iter().all(|n| n.is_finite())
                    || min > max
                    || step <= 0.
                    || (f.state == FieldState::Enabled
                        && (!value.is_finite() || !(min..=max).contains(&value)))
                {
                    return Err(UiError::InvalidLayout);
                }
            }
            InspectorControl::Toggle { on, off, .. } if !single_line(on) || !single_line(off) => {
                return Err(UiError::InvalidLayout);
            }
            InspectorControl::Choice { selected, choices }
                if selected >= choices.len() || choices.iter().any(|c| !single_line(c.label)) =>
            {
                return Err(UiError::InvalidLayout);
            }
            _ => {}
        }
        for action in 0..control_count(f.control) {
            id(control_id(f.control, action))?;
        }
    }
    Ok(())
}
// Source limit plus small authored prefixes; no unbounded String/format! path.
struct CopyBuffer {
    bytes: [u8; MAX_INSPECTOR_BYTES + 32],
    len: usize,
}
impl CopyBuffer {
    fn new() -> Self {
        Self {
            bytes: [0; MAX_INSPECTOR_BYTES + 32],
            len: 0,
        }
    }
    fn clear(&mut self) {
        self.len = 0;
    }
    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).expect("only UTF-8 str writes")
    }
}
impl Write for CopyBuffer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len.checked_add(text.len()).ok_or(fmt::Error)?;
        self.bytes
            .get_mut(self.len..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}

#[cfg(test)]
mod journey400_285 {
    use super::*;
    use crate::input::{
        ActionKind, Binding, InputCode, PlayerInput, host_frame, touch, touch_event,
    };
    use ferravex_play::ui::theme::ThemeMode;

    fn field() -> InspectorField<'static> {
        InspectorField {
            id: 1,
            label: "MASS",
            display: "1",
            unit: "kg",
            state: FieldState::Enabled,
            control: InspectorControl::Stepper {
                value: 1.,
                min: 0.,
                max: 10.,
                step: 0.25,
                policy: LimitPolicy::Reject,
                decrease: 2,
                increase: 3,
            },
        }
    }
    fn viewport() -> super::super::Viewport {
        super::super::Viewport::responsive([320., 480.], [0.; 4]).unwrap()
    }
    fn accept(_: ValueChange) -> Result<(), ()> {
        Ok(())
    }
    fn touch(phase: touch::Phase, rect: UiRect) {
        touch_event(touch::Event {
            id: 17,
            phase,
            position: crate::Vec2::from_array([rect.min[0] + 2., rect.min[1] + 2.]),
        });
    }

    #[test]
    fn keyboard_focus_skips_disabled_and_held_confirm_does_not_repeat() {
        let choices = [
            InspectorChoice {
                id: 12,
                label: "BALL",
                enabled: false,
            },
            InspectorChoice {
                id: 13,
                label: "CRATE",
                enabled: true,
            },
            InspectorChoice {
                id: 14,
                label: "COMPOUND",
                enabled: true,
            },
        ];
        let fields = [InspectorField {
            control: InspectorControl::Choice {
                selected: 0,
                choices: &choices,
            },
            ..field()
        }];
        let layout = InspectorLayout::new(
            viewport().bounds(),
            &fields,
            UiTheme::preset(ThemeMode::Normal),
        )
        .unwrap();
        let mut ui = Ui::new();
        ui.set_auto_focus(false);
        // Establish focus/epoch before injecting the actual keys.
        host_frame(0., 320, 480, || {
            ui.begin(viewport());
            layout.finish(&mut ui, true, accept).unwrap();
        });
        assert_eq!(ui.focused(), None);
        crate::key(0xff09, true);
        crate::key(0xff09, false);
        crate::key(0xff09, true);
        crate::key(0xff09, false);
        crate::key(0xff0d, true);
        host_frame(0., 320, 480, || {
            ui.begin(viewport());
            assert_eq!(
                layout.finish(&mut ui, true, accept).unwrap(),
                InspectorOutcome::Accepted(ValueChange {
                    field: 1,
                    previous: FieldValue::Choice(0),
                    value: FieldValue::Choice(2)
                })
            );
            assert_eq!(ui.interaction().keyboard_confirmed, Some(14));
        });
        host_frame(0., 320, 480, || {
            ui.begin(viewport());
            assert_eq!(
                layout.finish(&mut ui, true, accept).unwrap(),
                InspectorOutcome::Idle
            );
        });
        crate::key(0xff0d, false);
        host_frame(0., 320, 480, || {
            ui.begin(viewport());
            assert_eq!(
                layout.finish(&mut ui, true, accept).unwrap(),
                InspectorOutcome::Idle
            );
        });
    }

    #[test]
    fn real_remapped_confirm_leaves_gameplay_tap_for_actual_tick_and_blur_cancels() {
        let mut player = PlayerInput::transient("journey400-285-remap");
        player
            .global_action(
                "confirm",
                "Confirm",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(106))],
            )
            .unwrap();
        player
            .action(
                "jump",
                "Jump",
                ActionKind::Button,
                vec![Binding::digital(InputCode::Key(32))],
            )
            .unwrap();
        let fields = [field()];
        let layout = InspectorLayout::new(
            viewport().bounds(),
            &fields,
            UiTheme::preset(ThemeMode::Normal),
        )
        .unwrap();
        let mut ui = Ui::new();
        host_frame(0., 320, 480, || {
            player.sample(true);
            ui.begin(viewport());
            layout
                .finish_player(&mut ui, &mut player, true, accept)
                .unwrap();
        });
        crate::key(106, true);
        crate::key(106, false);
        crate::key(32, true);
        crate::key(32, false);
        host_frame(0., 320, 480, || {
            player.sample(true);
            ui.begin(viewport());
            assert!(matches!(
                layout
                    .finish_player(&mut ui, &mut player, true, accept)
                    .unwrap(),
                InspectorOutcome::Accepted(_)
            ));
            assert_eq!(ui.interaction().player_confirmed, Some(2));
            // This render frame executes no simulation tick.
        });
        host_frame(0., 320, 480, || {
            player.sample(true);
            ui.begin(viewport());
            assert_eq!(
                layout
                    .finish_player(&mut ui, &mut player, true, accept)
                    .unwrap(),
                InspectorOutcome::Idle
            );
            assert!(player.take_tick(["jump"])[0].pressed);
            assert!(!player.take_tick(["jump"])[0].pressed);
        });
        touch(touch::Phase::Start, layout.control_rect(3).unwrap());
        host_frame(0., 320, 480, || {
            ui.begin(viewport());
            layout.finish(&mut ui, true, accept).unwrap();
        });
        crate::focus(false);
        touch(touch::Phase::End, layout.control_rect(3).unwrap());
        host_frame(0., 320, 480, || {
            ui.begin(viewport());
            assert_eq!(
                layout.finish(&mut ui, true, accept).unwrap(),
                InspectorOutcome::Idle
            );
            assert!(ui.interaction().input_blocked);
        });
        crate::focus(true);
        host_frame(0., 320, 480, || {
            ui.begin(viewport());
            assert_eq!(
                layout.finish(&mut ui, true, accept).unwrap(),
                InspectorOutcome::Idle
            );
        });
    }

    #[test]
    fn failed_late_submission_preserves_prior_capture_focus_paint_and_clips() {
        let fields = [field()];
        let layout = InspectorLayout::new(
            viewport().bounds(),
            &fields,
            UiTheme::preset(ThemeMode::Normal),
        )
        .unwrap();
        let r = layout.control_rect(3).unwrap();
        let mut ui = Ui::new();
        ui.set_inspection_enabled(true);
        touch(touch::Phase::Start, r);
        host_frame(0., 320, 480, || {
            ui.begin(viewport());
            layout.finish(&mut ui, true, accept).unwrap();
        });
        assert_eq!(ui.interaction().pointer_capture, Some(3));
        touch(touch::Phase::End, r);
        host_frame(0., 320, 480, || {
            ui.begin(viewport());
            ui.button(3, r, "PRIOR", true, ButtonStyle::default())
                .unwrap();
            assert_eq!(
                layout.finish(&mut ui, true, accept),
                Err(UiError::DuplicateId)
            );
            assert_eq!(ui.inspect().unwrap().count(), 1);
            assert_eq!(ui.focused(), Some(3));
            ui.finish(true);
            assert!(ui.activated(3));
            ui.paint();
            let vertices = crate::read(|s| s.vertices.clone());
            // Repaint exact restored commands: no leaked panel/labels or clip.
            crate::edit(|s| s.vertices.clear());
            ui.paint();
            assert_eq!(vertices, crate::read(|s| s.vertices.clone()));
        });
        // A fully clipped inspector cannot own a fresh press.
        touch(touch::Phase::Start, r);
        touch(touch::Phase::End, r);
        host_frame(0., 320, 480, || {
            ui.begin(viewport());
            ui.push_clip(UiRect {
                min: [300., 450.],
                size: [10., 10.],
            })
            .unwrap();
            assert_eq!(
                layout.finish(&mut ui, true, accept).unwrap(),
                InspectorOutcome::Idle
            );
            assert!(!ui.interaction().pointer_consumed);
            ui.pop_clip();
        });
    }
}
