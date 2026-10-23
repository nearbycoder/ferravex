//! Checked closest witnesses for finite 2D and 3D segments.
//!
//! Inputs reuse [`Vec2`] and [`Vec3`]. Coordinates are promoted **before**
//! subtraction. Parameters, witness coordinates and distance are f64: even the
//! separation of opposite `f32::MAX` points is representable. Witness arrays
//! retain the computed precision rather than rounding to the engine's f32
//! vectors; `Vec3::from_array(hit.first.map(|x| x as f32))` is an explicit
//! presentation conversion. Distance is the Euclidean distance between the
//! returned arrays, not a separate calculation from rounded f32 witnesses.
//!
//! Parameters are clamped to `0..=1`. Exactly coincident endpoints define a
//! point segment with parameter zero; short nonzero segments are not discarded.
//! Pair queries examine the four endpoint projections and, for nonparallel
//! lines, their interior closest pair. Equal computed squared separations
//! choose the lexicographically lowest `(first_parameter, second_parameter)`.
//! Comparisons subtract squared separations componentwise before summing, so a
//! large identical component does not hide improvements on another axis.
//! Thus parallel overlap selects its earliest point on the first segment;
//! reversing or swapping segments can change the selected witness in a tie.
//! Parallel means an exactly zero f64 cross product; no angle epsilon changes
//! near-parallel crossings into parallel lines. Cross products avoid the
//! cancellation of `dot(u,u) * dot(v,v) - dot(u,v).powi(2)` near parallelism.
//!
//! These are ordinary floating-point queries, not exact geometric predicates.
//! Ill-conditioned inputs and coordinates spanning widely different scales
//! still lose precision in f64. No cross-platform bitwise result is promised.
//! All queries use bounded stack storage, constant work, no heap allocation,
//! and no retained state. A rejected query cannot affect a later one. There is
//! no capacity, clock, reset, collision sweep, or route-progress state.
//!
//! # Runnable aim, tether and gate recipe
//!
//! A game can use distance to choose aim proximity, report a stretched tether,
//! or measure proximity between a movement segment and a track gate. These
//! geometric observations do not establish swept shape contact or a valid lap.
//!
//! ```rust
//! use ferravex_math::{Vec2, Vec3, vec2, vec3};
//! use ferravex_math::segments::{
//!     ClosestPoints, SegmentError, closest_point_segment2, closest_point_segment3,
//!     closest_segments2, closest_segments3,
//! };
//!
//! fn main() -> Result<(), SegmentError> {
//!     let aim: ClosestPoints<2> = closest_point_segment2(
//!         vec2(4., 1.), Vec2::ZERO, vec2(10., 0.),
//!     )?;
//!     assert!(aim.distance <= 1.5); // target is within authored aim radius
//!     assert_eq!(aim.second, [4., 0.]);
//!
//!     let tether = closest_point_segment3(
//!         vec3(3., 4., 2.), Vec3::ZERO, vec3(6., 0., 0.),
//!     )?;
//!     assert!(tether.distance > 4.); // compare in the same world units
//!     let attachment = Vec3::from_array(tether.second.map(|x| x as f32));
//!     assert_eq!(attachment, vec3(3., 0., 0.));
//!
//!     let gate = closest_segments2(
//!         vec2(-2., 0.), vec2(2., 0.), vec2(0., -1.), vec2(0., 1.),
//!     )?;
//!     assert_eq!(gate.distance, 0.);
//!     assert_eq!(gate.first_parameter, 0.5);
//!     // The game still owns gate ordering, direction, and accepted-event feedback.
//!     let elevated_gate = closest_segments3(
//!         vec3(-2., 0., 2.), vec3(2., 0., 2.),
//!         vec3(0., -1., 0.), vec3(0., 1., 0.),
//!     )?;
//!     assert_eq!(elevated_gate.distance, 2.);
//!     Ok(())
//! }
//! ```

use crate::{Vec2, Vec3};

/// A query rejected NaN or an infinite input coordinate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SegmentError {
    /// Every coordinate of every supplied point must be finite.
    NonFiniteInput,
}

impl std::fmt::Display for SegmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("segment query coordinates must be finite")
    }
}

impl std::error::Error for SegmentError {}

/// Two closest witnesses, with finite f64 coordinates and distance.
///
/// Query functions return `N = 2` or `N = 3`. For a point query, `first` is the
/// supplied point and `first_parameter` is zero. Otherwise both parameters
/// describe their respective ordered start/end inputs. See the module recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClosestPoints<const N: usize> {
    /// Point on the first segment (or the query point).
    pub first: [f64; N],
    /// Point on the second segment.
    pub second: [f64; N],
    /// Fraction along the first segment, in `0..=1`.
    pub first_parameter: f64,
    /// Fraction along the second segment, in `0..=1`.
    pub second_parameter: f64,
    /// Euclidean distance between `first` and `second`, in input world units.
    pub distance: f64,
}

/// Projects a finite point onto an ordered 2D segment, including its endpoints.
/// A zero-length segment returns its start at parameter zero.
///
/// ```
/// use ferravex_math::{Vec2, vec2};
/// use ferravex_math::segments::closest_point_segment2;
/// let hit = closest_point_segment2(vec2(3., 4.), Vec2::ZERO, Vec2::X)?;
/// assert_eq!(hit.second, [1., 0.]);
/// assert_eq!(hit.second_parameter, 1.);
/// assert_eq!(hit.distance, 20_f64.sqrt());
/// # Ok::<(), ferravex_math::segments::SegmentError>(())
/// ```
pub fn closest_point_segment2(
    point: Vec2,
    start: Vec2,
    end: Vec2,
) -> Result<ClosestPoints<2>, SegmentError> {
    point_segment(point.to_array(), start.to_array(), end.to_array())
}

/// Projects a finite point onto an ordered 3D segment. See the module's f64
/// witness policy; nonfinite inputs are errors, not fallback points.
///
/// ```
/// use ferravex_math::{Vec3, vec3};
/// use ferravex_math::segments::closest_point_segment3;
/// let hit = closest_point_segment3(vec3(0., 2., 3.), Vec3::ZERO, Vec3::Z * 6.)?;
/// assert_eq!(hit.second, [0., 0., 3.]);
/// assert_eq!(hit.second_parameter, 0.5);
/// assert_eq!(hit.distance, 2.);
/// # Ok::<(), ferravex_math::segments::SegmentError>(())
/// ```
pub fn closest_point_segment3(
    point: Vec3,
    start: Vec3,
    end: Vec3,
) -> Result<ClosestPoints<3>, SegmentError> {
    point_segment(point.to_array(), start.to_array(), end.to_array())
}

/// Finds closest points on two finite ordered 2D segments, including crossings,
/// parallel overlap and point segments. Equal computed squared separations prefer the
/// lowest first parameter, then the lowest second parameter.
///
/// ```
/// use ferravex_math::{Vec2, vec2};
/// use ferravex_math::segments::closest_segments2;
/// let hit = closest_segments2(Vec2::ZERO, Vec2::X * 4., vec2(2., 0.), vec2(6., 0.))?;
/// assert_eq!((hit.first_parameter, hit.second_parameter), (0.5, 0.));
/// assert_eq!(hit.first, hit.second);
/// assert_eq!(hit.distance, 0.);
/// # Ok::<(), ferravex_math::segments::SegmentError>(())
/// ```
pub fn closest_segments2(
    first_start: Vec2,
    first_end: Vec2,
    second_start: Vec2,
    second_end: Vec2,
) -> Result<ClosestPoints<2>, SegmentError> {
    segment_pair(
        first_start.to_array(),
        first_end.to_array(),
        second_start.to_array(),
        second_end.to_array(),
    )
}

/// Finds closest points on two finite ordered 3D segments, including skew and
/// point segments. No normalization or minimum-length threshold is required.
///
/// ```
/// use ferravex_math::vec3;
/// use ferravex_math::segments::closest_segments3;
/// let hit = closest_segments3(
///     vec3(-1., 0., 0.), vec3(1., 0., 0.),
///     vec3(0., -1., 3.), vec3(0., 1., 3.),
/// )?;
/// assert_eq!(hit.first, [0., 0., 0.]);
/// assert_eq!(hit.second, [0., 0., 3.]);
/// assert_eq!(hit.distance, 3.);
/// # Ok::<(), ferravex_math::segments::SegmentError>(())
/// ```
pub fn closest_segments3(
    first_start: Vec3,
    first_end: Vec3,
    second_start: Vec3,
    second_end: Vec3,
) -> Result<ClosestPoints<3>, SegmentError> {
    segment_pair(
        first_start.to_array(),
        first_end.to_array(),
        second_start.to_array(),
        second_end.to_array(),
    )
}

fn finite<const N: usize>(point: [f32; N]) -> Result<[f64; N], SegmentError> {
    if point.iter().all(|x| x.is_finite()) {
        Ok(point.map(f64::from))
    } else {
        Err(SegmentError::NonFiniteInput)
    }
}

fn sub<const N: usize>(a: [f64; N], b: [f64; N]) -> [f64; N] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn dot<const N: usize>(a: [f64; N], b: [f64; N]) -> f64 {
    (0..N).map(|i| a[i] * b[i]).sum()
}

fn interpolate<const N: usize>(a: [f64; N], b: [f64; N], t: f64) -> [f64; N] {
    if t == 0. {
        a
    } else if t == 1. {
        b
    } else {
        std::array::from_fn(|i| a[i] * (1. - t) + b[i] * t)
    }
}

fn project<const N: usize>(point: [f64; N], a: [f64; N], b: [f64; N]) -> f64 {
    let delta = sub(b, a);
    let length_squared = dot(delta, delta);
    if length_squared == 0. {
        return 0.;
    }
    let t = dot(sub(point, a), delta) / length_squared;
    if t <= 0. { 0. } else { t.min(1.) }
}

fn witnesses<const N: usize>(
    a: [f64; N],
    b: [f64; N],
    c: [f64; N],
    d: [f64; N],
    s: f64,
    t: f64,
) -> ClosestPoints<N> {
    let first = interpolate(a, b, s);
    let second = interpolate(c, d, t);
    let offset = sub(first, second);
    ClosestPoints {
        first,
        second,
        first_parameter: s,
        second_parameter: t,
        // Hypot keeps tiny nonzero separation even when squaring it underflows.
        distance: offset.into_iter().fold(0., f64::hypot),
    }
}

fn point_segment<const N: usize>(
    point: [f32; N],
    a: [f32; N],
    b: [f32; N],
) -> Result<ClosestPoints<N>, SegmentError> {
    let (point, a, b) = (finite(point)?, finite(a)?, finite(b)?);
    Ok(witnesses(point, point, a, b, 0., project(point, a, b)))
}

// Compare norms without first rounding away small differences under an equal
// large component. Neumaier summation also retains residuals when components
// of the difference have opposite signs. This is still f64, not an exact test.
fn squared_difference<const N: usize>(a: &ClosestPoints<N>, b: &ClosestPoints<N>) -> f64 {
    let (mut sum, mut correction) = (0_f64, 0_f64);
    for i in 0..N {
        let x = a.first[i] - a.second[i];
        let y = b.first[i] - b.second[i];
        let term = (x - y) * (x + y);
        let next = sum + term;
        correction += if sum.abs() >= term.abs() {
            (sum - next) + term
        } else {
            (term - next) + sum
        };
        sum = next;
    }
    sum + correction
}

// Compensate the rounded second product when taking a 2x2 determinant.
fn determinant(a: f64, b: f64, c: f64, d: f64) -> f64 {
    let cd = c * d;
    a.mul_add(b, -cd) + (-c).mul_add(d, cd)
}

fn segment_pair<const N: usize>(
    a: [f32; N],
    b: [f32; N],
    c: [f32; N],
    d: [f32; N],
) -> Result<ClosestPoints<N>, SegmentError> {
    let (a, b, c, d) = (finite(a)?, finite(b)?, finite(c)?, finite(d)?);
    let u = sub(b, a);
    let v = sub(d, c);
    if dot(u, u) == 0. {
        return Ok(witnesses(a, b, c, d, 0., project(a, c, d)));
    }
    if dot(v, v) == 0. {
        return Ok(witnesses(a, b, c, d, project(c, a, b), 0.));
    }

    // Every constrained minimum lies on a boundary or is a stationary point
    // of the two infinite lines. Enumerating boundaries avoids division by a
    // small Gram determinant for parallel/endpoint cases.
    let candidates = [
        (0., project(a, c, d)),
        (1., project(b, c, d)),
        (project(c, a, b), 0.),
        (project(d, a, b), 1.),
    ];
    let mut best = witnesses(a, b, c, d, candidates[0].0, candidates[0].1);
    let mut consider = |s, t| {
        let candidate = witnesses(a, b, c, d, s, t);
        let difference = squared_difference(&candidate, &best);
        if difference < 0.
            || (difference == 0. && (s, t) < (best.first_parameter, best.second_parameter))
        {
            best = candidate;
        }
    };
    for (s, t) in candidates.into_iter().skip(1) {
        consider(s, t);
    }

    // Wedge products work in either dimension. In 3D these are the components
    // of cross(u,v), cross(c-a,v), and cross(c-a,u), with a shared sign per axis.
    // Their dot ratios give the unconstrained closest-line parameters without
    // subtracting almost equal squared dot products.
    let w = sub(c, a);
    let (mut denominator, mut s_numerator, mut t_numerator) = (0., 0., 0.);
    for i in 0..N {
        for j in i + 1..N {
            let n = determinant(u[i], v[j], u[j], v[i]);
            denominator += n * n;
            s_numerator += determinant(w[i], v[j], w[j], v[i]) * n;
            t_numerator += determinant(w[i], u[j], w[j], u[i]) * n;
        }
    }
    if denominator > 0. {
        let (s, t) = (s_numerator / denominator, t_numerator / denominator);
        if (0. ..=1.).contains(&s) && (0. ..=1.).contains(&t) {
            // Canonicalize negative zero in the public parameters.
            consider(if s == 0. { 0. } else { s }, if t == 0. { 0. } else { t });
        }
    }
    Ok(best)
}
