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

/// Checked factorization of a rotation into `swing * twist` (twist acts first).
///
/// Both factors are normalized. `twist` rotates about the requested local axis;
/// `swing` carries that axis to its final direction. Reconstruction agrees with
/// the normalized input orientation, up to floating-point rounding and sign.
///
/// ```
/// use ferravex_math::{Quat, SwingTwist, Vec3};
/// let parts: SwingTwist = Quat::from_rotation_y(0.4).try_swing_twist(Vec3::Y).unwrap();
/// assert!(!parts.is_singular);
/// assert!(parts.swing.try_angle_between(Quat::IDENTITY).unwrap() < 1.0e-6);
/// assert!((parts.twist_angle - 0.4).abs() < 1.0e-6);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SwingTwist {
    /// Swing factor; applied after [`Self::twist`].
    pub swing: Quat,
    /// Twist factor about the normalized authored axis.
    pub twist: Quat,
    /// Signed shortest twist in radians, in `[-pi, pi]`.
    pub twist_angle: f32,
    /// True for an exact half-turn swing with underdetermined twist.
    ///
    /// In that case `twist` is identity, `twist_angle` is zero and `swing`
    /// retains the entire input rotation. This is a convention, not a measured
    /// zero twist. Near-singular rotations are not snapped to this convention.
    pub is_singular: bool,
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

    /// Decomposes into `swing * twist` about a local-space `twist_axis`.
    ///
    /// Finite nonzero quaternions and axes are normalized internally, including
    /// very large and subnormal inputs. Zero or non-finite inputs return `None`.
    /// Projection and angle calculations use `f64` intermediates; results are
    /// rounded to `f32`. The operation uses no heap storage or retained state.
    ///
    /// The twist angle is the signed shortest angle in `[-pi, pi]`. Antipodal
    /// inputs give the same factors and angle: choose positive `w`, or at exact
    /// `w == 0`, choose the sign making the first nonzero component in `(x,y,z)`
    /// positive. This also resolves exact twist half-turn ties. Reversing the
    /// axis negates the angle (including those ties), retaining the factors.
    /// This tie rule is local to this API; existing interpolation is unchanged.
    ///
    /// A singular half-turn swing has both scalar part and vector projection
    /// onto the axis exactly zero in the wider calculation. It returns
    /// [`SwingTwist::is_singular`] with identity twist, rather than rejecting a
    /// valid rotation. No epsilon cutoff is applied: near this singularity the
    /// twist is inherently sensitive to small changes. Trigonometric creation
    /// of a nominal half turn may leave a small nonzero scalar component.
    ///
    /// ```
    /// use ferravex_math::{Quat, Vec3};
    /// let rotation = Quat::from_rotation_x(0.7) * Quat::from_rotation_z(-0.4);
    /// let parts = rotation.try_swing_twist(Vec3::Z * 5.0).unwrap();
    /// let reconstructed = parts.swing * parts.twist;
    /// assert!(rotation.try_angle_between(reconstructed).unwrap() < 1.0e-6);
    /// assert!((parts.twist_angle + 0.4).abs() < 1.0e-6);
    /// let half_swing = Quat::from_xyzw(1.0, 0.0, 0.0, 0.0);
    /// assert!(half_swing.try_swing_twist(Vec3::Z).unwrap().is_singular);
    /// assert_eq!(rotation.try_swing_twist(Vec3::ZERO), None);
    /// ```
    pub fn try_swing_twist(self, twist_axis: Vec3) -> Option<SwingTwist> {
        let mut q = self.try_normalized_components()?;
        let axis = twist_axis.to_array().map(f64::from);
        let axis_length = axis.iter().map(|v| v * v).sum::<f64>().sqrt();
        if axis_length == 0.0 || !axis_length.is_finite() {
            return None;
        }
        let axis = axis.map(|v| v / axis_length);
        if [q[3], q[0], q[1], q[2]].into_iter().find(|v| *v != 0.0)? < 0.0 {
            q = q.map(|v| -v);
        }
        let rotation = Self::from_array(q.map(|v| v as f32));
        let projection = axis.iter().zip(q).map(|(a, v)| a * v).sum::<f64>();
        let length = projection.hypot(q[3]);
        if length == 0.0 {
            return Some(SwingTwist {
                swing: rotation,
                twist: Self::IDENTITY,
                twist_angle: 0.0,
                is_singular: true,
            });
        }
        let twist = Self::from_xyzw(
            (axis[0] * projection / length) as f32,
            (axis[1] * projection / length) as f32,
            (axis[2] * projection / length) as f32,
            (q[3] / length) as f32,
        );
        Some(SwingTwist {
            swing: (rotation * twist.conjugate()).normalize(),
            twist,
            twist_angle: (2.0 * projection.atan2(q[3])) as f32,
            is_singular: false,
        })
    }

    /// Clamps the signed shortest twist about a local axis, preserving swing.
    ///
    /// Bounds are inclusive radians and must be finite with
    /// `-f32::consts::PI <= min_angle <= max_angle <= f32::consts::PI`.
    /// They describe an ordinary interval, not a wrapped interval across pi.
    /// Invalid bounds, axes or rotations return `None`, even for a full range.
    /// Uses the sign/tie/singularity conventions of [`Self::try_swing_twist`].
    /// A singular decomposition uses its conventional zero twist; if zero lies
    /// outside the limits, the nearest bound is applied to that swing. Inspect
    /// the decomposition first if authoring requires a different policy.
    ///
    /// Returns a normalized rotation. If the rounded twist angle already lies
    /// within the bounds, returns normalized `self` without recomposition,
    /// preserving its sign. Otherwise returns `swing * limited_twist`, whose
    /// sign need not match `self`. The rotated axis direction is preserved up
    /// to floating-point rounding. Rounding can put the recovered angle outside
    /// a bound; its error can be large arbitrarily close to a singular swing,
    /// especially when components underflow. Exact idempotence is not guaranteed.
    /// There is no iterative solver or state.
    ///
    /// This headless pose recipe aims a local Z axis, then limits authored roll
    /// without changing the aim direction. A caller can compose the result with
    /// its existing parent/bind-pose transform; no game behavior is selected.
    ///
    /// ```
    /// use ferravex_math::{Quat, Vec3};
    /// let aim = Quat::from_rotation_arc(Vec3::Z, Vec3::new(0.4, 0.2, 1.0));
    /// let authored_pose = aim * Quat::from_rotation_z(1.2);
    /// let limited_pose = authored_pose.try_clamp_twist(Vec3::Z, -0.3, 0.3).unwrap();
    /// assert!((limited_pose * Vec3::Z).abs_diff_eq(authored_pose * Vec3::Z, 1.0e-6));
    /// let parts = limited_pose.try_swing_twist(Vec3::Z).unwrap();
    /// assert!((parts.twist_angle - 0.3).abs() < 1.0e-6);
    /// assert!(parts.swing.try_angle_between(aim).unwrap() < 1.0e-6);
    /// assert_eq!(authored_pose.try_clamp_twist(Vec3::Z, 0.3, -0.3), None);
    /// ```
    pub fn try_clamp_twist(self, twist_axis: Vec3, min_angle: f32, max_angle: f32) -> Option<Self> {
        let pi = std::f32::consts::PI;
        if !min_angle.is_finite()
            || !max_angle.is_finite()
            || min_angle < -pi
            || max_angle > pi
            || min_angle > max_angle
        {
            return None;
        }
        let parts = self.try_swing_twist(twist_axis)?;
        let angle = parts.twist_angle.clamp(min_angle, max_angle);
        if angle == parts.twist_angle {
            return self.try_normalize();
        }
        // Apply only the twist correction. Algebraically this is swing times
        // the limited twist, but avoids rounding swing again on small repeated
        // boundary corrections. Right multiplication keeps the local axis fixed.
        (self.try_normalize()? * Self::from_axis_angle(twist_axis, angle - parts.twist_angle))
            .try_normalize()
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
    /// Returns the shortest angular distance in **radians**, in `[0, pi]`.
    ///
    /// Both inputs are normalized internally. Returns `None` if either input
    /// is zero or has a non-finite component; finite non-unit inputs, including
    /// subnormal and very large inputs, are accepted. Antipodal quaternions
    /// (`q` and `-q`) represent the same orientation and have zero distance.
    ///
    /// Unlike [`Self::angle_between`], this uses wider intermediates and chord
    /// lengths instead of `acos(dot)`, retaining small angular differences near
    /// identical orientations without a fixed small-angle cutoff. The returned
    /// distance is still subject to `f32` rounding.
    ///
    /// ```
    /// use ferravex_math::Quat;
    /// use std::f32::consts::FRAC_PI_2;
    ///
    /// let target = Quat::from_rotation_y(FRAC_PI_2);
    /// let distance = Quat::IDENTITY.try_angle_between(target).unwrap();
    /// assert!((distance - FRAC_PI_2).abs() < 1.0e-6);
    /// assert_eq!(target.try_angle_between(-target), Some(0.0));
    /// assert_eq!(Quat::NAN.try_angle_between(target), None);
    /// ```
    pub fn try_angle_between(self, rhs: Self) -> Option<f32> {
        let (_, _, half_angle) = self.try_shortest_arc(rhs)?;
        Some((2.0 * half_angle) as f32)
    }

    /// Rotates toward `rhs` along the shortest arc by at most `max_angle`
    /// **radians**, without overshooting (subject to floating-point rounding).
    ///
    /// Returns a normalized quaternion. Returns `None` if either endpoint is
    /// zero or non-finite, or if `max_angle` is negative or non-finite. Both
    /// endpoints are validated even for a zero step. Finite non-unit endpoints
    /// are accepted, as in [`Self::try_angle_between`].
    ///
    /// A zero step returns normalized `self`. Identical and antipodal (`q`,
    /// `-q`) orientations also return normalized `self`. Otherwise, `rhs` is
    /// sign-adjusted to the hemisphere of `self`; a sufficient step returns
    /// that normalized target. At an exact half turn (zero endpoint dot
    /// product), both directions are equally short and the supplied signs
    /// select the arc. Near-identical orientations use spherical interpolation
    /// with no fixed small-angle cutoff or premature snap.
    ///
    /// `max_angle` is a per-call angular budget, not an angular speed. For
    /// fixed-tick motion, multiply radians per second by the tick duration:
    ///
    /// ```
    /// use ferravex_math::Quat;
    ///
    /// let target = Quat::from_rotation_y(1.0);
    /// let radians_per_second = 2.0;
    /// let tick_seconds = 1.0 / 60.0;
    /// let step = radians_per_second * tick_seconds;
    /// let next = Quat::IDENTITY.try_rotate_towards(target, step).unwrap();
    /// let moved = Quat::IDENTITY.try_angle_between(next).unwrap();
    /// assert!((moved - step).abs() < 1.0e-6);
    /// assert!(next.is_normalized());
    /// assert_eq!(next.try_rotate_towards(target, -step), None);
    /// ```
    pub fn try_rotate_towards(self, rhs: Self, max_angle: f32) -> Option<Self> {
        if !max_angle.is_finite() || max_angle < 0.0 {
            return None;
        }
        let (a, b, half_angle) = self.try_shortest_arc(rhs)?;
        if max_angle == 0.0 || half_angle == 0.0 {
            return Some(Self::from_array(a.map(|v| v as f32)));
        }
        let step = f64::from(max_angle) * 0.5;
        if step >= half_angle {
            return Some(Self::from_array(b.map(|v| v as f32)));
        }
        let divisor = half_angle.sin();
        let from_weight = (half_angle - step).sin() / divisor;
        let to_weight = step.sin() / divisor;
        Self::from_array(std::array::from_fn(|i| {
            (a[i] * from_weight + b[i] * to_weight) as f32
        }))
        .try_normalize()
    }

    // Keep normalized components in f64 through distance and interpolation:
    // rounding them to f32 first can erase a small, representable rotation.
    fn try_shortest_arc(self, rhs: Self) -> Option<([f64; 4], [f64; 4], f64)> {
        let a = self.try_normalized_components()?;
        let mut b = rhs.try_normalized_components()?;
        if a.iter().zip(b).map(|(a, b)| a * b).sum::<f64>() < 0.0 {
            b = b.map(|v| -v);
        }
        let difference = a
            .iter()
            .zip(b)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f64>()
            .sqrt();
        let sum = a
            .iter()
            .zip(b)
            .map(|(a, b)| (a + b).powi(2))
            .sum::<f64>()
            .sqrt();
        // Unit quaternion chord ratio is tan(physical rotation angle / 4).
        let half_angle = (2.0 * difference.atan2(sum)).min(std::f64::consts::FRAC_PI_2);
        Some((a, b, half_angle))
    }

    fn try_normalized_components(self) -> Option<[f64; 4]> {
        let values = self.to_array().map(f64::from);
        let length = values.iter().map(|v| v * v).sum::<f64>().sqrt();
        if length > 0.0 && length.is_finite() {
            Some(values.map(|v| v / length))
        } else {
            None
        }
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
