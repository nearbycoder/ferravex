//! Kinematic upright capsules and planar boxes, shared by games and prediction.
//! Positive Y is up. Positions are feet/sole centers. Move terrain first, then motors.
mod geometry;
mod motion;
use ferravex_math::{Quat, Vec2, Vec3};
pub use motion::{
    Character, CharacterInput, CharacterStep, JumpState, Motion, Support, move_and_slide,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CharacterShape {
    Capsule {
        radius: f32,
        height: f32,
    },
    /// Axis-aligned planar rectangle, half extents in XY; Z remains zero.
    Box2 {
        half: Vec2,
    },
}
impl CharacterShape {
    pub fn valid(self) -> bool {
        match self {
            Self::Capsule { radius, height } => {
                radius.is_finite()
                    && height.is_finite()
                    && radius >= 0.001
                    && height >= radius * 2.
                    && height <= 1000.
            }
            Self::Box2 { half } => {
                half.is_finite() && half.min_element() >= 0.001 && half.max_element() <= 1000.
            }
        }
    }
    pub fn half_extents(self) -> Vec3 {
        match self {
            Self::Capsule { radius, height } => Vec3::new(radius, height * 0.5, radius),
            Self::Box2 { half } => Vec3::new(half.x, half.y, 0.),
        }
    }
    fn planar(self) -> bool {
        matches!(self, Self::Box2 { .. })
    }
    fn center(self, feet: Vec3) -> Vec3 {
        feet + Vec3::Y * self.half_extents().y
    }
    pub fn bounds(self, feet: Vec3) -> QueryBounds {
        let center = self.center(feet);
        let half = self.half_extents();
        QueryBounds {
            min: center - half,
            max: center + half,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct QueryBounds {
    pub min: Vec3,
    pub max: Vec3,
}
impl QueryBounds {
    pub fn intersects(self, other: Self) -> bool {
        (0..3).all(|i| self.min[i] <= other.max[i] && self.max[i] >= other.min[i])
    }
    pub fn expanded(self, amount: f32) -> Self {
        Self {
            min: self.min - Vec3::splat(amount),
            max: self.max + Vec3::splat(amount),
        }
    }
    fn union(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub enum ObstacleShape {
    Box {
        half: Vec3,
    },
    Ball {
        radius: f32,
    },
    Capsule {
        a: Vec3,
        b: Vec3,
        radius: f32,
    },
    Cylinder {
        half_height: f32,
        radius: f32,
    },
    /// An infinite solid half-space, with local outward unit normal.
    Plane {
        normal: Vec3,
    },
}
#[derive(Clone, Copy, Debug)]
pub struct Obstacle {
    /// Stable, unique identity within a terrain. Reuse requires a new generation/id.
    pub id: u64,
    pub shape: ObstacleShape,
    pub position: Vec3,
    pub rotation: Quat,
    pub membership: u32,
    pub sensor: bool,
}
impl Obstacle {
    pub fn cuboid(id: u64, position: Vec3, half: Vec3) -> Self {
        Self {
            id,
            position,
            shape: ObstacleShape::Box { half },
            rotation: Quat::IDENTITY,
            membership: u32::MAX,
            sensor: false,
        }
    }
    pub fn valid(self) -> bool {
        if !bounded(self.position)
            || !self.rotation.is_finite()
            || (self.rotation.length_squared() - 1.).abs() > 0.001
        {
            return false;
        }
        let radius_valid = |r: f32| r.is_finite() && (0.001..=1000.).contains(&r);
        match self.shape {
            ObstacleShape::Box { half } => {
                half.is_finite() && half.min_element() > 0. && half.max_element() <= 10000.
            }
            ObstacleShape::Ball { radius } => radius_valid(radius),
            ObstacleShape::Capsule { a, b, radius } => {
                bounded(a) && bounded(b) && (a - b).length() <= 2000. && radius_valid(radius)
            }
            ObstacleShape::Cylinder {
                half_height,
                radius,
            } => radius_valid(half_height) && radius_valid(radius),
            ObstacleShape::Plane { normal } => {
                normal.is_finite() && (normal.length_squared() - 1.).abs() < 0.001
            }
        }
    }
    pub fn bounds(self) -> QueryBounds {
        let (center, half) = match self.shape {
            ObstacleShape::Box { half } => (
                self.position,
                (self.rotation * Vec3::X).abs() * half.x
                    + (self.rotation * Vec3::Y).abs() * half.y
                    + (self.rotation * Vec3::Z).abs() * half.z,
            ),
            ObstacleShape::Ball { radius } => (self.position, Vec3::splat(radius)),
            ObstacleShape::Capsule { a, b, radius } => {
                let a = self.position + self.rotation * a;
                let b = self.position + self.rotation * b;
                ((a + b) * 0.5, (a - b).abs() * 0.5 + Vec3::splat(radius))
            }
            ObstacleShape::Cylinder {
                half_height,
                radius,
            } => {
                let y = self.rotation * Vec3::Y;
                (
                    self.position,
                    Vec3::new(
                        (1. - y.x * y.x).max(0.).sqrt(),
                        (1. - y.y * y.y).max(0.).sqrt(),
                        (1. - y.z * y.z).max(0.).sqrt(),
                    ) * radius
                        + y.abs() * half_height,
                )
            }
            ObstacleShape::Plane { .. } => (Vec3::ZERO, Vec3::splat(2e6)),
        };
        QueryBounds {
            min: center - half,
            max: center + half,
        }
    }
}
/// The visitor receives current poses. Implementations may use their broadphase;
/// they must visit every obstacle intersecting bounds, in deterministic order.
/// Character queries allocate no scratch buffers. Sensors/layers are filtered by the motor.
pub trait Terrain {
    fn visit(&self, bounds: QueryBounds, visitor: &mut dyn FnMut(Obstacle));
    /// Current pose for a previously reported support; None releases a removed platform.
    fn obstacle(&self, id: u64) -> Option<Obstacle>;
}
impl Terrain for [Obstacle] {
    fn visit(&self, bounds: QueryBounds, visitor: &mut dyn FnMut(Obstacle)) {
        for &o in self {
            if !o.valid() || bounds.intersects(o.bounds()) {
                visitor(o);
            }
        }
    }
    fn obstacle(&self, id: u64) -> Option<Obstacle> {
        self.iter().find(|o| o.id == id).copied()
    }
}
#[derive(Clone, Copy, Debug)]
pub struct CharacterSettings {
    pub skin: f32,
    pub ground_snap: f32,
    pub step_height: f32,
    pub min_ground_normal: f32,
    pub max_iterations: u32,
    pub max_candidates: usize,
    pub max_recovery: f32,
    pub membership_mask: u32,
    pub speed: f32,
    pub ground_acceleration: f32,
    pub air_acceleration: f32,
    pub gravity: f32,
    pub terminal_speed: f32,
    pub jump_speed: f32,
    pub coyote_time: f32,
    pub jump_buffer: f32,
}
impl Default for CharacterSettings {
    fn default() -> Self {
        Self {
            skin: 0.002,
            ground_snap: 0.08,
            step_height: 0.25,
            min_ground_normal: 0.7,
            max_iterations: 8,
            max_candidates: 16384,
            max_recovery: 1.,
            membership_mask: u32::MAX,
            speed: 7.,
            ground_acceleration: 55.,
            air_acceleration: 14.,
            gravity: 24.,
            terminal_speed: 50.,
            jump_speed: 8.6,
            coyote_time: 0.1,
            jump_buffer: 0.12,
        }
    }
}
impl CharacterSettings {
    pub fn valid(self) -> bool {
        let nonnegative = |v: f32, max: f32| v.is_finite() && (0. ..=max).contains(&v);
        self.skin.is_finite()
            && (1e-5..=1.).contains(&self.skin)
            && nonnegative(self.ground_snap, 100.)
            && nonnegative(self.step_height, 100.)
            && self.min_ground_normal.is_finite()
            && (0.01..=1.).contains(&self.min_ground_normal)
            && (1..=16).contains(&self.max_iterations)
            && (1..=1_000_000).contains(&self.max_candidates)
            && nonnegative(self.max_recovery, 1000.)
            && nonnegative(self.speed, 100000.)
            && nonnegative(self.ground_acceleration, 1e6)
            && nonnegative(self.air_acceleration, 1e6)
            && nonnegative(self.gravity, 1e6)
            && nonnegative(self.terminal_speed, 100000.)
            && nonnegative(self.jump_speed, 100000.)
            && nonnegative(self.coyote_time, 0.5)
            && nonnegative(self.jump_buffer, 0.5)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharacterError {
    InvalidInput,
    InvalidObstacle,
    QueryBudget,
    SweepBudget,
    UnresolvedPenetration,
}
impl std::fmt::Display for CharacterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "character controller: {self:?}")
    }
}
impl std::error::Error for CharacterError {}
fn bounded(p: Vec3) -> bool {
    p.is_finite() && p.abs().max_element() <= 1e6
}

// The rigid world adapter reads current poses and excludes the character's own
// body through the query mask/sensor flag when applications also store a proxy.
impl<B: crate::body::Access> Terrain for crate::world::World<B> {
    fn visit(&self, bounds: QueryBounds, visitor: &mut dyn FnMut(Obstacle)) {
        for (handle, collider) in self.colliders.iter() {
            if let Some(o) = self.character_obstacle(handle, collider)
                && bounds.intersects(o.bounds())
            {
                visitor(o);
            }
        }
    }
    fn obstacle(&self, id: u64) -> Option<Obstacle> {
        let handle = crate::ColliderHandle {
            index: (id & 0xffff_ffff) as usize,
            generation: (id >> 32) as u32,
        };
        self.character_obstacle(handle, self.colliders.get(handle)?)
    }
}
impl<B: crate::body::Access> crate::world::World<B> {
    fn character_obstacle(
        &self,
        handle: crate::ColliderHandle,
        collider: &crate::Collider,
    ) -> Option<Obstacle> {
        let core = self.bodies.get(collider.parent?)?.core();
        let shape = match collider.shape {
            crate::body::Shape::Ball(radius) => ObstacleShape::Ball { radius },
            crate::body::Shape::Box(half) => ObstacleShape::Box { half },
            #[cfg(feature = "dim2")]
            crate::body::Shape::Capsule { a, b, radius } => ObstacleShape::Capsule { a, b, radius },
            #[cfg(feature = "dim3")]
            crate::body::Shape::Cylinder {
                half_height,
                radius,
            } => ObstacleShape::Cylinder {
                half_height,
                radius,
            },
        };
        let pose = collider.pose(core);
        Some(Obstacle {
            id: (u64::from(handle.generation) << 32) | handle.index as u64,
            shape,
            position: pose.position,
            rotation: pose.rotation,
            membership: collider.membership,
            sensor: collider.sensor,
        })
    }
}

#[cfg(test)]
mod tests;
