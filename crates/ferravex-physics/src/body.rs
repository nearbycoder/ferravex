use crate::Rotation2;
use ferravex_math::{Quat, Vec3};
use std::ops::{Index, IndexMut};

/// Linear and angular velocity handling during a checked teleport.
/// See `d2::RigidBody::try_teleport` or `d3::RigidBody::try_teleport`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VelocityPolicy {
    /// Retain the stored world-space center-of-mass and angular velocities.
    /// This does not preserve angular momentum when the orientation changes.
    Preserve,
    /// Set both stored velocities to zero.
    Clear,
}

/// Independent motion-reset choices for a checked teleport.
/// Construction requires an explicit velocity policy. Fields may be edited to
/// retain forces or pending targets; see the dimensional teleport methods.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub struct TeleportOptions {
    pub velocity: VelocityPolicy,
    /// Clear both accumulated force and torque. Gravity/damping are unchanged.
    pub clear_forces: bool,
    /// Clear both next-position and next-rotation kinematic targets.
    pub clear_kinematic_targets: bool,
}
impl TeleportOptions {
    /// Select a velocity policy and clear forces/torque and kinematic targets.
    /// To retain either independently, set the corresponding field to `false`.
    /// See `d2::RigidBody::try_teleport` for a runnable reset recipe.
    pub const fn new(velocity: VelocityPolicy) -> Self {
        Self {
            velocity,
            clear_forces: true,
            clear_kinematic_targets: true,
        }
    }
}

/// Checked teleport failure. The entire request is rejected before any edit.
/// The world method checks the handle first, then translation, rotation and
/// revision capacity. Handles must come from the same world; they contain no
/// world identity. See the dimensional `PhysicsWorld::try_teleport_body` recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeleportError {
    /// The requested body handle is stale or absent.
    InvalidHandle,
    /// The authored body-origin translation contains a non-finite component.
    Translation,
    /// Rotation cannot be normalized to a finite, dimension-valid quaternion.
    Rotation,
    /// The body's contact revision cannot advance without reusing an identity.
    RevisionExhausted,
}
impl std::fmt::Display for TeleportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidHandle => "teleport body handle is stale or absent",
            Self::Translation => "teleport translation must be finite",
            Self::Rotation => "teleport rotation must be finite, nonzero and dimension-valid",
            Self::RevisionExhausted => "teleport body revision exhausted",
        })
    }
}
impl std::error::Error for TeleportError {}

/// Transactional shape replacement failure. See the dimensional
/// `PhysicsWorld::replace_collider_shape` method for a runnable editing recipe.
/// Existing `PhysicsError` variants remain unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColliderShapeError {
    /// The collider or its parent is stale or absent in this world.
    InvalidHandle,
    /// Candidate geometry or retained collider settings are invalid for this world.
    InvalidCollider,
    /// The resulting dynamic compound has zero or unrepresentable mass/inertia.
    InvalidMass,
    /// The collider or body contact revision cannot advance without identity reuse.
    RevisionExhausted,
}
impl std::fmt::Display for ColliderShapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidHandle => "shape replacement handle is stale or absent",
            Self::InvalidCollider => "shape replacement collider is invalid for this world",
            Self::InvalidMass => "shape replacement compound mass/inertia is invalid",
            Self::RevisionExhausted => "shape replacement contact revision exhausted",
        })
    }
}
impl std::error::Error for ColliderShapeError {}

/// The first invalid field encountered by checked rigid-body authoring.
///
/// Mass is derived from collider densities and dimensions, not supplied on a
/// rigid-body builder. `Mass` therefore describes the combined mass/inertia;
/// `Density` describes an invalid individual density. Checks never insert into
/// a world or change a previously built body/collider.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyValidationError {
    /// Body translation must be finite.
    Translation,
    /// Body rotation must represent a finite unit quaternion.
    Rotation,
    /// Linear velocity must be finite.
    LinearVelocity,
    /// Angular velocity must be finite.
    AngularVelocity,
    /// Linear damping must be finite and nonnegative.
    LinearDamping,
    /// Angular damping must be finite and nonnegative.
    AngularDamping,
    /// Gravity scale must be finite.
    GravityScale,
    /// Collider translation must be finite and within +/-1e6 on each axis.
    ColliderTranslation,
    /// Collider rotation must be valid and preserve the plane for 2D bodies.
    ColliderRotation,
    /// Density must be finite and in 0..=1e6; zero-density children are allowed.
    Density,
    /// Radius must be finite and positive.
    Radius,
    /// Every half-extent must be finite and positive.
    HalfExtents,
    /// Cylinder half-height must be finite and positive.
    HalfHeight,
    /// The first capsule endpoint must be finite and planar in 2D.
    CapsuleStart,
    /// The second capsule endpoint must be finite and planar in 2D.
    CapsuleEnd,
    /// A cylinder cannot be used in a 2D body definition.
    ShapeDimension,
    /// The contact material must satisfy `PhysicsMaterial::valid`.
    Material,
    /// Dynamic bodies need positive finite mass and representable inverse mass/inertia.
    Mass,
    /// A body definition may contain at most `MAX_CHILDREN` colliders.
    ChildCapacity,
}

impl std::fmt::Display for BodyValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Translation => "body translation must be finite",
            Self::Rotation => "body rotation must represent a finite unit quaternion",
            Self::LinearVelocity => "linear velocity must be finite",
            Self::AngularVelocity => "angular velocity must be finite",
            Self::LinearDamping => "linear damping must be finite and nonnegative",
            Self::AngularDamping => "angular damping must be finite and nonnegative",
            Self::GravityScale => "gravity scale must be finite",
            Self::ColliderTranslation => {
                "collider translation must be finite, within +/-1e6, and planar in 2D"
            }
            Self::ColliderRotation => {
                "collider rotation must be a finite unit quaternion and preserve the plane in 2D"
            }
            Self::Density => "density must be finite and in 0..=1e6",
            Self::Radius => "radius must be finite and positive",
            Self::HalfExtents => "half-extents must be finite and positive",
            Self::HalfHeight => "half-height must be finite and positive",
            Self::CapsuleStart => "capsule start must be finite and planar in 2D",
            Self::CapsuleEnd => "capsule end must be finite and planar in 2D",
            Self::ShapeDimension => "cylinders require a 3D body",
            Self::Material => "contact material is invalid",
            Self::Mass => "dynamic body mass/inertia must be positive, finite and invertible",
            Self::ChildCapacity => "body definition exceeds MAX_CHILDREN",
        })
    }
}
impl std::error::Error for BodyValidationError {}

fn require(valid: bool, error: BodyValidationError) -> Result<(), BodyValidationError> {
    if valid { Ok(()) } else { Err(error) }
}

fn unit_rotation(q: Quat, planar: bool) -> bool {
    q.is_finite()
        && (q.length_squared() - 1.).abs() < 0.001
        && (!planar || (q * Vec3::Z - Vec3::Z).length_squared() < 1e-6)
}

fn valid_density(density: f32) -> bool {
    density.is_finite() && (0. ..=1e6).contains(&density)
}

fn valid_local_translation(p: Vec3, planar: bool) -> bool {
    p.is_finite() && p.abs().max_element() <= 1e6 && (!planar || p.z == 0.)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RigidBodyHandle {
    pub(crate) index: usize,
    pub(crate) generation: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColliderHandle {
    pub(crate) index: usize,
    pub(crate) generation: u32,
}
#[derive(Clone, Debug)]
pub(crate) struct Slot<T> {
    pub(crate) generation: u32,
    pub(crate) value: Option<T>,
}
#[derive(Clone, Debug)]
pub struct BodySet<B> {
    slots: Vec<Slot<B>>,
    len: usize,
}
#[derive(Clone, Debug)]
pub struct ColliderSet {
    slots: Vec<Slot<Collider>>,
    len: usize,
}
macro_rules! arena {
    ($set:ident $(<$generic:ident>)?,$item:ty,$handle:ident) => {
        impl$(<$generic>)? Default for $set$(<$generic>)? {fn default()->Self {Self{slots:Vec::new(),len:0}}}
        impl$(<$generic>)? $set$(<$generic>)? {
            pub fn len(&self)->usize {self.len}
            pub fn is_empty(&self)->bool {self.len==0}
            pub fn get(&self,h:$handle)->Option<&$item> {let s=self.slots.get(h.index)?; (s.generation==h.generation).then_some(s.value.as_ref()).flatten()}
            pub fn get_mut(&mut self,h:$handle)->Option<&mut $item> {let s=self.slots.get_mut(h.index)?; (s.generation==h.generation).then_some(s.value.as_mut()).flatten()}
            pub fn iter(&self)->impl Iterator<Item=($handle,&$item)> {self.slots.iter().enumerate().filter_map(|(index,s)|s.value.as_ref().map(|v|($handle{index,generation:s.generation},v)))}
            pub fn iter_mut(&mut self)->impl Iterator<Item=($handle,&mut $item)> {self.slots.iter_mut().enumerate().filter_map(|(index,s)|s.value.as_mut().map(|v|($handle{index,generation:s.generation},v)))}
            pub(crate) fn insert(&mut self,value:$item)->$handle {
                self.len+=1;
                for(index,s) in self.slots.iter_mut().enumerate() {if s.value.is_none()&&s.generation<u32::MAX {s.value=Some(value);return $handle{index,generation:s.generation};}}
                let index=self.slots.len();self.slots.push(Slot{generation:0,value:Some(value)});$handle{index,generation:0}
            }
            pub(crate) fn remove(&mut self,h:$handle)->Option<$item> {let s=self.slots.get_mut(h.index)?;if s.generation!=h.generation {return None;} let v=s.value.take()?;s.generation=s.generation.saturating_add(1);self.len-=1;Some(v)}
        }
        impl$(<$generic>)? Index<$handle> for $set$(<$generic>)? {type Output=$item;fn index(&self,h:$handle)->&$item {self.get(h).expect("stale physics handle")}}
        impl$(<$generic>)? IndexMut<$handle> for $set$(<$generic>)? {fn index_mut(&mut self,h:$handle)->&mut $item {self.get_mut(h).expect("stale physics handle")}}
    }
}
pub(crate) use arena;
arena!(BodySet<B>, B, RigidBodyHandle);
arena!(ColliderSet, Collider, ColliderHandle);
#[derive(Clone, Copy, Debug)]
pub(crate) enum Shape {
    Ball(f32),
    Box(Vec3),
    #[cfg(feature = "dim2")]
    Capsule {
        a: Vec3,
        b: Vec3,
        radius: f32,
    },
    #[cfg(feature = "dim3")]
    Cylinder {
        half_height: f32,
        radius: f32,
    },
}
impl Shape {
    pub(crate) fn check_dimensions(self, planar: bool) -> Result<(), BodyValidationError> {
        use BodyValidationError as E;
        // Used even in builds without dimension-specific primitives.
        let _ = planar;
        match self {
            Self::Ball(radius) => require(radius.is_finite() && radius > 0., E::Radius),
            Self::Box(half) => require(
                half.is_finite() && half.x > 0. && half.y > 0. && half.z > 0.,
                E::HalfExtents,
            ),
            #[cfg(feature = "dim2")]
            Self::Capsule { a, b, radius } => {
                require(a.is_finite() && (!planar || a.z == 0.), E::CapsuleStart)?;
                require(b.is_finite() && (!planar || b.z == 0.), E::CapsuleEnd)?;
                require(radius.is_finite() && radius > 0., E::Radius)
            }
            #[cfg(feature = "dim3")]
            Self::Cylinder {
                half_height,
                radius,
            } => {
                require(!planar, E::ShapeDimension)?;
                require(half_height.is_finite() && half_height > 0., E::HalfHeight)?;
                require(radius.is_finite() && radius > 0., E::Radius)
            }
        }
    }

    pub fn validate(self) {
        match self {
            Self::Ball(r) => assert!(r.is_finite() && r > 0.),
            Self::Box(h) => assert!(h.is_finite() && h.x > 0. && h.y > 0. && h.z > 0.),
            #[cfg(feature = "dim2")]
            Self::Capsule { a, b, radius } => {
                assert!(a.is_finite() && b.is_finite() && radius.is_finite() && radius > 0.)
            }
            #[cfg(feature = "dim3")]
            Self::Cylinder {
                half_height,
                radius,
            } => assert!(
                half_height.is_finite() && half_height > 0. && radius.is_finite() && radius > 0.
            ),
        }
    }
}
#[derive(Clone, Debug)]
pub struct Collider {
    pub(crate) shape: Shape,
    pub(crate) local_translation: Vec3,
    pub(crate) local_rotation: Quat,
    pub(crate) mass_dirty: bool,
    pub(crate) parent: Option<RigidBodyHandle>,
    pub(crate) density: f32,
    pub(crate) material: crate::PhysicsMaterial,
    pub(crate) sensor: bool,
    pub(crate) membership: u32,
    pub(crate) filter: u32,
    pub(crate) revision: u64,
    pub(crate) dirty: bool,
}
impl Collider {
    fn validate_authoring(&self, planar: bool) -> Result<(), BodyValidationError> {
        use BodyValidationError as E;
        self.shape.check_dimensions(planar)?;
        require(
            valid_local_translation(self.local_translation, planar),
            E::ColliderTranslation,
        )?;
        require(
            unit_rotation(self.local_rotation, planar),
            E::ColliderRotation,
        )?;
        require(valid_density(self.density), E::Density)?;
        require(self.material.valid(), E::Material)
    }

    pub(crate) fn new(shape: Shape) -> Self {
        Self {
            shape,
            local_translation: Vec3::ZERO,
            local_rotation: Quat::IDENTITY,
            mass_dirty: false,
            parent: None,
            density: 1.,
            material: crate::PhysicsMaterial::default(),
            sensor: false,
            membership: u32::MAX,
            filter: u32::MAX,
            revision: 0,
            dirty: false,
        }
    }
    pub(crate) fn valid(&self, planar: bool) -> bool {
        self.material.valid()
            && self.local_translation.is_finite()
            && self.local_translation.abs().max_element() <= 1e6
            && self.local_rotation.is_finite()
            && (self.local_rotation.length_squared() - 1.).abs() < 0.001
            && self.density.is_finite()
            && (0. ..=1e6).contains(&self.density)
            && (!planar
                || self.local_translation.z == 0.
                    && (self.local_rotation * Vec3::Z - Vec3::Z).length_squared() < 1e-6)
    }
    pub fn local_translation(&self) -> Vec3 {
        self.local_translation
    }
    pub fn local_rotation(&self) -> Quat {
        self.local_rotation
    }
    pub fn density(&self) -> f32 {
        self.density
    }
    pub fn set_density(&mut self, density: f32) {
        assert!(density.is_finite() && (0. ..=1e6).contains(&density));
        if self.density != density {
            self.density = density;
            self.touch_mass();
        }
    }
    pub(crate) fn touch_mass(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("collider revision exhausted");
        self.dirty = true;
        self.mass_dirty = true;
    }
    pub(crate) fn pose(&self, body: &Core) -> crate::geometry::Pose {
        crate::geometry::Pose {
            position: body.position + body.rotation * self.local_translation,
            rotation: body.rotation * self.local_rotation,
            planar: body.planar,
        }
    }
    pub fn set_sensor(&mut self, sensor: bool) {
        if self.sensor != sensor {
            self.revision = self
                .revision
                .checked_add(1)
                .expect("collider revision exhausted");
            self.dirty = true;
            self.sensor = sensor;
        }
    }
    pub fn set_collision_layers(&mut self, membership: u32, filter: u32) {
        if (self.membership, self.filter) != (membership, filter) {
            self.revision = self
                .revision
                .checked_add(1)
                .expect("collider revision exhausted");
            self.dirty = true;
            self.membership = membership;
            self.filter = filter;
        }
    }
    pub fn parent(&self) -> Option<RigidBodyHandle> {
        self.parent
    }
    pub fn is_sensor(&self) -> bool {
        self.sensor
    }
    pub fn material(&self) -> crate::PhysicsMaterial {
        self.material
    }
    pub fn set_material(
        &mut self,
        material: crate::PhysicsMaterial,
    ) -> Result<(), crate::PhysicsError> {
        if !material.valid() {
            return Err(crate::PhysicsError::InvalidCollider);
        }
        if self.material != material {
            self.revision = self
                .revision
                .checked_add(1)
                .expect("collider revision exhausted");
            self.dirty = true;
            self.material = material;
        }
        Ok(())
    }
    pub fn set_restitution(&mut self, v: f32) {
        self.set_material(crate::PhysicsMaterial {
            restitution: v,
            ..self.material
        })
        .expect("valid restitution");
    }
    pub fn set_friction(&mut self, v: f32) {
        self.set_material(crate::PhysicsMaterial {
            static_friction: v,
            sliding_friction: v,
            ..self.material
        })
        .expect("valid friction");
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Fixed,
    Dynamic,
    Kinematic,
}
#[derive(Clone, Debug)]
#[doc(hidden)]
pub struct Core {
    pub(crate) kind: Kind,
    pub(crate) planar: bool,
    pub(crate) position: Vec3,
    pub(crate) rotation: Quat,
    pub(crate) rotation2: Rotation2,
    pub(crate) velocity: Vec3,
    pub(crate) angular_velocity: Vec3,
    pub(crate) mass: f32,
    pub(crate) inv_mass: f32,
    pub(crate) inv_inertia: crate::mass::Tensor,
    pub(crate) local_com: Vec3,
    pub(crate) force: Vec3,
    pub(crate) torque: Vec3,
    pub(crate) gravity_scale: f32,
    pub(crate) linear_damping: f32,
    pub(crate) angular_damping: f32,
    pub(crate) ccd: bool,
    pub(crate) sleeping: bool,
    pub(crate) quiet: f32,
    pub(crate) revision: u64,
    pub(crate) solver_dirty: bool,
    pub(crate) wake_requested: bool,
    pub(crate) next_position: Option<Vec3>,
    pub(crate) next_rotation: Option<Quat>,
    pub(crate) colliders: Vec<ColliderHandle>,
}
#[doc(hidden)]
pub trait Access {
    fn core(&self) -> &Core;
    fn core_mut(&mut self) -> &mut Core;
}
impl Core {
    fn validate_authoring(&self) -> Result<(), BodyValidationError> {
        use BodyValidationError as E;
        require(self.position.is_finite(), E::Translation)?;
        require(unit_rotation(self.rotation, self.planar), E::Rotation)?;
        require(self.velocity.is_finite(), E::LinearVelocity)?;
        require(self.angular_velocity.is_finite(), E::AngularVelocity)?;
        require(
            self.linear_damping.is_finite() && self.linear_damping >= 0.,
            E::LinearDamping,
        )?;
        require(
            self.angular_damping.is_finite() && self.angular_damping >= 0.,
            E::AngularDamping,
        )?;
        require(self.gravity_scale.is_finite(), E::GravityScale)
    }

    fn validate_definition(&self, children: &[Collider]) -> Result<(), BodyValidationError> {
        self.validate_authoring()?;
        require(
            children.len() <= crate::MAX_CHILDREN,
            BodyValidationError::ChildCapacity,
        )?;
        for child in children {
            child.validate_authoring(self.planar)?;
        }
        if self.kind == Kind::Dynamic {
            let properties = crate::mass::compound(children.iter(), self.planar)
                .map_err(|_| BodyValidationError::Mass)?;
            let inverse = properties.mass.recip();
            require(
                inverse.is_finite() && inverse > 0.,
                BodyValidationError::Mass,
            )?;
        }
        Ok(())
    }

    pub(crate) fn new(kind: Kind, planar: bool) -> Self {
        Self {
            kind,
            planar,
            position: Vec3::ZERO,
            rotation: Quat::IDENTITY,
            rotation2: Rotation2::default(),
            velocity: Vec3::ZERO,
            angular_velocity: Vec3::ZERO,
            mass: 0.,
            inv_mass: 0.,
            inv_inertia: crate::mass::Tensor::ZERO,
            local_com: Vec3::ZERO,
            force: Vec3::ZERO,
            torque: Vec3::ZERO,
            gravity_scale: 1.,
            linear_damping: 0.,
            angular_damping: 0.,
            ccd: false,
            sleeping: false,
            quiet: 0.,
            revision: 0,
            solver_dirty: false,
            wake_requested: false,
            next_position: None,
            next_rotation: None,
            colliders: Vec::with_capacity(1),
        }
    }
    // Prepare every fallible value before changing live state. Unlike the
    // legacy setters, a revision-capacity failure is returned, not asserted.
    pub(crate) fn try_teleport(
        &mut self,
        position: Vec3,
        rotation: Quat,
        options: TeleportOptions,
    ) -> Result<(), TeleportError> {
        if !position.is_finite() {
            return Err(TeleportError::Translation);
        }
        let rotation = rotation.try_normalize().ok_or(TeleportError::Rotation)?;
        if !unit_rotation(rotation, self.planar) {
            return Err(TeleportError::Rotation);
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or(TeleportError::RevisionExhausted)?;
        self.position = position;
        self.rotation = rotation;
        if self.planar {
            let axis = rotation * Vec3::X;
            self.rotation2 = Rotation2::from_angle(axis.y.atan2(axis.x));
        }
        if options.velocity == VelocityPolicy::Clear {
            self.velocity = Vec3::ZERO;
            self.angular_velocity = Vec3::ZERO;
        }
        if options.clear_forces {
            self.force = Vec3::ZERO;
            self.torque = Vec3::ZERO;
        }
        if options.clear_kinematic_targets {
            self.next_position = None;
            self.next_rotation = None;
        }
        // Even an identical-pose reset must not reuse pre-reset impulses.
        self.revision = revision;
        self.solver_dirty = true;
        self.wake();
        Ok(())
    }
    pub(crate) fn invalidate_contacts(&mut self) {
        self.revision = self
            .revision
            .checked_add(1)
            .expect("body revision exhausted");
        self.solver_dirty = true;
    }
    pub(crate) fn wake(&mut self) {
        self.wake_requested = true;
        self.wake_internal();
    }
    pub(crate) fn wake_internal(&mut self) {
        self.sleeping = false;
        self.quiet = 0.;
    }
    pub(crate) fn set_rotation(&mut self, q: Quat) {
        assert!(q.is_finite());
        self.rotation = q.normalize();
        if self.planar {
            let axis = self.rotation * Vec3::X;
            self.rotation2 = Rotation2::from_angle(axis.y.atan2(axis.x));
        }
    }
    pub(crate) fn inverse_inertia(&self, v: Vec3) -> Vec3 {
        // Fixed and kinematic bodies have zero inverse inertia. Contacts with
        // floors/walls need no quaternion transforms for that half of the pair.
        if self.kind != Kind::Dynamic {
            Vec3::ZERO
        } else {
            self.rotation * self.inv_inertia.apply(self.rotation.conjugate() * v)
        }
    }
    pub(crate) fn contact_inverse_inertia(&self, v: Vec3) -> Vec3 {
        if self.sleeping {
            Vec3::ZERO
        } else {
            self.inverse_inertia(v)
        }
    }
    pub(crate) fn contact_inv_mass(&self) -> f32 {
        if self.sleeping { 0. } else { self.inv_mass }
    }
    pub(crate) fn point_velocity(&self, p: Vec3) -> Vec3 {
        self.velocity + self.angular_velocity.cross(p - self.world_com())
    }
    pub(crate) fn impulse(&mut self, j: Vec3, p: Vec3) {
        if self.kind == Kind::Dynamic && !self.sleeping {
            self.velocity += j * self.inv_mass;
            self.angular_velocity += self.inverse_inertia((p - self.world_com()).cross(j));
        }
    }
    pub(crate) fn world_com(&self) -> Vec3 {
        self.position + self.rotation * self.local_com
    }
    pub(crate) fn set_mass_properties(&mut self, properties: crate::mass::Mass) {
        let old = self.world_com();
        self.local_com = properties.center;
        self.mass = properties.mass;
        self.inv_mass = 1. / properties.mass;
        self.inv_inertia = properties.inverse;
        self.velocity += self.angular_velocity.cross(self.world_com() - old);
        self.invalidate_contacts();
        self.wake();
    }
    pub(crate) fn integrate(&mut self, dt: f32) {
        let center = self.world_com() + self.velocity * dt;
        self.set_rotation(Quat::from_scaled_axis(self.angular_velocity * dt) * self.rotation);
        self.position = center - self.rotation * self.local_com;
    }
    pub(crate) fn position_impulse(&mut self, linear: Vec3, angular: Vec3) {
        if self.kind != Kind::Dynamic || self.sleeping {
            return;
        }
        let center = self.world_com() + linear * self.inv_mass;
        let delta = self.inverse_inertia(angular).clamp_length_max(0.2);
        if delta.length_squared() > 1e-20 {
            self.set_rotation(Quat::from_scaled_axis(delta) * self.rotation);
        }
        self.position = center - self.rotation * self.local_com;
    }
}
impl<B> BodySet<B> {
    pub(crate) fn two_mut(&mut self, a: RigidBodyHandle, b: RigidBodyHandle) -> (&mut B, &mut B) {
        assert!(a.index != b.index);
        let (sa, sb) = if a.index < b.index {
            let (l, r) = self.slots.split_at_mut(b.index);
            (&mut l[a.index], &mut r[0])
        } else {
            let (l, r) = self.slots.split_at_mut(a.index);
            (&mut r[0], &mut l[b.index])
        };
        assert_eq!(sa.generation, a.generation);
        assert_eq!(sb.generation, b.generation);
        (sa.value.as_mut().unwrap(), sb.value.as_mut().unwrap())
    }
}

// Inherent extensions live here to keep authoring checks together without
// changing the existing dimension-specific types or their constructors.
macro_rules! checked_builders {
    ($planar:expr) => {
        impl RigidBodyBuilder {
            /// Checked translation in world units. Rejects non-finite components.
            pub fn try_translation(self, p: Vector) -> Result<Self, BodyValidationError> {
                require(p.is_finite(), BodyValidationError::Translation)?;
                Ok(self.translation(p))
            }

            /// Checked rotation in radians (a scaled axis in 3D).
            /// Rejects non-finite input or an unrepresentable quaternion conversion.
            pub fn try_rotation(self, r: Angular) -> Result<Self, BodyValidationError> {
                let axis = angular3(r);
                require(axis.is_finite(), BodyValidationError::Rotation)?;
                require(
                    unit_rotation(Quat::from_scaled_axis(axis), $planar),
                    BodyValidationError::Rotation,
                )?;
                Ok(self.rotation(r))
            }

            /// Checked linear velocity in world units per second.
            pub fn try_linvel(self, v: Vector) -> Result<Self, BodyValidationError> {
                require(v.is_finite(), BodyValidationError::LinearVelocity)?;
                Ok(self.linvel(v))
            }

            /// Checked angular velocity in radians per second.
            pub fn try_angvel(self, v: Angular) -> Result<Self, BodyValidationError> {
                require(
                    angular3(v).is_finite(),
                    BodyValidationError::AngularVelocity,
                )?;
                Ok(self.angvel(v))
            }

            /// Checked finite, nonnegative linear damping.
            pub fn try_linear_damping(self, v: f32) -> Result<Self, BodyValidationError> {
                require(v.is_finite() && v >= 0., BodyValidationError::LinearDamping)?;
                Ok(self.linear_damping(v))
            }

            /// Checked finite, nonnegative angular damping.
            pub fn try_angular_damping(self, v: f32) -> Result<Self, BodyValidationError> {
                require(
                    v.is_finite() && v >= 0.,
                    BodyValidationError::AngularDamping,
                )?;
                Ok(self.angular_damping(v))
            }

            /// Checked finite gravity scale; negative scales are allowed.
            pub fn try_gravity_scale(self, v: f32) -> Result<Self, BodyValidationError> {
                require(v.is_finite(), BodyValidationError::GravityScale)?;
                Ok(self.gravity_scale(v))
            }

            /// Builds after checking pose, velocities, damping and gravity scale.
            ///
            /// Mass is assigned by world insertion. To also check the proposed
            /// colliders and dynamic mass, use [`Self::try_build_with_colliders`].
            /// Legacy setters can still panic before this method is reached;
            /// use the `try_*` setters for untrusted numeric input.
            ///
            /// ```
            /// # #[cfg(feature = "dim2")] {
            /// use ferravex_math::Vec2;
            /// use ferravex_physics::d2::RigidBodyBuilder;
            /// let body = RigidBodyBuilder::dynamic()
            ///     .try_translation(Vec2::new(2., 3.)).unwrap()
            ///     .try_rotation(0.5).unwrap()
            ///     .try_linvel(Vec2::X).unwrap()
            ///     .try_angvel(1.).unwrap()
            ///     .try_linear_damping(0.1).unwrap()
            ///     .try_angular_damping(0.2).unwrap()
            ///     .try_gravity_scale(-1.).unwrap()
            ///     .try_build().unwrap();
            /// assert_eq!(body.linvel(), Vec2::X);
            /// assert_eq!(body.mass(), 0.); // Mass is assigned on insertion.
            /// let error = RigidBodyBuilder::fixed().try_linvel(Vec2::NAN).unwrap_err();
            /// assert_eq!(error.to_string(), "linear velocity must be finite");
            /// # }
            /// ```
            pub fn try_build(self) -> Result<RigidBody, BodyValidationError> {
                let body = self.build();
                body.core().validate_authoring()?;
                Ok(body)
            }

            /// Builds and validates the proposed complete collider definition.
            ///
            /// This borrows the colliders without cloning, attaching or inserting
            /// them. Mass remains unassigned until world insertion. Validation
            /// does not reserve world capacity or guarantee stability for future
            /// forces, time steps or extreme finite coordinates/velocities.
            ///
            /// ```
            /// # #[cfg(feature = "dim3")] {
            /// use ferravex_math::{Quat, Vec3};
            /// use ferravex_physics::d3::{ColliderBuilder, PhysicsWorld, RigidBodyBuilder};
            /// let children = [ColliderBuilder::try_cuboid(1., 0.5, 0.25).unwrap()
            ///     .try_density(2.).unwrap()
            ///     .try_translation(Vec3::X).unwrap()
            ///     .try_rotation(Quat::IDENTITY).unwrap()
            ///     .try_build().unwrap()];
            /// let body = RigidBodyBuilder::dynamic()
            ///     .try_rotation(Vec3::Z * 0.3).unwrap()
            ///     .try_angvel(Vec3::Z).unwrap()
            ///     .try_build_with_colliders(&children).unwrap();
            /// body.validate_definition(&children).unwrap();
            /// let mut world = PhysicsWorld::new([0.; 3]);
            /// let handle = world.insert_compound(body, children).unwrap();
            /// assert_eq!(world.bodies[handle].mass(), 2.);
            /// # }
            /// ```
            pub fn try_build_with_colliders(
                self,
                colliders: &[Collider],
            ) -> Result<RigidBody, BodyValidationError> {
                let body = self.build();
                body.validate_definition(colliders)?;
                Ok(body)
            }
        }

        impl RigidBody {
            /// Checks this body's authored state against a proposed collider slice.
            ///
            /// Checks body fields first, then the child limit and each collider in
            /// slice order, then derived mass/inertia. The first failing field is
            /// returned. Fixed/kinematic bodies permit empty or massless geometry;
            /// dynamic bodies need positive combined mass and invertible inertia.
            /// Zero-density children are valid when the combined mass is valid.
            ///
            /// This allocation-free check does not mutate the body or colliders,
            /// inspect attached handles, or alter cached mass. See
            /// [`RigidBodyBuilder::try_build_with_colliders`] for an insertion example.
            pub fn validate_definition(
                &self,
                colliders: &[Collider],
            ) -> Result<(), BodyValidationError> {
                self.core().validate_definition(colliders)
            }
        }

        impl ColliderBuilder {
            /// Checked circle/sphere constructor with a finite positive radius.
            pub fn try_ball(radius: f32) -> Result<Self, BodyValidationError> {
                Shape::Ball(radius).check_dimensions($planar)?;
                Ok(Self::ball(radius))
            }

            /// Checked density in 0..=1e6. Zero is allowed; validate the complete
            /// dynamic definition to ensure its combined mass remains positive.
            pub fn try_density(self, density: f32) -> Result<Self, BodyValidationError> {
                require(valid_density(density), BodyValidationError::Density)?;
                Ok(self.density(density))
            }

            /// Checked body-local offset, within +/-1e6 on each axis.
            pub fn try_translation(self, p: Vector) -> Result<Self, BodyValidationError> {
                require(
                    valid_local_translation(vector3(p), $planar),
                    BodyValidationError::ColliderTranslation,
                )?;
                Ok(self.translation(p))
            }

            /// Checked local rotation, normalized as by the existing setter.
            /// A 3D quaternion must be finite with squared length greater than
            /// 1e-12 and yield a finite unit quaternion on normalization.
            pub fn try_rotation(self, rotation: Rotation) -> Result<Self, BodyValidationError> {
                let q = rotation3(rotation);
                require(
                    q.is_finite()
                        && q.length_squared() > 1e-12
                        && unit_rotation(q.normalize(), $planar),
                    BodyValidationError::ColliderRotation,
                )?;
                Ok(self.rotation(rotation))
            }

            /// Builds after checking dimensions, local pose, density and material.
            ///
            /// Does not require positive mass: massless children and fixed bodies
            /// are supported. Use `RigidBodyBuilder::try_build_with_colliders` to
            /// also validate dynamic mass/inertia. No world state is changed.
            ///
            /// ```
            /// # #[cfg(feature = "dim2")] {
            /// use ferravex_physics::d2::{ColliderBuilder, RigidBodyBuilder};
            /// let child = ColliderBuilder::try_ball(0.5).unwrap()
            ///     .try_density(0.).unwrap().try_build().unwrap();
            /// assert!(RigidBodyBuilder::dynamic()
            ///     .try_build_with_colliders(&[child]).is_err());
            /// assert!(ColliderBuilder::try_ball(f32::NAN).is_err());
            /// # }
            /// ```
            pub fn try_build(self) -> Result<Collider, BodyValidationError> {
                let collider = self.build();
                collider.validate_authoring($planar)?;
                Ok(collider)
            }
        }
    };
}

#[cfg(feature = "dim2")]
mod checked_d2 {
    use super::*;
    use crate::d2::{Angular, ColliderBuilder, RigidBody, RigidBodyBuilder, Rotation, Vector};

    fn vector3(v: Vector) -> Vec3 {
        Vec3::new(v.x, v.y, 0.)
    }
    fn angular3(v: Angular) -> Vec3 {
        Vec3::new(0., 0., v)
    }
    fn rotation3(r: Rotation) -> Quat {
        Quat::from_rotation_z(r.angle())
    }

    checked_builders!(true);

    impl ColliderBuilder {
        /// Checked rectangle constructor. Both half-extents must be finite and positive.
        ///
        /// ```
        /// # #[cfg(feature = "dim2")] {
        /// use ferravex_physics::d2::ColliderBuilder;
        /// assert!(ColliderBuilder::try_cuboid(1., 0.5).unwrap().try_build().is_ok());
        /// assert!(ColliderBuilder::try_cuboid(1., 0.).is_err());
        /// # }
        /// ```
        pub fn try_cuboid(x: f32, y: f32) -> Result<Self, BodyValidationError> {
            Shape::Box(Vec3::new(x, y, 0.5)).check_dimensions(true)?;
            Ok(Self::cuboid(x, y))
        }

        /// Checked capsule constructor. Coincident finite endpoints are allowed;
        /// the radius must be finite and positive.
        ///
        /// ```
        /// # #[cfg(feature = "dim2")] {
        /// use ferravex_math::Vec2;
        /// use ferravex_physics::d2::ColliderBuilder;
        /// let capsule = ColliderBuilder::try_capsule_from_endpoints(Vec2::ZERO, Vec2::Y, 0.2)
        ///     .unwrap().try_build().unwrap();
        /// assert_eq!(capsule.density(), 1.);
        /// # }
        /// ```
        pub fn try_capsule_from_endpoints(
            a: Vector,
            b: Vector,
            radius: f32,
        ) -> Result<Self, BodyValidationError> {
            Shape::Capsule {
                a: vector3(a),
                b: vector3(b),
                radius,
            }
            .check_dimensions(true)?;
            Ok(Self::capsule_from_endpoints(a, b, radius))
        }
    }
}

#[cfg(feature = "dim3")]
mod checked_d3 {
    use super::*;
    use crate::d3::{Angular, ColliderBuilder, RigidBody, RigidBodyBuilder, Rotation, Vector};

    fn vector3(v: Vector) -> Vec3 {
        v
    }
    fn angular3(v: Angular) -> Vec3 {
        v
    }
    fn rotation3(r: Rotation) -> Quat {
        r
    }

    checked_builders!(false);

    impl ColliderBuilder {
        /// Checked box constructor. All three half-extents must be finite and positive.
        /// See `RigidBodyBuilder::try_build_with_colliders` for a complete example.
        pub fn try_cuboid(x: f32, y: f32, z: f32) -> Result<Self, BodyValidationError> {
            Shape::Box(Vec3::new(x, y, z)).check_dimensions(false)?;
            Ok(Self::cuboid(x, y, z))
        }

        /// Checked cylinder constructor with finite positive half-height and radius.
        ///
        /// ```
        /// # #[cfg(feature = "dim3")] {
        /// use ferravex_physics::d3::{ColliderBuilder, RigidBodyBuilder};
        /// let cylinder = ColliderBuilder::try_cylinder(0.5, 0.25).unwrap().try_build().unwrap();
        /// assert!(RigidBodyBuilder::dynamic().try_build_with_colliders(&[cylinder]).is_ok());
        /// # }
        /// ```
        pub fn try_cylinder(half_height: f32, radius: f32) -> Result<Self, BodyValidationError> {
            Shape::Cylinder {
                half_height,
                radius,
            }
            .check_dimensions(false)?;
            Ok(Self::cylinder(half_height, radius))
        }
    }
}

#[cfg(test)]
mod journey200_031 {
    use super::*;
    use BodyValidationError as E;

    macro_rules! authoring_tests {
        ($dimension:ident, $vector:ty, $angular:expr) => {
            mod $dimension {
                use super::*;
                use crate::$dimension::{ColliderBuilder, PhysicsWorld, RigidBodyBuilder};
                type Vector = $vector;

                #[test]
                fn checked_body_fields_reject_bad_inputs_and_preserve_valid_values() {
                    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                        for axis in 0..Vector::ZERO.to_array().len() {
                            let mut components = Vector::ZERO.to_array();
                            components[axis] = invalid;
                            let v = Vector::from_array(components);
                            assert_eq!(RigidBodyBuilder::dynamic().try_translation(v).unwrap_err(), E::Translation);
                            assert_eq!(RigidBodyBuilder::dynamic().try_linvel(v).unwrap_err(), E::LinearVelocity);
                        }
                        assert_eq!(RigidBodyBuilder::dynamic().try_rotation($angular(invalid)).unwrap_err(), E::Rotation);
                        assert_eq!(RigidBodyBuilder::dynamic().try_angvel($angular(invalid)).unwrap_err(), E::AngularVelocity);
                        assert_eq!(RigidBodyBuilder::dynamic().try_gravity_scale(invalid).unwrap_err(), E::GravityScale);
                    }
                    for invalid in [-1., f32::NAN, f32::INFINITY] {
                        assert_eq!(RigidBodyBuilder::dynamic().try_linear_damping(invalid).unwrap_err(), E::LinearDamping);
                        assert_eq!(RigidBodyBuilder::dynamic().try_angular_damping(invalid).unwrap_err(), E::AngularDamping);
                    }
                    assert!(RigidBodyBuilder::dynamic().try_rotation($angular(f32::MAX)).unwrap().try_build().is_ok());
                    let body = RigidBodyBuilder::dynamic()
                        .try_translation(Vector::X * 2.).unwrap()
                        .try_rotation($angular(0.3)).unwrap()
                        .try_linvel(Vector::Y * 3.).unwrap()
                        .try_angvel($angular(0.4)).unwrap()
                        .try_linear_damping(0.1).unwrap()
                        .try_angular_damping(0.2).unwrap()
                        .try_gravity_scale(-1.).unwrap()
                        .ccd_enabled(true)
                        .try_build().unwrap();
                    assert_eq!(body.translation(), Vector::X * 2.);
                    assert_eq!(body.linvel(), Vector::Y * 3.);
                    assert_eq!(body.angvel(), $angular(0.4));
                    assert!(body.is_ccd_enabled());
                    assert_eq!(body.core().linear_damping, 0.1);
                    assert_eq!(body.core().angular_damping, 0.2);
                    assert_eq!(body.core().gravity_scale, -1.);
                    assert!(RigidBodyBuilder::fixed().try_rotation($angular(0.)).unwrap().try_build().is_ok());
                }

                #[test]
                fn collider_checks_dimensions_density_and_local_pose() {
                    for invalid in [0., -0., -1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
                        assert_eq!(ColliderBuilder::try_ball(invalid).unwrap_err(), E::Radius);
                    }
                    for invalid in [-1., 1_000_001., f32::NAN, f32::INFINITY] {
                        assert_eq!(ColliderBuilder::ball(1.).try_density(invalid).unwrap_err(), E::Density);
                    }
                    for valid in [0., -0., 1e6] {
                        let collider = ColliderBuilder::ball(1.).try_density(valid).unwrap().try_build().unwrap();
                        assert_eq!(collider.density(), valid);
                    }
                    for invalid in [Vector::NAN, Vector::X * 1_000_001.] {
                        assert_eq!(ColliderBuilder::ball(1.).try_translation(invalid).unwrap_err(), E::ColliderTranslation);
                        // The legacy setter accepts the out-of-range finite offset.
                        if invalid.is_finite() {
                            assert_eq!(ColliderBuilder::ball(1.).translation(invalid).try_build().unwrap_err(), E::ColliderTranslation);
                        }
                    }
                    assert!(ColliderBuilder::ball(1.).try_translation(Vector::X * 1e6).unwrap().try_build().is_ok());
                }

                #[test]
                fn definition_revalidates_fields_without_mutating_them() {
                    let base = RigidBodyBuilder::dynamic().build();
                    let colliders = [ColliderBuilder::ball(1.).build()];
                    let changes: [(fn(&mut Core), E); 8] = [
                        (|c| c.position.x = f32::NAN, E::Translation),
                        (|c| c.rotation = Quat::from_xyzw(0., 0., 0., 0.), E::Rotation),
                        (|c| c.velocity.y = f32::INFINITY, E::LinearVelocity),
                        (|c| c.angular_velocity.z = f32::NAN, E::AngularVelocity),
                        (|c| c.linear_damping = -1., E::LinearDamping),
                        (|c| c.angular_damping = -1., E::AngularDamping),
                        (|c| c.gravity_scale = f32::NAN, E::GravityScale),
                        (|c| { c.position.x = f32::NAN; c.velocity.y = f32::NAN; }, E::Translation),
                    ];
                    for (change, error) in changes {
                        let mut body = base.clone();
                        change(body.core_mut());
                        let before = format!("{body:?}");
                        assert_eq!(body.validate_definition(&colliders), Err(error));
                        assert_eq!(format!("{body:?}"), before);
                    }
                    let changes: [(fn(&mut Collider), E); 5] = [
                        (|c| c.shape = Shape::Ball(-1.), E::Radius),
                        (|c| c.local_translation = Vec3::NAN, E::ColliderTranslation),
                        (|c| c.local_rotation = Quat::from_xyzw(0., 0., 0., 0.), E::ColliderRotation),
                        (|c| c.density = f32::NAN, E::Density),
                        (|c| c.material.restitution = 2., E::Material),
                    ];
                    for (change, error) in changes {
                        let mut child = colliders[0].clone();
                        change(&mut child);
                        let before = format!("{child:?}");
                        assert_eq!(base.validate_definition(std::slice::from_ref(&child)), Err(error));
                        assert_eq!(format!("{child:?}"), before);
                    }
                }

                #[test]
                fn complete_definition_checks_combined_mass_and_child_bound() {
                    let dynamic = RigidBodyBuilder::dynamic().build();
                    let fixed = RigidBodyBuilder::fixed().build();
                    let kinematic = RigidBodyBuilder::kinematic_position_based().build();
                    assert_eq!(dynamic.validate_definition(&[]), Err(E::Mass));
                    assert_eq!(fixed.validate_definition(&[]), Ok(()));
                    assert_eq!(kinematic.validate_definition(&[]), Ok(()));
                    let massless = ColliderBuilder::ball(1.).density(0.).try_build().unwrap();
                    assert_eq!(dynamic.validate_definition(std::slice::from_ref(&massless)), Err(E::Mass));
                    assert_eq!(fixed.validate_definition(std::slice::from_ref(&massless)), Ok(()));
                    let children = [massless, ColliderBuilder::ball(1.).build()];
                    assert_eq!(dynamic.validate_definition(&children), Ok(()));
                    assert_eq!(children[0].density(), 0.);
                    assert_eq!(dynamic.mass(), 0.);
                    for radius in [1e-30, 1e30] {
                        let extreme = ColliderBuilder::try_ball(radius).unwrap().try_build().unwrap();
                        assert_eq!(dynamic.validate_definition(std::slice::from_ref(&extreme)), Err(E::Mass));
                        assert_eq!(fixed.validate_definition(&[extreme]), Ok(()));
                    }
                    let mut children = vec![ColliderBuilder::ball(1.).build(); crate::MAX_CHILDREN];
                    assert_eq!(dynamic.validate_definition(&children), Ok(()));
                    children.push(ColliderBuilder::ball(1.).build());
                    assert_eq!(dynamic.validate_definition(&children), Err(E::ChildCapacity));
                    assert_eq!(fixed.validate_definition(&children), Err(E::ChildCapacity));
                }

                #[test]
                fn checked_builds_insert_and_step_like_existing_builders() {
                    let children = [ColliderBuilder::try_ball(0.5).unwrap()
                        .try_density(2.).unwrap().try_build().unwrap()];
                    let body = RigidBodyBuilder::dynamic()
                        .try_translation(Vector::Y).unwrap()
                        .try_linvel(Vector::X * 3.).unwrap()
                        .try_angvel($angular(0.2)).unwrap()
                        .try_build_with_colliders(&children).unwrap();
                    let mut checked = PhysicsWorld::new(Vector::ZERO.to_array());
                    let a = checked.insert_compound(body, children).unwrap();
                    let mut legacy = PhysicsWorld::new(Vector::ZERO.to_array());
                    let b = legacy.insert(
                        RigidBodyBuilder::dynamic().translation(Vector::Y)
                            .linvel(Vector::X * 3.).angvel($angular(0.2)),
                        ColliderBuilder::ball(0.5).density(2.),
                    ).0;
                    assert_eq!(checked.bodies[a].mass(), legacy.bodies[b].mass());
                    checked.step(1. / 60.);
                    legacy.step(1. / 60.);
                    assert_eq!(checked.bodies[a].translation(), legacy.bodies[b].translation());
                    assert_eq!(checked.bodies[a].rotation(), legacy.bodies[b].rotation());
                    assert_eq!(checked.bodies[a].linvel(), legacy.bodies[b].linvel());
                    assert_eq!(checked.bodies[a].angvel(), legacy.bodies[b].angvel());
                }

                #[test]
                fn legacy_constructors_keep_their_panics() {
                    assert!(std::panic::catch_unwind(|| ColliderBuilder::ball(0.)).is_err());
                    assert!(std::panic::catch_unwind(|| RigidBodyBuilder::dynamic().linvel(Vector::NAN)).is_err());
                }
            }
        };
    }

    #[cfg(feature = "dim2")]
    authoring_tests!(d2, ferravex_math::Vec2, |v: f32| v);
    #[cfg(feature = "dim3")]
    authoring_tests!(d3, Vec3, |v: f32| Vec3::new(0., 0., v));

    #[cfg(feature = "dim2")]
    #[test]
    fn planar_dimensions_endpoints_and_rotations() {
        use crate::d2::{ColliderBuilder, RigidBodyBuilder};
        use ferravex_math::Vec2;
        for invalid in [0., -1., f32::NAN, f32::INFINITY] {
            assert_eq!(
                ColliderBuilder::try_cuboid(invalid, 1.).unwrap_err(),
                E::HalfExtents
            );
            assert_eq!(
                ColliderBuilder::try_cuboid(1., invalid).unwrap_err(),
                E::HalfExtents
            );
        }
        assert_eq!(
            ColliderBuilder::try_capsule_from_endpoints(Vec2::NAN, Vec2::ZERO, 1.).unwrap_err(),
            E::CapsuleStart
        );
        assert_eq!(
            ColliderBuilder::try_capsule_from_endpoints(Vec2::ZERO, Vec2::NAN, 1.).unwrap_err(),
            E::CapsuleEnd
        );
        assert_eq!(
            ColliderBuilder::try_capsule_from_endpoints(Vec2::ZERO, Vec2::ZERO, 0.).unwrap_err(),
            E::Radius
        );
        let body = RigidBodyBuilder::dynamic().build();
        let point_capsule = ColliderBuilder::try_capsule_from_endpoints(Vec2::ZERO, Vec2::ZERO, 1.)
            .unwrap()
            .try_rotation(Rotation2::from_angle(0.3))
            .unwrap()
            .try_build()
            .unwrap();
        assert_eq!(body.validate_definition(&[point_capsule]), Ok(()));
        let mut child = ColliderBuilder::ball(1.).build();
        child.local_rotation = Quat::from_rotation_x(0.5);
        assert_eq!(
            body.validate_definition(std::slice::from_ref(&child)),
            Err(E::ColliderRotation)
        );
        child.local_rotation = Quat::IDENTITY;
        child.local_translation.z = 1.;
        assert_eq!(
            body.validate_definition(&[child]),
            Err(E::ColliderTranslation)
        );
    }

    #[cfg(feature = "dim3")]
    #[test]
    fn spatial_dimensions_and_zero_quaternion_are_checked() {
        use crate::d3::{ColliderBuilder, RigidBodyBuilder};
        assert_eq!(
            RigidBodyBuilder::dynamic()
                .try_rotation(Vec3::splat(f32::MAX))
                .unwrap_err(),
            E::Rotation
        );
        for invalid in [0., -1., f32::NAN, f32::INFINITY] {
            assert_eq!(
                ColliderBuilder::try_cuboid(invalid, 1., 1.).unwrap_err(),
                E::HalfExtents
            );
            assert_eq!(
                ColliderBuilder::try_cuboid(1., invalid, 1.).unwrap_err(),
                E::HalfExtents
            );
            assert_eq!(
                ColliderBuilder::try_cuboid(1., 1., invalid).unwrap_err(),
                E::HalfExtents
            );
            assert_eq!(
                ColliderBuilder::try_cylinder(invalid, 1.).unwrap_err(),
                E::HalfHeight
            );
            assert_eq!(
                ColliderBuilder::try_cylinder(1., invalid).unwrap_err(),
                E::Radius
            );
        }
        for invalid in [
            Quat::from_xyzw(0., 0., 0., 0.),
            Quat::NAN,
            Quat::from_xyzw(0., 0., 0., 1e-7),
        ] {
            assert_eq!(
                ColliderBuilder::ball(1.).try_rotation(invalid).unwrap_err(),
                E::ColliderRotation
            );
        }
        let child = ColliderBuilder::try_cylinder(0.5, 1.)
            .unwrap()
            .try_rotation(Quat::from_xyzw(0., 0., 0., 2.))
            .unwrap()
            .try_build()
            .unwrap();
        assert_eq!(child.local_rotation(), Quat::IDENTITY);
        assert_eq!(
            RigidBodyBuilder::dynamic()
                .build()
                .validate_definition(&[child]),
            Ok(())
        );
    }

    #[cfg(all(feature = "dim2", feature = "dim3"))]
    #[test]
    fn spatial_cylinder_is_rejected_in_planar_definition() {
        let child = crate::d3::ColliderBuilder::cylinder(1., 1.).build();
        assert_eq!(
            crate::d2::RigidBodyBuilder::fixed()
                .build()
                .validate_definition(&[child]),
            Err(E::ShapeDimension)
        );
    }

    #[test]
    fn errors_are_field_specific_and_implement_std_error() {
        fn message(error: &dyn std::error::Error) -> String {
            error.to_string()
        }
        assert_eq!(
            message(&E::Density),
            "density must be finite and in 0..=1e6"
        );
        assert_ne!(message(&E::Mass), message(&E::Density));
        assert_ne!(message(&E::Translation), message(&E::ColliderTranslation));
    }
}

#[cfg(test)]
mod journey400_221 {
    use super::*;

    fn prepared(kind: Kind, planar: bool) -> Core {
        let mut core = Core::new(kind, planar);
        core.velocity = Vec3::X;
        core.angular_velocity = Vec3::Z;
        core.force = Vec3::Y;
        core.torque = Vec3::Z * 2.;
        core.next_position = Some(Vec3::X * 5.);
        core.next_rotation = Some(Quat::from_rotation_z(0.5));
        core.sleeping = true;
        core.quiet = 0.8;
        core
    }

    #[test]
    fn revision_capacity_failure_preserves_every_field_for_every_body_kind() {
        let options = TeleportOptions::new(VelocityPolicy::Clear);
        for planar in [false, true] {
            for kind in [Kind::Fixed, Kind::Dynamic, Kind::Kinematic] {
                let mut core = prepared(kind, planar);
                core.revision = u64::MAX - 1;
                core.try_teleport(Vec3::Y, Quat::from_rotation_z(0.7), options)
                    .unwrap();
                assert_eq!(core.revision, u64::MAX);
                assert!(core.solver_dirty && core.wake_requested);
                assert!(!core.sleeping);
                assert_eq!(core.quiet, 0.);
                // Restage state that a failed reset must not consume.
                core.force = Vec3::X;
                core.torque = Vec3::Y;
                core.next_position = Some(Vec3::X * 3.);
                core.next_rotation = Some(Quat::IDENTITY);
                core.velocity = Vec3::Y;
                core.angular_velocity = Vec3::Z;
                let before = format!("{core:?}");
                assert_eq!(
                    core.try_teleport(Vec3::X, Quat::IDENTITY, options),
                    Err(TeleportError::RevisionExhausted),
                );
                assert_eq!(format!("{core:?}"), before);
                assert_eq!(
                    core.try_teleport(Vec3::X, Quat::NAN, options),
                    Err(TeleportError::Rotation),
                );
                assert_eq!(
                    core.try_teleport(Vec3::NAN, Quat::NAN, options),
                    Err(TeleportError::Translation),
                );
                assert_eq!(format!("{core:?}"), before);
            }
        }
    }

    #[test]
    fn planar_invalid_rotation_leaves_sleep_forces_targets_and_pose_unchanged() {
        let mut core = prepared(Kind::Kinematic, true);
        let before = format!("{core:?}");
        for rotation in [
            Quat::NAN,
            Quat::from_array([0.; 4]),
            Quat::from_rotation_x(0.5),
            Quat::from_rotation_y(0.5),
        ] {
            assert_eq!(
                core.try_teleport(
                    Vec3::X,
                    rotation,
                    TeleportOptions::new(VelocityPolicy::Clear)
                ),
                Err(TeleportError::Rotation),
            );
            assert_eq!(format!("{core:?}"), before);
        }
    }

    #[test]
    fn reset_options_control_only_the_requested_motion_fields() {
        for velocity in [VelocityPolicy::Clear, VelocityPolicy::Preserve] {
            for clear_forces in [false, true] {
                for clear_kinematic_targets in [false, true] {
                    let mut core = prepared(Kind::Dynamic, false);
                    let before = core.clone();
                    core.try_teleport(
                        Vec3::X * 2.,
                        Quat::IDENTITY,
                        TeleportOptions {
                            velocity,
                            clear_forces,
                            clear_kinematic_targets,
                        },
                    )
                    .unwrap();
                    assert_eq!(
                        core.force,
                        if clear_forces {
                            Vec3::ZERO
                        } else {
                            before.force
                        }
                    );
                    assert_eq!(
                        core.torque,
                        if clear_forces {
                            Vec3::ZERO
                        } else {
                            before.torque
                        }
                    );
                    assert_eq!(
                        core.next_position,
                        if clear_kinematic_targets {
                            None
                        } else {
                            before.next_position
                        }
                    );
                    assert_eq!(
                        core.next_rotation,
                        if clear_kinematic_targets {
                            None
                        } else {
                            before.next_rotation
                        }
                    );
                    assert_eq!(
                        core.velocity,
                        if velocity == VelocityPolicy::Clear {
                            Vec3::ZERO
                        } else {
                            before.velocity
                        }
                    );
                    assert_eq!(
                        core.angular_velocity,
                        if velocity == VelocityPolicy::Clear {
                            Vec3::ZERO
                        } else {
                            before.angular_velocity
                        }
                    );
                    assert_eq!(core.revision, before.revision + 1);
                }
            }
        }
    }
}
