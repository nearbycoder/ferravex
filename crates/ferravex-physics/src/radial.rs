//! Allocation-free radial impulses selected by the existing direct-overlap query.
//!
//! Selection uses current collider surfaces, including compound child offsets,
//! rather than body-origin distance. Each dynamic body receives at most one
//! **linear COM impulse** (no torque). Direction is from the request center to
//! the body's current COM; falloff uses the nearest accepted child surface.
//! Fixed/kinematic bodies are excluded. Sleeping behavior is explicit.
//!
//! All ordinary failures are checked before changing velocities, wake flags,
//! forces or the accepted-output buffer. Candidate scratch may change on error.
//! A truncated query is rejected, never applied as a first-N blast. An additional
//! numeric preflight rejects geometry the existing f32 query would silently omit.
//! No clocks, detector, damage, feedback, replay state or persistent cache is added.
//!
//! A physics-lab pulse can replace a manual body loop at an actual fixed tick:
//!
//! ```
//! # #[cfg(feature = "dim2")] {
//! use ferravex_math::{Vec2, Vec3};
//! use ferravex_physics::{d2::*, radial::*};
//! let mut world = PhysicsWorld::default();
//! let ball = world.add_ball([2., 0.], 0.5, true);
//! let wall = world.add_ball([0., -2.], 0.5, false);
//! let mut candidates = [None; 8]; // Reuse these caller buffers on later ticks.
//! let mut accepted = [None; 4];
//! let request = RadialImpulseRequest {
//!     center: Vec3::ZERO, radius: 8.,
//!     model: ImpulseModel::PerBody(11.), falloff: Falloff::LinearSurface,
//!     zero_distance: ZeroDistance::Axis(Vec3::Y),
//!     sleeping: SleepingPolicy::Wake, sensors: RadialSensors::Exclude,
//!     layers: u32::MAX, ignored_body: None, max_bodies: 4, max_colliders: 8,
//! };
//! let report = apply_radial_impulse(&mut world, request, &mut candidates,
//!     &mut accepted).unwrap();
//! assert_eq!(report.applied, 1);
//! let effect = accepted[0].unwrap();
//! assert_eq!(effect.body, ball);
//! assert!((world.bodies[ball].linvel() - Vec2::X * effect.impulse.x
//!     / world.bodies[ball].mass()).length() < 1e-4);
//! assert_eq!(world.bodies[wall].linvel(), Vec2::ZERO);
//! world.step(1. / 120.); // Real physics progresses; the operation itself runs no tick.
//! assert!(world.bodies[ball].translation().x > 2.);
//! // Restart owns world/body restoration; this helper retains no pending blast.
//! assert!(world.remove_body(ball));
//! let replacement = world.add_ball([2., 0.], 0.5, true);
//! let report = apply_radial_impulse(&mut world, request, &mut candidates,
//!     &mut accepted).unwrap();
//! assert_eq!(report.applied, 1);
//! assert_eq!(accepted[0].unwrap().body, replacement);
//! assert_ne!(replacement, ball);
//! # }
//! ```
//!
//! The same generic adapter accepts the spatial world without a new renderer:
//!
//! ```
//! # #[cfg(feature = "dim3")] {
//! use ferravex_math::Vec3;
//! use ferravex_physics::{d3::*, radial::*};
//! let mut world = PhysicsWorld::default();
//! let body = world.add_ball([0., 0., 2.], 0.5, true);
//! let request = RadialImpulseRequest {
//!     center: Vec3::ZERO, radius: 4., model: ImpulseModel::Total(6.),
//!     falloff: Falloff::Constant, zero_distance: ZeroDistance::Skip,
//!     sleeping: SleepingPolicy::Skip, sensors: RadialSensors::Exclude,
//!     layers: u32::MAX, ignored_body: None, max_bodies: 1, max_colliders: 1,
//! };
//! let mut candidates = [None; 1];
//! let mut accepted = [None; 1];
//! assert_eq!(apply_radial_impulse(&mut world, request, &mut candidates,
//!     &mut accepted).unwrap().applied, 1);
//! assert_eq!(accepted[0].unwrap().impulse, Vec3::Z * 6.);
//! assert!((world.bodies[body].linvel() - Vec3::Z * 6.
//!     / world.bodies[body].mass()).length() < 1e-4);
//! world.step(1. / 120.);
//! assert!(world.bodies[body].translation().z > 2.);
//! # }
//! ```

use crate::{
    ColliderHandle, RigidBodyHandle,
    body::{Access, Kind},
    world::World,
};
use ferravex_math::Vec3;

/// Scalar impulse policy; magnitudes must be finite and nonnegative.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImpulseModel {
    /// Each body receives `magnitude * falloff_weight` impulse units.
    PerBody(f32),
    /// Split this total scalar impulse budget in proportion to falloff weights.
    /// This bounds the sum of magnitudes, not the vector sum or kinetic energy.
    /// With no eligible nonzero weights, no impulse is spent. f32 rounding applies.
    Total(f32),
}

/// Falloff uses signed distance to the nearest child accepted by the query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Falloff {
    Constant,
    /// Weight = clamp(1 - max(surface_distance, 0) / radius, 0, 1).
    /// A zero-radius point probe gives weight one to every overlapping child.
    LinearSurface,
}

/// Direction when the body's COM coincides exactly with the request center.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZeroDistance {
    /// Exclude this body from the budget, bound and accepted output.
    Skip,
    /// Normalize this finite nonzero vector using f64 intermediates.
    /// In a planar world the vector must have z == 0, even if no body is centered.
    Axis(Vec3),
}

/// Only dynamic bodies can be affected; sleeping dynamics follow this policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SleepingPolicy {
    Skip,
    /// Store the velocity change without changing sleep/quiet/wake flags.
    Preserve,
    /// Apply the impulse and request ordinary body/island waking.
    Wake,
}

/// Identical sensor membership rules to the dimensional overlap queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadialSensors {
    Exclude,
    Include,
    Only,
}

/// Explicit blast policy. See the module's runnable fixed-tick lab recipes.
/// No default model, falloff or zero-distance axis is chosen for the caller.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadialImpulseRequest {
    /// World units. Planar requests require z == 0.
    pub center: Vec3,
    /// Existing sphere-query radius range: finite 0..=1000.
    pub radius: f32,
    pub model: ImpulseModel,
    pub falloff: Falloff,
    pub zero_distance: ZeroDistance,
    pub sleeping: SleepingPolicy,
    pub sensors: RadialSensors,
    /// Collider membership mask, independent of collision response bits.
    pub layers: u32,
    /// Exclude all children of this exact generational body handle.
    pub ignored_body: Option<RigidBodyHandle>,
    /// Maximum accepted unique bodies. Output must hold this requested capacity.
    pub max_bodies: usize,
    /// Maximum live colliders in the world, including filtered/fixed colliders.
    /// Reject before query work when exceeded. See [`apply_radial_impulse`].
    pub max_colliders: usize,
}

/// Exact accepted impulse; zero/underflow impulses and skipped bodies are absent.
/// Output order is ascending generational body handle, independent of child order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadialImpulse {
    pub body: RigidBodyHandle,
    /// Nearest accepted child, with ascending collider handle breaking ties.
    pub collider: ColliderHandle,
    /// Signed sphere-probe separation from the existing overlap query.
    pub separation: f32,
    pub impulse: Vec3,
}

/// Counts for one successful invocation; no counters persist between invocations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[must_use]
pub struct RadialImpulseReport {
    /// Live colliders admitted by the work bound (including filtered colliders).
    pub colliders: usize,
    /// Complete accepted query children, including non-dynamic/skipped bodies.
    pub candidates: usize,
    pub applied: usize,
}

/// Atomic rejection: world state and accepted output remain unchanged.
/// Candidate scratch is unspecified after any error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RadialImpulseError {
    InvalidCenter,
    InvalidRadius,
    InvalidMagnitude,
    InvalidZeroAxis,
    /// Output length is smaller than the requested max-body capacity.
    OutputCapacity {
        required: usize,
    },
    /// World live-collider count exceeds the caller's work limit.
    WorkLimit {
        required: usize,
    },
    /// Complete query candidates did not fit. No first-N effects are applied.
    CandidateCapacity {
        required: usize,
    },
    BodyLimit {
        required: usize,
    },
    /// A query distance/witness or COM cannot be represented reliably as finite.
    IncompleteGeometry {
        collider: ColliderHandle,
    },
    /// An admitted body has pending mass edits; refresh explicitly before retry.
    /// This helper never refreshes mass as part of a supposedly atomic rejection.
    PendingMass {
        body: RigidBodyHandle,
    },
    /// Impulse, its f32 mass product or resulting velocity would be nonfinite.
    NonFiniteEffect {
        body: RigidBodyHandle,
    },
}
impl std::fmt::Display for RadialImpulseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "radial impulse: {self:?}")
    }
}
impl std::error::Error for RadialImpulseError {}

// Sealing prevents user callbacks/forged hits from claiming query completeness.
mod sealed {
    use super::*;
    pub trait Candidate: Copy {
        fn parts(self) -> (RigidBodyHandle, ColliderHandle, f32);
    }
    pub trait Body: Access {
        fn apply(&mut self, impulse: Vec3, wake: bool);
    }
    pub trait Adapter {
        type Body: Body;
        type Candidate: Candidate;
        fn raw(&self) -> &World<Self::Body>;
        fn raw_mut(&mut self) -> &mut World<Self::Body>;
        fn planar(&self) -> bool;
        fn query(
            &self,
            request: RadialImpulseRequest,
            scratch: &mut [Option<Self::Candidate>],
        ) -> (usize, usize);
    }
}

/// Sealed adapter for the existing d2/d3 `PhysicsWorld`s. See the module recipes.
/// Generic consumers can accept `W: RadialWorld`; no caller-provided detector or
/// callback runs between preflight and commit. The shared kernel uses `World<B>`.
///
/// ```
/// use ferravex_physics::radial::*;
/// fn tick_pulse<W: RadialWorld>(world: &mut W, request: RadialImpulseRequest,
///     scratch: &mut [Option<W::Candidate>], accepted: &mut [Option<RadialImpulse>])
///     -> Result<RadialImpulseReport, RadialImpulseError>
/// {
///     apply_radial_impulse(world, request, scratch, accepted)
/// }
/// ```
pub trait RadialWorld: sealed::Adapter {}

#[cfg(any(feature = "dim2", feature = "dim3"))]
macro_rules! dimension_adapter {
    ($module:ident, $planar:expr, $array:expr, $vector:expr) => {
        impl sealed::Body for crate::$module::RigidBody {
            fn apply(&mut self, impulse: Vec3, wake: bool) {
                self.apply_impulse(($vector)(impulse), wake);
            }
        }
        impl sealed::Candidate for crate::$module::OverlapHit {
            fn parts(self) -> (RigidBodyHandle, ColliderHandle, f32) {
                (
                    self.body.expect("overlap queries return attached children"),
                    self.collider,
                    self.separation,
                )
            }
        }
        impl sealed::Adapter for crate::$module::PhysicsWorld {
            type Body = crate::$module::RigidBody;
            type Candidate = crate::$module::OverlapHit;
            fn raw(&self) -> &World<Self::Body> {
                &self.raw
            }
            fn raw_mut(&mut self) -> &mut World<Self::Body> {
                &mut self.raw
            }
            fn planar(&self) -> bool {
                $planar
            }
            fn query(
                &self,
                request: RadialImpulseRequest,
                scratch: &mut [Option<Self::Candidate>],
            ) -> (usize, usize) {
                use crate::$module::{QueryOptions, SensorPolicy};
                let options = QueryOptions {
                    max_distance: 0.,
                    layers: request.layers,
                    ignored_body: request.ignored_body,
                    sensors: match request.sensors {
                        RadialSensors::Exclude => SensorPolicy::Exclude,
                        RadialSensors::Include => SensorPolicy::Include,
                        RadialSensors::Only => SensorPolicy::Only,
                    },
                };
                let result = self
                    .overlap_sphere_into(($array)(request.center), request.radius, options, scratch)
                    .expect("radial request validated query inputs");
                (result.written, result.total)
            }
        }
        impl RadialWorld for crate::$module::PhysicsWorld {}
    };
}
#[cfg(feature = "dim2")]
dimension_adapter!(d2, true, |v: Vec3| [v.x, v.y], |v: Vec3| v.truncate());
#[cfg(feature = "dim3")]
dimension_adapter!(d3, false, |v: Vec3| v.to_array(), |v: Vec3| v);

use sealed::{Body, Candidate};

/// Check the complete blast, then apply existing body COM-impulse methods once
/// per accepted body. See the module for runnable recipes in both dimensions.
///
/// `candidates` is dimensional `Option<OverlapHit>` scratch. Every query hit must
/// fit, including fixed/kinematic/sensor children; the caller can size it to the
/// world's collider count. `output.len() >= request.max_bodies` is required even
/// for a miss. Every Err preserves all accepted output entries. Success writes
/// exactly `report.applied` entries and clears the entire remaining output.
///
/// Work is bounded by the requested live-collider limit, the existing bounded
/// arenas and caller slice lengths: one numeric guard pass, one existing overlap
/// pass (sorted insertion at most O(C * min(C, scratch.len()))), in-place unstable
/// O(H log H) canonical sorting, and bounded body/child passes. There is no heap
/// allocation. This is a work-admission bound, not a time/throughput guarantee.
///
/// Falloff is derived from task220's signed separation, never a body-origin
/// radius test. Nonzero impulses that round to zero are omitted. Total-budget
/// distribution is not renormalized after f32 underflow. All f32 multiply/add
/// intermediates used by the existing impulse method are checked, not just the
/// wider mathematical result. Pending child mass edits reject without refreshing.
/// Forces, torque, angular velocity and kinematic targets are never modified.
/// No bitwise cross-platform determinism or concurrent-world safety is claimed.
pub fn apply_radial_impulse<W: RadialWorld>(
    world: &mut W,
    request: RadialImpulseRequest,
    candidates: &mut [Option<W::Candidate>],
    output: &mut [Option<RadialImpulse>],
) -> Result<RadialImpulseReport, RadialImpulseError> {
    let axis = validate_request(request, world.planar(), output.len())?;
    let colliders = world.raw().colliders.len();
    if colliders > request.max_colliders {
        return Err(RadialImpulseError::WorkLimit {
            required: colliders,
        });
    }
    world.raw().radial_check_geometry(request)?;
    let (written, total) = world.query(request, candidates);
    if written != total {
        return Err(RadialImpulseError::CandidateCapacity { required: total });
    }
    let hits = &mut candidates[..written];
    hits.sort_unstable_by(|a, b| {
        let (ab, ac, ad) = a.unwrap().parts();
        let (bb, bc, bd) = b.unwrap().parts();
        ab.cmp(&bb)
            .then_with(|| ad.total_cmp(&bd))
            .then(ac.cmp(&bc))
    });
    // Select exactly one (nearest, then lowest collider) candidate per body.
    let selected = || {
        unique_hits(hits).filter_map(|hit| {
            let (body, collider, separation) = hit.parts();
            let core = world.raw().bodies[body].core();
            if core.kind != Kind::Dynamic
                || (core.sleeping && request.sleeping == SleepingPolicy::Skip)
            {
                return None;
            }
            let weight = weight(request, separation);
            let direction = direction(core.world_com(), request.center).or(axis);
            (weight > 0.).then_some((body, collider, separation, weight, direction?))
        })
    };
    let weight_sum = selected().map(|v| v.3).sum::<f64>();
    let mut applied = 0;
    for (body, _, _, weight, direction) in selected() {
        let impulse = impulse(request.model, weight, weight_sum, direction);
        world.raw().radial_check_effect(body, impulse)?;
        if impulse != Vec3::ZERO {
            applied += 1;
        }
    }
    if applied > request.max_bodies {
        return Err(RadialImpulseError::BodyLimit { required: applied });
    }
    // No fallible callbacks or world edits intervene. Recomputing the checked
    // scalar effects avoids using/changing caller output as temporary storage.
    output.fill(None);
    let mut count = 0;
    for hit in unique_hits(hits) {
        let (body, collider, separation) = hit.parts();
        let core = world.raw().bodies[body].core();
        if core.kind != Kind::Dynamic || (core.sleeping && request.sleeping == SleepingPolicy::Skip)
        {
            continue;
        }
        let Some(direction) = direction(core.world_com(), request.center).or(axis) else {
            continue;
        };
        let impulse = impulse(
            request.model,
            weight(request, separation),
            weight_sum,
            direction,
        );
        if impulse == Vec3::ZERO {
            continue;
        }
        world
            .raw_mut()
            .bodies
            .get_mut(body)
            .unwrap()
            .apply(impulse, request.sleeping == SleepingPolicy::Wake);
        output[count] = Some(RadialImpulse {
            body,
            collider,
            separation,
            impulse,
        });
        count += 1;
    }
    Ok(RadialImpulseReport {
        colliders,
        candidates: total,
        applied: count,
    })
}

fn unique_hits<H: Candidate>(hits: &[Option<H>]) -> impl Iterator<Item = H> + '_ {
    let mut previous = None;
    hits.iter().filter_map(move |hit| {
        let hit = hit.unwrap();
        let body = hit.parts().0;
        if previous == Some(body) {
            None
        } else {
            previous = Some(body);
            Some(hit)
        }
    })
}

fn validate_request(
    request: RadialImpulseRequest,
    planar: bool,
    capacity: usize,
) -> Result<Option<Vec3>, RadialImpulseError> {
    use RadialImpulseError as E;
    if !request.center.is_finite() || (planar && request.center.z != 0.) {
        return Err(E::InvalidCenter);
    }
    if !request.radius.is_finite() || !(0. ..=1000.).contains(&request.radius) {
        return Err(E::InvalidRadius);
    }
    let (ImpulseModel::Total(magnitude) | ImpulseModel::PerBody(magnitude)) = request.model;
    if !magnitude.is_finite() || magnitude < 0. {
        return Err(E::InvalidMagnitude);
    }
    let axis = match request.zero_distance {
        ZeroDistance::Skip => None,
        ZeroDistance::Axis(v) => {
            if !v.is_finite() || (planar && v.z != 0.) {
                return Err(E::InvalidZeroAxis);
            }
            Some(direction(v, Vec3::ZERO).ok_or(E::InvalidZeroAxis)?)
        }
    };
    if capacity < request.max_bodies {
        return Err(E::OutputCapacity {
            required: request.max_bodies,
        });
    }
    Ok(axis)
}

// Wide subtraction/normalization admits tiny axes and opposite finite extremes.
fn direction(point: Vec3, center: Vec3) -> Option<Vec3> {
    let delta = [
        f64::from(point.x) - f64::from(center.x),
        f64::from(point.y) - f64::from(center.y),
        f64::from(point.z) - f64::from(center.z),
    ];
    let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
    (length.is_finite() && length > 0.).then(|| {
        Vec3::new(
            (delta[0] / length) as f32,
            (delta[1] / length) as f32,
            (delta[2] / length) as f32,
        )
    })
}
fn weight(request: RadialImpulseRequest, separation: f32) -> f64 {
    match request.falloff {
        Falloff::Constant => 1.,
        Falloff::LinearSurface if request.radius == 0. => 1.,
        Falloff::LinearSurface => {
            (-f64::from(separation) / f64::from(request.radius)).clamp(0., 1.)
        }
    }
}
fn impulse(model: ImpulseModel, weight: f64, sum: f64, direction: Vec3) -> Vec3 {
    let magnitude = match model {
        ImpulseModel::PerBody(m) => f64::from(m) * weight,
        ImpulseModel::Total(m) if sum > 0. => f64::from(m) * (weight / sum),
        ImpulseModel::Total(_) => 0.,
    };
    Vec3::new(
        (f64::from(direction.x) * magnitude) as f32,
        (f64::from(direction.y) * magnitude) as f32,
        (f64::from(direction.z) * magnitude) as f32,
    )
}

impl<B: Access> World<B> {
    // Preserve task220 completeness despite its documented finite-result omission.
    // This is a numeric guard using the same kernel, not a replacement selection.
    fn radial_check_geometry(
        &self,
        request: RadialImpulseRequest,
    ) -> Result<(), RadialImpulseError> {
        for (id, collider) in self.colliders.iter() {
            let sensor = match request.sensors {
                RadialSensors::Exclude => !collider.sensor,
                RadialSensors::Include => true,
                RadialSensors::Only => collider.sensor,
            };
            if !sensor
                || collider.membership & request.layers == 0
                || collider.parent == request.ignored_body
            {
                continue;
            }
            let body = collider
                .parent
                .and_then(|h| self.bodies.get(h))
                .ok_or(RadialImpulseError::IncompleteGeometry { collider: id })?;
            let core = body.core();
            let pose = collider.pose(core);
            let (distance, normal) = crate::geometry::distance(
                collider.shape,
                pose.position,
                pose.rotation,
                request.center,
                core.planar,
            );
            if !pose.position.is_finite()
                || !pose.rotation.is_finite()
                || !core.world_com().is_finite()
                || !distance.is_finite()
                || !(distance - request.radius).is_finite()
                || !normal.is_finite()
                || !(request.center - normal * distance).is_finite()
                || !(request.center - normal * request.radius).is_finite()
            {
                return Err(RadialImpulseError::IncompleteGeometry { collider: id });
            }
        }
        Ok(())
    }

    fn radial_check_effect(
        &self,
        body: RigidBodyHandle,
        impulse: Vec3,
    ) -> Result<(), RadialImpulseError> {
        if impulse == Vec3::ZERO {
            return Ok(());
        }
        let core = self.bodies[body].core();
        if core
            .colliders
            .iter()
            .any(|id| self.colliders[*id].mass_dirty)
        {
            return Err(RadialImpulseError::PendingMass { body });
        }
        let delta = impulse * core.inv_mass;
        if !impulse.is_finite()
            || !core.inv_mass.is_finite()
            || core.inv_mass <= 0.
            || !delta.is_finite()
            || !(core.velocity + delta).is_finite()
        {
            return Err(RadialImpulseError::NonFiniteEffect { body });
        }
        Ok(())
    }
}
