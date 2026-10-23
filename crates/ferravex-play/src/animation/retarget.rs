//! Checked local-pose remapping between two real [`Skeleton`]s.
//!
//! Joint names and rest locals are parallel, caller-owned authoring metadata;
//! neither changes the skeleton, animation, skin, save or replay format. Names
//! are inspected only during preparation and are not retained. Each skeleton
//! has 1..=[MAX_ANIMATION_JOINTS] joints, globally unique nonempty names of at
//! most 4096 bytes, and parent-before-child order. Siblings may be reordered.
//! Matched joints must have matched **immediate** parents (or both be roots).
//!
//! The convention is **local right delta**, not world-space motion or IK:
//! `target_local = target_rest * inverse(source_rest) * source_local`.
//! Thus the source rest maps to the target rest; translation deltas rotate
//! into the target rest frame and scale by target/source rest scale. Rotation
//! deltas act after the target rest orientation. Different bone lengths and
//! orientations are supported, but no length fitting or end-effector constraint
//! is inferred. Parent/world transforms are evaluated later by the target
//! skeleton. Only finite translations, valid scene quaternions and strictly
//! positive, exactly uniform scales are supported. Reflection, nonuniform scale,
//! zero scale and unrepresentable checked inverses/results are explicit errors.
//! Shear has no input representation: this API accepts TRS locals only and never
//! decomposes arbitrary matrices. Uniform local scales cannot introduce shear
//! during hierarchy composition. Existing inverse-bind matrices are left intact
//! and are not assumed to be inverses of these caller-supplied rest locals.
//! Checked success is subject to f32 rounding and inverse conditioning; it is
//! not an accuracy certificate. Translation underflow may round to zero; scale
//! underflow to zero is rejected. An offset or source full rest inverse that
//! cannot be represented is rejected even if a particular final pose could fit.
//!
//! # Two-skeleton animation-to-skin recipe
//!
//! This headless recipe constructs real skeletons with different sibling order,
//! bone lengths and rest orientations, samples a real animation, and uses the
//! mapped locals with the existing world/skin APIs. No renderer is required.
//!
//! ```rust
//! use ferravex_math::{Mat4, Quat};
//! use ferravex_play::{animation::*, animation::retarget::*, scene::*};
//!
//! fn skeleton(parents: &[Option<usize>], rest: &[Transform]) -> Skeleton {
//!     let provisional = Skeleton::new(parents.iter().map(|&parent| Joint {
//!         parent, inverse_bind: IDENTITY,
//!     }).collect()).unwrap();
//!     let world = provisional.world_pose(rest).unwrap();
//!     Skeleton::new(parents.iter().zip(world).map(|(&parent, world)| Joint {
//!         parent,
//!         inverse_bind: Mat4::from_cols_array(&world).checked_inverse().unwrap()
//!             .to_cols_array(),
//!     }).collect()).unwrap()
//! }
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let source_rest = [Transform::default(), Transform {
//!         translation: [1., 0., 0.], ..Transform::default()
//!     }, Transform { translation: [0., 0., 1.], ..Transform::default() }];
//!     let target_rest = [Transform::default(), source_rest[2], Transform {
//!         translation: [0., 2., 0.],
//!         rotation: Quat::from_rotation_z(std::f32::consts::FRAC_PI_2).to_array(),
//!         scale: [2.; 3],
//!     }];
//!     let source = skeleton(&[None, Some(0), Some(0)], &source_rest);
//!     let target = skeleton(&[None, Some(0), Some(0)], &target_rest);
//!     let map = PreparedRetarget::try_new(&source, &["root", "arm", "marker"],
//!         &source_rest, &target, &["root", "marker", "arm"], &target_rest,
//!         MissingJointPolicy::Strict)?;
//!     let clip = AnimationClip::new(3, 1., LoopMode::Clamp, vec![JointTrack {
//!         joint: 1,
//!         translation: Some(Track::new(vec![
//!             Keyframe { time: 0., value: [1., 0., 0.] },
//!             Keyframe { time: 1., value: [2., 0., 0.] },
//!         ], Interpolation::Linear).unwrap()),
//!         rotation: Some(Track::new(vec![Keyframe { time: 1.,
//!             value: Quat::from_rotation_x(0.5).to_array() }], Interpolation::Step).unwrap()),
//!         scale: None,
//!     }], vec![]).unwrap();
//!     let mut sampled = [Transform::default(); 3];
//!     clip.sample_into(1., &source_rest, &mut sampled).unwrap();
//!     let mut mapped = [Transform::default(); 3];
//!     map.map_pose_into(&sampled, &mut mapped)?;
//!     assert!((mapped[2].translation[1] - 4.).abs() < 1e-5);
//!     let expected = Quat::from_array(target_rest[2].rotation)
//!         * Quat::from_rotation_x(0.5);
//!     assert!(Quat::from_array(mapped[2].rotation).dot(expected).abs() > 0.99999);
//!     let mut world = [IDENTITY; 3];
//!     let mut palette = [IDENTITY; 3];
//!     map.target().world_pose_into(&mapped, &mut world)?;
//!     assert!((world[2][13] - 4.).abs() < 1e-5);
//!     map.target().skin_palette_into(&mapped, &mut world, &mut palette)?;
//!     assert!(palette.iter().flatten().all(|v| v.is_finite()));
//!     // Feeding the rest again resets the result without any hidden history.
//!     map.map_pose_into(&source_rest, &mut mapped)?;
//!     assert_eq!(mapped, target_rest);
//!     map.target().skin_palette_into(&mapped, &mut world, &mut palette)?;
//!     for matrix in palette {
//!         assert!(Mat4::from_cols_array(&matrix).abs_diff_eq(Mat4::IDENTITY, 1e-5));
//!     }
//!     assert!(matches!(PreparedRetarget::try_new(&source, &["root", "arm", "arm"],
//!         &source_rest, &target, &["root", "marker", "arm"], &target_rest,
//!         MissingJointPolicy::Strict), Err(RetargetError::DuplicateName { .. })));
//!     assert!(matches!(PreparedRetarget::try_new(&source, &["root", "arm", "marker"],
//!         &source_rest, &target, &["root", "unknown", "arm"], &target_rest,
//!         MissingJointPolicy::Strict), Err(RetargetError::MissingJoint { .. })));
//!     Ok(())
//! }
//! ```

use super::MAX_ANIMATION_JOINTS;
use crate::scene::{Skeleton, Transform, multiply};
use ferravex_math::{
    Mat4, MatrixError, Quat, Vec3,
    affine::{AffineError, PreparedAffine3D},
};

/// Side of the mapping containing a diagnostic's joint index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkeletonSide {
    Source,
    Target,
}

/// Explicit policy for names present on only one side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingJointPolicy {
    /// Both name sets must match exactly; the first unmatched target, then
    /// source, is reported. This does not require identical sibling ordering.
    Strict,
    /// Unmatched target joints keep their rest locals; unused source joints
    /// are permitted (but their poses are still validated). Matched children
    /// of unmatched parents are rejected rather than silently reparented.
    TargetRestFallback,
}

/// Unsupported local TRS or failed checked arithmetic; no decomposition occurs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RetargetTransformError {
    InvalidTransform,
    NonPositiveScale,
    NonUniformScale,
    Matrix(MatrixError),
    Affine(AffineError),
}

/// Indexed authoring, compatibility, capacity or numeric diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RetargetError {
    JointLimit {
        side: SkeletonSide,
        count: usize,
    },
    MetadataLength {
        side: SkeletonSide,
        joints: usize,
        names: usize,
        rest: usize,
    },
    InvalidName {
        side: SkeletonSide,
        joint: usize,
    },
    DuplicateName {
        side: SkeletonSide,
        first: usize,
        duplicate: usize,
    },
    InvalidParent {
        side: SkeletonSide,
        joint: usize,
    },
    MissingJoint {
        side: SkeletonSide,
        joint: usize,
    },
    HierarchyMismatch {
        source_joint: usize,
        target_joint: usize,
    },
    Transform {
        side: SkeletonSide,
        joint: usize,
        error: RetargetTransformError,
    },
    PoseLength {
        required: usize,
        actual: usize,
    },
    Capacity {
        required: usize,
        available: usize,
    },
}

impl std::fmt::Display for RetargetTransformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTransform => f.write_str("invalid scene TRS or unrepresentable result"),
            Self::NonPositiveScale => f.write_str(
                "scale must be strictly positive; reflections and singular scales are unsupported",
            ),
            Self::NonUniformScale => {
                f.write_str("nonuniform scale is unsupported; no shear decomposition is performed")
            }
            Self::Matrix(error) => write!(f, "rest inverse: {error}"),
            Self::Affine(error) => write!(f, "checked affine: {error}"),
        }
    }
}
impl std::error::Error for RetargetTransformError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Matrix(e) => Some(e),
            Self::Affine(e) => Some(e),
            _ => None,
        }
    }
}
impl std::fmt::Display for RetargetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transform { side, joint, error } => write!(f, "{side:?} joint {joint}: {error}"),
            _ => write!(f, "retarget: {self:?}"),
        }
    }
}
impl std::error::Error for RetargetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Transform { error, .. } => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct JointMapping {
    source: usize,
    offset: PreparedAffine3D,
    rotation: Quat,
    scale_ratio: f64,
}

/// Immutable prepared map tied by shared borrows to the exact two skeletons.
/// Names are discarded; normalized rest locals and indexed offsets are owned.
/// Skeleton mutation/replacement while borrowed is prevented by Rust. Raw pose
/// slices carry no identity: the caller must supply source-order locals and use
/// the target-order output with [`Self::target`], not an unrelated same-size rig.
///
/// Construction allocates O(joints) bounded storage and compares names O(n²).
/// Mapping is O(joints), with no allocation/name lookup and at most two 256-local
/// stack buffers. There is no clock, playback state or mutable reconfiguration.
/// To reconfigure, construct a replacement first, then assign it only on success;
/// construction failure cannot mutate the last-good map. See the module recipe.
#[derive(Debug)]
pub struct PreparedRetarget<'s> {
    source: &'s Skeleton,
    target: &'s Skeleton,
    source_rest: Vec<Transform>,
    target_rest: Vec<Transform>,
    mapping: Vec<Option<JointMapping>>,
}

impl<'s> PreparedRetarget<'s> {
    /// Validate all metadata/rest locals before constructing reusable offsets.
    /// Names/rest arrays need only live for this call; both skeleton borrows
    /// live as long as the returned map. Bounds and names are checked before
    /// lookup; no automatic aliases, ancestor skipping or case folding occurs.
    /// Numeric failure on an offset is reported at its target joint index.
    pub fn try_new(
        source: &'s Skeleton,
        source_names: &[&str],
        source_rest: &[Transform],
        target: &'s Skeleton,
        target_names: &[&str],
        target_rest: &[Transform],
        policy: MissingJointPolicy,
    ) -> Result<Self, RetargetError> {
        let source_rest =
            validate_metadata(source, source_names, source_rest, SkeletonSide::Source)?;
        let target_rest =
            validate_metadata(target, target_names, target_rest, SkeletonSide::Target)?;
        let indices: Vec<_> = target_names
            .iter()
            .map(|name| source_names.iter().position(|s| s == name))
            .collect();
        if policy == MissingJointPolicy::Strict {
            if let Some(joint) = indices.iter().position(Option::is_none) {
                return Err(RetargetError::MissingJoint {
                    side: SkeletonSide::Target,
                    joint,
                });
            }
            if let Some(joint) = (0..source_names.len()).find(|i| !indices.contains(&Some(*i))) {
                return Err(RetargetError::MissingJoint {
                    side: SkeletonSide::Source,
                    joint,
                });
            }
        }
        let mut mapping = Vec::with_capacity(target_rest.len());
        for (t, &s) in indices.iter().enumerate() {
            let Some(s) = s else {
                mapping.push(None);
                continue;
            };
            let compatible = match (source.joints()[s].parent, target.joints()[t].parent) {
                (None, None) => true,
                (Some(sp), Some(tp)) => indices[tp] == Some(sp),
                _ => false,
            };
            if !compatible {
                return Err(RetargetError::HierarchyMismatch {
                    source_joint: s,
                    target_joint: t,
                });
            }
            let entry = (|| {
                let inverse = matrix(source_rest[s])
                    .checked_inverse()
                    .map_err(RetargetTransformError::Matrix)?;
                let offset = PreparedAffine3D::try_new(Mat4::from_cols_array(&multiply(
                    matrix(target_rest[t]).to_cols_array(),
                    inverse.to_cols_array(),
                )))
                .map_err(RetargetTransformError::Affine)?;
                let rotation = (Quat::from_array(target_rest[t].rotation)
                    * Quat::from_array(source_rest[s].rotation)
                        .try_inverse()
                        .ok_or(RetargetTransformError::InvalidTransform)?)
                .try_normalize()
                .ok_or(RetargetTransformError::InvalidTransform)?;
                Ok(JointMapping {
                    source: s,
                    offset,
                    rotation,
                    scale_ratio: f64::from(target_rest[t].scale[0])
                        / f64::from(source_rest[s].scale[0]),
                })
            })()
            .map_err(|error| RetargetError::Transform {
                side: SkeletonSide::Target,
                joint: t,
                error,
            })?;
            mapping.push(Some(entry));
        }
        Ok(Self {
            source,
            target,
            source_rest,
            target_rest,
            mapping,
        })
    }

    /// The exact source skeleton whose joint order the input slice must use.
    pub fn source(&self) -> &'s Skeleton {
        self.source
    }

    /// The exact target skeleton whose joint order the output prefix uses.
    pub fn target(&self) -> &'s Skeleton {
        self.target
    }

    /// Remap a complete source local pose into the target-length output prefix.
    /// Checks exact source length first, then sufficient output capacity. All
    /// inputs (including unused joints) and results are checked before **any**
    /// writes; all output bits/tails survive failure. Success preserves the
    /// unused tail. Quaternions are normalized using checked existing math.
    /// Feeding the original source rest metadata again returns the stored,
    /// normalized target rest exactly.
    /// Reusing the map after errors or feeding rest again requires no reset.
    /// Downstream world/skin evaluation remains a separate checked operation.
    pub fn map_pose_into(
        &self,
        source_pose: &[Transform],
        output: &mut [Transform],
    ) -> Result<(), RetargetError> {
        if source_pose.len() != self.source_rest.len() {
            return Err(RetargetError::PoseLength {
                required: self.source_rest.len(),
                actual: source_pose.len(),
            });
        }
        if output.len() < self.target_rest.len() {
            return Err(RetargetError::Capacity {
                required: self.target_rest.len(),
                available: output.len(),
            });
        }
        let mut source = [Transform::default(); MAX_ANIMATION_JOINTS];
        for (joint, (&pose, out)) in source_pose.iter().zip(&mut source).enumerate() {
            *out = checked_local(pose).map_err(|error| RetargetError::Transform {
                side: SkeletonSide::Source,
                joint,
                error,
            })?;
        }
        let mut staged = [Transform::default(); MAX_ANIMATION_JOINTS];
        for (joint, (&rest, mapping)) in self.target_rest.iter().zip(&self.mapping).enumerate() {
            let result = (|| {
                let Some(m) = mapping else {
                    return Ok(rest);
                };
                let pose = source[m.source];
                if pose == self.source_rest[m.source] {
                    return Ok(rest);
                }
                let translation = m
                    .offset
                    .transform_point(Vec3::from_array(pose.translation))
                    .map_err(RetargetTransformError::Affine)?
                    .to_array();
                let rotation = (m.rotation * Quat::from_array(pose.rotation))
                    .try_normalize()
                    .ok_or(RetargetTransformError::InvalidTransform)?
                    .to_array();
                let scale = [(m.scale_ratio * f64::from(pose.scale[0])) as f32; 3];
                checked_local(Transform {
                    translation,
                    rotation,
                    scale,
                })
            })()
            .map_err(|error| RetargetError::Transform {
                side: SkeletonSide::Target,
                joint,
                error,
            })?;
            staged[joint] = result;
        }
        output[..self.target_rest.len()].copy_from_slice(&staged[..self.target_rest.len()]);
        Ok(())
    }
}

fn matrix(local: Transform) -> Mat4 {
    // Only called with checked, normalized local transforms.
    Mat4::from_cols_array(&local.matrix().expect("checked scene transform"))
}

fn checked_local(mut local: Transform) -> Result<Transform, RetargetTransformError> {
    if !local.valid() {
        return Err(RetargetTransformError::InvalidTransform);
    }
    if local.scale.iter().any(|s| *s <= 0.) {
        return Err(RetargetTransformError::NonPositiveScale);
    }
    if local.scale[0] != local.scale[1] || local.scale[0] != local.scale[2] {
        return Err(RetargetTransformError::NonUniformScale);
    }
    local.rotation = Quat::from_array(local.rotation)
        .try_normalize()
        .ok_or(RetargetTransformError::InvalidTransform)?
        .to_array();
    PreparedAffine3D::try_new(matrix(local)).map_err(RetargetTransformError::Affine)?;
    Ok(local)
}

fn validate_metadata(
    skeleton: &Skeleton,
    names: &[&str],
    rest: &[Transform],
    side: SkeletonSide,
) -> Result<Vec<Transform>, RetargetError> {
    let joints = skeleton.joint_count();
    if joints == 0 || joints > MAX_ANIMATION_JOINTS {
        return Err(RetargetError::JointLimit {
            side,
            count: joints,
        });
    }
    if names.len() != joints || rest.len() != joints {
        return Err(RetargetError::MetadataLength {
            side,
            joints,
            names: names.len(),
            rest: rest.len(),
        });
    }
    for (joint, name) in names.iter().enumerate() {
        if name.is_empty() || name.len() > 4096 {
            return Err(RetargetError::InvalidName { side, joint });
        }
        if let Some(first) = names[..joint].iter().position(|n| n == name) {
            return Err(RetargetError::DuplicateName {
                side,
                first,
                duplicate: joint,
            });
        }
        if skeleton.joints()[joint].parent.is_some_and(|p| p >= joint) {
            return Err(RetargetError::InvalidParent { side, joint });
        }
    }
    rest.iter()
        .enumerate()
        .map(|(joint, &local)| {
            checked_local(local).map_err(|error| RetargetError::Transform { side, joint, error })
        })
        .collect()
}
