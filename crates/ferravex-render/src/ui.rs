//! Quiet, readable overlay helpers shared by the examples.
use crate::prelude::*;

pub const BACKGROUND: Color = Color::from_rgba(14, 22, 35, 255);
pub const PANEL: Color = Color::from_rgba(20, 31, 47, 242);
pub const LINE: Color = Color::from_rgba(43, 59, 77, 255);
pub const INK: Color = Color::from_rgba(228, 238, 242, 255);
pub const MUTED: Color = Color::from_rgba(139, 160, 177, 255);
pub const AMBER: Color = Color::from_rgba(247, 181, 83, 255);
pub const CYAN: Color = Color::from_rgba(91, 213, 216, 255);

thread_local! {
    static FONT_CACHE: std::cell::RefCell<(u32, Vec<u16>)> = const { std::cell::RefCell::new((0, Vec::new())) };
}

/// Prepare printable ASCII at all UI sizes before submitting any drawing.
///
/// Call at the beginning of each frame. Work happens only on the first call
/// and when display DPI changes. Prewarming prevents Macroquad from resizing
/// its font atlas while queued glyph draws still reference the old texture.
pub fn warm_up_font() {
    warm_up_font_sizes(&[14, 17, 25, 26, 32]);
}

/// Prepare the default font at a game's declared HUD sizes, before any drawing.
/// Safe to call each frame: glyph work occurs only for a new size or DPI scale.
/// Prepares printable ASCII and common UI symbols, including the degree sign.
pub fn warm_up_font_sizes(sizes: &[u16]) {
    let dpi = macroquad::miniquad::window::dpi_scale();
    FONT_CACHE.with(|cached| {
        let mut cached = cached.borrow_mut();
        if cached.0 != dpi.to_bits() {
            cached.0 = dpi.to_bits();
            cached.1.clear();
        }
        if sizes.iter().all(|size| cached.1.contains(size)) {
            return;
        }
        let characters: Vec<char> = (' '..='~').chain("°×—·→←↑↓".chars()).collect();
        let font = get_default_font();
        for &size in sizes {
            if !cached.1.contains(&size) {
                font.populate_font_cache(&characters, (f32::from(size) * dpi).ceil() as u16);
                cached.1.push(size);
            }
        }
    });
}

pub fn header(section: &str, title: &str, subtitle: &str) {
    draw_rectangle(0.0, 0.0, screen_width(), 126.0, PANEL);
    draw_rectangle(24.0, 26.0, 5.0, 23.0, AMBER);
    draw_text("FERRAVEX", 39.0, 45.0, 26.0, INK);
    fitted_text(section, 117.0, 44.0, screen_width() - 141.0, 17.0, MUTED);
    fitted_text(title, 24.0, 84.0, screen_width() - 48.0, 32.0, INK);
    fitted_text(subtitle, 25.0, 108.0, screen_width() - 49.0, 17.0, MUTED);
    draw_line(24.0, 125.0, screen_width() - 24.0, 125.0, 1.0, LINE);
}

pub fn panel(rect: Rect) {
    draw_rectangle(rect.x, rect.y, rect.w, rect.h, PANEL);
    draw_rectangle_lines(rect.x, rect.y, rect.w, rect.h, 1.0, LINE);
}

/// Click on press; Macroquad's default touch-to-mouse emulation supports taps.
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
            ..Default::default()
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
            ..Default::default()
        },
    );
}

pub fn stat(x: f32, y: f32, label: &str, value: &str) {
    draw_text(label, x, y, 14.0, MUTED);
    draw_text(value, x, y + 24.0, 25.0, INK);
}
