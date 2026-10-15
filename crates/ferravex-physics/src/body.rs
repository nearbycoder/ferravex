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
struct Slot<T> {
    generation: u32,
    value: Option<T>,
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
    pub(crate) parent: Option<RigidBodyHandle>,
    pub(crate) density: f32,
    pub(crate) restitution: f32,
    pub(crate) friction: f32,
    pub(crate) sensor: bool,
    pub(crate) membership: u32,
    pub(crate) filter: u32,
}
impl Collider {
    pub(crate) fn new(shape: Shape) -> Self {
        Self {
            shape,
            parent: None,
            density: 1.,
            restitution: 0.,
            friction: 0.6,
            sensor: false,
            membership: u32::MAX,
            filter: u32::MAX,
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
        self.restitution = v;
    }
    pub fn set_friction(&mut self, v: f32) {
        assert!(v.is_finite() && v >= 0.);
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
    pub(crate) inv_inertia: Vec3,
    pub(crate) force: Vec3,
    pub(crate) torque: Vec3,
    pub(crate) gravity_scale: f32,
    pub(crate) linear_damping: f32,
    pub(crate) angular_damping: f32,
    pub(crate) ccd: bool,
    pub(crate) sleeping: bool,
    pub(crate) quiet: f32,
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
            inv_inertia: Vec3::ZERO,
            force: Vec3::ZERO,
            torque: Vec3::ZERO,
            gravity_scale: 1.,
            linear_damping: 0.,
            angular_damping: 0.,
            ccd: false,
            sleeping: false,
            quiet: 0.,
            next_position: None,
            next_rotation: None,
            colliders: Vec::with_capacity(1),
        }
    }
    pub(crate) fn wake(&mut self) {
        self.sleeping = false;
        self.quiet = 0.;
    }
    pub(crate) fn set_rotation(&mut self, q: Quat) {
        assert!(q.is_finite());
        self.rotation = q.normalize();
        let axis = self.rotation * Vec3::X;
        self.rotation2 = Rotation2::from_angle(axis.y.atan2(axis.x));
    }
    pub(crate) fn inverse_inertia(&self, v: Vec3) -> Vec3 {
        self.rotation * ((self.rotation.conjugate() * v) * self.inv_inertia)
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
        self.velocity + self.angular_velocity.cross(p - self.position)
    }
    pub(crate) fn impulse(&mut self, j: Vec3, p: Vec3) {
        if self.kind == Kind::Dynamic && !self.sleeping {
            self.velocity += j * self.inv_mass;
            self.angular_velocity += self.inverse_inertia((p - self.position).cross(j));
        }
    }
    pub(crate) fn set_mass(&mut self, c: &Collider) {
        if self.kind != Kind::Dynamic {
            return;
        }
        let (volume, inertia) = match c.shape {
            Shape::Ball(r) => {
                if self.planar {
                    (std::f32::consts::PI * r * r, Vec3::new(0., 0., 0.5 * r * r))
                } else {
                    (
                        4. / 3. * std::f32::consts::PI * r * r * r,
                        Vec3::ONE * (0.4 * r * r),
                    )
                }
            }
            Shape::Box(h) => {
                if self.planar {
                    (
                        4. * h.x * h.y,
                        Vec3::new(0., 0., (h.x * h.x + h.y * h.y) / 3.),
                    )
                } else {
                    (
                        8. * h.x * h.y * h.z,
                        Vec3::new(
                            h.y * h.y + h.z * h.z,
                            h.x * h.x + h.z * h.z,
                            h.x * h.x + h.y * h.y,
                        ) / 3.,
                    )
                }
            }
            _ => panic!(
                "capsules/cylinders are supported as fixed or kinematic ball obstacles, not dynamic bodies"
            ),
        };
        self.mass = volume * c.density;
        assert!(self.mass.is_finite() && self.mass > 0.);
        self.inv_mass = 1. / self.mass;
        self.inv_inertia = Vec3::new(
            if inertia.x > 0. {
                1. / (self.mass * inertia.x)
            } else {
                0.
            },
            if inertia.y > 0. {
                1. / (self.mass * inertia.y)
            } else {
                0.
            },
            if inertia.z > 0. {
                1. / (self.mass * inertia.z)
            } else {
                0.
            },
        );
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
