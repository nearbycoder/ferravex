use super::*;
pub(super) const MAX_POINTS: usize = 8;
pub(super) struct ImpactMotion {
    body: RigidBodyHandle,
    center: Vec3,
    velocity: Vec3,
    angular: Vec3,
}

fn push_manifold(buffer: &mut Vec<Manifold>, manifold: Manifold, limit: usize) {
    if buffer.len() == buffer.capacity() {
        let next = (buffer.capacity().max(1) * 2).min(limit);
        buffer.reserve_exact(next - buffer.len());
    }
    buffer.push(manifold);
}
/// Solver settings are validated and installed transactionally through the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SolverSettings {
    pub velocity_iterations: usize,
    pub position_iterations: usize,
    pub warm_start: bool,
    pub allow_sleep: bool,
    pub sleep_delay: f32,
    pub linear_sleep_threshold: f32,
    pub angular_sleep_threshold: f32,
    pub contact_match_distance: f32,
    pub contact_break_distance: f32,
    /// A full cache falls back to cold solving, never missing physical contacts.
    pub max_cached_manifolds: usize,
}
impl Default for SolverSettings {
    fn default() -> Self {
        Self {
            velocity_iterations: 12,
            position_iterations: 8,
            warm_start: true,
            allow_sleep: true,
            sleep_delay: 0.8,
            linear_sleep_threshold: 0.04,
            angular_sleep_threshold: 0.05,
            contact_match_distance: 0.05,
            contact_break_distance: 0.02,
            max_cached_manifolds: 2048,
        }
    }
}
impl SolverSettings {
    fn valid(self) -> bool {
        (1..=64).contains(&self.velocity_iterations)
            && self.position_iterations <= 32
            && self.sleep_delay.is_finite()
            && (0.05..=10.).contains(&self.sleep_delay)
            && self.linear_sleep_threshold.is_finite()
            && (0.001..=1.).contains(&self.linear_sleep_threshold)
            && self.angular_sleep_threshold.is_finite()
            && (0.001..=1.).contains(&self.angular_sleep_threshold)
            && self.contact_match_distance.is_finite()
            && (0.001..=0.2).contains(&self.contact_match_distance)
            && self.contact_break_distance.is_finite()
            && (0.001..=0.1).contains(&self.contact_break_distance)
            && self.max_cached_manifolds <= 65536
    }
}
/// Diagnostics for a persistent contact. Anchors are in each body's local space;
/// normal and tangent impulse are in world space at the last solved substep.
#[derive(Clone, Copy, Debug, Default)]
pub struct ContactPoint {
    pub id: u64,
    pub local_anchor1: Vec3,
    pub local_anchor2: Vec3,
    pub normal: Vec3,
    pub normal_impulse: f32,
    pub tangent_impulse: Vec3,
}
#[derive(Clone, Copy)]
pub(super) struct Manifold {
    pub key: (ColliderHandle, ColliderHandle),
    pub revisions: [u64; 4],
    pub dt: f32,
    pub points: [ContactPoint; MAX_POINTS],
    pub len: usize,
}
#[derive(Clone, Copy, Debug)]
pub struct ContactManifold<'a> {
    pub collider1: ColliderHandle,
    pub collider2: ColliderHandle,
    pub points: &'a [ContactPoint],
}
impl<B: Access> World<B> {
    /// Current observation configuration; `None` means disabled (the default).
    /// See [`Self::set_impact_settings`] for configuration and consumption.
    pub fn impact_settings(&self) -> Option<ImpactSettings> {
        self.impact_settings
    }
    /// Prepare bounded observation storage without changing physical state.
    /// Valid capacities are 0..=65536, impulse thresholds finite/nonnegative and
    /// approach thresholds finite/positive. Reservation is fallible and occurs
    /// here, never during recording: at most `capacity` records plus scratch for
    /// 4096 body motions. Allocators may round reservations upward.
    ///
    /// Changing settings or disabling clears records/drop counts. Reinstalling
    /// identical settings preserves them. Disabling and [`Self::clear_impacts`]
    /// retain memory; dropping the world releases it. No bodies are woken and no
    /// contact/joint caches are reset. New physical contacts may still allocate
    /// in existing solver scratch independently of recording preparation.
    ///
    /// Only approaching points accepted by the velocity solver are observed;
    /// sensors, separating/resting points and sweep-only hits with no constraint
    /// impulse are excluded. Approach uses saved pre-force point velocities at
    /// the old center of mass and the new contact position, then the pre-warm
    /// contact speed. This heuristic can omit an impact accelerated from rest
    /// entirely within one substep; it is not a second collision detector or a
    /// force/energy decomposition. Persistent solver jitter above the authored
    /// approach threshold can qualify. Choose thresholds for the scene's units.
    ///
    /// The local typed cue below is a pure feedback handoff. A game may map it to
    /// `ferravex_play::feedback::FeedbackEvent::hit` after stepping each executed
    /// tick, assign its own feedback ID and honor presentation preferences.
    /// This recipe performs no device/audio I/O and consumes each snapshot once.
    ///
    /// ```
    /// # #[cfg(any(feature = "dim2", feature = "dim3"))] {
    /// # #[cfg(feature = "dim2")]
    /// use ferravex_physics::d2::*;
    /// # #[cfg(all(not(feature = "dim2"), feature = "dim3"))]
    /// # use ferravex_physics::d3::*;
    /// use ferravex_math::Vec3;
    /// #[derive(Debug)]
    /// enum Cue { Hit { point: Vec3, intensity: f32 } }
    /// let mut world = PhysicsWorld::default();
    /// let mut options = world.impact_settings().unwrap_or_default();
    /// options.capacity = 8;
    /// options.min_approach_speed = 0.5;
    /// world.set_impact_settings(Some(options)).unwrap();
    /// world.insert(RigidBodyBuilder::fixed(), ColliderBuilder::ball(0.5));
    /// world.insert(
    ///     RigidBodyBuilder::dynamic().translation(-Vector::X * 1.1)
    ///         .linvel(Vector::X * 10.).ccd_enabled(true),
    ///     ColliderBuilder::ball(0.5),
    /// );
    /// world.step(1. / 60.); // Only after a simulation tick actually executes.
    /// let cues: Vec<_> = world.impacts().iter().map(|impact| Cue::Hit {
    ///     point: impact.point,
    ///     intensity: (impact.peak_normal_impulse / 10.).clamp(0., 4.),
    /// }).collect(); // Host-owned storage, independent of the bounded recorder.
    /// assert_eq!(cues.len(), 1);
    /// let Cue::Hit { point, intensity } = cues[0];
    /// assert!(point.is_finite() && intensity > 0.);
    /// assert_eq!(world.impact_dropped(), 0);
    /// world.clear_impacts(); // Scene restart/cancel, without changing physics.
    /// assert!(world.impacts().is_empty());
    /// world.set_impact_settings(None).unwrap();
    /// assert!(world.impact_settings().is_none());
    /// # }
    /// ```
    pub fn set_impact_settings(
        &mut self,
        settings: Option<ImpactSettings>,
    ) -> Result<(), ImpactRecordingError> {
        if let Some(settings) = settings {
            if settings.capacity > MAX_IMPACT_RECORDS
                || !settings.min_normal_impulse.is_finite()
                || settings.min_normal_impulse < 0.
                || !settings.min_approach_speed.is_finite()
                || settings.min_approach_speed <= 0.
            {
                return Err(ImpactRecordingError::InvalidSettings);
            }
            self.impacts
                .try_reserve_exact(settings.capacity.saturating_sub(self.impacts.len()))
                .map_err(|_| ImpactRecordingError::AllocationFailed)?;
            self.impact_motions
                .try_reserve_exact(4096_usize.saturating_sub(self.impact_motions.len()))
                .map_err(|_| ImpactRecordingError::AllocationFailed)?;
        }
        if self.impact_settings != settings {
            self.impact_settings = settings;
            self.clear_impacts();
            self.impact_motions.clear();
        }
        Ok(())
    }
    /// Canonically sorted, borrowed snapshot; at most one record per pair.
    /// Strictly stronger peaks replace point/normal/speed; ties keep the earliest
    /// substep, then the first contact feature in solver order. Removal/teleport
    /// and other edits preserve this historical snapshot with original handle
    /// generations. Check live handles before applying later gameplay to them.
    /// A valid step (including an empty world) replaces it; invalid dt/gravity
    /// panics preserve it. See [`Self::set_impact_settings`] for a runnable recipe.
    pub fn impacts(&self) -> &[ImpactRecord] {
        &self.impacts
    }
    /// Saturating count of qualifying pair/substep candidates rejected because
    /// the buffer was full. A retained pair can still update at full capacity.
    /// A rejected pair can count again in later substeps (at most eight per
    /// outer step); this is not a count of distinct lost pairs/gameplay hits.
    /// Multiple points/iterations of one pair in a substep count only once.
    /// See [`Self::set_impact_settings`] for configuration and reset semantics.
    pub fn impact_dropped(&self) -> usize {
        self.impact_dropped
    }
    /// Clear only the observation snapshot/drop count, retaining configuration
    /// and reserved memory. No simulation, wake or physical cache edits occur.
    /// See [`Self::set_impact_settings`] for a reset/cancel recipe.
    pub fn clear_impacts(&mut self) {
        self.impacts.clear();
        self.impact_dropped = 0;
    }
    pub(super) fn capture_impact_motion(&mut self) {
        self.impact_motions.clear();
        if self.impact_settings.is_some() {
            self.impact_motions
                .extend(self.bodies.iter().map(|(body, b)| {
                    let b = b.core();
                    ImpactMotion {
                        body,
                        center: b.world_com(),
                        velocity: b.velocity,
                        angular: b.angular_velocity,
                    }
                }));
        }
    }
    fn impact_approach(
        &self,
        a: RigidBodyHandle,
        b: RigidBodyHandle,
        point: Vec3,
        normal: Vec3,
    ) -> f32 {
        let velocity = |body| {
            // Arena iteration follows slot index; handle ordering starts with
            // that index, so holes and reused generations remain sorted.
            let i = self
                .impact_motions
                .binary_search_by_key(&body, |m| m.body)
                .expect("captured live body motion");
            let m = &self.impact_motions[i];
            m.velocity + m.angular.cross(point - m.center)
        };
        -(velocity(b) - velocity(a)).dot(normal)
    }
    pub(super) fn record_impacts(&mut self) {
        let Some(settings) = self.impact_settings else {
            return;
        };
        let mut first = 0;
        while first < self.constraints.len() {
            let key = self.constraints[first].key;
            let mut end = first + 1;
            let mut strongest = first;
            while end < self.constraints.len() && self.constraints[end].key == key {
                if self.constraints[end].impact_peak > self.constraints[strongest].impact_peak {
                    strongest = end;
                }
                end += 1;
            }
            let c = &self.constraints[strongest];
            if c.impact_peak.is_finite() && c.impact_peak > settings.min_normal_impulse {
                let record = ImpactRecord {
                    collider1: key.0,
                    collider2: key.1,
                    point: c.point,
                    normal: c.normal,
                    peak_normal_impulse: c.impact_peak,
                    approach_speed: c.impact_approach,
                };
                match self
                    .impacts
                    .binary_search_by_key(&key, |r| (r.collider1, r.collider2))
                {
                    Ok(i) => {
                        if record.peak_normal_impulse > self.impacts[i].peak_normal_impulse {
                            self.impacts[i] = record;
                        }
                    }
                    Err(i) if self.impacts.len() < settings.capacity => {
                        self.impacts.insert(i, record)
                    }
                    Err(_) => self.impact_dropped = self.impact_dropped.saturating_add(1),
                }
            }
            first = end;
        }
    }
    pub fn solver_settings(&self) -> SolverSettings {
        self.settings
    }
    /// Changing settings clears cached impulses and wakes the world before the next step.
    pub fn set_solver_settings(&mut self, settings: SolverSettings) -> Result<(), &'static str> {
        if !settings.valid() {
            return Err("invalid rigid solver settings");
        }
        if self.settings == settings {
            return Ok(());
        }
        self.settings = settings;
        for (_, j) in self.joints.iter_mut() {
            j.cache.fill(crate::joints::CachedRow::default());
            j.dt = 0.;
        }
        self.manifolds.clear();
        self.next_manifolds.clear();
        self.manifolds.shrink_to(settings.max_cached_manifolds);
        self.next_manifolds.shrink_to(settings.max_cached_manifolds);
        for (_, b) in self.bodies.iter_mut() {
            b.core_mut().wake();
        }
        Ok(())
    }
    /// Last-substep cache, including close separated points with zero impulse.
    /// Removal immediately discards affected entries; pose/material edits invalidate them on step.
    pub fn contact_manifolds(&self) -> impl Iterator<Item = ContactManifold<'_>> {
        self.manifolds.iter().map(|m| ContactManifold {
            collider1: m.key.0,
            collider2: m.key.1,
            points: &m.points[..m.len],
        })
    }
    pub(super) fn store_manifolds(&mut self, dt: f32) {
        for c in &self.constraints {
            if let Some((index, point)) = c.cache {
                let p = &mut self.next_manifolds[index].points[point];
                p.normal_impulse = c.normal_impulse;
                p.tangent_impulse = c.tangent_impulse;
            }
        }
        std::mem::swap(&mut self.manifolds, &mut self.next_manifolds);
        self.manifolds.sort_unstable_by_key(|m| m.key);
        self.next_manifolds.clear();
        for m in &mut self.manifolds {
            m.dt = dt;
        }
    }
}
pub(super) fn refreshed(
    m: &Manifold,
    a: &crate::body::Core,
    b: &crate::body::Core,
    breaking: f32,
) -> Manifold {
    let mut out = *m;
    out.len = 0;
    for point in &m.points[..m.len] {
        let pa = a.position + a.rotation * point.local_anchor1;
        let pb = b.position + b.rotation * point.local_anchor2;
        let delta = pb - pa;
        let separation = delta.dot(point.normal);
        let drift = delta - point.normal * separation;
        if separation <= breaking
            && separation >= -breaking * 2.
            && drift.length_squared() <= breaking * breaking
        {
            let mut p = *point;
            if separation > geometry::MARGIN {
                p.normal_impulse = 0.;
                p.tangent_impulse = Vec3::ZERO;
            }
            out.points[out.len] = p;
            out.len += 1;
        }
    }
    out
}
impl<B: Access> World<B> {
    pub(super) fn find_contacts(&mut self, dt: f32, stats: &mut StepStats) {
        self.proxies.clear();
        self.constraints.clear();
        self.next_manifolds.clear();
        let breaking = self.settings.contact_break_distance;
        for (collider, c) in self.colliders.iter() {
            let id = c.parent.unwrap();
            let b = self.bodies[id].core();
            let pose = c.pose(b);
            let bounds = geometry::bounds(c.shape, pose.position, pose.rotation);
            self.proxies.push(Proxy {
                body: id,
                collider,
                bounds: Bounds {
                    min: bounds.min - Vec3::splat(breaking),
                    max: bounds.max + Vec3::splat(breaking),
                },
            });
        }
        self.proxies.sort_unstable_by(|a, b| {
            a.bounds
                .min
                .x
                .total_cmp(&b.bounds.min.x)
                .then(a.collider.cmp(&b.collider))
        });
        for i in 0..self.proxies.len() {
            for j in i + 1..self.proxies.len() {
                let x = self.proxies[i];
                let y = self.proxies[j];
                if y.bounds.min.x > x.bounds.max.x {
                    break;
                }
                if x.body == y.body || self.joint_contacts_disabled(x.body, y.body) {
                    continue;
                }
                if !x.bounds.intersects(y.bounds) {
                    continue;
                }
                // Canonical collider ordering keeps normals, anchors and impulse signs stable.
                let (pa, pb) = if x.collider < y.collider {
                    (x, y)
                } else {
                    (y, x)
                };
                let ca = &self.colliders[pa.collider];
                let cb = &self.colliders[pb.collider];
                if ca.membership & cb.filter == 0 || cb.membership & ca.filter == 0 {
                    continue;
                }
                let a = self.bodies[pa.body].core();
                let b = self.bodies[pb.body].core();
                let sensor = ca.sensor || cb.sensor;
                if a.kind != Kind::Dynamic && b.kind != Kind::Dynamic && !sensor {
                    continue;
                }
                stats.broadphase_pairs += 1;
                let key = (pa.collider, pb.collider);
                let revisions = [a.revision, b.revision, ca.revision, cb.revision];
                let old = self
                    .manifolds
                    .binary_search_by_key(&key, |m| m.key)
                    .ok()
                    .map(|i| self.manifolds[i])
                    .filter(|m| m.revisions == revisions);
                self.points.clear();
                let immobile = |body: &crate::body::Core| {
                    body.kind == Kind::Fixed
                        || body.sleeping
                        || body.kind == Kind::Kinematic
                            && body.velocity.length_squared()
                                + body.angular_velocity.length_squared()
                                < 1e-12
                };
                if !sensor
                    && immobile(a)
                    && immobile(b)
                    && let Some(old) = old
                {
                    let cached = refreshed(&old, a, b, breaking);
                    for point in &cached.points[..cached.len] {
                        let ap = a.position + a.rotation * point.local_anchor1;
                        let bp = b.position + b.rotation * point.local_anchor2;
                        let separation = (bp - ap).dot(point.normal);
                        if separation <= geometry::MARGIN {
                            self.points.push(Point {
                                point: (ap + bp) * 0.5,
                                normal: point.normal,
                                depth: (-separation).max(0.),
                            });
                        }
                    }
                    stats.reused_manifolds += 1;
                } else {
                    geometry::contacts(
                        ca.shape,
                        &ca.pose(a),
                        cb.shape,
                        &cb.pose(b),
                        &mut self.points,
                    );
                }
                if self.points.is_empty() {
                    if !sensor && let Some(old) = old {
                        let mut cached = refreshed(&old, a, b, breaking);
                        for p in &mut cached.points[..cached.len] {
                            p.normal_impulse = 0.;
                            p.tangent_impulse = Vec3::ZERO;
                        }
                        if cached.len > 0 {
                            if self.next_manifolds.len() < self.settings.max_cached_manifolds {
                                push_manifold(
                                    &mut self.next_manifolds,
                                    cached,
                                    self.settings.max_cached_manifolds,
                                );
                            } else {
                                stats.cache_dropped += 1;
                            }
                        }
                    }
                    continue;
                }
                self.next_contacts.push(ContactPair {
                    collider1: key.0,
                    collider2: key.1,
                    normal: self.points[0].normal.to_array(),
                    sensor,
                });
                if sensor {
                    continue;
                }
                let material = ca.material.pair(cb.material);
                let friction = material.sliding_friction;
                let restitution = material.restitution;
                let cache_index = if self.next_manifolds.len() < self.settings.max_cached_manifolds
                {
                    Some(self.next_manifolds.len())
                } else {
                    stats.cache_dropped += 1;
                    None
                };
                let mut manifold = Manifold {
                    key,
                    revisions,
                    dt,
                    points: [ContactPoint::default(); MAX_POINTS],
                    len: self.points.len().min(MAX_POINTS),
                };
                let mut used = 0u16;
                let tolerance = self.settings.contact_match_distance.powi(2);
                for (index, p) in self.points.iter().enumerate() {
                    let local_a =
                        a.rotation.conjugate() * (p.point + p.normal * p.depth * 0.5 - a.position);
                    let local_b =
                        b.rotation.conjugate() * (p.point - p.normal * p.depth * 0.5 - b.position);
                    let speed =
                        (b.point_velocity(p.point) - a.point_velocity(p.point)).dot(p.normal);
                    let matched = old.and_then(|m| {
                        m.points[..m.len]
                            .iter()
                            .enumerate()
                            .filter(|(i, point)| {
                                used & (1 << i) == 0
                                    && point.normal.dot(p.normal) > 0.95
                                    && point.local_anchor1.distance_squared(local_a) < tolerance
                                    && point.local_anchor2.distance_squared(local_b) < tolerance
                            })
                            .min_by(|(_, x), (_, y)| {
                                let distance = |p: &ContactPoint| {
                                    p.local_anchor1.distance_squared(local_a)
                                        + p.local_anchor2.distance_squared(local_b)
                                };
                                distance(x).total_cmp(&distance(y)).then(x.id.cmp(&y.id))
                            })
                            .map(|(i, p)| (i, *p, m.dt))
                    });
                    let mut normal_impulse = 0.;
                    let mut tangent_impulse = Vec3::ZERO;
                    let feature = if let Some((index, previous, old_dt)) = matched {
                        used |= 1 << index;
                        stats.persisted_points += 1;
                        let ratio = dt / old_dt;
                        if self.settings.warm_start
                            && (0.5..=2.).contains(&ratio)
                            && (-1. ..=0.5).contains(&speed)
                        {
                            normal_impulse = (previous.normal_impulse * ratio).max(0.);
                            tangent_impulse = (previous.tangent_impulse
                                - p.normal * previous.tangent_impulse.dot(p.normal))
                                * ratio;
                            tangent_impulse = tangent_impulse
                                .clamp_length_max(material.static_friction * normal_impulse);
                        }
                        previous.id
                    } else {
                        let id = self.next_feature;
                        self.next_feature = self
                            .next_feature
                            .checked_add(1)
                            .expect("contact identity exhausted");
                        id
                    };
                    if index < MAX_POINTS {
                        manifold.points[index] = ContactPoint {
                            id: feature,
                            local_anchor1: local_a,
                            local_anchor2: local_b,
                            normal: p.normal,
                            normal_impulse,
                            tangent_impulse,
                        };
                    }
                    self.constraints.push(Constraint {
                        a: pa.body,
                        b: pb.body,
                        normal: p.normal,
                        point: p.point,
                        local_a,
                        local_b,
                        normal_impulse,
                        normal_denominator: 0.,
                        ra: Vec3::ZERO,
                        rb: Vec3::ZERO,
                        impact_baseline: normal_impulse,
                        impact_peak: 0.,
                        impact_approach: self.impact_settings.map_or(0., |settings| {
                            let approach = self
                                .impact_approach(pa.body, pb.body, p.point, p.normal)
                                .min(-speed);
                            if approach > settings.min_approach_speed {
                                approach
                            } else {
                                0.
                            }
                        }),
                        tangent_impulse,
                        bounce: if speed < -1. {
                            -speed * restitution
                        } else {
                            0.
                        },
                        friction,
                        static_friction: material.static_friction,
                        rolling_resistance: material.rolling_resistance,
                        rolling_impulse: Vec3::ZERO,
                        key,
                        feature,
                        cache: cache_index
                            .filter(|_| index < MAX_POINTS)
                            .map(|i| (i, index)),
                    });
                }
                if cache_index.is_some() {
                    push_manifold(
                        &mut self.next_manifolds,
                        manifold,
                        self.settings.max_cached_manifolds,
                    );
                }
            }
        }
        self.constraints
            .sort_unstable_by_key(|c| (c.key, c.feature));
        stats.contact_points += self.constraints.len();
    }
    pub(super) fn warm_start(&mut self, stats: &mut StepStats) {
        for c in &self.constraints {
            let (a, b) = self.bodies.two_mut(c.a, c.b);
            let a = a.core_mut();
            let b = b.core_mut();
            if a.contact_inv_mass() + b.contact_inv_mass() == 0. {
                continue;
            }
            let impulse = c.normal * c.normal_impulse + c.tangent_impulse;
            if impulse.length_squared() > 1e-16 {
                a.impulse(-impulse, c.point);
                b.impulse(impulse, c.point);
                stats.warm_started_points += 1;
            }
        }
    }
}

#[cfg(test)]
mod journey400_226 {
    use super::*;

    macro_rules! solver_observation_tests {
        ($dimension:ident) => {
            mod $dimension {
                use super::*;
                use crate::$dimension::{ColliderBuilder, PhysicsWorld, RigidBodyBuilder, Vector};

                // Synthetic baseline injection isolates the observation measure
                // from contact matching. Contacts and solves are production paths.
                #[test]
                fn peak_subtracts_initial_warm_start_and_does_not_sum_iterations() {
                    for (incoming, expected) in [(0., 0.), (2., 0.), (3., 1.)] {
                        for iterations in [1, 12, 64] {
                            let mut world = PhysicsWorld::default();
                            world.set_impact_settings(Some(ImpactSettings {
                                min_approach_speed: 0.01,
                                ..Default::default()
                            })).unwrap();
                            world.set_solver_settings(SolverSettings {
                                velocity_iterations: iterations,
                                ..Default::default()
                            }).unwrap();
                            world.insert(RigidBodyBuilder::fixed(), ColliderBuilder::ball(0.5).friction(0.));
                            let (body, _) = world.insert(
                                RigidBodyBuilder::dynamic().translation(-Vector::X * 0.9)
                                    .linvel(Vector::X * incoming),
                                ColliderBuilder::ball(0.5).friction(0.),
                            );
                            let mass = world.bodies[body].mass();
                            world.raw.capture_impact_motion();
                            world.raw.find_contacts(1. / 60., &mut StepStats::default());
                            assert_eq!(world.raw.constraints.len(), 1);
                            let c = &mut world.raw.constraints[0];
                            c.normal_impulse = mass * 2.;
                            c.impact_baseline = mass * 2.;
                            // Force eligibility for this measure-only test,
                            // including a replay with no physical approach.
                            c.impact_approach = 3.;
                            world.raw.warm_start(&mut StepStats::default());
                            world.raw.solve_velocities(&mut StepStats::default());
                            world.raw.record_impacts();
                            if expected == 0. {
                                assert!(world.impacts().is_empty());
                            } else {
                                assert_eq!(world.impacts().len(), 1);
                                assert!((world.impacts()[0].peak_normal_impulse - mass * expected).abs() < 1e-5);
                            }
                        }
                    }
                }

                #[test]
                fn stronger_later_substep_updates_at_capacity_ties_keep_first_and_drops_group_points() {
                    let mut world = PhysicsWorld::default();
                    world.set_impact_settings(Some(ImpactSettings { capacity: 1, ..Default::default() })).unwrap();
                    world.insert(RigidBodyBuilder::fixed(), ColliderBuilder::ball(0.5));
                    world.insert(RigidBodyBuilder::dynamic().translation(-Vector::X * 0.9), ColliderBuilder::ball(0.5));
                    world.raw.capture_impact_motion();
                    world.raw.find_contacts(1. / 60., &mut StepStats::default());
                    let mut later = world.raw.constraints[0].clone();
                    world.raw.constraints[0].impact_peak = 4.;
                    later.feature += 1;
                    later.point = Vec3::Y;
                    later.impact_peak = 4.;
                    world.raw.constraints.push(later);
                    world.raw.record_impacts();
                    let first = world.impacts()[0];
                    assert_eq!(first.point, world.raw.constraints[0].point);
                    world.raw.constraints[1].impact_peak = 8.;
                    world.raw.record_impacts();
                    assert_eq!(world.impacts().len(), 1);
                    assert_eq!(world.impacts()[0].point, Vec3::Y);
                    assert_eq!(world.impacts()[0].peak_normal_impulse, 8.);
                    assert_eq!(world.impact_dropped(), 0);
                    world.raw.constraints[0].impact_peak = 8.;
                    world.raw.record_impacts();
                    assert_eq!(world.impacts()[0].point, Vec3::Y);
                    world.raw.impact_settings.as_mut().unwrap().capacity = 0;
                    world.clear_impacts();
                    world.raw.record_impacts();
                    assert_eq!(world.impact_dropped(), 1);
                    world.raw.impact_dropped = usize::MAX;
                    world.raw.record_impacts();
                    assert_eq!(world.impact_dropped(), usize::MAX);
                }

                #[test]
                fn default_off_and_reset_retain_prepared_storage_only_when_enabled() {
                    let mut world = PhysicsWorld::default();
                    world.step(1. / 60.);
                    assert_eq!(world.raw.impacts.capacity(), 0);
                    assert_eq!(world.raw.impact_motions.capacity(), 0);
                    world.set_impact_settings(Some(ImpactSettings::default())).unwrap();
                    let capacities = (world.raw.impacts.capacity(), world.raw.impact_motions.capacity());
                    assert!(capacities.0 >= 64 && capacities.1 >= 4096);
                    world.set_impact_settings(None).unwrap();
                    world.clear_impacts();
                    world.step(1. / 60.);
                    assert_eq!((world.raw.impacts.capacity(), world.raw.impact_motions.capacity()), capacities);
                    let err = ImpactRecordingError::InvalidSettings;
                    let error: &dyn std::error::Error = &err;
                    assert!(error.to_string().contains("invalid"));
                }
            }
        };
    }
    #[cfg(feature = "dim2")]
    solver_observation_tests!(d2);
    #[cfg(feature = "dim3")]
    solver_observation_tests!(d3);
}
