//! Bounded, generational connections between two bodies. Anchors are relative to
//! authored body origins, so compound center-of-mass edits do not move a pivot.
use crate::PhysicsError;
use crate::body::{RigidBodyHandle, Slot};
use ferravex_math::{Quat, Vec3};
use std::fmt;
use std::ops::{Index, IndexMut};
pub const MAX_JOINTS: usize = 4096;

/// The first invalid field in a joint description or live configuration.
///
/// Field names follow [`JointDesc`], with paths such as `limits.lower` and
/// `motor.max_speed`. Validation never clamps or normalizes authored values.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum JointValidationError {
    /// The named scalar, vector or quaternion contains NaN or infinity.
    NonFinite { field: &'static str },
    /// A finite scalar or vector component is outside the inclusive bounds.
    OutOfRange {
        field: &'static str,
        min: f32,
        max: f32,
    },
    /// Spring stiffness must be strictly greater than zero.
    MustBePositive { field: &'static str },
    /// A rotation or hinge axis must have squared length within 0.001 of one.
    NotUnit { field: &'static str },
    /// A planar anchor must have zero Z; rotations and axes must preserve +Z.
    NotPlanar { field: &'static str },
    /// Only hinge joints accept angular limits and motors.
    HingeRequired { field: &'static str },
    /// The lower angular limit exceeds the upper angular limit.
    InvertedLimits,
}

impl JointValidationError {
    /// Returns the field to highlight in an editor or diagnostic.
    ///
    /// ```
    /// use ferravex_physics::JointMotor;
    /// let error = JointMotor::velocity(1., -1.).validate().unwrap_err();
    /// assert_eq!(error.field(), "motor.max_torque");
    /// assert!(error.to_string().contains("between 0 and 1000000000"));
    /// ```
    pub fn field(&self) -> &'static str {
        match *self {
            Self::NonFinite { field }
            | Self::OutOfRange { field, .. }
            | Self::MustBePositive { field }
            | Self::NotUnit { field }
            | Self::NotPlanar { field }
            | Self::HingeRequired { field } => field,
            Self::InvertedLimits => "limits",
        }
    }
}

impl fmt::Display for JointValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "joint {}: ", self.field())?;
        match *self {
            Self::NonFinite { .. } => write!(f, "must contain only finite values"),
            Self::OutOfRange { min, max, .. } => {
                write!(f, "must be between {min} and {max}, inclusive")
            }
            Self::MustBePositive { .. } => write!(f, "must be greater than zero"),
            Self::NotUnit { .. } => {
                write!(f, "squared length must differ from 1 by less than 0.001")
            }
            Self::NotPlanar { .. } => {
                write!(
                    f,
                    "planar anchors require Z = 0; axes and rotations must preserve +Z"
                )
            }
            Self::HingeRequired { .. } => write!(f, "requires a hinge joint"),
            Self::InvertedLimits => write!(f, "lower limit must not exceed upper limit"),
        }
    }
}

impl std::error::Error for JointValidationError {}

fn validate_range(
    value: f32,
    field: &'static str,
    min: f32,
    max: f32,
) -> Result<(), JointValidationError> {
    if !value.is_finite() {
        Err(JointValidationError::NonFinite { field })
    } else if !(min..=max).contains(&value) {
        Err(JointValidationError::OutOfRange { field, min, max })
    } else {
        Ok(())
    }
}

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
    /// Checks the existing motor bounds without changing any value.
    ///
    /// Velocity is in [-1000, 1000] rad/s, position in [-pi, pi] radians,
    /// stiffness and maximum torque in [0, 1e9], and maximum speed in
    /// [0, 1000] rad/s. Zero torque or speed is allowed. The solver clamps a
    /// position target to the joint's limits; a target outside those limits is
    /// therefore valid if it remains within [-pi, pi].
    ///
    /// ```
    /// use ferravex_physics::JointMotor;
    /// JointMotor::position(0.5, 20., 8., 80.).validate().unwrap();
    /// let error = JointMotor::velocity(f32::NAN, 1.).validate().unwrap_err();
    /// assert_eq!(error.field(), "motor.target_velocity");
    /// ```
    pub fn validate(self) -> Result<(), JointValidationError> {
        validate_range(self.target_velocity, "motor.target_velocity", -1000., 1000.)?;
        if let Some(position) = self.target_position {
            validate_range(
                position,
                "motor.target_position",
                -std::f32::consts::PI,
                std::f32::consts::PI,
            )?;
        }
        validate_range(self.stiffness, "motor.stiffness", 0., 1e9)?;
        validate_range(self.max_speed, "motor.max_speed", 0., 1000.)?;
        validate_range(self.max_torque, "motor.max_torque", 0., 1e9)
    }
}
/// Conservative sums of absolute constraint row forces/torques, avoiding
/// cancellation between opposing rows. Units are newtons and newton-meters.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JointLoad {
    pub force: f32,
    pub torque: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BreakLimits {
    pub force: f32,
    pub torque: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JointBreakEvent {
    pub joint: JointHandle,
    pub bodies: [RigidBodyHandle; 2],
    pub load: JointLoad,
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
    pub break_limits: Option<BreakLimits>,
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
            break_limits: None,
        }
    }
    pub(crate) fn valid(self, planar: bool) -> bool {
        self.validate(planar).is_ok()
    }

    /// Checks authored fields, returning the first invalid field and its cause.
    ///
    /// Pass `true` for a 2D world and `false` for a 3D world. This uses the same
    /// bounds as insertion, but does not check body handles, body types or world
    /// capacity. Insertion still performs those checks. No values are changed.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_physics::{JointDesc, JointKind};
    /// let mut desc = JointDesc::new(JointKind::Hinge {
    ///     axis1: Vec3::Z,
    ///     axis2: Vec3::Z,
    /// });
    /// desc.limits = Some([-0.5, 0.5]);
    /// desc.validate(true).unwrap();
    /// desc.limits = Some([0.5, -0.5]);
    /// assert_eq!(desc.validate(true).unwrap_err().field(), "limits");
    /// ```
    pub fn validate(self, planar: bool) -> Result<(), JointValidationError> {
        use JointValidationError as Error;
        for (p, field) in [
            (self.local_anchor1, "local_anchor1"),
            (self.local_anchor2, "local_anchor2"),
        ] {
            if !p.is_finite() {
                return Err(Error::NonFinite { field });
            }
            if p.abs().max_element() > 1e6 {
                return Err(Error::OutOfRange {
                    field,
                    min: -1e6,
                    max: 1e6,
                });
            }
            if planar && p.z != 0. {
                return Err(Error::NotPlanar { field });
            }
        }
        if let Some(q) = self.reference_rotation {
            let field = "reference_rotation";
            if !q.is_finite() {
                return Err(Error::NonFinite { field });
            }
            if (q.length_squared() - 1.).abs() >= 0.001 {
                return Err(Error::NotUnit { field });
            }
            if planar && (q * Vec3::Z - Vec3::Z).length_squared() >= 1e-6 {
                return Err(Error::NotPlanar { field });
            }
        }
        match self.kind {
            JointKind::Fixed => {}
            JointKind::Distance { length } => validate_range(length, "kind.length", 0., 1e6)?,
            JointKind::Spring {
                length,
                stiffness,
                damping,
            } => {
                validate_range(length, "kind.length", 0., 1e6)?;
                validate_range(stiffness, "kind.stiffness", 0., 1e6)?;
                if stiffness == 0. {
                    return Err(Error::MustBePositive {
                        field: "kind.stiffness",
                    });
                }
                validate_range(damping, "kind.damping", 0., 1e6)?;
            }
            JointKind::Hinge { axis1, axis2 } => {
                for (a, field) in [(axis1, "kind.axis1"), (axis2, "kind.axis2")] {
                    if !a.is_finite() {
                        return Err(Error::NonFinite { field });
                    }
                    if (a.length_squared() - 1.).abs() >= 0.001 {
                        return Err(Error::NotUnit { field });
                    }
                    if planar && a.distance_squared(Vec3::Z) >= 1e-6 {
                        return Err(Error::NotPlanar { field });
                    }
                }
            }
        }
        let hinge = matches!(self.kind, JointKind::Hinge { .. });
        if let Some([lower, upper]) = self.limits {
            if !hinge {
                return Err(Error::HingeRequired { field: "limits" });
            }
            let pi = std::f32::consts::PI;
            validate_range(lower, "limits.lower", -pi, pi)?;
            validate_range(upper, "limits.upper", -pi, pi)?;
            if lower > upper {
                return Err(Error::InvertedLimits);
            }
        }
        if let Some(limits) = self.break_limits {
            validate_range(limits.force, "break_limits.force", 0., 1e12)?;
            validate_range(limits.torque, "break_limits.torque", 0., 1e12)?;
        }
        if let Some(motor) = self.motor {
            if !hinge {
                return Err(Error::HingeRequired { field: "motor" });
            }
            motor.validate()?;
        }
        Ok(())
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
    pub(crate) load: JointLoad,
    pub(crate) peak: JointLoad,
}
impl Joint {
    pub fn peak_load(&self) -> JointLoad {
        self.peak
    }
    pub fn bodies(&self) -> [RigidBodyHandle; 2] {
        [self.a, self.b]
    }
    pub fn description(&self) -> JointDesc {
        self.desc
    }
    /// Replaces the motor transactionally, preserving the existing error type.
    /// Use [`Self::set_limits_and_motor`] for field-specific errors.
    pub fn set_motor(&mut self, motor: Option<JointMotor>) -> Result<(), PhysicsError> {
        self.set_limits_and_motor(self.desc.limits, motor)
            .map_err(|_| PhysicsError::InvalidJoint)
    }
    /// Replaces angular limits transactionally, preserving the existing error type.
    /// Use [`Self::set_limits_and_motor`] for field-specific errors.
    pub fn set_limits(&mut self, limits: Option<[f32; 2]>) -> Result<(), PhysicsError> {
        self.set_limits_and_motor(limits, self.desc.motor)
            .map_err(|_| PhysicsError::InvalidJoint)
    }

    /// Replaces angular limits and the motor together, or changes nothing.
    ///
    /// `None` removes the corresponding setting. Nonempty settings require a
    /// hinge; finite limits must satisfy `-pi <= lower <= upper <= pi`. Equal
    /// endpoints are allowed. Motor bounds are checked by [`JointMotor::validate`].
    /// Position targets outside the limits retain the solver's target clamping.
    ///
    /// A rejected edit leaves the description and solver state intact. An
    /// identical edit does not wake the bodies or invalidate cached impulses.
    /// An accepted change uses the existing dirty flag so the next physics step
    /// clears cached impulses and wakes connected bodies. Handles, anchors and
    /// the captured reference rotation are preserved.
    ///
    /// ```
    /// # #[cfg(feature = "dim2")] {
    /// use ferravex_physics::{d2, JointMotor};
    /// let mut world = d2::PhysicsWorld::default();
    /// let anchor = world.insert_body(d2::RigidBodyBuilder::fixed()).unwrap();
    /// let rotor = world.add_box([0.; 2], [0.5, 0.2], true);
    /// let handle = world.insert_joint(anchor, rotor, d2::JointBuilder::hinge()).unwrap();
    /// let joint = world.joints.get_mut(handle).unwrap();
    /// joint.set_limits_and_motor(
    ///     Some([-0.6, 0.8]), Some(JointMotor::position(0.5, 20., 8., 80.)),
    /// ).unwrap();
    /// let before = joint.description();
    /// assert!(joint.set_limits_and_motor(Some([1., -1.]), None).is_err());
    /// assert_eq!(joint.description(), before);
    /// world.step(1. / 120.);
    /// # }
    /// ```
    pub fn set_limits_and_motor(
        &mut self,
        limits: Option<[f32; 2]>,
        motor: Option<JointMotor>,
    ) -> Result<(), JointValidationError> {
        let desc = JointDesc {
            limits,
            motor,
            ..self.desc
        };
        // Dimension-specific validation was performed on insertion; these fields
        // are dimension-independent. Validate the entire candidate before mutation.
        desc.validate(false)?;
        if self.desc != desc {
            self.desc = desc;
            self.dirty = true;
        }
        Ok(())
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
}
#[derive(Clone, Debug)]
pub struct JointSet {
    slots: Vec<Slot<Joint>>,
    len: usize,
}
crate::body::arena!(JointSet, Joint, JointHandle);

#[cfg(test)]
mod tests {
    use super::*;
    use JointValidationError as Error;
    use std::f32::consts::PI;

    fn hinge() -> JointDesc {
        JointDesc::new(JointKind::Hinge {
            axis1: Vec3::Z,
            axis2: Vec3::Z,
        })
    }

    #[test]
    fn limit_errors_identify_the_endpoint_and_preserve_inclusive_bounds() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for (limits, field) in [([value, 0.], "limits.lower"), ([0., value], "limits.upper")] {
                let desc = JointDesc {
                    limits: Some(limits),
                    ..hinge()
                };
                assert_eq!(desc.validate(false), Err(Error::NonFinite { field }));
            }
        }
        for (limits, field) in [([-4., 0.], "limits.lower"), ([0., 4.], "limits.upper")] {
            assert_eq!(
                JointDesc {
                    limits: Some(limits),
                    ..hinge()
                }
                .validate(false),
                Err(Error::OutOfRange {
                    field,
                    min: -PI,
                    max: PI
                }),
            );
        }
        assert_eq!(
            JointDesc {
                limits: Some([1., -1.]),
                ..hinge()
            }
            .validate(false),
            Err(Error::InvertedLimits),
        );
        for limits in [[-PI, PI], [-PI, -PI], [PI, PI], [0., 0.]] {
            for planar in [false, true] {
                assert!(
                    JointDesc {
                        limits: Some(limits),
                        ..hinge()
                    }
                    .validate(planar)
                    .is_ok()
                );
            }
        }
    }

    #[test]
    fn motor_validation_reports_each_field_without_clamping() {
        let base = JointMotor::position(0.5, 20., 8., 80.);
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, f32::MAX] {
            for (motor, field) in [
                (
                    JointMotor {
                        target_velocity: value,
                        ..base
                    },
                    "motor.target_velocity",
                ),
                (
                    JointMotor {
                        target_position: Some(value),
                        ..base
                    },
                    "motor.target_position",
                ),
                (
                    JointMotor {
                        stiffness: value,
                        ..base
                    },
                    "motor.stiffness",
                ),
                (
                    JointMotor {
                        max_speed: value,
                        ..base
                    },
                    "motor.max_speed",
                ),
                (
                    JointMotor {
                        max_torque: value,
                        ..base
                    },
                    "motor.max_torque",
                ),
            ] {
                let error = motor.validate().unwrap_err();
                assert_eq!(error.field(), field);
                if !value.is_finite() {
                    assert_eq!(error, Error::NonFinite { field });
                } else {
                    assert!(matches!(error, Error::OutOfRange { .. }));
                }
                assert!(error.to_string().contains(field));
            }
        }
        for (motor, field, min, max) in [
            (
                JointMotor {
                    target_velocity: -1001.,
                    ..base
                },
                "motor.target_velocity",
                -1000.,
                1000.,
            ),
            (
                JointMotor {
                    target_position: Some(-4.),
                    ..base
                },
                "motor.target_position",
                -PI,
                PI,
            ),
            (
                JointMotor {
                    stiffness: -1.,
                    ..base
                },
                "motor.stiffness",
                0.,
                1e9,
            ),
            (
                JointMotor {
                    max_speed: -1.,
                    ..base
                },
                "motor.max_speed",
                0.,
                1000.,
            ),
            (
                JointMotor {
                    max_torque: -1.,
                    ..base
                },
                "motor.max_torque",
                0.,
                1e9,
            ),
        ] {
            assert_eq!(motor.validate(), Err(Error::OutOfRange { field, min, max }));
        }
        for motor in [
            JointMotor::velocity(0., 0.),
            JointMotor::velocity(-1000., 1e9),
            JointMotor::velocity(1000., 1e9),
            JointMotor::position(-PI, 0., 0., 0.),
            JointMotor::position(PI, 1e9, 1000., 1e9),
        ] {
            assert!(motor.validate().is_ok());
        }
    }

    #[test]
    fn descriptor_validation_explains_geometry_and_planar_restrictions() {
        for (desc, error) in [
            (
                JointDesc {
                    local_anchor1: Vec3::splat(f32::NAN),
                    ..hinge()
                },
                Error::NonFinite {
                    field: "local_anchor1",
                },
            ),
            (
                JointDesc {
                    local_anchor2: Vec3::splat(f32::INFINITY),
                    ..hinge()
                },
                Error::NonFinite {
                    field: "local_anchor2",
                },
            ),
            (
                JointDesc {
                    local_anchor2: Vec3::splat(1e7),
                    ..hinge()
                },
                Error::OutOfRange {
                    field: "local_anchor2",
                    min: -1e6,
                    max: 1e6,
                },
            ),
            (
                JointDesc {
                    reference_rotation: Some(Quat::from_xyzw(0., 0., 0., f32::NAN)),
                    ..hinge()
                },
                Error::NonFinite {
                    field: "reference_rotation",
                },
            ),
            (
                JointDesc {
                    reference_rotation: Some(Quat::from_xyzw(0., 0., 0., 2.)),
                    ..hinge()
                },
                Error::NotUnit {
                    field: "reference_rotation",
                },
            ),
            (
                JointDesc::new(JointKind::Hinge {
                    axis1: Vec3::ZERO,
                    axis2: Vec3::Z,
                }),
                Error::NotUnit {
                    field: "kind.axis1",
                },
            ),
            (
                JointDesc::new(JointKind::Hinge {
                    axis1: Vec3::Z,
                    axis2: Vec3::splat(f32::MAX),
                }),
                Error::NotUnit {
                    field: "kind.axis2",
                },
            ),
            (
                JointDesc::new(JointKind::Hinge {
                    axis1: Vec3::Z,
                    axis2: Vec3::splat(f32::INFINITY),
                }),
                Error::NonFinite {
                    field: "kind.axis2",
                },
            ),
        ] {
            assert_eq!(desc.validate(false), Err(error));
        }
        for (desc, field) in [
            (
                JointDesc {
                    local_anchor1: Vec3::Z,
                    ..hinge()
                },
                "local_anchor1",
            ),
            (
                JointDesc {
                    reference_rotation: Some(Quat::from_rotation_x(0.5)),
                    ..hinge()
                },
                "reference_rotation",
            ),
            (
                JointDesc::new(JointKind::Hinge {
                    axis1: Vec3::Y,
                    axis2: Vec3::Y,
                }),
                "kind.axis1",
            ),
            (
                JointDesc::new(JointKind::Hinge {
                    axis1: Vec3::Z,
                    axis2: -Vec3::Z,
                }),
                "kind.axis2",
            ),
        ] {
            assert_eq!(desc.validate(false), Ok(()));
            assert_eq!(desc.validate(true), Err(Error::NotPlanar { field }));
        }
    }

    #[test]
    fn descriptor_validation_preserves_kind_and_break_threshold_rules() {
        for kind in [
            JointKind::Fixed,
            JointKind::Distance { length: 0. },
            JointKind::Spring {
                length: 0.,
                stiffness: 1.,
                damping: 0.,
            },
        ] {
            let desc = JointDesc::new(kind);
            assert!(desc.validate(false).is_ok());
            assert_eq!(
                JointDesc {
                    limits: Some([0., 0.]),
                    ..desc
                }
                .validate(false),
                Err(Error::HingeRequired { field: "limits" })
            );
            assert_eq!(
                JointDesc {
                    motor: Some(JointMotor::velocity(0., 0.)),
                    ..desc
                }
                .validate(false),
                Err(Error::HingeRequired { field: "motor" })
            );
        }
        for (kind, error) in [
            (
                JointKind::Distance { length: -1. },
                Error::OutOfRange {
                    field: "kind.length",
                    min: 0.,
                    max: 1e6,
                },
            ),
            (
                JointKind::Spring {
                    length: 1.,
                    stiffness: 0.,
                    damping: 0.,
                },
                Error::MustBePositive {
                    field: "kind.stiffness",
                },
            ),
            (
                JointKind::Spring {
                    length: 1.,
                    stiffness: 1.,
                    damping: f32::NAN,
                },
                Error::NonFinite {
                    field: "kind.damping",
                },
            ),
        ] {
            assert_eq!(JointDesc::new(kind).validate(false), Err(error));
        }
        for (limits, error) in [
            (
                BreakLimits {
                    force: f32::INFINITY,
                    torque: 0.,
                },
                Error::NonFinite {
                    field: "break_limits.force",
                },
            ),
            (
                BreakLimits {
                    force: 0.,
                    torque: -1.,
                },
                Error::OutOfRange {
                    field: "break_limits.torque",
                    min: 0.,
                    max: 1e12,
                },
            ),
        ] {
            assert_eq!(
                JointDesc {
                    break_limits: Some(limits),
                    ..hinge()
                }
                .validate(false),
                Err(error)
            );
        }
        assert!(
            JointDesc {
                local_anchor1: Vec3::splat(-1e6),
                local_anchor2: Vec3::splat(1e6),
                break_limits: Some(BreakLimits {
                    force: 0.,
                    torque: 1e12
                }),
                ..JointDesc::new(JointKind::Spring {
                    length: 1e6,
                    stiffness: 1e6,
                    damping: 1e6
                })
            }
            .validate(false)
            .is_ok()
        );
    }

    // Check all persistent state, including impulses and pending invalidation,
    // rather than just the public description after a failed transaction.
    #[cfg(feature = "dim3")]
    fn assert_same_state(actual: &Joint, before: &Joint) {
        assert_eq!(actual.bodies(), before.bodies());
        assert_eq!(actual.desc, before.desc);
        assert_eq!(actual.dirty, before.dirty);
        assert_eq!(actual.revisions, before.revisions);
        assert_eq!(actual.dt, before.dt);
        assert_eq!(actual.load, before.load);
        assert_eq!(actual.peak, before.peak);
        for (actual, before) in actual.cache.iter().zip(&before.cache) {
            assert_eq!(actual.impulse, before.impulse);
            assert_eq!(actual.axis, before.axis);
        }
    }

    #[cfg(feature = "dim3")]
    #[test]
    fn live_edits_are_atomic_and_noops_preserve_solver_state() {
        use crate::d3;
        let mut world = d3::PhysicsWorld::default();
        let a = world.insert_body(d3::RigidBodyBuilder::fixed()).unwrap();
        let b = world.add_box([0.; 3], [0.5; 3], true);
        let id = world
            .insert_joint(
                a,
                b,
                d3::JointBuilder::hinge().motor(JointMotor::velocity(2., 10.)),
            )
            .unwrap();
        world.step(1. / 120.);
        assert!(world.joints[id].cache.iter().any(|row| row.impulse != 0.));
        let joint = &mut world.joints[id];
        let before = joint.clone();
        joint
            .set_limits_and_motor(before.desc.limits, before.desc.motor)
            .unwrap();
        joint.set_limits(before.desc.limits).unwrap();
        joint.set_motor(before.desc.motor).unwrap();
        assert_same_state(joint, &before);
        for limits in [[1., -1.], [f32::NAN, 1.], [-1., f32::INFINITY], [-4., 0.]] {
            assert!(joint.set_limits_and_motor(Some(limits), None).is_err());
            assert_same_state(joint, &before);
            assert_eq!(
                joint.set_limits(Some(limits)),
                Err(PhysicsError::InvalidJoint)
            );
            assert_same_state(joint, &before);
        }
        let bad_motor = Some(JointMotor::velocity(f32::NAN, 1.));
        assert!(
            joint
                .set_limits_and_motor(Some([-1., 1.]), bad_motor)
                .is_err()
        );
        assert_same_state(joint, &before);
        assert_eq!(joint.set_motor(bad_motor), Err(PhysicsError::InvalidJoint));
        assert_same_state(joint, &before);

        joint.set_limits_and_motor(Some([-0.5, 0.5]), None).unwrap();
        assert!(joint.dirty);
        let mut expected = before.clone();
        expected.desc.limits = Some([-0.5, 0.5]);
        expected.desc.motor = None;
        expected.dirty = true;
        assert_same_state(joint, &expected);
        // A failure or no-op must not discard an earlier pending accepted edit.
        assert!(joint.set_limits_and_motor(Some([1., -1.]), None).is_err());
        joint.set_limits_and_motor(Some([-0.5, 0.5]), None).unwrap();
        assert_same_state(joint, &expected);
        // Existing refresh owns cache invalidation and endpoint wake-up.
        world.step(1. / 120.);
        assert!(!world.joints[id].dirty);
        assert!(world.joints[id].dt > 0.);
        assert!(world.joints[id].cache.iter().all(|row| row.impulse == 0.));
        assert_eq!(world.joints[id].bodies(), [a, b]);

        assert!(world.remove_joint(id));
        let replacement = world.insert_joint(a, b, d3::JointBuilder::fixed()).unwrap();
        assert_ne!(replacement, id);
        assert!(world.joints.get_mut(id).is_none());
        let fixed = &mut world.joints[replacement];
        let before = fixed.clone();
        assert_eq!(
            fixed.set_limits_and_motor(Some([0., 0.]), None),
            Err(Error::HingeRequired { field: "limits" })
        );
        assert_eq!(
            fixed.set_limits_and_motor(None, Some(JointMotor::velocity(1., 1.))),
            Err(Error::HingeRequired { field: "motor" })
        );
        fixed.set_limits_and_motor(None, None).unwrap();
        assert_same_state(fixed, &before);
    }

    #[cfg(feature = "dim2")]
    #[test]
    fn accepted_configuration_wakes_a_sleeping_hinge_and_drives_clamped_target() {
        use crate::d2;
        const DT: f32 = 1. / 120.;
        let mut world = d2::PhysicsWorld::default();
        let a = world.insert_body(d2::RigidBodyBuilder::fixed()).unwrap();
        let b = world.add_box([0.; 2], [0.5, 0.2], true);
        let id = world.insert_joint(a, b, d2::JointBuilder::hinge()).unwrap();
        for _ in 0..240 {
            world.step(DT);
        }
        assert!(world.bodies[b].is_sleeping());
        world.joints[id].set_limits_and_motor(None, None).unwrap();
        assert!(
            world.joints[id]
                .set_limits_and_motor(Some([1., -1.]), None)
                .is_err()
        );
        world.step(DT);
        assert!(world.bodies[b].is_sleeping());

        world.joints[id]
            .set_limits_and_motor(
                Some([-0.4, 0.4]),
                Some(JointMotor::position(0.9, 20., 8., 80.)),
            )
            .unwrap();
        assert!(world.bodies[b].is_sleeping());
        world.step(DT);
        assert!(!world.bodies[b].is_sleeping());
        for _ in 0..240 {
            world.step(DT);
            assert!((-0.405..=0.405).contains(&world.joint_angle(id).unwrap()));
        }
        assert!((world.joint_angle(id).unwrap() - 0.4).abs() < 0.005);
        assert_eq!(
            world.joints[id].desc.motor.unwrap().target_position,
            Some(0.9)
        );

        // Both values disappear together; the now-unpowered rotor passes its old limit.
        world.joints[id].set_limits_and_motor(None, None).unwrap();
        world.bodies[b].set_angvel(2., true);
        for _ in 0..30 {
            world.step(DT);
        }
        assert!(world.joint_angle(id).unwrap() > 0.7);
        assert_eq!(world.joints[id].description().limits, None);
        assert_eq!(world.joints[id].description().motor, None);
    }
}
