//! Quiet, readable overlay helpers shared by the examples.
use crate::prelude::*;

pub const BACKGROUND: Color = Color::from_rgba(14, 22, 35, 255);
pub const PANEL: Color = Color::from_rgba(20, 31, 47, 242);
pub const LINE: Color = Color::from_rgba(43, 59, 77, 255);
pub const INK: Color = Color::from_rgba(228, 238, 242, 255);
pub const MUTED: Color = Color::from_rgba(139, 160, 177, 255);
pub const AMBER: Color = Color::from_rgba(247, 181, 83, 255);
pub const CYAN: Color = Color::from_rgba(91, 213, 216, 255);

/// Glyph geometry is static and needs no texture atlas or DPI prewarming.
pub fn warm_up_font() {}
pub fn warm_up_font_sizes(_sizes: &[u16]) {}

/// Compact lab title and controls; dimensions/backend details live in the host.
pub fn header(_section: &str, title: &str, subtitle: &str) {
    draw_rectangle(0.0, 0.0, screen_width(), 104.0, PANEL);
    fitted_text(title, 24.0, 45.0, screen_width() - 48.0, 28.0, INK);
    fitted_text(subtitle, 25.0, 78.0, screen_width() - 49.0, 16.0, MUTED);
    draw_line(24.0, 103.0, screen_width() - 24.0, 103.0, 1.0, LINE);
}

pub fn panel(rect: Rect) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, PANEL);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, LINE);
}

/// Click on press; the platform host maps the primary touch to the pointer.
pub fn button(rect: Rect, text: &str, selected: bool) -> bool {
    let hovered = rect.contains(Vec2::from(mouse_position()));
    draw_rectangle(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        if selected {
            AMBER
        } else if hovered {
            LINE
        } else {
            PANEL
        },
    );
    draw_rectangle_lines(
        rect.x,
        rect.y,
        rect.w,
        rect.h,
        1.0,
        if selected { AMBER } else { LINE },
    );
    // Keep one rasterized size across responsive layouts. Scaling existing
    // glyphs avoids atlas growth midway through a frame after a resize.
    let font_scale =
        ((rect.w - 12.0) / measure_text(text, None, 17, 1.0).width.max(1.0)).clamp(0.1, 1.0);
    let dimensions = measure_text(text, None, 17, font_scale);
    draw_text_ex(
        text,
        rect.x + (rect.w - dimensions.width) * 0.5,
        rect.y + rect.h * 0.5 + 6.0,
        TextParams {
            font_size: 17,
            font_scale,
            color: if selected { BACKGROUND } else { INK },
        },
    );
    hovered && is_mouse_button_pressed(MouseButton::Left)
}

fn fitted_text(text: &str, x: f32, y: f32, width: f32, size: f32, color: Color) {
    let measured = measure_text(text, None, size as u16, 1.0).width;
    draw_text_ex(
        text,
        x,
        y,
        TextParams {
            font_size: size as u16,
            font_scale: (width.max(1.0) / measured.max(1.0)).min(1.0),
            color,
        },
    );
}

pub fn stat(x: f32, y: f32, label: &str, value: &str) {
    draw_text(label, x, y, 14.0, MUTED);
    draw_text(value, x, y + 24.0, 25.0, INK);
}

mod shared;
pub use shared::*;
