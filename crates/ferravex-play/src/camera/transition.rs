//! Opt-in, checked camera presentation blends. No clock, simulation, collision
//! queries, input, or authoritative pose history is owned here.
//!
//! Blend eye position, positive eye-to-target distance and vertical FOV linearly;
//! blend the orthonormal view orientation along the shortest quaternion arc.
//! The look target is reconstructed from the blended eye, direction and distance,
//! not interpolated independently through a possible coincident eye/target.
//! Up scale and its component parallel to forward do not affect the view: interior
//! samples use unit perpendicular up. Exact endpoints retain the supplied `Pose3`
//! verbatim, including its up vector. No Euler-angle or swing/twist stack is added.
//!
//! Only symmetric, finite-far perspective lenses are described by [`Perspective3`].
//! Positive-duration blends require exactly matching lens descriptors, including
//! aspect, clipping planes, handedness and depth convention; only FOV blends.
//! Orthographic, infinite-far, asymmetric and custom projections need a host-owned
//! cut/adapter. Descriptors declare intent; the host must use the same conventions
//! in its renderer. Internally orientation uses local -Z forward/+Y up as in
//! `Mat4::try_look_to_rh`; the resulting world-space forward/up also work with the
//! host's LH view builder. Handedness never changes within a blend.
//!
//! Retargeting starts at the last **displayed** pose and resets segment time. It
//! preserves a hold; `resume` continues the new segment without consuming paused
//! time. A cut (including a zero-duration retarget or a `Frame3::cut`) immediately
//! replaces all endpoints and releases hold. `reset` forgets the pose entirely.
//! The next valid target initializes directly. Host-owned teleport/restart and
//! renderer history must also be reset; a camera transition cannot repair stale
//! simulation interpolation. Input frames are borrowed: their obstruction,
//! recovery and query diagnostics remain observations of the host's original
//! query, never observations of the blended path. Output is a `Pose3`, not a
//! fabricated `Frame3`. A blended path can cross obstacles even with safe endpoints.
//!
//! # Pause, continuous retarget, accepted teleport and restart
//!
//! Runnable headless recipe using actual `Pose3`, `Frame3` and `Rig3`. The host
//! decides when simulation/network/replay updates execute, even during a local
//! menu. These injected presentation deltas never schedule simulation ticks.
//!
//! ```rust
//! use ferravex_math::{Vec3, projection::NdcDepth};
//! use ferravex_play::camera::{Input3, Mode3, NoObstructions, Pose3, Rig3, Target3};
//! use ferravex_play::camera::transition::{Perspective3, Transition3, ViewHandedness};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let lens = Perspective3 { aspect: 16. / 9., near: 0.1, far: 1000.,
//!         handedness: ViewHandedness::Right, depth: NdcDepth::NegativeOneToOne };
//!     let mut view = Transition3::new();
//!     assert_eq!(view.pose(), None);
//!     view.cut(Pose3::default(), lens)?;
//!     view.retarget(Pose3 { position: Vec3::X * 8.,
//!         target: Vec3::X * 8. - Vec3::Z, ..Pose3::default() }, lens, 1.)?;
//!     let visible = view.advance(0.25)?.unwrap();
//!     assert_eq!(visible.position.x, 2.);
//!     assert!(view.is_blending());
//!     assert_eq!(view.projection(), Some(lens));
//!     assert!(view.hold());
//!     assert!(view.is_held());
//!     assert_eq!(view.advance(0.25)?, Some(visible));
//!     // A local menu does not block authoritative updates. A newly selected
//!     // presentation target may replace the pending segment while held.
//!     view.retarget(Pose3::default(), lens, 0.5)?;
//!     assert_eq!(view.pose(), Some(visible));
//!     assert!(view.resume());
//!     assert_eq!(view.advance(0.)?, Some(visible));
//!     assert_eq!(view.advance(0.25)?.unwrap().position.x, 1.);
//!
//!     let mut rig = Rig3::default();
//!     // Simulation has accepted this teleport; do not pass a pending request
//!     // or an old interpolated subject. The host already reset subject history.
//!     let accepted = Target3::at(Vec3::X * 100.);
//!     let frame = rig.snap(Input3 { target: accepted, mode: Mode3::Follow {
//!         eye_offset: Vec3::Z * 5., look_offset: Vec3::ZERO }, fov_y: 1. },
//!         &NoObstructions)?;
//!     assert!(frame.cut);
//!     let generation = view.generation();
//!     view.retarget_frame(&frame, lens, 1.)?; // Cut overrides blend and hold.
//!     assert_eq!(view.generation(), generation + 1);
//!     assert_eq!(view.advance(0.)?, Some(frame.pose));
//!     assert!(!view.is_blending());
//!     view.reset(); // Also reset host simulation/rig/history for a real restart.
//!     assert_eq!(view.advance(0.25)?, None);
//!     view.retarget(Pose3::default(), lens, 1.)?;
//!     assert_eq!(view.pose(), Some(Pose3::default()));
//!     Ok(())
//! }
//! ```
//!
//! # Numerical and storage limits
//!
//! Eye coordinates have absolute components <= 1e6; view distance is in
//! `0.001..=1e6`; FOV is in `0.05..=3.1` radians (the rig's existing FOV range).
//! Up must be finite/nonzero, with sine of its angle to forward >= 1e-4. Near
//! parallel up is rejected, not replaced with an arbitrary roll. Subtraction,
//! distance and scalar interpolation use f64 intermediates. Rotations use existing
//! checked quaternion normalization, angular distance and rotate-towards helpers.
//! Extracted endpoint quaternions are canonicalized to positive w, or, at w == 0,
//! first nonzero x/y/z positive. A negative endpoint dot flips the destination;
//! exact zero-dot half-turn ties retain these signs, as in the math helpers.
//! Small perturbations near half turns may choose the opposite equally short arc.
//!
//! Durations are finite seconds in `0..=3600`; per-call presentation deltas are
//! finite in `0..=0.25`, including during hold/idle. Long suspension time must be
//! discarded/rebased by the host or handled with a cut. Elapsed segment time uses
//! f64, capped at duration. Interior outputs round to f32; at large coordinates a
//! short reconstructed target may round onto the eye. Such samples return
//! [`TransitionError::NumericalLimit`] atomically. Valid endpoints do not guarantee
//! every intermediate target is representable. Endpoint completion bypasses
//! reconstruction and is exact; no cross-platform bitwise determinism is promised.
//! All methods use fixed-size inline state and bounded work, without heap storage.

use super::{Frame3, Pose3};
use ferravex_math::{Mat4, Quat, Vec3, Vec4, projection::NdcDepth};

/// Host view-matrix convention. See the module recipe for lens construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewHandedness {
    /// Host uses a right-handed view, looking along local -Z.
    Right,
    /// Host uses a left-handed view, looking along local +Z.
    Left,
}

/// Explicit symmetric finite-far perspective intent; FOV comes from `Pose3`.
///
/// `aspect` must be finite/positive, and finite planes satisfy `0 < near < far`.
/// The host owns projection construction and its representability. Exact equality
/// is required for nonzero blends. To change these values, cut explicitly (or use
/// zero duration). See the module recipe; no lens is silently inferred/defaulted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Perspective3 {
    /// Width divided by height, in the host's actual viewport.
    pub aspect: f32,
    /// Positive near-plane distance.
    pub near: f32,
    /// Finite far-plane distance, strictly greater than near.
    pub far: f32,
    /// View-matrix convention, independent of the world-space pose.
    pub handedness: ViewHandedness,
    /// Explicit clip-space depth convention, reusing the math projector's type.
    pub depth: NdcDepth,
}

/// Rejected input leaves the complete transition unchanged, even during hold.
/// See [`Transition3::retarget`] for an executable error example.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionError {
    /// Invalid/nonfinite/degenerate pose or documented pose bound exceeded.
    InvalidPose,
    /// Invalid finite-far perspective descriptor.
    InvalidProjection,
    /// Duration is nonfinite or outside `0..=3600` seconds.
    InvalidDuration,
    /// Delta is nonfinite or outside `0..=0.25` seconds.
    InvalidDelta,
    /// A positive-duration blend would change projection conventions/parameters.
    IncompatibleProjection,
    /// A reconstructed interior pose is not representable within the pose limits.
    NumericalLimit,
}
impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "camera transition: {self:?}")
    }
}
impl std::error::Error for TransitionError {}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PreparedPose {
    pose: Pose3,
    rotation: Quat,
    distance: f64,
}
impl PreparedPose {
    fn new(pose: Pose3) -> Result<Self, TransitionError> {
        if !pose.position.is_finite()
            || pose.position.abs().max_element() > 1e6
            || !pose.target.is_finite()
            || !pose.fov_y.is_finite()
            || !(0.05..=3.1).contains(&pose.fov_y)
        {
            return Err(TransitionError::InvalidPose);
        }
        let delta: [f64; 3] =
            std::array::from_fn(|i| f64::from(pose.target[i]) - f64::from(pose.position[i]));
        let distance = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
        if !(0.001..=1e6).contains(&distance) {
            return Err(TransitionError::InvalidPose);
        }
        let forward = Vec3::from_array(delta.map(|v| (v / distance) as f32));
        let up = pose
            .up
            .try_normalize()
            .ok_or(TransitionError::InvalidPose)?;
        if forward.cross(up).length() < 1e-4 {
            return Err(TransitionError::InvalidPose);
        }
        // Existing checked view construction and matrix decomposition own basis
        // and quaternion conversion. Remove translation before transposing.
        let view = Mat4::try_look_to_rh(Vec3::ZERO, forward, up)
            .map_err(|_| TransitionError::InvalidPose)?;
        let rotation_matrix =
            Mat4::from_cols(view.x_axis, view.y_axis, view.z_axis, Vec4::W).transpose();
        let (_, rotation, _) = rotation_matrix
            .try_to_scale_rotation_translation()
            .ok_or(TransitionError::InvalidPose)?;
        let mut rotation = rotation
            .try_normalize()
            .ok_or(TransitionError::InvalidPose)?;
        if [rotation.w, rotation.x, rotation.y, rotation.z]
            .into_iter()
            .find(|v| *v != 0.)
            .is_some_and(|v| v < 0.)
        {
            rotation = -rotation;
        }
        Ok(Self {
            pose,
            rotation,
            distance,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Segment {
    start: PreparedPose,
    end: PreparedPose,
    displayed: PreparedPose,
    projection: Perspective3,
    elapsed: f64,
    duration: f64,
}

/// Bounded presentation segment, initially empty. See the module's runnable recipe.
///
/// This is a timed camera blend, not a second tick interpolation/history system.
/// Task202's engine `InterpolationPair` remains the host's optional authoritative
/// tick-to-presentation adapter; this lower-level crate does not depend on engine.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Transition3 {
    segment: Option<Segment>,
    held: bool,
    generation: u64,
}
impl Transition3 {
    /// Empty, unheld, generation zero; equivalent to `Default`. See module recipe.
    pub fn new() -> Self {
        Self::default()
    }

    /// Last displayed pose, or `None` before initialization/after reset.
    pub fn pose(&self) -> Option<Pose3> {
        self.segment.map(|s| s.displayed.pose)
    }

    /// Lens of the displayed pose, or `None` when empty. See module recipe.
    pub fn projection(&self) -> Option<Perspective3> {
        self.segment.map(|s| s.projection)
    }

    /// Whether a positive-duration segment has unfinished work, even while held.
    pub fn is_blending(&self) -> bool {
        self.segment.is_some_and(|s| s.elapsed < s.duration)
    }

    /// Whether valid advances retain the displayed pose and segment time.
    pub fn is_held(&self) -> bool {
        self.held
    }

    /// Local discontinuity counter: initializations, cuts and resets increment it.
    /// Wraps at `u64::MAX`; clones copy it. Not a tick ID or globally unique token.
    /// Ordinary blends, retargets, holds and resumes do not increment it.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Freeze presentation time; returns true only on entering hold. Empty holds
    /// are allowed; initialization/cut releases them. See the module recipe.
    pub fn hold(&mut self) -> bool {
        let changed = !self.held;
        self.held = true;
        changed
    }

    /// Release a hold without consuming elapsed time or changing the pose.
    /// Returns true only on leaving hold. See the module recipe.
    pub fn resume(&mut self) -> bool {
        let changed = self.held;
        self.held = false;
        changed
    }

    /// Forget all pose/segment/lens state, release hold, advance generation.
    /// The next valid retarget initializes exactly. See the module recipe.
    pub fn reset(&mut self) {
        self.segment = None;
        self.held = false;
        self.generation = self.generation.wrapping_add(1);
    }

    /// Immediately replace endpoints and lens, release hold, advance generation.
    /// Validation is atomic. Lens changes are allowed. See the module recipe.
    pub fn cut(&mut self, pose: Pose3, projection: Perspective3) -> Result<(), TransitionError> {
        let prepared = prepare(pose, projection)?;
        self.install_cut(prepared, projection);
        Ok(())
    }

    fn install_cut(&mut self, pose: PreparedPose, projection: Perspective3) {
        self.segment = Some(Segment {
            start: pose,
            end: pose,
            displayed: pose,
            projection,
            elapsed: 0.,
            duration: 0.,
        });
        self.held = false;
        self.generation = self.generation.wrapping_add(1);
    }

    /// Start from the last displayed pose, retaining any hold. No time advances.
    /// Empty state or zero duration initializes/cuts directly, allowing lens change.
    /// Duration is checked even for an empty state or a cut frame.
    ///
    /// ```
    /// use ferravex_math::projection::NdcDepth;
    /// use ferravex_play::camera::{Pose3, transition::*};
    /// let lens = Perspective3 { aspect: 1., near: 0.1, far: 100.,
    ///     handedness: ViewHandedness::Right, depth: NdcDepth::ZeroToOne };
    /// let mut view = Transition3::new();
    /// view.retarget(Pose3::default(), lens, 0.).unwrap();
    /// let before = view;
    /// assert_eq!(view.retarget(Pose3::default(), lens, f32::NAN),
    ///     Err(TransitionError::InvalidDuration));
    /// assert_eq!(view, before);
    /// assert_eq!(view.advance(f32::INFINITY), Err(TransitionError::InvalidDelta));
    /// assert_eq!(view, before);
    /// ```
    pub fn retarget(
        &mut self,
        pose: Pose3,
        projection: Perspective3,
        duration: f32,
    ) -> Result<(), TransitionError> {
        check_duration(duration)?;
        let prepared = prepare(pose, projection)?;
        self.install_target(prepared, projection, duration)
    }

    /// Honor the host's cut flag, borrowing only pose/cut. Other flags/queries are
    /// neither interpolated nor returned. All inputs, including duration, are
    /// validated before mutation. See the actual `Rig3` frame in the module recipe.
    pub fn retarget_frame(
        &mut self,
        frame: &Frame3,
        projection: Perspective3,
        duration: f32,
    ) -> Result<(), TransitionError> {
        check_duration(duration)?;
        let prepared = prepare(frame.pose, projection)?;
        if frame.cut {
            self.install_cut(prepared, projection);
            Ok(())
        } else {
            self.install_target(prepared, projection, duration)
        }
    }

    fn install_target(
        &mut self,
        end: PreparedPose,
        projection: Perspective3,
        duration: f32,
    ) -> Result<(), TransitionError> {
        let Some(segment) = self.segment.as_mut().filter(|_| duration > 0.) else {
            self.install_cut(end, projection);
            return Ok(());
        };
        if segment.projection != projection {
            return Err(TransitionError::IncompatibleProjection);
        }
        segment.start = segment.displayed;
        segment.end = end;
        segment.elapsed = 0.;
        segment.duration = f64::from(duration);
        Ok(())
    }

    /// Advance caller-owned presentation seconds in `0..=0.25`. Valid zero time,
    /// hold, completion or empty state returns the last pose unchanged. Reaching
    /// duration returns the exact supplied endpoint; overshoot is discarded.
    /// Errors leave time and every other field unchanged. See the module recipe.
    pub fn advance(&mut self, delta: f32) -> Result<Option<Pose3>, TransitionError> {
        if !delta.is_finite() || !(0. ..=0.25).contains(&delta) {
            return Err(TransitionError::InvalidDelta);
        }
        if self.held || delta == 0. || !self.is_blending() {
            return Ok(self.pose());
        }
        // Calculate on a copy: numerical reconstruction failure also rolls back time.
        let mut segment = self.segment.unwrap();
        segment.elapsed = (segment.elapsed + f64::from(delta)).min(segment.duration);
        segment.displayed = if segment.elapsed == segment.duration {
            segment.end
        } else {
            blend(
                segment.start,
                segment.end,
                segment.elapsed / segment.duration,
            )?
        };
        self.segment = Some(segment);
        Ok(self.pose())
    }
}

fn check_duration(duration: f32) -> Result<(), TransitionError> {
    if duration.is_finite() && (0. ..=3600.).contains(&duration) {
        Ok(())
    } else {
        Err(TransitionError::InvalidDuration)
    }
}

fn prepare(pose: Pose3, projection: Perspective3) -> Result<PreparedPose, TransitionError> {
    if !projection.aspect.is_finite()
        || projection.aspect <= 0.
        || !projection.near.is_finite()
        || projection.near <= 0.
        || !projection.far.is_finite()
        || projection.far <= projection.near
    {
        return Err(TransitionError::InvalidProjection);
    }
    PreparedPose::new(pose)
}

fn blend(a: PreparedPose, b: PreparedPose, alpha: f64) -> Result<PreparedPose, TransitionError> {
    let lerp = |a: f64, b: f64| a * (1. - alpha) + b * alpha;
    let angle = a
        .rotation
        .try_angle_between(b.rotation)
        .ok_or(TransitionError::NumericalLimit)?;
    let rotation = a
        .rotation
        .try_rotate_towards(b.rotation, (f64::from(angle) * alpha) as f32)
        .ok_or(TransitionError::NumericalLimit)?;
    let position = Vec3::from_array(std::array::from_fn(|i| {
        lerp(f64::from(a.pose.position[i]), f64::from(b.pose.position[i])) as f32
    }));
    let distance = lerp(a.distance, b.distance);
    let forward = rotation * Vec3::NEG_Z;
    let pose = Pose3 {
        position,
        target: Vec3::from_array(std::array::from_fn(|i| {
            (f64::from(position[i]) + f64::from(forward[i]) * distance) as f32
        })),
        up: rotation * Vec3::Y,
        fov_y: lerp(f64::from(a.pose.fov_y), f64::from(b.pose.fov_y)) as f32,
    };
    // Verify the rounded public pose rather than claiming its ideal target exists.
    PreparedPose::new(pose).map_err(|_| TransitionError::NumericalLimit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discontinuity_generation_wraps_on_cut_reset_and_initialization() {
        let lens = Perspective3 {
            aspect: 1.,
            near: 0.1,
            far: 100.,
            handedness: ViewHandedness::Right,
            depth: NdcDepth::ZeroToOne,
        };
        let mut view = Transition3 {
            generation: u64::MAX,
            ..Default::default()
        };
        view.cut(Pose3::default(), lens).unwrap();
        assert_eq!(view.generation(), 0);
        view.generation = u64::MAX;
        view.reset();
        assert_eq!(view.generation(), 0);
        view.generation = u64::MAX;
        view.retarget(Pose3::default(), lens, 1.).unwrap();
        assert_eq!(view.generation(), 0);
    }
}
