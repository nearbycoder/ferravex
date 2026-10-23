//! Checked cubic Bezier authoring with allocation-free, bounded tessellation.
//!
//! [`CubicBezier3`] stores four finite f32 control points. Sampling uses f64
//! intermediates; positions round to Vec3, while tangents retain f64 derivative
//! components (not unit directions). A constant curve has zero tangent; a zero
//! computed derivative at a cusp stays zero, without a fallback direction.
//! Samples use ordinary floating-point arithmetic, not exact polynomial values.
//!
//! Tessellation emits an ordered **prefix**, beginning at control point zero.
//! Only accepted chords are written. [`TessellationStatus::Complete`] certifies
//! the whole curve; a limit leaves an incomplete prefix, never an unchecked
//! coarse chord to the final endpoint. Reuse caller storage on the next call;
//! there is no retained state, append mode, resume cursor, clock or reset.
//!
//! The error test uses outward-rounded f64 intervals for dyadic de Casteljau
//! subdivision. It bounds the L1 norm of the difference between each subcurve's
//! four control points and the degree-elevated control points of its **rounded
//! output chord**. Convexity then bounds Euclidean distance at paired local
//! parameters, in either direction, by that value. This includes subdivision
//! arithmetic and f32 endpoint rounding. It is conservative: a curved or even
//! collinear, nonuniformly parameterized curve can need extra subdivisions.
//! Tiny tolerances can saturate because output quantization/interval width
//! cannot be resolved. Tolerance is in control-point world units, not pixels,
//! angle, arc length or error in route-progress parameterization.
//!
//! # Runnable native authoring recipe
//!
//! This uses only math. A route owner can pass `&points[..result.written]` to
//! the existing `ferravex_play::navigation::Route3::set` **after** checking
//! `Complete`; Route3 continues to own arc-length sampling and progression.
//!
//! ```rust
//! use ferravex_math::{Vec3, vec3};
//! use ferravex_math::curves::{
//!     CubicBezier3, CurveError, TessellationOptions, TessellationStatus,
//! };
//!
//! fn main() -> Result<(), CurveError> {
//!     let curve = CubicBezier3::new([
//!         Vec3::ZERO, vec3(0., 2., 0.), vec3(4., 2., 0.), vec3(4., 0., 0.),
//!     ])?;
//!     assert_eq!(curve.position(0.)?, curve.control_points()[0]);
//!     assert_eq!(curve.position(1.)?, curve.control_points()[3]);
//!     assert_eq!(curve.tangent(0.)?, [0., 6., 0.]);
//!     let mut points = [Vec3::ZERO; 128];
//!     let result = curve.tessellate_into(&mut points, TessellationOptions {
//!         tolerance: 0.01, max_depth: 16, max_work: 1023,
//!     })?;
//!     assert_eq!(result.status, TessellationStatus::Complete);
//!     assert_eq!(result.covered_until, 1.);
//!     assert!(result.max_error_bound <= 0.01);
//!     assert_eq!(points[result.written - 1], curve.position(1.)?);
//!     Ok(())
//! }
//! ```

use crate::Vec3;

/// Largest permitted subdivision depth; bounds the internal DFS stack to 33
/// nodes. The independently supplied work limit bounds the number examined.
pub const MAX_TESSELLATION_DEPTH: u8 = 32;

/// Invalid authoring input. Errors are distinct from successful partial output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CurveError {
    /// All coordinates of all four control points must be finite.
    NonFiniteControl,
    /// Sampling requires a finite parameter in `0..=1`; it never clamps.
    InvalidParameter,
    /// Tolerance must be finite and strictly positive.
    InvalidTolerance,
    /// Depth must not exceed [`MAX_TESSELLATION_DEPTH`].
    InvalidDepth,
}

impl std::fmt::Display for CurveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NonFiniteControl => "cubic control points must be finite",
            Self::InvalidParameter => "cubic parameter must be finite and in 0..=1",
            Self::InvalidTolerance => "cubic tolerance must be finite and positive",
            Self::InvalidDepth => "cubic subdivision depth exceeds the fixed stack limit",
        })
    }
}

impl std::error::Error for CurveError {}

/// Explicit geometric and resource budgets. See the module recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TessellationOptions {
    /// Positive finite error tolerance in world units, including output rounding.
    pub tolerance: f64,
    /// Number of bisections along any branch, in `0..=MAX_TESSELLATION_DEPTH`.
    /// Zero permits only testing the original curve's chord.
    pub max_depth: u8,
    /// Maximum nodes examined (one error test and at most one split per node).
    /// Zero emits only the start if storage permits. Fixed validation and stack
    /// initialization are constant overhead outside this node counter.
    pub max_work: usize,
}

/// Why tessellation stopped. Limits return `Ok`, with only certified prefix
/// chords. If multiple limits coincide, output capacity is checked before work;
/// subdivision depth is checked only after a node fails its error test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TessellationStatus {
    /// The prefix covers all of `0..=1`, including both original endpoints.
    Complete,
    /// Another endpoint could not fit; empty storage writes nothing.
    OutputFull,
    /// The next node would exceed the work budget.
    WorkLimit,
    /// A failed error test reached the requested depth. This may also reflect
    /// f32 quantization or interval rounding, not just geometric curvature.
    SubdivisionLimit,
}

/// Result of a valid tessellation request; see the module recipe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tessellation {
    /// Initialized output prefix length; the remaining slice is unchanged.
    pub written: usize,
    /// Number of nodes examined, at most `options.max_work`.
    pub work: usize,
    /// Number of actual bisections, at most `work`.
    pub subdivisions: usize,
    /// Deepest examined node, at most `options.max_depth`; zero if none.
    pub depth_reached: u8,
    /// End parameter of the last accepted chord; zero if none. No statement
    /// about the unprocessed remainder is implied by a saturated result.
    pub covered_until: f64,
    /// Maximum certified Euclidean error bound of emitted chords, at most the
    /// requested tolerance. Zero if no chord was emitted. See the module's
    /// conservative L1 construction; this is not a measured worst deviation.
    pub max_error_bound: f64,
    /// Completion or the first limiting resource.
    pub status: TessellationStatus,
}

/// One cubic Bezier in three dimensions, with checked finite control points.
/// Construction and all operations use fixed storage and allocate nothing.
/// See the module's executable recipe for every operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier3 {
    control_points: [Vec3; 4],
}

impl CubicBezier3 {
    /// Checks all four points, including for constant/degenerate curves.
    pub fn new(control_points: [Vec3; 4]) -> Result<Self, CurveError> {
        if control_points.iter().any(|point| !point.is_finite()) {
            return Err(CurveError::NonFiniteControl);
        }
        Ok(Self { control_points })
    }

    /// Original ordered points, suitable for editing and checked reconstruction.
    pub fn control_points(&self) -> [Vec3; 4] {
        self.control_points
    }

    /// Samples a finite parameter in `0..=1`, using f64 de Casteljau
    /// interpolation before rounding to Vec3. Endpoints preserve their original
    /// bits, including signed zeros. No extrapolation or clamping occurs.
    pub fn position(&self, parameter: f64) -> Result<Vec3, CurveError> {
        check_parameter(parameter)?;
        if parameter == 0. {
            return Ok(self.control_points[0]);
        }
        if parameter == 1. {
            return Ok(self.control_points[3]);
        }
        let mut points = self.control_points.map(|p| p.to_array().map(f64::from));
        for count in (1..4).rev() {
            for i in 0..count {
                points[i] = lerp(points[i], points[i + 1], parameter);
            }
        }
        Ok(Vec3::from_array(points[0].map(|x| x as f32)))
    }

    /// Unnormalized derivative of the authored polynomial, before position
    /// rounding, as finite f64 components. Extreme f32 controls cannot overflow
    /// the tangent. Exact computed zero at a cusp/constant curve remains zero;
    /// small derivatives are not discarded by an epsilon. There is no
    /// unit-tangent fallback.
    pub fn tangent(&self, parameter: f64) -> Result<[f64; 3], CurveError> {
        check_parameter(parameter)?;
        let points = self.control_points.map(|p| p.to_array().map(f64::from));
        let derivative: [[f64; 3]; 3] = std::array::from_fn(|i| {
            std::array::from_fn(|axis| 3. * (points[i + 1][axis] - points[i][axis]))
        });
        if parameter == 0. {
            return Ok(derivative[0]);
        }
        if parameter == 1. {
            return Ok(derivative[2]);
        }
        Ok(lerp(
            lerp(derivative[0], derivative[1], parameter),
            lerp(derivative[1], derivative[2], parameter),
            parameter,
        ))
    }

    /// Adaptively bisects left-first into caller storage. Invalid options leave
    /// the entire output unchanged. Valid requests overwrite only the reported
    /// prefix, including on saturation. A constant curve emits two equal points
    /// on completion, preserving both endpoint parameters.
    ///
    /// Only `Complete` is a whole-curve tolerance certificate. A partial result
    /// is safe to inspect, but callers must explicitly choose whether to use
    /// that shorter path. Increasing a budget starts a fresh run. Stack storage
    /// is fixed (33 nodes); work and output never grow implicitly. See the
    /// module recipe and the bound described in the module documentation.
    pub fn tessellate_into(
        &self,
        output: &mut [Vec3],
        options: TessellationOptions,
    ) -> Result<Tessellation, CurveError> {
        if !options.tolerance.is_finite() || options.tolerance <= 0. {
            return Err(CurveError::InvalidTolerance);
        }
        if options.max_depth > MAX_TESSELLATION_DEPTH {
            return Err(CurveError::InvalidDepth);
        }
        let mut result = Tessellation {
            written: 0,
            work: 0,
            subdivisions: 0,
            depth_reached: 0,
            covered_until: 0.,
            max_error_bound: 0.,
            status: TessellationStatus::Complete,
        };
        if output.is_empty() {
            result.status = TessellationStatus::OutputFull;
            return Ok(result);
        }
        output[0] = self.control_points[0];
        result.written = 1;
        let root = Node {
            points: self
                .control_points
                .map(|p| p.to_array().map(|x| Interval::exact(f64::from(x)))),
            start: 0.,
            end: 1.,
            depth: 0,
        };
        let mut stack = [root; MAX_TESSELLATION_DEPTH as usize + 1];
        let mut pending = 1;
        while pending > 0 {
            if result.written == output.len() {
                result.status = TessellationStatus::OutputFull;
                break;
            }
            if result.work == options.max_work {
                result.status = TessellationStatus::WorkLimit;
                break;
            }
            pending -= 1;
            let node = stack[pending];
            result.work += 1;
            result.depth_reached = result.depth_reached.max(node.depth);
            let end = if node.end == 1. {
                self.control_points[3]
            } else {
                Vec3::from_array(node.points[3].map(|x| (x.low * 0.5 + x.high * 0.5) as f32))
            };
            let error = node.error_bound(output[result.written - 1], end);
            if error <= options.tolerance {
                output[result.written] = end;
                result.written += 1;
                result.covered_until = node.end;
                result.max_error_bound = result.max_error_bound.max(error);
            } else if node.depth == options.max_depth {
                result.status = TessellationStatus::SubdivisionLimit;
                break;
            } else {
                let (left, right) = node.split();
                stack[pending] = right;
                stack[pending + 1] = left;
                pending += 2;
                result.subdivisions += 1;
            }
        }
        Ok(result)
    }
}

fn check_parameter(parameter: f64) -> Result<(), CurveError> {
    if !parameter.is_finite() || !(0. ..=1.).contains(&parameter) {
        Err(CurveError::InvalidParameter)
    } else {
        Ok(())
    }
}

fn lerp(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    std::array::from_fn(|axis| (1. - t) * a[axis] + t * b[axis])
}

// All interval operands originate as finite f32, with at most 32 bisections.
// Outward steps can introduce f64 subnormals around zero; each operation is
// enclosed separately, including division/multiplication that might underflow.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Interval {
    low: f64,
    high: f64,
}

impl Interval {
    fn exact(value: f64) -> Self {
        Self {
            low: value,
            high: value,
        }
    }

    fn midpoint(self, other: Self) -> Self {
        if self == other {
            return self;
        }
        Self {
            low: ((self.low + other.low).next_down() * 0.5).next_down(),
            high: ((self.high + other.high).next_up() * 0.5).next_up(),
        }
    }

    // Exact line control point (2*a + b)/3 enclosed without assuming that 1/3
    // is representable. The equal-endpoint shortcut is exact, not an epsilon.
    fn third(a: f64, b: f64) -> Self {
        if a == b {
            return Self::exact(a);
        }
        Self {
            low: (((a * 2.).next_down() + b).next_down() / 3.).next_down(),
            high: (((a * 2.).next_up() + b).next_up() / 3.).next_up(),
        }
    }

    fn difference_bound(self, other: Self) -> f64 {
        if self.low == self.high && self == other {
            return 0.;
        }
        (self.low - other.high)
            .next_down()
            .abs()
            .max((self.high - other.low).next_up().abs())
    }
}

#[derive(Clone, Copy)]
struct Node {
    points: [[Interval; 3]; 4],
    start: f64,
    end: f64,
    depth: u8,
}

impl Node {
    fn error_bound(self, start: Vec3, end: Vec3) -> f64 {
        let start = start.to_array().map(f64::from);
        let end = end.to_array().map(f64::from);
        let mut bound = 0_f64;
        for i in 0..4 {
            let mut sum = 0_f64;
            for axis in 0..3 {
                let line = match i {
                    0 => Interval::exact(start[axis]),
                    1 => Interval::third(start[axis], end[axis]),
                    2 => Interval::third(end[axis], start[axis]),
                    _ => Interval::exact(end[axis]),
                };
                let difference = self.points[i][axis].difference_bound(line);
                if difference > 0. {
                    sum = (sum + difference).next_up();
                }
            }
            bound = bound.max(sum);
        }
        bound
    }

    fn split(self) -> (Self, Self) {
        let mid = |a: [Interval; 3], b: [Interval; 3]| {
            std::array::from_fn(|axis| a[axis].midpoint(b[axis]))
        };
        let [a, b, c, d] = self.points;
        let (ab, bc, cd) = (mid(a, b), mid(b, c), mid(c, d));
        let (abc, bcd) = (mid(ab, bc), mid(bc, cd));
        let center = mid(abc, bcd);
        let parameter = (self.start + self.end) * 0.5;
        (
            Self {
                points: [a, ab, abc, center],
                end: parameter,
                depth: self.depth + 1,
                ..self
            },
            Self {
                points: [center, bcd, cd, d],
                start: parameter,
                depth: self.depth + 1,
                ..self
            },
        )
    }
}
