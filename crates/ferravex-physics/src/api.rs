pub use crate::{ColliderHandle, RigidBodyHandle};
pub use body::{ColliderShapeError, TeleportError, TeleportOptions, VelocityPolicy};

/// Authored body-origin pose in world units, independent of center-of-mass offset.
/// A teleport accepts non-unit 3D rotations when they can be normalized. Its
/// returned pose contains the actual committed rotation (wrapped in 2D).
/// See [`RigidBody::try_teleport`] and [`PhysicsWorld::try_teleport_body`] for
/// runnable reset and host interpolation recipes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RigidBodyPose {
    pub translation: Vector,
    pub rotation: Rotation,
}

/// Rigid body state. Waking requests activation of its contact island on the next step.
#[derive(Clone, Debug)]
pub struct RigidBody {
    core: body::Core,
}
impl body::Access for RigidBody {
    fn core(&self) -> &body::Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut body::Core {
        &mut self.core
    }
}
impl RigidBody {
    pub fn translation(&self) -> Vector {
        from3(self.core.position)
    }
    pub fn rotation(&self) -> &Rotation {
        rotation(&self.core)
    }
    pub fn linvel(&self) -> Vector {
        from3(self.core.velocity)
    }
    pub fn angvel(&self) -> Angular {
        angular_from3(self.core.angular_velocity)
    }
    pub fn center_of_mass(&self) -> Vector {
        from3(self.core.world_com())
    }
    pub fn local_center_of_mass(&self) -> Vector {
        from3(self.core.local_com)
    }
    pub fn velocity_at_point(&self, p: Vector) -> Vector {
        from3(self.core.point_velocity(to3(p)))
    }
    pub fn mass(&self) -> f32 {
        self.core.mass
    }
    pub fn colliders(&self) -> &[ColliderHandle] {
        &self.core.colliders
    }
    pub fn is_ccd_enabled(&self) -> bool {
        self.core.ccd
    }
    pub fn enable_ccd(&mut self, enabled: bool) {
        self.core.ccd = enabled;
    }
    pub fn is_sleeping(&self) -> bool {
        self.core.sleeping
    }
    pub fn wake_up(&mut self, _strong: bool) {
        self.core.wake();
    }
    /// Atomically set the body-origin pose and apply independent reset policies.
    ///
    /// Rejects non-finite translation, invalid rotation and exhausted contact
    /// revision before changing anything. Rotation uses the math crate's checked
    /// normalization (zero and non-finite quaternions are rejected). Success
    /// always invalidates cached contact/joint impulses and wakes this body,
    /// including an identical-pose reset. Old/new solid contact and enabled-joint
    /// islands wake on the next valid world step, using existing revision flags.
    /// Sensors do not connect wake islands. No simulation tick is executed here.
    ///
    /// Identity, children, joint anchors/configuration, mass, CCD, gravity scale
    /// and damping are retained. `Preserve` retains stored world-space COM and
    /// angular velocities, not origin-point velocity or angular momentum. A
    /// kinematic body's next step still derives its velocities from any retained
    /// targets (or zero with no targets). Retained targets can move it away from
    /// the new pose on that step; clearing targets prevents this snap back.
    /// Forces/torque retained by policy apply on the next step as usual.
    ///
    /// The returned committed pose lets hosts snap both interpolation samples.
    /// Last-step contacts/manifolds/events remain snapshots until the next valid
    /// step; their impulses cannot be reused after this revision change. This
    /// operation performs no destination search or overlap rejection and allocates
    /// no storage. Existing joints may correct the pose or break on a later step.
    ///
    /// ```
    /// # #[cfg(feature = "dim2")] {
    /// use ferravex_math::Vec2;
    /// use ferravex_physics::{Rotation2, d2::{
    ///     ColliderBuilder, PhysicsWorld, RigidBodyBuilder, RigidBodyPose,
    ///     TeleportOptions, VelocityPolicy,
    /// }};
    /// let mut world = PhysicsWorld::default();
    /// let (ball, child) = world.insert(
    ///     RigidBodyBuilder::dynamic().linvel(Vec2::X), ColliderBuilder::ball(0.5),
    /// );
    /// world.bodies[ball].add_force(Vec2::Y * 20., true);
    /// let committed = world.bodies[ball].try_teleport(
    ///     RigidBodyPose { translation: Vec2::new(2., 3.), rotation: Rotation2::default() },
    ///     TeleportOptions::new(VelocityPolicy::Clear),
    /// ).unwrap();
    /// world.step(1. / 60.);
    /// assert_eq!(world.bodies[ball].translation(), committed.translation);
    /// assert_eq!(world.bodies[ball].linvel(), Vec2::ZERO);
    /// assert_eq!(world.bodies[ball].colliders(), &[child]);
    /// # }
    /// ```
    pub fn try_teleport(
        &mut self,
        pose: RigidBodyPose,
        options: TeleportOptions,
    ) -> Result<RigidBodyPose, TeleportError> {
        self.core
            .try_teleport(to3(pose.translation), rotation_to3(pose.rotation), options)?;
        Ok(RigidBodyPose {
            translation: self.translation(),
            rotation: *self.rotation(),
        })
    }
    pub fn set_translation(&mut self, p: Vector, wake: bool) {
        assert!(p.is_finite());
        if self.core.position != to3(p) {
            self.core.invalidate_contacts();
        }
        self.core.position = to3(p);
        if wake {
            self.core.wake();
        }
    }
    pub fn set_rotation(&mut self, q: Rotation, wake: bool) {
        self.core.invalidate_contacts();
        self.core.set_rotation(rotation_to3(q));
        if wake {
            self.core.wake();
        }
    }
    pub fn set_linvel(&mut self, v: Vector, wake: bool) {
        assert!(v.is_finite());
        self.core.velocity = to3(v);
        if wake {
            self.core.wake();
        }
    }
    pub fn set_angvel(&mut self, v: Angular, wake: bool) {
        let v = angular_to3(v);
        assert!(v.is_finite());
        self.core.angular_velocity = v;
        if wake {
            self.core.wake();
        }
    }
    pub fn set_gravity_scale(&mut self, scale: f32, wake: bool) {
        assert!(scale.is_finite());
        if self.core.gravity_scale != scale {
            self.core.invalidate_contacts();
        }
        self.core.gravity_scale = scale;
        if wake {
            self.core.wake();
        }
    }
    pub fn apply_impulse(&mut self, impulse: Vector, wake: bool) {
        let v = to3(impulse);
        assert!(v.is_finite());
        self.core.velocity += v * self.core.inv_mass;
        if wake {
            self.core.wake();
        }
    }
    pub fn apply_impulse_at_point(&mut self, impulse: Vector, point: Vector, wake: bool) {
        let j = to3(impulse);
        let p = to3(point);
        assert!(j.is_finite() && p.is_finite());
        let torque = (p - self.core.world_com()).cross(j);
        self.core.velocity += j * self.core.inv_mass;
        self.core.angular_velocity += self.core.inverse_inertia(torque);
        if wake {
            self.core.wake();
        }
    }
    pub fn apply_torque_impulse(&mut self, impulse: Angular, wake: bool) {
        let v = angular_to3(impulse);
        assert!(v.is_finite());
        self.core.angular_velocity += self.core.inverse_inertia(v);
        if wake {
            self.core.wake();
        }
    }
    pub fn add_force(&mut self, force: Vector, wake: bool) {
        let f = to3(force);
        assert!(f.is_finite());
        self.core.force += f;
        if wake {
            self.core.wake();
        }
    }
    pub fn add_torque(&mut self, torque: Angular, wake: bool) {
        let t = angular_to3(torque);
        assert!(t.is_finite());
        self.core.torque += t;
        if wake {
            self.core.wake();
        }
    }
    pub fn set_next_kinematic_translation(&mut self, p: Vector) {
        assert!(p.is_finite());
        self.core.next_position = Some(to3(p));
    }
    pub fn set_next_kinematic_rotation(&mut self, q: Rotation) {
        self.core.next_rotation = Some(rotation_to3(q));
    }
}
#[derive(Clone, Debug)]
pub struct RigidBodyBuilder {
    core: body::Core,
}
impl RigidBodyBuilder {
    pub fn dynamic() -> Self {
        Self {
            core: body::Core::new(body::Kind::Dynamic, PLANAR),
        }
    }
    pub fn fixed() -> Self {
        Self {
            core: body::Core::new(body::Kind::Fixed, PLANAR),
        }
    }
    pub fn kinematic_position_based() -> Self {
        Self {
            core: body::Core::new(body::Kind::Kinematic, PLANAR),
        }
    }
    pub fn translation(mut self, p: Vector) -> Self {
        assert!(p.is_finite());
        self.core.position = to3(p);
        self
    }
    pub fn rotation(mut self, r: Angular) -> Self {
        self.core
            .set_rotation(Quat::from_scaled_axis(angular_to3(r)));
        self
    }
    pub fn linvel(mut self, v: Vector) -> Self {
        assert!(v.is_finite());
        self.core.velocity = to3(v);
        self
    }
    pub fn angvel(mut self, v: Angular) -> Self {
        let v = angular_to3(v);
        assert!(v.is_finite());
        self.core.angular_velocity = v;
        self
    }
    pub fn linear_damping(mut self, v: f32) -> Self {
        assert!(v.is_finite() && v >= 0.);
        self.core.linear_damping = v;
        self
    }
    pub fn angular_damping(mut self, v: f32) -> Self {
        assert!(v.is_finite() && v >= 0.);
        self.core.angular_damping = v;
        self
    }
    pub fn gravity_scale(mut self, v: f32) -> Self {
        assert!(v.is_finite());
        self.core.gravity_scale = v;
        self
    }
    pub fn ccd_enabled(mut self, v: bool) -> Self {
        self.core.ccd = v;
        self
    }
    pub fn build(self) -> RigidBody {
        self.into()
    }
}
impl From<RigidBodyBuilder> for RigidBody {
    fn from(b: RigidBodyBuilder) -> Self {
        Self { core: b.core }
    }
}
#[derive(Clone, Debug)]
pub struct ColliderBuilder {
    collider: Collider,
}
impl ColliderBuilder {
    fn shape(shape: body::Shape) -> Self {
        shape.validate();
        Self {
            collider: Collider::new(shape),
        }
    }
    pub fn translation(mut self, offset: Vector) -> Self {
        assert!(offset.is_finite());
        self.collider.local_translation = to3(offset);
        self
    }
    pub fn rotation(mut self, rotation: Rotation) -> Self {
        let q = rotation_to3(rotation);
        assert!(q.is_finite() && q.length_squared() > 1e-12);
        self.collider.local_rotation = q.normalize();
        self
    }
    pub fn ball(radius: f32) -> Self {
        Self::shape(body::Shape::Ball(radius))
    }
    pub fn density(mut self, v: f32) -> Self {
        assert!(v.is_finite() && (0. ..=1e6).contains(&v));
        self.collider.density = v;
        self
    }
    pub fn friction(mut self, v: f32) -> Self {
        assert!(v.is_finite() && v >= 0.);
        self.collider.set_friction(v);
        self
    }
    pub fn material(mut self, v: crate::PhysicsMaterial) -> Self {
        self.collider.set_material(v).expect("valid material");
        self
    }
    pub fn restitution(mut self, v: f32) -> Self {
        self.collider.set_restitution(v);
        self
    }
    pub fn sensor(mut self, v: bool) -> Self {
        self.collider.sensor = v;
        self
    }
    /// Collision layers match when each collider's membership intersects the other's filter.
    pub fn collision_layers(mut self, membership: u32, filter: u32) -> Self {
        self.collider.membership = membership;
        self.collider.filter = filter;
        self
    }
    pub fn build(self) -> Collider {
        self.into()
    }
}
impl From<ColliderBuilder> for Collider {
    fn from(b: ColliderBuilder) -> Self {
        b.collider
    }
}

/// Which colliders participate in a query. Layers and the ignored body still
/// apply to every policy. See [`QueryOptions::sensors`] for usage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SensorPolicy {
    /// Query solid colliders only, matching the original cast methods.
    #[default]
    Exclude,
    /// Query both solid colliders and sensors.
    Include,
    /// Query sensors only, useful for selecting trigger volumes.
    Only,
}

/// Reusable distance and filtering options for casts and direct overlap queries.
///
/// Construct with an explicit maximum distance; the initial policy queries all
/// membership layers, excludes sensors, and ignores no body. Builder methods
/// can be chained, or the named fields can be edited directly. Invalid numeric
/// inputs are rejected without changing the world: nearest casts return `None`,
/// while caller-buffer casts return [`QueryError`].
///
/// ```
/// # #[cfg(feature = "dim3")] {
/// use ferravex_physics::d3::{PhysicsWorld, QueryOptions};
/// let mut world = PhysicsWorld::default();
/// let player = world.add_ball([0.; 3], 0.5, true);
/// let wall = world.add_box([5., 0., 0.], [0.5; 3], false);
/// let options = QueryOptions::new(10.).layers(0b10).ignore_body(player);
/// let hit = world.raycast_with_options([0.; 3], [1., 0., 0.], options).unwrap();
/// assert_eq!(hit.body, Some(wall));
/// // Reuse the same filters for a camera-volume sweep.
/// let camera = world.spherecast_with_options([0.; 3], [1., 0., 0.], 0.25, options)
///     .unwrap();
/// assert!(camera.distance < hit.distance);
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[must_use]
pub struct QueryOptions {
    /// Maximum travel distance for casts, or maximum signed separation for
    /// direct overlap queries, in world units. Finite and nonnegative.
    pub max_distance: f32,
    /// A collider participates when its membership intersects these bits.
    /// Its collision-response filter is not consulted. Zero matches nothing.
    pub layers: u32,
    /// Exclude all compound children of this body. A removed handle does not
    /// exclude a new body that later reuses the same slot.
    pub ignored_body: Option<RigidBodyHandle>,
    /// Whether sensors, solid colliders, or both can be hit.
    pub sensors: SensorPolicy,
}
impl QueryOptions {
    /// Query up to `max_distance` world units with all layers and no sensors.
    /// The query validates the distance, so construction does not panic.
    pub const fn new(max_distance: f32) -> Self {
        Self {
            max_distance,
            layers: u32::MAX,
            ignored_body: None,
            sensors: SensorPolicy::Exclude,
        }
    }

    // All current-pose queries share membership, sensor and generational exclusions.
    fn accepts(self, collider: &Collider) -> bool {
        let sensor = match self.sensors {
            SensorPolicy::Exclude => !collider.sensor,
            SensorPolicy::Include => true,
            SensorPolicy::Only => collider.sensor,
        };
        sensor && collider.membership & self.layers != 0 && collider.parent != self.ignored_body
    }

    /// Select collider membership bits; this has the same meaning as the
    /// original [`PhysicsWorld::spherecast`] `mask` argument.
    pub const fn layers(mut self, mask: u32) -> Self {
        self.layers = mask;
        self
    }

    /// Exclude a body and all its compound children. Handles must come from
    /// the queried world; they do not encode a world identity.
    pub const fn ignore_body(mut self, body: RigidBodyHandle) -> Self {
        self.ignored_body = Some(body);
        self
    }

    /// Select sensors independently of the layer and body filters.
    ///
    /// ```
    /// # #[cfg(feature = "dim2")] {
    /// use ferravex_physics::d2::{
    ///     ColliderBuilder, PhysicsWorld, QueryOptions, RigidBodyBuilder, SensorPolicy,
    /// };
    /// let mut world = PhysicsWorld::default();
    /// let (trigger, _) = world.insert(
    ///     RigidBodyBuilder::fixed(), ColliderBuilder::ball(1.).sensor(true),
    /// );
    /// let options = QueryOptions::new(5.).sensors(SensorPolicy::Only);
    /// let hit = world.raycast_with_options([-3., 0.], [1., 0.], options).unwrap();
    /// assert_eq!(hit.body, Some(trigger));
    /// # }
    /// ```
    pub const fn sensors(mut self, policy: SensorPolicy) -> Self {
        self.sensors = policy;
        self
    }
}

/// Counts returned by caller-buffer casts and direct overlap queries.
/// See [`PhysicsWorld::spherecast_into`] and [`PhysicsWorld::overlap_sphere_into`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[must_use]
pub struct QueryReport {
    /// Number of `Some` hits written to the start of the output slice.
    pub written: usize,
    /// Total accepted collider hits, including those that did not fit.
    /// Compound children count separately; this is not a unique-body count.
    pub total: usize,
}
impl QueryReport {
    /// Whether the output omitted any accepted hits. An exactly full buffer
    /// is not truncated if every hit fit; an empty buffer can be truncated.
    pub const fn is_truncated(self) -> bool {
        self.total > self.written
    }
}

/// Invalid input to a caller-buffer query. Errors leave its buffer unchanged.
/// Validation runs even for empty buffers, empty worlds or a zero layer mask,
/// in the order of these variants (direct overlaps omit direction/endpoint
/// checks). A valid miss returns `Ok(QueryReport)`.
///
/// ```
/// # #[cfg(feature = "dim2")] {
/// use ferravex_physics::d2::{PhysicsWorld, QueryError, QueryOptions};
/// let world = PhysicsWorld::default();
/// let error = world.raycast_into([0.; 2], [0.; 2], QueryOptions::new(5.), &mut [])
///     .unwrap_err();
/// assert_eq!(error, QueryError::ZeroDirection);
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryError {
    /// At least one origin component is not finite.
    NonFiniteOrigin,
    /// At least one direction component is not finite.
    NonFiniteDirection,
    /// Maximum distance is negative or not finite.
    InvalidMaxDistance,
    /// Sphere radius is not finite or outside `0..=1000`.
    InvalidRadius,
    /// Cast direction has zero length, including for a zero-distance cast.
    ZeroDirection,
    /// The endpoint computed from the normalized direction is not finite.
    NonFiniteEndpoint,
}
impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NonFiniteOrigin => "query origin must be finite",
            Self::NonFiniteDirection => "query direction must be finite",
            Self::InvalidMaxDistance => "query maximum distance must be finite and nonnegative",
            Self::InvalidRadius => "query radius must be finite and in 0..=1000",
            Self::ZeroDirection => "query direction must be nonzero",
            Self::NonFiniteEndpoint => "query endpoint must be finite",
        })
    }
}
impl std::error::Error for QueryError {}

// Shared validation, filtering and geometry keep nearest and buffered queries
// on the same current-pose path. Nearest queries retain their shrinking limit.
struct PreparedQuery {
    origin: Vec3,
    direction: Vec3,
    radius: f32,
    options: QueryOptions,
}
impl PreparedQuery {
    fn new(
        origin: [f32; DIM],
        direction: [f32; DIM],
        radius: f32,
        options: QueryOptions,
    ) -> Result<Self, QueryError> {
        let origin = to3(Vector::from_array(origin));
        let direction = to3(Vector::from_array(direction));
        if !origin.is_finite() {
            return Err(QueryError::NonFiniteOrigin);
        }
        if !direction.is_finite() {
            return Err(QueryError::NonFiniteDirection);
        }
        if !options.max_distance.is_finite() || options.max_distance < 0. {
            return Err(QueryError::InvalidMaxDistance);
        }
        if !radius.is_finite() || !(0. ..=1000.).contains(&radius) {
            return Err(QueryError::InvalidRadius);
        }
        let direction = direction.try_normalize().ok_or(QueryError::ZeroDirection)?;
        if !(origin + direction * options.max_distance).is_finite() {
            return Err(QueryError::NonFiniteEndpoint);
        }
        Ok(Self {
            origin,
            direction,
            radius,
            options,
        })
    }

    fn cast(
        &self,
        world: &PhysicsWorld,
        id: ColliderHandle,
        collider: &Collider,
        limit: f32,
    ) -> Option<RayHit> {
        if !self.options.accepts(collider) {
            return None;
        }
        let body = collider.parent?;
        let pose = collider.pose(&world.raw.bodies[body].core);
        let (distance, normal) = geometry::distance(
            collider.shape,
            pose.position,
            pose.rotation,
            self.origin,
            PLANAR,
        );
        let (distance, normal) = if distance <= self.radius {
            (0., normal)
        } else {
            let (time, normal) = geometry::sweep_ball(
                self.origin,
                self.origin + self.direction * limit,
                self.radius,
                collider.shape,
                pose.position,
                pose.rotation,
                PLANAR,
            )?;
            (time * limit, normal)
        };
        let point = self.origin + self.direction * distance;
        if !distance.is_finite() || !point.is_finite() || !normal.is_finite() {
            return None;
        }
        Some(RayHit {
            body: Some(body),
            collider: id,
            distance,
            point: from3(point).to_array(),
            normal: from3(normal).to_array(),
        })
    }
}

// Direct probes reuse the signed-distance kernel, without a sweep or direction.
struct PreparedOverlap {
    center: Vec3,
    radius: f32,
    options: QueryOptions,
}
impl PreparedOverlap {
    fn new(center: [f32; DIM], radius: f32, options: QueryOptions) -> Result<Self, QueryError> {
        let center = to3(Vector::from_array(center));
        if !center.is_finite() {
            return Err(QueryError::NonFiniteOrigin);
        }
        if !options.max_distance.is_finite() || options.max_distance < 0. {
            return Err(QueryError::InvalidMaxDistance);
        }
        if !radius.is_finite() || !(0. ..=1000.).contains(&radius) {
            return Err(QueryError::InvalidRadius);
        }
        Ok(Self {
            center,
            radius,
            options,
        })
    }

    fn hit(
        &self,
        world: &PhysicsWorld,
        id: ColliderHandle,
        collider: &Collider,
    ) -> Option<OverlapHit> {
        if !self.options.accepts(collider) {
            return None;
        }
        let body = collider.parent?;
        let pose = collider.pose(&world.raw.bodies.get(body)?.core);
        let (distance, normal) = geometry::distance(
            collider.shape,
            pose.position,
            pose.rotation,
            self.center,
            PLANAR,
        );
        let separation = distance - self.radius;
        let collider_point = self.center - normal * distance;
        let probe_point = self.center - normal * self.radius;
        if !separation.is_finite()
            || separation > self.options.max_distance
            || !normal.is_finite()
            || !collider_point.is_finite()
            || !probe_point.is_finite()
        {
            return None;
        }
        Some(OverlapHit {
            body: Some(body),
            collider: id,
            separation,
            collider_point: from3(collider_point).to_array(),
            probe_point: from3(probe_point).to_array(),
            normal: from3(normal).to_array(),
        })
    }
}

pub type RawPhysicsWorld = world::World<RigidBody>;
pub struct PhysicsWorld {
    pub raw: RawPhysicsWorld,
}
impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new([0.; DIM])
    }
}
impl std::ops::Deref for PhysicsWorld {
    type Target = RawPhysicsWorld;
    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}
impl std::ops::DerefMut for PhysicsWorld {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.raw
    }
}
const DIM: usize = if PLANAR { 2 } else { 3 };
impl PhysicsWorld {
    pub fn new(gravity: [f32; DIM]) -> Self {
        Self {
            raw: world::World::new(to3(Vector::from_array(gravity)), PLANAR),
        }
    }
    /// Derive body-local anchors, hinge axes and reference rotation from current
    /// live body poses. See [`WorldJointFrame`] for executable authoring recipes.
    ///
    /// Frame anchors replace the description's local anchors. For hinges, the
    /// finite nonzero world axis is normalized (including tiny/huge scales), then
    /// transformed into each body's local space; old local axes are replaced.
    /// In 2D only +Z is accepted, within [`crate::JointDesc::validate`]'s tolerance.
    /// The axis is ignored for other kinds. Origins are authored body origins,
    /// independent of compound center-of-mass offsets and child collider poses.
    ///
    /// [`JointPoseMode::Preserve`] captures current relative rotation and the
    /// distance/spring rest length, replacing those description fields. Fixed
    /// and hinge joints require identical world pivots in this mode. It removes
    /// initial geometric/rest error within floating-point rounding; retained
    /// limits, motors, velocities, forces, contacts and later body edits may
    /// still move the bodies on the next tick. It does not freeze simulation.
    /// [`JointPoseMode::Authored`] retains the description's rest length and
    /// explicit reference rotation; `None` captures relative rotation **now**.
    ///
    /// Checks both generational handles first (`InvalidHandle`), then distinct
    /// bodies with at least one dynamic endpoint (`InvalidJoint`), then finite
    /// current positions and unit rotations (`InvalidBody`). Invalid world
    /// geometry, separated preserve-mode fixed/hinge pivots, or a rejected final
    /// `JointDesc::validate` returns `InvalidJoint`. Handles must belong to this
    /// world; they do not encode world identity. No capacity check, mutation,
    /// allocation, wake, cache invalidation or timestep occurs. This reads poses
    /// immediately after setters/teleport without a step or query refresh.
    /// Caller-defined `Into<JointDesc>` code is outside these guarantees.
    ///
    /// The returned description is a snapshot, not a live binding. Insert it
    /// before moving either body, or use [`Self::insert_joint_from_world`].
    pub fn joint_desc_from_world(
        &self,
        a: RigidBodyHandle,
        b: RigidBodyHandle,
        desc: impl Into<crate::JointDesc>,
        frame: WorldJointFrame,
        pose: JointPoseMode,
    ) -> Result<crate::JointDesc, crate::PhysicsError> {
        self.raw.joint_desc_from_world(
            a,
            b,
            desc.into(),
            frame.anchors.map(to3),
            frame.axis,
            matches!(pose, JointPoseMode::Preserve),
        )
    }

    /// Author with [`Self::joint_desc_from_world`] and immediately use the
    /// existing joint insertion path. See [`WorldJointFrame`] for examples.
    ///
    /// Retains insertion's `JointCapacity` error, bounded storage, wake/filter
    /// behavior and allocations. Ordinary errors leave the world unchanged;
    /// authoring itself allocates nothing, but successful insertion can grow
    /// the existing arenas/scratch storage. No new solver or stepping policy.
    pub fn insert_joint_from_world(
        &mut self,
        a: RigidBodyHandle,
        b: RigidBodyHandle,
        desc: impl Into<crate::JointDesc>,
        frame: WorldJointFrame,
        pose: JointPoseMode,
    ) -> Result<crate::JointHandle, crate::PhysicsError> {
        let desc = self.joint_desc_from_world(a, b, desc, frame, pose)?;
        self.raw.insert_joint(a, b, desc)
    }
    /// Handle-checked [`RigidBody::try_teleport`], with the same atomic reset,
    /// deferred cache/wake and no-allocation behavior. Checks the handle before
    /// pose fields; a removed handle never edits a replacement in its old slot.
    /// Handles must belong to this world. Works for dynamic, fixed and kinematic
    /// bodies and needs no query refresh. No body/collider/joint is recreated.
    ///
    /// This checkpoint recipe updates host interpolation only after success.
    /// Replace the two samples with the J400 202 interpolation adapter's snap
    /// operation when composing with that engine feature.
    ///
    /// ```
    /// # #[cfg(feature = "dim3")] {
    /// use ferravex_math::{Quat, Vec3};
    /// use ferravex_physics::d3::{
    ///     PhysicsWorld, RigidBodyPose, TeleportError, TeleportOptions, VelocityPolicy,
    /// };
    /// let mut world = PhysicsWorld::default();
    /// let ball = world.add_ball([0.; 3], 0.5, true);
    /// let old = RigidBodyPose { translation: Vec3::ZERO, rotation: Quat::IDENTITY };
    /// let mut samples = [old; 2];
    /// let checkpoint = RigidBodyPose {
    ///     translation: Vec3::new(4., 2., 0.), rotation: Quat::IDENTITY * 2.,
    /// };
    /// let committed = world.try_teleport_body(
    ///     ball, checkpoint, TeleportOptions::new(VelocityPolicy::Clear),
    /// ).unwrap();
    /// assert!(committed.rotation.is_normalized());
    /// samples.fill(committed); // Snap previous and current only on success.
    /// assert_eq!(samples[0], samples[1]);
    /// let invalid = RigidBodyPose { translation: Vec3::NAN, ..checkpoint };
    /// assert_eq!(world.try_teleport_body(
    ///     ball, invalid, TeleportOptions::new(VelocityPolicy::Preserve),
    /// ), Err(TeleportError::Translation));
    /// assert_eq!(world.bodies[ball].translation(), samples[1].translation);
    /// world.step(1. / 60.);
    /// assert_eq!(world.bodies[ball].translation(), committed.translation);
    /// # }
    /// ```
    pub fn try_teleport_body(
        &mut self,
        body: RigidBodyHandle,
        pose: RigidBodyPose,
        options: TeleportOptions,
    ) -> Result<RigidBodyPose, TeleportError> {
        self.raw
            .bodies
            .get_mut(body)
            .ok_or(TeleportError::InvalidHandle)?
            .try_teleport(pose, options)
    }
    pub fn add_ball(&mut self, p: [f32; DIM], radius: f32, dynamic: bool) -> RigidBodyHandle {
        let b = if dynamic {
            RigidBodyBuilder::dynamic()
        } else {
            RigidBodyBuilder::fixed()
        };
        self.raw
            .insert(
                b.translation(Vector::from_array(p)),
                ColliderBuilder::ball(radius),
            )
            .0
    }
    pub fn add_box(&mut self, p: [f32; DIM], half: [f32; DIM], dynamic: bool) -> RigidBodyHandle {
        let b = if dynamic {
            RigidBodyBuilder::dynamic()
        } else {
            RigidBodyBuilder::fixed()
        };
        let h = if PLANAR {
            Vec3::new(half[0], half[1], 0.5)
        } else {
            Vec3::new(half[0], half[1], half.get(2).copied().unwrap_or(0.5))
        };
        self.raw
            .insert(
                b.translation(Vector::from_array(p)),
                ColliderBuilder::shape(body::Shape::Box(h)),
            )
            .0
    }
    /// Queries use current body poses directly; no separate broadphase synchronization is needed.
    pub fn refresh_queries(&mut self) {}
    /// Sweep a ball through current collider poses, filtering sensors, membership
    /// and an optional ignored body (all its compound children). The hit point
    /// is the swept ball center at impact. Invalid inputs return None.
    /// See [`Self::spherecast_with_options`] for named filters and sensor policies.
    pub fn spherecast(
        &self,
        origin: [f32; DIM],
        direction: [f32; DIM],
        max_distance: f32,
        radius: f32,
        mask: u32,
        ignore: Option<RigidBodyHandle>,
    ) -> Option<RayHit> {
        self.raw
            .cast_sphere(
                to3(Vector::from_array(origin)),
                to3(Vector::from_array(direction)),
                max_distance,
                radius,
                mask,
                ignore,
            )
            .map(|h| RayHit {
                body: Some(h.0),
                collider: h.1,
                distance: h.2,
                point: from3(h.3).to_array(),
                normal: from3(h.4).to_array(),
            })
    }
    /// Cast against solid colliders in all layers using their current poses.
    /// See [`Self::raycast_with_options`] to select layers, sensors or an ignored body.
    pub fn raycast(
        &self,
        origin: [f32; DIM],
        direction: [f32; DIM],
        max_distance: f32,
    ) -> Option<RayHit> {
        self.raw
            .cast_ray(
                to3(Vector::from_array(origin)),
                to3(Vector::from_array(direction)),
                max_distance,
            )
            .map(|h| RayHit {
                body: Some(h.0),
                collider: h.1,
                distance: h.2,
                point: from3(h.3).to_array(),
                normal: from3(h.4).to_array(),
            })
    }

    /// Cast a ray with reusable distance and filtering options.
    ///
    /// Direction is normalized internally, so distances are world units. The
    /// nearest accepted collider is returned; an initial overlap hits at zero.
    /// Queries see body and child-collider edits immediately, without a physics
    /// step or [`Self::refresh_queries`]. Validation and tie behavior match
    /// [`Self::spherecast_with_options`] with a zero radius.
    /// See [`QueryOptions`] for a complete ray/sphere example.
    pub fn raycast_with_options(
        &self,
        origin: [f32; DIM],
        direction: [f32; DIM],
        options: QueryOptions,
    ) -> Option<RayHit> {
        self.spherecast_with_options(origin, direction, 0., options)
    }

    /// Sweep a ball (a circle in 2D) with named distance and filtering options.
    ///
    /// Uses current poses and allocates no query storage. The hit point is the
    /// swept center, the normal points out of the target, and distance is travel
    /// in world units. Initial overlap returns distance zero. Equal-distance
    /// hits select the later collider in the world's iteration order, matching
    /// the original casts. See [`QueryOptions`] for usage.
    ///
    /// Returns `None` for no accepted hit, nonfinite origin/direction/distance/
    /// radius, zero direction, negative distance, radius outside `0..=1000`, or
    /// a cast endpoint that overflows. A zero distance still tests initial
    /// overlap and requires a nonzero direction. Like the original casts, this
    /// uses the solver's finite-iteration convex sweep, so grazing hits may be
    /// missed when its iteration budget is exhausted.
    pub fn spherecast_with_options(
        &self,
        origin: [f32; DIM],
        direction: [f32; DIM],
        radius: f32,
        options: QueryOptions,
    ) -> Option<RayHit> {
        let query = PreparedQuery::new(origin, direction, radius, options).ok()?;
        let mut best = None;
        let mut limit = options.max_distance;
        for (id, collider) in self.raw.colliders.iter() {
            if let Some(hit) = query.cast(self, id, collider, limit) {
                limit = hit.distance;
                best = Some(hit);
            }
        }
        best
    }

    /// Collect ray hits into caller-owned storage without allocating.
    ///
    /// This is [`Self::spherecast_into`] with radius zero: the nearest hits fit
    /// into `output` in distance order, and the report counts every accepted
    /// collider hit, even when the buffer is empty or too small. On success,
    /// unused entries are `None`; on invalid input, the buffer is unchanged.
    ///
    /// ```
    /// # #[cfg(feature = "dim2")] {
    /// use ferravex_physics::d2::{PhysicsWorld, QueryOptions};
    /// let mut world = PhysicsWorld::default();
    /// world.add_ball([6., 0.], 0.5, false);
    /// let near = world.add_ball([2., 0.], 0.5, false);
    /// let mut hits = [None; 1];
    /// let report = world.raycast_into([0.; 2], [1., 0.], QueryOptions::new(10.),
    ///     &mut hits).unwrap();
    /// assert_eq!((report.written, report.total), (1, 2));
    /// assert!(report.is_truncated());
    /// assert_eq!(hits[0].unwrap().body, Some(near));
    /// # }
    /// ```
    pub fn raycast_into(
        &self,
        origin: [f32; DIM],
        direction: [f32; DIM],
        options: QueryOptions,
        output: &mut [Option<RayHit>],
    ) -> Result<QueryReport, QueryError> {
        self.spherecast_into(origin, direction, 0., options, output)
    }

    /// Collect sphere (circle in 2D) hits using current body and child poses.
    ///
    /// Uses the same validation, membership layers, ignored body, sensor policy,
    /// normalized direction, swept-center points and initial-overlap behavior
    /// as [`Self::spherecast_with_options`]. Each accepted collider contributes
    /// at most one hit, including separate children of a compound body.
    ///
    /// Success replaces the buffer with the nearest `min(output.len(), total)`
    /// hits in ascending distance order, followed by `None`. Equal distances
    /// place the later collider in world iteration order first, matching the
    /// nearest cast's tie preference. Every collider is tested over the full
    /// distance, so a full buffer never stops counting farther hits. An empty
    /// buffer is a count-only query. Errors preserve every output entry.
    ///
    /// Storage is bounded by the caller's slice; no query allocation occurs.
    /// Insertion into the sorted buffer takes up to `output.len()` moves per
    /// hit. For large worlds, use a small buffer when only nearby hits matter.
    /// The existing finite-iteration convex sweep can miss grazing hits; counts
    /// describe hits it finds, not a proof that every geometric hit was found.
    /// Full-distance casts can also round differently from the shrinking-limit
    /// nearest cast. This does not advance simulation or refresh cached poses.
    ///
    /// ```
    /// # #[cfg(feature = "dim3")] {
    /// use ferravex_physics::d3::{PhysicsWorld, QueryOptions};
    /// let mut world = PhysicsWorld::default();
    /// world.add_ball([3., 0., 0.], 0.5, false);
    /// let mut hits = [None; 4];
    /// let report = world.spherecast_into([0.; 3], [2., 0., 0.], 0.5,
    ///     QueryOptions::new(5.), &mut hits).unwrap();
    /// assert_eq!((report.written, report.total), (1, 1));
    /// assert!(!report.is_truncated());
    /// assert!((hits[0].unwrap().distance - 2.).abs() < 0.001);
    /// assert!(hits[1..].iter().all(Option::is_none));
    /// # }
    /// ```
    pub fn spherecast_into(
        &self,
        origin: [f32; DIM],
        direction: [f32; DIM],
        radius: f32,
        options: QueryOptions,
        output: &mut [Option<RayHit>],
    ) -> Result<QueryReport, QueryError> {
        let query = PreparedQuery::new(origin, direction, radius, options)?;
        output.fill(None);
        let mut report = QueryReport::default();
        for (id, collider) in self.raw.colliders.iter() {
            let Some(hit) = query.cast(self, id, collider, options.max_distance) else {
                continue;
            };
            report.total += 1;
            // Insert before existing ties, so later colliders win even when
            // the buffer is full. The prefix contains only initialized hits.
            let index = output[..report.written]
                .partition_point(|entry| entry.as_ref().unwrap().distance < hit.distance);
            if index < output.len() {
                let end = report.written.min(output.len() - 1);
                output.copy_within(index..end, index + 1);
                output[index] = Some(hit);
                report.written = (report.written + 1).min(output.len());
            }
        }
        Ok(report)
    }

    /// Return the point result with the smallest signed separation.
    ///
    /// This is [`Self::overlap_sphere_with_options`] with radius zero. See
    /// [`Self::overlap_point_into`] for a runnable example.
    pub fn overlap_point_with_options(
        &self,
        point: [f32; DIM],
        options: QueryOptions,
    ) -> Option<OverlapHit> {
        self.overlap_sphere_with_options(point, 0., options)
    }

    /// Return the sphere result with the smallest signed separation.
    ///
    /// The deepest penetration wins, then touching, then the closest separated
    /// collider. Equal separations prefer the later collider in world iteration
    /// order. Filtering and geometry match [`Self::overlap_sphere_into`]. Invalid
    /// input or no accepted collider returns `None`; use the buffered method to
    /// distinguish invalid input from a miss. No allocation or world mutation.
    pub fn overlap_sphere_with_options(
        &self,
        center: [f32; DIM],
        radius: f32,
        options: QueryOptions,
    ) -> Option<OverlapHit> {
        let mut output = [None];
        let _ = self
            .overlap_sphere_into(center, radius, options, &mut output)
            .ok()?;
        output[0]
    }

    /// Collect point overlaps and nearby collider surfaces without a direction.
    ///
    /// Equivalent to [`Self::overlap_sphere_into`] with radius zero, including
    /// its signed separation, ordering, filtering and failure-atomic buffer.
    /// `QueryOptions::new(0.)` includes touching and inside points; a positive
    /// maximum distance also includes separated surfaces within that distance.
    ///
    /// ```
    /// # #[cfg(feature = "dim2")] {
    /// use ferravex_physics::d2::{PhysicsWorld, QueryOptions};
    /// let mut world = PhysicsWorld::default();
    /// let wall = world.add_box([0.; 2], [1.; 2], false);
    /// let mut hits = [None; 2];
    /// let report = world.overlap_point_into([1.25, 0.], QueryOptions::new(0.5),
    ///     &mut hits).unwrap();
    /// assert_eq!((report.written, report.total), (1, 1));
    /// let hit = hits[0].unwrap();
    /// assert_eq!(hit.body, Some(wall));
    /// assert_eq!(hit.separation, 0.25);
    /// assert_eq!(hit.collider_point, [1., 0.]);
    /// assert_eq!(hit.probe_point, [1.25, 0.]);
    /// # }
    /// ```
    pub fn overlap_point_into(
        &self,
        point: [f32; DIM],
        options: QueryOptions,
        output: &mut [Option<OverlapHit>],
    ) -> Result<QueryReport, QueryError> {
        self.overlap_sphere_into(point, 0., options, output)
    }

    /// Collect direction-free sphere (circle in 2D) proximity results.
    ///
    /// `options.max_distance` is the maximum **signed surface separation**:
    /// zero includes overlaps and exact touches, positive values also include
    /// nearby separated colliders. No solver contact margin is added. A negative
    /// separation means penetration; its negation is the distance to translate
    /// the probe along `normal` to touch this individual convex child. A compound
    /// produces one result per child, not a combined depenetration solution.
    ///
    /// Uses current body/child poses, membership layers, sensor policy and the
    /// generational ignored-body filter shared with casts. No step, refresh,
    /// broadphase, allocation or simulation mutation occurs. All existing target
    /// shapes are supported by the shared signed-distance kernel; probes are
    /// points/balls only. Results that cannot be represented in `f32`
    /// (including overflowed witnesses) are omitted, as in existing casts.
    ///
    /// Results sort by ascending signed separation (deepest penetration first),
    /// with later colliders first on exact ties. The nearest `output.len()` are
    /// retained; every accepted child counts toward `total` even after the buffer
    /// fills. Empty output counts only. Success clears unused entries to `None`.
    /// Insertion costs up to `output.len()` moves per accepted collider.
    ///
    /// Validation checks finite center, finite nonnegative maximum separation,
    /// then finite radius in `0..=1000`, even with an empty world/buffer or zero
    /// layer mask. Errors use existing [`QueryError`] variants and preserve the
    /// entire buffer. Direction and cast-endpoint validation do not apply.
    ///
    /// A placement preview can inspect penetration before committing a body.
    /// This recipe resolves a single wall, then rechecks the candidate; several
    /// overlapping children require an application-specific placement policy.
    ///
    /// ```
    /// # #[cfg(feature = "dim3")] {
    /// use ferravex_physics::d3::{PhysicsWorld, QueryOptions};
    /// let mut world = PhysicsWorld::default();
    /// let wall = world.add_box([0.; 3], [1.; 3], false);
    /// let mut candidate = [1.25, 0., 0.];
    /// let mut hits = [None; 4];
    /// let report = world.overlap_sphere_into(candidate, 0.5, QueryOptions::new(0.),
    ///     &mut hits).unwrap();
    /// assert_eq!((report.written, report.total), (1, 1));
    /// let hit = hits[0].unwrap();
    /// assert_eq!(hit.body, Some(wall));
    /// assert_eq!(hit.penetration_depth(), 0.25);
    /// for (coordinate, normal) in candidate.iter_mut().zip(hit.normal) {
    ///     *coordinate += normal * hit.penetration_depth();
    /// }
    /// let touching = world.overlap_sphere_with_options(candidate, 0.5,
    ///     QueryOptions::new(0.)).unwrap();
    /// assert_eq!(touching.separation, 0.);
    /// assert_eq!(touching.collider_point, touching.probe_point);
    /// // Queries did not move the authored wall or create the preview body.
    /// assert_eq!(world.bodies.len(), 1);
    /// # }
    /// ```
    pub fn overlap_sphere_into(
        &self,
        center: [f32; DIM],
        radius: f32,
        options: QueryOptions,
        output: &mut [Option<OverlapHit>],
    ) -> Result<QueryReport, QueryError> {
        let query = PreparedOverlap::new(center, radius, options)?;
        output.fill(None);
        let mut report = QueryReport::default();
        for (id, collider) in self.raw.colliders.iter() {
            let Some(hit) = query.hit(self, id, collider) else {
                continue;
            };
            report.total += 1;
            let index = output[..report.written]
                .partition_point(|entry| entry.as_ref().unwrap().separation < hit.separation);
            if index < output.len() {
                let end = report.written.min(output.len() - 1);
                output.copy_within(index..end, index + 1);
                output[index] = Some(hit);
                report.written = (report.written + 1).min(output.len());
            }
        }
        Ok(report)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct RayHit {
    pub body: Option<RigidBodyHandle>,
    pub collider: ColliderHandle,
    pub distance: f32,
    pub point: [f32; DIM],
    pub normal: [f32; DIM],
}

/// Signed point/ball proximity to one convex collider child.
///
/// See [`PhysicsWorld::overlap_sphere_into`] for a runnable placement recipe.
/// Witnesses satisfy `probe_point - collider_point = normal * separation`,
/// within floating-point rounding, including when the center is inside the
/// collider. For penetration these are opposing support witnesses, rather than
/// closest points of the intersecting volumes. At centers/edges where the
/// nearest exit is not unique the kernel chooses a deterministic outward normal;
/// it is a valid exit direction, not a uniquely defined surface gradient.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlapHit {
    /// Parent body, matching `RayHit`; attached children always return `Some`.
    pub body: Option<RigidBodyHandle>,
    /// Exact child identity; compound siblings count separately.
    pub collider: ColliderHandle,
    /// Positive gap, zero touch, or negative penetration, in world units.
    pub separation: f32,
    /// Boundary witness on the target collider in world coordinates.
    pub collider_point: [f32; DIM],
    /// Boundary witness on the probe (its center for point queries).
    pub probe_point: [f32; DIM],
    /// Unit outward target normal, pointing along the probe's exit direction.
    pub normal: [f32; DIM],
}
impl OverlapHit {
    /// Distance to translate the probe along `normal` to touch this child.
    /// Zero for touching or separated results. See the placement recipe on
    /// [`PhysicsWorld::overlap_sphere_into`].
    pub fn penetration_depth(self) -> f32 {
        (-self.separation).max(0.)
    }
}

impl crate::character::Terrain for PhysicsWorld {
    fn visit(
        &self,
        bounds: crate::character::QueryBounds,
        visitor: &mut dyn FnMut(crate::character::Obstacle),
    ) {
        crate::character::Terrain::visit(&self.raw, bounds, visitor);
    }
    fn obstacle(&self, id: u64) -> Option<crate::character::Obstacle> {
        crate::character::Terrain::obstacle(&self.raw, id)
    }
}

/// Select which rest fields world-space authoring captures.
/// See [`WorldJointFrame`] and [`PhysicsWorld::joint_desc_from_world`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointPoseMode {
    /// Capture current relative rotation and distance/spring rest length.
    /// Fixed/hinge world pivots must coincide. Controls remain authored.
    Preserve,
    /// Retain explicit reference rotation and rest length. A missing reference
    /// captures current relative rotation, matching ordinary insertion defaults.
    Authored,
}

/// World-space pivots and hinge axis, sampled against two live body poses.
///
/// `anchors[0]` belongs to body A, `anchors[1]` to body B. `axis` is a world
/// direction (`Vec3` in either dimension); hinges normalize/check it at use.
/// Other joint kinds ignore it. No local frame rotation is stored: the existing
/// solver uses local axes and a body-relative reference quaternion instead.
///
/// This 2D recipe attaches a rotated, translated rotor without initial snap.
/// It uses the existing insertion path, then executes an ordinary fixed tick.
///
/// ```
/// # #[cfg(feature = "dim2")] {
/// use ferravex_math::Vec2;
/// use ferravex_physics::d2::{
///     ColliderBuilder, JointBuilder, JointPoseMode, PhysicsWorld,
///     RigidBodyBuilder, WorldJointFrame,
/// };
/// let mut world = PhysicsWorld::default();
/// let support = world.insert_body(RigidBodyBuilder::fixed()
///     .translation(Vec2::new(2., 3.)).rotation(0.4)).unwrap();
/// let (rotor, _) = world.insert(RigidBodyBuilder::dynamic()
///     .translation(Vec2::new(4., 3.)).rotation(-0.7), ColliderBuilder::ball(0.2));
/// let joint = world.insert_joint_from_world(support, rotor,
///     JointBuilder::hinge().limits(-0.5, 0.5),
///     WorldJointFrame::at_pivot(Vec2::new(3., 3.)), JointPoseMode::Preserve).unwrap();
/// assert!(world.joint_angle(joint).unwrap().abs() < 0.00001);
/// world.step(1. / 120.);
/// assert!((world.bodies[rotor].translation() - Vec2::new(4., 3.)).length() < 0.0001);
/// # }
/// ```
///
/// In 3D, a visible world axis becomes two different local hinge axes. A
/// distance/spring can instead use `at_anchors` with two separated pivots.
/// `Authored` retains an intentional rest length; `Preserve` captures separation.
///
/// ```
/// # #[cfg(feature = "dim3")] {
/// use ferravex_math::Vec3;
/// use ferravex_physics::{JointKind, d3::{
///     ColliderBuilder, JointBuilder, JointPoseMode, PhysicsWorld,
///     RigidBodyBuilder, WorldJointFrame,
/// }};
/// let mut world = PhysicsWorld::default();
/// let support = world.insert_body(RigidBodyBuilder::fixed()
///     .translation(Vec3::new(2., 3., 1.)).rotation(Vec3::X * 0.6)).unwrap();
/// let (rotor, _) = world.insert(RigidBodyBuilder::dynamic()
///     .translation(Vec3::new(4., 3., 1.)).rotation(Vec3::Z * -0.7),
///     ColliderBuilder::ball(0.2));
/// let pivot = WorldJointFrame::at_pivot(Vec3::new(3., 3., 1.)).with_axis(Vec3::Y * 4.);
/// let desc = world.joint_desc_from_world(support, rotor, JointBuilder::hinge(),
///     pivot, JointPoseMode::Preserve).unwrap();
/// let joint = world.insert_joint(support, rotor, desc).unwrap();
/// world.step(1. / 120.);
/// assert!((world.bodies[rotor].translation() - Vec3::new(4., 3., 1.)).length() < 0.0001);
/// assert!(world.remove_joint(joint));
/// let anchors = WorldJointFrame::at_anchors([Vec3::new(2., 3., 1.), Vec3::new(4., 3., 1.)]);
/// let spring = world.joint_desc_from_world(support, rotor, JointBuilder::spring(0., 20., 2.),
///     anchors, JointPoseMode::Preserve).unwrap();
/// assert_eq!(spring.kind, JointKind::Spring { length: 2., stiffness: 20., damping: 2. });
/// world.insert_joint(support, rotor, spring).unwrap();
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldJointFrame {
    pub anchors: [Vector; 2],
    pub axis: Vec3,
}
impl WorldJointFrame {
    /// A shared pivot with the usual +Z (2D) or +Y (3D) world hinge axis.
    /// See this type's examples; this only stores inputs, checked at use.
    pub fn at_pivot(pivot: Vector) -> Self {
        Self::at_anchors([pivot; 2])
    }
    /// Two world pivots, normally used by distance/spring joints.
    /// See this type's examples; validation occurs when sampled against bodies.
    pub fn at_anchors(anchors: [Vector; 2]) -> Self {
        Self {
            anchors,
            axis: if PLANAR { Vec3::Z } else { Vec3::Y },
        }
    }
    /// Set an unnormalized world hinge axis; non-hinges ignore it.
    /// Zero/nonfinite axes fail at use. 2D requires normalized +Z.
    /// See this type's 3D example.
    pub fn with_axis(mut self, axis: Vec3) -> Self {
        self.axis = axis;
        self
    }
}

/// Connections use local anchors relative to each body's authored origin.
#[derive(Clone, Copy, Debug)]
pub struct JointBuilder {
    desc: crate::JointDesc,
}
impl JointBuilder {
    pub fn fixed() -> Self {
        Self {
            desc: crate::JointDesc::new(crate::JointKind::Fixed),
        }
    }
    pub fn hinge() -> Self {
        Self {
            desc: crate::JointDesc::new(crate::JointKind::Hinge {
                axis1: if PLANAR { Vec3::Z } else { Vec3::Y },
                axis2: if PLANAR { Vec3::Z } else { Vec3::Y },
            }),
        }
    }
    pub fn distance(length: f32) -> Self {
        Self {
            desc: crate::JointDesc::new(crate::JointKind::Distance { length }),
        }
    }
    pub fn spring(length: f32, stiffness: f32, damping: f32) -> Self {
        Self {
            desc: crate::JointDesc::new(crate::JointKind::Spring {
                length,
                stiffness,
                damping,
            }),
        }
    }
    pub fn local_anchor1(mut self, anchor: Vector) -> Self {
        self.desc.local_anchor1 = to3(anchor);
        self
    }
    pub fn local_anchor2(mut self, anchor: Vector) -> Self {
        self.desc.local_anchor2 = to3(anchor);
        self
    }
    pub fn reference_rotation(mut self, rotation: Rotation) -> Self {
        self.desc.reference_rotation = Some(rotation_to3(rotation));
        self
    }
    pub fn limits(mut self, lower: f32, upper: f32) -> Self {
        self.desc.limits = Some([lower, upper]);
        self
    }
    pub fn motor(mut self, motor: crate::JointMotor) -> Self {
        self.desc.motor = Some(motor);
        self
    }
    pub fn break_limits(mut self, force: f32, torque: f32) -> Self {
        self.desc.break_limits = Some(crate::BreakLimits { force, torque });
        self
    }
    pub fn contacts_enabled(mut self, enabled: bool) -> Self {
        self.desc.contacts_enabled = enabled;
        self
    }
    pub fn build(self) -> crate::JointDesc {
        self.desc
    }
}
impl From<JointBuilder> for crate::JointDesc {
    fn from(b: JointBuilder) -> Self {
        b.desc
    }
}

#[cfg(test)]
// api.rs is included before the dimension-specific impls in lib.rs.
#[allow(clippy::items_after_test_module)]
mod journey200_032 {
    use super::*;

    fn point(x: f32, y: f32) -> [f32; DIM] {
        let mut point = [0.; DIM];
        point[0] = x;
        point[1] = y;
        point
    }

    fn assert_hit(actual: Option<RayHit>, expected: Option<RayHit>) {
        let fields = |hit: RayHit| (hit.body, hit.collider, hit.distance, hit.point, hit.normal);
        assert_eq!(actual.map(fields), expected.map(fields));
    }

    #[test]
    fn buffered_queries_keep_nearest_hits_and_count_all_colliders_at_every_capacity() {
        let mut world = PhysicsWorld::default();
        let far = world.add_ball(point(8., 0.), 0.5, false);
        let early = world.add_ball(point(2., 0.), 0.5, false);
        let middle = world.add_ball(point(5., 0.), 0.5, false);
        let late = world.add_ball(point(2., 0.), 0.5, false);
        let options = QueryOptions::new(10.);
        for radius in [0., 0.25] {
            let mut all = [None; 6];
            let report = world
                .spherecast_into(point(0., 0.), point(1., 0.), radius, options, &mut all)
                .unwrap();
            assert_eq!(
                report,
                QueryReport {
                    written: 4,
                    total: 4
                }
            );
            assert!(!report.is_truncated());
            for (hit, body) in all.iter().zip([late, early, middle, far]) {
                assert_eq!(hit.unwrap().body, Some(body));
            }
            assert!(all[4..].iter().all(Option::is_none));
            assert_hit(
                all[0],
                world.spherecast_with_options(point(0., 0.), point(1., 0.), radius, options),
            );
            for capacity in 0..=6 {
                let mut buffer = [all[0]; 6];
                let report = world
                    .spherecast_into(
                        point(0., 0.),
                        point(1., 0.),
                        radius,
                        options,
                        &mut buffer[..capacity],
                    )
                    .unwrap();
                assert_eq!(report.written, capacity.min(4));
                assert_eq!(report.total, 4);
                assert_eq!(report.is_truncated(), capacity < 4);
                for index in 0..capacity {
                    assert_hit(buffer[index], all[index]);
                }
                for untouched in &buffer[capacity..] {
                    assert_hit(*untouched, all[0]);
                }
            }
            let report = world
                .spherecast_into(point(0., 0.), point(-1., 0.), radius, options, &mut all)
                .unwrap();
            assert_eq!(report, QueryReport::default());
            assert!(all.iter().all(Option::is_none));
        }
    }

    #[test]
    fn buffered_filters_compose_and_compound_children_count_separately() {
        let mut world = PhysicsWorld::default();
        let ignored = world
            .insert_compound(
                RigidBodyBuilder::fixed(),
                [4., 5.].map(|x| {
                    ColliderBuilder::ball(0.25)
                        .translation(Vector::from_array(point(x, 0.)))
                        .collision_layers(0b11, 0)
                }),
            )
            .unwrap();
        let (sensor, sensor_collider) = world.insert(
            RigidBodyBuilder::fixed().translation(Vector::from_array(point(1., 0.))),
            ColliderBuilder::ball(0.25)
                .sensor(true)
                .collision_layers(0b10, 0),
        );
        let (solid, solid_collider) = world.insert(
            RigidBodyBuilder::fixed().translation(Vector::from_array(point(2., 0.))),
            ColliderBuilder::ball(0.25).collision_layers(0b01, 0),
        );
        world.insert(
            RigidBodyBuilder::fixed().translation(Vector::from_array(point(3., 0.))),
            ColliderBuilder::ball(0.25).collision_layers(0b100, 0),
        );
        let options = QueryOptions::new(10.).layers(0b11).ignore_body(ignored);
        let mut hits = [None; 8];
        for radius in [0., 0.1] {
            for (policy, expected) in [
                (SensorPolicy::Exclude, vec![solid]),
                (SensorPolicy::Include, vec![sensor, solid]),
                (SensorPolicy::Only, vec![sensor]),
            ] {
                let report = world
                    .spherecast_into(
                        point(0., 0.),
                        point(1., 0.),
                        radius,
                        options.sensors(policy),
                        &mut hits,
                    )
                    .unwrap();
                assert_eq!(report.total, expected.len());
                assert_eq!(report.written, expected.len());
                assert_eq!(
                    hits.iter()
                        .flatten()
                        .map(|hit| hit.body.unwrap())
                        .collect::<Vec<_>>(),
                    expected
                );
            }
            let report = world
                .spherecast_into(
                    point(0., 0.),
                    point(1., 0.),
                    radius,
                    QueryOptions {
                        ignored_body: None,
                        ..options
                    },
                    &mut hits,
                )
                .unwrap();
            assert_eq!(report.total, 3);
            assert_eq!(hits[1].unwrap().body, Some(ignored));
            assert_eq!(hits[2].unwrap().body, Some(ignored));
            assert_ne!(hits[1].unwrap().collider, hits[2].unwrap().collider);
        }
        world.colliders[sensor_collider].set_sensor(false);
        world.colliders[solid_collider].set_collision_layers(0b100, 0);
        let report = world
            .raycast_into(point(0., 0.), point(1., 0.), options, &mut hits)
            .unwrap();
        assert_eq!(report.total, 1);
        assert_eq!(hits[0].unwrap().body, Some(sensor));
        for options in [options.sensors(SensorPolicy::Only), options.layers(0)] {
            let report = world
                .raycast_into(point(0., 0.), point(1., 0.), options, &mut hits)
                .unwrap();
            assert_eq!(report, QueryReport::default());
            assert!(hits.iter().all(Option::is_none));
        }
    }

    #[test]
    fn buffered_queries_read_current_body_and_child_poses_without_stepping() {
        let mut world = PhysicsWorld::default();
        let (body, child) = world.insert(
            RigidBodyBuilder::fixed().translation(Vector::from_array(point(4., 0.))),
            ColliderBuilder::ball(0.5).translation(Vector::from_array(point(0., 2.))),
        );
        let options = QueryOptions::new(20.);
        let mut hits = [None; 2];
        assert_eq!(
            world
                .raycast_into(point(0., 0.), point(1., 0.), options, &mut hits)
                .unwrap()
                .total,
            0
        );
        let rotated = RigidBodyBuilder::fixed()
            .rotation(angular_from3(Vec3::Z * -std::f32::consts::FRAC_PI_2))
            .build();
        world.bodies[body].set_rotation(*rotated.rotation(), false);
        let report = world
            .raycast_into(point(0., 0.), point(1., 0.), options, &mut hits)
            .unwrap();
        assert_eq!(report.total, 1);
        assert!((hits[0].unwrap().distance - 5.5).abs() < 0.001);
        world.bodies[body].set_translation(Vector::from_array(point(8., 0.)), false);
        world
            .set_collider_pose(child, Vec3::new(0., 3., 0.), Quat::IDENTITY)
            .unwrap();
        let report = world
            .spherecast_into(point(0., 0.), point(1., 0.), 0.25, options, &mut hits)
            .unwrap();
        assert_eq!(report.total, 1);
        assert_eq!(hits[0].unwrap().collider, child);
        assert!((hits[0].unwrap().distance - 10.25).abs() < 0.001);

        let mut half = [0.1; DIM];
        half[0] = 2.;
        let box_body = world.add_box(point(4., 0.), half, false);
        let box_child = world.bodies[box_body].colliders()[0];
        world
            .set_collider_pose(
                box_child,
                Vec3::ZERO,
                Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            )
            .unwrap();
        let report = world
            .raycast_into(point(0., 0.), point(1., 0.), options, &mut hits)
            .unwrap();
        assert_eq!(report.total, 2);
        assert_eq!(hits[0].unwrap().collider, box_child);
        assert!((hits[0].unwrap().distance - 3.9).abs() < 0.001);
        assert!(world.contact_events().is_empty());
    }

    #[test]
    fn buffered_queries_handle_removed_and_reused_handles() {
        let mut world = PhysicsWorld::default();
        let removed = world.add_ball(point(1., 0.), 0.25, false);
        let other = world.add_ball(point(2., 0.), 0.25, false);
        let options = QueryOptions::new(5.).ignore_body(removed);
        let mut hits = [None; 3];
        assert!(world.remove_body(removed));
        let replacement = world.add_ball(point(2., 0.), 0.25, false);
        assert_ne!(replacement, removed);
        let report = world
            .raycast_into(point(0., 0.), point(1., 0.), options, &mut hits)
            .unwrap();
        assert_eq!(report.total, 2);
        // Reuse filled an earlier slot: ties follow iteration, not creation time.
        assert_eq!(hits[0].unwrap().body, Some(other));
        assert_eq!(hits[1].unwrap().body, Some(replacement));
        let child = world.bodies[other].colliders()[0];
        world.remove_collider(child).unwrap();
        let report = world
            .raycast_into(point(0., 0.), point(1., 0.), options, &mut hits)
            .unwrap();
        assert_eq!(report.total, 1);
        assert_eq!(hits[0].unwrap().body, Some(replacement));
        assert!(hits[1..].iter().all(Option::is_none));
    }

    #[test]
    fn buffered_queries_preserve_overlap_distance_and_normalization_rules() {
        let mut world = PhysicsWorld::default();
        world.add_ball(point(3., 0.), 0.5, false);
        let options = QueryOptions::new(2.5);
        for speed in [f32::from_bits(1), 0.01, 1., 30., f32::MAX] {
            let mut ray = [None; 2];
            let report = world
                .raycast_into(point(0., 0.), point(speed, 0.), options, &mut ray)
                .unwrap();
            assert_eq!(report.total, 1);
            let hit = ray[0].unwrap();
            assert_eq!(hit.distance, 2.5);
            assert_eq!(hit.point, point(2.5, 0.));
            assert_eq!(hit.normal, point(-1., 0.));
            let mut sphere = [None; 2];
            assert_eq!(
                world
                    .spherecast_into(point(0., 0.), point(speed, 0.), 0., options, &mut sphere)
                    .unwrap(),
                report
            );
            assert_hit(ray[0], sphere[0]);
            assert_eq!(
                world
                    .raycast_into(
                        point(0., 0.),
                        point(speed, 0.),
                        QueryOptions::new(2.4),
                        &mut ray
                    )
                    .unwrap()
                    .total,
                0
            );
        }
        for radius in [0., 0.25, 1000.] {
            let mut hits = [None; 1];
            let report = world
                .spherecast_into(
                    point(3., 0.),
                    point(1., 0.),
                    radius,
                    QueryOptions::new(0.),
                    &mut hits,
                )
                .unwrap();
            assert_eq!(report.total, 1);
            let hit = hits[0].unwrap();
            assert_eq!(hit.distance, 0.);
            assert_eq!(hit.point, point(3., 0.));
            assert!(hit.normal.iter().all(|value| value.is_finite()));
        }
        let mut off_axis = PhysicsWorld::default();
        off_axis.add_ball(point(3., 1.), 0.5, false);
        let mut hits = [None; 1];
        assert_eq!(
            off_axis
                .raycast_into(point(0., 0.), point(1., 0.), options, &mut hits)
                .unwrap()
                .total,
            0
        );
        assert_eq!(
            off_axis
                .spherecast_into(point(0., 0.), point(1., 0.), 0.75, options, &mut hits)
                .unwrap()
                .total,
            1
        );
        assert!((hits[0].unwrap().distance - 2.25).abs() < 0.001);
    }

    #[test]
    fn buffered_query_errors_are_field_specific_and_preserve_the_buffer() {
        let mut world = PhysicsWorld::default();
        let body = world.add_ball(point(3., 0.), 0.5, false);
        let origin = point(0., 0.);
        let direction = point(1., 0.);
        let options = QueryOptions::new(10.);
        let previous = world.raycast_with_options(origin, direction, options);
        let mut buffer = [previous, None, previous];
        let before = buffer;
        let mut check = |origin, direction, radius, options, expected| {
            assert_eq!(
                world.spherecast_into(origin, direction, radius, options, &mut buffer),
                Err(expected)
            );
            for (actual, expected) in buffer.into_iter().zip(before) {
                assert_hit(actual, expected);
            }
        };
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for axis in 0..DIM {
                let mut bad_origin = origin;
                bad_origin[axis] = bad;
                check(
                    bad_origin,
                    direction,
                    0.25,
                    options,
                    QueryError::NonFiniteOrigin,
                );
                let mut bad_direction = direction;
                bad_direction[axis] = bad;
                check(
                    origin,
                    bad_direction,
                    0.25,
                    options,
                    QueryError::NonFiniteDirection,
                );
            }
        }
        for bad in [-1., f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            check(
                origin,
                direction,
                0.25,
                QueryOptions::new(bad),
                QueryError::InvalidMaxDistance,
            );
        }
        for bad in [-1., 1000.1, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            check(origin, direction, bad, options, QueryError::InvalidRadius);
        }
        check(origin, [0.; DIM], 0.25, options, QueryError::ZeroDirection);
        check(
            point(f32::MAX, 0.),
            direction,
            0.25,
            QueryOptions::new(f32::MAX),
            QueryError::NonFiniteEndpoint,
        );
        let empty = PhysicsWorld::default();
        assert_eq!(
            empty.raycast_into(origin, [0.; DIM], options.layers(0), &mut []),
            Err(QueryError::ZeroDirection)
        );
        assert_hit(
            world.raycast_with_options(origin, direction, options),
            previous,
        );
        assert_eq!(
            world.bodies[body].translation(),
            Vector::from_array(point(3., 0.))
        );
        assert!(world.contact_events().is_empty());
    }
}
