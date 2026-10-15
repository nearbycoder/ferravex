use crate::{Quat, Vec3, Vec4, vec3, vec4};
use std::ops::{Add, Index, IndexMut, Mul, MulAssign, Sub};

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
    pub fn try_inverse(self) -> Option<Self> {
        if !self.is_finite() {
            return None;
        }
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
            if divisor == 0.0 || !divisor.is_finite() {
                return None;
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
                result[column][row] = augmented[row][column + 4] as f32;
            }
        }
        result.is_finite().then_some(result)
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
