//! First-party CPU graphics: reusable mesh storage, visibility and low-poly rasterization.
//!
//! This is a software fallback and a headless reference renderer, not a GPU backend.
//! Geometry uses right-handed coordinates and counter-clockwise exterior winding.
#![forbid(unsafe_code)]
mod cache;
mod camera;
mod cooked;
mod image;
mod look;
mod mesh;
pub use look::Look;
mod obj;
mod raster;
pub use cache::*;
pub use camera::*;
pub use ferravex_math::{Color, Mat4, Quat, Vec2, Vec3, Vec4, vec2, vec3, vec4};
pub use image::ImageRegionError;
pub use mesh::*;
pub use obj::*;
pub use raster::*;
