//! Shared AI plus authoritative current-pose rigid-world occlusion.
use ferravex_math::Vec3;
use ferravex_physics::{RigidBodyHandle, d3::PhysicsWorld};
pub use ferravex_play::ai::*;
pub struct RigidVisibility<'a> {
    pub world: &'a PhysicsWorld,
    pub mask: u32,
    pub ignore: Option<RigidBodyHandle>,
}
impl VisibilityQuery for RigidVisibility<'_> {
    fn clear(&mut self, from: Vec3, to: Vec3) -> bool {
        if !from.is_finite() || !to.is_finite() {
            return false;
        }
        let delta = to - from;
        let length = delta.length();
        if !length.is_finite() {
            return false;
        }
        if length <= 1e-8 {
            return true;
        }
        self.world
            .spherecast(
                from.to_array(),
                (delta / length).to_array(),
                length,
                0.,
                self.mask,
                self.ignore,
            )
            .is_none_or(|h| h.distance >= length - 1e-5)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_physics::d3::{ColliderBuilder, RigidBodyBuilder};
    #[test]
    fn rigid_visibility_ignores_self_sensors_and_reads_current_poses() {
        let mut world = PhysicsWorld::default();
        let actor = world.add_ball([0.; 3], 0.5, true);
        world.insert(
            RigidBodyBuilder::fixed().translation(Vec3::X * 2.),
            ColliderBuilder::cuboid(0.2, 1., 1.).sensor(true),
        );
        let wall = world.insert(
            RigidBodyBuilder::fixed().translation(Vec3::X * 4.),
            ColliderBuilder::cuboid(0.2, 1., 1.).collision_layers(2, u32::MAX),
        );
        let mut q = RigidVisibility {
            world: &world,
            mask: 2,
            ignore: Some(actor),
        };
        assert!(!q.clear(Vec3::ZERO, Vec3::X * 8.));
        q.mask = 4;
        assert!(q.clear(Vec3::ZERO, Vec3::X * 8.));
        world.remove_body(wall.0);
        assert!(
            RigidVisibility {
                world: &world,
                mask: 2,
                ignore: Some(actor)
            }
            .clear(Vec3::ZERO, Vec3::X * 8.)
        );
    }
}
