//! Responsive screen layout shared by painting, touch hit-testing and menus.
//!
//! The top-right 72 × 60 CSS-pixel corner belongs to the browser's
//! fullscreen menu, so nothing interactive (or important) lives there.
use ferravex_render::input::touch::{Control, Output, Region};
use ferravex_render::prelude::{Rect, Vec2, vec2};
use ferravex_render::ui::{UiRect, Viewport};

pub const CORNER: Vec2 = Vec2::new(72., 60.);

/// Touch codes (`InputCode::Touch(n)`) owned by this game's actions.
pub const TOUCH_JUMP: u8 = 3;
pub const TOUCH_GRAB: u8 = 4;
pub const TOUCH_THROW: u8 = 5;
pub const TOUCH_CHITTER: u8 = 6;
pub const TOUCH_SLIDE: u8 = 7;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Button {
    pub center: Vec2,
    pub radius: f32,
    pub code: u8,
    pub label: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub screen: Vec2,
    pub compact: bool,
    pub short: bool,
    pub portrait: bool,
    pub viewport: Viewport,
}

impl Layout {
    pub fn new(screen: [f32; 2]) -> Self {
        let screen = Vec2::from(screen.map(|v| {
            if v.is_finite() {
                v.clamp(1., 65536.)
            } else {
                1.
            }
        }));
        let compact = screen.x < 760. || screen.y < 520.;
        let short = screen.y < 520.;
        let portrait = screen.y > screen.x * 1.2;
        let viewport = Viewport::responsive(screen.to_array(), [0.; 4]).expect("finite screen");
        Self {
            screen,
            compact,
            short,
            portrait,
            viewport,
        }
    }

    /// Right edge for top-row HUD items that must avoid the fullscreen corner.
    pub fn top_right(&self) -> f32 {
        self.screen.x - CORNER.x - 10.
    }

    pub fn pause_button(&self) -> UiRect {
        UiRect {
            min: [10., 10.],
            size: [44., 44.],
        }
    }

    /// Collapsed notepad tab (compact screens) or the full notepad's header.
    pub fn notes_tab(&self) -> UiRect {
        let left = if self.compact { 62. } else { 12. };
        let top = if self.compact { 10. } else { 62. };
        UiRect {
            min: [left, top],
            size: [if self.compact { 132. } else { 150. }, 44.],
        }
    }

    /// The expanded notepad page, sized for `rows` list lines.
    pub fn notes_page_for(&self, rows: usize) -> Rect {
        let full = self.notes_page();
        let row = if self.short { 26. } else { 30. };
        let head = if self.short { 46. } else { 54. };
        let foot = if self.short { 34. } else { 44. };
        let want = head + rows as f32 * row + foot + 6.;
        Rect::new(full.x, full.y, full.w, want.min(full.h))
    }

    /// The largest expanded notepad page. On small screens it stops above the
    /// touch buttons so the list never hides the controls.
    pub fn notes_page(&self) -> Rect {
        let w = if self.compact {
            (self.screen.x - 24.).min(340.)
        } else {
            (self.screen.x * 0.3).clamp(290., 360.)
        };
        let top = 62.;
        let floor = if self.compact {
            self.buttons()
                .iter()
                .map(|b| b.center.y - b.radius)
                .fold(self.screen.y, f32::min)
                - 10.
        } else {
            self.screen.y - 24.
        };
        let h = (floor - top).min(470.);
        Rect::new(12., top, w, h.max(120.))
    }

    pub fn buttons(&self) -> [Button; 5] {
        let s = if self.compact { 0.86 } else { 1. };
        let margin = if self.short { 14. } else { 26. };
        let w = self.screen.x;
        let h = self.screen.y - if self.portrait { 26. } else { 0. };
        let jump = vec2(w - margin - 46. * s, h - margin - 46. * s);
        [
            Button {
                center: jump,
                radius: 46. * s,
                code: TOUCH_JUMP,
                label: "Jump",
            },
            Button {
                center: jump + vec2(-104., 18.) * s,
                radius: 38. * s,
                code: TOUCH_GRAB,
                label: "Grab",
            },
            Button {
                center: jump + vec2(-12., -100.) * s,
                radius: 30. * s,
                code: TOUCH_THROW,
                label: "Throw",
            },
            Button {
                center: jump + vec2(-90., -78.) * s,
                radius: 30. * s,
                code: TOUCH_CHITTER,
                label: "Chitter",
            },
            Button {
                center: jump + vec2(-182., 26.) * s,
                radius: 30. * s,
                code: TOUCH_SLIDE,
                label: "Slide",
            },
        ]
    }

    /// Buttons first so they win overlapping touches; the floating stick
    /// owns the lower-left play field, and dragging elsewhere orbits the camera.
    pub fn touch_controls(&self) -> [Control; 7] {
        let b = self.buttons();
        let top = 60.;
        let split = self.screen.x * if self.portrait { 0.55 } else { 0.45 };
        let button = |k: usize| Control {
            region: Region::Circle {
                center: b[k].center,
                radius: b[k].radius + 8.,
            },
            output: Output::Button(b[k].code),
        };
        [
            button(0),
            button(1),
            button(2),
            button(3),
            button(4),
            Control {
                region: Region::Rect(Rect::new(0., top, split, (self.screen.y - top).max(1.))),
                output: Output::Stick {
                    x: 0,
                    y: 1,
                    radius: 54.,
                    floating: true,
                },
            },
            Control {
                region: Region::Rect(Rect::new(
                    split,
                    top,
                    (self.screen.x - split).max(1.),
                    (self.screen.y - top).max(1.),
                )),
                output: Output::Look { x: 0, y: 1 },
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corner_and_touch_controls_fit_every_target_size() {
        for size in [
            [320., 480.],
            [1280., 800.],
            [1600., 1000.],
            [390., 844.],
            [844., 390.],
        ] {
            let l = Layout::new(size);
            assert!(l.top_right() <= size[0] - CORNER.x);
            for b in l.buttons() {
                assert!(b.center.x - b.radius >= 0., "{size:?} {b:?}");
                assert!(b.center.x + b.radius <= size[0] && b.center.y + b.radius <= size[1]);
                assert!(
                    b.center.y - b.radius > CORNER.y,
                    "buttons stay below the corner"
                );
            }
            let pause = l.pause_button();
            assert!(pause.min[0] + pause.size[0] < size[0] - CORNER.x);
            let tab = l.notes_tab();
            assert!(tab.min[0] + tab.size[0] < size[0] - CORNER.x);
            let page = l.notes_page();
            assert!(page.x + page.w <= size[0] && page.y + page.h <= size[1]);
            assert!(page.y >= CORNER.y || page.x + page.w <= size[0] - CORNER.x);
            assert_eq!(l.touch_controls().len(), 7);
        }
        assert!(Layout::new([390., 844.]).compact && Layout::new([390., 844.]).portrait);
        assert!(Layout::new([844., 390.]).short);
        assert!(!Layout::new([1600., 1000.]).compact);
    }
}
