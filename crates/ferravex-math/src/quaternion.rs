use crate::{Vec3, Vec4, vec3};
use std::ops::{Add, Div, Mul, MulAssign, Neg, Sub};

/// Intrinsic Euler rotations: `XYZ(a,b,c)` composes `Rx(a) * Ry(b) * Rz(c)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EulerRot {
    XYZ,
    XZY,
    YXZ,
    YZX,
    ZXY,
    ZYX,
}

/// Quaternion stored as vector part `(x,y,z)` followed by scalar part `w`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}
impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}
impl Quat {
    pub const IDENTITY: Self = Self::from_xyzw(0.0, 0.0, 0.0, 1.0);
    pub const NAN: Self = Self::from_xyzw(f32::NAN, f32::NAN, f32::NAN, f32::NAN);
    pub const fn from_xyzw(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
    pub const fn from_array(v: [f32; 4]) -> Self {
        Self::from_xyzw(v[0], v[1], v[2], v[3])
    }
    pub const fn to_array(self) -> [f32; 4] {
        [self.x, self.y, self.z, self.w]
    }
    pub const fn from_vec4(v: Vec4) -> Self {
        Self::from_xyzw(v.x, v.y, v.z, v.w)
    }
    pub const fn to_vec4(self) -> Vec4 {
        Vec4::new(self.x, self.y, self.z, self.w)
    }
    pub const fn xyz(self) -> Vec3 {
        vec3(self.x, self.y, self.z)
    }
    /// Finite nonzero axes are normalized. A zero axis returns NaNs.
    pub fn from_axis_angle(axis: Vec3, angle: f32) -> Self {
        let unit = axis.normalize();
        let (s, c) = (angle * 0.5).sin_cos();
        Self::from_xyzw(unit.x * s, unit.y * s, unit.z * s, c)
    }
    pub fn from_rotation_x(angle: f32) -> Self {
        let (s, c) = (angle * 0.5).sin_cos();
        Self::from_xyzw(s, 0.0, 0.0, c)
    }
    pub fn from_rotation_y(angle: f32) -> Self {
        let (s, c) = (angle * 0.5).sin_cos();
        Self::from_xyzw(0.0, s, 0.0, c)
    }
    pub fn from_rotation_z(angle: f32) -> Self {
        let (s, c) = (angle * 0.5).sin_cos();
        Self::from_xyzw(0.0, 0.0, s, c)
    }
    pub fn from_euler(order: EulerRot, a: f32, b: f32, c: f32) -> Self {
        use EulerRot::*;
        match order {
            XYZ => Self::from_rotation_x(a) * Self::from_rotation_y(b) * Self::from_rotation_z(c),
            XZY => Self::from_rotation_x(a) * Self::from_rotation_z(b) * Self::from_rotation_y(c),
            YXZ => Self::from_rotation_y(a) * Self::from_rotation_x(b) * Self::from_rotation_z(c),
            YZX => Self::from_rotation_y(a) * Self::from_rotation_z(b) * Self::from_rotation_x(c),
            ZXY => Self::from_rotation_z(a) * Self::from_rotation_x(b) * Self::from_rotation_y(c),
            ZYX => Self::from_rotation_z(a) * Self::from_rotation_y(b) * Self::from_rotation_x(c),
        }
    }
    pub fn from_scaled_axis(axis: Vec3) -> Self {
        if axis == Vec3::ZERO {
            Self::IDENTITY
        } else {
            Self::from_axis_angle(axis, axis.length())
        }
    }
    /// Shortest rotation between two finite nonzero directions.
    pub fn from_rotation_arc(from: Vec3, to: Vec3) -> Self {
        let a = from.normalize();
        let b = to.normalize();
        let cosine = a.dot(b).clamp(-1.0, 1.0);
        if cosine > 1.0 - 1.0e-6 {
            return Self::IDENTITY;
        }
        if cosine < -1.0 + 1.0e-6 {
            return Self::from_axis_angle(a.any_orthonormal_vector(), std::f32::consts::PI);
        }
        let cross = a.cross(b);
        Self::from_xyzw(cross.x, cross.y, cross.z, 1.0 + cosine).normalize()
    }
    pub fn to_axis_angle(self) -> (Vec3, f32) {
        let q = self.normalize();
        let sine = q.xyz().length();
        if sine < 1.0e-7 {
            (Vec3::X, 0.0)
        } else {
            (q.xyz() / sine, 2.0 * sine.atan2(q.w))
        }
    }
    pub fn to_scaled_axis(self) -> Vec3 {
        let (axis, angle) = self.to_axis_angle();
        axis * angle
    }
    pub fn dot(self, rhs: Self) -> f32 {
        self.to_vec4().dot(rhs.to_vec4())
    }
    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }
    pub fn length(self) -> f32 {
        self.to_vec4().length()
    }
    pub fn is_finite(self) -> bool {
        self.to_vec4().is_finite()
    }
    pub fn is_nan(self) -> bool {
        self.to_vec4().is_nan()
    }
    pub fn is_normalized(self) -> bool {
        self.to_vec4().is_normalized()
    }
    pub fn try_normalize(self) -> Option<Self> {
        self.to_vec4().try_normalize().map(Self::from_vec4)
    }
    pub fn normalize(self) -> Self {
        self.try_normalize().unwrap_or(Self::NAN)
    }
    pub const fn conjugate(self) -> Self {
        Self::from_xyzw(-self.x, -self.y, -self.z, self.w)
    }
    /// Algebraic inverse, also correct for finite non-unit quaternions.
    pub fn try_inverse(self) -> Option<Self> {
        let a = self.to_array();
        let norm = a.iter().map(|x| f64::from(*x).powi(2)).sum::<f64>();
        if norm == 0.0 || !norm.is_finite() {
            return None;
        }
        let result = Self::from_array(
            self.conjugate()
                .to_array()
                .map(|x| (f64::from(x) / norm) as f32),
        );
        result.is_finite().then_some(result)
    }
    pub fn inverse(self) -> Self {
        self.try_inverse().unwrap_or(Self::NAN)
    }
    /// Applies a unit quaternion rotation. Non-unit quaternions must be normalized first.
    pub fn mul_vec3(self, v: Vec3) -> Vec3 {
        let t = 2.0 * self.xyz().cross(v);
        v + self.w * t + self.xyz().cross(t)
    }
    pub fn mul_quat(self, rhs: Self) -> Self {
        self * rhs
    }
    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        let rhs = if self.dot(rhs) < 0.0 { -rhs } else { rhs };
        (self * (1.0 - t) + rhs * t).normalize()
    }
    /// Spherical interpolation follows the shortest rotation and normalizes endpoints.
    pub fn slerp(self, rhs: Self, t: f32) -> Self {
        let a = self.normalize();
        let mut b = rhs.normalize();
        let mut cosine = a.dot(b);
        if cosine < 0.0 {
            b = -b;
            cosine = -cosine;
        }
        if cosine > 0.9995 {
            return a.lerp(b, t);
        }
        let angle = cosine.clamp(-1.0, 1.0).acos();
        let divisor = angle.sin();
        (a * ((1.0 - t) * angle).sin() / divisor + b * (t * angle).sin() / divisor).normalize()
    }
    pub fn angle_between(self, rhs: Self) -> f32 {
        2.0 * self
            .normalize()
            .dot(rhs.normalize())
            .abs()
            .clamp(0.0, 1.0)
            .acos()
    }
    pub fn abs_diff_eq(self, rhs: Self, tolerance: f32) -> bool {
        self.to_vec4().abs_diff_eq(rhs.to_vec4(), tolerance)
    }
    pub fn rotate_towards(self, rhs: Self, max_angle: f32) -> Self {
        assert!(max_angle.is_finite() && max_angle >= 0.0);
        let angle = self.angle_between(rhs);
        if angle <= max_angle {
            rhs.normalize()
        } else {
            self.slerp(rhs, max_angle / angle)
        }
    }
}
impl From<[f32; 4]> for Quat {
    fn from(v: [f32; 4]) -> Self {
        Self::from_array(v)
    }
}
impl From<Quat> for [f32; 4] {
    fn from(v: Quat) -> Self {
        v.to_array()
    }
}
impl From<Quat> for Vec4 {
    fn from(v: Quat) -> Self {
        v.to_vec4()
    }
}
impl Neg for Quat {
    type Output = Self;
    fn neg(self) -> Self {
        Self::from_vec4(-self.to_vec4())
    }
}
impl Add for Quat {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::from_vec4(self.to_vec4() + rhs.to_vec4())
    }
}
impl Sub for Quat {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::from_vec4(self.to_vec4() - rhs.to_vec4())
    }
}
impl Mul<f32> for Quat {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::from_vec4(self.to_vec4() * rhs)
    }
}
impl Mul<Quat> for f32 {
    type Output = Quat;
    fn mul(self, rhs: Quat) -> Quat {
        rhs * self
    }
}
impl Div<f32> for Quat {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self::from_vec4(self.to_vec4() / rhs)
    }
}
impl Mul for Quat {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let vector = self.w * rhs.xyz() + rhs.w * self.xyz() + self.xyz().cross(rhs.xyz());
        Self::from_xyzw(
            vector.x,
            vector.y,
            vector.z,
            self.w * rhs.w - self.xyz().dot(rhs.xyz()),
        )
    }
}
impl Mul<Vec3> for Quat {
    type Output = Vec3;
    fn mul(self, rhs: Vec3) -> Vec3 {
        self.mul_vec3(rhs)
    }
}
impl MulAssign for Quat {
    fn mul_assign(&mut self, rhs: Self) {
        *self = *self * rhs;
    }
}
