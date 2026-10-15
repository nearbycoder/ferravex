//! Original rigid-body simulation for the recovered Ferravex games.
//!
//! Primitive parts have analytical mass and angular inertia.
//! Bodies can own locally transformed primitive parts and fixed, hinge,
//! distance or spring connections. Capsule/cylinder contact pairs remain limited
//! to the documented primitive matrix.
//! This is a small native solver, not a compatibility implementation of Rapier.
mod body;
pub mod character;
mod geometry;
mod joints;
mod mass;
pub use joints::{Joint, JointDesc, JointHandle, JointKind, JointMotor, JointSet, MAX_JOINTS};
mod world;
pub use body::{BodySet, Collider, ColliderHandle, ColliderSet, RigidBodyHandle};
use ferravex_math::{Quat, Vec2, Vec3};
pub use world::{
    ContactEvent, ContactManifold, ContactPair, ContactPoint, MAX_CHILDREN, MAX_COLLIDERS,
    SolverSettings, StepStats,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicsError {
    InvalidBody,
    InvalidCollider,
    InvalidMass,
    InvalidHandle,
    BodyCapacity,
    ColliderCapacity,
    ChildCapacity,
    InvalidJoint,
    JointCapacity,
}
impl std::fmt::Display for PhysicsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rigid physics: {self:?}")
    }
}
impl std::error::Error for PhysicsError {}

/// A planar rotation, measured in radians.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rotation2(f32);
impl Rotation2 {
    pub fn from_angle(angle: f32) -> Self {
        assert!(angle.is_finite());
        Self(angle)
    }
    pub fn angle(self) -> f32 {
        self.0
    }
}
impl std::ops::Mul<Vec2> for Rotation2 {
    type Output = Vec2;
    fn mul(self, p: Vec2) -> Vec2 {
        let (s, c) = self.0.sin_cos();
        Vec2::new(c * p.x - s * p.y, s * p.x + c * p.y)
    }
}

#[cfg(feature = "dim2")]
pub mod d2 {
    use super::*;
    pub type Vector = Vec2;
    pub type Rotation = Rotation2;
    pub type Angular = f32;
    fn to3(v: Vector) -> Vec3 {
        Vec3::new(v.x, v.y, 0.)
    }
    fn from3(v: Vec3) -> Vector {
        Vec2::new(v.x, v.y)
    }
    fn angular_to3(v: Angular) -> Vec3 {
        Vec3::Z * v
    }
    fn angular_from3(v: Vec3) -> Angular {
        v.z
    }
    fn rotation(core: &body::Core) -> &Rotation {
        &core.rotation2
    }
    fn rotation_to3(r: Rotation) -> Quat {
        Quat::from_rotation_z(r.angle())
    }
    const PLANAR: bool = true;
    include!("api.rs");
    impl ColliderBuilder {
        pub fn cuboid(x: f32, y: f32) -> Self {
            Self::shape(body::Shape::Box(Vec3::new(x, y, 0.5)))
        }
        pub fn capsule_from_endpoints(a: Vector, b: Vector, radius: f32) -> Self {
            Self::shape(body::Shape::Capsule {
                a: to3(a),
                b: to3(b),
                radius,
            })
        }
    }
}
#[cfg(feature = "dim3")]
pub mod d3 {
    use super::*;
    pub type Vector = Vec3;
    pub type Rotation = Quat;
    pub type Angular = Vec3;
    fn to3(v: Vector) -> Vec3 {
        v
    }
    fn from3(v: Vec3) -> Vector {
        v
    }
    fn angular_to3(v: Angular) -> Vec3 {
        v
    }
    fn angular_from3(v: Vec3) -> Angular {
        v
    }
    fn rotation(core: &body::Core) -> &Rotation {
        &core.rotation
    }
    fn rotation_to3(r: Rotation) -> Quat {
        r
    }
    const PLANAR: bool = false;
    include!("api.rs");
    impl JointBuilder {
        pub fn axes(mut self, axis1: Vector, axis2: Vector) -> Self {
            self.desc.kind = crate::JointKind::Hinge {
                axis1: to3(axis1).normalize_or_zero(),
                axis2: to3(axis2).normalize_or_zero(),
            };
            self
        }
    }
    impl ColliderBuilder {
        pub fn cuboid(x: f32, y: f32, z: f32) -> Self {
            Self::shape(body::Shape::Box(Vec3::new(x, y, z)))
        }
        pub fn cylinder(half_height: f32, radius: f32) -> Self {
            Self::shape(body::Shape::Cylinder {
                half_height,
                radius,
            })
        }
    }
}
