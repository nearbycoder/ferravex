use super::*;
/// Exact closest pair between a line segment and an axis-aligned box. The squared
/// distance is quadratic between the segment's face crossings (at most 8 intervals).
fn segment_box(a: Vec3, b: Vec3, h: Vec3) -> (f32, Vec3) {
    let d = b - a;
    let mut ts = [0.; 8];
    ts[1] = 1.;
    let mut len = 2;
    for i in 0..3 {
        if d[i].abs() > 1e-12 {
            for face in [-h[i], h[i]] {
                let t = (face - a[i]) / d[i];
                if t > 0. && t < 1. {
                    ts[len] = t;
                    len += 1;
                }
            }
        }
    }
    ts[..len].sort_unstable_by(f32::total_cmp);
    let mut best = f32::INFINITY;
    let mut vector = Vec3::Y;
    let mut sample = |t: f32| {
        let p = a + d * t;
        let delta = p - p.clamp(-h, h);
        let distance = delta.length_squared();
        if distance < best {
            best = distance;
            vector = delta;
        }
    };
    for interval in ts[..len].windows(2) {
        let middle = (interval[0] + interval[1]) * 0.5;
        let p = a + d * middle;
        let mut numerator = 0.;
        let mut denominator = 0.;
        for i in 0..3 {
            if p[i].abs() > h[i] {
                let c = a[i] - p[i].signum() * h[i];
                numerator += c * d[i];
                denominator += d[i] * d[i];
            }
        }
        sample(interval[0]);
        sample(interval[1]);
        if denominator > 1e-20 {
            sample((-numerator / denominator).clamp(interval[0], interval[1]));
        }
    }
    if best > 1e-12 {
        let distance = best.sqrt();
        return (distance, vector / distance);
    }
    // Intersecting segment: a face separator supplies a conservative recovery.
    let mut depth = f32::INFINITY;
    let mut normal = Vec3::Y;
    for i in 0..3 {
        for sign in [-1., 1.] {
            let extent = if sign > 0. {
                a[i].min(b[i])
            } else {
                -a[i].max(b[i])
            };
            let push = h[i] - extent;
            if push < depth {
                depth = push;
                normal = Vec3::ZERO;
                normal[i] = sign;
            }
        }
    }
    (-depth, normal)
}
fn segment_point(a: Vec3, b: Vec3, p: Vec3) -> Vec3 {
    let d = b - a;
    let length = d.length_squared();
    a + d * if length > 1e-12 {
        ((p - a).dot(d) / length).clamp(0., 1.)
    } else {
        0.
    }
}
fn segment_segment(a: Vec3, b: Vec3, c: Vec3, d: Vec3) -> (Vec3, Vec3) {
    let u = b - a;
    let v = d - c;
    let w = a - c;
    let aa = u.dot(u);
    let bb = u.dot(v);
    let cc = v.dot(v);
    let dd = u.dot(w);
    let ee = v.dot(w);
    if aa < 1e-12 {
        return (a, segment_point(c, d, a));
    }
    if cc < 1e-12 {
        return (segment_point(a, b, c), c);
    }
    let denominator = aa * cc - bb * bb;
    let mut s = if denominator > 1e-12 {
        ((bb * ee - cc * dd) / denominator).clamp(0., 1.)
    } else {
        0.
    };
    let mut t = (bb * s + ee) / cc;
    if t < 0. {
        t = 0.;
        s = (-dd / aa).clamp(0., 1.);
    } else if t > 1. {
        t = 1.;
        s = ((bb - dd) / aa).clamp(0., 1.);
    }
    (a + u * s, c + v * t)
}
fn radial(vector: Vec3, radius: f32) -> (f32, Vec3) {
    let length = vector.length();
    (length - radius, vector.try_normalize().unwrap_or(Vec3::Y))
}
fn cylinder_distance(p: Vec3, half: f32, r: f32) -> (f32, Vec3) {
    let horizontal = Vec3::new(p.x, 0., p.z);
    let length = horizontal.length();
    let normal = horizontal.try_normalize().unwrap_or(Vec3::X);
    let side = length - r;
    let cap = p.y.abs() - half;
    if side > 0. || cap > 0. {
        radial(
            p - (normal * length.min(r) + Vec3::Y * p.y.clamp(-half, half)),
            0.,
        )
    } else if side > cap {
        (side, normal)
    } else {
        (cap, Vec3::Y * if p.y < 0. { -1. } else { 1. })
    }
}
pub(super) fn separation(
    shape: CharacterShape,
    feet: Vec3,
    o: Obstacle,
) -> Result<(f32, Vec3), CharacterError> {
    let inverse = o.rotation.conjugate();
    let result = match shape {
        CharacterShape::Capsule { radius, height } => {
            let a = inverse * (feet + Vec3::Y * radius - o.position);
            let b = inverse * (feet + Vec3::Y * (height - radius) - o.position);
            let (gap, normal) = match o.shape {
                ObstacleShape::Box { half } => segment_box(a, b, half),
                ObstacleShape::Ball { radius: r } => radial(segment_point(a, b, Vec3::ZERO), r),
                ObstacleShape::Capsule {
                    a: c,
                    b: d,
                    radius: r,
                } => {
                    let (p, q) = segment_segment(a, b, c, d);
                    radial(p - q, r)
                }
                ObstacleShape::Plane { normal } => (a.dot(normal).min(b.dot(normal)), normal),
                ObstacleShape::Cylinder {
                    half_height,
                    radius: r,
                } => {
                    // The convex distance along a segment is minimized with a fixed budget.
                    // Small residual overestimation is subtracted, preserving conservative sweeps.
                    let mut low = 0.;
                    let mut high = 1.;
                    let d = b - a;
                    for _ in 0..48 {
                        let m1 = (2. * low + high) / 3.;
                        let m2 = (low + 2. * high) / 3.;
                        if cylinder_distance(a + d * m1, half_height, r).0
                            < cylinder_distance(a + d * m2, half_height, r).0
                        {
                            high = m2
                        } else {
                            low = m1
                        }
                    }
                    let p = a + d * ((low + high) * 0.5);
                    let (g, n) = cylinder_distance(p, half_height, r);
                    (g - d.length() * (high - low), n)
                }
            };
            (gap - radius, o.rotation * normal)
        }
        CharacterShape::Box2 { half } => {
            // A planar terrain must not rotate out of its XY plane.
            if o.position.z.abs() > 1e-5 || (o.rotation * Vec3::Z - Vec3::Z).length_squared() > 1e-6
            {
                return Err(CharacterError::InvalidObstacle);
            }
            let center = shape.center(feet);
            match o.shape {
                ObstacleShape::Box { half: h } => {
                    let x = o.rotation * Vec3::X;
                    let y = o.rotation * Vec3::Y;
                    let delta = center - o.position;
                    let mut gap = f32::NEG_INFINITY;
                    let mut normal = Vec3::Y;
                    for axis in [Vec3::X, Vec3::Y, x, y] {
                        let value = delta.dot(axis);
                        let r = half.x * axis.x.abs()
                            + half.y * axis.y.abs()
                            + h.x * axis.dot(x).abs()
                            + h.y * axis.dot(y).abs();
                        let g = value.abs() - r;
                        if g > gap {
                            gap = g;
                            normal = axis * if value < 0. { -1. } else { 1. };
                        }
                    }
                    (gap, normal)
                }
                ObstacleShape::Plane { normal } => {
                    let n = o.rotation * normal;
                    if n.z.abs() > 1e-5 {
                        return Err(CharacterError::InvalidObstacle);
                    }
                    (
                        (center - o.position).dot(n) - half.x * n.x.abs() - half.y * n.y.abs(),
                        n,
                    )
                }
                ObstacleShape::Ball { radius } => {
                    let (g, n) = segment_box(
                        o.position - center,
                        o.position - center,
                        Vec3::new(half.x, half.y, 10000.),
                    );
                    (g - radius, -n)
                }
                ObstacleShape::Capsule { a, b, radius } => {
                    let (g, n) = segment_box(
                        o.position + o.rotation * a - center,
                        o.position + o.rotation * b - center,
                        Vec3::new(half.x, half.y, 10000.),
                    );
                    (g - radius, -n)
                }
                ObstacleShape::Cylinder { .. } => return Err(CharacterError::InvalidObstacle),
            }
        }
    };
    if !result.0.is_finite() || !result.1.is_finite() {
        return Err(CharacterError::InvalidObstacle);
    }
    Ok(result)
}
#[derive(Clone, Copy)]
pub(super) struct Hit {
    pub fraction: f32,
    pub normal: Vec3,
    pub obstacle: Obstacle,
}
pub(super) struct Queries<'a, T: Terrain + ?Sized> {
    pub world: &'a T,
    pub shape: CharacterShape,
    pub settings: CharacterSettings,
    pub candidates: usize,
    pub ignored: Option<u64>,
}
impl<T: Terrain + ?Sized> Queries<'_, T> {
    fn visit(
        &mut self,
        bounds: QueryBounds,
        mut visitor: impl FnMut(Obstacle) -> Result<(), CharacterError>,
    ) -> Result<(), CharacterError> {
        let mut error = None;
        let settings = self.settings;
        let ignored = self.ignored;
        let candidates = &mut self.candidates;
        self.world.visit(bounds, &mut |o| {
            if error.is_some() {
                return;
            }
            *candidates += 1;
            if *candidates > settings.max_candidates {
                error = Some(CharacterError::QueryBudget);
                return;
            }
            if o.sensor || o.membership & settings.membership_mask == 0 || ignored == Some(o.id) {
                return;
            }
            if !o.valid() {
                error = Some(CharacterError::InvalidObstacle);
                return;
            }
            if let Err(e) = visitor(o) {
                error = Some(e);
            }
        });
        error.map_or(Ok(()), Err)
    }
    pub fn recover(&mut self, mut p: Vec3) -> Result<Vec3, CharacterError> {
        let origin = p;
        for _ in 0..self.settings.max_iterations {
            let mut deepest = 0.;
            let mut normal = Vec3::ZERO;
            let shape = self.shape;
            self.visit(shape.bounds(p).expanded(self.settings.skin), |o| {
                let (g, n) = separation(shape, p, o)?;
                if g < deepest {
                    deepest = g;
                    normal = n;
                }
                Ok(())
            })?;
            if deepest >= -1e-5 {
                return Ok(p);
            }
            p += normal * (-deepest + self.settings.skin);
            if p.distance(origin) > self.settings.max_recovery {
                return Err(CharacterError::UnresolvedPenetration);
            }
        }
        Err(CharacterError::UnresolvedPenetration)
    }
    pub fn cast(&mut self, origin: Vec3, delta: Vec3) -> Result<Option<Hit>, CharacterError> {
        let shape = self.shape;
        let skin = self.settings.skin;
        let mut closest: Option<Hit> = None;
        let bounds = shape
            .bounds(origin)
            .union(shape.bounds(origin + delta))
            .expanded(skin * 2.);
        self.visit(bounds, |o| {
            let mut fraction = 0.;
            for _ in 0..48 {
                let (gap, normal) = separation(shape, origin + delta * fraction, o)?;
                let closing = -delta.dot(normal);
                if closing <= 1e-8 {
                    return Ok(());
                }
                if gap <= skin + 1e-5 {
                    if closest.is_none_or(|h| {
                        fraction < h.fraction - 1e-6
                            || ((fraction - h.fraction).abs() <= 1e-6 && o.id < h.obstacle.id)
                    }) {
                        closest = Some(Hit {
                            fraction,
                            normal,
                            obstacle: o,
                        });
                    }
                    return Ok(());
                }
                fraction += (gap - skin) / closing;
                if fraction > 1. || closest.is_some_and(|h| fraction > h.fraction) {
                    return Ok(());
                }
            }
            // Never turn a failed grazing query into permission to tunnel through geometry.
            Err(CharacterError::SweepBudget)
        })?;
        Ok(closest)
    }
}
