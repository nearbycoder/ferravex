//! Ferravex: original, modular Rust engine systems using only project crates and std.
//!
//! Start with [`prelude`] for session timing, math, cameras and input. Rendering
//! functions stay under [`render`] so their origin remains clear. The full
//! engine modules remain available for more specialized systems.
//!
//! # Find an API
//!
//! The prelude adds only common starting points; qualified paths retain the
//! original types and errors without facade wrappers:
//!
//! - [`SessionConfig`] validates reusable session settings. Use
//!   [`core::FixedClockError`] for setup failures and [`session::SessionStepError`]
//!   for rejected inspector ticks from [`GameSession::step_paused`].
//! - [`app::{App, Game, AppFrameReport}`](app) provides a host-driven lifecycle
//!   and reports scheduled versus executed updates, including partial failures.
//! - [`time::SimulationTimer`], [`scheduler`], [`events`], [`snapshots`] and
//!   [`diagnostics`] provide explicit timing, bounded work and observability.
//! - [`input`] contains action maps and binding validation; [`render::input`]
//!   adapts host input, controls editing and binding hints.
//! - [`gameplay`], [`play::navigation`], [`play::scene`] and [`authoring`] contain
//!   game rules and authored content. [`camera`] supplies simulation-independent
//!   rigs; [`render::Camera2D`] and [`render::Camera3D`] configure drawing.
//! - [`feedback`] produces responses to accepted events. [`render::preferences`]
//!   synchronizes presentation policy; [`audio::GameAudio`] owns runtime voices
//!   on its creating thread.
//! - [`play::replay`] provides generic recording/playback; [`replay::Replay`]
//!   retains the distinct legacy network-input format. [`persistence`] contains
//!   portable records/preferences; [`render::storage`] provides host storage.
//! - [`physics`] is the rotating rigid-body API; [`dynamics`] is the separate
//!   translational solver. Their world types are not interchangeable.
//!
//! The facade's optional `content` feature forwards runtime authoring support
//! to the engine. The default facade already exposes renderer-independent
//! [`authoring`] APIs; enabling `content` is not needed for the example below.
//!
//! ```
//! use ferravex::prelude::*;
//!
//! let config = SessionConfig::try_new(60.0, 8, true)?;
//! let mut session = GameSession::from_config(config);
//! let mut position = Vec2::ZERO;
//! let mut previous = position;
//! let velocity = vec2(120.0, 0.0);
//! let frame = session.advance(1.5 / 60.0);
//! for _ in 0..frame.steps {
//!     previous = position;
//!     position += velocity * frame.dt;
//! }
//! // Presentation interpolates snapshots without changing simulation state.
//! let visible_position = previous.lerp(position, frame.alpha);
//! assert!((visible_position.x - 1.0).abs() < 0.0001);
//! # Ok::<(), ferravex::core::FixedClockError>(())
//! ```
//!
//! Run the facade-only example's deterministic scenario without a display:
//! `cargo run -p ferravex --example quickstart --offline --locked -j 2 -- --headless`.
//! Omit `-- --headless` for an interactive native window. It demonstrates pause,
//! restart and paused single-stepping using the existing [`render::game!`] entry
//! point. Compile its Wasm entry point with
//! `cargo build -p ferravex --example quickstart --target wasm32-unknown-unknown --offline --locked -j 2`.
//! Compilation alone does not exercise a browser host or graphics device.
pub use ferravex_core as core;
pub use ferravex_core::session::{GameSession, SessionConfig, SessionState};
pub use ferravex_core::{Entity, FixedClock, FrameStats, FrameStep, TickBatch, TickContext, World};
pub use ferravex_engine::*;

pub mod prelude;
