//! First-party engine systems. Rust std and platform APIs are the only runtime foundations.
#![forbid(unsafe_code)]
pub use ferravex_dynamics as dynamics;
pub use ferravex_ecs as ecs;
pub use ferravex_math as math;
pub use ferravex_net as net;
pub use ferravex_play as play;
pub mod assets;
pub mod audio;
pub mod input;
pub mod profile;
pub mod replay;

pub use ferravex_graphics as graphics;
/// Original rotating rigid bodies for the 2D/3D physics games.
pub use ferravex_physics as physics;
/// Original window/input/GPU runtime shared by the restored low-poly games.
pub use ferravex_render as render;
