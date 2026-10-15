use super::*;
use crate::{
    PhysicsError,
    mass::{self, Mass},
};
pub const MAX_CHILDREN: usize = 32;
pub const MAX_COLLIDERS: usize = 16384;
impl<B: Access> World<B> {
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
