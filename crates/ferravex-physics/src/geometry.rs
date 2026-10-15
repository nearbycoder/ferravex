use crate::body::Shape;
#[derive(Clone, Copy)]
pub(crate) struct Pose {
    pub position: Vec3,
    pub rotation: Quat,
    pub planar: bool,
}
use ferravex_math::{Quat, Vec3};

pub(crate) const MARGIN: f32 = 0.001;
#[derive(Clone, Copy, Debug)]
pub(crate) struct Point {
    pub normal: Vec3,
    pub point: Vec3,
    pub depth: f32,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}
impl Bounds {
    pub fn intersects(self, b: Self) -> bool {
        (0..3).all(|i| self.min[i] <= b.max[i] && self.max[i] >= b.min[i])
    }
}
pub(crate) fn bounds(shape: Shape, p: Vec3, q: Quat) -> Bounds {
    let (center, h) = match shape {
        Shape::Ball(r) => (p, Vec3::ONE * r),
        Shape::Box(h) => (
            p,
            (q * Vec3::X).abs() * h.x + (q * Vec3::Y).abs() * h.y + (q * Vec3::Z).abs() * h.z,
        ),
        #[cfg(feature = "dim2")]
        Shape::Capsule { a, b, radius } => {
            let a = p + q * a;
            let b = p + q * b;
            ((a + b) * 0.5, (a - b).abs() * 0.5 + Vec3::ONE * radius)
        }
        #[cfg(feature = "dim3")]
        Shape::Cylinder {
            half_height,
            radius,
        } => {
            let y = q * Vec3::Y;
            let h = Vec3::new(
                (1. - y.x * y.x).max(0.).sqrt(),
                (1. - y.y * y.y).max(0.).sqrt(),
                (1. - y.z * y.z).max(0.).sqrt(),
            ) * radius
                + y.abs() * half_height;
            (p, h)
        }
    };
    Bounds {
        min: center - h - Vec3::ONE * MARGIN,
        max: center + h + Vec3::ONE * MARGIN,
    }
}
/// Signed distance and outward normal for the supported convex shapes.
pub(crate) fn distance(shape: Shape, p: Vec3, q: Quat, point: Vec3, planar: bool) -> (f32, Vec3) {
    let v = q.conjugate() * (point - p);
    let (d, n) = match shape {
        Shape::Ball(r) => {
            let length = v.length();
            (length - r, v.try_normalize().unwrap_or(Vec3::Y))
        }
        Shape::Box(h) => {
            let closest = v.clamp(-h, h);
            let delta = v - closest;
            if delta.length_squared() > 1e-12 {
                (delta.length(), delta.normalize())
            } else {
                let mut axis = 0;
                let mut gap = h.x - v.x.abs();
                for i in 1..if planar { 2 } else { 3 } {
                    if h[i] - v[i].abs() < gap {
                        axis = i;
                        gap = h[i] - v[i].abs();
                    }
                }
                let mut n = Vec3::ZERO;
                n[axis] = if v[axis] < 0. { -1. } else { 1. };
                (-gap, n)
            }
        }
        #[cfg(feature = "dim2")]
        Shape::Capsule { a, b, radius } => {
            let ab = b - a;
            let denom = ab.length_squared();
            let t = if denom > 1e-12 {
                ((v - a).dot(ab) / denom).clamp(0., 1.)
            } else {
                0.
            };
            let delta = v - (a + ab * t);
            (
                delta.length() - radius,
                delta.try_normalize().unwrap_or(Vec3::Y),
            )
        }
        #[cfg(feature = "dim3")]
        Shape::Cylinder {
            half_height,
            radius,
        } => {
            let radial = Vec3::new(v.x, 0., v.z);
            let length = radial.length();
            let radial_n = radial.try_normalize().unwrap_or(Vec3::X);
            let side = length - radius;
            let cap = v.y.abs() - half_height;
            if side > 0. || cap > 0. {
                let closest =
                    radial_n * length.min(radius) + Vec3::Y * v.y.clamp(-half_height, half_height);
                let delta = v - closest;
                (delta.length(), delta.try_normalize().unwrap_or(Vec3::Y))
            } else if side > cap {
                (side, radial_n)
            } else {
                (cap, Vec3::Y * if v.y < 0. { -1. } else { 1. })
            }
        }
    };
    (d, q * n)
}
/// Sphere sweep against a convex shape. Conservative advancement uses the signed
/// distance's supporting plane; no borrowed geometry code or shape approximation.
/// Target orientation is fixed for this individual substep.
pub(crate) fn sweep_ball(
    start: Vec3,
    end: Vec3,
    radius: f32,
    target: Shape,
    p: Vec3,
    q: Quat,
    planar: bool,
) -> Option<(f32, Vec3)> {
    let delta = end - start;
    let mut t = 0.;
    let (initial, _) = distance(target, p, q, start, planar);
    if initial - radius < -MARGIN {
        return None;
    } // Existing overlap belongs to the contact solver.
    for _ in 0..40 {
        let (distance, n) = distance(target, p, q, start + delta * t, planar);
        let gap = distance - radius;
        if gap <= 0.00005 {
            return Some((t, n));
        }
        let closing = -n.dot(delta);
        if closing <= 1e-9 {
            return None;
        }
        t += gap / closing;
        if t > 1. {
            return None;
        }
    }
    None // A grazing sweep may exhaust this finite iteration budget.
}

pub(crate) fn contacts(a: Shape, ca: &Pose, b: Shape, cb: &Pose, out: &mut Vec<Point>) {
    out.clear();
    match (a, b) {
        (Shape::Ball(radius), shape) => {
            ball_contact(ca.position, radius, shape, cb, ca.planar, false, out)
        }
        (shape, Shape::Ball(radius)) => {
            ball_contact(cb.position, radius, shape, ca, ca.planar, true, out)
        }
        (Shape::Box(ha), Shape::Box(hb)) => box_contacts(ha, ca, hb, cb, out),
        _ => {} // Capsule/cylinder pairs are outside the explicitly supported shape matrix.
    }
}
fn ball_contact(
    center: Vec3,
    radius: f32,
    shape: Shape,
    other: &Pose,
    planar: bool,
    reverse: bool,
    out: &mut Vec<Point>,
) {
    let (d, n) = distance(shape, other.position, other.rotation, center, planar);
    if d <= radius + MARGIN {
        let depth = (radius - d).max(0.);
        out.push(Point {
            normal: if reverse { n } else { -n },
            point: center - n * (radius - depth * 0.5),
            depth,
        });
    }
}
fn axes(q: Quat) -> [Vec3; 3] {
    [q * Vec3::X, q * Vec3::Y, q * Vec3::Z]
}
fn extent(axis: Vec3, basis: [Vec3; 3], h: Vec3) -> f32 {
    (0..3).map(|i| h[i] * axis.dot(basis[i]).abs()).sum()
}
fn box_contacts(ha: Vec3, a: &Pose, hb: Vec3, b: &Pose, out: &mut Vec<Point>) {
    let aa = axes(a.rotation);
    let ba = axes(b.rotation);
    let delta = b.position - a.position;
    let dimensions = if a.planar { 2 } else { 3 };
    let mut best = f32::INFINITY;
    let mut normal = Vec3::Y;
    let mut edge = false;
    let mut test = |axis: Vec3, is_edge: bool| -> bool {
        if axis.length_squared() < 1e-9 {
            return true;
        }
        let axis = axis.normalize();
        let overlap = extent(axis, aa, ha) + extent(axis, ba, hb) - delta.dot(axis).abs();
        if overlap < -MARGIN {
            return false;
        }
        // Favor face manifolds for almost parallel faces, avoiding unstable edge ties.
        if overlap + if is_edge { 0.001 } else { 0. } < best {
            best = overlap;
            normal = axis * if delta.dot(axis) < 0. { -1. } else { 1. };
            edge = is_edge;
        }
        true
    };
    for axis in aa.iter().take(dimensions).chain(ba.iter().take(dimensions)) {
        if !test(*axis, false) {
            return;
        }
    }
    if !a.planar {
        for x in aa {
            for y in ba {
                if !test(x.cross(y), true) {
                    return;
                }
            }
        }
    }
    if edge {
        let pa = support(a.position, aa, ha, normal);
        let pb = support(b.position, ba, hb, -normal);
        // Closest points on the supporting edges, rather than an arbitrary corner.
        let ia = (0..3)
            .min_by(|&i, &j| aa[i].dot(normal).abs().total_cmp(&aa[j].dot(normal).abs()))
            .unwrap();
        let ib = (0..3)
            .min_by(|&i, &j| ba[i].dot(normal).abs().total_cmp(&ba[j].dot(normal).abs()))
            .unwrap();
        let ac = pa - aa[ia] * ha[ia] * if aa[ia].dot(normal) < 0. { -1. } else { 1. };
        let bc = pb - ba[ib] * hb[ib] * if ba[ib].dot(-normal) < 0. { -1. } else { 1. };
        let (p, q) = closest_segments(
            ac - aa[ia] * ha[ia],
            ac + aa[ia] * ha[ia],
            bc - ba[ib] * hb[ib],
            bc + ba[ib] * hb[ib],
        );
        out.push(Point {
            normal,
            point: (p + q) * 0.5,
            depth: best.max(0.),
        });
        return;
    }
    let ia = (0..dimensions)
        .max_by(|&i, &j| aa[i].dot(normal).abs().total_cmp(&aa[j].dot(normal).abs()))
        .unwrap();
    let ib = (0..dimensions)
        .max_by(|&i, &j| ba[i].dot(normal).abs().total_cmp(&ba[j].dot(normal).abs()))
        .unwrap();
    let (reference, ra, rh, ri, incident, iaxes, ih, ref_normal) =
        if aa[ia].dot(normal).abs() >= ba[ib].dot(normal).abs() {
            (a, aa, ha, ia, b, ba, hb, normal)
        } else {
            (b, ba, hb, ib, a, aa, ha, -normal)
        };
    let rnormal = ra[ri] * if ra[ri].dot(ref_normal) < 0. { -1. } else { 1. };
    let center = reference.position + rnormal * rh[ri];
    let fi = (0..3)
        .max_by(|&i, &j| {
            iaxes[i]
                .dot(rnormal)
                .abs()
                .total_cmp(&iaxes[j].dot(rnormal).abs())
        })
        .unwrap();
    let fcenter =
        incident.position - iaxes[fi] * ih[fi] * if iaxes[fi].dot(rnormal) < 0. { -1. } else { 1. };
    let u = (fi + 1) % 3;
    let v = (fi + 2) % 3;
    let mut polygon = Polygon::new([
        fcenter + iaxes[u] * ih[u] + iaxes[v] * ih[v],
        fcenter - iaxes[u] * ih[u] + iaxes[v] * ih[v],
        fcenter - iaxes[u] * ih[u] - iaxes[v] * ih[v],
        fcenter + iaxes[u] * ih[u] - iaxes[v] * ih[v],
    ]);
    for side in (0..3).filter(|&i| i != ri) {
        for sign in [-1., 1.] {
            polygon = clip(
                &polygon,
                ra[side] * sign,
                reference.position.dot(ra[side] * sign) + rh[side],
            );
        }
    }
    for &p in polygon.as_slice() {
        let separation = (p - center).dot(rnormal);
        if separation <= MARGIN {
            let mut point = p - rnormal * (separation * 0.5);
            if a.planar {
                point.z = 0.;
            }
            if !out
                .iter()
                .any(|other| other.point.distance_squared(point) < 1e-8)
            {
                out.push(Point {
                    normal,
                    point,
                    depth: (-separation).max(0.),
                });
            }
        }
    }
    if out.is_empty() {
        out.push(Point {
            normal,
            point: (support(a.position, aa, ha, normal) + support(b.position, ba, hb, -normal))
                * 0.5,
            depth: best.max(0.),
        });
    }
}
fn support(p: Vec3, axes: [Vec3; 3], h: Vec3, n: Vec3) -> Vec3 {
    p + (0..3)
        .map(|i| axes[i] * h[i] * if axes[i].dot(n) < 0. { -1. } else { 1. })
        .fold(Vec3::ZERO, |a, b| a + b)
}
struct Polygon {
    points: [Vec3; 12],
    len: usize,
}
impl Polygon {
    fn new(points: [Vec3; 4]) -> Self {
        let mut p = Self {
            points: [Vec3::ZERO; 12],
            len: 4,
        };
        p.points[..4].copy_from_slice(&points);
        p
    }
    fn as_slice(&self) -> &[Vec3] {
        &self.points[..self.len]
    }
    fn push(&mut self, p: Vec3) {
        assert!(self.len < 12);
        self.points[self.len] = p;
        self.len += 1;
    }
}
fn clip(poly: &Polygon, normal: Vec3, offset: f32) -> Polygon {
    let poly = poly.as_slice();
    let mut out = Polygon {
        points: [Vec3::ZERO; 12],
        len: 0,
    };
    if poly.is_empty() {
        return out;
    }
    let mut prev = *poly.last().unwrap();
    let mut pd = prev.dot(normal) - offset;
    for &p in poly {
        let d = p.dot(normal) - offset;
        if (d <= 0.) != (pd <= 0.) {
            out.push(prev + (p - prev) * (pd / (pd - d)));
        }
        if d <= 0. {
            out.push(p);
        }
        prev = p;
        pd = d;
    }
    out
}
fn closest_segments(p: Vec3, q: Vec3, r: Vec3, s: Vec3) -> (Vec3, Vec3) {
    let a = q - p;
    let b = s - r;
    let d = p - r;
    let aa = a.dot(a);
    let bb = b.dot(b);
    let ab = a.dot(b);
    let ad = a.dot(d);
    let bd = b.dot(d);
    let denom = aa * bb - ab * ab;
    let mut u = if denom.abs() > 1e-9 {
        ((ab * bd - bb * ad) / denom).clamp(0., 1.)
    } else {
        0.
    };
    let v = if bb > 1e-9 {
        ((ab * u + bd) / bb).clamp(0., 1.)
    } else {
        0.
    };
    if aa > 1e-9 {
        u = ((ab * v - ad) / aa).clamp(0., 1.);
    }
    (p + a * u, r + b * v)
}
