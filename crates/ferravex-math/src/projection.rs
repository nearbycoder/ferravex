//! Prepared, viewport-aware 3D projection and finite picking rays.
//!
//! Supply the actual world-to-clip matrix and its conventions explicitly. This
//! module does not infer camera position, handedness, depth range, viewport, or
//! finite/infinite far intent from matrix coefficients. It caches the existing
//! [`Mat4::checked_inverse`]; point transforms reuse [`Mat4::try_project_point3`].
//! Preparation and queries use fixed stack storage and no heap allocations.
//! There is no clock, input, renderer, or physics ownership here.
//!
//! # Coordinates and clipping
//!
//! Screen positions and viewport dimensions share caller-selected units, normally
//! host logical pixels. Apply framebuffer/DPR conversion at the host boundary.
//! X increases right; [`ScreenYAxis`] defines screen Y. Viewport origin always
//! means the numerical minimum corner. NDC X/Y run from -1 to +1. Screen edges
//! map to NDC edges, with **no implicit half-pixel offset**. For an integer pixel
//! index, supply `origin + (index + 0.5)` to sample its center. Both edges are
//! inclusive; a host sharing adjacent rectangles must resolve edge ownership.
//!
//! Depth is returned/accepted as NDC Z, never a linear world distance or a
//! remapped window-depth value. [`NdcDepth`] names physical near and far endpoint
//! values, including reversed depth. All membership tests are exact inclusive
//! comparisons on the computed values, without epsilon or clamping.
//!
//! Matrices must use positive homogeneous W in the supported clip volume (as
//! Ferravex perspective and orthographic constructors do). Projection rejects
//! negative W as [`ProjectionError::BehindCamera`] and zero W as a matrix error.
//! This is a projective half-space test, **not** an inferred eye position: an
//! orthographic matrix has constant W, so behind-eye points are identified only
//! by their out-of-range depth. Projection permits outside XY/depth and reports
//! membership flags, useful for HUD anchors. Unprojection/picking reject outside
//! inputs. No operation promises accuracy for ill-conditioned matrices.
//!
//! # Runnable camera, HUD anchor, and picking recipe
//!
//! This headless first-party recipe uses the same RH GL depth convention as
//! `ferravex-render::Camera3D`. Graphics cameras using `perspective_rh` instead
//! need `NdcDepth::ZeroToOne`. A graphics `ClipDepth` adapter belongs in graphics,
//! never as a dependency of this math module.
//!
//! ```rust
//! use ferravex_math::{Mat4, Vec3, vec2, vec3};
//! use ferravex_math::projection::{
//!     FarPlane, NdcDepth, PreparedProjector3D, ScreenYAxis, Viewport,
//! };
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // A host has already reserved the header and selected this logical viewport.
//!     let viewport = Viewport { origin: vec2(20., 80.), size: vec2(800., 400.) };
//!     let view = Mat4::try_look_at_rh(vec3(0., 0., 5.), Vec3::ZERO, Vec3::Y)?;
//!     let lens = Mat4::try_perspective_rh_gl(1., 2., 0.5, 50.)?;
//!     let mut projector = PreparedProjector3D::try_new(
//!         lens * view, viewport, ScreenYAxis::Down,
//!         NdcDepth::NegativeOneToOne, FarPlane::Finite,
//!     )?;
//!     let anchor = projector.project(Vec3::ZERO)?;
//!     assert!(anchor.inside_viewport && anchor.inside_depth);
//!     assert!(anchor.screen.abs_diff_eq(vec2(420., 280.), 0.001));
//!     let restored = projector.unproject(anchor.screen, anchor.ndc_depth)?;
//!     assert!(restored.abs_diff_eq(Vec3::ZERO, 0.0001));
//!
//!     // A logical pointer uses the same viewport as drawing; no DPR multiplication.
//!     let ray = projector.pick_ray(anchor.screen)?;
//!     assert!(ray.origin.abs_diff_eq(vec3(0., 0., 4.5), 0.0001));
//!     assert!(ray.direction.abs_diff_eq(Vec3::NEG_Z, 0.0001));
//!     // Intersect the z=0 authoring plane. A physics consumer can instead pass
//!     // origin/direction to its own query and cap distance at the far endpoint.
//!     let distance = -ray.origin.z / ray.direction.z;
//!     let hit = ray.origin + ray.direction * distance;
//!     assert!(hit.abs_diff_eq(Vec3::ZERO, 0.0001));
//!     assert!(ray.far_point.z < hit.z);
//!
//!     // Resize/camera updates are transactional. Suspend rendering/input after
//!     // a zero-size error; the retained transform remains usable on restoration.
//!     let before = projector;
//!     let suspended = Viewport { size: vec2(0., 0.), ..viewport };
//!     assert!(projector.try_update(lens * view, suspended, ScreenYAxis::Down,
//!         NdcDepth::NegativeOneToOne, FarPlane::Finite).is_err());
//!     assert_eq!(projector, before);
//!     Ok(())
//! }
//! ```

use crate::{Mat4, MatrixError, Vec2, Vec3, vec2, vec3};

/// Numerical minimum and positive extent in the same units as screen queries.
///
/// Preparation requires each extent to be at least one logical unit. The maximum
/// edge is `origin + size`, rounded to f32; mapping and membership both use those
/// same rounded edges. Nonfinite, overflowing, or collapsed edges are rejected.
/// See the module recipe for construction with an offset viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    /// Minimum X/Y screen coordinate, independent of screen Y direction.
    pub origin: Vec2,
    /// Width/height, each at least one screen unit when prepared.
    pub size: Vec2,
}

/// Direction in which numerical screen Y increases; NDC +Y is always up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScreenYAxis {
    /// NDC +Y maps to the maximum screen Y edge.
    Up,
    /// NDC +Y maps to the minimum screen Y edge (usual logical pixel convention).
    Down,
}

/// Explicit NDC Z endpoints, ordered by physical near-to-far, not numeric value.
///
/// ```
/// use ferravex_math::projection::NdcDepth;
/// assert_eq!(NdcDepth::ReverseZeroToOne.near(), 1.);
/// assert_eq!(NdcDepth::ReverseZeroToOne.far(), 0.);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NdcDepth {
    /// Near -1, far +1 (RH GL constructors and render Camera3D).
    NegativeOneToOne,
    /// Near 0, far +1 (RH/LH non-GL constructors and graphics Camera).
    ZeroToOne,
    /// Near +1, far -1; supply a matrix authored for reversed GL depth.
    ReverseNegativeOneToOne,
    /// Near +1, far 0; supply a matrix authored for reversed zero-to-one depth.
    ReverseZeroToOne,
}

impl NdcDepth {
    /// Physical near plane's NDC Z. See the enum example.
    pub const fn near(self) -> f32 {
        match self {
            Self::NegativeOneToOne => -1.,
            Self::ZeroToOne => 0.,
            Self::ReverseNegativeOneToOne | Self::ReverseZeroToOne => 1.,
        }
    }

    /// Physical far plane's NDC Z. See the enum example.
    pub const fn far(self) -> f32 {
        match self {
            Self::NegativeOneToOne | Self::ZeroToOne => 1.,
            Self::ReverseNegativeOneToOne => -1.,
            Self::ReverseZeroToOne => 0.,
        }
    }

    fn contains(self, z: f32) -> bool {
        z >= self.near().min(self.far()) && z <= self.near().max(self.far())
    }
}

/// Caller declaration of far-plane intent; it cannot be recovered reliably from
/// rounded, composed matrix coefficients. Infinite far is explicitly unsupported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FarPlane {
    /// Caller authored a finite far plane; preparation also checks the clip volume.
    Finite,
    /// Always rejected, even if coefficient rounding happens to yield finite points.
    Infinite,
}

/// Checked projection/preparation failure, with no fallback camera or point.
///
/// ```
/// use ferravex_math::projection::ProjectionError;
/// let error: &dyn std::error::Error = &ProjectionError::OutsideViewport;
/// assert!(error.to_string().contains("viewport"));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProjectionError {
    /// Existing checked matrix diagnostic (including singular, nonfinite, zero W,
    /// or an unrepresentable projected coordinate).
    Matrix(MatrixError),
    /// Nonfinite viewport, extent below one, or overflowing/collapsed f32 edges.
    InvalidViewport,
    /// The caller declared an infinite far plane, which this API does not support.
    UnsupportedInfiniteFar,
    /// A corner has nonpositive inverse W. The declared closed clip volume
    /// touches/crosses infinity or lies outside the supported positive-W half-space.
    InvalidClipVolume,
    /// Projected point has negative homogeneous W.
    BehindCamera,
    /// Screen input contains NaN or infinity (checked before membership).
    NonFiniteScreen,
    /// NDC depth input is NaN or infinity (checked before membership).
    NonFiniteDepth,
    /// Screen input is outside the inclusive viewport.
    OutsideViewport,
    /// Finite NDC depth input is outside the inclusive declared depth interval.
    OutsideDepth,
    /// A screen output cannot round to finite f32.
    ResultOutOfRange,
    /// Rounded near/far world endpoints coincide; no direction can be normalized.
    DegenerateRay,
}

impl std::fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Matrix(error) => write!(f, "projection matrix operation failed: {error}"),
            Self::InvalidViewport => f.write_str(
                "viewport needs finite, distinct edges and extents of at least one screen unit",
            ),
            Self::UnsupportedInfiniteFar => f.write_str(
                "infinite far projection is unsupported; supply an authored finite-far camera",
            ),
            Self::InvalidClipVolume => f.write_str(
                "clip volume must have finite endpoints and positive inverse homogeneous W",
            ),
            Self::BehindCamera => f.write_str("point is behind the positive-W camera half-space"),
            Self::NonFiniteScreen => f.write_str("screen position must be finite"),
            Self::NonFiniteDepth => f.write_str("NDC depth must be finite"),
            Self::OutsideViewport => {
                f.write_str("screen position is outside the inclusive viewport")
            }
            Self::OutsideDepth => {
                f.write_str("NDC depth is outside the declared near/far interval")
            }
            Self::ResultOutOfRange => f.write_str("screen result exceeds finite f32 range"),
            Self::DegenerateRay => {
                f.write_str("near and far endpoints coincide after f32 rounding")
            }
        }
    }
}

impl std::error::Error for ProjectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Matrix(error) => Some(error),
            _ => None,
        }
    }
}

impl From<MatrixError> for ProjectionError {
    fn from(error: MatrixError) -> Self {
        Self::Matrix(error)
    }
}

/// A finite positive-W projection, including unclamped clip membership.
/// See the module's HUD recipe. Membership is not occlusion testing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProjectedPoint {
    /// Absolute screen coordinates, possibly outside the viewport.
    pub screen: Vec2,
    /// Unmodified NDC Z, possibly outside the declared near/far interval.
    pub ndc_depth: f32,
    /// True when computed NDC X/Y are both within inclusive -1..=1.
    pub inside_viewport: bool,
    /// True when computed NDC Z is inside the declared inclusive interval.
    pub inside_depth: bool,
}

/// Picking ray with explicit finite clipping endpoints. See the module recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickingRay3D {
    /// World point on the physical near plane, **not** an inferred camera eye.
    pub origin: Vec3,
    /// Unit world direction from near toward far (subject to f32 rounding).
    /// `origin + direction * t` uses world-distance t >= 0.
    pub direction: Vec3,
    /// World point on the physical finite far plane. Consumers enforcing camera
    /// clipping should limit their query to this endpoint; the ray has no implicit cap.
    pub far_point: Vec3,
}

/// Validated world-to-clip transform and cached clip-to-world inverse.
///
/// Perspective and orthographic finite-far matrices are supported, including
/// reversed depth and either handedness. Preparation checks all eight NDC box
/// corners for positive inverse W and representable world coordinates. Because
/// inverse W is affine, positive corner values exclude infinity throughout the
/// closed box in the cached transform. This cannot certify original camera
/// intent after matrix rounding: callers must declare [`FarPlane`] truthfully.
///
/// The inverse has f32 coefficients and the same conditioning/underflow limits
/// as [`Mat4::checked_inverse`]. Degenerate rays after endpoint rounding remain
/// query errors. No implicit matrix normalization changes the W-sign policy.
/// See the complete module recipe for construction, queries and failed updates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedProjector3D {
    view_projection: Mat4,
    inverse: Mat4,
    viewport: Viewport,
    end: Vec2,
    y_axis: ScreenYAxis,
    depth: NdcDepth,
}

impl PreparedProjector3D {
    /// Prepares a finite-far camera. See the module recipe.
    ///
    /// Validation order: viewport, explicit far intent, checked inverse, then
    /// clip-volume corners. No inverse is recomputed during point/ray queries.
    pub fn try_new(
        view_projection: Mat4,
        viewport: Viewport,
        y_axis: ScreenYAxis,
        depth: NdcDepth,
        far_plane: FarPlane,
    ) -> Result<Self, ProjectionError> {
        if !viewport.origin.is_finite()
            || !viewport.size.is_finite()
            || viewport.size.x < 1.
            || viewport.size.y < 1.
        {
            return Err(ProjectionError::InvalidViewport);
        }
        let end = viewport.origin + viewport.size;
        if !end.is_finite() || end.x <= viewport.origin.x || end.y <= viewport.origin.y {
            return Err(ProjectionError::InvalidViewport);
        }
        if far_plane == FarPlane::Infinite {
            return Err(ProjectionError::UnsupportedInfiniteFar);
        }
        let inverse = view_projection.checked_inverse()?;
        for z in [depth.near(), depth.far()] {
            for y in [-1., 1.] {
                for x in [-1., 1.] {
                    let ndc = vec3(x, y, z);
                    if homogeneous_w(inverse, ndc) <= 0. {
                        return Err(ProjectionError::InvalidClipVolume);
                    }
                    inverse.try_project_point3(ndc)?;
                }
            }
        }
        Ok(Self {
            view_projection,
            inverse,
            viewport,
            end,
            y_axis,
            depth,
        })
    }

    /// Replaces all prepared state only after successful validation. Any error
    /// retains the prior camera, inverse and viewport; see the module recipe.
    pub fn try_update(
        &mut self,
        view_projection: Mat4,
        viewport: Viewport,
        y_axis: ScreenYAxis,
        depth: NdcDepth,
        far_plane: FarPlane,
    ) -> Result<(), ProjectionError> {
        *self = Self::try_new(view_projection, viewport, y_axis, depth, far_plane)?;
        Ok(())
    }

    /// Projects a world point without clipping XY/depth. Returns separate
    /// membership flags; rejects nonfinite points, zero/negative W, and range
    /// failures. Tiny positive W is allowed with no arbitrary epsilon.
    /// Finite negative-W points return `BehindCamera` before perspective division;
    /// nonfinite inputs and zero W retain their checked matrix diagnostics.
    /// See the module HUD recipe.
    pub fn project(&self, world: Vec3) -> Result<ProjectedPoint, ProjectionError> {
        if world.is_finite() && homogeneous_w(self.view_projection, world) < 0. {
            return Err(ProjectionError::BehindCamera);
        }
        let ndc = self.view_projection.try_project_point3(world)?;
        let x = (f64::from(ndc.x) + 1.) * 0.5;
        let y = match self.y_axis {
            ScreenYAxis::Up => (f64::from(ndc.y) + 1.) * 0.5,
            ScreenYAxis::Down => (1. - f64::from(ndc.y)) * 0.5,
        };
        let screen = vec2(
            screen_f32(f64::from(self.viewport.origin.x) + x * self.span_x())?,
            screen_f32(f64::from(self.viewport.origin.y) + y * self.span_y())?,
        );
        Ok(ProjectedPoint {
            screen,
            ndc_depth: ndc.z,
            inside_viewport: ndc.x >= -1. && ndc.x <= 1. && ndc.y >= -1. && ndc.y <= 1.,
            inside_depth: self.depth.contains(ndc.z),
        })
    }

    /// Unprojects an inclusive viewport position and inclusive NDC depth through
    /// the cached inverse. Rejects nonfinite inputs, then outside XY, then outside
    /// depth. No extrapolation/clamping or window-depth conversion is performed.
    /// See the module round-trip recipe.
    pub fn unproject(&self, screen: Vec2, ndc_depth: f32) -> Result<Vec3, ProjectionError> {
        if !screen.is_finite() {
            return Err(ProjectionError::NonFiniteScreen);
        }
        if !ndc_depth.is_finite() {
            return Err(ProjectionError::NonFiniteDepth);
        }
        if screen.x < self.viewport.origin.x
            || screen.y < self.viewport.origin.y
            || screen.x > self.end.x
            || screen.y > self.end.y
        {
            return Err(ProjectionError::OutsideViewport);
        }
        if !self.depth.contains(ndc_depth) {
            return Err(ProjectionError::OutsideDepth);
        }
        let x = 2. * (f64::from(screen.x) - f64::from(self.viewport.origin.x)) / self.span_x() - 1.;
        let y = 2. * (f64::from(screen.y) - f64::from(self.viewport.origin.y)) / self.span_y() - 1.;
        let y = if self.y_axis == ScreenYAxis::Down {
            -y
        } else {
            y
        };
        Ok(self
            .inverse
            .try_project_point3(vec3(x as f32, y as f32, ndc_depth))?)
    }

    /// Near-plane-origin ray toward the physical finite far endpoint. Orthographic
    /// rays are parallel with distinct origins; perspective rays diverge from
    /// near-plane origins. Never guesses an eye or substitutes a finite far value.
    /// Endpoint subtraction/normalization uses f64 so extreme finite endpoints
    /// need not have a representable f32 difference. See the module picking recipe.
    pub fn pick_ray(&self, screen: Vec2) -> Result<PickingRay3D, ProjectionError> {
        let origin = self.unproject(screen, self.depth.near())?;
        let far_point = self.unproject(screen, self.depth.far())?;
        let delta = [
            f64::from(far_point.x) - f64::from(origin.x),
            f64::from(far_point.y) - f64::from(origin.y),
            f64::from(far_point.z) - f64::from(origin.z),
        ];
        let length = delta.into_iter().map(|v| v * v).sum::<f64>().sqrt();
        if length == 0. {
            return Err(ProjectionError::DegenerateRay);
        }
        let direction = Vec3::from_array(delta.map(|v| (v / length) as f32));
        Ok(PickingRay3D {
            origin,
            direction,
            far_point,
        })
    }

    fn span_x(&self) -> f64 {
        f64::from(self.end.x) - f64::from(self.viewport.origin.x)
    }

    fn span_y(&self) -> f64 {
        f64::from(self.end.y) - f64::from(self.viewport.origin.y)
    }
}

fn homogeneous_w(matrix: Mat4, point: Vec3) -> f64 {
    matrix
        .row(3)
        .to_array()
        .into_iter()
        .zip(point.extend(1.).to_array())
        .map(|(a, b)| f64::from(a) * f64::from(b))
        .sum()
}

fn screen_f32(value: f64) -> Result<f32, ProjectionError> {
    let value = value as f32;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ProjectionError::ResultOutOfRange)
    }
}
