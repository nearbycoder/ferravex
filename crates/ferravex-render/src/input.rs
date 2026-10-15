//! Input in the same logical-pixel coordinates as the screen and mouse APIs.
use crate::prelude::*;

/// Touch positions normalized for display DPI, retaining IDs and phases.
/// Macroquad 0.4.16's raw `touches()` uses framebuffer pixels, unlike
/// `mouse_position()` and `screen_width()`. Use this before canvas projection
/// or hit-testing logical-pixel buttons on Retina/high-DPI displays.
pub fn logical_touches() -> Vec<Touch> {
    let mut points = touches();
    normalize_positions(&mut points, screen_dpi_scale());
    points
}

fn normalize_positions(points: &mut [Touch], dpi: f32) {
    for point in points {
        point.position /= dpi;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Canvas2D;

    #[test]
    fn high_dpi_taps_hit_the_same_logical_canvas_button() {
        let canvas = Canvas2D::fit(vec2(1280.0, 800.0), vec2(390.0, 844.0));
        let button = Rect::new(1080.0, 600.0, 100.0, 100.0);
        let logical_position = canvas.project(button.center());
        for dpi in [1.0, 1.5, 2.0, 3.0] {
            let mut points = [Touch {
                id: 17,
                phase: TouchPhase::Started,
                position: logical_position * dpi,
            }];
            normalize_positions(&mut points, dpi);
            assert!(button.contains(canvas.pointer(points[0].position).unwrap()));
            assert!(points[0].position.distance(logical_position) < 0.001);
            assert_eq!(points[0].id, 17);
            assert_eq!(points[0].phase, TouchPhase::Started);
        }
    }
}
