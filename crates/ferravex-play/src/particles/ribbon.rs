use ferravex_math::Vec3;
pub const MAX_RIBBON_POINTS: usize = 4096;
/// Largest accepted full ribbon width, in world units.
pub const MAX_RIBBON_WIDTH: f32 = 1e6;
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
            && valid_width(self.width)
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
    /// A point has non-finite components or a width outside `0..=MAX_RIBBON_WIDTH`.
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
    /// Sets every point's full width without allocating. An invalid width returns
    /// [`RibbonError::InvalidPoint`] and leaves the path unchanged, even if empty.
    /// Zero is valid and collapses the ribbon to its centerline.
    ///
    /// ```
    /// use ferravex_play::particles::{RibbonError, RibbonPath, RibbonPoint};
    /// let mut path = RibbonPath::new(2)?;
    /// path.set(&[RibbonPoint::default(); 2])?;
    /// path.set_width(0.5)?;
    /// assert!(path.points().iter().all(|p| p.width == 0.5));
    /// assert_eq!(path.set_width(-1.0), Err(RibbonError::InvalidPoint));
    /// # Ok::<(), RibbonError>(())
    /// ```
    pub fn set_width(&mut self, width: f32) -> Result<(), RibbonError> {
        if !valid_width(width) {
            return Err(RibbonError::InvalidPoint);
        }
        for point in &mut self.points {
            point.width = width;
        }
        Ok(())
    }
    /// Replaces widths with a linear taper by world-space arc length, from the
    /// first to the last point of **each** run separated by `break_before`.
    /// These are absolute full widths, not multipliers of the existing widths.
    ///
    /// Coincident points have equal widths; singleton and entirely coincident
    /// runs use `start_width`. Empty paths are allowed. Both widths must be finite
    /// and in `0..=MAX_RIBBON_WIDTH`, otherwise [`RibbonError::InvalidPoint`] is
    /// returned without changing the path. Uses two passes per run and no allocation.
    ///
    /// ```
    /// use ferravex_play::particles::{RibbonError, RibbonPath, RibbonPoint};
    /// let mut path = RibbonPath::new(3)?;
    /// path.set(&[0.0, 1.0, 4.0].map(|x| RibbonPoint {
    ///     position: [x, 0.0, 0.0], ..RibbonPoint::default()
    /// }))?;
    /// path.set_taper(2.0, 0.0)?;
    /// assert_eq!(path.points().iter().map(|p| p.width).collect::<Vec<_>>(),
    ///            [2.0, 1.5, 0.0]);
    /// # Ok::<(), RibbonError>(())
    /// ```
    pub fn set_taper(&mut self, start_width: f32, end_width: f32) -> Result<(), RibbonError> {
        if !valid_width(start_width) || !valid_width(end_width) {
            return Err(RibbonError::InvalidPoint);
        }
        let mut start = 0;
        while start < self.points.len() {
            let mut end = start + 1;
            let mut length = 0.;
            while end < self.points.len() && !self.points[end].break_before {
                length += point_distance(self.points[end - 1].position, self.points[end].position);
                end += 1;
            }
            let mut distance = 0.;
            for i in start..end {
                if i > start {
                    distance +=
                        point_distance(self.points[i - 1].position, self.points[i].position);
                }
                let t = if length > 0. { distance / length } else { 0. };
                self.points[i].width =
                    (f64::from(start_width) * (1. - t) + f64::from(end_width) * t) as f32;
            }
            start = end;
        }
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
/// Triangle iterator with bounded miters and explicit breaks; no allocation.
///
/// Coincident segments emit no triangles and do not advance UV distance. Joins
/// look through coincident points within a run, retaining each endpoint's authored
/// width/color. Near reversals (tangent dot product <= -0.9999) use independent
/// butt ends instead of an unstable miter, as do nearly opposing facing sides.
/// Nearly view-aligned segments use a finite perpendicular fallback. Zero-width
/// segments may emit collapsed triangles.
///
/// UV `u` is world-space distance, saturated at `f32::MAX`. Breaks and missing or
/// invalid source points reset it and stop neighbor lookup. Wide intermediates
/// keep finite extreme positions from poisoning subsequent geometry.
pub struct RibbonTriangles<'a, S: RibbonSource + ?Sized> {
    source: &'a S,
    facing: RibbonFacing,
    miter: f32,
    index: usize,
    pending: Option<RibbonTriangle>,
    distance: f64,
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
        let count = source.point_count();
        if count > MAX_RIBBON_POINTS {
            return Err(RibbonError::InvalidCapacity);
        }
        Ok(Self {
            source,
            count,
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
            RibbonFacing::Camera { eye, .. } => {
                point_direction(point.to_array(), eye).unwrap_or(Vec3::ZERO)
            }
        };
        if let Some(side) = stable_normal(n.cross(tangent)) {
            return side;
        }
        let fallback = match self.facing {
            RibbonFacing::Plane { .. } => tangent.any_orthonormal_vector(),
            RibbonFacing::Camera { up, .. } => Vec3::from_array(up).normalize_or_zero(),
        };
        stable_normal(fallback - tangent * fallback.dot(tangent))
            .unwrap_or_else(|| tangent.any_orthonormal_vector())
    }
    fn neighbor_tangent(
        &self,
        mut index: usize,
        forward: bool,
        point: RibbonPoint,
    ) -> Option<Vec3> {
        let mut current = point;
        loop {
            index = if forward {
                (index + 1 < self.count).then_some(index + 1)?
            } else {
                if current.break_before {
                    return None;
                }
                index.checked_sub(1)?
            };
            let neighbor = self.source.point(index)?;
            if !neighbor.valid() || (forward && neighbor.break_before) {
                return None;
            }
            let direction = if forward {
                point_direction(point.position, neighbor.position)
            } else {
                point_direction(neighbor.position, point.position)
            };
            if direction.is_some() {
                return direction;
            }
            current = neighbor;
        }
    }
    fn offset(&self, index: usize, tangent: Vec3, forward: bool, point: RibbonPoint) -> Vec3 {
        let p = Vec3::from_array(point.position);
        let side = self.side(p, tangent);
        let Some(other) = self.neighbor_tangent(index, forward, point) else {
            return side * (point.width * 0.5);
        };
        if tangent.dot(other) <= -0.9999 {
            return side * (point.width * 0.5);
        }
        let other_side = self.side(p, other);
        let Some(miter) = stable_normal(side + other_side) else {
            return side * (point.width * 0.5);
        };
        // Symmetric evaluation gives adjoining segments the same join even when
        // f32 rounding makes their individual dot products differ slightly.
        let denominator = miter.dot(side).abs().min(miter.dot(other_side).abs());
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
                self.distance = 0.;
                continue;
            };
            if b.break_before || !a.valid() || !b.valid() {
                self.distance = 0.;
                continue;
            }
            let (pa, pb) = (Vec3::from_array(a.position), Vec3::from_array(b.position));
            let Some(tangent) = point_direction(a.position, b.position) else {
                continue;
            };
            let oa = self.offset(i, tangent, false, a);
            let ob = self.offset(i + 1, tangent, true, b);
            let end = self.distance + point_distance(a.position, b.position);
            let start_u = self.distance.min(f64::from(f32::MAX)) as f32;
            let end_u = end.min(f64::from(f32::MAX)) as f32;
            let vertex = |p: Vec3, color, uv| RibbonVertex {
                position: p.to_array(),
                color,
                uv,
            };
            let v = [
                vertex(pa - oa, a.color, [start_u, 0.]),
                vertex(pa + oa, a.color, [start_u, 1.]),
                vertex(pb - ob, b.color, [end_u, 0.]),
                vertex(pb + ob, b.color, [end_u, 1.]),
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

fn valid_width(width: f32) -> bool {
    width.is_finite() && (0.0..=MAX_RIBBON_WIDTH).contains(&width)
}

// Inputs here are sums, crosses or projections of unit directions. Do not turn
// cancellation noise at alignment/reversal into a full-width arbitrary normal.
fn stable_normal(direction: Vec3) -> Option<Vec3> {
    if direction.length_squared() > 1e-8 {
        direction.try_normalize()
    } else {
        None
    }
}

fn point_delta(a: [f32; 3], b: [f32; 3]) -> [f64; 3] {
    std::array::from_fn(|i| f64::from(b[i]) - f64::from(a[i]))
}

fn point_distance(a: [f32; 3], b: [f32; 3]) -> f64 {
    point_delta(a, b)
        .into_iter()
        .map(|d| d * d)
        .sum::<f64>()
        .sqrt()
}

fn point_direction(a: [f32; 3], b: [f32; 3]) -> Option<Vec3> {
    let delta = point_delta(a, b);
    let length = delta.into_iter().map(|d| d * d).sum::<f64>().sqrt();
    (length > 0.).then(|| Vec3::from_array(delta.map(|d| (d / length) as f32)))
}
