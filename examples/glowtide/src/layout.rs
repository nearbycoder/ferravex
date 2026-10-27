//! Responsive screen layout shared by painting and input.
use ferravex_render::input::touch::{Control, Output, Region};
use ferravex_render::prelude::{Rect, Vec2, vec2};
use ferravex_render::ui::{UiRect, Viewport};

/// Top-right CSS-px region owned by the browser fullscreen menu button.
pub const CORNER: Vec2 = Vec2::new(72., 60.);

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
        self.screen.x - CORNER.x - 12.
    }

    pub fn dash_button(&self) -> (Vec2, f32) {
        let r = if self.compact { 44. } else { 52. };
        let margin = if self.short { 22. } else { 34. };
        (
            vec2(
                self.screen.x - r - margin,
                self.screen.y - r - margin - if self.portrait { 30. } else { 0. },
            ),
            r,
        )
    }

    pub fn pause_button(&self) -> UiRect {
        UiRect {
            min: [8., 8.],
            size: [44., 40.],
        }
    }

    /// Dash button first so it wins overlapping touches; the floating stick
    /// accepts a touch anywhere else on the play field.
    pub fn touch_controls(&self) -> [Control; 2] {
        let (center, radius) = self.dash_button();
        let top = 56.;
        [
            Control {
                region: Region::Circle {
                    center,
                    radius: radius + 12.,
                },
                output: Output::Button(7),
            },
            Control {
                region: Region::Rect(Rect::new(
                    0.,
                    top,
                    self.screen.x,
                    (self.screen.y - top).max(1.),
                )),
                output: Output::Stick {
                    x: 0,
                    y: 1,
                    radius: 56.,
                    floating: true,
                },
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_corner_and_touch_controls_fit_every_target_size() {
        for size in [
            [320., 480.],
            [1280., 800.],
            [1600., 1000.],
            [390., 844.],
            [844., 390.],
        ] {
            let l = Layout::new(size);
            assert!(l.top_right() <= size[0] - CORNER.x);
            let (center, r) = l.dash_button();
            assert!(center.x + r <= size[0] && center.y + r <= size[1]);
            assert!(
                center.y - r > CORNER.y,
                "dash button stays below the corner"
            );
            let pause = l.pause_button();
            assert!(pause.min[0] + pause.size[0] < size[0] - CORNER.x);
            assert_eq!(l.touch_controls().len(), 2);
        }
        assert!(Layout::new([390., 844.]).compact && Layout::new([390., 844.]).portrait);
        assert!(Layout::new([844., 390.]).short);
        assert!(!Layout::new([1600., 1000.]).compact);
    }
}
