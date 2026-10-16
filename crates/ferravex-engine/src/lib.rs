//! First-party engine systems. Rust std and platform APIs are the only runtime foundations.
#![forbid(unsafe_code)]
pub use ferravex_dynamics as dynamics;
pub use ferravex_ecs as ecs;
pub use ferravex_math as math;
pub use ferravex_net as net;
pub use ferravex_play as play;
pub use ferravex_play::ability;
pub use ferravex_play::action;
pub use ferravex_play::attributes;
pub use ferravex_play::authoring;
pub use ferravex_play::feedback;
pub use ferravex_play::persistence;
pub use ferravex_play::tuning;
pub mod ai;
pub mod animation;
pub mod assets;
pub mod audio;
/// Shared follow, orbit, chase and first-person presentation rigs.
pub mod camera;
pub mod effects;
pub mod input;
pub mod profile;
pub mod replay;

pub use ferravex_graphics as graphics;
/// Original rotating rigid bodies for the 2D/3D physics games.
pub use ferravex_physics as physics;
/// Shared first-party upright capsule and planar platformer motors.
pub use ferravex_physics::character;
/// Original window/input/GPU runtime shared by the restored low-poly games.
pub use ferravex_render as render;

pub use ferravex_play::encounter;

pub use ferravex_play::vehicle;

pub use ferravex_play::racing;

pub use ferravex_play::environment;

pub use ferravex_play::objective;

pub use ferravex_play::performance;

pub use ferravex_play::gameplay;

/// See [`events`] for the system contract and examples.
pub mod events;

/// See [`frame_events`] for the system contract and examples.
pub mod frame_events;

/// See [`scheduler`] for the system contract and examples.
pub mod scheduler;

/// See [`system_order`] for the system contract and examples.
pub mod system_order;

/// See [`commands`] for the system contract and examples.
pub mod commands;

/// See [`pool`] for the system contract and examples.
pub mod pool;

/// See [`bitset`] for the system contract and examples.
pub mod bitset;

/// See [`random`] for the system contract and examples.
pub mod random;

/// See [`weighted`] for the system contract and examples.
pub mod weighted;

/// See [`shuffle`] for the system contract and examples.
pub mod shuffle;

/// See [`snapshots`] for the system contract and examples.
pub mod snapshots;

/// See [`time`] for the system contract and examples.
pub mod time;

/// See [`work_budget`] for the system contract and examples.
pub mod work_budget;

/// See [`smoothing`] for the system contract and examples.
pub mod smoothing;

/// See [`spatial`] for the system contract and examples.
pub mod spatial;

/// See [`tags`] for the system contract and examples.
pub mod tags;

/// See [`resources`] for the system contract and examples.
pub mod resources;
