//! Portable rendering without an engine-sized abstraction layer.
//!
//! The complete Macroquad API stays available through [`prelude`] and
//! [`macroquad`]. Ferravex adds reusable lit meshes, dual-backend shader loading,
//! and small UI helpers. GPU resources must be created and used on the
//! Macroquad event-loop thread after the window is initialized.

pub use macroquad;
pub use macroquad::prelude;

pub mod canvas;
pub mod demo;
pub mod input;
pub mod mesh;
pub mod shaders;
pub mod ui;

/// Shared window defaults. Metal is used on Apple, OpenGL/WebGL elsewhere.
pub fn window_conf(title: &str) -> macroquad::prelude::Conf {
    use macroquad::miniquad::conf::{AppleGfxApi, Platform};
    macroquad::prelude::Conf {
        window_title: title.to_owned(),
        window_width: 1280,
        window_height: 800,
        high_dpi: true,
        sample_count: 4,
        platform: Platform {
            apple_gfx_api: AppleGfxApi::Metal,
            ..Default::default()
        },
        ..Default::default()
    }
}
