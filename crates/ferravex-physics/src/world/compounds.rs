use super::*;
use crate::{
    PhysicsError,
    body::ColliderShapeError,
    mass::{self, Mass},
};
pub const MAX_CHILDREN: usize = 32;
pub const MAX_COLLIDERS: usize = 16384;
impl<B: Access> World<B> {
    // Read-only assembly preflight. Keep legacy insert_compound's acceptance
    // unchanged; assemblies additionally check geometry on non-dynamic bodies.
    pub(crate) fn check_assembly_compound(
        &self,
        b: &B,
        colliders: &[Collider],
    ) -> Result<(), PhysicsError> {
        let core = b.core();
        if core.planar != self.planar || !core.position.is_finite() || !core.rotation.is_finite() {
            return Err(PhysicsError::InvalidBody);
        }
        if colliders.len() > MAX_CHILDREN {
            return Err(PhysicsError::ChildCapacity);
        }
        if colliders
            .iter()
            .any(|c| !c.valid(self.planar) || c.shape.check_dimensions(self.planar).is_err())
        {
            return Err(PhysicsError::InvalidCollider);
        }
        if core.kind == Kind::Dynamic {
            mass::compound(colliders.iter(), self.planar)?;
        }
        Ok(())
    }

    pub fn insert_compound<C: Into<Collider>>(
        &mut self,
        b: impl Into<B>,
        children: impl IntoIterator<Item = C>,
    ) -> Result<RigidBodyHandle, PhysicsError> {
        if self.bodies.len() >= 4096 {
            return Err(PhysicsError::BodyCapacity);
        }
        let mut colliders = Vec::new();
        for c in children {
            if colliders.len() >= MAX_CHILDREN {
                return Err(PhysicsError::ChildCapacity);
            }
            colliders.push(c.into());
        }
        if self.colliders.len() + colliders.len() > MAX_COLLIDERS {
            return Err(PhysicsError::ColliderCapacity);
        }
        let mut b = b.into();
        let core = b.core_mut();
        if core.planar != self.planar || !core.position.is_finite() || !core.rotation.is_finite() {
            return Err(PhysicsError::InvalidBody);
        }
        if colliders.iter().any(|c| !c.valid(self.planar)) {
            return Err(PhysicsError::InvalidCollider);
        }
        let properties = if core.kind == Kind::Dynamic {
            Some(mass::compound(colliders.iter(), self.planar)?)
        } else {
            None
        };
        core.colliders.clear();
        if let Some(properties) = properties {
            let velocity = core.velocity;
            core.set_mass_properties(properties);
            core.velocity = velocity;
        }
        core.wake();
        let body = self.bodies.insert(b);
        for mut c in colliders {
            c.parent = Some(body);
            c.mass_dirty = false;
            let collider = self.colliders.insert(c);
            self.bodies[body].core_mut().colliders.push(collider);
        }
        Ok(body)
    }
    /// Fixed/kinematic anchors may have no collision geometry. Dynamic bodies require mass.
    pub fn insert_body(&mut self, b: impl Into<B>) -> Result<RigidBodyHandle, PhysicsError> {
        self.insert_compound(b, std::iter::empty::<Collider>())
    }
    pub fn attach_collider(
        &mut self,
        body: RigidBodyHandle,
        c: impl Into<Collider>,
    ) -> Result<ColliderHandle, PhysicsError> {
        if self.colliders.len() >= MAX_COLLIDERS {
            return Err(PhysicsError::ColliderCapacity);
        }
        let mut c = c.into();
        let b = self
            .bodies
            .get(body)
            .ok_or(PhysicsError::InvalidHandle)?
            .core();
        if b.colliders.len() >= MAX_CHILDREN {
            return Err(PhysicsError::ChildCapacity);
        }
        if !c.valid(self.planar) {
            return Err(PhysicsError::InvalidCollider);
        }
        let props = if b.kind == Kind::Dynamic {
            Some(mass::compound(
                b.colliders
                    .iter()
                    .map(|id| &self.colliders[*id])
                    .chain(std::iter::once(&c)),
                self.planar,
            )?)
        } else {
            None
        };
        c.parent = Some(body);
        c.mass_dirty = false;
        let id = self.colliders.insert(c);
        self.bodies[body].core_mut().colliders.push(id);
        self.apply_mass(body, props);
        Ok(id)
    }
    pub fn remove_collider(&mut self, id: ColliderHandle) -> Result<(), PhysicsError> {
        let c = self.colliders.get(id).ok_or(PhysicsError::InvalidHandle)?;
        let body = c.parent.unwrap();
        let b = self.bodies[body].core();
        let props = if b.kind == Kind::Dynamic {
            Some(mass::compound(
                b.colliders
                    .iter()
                    .filter(|&&c| c != id)
                    .map(|c| &self.colliders[*c]),
                self.planar,
            )?)
        } else {
            None
        };
        self.wake_collider_neighbors(id);
        self.manifolds.retain(|m| m.key.0 != id && m.key.1 != id);
        self.next_manifolds
            .retain(|m| m.key.0 != id && m.key.1 != id);
        self.colliders.remove(id);
        self.bodies[body].core_mut().colliders.retain(|&h| h != id);
        self.apply_mass(body, props);
        Ok(())
    }
    /// Body-local pose; changing it recomputes mass without moving the authored body origin.
    pub fn set_collider_pose(
        &mut self,
        id: ColliderHandle,
        translation: Vec3,
        rotation: Quat,
    ) -> Result<(), PhysicsError> {
        let old = self.colliders.get(id).ok_or(PhysicsError::InvalidHandle)?;
        let mut c = old.clone();
        c.local_translation = translation;
        c.local_rotation = rotation
            .try_normalize()
            .ok_or(PhysicsError::InvalidCollider)?;
        if !c.valid(self.planar) {
            return Err(PhysicsError::InvalidCollider);
        }
        if old.local_translation == c.local_translation && old.local_rotation == c.local_rotation {
            return Ok(());
        }
        let body = c.parent.unwrap();
        let b = self.bodies[body].core();
        let props = if b.kind == Kind::Dynamic {
            Some(mass::compound(
                b.colliders
                    .iter()
                    .map(|h| if *h == id { &c } else { &self.colliders[*h] }),
                self.planar,
            )?)
        } else {
            None
        };
        c.touch_mass();
        self.colliders[id] = c;
        self.apply_mass(body, props);
        Ok(())
    }
    pub fn collider_world_pose(&self, id: ColliderHandle) -> Option<(Vec3, Quat)> {
        let c = self.colliders.get(id)?;
        let b = self.bodies.get(c.parent?)?.core();
        let p = c.pose(b);
        Some((p.position, p.rotation))
    }
    /// Replace only geometry, retaining the collider's handle, parent, child
    /// order, local pose, density, material, layers and sensor status. Only the
    /// shape of `geometry` is read; its other settings and parent are ignored.
    /// Use checked `ColliderBuilder::try_*` constructors for untrusted dimensions;
    /// legacy constructors can panic before this method is called.
    ///
    /// Checks the live collider/parent, candidate geometry and retained settings,
    /// resulting dynamic compound mass/inertia, then both revision capacities.
    /// An error changes no live state, including pending density edits and caches.
    /// Fixed/kinematic bodies accept zero density; dynamic bodies require valid
    /// positive compound mass even when the replaced child is a sensor.
    ///
    /// Matches attach/remove/local-pose mass edits: preserve the authored body
    /// origin, its world-space point velocity and angular velocity. COM velocity
    /// adjusts for a changed mass center; linear/angular momentum is not preserved.
    /// Forces, torque and kinematic targets are retained. Every success, even an
    /// identical shape, recomputes mass and wakes the edited body. Revisions make
    /// affected contact/joint impulses ineligible for reuse on the next valid step;
    /// old contacts, events and manifolds remain last-step snapshots until then.
    /// Current-pose queries see the new geometry immediately without stepping.
    ///
    /// No identity is inserted/removed and no collection allocates in this method;
    /// replacement works at body, child and collider capacity. Handles must come
    /// from this world (they do not encode world identity). Caller-defined `Into`
    /// conversions are outside the transaction/allocation guarantee.
    ///
    /// A lab editor can keep its selected child and authored material while
    /// previewing a new primitive and committing only after validation:
    ///
    /// ```
    /// # #[cfg(feature = "dim3")] {
    /// use ferravex_physics::d3::{ColliderBuilder, PhysicsWorld, RigidBodyBuilder};
    /// let mut world = PhysicsWorld::default();
    /// let (body, selected) = world.insert(
    ///     RigidBodyBuilder::dynamic(),
    ///     ColliderBuilder::ball(0.5).density(2.).friction(0.8),
    /// );
    /// let material = world.colliders[selected].material();
    /// let geometry = ColliderBuilder::try_cuboid(2., 0.5, 0.25).unwrap();
    /// world.replace_collider_shape(selected, geometry).unwrap();
    /// assert_eq!(world.bodies[body].colliders(), &[selected]);
    /// assert_eq!(world.colliders[selected].material(), material);
    /// assert_eq!(world.colliders[selected].density(), 2.);
    /// let hit = world.raycast([-4., 0., 0.], [1., 0., 0.], 10.).unwrap();
    /// assert_eq!(hit.collider, selected);
    /// assert!((hit.distance - 2.).abs() < 1e-5); // No query refresh or tick.
    /// # }
    /// # #[cfg(feature = "dim2")] {
    /// use ferravex_physics::d2::{
    ///     ColliderBuilder, ColliderShapeError, PhysicsWorld, RigidBodyBuilder,
    /// };
    /// let mut world = PhysicsWorld::default();
    /// let (body, selected) = world.insert(
    ///     RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.),
    /// );
    /// let mass = world.bodies[body].mass();
    /// // Positive geometry can still underflow dynamic mass: reject the edit.
    /// let tiny = ColliderBuilder::try_ball(f32::MIN_POSITIVE).unwrap();
    /// assert_eq!(world.replace_collider_shape(selected, tiny),
    ///     Err(ColliderShapeError::InvalidMass));
    /// assert_eq!(world.bodies[body].mass(), mass);
    /// world.replace_collider_shape(selected,
    ///     ColliderBuilder::try_cuboid(2., 0.5).unwrap()).unwrap();
    /// assert_eq!(world.bodies[body].colliders(), &[selected]);
    /// assert!((world.raycast([-4., 0.], [1., 0.], 10.).unwrap().distance - 2.)
    ///     .abs() < 1e-5);
    /// # }
    /// ```
    pub fn replace_collider_shape(
        &mut self,
        id: ColliderHandle,
        geometry: impl Into<Collider>,
    ) -> Result<(), ColliderShapeError> {
        use ColliderShapeError as E;
        let old = self.colliders.get(id).ok_or(E::InvalidHandle)?;
        let body = old.parent.ok_or(E::InvalidHandle)?;
        let b = self.bodies.get(body).ok_or(E::InvalidHandle)?.core();
        let mut candidate = old.clone();
        candidate.shape = geometry.into().shape;
        if candidate.shape.check_dimensions(self.planar).is_err() || !candidate.valid(self.planar) {
            return Err(E::InvalidCollider);
        }
        let properties = if b.kind == Kind::Dynamic {
            Some(
                mass::compound(
                    b.colliders.iter().map(|h| {
                        if *h == id {
                            &candidate
                        } else {
                            &self.colliders[*h]
                        }
                    }),
                    self.planar,
                )
                .map_err(|error| match error {
                    PhysicsError::InvalidMass => E::InvalidMass,
                    _ => E::InvalidCollider,
                })?,
            )
        } else {
            None
        };
        if old.revision.checked_add(1).is_none() || b.revision.checked_add(1).is_none() {
            return Err(E::RevisionExhausted);
        }
        candidate.touch_mass();
        self.colliders[id] = candidate;
        self.apply_mass(body, properties);
        Ok(())
    }
    /// Direct density setters are collected/validated before any mass update is committed.
    pub fn refresh_mass_properties(&mut self) -> Result<(), PhysicsError> {
        self.mass_updates.clear();
        for (id, body) in self.bodies.iter() {
            let b = body.core();
            if !b.colliders.iter().any(|h| self.colliders[*h].mass_dirty) {
                continue;
            }
            if b.colliders
                .iter()
                .any(|h| !self.colliders[*h].valid(self.planar))
            {
                return Err(PhysicsError::InvalidCollider);
            }
            let p = if b.kind == Kind::Dynamic {
                Some(mass::compound(
                    b.colliders.iter().map(|h| &self.colliders[*h]),
                    self.planar,
                )?)
            } else {
                None
            };
            self.mass_updates.push((id, p));
        }
        for i in 0..self.mass_updates.len() {
            let (id, p) = self.mass_updates[i];
            self.apply_mass(id, p);
        }
        Ok(())
    }
    fn apply_mass(&mut self, id: RigidBodyHandle, properties: Option<Mass>) {
        let body = self.bodies[id].core_mut();
        if let Some(properties) = properties {
            body.set_mass_properties(properties);
        } else {
            body.invalidate_contacts();
            body.wake();
        }
        for &child in &body.colliders {
            self.colliders[child].mass_dirty = false;
        }
    }
    pub(super) fn wake_collider_neighbors(&mut self, id: ColliderHandle) {
        for pair in &self.contacts {
            if pair.sensor {
                continue;
            }
            let other = if pair.collider1 == id {
                Some(pair.collider2)
            } else if pair.collider2 == id {
                Some(pair.collider1)
            } else {
                None
            };
            if let Some(body) = other
                .and_then(|h| self.colliders.get(h))
                .and_then(|c| c.parent)
                .and_then(|b| self.bodies.get_mut(b))
            {
                body.core_mut().wake();
            }
        }
    }
}

#[cfg(test)]
mod shape_replacement_tests {
    use super::*;
    #[derive(Debug)]
    struct TestBody(crate::body::Core);
    impl Access for TestBody {
        fn core(&self) -> &crate::body::Core {
            &self.0
        }
        fn core_mut(&mut self) -> &mut crate::body::Core {
            &mut self.0
        }
    }
    impl std::ops::Deref for TestBody {
        type Target = crate::body::Core;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl std::ops::DerefMut for TestBody {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    fn world(kind: Kind) -> (World<TestBody>, RigidBodyHandle, ColliderHandle) {
        let mut w: World<TestBody> = World::new(Vec3::ZERO, true);
        let body = w
            .insert_compound(
                TestBody(crate::body::Core::new(kind, true)),
                [Collider::new(Shape::Ball(1.))],
            )
            .unwrap();
        let child = w.bodies[body].colliders[0];
        (w, body, child)
    }
    fn snapshot(w: &World<TestBody>) -> String {
        format!(
            "{:?}{:?}{:?}{:?}{:?}",
            w.bodies,
            w.colliders,
            w.sleep_stats(),
            w.events,
            w.last_step
        )
    }

    #[test]
    fn revisions_are_preflighted_for_all_body_kinds_without_partial_changes() {
        for kind in [Kind::Dynamic, Kind::Fixed, Kind::Kinematic] {
            for exhausted_collider in [false, true] {
                let (mut w, body, child) = world(kind);
                w.bodies[body].force = Vec3::X;
                w.bodies[body].torque = Vec3::Z;
                w.bodies[body].next_position = Some(Vec3::Y);
                w.bodies[body].sleeping = true;
                if exhausted_collider {
                    w.colliders[child].revision = u64::MAX;
                } else {
                    w.bodies[body].revision = u64::MAX;
                }
                let before = snapshot(&w);
                assert_eq!(
                    w.replace_collider_shape(child, Collider::new(Shape::Ball(2.))),
                    Err(ColliderShapeError::RevisionExhausted)
                );
                assert_eq!(snapshot(&w), before);
            }
            let (mut w, body, child) = world(kind);
            w.bodies[body].revision = u64::MAX - 1;
            w.colliders[child].revision = u64::MAX - 1;
            w.replace_collider_shape(child, Collider::new(Shape::Ball(2.)))
                .unwrap();
            assert_eq!(w.bodies[body].revision, u64::MAX);
            assert_eq!(w.colliders[child].revision, u64::MAX);
            assert!(w.bodies[body].solver_dirty);
            assert!(w.colliders[child].dirty);
            assert!(!w.colliders[child].mass_dirty);
            let before = snapshot(&w);
            assert_eq!(
                w.replace_collider_shape(child, Collider::new(Shape::Ball(2.))),
                Err(ColliderShapeError::RevisionExhausted)
            );
            assert_eq!(snapshot(&w), before);
        }
    }

    #[test]
    fn malformed_shapes_and_retained_settings_are_rejected_atomically() {
        for kind in [Kind::Dynamic, Kind::Fixed, Kind::Kinematic] {
            let (mut w, _, child) = world(kind);
            let mut shapes = vec![
                Shape::Ball(0.),
                Shape::Ball(-1.),
                Shape::Ball(f32::NAN),
                Shape::Ball(f32::INFINITY),
                Shape::Box(Vec3::new(1., 0., 1.)),
                Shape::Box(Vec3::NAN),
            ];
            #[cfg(feature = "dim2")]
            shapes.extend([
                Shape::Capsule {
                    a: Vec3::NAN,
                    b: Vec3::ZERO,
                    radius: 1.,
                },
                Shape::Capsule {
                    a: Vec3::ZERO,
                    b: Vec3::Z,
                    radius: 1.,
                },
                Shape::Capsule {
                    a: Vec3::ZERO,
                    b: Vec3::ZERO,
                    radius: 0.,
                },
            ]);
            #[cfg(feature = "dim3")]
            shapes.push(Shape::Cylinder {
                half_height: 1.,
                radius: 1.,
            });
            let before = snapshot(&w);
            for shape in shapes {
                assert_eq!(
                    w.replace_collider_shape(child, Collider::new(shape)),
                    Err(ColliderShapeError::InvalidCollider)
                );
                assert_eq!(snapshot(&w), before);
            }
            for corrupt in [
                |c: &mut Collider| c.local_translation = Vec3::Z,
                |c: &mut Collider| c.local_rotation = Quat::from_xyzw(0., 0., 0., 0.),
                |c: &mut Collider| c.density = f32::NAN,
                |c: &mut Collider| c.material.restitution = f32::NAN,
            ] {
                let valid = w.colliders[child].clone();
                corrupt(&mut w.colliders[child]);
                let before = snapshot(&w);
                assert_eq!(
                    w.replace_collider_shape(child, Collider::new(Shape::Ball(2.))),
                    Err(ColliderShapeError::InvalidCollider)
                );
                assert_eq!(snapshot(&w), before);
                w.colliders[child] = valid;
            }
        }
    }

    #[test]
    fn success_commits_mass_once_and_preserves_retained_settings_and_queued_motion() {
        let (mut w, body, child) = world(Kind::Dynamic);
        w.bodies[body].force = Vec3::X;
        w.bodies[body].torque = Vec3::Z;
        w.bodies[body].next_position = Some(Vec3::Y);
        w.bodies[body].next_rotation = Some(Quat::IDENTITY);
        w.colliders[child].set_collision_layers(0b10, 0b100);
        let revision = w.bodies[body].revision;
        let collider_revision = w.colliders[child].revision;
        let mut geometry = Collider::new(Shape::Ball(2.));
        // Incoming authoring fields are irrelevant, even when malformed.
        geometry.density = f32::NAN;
        geometry.local_translation = Vec3::NAN;
        w.replace_collider_shape(child, geometry).unwrap();
        assert_eq!(w.bodies[body].revision, revision + 1);
        assert_eq!(w.colliders[child].revision, collider_revision + 1);
        assert_eq!(w.bodies[body].force, Vec3::X);
        assert_eq!(w.bodies[body].torque, Vec3::Z);
        assert_eq!(w.bodies[body].next_position, Some(Vec3::Y));
        assert_eq!(w.bodies[body].next_rotation, Some(Quat::IDENTITY));
        assert_eq!(w.colliders[child].membership, 0b10);
        assert_eq!(w.colliders[child].filter, 0b100);
        assert!(!w.colliders[child].mass_dirty);
        w.refresh_mass_properties().unwrap();
        assert_eq!(w.bodies[body].revision, revision + 1);
    }
}

#[cfg(test)]
mod shape_replacement_joint_tests {
    macro_rules! joint_test {
        ($name:ident, $dim:ident) => {
            #[test]
            fn $name() {
                use crate::$dim::{
                    ColliderBuilder, JointBuilder, PhysicsWorld, RigidBodyBuilder, Vector,
                };
                let mut w = PhysicsWorld::default();
                let anchor = w.insert_body(RigidBodyBuilder::fixed()).unwrap();
                let (body, child) = w.insert(
                    RigidBodyBuilder::dynamic()
                        .translation(Vector::X)
                        .linvel(Vector::X),
                    ColliderBuilder::ball(0.2),
                );
                let joint = w
                    .insert_joint(anchor, body, JointBuilder::distance(1.))
                    .unwrap();
                w.step(1. / 60.);
                assert!(w.joints[joint].cache.iter().any(|row| row.impulse != 0.));
                let description = w.joints[joint].description();
                let revisions = w.joints[joint].revisions;
                w.replace_collider_shape(child, ColliderBuilder::ball(0.4))
                    .unwrap();
                w.raw.refresh_joints();
                assert!(w.joints[joint].cache.iter().all(|row| row.impulse == 0.));
                assert_eq!(w.joints[joint].dt, 0.);
                assert_ne!(w.joints[joint].revisions, revisions);
                assert_eq!(w.joints[joint].bodies(), [anchor, body]);
                assert_eq!(w.joints[joint].description(), description);
            }
        };
    }
    #[cfg(feature = "dim2")]
    joint_test!(planar_replacement_clears_joint_warm_start_at_refresh, d2);
    #[cfg(feature = "dim3")]
    joint_test!(spatial_replacement_clears_joint_warm_start_at_refresh, d3);
}

#[cfg(test)]
mod journey400_224 {
    use super::*;
    use crate::assembly::{Assembly, AssemblyError, LocalBodyId};

    #[derive(Clone, Debug)]
    struct StagedBody(crate::body::Core);
    impl Access for StagedBody {
        fn core(&self) -> &crate::body::Core {
            &self.0
        }
        fn core_mut(&mut self) -> &mut crate::body::Core {
            &mut self.0
        }
    }
    fn snapshot(w: &World<StagedBody>) -> String {
        format!(
            "{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}{:?}",
            w.bodies,
            w.colliders,
            w.joints,
            w.contacts,
            w.next_contacts,
            w.manifolds
                .iter()
                .map(|m| (m.key, m.revisions, m.dt, m.points, m.len))
                .collect::<Vec<_>>(),
            w.next_manifolds
                .iter()
                .map(|m| (m.key, m.revisions, m.dt, m.points, m.len))
                .collect::<Vec<_>>(),
            w.events,
            w.last_step,
            w.sleep_stats(),
            w.mass_updates,
            w.next_feature
        )
    }
    #[test]
    fn dimension_body_and_late_child_errors_preserve_existing_state() {
        for planar in [true, false] {
            let mut w: World<StagedBody> = World::new(Vec3::ZERO, planar);
            let live = w
                .insert_compound(
                    StagedBody(crate::body::Core::new(Kind::Dynamic, planar)),
                    [Collider::new(Shape::Ball(1.))],
                )
                .unwrap();
            w.insert_compound(
                StagedBody(crate::body::Core::new(Kind::Dynamic, planar)),
                [Collider::new(Shape::Ball(1.))],
            )
            .unwrap();
            w.step(1. / 120.);
            let child = w.bodies[live].core().colliders[0];
            w.colliders[child].set_density(2.); // Pending edit must remain pending.
            for invalid in 0..7 {
                let mut plan = Assembly::new();
                plan.add_body(
                    StagedBody(crate::body::Core::new(Kind::Dynamic, planar)),
                    [Collider::new(Shape::Ball(0.5))],
                )
                .unwrap();
                let mut body = crate::body::Core::new(Kind::Fixed, planar);
                let mut collider = Collider::new(Shape::Ball(1.));
                let error = match invalid {
                    0 => {
                        body.planar = !planar;
                        PhysicsError::InvalidBody
                    }
                    1 => {
                        body.position.x = f32::NAN;
                        PhysicsError::InvalidBody
                    }
                    2 => {
                        body.rotation.w = f32::INFINITY;
                        PhysicsError::InvalidBody
                    }
                    3 => {
                        collider.shape = Shape::Ball(f32::NAN);
                        PhysicsError::InvalidCollider
                    }
                    4 => {
                        collider.shape = Shape::Box(Vec3::ZERO);
                        PhysicsError::InvalidCollider
                    }
                    5 => {
                        collider.density = f32::NAN;
                        PhysicsError::InvalidCollider
                    }
                    _ => {
                        collider.local_rotation = Quat::from_xyzw(0., 0., 0., 0.);
                        PhysicsError::InvalidCollider
                    }
                };
                let id = plan.add_body(StagedBody(body), [collider]).unwrap();
                assert_eq!(id, LocalBodyId::from_index(1));
                let before = snapshot(&w);
                assert_eq!(
                    w.insert_assembly(plan),
                    Err(AssemblyError::Body { body: id, error })
                );
                assert_eq!(snapshot(&w), before);
                assert!(w.colliders[child].mass_dirty);
            }
        }
    }
}
