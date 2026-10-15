//! Original, dependency-free 2D/3D translational rigid-body physics.
//!
//! The same fixed-step solver handles circles/spheres and **axis-aligned** boxes.
//! This foundation deliberately does not claim a full rotational rigid-body or
//! general-purpose dynamics API. See the crate README for numerical and feature limits.
//!
//! ```
//! use ferravex_dynamics::{BodyDesc, PhysicsWorld, Shape};
//! let mut world = PhysicsWorld::<3>::new([0., -9.81, 0.]);
//! world.insert(BodyDesc::fixed(Shape::cuboid([5., 0.5, 5.]), [0., -0.5, 0.])).unwrap();
//! let ball = world.insert(BodyDesc::dynamic(Shape::ball(0.5), [0., 3., 0.], 1.)).unwrap();
//! for _ in 0..240 { world.step(1. / 120.).unwrap(); }
//! assert!((world.body(ball).unwrap().position()[1] - 0.5).abs() < 0.02);
//! ```
mod character;
mod collision;
mod math;
mod world;
pub use character::{CharacterMotion, CharacterSettings};
use math::*;
pub use world::{PhysicsWorld, StepStats};

/// Non-reusable identity: removing a body invalidates its handle immediately.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BodyHandle {
    pub(crate) index: u32,
    pub(crate) generation: u32,
}
impl BodyHandle {
    pub fn index(self) -> u32 {
        self.index
    }
    pub fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape<const D: usize> {
    Ball { radius: f32 },
    Box { half_extents: [f32; D] },
}
impl<const D: usize> Shape<D> {
    pub const fn ball(radius: f32) -> Self {
        Self::Ball { radius }
    }
    /// All sizes are half extents, not full dimensions.
    pub const fn cuboid(half_extents: [f32; D]) -> Self {
        Self::Box { half_extents }
    }
    pub(crate) fn valid(self) -> bool {
        match self {
            Self::Ball { radius } => radius.is_finite() && (1e-5..=1_000_000.).contains(&radius),
            Self::Box { half_extents } => {
                valid(half_extents) && half_extents.iter().all(|v| *v >= 1e-5)
            }
        }
    }
    pub(crate) fn half(self) -> [f32; D] {
        match self {
            Self::Ball { radius } => [radius; D],
            Self::Box { half_extents } => half_extents,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BodyKind {
    Static,
    /// Kinematic bodies integrate their velocity but never receive solver impulses.
    Kinematic,
    Dynamic {
        mass: f32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CollisionLayers {
    pub membership: u32,
    pub filter: u32,
}
impl Default for CollisionLayers {
    fn default() -> Self {
        Self {
            membership: 1,
            filter: u32::MAX,
        }
    }
}
impl CollisionLayers {
    pub const fn new(membership: u32, filter: u32) -> Self {
        Self { membership, filter }
    }
    pub(crate) fn allows(self, other: Self) -> bool {
        self.membership & other.filter != 0 && other.membership & self.filter != 0
    }
}

/// Validated when inserted. One primitive collider belongs to each body.
#[derive(Clone, Copy, Debug)]
pub struct BodyDesc<const D: usize> {
    pub shape: Shape<D>,
    pub position: [f32; D],
    pub velocity: [f32; D],
    pub kind: BodyKind,
    pub restitution: f32,
    pub friction: f32,
    pub linear_damping: f32,
    pub gravity_scale: f32,
    pub sensor: bool,
    pub layers: CollisionLayers,
    /// Continuous ball collision against static, non-sensor terrain. Boxes use discrete collision.
    pub ccd: bool,
    pub user_data: u64,
}
impl<const D: usize> BodyDesc<D> {
    pub fn fixed(shape: Shape<D>, position: [f32; D]) -> Self {
        Self {
            shape,
            position,
            velocity: [0.; D],
            kind: BodyKind::Static,
            restitution: 0.,
            friction: 0.6,
            linear_damping: 0.,
            gravity_scale: 1.,
            sensor: false,
            layers: CollisionLayers::default(),
            ccd: false,
            user_data: 0,
        }
    }
    pub fn dynamic(shape: Shape<D>, position: [f32; D], mass: f32) -> Self {
        Self {
            kind: BodyKind::Dynamic { mass },
            ..Self::fixed(shape, position)
        }
    }
    pub fn kinematic(shape: Shape<D>, position: [f32; D]) -> Self {
        Self {
            kind: BodyKind::Kinematic,
            ..Self::fixed(shape, position)
        }
    }
    pub(crate) fn valid(self) -> bool {
        self.shape.valid()
            && valid(self.position)
            && valid(self.velocity)
            && self.restitution.is_finite()
            && (0. ..=1.).contains(&self.restitution)
            && self.friction.is_finite()
            && (0. ..=10.).contains(&self.friction)
            && self.linear_damping.is_finite()
            && (0. ..=10_000.).contains(&self.linear_damping)
            && self.gravity_scale.is_finite()
            && self.gravity_scale.abs() <= 1_000.
            && match self.kind {
                BodyKind::Dynamic { mass } => mass.is_finite() && (1e-6..=1e8).contains(&mass),
                _ => true,
            }
    }
}

#[derive(Clone, Debug)]
pub struct Body<const D: usize> {
    pub(crate) desc: BodyDesc<D>,
    pub(crate) force: [f32; D],
}
impl<const D: usize> Body<D> {
    pub fn position(&self) -> [f32; D] {
        self.desc.position
    }
    pub fn velocity(&self) -> [f32; D] {
        self.desc.velocity
    }
    pub fn shape(&self) -> Shape<D> {
        self.desc.shape
    }
    pub fn kind(&self) -> BodyKind {
        self.desc.kind
    }
    pub fn is_sensor(&self) -> bool {
        self.desc.sensor
    }
    pub fn user_data(&self) -> u64 {
        self.desc.user_data
    }
    pub fn layers(&self) -> CollisionLayers {
        self.desc.layers
    }
    pub fn description(&self) -> BodyDesc<D> {
        self.desc
    }
    pub fn set_position(&mut self, position: [f32; D]) -> Result<(), PhysicsError> {
        if !valid(position) {
            return Err(PhysicsError::InvalidBody);
        }
        self.desc.position = position;
        Ok(())
    }
    pub fn set_velocity(&mut self, velocity: [f32; D]) -> Result<(), PhysicsError> {
        if !valid(velocity) || self.desc.kind == BodyKind::Static && velocity != [0.; D] {
            return Err(PhysicsError::InvalidBody);
        }
        self.desc.velocity = velocity;
        Ok(())
    }
    pub fn set_layers(&mut self, layers: CollisionLayers) {
        self.desc.layers = layers;
    }
    pub fn set_user_data(&mut self, data: u64) {
        self.desc.user_data = data;
    }
    pub fn clear_forces(&mut self) {
        self.force = [0.; D];
    }
    pub fn apply_force(&mut self, force: [f32; D]) -> Result<(), PhysicsError> {
        if !valid(force) {
            return Err(PhysicsError::InvalidBody);
        }
        if self.inv_mass() == 0. {
            return Ok(());
        }
        let sum = add(self.force, force);
        if !valid(sum) {
            return Err(PhysicsError::InvalidBody);
        }
        self.force = sum;
        Ok(())
    }
    pub fn apply_impulse(&mut self, impulse: [f32; D]) -> Result<(), PhysicsError> {
        if !valid(impulse) {
            return Err(PhysicsError::InvalidBody);
        }
        if self.inv_mass() == 0. {
            return Ok(());
        }
        self.set_velocity(add(self.desc.velocity, mul(impulse, self.inv_mass())))
    }
    pub(crate) fn inv_mass(&self) -> f32 {
        match self.desc.kind {
            BodyKind::Dynamic { mass } => mass.recip(),
            _ => 0.,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhysicsError {
    InvalidDimension,
    InvalidConfiguration,
    InvalidBody,
    InvalidTimestep,
    BodyCapacity,
    PairCapacity,
    NumericalLimit,
}
impl std::fmt::Display for PhysicsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidDimension => "physics supports only dimension 2 or 3",
            Self::InvalidConfiguration => "invalid physics world configuration",
            Self::InvalidBody => "body contains invalid or out-of-range parameters",
            Self::InvalidTimestep => "physics timestep must be finite and within (0, 0.1] seconds",
            Self::BodyCapacity => "physics body capacity reached",
            Self::PairCapacity => "broadphase pair capacity reached; step rolled back",
            Self::NumericalLimit => "physics numerical range exceeded; step rolled back",
        })
    }
}
impl std::error::Error for PhysicsError {}

#[derive(Clone, Copy, Debug)]
pub struct WorldConfig<const D: usize> {
    pub gravity: [f32; D],
    pub max_bodies: usize,
    pub max_pairs: usize,
    /// Bound work even when many AABBs overlap along the sweep axis only.
    pub max_pair_tests: usize,
    pub solver_iterations: u32,
    pub contact_slop: f32,
    /// At most this many terrain impacts are processed per CCD body per step.
    pub ccd_iterations: u32,
}
impl<const D: usize> WorldConfig<D> {
    pub fn new(gravity: [f32; D]) -> Self {
        Self {
            gravity,
            max_bodies: 4096,
            max_pairs: 65_536,
            max_pair_tests: 2_000_000,
            solver_iterations: 12,
            contact_slop: 0.001,
            ccd_iterations: 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Contact<const D: usize> {
    pub a: BodyHandle,
    pub b: BodyHandle,
    /// Unit vector from A toward B.
    pub normal: [f32; D],
    pub penetration: f32,
    pub sensor: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactEventKind {
    Started,
    Stopped,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContactEvent<const D: usize> {
    pub kind: ContactEventKind,
    pub contact: Contact<D>,
}

#[derive(Clone, Copy, Debug)]
pub struct QueryFilter {
    pub exclude: Option<BodyHandle>,
    pub layers: CollisionLayers,
    pub include_sensors: bool,
    pub only_static: bool,
}
impl Default for QueryFilter {
    fn default() -> Self {
        Self {
            exclude: None,
            layers: CollisionLayers {
                membership: u32::MAX,
                filter: u32::MAX,
            },
            include_sensors: false,
            only_static: false,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit<const D: usize> {
    pub body: BodyHandle,
    pub distance: f32,
    pub point: [f32; D],
    /// Unit surface normal pointing out of the hit collider.
    pub normal: [f32; D],
}

pub mod d2 {
    pub type PhysicsWorld = crate::PhysicsWorld<2>;
    pub type BodyDesc = crate::BodyDesc<2>;
    pub type Shape = crate::Shape<2>;
    pub type Body = crate::Body<2>;
}
pub mod d3 {
    pub type PhysicsWorld = crate::PhysicsWorld<3>;
    pub type BodyDesc = crate::BodyDesc<3>;
    pub type Shape = crate::Shape<3>;
    pub type Body = crate::Body<3>;
}
