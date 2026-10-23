//! Checked, allocation-free projection of caller-authorized planar points.
//!
//! World coordinates are `(x, y)` in caller units (adapt an XZ plane explicitly
//! with `vec2(position.x, position.z)`). [`WorldYAxis`] declares whether positive
//! world Y points up or down. Map coordinates are logical UI units, X right and
//! Y down. Positive rotation is **clockwise on screen**, in radians, applied
//! after the Y convention. The world center stays at the map center. One uniform
//! scale contains the entire rotated world rectangle, with centered letterboxing;
//! there is no crop/stretch mode. [`Minimap::footprint`] is the actual rotated
//! world boundary, which can occupy less than its axis-aligned bounding box.
//!
//! Both rectangles reuse [`Rect`]. Negative extents are rejected, never
//! normalized. Edges are `min` and the f32 sum `min + size`, matching UI geometry;
//! world edges are closed for projection. Pointer picking first uses the map's
//! **half-open** bounds and the caller's half-open safe/clip rectangle, then
//! rejects letterbox/rotated-corner space outside the world footprint. No DPR
//! conversion occurs here. Feed already-logical coordinates, or call
//! [`super::Viewport::unproject`] exactly once before picking in viewport space.
//!
//! Arithmetic widens before subtraction, scaling and rotation. Returned [`Vec2`]
//! coordinates round to f32; round trips are approximate, especially near large
//! origins or thin rectangles. Picking allows half the larger adjacent f32
//! spacing per map coordinate around the world footprint (plus an f64 arithmetic
//! allowance), then clamps that numerical fringe to its closed world edge.
//! This never widens the half-open map/safe rectangles. Rectangles with
//! subnormal/zero extents or collapsed/overflowing f32 edges are rejected, as is
//! a fitted world boundary with collapsed/collinear corners. There is no
//! exact-predicate or cross-platform bitwise-determinism promise.
//!
//! No journal, discovery, renderer, input, simulation or global state is owned.
//! Unknown/unauthorized points must be **omitted by the caller**; absence never
//! creates a marker. Reconfiguration replaces all geometry transactionally; no
//! separate reset is needed. Queries and batches retain no history and allocate
//! no heap storage. All batches have a fixed [`MAX_MINIMAP_POINTS`] work bound.
//!
//! # Runnable Crystal-style revealed-room and waypoint recipe
//!
//! This uses supplied sample data, not Crystal's real journal. Parent adoption
//! must select only rooms already revealed by accepted game events, retain IDs,
//! and authorize any picked waypoint. Geometry alone grants no gameplay access.
//!
//! ```rust
//! use ferravex_math::{Vec2, vec2};
//! use ferravex_play::ui::{Rect, Viewport};
//! use ferravex_play::ui::minimap::{
//!     MapConfig, Minimap, MinimapError, OffMapPolicy, ProjectionStatus, WorldYAxis,
//! };
//!
//! fn main() -> Result<(), MinimapError> {
//!     let viewport = Viewport::responsive([400., 300.], [12., 20., 8., 10.]).unwrap();
//!     let config = MapConfig {
//!         world: Rect { min: [-20., -10.], size: [80., 40.] },
//!         map: Rect { min: [16., 24.], size: [240., 160.] },
//!         y_axis: WorldYAxis::Up,
//!         rotation_radians: 0.,
//!     };
//!     let mut map = Minimap::new(config)?;
//!     // None represents unknown/omitted location, never an inferred objective.
//!     let supplied_rooms = [(7, Some(vec2(0., 0.))), (9, None),
//!                           (12, Some(vec2(40., 20.)))];
//!     let mut ids = [0; 3];
//!     let mut known = [Vec2::ZERO; 3];
//!     let mut count = 0;
//!     for (id, point) in supplied_rooms {
//!         if let Some(point) = point {
//!             ids[count] = id;
//!             known[count] = point;
//!             count += 1;
//!         }
//!     }
//!     let seed = map.project(known[0], OffMapPolicy::ClampToEdge)?;
//!     let mut markers = [seed; 3];
//!     assert_eq!(map.project_into(&known[..count], OffMapPolicy::ClampToEdge,
//!                                &mut markers)?, 2);
//!     assert_eq!(&ids[..count], &[7, 12]); // No room 9 marker.
//!     assert_eq!(markers[0].status, ProjectionStatus::Inside);
//!     let waypoint = map.project(vec2(100., 10.), OffMapPolicy::ClampToEdge)?;
//!     assert_eq!(waypoint.status, ProjectionStatus::Clamped);
//!     assert!(waypoint.off_map.unwrap().direction.x > 0.);
//!     let outside = map.project(vec2(100., 10.), OffMapPolicy::Preserve)?;
//!     assert_eq!(outside.status, ProjectionStatus::Outside);
//!     assert_eq!(outside.off_map, waypoint.off_map);
//!     let boundary = map.footprint(); // Use the same vertices for painting.
//!     assert!(boundary.into_iter().all(Vec2::is_finite));
//!     // Host pointer is already in logical screen units, with no DPR rescaling.
//!     let host_pointer = [markers[0].position.x + 12., markers[0].position.y + 20.];
//!     let logical = viewport.unproject(host_pointer).unwrap();
//!     let picked = map.pick(Vec2::from_array(logical), viewport.bounds())?.unwrap();
//!     assert!((picked - known[0]).length() < 0.0001);
//!     let old = map;
//!     let mut invalid = map.config();
//!     invalid.world.size[0] = 0.;
//!     assert!(map.reconfigure(invalid).is_err());
//!     assert_eq!(map, old); // Last-good transform survives failure.
//!     map.reconfigure(config)?; // Also the scene/resize reset operation.
//!     Ok(())
//! }
//! ```

use super::Rect;
use ferravex_math::Vec2;

/// Maximum input count for one atomic [`Minimap::project_into`] call.
pub const MAX_MINIMAP_POINTS: usize = 4096;

/// Positive planar world Y convention before clockwise screen rotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldYAxis {
    /// Positive world Y initially points toward the screen top.
    Up,
    /// Positive world Y initially points toward the screen bottom.
    Down,
}

/// Explicit world and map geometry. See the module recipe for construction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapConfig {
    /// Closed planar world bounds; not inferred from supplied points.
    pub world: Rect,
    /// Logical UI panel bounds, half-open for picking.
    pub map: Rect,
    pub y_axis: WorldYAxis,
    /// Clockwise screen rotation in radians, after applying `y_axis`.
    /// Any finite f32 angle is accepted; very large angles lose phase precision.
    pub rotation_radians: f32,
}

/// Treatment of finite points outside the closed world rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OffMapPolicy {
    /// Preserve the projected location, which can be outside the panel.
    /// Return `Unrepresentable` rather than an infinite f32 position.
    Preserve,
    /// Put the marker on the world-boundary ray intersection. This stays inside
    /// the map's closed bounds, including under rotation and letterboxing.
    ClampToEdge,
}

/// Separates inside, outside/unclamped, and outside/clamped markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionStatus {
    Inside,
    Outside,
    Clamped,
}

/// Boundary marker for an outside point, regardless of projection policy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OffMapIndicator {
    /// Intersection of the center-to-point ray with the rotated world boundary;
    /// this is not necessarily an edge of the axis-aligned panel.
    pub position: Vec2,
    /// Unit screen direction from map center toward the outside point.
    pub direction: Vec2,
}

/// One projection, with no inferred ID, discovery or gameplay permission.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapProjection {
    pub position: Vec2,
    pub status: ProjectionStatus,
    /// `None` exactly when `status == Inside`; otherwise the edge indicator.
    pub off_map: Option<OffMapIndicator>,
}

/// Checked failure. Rejected updates/batches leave their destinations unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MinimapError {
    /// Nonfinite/degenerate rectangle, angle, pointer, or clip rectangle.
    InvalidGeometry,
    /// A supplied world point contains NaN or infinity.
    InvalidPoint,
    /// Required f32 output overflows or the fitted world collapses in f32.
    Unrepresentable,
    /// Too many input points or too little caller output storage.
    Capacity,
}

impl std::fmt::Display for MinimapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidGeometry => "invalid minimap geometry",
            Self::InvalidPoint => "minimap world point must be finite",
            Self::Unrepresentable => "minimap output is not representable in f32",
            Self::Capacity => "minimap batch capacity exceeded",
        })
    }
}
impl std::error::Error for MinimapError {}

/// Immutable checked geometry with transactional replacement, no heap storage.
/// See the module recipe for all queries, batch projection and reconfiguration.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Minimap {
    config: MapConfig,
    world_min: [f64; 2],
    world_max: [f64; 2],
    world_center: [f64; 2],
    half: [f64; 2],
    map_min: [f64; 2],
    map_max: [f64; 2],
    map_center: [f64; 2],
    scale: f64,
    sin: f64,
    cos: f64,
    y_sign: f64,
}

impl Minimap {
    /// Validate explicit bounds and aspect-fit the full rotated world. Reject
    /// collapsed/subnormal geometry rather than silently repairing it.
    /// See the module recipe for an executed example.
    pub fn new(config: MapConfig) -> Result<Self, MinimapError> {
        let (world_min, world_max) = checked_rect(config.world)?;
        let (map_min, map_max) = checked_rect(config.map)?;
        if !config.rotation_radians.is_finite() {
            return Err(MinimapError::InvalidGeometry);
        }
        let half = std::array::from_fn(|i| (world_max[i] - world_min[i]) * 0.5);
        let map_half: [f64; 2] = std::array::from_fn(|i| (map_max[i] - map_min[i]) * 0.5);
        let (sin, cos) = f64::from(config.rotation_radians).sin_cos();
        let rotated_half = [
            cos.abs() * half[0] + sin.abs() * half[1],
            sin.abs() * half[0] + cos.abs() * half[1],
        ];
        let scale = (map_half[0] / rotated_half[0]).min(map_half[1] / rotated_half[1]);
        let map = Self {
            config,
            world_min,
            world_max,
            world_center: std::array::from_fn(|i| world_min[i] + half[i]),
            half,
            map_min,
            map_max,
            map_center: std::array::from_fn(|i| map_min[i] + map_half[i]),
            scale,
            sin,
            cos,
            y_sign: if config.y_axis == WorldYAxis::Up {
                -1.
            } else {
                1.
            },
        };
        let corners = map.footprint();
        // Display rounding must preserve a strictly convex quadrilateral. Even
        // distinct adjacent vertices can all lie on one line at a large origin.
        let turns: [f64; 4] = std::array::from_fn(|i| {
            let a = corners[i].to_array().map(f64::from);
            let b = corners[(i + 1) % 4].to_array().map(f64::from);
            let c = corners[(i + 2) % 4].to_array().map(f64::from);
            (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0])
        });
        if turns
            .iter()
            .any(|turn| *turn == 0. || turn.is_sign_positive() != turns[0].is_sign_positive())
        {
            return Err(MinimapError::Unrepresentable);
        }
        Ok(map)
    }

    /// Replace all geometry only after full validation. On error `self` stays
    /// bit-for-bit unchanged. This is also the explicit resize/scene reset path.
    /// See the module recipe for failure followed by successful replacement.
    pub fn reconfigure(&mut self, config: MapConfig) -> Result<(), MinimapError> {
        *self = Self::new(config)?;
        Ok(())
    }

    /// Original configuration, without normalization. See the module recipe.
    pub fn config(&self) -> MapConfig {
        self.config
    }

    /// Display boundary in world corner order: min/min, max/min, max/max,
    /// min/max. Y-up reverses its screen winding. Vertices round to f32 and stay
    /// in the panel's closed bounds. See the module recipe for paint geometry.
    pub fn footprint(&self) -> [Vec2; 4] {
        [[-1., -1.], [1., -1.], [1., 1.], [-1., 1.]]
            .map(|sign| self.bounded_screen([sign[0] * self.half[0], sign[1] * self.half[1]]))
    }

    /// Project a supplied finite point; world edges count as inside. Outside
    /// points always carry an indicator, even when `Preserve` puts the position
    /// inside a rotated footprint's bounding box. An overflowing preserved
    /// position returns `Unrepresentable`; `ClampToEdge` remains bounded.
    /// See the module recipe for both policies.
    pub fn project(
        &self,
        point: Vec2,
        policy: OffMapPolicy,
    ) -> Result<MapProjection, MinimapError> {
        if !point.is_finite() {
            return Err(MinimapError::InvalidPoint);
        }
        let point = point.to_array().map(f64::from);
        let delta = std::array::from_fn(|i| point[i] - self.world_center[i]);
        let inside = (0..2).all(|i| point[i] >= self.world_min[i] && point[i] <= self.world_max[i]);
        if inside {
            return Ok(MapProjection {
                position: self.bounded_screen(delta),
                status: ProjectionStatus::Inside,
                off_map: None,
            });
        }
        let fraction = (0..2)
            .map(|i| self.half[i] / delta[i].abs())
            .fold(1., f64::min);
        let position = self.bounded_screen(delta.map(|v| v * fraction));
        let rotated = self.rotate(delta);
        let length = rotated[0].hypot(rotated[1]);
        let indicator = OffMapIndicator {
            position,
            direction: Vec2::from_array(rotated.map(|v| (v / length) as f32)),
        };
        let (position, status) = match policy {
            OffMapPolicy::ClampToEdge => (position, ProjectionStatus::Clamped),
            OffMapPolicy::Preserve => (checked_vec(self.screen(delta))?, ProjectionStatus::Outside),
        };
        Ok(MapProjection {
            position,
            status,
            off_map: Some(indicator),
        })
    }

    /// Atomic bounded batch, preserving input order and writing only the output
    /// prefix of length `points.len()`. Empty input succeeds. Capacity is checked
    /// first, then **every projection**, before any write. Any invalid point or
    /// unrepresentable preserved output leaves the entire output unchanged.
    /// Two passes take at most `2 * MAX_MINIMAP_POINTS` scalar projections and
    /// constant scratch space. Unknown points must be omitted before this call.
    /// See the module recipe for caller-owned ID and output buffers.
    pub fn project_into(
        &self,
        points: &[Vec2],
        policy: OffMapPolicy,
        output: &mut [MapProjection],
    ) -> Result<usize, MinimapError> {
        if points.len() > MAX_MINIMAP_POINTS || output.len() < points.len() {
            return Err(MinimapError::Capacity);
        }
        for point in points {
            self.project(*point, policy)?;
        }
        for (point, slot) in points.iter().zip(output.iter_mut()) {
            // Inputs and transform are immutable; the validation pass has already
            // checked every conversion. No callback can change them between passes.
            *slot = self.project(*point, policy)?;
        }
        Ok(points.len())
    }

    /// Inverse pick in the same logical units as `config.map`. `clip` is the
    /// caller's current safe/visible rectangle, not another scale or DPR input.
    /// Both map and clip use half-open containment. A valid empty clip is inert.
    /// Returns `None` for letterbox space or outside the rotated footprint;
    /// nonfinite pointers or invalid clips return `InvalidGeometry`.
    /// Rounded boundary picks clamp only the documented half-cell fringe.
    /// See the module recipe for safe-area and viewport composition.
    pub fn pick(&self, pointer: Vec2, clip: Rect) -> Result<Option<Vec2>, MinimapError> {
        if !pointer.is_finite() || !clip.valid() {
            return Err(MinimapError::InvalidGeometry);
        }
        if !self.config.map.contains(pointer.to_array()) || !clip.contains(pointer.to_array()) {
            return Ok(None);
        }
        let p = pointer.to_array();
        let offset: [f64; 2] = std::array::from_fn(|i| f64::from(p[i]) - self.map_center[i]);
        let delta = [
            (self.cos * offset[0] + self.sin * offset[1]) / self.scale,
            (-self.sin * offset[0] + self.cos * offset[1]) * self.y_sign / self.scale,
        ];
        // Invert the screen rounding cell as well as its center. The tiny f64
        // arithmetic allowance covers cancellation in the inverse rotation.
        let cell = p.map(rounding_radius);
        let tolerance = [
            (self.cos.abs() * cell[0] + self.sin.abs() * cell[1]) / self.scale,
            (self.sin.abs() * cell[0] + self.cos.abs() * cell[1]) / self.scale,
        ];
        if (0..2).any(|i| {
            delta[i].abs() > self.half[i] + tolerance[i] + 16. * f64::EPSILON * self.half[i]
        }) {
            return Ok(None);
        }
        let world = std::array::from_fn(|i| {
            (self.world_center[i] + delta[i].clamp(-self.half[i], self.half[i]))
                .clamp(self.world_min[i], self.world_max[i])
        });
        Ok(Some(checked_vec(world)?))
    }

    fn rotate(&self, delta: [f64; 2]) -> [f64; 2] {
        let y = delta[1] * self.y_sign;
        [
            self.cos * delta[0] - self.sin * y,
            self.sin * delta[0] + self.cos * y,
        ]
    }

    fn screen(&self, delta: [f64; 2]) -> [f64; 2] {
        let rotated = self.rotate(delta);
        std::array::from_fn(|i| self.map_center[i] + rotated[i] * self.scale)
    }

    fn bounded_screen(&self, delta: [f64; 2]) -> Vec2 {
        // The analytical fitted footprint is inside the panel. Clip only f64
        // arithmetic drift before rounding to the panel's representable edges.
        let screen = self.screen(delta);
        Vec2::from_array(std::array::from_fn(|i| {
            screen[i].clamp(self.map_min[i], self.map_max[i]) as f32
        }))
    }
}

fn checked_rect(rect: Rect) -> Result<([f64; 2], [f64; 2]), MinimapError> {
    if !rect.valid()
        || (0..2)
            .any(|i| rect.size[i] < f32::MIN_POSITIVE || rect.min[i] + rect.size[i] <= rect.min[i])
    {
        return Err(MinimapError::InvalidGeometry);
    }
    Ok((
        rect.min.map(f64::from),
        std::array::from_fn(|i| f64::from(rect.min[i] + rect.size[i])),
    ))
}

fn checked_vec(point: [f64; 2]) -> Result<Vec2, MinimapError> {
    let result = Vec2::from_array(point.map(|v| v as f32));
    result
        .is_finite()
        .then_some(result)
        .ok_or(MinimapError::Unrepresentable)
}

fn rounding_radius(value: f32) -> f64 {
    let magnitude = value.abs();
    // At MAX use the preceding spacing rather than constructing infinity.
    let spacing = if magnitude == f32::MAX {
        f64::from(magnitude) - f64::from(f32::from_bits(magnitude.to_bits() - 1))
    } else {
        f64::from(f32::from_bits(magnitude.to_bits() + 1)) - f64::from(magnitude)
    };
    spacing * 0.5
}
