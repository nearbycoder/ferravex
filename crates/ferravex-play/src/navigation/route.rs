use super::NavigationError;
use ferravex_math::Vec3;
pub const MAX_ROUTE_POINTS: usize = 4096;
/// Reusable piecewise-linear world-space route. Arc lengths are f64, repeated
/// points are legal, and updates validate the complete input before mutation.
pub struct Route3 {
    points: Vec<Vec3>,
    lengths: Vec<f64>,
    capacity: usize,
    closed: bool,
    total_length: f64,
}
/// Arc-distance sample from [`Route3::sample`] or [`Route3::sample_progress`].
#[derive(Clone, Copy, Debug)]
pub struct RouteSample {
    pub position: Vec3,
    pub direction: Vec3,
    pub segment: usize,
    pub distance: f64,
    /// True only at the endpoint of an open route, including a zero-length one.
    pub complete: bool,
}
/// Nearest geometric observation from [`Route3::project`]. This is not evidence
/// of visiting checkpoints or completing laps. See that method for an example.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteProjection {
    pub position: Vec3,
    /// Unit tangent of the selected segment, including at its endpoints. Zero
    /// for a route consisting only of coincident points.
    pub direction: Vec3,
    /// Start vertex of the selected segment. The closing segment starts at the
    /// last vertex; a route consisting only of coincident points uses zero.
    pub segment: usize,
    /// Clamped fraction along the selected segment, in `0..=1`.
    pub segment_fraction: f64,
    /// Arc distance from the first vertex, in world units. The closed seam is
    /// canonicalized to zero; no lap count is retained.
    pub distance: f64,
    /// Arc distance divided by route length, or zero for a zero-length route.
    pub progress: f64,
    /// Unsigned world-space distance from the query to its projection, computed
    /// in f64 before rounding the returned position to Vec3.
    pub distance_to_route: f64,
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
            closed: false,
            total_length: 0.,
        })
    }
    /// Replaces the route with an open polyline, including after `set_closed`.
    /// Invalid input leaves the previous geometry and topology unchanged.
    pub fn set(&mut self, points: &[Vec3]) -> Result<(), NavigationError> {
        self.set_points(points, false)
    }
    /// Replaces the route with a closed polyline, adding a last-to-first segment
    /// without storing another point or consuming capacity. Repeating the first
    /// point at the end is also legal. Empty and zero-length loops are legal.
    /// Invalid input leaves the previous route unchanged.
    ///
    /// Closed distance samples and cursors wrap at the seam and never complete.
    /// `WaypointFollower` retains its finite traversal of the authored vertices.
    /// No query, cursor operation or successful replacement allocates.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::navigation::{PathCursor, Route3};
    /// let mut route = Route3::new(4)?;
    /// route.set_closed(&[Vec3::ZERO, Vec3::X * 2., Vec3::new(2., 2., 0.), Vec3::Y * 2.])?;
    /// assert!(route.is_closed());
    /// assert_eq!(route.length(), 8.);
    /// assert_eq!(route.sample(9.)?.unwrap().position, Vec3::X);
    /// let mut cursor = PathCursor::default();
    /// cursor.seek(&route, 7.)?;
    /// assert_eq!(cursor.advance(&route, 2., 1.)?.unwrap().distance, 1.);
    /// route.clear();
    /// assert!(!route.is_closed());
    /// # Ok::<(), ferravex_play::navigation::NavigationError>(())
    /// ```
    pub fn set_closed(&mut self, points: &[Vec3]) -> Result<(), NavigationError> {
        self.set_points(points, true)
    }
    /// Whether samples wrap over a last-to-first segment. See [`Self::set_closed`].
    pub fn is_closed(&self) -> bool {
        self.closed
    }
    fn set_points(&mut self, points: &[Vec3], closed: bool) -> Result<(), NavigationError> {
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
        if closed && points.len() > 1 {
            length += distance(points[points.len() - 1], points[0]);
        }
        self.total_length = length;
        self.closed = closed;
        Ok(())
    }
    pub fn clear(&mut self) {
        self.points.clear();
        self.lengths.clear();
        self.total_length = 0.;
        self.closed = false;
    }
    pub fn points(&self) -> &[Vec3] {
        &self.points
    }
    pub fn length(&self) -> f64 {
        self.total_length
    }
    /// Samples a finite nonnegative distance. Open routes clamp at the endpoint;
    /// closed routes wrap, with zero-length loops staying at their sole position.
    /// Empty routes return `None`. See [`Self::set_closed`] for a loop example.
    pub fn sample(&self, travel: f64) -> Result<Option<RouteSample>, NavigationError> {
        if !travel.is_finite() || travel < 0. {
            return Err(NavigationError::InvalidInput);
        }
        let Some(last) = self.points.last().copied() else {
            return Ok(None);
        };
        let travel = self.canonical_distance(travel);
        if self.length() == 0. || (!self.closed && travel >= self.length()) {
            return Ok(Some(RouteSample {
                position: last,
                direction: Vec3::ZERO,
                segment: self.points.len() - 1,
                distance: self.length(),
                complete: !self.closed,
            }));
        }
        // upper_bound skips zero-length segments without a linear traversal.
        let next = self.lengths.partition_point(|v| *v <= travel).max(1);
        let start = self.lengths[next - 1];
        let segment = self.lengths.get(next).copied().unwrap_or(self.length()) - start;
        let alpha = ((travel - start) / segment).clamp(0., 1.);
        let a = self.points[next - 1].to_array();
        let b = self.points[next % self.points.len()].to_array();
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

    /// Samples a normalized arc fraction in `0..=1`; out-of-range and non-finite
    /// inputs return `InvalidInput`, even on an empty route. One is the endpoint
    /// for an open route and the start of a closed route.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::navigation::Route3;
    /// let mut route = Route3::new(2)?;
    /// route.set(&[Vec3::ZERO, Vec3::X * 10.])?;
    /// assert_eq!(route.sample_progress(0.25)?.unwrap().position, Vec3::X * 2.5);
    /// assert!(route.sample_progress(1.)?.unwrap().complete);
    /// assert!(route.sample_progress(1.1).is_err());
    /// # Ok::<(), ferravex_play::navigation::NavigationError>(())
    /// ```
    pub fn sample_progress(&self, progress: f64) -> Result<Option<RouteSample>, NavigationError> {
        if !progress.is_finite() || !(0. ..=1.).contains(&progress) {
            return Err(NavigationError::InvalidInput);
        }
        self.sample(self.length() * progress)
    }

    /// Projects a finite world position onto the entire route in O(point count)
    /// time and constant scratch space. Empty routes return `None`. Calculations
    /// use f64, including differences between extreme finite f32 coordinates.
    ///
    /// Zero-length segments are skipped. Equal squared distances select the
    /// lowest index of a nonzero-length segment, including at corners or
    /// intersections. A point-only route returns its first point, zero progress
    /// and zero tangent.
    /// Projection is global: teleports and nearby branches can change progress
    /// discontinuously. It does not infer motion, laps or checkpoint visits.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::navigation::{Route3, RouteProjection};
    /// let mut route = Route3::new(3)?;
    /// route.set(&[Vec3::ZERO, Vec3::ZERO, Vec3::X * 10.])?;
    /// let hit: RouteProjection = route.project(Vec3::new(4., 3., 0.))?.unwrap();
    /// assert_eq!(hit.position, Vec3::X * 4.);
    /// assert_eq!(hit.segment, 1);
    /// assert_eq!(hit.distance, 4.);
    /// assert_eq!(hit.progress, 0.4);
    /// assert_eq!(hit.distance_to_route, 3.);
    /// # Ok::<(), ferravex_play::navigation::NavigationError>(())
    /// ```
    pub fn project(&self, position: Vec3) -> Result<Option<RouteProjection>, NavigationError> {
        if !position.is_finite() {
            return Err(NavigationError::InvalidInput);
        }
        let Some(first) = self.points.first().copied() else {
            return Ok(None);
        };
        let query = position.to_array().map(f64::from);
        let mut best = None;
        let mut best_squared = f64::INFINITY;
        let segment_count = self.points.len() - 1 + usize::from(self.closed);
        for segment in 0..segment_count {
            let a = self.points[segment].to_array().map(f64::from);
            let b = self.points[(segment + 1) % self.points.len()]
                .to_array()
                .map(f64::from);
            let delta: [f64; 3] = std::array::from_fn(|i| b[i] - a[i]);
            let squared_length = delta.iter().map(|v| v * v).sum::<f64>();
            if squared_length == 0. {
                continue;
            }
            let fraction = ((0..3).map(|i| (query[i] - a[i]) * delta[i]).sum::<f64>()
                / squared_length)
                .clamp(0., 1.);
            let projected: [f64; 3] =
                std::array::from_fn(|i| a[i] * (1. - fraction) + b[i] * fraction);
            let squared = (0..3)
                .map(|i| (query[i] - projected[i]).powi(2))
                .sum::<f64>();
            if squared < best_squared {
                let length = squared_length.sqrt();
                let along = self.canonical_distance(self.lengths[segment] + length * fraction);
                best = Some(RouteProjection {
                    position: Vec3::from_array(projected.map(|v| v as f32)),
                    direction: Vec3::from_array(delta.map(|v| (v / length) as f32)),
                    segment,
                    segment_fraction: fraction,
                    distance: along,
                    progress: along / self.length(),
                    distance_to_route: squared.sqrt(),
                });
                best_squared = squared;
            }
        }
        Ok(Some(best.unwrap_or_else(|| RouteProjection {
            position: first,
            direction: Vec3::ZERO,
            segment: 0,
            segment_fraction: 0.,
            distance: 0.,
            progress: 0.,
            distance_to_route: distance(position, first),
        })))
    }

    fn canonical_distance(&self, travel: f64) -> f64 {
        if self.closed && self.length() > 0. {
            travel % self.length()
        } else {
            travel.min(self.length())
        }
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
    /// Seeks to a finite nonnegative distance, clamping open routes and wrapping
    /// closed routes. See [`Route3::set_closed`] for a cursor example.
    pub fn seek(&mut self, route: &Route3, distance: f64) -> Result<(), NavigationError> {
        if !distance.is_finite() || distance < 0. {
            return Err(NavigationError::InvalidInput);
        }
        self.distance = route.canonical_distance(distance);
        Ok(())
    }
    /// Repositions the cursor to the global nearest projection after a teleport
    /// or route replacement. This can move backward or jump across the seam;
    /// it neither advances time nor awards laps/checkpoints. Invalid positions
    /// and empty routes leave the cursor unchanged. See [`Route3::project`] for
    /// tie rules. No route identity or previous projection is cached.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::navigation::{PathCursor, Route3};
    /// let mut route = Route3::new(2)?;
    /// route.set(&[Vec3::ZERO, Vec3::X * 10.])?;
    /// let mut cursor = PathCursor::default();
    /// cursor.seek(&route, 9.)?;
    /// cursor.recover(&route, Vec3::new(2., 3., 0.))?;
    /// assert_eq!(cursor.distance(), 2.);
    /// assert_eq!(cursor.advance(&route, 1., 1.)?.unwrap().position, Vec3::X * 3.);
    /// # Ok::<(), ferravex_play::navigation::NavigationError>(())
    /// ```
    pub fn recover(
        &mut self,
        route: &Route3,
        position: Vec3,
    ) -> Result<Option<RouteProjection>, NavigationError> {
        let projection = route.project(position)?;
        if let Some(hit) = projection {
            self.distance = hit.distance;
        }
        Ok(projection)
    }
    /// Advances by a caller-supplied speed and simulation delta. Open routes
    /// stop at the endpoint; closed routes wrap without tracking completed laps.
    pub fn advance(
        &mut self,
        route: &Route3,
        speed: f32,
        dt: f32,
    ) -> Result<Option<RouteSample>, NavigationError> {
        if !speed.is_finite() || speed < 0. || !dt.is_finite() || !(0. ..=60.).contains(&dt) {
            return Err(NavigationError::InvalidInput);
        }
        // Reduce each operand first so a large whole-loop step cannot erase a
        // small existing offset when they are added. This also bounds a stale
        // cursor after a route replacement without storing route identities.
        let step = f64::from(speed) * f64::from(dt);
        self.distance = route.canonical_distance(
            route.canonical_distance(self.distance) + route.canonical_distance(step),
        );
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
