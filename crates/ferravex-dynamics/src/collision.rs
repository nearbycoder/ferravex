use crate::{PhysicsError, Shape, math::*};

/// How the first contact of a checked ball sweep was reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SweepHitKind {
    /// The ball starts strictly inside the target's inflated boundary.
    InitialOverlap,
    /// The ball starts exactly on the boundary, regardless of movement direction.
    InitialTouch,
    /// The moving ball enters the boundary after the start.
    Impact,
    /// The moving ball touches the boundary tangentially after the start.
    Grazing,
}

/// First contact of a ball translated over a finite segment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepHit<const D: usize> {
    /// Fraction of the supplied displacement in `[0, 1]`, including both ends.
    pub fraction: f32,
    /// Unit normal pointing out of the stationary target toward the moving ball.
    /// Coincident ball centers use the opposite movement direction, or +X for
    /// zero movement. Inside boxes use the closest face, with X winning ties.
    pub normal: [f32; D],
    pub kind: SweepHitKind,
}

impl<const D: usize> Shape<D> {
    /// Sweep a ball against this stationary shape at `center`.
    ///
    /// `displacement` is the complete translation, not a direction or velocity.
    /// Zero movement still reports initial overlap/touch; a separated stationary
    /// ball returns `Ok(None)`. Initial contacts take precedence over later hits,
    /// even when moving away. Tangencies and endpoint hits are included. This
    /// geometric query does not apply the world's sliding/CCD contact policy.
    ///
    /// Shapes/radius and all coordinates use the existing world limits: radii
    /// and half extents in `[1e-5, 1_000_000]`, and center, origin and displacement
    /// components in `[-1_000_000, 1_000_000]`. Invalid/nonfinite inputs return
    /// [`PhysicsError::InvalidBody`]; dimensions other than 2/3 return
    /// [`PhysicsError::InvalidDimension`]. Intermediates use f64, with results
    /// rounded to f32; this is not an exact-arithmetic predicate.
    ///
    /// ```
    /// use ferravex_dynamics::Shape;
    /// let wall = Shape::cuboid([1., 1.]);
    /// let hit = wall.sweep_ball([0., 0.], [-3., 0.], 0.5, [4., 0.])?
    ///     .unwrap();
    /// assert_eq!(hit.fraction, 0.375);
    /// assert_eq!(hit.normal, [-1., 0.]);
    /// let overlap = wall.sweep_ball([0., 0.], [0., 0.], 0.5, [0., 0.])?
    ///     .unwrap();
    /// assert_eq!(overlap.fraction, 0.);
    /// assert!(wall.sweep_ball([0., 0.], [-3., 0.], 0.5, [0., 0.])?.is_none());
    /// # Ok::<(), ferravex_dynamics::PhysicsError>(())
    /// ```
    pub fn sweep_ball(
        self,
        center: [f32; D],
        origin: [f32; D],
        radius: f32,
        displacement: [f32; D],
    ) -> Result<Option<SweepHit<D>>, PhysicsError> {
        if D != 2 && D != 3 {
            return Err(PhysicsError::InvalidDimension);
        }
        if !self.valid()
            || !Shape::<D>::ball(radius).valid()
            || !valid(center)
            || !valid(origin)
            || !valid(displacement)
        {
            return Err(PhysicsError::InvalidBody);
        }
        let p = relative64(origin, center);
        let v = displacement.map(f64::from);
        let hit = match self {
            Shape::Ball { radius: target } => {
                trace_ball(p, v, f64::from(radius) + f64::from(target), 1.)
            }
            Shape::Box { half_extents } => {
                trace_ball_box(p, f64::from(radius), v, half_extents.map(f64::from), 1.)
            }
        };
        Ok(hit.map(|hit| SweepHit {
            fraction: hit.time as f32,
            normal: hit.normal,
            kind: hit.kind,
        }))
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Manifold<const D: usize> {
    /// From the first shape toward the second.
    pub normal: [f32; D],
    pub penetration: f32,
}

pub(crate) fn contact<const D: usize>(
    a: Shape<D>,
    pa: [f32; D],
    b: Shape<D>,
    pb: [f32; D],
    margin: f32,
) -> Option<Manifold<D>> {
    match (a, b) {
        (Shape::Ball { radius: ra }, Shape::Ball { radius: rb }) => {
            let delta = sub(pb, pa);
            let distance = length(delta);
            if distance > ra + rb + margin {
                return None;
            }
            Some(Manifold {
                normal: normalize(delta).unwrap_or_else(|| unit(0, 1.)),
                penetration: (ra + rb - distance).max(0.),
            })
        }
        (Shape::Box { half_extents: ha }, Shape::Box { half_extents: hb }) => {
            let delta = sub(pb, pa);
            let mut penetration = f32::INFINITY;
            let mut axis = 0;
            for i in 0..D {
                let overlap = ha[i] + hb[i] - delta[i].abs();
                if overlap < -margin {
                    return None;
                }
                if overlap < penetration {
                    penetration = overlap;
                    axis = i;
                }
            }
            Some(Manifold {
                normal: unit(axis, if delta[axis] >= 0. { 1. } else { -1. }),
                penetration: penetration.max(0.),
            })
        }
        (Shape::Ball { radius }, Shape::Box { half_extents }) => {
            ball_box(pa, radius, pb, half_extents, margin)
        }
        (Shape::Box { half_extents }, Shape::Ball { radius }) => {
            ball_box(pb, radius, pa, half_extents, margin).map(|m| Manifold {
                normal: mul(m.normal, -1.),
                ..m
            })
        }
    }
}
fn ball_box<const D: usize>(
    p: [f32; D],
    radius: f32,
    center: [f32; D],
    half: [f32; D],
    margin: f32,
) -> Option<Manifold<D>> {
    let local = sub(p, center);
    let nearest = std::array::from_fn(|i| local[i].clamp(-half[i], half[i]));
    let toward_box = sub(nearest, local);
    let distance = length(toward_box);
    if distance > radius + margin {
        return None;
    }
    if distance > 1e-7 {
        return Some(Manifold {
            normal: mul(toward_box, distance.recip()),
            penetration: (radius - distance).max(0.),
        });
    }
    // A ball whose center is inside the box must exit through the closest face.
    let axis = (0..D)
        .min_by(|&a, &b| (half[a] - local[a].abs()).total_cmp(&(half[b] - local[b].abs())))
        .unwrap();
    Some(Manifold {
        normal: unit(axis, if local[axis] >= 0. { -1. } else { 1. }),
        penetration: radius + half[axis] - local[axis].abs(),
    })
}

// Query kernels use f64 before subtracting, squaring or inflating. This also
// keeps tiny colliders visible from distant origins within the world limits.
fn relative64<const D: usize>(a: [f32; D], b: [f32; D]) -> [f64; D] {
    std::array::from_fn(|i| f64::from(a[i]) - f64::from(b[i]))
}
fn dot64<const D: usize>(a: [f64; D], b: [f64; D]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn normal64<const D: usize>(v: [f64; D]) -> Option<[f32; D]> {
    let length = dot64(v, v).sqrt();
    (length > 0.).then(|| v.map(|x| (x / length) as f32))
}
fn query_inputs<const D: usize>(
    origin: [f32; D],
    direction: [f32; D],
    center: [f32; D],
    max: f32,
) -> bool {
    (D == 2 || D == 3)
        && origin
            .into_iter()
            .chain(direction)
            .chain(center)
            .all(f32::is_finite)
        && max.is_finite()
        && max >= 0.
}

#[derive(Clone, Copy, Debug)]
struct TraceHit<const D: usize> {
    time: f64,
    normal: [f32; D],
    kind: SweepHitKind,
    penetration: f64,
}

// Unlike b*b - a*c, the closest-point form does not lose the radius squared
// when a small sphere is far down the ray. Work with the actual direction
// length: normalizing an f32 vector does not make its squared length exactly 1.
fn trace_ball<const D: usize>(
    p: [f64; D],
    v: [f64; D],
    radius: f64,
    max: f64,
) -> Option<TraceHit<D>> {
    let distance = dot64(p, p).sqrt();
    if distance <= radius {
        return Some(TraceHit {
            time: 0.,
            normal: normal64(p)
                .or_else(|| normal64(v.map(|x| -x)))
                .unwrap_or_else(|| unit(0, 1.)),
            kind: if distance < radius {
                SweepHitKind::InitialOverlap
            } else {
                SweepHitKind::InitialTouch
            },
            penetration: radius - distance,
        });
    }
    let a = dot64(v, v);
    let b = dot64(p, v);
    if a == 0. || b >= 0. {
        return None;
    }
    let closest_time = -b / a;
    let closest: [f64; D] = std::array::from_fn(|i| v[i].mul_add(closest_time, p[i]));
    let height_squared = radius * radius - dot64(closest, closest);
    if height_squared < 0. {
        return None;
    }
    let offset = (height_squared / a).sqrt();
    // The product form of c preserves very short entry distances, while the
    // closest-point normal avoids cancellation at far-away small colliders.
    let c = (distance - radius) * (distance + radius);
    let time = if offset > closest_time * 0.5 {
        c / (-b + (a * height_squared).sqrt())
    } else {
        closest_time - offset
    };
    if time > max {
        return None;
    }
    let outward = std::array::from_fn(|i| (-v[i]).mul_add(offset, closest[i]));
    Some(TraceHit {
        time,
        normal: normal64(outward)
            .or_else(|| normal64(v.map(|x| -x)))
            .unwrap_or_else(|| unit(0, 1.)),
        kind: if height_squared == 0. {
            SweepHitKind::Grazing
        } else {
            SweepHitKind::Impact
        },
        penetration: 0.,
    })
}

/// Ray parameter measured in world units because callers normalize direction.
pub(crate) fn ray_ball<const D: usize>(
    origin: [f32; D],
    direction: [f32; D],
    center: [f32; D],
    radius: f32,
    max: f32,
) -> Option<(f32, [f32; D])> {
    if !query_inputs(origin, direction, center, max) || !radius.is_finite() || radius < 0. {
        return None;
    }
    trace_ball(
        relative64(origin, center),
        direction.map(f64::from),
        f64::from(radius),
        f64::from(max),
    )
    .map(|hit| (hit.time as f32, hit.normal))
}

fn trace_box<const D: usize>(
    p: [f64; D],
    v: [f64; D],
    half: [f64; D],
    max: f64,
) -> Option<(f64, [f32; D])> {
    let mut enter: f64 = 0.;
    let mut exit = max;
    let mut normal = normal64(v.map(|x| -x)).unwrap_or_else(|| unit(0, 1.));
    for i in 0..D {
        // A tiny nonzero component can cross a slab over a long segment.
        if v[i] == 0. {
            if p[i].abs() > half[i] {
                return None;
            }
            continue;
        }
        let t1 = (-half[i] - p[i]) / v[i];
        let t2 = (half[i] - p[i]) / v[i];
        let near = t1.min(t2);
        if near > enter {
            enter = near;
            normal = unit(i, -v[i].signum() as f32);
        }
        exit = exit.min(t1.max(t2));
        if enter > exit {
            return None;
        }
    }
    Some((enter, normal))
}

pub(crate) fn ray_box<const D: usize>(
    origin: [f32; D],
    direction: [f32; D],
    center: [f32; D],
    half: [f32; D],
    max: f32,
) -> Option<(f32, [f32; D])> {
    if !query_inputs(origin, direction, center, max)
        || half.iter().any(|h| !h.is_finite() || *h < 0.)
    {
        return None;
    }
    trace_box(
        relative64(origin, center),
        direction.map(f64::from),
        half.map(f64::from),
        f64::from(max),
    )
    .map(|(time, normal)| (time as f32, normal))
}

// Each AABB feature contributes a face, edge-cylinder or corner-sphere root.
// At most 26 features are examined in 3D, eight in 2D. All storage is on-stack.
fn trace_ball_box<const D: usize>(
    p: [f64; D],
    radius: f64,
    v: [f64; D],
    half: [f64; D],
    max: f64,
) -> Option<TraceHit<D>> {
    trace_box(p, v, half.map(|h| h + radius), max)?;
    let outward = std::array::from_fn(|i| p[i] - p[i].clamp(-half[i], half[i]));
    let distance = dot64(outward, outward).sqrt();
    if distance <= radius {
        let (normal, penetration) = if distance > 0. {
            (normal64(outward).unwrap(), radius - distance)
        } else {
            let axis = (0..D)
                .min_by(|&a, &b| (half[a] - p[a].abs()).total_cmp(&(half[b] - p[b].abs())))
                .unwrap();
            (
                unit(axis, if p[axis] >= 0. { 1. } else { -1. }),
                radius + half[axis] - p[axis].abs(),
            )
        };
        return Some(TraceHit {
            time: 0.,
            normal,
            kind: if penetration > 0. {
                SweepHitKind::InitialOverlap
            } else {
                SweepHitKind::InitialTouch
            },
            penetration,
        });
    }
    let mut best = max;
    let mut result = None;
    for axes in 1..(1_usize << D) {
        for signs in 0..(1_usize << D) {
            if signs & !axes != 0 {
                continue;
            }
            let mut q = [0.; D];
            let mut projected = [0.; D];
            for i in 0..D {
                if axes & (1 << i) != 0 {
                    let sign = if signs & (1 << i) != 0 { 1. } else { -1. };
                    q[i] = p[i] - sign * half[i];
                    projected[i] = v[i];
                }
            }
            let Some(hit) = trace_ball(q, projected, radius, best) else {
                continue;
            };
            // Starting inside the projected feature does not mean starting
            // inside its bounded face/edge. Adjacent features cover its entry.
            if matches!(
                hit.kind,
                SweepHitKind::InitialOverlap | SweepHitKind::InitialTouch
            ) {
                continue;
            }
            let within = (0..D).all(|i| {
                if axes & (1 << i) != 0 {
                    let sign = if signs & (1 << i) != 0 { 1. } else { -1. };
                    f64::from(hit.normal[i]) * sign >= 0.
                } else {
                    v[i].mul_add(hit.time, p[i]).abs() <= half[i]
                }
            });
            if within {
                best = hit.time;
                result = Some(hit);
            }
        }
    }
    result
}

/// Exact sphere/circle sweep against an AABB, including rounded corners/edges.
/// Retains the solver policy: tangencies and departing surface contacts do not
/// block motion; overlaps deeper than the existing 1e-5 slop do.
pub(crate) fn sweep_ball_box<const D: usize>(
    origin: [f32; D],
    radius: f32,
    direction: [f32; D],
    center: [f32; D],
    half: [f32; D],
    max: f32,
) -> Option<(f32, [f32; D])> {
    if !query_inputs(origin, direction, center, max)
        || !radius.is_finite()
        || radius <= 0.
        || half.iter().any(|h| !h.is_finite() || *h < 0.)
    {
        return None;
    }
    let hit = trace_ball_box(
        relative64(origin, center),
        f64::from(radius),
        direction.map(f64::from),
        half.map(f64::from),
        f64::from(max),
    )?;
    let into = dot(direction, hit.normal);
    let blocking = match hit.kind {
        SweepHitKind::InitialOverlap => hit.penetration > 1e-5 || into < -1e-6,
        SweepHitKind::InitialTouch | SweepHitKind::Impact => into < -1e-7,
        SweepHitKind::Grazing => false,
    };
    blocking.then_some((hit.time as f32, hit.normal))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_hit<const D: usize>(hit: SweepHit<D>, fraction: f32, kind: SweepHitKind) {
        assert!((hit.fraction - fraction).abs() <= 1e-6, "{hit:?}");
        assert_eq!(hit.kind, kind);
        assert!(hit.normal.iter().all(|n| n.is_finite()), "{hit:?}");
        assert!((dot(hit.normal, hit.normal) - 1.).abs() < 1e-6, "{hit:?}");
    }

    #[test]
    fn stationary_sweeps_distinguish_separation_overlap_and_touch() {
        for shape in [Shape::ball(1.), Shape::cuboid([1.; 2])] {
            assert_eq!(shape.sweep_ball([0.; 2], [-3., 0.], 0.5, [0.; 2]), Ok(None));
            let overlap = shape
                .sweep_ball([0.; 2], [0.; 2], 0.5, [0.; 2])
                .unwrap()
                .unwrap();
            assert_hit(overlap, 0., SweepHitKind::InitialOverlap);
            assert_eq!(overlap.normal, [1., 0.]);
            let touch = shape
                .sweep_ball([0.; 2], [-1.5, 0.], 0.5, [0.; 2])
                .unwrap()
                .unwrap();
            assert_hit(touch, 0., SweepHitKind::InitialTouch);
            assert_eq!(touch.normal, [-1., 0.]);
        }
    }

    #[test]
    fn initial_contacts_take_precedence_for_inward_outward_and_tangent_motion() {
        for shape in [Shape::ball(1.), Shape::cuboid([1.; 3])] {
            for motion in [[4., 0., 0.], [-4., 0., 0.], [0., 4., 0.]] {
                let touch = shape
                    .sweep_ball([0.; 3], [-1.5, 0., 0.], 0.5, motion)
                    .unwrap()
                    .unwrap();
                assert_hit(touch, 0., SweepHitKind::InitialTouch);
                let overlap = shape
                    .sweep_ball([0.; 3], [-1.25, 0., 0.], 0.5, motion)
                    .unwrap()
                    .unwrap();
                assert_hit(overlap, 0., SweepHitKind::InitialOverlap);
            }
        }
    }

    #[test]
    fn checked_sweeps_include_endpoint_impacts_and_exclude_short_segments() {
        for shape in [Shape::ball(1.), Shape::cuboid([1.; 2])] {
            let hit = shape
                .sweep_ball([0.; 2], [-3., 0.], 0.5, [1.5, 0.])
                .unwrap()
                .unwrap();
            assert_hit(hit, 1., SweepHitKind::Impact);
            assert_eq!(hit.normal, [-1., 0.]);
            let short = f32::from_bits(1.5_f32.to_bits() - 1);
            assert_eq!(
                shape.sweep_ball([0.; 2], [-3., 0.], 0.5, [short, 0.]),
                Ok(None)
            );
        }
    }

    #[test]
    fn grazing_is_reported_without_turning_near_misses_into_hits() {
        for (shape, y, fraction) in [
            (Shape::ball(1.), 1.5, 0.75),
            (Shape::cuboid([1.; 2]), 1.5, 0.5),
        ] {
            let hit = shape
                .sweep_ball([0.; 2], [-3., y], 0.5, [4., 0.])
                .unwrap()
                .unwrap();
            assert_hit(hit, fraction, SweepHitKind::Grazing);
            assert_eq!(hit.normal, [0., 1.]);
            let beyond = f32::from_bits(y.to_bits() + 1);
            assert_eq!(
                shape.sweep_ball([0.; 2], [-3., beyond], 0.5, [4., 0.]),
                Ok(None)
            );
            let inside = f32::from_bits(y.to_bits() - 1);
            let hit = shape
                .sweep_ball([0.; 2], [-3., inside], 0.5, [4., 0.])
                .unwrap()
                .unwrap();
            assert_eq!(hit.kind, SweepHitKind::Impact);
            assert!(hit.normal[0] < 0.);
        }
    }

    #[test]
    fn three_dimensional_rounded_edge_and_corner_grazing() {
        let shape = Shape::cuboid([1.; 3]);
        for (origin, radius, expected_normal) in [
            ([-3., 1.5, 0.], 0.5, [0., 1., 0.]),
            ([-3., 1.375, 1.5], 0.625, [0., 0.6, 0.8]),
        ] {
            let hit = shape
                .sweep_ball([0.; 3], origin, radius, [4., 0., 0.])
                .unwrap()
                .unwrap();
            assert_hit(hit, 0.5, SweepHitKind::Grazing);
            assert_eq!(hit.normal, expected_normal);
        }
    }

    #[test]
    fn rounded_corner_is_not_an_expanded_box() {
        assert_eq!(
            Shape::cuboid([1.; 2]).sweep_ball([0.; 2], [-1.45, 1.45], 0.5, [0.1, 0.]),
            Ok(None)
        );
        let hit = Shape::cuboid([1.; 3])
            .sweep_ball([0.; 3], [-3., 1.2, 1.2], 0.5, [4., 0., 0.])
            .unwrap()
            .unwrap();
        assert_hit(hit, (2. - 0.17_f32.sqrt()) / 4., SweepHitKind::Impact);
        assert!((hit.normal[1] - 0.4).abs() < 1e-6);
        assert!((hit.normal[2] - 0.4).abs() < 1e-6);
    }

    #[test]
    fn distant_small_ball_has_no_spurious_tangency_or_lost_radius() {
        let (distance, normal) =
            ray_ball([-1_000_000., 0.], [1., 0.], [0.; 2], 0.5, 1_000_000.).unwrap();
        assert_eq!(distance, 999_999.5);
        assert_eq!(normal, [-1., 0.]);
        assert!(ray_ball([-1_000_000., 1.], [1., 0.], [0.; 2], 0.5, 1_000_000.).is_none());
        for shape in [Shape::ball(1e-5), Shape::cuboid([1e-5; 2])] {
            let hit = shape
                .sweep_ball([0.; 2], [-1_000_000., 0.], 1e-5, [1_000_000., 0.])
                .unwrap()
                .unwrap();
            assert_hit(hit, 1., SweepHitKind::Impact);
            assert_eq!(hit.normal, [-1., 0.]);
            assert_eq!(
                shape.sweep_ball([0.; 2], [-1_000_000., 0.001], 1e-5, [1_000_000., 0.]),
                Ok(None)
            );
        }
    }

    #[test]
    fn shallow_ray_and_sweep_components_are_not_treated_as_zero() {
        let (_, normal) = ray_box([-2., 0.], [1e-9, 0.], [0.; 2], [1.; 2], 2e9).unwrap();
        assert_eq!(normal, [-1., 0.]);
        // Both the tiny component and the total segment are within world limits.
        let hit = Shape::cuboid([1e-5, 1_000_000.])
            .sweep_ball([0.; 2], [-0.001, -500_000.], 1e-5, [0.002, 1_000_000.])
            .unwrap()
            .unwrap();
        assert_hit(hit, 0.49, SweepHitKind::Impact);
        assert_eq!(hit.normal, [-1., 0.]);
    }

    #[test]
    fn tiny_positive_gaps_are_not_rounded_into_initial_overlap() {
        let origin = f32::from_bits(1.5_f32.to_bits() + 1);
        for shape in [Shape::ball(1.), Shape::cuboid([1.; 2])] {
            let hit = shape
                .sweep_ball([0.; 2], [-origin, 0.], 0.5, [1., 0.])
                .unwrap()
                .unwrap();
            assert_hit(hit, origin - 1.5, SweepHitKind::Impact);
            assert!(hit.fraction > 0.);
        }
    }

    #[test]
    fn invalid_checked_inputs_report_errors_instead_of_misses() {
        let shape = Shape::cuboid([1.; 2]);
        for bad in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            -f32::MAX,
        ] {
            assert_eq!(
                shape.sweep_ball([bad, 0.], [0.; 2], 0.5, [0.; 2]),
                Err(PhysicsError::InvalidBody)
            );
            assert_eq!(
                shape.sweep_ball([0.; 2], [bad, 0.], 0.5, [0.; 2]),
                Err(PhysicsError::InvalidBody)
            );
            assert_eq!(
                shape.sweep_ball([0.; 2], [0.; 2], 0.5, [bad, 0.]),
                Err(PhysicsError::InvalidBody)
            );
            assert_eq!(
                shape.sweep_ball([0.; 2], [0.; 2], bad, [0.; 2]),
                Err(PhysicsError::InvalidBody)
            );
            assert_eq!(
                Shape::ball(bad).sweep_ball([0.; 2], [0.; 2], 0.5, [0.; 2]),
                Err(PhysicsError::InvalidBody)
            );
            assert_eq!(
                Shape::cuboid([bad, 1.]).sweep_ball([0.; 2], [0.; 2], 0.5, [0.; 2]),
                Err(PhysicsError::InvalidBody)
            );
        }
        for radius in [0., -1., 1e-6] {
            assert_eq!(
                shape.sweep_ball([0.; 2], [0.; 2], radius, [0.; 2]),
                Err(PhysicsError::InvalidBody)
            );
        }
        assert_eq!(
            Shape::<0>::ball(1.).sweep_ball([], [], 0.5, []),
            Err(PhysicsError::InvalidDimension)
        );
        assert_eq!(
            Shape::<4>::ball(1.).sweep_ball([0.; 4], [0.; 4], 0.5, [0.; 4]),
            Err(PhysicsError::InvalidDimension)
        );
    }

    #[test]
    fn unchecked_kernels_reject_nonfinite_input_and_invalid_dimensions() {
        assert!(ray_ball([f32::NAN, 0.], [1., 0.], [0.; 2], 1., 1.).is_none());
        assert!(ray_box([0.; 2], [f32::INFINITY, 0.], [0.; 2], [1.; 2], 1.).is_none());
        assert!(sweep_ball_box([0.; 2], 1., [1., 0.], [0.; 2], [-1.; 2], 1.).is_none());
        assert!(ray_ball::<0>([], [], [], 1., 1.).is_none());
        assert!(sweep_ball_box::<4>([0.; 4], 1., [0.; 4], [0.; 4], [1.; 4], 1.).is_none());
        for max in [-1., f32::NAN, f32::INFINITY] {
            assert!(ray_ball([0.; 2], [1., 0.], [0.; 2], 1., max).is_none());
            assert!(ray_box([0.; 2], [1., 0.], [0.; 2], [1.; 2], max).is_none());
            assert!(sweep_ball_box([0.; 2], 1., [1., 0.], [0.; 2], [1.; 2], max).is_none());
        }
    }

    #[test]
    fn finite_extreme_kernel_inputs_never_overflow_the_hit() {
        let largest = f32::MAX;
        let (distance, normal) =
            ray_ball([-largest, 0.], [1., 0.], [largest, 0.], largest, largest).unwrap();
        assert_eq!(distance, largest);
        assert_eq!(normal, [-1., 0.]);
        let (distance, normal) = ray_box(
            [-largest, 0.],
            [1., 0.],
            [largest, 0.],
            [largest; 2],
            largest,
        )
        .unwrap();
        assert_eq!(distance, largest);
        assert_eq!(normal, [-1., 0.]);
        let (distance, normal) = sweep_ball_box(
            [-largest, 0.],
            largest,
            [1., 0.],
            [largest, 0.],
            [largest; 2],
            largest,
        )
        .unwrap();
        assert_eq!(distance, 0.);
        assert_eq!(normal, [-1., 0.]);
    }

    #[test]
    fn legacy_solver_sweep_retains_sliding_and_overlap_policy() {
        for direction in [[-1., 0.], [0., 1.], [0.; 2]] {
            assert!(sweep_ball_box([-1.5, 0.], 0.5, direction, [0.; 2], [1.; 2], 3.).is_none());
            assert_eq!(
                sweep_ball_box([-1.25, 0.], 0.5, direction, [0.; 2], [1.; 2], 3.),
                Some((0., [-1., 0.]))
            );
        }
        assert_eq!(
            sweep_ball_box([-1.5, 0.], 0.5, [1., 0.], [0.; 2], [1.; 2], 0.),
            Some((0., [-1., 0.]))
        );
        assert!(sweep_ball_box([-3., 1.5], 0.5, [1., 0.], [0.; 2], [1.; 2], 4.).is_none());
        let shallow = -1.5 + 1e-6;
        assert!(sweep_ball_box([shallow, 0.], 0.5, [-1., 0.], [0.; 2], [1.; 2], 3.).is_none());
        assert_eq!(
            sweep_ball_box([shallow, 0.], 0.5, [1., 0.], [0.; 2], [1.; 2], 3.),
            Some((0., [-1., 0.]))
        );
    }

    #[test]
    fn discrete_contact_normal_and_penetration_conventions_are_unchanged() {
        let ball = Shape::ball(0.5);
        let cube = Shape::cuboid([1.; 2]);
        let a = contact(ball, [0.; 2], cube, [0.; 2], 0.).unwrap();
        let b = contact(cube, [0.; 2], ball, [0.; 2], 0.).unwrap();
        assert_eq!(a.normal, [-1., 0.]);
        assert_eq!(b.normal, [1., 0.]);
        assert_eq!(a.penetration, 1.5);
        assert_eq!(b.penetration, 1.5);
        assert!(contact(ball, [-1.6, 0.], cube, [0.; 2], 0.).is_none());
    }
}
