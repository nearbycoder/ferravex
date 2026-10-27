//! Bounded screen-space HUD/menu drawing. No camera, lighting or material changes.
use super::text_layout::{LineLimit, TextBlockMetrics, WrappedLines, WrappedText};
use super::{AMBER, INK, LINE, PANEL};
use crate::{
    Color,
    input::{InputCode, PlatformEvent, touch},
};
pub use ferravex_play::ui::{
    Anchor, FittedLabel, Rect as UiRect, UiError, Viewport, WidgetId, fit_label, hud_slots, slots,
};
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
    /// Wrap at word boundaries, splitting only words wider than the rectangle.
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
    Text(Range<usize>, TextStyle, LineLimit),
    Button(WidgetId, Range<usize>, ButtonStyle, bool),
}
#[derive(Clone)]
struct Command {
    rect: UiRect,
    clip: UiRect,
    kind: Kind,
}
/// Read-only ownership summary for the most recently finished host frame.
///
/// Sample `PlayerInput`, submit UI, call [`Ui::finish_player`], then inspect
/// [`Ui::interaction`] **before** translating fresh pointer edges into scene
/// commands. Reject scene pointer commands when `pointer_consumed` or
/// `input_blocked` is true. Keep accepted commands in a bounded caller queue
/// until an actual simulation tick; a later frame's summary cannot classify an
/// earlier, buffered gameplay edge. Do not bind that same scene pointer action
/// independently through `PlayerInput` and then gate it using a later summary.
/// Unrelated movement/jump actions still use `PlayerInput::take_tick` in ticks.
/// Local UI must not stop authoritative network/replay ticks.
///
/// This is a conservative frame aggregate, not an event list: several pointer
/// gestures in one frame share one consumed flag; activation fields retain the
/// last accepted ID **per source**, while [`Ui::activated`] retains the last ID
/// overall. It never drains the platform journal or gameplay actions. The
/// existing `finish_player` consumption of the global `confirm` is unchanged.
///
/// Enabled and disabled buttons own presses in their visible clipped bounds.
/// A disabled top button blocks lower buttons and the world, without activating.
/// Fully clipped buttons do not own presses. Panels/text are paint only, even
/// if visually opaque; use [`Ui::modal`] for an explicit viewport-wide blocker.
/// A modal discards lower interaction widgets, but preserves their paint order.
/// Ownership is assigned on press, so a world press dragged onto UI stays world.
/// Only the UI's existing primary-mouse/one-touch route is classified; secondary
/// mouse buttons and additional touches need caller policy.
///
/// ```
/// use ferravex_render::ui::UiInteraction;
/// let idle = UiInteraction::default();
/// assert!(!idle.pointer_consumed && !idle.input_blocked);
/// assert_eq!(idle.pointer_capture, None);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UiInteraction {
    /// Some part of a UI-owned primary gesture occurred or remained held this
    /// frame, including release outside, cancellation and a removed/disabled ID.
    pub pointer_consumed: bool,
    /// Original widget owning the still-held gesture, including its cancelled
    /// tail. This can name a removed widget; it does not promise activation.
    pub pointer_capture: Option<WidgetId>,
    /// Last accepted pointer release, including a nonfocusable modal blocker.
    pub pointer_activated: Option<WidgetId>,
    /// Last focused widget accepted by the built-in keyboard Enter route.
    pub keyboard_confirmed: Option<WidgetId>,
    /// Last focused widget accepted by the selected gamepad's A route.
    pub gamepad_confirmed: Option<WidgetId>,
    /// Last focused widget accepted by the player's remapped global `confirm`.
    /// Its physical device is not inferred; it may also match a built-in route.
    pub player_confirmed: Option<WidgetId>,
    /// Focus loss, journal overflow or the host Controls editor blocks input.
    /// Cancel caller-owned scene gestures too; simulation may keep progressing.
    /// `finish(false)` only disables UI, so it does not itself set this flag.
    /// Once observed, this stays true for the rest of that host frame.
    pub input_blocked: bool,
}

/// Semantic kind of an accepted UI submission. Paint-only items have no ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UiRole {
    Panel,
    Gradient,
    Text,
    Button,
    /// The latest explicit, nonfocusable viewport-wide modal input blocker.
    ModalBlocker,
}

/// Complete original UTF-8 copy, or an explicit inspection-only capacity miss.
/// Missing copy never rejects painting/input and is never silently truncated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiLabel<'a> {
    Full(&'a str),
    Unavailable { required_bytes: usize },
}

/// Borrowed description of one accepted submission, in composition order.
/// Read after finishing layout/input for current focus. Bounds are the exact
/// logical rectangles used by painting/hit testing, before viewport projection.
/// `visible_bounds` is their positive-area clip intersection, **not** ink bounds,
/// occlusion, or a promise that truncated/limited text is all painted. Panels,
/// text and gradients are paint only. Earlier modal scopes remain inspectable.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiElement<'a> {
    pub id: Option<WidgetId>,
    pub role: UiRole,
    pub label: UiLabel<'a>,
    /// Actual retained painter copy (possibly ellipsized); empty for decoration.
    pub painted_label: &'a str,
    pub bounds: UiRect,
    pub visible_bounds: Option<UiRect>,
    /// Submitted button state, not the host/global input enable flag.
    pub enabled: bool,
    pub focused: bool,
    /// Button belongs to the latest modal scope, or is its blocker. Disabled
    /// buttons are still registered and can own/block pointer gestures.
    pub interaction_registered: bool,
}

struct InspectionLabel {
    command: usize,
    range: Option<Range<usize>>,
    required_bytes: usize,
}
struct InspectionStorage {
    labels: Vec<InspectionLabel>,
    text: String,
    active: bool,
}
impl InspectionStorage {
    fn new() -> Self {
        Self {
            labels: Vec::with_capacity(1024),
            text: String::with_capacity(32768),
            active: false,
        }
    }
}

/// Allocation-free borrowed traversal; obtained only by opting in before
/// [`Ui::begin`]. At most 1024 commands plus the latest modal blocker. IDs may
/// repeat across discarded modal scopes; use order and `interaction_registered`.
/// All borrows expire before another mutable UI operation, including `begin`.
#[derive(Clone)]
pub struct UiInspection<'a> {
    ui: &'a Ui,
    storage: &'a InspectionStorage,
    command: usize,
    label: usize,
    blocker_pending: bool,
}
impl<'a> Iterator for UiInspection<'a> {
    type Item = UiElement<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.blocker_pending && self.command == self.ui.modal_start {
            self.blocker_pending = false;
            let bounds = self.ui.viewport.bounds();
            return Some(UiElement {
                id: self.ui.modal_blocker,
                role: UiRole::ModalBlocker,
                label: UiLabel::Full(""),
                painted_label: "",
                bounds,
                visible_bounds: bounds.intersect(bounds),
                enabled: true,
                focused: false,
                interaction_registered: true,
            });
        }
        let index = self.command;
        let command = self.ui.commands.get(index)?;
        self.command += 1;
        let (id, role, painted_label, enabled) = match &command.kind {
            Kind::Panel(_) => (None, UiRole::Panel, "", false),
            Kind::Gradient(..) => (None, UiRole::Gradient, "", false),
            Kind::Text(range, ..) => (None, UiRole::Text, &self.ui.text[range.clone()], false),
            Kind::Button(id, range, _, enabled) => (
                Some(*id),
                UiRole::Button,
                &self.ui.text[range.clone()],
                *enabled,
            ),
        };
        let mut label = UiLabel::Full(painted_label);
        if let Some(full) = self
            .storage
            .labels
            .get(self.label)
            .filter(|l| l.command == index)
        {
            self.label += 1;
            label = match &full.range {
                Some(range) => UiLabel::Full(&self.storage.text[range.clone()]),
                None => UiLabel::Unavailable {
                    required_bytes: full.required_bytes,
                },
            };
        }
        let visible_bounds = command.rect.intersect(command.clip);
        let interaction_registered = id.is_some() && index >= self.ui.modal_start;
        Some(UiElement {
            id,
            role,
            label,
            painted_label,
            bounds: command.rect,
            visible_bounds,
            enabled,
            focused: interaction_registered
                && enabled
                && visible_bounds.is_some()
                && id == self.ui.state.focused(),
            interaction_registered,
        })
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.ui.commands.len() - self.command + usize::from(self.blocker_pending);
        (len, Some(len))
    }
}
impl ExactSizeIterator for UiInspection<'_> {}
impl std::iter::FusedIterator for UiInspection<'_> {}

/// Diagnostic export failure; UI, input and the caller buffer are unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiInspectionExportError {
    Disabled,
    Capacity { required_bytes: usize },
}

#[derive(Clone, Copy)]
enum OwnedPointer {
    Mouse,
    Touch(u64),
}

struct CompositionCheckpoint {
    commands: usize,
    text: usize,
    vertices: usize,
    clips: [UiRect; 16],
    clip_len: usize,
    modal_start: usize,
    modal_blocker: Option<WidgetId>,
    inspection: Option<(usize, usize)>,
}

/// Restricted drawing/widget submission inside [`Ui::compose`].
///
/// Methods use the existing command budgets and styles. The first submission
/// error is sticky: later submissions return it without modifying the UI, and
/// the group fails even if the callback ignores that error. No mutable `Ui`,
/// input delivery, focus changes, painting, lifecycle or configuration is exposed.
/// Callback/formatting side effects outside the UI cannot be rolled back.
///
/// Clips must be balanced within each group; popping an inherited clip or
/// leaving a new clip open rejects the group with [`UiError::InvalidLayout`].
/// Nested groups have independent rollback and may be replaced by a fallback;
/// propagate their error with `?` to reject the containing group. At most 16
/// groups may be active, including the outermost group. Checkpoints use fixed
/// stack storage; no command/text arena is cloned or widget undo journal kept.
///
/// See [`Ui::compose`] for a complete result-card/fallback recipe.
/// Input and lifecycle operations are intentionally unavailable:
///
/// ```compile_fail
/// use ferravex_render::ui::{Ui, Viewport};
/// let mut ui = Ui::new();
/// ui.begin(Viewport::responsive([320., 240.], [0.; 4]).unwrap());
/// ui.compose(|group| {
///     group.finish(true); // Input delivery cannot be rolled back.
///     Ok(())
/// }).unwrap();
/// ```
pub struct UiComposition<'a> {
    ui: &'a mut Ui,
    checkpoint: Option<CompositionCheckpoint>,
    error: Option<UiError>,
    depth: usize,
}
impl UiComposition<'_> {
    fn run<R>(
        ui: &mut Ui,
        depth: usize,
        build: impl FnOnce(&mut UiComposition<'_>) -> Result<R, UiError>,
    ) -> Result<R, UiError> {
        if depth > 16 {
            return Err(UiError::Capacity);
        }
        let checkpoint = CompositionCheckpoint {
            commands: ui.commands.len(),
            text: ui.text.len(),
            vertices: ui.vertices,
            clips: ui.clips,
            clip_len: ui.clip_len,
            modal_start: ui.modal_start,
            modal_blocker: ui.modal_blocker,
            inspection: ui
                .inspection
                .as_ref()
                .map(|s| (s.labels.len(), s.text.len())),
        };
        let mut group = UiComposition {
            ui,
            checkpoint: Some(checkpoint),
            error: None,
            depth,
        };
        let result = build(&mut group);
        if let Some(error) = group.error {
            return Err(error);
        }
        let value = result?;
        if group.ui.clip_len != group.checkpoint.as_ref().unwrap().clip_len {
            return Err(UiError::InvalidLayout);
        }
        group.checkpoint = None;
        Ok(value)
    }
    fn submit<R>(
        &mut self,
        submit: impl FnOnce(&mut Ui) -> Result<R, UiError>,
    ) -> Result<R, UiError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let result = submit(self.ui);
        if let Err(error) = result {
            self.error = Some(error);
        }
        result
    }
    /// Independently atomic subgroup. A handled error does not poison this
    /// group; use `?` to propagate it. Depth 17 rejects before calling `build`.
    pub fn compose<R>(
        &mut self,
        build: impl FnOnce(&mut UiComposition<'_>) -> Result<R, UiError>,
    ) -> Result<R, UiError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        UiComposition::run(self.ui, self.depth + 1, build)
    }
    /// Same logical geometry and validation as [`Ui::push_clip`].
    pub fn push_clip(&mut self, rect: UiRect) -> Result<(), UiError> {
        self.submit(|ui| ui.push_clip(rect))
    }
    /// Pop only a clip pushed in this group; inherited clips remain protected.
    pub fn pop_clip(&mut self) -> Result<(), UiError> {
        let floor = self.checkpoint.as_ref().unwrap().clip_len;
        self.submit(|ui| {
            if ui.clip_len == floor {
                return Err(UiError::InvalidLayout);
            }
            ui.pop_clip();
            Ok(())
        })
    }
    /// Same modal scope change as [`Ui::modal`], restored if this group fails.
    /// Repeated modal calls reuse widget storage; no destructive-operation log.
    pub fn modal(&mut self, blocker: WidgetId) -> Result<(), UiError> {
        self.submit(|ui| ui.modal(blocker))
    }
    /// Submit using [`Ui::panel`].
    pub fn panel(&mut self, rect: UiRect, color: Color) -> Result<(), UiError> {
        self.submit(|ui| ui.panel(rect, color))
    }
    /// Submit using [`Ui::gradient`].
    pub fn gradient(&mut self, rect: UiRect, left: Color, right: Color) -> Result<(), UiError> {
        self.submit(|ui| ui.gradient(rect, left, right))
    }
    /// Submit using [`Ui::text_fmt`]; formatting runs only until the first error.
    pub fn text_fmt(
        &mut self,
        rect: UiRect,
        style: TextStyle,
        args: fmt::Arguments<'_>,
    ) -> Result<(), UiError> {
        self.submit(|ui| ui.text_fmt(rect, style, args))
    }
    /// Submit using [`Ui::text`].
    pub fn text(&mut self, rect: UiRect, text: &str, style: TextStyle) -> Result<(), UiError> {
        self.submit(|ui| ui.text(rect, text, style))
    }
    /// Submit using [`Ui::wrapped_text`], returning its measured block metrics.
    pub fn wrapped_text(
        &mut self,
        rect: UiRect,
        text: &str,
        style: TextStyle,
        limit: LineLimit,
    ) -> Result<TextBlockMetrics, UiError> {
        self.submit(|ui| ui.wrapped_text(rect, text, style, limit))
    }
    /// Submit using [`Ui::label`], including full inspection copy when opted in.
    pub fn label(&mut self, rect: UiRect, text: &str, style: TextStyle) -> Result<(), UiError> {
        self.submit(|ui| ui.label(rect, text, style))
    }
    /// Submit using [`Ui::button`].
    pub fn button(
        &mut self,
        id: WidgetId,
        rect: UiRect,
        text: &str,
        enabled: bool,
        style: ButtonStyle,
    ) -> Result<(), UiError> {
        self.submit(|ui| ui.button(id, rect, text, enabled, style))
    }
    /// Submit using [`Ui::hud_button`].
    pub fn hud_button(
        &mut self,
        id: WidgetId,
        rect: UiRect,
        text: &str,
        enabled: bool,
        style: ButtonStyle,
    ) -> Result<(), UiError> {
        self.submit(|ui| ui.hud_button(id, rect, text, enabled, style))
    }
    /// Submit using [`Ui::button_with_options`].
    pub fn button_with_options(
        &mut self,
        id: WidgetId,
        rect: UiRect,
        text: &str,
        options: super::ButtonOptions,
        style: ButtonStyle,
    ) -> Result<(), UiError> {
        self.submit(|ui| ui.button_with_options(id, rect, text, options, style))
    }
    /// Submit using [`Ui::hud_button_with_options`].
    pub fn hud_button_with_options(
        &mut self,
        id: WidgetId,
        rect: UiRect,
        text: &str,
        options: super::ButtonOptions,
        style: ButtonStyle,
    ) -> Result<(), UiError> {
        self.submit(|ui| ui.hud_button_with_options(id, rect, text, options, style))
    }
}
impl Drop for UiComposition<'_> {
    fn drop(&mut self) {
        let Some(saved) = self.checkpoint.take() else {
            return;
        };
        self.ui.commands.truncate(saved.commands);
        self.ui.text.truncate(saved.text);
        self.ui.vertices = saved.vertices;
        self.ui.clips = saved.clips;
        self.ui.clip_len = saved.clip_len;
        self.ui.modal_start = saved.modal_start;
        self.ui.modal_blocker = saved.modal_blocker;
        if let (Some(storage), Some((labels, text))) = (&mut self.ui.inspection, saved.inspection) {
            storage.labels.truncate(labels);
            storage.text.truncate(text);
        }
        // Only add/begin_frame are exposed through composition. Neither mutates
        // focus/capture, so reconstruct registrations from the retained painter
        // commands without calling end_frame or maintaining a widget journal.
        // These exact widgets already fitted before the group started.
        self.ui.state.begin_frame();
        if let Some(id) = saved.modal_blocker {
            self.ui
                .state
                .add(Widget {
                    id,
                    rect: self.ui.viewport.bounds(),
                    clip: Some(self.ui.viewport.bounds()),
                    enabled: true,
                    focusable: false,
                })
                .expect("retained modal registration fits");
        }
        for command in &self.ui.commands[saved.modal_start..] {
            if let Kind::Button(id, _, _, enabled) = command.kind {
                self.ui
                    .state
                    .add(Widget {
                        id,
                        rect: command.rect,
                        clip: Some(command.clip),
                        enabled,
                        focusable: true,
                    })
                    .expect("retained widget registration fits");
            }
        }
    }
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
    interaction: UiInteraction,
    owned_pointer: Option<(OwnedPointer, WidgetId)>,
    modal_start: usize,
    modal_blocker: Option<WidgetId>,
    inspection: Option<Box<InspectionStorage>>,
}
impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}
impl Ui {
    /// Submit an atomic group through a restricted [`UiComposition`].
    ///
    /// On an ordinary submission/callback error, restore commands, text, vertex
    /// estimates, clips, inspection copy/metadata and modal/widget registrations.
    /// Prior focus, capture and [`UiInteraction`] are preserved: input is not
    /// delivered within the group. Success commits all submissions and returns
    /// the callback's value. The first submission error wins over a later error.
    /// Nested errors may be handled locally; see [`UiComposition::compose`].
    ///
    /// Rollback truncates append-only arenas and reconstructs at most 1024
    /// widgets from retained commands. Existing duplicate-ID checking makes
    /// reconstruction at worst quadratic in that fixed widget bound. No heap
    /// scratch, arena clones or unbounded undo log are needed, including for
    /// repeated modal replacements. Each active group saves a 16-entry clip
    /// array and scalar checkpoints, with a maximum nesting depth of 16.
    ///
    /// The drop guard also restores UI during Rust panic unwinding; it does not
    /// catch panics, handle abort/allocation failure or undo callback/formatter
    /// side effects outside this UI. Do not paint, deliver input or perform game
    /// actions indirectly from a callback. Compose before finishing/painting.
    ///
    /// This headless result-card recipe exercises both success and a rejected
    /// oversized body. A small explicit fallback retains the same readable
    /// primary action. Start with room for that fallback; if prior groups use
    /// all budgets, even a fallback can reject and its error must be handled.
    /// Game rules/feedback run later from accepted actions, on actual ticks.
    ///
    /// ```
    /// use ferravex_render::ui::{ButtonStyle, TextStyle, Ui, UiError, UiRect,
    ///     UiLabel, Viewport, PANEL, text_layout::LineLimit};
    /// let mut ui = Ui::new(); // Reuse across frames.
    /// ui.set_inspection_enabled(true); // Optional host diagnostics.
    /// let viewport = Viewport::responsive([320., 240.], [0.; 4])?;
    /// let card = UiRect { min: [12.; 2], size: [296., 216.] };
    /// let title = UiRect { min: [24., 24.], size: [272., 28.] };
    /// let body = UiRect { min: [24., 64.], size: [272., 90.] };
    /// let action = UiRect { min: [24., 172.], size: [272., 44.] };
    /// let oversized = "x".repeat(32769); // Inject a rejected result payload.
    /// for (copy, rejected) in [("DELIVERY COMPLETE", false), (&oversized, true)] {
    ///     ui.begin(viewport);
    ///     let result = ui.compose(|card_ui| {
    ///         card_ui.modal(900)?;
    ///         card_ui.panel(card, PANEL)?;
    ///         card_ui.label(title, "RESULTS", TextStyle::default())?;
    ///         card_ui.wrapped_text(body, copy, TextStyle::default(), LineLimit::Clip(5))?;
    ///         card_ui.hud_button(901, action, "CONTINUE", true, ButtonStyle::default())
    ///     });
    ///     assert_eq!(result.is_err(), rejected);
    ///     if result.is_err() {
    ///         ui.compose(|fallback| {
    ///             fallback.modal(900)?;
    ///             fallback.panel(card, PANEL)?;
    ///             fallback.label(title, "RESULTS UNAVAILABLE", TextStyle::default())?;
    ///             fallback.hud_button(901, action, "CONTINUE", true, ButtonStyle::default())
    ///         })?;
    ///     }
    ///     ui.finish(true); // Deliver input only after choosing the full group.
    ///     let primary = ui.inspect().unwrap().find(|e| e.id == Some(901)).unwrap();
    ///     assert_eq!(primary.label, UiLabel::Full("CONTINUE"));
    ///     assert!(primary.enabled && primary.interaction_registered);
    ///     assert_eq!(primary.visible_bounds, Some(action));
    ///     ui.paint();
    /// }
    /// # Ok::<(), UiError>(())
    /// ```
    pub fn compose<R>(
        &mut self,
        build: impl FnOnce(&mut UiComposition<'_>) -> Result<R, UiError>,
    ) -> Result<R, UiError> {
        UiComposition::run(self, 1, build)
    }
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
            interaction: UiInteraction::default(),
            owned_pointer: None,
            modal_start: 0,
            modal_blocker: None,
            inspection: None,
        }
    }
    /// Reserved CPU storage, including this value; excludes the renderer's shared stream.
    pub fn reserved_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.commands.capacity() * std::mem::size_of::<Command>()
            + self.text.capacity()
            + self.state.capacity() * std::mem::size_of::<Widget>()
            + self.inspection.as_ref().map_or(0, |s| {
                std::mem::size_of::<InspectionStorage>()
                    + s.labels.capacity() * std::mem::size_of::<InspectionLabel>()
                    + s.text.capacity()
            })
    }
    /// Opt in before `begin`; enabling allocates bounded supplemental storage
    /// once. Inspection starts at the next `begin`, so shortened labels from an
    /// earlier submission are never misreported as full. Disabling immediately
    /// drops this storage. Default-off instances retain the original arenas and
    /// submission behavior, with only one added optional pointer in `Ui`.
    ///
    /// Ordinary text/button copy borrows the existing paint arena. Only shortened
    /// `label`/`hud_button` source uses an extra 32 KiB UTF-8 arena and at most
    /// 1024 metadata entries. Overflow yields `UiLabel::Unavailable`, without
    /// changing command acceptance. `begin` clears all inspection copy and IDs.
    pub fn set_inspection_enabled(&mut self, enabled: bool) {
        if enabled && self.inspection.is_none() {
            self.inspection = Some(Box::new(InspectionStorage::new()));
        } else if !enabled {
            self.inspection = None;
        }
    }
    /// Inspect accepted submissions without consuming input or modifying paint.
    /// Rejected commands never appear. The latest modal blocker is synthesized
    /// from existing modal state; earlier buttons remain with registration false.
    /// No snapshot history or action-execution API is provided.
    ///
    /// Runnable native/headless host recipe: compose actual UI, locate a visible
    /// action by its full label, project its clipped geometry for host diagnostics,
    /// then explicitly export into a bounded caller buffer. Hosts choose where
    /// (or whether) to publish these diagnostics; there is no global/Wasm export.
    ///
    /// ```
    /// use ferravex_render::{input::host_frame, ui::{ButtonStyle, Ui,
    ///     UiLabel, UiRect, UiRole, Viewport}};
    /// let viewport = Viewport::fit([400., 200.], [100.; 2], [0.; 4]).unwrap();
    /// let mut ui = Ui::new();
    /// ui.set_inspection_enabled(true); // Setup, outside the frame loop.
    /// host_frame(0., 400, 200, || {
    ///     ui.begin(viewport);
    ///     ui.push_clip(UiRect { min: [20., 30.], size: [10., 20.] }).unwrap();
    ///     ui.hud_button(7, UiRect { min: [10., 20.], size: [80., 40.] },
    ///         "RESUME EXPEDITION", true, ButtonStyle::default()).unwrap();
    ///     ui.finish(true);
    ///     let action = ui.inspect().unwrap().find(|e| e.role == UiRole::Button
    ///         && e.label == UiLabel::Full("RESUME EXPEDITION")).unwrap();
    ///     assert_eq!(action.id, Some(7));
    ///     let visible = action.visible_bounds.unwrap();
    ///     assert_eq!(visible, UiRect { min: [20., 30.], size: [10., 20.] });
    ///     assert!(visible.contains([25., 40.])); // Same half-open hit geometry.
    ///     let pixel = std::array::from_fn::<_, 2, _>(|i|
    ///         viewport.pixels().min[i] + [25., 40.][i] * viewport.scale());
    ///     assert_eq!(pixel, [150., 80.]);
    ///     ui.paint(); // Inspection leaves the normal paint path intact.
    ///     let mut diagnostic = [0_u8; 2048];
    ///     let written = ui.write_inspection(&mut diagnostic).unwrap();
    ///     let json = std::str::from_utf8(&diagnostic[..written]).unwrap();
    ///     assert!(json.contains("RESUME EXPEDITION"));
    ///     assert!(json.contains("\"visible_bounds\":[20,30,10,20]"));
    /// });
    /// ```
    pub fn inspect(&self) -> Option<UiInspection<'_>> {
        let storage = self.inspection.as_deref().filter(|s| s.active)?;
        Some(UiInspection {
            ui: self,
            storage,
            command: 0,
            label: 0,
            blocker_pending: self.modal_blocker.is_some(),
        })
    }
    /// Write diagnostic JSON schema 1 (UTF-8, no terminator) to caller storage.
    /// Its `elements` follow `inspect`; IDs are decimal strings (lossless u64
    /// in JavaScript), rectangles are `[x,y,width,height]` in
    /// logical units. Unavailable labels are null with `label_required_bytes`.
    /// This is a diagnostic format, not scene/save/replay or an action protocol.
    /// Two bounded passes determine required bytes before writing: insufficient
    /// output returns its exact requirement and leaves the entire buffer intact.
    /// The tail after the returned byte count is unchanged. Disabled inspection
    /// returns `Disabled`, rather than misleadingly exporting an empty scene.
    pub fn write_inspection(&self, output: &mut [u8]) -> Result<usize, UiInspectionExportError> {
        let inspection = self.inspect().ok_or(UiInspectionExportError::Disabled)?;
        let mut count = InspectionCount(0);
        write_inspection_json(inspection.clone(), &mut count).expect("counting cannot fail");
        if output.len() < count.0 {
            return Err(UiInspectionExportError::Capacity {
                required_bytes: count.0,
            });
        }
        write_inspection_json(
            inspection,
            &mut InspectionOutput {
                bytes: output,
                len: 0,
            },
        )
        .expect("preflighted output capacity");
        Ok(count.0)
    }
    // Only called after a successful command. Command budget bounds metadata;
    // no inspection capacity failure can alter paint or interaction acceptance.
    fn inspect_full_label(&mut self, source: &str, shortened: bool) {
        if !shortened {
            return;
        }
        if let Some(storage) = self.inspection.as_deref_mut().filter(|s| s.active) {
            let range = if source.len() <= 32768 - storage.text.len() {
                let start = storage.text.len();
                storage.text.push_str(source);
                Some(start..storage.text.len())
            } else {
                None
            };
            storage.labels.push(InspectionLabel {
                command: self.commands.len() - 1,
                range,
                required_bytes: source.len(),
            });
        }
    }
    pub fn begin(&mut self, viewport: Viewport) {
        if let Some(storage) = &mut self.inspection {
            storage.labels.clear();
            storage.text.clear();
            storage.active = true;
        }
        self.viewport = viewport;
        self.commands.clear();
        self.text.clear();
        self.vertices = 0;
        self.clips[0] = viewport.bounds();
        self.clip_len = 1;
        self.state.begin_frame();
        self.active = None;
        self.modal_start = 0;
        self.modal_blocker = None;
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
        })?;
        self.modal_start = self.commands.len();
        self.modal_blocker = Some(blocker);
        Ok(())
    }
    /// Menus may accept the first button immediately; gameplay overlays can require
    /// explicit navigation/pointer focus so a shared gamepad action keeps its role.
    pub fn set_auto_focus(&mut self, enabled: bool) {
        self.auto_focus = enabled;
    }
    pub fn focused(&self) -> Option<WidgetId> {
        self.state.focused()
    }
    /// Enabled widget under the pointer as of the last finished frame (the
    /// same hover that [`Self::paint`] highlights). Used by `ui::kit` visuals.
    pub fn hovered(&self) -> Option<WidgetId> {
        self.hovered
    }
    /// Viewport passed to the most recent [`Self::begin`].
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }
    pub fn activated(&self, id: WidgetId) -> bool {
        self.active == Some(id)
    }
    /// Inspect after finishing UI, before translating this frame's scene input.
    /// Repeated reads/finishes in one host frame retain the aggregate, including
    /// across `begin`; the next host frame's finish resets transient fields.
    /// No allocations or input consumption occur here. See [`UiInteraction`]
    /// for routing policy and the runnable tick-queue recipe below.
    ///
    /// This headless recipe uses injected primary touches in logical pixels.
    /// Its one-slot queue rejects new requests while full (no overwrite). A real
    /// host passes its fresh pointer press/position at the same point; mouse
    /// polling can use `is_mouse_button_pressed(MouseButton::Left)`. Do not read
    /// a tick-buffered fire action here. Buy/drop are requests: rules and feedback
    /// run only when the actual tick accepts them. Reset the queue on restart.
    ///
    /// ```
    /// use ferravex_core::session::GameSession;
    /// use ferravex_render::{Vec2, input::{ActionKind, Binding, InputCode,
    ///     PlayerInput, host_frame, touch, touch_event},
    ///     ui::{ButtonStyle, Ui, UiRect, Viewport}};
    /// #[derive(Clone, Copy, Debug, PartialEq)]
    /// enum Command { Buy, Drop([f32; 2]) }
    /// let mut ui = Ui::new();
    /// let mut player = PlayerInput::transient("ui-world-recipe");
    /// player.action("jump", "Jump", ActionKind::Button,
    ///     vec![Binding::digital(InputCode::Key(32))]).unwrap();
    /// let viewport = Viewport::responsive([320., 240.], [0.; 4]).unwrap();
    /// let mut session = GameSession::try_new(64., 4, true).unwrap();
    /// let mut pending = None; // Bounded one-slot command queue.
    /// let mut executed = Vec::new(); // Test observation, not the command queue.
    /// for (id, position, elapsed) in [
    ///     (1, [30., 30.], 0.), // Buy: zero ticks; no scene drop.
    ///     (2, [180., 160.], 0.), // Queue full: this request is rejected.
    ///     (3, [180., 160.], 1. / 64.), // First actual tick executes Buy.
    ///     (4, [180., 160.], 1. / 64.), // World drop executes once.
    /// ] {
    ///     for phase in [touch::Phase::Start, touch::Phase::End] {
    ///         touch_event(touch::Event { id, phase,
    ///             position: Vec2::from(position) });
    ///     }
    ///     host_frame(elapsed as f32, 320, 240, || {
    ///         player.sample(true); // Sample globals/gameplay without ticking.
    ///         ui.begin(viewport);
    ///         ui.button(10, UiRect { min: [20.; 2], size: [100., 44.] },
    ///             "BUY", true, ButtonStyle::default()).unwrap();
    ///         ui.finish_player(&mut player, true);
    ///         let ownership = ui.interaction();
    ///         let request = if ui.activated(10) { Some(Command::Buy) }
    ///             else if !ownership.input_blocked && !ownership.pointer_consumed {
    ///                 Some(Command::Drop(position)) // This host's fresh press.
    ///             } else { None };
    ///         if pending.is_none() { pending = request; } // Explicit full policy.
    ///         let frame = session.advance(elapsed);
    ///         for _ in frame.ticks() { // Only actually executed updates consume.
    ///             let [_jump] = player.take_tick(["jump"]);
    ///             if let Some(command) = pending.take() { executed.push(command); }
    ///         }
    ///         ui.paint();
    ///     });
    /// }
    /// assert_eq!(executed, [Command::Buy, Command::Drop([180., 160.])]);
    /// assert_eq!(pending, None);
    /// ```
    pub fn interaction(&self) -> UiInteraction {
        self.interaction
    }
    /// Cancel activation but retain an owned gesture's tail until release, so
    /// closing/removing UI cannot turn that release into a scene action. Focus
    /// loss/overflow reset ownership because the journal can no longer prove it.
    pub fn cancel(&mut self) {
        self.state.cancel_pointer();
        self.touch = None;
        self.mouse_capture = false;
    }
    fn ownership_hit(&self, point: [f32; 2]) -> Option<WidgetId> {
        self.commands[self.modal_start..]
            .iter()
            .rev()
            .find_map(|command| match command.kind {
                Kind::Button(id, ..)
                    if command.rect.contains(point) && command.clip.contains(point) =>
                {
                    Some(id)
                }
                _ => None,
            })
            .or(self
                .modal_blocker
                .filter(|_| self.viewport.bounds().contains(point)))
    }
    fn own_pointer(&mut self, pointer: OwnedPointer, point: [f32; 2]) {
        if self.owned_pointer.is_none() {
            self.owned_pointer = self.ownership_hit(point).map(|id| (pointer, id));
        }
        if self.owned_pointer.is_some() {
            self.interaction.pointer_consumed = true;
        }
    }
    fn pointer_up(&mut self, point: [f32; 2]) {
        let accepted = self.state.pointer_up(point);
        self.active = accepted.or(self.active);
        self.interaction.pointer_consumed |= accepted.is_some();
        self.interaction.pointer_activated = accepted.or(self.interaction.pointer_activated);
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
            kind: Kind::Text(start..self.text.len(), style, LineLimit::Unlimited),
        });
        self.vertices += vertices;
        Ok(())
    }
    pub fn text(&mut self, rect: UiRect, text: &str, style: TextStyle) -> Result<(), UiError> {
        self.text_fmt(rect, style, format_args!("{text}"))
    }
    /// Queue measured word-wrapped copy at fixed `style.size`, with an explicit
    /// line limit. `style.wrap` is ignored; color, alignment and shadow are kept.
    /// The returned metrics match [`WrappedText::measure`] before geometric
    /// clipping: `rect` and the current clip can still cut a partial last line.
    /// Use the measured `height` for a content-sized block, or a line limit of
    /// `floor(rect.size[1] / style.size)` to keep complete line slots in a card.
    ///
    /// The full source is copied into the existing 32 KiB arena, even when lines
    /// are hidden. Only retained glyphs (including the suffix) reserve vertices.
    /// Invalid styles/rectangles or exhausted budgets reject the whole command;
    /// an empty/zero-line block still uses one command, like [`Self::text`].
    ///
    /// ```
    /// use ferravex_render::ui::{TextStyle, Ui, UiRect, Viewport};
    /// use ferravex_render::ui::text_layout::LineLimit;
    /// let mut ui = Ui::new();
    /// ui.begin(Viewport::responsive([320., 240.], [0.; 4])?);
    /// let report = ui.wrapped_text(
    ///     UiRect { min: [16., 24.], size: [288., 36.] },
    ///     "BRIEFING\nCollect the keys.\nReturn to the gate.",
    ///     TextStyle::default(), LineLimit::Ellipsis(2),
    /// )?;
    /// assert_eq!((report.visible_lines, report.hidden_lines), (2, 1));
    /// assert!(report.is_truncated());
    /// ui.paint();
    /// # Ok::<(), ferravex_render::ui::UiError>(())
    /// ```
    pub fn wrapped_text(
        &mut self,
        rect: UiRect,
        text: &str,
        style: TextStyle,
        limit: LineLimit,
    ) -> Result<TextBlockMetrics, UiError> {
        if !valid_text(style) {
            return Err(UiError::InvalidLayout);
        }
        self.reserve(rect, 0)?;
        if text.len() > 32768 - self.text.len() {
            return Err(UiError::Capacity);
        }
        let layout = WrappedText::new(text, rect.size[0], style.size)?.with_limit(limit);
        let measured = layout.measure();
        let glyphs = layout
            .lines()
            .map(|line| line.text.chars().count() + line.suffix.len())
            .sum::<usize>();
        let vertices = glyphs * 126 * if style.shadow { 2 } else { 1 };
        self.reserve(rect, vertices)?;
        let start = self.text.len();
        self.text.push_str(text);
        self.commands.push(Command {
            rect,
            clip: self.clip(),
            kind: Kind::Text(
                start..self.text.len(),
                TextStyle {
                    wrap: true,
                    ..style
                },
                limit,
            ),
        });
        self.vertices += vertices;
        Ok(measured)
    }
    /// A single-line HUD label that keeps `style.size` and ellipsizes overflow.
    /// Alignment and shadow are honored; `wrap` is ignored. Newlines truncate.
    /// The visible prefix is copied directly into the reusable text arena, so
    /// even a very long source label consumes only its visible text budget.
    /// Use [`Viewport::responsive`] to keep this size in host logical pixels.
    ///
    /// ```
    /// use ferravex_render::ui::{Anchor, TextStyle, Ui, Viewport};
    /// let mut ui = Ui::new(); // Keep this outside the frame loop.
    /// let viewport = Viewport::responsive([320., 240.], [0., 20., 0., 0.])?;
    /// ui.begin(viewport);
    /// let score = Anchor::TopRight.place(viewport.bounds().inset([12.; 4])?, [180., 24.])?;
    /// ui.label(score, "COLLECT THE REMAINING CRYSTALS", TextStyle::default())?;
    /// # Ok::<(), ferravex_render::ui::UiError>(())
    /// ```
    pub fn label(&mut self, rect: UiRect, text: &str, style: TextStyle) -> Result<(), UiError> {
        if !valid_text(style) {
            return Err(UiError::InvalidLayout);
        }
        let label = fit_label(text, rect.size[0], style.size * 0.75)?;
        self.text_fmt(
            rect,
            TextStyle {
                wrap: false,
                ..style
            },
            format_args!("{}{}", label.prefix, label.suffix),
        )?;
        self.inspect_full_label(text, label.prefix.len() != text.len());
        Ok(())
    }
    /// A button with an ellipsized, fixed-size label and the same clipped
    /// hit/focus rectangle as [`Self::button`]. Eight logical pixels on each side
    /// are reserved for padding; on tiny rectangles text clips to the button.
    /// The full label is retained only when inspection is opted in and its
    /// separate bounded arena has room; otherwise hosts keep accessibility copy.
    ///
    /// ```
    /// use ferravex_render::ui::{ButtonStyle, Ui, Viewport, hud_slots};
    /// let mut ui = Ui::new(); // Reuse between frames.
    /// let viewport = Viewport::responsive([320., 240.], [0.; 4])?;
    /// ui.begin(viewport);
    /// let labels = ["RESUME EXPEDITION", "RESTART FROM CHECKPOINT", "QUIT"];
    /// // Two 144px columns; the third action wraps, retaining a 44px row height.
    /// for (index, rect) in hud_slots(viewport.bounds().inset([12.; 4])?, 3, 140., 44., 8.)?.enumerate() {
    ///     ui.hud_button(index as u64, rect, labels[index], true, ButtonStyle::default())?;
    /// }
    /// # Ok::<(), ferravex_render::ui::UiError>(())
    /// ```
    pub fn hud_button(
        &mut self,
        id: WidgetId,
        rect: UiRect,
        text: &str,
        enabled: bool,
        style: ButtonStyle,
    ) -> Result<(), UiError> {
        let label = fit_label(text, (rect.size[0] - 16.).max(0.), style.size * 0.75)?;
        self.queue_button(id, rect, [label.prefix, label.suffix], enabled, style)?;
        self.inspect_full_label(text, label.prefix.len() != text.len());
        Ok(())
    }
    pub fn button(
        &mut self,
        id: WidgetId,
        rect: UiRect,
        text: &str,
        enabled: bool,
        style: ButtonStyle,
    ) -> Result<(), UiError> {
        self.queue_button(id, rect, [text, ""], enabled, style)
    }
    fn queue_button(
        &mut self,
        id: WidgetId,
        rect: UiRect,
        text: [&str; 2],
        enabled: bool,
        style: ButtonStyle,
    ) -> Result<(), UiError> {
        let vertices = text.iter().map(|part| part.chars().count()).sum::<usize>() * 126 + 30;
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
        if text.iter().map(|part| part.len()).sum::<usize>() > 32768 - self.text.len() {
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
        for part in text {
            self.text.push_str(part);
        }
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
            self.interaction.player_confirmed = self.accept().or(self.interaction.player_confirmed);
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
        if self.frame != Some(frame) {
            self.interaction = UiInteraction {
                pointer_consumed: self.owned_pointer.is_some(),
                pointer_capture: self.owned_pointer.map(|(_, id)| id),
                ..Default::default()
            };
        }
        self.interaction.input_blocked |= blocked || !focused || overflow;
        if epoch != self.epoch || !focused || overflow {
            self.shift = 0;
        }
        if epoch != self.epoch || !focused || overflow || !enabled || blocked {
            self.cancel();
        }
        if epoch != self.epoch || !focused || overflow {
            self.owned_pointer = None;
            self.interaction.pointer_capture = None;
        }
        self.epoch = epoch;
        if self.frame == Some(frame) {
            return;
        }
        self.frame = Some(frame);
        let interactive = enabled && !blocked && focused && !overflow;
        if interactive {
            self.hovered = self
                .viewport
                .unproject(pointer)
                .and_then(|p| self.state.hit_test(p));
        }
        let mut event_pointer = pointer;
        for event in events {
            if matches!(event, PlatformEvent::None) {
                break;
            }
            // A cancelled/removed widget still owns its gesture's release.
            // Observe terminal events even when UI is disabled or Controls open.
            let ended = match (self.owned_pointer, event) {
                (Some((OwnedPointer::Mouse, _)), PlatformEvent::Value(InputCode::Mouse(1), v)) => {
                    v < 0.5
                }
                (Some((OwnedPointer::Touch(id), _)), PlatformEvent::Touch(e)) => {
                    e.id == id && matches!(e.phase, touch::Phase::End | touch::Phase::Cancel)
                }
                _ => false,
            };
            if ended {
                self.interaction.pointer_consumed = true;
                self.owned_pointer = None;
            }
            if !interactive {
                continue;
            }
            match event {
                PlatformEvent::PointerPosition(p) => event_pointer = [p.x, p.y],
                PlatformEvent::Value(InputCode::Mouse(1), v) if self.touch.is_none() => {
                    let p = self
                        .viewport
                        .unproject(event_pointer)
                        .unwrap_or([f32::NEG_INFINITY; 2]);
                    if v >= 0.5 {
                        if self.owned_pointer.is_none() {
                            self.mouse_capture = true;
                            self.own_pointer(OwnedPointer::Mouse, p);
                            self.state.pointer_down(p);
                        }
                    } else if self.mouse_capture {
                        self.mouse_capture = false;
                        self.pointer_up(p);
                    }
                }
                PlatformEvent::Touch(e) => {
                    let p = self
                        .viewport
                        .unproject([e.position.x, e.position.y])
                        .unwrap_or([f32::NEG_INFINITY; 2]);
                    match e.phase {
                        touch::Phase::Start
                            if self.touch.is_none()
                                && !self.mouse_capture
                                && self.owned_pointer.is_none() =>
                        {
                            self.touch = Some(e.id);
                            self.own_pointer(OwnedPointer::Touch(e.id), p);
                            self.state.pointer_down(p);
                        }
                        touch::Phase::End if self.touch == Some(e.id) => {
                            self.touch = None;
                            self.pointer_up(p);
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
                    0xff0d => {
                        self.interaction.keyboard_confirmed =
                            self.accept().or(self.interaction.keyboard_confirmed);
                    }
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
                    0 => {
                        self.interaction.gamepad_confirmed =
                            self.accept().or(self.interaction.gamepad_confirmed);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        self.interaction.pointer_capture = self.owned_pointer.map(|(_, id)| id);
    }
    fn accept(&mut self) -> Option<WidgetId> {
        if self.auto_focus && self.state.focused().is_none() {
            self.state.focus_next(false);
        }
        let accepted = self.state.activate_focused();
        self.active = accepted.or(self.active);
        accepted
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
                    Kind::Text(range, style, limit) => {
                        painter.text(c.rect, c.clip, &self.text[range.clone()], *style, *limit)
                    }
                    Kind::Button(id, range, style, enabled) => {
                        let Some(clip) = c.rect.intersect(c.clip) else {
                            continue;
                        };
                        painter.rect(
                            c.rect,
                            clip,
                            if *enabled { style.fill } else { style.disabled },
                        );
                        if *enabled
                            && (self.state.focused() == Some(*id) || self.hovered == Some(*id))
                        {
                            let r = c.rect;
                            let border = 2_f32.min(r.size[0]).min(r.size[1]);
                            for edge in [
                                UiRect {
                                    min: r.min,
                                    size: [r.size[0], border],
                                },
                                UiRect {
                                    min: [r.min[0], r.min[1] + r.size[1] - border],
                                    size: [r.size[0], border],
                                },
                                UiRect {
                                    min: r.min,
                                    size: [border, r.size[1]],
                                },
                                UiRect {
                                    min: [r.min[0] + r.size[0] - border, r.min[1]],
                                    size: [border, r.size[1]],
                                },
                            ] {
                                painter.rect(edge, clip, style.focus);
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
                            clip,
                            &self.text[range.clone()],
                            TextStyle {
                                size: style.size,
                                color: style.ink,
                                align: Align::Center,
                                ..Default::default()
                            },
                            LineLimit::Unlimited,
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
struct InspectionCount(usize);
impl Write for InspectionCount {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        self.0 += text.len();
        Ok(())
    }
}
struct InspectionOutput<'a> {
    bytes: &'a mut [u8],
    len: usize,
}
impl Write for InspectionOutput<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.len + text.len();
        let dest = self.bytes.get_mut(self.len..end).ok_or(fmt::Error)?;
        dest.copy_from_slice(text.as_bytes());
        self.len = end;
        Ok(())
    }
}
fn inspection_json_string(out: &mut impl Write, text: &str) -> fmt::Result {
    out.write_char('"')?;
    for c in text.chars() {
        match c {
            '"' => out.write_str("\\\"")?,
            '\\' => out.write_str("\\\\")?,
            c if c <= '\u{1f}' => write!(out, "\\u{:04x}", c as u32)?,
            c => out.write_char(c)?,
        }
    }
    out.write_char('"')
}
fn inspection_json_rect(out: &mut impl Write, rect: Option<UiRect>) -> fmt::Result {
    match rect {
        Some(r) => write!(
            out,
            "[{},{},{},{}]",
            r.min[0], r.min[1], r.size[0], r.size[1]
        ),
        None => out.write_str("null"),
    }
}
fn write_inspection_json(elements: UiInspection<'_>, out: &mut impl Write) -> fmt::Result {
    out.write_str("{\"schema\":1,\"elements\":[")?;
    for (i, e) in elements.enumerate() {
        if i != 0 {
            out.write_char(',')?;
        }
        out.write_str("{\"id\":")?;
        match e.id {
            Some(id) => write!(out, "\"{id}\"")?,
            None => out.write_str("null")?,
        }
        out.write_str(",\"role\":")?;
        inspection_json_string(
            out,
            match e.role {
                UiRole::Panel => "panel",
                UiRole::Gradient => "gradient",
                UiRole::Text => "text",
                UiRole::Button => "button",
                UiRole::ModalBlocker => "modal_blocker",
            },
        )?;
        out.write_str(",\"label\":")?;
        match e.label {
            UiLabel::Full(text) => inspection_json_string(out, text)?,
            UiLabel::Unavailable { .. } => out.write_str("null")?,
        }
        if let UiLabel::Unavailable { required_bytes } = e.label {
            write!(out, ",\"label_required_bytes\":{required_bytes}")?;
        }
        out.write_str(",\"painted_label\":")?;
        inspection_json_string(out, e.painted_label)?;
        out.write_str(",\"bounds\":")?;
        inspection_json_rect(out, Some(e.bounds))?;
        out.write_str(",\"visible_bounds\":")?;
        inspection_json_rect(out, e.visible_bounds)?;
        write!(
            out,
            ",\"enabled\":{},\"focused\":{},\"interaction_registered\":{}}}",
            e.enabled, e.focused, e.interaction_registered
        )?;
    }
    out.write_str("]}")
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
    fn text(&mut self, r: UiRect, clip: UiRect, text: &str, style: TextStyle, limit: LineLimit) {
        let Some(clip) = r.intersect(clip) else {
            return;
        };
        // Fixed bitmap metrics, UTF-8 boundaries and word wrapping preserve readable copy.
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
        let columns = (r.size[0] / advance).floor() as usize;
        let lines = WrappedLines::new(
            text,
            if style.wrap { columns } else { usize::MAX },
            columns,
            advance,
            limit,
        );
        for (line_index, line) in lines.enumerate() {
            let y = r.min[1] + line_index as f32 * size;
            if y >= r.min[1] + r.size[1] {
                break;
            }
            let width = line.width;
            let x = r.min[0]
                + match style.align {
                    Align::Left => 0.,
                    Align::Center => (r.size[0] - width) * 0.5,
                    Align::Right => r.size[0] - width,
                };
            for (index, c) in line.text.chars().chain(line.suffix.chars()).enumerate() {
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
#[cfg(test)]
mod tests {
    use super::super::text_layout::Lines;
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
    #[derive(Debug, PartialEq)]
    struct CompositionSnapshot {
        commands: usize,
        text: String,
        vertices: usize,
        clips: [UiRect; 16],
        clip_len: usize,
        modal: (usize, Option<WidgetId>),
        inspection: Option<(usize, String, bool, Vec<u8>)>,
        focus_capture: (Option<WidgetId>, Option<WidgetId>),
        interaction: UiInteraction,
        active_hover: (Option<WidgetId>, Option<WidgetId>),
        owner: Option<(Option<u64>, WidgetId)>,
        input: (Option<u64>, Option<u64>, u64, u8, bool),
        hits: Vec<Option<WidgetId>>,
        paint: Vec<[f32; 8]>,
        reserved: usize,
    }
    fn composition_snapshot(ui: &Ui) -> CompositionSnapshot {
        let inspection = ui.inspection.as_ref().map(|storage| {
            let mut bytes = vec![0; 512 * 1024];
            if storage.active {
                let n = ui.write_inspection(&mut bytes).unwrap();
                bytes.truncate(n);
            } else {
                bytes.clear();
            }
            (
                storage.labels.len(),
                storage.text.clone(),
                storage.active,
                bytes,
            )
        });
        crate::edit(|s| {
            s.vertices.clear();
            s.batches.clear();
            s.retained.commands.clear();
        });
        ui.paint();
        CompositionSnapshot {
            commands: ui.commands.len(),
            text: ui.text.clone(),
            vertices: ui.vertices,
            clips: ui.clips,
            clip_len: ui.clip_len,
            modal: (ui.modal_start, ui.modal_blocker),
            inspection,
            focus_capture: (ui.state.focused(), ui.state.captured()),
            interaction: ui.interaction(),
            active_hover: (ui.active, ui.hovered),
            owner: ui.owned_pointer.map(|(p, id)| {
                (
                    match p {
                        OwnedPointer::Mouse => None,
                        OwnedPointer::Touch(id) => Some(id),
                    },
                    id,
                )
            }),
            input: (ui.touch, ui.frame, ui.epoch, ui.shift, ui.mouse_capture),
            hits: (0..20)
                .flat_map(|y| {
                    (0..20).map(move |x| {
                        ui.state
                            .hit_test([x as f32 * 10. + 5., y as f32 * 10. + 5.])
                    })
                })
                .collect(),
            paint: crate::read(|s| s.vertices.clone()),
            reserved: ui.reserved_bytes(),
        }
    }
    fn composition_prior() -> Ui {
        reset();
        let mut ui = Ui::new();
        ui.set_inspection_enabled(true);
        begin(&mut ui);
        button(&mut ui, 1, true);
        ui.modal(90).unwrap();
        ui.hud_button(
            2,
            RECT,
            "PRIOR FOCUSED FULL LABEL é",
            true,
            ButtonStyle::default(),
        )
        .unwrap();
        crate::pointer(30., 30., 0., 0.);
        crate::button(1, true);
        ui.finish(true);
        assert_eq!(ui.state.focused(), Some(2));
        assert_eq!(ui.state.captured(), Some(2));
        assert_eq!(ui.interaction().pointer_capture, Some(2));
        ui.push_clip(UiRect {
            min: [10.; 2],
            size: [150.; 2],
        })
        .unwrap();
        ui
    }
    fn composition_part(
        group: &mut UiComposition<'_>,
        part: usize,
        bad: bool,
    ) -> Result<(), UiError> {
        composition_part_at(group, part, RECT, bad)
    }
    fn composition_part_at(
        group: &mut UiComposition<'_>,
        part: usize,
        rect: UiRect,
        bad: bool,
    ) -> Result<(), UiError> {
        let color = if bad {
            Color::new(f32::NAN, 0., 0., 1.)
        } else {
            INK
        };
        let text = TextStyle {
            color,
            ..Default::default()
        };
        let button = ButtonStyle {
            disabled: color,
            ..Default::default()
        };
        match part {
            0 => group.panel(rect, color),
            1 => group.gradient(rect, PANEL, color),
            2 => group.text_fmt(rect, text, format_args!("{}", "Ω")),
            3 => group.text(rect, "TEXT", text),
            4 => group
                .wrapped_text(rect, "WRAPPED", text, LineLimit::Clip(2))
                .map(|_| ()),
            5 => group.label(rect, "FULL LABEL é TO RETAIN", text),
            6 => group.button(106, rect, "OK", true, button),
            7 => group.hud_button(107, rect, "FULL BUTTON LABEL TO RETAIN", true, button),
            8 => group.button_with_options(
                108,
                rect,
                "OK",
                super::super::ButtonOptions {
                    selected: true,
                    ..Default::default()
                },
                button,
            ),
            9 => group.hud_button_with_options(
                109,
                rect,
                "FULL OPTION LABEL TO RETAIN",
                super::super::ButtonOptions {
                    enabled: false,
                    selected: true,
                },
                button,
            ),
            _ => unreachable!(),
        }
    }
    fn composition_parts(group: &mut UiComposition<'_>) -> Result<(), UiError> {
        group.modal(99)?;
        group.push_clip(RECT)?;
        for part in 0..10 {
            composition_part(group, part, false)?;
        }
        group.pop_clip()
    }
    #[test]
    fn journey400_281_every_constituent_command_vertex_and_text_budget_rolls_back() {
        // Measure each production submission's charge, then exhaust the actual
        // arenas at every constituent boundary (no budget fields are patched).
        let mut probe = composition_prior();
        let mut charges = [(0, 0); 10];
        probe
            .compose(|group| {
                for (part, charge) in charges.iter_mut().enumerate() {
                    let before = (group.ui.vertices, group.ui.text.len());
                    composition_part(group, part, false)?;
                    *charge = (group.ui.vertices - before.0, group.ui.text.len() - before.1);
                }
                Ok(())
            })
            .unwrap();
        for budget in 0..3 {
            for fail_at in 0..10 {
                if budget == 2 && charges[fail_at].1 == 0 {
                    continue;
                }
                let mut ui = composition_prior();
                match budget {
                    0 => {
                        while ui.commands.len() < 1024 - fail_at {
                            ui.panel(RECT, PANEL).unwrap();
                        }
                    }
                    1 => {
                        let remaining = charges[..=fail_at].iter().map(|c| c.0).sum::<usize>() - 6;
                        let fill = 196608 - ui.vertices - remaining;
                        ui.text(RECT, &"A".repeat(fill / 126), TextStyle::default())
                            .unwrap();
                        for _ in 0..(fill % 126) / 6 {
                            ui.panel(RECT, PANEL).unwrap();
                        }
                        assert_eq!(196608 - ui.vertices, remaining);
                    }
                    2 => {
                        let remaining = charges[..=fail_at].iter().map(|c| c.1).sum::<usize>() - 1;
                        let fill = "x".repeat(32768 - ui.text.len() - remaining);
                        ui.wrapped_text(RECT, &fill, TextStyle::default(), LineLimit::Clip(0))
                            .unwrap();
                    }
                    _ => unreachable!(),
                }
                let before = composition_snapshot(&ui);
                let mut reached = 0;
                assert_eq!(
                    ui.compose(|group| {
                        group.modal(99)?;
                        group.push_clip(RECT)?;
                        for part in 0..10 {
                            reached = part;
                            composition_part(group, part, false)?;
                        }
                        group.pop_clip()
                    }),
                    Err(UiError::Capacity),
                    "budget {budget}, constituent {fail_at}"
                );
                assert_eq!(reached, fail_at);
                assert_eq!(
                    composition_snapshot(&ui),
                    before,
                    "budget {budget}, constituent {fail_at}"
                );
                assert_eq!(ui.ownership_hit([30.; 2]), Some(2));
            }
        }
    }
    #[test]
    fn journey400_281_invalid_colors_layout_and_duplicate_ids_restore_everything() {
        for fail_at in 0..10 {
            let mut ui = composition_prior();
            let before = composition_snapshot(&ui);
            assert_eq!(
                ui.compose(|group| {
                    group.modal(99)?;
                    group.push_clip(RECT)?;
                    for part in 0..=fail_at {
                        composition_part(group, part, part == fail_at)?;
                    }
                    Ok(())
                }),
                Err(UiError::InvalidLayout)
            );
            assert_eq!(composition_snapshot(&ui), before);
        }
        for id in [99, 106, 107, 108, 109] {
            let mut ui = composition_prior();
            let before = composition_snapshot(&ui);
            assert_eq!(
                ui.compose(|group| {
                    composition_parts(group)?;
                    group.hud_button(id, RECT, "DUPLICATE", true, ButtonStyle::default())
                }),
                Err(UiError::DuplicateId)
            );
            assert_eq!(composition_snapshot(&ui), before);
        }
        for fail_at in 0..10 {
            for bad_rect in [
                UiRect {
                    min: [f32::INFINITY, 0.],
                    ..RECT
                },
                UiRect {
                    size: [-1., 10.],
                    ..RECT
                },
            ] {
                let mut ui = composition_prior();
                let before = composition_snapshot(&ui);
                assert_eq!(
                    ui.compose(|group| {
                        group.modal(99)?;
                        for part in 0..=fail_at {
                            composition_part_at(
                                group,
                                part,
                                if part == fail_at { bad_rect } else { RECT },
                                false,
                            )?;
                        }
                        Ok(())
                    }),
                    Err(UiError::InvalidLayout)
                );
                assert_eq!(composition_snapshot(&ui), before);
            }
        }
        // Each button variant rejects the current blocker ID before adding a
        // command, and rolls back the commands accepted earlier in the group.
        for fail_at in 6..10 {
            let mut ui = composition_prior();
            let before = composition_snapshot(&ui);
            let mut reached = 0;
            assert_eq!(
                ui.compose(|group| {
                    group.modal((100 + fail_at) as u64)?;
                    for part in 0..=fail_at {
                        reached = part;
                        composition_part(group, part, false)?;
                    }
                    Ok(())
                }),
                Err(UiError::DuplicateId)
            );
            assert_eq!(reached, fail_at);
            assert_eq!(composition_snapshot(&ui), before);
        }
        let mut ui = composition_prior();
        let before = composition_snapshot(&ui);
        // Ordinary duplicate against an existing scope, without modal replacement.
        assert_eq!(
            ui.compose(|group| {
                group.panel(RECT, PANEL)?;
                group.button(2, RECT, "DUPLICATE", true, ButtonStyle::default())
            }),
            Err(UiError::DuplicateId)
        );
        assert_eq!(composition_snapshot(&ui), before);
    }
    #[test]
    fn journey400_281_inspection_misses_modal_replacement_and_widget_capacity_restore() {
        let mut ui = composition_prior();
        let before = composition_snapshot(&ui);
        let huge = "é".repeat(16385);
        let full = "x".repeat(32768);
        assert_eq!(
            ui.compose(|group| {
                // Repeated destructive modal changes cannot accumulate an undo log.
                for id in 200..1400 {
                    group.modal(id)?;
                }
                group.label(RECT, &full, TextStyle::default())?;
                group.label(RECT, &huge, TextStyle::default())?; // Metadata-only miss.
                assert_eq!(
                    group.ui.inspection.as_ref().unwrap().text.len(),
                    before.inspection.as_ref().unwrap().1.len()
                ); // Full cannot fit either.
                group.modal(500)?;
                group.button(500, RECT, "BAD", true, ButtonStyle::default())
            }),
            Err(UiError::DuplicateId)
        );
        assert_eq!(composition_snapshot(&ui), before);
        ui.label(RECT, "NEW SHORTENED LABEL", TextStyle::default())
            .unwrap();
        assert_eq!(
            ui.inspect().unwrap().last().unwrap().label,
            UiLabel::Full("NEW SHORTENED LABEL")
        );
        // At widget capacity there is still one command slot: the modal blocker
        // uses a registration without using a paint command.
        let mut ui = composition_prior();
        begin(&mut ui);
        ui.modal(99).unwrap();
        for id in 1000..2022 {
            ui.button(id, RECT, "", true, ButtonStyle::default())
                .unwrap();
        }
        let before = composition_snapshot(&ui);
        assert_eq!(
            ui.compose(|group| {
                group.button(4000, RECT, "", true, ButtonStyle::default())?;
                assert_eq!(group.ui.commands.len(), 1023);
                let result = group.button(4001, RECT, "", true, ButtonStyle::default());
                assert_eq!(result, Err(UiError::Capacity));
                assert_eq!(group.ui.commands.len(), 1023); // Command budget still has room.
                result
            }),
            Err(UiError::Capacity)
        );
        assert_eq!(composition_snapshot(&ui), before);
        begin(&mut ui);
        assert_eq!(ui.inspect().unwrap().len(), 0);
        ui.compose(composition_parts).unwrap();
        assert!(ui.state.hit_test([30.; 2]).is_none()); // Top option is disabled.
        assert_eq!(ui.ownership_hit([30.; 2]), Some(109));
        assert_eq!(ui.state.hit_test([150.; 2]), Some(99));
    }
    #[test]
    fn journey400_281_clip_balance_nested_fallback_depth_and_sticky_errors() {
        let mut ui = composition_prior();
        let before = composition_snapshot(&ui);
        assert_eq!(
            ui.compose(|group| group.pop_clip()),
            Err(UiError::InvalidLayout)
        );
        assert_eq!(composition_snapshot(&ui), before);
        assert_eq!(
            ui.compose(|group| group.push_clip(RECT)),
            Err(UiError::InvalidLayout)
        );
        assert_eq!(composition_snapshot(&ui), before);
        assert_eq!(
            ui.compose(|group| {
                for _ in 0..16 {
                    group.push_clip(RECT)?;
                }
                Ok(())
            }),
            Err(UiError::Capacity)
        );
        assert_eq!(composition_snapshot(&ui), before);
        assert_eq!(
            ui.compose(|group| {
                group.push_clip(UiRect {
                    size: [f32::NAN, 1.],
                    ..RECT
                })
            }),
            Err(UiError::InvalidLayout)
        );
        assert_eq!(composition_snapshot(&ui), before);
        assert_eq!(
            ui.compose(|group| {
                let _ = group.button(2, RECT, "BAD", true, ButtonStyle::default());
                assert_eq!(group.panel(RECT, PANEL), Err(UiError::DuplicateId));
                let mut called = false;
                assert_eq!(
                    group.compose(|_| {
                        called = true;
                        Ok(())
                    }),
                    Err(UiError::DuplicateId)
                );
                assert!(!called);
                Ok(()) // Ignoring errors still rejects the group.
            }),
            Err(UiError::DuplicateId)
        );
        assert_eq!(composition_snapshot(&ui), before);
        // Successful inner work is still rolled back by outer callback error.
        assert_eq!(
            ui.compose(|group| {
                group.compose(composition_parts)?;
                Err::<(), _>(UiError::InvalidLayout)
            }),
            Err(UiError::InvalidLayout)
        );
        assert_eq!(composition_snapshot(&ui), before);
        // A child cannot pop its parent's clip; the parent can handle rejection.
        assert_eq!(
            ui.compose(|group| {
                group.push_clip(RECT)?;
                assert_eq!(
                    group.compose(|inner| {
                        inner.modal(100)?;
                        inner.pop_clip()
                    }),
                    Err(UiError::InvalidLayout)
                );
                assert_eq!(group.ui.modal_blocker, Some(90));
                group.label(RECT, "READABLE FALLBACK", TextStyle::default())?;
                group.pop_clip()?;
                Ok(42)
            }),
            Ok(42)
        );
        assert_eq!(ui.state.captured(), Some(2));
        assert_eq!(ui.commands.len(), before.commands + 1);
        fn nest(group: &mut UiComposition<'_>, depth: usize) -> Result<(), UiError> {
            group.panel(RECT, PANEL)?;
            if depth == 16 {
                let mut called = false;
                assert_eq!(
                    group.compose(|_| {
                        called = true;
                        Ok(())
                    }),
                    Err(UiError::Capacity)
                );
                assert!(!called);
                Ok(())
            } else {
                group.compose(|inner| nest(inner, depth + 1))
            }
        }
        ui.compose(|group| nest(group, 1)).unwrap();
        assert_eq!(ui.commands.len(), before.commands + 17);
        // Propagation instead of local handling rejects every enclosing group.
        fn overflow(group: &mut UiComposition<'_>) -> Result<(), UiError> {
            group.panel(RECT, PANEL)?;
            group.compose(overflow)
        }
        let before = composition_snapshot(&ui);
        assert_eq!(ui.compose(overflow), Err(UiError::Capacity));
        assert_eq!(composition_snapshot(&ui), before);
        // A callback error is retained even if a newly pushed clip is open.
        assert_eq!(
            ui.compose(|group| {
                group.push_clip(RECT)?;
                Err::<(), _>(UiError::Capacity)
            }),
            Err(UiError::Capacity)
        );
        assert_eq!(composition_snapshot(&ui), before);
    }
    #[test]
    fn journey400_281_unwinding_and_release_preserve_prior_capture() {
        let mut ui = composition_prior();
        let before = composition_snapshot(&ui);
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<(), UiError> = ui.compose(|group| {
                composition_parts(group)?;
                panic!("injected callback panic")
            });
        }));
        assert!(panic.is_err());
        assert_eq!(composition_snapshot(&ui), before);
        // Actual host input is still delivered once, after rollback. The prior
        // owner can release/activate and rejected widgets never steal its tail.
        next();
        crate::button(1, false);
        ui.finish(true);
        assert!(ui.activated(2));
        assert_eq!(ui.interaction().pointer_activated, Some(2));
        assert_eq!(ui.interaction().pointer_capture, None);
        ui.state.end_frame();
        assert_eq!(ui.focused(), Some(2));
    }
    #[test]
    fn journey400_281_inactive_inspection_empty_groups_and_reset_preserve_defaults() {
        reset();
        let mut ui = Ui::new();
        let reserved = ui.reserved_bytes();
        begin(&mut ui);
        assert_eq!(ui.compose(|_| Ok(7)), Ok(7));
        assert!(ui.commands.is_empty());
        assert_eq!(ui.reserved_bytes(), reserved);
        button(&mut ui, 1, true);
        // Inspection configured after begin is intentionally inactive until the
        // next begin. Composition exposes no configuration/lifecycle methods.
        ui.set_inspection_enabled(true);
        let before = composition_snapshot(&ui);
        assert!(ui.inspect().is_none());
        assert_eq!(
            ui.compose(|group| {
                composition_parts(group)?;
                Err::<(), _>(UiError::Capacity)
            }),
            Err(UiError::Capacity)
        );
        assert_eq!(composition_snapshot(&ui), before);
        assert!(!ui.inspection.as_ref().unwrap().active);
        begin(&mut ui);
        assert!(ui.inspection.as_ref().unwrap().active);
        ui.compose(composition_parts).unwrap();
        assert_eq!(ui.inspect().unwrap().len(), 11); // Commands plus blocker.
        begin(&mut ui);
        assert_eq!(ui.inspect().unwrap().len(), 0);
        assert_eq!(ui.modal_blocker, None);
        assert_eq!(ui.clip_len, 1);
        assert_eq!(ui.vertices, 0);
        ui.compose(|group| group.button(106, RECT, "NEW", true, ButtonStyle::default()))
            .unwrap();
        assert_eq!(
            ui.inspect().unwrap().next().unwrap().label,
            UiLabel::Full("NEW")
        );
        ui.set_inspection_enabled(false);
        assert_eq!(ui.reserved_bytes(), reserved);
    }
    #[test]
    fn journey400_280_inspection_preserves_exact_paint_hit_and_input() {
        let run = |inspect: bool| {
            reset();
            crate::edit(|s| {
                s.w = 400;
                s.h = 240;
            });
            let mut ui = Ui::new();
            ui.set_inspection_enabled(inspect);
            let viewport = Viewport::fit([400., 240.], [100.; 2], [20., 10., 20., 30.]).unwrap();
            ui.begin(viewport);
            let clip = UiRect {
                min: [20., 30.],
                size: [10., 20.],
            };
            ui.push_clip(clip).unwrap();
            ui.hud_button(
                9,
                UiRect {
                    min: [10., 20.],
                    size: [80., 40.],
                },
                "RESUME EXPEDITION é",
                true,
                ButtonStyle::default(),
            )
            .unwrap();
            // Project a synthetic mouse press/release through this scaled safe area.
            let pixel: [f32; 2] = std::array::from_fn(|i| {
                viewport.pixels().min[i] + [25., 40.][i] * viewport.scale()
            });
            crate::pointer(pixel[0], pixel[1], 0., 0.);
            crate::button(1, true);
            crate::button(1, false);
            ui.finish(true);
            assert!(ui.activated(9));
            assert_eq!(ui.state.hit_test([25., 40.]), Some(9));
            for point in [[19.999, 40.], [30., 40.], [25., 50.]] {
                assert_eq!(ui.state.hit_test(point), None);
            }
            let before = (
                ui.state.focused(),
                ui.state.captured(),
                ui.interaction(),
                ui.active,
                ui.text.clone(),
                ui.commands.len(),
                ui.vertices,
                crate::read(|s| s.input.events().len()),
            );
            if inspect {
                for _ in 0..3 {
                    let element = ui.inspect().unwrap().next().unwrap();
                    assert_eq!(element.visible_bounds, Some(clip));
                    assert_eq!(element.label, UiLabel::Full("RESUME EXPEDITION é"));
                    assert!(element.enabled && element.focused && element.interaction_registered);
                    let mut bytes = [0; 2048];
                    ui.write_inspection(&mut bytes).unwrap();
                }
            }
            assert_eq!(
                before,
                (
                    ui.state.focused(),
                    ui.state.captured(),
                    ui.interaction(),
                    ui.active,
                    ui.text.clone(),
                    ui.commands.len(),
                    ui.vertices,
                    crate::read(|s| s.input.events().len())
                )
            );
            ui.paint();
            crate::read(|s| {
                // First six vertices are the actual button fill; compare all
                // corners to separately specified logical clipped geometry.
                let points = [
                    [20., 30.],
                    [30., 30.],
                    [30., 50.],
                    [20., 30.],
                    [30., 50.],
                    [20., 50.],
                ];
                for (vertex, point) in s.vertices[..6].iter().zip(points) {
                    let px = viewport.pixels().min[0] + point[0] * viewport.scale();
                    let py = viewport.pixels().min[1] + point[1] * viewport.scale();
                    assert_eq!(vertex[0], px / 400. * 2. - 1.);
                    assert_eq!(vertex[1], 1. - py / 240. * 2.);
                }
                (
                    s.vertices.clone(),
                    ui.interaction(),
                    ui.focused(),
                    ui.activated(9),
                )
            })
        };
        assert_eq!(run(false), run(true));
    }

    #[test]
    fn journey400_280_opt_out_and_label_capacity_do_not_change_submission() {
        reset();
        let mut ui = Ui::new();
        begin(&mut ui);
        let capacities = (
            ui.commands.capacity(),
            ui.text.capacity(),
            ui.state.capacity(),
        );
        let default_reserved = ui.reserved_bytes();
        let huge = "é".repeat(50_000);
        ui.hud_button(1, RECT, &huge, true, ButtonStyle::default())
            .unwrap();
        assert!(ui.inspection.is_none());
        assert!(ui.inspect().is_none());
        assert_eq!(ui.text.len(), 5); // One 2-byte scalar followed by ellipsis.
        ui.set_inspection_enabled(true);
        assert!(
            ui.inspect().is_none(),
            "cannot reconstruct already shortened copy"
        );
        begin(&mut ui);
        let full = "é".repeat(16384); // Exactly 32 KiB, all scalar boundaries valid.
        ui.label(RECT, &full, TextStyle::default()).unwrap();
        ui.hud_button(
            2,
            RECT,
            "overflow after full arena",
            true,
            ButtonStyle::default(),
        )
        .unwrap();
        ui.hud_button(3, RECT, &huge, false, ButtonStyle::default())
            .unwrap();
        let elements: Vec<_> = ui.inspect().unwrap().collect();
        assert_eq!(elements[0].label, UiLabel::Full(&full));
        assert_eq!(
            elements[1].label,
            UiLabel::Unavailable { required_bytes: 25 }
        );
        assert_eq!(
            elements[2].label,
            UiLabel::Unavailable {
                required_bytes: huge.len()
            }
        );
        assert!(elements[1].painted_label.ends_with("..."));
        let storage = ui.inspection.as_ref().unwrap();
        assert_eq!((storage.text.len(), storage.labels.len()), (32768, 3));
        assert_eq!(
            (storage.text.capacity(), storage.labels.capacity()),
            (32768, 1024)
        );
        assert_eq!(
            capacities,
            (
                ui.commands.capacity(),
                ui.text.capacity(),
                ui.state.capacity()
            )
        );
        ui.set_inspection_enabled(false);
        assert!(ui.inspect().is_none());
        assert_eq!(ui.reserved_bytes(), default_reserved);
        assert_eq!(
            ui.commands.len(),
            3,
            "disabling preserves composed paint/input"
        );
    }

    #[test]
    fn journey400_280_failed_commands_leave_no_inspection_or_stale_copy() {
        reset();
        let mut ui = Ui::new();
        ui.set_inspection_enabled(true);
        begin(&mut ui);
        ui.hud_button(
            1,
            RECT,
            "long original source",
            true,
            ButtonStyle::default(),
        )
        .unwrap();
        let before = (
            ui.commands.len(),
            ui.text.len(),
            ui.vertices,
            ui.inspection.as_ref().unwrap().labels.len(),
            ui.inspection.as_ref().unwrap().text.len(),
        );
        assert_eq!(
            ui.hud_button(
                1,
                RECT,
                "another original source",
                true,
                ButtonStyle::default()
            ),
            Err(UiError::DuplicateId)
        );
        assert_eq!(
            ui.label(
                RECT,
                "bad",
                TextStyle {
                    size: f32::NAN,
                    ..Default::default()
                }
            ),
            Err(UiError::InvalidLayout)
        );
        assert_eq!(
            before,
            (
                ui.commands.len(),
                ui.text.len(),
                ui.vertices,
                ui.inspection.as_ref().unwrap().labels.len(),
                ui.inspection.as_ref().unwrap().text.len()
            )
        );
        begin(&mut ui);
        assert_eq!(ui.inspect().unwrap().len(), 0);
        let all_text = "x".repeat(32768);
        ui.wrapped_text(RECT, &all_text, TextStyle::default(), LineLimit::Clip(0))
            .unwrap();
        assert_eq!(
            ui.label(RECT, "cannot append", TextStyle::default()),
            Err(UiError::Capacity)
        );
        assert_eq!(
            ui.button(2, RECT, "x", true, ButtonStyle::default()),
            Err(UiError::Capacity)
        );
        assert_eq!(ui.inspect().unwrap().len(), 1);
        assert_eq!(
            ui.inspect().unwrap().next().unwrap().label,
            UiLabel::Full(&all_text)
        );
        assert!(ui.inspection.as_ref().unwrap().labels.is_empty());
        begin(&mut ui);
        for _ in 0..1024 {
            ui.label(RECT, "", TextStyle::default()).unwrap();
        }
        assert_eq!(
            ui.label(RECT, "overflow", TextStyle::default()),
            Err(UiError::Capacity)
        );
        assert_eq!(ui.inspect().unwrap().count(), 1024);
        begin(&mut ui);
        ui.label(RECT, "fresh original source", TextStyle::default())
            .unwrap();
        assert_eq!(
            ui.inspect().unwrap().next().unwrap().label,
            UiLabel::Full("fresh original source")
        );
    }

    #[test]
    fn journey400_280_modal_disabled_clipped_and_stale_ids_are_explicit() {
        reset();
        let mut ui = Ui::new();
        ui.set_inspection_enabled(true);
        begin(&mut ui);
        button(&mut ui, 1, true);
        ui.state.focus_next(false);
        ui.modal(90).unwrap();
        button(&mut ui, 2, true);
        ui.modal(91).unwrap();
        button(&mut ui, 1, false); // Reused ID in latest modal scope.
        ui.push_clip(UiRect {
            min: [180.; 2],
            size: [10.; 2],
        })
        .unwrap();
        button(&mut ui, 3, true);
        ui.pop_clip();
        ui.panel(RECT, INK).unwrap();
        ui.gradient(RECT, INK, PANEL).unwrap();
        ui.finish(true);
        let mut elements = ui.inspect().unwrap();
        assert_eq!(elements.len(), 7);
        let lower = elements.next().unwrap();
        assert_eq!(lower.id, Some(1));
        assert!(!lower.interaction_registered && !lower.focused);
        assert!(!elements.next().unwrap().interaction_registered);
        let blocker = elements.next().unwrap();
        assert_eq!(blocker.role, UiRole::ModalBlocker);
        assert_eq!(blocker.id, Some(91));
        assert!(blocker.interaction_registered && !blocker.focused);
        let disabled = elements.next().unwrap();
        assert!(!disabled.enabled && !disabled.focused);
        assert!(disabled.interaction_registered && disabled.visible_bounds.is_some());
        let clipped = elements.next().unwrap();
        assert!(clipped.enabled && clipped.interaction_registered && !clipped.focused);
        assert_eq!(clipped.visible_bounds, None);
        assert_eq!(elements.next().unwrap().role, UiRole::Panel);
        assert_eq!(elements.next().unwrap().role, UiRole::Gradient);
        assert_eq!(elements.len(), 0);
        assert_eq!(elements.next(), None);
        assert_eq!(elements.next(), None);
        next();
        begin(&mut ui);
        assert_eq!(ui.inspect().unwrap().next(), None);
        ui.modal(99).unwrap(); // Blocker at end, without any paint command.
        assert_eq!(ui.inspect().unwrap().len(), 1);
        assert_eq!(ui.inspect().unwrap().next().unwrap().id, Some(99));
        next();
        ui.begin(Viewport::responsive([0., 0.], [0.; 4]).unwrap());
        ui.button(5, RECT, "zero viewport", true, ButtonStyle::default())
            .unwrap();
        assert_eq!(ui.inspect().unwrap().next().unwrap().visible_bounds, None);
    }

    #[test]
    fn journey400_279_ui_first_queue_survives_zero_ticks_without_stealing_movement() {
        use crate::input::{ActionKind, Binding, PlayerInput, host_frame};
        use ferravex_core::session::GameSession;
        reset();
        let mut ui = Ui::new();
        let mut player = PlayerInput::transient("ownership-ticks");
        for (name, code) in [("move", 100), ("jump", 32)] {
            player
                .action(
                    name,
                    name,
                    ActionKind::Button,
                    vec![Binding::digital(InputCode::Key(code))],
                )
                .unwrap();
        }
        // Enable the existing gameplay context before injecting host input.
        host_frame(0., 200, 200, || player.sample(true));
        let mut session = GameSession::try_new(64., 4, true).unwrap();
        let mut pending = None;
        let mut executed = Vec::new();
        let mut jumps = 0;
        crate::key(100, true);
        crate::key(32, true);
        crate::key(32, false);
        for (position, elapsed, expected_consumed, expected_ticks) in [
            ([40., 40.], 0., true, 0),
            ([150., 150.], 0., false, 0), // Queue full: reject this drop.
            ([150., 150.], 1. / 64., false, 1), // Execute the queued buy.
            ([150., 150.], 2. / 64., false, 2), // Execute one drop, two ticks.
        ] {
            crate::pointer(position[0], position[1], 0., 0.);
            crate::button(1, true);
            crate::button(1, false);
            host_frame(elapsed as f32, 200, 200, || {
                player.sample(true);
                begin(&mut ui);
                button(&mut ui, 1, true);
                ui.finish_player(&mut player, true);
                let ownership = ui.interaction();
                assert_eq!(ownership.pointer_consumed, expected_consumed);
                assert_eq!(ownership.pointer_activated, expected_consumed.then_some(1));
                assert_eq!(ownership.pointer_capture, None);
                assert!(!ownership.input_blocked);
                assert!(player.held("move"));
                assert_eq!(ui.interaction(), ownership, "inspection is read only");
                ui.finish_player(&mut player, true);
                assert_eq!(
                    ui.interaction(),
                    ownership,
                    "one journal delivery per frame"
                );
                let request = if ui.activated(1) {
                    Some("buy")
                } else if !ownership.pointer_consumed {
                    Some("drop")
                } else {
                    None
                };
                if pending.is_none() {
                    pending = request;
                }
                let frame = session.advance(elapsed);
                assert_eq!(frame.steps, expected_ticks);
                for _ in frame.ticks() {
                    let [movement, jump] = player.take_tick(["move", "jump"]);
                    assert_eq!(movement.value, 1.);
                    jumps += usize::from(jump.pressed);
                    if let Some(command) = pending.take() {
                        executed.push(command);
                    }
                }
                ui.paint();
            });
        }
        assert_eq!(executed, ["buy", "drop"]);
        assert_eq!(
            jumps, 1,
            "unrelated pending edge survives both zero-tick frames"
        );
        assert!(player.held("move"));
        assert_eq!(pending, None);
    }

    #[test]
    fn journey400_279_capture_dragout_and_world_origin_follow_event_positions() {
        reset();
        let mut ui = Ui::new();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::pointer(40., 40., 0., 0.);
        crate::button(1, true);
        ui.finish(true);
        assert!(ui.interaction().pointer_consumed);
        assert_eq!(ui.interaction().pointer_capture, Some(1));
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::pointer(150., 150., 110., 110.);
        ui.finish(true);
        assert!(
            ui.interaction().pointer_consumed,
            "held drag stays UI-owned"
        );
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::button(1, false);
        ui.finish(true);
        assert!(
            ui.interaction().pointer_consumed,
            "outside release stays UI-owned"
        );
        assert_eq!(ui.interaction().pointer_capture, None);
        assert_eq!(ui.interaction().pointer_activated, None);
        assert!(!ui.activated(1));
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::button(1, true); // World press.
        crate::pointer(40., 40., -110., -110.);
        crate::button(1, false);
        ui.finish(true);
        assert_eq!(ui.interaction(), UiInteraction::default());
        assert!(!ui.activated(1));
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        // A UI tap followed by a world tap is deliberately conservative.
        crate::button(1, true);
        crate::button(1, false);
        crate::pointer(150., 150., 110., 110.);
        crate::button(1, true);
        crate::button(1, false);
        ui.finish(true);
        assert!(ui.interaction().pointer_consumed);
        assert_eq!(ui.interaction().pointer_activated, Some(1));
    }

    #[test]
    fn journey400_279_disabled_occluded_clipped_and_paint_only_policy() {
        for (top_enabled, clipped, modal, paint_only, position, owner, active) in [
            (true, false, false, false, [40., 40.], Some(2), Some(2)),
            (false, false, false, false, [40., 40.], Some(2), None),
            (false, true, false, false, [40., 40.], Some(1), Some(1)),
            (true, false, true, false, [150., 150.], Some(99), Some(99)),
            (true, false, false, true, [150., 150.], None, None),
            (true, false, false, false, [200., 40.], None, None),
        ] {
            reset();
            let mut ui = Ui::new();
            begin(&mut ui);
            button(&mut ui, 1, true);
            if clipped {
                ui.push_clip(UiRect {
                    min: [120.; 2],
                    size: [20.; 2],
                })
                .unwrap();
            }
            button(&mut ui, 2, top_enabled);
            if clipped {
                ui.pop_clip();
            }
            if modal {
                ui.modal(99).unwrap();
            }
            if paint_only {
                ui.panel(ui.viewport.bounds(), PANEL).unwrap();
            }
            crate::pointer(position[0], position[1], 0., 0.);
            crate::button(1, true);
            ui.finish(true);
            assert_eq!(ui.interaction().pointer_capture, owner);
            next();
            // Keep the submitted frame geometry; next host frame releases it.
            crate::button(1, false);
            ui.finish(true);
            assert_eq!(ui.interaction().pointer_consumed, owner.is_some());
            assert_eq!(ui.interaction().pointer_activated, active);
            assert_eq!(ui.interaction().pointer_capture, None);
        }
    }

    #[test]
    fn journey400_279_modal_close_cancel_disable_remove_and_release_tail() {
        // Exercise both primary pointer sources through the real journal.
        for touch in [false, true] {
            for mode in 0..7 {
                reset();
                let mut ui = Ui::new();
                begin(&mut ui);
                if mode == 4 {
                    ui.modal(1).unwrap();
                } else {
                    button(&mut ui, 1, true);
                }
                if touch {
                    crate::touch(7, 0, 40., 40.);
                } else {
                    crate::pointer(40., 40., 0., 0.);
                    crate::button(1, true);
                }
                ui.finish(true);
                assert_eq!(ui.interaction().pointer_capture, Some(1));
                next();
                begin(&mut ui);
                if !matches!(mode, 2 | 4) {
                    button(&mut ui, 1, mode != 3);
                }
                if mode == 0 {
                    ui.cancel();
                }
                if mode == 5 {
                    crate::edit(|s| s.controls_open = true);
                }
                if mode == 6 {
                    crate::key(0xff1b, true);
                }
                ui.finish(mode != 1);
                assert!(ui.interaction().pointer_consumed);
                assert_eq!(ui.interaction().pointer_capture, Some(1));
                assert_eq!(ui.interaction().input_blocked, mode == 5);
                next();
                begin(&mut ui);
                // Close the modal/editor or re-enable UI before the release.
                crate::edit(|s| s.controls_open = false);
                if touch {
                    crate::touch(7, 2, 40., 40.);
                } else {
                    crate::button(1, false);
                }
                ui.finish(true);
                assert!(
                    ui.interaction().pointer_consumed,
                    "touch={touch}, mode={mode}"
                );
                assert_eq!(ui.interaction().pointer_capture, None);
                assert_eq!(ui.interaction().pointer_activated, None);
                next();
                begin(&mut ui);
                ui.finish(true);
                assert_eq!(ui.interaction(), UiInteraction::default());
            }
        }
    }

    #[test]
    fn journey400_279_touch_cancel_and_other_touch_do_not_release_owner() {
        reset();
        let mut ui = Ui::new();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::touch(7, 0, 40., 40.);
        crate::touch(8, 0, 40., 40.);
        crate::touch(8, 2, 40., 40.);
        ui.finish(true);
        assert_eq!(ui.interaction().pointer_capture, Some(1));
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::touch(7, 3, 40., 40.);
        ui.finish(true);
        assert!(ui.interaction().pointer_consumed);
        assert_eq!(ui.interaction().pointer_capture, None);
        assert_eq!(ui.interaction().pointer_activated, None);
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::touch(9, 0, 150., 150.);
        crate::touch(9, 2, 40., 40.);
        ui.finish(true);
        assert_eq!(ui.interaction(), UiInteraction::default());
    }

    #[test]
    fn journey400_279_cancelled_tail_cannot_be_stolen_by_a_second_pointer() {
        for mouse in [false, true] {
            reset();
            let mut ui = Ui::new();
            begin(&mut ui);
            button(&mut ui, 1, true);
            crate::touch(7, 0, 40., 40.);
            ui.finish(true);
            ui.cancel();
            next();
            begin(&mut ui);
            button(&mut ui, 1, true);
            if mouse {
                crate::pointer(40., 40., 0., 0.);
                crate::button(1, true);
            } else {
                crate::touch(8, 0, 40., 40.);
            }
            crate::touch(7, 2, 40., 40.);
            ui.finish(true);
            assert!(ui.interaction().pointer_consumed);
            assert_eq!(ui.interaction().pointer_capture, None);
            assert_eq!(ui.interaction().pointer_activated, None);
            next();
            begin(&mut ui);
            button(&mut ui, 1, true);
            if mouse {
                crate::button(1, false);
            } else {
                crate::touch(8, 2, 40., 40.);
            }
            ui.finish(true);
            assert_eq!(ui.interaction(), UiInteraction::default());
            assert!(!ui.activated(1));
        }
    }

    #[test]
    fn journey400_279_confirm_sources_are_explicit_and_unrelated_edges_remain() {
        use crate::input::{ActionKind, Binding, PlayerInput};
        reset();
        let mut player = PlayerInput::transient("ownership-confirm");
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
        let mut ui = Ui::new();
        ui.set_auto_focus(false);
        player.sample(true);
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::key(106, true);
        crate::key(106, false);
        player.sample(true);
        ui.finish_player(&mut player, true);
        assert_eq!(
            ui.interaction(),
            UiInteraction::default(),
            "no focus, no acceptance"
        );
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::key(0xff09, true);
        crate::key(0xff09, false);
        crate::key(106, true);
        crate::key(106, false);
        crate::key(32, true);
        crate::key(32, false);
        player.sample(true);
        ui.finish_player(&mut player, true);
        assert_eq!(ui.interaction().player_confirmed, Some(1));
        assert_eq!(ui.interaction().keyboard_confirmed, None);
        assert_eq!(ui.interaction().pointer_activated, None);
        assert!(!ui.interaction().pointer_consumed);
        assert!(
            !player.pressed("confirm"),
            "legacy global-confirm consumption"
        );
        assert!(player.take_tick(["jump"])[0].pressed);
        let result = ui.interaction();
        begin(&mut ui); // A second layout/finish must not replay the journal.
        button(&mut ui, 1, true);
        ui.finish_player(&mut player, true);
        assert_eq!(ui.interaction(), result);
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::key(0xff0d, true);
        crate::key(0xff0d, false);
        ui.finish(true);
        assert_eq!(ui.interaction().keyboard_confirmed, Some(1));
        assert_eq!(ui.interaction().player_confirmed, None);
        next();
        crate::edit(|s| s.input.pad_connected(1, true));
        begin(&mut ui);
        button(&mut ui, 1, true);
        ui.finish_with_gamepad(true, 1); // First sample establishes pad epoch.
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::edit(|s| s.input.pad_button(1, 0, 1.));
        ui.finish_with_gamepad(true, 1);
        assert_eq!(ui.interaction().gamepad_confirmed, Some(1));
        assert_eq!(ui.interaction().keyboard_confirmed, None);
        assert_eq!(ui.interaction().pointer_capture, None);
    }

    #[test]
    fn journey400_279_focus_overflow_and_blocked_ui_fail_closed_then_reset() {
        for overflow in [false, true] {
            reset();
            let mut ui = Ui::new();
            begin(&mut ui);
            button(&mut ui, 1, true);
            crate::pointer(40., 40., 0., 0.);
            crate::button(1, true);
            ui.finish(true);
            next();
            begin(&mut ui);
            button(&mut ui, 1, true);
            crate::edit(|s| {
                if overflow {
                    for _ in 0..=crate::input::MAX_EVENTS {
                        s.input
                            .push(PlatformEvent::PointerPosition(crate::Vec2::ZERO));
                    }
                    assert!(s.input.overflow);
                } else {
                    s.input.focus(false);
                }
            });
            ui.finish(true);
            assert!(ui.interaction().input_blocked);
            assert!(ui.interaction().pointer_consumed);
            assert_eq!(ui.interaction().pointer_capture, None);
            assert_eq!(ui.interaction().pointer_activated, None);
            next();
            begin(&mut ui);
            button(&mut ui, 1, true);
            crate::edit(|s| s.input.focus(true));
            crate::button(1, false);
            ui.finish(true);
            assert_eq!(ui.interaction(), UiInteraction::default());
            assert!(!ui.activated(1));
        }
        reset();
        let mut ui = Ui::new();
        begin(&mut ui);
        button(&mut ui, 1, true);
        crate::pointer(40., 40., 0., 0.);
        crate::button(1, true);
        crate::button(1, false);
        ui.finish(false);
        assert_eq!(
            ui.interaction(),
            UiInteraction::default(),
            "UI disabled, world allowed"
        );
        next();
        begin(&mut ui);
        crate::edit(|s| s.controls_open = true);
        ui.finish(true);
        assert!(ui.interaction().input_blocked);
        crate::edit(|s| s.controls_open = false);
        ui.finish(true);
        assert!(
            ui.interaction().input_blocked,
            "same-frame close cannot leak earlier input"
        );
        next();
        begin(&mut ui);
        ui.finish(true);
        assert_eq!(ui.interaction(), UiInteraction::default());
    }

    #[test]
    fn journey400_279_ownership_uses_the_same_scaled_safe_area_clip_as_widgets() {
        reset();
        let mut ui = Ui::new();
        let viewport = Viewport::fit([400., 240.], [200.; 2], [12., 8., 12., 8.]).unwrap();
        ui.begin(viewport);
        ui.push_clip(UiRect {
            min: [30.; 2],
            size: [20.; 2],
        })
        .unwrap();
        button(&mut ui, 1, true);
        ui.pop_clip();
        // [40, 40] is visible, [25, 40] lies inside the button but outside its clip.
        for (logical, expected) in [([40., 40.], Some(1)), ([25., 40.], None)] {
            let pixels: [f32; 2] = std::array::from_fn(|axis| {
                viewport.pixels().min[axis] + logical[axis] * viewport.scale()
            });
            crate::pointer(pixels[0], pixels[1], 0., 0.);
            crate::button(1, true);
            crate::button(1, false);
            ui.finish(true);
            assert_eq!(ui.interaction().pointer_activated, expected);
            assert_eq!(ui.interaction().pointer_consumed, expected.is_some());
            next();
        }
    }

    #[test]
    fn journey400_279_rejected_widgets_never_claim_and_full_frame_stays_bounded() {
        reset();
        let mut ui = Ui::new();
        let reserved = ui.reserved_bytes();
        begin(&mut ui);
        for id in 0..1024 {
            ui.button(id, RECT, "", true, ButtonStyle::default())
                .unwrap();
        }
        let rejected = UiRect {
            min: [140.; 2],
            size: [40.; 2],
        };
        assert_eq!(
            ui.button(1024, rejected, "", true, ButtonStyle::default()),
            Err(UiError::Capacity)
        );
        crate::pointer(150., 150., 0., 0.);
        crate::button(1, true);
        crate::button(1, false);
        ui.finish(true);
        assert_eq!(ui.interaction(), UiInteraction::default());
        next();
        crate::pointer(40., 40., 0., 0.);
        crate::button(1, true);
        crate::button(1, false);
        ui.finish(true);
        assert_eq!(ui.interaction().pointer_activated, Some(1023));
        next();
        begin(&mut ui);
        button(&mut ui, 1, true);
        assert_eq!(
            ui.button(1, rejected, "", true, ButtonStyle::default()),
            Err(UiError::DuplicateId)
        );
        assert_eq!(
            ui.button(
                2,
                rejected,
                "",
                true,
                ButtonStyle {
                    size: f32::NAN,
                    ..Default::default()
                }
            ),
            Err(UiError::InvalidLayout)
        );
        crate::pointer(150., 150., 0., 0.);
        crate::button(1, true);
        crate::button(1, false);
        ui.finish(true);
        assert_eq!(ui.interaction(), UiInteraction::default());
        assert_eq!(ui.reserved_bytes(), reserved);
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
    fn wrapped_help_keeps_words_and_explicit_blank_lines_readable() {
        assert_eq!(
            Lines::new("Five amber gates then land", 12).collect::<Vec<_>>(),
            ["Five amber", "gates then", "land"]
        );
        assert_eq!(
            Lines::new("alpha  beta\n\né日A", 5).collect::<Vec<_>>(),
            ["alpha", "beta", "", "é日A"]
        );
        assert_eq!(
            Lines::new("é日ABC", 2).collect::<Vec<_>>(),
            ["é日", "AB", "C"]
        );
        assert_eq!(Lines::new("AB", 0).collect::<Vec<_>>(), ["A", "B"]);
    }

    fn painted_lines(
        lines: &[&str],
        rect: UiRect,
        clip: UiRect,
        style: TextStyle,
    ) -> Vec<[f32; 8]> {
        let mut vertices = Vec::new();
        let mut painter = Painter {
            vertices: &mut vertices,
            viewport: Viewport::responsive([320., 240.], [0.; 4]).unwrap(),
            screen: [320., 240.],
        };
        for (index, line) in lines.iter().enumerate() {
            let offset = index as f32 * style.size;
            if offset >= rect.size[1] {
                break;
            }
            painter.text(
                UiRect {
                    min: [rect.min[0], rect.min[1] + offset],
                    size: [rect.size[0], rect.size[1] - offset],
                },
                clip,
                line,
                style,
                LineLimit::Unlimited,
            );
        }
        vertices
    }

    #[test]
    fn journey400_273_measure_legacy_and_queued_paint_share_line_geometry() {
        reset();
        crate::edit(|s| {
            s.w = 320;
            s.h = 240;
        });
        for (copy, columns, expected) in [
            (
                "Five amber gates then land",
                12,
                vec!["Five amber", "gates then", "land"],
            ),
            ("alpha  beta\n\né日A", 5, vec!["alpha", "beta", "", "??A"]),
            ("é日ABC", 2, vec!["??", "AB", "C"]),
            ("A\r\nB", 5, vec!["A?", "B"]),
            ("A\t\r B", 2, vec!["A", "B"]),
            ("\nA\n\n", 2, vec!["", "A", ""]),
        ] {
            for size in [8., 18.5] {
                for align in [Align::Left, Align::Center, Align::Right] {
                    for height in [0., size * 1.5, 180.] {
                        next();
                        let rect = UiRect {
                            min: [16., 24.],
                            size: [columns as f32 * (size / 8. * 6.), height],
                        };
                        let clip = UiRect {
                            min: [18., 25.],
                            size: [280., 170.],
                        };
                        let style = TextStyle {
                            size,
                            align,
                            wrap: true,
                            shadow: true,
                            ..Default::default()
                        };
                        let oracle = painted_lines(&expected, rect, clip, style);
                        let mut legacy = Vec::new();
                        Painter {
                            vertices: &mut legacy,
                            viewport: Viewport::responsive([320., 240.], [0.; 4]).unwrap(),
                            screen: [320., 240.],
                        }
                        .text(
                            rect,
                            clip,
                            copy,
                            style,
                            LineLimit::Unlimited,
                        );
                        assert_eq!(legacy, oracle, "legacy: {copy:?} {size} {height}");
                        let mut ui = Ui::new();
                        ui.begin(Viewport::responsive([320., 240.], [0.; 4]).unwrap());
                        ui.push_clip(clip).unwrap();
                        let report = ui
                            .wrapped_text(rect, copy, style, LineLimit::Unlimited)
                            .unwrap();
                        assert_eq!(report.line_count, expected.len());
                        assert_eq!(report.required_height, expected.len() as f32 * size);
                        // Geometric clipping does not pretend to be a line policy.
                        assert_eq!(report.hidden_lines, 0);
                        ui.paint();
                        crate::read(|s| {
                            assert_eq!(s.vertices, oracle, "queued: {copy:?} {size} {height}");
                            assert!(s.vertices.len() <= ui.vertices);
                            for vertex in &s.vertices {
                                let p = [(vertex[0] + 1.) * 160., (1. - vertex[1]) * 120.];
                                let visible = rect.intersect(clip).unwrap();
                                for (axis, coordinate) in p.into_iter().enumerate() {
                                    assert!(coordinate >= visible.min[axis] - 0.001);
                                    assert!(
                                        coordinate
                                            <= visible.min[axis] + visible.size[axis] + 0.001
                                    );
                                }
                            }
                        });
                    }
                }
            }
        }
    }

    #[test]
    fn journey400_273_final_line_marker_and_partial_height_clipping_are_explicit() {
        reset();
        crate::edit(|s| {
            s.w = 320;
            s.h = 240;
        });
        let viewport = Viewport::responsive([320., 240.], [0.; 4]).unwrap();
        let style = TextStyle {
            size: 8.,
            align: Align::Right,
            shadow: true,
            ..Default::default()
        };
        for (limit, expected) in [
            (LineLimit::Unlimited, vec!["??ABC", "DEF", "G"]),
            (LineLimit::Clip(1), vec!["??ABC"]),
            (LineLimit::Ellipsis(1), vec!["??..."]),
            (LineLimit::Ellipsis(2), vec!["??ABC", "DE..."]),
            (LineLimit::Ellipsis(3), vec!["??ABC", "DEF", "G"]),
            (LineLimit::Ellipsis(0), vec![]),
        ] {
            for height in [0., 4., 8., 12., 24.] {
                next();
                let rect = UiRect {
                    min: [16., 24.],
                    size: [30., height],
                };
                let mut ui = Ui::new();
                ui.begin(viewport);
                let report = ui
                    .wrapped_text(rect, "é日ABC DEF\nG", style, limit)
                    .unwrap();
                assert_eq!(report.visible_lines, expected.len());
                assert_eq!(report.hidden_lines, 3 - expected.len());
                assert_eq!(report.required_height, 24.);
                ui.paint();
                let oracle = painted_lines(
                    &expected,
                    rect,
                    rect,
                    TextStyle {
                        wrap: true,
                        ..style
                    },
                );
                crate::read(|s| {
                    assert_eq!(s.vertices, oracle);
                    assert!(s.vertices.len() <= ui.vertices);
                });
            }
        }
        // Legacy non-wrapped copy still shrinks all lines to fit the longest.
        let rect = UiRect {
            min: [16., 24.],
            size: [12., 16.],
        };
        let mut actual = Vec::new();
        Painter {
            vertices: &mut actual,
            viewport,
            screen: [320., 240.],
        }
        .text(rect, rect, "ABCD\nEF", style, LineLimit::Unlimited);
        assert_eq!(
            actual,
            painted_lines(
                &["ABCD", "EF"],
                rect,
                rect,
                TextStyle {
                    size: 4.,
                    wrap: true,
                    ..style
                }
            )
        );
    }

    #[test]
    fn journey400_273_rejections_are_atomic_and_begin_restores_budgets() {
        reset();
        let mut ui = Ui::new();
        begin(&mut ui);
        ui.text(RECT, "kept", TextStyle::default()).unwrap();
        let before = (ui.commands.len(), ui.text.clone(), ui.vertices);
        let huge = "A".repeat(32769);
        for (rect, copy, style, limit, error) in [
            (
                RECT,
                "bad",
                TextStyle {
                    size: f32::NAN,
                    ..Default::default()
                },
                LineLimit::Unlimited,
                UiError::InvalidLayout,
            ),
            (
                RECT,
                "bad",
                TextStyle {
                    color: Color::new(f32::NAN, 0., 0., 1.),
                    ..Default::default()
                },
                LineLimit::Unlimited,
                UiError::InvalidLayout,
            ),
            (
                UiRect {
                    min: [f32::INFINITY, 0.],
                    ..RECT
                },
                "bad",
                TextStyle::default(),
                LineLimit::Clip(0),
                UiError::InvalidLayout,
            ),
            (
                RECT,
                huge.as_str(),
                TextStyle::default(),
                LineLimit::Clip(0),
                UiError::Capacity,
            ),
            (
                RECT,
                &huge[..2000],
                TextStyle::default(),
                LineLimit::Unlimited,
                UiError::Capacity,
            ),
        ] {
            assert_eq!(ui.wrapped_text(rect, copy, style, limit), Err(error));
            assert_eq!((ui.commands.len(), ui.text.clone(), ui.vertices), before);
        }
        let storage = (ui.commands.as_ptr(), ui.text.as_ptr(), ui.reserved_bytes());
        begin(&mut ui);
        // Full arena accepted for an explicit zero-line policy; no glyph budget.
        let report = ui
            .wrapped_text(
                RECT,
                &huge[..32768],
                TextStyle::default(),
                LineLimit::Clip(0),
            )
            .unwrap();
        assert!(report.is_truncated());
        assert_eq!(ui.vertices, 0);
        assert_eq!(ui.text.len(), 32768);
        assert_eq!(
            ui.wrapped_text(RECT, "A", TextStyle::default(), LineLimit::Clip(0)),
            Err(UiError::Capacity)
        );
        assert_eq!(ui.text.len(), 32768);
        begin(&mut ui);
        for _ in 0..1024 {
            ui.wrapped_text(RECT, "", TextStyle::default(), LineLimit::Unlimited)
                .unwrap();
        }
        assert_eq!(
            ui.wrapped_text(RECT, "", TextStyle::default(), LineLimit::Unlimited),
            Err(UiError::Capacity)
        );
        assert_eq!(ui.commands.len(), 1024);
        begin(&mut ui);
        assert!(ui.commands.is_empty() && ui.text.is_empty());
        let report = ui
            .wrapped_text(RECT, "NEW", TextStyle::default(), LineLimit::Unlimited)
            .unwrap();
        assert_eq!((report.line_count, report.hidden_lines), (1, 0));
        assert_eq!(
            (ui.commands.as_ptr(), ui.text.as_ptr(), ui.reserved_bytes()),
            storage
        );
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

    #[test]
    fn responsive_labels_keep_glyph_height_and_only_store_visible_prefixes() {
        reset();
        crate::edit(|s| {
            s.w = 320;
            s.h = 240;
        });
        let mut ui = Ui::new();
        ui.begin(Viewport::responsive([320., 240.], [0.; 4]).unwrap());
        let long = "X".repeat(100_000);
        ui.label(
            UiRect {
                min: [0.; 2],
                size: [60., 16.],
            },
            &long,
            TextStyle {
                size: 16.,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(ui.text, "XX...");
        ui.paint();
        crate::read(|s| {
            assert!(!s.vertices.is_empty());
            let bottom = s
                .vertices
                .iter()
                .map(|v| (1. - v[1]) * 120.)
                .fold(0_f32, f32::max);
            assert!(
                (bottom - 14.).abs() < 0.001,
                "7 bitmap rows at a 2px cell height"
            );
            assert!(s.vertices.len() <= ui.vertices);
        });
        ui.hud_button(
            1,
            RECT,
            &long,
            true,
            ButtonStyle {
                size: 16.,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(ui.text, "XX...XX..."); // 64px text area fits five glyphs.
        let before = (ui.commands.len(), ui.text.len(), ui.vertices);
        assert_eq!(
            ui.hud_button(1, RECT, &long, true, ButtonStyle::default()),
            Err(UiError::DuplicateId)
        );
        assert_eq!(before, (ui.commands.len(), ui.text.len(), ui.vertices));
        assert_eq!(
            ui.label(
                RECT,
                "bad",
                TextStyle {
                    size: f32::NAN,
                    ..Default::default()
                }
            ),
            Err(UiError::InvalidLayout)
        );
        assert_eq!(before, (ui.commands.len(), ui.text.len(), ui.vertices));
    }

    #[test]
    fn tiny_button_paint_and_focus_never_escape_the_hit_rectangle() {
        reset();
        crate::edit(|s| {
            s.w = 200;
            s.h = 200;
        });
        let mut ui = Ui::new();
        for size in [[40., 4.], [1., 1.], [0., 0.]] {
            next();
            ui.begin(Viewport::responsive([200.; 2], [0.; 4]).unwrap());
            let rect = UiRect {
                min: [20.; 2],
                size,
            };
            // Existing buttons get the same geometry fix as new HUD buttons.
            ui.button(1, rect, "LABEL", true, ButtonStyle::default())
                .unwrap();
            ui.state.end_frame();
            let visible = size[0] > 0. && size[1] > 0.;
            assert_eq!(ui.state.focus_next(false), visible.then_some(1));
            assert_eq!(ui.state.hit_test([20.; 2]), visible.then_some(1));
            assert_eq!(ui.state.hit_test([20. + size[0], 20.]), None);
            ui.paint();
            crate::read(|s| {
                assert_eq!(s.vertices.is_empty(), !visible);
                for vertex in &s.vertices {
                    let pixel = [(vertex[0] + 1.) * 100., (1. - vertex[1]) * 100.];
                    for (axis, coordinate) in pixel.iter().enumerate() {
                        assert!(*coordinate >= rect.min[axis] - 0.001);
                        assert!(*coordinate <= rect.min[axis] + rect.size[axis] + 0.001);
                    }
                }
                assert!(s.vertices.len() <= ui.vertices);
            });
        }
    }

    #[test]
    fn responsive_hud_rebuilds_keep_arena_storage_and_hide_empty_viewports() {
        reset();
        crate::edit(|s| {
            s.w = 320;
            s.h = 240;
        });
        let mut ui = Ui::new();
        let storage = (ui.commands.as_ptr(), ui.text.as_ptr(), ui.reserved_bytes());
        let long = "é日".repeat(10_000);
        for width in [320., 180., 40., 1., 0., 320.] {
            next();
            let viewport = Viewport::responsive([width, 160.], [12.; 4]).unwrap();
            ui.begin(viewport);
            for (index, rect) in hud_slots(viewport.bounds(), 3, 140., 44., 8.)
                .unwrap()
                .enumerate()
            {
                ui.hud_button(index as u64, rect, &long, true, ButtonStyle::default())
                    .unwrap();
            }
            ui.state.end_frame();
            let focus = ui.state.focus_next(false);
            if viewport.bounds().size[0] == 0. {
                assert_eq!(focus, None);
                assert_eq!(ui.state.hit_test([0.; 2]), None);
            } else {
                assert!(focus.is_some());
            }
            ui.paint();
            crate::read(|s| {
                if viewport.bounds().size[0] == 0. {
                    assert!(s.vertices.is_empty());
                }
                assert!(s.vertices.len() <= ui.vertices);
            });
            assert_eq!(
                (ui.commands.as_ptr(), ui.text.as_ptr(), ui.reserved_bytes()),
                storage
            );
        }
    }
}
