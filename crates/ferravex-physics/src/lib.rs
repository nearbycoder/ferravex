//! Small, optional 2D and 3D physics adapters for Ferravex.
//!
//! Both dimensions expose the same convenience API and all of Rapier's prelude.
//! Disable default features and enable only `dim2` or `dim3` to avoid compiling
//! an unused physics engine. No renderer, windowing system, or network transport
//! is required; these worlds run on a headless game server too.
//!
//! Use a **fixed timestep** (typically 1/60 or 1/120 second) and interpolate
//! render transforms separately. `step` advances one simulation tick, with no
//! hidden catch-up loop or allocation of a new world.
//!
//! ```
//! # #[cfg(feature = "dim3")] {
//! use ferravex_physics::d3::{PhysicsWorld, Vector};
//!
//! let mut world = PhysicsWorld::new([0.0, -9.81, 0.0]);
//! world.add_box([0.0, -0.5, 0.0], [10.0, 0.5, 10.0], false);
//! let ball = world.add_ball([0.0, 3.0, 0.0], 0.25, true);
//! world.bodies[ball].enable_ccd(true);
//! world.step(1.0 / 60.0);
//! world.bodies[ball].apply_impulse(Vector::new(1.0, 0.0, 0.0), true);
//! # }
//! ```
//!
//! Custom shapes, joints, sensors, collision groups, materials, character
//! controllers, hooks, and events remain accessible through the Rapier API.
//! Each wrapper dereferences to the underlying world. Access its public `raw`
//! field when borrowing several simulation fields mutably at the same time.

#![forbid(unsafe_code)]

/// Two-dimensional physics; compiled only with the `dim2` feature.
#[cfg(feature = "dim2")]
pub mod d2;

/// Three-dimensional physics; compiled only with the `dim3` feature.
#[cfg(feature = "dim3")]
pub mod d3;
