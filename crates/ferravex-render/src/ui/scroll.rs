//! A bounded vertical region composed from the existing [`UiState`] and [`Ui`].
//!
//! Keep one [`ScrollRegion`] and a dedicated paint-only `Ui` across frames. Pass
//! all content items (including offscreen ones) to `update`, then queue labels
//! and buttons using the resulting offset. Do **not** call `Ui::finish` on that
//! paint-only UI: the region owns its focus/capture and reads the ordered host
//! journal itself. Other menus keep their own `Ui` and focus. The caller enables
//! only the active input owner; overlapping/nested scroll routing is unsupported.
//!
//! All coordinates are logical UI units. A responsive viewport keeps fonts in
//! host logical pixels; use a zero-inset viewport with absolute `HudRegions`
//! rectangles. A fitted viewport is also supported: input is unprojected once.
//! No simulation clock, gameplay edge, camera or renderer is replaced.
//!
//! ```
//! use ferravex_render::ui::{ButtonStyle, Ui, UiRect, Viewport};
//! use ferravex_render::ui::scroll::{ScrollItem, ScrollRegion};
//! let viewport = Viewport::responsive([320., 240.], [0.; 4])?;
//! let bounds = UiRect { min: [12., 32.], size: [296., 100.] };
//! let items = [10, 20, 30].map(|id| ScrollItem {
//!     id, rect: UiRect { min: [0., (id / 10 - 1) as f32 * 70.], size: [296., 44.] },
//!     enabled: true,
//! });
//! let mut region = ScrollRegion::new(3);
//! let mut paint = Ui::new(); // Reuse both between frames.
//! region.update(viewport, bounds, 184., &items, true)?;
//! assert_eq!(region.focus_next(false), Some(10));
//! assert_eq!(region.focus_next(false), Some(20));
//! assert_eq!(region.focus_next(false), Some(30));
//! assert_eq!(region.scroll().offset(), 84.);
//! paint.begin(viewport);
//! for (item, label) in items.iter().zip(["COURSE", "BEST TIME", "CONTROLS"]) {
//!     region.button(&mut paint, item.id, label, ButtonStyle::default())?;
//! }
//! // In a host frame: paint.paint(); inspect region.activated(item.id).
//! // For remapped confirmation: call activate_focused only on its accepted edge.
//! assert_eq!(region.activate_focused(), Some(30));
//! assert!(region.activated(30));
//! # Ok::<(), ferravex_render::ui::UiError>(())
//! ```

use super::{ButtonStyle, Ui, UiError, UiRect, Viewport, WidgetId};
use crate::input::{InputCode, MAX_EVENTS, PlatformEvent, touch};
use ferravex_play::ui::{UiState, Widget, scroll::ScrollState};

/// Region item ceiling, additionally subject to the paint UI's command/text budgets.
pub const SCROLL_ITEM_CAPACITY: usize = 1024;

/// A stable identity and unscrolled content-space rectangle. Items must fit the
/// declared content width/height. Disabled items still occlude pointer hits.
/// Slice order defines Tab/arrow order and back-to-front pointer hit order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollItem {
    pub id: WidgetId,
    pub rect: UiRect,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pointer {
    Mouse,
    Touch(u64),
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    pointer: Pointer,
    start: [f32; 2],
    offset: f32,
    moved: bool,
}

/// Bounded content registry with the existing UI's press/release capture rules.
/// Construction reserves storage once; update/navigation/drag do not grow it.
/// Focus survives scrolling and cancellation. Resize/reflow cancels a pending
/// press, preserves eligible focus IDs, and clamps the offset without activation.
pub struct ScrollRegion {
    scroll: ScrollState,
    state: UiState,
    items: Vec<ScrollItem>,
    viewport: Viewport,
    bounds: UiRect,
    drag: Option<Drag>,
    active: Option<WidgetId>,
    frame: Option<u64>,
    epoch: u64,
    shift: u8,
    gamepad: u8,
    pad_epoch: Option<(u8, u32)>,
    accepting: bool,
    mouse: Option<[f32; 2]>,
}

impl ScrollRegion {
    /// Reserve at most [`SCROLL_ITEM_CAPACITY`] items; zero capacity is legal.
    /// See the module recipe for the update-before-paint lifecycle.
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.min(SCROLL_ITEM_CAPACITY);
        let viewport = Viewport::responsive([0.; 2], [0.; 4]).unwrap();
        Self {
            scroll: ScrollState::default(),
            state: UiState::with_capacity(capacity),
            items: Vec::with_capacity(capacity),
            viewport,
            bounds: viewport.bounds(),
            drag: None,
            active: None,
            frame: None,
            epoch: 0,
            shift: 0,
            gamepad: 0,
            pad_epoch: None,
            accepting: false,
            mouse: None,
        }
    }

    pub fn capacity(&self) -> usize {
        self.state.capacity()
    }

    pub fn scroll(&self) -> &ScrollState {
        &self.scroll
    }

    pub fn bounds(&self) -> UiRect {
        self.bounds
    }

    pub fn focused(&self) -> Option<WidgetId> {
        self.state.focused()
    }

    pub fn activated(&self, id: WidgetId) -> bool {
        self.active == Some(id)
    }

    /// Select the player's controller (0..4). New/reconnected pads need a fresh
    /// edge after their first update, matching the existing `Ui` adapter.
    pub fn set_gamepad(&mut self, pad: u8) -> Result<(), UiError> {
        if pad >= 4 {
            return Err(UiError::InvalidLayout);
        }
        if self.gamepad != pad {
            self.gamepad = pad;
            self.pad_epoch = None;
        }
        Ok(())
    }

    /// Validate/rebuild content, then handle the current host journal once.
    ///
    /// Wheel uses 40 logical units per host notch, positive host wheel meaning
    /// up. Mouse/one-touch drags start anywhere inside the region, cancel child
    /// clicks after six logical units of movement, and continue outside it.
    /// Tab/arrows and selected-pad D-pad reveal focus; Page Up/Down move one
    /// viewport, Home/End move to the boundaries. Enter/pad A confirm. Feed
    /// remapped edges through `focus_next`, `page`, and `activate_focused`.
    ///
    /// Bounds must be inside `viewport.bounds()`. Items must be unique and fully
    /// inside the declared content. Errors preserve the old layout/offset/focus
    /// but cancel capture/activation and disable acceptance until a valid update.
    /// Hidden, disabled, host-unfocused, journal-overflow and settings-editor
    /// frames cancel gestures. Duplicate calls in a host frame do not replay
    /// events and clear the previous activation result.
    pub fn update(
        &mut self,
        viewport: Viewport,
        bounds: UiRect,
        content_extent: f32,
        items: &[ScrollItem],
        enabled: bool,
    ) -> Result<(), UiError> {
        self.active = None;
        let mut next_scroll = self.scroll;
        let validation = (|| {
            next_scroll.set_extents(bounds.size[1], content_extent)?;
            if !bounds.valid()
                || (0..2).any(|axis| {
                    bounds.min[axis] < viewport.bounds().min[axis]
                        || bounds.min[axis] as f64 + bounds.size[axis] as f64
                            > viewport.bounds().min[axis] as f64
                                + viewport.bounds().size[axis] as f64
                })
            {
                return Err(UiError::InvalidLayout);
            }
            if items.len() > self.capacity() {
                return Err(UiError::Capacity);
            }
            for (index, item) in items.iter().enumerate() {
                next_scroll.translate_rect(bounds, item.rect)?;
                if items[..index].iter().any(|old| old.id == item.id) {
                    return Err(UiError::DuplicateId);
                }
            }
            Ok(())
        })();
        if let Err(error) = validation {
            self.cancel();
            self.accepting = false;
            // A repaired layout must not replay the packet that failed admission.
            self.frame = Some(crate::read(|s| s.frame));
            return Err(error);
        }
        let changed = self.bounds != bounds
            || self.viewport.bounds() != viewport.bounds()
            || self.viewport.pixels() != viewport.pixels()
            || self.viewport.scale() != viewport.scale()
            || self.scroll.content_extent() != content_extent
            || self.items != items;
        if changed {
            self.cancel();
        }
        self.scroll = next_scroll;
        self.viewport = viewport;
        self.bounds = bounds;
        self.items.clear();
        self.items.extend_from_slice(items);
        self.state.begin_frame();
        for item in items {
            self.state.add(Widget {
                id: item.id,
                rect: item.rect,
                clip: None,
                enabled: item.enabled,
                focusable: true,
            })?; // Geometry, duplicates and capacity were preflighted above.
        }
        self.state.end_frame();

        let mut events = [PlatformEvent::None; MAX_EVENTS];
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
                s.input.pads[self.gamepad as usize]
                    .connected
                    .then_some((self.gamepad, s.input.pads[self.gamepad as usize].epoch)),
            )
        });
        let fresh_pad = self.pad_epoch != pad_epoch;
        self.pad_epoch = pad_epoch;
        self.accepting = enabled
            && focused
            && !overflow
            && !blocked
            && bounds.size.iter().all(|size| *size > 0.);
        if epoch != self.epoch || !self.accepting {
            self.cancel();
            self.shift = 0;
        }
        self.epoch = epoch;
        if self.frame == Some(frame) {
            return Ok(());
        }
        self.frame = Some(frame);
        if !self.accepting {
            return Ok(());
        }
        // Absolute positions are journaled at button edges. Between them the
        // existing host supplies relative motion and a final absolute sample.
        let final_pointer = pointer;
        let mut pointer = self.mouse.unwrap_or(final_pointer);
        self.mouse = Some(final_pointer);
        for event in events {
            match event {
                PlatformEvent::None => break,
                PlatformEvent::PointerPosition(p) => {
                    pointer = p.to_array();
                    self.move_pointer(Pointer::Mouse, pointer);
                }
                PlatformEvent::MouseMotion(delta) => {
                    pointer[0] += delta.x;
                    pointer[1] += delta.y;
                    self.move_pointer(Pointer::Mouse, pointer);
                }
                PlatformEvent::Value(InputCode::Mouse(1), value) => {
                    if value >= 0.5 {
                        self.press(Pointer::Mouse, pointer);
                    } else {
                        self.release(Pointer::Mouse, pointer);
                    }
                }
                PlatformEvent::Touch(event) => {
                    let owner = Pointer::Touch(event.id);
                    let point = event.position.to_array();
                    match event.phase {
                        touch::Phase::Start => self.press(owner, point),
                        touch::Phase::Move => self.move_pointer(owner, point),
                        touch::Phase::End => self.release(owner, point),
                        touch::Phase::Cancel => {
                            if self.drag.is_some_and(|drag| drag.pointer == owner) {
                                self.cancel();
                            }
                        }
                    }
                }
                PlatformEvent::Value(InputCode::Wheel(1), value) => {
                    if self
                        .local_point(final_pointer)
                        .is_some_and(|p| self.bounds.contains(p))
                    {
                        // Host values are finite and capped at 1000; retain a
                        // finite delta even for a directly injected journal.
                        if value.is_finite() {
                            let _ = self.scroll_by(-value.clamp(-1000., 1000.) * 40.);
                        }
                    }
                }
                PlatformEvent::Value(InputCode::Key(key @ (0xffe1 | 0xffe2)), value) => {
                    let bit = if key == 0xffe1 { 1 } else { 2 };
                    if value >= 0.5 {
                        self.shift |= bit;
                    } else {
                        self.shift &= !bit;
                    }
                }
                PlatformEvent::Value(InputCode::Key(key), value) if value >= 0.5 => match key {
                    0xff09 => {
                        self.focus_next(self.shift != 0);
                    }
                    0xff54 | 0xff53 => {
                        self.focus_next(false);
                    }
                    0xff52 | 0xff51 => {
                        self.focus_next(true);
                    }
                    0xff55 => {
                        self.page(true);
                    }
                    0xff56 => {
                        self.page(false);
                    }
                    0xff50 => {
                        let _ = self.scroll_by(-f32::MAX);
                    }
                    0xff57 => {
                        let _ = self.scroll_by(f32::MAX);
                    }
                    0xff0d => {
                        self.activate_focused();
                    }
                    0xff1b => self.cancel(),
                    _ => {}
                },
                PlatformEvent::Pad {
                    index,
                    code: InputCode::PadButton(button),
                    value,
                } if index == self.gamepad && !fresh_pad && value >= 0.5 => match button {
                    12 | 14 => {
                        self.focus_next(true);
                    }
                    13 | 15 => {
                        self.focus_next(false);
                    }
                    0 => {
                        self.activate_focused();
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        self.move_pointer(Pointer::Mouse, final_pointer);
        Ok(())
    }

    /// Submission-order navigation including offscreen items, with wrap and
    /// disabled/zero-area skipping inherited from `UiState`. Reveals the result
    /// before the next paint; does not activate it or change another UI's focus.
    pub fn focus_next(&mut self, reverse: bool) -> Option<WidgetId> {
        self.cancel();
        let id = self.state.focus_next(reverse)?;
        self.reveal(id);
        Some(id)
    }

    /// Reveal a registered enabled, nonempty item without selecting or activating
    /// it. Unknown/disabled IDs return false. Returns whether the offset changed.
    pub fn reveal(&mut self, id: WidgetId) -> bool {
        let Some(item) = self.items.iter().find(|item| {
            item.id == id && item.enabled && item.rect.size.iter().all(|size| *size > 0.)
        }) else {
            return false;
        };
        let changed = self
            .scroll
            .ensure_visible(item.rect.min[1], item.rect.size[1])
            .unwrap_or(false);
        if changed {
            self.cancel();
        }
        changed
    }

    /// Explicit/remapped scroll input. Cancels a pending click even at an exhausted
    /// boundary. Nonfinite deltas fail without moving or canceling anything.
    pub fn scroll_by(&mut self, delta: f32) -> Result<bool, UiError> {
        let changed = self.scroll.scroll_by(delta)?;
        self.cancel();
        Ok(changed)
    }

    /// Explicit/remapped Page Up/Down, retaining focus without activating a row.
    pub fn page(&mut self, reverse: bool) -> bool {
        self.cancel();
        self.scroll.page(reverse)
    }

    /// Confirm the focused visible item while input is enabled. If focus is
    /// entirely clipped (for example after Page Down), reveal it and return
    /// `None`; another edge is required to activate. No focus is auto-created.
    pub fn activate_focused(&mut self) -> Option<WidgetId> {
        if !self.accepting {
            return None;
        }
        let id = self.state.activate_focused()?;
        if self.rect(id)?.intersect(self.bounds).is_none() {
            self.reveal(id);
            return None;
        }
        self.active = Some(id);
        Some(id)
    }

    /// Current translated geometry; clip it against `bounds()` for paint.
    pub fn rect(&self, id: WidgetId) -> Option<UiRect> {
        let item = self.items.iter().find(|item| item.id == id)?;
        self.scroll.translate_rect(self.bounds, item.rect).ok()
    }

    /// Queue a registered item using the existing fixed-size HUD button and clip
    /// stack. The focused item uses the style's focus fill and original fill ink.
    /// This is paint only: inspect this region's `activated`, not `Ui::activated`.
    /// On queue failure the clip is still popped; earlier paint commands remain.
    pub fn button(
        &self,
        ui: &mut Ui,
        id: WidgetId,
        label: &str,
        mut style: ButtonStyle,
    ) -> Result<(), UiError> {
        let item = self
            .items
            .iter()
            .find(|item| item.id == id)
            .ok_or(UiError::InvalidLayout)?;
        let rect = self.rect(id).ok_or(UiError::InvalidLayout)?;
        if self.focused() == Some(id) && item.enabled {
            style.ink = style.fill;
            style.fill = style.focus;
        }
        ui.push_clip(self.bounds)?;
        let result = ui.hud_button(id, rect, label, item.enabled, style);
        ui.pop_clip();
        result
    }

    /// Cancel mouse/touch capture and the pending activation, preserving focus
    /// and offset. Call on consumer context changes even without another update.
    pub fn cancel(&mut self) {
        self.drag = None;
        self.state.cancel_pointer();
        self.active = None;
    }

    /// Reset offset, focus, content registration and input acceptance while
    /// retaining allocated capacity and the once-per-host-frame input guard.
    pub fn reset(&mut self) {
        self.cancel();
        self.scroll.reset();
        self.items.clear();
        self.state.begin_frame();
        self.state.end_frame();
        self.accepting = false;
        self.shift = 0;
    }

    fn local_point(&self, raw: [f32; 2]) -> Option<[f32; 2]> {
        let point = std::array::from_fn(|axis| {
            ((raw[axis] as f64 - self.viewport.pixels().min[axis] as f64)
                / self.viewport.scale() as f64) as f32
        });
        point.iter().all(|v| v.is_finite()).then_some(point)
    }

    fn content_point(&self, raw: [f32; 2]) -> [f32; 2] {
        self.local_point(raw)
            .and_then(|point| self.scroll.content_point(self.bounds, point))
            .unwrap_or([f32::NAN; 2])
    }

    fn press(&mut self, pointer: Pointer, raw: [f32; 2]) {
        if self.drag.is_some() {
            return;
        }
        let Some(point) = self.local_point(raw).filter(|p| self.bounds.contains(*p)) else {
            return;
        };
        self.state.pointer_down(self.content_point(raw));
        self.drag = Some(Drag {
            pointer,
            start: point,
            offset: self.scroll.offset(),
            moved: false,
        });
    }

    fn move_pointer(&mut self, pointer: Pointer, raw: [f32; 2]) {
        let Some(mut drag) = self.drag.filter(|drag| drag.pointer == pointer) else {
            return;
        };
        let Some(point) = self.local_point(raw) else {
            self.cancel();
            return;
        };
        let dx = point[0] as f64 - drag.start[0] as f64;
        let dy = point[1] as f64 - drag.start[1] as f64;
        if dx * dx + dy * dy >= 36. {
            drag.moved = true;
        }
        if drag.moved {
            self.state.cancel_pointer();
            let offset =
                (drag.offset as f64 - dy).clamp(0., self.scroll.max_offset() as f64) as f32;
            let _ = self.scroll.set_offset(offset);
        }
        self.drag = Some(drag);
    }

    fn release(&mut self, pointer: Pointer, raw: [f32; 2]) {
        if !self.drag.is_some_and(|drag| drag.pointer == pointer) {
            return;
        }
        self.move_pointer(pointer, raw); // End may carry the only movement sample.
        let Some(drag) = self.drag.take() else {
            return;
        };
        if !drag.moved {
            self.active = self
                .state
                .pointer_up(self.content_point(raw))
                .or(self.active);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDS: UiRect = UiRect {
        min: [20., 30.],
        size: [100., 80.],
    };
    const ITEMS: [ScrollItem; 4] = [
        ScrollItem {
            id: 10,
            rect: UiRect {
                min: [0., 0.],
                size: [100., 40.],
            },
            enabled: true,
        },
        ScrollItem {
            id: 20,
            rect: UiRect {
                min: [0., 60.],
                size: [100., 40.],
            },
            enabled: true,
        },
        ScrollItem {
            id: 30,
            rect: UiRect {
                min: [0., 120.],
                size: [100., 40.],
            },
            enabled: false,
        },
        ScrollItem {
            id: 40,
            rect: UiRect {
                min: [0., 180.],
                size: [100., 40.],
            },
            enabled: true,
        },
    ];

    fn setup() -> ScrollRegion {
        crate::edit(|s| *s = crate::State::default());
        let mut region = ScrollRegion::new(4);
        update(&mut region);
        next();
        region
    }

    fn viewport() -> Viewport {
        Viewport::responsive([200.; 2], [0.; 4]).unwrap()
    }

    fn update(region: &mut ScrollRegion) {
        region
            .update(viewport(), BOUNDS, 220., &ITEMS, true)
            .unwrap();
    }

    fn next() {
        crate::edit(|s| {
            s.frame += 1;
            s.input.finish_frame();
        });
    }

    fn key(code: u32) {
        crate::key(code, true);
        crate::key(code, false);
    }

    fn tap(x: f32, y: f32) {
        crate::pointer(x, y, 0., 0.);
        crate::button(1, true);
        crate::button(1, false);
    }

    #[test]
    fn navigation_reveals_offscreen_focus_before_confirmation_and_keeps_other_ui_focus() {
        let mut region = setup();
        let mut other = Ui::new();
        other.begin(viewport());
        other
            .button(99, BOUNDS, "OTHER", true, ButtonStyle::default())
            .unwrap();
        key(0xff09);
        other.finish(true);
        assert_eq!(other.focused(), Some(99));
        next();
        for (id, offset) in [(10, 0.), (20, 20.), (40, 140.), (10, 0.)] {
            key(0xff54);
            update(&mut region);
            assert_eq!(region.focused(), Some(id));
            assert_eq!(region.scroll().offset(), offset);
            assert!(!region.activated(id));
            assert_eq!(other.focused(), Some(99));
            next();
        }
        key(0xff52);
        key(0xff0d);
        update(&mut region);
        assert_eq!(region.focused(), Some(40));
        assert!(region.activated(40));
        // Repeated processing of the same host frame never repeats activation.
        update(&mut region);
        assert!(!region.activated(40));
    }

    #[test]
    fn clipped_children_and_half_open_edges_never_click_or_activate_unseen_focus() {
        let mut region = setup();
        tap(30., 125.); // Item 20's unscrolled bounds, below the viewport clip.
        update(&mut region);
        assert_eq!(region.focused(), None);
        assert!(!region.activated(20));
        next();
        tap(30., 110.); // Bottom boundary is excluded.
        update(&mut region);
        assert!(!region.activated(20));
        next();
        tap(30., 100.); // Visible part of item 20.
        update(&mut region);
        assert!(region.activated(20));
        region.scroll_by(f32::MAX).unwrap();
        assert_eq!(region.focused(), Some(20));
        assert_eq!(region.activate_focused(), None); // Reveal only.
        assert_eq!(region.scroll().offset(), 60.);
        assert_eq!(region.activate_focused(), Some(20));
    }

    #[test]
    fn mouse_drag_moves_while_held_and_outside_without_releasing_a_button() {
        let mut region = setup();
        crate::pointer(30., 50., 0., 0.);
        crate::button(1, true);
        update(&mut region);
        assert_eq!(region.focused(), Some(10));
        next();
        crate::pointer(30., 10., 0., -40.);
        update(&mut region);
        assert_eq!(region.scroll().offset(), 40.);
        assert_eq!(region.state.captured(), None);
        next();
        crate::pointer(30., -30., 0., -40.);
        update(&mut region);
        assert_eq!(region.scroll().offset(), 80.);
        next();
        crate::pointer(30., 50., 0., 80.);
        crate::button(1, false);
        update(&mut region);
        assert_eq!(region.scroll().offset(), 0.);
        assert!(!region.activated(10));
        assert!(region.drag.is_none());
    }

    #[test]
    fn same_packet_movement_and_exhausted_drag_cancel_clicks() {
        let mut region = setup();
        crate::pointer(30., 50., 0., 0.);
        crate::button(1, true);
        crate::pointer(30., 60., 0., 10.); // Pull past top, then return.
        crate::pointer(30., 50., 0., -10.);
        crate::button(1, false);
        update(&mut region);
        assert_eq!(region.scroll().offset(), 0.);
        assert!(!region.activated(10));
        next();
        crate::touch(1, 0, 30., 50.);
        crate::touch(1, 2, 30., 40.); // End is the only motion sample.
        update(&mut region);
        assert_eq!(region.scroll().offset(), 10.);
        assert!(!region.activated(10));
    }

    #[test]
    fn touch_ownership_cancellation_and_quick_taps() {
        let mut region = setup();
        crate::touch(1, 0, 30., 50.);
        crate::touch(2, 0, 30., 50.);
        crate::touch(2, 1, 30., 5.);
        crate::touch(2, 2, 30., 5.);
        update(&mut region);
        assert_eq!(region.scroll().offset(), 0.);
        assert_eq!(region.state.captured(), Some(10));
        next();
        crate::touch(1, 1, 30., 20.);
        crate::touch(1, 3, 30., 20.);
        update(&mut region);
        assert_eq!(region.scroll().offset(), 30.);
        assert!(region.drag.is_none());
        next();
        crate::touch(1, 2, 30., 50.);
        update(&mut region);
        assert!(!region.activated(10));
        region.scroll_by(-f32::MAX).unwrap();
        next();
        crate::touch(3, 0, 30., 50.);
        crate::touch(3, 2, 30., 50.);
        update(&mut region);
        assert!(region.activated(10));
    }

    #[test]
    fn focus_loss_disable_settings_and_overflow_cancel_pending_drags() {
        for reason in 0..4 {
            let mut region = setup();
            crate::touch(1, 0, 30., 50.);
            update(&mut region);
            next();
            crate::edit(|s| match reason {
                0 => s.input.focus(false),
                1 => {}
                2 => s.controls_open = true,
                _ => {
                    s.input.overflow = true;
                }
            });
            region
                .update(viewport(), BOUNDS, 220., &ITEMS, reason != 1)
                .unwrap();
            assert!(region.drag.is_none());
            assert_eq!(region.focused(), Some(10));
            assert_eq!(region.activate_focused(), None);
            next();
            crate::edit(|s| {
                s.input.focus(true);
                s.controls_open = false;
            });
            crate::touch(1, 1, 30., 20.);
            crate::touch(1, 2, 30., 50.);
            update(&mut region);
            assert_eq!(region.scroll().offset(), 0.);
            assert!(!region.activated(10));
        }
    }

    #[test]
    fn resize_and_reorder_clamp_offset_without_retargeting_a_press() {
        let mut region = setup();
        region.scroll_by(f32::MAX).unwrap();
        crate::touch(1, 0, 30., 85.); // Item 40 at scrolled Y=70..110.
        update(&mut region);
        assert_eq!(region.focused(), Some(40));
        next();
        crate::touch(1, 2, 30., 85.);
        let bounds = UiRect {
            size: [100., 160.],
            ..BOUNDS
        };
        region
            .update(viewport(), bounds, 220., &ITEMS, true)
            .unwrap();
        assert_eq!(region.scroll().offset(), 60.);
        assert_eq!(region.focused(), Some(40));
        assert!(!region.activated(40));
        assert!(!region.activated(20));
        next();
        crate::touch(2, 0, 30., 40.); // Item 20.
        region
            .update(viewport(), bounds, 220., &ITEMS, true)
            .unwrap();
        next();
        let mut changed = ITEMS;
        changed.swap(0, 1);
        crate::touch(2, 2, 30., 40.);
        region
            .update(viewport(), bounds, 220., &changed, true)
            .unwrap();
        assert!(!region.activated(20));
        assert_eq!(region.focused(), Some(20));
    }

    #[test]
    fn wheel_pages_exhaustion_and_input_frame_deduplication() {
        let mut region = setup();
        crate::pointer(130., 50., 0., 0.);
        crate::wheel(-1.);
        update(&mut region);
        assert_eq!(region.scroll().offset(), 0.);
        next();
        crate::pointer(30., 50., 0., 0.);
        crate::wheel(-1.);
        update(&mut region);
        assert_eq!(region.scroll().offset(), 40.);
        update(&mut region);
        assert_eq!(region.scroll().offset(), 40.);
        next();
        key(0xff56);
        update(&mut region);
        assert_eq!(region.scroll().offset(), 120.);
        next();
        key(0xff57);
        update(&mut region);
        assert_eq!(region.scroll().offset(), 140.);
        assert!(!region.page(false));
        next();
        key(0xff55);
        update(&mut region);
        assert_eq!(region.scroll().offset(), 60.);
        next();
        key(0xff50);
        update(&mut region);
        assert_eq!(region.scroll().offset(), 0.);
        assert!(!region.page(true));
    }

    #[test]
    fn invalid_capacity_and_duplicate_updates_preserve_layout_but_cancel_capture() {
        let mut region = setup();
        crate::touch(1, 0, 30., 50.);
        update(&mut region);
        next();
        let original = *region.scroll();
        for (items, error) in [
            (&[ITEMS[0]; 5][..], UiError::Capacity),
            (&[ITEMS[0]; 2][..], UiError::DuplicateId),
            (
                &[ScrollItem {
                    rect: UiRect {
                        min: [0., 221.],
                        size: [1.; 2],
                    },
                    ..ITEMS[0]
                }][..],
                UiError::InvalidLayout,
            ),
        ] {
            assert_eq!(
                region.update(viewport(), BOUNDS, 220., items, true),
                Err(error)
            );
            assert_eq!(*region.scroll(), original);
            assert_eq!(region.items, ITEMS);
            assert_eq!(region.focused(), Some(10));
            assert!(region.drag.is_none());
            assert_eq!(region.activate_focused(), None);
        }
        assert_eq!(region.set_gamepad(4), Err(UiError::InvalidLayout));
        crate::touch(1, 2, 30., 50.);
        update(&mut region); // No failed packet replay on same-frame recovery.
        assert!(!region.activated(10));
        let capacity = region.capacity();
        region.reset();
        assert_eq!(region.capacity(), capacity);
        assert_eq!(region.focused(), None);
        assert_eq!(region.scroll().offset(), 0.);
        assert!(region.items.is_empty());
        let mut empty = ScrollRegion::new(0);
        assert_eq!(
            empty.update(viewport(), BOUNDS, 220., &ITEMS, true),
            Err(UiError::Capacity)
        );
        empty.update(viewport(), BOUNDS, 220., &[], true).unwrap();
        assert_eq!(empty.focus_next(false), None);
    }

    #[test]
    fn scaled_safe_area_input_and_painted_vertices_share_the_clip() {
        let mut region = setup();
        let viewport = Viewport::fit([440., 420.], [200.; 2], [20., 10., 20., 10.]).unwrap();
        assert_eq!(viewport.scale(), 2.);
        crate::touch(1, 0, 80., 110.); // Local point [30,50].
        crate::touch(1, 2, 80., 110.);
        region.update(viewport, BOUNDS, 220., &ITEMS, true).unwrap();
        assert!(region.activated(10));
        let mut paint = Ui::new();
        paint.begin(viewport);
        for item in ITEMS {
            region
                .button(&mut paint, item.id, "ROW", ButtonStyle::default())
                .unwrap();
        }
        crate::edit(|s| {
            s.w = 440;
            s.h = 420;
            s.vertices.clear();
        });
        paint.paint();
        crate::read(|s| {
            assert!(!s.vertices.is_empty());
            for vertex in &s.vertices {
                let raw = [(vertex[0] + 1.) * 220., (1. - vertex[1]) * 210.];
                let local = [(raw[0] - 20.) / 2., (raw[1] - 10.) / 2.];
                for (axis, coordinate) in local.into_iter().enumerate() {
                    assert!(coordinate >= BOUNDS.min[axis] - 0.001);
                    assert!(coordinate <= BOUNDS.min[axis] + BOUNDS.size[axis] + 0.001);
                }
            }
        });
        // A full existing clip stack returns a capacity error without popping it.
        for _ in 0..15 {
            paint.push_clip(BOUNDS).unwrap();
        }
        assert_eq!(
            region.button(&mut paint, 10, "ROW", ButtonStyle::default()),
            Err(UiError::Capacity)
        );
        assert_eq!(paint.push_clip(BOUNDS), Err(UiError::Capacity));
    }

    #[test]
    fn selected_gamepad_reconnect_suppression_and_remapped_methods() {
        let mut region = setup();
        region.set_gamepad(2).unwrap();
        crate::edit(|s| {
            s.input.pad_connected(2, true);
            s.input.pad_button(2, 13, 1.);
        });
        update(&mut region);
        assert_eq!(region.focused(), None); // Connection frame is suppressed.
        next();
        crate::edit(|s| {
            s.input.pad_button(2, 13, 0.);
            s.input.pad_button(2, 13, 1.);
            s.input.pad_button(2, 0, 1.);
        });
        update(&mut region);
        assert_eq!(region.focused(), Some(10));
        assert!(region.activated(10));
        // A caller's remapped edges use the same operations, no synthetic keys.
        assert_eq!(region.focus_next(true), Some(40));
        assert_eq!(region.activate_focused(), Some(40));
        assert!(region.activated(40));
    }
}
