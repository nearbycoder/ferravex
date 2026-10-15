//! 2D rigid bodies, collision detection, joints, and scene queries.

pub use rapier::prelude::*;
pub use rapier2d as rapier;

include!("world.rs");

fn box_collider(half_extents: [f32; DIM]) -> ColliderBuilder {
    ColliderBuilder::cuboid(half_extents[0], half_extents[1])
}
