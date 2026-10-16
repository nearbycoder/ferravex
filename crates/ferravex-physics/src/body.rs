use crate::Rotation2;
use ferravex_math::{Quat, Vec3};
use std::ops::{Index, IndexMut};

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
    pub(crate) restitution: f32,
    pub(crate) friction: f32,
    pub(crate) sensor: bool,
    pub(crate) membership: u32,
    pub(crate) filter: u32,
    pub(crate) revision: u64,
    pub(crate) dirty: bool,
}
impl Collider {
    pub(crate) fn new(shape: Shape) -> Self {
        Self {
            shape,
            local_translation: Vec3::ZERO,
            local_rotation: Quat::IDENTITY,
            mass_dirty: false,
            parent: None,
            density: 1.,
            restitution: 0.,
            friction: 0.6,
            sensor: false,
            membership: u32::MAX,
            filter: u32::MAX,
            revision: 0,
            dirty: false,
        }
    }
    pub(crate) fn valid(&self, planar: bool) -> bool {
        self.local_translation.is_finite()
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
    pub fn set_restitution(&mut self, v: f32) {
        assert!(v.is_finite() && (0.0..=1.0).contains(&v));
        if self.restitution != v {
            self.revision = self
                .revision
                .checked_add(1)
                .expect("collider revision exhausted");
            self.dirty = true;
        }
        self.restitution = v;
    }
    pub fn set_friction(&mut self, v: f32) {
        assert!(v.is_finite() && v >= 0.);
        if self.friction != v {
            self.revision = self
                .revision
                .checked_add(1)
                .expect("collider revision exhausted");
            self.dirty = true;
        }
        self.friction = v;
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
