//! Ferravex: original, modular Rust engine systems using only project crates and std.
//! Legacy Macroquad/Rapier adapters require explicit `render`, `physics2d` or `physics3d` features.
pub use ferravex_core as core;
pub use ferravex_core::{Entity, FixedClock, FrameStats, FrameStep, World};
pub use ferravex_engine::*;
#[cfg(feature = "net")]
pub use ferravex_net as legacy_net;
#[cfg(any(feature = "physics2d", feature = "physics3d"))]
pub use ferravex_physics as physics;
#[cfg(feature = "render")]
pub use ferravex_render as render;
