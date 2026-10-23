//! Checked, reusable affine point, vector and normal transformation.
//!
//! [`PreparedAffine3D`] accepts an existing [`Mat4`]; it does not author another
//! TRS/scene hierarchy. Preparation checks finite coefficients, the exact affine
//! row `(0, 0, 0, 1)` and a representable inverse of the linear part. Translation
//! is excluded from inversion, so an unrepresentable inverse translation does
//! not prevent forward transformation. No projective division is performed.
//!
//! Normals use the inverse transpose, including for nonuniform scale and shear.
//! Reflections are accepted. Normals retain their covector orientation: there
//! is **no** extra determinant-sign flip. For a mirrored model, a transformed
//! triangle's edge cross product points opposite its transformed authored
//! normal. Use [`PreparedAffine3D::is_reflected`] to choose a winding/culling
//! policy, or explicitly negate the returned normal when matching that cross
//! product is intended. The cached normal matrix follows the same policy.
//!
//! Scalar operations reject nonfinite inputs. Points/vectors use f64 dot
//! products and reject outputs that round to nonfinite f32. Normals rescale
//! wide dot products before using [`Vec3::try_normalize`], allowing a unit
//! result even when an unnormalized component would overflow/underflow f32.
//! A zero input or a transformed normal that collapses to zero is an error.
//! Ordinary f32 rounding and the conditioning limits of
//! [`Mat4::checked_inverse`] remain; success is not an accuracy certificate.
//!
//! Slice operations require enough caller storage, process the entire input
//! (never truncate), and leave **all** output elements unchanged on any error.
//! On success only the input-length prefix is written. They use two passes,
//! O(n) work, O(1) stack storage and no heap allocations. Callers impose their
//! own input/work bound. Separate calls are separate transactions.
//!
//! # Runnable mesh preparation recipe
//!
//! Prepare once when an authored model changes, then reuse for mesh positions,
//! directions and lighting normals. This recipe uses a mirrored, sheared model
//! and caller-owned output arrays; it does not replace a renderer.
//!
//! ```rust
//! use ferravex_math::{Mat4, Vec3, Vec4, vec3, vec4};
//! use ferravex_math::affine::PreparedAffine3D;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let model = Mat4::from_cols(
//!         vec4(-2.0, 0.0, 0.0, 0.0),
//!         vec4(1.0, 3.0, 0.0, 0.0),
//!         vec4(0.0, 0.0, 0.5, 0.0),
//!         vec3(10.0, 2.0, -1.0).extend(1.0),
//!     );
//!     let mut prepared = PreparedAffine3D::try_new(model)?;
//!     assert_eq!(prepared.model(), model);
//!     assert!(prepared.is_reflected()); // renderer chooses winding explicitly
//!     let local_positions = [Vec3::ZERO, Vec3::X, Vec3::Y];
//!     let mut world_positions = [Vec3::ZERO; 3];
//!     let mut world_normals = [Vec3::ZERO; 3];
//!     prepared.transform_points_into(&local_positions, &mut world_positions)?;
//!     prepared.transform_normals_into(&[Vec3::Z; 3], &mut world_normals)?;
//!     let edge = prepared.transform_vector(Vec3::X)?;
//!     assert!(edge.dot(world_normals[0]).abs() < 1.0e-6);
//!     let winding_normal = (world_positions[1] - world_positions[0])
//!         .cross(world_positions[2] - world_positions[0]).normalize();
//!     assert!(winding_normal.dot(world_normals[0]) < -0.999);
//!     assert_eq!(prepared.normal_matrix().transform_vector3(Vec3::Z), Vec3::Z * 2.0);
//!     assert_eq!(prepared.transform_point(Vec3::ZERO)?, model.w_axis.truncate());
//!     let mut directions = [Vec3::ZERO];
//!     prepared.transform_vectors_into(&[Vec3::X], &mut directions)?;
//!     assert_eq!(directions[0], edge);
//!     let retained = prepared;
//!     assert!(prepared.try_update(Mat4::from_scale(Vec3::ZERO)).is_err());
//!     assert_eq!(prepared, retained); // failed edits retain the last good cache
//!     prepared.try_update(Mat4::IDENTITY)?;
//!     assert!(!prepared.is_reflected());
//!     assert_eq!(prepared.normal_matrix(), Mat4::IDENTITY);
//!     assert_eq!(prepared.transform_normal(Vec3::Z)?, Vec3::Z);
//!     assert_eq!(prepared.model().w_axis, Vec4::W);
//!     Ok(())
//! }
//! ```

use crate::{Mat4, MatrixError, Vec3, Vec4};

/// Failure from preparation or one scalar affine operation.
/// No identity, zero normal or other fallback is substituted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AffineError {
    /// A model coefficient, point, vector or normal is NaN/infinite.
    NonFiniteInput { input: &'static str },
    /// The bottom row must equal `(0, 0, 0, 1)` exactly; signed zero is accepted.
    NonAffine,
    /// Checked inversion of the translation-free linear matrix failed.
    Matrix(MatrixError),
    /// A point/vector output cannot round to finite f32.
    ResultOutOfRange,
    /// An input normal is zero, or its cached inverse-transpose image is zero.
    DegenerateNormal,
}

impl std::fmt::Display for AffineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteInput { input } => write!(f, "affine {input} must be finite"),
            Self::NonAffine => f.write_str("model bottom row must equal (0, 0, 0, 1)"),
            Self::Matrix(error) => write!(f, "affine linear inverse: {error}"),
            Self::ResultOutOfRange => f.write_str("affine output exceeds finite f32 range"),
            Self::DegenerateNormal => {
                f.write_str("normal must have a nonzero transformed direction")
            }
        }
    }
}

impl std::error::Error for AffineError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Matrix(error) => Some(error),
            _ => None,
        }
    }
}

impl From<MatrixError> for AffineError {
    fn from(value: MatrixError) -> Self {
        Self::Matrix(value)
    }
}

/// An atomic slice operation failed before writing any output.
/// Capacity is checked first; otherwise the first failing input index wins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AffineSliceError {
    /// Output must hold the entire input, not a truncated prefix.
    InsufficientCapacity { required: usize, available: usize },
    /// A zero-based element failed scalar validation.
    Element { index: usize, source: AffineError },
}

impl std::fmt::Display for AffineSliceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InsufficientCapacity {
                required,
                available,
            } => {
                write!(f, "affine output needs {required} slots, has {available}")
            }
            Self::Element { index, source } => write!(f, "affine input[{index}]: {source}"),
        }
    }
}

impl std::error::Error for AffineSliceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Element { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Validated immutable cache of an affine model, linear inverse transpose and
/// orientation. All accessors return copies, preserving cache invariants.
///
/// Use the [module recipe](self) for scalar, bounded batch and update examples.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreparedAffine3D {
    model: Mat4,
    normal_matrix: Mat4,
    reflected: bool,
}

impl PreparedAffine3D {
    /// Checks finiteness, exact affine row, then the linear checked inverse.
    /// Singular models and inverses outside f32 range are rejected. There is no
    /// epsilon cutoff or implicit repair of a nearly affine row.
    pub fn try_new(model: Mat4) -> Result<Self, AffineError> {
        if !model.is_finite() {
            return Err(AffineError::NonFiniteInput { input: "model" });
        }
        if model.row(3) != Vec4::W {
            return Err(AffineError::NonAffine);
        }
        let linear = Mat4::from_cols(model.x_axis, model.y_axis, model.z_axis, Vec4::W);
        let normal_matrix = linear.checked_inverse()?.transpose();
        // determinant() uses f64 elimination before its f32 cast. Signed zero
        // preserves orientation when magnitude underflows; +/-infinity also
        // preserves sign when magnitude overflows. No second inverse is needed.
        let reflected = linear.determinant().is_sign_negative();
        Ok(Self {
            model,
            normal_matrix,
            reflected,
        })
    }

    /// Atomically replaces all cached state. Failure leaves `self` unchanged.
    /// Updating to identity resets the transform; there is no hidden history.
    pub fn try_update(&mut self, model: Mat4) -> Result<(), AffineError> {
        *self = Self::try_new(model)?;
        Ok(())
    }

    /// The exact finite affine model supplied at the last successful preparation.
    pub const fn model(&self) -> Mat4 {
        self.model
    }

    /// Linear inverse transpose in a Mat4 with zero translation and affine row.
    /// Its `transform_vector3` gives an **unnormalized**, unchecked normal.
    /// Prefer [`Self::transform_normal`] for checked, normalized results.
    pub const fn normal_matrix(&self) -> Mat4 {
        self.normal_matrix
    }

    /// Whether the computed linear determinant is negative (a mirror).
    /// This uses its sign even when f32 magnitude overflows or underflows.
    /// Like the checked inverse, it is not an exact-arithmetic rank or
    /// conditioning certificate for nearly dependent axes.
    pub const fn is_reflected(&self) -> bool {
        self.reflected
    }

    /// Transforms a finite position with translation, without projective divide.
    /// Returns a finite f32 result or an error; coordinate underflow is allowed.
    pub fn transform_point(&self, point: Vec3) -> Result<Vec3, AffineError> {
        finite_vector(point, "point")?;
        finite_output(transform_wide(self.model, point, true))
    }

    /// Transforms a finite direction/displacement without translation.
    /// Zero vectors are valid. Does not normalize or preserve length.
    pub fn transform_vector(&self, vector: Vec3) -> Result<Vec3, AffineError> {
        finite_vector(vector, "vector")?;
        finite_output(transform_wide(self.model, vector, false))
    }

    /// Returns a unit inverse-transpose normal, with no determinant-sign flip.
    /// Nonunit finite inputs are allowed; zero/nonfinite normals fail.
    /// Wide rescaling avoids intermediate f32 overflow/underflow. Component
    /// quantization and rounded inverse coefficients can still affect direction.
    pub fn transform_normal(&self, normal: Vec3) -> Result<Vec3, AffineError> {
        finite_vector(normal, "normal")?;
        let wide = transform_wide(self.normal_matrix, normal, false);
        let scale = wide.into_iter().map(f64::abs).fold(0.0, f64::max);
        if scale == 0.0 {
            return Err(AffineError::DegenerateNormal);
        }
        Vec3::from_array(wide.map(|v| (v / scale) as f32))
            .try_normalize()
            .ok_or(AffineError::DegenerateNormal)
    }

    /// Transforms every input point into the output prefix, atomically.
    /// Empty input succeeds; unused output tail is retained. See module bounds.
    pub fn transform_points_into(
        &self,
        input: &[Vec3],
        output: &mut [Vec3],
    ) -> Result<(), AffineSliceError> {
        transform_slice(input, output, |v| self.transform_point(v))
    }

    /// Transforms every input vector into the output prefix, atomically.
    /// Zero vectors are accepted; capacity/element errors retain the whole output.
    pub fn transform_vectors_into(
        &self,
        input: &[Vec3],
        output: &mut [Vec3],
    ) -> Result<(), AffineSliceError> {
        transform_slice(input, output, |v| self.transform_vector(v))
    }

    /// Transforms and normalizes every normal into the output prefix, atomically.
    /// A zero/nonfinite/collapsed normal fails with its input index. No fallback
    /// normal is inserted, and even earlier valid elements are left unwritten.
    pub fn transform_normals_into(
        &self,
        input: &[Vec3],
        output: &mut [Vec3],
    ) -> Result<(), AffineSliceError> {
        transform_slice(input, output, |v| self.transform_normal(v))
    }
}

fn finite_vector(value: Vec3, input: &'static str) -> Result<(), AffineError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(AffineError::NonFiniteInput { input })
    }
}

fn transform_wide(matrix: Mat4, value: Vec3, translate: bool) -> [f64; 3] {
    std::array::from_fn(|row| {
        let linear = f64::from(matrix.x_axis[row]) * f64::from(value.x)
            + f64::from(matrix.y_axis[row]) * f64::from(value.y)
            + f64::from(matrix.z_axis[row]) * f64::from(value.z);
        linear
            + if translate {
                f64::from(matrix.w_axis[row])
            } else {
                0.0
            }
    })
}

fn finite_output(value: [f64; 3]) -> Result<Vec3, AffineError> {
    let result = Vec3::from_array(value.map(|v| v as f32));
    if result.is_finite() {
        Ok(result)
    } else {
        Err(AffineError::ResultOutOfRange)
    }
}

// Private callers supply only pure operations over immutable prepared state.
// No caller callback, mutable input or external state can change between passes.
fn transform_slice(
    input: &[Vec3],
    output: &mut [Vec3],
    transform: impl Fn(Vec3) -> Result<Vec3, AffineError>,
) -> Result<(), AffineSliceError> {
    if output.len() < input.len() {
        return Err(AffineSliceError::InsufficientCapacity {
            required: input.len(),
            available: output.len(),
        });
    }
    for (index, &value) in input.iter().enumerate() {
        transform(value).map_err(|source| AffineSliceError::Element { index, source })?;
    }
    for (&value, slot) in input.iter().zip(output) {
        *slot = transform(value).expect("immutable affine operation already validated");
    }
    Ok(())
}
