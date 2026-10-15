//! Aspect-preserving 2D canvas with one coordinate transform for drawing and input.
use crate::prelude::*;

/// Fits a Y-down logical canvas into a logical-pixel screen, centered with letterboxing.
/// Uses camera zoom rather than a raw GPU viewport, so high-DPI screens behave identically.
#[derive(Debug, Clone, Copy)]
pub struct Canvas2D {
    pub size: Vec2,
    pub screen: Vec2,
    pub scale: f32,
    pub offset: Vec2,
}

impl Canvas2D {
    pub fn fit(size: Vec2, screen: Vec2) -> Self {
        assert!(
            size.is_finite()
                && screen.is_finite()
                && size.min_element() > 0.0
                && screen.min_element() > 0.0,
            "canvas and screen dimensions must be finite and positive"
        );
        let scale = (screen / size).min_element();
        Self {
            size,
            screen,
            scale,
            offset: (screen - size * scale) * 0.5,
        }
    }

    pub fn project(self, point: Vec2) -> Vec2 {
        self.offset + point * self.scale
    }
    pub fn unproject(self, point: Vec2) -> Vec2 {
        (point - self.offset) / self.scale
    }

    /// Returns None for input in the letterbox margin, outside the playable canvas.
    pub fn pointer(self, point: Vec2) -> Option<Vec2> {
        let p = self.unproject(point);
        (p.is_finite() && p.x >= 0.0 && p.y >= 0.0 && p.x <= self.size.x && p.y <= self.size.y)
            .then_some(p)
    }

    pub fn camera(self) -> Camera2D {
        Camera2D {
            target: self.size * 0.5,
            // Macroquad applies its screen Y flip internally.
            zoom: Vec2::splat(2.0 * self.scale) / self.screen,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_tracks_letterboxed_drawing_after_resize() {
        for screen in [
            vec2(1280.0, 800.0),
            vec2(390.0, 844.0),
            vec2(2560.0, 1440.0),
        ] {
            let canvas = Canvas2D::fit(vec2(1280.0, 800.0), screen);
            for point in [Vec2::ZERO, vec2(640.0, 400.0), vec2(1280.0, 800.0)] {
                assert!(canvas.unproject(canvas.project(point)).distance(point) < 0.001);
            }
            assert!(canvas.project(canvas.size * 0.5).distance(screen * 0.5) < 0.001);
            assert!(canvas.pointer(vec2(-1.0, -1.0)).is_none());
            assert_eq!(canvas.pointer(screen * 0.5), Some(canvas.size * 0.5));
            assert!(canvas.camera().zoom.y > 0.0);
        }
    }
}
