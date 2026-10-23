use crate::{Quat, Vec3, Vec4, vec3, vec4};
use std::ops::{Add, Index, IndexMut, Mul, MulAssign, Sub};

/// Failure from a checked matrix operation. No fallback transform is substituted.
///
/// ```
/// use ferravex_math::{Mat4, MatrixError};
/// assert_eq!(Mat4::ZERO.checked_inverse(), Err(MatrixError::Singular { column: 0 }));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MatrixError {
    /// The named input contains NaN or infinity.
    NonFiniteInput { input: &'static str },
    /// Elimination encountered an exactly zero pivot at this zero-based column.
    Singular { column: usize },
    /// An intermediate/output is non-finite, or a required projection coefficient
    /// rounds to zero in f32. Rescale the inputs or use a less extreme frustum.
    ResultOutOfRange,
    /// The transformed homogeneous w is exactly zero: this point is at infinity.
    ZeroHomogeneousW,
    /// Perspective requires `0 < fov_y < PI`, `aspect > 0`, `near > 0`,
    /// and (for a finite frustum) `far > near`.
    InvalidPerspective { parameter: &'static str },
    /// The two orthographic bounds on this axis are equal.
    DegenerateOrthographic { axis: &'static str },
    /// The direction is zero, or the eye and target coincide.
    DegenerateDirection,
    /// Up is zero or parallel to the viewing direction; choose a different up.
    DegenerateUp,
}

impl std::fmt::Display for MatrixError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteInput { input } => write!(f, "{input} must contain only finite values"),
            Self::Singular { column } => write!(f, "matrix has a zero pivot at column {column}; remove dependent or zero axes"),
            Self::ResultOutOfRange => f.write_str("matrix operation exceeds usable f32 range; rescale inputs or reduce frustum extremes"),
            Self::ZeroHomogeneousW => f.write_str("projected point has zero homogeneous w; choose a point away from the projection plane"),
            Self::InvalidPerspective { parameter } => write!(f, "invalid perspective {parameter}; require 0 < fov_y < PI, aspect > 0, near > 0, and far > near when supplied"),
            Self::DegenerateOrthographic { axis } => write!(f, "orthographic {axis} bounds must differ"),
            Self::DegenerateDirection => f.write_str("camera direction must be nonzero; eye and target must differ"),
            Self::DegenerateUp => f.write_str("camera up must be nonzero and nonparallel to its direction"),
        }
    }
}

impl std::error::Error for MatrixError {}

/// Column-major 4x4 matrix acting on column vectors: `translation * rotation * scale`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4 {
    pub x_axis: Vec4,
    pub y_axis: Vec4,
    pub z_axis: Vec4,
    pub w_axis: Vec4,
}
impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl Mat4 {
    pub const IDENTITY: Self = Self::from_cols(Vec4::X, Vec4::Y, Vec4::Z, Vec4::W);
    pub const ZERO: Self = Self::from_cols(Vec4::ZERO, Vec4::ZERO, Vec4::ZERO, Vec4::ZERO);
    pub const NAN: Self = Self::from_cols(Vec4::NAN, Vec4::NAN, Vec4::NAN, Vec4::NAN);
    pub const fn from_cols(x_axis: Vec4, y_axis: Vec4, z_axis: Vec4, w_axis: Vec4) -> Self {
        Self {
            x_axis,
            y_axis,
            z_axis,
            w_axis,
        }
    }
    pub const fn from_cols_array(v: &[f32; 16]) -> Self {
        Self::from_cols(
            vec4(v[0], v[1], v[2], v[3]),
            vec4(v[4], v[5], v[6], v[7]),
            vec4(v[8], v[9], v[10], v[11]),
            vec4(v[12], v[13], v[14], v[15]),
        )
    }
    pub const fn from_cols_array_2d(v: &[[f32; 4]; 4]) -> Self {
        Self::from_cols(
            Vec4::from_array(v[0]),
            Vec4::from_array(v[1]),
            Vec4::from_array(v[2]),
            Vec4::from_array(v[3]),
        )
    }
    pub const fn to_cols_array(self) -> [f32; 16] {
        let a = self.to_cols_array_2d();
        [
            a[0][0], a[0][1], a[0][2], a[0][3], a[1][0], a[1][1], a[1][2], a[1][3], a[2][0],
            a[2][1], a[2][2], a[2][3], a[3][0], a[3][1], a[3][2], a[3][3],
        ]
    }
    pub const fn to_cols_array_2d(self) -> [[f32; 4]; 4] {
        [
            self.x_axis.to_array(),
            self.y_axis.to_array(),
            self.z_axis.to_array(),
            self.w_axis.to_array(),
        ]
    }
    pub fn col(self, index: usize) -> Vec4 {
        self[index]
    }
    pub fn row(self, index: usize) -> Vec4 {
        vec4(
            self.x_axis[index],
            self.y_axis[index],
            self.z_axis[index],
            self.w_axis[index],
        )
    }
    pub fn transpose(self) -> Self {
        Self::from_cols(self.row(0), self.row(1), self.row(2), self.row(3))
    }
    pub const fn from_translation(v: Vec3) -> Self {
        Self::from_cols(Vec4::X, Vec4::Y, Vec4::Z, v.extend(1.0))
    }
    pub const fn from_scale(v: Vec3) -> Self {
        Self::from_diagonal(vec4(v.x, v.y, v.z, 1.0))
    }
    pub const fn from_diagonal(v: Vec4) -> Self {
        Self::from_cols(
            vec4(v.x, 0.0, 0.0, 0.0),
            vec4(0.0, v.y, 0.0, 0.0),
            vec4(0.0, 0.0, v.z, 0.0),
            vec4(0.0, 0.0, 0.0, v.w),
        )
    }
    /// The quaternion is normalized to avoid embedding accidental scale.
    pub fn from_quat(rotation: Quat) -> Self {
        let q = rotation.normalize();
        Self::from_cols(
            (q * Vec3::X).extend(0.0),
            (q * Vec3::Y).extend(0.0),
            (q * Vec3::Z).extend(0.0),
            Vec4::W,
        )
    }
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Self {
        Self::from_quat(Quat::from_axis_angle(axis, angle))
    }
    pub fn from_rotation_x(angle: f32) -> Self {
        Self::from_quat(Quat::from_rotation_x(angle))
    }
    pub fn from_rotation_y(angle: f32) -> Self {
        Self::from_quat(Quat::from_rotation_y(angle))
    }
    pub fn from_rotation_z(angle: f32) -> Self {
        Self::from_quat(Quat::from_rotation_z(angle))
    }
    pub fn from_rotation_translation(rotation: Quat, translation: Vec3) -> Self {
        Self::from_scale_rotation_translation(Vec3::ONE, rotation, translation)
    }
    pub fn from_scale_rotation_translation(scale: Vec3, rotation: Quat, translation: Vec3) -> Self {
        let mut result = Self::from_quat(rotation);
        result.x_axis *= scale.x;
        result.y_axis *= scale.y;
        result.z_axis *= scale.z;
        result.w_axis = translation.extend(1.0);
        result
    }
    pub fn transform_point3(self, v: Vec3) -> Vec3 {
        (self * v.extend(1.0)).truncate()
    }
    pub fn transform_vector3(self, v: Vec3) -> Vec3 {
        (self * v.extend(0.0)).truncate()
    }
    pub fn project_point3(self, v: Vec3) -> Vec3 {
        let result = self * v.extend(1.0);
        result.truncate() / result.w
    }
    /// Projects a finite point with f64 dot products and perspective division.
    ///
    /// Rejects a non-finite matrix or point with [`MatrixError::NonFiniteInput`].
    /// Returns [`MatrixError::ZeroHomogeneousW`] for exactly zero w, and
    /// [`MatrixError::ResultOutOfRange`] if a divided coordinate cannot round to
    /// finite f32. There is no absolute epsilon: tiny nonzero w is allowed when
    /// the divided result fits. Negative w and points outside the clip volume
    /// are allowed; this method does not test visibility or require an inverse.
    /// Output rounding may underflow coordinates to zero.
    ///
    /// An inverse matrix projects normalized device coordinates back to world
    /// space (subject to floating-point error):
    /// ```
    /// use ferravex_math::{Mat4, MatrixError, vec3};
    /// let projection = Mat4::try_perspective_rh(1.0, 1.5, 0.1, 100.0)?;
    /// let point = vec3(0.2, -0.1, -2.0);
    /// let ndc = projection.try_project_point3(point)?;
    /// let restored = projection.checked_inverse()?.try_project_point3(ndc)?;
    /// assert!(restored.abs_diff_eq(point, 1.0e-5));
    /// # Ok::<(), MatrixError>(())
    /// ```
    pub fn try_project_point3(self, v: Vec3) -> Result<Vec3, MatrixError> {
        finite_input("matrix", &self.to_cols_array())?;
        finite_input("point", &v.to_array())?;
        let point = v.extend(1.0).to_array().map(f64::from);
        let projected: [f64; 4] = std::array::from_fn(|row| {
            self.row(row)
                .to_array()
                .into_iter()
                .zip(point)
                .map(|(a, b)| f64::from(a) * b)
                .sum()
        });
        if projected[3] == 0.0 {
            return Err(MatrixError::ZeroHomogeneousW);
        }
        Ok(vec3(
            finite_f32(projected[0] / projected[3])?,
            finite_f32(projected[1] / projected[3])?,
            finite_f32(projected[2] / projected[3])?,
        ))
    }
    pub fn mul_vec4(self, v: Vec4) -> Vec4 {
        self.x_axis * v.x + self.y_axis * v.y + self.z_axis * v.z + self.w_axis * v.w
    }
    pub fn mul_mat4(self, rhs: Self) -> Self {
        self * rhs
    }
    pub fn is_finite(self) -> bool {
        self.x_axis.is_finite()
            && self.y_axis.is_finite()
            && self.z_axis.is_finite()
            && self.w_axis.is_finite()
    }
    pub fn is_nan(self) -> bool {
        self.x_axis.is_nan() || self.y_axis.is_nan() || self.z_axis.is_nan() || self.w_axis.is_nan()
    }
    pub fn abs_diff_eq(self, rhs: Self, tolerance: f32) -> bool {
        (0..4).all(|i| self[i].abs_diff_eq(rhs[i], tolerance))
    }
    /// Partial-pivot Gaussian elimination with f64 intermediates.
    pub fn determinant(self) -> f32 {
        let mut rows = std::array::from_fn::<_, 4, _>(|r| self.row(r).to_array().map(f64::from));
        let mut determinant = 1.0_f64;
        for column in 0..4 {
            let pivot = (column..4)
                .max_by(|&a, &b| rows[a][column].abs().total_cmp(&rows[b][column].abs()))
                .unwrap();
            if rows[pivot][column] == 0.0 {
                return 0.0;
            }
            if pivot != column {
                rows.swap(pivot, column);
                determinant = -determinant;
            }
            let divisor = rows[column][column];
            determinant *= divisor;
            let pivot_row = rows[column];
            for row in rows.iter_mut().skip(column + 1) {
                let ratio = row[column] / divisor;
                for (value, pivot_value) in row.iter_mut().zip(pivot_row).skip(column + 1) {
                    *value -= ratio * pivot_value;
                }
            }
        }
        determinant as f32
    }
    /// Returns None for singular/non-finite matrices or inverses outside f32 range.
    /// See [`Self::checked_inverse`] for error details and precision limits.
    pub fn try_inverse(self) -> Option<Self> {
        self.checked_inverse().ok()
    }
    /// General partial-pivot Gauss-Jordan inverse with diagnostic errors.
    ///
    /// Uses a fixed stack buffer and f64 intermediates. A zero pivot is singular;
    /// there is no determinant/epsilon cutoff, so tiny finite scales are accepted
    /// when the inverse rounds to finite f32. Non-finite input and overflow are
    /// reported separately. Output rounding may underflow coefficients to zero.
    /// Singularity is tested on the computed pivots, not with exact arithmetic;
    /// cancellation can hide a dependency or erase a small nonzero pivot.
    /// This is not a condition-number test: nearly dependent axes can amplify
    /// rounding error, and success does not promise an accurate round trip.
    /// [`Self::try_inverse`] and [`Self::inverse`] retain their Option/NaN behavior.
    ///
    /// ```
    /// use ferravex_math::{Mat4, MatrixError, vec3};
    /// let tiny = Mat4::from_scale(vec3(1.0e-30, 2.0e-30, 3.0e-30));
    /// let inverse = tiny.checked_inverse()?;
    /// assert!((tiny * inverse).abs_diff_eq(Mat4::IDENTITY, 1.0e-6));
    /// assert_eq!(Mat4::NAN.checked_inverse(),
    ///            Err(MatrixError::NonFiniteInput { input: "matrix" }));
    /// # Ok::<(), MatrixError>(())
    /// ```
    pub fn checked_inverse(self) -> Result<Self, MatrixError> {
        finite_input("matrix", &self.to_cols_array())?;
        let mut augmented = [[0.0_f64; 8]; 4];
        for (row, values) in augmented.iter_mut().enumerate() {
            for (column, value) in values.iter_mut().take(4).enumerate() {
                *value = f64::from(self[column][row]);
            }
            values[row + 4] = 1.0;
        }
        for column in 0..4 {
            let pivot = (column..4)
                .max_by(|&a, &b| {
                    augmented[a][column]
                        .abs()
                        .total_cmp(&augmented[b][column].abs())
                })
                .unwrap();
            let divisor = augmented[pivot][column];
            if !divisor.is_finite() {
                return Err(MatrixError::ResultOutOfRange);
            }
            if divisor == 0.0 {
                return Err(MatrixError::Singular { column });
            }
            augmented.swap(column, pivot);
            for value in &mut augmented[column] {
                *value /= divisor;
            }
            let pivot_row = augmented[column];
            for (row, values) in augmented.iter_mut().enumerate() {
                if row == column {
                    continue;
                }
                let coefficient = values[column];
                for (value, pivot_value) in values.iter_mut().zip(pivot_row) {
                    *value -= coefficient * pivot_value;
                }
            }
        }
        let mut result = Self::ZERO;
        for row in 0..4 {
            for column in 0..4 {
                result[column][row] = finite_f32(augmented[row][column + 4])?;
            }
        }
        Ok(result)
    }
    pub fn inverse(self) -> Self {
        self.try_inverse().unwrap_or(Self::NAN)
    }
    /// Right-handed view: the camera looks along its negative Z axis.
    pub fn look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        Self::look_to_rh(eye, target - eye, up)
    }
    pub fn look_to_rh(eye: Vec3, direction: Vec3, up: Vec3) -> Self {
        let z = (-direction).normalize();
        let x = up.cross(z).normalize();
        let y = z.cross(x);
        Self::from_cols(
            vec4(x.x, y.x, z.x, 0.0),
            vec4(x.y, y.y, z.y, 0.0),
            vec4(x.z, y.z, z.z, 0.0),
            vec4(-x.dot(eye), -y.dot(eye), -z.dot(eye), 1.0),
        )
    }
    pub fn look_at_lh(eye: Vec3, target: Vec3, up: Vec3) -> Self {
        Self::look_to_lh(eye, target - eye, up)
    }
    pub fn look_to_lh(eye: Vec3, direction: Vec3, up: Vec3) -> Self {
        Self::look_to_rh(eye, -direction, up)
    }
    /// Checked right-handed view looking from `eye` toward `target` along -Z.
    ///
    /// Rejects non-finite inputs, coincident eye/target, zero or parallel up,
    /// and translations outside f32 range. Subtraction, cross products and
    /// normalization use f64, so tiny directions and extreme finite inputs do
    /// not fail merely because an f32 intermediate would underflow/overflow.
    /// No angular epsilon is imposed; almost parallel up can still be unstable.
    ///
    /// ```
    /// use ferravex_math::{Mat4, MatrixError, Vec3, vec3};
    /// let eye = vec3(0.0, 0.0, 5.0);
    /// let view = Mat4::try_look_at_rh(eye, Vec3::ZERO, Vec3::Y)?;
    /// assert_eq!(view.transform_point3(eye), Vec3::ZERO);
    /// assert_eq!(Mat4::try_look_at_rh(eye, eye, Vec3::Y),
    ///            Err(MatrixError::DegenerateDirection));
    /// # Ok::<(), MatrixError>(())
    /// ```
    pub fn try_look_at_rh(eye: Vec3, target: Vec3, up: Vec3) -> Result<Self, MatrixError> {
        finite_input("eye", &eye.to_array())?;
        finite_input("target", &target.to_array())?;
        view_from_f64(
            eye,
            std::array::from_fn(|i| f64::from(eye[i]) - f64::from(target[i])),
            up,
        )
    }
    /// Checked right-handed view along `direction` (camera -Z).
    /// Uses the finite/degeneracy rules of [`Self::try_look_at_rh`].
    ///
    /// ```
    /// use ferravex_math::{Mat4, Vec3};
    /// assert_eq!(Mat4::try_look_to_rh(Vec3::ZERO, Vec3::NEG_Z, Vec3::Y).unwrap(), Mat4::IDENTITY);
    /// ```
    pub fn try_look_to_rh(eye: Vec3, direction: Vec3, up: Vec3) -> Result<Self, MatrixError> {
        finite_input("direction", &direction.to_array())?;
        view_from_f64(eye, direction.to_array().map(|v| -f64::from(v)), up)
    }
    /// Checked left-handed view toward `target` (camera +Z).
    /// Uses the finite/degeneracy rules of [`Self::try_look_at_rh`].
    ///
    /// ```
    /// use ferravex_math::{Mat4, Vec3};
    /// assert_eq!(Mat4::try_look_at_lh(Vec3::ZERO, Vec3::Z, Vec3::Y).unwrap(), Mat4::IDENTITY);
    /// ```
    pub fn try_look_at_lh(eye: Vec3, target: Vec3, up: Vec3) -> Result<Self, MatrixError> {
        finite_input("eye", &eye.to_array())?;
        finite_input("target", &target.to_array())?;
        view_from_f64(
            eye,
            std::array::from_fn(|i| f64::from(target[i]) - f64::from(eye[i])),
            up,
        )
    }
    /// Checked left-handed view along `direction` (camera +Z).
    /// Uses the finite/degeneracy rules of [`Self::try_look_at_rh`].
    ///
    /// ```
    /// use ferravex_math::{Mat4, Vec3};
    /// assert_eq!(Mat4::try_look_to_lh(Vec3::ZERO, Vec3::Z, Vec3::Y).unwrap(), Mat4::IDENTITY);
    /// ```
    pub fn try_look_to_lh(eye: Vec3, direction: Vec3, up: Vec3) -> Result<Self, MatrixError> {
        finite_input("direction", &direction.to_array())?;
        view_from_f64(eye, direction.to_array().map(f64::from), up)
    }

    /// Right-handed perspective with OpenGL depth [-1, 1].
    pub fn perspective_rh_gl(fov_y: f32, aspect: f32, near: f32, far: f32) -> Self {
        validate_perspective(fov_y, aspect, near, far);
        let y = (0.5 * fov_y).tan().recip();
        Self::from_cols(
            vec4(y / aspect, 0.0, 0.0, 0.0),
            vec4(0.0, y, 0.0, 0.0),
            vec4(0.0, 0.0, (far + near) / (near - far), -1.0),
            vec4(0.0, 0.0, 2.0 * far * near / (near - far), 0.0),
        )
    }
    /// Right-handed perspective with WebGPU/Vulkan/Metal depth [0, 1].
    pub fn perspective_rh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Self {
        validate_perspective(fov_y, aspect, near, far);
        let y = (0.5 * fov_y).tan().recip();
        Self::from_cols(
            vec4(y / aspect, 0.0, 0.0, 0.0),
            vec4(0.0, y, 0.0, 0.0),
            vec4(0.0, 0.0, far / (near - far), -1.0),
            vec4(0.0, 0.0, far * near / (near - far), 0.0),
        )
    }
    pub fn perspective_lh(fov_y: f32, aspect: f32, near: f32, far: f32) -> Self {
        let mut result = Self::perspective_rh(fov_y, aspect, near, far);
        result.z_axis = -result.z_axis;
        result
    }
    pub fn perspective_infinite_rh(fov_y: f32, aspect: f32, near: f32) -> Self {
        validate_perspective(fov_y, aspect, near, near * 2.0);
        let y = (0.5 * fov_y).tan().recip();
        Self::from_cols(
            vec4(y / aspect, 0.0, 0.0, 0.0),
            vec4(0.0, y, 0.0, 0.0),
            vec4(0.0, 0.0, -1.0, -1.0),
            vec4(0.0, 0.0, -near, 0.0),
        )
    }
    pub fn perspective_infinite_reverse_rh(fov_y: f32, aspect: f32, near: f32) -> Self {
        validate_perspective(fov_y, aspect, near, near * 2.0);
        let y = (0.5 * fov_y).tan().recip();
        Self::from_cols(
            vec4(y / aspect, 0.0, 0.0, 0.0),
            vec4(0.0, y, 0.0, 0.0),
            vec4(0.0, 0.0, 0.0, -1.0),
            vec4(0.0, 0.0, near, 0.0),
        )
    }
    pub fn orthographic_rh_gl(
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        near: f32,
        far: f32,
    ) -> Self {
        validate_orthographic(left, right, bottom, top, near, far);
        Self::from_cols(
            vec4(2.0 / (right - left), 0.0, 0.0, 0.0),
            vec4(0.0, 2.0 / (top - bottom), 0.0, 0.0),
            vec4(0.0, 0.0, -2.0 / (far - near), 0.0),
            vec4(
                -(right + left) / (right - left),
                -(top + bottom) / (top - bottom),
                -(far + near) / (far - near),
                1.0,
            ),
        )
    }
    pub fn orthographic_rh(
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        near: f32,
        far: f32,
    ) -> Self {
        validate_orthographic(left, right, bottom, top, near, far);
        Self::from_cols(
            vec4(2.0 / (right - left), 0.0, 0.0, 0.0),
            vec4(0.0, 2.0 / (top - bottom), 0.0, 0.0),
            vec4(0.0, 0.0, -1.0 / (far - near), 0.0),
            vec4(
                -(right + left) / (right - left),
                -(top + bottom) / (top - bottom),
                -near / (far - near),
                1.0,
            ),
        )
    }
    pub fn orthographic_lh(
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        near: f32,
        far: f32,
    ) -> Self {
        let mut result = Self::orthographic_rh(left, right, bottom, top, near, far);
        result.z_axis = -result.z_axis;
        result
    }
    /// Checked right-handed perspective with OpenGL depth [-1, 1].
    ///
    /// All arguments must be finite, `0 < fov_y < PI`, `aspect > 0`, and
    /// `0 < near < far`. Uses f64 intermediates; returns a diagnostic error
    /// instead of panicking. Rejects non-finite output and required scale/depth
    /// coefficients that round to zero. Success does not guarantee good
    /// conditioning or that the inverse is representable in f32.
    ///
    /// ```
    /// use ferravex_math::{Mat4, MatrixError, vec3};
    /// let p = Mat4::try_perspective_rh_gl(1.0, 1.5, 0.5, 100.0)?;
    /// assert!((p.try_project_point3(vec3(0.0, 0.0, -0.5))?.z + 1.0).abs() < 1.0e-6);
    /// # Ok::<(), MatrixError>(())
    /// ```
    pub fn try_perspective_rh_gl(
        fov_y: f32,
        aspect: f32,
        near: f32,
        far: f32,
    ) -> Result<Self, MatrixError> {
        checked_perspective(fov_y, aspect, near, far, true)
    }
    /// Checked right-handed perspective with depth [0, 1].
    /// Uses the input/range rules of [`Self::try_perspective_rh_gl`].
    ///
    /// ```
    /// use ferravex_math::{Mat4, MatrixError};
    /// assert_eq!(Mat4::try_perspective_rh(1.0, 0.0, 0.1, 100.0),
    ///            Err(MatrixError::InvalidPerspective { parameter: "aspect" }));
    /// ```
    pub fn try_perspective_rh(
        fov_y: f32,
        aspect: f32,
        near: f32,
        far: f32,
    ) -> Result<Self, MatrixError> {
        checked_perspective(fov_y, aspect, near, far, false)
    }
    /// Checked left-handed perspective with depth [0, 1].
    /// Uses the input/range rules of [`Self::try_perspective_rh_gl`].
    ///
    /// ```
    /// use ferravex_math::{Mat4, MatrixError, vec3};
    /// let p = Mat4::try_perspective_lh(1.0, 1.0, 0.5, 10.0)?;
    /// assert!(p.try_project_point3(vec3(0.0, 0.0, 0.5))?.z.abs() < 1.0e-6);
    /// # Ok::<(), MatrixError>(())
    /// ```
    pub fn try_perspective_lh(
        fov_y: f32,
        aspect: f32,
        near: f32,
        far: f32,
    ) -> Result<Self, MatrixError> {
        let mut result = Self::try_perspective_rh(fov_y, aspect, near, far)?;
        result.z_axis = -result.z_axis;
        Ok(result)
    }
    /// Checked right-handed infinite perspective: near maps to 0, infinity to 1.
    /// Uses [`Self::try_perspective_rh_gl`]'s rules without a far argument.
    /// A finite positive near is validated directly, without computing `near * 2`.
    ///
    /// ```
    /// use ferravex_math::Mat4;
    /// assert!(Mat4::try_perspective_infinite_rh(1.0, 1.0, f32::MAX).is_ok());
    /// ```
    pub fn try_perspective_infinite_rh(
        fov_y: f32,
        aspect: f32,
        near: f32,
    ) -> Result<Self, MatrixError> {
        let (x, y) = perspective_scales(fov_y, aspect, near)?;
        perspective_from_f64(x, y, -1.0, -f64::from(near))
    }
    /// Checked right-handed infinite reverse depth: near maps to 1, infinity to 0.
    /// Uses the input/range rules of [`Self::try_perspective_infinite_rh`].
    ///
    /// ```
    /// use ferravex_math::{Mat4, MatrixError, vec3};
    /// let p = Mat4::try_perspective_infinite_reverse_rh(1.0, 1.0, 0.5)?;
    /// assert_eq!(p.try_project_point3(vec3(0.0, 0.0, -0.5))?.z, 1.0);
    /// # Ok::<(), MatrixError>(())
    /// ```
    pub fn try_perspective_infinite_reverse_rh(
        fov_y: f32,
        aspect: f32,
        near: f32,
    ) -> Result<Self, MatrixError> {
        let (x, y) = perspective_scales(fov_y, aspect, near)?;
        perspective_from_f64(x, y, 0.0, f64::from(near))
    }
    /// Checked right-handed orthographic projection with OpenGL depth [-1, 1].
    ///
    /// Requires finite bounds and unequal endpoints on each axis. Reversed
    /// ranges and negative near/far are allowed. Uses f64 intermediates and
    /// reports output overflow or a scale rounded to zero instead of panicking.
    /// Success does not guarantee that the inverse fits in f32: even a scale
    /// rounded to a nonzero subnormal can have an overflowing reciprocal.
    ///
    /// ```
    /// use ferravex_math::{Mat4, MatrixError, vec3};
    /// let p = Mat4::try_orthographic_rh_gl(-2.0, 2.0, -1.0, 1.0, 0.5, 10.0)?;
    /// assert!(p.try_project_point3(vec3(-2.0, -1.0, -0.5))?
    ///             .abs_diff_eq(vec3(-1.0, -1.0, -1.0), 1.0e-6));
    /// let wide = Mat4::try_orthographic_rh_gl(
    ///     -f32::MAX, f32::MAX, -1.0, 1.0, 0.0, 1.0)?;
    /// assert_eq!(wide.checked_inverse(), Err(MatrixError::ResultOutOfRange));
    /// # Ok::<(), MatrixError>(())
    /// ```
    pub fn try_orthographic_rh_gl(
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        near: f32,
        far: f32,
    ) -> Result<Self, MatrixError> {
        checked_orthographic([left, right, bottom, top, near, far], true)
    }
    /// Checked right-handed orthographic projection with depth [0, 1].
    /// Uses the input/range rules of [`Self::try_orthographic_rh_gl`].
    ///
    /// ```
    /// use ferravex_math::{Mat4, MatrixError};
    /// assert_eq!(Mat4::try_orthographic_rh(0.0, 0.0, -1.0, 1.0, 0.1, 10.0),
    ///            Err(MatrixError::DegenerateOrthographic { axis: "x" }));
    /// ```
    pub fn try_orthographic_rh(
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        near: f32,
        far: f32,
    ) -> Result<Self, MatrixError> {
        checked_orthographic([left, right, bottom, top, near, far], false)
    }
    /// Checked left-handed orthographic projection with depth [0, 1].
    /// Uses the input/range rules of [`Self::try_orthographic_rh_gl`].
    ///
    /// ```
    /// use ferravex_math::{Mat4, MatrixError, vec3};
    /// let p = Mat4::try_orthographic_lh(-1.0, 1.0, -1.0, 1.0, 0.5, 10.0)?;
    /// assert!(p.try_project_point3(vec3(0.0, 0.0, 0.5))?.z.abs() < 1.0e-6);
    /// # Ok::<(), MatrixError>(())
    /// ```
    pub fn try_orthographic_lh(
        left: f32,
        right: f32,
        bottom: f32,
        top: f32,
        near: f32,
        far: f32,
    ) -> Result<Self, MatrixError> {
        let mut result = Self::try_orthographic_rh(left, right, bottom, top, near, far)?;
        result.z_axis = -result.z_axis;
        Ok(result)
    }
    /// Extracts TRS from an affine matrix with orthogonal, nonzero scale axes.
    /// Shear and perspective are rejected rather than approximated.
    pub fn try_to_scale_rotation_translation(self) -> Option<(Vec3, Quat, Vec3)> {
        if !self.is_finite() || !self.row(3).abs_diff_eq(Vec4::W, 1.0e-5) {
            return None;
        }
        let mut scale = vec3(
            self.x_axis.truncate().length(),
            self.y_axis.truncate().length(),
            self.z_axis.truncate().length(),
        );
        if scale.min_element() <= 0.0 {
            return None;
        }
        if self.determinant() < 0.0 {
            scale.x = -scale.x;
        }
        let x = self.x_axis.truncate() / scale.x;
        let y = self.y_axis.truncate() / scale.y;
        let z = self.z_axis.truncate() / scale.z;
        if x.dot(y).abs() > 1.0e-4 || y.dot(z).abs() > 1.0e-4 || z.dot(x).abs() > 1.0e-4 {
            return None;
        }
        let rotation = rotation_from_basis(x, y, z);
        Some((scale, rotation, self.w_axis.truncate()))
    }
    pub fn to_scale_rotation_translation(self) -> (Vec3, Quat, Vec3) {
        self.try_to_scale_rotation_translation()
            .unwrap_or((Vec3::NAN, Quat::NAN, Vec3::NAN))
    }
}
fn finite_input(input: &'static str, values: &[f32]) -> Result<(), MatrixError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(MatrixError::NonFiniteInput { input })
    }
}

fn finite_f32(value: f64) -> Result<f32, MatrixError> {
    let value = value as f32;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(MatrixError::ResultOutOfRange)
    }
}

fn matrix_from_f64(columns: [[f64; 4]; 4]) -> Result<Mat4, MatrixError> {
    let mut result = Mat4::ZERO;
    for (column, values) in columns.into_iter().enumerate() {
        for (row, value) in values.into_iter().enumerate() {
            result[column][row] = finite_f32(value)?;
        }
    }
    Ok(result)
}

fn dot3_f64(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

fn cross3_f64(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit3_f64(v: [f64; 3]) -> Option<[f64; 3]> {
    let length = dot3_f64(v, v).sqrt();
    (length > 0.0).then(|| v.map(|value| value / length))
}

fn view_from_f64(eye: Vec3, back: [f64; 3], up: Vec3) -> Result<Mat4, MatrixError> {
    finite_input("eye", &eye.to_array())?;
    finite_input("up", &up.to_array())?;
    let z = unit3_f64(back).ok_or(MatrixError::DegenerateDirection)?;
    // Cross before normalizing: products of finite f32 inputs fit in f64,
    // including subnormals, without losing an exactly parallel basis to rounding.
    let x = unit3_f64(cross3_f64(up.to_array().map(f64::from), back))
        .ok_or(MatrixError::DegenerateUp)?;
    let y = cross3_f64(z, x);
    let eye = eye.to_array().map(f64::from);
    matrix_from_f64([
        [x[0], y[0], z[0], 0.0],
        [x[1], y[1], z[1], 0.0],
        [x[2], y[2], z[2], 0.0],
        [-dot3_f64(x, eye), -dot3_f64(y, eye), -dot3_f64(z, eye), 1.0],
    ])
}

fn perspective_scales(fov_y: f32, aspect: f32, near: f32) -> Result<(f64, f64), MatrixError> {
    for (input, value) in [("fov_y", fov_y), ("aspect", aspect), ("near", near)] {
        finite_input(input, &[value])?;
        if value <= 0.0 || (input == "fov_y" && value >= std::f32::consts::PI) {
            return Err(MatrixError::InvalidPerspective { parameter: input });
        }
    }
    let y = (0.5 * f64::from(fov_y)).tan().recip();
    Ok((y / f64::from(aspect), y))
}

fn perspective_from_f64(x: f64, y: f64, z: f64, depth: f64) -> Result<Mat4, MatrixError> {
    let result = matrix_from_f64([
        [x, 0.0, 0.0, 0.0],
        [0.0, y, 0.0, 0.0],
        [0.0, 0.0, z, -1.0],
        [0.0, 0.0, depth, 0.0],
    ])?;
    if result.x_axis.x == 0.0 || result.y_axis.y == 0.0 || result.w_axis.z == 0.0 {
        return Err(MatrixError::ResultOutOfRange);
    }
    Ok(result)
}

fn checked_perspective(
    fov_y: f32,
    aspect: f32,
    near: f32,
    far: f32,
    gl: bool,
) -> Result<Mat4, MatrixError> {
    let (x, y) = perspective_scales(fov_y, aspect, near)?;
    finite_input("far", &[far])?;
    if far <= near {
        return Err(MatrixError::InvalidPerspective { parameter: "far" });
    }
    let (near, far) = (f64::from(near), f64::from(far));
    let span = near - far;
    let z = if gl { (far + near) / span } else { far / span };
    let depth = (if gl { 2.0 } else { 1.0 }) * far * near / span;
    perspective_from_f64(x, y, z, depth)
}

fn checked_orthographic(bounds: [f32; 6], gl: bool) -> Result<Mat4, MatrixError> {
    for (input, value) in ["left", "right", "bottom", "top", "near", "far"]
        .into_iter()
        .zip(bounds)
    {
        finite_input(input, &[value])?;
    }
    for (axis, pair) in ["x", "y", "z"].into_iter().zip(bounds.as_chunks::<2>().0) {
        if pair[0] == pair[1] {
            return Err(MatrixError::DegenerateOrthographic { axis });
        }
    }
    let [left, right, bottom, top, near, far] = bounds.map(f64::from);
    let (width, height, depth) = (right - left, top - bottom, far - near);
    let result = matrix_from_f64([
        [2.0 / width, 0.0, 0.0, 0.0],
        [0.0, 2.0 / height, 0.0, 0.0],
        [0.0, 0.0, -(if gl { 2.0 } else { 1.0 }) / depth, 0.0],
        [
            -(right + left) / width,
            -(top + bottom) / height,
            -(if gl { far + near } else { near }) / depth,
            1.0,
        ],
    ])?;
    if result.x_axis.x == 0.0 || result.y_axis.y == 0.0 || result.z_axis.z == 0.0 {
        return Err(MatrixError::ResultOutOfRange);
    }
    Ok(result)
}

fn rotation_from_basis(x: Vec3, y: Vec3, z: Vec3) -> Quat {
    let trace = x.x + y.y + z.z;
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        Quat::from_xyzw((y.z - z.y) / s, (z.x - x.z) / s, (x.y - y.x) / s, s * 0.25)
    } else if x.x > y.y && x.x > z.z {
        let s = (1.0 + x.x - y.y - z.z).sqrt() * 2.0;
        Quat::from_xyzw(s * 0.25, (y.x + x.y) / s, (z.x + x.z) / s, (y.z - z.y) / s)
    } else if y.y > z.z {
        let s = (1.0 + y.y - x.x - z.z).sqrt() * 2.0;
        Quat::from_xyzw((y.x + x.y) / s, s * 0.25, (z.y + y.z) / s, (z.x - x.z) / s)
    } else {
        let s = (1.0 + z.z - x.x - y.y).sqrt() * 2.0;
        Quat::from_xyzw((z.x + x.z) / s, (z.y + y.z) / s, s * 0.25, (x.y - y.x) / s)
    };
    q.normalize()
}
fn validate_perspective(fov: f32, aspect: f32, near: f32, far: f32) {
    assert!(
        fov.is_finite()
            && fov > 0.0
            && fov < std::f32::consts::PI
            && aspect.is_finite()
            && aspect > 0.0
            && near.is_finite()
            && far.is_finite()
            && near > 0.0
            && far > near,
        "invalid perspective frustum"
    );
}
fn validate_orthographic(left: f32, right: f32, bottom: f32, top: f32, near: f32, far: f32) {
    assert!(
        [left, right, bottom, top, near, far]
            .into_iter()
            .all(f32::is_finite)
            && left != right
            && bottom != top
            && near != far,
        "invalid orthographic frustum"
    );
}
impl Index<usize> for Mat4 {
    type Output = Vec4;
    fn index(&self, index: usize) -> &Vec4 {
        match index {
            0 => &self.x_axis,
            1 => &self.y_axis,
            2 => &self.z_axis,
            3 => &self.w_axis,
            _ => panic!("matrix column {index} is out of range"),
        }
    }
}
impl IndexMut<usize> for Mat4 {
    fn index_mut(&mut self, index: usize) -> &mut Vec4 {
        match index {
            0 => &mut self.x_axis,
            1 => &mut self.y_axis,
            2 => &mut self.z_axis,
            3 => &mut self.w_axis,
            _ => panic!("matrix column {index} is out of range"),
        }
    }
}
impl Mul<Vec4> for Mat4 {
    type Output = Vec4;
    fn mul(self, rhs: Vec4) -> Vec4 {
        self.mul_vec4(rhs)
    }
}
impl Mul for Mat4 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self::from_cols(
            self * rhs.x_axis,
            self * rhs.y_axis,
            self * rhs.z_axis,
            self * rhs.w_axis,
        )
    }
}
impl Mul<f32> for Mat4 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::from_cols(
            self.x_axis * rhs,
            self.y_axis * rhs,
            self.z_axis * rhs,
            self.w_axis * rhs,
        )
    }
}
impl Mul<Mat4> for f32 {
    type Output = Mat4;
    fn mul(self, rhs: Mat4) -> Mat4 {
        rhs * self
    }
}
impl MulAssign for Mat4 {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}
impl Add for Mat4 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::from_cols(
            self.x_axis + rhs.x_axis,
            self.y_axis + rhs.y_axis,
            self.z_axis + rhs.z_axis,
            self.w_axis + rhs.w_axis,
        )
    }
}
impl Sub for Mat4 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::from_cols(
            self.x_axis - rhs.x_axis,
            self.y_axis - rhs.y_axis,
            self.z_axis - rhs.z_axis,
            self.w_axis - rhs.w_axis,
        )
    }
}
