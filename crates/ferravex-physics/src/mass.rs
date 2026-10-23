//! Body-space symmetric inertia tensors and primitive/compound mass properties.
use crate::{
    PhysicsError,
    body::{Collider, Shape},
};
use ferravex_math::{Quat, Vec3};

/// Why checked mass construction failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MassPropertyIssue {
    /// A dynamic body needs at least one collider.
    EmptyCompound,
    /// The compound exceeds [`crate::MAX_CHILDREN`].
    TooManyChildren,
    /// The collider's material or local pose is invalid for this dimension.
    InvalidCollider,
    /// Density must be finite and in `0..=1e6`.
    InvalidDensity,
    /// A shape coordinate or dimension is not finite.
    NonFiniteShape,
    /// A radius or half extent is not strictly positive.
    DegenerateShape,
    /// The shape cannot be used in the requested dimension.
    UnsupportedShape,
    /// All children have zero density.
    ZeroMass,
    /// Positive density and dimensions produced zero mass in `f32` arithmetic.
    MassUnderflow,
    /// A primitive or accumulated mass is not finite.
    MassOverflow,
    /// The reciprocal of the total mass is not finite.
    InverseMassOverflow,
    /// A primitive center or weighted center calculation is not finite.
    CenterOverflow,
    /// A primitive, transformed, or accumulated inertia is not finite.
    InertiaOverflow,
    /// Active inertia is nonpositive or numerically singular.
    DegenerateInertia,
    /// Inverting inertia produced a nonfinite or nonpositive active diagonal.
    InverseInertiaOverflow,
}

/// A mass-construction failure, with the zero-based input child when known.
///
/// `child == None` identifies a compound-wide failure, such as zero total mass
/// or a singular combined inertia. This error also converts to the existing
/// [`PhysicsError`] categories used by world insertion and mass refresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MassPropertyError {
    /// Zero-based input child, or `None` for a failure of the combined result.
    pub child: Option<usize>,
    /// The failed validation or calculation stage.
    pub issue: MassPropertyIssue,
}

impl From<MassPropertyIssue> for MassPropertyError {
    fn from(issue: MassPropertyIssue) -> Self {
        Self { child: None, issue }
    }
}

impl From<MassPropertyError> for PhysicsError {
    fn from(error: MassPropertyError) -> Self {
        use MassPropertyIssue::*;
        match error.issue {
            TooManyChildren => Self::ChildCapacity,
            InvalidCollider | InvalidDensity | NonFiniteShape | DegenerateShape
            | UnsupportedShape => Self::InvalidCollider,
            _ => Self::InvalidMass,
        }
    }
}

impl std::fmt::Display for MassPropertyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use MassPropertyIssue::*;
        let message = match self.issue {
            EmptyCompound => "compound has no colliders",
            TooManyChildren => "compound exceeds the collider child limit",
            InvalidCollider => "collider material or local pose is invalid",
            InvalidDensity => "density must be finite and in 0..=1e6",
            NonFiniteShape => "shape dimensions and coordinates must be finite",
            DegenerateShape => "shape radii and half extents must be positive",
            UnsupportedShape => "shape is incompatible with the requested dimension",
            ZeroMass => "compound has no positive-density children",
            MassUnderflow => "positive-density shape mass underflowed to zero",
            MassOverflow => "mass calculation overflowed",
            InverseMassOverflow => "reciprocal mass overflowed",
            CenterOverflow => "center-of-mass calculation overflowed",
            InertiaOverflow => "inertia calculation overflowed",
            DegenerateInertia => "inertia is nonpositive or numerically singular",
            InverseInertiaOverflow => "inverse inertia is not finite and positive",
        };
        if let Some(child) = self.child {
            write!(f, "mass properties, child {child}: {message}")
        } else {
            write!(f, "mass properties: {message}")
        }
    }
}

impl std::error::Error for MassPropertyError {}

/// Checked body-local mass and inertia, computed using the solver's formulas.
///
/// The inertia is about the combined center of mass. Inspect a collider without
/// inserting it into a world:
///
/// ```
/// # #[cfg(feature = "dim3")] {
/// use ferravex_physics::{Collider, d3::ColliderBuilder};
/// let collider: Collider = ColliderBuilder::cuboid(1., 2., 3.).density(2.).into();
/// let properties = collider.try_mass_properties(false).unwrap();
/// assert_eq!(properties.mass(), 96.);
/// assert_eq!(properties.local_center_of_mass(), ferravex_math::Vec3::ZERO);
/// let (diagonal, off_diagonal) = properties.inertia();
/// assert_eq!(diagonal, ferravex_math::Vec3::new(416., 320., 160.));
/// assert_eq!(off_diagonal, ferravex_math::Vec3::ZERO);
/// let (inverse, _) = properties.inverse_inertia();
/// assert!((inverse.x * diagonal.x - 1.).abs() < 1e-6);
/// # }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct MassProperties {
    properties: Mass,
    inertia: Tensor,
}

impl MassProperties {
    /// Compute a compound without allocating or mutating its colliders.
    ///
    /// Uses at most two passes over the same ordered inputs; clones of the
    /// iterator must yield the same colliders. At most [`crate::MAX_CHILDREN`]
    /// children are accepted. Zero-density children contribute no mass; sensors
    /// contribute normally, matching world insertion. `planar` selects area
    /// density and Z-axis rotation instead of volume density and 3D rotation.
    /// The 3D solver's normalized determinant cutoff (`1e-10`) is retained.
    ///
    /// Invalid geometry, nonfinite intermediate results, underflowed mass and
    /// unrepresentable reciprocals return a diagnostic. No input is clamped.
    pub fn try_from_colliders<'a>(
        children: impl Iterator<Item = &'a Collider> + Clone,
        planar: bool,
    ) -> Result<Self, MassPropertyError> {
        checked_compound(children, planar)
    }

    /// The finite, positive total mass.
    pub fn mass(&self) -> f32 {
        self.properties.mass
    }

    /// The center of mass relative to the authored body origin.
    pub fn local_center_of_mass(&self) -> Vec3 {
        self.properties.center
    }

    /// Symmetric inertia as `(xx/yy/zz, xy/xz/yz)` about the center of mass.
    /// Only `zz` is active for planar bodies.
    pub fn inertia(&self) -> (Vec3, Vec3) {
        (self.inertia.diagonal, self.inertia.off)
    }

    /// Inverse inertia as `(xx/yy/zz, xy/xz/yz)` in body-local coordinates.
    /// Planar bodies have only `zz`; all other inverse entries are zero.
    pub fn inverse_inertia(&self) -> (Vec3, Vec3) {
        (
            self.properties.inverse.diagonal,
            self.properties.inverse.off,
        )
    }
}

impl Collider {
    /// Check this collider's mass and inertia without inserting it into a world.
    ///
    /// `planar` must match the intended world's dimension. A zero-density
    /// collider is valid as part of a compound but cannot supply mass by itself.
    ///
    /// ```
    /// # #[cfg(feature = "dim2")] {
    /// use ferravex_physics::{Collider, d2::ColliderBuilder};
    /// let collider: Collider = ColliderBuilder::ball(1.).density(0.).into();
    /// let error = collider.try_mass_properties(true).unwrap_err();
    /// assert_eq!(error.child, None);
    /// assert!(error.to_string().contains("positive-density"));
    /// # }
    /// ```
    pub fn try_mass_properties(&self, planar: bool) -> Result<MassProperties, MassPropertyError> {
        MassProperties::try_from_colliders(std::iter::once(self), planar)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Tensor {
    pub diagonal: Vec3,
    pub off: Vec3,
}
impl Tensor {
    pub const ZERO: Self = Self {
        diagonal: Vec3::ZERO,
        off: Vec3::ZERO,
    };
    pub fn diagonal(v: Vec3) -> Self {
        Self {
            diagonal: v,
            off: Vec3::ZERO,
        }
    }
    pub fn outer(v: Vec3) -> Self {
        Self {
            diagonal: v * v,
            off: Vec3::new(v.x * v.y, v.x * v.z, v.y * v.z),
        }
    }
    pub fn apply(self, v: Vec3) -> Vec3 {
        let d = self.diagonal * v;
        if self.off == Vec3::ZERO {
            return d;
        }
        d + Vec3::new(
            self.off.x * v.y + self.off.y * v.z,
            self.off.x * v.x + self.off.z * v.z,
            self.off.y * v.x + self.off.z * v.y,
        )
    }
    pub fn rotated(self, q: Quat) -> Self {
        let a = q * self.apply(q.conjugate() * Vec3::X);
        let b = q * self.apply(q.conjugate() * Vec3::Y);
        let c = q * self.apply(q.conjugate() * Vec3::Z);
        Self {
            diagonal: Vec3::new(a.x, b.y, c.z),
            off: Vec3::new((a.y + b.x) * 0.5, (a.z + c.x) * 0.5, (b.z + c.y) * 0.5),
        }
    }
    pub fn inverse(self, planar: bool) -> Result<Self, MassPropertyIssue> {
        if !self.diagonal.is_finite() || !self.off.is_finite() {
            return Err(MassPropertyIssue::InertiaOverflow);
        }
        if planar {
            if self.diagonal.z <= 0. {
                return Err(MassPropertyIssue::DegenerateInertia);
            }
            let z = 1. / self.diagonal.z;
            return if z.is_finite() && z > 0. {
                Ok(Self::diagonal(Vec3::Z * z))
            } else {
                Err(MassPropertyIssue::InverseInertiaOverflow)
            };
        }
        let scale = self
            .diagonal
            .abs()
            .max_element()
            .max(self.off.abs().max_element());
        if !scale.is_finite() || scale <= 0. {
            return Err(MassPropertyIssue::DegenerateInertia);
        }
        let m = if (1. / scale).is_finite() {
            self * (1. / scale)
        } else {
            Self {
                diagonal: self.diagonal / scale,
                off: self.off / scale,
            }
        };
        let d = m.diagonal;
        let o = m.off;
        // A positive determinant alone also admits two negative eigenvalues.
        if d.x <= 0. || d.y <= 0. || d.z <= 0. || d.x * d.y - o.x * o.x <= 0. {
            return Err(MassPropertyIssue::DegenerateInertia);
        }
        let a = Vec3::new(d.x, o.x, o.y);
        let b = Vec3::new(o.x, d.y, o.z);
        let c = Vec3::new(o.y, o.z, d.z);
        let determinant = a.dot(b.cross(c));
        if !determinant.is_finite() || determinant <= 1e-10 {
            return Err(MassPropertyIssue::DegenerateInertia);
        }
        let denominator = determinant * scale;
        let divide = |v: Vec3| {
            if denominator.is_finite() && denominator > 0. {
                v / denominator
            } else {
                (v / determinant) / scale
            }
        };
        let r0 = divide(b.cross(c));
        let r1 = divide(c.cross(a));
        let r2 = divide(a.cross(b));
        let average = |a: f32, b: f32| {
            if (a + b).is_finite() {
                (a + b) * 0.5
            } else {
                a * 0.5 + b * 0.5
            }
        };
        let inverse = Self {
            diagonal: Vec3::new(r0.x, r1.y, r2.z),
            off: Vec3::new(
                average(r0.y, r1.x),
                average(r0.z, r2.x),
                average(r1.z, r2.y),
            ),
        };
        if !inverse.diagonal.is_finite()
            || !inverse.off.is_finite()
            || inverse.diagonal.min_element() <= 0.
        {
            return Err(MassPropertyIssue::InverseInertiaOverflow);
        }
        Ok(inverse)
    }
}
impl std::ops::Add for Tensor {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self {
            diagonal: self.diagonal + b.diagonal,
            off: self.off + b.off,
        }
    }
}
impl std::ops::Sub for Tensor {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        Self {
            diagonal: self.diagonal - b.diagonal,
            off: self.off - b.off,
        }
    }
}
impl std::ops::Mul<f32> for Tensor {
    type Output = Self;
    fn mul(self, s: f32) -> Self {
        Self {
            diagonal: self.diagonal * s,
            off: self.off * s,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Mass {
    pub mass: f32,
    pub center: Vec3,
    pub inverse: Tensor,
}

fn validate_shape(shape: Shape, _planar: bool) -> Result<(), MassPropertyIssue> {
    let positive = |v: f32| {
        if !v.is_finite() {
            Err(MassPropertyIssue::NonFiniteShape)
        } else if v <= 0. {
            Err(MassPropertyIssue::DegenerateShape)
        } else {
            Ok(())
        }
    };
    match shape {
        Shape::Ball(radius) => positive(radius),
        Shape::Box(half) => {
            positive(half.x)?;
            positive(half.y)?;
            positive(half.z)
        }
        #[cfg(feature = "dim2")]
        Shape::Capsule { a, b, radius } => {
            positive(radius)?;
            if !a.is_finite() || !b.is_finite() {
                return Err(MassPropertyIssue::NonFiniteShape);
            }
            if _planar && (a.z != 0. || b.z != 0.) {
                return Err(MassPropertyIssue::UnsupportedShape);
            }
            Ok(())
        }
        #[cfg(feature = "dim3")]
        Shape::Cylinder {
            half_height,
            radius,
        } => {
            positive(half_height)?;
            positive(radius)?;
            if _planar {
                Err(MassPropertyIssue::UnsupportedShape)
            } else {
                Ok(())
            }
        }
    }
}

fn primitive(
    shape: Shape,
    density: f32,
    planar: bool,
) -> Result<(f32, Vec3, Tensor), MassPropertyIssue> {
    if !density.is_finite() || !(0. ..=1e6).contains(&density) {
        return Err(MassPropertyIssue::InvalidDensity);
    }
    validate_shape(shape, planar)?;
    // Massless collision geometry must not introduce 0 * infinity into a
    // compound. Still validate its dimensions before ignoring its contribution.
    if density == 0. {
        return Ok((0., Vec3::ZERO, Tensor::ZERO));
    }
    let pi = std::f32::consts::PI;
    let (mass, center, inertia) = match shape {
        Shape::Ball(r) => {
            let mass = if planar {
                pi * r * r * density
            } else {
                4. / 3. * pi * r * r * r * density
            };
            (
                mass,
                Vec3::ZERO,
                Tensor::diagonal(if planar {
                    Vec3::Z * (0.5 * mass * r * r)
                } else {
                    Vec3::splat(0.4 * mass * r * r)
                }),
            )
        }
        Shape::Box(h) => {
            let mass = if planar {
                4. * h.x * h.y * density
            } else {
                8. * h.x * h.y * h.z * density
            };
            (
                mass,
                Vec3::ZERO,
                Tensor::diagonal(if planar {
                    Vec3::Z * (mass * (h.x * h.x + h.y * h.y) / 3.)
                } else {
                    Vec3::new(
                        h.y * h.y + h.z * h.z,
                        h.x * h.x + h.z * h.z,
                        h.x * h.x + h.y * h.y,
                    ) * mass
                        / 3.
                }),
            )
        }
        #[cfg(feature = "dim2")]
        Shape::Capsule { a, b, radius: r } => {
            let length = a.distance(b);
            let center = (a + b) * 0.5;
            if planar {
                let mr = 2. * r * length * density;
                let ms = pi * r * r * density;
                let inertia = mr * (length * length + 4. * r * r) / 12.
                    + ms * (0.5 * r * r + 0.25 * length * length + 4. * length * r / (3. * pi));
                (mr + ms, center, Tensor::diagonal(Vec3::Z * inertia))
            } else {
                let mc = pi * r * r * length * density;
                let ms = 4. / 3. * pi * r * r * r * density;
                let parallel = 0.5 * mc * r * r + 0.4 * ms * r * r;
                let perpendicular = mc * (3. * r * r + length * length) / 12.
                    + ms * (0.4 * r * r + length * length / 4. + 3. * length * r / 8.);
                let axis = (b - a).try_normalize().unwrap_or(Vec3::Y);
                (
                    mc + ms,
                    center,
                    Tensor::diagonal(Vec3::splat(perpendicular))
                        + Tensor::outer(axis) * (parallel - perpendicular),
                )
            }
        }
        #[cfg(feature = "dim3")]
        Shape::Cylinder {
            half_height: h,
            radius: r,
        } => {
            if planar {
                return Err(MassPropertyIssue::UnsupportedShape);
            }
            let mass = 2. * pi * r * r * h * density;
            (
                mass,
                Vec3::ZERO,
                Tensor::diagonal(
                    Vec3::new(r * r / 4. + h * h / 3., r * r / 2., r * r / 4. + h * h / 3.) * mass,
                ),
            )
        }
    };
    if !mass.is_finite() {
        return Err(MassPropertyIssue::MassOverflow);
    }
    if mass <= 0. {
        return Err(MassPropertyIssue::MassUnderflow);
    }
    if !center.is_finite() {
        return Err(MassPropertyIssue::CenterOverflow);
    }
    if !inertia.diagonal.is_finite() || !inertia.off.is_finite() {
        return Err(MassPropertyIssue::InertiaOverflow);
    }
    Ok((mass, center, inertia))
}
pub(crate) fn compound<'a>(
    children: impl Iterator<Item = &'a Collider> + Clone,
    planar: bool,
) -> Result<Mass, PhysicsError> {
    MassProperties::try_from_colliders(children, planar)
        .map(|p| p.properties)
        .map_err(PhysicsError::from)
}

fn checked_compound<'a>(
    children: impl Iterator<Item = &'a Collider> + Clone,
    planar: bool,
) -> Result<MassProperties, MassPropertyError> {
    let mut mass = 0.;
    let mut center = Vec3::ZERO;
    let mut any = false;
    for (child, c) in children.clone().enumerate() {
        let error = |issue| MassPropertyError {
            child: Some(child),
            issue,
        };
        if child >= crate::MAX_CHILDREN {
            return Err(error(MassPropertyIssue::TooManyChildren));
        }
        any = true;
        let (m, p, _) = primitive(c.shape, c.density, planar).map_err(error)?;
        if !c.valid(planar) {
            return Err(error(MassPropertyIssue::InvalidCollider));
        }
        if m == 0. {
            continue;
        }
        mass += m;
        if !mass.is_finite() {
            return Err(error(MassPropertyIssue::MassOverflow));
        }
        center += (c.local_translation + c.local_rotation * p) * m;
        if !center.is_finite() {
            return Err(error(MassPropertyIssue::CenterOverflow));
        }
    }
    if !any {
        return Err(MassPropertyIssue::EmptyCompound.into());
    }
    if mass <= 0. {
        return Err(MassPropertyIssue::ZeroMass.into());
    }
    if !(1. / mass).is_finite() {
        return Err(MassPropertyIssue::InverseMassOverflow.into());
    }
    center /= mass;
    if !center.is_finite() {
        return Err(MassPropertyIssue::CenterOverflow.into());
    }
    let mut inertia = Tensor::ZERO;
    for (child, c) in children.enumerate() {
        let error = |issue| MassPropertyError {
            child: Some(child),
            issue,
        };
        // Bound both passes even for a caller-supplied inconsistent iterator.
        if child >= crate::MAX_CHILDREN {
            return Err(error(MassPropertyIssue::TooManyChildren));
        }
        let (m, p, i) = primitive(c.shape, c.density, planar).map_err(error)?;
        if m == 0. {
            continue;
        }
        let delta = c.local_translation + c.local_rotation * p - center;
        inertia = inertia
            + i.rotated(c.local_rotation)
            + (Tensor::diagonal(Vec3::splat(delta.length_squared())) - Tensor::outer(delta)) * m;
        if !inertia.diagonal.is_finite() || !inertia.off.is_finite() {
            return Err(error(MassPropertyIssue::InertiaOverflow));
        }
    }
    let inverse = inertia.inverse(planar)?;
    Ok(MassProperties {
        properties: Mass {
            mass,
            center,
            inverse,
        },
        inertia,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= expected.abs().max(1.) * 2e-6,
            "{actual} != {expected}"
        );
    }

    fn check_diagonal(properties: MassProperties, mass: f32, diagonal: Vec3, planar: bool) {
        close(properties.mass(), mass);
        assert_eq!(properties.local_center_of_mass(), Vec3::ZERO);
        assert_eq!(properties.inertia().1, Vec3::ZERO);
        assert_eq!(properties.inverse_inertia().1, Vec3::ZERO);
        let actual = properties.inertia().0;
        let inverse = properties.inverse_inertia().0;
        for (a, e) in actual.to_array().into_iter().zip(diagonal.to_array()) {
            close(a, e);
        }
        if planar {
            assert_eq!(inverse.x, 0.);
            assert_eq!(inverse.y, 0.);
            close(inverse.z * diagonal.z, 1.);
        } else {
            for product in (inverse * diagonal).to_array() {
                close(product, 1.);
            }
        }
    }

    fn failure(
        children: &[Collider],
        planar: bool,
        child: Option<usize>,
        issue: MassPropertyIssue,
    ) {
        let error = MassProperties::try_from_colliders(children.iter(), planar).unwrap_err();
        assert_eq!(error, MassPropertyError { child, issue });
    }

    #[test]
    fn ball_and_box_match_analytic_mass_and_inertia_in_both_dimensions() {
        let mut ball = Collider::new(Shape::Ball(2.));
        ball.density = 3.;
        let disk_mass = 12. * std::f32::consts::PI;
        check_diagonal(
            ball.try_mass_properties(true).unwrap(),
            disk_mass,
            Vec3::Z * (disk_mass * 2.),
            true,
        );
        let sphere_mass = 32. * std::f32::consts::PI;
        check_diagonal(
            ball.try_mass_properties(false).unwrap(),
            sphere_mass,
            Vec3::splat(sphere_mass * 1.6),
            false,
        );

        let mut cuboid = Collider::new(Shape::Box(Vec3::new(1., 2., 3.)));
        cuboid.density = 2.;
        check_diagonal(
            cuboid.try_mass_properties(true).unwrap(),
            16.,
            Vec3::Z * (80. / 3.),
            true,
        );
        check_diagonal(
            cuboid.try_mass_properties(false).unwrap(),
            96.,
            Vec3::new(416., 320., 160.),
            false,
        );
    }

    #[cfg(feature = "dim3")]
    #[test]
    fn cylinder_matches_analytic_axial_and_transverse_moments() {
        let cylinder = Collider::new(Shape::Cylinder {
            half_height: 3.,
            radius: 2.,
        });
        let mass = 24. * std::f32::consts::PI;
        check_diagonal(
            cylinder.try_mass_properties(false).unwrap(),
            mass,
            Vec3::new(4., 2., 4.) * mass,
            false,
        );
        failure(
            &[cylinder],
            true,
            Some(0),
            MassPropertyIssue::UnsupportedShape,
        );
    }

    #[cfg(feature = "dim2")]
    #[test]
    fn capsule_matches_stadium_and_solid_capsule_moments() {
        let capsule = Collider::new(Shape::Capsule {
            a: -Vec3::X,
            b: Vec3::X,
            radius: 1.,
        });
        let pi = std::f32::consts::PI;
        check_diagonal(
            capsule.try_mass_properties(true).unwrap(),
            4. + pi,
            Vec3::Z * (16. / 3. + 1.5 * pi),
            true,
        );
        check_diagonal(
            capsule.try_mass_properties(false).unwrap(),
            10. / 3. * pi,
            Vec3::new(23. / 15., 121. / 30., 121. / 30.) * pi,
            false,
        );
    }

    #[cfg(feature = "dim2")]
    #[test]
    fn coincident_capsule_endpoints_are_a_valid_offset_ball() {
        let mut capsule = Collider::new(Shape::Capsule {
            a: Vec3::new(3., 4., 0.),
            b: Vec3::new(3., 4., 0.),
            radius: 0.5,
        });
        capsule.local_rotation = Quat::from_rotation_z(0.5);
        capsule.local_translation = Vec3::X;
        let center = capsule.local_translation + capsule.local_rotation * Vec3::new(3., 4., 0.);
        let ball = Collider::new(Shape::Ball(0.5));
        for planar in [true, false] {
            let properties = capsule.try_mass_properties(planar).unwrap();
            let expected = ball.try_mass_properties(planar).unwrap();
            assert!(properties.local_center_of_mass().distance(center) < 1e-6);
            close(properties.mass(), expected.mass());
            for (a, b) in properties
                .inertia()
                .0
                .to_array()
                .into_iter()
                .zip(expected.inertia().0.to_array())
            {
                close(a, b);
            }
        }
    }

    #[test]
    fn compound_matches_weighted_center_and_parallel_axis_theorem() {
        let mut a = Collider::new(Shape::Ball(1.));
        a.local_translation = -Vec3::X;
        let mut b = Collider::new(Shape::Ball(1.));
        b.local_translation = Vec3::X;
        b.density = 3.;
        let children = [a, b];
        let properties = MassProperties::try_from_colliders(children.iter(), false).unwrap();
        let mass = 16. / 3. * std::f32::consts::PI;
        close(properties.mass(), mass);
        close(properties.local_center_of_mass().x, 0.5);
        close(properties.inertia().0.x, mass * 0.4);
        close(properties.inertia().0.y, mass * (0.4 + 0.75));
        close(properties.inertia().0.z, mass * (0.4 + 0.75));
        assert_eq!(properties.inertia().1, Vec3::ZERO);
    }

    #[test]
    fn rotated_inertia_and_its_inverse_preserve_off_diagonal_response() {
        let mut collider = Collider::new(Shape::Box(Vec3::new(2., 0.5, 0.25)));
        collider.local_rotation = Quat::from_rotation_z(0.6);
        let q = collider.local_rotation;
        let properties = collider.try_mass_properties(false).unwrap();
        assert!(properties.inertia().1.x.abs() > 0.1);
        let moment = Vec3::new(0.3125, 4.0625, 4.25) * properties.mass() / 3.;
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            let expected = q * ((q.conjugate() * axis) / moment);
            let actual = properties.properties.inverse.apply(axis);
            assert!(actual.distance(expected) < 1e-5);
            assert!(properties.inertia.apply(actual).distance(axis) < 1e-5);
        }
    }

    #[test]
    fn distinguishes_empty_zero_density_and_child_capacity() {
        failure(&[], false, None, MassPropertyIssue::EmptyCompound);
        let mut massless = Collider::new(Shape::Ball(1.));
        massless.density = 0.;
        failure(&[massless], false, None, MassPropertyIssue::ZeroMass);
        let children = vec![Collider::new(Shape::Ball(1.)); crate::MAX_CHILDREN + 1];
        MassProperties::try_from_colliders(children[..crate::MAX_CHILDREN].iter(), false).unwrap();
        failure(
            &children,
            false,
            Some(crate::MAX_CHILDREN),
            MassPropertyIssue::TooManyChildren,
        );
    }

    #[test]
    fn density_errors_report_the_input_child() {
        for density in [-1., f32::NAN, f32::INFINITY, 1_000_001.] {
            let mut invalid = Collider::new(Shape::Ball(1.));
            invalid.density = density;
            let children = [Collider::new(Shape::Ball(1.)), invalid];
            failure(&children, false, Some(1), MassPropertyIssue::InvalidDensity);
            let error = MassProperties::try_from_colliders(children.iter(), false).unwrap_err();
            assert_eq!(PhysicsError::from(error), PhysicsError::InvalidCollider);
            assert_eq!(
                error.to_string(),
                "mass properties, child 1: density must be finite and in 0..=1e6"
            );
        }
    }

    #[test]
    fn shape_validation_rejects_nonfinite_and_degenerate_dimensions() {
        for shape in [
            Shape::Ball(0.),
            Shape::Ball(-1.),
            Shape::Box(Vec3::new(1., 0., 1.)),
        ] {
            failure(
                &[Collider::new(shape)],
                false,
                Some(0),
                MassPropertyIssue::DegenerateShape,
            );
        }
        for shape in [
            Shape::Ball(f32::NAN),
            Shape::Ball(f32::INFINITY),
            Shape::Box(Vec3::splat(f32::INFINITY)),
        ] {
            failure(
                &[Collider::new(shape)],
                false,
                Some(0),
                MassPropertyIssue::NonFiniteShape,
            );
        }
        let mut invalid_pose = Collider::new(Shape::Ball(1.));
        invalid_pose.local_translation = Vec3::Z;
        failure(
            &[invalid_pose],
            true,
            Some(0),
            MassPropertyIssue::InvalidCollider,
        );
    }

    #[test]
    fn mass_and_inertia_overflow_identify_the_child_and_stage() {
        failure(
            &[Collider::new(Shape::Ball(1e20))],
            false,
            Some(0),
            MassPropertyIssue::MassOverflow,
        );
        failure(
            &[Collider::new(Shape::Ball(1e9))],
            false,
            Some(0),
            MassPropertyIssue::InertiaOverflow,
        );
        let child = Collider::new(Shape::Ball(4e7));
        child.try_mass_properties(false).unwrap();
        failure(
            &[child.clone(), child],
            false,
            Some(1),
            MassPropertyIssue::InertiaOverflow,
        );
    }

    #[cfg(feature = "dim2")]
    #[test]
    fn capsule_center_and_parallel_axis_overflow_are_diagnosed() {
        let capsule_at = |x| {
            Collider::new(Shape::Capsule {
                a: Vec3::X * x,
                b: Vec3::X * x,
                radius: 1.,
            })
        };
        failure(
            &[capsule_at(f32::MAX)],
            false,
            Some(0),
            MassPropertyIssue::CenterOverflow,
        );
        failure(
            &[capsule_at(1e20), capsule_at(-1e20)],
            false,
            Some(0),
            MassPropertyIssue::InertiaOverflow,
        );
    }

    #[test]
    fn underflow_and_unrepresentable_reciprocals_are_distinct() {
        failure(
            &[Collider::new(Shape::Ball(f32::MIN_POSITIVE))],
            true,
            Some(0),
            MassPropertyIssue::MassUnderflow,
        );
        let mut tiny_mass = Collider::new(Shape::Ball(1.));
        tiny_mass.density = f32::from_bits(1);
        failure(
            &[tiny_mass],
            true,
            None,
            MassPropertyIssue::InverseMassOverflow,
        );
        failure(
            &[Collider::new(Shape::Ball(1e-12))],
            true,
            None,
            MassPropertyIssue::DegenerateInertia,
        );
        failure(
            &[Collider::new(Shape::Ball(1e-10))],
            true,
            None,
            MassPropertyIssue::InverseInertiaOverflow,
        );
    }

    #[test]
    fn zero_density_children_do_not_contaminate_valid_mass_arithmetic() {
        let ball = Collider::new(Shape::Ball(1.));
        let expected = ball.try_mass_properties(false).unwrap();
        let mut massless = Collider::new(Shape::Ball(f32::MAX));
        massless.density = 0.;
        massless.local_translation = Vec3::X * 1e6;
        let children = [massless.clone(), ball];
        let properties = MassProperties::try_from_colliders(children.iter(), false).unwrap();
        assert_eq!(properties.mass(), expected.mass());
        assert_eq!(
            properties.local_center_of_mass(),
            expected.local_center_of_mass()
        );
        assert_eq!(properties.inertia(), expected.inertia());
        assert_eq!(properties.inverse_inertia(), expected.inverse_inertia());
        massless.shape = Shape::Ball(0.);
        failure(
            &[massless],
            false,
            Some(0),
            MassPropertyIssue::DegenerateShape,
        );
    }

    #[test]
    fn tensor_rejects_indefinite_singular_and_nonfinite_inertia() {
        for diagonal in [
            Vec3::new(-1., -1., 1.),
            Vec3::new(0., 1., 1.),
            Vec3::new(1e-12, 1., 1.),
        ] {
            assert_eq!(
                Tensor::diagonal(diagonal).inverse(false).unwrap_err(),
                MassPropertyIssue::DegenerateInertia
            );
        }
        let indefinite = Tensor {
            diagonal: Vec3::ONE,
            off: Vec3::splat(2.),
        };
        assert_eq!(
            indefinite.inverse(false).unwrap_err(),
            MassPropertyIssue::DegenerateInertia
        );
        let nonfinite = Tensor {
            diagonal: Vec3::ONE,
            off: Vec3::X * f32::INFINITY,
        };
        for planar in [true, false] {
            assert_eq!(
                nonfinite.inverse(planar).unwrap_err(),
                MassPropertyIssue::InertiaOverflow
            );
        }
    }

    #[test]
    fn tensor_inversion_handles_extreme_finite_scales() {
        for scale in [4e-39, 1., f32::MAX] {
            let inverse = Tensor::diagonal(Vec3::splat(scale)).inverse(false).unwrap();
            for product in (inverse.diagonal * scale).to_array() {
                close(product, 1.);
            }
            assert_eq!(inverse.off, Vec3::ZERO);
        }
        assert_eq!(
            Tensor::diagonal(Vec3::splat(f32::from_bits(1)))
                .inverse(false)
                .unwrap_err(),
            MassPropertyIssue::InverseInertiaOverflow
        );
        // Each inverse off-diagonal is representable even though adding the
        // two symmetric entries before averaging would overflow.
        let tensor = Tensor {
            diagonal: Vec3::splat(2e-38),
            off: Vec3::new(1.8e-38, 0., 0.),
        };
        let inverse = tensor.inverse(false).unwrap();
        assert!(inverse.off.x < -2e38);
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            assert!(tensor.apply(inverse.apply(axis)).distance(axis) < 1e-5);
        }
    }

    #[cfg(feature = "dim2")]
    #[test]
    fn rejected_mass_construction_leaves_world_unchanged() {
        use crate::d2::{ColliderBuilder, PhysicsWorld, RigidBodyBuilder};
        let mut world = PhysicsWorld::new([0.; 2]);
        assert_eq!(
            world.insert_compound(RigidBodyBuilder::dynamic(), [ColliderBuilder::ball(1e-10)]),
            Err(PhysicsError::InvalidMass)
        );
        assert!(world.bodies.is_empty());
        assert!(world.colliders.is_empty());
    }
}
