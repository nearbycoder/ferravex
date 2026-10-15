use ferravex_math::Vec3;
pub const MAX_RIBBON_POINTS: usize = 4096;
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RibbonPoint {
    pub position: [f32; 3],
    pub color: [f32; 4],
    pub width: f32,
    pub break_before: bool,
}
impl RibbonPoint {
    pub fn valid(&self) -> bool {
        self.position
            .into_iter()
            .chain(self.color)
            .all(f32::is_finite)
            && self.width.is_finite()
            && (0.0..=1e6).contains(&self.width)
    }
}
pub trait RibbonSource {
    fn point_count(&self) -> usize;
    fn point(&self, index: usize) -> Option<RibbonPoint>;
}
impl RibbonSource for [RibbonPoint] {
    fn point_count(&self) -> usize {
        self.len()
    }
    fn point(&self, index: usize) -> Option<RibbonPoint> {
        self.get(index).copied()
    }
}
impl<const N: usize> RibbonSource for [RibbonPoint; N] {
    fn point_count(&self) -> usize {
        N
    }
    fn point(&self, index: usize) -> Option<RibbonPoint> {
        self.get(index).copied()
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RibbonError {
    InvalidPoint,
    InvalidCapacity,
    InvalidView,
    Full,
}
pub struct RibbonPath {
    points: Vec<RibbonPoint>,
    capacity: usize,
}
impl RibbonPath {
    pub fn new(capacity: usize) -> Result<Self, RibbonError> {
        if capacity > MAX_RIBBON_POINTS {
            return Err(RibbonError::InvalidCapacity);
        }
        Ok(Self {
            points: Vec::with_capacity(capacity),
            capacity,
        })
    }
    pub fn set(&mut self, points: &[RibbonPoint]) -> Result<(), RibbonError> {
        if points.len() > self.capacity {
            return Err(RibbonError::Full);
        }
        if !points.iter().all(RibbonPoint::valid) {
            return Err(RibbonError::InvalidPoint);
        }
        self.points.clear();
        self.points.extend_from_slice(points);
        Ok(())
    }
    pub fn clear(&mut self) {
        self.points.clear();
    }
    pub fn points(&self) -> &[RibbonPoint] {
        &self.points
    }
}
impl RibbonSource for RibbonPath {
    fn point_count(&self) -> usize {
        self.points.len()
    }
    fn point(&self, index: usize) -> Option<RibbonPoint> {
        self.points.get(index).copied()
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RibbonFacing {
    /// XY for 2D: normal `[0,0,1]`. XZ ground marks: normal `[0,1,0]`.
    Plane {
        normal: [f32; 3],
    },
    Camera {
        eye: [f32; 3],
        up: [f32; 3],
    },
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RibbonVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RibbonTriangle {
    pub vertices: [RibbonVertex; 3],
}
/// Triangle iterator with bounded miters and explicit breaks. Degenerate points
/// and view-aligned segments get a finite fallback or are skipped; no allocation.
pub struct RibbonTriangles<'a, S: RibbonSource + ?Sized> {
    source: &'a S,
    facing: RibbonFacing,
    miter: f32,
    index: usize,
    pending: Option<RibbonTriangle>,
    distance: f32,
    count: usize,
}
impl<'a, S: RibbonSource + ?Sized> RibbonTriangles<'a, S> {
    pub fn new(source: &'a S, facing: RibbonFacing, miter_limit: f32) -> Result<Self, RibbonError> {
        let valid = match facing {
            RibbonFacing::Plane { normal } => Vec3::from_array(normal).try_normalize().is_some(),
            RibbonFacing::Camera { eye, up } => {
                eye.into_iter().all(f32::is_finite)
                    && Vec3::from_array(up).try_normalize().is_some()
            }
        };
        if !valid || !miter_limit.is_finite() || !(1.0..=4.).contains(&miter_limit) {
            return Err(RibbonError::InvalidView);
        }
        if source.point_count() > MAX_RIBBON_POINTS {
            return Err(RibbonError::InvalidCapacity);
        }
        Ok(Self {
            source,
            count: source.point_count(),
            facing,
            miter: miter_limit,
            index: 0,
            pending: None,
            distance: 0.,
        })
    }
    fn side(&self, point: Vec3, tangent: Vec3) -> Vec3 {
        let n = match self.facing {
            RibbonFacing::Plane { normal } => Vec3::from_array(normal).normalize_or_zero(),
            RibbonFacing::Camera { eye, .. } => (Vec3::from_array(eye) - point).normalize_or_zero(),
        };
        if let Some(side) = n.cross(tangent).try_normalize() {
            return side;
        }
        let fallback = match self.facing {
            RibbonFacing::Plane { .. } => tangent.any_orthonormal_vector(),
            RibbonFacing::Camera { up, .. } => Vec3::from_array(up).normalize_or_zero(),
        };
        (fallback - tangent * fallback.dot(tangent))
            .try_normalize()
            .unwrap_or_else(|| tangent.any_orthonormal_vector())
    }
    fn offset(&self, index: usize, tangent: Vec3, forward: bool, point: RibbonPoint) -> Vec3 {
        let p = Vec3::from_array(point.position);
        let side = self.side(p, tangent);
        let neighbor = if forward {
            self.source.point(index + 1).filter(|n| !n.break_before)
        } else {
            index
                .checked_sub(1)
                .and_then(|i| self.source.point(i))
                .filter(|_| !point.break_before)
        };
        let other = neighbor.filter(RibbonPoint::valid).and_then(|n| {
            if forward {
                (Vec3::from_array(n.position) - p).try_normalize()
            } else {
                (p - Vec3::from_array(n.position)).try_normalize()
            }
        });
        let Some(other) = other else {
            return side * (point.width * 0.5);
        };
        let other_side = self.side(p, other);
        let Some(miter) = (side + other_side).try_normalize() else {
            return side * (point.width * 0.5);
        };
        let denominator = miter.dot(side).abs();
        miter * (point.width * 0.5 * (1. / denominator.max(0.0001)).min(self.miter))
    }
}
impl<S: RibbonSource + ?Sized> Iterator for RibbonTriangles<'_, S> {
    type Item = RibbonTriangle;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(triangle) = self.pending.take() {
            return Some(triangle);
        }
        while self.index + 1 < self.count {
            let i = self.index;
            self.index += 1;
            let (Some(a), Some(b)) = (self.source.point(i), self.source.point(i + 1)) else {
                continue;
            };
            if b.break_before || !a.valid() || !b.valid() {
                self.distance = 0.;
                continue;
            }
            let (pa, pb) = (Vec3::from_array(a.position), Vec3::from_array(b.position));
            let Some(tangent) = (pb - pa).try_normalize() else {
                continue;
            };
            let oa = self.offset(i, tangent, false, a);
            let ob = self.offset(i + 1, tangent, true, b);
            let end = self.distance + pa.distance(pb);
            let vertex = |p: Vec3, color, uv| RibbonVertex {
                position: p.to_array(),
                color,
                uv,
            };
            let v = [
                vertex(pa - oa, a.color, [self.distance, 0.]),
                vertex(pa + oa, a.color, [self.distance, 1.]),
                vertex(pb - ob, b.color, [end, 0.]),
                vertex(pb + ob, b.color, [end, 1.]),
            ];
            self.distance = end;
            if !v
                .iter()
                .all(|p| p.position.into_iter().chain(p.uv).all(f32::is_finite))
            {
                continue;
            }
            self.pending = Some(RibbonTriangle {
                vertices: [v[2], v[1], v[3]],
            });
            return Some(RibbonTriangle {
                vertices: [v[0], v[1], v[2]],
            });
        }
        None
    }
}
