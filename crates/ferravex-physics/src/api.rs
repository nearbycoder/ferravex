pub use crate::{ColliderHandle, RigidBodyHandle};
/// Rigid body state. The boolean wake parameters are retained for game source compatibility.
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
    pub fn set_translation(&mut self, p: Vector, wake: bool) {
        assert!(p.is_finite());
        self.core.position = to3(p);
        if wake {
            self.core.wake();
        }
    }
    pub fn set_rotation(&mut self, q: Rotation, wake: bool) {
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
        let torque = (p - self.core.position).cross(j);
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
    pub fn ball(radius: f32) -> Self {
        Self::shape(body::Shape::Ball(radius))
    }
    pub fn density(mut self, v: f32) -> Self {
        assert!(v.is_finite() && v > 0.);
        self.collider.density = v;
        self
    }
    pub fn friction(mut self, v: f32) -> Self {
        assert!(v.is_finite() && v >= 0.);
        self.collider.friction = v;
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
}
#[derive(Clone, Copy, Debug)]
pub struct RayHit {
    pub body: Option<RigidBodyHandle>,
    pub collider: ColliderHandle,
    pub distance: f32,
    pub point: [f32; DIM],
    pub normal: [f32; DIM],
}
