use super::{CameraError, bounded};
use ferravex_math::{Quat, Vec3};
pub const MAX_BLOCKERS: usize = 4096;
/// A sphere-center segment. Equal endpoints perform a stationary overlap check.
/// Use [`Sweep::new`] to check inputs, or [`Sweep::probe`] to also check adapter
/// results and obtain distances and a padded placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sweep {
    pub start: Vec3,
    pub end: Vec3,
    pub radius: f32,
}
impl Sweep {
    /// Construct a checked probe with finite endpoints in `[-1e6, 1e6]` on
    /// each axis and a finite radius in `0.001..=1000`.
    ///
    /// Invalid inputs return [`CameraError::InvalidQuery`]. Zero-length
    /// segments are valid and must still be sent to the collision query.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::camera::{CameraError, Sweep};
    /// let point = Sweep::new(Vec3::ZERO, Vec3::ZERO, 0.25)?;
    /// assert!(point.valid());
    /// assert_eq!(Sweep::new(Vec3::ZERO, Vec3::Z, 0.),
    ///            Err(CameraError::InvalidQuery));
    /// # Ok::<(), CameraError>(())
    /// ```
    pub fn new(start: Vec3, end: Vec3, radius: f32) -> Result<Self, CameraError> {
        let sweep = Self { start, end, radius };
        if sweep.valid() {
            Ok(sweep)
        } else {
            Err(CameraError::InvalidQuery)
        }
    }

    pub fn valid(self) -> bool {
        bounded(self.start)
            && bounded(self.end)
            && self.radius.is_finite()
            && (0.001..=1000.).contains(&self.radius)
    }

    /// Run exactly one query and report travel available before the first hit.
    ///
    /// `padding` is a finite distance in `0..=10`, matching camera obstruction
    /// settings. It retreats along the segment only when a hit is reported;
    /// it does not enlarge the probe or change a clear endpoint. Zero padding
    /// permits contact. A blocked start has no permitted placement: this
    /// method does not depenetrate or search for an escape route.
    ///
    /// Invalid sweep/padding or an invalid adapter fraction returns
    /// [`CameraError::InvalidQuery`]; adapter errors propagate unchanged.
    /// A stationary query still runs, and may return only `None` or `Some(0)`.
    /// Storage and query count are constant; slice adapters retain their
    /// [`MAX_BLOCKERS`] budget. Custom adapters determine geometric accuracy.
    ///
    /// ```
    /// use ferravex_math::Vec3;
    /// use ferravex_play::camera::{Blocker, CameraError, NoObstructions, Sweep};
    /// let sweep = Sweep::new(Vec3::ZERO, Vec3::Z * 10., 0.25)?;
    /// let wall = [Blocker::Plane { normal: -Vec3::Z, offset: -5. }];
    /// let hit = sweep.probe(&wall[..], 0.25)?;
    /// assert_eq!(hit.requested_distance, 10.);
    /// assert_eq!(hit.hit_fraction, Some(0.475));
    /// assert_eq!(hit.allowed_distance, 4.5);
    /// assert_eq!(hit.allowed_position, Some(Vec3::Z * 4.5));
    /// assert!(!hit.starts_blocked());
    /// let clear = sweep.probe(&NoObstructions, 0.25)?;
    /// assert_eq!(clear.hit_fraction, None);
    /// assert_eq!(clear.allowed_position, Some(sweep.end));
    /// # Ok::<(), CameraError>(())
    /// ```
    pub fn probe(
        self,
        query: &(impl CollisionQuery + ?Sized),
        padding: f32,
    ) -> Result<SweepOutcome, CameraError> {
        if !self.valid() || !padding.is_finite() || !(0. ..=10.).contains(&padding) {
            return Err(CameraError::InvalidQuery);
        }
        let hit_fraction = query.sweep(self)?;
        if hit_fraction.is_some_and(|t| {
            !t.is_finite() || !(0. ..=1.).contains(&t) || (self.start == self.end && t != 0.)
        }) {
            return Err(CameraError::InvalidQuery);
        }
        let requested_distance = self.start.distance(self.end);
        let allowed_distance = hit_fraction.map_or(requested_distance, |t| {
            (t * requested_distance - padding).max(0.)
        });
        let allowed_position = match hit_fraction {
            Some(0.) => None,
            None => Some(self.end),
            Some(_) if allowed_distance == 0. => Some(self.start),
            Some(_) => Some(
                self.start
                    .lerp(self.end, allowed_distance / requested_distance),
            ),
        };
        Ok(SweepOutcome {
            hit_fraction,
            requested_distance,
            allowed_distance,
            allowed_position,
        })
    }
}

/// Diagnostics from [`Sweep::probe`], describing only the queried segment.
/// Distances are sphere-center travel in world units, not surface separation.
/// No penetration depth, surface normal or blocker identity is inferred from
/// the fraction-only [`CollisionQuery`] abstraction.
///
/// ```
/// use ferravex_math::Vec3;
/// use ferravex_play::camera::{Blocker, CameraError, Sweep};
/// let blockers = [Blocker::Sphere { center: Vec3::ZERO, radius: 1. }];
/// let outcome = Sweep::new(Vec3::ZERO, Vec3::ZERO, 0.25)?
///     .probe(&blockers[..], 0.02)?;
/// assert!(outcome.starts_blocked());
/// assert_eq!(outcome.allowed_position, None);
/// # Ok::<(), CameraError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepOutcome {
    /// First impact in `[0, 1]`; `None` means the whole segment is clear.
    /// `Some(0)` includes initial contact as well as initial penetration.
    pub hit_fraction: Option<f32>,
    /// Length of the requested center segment, including zero-length queries.
    pub requested_distance: f32,
    /// Travel after subtracting padding from the hit distance and clamping to
    /// zero. A clear probe retains the full requested distance.
    pub allowed_distance: f32,
    /// Placement after padding, or `None` when the starting point is blocked.
    /// With zero padding this may touch an obstacle. It is not an escape point.
    pub allowed_position: Option<Vec3>,
}
impl SweepOutcome {
    /// Whether the adapter reported initial contact/overlap. Zero allowed
    /// distance alone does not imply this: padding can consume a clear gap.
    pub fn starts_blocked(self) -> bool {
        self.hit_fraction == Some(0.)
    }
}

/// Return the first sphere-center impact fraction in `[0,1]`, or None if clear.
/// A blocked starting point must return Some(0), including zero-length sweeps.
/// Stationary sweeps must return only None or Some(0). Use [`Sweep::probe`] for
/// checked results and padded clearance without changing this adapter contract.
pub trait CollisionQuery {
    fn sweep(&self, sweep: Sweep) -> Result<Option<f32>, CameraError>;
}
impl<F: Fn(Sweep) -> Result<Option<f32>, CameraError>> CollisionQuery for F {
    fn sweep(&self, sweep: Sweep) -> Result<Option<f32>, CameraError> {
        self(sweep)
    }
}
pub struct NoObstructions;
impl CollisionQuery for NoObstructions {
    fn sweep(&self, sweep: Sweep) -> Result<Option<f32>, CameraError> {
        if sweep.valid() {
            Ok(None)
        } else {
            Err(CameraError::InvalidQuery)
        }
    }
}
/// A small built-in adapter for authored environments. Box sweeps inflate the
/// local box by radius and are conservative at corners; physics adapters may
/// provide exact rounded-box/primitive sphere sweeps through CollisionQuery.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Blocker {
    Box {
        center: Vec3,
        half: Vec3,
        rotation: Quat,
    },
    Sphere {
        center: Vec3,
        radius: f32,
    },
    /// Solid half-space normal.dot(point) <= offset, with unit outward normal.
    Plane {
        normal: Vec3,
        offset: f32,
    },
}
impl Blocker {
    pub fn valid(self) -> bool {
        match self {
            Self::Box {
                center,
                half,
                rotation,
            } => {
                bounded(center)
                    && half.is_finite()
                    && half.min_element() > 0.
                    && half.max_element() <= 1e5
                    && rotation.is_finite()
                    && (rotation.length_squared() - 1.).abs() < 0.001
            }
            Self::Sphere { center, radius } => {
                bounded(center) && radius.is_finite() && (0.001..=1e5).contains(&radius)
            }
            Self::Plane { normal, offset } => {
                normal.is_finite()
                    && (normal.length_squared() - 1.).abs() < 0.001
                    && offset.is_finite()
                    && offset.abs() <= 1e6
            }
        }
    }
    fn fraction(self, sweep: Sweep) -> Option<f32> {
        match self {
            Self::Box {
                center,
                half,
                rotation,
            } => {
                let start = rotation.conjugate() * (sweep.start - center);
                let end = rotation.conjugate() * (sweep.end - center);
                let delta = end - start;
                let half = half + Vec3::splat(sweep.radius);
                let mut enter = 0_f32;
                let mut exit = 1_f32;
                for i in 0..3 {
                    if delta[i].abs() < 1e-10 {
                        if start[i].abs() > half[i] {
                            return None;
                        }
                    } else {
                        let a = (-half[i] - start[i]) / delta[i];
                        let b = (half[i] - start[i]) / delta[i];
                        enter = enter.max(a.min(b));
                        exit = exit.min(a.max(b));
                        if enter > exit {
                            return None;
                        }
                    }
                }
                Some(enter)
            }
            Self::Sphere { center, radius } => {
                // Widen before subtraction and dot products: an f32 squared
                // distance can round away a real gap in a stationary probe.
                let relative: [f64; 3] =
                    std::array::from_fn(|i| f64::from(sweep.start[i]) - f64::from(center[i]));
                let delta: [f64; 3] =
                    std::array::from_fn(|i| f64::from(sweep.end[i]) - f64::from(sweep.start[i]));
                let r = f64::from(radius) + f64::from(sweep.radius);
                let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
                let c = dot(relative, relative) - r * r;
                if c <= 0. {
                    return Some(0.);
                }
                let a = dot(delta, delta);
                let b = dot(relative, delta);
                if a == 0. || b >= 0. {
                    return None;
                }
                // Use perpendicular distance to the line, avoiding cancellation
                // in b*b - a*c on long segments passing small spheres.
                let closest = std::array::from_fn(|i| relative[i] - delta[i] * (b / a));
                let chord_squared = r * r - dot(closest, closest);
                if chord_squared < 0. {
                    return None;
                }
                // Equivalent to (-b - sqrt(discriminant)) / a, but does not
                // subtract almost equal numbers when starting close to contact.
                let t = c / (-b + (a * chord_squared).sqrt());
                (0. ..=1.).contains(&t).then_some(t as f32)
            }
            Self::Plane { normal, offset } => {
                let start = normal.dot(sweep.start) - offset - sweep.radius;
                if start <= 0. {
                    return Some(0.);
                }
                let end = normal.dot(sweep.end) - offset - sweep.radius;
                if end > 0. {
                    None
                } else {
                    Some(start / (start - end))
                }
            }
        }
    }
}
impl CollisionQuery for [Blocker] {
    fn sweep(&self, sweep: Sweep) -> Result<Option<f32>, CameraError> {
        if !sweep.valid() {
            return Err(CameraError::InvalidQuery);
        }
        if self.len() > MAX_BLOCKERS {
            return Err(CameraError::QueryBudget);
        }
        let mut first = None;
        for &blocker in self {
            if !blocker.valid() {
                return Err(CameraError::InvalidObstacle);
            }
            if let Some(t) = blocker.fraction(sweep) {
                first = Some(first.map_or(t, |old: f32| old.min(t)));
            }
        }
        Ok(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn checked_sweep_enforces_existing_bounds_and_accepts_stationary_queries() {
        for radius in [0.001, 1000.] {
            assert!(Sweep::new(Vec3::splat(-1e6), Vec3::splat(1e6), radius).is_ok());
            assert!(Sweep::new(Vec3::ZERO, Vec3::ZERO, radius).is_ok());
        }
        for radius in [0., -1., 0.0009, 1000.1, f32::NAN, f32::INFINITY] {
            assert_eq!(
                Sweep::new(Vec3::ZERO, Vec3::X, radius),
                Err(CameraError::InvalidQuery)
            );
        }
        for position in [Vec3::splat(1_000_001.), Vec3::NAN, Vec3::NEG_INFINITY] {
            assert_eq!(
                Sweep::new(position, Vec3::ZERO, 0.25),
                Err(CameraError::InvalidQuery)
            );
            assert_eq!(
                Sweep::new(Vec3::ZERO, position, 0.25),
                Err(CameraError::InvalidQuery)
            );
        }
    }

    #[test]
    fn invalid_probe_configuration_is_rejected_before_querying() {
        let query = |_: Sweep| -> Result<Option<f32>, CameraError> {
            panic!("invalid configuration must not reach the adapter")
        };
        let sweep = Sweep::new(Vec3::ZERO, Vec3::X, 0.25).unwrap();
        for padding in [-0.001, 10.001, f32::NAN, f32::INFINITY] {
            assert_eq!(sweep.probe(&query, padding), Err(CameraError::InvalidQuery));
        }
        for invalid in [
            Sweep {
                radius: 0.,
                ..sweep
            },
            Sweep {
                start: Vec3::NAN,
                ..sweep
            },
            Sweep {
                end: Vec3::INFINITY,
                ..sweep
            },
        ] {
            assert_eq!(invalid.probe(&query, 0.), Err(CameraError::InvalidQuery));
        }
    }

    #[test]
    fn clear_probes_keep_the_exact_endpoint_and_query_even_when_stationary() {
        for (start, end) in [
            (Vec3::ZERO, Vec3::Z * 10.),
            (Vec3::X, Vec3::X),
            (Vec3::splat(-1e6), Vec3::splat(1e6)),
        ] {
            let sweep = Sweep::new(start, end, 0.25).unwrap();
            let calls = Cell::new(0);
            let adapter = |observed| {
                assert_eq!(observed, sweep);
                calls.set(calls.get() + 1);
                Ok(None)
            };
            let query: &dyn CollisionQuery = &adapter;
            let result = sweep.probe(query, 10.).unwrap();
            assert_eq!(calls.get(), 1);
            assert_eq!(result.hit_fraction, None);
            assert_eq!(result.requested_distance, start.distance(end));
            assert_eq!(result.allowed_distance, result.requested_distance);
            assert_eq!(result.allowed_position, Some(end));
            assert!(!result.starts_blocked());
        }
    }

    #[test]
    fn padding_consuming_the_clearance_does_not_mean_the_start_is_blocked() {
        let sweep = Sweep::new(Vec3::ZERO, Vec3::X * 10., 0.25).unwrap();
        let blockers = [Blocker::Sphere {
            center: Vec3::X * 5.,
            radius: 1.,
        }];
        for (padding, expected_distance) in [(0., 3.75), (0.25, 3.5), (10., 0.)] {
            let result = sweep.probe(&blockers[..], padding).unwrap();
            assert_eq!(result.hit_fraction, Some(0.375));
            assert_eq!(result.requested_distance, 10.);
            assert_eq!(result.allowed_distance, expected_distance);
            assert_eq!(result.allowed_position, Some(Vec3::X * expected_distance));
            assert!(!result.starts_blocked());
        }
        let result = sweep.probe(&|_| Ok(Some(0.)), 0.25).unwrap();
        assert!(result.starts_blocked());
        assert_eq!(result.allowed_distance, 0.);
        assert_eq!(result.allowed_position, None);
    }

    #[test]
    fn every_shape_reports_initial_contact_and_stationary_overlap() {
        for blocker in [
            Blocker::Sphere {
                center: Vec3::ZERO,
                radius: 1.,
            },
            Blocker::Box {
                center: Vec3::ZERO,
                half: Vec3::ONE,
                rotation: Quat::IDENTITY,
            },
            Blocker::Plane {
                normal: Vec3::X,
                offset: 1.,
            },
        ] {
            let blockers = [blocker];
            for start in [Vec3::ZERO, Vec3::X * 1.25] {
                // Even motion out of an obstacle reports the blocked start.
                for end in [start, Vec3::X * 3.] {
                    let result = Sweep::new(start, end, 0.25)
                        .unwrap()
                        .probe(&blockers[..], 0.02)
                        .unwrap();
                    assert_eq!(result.hit_fraction, Some(0.), "{blocker:?}");
                    assert!(result.starts_blocked());
                    assert_eq!(result.allowed_distance, 0.);
                    assert_eq!(result.allowed_position, None);
                }
            }
            let outside = Vec3::X * 3.;
            let result = Sweep::new(outside, outside, 0.25)
                .unwrap()
                .probe(&blockers[..], 0.02)
                .unwrap();
            assert_eq!(result.hit_fraction, None, "{blocker:?}");
            assert_eq!(result.requested_distance, 0.);
            assert_eq!(result.allowed_position, Some(outside));
        }
    }

    #[test]
    fn malformed_adapter_results_are_rejected_and_errors_propagate() {
        let sweep = Sweep::new(Vec3::ZERO, Vec3::X, 0.25).unwrap();
        for fraction in [-0.001, 1.001, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                sweep.probe(&|_| Ok(Some(fraction)), 0.),
                Err(CameraError::InvalidQuery)
            );
        }
        let point = Sweep::new(Vec3::ZERO, Vec3::ZERO, 0.25).unwrap();
        for fraction in [f32::EPSILON, 0.5, 1.] {
            assert_eq!(
                point.probe(&|_| Ok(Some(fraction)), 0.),
                Err(CameraError::InvalidQuery)
            );
        }
        assert!(
            point
                .probe(&|_| Ok(Some(-0.)), 0.)
                .unwrap()
                .starts_blocked()
        );
        for error in [CameraError::InvalidObstacle, CameraError::QueryBudget] {
            assert_eq!(sweep.probe(&|_| Err(error), 0.), Err(error));
            assert_eq!(point.probe(&|_| Err(error), 0.), Err(error));
        }
    }

    #[test]
    fn probe_preserves_blocker_budget_and_validates_even_after_an_initial_hit() {
        let sweep = Sweep::new(Vec3::ZERO, Vec3::ZERO, 0.25).unwrap();
        let sphere = Blocker::Sphere {
            center: Vec3::ZERO,
            radius: 1.,
        };
        let mut blockers = vec![sphere; MAX_BLOCKERS];
        assert!(sweep.probe(&blockers[..], 0.).unwrap().starts_blocked());
        blockers.push(sphere);
        assert_eq!(
            sweep.probe(&blockers[..], 0.),
            Err(CameraError::QueryBudget)
        );
        blockers.truncate(2);
        blockers[1] = Blocker::Sphere {
            center: Vec3::ZERO,
            radius: f32::NAN,
        };
        assert_eq!(
            sweep.probe(&blockers[..], 0.),
            Err(CameraError::InvalidObstacle)
        );
        assert_eq!(sweep.probe(&[][..], 0.).unwrap().hit_fraction, None);
    }

    #[test]
    fn stationary_sphere_probe_preserves_a_near_tangent_gap() {
        let blockers = [Blocker::Sphere {
            center: Vec3::ZERO,
            radius: 1.,
        }];
        let position = Vec3::new(1.25, 0.0001, 0.);
        assert_eq!(
            blockers[..].sweep(Sweep {
                start: position,
                end: position,
                radius: 0.25,
            }),
            Ok(None)
        );
    }

    #[test]
    fn long_sphere_sweep_preserves_a_clear_miss() {
        let blockers = [Blocker::Sphere {
            center: Vec3::new(500_000., 2., 0.),
            radius: 0.25,
        }];
        assert_eq!(
            blockers[..].sweep(Sweep {
                start: Vec3::ZERO,
                end: Vec3::X * 1_000_000.,
                radius: 0.25,
            }),
            Ok(None)
        );
    }

    #[test]
    fn sphere_entry_at_near_contact_tangency_and_endpoints_is_retained() {
        let blockers = [Blocker::Sphere {
            center: Vec3::ZERO,
            radius: 1.,
        }];
        for (start, end, expected) in [
            (Vec3::X * 2., Vec3::ZERO, Some(0.375)),
            (Vec3::X * 2., Vec3::X * 1.25, Some(1.)),
            (Vec3::new(-2., 1.25, 0.), Vec3::new(2., 1.25, 0.), Some(0.5)),
            (Vec3::X * 2., Vec3::X * 3., None),
            (Vec3::X * 2., Vec3::X * 1.5, None),
        ] {
            assert_eq!(
                blockers[..].sweep(Sweep::new(start, end, 0.25).unwrap()),
                Ok(expected)
            );
        }
        let start = Vec3::X * f32::from_bits(1.25_f32.to_bits() + 1);
        let sweep = Sweep::new(start, Vec3::ZERO, 0.25).unwrap();
        let result = sweep.probe(&blockers[..], 0.).unwrap();
        assert!(!result.starts_blocked());
        assert!(result.hit_fraction.unwrap() > 0.);
        assert!(result.allowed_distance > 0.);
        assert_eq!(result.allowed_position, Some(Vec3::X * 1.25));
    }

    #[test]
    fn sphere_probe_retains_small_radii_on_long_paths() {
        for y in [0., 0.004] {
            let blockers = [Blocker::Sphere {
                center: Vec3::new(500_000., y, 0.),
                radius: 0.001,
            }];
            let sweep = Sweep::new(Vec3::ZERO, Vec3::X * 1_000_000., 0.001).unwrap();
            let fraction = blockers[..].sweep(sweep).unwrap();
            if y == 0. {
                assert!((fraction.unwrap() - 0.5).abs() < f32::EPSILON);
            } else {
                assert_eq!(fraction, None);
            }
        }
    }
}
