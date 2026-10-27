use crate::{
    body::{Access, BodySet, Collider, ColliderHandle, ColliderSet, Kind, RigidBodyHandle, Shape},
    geometry::{self, Bounds, Point},
};
use ferravex_math::{Quat, Vec3};
mod cache;
mod compounds;
mod islands;
mod joints;
use cache::Manifold;
pub use cache::{ContactManifold, ContactPoint, SolverSettings};
pub use compounds::{MAX_CHILDREN, MAX_COLLIDERS};
use islands::Islands;
use joints::{JointPhase, JointRow};

/// Maximum logical outer-step impact records. Recording is disabled by default.
/// Call `set_impact_settings` through the dimensional `PhysicsWorld`; its
/// method documentation includes preparation and a runnable recipe.
pub const MAX_IMPACT_RECORDS: usize = 65536;

/// Observation thresholds only; these never change solver tolerances or impulses.
/// The dimensional `PhysicsWorld::set_impact_settings` method documents an
/// example using inferred defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImpactSettings {
    /// First qualifying pairs in substep/solver order are retained, up to this
    /// bound (0..=MAX_IMPACT_RECORDS). Zero records still counts rejections.
    pub capacity: usize,
    /// Strict lower bound on the peak newly solved point normal impulse.
    /// Finite and nonnegative; zero accepts any positive qualifying impulse.
    pub min_normal_impulse: f32,
    /// Strict lower bound on approach speed, finite and positive. Approach must
    /// exceed this both before substep forces/gravity/damping/integration and at
    /// contact creation before warm start. This excludes ordinary resting
    /// support, even with warm start off. Choose above expected solver jitter.
    pub min_approach_speed: f32,
}
impl Default for ImpactSettings {
    fn default() -> Self {
        Self {
            capacity: 64,
            min_normal_impulse: 0.,
            min_approach_speed: 1.,
        }
    }
}
/// One canonical generational collider pair's strongest accepted point solve in
/// the latest outer step, including contacts absent from the final cache.
/// Read these through the dimensional world's `impacts` method after opting
/// in with `set_impact_settings`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImpactRecord {
    pub collider1: ColliderHandle,
    pub collider2: ColliderHandle,
    /// World-space constraint creation position, represented as Vec3 in 2D/3D.
    pub point: Vec3,
    /// Unit world normal from collider1 toward collider2.
    pub normal: Vec3,
    /// Maximum over points, iterations and substeps of
    /// `max(0, accumulated_normal_impulse - initial_warm_start_impulse)`.
    /// Neither a sum over points/substeps nor total energy/acoustic loudness.
    /// Solver iteration overshoot can affect this peak; tangential, rolling,
    /// joint, positional and cached warm-start replay impulses are excluded.
    pub peak_normal_impulse: f32,
    /// Minimum of the before-force and before-warm-start approach speeds at
    /// this record's selected point. Useful when authoring thresholds.
    pub approach_speed: f32,
}
/// Transactional impact configuration failure; the previous settings and
/// snapshot remain intact. Successful reservation before a later allocation
/// failure may retain additional unused capacity for reuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImpactRecordingError {
    InvalidSettings,
    AllocationFailed,
}
impl std::fmt::Display for ImpactRecordingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidSettings => "invalid impact recording settings",
            Self::AllocationFailed => "impact recording reservation failed",
        })
    }
}
impl std::error::Error for ImpactRecordingError {}

/// Contacts include transient impacts from all substeps of the latest outer step.
#[derive(Clone, Copy, Debug)]
pub struct ContactPair {
    pub collider1: ColliderHandle,
    pub collider2: ColliderHandle,
    pub normal: [f32; 3],
    pub sensor: bool,
}
impl ContactPair {
    pub fn has_any_active_contact(&self) -> bool {
        !self.sensor
    }
}
/// Changes to the set of pairs observed during consecutive valid outer steps.
/// Both physical contacts and sensor overlaps count. Changing sensor status alone
/// does not restart an overlap, provided the pair remains collision-eligible.
/// Handles retain their original generations, including in removal stop events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContactEvent {
    Started(ColliderHandle, ColliderHandle),
    Stopped(ColliderHandle, ColliderHandle),
}
/// A copied, current sleep snapshot for a live body; no solver state is exposed
/// for mutation. The dimensional world's `sleep_state` method includes a
/// handle-checked example.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodySleepState {
    /// Only dynamic bodies participate in sleeping and sleep counters.
    pub dynamic: bool,
    pub sleeping: bool,
    /// An explicit wake request is pending until the next valid outer step ends.
    /// Edits, gravity and moving neighbors can also wake bodies without this flag.
    pub wake_requested: bool,
    /// Quiet simulation seconds, capped at the current solver's sleep delay.
    /// Fixed and kinematic bodies report zero. This is not time spent asleep.
    pub quiet_time: f32,
}
/// Current dynamic-body counts, each bounded by the world's 4096-body limit.
/// This is an allocation-free snapshot, not a cumulative event history or the
/// last-step island report. See the dimensional world's `sleep_stats` method
/// for an example.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SleepStats {
    pub awake_bodies: usize,
    pub sleeping_bodies: usize,
    /// Dynamic bodies with explicit wake requests awaiting a valid outer step.
    /// These bodies are also counted in `awake_bodies`.
    pub wake_requests: usize,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct StepStats {
    pub substeps: usize,
    pub broadphase_pairs: usize,
    pub contact_points: usize,
    pub ccd_impacts: usize,
    pub sleeping_bodies: usize,
    pub islands: usize,
    pub active_islands: usize,
    pub sleeping_islands: usize,
    pub persisted_points: usize,
    pub warm_started_points: usize,
    pub reused_manifolds: usize,
    pub cached_manifolds: usize,
    pub cache_dropped: usize,
    pub velocity_constraints: usize,
    pub joint_rows: usize,
    pub joint_constraints: usize,
}
#[derive(Clone, Debug)]
struct Constraint {
    a: RigidBodyHandle,
    b: RigidBodyHandle,
    normal: Vec3,
    point: Vec3,
    local_a: Vec3,
    local_b: Vec3,
    normal_impulse: f32,
    normal_denominator: f32,
    /// Contact point minus each body's world center of mass. Poses are fixed
    /// during velocity iterations, so these are computed once per solve.
    ra: Vec3,
    rb: Vec3,
    impact_baseline: f32,
    impact_peak: f32,
    impact_approach: f32,
    tangent_impulse: Vec3,
    bounce: f32,
    friction: f32,
    static_friction: f32,
    rolling_resistance: f32,
    rolling_impulse: Vec3,
    key: (ColliderHandle, ColliderHandle),
    feature: u64,
    cache: Option<(usize, usize)>,
}
/// One collider's current pose, bounds, substep start position and filter
/// data, cached once per substep for the all-pairs continuous sweep.
#[derive(Clone, Copy)]
struct CcdTarget {
    pose: geometry::Pose,
    start: Vec3,
    bounds: Bounds,
    shape: Shape,
    sensor: bool,
    membership: u32,
    filter: u32,
}
impl CcdTarget {
    fn new(collider: &Collider, body: &crate::body::Core, origin: Vec3, rotation: Quat) -> Self {
        let pose = collider.pose(body);
        Self {
            pose,
            start: origin + rotation * collider.local_translation,
            bounds: geometry::bounds(collider.shape, pose.position, pose.rotation),
            shape: collider.shape,
            sensor: collider.sensor,
            membership: collider.membership,
            filter: collider.filter,
        }
    }
}
#[derive(Clone, Copy)]
struct Proxy {
    body: RigidBodyHandle,
    collider: ColliderHandle,
    bounds: Bounds,
}
/// Bodies and child collider handles use generation counters;
/// capacity is deliberately limited to 4096 bodies for predictable memory use.
pub struct World<B> {
    pub bodies: BodySet<B>,
    pub colliders: ColliderSet,
    pub gravity: Vec3,
    pub joints: crate::JointSet,
    joint_rows: Vec<JointRow>,
    broken_joints: Vec<crate::JointBreakEvent>,
    joint_solver: Option<fn(&mut Self, JointPhase, f32, &mut StepStats)>,
    disabled_joint_pairs: Vec<(RigidBodyHandle, RigidBodyHandle)>,
    planar: bool,
    contacts: Vec<ContactPair>,
    next_contacts: Vec<ContactPair>,
    manifolds: Vec<Manifold>,
    next_manifolds: Vec<Manifold>,
    next_feature: u64,
    settings: SolverSettings,
    impact_settings: Option<ImpactSettings>,
    impacts: Vec<ImpactRecord>,
    impact_dropped: usize,
    impact_motions: Vec<cache::ImpactMotion>,
    islands: Islands,
    last_gravity: Vec3,
    old_positions: Vec<(RigidBodyHandle, Vec3, Quat)>,
    /// Reused sweep scratch: per-collider targets and each `old_positions`
    /// entry's first target index, followed by the total count.
    ccd_targets: Vec<CcdTarget>,
    ccd_first: Vec<usize>,
    mass_updates: Vec<(RigidBodyHandle, Option<crate::mass::Mass>)>,
    events: Vec<ContactEvent>,
    constraints: Vec<Constraint>,
    proxies: Vec<Proxy>,
    points: Vec<Point>,
    pub last_step: StepStats,
}
impl<B: Access> World<B> {
    pub(crate) fn new(gravity: Vec3, planar: bool) -> Self {
        assert!(gravity.is_finite());
        Self {
            bodies: BodySet::default(),
            colliders: ColliderSet::default(),
            gravity,
            joints: Default::default(),
            joint_rows: Vec::new(),
            broken_joints: Vec::new(),
            joint_solver: None,
            disabled_joint_pairs: Vec::new(),
            planar,
            contacts: Vec::new(),
            next_contacts: Vec::new(),
            manifolds: Vec::new(),
            next_manifolds: Vec::new(),
            next_feature: 1,
            settings: Default::default(),
            impact_settings: None,
            impacts: Vec::new(),
            impact_dropped: 0,
            impact_motions: Vec::new(),
            islands: Default::default(),
            last_gravity: gravity,
            old_positions: Vec::new(),
            ccd_targets: Vec::new(),
            ccd_first: Vec::new(),
            mass_updates: Vec::new(),
            events: Vec::new(),
            constraints: Vec::new(),
            proxies: Vec::new(),
            points: Vec::with_capacity(8),
            last_step: StepStats::default(),
        }
    }
    pub fn joint_break_events(&self) -> &[crate::JointBreakEvent] {
        &self.broken_joints
    }
    /// Inspect current body state without indexing a possibly stale handle.
    /// Removed/reused handles return `None`; fixed and kinematic bodies report
    /// `dynamic == false` and never report sleeping. No allocation or wake occurs.
    ///
    /// ```
    /// # #[cfg(feature = "dim2")] {
    /// use ferravex_physics::d2::*;
    /// let mut world = PhysicsWorld::default();
    /// let (body, _) = world.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
    /// let state = world.sleep_state(body).unwrap();
    /// assert!(state.dynamic && !state.sleeping && state.wake_requested);
    /// assert_eq!(state.quiet_time, 0.);
    /// world.remove_body(body);
    /// assert_eq!(world.sleep_state(body), None);
    /// # }
    /// ```
    pub fn sleep_state(&self, body: RigidBodyHandle) -> Option<BodySleepState> {
        let b = self.bodies.get(body)?.core();
        let dynamic = b.kind == Kind::Dynamic;
        Some(BodySleepState {
            dynamic,
            sleeping: b.sleeping,
            wake_requested: b.wake_requested,
            quiet_time: if dynamic {
                b.quiet.min(self.settings.sleep_delay)
            } else {
                0.
            },
        })
    }
    /// Count current dynamic-body states, including edits since the last step.
    /// Fixed/kinematic bodies are excluded. Reads reuse body storage; counts do
    /// not accumulate over steps. Use `last_step` for last-step island counts.
    ///
    /// ```
    /// # #[cfg(feature = "dim2")] {
    /// use ferravex_physics::d2::*;
    /// let mut world = PhysicsWorld::default();
    /// world.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
    /// for _ in 0..120 { world.step(1. / 60.); }
    /// let stats = world.sleep_stats();
    /// assert_eq!(stats.awake_bodies, 0);
    /// assert_eq!(stats.sleeping_bodies, 1);
    /// assert_eq!(stats.wake_requests, 0);
    /// # }
    /// ```
    pub fn sleep_stats(&self) -> SleepStats {
        let mut stats = SleepStats::default();
        for (_, body) in self.bodies.iter() {
            let b = body.core();
            if b.kind != Kind::Dynamic {
                continue;
            }
            if b.sleeping {
                stats.sleeping_bodies += 1;
            } else {
                stats.awake_bodies += 1;
            }
            stats.wake_requests += usize::from(b.wake_requested);
        }
        stats
    }
    /// Wake a dynamic body immediately and request island propagation on the
    /// next valid step. Returns whether it was sleeping; an already awake body
    /// still has its quiet timer reset and its wake request set. This neither
    /// advances simulation nor changes the last-step contacts/events/stats.
    ///
    /// Returns [`crate::PhysicsError::InvalidHandle`] for stale handles and
    /// [`crate::PhysicsError::InvalidBody`] for fixed/kinematic bodies, without edits.
    ///
    /// ```
    /// # #[cfg(feature = "dim2")] {
    /// use ferravex_physics::{PhysicsError, d2::*};
    /// let mut world = PhysicsWorld::default();
    /// let (body, _) = world.insert(RigidBodyBuilder::dynamic(), ColliderBuilder::ball(1.));
    /// for _ in 0..120 { world.step(1. / 60.); }
    /// assert_eq!(world.wake_body(body), Ok(true));
    /// assert_eq!(world.wake_body(body), Ok(false));
    /// assert_eq!(world.sleep_stats().wake_requests, 1);
    /// world.remove_body(body);
    /// assert_eq!(world.wake_body(body), Err(PhysicsError::InvalidHandle));
    /// # }
    /// ```
    pub fn wake_body(&mut self, body: RigidBodyHandle) -> Result<bool, crate::PhysicsError> {
        let b = self
            .bodies
            .get_mut(body)
            .ok_or(crate::PhysicsError::InvalidHandle)?
            .core_mut();
        if b.kind != Kind::Dynamic {
            return Err(crate::PhysicsError::InvalidBody);
        }
        let was_sleeping = b.sleeping;
        b.wake();
        Ok(was_sleeping)
    }
    pub fn insert(
        &mut self,
        b: impl Into<B>,
        c: impl Into<Collider>,
    ) -> (RigidBodyHandle, ColliderHandle) {
        let body = self
            .insert_compound(b, [c.into()])
            .expect("valid body/collider within capacities");
        (body, self.bodies[body].core().colliders[0])
    }

    /// Remove a live body and its colliders/joints; return false for a stale handle.
    /// Contact queries immediately exclude its colliders. Their stop events are
    /// reported once on the next valid [`Self::step`], even if slots are reused.
    pub fn remove_body(&mut self, h: RigidBodyHandle) -> bool {
        if self.bodies.get(h).is_none() {
            return false;
        }
        loop {
            let id = self
                .joints
                .iter()
                .find(|(_, j)| j.a == h || j.b == h)
                .map(|(id, _)| id);
            let Some(id) = id else {
                break;
            };
            self.remove_joint(id);
        }
        let Some(b) = self.bodies.remove(h) else {
            return false;
        };
        for &c in &b.core().colliders {
            for contact in &self.contacts {
                if contact.sensor {
                    continue;
                }
                let other = if contact.collider1 == c {
                    Some(contact.collider2)
                } else if contact.collider2 == c {
                    Some(contact.collider1)
                } else {
                    None
                };
                if let Some(other) = other
                    .and_then(|id| self.colliders.get(id))
                    .and_then(|o| o.parent)
                    && let Some(body) = self.bodies.get_mut(other)
                {
                    body.core_mut().wake();
                }
            }
            self.manifolds.retain(|m| m.key.0 != c && m.key.1 != c);
            self.next_manifolds.retain(|m| m.key.0 != c && m.key.1 != c);
            self.colliders.remove(c);
        }
        true
    }
    /// Look up a pair from the last valid outer step, in either handle order.
    /// Removed handles return `None` immediately, without consuming the pending
    /// stop event. Pose, sensor and layer edits are reflected after the next step.
    ///
    /// ```
    /// # #[cfg(feature = "dim2")] {
    /// use ferravex_physics::{ContactEvent, d2::*};
    /// let mut world = PhysicsWorld::default();
    /// let (_, sensor) = world.insert(
    ///     RigidBodyBuilder::fixed(), ColliderBuilder::ball(2.).sensor(true),
    /// );
    /// let (body, collider) = world.insert(
    ///     RigidBodyBuilder::dynamic(), ColliderBuilder::ball(0.5),
    /// );
    /// world.step(1. / 60.);
    /// assert!(world.contact_pair(collider, sensor).unwrap().sensor);
    /// world.remove_body(body);
    /// assert!(world.contact_pair(sensor, collider).is_none());
    /// world.step(1. / 60.);
    /// assert_eq!(world.contact_events(), &[ContactEvent::Stopped(sensor, collider)]);
    /// # }
    /// ```
    pub fn contact_pair(&self, a: ColliderHandle, b: ColliderHandle) -> Option<&ContactPair> {
        self.colliders.get(a)?;
        self.colliders.get(b)?;
        let key = pair_key(a, b);
        self.contacts
            .binary_search_by_key(&key, |c| pair_key(c.collider1, c.collider2))
            .ok()
            .map(|i| &self.contacts[i])
    }
    /// Borrow events from the last valid outer step without consuming them.
    /// Started events precede stopped events; each group is sorted by canonical
    /// `(collider1, collider2)` handle order. A pair appears at most once per group.
    /// Removal stops arrive on the next valid step; an empty-world step still
    /// reports them. Invalid timestep/gravity panics leave this snapshot intact.
    /// Transient substep contacts count for the whole outer step and stop on a
    /// subsequent step in which they are absent.
    pub fn contact_events(&self) -> &[ContactEvent] {
        &self.events
    }
    /// Iterate last-step contacts in canonical pair order, skipping removed
    /// colliders. Like [`Self::contact_pair`], this includes sensors and transient
    /// substep contacts and borrows existing storage without allocating.
    pub fn contacts(&self) -> impl Iterator<Item = &ContactPair> {
        self.contacts.iter().filter(|c| {
            self.colliders.get(c.collider1).is_some() && self.colliders.get(c.collider2).is_some()
        })
    }
    /// Advance a fixed simulation interval in (0, 0.1] seconds. Forces apply for
    /// this complete interval and are then cleared. The caller owns accumulation.
    ///
    /// # Panics
    /// Invalid timesteps or non-finite gravity panic before replacing contact
    /// events or advancing bodies. Invalid compound mass properties also panic.
    pub fn step(&mut self, dt: f32) -> StepStats {
        assert!(
            dt.is_finite() && dt > 0. && dt <= 0.1,
            "use a fixed timestep in (0, 0.1]"
        );
        assert!(self.gravity.is_finite());
        self.refresh_mass_properties()
            .expect("valid compound mass properties");
        self.run_joints(JointPhase::Refresh, dt, &mut StepStats::default());
        self.next_contacts.clear();
        self.events.clear();
        self.clear_impacts();
        let mut substeps = 1;
        for (_, body) in self.bodies.iter_mut() {
            let b = body.core_mut();
            if b.kind == Kind::Kinematic {
                b.velocity = b.next_position.map_or(Vec3::ZERO, |p| {
                    (p + b.next_rotation.unwrap_or(b.rotation) * b.local_com - b.world_com()) / dt
                });
                b.angular_velocity = b
                    .next_rotation
                    .map_or(Vec3::ZERO, |r| rotation_delta(b.rotation, r) / dt);
                substeps = substeps.max((b.angular_velocity.length() * dt / 0.06).ceil() as usize);
            }
            if b.kind == Kind::Dynamic && b.ccd {
                let mut size = f32::INFINITY;
                for child in &b.colliders {
                    let extent = match self.colliders[*child].shape {
                        Shape::Ball(r) => r,
                        Shape::Box(h) => h.x.min(h.y).min(if self.planar { h.x } else { h.z }),
                        #[cfg(feature = "dim2")]
                        Shape::Capsule { radius, .. } => radius,
                        #[cfg(feature = "dim3")]
                        Shape::Cylinder {
                            radius,
                            half_height,
                        } => radius.min(half_height),
                    };
                    size = size.min(extent);
                }
                substeps = substeps.max((b.velocity.length() * dt / (size * 0.75)).ceil() as usize);
                substeps = substeps.max((b.angular_velocity.length() * dt / 0.06).ceil() as usize);
            }
        }
        for (_, j) in self.joints.iter() {
            if j.description().enabled {
                let speed =
                    joints::motor_speed(j, self.bodies[j.a].core(), self.bodies[j.b].core());
                substeps = substeps.max((speed * dt / 0.06).ceil() as usize);
            }
        }
        substeps = substeps.clamp(1, 8);
        let h = dt / substeps as f32;
        let mut stats = StepStats {
            substeps,
            ..StepStats::default()
        };
        let gravity_changed = self.gravity != self.last_gravity;
        self.wake_previous_islands(gravity_changed);
        if gravity_changed {
            self.manifolds.clear();
        }
        self.broken_joints.clear();
        for (_, j) in self.joints.iter_mut() {
            j.peak = crate::JointLoad::default();
        }
        for _ in 0..substeps {
            self.capture_impact_motion();
            self.old_positions.clear();
            self.old_positions.extend(
                self.bodies
                    .iter()
                    .map(|(id, b)| (id, b.core().position, b.core().rotation)),
            );
            for (_, body) in self.bodies.iter_mut() {
                let b = body.core_mut();
                if b.kind == Kind::Dynamic && !b.sleeping {
                    b.velocity += (self.gravity * b.gravity_scale + b.force * b.inv_mass) * h;
                    b.angular_velocity += b.inverse_inertia(b.torque) * h;
                    b.velocity /= 1. + b.linear_damping * h;
                    b.angular_velocity /= 1. + b.angular_damping * h;
                }
                if b.kind != Kind::Fixed && !b.sleeping {
                    b.integrate(h);
                }
            }
            stats.ccd_impacts += self.sweep_fast_bodies();
            self.find_contacts(h, &mut stats);
            self.wake_edited_contact_bodies();
            self.wake_contact_islands();
            self.warm_start(&mut stats);
            self.run_joints(JointPhase::Prepare, h, &mut stats);
            self.solve_velocities(&mut stats);
            self.record_impacts();
            self.run_joints(JointPhase::Integrate, h, &mut stats);
            self.solve_positions();
            self.store_manifolds(h);
            self.run_joints(JointPhase::Store, h, &mut stats);
            self.sleep_islands(h, &mut stats);
        }
        for (_, body) in self.bodies.iter_mut() {
            let b = body.core_mut();
            b.force = Vec3::ZERO;
            b.torque = Vec3::ZERO;
            if b.kind == Kind::Kinematic {
                if let Some(p) = b.next_position.take() {
                    b.position = p;
                }
                if let Some(q) = b.next_rotation.take() {
                    b.set_rotation(q);
                }
            }
            b.wake_requested = false;
            b.solver_dirty = false;
            if b.sleeping {
                stats.sleeping_bodies += 1;
            }
        }
        self.next_contacts
            .sort_unstable_by_key(|c| pair_key(c.collider1, c.collider2));
        self.next_contacts
            .dedup_by_key(|c| pair_key(c.collider1, c.collider2));
        for c in &self.next_contacts {
            if self
                .contacts
                .binary_search_by_key(&pair_key(c.collider1, c.collider2), |c| {
                    pair_key(c.collider1, c.collider2)
                })
                .is_err()
            {
                self.events
                    .push(ContactEvent::Started(c.collider1, c.collider2));
            }
        }
        for c in &self.contacts {
            if self
                .next_contacts
                .binary_search_by_key(&pair_key(c.collider1, c.collider2), |c| {
                    pair_key(c.collider1, c.collider2)
                })
                .is_err()
            {
                self.events
                    .push(ContactEvent::Stopped(c.collider1, c.collider2));
            }
        }
        std::mem::swap(&mut self.contacts, &mut self.next_contacts);
        self.next_contacts.clear();
        for (_, c) in self.colliders.iter_mut() {
            c.dirty = false;
        }
        self.last_gravity = self.gravity;
        stats.cached_manifolds = self.manifolds.len();
        self.last_step = stats;
        ferravex_core::telemetry::record_physics(
            stats.broadphase_pairs,
            stats.contact_points,
            stats.ccd_impacts,
        );
        stats
    }
    fn wake_edited_contact_bodies(&mut self) {
        // A sensor becoming solid or newly matching layers has no previous
        // solid island to wake. A moved or newly inserted fixed/kinematic body
        // also has no dynamic island of its own. Insertion sets wake_requested
        // even when there is no dirty pose or collider revision. Seed current
        // solid contacts before their normal island activation/solving pass.
        for c in &self.constraints {
            if self.colliders[c.key.0].dirty
                || self.colliders[c.key.1].dirty
                || self.bodies[c.a].core().solver_dirty
                || self.bodies[c.b].core().solver_dirty
                || self.bodies[c.a].core().wake_requested
                || self.bodies[c.b].core().wake_requested
            {
                for id in [c.a, c.b] {
                    let b = self.bodies[id].core_mut();
                    if b.kind == Kind::Dynamic && b.sleeping {
                        b.wake();
                    }
                }
            }
        }
    }
    fn sweep_fast_bodies(&mut self) -> usize {
        let swept_body = |b: &crate::body::Core| b.ccd && b.kind == Kind::Dynamic;
        if !self
            .old_positions
            .iter()
            .any(|&(id, _, _)| swept_body(self.bodies[id].core()))
        {
            return 0;
        }
        // Each target collider's pose, start, bounds and filter data are
        // pair-independent: gather them once per substep instead of once per
        // swept collider. `ccd_first[i]..ccd_first[i + 1]` are body i's colliders.
        let mut targets = std::mem::take(&mut self.ccd_targets);
        let mut first = std::mem::take(&mut self.ccd_first);
        targets.clear();
        first.clear();
        for &(id, origin, rotation) in &self.old_positions {
            let body = self.bodies[id].core();
            first.push(targets.len());
            targets.extend(
                body.colliders
                    .iter()
                    .map(|&child| CcdTarget::new(&self.colliders[child], body, origin, rotation)),
            );
        }
        first.push(targets.len());
        let mut hits = 0;
        for (index, &(id, start_origin, start_rotation)) in self.old_positions.iter().enumerate() {
            if !swept_body(self.bodies[id].core()) {
                continue;
            }
            let mut closest = 1.;
            let mut correction = None;
            for own in &targets[first[index]..first[index + 1]] {
                if own.sensor {
                    continue;
                }
                let radius = match own.shape {
                    Shape::Ball(r) => r,
                    Shape::Box(h) => h.length(),
                    _ => continue,
                };
                let start = own.start;
                let end = own.pose.position;
                let minimum_motion = if matches!(own.shape, Shape::Ball(_)) {
                    radius * radius * 0.04
                } else {
                    1e-12
                };
                for (other_index, &(other, _, _)) in self.old_positions.iter().enumerate() {
                    if other == id || self.joint_contacts_disabled(id, other) {
                        continue;
                    }
                    for target in &targets[first[other_index]..first[other_index + 1]] {
                        if target.sensor
                            || own.membership & target.filter == 0
                            || target.membership & own.filter == 0
                        {
                            continue;
                        }
                        let target_pose = target.pose;
                        let relative_start = start - target.start + target_pose.position;
                        if relative_start.distance_squared(end) < minimum_motion {
                            continue;
                        }
                        let swept = Bounds {
                            min: relative_start.min(end) - Vec3::splat(radius),
                            max: relative_start.max(end) + Vec3::splat(radius),
                        };
                        if !swept.intersects(target.bounds) {
                            continue;
                        }
                        let impact = match (own.shape, target.shape) {
                            (Shape::Ball(r), shape) => geometry::sweep_ball(
                                relative_start,
                                end,
                                r,
                                shape,
                                target_pose.position,
                                target_pose.rotation,
                                self.planar,
                            ),
                            (Shape::Box(h), Shape::Box(target_half)) => geometry::sweep_box(
                                relative_start,
                                end,
                                h,
                                own.pose.rotation,
                                target_half,
                                &target_pose,
                            ),
                            _ => None,
                        };
                        if let Some((t, n)) = impact
                            && t < closest
                            && (end - relative_start).dot(n) < -1e-7
                        {
                            closest = t;
                            correction = Some(
                                relative_start + (end - relative_start) * t + n * 0.00002 - end,
                            );
                        }
                    }
                }
            }
            if let Some(delta) = correction {
                self.bodies[id].core_mut().position += delta;
                // Later swept bodies observe this corrected pose as their target.
                let body = self.bodies[id].core();
                for (cached, &child) in targets[first[index]..first[index + 1]]
                    .iter_mut()
                    .zip(&body.colliders)
                {
                    *cached =
                        CcdTarget::new(&self.colliders[child], body, start_origin, start_rotation);
                }
                hits += 1;
            }
        }
        self.ccd_targets = targets;
        self.ccd_first = first;
        hits
    }
    fn solve_velocities(&mut self, stats: &mut StepStats) {
        // Waking is complete and velocity iterations do not change body poses,
        // masses or contact normals. Reuse the normal denominator for all
        // iterations; tangent directions still follow the current slip velocity.
        for c in &mut self.constraints {
            let a = self.bodies[c.a].core();
            let b = self.bodies[c.b].core();
            c.ra = c.point - a.world_com();
            c.rb = c.point - b.world_com();
            c.normal_denominator = if a.contact_inv_mass() + b.contact_inv_mass() == 0. {
                0.
            } else {
                effective_mass(a, b, c.ra, c.rb, c.normal)
            };
        }
        for _ in 0..self.settings.velocity_iterations {
            for c in &mut self.constraints {
                let (a, b) = self.bodies.two_mut(c.a, c.b);
                let a = a.core_mut();
                let b = b.core_mut();
                if a.sleeping && (b.sleeping || b.kind != Kind::Dynamic)
                    || b.sleeping && a.kind != Kind::Dynamic
                {
                    continue;
                }
                stats.velocity_constraints += 1;
                let (ra, rb) = (c.ra, c.rb);
                let denom = c.normal_denominator;
                if denom <= 1e-10 {
                    continue;
                }
                let relative = b.offset_velocity(rb) - a.offset_velocity(ra);
                let change = (c.bounce - relative.dot(c.normal)) / denom;
                let accumulated = (c.normal_impulse + change).max(0.);
                let j = c.normal * (accumulated - c.normal_impulse);
                c.normal_impulse = accumulated;
                if c.impact_approach > 0. {
                    c.impact_peak = c.impact_peak.max((accumulated - c.impact_baseline).max(0.));
                }
                a.offset_impulse(-j, ra);
                b.offset_impulse(j, rb);
                let relative = b.offset_velocity(rb) - a.offset_velocity(ra);
                let tangent = relative - c.normal * relative.dot(c.normal);
                let mut candidate = c.tangent_impulse;
                if let Some(t) = tangent.try_normalize() {
                    let denominator = effective_mass(a, b, ra, rb, t);
                    if denominator > 1e-10 {
                        candidate -= t * (relative.dot(t) / denominator);
                    }
                }
                // Re-clamp even at zero slip: a reduced normal load also reduces
                // the friction cone of an already accumulated tangent impulse.
                let new = if candidate.length() <= c.static_friction * c.normal_impulse {
                    candidate
                } else {
                    candidate.clamp_length_max(c.friction * c.normal_impulse)
                };
                let j = new - c.tangent_impulse;
                c.tangent_impulse = new;
                a.offset_impulse(-j, ra);
                b.offset_impulse(j, rb);
                if c.rolling_resistance > 0. {
                    let relative = b.angular_velocity - a.angular_velocity;
                    if let Some(axis) = relative.try_normalize() {
                        let denominator = axis
                            .dot(a.contact_inverse_inertia(axis) + b.contact_inverse_inertia(axis));
                        if denominator > 1e-10 {
                            let new = (c.rolling_impulse
                                - axis * (relative.dot(axis) / denominator))
                                .clamp_length_max(c.rolling_resistance * c.normal_impulse);
                            let j = new - c.rolling_impulse;
                            c.rolling_impulse = new;
                            a.angular_velocity -= a.contact_inverse_inertia(j);
                            b.angular_velocity += b.contact_inverse_inertia(j);
                        }
                    }
                }
            }
            self.run_joints(JointPhase::Velocity, 0., stats);
        }
    }
    fn solve_positions(&mut self) {
        for _ in 0..self.settings.position_iterations {
            for c in &self.constraints {
                let (a, b) = self.bodies.two_mut(c.a, c.b);
                let a = a.core_mut();
                let b = b.core_mut();
                if a.contact_inv_mass() + b.contact_inv_mass() == 0. {
                    continue;
                }
                let pa = a.position + a.rotation * c.local_a;
                let pb = b.position + b.rotation * c.local_b;
                let ra = a.rotation * (c.local_a - a.local_com);
                let rb = b.rotation * (c.local_b - b.local_com);
                let depth = -(pb - pa).dot(c.normal);
                if depth <= 0.0005 {
                    continue;
                }
                let denom = effective_mass(a, b, ra, rb, c.normal);
                if denom <= 1e-10 {
                    continue;
                }
                let j = c.normal * ((depth - 0.0005).min(0.2) * 0.4 / denom);
                a.position_impulse(-j, ra.cross(-j));
                b.position_impulse(j, rb.cross(j));
            }
            self.run_joints(JointPhase::Position, 0., &mut StepStats::default());
        }
    }
    pub(crate) fn cast_ray(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<(RigidBodyHandle, ColliderHandle, f32, Vec3, Vec3)> {
        self.cast_sphere(origin, direction, max_distance, 0., u32::MAX, None)
    }
    /// Current-pose sphere sweep. Radius zero preserves the raycast behavior.
    pub(crate) fn cast_sphere(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
        radius: f32,
        mask: u32,
        ignore: Option<RigidBodyHandle>,
    ) -> Option<(RigidBodyHandle, ColliderHandle, f32, Vec3, Vec3)> {
        if !origin.is_finite()
            || !direction.is_finite()
            || !max_distance.is_finite()
            || max_distance < 0.
            || !radius.is_finite()
            || !(0. ..=1000.).contains(&radius)
        {
            return None;
        }
        let direction = direction.try_normalize()?;
        let mut best = None;
        let mut limit = max_distance;
        for (id, c) in self.colliders.iter() {
            if c.sensor || c.membership & mask == 0 || c.parent == ignore {
                continue;
            }
            let body = c.parent.unwrap();
            let b = self.bodies[body].core();
            let pose = c.pose(b);
            let (d, n) =
                geometry::distance(c.shape, pose.position, pose.rotation, origin, self.planar);
            let hit = if d <= radius {
                Some((0., n))
            } else {
                geometry::sweep_ball(
                    origin,
                    origin + direction * limit,
                    radius,
                    c.shape,
                    pose.position,
                    pose.rotation,
                    self.planar,
                )
                .map(|(t, n)| (t * limit, n))
            };
            if let Some((distance, normal)) = hit {
                limit = distance;
                best = Some((body, id, distance, origin + direction * distance, normal));
            }
        }
        best
    }
}
fn pair_key(a: ColliderHandle, b: ColliderHandle) -> (ColliderHandle, ColliderHandle) {
    if a < b { (a, b) } else { (b, a) }
}
fn effective_mass(
    a: &crate::body::Core,
    b: &crate::body::Core,
    ra: Vec3,
    rb: Vec3,
    n: Vec3,
) -> f32 {
    a.contact_inv_mass()
        + b.contact_inv_mass()
        + n.dot(
            a.contact_inverse_inertia(ra.cross(n)).cross(ra)
                + b.contact_inverse_inertia(rb.cross(n)).cross(rb),
        )
}
fn rotation_delta(from: Quat, to: Quat) -> Vec3 {
    let mut delta = (to * from.conjugate()).normalize();
    if delta.w < 0. {
        delta = -delta;
    }
    let axis = Vec3::new(delta.x, delta.y, delta.z);
    let length = axis.length();
    if length < 1e-9 {
        Vec3::ZERO
    } else {
        axis * (2. * length.atan2(delta.w) / length)
    }
}

#[cfg(all(test, any(feature = "dim2", feature = "dim3")))]
mod journey200_037 {
    use super::*;
    use crate::PhysicsError;

    const DT: f32 = 1. / 120.;

    macro_rules! sleep_tests {
        ($module:ident, $dimension:ident) => {
            mod $module {
                use super::*;
                use crate::$dimension::{
                    ColliderBuilder, JointBuilder, PhysicsWorld, RigidBodyBuilder, Vector,
                };

                fn world() -> PhysicsWorld {
                    let mut world = PhysicsWorld::default();
                    world
                        .set_solver_settings(SolverSettings {
                            sleep_delay: 0.05,
                            ..SolverSettings::default()
                        })
                        .unwrap();
                    world
                }

                fn body(world: &mut PhysicsWorld, x: f32) -> RigidBodyHandle {
                    world
                        .insert(
                            RigidBodyBuilder::dynamic().translation(Vector::X * x),
                            ColliderBuilder::ball(1.),
                        )
                        .0
                }

                fn settle(world: &mut PhysicsWorld) {
                    for _ in 0..20 {
                        world.step(DT);
                    }
                }

                #[test]
                fn checked_inspection_and_wake_reject_stale_and_nondynamic_bodies() {
                    let mut world = world();
                    let fixed = world.insert_body(RigidBodyBuilder::fixed()).unwrap();
                    let kinematic = world
                        .insert_body(RigidBodyBuilder::kinematic_position_based())
                        .unwrap();
                    let dynamic = body(&mut world, 0.);
                    assert_eq!(
                        world.sleep_stats(),
                        SleepStats {
                            awake_bodies: 1,
                            sleeping_bodies: 0,
                            wake_requests: 1,
                        }
                    );
                    settle(&mut world);
                    for h in [fixed, kinematic] {
                        let before = world.sleep_state(h).unwrap();
                        assert!(!before.dynamic && !before.sleeping && !before.wake_requested);
                        assert_eq!(before.quiet_time, 0.);
                        assert_eq!(world.wake_body(h), Err(PhysicsError::InvalidBody));
                        assert_eq!(world.sleep_state(h), Some(before));
                    }
                    let sleeping = world.sleep_state(dynamic).unwrap();
                    assert!(sleeping.dynamic && sleeping.sleeping && !sleeping.wake_requested);
                    assert_eq!(sleeping.quiet_time, world.solver_settings().sleep_delay);
                    assert_eq!(world.wake_body(dynamic), Ok(true));
                    assert_eq!(world.wake_body(dynamic), Ok(false));
                    let awake = world.sleep_state(dynamic).unwrap();
                    assert!(!awake.sleeping && awake.wake_requested);
                    assert_eq!(awake.quiet_time, 0.);
                    assert!(world.remove_body(dynamic));
                    let replacement = body(&mut world, 0.);
                    assert_eq!(dynamic.index, replacement.index);
                    assert_ne!(dynamic, replacement);
                    let stats = world.sleep_stats();
                    assert_eq!(world.sleep_state(dynamic), None);
                    assert_eq!(world.wake_body(dynamic), Err(PhysicsError::InvalidHandle));
                    assert_eq!(world.sleep_stats(), stats);
                    assert_eq!(stats.awake_bodies, 1);
                    assert_eq!(stats.sleeping_bodies, 0);
                }

                #[test]
                fn live_counts_update_before_step_without_consuming_requests_or_events() {
                    let mut world = world();
                    let a = body(&mut world, 0.);
                    let b = body(&mut world, 2.);
                    settle(&mut world);
                    assert_eq!(world.sleep_stats().sleeping_bodies, 2);
                    assert_eq!(world.last_step.sleeping_bodies, 2);
                    let ca = world.bodies[a].colliders()[0];
                    let cb = world.bodies[b].colliders()[0];
                    assert!(world.contact_events().is_empty());
                    assert!(world.contact_pair(ca, cb).is_some());
                    assert_eq!(world.wake_body(a), Ok(true));
                    let stats = world.sleep_stats();
                    assert_eq!(stats.awake_bodies, 1);
                    assert_eq!(stats.sleeping_bodies, 1);
                    assert_eq!(stats.wake_requests, 1);
                    assert_eq!(world.last_step.sleeping_bodies, 2);
                    assert!(world.sleep_state(b).unwrap().sleeping);
                    assert!(world.contact_events().is_empty());
                    assert!(
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            world.step(f32::NAN);
                        }))
                        .is_err()
                    );
                    assert_eq!(world.sleep_stats(), stats);
                    let step = world.step(DT);
                    assert_eq!(step.sleeping_bodies, 0);
                    assert_eq!(world.sleep_stats().awake_bodies, 2);
                    assert_eq!(world.sleep_stats().wake_requests, 0);
                    assert!(world.contact_events().is_empty());
                    assert_eq!(world.contacts().count(), 1);
                    assert!(world.remove_body(a));
                    assert_eq!(world.sleep_stats().awake_bodies, 1);
                    world.step(DT);
                    assert_eq!(world.contact_events(), &[ContactEvent::Stopped(ca, cb)]);
                }

                #[test]
                fn new_fixed_and_stationary_kinematic_contacts_wake_sleeping_islands() {
                    for obstacle in [
                        RigidBodyBuilder::fixed(),
                        RigidBodyBuilder::kinematic_position_based(),
                    ] {
                        let mut world = world();
                        let a = body(&mut world, 0.);
                        let b = body(&mut world, 2.);
                        let isolated = body(&mut world, 10.);
                        settle(&mut world);
                        assert_eq!(world.sleep_stats().sleeping_bodies, 3);
                        let ca = world.bodies[a].colliders()[0];
                        let (_, obstacle) = world.insert(
                            obstacle.translation(-Vector::X * 1.5),
                            ColliderBuilder::ball(1.),
                        );
                        world.step(DT);
                        assert!(!world.sleep_state(a).unwrap().sleeping);
                        assert!(!world.sleep_state(b).unwrap().sleeping);
                        assert!(world.sleep_state(isolated).unwrap().sleeping);
                        assert!(world.bodies[a].translation().x > 0.);
                        assert_eq!(world.sleep_stats().awake_bodies, 2);
                        assert_eq!(world.sleep_stats().sleeping_bodies, 1);
                        assert_eq!(world.sleep_stats().wake_requests, 0);
                        assert_eq!(
                            world.contact_events(),
                            &[ContactEvent::Started(ca, obstacle)]
                        );
                        world.step(DT);
                        assert!(world.contact_events().is_empty());
                    }
                }

                #[test]
                fn sensor_insertion_and_explicit_wakes_leave_unconnected_sleepers_alone() {
                    let mut world = world();
                    let a = body(&mut world, 0.);
                    let b = body(&mut world, 4.);
                    settle(&mut world);
                    let (_, sensor) = world.insert(
                        RigidBodyBuilder::fixed().translation(Vector::X * 2.),
                        ColliderBuilder::ball(4.).sensor(true),
                    );
                    world.step(DT);
                    assert_eq!(world.sleep_stats().sleeping_bodies, 2);
                    assert_eq!(world.contacts().count(), 2);
                    assert!(world.contacts().all(|c| c.sensor));
                    assert_eq!(world.wake_body(a), Ok(true));
                    world.step(DT);
                    assert!(!world.sleep_state(a).unwrap().sleeping);
                    assert!(world.sleep_state(b).unwrap().sleeping);
                    assert!(world.contact_events().is_empty());
                    assert!(world.remove_collider(sensor).is_ok());
                    world.step(DT);
                    assert!(world.sleep_state(b).unwrap().sleeping);
                }

                #[test]
                fn explicit_wake_propagates_through_joints_on_the_next_step() {
                    let mut world = world();
                    let a = body(&mut world, 0.);
                    let b = body(&mut world, 4.);
                    let isolated = body(&mut world, 10.);
                    world.insert_joint(a, b, JointBuilder::fixed()).unwrap();
                    settle(&mut world);
                    assert_eq!(world.sleep_stats().sleeping_bodies, 3);
                    assert_eq!(world.wake_body(a), Ok(true));
                    assert!(world.sleep_state(b).unwrap().sleeping);
                    world.step(DT);
                    assert!(!world.sleep_state(b).unwrap().sleeping);
                    assert!(world.sleep_state(isolated).unwrap().sleeping);
                    assert_eq!(world.sleep_stats().awake_bodies, 2);
                }

                #[test]
                fn collider_attachment_and_pose_edits_wake_current_contacts() {
                    for attach in [false, true] {
                        let mut world = world();
                        let dynamic = body(&mut world, 0.);
                        let (fixed, collider) = world.insert(
                            RigidBodyBuilder::fixed().translation(Vector::X * 5.),
                            ColliderBuilder::ball(1.),
                        );
                        settle(&mut world);
                        assert!(world.sleep_state(dynamic).unwrap().sleeping);
                        if attach {
                            world
                                .attach_collider(
                                    fixed,
                                    ColliderBuilder::ball(1.).translation(-Vector::X * 4.),
                                )
                                .unwrap();
                        } else {
                            world
                                .set_collider_pose(collider, -Vec3::X * 4., Quat::IDENTITY)
                                .unwrap();
                        }
                        world.step(DT);
                        assert!(!world.sleep_state(dynamic).unwrap().sleeping);
                        assert!(world.bodies[dynamic].translation().x < 0.);
                        assert_eq!(world.sleep_stats().awake_bodies, 1);
                    }
                }

                #[test]
                fn quiet_time_is_capped_and_disabling_sleep_resets_live_state() {
                    let mut world = world();
                    let h = body(&mut world, 0.);
                    world.step(DT);
                    let initial = world.sleep_state(h).unwrap();
                    assert!(!initial.sleeping && !initial.wake_requested);
                    assert!(initial.quiet_time > 0. && initial.quiet_time < 0.05);
                    settle(&mut world);
                    assert_eq!(world.sleep_state(h).unwrap().quiet_time, 0.05);
                    let settings = world.solver_settings();
                    world
                        .set_solver_settings(SolverSettings {
                            allow_sleep: false,
                            ..settings
                        })
                        .unwrap();
                    assert_eq!(world.sleep_stats().awake_bodies, 1);
                    assert_eq!(world.sleep_stats().wake_requests, 1);
                    settle(&mut world);
                    assert_eq!(world.sleep_stats().awake_bodies, 1);
                    assert_eq!(world.sleep_stats().sleeping_bodies, 0);
                    assert_eq!(world.sleep_state(h).unwrap().quiet_time, 0.);
                    world.remove_body(h);
                    assert_eq!(world.sleep_stats(), SleepStats::default());
                }
            }
        };
    }

    #[cfg(feature = "dim2")]
    sleep_tests!(planar, d2);
    #[cfg(feature = "dim3")]
    sleep_tests!(spatial, d3);
}

#[cfg(test)]
mod journey400_221 {
    use super::*;
    macro_rules! cache_test {
        ($module:ident, $dimension:ident) => {
            mod $module {
                use super::*;
                use crate::$dimension::{
                    ColliderBuilder, JointBuilder, PhysicsWorld, RigidBodyBuilder, RigidBodyPose,
                    TeleportOptions, Vector, VelocityPolicy,
                };
                #[test]
                fn teleport_revision_clears_attached_joint_impulses_at_refresh() {
                    let mut world = PhysicsWorld::default();
                    let anchor = world.insert_body(RigidBodyBuilder::fixed()).unwrap();
                    let (body, _) = world.insert(
                        RigidBodyBuilder::dynamic()
                            .translation(Vector::X)
                            .linvel(Vector::X),
                        ColliderBuilder::ball(0.2),
                    );
                    let joint = world
                        .insert_joint(anchor, body, JointBuilder::distance(1.))
                        .unwrap();
                    world.step(1. / 60.);
                    assert!(
                        world.joints[joint]
                            .cache
                            .iter()
                            .any(|row| row.impulse != 0.)
                    );
                    let old_revisions = world.joints[joint].revisions;
                    world
                        .try_teleport_body(
                            body,
                            RigidBodyPose {
                                translation: Vector::X * 1.1,
                                rotation: *world.bodies[body].rotation(),
                            },
                            TeleportOptions::new(VelocityPolicy::Clear),
                        )
                        .unwrap();
                    // The production pre-solve refresh must discard every cached
                    // row while retaining the joint and authored configuration.
                    world
                        .raw
                        .run_joints(JointPhase::Refresh, 1. / 60., &mut StepStats::default());
                    assert!(
                        world.joints[joint]
                            .cache
                            .iter()
                            .all(|row| row.impulse == 0.)
                    );
                    assert_eq!(world.joints[joint].dt, 0.);
                    assert_ne!(world.joints[joint].revisions, old_revisions);
                    assert_eq!(world.joints[joint].bodies(), [anchor, body]);
                }
            }
        };
    }
    #[cfg(feature = "dim2")]
    cache_test!(planar, d2);
    #[cfg(feature = "dim3")]
    cache_test!(spatial, d3);
}
