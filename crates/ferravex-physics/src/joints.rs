//! Bounded, generational connections between two bodies. Anchors are relative to
//! authored body origins, so compound center-of-mass edits do not move a pivot.
use crate::PhysicsError;
use crate::body::{RigidBodyHandle, Slot};
use ferravex_math::{Quat, Vec3};
use std::ops::{Index, IndexMut};
pub const MAX_JOINTS: usize = 4096;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JointHandle {
    pub(crate) index: usize,
    pub(crate) generation: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JointKind {
    Fixed,
    Hinge {
        axis1: Vec3,
        axis2: Vec3,
    },
    Distance {
        length: f32,
    },
    /// Force = -stiffness * extension - damping * relative speed, solved implicitly.
    Spring {
        length: f32,
        stiffness: f32,
        damping: f32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointMotor {
    pub target_velocity: f32,
    pub target_position: Option<f32>,
    pub stiffness: f32,
    pub max_speed: f32,
    pub max_torque: f32,
}
impl JointMotor {
    pub fn velocity(speed: f32, max_torque: f32) -> Self {
        Self {
            target_velocity: speed,
            target_position: None,
            stiffness: 0.,
            max_speed: speed.abs(),
            max_torque,
        }
    }
    pub fn position(angle: f32, stiffness: f32, max_speed: f32, max_torque: f32) -> Self {
        Self {
            target_velocity: 0.,
            target_position: Some(angle),
            stiffness,
            max_speed,
            max_torque,
        }
    }
    fn valid(self) -> bool {
        self.target_velocity.is_finite()
            && self.target_velocity.abs() <= 1000.
            && self
                .target_position
                .is_none_or(|p| p.is_finite() && p.abs() <= std::f32::consts::PI)
            && [self.stiffness, self.max_speed, self.max_torque]
                .iter()
                .all(|v| v.is_finite() && (0. ..=1e9).contains(v))
            && self.max_speed <= 1000.
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointDesc {
    pub kind: JointKind,
    pub local_anchor1: Vec3,
    pub local_anchor2: Vec3,
    /// None captures the bodies' relative orientation at insertion.
    pub reference_rotation: Option<Quat>,
    pub contacts_enabled: bool,
    pub enabled: bool,
    /// Twist range relative to the captured orientation, in radians [-pi, pi].
    pub limits: Option<[f32; 2]>,
    pub motor: Option<JointMotor>,
}
impl JointDesc {
    pub fn new(kind: JointKind) -> Self {
        Self {
            kind,
            local_anchor1: Vec3::ZERO,
            local_anchor2: Vec3::ZERO,
            reference_rotation: None,
            contacts_enabled: false,
            enabled: true,
            limits: None,
            motor: None,
        }
    }
    pub(crate) fn valid(self, planar: bool) -> bool {
        let scalar = |v: f32| v.is_finite() && (0. ..=1e6).contains(&v);
        let hinge = matches!(self.kind, JointKind::Hinge { .. });
        [self.local_anchor1, self.local_anchor2]
            .iter()
            .all(|p| p.is_finite() && p.abs().max_element() <= 1e6 && (!planar || p.z == 0.))
            && self.reference_rotation.is_none_or(|q| {
                q.is_finite()
                    && (q.length_squared() - 1.).abs() < 0.001
                    && (!planar || (q * Vec3::Z - Vec3::Z).length_squared() < 1e-6)
            })
            && match self.kind {
                JointKind::Fixed => true,
                JointKind::Distance { length } => scalar(length),
                JointKind::Spring {
                    length,
                    stiffness,
                    damping,
                } => scalar(length) && scalar(stiffness) && stiffness > 0. && scalar(damping),
                JointKind::Hinge { axis1, axis2 } => [axis1, axis2].iter().all(|a| {
                    a.is_finite()
                        && (a.length_squared() - 1.).abs() < 0.001
                        && (!planar || a.distance_squared(Vec3::Z) < 1e-6)
                }),
            }
            && self.limits.is_none_or(|[lo, hi]| {
                hinge
                    && lo.is_finite()
                    && hi.is_finite()
                    && lo <= hi
                    && lo >= -std::f32::consts::PI
                    && hi <= std::f32::consts::PI
            })
            && self.motor.is_none_or(|m| hinge && m.valid())
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CachedRow {
    pub impulse: f32,
    pub axis: Vec3,
}
#[derive(Clone, Debug)]
pub struct Joint {
    pub(crate) a: RigidBodyHandle,
    pub(crate) b: RigidBodyHandle,
    pub(crate) desc: JointDesc,
    pub(crate) dirty: bool,
    pub(crate) revisions: [u64; 2],
    pub(crate) dt: f32,
    pub(crate) cache: [CachedRow; 8],
}
impl Joint {
    pub fn bodies(&self) -> [RigidBodyHandle; 2] {
        [self.a, self.b]
    }
    pub fn description(&self) -> JointDesc {
        self.desc
    }
    pub fn set_motor(&mut self, motor: Option<JointMotor>) -> Result<(), PhysicsError> {
        let desc = JointDesc { motor, ..self.desc };
        self.set_description(desc)
    }
    pub fn set_limits(&mut self, limits: Option<[f32; 2]>) -> Result<(), PhysicsError> {
        self.set_description(JointDesc {
            limits,
            ..self.desc
        })
    }
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.desc.enabled != enabled {
            self.desc.enabled = enabled;
            self.dirty = true;
        }
    }
    pub fn set_contacts_enabled(&mut self, enabled: bool) {
        if self.desc.contacts_enabled != enabled {
            self.desc.contacts_enabled = enabled;
            self.dirty = true;
        }
    }
    fn set_description(&mut self, desc: JointDesc) -> Result<(), PhysicsError> {
        // Dimension-specific validation was performed on insertion; these setters
        // only change dimension-independent motor/limit fields.
        if !desc.valid(false) {
            return Err(PhysicsError::InvalidJoint);
        }
        if self.desc != desc {
            self.desc = desc;
            self.dirty = true;
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct JointSet {
    slots: Vec<Slot<Joint>>,
    len: usize,
}
crate::body::arena!(JointSet, Joint, JointHandle);
