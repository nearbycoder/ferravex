//! Shared presentation rigs and a current-pose rigid-physics obstruction adapter.
use ferravex_physics::{RigidBodyHandle, d3::PhysicsWorld};
pub use ferravex_play::camera::*;
/// Filter by collider membership and ignore all parts of the followed body.
/// Current compound/local transforms are read directly; no query sync is needed.
pub struct RigidObstructions<'a> {
    pub world: &'a PhysicsWorld,
    pub mask: u32,
    pub ignore: Option<RigidBodyHandle>,
}
impl<'a> RigidObstructions<'a> {
    pub fn new(world: &'a PhysicsWorld) -> Self {
        Self {
            world,
            mask: u32::MAX,
            ignore: None,
        }
    }
}
impl CollisionQuery for RigidObstructions<'_> {
    fn sweep(&self, sweep: Sweep) -> Result<Option<f32>, CameraError> {
        if !sweep.valid() {
            return Err(CameraError::InvalidQuery);
        }
        let delta = sweep.end - sweep.start;
        let length = delta.length();
        let direction = if length > 1e-10 {
            delta / length
        } else {
            ferravex_math::Vec3::Z
        };
        Ok(self
            .world
            .spherecast(
                sweep.start.to_array(),
                direction.to_array(),
                length,
                sweep.radius,
                self.mask,
                self.ignore,
            )
            .map(|hit| {
                if length > 1e-10 {
                    hit.distance / length
                } else {
                    0.
                }
            }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use ferravex_math::{Quat, Vec3};
    use ferravex_physics::d3::{ColliderBuilder, RigidBodyBuilder};
    #[test]
    fn rigid_adapter_uses_filtered_current_compound_poses_and_ignores_the_player() {
        let mut w = PhysicsWorld::default();
        let player = w.add_ball([0.; 3], 0.5, true);
        let wall = w
            .insert_compound(
                RigidBodyBuilder::fixed(),
                [
                    ColliderBuilder::cuboid(0.1, 2., 2.)
                        .translation(Vec3::new(4., 0., 0.))
                        .collision_layers(2, u32::MAX),
                    ColliderBuilder::cuboid(0.1, 2., 2.)
                        .translation(Vec3::new(6., 0., 0.))
                        .collision_layers(4, u32::MAX),
                ],
            )
            .unwrap();
        w.insert(
            RigidBodyBuilder::fixed().translation(Vec3::new(2., 0., 0.)),
            ColliderBuilder::cuboid(0.1, 2., 2.).sensor(true),
        );
        let s = Sweep {
            start: Vec3::ZERO,
            end: Vec3::X * 10.,
            radius: 0.25,
        };
        let query = RigidObstructions {
            world: &w,
            mask: 2,
            ignore: Some(player),
        };
        assert!((query.sweep(s).unwrap().unwrap() - 0.365).abs() < 0.0001);
        assert!(
            query
                .sweep(Sweep {
                    end: Vec3::ZERO,
                    ..s
                })
                .unwrap()
                .is_none()
        );
        let first = w.bodies[wall].colliders()[0];
        w.set_collider_pose(first, Vec3::new(3., 0., 0.), Quat::IDENTITY)
            .unwrap();
        let query = RigidObstructions {
            world: &w,
            mask: 2,
            ignore: Some(player),
        };
        assert!((query.sweep(s).unwrap().unwrap() - 0.265).abs() < 0.0001);
        let query = RigidObstructions {
            world: &w,
            mask: 4,
            ignore: Some(player),
        };
        assert!((query.sweep(s).unwrap().unwrap() - 0.565).abs() < 0.0001);
        let query = RigidObstructions {
            world: &w,
            mask: u32::MAX,
            ignore: None,
        };
        assert_eq!(query.sweep(s).unwrap(), Some(0.));
        w.remove_body(wall);
        let query = RigidObstructions {
            world: &w,
            mask: 2,
            ignore: Some(player),
        };
        assert_eq!(query.sweep(s).unwrap(), None);
    }
}
