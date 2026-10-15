//! Renderer-independent anchored rectangles, clipped hit testing, pointer capture,
//! and keyboard focus. Rendering and accessibility adapters remain host concerns.
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
    fn contains(self, point: [f32; 2]) -> bool {
        self.enabled
            && self.rect.contains(point)
            && self.clip.is_none_or(|clip| clip.contains(point))
    }
}
#[derive(Default)]
pub struct UiState {
    widgets: Vec<Widget>,
    focus: Option<WidgetId>,
    pressed: Option<WidgetId>,
}
impl UiState {
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
        self.widgets.push(widget);
        Ok(())
    }
    /// Call after layout, before delivering input, to discard removed/disabled IDs.
    pub fn end_frame(&mut self) {
        if self.focus.is_some_and(|id| {
            !self
                .widgets
                .iter()
                .any(|w| w.id == id && w.enabled && w.focusable)
        }) {
            self.focus = None;
        }
        if self
            .pressed
            .is_some_and(|id| !self.widgets.iter().any(|w| w.id == id && w.enabled))
        {
            self.pressed = None;
        }
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
            if w.enabled && w.focusable {
                self.focus = Some(w.id);
                return self.focus;
            }
        }
        self.focus = None;
        None
    }
    pub fn activate_focused(&self) -> Option<WidgetId> {
        self.focus.filter(|id| {
            self.widgets
                .iter()
                .any(|w| w.id == *id && w.enabled && w.focusable)
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
