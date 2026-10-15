//! Body-space symmetric inertia tensors and primitive/compound mass properties.
use crate::{
    PhysicsError,
    body::{Collider, Shape},
};
use ferravex_math::{Quat, Vec3};
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
    pub fn inverse(self, planar: bool) -> Result<Self, PhysicsError> {
        if planar {
            let z = 1. / self.diagonal.z;
            return if z.is_finite() && z > 0. {
                Ok(Self::diagonal(Vec3::Z * z))
            } else {
                Err(PhysicsError::InvalidMass)
            };
        }
        let scale = self
            .diagonal
            .abs()
            .max_element()
            .max(self.off.abs().max_element());
        if !scale.is_finite() || scale <= 0. {
            return Err(PhysicsError::InvalidMass);
        }
        let m = self * (1. / scale);
        let d = m.diagonal;
        let o = m.off;
        let a = Vec3::new(d.x, o.x, o.y);
        let b = Vec3::new(o.x, d.y, o.z);
        let c = Vec3::new(o.y, o.z, d.z);
        let determinant = a.dot(b.cross(c));
        if !determinant.is_finite() || determinant <= 1e-10 {
            return Err(PhysicsError::InvalidMass);
        }
        let r0 = b.cross(c) / (determinant * scale);
        let r1 = c.cross(a) / (determinant * scale);
        let r2 = a.cross(b) / (determinant * scale);
        let inverse = Self {
            diagonal: Vec3::new(r0.x, r1.y, r2.z),
            off: Vec3::new(
                (r0.y + r1.x) * 0.5,
                (r0.z + r2.x) * 0.5,
                (r1.z + r2.y) * 0.5,
            ),
        };
        if !inverse.diagonal.is_finite() || !inverse.off.is_finite() {
            return Err(PhysicsError::InvalidMass);
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
fn primitive(
    shape: Shape,
    density: f32,
    planar: bool,
) -> Result<(f32, Vec3, Tensor), PhysicsError> {
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
                return Err(PhysicsError::InvalidCollider);
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
    if !mass.is_finite() || !inertia.diagonal.is_finite() || !inertia.off.is_finite() {
        return Err(PhysicsError::InvalidMass);
    }
    Ok((mass, center, inertia))
}
pub(crate) fn compound<'a>(
    children: impl Iterator<Item = &'a Collider> + Clone,
    planar: bool,
) -> Result<Mass, PhysicsError> {
    let mut mass = 0.;
    let mut center = Vec3::ZERO;
    for c in children.clone() {
        if !c.valid(planar) {
            return Err(PhysicsError::InvalidCollider);
        }
        let (m, p, _) = primitive(c.shape, c.density, planar)?;
        mass += m;
        center += (c.local_translation + c.local_rotation * p) * m;
    }
    if !mass.is_finite() || mass <= 0. {
        return Err(PhysicsError::InvalidMass);
    }
    center /= mass;
    let mut inertia = Tensor::ZERO;
    for c in children {
        let (m, p, i) = primitive(c.shape, c.density, planar)?;
        let delta = c.local_translation + c.local_rotation * p - center;
        inertia = inertia
            + i.rotated(c.local_rotation)
            + (Tensor::diagonal(Vec3::splat(delta.length_squared())) - Tensor::outer(delta)) * m;
    }
    let inverse = inertia.inverse(planar)?;
    if !center.is_finite() {
        return Err(PhysicsError::InvalidMass);
    }
    Ok(Mass {
        mass,
        center,
        inverse,
    })
}
