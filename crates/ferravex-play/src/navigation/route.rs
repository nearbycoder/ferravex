use super::NavigationError;
use ferravex_math::Vec3;
pub const MAX_ROUTE_POINTS: usize = 4096;
/// Reusable piecewise-linear world-space route. Arc lengths are f64, repeated
/// points are legal, and updates validate the complete input before mutation.
pub struct Route3 {
    points: Vec<Vec3>,
    lengths: Vec<f64>,
    capacity: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct RouteSample {
    pub position: Vec3,
    pub direction: Vec3,
    pub segment: usize,
    pub distance: f64,
    pub complete: bool,
}
impl Route3 {
    pub fn new(capacity: usize) -> Result<Self, NavigationError> {
        if capacity == 0 || capacity > MAX_ROUTE_POINTS {
            return Err(NavigationError::Capacity);
        }
        Ok(Self {
            points: Vec::with_capacity(capacity),
            lengths: Vec::with_capacity(capacity),
            capacity,
        })
    }
    pub fn set(&mut self, points: &[Vec3]) -> Result<(), NavigationError> {
        if points.len() > self.capacity {
            return Err(NavigationError::Capacity);
        }
        if points.iter().any(|p| !p.is_finite()) {
            return Err(NavigationError::InvalidInput);
        }
        self.points.clear();
        self.lengths.clear();
        let mut length = 0.;
        for (i, point) in points.iter().enumerate() {
            if i > 0 {
                length += distance(points[i - 1], *point);
            }
            self.points.push(*point);
            self.lengths.push(length);
        }
        Ok(())
    }
    pub fn clear(&mut self) {
        self.points.clear();
        self.lengths.clear();
    }
    pub fn points(&self) -> &[Vec3] {
        &self.points
    }
    pub fn length(&self) -> f64 {
        self.lengths.last().copied().unwrap_or(0.)
    }
    pub fn sample(&self, travel: f64) -> Result<Option<RouteSample>, NavigationError> {
        if !travel.is_finite() || travel < 0. {
            return Err(NavigationError::InvalidInput);
        }
        let Some(last) = self.points.last().copied() else {
            return Ok(None);
        };
        if self.length() == 0. || travel >= self.length() {
            return Ok(Some(RouteSample {
                position: last,
                direction: Vec3::ZERO,
                segment: self.points.len() - 1,
                distance: self.length(),
                complete: true,
            }));
        }
        // upper_bound skips zero-length segments without a linear traversal.
        let next = self.lengths.partition_point(|v| *v <= travel).max(1);
        let start = self.lengths[next - 1];
        let segment = self.lengths[next] - start;
        let alpha = ((travel - start) / segment).clamp(0., 1.);
        let a = self.points[next - 1].to_array();
        let b = self.points[next].to_array();
        let position = Vec3::from_array(std::array::from_fn(|i| {
            (f64::from(a[i]) * (1. - alpha) + f64::from(b[i]) * alpha) as f32
        }));
        let direction = Vec3::from_array(std::array::from_fn(|i| {
            ((f64::from(b[i]) - f64::from(a[i])) / segment) as f32
        }));
        Ok(Some(RouteSample {
            position,
            direction,
            segment: next - 1,
            distance: travel,
            complete: false,
        }))
    }
}
fn distance(a: Vec3, b: Vec3) -> f64 {
    let a = a.to_array();
    let b = b.to_array();
    (0..3)
        .map(|i| (f64::from(b[i]) - f64::from(a[i])).powi(2))
        .sum::<f64>()
        .sqrt()
}
/// Exact distance cursor for authored rail/path movement. Collision-driven actors
/// should use WaypointFollower, so a blocked body cannot advance along a route.
#[derive(Clone, Copy, Debug, Default)]
pub struct PathCursor {
    distance: f64,
}
impl PathCursor {
    pub fn reset(&mut self) {
        self.distance = 0.;
    }
    pub fn distance(&self) -> f64 {
        self.distance
    }
    pub fn seek(&mut self, route: &Route3, distance: f64) -> Result<(), NavigationError> {
        if !distance.is_finite() || distance < 0. {
            return Err(NavigationError::InvalidInput);
        }
        self.distance = distance.min(route.length());
        Ok(())
    }
    pub fn advance(
        &mut self,
        route: &Route3,
        speed: f32,
        dt: f32,
    ) -> Result<Option<RouteSample>, NavigationError> {
        if !speed.is_finite() || speed < 0. || !dt.is_finite() || !(0. ..=60.).contains(&dt) {
            return Err(NavigationError::InvalidInput);
        }
        self.distance = (self.distance + f64::from(speed) * f64::from(dt)).min(route.length());
        route.sample(self.distance)
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct WaypointFollower {
    next: usize,
}
impl WaypointFollower {
    pub fn reset(&mut self) {
        self.next = 0;
    }
    pub fn next_index(&self) -> usize {
        self.next
    }
    /// Returns the next unvisited waypoint. Stops at the endpoint; never advances
    /// merely because time passes while an actor is blocked. At most max_skips
    /// waypoints are consumed per call, even for a route of repeated points.
    pub fn target(
        &mut self,
        route: &Route3,
        position: Vec3,
        radius: f32,
        max_skips: usize,
    ) -> Result<Option<Vec3>, NavigationError> {
        if !position.is_finite() || !radius.is_finite() || radius < 0. {
            return Err(NavigationError::InvalidInput);
        }
        self.next = self.next.min(route.points.len());
        let mut skipped = 0;
        while self.next < route.points.len()
            && distance(position, route.points[self.next]) <= f64::from(radius)
        {
            if skipped == max_skips {
                return Ok(Some(route.points[self.next]));
            }
            self.next += 1;
            skipped += 1;
        }
        Ok(route.points.get(self.next).copied())
    }
}
