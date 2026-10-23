//! Original, dependency-free scalar game mathematics.
//!
//! Vectors use `f32`; angles are radians. Matrices are column-major and multiply
//! column vectors. `a * b` applies `b` before `a`. Rotation is right-handed.
//! Explicit `try_*` operations report degenerate inputs; unchecked normalization
//! and inversion return NaNs instead of silently substituting another transform.
#![forbid(unsafe_code)]

pub mod affine;
pub mod curves;
mod matrix;
pub mod projection;
mod quaternion;
pub mod segments;
mod shapes;
mod vector;
pub use matrix::*;
pub use quaternion::*;
pub use shapes::*;
pub use vector::*;

/// Frequently used types and constructor functions.
pub mod prelude {
    pub use crate::{Color, EulerRot, Mat4, Quat, Rect, Vec2, Vec3, Vec4, vec2, vec3, vec4};
}

pub mod deterministic;
