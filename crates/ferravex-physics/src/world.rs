/// Rapier's complete world, available without any Ferravex API restrictions.
pub type RawPhysicsWorld = rapier::pipeline::PhysicsWorld;

/// A Rapier world with array-based constructors and an explicit timestep.
///
/// Body and collider defaults are Rapier's defaults: dynamic objects rotate,
/// fixed objects do not move, and materials have zero restitution and 0.5
/// friction. Continuous collision detection is opt-in per body. Use `insert`
/// with `RigidBodyBuilder` and `ColliderBuilder` for complete control.
#[derive(Default)]
pub struct PhysicsWorld {
    /// Full simulation state. `Deref` also exposes these fields directly.
    pub raw: RawPhysicsWorld,
}

/// The nearest hit of a normalized world-space ray.
#[derive(Debug, Clone, Copy)]
pub struct RayHit {
    /// Collider that the ray hit.
    pub collider: ColliderHandle,
    /// Body owning the collider, if the collider is attached to a body.
    pub body: Option<RigidBodyHandle>,
    /// Distance from the origin in world units.
    pub distance: f32,
    /// World-space point of impact.
    pub point: [f32; DIM],
    /// World-space surface normal.
    pub normal: [f32; DIM],
}

impl PhysicsWorld {
    /// Construct a world with gravity in world units per second squared.
    ///
    /// # Panics
    /// Panics if any gravity component is non-finite.
    pub fn new(gravity: [f32; DIM]) -> Self {
        assert_finite(gravity, "gravity");
        Self {
            raw: RawPhysicsWorld {
                gravity: Vector::from_array(gravity),
                ..RawPhysicsWorld::default()
            },
        }
    }

    /// Advance one physics tick. Call with a fixed, positive timestep.
    ///
    /// # Panics
    /// Panics if `dt` is zero, negative, or non-finite.
    pub fn step(&mut self, dt: f32) {
        self.step_with_events(dt, &(), &());
    }

    /// Advance one tick with custom Rapier collision hooks and event handlers.
    ///
    /// Enable the appropriate `ActiveEvents` on colliders to receive events.
    ///
    /// # Panics
    /// Panics if `dt` is zero, negative, or non-finite.
    pub fn step_with_events(
        &mut self,
        dt: f32,
        hooks: &dyn PhysicsHooks,
        events: &dyn EventHandler,
    ) {
        assert!(
            dt.is_finite() && dt > 0.0,
            "physics dt must be finite and positive"
        );
        self.raw.integration_parameters.dt = dt;
        self.raw.step_with_events(hooks, events);
    }

    /// Add an axis-aligned box; all extents are **half** its full dimensions.
    ///
    /// Set `dynamic` to false for static terrain and true for movable objects.
    /// Use the returned handle to set rotation, velocities, damping, or CCD.
    ///
    /// # Panics
    /// Panics for non-finite positions or non-positive/non-finite extents.
    pub fn add_box(
        &mut self,
        position: [f32; DIM],
        half_extents: [f32; DIM],
        dynamic: bool,
    ) -> RigidBodyHandle {
        assert_finite(position, "position");
        assert!(
            half_extents.into_iter().all(|n| n.is_finite() && n > 0.0),
            "box half extents must be finite and positive"
        );
        self.raw
            .insert(body_builder(position, dynamic), box_collider(half_extents))
            .0
    }

    /// Add a circle in 2D or sphere in 3D, returning its body handle.
    ///
    /// # Panics
    /// Panics for a non-finite position or non-positive/non-finite radius.
    pub fn add_ball(
        &mut self,
        position: [f32; DIM],
        radius: f32,
        dynamic: bool,
    ) -> RigidBodyHandle {
        assert_finite(position, "position");
        assert!(
            radius.is_finite() && radius > 0.0,
            "ball radius must be finite and positive"
        );
        self.raw
            .insert(
                body_builder(position, dynamic),
                ColliderBuilder::ball(radius),
            )
            .0
    }

    /// Refresh contacts and scene queries after adding, moving, or removing
    /// bodies outside `step`, without advancing simulation time.
    ///
    /// Call this before a query if the world has changed since its last step.
    pub fn refresh_queries(&mut self) {
        self.raw.detect_collisions(&(), &());
    }

    /// Cast a ray, normalizing `direction` so distances are in world units.
    ///
    /// Hits inside a solid shape have distance zero. The acceleration structure
    /// represents the last `step` or `refresh_queries`, so refresh after edits.
    /// Returns `None` for invalid inputs, a zero direction, or no collision.
    pub fn raycast(
        &self,
        origin: [f32; DIM],
        direction: [f32; DIM],
        max_distance: f32,
    ) -> Option<RayHit> {
        self.raycast_with_filter(origin, direction, max_distance, QueryFilter::default())
    }

    /// Raycast with collision groups, body exclusions, or a custom predicate.
    ///
    /// For example, exclude the shooting player's body with
    /// `QueryFilter::default().exclude_rigid_body(player)`.
    pub fn raycast_with_filter(
        &self,
        origin: [f32; DIM],
        direction: [f32; DIM],
        max_distance: f32,
        filter: QueryFilter<'_>,
    ) -> Option<RayHit> {
        if !origin.into_iter().all(f32::is_finite)
            || !max_distance.is_finite()
            || max_distance < 0.0
        {
            return None;
        }
        let direction = Vector::from_array(direction).try_normalize()?;
        let ray = Ray::new(Vector::from_array(origin), direction);
        let (collider, hit) = self
            .raw
            .cast_ray_and_get_normal(&ray, max_distance, true, filter)?;
        Some(RayHit {
            collider,
            body: self.raw.colliders[collider].parent(),
            distance: hit.time_of_impact,
            point: (ray.origin + ray.dir * hit.time_of_impact).to_array(),
            normal: hit.normal.to_array(),
        })
    }
}

impl core::ops::Deref for PhysicsWorld {
    type Target = RawPhysicsWorld;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl core::ops::DerefMut for PhysicsWorld {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.raw
    }
}

fn assert_finite(values: [f32; DIM], name: &str) {
    assert!(
        values.into_iter().all(f32::is_finite),
        "{name} must be finite"
    );
}

fn body_builder(position: [f32; DIM], dynamic: bool) -> RigidBodyBuilder {
    let builder = if dynamic {
        RigidBodyBuilder::dynamic()
    } else {
        RigidBodyBuilder::fixed()
    };
    builder.translation(Vector::from_array(position))
}
