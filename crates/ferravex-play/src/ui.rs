//! Renderer-independent anchored rectangles, clipped hit testing, pointer capture,
//! and keyboard focus. Rendering and accessibility adapters remain host concerns.
//!
//! For a HUD that keeps readable pixel-sized text when resized, use
//! [`Viewport::responsive`], [`Anchor::place`] and [`hud_slots`]. These helpers
//! return geometry/borrowed text without allocating; reuse [`UiState`] each frame.
//! Use [`UiState::focus_direction`] for spatial arrow/gamepad navigation, and
//! [`UiState::focus_next`] for submission-order tab navigation.
//! [`HudRegions`] reserves screen-space strips before fitting a world canvas.

pub mod flow;
pub mod layout;
pub mod meters;
pub mod minimap;
pub mod notices;
pub mod scroll;
pub mod theme;
pub use layout::{CanvasFit, HUD_REGION_CAPACITY, HudEdge, HudLayoutError, HudRegion, HudRegions};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min: [f32; 2],
    pub size: [f32; 2],
}
impl Rect {
    pub fn valid(self) -> bool {
        self.min.into_iter().chain(self.size).all(f32::is_finite)
            && self.size.into_iter().all(|n| n >= 0.0)
            && (0..2).all(|i| (self.min[i] + self.size[i]).is_finite())
    }
    /// Half-open bounds give adjacent widgets an unambiguous shared edge.
    pub fn contains(self, p: [f32; 2]) -> bool {
        self.valid()
            && (0..2).all(|i| {
                p[i].is_finite() && p[i] >= self.min[i] && p[i] < self.min[i] + self.size[i]
            })
    }
    pub fn intersect(self, other: Self) -> Option<Self> {
        if !self.valid() || !other.valid() {
            return None;
        }
        let min = std::array::from_fn(|i| self.min[i].max(other.min[i]));
        let max: [f32; 2] =
            std::array::from_fn(|i| (self.min[i] + self.size[i]).min(other.min[i] + other.size[i]));
        (max[0] > min[0] && max[1] > min[1]).then_some(Self {
            min,
            size: [max[0] - min[0], max[1] - min[1]],
        })
    }
    /// Inset by left, top, right, bottom, keeping the result inside this rectangle.
    /// If opposite insets exceed the extent, scale that pair proportionally and
    /// collapse the axis to zero. Negative/non-finite values are rejected.
    ///
    /// ```
    /// use ferravex_play::ui::Rect;
    /// let panel = Rect { min: [10., 20.], size: [100., 40.] };
    /// assert_eq!(panel.inset([8.; 4])?.size, [84., 24.]);
    /// assert_eq!(panel.inset([80., 0., 80., 0.])?.size, [0., 40.]);
    /// # Ok::<(), ferravex_play::ui::UiError>(())
    /// ```
    pub fn inset(self, insets: [f32; 4]) -> Result<Self, UiError> {
        if !self.valid() || !insets.into_iter().all(|v| v.is_finite() && v >= 0.) {
            return Err(UiError::InvalidLayout);
        }
        let mut rect = self;
        for axis in 0..2 {
            // Widen the sum so even two finite f32::MAX insets collapse safely.
            let leading = insets[axis] as f64;
            let total = leading + insets[axis + 2] as f64;
            let extent = self.size[axis] as f64;
            let offset = if total > extent {
                extent * (leading / total)
            } else {
                leading
            };
            rect.min[axis] += offset as f32;
            rect.size[axis] = (extent - total).max(0.) as f32;
        }
        Ok(rect)
    }
}

/// Placement within a parent; oversized requests shrink to its available extent.
/// Unlike stretched [`Anchors`], this keeps a fixed HUD size whenever it fits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}
impl Anchor {
    /// Place a requested size entirely inside `parent`, including empty parents.
    ///
    /// ```
    /// use ferravex_play::ui::{Anchor, Viewport};
    /// let viewport = Viewport::responsive([800., 600.], [0., 24., 0., 16.])?;
    /// let safe = viewport.bounds().inset([12.; 4])?;
    /// let score = Anchor::TopRight.place(safe, [180., 44.])?;
    /// assert_eq!(score.min, [608., 12.]); // Logical coordinates within the safe area.
    /// # Ok::<(), ferravex_play::ui::UiError>(())
    /// ```
    pub fn place(self, parent: Rect, size: [f32; 2]) -> Result<Rect, UiError> {
        if !parent.valid() || !size.into_iter().all(|v| v.is_finite() && v >= 0.) {
            return Err(UiError::InvalidLayout);
        }
        let align = match self {
            Self::TopLeft => [0., 0.],
            Self::TopCenter => [0.5, 0.],
            Self::TopRight => [1., 0.],
            Self::CenterLeft => [0., 0.5],
            Self::Center => [0.5, 0.5],
            Self::CenterRight => [1., 0.5],
            Self::BottomLeft => [0., 1.],
            Self::BottomCenter => [0.5, 1.],
            Self::BottomRight => [1., 1.],
        };
        let size = std::array::from_fn(|i| size[i].min(parent.size[i]));
        Ok(Rect {
            min: std::array::from_fn(|i| parent.min[i] + (parent.size[i] - size[i]) * align[i]),
            size,
        })
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Anchors {
    pub min: [f32; 2],
    pub max: [f32; 2],
    pub offset_min: [f32; 2],
    pub offset_max: [f32; 2],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiError {
    InvalidLayout,
    DuplicateId,
    Capacity,
}
impl Anchors {
    pub fn resolve(self, parent: Rect) -> Result<Rect, UiError> {
        if !parent.valid()
            || !self
                .min
                .into_iter()
                .chain(self.max)
                .all(|n| n.is_finite() && (0.0..=1.0).contains(&n))
            || !self
                .offset_min
                .into_iter()
                .chain(self.offset_max)
                .all(f32::is_finite)
            || (0..2).any(|i| self.min[i] > self.max[i])
        {
            return Err(UiError::InvalidLayout);
        }
        let min = std::array::from_fn(|i| {
            parent.min[i] + parent.size[i] * self.min[i] + self.offset_min[i]
        });
        let max: [f32; 2] = std::array::from_fn(|i| {
            parent.min[i] + parent.size[i] * self.max[i] + self.offset_max[i]
        });
        let rect = Rect {
            min,
            size: [max[0] - min[0], max[1] - min[1]],
        };
        if rect.valid() {
            Ok(rect)
        } else {
            Err(UiError::InvalidLayout)
        }
    }
}
pub type WidgetId = u64;
#[derive(Clone, Copy, Debug)]
pub struct Widget {
    pub id: WidgetId,
    pub rect: Rect,
    pub clip: Option<Rect>,
    pub enabled: bool,
    pub focusable: bool,
}
impl Widget {
    fn available(self) -> bool {
        self.enabled
            && self.rect.size.into_iter().all(|n| n > 0.)
            && self.clip.is_none_or(|c| self.rect.intersect(c).is_some())
    }
    fn contains(self, point: [f32; 2]) -> bool {
        self.rect.contains(point) && self.clip.is_none_or(|clip| clip.contains(point))
    }
    fn focus_rect(self) -> Option<Rect> {
        if !self.available() || !self.focusable {
            return None;
        }
        self.clip
            .map_or(Some(self.rect), |clip| self.rect.intersect(clip))
    }
}

/// Spatial navigation in logical UI coordinates; positive Y points down.
/// See [`UiState::focus_direction`] for a complete example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusDirection {
    Left,
    Right,
    Up,
    Down,
}

/// Edge behavior for [`UiState::focus_direction`], chosen on each call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusWrap {
    /// Keep focus when no eligible center lies ahead in the requested direction.
    Clamp,
    /// If no center lies ahead, choose one behind at the opposite edge.
    /// Prefer overlapping rows/columns, then the furthest center behind, then
    /// the smallest perpendicular center distance, then the lowest widget ID.
    /// Equal-axis centers are not wrap targets; a lone widget retains focus.
    Wrap,
}

pub struct UiState {
    widgets: Vec<Widget>,
    focus: Option<WidgetId>,
    pressed: Option<WidgetId>,
    limit: usize,
}
impl Default for UiState {
    fn default() -> Self {
        Self::with_capacity(256)
    }
}
impl UiState {
    /// Fixed widget budget; rebuilding a frame never grows this buffer.
    pub fn with_capacity(limit: usize) -> Self {
        let limit = limit.min(4096);
        Self {
            widgets: Vec::with_capacity(limit),
            focus: None,
            pressed: None,
            limit,
        }
    }
    pub fn new() -> Self {
        Self::default()
    }
    /// Rebuild widgets in back-to-front paint order; IDs preserve focus/capture.
    pub fn begin_frame(&mut self) {
        self.widgets.clear();
    }
    pub fn add(&mut self, widget: Widget) -> Result<(), UiError> {
        if !widget.rect.valid() || widget.clip.is_some_and(|clip| !clip.valid()) {
            return Err(UiError::InvalidLayout);
        }
        if self.widgets.iter().any(|old| old.id == widget.id) {
            return Err(UiError::DuplicateId);
        }
        if self.widgets.len() == self.limit {
            return Err(UiError::Capacity);
        }
        self.widgets.push(widget);
        Ok(())
    }
    /// Call after layout, before delivering input, to discard removed/disabled IDs.
    pub fn end_frame(&mut self) {
        if self.focus.is_some_and(|id| {
            !self
                .widgets
                .iter()
                .any(|w| w.id == id && w.available() && w.focusable)
        }) {
            self.focus = None;
        }
        if self
            .pressed
            .is_some_and(|id| !self.widgets.iter().any(|w| w.id == id && w.available()))
        {
            self.pressed = None;
        }
    }
    pub fn capacity(&self) -> usize {
        self.limit
    }
    pub fn focused(&self) -> Option<WidgetId> {
        self.focus
    }
    pub fn captured(&self) -> Option<WidgetId> {
        self.pressed
    }
    pub fn hit_test(&self, point: [f32; 2]) -> Option<WidgetId> {
        self.widgets
            .iter()
            .rev()
            .find(|w| w.contains(point))
            .filter(|w| w.available())
            .map(|w| w.id)
    }
    pub fn pointer_down(&mut self, point: [f32; 2]) -> Option<WidgetId> {
        self.pressed = self.hit_test(point);
        self.focus = self
            .pressed
            .filter(|id| self.widgets.iter().any(|w| w.id == *id && w.focusable));
        self.pressed
    }
    /// Click activates only when both press and release belong to the same widget.
    pub fn pointer_up(&mut self, point: [f32; 2]) -> Option<WidgetId> {
        let pressed = self.pressed.take();
        pressed.filter(|id| self.hit_test(point) == Some(*id))
    }
    pub fn cancel_pointer(&mut self) {
        self.pressed = None;
    }
    pub fn focus_next(&mut self, reverse: bool) -> Option<WidgetId> {
        let count = self.widgets.len();
        if count == 0 {
            self.focus = None;
            return None;
        }
        let current = self
            .focus
            .and_then(|id| self.widgets.iter().position(|w| w.id == id));
        for step in 1..=count {
            let index = if reverse {
                (current.unwrap_or(0) + count - step) % count
            } else {
                current.map_or(step - 1, |index| (index + step) % count)
            };
            let w = self.widgets[index];
            if w.available() && w.focusable {
                self.focus = Some(w.id);
                return self.focus;
            }
        }
        self.focus = None;
        None
    }
    /// Move focus spatially and return the resulting focused ID, even if unchanged.
    /// Only enabled, focusable widgets with nonempty clipped geometry participate.
    /// As with tab focus, paint occlusion does not remove a widget from navigation.
    ///
    /// A candidate's clipped center must lie strictly ahead of the current center.
    /// Candidates whose perpendicular intervals overlap the current rectangle
    /// rank first (the same row for left/right, column for up/down), followed by
    /// squared center distance, then the lowest [`WidgetId`]. Touching intervals
    /// do not overlap. Equal centers are not directional neighbors. Geometry is
    /// widened to `f64` before arithmetic, including distance calculations.
    ///
    /// [`FocusWrap`] applies only when no candidate lies ahead. If the focused ID
    /// is absent or no longer eligible, select the first eligible widget in tab
    /// order without taking another step; return `None` if none is eligible.
    /// Call after layout and [`Self::end_frame`], as with other input methods.
    /// This does not activate widgets, reorder tab/paint order, or change pointer
    /// capture. It scans the bounded widget buffer without allocating.
    ///
    /// ```
    /// use ferravex_play::ui::{FocusDirection, FocusWrap, Rect, UiState, Widget};
    /// let mut ui = UiState::with_capacity(2);
    /// ui.begin_frame();
    /// for (id, x) in [(1, 0.), (2, 80.)] {
    ///     ui.add(Widget {
    ///         id, rect: Rect { min: [x, 0.], size: [60., 30.] },
    ///         clip: None, enabled: true, focusable: true,
    ///     })?;
    /// }
    /// ui.end_frame();
    /// assert_eq!(ui.focus_direction(FocusDirection::Right, FocusWrap::Clamp), Some(1));
    /// assert_eq!(ui.focus_direction(FocusDirection::Right, FocusWrap::Clamp), Some(2));
    /// assert_eq!(ui.focus_direction(FocusDirection::Right, FocusWrap::Clamp), Some(2));
    /// assert_eq!(ui.focus_direction(FocusDirection::Right, FocusWrap::Wrap), Some(1));
    /// assert_eq!(ui.activate_focused(), Some(1));
    /// # Ok::<(), ferravex_play::ui::UiError>(())
    /// ```
    pub fn focus_direction(
        &mut self,
        direction: FocusDirection,
        wrap: FocusWrap,
    ) -> Option<WidgetId> {
        let origin = self.focus.and_then(|id| {
            self.widgets
                .iter()
                .find(|w| w.id == id)
                .and_then(|w| w.focus_rect())
        });
        let Some(origin) = origin else {
            self.focus = self
                .widgets
                .iter()
                .find_map(|w| w.focus_rect().map(|_| w.id));
            return self.focus;
        };
        let (axis, sign) = match direction {
            FocusDirection::Left => (0, -1.),
            FocusDirection::Right => (0, 1.),
            FocusDirection::Up => (1, -1.),
            FocusDirection::Down => (1, 1.),
        };
        let cross = 1 - axis;
        let center = |rect: Rect| -> [f64; 2] {
            std::array::from_fn(|i| rect.min[i] as f64 + rect.size[i] as f64 * 0.5)
        };
        let origin_center = center(origin);
        let mut forward: Option<(bool, f64, WidgetId)> = None;
        let mut wrapped: Option<(bool, f64, f64, WidgetId)> = None;
        for widget in &self.widgets {
            if Some(widget.id) == self.focus {
                continue;
            }
            let Some(rect) = widget.focus_rect() else {
                continue;
            };
            let candidate = center(rect);
            let along = sign * (candidate[axis] - origin_center[axis]);
            let across = candidate[cross] - origin_center[cross];
            let aligned = (rect.min[cross] as f64)
                < origin.min[cross] as f64 + origin.size[cross] as f64
                && (origin.min[cross] as f64) < rect.min[cross] as f64 + rect.size[cross] as f64;
            if along > 0. {
                let score = (!aligned, along * along + across * across, widget.id);
                if forward.is_none_or(|best| score < best) {
                    forward = Some(score);
                }
            } else if along < 0. && wrap == FocusWrap::Wrap {
                let score = (!aligned, sign * candidate[axis], across.abs(), widget.id);
                if wrapped.is_none_or(|best| score < best) {
                    wrapped = Some(score);
                }
            }
        }
        self.focus = forward
            .map(|score| score.2)
            .or_else(|| wrapped.map(|score| score.3))
            .or(self.focus);
        self.focus
    }
    pub fn activate_focused(&self) -> Option<WidgetId> {
        self.focus.filter(|id| {
            self.widgets
                .iter()
                .any(|w| w.id == *id && w.available() && w.focusable)
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn widget(id: u64) -> Widget {
        Widget {
            id,
            rect: Rect {
                min: [0.0; 2],
                size: [100.0; 2],
            },
            clip: None,
            enabled: true,
            focusable: true,
        }
    }
    #[test]
    fn anchors_resolve_stretch_and_center_after_resize() {
        let parent = Rect {
            min: [10.0, 20.0],
            size: [400.0, 200.0],
        };
        let center = Anchors {
            min: [0.5; 2],
            max: [0.5; 2],
            offset_min: [-50.0, -20.0],
            offset_max: [50.0, 20.0],
        }
        .resolve(parent)
        .unwrap();
        assert_eq!(
            center,
            Rect {
                min: [160.0, 100.0],
                size: [100.0, 40.0]
            }
        );
        let stretch = Anchors {
            min: [0.0; 2],
            max: [1.0; 2],
            offset_min: [5.0; 2],
            offset_max: [-5.0; 2],
        }
        .resolve(parent)
        .unwrap();
        assert_eq!(stretch.size, [390.0, 190.0]);
    }
    #[test]
    fn topmost_clipped_widget_captures_click_and_drag_out_cancels() {
        let mut ui = UiState::new();
        ui.add(widget(1)).unwrap();
        ui.add(Widget {
            clip: Some(Rect {
                min: [20.0; 2],
                size: [20.0; 2],
            }),
            ..widget(2)
        })
        .unwrap();
        assert_eq!(ui.hit_test([30.0; 2]), Some(2));
        assert_eq!(ui.hit_test([10.0; 2]), Some(1));
        ui.pointer_down([30.0; 2]);
        assert_eq!(ui.pointer_up([90.0; 2]), None);
        ui.pointer_down([30.0; 2]);
        assert_eq!(ui.pointer_up([30.0; 2]), Some(2));
    }
    #[test]
    fn keyboard_focus_skips_disabled_and_clears_removed_nodes() {
        let mut ui = UiState::new();
        ui.add(widget(1)).unwrap();
        ui.add(Widget {
            enabled: false,
            ..widget(2)
        })
        .unwrap();
        ui.add(widget(3)).unwrap();
        assert_eq!(ui.focus_next(false), Some(1));
        assert_eq!(ui.focus_next(false), Some(3));
        assert_eq!(ui.focus_next(false), Some(1));
        assert_eq!(ui.focus_next(true), Some(3));
        ui.begin_frame();
        ui.add(widget(1)).unwrap();
        ui.end_frame();
        assert_eq!(ui.focused(), None);
        assert_eq!(ui.add(widget(1)), Err(UiError::DuplicateId));
    }
}

#[cfg(test)]
mod directional_focus_tests {
    use super::*;
    use FocusDirection::{Down, Left, Right, Up};
    use FocusWrap::{Clamp, Wrap};

    fn widget(id: WidgetId, min: [f32; 2]) -> Widget {
        Widget {
            id,
            rect: Rect {
                min,
                size: [10.; 2],
            },
            clip: None,
            enabled: true,
            focusable: true,
        }
    }

    fn grid() -> UiState {
        let mut ui = UiState::with_capacity(9);
        for id in [9, 5, 2, 7, 1, 6, 3, 8, 4] {
            ui.add(widget(
                id,
                [((id - 1) % 3) as f32 * 20., ((id - 1) / 3) as f32 * 20.],
            ))
            .unwrap();
        }
        ui.end_frame();
        ui
    }

    #[test]
    fn all_four_directions_follow_geometry_instead_of_submission_order() {
        let mut ui = grid();
        assert_eq!(ui.pointer_down([25., 25.]), Some(5));
        for (direction, expected) in [
            (Right, 6),
            (Down, 9),
            (Left, 8),
            (Up, 5),
            (Up, 2),
            (Left, 1),
            (Down, 4),
            (Right, 5),
        ] {
            assert_eq!(ui.focus_direction(direction, Clamp), Some(expected));
        }
    }

    #[test]
    fn unavailable_widgets_are_skipped_in_both_directions_and_when_wrapping() {
        let mut ui = UiState::with_capacity(8);
        ui.add(widget(1, [0., 0.])).unwrap();
        ui.add(Widget {
            enabled: false,
            ..widget(2, [20., 0.])
        })
        .unwrap();
        ui.add(Widget {
            focusable: false,
            ..widget(3, [30., 0.])
        })
        .unwrap();
        for (id, size) in [(4, [0., 10.]), (5, [10., 0.])] {
            ui.add(Widget {
                rect: Rect {
                    min: [40., 0.],
                    size,
                },
                ..widget(id, [0.; 2])
            })
            .unwrap();
        }
        for (id, clip_x) in [(6, 100.), (7, 60.)] {
            ui.add(Widget {
                clip: Some(Rect {
                    min: [clip_x, 0.],
                    size: [10.; 2],
                }),
                ..widget(id, [50., 0.])
            })
            .unwrap();
        }
        ui.add(widget(8, [80., 0.])).unwrap();
        ui.end_frame();
        assert_eq!(ui.focus_next(false), Some(1));
        assert_eq!(ui.focus_direction(Right, Clamp), Some(8));
        assert_eq!(ui.focus_direction(Left, Clamp), Some(1));
        assert_eq!(ui.focus_direction(Left, Wrap), Some(8));
        assert_eq!(ui.focus_direction(Right, Wrap), Some(1));
    }

    #[test]
    fn origin_and_candidate_use_their_clipped_geometry() {
        let mut ui = UiState::with_capacity(3);
        for (id, x, width, clip_x) in [(1, 0., 200., 0.), (2, -100., 130., 20.)] {
            ui.add(Widget {
                rect: Rect {
                    min: [x, 0.],
                    size: [width, 10.],
                },
                clip: Some(Rect {
                    min: [clip_x, 0.],
                    size: [10.; 2],
                }),
                ..widget(id, [0.; 2])
            })
            .unwrap();
        }
        ui.add(widget(3, [40., 0.])).unwrap();
        assert_eq!(ui.pointer_down([5., 5.]), Some(1));
        assert_eq!(ui.focus_direction(Right, Clamp), Some(2));
        assert_eq!(ui.focus_direction(Left, Clamp), Some(1));
        assert_eq!(ui.focus_direction(Left, Wrap), Some(3));
    }

    #[test]
    fn overlapping_rows_rank_first_then_distance_with_diagonal_fallback() {
        let mut ui = UiState::with_capacity(4);
        for w in [
            widget(10, [0., 0.]),
            widget(1, [10., 10.]), // Touching rows do not overlap.
            widget(2, [80., 0.]),
            widget(3, [40., 0.]),
        ] {
            ui.add(w).unwrap();
        }
        ui.focus_next(false);
        assert_eq!(ui.focus_direction(Right, Clamp), Some(3));

        ui.begin_frame();
        for w in [
            widget(10, [0., 0.]),
            widget(1, [30., 10.]),
            widget(9, [10., 20.]),
        ] {
            ui.add(w).unwrap();
        }
        ui.end_frame();
        ui.focus_next(false);
        assert_eq!(ui.focus_direction(Right, Clamp), Some(9));
    }

    #[test]
    fn distance_ties_use_ids_even_after_paint_order_changes() {
        let mut ui = UiState::with_capacity(3);
        for ids in [[9, 3], [3, 9]] {
            ui.begin_frame();
            ui.add(widget(99, [0., 20.])).unwrap();
            for id in ids {
                ui.add(widget(id, [20., if id == 3 { 10. } else { 30. }]))
                    .unwrap();
            }
            ui.end_frame();
            ui.pointer_down([5., 25.]);
            assert_eq!(ui.focus_direction(Right, Clamp), Some(3));
        }
    }

    #[test]
    fn wrap_uses_opposite_edges_in_the_same_row_or_column() {
        let mut ui = grid();
        for (point, direction, clamped, wrapped) in [
            ([45., 25.], Right, 6, 4),
            ([5., 25.], Left, 4, 6),
            ([25., 5.], Up, 2, 8),
            ([25., 45.], Down, 8, 2),
        ] {
            ui.pointer_down(point);
            assert_eq!(ui.focus_direction(direction, Clamp), Some(clamped));
            assert_eq!(ui.focus_direction(direction, Wrap), Some(wrapped));
        }
    }

    #[test]
    fn wrap_ranks_alignment_extreme_edge_perpendicular_distance_then_id() {
        let mut ui = UiState::with_capacity(6);
        for reverse in [false, true] {
            let mut widgets = [
                widget(99, [100., 20.]),
                widget(8, [20., 20.]),
                widget(3, [0., 18.]),
                widget(2, [0., 22.]),
                widget(1, [0., 24.]),
                widget(7, [-100., 100.]),
            ];
            if reverse {
                widgets.reverse();
            }
            ui.begin_frame();
            for w in widgets {
                ui.add(w).unwrap();
            }
            ui.end_frame();
            ui.pointer_down([105., 25.]);
            assert_eq!(ui.focus_direction(Right, Wrap), Some(2));
        }
    }

    #[test]
    fn forward_candidate_prevents_wrap_even_when_it_is_not_aligned() {
        let mut ui = UiState::with_capacity(3);
        for w in [
            widget(1, [0., 0.]),
            widget(2, [20., 30.]),
            widget(3, [-30., 0.]),
        ] {
            ui.add(w).unwrap();
        }
        ui.focus_next(false);
        assert_eq!(ui.focus_direction(Right, Wrap), Some(2));
    }

    #[test]
    fn empty_singleton_equal_centers_and_missing_focus_are_well_defined() {
        for capacity in [0, 3] {
            let mut empty = UiState::with_capacity(capacity);
            for direction in [Left, Right, Up, Down] {
                for wrap in [Clamp, Wrap] {
                    assert_eq!(empty.focus_direction(direction, wrap), None);
                }
            }
        }
        let mut ui = UiState::with_capacity(3);
        ui.add(Widget {
            enabled: false,
            ..widget(1, [-20.; 2])
        })
        .unwrap();
        assert_eq!(ui.focus_direction(Down, Wrap), None);
        ui.add(widget(3, [0.; 2])).unwrap();
        // Missing focus starts in tab order regardless of direction/wrap policy.
        assert_eq!(ui.focus_direction(Up, Wrap), Some(3));
        for direction in [Left, Right, Up, Down] {
            for wrap in [Clamp, Wrap] {
                assert_eq!(ui.focus_direction(direction, wrap), Some(3));
            }
        }
        ui.add(widget(2, [0.; 2])).unwrap();
        for direction in [Left, Right, Up, Down] {
            assert_eq!(ui.focus_direction(direction, Wrap), Some(3));
        }
        assert_eq!(ui.focus_next(false), Some(2)); // Tab still reaches equal centers.
    }

    #[test]
    fn rebuilds_use_new_positions_and_clear_or_recover_unavailable_focus() {
        let mut ui = UiState::with_capacity(2);
        ui.add(widget(1, [0.; 2])).unwrap();
        ui.add(widget(2, [20., 0.])).unwrap();
        ui.pointer_down([5.; 2]);
        ui.begin_frame();
        ui.add(widget(2, [0., 20.])).unwrap();
        ui.add(widget(1, [0.; 2])).unwrap();
        ui.end_frame();
        assert_eq!(ui.focused(), Some(1));
        assert_eq!(ui.focus_direction(Right, Wrap), Some(1));
        assert_eq!(ui.focus_direction(Down, Clamp), Some(2));
        for unavailable in [
            Widget {
                enabled: false,
                ..widget(2, [20., 0.])
            },
            Widget {
                focusable: false,
                ..widget(2, [20., 0.])
            },
            Widget {
                clip: Some(Rect {
                    min: [100.; 2],
                    size: [10.; 2],
                }),
                ..widget(2, [20., 0.])
            },
        ] {
            ui.begin_frame();
            ui.add(unavailable).unwrap();
            ui.add(widget(1, [0.; 2])).unwrap();
            // Defensive recovery also handles an ineligible ID before end_frame.
            assert_eq!(ui.focus_direction(Left, Clamp), Some(1));
            ui.end_frame();
            ui.begin_frame();
            ui.add(widget(1, [0.; 2])).unwrap();
            ui.add(widget(2, [20., 0.])).unwrap();
            ui.end_frame();
            assert_eq!(ui.focus_direction(Right, Clamp), Some(2));
        }
        ui.begin_frame();
        ui.add(widget(1, [0.; 2])).unwrap();
        assert_eq!(ui.focus_direction(Up, Wrap), Some(1)); // Removed ID.
        ui.begin_frame();
        ui.end_frame();
        assert_eq!(ui.focused(), None);
        assert_eq!(ui.captured(), None);
        assert_eq!(ui.focus_direction(Right, Wrap), None);
    }

    #[test]
    fn navigation_preserves_capture_tab_order_hit_ownership_and_storage() {
        let mut ui = UiState::with_capacity(4);
        let storage = ui.widgets.as_ptr();
        for w in [
            widget(1, [0.; 2]),
            widget(2, [40., 0.]),
            widget(3, [20., 0.]),
        ] {
            ui.add(w).unwrap();
        }
        ui.add(Widget {
            enabled: false,
            ..widget(4, [40., 0.])
        })
        .unwrap();
        assert_eq!(ui.pointer_down([5.; 2]), Some(1));
        assert_eq!(ui.focus_direction(Right, Clamp), Some(3));
        assert_eq!(ui.captured(), Some(1));
        assert_eq!(ui.activate_focused(), Some(3));
        assert_eq!(ui.pointer_up([5.; 2]), Some(1));
        assert_eq!(ui.focus_next(false), Some(1));
        assert_eq!(ui.focus_next(true), Some(3));
        assert_eq!(ui.focus_direction(Right, Clamp), Some(2));
        assert_eq!(ui.hit_test([45., 5.]), None); // Disabled overlay still owns the hit.
        assert_eq!(ui.activate_focused(), Some(2)); // Occlusion does not alter tab eligibility.
        assert_eq!(ui.widgets.as_ptr(), storage);
        assert_eq!(ui.capacity(), 4);
        assert_eq!(ui.add(widget(5, [60., 0.])), Err(UiError::Capacity));
    }

    #[test]
    fn extreme_and_tiny_finite_coordinates_keep_distance_ordering() {
        for scale in [f32::MAX / 4., f32::from_bits(1)] {
            let mut ui = UiState::with_capacity(3);
            for (id, x) in [(5, -3. * scale), (1, 2. * scale), (9, 0.)] {
                ui.add(Widget {
                    rect: Rect {
                        min: [x, 0.],
                        size: [scale, 10.],
                    },
                    ..widget(id, [0.; 2])
                })
                .unwrap();
            }
            ui.focus_next(false);
            assert_eq!(ui.focus_direction(Right, Clamp), Some(9));
            assert_eq!(ui.focus_direction(Right, Clamp), Some(1));
            assert_eq!(ui.focus_direction(Right, Wrap), Some(5));
        }
    }
}

/// Logical UI units mapped into a safe-area-aware pixel viewport.
/// [`Self::fit`] letterboxes a design canvas; [`Self::responsive`] keeps one
/// logical unit per host logical pixel and lets layout reflow.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    bounds: Rect,
    pixels: Rect,
    scale: f32,
}
impl Viewport {
    pub fn bounds(self) -> Rect {
        self.bounds
    }
    pub fn pixels(self) -> Rect {
        self.pixels
    }
    pub fn scale(self) -> f32 {
        self.scale
    }
    /// A HUD viewport with no font scaling or letterboxing. Insets are left, top,
    /// right, bottom in host logical pixels; the host handles device pixel ratio.
    /// Layout coordinates start at zero inside the safe area. Oversized insets
    /// collapse their axis proportionally; zero-sized screens are valid and inert.
    /// Negative/non-finite dimensions or insets return [`UiError::InvalidLayout`].
    ///
    /// ```
    /// use ferravex_play::ui::Viewport;
    /// let hud = Viewport::responsive([320., 240.], [10., 20., 10., 0.])?;
    /// assert_eq!(hud.bounds().size, [300., 220.]);
    /// assert_eq!(hud.unproject([10., 20.]), Some([0., 0.]));
    /// assert!(Viewport::responsive([0.; 2], [12.; 4])?.unproject([0.; 2]).is_none());
    /// # Ok::<(), ferravex_play::ui::UiError>(())
    /// ```
    pub fn responsive(screen: [f32; 2], insets: [f32; 4]) -> Result<Self, UiError> {
        let pixels = Rect {
            min: [0.; 2],
            size: screen,
        }
        .inset(insets)?;
        Ok(Self {
            bounds: Rect {
                min: [0.; 2],
                size: pixels.size,
            },
            pixels,
            scale: 1.,
        })
    }
    /// Insets are left, top, right, bottom in host logical pixels (DPR applied by host).
    pub fn fit(screen: [f32; 2], design: [f32; 2], insets: [f32; 4]) -> Result<Self, UiError> {
        if !screen
            .into_iter()
            .chain(design)
            .all(|v| v.is_finite() && v > 0.)
            || !insets.into_iter().all(|v| v.is_finite() && v >= 0.)
        {
            return Err(UiError::InvalidLayout);
        }
        let available = [
            screen[0] - insets[0] - insets[2],
            screen[1] - insets[1] - insets[3],
        ];
        let scale = (available[0] / design[0]).min(available[1] / design[1]);
        if !scale.is_finite() || scale <= 0. {
            return Err(UiError::InvalidLayout);
        }
        let size = [design[0] * scale, design[1] * scale];
        let viewport = Self {
            bounds: Rect {
                min: [0.; 2],
                size: design,
            },
            pixels: Rect {
                min: [
                    insets[0] + (available[0] - size[0]) * 0.5,
                    insets[1] + (available[1] - size[1]) * 0.5,
                ],
                size,
            },
            scale,
        };
        if !viewport.bounds.valid() || !viewport.pixels.valid() {
            return Err(UiError::InvalidLayout);
        }
        Ok(viewport)
    }
    pub fn unproject(self, point: [f32; 2]) -> Option<[f32; 2]> {
        self.pixels
            .contains(point)
            .then(|| std::array::from_fn(|i| (point[i] - self.pixels.min[i]) / self.scale))
    }
}
/// Equal-width/height slots without temporary layout allocations.
pub fn slots(
    parent: Rect,
    count: usize,
    gap: f32,
    vertical: bool,
) -> Result<impl Iterator<Item = Rect>, UiError> {
    let axis = usize::from(vertical);
    if !parent.valid()
        || count == 0
        || count > 4096
        || !gap.is_finite()
        || gap < 0.
        || gap * (count - 1) as f32 > parent.size[axis]
    {
        return Err(UiError::InvalidLayout);
    }
    let extent = (parent.size[axis] - gap * (count - 1) as f32) / count as f32;
    Ok((0..count).map(move |i| {
        let mut r = parent;
        r.min[axis] += (extent + gap) * i as f32;
        r.size[axis] = extent;
        r
    }))
}

/// Reflow equal-width HUD cells left-to-right, then top-to-bottom, without allocation.
/// Each row fits as many `min_width` cells as possible (at least one), distributing
/// spare width equally. Rows keep `height`; a narrow viewport shrinks the one
/// column. The last row keeps the same column widths, even when not full.
/// Cells clip to `parent`, so overflow rows become empty, non-interactive rectangles.
/// This never scales text: combine with [`fit_label`] for long labels.
///
/// `count` may be zero and must not exceed 4096. Width/height must be finite and
/// positive; gap must be finite and nonnegative.
///
/// ```
/// use ferravex_play::ui::{hud_slots, Rect};
/// let area = Rect { min: [12., 12.], size: [300., 104.] };
/// let mut cells = hud_slots(area, 3, 140., 44., 8.)?;
/// assert_eq!(cells.next().unwrap().size, [146., 44.]);
/// assert_eq!(cells.nth(1).unwrap().min, [12., 64.]); // Third item wraps.
/// # Ok::<(), ferravex_play::ui::UiError>(())
/// ```
pub fn hud_slots(
    parent: Rect,
    count: usize,
    min_width: f32,
    height: f32,
    gap: f32,
) -> Result<impl ExactSizeIterator<Item = Rect>, UiError> {
    if !parent.valid()
        || count > 4096
        || ![min_width, height]
            .into_iter()
            .all(|v| v.is_finite() && v > 0.)
        || !gap.is_finite()
        || gap < 0.
    {
        return Err(UiError::InvalidLayout);
    }
    let gap = gap as f64;
    let width = parent.size[0] as f64;
    let columns =
        (((width + gap) / (min_width as f64 + gap)).floor() as usize).clamp(1, count.max(1));
    let cell_width = ((width - gap * (columns - 1) as f64) / columns as f64).max(0.);
    Ok((0..count).map(move |index| {
        let x = ((cell_width + gap) * (index % columns) as f64).min(width);
        let y = ((height as f64 + gap) * (index / columns) as f64).min(parent.size[1] as f64);
        Rect {
            min: [parent.min[0] + x as f32, parent.min[1] + y as f32],
            size: [
                cell_width.min(width - x) as f32,
                (height as f64).min(parent.size[1] as f64 - y) as f32,
            ],
        }
    }))
}

/// A borrowed, single-line prefix and static ASCII ellipsis. Draw the two pieces
/// consecutively at the original font size; no temporary `String` is needed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FittedLabel<'a> {
    pub prefix: &'a str,
    pub suffix: &'static str,
    pub width: f32,
    pub truncated: bool,
}

/// Fit monospaced text without shrinking its glyphs or allocating.
/// `advance` is the positive, finite glyph advance, not the font height. The
/// renderer's bitmap font uses `size * 0.75`. Unicode scalar values count as one
/// glyph (unsupported glyphs are rendered as `?`); UTF-8 is never split.
/// Newlines end the label and signal truncation. A width fitting fewer than three
/// glyphs shows only as many dots as fit; a zero width returns empty pieces.
/// Work is bounded by the visible prefix, even for very long input strings.
///
/// ```
/// use ferravex_play::ui::fit_label;
/// let label = fit_label("COLLECT CRYSTALS", 90., 10.)?;
/// assert_eq!((label.prefix, label.suffix), ("COLLEC", "..."));
/// assert_eq!(label.width, 90.);
/// # Ok::<(), ferravex_play::ui::UiError>(())
/// ```
pub fn fit_label(text: &str, width: f32, advance: f32) -> Result<FittedLabel<'_>, UiError> {
    if !width.is_finite() || width < 0. || !advance.is_finite() || advance <= 0. {
        return Err(UiError::InvalidLayout);
    }
    let columns = (width as f64 / advance as f64).floor() as usize;
    let mut count = 0;
    let mut end = 0;
    let mut truncated = false;
    for (index, ch) in text.char_indices() {
        if count == columns || ch == '\n' || ch == '\r' {
            truncated = true;
            break;
        }
        count += 1;
        end = index + ch.len_utf8();
    }
    let suffix = if truncated {
        let dots = columns.min(3);
        let keep = count.min(columns - dots);
        end = text[..end].char_indices().nth(keep).map_or(end, |(i, _)| i);
        count = keep + dots;
        &"..."[..dots]
    } else {
        ""
    };
    Ok(FittedLabel {
        prefix: &text[..end],
        suffix,
        width: (count as f64 * advance as f64) as f32,
        truncated,
    })
}

#[cfg(test)]
mod bounds_tests {
    use super::*;
    fn widget(id: u64) -> Widget {
        Widget {
            id,
            rect: Rect {
                min: [0.; 2],
                size: [100.; 2],
            },
            clip: None,
            enabled: true,
            focusable: true,
        }
    }
    #[test]
    fn disabled_overlay_occludes_and_invisible_widgets_cannot_focus() {
        let mut ui = UiState::with_capacity(3);
        ui.add(widget(1)).unwrap();
        ui.add(Widget {
            enabled: false,
            ..widget(2)
        })
        .unwrap();
        assert_eq!(ui.hit_test([10.; 2]), None);
        ui.add(Widget {
            clip: Some(Rect {
                min: [200.; 2],
                size: [10.; 2],
            }),
            ..widget(3)
        })
        .unwrap();
        assert_eq!(ui.focus_next(false), Some(1));
        assert_eq!(ui.focus_next(false), Some(1));
        assert_eq!(ui.add(widget(4)), Err(UiError::Capacity));
    }
    #[test]
    fn viewport_safe_area_letterbox_edges_and_layout_limits() {
        let v = Viewport::fit([1000., 700.], [800., 400.], [20., 30., 40., 50.]).unwrap();
        assert_eq!(v.scale, 1.175);
        assert!((v.pixels.min[0] - 20.).abs() < 0.001 && (v.pixels.min[1] - 105.).abs() < 0.001);
        assert_eq!(v.unproject(v.pixels.min), Some([0.; 2]));
        assert_eq!(
            v.unproject([v.pixels.min[0] + v.pixels.size[0], 200.]),
            None
        );
        assert!(Viewport::fit([100.; 2], [0.; 2], [0.; 4]).is_err());
        assert!(Viewport::fit([100.; 2], [1.; 2], [60.; 4]).is_err());
        let parent = Rect {
            min: [10., 20.],
            size: [100., 40.],
        };
        let row: Vec<_> = slots(parent, 3, 5., false).unwrap().collect();
        assert_eq!(
            row[2],
            Rect {
                min: [80., 20.],
                size: [30., 40.]
            }
        );
        assert!(slots(parent, 0, 0., false).is_err());
        assert!(slots(parent, 3, 51., false).is_err());
    }
}

#[cfg(test)]
mod responsive_tests {
    use super::*;

    #[test]
    fn safe_area_preserves_pixel_size_and_handles_collapsed_viewports() {
        let viewport = Viewport::responsive([320., 240.], [10., 20., 30., 40.]).unwrap();
        assert_eq!(viewport.scale(), 1.);
        assert_eq!(
            viewport.bounds(),
            Rect {
                min: [0.; 2],
                size: [280., 180.]
            }
        );
        assert_eq!(
            viewport.pixels(),
            Rect {
                min: [10., 20.],
                size: [280., 180.]
            }
        );
        assert_eq!(viewport.unproject([10., 20.]), Some([0.; 2]));
        assert_eq!(viewport.unproject([289., 199.]), Some([279., 179.]));
        for point in [[9., 20.], [10., 19.], [290., 20.], [10., 200.]] {
            assert_eq!(viewport.unproject(point), None);
        }
        let collapsed = Viewport::responsive([100., 60.], [80., 20., 80., 60.]).unwrap();
        assert_eq!(
            collapsed.pixels(),
            Rect {
                min: [50., 15.],
                size: [0.; 2]
            }
        );
        assert_eq!(collapsed.unproject([50., 15.]), None);
        for screen in [[0.; 2], [0., 100.], [100., 0.], [1.; 2]] {
            let tiny = Viewport::responsive(screen, [f32::MAX; 4]).unwrap();
            assert!(tiny.bounds().valid() && tiny.pixels().valid());
            assert_eq!(tiny.bounds().size, [0.; 2]);
            assert_eq!(tiny.unproject(tiny.pixels().min), None);
        }
        for invalid in [-1., f32::NAN, f32::INFINITY] {
            assert!(Viewport::responsive([invalid, 100.], [0.; 4]).is_err());
            assert!(Viewport::responsive([100.; 2], [invalid, 0., 0., 0.]).is_err());
        }
    }

    #[test]
    fn all_anchors_keep_requested_and_oversized_panels_inside_parent() {
        let parent = Rect {
            min: [10., 20.],
            size: [100., 60.],
        };
        for (anchor, min) in [
            (Anchor::TopLeft, [10., 20.]),
            (Anchor::TopCenter, [50., 20.]),
            (Anchor::TopRight, [90., 20.]),
            (Anchor::CenterLeft, [10., 40.]),
            (Anchor::Center, [50., 40.]),
            (Anchor::CenterRight, [90., 40.]),
            (Anchor::BottomLeft, [10., 60.]),
            (Anchor::BottomCenter, [50., 60.]),
            (Anchor::BottomRight, [90., 60.]),
        ] {
            assert_eq!(
                anchor.place(parent, [20.; 2]).unwrap(),
                Rect {
                    min,
                    size: [20.; 2]
                }
            );
            assert_eq!(anchor.place(parent, [200.; 2]).unwrap(), parent);
            let empty = Rect {
                size: [0.; 2],
                ..parent
            };
            assert_eq!(anchor.place(empty, [20.; 2]).unwrap(), empty);
        }
        assert!(Anchor::Center.place(parent, [f32::NAN, 20.]).is_err());
        assert!(Anchor::Center.place(parent, [-1., 20.]).is_err());
    }

    #[test]
    fn hud_cells_reflow_clip_and_reject_invalid_dimensions() {
        let parent = Rect {
            min: [12.; 2],
            size: [300., 96.],
        };
        let cells: Vec<_> = hud_slots(parent, 5, 140., 44., 8.).unwrap().collect();
        assert_eq!(
            cells[0],
            Rect {
                min: [12.; 2],
                size: [146., 44.]
            }
        );
        assert_eq!(
            cells[1],
            Rect {
                min: [166., 12.],
                size: [146., 44.]
            }
        );
        assert_eq!(
            cells[2],
            Rect {
                min: [12., 64.],
                size: [146., 44.]
            }
        );
        assert_eq!(cells[4].size, [146., 0.]);
        let narrow = Rect {
            size: [100., 30.],
            ..parent
        };
        let mut cells = hud_slots(narrow, 3, 140., 44., 8.).unwrap();
        assert_eq!(cells.len(), 3);
        assert_eq!(cells.next().unwrap(), narrow);
        assert!(cells.all(|r| r.valid() && r.size[1] == 0.));
        let zero = Rect {
            min: [0.; 2],
            size: [0.; 2],
        };
        assert!(
            hud_slots(zero, 3, 140., 44., 8.)
                .unwrap()
                .all(|r| r == zero)
        );
        assert_eq!(hud_slots(parent, 0, 140., 44., 8.).unwrap().len(), 0);
        assert!(hud_slots(parent, 4097, 140., 44., 8.).is_err());
        assert!(hud_slots(parent, 2, 0., 44., 8.).is_err());
        assert!(hud_slots(parent, 2, 140., f32::NAN, 8.).is_err());
        assert!(hud_slots(parent, 2, 140., 44., -1.).is_err());
        assert!(
            hud_slots(parent, 2, f32::MAX, f32::MAX, f32::MAX)
                .unwrap()
                .all(Rect::valid)
        );
    }

    #[test]
    fn resized_hud_hit_regions_focus_and_capture_use_visible_geometry() {
        let mut ui = UiState::with_capacity(3);
        let storage = ui.widgets.as_ptr();
        let build = |ui: &mut UiState, width, height| {
            let viewport = Viewport::responsive([width, height], [10., 20., 10., 0.]).unwrap();
            ui.begin_frame();
            for (index, rect) in hud_slots(viewport.bounds(), 3, 100., 44., 8.)
                .unwrap()
                .enumerate()
            {
                ui.add(Widget {
                    id: index as u64 + 1,
                    rect,
                    clip: Some(viewport.bounds()),
                    enabled: true,
                    focusable: true,
                })
                .unwrap();
            }
            ui.end_frame();
            viewport
        };
        let viewport = build(&mut ui, 228., 64.);
        assert_eq!(
            ui.hit_test(viewport.unproject([10., 20.]).unwrap()),
            Some(1)
        );
        assert_eq!(ui.hit_test([100., 10.]), None); // Gap, not an enlarged label hit box.
        assert_eq!(ui.pointer_down([108., 10.]), Some(2));
        assert_eq!(ui.focus_next(false), Some(1)); // Third row is invisible.
        ui.focus_next(false);
        build(&mut ui, 120., 116.); // Same IDs now stack vertically.
        assert_eq!(ui.focused(), Some(2));
        assert_eq!(ui.pointer_up([108., 10.]), None); // Old position cannot activate it.
        assert_eq!(ui.pointer_down([10., 60.]), Some(2));
        assert_eq!(ui.pointer_up([10., 60.]), Some(2));
        ui.pointer_down([10., 60.]);
        build(&mut ui, 0., 0.);
        assert_eq!(ui.focused(), None);
        assert_eq!(ui.captured(), None);
        assert_eq!(ui.focus_next(false), None);
        assert_eq!(ui.hit_test([0.; 2]), None);
        assert_eq!(ui.widgets.as_ptr(), storage); // Rebuild never replaces storage.
    }

    #[test]
    fn adjacent_responsive_cells_have_one_hit_owner() {
        let mut ui = UiState::with_capacity(2);
        for (index, rect) in hud_slots(
            Rect {
                min: [0.; 2],
                size: [200., 44.],
            },
            2,
            100.,
            44.,
            0.,
        )
        .unwrap()
        .enumerate()
        {
            ui.add(Widget {
                id: index as u64,
                rect,
                clip: None,
                enabled: true,
                focusable: true,
            })
            .unwrap();
        }
        assert_eq!(ui.hit_test([99.999, 10.]), Some(0));
        assert_eq!(ui.hit_test([100., 10.]), Some(1));
        assert_eq!(ui.hit_test([200., 10.]), None);
    }

    #[test]
    fn readable_labels_preserve_utf8_and_never_shrink_or_exceed_width() {
        for (text, width, prefix, suffix, truncated) in [
            ("é日ABC", 50., "é日ABC", "", false),
            ("é日ABC", 40., "é", "...", true),
            ("é\nAB", 60., "é", "...", true),
            ("A\r\nB", 60., "A", "...", true),
            ("LONG", 20., "", "..", true),
            ("LONG", 10., "", ".", true),
            ("LONG", 9., "", "", true),
            ("LONG", 0., "", "", true),
            ("", 0., "", "", false),
        ] {
            let fit = fit_label(text, width, 10.).unwrap();
            assert_eq!(
                (fit.prefix, fit.suffix, fit.truncated),
                (prefix, suffix, truncated)
            );
            assert_eq!(
                fit.width,
                (prefix.chars().count() + suffix.len()) as f32 * 10.
            );
            assert!(fit.width <= width);
            assert_eq!(fit.prefix.as_ptr(), text.as_ptr());
        }
        let long = "é".repeat(100_000);
        assert_eq!(fit_label(&long, 50., 10.).unwrap().prefix, "éé");
        for invalid in [-1., f32::NAN, f32::INFINITY] {
            assert!(fit_label("TEXT", invalid, 10.).is_err());
            assert!(fit_label("TEXT", 100., invalid).is_err());
        }
        assert!(fit_label("TEXT", 100., 0.).is_err());
    }
}
